//! What a bundle stores: static scene data, deduplicated tables and frames.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::Entity;
use gaanim_core::{kurbo, peniko};
use gaanim_renderer::effects::{
    CameraViewBackground, ClipMask, DropShadow, GaussianBlur, Glow, StrokeAlign, StrokeProfile,
};
use gaanim_renderer::fragment::{FragmentRecipe, GroupShadow};
use gaanim_renderer::pipeline::{CapturedElement, CapturedTransition, CapturedView, FrameCapture};
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
}

impl EntityKeys {
    pub fn key(&mut self, entity: Entity) -> u32 {
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

/// Deduplicated paths, images, recipes and strings built while recording.
#[derive(Default)]
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
}

impl Interner for Tables {
    fn path(&mut self, path: &Arc<kurbo::BezPath>) -> u32 {
        let ptr = Arc::as_ptr(path) as usize;
        if let Some(index) = self.path_by_ptr.get(&ptr) {
            return *index;
        }
        let mut w = Writer::new();
        codec::write_path(&mut w, path);
        let hash = blake3::hash(&w.buf);
        let index = *self.path_by_hash.entry(hash).or_insert_with(|| {
            self.paths.push(Arc::clone(path));
            (self.paths.len() - 1) as u32
        });
        // Keep the pointer key valid: the table holds the path alive.
        if Arc::ptr_eq(&self.paths[index as usize], path) {
            self.path_by_ptr.insert(ptr, index);
        }
        index
    }

    fn image(&mut self, image: &peniko::ImageData) -> u32 {
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
            (self.images.len() - 1) as u32
        });
        // The table keeps the blob alive, so its id stays unique.
        self.image_by_id.insert(id, index);
        index
    }
}

impl Tables {
    pub fn transition_shader(&mut self, shader: &Arc<TransitionShader>) -> u32 {
        let index = self
            .transition_shaders
            .iter()
            .position(|known| Arc::ptr_eq(known, shader) || **known == **shader)
            .unwrap_or_else(|| {
                self.transition_shaders.push(Arc::clone(shader));
                self.transition_shaders.len() - 1
            });
        index as u32
    }

    pub fn string(&mut self, value: &Arc<str>) -> u32 {
        if let Some(index) = self.string_index.get(value) {
            return *index;
        }
        self.strings.push(Arc::clone(value));
        let index = (self.strings.len() - 1) as u32;
        self.string_index.insert(Arc::clone(value), index);
        index
    }

    pub fn recipe(&mut self, recipe: &Arc<FragmentRecipe>) -> u32 {
        let ptr = Arc::as_ptr(recipe) as usize;
        if let Some((_, index)) = self.recipe_by_ptr.get(&ptr) {
            return *index;
        }
        let mut w = Writer::new();
        write_recipe(&mut w, self, recipe);
        let hash = blake3::hash(&w.buf);
        let bytes = w.into_bytes();
        let index = *self.recipe_by_hash.entry(hash).or_insert_with(|| {
            self.recipes.push(bytes);
            (self.recipes.len() - 1) as u32
        });
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

/// Decoded tables of an open bundle.
pub struct DecodedTables {
    pub paths: Vec<Arc<kurbo::BezPath>>,
    pub images: Vec<peniko::ImageData>,
    pub recipes: Vec<Arc<FragmentRecipe>>,
    pub strings: Vec<Arc<str>>,
    /// Shared by every frame of one transition, so its shader is built once.
    pub transition_shaders: Vec<Arc<TransitionShader>>,
}

impl Resolver for DecodedTables {
    fn path(&self, index: u32) -> Result<Arc<kurbo::BezPath>> {
        self.paths
            .get(index as usize)
            .cloned()
            .ok_or_else(|| corrupt("path index out of range"))
    }

    fn image(&self, index: u32) -> Result<peniko::ImageData> {
        self.images
            .get(index as usize)
            .cloned()
            .ok_or_else(|| corrupt("image index out of range"))
    }
}

impl DecodedTables {
    fn recipe(&self, index: u32) -> Result<Arc<FragmentRecipe>> {
        self.recipes
            .get(index as usize)
            .cloned()
            .ok_or_else(|| corrupt("recipe index out of range"))
    }

    fn string(&self, index: u32) -> Result<Arc<str>> {
        self.strings
            .get(index as usize)
            .cloned()
            .ok_or_else(|| corrupt("string index out of range"))
    }
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
        // Bundle format 2 refuses to record chalk.
        chalk: None,
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
        gaanim_math::Projection::Perspective { fov_y, near, far } => {
            w.u8(1);
            w.f64(fov_y);
            w.f64(near);
            w.f64(far);
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
    let projection = match r.u8()? {
        0 => gaanim_math::Projection::Orthographic { zoom: r.f64()? },
        1 => gaanim_math::Projection::Perspective {
            fov_y: r.f64()?,
            near: r.f64()?,
            far: r.f64()?,
        },
        _ => return Err(corrupt("invalid projection")),
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
    });
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
        let index = r.u32()? as usize;
        let shader = tables
            .transition_shaders
            .get(index)
            .cloned()
            .ok_or_else(|| corrupt("transition shader index out of range"))?;
        Ok(TransitionShaderFrame {
            shader,
            progress: r.f32()?,
        })
    })?;
    Ok(CapturedTransition {
        outgoing_mask,
        incoming_mask,
        backgrounds,
        overlays,
        shader,
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
        Self {
            key: key_of(element.entity),
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
    }

    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
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
    pub camera: gaanim_math::Camera,
    /// Encoded [`CapturedTransition`], compared as bytes.
    pub transition: Option<Vec<u8>>,
    pub post: Vec<PostPass>,
    pub elements: Vec<ElementRecord>,
    /// Motion blur sub-frames, each without sub-frames of its own.
    pub motion_blur: Vec<FrameRecord>,
}

impl FrameRecord {
    pub fn capture(
        frame: &Frame,
        tables: &mut Tables,
        keys: &mut EntityKeys,
        lottie: &dyn Fn(&CapturedElement) -> Option<u32>,
    ) -> Self {
        Self {
            time: frame.time,
            background_time: frame.capture.background_time,
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
        let elements = self
            .elements
            .iter()
            .map(|element| {
                let scene = element
                    .lottie
                    .map(&mut *lottie_scene)
                    .transpose()?
                    .flatten();
                element.resolve(tables, scene)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Frame {
            time: self.time,
            camera: self.camera,
            capture: FrameCapture {
                background_time: self.background_time,
                elements,
                transition: self
                    .transition
                    .as_ref()
                    .map(|bytes| read_transition(&mut Reader::new(bytes), tables))
                    .transpose()?,
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

/// Encodes a run of frames, each against the one before it. The first frame
/// of a chunk is written whole so a chunk decodes on its own.
#[derive(Default)]
pub(crate) struct DeltaEncoder {
    previous: Option<FrameRecord>,
}

const FRAME_FULL: u8 = 0;
const FRAME_DELTA: u8 = 1;

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
        write_camera(w, &frame.camera);
        w.option(frame.transition.as_ref(), |w, bytes| w.bytes(bytes));
        w.len(frame.post.len());
        for pass in &frame.post {
            w.var(u64::from(pass.shader));
            w.len(pass.values.len());
            for value in &pass.values {
                w.f32(*value);
            }
        }
        match &self.previous {
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

#[derive(Default)]
pub(crate) struct DeltaDecoder {
    previous: Option<FrameRecord>,
}

impl DeltaDecoder {
    pub fn read(&mut self, r: &mut Reader<'_>) -> Result<FrameRecord> {
        let mut frame = self.read_frame(r)?;
        let count = r.len()?;
        let mut sub = DeltaDecoder {
            previous: self.previous.clone(),
        };
        frame.motion_blur = (0..count)
            .map(|_| sub.read_frame(r))
            .collect::<Result<Vec<_>>>()?;
        Ok(frame)
    }

    fn read_frame(&mut self, r: &mut Reader<'_>) -> Result<FrameRecord> {
        let time = r.f64()?;
        let background_time = r.f64()?;
        let camera = read_camera(r)?;
        let transition = r.option(|r| Ok(r.bytes()?.to_vec()))?;
        let pass_count = r.len()?;
        let mut post = Vec::with_capacity(pass_count.min(64));
        for _ in 0..pass_count {
            let shader = r.u32()?;
            let count = r.len()?;
            let mut values = Vec::with_capacity(count.min(256));
            for _ in 0..count {
                values.push(r.f32()?);
            }
            post.push(PostPass { shader, values });
        }
        let elements = match r.u8()? {
            FRAME_FULL => {
                let count = r.len()?;
                let mut elements = Vec::with_capacity(count.min(1 << 20));
                for _ in 0..count {
                    elements.push(ElementRecord::read(r)?);
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
                    let element = ElementRecord::read(r)?;
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
            camera,
            transition,
            post,
            elements,
            motion_blur: Vec::new(),
        };
        self.previous = Some(frame.clone());
        Ok(frame)
    }
}

pub(crate) fn decode_recipes(encoded: &[Vec<u8>], tables: &mut DecodedTables) -> Result<()> {
    for bytes in encoded {
        let recipe = read_recipe(&mut Reader::new(bytes), tables)?;
        tables.recipes.push(Arc::new(recipe));
    }
    Ok(())
}
