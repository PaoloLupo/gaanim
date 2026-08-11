//! Canvas — the top-level facade for building Gaanim animations.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use bevy::prelude::*;
use gaanim_core::glam::{DVec2, DVec3};
use gaanim_core::kurbo::Shape;
use gaanim_core::peniko::Color;
use gaanim_objects::prelude::{GltfDocument, GltfLoadError, GltfSceneSelector, SvgLoadError};
use gaanim_objects::primitives3d;
use gaanim_timeline::transition::TransitionType;

use crate::anim::{AnimationBuilder, AnimationType};
use crate::canvas::drawable::DrawableHandle;
use crate::canvas::ops::{
    CanvasEndpoint, CanvasState, LocalSegmentStop, Op, Segment, SharedCanvasState,
};
use crate::canvas::types::{
    Anim, CanvasUnits, ImageOptions, ImageOptionsError, LayoutMemberSpec, LayoutSpec,
    LayoutTreeSnapshot, Margin, ReactiveReadoutLayoutSpec, SpawnKind,
};
use crate::canvas::{
    Anchor, CanvasTheme, PresentationBrand, SegmentError, SegmentHandle, SegmentManifest,
    SegmentSpec, SegmentStop,
};
use crate::export::{AudioTrack, AudioTrackError};

/// Error returned when selecting a built-in visual theme by name.
#[derive(Debug, thiserror::Error)]
#[error("unknown theme '{name}'; use CanvasTheme::BUILTIN_NAMES for the available color schemes")]
pub struct ThemeError {
    pub name: String,
}

/// Failures while decoding a raster image requested by `Canvas::image`.
#[derive(Debug, thiserror::Error)]
pub enum ImageLoadError {
    #[error("could not load image '{path}': {source}")]
    Load {
        path: PathBuf,
        #[source]
        source: image::ImageError,
    },
    #[error(transparent)]
    Options(#[from] ImageOptionsError),
}

#[derive(Debug, thiserror::Error)]
pub enum AssetRootError {
    #[error("asset directory '{path}' does not exist or is not a directory")]
    Invalid { path: PathBuf },
    #[error("could not resolve asset directory '{path}': {source}")]
    Resolve {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Error returned when a scene-lifetime operation receives an incompatible drawable.
#[derive(Debug, thiserror::Error)]
pub enum SceneObjectError {
    #[error("at least one drawable is required")]
    NoObjects,
    #[error("drawable belongs to a different Scene")]
    ForeignScene,
    #[error(transparent)]
    Layout(#[from] gaanim_layout::LayoutError),
}

#[derive(Clone, Copy)]
enum SceneObjectAction {
    Reuse,
    Persist,
    Release,
}

#[derive(Debug, thiserror::Error)]
pub enum AssetPreloadError {
    #[error("could not preload image '{path}': {source}")]
    Image {
        path: PathBuf,
        #[source]
        source: ImageLoadError,
    },
    #[error("could not preload SVG '{path}': {source}")]
    Svg {
        path: PathBuf,
        #[source]
        source: SvgLoadError,
    },
    #[error("could not preload glTF '{path}': {source}")]
    Gltf {
        path: PathBuf,
        #[source]
        source: GltfLoadError,
    },
}

/// Process-local decoded texture cache. Each canvas still receives its own
/// mobject, while repeated references to the same canonical path share the
/// immutable RGBA data used by Vello.
static IMAGE_CACHE: OnceLock<Mutex<HashMap<PathBuf, gaanim_core::peniko::ImageData>>> =
    OnceLock::new();

fn load_image(path: impl AsRef<Path>) -> Result<gaanim_core::peniko::ImageData, ImageLoadError> {
    let requested = path.as_ref();
    let cache_key = requested
        .canonicalize()
        .unwrap_or_else(|_| requested.to_path_buf());
    let cache = IMAGE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(image) = cache
        .lock()
        .expect("image cache poisoned")
        .get(&cache_key)
        .cloned()
    {
        return Ok(image);
    }

    let decoded = image::open(&cache_key).map_err(|source| ImageLoadError::Load {
        path: requested.to_path_buf(),
        source,
    })?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    let image = gaanim_core::peniko::ImageData {
        data: gaanim_core::peniko::Blob::from(rgba.into_raw()),
        format: gaanim_core::peniko::ImageFormat::Rgba8,
        alpha_type: gaanim_core::peniko::ImageAlphaType::Alpha,
        width,
        height,
    };
    let mut cache = cache.lock().expect("image cache poisoned");
    Ok(cache.entry(cache_key).or_insert(image).clone())
}

/// Top-level facade for building Gaanim animations.
#[derive(Debug, Clone)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub background: Option<Color>,
    pub(crate) background_overridden: bool,
    pub units: CanvasUnits,
    /// Canonical name of the selected theme.
    pub theme: Option<String>,
    /// Complete semantic colors and typography for the selected theme.
    pub theme_style: Option<CanvasTheme>,
    pub margin: Margin,
    pub asset_root: Option<PathBuf>,
    /// Audio sources mixed by FFmpeg when this canvas is exported.
    pub audio_tracks: Vec<AudioTrack>,
    /// Reusable logo/footer treatment generated for every explicit segment.
    pub branding: Option<PresentationBrand>,
    pub(crate) camera_position: gaanim_core::glam::DVec3,
    pub(crate) camera_zoom: f64,
    pub(crate) camera_rotation: gaanim_core::glam::DQuat,
    pub(crate) lighting_3d: gaanim_scene::Lighting3D,
    pub(crate) state: SharedCanvasState,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            background: None,
            background_overridden: false,
            theme: None,
            theme_style: None,
            units: CanvasUnits::Pixels,
            margin: Margin::default(),
            asset_root: None,
            audio_tracks: Vec::new(),
            branding: None,
            camera_position: gaanim_core::glam::DVec3::ZERO,
            camera_zoom: 1.0,
            camera_rotation: gaanim_core::glam::DQuat::IDENTITY,
            lighting_3d: gaanim_scene::Lighting3D::default(),
            state: Arc::new(Mutex::new(CanvasState::new())),
        }
    }

    /// Whether a handle was created by this exact Scene, independent of its
    /// numeric object ID.
    pub fn owns_drawable(&self, drawable: &DrawableHandle) -> bool {
        Arc::ptr_eq(&self.state, &drawable.state)
    }

    /// Whether replay requires Bevy's native 3D render graph.
    pub fn has_native_3d_content(&self) -> bool {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .iter()
            .flat_map(|segment| segment.ops.iter())
            .any(|op| {
                matches!(
                    op,
                    Op::Spawn(spec)
                        if matches!(
                            spec.lock().expect("object spec poisoned").kind,
                            SpawnKind::GltfModel { .. }
                                | SpawnKind::Axes3D { .. }
                                | SpawnKind::Primitive3D(..)
                                | SpawnKind::SurfaceMesh { .. }
                                | SpawnKind::Polyline3D { .. }
                                | SpawnKind::LineSegments3D { .. }
                                | SpawnKind::TracedPath3DLine
                        )
                )
            })
    }

    pub fn background(mut self, c: Color) -> Self {
        self.background = Some(c);
        self.background_overridden = true;
        self
    }

    pub fn set_background(&mut self, color: Option<Color>) {
        self.background = color;
        self.background_overridden = true;
    }

    /// Apply one of the built-in visual themes.
    ///
    /// `technical` is the quiet dark style used by the built-in technical
    /// components. `presentation` adds a warmer, higher-contrast hierarchy for
    /// projected slides. `paper` provides a light documentation canvas.
    /// Calling this method also selects the theme background; callers can
    /// still override [`Canvas::background`] afterwards.
    pub fn set_theme(&mut self, name: &str) -> Result<(), ThemeError> {
        self.apply_theme(CanvasTheme::builtin(name)?);
        Ok(())
    }

    /// Apply a complete custom or derived visual theme.
    pub fn apply_theme(&mut self, theme: CanvasTheme) {
        if !self.background_overridden {
            self.background = Some(theme.palette.background);
        }
        self.theme = Some(theme.name.clone());
        self.theme_style = Some(theme);
    }

    /// Resolve a semantic token from the active theme.
    pub fn theme_color(&self, role: &str) -> Result<Color, String> {
        self.theme_style
            .as_ref()
            .ok_or_else(|| "no theme is active on this canvas".to_string())?
            .color(role)
    }

    /// Resolve a spacing/layout token. Scenes without an active theme use the
    /// canonical default token scale so templates remain deterministic.
    pub fn theme_layout_token(&self, name: &str) -> Result<f64, String> {
        self.theme_style
            .as_ref()
            .map(|theme| theme.layout.clone())
            .unwrap_or_default()
            .get(name)
    }

    /// Validate the active theme for projected-text readability.
    pub fn validate_theme(&self) -> Result<Vec<String>, String> {
        Ok(self
            .theme_style
            .as_ref()
            .ok_or_else(|| "no theme is active on this canvas".to_string())?
            .validate())
    }

    pub(crate) fn themed_text_config(&self) -> gaanim_text::prelude::TextConfig {
        let mut config = self
            .theme_style
            .as_ref()
            .map(|theme| theme.text.clone())
            .unwrap_or_default();
        if let Some(theme) = &self.theme_style {
            for (role, overlay) in &theme.text_styles {
                if let Some(style) = config.roles.get_mut(role) {
                    if let Some(font) = &overlay.font {
                        style.font_family = font.clone();
                    }
                    if let Some(size) = overlay.size {
                        style.size = size;
                    }
                    if let Some(color) = overlay.color {
                        style.fill_color = color;
                    }
                }
            }
        }
        config
    }

    pub(crate) fn register_theme_fonts(&self, registry: &mut gaanim_text::font::FontRegistry) {
        if let Some(theme) = &self.theme_style {
            for font in &theme.fonts {
                registry.register_font(font.family.clone(), font.bytes.to_vec());
            }
        }
    }

    pub fn with_units(mut self, u: CanvasUnits) -> Self {
        self.units = u;
        self
    }

    /// Set uniform margin on all four sides.
    /// Layout operations (`to_edge`, `to_corner`) will respect this inset.
    pub fn margin_all(mut self, v: f64) -> Self {
        self.margin = Margin::all(v);
        self
    }

    /// Set per-side margins.
    pub fn margin(mut self, m: Margin) -> Self {
        self.margin = m;
        self
    }

    /// Set the base directory used by relative image and SVG paths.
    pub fn set_asset_root(&mut self, path: impl AsRef<Path>) -> Result<(), AssetRootError> {
        let path = path.as_ref().to_path_buf();
        if !path.is_dir() {
            return Err(AssetRootError::Invalid { path });
        }
        self.asset_root = Some(
            path.canonicalize()
                .map_err(|source| AssetRootError::Resolve {
                    path: path.clone(),
                    source,
                })?,
        );
        Ok(())
    }

    fn resolve_asset_path(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();
        if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(root) = &self.asset_root {
            root.join(path)
        } else {
            path.to_path_buf()
        }
    }

    /// Add an audio source at an absolute scene time, or at the current cursor
    /// when `start_time` is omitted. Audio is mixed and muxed into MP4/WebM
    /// exports; preview playback remains visual-only for now.
    pub fn audio(
        &mut self,
        path: impl AsRef<Path>,
        start_time: Option<f64>,
        duration: Option<f64>,
        volume: f64,
        fade_in: f64,
        fade_out: f64,
    ) -> Result<(), AudioTrackError> {
        let start_time = start_time.unwrap_or_else(|| {
            self.state
                .lock()
                .expect("canvas state poisoned")
                .active()
                .cursor
        });
        let track = AudioTrack::new(
            self.resolve_asset_path(path),
            start_time,
            duration,
            volume,
            fade_in,
            fade_out,
        )?;
        self.audio_tracks.push(track);
        Ok(())
    }

    /// Resolve and validate assets before playback. Raster images are also
    /// decoded into the process-local image cache used by [`Self::image`].
    pub fn preload(&self, paths: &[PathBuf]) -> Result<(), AssetPreloadError> {
        for path in paths {
            let resolved = self.resolve_asset_path(path);
            let extension = resolved
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or_default();
            if extension.eq_ignore_ascii_case("svg") {
                gaanim_objects::prelude::SvgDocument::load(&resolved).map_err(|source| {
                    AssetPreloadError::Svg {
                        path: resolved.clone(),
                        source,
                    }
                })?;
            } else if extension.eq_ignore_ascii_case("gltf")
                || extension.eq_ignore_ascii_case("glb")
            {
                GltfDocument::load(&resolved, &GltfSceneSelector::Default).map_err(|source| {
                    AssetPreloadError::Gltf {
                        path: resolved.clone(),
                        source,
                    }
                })?;
            } else {
                load_image(&resolved).map_err(|source| AssetPreloadError::Image {
                    path: resolved.clone(),
                    source,
                })?;
            }
        }
        Ok(())
    }

    /// Drop decoded raster assets so the next `image`/`preload` observes files
    /// changed on disk. SVG documents are resolved anew for every drawable.
    pub fn reload_assets(&mut self) {
        if let Some(cache) = IMAGE_CACHE.get() {
            cache.lock().expect("image cache poisoned").clear();
        }
        gaanim_objects::prelude::clear_gltf_cache();
    }

    pub(crate) fn safe_frame(&self) -> gaanim_math::Bounds3D {
        let raw = self.units.frame_bounds(self.width, self.height);
        gaanim_math::Bounds3D::new_2d(
            raw.min.x + self.margin.left,
            raw.min.y + self.margin.bottom,
            raw.max.x - self.margin.right,
            raw.max.y - self.margin.top,
        )
    }

    pub fn current_time(&self) -> f64 {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .iter()
            .map(|segment| segment.cursor)
            .sum()
    }

    pub fn segment_count(&self) -> usize {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .len()
    }

    pub(crate) fn spawn(&mut self, kind: SpawnKind) -> DrawableHandle {
        self.spawn_registered(kind, true)
    }

    fn spawn_registered(&mut self, kind: SpawnKind, register_top_level: bool) -> DrawableHandle {
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let id = guard.next_object_id();
        let active_idx = guard.active_idx;
        if register_top_level {
            guard.active_mut().mobject_ids.push(id);
            guard.all_drawables.push(id);
        }
        drop(guard);

        let handle = DrawableHandle::new(id, kind, self.state.clone(), active_idx);
        self.state.lock().expect("canvas state poisoned").segments[active_idx]
            .ops
            .push(Op::Spawn(handle.spec.clone()));
        handle
    }

    // -- Segment management --

    /// Create a named segment and switch to it.
    ///
    /// The first explicit segment replaces the untouched implicit segment. A
    /// transition therefore requires an existing authored predecessor.
    pub fn segment(
        &mut self,
        name: impl Into<String>,
        transition: Option<TransitionType>,
    ) -> Result<SegmentHandle, SegmentError> {
        self.segment_with(name, transition, None, None)
    }

    /// Create a segment with presentation metadata and optional Python
    /// template metadata. Template execution belongs to the Python layer.
    pub fn segment_with(
        &mut self,
        name: impl Into<String>,
        transition: Option<TransitionType>,
        notes: Option<String>,
        template: Option<String>,
    ) -> Result<SegmentHandle, SegmentError> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err(SegmentError::EmptyName);
        }
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let normalized_name = name.to_lowercase();
        if guard
            .segments
            .iter()
            .any(|segment| segment.explicit && segment.name.to_lowercase() == normalized_name)
        {
            return Err(SegmentError::DuplicateName { name });
        }

        let replace_implicit = guard.segments.len() == 1
            && guard.active_idx == 0
            && guard.segments[0].is_untouched_implicit();
        if replace_implicit && transition.is_some() {
            return Err(SegmentError::FirstTransition);
        }

        let id = guard.next_segment_id();
        let mut segment = Segment::new(id, name, notes, template.clone());
        if replace_implicit {
            guard.segments[0] = segment;
            guard.active_idx = 0;
        } else {
            let previous = guard.active_idx;
            segment.transition = transition;
            segment.prev_segment = Some(previous);
            let index = guard.segments.len();
            guard.segments.push(segment);
            guard.active_idx = index;
        }
        let segment_number = guard
            .segments
            .iter()
            .filter(|segment| segment.explicit)
            .count();
        drop(guard);

        self.spawn_segment_branding(template.as_deref(), segment_number)?;
        Ok(SegmentHandle::new(id, self.state.clone()))
    }

    /// Explicitly link two segments created by this canvas.
    pub fn link(
        &mut self,
        from: &SegmentHandle,
        to: &SegmentHandle,
        transition: TransitionType,
    ) -> Result<(), SegmentError> {
        if !from.belongs_to(&self.state) || !to.belongs_to(&self.state) {
            return Err(SegmentError::ForeignSegment);
        }
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let from_index = guard
            .segments
            .iter()
            .position(|segment| segment.id == from.id())
            .ok_or(SegmentError::UnknownSegment { id: from.id() })?;
        let to_index = guard
            .segments
            .iter()
            .position(|segment| segment.id == to.id())
            .ok_or(SegmentError::UnknownSegment { id: to.id() })?;
        if from_index >= to_index {
            return Err(SegmentError::InvalidLink);
        }
        guard.segments[to_index].transition = Some(transition);
        guard.segments[to_index].prev_segment = Some(from_index);
        Ok(())
    }

    // -- Object factories --

    pub fn circle(&mut self, r: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Circle(r))
    }
    pub fn rect(&mut self, w: f64, h: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Rect(w, h))
    }
    pub fn rounded_rect(&mut self, w: f64, h: f64, r: f64) -> DrawableHandle {
        self.spawn(SpawnKind::RoundedRect(w, h, r))
    }
    pub fn square(&mut self, s: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Square(s))
    }
    pub fn dot(&mut self, r: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Dot(r))
    }
    pub fn ellipse(&mut self, rx: f64, ry: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Ellipse(rx, ry))
    }
    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Line(x1, y1, x2, y2))
    }
    pub fn arrow(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Arrow(x1, y1, x2, y2))
    }
    pub fn dashed_line(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        dash_length: f64,
        gap_length: f64,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::DashedLine {
            start: (x1, y1),
            end: (x2, y2),
            dash_length,
            gap_length,
        })
    }
    pub fn double_arrow(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        head_length: Option<f64>,
        head_width: Option<f64>,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::DoubleArrow {
            start: (x1, y1),
            end: (x2, y2),
            head_length,
            head_width,
        })
    }
    pub fn polygon(&mut self, points: Vec<(f64, f64)>) -> DrawableHandle {
        self.spawn(SpawnKind::Polygon(points))
    }
    pub fn star(&mut self, points: u32, outer_radius: f64, inner_radius: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Star {
            points,
            outer_radius,
            inner_radius,
        })
    }
    pub fn regular_polygon(&mut self, sides: u32, radius: f64) -> DrawableHandle {
        self.spawn(SpawnKind::RegularPolygon { sides, radius })
    }
    pub fn sector(
        &mut self,
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::Sector {
            center: (cx, cy),
            radius,
            start_angle,
            sweep_angle,
        })
    }
    pub fn annulus(&mut self, outer_radius: f64, inner_radius: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Annulus {
            outer_radius,
            inner_radius,
        })
    }
    pub fn brace(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, height: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Brace {
            start: (x1, y1),
            end: (x2, y2),
            height,
        })
    }
    pub fn checkmark(&mut self, size: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Checkmark(size))
    }
    pub fn cross(&mut self, size: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Cross(size))
    }
    pub fn right_angle(&mut self, arm_length: f64) -> DrawableHandle {
        self.spawn(SpawnKind::RightAngle(arm_length))
    }
    /// Creates an open circular arc. Angles are expressed in radians.
    pub fn arc(
        &mut self,
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::Arc {
            center: (cx, cy),
            radius,
            start_angle,
            sweep_angle,
        })
    }
    /// Creates a curved arrow between two points.
    pub fn curved_arrow(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        angle: f64,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::CurvedArrow(x1, y1, x2, y2, angle))
    }
    /// Creates a curved arrow along an explicit circular arc. Angles are in radians.
    pub fn curved_arrow_arc(
        &mut self,
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::CurvedArrowArc {
            center: (cx, cy),
            radius,
            start_angle,
            sweep_angle,
        })
    }
    /// Creates a dimension line offset perpendicularly from the measured segment.
    pub fn dimension(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, offset: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Dimension {
            start: (x1, y1),
            end: (x2, y2),
            offset,
        })
    }
    /// Creates an open path connecting the given points in order.
    ///
    /// Use this for technical geometry such as springs, rails, or trajectories.
    pub fn polyline(&mut self, points: &[(f64, f64)]) -> DrawableHandle {
        self.spawn(SpawnKind::Polyline(points.to_vec()))
    }

    /// Create a native quadratic or cubic Bézier path.
    pub fn bezier(
        &mut self,
        start: (f64, f64),
        controls: Vec<(f64, f64)>,
        end: (f64, f64),
    ) -> DrawableHandle {
        self.spawn(SpawnKind::Bezier {
            start,
            controls,
            end,
        })
    }

    /// Create a composed native curve from cursor commands.
    ///
    /// Commands may move the cursor, add line or Bézier segments, and close a
    /// subpath. Points on a relative command are offsets from the current
    /// cursor. `CurveControl::Auto` mirrors the preceding matching control
    /// point; `CurveControl::None` collapses that handle onto the endpoint.
    pub fn curve(&mut self, elements: Vec<crate::canvas::CurveElement>) -> DrawableHandle {
        self.spawn(SpawnKind::Curve(elements))
    }
    /// Creates Cartesian axes — manim `Axes` compatible.
    ///
    /// Mirrors `manim.mobject.graphing.coordinate_systems.Axes`:
    /// `x_range`/`y_range` as `(min, max, step)`, `x_length`/`y_length` control
    /// scene size (like Manim), `tips` adds arrowheads, `axis_config` dicts are
    /// accepted for compatibility (mapped to `axis_color`/`include_numbers` etc).
    /// `auto_fit=true` (default) scales data to `safe_frame` (gaanim layout idiom);
    /// set `auto_fit=false` or explicit `x_length`/`y_length` for Manim-like fixed size.
    ///
    /// The returned `DrawableHandle` is animable: `axes.create()` draws
    /// sequentially `Grid → Axes → Ticks → Numbers/Labels` via `PathCompletion`
    /// leaves (`crates/gaanim_api/src/builder.rs` expansion). Use
    /// `scene.plot(axes, f, x_range)` or `axes.coords_to_point(x,y)` /
    /// `axes.point_to_coords(p)` (manim `coords_to_point`/`point_to_coords`),
    /// `scene.plot_parametric_curve(axes, f, t_range)`, and
    /// `axes.get_x_axis()`/`get_y_axis()` for compatibility.
    pub fn axes(
        &mut self,
        x_range: (f64, f64, f64),
        y_range: (f64, f64, f64),
        config: crate::canvas::AxesConfig,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::Axes {
            x_range,
            y_range,
            config,
        })
    }

    /// Creates 3D Cartesian axes with three grid planes and perspective support.
    pub fn axes_3d(
        &mut self,
        x_range: (f64, f64, f64),
        y_range: (f64, f64, f64),
        z_range: (f64, f64, f64),
        config: crate::canvas::types::Axes3DConfig,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::Axes3D {
            x_range,
            y_range,
            z_range,
            config,
        })
    }

    /// Creates a triangulated 3D surface mesh in world space.
    /// Vertices are expected in world coordinates (use `axes_3d` scale to compute).
    pub fn surface_mesh(
        &mut self,
        vertices: Vec<[f32; 3]>,
        indices: Vec<u32>,
        color: Option<Color>,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::SurfaceMesh {
            vertices,
            indices,
            color,
            colors: None,
        })
    }

    pub fn cube(
        &mut self,
        size: f64,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        Ok(self.spawn(SpawnKind::Primitive3D(primitives3d::cube(size, material)?)))
    }

    pub fn sphere(
        &mut self,
        radius: f64,
        segments: u32,
        rings: u32,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        Ok(self.spawn(SpawnKind::Primitive3D(primitives3d::sphere(
            radius, segments, rings, material,
        )?)))
    }

    pub fn cylinder(
        &mut self,
        radius: f64,
        height: f64,
        segments: u32,
        caps: bool,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        Ok(self.spawn(SpawnKind::Primitive3D(primitives3d::cylinder(
            radius, height, segments, caps, material,
        )?)))
    }

    pub fn cone(
        &mut self,
        radius: f64,
        height: f64,
        segments: u32,
        cap: bool,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        Ok(self.spawn(SpawnKind::Primitive3D(primitives3d::cone(
            radius, height, segments, cap, material,
        )?)))
    }

    pub fn plane(
        &mut self,
        width: f64,
        height: f64,
        subdivisions: (u32, u32),
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        Ok(self.spawn(SpawnKind::Primitive3D(primitives3d::plane(
            width,
            height,
            subdivisions,
            material,
        )?)))
    }

    pub fn lighting_3d(&mut self, enabled: bool, intensity: f32, shadows: bool) {
        self.lighting_3d = gaanim_scene::Lighting3D {
            enabled,
            intensity,
            shadows,
        };
    }

    pub fn surface_mesh_with_colors(
        &mut self,
        vertices: Vec<[f32; 3]>,
        indices: Vec<u32>,
        colors: Vec<Color>,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::SurfaceMesh {
            vertices,
            indices,
            color: None,
            colors: Some(colors),
        })
    }

    /// Creates a 3D polyline from world-space points.
    pub fn polyline_3d(&mut self, points: Vec<[f32; 3]>) -> DrawableHandle {
        self.spawn(SpawnKind::Polyline3D {
            points,
            colors: None,
        })
    }

    /// Creates a 3D polyline with per-vertex colors (e.g. colormap).
    /// `colors` must have the same length as `points`; otherwise a uniform color fallback is used.
    pub fn polyline_3d_with_colors(
        &mut self,
        points: Vec<[f32; 3]>,
        colors: Vec<Color>,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::Polyline3D {
            points,
            colors: Some(colors),
        })
    }

    pub(crate) fn line_segments_3d(&mut self, points: Vec<[f32; 3]>) -> DrawableHandle {
        self.spawn(SpawnKind::LineSegments3D {
            points,
            colors: None,
        })
    }

    pub(crate) fn line_segments_3d_with_colors(
        &mut self,
        points: Vec<[f32; 3]>,
        colors: Vec<Color>,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::LineSegments3D {
            points,
            colors: Some(colors),
        })
    }
    pub fn text(&mut self, s: &str) -> DrawableHandle {
        let spec = gaanim_text::prelude::TextSpec::new(
            vec![s.into()],
            None,
            gaanim_text::prelude::TextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("Canvas::text received invalid text; public bindings validate input first");
        self.text_spec(spec)
    }

    /// Creates unified structured text. This is the owning Rust entry point
    /// used by the Python `Scene.text(*content, ...)` factory.
    pub fn text_spec(&mut self, spec: gaanim_text::prelude::TextSpec) -> DrawableHandle {
        let mut handle = self.spawn(SpawnKind::Text(spec.clone()));
        for part in spec.parts() {
            if let Some(color) = part.style.color {
                handle = handle.color_by(part.text.clone(), color);
            }
            handle = handle.define_tag(part.path.join("."), part.text, Some(part.occurrence));
        }
        handle
    }
    /// Compile full Typst markup, including tables and other document layout.
    pub fn typst(&mut self, source: &str) -> DrawableHandle {
        self.typst_inner(source, None)
    }

    /// Compile full Typst markup with a custom page width (e.g. `"16cm"`, `"800pt"`, `"12in"`).
    pub fn typst_with_width(&mut self, source: &str, page_width: &str) -> DrawableHandle {
        self.typst_inner(source, Some(page_width))
    }

    fn typst_inner(&mut self, source: &str, page_width: Option<&str>) -> DrawableHandle {
        self.spawn(SpawnKind::Typst {
            source: source.to_string(),
            page_width: page_width.map(|w| w.to_string()),
        })
    }

    fn text_semantic_pairs(
        source: &DrawableHandle,
        target: &DrawableHandle,
        requested: Option<Vec<(String, String)>>,
    ) -> Vec<(String, Option<usize>, String, Option<usize>)> {
        let source_tags = source
            .spec
            .lock()
            .expect("object spec poisoned")
            .fragment_tags
            .clone();
        let target_tags = target
            .spec
            .lock()
            .expect("object spec poisoned")
            .fragment_tags
            .clone();
        let requested = requested.unwrap_or_else(|| {
            source_tags
                .iter()
                .filter_map(|(name, _, _)| {
                    target_tags
                        .iter()
                        .any(|(target_name, _, _)| target_name == name)
                        .then_some((name.clone(), name.clone()))
                })
                .collect()
        });
        requested
            .into_iter()
            .filter_map(|(source_name, target_name)| {
                let (_, source_fragment, source_occurrence) = source_tags
                    .iter()
                    .rev()
                    .find(|(tag, _, _)| tag == &source_name)?;
                let (_, target_fragment, target_occurrence) = target_tags
                    .iter()
                    .rev()
                    .find(|(tag, _, _)| tag == &target_name)?;
                Some((
                    source_fragment.clone(),
                    *source_occurrence,
                    target_fragment.clone(),
                    *target_occurrence,
                ))
            })
            .collect()
    }

    /// Auto-match and morph submobjects — improved `TransformMatchingShapes`.
    ///
    /// Matches sub-elements between `source` and `target` using Hungarian minimum-cost pairing,
    /// normalized shape hashing, relative world position, and color similarity.
    /// Matched source sub-elements morph into target sub-elements in exact world space,
    /// surplus source elements fade out, and new target elements fade in.
    pub fn transform_matching_shapes(
        &mut self,
        source: &DrawableHandle,
        target: &DrawableHandle,
        duration: f64,
    ) -> DrawableHandle {
        self.transform_matching(source, target, "shapes", duration)
    }

    /// Generic auto-matching morph. `mode` can be `"shapes"` or `"tex"`.
    ///
    /// Queues an `Op::TransformMatching` operation to pair and morph submobjects between
    /// `source` and `target`.
    pub fn transform_matching(
        &mut self,
        source: &DrawableHandle,
        target: &DrawableHandle,
        mode: &str,
        duration: f64,
    ) -> DrawableHandle {
        if !Arc::ptr_eq(&self.state, &source.state)
            || !Arc::ptr_eq(&self.state, &target.state)
            || !duration.is_finite()
            || duration <= 0.0
        {
            return target.clone();
        }
        let mode = match mode.to_ascii_lowercase().as_str() {
            "tex" | "text" | "chars" => "tex".to_string(),
            _ => "shapes".to_string(),
        };
        let semantic_pairs = if mode == "tex" {
            Self::text_semantic_pairs(source, target, None)
        } else {
            Vec::new()
        };
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::TransformMatching {
                source: source.id,
                target: target.id,
                mode,
                semantic_pairs,
                duration,
            });
        target.clone()
    }

    /// Load a PNG, JPEG, or WebP image as an animatable raster mobject.
    ///
    /// Source pixels are decoded once per canonical path and are displayed at
    /// their native pixel dimensions before `.scaled()` is applied.
    pub fn image(&mut self, path: impl AsRef<Path>) -> Result<DrawableHandle, ImageLoadError> {
        self.image_with_options(path, ImageOptions::default())
    }

    /// Load an image with an optional target size, fit mode, and source crop.
    pub fn image_with_options(
        &mut self,
        path: impl AsRef<Path>,
        options: ImageOptions,
    ) -> Result<DrawableHandle, ImageLoadError> {
        let image = load_image(self.resolve_asset_path(path))?;
        let view = options.resolve(image.width, image.height)?;
        Ok(self.spawn(SpawnKind::Image { image, view }))
    }

    /// Load an SVG as an animatable group of resolved vector paths.
    ///
    /// Shapes, paths, solid or gradient paints, outlined text, transforms,
    /// CSS, `<use>`, `viewBox`, and vector `clipPath` groups are imported.
    /// Raster images, patterns, masks, and arbitrary filters remain omitted.
    pub fn svg(&mut self, path: impl AsRef<Path>) -> Result<DrawableHandle, SvgLoadError> {
        let document = gaanim_objects::prelude::SvgDocument::load(self.resolve_asset_path(path))?;
        let mut parts = HashMap::new();
        let (root, _) = self.spawn_svg_group(&document.root, true, &mut parts);
        if !document.root.id.is_empty() {
            parts.insert(document.root.id.clone(), root.clone());
        }
        Ok(root.with_svg_parts(parts))
    }

    /// Import the default scene of a local glTF 2.0 `.gltf` or `.glb` model.
    pub fn gltf(&mut self, path: impl AsRef<Path>) -> Result<DrawableHandle, GltfLoadError> {
        self.gltf_scene(path, GltfSceneSelector::Default)
    }

    /// Import a selected glTF scene by name or index.
    pub fn gltf_scene(
        &mut self,
        path: impl AsRef<Path>,
        selector: GltfSceneSelector,
    ) -> Result<DrawableHandle, GltfLoadError> {
        let document = GltfDocument::load(self.resolve_asset_path(path), &selector)?;
        let mut handles = HashMap::<usize, DrawableHandle>::new();
        for node in &document.nodes {
            let handle = self.spawn_registered(
                SpawnKind::GltfNode {
                    node_index: node.index,
                    path: node.path.clone(),
                    bounds: node.bounds,
                },
                false,
            );
            handles.insert(node.index, handle);
        }

        let bindings = document
            .nodes
            .iter()
            .map(|node| {
                (
                    node.index,
                    node.parent,
                    node.path.clone(),
                    handles[&node.index].id,
                )
            })
            .collect();
        let animation_names = document
            .animations
            .iter()
            .map(|animation| animation.name.clone())
            .collect::<Vec<_>>();
        let root = self.spawn(SpawnKind::GltfModel {
            path: document.path,
            scene_index: document.scene_index,
            bounds: document.bounds,
            nodes: bindings,
            animation_names: animation_names.clone(),
        });

        let mut parts = HashMap::new();
        let mut short_counts = HashMap::<String, usize>::new();
        for node in &document.nodes {
            *short_counts.entry(node.name.clone()).or_default() += 1;
        }
        for node in &document.nodes {
            let handle = handles[&node.index].clone();
            parts.insert(node.path.clone(), handle.clone());
            if short_counts.get(&node.name) == Some(&1) {
                parts.insert(node.name.clone(), handle);
            }
        }
        Ok(root.with_gltf_metadata(parts, document.animations))
    }

    fn spawn_svg_group(
        &mut self,
        group: &gaanim_objects::prelude::SvgGroup,
        register_top_level: bool,
        parts: &mut HashMap<String, DrawableHandle>,
    ) -> (DrawableHandle, Vec<crate::canvas::ops::SharedObjectSpec>) {
        let mut children = Vec::new();
        let mut leaf_specs = Vec::new();
        for node in &group.children {
            match node {
                gaanim_objects::prelude::SvgNode::Path(path) => {
                    let handle = self.spawn_registered(
                        SpawnKind::SvgPath(Box::new(path.as_ref().clone())),
                        false,
                    );
                    leaf_specs.push(handle.spec.clone());
                    if !path.id.is_empty() {
                        parts.insert(path.id.clone(), handle.clone());
                    }
                    children.push(handle);
                }
                gaanim_objects::prelude::SvgNode::Group(child_group) => {
                    let (handle, descendants) = self.spawn_svg_group(child_group, false, parts);
                    leaf_specs.extend(descendants);
                    children.push(handle);
                }
            }
        }

        let child_ids = children.iter().map(|child| child.id).collect();
        let mut handle = self.spawn_registered(SpawnKind::Group(child_ids), register_top_level);
        handle.spec.lock().expect("SVG group spec poisoned").opacity = group.opacity;
        handle = handle.with_style_targets(leaf_specs.clone());
        if let Some(sigma) = group.blur_sigma {
            handle = handle.blur(sigma);
        }
        if let Some(shadow) = &group.shadow {
            handle = handle.shadow(
                shadow.color,
                DVec2::new(shadow.offset_x, shadow.offset_y),
                shadow.blur_radius,
            );
        }
        if let Some(clip_path) = &group.clip_path {
            let rect = clip_path.bounding_box();
            let mask = self.spawn_registered(
                SpawnKind::SvgPath(Box::new(gaanim_objects::prelude::SvgPath {
                    id: String::new(),
                    path: clip_path.clone(),
                    bounds: gaanim_math::Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1),
                    fill: None,
                    stroke: gaanim_scene::StrokeBrush::transparent(),
                })),
                false,
            );
            handle = handle.clip(&mask, gaanim_core::peniko::Fill::NonZero);
        }
        if !group.id.is_empty() {
            parts.insert(group.id.clone(), handle.clone());
        }
        (handle, leaf_specs)
    }

    pub fn group(&mut self, members: &[&DrawableHandle]) -> DrawableHandle {
        let defer_visibility = members.iter().any(|member| {
            member
                .spec
                .lock()
                .expect("object spec poisoned")
                .defer_visibility_until_play
        });
        let handle = self
            .spawn(SpawnKind::Group(members.iter().map(|m| m.id).collect()))
            .with_style_targets(
                members
                    .iter()
                    .flat_map(|member| member.inherited_style_targets())
                    .collect(),
            );
        if defer_visibility {
            handle.defer_visibility_until_play();
        }
        handle
    }

    /// Build the stable, equation-style row used by reactive numeric readouts.
    #[doc(hidden)]
    pub fn reactive_readout_group(
        &mut self,
        label: Option<&DrawableHandle>,
        equals: Option<&DrawableHandle>,
        number: &DrawableHandle,
        unit: Option<&DrawableHandle>,
        spacing: f64,
    ) -> DrawableHandle {
        let mut members = Vec::with_capacity(4);
        if let Some(label) = label {
            members.push(label);
        }
        if let Some(equals) = equals {
            members.push(equals);
        }
        members.push(number);
        if let Some(unit) = unit {
            members.push(unit);
        }
        let group = self.group(&members);
        group
            .spec
            .lock()
            .expect("readout group spec poisoned")
            .reactive_readout_layout = Some(ReactiveReadoutLayoutSpec {
            label: label.map(|part| part.id),
            equals: equals.map(|part| part.id),
            number: number.id,
            unit: unit.map(|part| part.id),
            spacing,
        });
        group
    }

    pub(crate) fn group_no_center(&mut self, members: &[&DrawableHandle]) -> DrawableHandle {
        let defer_visibility = members.iter().any(|member| {
            member
                .spec
                .lock()
                .expect("object spec poisoned")
                .defer_visibility_until_play
        });
        let handle = self
            .spawn(SpawnKind::GroupNoCenter(
                members.iter().map(|m| m.id).collect(),
            ))
            .with_style_targets(
                members
                    .iter()
                    .flat_map(|member| member.inherited_style_targets())
                    .collect(),
            );
        if defer_visibility {
            handle.defer_visibility_until_play();
        }
        handle
    }

    /// Updates the direct children of a group created by [`Self::group`].
    /// This is used by the persistent layout container; regular group users
    /// can continue treating groups as immutable.
    pub fn set_group_members(&mut self, group: &DrawableHandle, members: &[&DrawableHandle]) {
        let mut spec = group.spec.lock().expect("group spec poisoned");
        if let SpawnKind::Group(children) = &mut spec.kind {
            *children = members.iter().map(|member| member.id).collect();
        }
    }

    /// Queue a layout recalculation. `duration = Some(_)` animates the move
    /// and fades the newly inserted member in; `None` updates immediately.
    pub fn reflow_layout(
        &mut self,
        container: &DrawableHandle,
        members: Vec<LayoutMemberSpec>,
        spec: LayoutSpec,
        version: u64,
        duration: Option<f64>,
        entering: Option<&DrawableHandle>,
        leaving: Option<&DrawableHandle>,
    ) {
        let mut state = self.state.lock().expect("canvas state poisoned");
        let snapshot = LayoutTreeSnapshot {
            version,
            container: container.id,
            members,
            spec,
        };
        state.latest_layouts.insert(container.id, snapshot.clone());
        state.active_mut().ops.push(Op::LayoutTransition {
            from_version: version.checked_sub(1).filter(|version| *version > 0),
            to: snapshot,
            duration: duration.filter(|value| value.is_finite() && *value > 0.0),
            entering: entering.map(|member| member.id),
            leaving: leaving.map(|member| member.id),
        });
        if !state.layout_constraints.is_empty() {
            let constraints = state.layout_constraints.clone();
            state.active_mut().ops.push(Op::LayoutConstraints {
                constraints,
                duration: duration.filter(|value| value.is_finite() && *value > 0.0),
            });
        }
    }

    /// Register relational Layout v2 constraints and resolve them at the
    /// current timeline cursor. Registered relations are automatically
    /// replayed after later structural reflows.
    pub fn constrain_layout(
        &mut self,
        constraints: Vec<gaanim_layout::LayoutConstraint>,
        duration: Option<f64>,
    ) -> Result<(), SceneObjectError> {
        let duration = duration.filter(|value| value.is_finite() && *value > 0.0);
        let mut state = self.state.lock().expect("canvas state poisoned");
        let owns_every_reference = constraints.iter().all(|constraint| {
            constraint
                .lhs
                .terms
                .keys()
                .chain(constraint.rhs.terms.keys())
                .all(|variable| {
                    state
                        .all_drawables
                        .contains(&gaanim_core::ObjectId::from_raw(variable.node.0))
                })
        });
        if !owns_every_reference {
            return Err(SceneObjectError::ForeignScene);
        }
        let mut combined = state.layout_constraints.clone();
        combined.extend(constraints);
        let referenced: std::collections::BTreeSet<_> = combined
            .iter()
            .flat_map(|constraint| {
                constraint
                    .lhs
                    .terms
                    .keys()
                    .chain(constraint.rhs.terms.keys())
                    .map(|variable| variable.node)
            })
            .collect();
        let mut preflight = gaanim_layout::ResolvedLayout::default();
        for node in referenced {
            preflight.boxes.insert(
                node,
                gaanim_layout::ResolvedBox {
                    bounds: gaanim_math::Bounds3D::new_2d(0.0, 0.0, 1.0, 1.0),
                    clip: None,
                    scale: gaanim_core::glam::DVec3::ONE,
                },
            );
        }
        gaanim_layout::solve_constraints(&mut preflight, &combined)?;
        state.layout_diagnostics = preflight
            .diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    None,
                    format!(
                        "constraint #{}: {} (residual {:.6})",
                        diagnostic.constraint, diagnostic.message, diagnostic.residual
                    ),
                )
            })
            .collect();
        state.layout_constraints = combined;
        let constraints = state.layout_constraints.clone();
        state.active_mut().ops.push(Op::LayoutConstraints {
            constraints,
            duration,
        });
        Ok(())
    }

    /// Diagnostics produced by the most recent layout compilation.
    pub fn check_layout(&self) -> Vec<String> {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .layout_diagnostics
            .iter()
            .map(|(_, message)| message.clone())
            .collect()
    }

    /// Diagnostics associated with one layout root.
    pub fn layout_diagnostics(&self, root: &DrawableHandle) -> Vec<String> {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .layout_diagnostics
            .iter()
            .filter(|(owner, _)| owner.is_none_or(|owner| owner == root.id))
            .map(|(_, message)| message.clone())
            .collect()
    }

    fn camera_anim(&self, ty: AnimationType, duration: f64) -> Anim {
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Anim::queued(
            gaanim_core::ObjectId::from_raw(u64::MAX),
            ty,
            self.state.clone(),
            active_idx,
        )
        .duration(duration.max(0.0))
    }

    /// Pan the orthographic camera to a world-space point.
    pub fn camera_pan_to(&mut self, x: f64, y: f64, duration: f64) -> Anim {
        let to = gaanim_core::glam::DVec3::new(x, y, self.camera_position.z);
        self.camera_position = to;
        self.camera_anim(AnimationType::CameraPosition { to }, duration)
    }

    /// Animate orthographic zoom. Values above one zoom in.
    pub fn camera_zoom_to(&mut self, zoom: f64, duration: f64) -> Anim {
        let to = zoom.max(0.01);
        self.camera_zoom = to;
        self.camera_anim(AnimationType::CameraZoom { to }, duration)
    }

    /// Pan and zoom to keep `target` inside the viewport with a uniform margin.
    pub fn camera_frame_to(&mut self, target: &DrawableHandle, margin: f64, duration: f64) -> Anim {
        self.camera_anim(
            AnimationType::CameraFrame {
                target: target.id,
                margin: margin.max(0.0),
            },
            duration,
        )
    }

    /// Rotate the 2D camera around the viewport center, in radians.
    pub fn camera_rotate_to(&mut self, angle: f64, duration: f64) -> Anim {
        let to = gaanim_core::glam::DQuat::from_rotation_z(angle);
        self.camera_rotation = to;
        self.camera_anim(AnimationType::CameraRotation { to }, duration)
    }

    /// Keep the camera centered on `target` while its updaters run.
    pub fn camera_follow(&mut self, target: &DrawableHandle, duration: f64) -> Anim {
        self.camera_anim(AnimationType::CameraFollow { target: target.id }, duration)
    }

    /// Apply a deterministic camera shake that settles back at its start position.
    pub fn camera_shake(&mut self, amplitude: f64, frequency: f64, duration: f64) -> Anim {
        self.camera_anim(
            AnimationType::CameraShake {
                amplitude: amplitude.max(0.0),
                frequency: frequency.max(0.0),
            },
            duration,
        )
    }

    /// Set camera to look at `target` from `eye` with `up` (3D perspective).
    pub fn camera_look_at(
        &mut self,
        eye: (f64, f64, f64),
        target: (f64, f64, f64),
        up: Option<(f64, f64, f64)>,
        duration: f64,
    ) -> Anim {
        let eye = DVec3::new(eye.0, eye.1, eye.2);
        let target = DVec3::new(target.0, target.1, target.2);
        let up = up.map(|(x, y, z)| DVec3::new(x, y, z)).unwrap_or(DVec3::Y);
        self.camera_anim(AnimationType::CameraLookAt { eye, target, up }, duration)
    }

    /// Orbit around current target by yaw/pitch radians.
    pub fn camera_orbit(&mut self, delta_yaw: f64, delta_pitch: f64, duration: f64) -> Anim {
        self.camera_anim(
            AnimationType::CameraOrbit {
                delta_yaw,
                delta_pitch,
            },
            duration,
        )
    }

    /// Animate perspective projection parameters.
    pub fn camera_perspective(&mut self, fov_y: f64, near: f64, far: f64, duration: f64) -> Anim {
        self.camera_anim(
            AnimationType::CameraPerspective {
                fov_y,
                near: near.max(0.01),
                far: far.max(near + 0.1),
            },
            duration,
        )
    }

    /// Dolly camera toward/away from target (factor <1 closer).
    pub fn camera_dolly(&mut self, factor: f64, duration: f64) -> Anim {
        self.camera_anim(AnimationType::CameraDolly { factor }, duration)
    }

    // -- Time controls --

    pub fn wait(&mut self, dur: f64) {
        let mut guard = self.state.lock().expect("canvas state poisoned");
        guard.active_mut().cursor += dur.max(0.0);
        guard.active_mut().ops.push(Op::Wait(dur.max(0.0)));
    }

    /// Regroup auto-queued animations into a parallel batch at the current
    /// cursor. Each `Anim` passed here is deactivated from its original
    /// sequential position before the batch is inserted.
    pub fn play(&mut self, anims: Vec<Anim>) {
        self.play_with_lag(anims, 0.0);
    }

    /// Parallel playback with a uniform stagger between each animation's
    /// start time. Existing per-animation delays are preserved and the lag is
    /// added on top.
    pub fn play_with_lag(&mut self, anims: Vec<Anim>, lag: f64) {
        let lag = lag.max(0.0);
        let builders: Vec<AnimationBuilder> = anims
            .into_iter()
            .enumerate()
            .map(|(idx, anim)| {
                anim.deactivate_auto_queue();
                let mut anim = anim.into_builder();
                anim.delay += idx as f64 * lag;
                anim
            })
            .collect();
        self.play_builders(builders);
    }

    /// Low-level parallel playback for legacy `AnimationBuilder` values.
    pub fn play_builders(&mut self, anims: Vec<AnimationBuilder>) {
        let max_dur = anims
            .iter()
            .map(|a| a.delay.max(0.0) + a.duration.max(0.0))
            .fold(0.0, f64::max);
        let mut guard = self.state.lock().expect("canvas state poisoned");
        guard.active_mut().cursor += max_dur;
        guard.active_mut().ops.push(Op::Play(anims));
    }

    /// Configure reusable branding generated automatically for explicit segments.
    pub fn set_branding(&mut self, branding: PresentationBrand) {
        self.branding = Some(branding);
    }

    fn spawn_segment_branding(
        &mut self,
        template: Option<&str>,
        segment_number: usize,
    ) -> Result<(), SegmentError> {
        let Some(branding) = self.branding.clone() else {
            return Ok(());
        };
        if matches!(template, Some("title_slide" | "title" | "cover")) && !branding.show_on_cover {
            return Ok(());
        }

        let frame = self.safe_frame();
        let palette = self.theme_style.as_ref().map(|theme| theme.palette);
        let muted = palette
            .map(|palette| palette.muted)
            .unwrap_or(Color::from_rgb8(0x94, 0xA3, 0xB8));
        let rule = palette
            .map(|palette| palette.rule)
            .unwrap_or(Color::from_rgb8(0x5B, 0x70, 0x88));
        let footer_y = frame.min.y + frame.height() * 0.018;
        let rule_y = frame.min.y + frame.height() * 0.075;

        if branding.rule {
            self.line(frame.min.x, rule_y, frame.max.x, rule_y)
                .no_fill()
                .stroke(rule, 1.5)
                .z_index(100);
        }
        let footer = match (branding.footer.as_deref(), branding.slide_numbers) {
            (Some(footer), true) => Some(format!("{footer}    ·    {segment_number:02}")),
            (Some(footer), false) => Some(footer.to_owned()),
            (None, true) => Some(format!("{segment_number:02}")),
            (None, false) => None,
        };
        if let Some(footer) = footer {
            self.text(&footer)
                .fill(muted)
                .scaled(0.5)
                .at(frame.min.x + frame.width() * 0.14, footer_y + 8.0)
                .z_index(101);
        }
        if let Some(logo) = branding.logo.as_deref() {
            let extension = logo
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let logo = if extension == "svg" {
                self.svg(logo).map_err(|error| SegmentError::BrandAsset {
                    message: error.to_string(),
                })?
            } else {
                self.image(logo).map_err(|error| SegmentError::BrandAsset {
                    message: error.to_string(),
                })?
            };
            logo.scaled(branding.logo_scale)
                .at_anchor(frame.max.x, frame.max.y, Anchor::TopRight)
                .z_index(101);
        }
        Ok(())
    }

    /// Insert a named or anonymous interactive stop in the active segment.
    pub fn stop(&mut self, name: Option<String>) -> Result<(), SegmentError> {
        let name = match name {
            Some(name) => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return Err(SegmentError::EmptyStopName);
                }
                Some(name)
            }
            None => None,
        };
        let mut state = self.state.lock().expect("canvas state poisoned");
        let segment = state.active_mut();
        let time = segment.cursor;
        if segment
            .stops
            .iter()
            .any(|stop| (stop.time - time).abs() < 1e-9)
        {
            return Err(SegmentError::DuplicateStopTime { time });
        }
        segment.stops.push(LocalSegmentStop { name, time });
        segment.ops.push(Op::Stop);
        Ok(())
    }

    /// Return all segment metadata with local cursors converted to absolute time.
    pub fn segment_manifest(&self) -> SegmentManifest {
        let state = self.state.lock().expect("canvas state poisoned");
        let mut start_time = 0.0;
        let segments = state
            .segments
            .iter()
            .map(|segment| {
                let end_time = start_time + segment.cursor;
                let spec = SegmentSpec {
                    id: segment.id,
                    name: segment.name.clone(),
                    notes: segment.notes.clone(),
                    template: segment.template.clone(),
                    start_time,
                    end_time,
                    stops: segment
                        .stops
                        .iter()
                        .map(|stop| SegmentStop {
                            name: stop.name.clone(),
                            time: start_time + stop.time,
                        })
                        .collect(),
                };
                start_time = end_time;
                spec
            })
            .collect();
        SegmentManifest { segments }
    }

    /// Whether the canvas uses presentation-oriented segment features.
    pub fn has_presentation_features(&self) -> bool {
        self.branding.is_some()
            || self.segment_manifest().segments.iter().any(|segment| {
                segment.notes.is_some() || segment.template.is_some() || !segment.stops.is_empty()
            })
    }

    pub fn fade_out_all(&mut self, dur: f64) {
        let ids = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .mobject_ids
            .clone();
        let anims: Vec<AnimationBuilder> = ids
            .into_iter()
            .map(|id| AnimationBuilder {
                target: id,
                anim_type: AnimationType::FadeOut,
                duration: dur.max(0.0),
                rate_func: gaanim_math::RateFunc::Smooth,
                delay: 0.0,
            })
            .collect();
        if !anims.is_empty() {
            self.play_builders(anims);
        }
    }

    // -- Object controls --

    pub fn show(&mut self, o: &DrawableHandle) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::Show(o.id));
    }

    pub fn hide(&mut self, o: &DrawableHandle) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::Hide(o.id));
    }

    pub fn remove(&mut self, o: &DrawableHandle) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::Remove(o.id));
    }

    /// Adopt an existing drawable into the active segment at the current cursor.
    pub fn reuse(&mut self, o: &DrawableHandle) -> Result<(), SceneObjectError> {
        self.reuse_many(std::slice::from_ref(o))
    }

    /// Adopt several existing drawables into the active segment.
    pub fn reuse_many(&mut self, objects: &[DrawableHandle]) -> Result<(), SceneObjectError> {
        self.queue_scene_objects(objects, SceneObjectAction::Reuse)
    }

    /// Keep an existing drawable visible and animatable across future segments.
    pub fn persist(&mut self, o: &DrawableHandle) -> Result<(), SceneObjectError> {
        self.persist_many(std::slice::from_ref(o))
    }

    /// Keep several existing drawables visible and animatable across future segments.
    pub fn persist_many(&mut self, objects: &[DrawableHandle]) -> Result<(), SceneObjectError> {
        self.queue_scene_objects(objects, SceneObjectAction::Persist)
    }

    /// Stop persisting a drawable and attach it to the active segment.
    pub fn release(&mut self, o: &DrawableHandle) -> Result<(), SceneObjectError> {
        self.release_many(std::slice::from_ref(o))
    }

    /// Stop persisting several drawables and attach them to the active segment.
    pub fn release_many(&mut self, objects: &[DrawableHandle]) -> Result<(), SceneObjectError> {
        self.queue_scene_objects(objects, SceneObjectAction::Release)
    }

    fn queue_scene_objects(
        &mut self,
        objects: &[DrawableHandle],
        action: SceneObjectAction,
    ) -> Result<(), SceneObjectError> {
        if objects.is_empty() {
            return Err(SceneObjectError::NoObjects);
        }
        if objects
            .iter()
            .any(|object| !Arc::ptr_eq(&self.state, &object.state))
        {
            return Err(SceneObjectError::ForeignScene);
        }

        let mut seen = HashSet::new();
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let segment = guard.active_mut();
        for object in objects {
            if !seen.insert(object.id) {
                continue;
            }
            if matches!(
                action,
                SceneObjectAction::Reuse | SceneObjectAction::Release
            ) && !segment.mobject_ids.contains(&object.id)
            {
                segment.mobject_ids.push(object.id);
            }
            segment.ops.push(match action {
                SceneObjectAction::Reuse => Op::Reuse(object.id),
                SceneObjectAction::Persist => Op::Persist(object.id),
                SceneObjectAction::Release => Op::Release(object.id),
            });
        }
        Ok(())
    }

    // -- Reactive objects --

    /// Spawn a value tracker — a reactive float signal that can be animated
    /// with `.animate_to()` and referenced by other reactive components.
    pub fn value_tracker(&mut self, initial: f64) -> DrawableHandle {
        self.spawn(SpawnKind::ValueTracker(initial))
    }

    /// Spawn a hidden dot that follows `curve` at the normalized value of
    /// `tracker`; reveal it with an entry animation in `Canvas::play`.
    ///
    /// The tracker is clamped to `[0, 1]` and sampled by native arc length, so
    /// `0` is the first polyline point and `1` is the last one.
    pub fn point_on_curve(
        &mut self,
        curve: &DrawableHandle,
        tracker: &DrawableHandle,
    ) -> DrawableHandle {
        let handle = self.dot(8.0);
        handle.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachPointOnCurve {
                target: handle.id,
                curve: curve.id,
                tracker: tracker.id,
            });
        handle
    }

    /// Spawn a hidden line centered and aligned with the tangent of `curve`.
    pub fn tangent_on_curve(
        &mut self,
        curve: &DrawableHandle,
        tracker: &DrawableHandle,
        length: f64,
    ) -> DrawableHandle {
        let half_length = length.max(0.0) / 2.0;
        let handle = self.line(-half_length, 0.0, half_length, 0.0);
        handle.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTangentOnCurve {
                target: handle.id,
                curve: curve.id,
                tracker: tracker.id,
            });
        handle
    }

    /// Spawn a hidden line centered and perpendicular to the tangent of `curve`.
    pub fn normal_on_curve(
        &mut self,
        curve: &DrawableHandle,
        tracker: &DrawableHandle,
        length: f64,
    ) -> DrawableHandle {
        let half_length = length.max(0.0) / 2.0;
        let handle = self.line(-half_length, 0.0, half_length, 0.0);
        handle.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachNormalOnCurve {
                target: handle.id,
                curve: curve.id,
                tracker: tracker.id,
            });
        handle
    }

    /// Spawn a hidden unit circle scaled to the local osculating circle of `curve`.
    pub fn curvature_on_curve(
        &mut self,
        curve: &DrawableHandle,
        tracker: &DrawableHandle,
        window: f64,
    ) -> DrawableHandle {
        let handle = self.circle(1.0);
        handle.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachCurvatureOnCurve {
                target: handle.id,
                curve: curve.id,
                tracker: tracker.id,
                window,
            });
        handle
    }

    /// Creates a hidden curved arrow whose sweep is regenerated from `tracker`
    /// on every frame. The effective sweep is `value * sweep_scale + sweep_offset`.
    pub fn always_redraw_arc(
        &mut self,
        tracker: &DrawableHandle,
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        initial_value: f64,
        sweep_scale: f64,
        sweep_offset: f64,
    ) -> DrawableHandle {
        let handle = self.curved_arrow_arc(
            cx,
            cy,
            radius,
            start_angle,
            initial_value * sweep_scale + sweep_offset,
        );
        handle.defer_visibility_until_play();
        let target = handle.id;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTrackerArc {
                target,
                tracker: tracker.id,
                center: (cx, cy),
                radius,
                start_angle,
                sweep_scale,
                sweep_offset,
            });
        handle
    }

    /// Spawn a hidden traced path that accumulates the trajectory of `source`
    /// as a continuous line. The returned drawable's Path2D is regenerated
    /// every frame and revealed by an entry animation in `Canvas::play`.
    pub fn traced_path(&mut self, source: &DrawableHandle) -> DrawableHandle {
        self.traced_path_with_options(source, None, None, 1.0)
    }

    /// Spawn a traced path with optional temporal and sample-count retention limits.
    pub fn traced_path_with_options(
        &mut self,
        source: &DrawableHandle,
        dissipating_time: Option<f64>,
        max_points: Option<usize>,
        min_distance: f64,
    ) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TracedPathLine);
        handle.defer_visibility_until_play();
        let source_id = source.id;
        let id = handle.id;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTracedPath {
                target: id,
                source: source_id,
                min_distance,
                max_points,
                dissipating_time,
            });
        handle
    }

    /// Spawn a hidden 3D traced path that accumulates the 3D trajectory of
    /// `source` as a `LineList`.
    /// Supports optional colormap (`"inferno"`, `"viridis"`, `"plasma"`) for time-based coloring.
    pub fn traced_path_3d(
        &mut self,
        source: &DrawableHandle,
        colormap: Option<String>,
        max_points: Option<usize>,
        min_distance: f64,
    ) -> DrawableHandle {
        self.traced_path_3d_with_options(source, colormap, max_points, min_distance, None)
    }

    /// Spawn a 3D traced path with an optional temporal retention window.
    pub fn traced_path_3d_with_options(
        &mut self,
        source: &DrawableHandle,
        colormap: Option<String>,
        max_points: Option<usize>,
        min_distance: f64,
        dissipating_time: Option<f64>,
    ) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TracedPath3DLine);
        handle.defer_visibility_until_play();
        let source_id = source.id;
        let id = handle.id;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTracedPath3D {
                target: id,
                source: source_id,
                min_distance: min_distance.max(0.0),
                max_points,
                colormap,
                dissipating_time,
            });
        handle
    }

    /// Attach a retained custom updater to `target`.
    #[doc(hidden)]
    pub fn attach_custom_updater(
        &mut self,
        target: &DrawableHandle,
        updater: gaanim_animation::Updater,
    ) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachCustomUpdater {
                target: target.id,
                updater,
            });
    }

    /// Spawn a hidden tracking line — a reactive line whose endpoints follow
    /// entities or remain at fixed positions. Updated every frame and revealed
    /// by an entry animation in `Canvas::play`.
    ///
    /// Endpoints can be `DrawableHandle` references (their `.id` is used) or
    /// static `(f64, f64)` positions passed as tuples.
    pub fn tracking_line(&mut self, from: CanvasEndpoint, to: CanvasEndpoint) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TrackingLine);
        handle.defer_visibility_until_play();
        let id = handle.id;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTrackingLine {
                target: id,
                from,
                to,
            });
        handle
    }

    /// Spawn a hidden reactive helical spring between two endpoints.
    ///
    /// Each endpoint can be static or follow a drawable. The path is rebuilt
    /// natively after updaters and position bindings have run, changing the
    /// coil pitch as the endpoint distance changes.
    pub fn spring_between(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        coils: usize,
        amplitude: f64,
    ) -> DrawableHandle {
        self.spring_between_with_crossing(from, to, coils, amplitude, 0.0)
    }

    /// Spawn a reactive helical spring with optional e-like turn crossings.
    ///
    /// `crossing` is clamped to `[0, 1]`: zero produces ordinary sinusoidal
    /// coils, while one folds each turn back along the spring axis briefly.
    pub fn spring_between_with_crossing(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        coils: usize,
        amplitude: f64,
        crossing: f64,
    ) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TrackingLine);
        handle.defer_visibility_until_play();
        let id = handle.id;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTrackingSpring {
                target: id,
                from,
                to,
                coils,
                amplitude,
                crossing,
            });
        handle
    }

    /// Spawn a hidden reactive dimension line between two endpoints.
    pub fn dimension_between(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        offset: f64,
    ) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TrackingLine);
        handle.defer_visibility_until_play();
        let id = handle.id;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTrackingDimension {
                target: id,
                from,
                to,
                offset,
            });
        handle
    }

    // -- Render / export --

    pub fn render(&self) -> bool {
        crate::host::send_to_host(self.clone())
    }

    pub fn export(
        &self,
        path: &str,
        fps: Option<u32>,
        _encoder: Option<&str>,
        transparent: Option<bool>,
    ) -> Result<(), gaanim_export::encoder::ExportError> {
        crate::export::export_canvas_to_path(self.clone(), path, fps, transparent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::ops::Op;
    use bevy::prelude::World;
    use gaanim_math::SpatialTransform;
    use gaanim_scene::{MobjectId, Opacity};
    use gaanim_timeline::scene::SceneMember;
    use gaanim_timeline::snapshot::WorldSnapshot;
    use gaanim_timeline::timeline::Timeline;

    trait UnifiedTextFixture {
        fn math_text(&mut self, source: &str) -> DrawableHandle;
        fn test_title(&mut self, source: &str) -> DrawableHandle;
    }

    impl UnifiedTextFixture for Canvas {
        fn math_text(&mut self, source: &str) -> DrawableHandle {
            self.text(&format!("${source}$"))
        }

        fn test_title(&mut self, source: &str) -> DrawableHandle {
            let spec = gaanim_text::prelude::TextSpec::new(
                vec![source.into()],
                Some(gaanim_text::prelude::TextRole::Title),
                gaanim_text::prelude::TextStyle::default(),
                gaanim_text::prelude::TextFlow::default(),
            )
            .expect("valid title fixture");
            self.text_spec(spec)
        }
    }

    fn compile_updater_count(canvas: &Canvas) -> usize {
        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        world
            .query_filtered::<bevy::prelude::Entity, bevy::prelude::With<gaanim_animation::Updater>>()
            .iter(&world)
            .count()
    }

    #[test]
    fn step_equation_preserves_explicit_tag_mapping_and_occurrence() {
        let mut canvas = Canvas::new(320, 180);
        let source = canvas
            .math_text("x + x = 2x")
            .define_tag("right_x", "x", Some(1));
        let target = canvas
            .math_text("x = y")
            .define_tag("renamed", "y", Some(0));
        let step = source
            .step_to(
                &target,
                Some(vec![("right_x".to_string(), "renamed".to_string())]),
                0.8,
            )
            .unwrap();
        canvas.play(vec![step]);

        let state = canvas.state.lock().expect("canvas state poisoned");
        let pairs = state
            .segments
            .iter()
            .flat_map(|segment| &segment.ops)
            .find_map(|op| match op {
                Op::Play(anims) => anims.iter().find_map(|anim| match &anim.anim_type {
                    AnimationType::TextTransition { semantic_pairs, .. } => Some(semantic_pairs),
                    _ => None,
                }),
                _ => None,
            })
            .expect("step equation op should be queued");
        assert_eq!(
            pairs,
            &vec![("x".to_string(), Some(1), "y".to_string(), Some(0),)]
        );
    }

    #[test]
    fn custom_updater_survives_preview_and_export_recompilation() {
        let mut canvas = Canvas::new(320, 180);
        let dot = canvas.dot(8.0);
        dot.add_custom_updater(gaanim_animation::Updater::new(
            |_dt, _elapsed, _entity, _world| true,
        ));

        assert_eq!(compile_updater_count(&canvas), 1);
        assert_eq!(compile_updater_count(&canvas), 1);
    }

    #[test]
    fn reactive_objects_do_not_run_before_their_authored_cursor() {
        let mut canvas = Canvas::new(320, 180);
        canvas.wait(2.0);
        let dot = canvas.dot(8.0);
        dot.add_custom_updater(gaanim_animation::Updater::new(
            |_dt, elapsed, entity, world| {
                if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                    transform.translation.x = elapsed;
                }
                true
            },
        ));
        let _trail = canvas.traced_path(&dot);
        canvas.wait(1.0);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_animation::PlaybackState::default());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let snapshot = WorldSnapshot::capture(&mut world);
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, snapshot);
        timeline.seek(&mut world, 1.0);

        let mut updater_query = world.query_filtered::<
            (bevy::prelude::Entity, &SpatialTransform),
            bevy::prelude::With<gaanim_animation::Updater>,
        >();
        let (_, transform) = updater_query.single(&world).unwrap();
        assert_eq!(transform.translation.x, 0.0);

        let mut trail_query = world.query::<&gaanim_animation::TracedPath>();
        let traced_path = trail_query.single(&world).unwrap();
        assert!(traced_path.points.is_empty());
    }

    #[test]
    fn traced_path_requires_an_explicit_entry_animation() {
        let mut hidden_canvas = Canvas::new(320, 180);
        let hidden_dot = hidden_canvas.dot(8.0);
        let _hidden_trail = hidden_canvas.traced_path(&hidden_dot);
        hidden_canvas.wait(1.0);

        let mut hidden_world = World::new();
        hidden_world.insert_resource(Timeline::new());
        hidden_world.insert_resource(gaanim_animation::PlaybackState::default());
        hidden_world.insert_resource(gaanim_text::font::FontRegistry::new());
        hidden_world.insert_resource(gaanim_text::prelude::TextConfig::default());
        hidden_canvas.compile(&mut hidden_world);
        hidden_world.flush();

        let hidden_opacity = hidden_world
            .query_filtered::<&Opacity, bevy::prelude::With<gaanim_animation::TracedPath>>()
            .single(&hidden_world)
            .unwrap();
        assert_eq!(hidden_opacity.0, 0.0);

        let mut animated_canvas = Canvas::new(320, 180);
        let animated_dot = animated_canvas.dot(8.0);
        let animated_trail = animated_canvas.traced_path(&animated_dot);
        animated_canvas.play(vec![animated_trail.fade_in(1.0)]);

        let mut animated_world = World::new();
        animated_world.insert_resource(Timeline::new());
        animated_world.insert_resource(gaanim_animation::PlaybackState::default());
        animated_world.insert_resource(gaanim_text::font::FontRegistry::new());
        animated_world.insert_resource(gaanim_text::prelude::TextConfig::default());
        animated_canvas.compile(&mut animated_world);
        animated_world.flush();

        let snapshot = WorldSnapshot::capture(&mut animated_world);
        let mut timeline = animated_world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, snapshot);
        timeline.seek(&mut animated_world, 0.5);
        let halfway = animated_world
            .query_filtered::<&Opacity, bevy::prelude::With<gaanim_animation::TracedPath>>()
            .single(&animated_world)
            .unwrap()
            .0;
        assert!(halfway > 0.0 && halfway < 1.0);
    }

    #[test]
    fn reactive_visuals_require_their_own_play_entry() {
        let mut canvas = Canvas::new(320, 180);
        let anchor = canvas.dot(8.0).at(-60.0, 0.0);
        let mass = canvas.dot(8.0).at(60.0, 0.0);
        let spring = canvas.spring_between_with_crossing(
            CanvasEndpoint::Entity(anchor.id),
            CanvasEndpoint::Entity(mass.id),
            6,
            10.0,
            1.0,
        );
        let dimension = canvas.dimension_between(
            CanvasEndpoint::Entity(anchor.id),
            CanvasEndpoint::Entity(mass.id),
            -24.0,
        );
        let label = canvas.text("mass");
        label.follow_to(&mass, 0.0, 28.0);
        canvas.play(vec![
            spring.fade_in(1.0),
            dimension.create(1.0),
            label.fade_in(1.0),
        ]);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_animation::PlaybackState::default());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let opacity_for = |world: &mut World, id: gaanim_core::ObjectId| {
            // SceneBuilder's id counter starts at zero while Canvas ids start
            // at one; the test resolves the corresponding compiled root.
            let compiled_id = gaanim_core::ObjectId::from_raw(id.as_raw() - 1);
            let mut query = world.query::<(&gaanim_scene::MobjectId, &Opacity)>();
            query
                .iter(world)
                .find(|(object_id, _)| object_id.0 == compiled_id)
                .map(|(_, opacity)| opacity.0)
                .expect("reactive visual should compile into a mobject")
        };

        assert_eq!(opacity_for(&mut world, anchor.id), 1.0);
        assert_eq!(opacity_for(&mut world, spring.id), 0.0);
        assert_eq!(opacity_for(&mut world, dimension.id), 0.0);
        assert_eq!(opacity_for(&mut world, label.id), 0.0);

        let snapshot = WorldSnapshot::capture(&mut world);
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, snapshot);
        timeline.seek(&mut world, 0.5);

        let spring_opacity = opacity_for(&mut world, spring.id);
        let dimension_opacity = opacity_for(&mut world, dimension.id);
        let label_opacity = opacity_for(&mut world, label.id);
        assert!(spring_opacity > 0.0);
        assert!(dimension_opacity > 0.0);
        assert!(label_opacity > 0.0);
    }

    #[test]
    fn deferred_group_fade_in_reveals_deferred_children() {
        let mut canvas = Canvas::new(320, 180);
        let anchor = canvas.dot(8.0).at(-60.0, 0.0);
        let child = canvas.dot(8.0);
        child.follow_to(&anchor, 0.0, 24.0);
        let group = canvas.group(&[&child]);
        canvas.play(vec![group.fade_in(1.0)]);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_animation::PlaybackState::default());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let opacity_for = |world: &mut World, id: gaanim_core::ObjectId| {
            let compiled_id = gaanim_core::ObjectId::from_raw(id.as_raw() - 1);
            let mut query = world.query::<(&gaanim_scene::MobjectId, &Opacity)>();
            query
                .iter(world)
                .find(|(object_id, _)| object_id.0 == compiled_id)
                .map(|(_, opacity)| opacity.0)
                .expect("group member should compile into a mobject")
        };

        assert_eq!(opacity_for(&mut world, group.id), 0.0);
        assert_eq!(opacity_for(&mut world, child.id), 0.0);

        let snapshot = WorldSnapshot::capture(&mut world);
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, snapshot);
        timeline.seek(&mut world, 0.5);

        assert!(opacity_for(&mut world, group.id) > 0.0);
        assert!(opacity_for(&mut world, child.id) > 0.0);
    }

    #[test]
    fn paper_theme_uses_dark_text_fills() {
        use gaanim_text::prelude::TextRole;

        let mut canvas = Canvas::new(1280, 720);
        canvas
            .set_theme("paper")
            .expect("paper is a built-in theme");
        let config = canvas.themed_text_config();

        for role in [
            TextRole::Title,
            TextRole::Subtitle,
            TextRole::Heading,
            TextRole::Body,
            TextRole::Caption,
            TextRole::Label,
            TextRole::Math,
            TextRole::Code,
        ] {
            assert_eq!(config.roles[&role].fill_color, Color::BLACK);
        }
    }

    #[test]
    fn presentation_theme_uses_projector_contrast() {
        use gaanim_text::prelude::TextRole;

        let mut canvas = Canvas::new(1920, 1080);
        canvas
            .set_theme("presentation")
            .expect("presentation is a built-in theme");
        let config = canvas.themed_text_config();

        assert_eq!(canvas.background, Some(Color::from_rgb8(0x07, 0x0B, 0x16)));
        assert_eq!(
            config.roles[&TextRole::Title].fill_color,
            Color::from_rgb8(0xFF, 0xD1, 0x66)
        );
        assert_eq!(
            config.roles[&TextRole::Body].fill_color,
            Color::from_rgb8(0xF4, 0xF7, 0xFB)
        );
    }

    #[test]
    fn known_color_scheme_drives_text_and_components_from_one_palette() {
        use gaanim_text::prelude::TextRole;

        let mut canvas = Canvas::new(1920, 1080);
        canvas.set_theme("dracula").expect("dracula is built in");
        let theme = canvas.theme_style.as_ref().unwrap();

        assert_eq!(canvas.theme.as_deref(), Some("dracula"));
        assert_eq!(canvas.background, Some(Color::from_rgb8(0x28, 0x2A, 0x36)));
        assert_eq!(
            theme.text.roles[&TextRole::Title].fill_color,
            theme.palette.title
        );
        assert_eq!(
            theme.text.roles[&TextRole::Body].fill_color,
            theme.palette.foreground
        );
    }

    #[test]
    fn explicit_background_survives_later_theme_changes() {
        let explicit = Color::from_rgb8(0x12, 0x34, 0x56);
        let mut canvas = Canvas::new(640, 360);
        canvas.set_background(Some(explicit));
        canvas.set_theme("paper").unwrap();
        assert_eq!(canvas.background, Some(explicit));
    }

    #[test]
    fn theme_classes_propagate_through_nested_groups() {
        let mut canvas = Canvas::new(640, 360);
        let first = canvas.circle(20.0);
        let second = canvas.square(30.0);
        let inner = canvas.group(&[&first, &second]);
        let outer = canvas.group(&[&inner]);
        outer.style_class("focus").unwrap();
        assert_eq!(
            first
                .spec
                .lock()
                .expect("first spec poisoned")
                .style_classes,
            ["focus"]
        );
        assert_eq!(
            second
                .spec
                .lock()
                .expect("second spec poisoned")
                .style_classes,
            ["focus"]
        );
    }

    #[test]
    fn derived_theme_can_override_semantic_colors_and_fonts() {
        use std::collections::HashMap;

        use gaanim_text::prelude::TextRole;

        let mut theme = CanvasTheme::builtin("nord").unwrap();
        theme.name = "research-lab".into();
        theme
            .set_colors(&HashMap::from([(
                "accent".into(),
                Color::from_rgb8(0xFF, 0x00, 0x66),
            )]))
            .unwrap();
        theme
            .set_fonts(&HashMap::from([("text".into(), "Inter".into())]))
            .unwrap();

        assert_eq!(theme.palette.accent, Color::from_rgb8(0xFF, 0x00, 0x66));
        for role in [
            TextRole::Title,
            TextRole::Subtitle,
            TextRole::Heading,
            TextRole::Body,
            TextRole::Caption,
            TextRole::Label,
        ] {
            assert_eq!(theme.text.roles[&role].font_family, "Inter");
        }
        assert_ne!(theme.text.roles[&TextRole::Math].font_family, "Inter");
    }

    #[test]
    fn theme_tokens_are_queryable_and_low_contrast_is_reported() {
        use std::collections::HashMap;

        let mut theme = CanvasTheme::builtin("presentation").unwrap();
        assert_eq!(theme.color("border").unwrap(), theme.palette.rule);
        assert_eq!(theme.color("primary").unwrap(), theme.palette.foreground);
        assert!(theme.validate().is_empty());

        theme
            .set_colors(&HashMap::from([(
                "foreground".into(),
                theme.palette.background,
            )]))
            .unwrap();
        assert!(
            theme
                .validate()
                .iter()
                .any(|warning| warning.contains("foreground on background"))
        );
    }

    #[test]
    fn svg_parts_are_addressable_and_group_styles_reach_descendant_paths() {
        let temp = std::env::temp_dir().join(format!(
            "gaanim_svg_parts_api_test_{}.svg",
            std::process::id()
        ));
        std::fs::write(
            &temp,
            r##"<svg width="80" height="40" xmlns="http://www.w3.org/2000/svg">
                <g id="assembly">
                  <rect id="body" width="30" height="20" fill="#0000ff"/>
                  <circle id="joint" cx="50" cy="20" r="8" fill="#00ff00"/>
                </g>
              </svg>"##,
        )
        .unwrap();

        let mut canvas = Canvas::new(320, 180);
        let svg = canvas.svg(&temp).unwrap();
        std::fs::remove_file(temp).unwrap();

        assert_eq!(
            canvas
                .state
                .lock()
                .expect("canvas state poisoned")
                .all_drawables
                .len(),
            1,
            "only the SVG root should be a top-level drawable"
        );

        let red = Color::from_rgb8(255, 0, 0);
        svg.part("assembly").unwrap().fill(red);
        for id in ["body", "joint"] {
            let part = svg.part(id).unwrap();
            let spec = part.spec.lock().expect("SVG path spec poisoned");
            assert!(spec.fill_overridden);
            assert!(matches!(
                spec.fill,
                Some(gaanim_core::peniko::Brush::Solid(color)) if color == red
            ));
        }

        assert!(matches!(
            svg.part("missing"),
            Err(crate::canvas::SvgPartError::Unknown { available, .. })
                if available == "assembly, body, joint"
        ));
        assert!(matches!(
            canvas.circle(10.0).part("body"),
            Err(crate::canvas::SvgPartError::NotSvg)
        ));
    }

    #[test]
    fn play_with_lag_offsets_delays_and_cursor() {
        let mut canvas = Canvas::new(1280, 720);
        let first = canvas.circle(20.0);
        let second = canvas.circle(20.0);

        canvas.play_with_lag(vec![first.fade_in(1.0), second.fade_in(1.0)], 0.25);

        let guard = canvas.state.lock().expect("canvas state poisoned");
        let segment = guard.active();
        assert!((segment.cursor - 1.25).abs() < 1e-9);

        let Some(Op::Play(anims)) = segment.ops.last() else {
            panic!("expected parallel play op");
        };
        assert_eq!(anims.len(), 2);
        assert!((anims[0].delay - 0.0).abs() < 1e-9);
        assert!((anims[1].delay - 0.25).abs() < 1e-9);
    }

    #[test]
    fn camera_animation_can_be_regrouped_with_drawables() {
        let mut canvas = Canvas::new(1280, 720);
        let marker = canvas.circle(20.0);
        let marker_anim = marker.fade_in(2.0);
        let camera_anim = canvas.camera_orbit(0.5, 0.1, 2.0).linear().delay(0.25);

        canvas.play(vec![marker_anim, camera_anim]);

        let guard = canvas.state.lock().expect("canvas state poisoned");
        let segment = guard.active();
        assert!((segment.cursor - 2.25).abs() < 1e-9);
        let Some(Op::Play(anims)) = segment.ops.last() else {
            panic!("expected parallel play op");
        };
        assert_eq!(anims.len(), 2);
        let camera = anims
            .iter()
            .find(|anim| matches!(anim.anim_type, AnimationType::CameraOrbit { .. }))
            .expect("camera orbit in parallel batch");
        assert!(matches!(camera.rate_func, gaanim_math::RateFunc::Linear));
        assert!((camera.delay - 0.25).abs() < 1e-9);
    }

    #[test]
    fn play_builders_counts_existing_delays_in_cursor() {
        let mut canvas = Canvas::new(1280, 720);
        let first = canvas.circle(20.0);
        let second = canvas.circle(20.0);

        canvas.play(vec![
            first.fade_in(1.0).delay(0.1),
            second.fade_in(1.0).delay(0.4),
        ]);

        let guard = canvas.state.lock().expect("canvas state poisoned");
        let segment = guard.active();
        assert!((segment.cursor - 1.4).abs() < 1e-9);
    }

    #[test]
    fn scene_object_commands_deduplicate_and_reject_foreign_drawables() {
        let mut canvas = Canvas::new(1280, 720);
        let title = canvas.test_title("Persistent title");
        canvas.segment("next", None).unwrap();
        canvas
            .reuse_many(&[title.clone(), title.clone()])
            .expect("same-scene drawable should be reusable");

        let guard = canvas.state.lock().expect("canvas state poisoned");
        let segment = guard.active();
        assert_eq!(
            segment
                .mobject_ids
                .iter()
                .filter(|id| **id == title.id)
                .count(),
            1
        );
        assert_eq!(
            segment
                .ops
                .iter()
                .filter(|op| matches!(op, Op::Reuse(id) if *id == title.id))
                .count(),
            1
        );
        drop(guard);

        let mut other = Canvas::new(1280, 720);
        let foreign = other.circle(20.0);
        assert!(matches!(
            canvas.persist(&foreign),
            Err(SceneObjectError::ForeignScene)
        ));
    }

    #[test]
    fn reuse_persist_and_release_schedule_reversible_scene_membership() {
        let mut canvas = Canvas::new(1280, 720);
        let title = canvas.test_title("Shared title");
        canvas.wait(1.0);

        canvas
            .segment("reused", Some(TransitionType::CrossFade { duration: 0.5 }))
            .unwrap();
        canvas.reuse(&title).unwrap();
        canvas.wait(0.6);
        canvas.persist(&title).unwrap();
        canvas.wait(0.4);

        canvas
            .segment(
                "released",
                Some(TransitionType::Slide {
                    duration: 0.5,
                    direction: gaanim_timeline::transition::SlideDirection::Left,
                }),
            )
            .unwrap();
        canvas.release(&title).unwrap();
        canvas.wait(1.0);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let (initial_scene, reused_scene, released_scene) = {
            let timeline = world.resource::<Timeline>();
            let scene = |name: &str| {
                timeline
                    .scenes
                    .values()
                    .find(|scene| scene.name == name)
                    .map(|scene| scene.id)
                    .unwrap()
            };
            (scene("_default"), scene("reused"), scene("released"))
        };
        let title_entity = world
            .query::<(bevy::prelude::Entity, &MobjectId)>()
            .iter(&world)
            .find_map(|(entity, id)| (id.0 == title.id).then_some(entity))
            .expect("title entity");

        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(
            0.0,
            gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
        );

        timeline.seek(&mut world, 0.5);
        assert_eq!(
            world
                .get::<SceneMember>(title_entity)
                .map(|member| member.0),
            Some(initial_scene)
        );

        timeline.seek(&mut world, 1.25);
        assert_eq!(world.get::<SceneMember>(title_entity), None);
        assert!(world.get::<gaanim_scene::Visible>(title_entity).is_some());
        assert_eq!(
            world.get::<gaanim_scene::Opacity>(title_entity).unwrap().0,
            1.0
        );
        let hierarchy_memberships: Vec<_> = world
            .query::<(bevy::prelude::Entity, &MobjectId)>()
            .iter(&world)
            .map(|(entity, _)| world.get::<SceneMember>(entity).copied())
            .collect();
        assert!(
            hierarchy_memberships.len() > 1,
            "title should contain glyphs"
        );
        assert!(hierarchy_memberships.iter().all(Option::is_none));

        timeline.seek(&mut world, 1.55);
        assert_eq!(
            world
                .get::<SceneMember>(title_entity)
                .map(|member| member.0),
            Some(reused_scene)
        );

        timeline.seek(&mut world, 1.75);
        assert_eq!(world.get::<SceneMember>(title_entity), None);

        timeline.seek(&mut world, 2.0);
        let released_start_x = world
            .get::<gaanim_math::SpatialTransform>(title_entity)
            .unwrap()
            .translation
            .x;
        timeline.seek(&mut world, 2.25);
        assert_eq!(world.get::<SceneMember>(title_entity), None);
        assert_eq!(
            world
                .get::<gaanim_math::SpatialTransform>(title_entity)
                .unwrap()
                .translation
                .x,
            released_start_x
        );

        timeline.seek(&mut world, 2.5);
        assert_eq!(
            world
                .get::<SceneMember>(title_entity)
                .map(|member| member.0),
            Some(released_scene)
        );

        timeline.seek(&mut world, 0.5);
        assert_eq!(
            world
                .get::<SceneMember>(title_entity)
                .map(|member| member.0),
            Some(initial_scene)
        );

        assert_ne!(reused_scene, released_scene);
    }

    #[test]
    fn reuse_from_a_nonadjacent_segment_enters_with_the_destination() {
        let mut canvas = Canvas::new(640, 360);
        let marker = canvas.circle(24.0);
        canvas.wait(0.5);
        canvas.segment("middle", Some(TransitionType::Cut)).unwrap();
        canvas.wait(0.5);
        canvas
            .segment("return", Some(TransitionType::CrossFade { duration: 0.4 }))
            .unwrap();
        canvas.reuse(&marker).unwrap();
        canvas.wait(0.5);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let return_scene = world
            .resource::<Timeline>()
            .scenes
            .values()
            .find(|scene| scene.name == "return")
            .map(|scene| scene.id)
            .unwrap();
        let marker_entity = world
            .query::<(
                bevy::prelude::Entity,
                &MobjectId,
                Option<&bevy::prelude::ChildOf>,
            )>()
            .iter(&world)
            .find_map(|(entity, _, parent)| parent.is_none().then_some(entity))
            .unwrap();
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(
            0.0,
            gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
        );

        timeline.seek(&mut world, 1.2);
        assert_eq!(
            world
                .get::<SceneMember>(marker_entity)
                .map(|member| member.0),
            Some(return_scene)
        );
        let opacity = world.get::<gaanim_scene::Opacity>(marker_entity).unwrap().0;
        assert!((opacity - 0.5).abs() < 1e-5);
    }

    #[test]
    fn persistent_object_stays_fixed_for_the_entire_slide_transition() {
        let mut canvas = Canvas::new(640, 360);
        let title = canvas.test_title("KEEP");
        canvas.wait(1.0);
        canvas.persist(&title).unwrap();
        canvas
            .segment(
                "next",
                Some(TransitionType::Slide {
                    duration: 1.0,
                    direction: gaanim_timeline::transition::SlideDirection::Left,
                }),
            )
            .unwrap();
        canvas.wait(1.0);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let title_entity = world
            .query::<(
                bevy::prelude::Entity,
                &MobjectId,
                Option<&bevy::prelude::ChildOf>,
            )>()
            .iter(&world)
            .find_map(|(entity, _, parent)| parent.is_none().then_some(entity))
            .unwrap();
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(
            0.0,
            gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
        );

        timeline.seek(&mut world, 1.25);
        assert_eq!(world.get::<SceneMember>(title_entity), None);
        let first_x = world
            .get::<gaanim_math::SpatialTransform>(title_entity)
            .unwrap()
            .translation
            .x;
        timeline.seek(&mut world, 1.75);
        assert_eq!(world.get::<SceneMember>(title_entity), None);
        assert_eq!(
            world
                .get::<gaanim_math::SpatialTransform>(title_entity)
                .unwrap()
                .translation
                .x,
            first_x
        );
        assert!(world.get::<gaanim_scene::Visible>(title_entity).is_some());
    }

    #[test]
    fn late_reuse_changes_membership_at_the_current_cursor() {
        let mut canvas = Canvas::new(640, 360);
        let marker = canvas.circle(24.0);
        canvas.wait(1.0);
        canvas
            .segment("second", Some(TransitionType::CrossFade { duration: 0.5 }))
            .unwrap();
        canvas.wait(0.25);
        canvas.reuse(&marker).unwrap();
        canvas.wait(0.5);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let (first_scene, second_scene) = {
            let timeline = world.resource::<Timeline>();
            let scene = |name: &str| {
                timeline
                    .scenes
                    .values()
                    .find(|scene| scene.name == name)
                    .map(|scene| scene.id)
                    .unwrap()
            };
            (scene("_default"), scene("second"))
        };
        let marker_entity = world
            .query::<(
                bevy::prelude::Entity,
                &MobjectId,
                Option<&bevy::prelude::ChildOf>,
            )>()
            .iter(&world)
            .find_map(|(entity, _, parent)| parent.is_none().then_some(entity))
            .unwrap();
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(
            0.0,
            gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
        );

        timeline.seek(&mut world, 1.2);
        assert_eq!(
            world
                .get::<SceneMember>(marker_entity)
                .map(|member| member.0),
            Some(first_scene)
        );
        timeline.seek(&mut world, 1.3);
        assert_eq!(
            world
                .get::<SceneMember>(marker_entity)
                .map(|member| member.0),
            Some(second_scene)
        );
    }

    #[test]
    fn persistence_covers_group_and_svg_descendants() {
        let temp = std::env::temp_dir().join(format!(
            "gaanim_persistent_svg_api_test_{}.svg",
            std::process::id()
        ));
        std::fs::write(
            &temp,
            r##"<svg width="80" height="40" xmlns="http://www.w3.org/2000/svg">
                <g id="assembly">
                  <rect id="body" width="30" height="20" fill="#0000ff"/>
                  <circle id="joint" cx="50" cy="20" r="8" fill="#00ff00"/>
                </g>
              </svg>"##,
        )
        .unwrap();

        let mut canvas = Canvas::new(640, 360);
        let left = canvas.circle(20.0);
        let right = canvas.square(40.0);
        let group = canvas.group(&[&left, &right]);
        let svg = canvas.svg(&temp).unwrap();
        std::fs::remove_file(temp).unwrap();
        canvas.persist_many(&[group, svg]).unwrap();

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(
            0.0,
            gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
        );
        timeline.seek(&mut world, 0.0);

        let memberships: Vec<_> = world
            .query::<(bevy::prelude::Entity, &MobjectId)>()
            .iter(&world)
            .map(|(entity, _)| world.get::<SceneMember>(entity).copied())
            .collect();
        assert!(memberships.len() >= 7, "expected group and SVG hierarchies");
        assert!(memberships.iter().all(Option::is_none));
    }

    #[test]
    fn persistent_transform_does_not_localize_its_source() {
        let mut canvas = Canvas::new(1280, 720);
        let source = canvas.test_title("Source");
        canvas.persist(&source).unwrap();
        canvas.wait(0.5);
        canvas
            .segment("target", Some(TransitionType::CrossFade { duration: 0.2 }))
            .unwrap();
        let target = canvas.test_title("Target");
        source.transform(&target).duration(0.5);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.seek(&mut world, 0.8);
        let source_entity = world
            .query::<(bevy::prelude::Entity, &MobjectId)>()
            .iter(&world)
            .find_map(|(entity, id)| (id.0 == source.id).then_some(entity))
            .expect("source entity");
        assert_eq!(world.get::<SceneMember>(source_entity), None);
    }

    #[test]
    fn cross_segment_transform_does_not_retag_source_in_initial_snapshot() {
        let mut canvas = Canvas::new(1280, 720);
        canvas.segment("first", None).unwrap();
        let circle = canvas.circle(40.0);
        let diamond = canvas.rect(80.0, 80.0);
        circle.transform(&diamond).duration(1.0);

        canvas
            .segment(
                "second",
                Some(gaanim_timeline::transition::TransitionType::CrossFade { duration: 0.2 }),
            )
            .unwrap();
        let replacement = canvas.rect(120.0, 40.0);
        circle.replacement_transform(&replacement).duration(1.0);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let timeline = world.resource::<Timeline>();
        let first_scene = timeline
            .scenes
            .values()
            .find(|scene| scene.name == "first")
            .map(|scene| scene.id)
            .expect("first scene");
        let mut query = world.query::<(bevy::prelude::Entity, &MobjectId)>();
        let circle_entity = query
            .iter(&world)
            .find_map(|(entity, id)| (id.0 == circle.id).then_some(entity))
            .expect("circle entity");

        assert_eq!(
            world
                .get::<SceneMember>(circle_entity)
                .map(|member| member.0),
            Some(first_scene)
        );
    }

    #[test]
    fn tracker_arc_compiles_with_signal_and_regenerator() {
        let mut canvas = Canvas::new(1280, 720);
        let tracker = canvas.value_tracker(0.4);
        let _arrow = canvas.always_redraw_arc(&tracker, 0.0, 0.0, 120.0, 0.0, 0.4, 1.0, 0.0);
        canvas.play(vec![tracker.animate_value_to(1.2).duration(1.0)]);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let tracker_entity = world
            .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::FloatSignal>>()
            .iter(&world)
            .next()
            .expect("tracker entity");
        let arrow_entity = world
            .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::AlwaysRedrawRegen>>()
            .iter(&world)
            .next()
            .expect("arrow entity");

        assert_eq!(
            world
                .get::<gaanim_animation::FloatSignal>(tracker_entity)
                .map(|signal| signal.value),
            Some(0.4)
        );
        assert!(
            world
                .get::<gaanim_animation::AlwaysRedrawRegen>(arrow_entity)
                .is_some()
        );

        world
            .get_mut::<gaanim_animation::FloatSignal>(tracker_entity)
            .expect("tracker signal")
            .value = 1.2;
        gaanim_animation::always_redraw_regen_system(&mut world);
        assert!(
            !world
                .get::<gaanim_scene::Path2D>(arrow_entity)
                .expect("reactive path")
                .0
                .elements()
                .is_empty(),
            "the regenerated arrow path must reflect the signal value"
        );
    }

    #[test]
    fn group_pivot_is_preserved_in_the_compiled_transform() {
        let mut canvas = Canvas::new(1280, 720);
        let rail = canvas.line(40.0, 0.0, 140.0, 0.0);
        let _mechanism = canvas.group(&[&rail]).with_pivot(100.0, -18.0);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let mut query = world.query::<(&MobjectId, &gaanim_math::SpatialTransform)>();
        let transform = query
            .iter(&world)
            .find_map(|(_, transform)| {
                (transform.anchor == gaanim_core::glam::DVec3::new(10.0, -18.0, 0.0))
                    .then_some(transform)
            })
            .expect("group transform");

        assert_eq!(
            transform.anchor,
            gaanim_core::glam::DVec3::new(10.0, -18.0, 0.0)
        );
    }

    #[test]
    fn axes_compile_grid_lines_and_ticks_with_independent_styles() {
        let mut canvas = Canvas::new(640, 360);
        let axis_color = Color::from_rgb8(0x11, 0x22, 0x33);
        let grid_color = Color::from_rgb8(0x44, 0x55, 0x66);
        let tick_color = Color::from_rgb8(0x77, 0x88, 0x99);
        let config = crate::canvas::AxesConfig {
            y_grid: false,
            y_ticks: false,
            y_numbers: false,
            axis_color,
            grid_color,
            tick_color,
            x_label: Some("time".to_owned()),
            ..Default::default()
        };
        let _axes = canvas.axes((-200.0, 200.0, 50.0), (-100.0, 100.0, 25.0), config);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let mut query = world.query::<(&gaanim_scene::ObjectTag, &gaanim_scene::StrokeBrush)>();
        let layers = query
            .iter(&world)
            .filter_map(|(tag, stroke)| {
                let gaanim_core::peniko::Brush::Solid(color) = stroke.brush.as_ref()? else {
                    return None;
                };
                Some((tag.0.as_str(), *color, stroke.style.width))
            })
            .collect::<Vec<_>>();

        assert!(layers.contains(&("SvgPath#AxesLines", axis_color, 3.0)));
        assert!(layers.contains(&("SvgPath#AxesGrid", grid_color, 1.0)));
        assert!(layers.contains(&("SvgPath#AxesTicks", tick_color, 2.0)));
    }

    #[test]
    fn reactive_spring_regenerates_a_helical_path() {
        let mut canvas = Canvas::new(1280, 720);
        let _spring = canvas.spring_between_with_crossing(
            crate::canvas::CanvasEndpoint::Static(gaanim_core::glam::DVec3::new(-80.0, 0.0, 0.0)),
            crate::canvas::CanvasEndpoint::Static(gaanim_core::glam::DVec3::new(80.0, 0.0, 0.0)),
            5,
            14.0,
            0.7,
        );

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let spring = world
            .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::AlwaysRedrawRegen>>()
            .iter(&world)
            .next()
            .expect("reactive spring entity");
        gaanim_animation::always_redraw_regen_system(&mut world);
        let path = world
            .get::<gaanim_scene::Path2D>(spring)
            .expect("spring path");
        assert!(
            path.0.elements().len() > 100,
            "spring must have enough samples for a smooth helical coil"
        );
    }

    #[test]
    fn segments_use_absolute_ranges_and_only_explicit_stops_pause() {
        let mut canvas = Canvas::new(1280, 720);
        let intro = canvas
            .segment_with("intro", None, Some("Opening".to_string()), None)
            .unwrap();
        canvas.wait(1.0);
        canvas.stop(Some("reveal".to_string())).unwrap();
        canvas.wait(2.0);
        let details = canvas
            .segment_with("details", None, None, Some("comparison".to_string()))
            .unwrap();
        canvas.wait(0.5);

        let manifest = canvas.segment_manifest();
        assert_eq!(manifest.segments.len(), 2);
        assert_eq!(manifest.segments[0].id, intro.id());
        assert_eq!(manifest.segments[0].name, "intro");
        assert_eq!(manifest.segments[0].notes.as_deref(), Some("Opening"));
        assert_eq!(manifest.segments[0].start_time, 0.0);
        assert_eq!(manifest.segments[0].end_time, 3.0);
        assert_eq!(manifest.segments[0].stops[0].time, 1.0);
        assert_eq!(manifest.segments[1].id, details.id());
        assert_eq!(manifest.segments[1].start_time, 3.0);
        assert_eq!(manifest.segments[1].end_time, 3.5);
        assert_eq!(canvas.current_time(), 3.5);

        let state = canvas.state.lock().expect("canvas state poisoned");
        let stops = state
            .segments
            .iter()
            .flat_map(|segment| segment.ops.iter())
            .filter(|op| matches!(op, Op::Stop))
            .count();
        assert_eq!(stops, 1);
    }

    #[test]
    fn segments_validate_names_links_and_duplicate_stops() {
        let mut canvas = Canvas::new(1280, 720);
        assert!(matches!(
            canvas.segment("  ", None),
            Err(SegmentError::EmptyName)
        ));

        let first = canvas.segment("first", None).unwrap();
        assert!(matches!(
            canvas.segment("FIRST", None),
            Err(SegmentError::DuplicateName { .. })
        ));
        canvas.wait(1.0);
        canvas.stop(None).unwrap();
        assert!(matches!(
            canvas.stop(None),
            Err(SegmentError::DuplicateStopTime { .. })
        ));
        let second = canvas.segment("second", None).unwrap();
        assert!(
            canvas
                .link(&first, &second, TransitionType::CrossFade { duration: 0.2 })
                .is_ok()
        );

        let mut foreign_canvas = Canvas::new(1280, 720);
        let foreign = foreign_canvas.segment("foreign", None).unwrap();
        assert!(matches!(
            canvas.link(&foreign, &second, TransitionType::Cut),
            Err(SegmentError::ForeignSegment)
        ));

        let mut unicode_canvas = Canvas::new(1280, 720);
        unicode_canvas.segment("ÁREA", None).unwrap();
        assert!(matches!(
            unicode_canvas.segment("área", None),
            Err(SegmentError::DuplicateName { .. })
        ));
    }

    #[test]
    fn segment_template_metadata_is_preserved() {
        let mut canvas = Canvas::new(1000, 600);
        canvas
            .segment_with("content", None, None, Some("lecture".to_string()))
            .unwrap();
        let manifest = canvas.segment_manifest();
        assert_eq!(manifest.segments[0].template.as_deref(), Some("lecture"));
    }

    #[test]
    fn segment_layout_aliases_and_branding_are_reusable() {
        let mut canvas = Canvas::new(1280, 720);
        canvas.set_branding(PresentationBrand {
            footer: Some("Slides · Research Lab".to_owned()),
            show_on_cover: false,
            ..Default::default()
        });
        let before = canvas
            .state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .ops
            .len();
        canvas
            .segment_with("cover", None, None, Some("title_slide".to_string()))
            .expect("cover segment");
        let after_cover = canvas
            .state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .ops
            .len();
        assert_eq!(after_cover, before, "cover branding is opt-in");

        canvas.wait(0.1);
        canvas
            .segment_with("content", None, None, Some("lecture".to_string()))
            .expect("content segment");
        let after_content = canvas
            .state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .ops
            .len();
        assert_eq!(
            after_content, 2,
            "rule and numbered footer are added to the active segment"
        );
    }

    #[test]
    fn required_constraint_conflicts_fail_during_authoring() {
        let mut canvas = Canvas::new(320, 180);
        let object = canvas.circle(10.0);
        let left = gaanim_layout::LayoutExpression::variable(
            gaanim_layout::LayoutId(object.id.as_raw()),
            gaanim_layout::LayoutAttribute::Left,
        );
        let result = canvas.constrain_layout(
            vec![
                gaanim_layout::LayoutConstraint::equal(left.clone(), 10.0.into()),
                gaanim_layout::LayoutConstraint::equal(left, 20.0.into()),
            ],
            None,
        );
        assert!(matches!(
            result,
            Err(SceneObjectError::Layout(
                gaanim_layout::LayoutError::Unsatisfiable(_)
            ))
        ));
    }

    #[test]
    fn weak_constraint_diagnostics_are_available_before_compile() {
        let mut canvas = Canvas::new(320, 180);
        let object = canvas.circle(10.0);
        let left = gaanim_layout::LayoutExpression::variable(
            gaanim_layout::LayoutId(object.id.as_raw()),
            gaanim_layout::LayoutAttribute::Left,
        );
        canvas
            .constrain_layout(
                vec![
                    gaanim_layout::LayoutConstraint::equal(left.clone(), 10.0.into()),
                    gaanim_layout::LayoutConstraint::equal(left, 20.0.into())
                        .with_strength(gaanim_layout::ConstraintStrength::Weak),
                ],
                None,
            )
            .unwrap();
        let diagnostics = canvas.check_layout();
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("residual 10.000000"));

        let mut foreign = Canvas::new(320, 180);
        let foreign_object = foreign.circle(10.0);
        assert!(!canvas.owns_drawable(&foreign_object));
    }

    #[test]
    fn layout_ownership_rejects_positioning_but_allows_visual_transforms() {
        let mut canvas = Canvas::new(320, 180);
        let owner = canvas.group(&[]);
        let positioned = canvas.circle(10.0).at(12.0, 0.0);
        assert_eq!(
            positioned.claim_layout(&owner),
            Err(crate::canvas::LayoutOwnershipError::PositionalOperation)
        );

        let animated = canvas.circle(10.0);
        let _ = animated.r#move(8.0, 0.0);
        assert_eq!(
            animated.claim_layout(&owner),
            Err(crate::canvas::LayoutOwnershipError::PositionalOperation)
        );

        let visual = canvas.circle(10.0);
        let _ = visual.scale(1.5);
        let _ = visual.rotate(0.25);
        assert!(visual.claim_layout(&owner).is_ok());

        let mut foreign_canvas = Canvas::new(320, 180);
        let foreign = foreign_canvas.circle(10.0);
        assert_eq!(
            foreign.claim_layout(&owner),
            Err(crate::canvas::LayoutOwnershipError::ForeignScene)
        );
    }
}
