use std::sync::Arc;

use bevy::prelude::{BuildChildrenTransformExt, Commands, Entity};
use gaanim_core::kurbo::{Affine, Shape};
use gaanim_core::{ObjectId, glam::DVec3, peniko::Brush};
use gaanim_math::{Bounds3D, SpatialTransform};
use gaanim_objects::prelude::MobjectBundle;
use gaanim_scene::{ObjectTag, StrokeBrush, components::TextSpan};

use crate::font::{FontRegistry, OutlineCollector};

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
    /// Index of the first source character that produced this glyph.
    /// Used to correctly map glyphs back to characters when HarfBuzz
    /// applies ligature or contextual substitution features.
    pub cluster: u32,
}

#[derive(Clone, Debug)]
pub struct HierarchyChild {
    pub id: ObjectId,
    pub entity: Entity,
    pub span: TextSpan,
    pub path: Arc<gaanim_core::kurbo::BezPath>,
    pub bounds: Bounds3D,
    pub transform: SpatialTransform,
    pub fill: Option<Brush>,
    pub stroke: StrokeBrush,
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
            cluster: info.cluster,
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
    child_spans: &mut Vec<HierarchyChild>,
) -> Result<(Entity, Bounds3D), TextError> {
    // 1. Fetch font bytes
    let font_bytes = font_registry
        .get_font(font_family)
        .ok_or(TextError::FontNotFound)?;

    // 2. Shape text with HarfBuzz (RustyBuzz)
    let shaped_glyphs = shape_text(&font_bytes, text);

    // 3. Parse OpenType font outlines using ttf-parser
    let parser_face =
        ttf_parser::Face::parse(&font_bytes, 0).map_err(|_| TextError::FontParseError)?;
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
    let mut total_bounds: Option<Bounds3D> = None;
    let mut spawned_children = Vec::new();

    // Pre-compute character byte offsets for cluster-based lookup.
    // HarfBuzz `cluster` values are byte indices into the source string,
    // so we build a map from byte offset → (char, char_index).
    let char_byte_offsets: Vec<(usize, char)> = text.char_indices().collect();

    // Track the next candidate character index for glyphs that share a
    // cluster value (e.g. combining marks attached to the same base).
    // For ligatures (one glyph, multiple chars) the cluster points to
    // the first consumed character; subsequent glyphs naturally advance
    // past it via the sorted-cluster invariant.
    let mut next_char_idx: usize = 0;

    for glyph in shaped_glyphs.iter() {
        let glyph_id = ttf_parser::GlyphId(glyph.glyph_id as u16);
        let mut collector = OutlineCollector::new();

        // Query OpenType outline for this specific glyph
        if let Some(_bounding_box) = parser_face.outline_glyph(glyph_id, &mut collector) {
            let mut path = collector.path;

            // Apply horizontal Pen offsets and EM scale
            let glyph_x = pen_x + glyph.x_offset;
            let glyph_y = pen_y + glyph.y_offset;

            // Font outlines and gaanim world coordinates are both Y-up.
            path.apply_affine(Affine::scale(scale));

            let path_bounding_rect = path.bounding_box();
            let mut glyph_local_bounds = Bounds3D::new_2d(
                path_bounding_rect.x0,
                path_bounding_rect.y0,
                path_bounding_rect.x1,
                path_bounding_rect.y1,
            );

            // Shift bounds by the Y-up pen position.
            glyph_local_bounds.min.x += glyph_x * scale;
            glyph_local_bounds.max.x += glyph_x * scale;
            glyph_local_bounds.min.y += glyph_y * scale;
            glyph_local_bounds.max.y += glyph_y * scale;

            // Resolve character from HarfBuzz cluster (byte offset).
            let cluster_byte = glyph.cluster as usize;
            let mut char_idx = char_byte_offsets
                .iter()
                .position(|(byte, _)| *byte == cluster_byte)
                .unwrap_or(next_char_idx);
            // Ensure forward progress: if this cluster maps to a
            // character we already passed, use the next unconsumed one.
            if char_idx < next_char_idx {
                char_idx = next_char_idx;
            }
            if char_idx >= char_byte_offsets.len() {
                // No unconsumed character left — skip this glyph.
                pen_x += glyph.x_advance;
                pen_y += glyph.y_advance;
                continue;
            }
            next_char_idx = char_idx + 1;
            let (char_byte_start, c) = char_byte_offsets[char_idx];
            let char_byte_end = char_byte_start + c.len_utf8();

            // Spawn the child letter Mobject
            let char_id = next_id_fn();
            let mut child_bundle = MobjectBundle::new(char_id, path, glyph_local_bounds);
            child_bundle.fill = gaanim_scene::FillBrush(fill.clone());
            child_bundle.stroke = stroke.clone();
            child_bundle.tag = ObjectTag(format!("Char('{}')", c));

            // Offset the child's local transform according to pen advances
            let local_translation =
                gaanim_core::glam::DVec3::new(glyph_x * scale, glyph_y * scale, 0.0);
            let child_transform =
                SpatialTransform::new_2d(local_translation.x, local_translation.y);
            child_bundle.transform = child_transform;
            let child_path = child_bundle.path.0.clone();

            let child_entity = commands.spawn(child_bundle).id();
            spawned_children.push((child_entity, local_translation, glyph_local_bounds));

            let span = gaanim_scene::components::TextSpan {
                character: c,
                char_index: char_idx,
                source_range: core::range::Range {
                    start: char_byte_start,
                    end: char_byte_end,
                },
            };

            commands.entity(child_entity).insert(span);
            child_spans.push(HierarchyChild {
                id: char_id,
                entity: child_entity,
                span,
                path: child_path,
                bounds: glyph_local_bounds,
                transform: child_transform,
                fill: fill.clone(),
                stroke: stroke.clone(),
            });

            commands
                .entity(child_entity)
                .set_parent_in_place(parent_entity);

            // Accumulate total bounding box of the entire text string
            if let Some(tb) = &mut total_bounds {
                *tb = tb.union(&glyph_local_bounds);
            } else {
                total_bounds = Some(glyph_local_bounds);
            }
        }

        // Advance horizontal pen
        pen_x += glyph.x_advance;
        pen_y += glyph.y_advance;
    }

    let mut total_bounds = total_bounds.unwrap_or_default();

    // Centering visual adjustment: Shift all children relative to the text center
    let text_center = total_bounds.center();
    for (child_entity, orig_trans, orig_bounds) in spawned_children {
        let new_trans = orig_trans - text_center;
        commands
            .entity(child_entity)
            .insert(SpatialTransform::new_2d(new_trans.x, new_trans.y));

        let mut new_bounds = orig_bounds;
        new_bounds.min -= text_center;
        new_bounds.max -= text_center;
        commands
            .entity(child_entity)
            .insert(gaanim_scene::LocalBounds(new_bounds));
    }

    for child in child_spans.iter_mut() {
        child.transform.translation -= text_center;
        child.bounds.min -= text_center;
        child.bounds.max -= text_center;
    }

    // Shift total_bounds to be centered at origin
    let half_size = total_bounds.size() * 0.5;
    total_bounds = Bounds3D::new(
        DVec3::new(-half_size.x, -half_size.y, 0.0),
        DVec3::new(half_size.x, half_size.y, 0.0),
    );

    // 6. Update parent Mobject bounds with the union of all child letter boundaries
    commands
        .entity(parent_entity)
        .insert(gaanim_scene::LocalBounds(total_bounds));

    Ok((parent_entity, total_bounds))
}

/// Shapes and compiles text directly into a single BezPath outline.
pub fn compile_text_to_path(
    font_registry: &FontRegistry,
    text: &str,
    font_family: &str,
    font_size: f64,
) -> Result<(gaanim_core::kurbo::BezPath, Bounds3D), TextError> {
    let font_bytes = font_registry
        .get_font(font_family)
        .ok_or(TextError::FontNotFound)?;

    let shaped_glyphs = shape_text(&font_bytes, text);
    let parser_face =
        ttf_parser::Face::parse(&font_bytes, 0).map_err(|_| TextError::FontParseError)?;
    let units_per_em = parser_face.units_per_em() as f64;
    let scale = font_size / units_per_em;

    let mut merged_path = gaanim_core::kurbo::BezPath::new();
    let mut total_bounds = Bounds3D::new_2d(0.0, 0.0, 0.0, 0.0);
    let mut pen_x = 0.0;
    let mut pen_y = 0.0;

    for glyph in shaped_glyphs.iter() {
        let glyph_id = ttf_parser::GlyphId(glyph.glyph_id as u16);
        let mut collector = OutlineCollector::new();

        if let Some(_bounding_box) = parser_face.outline_glyph(glyph_id, &mut collector) {
            let mut path = collector.path;
            let glyph_x = pen_x + glyph.x_offset;
            let glyph_y = pen_y + glyph.y_offset;

            // Apply horizontal pen offsets
            path.apply_affine(Affine::translate((glyph_x, glyph_y)));
            // Scale and flip outline
            path.apply_affine(Affine::scale_non_uniform(scale, -scale));

            let path_bounding_rect = path.bounding_box();
            let glyph_local_bounds = Bounds3D::new_2d(
                path_bounding_rect.x0,
                path_bounding_rect.y0,
                path_bounding_rect.x1,
                path_bounding_rect.y1,
            );

            // Accumulate
            merged_path.extend(path);
            total_bounds = total_bounds.union(&glyph_local_bounds);
        }

        pen_x += glyph.x_advance;
        pen_y += glyph.y_advance;
    }

    Ok((merged_path, total_bounds))
}
