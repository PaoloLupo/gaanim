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
//! front of it, blends once over what is behind it. Outlines filled in a
//! drawable's plane ([`PlaneFillData`], see [`crate::plane_fill`]) draw from
//! their curves after the opaque draws, blending their edges without
//! writing depth; then they write it where they are at least half covered
//! and opaque, so the translucent layers behind them do not draw over them.

use std::any::Any;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};

use gaanim_core::glam::dcamera::rh::{proj::directx, view};
use gaanim_core::glam::{DMat4, DVec3};
use gaanim_core::{kurbo, peniko};
use gaanim_math::{Camera, GlobalSpatialTransform, Projection};
use gaanim_scene::{
    AlphaMode, GlobalOpacity, Light3D, LightKind3D, Lighting3D, LineListData, Material3D,
    MeshCopies, MeshInstances, MeshTexture, PlaneFillData, TextureWrap, Transparency3D,
    TriangleMeshData,
};
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use vello::wgpu;
use wgpu::util::DeviceExt;

use crate::effects::{DropShadow, GaussianBlur, Glow};

mod focus;
mod glow;
mod peel;

/// A light as shading uses it: where it is in the world and its linear
/// color, scaled by its intensity and its entity's opacity, and a spot
/// light's cone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Light {
    pub position: DVec3,
    pub directional: bool,
    pub color: [f32; 3],
    pub spot: Option<Spot>,
}

/// The cone a spot light shines in: its direction, a unit vector, and the
/// cosines of the angles from it within which it shines fully and beyond
/// which it gives no light.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spot {
    pub direction: DVec3,
    pub cos_inner: f32,
    pub cos_outer: f32,
}

impl Spot {
    /// The cone `cone` gives a spot at `position`; `None` when it aims at
    /// itself.
    fn new(cone: &gaanim_scene::SpotCone, position: DVec3) -> Option<Self> {
        let direction = (cone.target - position).try_normalize()?;
        let outer = cone.angle.clamp(1e-3, gaanim_scene::SpotCone::MAX_ANGLE);
        let inner = outer * (1.0 - cone.softness.clamp(0.0, 1.0));
        Some(Self {
            direction,
            cos_inner: inner.cos(),
            cos_outer: outer.cos(),
        })
    }
}

impl Light {
    /// The light `light` gives from `position`, dimmed to `opacity`; `None`
    /// when it gives none.
    pub fn new(light: &Light3D, position: DVec3, opacity: f32) -> Option<Self> {
        let strength = light.intensity.max(0.0) * opacity.clamp(0.0, 1.0);
        let [r, g, b, a] = to_linear(light.color);
        let color = [r, g, b].map(|channel| channel * a * strength);
        let spot = match &light.kind {
            LightKind3D::Spot(cone) => Some(Spot::new(cone, position)?),
            _ => None,
        };
        (strength > 0.0 && position.is_finite()).then_some(Self {
            position,
            directional: light.kind == LightKind3D::Directional,
            color,
            spot,
        })
    }
}

/// How lit 3D content is lit in a frame: the scene's [`Lighting3D`] and the
/// lights it shows; how its translucent content blends; and the camera's
/// depth of field, which blurs it by depth (see [`focus`]).
#[derive(Debug, Clone, Default)]
pub struct Illumination {
    pub lighting: Lighting3D,
    pub lights: Vec<Light>,
    pub transparency: Transparency3D,
    pub depth_of_field: gaanim_scene::DepthOfField,
}

/// One drawable's 3D content, as the ECS holds it.
pub(crate) struct Content<'a> {
    pub transform: &'a GlobalSpatialTransform,
    pub opacity: &'a GlobalOpacity,
    pub mesh: Option<&'a TriangleMeshData>,
    pub lines: Option<&'a LineListData>,
    pub material: Option<&'a Material3D>,
    /// Width of the lines in scene units, from the drawable's stroke; `None`
    /// draws them [`LINE_WIDTH_OF_FRAME`] of the frame's height wide.
    pub line_width: Option<f64>,
    /// The part of the lines shown, from and to, as fractions of their
    /// segments; `None` shows all of them.
    pub line_range: Option<[f32; 2]>,
    /// Outlines filled in the drawable's plane (see [`crate::plane_fill`]).
    pub fill: Option<&'a PlaneFillData>,
    /// The matrices of a skinned mesh's joints (see
    /// [`gaanim_scene::SkinPalette`]).
    pub joints: Option<&'a Arc<[[f32; 16]]>>,
    /// The glow of the drawable or of the group that holds it (see
    /// [`glow`]).
    pub glow: Option<Glow>,
    /// Its drop shadow, found as its glow is.
    pub shadow: Option<DropShadow>,
    /// Its blur, found as its glow is.
    pub blur: Option<GaussianBlur>,
    /// Copies of the mesh drawn instead of it, one at each point (see
    /// [`gaanim_scene::MeshInstances`]).
    pub instances: Option<&'a MeshInstances>,
    /// The weights of an imported mesh's morph targets (see
    /// [`gaanim_scene::MorphWeights`]).
    pub morph: Option<&'a Arc<[f32]>>,
}

/// Width of 3D lines without a stroke width, as a fraction of the frame's
/// height: 2 pixels of a 1080-pixel-tall output, at any output size.
const LINE_WIDTH_OF_FRAME: f64 = 2.0 / 1080.0;

/// How `camera` maps its NDC onto the canvas, in scene units, and the width
/// of lines without a stroke width in scene units; `None` when it shows
/// nothing.
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
        camera.frame_height * LINE_WIDTH_OF_FRAME,
    ))
}

/// Bytes of a mesh vertex: position, normal and linear color.
const MESH_VERTEX_BYTES: usize = 40;
/// Bytes of a line segment: both ends (the first with a vertex-color flag)
/// and their linear colors.
const SEGMENT_BYTES: usize = 64;
/// Bytes of a shadow map layer's view-projection (`Caster` in [`SHADER`]),
/// at `DRAW_STRIDE` from one layer to the next.
const CASTER_BYTES: usize = 64;
/// Bytes of a draw's instance data: its model matrix, the columns of its
/// normal matrix and its linear base color (`VertexIn` in [`SHADER`]).
const INSTANCE_BYTES: usize = 128;
/// Bytes of the frame uniforms (`Frame` in [`SHADER`]).
const FRAME_BYTES: usize = 2592;
/// Bytes of one draw's uniforms (`Draw` in [`SHADER`]), and their stride in
/// the draw buffer, the minimum uniform offset alignment.
const DRAW_BYTES: usize = 240;
/// Bytes per vertex of what an imported model's primitive adds: its UV,
/// four joints and their weights (`ModelIn` in [`MODEL_SHADER`]).
const MODEL_VERTEX_BYTES: usize = 32;
/// Bytes of a skin's palette as a draw binds it: the most joints a skin
/// moves, a matrix each (`joints` in [`MODEL_SHADER`]).
const JOINTS_BYTES: usize = gaanim_scene::model::MAX_JOINTS * 64;
/// Most bytes of what a model's vertices read by index (see
/// [`model_geometry`]): the storage binding size every device allows.
const MAX_GEOMETRY_BYTES: usize = 128 << 20;
/// Where a draw's uniforms keep its lines' half width and shown segments
/// (`Draw.line` in [`SHADER`]).
#[cfg(test)]
const LINE_ROW: usize = 176;
const DRAW_STRIDE: usize = 256;
/// Lights the shader reads; further lights are left out.
const MAX_LIGHTS: usize = 8;
/// Lights that cast shadows: the first ones of a frame.
const MAX_SHADOW_LIGHTS: usize = 4;
/// Layers of the shadow maps: one for a directional light, one per cube
/// face for a point light.
const MAX_SHADOW_LAYERS: usize = MAX_SHADOW_LIGHTS * 6;
/// Texels on a side of the shadow maps: of the only one of a directional
/// light, of up to two lights' maps, and of more.
const SHADOW_SIZE: u32 = 2048;
const SHADOW_SIZE_SHARED: u32 = 1024;
const SHADOW_SIZE_MANY: u32 = 512;
/// Radius of the edge of shadows where an object touches what it shadows,
/// in shadow-map texels; it widens with the distance between them
/// (`shadowed` in [`SHADER`]).
const SHADOW_CONTACT: f32 = 1.0;
/// Where the fixed key light of [`Lighting3D`] shines from (`KEY_LIGHT` in
/// [`SHADER`]).
const KEY_LIGHT: DVec3 = DVec3::new(-3.0, 6.0, 4.0);
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
    /// A lit mesh that encloses a volume: every edge joins two triangles
    /// that run it in opposite directions, once vertices at one place are
    /// one. Only a lit mesh shadows itself, so only a lit one is checked.
    closed: bool,
    /// A lit mesh that bounds no volume: an open one, or both sides of a
    /// sheet, which is closed but holds nothing.
    sheet: bool,
    /// A lit mesh's outline directions (see [`outline_directions`]); empty
    /// for others, which have no material to outline them.
    outline: Vec<u8>,
    /// Hash of what the shader reads: sources alike share one upload, and
    /// meshes alike draw next to each other, as one instanced draw.
    digest: u64,
    /// Filled outlines: `vertices` holds an instance per outline, and these
    /// the curves and bands their fragments read.
    fill: Option<FillBuffers>,
    /// An imported model's primitive: its texture, alpha and skin, drawn by
    /// pipelines of their own ([`ModelPipelines`]).
    model: Option<ModelBuffers>,
}

/// What an imported model's primitive adds to its vertices (see
/// [`gaanim_scene::MeshSurface`]).
struct ModelBuffers {
    /// [`MODEL_VERTEX_BYTES`] per vertex: UV, joints and weights.
    vertices: Vec<u8>,
    texture: Option<MeshTexture>,
    /// A digest of the texture's size, sampling and texels, computed once,
    /// which frame digests take instead of the texels.
    texture_digest: u64,
    /// Whether every texel of the texture is opaque.
    opaque_texture: bool,
    /// The normal map's texels, read in linear light, and their digest.
    normal_map: Option<MeshTexture>,
    normal_digest: u64,
    /// What its vertices read by index (see [`model_geometry`]): tangents
    /// for the normal map and the morph targets' displacements; empty
    /// without either. And its digest, computed once.
    geometry: Vec<u8>,
    geometry_digest: u64,
    /// How far each morph target moves a vertex at most, at a weight of one.
    reach: Vec<f32>,
    alpha: AlphaMode,
    double_sided: bool,
    skinned: bool,
}

impl ModelBuffers {
    /// The alpha below which a pixel is cut away, 0 when none is.
    fn cutoff(&self) -> f32 {
        match self.alpha {
            AlphaMode::Mask(cutoff) => cutoff.max(1e-6),
            _ => 0.0,
        }
    }
}

/// The curves and bands of a source's filled outlines, and their gradients'
/// records and ramps (see [`crate::plane_fill`]).
struct FillBuffers {
    curves: Vec<u8>,
    bands: Vec<u8>,
    paints: Vec<u8>,
    ramps: Vec<u8>,
    /// Runs of instances and the image they are filled with, bound with
    /// the buffers for their draw.
    runs: Vec<(std::ops::Range<u32>, Option<peniko::ImageBrush>)>,
}

impl FillBuffers {
    /// What tells the runs' images apart: their pixels and sampling.
    fn run_keys(&self) -> impl Iterator<Item = (u32, u32, u64, u8)> + '_ {
        self.runs.iter().map(|(range, brush)| {
            let (id, quality) = brush.as_ref().map_or((0, 0), |brush| {
                (brush.image.data.id(), brush.sampler.quality as u8)
            });
            (range.start, range.end, id, quality)
        })
    }
}

/// What a source holds, which keys a drawable's uploads apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    Mesh,
    /// A mesh drawn with an outline, whose source holds the directions the
    /// outline pushes its vertices out along.
    OutlinedMesh,
    Lines,
    Fill,
}

impl MeshSource {
    /// The hash of what the shader reads of the source.
    fn content_digest(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.vertices.hash(&mut hasher);
        self.indices.hash(&mut hasher);
        (self.lines, self.has_colors, self.closed, self.sheet).hash(&mut hasher);
        self.outline.is_empty().hash(&mut hasher);
        if let Some(model) = &self.model {
            model.vertices.hash(&mut hasher);
            model
                .texture
                .as_ref()
                .map(|texture| (texture.image.data.id(), texture.wrap, texture.nearest))
                .hash(&mut hasher);
            (model.double_sided, model.skinned).hash(&mut hasher);
            model.cutoff().to_bits().hash(&mut hasher);
            if model.normal_map.is_some() || !model.geometry.is_empty() {
                (model.normal_digest, model.geometry_digest).hash(&mut hasher);
            }
        }
        if let Some(fill) = &self.fill {
            fill.curves.hash(&mut hasher);
            fill.bands.hash(&mut hasher);
            fill.paints.hash(&mut hasher);
            fill.ramps.hash(&mut hasher);
            for key in fill.run_keys() {
                key.hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    /// Whether it is a triangle mesh, neither lines nor filled outlines.
    fn is_mesh(&self) -> bool {
        !self.lines && self.fill.is_none()
    }

    /// Whether its vertices move with a skin's joints.
    fn skinned(&self) -> bool {
        self.model.as_ref().is_some_and(|model| model.skinned)
    }

    /// Whether `other` draws exactly what this source does.
    fn same_as(&self, other: &MeshSource) -> bool {
        self.digest == other.digest
            && self.lines == other.lines
            && self.has_colors == other.has_colors
            && self.closed == other.closed
            && self.sheet == other.sheet
            && self.outline.is_empty() == other.outline.is_empty()
            && self.vertices == other.vertices
            && self.indices == other.indices
            && match (&self.fill, &other.fill) {
                (None, None) => true,
                (Some(a), Some(b)) => a.curves == b.curves && a.bands == b.bands,
                _ => false,
            }
            && match (&self.model, &other.model) {
                (None, None) => true,
                (Some(a), Some(b)) => {
                    a.vertices == b.vertices
                        && a.texture == b.texture
                        && a.normal_map == b.normal_map
                        && a.geometry == b.geometry
                        && a.alpha == b.alpha
                        && a.double_sided == b.double_sided
                        && a.skinned == b.skinned
                }
                _ => false,
            }
    }
}

/// Whether the triangles `indices` over `positions` enclose a volume: each
/// edge, once vertices at one place are one, is run once in each direction.
#[cfg(test)]
fn encloses(positions: &[DVec3], indices: &[u32], radius: f64) -> bool {
    encloses_places(&weld(positions, radius), indices)
}

/// A map hashed for speed rather than against collisions an attacker
/// chooses, for the maps of a mesh's places and edges, which change with
/// the mesh and are rebuilt whenever it does.
type FastMap<K, V> = bevy::platform::collections::HashMap<K, V>;

/// The place of each of `positions`, the same for vertices at one place: those
/// closer than a hundred-thousandth of `radius`, found in their cell of a
/// grid that fine or in a neighboring one, since two copies of a vertex
/// computed with different rounding may straddle a cell boundary.
fn weld(positions: &[DVec3], radius: f64) -> Vec<u32> {
    let step = (radius * 1e-5).max(f64::MIN_POSITIVE);
    let mut cells: FastMap<[i64; 3], u32> = FastMap::with_capacity(positions.len());
    let mut places: Vec<DVec3> = Vec::with_capacity(positions.len());
    let place: Vec<u32> = positions
        .iter()
        .map(|p| {
            let cell = [p.x, p.y, p.z].map(|value| (value / step).floor() as i64);
            let near = (-1..=1)
                .flat_map(|x| (-1..=1).flat_map(move |y| (-1..=1).map(move |z| [x, y, z])))
                .filter_map(|[x, y, z]| cells.get(&[cell[0] + x, cell[1] + y, cell[2] + z]))
                .find(|&&id| places[id as usize].distance(*p) <= step)
                .copied();
            near.unwrap_or_else(|| {
                let id = places.len() as u32;
                places.push(*p);
                cells.entry(cell).or_insert(id);
                id
            })
        })
        .collect();
    place
}

/// Whether the triangles `indices` over vertices at `place`s enclose a
/// volume (see [`encloses`]).
fn encloses_places(place: &[u32], indices: &[u32]) -> bool {
    // Each edge counts +1 one way and -1 the other.
    let mut edges: FastMap<(u32, u32), i32> = FastMap::with_capacity(indices.len());
    for triangle in indices.as_chunks::<3>().0 {
        let [a, b, c] = triangle.map(|index| place[index as usize]);
        for (from, to) in [(a, b), (b, c), (c, a)] {
            if from != to {
                *edges.entry((from.min(to), from.max(to))).or_insert(0) +=
                    if from < to { 1 } else { -1 };
            }
        }
    }
    !edges.is_empty() && edges.values().all(|count| *count == 0)
}

/// Whether the closed triangles `indices` over `positions` hold a volume,
/// rather than run both sides of a sheet, whose volumes cancel: more than a
/// millionth of the cube of `radius`, the bounding sphere's about `center`.
fn holds_volume(positions: &[DVec3], indices: &[u32], center: DVec3, radius: f64) -> bool {
    let volume: f64 = indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|triangle| {
            let [a, b, c] = triangle.map(|index| positions[index as usize] - center);
            a.dot(b.cross(c))
        })
        .sum::<f64>()
        / 6.0;
    volume.abs() > 1e-6 * radius.powi(3)
}

/// The way an outline pushes each vertex out, 12 bytes a vertex. A closed
/// mesh, or a two-sided sheet, pushes out along the sum of the normals of
/// the faces at its place on its side (all of a cube corner's faces, but not
/// the other copy of the sheet), so that the outline of a mesh with hard
/// edges stays whole at its corners, plus, along a sheet's border, out of
/// the face within its plane. An open mesh, a plane's, pushes out only its
/// border, within its plane: pushed along its normal, a mesh seen at an
/// angle would slide over itself.
fn outline_directions(
    positions: &[DVec3],
    indices: &[u32],
    place: &[u32],
    closed: bool,
) -> Vec<u8> {
    let triangles = indices.as_chunks::<3>().0;
    let faces: Vec<DVec3> = triangles
        .iter()
        .map(|triangle| {
            let [a, b, c] = triangle.map(|index| positions[index as usize]);
            (b - a).cross(c - a).normalize_or_zero()
        })
        .collect();
    // Each vertex's own normal, from the faces it belongs to.
    let mut own = vec![DVec3::ZERO; positions.len()];
    // The faces at each place, and the faces along each edge between places.
    let mut at_place: FastMap<u32, Vec<usize>> = FastMap::default();
    let mut along: FastMap<(u32, u32), Vec<usize>> = FastMap::default();
    for (face, triangle) in triangles.iter().enumerate() {
        for corner in 0..3 {
            let index = triangle[corner] as usize;
            own[index] += faces[face];
            let here = at_place.entry(place[index]).or_default();
            if here.last() != Some(&face) {
                here.push(face);
            }
            let [from, to] = [place[index], place[triangle[(corner + 1) % 3] as usize]];
            along
                .entry((from.min(to), from.max(to)))
                .or_default()
                .push(face);
        }
    }
    let side = |a: DVec3, b: DVec3| a.dot(b) > -0.5;
    let mut directions = vec![DVec3::ZERO; positions.len()];
    let mut borders = vec![DVec3::ZERO; positions.len()];
    for (index, direction) in directions.iter_mut().enumerate() {
        let normal = own[index].normalize_or_zero();
        for &face in at_place.get(&place[index]).map_or(&[][..], Vec::as_slice) {
            if side(faces[face], normal) {
                *direction += faces[face];
            }
        }
    }
    for (face, triangle) in triangles.iter().enumerate() {
        for corner in 0..3 {
            let [a, b] = [
                triangle[corner] as usize,
                triangle[(corner + 1) % 3] as usize,
            ];
            let [from, to] = [place[a], place[b]];
            let others = along[&(from.min(to), from.max(to))]
                .iter()
                .filter(|&&other| other != face && side(faces[other], faces[face]))
                .count();
            if others == 0 {
                // Counter-clockwise, the face lies left of a to b.
                let out = (positions[b] - positions[a])
                    .cross(faces[face])
                    .normalize_or_zero();
                borders[a] += out;
                borders[b] += out;
            }
        }
    }
    // A unit push along the faces' normal, and a unit push out of a border
    // within the faces' plane: an outline is as wide beyond a border as
    // beyond a silhouette.
    let mut bytes = Vec::with_capacity(positions.len() * 12);
    for (direction, border) in directions.into_iter().zip(borders) {
        let along = if closed {
            direction.normalize_or_zero()
        } else {
            DVec3::ZERO
        };
        let direction = along + border.normalize_or_zero();
        push_f32s(
            &mut bytes,
            &[direction.x as f32, direction.y as f32, direction.z as f32],
        );
    }
    bytes
}

impl MeshSource {
    /// `None` when `mesh` has no triangle; whether it is closed is checked
    /// when it is `lit`, and the directions its outline pushes it out along
    /// are found when it is `outlined` too.
    fn mesh(mesh: &TriangleMeshData, lit: bool, outlined: bool) -> Option<Self> {
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
        let place = lit.then(|| weld(&positions, radius));
        let closed = place
            .as_ref()
            .is_some_and(|place| encloses_places(place, &indices));
        let sheet = lit && !(closed && holds_volume(&positions, &indices, center, radius));
        let model = mesh
            .surface
            .as_deref()
            .map(|surface| model_buffers(mesh, surface));
        Some(Self {
            vertices,
            count: indices.len() as u32,
            closed,
            sheet,
            // A model's outline would move with its skin: models draw none.
            outline: place
                .as_ref()
                .filter(|_| outlined && model.is_none())
                .map(|place| outline_directions(&positions, &indices, place, closed))
                .unwrap_or_default(),
            digest: 0,
            fill: None,
            model,
            indices,
            lines: false,
            has_colors: colors.is_some(),
            opaque_colors: colors.is_none_or(|colors| colors.iter().all(|c| c[3] >= 0.999)),
            center,
            radius,
        })
    }

    /// The outlines of `fill`, an instance each (see [`crate::plane_fill`]);
    /// `None` when none encloses anything.
    fn fill(fill: &PlaneFillData) -> Option<Self> {
        let built = crate::plane_fill::build(fill)?;
        let (center, radius) = bounding_sphere(&built.corners);
        Some(Self {
            vertices: built.instances,
            indices: Vec::new(),
            count: built.count,
            lines: false,
            has_colors: true,
            opaque_colors: built.opaque,
            center,
            radius,
            closed: false,
            sheet: false,
            outline: Vec::new(),
            digest: 0,
            model: None,
            fill: Some(FillBuffers {
                curves: built.curves,
                bands: built.bands,
                paints: built.paints,
                ramps: built.ramps,
                runs: built.runs,
            }),
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
            closed: false,
            sheet: false,
            outline: Vec::new(),
            digest: 0,
            fill: None,
            model: None,
            has_colors: colors.is_some(),
            opaque_colors: colors.is_none_or(|colors| colors.iter().all(|c| c[3] >= 0.999)),
            center,
            radius,
        })
    }
}

/// The UVs, joints and weights of `mesh`'s vertices, with what else its
/// `surface` draws.
fn model_buffers(mesh: &TriangleMeshData, surface: &gaanim_scene::MeshSurface) -> ModelBuffers {
    let count = mesh.vertices.len();
    let uvs = mesh.uvs.as_ref().filter(|uvs| uvs.len() == count);
    let skin = surface
        .skin
        .as_ref()
        .filter(|skin| skin.joints.len() == count && skin.weights.len() == count);
    let last = (gaanim_scene::model::MAX_JOINTS - 1) as u16;
    let mut vertices = Vec::with_capacity(count * MODEL_VERTEX_BYTES);
    for index in 0..count {
        push_f32s(&mut vertices, &uvs.map_or([0.0; 2], |uvs| uvs[index]));
        let joints = skin.map_or([0; 4], |skin| skin.joints[index]);
        for joint in joints {
            vertices.extend_from_slice(&joint.min(last).to_le_bytes());
        }
        push_f32s(
            &mut vertices,
            &skin.map_or([0.0; 4], |skin| skin.weights[index]),
        );
    }
    let texture = surface.texture.clone().filter(|_| uvs.is_some());
    let opaque_texture = texture.as_ref().is_none_or(|texture| {
        texture
            .image
            .data
            .data()
            .chunks_exact(4)
            .all(|texel| texel[3] == 255)
    });
    let digest = texture.as_ref().map_or(0, texture_digest);
    let normal_map = surface
        .normal_map
        .as_ref()
        .filter(|map| uvs.is_some() && map.tangents.len() == count);
    let (geometry, reach) = model_geometry(surface, normal_map, count);
    ModelBuffers {
        vertices,
        texture,
        texture_digest: digest,
        opaque_texture,
        normal_digest: normal_map.map_or(0, |map| texture_digest(&map.texture)),
        normal_map: normal_map.map(|map| map.texture.clone()),
        geometry_digest: fnv(FNV_START, &geometry),
        geometry,
        reach,
        alpha: surface.alpha,
        double_sided: surface.double_sided,
        skinned: skin.is_some(),
    }
}

/// Where FNV-1a starts.
const FNV_START: u64 = 0xcbf2_9ce4_8422_2325;

/// `digest` with `bytes` taken in by FNV-1a, which is stable across builds,
/// as the digests recordings keep.
fn fnv(mut digest: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        digest = (digest ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
    }
    digest
}

/// A digest of a texture's size, sampling and texels.
fn texture_digest(texture: &MeshTexture) -> u64 {
    let mut digest = fnv(FNV_START, &texture.image.width.to_le_bytes());
    digest = fnv(digest, &texture.image.height.to_le_bytes());
    digest = fnv(
        digest,
        &[
            texture.nearest as u8,
            texture.wrap[0] as u8,
            texture.wrap[1] as u8,
        ],
    );
    fnv(digest, texture.image.data.data())
}

/// What the vertex shader of a model with `count` vertices reads by vertex
/// index (`geometry` in [`MODEL_SHADER`]), and how far each of its morph
/// targets moves a vertex at most: a header, holding `normal_map`'s scale,
/// 1 when tangents follow, the vertices and the targets; a tangent per
/// vertex for `normal_map`; then, for each target and vertex, its
/// displacement and the change of its normal, a row each. Empty with
/// neither. Targets past [`MAX_GEOMETRY_BYTES`] are left out.
fn model_geometry(
    surface: &gaanim_scene::MeshSurface,
    normal_map: Option<&gaanim_scene::NormalMap>,
    count: usize,
) -> (Vec<u8>, Vec<f32>) {
    let targets: Vec<&gaanim_scene::MorphTarget> = surface
        .morph_targets
        .iter()
        .filter(|target| target.positions.len() == count)
        .collect();
    if (normal_map.is_none() && targets.is_empty()) || count == 0 {
        return (Vec::new(), Vec::new());
    }
    let row = 16;
    let fixed = row * (1 + normal_map.map_or(0, |_| count));
    let room = MAX_GEOMETRY_BYTES.saturating_sub(fixed) / (count * 2 * row);
    if targets.len() > room {
        tracing::warn!(
            "a mesh of {count} vertices draws the first {room} of its {} morph targets",
            targets.len()
        );
    }
    let targets = &targets[..targets.len().min(room)];
    let mut bytes = Vec::with_capacity(fixed + targets.len() * count * 2 * row);
    push_f32s(
        &mut bytes,
        &[
            normal_map.map_or(0.0, |map| map.scale),
            f32::from(u8::from(normal_map.is_some())),
            count as f32,
            targets.len() as f32,
        ],
    );
    if let Some(map) = normal_map {
        for tangent in &map.tangents {
            push_f32s(&mut bytes, tangent);
        }
    }
    let mut reach = Vec::with_capacity(targets.len());
    for target in targets {
        let mut farthest = 0.0_f32;
        for (vertex, moved) in target.positions.iter().enumerate() {
            let moved = moved.map(|value| if value.is_finite() { value } else { 0.0 });
            farthest = farthest
                .max((moved[0] * moved[0] + moved[1] * moved[1] + moved[2] * moved[2]).sqrt());
            let turned = target
                .normals
                .as_ref()
                .and_then(|normals| normals.get(vertex))
                .map_or([0.0; 3], |turned| {
                    turned.map(|value| if value.is_finite() { value } else { 0.0 })
                });
            push_f32s(&mut bytes, &[moved[0], moved[1], moved[2], 0.0]);
            push_f32s(&mut bytes, &[turned[0], turned[1], turned[2], 0.0]);
        }
        reach.push(farthest);
    }
    (bytes, reach)
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

pub(crate) fn push_f32s(bytes: &mut Vec<u8>, values: &[f32]) {
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
    /// By key and what it holds.
    sources: HashMap<(u64, Kind), Held>,
    used: HashSet<(u64, Kind)>,
    /// Sources by digest, so that drawables alike share one.
    shared: HashMap<u64, std::sync::Weak<MeshSource>>,
}

struct Held {
    revision: u32,
    source: Option<Arc<MeshSource>>,
    _keep: Option<Keep>,
}

impl MeshSources {
    /// The source of the `kind` of content keyed `key` at `revision`, built
    /// by `build` when it changed.
    fn get(
        &mut self,
        key: u64,
        kind: Kind,
        revision: u32,
        keep: Option<Keep>,
        build: impl FnOnce() -> Option<MeshSource>,
    ) -> Option<Arc<MeshSource>> {
        self.used.insert((key, kind));
        let stale = self
            .sources
            .get(&(key, kind))
            .is_none_or(|held| held.revision != revision);
        if stale {
            let source = build().map(|source| self.share(source));
            self.sources.insert(
                (key, kind),
                Held {
                    revision,
                    source,
                    _keep: keep,
                },
            );
        }
        self.sources.get(&(key, kind))?.source.clone()
    }

    /// `source`, or the one alike that another drawable already holds.
    fn share(&mut self, mut source: MeshSource) -> Arc<MeshSource> {
        source.digest = source.content_digest();
        match self.shared.entry(source.digest) {
            Entry::Occupied(mut entry) => match entry.get().upgrade() {
                Some(held) if held.same_as(&source) => held,
                Some(_) => Arc::new(source),
                None => {
                    let source = Arc::new(source);
                    entry.insert(Arc::downgrade(&source));
                    source
                }
            },
            Entry::Vacant(entry) => {
                let source = Arc::new(source);
                entry.insert(Arc::downgrade(&source));
                source
            }
        }
    }

    /// Forget the meshes and lines this frame did not draw.
    pub(crate) fn end_frame(&mut self) {
        let used = std::mem::take(&mut self.used);
        self.sources.retain(|key, _| used.contains(key));
        self.shared.retain(|_, source| source.strong_count() > 0);
    }
}

/// The image the meshes of a frame's `slot` draw into: one per size and
/// slot, so that frames refresh the same slots of Vello's atlas. The least
/// recently drawn go first when camera views and snapshots take many.
fn frame_image(width: u32, height: u32, slot: usize) -> ImageData {
    static IMAGES: Mutex<Vec<(usize, ImageData)>> = Mutex::new(Vec::new());
    let mut images = IMAGES.lock().expect("3D image cache poisoned");
    if let Some(index) = images
        .iter()
        .position(|(held, image)| *held == slot && image.width == width && image.height == height)
    {
        let held = images.remove(index);
        let image = held.1.clone();
        images.push(held);
        return image;
    }
    if images.len() >= 32 {
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
    /// Drawn into the shadow maps: opaque meshes that do not glow, since
    /// a glowing mesh may hold the light that makes it glow.
    casts: bool,
    /// Lines only: the segments drawn, those a reveal shows at least part of;
    /// filled outlines: all of them.
    instances: std::ops::Range<u32>,
    /// Meshes only: the records of the frame's instances it draws, one, or
    /// one per copy shown of a mesh drawn at many points.
    copies: std::ops::Range<u32>,
    /// An opaque mesh whose material draws an outline.
    outlined: bool,
    /// A skinned mesh: where its palette starts in the frame's joints.
    joints: Option<u32>,
    /// A morphed mesh: where its run of targets and weights starts among
    /// those in the frame's joints, and how many it holds.
    morph: Option<(u32, u32)>,
    /// A glowing mesh or line: its glow's blur and gain.
    glow: Option<glow::GlowDraw>,
    /// One that casts a drop shadow: its blur, gain and offset.
    shadow: Option<glow::GlowDraw>,
    /// A blurred one: its blur's sigma in pixels, as bits. It draws apart
    /// from the others, blurred, over them (see [`glow::draw_blurs`]).
    blur: Option<u32>,
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
    /// `INSTANCE_BYTES` per draw: what differs between meshes drawn as one.
    instances: Arc<[u8]>,
    /// Texels on a side of the shadow maps, and the view-projection of each
    /// layer, which the opaque meshes are drawn into before the frame.
    shadow_size: u32,
    shadow_layers: Arc<[[f32; 16]]>,
    /// The palettes of the skinned draws, each [`JOINTS_BYTES`] apart at
    /// most, aligned for the dynamic offsets that bind them, and between
    /// them the morph targets and weights of the morphed draws.
    joints: Arc<[u8]>,
    /// The translucent surfaces each pixel blends in depth order (see
    /// [`peel`]), 0 when translucent draws blend as layers from back to
    /// front.
    peel: u8,
    /// How the image blurs by depth, when it is out of focus.
    focus: Option<focus::FocusDraw>,
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
        update(&self.instances);
        for draw in self.draws.iter() {
            update(&[
                u8::from(draw.culled),
                u8::from(draw.translucent),
                u8::from(draw.casts),
                u8::from(draw.outlined),
            ]);
            if draw.glow.is_some() || draw.shadow.is_some() || draw.blur.is_some() {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                (draw.glow, draw.shadow, draw.blur).hash(&mut hasher);
                update(&hasher.finish().to_le_bytes());
            }
            update(&draw.source.vertices);
            for index in &draw.source.indices {
                update(&index.to_le_bytes());
            }
            if let Some(model) = &draw.source.model {
                update(&model.vertices);
                update(&model.texture_digest.to_le_bytes());
                // Normal maps and morph targets, when there are any.
                if model.normal_map.is_some() || !model.geometry.is_empty() {
                    update(&model.normal_digest.to_le_bytes());
                    update(&model.geometry_digest.to_le_bytes());
                }
            }
            if let Some(fill) = &draw.source.fill {
                update(&fill.curves);
                update(&fill.bands);
                update(&fill.paints);
                update(&fill.ramps);
                for (start, end, id, quality) in fill.run_keys() {
                    update(&start.to_le_bytes());
                    update(&end.to_le_bytes());
                    update(&id.to_le_bytes());
                    update(&[quality]);
                }
            }
        }
        update(&self.joints);
        update(&[self.peel]);
        if let Some(focus) = &self.focus {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            focus.hash(&mut hasher);
            update(&hasher.finish().to_le_bytes());
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
    /// Lines only: their width in scene units, or the default.
    line_width: Option<f64>,
    /// Lines only: the part shown, as fractions of their segments.
    line_range: Option<[f32; 2]>,
    /// A skinned mesh's palette.
    joints: Option<Arc<[[f32; 16]]>>,
    /// A morphed mesh's targets and their weights, those it weighs.
    morph: Option<Vec<[f32; 2]>>,
    /// The bounding sphere of what it draws, in model space: a skinned
    /// mesh's spans where its joints take it.
    sphere: (DVec3, f64),
    /// Its glow, drop shadow and blur.
    glow: Option<Glow>,
    shadow: Option<DropShadow>,
    blur: Option<GaussianBlur>,
    /// A mesh drawn at many points: its copies and how far they have grown
    /// in.
    copies: Option<(Arc<MeshCopies>, f32)>,
}

impl Pending {
    /// Whether it is drawn into the shadow maps: an opaque mesh, or filled
    /// outlines, whose texels test their own opacity, that do not glow,
    /// since a glowing mesh may hold the light that makes it glow.
    fn casts(&self) -> bool {
        let glows = self.material.is_some_and(|material| {
            material.emissive_strength > 0.0
                && material.emissive.components[..3]
                    .iter()
                    .any(|value| *value > 0.0)
                && material.emissive.components[3] > 0.0
        });
        !glows && (self.source.fill.is_some() || (!self.translucent && self.source.is_mesh()))
    }
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
    /// Whether [`GpuMeshes::finish`] ends the frame of `sources`, forgetting
    /// what it did not draw: not for one of several batches a frame draws
    /// from them.
    ends_frame: bool,
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
            ends_frame: true,
        })
    }

    /// These meshes as one of several batches the frame draws from the
    /// same sources, before the last, which ends the frame for all of them.
    pub(crate) fn within_frame(mut self) -> Self {
        self.ends_frame = false;
        self
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
        // Only an outlined mesh's source finds its outline's directions.
        let outlined = content.material.is_some_and(|material| material.outlined());
        let kind = if outlined {
            Kind::OutlinedMesh
        } else {
            Kind::Mesh
        };
        if let Some(mesh) = content.mesh
            && let Some(source) = self
                .sources
                .get(mesh_key.0, kind, mesh_key.1, mesh_keep, || {
                    MeshSource::mesh(mesh, content.material.is_some(), outlined)
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
                None,
            );
            taken.0 = true;
        }
        if let Some(lines) = content.lines
            && let Some(source) =
                self.sources
                    .get(lines_key.0, Kind::Lines, lines_key.1, lines_keep, || {
                        MeshSource::lines(lines)
                    })
        {
            let width = content
                .line_width
                .filter(|width| width.is_finite() && *width > 0.0);
            self.add(class, source, content, opacity, lines.color, None, width);
            taken.1 = true;
        }
        taken
    }

    /// Take the filled outlines of a drawable into the image of `class`,
    /// keyed as [`Self::push`] keys meshes; returns whether the GPU took them.
    pub(crate) fn push_fill(
        &mut self,
        class: usize,
        (key, revision): (u64, u32),
        keep: Option<Keep>,
        content: &Content<'_>,
    ) -> bool {
        let Some(fill) = content.fill else {
            return false;
        };
        let Some(source) = self
            .sources
            .get(key, Kind::Fill, revision, keep, || MeshSource::fill(fill))
        else {
            return false;
        };
        let opacity = content.opacity.0.clamp(0.0, 1.0);
        let base = content
            .material
            .map_or(peniko::Color::WHITE, |material| material.color);
        self.add(
            class,
            source,
            content,
            opacity,
            base,
            content.material.copied(),
            None,
        );
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        class: usize,
        source: Arc<MeshSource>,
        content: &Content<'_>,
        opacity: f32,
        base: peniko::Color,
        material: Option<Material3D>,
        line_width: Option<f64>,
    ) {
        if opacity <= f32::EPSILON {
            return;
        }
        let source_lines = source.lines;
        let opaque_colors = if source.has_colors {
            source.opaque_colors
        } else {
            base.components[3] >= 0.999
        };
        // A model's surface reads its alpha as its material says: an opaque
        // or cut-out one ignores it, a blended one is opaque only where its
        // colors and texture all are.
        let opaque_colors = match &source.model {
            Some(model) if model.alpha == AlphaMode::Blend => {
                opaque_colors
                    && model.opaque_texture
                    && (!source.has_colors || base.components[3] >= 0.999)
            }
            Some(_) => true,
            None => opaque_colors,
        };
        // Filled outlines blend their edges over what is drawn before them,
        // after the opaque draws, so they draw there at any opacity.
        let fill = source.fill.is_some();
        let joints = content
            .joints
            .filter(|joints| source.skinned() && !joints.is_empty())
            .cloned();
        // The targets a morphed mesh weighs, and how far they reach.
        let (morph, reach) = match (content.morph, &source.model) {
            (Some(weights), Some(model)) if !model.reach.is_empty() => {
                let weighed: Vec<[f32; 2]> = weights
                    .iter()
                    .take(model.reach.len())
                    .enumerate()
                    .filter(|(_, weight)| weight.is_finite() && weight.abs() > 1e-6)
                    .map(|(target, weight)| [target as f32, *weight])
                    .collect();
                let reach: f64 = weighed
                    .iter()
                    .map(|[target, weight]| f64::from(model.reach[*target as usize] * weight.abs()))
                    .sum();
                ((!weighed.is_empty()).then_some(weighed), reach)
            }
            _ => (None, 0.0),
        };
        let rest = (source.center, source.radius + reach);
        let mut sphere = match &joints {
            Some(joints) => skinned_sphere(rest, joints),
            None => rest,
        };
        // Copies of a mesh reach as far as the sphere about every copy.
        let copies = content
            .instances
            .filter(|_| source.is_mesh() && joints.is_none())
            .filter(|instances| instances.copies.is_consistent());
        if let Some(instances) = copies {
            let (center, radius) = (sphere.0.as_vec3(), sphere.1 as f32);
            let mesh = (center - radius, center + radius);
            let Some((min, max)) = instances.copies.bounds(mesh) else {
                return;
            };
            let (min, max) = (min.as_dvec3(), max.as_dvec3());
            sphere = ((min + max) * 0.5, (max - min).length() * 0.5);
        }
        self.pending.push(Pending {
            copies: copies.map(|instances| (Arc::clone(&instances.copies), instances.reveal)),
            class,
            translucent: !fill && (opacity < 0.999 || !opaque_colors),
            joints,
            morph,
            sphere,
            source,
            model: content.transform.mat4,
            opacity,
            base: to_linear(base),
            material,
            line_width,
            line_range: if source_lines {
                content.line_range
            } else {
                None
            },
            glow: content.glow.clone(),
            shadow: content.shadow.clone(),
            blur: content.blur,
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
        if self.ends_frame {
            self.sources.end_frame();
        }
        if self.pending.is_empty() {
            return Vec::new();
        }
        let (width, height) = self.size;
        let shadows = shadows(illumination, &self.pending);
        let uniforms: Arc<[u8]> = frame_uniforms(
            camera,
            pixels_to_ndc(width, height) * self.placement * ndc_to_scene,
            (width, height),
            illumination,
            &shadows,
        )
        .into();
        let shadow_layers: Arc<[[f32; 16]]> = shadows
            .layers
            .iter()
            .map(|layer| layer.view_proj.as_mat4().to_cols_array())
            .collect();
        // Pixels of the image per scene unit, which line widths scale by.
        let pixels_per_unit = self.placement.determinant().abs().sqrt();
        // How far along the view the content lies, which bounds its blur.
        let forward = camera.rotation * DVec3::NEG_Z;
        let depths = self
            .pending
            .iter()
            .map(|pending| {
                let (center, radius) = world_sphere(pending);
                let distance = forward.dot(center - camera.position);
                (distance - radius, distance + radius)
            })
            .filter(|(near, far)| near.is_finite() && far.is_finite())
            .reduce(|(a, b), (c, d)| (a.min(c), b.max(d)));
        let focus = depths.and_then(|depths| {
            focus::FocusDraw::new(
                &illumination.depth_of_field,
                camera,
                pixels_per_unit,
                depths,
            )
        });

        // Opaque draws first, near ones first so that the depth test skips
        // what they hide; translucent ones after them, from back to front.
        let view = camera.view_matrix();
        let mut pending: Vec<(f64, Pending)> = self
            .pending
            .into_iter()
            .map(|pending| {
                let (center, radius) = world_sphere(&pending);
                // Filled outlines lie in a plane: its middle orders them, where
                // a wide text's far edge would put it behind a small shape that
                // lies behind it.
                let reach = if pending.source.fill.is_some() {
                    0.0
                } else {
                    radius
                };
                let depth = -view.transform_point3(center).z + reach;
                (depth, pending)
            })
            .collect();
        // Opaque meshes alike next to each other, so that they draw as one.
        // Filled outlines after them, as they blend without writing depth:
        // over all that is opaque, and from back to front.
        let rank = |pending: &Pending| {
            if pending.translucent {
                2
            } else if pending.source.fill.is_some() {
                1
            } else {
                0
            }
        };
        pending.sort_by(|(a_depth, a), (b_depth, b)| {
            a.class
                .cmp(&b.class)
                .then(rank(a).cmp(&rank(b)))
                .then_with(|| {
                    if rank(a) > 0 {
                        b_depth.total_cmp(a_depth)
                    } else {
                        a.source
                            .digest
                            .cmp(&b.source.digest)
                            .then(a_depth.total_cmp(b_depth))
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
            let mut instances = Vec::with_capacity(members.len() * INSTANCE_BYTES);
            let mut joints: Vec<u8> = Vec::new();
            let mut shown = 0_u32;
            for (_, pending) in members {
                let first_copy = shown;
                shown += push_instance(&mut instances, pending);
                let line_px = pending.line_width.unwrap_or(line_width) * pixels_per_unit;
                // The segments shown: all of them, or a reveal's part.
                let segments = pending.source.count as f32;
                let [from, to] = pending.line_range.map_or([0.0, segments], |[from, to]| {
                    [from * segments, to * segments]
                });
                let outline_px = pending.material.map_or(0.0, |material| {
                    f64::from(material.outline_width) * pixels_per_unit
                });
                let glow = pending
                    .glow
                    .as_ref()
                    .and_then(|glow| glow::GlowDraw::new(glow, pixels_per_unit));
                let shadow = pending
                    .shadow
                    .as_ref()
                    .and_then(|shadow| glow::GlowDraw::shadow(shadow, pixels_per_unit));
                // A blur of less than a tenth of a pixel leaves it sharp.
                let blur = pending
                    .blur
                    .map(|blur| blur.sigma * pixels_per_unit)
                    .filter(|sigma| sigma.is_finite() && *sigma >= 0.1)
                    .map(|sigma| (sigma as f32).to_bits());
                // A palette per skinned draw, at an offset the bind group
                // can start at.
                let palette = pending.joints.as_ref().map(|palette| {
                    let start = joints.len().next_multiple_of(DRAW_STRIDE);
                    joints.resize(start, 0);
                    for matrix in palette.iter() {
                        push_f32s(&mut joints, matrix);
                    }
                    start as u32
                });
                // A run of targets and weights per morphed draw, which the
                // shader reads as pairs of floats.
                let morph = pending.morph.as_ref().map(|weighed| {
                    let start = joints.len() / 8;
                    for pair in weighed {
                        push_f32s(&mut joints, pair);
                    }
                    (start as u32, weighed.len() as u32)
                });
                push_draw_uniforms(
                    &mut draw_uniforms,
                    pending,
                    [line_px, outline_px],
                    [from, to],
                    morph,
                );
                let instances = if pending.source.fill.is_some() {
                    0..pending.source.count
                } else if to > from {
                    (from.floor().max(0.0) as u32)..(to.ceil() as u32).min(pending.source.count)
                } else {
                    0..0
                };
                let double_sided = pending
                    .source
                    .model
                    .as_ref()
                    .is_some_and(|model| model.double_sided);
                draws.push(GpuDraw {
                    culled: pending.material.is_some() && pending.source.is_mesh() && !double_sided,
                    translucent: pending.translucent,
                    casts: pending.casts(),
                    instances,
                    copies: first_copy..shown,
                    outlined: !pending.translucent
                        && !pending.source.outline.is_empty()
                        && pending.material.is_some_and(|material| material.outlined()),
                    joints: palette,
                    morph,
                    glow,
                    shadow,
                    blur,
                    source: Arc::clone(&pending.source),
                });
            }
            // A palette binds JOINTS_BYTES from its start: the buffer holds
            // that much past the last one.
            if let Some(last) = draws.iter().filter_map(|draw| draw.joints).max() {
                joints.resize(joints.len().max(last as usize + JOINTS_BYTES), 0);
            }
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            image.data.id().hash(&mut hasher);
            uniforms.hash(&mut hasher);
            draw_uniforms.hash(&mut hasher);
            instances.hash(&mut hasher);
            joints.hash(&mut hasher);
            illumination.transparency.layers.hash(&mut hasher);
            focus.hash(&mut hasher);
            for draw in &draws {
                (Arc::as_ptr(&draw.source) as usize).hash(&mut hasher);
                draw.culled.hash(&mut hasher);
                draw.translucent.hash(&mut hasher);
                draw.casts.hash(&mut hasher);
                draw.outlined.hash(&mut hasher);
                draw.joints.hash(&mut hasher);
                draw.morph.hash(&mut hasher);
                draw.glow.hash(&mut hasher);
                draw.shadow.hash(&mut hasher);
                draw.blur.hash(&mut hasher);
            }
            frames.push((
                class,
                GpuMeshFrame {
                    image,
                    image_to_scene: self.placement.inverse(),
                    uniforms: Arc::clone(&uniforms),
                    draws: draws.into(),
                    draw_uniforms: draw_uniforms.into(),
                    instances: instances.into(),
                    shadow_size: shadows.size,
                    shadow_layers: Arc::clone(&shadow_layers),
                    joints: joints.into(),
                    peel: illumination.transparency.layers,
                    focus,
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

/// The largest factor by which `matrix` scales a length.
fn largest_scale(matrix: &DMat4) -> f64 {
    [
        matrix.x_axis.truncate().length(),
        matrix.y_axis.truncate().length(),
        matrix.z_axis.truncate().length(),
    ]
    .into_iter()
    .fold(0.0, f64::max)
}

/// The bounding sphere of a drawable's mesh or lines in the world.
fn world_sphere(pending: &Pending) -> (DVec3, f64) {
    let (center, radius) = pending.sphere;
    (
        pending.model.transform_point3(center),
        radius * largest_scale(&pending.model),
    )
}

/// A sphere, in model space, around a skinned mesh posed by `joints`: around
/// its bind-pose sphere `rest` placed by each joint, which holds every
/// vertex since a vertex lies between the places its joints take it.
fn skinned_sphere(rest: (DVec3, f64), joints: &[[f32; 16]]) -> (DVec3, f64) {
    let placed: Vec<(DVec3, f64)> = joints
        .iter()
        .map(|matrix| {
            let matrix = DMat4::from_cols_array(&matrix.map(f64::from));
            (
                matrix.transform_point3(rest.0),
                rest.1 * largest_scale(&matrix),
            )
        })
        .filter(|(center, radius)| center.is_finite() && radius.is_finite())
        .collect();
    let Some(low) = placed
        .iter()
        .map(|(center, radius)| *center - *radius)
        .reduce(DVec3::min)
    else {
        return rest;
    };
    let high = placed
        .iter()
        .map(|(center, radius)| *center + *radius)
        .fold(low, DVec3::max);
    let center = (low + high) * 0.5;
    let radius = placed
        .iter()
        .map(|(sphere, radius)| sphere.distance(center) + radius)
        .fold(0.0, f64::max);
    (center, radius)
}

/// The shadow maps of a frame: what each layer sees, and where the shadows
/// of the lights that cast them start.
#[derive(Default)]
struct Shadows {
    /// Texels on a side of each layer.
    size: u32,
    layers: Vec<ShadowLayer>,
    /// The first layer of the fixed key light's shadow and of each light's.
    key: Option<usize>,
    lights: [Option<usize>; MAX_LIGHTS],
}

/// What one shadow map layer sees.
#[derive(Clone, Copy, Debug)]
struct ShadowLayer {
    view_proj: DMat4,
    perspective: bool,
    /// The size of its texels: in scene units, or per scene unit of
    /// distance from the light under perspective.
    texel: f64,
    /// The distances from the light its depths span.
    near: f64,
    far: f64,
}

/// The shadows of the first lights of `illumination` (or of its fixed key
/// light while it shows none) over the meshes of `pending`: a map that
/// covers their bounding sphere along a directional light, and a cube of
/// six around a point light.
fn shadows(illumination: &Illumination, pending: &[Pending]) -> Shadows {
    let mut shadows = Shadows::default();
    let lighting = &illumination.lighting;
    // Shadows need a lit mesh to fall on and a mesh to cast them.
    let receives = pending
        .iter()
        .any(|pending| pending.material.is_some() && !pending.source.lines);
    if !lighting.shadows || !receives || !pending.iter().any(Pending::casts) {
        return shadows;
    }
    let spheres: Vec<(DVec3, f64)> = pending
        .iter()
        .filter(|pending| !pending.source.lines)
        .map(world_sphere)
        .filter(|(center, radius)| center.is_finite() && radius.is_finite())
        .collect();
    let Some(low) = spheres
        .iter()
        .map(|(center, radius)| center - *radius)
        .reduce(DVec3::min)
    else {
        return shadows;
    };
    let high = spheres
        .iter()
        .map(|(center, radius)| center + *radius)
        .fold(low, DVec3::max);
    let center = (low + high) * 0.5;
    let radius = spheres
        .iter()
        .map(|(sphere, radius)| sphere.distance(center) + radius)
        .fold(0.0, f64::max);
    if !(radius.is_finite() && radius > 1e-9) {
        return shadows;
    }
    let key = Light {
        position: KEY_LIGHT,
        directional: true,
        color: [1.0; 3],
        spot: None,
    };
    let casters: Vec<(Option<usize>, Light)> = if illumination.lights.is_empty() {
        let lit = lighting.enabled && lighting.intensity > 0.0;
        lit.then_some((None, key)).into_iter().collect()
    } else {
        illumination
            .lights
            .iter()
            .take(MAX_SHADOW_LIGHTS)
            .enumerate()
            .map(|(index, light)| (Some(index), *light))
            .collect()
    };
    // A cube of six around a point light, one map for the others.
    let layers: usize = casters
        .iter()
        .map(|(_, light)| {
            if light.directional || light.spot.is_some() {
                1
            } else {
                6
            }
        })
        .sum();
    shadows.size = match layers {
        1 => SHADOW_SIZE,
        ..=12 => SHADOW_SIZE_SHARED,
        _ => SHADOW_SIZE_MANY,
    };
    for (index, light) in casters {
        let first = shadows.layers.len();
        if light.directional {
            match directional_shadow(light.position, center, radius, shadows.size) {
                Some(layer) => shadows.layers.push(layer),
                None => continue,
            }
        } else if let Some(spot) = &light.spot {
            shadows.layers.push(spot_shadow(
                light.position,
                spot,
                center,
                radius,
                shadows.size,
            ));
        } else {
            shadows
                .layers
                .extend(point_shadows(light.position, center, radius, shadows.size));
        }
        match index {
            None => shadows.key = Some(first),
            Some(index) => shadows.lights[index] = Some(first),
        }
    }
    if shadows.layers.is_empty() {
        shadows.size = 0;
    }
    shadows
}

/// The shadow map of a light shining from the direction `toward` over the
/// sphere at `center` with `radius`, `size` texels on a side. The map is
/// seen from the origin and its radius steps by an eighth of an octave, so
/// that its texels stay put while the content moves within it.
fn directional_shadow(toward: DVec3, center: DVec3, radius: f64, size: u32) -> Option<ShadowLayer> {
    let toward = toward.try_normalize()?;
    let up = if toward.cross(DVec3::Y).length_squared() > 1e-6 {
        DVec3::Y
    } else {
        DVec3::Z
    };
    let view = view::look_at_mat4(DVec3::ZERO, -toward, up);
    let radius = ((radius.log2() * 8.0).ceil() / 8.0).exp2();
    let texel = 2.0 * radius / f64::from(size);
    let middle = view.transform_point3(center);
    let x = (middle.x / texel).round() * texel;
    let y = (middle.y / texel).round() * texel;
    let reach = radius + texel;
    let depth = -middle.z;
    let (near, far) = (depth - reach, depth + reach);
    let projection = directx::orthographic(x - reach, x + reach, y - reach, y + reach, near, far);
    Some(ShadowLayer {
        view_proj: projection * view,
        perspective: false,
        texel,
        near,
        far,
    })
}

/// The six shadow maps of a point light at `position` over the sphere at
/// `center` with `radius`, `size` texels on a side: one per cube face, in
/// the order +x, -x, +y, -y, +z, -z (`shadowed` in [`SHADER`]).
fn point_shadows(position: DVec3, center: DVec3, radius: f64, size: u32) -> [ShadowLayer; 6] {
    let distance = position.distance(center);
    let near = ((distance - radius) * 0.5).max(radius * 1e-3).max(1e-4);
    let far = (distance + radius) * 1.01;
    let projection = directx::perspective(std::f64::consts::FRAC_PI_2, 1.0, near, far);
    let faces = [
        (DVec3::X, DVec3::NEG_Y),
        (DVec3::NEG_X, DVec3::NEG_Y),
        (DVec3::Y, DVec3::Z),
        (DVec3::NEG_Y, DVec3::NEG_Z),
        (DVec3::Z, DVec3::NEG_Y),
        (DVec3::NEG_Z, DVec3::NEG_Y),
    ];
    // A face spans 90 degrees: its texels are 2 / size wide a unit away.
    faces.map(|(axis, up)| ShadowLayer {
        view_proj: projection * view::look_at_mat4(position, position + axis, up),
        perspective: true,
        texel: 2.0 / f64::from(size),
        near,
        far,
    })
}

/// The shadow map of a spot light at `position` over the sphere at `center`
/// with `radius`, `size` texels on a side: a perspective view along its
/// direction, a little wider than its cone.
fn spot_shadow(position: DVec3, spot: &Spot, center: DVec3, radius: f64, size: u32) -> ShadowLayer {
    let distance = position.distance(center);
    let near = ((distance - radius) * 0.5).max(radius * 1e-3).max(1e-4);
    let far = (distance + radius) * 1.01;
    let half = (f64::from(spot.cos_outer).acos() * 1.05 + 0.02).min(1.5);
    let projection = directx::perspective(2.0 * half, 1.0, near, far);
    let up = if spot.direction.cross(DVec3::Y).length_squared() > 1e-6 {
        DVec3::Y
    } else {
        DVec3::Z
    };
    ShadowLayer {
        view_proj: projection * view::look_at_mat4(position, position + spot.direction, up),
        perspective: true,
        texel: 2.0 * half.tan() / f64::from(size),
        near,
        far,
    }
}

/// The frame uniforms: `camera`, the map `ndc_map` from its NDC to the
/// image's, the image's `size`, the lights and their shadows.
fn frame_uniforms(
    camera: &Camera,
    ndc_map: kurbo::Affine,
    size: (u32, u32),
    illumination: &Illumination,
    shadows: &Shadows,
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
    // A shadow's first layer, plus one; 0 for none.
    let layer = |first: Option<usize>| first.map_or(0.0, |first| first as f32 + 1.0);
    push_f32s(
        &mut uniforms,
        &[
            f32::from(u8::from(lights.is_empty())),
            lights.len() as f32,
            layer(shadows.key),
            0.0,
        ],
    );
    push_f32s(
        &mut uniforms,
        &[
            if shadows.size > 0 {
                1.0 / shadows.size as f32
            } else {
                0.0
            },
            SHADOW_CONTACT,
            shadows.layers.len() as f32,
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
                    &[
                        light.color[0],
                        light.color[1],
                        light.color[2],
                        layer(shadows.lights[index]),
                    ],
                );
                // A spot light's direction and cone; none for the others.
                let (direction, cos_outer, cos_inner) =
                    light.spot.map_or((DVec3::ZERO, 0.0, 0.0), |spot| {
                        (spot.direction, spot.cos_outer, spot.cos_inner)
                    });
                push_f32s(
                    &mut uniforms,
                    &[
                        direction.x as f32,
                        direction.y as f32,
                        direction.z as f32,
                        cos_outer,
                    ],
                );
                push_f32s(&mut uniforms, &[cos_inner, 0.0, 0.0, 0.0]);
            }
            None => push_f32s(&mut uniforms, &[0.0; 16]),
        }
    }
    for index in 0..MAX_SHADOW_LAYERS {
        match shadows.layers.get(index) {
            Some(layer) => {
                push_mat4(&mut uniforms, layer.view_proj);
                push_f32s(
                    &mut uniforms,
                    &[
                        layer.texel as f32,
                        f32::from(u8::from(layer.perspective)),
                        layer.near as f32,
                        layer.far as f32,
                    ],
                );
            }
            None => push_f32s(&mut uniforms, &[0.0; 20]),
        }
    }
    debug_assert_eq!(uniforms.len(), FRAME_BYTES);
    uniforms
}

/// Append one draw's instance data (see [`INSTANCE_BYTES`]): one record,
/// or one per copy shown of a mesh drawn at many points; returns how many.
fn push_instance(bytes: &mut Vec<u8>, pending: &Pending) -> u32 {
    let push = |bytes: &mut Vec<u8>, model: DMat4, base: &[f32; 4]| {
        let start = bytes.len();
        push_mat4(bytes, model);
        let normal = model.inverse().transpose();
        for column in [normal.x_axis, normal.y_axis, normal.z_axis] {
            push_f32s(
                bytes,
                &[column.x as f32, column.y as f32, column.z as f32, 0.0],
            );
        }
        push_f32s(bytes, base);
        debug_assert_eq!(bytes.len() - start, INSTANCE_BYTES);
    };
    let Some((copies, reveal)) = &pending.copies else {
        push(bytes, pending.model, &pending.base);
        return 1;
    };
    let count = copies.positions.len();
    let instances = MeshInstances {
        copies: Arc::clone(copies),
        reveal: *reveal,
    };
    let mut pushed = 0;
    for index in 0..count {
        let size = MeshInstances::shown(*reveal, index, count);
        if size <= 0.0 {
            continue;
        }
        let model = pending.model * instances.matrix(index, size).as_dmat4();
        let base = copies
            .colors
            .as_ref()
            .map_or(pending.base, |colors| colors[index]);
        push(bytes, model, &base);
        pushed += 1;
    }
    pushed
}

/// The instance records draws `first..end` of `frame` draw: alike draws
/// draw as one, and their records follow each other.
fn copies_of(frame: &GpuMeshFrame, first: usize, end: usize) -> std::ops::Range<u32> {
    frame.draws[first].copies.start..frame.draws[end - 1].copies.end
}

/// Whether draws `a` and `b` of a frame draw as one: opaque meshes alike,
/// with the same uniforms but for their placement and base color.
fn draw_alike(frame: &GpuMeshFrame, a: usize, b: usize) -> bool {
    let (first, second) = (&frame.draws[a], &frame.draws[b]);
    // The model and normal matrices and the base color lead each draw's
    // uniforms; instances carry those.
    const SHARED: std::ops::Range<usize> = 144..DRAW_BYTES;
    let uniforms = |index: usize| &frame.draw_uniforms[index * DRAW_STRIDE..][SHARED];
    first.source.is_mesh()
        && !first.translucent
        && !second.translucent
        && first.joints.is_none()
        && second.joints.is_none()
        && first.morph.is_none()
        && second.morph.is_none()
        && Arc::ptr_eq(&first.source, &second.source)
        && first.culled == second.culled
        && first.casts == second.casts
        && first.blur == second.blur
        && uniforms(a) == uniforms(b)
}

/// Append one draw's uniforms, padded to `DRAW_STRIDE`: its lines and its
/// outline `widths_px` pixels wide, its lines showing the segments from
/// `shown[0]` to `shown[1]` (fractional at the ends), and a morphed model's
/// run of targets and weights among the frame's, `morph`.
fn push_draw_uniforms(
    bytes: &mut Vec<u8>,
    pending: &Pending,
    [line_px, outline_px]: [f64; 2],
    shown: [f32; 2],
    morph: Option<(u32, u32)>,
) {
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
    push_f32s(bytes, &[(line_px * 0.5) as f32, shown[0], shown[1], 0.0]);
    let outline = pending
        .material
        .map_or([0.0; 4], |material| to_linear(material.outline));
    push_f32s(bytes, &outline);
    let open = f32::from(u8::from(!pending.source.closed));
    let sheet = f32::from(u8::from(pending.source.sheet));
    push_f32s(bytes, &[outline_px as f32, open, sheet, 0.0]);
    // An imported model's alpha cutoff; whether it has a texture, a
    // palette and blended alpha, as bits; and a morphed one's run of
    // targets and weights (`Draw.imported` in [`SHADER`]).
    let model = pending.source.model.as_ref().map_or([0.0; 4], |model| {
        let flags = u8::from(model.texture.is_some())
            | u8::from(pending.joints.is_some()) << 1
            | u8::from(model.alpha == AlphaMode::Blend) << 2;
        let [first, count] = morph.map_or([0.0; 2], |(first, count)| [first as f32, count as f32]);
        [model.cutoff(), f32::from(flags), first, count]
    });
    push_f32s(bytes, &model);
    debug_assert_eq!(bytes.len() - start, DRAW_BYTES);
    bytes.resize(start + DRAW_STRIDE, 0);
}

pub(crate) fn to_linear(color: peniko::Color) -> [f32; 4] {
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
            // Peels read the layer's depth (see `peel`).
            layer: texture(
                "gaanim-3d-layer-depth",
                DEPTH_FORMAT,
                attachment | wgpu::TextureUsages::TEXTURE_BINDING,
            ),
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
    shadow_layout: wgpu::BindGroupLayout,
    caster_layout: wgpu::BindGroupLayout,
    /// The curves and bands of filled outlines.
    fill_layout: wgpu::BindGroupLayout,
    shadow_sampler: wgpu::Sampler,
    /// Meshes by `[culled][pass]`.
    meshes: [[wgpu::RenderPipeline; 3]; 2],
    /// Lines by pass.
    lines: [wgpu::RenderPipeline; 3],
    /// The outlines of opaque meshes, by `[closed]`: a closed mesh's
    /// outline draws the faces turned away, an open mesh's both faces.
    outline: [wgpu::RenderPipeline; 2],
    /// Filled outlines into a shadow map.
    shadow_fill: wgpu::RenderPipeline,
    /// Filled outlines into the depth, where they hide what lies behind
    /// them from the translucent layers.
    fill_depth: wgpu::RenderPipeline,
    /// Opaque meshes into a shadow map, by `[closed]`: an open mesh casts
    /// with both faces, a closed one with the faces turned away from the
    /// light, so that it never shadows the side that faces it.
    shadow: [wgpu::RenderPipeline; 2],
    /// Filled outlines, both faces, blended over what is drawn before them.
    fill: wgpu::RenderPipeline,
}

impl Pipelines {
    fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-3d"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{SHADER}{}", crate::plane_fill::SHADER).into(),
            ),
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
        // Group 1 holds what each pass reads besides the uniforms: the
        // opaque depth for translucent layers, the shadow maps for shading,
        // and a shadow map's view for the meshes drawn into it.
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-3d-shadows"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let caster_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-3d-caster"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(CASTER_BYTES as u64),
                },
                count: None,
            }],
        });
        let storage = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let fill_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-3d-fill"),
            entries: &[
                storage(0),
                storage(1),
                storage(2),
                storage(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("gaanim-3d-shadows"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let plain = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-3d"),
            bind_group_layouts: &[Some(&layout), Some(&shadow_layout)],
            immediate_size: 0,
        });
        let casting = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-3d-shadow"),
            bind_group_layouts: &[Some(&layout), Some(&caster_layout)],
            immediate_size: 0,
        });
        let filling = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-3d-fill"),
            bind_group_layouts: &[Some(&layout), Some(&shadow_layout), Some(&fill_layout)],
            immediate_size: 0,
        });
        let fill_casting = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-3d-shadow-fill"),
            bind_group_layouts: &[Some(&layout), Some(&caster_layout), Some(&fill_layout)],
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
        // A draw's instance data: model matrix, normal matrix, base color.
        let instance_attributes: [wgpu::VertexAttribute; 8] =
            std::array::from_fn(|index| wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: index as u64 * 16,
                shader_location: 3 + index as u32,
            });
        let instance_layout = |attributes| wgpu::VertexBufferLayout {
            array_stride: INSTANCE_BYTES as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes,
        };
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
            let segment_buffers = [wgpu::VertexBufferLayout {
                array_stride: SEGMENT_BYTES as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &segment_attributes,
            }];
            let mesh_buffers = [
                wgpu::VertexBufferLayout {
                    array_stride: MESH_VERTEX_BYTES as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &mesh_attributes,
                },
                instance_layout(&instance_attributes),
            ];
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
                    buffers: if lines {
                        &segment_buffers[..]
                    } else {
                        &mesh_buffers[..]
                    },
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
        let shadow = |closed: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("gaanim-3d-shadow"),
                layout: Some(&casting),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vertex_shadow"),
                    compilation_options: Default::default(),
                    buffers: &[
                        wgpu::VertexBufferLayout {
                            array_stride: MESH_VERTEX_BYTES as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &mesh_attributes[..1],
                        },
                        instance_layout(&instance_attributes[..4]),
                    ],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: closed.then_some(wgpu::Face::Front),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    // Keeps a surface from shadowing itself.
                    bias: wgpu::DepthBiasState {
                        constant: 2,
                        slope_scale: 2.0,
                        clamp: 0.0,
                    },
                }),
                multisample: Default::default(),
                fragment: None,
                multiview_mask: None,
                cache: None,
            })
        };
        let outline_attributes = [wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 0,
            shader_location: 11,
        }];
        // The mesh pushed out along its outline directions by the outline's
        // width: its faces turned away from the viewer show only beyond the
        // silhouette. An open mesh has none, so it draws both faces, pushed
        // back behind the mesh too.
        let outline = |closed: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("gaanim-3d-outline"),
                layout: Some(&plain),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vertex_outline"),
                    compilation_options: Default::default(),
                    buffers: &[
                        wgpu::VertexBufferLayout {
                            array_stride: MESH_VERTEX_BYTES as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &mesh_attributes[..1],
                        },
                        instance_layout(&instance_attributes),
                        wgpu::VertexBufferLayout {
                            array_stride: 12,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &outline_attributes,
                        },
                    ],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: closed.then_some(wgpu::Face::Front),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Greater),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fragment_outline"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let fill_attributes = fill_attributes();
        let fill = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gaanim-3d-fill"),
            layout: Some(&filling),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vertex_fill"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: crate::plane_fill::GLYPH_BYTES as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &fill_attributes,
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                // Edges blend, so what lies behind still draws there.
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLES,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fragment_fill"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let fill_depth = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gaanim-3d-fill-depth"),
            layout: Some(&filling),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vertex_fill"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: crate::plane_fill::GLYPH_BYTES as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &fill_attributes,
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLES,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fragment_fill_depth"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::empty(),
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let shadow_fill = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gaanim-3d-shadow-fill"),
            layout: Some(&fill_casting),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vertex_fill_shadow"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: crate::plane_fill::GLYPH_BYTES as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &fill_attributes,
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                // As meshes: keeps an outline from shadowing itself.
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fragment_fill_shadow"),
                compilation_options: Default::default(),
                targets: &[],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            layout,
            depth_layout,
            shadow_layout,
            caster_layout,
            fill_layout,
            shadow_sampler,
            meshes: [passes(false, false), passes(false, true)],
            lines: passes(true, false),
            outline: [outline(false), outline(true)],
            shadow: [shadow(false), shadow(true)],
            shadow_fill,
            fill_depth,
            fill,
        }
    }
}

/// The pipelines of imported models' primitives ([`ModelBuffers`]): those
/// of plain meshes plus a texture, an alpha test and a skin's palette, in a
/// shader module of their own ([`MODEL_SHADER`]). A renderer builds them the
/// first time a frame draws a model, so scenes without one never compile
/// them.
struct ModelPipelines {
    /// Group 2: the base-color texture and the normal map with their
    /// samplers, and what the vertices read by index.
    texture_layout: wgpu::BindGroupLayout,
    /// Group 3: a draw's palette, bound at its offset in the frame's joints,
    /// and the frame's morph targets and weights.
    joints_layout: wgpu::BindGroupLayout,
    /// Meshes by `[culled][pass]`.
    meshes: [[wgpu::RenderPipeline; 3]; 2],
    /// Into a shadow map, by `[closed]`, as [`Pipelines::shadow`].
    shadow: [wgpu::RenderPipeline; 2],
    /// A white texel, which models without a texture read, a texel of a
    /// normal that tilts nothing, a sampler for either, and a header of no
    /// tangents and no targets, which models without a normal map or
    /// morph targets read.
    white: wgpu::TextureView,
    flat: wgpu::TextureView,
    sampler: wgpu::Sampler,
    no_geometry: wgpu::Buffer,
    /// What models with none of these bind.
    blank: wgpu::BindGroup,
}

impl ModelPipelines {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, base: &Pipelines) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-3d-model"),
            source: wgpu::ShaderSource::Wgsl(format!("{SHADER}{MODEL_SHADER}").into()),
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-3d-model-texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(16),
                    },
                    count: None,
                },
            ],
        });
        let joints_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-3d-model-joints"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: NonZeroU64::new(JOINTS_BYTES as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 10,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(8),
                    },
                    count: None,
                },
            ],
        });
        let layout = |label, second: &wgpu::BindGroupLayout| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[
                    Some(&base.layout),
                    Some(second),
                    Some(&texture_layout),
                    Some(&joints_layout),
                ],
                immediate_size: 0,
            })
        };
        let plain = layout("gaanim-3d-model", &base.shadow_layout);
        let layered = layout("gaanim-3d-model-layer", &base.depth_layout);
        let casting = layout("gaanim-3d-model-shadow", &base.caster_layout);
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
        let instance_attributes: [wgpu::VertexAttribute; 8] =
            std::array::from_fn(|index| wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: index as u64 * 16,
                shader_location: 3 + index as u32,
            });
        let model_attributes = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 12,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint16x4,
                offset: 8,
                shader_location: 13,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 14,
            },
        ];
        let buffers = |positions_only: bool| {
            [
                wgpu::VertexBufferLayout {
                    array_stride: MESH_VERTEX_BYTES as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: if positions_only {
                        &mesh_attributes[..1]
                    } else {
                        &mesh_attributes[..]
                    },
                },
                wgpu::VertexBufferLayout {
                    array_stride: INSTANCE_BYTES as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: if positions_only {
                        &instance_attributes[..4]
                    } else {
                        &instance_attributes[..]
                    },
                },
                wgpu::VertexBufferLayout {
                    array_stride: MODEL_VERTEX_BYTES as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &model_attributes,
                },
            ]
        };
        let mesh_buffers = buffers(false);
        let shadow_buffers = buffers(true);
        // As `Pipelines::new` builds meshes, with the model's entry points.
        let pipeline = |culled: bool, pass: Pass| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("gaanim-3d-model"),
                layout: Some(if matches!(pass, Pass::Layer) {
                    &layered
                } else {
                    &plain
                }),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vertex_model"),
                    compilation_options: Default::default(),
                    buffers: &mesh_buffers,
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
                    entry_point: Some(if matches!(pass, Pass::Layer) {
                        "fragment_model_layer"
                    } else {
                        "fragment_model"
                    }),
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
        let passes = |culled| {
            [
                pipeline(culled, Pass::Opaque),
                pipeline(culled, Pass::Layer),
                pipeline(culled, Pass::Blend),
            ]
        };
        let shadow = |closed: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("gaanim-3d-model-shadow"),
                layout: Some(&casting),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vertex_model_shadow"),
                    compilation_options: Default::default(),
                    buffers: &shadow_buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: closed.then_some(wgpu::Face::Front),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: wgpu::DepthBiasState {
                        constant: 2,
                        slope_scale: 2.0,
                        clamp: 0.0,
                    },
                }),
                multisample: Default::default(),
                // Cut-out texels cast no shadow.
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fragment_model_shadow"),
                    compilation_options: Default::default(),
                    targets: &[],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let white = upload_texture(device, queue, &white_texel(), false);
        let flat = upload_texture(
            device,
            queue,
            &ImageData {
                data: Blob::from(vec![128_u8, 128, 255, 255]),
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::Alpha,
                width: 1,
                height: 1,
            },
            true,
        );
        let sampler = texture_sampler(device, [TextureWrap::Repeat; 2], false);
        let no_geometry = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gaanim-3d-model-geometry"),
            contents: &[0; 16],
            usage: wgpu::BufferUsages::STORAGE,
        });
        let blank = model_group(
            device,
            &texture_layout,
            (&white, &sampler),
            (&flat, &sampler),
            &no_geometry,
        );
        Self {
            meshes: [passes(false), passes(true)],
            shadow: [shadow(false), shadow(true)],
            texture_layout,
            joints_layout,
            white,
            flat,
            sampler,
            no_geometry,
            blank,
        }
    }
}

/// An outline's instance: its box, placement, bands, band header, color,
/// and its clip's bands and band header (`FillIn` in the shader).
fn fill_attributes() -> [wgpu::VertexAttribute; 8] {
    std::array::from_fn(|index| wgpu::VertexAttribute {
        format: if index == 4 || index == 7 {
            wgpu::VertexFormat::Uint32x4
        } else {
            wgpu::VertexFormat::Float32x4
        },
        offset: index as u64 * 16,
        shader_location: index as u32,
    })
}

/// Draw the quads of `instances` of filled outlines, each run of them with
/// its image bound.
fn draw_runs(
    pass: &mut wgpu::RenderPass<'_>,
    runs: &[(std::ops::Range<u32>, wgpu::BindGroup)],
    instances: &std::ops::Range<u32>,
) {
    for (range, group) in runs {
        let (start, end) = (
            range.start.max(instances.start),
            range.end.min(instances.end),
        );
        if start < end {
            pass.set_bind_group(2, group, &[]);
            pass.draw(0..6, start..end);
        }
    }
}

/// One opaque white texel.
fn white_texel() -> ImageData {
    ImageData {
        data: Blob::from(vec![255_u8; 4]),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: 1,
        height: 1,
    }
}

/// `image` as RGBA texels with straight alpha, as [`upload_texture`] takes
/// them: itself when it already is.
fn straight_rgba(image: &ImageData) -> std::borrow::Cow<'_, ImageData> {
    if image.format == ImageFormat::Rgba8 && image.alpha_type == ImageAlphaType::Alpha {
        return std::borrow::Cow::Borrowed(image);
    }
    let texels = image.width as usize * image.height as usize;
    let bytes = image.data.data();
    let mut converted = Vec::with_capacity(texels * 4);
    for texel in bytes.chunks_exact(4).take(texels) {
        let [mut r, g, mut b, a] = [texel[0], texel[1], texel[2], texel[3]];
        if image.format == ImageFormat::Bgra8 {
            std::mem::swap(&mut r, &mut b);
        }
        let straight = |value: u8| {
            if image.alpha_type == ImageAlphaType::AlphaPremultiplied && a > 0 {
                ((u32::from(value) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
            } else {
                value
            }
        };
        converted.extend([straight(r), straight(g), straight(b), a]);
    }
    std::borrow::Cow::Owned(ImageData {
        data: Blob::from(converted),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: image.width,
        height: image.height,
    })
}

/// A view of `image`, uploaded with its mipmaps: sRGB texels that the
/// shader reads in linear light, or, `linear`, texels it reads as they are,
/// as a normal map's.
fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &ImageData,
    linear: bool,
) -> wgpu::TextureView {
    let levels = mipmaps(image, linear);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gaanim-3d-model-texture"),
        size: wgpu::Extent3d {
            width: image.width.max(1),
            height: image.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: if linear {
            wgpu::TextureFormat::Rgba8Unorm
        } else {
            wgpu::TextureFormat::Rgba8UnormSrgb
        },
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, (width, height, texels)) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            texels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(*height),
            },
            wgpu::Extent3d {
                width: *width,
                height: *height,
                depth_or_array_layers: 1,
            },
        );
    }
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// The mipmaps of an RGBA8 image in sRGB with straight alpha, from the
/// image itself to one texel: each level averages 2x2 texels of the one
/// before in linear light, weighted by their alpha. A `linear` image, a
/// normal map's, averages its texels as they are, alpha aside.
fn mipmaps(image: &ImageData, linear: bool) -> Vec<(u32, u32, std::borrow::Cow<'_, [u8]>)> {
    let (width, height) = (image.width.max(1), image.height.max(1));
    let bytes = image.data.data();
    if bytes.len() < width as usize * height as usize * 4 {
        return vec![(1, 1, std::borrow::Cow::Owned(vec![255; 4]))];
    }
    let decode: [f32; 256] = std::array::from_fn(|value| {
        let value = value as f32 / 255.0;
        if linear {
            value
        } else if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    });
    let encode = |value: f32| {
        let value = value.clamp(0.0, 1.0);
        let srgb = if linear {
            value
        } else if value <= 0.0031308 {
            value * 12.92
        } else {
            1.055 * value.powf(1.0 / 2.4) - 0.055
        };
        (srgb * 255.0).round() as u8
    };
    let mut levels = vec![(
        width,
        height,
        std::borrow::Cow::Borrowed(&bytes[..width as usize * height as usize * 4]),
    )];
    // Premultiplied linear light, which averages without dark fringes.
    let mut current: Vec<[f32; 4]> = bytes
        .chunks_exact(4)
        .take(width as usize * height as usize)
        .map(|texel| {
            let alpha = if linear {
                1.0
            } else {
                f32::from(texel[3]) / 255.0
            };
            [
                decode[texel[0] as usize] * alpha,
                decode[texel[1] as usize] * alpha,
                decode[texel[2] as usize] * alpha,
                alpha,
            ]
        })
        .collect();
    let (mut w, mut h) = (width, height);
    while w > 1 || h > 1 {
        let (next_w, next_h) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![[0.0_f32; 4]; next_w as usize * next_h as usize];
        for y in 0..next_h {
            for x in 0..next_w {
                let mut sum = [0.0_f32; 4];
                let mut count = 0.0;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (sx, sy) = ((x * 2 + dx).min(w - 1), (y * 2 + dy).min(h - 1));
                    let texel = current[(sy * w + sx) as usize];
                    for channel in 0..4 {
                        sum[channel] += texel[channel];
                    }
                    count += 1.0;
                }
                next[(y * next_w + x) as usize] = sum.map(|value| value / count);
            }
        }
        let texels: Vec<u8> = next
            .iter()
            .flat_map(|&[r, g, b, a]| {
                let straight = |value: f32| if a > 1e-6 { value / a } else { 0.0 };
                [
                    encode(straight(r)),
                    encode(straight(g)),
                    encode(straight(b)),
                    (a.clamp(0.0, 1.0) * 255.0).round() as u8,
                ]
            })
            .collect();
        levels.push((next_w, next_h, std::borrow::Cow::Owned(texels)));
        current = next;
        (w, h) = (next_w, next_h);
    }
    levels
}

/// A sampler that repeats as `wrap` says along u and v, trilinear, or with
/// nearest texels up close for pixel art.
fn texture_sampler(device: &wgpu::Device, wrap: [TextureWrap; 2], nearest: bool) -> wgpu::Sampler {
    let mode = |wrap: TextureWrap| match wrap {
        TextureWrap::Repeat => wgpu::AddressMode::Repeat,
        TextureWrap::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
        TextureWrap::ClampToEdge => wgpu::AddressMode::ClampToEdge,
    };
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("gaanim-3d-model-texture"),
        address_mode_u: mode(wrap[0]),
        address_mode_v: mode(wrap[1]),
        mag_filter: if nearest {
            wgpu::FilterMode::Nearest
        } else {
            wgpu::FilterMode::Linear
        },
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    })
}

/// Binds a model's base-color texture and normal map, each through its
/// sampler, and what its vertices read by index.
fn model_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    (texture, sampler): (&wgpu::TextureView, &wgpu::Sampler),
    (normal_map, normal_sampler): (&wgpu::TextureView, &wgpu::Sampler),
    geometry: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gaanim-3d-model-texture"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(texture),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(normal_map),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::Sampler(normal_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 9,
                resource: geometry.as_entire_binding(),
            },
        ],
    })
}

/// The shadow maps frames draw their opaque meshes into, one layer per view
/// of a light, and the bind groups that read and write them.
struct ShadowMaps {
    size: u32,
    layers: u32,
    /// Each layer, as the depth target of the meshes drawn into it.
    targets: Vec<wgpu::TextureView>,
    /// Reads every layer while shading.
    group: wgpu::BindGroup,
}

impl ShadowMaps {
    /// `layers` maps `size` texels on a side; one texel when a frame has no
    /// shadows, which the shading pipelines still bind.
    fn new(device: &wgpu::Device, pipelines: &Pipelines, size: u32, layers: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gaanim-3d-shadows"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let all = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let targets = (0..layers)
            .map(|layer| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gaanim-3d-shadows"),
            layout: &pipelines.shadow_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&all),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&pipelines.shadow_sampler),
                },
            ],
        });
        Self {
            size,
            layers,
            targets,
            group,
        }
    }
}

/// A source's uploaded buffers.
struct Uploaded {
    /// Holding the source keeps its address, the key, unique.
    _source: Arc<MeshSource>,
    vertices: wgpu::Buffer,
    indices: Option<wgpu::Buffer>,
    outline: Option<wgpu::Buffer>,
    /// Binds the curves and bands of filled outlines, with the image of
    /// each run of them.
    fill: Option<Vec<(std::ops::Range<u32>, wgpu::BindGroup)>>,
    /// An imported model's UVs, joints and weights, and its texture.
    model: Option<(wgpu::Buffer, wgpu::BindGroup)>,
}

/// Draws [`GpuMeshFrame`]s on the device of a Vello renderer.
#[derive(Default)]
pub struct GpuThreeD {
    device: Option<wgpu::Device>,
    pipelines: Option<Pipelines>,
    /// Built the first time a frame draws an imported model.
    models: Option<ModelPipelines>,
    /// Built the first time a frame draws a glow, and its textures.
    glow: Option<glow::GlowPipelines>,
    glow_targets: Option<glow::GlowTargets>,
    /// Built the first time a frame peels its translucent surfaces, and the
    /// textures they peel into, which bind the scratch targets.
    peel: Option<peel::PeelPipelines>,
    peel_targets: Option<peel::PeelTargets>,
    /// Built the first time a frame is out of focus, and the textures it
    /// blurs in.
    focus: Option<focus::FocusPipelines>,
    focus_targets: Option<focus::FocusTargets>,
    uploads: HashMap<usize, Uploaded>,
    /// Models' and filled outlines' textures by the id of their pixels and
    /// whether they are read as they are (normal maps), and their samplers.
    textures: HashMap<(u64, bool), wgpu::TextureView>,
    /// The texel outlines filled otherwise bind in place of an image.
    white: Option<wgpu::TextureView>,
    samplers: HashMap<([TextureWrap; 2], bool), wgpu::Sampler>,
    /// The frame's palettes, with room for that many bytes, and their bind
    /// group.
    joint_buffer: Option<(wgpu::Buffer, usize, wgpu::BindGroup)>,
    /// By image.
    targets: HashMap<u64, Target>,
    scratch: Option<Scratch>,
    frame_buffer: Option<wgpu::Buffer>,
    /// The draw uniforms, with room for that many bytes, and their bind group.
    draw_buffer: Option<(wgpu::Buffer, usize, wgpu::BindGroup)>,
    shadow_maps: Option<ShadowMaps>,
    /// The frame's instance data, with room for that many bytes.
    instance_buffer: Option<(wgpu::Buffer, usize)>,
    /// The view-projections of the shadow map layers, with room for that
    /// many layers, and their bind group.
    caster_buffer: Option<(wgpu::Buffer, usize, wgpu::BindGroup)>,
    /// Tests only: two timestamps, written as the color passes begin and
    /// end, which time them on the GPU.
    #[cfg(test)]
    timing: Option<wgpu::QuerySet>,
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
        if !self.textures.is_empty() {
            let shown: HashSet<(u64, bool)> = frames
                .iter()
                .flat_map(|frame| frame.draws.iter())
                .flat_map(|draw| {
                    let model = draw.source.model.as_ref();
                    let texture = model
                        .and_then(|model| model.texture.as_ref())
                        .map(|texture| (texture.image.data.id(), false));
                    let normal_map = model
                        .and_then(|model| model.normal_map.as_ref())
                        .map(|texture| (texture.image.data.id(), true));
                    let images = draw.source.fill.iter().flat_map(|fill| {
                        fill.runs
                            .iter()
                            .filter_map(|(_, brush)| Some((brush.as_ref()?.image.data.id(), false)))
                    });
                    texture.into_iter().chain(normal_map).chain(images)
                })
                .collect();
            self.textures.retain(|key, _| shown.contains(key));
        }
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
            let resolved = (target.resolved.clone(), target.resolved_view.clone());
            self.draw(device, queue, frame, (&resolved.0, &resolved.1));
        }
        changed
    }

    /// Draw `frame` and resolve it into `target`, a view of `resolved`.
    fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &GpuMeshFrame,
        (resolved, target): (&wgpu::Texture, &wgpu::TextureView),
    ) {
        let pipelines = self.pipelines.get_or_insert_with(|| Pipelines::new(device));
        let size = (frame.image.width, frame.image.height);
        if self
            .scratch
            .as_ref()
            .is_none_or(|scratch| scratch.size != size)
        {
            self.scratch = Some(Scratch::new(device, pipelines, size));
            self.peel_targets = None;
        }
        let scratch = self.scratch.as_ref().expect("scratch targets are set");
        let draws_models = frame.draws.iter().any(|draw| draw.source.model.is_some());
        if draws_models && self.models.is_none() {
            self.models = Some(ModelPipelines::new(device, queue, pipelines));
        }
        let models = self.models.as_ref().filter(|_| draws_models);
        // Translucent draws peel when the frame peels (see `peel`), but for
        // filled outlines, which blend from back to front after the peeled
        // layers, and blurred ones, which `glow::draw_soft` draws.
        let peels =
            |draw: &GpuDraw| frame.peel > 0 && draw.translucent && draw.source.fill.is_none();
        let peeled = frame
            .draws
            .iter()
            .any(|draw| peels(draw) && draw.blur.is_none());
        if peeled {
            let peel = self
                .peel
                .get_or_insert_with(|| peel::PeelPipelines::new(device, pipelines));
            if let Some(models) = models
                && frame
                    .draws
                    .iter()
                    .any(|draw| peels(draw) && draw.source.model.is_some())
            {
                peel.add_models(device, pipelines, models);
            }
            if self.peel_targets.is_none() {
                self.peel_targets = Some(peel::PeelTargets::new(
                    device,
                    peel,
                    size,
                    &scratch.depth,
                    &scratch.layer,
                ));
            }
        }
        let glows = frame
            .draws
            .iter()
            .any(|draw| draw.glow.is_some() || draw.shadow.is_some() || draw.blur.is_some());
        if glows {
            let glow = self
                .glow
                .get_or_insert_with(|| glow::GlowPipelines::new(device, pipelines));
            let soft = |draw: &&GpuDraw| {
                draw.glow.is_some() || draw.shadow.is_some() || draw.blur.is_some()
            };
            if let Some(models) = models
                && frame
                    .draws
                    .iter()
                    .filter(soft)
                    .any(|draw| draw.source.model.is_some())
            {
                glow.add_models(device, pipelines, models);
            }
            if frame
                .draws
                .iter()
                .filter(soft)
                .any(|draw| draw.source.fill.is_some())
            {
                glow.add_fills(device, pipelines);
            }
            if self
                .glow_targets
                .as_ref()
                .is_none_or(|targets| targets.size() != size)
            {
                self.glow_targets = Some(glow::GlowTargets::new(device, size));
            }
        }
        let textures = &mut self.textures;
        let samplers = &mut self.samplers;
        let white = self
            .white
            .get_or_insert_with(|| upload_texture(device, queue, &white_texel(), false));
        for draw in frame.draws.iter() {
            self.uploads
                .entry(Arc::as_ptr(&draw.source) as usize)
                .or_insert_with(|| Uploaded {
                    model: draw
                        .source
                        .model
                        .as_ref()
                        .zip(models)
                        .map(|(model, models)| {
                            let vertices =
                                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("gaanim-3d-model-vertices"),
                                    contents: &model.vertices,
                                    usage: wgpu::BufferUsages::VERTEX,
                                });
                            if model.texture.is_none()
                                && model.normal_map.is_none()
                                && model.geometry.is_empty()
                            {
                                return (vertices, models.blank.clone());
                            }
                            // A texture uploads once for every model that
                            // shows it, a sampler once per way of sampling.
                            let mut bound = |texture: &Option<MeshTexture>, linear: bool| {
                                let Some(texture) = texture else {
                                    let view = if linear { &models.flat } else { &models.white };
                                    return (view.clone(), models.sampler.clone());
                                };
                                let view = textures
                                    .entry((texture.image.data.id(), linear))
                                    .or_insert_with(|| {
                                        upload_texture(device, queue, &texture.image, linear)
                                    })
                                    .clone();
                                let sampler = samplers
                                    .entry((texture.wrap, texture.nearest))
                                    .or_insert_with(|| {
                                        texture_sampler(device, texture.wrap, texture.nearest)
                                    })
                                    .clone();
                                (view, sampler)
                            };
                            let base = bound(&model.texture, false);
                            let normal_map = bound(&model.normal_map, true);
                            let geometry = (!model.geometry.is_empty()).then(|| {
                                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("gaanim-3d-model-geometry"),
                                    contents: &model.geometry,
                                    usage: wgpu::BufferUsages::STORAGE,
                                })
                            });
                            let group = model_group(
                                device,
                                &models.texture_layout,
                                (&base.0, &base.1),
                                (&normal_map.0, &normal_map.1),
                                geometry.as_ref().unwrap_or(&models.no_geometry),
                            );
                            (vertices, group)
                        }),
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
                    outline: (!draw.source.outline.is_empty()).then(|| {
                        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("gaanim-3d-outline"),
                            contents: &draw.source.outline,
                            usage: wgpu::BufferUsages::VERTEX,
                        })
                    }),
                    fill: draw.source.fill.as_ref().map(|fill| {
                        let buffer = |label, contents: &[u8]| {
                            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some(label),
                                contents,
                                usage: wgpu::BufferUsages::STORAGE,
                            })
                        };
                        let buffers = [
                            buffer("gaanim-3d-fill-curves", &fill.curves),
                            buffer("gaanim-3d-fill-bands", &fill.bands),
                            buffer("gaanim-3d-fill-paints", &fill.paints),
                            buffer("gaanim-3d-fill-ramps", &fill.ramps),
                        ];
                        // A group per image, which its runs share.
                        let mut groups: HashMap<(u64, bool), wgpu::BindGroup> = HashMap::new();
                        let mut group = |brush: Option<&peniko::ImageBrush>| {
                            let nearest = brush.is_some_and(|brush| {
                                brush.sampler.quality == peniko::ImageQuality::Low
                            });
                            let id = brush.map_or(0, |brush| brush.image.data.id());
                            groups
                                .entry((id, nearest))
                                .or_insert_with(|| {
                                    let view = match brush {
                                        Some(brush) => {
                                            &*textures.entry((id, false)).or_insert_with(|| {
                                                upload_texture(
                                                    device,
                                                    queue,
                                                    &straight_rgba(&brush.image),
                                                    false,
                                                )
                                            })
                                        }
                                        None => &*white,
                                    };
                                    let wrap = [TextureWrap::ClampToEdge; 2];
                                    let sampler = samplers
                                        .entry((wrap, nearest))
                                        .or_insert_with(|| texture_sampler(device, wrap, nearest));
                                    let mut entries: Vec<wgpu::BindGroupEntry> = buffers
                                        .iter()
                                        .enumerate()
                                        .map(|(binding, buffer)| wgpu::BindGroupEntry {
                                            binding: binding as u32,
                                            resource: buffer.as_entire_binding(),
                                        })
                                        .collect();
                                    entries.push(wgpu::BindGroupEntry {
                                        binding: 4,
                                        resource: wgpu::BindingResource::TextureView(view),
                                    });
                                    entries.push(wgpu::BindGroupEntry {
                                        binding: 5,
                                        resource: wgpu::BindingResource::Sampler(sampler),
                                    });
                                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                                        label: Some("gaanim-3d-fill"),
                                        layout: &pipelines.fill_layout,
                                        entries: &entries,
                                    })
                                })
                                .clone()
                        };
                        fill.runs
                            .iter()
                            .map(|(range, brush)| (range.clone(), group(brush.as_ref())))
                            .collect()
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
        let needed = frame.instances.len().max(INSTANCE_BYTES);
        if self
            .instance_buffer
            .as_ref()
            .is_none_or(|(_, capacity)| *capacity < needed)
        {
            let capacity = needed.next_power_of_two();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gaanim-3d-instances"),
                size: capacity as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_buffer = Some((buffer, capacity));
        }
        let (instance_buffer, _) = self
            .instance_buffer
            .as_ref()
            .expect("instance buffer is set");
        if !frame.instances.is_empty() {
            queue.write_buffer(instance_buffer, 0, &frame.instances);
        }
        // Every model binds a palette, a skinned one at its offset, and the
        // frame's morph targets and weights, which the buffer holds too.
        let joints_group = match models {
            Some(models) => {
                let needed = frame.joints.len().max(JOINTS_BYTES);
                if self
                    .joint_buffer
                    .as_ref()
                    .is_none_or(|(_, capacity, _)| *capacity < needed)
                {
                    let capacity = needed.next_power_of_two();
                    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("gaanim-3d-model-joints"),
                        size: capacity as u64,
                        usage: wgpu::BufferUsages::UNIFORM
                            | wgpu::BufferUsages::STORAGE
                            | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("gaanim-3d-model-joints"),
                        layout: &models.joints_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 6,
                                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                    buffer: &buffer,
                                    offset: 0,
                                    size: NonZeroU64::new(JOINTS_BYTES as u64),
                                }),
                            },
                            wgpu::BindGroupEntry {
                                binding: 10,
                                resource: buffer.as_entire_binding(),
                            },
                        ],
                    });
                    self.joint_buffer = Some((buffer, capacity, group));
                }
                let (buffer, _, group) = self.joint_buffer.as_ref().expect("joint buffer is set");
                if !frame.joints.is_empty() {
                    queue.write_buffer(buffer, 0, &frame.joints);
                }
                Some(group)
            }
            None => None,
        };

        // The shadow maps: one texel while the frame casts no shadow.
        let layers = frame.shadow_layers.len();
        let (size, count) = if layers > 0 {
            (frame.shadow_size, layers as u32)
        } else {
            (1, 1)
        };
        if self
            .shadow_maps
            .as_ref()
            .is_none_or(|maps| maps.size != size || maps.layers != count)
        {
            self.shadow_maps = Some(ShadowMaps::new(device, pipelines, size, count));
        }
        let shadow_maps = self.shadow_maps.as_ref().expect("shadow maps are set");
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("gaanim-3d"),
        });
        if layers > 0 {
            let needed = layers * DRAW_STRIDE;
            if self
                .caster_buffer
                .as_ref()
                .is_none_or(|(_, capacity, _)| *capacity < needed)
            {
                let capacity = needed.next_power_of_two();
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("gaanim-3d-casters"),
                    size: capacity as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("gaanim-3d-caster"),
                    layout: &pipelines.caster_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &buffer,
                            offset: 0,
                            size: NonZeroU64::new(CASTER_BYTES as u64),
                        }),
                    }],
                });
                self.caster_buffer = Some((buffer, capacity, group));
            }
            let (caster_buffer, _, caster_group) =
                self.caster_buffer.as_ref().expect("caster buffer is set");
            let mut bytes = Vec::with_capacity(needed);
            for matrix in frame.shadow_layers.iter() {
                push_f32s(&mut bytes, matrix);
                bytes.resize(bytes.len() + DRAW_STRIDE - CASTER_BYTES, 0);
            }
            queue.write_buffer(caster_buffer, 0, &bytes);
            for (layer, target) in shadow_maps.targets.iter().enumerate() {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("gaanim-3d-shadow"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: target,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_bind_group(1, caster_group, &[(layer * DRAW_STRIDE) as u32]);
                pass.set_vertex_buffer(1, instance_buffer.slice(..));
                let mut index = 0;
                while index < frame.draws.len() {
                    let draw = &frame.draws[index];
                    let mut end = index + 1;
                    while end < frame.draws.len() && draw_alike(frame, index, end) {
                        end += 1;
                    }
                    let first = index;
                    index = end;
                    if !draw.casts {
                        continue;
                    }
                    let Some(uploaded) = self.uploads.get(&(Arc::as_ptr(&draw.source) as usize))
                    else {
                        continue;
                    };
                    if let Some(runs) = &uploaded.fill {
                        // A quad per outline, which tests its texels.
                        pass.set_pipeline(&pipelines.shadow_fill);
                        pass.set_bind_group(0, bind_group, &[(first * DRAW_STRIDE) as u32]);
                        pass.set_vertex_buffer(0, uploaded.vertices.slice(..));
                        draw_runs(&mut pass, runs, &draw.instances);
                        continue;
                    }
                    let Some(indices) = &uploaded.indices else {
                        continue;
                    };
                    match (&uploaded.model, models, joints_group) {
                        (Some((vertices, texture)), Some(models), Some(joints)) => {
                            pass.set_pipeline(&models.shadow[usize::from(draw.source.closed)]);
                            pass.set_bind_group(2, texture, &[]);
                            pass.set_bind_group(3, joints, &[draw.joints.unwrap_or(0)]);
                            pass.set_vertex_buffer(2, vertices.slice(..));
                        }
                        (Some(_), ..) => continue,
                        _ => {
                            pass.set_pipeline(&pipelines.shadow[usize::from(draw.source.closed)]);
                        }
                    }
                    pass.set_bind_group(0, bind_group, &[(first * DRAW_STRIDE) as u32]);
                    pass.set_vertex_buffer(0, uploaded.vertices.slice(..));
                    pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..draw.source.count, 0, copies_of(frame, first, end));
                }
            }
        }

        // Draws `first..end`, meshes alike drawn as instances of one: those
        // blurred or not, as `blurred` says, the blurred ones as translucent
        // layers drawn apart (see `glow::draw_soft`).
        let draw_one =
            |pass: &mut wgpu::RenderPass<'_>, first: usize, end: usize, blurred: bool| {
                let draw = &frame.draws[first];
                if draw.blur.is_some() != blurred {
                    return;
                }
                let Some(uploaded) = self.uploads.get(&(Arc::as_ptr(&draw.source) as usize)) else {
                    return;
                };
                if let Some(runs) = &uploaded.fill {
                    // A quad per outline.
                    pass.set_pipeline(&pipelines.fill);
                    pass.set_bind_group(0, bind_group, &[(first * DRAW_STRIDE) as u32]);
                    pass.set_bind_group(1, &shadow_maps.group, &[]);
                    pass.set_vertex_buffer(0, uploaded.vertices.slice(..));
                    draw_runs(pass, runs, &draw.instances);
                    // Then its depth, which the translucent layers read.
                    if !blurred {
                        pass.set_pipeline(&pipelines.fill_depth);
                        draw_runs(pass, runs, &draw.instances);
                    }
                    return;
                }
                let set = match (&uploaded.model, models, joints_group) {
                    (Some((vertices, texture)), Some(models), Some(joints)) => {
                        pass.set_bind_group(2, texture, &[]);
                        pass.set_bind_group(3, joints, &[draw.joints.unwrap_or(0)]);
                        pass.set_vertex_buffer(2, vertices.slice(..));
                        &models.meshes[usize::from(draw.culled)]
                    }
                    (Some(_), ..) => return,
                    _ if draw.source.lines => &pipelines.lines,
                    _ => &pipelines.meshes[usize::from(draw.culled)],
                };
                pass.set_bind_group(0, bind_group, &[(first * DRAW_STRIDE) as u32]);
                pass.set_vertex_buffer(0, uploaded.vertices.slice(..));
                if let Some(indices) = &uploaded.indices {
                    pass.set_vertex_buffer(1, instance_buffer.slice(..));
                    pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                }
                let passes: &[Pass] = if draw.translucent || blurred {
                    &[Pass::Layer, Pass::Blend]
                } else {
                    &[Pass::Opaque]
                };
                for &each in passes {
                    pass.set_bind_group(
                        1,
                        if matches!(each, Pass::Layer) {
                            &scratch.depth_group
                        } else {
                            &shadow_maps.group
                        },
                        &[],
                    );
                    pass.set_pipeline(&set[each as usize]);
                    if draw.source.lines {
                        // A quad per segment shown.
                        if !draw.instances.is_empty() {
                            pass.draw(0..6, draw.instances.clone());
                        }
                    } else {
                        pass.draw_indexed(0..draw.source.count, 0, copies_of(frame, first, end));
                    }
                }
                if draw.outlined
                    && !blurred
                    && let Some(outline) = &uploaded.outline
                {
                    pass.set_pipeline(&pipelines.outline[usize::from(draw.source.closed)]);
                    pass.set_vertex_buffer(2, outline.slice(..));
                    pass.draw_indexed(0..draw.source.count, 0, copies_of(frame, first, end));
                }
            };
        // The translucent draws that blend as layers from back to front: all
        // of them, or those that do not peel.
        let sorted: Vec<usize> = (0..frame.draws.len())
            .filter(|&index| frame.draws[index].translucent && !peels(&frame.draws[index]))
            .collect();
        // Begins a pass over the color: the opaque draws' with their depth, or
        // a translucent layer's with a depth of its own; the last resolves.
        let begin = |encoder: &mut wgpu::CommandEncoder, opaque: bool, last: bool| {
            #[cfg(test)]
            let timestamp_writes =
                self.timing
                    .as_ref()
                    .map(|query_set| wgpu::RenderPassTimestampWrites {
                        query_set,
                        beginning_of_pass_write_index: opaque.then_some(0),
                        end_of_pass_write_index: last.then_some(1),
                    });
            #[cfg(not(test))]
            let timestamp_writes = None;
            encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
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
                            // The translucent layers read the opaque depth,
                            // the glows test their silhouettes against it and
                            // the depth of field blurs by it.
                            store: if opaque && (!last || glows || frame.focus.is_some()) {
                                wgpu::StoreOp::Store
                            } else {
                                wgpu::StoreOp::Discard
                            },
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime()
        };
        // The opaque draws, alike meshes as one; then the peeled layers; then
        // one pass per translucent drawable that blends as a layer, with a
        // layer depth of its own.
        {
            let mut pass = begin(&mut encoder, true, sorted.is_empty() && !peeled);
            let mut next = 0;
            while next < frame.draws.len() && !frame.draws[next].translucent {
                let first = next;
                next += 1;
                while next < frame.draws.len() && draw_alike(frame, first, next) {
                    next += 1;
                }
                draw_one(&mut pass, first, next, false);
            }
        }
        if peeled
            && let (Some(peel), Some(peel_targets)) =
                (self.peel.as_ref(), self.peel_targets.as_ref())
        {
            // Draws the peeled draws with the peel's pipelines, `group` (or
            // the shadow maps, which shade their color) bound to group 1.
            let draw_peel = |pass: &mut wgpu::RenderPass<'_>,
                             which: peel::Peel,
                             group: Option<&wgpu::BindGroup>| {
                for (index, draw) in frame.draws.iter().enumerate() {
                    if !peels(draw) || draw.blur.is_some() {
                        continue;
                    }
                    let Some(uploaded) = self.uploads.get(&(Arc::as_ptr(&draw.source) as usize))
                    else {
                        continue;
                    };
                    let pipeline = match (&uploaded.model, models, joints_group) {
                        (Some((vertices, texture)), Some(_), Some(joints)) => {
                            let Some(pipeline) = peel.model(draw.culled, which) else {
                                continue;
                            };
                            pass.set_bind_group(2, texture, &[]);
                            pass.set_bind_group(3, joints, &[draw.joints.unwrap_or(0)]);
                            pass.set_vertex_buffer(2, vertices.slice(..));
                            pipeline
                        }
                        (Some(_), ..) => continue,
                        _ if draw.source.lines => peel.lines(which),
                        _ => peel.mesh(draw.culled, which),
                    };
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, bind_group, &[(index * DRAW_STRIDE) as u32]);
                    pass.set_bind_group(1, group.unwrap_or(&shadow_maps.group), &[]);
                    pass.set_vertex_buffer(0, uploaded.vertices.slice(..));
                    if draw.source.lines {
                        if !draw.instances.is_empty() {
                            pass.draw(0..6, draw.instances.clone());
                        }
                    } else if let Some(indices) = &uploaded.indices {
                        pass.set_vertex_buffer(1, instance_buffer.slice(..));
                        pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..draw.source.count, 0, draw.copies.clone());
                    }
                }
            };
            let last = sorted.is_empty();
            #[cfg(test)]
            let timestamp_writes = self.timing.as_ref().filter(|_| last).map(|query_set| {
                wgpu::RenderPassTimestampWrites {
                    query_set,
                    beginning_of_pass_write_index: None,
                    end_of_pass_write_index: Some(1),
                }
            });
            #[cfg(not(test))]
            let timestamp_writes = None;
            peel::draw_peeled(
                &mut encoder,
                peel,
                peel_targets,
                frame.peel,
                &scratch.color,
                &scratch.layer,
                last.then_some(target),
                timestamp_writes,
                &draw_peel,
            );
        }
        for (position, &index) in sorted.iter().enumerate() {
            let mut pass = begin(&mut encoder, false, position + 1 == sorted.len());
            draw_one(&mut pass, index, index + 1, false);
        }
        if glows
            && let (Some(glow), Some(targets)) = (self.glow.as_ref(), self.glow_targets.as_mut())
        {
            // Draw `at`'s silhouette with the geometry its color pass binds.
            let silhouette = |pass: &mut wgpu::RenderPass<'_>,
                              glow: &glow::GlowPipelines,
                              draw: &GpuDraw,
                              at: usize| {
                let Some(uploaded) = self.uploads.get(&(Arc::as_ptr(&draw.source) as usize)) else {
                    return;
                };
                if let Some(runs) = &uploaded.fill {
                    let Some(pipeline) = glow.fill() else {
                        return;
                    };
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, bind_group, &[(at * DRAW_STRIDE) as u32]);
                    pass.set_bind_group(1, &shadow_maps.group, &[]);
                    pass.set_vertex_buffer(0, uploaded.vertices.slice(..));
                    draw_runs(pass, runs, &draw.instances);
                    return;
                }
                let pipeline = match (&uploaded.model, models, joints_group) {
                    (Some((vertices, texture)), Some(_), Some(joints)) => {
                        let Some(pipeline) = glow.model(draw.culled) else {
                            return;
                        };
                        pass.set_bind_group(2, texture, &[]);
                        pass.set_bind_group(3, joints, &[draw.joints.unwrap_or(0)]);
                        pass.set_vertex_buffer(2, vertices.slice(..));
                        pipeline
                    }
                    (Some(_), ..) => return,
                    _ if draw.source.lines => glow.lines(),
                    _ => glow.mesh(draw.culled),
                };
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, bind_group, &[(at * DRAW_STRIDE) as u32]);
                pass.set_bind_group(1, &shadow_maps.group, &[]);
                pass.set_vertex_buffer(0, uploaded.vertices.slice(..));
                if draw.source.lines {
                    if !draw.instances.is_empty() {
                        pass.draw(0..6, draw.instances.clone());
                    }
                } else if let Some(indices) = &uploaded.indices {
                    pass.set_vertex_buffer(1, instance_buffer.slice(..));
                    pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..draw.source.count, 0, draw.copies.clone());
                }
            };
            glow::draw_soft(
                device,
                queue,
                &mut encoder,
                glow,
                targets,
                &frame.draws,
                &|first, second| draw_alike(frame, first, second),
                glow::SoftTargets {
                    depth: &scratch.depth,
                    layer: &scratch.layer,
                    target,
                },
                &silhouette,
                &|pass: &mut wgpu::RenderPass<'_>, first, end| draw_one(pass, first, end, true),
            );
        }
        // Last, the image blurs by depth, glows and all.
        if let Some(focus) = &frame.focus {
            let pipelines = self
                .focus
                .get_or_insert_with(|| focus::FocusPipelines::new(device));
            if self
                .focus_targets
                .as_ref()
                .is_none_or(|targets| targets.size() != (frame.image.width, frame.image.height))
            {
                self.focus_targets = Some(focus::FocusTargets::new(
                    device,
                    (frame.image.width, frame.image.height),
                ));
            }
            let targets = self.focus_targets.as_ref().expect("focus targets are set");
            focus::draw_focus(
                device,
                &mut encoder,
                pipelines,
                targets,
                focus,
                resolved,
                target,
                &scratch.depth,
            );
        }
        queue.submit(Some(encoder.finish()));
    }
}

/// The shader: [`crate::three_d`]'s projection and illustrative shading.
const SHADER: &str = r#"
struct Light {
    // Position, or direction toward a directional light, and 1 for one.
    position: vec4<f32>,
    // Linear color, and its shadow's first layer plus one (0 for none).
    color: vec4<f32>,
    // A spot light's direction, the way it shines, and the cosine of its
    // cone's edge; zero for other lights.
    spot: vec4<f32>,
    // The cosine of the angle within which a spot light shines fully.
    cone: vec4<f32>,
}

fn is_spot(light: Light) -> bool {
    return dot(light.spot.xyz, light.spot.xyz) > 0.5;
}

// A shadow map layer: its view-projection, the size of its texels (in scene
// units, or per scene unit away from the light), 1 under perspective, and
// the distances from the light its depths span.
struct Shadow {
    view_proj: mat4x4<f32>,
    params: vec4<f32>,
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
    // 1 when the fixed key light shines, the count of lights, and the
    // first layer of the key light's shadow plus one (0 for none).
    params: vec4<f32>,
    // 1 over the texels on a side of a shadow map, and the radius of the
    // edge of shadows in texels where an object touches what it shadows.
    shadow: vec4<f32>,
    lights: array<Light, 8>,
    shadows: array<Shadow, 24>,
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
    // Half the width of lines in pixels, and the segments shown, from and
    // to, fractional at the ends.
    line: vec4<f32>,
    // The linear color of the outline; its width in pixels, 1 for an open
    // mesh, and 1 for a sheet (see `SHEET_SHADE`).
    outline: vec4<f32>,
    outline_size: vec4<f32>,
    // An imported model's alpha cutoff (0 for none); 1 when it has a
    // texture, plus 2 when a palette skins it and 4 when its alpha blends;
    // and a morphed one's first target and weight among the frame's (see
    // `morph_weights` in `MODEL_SHADER`) and how many it weighs.
    imported: vec4<f32>,
}

struct Caster {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<uniform> draw: Draw;
// The opaque depth, which translucent layers lie in front of.
@group(1) @binding(0) var opaque_depth: texture_depth_multisampled_2d;
// The shadow maps, which shading reads.
@group(1) @binding(1) var shadow_maps: texture_depth_2d_array;
@group(1) @binding(2) var shadow_sampler: sampler_comparison;
// The shadow map layer meshes are drawn into.
@group(1) @binding(3) var<uniform> caster: Caster;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    // The draw's instance data: its model matrix, the columns of its normal
    // matrix, and its linear base color.
    @location(3) model_x: vec4<f32>,
    @location(4) model_y: vec4<f32>,
    @location(5) model_z: vec4<f32>,
    @location(6) model_w: vec4<f32>,
    @location(7) normal_x: vec4<f32>,
    @location(8) normal_y: vec4<f32>,
    @location(9) normal_z: vec4<f32>,
    @location(10) base: vec4<f32>,
}

struct OutlineIn {
    @location(0) position: vec3<f32>,
    @location(3) model_x: vec4<f32>,
    @location(4) model_y: vec4<f32>,
    @location(5) model_z: vec4<f32>,
    @location(6) model_w: vec4<f32>,
    @location(7) normal_x: vec4<f32>,
    @location(8) normal_y: vec4<f32>,
    @location(9) normal_z: vec4<f32>,
    // The vertex's outline direction, the mean normal around its place.
    @location(11) direction: vec3<f32>,
}

struct ShadowIn {
    @location(0) position: vec3<f32>,
    @location(3) model_x: vec4<f32>,
    @location(4) model_y: vec4<f32>,
    @location(5) model_z: vec4<f32>,
    @location(6) model_w: vec4<f32>,
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

// How far into the light a sheet's shadows fade in, in the cosine of the
// light's angle to the side shaded. A sheet bounds no volume, so it shadows
// the side it turns from the light, where a solid's far side, which casts,
// does not: the wrapped light past its terminator would end at a shadow
// stepped by the shadow map's texels. It fades out there instead, as on a
// solid.
const SHEET_SHADE: f32 = 0.2;

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

// Points around a shadow map lookup, within a unit disc, which soften the
// edge of shadows.
const SHADOW_TAPS = array<vec2<f32>, 12>(
    vec2<f32>(-0.326, -0.406),
    vec2<f32>(-0.840, -0.074),
    vec2<f32>(-0.696, 0.457),
    vec2<f32>(-0.203, 0.621),
    vec2<f32>(0.962, -0.195),
    vec2<f32>(0.473, -0.480),
    vec2<f32>(0.519, 0.767),
    vec2<f32>(0.185, -0.893),
    vec2<f32>(0.507, 0.064),
    vec2<f32>(0.896, 0.412),
    vec2<f32>(-0.322, -0.933),
    vec2<f32>(-0.792, -0.598),
);

// How wide the edge of a shadow grows per unit of distance between the
// object that casts it and what it falls on, under a directional light (the
// key light's included), as the sun's disc spreads it.
const SUN_SPREAD: f32 = 0.03;
// The radius of a point light, in scene units, which spreads its shadows'
// edges the same way.
const LAMP_RADIUS: f32 = 0.12;
// The widest edge of a shadow, in texels, and so how far around a point the
// lookup searches for what casts it.
const SHADOW_SEARCH: f32 = 16.0;

// How much of `light` reaches `point`, whose surface faces `normal`: 1 where
// no opaque mesh lies between them, 0 in its shadow, between at the edge,
// which is sharp where the object touches the surface and widens as they
// part.
fn shadowed(light: Light, point: vec3<f32>, normal: vec3<f32>, pixel: vec2<f32>) -> f32 {
    if light.color.w < 0.5 {
        return 1.0;
    }
    var layer = u32(light.color.w - 0.5);
    var toward = light.position.xyz;
    if light.position.w < 0.5 {
        let ray = point - light.position.xyz;
        // A point light's cube face: the one its ray to the point leaves by;
        // a spot light's one map.
        if !is_spot(light) {
            let size = abs(ray);
            var face = select(5u, 4u, ray.z > 0.0);
            if size.x >= size.y && size.x >= size.z {
                face = select(1u, 0u, ray.x > 0.0);
            } else if size.y >= size.z {
                face = select(3u, 2u, ray.y > 0.0);
            }
            layer += face;
        }
        toward = -ray;
    }
    let shadow = frame.shadows[layer];
    var texel = shadow.params.x;
    if shadow.params.y > 0.5 {
        texel *= distance(point, light.position.xyz);
    }
    // Off the surface, on the side the light comes from, by a texel and a
    // half: a surface does not shadow itself. A closed mesh casts with its
    // faces turned away from the light, so its far side, moved toward the
    // light into the mesh, lies in front of what it casts: its own shadow
    // leaves the wrapped light past the terminator alone.
    var side = normal;
    if dot(side, toward) < 0.0 {
        side = -side;
    }
    let clip = shadow.view_proj * vec4<f32>(point + side * texel * 1.5, 1.0);
    if clip.w <= 0.0 {
        return 1.0;
    }
    let ndc = clip.xyz / clip.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if ndc.z < 0.0 || ndc.z > 1.0 || any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) {
        return 1.0;
    }
    var taps = SHADOW_TAPS;
    // The taps turn by a different angle at each pixel (interleaved
    // gradient noise), so that a wide edge shades smoothly instead of in
    // steps of the same pattern.
    let turn = 6.2831853 * fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
    let rotation = mat2x2<f32>(cos(turn), sin(turn), -sin(turn), cos(turn));
    for (var i = 0u; i < 12u; i = i + 1u) {
        taps[i] = rotation * taps[i];
    }
    // The mean depth of what casts the shadow around the point.
    let size = vec2<f32>(textureDimensions(shadow_maps));
    var blockers = 0.0;
    var found = 0.0;
    for (var i = 0u; i < 12u; i = i + 1u) {
        let at = (uv + taps[i] * SHADOW_SEARCH * frame.shadow.x) * size;
        let texel_at = vec2<i32>(clamp(at, vec2<f32>(0.0), size - 1.0));
        let stored = textureLoad(shadow_maps, texel_at, i32(layer), 0);
        if stored < ndc.z {
            blockers += stored;
            found += 1.0;
        }
    }
    if found < 0.5 {
        return 1.0;
    }
    let blocker = blockers / found;
    // The width of the edge, in texels, from the distance between them.
    var penumbra: f32;
    let near = shadow.params.z;
    let far = shadow.params.w;
    if shadow.params.y > 0.5 {
        let receiver_distance = far * near / (far - ndc.z * (far - near));
        let blocker_distance = far * near / (far - blocker * (far - near));
        penumbra = LAMP_RADIUS * (receiver_distance - blocker_distance) / blocker_distance / texel;
    } else {
        penumbra = (ndc.z - blocker) * (far - near) * SUN_SPREAD / texel;
    }
    let reach = clamp(penumbra, frame.shadow.y, SHADOW_SEARCH) * frame.shadow.x;
    var lit = 0.0;
    for (var i = 0u; i < 12u; i = i + 1u) {
        lit += textureSampleCompareLevel(
            shadow_maps,
            shadow_sampler,
            uv + taps[i] * reach,
            i32(layer),
            ndc.z,
        );
    }
    return lit / 12.0;
}

// The lit color of a surface's point: `normal` shades it, and `face`, the
// normal of the triangle it lies on, decides where shadows fall.
fn shade(
    base: vec4<f32>,
    normal: vec3<f32>,
    face: vec3<f32>,
    viewer: vec3<f32>,
    point: vec3<f32>,
    pixel: vec2<f32>,
) -> vec4<f32> {
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
        var light = Light(
            vec4<f32>(KEY_LIGHT, 1.0),
            vec4<f32>(1.0, 1.0, 1.0, frame.params.z),
            vec4<f32>(0.0),
            vec4<f32>(0.0),
        );
        if !fixed {
            light = frame.lights[i];
        }
        var toward_light = light.position.xyz;
        if light.position.w < 0.5 {
            toward_light = light.position.xyz - point;
        }
        toward_light = normalize_or_zero(toward_light);
        var lit = shadowed(light, point, face, pixel);
        if draw.outline_size.z > 0.5 {
            lit = mix(1.0, lit, smoothstep(0.0, SHEET_SHADE, dot(normal, toward_light)));
        }
        let key = max((dot(normal, toward_light) + KEY_WRAP) / (1.0 + KEY_WRAP), 0.0);
        let half = normalize_or_zero(toward_light + viewer);
        let shine = pow(max(dot(normal, half), 0.0), shininess) * polish * min(key, 1.0);
        // A spot light fades out toward the edge of its cone.
        if is_spot(light) {
            lit *= smoothstep(light.spot.w, light.cone.x, dot(-toward_light, light.spot.xyz));
        }
        keyed += KEY_STRENGTH * key * light.color.rgb * lit;
        highlight += shine * light.color.rgb * lit;
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
    let model = mat4x4<f32>(in.model_x, in.model_y, in.model_z, in.model_w);
    let normal = mat3x3<f32>(in.normal_x.xyz, in.normal_y.xyz, in.normal_z.xyz);
    let world = (model * vec4<f32>(in.position, 1.0)).xyz;
    var out: VertexOut;
    out.clip = image_clip(world);
    out.world = world;
    out.normal = normal * in.normal;
    out.color = in.base;
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
        color = shade(color, normal, face, viewer, in.world, in.clip.xy);
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
fn vertex_line(
    @builtin(vertex_index) index: u32,
    @builtin(instance_index) segment: u32,
    in: LineIn,
) -> LineOut {
    var out: LineOut;
    // The part of the segment shown, when a reveal ends within it.
    let first = clamp(draw.line.y - f32(segment), 0.0, 1.0);
    let last = clamp(draw.line.z - f32(segment), 0.0, 1.0);
    if last <= first {
        out.clip = vec4<f32>(0.0, 0.0, -1.0, 1.0);
        return out;
    }
    let head = mix(in.start.xyz, in.end.xyz, first);
    let tail = mix(in.start.xyz, in.end.xyz, last);
    var start = image_clip((draw.model * vec4<f32>(head, 1.0)).xyz);
    var end = image_clip((draw.model * vec4<f32>(tail, 1.0)).xyz);
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
    let reach = draw.line.x + 1.0;
    var corners = CORNERS;
    let corner = corners[index];
    var pixel = s0 - along * reach;
    var clip = start;
    var color = mix(in.start_color, in.end_color, first);
    if corner.x > 0.5 {
        pixel = s1 + along * reach;
        clip = end;
        color = mix(in.start_color, in.end_color, last);
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
    let coverage = clamp(draw.line.x + 0.5 - distance, 0.0, 1.0);
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

// A vertex of a mesh's outline: pushed out along its outline direction by
// the outline's width, measured across the line of sight at its depth, and
// for an open mesh back by as much too, so that the outline shows only
// beyond its border.
@vertex
fn vertex_outline(in: OutlineIn) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(in.model_x, in.model_y, in.model_z, in.model_w);
    let normal = mat3x3<f32>(in.normal_x.xyz, in.normal_y.xyz, in.normal_z.xyz);
    let world = (model * vec4<f32>(in.position, 1.0)).xyz;
    // Through the normal matrix, keeping the push's own length.
    let direction = normalize_or_zero(normal * in.direction) * length(in.direction);
    let toward = toward_viewer(world);
    var across = cross(toward, vec3<f32>(0.0, 1.0, 0.0));
    if dot(across, across) < 1e-6 {
        across = cross(toward, vec3<f32>(1.0, 0.0, 0.0));
    }
    across = normalize(across);
    // The size of a pixel in the scene at the vertex's depth.
    let half_size = vec2<f32>(frame.ndc_x.w, frame.ndc_y.w);
    let clip = image_clip(world);
    let side = image_clip(world + across * 0.01);
    let pixels = length(side.xy / side.w * half_size - clip.xy / clip.w * half_size);
    var pixel_size = 0.0;
    if pixels > 1e-6 {
        pixel_size = 0.01 / pixels;
    }
    let reach = draw.outline_size.x * pixel_size;
    var shifted = world + direction * reach;
    if draw.outline_size.y > 0.5 {
        // An open mesh's border, within its plane, and the mesh behind
        // itself: only what lies beyond the border shows.
        shifted -= toward * reach;
    }
    return image_clip(shifted);
}

@fragment
fn fragment_outline() -> @location(0) vec4<f32> {
    return output(draw.outline);
}

// An opaque mesh's depth in the shadow map layer it casts into.
@vertex
fn vertex_shadow(in: ShadowIn) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(in.model_x, in.model_y, in.model_z, in.model_w);
    return caster.view_proj * (model * vec4<f32>(in.position, 1.0));
}
"#;

/// The shader of imported models ([`ModelPipelines`]), after [`SHADER`]: a
/// base-color texture and a normal map read through each vertex's UV,
/// alpha cut out or blended as the material says, morph targets added by
/// their weights and a skin's palette blended by each vertex's weights.
const MODEL_SHADER: &str = r#"
@group(2) @binding(4) var model_texture: texture_2d<f32>;
@group(2) @binding(5) var model_sampler: sampler;
// The normal map, in linear texels.
@group(2) @binding(7) var normal_texture: texture_2d<f32>;
@group(2) @binding(8) var normal_sampler: sampler;
// What a model's vertices read by their index (see `model_geometry`): a
// header, holding the normal map's scale, 1 when tangents follow, the
// vertices and the morph targets; a tangent per vertex for a normal map;
// then, for each target and vertex, its displacement and the change of its
// normal.
@group(2) @binding(9) var<storage, read> geometry: array<vec4<f32>>;
// The palette of a skinned draw, bound at its offset in the frame's joints.
@group(3) @binding(6) var<uniform> joints: array<mat4x4<f32>, 256>;
// The morph targets the frame's draws weigh, each with its weight: a draw's
// run starts at `draw.imported.z` and holds `draw.imported.w` of them.
@group(3) @binding(10) var<storage, read> morph_weights: array<vec2<f32>>;

struct ModelIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) model_x: vec4<f32>,
    @location(4) model_y: vec4<f32>,
    @location(5) model_z: vec4<f32>,
    @location(6) model_w: vec4<f32>,
    @location(7) normal_x: vec4<f32>,
    @location(8) normal_y: vec4<f32>,
    @location(9) normal_z: vec4<f32>,
    @location(10) base: vec4<f32>,
    @location(12) uv: vec2<f32>,
    @location(13) joints: vec4<u32>,
    @location(14) weights: vec4<f32>,
    @builtin(vertex_index) vertex_id: u32,
}

struct ModelShadowIn {
    @location(0) position: vec3<f32>,
    @location(3) model_x: vec4<f32>,
    @location(4) model_y: vec4<f32>,
    @location(5) model_z: vec4<f32>,
    @location(6) model_w: vec4<f32>,
    @location(12) uv: vec2<f32>,
    @location(13) joints: vec4<u32>,
    @location(14) weights: vec4<f32>,
    @builtin(vertex_index) vertex_id: u32,
}

struct ModelOut {
    @invariant @builtin(position) clip: vec4<f32>,
    // Linear base color of the vertex, which its texel multiplies.
    @location(0) color: vec4<f32>,
    @location(1) world: vec3<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) uv: vec2<f32>,
    // A normal-mapped vertex's tangent in the world, and the sign of its
    // bitangent; zero without a normal map.
    @location(4) tangent: vec4<f32>,
}

struct ModelShadowOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// What `Draw.imported.y` says of a model, a bit each: that it has a
// texture, that a palette skins it and that its alpha blends.
const TEXTURED: u32 = 1u;
const SKINNED: u32 = 2u;
const BLENDED: u32 = 4u;

// Whether the draw's model is `flag`.
fn imported(flag: u32) -> bool {
    return (u32(draw.imported.y + 0.5) & flag) != 0u;
}

// Where a skinned vertex's joints take it, in the mesh's space: their
// matrices blended by its weights.
fn skin(joint: vec4<u32>, weight: vec4<f32>) -> mat4x4<f32> {
    let identity = mat4x4<f32>(
        vec4<f32>(1.0, 0.0, 0.0, 0.0),
        vec4<f32>(0.0, 1.0, 0.0, 0.0),
        vec4<f32>(0.0, 0.0, 1.0, 0.0),
        vec4<f32>(0.0, 0.0, 0.0, 1.0),
    );
    if !imported(SKINNED) || weight.x + weight.y + weight.z + weight.w < 1e-6 {
        return identity;
    }
    let j = min(joint, vec4<u32>(255u));
    return joints[j.x] * weight.x + joints[j.y] * weight.y
        + joints[j.z] * weight.z + joints[j.w] * weight.w;
}

struct Morphed {
    position: vec3<f32>,
    normal: vec3<f32>,
}

// Vertex `vertex_id` at `position` with `normal`, moved by the morph
// targets the draw weighs, before its skin moves it.
fn morphed(vertex_id: u32, position: vec3<f32>, normal: vec3<f32>) -> Morphed {
    var out = Morphed(position, normal);
    let weighed = u32(draw.imported.w);
    if weighed == 0u {
        return out;
    }
    let header = geometry[0];
    let vertices = u32(header.z);
    let shapes = u32(header.w);
    if vertex_id >= vertices {
        return out;
    }
    // The targets' rows follow the header and the tangents.
    let rows_at = 1u + select(0u, vertices, header.y > 0.5);
    let run = u32(draw.imported.z);
    for (var index = 0u; index < weighed; index += 1u) {
        let entry = morph_weights[run + index];
        let shape = u32(entry.x);
        if shape < shapes {
            let row = rows_at + (shape * vertices + vertex_id) * 2u;
            out.position += entry.y * geometry[row].xyz;
            out.normal += entry.y * geometry[row + 1u].xyz;
        }
    }
    return out;
}

// The tangent of normal-mapped vertex `vertex_id` in the mesh's space;
// zero without a normal map.
fn tangent_of(vertex_id: u32) -> vec4<f32> {
    let header = geometry[0];
    if header.y < 0.5 || vertex_id >= u32(header.z) {
        return vec4<f32>(0.0);
    }
    return geometry[1u + vertex_id];
}

// The normal map's texel at `uv`, from -1 to 1 along the tangent, the
// bitangent and the normal, its tilt scaled; straight along the normal
// without a map.
fn normal_texel(uv: vec2<f32>) -> vec3<f32> {
    let texel = textureSample(normal_texture, normal_sampler, uv).xyz * 2.0 - 1.0;
    let header = geometry[0];
    if header.y < 0.5 {
        return vec3<f32>(0.0, 0.0, 1.0);
    }
    return vec3<f32>(texel.xy * header.x, texel.z);
}

// Unit `normal` tilted by a normal map's texel `bump` toward `tangent`,
// made perpendicular to it, and its bitangent.
fn bumped(normal: vec3<f32>, tangent: vec4<f32>, bump: vec3<f32>) -> vec3<f32> {
    let along = tangent.xyz - normal * dot(normal, tangent.xyz);
    if dot(along, along) < 1e-12 {
        return normal;
    }
    let u = normalize(along);
    let v = cross(normal, u) * select(1.0, -1.0, tangent.w < 0.0);
    let tilted = normalize_or_zero(u * bump.x + v * bump.y + normal * bump.z);
    if dot(tilted, tilted) < 0.5 {
        return normal;
    }
    return tilted;
}

@vertex
fn vertex_model(in: ModelIn) -> ModelOut {
    let model = mat4x4<f32>(in.model_x, in.model_y, in.model_z, in.model_w);
    let normal = mat3x3<f32>(in.normal_x.xyz, in.normal_y.xyz, in.normal_z.xyz);
    let skinned = skin(in.joints, in.weights);
    let shaped = morphed(in.vertex_id, in.position, in.normal);
    let world = (model * (skinned * vec4<f32>(shaped.position, 1.0))).xyz;
    var out: ModelOut;
    out.clip = image_clip(world);
    out.world = world;
    out.normal = normal * (skinned * vec4<f32>(shaped.normal, 0.0)).xyz;
    // A tangent lies along the surface, which the model matrix carries; a
    // mirroring matrix turns its bitangent around.
    let tangent = tangent_of(in.vertex_id);
    let mirrored = determinant(mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz)) < 0.0;
    out.tangent = vec4<f32>(
        (model * (skinned * vec4<f32>(tangent.xyz, 0.0))).xyz,
        select(tangent.w, -tangent.w, mirrored),
    );
    // A model's vertex colors tint its base color, as glTF multiplies them.
    out.color = in.base;
    if draw.material.w > 0.5 {
        out.color = in.base * in.color;
    }
    out.uv = in.uv;
    return out;
}

// The base color of a model's pixel: its vertex's, times its texel.
fn model_color(in: ModelOut) -> vec4<f32> {
    var color = in.color;
    if imported(TEXTURED) {
        color = color * textureSample(model_texture, model_sampler, in.uv);
    }
    return color;
}

// The alpha below which a model's pixel is cut away: its cutoff, a texel's
// worth for blended alpha, which then hides nothing behind it, and none for
// an opaque surface.
fn model_cut() -> f32 {
    if draw.imported.x > 0.0 {
        return draw.imported.x;
    }
    if imported(BLENDED) {
        return 1.0 / 255.0;
    }
    return 0.0;
}

@fragment
fn fragment_model(in: ModelOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    // Derivatives and the texture lookup before any pixel is discarded.
    let viewer = toward_viewer(in.world);
    var face = normalize_or_zero(cross(dpdx(in.world), dpdy(in.world)));
    if dot(face, viewer) < 0.0 {
        face = -face;
    }
    var color = model_color(in);
    let bump = normal_texel(in.uv);
    if color.a < draw.imported.x {
        discard;
    }
    if !imported(BLENDED) {
        color.a = 1.0;
    }
    if draw.material.z > 0.5 {
        var normal = normalize_or_zero(in.normal);
        if dot(normal, normal) < 0.5 {
            normal = face;
        } else {
            normal = bumped(normal, in.tangent, bump);
            // The back face of a two-sided surface faces the other way.
            if !front {
                normal = -normal;
            }
        }
        color = shade(color, normal, face, viewer, in.world, in.clip.xy);
    }
    return output(color);
}

@fragment
fn fragment_model_layer(in: ModelOut, @builtin(sample_index) sample: u32) -> @location(0) vec4<f32> {
    let alpha = model_color(in).a;
    if hidden(in.clip, sample) || alpha < model_cut() {
        discard;
    }
    return vec4<f32>(0.0);
}

// A model's depth in the shadow map layer it casts into.
@vertex
fn vertex_model_shadow(in: ModelShadowIn) -> ModelShadowOut {
    let model = mat4x4<f32>(in.model_x, in.model_y, in.model_z, in.model_w);
    let skinned = skin(in.joints, in.weights);
    let shaped = morphed(in.vertex_id, in.position, vec3<f32>(0.0));
    var out: ModelShadowOut;
    out.clip = caster.view_proj * (model * (skinned * vec4<f32>(shaped.position, 1.0)));
    out.uv = in.uv;
    return out;
}

// Texels cut away cast no shadow.
@fragment
fn fragment_model_shadow(in: ModelShadowOut) {
    var alpha = draw.base.a;
    if imported(TEXTURED) {
        alpha = alpha * textureSample(model_texture, model_sampler, in.uv).a;
    }
    if alpha < draw.imported.x {
        discard;
    }
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
            surface: None,
            blend: None,
        }
    }

    #[test]
    fn orthographic_projection_keeps_the_canvas_in_scene_units() {
        let (ndc_to_canvas, line_width) =
            canvas_projection(&camera(Projection::Orthographic { zoom: 1.0 })).unwrap();
        // The NDC square spans the 16 x 9 frame around the camera.
        let corner = ndc_to_canvas * kurbo::Point::new(1.0, 1.0);
        assert!((corner.x - 8.0).abs() < 1e-9 && (corner.y - 4.5).abs() < 1e-9);
        // 2 pixels of a 1080-pixel-tall output, in the 9-unit-tall frame.
        assert!((line_width - 9.0 * 2.0 / 1080.0).abs() < 1e-12);
    }

    #[test]
    fn meshes_keep_their_valid_triangles_with_unit_normals() {
        let mut mesh = quad(peniko::Color::WHITE);
        mesh.indices.extend([0, 1, 9]);
        let source = MeshSource::mesh(&mesh, true, false).unwrap();
        assert_eq!(source.count, 6);
        assert_eq!(source.vertices.len(), 4 * MESH_VERTEX_BYTES);
        // The first vertex's normal, generated from the faces, faces +z.
        let normal: Vec<f32> = source.vertices[12..24]
            .chunks(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        assert_eq!(normal, [0.0, 0.0, 1.0]);
        mesh.indices.clear();
        assert!(MeshSource::mesh(&mesh, true, false).is_none());
    }

    #[test]
    fn closed_meshes_are_told_apart_from_open_ones() {
        assert!(
            !MeshSource::mesh(&quad(peniko::Color::WHITE), true, false)
                .unwrap()
                .closed
        );
        // A tetrahedron whose faces have vertices of their own, as flat
        // shading and seams make them, encloses its volume.
        let corners = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let faces = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let tetrahedron = TriangleMeshData {
            vertices: faces
                .iter()
                .flatten()
                .map(|&corner| corners[corner])
                .collect(),
            indices: (0..12).collect(),
            ..quad(peniko::Color::WHITE)
        };
        let solid = MeshSource::mesh(&tetrahedron, true, false).unwrap();
        assert!(solid.closed && !solid.sheet);
        // Without a face it is open, and holds nothing.
        let mut open = tetrahedron.clone();
        open.indices.truncate(9);
        let open = MeshSource::mesh(&open, true, false).unwrap();
        assert!(!open.closed && open.sheet);
        // Both sides of a quad, far from the origin, are closed but hold
        // nothing; unlit, it is neither.
        let mut sides = quad(peniko::Color::WHITE);
        for vertex in &mut sides.vertices {
            vertex[0] += 1000.0;
        }
        let back: Vec<u32> = sides
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|[a, b, c]| [a + 4, c + 4, b + 4])
            .collect();
        sides.vertices.extend_from_within(..);
        sides.indices.extend(back);
        let two_sided = MeshSource::mesh(&sides, true, false).unwrap();
        assert!(two_sided.closed && two_sided.sheet);
        assert!(!MeshSource::mesh(&sides, false, false).unwrap().sheet);
        // Copies of a corner computed with different rounding still meet,
        // even across a cell of the grid that welds them (1e-5 here).
        let mut positions: Vec<DVec3> = tetrahedron
            .vertices
            .iter()
            .map(|v| DVec3::new(v[0].into(), v[1].into(), v[2].into()))
            .collect();
        for (index, corner) in faces.iter().flatten().enumerate() {
            if *corner == 1 && index % 2 == 0 {
                positions[index].x -= 1e-8;
            }
        }
        assert!(encloses(&positions, &tetrahedron.indices, 1.0));
    }

    #[test]
    fn line_lists_pair_their_points_as_strips_indices_or_pairs() {
        let lines = |strip, indices| LineListData {
            points: vec![[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            indices,
            strip,
            color: peniko::Color::WHITE,
            colors: None,
            revealed: None,
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
        let mut moved = quad(peniko::Color::WHITE);
        moved.vertices[0] = [-2.0, -1.0, 0.0];
        let mut sources = MeshSources::default();
        let mut built = |revision, mesh: &TriangleMeshData| {
            sources
                .get(7, Kind::Mesh, revision, None, || {
                    MeshSource::mesh(mesh, true, false)
                })
                .unwrap()
        };
        let first = built(1, &mesh);
        // The same revision keeps the source without building it again.
        assert!(Arc::ptr_eq(&first, &built(1, &moved)));
        // A new revision builds it again: the same mesh keeps its source,
        // which drawables alike share; another mesh gets a new one.
        assert!(Arc::ptr_eq(&first, &built(2, &mesh)));
        assert!(!Arc::ptr_eq(&first, &built(3, &moved)));
        sources.end_frame();
        sources.end_frame();
        assert!(sources.sources.is_empty());
        drop(first);
        sources.end_frame();
        assert!(sources.shared.is_empty());
    }

    #[test]
    fn classes_draw_into_images_of_their_own_with_translucent_draws_last() {
        let camera = camera(Projection::Orthographic { zoom: 1.0 });
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let transform = GlobalSpatialTransform::default();
        let (opaque, faded) = (GlobalOpacity(1.0), GlobalOpacity(0.5));
        let red = quad(peniko::Color::from_rgb8(255, 0, 0));
        let content = |opacity| Content {
            line_width: None,
            line_range: None,
            fill: None,
            joints: None,
            transform: &transform,
            opacity,
            mesh: Some(&red),
            lines: None,
            material: None,
            glow: None,
            shadow: None,
            blur: None,
            instances: None,
            morph: None,
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

    fn lit_quad() -> TriangleMeshData {
        TriangleMeshData {
            material: Some(Material3D::default()),
            ..quad(peniko::Color::WHITE)
        }
    }

    fn placed(scale: f64, translation: DVec3) -> GlobalSpatialTransform {
        GlobalSpatialTransform {
            mat4: DMat4::from_scale_rotation_translation(
                DVec3::splat(scale),
                Default::default(),
                translation,
            ),
            ..Default::default()
        }
    }

    #[test]
    fn shadows_cover_the_meshes_from_the_first_lights() {
        let camera = camera(Projection::Orthographic { zoom: 1.0 });
        let floor = lit_quad();
        let (transform, opacity) = (placed(2.0, DVec3::ZERO), GlobalOpacity(1.0));
        let mut sources = MeshSources::default();
        let mut meshes =
            GpuMeshes::new(&mut sources, (1920, 1080), camera.to_vello_transform(), 0).unwrap();
        meshes.push(
            0,
            [(1, 1), (1, 1)],
            [None, None],
            &Content {
                transform: &transform,
                opacity: &opacity,
                mesh: Some(&floor),
                lines: None,
                material: floor.material.as_ref(),
                line_width: None,
                line_range: None,
                fill: None,
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
            },
        );
        // The fixed key light casts one map, at full size.
        let mut illumination = Illumination::default();
        let key = shadows(&illumination, &meshes.pending);
        assert_eq!(
            (key.key, key.layers.len(), key.size),
            (Some(0), 1, SHADOW_SIZE)
        );
        // The floor's middle lies in the map, its corner too.
        for point in [DVec3::ZERO, DVec3::new(2.0, 2.0, 0.0)] {
            let ndc = key.layers[0].view_proj.project_point3(point);
            assert!(ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0, "{ndc:?}");
            assert!((0.0..=1.0).contains(&ndc.z), "{ndc:?}");
        }
        // A point light casts a cube of six, a directional one a map: three
        // lights cast 13 smaller maps.
        let light = |position, directional| Light {
            position,
            directional,
            color: [1.0; 3],
            spot: None,
        };
        illumination.lights = vec![
            light(DVec3::new(0.0, 0.0, 3.0), false),
            light(DVec3::new(1.0, 1.0, 1.0), true),
            light(DVec3::new(0.0, 3.0, 0.0), false),
        ];
        let lit = shadows(&illumination, &meshes.pending);
        assert_eq!(lit.key, None);
        assert_eq!(&lit.lights[..3], [Some(0), Some(6), Some(7)]);
        assert_eq!((lit.layers.len(), lit.size), (13, SHADOW_SIZE_MANY));
        // The floor beneath the point light lies in the face looking down.
        let below = lit.layers[5]
            .view_proj
            .project_point3(DVec3::new(0.5, 0.5, 0.0));
        assert!(below.x.abs() <= 1.0 && below.y.abs() <= 1.0, "{below:?}");
        illumination.lighting.shadows = false;
        assert!(shadows(&illumination, &meshes.pending).layers.is_empty());
    }

    #[test]
    fn lines_are_as_wide_as_their_stroke() {
        let camera = camera(Projection::Orthographic { zoom: 1.0 });
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let lines = LineListData {
            points: vec![[0.0; 3], [1.0, 0.0, 0.0]],
            indices: None,
            strip: true,
            color: peniko::Color::WHITE,
            colors: None,
            revealed: None,
        };
        let (transform, opacity) = (GlobalSpatialTransform::default(), GlobalOpacity(1.0));
        let mut sources = MeshSources::default();
        let mut meshes =
            GpuMeshes::new(&mut sources, (1920, 1080), camera.to_vello_transform(), 0).unwrap();
        for (key, width) in [(1, Some(0.05)), (2, None)] {
            meshes.push(
                0,
                [(key, 1), (key, 1)],
                [None, None],
                &Content {
                    transform: &transform,
                    opacity: &opacity,
                    mesh: None,
                    lines: Some(&lines),
                    material: None,
                    line_width: width,
                    line_range: None,
                    fill: None,
                    joints: None,
                    glow: None,
                    shadow: None,
                    blur: None,
                    instances: None,
                    morph: None,
                },
            );
        }
        let frames = meshes.finish(&camera, ndc_to_canvas, line_width, &Illumination::default());
        let half_width = |draw: usize| {
            let start = draw * DRAW_STRIDE + LINE_ROW;
            f32::from_le_bytes(
                frames[0].1.draw_uniforms[start..start + 4]
                    .try_into()
                    .unwrap(),
            )
        };
        // 0.05 scene units at 120 pixels per unit, and the default 2 pixels
        // of a 1080-pixel-tall output.
        assert!((half_width(0) - 3.0).abs() < 1e-4, "{}", half_width(0));
        assert!((half_width(1) - 1.0).abs() < 1e-4, "{}", half_width(1));
    }

    #[test]
    fn a_revealed_line_draws_only_the_segments_it_shows() {
        let camera = camera(Projection::Orthographic { zoom: 1.0 });
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let lines = LineListData {
            points: vec![
                [0.0; 3],
                [1.0, 0.0, 0.0],
                [2.0, 0.0, 0.0],
                [3.0, 0.0, 0.0],
                [4.0; 3],
            ],
            indices: None,
            strip: true,
            color: peniko::Color::WHITE,
            colors: None,
            revealed: None,
        };
        let (transform, opacity) = (GlobalSpatialTransform::default(), GlobalOpacity(1.0));
        let mut sources = MeshSources::default();
        let mut meshes =
            GpuMeshes::new(&mut sources, (1920, 1080), camera.to_vello_transform(), 0).unwrap();
        for (key, range) in [(1, Some([0.25, 0.6])), (2, Some([0.5, 0.5])), (3, None)] {
            meshes.push(
                0,
                [(key, 1), (key, 1)],
                [None, None],
                &Content {
                    transform: &transform,
                    opacity: &opacity,
                    mesh: None,
                    lines: Some(&lines),
                    material: None,
                    line_width: None,
                    line_range: range,
                    fill: None,
                    joints: None,
                    glow: None,
                    shadow: None,
                    blur: None,
                    instances: None,
                    morph: None,
                },
            );
        }
        let frames = meshes.finish(&camera, ndc_to_canvas, line_width, &Illumination::default());
        let frame = &frames[0].1;
        let shown = |draw: usize| {
            let start = draw * DRAW_STRIDE + LINE_ROW + 4;
            let value = |offset: usize| {
                f32::from_le_bytes(
                    frame.draw_uniforms[start + offset..start + offset + 4]
                        .try_into()
                        .unwrap(),
                )
            };
            [value(0), value(4)]
        };
        // A quarter to 60% of 4 segments: from 1 to 2.4, which draws the
        // second and third segments, the third in part.
        assert_eq!(shown(0), [1.0, 2.4]);
        assert_eq!(frame.draws[0].instances, 1..3);
        // Nothing shown draws nothing; without a reveal, every segment.
        assert!(frame.draws[1].instances.is_empty());
        assert_eq!(
            (shown(2), frame.draws[2].instances.clone()),
            ([0.0, 4.0], 0..4)
        );
    }

    #[test]
    fn a_line_revealed_halfway_draws_its_first_half() {
        let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
            return;
        };
        let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        let (ndc_to_canvas, _) = canvas_projection(&camera).unwrap();
        let placement = camera.to_vello_transform();
        let (transform, opacity) = (GlobalSpatialTransform::default(), GlobalOpacity(1.0));
        let lines = LineListData {
            points: vec![[-1.5, 0.0, 0.0], [0.0, 0.0, 0.0], [1.5, 0.0, 0.0]],
            indices: None,
            strip: true,
            color: peniko::Color::from_rgb8(255, 0, 0),
            colors: None,
            revealed: None,
        };
        let mut sources = MeshSources::default();
        let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
        meshes.push(
            0,
            [(1, 1), (1, 1)],
            [None, None],
            &Content {
                transform: &transform,
                opacity: &opacity,
                mesh: None,
                lines: Some(&lines),
                material: None,
                line_width: Some(0.25),
                line_range: Some([0.0, 0.5]),
                fill: None,
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
            },
        );
        let frames: Vec<GpuMeshFrame> = meshes
            .finish(&camera, ndc_to_canvas, 0.1, &Illumination::default())
            .into_iter()
            .map(|(_, frame)| frame)
            .collect();
        let mut scene = vello::Scene::new();
        scene.append(&frames[0].scene(), Some(placement));
        let mut three_d = GpuThreeD::default();
        three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
        let target = gpu.target(64, 64);
        gpu.render(&scene, &target);
        let pixels = gpu.read(&target);
        let red = |x: usize| pixels[(32 * 64 + x) * 4];
        // At 16 pixels per unit, x = -1 is shown and x = 1 is not.
        assert!(red(16) > 200, "{}", red(16));
        assert_eq!(red(48), 0);
    }

    #[test]
    fn an_opaque_mesh_shadows_the_lit_floor_beneath_it() {
        let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
            return;
        };
        let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let placement = camera.to_vello_transform();
        let opacity = GlobalOpacity(1.0);
        let lit = lit_quad();
        // A floor 2 units on a side, and a card a unit above it, right of the
        // middle: light from the right throws the card's shadow left of it.
        let floor = placed(1.0, DVec3::ZERO);
        let card = placed(0.25, DVec3::new(0.5, 0.0, 1.0));
        let mut sources = MeshSources::default();
        let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
        for (key, transform) in [(1, &floor), (2, &card)] {
            meshes.push(
                0,
                [(key, 1), (key, 1)],
                [None, None],
                &Content {
                    transform,
                    opacity: &opacity,
                    mesh: Some(&lit),
                    lines: None,
                    material: lit.material.as_ref(),
                    line_width: None,
                    line_range: None,
                    fill: None,
                    joints: None,
                    glow: None,
                    shadow: None,
                    blur: None,
                    instances: None,
                    morph: None,
                },
            );
        }
        let illumination = Illumination {
            lighting: Lighting3D::default(),
            lights: vec![Light {
                position: DVec3::new(1.0, 0.0, 1.0),
                directional: true,
                color: [1.0; 3],
                spot: None,
            }],
            ..Default::default()
        };
        let frames: Vec<GpuMeshFrame> = meshes
            .finish(&camera, ndc_to_canvas, line_width, &illumination)
            .into_iter()
            .map(|(_, frame)| frame)
            .collect();
        let mut scene = vello::Scene::new();
        scene.append(&frames[0].scene(), Some(placement));
        let mut three_d = GpuThreeD::default();
        assert!(three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames));
        let target = gpu.target(64, 64);
        gpu.render(&scene, &target);
        let pixels = gpu.read(&target);
        let pixel = |x: usize, y: usize| pixels[(y * 64 + x) * 4];
        // At 16 pixels per unit: the shadow at (-0.5, 0), the floor in the
        // light at (-0.5, 0.75).
        let (shadow, light) = (pixel(24, 32), pixel(24, 20));
        assert!(
            u16::from(shadow) + 40 < u16::from(light),
            "shadow {shadow}, light {light}"
        );
    }

    #[test]
    fn filled_outlines_shadow_the_lit_floor_through_their_holes() {
        let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
            return;
        };
        let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let placement = camera.to_vello_transform();
        let opacity = GlobalOpacity(1.0);
        let lit = lit_quad();
        let floor = placed(1.0, DVec3::ZERO);
        // A square frame, half a unit on a side with a hole a fifth of a unit
        // across, a unit above the floor and right of the middle: light from
        // the right throws its shadow a unit left of it.
        let mut frame = kurbo::BezPath::new();
        frame.move_to((0.25, -0.25));
        frame.line_to((0.75, -0.25));
        frame.line_to((0.75, 0.25));
        frame.line_to((0.25, 0.25));
        frame.close_path();
        frame.move_to((0.4, -0.1));
        frame.line_to((0.4, 0.1));
        frame.line_to((0.6, 0.1));
        frame.line_to((0.6, -0.1));
        frame.close_path();
        let fill = PlaneFillData {
            glyphs: vec![gaanim_scene::PlaneGlyph {
                outline: Arc::new(frame),
                placement: kurbo::Affine::IDENTITY,
                color: peniko::Color::from_rgb8(255, 0, 0),
                gradient: None,
                clip: None,
                image: None,
            }],
        };
        let above = placed(1.0, DVec3::new(0.0, 0.0, 1.0));
        let mut sources = MeshSources::default();
        let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
        meshes.push(
            0,
            [(1, 1), (1, 1)],
            [None, None],
            &Content {
                transform: &floor,
                opacity: &opacity,
                mesh: Some(&lit),
                lines: None,
                material: lit.material.as_ref(),
                line_width: None,
                line_range: None,
                fill: None,
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
            },
        );
        assert!(meshes.push_fill(
            0,
            (2, 1),
            None,
            &Content {
                transform: &above,
                opacity: &opacity,
                mesh: None,
                lines: None,
                material: None,
                line_width: None,
                line_range: None,
                fill: Some(&fill),
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
            },
        ));
        let illumination = Illumination {
            lighting: Lighting3D::default(),
            lights: vec![Light {
                position: DVec3::new(1.0, 0.0, 1.0),
                directional: true,
                color: [1.0; 3],
                spot: None,
            }],
            ..Default::default()
        };
        let frames: Vec<GpuMeshFrame> = meshes
            .finish(&camera, ndc_to_canvas, line_width, &illumination)
            .into_iter()
            .map(|(_, frame)| frame)
            .collect();
        let mut scene = vello::Scene::new();
        scene.append(&frames[0].scene(), Some(placement));
        let mut three_d = GpuThreeD::default();
        assert!(three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames));
        let target = gpu.target(64, 64);
        gpu.render(&scene, &target);
        let pixels = gpu.read(&target);
        let pixel = |x: usize, y: usize| u16::from(pixels[(y * 64 + x) * 4]);
        // At 16 pixels per unit: the frame's shadow at (-0.67, 0), its hole's
        // at (-0.5, 0), and the floor in the light at (-0.67, 0.75).
        let (shadow, hole, light) = (pixel(21, 32), pixel(24, 32), pixel(21, 20));
        assert!(shadow + 40 < light, "shadow {shadow}, light {light}");
        assert!(hole + 10 > light, "hole {hole}, light {light}");
    }

    #[test]
    fn translucent_layers_behind_filled_outlines_do_not_cover_them() {
        let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
            return;
        };
        let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let placement = camera.to_vello_transform();
        // A half-opaque blue sheet, and a red square a unit in front of it.
        let sheet = quad(peniko::Color::from_rgb8(0, 0, 255));
        let behind = placed(1.0, DVec3::ZERO);
        let half = GlobalOpacity(0.5);
        let fill = PlaneFillData {
            glyphs: vec![gaanim_scene::PlaneGlyph {
                outline: Arc::new(kurbo::Shape::to_path(
                    &kurbo::Rect::new(-0.5, -0.5, 0.5, 0.5),
                    0.0,
                )),
                placement: kurbo::Affine::IDENTITY,
                color: peniko::Color::from_rgb8(255, 0, 0),
                gradient: None,
                clip: None,
                image: None,
            }],
        };
        let front = placed(1.0, DVec3::new(0.0, 0.0, 1.0));
        let opaque = GlobalOpacity(1.0);
        let mut sources = MeshSources::default();
        let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
        meshes.push(
            0,
            [(1, 1), (1, 1)],
            [None, None],
            &Content {
                transform: &behind,
                opacity: &half,
                mesh: Some(&sheet),
                lines: None,
                material: None,
                line_width: None,
                line_range: None,
                fill: None,
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
            },
        );
        assert!(meshes.push_fill(
            0,
            (2, 1),
            None,
            &Content {
                transform: &front,
                opacity: &opaque,
                mesh: None,
                lines: None,
                material: None,
                line_width: None,
                line_range: None,
                fill: Some(&fill),
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
            },
        ));
        let illumination = Illumination {
            lighting: Lighting3D::default(),
            lights: Vec::new(),
            ..Default::default()
        };
        let frames: Vec<GpuMeshFrame> = meshes
            .finish(&camera, ndc_to_canvas, line_width, &illumination)
            .into_iter()
            .map(|(_, frame)| frame)
            .collect();
        let mut scene = vello::Scene::new();
        scene.append(&frames[0].scene(), Some(placement));
        let mut three_d = GpuThreeD::default();
        assert!(three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames));
        let target = gpu.target(64, 64);
        gpu.render(&scene, &target);
        let pixels = gpu.read(&target);
        let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 3];
        // The square, drawn after the sheet's layer, is not blended with it.
        let square = pixel(32, 32);
        assert!(square[0] > 240 && square[2] < 10, "square {square:?}");
        // Beside it, the sheet shows.
        let sheet = pixel(32, 18);
        assert!(sheet[2] > 60 && sheet[0] < 10, "sheet {sheet:?}");
    }

    /// The red channel at `pixels` of a 64 x 64 render of a lit floor 2
    /// units on a side and a card a unit above it, right of the middle,
    /// under `light`, seen from above at 16 pixels per unit.
    fn floor_and_card(lights: Vec<Light>) -> Option<Vec<u8>> {
        let mut gpu = crate::background::test_gpu::test_gpu()?;
        let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let placement = camera.to_vello_transform();
        let opacity = GlobalOpacity(1.0);
        let lit = lit_quad();
        let floor = placed(1.0, DVec3::ZERO);
        let card = placed(0.25, DVec3::new(0.5, 0.0, 1.0));
        let mut sources = MeshSources::default();
        let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
        for (key, transform) in [(1, &floor), (2, &card)] {
            meshes.push(
                0,
                [(key, 1), (key, 1)],
                [None, None],
                &Content {
                    transform,
                    opacity: &opacity,
                    mesh: Some(&lit),
                    lines: None,
                    material: lit.material.as_ref(),
                    line_width: None,
                    line_range: None,
                    fill: None,
                    joints: None,
                    glow: None,
                    shadow: None,
                    blur: None,
                    instances: None,
                    morph: None,
                },
            );
        }
        let illumination = Illumination {
            lighting: Lighting3D::default(),
            lights,
            ..Default::default()
        };
        let frames: Vec<GpuMeshFrame> = meshes
            .finish(&camera, ndc_to_canvas, line_width, &illumination)
            .into_iter()
            .map(|(_, frame)| frame)
            .collect();
        let mut scene = vello::Scene::new();
        scene.append(&frames[0].scene(), Some(placement));
        let mut three_d = GpuThreeD::default();
        three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
        let target = gpu.target(64, 64);
        gpu.render(&scene, &target);
        Some(gpu.read(&target).chunks(4).map(|pixel| pixel[0]).collect())
    }

    #[test]
    fn a_point_light_shadows_through_the_face_of_its_cube_the_floor_lies_in() {
        // From (1.5, 0, 3) through the card, the shadow falls around the
        // middle of the floor, which the light sees through its -z face.
        let Some(red) = floor_and_card(vec![Light {
            position: DVec3::new(1.5, 0.0, 3.0),
            directional: false,
            color: [1.0; 3],
            spot: None,
        }]) else {
            return;
        };
        // At 16 pixels per unit: the shadow at (-0.2, 0), the floor in the
        // light at (-0.2, 0.75).
        let (shadow, light) = (red[32 * 64 + 29], red[20 * 64 + 29]);
        assert!(
            u16::from(shadow) + 40 < u16::from(light),
            "shadow {shadow}, light {light}"
        );
    }

    /// A mesh drawn at many points takes one record per copy shown, in one
    /// draw, each placed, sized and colored by its copy; hidden copies take
    /// none.
    #[test]
    fn copies_of_a_mesh_draw_as_one_with_a_record_each() {
        let camera = camera(Projection::Orthographic { zoom: 1.0 });
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let mesh = lit_quad();
        let opacity = GlobalOpacity(1.0);
        let transform = placed(2.0, DVec3::new(1.0, 0.0, 0.0));
        let frame = |reveal: f32| {
            let instances = MeshInstances {
                copies: Arc::new(MeshCopies {
                    positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
                    scales: Some(vec![[1.0; 3], [0.5; 3], [1.0; 3]]),
                    rotations: None,
                    colors: Some(vec![[1.0, 0.0, 0.0, 1.0]; 3]),
                }),
                reveal,
            };
            let mut sources = MeshSources::default();
            let mut meshes =
                GpuMeshes::new(&mut sources, (1920, 1080), camera.to_vello_transform(), 0).unwrap();
            meshes.push(
                0,
                [(1, 1), (1, 1)],
                [None, None],
                &Content {
                    transform: &transform,
                    opacity: &opacity,
                    mesh: Some(&mesh),
                    lines: None,
                    material: mesh.material.as_ref(),
                    line_width: None,
                    line_range: None,
                    fill: None,
                    joints: None,
                    glow: None,
                    shadow: None,
                    blur: None,
                    instances: Some(&instances),
                    morph: None,
                },
            );
            meshes
                .finish(&camera, ndc_to_canvas, line_width, &Illumination::default())
                .remove(0)
                .1
        };
        let shown = frame(1.0);
        assert_eq!(shown.draws.len(), 1);
        assert_eq!(shown.draws[0].copies, 0..3);
        assert_eq!(shown.instances.len(), 3 * INSTANCE_BYTES);
        let value = |record: usize, at: usize| {
            let start = record * INSTANCE_BYTES + at * 4;
            f32::from_le_bytes(shown.instances[start..start + 4].try_into().unwrap())
        };
        // The third copy sits 2 units up in the drawable, 4 in the world;
        // the second is half the size; each takes its own color.
        assert_eq!([value(2, 12), value(2, 13)], [1.0, 4.0]);
        assert_eq!(value(1, 0), 1.0);
        assert_eq!(value(0, 28), 1.0);
        assert_eq!(value(0, 29), 0.0);
        // Before they grow in, none draws.
        let hidden = frame(0.0);
        assert_eq!(hidden.draws[0].copies, 0..0);
        assert!(hidden.instances.is_empty());
    }

    #[test]
    fn meshes_alike_share_one_upload_and_draw_as_one() {
        let camera = camera(Projection::Orthographic { zoom: 1.0 });
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let lit = lit_quad();
        let mut other = lit_quad();
        other.vertices[2] = [2.0, 2.0, 0.0];
        let opacity = GlobalOpacity(1.0);
        let places: Vec<GlobalSpatialTransform> = (0..4)
            .map(|index| placed(0.5, DVec3::new(index as f64, 0.0, 0.0)))
            .collect();
        let mut sources = MeshSources::default();
        let mut meshes =
            GpuMeshes::new(&mut sources, (1920, 1080), camera.to_vello_transform(), 0).unwrap();
        for (index, transform) in places.iter().enumerate() {
            let mesh = if index == 2 { &other } else { &lit };
            meshes.push(
                0,
                [(index as u64, 1), (index as u64, 1)],
                [None, None],
                &Content {
                    transform,
                    opacity: &opacity,
                    mesh: Some(mesh),
                    lines: None,
                    material: mesh.material.as_ref(),
                    line_width: None,
                    line_range: None,
                    fill: None,
                    joints: None,
                    glow: None,
                    shadow: None,
                    blur: None,
                    instances: None,
                    morph: None,
                },
            );
        }
        let frames = meshes.finish(&camera, ndc_to_canvas, line_width, &Illumination::default());
        let frame = &frames[0].1;
        assert_eq!(frame.instances.len(), 4 * INSTANCE_BYTES);
        // The three quads alike share one source and sit next to each
        // other, so they draw as one; the other quad draws apart.
        let alike = (0..4)
            .map(|reference| -> Vec<usize> {
                (0..4)
                    .filter(|index| {
                        frame.draws[*index].source.vertices
                            == frame.draws[reference].source.vertices
                    })
                    .collect()
            })
            .max_by_key(Vec::len)
            .unwrap();
        assert_eq!(alike.len(), 3);
        assert!(alike.windows(2).all(|pair| pair[1] == pair[0] + 1));
        let [first, .., last] = alike[..] else {
            unreachable!()
        };
        assert!(Arc::ptr_eq(
            &frame.draws[first].source,
            &frame.draws[last].source
        ));
        assert!(draw_alike(frame, first, first + 1) && draw_alike(frame, first, last));
        let apart = (0..4).find(|index| !alike.contains(index)).unwrap();
        assert!(!draw_alike(frame, first, apart));
    }

    #[test]
    fn outlines_push_a_cube_out_along_the_mean_normal_of_its_corners() {
        // A tetrahedron whose faces have vertices of their own: each copy of
        // a corner pushes out the same way, between its three faces.
        let corners = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let faces = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let tetrahedron = TriangleMeshData {
            vertices: faces
                .iter()
                .flatten()
                .map(|&corner| corners[corner])
                .collect(),
            indices: (0..12).collect(),
            ..quad(peniko::Color::WHITE)
        };
        let source = MeshSource::mesh(&tetrahedron, true, true).unwrap();
        assert_eq!(source.outline.len(), 12 * 12);
        let direction = |vertex: usize| -> [f32; 3] {
            std::array::from_fn(|axis| {
                let at = vertex * 12 + axis * 4;
                f32::from_le_bytes(source.outline[at..at + 4].try_into().unwrap())
            })
        };
        // The origin corner's copies (vertices 0, 3 and 6) all point away
        // from the solid, equally along the three axes.
        for vertex in [0, 3, 6] {
            let [x, y, z] = direction(vertex);
            assert!(
                x < 0.0 && (x - y).abs() < 1e-5 && (y - z).abs() < 1e-5,
                "{x} {y} {z}"
            );
        }
        // Unlit meshes have no material to outline them.
        assert!(
            MeshSource::mesh(&tetrahedron, false, true)
                .unwrap()
                .outline
                .is_empty()
        );
        // Nor do meshes whose material draws no outline.
        assert!(
            MeshSource::mesh(&tetrahedron, true, false)
                .unwrap()
                .outline
                .is_empty()
        );
    }

    #[test]
    fn an_outlined_mesh_draws_its_outline_around_its_silhouette() {
        let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
            return;
        };
        let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let placement = camera.to_vello_transform();
        let opacity = GlobalOpacity(1.0);
        let mut card = lit_quad();
        card.material = Some(
            Material3D {
                color: peniko::Color::BLACK,
                emissive: peniko::Color::from_rgb8(0, 0, 255),
                emissive_strength: 1.0,
                ..Material3D::default()
            }
            .with_outline(peniko::Color::from_rgb8(255, 0, 0), 0.25)
            .unwrap(),
        );
        // A card 1 unit on a side in the middle: its outline reaches 0.25
        // units beyond its edges, 4 pixels at 16 pixels per unit.
        let transform = placed(0.5, DVec3::ZERO);
        let mut sources = MeshSources::default();
        let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
        meshes.push(
            0,
            [(1, 1), (1, 1)],
            [None, None],
            &Content {
                transform: &transform,
                opacity: &opacity,
                mesh: Some(&card),
                lines: None,
                material: card.material.as_ref(),
                line_width: None,
                line_range: None,
                fill: None,
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
            },
        );
        let frames: Vec<GpuMeshFrame> = meshes
            .finish(&camera, ndc_to_canvas, line_width, &Illumination::default())
            .into_iter()
            .map(|(_, frame)| frame)
            .collect();
        assert!(frames[0].draws[0].outlined);
        let mut scene = vello::Scene::new();
        scene.append(&frames[0].scene(), Some(placement));
        let mut three_d = GpuThreeD::default();
        three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
        let target = gpu.target(64, 64);
        gpu.render(&scene, &target);
        let pixels = gpu.read(&target);
        let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        // The card's middle is the card; 2 pixels beyond its right edge
        // (x = 40 + 2), the outline; well beyond it, nothing.
        assert!(
            pixel(32, 32)[2] > 200 && pixel(32, 32)[0] < 100,
            "{:?}",
            pixel(32, 32)
        );
        assert!(
            pixel(42, 32)[0] > 200 && pixel(42, 32)[2] < 100,
            "{:?}",
            pixel(42, 32)
        );
        assert_eq!(pixel(50, 32)[3], 0);
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
                line_width: None,
                line_range: None,
                fill: None,
                joints: None,
                transform: &transform,
                opacity: &opacity,
                mesh: Some(&red),
                lines: None,
                material: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
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

    /// `quad` as an imported model's primitive: its UVs from its corners
    /// (v down), `texture`, `alpha`, and with `skinned` every vertex on
    /// joint 0.
    fn model_quad(
        texture: Option<MeshTexture>,
        alpha: AlphaMode,
        skinned: bool,
    ) -> TriangleMeshData {
        let mut mesh = quad(peniko::Color::WHITE);
        mesh.uvs = Some(
            mesh.vertices
                .iter()
                .map(|v| [(v[0] + 1.0) * 0.5, (1.0 - v[1]) * 0.5])
                .collect(),
        );
        let count = mesh.vertices.len();
        mesh.surface = Some(Arc::new(gaanim_scene::MeshSurface {
            texture,
            alpha,
            double_sided: false,
            skin: skinned.then(|| gaanim_scene::SkinWeights {
                joints: vec![[0; 4]; count],
                weights: vec![[1.0, 0.0, 0.0, 0.0]; count],
            }),
            ..Default::default()
        }));
        mesh
    }

    /// A 2x1 texture: its left texel `left`, its right one `right`, read
    /// with nearest texels and clamped.
    fn two_texels(left: [u8; 4], right: [u8; 4]) -> MeshTexture {
        MeshTexture {
            image: ImageData {
                data: Blob::from([left, right].concat()),
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::Alpha,
                width: 2,
                height: 1,
            },
            wrap: [TextureWrap::ClampToEdge; 2],
            nearest: true,
        }
    }

    /// The RGBA pixels of `meshes` drawn into a 64-pixel frame 4 units wide,
    /// each with its palette, and whether the model pipelines were built.
    fn draw_meshes(
        meshes: &[(TriangleMeshData, Option<Arc<[[f32; 16]]>>)],
    ) -> Option<(Vec<u8>, bool)> {
        let meshes: Vec<_> = meshes
            .iter()
            .map(|(mesh, joints)| (mesh.clone(), joints.clone(), None))
            .collect();
        draw_morphed(&meshes)
    }

    /// The RGBA pixels of `meshes` drawn as [`draw_meshes`] draws them, each
    /// with its palette and its morph targets' weights.
    #[allow(clippy::type_complexity)]
    fn draw_morphed(
        meshes: &[(
            TriangleMeshData,
            Option<Arc<[[f32; 16]]>>,
            Option<Arc<[f32]>>,
        )],
    ) -> Option<(Vec<u8>, bool)> {
        let mut gpu = crate::background::test_gpu::test_gpu()?;
        let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let placement = camera.to_vello_transform();
        let transform = GlobalSpatialTransform::default();
        let opacity = GlobalOpacity(1.0);
        let mut sources = MeshSources::default();
        let mut pending = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
        for (index, (mesh, joints, morph)) in meshes.iter().enumerate() {
            let key = index as u64 + 1;
            pending.push(
                0,
                [(key, 1), (key, 1)],
                [None, None],
                &Content {
                    transform: &transform,
                    opacity: &opacity,
                    mesh: Some(mesh),
                    lines: None,
                    material: mesh.material.as_ref(),
                    line_width: None,
                    line_range: None,
                    fill: None,
                    joints: joints.as_ref(),
                    glow: None,
                    shadow: None,
                    blur: None,
                    instances: None,
                    morph: morph.as_ref(),
                },
            );
        }
        let frames: Vec<GpuMeshFrame> = pending
            .finish(&camera, ndc_to_canvas, line_width, &Illumination::default())
            .into_iter()
            .map(|(_, frame)| frame)
            .collect();
        let mut scene = vello::Scene::new();
        scene.append(&frames[0].scene(), Some(placement));
        let mut three_d = GpuThreeD::default();
        three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
        let target = gpu.target(64, 64);
        gpu.render(&scene, &target);
        Some((gpu.read(&target), three_d.models.is_some()))
    }

    fn pixel(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
        pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4]
            .try_into()
            .unwrap()
    }

    /// The RGBA pixels of `meshes`, each placed by its matrix and with its
    /// glow, drawn as [`draw_meshes`] draws them, and whether the glow
    /// pipelines were built.
    fn draw_glowing(meshes: &[(TriangleMeshData, DMat4, Option<Glow>)]) -> Option<(Vec<u8>, bool)> {
        let meshes: Vec<_> = meshes
            .iter()
            .map(|(mesh, matrix, glow)| (mesh.clone(), *matrix, glow.clone(), None, None))
            .collect();
        draw_soft(&meshes)
    }

    /// A mesh, where it stands, and its glow, shadow and blur.
    type Soft = (
        TriangleMeshData,
        DMat4,
        Option<Glow>,
        Option<DropShadow>,
        Option<GaussianBlur>,
    );

    /// Draw `meshes` with their glows, shadows and blurs: the pixels, and
    /// whether the glow pipelines were built.
    fn draw_soft(meshes: &[Soft]) -> Option<(Vec<u8>, bool)> {
        let mut gpu = crate::background::test_gpu::test_gpu()?;
        let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let placement = camera.to_vello_transform();
        let opacity = GlobalOpacity(1.0);
        let transforms: Vec<GlobalSpatialTransform> = meshes
            .iter()
            .map(|(_, matrix, _, _, _)| GlobalSpatialTransform {
                mat4: *matrix,
                ..Default::default()
            })
            .collect();
        let mut sources = MeshSources::default();
        let mut pending = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
        for (index, ((mesh, _, glow, shadow, blur), transform)) in
            meshes.iter().zip(&transforms).enumerate()
        {
            let key = index as u64 + 1;
            pending.push(
                0,
                [(key, 1), (key, 1)],
                [None, None],
                &Content {
                    transform,
                    opacity: &opacity,
                    mesh: Some(mesh),
                    lines: None,
                    material: mesh.material.as_ref(),
                    line_width: None,
                    line_range: None,
                    fill: None,
                    joints: None,
                    glow: glow.clone(),
                    shadow: shadow.clone(),
                    blur: *blur,
                    instances: None,
                    morph: None,
                },
            );
        }
        let frames: Vec<GpuMeshFrame> = pending
            .finish(&camera, ndc_to_canvas, line_width, &Illumination::default())
            .into_iter()
            .map(|(_, frame)| frame)
            .collect();
        let mut scene = vello::Scene::new();
        scene.append(&frames[0].scene(), Some(placement));
        let mut three_d = GpuThreeD::default();
        three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
        let target = gpu.target(64, 64);
        gpu.render(&scene, &target);
        Some((gpu.read(&target), three_d.glow.is_some()))
    }

    /// The RGBA pixels of `meshes`, each placed by its matrix and at its
    /// opacity, drawn as [`draw_meshes`] draws them with translucent
    /// content blending as `transparency` says, and whether the peel
    /// pipelines were built.
    fn draw_translucent(
        meshes: &[(TriangleMeshData, DMat4, f32)],
        transparency: Transparency3D,
    ) -> Option<(Vec<u8>, bool)> {
        let illumination = Illumination {
            transparency,
            ..Default::default()
        };
        let (pixels, three_d) = draw_illuminated(meshes, &illumination)?;
        Some((pixels, three_d.peel.is_some()))
    }

    /// The RGBA pixels of `meshes`, each placed by its matrix and at its
    /// opacity, drawn as [`draw_meshes`] draws them under `illumination`,
    /// and what drew them.
    fn draw_illuminated(
        meshes: &[(TriangleMeshData, DMat4, f32)],
        illumination: &Illumination,
    ) -> Option<(Vec<u8>, GpuThreeD)> {
        let mut gpu = crate::background::test_gpu::test_gpu()?;
        let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let placement = camera.to_vello_transform();
        let placed: Vec<(GlobalSpatialTransform, GlobalOpacity)> = meshes
            .iter()
            .map(|(_, matrix, opacity)| {
                let transform = GlobalSpatialTransform {
                    mat4: *matrix,
                    ..Default::default()
                };
                (transform, GlobalOpacity(*opacity))
            })
            .collect();
        let mut sources = MeshSources::default();
        let mut pending = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
        for (index, ((mesh, _, _), (transform, opacity))) in meshes.iter().zip(&placed).enumerate()
        {
            let key = index as u64 + 1;
            pending.push(
                0,
                [(key, 1), (key, 1)],
                [None, None],
                &Content {
                    transform,
                    opacity,
                    mesh: Some(mesh),
                    lines: None,
                    material: mesh.material.as_ref(),
                    line_width: None,
                    line_range: None,
                    fill: None,
                    joints: None,
                    glow: None,
                    shadow: None,
                    blur: None,
                    instances: None,
                    morph: None,
                },
            );
        }
        let frames: Vec<GpuMeshFrame> = pending
            .finish(&camera, ndc_to_canvas, line_width, illumination)
            .into_iter()
            .map(|(_, frame)| frame)
            .collect();
        let mut scene = vello::Scene::new();
        scene.append(&frames[0].scene(), Some(placement));
        let mut three_d = GpuThreeD::default();
        three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
        let target = gpu.target(64, 64);
        gpu.render(&scene, &target);
        Some((gpu.read(&target), three_d))
    }

    /// A white quad in focus on the left and one farther away on the right:
    /// out of focus, the far one's edge spreads over several pixels while
    /// the near one's stays sharp, and in focus both are sharp.
    #[test]
    fn the_depth_of_field_blurs_what_lies_away_from_the_focus() {
        let white = quad(peniko::Color::WHITE);
        // 16 pixels per unit; the camera stands 10 units in front of its
        // target, so the far quad lies at 1.5 times its distance.
        let near = DMat4::from_translation(DVec3::new(-1.0, 0.0, 0.0))
            * DMat4::from_scale(DVec3::splat(0.5));
        let far = DMat4::from_translation(DVec3::new(1.0, 0.0, -5.0))
            * DMat4::from_scale(DVec3::splat(0.5));
        let meshes = [(white.clone(), near, 1.0), (white, far, 1.0)];
        let blurred = Illumination {
            depth_of_field: gaanim_scene::DepthOfField {
                focus: 1.0,
                aperture: 1.0,
                max_blur: 1.0,
            },
            ..Default::default()
        };
        let Some((sharp, three_d)) = draw_illuminated(&meshes, &Illumination::default()) else {
            return;
        };
        assert!(three_d.focus.is_none(), "a frame in focus builds no blur");
        let (out, three_d) = draw_illuminated(&meshes, &blurred).unwrap();
        assert!(three_d.focus.is_some());
        // The near quad spans pixels 8 to 24 across, the far one 40 to 56.
        let alpha = |pixels: &[u8], x| pixel(pixels, x, 32)[3];
        assert_eq!((alpha(&sharp, 23), alpha(&sharp, 25)), (255, 0));
        assert_eq!((alpha(&sharp, 40), alpha(&sharp, 38)), (255, 0));
        assert_eq!(alpha(&out, 23), 255, "in focus");
        assert!(alpha(&out, 25) <= 2, "in focus: {}", alpha(&out, 25));
        // A third of a unit of blur: 5 pixels of sigma.
        let (inside, edge, outside) = (alpha(&out, 48), alpha(&out, 40), alpha(&out, 36));
        assert!(inside > 150, "{inside}");
        assert!((40..200).contains(&edge), "{edge}");
        assert!((5..130).contains(&outside), "{outside}");
    }

    /// A spot light lights the floor within its cone and leaves the rest
    /// to the sky and ground light, where a point light at its place lights
    /// all of it.
    #[test]
    fn a_spot_light_lights_only_within_its_cone() {
        let position = DVec3::new(-0.6, 0.6, 3.0);
        let target = DVec3::new(-0.6, 0.6, 0.0);
        let cone = gaanim_scene::SpotCone {
            target,
            angle: 0.25,
            softness: 0.2,
        };
        let spot = Light {
            position,
            directional: false,
            color: [1.0; 3],
            spot: Spot::new(&cone, position),
        };
        assert!(spot.spot.is_some());
        let point = Light { spot: None, ..spot };
        let Some(spotted) = floor_and_card(vec![spot]) else {
            return;
        };
        let lit = floor_and_card(vec![point]).unwrap();
        // At 16 pixels per unit: the cone's middle at (-0.6, 0.6), and the
        // floor well outside it at (0.75, -0.75).
        let (within, beyond) = (spotted[22 * 64 + 22], spotted[44 * 64 + 44]);
        assert!(
            u16::from(beyond) + 40 < u16::from(within),
            "within {within}, beyond {beyond}"
        );
        assert!(
            lit[44 * 64 + 44] > beyond + 40,
            "{} {beyond}",
            lit[44 * 64 + 44]
        );
        assert!(within.abs_diff(lit[22 * 64 + 22]) < 4);
        // A spot that aims at itself gives no light.
        let aimless = gaanim_scene::SpotCone {
            target: position,
            ..cone
        };
        assert!(Spot::new(&aimless, position).is_none());
    }

    /// Two translucent quads that cross, red nearer on the left and blue on
    /// the right: peeled, each half shows the nearer one over the other;
    /// sorted as layers, one lies over the other on both halves.
    #[test]
    fn peeled_translucent_drawables_blend_in_the_order_they_lie_where_they_cross() {
        let red = quad(peniko::Color::new([1.0, 0.0, 0.0, 1.0]));
        let blue = quad(peniko::Color::new([0.0, 0.0, 1.0, 1.0]));
        let crossing = |angle: f64| DMat4::from_rotation_y(angle);
        let meshes = [(red, crossing(0.5), 0.5), (blue, crossing(-0.5), 0.5)];
        let Some((sorted, built)) = draw_translucent(&meshes, Transparency3D::default()) else {
            return;
        };
        assert!(!built, "a frame that does not peel builds no peel pipeline");
        let (peeled, built) = draw_translucent(&meshes, Transparency3D { layers: 4 }).unwrap();
        assert!(built);
        let (left, right) = (pixel(&peeled, 24, 32), pixel(&peeled, 40, 32));
        assert!(
            left[0] > left[2] + 40,
            "red lies nearer on the left: {left:?}"
        );
        assert!(
            right[2] > right[0] + 40,
            "blue lies nearer on the right: {right:?}"
        );
        assert_eq!(left[3], right[3]);
        let (left, right) = (pixel(&sorted, 24, 32), pixel(&sorted, 40, 32));
        assert_eq!(
            left[0] > left[2],
            right[0] > right[2],
            "sorted, one quad lies over the other on both halves: {left:?} {right:?}"
        );
    }

    /// One translucent drawable folded over itself shows its far fold
    /// through its near one when peeled, and only its nearest surface when
    /// it blends as a layer; one layer peels only the nearest.
    #[test]
    fn peeled_layers_show_the_folds_of_one_translucent_drawable() {
        let mut folded = quad(peniko::Color::WHITE);
        folded.vertices = vec![
            [-1.0, -1.0, 0.5],
            [1.0, -1.0, 0.5],
            [1.0, 1.0, 0.5],
            [-1.0, 1.0, 0.5],
            [-1.0, -1.0, -0.5],
            [1.0, -1.0, -0.5],
            [1.0, 1.0, -0.5],
            [-1.0, 1.0, -0.5],
        ];
        folded.indices = vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7];
        folded.color = None;
        let (red, blue) = ([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]);
        folded.colors = Some(vec![red, red, red, red, blue, blue, blue, blue]);
        let meshes = [(folded, DMat4::IDENTITY, 0.5)];
        let Some((sorted, _)) = draw_translucent(&meshes, Transparency3D::default()) else {
            return;
        };
        let (peeled, _) = draw_translucent(&meshes, Transparency3D { layers: 4 }).unwrap();
        let (nearest, _) = draw_translucent(&meshes, Transparency3D { layers: 1 }).unwrap();
        let (sorted, peeled, nearest) = (
            pixel(&sorted, 32, 32),
            pixel(&peeled, 32, 32),
            pixel(&nearest, 32, 32),
        );
        assert!(
            sorted[2] < 4,
            "a layer shows its nearest surface: {sorted:?}"
        );
        assert!(peeled[2] > 40 && peeled[0] > peeled[2], "{peeled:?}");
        assert!(peeled[3] > sorted[3], "{peeled:?} {sorted:?}");
        // The peeled layer passes through 8 bits once more.
        assert!(
            nearest.iter().zip(sorted).all(|(a, b)| a.abs_diff(b) <= 2),
            "{nearest:?} {sorted:?}"
        );
    }

    fn red_glow() -> Option<Glow> {
        Some(Glow {
            radius: 0.5,
            intensity: 4.0,
            color: peniko::Color::new([1.0, 0.0, 0.0, 1.0]),
        })
    }

    /// A glowing mesh lights the pixels around it in its glow's color,
    /// fading with the distance, and keeps its own.
    #[test]
    fn a_glow_lights_the_pixels_around_a_mesh() {
        let white = quad(peniko::Color::WHITE);
        let Some((plain, built)) = draw_glowing(&[(white.clone(), DMat4::IDENTITY, None)]) else {
            return;
        };
        assert!(!built, "a frame without glows builds no glow pipeline");
        let (glowing, built) = draw_glowing(&[(white, DMat4::IDENTITY, red_glow())]).unwrap();
        assert!(built);
        // The quad spans pixels 16 to 48; its glow blurs by 2 pixels.
        assert_eq!(pixel(&glowing, 32, 32), [255, 255, 255, 255]);
        assert_eq!(pixel(&plain, 50, 32), [0, 0, 0, 0]);
        let near = pixel(&glowing, 50, 32);
        assert!(
            near[0] > 60 && near[3] > 60 && near[1] < 8 && near[2] < 8,
            "{near:?}"
        );
        let far = pixel(&glowing, 53, 32);
        assert!(far[3] < near[3], "{far:?} {near:?}");
        assert!(pixel(&glowing, 62, 32)[3] < 4);
    }

    /// A mesh's drop shadow falls beside it by its offset, in its color,
    /// and hides under the mesh itself.
    #[test]
    fn a_shadow_falls_beside_a_mesh_by_its_offset() {
        let shadow = DropShadow {
            offset: gaanim_core::glam::DVec2::new(0.5, -0.5),
            blur_radius: 0.05,
            color: peniko::Color::new([0.0, 0.0, 0.0, 1.0]),
        };
        let Some((pixels, built)) = draw_soft(&[(
            quad(peniko::Color::WHITE),
            DMat4::IDENTITY,
            None,
            Some(shadow),
            None,
        )]) else {
            return;
        };
        assert!(built);
        // The quad spans pixels 16 to 48, its shadow 24 to 56, 8 pixels
        // right and down.
        assert_eq!(pixel(&pixels, 32, 32), [255, 255, 255, 255]);
        let cast = pixel(&pixels, 52, 52);
        assert!(cast[3] > 230 && cast[0] < 8, "{cast:?}");
        assert_eq!(pixel(&pixels, 52, 20)[3], 0);
        assert_eq!(pixel(&pixels, 12, 12)[3], 0);
    }

    /// A blurred mesh spreads its color over what lies behind it, and what
    /// lies in front of it hides its blur.
    #[test]
    fn a_blurred_mesh_spreads_its_color_over_what_lies_behind() {
        let blue = peniko::Color::new([0.0, 0.0, 1.0, 1.0]);
        let behind = DMat4::from_translation(DVec3::new(0.0, 0.0, -1.0))
            * DMat4::from_scale(DVec3::splat(1.8));
        // A green strip in front of the white quad's lower edge.
        let front = DMat4::from_translation(DVec3::new(0.0, -1.0, 1.0))
            * DMat4::from_scale(DVec3::new(1.9, 0.12, 1.0));
        let blur = GaussianBlur { sigma: 0.25 };
        let Some((pixels, built)) = draw_soft(&[
            (quad(blue), behind, None, None, None),
            (
                quad(peniko::Color::WHITE),
                DMat4::IDENTITY,
                None,
                None,
                Some(blur),
            ),
            (
                quad(peniko::Color::new([0.0, 1.0, 0.0, 1.0])),
                front,
                None,
                None,
                None,
            ),
        ]) else {
            return;
        };
        assert!(built);
        // The white quad spans pixels 16 to 48, blurred by 4 pixels: white
        // in its middle, half white at its edge, blue farther out.
        let middle = pixel(&pixels, 32, 32);
        assert!(middle.iter().all(|value| *value > 245), "{middle:?}");
        let inside = pixel(&pixels, 45, 32);
        assert!(inside[0] < 245 && inside[0] > 128, "{inside:?}");
        let outside = pixel(&pixels, 51, 32);
        assert!(
            outside[0] > 40 && outside[0] < 128 && outside[2] > 200,
            "{outside:?}"
        );
        let far = pixel(&pixels, 60, 32);
        assert!(far[0] < 8 && far[2] > 245, "{far:?}");
        // The strip in front stays green across the quad's edge.
        let strip = pixel(&pixels, 32, 48);
        assert!(strip[1] > 245 && strip[0] < 8, "{strip:?}");
    }

    /// Outlines filled in a drawable's plane glow and cast a drop shadow as
    /// meshes do.
    #[test]
    fn filled_outlines_glow_and_cast_shadows() {
        let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
            return;
        };
        let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let placement = camera.to_vello_transform();
        let opacity = GlobalOpacity(1.0);
        let at_origin = GlobalSpatialTransform::default();
        // A white square 2 units on a side: pixels 16 to 48.
        let square = kurbo::Shape::to_path(&kurbo::Rect::new(0.0, 0.0, 2.0, 2.0), 0.0);
        let fill = PlaneFillData {
            glyphs: vec![gaanim_scene::PlaneGlyph {
                outline: Arc::new(square),
                placement: kurbo::Affine::translate((-1.0, -1.0)),
                color: peniko::Color::WHITE,
                gradient: None,
                clip: None,
                image: None,
            }],
        };
        let mut draw = |glow: Option<Glow>, shadow: Option<DropShadow>| {
            let mut sources = MeshSources::default();
            let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
            assert!(meshes.push_fill(
                0,
                (1, 1),
                None,
                &Content {
                    transform: &at_origin,
                    opacity: &opacity,
                    mesh: None,
                    lines: None,
                    material: None,
                    line_width: None,
                    line_range: None,
                    fill: Some(&fill),
                    joints: None,
                    glow,
                    shadow,
                    blur: None,
                    instances: None,
                    morph: None,
                },
            ));
            let frames: Vec<GpuMeshFrame> = meshes
                .finish(&camera, ndc_to_canvas, line_width, &Illumination::default())
                .into_iter()
                .map(|(_, frame)| frame)
                .collect();
            let mut scene = vello::Scene::new();
            scene.append(&frames[0].scene(), Some(placement));
            let mut three_d = GpuThreeD::default();
            three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
            let target = gpu.target(64, 64);
            gpu.render(&scene, &target);
            gpu.read(&target)
        };
        let glowing = draw(red_glow(), None);
        assert_eq!(pixel(&glowing, 32, 32), [255, 255, 255, 255]);
        let near = pixel(&glowing, 50, 32);
        assert!(near[0] > 60 && near[3] > 60 && near[1] < 8, "{near:?}");
        let shadowed = draw(
            None,
            Some(DropShadow {
                offset: gaanim_core::glam::DVec2::new(0.5, -0.5),
                blur_radius: 0.05,
                color: peniko::Color::new([0.0, 0.0, 0.0, 1.0]),
            }),
        );
        assert_eq!(pixel(&shadowed, 32, 32), [255, 255, 255, 255]);
        let cast = pixel(&shadowed, 52, 52);
        assert!(cast[3] > 230 && cast[0] < 8, "{cast:?}");
        assert_eq!(pixel(&shadowed, 12, 12)[3], 0);
    }

    /// The glow of a mesh hidden behind an opaque one does not show.
    #[test]
    fn a_hidden_mesh_does_not_glow() {
        let behind = DMat4::from_translation(DVec3::new(0.0, 0.0, -1.0))
            * DMat4::from_scale(DVec3::splat(0.5));
        let front = DMat4::from_scale(DVec3::splat(1.5));
        let Some((pixels, _)) = draw_glowing(&[
            (quad(peniko::Color::WHITE), behind, red_glow()),
            (quad(peniko::Color::new([0.0, 0.0, 1.0, 1.0])), front, None),
        ]) else {
            return;
        };
        // Around where the small quad would glow, the front quad, and
        // outside it nothing.
        assert_eq!(pixel(&pixels, 44, 32), [0, 0, 255, 255]);
        assert_eq!(pixel(&pixels, 58, 32), [0, 0, 0, 0]);
    }

    /// A drawable in front of a glowing one covers its glow, as a drawable
    /// drawn over a 2D glow does, and one behind it receives the glow.
    #[test]
    fn what_lies_in_front_of_a_glow_covers_it() {
        let glowing = DMat4::from_scale(DVec3::splat(0.5));
        // Blue covers the right half of the frame in front of the glowing
        // quad, and green its left half behind it.
        let side = |x: f64, z: f64| {
            DMat4::from_translation(DVec3::new(x, 0.0, z))
                * DMat4::from_scale(DVec3::new(1.0, 2.0, 1.0))
        };
        let Some((pixels, _)) = draw_glowing(&[
            (quad(peniko::Color::WHITE), glowing, red_glow()),
            (
                quad(peniko::Color::new([0.0, 0.0, 1.0, 1.0])),
                side(1.05, 1.0),
                None,
            ),
            (
                quad(peniko::Color::new([0.0, 1.0, 0.0, 1.0])),
                side(-1.05, -1.0),
                None,
            ),
        ]) else {
            return;
        };
        // The glowing quad spans pixels 24 to 40; blue starts at 33.
        assert_eq!(pixel(&pixels, 36, 42), [0, 0, 255, 255]);
        let behind = pixel(&pixels, 22, 32);
        assert!(behind[0] > 40 && behind[1] < 250, "{behind:?}");
    }

    #[test]
    fn scenes_without_models_never_build_their_pipelines() {
        let Some((pixels, built)) = draw_meshes(&[(quad(peniko::Color::WHITE), None)]) else {
            return;
        };
        assert_eq!(pixel(&pixels, 32, 32), [255, 255, 255, 255]);
        assert!(!built);
    }

    #[test]
    fn a_model_draws_its_texture_through_its_uvs() {
        let texture = two_texels([255, 0, 0, 255], [0, 0, 255, 255]);
        let mesh = model_quad(Some(texture), AlphaMode::Opaque, false);
        let Some((pixels, built)) = draw_meshes(&[(mesh, None)]) else {
            return;
        };
        assert!(built);
        // The quad spans pixels 16 to 48: its left half shows the left texel.
        assert_eq!(pixel(&pixels, 20, 32), [255, 0, 0, 255]);
        assert_eq!(pixel(&pixels, 44, 32), [0, 0, 255, 255]);
        assert_eq!(pixel(&pixels, 4, 4)[3], 0);
    }

    #[test]
    fn a_cut_out_model_hides_its_transparent_texels_and_an_opaque_one_ignores_them() {
        let texture = two_texels([255, 0, 0, 255], [0, 255, 0, 40]);
        let masked = model_quad(Some(texture.clone()), AlphaMode::Mask(0.5), false);
        let Some((pixels, _)) = draw_meshes(&[(masked, None)]) else {
            return;
        };
        assert_eq!(pixel(&pixels, 20, 32), [255, 0, 0, 255]);
        assert_eq!(pixel(&pixels, 44, 32)[3], 0);
        let opaque = model_quad(Some(texture), AlphaMode::Opaque, false);
        let (pixels, _) = draw_meshes(&[(opaque, None)]).unwrap();
        assert_eq!(pixel(&pixels, 44, 32), [0, 255, 0, 255]);
    }

    #[test]
    fn a_blended_model_draws_over_what_lies_behind_it() {
        let texture = two_texels([255, 255, 255, 128], [255, 255, 255, 128]);
        let mut front = model_quad(Some(texture), AlphaMode::Blend, false);
        for vertex in &mut front.vertices {
            vertex[2] = 1.0;
        }
        let back = quad(peniko::Color::from_rgb8(0, 0, 255));
        let Some((pixels, _)) = draw_meshes(&[(back, None), (front, None)]) else {
            return;
        };
        let [r, _, b, a] = pixel(&pixels, 32, 32);
        assert_eq!(a, 255);
        assert!(r > 100 && r < 200 && b == 255, "{r} {b}");
    }

    #[test]
    fn a_skinned_model_draws_where_its_joints_take_it() {
        let mesh = model_quad(None, AlphaMode::Opaque, true);
        // Joint 0 one unit to the right: the quad spans pixels 32 to 64.
        let moved = DMat4::from_translation(DVec3::X).as_mat4().to_cols_array();
        let Some((pixels, _)) = draw_meshes(&[(mesh, Some(Arc::from(vec![moved])))]) else {
            return;
        };
        assert_eq!(pixel(&pixels, 20, 32)[3], 0);
        assert_eq!(pixel(&pixels, 56, 32), [255, 255, 255, 255]);
    }

    #[test]
    fn a_morphed_model_draws_its_targets_by_their_weights() {
        // One target moves every corner a unit right: 16 pixels.
        let mut mesh = model_quad(None, AlphaMode::Opaque, false);
        let count = mesh.vertices.len();
        Arc::make_mut(mesh.surface.as_mut().unwrap()).morph_targets =
            vec![gaanim_scene::MorphTarget {
                positions: vec![[1.0, 0.0, 0.0]; count],
                normals: None,
            }];
        let weighed =
            |weight: f32| draw_morphed(&[(mesh.clone(), None, Some(Arc::from([weight])))]);
        // At weight 0 the quad spans pixels 16 to 48, at 1 from 32 to 64,
        // and at 0.5 from 24 to 56.
        let Some((rest, built)) = weighed(0.0) else {
            return;
        };
        assert!(built);
        assert_eq!(pixel(&rest, 20, 32)[3], 255);
        assert_eq!(pixel(&rest, 52, 32)[3], 0);
        let (full, _) = weighed(1.0).unwrap();
        assert_eq!(pixel(&full, 28, 32)[3], 0);
        assert_eq!(pixel(&full, 60, 32)[3], 255);
        let (half, _) = weighed(0.5).unwrap();
        for (x, alpha) in [(22, 0), (26, 255), (54, 255), (58, 0)] {
            assert_eq!(pixel(&half, x, 32)[3], alpha, "pixel {x}");
        }
    }

    #[test]
    fn a_normal_map_tilts_the_shading_of_a_flat_face() {
        // A lit card facing the camera, its one texel tilting every normal
        // toward the key light, up and to the left, or away from it.
        let card = |texel: [u8; 4]| {
            let mut mesh = model_quad(None, AlphaMode::Opaque, false);
            mesh.material = Some(Material3D::default());
            let count = mesh.vertices.len();
            Arc::make_mut(mesh.surface.as_mut().unwrap()).normal_map =
                Some(gaanim_scene::NormalMap {
                    texture: MeshTexture {
                        image: ImageData {
                            data: Blob::from(texel.to_vec()),
                            format: ImageFormat::Rgba8,
                            alpha_type: ImageAlphaType::Alpha,
                            width: 1,
                            height: 1,
                        },
                        wrap: [TextureWrap::Repeat; 2],
                        nearest: false,
                    },
                    scale: 1.0,
                    tangents: vec![[1.0, 0.0, 0.0, 1.0]; count],
                });
            mesh
        };
        let shade = |texel| {
            draw_meshes(&[(card(texel), None)]).map(|(pixels, _)| {
                let [r, g, b, _] = pixel(&pixels, 32, 32);
                u32::from(r) + u32::from(g) + u32::from(b)
            })
        };
        let Some(flat) = shade([128, 128, 255, 255]) else {
            return;
        };
        let toward = shade([38, 128, 218, 255]).unwrap();
        let away = shade([218, 128, 218, 255]).unwrap();
        // The white card saturates toward the light, so it brightens less.
        assert!(
            toward > flat + 10 && flat > away + 40,
            "{toward} {flat} {away}"
        );
        // A flat texel shades as the card without a map.
        let mut plain = model_quad(None, AlphaMode::Opaque, false);
        plain.material = Some(Material3D::default());
        let (pixels, _) = draw_meshes(&[(plain, None)]).unwrap();
        let [r, g, b, _] = pixel(&pixels, 32, 32);
        let plain = u32::from(r) + u32::from(g) + u32::from(b);
        assert!(plain.abs_diff(flat) <= 3, "{plain} {flat}");
    }

    #[test]
    fn a_skinned_lit_model_casts_its_posed_shadow() {
        // A lit floor and, a quarter unit above it, a lit card half a unit
        // wide on a joint. The key light casts its shadow 0.19 units right
        // and 0.38 down: the shadow follows where the joint takes the card.
        let mut floor = quad(peniko::Color::WHITE);
        floor.material = Some(Material3D::default());
        let mut card = model_quad(None, AlphaMode::Opaque, true);
        card.material = Some(Material3D::default());
        for vertex in &mut card.vertices {
            vertex[0] *= 0.25;
            vertex[1] *= 0.25;
            vertex[2] = 0.25;
        }
        let still = DMat4::IDENTITY.as_mat4().to_cols_array();
        let moved = DMat4::from_translation(DVec3::new(0.5, 0.0, 0.0))
            .as_mat4()
            .to_cols_array();
        let Some((rest, _)) = draw_meshes(&[
            (floor.clone(), None),
            (card.clone(), Some(Arc::from(vec![still]))),
        ]) else {
            return;
        };
        let (posed, _) =
            draw_meshes(&[(floor, None), (card, Some(Arc::from(vec![moved])))]).unwrap();
        // Pixel (33, 40) lies in the shadow of the card at rest, and in the
        // light once the joint moves it away.
        let shade = |pixels: &[u8]| pixel(pixels, 33, 40)[0];
        assert!(
            shade(&rest) + 40 < shade(&posed),
            "{} {}",
            shade(&rest),
            shade(&posed)
        );
        // Where the moved card lies, it is drawn.
        assert_eq!(pixel(&posed, 40, 32)[3], 255);
    }

    #[test]
    fn a_filled_outline_draws_over_meshes_behind_it_and_under_those_in_front() {
        let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
            return;
        };
        let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let placement = camera.to_vello_transform();
        let opacity = GlobalOpacity(1.0);
        // A red square 2 units on a side at z = 0, 16 pixels per unit, its
        // top edge through the middle of pixel row 15.
        let fill = PlaneFillData {
            glyphs: vec![gaanim_scene::PlaneGlyph {
                outline: Arc::new(kurbo::Shape::to_path(
                    &kurbo::Rect::new(-1.0, -1.0, 1.0, 1.03125),
                    0.0,
                )),
                placement: kurbo::Affine::IDENTITY,
                color: peniko::Color::from_rgb8(255, 0, 0),
                gradient: None,
                clip: None,
                image: None,
            }],
        };
        let at_origin = GlobalSpatialTransform::default();
        // Blue cards over x from 0 to 1.5 in front, and from -1.5 to 0
        // behind.
        let blue = quad(peniko::Color::from_rgb8(0, 0, 255));
        let front = placed(0.75, DVec3::new(0.75, 0.0, 1.0));
        let back = placed(0.75, DVec3::new(-0.75, 0.0, -1.0));
        let mut sources = MeshSources::default();
        let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
        let card = |transform| Content {
            transform,
            opacity: &opacity,
            mesh: Some(&blue),
            lines: None,
            material: None,
            line_width: None,
            line_range: None,
            fill: None,
            joints: None,
            glow: None,
            shadow: None,
            blur: None,
            instances: None,
            morph: None,
        };
        meshes.push(0, [(2, 1), (2, 1)], [None, None], &card(&front));
        meshes.push(0, [(3, 1), (3, 1)], [None, None], &card(&back));
        assert!(meshes.push_fill(
            0,
            (1, 1),
            None,
            &Content {
                transform: &at_origin,
                opacity: &opacity,
                mesh: None,
                lines: None,
                material: None,
                line_width: None,
                line_range: None,
                fill: Some(&fill),
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
            },
        ));
        let frames: Vec<GpuMeshFrame> = meshes
            .finish(&camera, ndc_to_canvas, line_width, &Illumination::default())
            .into_iter()
            .map(|(_, frame)| frame)
            .collect();
        // The outlines draw after the opaque meshes.
        assert!(frames[0].draws[2].source.fill.is_some());
        let mut scene = vello::Scene::new();
        scene.append(&frames[0].scene(), Some(placement));
        let mut three_d = GpuThreeD::default();
        three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
        let target = gpu.target(64, 64);
        gpu.render(&scene, &target);
        let pixels = gpu.read(&target);
        let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        // Over the card behind it, the square; beside it, that card; in
        // front of it, the other card; beyond all, nothing.
        assert_eq!(pixel(24, 32), [255, 0, 0, 255]);
        assert_eq!(pixel(11, 32), [0, 0, 255, 255]);
        assert_eq!(pixel(40, 32), [0, 0, 255, 255]);
        assert_eq!(pixel(32, 8)[3], 0);
        // Its top edge: half covered, over nothing.
        let edge = pixel(32 - 4, 15);
        assert!((100..=156).contains(&edge[3]), "{edge:?}");
        assert_eq!(pixel(32 - 4, 17), [255, 0, 0, 255]);
    }

    #[test]
    fn strokes_aligned_inside_and_outside_cover_their_ring() {
        let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
            return;
        };
        let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let placement = camera.to_vello_transform();
        let opacity = GlobalOpacity(1.0);
        let at_origin = GlobalSpatialTransform::default();
        // A circle of radius 1.2, stroked a quarter of a unit wide: 4 pixels.
        let circle = kurbo::Shape::to_path(&kurbo::Circle::new((0.1, -0.05), 1.2), 1e-3);
        let pen = kurbo::Stroke::new(0.25);
        let at_corner = |path: kurbo::BezPath| {
            let corner = kurbo::Shape::bounding_box(&path).origin().to_vec2();
            (Arc::new(kurbo::Affine::translate(-corner) * path), corner)
        };
        for align in [
            crate::effects::StrokeAlign::Inside,
            crate::effects::StrokeAlign::Outside,
        ] {
            let doubled = crate::pipeline::clipped_pen(&pen);
            let (outline, corner) = at_corner(kurbo::stroke(
                circle.iter(),
                &doubled,
                &kurbo::StrokeOpts::default(),
                1e-3,
            ));
            let (contour, contour_corner) = at_corner(circle.clone());
            let fill = PlaneFillData {
                glyphs: vec![gaanim_scene::PlaneGlyph {
                    outline,
                    placement: kurbo::Affine::translate(corner),
                    color: peniko::Color::from_rgb8(255, 0, 0),
                    gradient: None,
                    clip: Some(gaanim_scene::PlaneClip {
                        outline: contour,
                        offset: corner - contour_corner,
                        outside: align == crate::effects::StrokeAlign::Outside,
                    }),
                    image: None,
                }],
            };
            let mut sources = MeshSources::default();
            let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
            assert!(meshes.push_fill(
                0,
                (1, 1),
                None,
                &Content {
                    transform: &at_origin,
                    opacity: &opacity,
                    mesh: None,
                    lines: None,
                    material: None,
                    line_width: None,
                    line_range: None,
                    fill: Some(&fill),
                    joints: None,
                    glow: None,
                    shadow: None,
                    blur: None,
                    instances: None,
                    morph: None,
                },
            ));
            let frames: Vec<GpuMeshFrame> = meshes
                .finish(&camera, ndc_to_canvas, line_width, &Illumination::default())
                .into_iter()
                .map(|(_, frame)| frame)
                .collect();
            let mut scene = vello::Scene::new();
            scene.append(&frames[0].scene(), Some(placement));
            let mut three_d = GpuThreeD::default();
            three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
            let target = gpu.target(64, 64);
            gpu.render(&scene, &target);
            let drawn = gpu.read(&target);
            // Alpha against the exact area of the ring the stroke keeps,
            // within the circle or beyond it, by supersampling. (Vello's clip
            // layers soften these edges more; the area is the reference.)
            let (inner, outer) = match align {
                crate::effects::StrokeAlign::Inside => (0.95, 1.2),
                _ => (1.2, 1.45),
            };
            let exact = |x: usize, y: usize| {
                let mut inside = 0;
                for i in 0..8 {
                    for j in 0..8 {
                        let px = -2.0 + (x as f64 + (f64::from(i) + 0.5) / 8.0) / 16.0;
                        let py = 2.0 - (y as f64 + (f64::from(j) + 0.5) / 8.0) / 16.0;
                        let r = (px - 0.1).hypot(py + 0.05);
                        inside += usize::from((inner..=outer).contains(&r));
                    }
                }
                inside as f64 / 64.0 * 255.0
            };
            let (mut total, mut worst, mut count) = (0.0, 0.0_f64, 0);
            for y in 0..64 {
                for x in 0..64 {
                    let truth = exact(x, y);
                    let alpha = f64::from(drawn[(y * 64 + x) * 4 + 3]);
                    if truth > 0.0 || alpha > 0.0 {
                        total += (alpha - truth).abs();
                        worst = worst.max((alpha - truth).abs());
                        count += 1;
                    }
                }
            }
            let mean = total / f64::from(count);
            assert!(
                mean <= 5.0 && worst <= 40.0,
                "{align:?}: off by {mean:.1} on average, {worst:.0} at most"
            );
        }
    }

    #[test]
    fn gradient_fills_match_their_2d_drawing() {
        let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
            return;
        };
        let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let placement = camera.to_vello_transform();
        let opacity = GlobalOpacity(1.0);
        let at_origin = GlobalSpatialTransform::default();
        // A square 3 units on a side, 16 pixels per unit: pixels 8 to 56.
        let square = kurbo::Shape::to_path(&kurbo::Rect::new(-1.5, -1.5, 1.5, 1.5), 0.0);
        let red = peniko::Color::from_rgb8(255, 0, 0);
        let blue = peniko::Color::from_rgb8(0, 0, 255);
        let white = peniko::Color::from_rgb8(255, 255, 255);
        let gradients = [
            peniko::Gradient::new_linear((-1.5, -1.0), (1.5, 1.0)).with_stops([red, blue]),
            peniko::Gradient::new_radial((0.2, 0.3), 1.2)
                .with_extend(peniko::Extend::Reflect)
                .with_stops([white, red, blue]),
            peniko::Gradient::new_sweep((0.0, 0.0), 0.5, 5.0).with_stops([red, white, blue]),
        ];
        for gradient in gradients {
            // The square laid at its corner, as in-world outlines are, with
            // its gradient mapped back to the square's units.
            let corner = kurbo::Vec2::new(-1.5, -1.5);
            let fill = PlaneFillData {
                glyphs: vec![gaanim_scene::PlaneGlyph {
                    outline: Arc::new(kurbo::Affine::translate(-corner) * square.clone()),
                    placement: kurbo::Affine::translate(corner),
                    color: red,
                    gradient: Some((Arc::new(gradient.clone()), kurbo::Affine::translate(corner))),
                    clip: None,
                    image: None,
                }],
            };
            let mut sources = MeshSources::default();
            let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
            assert!(meshes.push_fill(
                0,
                (1, 1),
                None,
                &Content {
                    transform: &at_origin,
                    opacity: &opacity,
                    mesh: None,
                    lines: None,
                    material: None,
                    line_width: None,
                    line_range: None,
                    fill: Some(&fill),
                    joints: None,
                    glow: None,
                    shadow: None,
                    blur: None,
                    instances: None,
                    morph: None,
                },
            ));
            let frames: Vec<GpuMeshFrame> = meshes
                .finish(&camera, ndc_to_canvas, line_width, &Illumination::default())
                .into_iter()
                .map(|(_, frame)| frame)
                .collect();
            let mut scene = vello::Scene::new();
            scene.append(&frames[0].scene(), Some(placement));
            let mut three_d = GpuThreeD::default();
            three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
            let target = gpu.target(64, 64);
            gpu.render(&scene, &target);
            let drawn = gpu.read(&target);
            // Vello samples a gradient at a pixel's corner, the 3D pass at its
            // middle: half a pixel (a 32nd of a unit) up and left, the 2D
            // gradient is sampled where the 3D one is.
            let mut flat = vello::Scene::new();
            flat.fill(
                peniko::Fill::NonZero,
                placement,
                &peniko::Brush::Gradient(gradient.clone()),
                Some(kurbo::Affine::translate((-1.0 / 32.0, 1.0 / 32.0))),
                &square,
            );
            let target = gpu.target(64, 64);
            gpu.render(&flat, &target);
            let expected = gpu.read(&target);
            // Inside the square, clear of its edges.
            for y in 12..52 {
                for x in 12..52 {
                    let at = (y * 64 + x) * 4;
                    let worst = (0..4)
                        .map(|channel| drawn[at + channel].abs_diff(expected[at + channel]))
                        .max()
                        .unwrap();
                    assert!(
                        worst <= 3,
                        "{:?} at ({x}, {y}): {:?} against {:?}",
                        gradient.kind,
                        &drawn[at..at + 4],
                        &expected[at..at + 4],
                    );
                }
            }
        }
    }

    /// An outline filled with an image draws its texels where Vello draws
    /// the image, the right way up, and nothing where the image is
    /// transparent.
    #[test]
    fn image_fills_match_their_2d_drawing() {
        let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
            return;
        };
        let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let placement = camera.to_vello_transform();
        let opacity = GlobalOpacity(1.0);
        let at_origin = GlobalSpatialTransform::default();
        // A square 3 units on a side, 16 pixels per unit: pixels 8 to 56,
        // filled with an image of 8 x 8 texels, 6 pixels each, its last
        // column transparent.
        let square = kurbo::Shape::to_path(&kurbo::Rect::new(-1.5, -1.5, 1.5, 1.5), 0.0);
        let texels: Vec<u8> = (0..64_u32)
            .flat_map(|at| {
                let (column, row) = (at % 8, at / 8);
                let alpha = if column == 7 { 0 } else { 255 };
                [(column * 32) as u8, (row * 32) as u8, 128, alpha]
            })
            .collect();
        let mut brush = peniko::ImageBrush::new(ImageData {
            data: Blob::from(texels),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: 8,
            height: 8,
        });
        brush.sampler.quality = peniko::ImageQuality::Low;
        let to_local = kurbo::Affine::translate((-1.5, -1.5)) * kurbo::Affine::scale(3.0 / 8.0);
        let corner = kurbo::Vec2::new(-1.5, -1.5);
        let fill = PlaneFillData {
            glyphs: vec![gaanim_scene::PlaneGlyph {
                outline: Arc::new(kurbo::Affine::translate(-corner) * square.clone()),
                placement: kurbo::Affine::translate(corner),
                color: peniko::Color::WHITE,
                gradient: None,
                clip: None,
                image: Some(gaanim_scene::PlaneImage {
                    brush: brush.clone(),
                    to_pixels: to_local.inverse() * kurbo::Affine::translate(corner),
                }),
            }],
        };
        let mut sources = MeshSources::default();
        let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
        assert!(meshes.push_fill(
            0,
            (1, 1),
            None,
            &Content {
                transform: &at_origin,
                opacity: &opacity,
                mesh: None,
                lines: None,
                material: None,
                line_width: None,
                line_range: None,
                fill: Some(&fill),
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
            },
        ));
        let frames: Vec<GpuMeshFrame> = meshes
            .finish(&camera, ndc_to_canvas, line_width, &Illumination::default())
            .into_iter()
            .map(|(_, frame)| frame)
            .collect();
        let mut scene = vello::Scene::new();
        scene.append(&frames[0].scene(), Some(placement));
        let mut three_d = GpuThreeD::default();
        three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
        let target = gpu.target(64, 64);
        gpu.render(&scene, &target);
        let drawn = gpu.read(&target);
        let mut flat = vello::Scene::new();
        flat.draw_image(&brush, placement * to_local);
        let target = gpu.target(64, 64);
        gpu.render(&flat, &target);
        let expected = gpu.read(&target);
        // The middle of each texel.
        for row in 0..8 {
            for column in 0..8 {
                let (x, y) = (8 + column * 6 + 3, 8 + row * 6 + 3);
                let at = (y * 64 + x) * 4;
                let worst = (0..4)
                    .map(|channel| drawn[at + channel].abs_diff(expected[at + channel]))
                    .max()
                    .unwrap();
                assert!(
                    worst <= 3,
                    "texel ({column}, {row}): {:?} against {:?}",
                    &drawn[at..at + 4],
                    &expected[at..at + 4],
                );
            }
        }
    }

    fn wait(device: &wgpu::Device) {
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
    }

    /// Milliseconds a frame of `run` takes on the wall clock: the median of
    /// batches of frames queued at once.
    fn per_frame(device: &wgpu::Device, mut run: impl FnMut()) -> f64 {
        const BATCH: u32 = 40;
        run();
        wait(device);
        let mut batches: Vec<f64> = (0..7)
            .map(|_| {
                let start = std::time::Instant::now();
                for _ in 0..BATCH {
                    run();
                }
                wait(device);
                start.elapsed().as_secs_f64() * 1000.0 / f64::from(BATCH)
            })
            .collect();
        batches.sort_by(f64::total_cmp);
        batches[batches.len() / 2]
    }

    /// The median milliseconds the GPU spends in `frame`'s color passes,
    /// timed by `three_d.timing`.
    fn gpu_ms(
        three_d: &mut GpuThreeD,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &GpuMeshFrame,
        view: (&wgpu::Texture, &wgpu::TextureView),
    ) -> f64 {
        const ROUNDS: u64 = 100;
        let buffer = |size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let resolved = buffer(
            16,
            wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        );
        let read = buffer(
            16 * ROUNDS,
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );
        three_d.draw(device, queue, frame, view);
        for round in 0..ROUNDS {
            three_d.draw(device, queue, frame, view);
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.resolve_query_set(three_d.timing.as_ref().unwrap(), 0..2, &resolved, 0);
            encoder.copy_buffer_to_buffer(&resolved, 0, &read, round * 16, 16);
            queue.submit(Some(encoder.finish()));
        }
        read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        wait(device);
        let period = f64::from(queue.get_timestamp_period());
        let mut times: Vec<f64> = read
            .slice(..)
            .get_mapped_range()
            .chunks_exact(16)
            .map(|pair| {
                let begin = u64::from_le_bytes(pair[..8].try_into().unwrap());
                let end = u64::from_le_bytes(pair[8..].try_into().unwrap());
                end.saturating_sub(begin) as f64 * period / 1e6
            })
            .collect();
        times.sort_by(f64::total_cmp);
        times[times.len() / 2]
    }

    /// Times 1,000 glyphs of New Computer Modern (the default text face, of
    /// cubic outlines) filled from their curves by the 3D pass, against
    /// Vello filling the same outlines, at three sizes on a 1920 x 1080
    /// image, and prints what they upload. Run with `just test-package
    /// gaanim_renderer --lib -- --ignored --nocapture bench_filled`.
    #[test]
    #[ignore]
    fn bench_filled_outlines_of_a_thousand_glyphs() {
        struct Outline(kurbo::BezPath);
        impl ttf_parser::OutlineBuilder for Outline {
            fn move_to(&mut self, x: f32, y: f32) {
                self.0.move_to((f64::from(x), f64::from(y)));
            }
            fn line_to(&mut self, x: f32, y: f32) {
                self.0.line_to((f64::from(x), f64::from(y)));
            }
            fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
                self.0
                    .quad_to((f64::from(x1), f64::from(y1)), (f64::from(x), f64::from(y)));
            }
            fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
                self.0.curve_to(
                    (f64::from(x1), f64::from(y1)),
                    (f64::from(x2), f64::from(y2)),
                    (f64::from(x), f64::from(y)),
                );
            }
            fn close(&mut self) {
                self.0.close_path();
            }
        }
        let font = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/assets/fonts/NewCMMath-Regular.otf"
        ))
        .expect("the docs' math font");
        let face = ttf_parser::Face::parse(&font, 0).unwrap();
        let units = f64::from(face.units_per_em());
        let mut outlines: HashMap<char, Option<(Arc<kurbo::BezPath>, f64)>> = HashMap::new();
        let mut glyph = |c: char| {
            outlines
                .entry(c)
                .or_insert_with(|| {
                    let id = face.glyph_index(c)?;
                    let advance = f64::from(face.glyph_hor_advance(id)?);
                    let mut outline = Outline(kurbo::BezPath::new());
                    face.outline_glyph(id, &mut outline);
                    Some((Arc::new(outline.0), advance))
                })
                .clone()
        };
        let text = "Sphinx of black quartz, judge my vow. The five boxing wizards jump quickly! ";
        // `count` glyphs in rows across the frame, `em` pixels tall.
        let mut layout = |em: f64, count: usize| {
            let scale = em / units;
            let (mut x, mut y) = (-940.0, 540.0 - em * 1.2);
            let mut glyphs = Vec::new();
            for c in text.chars().cycle() {
                if glyphs.len() == count {
                    break;
                }
                let Some((outline, advance)) = glyph(c) else {
                    continue;
                };
                if x + advance * scale > 940.0 {
                    (x, y) = (-940.0, y - em * 1.3);
                }
                if !outline.elements().is_empty() {
                    glyphs.push(gaanim_scene::PlaneGlyph {
                        outline,
                        placement: kurbo::Affine::translate((x, y)) * kurbo::Affine::scale(scale),
                        color: peniko::Color::from_rgb8(20, 20, 30),
                        gradient: None,
                        clip: None,
                        image: None,
                    });
                }
                x += advance * scale;
            }
            PlaneFillData { glyphs }
        };

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all().with_env(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapters =
            pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all().with_env()));
        for adapter in adapters {
            let info = adapter.get_info();
            if info.device_type == wgpu::DeviceType::Cpu {
                continue;
            }
            println!("adapter: {} ({:?})", info.name, info.backend);
            let timestamps = wgpu::Features::TIMESTAMP_QUERY;
            if !adapter.features().contains(timestamps) {
                println!("  no timestamp queries");
                continue;
            }
            let (device, queue) =
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    required_features: timestamps,
                    ..Default::default()
                }))
                .unwrap();
            let mut renderer = vello::Renderer::new(
                &device,
                vello::RendererOptions {
                    use_cpu: false,
                    antialiasing_support: vello::AaSupport::area_only(),
                    num_init_threads: None,
                    pipeline_cache: None,
                },
            )
            .unwrap();
            let (width, height) = (1920, 1080);
            let mut camera = Camera::ortho_2d_frame(1920.0, 1080.0, width, height);
            camera.position = DVec3::new(0.0, 0.0, 10.0);
            let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
            let placement = camera.to_vello_transform();
            let opacity = GlobalOpacity(1.0);
            let at_origin = GlobalSpatialTransform::default();
            let vello_target = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let vello_view = vello_target.create_view(&Default::default());
            let vello_ms = |scene: &vello::Scene, renderer: &mut vello::Renderer| {
                per_frame(&device, || {
                    renderer
                        .render_to_texture(
                            &device,
                            &queue,
                            scene,
                            &vello_view,
                            &vello::RenderParams {
                                base_color: peniko::Color::TRANSPARENT,
                                width,
                                height,
                                antialiasing_method: vello::AaConfig::Area,
                            },
                        )
                        .unwrap();
                })
            };
            // What a frame costs with nothing to draw but a tiny card.
            let card = quad(peniko::Color::WHITE);
            let speck = placed(0.5, DVec3::ZERO);
            let frame_of = |content: &Content<'_>, fill: bool| {
                let mut sources = MeshSources::default();
                let mut meshes =
                    GpuMeshes::new(&mut sources, (width, height), placement, 0).unwrap();
                if fill {
                    meshes.push_fill(0, (1, 1), None, content);
                } else {
                    meshes.push(0, [(1, 1), (1, 1)], [None, None], content);
                }
                meshes
                    .finish(&camera, ndc_to_canvas, line_width, &Illumination::default())
                    .remove(0)
                    .1
            };
            let empty = frame_of(
                &Content {
                    transform: &speck,
                    opacity: &opacity,
                    mesh: Some(&card),
                    lines: None,
                    material: None,
                    line_width: None,
                    line_range: None,
                    fill: None,
                    joints: None,
                    glow: None,
                    shadow: None,
                    blur: None,
                    instances: None,
                    morph: None,
                },
                false,
            );
            let mut three_d = GpuThreeD {
                timing: Some(device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: None,
                    ty: wgpu::QueryType::Timestamp,
                    count: 2,
                })),
                ..Default::default()
            };
            let target = Target::new(&device, &empty.image);
            let view = (&target.resolved, &target.resolved_view);
            let base_gpu = gpu_ms(&mut three_d, &device, &queue, &empty, view);
            let base_3d = per_frame(&device, || three_d.draw(&device, &queue, &empty, view));
            let base_vello = vello_ms(&vello::Scene::new(), &mut renderer);
            println!(
                "empty frame: 3D pass {base_gpu:.3} ms on the GPU, {base_3d:.3} ms wall; Vello {base_vello:.3} ms wall"
            );
            println!(
                "em px | glyphs | distinct | build ms | curves B | bands B | instances B | 3D GPU ms | 3D wall ms | Vello wall ms"
            );
            for (em, count) in [(12.0, 1000), (24.0, 1000), (36.0, 1000), (12.0, 10_000)] {
                let fill = layout(em, count);
                let start = std::time::Instant::now();
                let built = crate::plane_fill::build(&fill).unwrap();
                let build_ms = start.elapsed().as_secs_f64() * 1000.0;
                let distinct: HashSet<*const kurbo::BezPath> = fill
                    .glyphs
                    .iter()
                    .map(|g| Arc::as_ptr(&g.outline))
                    .collect();
                let frame = frame_of(
                    &Content {
                        transform: &at_origin,
                        opacity: &opacity,
                        mesh: None,
                        lines: None,
                        material: None,
                        line_width: None,
                        line_range: None,
                        fill: Some(&fill),
                        joints: None,
                        glow: None,
                        shadow: None,
                        blur: None,
                        instances: None,
                        morph: None,
                    },
                    true,
                );
                let gpu = gpu_ms(&mut three_d, &device, &queue, &frame, view) - base_gpu;
                let wall =
                    per_frame(&device, || three_d.draw(&device, &queue, &frame, view)) - base_3d;
                let mut scene = vello::Scene::new();
                for glyph in &fill.glyphs {
                    scene.fill(
                        peniko::Fill::NonZero,
                        placement * glyph.placement,
                        glyph.color,
                        None,
                        &*glyph.outline,
                    );
                }
                let vello = vello_ms(&scene, &mut renderer) - base_vello;
                println!(
                    "{em} | {} | {} | {build_ms:.2} | {} | {} | {} | {gpu:.3} | {wall:.3} | {vello:.3}",
                    built.count,
                    distinct.len(),
                    built.curves.len(),
                    built.bands.len(),
                    built.instances.len(),
                );
            }
        }
    }
}
