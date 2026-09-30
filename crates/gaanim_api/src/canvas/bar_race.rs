//! Bar chart races driven by one keyframe-position [`Parameter`].
//!
//! Every bar is an ordinary drawable whose length, slot, opacity and value
//! label are reactive bindings of the race position, evaluated through the
//! pure [`BarRaceModel`]. A seek therefore shows exactly the frame that
//! continuous playback shows at the same time.

use std::sync::{Arc, Mutex};

use gaanim_animation::{PropertySources, ReactiveFunction, ScalarSource};
use gaanim_core::glam::DVec3;
use gaanim_core::peniko::Color;
use gaanim_text::prelude::{TextFlow, TextSpec, TextStyle, TextWrap};
use gaanim_visualization::{BarRaceModel, BarRaceSlots, BarRaceState, ValueFormat};

use super::visualization::{Parameter, rounded_label_value};
use super::{Anchor, DrawableHandle, SceneModel};

/// Default categorical colors of the bars, cycled in data order.
pub const BAR_RACE_PALETTE: [Color; 10] = [
    Color::from_rgb8(0x4E, 0x79, 0xA7),
    Color::from_rgb8(0xF2, 0x8E, 0x2B),
    Color::from_rgb8(0xE1, 0x57, 0x59),
    Color::from_rgb8(0x76, 0xB7, 0xB2),
    Color::from_rgb8(0x59, 0xA1, 0x4F),
    Color::from_rgb8(0xED, 0xC9, 0x48),
    Color::from_rgb8(0xB0, 0x7A, 0xA1),
    Color::from_rgb8(0xFF, 0x9D, 0xA7),
    Color::from_rgb8(0x9C, 0x75, 0x5F),
    Color::from_rgb8(0xBA, 0xB0, 0xAC),
];

/// Keyframe labels shown by the ticker.
#[derive(Debug, Clone, PartialEq)]
pub enum BarRaceLabels {
    /// Whole numbers such as years; the ticker rolls between them.
    Numeric(Vec<f64>),
    /// Any text; the ticker crossfades to the nearest keyframe's label.
    Text(Vec<String>),
}

impl BarRaceLabels {
    fn len(&self) -> usize {
        match self {
            Self::Numeric(labels) => labels.len(),
            Self::Text(labels) => labels.len(),
        }
    }
}

/// Appearance of a [`SceneModel::bar_race`].
#[derive(Debug, Clone, PartialEq)]
pub struct BarRaceOptions {
    /// Number of ranks shown.
    pub top: usize,
    /// Keyframes over which an overtake is eased; `0.0` swaps instantly.
    pub rank_smoothing: f64,
    pub value_format: ValueFormat,
    /// Size of the whole race, centered on its origin, in scene units.
    pub width: f64,
    pub height: f64,
    /// Width of the name column left of the bars; `None` uses 22% of `width`.
    pub label_width: Option<f64>,
    /// Fraction of each slot left empty between bars, in `[0, 0.9]`.
    pub bar_gap: f64,
    /// Bar colors in data order, cycled; empty uses [`BAR_RACE_PALETTE`].
    pub colors: Vec<Color>,
    /// Name and value color; `None` uses the theme foreground or white.
    pub label_color: Option<Color>,
    /// Name and value size; `None` scales with the bar thickness.
    pub font_size: Option<f64>,
    /// Show the keyframe label in the bottom-right corner.
    pub ticker: bool,
    /// `None` uses 15% of `height`.
    pub ticker_size: Option<f64>,
    /// `None` uses the theme's muted color or a slate gray.
    pub ticker_color: Option<Color>,
}

impl Default for BarRaceOptions {
    fn default() -> Self {
        Self {
            top: 10,
            rank_smoothing: 0.3,
            value_format: ValueFormat::default(),
            width: 10.0,
            height: 6.0,
            label_width: None,
            bar_gap: 0.18,
            colors: Vec::new(),
            label_color: None,
            font_size: None,
            ticker: true,
            ticker_size: None,
            ticker_color: None,
        }
    }
}

impl BarRaceOptions {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [("width", self.width), ("height", self.height)] {
            if !value.is_finite() || value <= 0.0 {
                return Err(format!("bar race {name} must be finite and positive"));
            }
        }
        for (name, value) in [
            ("label_width", self.label_width),
            ("font_size", self.font_size),
            ("ticker_size", self.ticker_size),
        ] {
            if value.is_some_and(|value| !value.is_finite() || value <= 0.0) {
                return Err(format!("bar race {name} must be finite and positive"));
            }
        }
        if self
            .label_width
            .is_some_and(|label_width| label_width >= self.width * 0.8)
        {
            return Err("bar race label_width must leave room for the bars".into());
        }
        if !self.bar_gap.is_finite() || !(0.0..=0.9).contains(&self.bar_gap) {
            return Err("bar race bar_gap must be between 0 and 0.9".into());
        }
        Ok(())
    }
}

/// One bar of a [`BarRace`]: its row and the parts inside it.
#[derive(Debug, Clone)]
pub struct BarRaceBar {
    pub name: String,
    /// Group of the bar, the name and the value; it moves between slots and
    /// fades out below the last shown rank.
    pub row: DrawableHandle,
    pub bar: DrawableHandle,
    pub label: DrawableHandle,
    pub value: DrawableHandle,
}

/// A bar chart race; see [`SceneModel::bar_race`].
#[derive(Debug, Clone)]
pub struct BarRace {
    /// Group of every row and the ticker, with its origin at the race center.
    pub group: DrawableHandle,
    pub bars: Vec<BarRaceBar>,
    /// The keyframe label, if shown.
    pub ticker: Option<DrawableHandle>,
    /// Keyframe position: `0.0` is the first keyframe.
    pub parameter: Parameter,
    pub model: Arc<BarRaceModel>,
    pub labels: BarRaceLabels,
}

impl BarRace {
    pub fn last_position(&self) -> f64 {
        self.model.last_position()
    }

    /// Animate the position to `position` at a constant keyframe rate.
    pub fn to(&self, position: f64) -> Result<super::Anim, String> {
        if !position.is_finite() || position < 0.0 || position > self.last_position() {
            return Err(format!(
                "bar race position must be between 0 and {}",
                self.last_position()
            ));
        }
        Ok(self
            .parameter
            .animate()
            .set(position)
            .rate_func(gaanim_math::RateFunc::Linear))
    }

    /// Animate from the current position to the last keyframe, one second
    /// per keyframe unless `.duration` changes it.
    pub fn play(&self) -> super::Anim {
        let remaining = (self.last_position() - self.parameter.current()).max(0.0);
        self.to(self.last_position())
            .expect("the last keyframe is a valid position")
            .duration(remaining)
    }
}

/// The race state at one position, shared by every binding of a race so a
/// frame evaluates the pairwise ranks once.
#[derive(Debug)]
struct StateCache {
    model: Arc<BarRaceModel>,
    last: Mutex<Option<(u64, Arc<BarRaceState>)>>,
}

impl StateCache {
    fn state(&self, position: f64) -> Arc<BarRaceState> {
        let key = position.to_bits();
        let mut last = self.last.lock().expect("bar race cache poisoned");
        if let Some((cached, state)) = last.as_ref()
            && *cached == key
        {
            return state.clone();
        }
        let state = Arc::new(self.model.state(position));
        *last = Some((key, state.clone()));
        state
    }
}

/// A reactive scalar of the race position, described by `recipe` so hot
/// reload recognizes an unchanged race.
fn position_source(
    parameter: &Parameter,
    recipe: String,
    map: impl Fn(f64) -> f64 + Send + Sync + 'static,
) -> ScalarSource {
    ScalarSource::Function(
        ReactiveFunction::from_sources(0, 1, vec![parameter.source()], move |arguments| {
            Ok(vec![map(arguments[0])])
        })
        .with_recipe(recipe),
    )
}

/// Smallest horizontal scale of a bar, which keeps its transform invertible.
const MIN_BAR_SCALE: f64 = 1e-4;

impl SceneModel {
    /// A bar chart race over `values[frame][bar]`, one keyframe per label.
    ///
    /// Bars are sorted by value in `options.top` slots, the leader on top.
    /// Values interpolate linearly between keyframes, overtakes are eased
    /// over `rank_smoothing` keyframes, bars leaving the top fade out below
    /// the last slot, and the longest bar always spans the bar area. Names
    /// sit in a column left of the bars and rolling values follow each bar's
    /// end. Animate [`BarRace::parameter`], usually with [`BarRace::play`].
    pub fn bar_race(
        &mut self,
        labels: BarRaceLabels,
        names: Vec<String>,
        values: Vec<Vec<f64>>,
        options: BarRaceOptions,
    ) -> Result<BarRace, String> {
        options.validate()?;
        if labels.len() != values.len() {
            return Err(format!(
                "bar race has {} labels for {} keyframes",
                labels.len(),
                values.len()
            ));
        }
        if let BarRaceLabels::Numeric(labels) = &labels
            && labels.iter().any(|label| !label.is_finite())
        {
            return Err("numeric bar race labels must be finite".into());
        }
        let model = Arc::new(
            BarRaceModel::new(names, values, options.top, options.rank_smoothing)
                .map_err(|error| error.to_string())?,
        );
        let format = &options.value_format;
        let value_options = gaanim_animation::RollingNumberOptions {
            decimals: format.decimals,
            group_separator: format.group_separator.map(String::from).unwrap_or_default(),
            prefix: format.prefix.clone(),
            suffix: format.suffix.clone(),
            ..Default::default()
        };
        for frame in 0..model.frame_count() {
            for &value in model.keyframe(frame) {
                value_options.validate_value(value)?;
            }
        }

        let parameter = self.parameter(0.0).map_err(|error| error.to_string())?;
        let cache = Arc::new(StateCache {
            model: model.clone(),
            last: Mutex::new(None),
        });
        let fingerprint = model.fingerprint();
        let slots = BarRaceSlots {
            height: options.height,
            top: options.top,
            gap: options.bar_gap,
        };
        let thickness = slots.bar_thickness();
        let font_size = options.font_size.unwrap_or(thickness * 0.55);
        let pad = font_size * 0.4;
        let label_width = options.label_width.unwrap_or(options.width * 0.22);
        let bar_start = -options.width * 0.5 + label_width;
        // Room right of the longest bar for its value.
        let bar_length = (options.width - label_width) * 0.82;
        let label_color = options
            .label_color
            .or_else(|| self.theme_color("foreground").ok())
            .unwrap_or(Color::WHITE);
        let recipe = |part: &str, bar: usize| {
            format!(
                "bar_race {fingerprint:x} {part} {bar} {:?}",
                (options.height, options.bar_gap, bar_start, bar_length, pad)
            )
        };

        let mut bars = Vec::with_capacity(model.bar_count());
        for (index, name) in model.names().iter().enumerate() {
            let color = if options.colors.is_empty() {
                BAR_RACE_PALETTE[index % BAR_RACE_PALETTE.len()]
            } else {
                options.colors[index % options.colors.len()]
            };
            let bar = self.rect(bar_length, thickness).no_stroke().fill(color);
            let fraction = {
                let cache = cache.clone();
                move |position: f64| cache.state(position).fraction(index).max(MIN_BAR_SCALE)
            };
            let bar = bar
                .bind_property(PropertySources::Scale([
                    position_source(&parameter, recipe("length", index), fraction.clone()),
                    1.0.into(),
                    1.0.into(),
                ]))?
                .bind_property(PropertySources::Translation {
                    values: [bar_start.into(), 0.0.into(), 0.0.into()],
                    anchor: Some(DVec3::new(-1.0, 0.0, 0.0)),
                })?;

            let label = self.text_spec(
                TextSpec::new_with_markup(
                    vec![name.clone().into()],
                    None,
                    TextStyle {
                        size: Some(font_size),
                        weight: Some(600),
                        color: Some(label_color),
                        ..Default::default()
                    },
                    TextFlow {
                        wrap: TextWrap::NoWrap,
                        ..Default::default()
                    },
                    false,
                )
                .map_err(|error| error.to_string())?,
            );
            let label = label.at_anchor(bar_start - pad, 0.0, Anchor::Right);

            let number = self
                .rolling_number(
                    {
                        let cache = cache.clone();
                        position_source(&parameter, recipe("value", index), move |position| {
                            cache.state(position).values[index]
                        })
                    },
                    gaanim_animation::RollingNumberOptions {
                        font_size,
                        ..value_options.clone()
                    },
                )?
                .fill(label_color);
            // Keep the number's start at the bar's end while its width changes.
            let value = self.group(&[&number]);
            value
                .spec
                .lock()
                .expect("bar race value spec poisoned")
                .reactive_readout_layout = Some(super::types::ReactiveReadoutLayoutSpec {
                label: None,
                equals: None,
                number: number.id,
                unit: None,
                spacing: 0.0,
                align: -1.0,
            });
            let value = value.bind_property(PropertySources::Translation {
                values: [
                    position_source(&parameter, recipe("value_x", index), move |position| {
                        bar_start + fraction(position) * bar_length + pad
                    }),
                    0.0.into(),
                    0.0.into(),
                ],
                anchor: None,
            })?;

            let row = self.group_no_center(&[&bar, &label, &value]);
            let row = row
                .bind_property(PropertySources::Translation {
                    values: [
                        0.0.into(),
                        {
                            let cache = cache.clone();
                            position_source(&parameter, recipe("slot", index), move |position| {
                                slots.center_y(cache.state(position).ranks[index])
                            })
                        },
                        0.0.into(),
                    ],
                    anchor: None,
                })?
                .bind_property(PropertySources::Opacity({
                    let cache = cache.clone();
                    let model = model.clone();
                    position_source(&parameter, recipe("opacity", index), move |position| {
                        model.visibility(cache.state(position).ranks[index])
                    })
                }))?;
            bars.push(BarRaceBar {
                name: name.clone(),
                row,
                bar,
                label,
                value,
            });
        }

        let ticker = if options.ticker {
            let size = options.ticker_size.unwrap_or(options.height * 0.15);
            let color = options
                .ticker_color
                .or_else(|| self.theme_color("muted").ok())
                .unwrap_or(Color::from_rgb8(0x94, 0xa3, 0xb8));
            let right = options.width * 0.5;
            let baseline = -options.height * 0.5 + size * 0.6;
            Some(match &labels {
                BarRaceLabels::Numeric(values) => {
                    let values = values.clone();
                    let source = position_source(
                        &parameter,
                        format!("bar_race {fingerprint:x} ticker {values:?}"),
                        move |position| {
                            rounded_label_value(
                                gaanim_visualization::interpolate_label(&values, position),
                                1.0,
                            )
                        },
                    );
                    // Rolling numbers end at their origin, so this right aligns it.
                    self.rolling_number(
                        source,
                        gaanim_animation::RollingNumberOptions {
                            font_size: size,
                            weight: Some(700),
                            ..Default::default()
                        },
                    )?
                    .fill(color)
                    .move_to(right, baseline)
                }
                BarRaceLabels::Text(texts) => {
                    let mut parts = Vec::with_capacity(texts.len());
                    for (frame, text) in texts.iter().enumerate() {
                        let part = self.text_spec(
                            TextSpec::new_with_markup(
                                vec![text.clone().into()],
                                None,
                                TextStyle {
                                    size: Some(size),
                                    weight: Some(700),
                                    color: Some(color),
                                    ..Default::default()
                                },
                                TextFlow {
                                    wrap: TextWrap::NoWrap,
                                    ..Default::default()
                                },
                                false,
                            )
                            .map_err(|error| error.to_string())?,
                        );
                        let part = part.at_anchor(right, baseline, Anchor::Right).opacity(
                            position_source(
                                &parameter,
                                format!("bar_race {fingerprint:x} ticker {frame}"),
                                move |position| {
                                    gaanim_visualization::label_opacity(frame, position, 0.12)
                                },
                            ),
                        );
                        parts.push(part);
                    }
                    self.group_no_center(&parts.iter().collect::<Vec<_>>())
                }
            })
        } else {
            None
        };

        let mut members = bars.iter().map(|bar| &bar.row).collect::<Vec<_>>();
        members.extend(ticker.as_ref());
        let group = self.group_no_center(&members);
        Ok(BarRace {
            group,
            bars,
            ticker,
            parameter,
            model,
            labels,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::World;
    use gaanim_math::SpatialTransform;
    use gaanim_timeline::snapshot::WorldSnapshot;
    use gaanim_timeline::timeline::Timeline;

    fn race(canvas: &mut SceneModel) -> BarRace {
        canvas
            .bar_race(
                BarRaceLabels::Numeric(vec![2000.0, 2001.0, 2002.0]),
                vec!["a".into(), "b".into(), "c".into()],
                vec![
                    vec![30.0, 20.0, 10.0],
                    vec![10.0, 40.0, 20.0],
                    vec![5.0, 50.0, 60.0],
                ],
                BarRaceOptions {
                    top: 2,
                    ..Default::default()
                },
            )
            .unwrap()
    }

    #[test]
    fn bar_races_validate_their_inputs() {
        let mut canvas = SceneModel::new(16.0, 9.0);
        let names = vec!["a".to_owned()];
        assert!(
            canvas
                .bar_race(
                    BarRaceLabels::Numeric(vec![1.0, 2.0]),
                    names.clone(),
                    vec![vec![1.0]],
                    BarRaceOptions::default(),
                )
                .is_err()
        );
        assert!(
            canvas
                .bar_race(
                    BarRaceLabels::Text(vec!["x".into()]),
                    names.clone(),
                    vec![vec![1.0]],
                    BarRaceOptions {
                        bar_gap: 1.0,
                        ..Default::default()
                    },
                )
                .is_err()
        );
        assert!(
            canvas
                .bar_race(
                    BarRaceLabels::Text(vec!["x".into()]),
                    names,
                    vec![vec![1e20]],
                    BarRaceOptions::default(),
                )
                .is_err()
        );
    }

    #[test]
    fn play_runs_to_the_last_keyframe_one_second_per_keyframe() {
        let mut canvas = SceneModel::new(16.0, 9.0);
        let race = race(&mut canvas);
        assert_eq!(race.bars.len(), 3);
        assert!(race.ticker.is_some());
        assert_eq!(race.last_position(), 2.0);
        assert!(race.to(2.5).is_err());
        assert!(race.to(-0.1).is_err());
        assert!(race.to(1.0).is_ok());
        let _ = race.play();
    }

    #[test]
    fn seeking_matches_the_pure_model_at_every_time() {
        let mut canvas = SceneModel::new(16.0, 9.0);
        let race = race(&mut canvas);
        canvas
            .play_items(vec![race.play().duration(2.0).into()])
            .unwrap();
        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        // Find each bound part by the recipe its reactive source carries.
        let bindings = world
            .query::<&gaanim_animation::PropertyBinding>()
            .iter(&world)
            .map(|binding| {
                let description = gaanim_core::fingerprint::with_identity_debug(|| {
                    format!("{:?}", binding.source.sources)
                });
                (binding.target, description)
            })
            .collect::<Vec<_>>();
        let target = |part: &str, index: usize| {
            let needle = format!(" {part} {index} ");
            let matches = bindings
                .iter()
                .filter(|(_, description)| description.contains(&needle))
                .map(|(target, _)| *target)
                .collect::<Vec<_>>();
            assert_eq!(matches.len(), 1, "one {part} binding for bar {index}");
            matches[0]
        };
        let rows = (0..race.bars.len())
            .map(|index| (target("slot", index), target("length", index)))
            .collect::<Vec<_>>();
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        let slots = BarRaceSlots {
            height: 6.0,
            top: 2,
            gap: BarRaceOptions::default().bar_gap,
        };
        // Seek out of order: every time must match the model on its own.
        for time in [1.5, 0.0, 2.0, 0.4, 1.0, 1.5] {
            timeline.seek(&mut world, time);
            let state = race.model.state(time);
            for (index, (row, bar)) in rows.iter().enumerate() {
                let row_y = world.get::<SpatialTransform>(*row).unwrap().translation.y;
                assert!(
                    (row_y - slots.center_y(state.ranks[index])).abs() < 1e-9,
                    "slot of bar {index} at {time}"
                );
                let scale = world.get::<SpatialTransform>(*bar).unwrap().scale.x;
                assert!(
                    (scale - state.fraction(index).max(MIN_BAR_SCALE)).abs() < 1e-9,
                    "length of bar {index} at {time}"
                );
                let opacity = world.get::<gaanim_scene::Opacity>(*row).unwrap().0 as f64;
                assert!(
                    (opacity - race.model.visibility(state.ranks[index])).abs() < 1e-6,
                    "opacity of bar {index} at {time}"
                );
            }
        }
    }
}
