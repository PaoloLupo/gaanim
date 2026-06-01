use bevy::prelude::{Commands, Entity, BuildChildrenTransformExt};
use gaanim_core::ObjectId;
use gaanim_core::kurbo::{Affine, Shape};
use gaanim_math::{Bounds3D, SpatialTransform};
use gaanim_scene::ObjectTag;
use gaanim_objects::prelude::MobjectBundle;

use crate::font::{OutlineCollector, FontRegistry};

#[derive(thiserror::Error, Debug)]
pub enum TextError {
    #[error("Failed to parse OTF/TTF font face")]
    FontParseError,
    #[error("Font family not found in registry")]
    FontNotFound,
}

/// Represents a successfully shaped glyph with font spacing coordinates.
#[derive(Debug, Clone, Copy)]
pub struct ShapedGlyph {
    pub glyph_id: u32,
    pub x_advance: f64,
    pub y_advance: f64,
    pub x_offset: f64,
    pub y_offset: f64,
}

/// Shapes a text string using rustybuzz.
pub fn shape_text(font_bytes: &[u8], text: &str) -> Vec<ShapedGlyph> {
    let face = match rustybuzz::Face::from_slice(font_bytes, 0) {
        Some(f) => f,
        None => return Vec::new(),
    };

    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);

    let output = rustybuzz::shape(&face, &[], buffer);
    let glyph_infos = output.glyph_infos();
    let glyph_positions = output.glyph_positions();

    glyph_infos
        .iter()
        .zip(glyph_positions.iter())
        .map(|(info, pos)| ShapedGlyph {
            glyph_id: info.glyph_id,
            x_advance: pos.x_advance as f64,
            y_advance: pos.y_advance as f64,
            x_offset: pos.x_offset as f64,
            y_offset: pos.y_offset as f64,
        })
        .collect()
}

/// Compiles a plain text string into a parent Mobject entity with individual child letters.
///
/// This registers each letter as a fully animatable `MobjectBundle` with its own
/// local bounds, spatial offset, and distinct `ObjectTag`.
    pub fn compile_text_to_hierarchy(
    commands: &mut Commands,
    font_registry: &FontRegistry,
    text: &str,
    font_family: &str,
    font_size: f64,
    fill: Option<gaanim_core::peniko::Brush>,
    stroke: gaanim_scene::StrokeBrush,
    parent_id: ObjectId,
    mut next_id_fn: impl FnMut() -> ObjectId,
    child_spans: &mut Vec<(ObjectId, Entity, gaanim_scene::components::TextSpan)>,
) -> Result<(Entity, Bounds3D), TextError> {
    // 1. Fetch font bytes
    let font_bytes = font_registry.get_font(font_family)
        .ok_or(TextError::FontNotFound)?;

    // 2. Shape text with HarfBuzz (RustyBuzz)
    let shaped_glyphs = shape_text(font_bytes, text);

    // 3. Parse OpenType font outlines using ttf-parser
    let parser_face = ttf_parser::Face::parse(font_bytes, 0).map_err(|_| TextError::FontParseError)?;
    let units_per_em = parser_face.units_per_em() as f64;
    let scale = font_size / units_per_em;

    // 4. Spawn Parent Mobject (empty path, serving as the layout frame / group anchor)
    let parent_path = gaanim_core::kurbo::BezPath::new();
    let parent_bounds = Bounds3D::default();
    let mut parent_bundle = MobjectBundle::new(parent_id, parent_path, parent_bounds);
    parent_bundle.tag = ObjectTag(format!("Text('{}')", text));
    parent_bundle.fill = gaanim_scene::FillBrush(None); // Parent is just a group container
    
    let parent_entity = commands.spawn(parent_bundle).id();

    // 5. Spawn child letter Mobjects
    let mut pen_x = 0.0;
    let mut pen_y = 0.0;
    let mut total_bounds = Bounds3D::new_2d(0.0, 0.0, 0.0, 0.0);

    for (i, glyph) in shaped_glyphs.iter().enumerate() {
        let glyph_id = ttf_parser::GlyphId(glyph.glyph_id as u16);
        let mut collector = OutlineCollector::new();

        // Query OpenType outline for this specific glyph
        if let Some(_bounding_box) = parser_face.outline_glyph(glyph_id, &mut collector) {
            let mut path = collector.path;

            // Apply horizontal Pen offsets and EM scale
            let glyph_x = pen_x + glyph.x_offset;
            let glyph_y = pen_y + glyph.y_offset;
            
            // Transform path: scale it and vertically flip the outline to correct Y-down space
            path.apply_affine(Affine::scale_non_uniform(scale, -scale));
            
            let path_bounding_rect = path.bounding_box();
            let mut glyph_local_bounds = Bounds3D::new_2d(
                path_bounding_rect.x0,
                path_bounding_rect.y0,
                path_bounding_rect.x1,
                path_bounding_rect.y1,
            );

            // Shift bounds by pen position (with positive Y to align with Bevy's Y-down space)
            glyph_local_bounds.min.x += glyph_x * scale;
            glyph_local_bounds.max.x += glyph_x * scale;
            glyph_local_bounds.min.y += glyph_y * scale;
            glyph_local_bounds.max.y += glyph_y * scale;

            // Spawn the child letter Mobject
            let char_id = next_id_fn();
            let mut child_bundle = MobjectBundle::new(char_id, path, glyph_local_bounds);
            child_bundle.fill = gaanim_scene::FillBrush(fill.clone());
            child_bundle.stroke = stroke.clone();
            
            // Try to extract character representation for tag debug readability
            let c = text.chars().nth(i).unwrap_or('?');
            child_bundle.tag = ObjectTag(format!("Char('{}')", c));
            
            // Offset the child's local transform according to pen advances
            child_bundle.transform = SpatialTransform::new_2d(glyph_x * scale, glyph_y * scale);

            let child_entity = commands.spawn(child_bundle).id();
            
            // Calculate UTF-8 byte range of this character in the source text
            let char_byte_start = text.char_indices().nth(i).map(|(idx, _)| idx).unwrap_or(0);
            let char_byte_end = char_byte_start + c.len_utf8();
            
            let span = gaanim_scene::components::TextSpan {
                character: c,
                char_index: i,
                source_range: core::range::Range {
                    start: char_byte_start,
                    end: char_byte_end,
                },
            };
            
            commands.entity(child_entity).insert(span);
            child_spans.push((char_id, child_entity, span));
            
            commands.entity(child_entity).set_parent_in_place(parent_entity);

            // Accumulate total bounding box of the entire text string
            total_bounds = total_bounds.union(&glyph_local_bounds);
        }

        // Advance horizontal pen
        pen_x += glyph.x_advance;
        pen_y += glyph.y_advance;
    }

    // 6. Update parent Mobject bounds with the union of all child letter boundaries
    commands.entity(parent_entity).insert(gaanim_scene::LocalBounds(total_bounds));

    Ok((parent_entity, total_bounds))
}
