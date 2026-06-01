use bevy::prelude::{BuildChildrenTransformExt, Commands, Entity};
use gaanim_core::{ObjectId, kurbo, peniko};
use gaanim_math::Bounds3D;
use gaanim_objects::prelude::MobjectBundle;
use gaanim_scene::{FillBrush, ObjectTag, StrokeBrush};

use crate::font::{FontRegistry, OutlineCollector};

// Typst imports
use typst::{
    Library, LibraryExt, World,
    diag::FileError,
    foundations::{Bytes, Datetime},
    layout::{Frame, FrameItem, PagedDocument, Transform},
    syntax::{FileId, Source, VirtualPath},
    text::{Font, FontBook},
    utils::LazyHash,
    visualize::{CurveItem as TypstCurveItem, FixedStroke, Geometry, LineCap, LineJoin, Paint},
};

use kurbo::{Cap, Join, Shape};

/// A custom self-contained implementation of `typst::World` for math and document vector compilation.
pub struct GaanimTypstWorld {
    source: Source,
    fonts: Vec<Font>,
    font_book: LazyHash<FontBook>,
    library: LazyHash<Library>,
    main_id: FileId,
}

impl GaanimTypstWorld {
    /// Creates a new `GaanimTypstWorld` with the user source, Typst default fonts,
    /// system fonts, and any additional fonts registered in the `FontRegistry`.
    pub fn new(source_code: &str, font_registry: &FontRegistry) -> Self {
        let main_id = FileId::new_fake(VirtualPath::new("/main.typ"));
        let source = Source::new(main_id, source_code.to_string());

        // 1. Load Typst embedded defaults + system fonts via typst-kit.
        let mut searcher = typst_kit::fonts::FontSearcher::new();
        searcher.include_system_fonts(true);
        searcher.include_embedded_fonts(true);
        let kit_fonts = searcher.search();

        let mut font_book = kit_fonts.book;
        let mut fonts = Vec::new();
        for slot in &kit_fonts.fonts {
            if let Some(font) = slot.get() {
                fonts.push(font);
            }
        }

        // 2. Append any extra fonts the user registered manually.
        for (_, bytes) in &font_registry.fonts {
            if let Some(font) = Font::new(Bytes::new(bytes.clone()), 0) {
                font_book.push(font.info().clone());
                fonts.push(font);
            }
        }

        if fonts.is_empty() {
            bevy::prelude::warn!(
                "GaanimTypstWorld: no fonts available. \
                 Typst compilation will fail with 'no font could be found'."
            );
        }

        let font_book = LazyHash::new(font_book);
        let library = LazyHash::new(Library::builder().build());

        Self {
            source,
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
            Err(FileError::NotFound(id.vpath().as_rooted_path().into()))
        }
    }

    fn file(&self, id: FileId) -> typst::diag::FileResult<Bytes> {
        Err(FileError::NotFound(id.vpath().as_rooted_path().into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        None
    }
}

/// Convert a Typst `Paint` into an optional `peniko::Brush`, overriding default black with `default_brush` if provided.
fn typst_paint_to_brush(
    paint: &Paint,
    default_brush: &Option<peniko::Brush>,
) -> Option<peniko::Brush> {
    match paint {
        Paint::Solid(color) => {
            let [r, g, b, a] = color.to_vec4_u8();
            // Typst uses black (#000000) as its default document color.
            // In Gaanim, we want defaults to match the parent's default_fill (often white on a dark background).
            if r == 0 && g == 0 && b == 0 && a == 255 {
                if let Some(db) = default_brush {
                    return Some(db.clone());
                }
            }
            Some(peniko::Brush::Solid(peniko::Color::from_rgba8(r, g, b, a)))
        }
        _ => None,
    }
}

/// Convert a Typst 2D `Transform` into a `kurbo::Affine`.
fn typst_transform_to_affine(transform: &Transform) -> kurbo::Affine {
    kurbo::Affine::new([
        transform.sx.get() as f64,
        transform.ky.get() as f64,
        transform.kx.get() as f64,
        transform.sy.get() as f64,
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
        .with_miter_limit(stroke.miter_limit.into())
}

/// Recursively extract vector items from a Typst `Frame` into Gaanim Mobject entities.
fn extract_frame_items(
    commands: &mut Commands,
    frame: &Frame,
    parent_entity: Entity,
    current_transform: &kurbo::Affine,
    next_id_fn: &mut impl FnMut() -> ObjectId,
    total_bounds: &mut Bounds3D,
    default_fill: &Option<peniko::Brush>,
    default_stroke: &StrokeBrush,
    source: &Source,
    char_index_counter: &mut usize,
    child_spans: &mut Vec<(ObjectId, Entity, gaanim_scene::components::TextSpan)>,
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
                    commands,
                    &group.frame,
                    parent_entity,
                    &new_transform,
                    next_id_fn,
                    total_bounds,
                    default_fill,
                    default_stroke,
                    source,
                    char_index_counter,
                    child_spans,
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

                        let child_id = next_id_fn();
                        let mut bundle = MobjectBundle::new(child_id, path, local_bounds);
                        bundle.fill = FillBrush(fill_brush.clone());
                        bundle.stroke = default_stroke.clone();
                        bundle.tag = ObjectTag("TypstGlyph".into());

                        let child_entity = commands.spawn(bundle).id();

                        // Match glyph to corresponding source char and range
                        let byte_offset = glyph.span.1 as usize;
                        let c = text
                            .text
                            .get(byte_offset..)
                            .and_then(|s| s.chars().next())
                            .unwrap_or('?');

                        let span_range = source.range(glyph.span.0).unwrap_or(0..0);
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

                        commands.entity(child_entity).insert(span.clone());
                        child_spans.push((child_id, child_entity, span));

                        *char_index_counter += 1;

                        commands
                            .entity(child_entity)
                            .set_parent_in_place(parent_entity);

                        *total_bounds = total_bounds.union(&local_bounds);
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

                let child_id = next_id_fn();
                let mut bundle = MobjectBundle::new(child_id, path, local_bounds);
                bundle.fill = FillBrush(
                    shape
                        .fill
                        .as_ref()
                        .and_then(|p| typst_paint_to_brush(p, default_fill)),
                );
                if let Some(stroke) = &shape.stroke {
                    bundle.stroke = StrokeBrush {
                        brush: typst_paint_to_brush(&stroke.paint, default_fill),
                        style: typst_stroke_to_kurbo(stroke),
                    };
                }
                bundle.tag = ObjectTag("TypstShape".into());

                let child_entity = commands.spawn(bundle).id();

                let span_range = source.range(*_span).unwrap_or(0..0);
                let span = gaanim_scene::components::TextSpan {
                    character: '_', // Marker for drawing shapes
                    char_index: *char_index_counter,
                    source_range: core::range::Range {
                        start: span_range.start,
                        end: span_range.end,
                    },
                };
                commands.entity(child_entity).insert(span.clone());
                child_spans.push((child_id, child_entity, span));
                *char_index_counter += 1;

                commands
                    .entity(child_entity)
                    .set_parent_in_place(parent_entity);

                *total_bounds = total_bounds.union(&local_bounds);
            }
            _ => {}
        }
    }
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
    mut next_id_fn: impl FnMut() -> ObjectId,
    child_spans: &mut Vec<(ObjectId, Entity, gaanim_scene::components::TextSpan)>,
) -> (Entity, Bounds3D) {
    // Build optional font directives.
    // Typst default fonts (LibertinusSerif / NewCMMath) are already loaded in the FontBook,
    // so if the user passes None we let Typst pick its own defaults.
    let mut directives = String::new();
    if let Some(family) = text_font {
        directives.push_str(&format!("#set text(font: \"{}\")\n", family));
    }
    if let Some(size) = text_size {
        directives.push_str(&format!("#set text(size: {}pt)\n", size));
    }
    if is_math {
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
    }

    // Wrap math formulas in Typst math mode if requested.
    let full_source = if is_math {
        format!("{}$ {} $", directives, source)
    } else {
        format!("{}{}", directives, source)
    };

    let world = GaanimTypstWorld::new(&full_source, font_registry);

    let result = typst::compile::<PagedDocument>(&world);

    // Log warnings if any.
    for warning in &result.warnings {
        bevy::prelude::warn!("Typst warning: {}", warning.message);
    }

    let document = match result.output {
        Ok(doc) => doc,
        Err(errors) => {
            for error in &errors {
                bevy::prelude::error!("Typst compilation error: {}", error.message);
            }
            // Fallback: spawn an empty Mobject bundle so the caller doesn't crash.
            let bounds = Bounds3D::default();
            let bundle = MobjectBundle::new(parent_id, kurbo::BezPath::new(), bounds);
            let entity = commands.spawn(bundle).id();
            return (entity, bounds);
        }
    };

    // Spawn parent container
    let parent_path = kurbo::BezPath::new();
    let parent_bounds = Bounds3D::default();
    let mut parent_bundle = MobjectBundle::new(parent_id, parent_path, parent_bounds);
    parent_bundle.tag = ObjectTag(format!("Typst('{}')", source));
    parent_bundle.fill = FillBrush(None);

    let parent_entity = commands.spawn(parent_bundle).id();

    let mut total_bounds = Bounds3D::new_2d(0.0, 0.0, 0.0, 0.0);
    // The root transform remains IDENTITY because Bevy's world coordinate space
    // inside the Vello canvas is natively Y-down.
    let root_transform = kurbo::Affine::IDENTITY;

    // Process the first page only (formulas are typically single-page).
    if let Some(page) = document.pages.first() {
        let source_obj = match world.source(world.main()) {
            Ok(s) => s,
            Err(_) => {
                bevy::prelude::error!("Failed to get main source from Typst world");
                return (parent_entity, total_bounds);
            }
        };
        let mut char_index_counter = 0;
        extract_frame_items(
            commands,
            &page.frame,
            parent_entity,
            &root_transform,
            &mut next_id_fn,
            &mut total_bounds,
            &fill,
            &stroke,
            &source_obj,
            &mut char_index_counter,
            child_spans,
        );
    }

    commands
        .entity(parent_entity)
        .insert(gaanim_scene::LocalBounds(total_bounds));

    (parent_entity, total_bounds)
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

        let has_math_font = world.fonts.iter().any(|font| {
            font.info()
                .families
                .iter()
                .any(|info| info.as_str() == "New Computer Modern Math")
        });
        assert!(
            has_math_font,
            "Default Typst math font (New Computer Modern Math) must be loaded in the GaanimTypstWorld"
        );
    }
}
