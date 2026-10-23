//! What a bundle stores: static scene data, deduplicated tables and frames.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::Entity;
use gaanim_core::{kurbo, peniko};
use gaanim_renderer::effects::ChalkBrush;
use gaanim_renderer::effects::{
    CameraViewBackground, ClipMask, DropShadow, GaussianBlur, Glow, StrokeAlign, StrokeProfile,
};
use gaanim_renderer::fragment::{FragmentRecipe, GroupShadow};
use gaanim_renderer::object_effects::{Glass, MatteMode};
use gaanim_renderer::pipeline::{
    CapturedEffect, CapturedElement, CapturedGlass, CapturedMatte, CapturedThreeD,
    CapturedThreeDSource, CapturedTransition, CapturedView, FrameCapture, FrozenThreeD,
    ThreeDComposition,
};
use gaanim_renderer::post_process::PostProcessShader;
use gaanim_scene::{
    RasterImage, RenderOrder, StrokeBrush, TransitionMask, TransitionShader, TransitionShaderFrame,
    TransitionSide,
};

use crate::BundleError;
use crate::codec::{self, Interner, Reader, Resolver, Writer};

type Result<T> = std::result::Result<T, BundleError>;

fn corrupt(what: &str) -> BundleError {
    BundleError::Corrupt(what.to_owned())
}

// ---------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------

/// A post-process pass active in a frame, with its evaluated uniforms.
#[derive(Clone, Debug, PartialEq)]
pub struct PostPass {
    /// Index into [`crate::SceneData::post_shaders`].
    pub shader: u32,
    pub values: Vec<f32>,
}

/// One recorded frame.
#[derive(Clone)]
pub struct Frame {
    /// Timeline seconds the frame shows.
    pub time: f64,
    /// The camera after every binding, rig and animation.
    pub camera: gaanim_math::Camera,
    /// What the 2D renderer draws.
    pub capture: FrameCapture,
    /// Active post-process passes, evaluated at `time`.
    pub post: Vec<PostPass>,
    /// Sub-frames a motion-blurred export averages into this frame, in
    /// order; empty without motion blur. A preview shows the frame itself.
    pub motion_blur: Vec<Frame>,
}

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

/// Dense keys for the entities a recording meets, so a bundle never stores
/// the recording world's entity bits.
#[derive(Default)]
pub struct EntityKeys {
    keys: HashMap<Entity, u32>,
    /// The entities are stand-ins for keys already ([`key_entity`]), as the
    /// frames of a bundle being written again are: they keep their keys, so
    /// the static data that names drawables by key stays right.
    pub(crate) keep: bool,
}

impl EntityKeys {
    pub fn key(&mut self, entity: Entity) -> u32 {
        if self.keep {
            return entity.index_u32();
        }
        let next = self.keys.len() as u32;
        *self.keys.entry(entity).or_insert(next)
    }
}

/// A stand-in entity for a key, only ever compared for equality.
pub fn key_entity(key: u32) -> Result<Entity> {
    Entity::from_raw_u32(key).ok_or_else(|| corrupt("entity key out of range"))
}

// ---------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------

/// How many items of each table a segment holds, or where it starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TableCounts {
    pub paths: u32,
    pub images: u32,
    pub recipes: u32,
    pub strings: u32,
    pub transitions: u32,
    pub effects: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub meshes: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub lines: u32,
    /// Copies of meshes drawn at many points (format 9).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub copies: u32,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

impl TableCounts {
    pub(crate) fn add(self, other: Self) -> Self {
        Self {
            paths: self.paths + other.paths,
            images: self.images + other.images,
            recipes: self.recipes + other.recipes,
            strings: self.strings + other.strings,
            transitions: self.transitions + other.transitions,
            effects: self.effects + other.effects,
            meshes: self.meshes + other.meshes,
            lines: self.lines + other.lines,
            copies: self.copies + other.copies,
        }
    }

    fn sub(self, other: Self) -> Self {
        Self {
            paths: self.paths - other.paths,
            images: self.images - other.images,
            recipes: self.recipes - other.recipes,
            strings: self.strings - other.strings,
            transitions: self.transitions - other.transitions,
            effects: self.effects - other.effects,
            meshes: self.meshes - other.meshes,
            lines: self.lines - other.lines,
            copies: self.copies - other.copies,
        }
    }

    fn is_empty(self) -> bool {
        self == Self::default()
    }
}

/// Deduplicated paths, images, recipes and strings built while recording.
///
/// The tables grow in segments: the items a chunk of frames meets first are
/// stored with that chunk (`segments/NNNNNN.bin`), and the chunk lists every
/// segment its frames refer to, so a player that opens or seeks downloads
/// those segments and not the whole history before them.
pub struct Tables {
    pub(crate) paths: Vec<Arc<kurbo::BezPath>>,
    path_by_ptr: HashMap<usize, u32>,
    path_by_hash: HashMap<blake3::Hash, u32>,
    pub(crate) images: Vec<peniko::ImageData>,
    image_by_id: HashMap<u64, u32>,
    image_by_hash: HashMap<blake3::Hash, u32>,
    /// Encoded recipes; they reference paths and images by index.
    pub(crate) recipes: Vec<Vec<u8>>,
    recipe_by_ptr: HashMap<usize, (Arc<FragmentRecipe>, u32)>,
    recipe_by_hash: HashMap<blake3::Hash, u32>,
    pub(crate) strings: Vec<Arc<str>>,
    string_index: HashMap<Arc<str>, u32>,
    /// Shader transitions the frames blend with, deduplicated.
    pub(crate) transition_shaders: Vec<Arc<TransitionShader>>,
    /// Passes of the drawables' shader effects, deduplicated.
    pub(crate) effect_shaders: Vec<PostProcessShader>,
    /// Native 3D meshes and line lists, encoded, deduplicated by content,
    /// and by the address of a frame's `Arc` while it lives: frames capture
    /// unchanged 3D content in the same `Arc`, which is then encoded once.
    pub(crate) meshes: Vec<Vec<u8>>,
    mesh_by_hash: HashMap<blake3::Hash, u32>,
    mesh_by_address: ByAddress<gaanim_scene::TriangleMeshData>,
    /// Segments each mesh needs besides its own: those of its texture.
    mesh_deps: Vec<Box<[u32]>>,
    pub(crate) lines: Vec<Vec<u8>>,
    lines_by_hash: HashMap<blake3::Hash, u32>,
    lines_by_address: ByAddress<gaanim_scene::LineListData>,
    /// The last meshes and line lists of the segment being filled, by the
    /// drawable that drew them and their layout: a mesh that only moves its
    /// vertices stores their change from them (see [`predict`]).
    mesh_chains: HashMap<(u32, blake3::Hash), Chain>,
    line_chains: HashMap<(u32, blake3::Hash), Chain>,
    /// The copies of meshes drawn at many points, as line lists are stored.
    pub(crate) copies: Vec<Vec<u8>>,
    copies_by_hash: HashMap<blake3::Hash, u32>,
    copies_by_address: ByAddress<gaanim_scene::MeshCopies>,
    copy_chains: HashMap<(u32, blake3::Hash), Chain>,
    /// Keys of the drawables the post-process leaves out.
    pub(crate) post_exempt: BTreeSet<u32>,
    /// Whether a frame draws glass, which only a GPU pass composes.
    pub(crate) glass: bool,
    /// Where each segment starts; the last one is the segment being filled.
    starts: Vec<TableCounts>,
    /// Segments the items handed out since [`Tables::take_used`] belong to.
    used: BTreeSet<u32>,
    /// Segments each recipe needs: its own and those of its paths and images.
    recipe_deps: Vec<Box<[u32]>>,
    /// While a recipe or a Lottie frame is encoded, the segments it refers to.
    collecting: Option<BTreeSet<u32>>,
    /// About how many bytes the segment being filled holds, encoded.
    pending_bytes: usize,
    /// For a bundle packed for the web, the most output pixels per unit each
    /// path is drawn at, by its address: segments store paths on a grid
    /// fine enough for that (see [`grid_exponent`]) instead of exactly.
    pub(crate) grid: Option<HashMap<usize, f64>>,
}

impl Default for Tables {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            path_by_ptr: HashMap::new(),
            path_by_hash: HashMap::new(),
            images: Vec::new(),
            image_by_id: HashMap::new(),
            image_by_hash: HashMap::new(),
            recipes: Vec::new(),
            recipe_by_ptr: HashMap::new(),
            recipe_by_hash: HashMap::new(),
            strings: Vec::new(),
            string_index: HashMap::new(),
            transition_shaders: Vec::new(),
            effect_shaders: Vec::new(),
            meshes: Vec::new(),
            mesh_by_hash: HashMap::new(),
            mesh_deps: Vec::new(),
            mesh_by_address: ByAddress::default(),
            lines: Vec::new(),
            lines_by_hash: HashMap::new(),
            lines_by_address: ByAddress::default(),
            mesh_chains: HashMap::new(),
            line_chains: HashMap::new(),
            copies: Vec::new(),
            copies_by_hash: HashMap::new(),
            copies_by_address: ByAddress::default(),
            copy_chains: HashMap::new(),
            post_exempt: BTreeSet::new(),
            glass: false,
            starts: vec![TableCounts::default()],
            used: BTreeSet::new(),
            recipe_deps: Vec::new(),
            collecting: None,
            pending_bytes: 0,
            grid: None,
        }
    }
}

/// Largest error, in pixels of a 4K export, that a path packed for the web
/// may have: half its grid step. A shift this small changes an edge pixel's
/// coverage by under 2%.
pub(crate) const GRID_PIXELS: f64 = 1.0 / 32.0;

/// A mesh or line list of the segment being filled, as later ones with its
/// layout are predicted from it: its index, and its positions and normals
/// as the bundle decodes them, on the grid they are stored on.
struct Recent {
    index: u32,
    points: Vec<[f32; 3]>,
    normals: Option<Vec<[f32; 3]>>,
    grid: Option<i32>,
}

/// The last two items of one layout in the segment being filled.
#[derive(Default)]
struct Chain {
    last: Option<Recent>,
    before: Option<Recent>,
}

/// An item [`predict`] stores: its index, where its table's segment being
/// filled starts, whether the bundle is packed for the web, and its
/// positions and normals.
struct Item<'a> {
    index: u32,
    start: u32,
    web: bool,
    points: &'a [[f32; 3]],
    normals: Option<&'a [[f32; 3]]>,
}

/// Encode a new mesh or line list (`whole` is its lossless whole encoding,
/// `encode` writes it with a storage) as the smallest of: whole, or the
/// change of its positions and normals from the last one its drawable drew
/// with its layout in the segment, or from the line through the last two,
/// as a surface that bends or a curve that moves from frame to frame
/// changes little. Packed for the web, its positions go on a grid
/// ([`grid_bounds`]), the one of the last item when that is fine enough,
/// so it can be predicted from it. Then it becomes the last of its chain.
fn predict(
    chain: &mut Chain,
    item: Item<'_>,
    whole: Vec<u8>,
    mut encode: impl FnMut(codec::Storage<'_>) -> Vec<u8>,
) -> Vec<u8> {
    let start = item.start;
    fn in_segment(recent: Option<&Recent>, start: u32) -> Option<&Recent> {
        recent.filter(|recent| recent.index >= start)
    }
    let grid = match item.web.then(|| grid_bounds(item.points)).flatten() {
        Some((coarsest, finest)) => Some(
            in_segment(chain.last.as_ref(), start)
                .and_then(|last| last.grid)
                .filter(|grid| (finest..=coarsest).contains(grid))
                .unwrap_or(coarsest),
        ),
        None => None,
    };
    let mut bases = Vec::new();
    for recent in [chain.last.as_ref(), chain.before.as_ref()] {
        match in_segment(recent, start).filter(|recent| recent.grid == grid) {
            Some(recent) => bases.push(codec::Base {
                back: item.index - recent.index,
                points: &recent.points,
                normals: recent.normals.as_deref(),
            }),
            None => break,
        }
    }
    let mut best = match grid {
        None => whole,
        Some(_) => encode(codec::Storage { bases: &[], grid }),
    };
    for count in 1..=bases.len() {
        let bytes = encode(codec::Storage {
            bases: &bases[..count],
            grid,
        });
        if bytes.len() < best.len() {
            best = bytes;
        }
    }
    let decoded = |points: &[[f32; 3]], grid: Option<i32>| -> Vec<[f32; 3]> {
        points
            .iter()
            .map(|point| codec::on_grid(*point, grid))
            .collect()
    };
    let recent = Recent {
        index: item.index,
        points: decoded(item.points, grid),
        normals: item
            .normals
            .map(|normals| decoded(normals, grid.map(|_| codec::NORMAL_GRID))),
        grid,
    };
    chain.before = chain.last.replace(recent);
    best
}

/// The interner a mesh is encoded again with, its texture already in the
/// images table at `texture`.
struct Interned {
    /// The table index of each of its textures, by the id of its pixels.
    textures: Vec<(u64, u32)>,
}

impl codec::Interner for Interned {
    fn path(&mut self, _: &Arc<kurbo::BezPath>) -> u32 {
        unreachable!("a mesh holds no paths")
    }

    fn image(&mut self, image: &peniko::ImageData) -> u32 {
        self.textures
            .iter()
            .find(|(id, _)| *id == image.data.id())
            .map(|(_, index)| *index)
            .expect("the mesh's textures are in the table")
    }
}

/// What a predicted mesh shares with the meshes it is predicted from:
/// everything but the values of its positions and normals. Its surface
/// (an imported model's) counts by address.
fn mesh_layout(mesh: &gaanim_scene::TriangleMeshData) -> blake3::Hash {
    let mut w = Writer::new();
    w.len(mesh.vertices.len());
    w.option(mesh.normals.as_ref(), |w, normals| w.len(normals.len()));
    codec::write_indices(&mut w, &mesh.indices);
    w.option(mesh.uvs.as_deref(), codec::write_points);
    w.option(mesh.color, codec::write_color);
    w.option(mesh.colors.as_deref(), codec::write_points);
    w.option(mesh.material.as_ref(), codec::write_material);
    w.option(mesh.surface.as_ref(), |w, surface| {
        w.var(Arc::as_ptr(surface) as usize as u64)
    });
    blake3::hash(&w.buf)
}

/// What a predicted line list shares with those it is predicted from:
/// everything but its points' values.
fn lines_layout(lines: &gaanim_scene::LineListData) -> blake3::Hash {
    let mut w = Writer::new();
    w.len(lines.points.len());
    w.option(lines.indices.as_deref(), codec::write_indices);
    w.bool(lines.strip);
    codec::write_color(&mut w, lines.color);
    w.option(lines.colors.as_deref(), codec::write_points);
    blake3::hash(&w.buf)
}

/// What predicted copies share with those they are predicted from:
/// everything but their positions' values.
fn copies_layout(copies: &gaanim_scene::MeshCopies) -> blake3::Hash {
    let mut w = Writer::new();
    w.len(copies.positions.len());
    w.option(copies.scales.as_deref(), codec::write_points);
    w.option(copies.rotations.as_deref(), codec::write_points);
    w.option(copies.colors.as_deref(), codec::write_points);
    blake3::hash(&w.buf)
}

/// The grids (exponents of powers of two) a bundle packed for the web may
/// store `points` on, the coarsest and the finest: the coarsest power of
/// two finer than 2^-17 of their size, so a 4K export in which they fill
/// the frame moves no point more than 1/64 of a pixel, as paths, and the
/// finest on which an `f32` still holds every multiple out to the farthest
/// point.
/// `None` when a point is not finite: those are stored exactly.
pub(crate) fn grid_bounds(points: &[[f32; 3]]) -> Option<(i32, i32)> {
    let mut low = [f32::INFINITY; 3];
    let mut high = [f32::NEG_INFINITY; 3];
    let mut reach = 0.0_f32;
    for point in points {
        for axis in 0..3 {
            let value = point[axis];
            if !value.is_finite() {
                return None;
            }
            low[axis] = low[axis].min(value);
            high[axis] = high[axis].max(value);
            reach = reach.max(value.abs());
        }
    }
    let size = (0..3)
        .map(|axis| high[axis] - low[axis])
        .filter(|size| size.is_finite())
        .fold(0.0_f32, f32::max);
    let exponent = |value: f32, offset: i32, round: fn(f64) -> f64| {
        if value > 0.0 {
            (round(f64::from(value).log2()) as i32 + offset).clamp(-149, 127)
        } else {
            -149
        }
    };
    let finest = exponent(reach, -23, f64::ceil);
    let coarsest = exponent(size, -17, f64::floor).max(finest);
    Some((coarsest, finest))
}

/// Exponent of the grid (2^exponent units) for `path`, drawn at most at
/// `pixels_per_unit` output pixels per unit of its own coordinates: the
/// coarsest power of two finer than [`GRID_PIXELS`] there. A path whose
/// scale is unknown takes a grid of a millionth of its size. Every grid
/// coordinate stays an integer an `f64` holds exactly.
pub(crate) fn grid_exponent(path: &kurbo::BezPath, pixels_per_unit: Option<f64>) -> i32 {
    use kurbo::Shape;
    let bounds = path.bounding_box();
    let fine = match pixels_per_unit.filter(|scale| scale.is_finite() && *scale > 0.0) {
        Some(scale) => (GRID_PIXELS / scale).log2().floor(),
        None => {
            let size = bounds.width().max(bounds.height());
            if size > 0.0 && size.is_finite() {
                (size / f64::from(1 << 20)).log2().floor()
            } else {
                -30.0
            }
        }
    };
    let reach = [bounds.x0, bounds.y0, bounds.x1, bounds.y1]
        .into_iter()
        .map(f64::abs)
        .filter(|value| value.is_finite())
        .fold(0.0, f64::max);
    let coarsest_exact = if reach > 0.0 {
        reach.log2().ceil() - 52.0
    } else {
        -1074.0
    };
    (fine.max(coarsest_exact) as i32).clamp(-1000, 1000)
}

/// The segment that holds item `index`, given where each segment starts
/// (`start` reads one table's start): the last one starting at or before it.
/// Table indexes by the address of the `Arc` a frame stored. The weak
/// reference keeps the allocation, so no other value takes that address
/// while it is listed: a later frame with the same `Arc` finds its index
/// without encoding and hashing it again.
struct ByAddress<T> {
    entries: HashMap<usize, (std::sync::Weak<T>, u32)>,
    /// Entries left by the last sweep of those no frame holds any more.
    swept: usize,
}

impl<T> Default for ByAddress<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            swept: 0,
        }
    }
}

impl<T> ByAddress<T> {
    fn get(&self, value: &Arc<T>) -> Option<u32> {
        self.entries
            .get(&(Arc::as_ptr(value) as usize))
            .map(|(_, index)| *index)
    }

    fn insert(&mut self, value: &Arc<T>, index: u32) {
        // Values that change every frame leave an entry each: sweep the
        // dead ones whenever the entries double.
        if self.entries.len() >= 64 && self.entries.len() >= 2 * self.swept {
            self.entries.retain(|_, (weak, _)| weak.strong_count() > 0);
            self.swept = self.entries.len();
        }
        self.entries
            .insert(Arc::as_ptr(value) as usize, (Arc::downgrade(value), index));
    }
}

fn segment_of(starts: &[TableCounts], start: fn(&TableCounts) -> u32, index: u32) -> u32 {
    (starts.partition_point(|counts| start(counts) <= index) - 1) as u32
}

impl Interner for Tables {
    fn path(&mut self, path: &Arc<kurbo::BezPath>) -> u32 {
        let index = self.path_index(path);
        self.note(segment_of(&self.starts, |c| c.paths, index));
        index
    }

    fn image(&mut self, image: &peniko::ImageData) -> u32 {
        let index = self.image_index(image);
        self.note(segment_of(&self.starts, |c| c.images, index));
        index
    }
}

impl Tables {
    fn path_index(&mut self, path: &Arc<kurbo::BezPath>) -> u32 {
        let ptr = Arc::as_ptr(path) as usize;
        if let Some(index) = self.path_by_ptr.get(&ptr) {
            return *index;
        }
        let mut w = Writer::new();
        codec::write_path(&mut w, path);
        let hash = blake3::hash(&w.buf);
        let encoded = w.buf.len();
        let index = *self.path_by_hash.entry(hash).or_insert_with(|| {
            self.paths.push(Arc::clone(path));
            self.pending_bytes += encoded;
            (self.paths.len() - 1) as u32
        });
        // Keep the pointer key valid: the table holds the path alive.
        if Arc::ptr_eq(&self.paths[index as usize], path) {
            self.path_by_ptr.insert(ptr, index);
        }
        index
    }

    fn image_index(&mut self, image: &peniko::ImageData) -> u32 {
        let id = image.data.id();
        if let Some(index) = self.image_by_id.get(&id) {
            return *index;
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[image.format as u8, image.alpha_type as u8]);
        hasher.update(&image.width.to_le_bytes());
        hasher.update(&image.height.to_le_bytes());
        hasher.update(image.data.data());
        let hash = hasher.finalize();
        let index = *self.image_by_hash.entry(hash).or_insert_with(|| {
            self.images.push(image.clone());
            self.pending_bytes += image.data.data().len();
            (self.images.len() - 1) as u32
        });
        // The table keeps the blob alive, so its id stays unique.
        self.image_by_id.insert(id, index);
        index
    }

    /// Remember that the chunk being recorded refers to `segment`.
    fn note(&mut self, segment: u32) {
        self.used.insert(segment);
        if let Some(collecting) = &mut self.collecting {
            collecting.insert(segment);
        }
    }

    /// Note that the chunk being recorded refers to each of `segments`.
    pub(crate) fn note_all(&mut self, segments: &[u32]) {
        for segment in segments {
            self.note(*segment);
        }
    }

    /// The segments referred to since the last call, in increasing order.
    pub(crate) fn take_used(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.used).into_iter().collect()
    }

    /// Run `encode` and return what it returned with the segments it
    /// referred to, as a recipe or a Lottie frame stores them.
    pub(crate) fn collect<T>(&mut self, encode: impl FnOnce(&mut Self) -> T) -> (T, Vec<u32>) {
        let outer = self.collecting.replace(BTreeSet::new());
        let value = encode(self);
        let inner = std::mem::replace(&mut self.collecting, outer).unwrap_or_default();
        if let Some(outer) = &mut self.collecting {
            outer.extend(&inner);
        }
        (value, inner.into_iter().collect())
    }

    /// About how many bytes the segment being filled holds, encoded.
    pub(crate) fn pending_bytes(&self) -> usize {
        self.pending_bytes
    }

    fn counts(&self) -> TableCounts {
        TableCounts {
            paths: self.paths.len() as u32,
            images: self.images.len() as u32,
            recipes: self.recipes.len() as u32,
            strings: self.strings.len() as u32,
            transitions: self.transition_shaders.len() as u32,
            effects: self.effect_shaders.len() as u32,
            meshes: self.meshes.len() as u32,
            lines: self.lines.len() as u32,
            copies: self.copies.len() as u32,
        }
    }

    /// Store a mesh once and return its index. A frame that draws it needs
    /// its segment and that of its texture. `track` is the key of the
    /// drawable that draws it, whose earlier meshes it is predicted from.
    pub(crate) fn mesh(&mut self, mesh: &Arc<gaanim_scene::TriangleMeshData>, track: u32) -> u32 {
        if let Some(blend) = &mesh.blend
            && self.mesh_by_address.get(mesh).is_none()
        {
            let index = self.blend(blend, track);
            self.mesh_by_address.insert(mesh, index);
        }
        let index = match self.mesh_by_address.get(mesh) {
            Some(index) => index,
            None => {
                // Stored once by its whole encoding, which also puts its
                // texture in the images table.
                let (whole, deps) = self.collect(|tables| {
                    let mut w = Writer::new();
                    codec::write_mesh(&mut w, tables, mesh, codec::Storage::default());
                    w.into_bytes()
                });
                let hash = blake3::hash(&whole);
                let index = match self.mesh_by_hash.get(&hash) {
                    Some(index) => *index,
                    None => {
                        let index = self.meshes.len() as u32;
                        let start = self.starts.last().expect("a segment is open").meshes;
                        // Encoded again, its textures are those already in
                        // the table.
                        let surface = mesh.surface.as_deref();
                        let mut texture = Interned {
                            textures: surface
                                .into_iter()
                                .flat_map(|surface| {
                                    surface
                                        .texture
                                        .iter()
                                        .chain(surface.normal_map.iter().map(|map| &map.texture))
                                })
                                .map(|texture| {
                                    (texture.image.data.id(), self.image_index(&texture.image))
                                })
                                .collect(),
                        };
                        let chain = self
                            .mesh_chains
                            .entry((track, mesh_layout(mesh)))
                            .or_default();
                        let item = Item {
                            index,
                            start,
                            web: self.grid.is_some(),
                            points: &mesh.vertices,
                            normals: mesh.normals.as_deref(),
                        };
                        let bytes = predict(chain, item, whole, |storage| {
                            let mut w = Writer::new();
                            codec::write_mesh(&mut w, &mut texture, mesh, storage);
                            w.into_bytes()
                        });
                        self.pending_bytes += bytes.len();
                        self.meshes.push(bytes);
                        self.mesh_deps.push(deps.into());
                        self.mesh_by_hash.insert(hash, index);
                        index
                    }
                };
                self.mesh_by_address.insert(mesh, index);
                index
            }
        };
        self.note(segment_of(&self.starts, |c| c.meshes, index));
        let deps = self.mesh_deps[index as usize].clone();
        self.note_all(&deps);
        index
    }

    /// Store a mesh that blends two others, a frame of a morph, and return
    /// its index: the two meshes once, as any meshes of `track` (the second
    /// predicted from the first), and the blend as their indices and how
    /// far between them it lies. A frame that draws it needs their
    /// segments too.
    fn blend(&mut self, blend: &gaanim_scene::MeshBlend, track: u32) -> u32 {
        let ends = [self.mesh(&blend.from, track), self.mesh(&blend.to, track)];
        let mut w = Writer::new();
        codec::write_mesh_blend(&mut w, ends, blend.t);
        let bytes = w.into_bytes();
        let hash = blake3::hash(&bytes);
        if let Some(index) = self.mesh_by_hash.get(&hash) {
            return *index;
        }
        let mut deps: BTreeSet<u32> = BTreeSet::new();
        for end in ends {
            deps.insert(segment_of(&self.starts, |c| c.meshes, end));
            deps.extend(self.mesh_deps[end as usize].iter().copied());
        }
        let index = self.meshes.len() as u32;
        self.pending_bytes += bytes.len();
        self.meshes.push(bytes);
        self.mesh_deps.push(deps.into_iter().collect());
        self.mesh_by_hash.insert(hash, index);
        index
    }

    /// Store the copies of a mesh drawn at many points once and return their
    /// index, their positions predicted from the earlier copies of the
    /// drawable `track`, as a line list's points are.
    pub(crate) fn copies(&mut self, copies: &Arc<gaanim_scene::MeshCopies>, track: u32) -> u32 {
        let index = match self.copies_by_address.get(copies) {
            Some(index) => index,
            None => {
                let mut w = Writer::new();
                codec::write_copies(&mut w, copies, codec::Storage::default());
                let whole = w.into_bytes();
                let hash = blake3::hash(&whole);
                let index = match self.copies_by_hash.get(&hash) {
                    Some(index) => *index,
                    None => {
                        let index = self.copies.len() as u32;
                        let start = self.starts.last().expect("a segment is open").copies;
                        let chain = self
                            .copy_chains
                            .entry((track, copies_layout(copies)))
                            .or_default();
                        let item = Item {
                            index,
                            start,
                            web: self.grid.is_some(),
                            points: &copies.positions,
                            normals: None,
                        };
                        let bytes = predict(chain, item, whole, |storage| {
                            let mut w = Writer::new();
                            codec::write_copies(&mut w, copies, storage);
                            w.into_bytes()
                        });
                        self.pending_bytes += bytes.len();
                        self.copies.push(bytes);
                        self.copies_by_hash.insert(hash, index);
                        index
                    }
                };
                self.copies_by_address.insert(copies, index);
                index
            }
        };
        self.note(segment_of(&self.starts, |c| c.copies, index));
        index
    }

    /// Store a line list once and return its index, predicted from the
    /// earlier ones of the drawable `track`.
    pub(crate) fn line_list(&mut self, lines: &Arc<gaanim_scene::LineListData>, track: u32) -> u32 {
        let index = match self.lines_by_address.get(lines) {
            Some(index) => index,
            None => {
                let mut w = Writer::new();
                codec::write_lines(&mut w, lines, codec::Storage::default());
                let whole = w.into_bytes();
                let hash = blake3::hash(&whole);
                let index = match self.lines_by_hash.get(&hash) {
                    Some(index) => *index,
                    None => {
                        let index = self.lines.len() as u32;
                        let start = self.starts.last().expect("a segment is open").lines;
                        let chain = self
                            .line_chains
                            .entry((track, lines_layout(lines)))
                            .or_default();
                        let item = Item {
                            index,
                            start,
                            web: self.grid.is_some(),
                            points: &lines.points,
                            normals: None,
                        };
                        let bytes = predict(chain, item, whole, |storage| {
                            let mut w = Writer::new();
                            codec::write_lines(&mut w, lines, storage);
                            w.into_bytes()
                        });
                        self.pending_bytes += bytes.len();
                        self.lines.push(bytes);
                        self.lines_by_hash.insert(hash, index);
                        index
                    }
                };
                self.lines_by_address.insert(lines, index);
                index
            }
        };
        self.note(segment_of(&self.starts, |c| c.lines, index));
        index
    }

    /// Close the segment being filled: its encoding and how many items of
    /// each table it holds, or `None` when nothing was added to it.
    pub(crate) fn close_segment(&mut self) -> Option<(Vec<u8>, TableCounts)> {
        let start = *self.starts.last().expect("a segment is open");
        let end = self.counts();
        let counts = end.sub(start);
        if counts.is_empty() {
            return None;
        }
        let range = |from: u32, to: u32| from as usize..to as usize;
        let mut w = Writer::new();
        let paths = &self.paths[range(start.paths, end.paths)];
        match &self.grid {
            None => {
                w.u8(PATHS_EXACT);
                w.len(paths.len());
                for path in paths {
                    codec::write_path(&mut w, path);
                }
            }
            Some(scales) => {
                // Curves compress best point after point, meshes (whose
                // vertices repeat) with every point where it is: keep the
                // smaller once compressed.
                let encode = |relative: bool| {
                    let mut w = Writer::new();
                    w.u8(if relative {
                        PATHS_GRID
                    } else {
                        PATHS_GRID_ABSOLUTE
                    });
                    w.len(paths.len());
                    for path in paths {
                        let scale = scales.get(&(Arc::as_ptr(path) as usize)).copied();
                        codec::write_grid_path(&mut w, path, grid_exponent(path, scale), relative);
                    }
                    w.into_bytes()
                };
                let (relative, absolute) = (encode(true), encode(false));
                let size = |bytes: &[u8]| crate::compress(bytes).map_or(usize::MAX, |c| c.len());
                let smaller = if size(&absolute) < size(&relative) {
                    absolute
                } else {
                    relative
                };
                w.buf.extend_from_slice(&smaller);
            }
        }
        w.len(counts.images as usize);
        for image in &self.images[range(start.images, end.images)] {
            codec::write_image_data(&mut w, image);
        }
        w.len(counts.strings as usize);
        for string in &self.strings[range(start.strings, end.strings)] {
            w.str(string);
        }
        w.len(counts.transitions as usize);
        for shader in &self.transition_shaders[range(start.transitions, end.transitions)] {
            write_transition_shader(&mut w, shader);
        }
        w.len(counts.effects as usize);
        for shader in &self.effect_shaders[range(start.effects, end.effects)] {
            write_post_shader(&mut w, shader);
        }
        w.len(counts.recipes as usize);
        for recipe in &self.recipes[range(start.recipes, end.recipes)] {
            w.bytes(recipe);
        }
        w.len(counts.meshes as usize);
        for mesh in &self.meshes[range(start.meshes, end.meshes)] {
            w.buf.extend_from_slice(mesh);
        }
        w.len(counts.lines as usize);
        for lines in &self.lines[range(start.lines, end.lines)] {
            w.buf.extend_from_slice(lines);
        }
        w.len(counts.copies as usize);
        for copies in &self.copies[range(start.copies, end.copies)] {
            w.buf.extend_from_slice(copies);
        }
        self.starts.push(end);
        self.pending_bytes = 0;
        // Items predict only from their own segment, which loads whole.
        self.mesh_chains.clear();
        self.line_chains.clear();
        self.copy_chains.clear();
        Some((w.into_bytes(), counts))
    }

    pub fn transition_shader(&mut self, shader: &Arc<TransitionShader>) -> u32 {
        let index = self
            .transition_shaders
            .iter()
            .position(|known| Arc::ptr_eq(known, shader) || **known == **shader)
            .unwrap_or_else(|| {
                self.transition_shaders.push(Arc::clone(shader));
                self.transition_shaders.len() - 1
            }) as u32;
        self.note(segment_of(&self.starts, |c| c.transitions, index));
        index
    }

    pub fn effect_shader(&mut self, shader: &PostProcessShader) -> u32 {
        let index = self
            .effect_shaders
            .iter()
            .position(|known| known == shader)
            .unwrap_or_else(|| {
                self.effect_shaders.push(shader.clone());
                self.effect_shaders.len() - 1
            }) as u32;
        self.note(segment_of(&self.starts, |c| c.effects, index));
        index
    }

    pub fn string(&mut self, value: &Arc<str>) -> u32 {
        let index = match self.string_index.get(value) {
            Some(index) => *index,
            None => {
                self.strings.push(Arc::clone(value));
                self.pending_bytes += value.len();
                let index = (self.strings.len() - 1) as u32;
                self.string_index.insert(Arc::clone(value), index);
                index
            }
        };
        self.note(segment_of(&self.starts, |c| c.strings, index));
        index
    }

    pub fn recipe(&mut self, recipe: &Arc<FragmentRecipe>) -> u32 {
        let ptr = Arc::as_ptr(recipe) as usize;
        if let Some((_, index)) = self.recipe_by_ptr.get(&ptr) {
            let index = *index;
            let deps = self.recipe_deps[index as usize].clone();
            self.note_all(&deps);
            return index;
        }
        let (bytes, mut deps) = self.collect(|tables| {
            let mut w = Writer::new();
            write_recipe(&mut w, tables, recipe);
            w.into_bytes()
        });
        let hash = blake3::hash(&bytes);
        let index = match self.recipe_by_hash.get(&hash) {
            Some(index) => {
                let index = *index;
                let deps = self.recipe_deps[index as usize].clone();
                self.note_all(&deps);
                index
            }
            None => {
                let index = self.recipes.len() as u32;
                self.pending_bytes += bytes.len();
                self.recipes.push(bytes);
                let own = segment_of(&self.starts, |c| c.recipes, index);
                if let Err(position) = deps.binary_search(&own) {
                    deps.insert(position, own);
                }
                self.note(own);
                self.recipe_deps.push(deps.into());
                self.recipe_by_hash.insert(hash, index);
                index
            }
        };
        // Hold the recipe so its address is not reused by another one.
        self.recipe_by_ptr.insert(ptr, (Arc::clone(recipe), index));
        index
    }

    /// Drop the pointer caches of recipes no frame holds any more; their
    /// encodings stay deduplicated by content.
    pub fn release_unused_recipes(&mut self) {
        self.recipe_by_ptr
            .retain(|_, (recipe, _)| Arc::strong_count(recipe) > 1);
    }
}

/// Paths of a segment stored exactly, every coordinate's bits.
pub(crate) const PATHS_EXACT: u8 = 0;
/// Paths of a segment stored on a grid each, point after point
/// ([`codec::write_grid_path`]), in a bundle packed for the web.
pub(crate) const PATHS_GRID: u8 = 1;
/// Paths of a segment stored on a grid each, every point where it is.
pub(crate) const PATHS_GRID_ABSOLUTE: u8 = 2;

/// The items of one decoded segment.
#[derive(Default)]
struct Segment {
    paths: Vec<Arc<kurbo::BezPath>>,
    images: Vec<peniko::ImageData>,
    strings: Vec<Arc<str>>,
    /// Shared by every frame of one transition, so its shader is built once.
    transition_shaders: Vec<Arc<TransitionShader>>,
    effect_shaders: Vec<PostProcessShader>,
    recipes: Vec<SegmentRecipe>,
    meshes: Vec<Arc<gaanim_scene::TriangleMeshData>>,
    lines: Vec<Arc<gaanim_scene::LineListData>>,
    copies: Vec<Arc<gaanim_scene::MeshCopies>>,
}

/// A recipe of a loaded segment, decoded the first time a frame draws it.
/// A segment holds the recipes of every chunk that met them first, and a
/// chunk lists only the segments of the recipes it draws: decoding the
/// others as the segment loads would need segments the chunk does not list.
struct SegmentRecipe {
    bytes: Vec<u8>,
    decoded: std::sync::OnceLock<Arc<FragmentRecipe>>,
}

impl SegmentRecipe {
    fn new(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.to_vec(),
            decoded: std::sync::OnceLock::new(),
        }
    }
}

/// Decoded tables of an open bundle, segment by segment as frames need
/// them (see [`Tables`]).
pub struct DecodedTables {
    /// The bundle's format, which some items' encoding depends on.
    version: u32,
    /// Where each segment starts, then the totals.
    starts: Vec<TableCounts>,
    segments: Vec<Option<Segment>>,
    /// Keys of the drawables the post-process leaves out; empty in a bundle
    /// recorded without any.
    pub post_exempt: HashSet<u32>,
}

/// Item `index` of the table `items` reads from a segment, found through
/// where each segment starts in that table (`start`).
fn lookup<'a, T>(
    tables: &'a DecodedTables,
    start: fn(&TableCounts) -> u32,
    items: fn(&Segment) -> &Vec<T>,
    index: u32,
    what: &str,
) -> Result<&'a T> {
    if index >= start(tables.starts.last().expect("totals")) {
        return Err(corrupt(&format!("{what} index out of range")));
    }
    let segment = segment_of(&tables.starts, start, index) as usize;
    let loaded = tables.segments[segment].as_ref().ok_or_else(|| {
        corrupt(&format!(
            "a frame refers to a {what} of a segment it does not list"
        ))
    })?;
    items(loaded)
        .get((index - start(&tables.starts[segment])) as usize)
        .ok_or_else(|| corrupt(&format!("{what} index out of range")))
}

impl Resolver for DecodedTables {
    fn path(&self, index: u32) -> Result<Arc<kurbo::BezPath>> {
        lookup(self, |c| c.paths, |s| &s.paths, index, "path").cloned()
    }

    fn image(&self, index: u32) -> Result<peniko::ImageData> {
        lookup(self, |c| c.images, |s| &s.images, index, "image").cloned()
    }
}

impl DecodedTables {
    /// Tables of `counts.len()` segments, holding `counts[i]` items each,
    /// none decoded yet.
    pub fn new(counts: &[TableCounts], version: u32) -> Self {
        let mut starts = vec![TableCounts::default()];
        for segment in counts {
            starts.push(starts.last().expect("a start").add(*segment));
        }
        // The last start is the totals: no segment starts there.
        Self {
            version,
            starts,
            segments: counts.iter().map(|_| None).collect(),
            post_exempt: HashSet::new(),
        }
    }

    pub fn is_loaded(&self, segment: usize) -> bool {
        self.segments.get(segment).is_some_and(Option::is_some)
    }

    /// How many items of each table there are, over every segment.
    pub fn totals(&self) -> TableCounts {
        *self.starts.last().expect("totals")
    }

    /// Decode segment `segment` from `bytes`. Its recipes decode the first
    /// time a frame draws one, so the segments a recipe refers to must be
    /// loaded by then: the chunk drawing it lists them.
    pub fn load(&mut self, segment: usize, bytes: &[u8]) -> Result<()> {
        let expected = self.starts[segment + 1].sub(self.starts[segment]);
        let mut r = Reader::new(bytes);
        let read_path = match r.u8()? {
            PATHS_EXACT => codec::read_path,
            PATHS_GRID => codec::read_grid_path,
            PATHS_GRID_ABSOLUTE => codec::read_absolute_grid_path,
            other => {
                return Err(BundleError::Unsupported(format!(
                    "a segment stores its paths in an encoding this Gaanim does not read ({other})"
                )));
            }
        };
        let mut decoded = Segment::default();
        let count = |r: &mut Reader<'_>, expected: u32, what: &str| -> Result<usize> {
            let count = r.len()?;
            if count != expected as usize {
                return Err(corrupt(&format!(
                    "a segment holds an unexpected number of {what}"
                )));
            }
            Ok(count)
        };
        for _ in 0..count(&mut r, expected.paths, "paths")? {
            decoded.paths.push(Arc::new(read_path(&mut r)?));
        }
        for _ in 0..count(&mut r, expected.images, "images")? {
            decoded.images.push(codec::read_image_data(&mut r)?);
        }
        for _ in 0..count(&mut r, expected.strings, "strings")? {
            decoded.strings.push(Arc::from(r.str()?));
        }
        for _ in 0..count(&mut r, expected.transitions, "transitions")? {
            decoded
                .transition_shaders
                .push(Arc::new(read_transition_shader(&mut r)?));
        }
        for _ in 0..count(&mut r, expected.effects, "effects")? {
            decoded.effect_shaders.push(read_post_shader(&mut r)?);
        }
        let recipes = count(&mut r, expected.recipes, "recipes")?;
        decoded.recipes.reserve(recipes.min(1 << 20));
        for _ in 0..recipes {
            decoded.recipes.push(SegmentRecipe::new(r.bytes()?));
        }
        // A mesh's texture is among this segment's images or those of an
        // earlier segment, which a chunk loads first.
        let first_image = self.starts[segment].images;
        let image = |index: u32| -> Result<peniko::ImageData> {
            match index.checked_sub(first_image) {
                Some(local) if (local as usize) < decoded.images.len() => {
                    Ok(decoded.images[local as usize].clone())
                }
                _ => lookup(self, |c| c.images, |s| &s.images, index, "image").cloned(),
            }
        };
        // A blended mesh's ends are among this segment's meshes before it
        // or those of an earlier segment.
        let first = self.starts[segment].meshes;
        let table = |index: u32| -> Result<Arc<gaanim_scene::TriangleMeshData>> {
            lookup(self, |c| c.meshes, |s| &s.meshes, index, "mesh").cloned()
        };
        let mut meshes = Vec::new();
        for _ in 0..count(&mut r, expected.meshes, "meshes")? {
            let table = codec::MeshTable {
                earlier: &meshes,
                first,
                table: &table,
            };
            let mesh = codec::read_mesh(&mut r, self.version, &image, &table)?;
            meshes.push(Arc::new(mesh));
        }
        decoded.meshes = meshes;
        for _ in 0..count(&mut r, expected.lines, "line lists")? {
            let lines = codec::read_lines(&mut r, self.version, &decoded.lines)?;
            decoded.lines.push(Arc::new(lines));
        }
        // Format 9 added the copies of meshes drawn at many points.
        if self.version >= 9 {
            for _ in 0..count(&mut r, expected.copies, "copies")? {
                let copies = codec::read_copies(&mut r, &decoded.copies)?;
                decoded.copies.push(Arc::new(copies));
            }
        }
        if !r.is_empty() {
            return Err(corrupt("a segment has trailing data"));
        }
        self.segments[segment] = Some(decoded);
        Ok(())
    }

    /// Tables of a format 5 bundle, which stores each table whole in an
    /// entry of its own: one segment that every chunk needs.
    pub fn from_format_5(
        paths: &[u8],
        images: &[u8],
        strings: &[u8],
        transitions: &[u8],
        effects: &[u8],
        recipes: &[u8],
    ) -> Result<Self> {
        let mut decoded = Segment::default();
        let mut r = Reader::new(paths);
        for _ in 0..r.len()? {
            decoded.paths.push(Arc::new(codec::read_path(&mut r)?));
        }
        let mut r = Reader::new(images);
        for _ in 0..r.len()? {
            decoded.images.push(codec::read_image_data(&mut r)?);
        }
        let mut r = Reader::new(strings);
        for _ in 0..r.len()? {
            decoded.strings.push(Arc::from(r.str()?));
        }
        let mut r = Reader::new(transitions);
        for _ in 0..r.len()? {
            decoded
                .transition_shaders
                .push(Arc::new(read_transition_shader(&mut r)?));
        }
        let mut r = Reader::new(effects);
        for _ in 0..r.len()? {
            decoded.effect_shaders.push(read_post_shader(&mut r)?);
        }
        let mut r = Reader::new(recipes);
        for _ in 0..r.len()? {
            decoded.recipes.push(SegmentRecipe::new(r.bytes()?));
        }
        let counts = TableCounts {
            paths: decoded.paths.len() as u32,
            images: decoded.images.len() as u32,
            recipes: decoded.recipes.len() as u32,
            strings: decoded.strings.len() as u32,
            transitions: decoded.transition_shaders.len() as u32,
            effects: decoded.effect_shaders.len() as u32,
            meshes: 0,
            lines: 0,
            copies: 0,
        };
        let mut tables = Self::new(&[counts], 5);
        tables.segments[0] = Some(decoded);
        Ok(tables)
    }

    fn recipe(&self, index: u32) -> Result<Arc<FragmentRecipe>> {
        let recipe = lookup(self, |c| c.recipes, |s| &s.recipes, index, "recipe")?;
        if let Some(decoded) = recipe.decoded.get() {
            return Ok(Arc::clone(decoded));
        }
        // Recipes refer to their own segment's paths and images too.
        let decoded = Arc::new(read_recipe(&mut Reader::new(&recipe.bytes), self)?);
        Ok(Arc::clone(recipe.decoded.get_or_init(|| decoded)))
    }

    fn effect_shader(&self, index: u32) -> Result<PostProcessShader> {
        lookup(
            self,
            |c| c.effects,
            |s| &s.effect_shaders,
            index,
            "effect shader",
        )
        .cloned()
    }

    fn transition_shader(&self, index: u32) -> Result<Arc<TransitionShader>> {
        lookup(
            self,
            |c| c.transitions,
            |s| &s.transition_shaders,
            index,
            "transition shader",
        )
        .cloned()
    }

    fn string(&self, index: u32) -> Result<Arc<str>> {
        lookup(self, |c| c.strings, |s| &s.strings, index, "string").cloned()
    }

    fn mesh(&self, index: u32) -> Result<Arc<gaanim_scene::TriangleMeshData>> {
        lookup(self, |c| c.meshes, |s| &s.meshes, index, "mesh").cloned()
    }

    fn line_list(&self, index: u32) -> Result<Arc<gaanim_scene::LineListData>> {
        lookup(self, |c| c.lines, |s| &s.lines, index, "line list").cloned()
    }
}

impl DecodedTables {
    fn copies(&self, index: u32) -> Result<Arc<gaanim_scene::MeshCopies>> {
        lookup(self, |c| c.copies, |s| &s.copies, index, "copies").cloned()
    }
}

// ---------------------------------------------------------------------------
// Native 3D content
// ---------------------------------------------------------------------------

fn write_side(w: &mut Writer, side: TransitionSide) {
    w.u8(match side {
        TransitionSide::None => 0,
        TransitionSide::Outgoing => 1,
        TransitionSide::Incoming => 2,
    });
}

fn read_side(r: &mut Reader<'_>) -> Result<TransitionSide> {
    match r.u8()? {
        0 => Ok(TransitionSide::None),
        1 => Ok(TransitionSide::Outgoing),
        2 => Ok(TransitionSide::Incoming),
        _ => Err(corrupt("invalid transition side")),
    }
}

/// Encode a frame's 3D content, its meshes and line lists in the tables.
fn write_three_d(
    w: &mut Writer,
    tables: &mut Tables,
    keys: &mut EntityKeys,
    three_d: &CapturedThreeD,
    views: &[(u32, &FrozenThreeD)],
) {
    w.bool(three_d.lighting.enabled);
    w.f32(three_d.lighting.intensity);
    w.bool(three_d.lighting.shadows);
    w.bool(three_d.lighting.z_up);
    w.len(three_d.lights.len());
    for light in &three_d.lights {
        for value in light.position.to_array() {
            w.f64(value);
        }
        w.bool(light.directional);
        for value in light.color {
            w.f32(value);
        }
    }
    w.rect(three_d.opacity_fallback);
    // Formerly the count of elements a CPU projection drew; the GPU draws
    // the content now, beneath the elements, and readers ignore it.
    w.var(0);
    w.len(three_d.sources.len());
    for source in &three_d.sources {
        let key = keys.key(source.entity);
        w.var(u64::from(key));
        for value in source.transform.to_cols_array() {
            w.f64(value);
        }
        w.f32(source.opacity);
        w.option(source.mesh.as_ref(), |w, mesh| {
            w.var(u64::from(tables.mesh(mesh, key)))
        });
        w.option(source.lines.as_ref(), |w, lines| {
            w.var(u64::from(tables.line_list(lines, key)))
        });
        w.option(source.material.as_ref(), codec::write_material);
        let composition = &source.composition;
        w.bool(composition.in_views);
        w.bool(composition.post_exempt);
        w.option(composition.layer.as_ref(), |w, layer| {
            w.var(u64::from(tables.string(layer)))
        });
        write_side(w, composition.transition_side);
        let lineage: Vec<u32> = composition
            .lineage
            .iter()
            .map(|entity| keys.key(*entity))
            .collect();
        write_keys(w, &lineage);
    }
    // After the sources, by source: the widths of lines that set one, then
    // the parts of lines a reveal shows. Frames of bundles recorded before
    // end before either, and readers that predate them stop there.
    let widths: Vec<(usize, f32)> = three_d
        .sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| source.line_width.map(|width| (index, width)))
        .collect();
    let ranges: Vec<(usize, [f32; 2])> = three_d
        .sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| source.line_range.map(|range| (index, range)))
        .collect();
    // Then the outlines of materials, which the material's own encoding
    // predates.
    let outlines: Vec<(usize, gaanim_scene::Material3D)> = three_d
        .sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| {
            source
                .material
                .filter(|material| material.outlined())
                .map(|material| (index, material))
        })
        .collect();
    // Then the outlines filled in the sources' planes: each outline once in
    // the paths table, its placement and its color, from the colors the
    // source uses.
    let fills: Vec<(usize, &gaanim_scene::PlaneFillData)> = three_d
        .sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| source.fill.as_deref().map(|fill| (index, fill)))
        .collect();
    // Last, the palettes of skinned meshes (format 7): by source, its
    // joints' matrices.
    let palettes: Vec<(usize, &Arc<[[f32; 16]]>)> = three_d
        .sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| source.joints.as_ref().map(|joints| (index, joints)))
        .collect();
    // Then the glows of 3D drawables (format 7): by source, its glow.
    let glows: Vec<(usize, &gaanim_renderer::effects::Glow)> = three_d
        .sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| source.glow.as_ref().map(|glow| (index, glow)))
        .collect();
    // After the views, the copies of meshes drawn at many points (format
    // 9): by source, its copies and how far they have grown in.
    let copies: Vec<(usize, &gaanim_scene::MeshInstances)> = three_d
        .sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| source.instances.as_ref().map(|copies| (index, copies)))
        .collect();
    // After the copies, the images outlines are filled with instead of
    // their color (format 10): by source, each glyph's index, its brush and
    // the map from its outline's units to the image's pixels.
    let images: Vec<(usize, Vec<(usize, &gaanim_scene::PlaneImage)>)> = fills
        .iter()
        .filter_map(|&(index, fill)| {
            let glyphs: Vec<_> = fill
                .glyphs
                .iter()
                .enumerate()
                .filter_map(|(glyph, filled)| filled.image.as_ref().map(|image| (glyph, image)))
                .collect();
            (!glyphs.is_empty()).then_some((index, glyphs))
        })
        .collect();
    // Last, the drop shadows of 3D drawables (format 10): by source, its
    // color, offset and blur.
    let shadows: Vec<(usize, &gaanim_renderer::effects::DropShadow)> = three_d
        .sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| source.shadow.as_ref().map(|shadow| (index, shadow)))
        .collect();
    // And their blurs: by source, its sigma.
    let blurs: Vec<(usize, f64)> = three_d
        .sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| source.blur.map(|blur| (index, blur.sigma)))
        .collect();
    // Then the weights of morphed meshes' targets (format 11): by source,
    // its weights.
    let morphs: Vec<(usize, &Arc<[f32]>)> = three_d
        .sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| source.morph.as_ref().map(|weights| (index, weights)))
        .collect();
    // Then how translucent content blends (format 12): the surfaces each
    // pixel blends in depth order, 0 when drawables blend as layers.
    let peeled = three_d.transparency.layers;
    // Then the cones of spot lights (format 12): by light, its direction and
    // the cosines of its cone's edges.
    let spots: Vec<(usize, &gaanim_renderer::three_d::Spot)> = three_d
        .lights
        .iter()
        .enumerate()
        .filter_map(|(index, light)| light.spot.as_ref().map(|spot| (index, spot)))
        .collect();
    // Last, the depth of field that blurs the content (format 12): its
    // focus, aperture and largest blur, when it blurs.
    let depth_of_field = three_d.depth_of_field;
    // Then the volumes (format 12): by source, its values and the colors of
    // its values in the images table, its grid, its box and its density.
    let volumes: Vec<(usize, &gaanim_scene::VolumeData)> = three_d
        .sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| source.volume.as_deref().map(|volume| (index, volume)))
        .collect();
    // Each section is written when it or a later one holds something.
    let after_depth_of_field = !volumes.is_empty();
    let after_spots = depth_of_field.aperture > 0.0 || after_depth_of_field;
    let after_transparency = !spots.is_empty() || after_spots;
    let after_morphs = peeled > 0 || after_transparency;
    let after_blurs = !blurs.is_empty() || !morphs.is_empty() || after_morphs;
    let after_images = !images.is_empty() || !shadows.is_empty() || after_blurs;
    let later = !palettes.is_empty()
        || !glows.is_empty()
        || !views.is_empty()
        || !copies.is_empty()
        || after_images;
    if !widths.is_empty()
        || !ranges.is_empty()
        || !outlines.is_empty()
        || !fills.is_empty()
        || later
    {
        w.len(widths.len());
        for (index, width) in widths {
            w.len(index);
            w.f32(width);
        }
    }
    if !ranges.is_empty() || !outlines.is_empty() || !fills.is_empty() || later {
        w.len(ranges.len());
        for (index, [from, to]) in ranges {
            w.len(index);
            w.f32(from);
            w.f32(to);
        }
    }
    if !outlines.is_empty() || !fills.is_empty() || later {
        w.len(outlines.len());
        for (index, material) in outlines {
            w.len(index);
            codec::write_color(w, material.outline);
            w.f32(material.outline_width);
        }
    }
    if !fills.is_empty() || later {
        w.len(fills.len());
        for &(index, fill) in &fills {
            w.len(index);
            let mut colors: Vec<peniko::Color> = Vec::new();
            let mut color_of = Vec::with_capacity(fill.glyphs.len());
            for glyph in &fill.glyphs {
                let known = colors
                    .iter()
                    .position(|color| color.components == glyph.color.components);
                color_of.push(known.unwrap_or_else(|| {
                    colors.push(glyph.color);
                    colors.len() - 1
                }));
            }
            w.len(colors.len());
            for color in colors {
                codec::write_color(w, color);
            }
            w.len(fill.glyphs.len());
            for (glyph, color) in fill.glyphs.iter().zip(color_of) {
                w.var(u64::from(Interner::path(tables, &glyph.outline)));
                for value in glyph.placement.as_coeffs() {
                    w.f32(value as f32);
                }
                w.len(color);
            }
        }
    }
    // Then the gradients outlines are filled with instead of their color:
    // by source, each glyph's index, its gradient and the map to it. Readers
    // that predate them stop before, and draw the gradients' first colors.
    let gradients: Vec<(usize, Vec<(usize, &(Arc<peniko::Gradient>, kurbo::Affine))>)> = fills
        .iter()
        .filter_map(|&(index, fill)| {
            let glyphs: Vec<_> = fill
                .glyphs
                .iter()
                .enumerate()
                .filter_map(|(glyph, filled)| filled.gradient.as_ref().map(|paint| (glyph, paint)))
                .collect();
            (!glyphs.is_empty()).then_some((index, glyphs))
        })
        .collect();
    // Then the contours outlines are clipped to: by source, each glyph's
    // index, its contour in the paths table, the offset to it and its side.
    let clips: Vec<(usize, Vec<(usize, &gaanim_scene::PlaneClip)>)> = fills
        .iter()
        .filter_map(|&(index, fill)| {
            let glyphs: Vec<_> = fill
                .glyphs
                .iter()
                .enumerate()
                .filter_map(|(glyph, filled)| filled.clip.as_ref().map(|clip| (glyph, clip)))
                .collect();
            (!glyphs.is_empty()).then_some((index, glyphs))
        })
        .collect();
    if !gradients.is_empty() || !clips.is_empty() || later {
        w.len(gradients.len());
        for (index, glyphs) in gradients {
            w.len(index);
            w.len(glyphs.len());
            for (glyph, (gradient, to_gradient)) in glyphs {
                w.len(glyph);
                codec::write_gradient(w, gradient);
                for value in to_gradient.as_coeffs() {
                    w.f32(value as f32);
                }
            }
        }
    }
    if !clips.is_empty() || later {
        w.len(clips.len());
        for (index, glyphs) in clips {
            w.len(index);
            w.len(glyphs.len());
            for (glyph, clip) in glyphs {
                w.len(glyph);
                w.var(u64::from(Interner::path(tables, &clip.outline)));
                w.f32(clip.offset.x as f32);
                w.f32(clip.offset.y as f32);
                w.bool(clip.outside);
            }
        }
    }
    if later {
        w.len(palettes.len());
        for (index, joints) in palettes {
            w.len(index);
            w.len(joints.len());
            for matrix in joints.iter() {
                for value in matrix {
                    w.f32(*value);
                }
            }
        }
    }
    if !glows.is_empty() || !views.is_empty() || !copies.is_empty() || after_images {
        w.len(glows.len());
        for (index, glow) in glows {
            w.len(index);
            codec::write_color(w, glow.color);
            w.f64(glow.radius);
            w.f32(glow.intensity);
        }
    }
    // Last, the 3D content of camera views that draw their own (format 7):
    // what snapshots froze and what views with a 3D camera of their own
    // draw this frame. By screen, the camera it was drawn through, the
    // region and detail of its image, and its content, encoded as a frame's.
    if !views.is_empty() || !copies.is_empty() || after_images {
        w.len(views.len());
        for &(screen, frozen) in views {
            w.var(u64::from(screen));
            write_camera(w, &frozen.camera);
            w.rect(frozen.region);
            w.f64(frozen.resolution);
            let mut content = Writer::new();
            write_three_d(&mut content, tables, keys, &frozen.three_d, &[]);
            w.bytes(&content.into_bytes());
        }
    }
    if !copies.is_empty() || after_images {
        w.len(copies.len());
        for (index, instances) in copies {
            let key = keys.key(three_d.sources[index].entity);
            w.len(index);
            w.var(u64::from(tables.copies(&instances.copies, key)));
            w.f32(instances.reveal);
        }
    }
    if after_images {
        w.len(images.len());
        for (index, glyphs) in images {
            w.len(index);
            w.len(glyphs.len());
            for (glyph, image) in glyphs {
                w.len(glyph);
                codec::write_image_brush(w, tables, &image.brush);
                for value in image.to_pixels.as_coeffs() {
                    w.f64(value);
                }
            }
        }
    }
    if !shadows.is_empty() || after_blurs {
        w.len(shadows.len());
        for (index, shadow) in shadows {
            w.len(index);
            codec::write_color(w, shadow.color);
            w.f64(shadow.offset.x);
            w.f64(shadow.offset.y);
            w.f64(shadow.blur_radius);
        }
    }
    if after_blurs {
        w.len(blurs.len());
        for (index, sigma) in blurs {
            w.len(index);
            w.f64(sigma);
        }
    }
    if !morphs.is_empty() || after_morphs {
        w.len(morphs.len());
        for (index, weights) in morphs {
            w.len(index);
            w.len(weights.len());
            for weight in weights.iter() {
                w.f32(*weight);
            }
        }
    }
    if after_morphs {
        w.u8(peeled);
    }
    if after_transparency {
        w.len(spots.len());
        for (index, spot) in spots {
            w.len(index);
            for value in spot.direction.to_array() {
                w.f64(value);
            }
            w.f32(spot.cos_inner);
            w.f32(spot.cos_outer);
        }
    }
    if after_spots {
        w.f64(depth_of_field.focus);
        w.f64(depth_of_field.aperture);
        w.f64(depth_of_field.max_blur);
    }
    if after_depth_of_field {
        w.len(volumes.len());
        for (index, volume) in volumes {
            w.len(index);
            w.var(u64::from(tables.image(&volume.values)));
            for side in volume.size {
                w.var(u64::from(side));
            }
            for value in volume.min.into_iter().chain(volume.max) {
                w.f32(value);
            }
            w.var(u64::from(tables.image(&volume.transfer)));
            w.f32(volume.density);
        }
    }
}

/// The 3D content of camera views a frame's 3D block holds, by the key of
/// their screen.
type ViewsThreeD = Vec<(u32, Arc<FrozenThreeD>)>;

/// Decode what [`write_three_d`] wrote, and the 3D content of the camera
/// views that draw their own.
fn read_three_d(
    r: &mut Reader<'_>,
    tables: &DecodedTables,
) -> Result<(CapturedThreeD, ViewsThreeD)> {
    use gaanim_core::glam::{DMat4, DVec3};
    let lighting = gaanim_scene::Lighting3D {
        enabled: r.bool()?,
        intensity: r.f32()?,
        shadows: r.bool()?,
        // The world's up axis (version 15 on).
        z_up: tables.version >= 15 && r.bool()?,
    };
    let count = r.len()?;
    let mut lights = Vec::with_capacity(count.min(256));
    for _ in 0..count {
        lights.push(gaanim_renderer::three_d::Light {
            position: DVec3::new(r.f64()?, r.f64()?, r.f64()?),
            directional: r.bool()?,
            color: [r.f32()?, r.f32()?, r.f32()?],
            spot: None,
        });
    }
    let opacity_fallback = r.rect()?;
    // A count of elements a CPU projection drew, which the GPU replaced.
    usize::try_from(r.var()?).map_err(|_| corrupt("3D element count"))?;
    let count = r.len()?;
    let mut sources = Vec::with_capacity(count.min(1 << 16));
    for _ in 0..count {
        let entity = key_entity(r.u32()?)?;
        let mut matrix = [0.0; 16];
        for value in &mut matrix {
            *value = r.f64()?;
        }
        let opacity = r.f32()?;
        let mesh = r.option(|r| tables.mesh(r.u32()?))?;
        let lines = r.option(|r| tables.line_list(r.u32()?))?;
        let material = r.option(codec::read_material)?;
        let in_views = r.bool()?;
        let post_exempt = r.bool()?;
        let layer = r.option(|r| tables.string(r.u32()?))?;
        let transition_side = read_side(r)?;
        let lineage = read_keys(r)?
            .into_iter()
            .map(key_entity)
            .collect::<Result<Vec<_>>>()?;
        sources.push(CapturedThreeDSource {
            entity,
            transform: DMat4::from_cols_array(&matrix),
            opacity,
            mesh,
            lines,
            material,
            line_width: None,
            line_range: None,
            fill: None,
            joints: None,
            morph: None,
            glow: None,
            shadow: None,
            blur: None,
            instances: None,
            volume: None,
            composition: ThreeDComposition {
                in_views,
                post_exempt,
                layer,
                transition_side,
                lineage,
            },
        });
    }
    if !r.is_empty() {
        for _ in 0..r.len()? {
            let index = r.len()?;
            let width = r.f32()?;
            sources
                .get_mut(index)
                .ok_or_else(|| corrupt("a line width of no 3D source"))?
                .line_width = Some(width);
        }
    }
    if !r.is_empty() {
        for _ in 0..r.len()? {
            let index = r.len()?;
            let range = [r.f32()?, r.f32()?];
            sources
                .get_mut(index)
                .ok_or_else(|| corrupt("a revealed part of no 3D source"))?
                .line_range = Some(range);
        }
    }
    if !r.is_empty() {
        for _ in 0..r.len()? {
            let index = r.len()?;
            let outline = codec::read_color(r)?;
            let width = r.f32()?;
            if let Some(material) = sources
                .get_mut(index)
                .ok_or_else(|| corrupt("an outline of no 3D source"))?
                .material
                .as_mut()
            {
                material.outline = outline;
                material.outline_width = width;
            }
        }
    }
    let mut fills = Vec::new();
    if !r.is_empty() {
        for _ in 0..r.len()? {
            let index = r.len()?;
            let colors = (0..r.len()?)
                .map(|_| codec::read_color(r))
                .collect::<Result<Vec<_>>>()?;
            let count = r.len()?;
            let mut glyphs = Vec::with_capacity(count.min(1 << 16));
            for _ in 0..count {
                let outline = Resolver::path(tables, r.u32()?)?;
                let mut placement = [0.0; 6];
                for value in &mut placement {
                    *value = f64::from(r.f32()?);
                }
                let color = *colors
                    .get(r.len()?)
                    .ok_or_else(|| corrupt("a filled outline of no color"))?;
                glyphs.push(gaanim_scene::PlaneGlyph {
                    outline,
                    placement: kurbo::Affine::new(placement),
                    color,
                    gradient: None,
                    clip: None,
                    image: None,
                });
            }
            if index >= sources.len() {
                return Err(corrupt("filled outlines of no 3D source"));
            }
            fills.push((index, glyphs));
        }
    }
    if !r.is_empty() {
        for _ in 0..r.len()? {
            let index = r.len()?;
            let (_, glyphs) = fills
                .iter_mut()
                .find(|(filled, _)| *filled == index)
                .ok_or_else(|| corrupt("gradients of no filled outlines"))?;
            for _ in 0..r.len()? {
                let glyph = r.len()?;
                let gradient = codec::read_gradient(r)?;
                let mut to_gradient = [0.0; 6];
                for value in &mut to_gradient {
                    *value = f64::from(r.f32()?);
                }
                glyphs
                    .get_mut(glyph)
                    .ok_or_else(|| corrupt("a gradient of no filled outline"))?
                    .gradient = Some((Arc::new(gradient), kurbo::Affine::new(to_gradient)));
            }
        }
    }
    if !r.is_empty() {
        for _ in 0..r.len()? {
            let index = r.len()?;
            let (_, glyphs) = fills
                .iter_mut()
                .find(|(filled, _)| *filled == index)
                .ok_or_else(|| corrupt("clips of no filled outlines"))?;
            for _ in 0..r.len()? {
                let glyph = r.len()?;
                let outline = Resolver::path(tables, r.u32()?)?;
                let offset = kurbo::Vec2::new(f64::from(r.f32()?), f64::from(r.f32()?));
                let outside = r.bool()?;
                glyphs
                    .get_mut(glyph)
                    .ok_or_else(|| corrupt("a clip of no filled outline"))?
                    .clip = Some(gaanim_scene::PlaneClip {
                    outline,
                    offset,
                    outside,
                });
            }
        }
    }
    if !r.is_empty() {
        for _ in 0..r.len()? {
            let index = r.len()?;
            let count = r.len()?;
            if count > gaanim_scene::model::MAX_JOINTS {
                return Err(corrupt("a palette of more joints than a skin moves"));
            }
            let mut joints = Vec::with_capacity(count);
            for _ in 0..count {
                let mut matrix = [0.0_f32; 16];
                for value in &mut matrix {
                    *value = r.f32()?;
                }
                joints.push(matrix);
            }
            sources
                .get_mut(index)
                .ok_or_else(|| corrupt("a palette of no 3D source"))?
                .joints = Some(joints.into());
        }
    }
    if !r.is_empty() {
        for _ in 0..r.len()? {
            let index = r.len()?;
            let glow = gaanim_renderer::effects::Glow {
                color: codec::read_color(r)?,
                radius: r.f64()?,
                intensity: r.f32()?,
            };
            sources
                .get_mut(index)
                .ok_or_else(|| corrupt("a glow of no 3D source"))?
                .glow = Some(glow);
        }
    }
    let mut views = Vec::new();
    if !r.is_empty() {
        for _ in 0..r.len()? {
            let screen = r.u32()?;
            let camera = read_camera(r)?;
            let region = r.rect()?;
            let resolution = r.f64()?;
            let (three_d, _) = read_three_d(&mut Reader::new(r.bytes()?), tables)?;
            views.push((
                screen,
                Arc::new(FrozenThreeD {
                    three_d,
                    camera,
                    region,
                    resolution,
                }),
            ));
        }
    }
    if !r.is_empty() {
        for _ in 0..r.len()? {
            let index = r.len()?;
            let copies = tables.copies(r.u32()?)?;
            let reveal = r.f32()?;
            sources
                .get_mut(index)
                .ok_or_else(|| corrupt("copies of no 3D source"))?
                .instances = Some(gaanim_scene::MeshInstances { copies, reveal });
        }
    }
    if !r.is_empty() {
        for _ in 0..r.len()? {
            let index = r.len()?;
            let (_, glyphs) = fills
                .iter_mut()
                .find(|(filled, _)| *filled == index)
                .ok_or_else(|| corrupt("images of no filled outlines"))?;
            for _ in 0..r.len()? {
                let glyph = r.len()?;
                let brush = codec::read_image_brush(r, tables)?;
                let mut to_pixels = [0.0; 6];
                for value in &mut to_pixels {
                    *value = r.f64()?;
                }
                glyphs
                    .get_mut(glyph)
                    .ok_or_else(|| corrupt("an image of no filled outline"))?
                    .image = Some(gaanim_scene::PlaneImage {
                    brush,
                    to_pixels: kurbo::Affine::new(to_pixels),
                });
            }
        }
    }
    if !r.is_empty() {
        for _ in 0..r.len()? {
            let index = r.len()?;
            let color = codec::read_color(r)?;
            let offset = gaanim_core::glam::DVec2::new(r.f64()?, r.f64()?);
            let blur_radius = r.f64()?;
            sources
                .get_mut(index)
                .ok_or_else(|| corrupt("a shadow of no 3D source"))?
                .shadow = Some(gaanim_renderer::effects::DropShadow {
                offset,
                blur_radius,
                color,
            });
        }
    }
    if !r.is_empty() {
        for _ in 0..r.len()? {
            let index = r.len()?;
            let sigma = r.f64()?;
            sources
                .get_mut(index)
                .ok_or_else(|| corrupt("a blur of no 3D source"))?
                .blur = Some(gaanim_renderer::effects::GaussianBlur { sigma });
        }
    }
    if !r.is_empty() {
        for _ in 0..r.len()? {
            let index = r.len()?;
            let count = r.len()?;
            let mut weights = Vec::with_capacity(count.min(1 << 12));
            for _ in 0..count {
                weights.push(r.f32()?);
            }
            sources
                .get_mut(index)
                .ok_or_else(|| corrupt("morph weights of no 3D source"))?
                .morph = Some(weights.into());
        }
    }
    let mut transparency = gaanim_scene::Transparency3D::default();
    if !r.is_empty() {
        transparency.layers = r.u8()?.min(gaanim_scene::Transparency3D::MAX_LAYERS);
    }
    if !r.is_empty() {
        for _ in 0..r.len()? {
            let index = r.len()?;
            let direction = DVec3::new(r.f64()?, r.f64()?, r.f64()?);
            let spot = gaanim_renderer::three_d::Spot {
                direction,
                cos_inner: r.f32()?,
                cos_outer: r.f32()?,
            };
            lights
                .get_mut(index)
                .ok_or_else(|| corrupt("the cone of no light"))?
                .spot = Some(spot);
        }
    }
    // Without one, nothing blurs: the aperture is 0.
    let mut depth_of_field = gaanim_scene::DepthOfField::default();
    if !r.is_empty() {
        depth_of_field = gaanim_scene::DepthOfField {
            focus: r.f64()?,
            aperture: r.f64()?,
            max_blur: r.f64()?,
        };
    }
    if !r.is_empty() {
        for _ in 0..r.len()? {
            let index = r.len()?;
            let values = tables.image(r.u32()?)?;
            let size = [r.u32()?, r.u32()?, r.u32()?];
            let min = [r.f32()?, r.f32()?, r.f32()?];
            let max = [r.f32()?, r.f32()?, r.f32()?];
            let volume = gaanim_scene::VolumeData {
                values,
                size,
                min,
                max,
                transfer: tables.image(r.u32()?)?,
                density: r.f32()?,
            };
            sources
                .get_mut(index)
                .ok_or_else(|| corrupt("a volume of no 3D source"))?
                .volume = Some(Arc::new(volume));
        }
    }
    for (index, glyphs) in fills {
        sources[index].fill = Some(Arc::new(gaanim_scene::PlaneFillData { glyphs }));
    }
    Ok((
        CapturedThreeD {
            lighting,
            lights,
            transparency,
            depth_of_field,
            opacity_fallback,
            sources,
        },
        views,
    ))
}

// ---------------------------------------------------------------------------
// Post-process shaders
// ---------------------------------------------------------------------------

pub(crate) fn write_post_shader(w: &mut Writer, shader: &PostProcessShader) {
    w.str(shader.source());
    w.len(shader.uniforms().len());
    for uniform in shader.uniforms() {
        w.str(uniform);
    }
    w.option(shader.data(), |w, data| {
        w.len(data.len());
        for texel in data.iter() {
            for value in texel {
                w.f32(*value);
            }
        }
    });
    w.bool(shader.bloom());
}

pub(crate) fn read_post_shader(r: &mut Reader<'_>) -> Result<PostProcessShader> {
    let source = r.str()?.to_owned();
    let uniform_count = r.len()?;
    let mut uniforms = Vec::with_capacity(uniform_count.min(256));
    for _ in 0..uniform_count {
        uniforms.push(Arc::<str>::from(r.str()?));
    }
    let data = r.option(|r| {
        let count = r.len()?;
        let mut data = Vec::with_capacity(count.min(1 << 20));
        for _ in 0..count {
            data.push([r.f32()?, r.f32()?, r.f32()?, r.f32()?]);
        }
        Ok(Arc::<[[f32; 4]]>::from(data))
    })?;
    let bloom = r.bool()?;
    PostProcessShader::from_parts(source, &uniforms, data, bloom, false)
        .map_err(|error| BundleError::Corrupt(error.to_string()))
}

// ---------------------------------------------------------------------------
// Recipes
// ---------------------------------------------------------------------------

fn write_stroke_brush(w: &mut Writer, tables: &mut impl Interner, stroke: &StrokeBrush) {
    w.option(stroke.brush.as_ref(), |w, brush| {
        codec::write_brush(w, tables, brush)
    });
    codec::write_stroke(w, &stroke.style);
}

fn read_stroke_brush(r: &mut Reader<'_>, tables: &impl Resolver) -> Result<StrokeBrush> {
    Ok(StrokeBrush {
        brush: r.option(|r| codec::read_brush(r, tables))?,
        style: codec::read_stroke(r)?,
    })
}

fn write_stroke_align(w: &mut Writer, align: StrokeAlign) {
    w.u8(match align {
        StrokeAlign::Inside => 0,
        StrokeAlign::Center => 1,
        StrokeAlign::Outside => 2,
    });
}

fn read_stroke_align(r: &mut Reader<'_>) -> Result<StrokeAlign> {
    match r.u8()? {
        0 => Ok(StrokeAlign::Inside),
        1 => Ok(StrokeAlign::Center),
        2 => Ok(StrokeAlign::Outside),
        _ => Err(corrupt("invalid stroke alignment")),
    }
}

pub(crate) fn write_recipe(w: &mut Writer, tables: &mut Tables, recipe: &FragmentRecipe) {
    w.option(recipe.path.as_ref(), |w, path| {
        w.var(u64::from(tables.path(path)))
    });
    w.option(recipe.source.as_ref(), |w, path| {
        w.var(u64::from(tables.path(path)))
    });
    w.option(recipe.fill.as_ref(), |w, brush| {
        codec::write_brush(w, tables, brush)
    });
    w.option(recipe.stroke.as_ref(), |w, stroke| {
        write_stroke_brush(w, tables, stroke)
    });
    w.option(recipe.raster.as_ref(), |w, raster| {
        w.option(raster.image.as_ref(), |w, image| {
            codec::write_image_brush(w, tables, image)
        });
        w.affine(raster.local_transform);
    });
    w.bool(recipe.lottie);
    w.option(recipe.shadow.as_ref(), write_shadow);
    w.option(recipe.glow.as_ref(), |w, glow| {
        w.f64(glow.radius);
        w.f32(glow.intensity);
        codec::write_color(w, glow.color);
    });
    w.option(recipe.blur, |w, blur| w.f64(blur.sigma));
    w.option(recipe.fill_progress, Writer::f32);
    w.option(recipe.completion, Writer::f64);
    write_stroke_align(w, recipe.stroke_align);
    w.option(recipe.stroke_profile.as_ref(), |w, profile| {
        w.len(profile.0.len());
        for (position, factor) in profile.0.iter() {
            w.f64(*position);
            w.f64(*factor);
        }
    });
    w.option(recipe.stroke_view, Writer::affine);
    w.bool(recipe.screen);
    w.option(recipe.chalk.as_ref(), |w, chalk| {
        w.var(chalk.seed);
        w.f64(chalk.roughness);
    });
}

fn write_shadow(w: &mut Writer, shadow: &DropShadow) {
    w.f64(shadow.offset.x);
    w.f64(shadow.offset.y);
    w.f64(shadow.blur_radius);
    codec::write_color(w, shadow.color);
}

fn read_shadow(r: &mut Reader<'_>) -> Result<DropShadow> {
    Ok(DropShadow {
        offset: gaanim_core::glam::DVec2::new(r.f64()?, r.f64()?),
        blur_radius: r.f64()?,
        color: codec::read_color(r)?,
    })
}

pub(crate) fn read_recipe(r: &mut Reader<'_>, tables: &DecodedTables) -> Result<FragmentRecipe> {
    Ok(FragmentRecipe {
        path: r.option(|r| tables.path(r.u32()?))?,
        source: r.option(|r| tables.path(r.u32()?))?,
        fill: r.option(|r| codec::read_brush(r, tables))?,
        stroke: r.option(|r| read_stroke_brush(r, tables))?,
        raster: r.option(|r| {
            Ok(RasterImage {
                image: r.option(|r| codec::read_image_brush(r, tables))?,
                local_transform: r.affine()?,
            })
        })?,
        lottie: r.bool()?,
        shadow: r.option(read_shadow)?,
        glow: r.option(|r| {
            Ok(Glow {
                radius: r.f64()?,
                intensity: r.f32()?,
                color: codec::read_color(r)?,
            })
        })?,
        blur: r.option(|r| Ok(GaussianBlur { sigma: r.f64()? }))?,
        fill_progress: r.option(Reader::f32)?,
        completion: r.option(Reader::f64)?,
        stroke_align: read_stroke_align(r)?,
        stroke_profile: r.option(|r| {
            let count = r.len()?;
            let mut points = Vec::with_capacity(count.min(1 << 16));
            for _ in 0..count {
                points.push((r.f64()?, r.f64()?));
            }
            Ok(StrokeProfile(points.into()))
        })?,
        stroke_view: r.option(Reader::affine)?,
        screen: r.bool()?,
        chalk: r.option(|r| {
            Ok(ChalkBrush {
                seed: r.var()?,
                roughness: r.f64()?,
            })
        })?,
    })
}

// ---------------------------------------------------------------------------
// Camera
// ---------------------------------------------------------------------------

pub(crate) fn write_camera(w: &mut Writer, camera: &gaanim_math::Camera) {
    for value in [
        camera.position.x,
        camera.position.y,
        camera.position.z,
        camera.rotation.x,
        camera.rotation.y,
        camera.rotation.z,
        camera.rotation.w,
        camera.target.x,
        camera.target.y,
        camera.target.z,
        camera.up.x,
        camera.up.y,
        camera.up.z,
    ] {
        w.f64(value);
    }
    match camera.projection {
        gaanim_math::Projection::Orthographic { zoom } => {
            w.u8(0);
            w.f64(zoom);
        }
        // A shifted lens adds its shift: a centered one is written as
        // before version 13.
        gaanim_math::Projection::Perspective { fov_y, near, far } => {
            let shifted = camera.lens_shift != gaanim_core::glam::DVec2::ZERO;
            w.u8(if shifted { 2 } else { 1 });
            w.f64(fov_y);
            w.f64(near);
            w.f64(far);
            if shifted {
                w.f64(camera.lens_shift.x);
                w.f64(camera.lens_shift.y);
            }
        }
    }
    w.f64(camera.frame_width);
    w.f64(camera.frame_height);
    w.var(u64::from(camera.viewport_width));
    w.var(u64::from(camera.viewport_height));
}

pub(crate) fn read_camera(r: &mut Reader<'_>) -> Result<gaanim_math::Camera> {
    use gaanim_core::glam::{DQuat, DVec3};
    let position = DVec3::new(r.f64()?, r.f64()?, r.f64()?);
    let rotation = DQuat::from_xyzw(r.f64()?, r.f64()?, r.f64()?, r.f64()?);
    let target = DVec3::new(r.f64()?, r.f64()?, r.f64()?);
    let up = DVec3::new(r.f64()?, r.f64()?, r.f64()?);
    let tag = r.u8()?;
    let projection = match tag {
        0 => gaanim_math::Projection::Orthographic { zoom: r.f64()? },
        1 | 2 => gaanim_math::Projection::Perspective {
            fov_y: r.f64()?,
            near: r.f64()?,
            far: r.f64()?,
        },
        _ => return Err(corrupt("invalid projection")),
    };
    let lens_shift = if tag == 2 {
        gaanim_core::glam::DVec2::new(r.f64()?, r.f64()?)
    } else {
        gaanim_core::glam::DVec2::ZERO
    };
    Ok(gaanim_math::Camera {
        position,
        rotation,
        target,
        up,
        projection,
        frame_width: r.f64()?,
        frame_height: r.f64()?,
        viewport_width: r.u32()?,
        viewport_height: r.u32()?,
        lens_shift,
    })
}

// ---------------------------------------------------------------------------
// Transitions
// ---------------------------------------------------------------------------

fn write_mask(w: &mut Writer, mask: &TransitionMask) {
    codec::write_path(w, &mask.path);
    codec::write_fill(w, mask.rule);
    w.option(mask.fade, |w, (a, b)| {
        w.point(a);
        w.point(b);
    });
}

fn read_mask(r: &mut Reader<'_>) -> Result<TransitionMask> {
    Ok(TransitionMask {
        path: codec::read_path(r)?,
        rule: codec::read_fill(r)?,
        fade: r.option(|r| Ok((r.point()?, r.point()?)))?,
    })
}

fn write_transition(w: &mut Writer, tables: &mut Tables, transition: &CapturedTransition) {
    w.option(transition.outgoing_mask.as_ref(), write_mask);
    w.option(transition.incoming_mask.as_ref(), write_mask);
    w.option(transition.backgrounds, |w, (a, b)| {
        w.f64(a);
        w.f64(b);
    });
    w.len(transition.overlays.len());
    for overlay in &transition.overlays {
        codec::write_blend(w, overlay.blend);
        codec::write_path(w, &overlay.clip);
        w.len(overlay.fills.len());
        for (path, brush) in &overlay.fills {
            codec::write_path(w, path);
            codec::write_brush(w, tables, brush);
        }
    }
    w.option(transition.shader.as_ref(), |w, frame| {
        w.var(u64::from(tables.transition_shader(&frame.shader)));
        w.f32(frame.progress);
        w.len(frame.values.len());
        for value in &frame.values {
            w.f32(*value);
        }
    });
    // The camera the outgoing side keeps (version 14 on), only when it
    // has one: a transition without it is written as before.
    if let Some(camera) = &transition.outgoing_camera {
        write_camera(w, camera);
        for value in transition.outgoing_view.as_coeffs() {
            w.f64(value);
        }
    }
}

/// Write a shader transition table entry.
pub(crate) fn write_transition_shader(w: &mut Writer, shader: &TransitionShader) {
    w.str(&shader.source);
    w.len(shader.uniforms.len());
    for uniform in &shader.uniforms {
        w.str(uniform);
    }
    w.len(shader.values.len());
    for value in &shader.values {
        w.f32(*value);
    }
    w.option(shader.data.as_ref(), |w, data| {
        w.len(data.len());
        for texel in data {
            for value in texel {
                w.f32(*value);
            }
        }
    });
}

/// Read an entry [`write_transition_shader`] wrote.
pub(crate) fn read_transition_shader(r: &mut Reader<'_>) -> Result<TransitionShader> {
    let source = r.str()?.to_owned();
    let count = r.len()?;
    let mut uniforms = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        uniforms.push(r.str()?.to_owned());
    }
    let count = r.len()?;
    let mut values = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        values.push(r.f32()?);
    }
    let data = r.option(|r| {
        let count = r.len()?;
        let mut data = Vec::with_capacity(count.min(1 << 20));
        for _ in 0..count {
            data.push([r.f32()?, r.f32()?, r.f32()?, r.f32()?]);
        }
        Ok(data)
    })?;
    Ok(TransitionShader {
        source,
        uniforms,
        values,
        data,
    })
}

fn read_transition(r: &mut Reader<'_>, tables: &DecodedTables) -> Result<CapturedTransition> {
    let outgoing_mask = r.option(read_mask)?;
    let incoming_mask = r.option(read_mask)?;
    let backgrounds = r.option(|r| Ok((r.f64()?, r.f64()?)))?;
    let count = r.len()?;
    let mut overlays = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        let blend = codec::read_blend(r)?;
        let clip = codec::read_path(r)?;
        let fills_count = r.len()?;
        let mut fills = Vec::with_capacity(fills_count.min(1 << 12));
        for _ in 0..fills_count {
            fills.push((codec::read_path(r)?, codec::read_brush(r, tables)?));
        }
        overlays.push(gaanim_scene::TransitionOverlayLayer { blend, clip, fills });
    }
    let shader = r.option(|r| {
        let shader = tables.transition_shader(r.u32()?)?;
        let progress = r.f32()?;
        let count = r.len()?;
        let mut values = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            values.push(r.f32()?);
        }
        Ok(TransitionShaderFrame {
            shader,
            progress,
            values,
        })
    })?;
    let (outgoing_camera, outgoing_view) = if r.is_empty() {
        (None, kurbo::Affine::IDENTITY)
    } else {
        let camera = read_camera(r)?;
        let mut coefficients = [0.0; 6];
        for value in &mut coefficients {
            *value = r.f64()?;
        }
        (Some(camera), kurbo::Affine::new(coefficients))
    };
    Ok(CapturedTransition {
        outgoing_mask,
        incoming_mask,
        backgrounds,
        overlays,
        shader,
        outgoing_camera,
        outgoing_view,
    })
}

// ---------------------------------------------------------------------------
// Elements
// ---------------------------------------------------------------------------

/// A frame element with its references resolved to table indices and keys.
#[derive(Clone, PartialEq)]
pub(crate) struct ElementRecord {
    pub key: u32,
    pub recipe: u32,
    pub lottie: Option<u32>,
    pub transform: kurbo::Affine,
    pub opacity: f32,
    pub opacity_bounds: kurbo::Rect,
    pub opacity_reach: Option<f64>,
    pub opacity_group: u32,
    pub render_order: RenderOrder,
    pub clip: Option<ClipRecord>,
    pub blend: Option<peniko::BlendMode>,
    pub side: TransitionSide,
    pub lineage: Vec<u32>,
    pub view_bounds: Option<kurbo::Rect>,
    pub in_views: bool,
    pub layer: Option<u32>,
    pub screen: Option<ViewRecord>,
    pub echo_rank: u32,
    pub group_opacity: f32,
    /// The shared drop shadow and the table index of its outline.
    pub group_shadow: Option<(u32, DropShadow)>,
    pub effect_root: Option<u32>,
    pub effect_extent: Option<kurbo::Rect>,
    pub matte_root: Option<u32>,
    pub matte_of: Option<u32>,
    pub glass_root: Option<u32>,
}

#[derive(Clone, PartialEq)]
pub(crate) struct ClipRecord {
    pub path: u32,
    pub rule: peniko::Fill,
    pub sources: Vec<u32>,
    pub invert: bool,
}

#[derive(Clone, PartialEq)]
pub(crate) struct ViewRecord {
    pub clip: u32,
    pub content: Option<kurbo::Affine>,
    pub region: kurbo::Rect,
    pub excluded: Vec<u32>,
    pub layers: Vec<u32>,
    /// Shows only the drawables on `layers` (version 7 on).
    pub only_layers: bool,
    pub background: ViewBackgroundRecord,
}

#[derive(Clone, PartialEq)]
pub(crate) enum ViewBackgroundRecord {
    Canvas,
    None,
    /// An encoded brush.
    Brush(Vec<u8>),
}

fn write_keys(w: &mut Writer, keys: &[u32]) {
    w.len(keys.len());
    for key in keys {
        w.var(u64::from(*key));
    }
}

fn read_keys(r: &mut Reader<'_>) -> Result<Vec<u32>> {
    let count = r.len()?;
    let mut keys = Vec::with_capacity(count.min(1 << 16));
    for _ in 0..count {
        keys.push(r.u32()?);
    }
    Ok(keys)
}

impl ElementRecord {
    /// Resolve a captured element against the recording's tables.
    pub fn capture(
        element: &CapturedElement,
        tables: &mut Tables,
        keys: &mut EntityKeys,
        lottie: Option<u32>,
    ) -> Self {
        let mut key_of = |entity: Entity| keys.key(entity);
        let key = key_of(element.entity);
        if element.post_exempt {
            tables.post_exempt.insert(key);
        }
        Self {
            key,
            recipe: tables.recipe(&element.recipe),
            lottie,
            transform: element.transform,
            opacity: element.opacity,
            opacity_bounds: element.opacity_bounds,
            opacity_reach: element.opacity_reach,
            opacity_group: key_of(element.opacity_group),
            render_order: element.render_order,
            clip: element.clip_mask.as_ref().map(|clip| ClipRecord {
                path: tables.path(&Arc::new(clip.path.clone())),
                rule: clip.rule,
                sources: clip.sources.iter().map(|entity| key_of(*entity)).collect(),
                invert: clip.invert,
            }),
            blend: element.blend,
            side: element.transition_side,
            lineage: element
                .lineage
                .iter()
                .map(|entity| key_of(*entity))
                .collect(),
            view_bounds: element.view_bounds,
            in_views: element.in_views,
            layer: element.layer.as_ref().map(|layer| tables.string(layer)),
            screen: element.screen.as_ref().map(|view| ViewRecord {
                clip: tables.path(&view.clip),
                content: view.content,
                region: view.region,
                excluded: view.excluded.iter().map(|entity| key_of(*entity)).collect(),
                layers: view
                    .layers
                    .iter()
                    .map(|layer| tables.string(layer))
                    .collect(),
                only_layers: view.only_layers,
                background: match &view.background {
                    CameraViewBackground::Canvas => ViewBackgroundRecord::Canvas,
                    CameraViewBackground::None => ViewBackgroundRecord::None,
                    CameraViewBackground::Brush(brush) => {
                        let mut w = Writer::new();
                        codec::write_brush(&mut w, tables, brush);
                        ViewBackgroundRecord::Brush(w.into_bytes())
                    }
                },
            }),
            echo_rank: element.echo_rank,
            group_opacity: element.group_opacity,
            group_shadow: element
                .group_shadow
                .as_ref()
                .map(|shared| (tables.path(&shared.path), shared.shadow.clone())),
            effect_root: element.effect_root.map(&mut key_of),
            effect_extent: element.effect_extent,
            matte_root: element.matte_root.map(&mut key_of),
            matte_of: element.matte_of.map(&mut key_of),
            glass_root: element.glass_root.map(&mut key_of),
        }
    }

    pub fn write(&self, w: &mut Writer) {
        w.var(u64::from(self.key));
        w.var(u64::from(self.recipe));
        w.option(self.lottie, |w, index| w.var(u64::from(index)));
        w.affine(self.transform);
        w.f32(self.opacity);
        w.rect(self.opacity_bounds);
        w.option(self.opacity_reach, Writer::f64);
        w.var(u64::from(self.opacity_group));
        w.ivar(i64::from(self.render_order.z_index));
        w.var(self.render_order.creation_order);
        w.option(self.clip.as_ref(), |w, clip| {
            w.var(u64::from(clip.path));
            codec::write_fill(w, clip.rule);
            write_keys(w, &clip.sources);
            w.bool(clip.invert);
        });
        w.option(self.blend, codec::write_blend);
        w.u8(match self.side {
            TransitionSide::None => 0,
            TransitionSide::Outgoing => 1,
            TransitionSide::Incoming => 2,
        });
        write_keys(w, &self.lineage);
        w.option(self.view_bounds, Writer::rect);
        w.bool(self.in_views);
        w.option(self.layer, |w, index| w.var(u64::from(index)));
        w.option(self.screen.as_ref(), |w, view| {
            w.var(u64::from(view.clip));
            w.option(view.content, Writer::affine);
            w.rect(view.region);
            write_keys(w, &view.excluded);
            write_keys(w, &view.layers);
            w.bool(view.only_layers);
            match &view.background {
                ViewBackgroundRecord::Canvas => w.u8(0),
                ViewBackgroundRecord::None => w.u8(1),
                ViewBackgroundRecord::Brush(bytes) => {
                    w.u8(2);
                    w.bytes(bytes);
                }
            }
        });
        w.var(u64::from(self.echo_rank));
        w.f32(self.group_opacity);
        w.option(self.group_shadow.as_ref(), |w, (path, shadow)| {
            w.var(u64::from(*path));
            write_shadow(w, shadow);
        });
        w.option(self.effect_root, |w, key| w.var(u64::from(key)));
        w.option(self.effect_extent, Writer::rect);
        w.option(self.matte_root, |w, key| w.var(u64::from(key)));
        w.option(self.matte_of, |w, key| w.var(u64::from(key)));
        w.option(self.glass_root, |w, key| w.var(u64::from(key)));
    }

    /// Read a record of a bundle of format `version`.
    pub fn read(r: &mut Reader<'_>, version: u32) -> Result<Self> {
        Ok(Self {
            key: r.u32()?,
            recipe: r.u32()?,
            lottie: r.option(Reader::u32)?,
            transform: r.affine()?,
            opacity: r.f32()?,
            opacity_bounds: r.rect()?,
            opacity_reach: r.option(Reader::f64)?,
            opacity_group: r.u32()?,
            render_order: RenderOrder {
                z_index: i32::try_from(r.ivar()?).map_err(|_| corrupt("z-index out of range"))?,
                creation_order: r.var()?,
            },
            clip: r.option(|r| {
                Ok(ClipRecord {
                    path: r.u32()?,
                    rule: codec::read_fill(r)?,
                    sources: read_keys(r)?,
                    invert: r.bool()?,
                })
            })?,
            blend: r.option(codec::read_blend)?,
            side: match r.u8()? {
                0 => TransitionSide::None,
                1 => TransitionSide::Outgoing,
                2 => TransitionSide::Incoming,
                _ => return Err(corrupt("invalid transition side")),
            },
            lineage: read_keys(r)?,
            view_bounds: r.option(Reader::rect)?,
            in_views: r.bool()?,
            layer: r.option(Reader::u32)?,
            screen: r.option(|r| {
                Ok(ViewRecord {
                    clip: r.u32()?,
                    content: r.option(Reader::affine)?,
                    region: r.rect()?,
                    excluded: read_keys(r)?,
                    layers: read_keys(r)?,
                    only_layers: version >= 7 && r.bool()?,
                    background: match r.u8()? {
                        0 => ViewBackgroundRecord::Canvas,
                        1 => ViewBackgroundRecord::None,
                        2 => ViewBackgroundRecord::Brush(r.bytes()?.to_vec()),
                        _ => return Err(corrupt("invalid view background")),
                    },
                })
            })?,
            echo_rank: r.u32()?,
            group_opacity: r.f32()?,
            group_shadow: r.option(|r| Ok((r.u32()?, read_shadow(r)?)))?,
            effect_root: r.option(Reader::u32)?,
            effect_extent: r.option(Reader::rect)?,
            matte_root: r.option(Reader::u32)?,
            matte_of: r.option(Reader::u32)?,
            glass_root: r.option(Reader::u32)?,
        })
    }

    /// The captured element this record describes, drawn through
    /// `lottie_scene` when it records a Lottie frame.
    pub fn resolve(
        &self,
        tables: &DecodedTables,
        lottie_scene: Option<Arc<vello::Scene>>,
    ) -> Result<CapturedElement> {
        let entities = |keys: &[u32]| {
            keys.iter()
                .map(|key| key_entity(*key))
                .collect::<Result<Vec<_>>>()
        };
        Ok(CapturedElement {
            entity: key_entity(self.key)?,
            recipe: tables.recipe(self.recipe)?,
            lottie: lottie_scene,
            transform: self.transform,
            opacity: self.opacity,
            opacity_bounds: self.opacity_bounds,
            opacity_reach: self.opacity_reach,
            opacity_group: key_entity(self.opacity_group)?,
            render_order: self.render_order,
            clip_mask: self
                .clip
                .as_ref()
                .map(|clip| -> Result<ClipMask> {
                    Ok(ClipMask {
                        path: tables.path(clip.path)?.as_ref().clone(),
                        rule: clip.rule,
                        sources: entities(&clip.sources)?,
                        invert: clip.invert,
                    })
                })
                .transpose()?,
            blend: self.blend,
            transition_side: self.side,
            post_exempt: tables.post_exempt.contains(&self.key),
            lineage: entities(&self.lineage)?,
            view_bounds: self.view_bounds,
            in_views: self.in_views,
            layer: self.layer.map(|index| tables.string(index)).transpose()?,
            screen: self
                .screen
                .as_ref()
                .map(|view| -> Result<CapturedView> {
                    Ok(CapturedView {
                        clip: tables.path(view.clip)?,
                        content: view.content,
                        region: view.region,
                        excluded: entities(&view.excluded)?,
                        layers: view
                            .layers
                            .iter()
                            .map(|index| tables.string(*index))
                            .collect::<Result<_>>()?,
                        only_layers: view.only_layers,
                        frozen: None,
                        background: match &view.background {
                            ViewBackgroundRecord::Canvas => CameraViewBackground::Canvas,
                            ViewBackgroundRecord::None => CameraViewBackground::None,
                            ViewBackgroundRecord::Brush(bytes) => CameraViewBackground::Brush(
                                codec::read_brush(&mut Reader::new(bytes), tables)?,
                            ),
                        },
                    })
                })
                .transpose()?,
            echo_rank: self.echo_rank,
            group_opacity: self.group_opacity,
            group_shadow: self
                .group_shadow
                .as_ref()
                .map(|(path, shadow)| -> Result<_> {
                    Ok(Arc::new(GroupShadow {
                        shadow: shadow.clone(),
                        path: tables.path(*path)?,
                    }))
                })
                .transpose()?,
            effect_root: self.effect_root.map(key_entity).transpose()?,
            effect_extent: self.effect_extent,
            matte_root: self.matte_root.map(key_entity).transpose()?,
            matte_of: self.matte_of.map(key_entity).transpose()?,
            glass_root: self.glass_root.map(key_entity).transpose()?,
        })
    }
}

// ---------------------------------------------------------------------------
// Frame records and their delta encoding
// ---------------------------------------------------------------------------

/// A frame with its elements resolved to records.
#[derive(Clone)]
pub(crate) struct FrameRecord {
    pub time: f64,
    pub background_time: f64,
    /// Uniforms of the shader backgrounds the frame draws.
    pub background_values: Vec<gaanim_renderer::pipeline::BackgroundValues>,
    pub camera: gaanim_math::Camera,
    /// Encoded [`CapturedTransition`], compared as bytes.
    pub transition: Option<Vec<u8>>,
    pub post: Vec<PostPass>,
    pub elements: Vec<ElementRecord>,
    /// Shader effects of drawables; each pass's shader indexes
    /// [`Tables::effect_shaders`].
    pub effects: Vec<EffectRecord>,
    /// Track mattes: the matted drawable, its matte and the mode.
    pub mattes: Vec<(u32, u32, MatteMode)>,
    /// Glass drawables and their glass.
    pub glasses: Vec<(u32, Glass)>,
    /// Encoded native 3D content ([`write_three_d`]), compared as bytes;
    /// `elements` leaves out what its projection draws.
    pub three_d: Option<Vec<u8>>,
    /// Motion blur sub-frames, each without sub-frames of its own.
    pub motion_blur: Vec<FrameRecord>,
}

/// A [`CapturedEffect`] with its drawable and shaders resolved to keys and
/// table indices.
#[derive(Clone, PartialEq)]
pub(crate) struct EffectRecord {
    pub root: u32,
    pub margin: f64,
    pub passes: Vec<PostPass>,
    /// The second a snapshot froze the effect at (version 7 on).
    pub time: Option<f64>,
}

impl EffectRecord {
    fn write(&self, w: &mut Writer) {
        w.var(u64::from(self.root));
        w.f64(self.margin);
        write_passes(w, &self.passes);
        w.option(self.time.as_ref(), |w, time| w.f64(*time));
    }

    fn read(r: &mut Reader<'_>, version: u32) -> Result<Self> {
        Ok(Self {
            root: r.u32()?,
            margin: r.f64()?,
            passes: read_passes(r)?,
            time: if version >= 7 {
                r.option(|r| r.f64())?
            } else {
                None
            },
        })
    }
}

fn write_matte_mode(w: &mut Writer, mode: MatteMode) {
    w.u8(match mode {
        MatteMode::Alpha => 0,
        MatteMode::AlphaInverted => 1,
        MatteMode::Luma => 2,
        MatteMode::LumaInverted => 3,
    });
}

fn read_matte_mode(r: &mut Reader<'_>) -> Result<MatteMode> {
    match r.u8()? {
        0 => Ok(MatteMode::Alpha),
        1 => Ok(MatteMode::AlphaInverted),
        2 => Ok(MatteMode::Luma),
        3 => Ok(MatteMode::LumaInverted),
        _ => Err(corrupt("invalid matte mode")),
    }
}

fn write_passes(w: &mut Writer, passes: &[PostPass]) {
    w.len(passes.len());
    for pass in passes {
        w.var(u64::from(pass.shader));
        w.len(pass.values.len());
        for value in &pass.values {
            w.f32(*value);
        }
    }
}

fn read_passes(r: &mut Reader<'_>) -> Result<Vec<PostPass>> {
    let pass_count = r.len()?;
    let mut passes = Vec::with_capacity(pass_count.min(64));
    for _ in 0..pass_count {
        let shader = r.u32()?;
        let count = r.len()?;
        let mut values = Vec::with_capacity(count.min(256));
        for _ in 0..count {
            values.push(r.f32()?);
        }
        passes.push(PostPass { shader, values });
    }
    Ok(passes)
}

impl FrameRecord {
    pub fn capture(
        frame: &Frame,
        tables: &mut Tables,
        keys: &mut EntityKeys,
        lottie: &dyn Fn(&CapturedElement) -> Option<u32>,
    ) -> Self {
        if !frame.capture.glasses.is_empty() {
            tables.glass = true;
        }
        // The 3D content camera views draw on their own goes with the frame's.
        let frozen: Vec<(u32, &FrozenThreeD)> = frame
            .capture
            .elements
            .iter()
            .filter_map(|element| {
                let frozen = element.screen.as_ref()?.frozen.as_deref()?;
                Some((keys.key(element.entity), frozen))
            })
            .collect();
        let empty = CapturedThreeD::default();
        let three_d = frame
            .capture
            .three_d
            .as_ref()
            .or_else(|| (!frozen.is_empty()).then_some(&empty));
        Self {
            three_d: three_d.map(|three_d| {
                let mut w = Writer::new();
                write_three_d(&mut w, tables, keys, three_d, &frozen);
                w.into_bytes()
            }),
            time: frame.time,
            background_time: frame.capture.background_time,
            background_values: frame.capture.background_values.clone(),
            camera: frame.camera,
            transition: frame.capture.transition.as_ref().map(|transition| {
                let mut w = Writer::new();
                write_transition(&mut w, tables, transition);
                w.into_bytes()
            }),
            post: frame.post.clone(),
            elements: frame
                .capture
                .elements
                .iter()
                .map(|element| ElementRecord::capture(element, tables, keys, lottie(element)))
                .collect(),
            effects: frame
                .capture
                .effects
                .iter()
                .map(|effect| EffectRecord {
                    root: keys.key(effect.root),
                    margin: effect.margin,
                    passes: effect
                        .passes
                        .iter()
                        .map(|(shader, values)| PostPass {
                            shader: tables.effect_shader(shader),
                            values: values.clone(),
                        })
                        .collect(),
                    time: effect.time,
                })
                .collect(),
            mattes: frame
                .capture
                .mattes
                .iter()
                .map(|matte| (keys.key(matte.root), keys.key(matte.source), matte.mode))
                .collect(),
            glasses: frame
                .capture
                .glasses
                .iter()
                .map(|glass| (keys.key(glass.root), glass.glass))
                .collect(),
            motion_blur: frame
                .motion_blur
                .iter()
                .map(|sample| Self::capture(sample, tables, keys, lottie))
                .collect(),
        }
    }

    pub fn resolve(
        &self,
        tables: &DecodedTables,
        lottie_scene: &mut dyn FnMut(u32) -> Result<Option<Arc<vello::Scene>>>,
    ) -> Result<Frame> {
        // The GPU draws the 3D content from what was recorded (see
        // `gaanim_renderer::three_d`).
        let (three_d, frozen) = self
            .three_d
            .as_ref()
            .map(|bytes| read_three_d(&mut Reader::new(bytes), tables))
            .transpose()?
            .unzip();
        let frozen = frozen.unwrap_or_default();
        let mut elements = Vec::with_capacity(self.elements.len());
        for element in &self.elements {
            let scene = element
                .lottie
                .map(&mut *lottie_scene)
                .transpose()?
                .flatten();
            let mut resolved = element.resolve(tables, scene)?;
            if let Some(view) = resolved.screen.as_mut() {
                view.frozen = frozen
                    .iter()
                    .find(|(screen, _)| *screen == element.key)
                    .map(|(_, frozen)| Arc::clone(frozen));
            }
            elements.push(resolved);
        }
        Ok(Frame {
            time: self.time,
            camera: self.camera,
            capture: FrameCapture {
                background_time: self.background_time,
                background_values: self.background_values.clone(),
                elements,
                transition: self
                    .transition
                    .as_ref()
                    .map(|bytes| read_transition(&mut Reader::new(bytes), tables))
                    .transpose()?,
                effects: self
                    .effects
                    .iter()
                    .map(|effect| -> Result<CapturedEffect> {
                        Ok(CapturedEffect {
                            root: key_entity(effect.root)?,
                            margin: effect.margin,
                            passes: effect
                                .passes
                                .iter()
                                .map(|pass| {
                                    Ok((tables.effect_shader(pass.shader)?, pass.values.clone()))
                                })
                                .collect::<Result<_>>()?,
                            time: effect.time,
                        })
                    })
                    .collect::<Result<_>>()?,
                mattes: self
                    .mattes
                    .iter()
                    .map(|&(root, source, mode)| -> Result<CapturedMatte> {
                        Ok(CapturedMatte {
                            root: key_entity(root)?,
                            source: key_entity(source)?,
                            mode,
                        })
                    })
                    .collect::<Result<_>>()?,
                glasses: self
                    .glasses
                    .iter()
                    .map(|&(root, glass)| -> Result<CapturedGlass> {
                        Ok(CapturedGlass {
                            root: key_entity(root)?,
                            glass,
                        })
                    })
                    .collect::<Result<_>>()?,
                three_d,
            },
            post: self.post.clone(),
            motion_blur: self
                .motion_blur
                .iter()
                .map(|sample| sample.resolve(tables, &mut *lottie_scene))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

/// Whether no two `elements` belong to the same drawable.
fn unique_keys(elements: &[ElementRecord]) -> bool {
    let mut seen = std::collections::HashSet::with_capacity(elements.len());
    elements.iter().all(|element| seen.insert(element.key))
}

/// Encodes a run of frames, each against the one before it. The first frame
/// of a chunk is written whole so a chunk decodes on its own.
#[derive(Default)]
pub(crate) struct DeltaEncoder {
    previous: Option<FrameRecord>,
}

const FRAME_FULL: u8 = 0;
const FRAME_DELTA: u8 = 1;
/// A frame without native 3D content, with the previous frame's, or with
/// its own, which follows.
const THREE_D_NONE: u8 = 0;
const THREE_D_SAME: u8 = 1;
const THREE_D_NEW: u8 = 2;

impl DeltaEncoder {
    pub fn reset(&mut self) {
        self.previous = None;
    }

    /// Write `frame`, then its motion blur sub-frames, each against the one
    /// before it.
    pub fn write(&mut self, w: &mut Writer, mut frame: FrameRecord) {
        let samples = std::mem::take(&mut frame.motion_blur);
        self.write_frame(w, frame);
        w.len(samples.len());
        let mut sub = DeltaEncoder {
            previous: self.previous.clone(),
        };
        for sample in samples {
            sub.write_frame(w, sample);
        }
    }

    fn write_frame(&mut self, w: &mut Writer, frame: FrameRecord) {
        w.f64(frame.time);
        w.f64(frame.background_time);
        w.len(frame.background_values.len());
        for entry in &frame.background_values {
            w.var(u64::from(entry.paint));
            w.f64(entry.time);
            w.len(entry.values.len());
            for value in &entry.values {
                w.f32(*value);
            }
        }
        write_camera(w, &frame.camera);
        w.option(frame.transition.as_ref(), |w, bytes| w.bytes(bytes));
        write_passes(w, &frame.post);
        w.len(frame.effects.len());
        for effect in &frame.effects {
            effect.write(w);
        }
        w.len(frame.mattes.len());
        for &(root, source, mode) in &frame.mattes {
            w.var(u64::from(root));
            w.var(u64::from(source));
            write_matte_mode(w, mode);
        }
        w.len(frame.glasses.len());
        for (root, glass) in &frame.glasses {
            w.var(u64::from(*root));
            for value in [
                glass.blur,
                glass.saturation,
                glass.refraction,
                glass.edge,
                glass.dispersion,
                glass.bevel,
                glass.transparency,
                glass.twist,
            ] {
                w.f64(value);
            }
        }
        // 3D content usually stays while the camera moves: written once.
        match &frame.three_d {
            None => w.u8(THREE_D_NONE),
            Some(bytes)
                if self
                    .previous
                    .as_ref()
                    .is_some_and(|previous| previous.three_d.as_ref() == Some(bytes)) =>
            {
                w.u8(THREE_D_SAME)
            }
            Some(bytes) => {
                w.u8(THREE_D_NEW);
                w.bytes(bytes);
            }
        }
        // A delta matches elements by their drawable's key, so a frame where
        // one drawable draws several elements (the runs of a 3D mesh, echo
        // copies) is written whole, as is the frame after it.
        let previous = self
            .previous
            .as_ref()
            .filter(|previous| unique_keys(&previous.elements) && unique_keys(&frame.elements));
        match previous {
            None => {
                w.u8(FRAME_FULL);
                w.len(frame.elements.len());
                for element in &frame.elements {
                    element.write(w);
                }
            }
            Some(previous) => {
                w.u8(FRAME_DELTA);
                let before: HashMap<u32, &ElementRecord> = previous
                    .elements
                    .iter()
                    .map(|element| (element.key, element))
                    .collect();
                // Elements whose record changed or that are new.
                let changed: Vec<&ElementRecord> = frame
                    .elements
                    .iter()
                    .filter(|element| before.get(&element.key) != Some(element))
                    .collect();
                w.len(changed.len());
                for element in changed {
                    element.write(w);
                }
                // The draw order, omitted when it did not change.
                let same_order = previous.elements.len() == frame.elements.len()
                    && previous
                        .elements
                        .iter()
                        .zip(&frame.elements)
                        .all(|(a, b)| a.key == b.key);
                w.bool(same_order);
                if !same_order {
                    let order: Vec<u32> =
                        frame.elements.iter().map(|element| element.key).collect();
                    write_keys(w, &order);
                }
            }
        }
        self.previous = Some(frame);
    }
}

pub(crate) struct DeltaDecoder {
    previous: Option<FrameRecord>,
    /// Format version of the frames: version 5 frames hold no 3D content.
    version: u32,
}

impl DeltaDecoder {
    pub fn new(version: u32) -> Self {
        Self {
            previous: None,
            version,
        }
    }

    pub fn read(&mut self, r: &mut Reader<'_>) -> Result<FrameRecord> {
        let mut frame = self.read_frame(r)?;
        let count = r.len()?;
        let mut sub = DeltaDecoder {
            previous: self.previous.clone(),
            version: self.version,
        };
        frame.motion_blur = (0..count)
            .map(|_| sub.read_frame(r))
            .collect::<Result<Vec<_>>>()?;
        Ok(frame)
    }

    fn read_frame(&mut self, r: &mut Reader<'_>) -> Result<FrameRecord> {
        let time = r.f64()?;
        let background_time = r.f64()?;
        let count = r.len()?;
        let mut background_values = Vec::with_capacity(count.min(16));
        for _ in 0..count {
            let paint = u32::try_from(r.var()?).map_err(|_| corrupt("background paint index"))?;
            let time = r.f64()?;
            let count = r.len()?;
            let mut values = Vec::with_capacity(count.min(64));
            for _ in 0..count {
                values.push(r.f32()?);
            }
            background_values.push(gaanim_renderer::pipeline::BackgroundValues {
                paint,
                time,
                values,
            });
        }
        let camera = read_camera(r)?;
        let transition = r.option(|r| Ok(r.bytes()?.to_vec()))?;
        let post = read_passes(r)?;
        let effect_count = r.len()?;
        let mut effects = Vec::with_capacity(effect_count.min(1 << 12));
        for _ in 0..effect_count {
            effects.push(EffectRecord::read(r, self.version)?);
        }
        let matte_count = r.len()?;
        let mut mattes = Vec::with_capacity(matte_count.min(1 << 12));
        for _ in 0..matte_count {
            mattes.push((r.u32()?, r.u32()?, read_matte_mode(r)?));
        }
        let glass_count = r.len()?;
        let mut glasses = Vec::with_capacity(glass_count.min(1 << 12));
        for _ in 0..glass_count {
            glasses.push((
                r.u32()?,
                Glass {
                    blur: r.f64()?,
                    saturation: r.f64()?,
                    refraction: r.f64()?,
                    edge: r.f64()?,
                    dispersion: r.f64()?,
                    bevel: r.f64()?,
                    transparency: r.f64()?,
                    twist: r.f64()?,
                },
            ));
        }
        let three_d = if self.version < 6 {
            None
        } else {
            match r.u8()? {
                THREE_D_NONE => None,
                THREE_D_SAME => Some(
                    self.previous
                        .as_ref()
                        .and_then(|previous| previous.three_d.clone())
                        .ok_or_else(|| corrupt("a frame repeats 3D content no frame had"))?,
                ),
                THREE_D_NEW => Some(r.bytes()?.to_vec()),
                _ => return Err(corrupt("invalid 3D content")),
            }
        };
        let elements = match r.u8()? {
            FRAME_FULL => {
                let count = r.len()?;
                let mut elements = Vec::with_capacity(count.min(1 << 20));
                for _ in 0..count {
                    elements.push(ElementRecord::read(r, self.version)?);
                }
                elements
            }
            FRAME_DELTA => {
                let previous = self
                    .previous
                    .as_ref()
                    .ok_or_else(|| corrupt("delta frame without a previous frame"))?;
                let mut by_key: HashMap<u32, ElementRecord> = previous
                    .elements
                    .iter()
                    .map(|element| (element.key, element.clone()))
                    .collect();
                let changed = r.len()?;
                let mut changed_order = Vec::with_capacity(changed.min(1 << 20));
                for _ in 0..changed {
                    let element = ElementRecord::read(r, self.version)?;
                    changed_order.push(element.key);
                    by_key.insert(element.key, element);
                }
                let order = if r.bool()? {
                    previous
                        .elements
                        .iter()
                        .map(|element| element.key)
                        .collect()
                } else {
                    read_keys(r)?
                };
                order
                    .into_iter()
                    .map(|key| {
                        by_key
                            .get(&key)
                            .cloned()
                            .ok_or_else(|| corrupt("frame references an unknown element"))
                    })
                    .collect::<Result<Vec<_>>>()?
            }
            _ => return Err(corrupt("invalid frame kind")),
        };
        let frame = FrameRecord {
            time,
            background_time,
            background_values,
            camera,
            transition,
            post,
            elements,
            effects,
            mattes,
            glasses,
            three_d,
            motion_blur: Vec::new(),
        };
        self.previous = Some(frame.clone());
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shifted perspective camera keeps its lens shift; a centered one is
    /// written byte for byte as before the shift existed.
    #[test]
    fn cameras_round_trip_with_their_lens_shift() {
        let mut camera = gaanim_math::Camera::perspective_3d(1280, 720, 0.8);
        let bytes = |camera: &gaanim_math::Camera| {
            let mut w = Writer::new();
            write_camera(&mut w, camera);
            w.into_bytes()
        };
        let centered = bytes(&camera);
        assert_eq!(centered[13 * 8], 1, "a centered lens keeps its tag");
        camera.lens_shift = gaanim_core::glam::DVec2::new(-3.5, 0.75);
        let shifted = bytes(&camera);
        assert_eq!(shifted.len(), centered.len() + 16);
        let read = read_camera(&mut Reader::new(&shifted)).unwrap();
        assert_eq!(read, camera);
        camera.lens_shift = gaanim_core::glam::DVec2::ZERO;
        assert_eq!(read_camera(&mut Reader::new(&centered)).unwrap(), camera);
    }

    /// A frame's 3D block keeps whether the world's up axis is z; a block
    /// of version 14, written before the flag, reads as y up.
    #[test]
    fn three_d_lighting_keeps_the_world_up_axis() {
        let block = |z_up: bool| {
            let three_d = CapturedThreeD {
                lighting: gaanim_scene::Lighting3D {
                    z_up,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut w = Writer::new();
            write_three_d(
                &mut w,
                &mut Tables::default(),
                &mut EntityKeys::default(),
                &three_d,
                &[],
            );
            w.into_bytes()
        };
        let read = |bytes: &[u8], version: u32| {
            let tables = DecodedTables::new(&[], version);
            read_three_d(&mut Reader::new(bytes), &tables)
                .unwrap()
                .0
                .lighting
        };
        assert!(read(&block(true), crate::VERSION).z_up);
        assert!(!read(&block(false), crate::VERSION).z_up);
        // Version 14 had no flag: drop it and the block still reads.
        let mut old = block(false);
        let flag = 1 + 4 + 1;
        old.remove(flag);
        assert!(!read(&old, 14).z_up);
    }

    fn triangle(indices: Vec<u32>) -> gaanim_scene::TriangleMeshData {
        gaanim_scene::TriangleMeshData {
            vertices: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            indices,
            normals: None,
            uvs: None,
            color: None,
            colors: None,
            material: None,
            surface: None,
            blend: None,
        }
    }

    /// A frame that stores the same mesh `Arc` as the frame before finds its
    /// index by address; a copy finds it by content, another mesh gets its
    /// own, and entries of meshes no frame holds any more are swept.
    #[test]
    fn meshes_are_stored_once_by_address_and_by_content() {
        let mut tables = Tables::default();
        let mesh = Arc::new(triangle(vec![0, 1, 2]));
        let first = tables.mesh(&mesh, 0);
        assert_eq!(tables.mesh_by_address.get(&mesh), Some(first));
        assert_eq!(tables.mesh(&mesh, 0), first);
        let copy = Arc::new((*mesh).clone());
        assert_eq!(tables.mesh_by_address.get(&copy), None);
        assert_eq!(tables.mesh(&copy, 0), first);
        assert_ne!(tables.mesh(&Arc::new(triangle(vec![0, 2, 1])), 0), first);
        assert_eq!(tables.meshes.len(), 2);
        for _ in 0..300 {
            tables.mesh(&Arc::new(triangle(vec![0, 1, 2])), 0);
        }
        assert!(tables.mesh_by_address.entries.len() < 150);
        assert_eq!(tables.mesh_by_address.get(&mesh), Some(first));
    }

    /// A sheet of `side` × `side` vertices bent by `angle`, with colors and
    /// normals that stay, as a parametric surface that changes every frame.
    fn sheet(side: u32, angle: f32) -> gaanim_scene::TriangleMeshData {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for row in 0..side {
            for column in 0..side {
                let (u, v) = (column as f32 / 7.0 - 3.0, row as f32 / 7.0 - 2.0);
                vertices.push([u * angle.cos(), v, u * angle.sin() + 0.3 * v * v]);
                if row + 1 < side && column + 1 < side {
                    let at = row * side + column;
                    indices.extend([at, at + 1, at + side, at + 1, at + side + 1, at + side]);
                }
            }
        }
        let count = vertices.len();
        gaanim_scene::TriangleMeshData {
            normals: Some(vec![[0.0, 0.6, 0.8]; count]),
            colors: Some(
                (0..count)
                    .map(|i| [i as f32 / count as f32, 0.5, 0.2, 1.0])
                    .collect(),
            ),
            vertices,
            indices,
            ..triangle(Vec::new())
        }
    }

    /// Write `meshes` and `lines` through tables, closing a segment every
    /// `per` frames, and decode them again; with the segments' size once
    /// compressed, as the bundle stores them.
    fn round_trip(
        mut tables: Tables,
        meshes: &[gaanim_scene::TriangleMeshData],
        lines: &[gaanim_scene::LineListData],
        per: usize,
    ) -> (
        Vec<Arc<gaanim_scene::TriangleMeshData>>,
        Vec<Arc<gaanim_scene::LineListData>>,
        usize,
    ) {
        let mut segments = Vec::new();
        let (mut mesh_indices, mut line_indices) = (Vec::new(), Vec::new());
        for frame in 0..meshes.len().max(lines.len()) {
            if let Some(mesh) = meshes.get(frame) {
                mesh_indices.push(tables.mesh(&Arc::new(mesh.clone()), 0));
            }
            if let Some(list) = lines.get(frame) {
                line_indices.push(tables.line_list(&Arc::new(list.clone()), 0));
            }
            if (frame + 1) % per == 0 {
                segments.extend(tables.close_segment());
            }
        }
        segments.extend(tables.close_segment());
        let size = segments
            .iter()
            .map(|(bytes, _)| crate::compress(bytes).unwrap().len())
            .sum();
        let counts: Vec<_> = segments.iter().map(|(_, counts)| *counts).collect();
        let mut decoded = DecodedTables::new(&counts, crate::VERSION);
        for (index, (bytes, _)) in segments.iter().enumerate() {
            decoded.load(index, bytes).unwrap();
        }
        let meshes = mesh_indices
            .iter()
            .map(|i| decoded.mesh(*i).unwrap())
            .collect();
        let lines = line_indices
            .iter()
            .map(|i| decoded.line_list(*i).unwrap())
            .collect();
        (meshes, lines, size)
    }

    /// The compressed size of `meshes` and `lines` each stored whole, in
    /// one segment.
    fn stored_whole(
        meshes: &[gaanim_scene::TriangleMeshData],
        lines: &[gaanim_scene::LineListData],
    ) -> usize {
        let mut w = Writer::new();
        for mesh in meshes {
            codec::write_mesh(
                &mut w,
                &mut Tables::default(),
                mesh,
                codec::Storage::default(),
            );
        }
        for list in lines {
            codec::write_lines(&mut w, list, codec::Storage::default());
        }
        crate::compress(&w.buf).unwrap().len()
    }

    fn curve(shift: f32) -> gaanim_scene::LineListData {
        gaanim_scene::LineListData {
            points: (0..200)
                .map(|i| {
                    let t = i as f32 / 20.0;
                    [t.cos() + shift, t.sin(), 0.1 * t * shift]
                })
                .collect(),
            indices: None,
            strip: true,
            color: peniko::Color::new([0.2, 0.4, 0.6, 1.0]),
            colors: None,
            revealed: None,
        }
    }

    /// The frames of a morph store the two meshes they blend once, and
    /// each frame as how far between them it lies, also in later
    /// segments; they decode exactly and keep the blend, for a repack.
    #[test]
    fn blended_meshes_store_their_ends_once_and_decode_exactly() {
        let (from, to) = (Arc::new(sheet(24, 0.0)), Arc::new(sheet(24, 1.2)));
        let frames: Vec<_> = (0..=30)
            .map(|k| {
                let blend = Arc::new(gaanim_scene::MeshBlend {
                    from: Arc::clone(&from),
                    to: Arc::clone(&to),
                    t: k as f32 / 30.0,
                });
                gaanim_scene::MeshBlend::mesh(blend).unwrap()
            })
            .collect();
        let (decoded, _, size) = round_trip(Tables::default(), &frames, &[], 7);
        for (frame, decoded) in frames.iter().zip(&decoded) {
            assert_eq!(frame.vertices, decoded.vertices);
            assert_eq!(frame.normals, decoded.normals);
            assert_eq!(frame.colors, decoded.colors);
            assert_eq!(
                decoded.blend.as_ref().map(|blend| blend.t),
                frame.blend.as_ref().map(|blend| blend.t)
            );
        }
        // Stored as their change from frame to frame, they take far more.
        let unblended: Vec<_> = frames
            .iter()
            .map(|frame| gaanim_scene::TriangleMeshData {
                blend: None,
                ..frame.clone()
            })
            .collect();
        let (_, _, changes) = round_trip(Tables::default(), &unblended, &[], 7);
        assert!(size * 4 < changes, "{size} against {changes}");
    }

    /// A mesh that bends from frame to frame, and a curve that moves, store
    /// their change from the frames before and decode bit for bit, in
    /// segments that each decode alone.
    #[test]
    fn meshes_and_lines_that_move_store_their_change_and_decode_exactly() {
        let meshes: Vec<_> = (0..40)
            .map(|frame| sheet(30, frame as f32 * 0.02))
            .collect();
        let lines: Vec<_> = (0..40).map(|frame| curve(frame as f32 * 0.01)).collect();
        let (decoded, decoded_lines, size) = round_trip(Tables::default(), &meshes, &lines, 16);
        for (mesh, back) in meshes.iter().zip(&decoded) {
            assert_eq!(back.vertices, mesh.vertices);
            assert_eq!(back.normals, mesh.normals);
            assert_eq!(back.indices, mesh.indices);
            assert_eq!(back.colors, mesh.colors);
        }
        for (list, back) in lines.iter().zip(&decoded_lines) {
            assert_eq!(back.points, list.points);
            assert_eq!((back.strip, back.color), (list.strip, list.color));
        }
        let whole = stored_whole(&meshes, &lines);
        assert!(size * 3 < whole * 2, "{size} of {whole}");
    }

    /// Packed for the web, positions and normals go on a grid within
    /// 2^-18 of the mesh's size, still predicted from frame to frame.
    #[test]
    fn web_bundles_store_meshes_on_a_grid() {
        let meshes: Vec<_> = (0..40)
            .map(|frame| sheet(30, frame as f32 * 0.02))
            .collect();
        let lines: Vec<_> = (0..40).map(|frame| curve(frame as f32 * 0.01)).collect();
        let web = Tables {
            grid: Some(HashMap::new()),
            ..Tables::default()
        };
        let (decoded, decoded_lines, size) = round_trip(web, &meshes, &lines, 16);
        let (_, _, exact) = round_trip(Tables::default(), &meshes, &lines, 16);
        assert!(size * 2 < exact, "{size} of {exact}");
        let close = |a: &[[f32; 3]], b: &[[f32; 3]], tolerance: f32| {
            a.iter()
                .zip(b)
                .all(|(a, b)| (0..3).all(|axis| (a[axis] - b[axis]).abs() <= tolerance))
        };
        for (mesh, back) in meshes.iter().zip(&decoded) {
            // The sheet is about 4.6 units across.
            assert!(close(&back.vertices, &mesh.vertices, 4.6 / 2_f32.powi(18)));
            assert_ne!(back.vertices, mesh.vertices);
            let normals = back.normals.as_ref().unwrap();
            assert!(close(
                normals,
                mesh.normals.as_ref().unwrap(),
                2_f32.powi(-17)
            ));
            assert_eq!(back.colors, mesh.colors);
        }
        for (list, back) in lines.iter().zip(&decoded_lines) {
            assert!(close(&back.points, &list.points, 2.5 / 2_f32.powi(18)));
        }
    }

    /// Two drawables whose items share a layout, such as a curve drawn on
    /// each side of a surface, each predict from their own.
    #[test]
    fn each_drawable_predicts_from_its_own_items() {
        let mut tables = Tables {
            grid: Some(HashMap::new()),
            ..Tables::default()
        };
        for frame in 0..6 {
            let shift = frame as f32 * 0.01;
            tables.line_list(&Arc::new(curve(shift)), 1);
            tables.line_list(&Arc::new(curve(shift + 3.0)), 2);
        }
        for (index, bytes) in tables.lines.iter().enumerate() {
            // Stored how, on which grid, and how many items back the base is.
            let mut r = Reader::new(bytes);
            let predicted = r.u8().unwrap() == 1;
            r.option(|r| r.ivar()).unwrap();
            assert_eq!(predicted, index >= 2, "item {index}");
            if predicted {
                assert_eq!(r.var().unwrap(), 2, "item {index}");
            }
        }
    }

    /// A mesh whose layout changes, such as a sheet that gains vertices,
    /// is stored whole and decodes as it was.
    #[test]
    fn meshes_of_another_layout_are_stored_whole() {
        let meshes = [
            sheet(10, 0.1),
            sheet(10, 0.2),
            sheet(12, 0.3),
            sheet(12, 0.4),
        ];
        let mut tables = Tables::default();
        for mesh in &meshes {
            tables.mesh(&Arc::new(mesh.clone()), 0);
        }
        let stored: Vec<u8> = tables.meshes.iter().map(|bytes| bytes[0]).collect();
        assert_eq!(stored, [0, 1, 0, 1]);
        let (decoded, _, _) = round_trip(Tables::default(), &meshes, &[], 64);
        for (mesh, back) in meshes.iter().zip(&decoded) {
            assert_eq!(back.vertices, mesh.vertices);
            assert_eq!(back.indices, mesh.indices);
        }
    }
}
