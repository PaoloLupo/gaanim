//! glTF 2.0 models (`.gltf`, `.glb`), read when a script imports them.
//!
//! A model becomes plain scene content: a node per glTF node, with its rest
//! transform, holding a [`TriangleMeshData`] per mesh primitive with its
//! [`Material3D`], base-color texture, alpha mode and skin weights
//! ([`MeshSurface`]). Skins list the nodes that move their vertices, and
//! animations are sampled channels of node translations, rotations and
//! scales ([`GltfAnimations`]), evaluated at any time without state, so
//! seeks are exact. Recordings keep what this resolves, so players never
//! read glTF.
//!
//! Read: triangles (lists, strips and fans), positions, normals, the UVs of
//! the base-color texture, `COLOR_0`, `JOINTS_0` and `WEIGHTS_0`; the
//! metallic-roughness factors, emission (with `KHR_materials_emissive_strength`),
//! `KHR_materials_unlit`, alpha modes and `doubleSided`; and `LINEAR`, `STEP`
//! and `CUBICSPLINE` channels. Left out, for now: morph targets, normal,
//! occlusion and metallic-roughness maps, texture transforms, and the file's
//! cameras and lights.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use gaanim_core::glam::{DMat4, DQuat, DVec3};
use gaanim_core::peniko::{self, Blob, Color, ImageAlphaType, ImageData, ImageFormat};
use gaanim_math::Bounds3D;
use gaanim_scene::{
    AlphaMode, Material3D, MeshSurface, MeshTexture, SkinWeights, TextureWrap, TriangleMeshData,
};

/// Longest side of a texture once imported: larger images are scaled down,
/// which keeps recordings and GPU memory in proportion to what a frame shows.
pub const MAX_TEXTURE_SIZE: u32 = 2048;

/// Which of a file's scenes to import.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum GltfSceneSelector {
    /// The file's default scene, or its first.
    #[default]
    Default,
    Index(usize),
    Name(String),
}

/// A node's translation, rotation and scale, in its parent's space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodePose {
    pub translation: DVec3,
    pub rotation: DQuat,
    pub scale: DVec3,
}

impl Default for NodePose {
    fn default() -> Self {
        Self {
            translation: DVec3::ZERO,
            rotation: DQuat::IDENTITY,
            scale: DVec3::ONE,
        }
    }
}

impl NodePose {
    pub fn to_mat4(&self) -> DMat4 {
        DMat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    /// `t` of the way from this pose to `other`, the rotation by the
    /// shorter arc.
    pub fn lerp(&self, other: &Self, t: f64) -> Self {
        let to = if self.rotation.dot(other.rotation) < 0.0 {
            -other.rotation
        } else {
            other.rotation
        };
        Self {
            translation: self.translation.lerp(other.translation, t),
            rotation: self.rotation.slerp(to, t).normalize(),
            scale: self.scale.lerp(other.scale, t),
        }
    }
}

/// A mesh primitive of a node.
#[derive(Debug, Clone)]
pub struct GltfPrimitive {
    pub mesh: TriangleMeshData,
    /// Name of its material, when it has one.
    pub material: Option<String>,
}

/// A node of the imported scene.
#[derive(Debug, Clone)]
pub struct GltfNode {
    /// The node's glTF index.
    pub index: usize,
    /// What selects it: its name, or `node<index>` without one, with
    /// `#<index>` appended when another node has the same name.
    pub name: String,
    /// Its position among [`GltfModel::nodes`] of its parent, `None` at the
    /// scene's root.
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub rest: NodePose,
    pub primitives: Vec<GltfPrimitive>,
    /// The skin of its primitives, among [`GltfModel::skins`].
    pub skin: Option<usize>,
}

/// The joints of a skin, as positions among [`GltfModel::nodes`], and the
/// inverse of each joint's world matrix in the pose the mesh was bound in.
#[derive(Debug, Clone)]
pub struct GltfSkin {
    pub joints: Vec<usize>,
    pub inverse_bind: Vec<DMat4>,
}

/// How a channel reads between its keyframes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelInterpolation {
    Step,
    Linear,
    /// Hermite splines, with an in- and an out-tangent around each value.
    CubicSpline,
}

/// What a channel moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelTarget {
    Translation,
    Rotation,
    Scale,
}

/// The keyframes of one property of one node.
#[derive(Debug, Clone)]
pub struct GltfChannel {
    /// The node it moves, among [`GltfModel::nodes`].
    pub node: usize,
    pub target: ChannelTarget,
    pub interpolation: ChannelInterpolation,
    pub times: Vec<f64>,
    /// A value per time, `[x, y, z, w]` (`w` unused but for rotations); for
    /// cubic splines, three per time: in-tangent, value, out-tangent.
    pub values: Vec<[f64; 4]>,
}

/// An animation of the file, one of Blender's Actions.
#[derive(Debug, Clone)]
pub struct GltfClip {
    pub name: String,
    /// From time 0 to its last keyframe, in seconds.
    pub duration: f64,
    pub channels: Vec<GltfChannel>,
}

/// The rest pose of the nodes and the animations that move them.
#[derive(Debug, Clone, Default)]
pub struct GltfAnimations {
    pub rest: Vec<NodePose>,
    pub clips: Vec<GltfClip>,
}

impl GltfAnimations {
    /// The clip named `name`.
    pub fn clip(&self, name: &str) -> Option<usize> {
        self.clips.iter().position(|clip| clip.name == name)
    }

    /// The nodes clip `clip` moves, sorted.
    pub fn animated_nodes(&self, clip: usize) -> Vec<usize> {
        let mut nodes: Vec<usize> = self.clips[clip]
            .channels
            .iter()
            .map(|channel| channel.node)
            .collect();
        nodes.sort_unstable();
        nodes.dedup();
        nodes
    }

    /// Write into `pose`, which holds a pose per node, what clip `clip`
    /// shows at `time` seconds into it, clamped to its keyframes; nodes it
    /// does not move keep what `pose` holds.
    pub fn sample(&self, clip: usize, time: f64, pose: &mut [NodePose]) {
        for channel in &self.clips[clip].channels {
            let Some(node) = pose.get_mut(channel.node) else {
                continue;
            };
            let value = channel.sample(time);
            match channel.target {
                ChannelTarget::Translation => {
                    node.translation = DVec3::new(value[0], value[1], value[2]);
                }
                ChannelTarget::Scale => node.scale = DVec3::new(value[0], value[1], value[2]),
                ChannelTarget::Rotation => {
                    let rotation = DQuat::from_xyzw(value[0], value[1], value[2], value[3]);
                    if rotation.length_squared() > 1e-12 && rotation.is_finite() {
                        node.rotation = rotation.normalize();
                    }
                }
            }
        }
    }
}

impl GltfChannel {
    /// The channel's value at `time`, held at its first and last keyframes.
    pub fn sample(&self, time: f64) -> [f64; 4] {
        let count = self.times.len();
        let value = |key: usize| match self.interpolation {
            ChannelInterpolation::CubicSpline => self.values[key * 3 + 1],
            _ => self.values[key],
        };
        if count == 0 {
            return [0.0, 0.0, 0.0, 1.0];
        }
        if count == 1 || !(time > self.times[0]) {
            return value(0);
        }
        if time >= self.times[count - 1] {
            return value(count - 1);
        }
        let next = self.times.partition_point(|key| *key <= time);
        let previous = next - 1;
        let (t0, t1) = (self.times[previous], self.times[next]);
        let span = t1 - t0;
        if !(span > 0.0) {
            return value(next);
        }
        let s = (time - t0) / span;
        match self.interpolation {
            ChannelInterpolation::Step => value(previous),
            ChannelInterpolation::Linear => {
                let (a, b) = (value(previous), value(next));
                if self.target == ChannelTarget::Rotation {
                    let a = DQuat::from_array(a);
                    let mut b = DQuat::from_array(b);
                    if a.dot(b) < 0.0 {
                        b = -b;
                    }
                    a.slerp(b, s).to_array()
                } else {
                    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * s)
                }
            }
            ChannelInterpolation::CubicSpline => {
                let p0 = self.values[previous * 3 + 1];
                let m0 = self.values[previous * 3 + 2];
                let m1 = self.values[next * 3];
                let p1 = self.values[next * 3 + 1];
                let (s2, s3) = (s * s, s * s * s);
                let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
                let h10 = s3 - 2.0 * s2 + s;
                let h01 = -2.0 * s3 + 3.0 * s2;
                let h11 = s3 - s2;
                let mut out: [f64; 4] = std::array::from_fn(|i| {
                    h00 * p0[i] + h10 * span * m0[i] + h01 * p1[i] + h11 * span * m1[i]
                });
                if self.target == ChannelTarget::Rotation {
                    let length = out.iter().map(|v| v * v).sum::<f64>().sqrt();
                    if length > 1e-12 {
                        out = out.map(|v| v / length);
                    }
                }
                out
            }
        }
    }
}

/// A glTF scene, ready to become scene content.
#[derive(Debug, Clone)]
pub struct GltfModel {
    pub path: PathBuf,
    /// The nodes of the scene, parents before their children.
    pub nodes: Vec<GltfNode>,
    /// The scene's root nodes, among [`Self::nodes`].
    pub roots: Vec<usize>,
    pub skins: Vec<GltfSkin>,
    pub animations: Arc<GltfAnimations>,
    /// The scene's box in its own coordinates, in its rest pose.
    pub bounds: Bounds3D,
}

impl GltfModel {
    /// Names of the animations, in the file's order, and their durations.
    pub fn clips(&self) -> Vec<(String, f64)> {
        self.animations
            .clips
            .iter()
            .map(|clip| (clip.name.clone(), clip.duration))
            .collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GltfLoadError {
    #[error("glTF model '{path}' does not exist")]
    Missing { path: PathBuf },
    #[error("unsupported 3D model extension for '{path}'; expected .gltf or .glb")]
    UnsupportedExtension { path: PathBuf },
    #[error("could not read glTF model '{path}': {source}")]
    Import {
        path: PathBuf,
        #[source]
        source: gltf::Error,
    },
    #[error("could not read image {image} of glTF model '{path}': {reason}")]
    Image {
        path: PathBuf,
        image: usize,
        reason: String,
    },
    #[error("glTF model '{path}' has no scenes")]
    NoScenes { path: PathBuf },
    #[error("glTF scene index {index} does not exist in '{path}'; it has {count}")]
    UnknownSceneIndex {
        path: PathBuf,
        index: usize,
        count: usize,
    },
    #[error("glTF scene '{name}' does not exist in '{path}'; available scenes: {available}")]
    UnknownSceneName {
        path: PathBuf,
        name: String,
        available: String,
    },
}

type Cache = HashMap<(PathBuf, GltfSceneSelector), (std::time::SystemTime, Arc<GltfModel>)>;

impl GltfModel {
    /// Read scene `selector` of the model at `path`. Models are kept by
    /// path and modification time, so that a script reloaded after an
    /// unrelated change does not read them again.
    pub fn load(
        path: impl AsRef<Path>,
        selector: &GltfSceneSelector,
    ) -> Result<Arc<Self>, GltfLoadError> {
        let path = path.as_ref().to_path_buf();
        if !path.is_file() {
            return Err(GltfLoadError::Missing { path });
        }
        let supported = path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "gltf" | "glb"));
        if !supported {
            return Err(GltfLoadError::UnsupportedExtension { path });
        }
        let modified = std::fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let key = (path.clone(), selector.clone());
        if let Some((when, model)) = cache.lock().expect("glTF cache poisoned").get(&key)
            && *when == modified
        {
            return Ok(Arc::clone(model));
        }
        let bytes = std::fs::read(&path).map_err(|error| GltfLoadError::Import {
            path: path.clone(),
            source: gltf::Error::Io(error),
        })?;
        let model = Arc::new(Self::from_slice(&bytes, &path, selector)?);
        let mut cache = cache.lock().expect("glTF cache poisoned");
        // A model per path and scene: older versions of a file go.
        cache.retain(|(held, _), _| held != &path);
        cache.insert(key, (modified, Arc::clone(&model)));
        Ok(model)
    }

    /// Read scene `selector` of a model held in `bytes`, whose external
    /// buffers and images sit next to `path`.
    pub fn from_slice(
        bytes: &[u8],
        path: &Path,
        selector: &GltfSceneSelector,
    ) -> Result<Self, GltfLoadError> {
        let import = |source| GltfLoadError::Import {
            path: path.to_path_buf(),
            source,
        };
        let gltf::Gltf { document, blob } = gltf::Gltf::from_slice(bytes).map_err(import)?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        let buffers = gltf::import_buffers(&document, Some(base), blob).map_err(import)?;
        let scenes: Vec<gltf::Scene> = document.scenes().collect();
        if scenes.is_empty() {
            return Err(GltfLoadError::NoScenes {
                path: path.to_path_buf(),
            });
        }
        let scene = match selector {
            GltfSceneSelector::Default => document
                .default_scene()
                .unwrap_or_else(|| scenes[0].clone()),
            GltfSceneSelector::Index(index) => {
                scenes
                    .get(*index)
                    .cloned()
                    .ok_or_else(|| GltfLoadError::UnknownSceneIndex {
                        path: path.to_path_buf(),
                        index: *index,
                        count: scenes.len(),
                    })?
            }
            GltfSceneSelector::Name(name) => scenes
                .iter()
                .find(|scene| scene.name() == Some(name.as_str()))
                .cloned()
                .ok_or_else(|| GltfLoadError::UnknownSceneName {
                    path: path.to_path_buf(),
                    name: name.clone(),
                    available: scenes
                        .iter()
                        .enumerate()
                        .map(|(index, scene)| {
                            scene
                                .name()
                                .map_or_else(|| format!("#{index}"), str::to_owned)
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                })?,
        };
        let mut reader = Reader {
            path,
            base,
            buffers: &buffers,
            images: HashMap::new(),
        };

        // The scene's nodes, parents first: a node's position here, by its
        // glTF index.
        let mut nodes: Vec<GltfNode> = Vec::new();
        let mut position: HashMap<usize, usize> = HashMap::new();
        let mut pending: Vec<(gltf::Node, Option<usize>)> =
            scene.nodes().map(|node| (node, None)).collect();
        pending.reverse();
        while let Some((node, parent)) = pending.pop() {
            // A node reached twice (a DAG the spec forbids) is kept once.
            if position.contains_key(&node.index()) {
                continue;
            }
            let at = nodes.len();
            position.insert(node.index(), at);
            if let Some(parent) = parent {
                nodes[parent].children.push(at);
            }
            let (translation, rotation, scale) = node.transform().decomposed();
            let rest = NodePose {
                translation: DVec3::from_array(translation.map(f64::from)),
                rotation: {
                    let rotation = DQuat::from_array(rotation.map(f64::from));
                    if rotation.length_squared() > 1e-12 {
                        rotation.normalize()
                    } else {
                        DQuat::IDENTITY
                    }
                },
                scale: DVec3::from_array(scale.map(f64::from)),
            };
            let primitives = match node.mesh() {
                Some(mesh) => reader.mesh(&mesh)?,
                None => Vec::new(),
            };
            nodes.push(GltfNode {
                index: node.index(),
                name: node
                    .name()
                    .filter(|name| !name.is_empty())
                    .map_or_else(|| format!("node{}", node.index()), str::to_owned),
                parent,
                children: Vec::new(),
                rest,
                primitives,
                skin: node.skin().map(|skin| skin.index()),
            });
            let mut children: Vec<_> = node.children().map(|child| (child, Some(at))).collect();
            children.reverse();
            pending.extend(children);
        }
        // Names select nodes: a repeated one takes its index.
        let mut counts: HashMap<String, usize> = HashMap::new();
        for node in &nodes {
            *counts.entry(node.name.clone()).or_default() += 1;
        }
        for node in &mut nodes {
            if counts[&node.name] > 1 {
                node.name = format!("{}#{}", node.name, node.index);
            }
        }

        // Skins of the scene's nodes, renumbered to their order here.
        let mut skin_position: HashMap<usize, usize> = HashMap::new();
        let mut skins = Vec::new();
        for node in &mut nodes {
            let Some(index) = node.skin else { continue };
            let at = match skin_position.get(&index) {
                Some(at) => *at,
                None => {
                    let skin = document
                        .skins()
                        .nth(index)
                        .expect("a node's skin is in the document");
                    let joints: Vec<usize> = skin
                        .joints()
                        .map(|joint| position.get(&joint.index()).copied())
                        .collect::<Option<Vec<_>>>()
                        .unwrap_or_default();
                    let count = joints.len();
                    if count > gaanim_scene::model::MAX_JOINTS {
                        tracing::warn!(
                            "glTF model '{}': skin {index} has {count} joints; only the first {} \
                             move its vertices",
                            path.display(),
                            gaanim_scene::model::MAX_JOINTS
                        );
                    }
                    let inverse_bind: Vec<DMat4> = skin
                        .reader(buffer_of(reader.buffers))
                        .read_inverse_bind_matrices()
                        .map(|matrices| {
                            matrices
                                .map(|matrix| {
                                    DMat4::from_cols_array_2d(
                                        &matrix.map(|column| column.map(f64::from)),
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_else(|| vec![DMat4::IDENTITY; count]);
                    let at = skins.len();
                    skins.push(GltfSkin {
                        inverse_bind: (0..count)
                            .map(|joint| {
                                inverse_bind.get(joint).copied().unwrap_or(DMat4::IDENTITY)
                            })
                            .collect(),
                        joints,
                    });
                    skin_position.insert(index, at);
                    at
                }
            };
            // A skin whose joints lie outside the scene skins nothing.
            node.skin = (!skins[at].joints.is_empty()).then_some(at);
        }
        for node in &mut nodes {
            if node.skin.is_none() {
                for primitive in &mut node.primitives {
                    if let Some(surface) = &mut primitive.mesh.surface
                        && surface.skin.is_some()
                    {
                        Arc::make_mut(surface).skin = None;
                    }
                }
            }
        }

        let rest: Vec<NodePose> = nodes.iter().map(|node| node.rest).collect();
        let clips = document
            .animations()
            .enumerate()
            .map(|(index, animation)| reader.clip(&animation, index, &position))
            .collect();
        let bounds = rest_bounds(&nodes, &skins);
        Ok(Self {
            path: path.to_path_buf(),
            roots: (0..nodes.len())
                .filter(|node| nodes[*node].parent.is_none())
                .collect(),
            nodes,
            skins,
            animations: Arc::new(GltfAnimations { rest, clips }),
            bounds,
        })
    }
}

/// The box of the scene in its rest pose: each primitive's vertices placed
/// by its node, or by its skin's bind pose.
fn rest_bounds(nodes: &[GltfNode], skins: &[GltfSkin]) -> Bounds3D {
    let mut world = vec![DMat4::IDENTITY; nodes.len()];
    for (index, node) in nodes.iter().enumerate() {
        let parent = node.parent.map_or(DMat4::IDENTITY, |parent| world[parent]);
        world[index] = parent * node.rest.to_mat4();
    }
    let (mut low, mut high) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for (index, node) in nodes.iter().enumerate() {
        for primitive in &node.primitives {
            let skin = node.skin.map(|skin| &skins[skin]);
            let weights = primitive
                .mesh
                .surface
                .as_ref()
                .and_then(|surface| surface.skin.as_ref());
            for (vertex, position) in primitive.mesh.vertices.iter().enumerate() {
                let local = DVec3::from_array(position.map(f64::from));
                let point = match (skin, weights) {
                    (Some(skin), Some(weights)) => {
                        let joint = weights.joints[vertex][0] as usize;
                        match (skin.joints.get(joint), skin.inverse_bind.get(joint)) {
                            (Some(node), Some(bind)) => {
                                (world[*node] * *bind).transform_point3(local)
                            }
                            _ => world[index].transform_point3(local),
                        }
                    }
                    _ => world[index].transform_point3(local),
                };
                if point.is_finite() {
                    low = low.min(point);
                    high = high.max(point);
                }
            }
        }
    }
    if low.x > high.x {
        return Bounds3D::default();
    }
    Bounds3D::new(low, high)
}

/// Reads meshes, textures and animations from a document's buffers.
struct Reader<'a> {
    path: &'a Path,
    base: &'a Path,
    buffers: &'a [gltf::buffer::Data],
    /// Decoded images by glTF image index: textures that show one image
    /// share its pixels.
    images: HashMap<usize, Option<ImageData>>,
}

/// Reads the bytes of a document's buffer from `buffers`.
fn buffer_of<'a>(
    buffers: &'a [gltf::buffer::Data],
) -> impl Fn(gltf::Buffer<'_>) -> Option<&'a [u8]> + Clone + 'a {
    move |buffer| buffers.get(buffer.index()).map(|data| &data.0[..])
}

impl Reader<'_> {
    /// The triangle primitives of `mesh`, each with its material.
    fn mesh(&mut self, mesh: &gltf::Mesh<'_>) -> Result<Vec<GltfPrimitive>, GltfLoadError> {
        let mut primitives = Vec::new();
        for primitive in mesh.primitives() {
            let mode = primitive.mode();
            if !matches!(
                mode,
                gltf::mesh::Mode::Triangles
                    | gltf::mesh::Mode::TriangleStrip
                    | gltf::mesh::Mode::TriangleFan
            ) {
                tracing::warn!(
                    "glTF model '{}': skipped a primitive of mesh '{}' drawn as {mode:?}; \
                     only triangles are imported",
                    self.path.display(),
                    mesh.name().unwrap_or_default()
                );
                continue;
            }
            let read = primitive.reader(buffer_of(self.buffers));
            let Some(positions) = read.read_positions() else {
                continue;
            };
            let vertices: Vec<[f32; 3]> = positions.collect();
            let count = vertices.len();
            let corners: Vec<u32> = match read.read_indices() {
                Some(indices) => indices.into_u32().collect(),
                None => (0..count as u32).collect(),
            };
            let indices = triangles(mode, &corners);
            let normals = read
                .read_normals()
                .map(|normals| normals.collect::<Vec<_>>())
                .filter(|normals| normals.len() == count);
            let colors = read
                .read_colors(0)
                .map(|colors| colors.into_rgba_f32().collect::<Vec<_>>())
                .filter(|colors| colors.len() == count);

            let material = primitive.material();
            let pbr = material.pbr_metallic_roughness();
            let texture_info = pbr.base_color_texture();
            let uvs = texture_info
                .as_ref()
                .and_then(|info| read.read_tex_coords(info.tex_coord()))
                .map(|uvs| uvs.into_f32().collect::<Vec<_>>())
                .filter(|uvs| uvs.len() == count);
            let texture = match (&texture_info, &uvs) {
                (Some(info), Some(_)) => self.texture(&info.texture())?,
                _ => None,
            };
            let skin = match (read.read_joints(0), read.read_weights(0)) {
                (Some(joints), Some(weights)) => {
                    let joints: Vec<[u16; 4]> = joints.into_u16().collect();
                    let weights: Vec<[f32; 4]> = weights
                        .into_f32()
                        .map(|weight| {
                            let sum: f32 = weight.iter().sum();
                            if sum > 1e-6 {
                                weight.map(|w| w / sum)
                            } else {
                                [0.0; 4]
                            }
                        })
                        .collect();
                    (joints.len() == count && weights.len() == count)
                        .then_some(SkinWeights { joints, weights })
                }
                _ => None,
            };

            let alpha = match material.alpha_mode() {
                gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
                gltf::material::AlphaMode::Mask => {
                    AlphaMode::Mask(material.alpha_cutoff().unwrap_or(0.5))
                }
                gltf::material::AlphaMode::Blend => AlphaMode::Blend,
            };
            let [r, g, b, a] = pbr.base_color_factor();
            let lit = !material.unlit();
            let emissive = material.emissive_factor();
            let strength = material.emissive_strength().unwrap_or(1.0);
            let glows = emissive.iter().any(|value| *value > 0.0);
            let surface = Material3D {
                color: srgb_color([r, g, b], a),
                roughness: pbr.roughness_factor().clamp(0.0, 1.0),
                metallic: pbr.metallic_factor().clamp(0.0, 1.0),
                emissive: if glows {
                    srgb_color(emissive, 1.0)
                } else {
                    Color::TRANSPARENT
                },
                emissive_strength: if glows { strength.max(0.0) } else { 0.0 },
                ..Material3D::default()
            };
            primitives.push(GltfPrimitive {
                mesh: TriangleMeshData {
                    vertices,
                    indices,
                    normals,
                    uvs,
                    color: Some(surface.color),
                    colors,
                    material: lit.then_some(surface),
                    surface: Some(Arc::new(MeshSurface {
                        texture,
                        alpha,
                        double_sided: material.double_sided(),
                        skin,
                    })),
                    blend: None,
                },
                material: material.name().map(str::to_owned),
            });
        }
        Ok(primitives)
    }

    /// The base-color texture `texture`, its image decoded once; `None`
    /// when the image is in a format this reader does not decode.
    fn texture(
        &mut self,
        texture: &gltf::Texture<'_>,
    ) -> Result<Option<MeshTexture>, GltfLoadError> {
        let Some(image) = self.image(&texture.source())? else {
            return Ok(None);
        };
        let sampler = texture.sampler();
        let wrap = |mode: gltf::texture::WrappingMode| match mode {
            gltf::texture::WrappingMode::Repeat => TextureWrap::Repeat,
            gltf::texture::WrappingMode::MirroredRepeat => TextureWrap::MirroredRepeat,
            gltf::texture::WrappingMode::ClampToEdge => TextureWrap::ClampToEdge,
        };
        Ok(Some(MeshTexture {
            image,
            wrap: [wrap(sampler.wrap_s()), wrap(sampler.wrap_t())],
            nearest: sampler.mag_filter() == Some(gltf::texture::MagFilter::Nearest),
        }))
    }

    /// The pixels of `image`, decoded once and scaled down to
    /// [`MAX_TEXTURE_SIZE`].
    fn image(&mut self, image: &gltf::Image<'_>) -> Result<Option<ImageData>, GltfLoadError> {
        if let Some(held) = self.images.get(&image.index()) {
            return Ok(held.clone());
        }
        let encoded = match image.source() {
            gltf::image::Source::View { view, .. } => {
                let data = &self.buffers[view.buffer().index()].0;
                data.get(view.offset()..view.offset() + view.length())
                    .map(<[u8]>::to_vec)
            }
            gltf::image::Source::Uri { uri, .. } => self.uri(uri, image.index())?,
        };
        let encoded = encoded.ok_or_else(|| GltfLoadError::Image {
            path: self.path.to_path_buf(),
            image: image.index(),
            reason: "its bytes lie outside its buffer".into(),
        })?;
        let decoded = match image::load_from_memory(&encoded) {
            Ok(decoded) => decoded,
            Err(reason) => {
                tracing::warn!(
                    "glTF model '{}': image {} left out: {reason}",
                    self.path.display(),
                    image.index()
                );
                self.images.insert(image.index(), None);
                return Ok(None);
            }
        };
        let mut rgba = decoded.to_rgba8();
        let (width, height) = rgba.dimensions();
        if width.max(height) > MAX_TEXTURE_SIZE {
            let scale = f64::from(MAX_TEXTURE_SIZE) / f64::from(width.max(height));
            let size = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
            rgba = image::imageops::resize(
                &rgba,
                size(width),
                size(height),
                image::imageops::FilterType::Triangle,
            );
        }
        let (width, height) = rgba.dimensions();
        let held = Some(ImageData {
            data: Blob::new(Arc::new(rgba.into_raw())),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width,
            height,
        });
        self.images.insert(image.index(), held.clone());
        Ok(held)
    }

    /// The bytes of an image at `uri`: a data URI, or a file beside the model.
    fn uri(&self, uri: &str, image: usize) -> Result<Option<Vec<u8>>, GltfLoadError> {
        use base64::Engine as _;
        let error = |reason: String| GltfLoadError::Image {
            path: self.path.to_path_buf(),
            image,
            reason,
        };
        if let Some(data) = uri.strip_prefix("data:") {
            let Some((_, encoded)) = data.split_once(";base64,") else {
                return Err(error("a data URI that is not base64".into()));
            };
            return base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map(Some)
                .map_err(|reason| error(reason.to_string()));
        }
        if uri.contains("://") {
            return Err(error(format!("'{uri}' is not a file beside the model")));
        }
        let file = self.base.join(percent_decode(uri));
        std::fs::read(&file)
            .map(Some)
            .map_err(|reason| error(format!("'{}': {reason}", file.display())))
    }

    /// Animation `animation` with its channels on the scene's nodes;
    /// channels on other nodes and on morph target weights are left out.
    fn clip(
        &self,
        animation: &gltf::Animation<'_>,
        index: usize,
        position: &HashMap<usize, usize>,
    ) -> GltfClip {
        let mut channels = Vec::new();
        for channel in animation.channels() {
            let Some(&node) = position.get(&channel.target().node().index()) else {
                continue;
            };
            let target = match channel.target().property() {
                gltf::animation::Property::Translation => ChannelTarget::Translation,
                gltf::animation::Property::Rotation => ChannelTarget::Rotation,
                gltf::animation::Property::Scale => ChannelTarget::Scale,
                gltf::animation::Property::MorphTargetWeights => continue,
            };
            let interpolation = match channel.sampler().interpolation() {
                gltf::animation::Interpolation::Step => ChannelInterpolation::Step,
                gltf::animation::Interpolation::Linear => ChannelInterpolation::Linear,
                gltf::animation::Interpolation::CubicSpline => ChannelInterpolation::CubicSpline,
            };
            let read = channel.reader(buffer_of(self.buffers));
            let Some(times) = read.read_inputs() else {
                continue;
            };
            let times: Vec<f64> = times.map(f64::from).collect();
            let Some(outputs) = read.read_outputs() else {
                continue;
            };
            use gltf::animation::util::ReadOutputs;
            let values: Vec<[f64; 4]> = match outputs {
                ReadOutputs::Translations(values) | ReadOutputs::Scales(values) => values
                    .map(|[x, y, z]| [f64::from(x), f64::from(y), f64::from(z), 0.0])
                    .collect(),
                ReadOutputs::Rotations(values) => values
                    .into_f32()
                    .map(|value| value.map(f64::from))
                    .collect(),
                ReadOutputs::MorphTargetWeights(_) => continue,
            };
            let per_key = if interpolation == ChannelInterpolation::CubicSpline {
                3
            } else {
                1
            };
            let well_formed = !times.is_empty()
                && values.len() == times.len() * per_key
                && times.iter().all(|time| time.is_finite())
                && times.windows(2).all(|pair| pair[0] <= pair[1])
                && values.iter().flatten().all(|value| value.is_finite());
            if !well_formed {
                tracing::warn!(
                    "glTF model '{}': skipped a malformed channel of animation {index}",
                    self.path.display()
                );
                continue;
            }
            channels.push(GltfChannel {
                node,
                target,
                interpolation,
                times,
                values,
            });
        }
        let duration = channels
            .iter()
            .filter_map(|channel| channel.times.last().copied())
            .fold(0.0, f64::max);
        GltfClip {
            name: animation
                .name()
                .filter(|name| !name.is_empty())
                .map_or_else(|| format!("animation{index}"), str::to_owned),
            duration,
            channels,
        }
    }
}

/// The triangles of a primitive drawn in `mode` through `corners`.
fn triangles(mode: gltf::mesh::Mode, corners: &[u32]) -> Vec<u32> {
    match mode {
        gltf::mesh::Mode::TriangleStrip => (2..corners.len())
            .flat_map(|index| {
                let [a, b, c] = [corners[index - 2], corners[index - 1], corners[index]];
                // Every other triangle runs the other way round.
                if index % 2 == 0 { [a, b, c] } else { [b, a, c] }
            })
            .collect(),
        gltf::mesh::Mode::TriangleFan => (2..corners.len())
            .flat_map(|index| [corners[0], corners[index - 1], corners[index]])
            .collect(),
        _ => corners[..corners.len() / 3 * 3].to_vec(),
    }
}

/// `rgb` in linear light as an sRGB color with `alpha`.
fn srgb_color(rgb: [f32; 3], alpha: f32) -> Color {
    let encode = |value: f32| {
        let value = value.clamp(0.0, 1.0);
        if value <= 0.0031308 {
            value * 12.92
        } else {
            1.055 * value.powf(1.0 / 2.4) - 0.055
        }
    };
    peniko::Color::new([
        encode(rgb[0]),
        encode(rgb[1]),
        encode(rgb[2]),
        alpha.clamp(0.0, 1.0),
    ])
}

/// `uri` with its `%XX` escapes decoded, as relative glTF URIs are written.
fn percent_decode(uri: &str) -> String {
    let bytes = uri.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&uri[index + 1..index + 3], 16)
        {
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(interpolation: ChannelInterpolation, values: Vec<[f64; 4]>) -> GltfChannel {
        GltfChannel {
            node: 0,
            target: ChannelTarget::Translation,
            interpolation,
            times: vec![0.0, 1.0],
            values,
        }
    }

    #[test]
    fn channels_hold_their_ends_and_interpolate_between_keys() {
        let linear = channel(
            ChannelInterpolation::Linear,
            vec![[0.0, 0.0, 0.0, 0.0], [2.0, 4.0, 0.0, 0.0]],
        );
        assert_eq!(linear.sample(-1.0)[0], 0.0);
        assert_eq!(linear.sample(0.5), [1.0, 2.0, 0.0, 0.0]);
        assert_eq!(linear.sample(3.0)[1], 4.0);
        let step = channel(
            ChannelInterpolation::Step,
            vec![[0.0, 0.0, 0.0, 0.0], [2.0, 4.0, 0.0, 0.0]],
        );
        assert_eq!(step.sample(0.99)[0], 0.0);
        assert_eq!(step.sample(1.0)[0], 2.0);
    }

    #[test]
    fn cubic_splines_pass_through_their_values_with_their_tangents() {
        // Flat tangents: an ease between the two values.
        let spline = channel(
            ChannelInterpolation::CubicSpline,
            vec![
                [0.0; 4],
                [0.0, 0.0, 0.0, 0.0],
                [0.0; 4],
                [0.0; 4],
                [1.0, 0.0, 0.0, 0.0],
                [0.0; 4],
            ],
        );
        assert_eq!(spline.sample(0.0)[0], 0.0);
        assert!((spline.sample(0.5)[0] - 0.5).abs() < 1e-12);
        assert!(spline.sample(0.25)[0] < 0.25);
        assert_eq!(spline.sample(1.0)[0], 1.0);
    }

    #[test]
    fn rotations_take_the_shorter_arc() {
        let quarter = DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2);
        let rotation = GltfChannel {
            node: 0,
            target: ChannelTarget::Rotation,
            interpolation: ChannelInterpolation::Linear,
            times: vec![0.0, 1.0],
            // The same turn, written with opposite signs at the end.
            values: vec![DQuat::IDENTITY.to_array(), (-quarter).to_array()],
        };
        let half = DQuat::from_array(rotation.sample(0.5));
        let expected = DQuat::from_rotation_y(std::f64::consts::FRAC_PI_4);
        assert!(half.dot(expected).abs() > 1.0 - 1e-9);
    }

    #[test]
    fn strips_and_fans_become_triangle_lists() {
        assert_eq!(
            triangles(gltf::mesh::Mode::TriangleStrip, &[0, 1, 2, 3]),
            vec![0, 1, 2, 2, 1, 3]
        );
        assert_eq!(
            triangles(gltf::mesh::Mode::TriangleFan, &[0, 1, 2, 3]),
            vec![0, 1, 2, 0, 2, 3]
        );
        assert_eq!(
            triangles(gltf::mesh::Mode::Triangles, &[0, 1, 2, 3]),
            vec![0, 1, 2]
        );
    }

    /// A skinned triangle on two joints, `root` and its child `tip`, and an
    /// animation that turns `tip`, all in one glTF with an embedded buffer.
    fn skinned_triangle() -> Vec<u8> {
        use base64::Engine as _;
        let mut bytes = Vec::new();
        let floats = |bytes: &mut Vec<u8>, values: &[f32]| {
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        };
        // Positions (0..36), indices (36..42, padded to 44), joints (44..56),
        // weights (56..104), inverse binds (104..232), times (232..240) and
        // rotations (240..272).
        floats(&mut bytes, &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
        for index in [0_u16, 1, 2, 0] {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
        bytes.extend_from_slice(&[0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0]);
        floats(
            &mut bytes,
            &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0],
        );
        let identity = DMat4::IDENTITY.as_mat4().to_cols_array();
        let tip = DMat4::from_translation(DVec3::new(0.0, -1.0, 0.0))
            .as_mat4()
            .to_cols_array();
        floats(&mut bytes, &identity);
        floats(&mut bytes, &tip);
        floats(&mut bytes, &[0.0, 2.0]);
        let quarter = DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2).as_quat();
        floats(&mut bytes, &[0.0, 0.0, 0.0, 1.0]);
        floats(&mut bytes, &quarter.to_array());
        let uri = base64::engine::general_purpose::STANDARD.encode(&bytes);
        format!(
            r#"{{
  "asset": {{"version": "2.0"}},
  "scene": 0,
  "scenes": [{{"nodes": [0, 2]}}],
  "nodes": [
    {{"name": "root", "children": [1], "translation": [1, 0, 0]}},
    {{"name": "tip", "translation": [0, 1, 0]}},
    {{"name": "body", "mesh": 0, "skin": 0}}
  ],
  "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0, "JOINTS_0": 2, "WEIGHTS_0": 3}}, "indices": 1}}]}}],
  "skins": [{{"joints": [0, 1], "inverseBindMatrices": 4}}],
  "animations": [{{"name": "Bend", "channels": [{{"sampler": 0, "target": {{"node": 1, "path": "rotation"}}}}],
    "samplers": [{{"input": 5, "output": 6, "interpolation": "LINEAR"}}]}}],
  "buffers": [{{"byteLength": {length}, "uri": "data:application/octet-stream;base64,{uri}"}}],
  "bufferViews": [
    {{"buffer": 0, "byteOffset": 0, "byteLength": 36}},
    {{"buffer": 0, "byteOffset": 36, "byteLength": 6}},
    {{"buffer": 0, "byteOffset": 44, "byteLength": 12}},
    {{"buffer": 0, "byteOffset": 56, "byteLength": 48}},
    {{"buffer": 0, "byteOffset": 104, "byteLength": 128}},
    {{"buffer": 0, "byteOffset": 232, "byteLength": 8}},
    {{"buffer": 0, "byteOffset": 240, "byteLength": 32}}
  ],
  "accessors": [
    {{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0]}},
    {{"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}},
    {{"bufferView": 2, "componentType": 5121, "count": 3, "type": "VEC4"}},
    {{"bufferView": 3, "componentType": 5126, "count": 3, "type": "VEC4"}},
    {{"bufferView": 4, "componentType": 5126, "count": 2, "type": "MAT4"}},
    {{"bufferView": 5, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0], "max": [2]}},
    {{"bufferView": 6, "componentType": 5126, "count": 2, "type": "VEC4"}}
  ]
}}"#,
            length = bytes.len(),
        )
        .into_bytes()
    }

    #[test]
    fn a_skinned_animated_model_reads_its_nodes_skin_and_animation() {
        let model = GltfModel::from_slice(
            &skinned_triangle(),
            Path::new("triangle.gltf"),
            &GltfSceneSelector::Default,
        )
        .unwrap();
        let names: Vec<&str> = model.nodes.iter().map(|node| node.name.as_str()).collect();
        assert_eq!(names, ["root", "tip", "body"]);
        assert_eq!(model.nodes[1].parent, Some(0));
        assert_eq!(model.roots, [0, 2]);
        assert_eq!(model.nodes[0].rest.translation, DVec3::X);
        // The body is skinned on both joints, with weights that sum to one.
        assert_eq!(model.nodes[2].skin, Some(0));
        assert_eq!(model.skins[0].joints, [0, 1]);
        let mesh = &model.nodes[2].primitives[0].mesh;
        let skin = mesh.surface.as_ref().unwrap().skin.as_ref().unwrap();
        assert_eq!(skin.joints[2], [0, 1, 0, 0]);
        assert_eq!(skin.weights[2], [0.5, 0.5, 0.0, 0.0]);
        assert!(mesh.material.is_some());
        assert!(
            model.skins[0].inverse_bind[1]
                .abs_diff_eq(DMat4::from_translation(DVec3::new(0.0, -1.0, 0.0)), 1e-6)
        );
        // The animation turns the tip a quarter turn in two seconds.
        assert_eq!(model.clips(), [("Bend".to_owned(), 2.0)]);
        let mut pose = model.animations.rest.clone();
        model.animations.sample(0, 1.0, &mut pose);
        let eighth = DQuat::from_rotation_z(std::f64::consts::FRAC_PI_4);
        assert!(pose[1].rotation.dot(eighth).abs() > 1.0 - 1e-6);
        // The node it does not move keeps its rest pose.
        assert_eq!(pose[0], model.animations.rest[0]);
    }

    #[test]
    fn scenes_are_chosen_by_name_or_index() {
        let bytes = skinned_triangle();
        let path = Path::new("triangle.gltf");
        assert!(GltfModel::from_slice(&bytes, path, &GltfSceneSelector::Index(0)).is_ok());
        assert!(matches!(
            GltfModel::from_slice(&bytes, path, &GltfSceneSelector::Index(3)),
            Err(GltfLoadError::UnknownSceneIndex { count: 1, .. })
        ));
        assert!(matches!(
            GltfModel::from_slice(&bytes, path, &GltfSceneSelector::Name("Other".into())),
            Err(GltfLoadError::UnknownSceneName { .. })
        ));
    }

    #[test]
    fn relative_uris_are_percent_decoded() {
        assert_eq!(percent_decode("my%20texture.png"), "my texture.png");
        assert_eq!(percent_decode("plain.png"), "plain.png");
    }
}
