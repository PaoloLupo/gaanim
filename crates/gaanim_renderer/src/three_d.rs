//! Native 3D content, rasterized on the GPU.
//!
//! Triangle meshes ([`TriangleMeshData`]) and line lists ([`LineListData`])
//! are uploaded once per change ([`MeshSources`]); each frame is a list of
//! draws with their uniforms ([`GpuMeshFrame`]) that [`GpuThreeD`] rasterizes
//! on the device Vello renders with, with a depth buffer and 4x MSAA. The
//! frame's composition draws the result as an image beneath the 2D content;
//! the image's texture is handed to Vello with
//! [`vello::Renderer::override_image`], as shader backgrounds are. Drawables
//! that compose apart (each side of a transition, those the post-process
//! leaves out, each view layer) draw into images of their own.
//!
//! A mesh without a [`Material3D`] is unlit and double-sided; a mesh with one
//! is lit by the scene's [`Lighting3D`] in Gaanim's illustrative style
//! (`shade` in [`SHADER`]), per pixel in linear light, and hides its back
//! faces. Lines are unlit, 1.5 output pixels wide with round ends. Faces and
//! lines meet in the depth buffer, and geometry crossing the near plane is
//! clipped. A translucent drawable (an opacity, or colors with alpha) draws
//! as one layer, after the opaque ones and from back to front: its nearest
//! surface, kept by a depth buffer of its own and hidden by what is opaque in
//! front of it, blends once over what is behind it.

use std::any::Any;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};

use gaanim_core::glam::{DMat4, DVec3};
use gaanim_core::{kurbo, peniko};
use gaanim_math::{Camera, GlobalSpatialTransform, Projection};
use gaanim_scene::{
    GlobalOpacity, Light3D, LightKind3D, Lighting3D, LineListData, Material3D, TriangleMeshData,
};
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use vello::wgpu;
use wgpu::util::DeviceExt;

/// A light as shading uses it: where it is in the world and its linear
/// color, scaled by its intensity and its entity's opacity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Light {
    pub position: DVec3,
    pub directional: bool,
    pub color: [f32; 3],
}

impl Light {
    /// The light `light` gives from `position`, dimmed to `opacity`; `None`
    /// when it gives none.
    pub fn new(light: &Light3D, position: DVec3, opacity: f32) -> Option<Self> {
        let strength = light.intensity.max(0.0) * opacity.clamp(0.0, 1.0);
        let [r, g, b, a] = to_linear(light.color);
        let color = [r, g, b].map(|channel| channel * a * strength);
        (strength > 0.0 && position.is_finite()).then_some(Self {
            position,
            directional: light.kind == LightKind3D::Directional,
            color,
        })
    }
}

/// How lit 3D content is lit in a frame: the scene's [`Lighting3D`] and the
/// lights it shows.
#[derive(Debug, Clone, Default)]
pub struct Illumination {
    pub lighting: Lighting3D,
    pub lights: Vec<Light>,
}

/// One drawable's 3D content, as the ECS holds it.
pub(crate) struct Content<'a> {
    pub transform: &'a GlobalSpatialTransform,
    pub opacity: &'a GlobalOpacity,
    pub mesh: Option<&'a TriangleMeshData>,
    pub lines: Option<&'a LineListData>,
    pub material: Option<&'a Material3D>,
}

/// Width of 3D lines, in output pixels.
const LINE_WIDTH_PX: f64 = 1.5;

/// How `camera` maps its NDC onto the canvas, in scene units, and the width
/// of lines in scene units; `None` when it shows nothing.
pub(crate) fn canvas_projection(camera: &Camera) -> Option<(kurbo::Affine, f64)> {
    let width = f64::from(camera.viewport_width);
    let height = f64::from(camera.viewport_height);
    if width <= 0.0 || height <= 0.0 || camera.frame_width <= 0.0 {
        return None;
    }
    // Under perspective the 2D camera stays at the origin, unrotated and
    // unzoomed (see `sync_gaanim_camera_to_bevy_system`); orthographic
    // cameras move the canvas themselves.
    let canvas = match camera.projection {
        Projection::Perspective { .. } => {
            let ppu = camera.pixels_per_unit();
            kurbo::Affine::translate((width * 0.5, height * 0.5))
                * kurbo::Affine::scale_non_uniform(ppu, -ppu)
        }
        Projection::Orthographic { .. } => camera.to_vello_transform(),
    };
    let pixels_per_unit = canvas.determinant().abs().sqrt();
    if !pixels_per_unit.is_finite() || pixels_per_unit <= 0.0 {
        return None;
    }
    let ndc_to_pixels = kurbo::Affine::new([
        width * 0.5,
        0.0,
        0.0,
        -height * 0.5,
        width * 0.5,
        height * 0.5,
    ]);
    Some((
        canvas.inverse() * ndc_to_pixels,
        LINE_WIDTH_PX / pixels_per_unit,
    ))
}

/// Bytes of a mesh vertex: position, normal and linear color.
const MESH_VERTEX_BYTES: usize = 40;
/// Bytes of a line segment: both ends (the first with a vertex-color flag)
/// and their linear colors.
const SEGMENT_BYTES: usize = 64;
/// Bytes of the frame uniforms (`Frame` in [`SHADER`]).
const FRAME_BYTES: usize = 400;
/// Bytes of one draw's uniforms (`Draw` in [`SHADER`]), and their stride in
/// the draw buffer, the minimum uniform offset alignment.
const DRAW_BYTES: usize = 176;
const DRAW_STRIDE: usize = 256;
/// Lights the shader reads; further lights are left out.
const MAX_LIGHTS: usize = 8;
const SAMPLES: u32 = 4;
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// A mesh's vertices and triangles, or a line list's segments, as the
/// shader reads them. Built once per change of the mesh or the lines.
pub struct MeshSource {
    vertices: Vec<u8>,
    /// Triangle corners of a mesh; empty for lines.
    indices: Vec<u32>,
    /// Corners of a mesh, segments of lines.
    count: u32,
    lines: bool,
    /// The geometry carries its own colors, and whether all are opaque.
    has_colors: bool,
    opaque_colors: bool,
    /// Bounding sphere in model space.
    center: DVec3,
    radius: f64,
}

impl MeshSource {
    /// `None` when `mesh` has no triangle.
    fn mesh(mesh: &TriangleMeshData) -> Option<Self> {
        let count = mesh.vertices.len();
        let positions: Vec<DVec3> = mesh
            .vertices
            .iter()
            .map(|v| DVec3::new(v[0].into(), v[1].into(), v[2].into()))
            .collect();
        let indices: Vec<u32> = mesh
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|triangle| triangle.iter().all(|&index| (index as usize) < count))
            .flatten()
            .copied()
            .collect();
        if indices.is_empty() {
            return None;
        }
        let colors = mesh.colors.as_ref().filter(|colors| colors.len() == count);
        // Unit normals, or zero where the shader takes the face's.
        let normals: Vec<DVec3> = match mesh.normals.as_ref().filter(|n| n.len() == count) {
            Some(normals) => normals
                .iter()
                .map(|n| DVec3::new(n[0].into(), n[1].into(), n[2].into()).normalize_or_zero())
                .collect(),
            None => {
                let mut normals = vec![DVec3::ZERO; count];
                for triangle in indices.as_chunks::<3>().0 {
                    let [a, b, c] = triangle.map(|index| index as usize);
                    let face = (positions[b] - positions[a]).cross(positions[c] - positions[a]);
                    for index in [a, b, c] {
                        normals[index] += face;
                    }
                }
                normals.iter().map(|n| n.normalize_or_zero()).collect()
            }
        };
        let mut vertices = Vec::with_capacity(count * MESH_VERTEX_BYTES);
        for (index, (p, n)) in positions.iter().zip(&normals).enumerate() {
            push_f32s(
                &mut vertices,
                &[
                    p.x as f32, p.y as f32, p.z as f32, n.x as f32, n.y as f32, n.z as f32,
                ],
            );
            push_f32s(
                &mut vertices,
                &colors.map_or([0.0; 4], |colors| colors[index]),
            );
        }
        let (center, radius) = bounding_sphere(&positions);
        Some(Self {
            vertices,
            count: indices.len() as u32,
            indices,
            lines: false,
            has_colors: colors.is_some(),
            opaque_colors: colors.is_none_or(|colors| colors.iter().all(|c| c[3] >= 0.999)),
            center,
            radius,
        })
    }

    /// The segments of `lines`: consecutive points of a strip, index pairs,
    /// or else consecutive pairs of points; `None` when it has none.
    fn lines(lines: &LineListData) -> Option<Self> {
        let count = lines.points.len();
        let colors = lines.colors.as_ref().filter(|colors| colors.len() == count);
        let pairs: Vec<(usize, usize)> = if lines.strip {
            (1..count).map(|index| (index - 1, index)).collect()
        } else if let Some(indices) = &lines.indices {
            indices
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[a, b]| (a as usize, b as usize))
                .filter(|&(a, b)| a < count && b < count)
                .collect()
        } else {
            (0..count / 2)
                .map(|pair| (pair * 2, pair * 2 + 1))
                .collect()
        };
        if pairs.is_empty() {
            return None;
        }
        let mut vertices = Vec::with_capacity(pairs.len() * SEGMENT_BYTES);
        for (a, b) in &pairs {
            let [pa, pb] = [lines.points[*a], lines.points[*b]];
            push_f32s(
                &mut vertices,
                &[pa[0], pa[1], pa[2], f32::from(u8::from(colors.is_some()))],
            );
            push_f32s(&mut vertices, &[pb[0], pb[1], pb[2], 0.0]);
            push_f32s(&mut vertices, &colors.map_or([0.0; 4], |colors| colors[*a]));
            push_f32s(&mut vertices, &colors.map_or([0.0; 4], |colors| colors[*b]));
        }
        let positions: Vec<DVec3> = lines
            .points
            .iter()
            .map(|p| DVec3::new(p[0].into(), p[1].into(), p[2].into()))
            .collect();
        let (center, radius) = bounding_sphere(&positions);
        Some(Self {
            vertices,
            indices: Vec::new(),
            count: pairs.len() as u32,
            lines: true,
            has_colors: colors.is_some(),
            opaque_colors: colors.is_none_or(|colors| colors.iter().all(|c| c[3] >= 0.999)),
            center,
            radius,
        })
    }
}

/// Center and radius of a sphere around `points`.
fn bounding_sphere(points: &[DVec3]) -> (DVec3, f64) {
    let (low, high) = points.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(low, high), p| (low.min(*p), high.max(*p)),
    );
    let center = (low + high) * 0.5;
    let radius = points
        .iter()
        .map(|p| p.distance(center))
        .fold(0.0, f64::max);
    (center, radius)
}

fn push_f32s(bytes: &mut Vec<u8>, values: &[f32]) {
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

fn push_mat4(bytes: &mut Vec<u8>, matrix: DMat4) {
    push_f32s(bytes, &matrix.to_cols_array().map(|value| value as f32));
}

/// What keeps a captured mesh alive while its address keys its upload.
pub(crate) type Keep = Arc<dyn Any + Send + Sync>;

/// The uploaded meshes and lines of the drawables, kept from frame to frame
/// while they do not change: by drawable and change tick in a world, by
/// address in a recording.
#[derive(bevy::prelude::Resource, Default)]
pub struct MeshSources {
    /// By key and whether it is lines.
    sources: HashMap<(u64, bool), Held>,
    used: HashSet<(u64, bool)>,
}

struct Held {
    revision: u32,
    source: Option<Arc<MeshSource>>,
    _keep: Option<Keep>,
}

impl MeshSources {
    /// The source of the mesh or lines (`lines`) keyed `key` at `revision`,
    /// built by `build` when they changed.
    fn get(
        &mut self,
        key: u64,
        lines: bool,
        revision: u32,
        keep: Option<Keep>,
        build: impl FnOnce() -> Option<MeshSource>,
    ) -> Option<Arc<MeshSource>> {
        self.used.insert((key, lines));
        match self.sources.entry((key, lines)) {
            Entry::Occupied(mut entry) => {
                let held = entry.get_mut();
                if held.revision != revision {
                    held.revision = revision;
                    held.source = build().map(Arc::new);
                    held._keep = keep;
                }
                held.source.clone()
            }
            Entry::Vacant(entry) => entry
                .insert(Held {
                    revision,
                    source: build().map(Arc::new),
                    _keep: keep,
                })
                .source
                .clone(),
        }
    }

    /// Forget the meshes and lines this frame did not draw.
    fn end_frame(&mut self) {
        let used = std::mem::take(&mut self.used);
        self.sources.retain(|key, _| used.contains(key));
    }
}

/// The image the meshes of a frame's `slot` draw into: one per size and
/// slot, so that frames refresh the same slots of Vello's atlas.
fn frame_image(width: u32, height: u32, slot: usize) -> ImageData {
    static IMAGES: Mutex<Vec<(usize, ImageData)>> = Mutex::new(Vec::new());
    let mut images = IMAGES.lock().expect("3D image cache poisoned");
    if let Some((_, image)) = images
        .iter()
        .find(|(held, image)| *held == slot && image.width == width && image.height == height)
    {
        return image.clone();
    }
    if images.len() >= 8 {
        images.remove(0);
    }
    // Vello copies the registered texture instead of reading these bytes; a
    // missing texture draws a transparent image.
    let image = ImageData {
        data: Blob::from(vec![0_u8; width as usize * height as usize * 4]),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width,
        height,
    };
    images.push((slot, image.clone()));
    image
}

/// One mesh or line list of a frame, as [`GpuThreeD`] draws it.
#[derive(Clone)]
struct GpuDraw {
    source: Arc<MeshSource>,
    /// Lit meshes hide their back faces.
    culled: bool,
    /// Drawn after the opaque meshes, as one layer.
    translucent: bool,
}

/// The meshes and lines one image of a frame draws, with what the shader
/// reads.
#[derive(Clone)]
pub struct GpuMeshFrame {
    image: ImageData,
    /// Maps the image's pixels into the scene (the inverse of the placement
    /// the meshes were drawn for).
    image_to_scene: kurbo::Affine,
    uniforms: Arc<[u8]>,
    /// Opaque draws first, then translucent ones from back to front.
    draws: Arc<[GpuDraw]>,
    /// `DRAW_STRIDE` bytes per draw.
    draw_uniforms: Arc<[u8]>,
    digest: u64,
}

impl GpuMeshFrame {
    /// A scene drawing the frame's image, in scene units.
    pub(crate) fn scene(&self) -> vello::Scene {
        let mut scene = vello::Scene::new();
        scene.draw_image(&self.image, self.image_to_scene);
        scene
    }

    /// Feed `update` what the frame draws: its uniforms and the uploaded
    /// geometry of its draws, for a digest of the frame.
    pub fn hash_content(&self, update: &mut dyn FnMut(&[u8])) {
        update(&self.uniforms);
        update(&self.draw_uniforms);
        for draw in self.draws.iter() {
            update(&[u8::from(draw.culled), u8::from(draw.translucent)]);
            update(&draw.source.vertices);
            for index in &draw.source.indices {
                update(&index.to_le_bytes());
            }
        }
    }
}

/// A drawable's mesh or lines, waiting for the frame to be built.
struct Pending {
    class: usize,
    source: Arc<MeshSource>,
    model: DMat4,
    opacity: f32,
    base: [f32; 4],
    material: Option<Material3D>,
    translucent: bool,
}

/// Collects the meshes and lines of a frame that the GPU draws, in classes
/// that each draw into an image of their own.
pub(crate) struct GpuMeshes<'a> {
    sources: &'a mut MeshSources,
    /// Pixels of the images, and the map from scene units to them.
    size: (u32, u32),
    placement: kurbo::Affine,
    /// The first image slot the classes take (see [`frame_image`]).
    slot: usize,
    pending: Vec<Pending>,
}

impl<'a> GpuMeshes<'a> {
    /// Meshes drawn into `size` images placed by `placement`, which maps
    /// scene units to their pixels, from image `slot` on: hosts that draw at
    /// once with one Vello renderer take slots apart.
    pub(crate) fn new(
        sources: &'a mut MeshSources,
        size: (u32, u32),
        placement: kurbo::Affine,
        slot: usize,
    ) -> Option<Self> {
        (size.0 > 0 && size.1 > 0 && placement.determinant().abs() > 1e-12).then_some(Self {
            sources,
            size,
            placement,
            slot,
            pending: Vec::new(),
        })
    }

    /// Take the mesh and the lines of a drawable into the image of `class`.
    /// `keys` are a key and a revision for the mesh and for the lines, which
    /// identify their uploads (see [`MeshSources`]); `keep` holds what a key
    /// by address points to. Returns whether the GPU took the mesh and
    /// whether it took the lines.
    pub(crate) fn push(
        &mut self,
        class: usize,
        keys: [(u64, u32); 2],
        keep: [Option<Keep>; 2],
        content: &Content<'_>,
    ) -> (bool, bool) {
        let opacity = content.opacity.0.clamp(0.0, 1.0);
        let [mesh_key, lines_key] = keys;
        let [mesh_keep, lines_keep] = keep;
        let mut taken = (false, false);
        if let Some(mesh) = content.mesh
            && let Some(source) = self
                .sources
                .get(mesh_key.0, false, mesh_key.1, mesh_keep, || {
                    MeshSource::mesh(mesh)
                })
        {
            let base = content
                .material
                .map(|material| material.color)
                .or(mesh.color)
                .unwrap_or(peniko::Color::WHITE);
            self.add(
                class,
                source,
                content,
                opacity,
                base,
                content.material.copied(),
            );
            taken.0 = true;
        }
        if let Some(lines) = content.lines
            && let Some(source) =
                self.sources
                    .get(lines_key.0, true, lines_key.1, lines_keep, || {
                        MeshSource::lines(lines)
                    })
        {
            self.add(class, source, content, opacity, lines.color, None);
            taken.1 = true;
        }
        taken
    }

    fn add(
        &mut self,
        class: usize,
        source: Arc<MeshSource>,
        content: &Content<'_>,
        opacity: f32,
        base: peniko::Color,
        material: Option<Material3D>,
    ) {
        if opacity <= f32::EPSILON {
            return;
        }
        let opaque_colors = if source.has_colors {
            source.opaque_colors
        } else {
            base.components[3] >= 0.999
        };
        self.pending.push(Pending {
            class,
            translucent: opacity < 0.999 || !opaque_colors,
            source,
            model: content.transform.mat4,
            opacity,
            base: to_linear(base),
            material,
        });
    }

    /// The frames of the classes that took something, seen by `camera`,
    /// whose canvas projection maps NDC to scene units by `ndc_to_scene` and
    /// draws lines `line_width` scene units wide.
    pub(crate) fn finish(
        self,
        camera: &Camera,
        ndc_to_scene: kurbo::Affine,
        line_width: f64,
        illumination: &Illumination,
    ) -> Vec<(usize, GpuMeshFrame)> {
        self.sources.end_frame();
        if self.pending.is_empty() {
            return Vec::new();
        }
        let (width, height) = self.size;
        let uniforms: Arc<[u8]> = frame_uniforms(
            camera,
            pixels_to_ndc(width, height) * self.placement * ndc_to_scene,
            (width, height),
            line_width * self.placement.determinant().abs().sqrt(),
            illumination,
        )
        .into();

        // Opaque draws first, near ones first so that the depth test skips
        // what they hide; translucent ones after them, from back to front.
        let view = camera.view_matrix();
        let mut pending: Vec<(f64, Pending)> = self
            .pending
            .into_iter()
            .map(|pending| {
                let center = pending.model.transform_point3(pending.source.center);
                let scale = [
                    pending.model.x_axis.truncate().length(),
                    pending.model.y_axis.truncate().length(),
                    pending.model.z_axis.truncate().length(),
                ]
                .into_iter()
                .fold(0.0, f64::max);
                let depth = -view.transform_point3(center).z + pending.source.radius * scale;
                (depth, pending)
            })
            .collect();
        pending.sort_by(|(a_depth, a), (b_depth, b)| {
            a.class
                .cmp(&b.class)
                .then(a.translucent.cmp(&b.translucent))
                .then_with(|| {
                    if a.translucent {
                        b_depth.total_cmp(a_depth)
                    } else {
                        a_depth.total_cmp(b_depth)
                    }
                })
        });

        let mut frames = Vec::new();
        let mut rest = pending.as_slice();
        while let Some((_, first)) = rest.first() {
            let class = first.class;
            let end = rest
                .iter()
                .position(|(_, pending)| pending.class != class)
                .unwrap_or(rest.len());
            let (members, after) = rest.split_at(end);
            rest = after;
            let image = frame_image(width, height, self.slot + class);
            let mut draws = Vec::with_capacity(members.len());
            let mut draw_uniforms = Vec::with_capacity(members.len() * DRAW_STRIDE);
            for (_, pending) in members {
                push_draw_uniforms(&mut draw_uniforms, pending);
                draws.push(GpuDraw {
                    culled: pending.material.is_some() && !pending.source.lines,
                    translucent: pending.translucent,
                    source: Arc::clone(&pending.source),
                });
            }
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            image.data.id().hash(&mut hasher);
            uniforms.hash(&mut hasher);
            draw_uniforms.hash(&mut hasher);
            for draw in &draws {
                (Arc::as_ptr(&draw.source) as usize).hash(&mut hasher);
                draw.culled.hash(&mut hasher);
                draw.translucent.hash(&mut hasher);
            }
            frames.push((
                class,
                GpuMeshFrame {
                    image,
                    image_to_scene: self.placement.inverse(),
                    uniforms: Arc::clone(&uniforms),
                    draws: draws.into(),
                    draw_uniforms: draw_uniforms.into(),
                    digest: hasher.finish(),
                },
            ));
        }
        frames
    }
}

/// Maps a `width` x `height` image's pixels to its NDC.
fn pixels_to_ndc(width: u32, height: u32) -> kurbo::Affine {
    kurbo::Affine::new([
        2.0 / f64::from(width),
        0.0,
        0.0,
        -2.0 / f64::from(height),
        -1.0,
        1.0,
    ])
}

/// The frame uniforms: `camera`, the map `ndc_map` from its NDC to the
/// image's, the image's `size`, lines `line_px` pixels wide and the lights.
fn frame_uniforms(
    camera: &Camera,
    ndc_map: kurbo::Affine,
    size: (u32, u32),
    line_px: f64,
    illumination: &Illumination,
) -> Vec<u8> {
    let [a, b, c, d, e, f] = ndc_map.as_coeffs();
    let perspective = matches!(camera.projection, Projection::Perspective { .. });
    let forward = camera.rotation * DVec3::NEG_Z;
    let lighting = &illumination.lighting;
    let intensity = if lighting.enabled {
        lighting.intensity.max(0.0)
    } else {
        0.0
    };
    let lights = &illumination.lights[..illumination.lights.len().min(MAX_LIGHTS)];
    let mut uniforms = Vec::with_capacity(FRAME_BYTES);
    push_mat4(
        &mut uniforms,
        camera.projection_matrix() * camera.view_matrix(),
    );
    // With half the image's size in pixels, which lines are measured in.
    push_f32s(
        &mut uniforms,
        &[a as f32, c as f32, e as f32, size.0 as f32 * 0.5],
    );
    push_f32s(
        &mut uniforms,
        &[b as f32, d as f32, f as f32, size.1 as f32 * 0.5],
    );
    let eye = camera.position;
    push_f32s(
        &mut uniforms,
        &[
            eye.x as f32,
            eye.y as f32,
            eye.z as f32,
            f32::from(u8::from(perspective)),
        ],
    );
    push_f32s(
        &mut uniforms,
        &[
            forward.x as f32,
            forward.y as f32,
            forward.z as f32,
            intensity,
        ],
    );
    push_f32s(
        &mut uniforms,
        &[
            f32::from(u8::from(lights.is_empty())),
            lights.len() as f32,
            (line_px * 0.5) as f32,
            0.0,
        ],
    );
    for index in 0..MAX_LIGHTS {
        match lights.get(index) {
            Some(light) => {
                let p = light.position;
                push_f32s(
                    &mut uniforms,
                    &[
                        p.x as f32,
                        p.y as f32,
                        p.z as f32,
                        f32::from(u8::from(light.directional)),
                    ],
                );
                push_f32s(
                    &mut uniforms,
                    &[light.color[0], light.color[1], light.color[2], 0.0],
                );
            }
            None => push_f32s(&mut uniforms, &[0.0; 8]),
        }
    }
    debug_assert_eq!(uniforms.len(), FRAME_BYTES);
    uniforms
}

/// Append one draw's uniforms, padded to `DRAW_STRIDE`.
fn push_draw_uniforms(bytes: &mut Vec<u8>, pending: &Pending) {
    let start = bytes.len();
    push_mat4(bytes, pending.model);
    push_mat4(bytes, pending.model.inverse().transpose());
    push_f32s(bytes, &pending.base);
    let (emissive, surface) = match pending.material {
        Some(material) => {
            let glow = to_linear(material.emissive);
            let strength = material.emissive_strength.max(0.0);
            (
                [glow[0] * strength, glow[1] * strength, glow[2] * strength],
                [material.roughness, material.metallic, 1.0],
            )
        }
        None => ([0.0; 3], [0.0; 3]),
    };
    push_f32s(
        bytes,
        &[emissive[0], emissive[1], emissive[2], pending.opacity],
    );
    push_f32s(
        bytes,
        &[
            surface[0],
            surface[1],
            surface[2],
            f32::from(u8::from(pending.source.has_colors)),
        ],
    );
    debug_assert_eq!(bytes.len() - start, DRAW_BYTES);
    bytes.resize(start + DRAW_STRIDE, 0);
}

fn to_linear(color: peniko::Color) -> [f32; 4] {
    let [r, g, b, a] = color.components;
    let linear = |value: f32| {
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    [linear(r), linear(g), linear(b), a]
}

/// The texture an image's frame resolves into, which Vello draws.
struct Target {
    image: ImageData,
    resolved: wgpu::Texture,
    resolved_view: wgpu::TextureView,
    /// Digest of the frame it holds.
    drawn: Option<u64>,
}

impl Target {
    fn new(device: &wgpu::Device, image: &ImageData) -> Self {
        let resolved = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gaanim-3d-image"),
            size: wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        Self {
            image: image.clone(),
            resolved_view: resolved.create_view(&Default::default()),
            resolved,
            drawn: None,
        }
    }

    fn texture_copy(&self) -> wgpu::TexelCopyTextureInfoBase<wgpu::Texture> {
        wgpu::TexelCopyTextureInfoBase {
            texture: self.resolved.clone(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        }
    }
}

/// The multisampled targets frames draw in before they resolve: color, the
/// opaque depth, and the depth of one translucent layer.
struct Scratch {
    size: (u32, u32),
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    layer: wgpu::TextureView,
    /// Binds `depth` for the translucent layers to read.
    depth_group: wgpu::BindGroup,
}

impl Scratch {
    fn new(device: &wgpu::Device, pipelines: &Pipelines, size: (u32, u32)) -> Self {
        let texture = |label, format, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: SAMPLES,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let depth = texture(
            "gaanim-3d-depth",
            DEPTH_FORMAT,
            attachment | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gaanim-3d-opaque-depth"),
            layout: &pipelines.depth_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&depth),
            }],
        });
        Self {
            size,
            color: texture("gaanim-3d-color", COLOR_FORMAT, attachment),
            layer: texture("gaanim-3d-layer-depth", DEPTH_FORMAT, attachment),
            depth,
            depth_group,
        }
    }
}

/// What a draw does in its pass.
#[derive(Clone, Copy)]
enum Pass {
    /// Opaque: writes color and depth.
    Opaque = 0,
    /// A translucent drawable's nearest surface in front of the opaque
    /// depth, into the layer's depth.
    Layer = 1,
    /// That surface, blended once.
    Blend = 2,
}

struct Pipelines {
    layout: wgpu::BindGroupLayout,
    depth_layout: wgpu::BindGroupLayout,
    /// Meshes by `[culled][pass]`.
    meshes: [[wgpu::RenderPipeline; 3]; 2],
    /// Lines by pass.
    lines: [wgpu::RenderPipeline; 3],
}

impl Pipelines {
    fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-3d"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let uniform = |binding, dynamic, size: usize| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: NonZeroU64::new(size as u64),
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-3d"),
            entries: &[uniform(0, false, FRAME_BYTES), uniform(1, true, DRAW_BYTES)],
        });
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-3d-opaque-depth"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: true,
                },
                count: None,
            }],
        });
        let plain = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-3d"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let layered = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-3d-layer"),
            bind_group_layouts: &[Some(&layout), Some(&depth_layout)],
            immediate_size: 0,
        });
        let mesh_attributes = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 12,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 24,
                shader_location: 2,
            },
        ];
        let segment_attributes: [wgpu::VertexAttribute; 4] =
            std::array::from_fn(|index| wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: index as u64 * 16,
                shader_location: index as u32,
            });
        let pipeline = |lines: bool, culled: bool, pass: Pass| {
            let (vertex, fragment) = match (lines, pass) {
                (false, Pass::Layer) => ("vertex", "fragment_layer"),
                (false, _) => ("vertex", "fragment"),
                (true, Pass::Layer) => ("vertex_line", "fragment_line_layer"),
                (true, _) => ("vertex_line", "fragment_line"),
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("gaanim-3d"),
                layout: Some(if matches!(pass, Pass::Layer) {
                    &layered
                } else {
                    &plain
                }),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers: &[if lines {
                        wgpu::VertexBufferLayout {
                            array_stride: SEGMENT_BYTES as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &segment_attributes,
                        }
                    } else {
                        wgpu::VertexBufferLayout {
                            array_stride: MESH_VERTEX_BYTES as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &mesh_attributes,
                        }
                    }],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: culled.then_some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(!matches!(pass, Pass::Blend)),
                    // Reversed depth: near is 1, far 0.
                    depth_compare: Some(match pass {
                        Pass::Opaque => wgpu::CompareFunction::Greater,
                        Pass::Layer => wgpu::CompareFunction::GreaterEqual,
                        Pass::Blend => wgpu::CompareFunction::Equal,
                    }),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: matches!(pass, Pass::Blend)
                            .then_some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: if matches!(pass, Pass::Layer) {
                            wgpu::ColorWrites::empty()
                        } else {
                            wgpu::ColorWrites::ALL
                        },
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let passes = |lines, culled| {
            [
                pipeline(lines, culled, Pass::Opaque),
                pipeline(lines, culled, Pass::Layer),
                pipeline(lines, culled, Pass::Blend),
            ]
        };
        Self {
            layout,
            depth_layout,
            meshes: [passes(false, false), passes(false, true)],
            lines: passes(true, false),
        }
    }
}

/// A source's uploaded buffers.
struct Uploaded {
    /// Holding the source keeps its address, the key, unique.
    _source: Arc<MeshSource>,
    vertices: wgpu::Buffer,
    indices: Option<wgpu::Buffer>,
}

/// Draws [`GpuMeshFrame`]s on the device of a Vello renderer.
#[derive(Default)]
pub struct GpuThreeD {
    device: Option<wgpu::Device>,
    pipelines: Option<Pipelines>,
    uploads: HashMap<usize, Uploaded>,
    /// By image.
    targets: HashMap<u64, Target>,
    scratch: Option<Scratch>,
    frame_buffer: Option<wgpu::Buffer>,
    /// The draw uniforms, with room for that many bytes, and their bind group.
    draw_buffer: Option<(wgpu::Buffer, usize, wgpu::BindGroup)>,
}

impl GpuThreeD {
    /// Forget what the previous renderer held; the next frames register
    /// their images with the new one.
    pub fn renderer_replaced(&mut self) {
        self.targets.clear();
    }

    /// Draw `frames` into their images for the next render of `renderer`,
    /// skipping images that already hold theirs, and release the images no
    /// frame draws; returns whether an image changed. Call before the frame
    /// is submitted to `queue`.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut vello::Renderer,
        frames: &[GpuMeshFrame],
    ) -> bool {
        if self.device.as_ref() != Some(device) {
            for target in self.targets.values() {
                renderer.override_image(&target.image, None);
            }
            *self = Self {
                device: Some(device.clone()),
                ..Self::default()
            };
        }
        self.targets.retain(|id, target| {
            let keep = frames.iter().any(|frame| frame.image.data.id() == *id);
            if !keep {
                renderer.override_image(&target.image, None);
            }
            keep
        });
        if frames.is_empty() {
            self.uploads.clear();
            return false;
        }
        self.uploads.retain(|key, _| {
            frames.iter().any(|frame| {
                frame
                    .draws
                    .iter()
                    .any(|draw| Arc::as_ptr(&draw.source) as usize == *key)
            })
        });
        let mut changed = false;
        for frame in frames {
            let target = self
                .targets
                .entry(frame.image.data.id())
                .or_insert_with(|| {
                    let target = Target::new(device, &frame.image);
                    renderer.override_image(&target.image, Some(target.texture_copy()));
                    target
                });
            if target.drawn == Some(frame.digest) {
                continue;
            }
            target.drawn = Some(frame.digest);
            renderer.mark_override_image_dirty(&target.image);
            changed = true;
            let target_view = target.resolved_view.clone();
            self.draw(device, queue, frame, &target_view);
        }
        changed
    }

    /// Draw `frame` and resolve it into `target`.
    fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &GpuMeshFrame,
        target: &wgpu::TextureView,
    ) {
        let pipelines = self.pipelines.get_or_insert_with(|| Pipelines::new(device));
        let size = (frame.image.width, frame.image.height);
        if self
            .scratch
            .as_ref()
            .is_none_or(|scratch| scratch.size != size)
        {
            self.scratch = Some(Scratch::new(device, pipelines, size));
        }
        let scratch = self.scratch.as_ref().expect("scratch targets are set");
        for draw in frame.draws.iter() {
            self.uploads
                .entry(Arc::as_ptr(&draw.source) as usize)
                .or_insert_with(|| Uploaded {
                    vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("gaanim-3d-vertices"),
                        contents: &draw.source.vertices,
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    indices: (!draw.source.indices.is_empty()).then(|| {
                        let bytes: Vec<u8> = draw
                            .source
                            .indices
                            .iter()
                            .flat_map(|index| index.to_le_bytes())
                            .collect();
                        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("gaanim-3d-indices"),
                            contents: &bytes,
                            usage: wgpu::BufferUsages::INDEX,
                        })
                    }),
                    _source: Arc::clone(&draw.source),
                });
        }

        let frame_buffer = self.frame_buffer.get_or_insert_with(|| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gaanim-3d-frame"),
                size: FRAME_BYTES as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        queue.write_buffer(frame_buffer, 0, &frame.uniforms);
        let needed = frame.draw_uniforms.len().max(DRAW_STRIDE);
        if self
            .draw_buffer
            .as_ref()
            .is_none_or(|(_, capacity, _)| *capacity < needed)
        {
            let capacity = needed.next_power_of_two();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gaanim-3d-draws"),
                size: capacity as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gaanim-3d"),
                layout: &pipelines.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: frame_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &buffer,
                            offset: 0,
                            size: NonZeroU64::new(DRAW_BYTES as u64),
                        }),
                    },
                ],
            });
            self.draw_buffer = Some((buffer, capacity, bind_group));
        }
        let (draw_buffer, _, bind_group) = self.draw_buffer.as_ref().expect("draw buffer is set");
        queue.write_buffer(draw_buffer, 0, &frame.draw_uniforms);

        let translucent = frame.draws.iter().filter(|draw| draw.translucent).count();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("gaanim-3d"),
        });
        // The opaque draws, then one pass per translucent drawable with a
        // layer depth of its own; the last pass resolves.
        let mut draws = frame.draws.iter().enumerate().peekable();
        let mut passes_left = 1 + translucent;
        while passes_left > 0 {
            passes_left -= 1;
            let opaque = passes_left == translucent;
            let last = passes_left == 0;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gaanim-3d"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &scratch.color,
                    depth_slice: None,
                    resolve_target: last.then_some(target),
                    ops: wgpu::Operations {
                        load: if opaque {
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: if last {
                            wgpu::StoreOp::Discard
                        } else {
                            wgpu::StoreOp::Store
                        },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: if opaque {
                        &scratch.depth
                    } else {
                        &scratch.layer
                    },
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        // The translucent layers read the opaque depth.
                        store: if opaque && !last {
                            wgpu::StoreOp::Store
                        } else {
                            wgpu::StoreOp::Discard
                        },
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let draw_one = |pass: &mut wgpu::RenderPass<'_>, index: usize, draw: &GpuDraw| {
                let Some(uploaded) = self.uploads.get(&(Arc::as_ptr(&draw.source) as usize)) else {
                    return;
                };
                let set = if draw.source.lines {
                    &pipelines.lines
                } else {
                    &pipelines.meshes[usize::from(draw.culled)]
                };
                pass.set_bind_group(0, bind_group, &[(index * DRAW_STRIDE) as u32]);
                pass.set_vertex_buffer(0, uploaded.vertices.slice(..));
                if let Some(indices) = &uploaded.indices {
                    pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                }
                let passes: &[Pass] = if draw.translucent {
                    &[Pass::Layer, Pass::Blend]
                } else {
                    &[Pass::Opaque]
                };
                for &each in passes {
                    if matches!(each, Pass::Layer) {
                        pass.set_bind_group(1, &scratch.depth_group, &[]);
                    }
                    pass.set_pipeline(&set[each as usize]);
                    if draw.source.lines {
                        // A quad per segment.
                        pass.draw(0..6, 0..draw.source.count);
                    } else {
                        pass.draw_indexed(0..draw.source.count, 0, 0..1);
                    }
                }
            };
            if opaque {
                while let Some((index, draw)) = draws.next_if(|(_, draw)| !draw.translucent) {
                    draw_one(&mut pass, index, draw);
                }
            } else if let Some((index, draw)) = draws.next() {
                draw_one(&mut pass, index, draw);
            }
        }
        queue.submit(Some(encoder.finish()));
    }
}

/// The shader: [`crate::three_d`]'s projection and illustrative shading.
const SHADER: &str = r#"
struct Light {
    position: vec4<f32>,
    color: vec4<f32>,
}

struct Frame {
    view_proj: mat4x4<f32>,
    // NDC to the image's NDC, applied to clip x, y and w; and half the
    // image's width and height in pixels.
    ndc_x: vec4<f32>,
    ndc_y: vec4<f32>,
    // Eye, and 1 under perspective.
    eye: vec4<f32>,
    // View direction, and the scene's light intensity.
    forward: vec4<f32>,
    // 1 when the fixed key light shines, the count of lights, and half the
    // width of lines in pixels.
    params: vec4<f32>,
    lights: array<Light, 8>,
}

struct Draw {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
    // Linear base color.
    base: vec4<f32>,
    // Linear emission times its strength, and the drawable's opacity.
    emissive: vec4<f32>,
    // Roughness, metalness, 1 when lit, and 1 when vertices have colors.
    material: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<uniform> draw: Draw;
// The opaque depth, which translucent layers lie in front of.
@group(1) @binding(0) var opaque_depth: texture_depth_multisampled_2d;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
}

struct VertexOut {
    @invariant @builtin(position) clip: vec4<f32>,
    // Linear color of the surface.
    @location(0) color: vec4<f32>,
    @location(1) world: vec3<f32>,
    @location(2) normal: vec3<f32>,
}

const KEY_LIGHT = vec3<f32>(-3.0, 6.0, 4.0);
const KEY_WRAP: f32 = 0.35;
const KEY_STRENGTH: f32 = 0.62;
const SKY: f32 = 0.5;
const GROUND: f32 = 0.3;
const SKY_TINT = vec3<f32>(0.86, 0.92, 1.0);
const RIM: f32 = 0.22;

// How much farther lines lie than they are, relative to their depth: a line
// on a surface, as a floor grid under a plot, stays beneath it.
const LINE_DEPTH_BIAS: f32 = 1e-5;

fn normalize_or_zero(v: vec3<f32>) -> vec3<f32> {
    let size = length(v);
    if size > 1e-20 {
        return v / size;
    }
    return vec3<f32>(0.0);
}

fn toward_viewer(point: vec3<f32>) -> vec3<f32> {
    if frame.eye.w > 0.5 {
        return normalize_or_zero(frame.eye.xyz - point);
    }
    return -frame.forward.xyz;
}

fn to_srgb(value: f32) -> f32 {
    let v = clamp(value, 0.0, 1.0);
    if v <= 0.0031308 {
        return v * 12.92;
    }
    return 1.055 * pow(v, 1.0 / 2.4) - 0.055;
}

fn srgb_color(color: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        to_srgb(color.r),
        to_srgb(color.g),
        to_srgb(color.b),
        clamp(color.a, 0.0, 1.0),
    );
}

// Premultiplied sRGB of a linear color at the drawable's opacity.
fn output(color: vec4<f32>) -> vec4<f32> {
    let srgb = srgb_color(color);
    let alpha = srgb.a * draw.emissive.w;
    return vec4<f32>(srgb.rgb * alpha, alpha);
}

// Clip coordinates of a world point in the image, with depth reversed (near
// is 1, far 0), which a float depth buffer holds far more precisely.
fn image_clip(world: vec3<f32>) -> vec4<f32> {
    let clip = frame.view_proj * vec4<f32>(world, 1.0);
    let xyw = vec3<f32>(clip.x, clip.y, clip.w);
    return vec4<f32>(
        dot(frame.ndc_x.xyz, xyw),
        dot(frame.ndc_y.xyz, xyw),
        clip.w - clip.z,
        clip.w,
    );
}

// Whether a translucent layer's fragment lies behind the opaque depth.
fn hidden(position: vec4<f32>, sample: u32) -> bool {
    let opaque = textureLoad(opaque_depth, vec2<i32>(position.xy), i32(sample));
    return position.z < opaque;
}

fn shade(base: vec4<f32>, normal: vec3<f32>, viewer: vec3<f32>, point: vec3<f32>) -> vec4<f32> {
    let metallic = clamp(draw.material.y, 0.0, 1.0);
    let gloss = 1.0 - clamp(draw.material.x, 0.0, 1.0);
    let polish = gloss * gloss;
    let intensity = frame.forward.w;
    let facing_up = 0.5 + 0.5 * normal.y;
    let sky = GROUND + (SKY - GROUND) * facing_up;
    let shininess = 8.0 + 248.0 * polish;
    let fixed = frame.params.x > 0.5;
    let count = select(u32(frame.params.y), 1u, fixed);
    var keyed = vec3<f32>(0.0);
    var highlight = vec3<f32>(0.0);
    for (var i = 0u; i < count; i = i + 1u) {
        var light = Light(vec4<f32>(KEY_LIGHT, 1.0), vec4<f32>(1.0));
        if !fixed {
            light = frame.lights[i];
        }
        var toward_light = light.position.xyz;
        if light.position.w < 0.5 {
            toward_light = light.position.xyz - point;
        }
        toward_light = normalize_or_zero(toward_light);
        let key = max((dot(normal, toward_light) + KEY_WRAP) / (1.0 + KEY_WRAP), 0.0);
        let half = normalize_or_zero(toward_light + viewer);
        let shine = pow(max(dot(normal, half), 0.0), shininess) * polish * min(key, 1.0);
        keyed += KEY_STRENGTH * key * light.color.rgb;
        highlight += shine * light.color.rgb;
    }
    var diffuse: vec3<f32>;
    if fixed {
        diffuse = (vec3<f32>(sky) + keyed) * intensity;
        highlight = highlight * intensity;
    } else {
        diffuse = vec3<f32>(sky * intensity) + keyed;
    }
    let reflection = (0.25 + 0.95 * facing_up * facing_up) * intensity;
    let facing = clamp(dot(normal, viewer), 0.0, 1.0);
    let rim = RIM * pow(1.0 - facing, 3.0) * intensity;
    let dielectric = base.rgb * diffuse + highlight * 0.9;
    let metal = base.rgb * reflection * SKY_TINT + highlight * (0.35 + 0.65 * base.rgb);
    let rgb = dielectric + (metal - dielectric) * metallic
        + rim * SKY_TINT * (0.4 + 0.6 * base.rgb) + draw.emissive.rgb;
    return vec4<f32>(rgb, base.a);
}

@vertex
fn vertex(in: VertexIn) -> VertexOut {
    let world = (draw.model * vec4<f32>(in.position, 1.0)).xyz;
    var out: VertexOut;
    out.clip = image_clip(world);
    out.world = world;
    out.normal = (draw.normal * vec4<f32>(in.normal, 0.0)).xyz;
    out.color = draw.base;
    if draw.material.w > 0.5 {
        out.color = in.color;
    }
    return out;
}

// The linear color of a surface's pixel: lit per pixel when the drawable is.
fn surface(in: VertexOut) -> vec4<f32> {
    let viewer = toward_viewer(in.world);
    // The face's normal, for vertices without one.
    var face = normalize_or_zero(cross(dpdx(in.world), dpdy(in.world)));
    if dot(face, viewer) < 0.0 {
        face = -face;
    }
    var color = in.color;
    if draw.material.z > 0.5 {
        var normal = normalize_or_zero(in.normal);
        if dot(normal, normal) < 0.5 {
            normal = face;
        }
        color = shade(color, normal, viewer, in.world);
    }
    return color;
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    return output(surface(in));
}

@fragment
fn fragment_layer(in: VertexOut, @builtin(sample_index) sample: u32) -> @location(0) vec4<f32> {
    if hidden(in.clip, sample) {
        discard;
    }
    return vec4<f32>(0.0);
}

struct LineIn {
    // First end, and 1 when the ends have their own colors.
    @location(0) start: vec4<f32>,
    @location(1) end: vec4<f32>,
    @location(2) start_color: vec4<f32>,
    @location(3) end_color: vec4<f32>,
}

struct LineOut {
    @invariant @builtin(position) clip: vec4<f32>,
    // Linear color.
    @location(0) @interpolate(linear) color: vec4<f32>,
    // Where the fragment lies, and the segment's ends, in image pixels.
    @location(1) @interpolate(linear) pixel: vec2<f32>,
    @location(2) @interpolate(flat) ends: vec4<f32>,
}

// The corners of a segment's quad: which end, and which side.
const CORNERS = array<vec2<f32>, 6>(
    vec2<f32>(0.0, -1.0),
    vec2<f32>(1.0, -1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, -1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 1.0),
);

@vertex
fn vertex_line(@builtin(vertex_index) index: u32, in: LineIn) -> LineOut {
    var out: LineOut;
    var start = image_clip((draw.model * vec4<f32>(in.start.xyz, 1.0)).xyz);
    var end = image_clip((draw.model * vec4<f32>(in.end.xyz, 1.0)).xyz);
    // The part in front of the near plane, where reversed depth reaches w.
    let start_out = start.z - start.w;
    let end_out = end.z - end.w;
    if start_out > 0.0 && end_out > 0.0 {
        out.clip = vec4<f32>(0.0, 0.0, -1.0, 1.0);
        return out;
    }
    if start_out > 0.0 {
        start = mix(start, end, start_out / (start_out - end_out));
    }
    if end_out > 0.0 {
        end = mix(end, start, end_out / (end_out - start_out));
    }
    let half_size = vec2<f32>(frame.ndc_x.w, frame.ndc_y.w);
    let s0 = start.xy / start.w * half_size;
    let s1 = end.xy / end.w * half_size;
    let span = s1 - s0;
    let span_length = length(span);
    var along = vec2<f32>(1.0, 0.0);
    if span_length > 1e-6 {
        along = span / span_length;
    }
    let across = vec2<f32>(-along.y, along.x);
    // Half the width, and a pixel of antialiasing around it.
    let reach = frame.params.z + 1.0;
    var corners = CORNERS;
    let corner = corners[index];
    var pixel = s0 - along * reach;
    var clip = start;
    var color = in.start_color;
    if corner.x > 0.5 {
        pixel = s1 + along * reach;
        clip = end;
        color = in.end_color;
    }
    if draw.material.w < 0.5 {
        color = draw.base;
    }
    pixel += across * corner.y * reach;
    out.clip = vec4<f32>(pixel / half_size * clip.w, clip.z * (1.0 - LINE_DEPTH_BIAS), clip.w);
    out.pixel = pixel;
    out.ends = vec4<f32>(s0, s1);
    out.color = color;
    return out;
}

struct LineFragment {
    @location(0) color: vec4<f32>,
    // The samples a round-capped segment covers, as many as its coverage.
    @builtin(sample_mask) mask: u32,
}

fn line_fragment(in: LineOut) -> LineFragment {
    let s0 = in.ends.xy;
    let span = in.ends.zw - s0;
    let t = clamp(dot(in.pixel - s0, span) / max(dot(span, span), 1e-12), 0.0, 1.0);
    let distance = length(in.pixel - (s0 + span * t));
    let coverage = clamp(frame.params.z + 0.5 - distance, 0.0, 1.0);
    let samples = u32(round(coverage * 4.0));
    var out: LineFragment;
    out.mask = (1u << samples) - 1u;
    out.color = output(in.color);
    return out;
}

@fragment
fn fragment_line(in: LineOut) -> LineFragment {
    return line_fragment(in);
}

@fragment
fn fragment_line_layer(in: LineOut, @builtin(sample_index) sample: u32) -> LineFragment {
    if hidden(in.clip, sample) {
        discard;
    }
    return line_fragment(in);
}
"#;

#[cfg(not(target_os = "emscripten"))]
pub use canvas::GpuThreeDFrame;
#[cfg(not(target_os = "emscripten"))]
pub(crate) use canvas::{RenderGpuThreeD, build};

/// The interactive canvas's side: the composed frame's meshes reach the
/// render world, which draws them before Vello renders the canvas.
#[cfg(not(target_os = "emscripten"))]
mod canvas {
    use std::sync::{Arc, Mutex, Weak};

    use bevy::prelude::*;
    use bevy::render::renderer::{RenderDevice, RenderQueue};
    use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};

    use super::{GpuMeshFrame, GpuThreeD};
    use crate::canvas::VelloRenderer;

    /// The meshes of the latest composed frame. Present only when a render
    /// world draws them.
    #[derive(Resource, Default)]
    pub struct GpuThreeDFrame(pub(crate) Vec<GpuMeshFrame>);

    #[derive(Resource, Default)]
    struct ExtractedGpuThreeD(Vec<GpuMeshFrame>);

    #[derive(Resource, Default)]
    pub(crate) struct RenderGpuThreeD {
        three_d: GpuThreeD,
        renderer: Weak<Mutex<vello::Renderer>>,
        /// An image changed this frame, so the canvas draws it again.
        pub(crate) changed: bool,
    }

    pub(crate) fn build(app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<ExtractedGpuThreeD>()
            .init_resource::<RenderGpuThreeD>()
            .add_systems(ExtractSchedule, extract_meshes)
            .add_systems(
                Render,
                prepare_meshes
                    .in_set(RenderSystems::PrepareResources)
                    .run_if(resource_exists::<RenderDevice>)
                    .run_if(resource_exists::<VelloRenderer>),
            );
        app.init_resource::<GpuThreeDFrame>();
    }

    fn extract_meshes(
        frame: Extract<Option<Res<GpuThreeDFrame>>>,
        mut extracted: ResMut<ExtractedGpuThreeD>,
    ) {
        extracted.0 = frame
            .as_ref()
            .map(|frame| frame.0.clone())
            .unwrap_or_default();
    }

    fn prepare_meshes(
        extracted: Res<ExtractedGpuThreeD>,
        device: Res<RenderDevice>,
        queue: Res<RenderQueue>,
        vello_renderer: Res<VelloRenderer>,
        mut state: ResMut<RenderGpuThreeD>,
    ) {
        let shared: &Arc<Mutex<vello::Renderer>> = &vello_renderer;
        let Ok(mut renderer) = shared.lock() else {
            return;
        };
        let state = &mut *state;
        if !Weak::ptr_eq(&state.renderer, &Arc::downgrade(shared)) {
            state.three_d.renderer_replaced();
            state.renderer = Arc::downgrade(shared);
        }
        state.changed =
            state
                .three_d
                .prepare(device.wgpu_device(), &queue, &mut renderer, &extracted.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera(projection: Projection) -> Camera {
        let mut camera = Camera::ortho_2d_frame(16.0, 9.0, 1920, 1080);
        camera.projection = projection;
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        camera
    }

    fn quad(color: peniko::Color) -> TriangleMeshData {
        TriangleMeshData {
            vertices: vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            normals: None,
            uvs: None,
            color: Some(color),
            colors: None,
            material: None,
        }
    }

    #[test]
    fn orthographic_projection_keeps_the_canvas_in_scene_units() {
        let (ndc_to_canvas, line_width) =
            canvas_projection(&camera(Projection::Orthographic { zoom: 1.0 })).unwrap();
        // The NDC square spans the 16 x 9 frame around the camera.
        let corner = ndc_to_canvas * kurbo::Point::new(1.0, 1.0);
        assert!((corner.x - 8.0).abs() < 1e-9 && (corner.y - 4.5).abs() < 1e-9);
        // 1.5 output pixels at 120 pixels per unit.
        assert!((line_width - 1.5 / 120.0).abs() < 1e-12);
    }

    #[test]
    fn meshes_keep_their_valid_triangles_with_unit_normals() {
        let mut mesh = quad(peniko::Color::WHITE);
        mesh.indices.extend([0, 1, 9]);
        let source = MeshSource::mesh(&mesh).unwrap();
        assert_eq!(source.count, 6);
        assert_eq!(source.vertices.len(), 4 * MESH_VERTEX_BYTES);
        // The first vertex's normal, generated from the faces, faces +z.
        let normal: Vec<f32> = source.vertices[12..24]
            .chunks(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        assert_eq!(normal, [0.0, 0.0, 1.0]);
        mesh.indices.clear();
        assert!(MeshSource::mesh(&mesh).is_none());
    }

    #[test]
    fn line_lists_pair_their_points_as_strips_indices_or_pairs() {
        let lines = |strip, indices| LineListData {
            points: vec![[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            indices,
            strip,
            color: peniko::Color::WHITE,
            colors: None,
        };
        assert_eq!(MeshSource::lines(&lines(true, None)).unwrap().count, 3);
        assert_eq!(MeshSource::lines(&lines(false, None)).unwrap().count, 2);
        let indexed = lines(false, Some(vec![0, 2, 1, 3, 2, 7]));
        assert_eq!(MeshSource::lines(&indexed).unwrap().count, 2);
        let mut single = lines(true, None);
        single.points.truncate(1);
        assert!(MeshSource::lines(&single).is_none());
    }

    #[test]
    fn uploads_are_kept_until_their_revision_changes() {
        let mesh = quad(peniko::Color::WHITE);
        let mut sources = MeshSources::default();
        let built = |sources: &mut MeshSources, revision| {
            sources
                .get(7, false, revision, None, || MeshSource::mesh(&mesh))
                .unwrap()
        };
        let first = built(&mut sources, 1);
        assert!(Arc::ptr_eq(&first, &built(&mut sources, 1)));
        assert!(!Arc::ptr_eq(&first, &built(&mut sources, 2)));
        sources.end_frame();
        sources.end_frame();
        assert!(sources.sources.is_empty());
    }

    #[test]
    fn classes_draw_into_images_of_their_own_with_translucent_draws_last() {
        let camera = camera(Projection::Orthographic { zoom: 1.0 });
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let transform = GlobalSpatialTransform::default();
        let (opaque, faded) = (GlobalOpacity(1.0), GlobalOpacity(0.5));
        let red = quad(peniko::Color::from_rgb8(255, 0, 0));
        let content = |opacity| Content {
            transform: &transform,
            opacity,
            mesh: Some(&red),
            lines: None,
            material: None,
        };
        let mut sources = MeshSources::default();
        let mut meshes =
            GpuMeshes::new(&mut sources, (1920, 1080), camera.to_vello_transform(), 0).unwrap();
        assert_eq!(
            meshes.push(0, [(1, 1), (1, 1)], [None, None], &content(&faded)),
            (true, false)
        );
        meshes.push(0, [(2, 1), (2, 1)], [None, None], &content(&opaque));
        meshes.push(1, [(3, 1), (3, 1)], [None, None], &content(&opaque));
        let frames = meshes.finish(&camera, ndc_to_canvas, line_width, &Illumination::default());
        assert_eq!(frames.len(), 2);
        assert_ne!(frames[0].1.image.data.id(), frames[1].1.image.data.id());
        let translucent: Vec<bool> = frames[0]
            .1
            .draws
            .iter()
            .map(|draw| draw.translucent)
            .collect();
        assert_eq!(translucent, [false, true]);
    }

    #[test]
    fn a_mesh_draws_into_the_image_the_scene_shows() {
        let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
            return;
        };
        let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let placement = camera.to_vello_transform();
        let transform = GlobalSpatialTransform::default();
        let opacity = GlobalOpacity(1.0);
        let red = quad(peniko::Color::from_rgb8(255, 0, 0));
        let mut sources = MeshSources::default();
        let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
        meshes.push(
            0,
            [(1, 1), (1, 1)],
            [None, None],
            &Content {
                transform: &transform,
                opacity: &opacity,
                mesh: Some(&red),
                lines: None,
                material: None,
            },
        );
        let frames: Vec<GpuMeshFrame> = meshes
            .finish(&camera, ndc_to_canvas, line_width, &Illumination::default())
            .into_iter()
            .map(|(_, frame)| frame)
            .collect();
        let mut scene = vello::Scene::new();
        scene.append(&frames[0].scene(), Some(placement));
        let mut three_d = GpuThreeD::default();
        assert!(three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames));
        // The same frame again keeps the image.
        assert!(!three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames));
        let target = gpu.target(64, 64);
        gpu.render(&scene, &target);
        let pixels = gpu.read(&target);
        let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        // The quad spans the middle half of the frame.
        assert_eq!(pixel(32, 32), [255, 0, 0, 255]);
        assert_eq!(pixel(4, 4)[3], 0);
    }
}
