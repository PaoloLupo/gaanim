//! Ruler-and-compass constructions: reactive points, lines and circles found
//! from other points, recomputed every frame as those move. A construction
//! with no answer (lines that never cross, a point inside the circle) hides
//! what depends on it.

use gaanim_math::Construction;

use super::SceneModel;
use super::drawable::DrawableHandle;
use super::ops::{CanvasEndpoint, CanvasRadius, Op, PointRef};
use super::types::SpawnKind;

/// Why a construction cannot be made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConstructionError {
    #[error("this construction takes {expected} points, not {given}")]
    Arity { expected: usize, given: usize },
}

impl SceneModel {
    /// The point `kind` constructs from `inputs`, recomputed every frame.
    pub fn construction_point(
        &self,
        kind: Construction,
        inputs: Vec<CanvasEndpoint>,
    ) -> Result<PointRef, ConstructionError> {
        if inputs.len() != kind.arity() {
            return Err(ConstructionError::Arity {
                expected: kind.arity(),
                given: inputs.len(),
            });
        }
        Ok(PointRef(CanvasEndpoint::Construction { kind, inputs }))
    }

    /// Half the length a constructed line is drawn at: past the frame in
    /// every direction, so it reads as a whole line.
    fn line_reach(&self) -> f64 {
        (self.frame.width + self.frame.height).max(1.0) * 2.0
    }

    /// The whole line through `through` and `toward`, drawn across the frame
    /// and hidden while either point has no position. Like `tracking_line`,
    /// it shows once a `play` reveals it.
    pub fn construction_line(
        &mut self,
        through: CanvasEndpoint,
        toward: CanvasEndpoint,
    ) -> DrawableHandle {
        let reach = self.line_reach();
        let end = |length: f64| CanvasEndpoint::Construction {
            kind: Construction::Extend { length },
            inputs: vec![through.clone(), toward.clone()],
        };
        let handle = self.tracking_line(end(-reach), end(reach));
        self.push_op(Op::HideUnresolved { target: handle.id });
        handle
    }

    /// A circle around `center` of `radius`, recomputed every frame and
    /// empty while its points have no position. It shows once a `play`
    /// reveals it.
    pub fn tracking_circle(
        &mut self,
        center: CanvasEndpoint,
        radius: CanvasRadius,
    ) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TrackingLine);
        handle.defer_visibility_until_play();
        self.push_op(Op::AttachTrackingCircle {
            target: handle.id,
            center,
            radius,
        });
        handle
    }

    fn push_op(&mut self, op: Op) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(op);
    }
}
