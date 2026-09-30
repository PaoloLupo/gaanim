//! Live zones: [`SceneModel::live_zone`]. A zone is data: its bounds, the
//! size of its characters and its behavior, a compiled [`Program`] that
//! poses each player's character from what the engine tells it. It runs
//! while a presentation takes votes (see `gaanim_animation::live`), and in
//! previews and exports it replays its preview players.

use gaanim_animation::live::{LiveZone, Motion, Program, ProgramError};
use gaanim_objects::character::catalog;

use super::SceneModel;
use super::ops::SharedCanvasState;
use super::poll::AudienceHandle;

/// Errors raised while authoring a live zone.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LiveZoneError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Program(#[from] ProgramError),
    #[error("{behavior} plays unknown expression {name:?}; expressions: {known}")]
    Expression {
        behavior: String,
        name: String,
        known: String,
    },
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

/// Check a behavior: it must run here and play only known expressions.
pub fn check_behavior(behavior: Program) -> Result<Program, LiveZoneError> {
    let behavior = behavior.checked()?;
    let known = catalog().expressions();
    if let Some(name) = behavior
        .strings
        .iter()
        .find(|name| !known.contains(&name.as_str()))
    {
        return Err(LiveZoneError::Expression {
            behavior: behavior.name.clone(),
            name: name.clone(),
            known: known.join(", "),
        });
    }
    Ok(behavior)
}

impl SceneModel {
    /// Open a live zone at the cursor: while a presentation takes votes and
    /// the timeline is between here and [`LiveZoneHandle::close`] (or the
    /// end of this segment), each player of `audience` arrives in it as
    /// their character, posed every frame by `behavior`. `bounds` is
    /// [x0, y0, x1, y1]; characters are `size` units tall at scale 1, and
    /// `motion` deforms them from how they move. Previews and exports show
    /// `preview` players, or the audience's.
    pub fn live_zone(
        &mut self,
        audience: &AudienceHandle,
        behavior: Program,
        bounds: [f64; 4],
        size: f64,
        motion: Motion,
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
        let preview_every = positive(preview_every, "preview_every")?;
        let settings = [
            (motion.squash, "squash"),
            (motion.max_stretch, "max_stretch"),
            (motion.lean, "lean"),
            (motion.max_lean, "max_lean"),
        ];
        if let Some((value, name)) = settings
            .iter()
            .find(|(value, _)| !(value.is_finite() && *value >= 0.0))
        {
            return Err(LiveZoneError::Invalid(format!(
                "{name} must be zero or positive, got {value}"
            )));
        }
        let behavior = check_behavior(behavior)?;
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
                preview,
                preview_every,
                behavior,
                motion,
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
    /// The zone as it stands, with its window still unresolved.
    pub fn zone(&self) -> LiveZone {
        let state = self.state.lock().expect("canvas state poisoned");
        state.live_zones[self.index].zone.clone()
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
