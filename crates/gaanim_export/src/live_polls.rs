//! Poll elements a presented bundle redraws from live results.
//!
//! A bundle replays recorded frames, so what follows live votes has to be
//! redrawn when the bundle is presented: bars, leaderboard nicknames and
//! readouts of poll values. Recording names those elements and stores what
//! redrawing needs (their geometry, and glyph atlases so text needs no font);
//! presenting rebuilds their outlines from the live results and keeps
//! everything else each frame recorded.

use std::collections::HashMap;
use std::io::{Seek, Write};
use std::sync::Arc;

use bevy::prelude::{Entity, World};
use gaanim_animation::polls::{
    BarDirection, BarScale, BarSource, BarSpec, GlyphAtlas, LiveText, LiveTextSource, PollBar,
    PollMeasure, PollResults, PollSource, PollValue, TextAlign,
};
use gaanim_animation::{ReactiveReadout, RollingNumber};
use gaanim_bundle::{
    BarSourceRecord, BundleWriter, GlyphRecord, LiveReadoutRecord, LiveSourceRecord,
    LiveTextRecord, PollBarRecord, RunRecord, SceneData,
};
use gaanim_core::kurbo::{Affine, BezPath};
use gaanim_renderer::pipeline::FrameCapture;
use gaanim_text::font::FontRegistry;

/// Characters a readout may print, besides its `invalid` text.
const NUMBER_CHARACTERS: &str = "0123456789+-.,e%";

fn glyph_records(atlas: &GlyphAtlas) -> Vec<GlyphRecord> {
    let mut glyphs: Vec<GlyphRecord> = atlas
        .glyphs
        .iter()
        .map(|(ch, (path, advance))| GlyphRecord {
            ch: *ch,
            advance: *advance,
            path: path.to_svg(),
        })
        .collect();
    glyphs.sort_by_key(|glyph| glyph.ch);
    glyphs
}

fn atlas_from(glyphs: &[GlyphRecord]) -> GlyphAtlas {
    GlyphAtlas {
        glyphs: glyphs
            .iter()
            .filter_map(|glyph| {
                let path = BezPath::from_svg(&glyph.path).ok()?;
                Some((glyph.ch, (Arc::new(path), glyph.advance)))
            })
            .collect(),
    }
}

fn run_record(run: Option<(BezPath, f64)>) -> RunRecord {
    run.map_or_else(RunRecord::default, |(path, advance)| RunRecord {
        advance,
        path: path.to_svg(),
    })
}

fn run_from(record: &RunRecord) -> (BezPath, f64) {
    (
        BezPath::from_svg(&record.path).unwrap_or_default(),
        record.advance,
    )
}

fn source_record(source: &PollSource) -> LiveSourceRecord {
    match source {
        PollSource::Poll {
            poll,
            answers,
            measure,
        } => {
            let (name, answer, time) = match *measure {
                PollMeasure::Votes(answer) => ("votes", answer, 0.0),
                PollMeasure::Share(answer) => ("share", answer, 0.0),
                PollMeasure::Percent(answer) => ("percent", answer, 0.0),
                PollMeasure::Total => ("total", 0, 0.0),
                PollMeasure::Remaining { time } => ("remaining", 0, time),
            };
            LiveSourceRecord::Poll {
                poll: poll.to_string(),
                answers: *answers,
                measure: name.to_string(),
                answer,
                time,
            }
        }
        PollSource::LeaderScore { rank } => LiveSourceRecord::LeaderScore { rank: *rank },
        PollSource::Players => LiveSourceRecord::Players,
    }
}

fn source_from(record: &LiveSourceRecord) -> Option<PollSource> {
    Some(match record {
        LiveSourceRecord::Poll {
            poll,
            answers,
            measure,
            answer,
            time,
        } => PollSource::Poll {
            poll: poll.as_str().into(),
            answers: *answers,
            measure: match measure.as_str() {
                "votes" => PollMeasure::Votes(*answer),
                "share" => PollMeasure::Share(*answer),
                "percent" => PollMeasure::Percent(*answer),
                "total" => PollMeasure::Total,
                "remaining" => PollMeasure::Remaining { time: *time },
                _ => return None,
            },
        },
        LiveSourceRecord::LeaderScore { rank } => PollSource::LeaderScore { rank: *rank },
        LiveSourceRecord::Players => PollSource::Players,
    })
}

/// Shape `text` as one run, as a readout shapes its prefix and suffix.
fn shape_run(
    registry: &FontRegistry,
    text: &str,
    family: &str,
    weight: Option<u16>,
    size: f64,
) -> Option<(BezPath, f64)> {
    if text.is_empty() {
        return None;
    }
    let run =
        gaanim_text::typst_compiler::shape_typst_text_run(registry, text, family, weight, size)
            .ok()?;
    Some((run.path, run.advance))
}

/// The elements drawn from live poll data in `world`, with the keys
/// `writer` gave their entities.
pub fn record<W: Write + Seek>(
    world: &mut World,
    writer: &mut BundleWriter<W>,
) -> (
    Vec<PollBarRecord>,
    Vec<LiveTextRecord>,
    Vec<LiveReadoutRecord>,
) {
    let bars: Vec<(Entity, PollBar)> = world
        .query::<(Entity, &PollBar)>()
        .iter(world)
        .map(|(entity, bar)| (entity, bar.clone()))
        .collect();
    let texts: Vec<(Entity, LiveText)> = world
        .query::<(Entity, &LiveText)>()
        .iter(world)
        .map(|(entity, text)| (entity, text.clone()))
        .collect();
    let mut readouts = Vec::new();
    for (entity, readout, rolling) in world
        .query::<(Entity, &ReactiveReadout, Option<&RollingNumber>)>()
        .iter(world)
    {
        // Only a readout of one poll value, without Python, can be redrawn.
        let gaanim_animation::reactive::ScalarSource::Signal(id) = &readout.source else {
            continue;
        };
        if rolling.is_some() {
            continue;
        }
        let Some(parameter) = readout
            .parameters
            .iter()
            .find_map(|(logical, parameter)| (logical == id).then_some(*parameter))
        else {
            continue;
        };
        if let Some(value) = world.get::<PollValue>(parameter) {
            readouts.push((entity, readout.clone(), value.source.clone()));
        }
    }

    let bars = bars
        .into_iter()
        .map(|(entity, bar)| PollBarRecord {
            key: writer.entity_key(entity),
            source: match &bar.source {
                BarSource::Answer {
                    poll,
                    answer,
                    answers,
                } => BarSourceRecord::Answer {
                    poll: poll.to_string(),
                    answer: *answer,
                    answers: *answers,
                },
                BarSource::Leader { rank } => BarSourceRecord::Leader { rank: *rank },
            },
            length: bar.spec.length,
            thickness: bar.spec.thickness,
            radius: bar.spec.radius,
            direction: bar.spec.direction.name().to_string(),
            scale: bar.spec.scale.name().to_string(),
        })
        .collect();

    let registry = world.get_resource::<FontRegistry>();
    let characters: String = gaanim_animation::polls::atlas_characters().collect();
    let mut cache = gaanim_animation::polls::GlyphCache::default();
    let texts = texts
        .into_iter()
        .map(|(entity, mut text)| {
            if let Some(registry) = registry {
                text.learn_with(registry, &characters, &mut cache);
            }
            let LiveTextSource::LeaderName { rank } = text.source;
            LiveTextRecord {
                key: writer.entity_key(entity),
                rank,
                align: text.align.name().to_string(),
                glyphs: glyph_records(&text.atlas),
            }
        })
        .collect();
    let readouts = readouts
        .into_iter()
        .map(|(entity, readout, source)| {
            let mut atlas = GlyphAtlas::default();
            let mut prefix = None;
            let mut suffix = None;
            if let Some(registry) = registry {
                let family = &readout.font_family;
                let characters = NUMBER_CHARACTERS
                    .chars()
                    .chain(readout.invalid.chars())
                    .chain([readout.decimal_separator]);
                for ch in characters {
                    if let Some(glyph) = gaanim_animation::polls::shape_glyph(
                        registry,
                        ch,
                        family,
                        readout.font_weight,
                        readout.font_size,
                    ) {
                        atlas.glyphs.insert(ch, glyph);
                    }
                }
                prefix = shape_run(
                    registry,
                    &readout.prefix,
                    family,
                    readout.font_weight,
                    readout.font_size,
                );
                suffix = shape_run(
                    registry,
                    &readout.suffix,
                    family,
                    readout.font_weight,
                    readout.font_size,
                );
            }
            LiveReadoutRecord {
                key: writer.entity_key(entity),
                source: source_record(&source),
                format: readout.format.clone(),
                invalid: readout.invalid.clone(),
                decimal_separator: readout.decimal_separator,
                prefix: run_record(prefix),
                suffix: run_record(suffix),
                glyphs: glyph_records(&atlas),
            }
        })
        .collect();
    (bars, texts, readouts)
}

enum Piece {
    Bar {
        source: BarSource,
        spec: BarSpec,
    },
    Text {
        rank: usize,
        align: TextAlign,
        atlas: GlyphAtlas,
    },
    Readout {
        source: PollSource,
        format: String,
        invalid: String,
        decimal_separator: char,
        prefix: (BezPath, f64),
        suffix: (BezPath, f64),
        atlas: GlyphAtlas,
    },
}

/// The live elements of a bundle, ready to redraw.
#[derive(Default)]
pub struct LiveElements {
    pieces: HashMap<Entity, Piece>,
}

/// What a live element shows, so a player redraws a frame only when that
/// changes rather than whenever the results do (a quiz's clock changes them
/// every frame).
#[derive(Debug, Clone, PartialEq)]
pub enum Shown {
    Fraction(u64),
    Text(Arc<str>),
    Number(String),
}

impl LiveElements {
    pub fn from_scene(scene: &SceneData) -> Self {
        let mut pieces = HashMap::new();
        let entity = |key| gaanim_bundle::element_entity(key).ok();
        for bar in &scene.poll_bars {
            let (Some(entity), Some(direction), Some(scale)) = (
                entity(bar.key),
                BarDirection::from_name(&bar.direction),
                BarScale::from_name(&bar.scale),
            ) else {
                continue;
            };
            let source = match &bar.source {
                BarSourceRecord::Answer {
                    poll,
                    answer,
                    answers,
                } => BarSource::Answer {
                    poll: poll.as_str().into(),
                    answer: *answer,
                    answers: *answers,
                },
                BarSourceRecord::Leader { rank } => BarSource::Leader { rank: *rank },
            };
            let spec = BarSpec {
                length: bar.length,
                thickness: bar.thickness,
                radius: bar.radius,
                direction,
                scale,
            };
            pieces.insert(entity, Piece::Bar { source, spec });
        }
        for text in &scene.poll_texts {
            let (Some(entity), Some(align)) = (entity(text.key), TextAlign::from_name(&text.align))
            else {
                continue;
            };
            pieces.insert(
                entity,
                Piece::Text {
                    rank: text.rank,
                    align,
                    atlas: atlas_from(&text.glyphs),
                },
            );
        }
        for readout in &scene.poll_readouts {
            let (Some(entity), Some(source)) = (entity(readout.key), source_from(&readout.source))
            else {
                continue;
            };
            pieces.insert(
                entity,
                Piece::Readout {
                    source,
                    format: readout.format.clone(),
                    invalid: readout.invalid.clone(),
                    decimal_separator: readout.decimal_separator,
                    prefix: run_from(&readout.prefix),
                    suffix: run_from(&readout.suffix),
                    atlas: atlas_from(&readout.glyphs),
                },
            );
        }
        Self { pieces }
    }

    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    /// What every live element shows with `results`; `None` outside a live
    /// presentation or without live elements.
    pub fn shown(&self, results: &PollResults) -> Option<Vec<Shown>> {
        if !results.live || self.pieces.is_empty() {
            return None;
        }
        self.pieces
            .values()
            .map(|piece| piece.shown(results))
            .collect()
    }

    /// Redraw the live elements of `capture` from `results`; outside a live
    /// presentation the recorded frame stays as it is.
    pub fn apply(&self, results: &PollResults, capture: &mut FrameCapture) {
        if !results.live || self.pieces.is_empty() {
            return;
        }
        for element in &mut capture.elements {
            let Some(piece) = self.pieces.get(&element.entity) else {
                continue;
            };
            let Some(outline) = piece.outline(results) else {
                continue;
            };
            let outline = Arc::new(outline);
            let mut recipe = (*element.recipe).clone();
            recipe.path = Some(outline.clone());
            recipe.source = Some(outline);
            element.recipe = Arc::new(recipe);
        }
    }
}

impl Piece {
    fn shown(&self, results: &PollResults) -> Option<Shown> {
        Some(match self {
            Self::Bar { source, spec } => {
                Shown::Fraction(source.live_fraction(spec, results)?.to_bits())
            }
            Self::Text { rank, .. } => Shown::Text(results.leader_name(*rank)?),
            Self::Readout {
                source,
                format,
                invalid,
                decimal_separator,
                ..
            } => Shown::Number(gaanim_animation::localize_decimal_separator(
                &gaanim_animation::format_reactive_number(results.value(source)?, format, invalid),
                *decimal_separator,
            )),
        })
    }

    fn outline(&self, results: &PollResults) -> Option<BezPath> {
        Some(match (self, self.shown(results)?) {
            (Self::Bar { spec, .. }, Shown::Fraction(bits)) => spec.path(f64::from_bits(bits)),
            (Self::Text { align, atlas, .. }, Shown::Text(name)) => atlas.place(&name, *align).0,
            (
                Self::Readout {
                    prefix,
                    suffix,
                    atlas,
                    ..
                },
                Shown::Number(number),
            ) => {
                // As a readout lays out: the prefix run, each character of
                // the number, the suffix run, then right-aligned and
                // centered on its row.
                let mut path = prefix.0.clone();
                let mut pen = prefix.1;
                let (digits, width) = atlas.layout(&number);
                let mut digits = digits;
                digits.apply_affine(Affine::translate((pen, 0.0)));
                path.extend(digits);
                pen += width;
                let mut tail = suffix.0.clone();
                tail.apply_affine(Affine::translate((pen, 0.0)));
                path.extend(tail);
                let bounds = gaanim_math::Bounds3D::default();
                gaanim_animation::right_align_readout_path(path, bounds).0
            }
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_core::kurbo::{Rect, Shape};

    fn square(size: f64) -> Arc<BezPath> {
        Arc::new(Rect::new(0.0, 0.0, size, size).to_path(1e-3))
    }

    #[test]
    fn sources_and_atlases_round_trip_through_records() {
        for source in [
            PollSource::Poll {
                poll: "q0-1".into(),
                answers: 3,
                measure: PollMeasure::Remaining { time: 20.0 },
            },
            PollSource::Poll {
                poll: "p0-1".into(),
                answers: 3,
                measure: PollMeasure::Percent(2),
            },
            PollSource::LeaderScore { rank: 4 },
            PollSource::Players,
        ] {
            assert_eq!(source_from(&source_record(&source)), Some(source));
        }
        let mut atlas = GlyphAtlas::default();
        atlas.glyphs.insert('Ñ', (square(1.0), 1.25));
        let back = atlas_from(&glyph_records(&atlas));
        assert_eq!(back.glyphs[&'Ñ'].1, 1.25);
        assert_eq!(
            back.glyphs[&'Ñ'].0.bounding_box(),
            Rect::new(0.0, 0.0, 1.0, 1.0)
        );
    }

    #[test]
    fn pieces_redraw_only_while_live() {
        let mut atlas = GlyphAtlas::default();
        for (ch, size) in [('A', 1.0), ('H', 1.0), ('1', 0.5), ('2', 0.5)] {
            atlas.glyphs.insert(ch, (square(size), size + 0.25));
        }
        let text = Piece::Text {
            rank: 0,
            align: TextAlign::Left,
            atlas: atlas.clone(),
        };
        let readout = Piece::Readout {
            source: PollSource::Players,
            format: ".0f".into(),
            invalid: "?".into(),
            decimal_separator: '.',
            prefix: (BezPath::new(), 0.0),
            suffix: (BezPath::new(), 0.0),
            atlas,
        };
        let mut results = PollResults::default();
        assert!(text.outline(&results).is_none());
        assert!(readout.outline(&results).is_none());
        results.live = true;
        results.leaderboard = vec![("AA".into(), 10)];
        results.players = 12;
        let name = text.outline(&results).unwrap().bounding_box();
        assert!((name.x0, name.width()) == (0.0, 2.25));
        // Readouts end at x = 0, like a readout in the scene.
        let count = readout.outline(&results).unwrap().bounding_box();
        assert!(count.x1.abs() < 1e-9 && (count.width() - 1.25).abs() < 1e-9);
    }
}
