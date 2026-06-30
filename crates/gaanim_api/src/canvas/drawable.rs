//! DrawableHandle — ergonomic mobject handle with fluent configuration.

use gaanim_core::ObjectId;
use gaanim_core::glam::DVec3;
use gaanim_core::kurbo::BezPath;
use gaanim_core::peniko::{Brush, Color};
use gaanim_math::RateFunc;

use crate::anim::{AnimationBuilder, AnimationType};
use crate::canvas::ops::{SharedCanvasState, SharedObjectSpec};
use crate::canvas::types::{Anim, ObjectSpec, SpawnKind};

/// An ergonomic handle to a mobject on a Canvas.
///
/// - Instant setters return `Self` (fluent): `obj.fill(RED).at(0,0)`.
/// - Animation methods return `Anim`: `obj.fade_in().duration(1.0)` and
///   auto-enqueue the animation on the active segment's sequential track.
#[derive(Debug, Clone)]
pub struct DrawableHandle {
    pub id: ObjectId,
    pub(crate) spec: SharedObjectSpec,
    pub(crate) state: SharedCanvasState,
    pub(crate) segment_idx: usize,
}

impl DrawableHandle {
    pub(crate) fn new(
        id: ObjectId,
        kind: SpawnKind,
        state: SharedCanvasState,
        segment_idx: usize,
    ) -> Self {
        Self {
            id,
            spec: std::sync::Arc::new(std::sync::Mutex::new(ObjectSpec::new(id, kind))),
            state,
            segment_idx,
        }
    }

    fn update_spec(&self, f: impl FnOnce(&mut ObjectSpec)) -> Self {
        f(&mut self.spec.lock().expect("object spec poisoned"));
        self.clone()
    }

    // -- Instant setters (return Self) --

    pub fn fill(self, color: Color) -> Self {
        self.update_spec(|spec| spec.fill = Some(Brush::Solid(color)))
    }

    pub fn no_fill(self) -> Self {
        self.update_spec(|spec| spec.fill = None)
    }

    pub fn stroke(self, color: Color, width: f64) -> Self {
        self.update_spec(|spec| spec.stroke = Some((color, width)))
    }

    pub fn no_stroke(self) -> Self {
        self.update_spec(|spec| spec.stroke = None)
    }

    pub fn opacity(self, op: f32) -> Self {
        self.update_spec(|spec| spec.opacity = op)
    }

    pub fn z_index(self, z: i32) -> Self {
        self.update_spec(|spec| spec.z_index = z)
    }

    pub fn at(self, x: f64, y: f64) -> Self {
        self.update_spec(|spec| spec.position = DVec3::new(x, y, 0.0))
    }

    // -- Internal helper --

    fn anim(&self, ty: AnimationType) -> Anim {
        Anim::queued(self.id, ty, self.state.clone(), self.segment_idx)
    }

    // -- Animation methods (return Anim, auto-enqueued) --

    pub fn r#move(&self, dx: f64, dy: f64) -> Anim {
        self.anim(AnimationType::TranslateBy {
            delta: DVec3::new(dx, dy, 0.0),
        })
    }

    pub fn move_to(&self, x: f64, y: f64) -> Anim {
        self.anim(AnimationType::TranslateTo {
            to: DVec3::new(x, y, 0.0),
        })
    }

    pub fn glide_to(&self, x: f64, y: f64) -> Anim {
        self.move_to(x, y)
    }

    pub fn scale(&self, factor: f64) -> Anim {
        self.anim(AnimationType::ScaleUniform { factor })
    }

    pub fn rotate(&self, rad: f64) -> Anim {
        self.anim(AnimationType::RotateBy { angle_radians: rad })
    }

    pub fn fade_in(&self) -> Anim {
        self.anim(AnimationType::FadeIn)
    }

    pub fn fade_out(&self) -> Anim {
        self.anim(AnimationType::FadeOut)
    }

    pub fn fade_to(&self, alpha: f32) -> Anim {
        self.anim(AnimationType::FadeTo { to: alpha })
    }

    pub fn write(&self) -> Anim {
        self.anim(AnimationType::Write { stroke_width: None })
    }

    pub fn create(&self) -> Anim {
        self.anim(AnimationType::Create { stroke_width: None })
    }

    pub fn unwrite(&self) -> Anim {
        self.anim(AnimationType::Unwrite { stroke_width: None })
    }

    pub fn uncreate(&self) -> Anim {
        self.anim(AnimationType::Uncreate { stroke_width: None })
    }

    pub fn grow_from_center(&self) -> Anim {
        self.anim(AnimationType::GrowFromCenter)
    }

    pub fn shrink_to_center(&self) -> Anim {
        self.anim(AnimationType::ShrinkToCenter)
    }

    pub fn spin_in_from_nothing(&self) -> Anim {
        self.anim(AnimationType::SpinInFromNothing)
    }

    pub fn grow_from_point(&self, px: f64, py: f64) -> Anim {
        self.anim(AnimationType::GrowFromPoint { px, py })
    }

    pub fn grow_from_edge(&self, dir: &str) -> Anim {
        self.anim(AnimationType::GrowFromEdge {
            direction: dir.to_string(),
        })
    }

    pub fn draw_border_then_fill(&self) -> Anim {
        self.anim(AnimationType::DrawBorderThenFill)
    }

    pub fn indicate(&self) -> Anim {
        self.anim(AnimationType::Indicate {
            color: None,
            scale_factor: 1.3,
        })
    }

    pub fn circumscribe(&self) -> Anim {
        self.anim(AnimationType::Circumscribe { color: None })
    }

    pub fn flash(&self) -> Anim {
        self.anim(AnimationType::Flash {
            color: None,
            n_lines: 16,
            radius: 100.0,
        })
    }

    pub fn wiggle(&self) -> Anim {
        self.anim(AnimationType::Wiggle)
    }

    pub fn move_along_path(&self, path: BezPath) -> Anim {
        self.anim(AnimationType::MoveAlongPath { path })
    }

    pub fn fade_transform(&self, target: &DrawableHandle) -> Anim {
        self.anim(AnimationType::FadeTransform { target: target.id })
    }

    /// Start the legacy `.animate()` compound builder. This intentionally does
    /// not auto-enqueue because the old builder is incomplete until a concrete
    /// animation kind is selected.
    pub fn animate(&self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::TranslateBy { delta: DVec3::ZERO },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }
}
