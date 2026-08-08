//! Canvas — the top-level facade for building Gaanim animations.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use bevy::prelude::*;
use gaanim_core::glam::{DVec2, DVec3};
use gaanim_core::kurbo::Shape;
use gaanim_core::peniko::Color;
use gaanim_objects::prelude::SvgLoadError;
use gaanim_timeline::transition::TransitionType;

use crate::anim::{AnimationBuilder, AnimationType};
use crate::canvas::drawable::DrawableHandle;
use crate::canvas::ops::{CanvasEndpoint, CanvasState, Op, Segment, SharedCanvasState};
use crate::canvas::types::{
    Anim, CoordinateSystem, ImageOptions, ImageOptionsError, LayoutKind, Margin, ParagraphOptions,
    SpawnKind,
};
use crate::canvas::{
    Anchor, CanvasTheme, FrameLayout, LayoutPreset, LayoutRegion, PresentationBrand,
    PresentationError, PresentationManifest, SlideId, SlideTemplate,
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
    pub units: CoordinateSystem,
    /// Canonical name of the selected theme.
    pub theme: Option<String>,
    /// Complete semantic colors and typography for the selected theme.
    pub theme_style: Option<CanvasTheme>,
    pub margin: Margin,
    pub asset_root: Option<PathBuf>,
    /// Audio sources mixed by FFmpeg when this canvas is exported.
    pub audio_tracks: Vec<AudioTrack>,
    /// Reusable logo/footer treatment generated for every semantic slide.
    pub branding: Option<PresentationBrand>,
    presentation: Arc<Mutex<PresentationManifest>>,
    pub(crate) camera_position: gaanim_core::glam::DVec3,
    pub(crate) camera_zoom: f64,
    pub(crate) camera_rotation: gaanim_core::glam::DQuat,
    pub(crate) state: SharedCanvasState,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            background: None,
            theme: None,
            theme_style: None,
            units: CoordinateSystem::Pixels,
            margin: Margin::default(),
            asset_root: None,
            audio_tracks: Vec::new(),
            branding: None,
            presentation: Arc::new(Mutex::new(PresentationManifest::default())),
            camera_position: gaanim_core::glam::DVec3::ZERO,
            camera_zoom: 1.0,
            camera_rotation: gaanim_core::glam::DQuat::IDENTITY,
            state: Arc::new(Mutex::new(CanvasState::new())),
        }
    }

    pub fn background(mut self, c: Color) -> Self {
        self.background = Some(c);
        self
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
        self.background = Some(theme.palette.background);
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

    /// Validate the active theme for projected-text readability.
    pub fn validate_theme(&self) -> Result<Vec<String>, String> {
        Ok(self
            .theme_style
            .as_ref()
            .ok_or_else(|| "no theme is active on this canvas".to_string())?
            .validate())
    }

    pub(crate) fn themed_text_config(&self) -> gaanim_text::prelude::TextConfig {
        self.theme_style
            .as_ref()
            .map(|theme| theme.text.clone())
            .unwrap_or_default()
    }

    pub(crate) fn register_theme_fonts(&self, registry: &mut gaanim_text::font::FontRegistry) {
        if let Some(theme) = &self.theme_style {
            for font in &theme.fonts {
                registry.register_font(font.family.clone(), font.bytes.to_vec());
            }
        }
    }

    pub fn with_units(mut self, u: CoordinateSystem) -> Self {
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

    /// Creates reusable regions for a conventional title/content/footer video.
    /// Heights, gap, and margins are expressed in the canvas coordinate system.
    pub fn layout(&self, header_height: f64, footer_height: f64, gap: f64) -> FrameLayout {
        FrameLayout::new(self.safe_frame(), header_height, footer_height, gap)
    }

    /// Creates one of the built-in editorial compositions.
    pub fn layout_preset(&self, preset: LayoutPreset) -> FrameLayout {
        FrameLayout::preset(self.safe_frame(), preset)
    }

    /// Returns the drawable region remaining after the configured safe-area margins.
    pub fn safe_area(&self) -> LayoutRegion {
        LayoutRegion {
            bounds: self.safe_frame(),
        }
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
            if resolved
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
            {
                gaanim_objects::prelude::SvgDocument::load(&resolved).map_err(|source| {
                    AssetPreloadError::Svg {
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
    }

    fn safe_frame(&self) -> gaanim_math::Bounds3D {
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
            .active()
            .cursor
    }

    pub fn segment_count(&self) -> usize {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .len()
    }

    fn spawn(&mut self, kind: SpawnKind) -> DrawableHandle {
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

    /// Create a named segment and switch to it. If `transition` is `Some`, it is
    /// linked from the previously active segment.
    pub fn segment(&mut self, name: &str, transition: Option<TransitionType>) -> usize {
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let prev = guard.active_idx;
        let mut seg = Segment::new(name);
        seg.transition = transition;
        seg.prev_segment = Some(prev);
        let idx = guard.segments.len();
        guard.segments.push(seg);
        guard.active_idx = idx;
        idx
    }

    /// Explicitly link two segments by index.
    pub fn link(&mut self, from: usize, to: usize, transition: TransitionType) {
        let mut guard = self.state.lock().expect("canvas state poisoned");
        if from < guard.segments.len() && to < guard.segments.len() {
            guard.segments[to].transition = Some(transition);
            guard.segments[to].prev_segment = Some(from);
        }
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
    pub fn text(&mut self, s: &str) -> DrawableHandle {
        self.spawn(SpawnKind::Text(s.to_string()))
    }
    pub fn paragraph(&mut self, s: &str, options: ParagraphOptions) -> DrawableHandle {
        self.spawn(SpawnKind::Paragraph {
            text: s.to_string(),
            options,
        })
    }
    pub fn title(&mut self, s: &str) -> DrawableHandle {
        self.spawn(SpawnKind::Title(s.to_string()))
    }
    pub fn subtitle(&mut self, s: &str) -> DrawableHandle {
        self.spawn(SpawnKind::Subtitle(s.to_string()))
    }
    pub fn equation(&mut self, s: &str) -> DrawableHandle {
        self.spawn(SpawnKind::Equation(s.to_string()))
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

    /// Morphs all shared semantic tags from one equation into another in
    /// parallel. Tags are registered through `DrawableHandle::define_tag`.
    pub fn transform_equation_tags(
        &mut self,
        source: &DrawableHandle,
        target: &DrawableHandle,
        tags: Option<Vec<String>>,
        duration: f64,
    ) {
        if !Arc::ptr_eq(&self.state, &source.state) || !Arc::ptr_eq(&self.state, &target.state) {
            return;
        }
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
        let requested = tags.unwrap_or_else(|| {
            source_tags
                .iter()
                .filter_map(|(name, _, _)| {
                    target_tags
                        .iter()
                        .any(|(target_name, _, _)| target_name == name)
                        .then_some(name.clone())
                })
                .collect()
        });
        let pairs = requested
            .into_iter()
            .filter_map(|name| {
                let (_, source_fragment, source_occurrence) =
                    source_tags.iter().rev().find(|(tag, _, _)| tag == &name)?;
                let (_, target_fragment, target_occurrence) =
                    target_tags.iter().rev().find(|(tag, _, _)| tag == &name)?;
                Some((
                    source_fragment.clone(),
                    *source_occurrence,
                    target_fragment.clone(),
                    *target_occurrence,
                ))
            })
            .collect::<Vec<_>>();
        if !pairs.is_empty() && duration.is_finite() && duration > 0.0 {
            self.state
                .lock()
                .expect("canvas state poisoned")
                .active_mut()
                .ops
                .push(Op::TaggedTransform {
                    source: source.id,
                    target: target.id,
                    pairs,
                    duration,
                });
        }
    }

    /// Replaces `source` with `target`, keeping the named tag as the moving
    /// anchor. This is useful when a term grows into a longer expression: the
    /// anchor moves while the added glyphs fade in with the new equation.
    pub fn expand_equation_tag(
        &mut self,
        source: &DrawableHandle,
        target: &DrawableHandle,
        tag: &str,
        duration: f64,
    ) {
        if !Arc::ptr_eq(&self.state, &source.state)
            || !Arc::ptr_eq(&self.state, &target.state)
            || tag.trim().is_empty()
            || !duration.is_finite()
            || duration <= 0.0
        {
            return;
        }
        let source_tag = source
            .spec
            .lock()
            .expect("object spec poisoned")
            .fragment_tags
            .iter()
            .rev()
            .find(|(name, _, _)| name == tag)
            .cloned();
        let target_tag = target
            .spec
            .lock()
            .expect("object spec poisoned")
            .fragment_tags
            .iter()
            .rev()
            .find(|(name, _, _)| name == tag)
            .cloned();
        let (
            Some((_, source_fragment, source_occurrence)),
            Some((_, target_fragment, target_occurrence)),
        ) = (source_tag, target_tag)
        else {
            return;
        };

        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::ExpandEquation {
                source: source.id,
                target: target.id,
                source_fragment,
                source_occurrence,
                target_fragment,
                target_occurrence,
                duration,
            });
    }

    /// Replaces a tagged term while retaining the rest of the equation in
    /// place. Source and target tags may differ when a derivation renames a
    /// semantic role.
    pub fn replace_equation_term(
        &mut self,
        source: &DrawableHandle,
        target: &DrawableHandle,
        source_tag: &str,
        target_tag: &str,
        duration: f64,
    ) {
        if !Arc::ptr_eq(&self.state, &source.state)
            || !Arc::ptr_eq(&self.state, &target.state)
            || source_tag.trim().is_empty()
            || target_tag.trim().is_empty()
            || !duration.is_finite()
            || duration <= 0.0
        {
            return;
        }
        let find_tag = |equation: &DrawableHandle, tag: &str| {
            equation
                .spec
                .lock()
                .expect("object spec poisoned")
                .fragment_tags
                .iter()
                .rev()
                .find(|(name, _, _)| name == tag)
                .cloned()
        };
        let (
            Some((_, source_fragment, source_occurrence)),
            Some((_, target_fragment, target_occurrence)),
        ) = (find_tag(source, source_tag), find_tag(target, target_tag))
        else {
            return;
        };
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::ExpandEquation {
                source: source.id,
                target: target.id,
                source_fragment,
                source_occurrence,
                target_fragment,
                target_occurrence,
                duration,
            });
    }

    /// Transitions from one equation step to another. Shared glyphs move to
    /// their new positions; removed and introduced glyphs fade out and in.
    pub fn step_equation(
        &mut self,
        source: &DrawableHandle,
        target: &DrawableHandle,
        duration: f64,
    ) {
        if !Arc::ptr_eq(&self.state, &source.state)
            || !Arc::ptr_eq(&self.state, &target.state)
            || !duration.is_finite()
            || duration <= 0.0
        {
            return;
        }
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::StepEquation {
                source: source.id,
                target: target.id,
                duration,
            });
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
    ) {
        self.transform_matching(source, target, "shapes", duration);
    }

    /// Auto-match and morph for text & math equations — improved `TransformMatchingTex`.
    ///
    /// Matches sub-elements (character glyphs) between text/math objects using an
    /// order-preserving Longest Common Subsequence (LCS) algorithm on character keys,
    /// combined with Hungarian assignment for remaining elements.
    pub fn transform_matching_tex(
        &mut self,
        source: &DrawableHandle,
        target: &DrawableHandle,
        duration: f64,
    ) {
        self.transform_matching(source, target, "tex", duration);
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
    ) {
        if !Arc::ptr_eq(&self.state, &source.state)
            || !Arc::ptr_eq(&self.state, &target.state)
            || !duration.is_finite()
            || duration <= 0.0
        {
            return;
        }
        let mode = match mode.to_ascii_lowercase().as_str() {
            "tex" | "text" | "chars" => "tex".to_string(),
            _ => "shapes".to_string(),
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
                duration,
            });
    }

    /// Dims all equation glyphs except the requested semantic tags and pulses
    /// the surviving terms. This is tag-based rather than glyph-index based,
    /// so it stays stable when equation layout changes.
    pub fn focus_equation(
        &mut self,
        equation: &DrawableHandle,
        tags: Vec<String>,
        dim_opacity: f32,
        duration: f64,
    ) {
        if !Arc::ptr_eq(&self.state, &equation.state)
            || tags.is_empty()
            || !dim_opacity.is_finite()
            || !(0.0..=1.0).contains(&dim_opacity)
            || !duration.is_finite()
            || duration <= 0.0
        {
            return;
        }
        let declared = equation
            .spec
            .lock()
            .expect("object spec poisoned")
            .fragment_tags
            .clone();
        let terms = tags
            .into_iter()
            .filter_map(|name| {
                declared
                    .iter()
                    .rev()
                    .find(|(tag, _, _)| tag == &name)
                    .map(|(_, fragment, occurrence)| (fragment.clone(), *occurrence))
            })
            .collect::<Vec<_>>();
        if !terms.is_empty() {
            self.state
                .lock()
                .expect("canvas state poisoned")
                .active_mut()
                .ops
                .push(Op::FocusEquation {
                    target: equation.id,
                    terms,
                    dim_opacity,
                    duration,
                });
        }
    }

    pub fn brace_label(
        &mut self,
        equation: &DrawableHandle,
        tag: &str,
        label: String,
        above: bool,
        duration: f64,
    ) {
        let Some(selection) = equation.tag(tag) else {
            return;
        };
        if label.trim().is_empty() || !duration.is_finite() || duration <= 0.0 {
            return;
        }
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::BraceLabel {
                target: equation.id,
                fragment: selection.fragment,
                occurrence: selection.occurrence,
                label,
                above,
                duration,
            });
    }

    pub fn annotate_tag(
        &mut self,
        equation: &DrawableHandle,
        tag: &str,
        label: String,
        offset: DVec3,
        duration: f64,
    ) {
        let Some(selection) = equation.tag(tag) else {
            return;
        };
        if label.trim().is_empty() || !duration.is_finite() || duration <= 0.0 {
            return;
        }
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AnnotateFragment {
                target: equation.id,
                fragment: selection.fragment,
                occurrence: selection.occurrence,
                label,
                offset,
                duration,
            });
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
        self.spawn(SpawnKind::Group(members.iter().map(|m| m.id).collect()))
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
        members: &[&DrawableHandle],
        kind: LayoutKind,
        gap: f64,
        duration: Option<f64>,
        entering: Option<&DrawableHandle>,
        leaving: Option<&DrawableHandle>,
        max_width: Option<f64>,
        max_height: Option<f64>,
        shrink_to_fit: bool,
        wrap: bool,
        justify: &str,
    ) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::LayoutReflow {
                container: container.id,
                members: members.iter().map(|member| member.id).collect(),
                kind,
                gap: gap.max(0.0),
                duration: duration.filter(|value| value.is_finite() && *value > 0.0),
                entering: entering.map(|member| member.id),
                leaving: leaving.map(|member| member.id),
                max_width: max_width.filter(|value| value.is_finite() && *value > 0.0),
                max_height: max_height.filter(|value| value.is_finite() && *value > 0.0),
                shrink_to_fit,
                wrap,
                justify: justify.to_string(),
            });
    }

    /// Pan the orthographic camera to a world-space point.
    pub fn camera_pan_to(&mut self, x: f64, y: f64, duration: f64) {
        let to = gaanim_core::glam::DVec3::new(x, y, self.camera_position.z);
        self.camera_position = to;
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let duration = duration.max(0.0);
        guard.active_mut().cursor += duration;
        guard
            .active_mut()
            .ops
            .push(Op::CameraPosition { to, duration });
    }

    /// Animate orthographic zoom. Values above one zoom in.
    pub fn camera_zoom_to(&mut self, zoom: f64, duration: f64) {
        let to = zoom.max(0.01);
        self.camera_zoom = to;
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let duration = duration.max(0.0);
        guard.active_mut().cursor += duration;
        guard.active_mut().ops.push(Op::CameraZoom { to, duration });
    }

    /// Pan and zoom to keep `target` inside the viewport with a uniform margin.
    pub fn camera_frame_to(&mut self, target: &DrawableHandle, margin: f64, duration: f64) {
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let duration = duration.max(0.0);
        guard.active_mut().cursor += duration;
        guard.active_mut().ops.push(Op::CameraFrame {
            target: target.id,
            margin: margin.max(0.0),
            duration,
        });
    }

    /// Rotate the 2D camera around the viewport center, in radians.
    pub fn camera_rotate_to(&mut self, angle: f64, duration: f64) {
        let to = gaanim_core::glam::DQuat::from_rotation_z(angle);
        self.camera_rotation = to;
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let duration = duration.max(0.0);
        guard.active_mut().cursor += duration;
        guard
            .active_mut()
            .ops
            .push(Op::CameraRotation { to, duration });
    }

    /// Keep the camera centered on `target` while its updaters run.
    pub fn camera_follow(&mut self, target: &DrawableHandle, duration: f64) {
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let duration = duration.max(0.0);
        guard.active_mut().cursor += duration;
        guard.active_mut().ops.push(Op::CameraFollow {
            target: target.id,
            duration,
        });
    }

    /// Apply a deterministic camera shake that settles back at its start position.
    pub fn camera_shake(&mut self, amplitude: f64, frequency: f64, duration: f64) {
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let duration = duration.max(0.0);
        guard.active_mut().cursor += duration;
        guard.active_mut().ops.push(Op::CameraShake {
            amplitude: amplitude.max(0.0),
            frequency: frequency.max(0.0),
            duration,
        });
    }

    /// Set camera to look at `target` from `eye` with `up` (3D perspective).
    pub fn camera_look_at(
        &mut self,
        eye: (f64, f64, f64),
        target: (f64, f64, f64),
        up: Option<(f64, f64, f64)>,
        duration: f64,
    ) {
        let eye = DVec3::new(eye.0, eye.1, eye.2);
        let target = DVec3::new(target.0, target.1, target.2);
        let up = up.map(|(x, y, z)| DVec3::new(x, y, z)).unwrap_or(DVec3::Y);
        let duration = duration.max(0.0);
        let mut guard = self.state.lock().expect("canvas state poisoned");
        guard.active_mut().cursor += duration;
        guard.active_mut().ops.push(Op::CameraLookAt {
            eye,
            target,
            up,
            duration,
        });
    }

    /// Orbit around current target by yaw/pitch radians.
    pub fn camera_orbit(&mut self, delta_yaw: f64, delta_pitch: f64, duration: f64) {
        let duration = duration.max(0.0);
        let mut guard = self.state.lock().expect("canvas state poisoned");
        guard.active_mut().cursor += duration;
        guard.active_mut().ops.push(Op::CameraOrbit {
            delta_yaw,
            delta_pitch,
            duration,
        });
    }

    /// Animate perspective projection parameters.
    pub fn camera_perspective(&mut self, fov_y: f64, near: f64, far: f64, duration: f64) {
        let duration = duration.max(0.0);
        let mut guard = self.state.lock().expect("canvas state poisoned");
        guard.active_mut().cursor += duration;
        guard.active_mut().ops.push(Op::CameraPerspective {
            fov_y,
            near: near.max(0.01),
            far: far.max(near + 0.1),
            duration,
        });
    }

    /// Dolly camera toward/away from target (factor <1 closer).
    pub fn camera_dolly(&mut self, factor: f64, duration: f64) {
        let duration = duration.max(0.0);
        let mut guard = self.state.lock().expect("canvas state poisoned");
        guard.active_mut().cursor += duration;
        guard
            .active_mut()
            .ops
            .push(Op::CameraDolly { factor, duration });
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

    /// Configure reusable branding generated automatically for semantic slides.
    pub fn set_branding(&mut self, branding: PresentationBrand) {
        self.branding = Some(branding);
    }

    fn spawn_slide_branding(
        &mut self,
        template: SlideTemplate,
        slide_number: usize,
    ) -> Result<(), PresentationError> {
        let Some(branding) = self.branding.clone() else {
            return Ok(());
        };
        if template == SlideTemplate::Title && !branding.show_on_cover {
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
            (Some(footer), true) => Some(format!("{footer}    ·    {slide_number:02}")),
            (Some(footer), false) => Some(footer.to_owned()),
            (None, true) => Some(format!("{slide_number:02}")),
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
                self.svg(logo)
                    .map_err(|error| PresentationError::BrandAsset {
                        message: error.to_string(),
                    })?
            } else {
                self.image(logo)
                    .map_err(|error| PresentationError::BrandAsset {
                        message: error.to_string(),
                    })?
            };
            logo.scaled(branding.logo_scale)
                .at_anchor(frame.max.x, frame.max.y, Anchor::TopRight)
                .z_index(101);
        }
        Ok(())
    }

    /// Begin a named presentation slide at the current timeline cursor.
    ///
    /// Starting a later slide closes the preceding slide. Slide boundaries
    /// deliberately do not create a pause: advancing from the preceding
    /// reveal continues into this slide's first animation, like PowerPoint.
    pub fn slide(
        &mut self,
        name: impl Into<String>,
        notes: Option<String>,
        template: SlideTemplate,
    ) -> Result<SlideId, PresentationError> {
        let start_time = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .cursor;
        let mut presentation = self.presentation.lock().expect("presentation poisoned");
        let id = presentation.start_slide(name.into(), notes, template, start_time)?;
        let slide_number = presentation.slides.len();
        drop(presentation);
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::PresentationSlideStart(id));
        self.spawn_slide_branding(template, slide_number)?;
        Ok(id)
    }

    /// Resolve a named region supplied by a built-in slide template.
    pub fn slide_region(
        &self,
        template: SlideTemplate,
        region: &str,
    ) -> Result<LayoutRegion, PresentationError> {
        template
            .region(self.safe_frame(), region)
            .ok_or_else(|| PresentationError::UnknownRegion {
                template: template.name().to_string(),
                region: region.to_string(),
            })
    }

    /// Insert a named or anonymous reveal pause inside the active slide.
    pub fn slide_step(
        &mut self,
        id: SlideId,
        name: Option<String>,
    ) -> Result<(), PresentationError> {
        let time = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .cursor;
        self.presentation
            .lock()
            .expect("presentation poisoned")
            .add_step(id, name, time)?;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::Slide);
        Ok(())
    }

    /// Return the presentation metadata, closing the final slide at the
    /// canvas cursor when necessary.
    pub fn presentation_manifest(&self) -> PresentationManifest {
        let end_time = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .cursor;
        let mut presentation = self.presentation.lock().expect("presentation poisoned");
        presentation.finalize(end_time);
        presentation.clone()
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

    /// Spawn a dot that follows `curve` at the normalized value of `tracker`.
    ///
    /// The tracker is clamped to `[0, 1]` and sampled by native arc length, so
    /// `0` is the first polyline point and `1` is the last one.
    pub fn point_on_curve(
        &mut self,
        curve: &DrawableHandle,
        tracker: &DrawableHandle,
    ) -> DrawableHandle {
        let handle = self.dot(8.0);
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

    /// Spawn a line centered and aligned with the tangent of `curve`.
    pub fn tangent_on_curve(
        &mut self,
        curve: &DrawableHandle,
        tracker: &DrawableHandle,
        length: f64,
    ) -> DrawableHandle {
        let half_length = length.max(0.0) / 2.0;
        let handle = self.line(-half_length, 0.0, half_length, 0.0);
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

    /// Spawn a line centered and perpendicular to the tangent of `curve`.
    pub fn normal_on_curve(
        &mut self,
        curve: &DrawableHandle,
        tracker: &DrawableHandle,
        length: f64,
    ) -> DrawableHandle {
        let half_length = length.max(0.0) / 2.0;
        let handle = self.line(-half_length, 0.0, half_length, 0.0);
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

    /// Spawn a unit circle scaled to the local osculating circle of `curve`.
    pub fn curvature_on_curve(
        &mut self,
        curve: &DrawableHandle,
        tracker: &DrawableHandle,
        window: f64,
    ) -> DrawableHandle {
        let handle = self.circle(1.0);
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

    /// Creates a curved arrow whose sweep is regenerated from `tracker` on
    /// every frame. The effective sweep is `value * sweep_scale + sweep_offset`.
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

    /// Spawn a traced path that accumulates the trajectory of `source` as a
    /// continuous line. The returned drawable's Path2D is regenerated every frame.
    pub fn traced_path(&mut self, source: &DrawableHandle) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TracedPathLine);
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
                min_distance: 1.0,
                max_points: None,
            });
        handle
    }

    /// Spawn a 3D traced path that accumulates the 3D trajectory of `source` as a `LineList`.
    /// Supports optional colormap (`"inferno"`, `"viridis"`, `"plasma"`) for time-based coloring.
    pub fn traced_path_3d(
        &mut self,
        source: &DrawableHandle,
        colormap: Option<String>,
        max_points: Option<usize>,
        min_distance: f64,
    ) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TracedPath3DLine);
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
            });
        handle
    }

    /// Attach a generic Python callback updater to `target`.
    /// `key` must have been registered via `register_python_updater`.
    pub fn attach_python_updater(&mut self, target: &DrawableHandle, key: u64) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachPythonUpdater {
                target: target.id,
                key,
            });
    }

    /// Spawn a tracking line — a reactive line whose endpoints follow entities
    /// or remain at fixed positions. Updated every frame.
    ///
    /// Endpoints can be `DrawableHandle` references (their `.id` is used) or
    /// static `(f64, f64)` positions passed as tuples.
    pub fn tracking_line(&mut self, from: CanvasEndpoint, to: CanvasEndpoint) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TrackingLine);
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

    /// Spawn a reactive zig-zag spring between two endpoints.
    ///
    /// Each endpoint can be static or follow a drawable. The path is rebuilt
    /// natively after updaters and position bindings have run.
    pub fn spring_between(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        coils: usize,
        amplitude: f64,
    ) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TrackingLine);
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
            });
        handle
    }

    /// Spawn a reactive dimension line between two endpoints.
    pub fn dimension_between(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        offset: f64,
    ) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TrackingLine);
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
    use gaanim_scene::MobjectId;
    use gaanim_timeline::scene::SceneMember;
    use gaanim_timeline::timeline::Timeline;

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
            TextRole::Body,
            TextRole::Caption,
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
            TextRole::Body,
            TextRole::Caption,
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
        let title = canvas.title("Persistent title");
        canvas.segment("next", None);
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
        let title = canvas.title("Shared title");
        canvas.wait(1.0);

        canvas.segment("reused", Some(TransitionType::CrossFade { duration: 0.5 }));
        canvas.reuse(&title).unwrap();
        canvas.wait(0.6);
        canvas.persist(&title).unwrap();
        canvas.wait(0.4);

        canvas.segment(
            "released",
            Some(TransitionType::Slide {
                duration: 0.5,
                direction: gaanim_timeline::transition::SlideDirection::Left,
            }),
        );
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
        canvas.segment("middle", Some(TransitionType::Cut));
        canvas.wait(0.5);
        canvas.segment("return", Some(TransitionType::CrossFade { duration: 0.4 }));
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
        let title = canvas.title("KEEP");
        canvas.wait(1.0);
        canvas.persist(&title).unwrap();
        canvas.segment(
            "next",
            Some(TransitionType::Slide {
                duration: 1.0,
                direction: gaanim_timeline::transition::SlideDirection::Left,
            }),
        );
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
        canvas.segment("second", Some(TransitionType::CrossFade { duration: 0.5 }));
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
        let source = canvas.title("Source");
        canvas.persist(&source).unwrap();
        canvas.wait(0.5);
        canvas.segment("target", Some(TransitionType::CrossFade { duration: 0.2 }));
        let target = canvas.title("Target");
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
        canvas.segment("first", None);
        let circle = canvas.circle(40.0);
        let diamond = canvas.rect(80.0, 80.0);
        circle.transform(&diamond).duration(1.0);

        canvas.segment(
            "second",
            Some(gaanim_timeline::transition::TransitionType::CrossFade { duration: 0.2 }),
        );
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
    fn reactive_spring_regenerates_a_zig_zag_path() {
        let mut canvas = Canvas::new(1280, 720);
        let _spring = canvas.spring_between(
            crate::canvas::CanvasEndpoint::Static(gaanim_core::glam::DVec3::new(-80.0, 0.0, 0.0)),
            crate::canvas::CanvasEndpoint::Static(gaanim_core::glam::DVec3::new(80.0, 0.0, 0.0)),
            5,
            14.0,
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
            path.0.elements().len() > 4,
            "spring must have zig-zag segments"
        );
    }

    #[test]
    fn semantic_slides_close_at_boundaries_and_steps_add_breakpoints() {
        let mut canvas = Canvas::new(1280, 720);
        let intro = canvas
            .slide("intro", Some("Opening".to_string()), SlideTemplate::Blank)
            .unwrap();
        canvas.wait(1.0);
        canvas
            .slide_step(intro, Some("reveal".to_string()))
            .unwrap();
        canvas.wait(2.0);
        let details = canvas
            .slide("details", None, SlideTemplate::TwoColumns)
            .unwrap();
        canvas.wait(0.5);

        let manifest = canvas.presentation_manifest();
        assert_eq!(manifest.slides.len(), 2);
        assert_eq!(manifest.slides[0].name, "intro");
        assert_eq!(manifest.slides[0].notes.as_deref(), Some("Opening"));
        assert_eq!(manifest.slides[0].start_time, 0.0);
        assert_eq!(manifest.slides[0].end_time, Some(3.0));
        assert_eq!(manifest.slides[0].steps[0].time, 1.0);
        assert_eq!(manifest.slides[1].id, details);
        assert_eq!(manifest.slides[1].end_time, Some(3.5));

        let state = canvas.state.lock().expect("canvas state poisoned");
        let breakpoints = state
            .active()
            .ops
            .iter()
            .filter(|op| matches!(op, Op::Slide))
            .count();
        assert_eq!(breakpoints, 1, "only explicit slide steps pause playback");
        assert_eq!(
            state
                .active()
                .ops
                .iter()
                .filter(|op| matches!(op, Op::PresentationSlideStart(_)))
                .count(),
            2,
            "each semantic slide starts an automatic visibility scope"
        );
    }

    #[test]
    fn semantic_slides_validate_names_and_active_handles() {
        let mut canvas = Canvas::new(1280, 720);
        assert!(matches!(
            canvas.slide("  ", None, SlideTemplate::Blank),
            Err(PresentationError::EmptySlideName)
        ));

        let first = canvas.slide("first", None, SlideTemplate::Blank).unwrap();
        assert!(matches!(
            canvas.slide("first", None, SlideTemplate::Blank),
            Err(PresentationError::DuplicateSlideName { .. })
        ));
        let second = canvas.slide("second", None, SlideTemplate::Blank).unwrap();
        assert!(matches!(
            canvas.slide_step(first, None),
            Err(PresentationError::InactiveSlide { .. })
        ));
        assert!(canvas.slide_step(second, None).is_ok());
    }

    #[test]
    fn slide_templates_resolve_named_regions_inside_the_safe_area() {
        let mut canvas = Canvas::new(1000, 600);
        canvas.margin = Margin::all(50.0);

        let title = canvas
            .slide_region(SlideTemplate::TitleContent, "title")
            .unwrap();
        let content = canvas
            .slide_region(SlideTemplate::TitleContent, "content")
            .unwrap();
        let left = canvas
            .slide_region(SlideTemplate::TwoColumns, "left")
            .unwrap();
        let right = canvas
            .slide_region(SlideTemplate::TwoColumns, "right")
            .unwrap();

        assert!(title.bounds.min.y >= content.bounds.max.y);
        assert!(content.height() > title.height());
        assert!(left.bounds.max.x < right.bounds.min.x);
        assert!(matches!(
            canvas.slide_region(SlideTemplate::Blank, "title"),
            Err(PresentationError::UnknownRegion { .. })
        ));
    }

    #[test]
    fn semantic_template_aliases_and_branding_are_reusable() {
        assert_eq!(
            SlideTemplate::parse("cover").expect("cover alias"),
            SlideTemplate::Title
        );
        assert_eq!(
            SlideTemplate::parse("comparison").expect("comparison alias"),
            SlideTemplate::TwoColumns
        );
        assert!(
            SlideTemplate::TwoColumns
                .region(
                    gaanim_math::Bounds3D::new_2d(-100.0, -50.0, 100.0, 50.0),
                    "before",
                )
                .is_some()
        );

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
            .slide("cover", None, SlideTemplate::Title)
            .expect("cover slide");
        let after_cover = canvas
            .state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .ops
            .len();
        assert_eq!(after_cover, before + 1, "cover branding is opt-in");

        canvas
            .slide("content", None, SlideTemplate::TitleContent)
            .expect("content slide");
        let after_content = canvas
            .state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .ops
            .len();
        assert_eq!(
            after_content,
            after_cover + 3,
            "slide start plus rule and numbered footer"
        );
    }
}
