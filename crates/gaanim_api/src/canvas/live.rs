//! Live zones: [`SceneModel::live_zone`]. A zone is data: where characters
//! land, how they are launched, the podium places or race they take and the
//! rules they react by. It runs while a presentation takes votes (see
//! `gaanim_animation::live`), and in previews and exports it replays its
//! preview players.

use gaanim_animation::live::{
    Event, Express, Launcher, LiveZone, Place, Race, Rule, Surface, Wave,
};
use gaanim_objects::character::catalog;

use super::SceneModel;
use super::ops::SharedCanvasState;
use super::poll::AudienceHandle;

/// Errors raised while authoring a live zone.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LiveZoneError {
    #[error("{0}")]
    Invalid(String),
    #[error("unknown expression {name:?}; expressions: {known}")]
    Expression { name: String, known: String },
    #[error("the live zone is already closed")]
    AlreadyClosed,
}

/// A zone as authored: its window is `open` to `close` (or the end of the
/// segment where it opened), each a segment index and a local cursor.
#[derive(Debug, Clone)]
pub(crate) struct LiveZoneRecord {
    pub zone: LiveZone,
    pub open: (usize, f64),
    pub close: Option<(usize, f64)>,
}

/// A live zone authored with [`SceneModel::live_zone`].
#[derive(Clone)]
pub struct LiveZoneHandle {
    index: usize,
    state: SharedCanvasState,
}

impl std::fmt::Debug for LiveZoneHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LiveZoneHandle")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

fn positive(value: f64, what: &str) -> Result<f64, LiveZoneError> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(LiveZoneError::Invalid(format!(
            "{what} must be positive, got {value}"
        )))
    }
}

fn finite(point: [f64; 2], what: &str) -> Result<[f64; 2], LiveZoneError> {
    if point.iter().all(|value| value.is_finite()) {
        Ok(point)
    } else {
        Err(LiveZoneError::Invalid(format!(
            "{what} must be finite, got {point:?}"
        )))
    }
}

/// An expression characters can play, checked against the catalog.
pub fn express(name: &str, looped: bool) -> Result<Express, LiveZoneError> {
    if !catalog().expressions().contains(&name) {
        return Err(LiveZoneError::Expression {
            name: name.to_string(),
            known: catalog().expressions().join(", "),
        });
    }
    Ok(Express {
        name: name.to_string(),
        looped,
    })
}

impl SceneModel {
    /// Open a live zone at the cursor: while a presentation takes votes and
    /// the timeline is between here and [`LiveZoneHandle::close`] (or the
    /// end of this segment), each player of `audience` arrives in it as
    /// their character. `bounds` is [x0, y0, x1, y1]; characters are `size`
    /// units tall and fall with `gravity`. Previews and exports show
    /// `preview` players, or the audience's.
    pub fn live_zone(
        &mut self,
        audience: &AudienceHandle,
        bounds: [f64; 4],
        size: f64,
        gravity: f64,
        preview: Option<Vec<String>>,
        preview_every: f64,
    ) -> Result<LiveZoneHandle, LiveZoneError> {
        let [x0, y0, x1, y1] = bounds;
        if !(bounds.iter().all(|value| value.is_finite()) && x1 > x0 && y1 > y0) {
            return Err(LiveZoneError::Invalid(format!(
                "bounds must be [x0, y0, x1, y1] with x1 > x0 and y1 > y0, got {bounds:?}"
            )));
        }
        let size = positive(size, "size")?;
        if !(gravity.is_finite() && gravity >= 0.0) {
            return Err(LiveZoneError::Invalid(format!(
                "gravity must be zero or positive, got {gravity}"
            )));
        }
        let preview_every = positive(preview_every, "preview_every")?;
        let preview = preview.unwrap_or_else(|| audience.preview());
        let mut state = self.state.lock().expect("canvas state poisoned");
        let index = state.live_zones.len();
        let open = (state.active_idx, state.active().cursor);
        state.live_zones.push(LiveZoneRecord {
            zone: LiveZone {
                id: format!("zone{index}"),
                open: 0.0,
                close: 0.0,
                bounds,
                size,
                gravity,
                preview,
                preview_every,
                surfaces: Vec::new(),
                launchers: Vec::new(),
                places: Vec::new(),
                race: None,
                rules: Vec::new(),
            },
            open,
            close: None,
        });
        Ok(LiveZoneHandle {
            index,
            state: self.state.clone(),
        })
    }
}

impl LiveZoneHandle {
    fn edit<T>(&self, change: impl FnOnce(&mut LiveZone) -> T) -> T {
        let mut state = self.state.lock().expect("canvas state poisoned");
        change(&mut state.live_zones[self.index].zone)
    }

    /// The zone as it stands, with its window still unresolved.
    pub fn zone(&self) -> LiveZone {
        self.edit(|zone| zone.clone())
    }

    /// A segment characters land on, tagged for rules. `sink` is how deep
    /// they sink into it, in character heights (water).
    pub fn surface(
        &self,
        from: [f64; 2],
        to: [f64; 2],
        tag: &str,
        sink: f64,
    ) -> Result<(), LiveZoneError> {
        let (from, to) = (finite(from, "from")?, finite(to, "to")?);
        if (to[0] - from[0]).abs() < 1e-9 {
            return Err(LiveZoneError::Invalid(
                "a surface must span some width; vertical surfaces are not supported".into(),
            ));
        }
        if tag.trim().is_empty() {
            return Err(LiveZoneError::Invalid("a surface needs a tag".into()));
        }
        self.edit(|zone| {
            zone.surfaces.push(Surface {
                tag: tag.trim().to_string(),
                from,
                to,
                sink: sink.max(0.0),
            })
        });
        Ok(())
    }

    /// Launch arriving characters from `from`, one every `every` seconds,
    /// at an angle sweeping from `angle.0` to `angle.1` degrees every
    /// `period` seconds, toward `direction` (1 right, -1 left).
    pub fn launcher(
        &self,
        from: [f64; 2],
        angle: (f64, f64),
        period: f64,
        speed: f64,
        every: f64,
        direction: f64,
    ) -> Result<(), LiveZoneError> {
        let from = finite(from, "from")?;
        let speed = positive(speed, "speed")?;
        let every = positive(every, "every")?;
        if !(angle.0.is_finite() && angle.1.is_finite() && period.is_finite() && period >= 0.0) {
            return Err(LiveZoneError::Invalid(format!(
                "angle must be finite and period zero or positive, got {angle:?} and {period}"
            )));
        }
        self.edit(|zone| {
            zone.launchers = vec![Launcher {
                from,
                angle: Wave {
                    min: angle.0,
                    max: angle.1,
                    period,
                },
                speed,
                every,
                direction: if direction < 0.0 { -1.0 } else { 1.0 },
            }]
        });
        Ok(())
    }

    /// The player at `rank` (0 for the leader) stands at `at`, playing
    /// `express` while there.
    pub fn place(
        &self,
        rank: usize,
        at: [f64; 2],
        express: Option<Express>,
    ) -> Result<(), LiveZoneError> {
        let at = finite(at, "at")?;
        self.edit(|zone| {
            zone.places.retain(|place| place.rank != rank);
            zone.places.push(Place { rank, at, express });
        });
        Ok(())
    }

    /// The first `count` players run along bars: rank `r` stands at
    /// `origin + step * r + direction * length * score / leader's score`.
    pub fn race(
        &self,
        origin: [f64; 2],
        step: [f64; 2],
        direction: [f64; 2],
        length: f64,
        count: usize,
    ) -> Result<(), LiveZoneError> {
        let (origin, step, direction) = (
            finite(origin, "origin")?,
            finite(step, "step")?,
            finite(direction, "direction")?,
        );
        let length = positive(length, "length")?;
        if count == 0 {
            return Err(LiveZoneError::Invalid(
                "a race needs at least one lane".into(),
            ));
        }
        self.edit(|zone| {
            zone.race = Some(Race {
                origin,
                step,
                direction,
                length,
                count,
            })
        });
        Ok(())
    }

    /// When `on` happens to a character, it plays `express`.
    pub fn rule(&self, on: Event, express: Express) {
        self.edit(|zone| zone.rules.push(Rule { on, express }));
    }

    /// Stop the zone at the cursor instead of at the end of the segment
    /// where it opened.
    pub fn close(&self) -> Result<(), LiveZoneError> {
        let mut state = self.state.lock().expect("canvas state poisoned");
        let at = (state.active_idx, state.active().cursor);
        let record = &mut state.live_zones[self.index];
        if record.close.is_some() {
            return Err(LiveZoneError::AlreadyClosed);
        }
        record.close = Some(at);
        Ok(())
    }
}
