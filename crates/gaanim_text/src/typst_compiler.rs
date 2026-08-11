use bevy::prelude::{BuildChildrenTransformExt, Commands, Entity};
use gaanim_core::{ObjectId, glam::DVec3, kurbo, peniko};
use gaanim_math::{Bounds3D, SpatialTransform};
use gaanim_objects::prelude::MobjectBundle;
use gaanim_scene::{FillBrush, ObjectTag, StrokeBrush};
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
    children: Vec<CachedTypstChild>,
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
}

static TYPST_HIERARCHY_CACHE: OnceLock<Mutex<HashMap<TypstCacheKey, Arc<CachedTypstHierarchy>>>> =
    OnceLock::new();

fn typst_hierarchy_cache() -> &'static Mutex<HashMap<TypstCacheKey, Arc<CachedTypstHierarchy>>> {
    TYPST_HIERARCHY_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A custom self-contained implementation of `typst::World` for math and document vector compilation.
pub struct GaanimTypstWorld {
    source: Source,
    files: FileStore<UniverseFileLoader>,
    fonts: Vec<Font>,
    font_book: LazyHash<FontBook>,
    library: LazyHash<Library>,
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
        let main_id = FileId::unique(RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("/main.typ").unwrap(),
        ));
        let source = Source::new(main_id, source_code.to_string());

        // 1. Load Typst embedded defaults + system fonts via typst-kit.
        let mut font_store = typst_kit::fonts::FontStore::new();
        font_store.extend(typst_kit::fonts::embedded());
        font_store.extend(typst_kit::fonts::system());

        let mut font_book = font_store.book().clone();
        let mut fonts = Vec::new();
        let mut idx = 0;
        while let Some(font) = font_store.font(idx) {
            fonts.push(font);
            idx += 1;
        }

        // 2. Append any extra fonts the user registered manually
        //    (system fonts are already loaded by `FontSearcher` above).
        for bytes in font_registry.registered.values() {
            if let Some(font) = Font::new(Bytes::new(bytes.clone()), 0) {
                font_book.push(font.info().clone());
                fonts.push(font);
            }
        }

        if fonts.is_empty() {
            eprintln!(
                "GaanimTypstWorld: no fonts available. \
                 Typst compilation will fail with 'no font could be found'."
            );
        }

        let library = LazyHash::new(Library::builder().build());

        Self {
            source,
            files: FileStore::new(UniverseFileLoader::new()),
            fonts,
            font_book,
            library,
            main_id,
        }
    }
}

impl World for GaanimTypstWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.font_book
    }

    fn main(&self) -> FileId {
        self.main_id
    }

    fn source(&self, id: FileId) -> typst::diag::FileResult<Source> {
        if id == self.main_id {
            Ok(self.source.clone())
        } else {
            self.files.source(id)
        }
    }

    fn file(&self, id: FileId) -> typst::diag::FileResult<Bytes> {
        self.files.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
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

fn build_typst_cache_key(
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
    }
}

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

    let world = GaanimTypstWorld::new(&full_source, font_registry);
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

    if let Some(page) = document.pages().first() {
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
        let mut new_bounds = child.bounds;
        new_bounds.min -= text_center;
        new_bounds.max -= text_center;
        centered_children.push(CachedTypstChild {
            path: child.path,
            bounds: new_bounds,
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

    Ok(CachedTypstHierarchy {
        parent_bounds: total_bounds,
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
    let cache_key = build_typst_cache_key(
        source, is_math, text_font, math_font, text_size, math_size, fill, stroke,
    );
    if let Some(cached) = typst_hierarchy_cache()
        .lock()
        .expect("Typst hierarchy cache poisoned")
        .get(&cache_key)
        .cloned()
    {
        return Ok(cached);
    }
    let compiled = Arc::new(compile_typst_source(
        font_registry,
        source,
        is_math,
        text_font,
        math_font,
        text_size,
        math_size,
        fill,
        stroke,
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
) -> (Entity, Bounds3D) {
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
            return (entity, bounds);
        }
    };

    spawn_cached_typst_hierarchy(
        commands,
        source,
        parent_id,
        next_id_fn,
        child_spans,
        &cached,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_math_font_loaded() {
        let registry = FontRegistry::new();
        let world = GaanimTypstWorld::new("", &registry);
        assert!(
            !world.fonts.is_empty(),
            "World fonts list must not be empty"
        );

        let has_math_font = world
            .fonts
            .iter()
            .any(|font| font.info().family.as_str().eq("New Computer Modern Math"));
        assert!(
            has_math_font,
            "Default Typst math font (New Computer Modern Math) must be loaded in the GaanimTypstWorld"
        );
    }
}
