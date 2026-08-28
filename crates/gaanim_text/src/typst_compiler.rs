use bevy::prelude::{BuildChildrenTransformExt, Commands, Entity};
use gaanim_core::{ObjectId, glam::DVec3, kurbo, peniko};
use gaanim_math::{Bounds3D, SpatialTransform};
use gaanim_objects::prelude::MobjectBundle;
use gaanim_scene::{FillBrush, ObjectTag, StrokeBrush, TextBaseline};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};
use typst_kit::{
    downloader::SystemDownloader,
    files::{FileLoader, FileStore},
    packages::SystemPackages,
};

use crate::font::{FontRegistry, OutlineCollector};
use crate::shaper::HierarchyChild;

// Typst imports
use typst::{
    Library, LibraryExt, World, WorldExt,
    diag::FileError,
    foundations::{Bytes, Datetime, Duration},
    layout::{Frame, FrameItem, Transform},
    syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot},
    text::{Font, FontBook},
    utils::LazyHash,
    visualize::{CurveItem as TypstCurveItem, FixedStroke, Geometry, LineCap, LineJoin, Paint},
};
use typst_layout::PagedDocument;

use kurbo::{Cap, Join, Shape};

#[derive(Clone)]
struct PendingTypstChild {
    path: kurbo::BezPath,
    bounds: Bounds3D,
    fill: FillBrush,
    stroke: StrokeBrush,
    tag: ObjectTag,
    span: gaanim_scene::components::TextSpan,
}

#[derive(Clone)]
struct CachedTypstChild {
    path: kurbo::BezPath,
    bounds: Bounds3D,
    transform: SpatialTransform,
    fill: FillBrush,
    stroke: StrokeBrush,
    tag: ObjectTag,
    span: gaanim_scene::components::TextSpan,
}

#[derive(Clone)]
struct CachedTypstHierarchy {
    parent_bounds: Bounds3D,
    metrics: TextMetrics,
    children: Vec<CachedTypstChild>,
}

/// Typographic metrics retained alongside compiled Typst vector geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMetrics {
    /// Local Y coordinate of the first visual line's baseline.
    pub first_baseline: f64,
    /// Number of visual text lines represented by the compiled frame.
    pub line_count: usize,
}

impl Default for TextMetrics {
    fn default() -> Self {
        Self {
            first_baseline: 0.0,
            line_count: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TypstCacheKey {
    source: String,
    is_math: bool,
    text_font: Option<String>,
    math_font: Option<String>,
    text_size_bits: Option<u64>,
    math_size_bits: Option<u64>,
    fill_debug: String,
    stroke_debug: String,
    font_universe: FontUniverseKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FontUniverseKey(Vec<(String, Arc<[u8]>)>);

impl FontUniverseKey {
    fn from_registry(font_registry: &FontRegistry) -> Self {
        let mut fonts = font_registry
            .registered
            .iter()
            .map(|(family, bytes)| (family.clone(), bytes.clone()))
            .collect::<Vec<_>>();
        fonts.sort_by(|left, right| left.0.cmp(&right.0));
        Self(fonts)
    }
}

struct SharedTypstResources {
    files: FileStore<UniverseFileLoader>,
    fonts: typst_kit::fonts::FontStore,
    library: LazyHash<Library>,
}

struct TypstResources {
    shared: &'static SharedTypstResources,
    font_book: LazyHash<FontBook>,
    extra_fonts: Vec<Font>,
    system_font_count: usize,
}

static TYPST_HIERARCHY_CACHE: OnceLock<Mutex<HashMap<TypstCacheKey, Arc<CachedTypstHierarchy>>>> =
    OnceLock::new();
static SHARED_TYPST_RESOURCES: OnceLock<SharedTypstResources> = OnceLock::new();
static TYPST_RESOURCES_CACHE: OnceLock<Mutex<HashMap<FontUniverseKey, Arc<TypstResources>>>> =
    OnceLock::new();

fn typst_hierarchy_cache() -> &'static Mutex<HashMap<TypstCacheKey, Arc<CachedTypstHierarchy>>> {
    TYPST_HIERARCHY_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn shared_typst_resources() -> &'static SharedTypstResources {
    SHARED_TYPST_RESOURCES.get_or_init(|| {
        let mut fonts = typst_kit::fonts::FontStore::new();
        fonts.extend(typst_kit::fonts::embedded());
        fonts.extend(typst_kit::fonts::system());

        if fonts.book().families().next().is_none() {
            eprintln!(
                "GaanimTypstWorld: no system or embedded fonts available. \
                 Typst compilation will fail with 'no font could be found'."
            );
        }

        SharedTypstResources {
            files: FileStore::new(UniverseFileLoader::new()),
            fonts,
            library: LazyHash::new(Library::builder().build()),
        }
    })
}

fn typst_resources_cache() -> &'static Mutex<HashMap<FontUniverseKey, Arc<TypstResources>>> {
    TYPST_RESOURCES_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn typst_resources_for(font_registry: &FontRegistry) -> (FontUniverseKey, Arc<TypstResources>) {
    let key = FontUniverseKey::from_registry(font_registry);
    if let Some(resources) = typst_resources_cache()
        .lock()
        .expect("Typst resources cache poisoned")
        .get(&key)
        .cloned()
    {
        return (key, resources);
    }

    let shared = shared_typst_resources();
    let system_font_count = shared
        .fonts
        .book()
        .families()
        .map(|(_, indices)| indices.count())
        .sum();
    let mut font_book = shared.fonts.book().clone();
    let mut extra_fonts = Vec::new();
    for (_, bytes) in &key.0 {
        if let Some(font) = Font::new(Bytes::new(bytes.clone()), 0) {
            font_book.push(font.info().clone());
            extra_fonts.push(font);
        }
    }

    let resources = Arc::new(TypstResources {
        shared,
        font_book,
        extra_fonts,
        system_font_count,
    });
    typst_resources_cache()
        .lock()
        .expect("Typst resources cache poisoned")
        .insert(key.clone(), resources.clone());
    (key, resources)
}

/// A custom self-contained implementation of `typst::World` for math and document vector compilation.
pub struct GaanimTypstWorld {
    source: Source,
    resources: Arc<TypstResources>,
    main_id: FileId,
}

/// Resolves package files through the same cache and registry as Typst's CLI.
/// Project-local files deliberately remain unavailable: scene markup is supplied
/// in memory and should not gain implicit access to the host file system.
struct UniverseFileLoader {
    packages: SystemPackages,
}

impl UniverseFileLoader {
    fn new() -> Self {
        Self {
            packages: SystemPackages::new(SystemDownloader::new("gaanim/0.3")),
        }
    }
}

impl FileLoader for UniverseFileLoader {
    fn load(&self, id: FileId) -> typst::diag::FileResult<Bytes> {
        match id.root() {
            VirtualRoot::Package(spec) => self.packages.obtain(spec)?.load(id.vpath()),
            VirtualRoot::Project => Err(FileError::NotFound(id.vpath().get_with_slash().into())),
        }
    }
}

impl GaanimTypstWorld {
    /// Creates a new `GaanimTypstWorld` with the user source, Typst default fonts,
    /// system fonts, and any additional fonts registered in the `FontRegistry`.
    pub fn new(source_code: &str, font_registry: &FontRegistry) -> Self {
        let (_, resources) = typst_resources_for(font_registry);
        Self::with_resources(source_code, resources)
    }

    fn with_resources(source_code: &str, resources: Arc<TypstResources>) -> Self {
        let main_id = FileId::unique(RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("/main.typ").unwrap(),
        ));
        let source = Source::new(main_id, source_code.to_string());

        Self {
            source,
            resources,
            main_id,
        }
    }
}

impl World for GaanimTypstWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.resources.shared.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.resources.font_book
    }

    fn main(&self) -> FileId {
        self.main_id
    }

    fn source(&self, id: FileId) -> typst::diag::FileResult<Source> {
        if id == self.main_id {
            Ok(self.source.clone())
        } else {
            self.resources.shared.files.source(id)
        }
    }

    fn file(&self, id: FileId) -> typst::diag::FileResult<Bytes> {
        self.resources.shared.files.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        if index < self.resources.system_font_count {
            self.resources.shared.fonts.font(index)
        } else {
            self.resources
                .extra_fonts
                .get(index - self.resources.system_font_count)
                .cloned()
        }
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

/// Convert a Typst `Paint` into an optional `peniko::Brush`.
fn typst_paint_to_brush(
    paint: &Paint,
    _default_brush: &Option<peniko::Brush>,
) -> Option<peniko::Brush> {
    match paint {
        Paint::Solid(color) => {
            let [r, g, b, a] = color.to_vec4_u8();
            Some(peniko::Brush::Solid(peniko::Color::from_rgba8(r, g, b, a)))
        }
        _ => None,
    }
}

/// Convert a Typst 2D `Transform` into a `kurbo::Affine`.
fn typst_transform_to_affine(transform: &Transform) -> kurbo::Affine {
    kurbo::Affine::new([
        transform.sx.get(),
        transform.ky.get(),
        transform.kx.get(),
        transform.sy.get(),
        transform.tx.to_pt(),
        transform.ty.to_pt(),
    ])
}

/// Convert a Typst layout `Point` into a `kurbo::Point`.
fn typst_point_to_kurbo(point: &typst::layout::Point) -> kurbo::Point {
    kurbo::Point::new(point.x.to_pt(), point.y.to_pt())
}

/// Convert a Typst `Geometry` into a `kurbo::BezPath`.
fn typst_geometry_to_bezpath(geometry: &Geometry) -> kurbo::BezPath {
    let mut path = kurbo::BezPath::new();
    match geometry {
        Geometry::Line(target) => {
            path.move_to(kurbo::Point::new(0.0, 0.0));
            path.line_to(typst_point_to_kurbo(target));
        }
        Geometry::Rect(size) => {
            let rect = kurbo::Rect::new(0.0, 0.0, size.x.to_pt(), size.y.to_pt());
            for el in rect.path_elements(0.1) {
                path.push(el);
            }
        }
        Geometry::Curve(curve) => {
            for item in &curve.0 {
                match item {
                    TypstCurveItem::Move(p) => path.move_to(typst_point_to_kurbo(p)),
                    TypstCurveItem::Line(p) => path.line_to(typst_point_to_kurbo(p)),
                    TypstCurveItem::Cubic(p1, p2, p3) => {
                        path.curve_to(
                            typst_point_to_kurbo(p1),
                            typst_point_to_kurbo(p2),
                            typst_point_to_kurbo(p3),
                        );
                    }
                    TypstCurveItem::Close => path.close_path(),
                }
            }
        }
    }
    path
}

/// Convert a Typst `FixedStroke` into a `kurbo::Stroke`.
fn typst_stroke_to_kurbo(stroke: &FixedStroke) -> kurbo::Stroke {
    let cap = match stroke.cap {
        LineCap::Butt => Cap::Butt,
        LineCap::Round => Cap::Round,
        LineCap::Square => Cap::Square,
    };
    let join = match stroke.join {
        LineJoin::Miter => Join::Miter,
        LineJoin::Round => Join::Round,
        LineJoin::Bevel => Join::Bevel,
    };
    kurbo::Stroke::new(stroke.thickness.to_pt())
        .with_start_cap(cap)
        .with_end_cap(cap)
        .with_join(join)
        .with_miter_limit(stroke.miter_limit.get())
}

/// Recursively extract vector items from a Typst `Frame` into Gaanim Mobject entities.
fn extract_frame_items(
    frame: &Frame,
    current_transform: &kurbo::Affine,
    total_bounds: &mut Option<Bounds3D>,
    default_fill: &Option<peniko::Brush>,
    default_stroke: &StrokeBrush,
    world: &dyn World,
    char_index_counter: &mut usize,
    extracted_children: &mut Vec<PendingTypstChild>,
) {
    for (pos, item) in frame.items() {
        // Typst frames use Y-down coordinate system, and we convert it to Y-up
        // globally using a root flip transform. So we keep translation Y positive.
        let item_offset = kurbo::Affine::translate((pos.x.to_pt(), pos.y.to_pt()));
        let item_transform = *current_transform * item_offset;

        match item {
            FrameItem::Group(group) => {
                let group_affine = typst_transform_to_affine(&group.transform);
                let new_transform = item_transform * group_affine;
                extract_frame_items(
                    &group.frame,
                    &new_transform,
                    total_bounds,
                    default_fill,
                    default_stroke,
                    world,
                    char_index_counter,
                    extracted_children,
                );
            }
            FrameItem::Text(text) => {
                let font = &text.font;
                let size = text.size;
                let upem = font.units_per_em();
                let scale = size.to_pt() / upem;
                let ttf = font.ttf();
                // Determine effective fill brush
                let fill_brush = typst_paint_to_brush(&text.fill, default_fill);

                // Typst accumulates advances manually when rendering.
                // We must do the same to recover each glyph's correct position
                // inside the text run.
                let mut pen_x = 0.0;
                let mut pen_y = 0.0;
                for glyph in text.glyphs.iter() {
                    let mut collector = OutlineCollector::new();
                    let glyph_id = ttf_parser::GlyphId(glyph.id);
                    if ttf.outline_glyph(glyph_id, &mut collector).is_some() {
                        let mut path = collector.path;

                        let glyph_x = pen_x + glyph.x_offset.at(size).to_pt();
                        let glyph_y = pen_y + glyph.y_offset.at(size).to_pt();

                        // Scale outline and vertically flip it to map Y-up font outline to Y-down Typst canvas space
                        let glyph_transform = item_transform
                            * kurbo::Affine::translate((glyph_x, glyph_y))
                            * kurbo::Affine::scale_non_uniform(scale, -scale);

                        path.apply_affine(glyph_transform);

                        let bbox = path.bounding_box();
                        let local_bounds = Bounds3D::new_2d(bbox.x0, bbox.y0, bbox.x1, bbox.y1);

                        // Match glyph to corresponding source char and range
                        let byte_offset = glyph.span.1 as usize;
                        let c = text
                            .text
                            .get(byte_offset..)
                            .and_then(|s| s.chars().next())
                            .unwrap_or('?');

                        let span_range = world.range(glyph.span.0).unwrap_or(0..0);
                        let source_start = span_range.start + byte_offset;
                        let source_end = source_start + c.len_utf8();

                        let span = gaanim_scene::components::TextSpan {
                            character: c,
                            char_index: *char_index_counter,
                            source_range: core::range::Range {
                                start: source_start,
                                end: source_end,
                            },
                        };

                        extracted_children.push(PendingTypstChild {
                            path,
                            bounds: local_bounds,
                            fill: FillBrush(fill_brush.clone()),
                            stroke: default_stroke.clone(),
                            tag: ObjectTag("TypstGlyph".into()),
                            span,
                        });

                        *char_index_counter += 1;

                        if let Some(tb) = total_bounds {
                            *tb = tb.union(&local_bounds);
                        } else {
                            *total_bounds = Some(local_bounds);
                        }
                    }

                    pen_x += glyph.x_advance.at(size).to_pt();
                    pen_y += glyph.y_advance.at(size).to_pt();
                }
            }
            FrameItem::Shape(shape, _span) => {
                let mut path = typst_geometry_to_bezpath(&shape.geometry);
                path.apply_affine(item_transform);

                let bbox = path.bounding_box();
                let local_bounds = Bounds3D::new_2d(bbox.x0, bbox.y0, bbox.x1, bbox.y1);

                let span_range = world.range(*_span).unwrap_or(0..0);
                let span = gaanim_scene::components::TextSpan {
                    character: '_', // Marker for drawing shapes
                    char_index: *char_index_counter,
                    source_range: core::range::Range {
                        start: span_range.start,
                        end: span_range.end,
                    },
                };
                extracted_children.push(PendingTypstChild {
                    path,
                    bounds: local_bounds,
                    fill: FillBrush(
                        shape
                            .fill
                            .as_ref()
                            .and_then(|p| typst_paint_to_brush(p, default_fill)),
                    ),
                    stroke: shape
                        .stroke
                        .as_ref()
                        .map(|stroke| StrokeBrush {
                            brush: typst_paint_to_brush(&stroke.paint, default_fill),
                            style: typst_stroke_to_kurbo(stroke),
                        })
                        .unwrap_or_else(StrokeBrush::transparent),
                    tag: ObjectTag("TypstShape".into()),
                    span,
                });
                *char_index_counter += 1;

                if let Some(tb) = total_bounds {
                    *tb = tb.union(&local_bounds);
                } else {
                    *total_bounds = Some(local_bounds);
                }
            }
            _ => {}
        }
    }
}

/// Collect the baselines of visual lines without mistaking the numerator,
/// denominator, or scripts inside a math group for separate paragraph lines.
#[derive(Debug, Clone, Copy)]
struct LineBaselineSample {
    y: f64,
    size: f64,
}

fn collect_visual_line_baselines(
    frame: &Frame,
    current_transform: &kurbo::Affine,
) -> Vec<LineBaselineSample> {
    let mut local = Vec::new();
    let mut deferred_groups = Vec::new();

    for (pos, item) in frame.items() {
        let item_offset = kurbo::Affine::translate((pos.x.to_pt(), pos.y.to_pt()));
        let item_transform = *current_transform * item_offset;
        match item {
            FrameItem::Text(text) => {
                local.push(LineBaselineSample {
                    y: (item_transform * kurbo::Point::ORIGIN).y,
                    size: text.size.to_pt(),
                });
            }
            FrameItem::Group(group) => {
                let group_transform = item_transform * typst_transform_to_affine(&group.transform);
                if group.frame.has_baseline() {
                    let point =
                        group_transform * kurbo::Point::new(0.0, group.frame.baseline().to_pt());
                    local.push(LineBaselineSample {
                        y: point.y,
                        size: group.frame.height().to_pt(),
                    });
                } else {
                    deferred_groups.push((&group.frame, group_transform));
                }
            }
            _ => {}
        }
    }

    if !local.is_empty() {
        return local;
    }

    deferred_groups
        .into_iter()
        .flat_map(|(frame, transform)| collect_visual_line_baselines(frame, &transform))
        .collect()
}

fn effective_line_baselines(mut samples: Vec<LineBaselineSample>) -> Vec<f64> {
    samples.sort_by(|left, right| right.y.total_cmp(&left.y));
    let mut clusters: Vec<LineBaselineSample> = Vec::new();
    for sample in samples {
        if let Some(cluster) = clusters
            .iter_mut()
            .find(|cluster| (cluster.y - sample.y).abs() <= 0.5)
        {
            cluster.size = cluster.size.max(sample.size);
        } else {
            clusters.push(sample);
        }
    }

    // Typst emits scripts as independent text runs. A smaller run close to a
    // larger run belongs to that same mathematical line; a genuine following
    // line is separated by approximately a full em or more.
    clusters
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            let is_script = clusters.iter().enumerate().any(|(other_index, other)| {
                other_index != index
                    && other.size > candidate.size * 1.1
                    && (other.y - candidate.y).abs() < other.size * 0.75
            });
            (!is_script).then_some(candidate.y)
        })
        .collect()
}

fn build_typst_cache_key(
    font_universe: FontUniverseKey,
    source: &str,
    is_math: bool,
    text_font: Option<&str>,
    math_font: Option<&str>,
    text_size: Option<f64>,
    math_size: Option<f64>,
    fill: &Option<peniko::Brush>,
    stroke: &StrokeBrush,
) -> TypstCacheKey {
    TypstCacheKey {
        source: source.to_string(),
        is_math,
        text_font: text_font.map(str::to_string),
        math_font: math_font.map(str::to_string),
        text_size_bits: text_size.map(f64::to_bits),
        math_size_bits: math_size.map(f64::to_bits),
        fill_debug: format!("{fill:?}"),
        stroke_debug: format!("{stroke:?}"),
        font_universe,
    }
}

#[cfg(test)]
fn compile_typst_source(
    font_registry: &FontRegistry,
    source: &str,
    is_math: bool,
    text_font: Option<&str>,
    math_font: Option<&str>,
    text_size: Option<f64>,
    math_size: Option<f64>,
    fill: &Option<peniko::Brush>,
    stroke: &StrokeBrush,
) -> Result<CachedTypstHierarchy, Vec<String>> {
    let (_, resources) = typst_resources_for(font_registry);
    compile_typst_source_with_resources(
        resources, source, is_math, text_font, math_font, text_size, math_size, fill, stroke,
    )
}

#[allow(clippy::too_many_arguments)]
fn compile_typst_source_with_resources(
    resources: Arc<TypstResources>,
    source: &str,
    is_math: bool,
    text_font: Option<&str>,
    math_font: Option<&str>,
    text_size: Option<f64>,
    math_size: Option<f64>,
    fill: &Option<peniko::Brush>,
    stroke: &StrokeBrush,
) -> Result<CachedTypstHierarchy, Vec<String>> {
    // Build optional font directives.
    // Gaanim supplies New Computer Modern for plain text by default; math uses
    // the dedicated New Computer Modern Math face when configured.
    let mut directives = String::new();
    if let Some(family) = text_font {
        directives.push_str(&format!("#set text(font: \"{}\")\n", family));
    }
    if let Some(size) = text_size {
        directives.push_str(&format!("#set text(size: {}pt)\n", size));
    }
    if let Some(family) = math_font {
        let actual_family = if family.eq_ignore_ascii_case("newcmmath") {
            "New Computer Modern Math"
        } else {
            family
        };
        directives.push_str(&format!(
            "#show math.equation: set text(font: \"{}\")\n",
            actual_family
        ));
    }
    if let Some(size) = math_size {
        directives.push_str(&format!(
            "#show math.equation: set text(size: {}pt)\n",
            size
        ));
    }

    let full_source = if is_math {
        format!("{}$ {} $", directives, source)
    } else {
        format!("{}{}", directives, source)
    };

    let world = GaanimTypstWorld::with_resources(&full_source, resources);
    let result = typst::compile::<PagedDocument>(&world);

    for warning in &result.warnings {
        eprintln!("Typst warning: {}", warning.message);
    }

    let document = match result.output {
        Ok(doc) => doc,
        Err(errors) => {
            return Err(errors
                .iter()
                .map(|error| error.message.to_string())
                .collect());
        }
    };

    let mut total_bounds: Option<Bounds3D> = None;
    let root_transform = kurbo::Affine::scale_non_uniform(1.0, -1.0);
    let mut extracted_children = Vec::new();
    let mut line_baselines = Vec::new();

    if let Some(page) = document.pages().first() {
        line_baselines =
            effective_line_baselines(collect_visual_line_baselines(&page.frame, &root_transform));
        let mut char_index_counter = 0;
        extract_frame_items(
            &page.frame,
            &root_transform,
            &mut total_bounds,
            fill,
            stroke,
            &world,
            &mut char_index_counter,
            &mut extracted_children,
        );
    }

    let mut total_bounds = total_bounds.unwrap_or_default();
    let text_center = total_bounds.center();
    let mut centered_children = Vec::with_capacity(extracted_children.len());
    for child in extracted_children {
        centered_children.push(CachedTypstChild {
            path: child.path,
            // Keep bounds in the same local coordinate space as the path.
            // The child transform below centers both for rendering and world
            // bounds propagation; shifting bounds here would apply the offset
            // twice to bounds-driven derived geometry.
            bounds: child.bounds,
            transform: SpatialTransform::new_2d(-text_center.x, -text_center.y),
            fill: child.fill,
            stroke: child.stroke,
            tag: child.tag,
            span: child.span,
        });
    }

    let half_size = total_bounds.size() * 0.5;
    total_bounds = Bounds3D::new(
        DVec3::new(-half_size.x, -half_size.y, 0.0),
        DVec3::new(half_size.x, half_size.y, 0.0),
    );

    for baseline in &mut line_baselines {
        *baseline -= text_center.y;
    }
    line_baselines.sort_by(|left, right| right.total_cmp(left));
    line_baselines.dedup_by(|left, right| (*left - *right).abs() <= 0.5);
    let metrics = TextMetrics {
        first_baseline: line_baselines.first().copied().unwrap_or(0.0),
        line_count: line_baselines.len(),
    };

    Ok(CachedTypstHierarchy {
        parent_bounds: total_bounds,
        metrics,
        children: centered_children,
    })
}

#[allow(clippy::too_many_arguments)]
fn cached_typst_hierarchy(
    font_registry: &FontRegistry,
    source: &str,
    is_math: bool,
    text_font: Option<&str>,
    math_font: Option<&str>,
    text_size: Option<f64>,
    math_size: Option<f64>,
    fill: &Option<peniko::Brush>,
    stroke: &StrokeBrush,
) -> Result<Arc<CachedTypstHierarchy>, Vec<String>> {
    let (font_universe, resources) = typst_resources_for(font_registry);
    let cache_key = build_typst_cache_key(
        font_universe,
        source,
        is_math,
        text_font,
        math_font,
        text_size,
        math_size,
        fill,
        stroke,
    );
    if let Some(cached) = typst_hierarchy_cache()
        .lock()
        .expect("Typst hierarchy cache poisoned")
        .get(&cache_key)
        .cloned()
    {
        return Ok(cached);
    }
    let compiled = Arc::new(compile_typst_source_with_resources(
        resources, source, is_math, text_font, math_font, text_size, math_size, fill, stroke,
    )?);
    typst_hierarchy_cache()
        .lock()
        .expect("Typst hierarchy cache poisoned")
        .insert(cache_key, compiled.clone());
    Ok(compiled)
}

/// Measure Typst vector output without spawning ECS entities.
///
/// Measurement and materialization share the same cache key, so resolving a
/// responsive paragraph warms the exact hierarchy later used for rendering.
#[allow(clippy::too_many_arguments)]
pub fn measure_typst(
    font_registry: &FontRegistry,
    source: &str,
    is_math: bool,
    text_font: Option<&str>,
    math_font: Option<&str>,
    text_size: Option<f64>,
    math_size: Option<f64>,
    fill: Option<peniko::Brush>,
    stroke: StrokeBrush,
) -> Result<Bounds3D, Vec<String>> {
    cached_typst_hierarchy(
        font_registry,
        source,
        is_math,
        text_font,
        math_font,
        text_size,
        math_size,
        &fill,
        &stroke,
    )
    .map(|cached| cached.parent_bounds)
}

fn spawn_cached_typst_hierarchy(
    commands: &mut Commands,
    source: &str,
    parent_id: ObjectId,
    mut next_id_fn: impl FnMut() -> ObjectId,
    child_spans: &mut Vec<HierarchyChild>,
    cached: &CachedTypstHierarchy,
) -> (Entity, Bounds3D) {
    let mut parent_bundle =
        MobjectBundle::new(parent_id, kurbo::BezPath::new(), cached.parent_bounds);
    parent_bundle.tag = ObjectTag(format!("Typst('{}')", source));
    parent_bundle.fill = FillBrush(None);
    let parent_entity = commands.spawn(parent_bundle).id();
    commands
        .entity(parent_entity)
        .insert(TextBaseline(cached.metrics.first_baseline));

    for child in &cached.children {
        let child_id = next_id_fn();
        let mut bundle = MobjectBundle::new(child_id, child.path.clone(), child.bounds);
        bundle.fill = child.fill.clone();
        bundle.stroke = child.stroke.clone();
        bundle.tag = child.tag.clone();
        bundle.transform = child.transform;

        let child_entity = commands.spawn(bundle).id();
        commands.entity(child_entity).insert(child.span);
        commands
            .entity(child_entity)
            .set_parent_in_place(parent_entity);
        child_spans.push(HierarchyChild {
            id: child_id,
            entity: child_entity,
            span: child.span,
            path: Arc::new(child.path.clone()),
            bounds: child.bounds,
            transform: child.transform,
            fill: child.fill.0.clone(),
            stroke: child.stroke.clone(),
        });
    }

    (parent_entity, cached.parent_bounds)
}

/// Compiles a LaTeX-style math formula or Typst markup into a structured hierarchy of visual Mobjects.
pub fn compile_typst_to_hierarchy(
    commands: &mut Commands,
    font_registry: &FontRegistry,
    source: &str,
    is_math: bool,
    text_font: Option<&str>,
    math_font: Option<&str>,
    text_size: Option<f64>,
    math_size: Option<f64>,
    fill: Option<gaanim_core::peniko::Brush>,
    stroke: gaanim_scene::StrokeBrush,
    parent_id: ObjectId,
    next_id_fn: impl FnMut() -> ObjectId,
    child_spans: &mut Vec<HierarchyChild>,
) -> (Entity, Bounds3D, TextMetrics) {
    let cached = match cached_typst_hierarchy(
        font_registry,
        source,
        is_math,
        text_font,
        math_font,
        text_size,
        math_size,
        &fill,
        &stroke,
    ) {
        Ok(cached) => cached,
        Err(errors) => {
            for error in errors {
                eprintln!("Typst compilation error: {error}");
            }
            let bounds = Bounds3D::default();
            let bundle = MobjectBundle::new(parent_id, kurbo::BezPath::new(), bounds);
            let entity = commands.spawn(bundle).id();
            return (entity, bounds, TextMetrics::default());
        }
    };

    let (entity, bounds) = spawn_cached_typst_hierarchy(
        commands,
        source,
        parent_id,
        next_id_fn,
        child_spans,
        &cached,
    );
    (entity, bounds, cached.metrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_math_font_loaded() {
        let registry = FontRegistry::new();
        let world = GaanimTypstWorld::new("", &registry);
        assert!(
            world.book().families().next().is_some(),
            "World font book must not be empty"
        );

        let has_math_font = world
            .book()
            .families()
            .any(|(family, _)| family == "New Computer Modern Math");
        assert!(
            has_math_font,
            "Default Typst math font (New Computer Modern Math) must be loaded in the GaanimTypstWorld"
        );
    }

    #[test]
    fn typst_resources_are_reused_for_the_same_registered_fonts() {
        let registry = FontRegistry::new();
        let (first_key, first) = typst_resources_for(&registry);
        let (second_key, second) = typst_resources_for(&registry);

        assert_eq!(first_key, second_key);
        assert!(
            Arc::ptr_eq(&first, &second),
            "each text should reuse the same Typst resources"
        );
        assert!(std::ptr::eq(first.shared, second.shared));
    }

    #[test]
    fn typst_resource_and_hierarchy_keys_include_registered_font_contents() {
        let first = FontRegistry::new();
        let mut second = FontRegistry::new();
        second.register_font("cache-identity-test", vec![1, 2, 3, 4]);

        let (first_key, first_resources) = typst_resources_for(&first);
        let (second_key, second_resources) = typst_resources_for(&second);

        assert_ne!(first_key, second_key);
        assert!(!Arc::ptr_eq(&first_resources, &second_resources));

        let first_hierarchy_key = build_typst_cache_key(
            first_key,
            "same source",
            false,
            None,
            None,
            None,
            None,
            &None,
            &StrokeBrush::transparent(),
        );
        let second_hierarchy_key = build_typst_cache_key(
            second_key,
            "same source",
            false,
            None,
            None,
            None,
            None,
            &None,
            &StrokeBrush::transparent(),
        );
        assert_ne!(first_hierarchy_key, second_hierarchy_key);
    }

    #[test]
    fn typst_math_exposes_a_real_baseline_after_centering() {
        let registry = FontRegistry::new();
        let source = "#set page(width: auto, height: auto, margin: 0pt)\n$W_f$";
        let compiled = compile_typst_source(
            &registry,
            source,
            false,
            Some("New Computer Modern"),
            Some("New Computer Modern Math"),
            Some(32.0),
            Some(32.0),
            &Some(peniko::Brush::Solid(peniko::Color::WHITE)),
            &StrokeBrush::transparent(),
        )
        .expect("math label should compile");

        assert!(compiled.metrics.first_baseline.is_finite());
        assert!(compiled.metrics.first_baseline > compiled.parent_bounds.min.y);
        assert!(compiled.metrics.first_baseline < compiled.parent_bounds.max.y);
        assert!(
            compiled.metrics.first_baseline.abs() > 0.1,
            "a subscripted formula baseline must not collapse to its visual center"
        );
        assert_eq!(compiled.metrics.line_count, 1);
    }

    #[test]
    fn typst_child_local_bounds_share_the_path_coordinate_space() {
        let registry = FontRegistry::new();
        let compiled = compile_typst_source(
            &registry,
            "#set page(width: auto, height: auto, margin: 0pt)\n$E = m c^2$",
            false,
            Some("New Computer Modern"),
            Some("New Computer Modern Math"),
            Some(48.0),
            Some(48.0),
            &Some(peniko::Brush::Solid(peniko::Color::WHITE)),
            &StrokeBrush::transparent(),
        )
        .expect("equation should compile");

        for child in compiled.children {
            let path_bounds = child.path.bounding_box();
            assert_eq!(
                child.bounds,
                Bounds3D::new_2d(
                    path_bounds.x0,
                    path_bounds.y0,
                    path_bounds.x1,
                    path_bounds.y1
                ),
                "LocalBounds must describe Path2D before SpatialTransform is applied"
            );
        }
    }

    #[test]
    fn typst_metrics_distinguish_visual_lines_from_math_scripts() {
        let registry = FontRegistry::new();
        let compile = |source: &str| {
            compile_typst_source(
                &registry,
                source,
                false,
                Some("New Computer Modern"),
                Some("New Computer Modern Math"),
                Some(32.0),
                Some(32.0),
                &Some(peniko::Brush::Solid(peniko::Color::WHITE)),
                &StrokeBrush::transparent(),
            )
            .expect("text fixture should compile")
        };

        let equation =
            compile("#set page(width: auto, height: auto, margin: 0pt)\n$frac(x_1^2, y_2)$");
        assert_eq!(equation.metrics.line_count, 1);

        let paragraph = compile(
            "#set page(width: 180pt, height: auto, margin: 0pt)\n#set text(size: 32pt)\nFirst line\\\nSecond line",
        );
        assert_eq!(paragraph.metrics.line_count, 2);
        assert!(paragraph.metrics.first_baseline > 0.0);
    }

    #[test]
    fn typst_metrics_keep_logical_unit_text_lines_distinct() {
        let registry = FontRegistry::new();
        let paragraph = compile_typst_source(
            &registry,
            "#set page(width: auto, height: auto, margin: 0pt)\nFirst line\\\nSecond line",
            false,
            Some("New Computer Modern"),
            Some("New Computer Modern Math"),
            Some(0.475),
            Some(0.475),
            &Some(peniko::Brush::Solid(peniko::Color::WHITE)),
            &StrokeBrush::transparent(),
        )
        .expect("logical-unit paragraph should compile");

        assert_eq!(paragraph.metrics.line_count, 2);
    }
}
