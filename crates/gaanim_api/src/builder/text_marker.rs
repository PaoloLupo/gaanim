//! Marker-style highlighter behind text selections (AN-02).
//!
//! A marker is one slightly tilted band per rendered line spanned by the
//! selection. Each band is a thick, butt-capped stroke along a straight
//! centerline, so a plain [`PropertyLensSpec::PathTrim`] sweep grows it from
//! the left edge like a highlighter pen. Bands are placed behind the glyphs
//! with the text's stack level and a creation order just before the first
//! glyph, which keeps shapes authored under the text (cards, panels) below it.
//!
//! [`PropertyLensSpec::PathTrim`]: gaanim_timeline::clip::PropertyLensSpec::PathTrim

use super::SceneBuilder;
use crate::anim::{AnimationBuilder, AnimationType};
use gaanim_core::ObjectId;
use gaanim_core::glam::DVec2;
use gaanim_core::kurbo;
use gaanim_core::peniko::{Brush, Color};
use std::collections::{HashMap, HashSet};

/// Default translucent highlighter yellow.
pub const DEFAULT_MARKER_COLOR: Color = Color::from_rgb8(0xFF, 0xDC, 0x3C);
/// Default marker opacity multiplied into the color's alpha.
pub const DEFAULT_MARKER_OPACITY: f32 = 0.45;
/// Default tilt in radians (about 2.9 degrees, rising to the right).
pub const DEFAULT_MARKER_SKEW: f64 = 0.05;

/// Compositing requested for a text marker.
///
/// Per-object blend modes (FX-07) are not implemented yet. `Multiply` is
/// accepted and currently composited like `Normal`: the band is always drawn
/// behind the glyphs, so text stays crisp and the result reads like a
/// multiplied highlighter on light backgrounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarkerBlend {
    #[default]
    Normal,
    Multiply,
}

impl MarkerBlend {
    /// Parses the public names `"normal"` and `"multiply"`.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "normal" => Some(Self::Normal),
            "multiply" => Some(Self::Multiply),
            _ => None,
        }
    }
}

/// Appearance of a marker highlight drawn behind a text selection.
#[derive(Debug, Clone, PartialEq)]
pub struct TextMarkerStyle {
    /// Band color; its alpha is multiplied by `opacity`.
    pub color: Color,
    /// Opacity in `0..=1` multiplied into the color's alpha.
    pub opacity: f32,
    /// Tilt of each band in radians, positive rising to the right. The rise is
    /// capped at a quarter of the band thickness so long lines stay covered.
    pub skew: f64,
    /// Padding around each line's glyphs in world units; `None` uses 10% of
    /// the line height.
    pub padding: Option<f64>,
    /// Requested compositing; see [`MarkerBlend`].
    pub blend: MarkerBlend,
    /// Stack level of the marked text, resolved while compiling a canvas.
    pub(crate) text_z_index: i32,
}

impl Default for TextMarkerStyle {
    fn default() -> Self {
        Self {
            color: DEFAULT_MARKER_COLOR,
            opacity: DEFAULT_MARKER_OPACITY,
            skew: DEFAULT_MARKER_SKEW,
            padding: None,
            blend: MarkerBlend::Normal,
            text_z_index: 0,
        }
    }
}

impl TextMarkerStyle {
    /// Returns the style with `text_z_index` replaced; used by the canvas
    /// compiler once the marked text's stack level is known.
    pub(crate) fn with_text_z_index(mut self, z_index: i32) -> Self {
        self.text_z_index = z_index;
        self
    }
}

/// World-space box of one rendered glyph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GlyphBox {
    pub min: DVec2,
    pub max: DVec2,
    pub selected: bool,
}

/// One marker band: a centerline stroked with `thickness`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MarkerBand {
    pub start: DVec2,
    pub end: DVec2,
    pub thickness: f64,
}

#[derive(Debug, Clone, Copy)]
struct LineSpan {
    min_y: f64,
    max_y: f64,
    selected_x: Option<(f64, f64)>,
}

/// Groups glyphs into rendered lines by vertical overlap and returns one band
/// per line that contains selected glyphs, ordered from the top line down.
///
/// A line's band spans the selected glyphs horizontally and the full height of
/// every glyph on that line, so all words of a line get the same thickness.
pub(crate) fn marker_bands(
    glyphs: &[GlyphBox],
    padding: Option<f64>,
    skew: f64,
) -> Vec<MarkerBand> {
    let mut order: Vec<&GlyphBox> = glyphs
        .iter()
        .filter(|glyph| {
            glyph.min.is_finite()
                && glyph.max.is_finite()
                && glyph.max.x > glyph.min.x
                && glyph.max.y > glyph.min.y
        })
        .collect();
    order.sort_by(|a, b| (b.min.y + b.max.y).total_cmp(&(a.min.y + a.max.y)));

    let mut lines: Vec<LineSpan> = Vec::new();
    for glyph in order {
        let height = glyph.max.y - glyph.min.y;
        let best = lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let overlap = glyph.max.y.min(line.max_y) - glyph.min.y.max(line.min_y);
                (index, overlap)
            })
            .filter(|(_, overlap)| *overlap >= 0.35 * height)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index);
        let line = match best {
            Some(index) => &mut lines[index],
            None => {
                lines.push(LineSpan {
                    min_y: glyph.min.y,
                    max_y: glyph.max.y,
                    selected_x: None,
                });
                lines.last_mut().expect("line was just pushed")
            }
        };
        line.min_y = line.min_y.min(glyph.min.y);
        line.max_y = line.max_y.max(glyph.max.y);
        if glyph.selected {
            line.selected_x = Some(match line.selected_x {
                Some((low, high)) => (low.min(glyph.min.x), high.max(glyph.max.x)),
                None => (glyph.min.x, glyph.max.x),
            });
        }
    }
    lines.sort_by(|a, b| (b.min_y + b.max_y).total_cmp(&(a.min_y + a.max_y)));

    let skew = if skew.is_finite() { skew } else { 0.0 };
    lines
        .into_iter()
        .filter_map(|line| {
            let (low, high) = line.selected_x?;
            let height = line.max_y - line.min_y;
            let pad = padding
                .filter(|pad| pad.is_finite())
                .unwrap_or(0.1 * height)
                .max(0.0);
            let thickness = height + 2.0 * pad;
            let half_length = 0.5 * (high - low) + pad;
            let rise = (skew.tan() * half_length).clamp(-0.25 * thickness, 0.25 * thickness);
            let center = DVec2::new(0.5 * (low + high), 0.5 * (line.min_y + line.max_y));
            Some(MarkerBand {
                start: center + DVec2::new(-half_length, -rise),
                end: center + DVec2::new(half_length, rise),
                thickness,
            })
        })
        .collect()
}

/// Splits `duration` across consecutive bands in proportion to their lengths,
/// so the marker sweeps at a steady pace from one line to the next. Returns
/// `(delay, duration)` pairs relative to the animation start.
pub(crate) fn band_schedule(lengths: &[f64], duration: f64) -> Vec<(f64, f64)> {
    let duration = duration.max(0.0);
    let total: f64 = lengths.iter().map(|length| length.max(0.0)).sum();
    let mut cursor = 0.0;
    lengths
        .iter()
        .map(|length| {
            let share = if total > 0.0 {
                duration * length.max(0.0) / total
            } else {
                duration / lengths.len() as f64
            };
            let slot = (cursor, share);
            cursor += share;
            slot
        })
        .collect()
}

/// Creation order that sorts the marker before the first glyph of `root`
/// while staying after anything authored before the text.
fn marker_creation_order(root: ObjectId, glyphs: &[ObjectId]) -> u64 {
    let root_order = root.index() as u64;
    match glyphs.iter().map(|id| id.index() as u64).min() {
        Some(first) if first <= root_order => first.saturating_sub(1),
        _ => root_order,
    }
}

/// Sum of the authored `z_index` of `id` and every canvas group containing
/// it, matching how the renderer stacks a text's glyphs.
pub(crate) fn stacked_spec_z_index(
    object_specs: &HashMap<ObjectId, crate::canvas::ObjectSpec>,
    id: ObjectId,
) -> i32 {
    let mut z_index = 0i32;
    let mut current = id;
    let mut visited = HashSet::new();
    while visited.insert(current) {
        let Some(spec) = object_specs.get(&current) else {
            break;
        };
        z_index = z_index.saturating_add(spec.z_index);
        let parent = object_specs
            .values()
            .find_map(|candidate| match &candidate.kind {
                crate::canvas::SpawnKind::Group(children) if children.contains(&current) => {
                    Some(candidate.id)
                }
                _ => None,
            });
        match parent {
            Some(parent) => current = parent,
            None => break,
        }
    }
    z_index
}

impl<'w, 's, 'a> SceneBuilder<'w, 's, 'a> {
    /// Spawns one marker band per selected line behind the glyphs and sweeps
    /// them in reading order within `anim`'s duration.
    pub(super) fn play_text_selection_marker_internal(
        &mut self,
        anim: AnimationBuilder,
        selected: &[ObjectId],
        style: TextMarkerStyle,
    ) {
        let selected_set: HashSet<ObjectId> = selected.iter().copied().collect();
        let mut glyph_ids: Vec<ObjectId> = self
            .states
            .get(anim.target)
            .map(|state| state.child_spans.iter().map(|child| child.id).collect())
            .unwrap_or_default();
        if glyph_ids.is_empty() {
            glyph_ids = selected.to_vec();
        }
        let glyphs: Vec<GlyphBox> = glyph_ids
            .iter()
            .filter_map(|id| {
                let state = self.states.get(*id)?;
                if state.path.elements().is_empty() {
                    return None;
                }
                let world = state
                    .bounds
                    .transform_2d(&self.get_world_transform(*id).to_affine_2d());
                Some(GlyphBox {
                    min: world.min.truncate(),
                    max: world.max.truncate(),
                    selected: selected_set.contains(id),
                })
            })
            .collect();
        let bands = marker_bands(&glyphs, style.padding, style.skew);
        if bands.is_empty() {
            bevy::prelude::warn!("text marker found no visible glyphs to highlight");
            return;
        }

        let color = style.color.multiply_alpha(style.opacity.clamp(0.0, 1.0));
        let creation_order = marker_creation_order(anim.target, &glyph_ids);
        let lengths: Vec<f64> = bands
            .iter()
            .map(|band| band.start.distance(band.end))
            .collect();
        let schedule = band_schedule(&lengths, anim.duration);
        for (band, (delay, duration)) in bands.into_iter().zip(schedule) {
            let marker = self
                .line(
                    kurbo::Point::new(band.start.x, band.start.y),
                    kurbo::Point::new(band.end.x, band.end.y),
                )
                .no_fill()
                .stroke_with_style(
                    Brush::Solid(color),
                    kurbo::Stroke::new(band.thickness).with_caps(kurbo::Cap::Butt),
                )
                .spawn();
            if let Some(state) = self.states.get(marker.id) {
                // Hidden until its sweep starts, as `create` does.
                self.commands.entity(state.entity).insert((
                    gaanim_scene::RenderOrder {
                        z_index: style.text_z_index,
                        creation_order,
                    },
                    gaanim_scene::components::Path2D(std::sync::Arc::new(kurbo::BezPath::new())),
                ));
            }
            self.path_trims.insert(marker.id, ([0.0, 0.0, 0.0], false));
            self.play_internal(AnimationBuilder {
                target: marker.id,
                anim_type: AnimationType::PathTrim {
                    start: Some(0.0),
                    end: Some(1.0),
                    offset: Some(0.0),
                    sequential: Some(false),
                },
                duration,
                rate_func: anim.rate_func.clone(),
                delay: anim.delay + delay,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(x: f64, y: f64, width: f64, height: f64, selected: bool) -> GlyphBox {
        GlyphBox {
            min: DVec2::new(x, y),
            max: DVec2::new(x + width, y + height),
            selected,
        }
    }

    #[test]
    fn one_band_spans_selected_glyphs_with_full_line_height() {
        let glyphs = [
            glyph(0.0, 0.0, 0.2, 0.5, false),
            glyph(0.3, 0.0, 0.2, 0.3, true),
            glyph(0.6, -0.1, 0.2, 0.4, true),
            glyph(0.9, 0.0, 0.2, 0.3, false),
        ];
        let bands = marker_bands(&glyphs, Some(0.05), 0.0);
        assert_eq!(bands.len(), 1);
        let band = bands[0];
        assert!((band.start.x - 0.25).abs() < 1e-9);
        assert!((band.end.x - 0.85).abs() < 1e-9);
        // Line spans -0.1..0.5 (all glyphs), plus padding on both sides.
        assert!((band.thickness - 0.7).abs() < 1e-9);
        assert!((band.start.y - 0.2).abs() < 1e-9);
        assert_eq!(band.start.y, band.end.y);
    }

    #[test]
    fn multi_line_selection_yields_one_band_per_line_top_first() {
        let glyphs = [
            glyph(0.0, 0.0, 0.2, 0.4, false),
            glyph(0.3, -0.05, 0.2, 0.45, true),
            // Second line, below the first.
            glyph(0.0, -0.8, 0.2, 0.4, true),
            glyph(0.3, -0.8, 0.2, 0.4, false),
            // Third line is not selected.
            glyph(0.0, -1.6, 0.2, 0.4, false),
        ];
        let bands = marker_bands(&glyphs, None, 0.0);
        assert_eq!(bands.len(), 2);
        assert!(bands[0].start.y > bands[1].start.y);
        assert!((bands[0].start.x - (0.3 - 0.045)).abs() < 1e-9);
        assert!((bands[1].end.x - (0.2 + 0.04)).abs() < 1e-9);
    }

    #[test]
    fn skew_tilts_band_and_is_capped_for_long_lines() {
        let short = marker_bands(&[glyph(0.0, 0.0, 1.0, 0.4, true)], Some(0.0), 0.05);
        let rise = short[0].end.y - short[0].start.y;
        assert!((rise - 2.0 * 0.05f64.tan() * 0.5).abs() < 1e-9);

        let long = marker_bands(&[glyph(0.0, 0.0, 40.0, 0.4, true)], Some(0.0), 0.05);
        let rise = long[0].end.y - long[0].start.y;
        assert!((rise - 2.0 * 0.25 * 0.4).abs() < 1e-9);

        let negative = marker_bands(&[glyph(0.0, 0.0, 1.0, 0.4, true)], Some(0.0), -0.05);
        assert!(negative[0].end.y < negative[0].start.y);
    }

    #[test]
    fn degenerate_glyphs_are_ignored() {
        let glyphs = [
            glyph(0.0, 0.0, 0.0, 0.0, true),
            glyph(0.5, 0.0, 0.2, f64::NAN, true),
        ];
        assert!(marker_bands(&glyphs, None, 0.05).is_empty());
    }

    #[test]
    fn schedule_splits_duration_by_band_length() {
        let schedule = band_schedule(&[3.0, 1.0], 2.0);
        assert_eq!(schedule, vec![(0.0, 1.5), (1.5, 0.5)]);
        assert_eq!(
            band_schedule(&[1.0, 1.0], 0.0),
            vec![(0.0, 0.0), (0.0, 0.0)]
        );
        assert_eq!(
            band_schedule(&[0.0, 0.0], 1.0),
            vec![(0.0, 0.5), (0.5, 0.5)]
        );
    }

    fn compile_marker_scene(
        build: impl FnOnce(&mut crate::canvas::SceneModel),
    ) -> (bevy::prelude::World, gaanim_timeline::timeline::Timeline) {
        use bevy::ecs::world::CommandQueue;
        use bevy::prelude::{Commands, World};
        let mut canvas = crate::canvas::SceneModel::new(16.0, 9.0);
        build(&mut canvas);
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut timeline = gaanim_timeline::timeline::Timeline::new();
        {
            let mut commands = Commands::new(&mut queue, &world);
            let fonts = gaanim_text::font::FontRegistry::new();
            let text_config = gaanim_text::prelude::TextConfig::default();
            canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        }
        queue.apply(&mut world);
        (world, timeline)
    }

    fn marker_trims(timeline: &gaanim_timeline::timeline::Timeline) -> Vec<(f64, f64)> {
        use gaanim_timeline::clip::{AnimationSpec, ClipPayload, PropertyLensSpec};
        let mut trims: Vec<(f64, f64)> = timeline
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                ClipPayload::Animation(AnimationSpec {
                    lens: PropertyLensSpec::PathTrim { from, to, .. },
                    ..
                }) if from[1] == 0.0 && to[1] == 1.0 => Some((clip.start, clip.duration)),
                _ => None,
            })
            .collect();
        trims.sort_by(|a, b| a.0.total_cmp(&b.0));
        trims
    }

    #[test]
    fn canvas_marker_sweeps_behind_the_glyphs() {
        // Multi-line splitting is covered by `marker_bands` above; selections
        // after a line break do not resolve yet (glyph spans past the first
        // line lose their characters), so this compile test stays on one line.
        let (mut world, timeline) = compile_marker_scene(|canvas| {
            let text = canvas.text("uno dos tres cuatro").z_index(3);
            canvas.play(vec![
                text.select("dos tres")
                    .marker(TextMarkerStyle::default(), 1.0),
            ]);
        });

        let trims = marker_trims(&timeline);
        assert_eq!(trims, vec![(0.0, 1.0)]);

        // Markers share the text's stack level and sort before every glyph.
        let mut query = world.query::<(
            &gaanim_scene::RenderOrder,
            &gaanim_scene::StrokeBrush,
            Option<&gaanim_scene::components::TextSpan>,
        )>();
        let rows: Vec<_> = query
            .iter(&world)
            .map(|(order, stroke, span)| (*order, stroke.style.start_cap, span.is_some()))
            .collect();
        let markers: Vec<_> = rows
            .iter()
            .filter(|(order, cap, glyph)| !glyph && *cap == kurbo::Cap::Butt && order.z_index == 3)
            .collect();
        assert_eq!(markers.len(), trims.len());
        let first_glyph = rows
            .iter()
            .filter(|(_, _, glyph)| *glyph)
            .map(|(order, _, _)| order.creation_order)
            .min()
            .expect("text glyphs");
        assert!(
            markers
                .iter()
                .all(|(order, _, _)| order.creation_order < first_glyph)
        );
    }

    #[test]
    fn immediate_marker_is_a_zero_length_sweep_at_the_cursor() {
        let (_, timeline) = compile_marker_scene(|canvas| {
            let text = canvas.text("uno dos tres");
            canvas.wait(0.5);
            text.select("dos").with_marker(TextMarkerStyle::default());
        });
        assert_eq!(marker_trims(&timeline), vec![(0.5, 0.0)]);
    }

    #[test]
    fn creation_order_precedes_first_glyph() {
        let root = ObjectId::from_parts(10, 1);
        let glyphs = [ObjectId::from_parts(11, 1), ObjectId::from_parts(12, 1)];
        assert_eq!(marker_creation_order(root, &glyphs), 10);
        let late_root = ObjectId::from_parts(20, 1);
        assert_eq!(marker_creation_order(late_root, &glyphs), 10);
    }
}
