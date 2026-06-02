use gaanim_core::ObjectId;
use gaanim_core::glam::{DQuat, DVec3};
use gaanim_core::peniko::Color;
use gaanim_math::RateFunc;

/// High-level, developer-friendly animation types that do not require explicitly defining
/// the initial "from" properties (resolved dynamically at timeline playback scheduling).
#[derive(Debug, Clone)]
pub enum AnimationType {
    TranslateTo {
        to: DVec3,
    },
    TranslateBy {
        delta: DVec3,
    },
    RotateTo {
        to: DQuat,
    },
    RotateBy {
        angle_radians: f64,
    },
    ScaleTo {
        to: DVec3,
    },
    ScaleUniform {
        factor: f64,
    },
    FadeTo {
        to: f32,
    },
    FadeIn,
    FadeOut,
    FillColorTo {
        to: Color,
    },
    StrokeColorTo {
        to: Color,
    },
    StrokeWidthTo {
        to: f64,
    },
    /// Manim-style `Write`: progressively draw the target's path(s) along
    /// their arc length, then cross-fade the fill in once the outline is
    /// complete. This produces the characteristic "pen draws the object,
    /// then ink fills it" effect that Manim's `Write` is known for.
    ///
    /// Internally, each item gets a triplet of clips scheduled on the
    /// default track:
    ///
    /// 1. A `FillDrawProgress` hold (`0 -> 0`) spanning the draw phase.
    ///    This is the authoritative reset that guarantees the fill is
    ///    hidden from the very first frame of every Write — even if a
    ///    previous Write left the component at 1.0, the hold lens runs
    ///    at `item_start` and overwrites it to 0.0.
    /// 2. A `PathCompletion` (`0 -> 1`) over the draw phase. The lens
    ///    trims the cached `PathSource` along its arc length, producing
    ///    the progressive pen-stroke effect.
    /// 3. A `FillDrawProgress` cross-fade (`0 -> 1`) over the fade
    ///    phase, starting right after the draw completes. The renderer
    ///    reads this component and modulates the fill brush's color
    ///    alpha uniformly (works for Solid / Gradient / Image brushes).
    ///
    /// - If the target has children (text root, equation root, group),
    ///   each child gets its own staggered sub-clip triplet. The
    ///   per-item timeline math is
    ///   `item_duration = duration / (1 + (n - 1) * lag_ratio)` and the
    ///   lag step is `item_duration * lag_ratio`, so the next child
    ///   starts when the previous one is `lag_ratio` (default 0.5)
    ///   through its total duration.
    /// - If the target has no children, a single sub-clip triplet is
    ///   scheduled on the target itself.
    /// - The draw phase occupies ~70% of each item's duration; the
    ///   fill cross-fade occupies the remaining ~30%.
    /// - The `stroke_width` controls the outline thickness used during the
    ///   draw phase (defaults to the target's existing stroke width).
    Write {
        /// Optional override for the outline stroke width used during the
        /// draw phase. `None` means "use the target's existing stroke width".
        stroke_width: Option<f64>,
    },
    Create {
        stroke_width: Option<f64>,
    },
    Uncreate {
        stroke_width: Option<f64>,
    },
    Unwrite {
        stroke_width: Option<f64>,
    },
    GrowFromCenter,
    ShrinkToCenter,
    SpinInFromNothing,
    Indicate {
        color: Option<Color>,
        scale_factor: f64,
    },
    /// Fade out the source and fade in the target concurrently.
    FadeTransform {
        target: ObjectId,
    },
    /// Oscillating wiggle vibration (horizontal).
    Wiggle,
    /// Scale from 0 at a specific anchor point, growing to full size.
    GrowFromPoint {
        px: f64,
        py: f64,
    },
    /// Scale from 0 at a specific edge direction.
    GrowFromEdge {
        direction: String,
    },
    /// Draw the outline first (like Write) then fill in.
    DrawBorderThenFill,
    /// Lines radiating outward from a point (flash of insight effect).
    Flash {
        color: Option<Color>,
        n_lines: u32,
        radius: f64,
    },
    /// A rectangle/circle that appears around the target, grows, and fades.
    Circumscribe {
        color: Option<Color>,
    },
}

/// A fluent builder for an animation tween clip.
#[derive(Debug, Clone)]
pub struct AnimationBuilder {
    pub target: ObjectId,
    pub anim_type: AnimationType,
    pub duration: f64,
    pub rate_func: RateFunc,
}

impl AnimationBuilder {
    /// Sets the animation duration in seconds.
    pub fn duration(mut self, sec: f64) -> Self {
        self.duration = sec;
        self
    }

    /// Configures the pacing curve or rate function.
    pub fn rate_func(mut self, f: RateFunc) -> Self {
        self.rate_func = f;
        self
    }

    /// Sets ease curve to a fluid, premium physical Spring simulation.
    pub fn spring(self) -> Self {
        self.rate_func(RateFunc::Spring {
            stiffness: 90.0,
            damping: 12.0,
        })
    }

    /// Sets ease curve to standard cubic-bezier Double Smooth.
    pub fn smooth(self) -> Self {
        self.rate_func(RateFunc::Smooth)
    }

    /// Sets pacing to flat Linear.
    pub fn linear(self) -> Self {
        self.rate_func(RateFunc::Linear)
    }
}

use crate::builder::MobjectRef;

impl MobjectRef {
    pub fn translate_to(self, to: DVec3) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::TranslateTo { to },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    pub fn translate_to_2d(self, x: f64, y: f64) -> AnimationBuilder {
        self.translate_to(DVec3::new(x, y, 0.0))
    }

    pub fn shift(self, delta: DVec3) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::TranslateBy { delta },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    pub fn shift_2d(self, x: f64, y: f64) -> AnimationBuilder {
        self.shift(DVec3::new(x, y, 0.0))
    }

    pub fn rotate_to(self, to: DQuat) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::RotateTo { to },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    pub fn rotate_to_2d(self, angle_radians: f64) -> AnimationBuilder {
        self.rotate_to(DQuat::from_rotation_z(angle_radians))
    }

    pub fn rotate_by(self, angle_radians: f64) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::RotateBy { angle_radians },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    pub fn scale_to(self, to: DVec3) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::ScaleTo { to },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    pub fn scale_uniform(self, factor: f64) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::ScaleUniform { factor },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    pub fn fade_to(self, to: f32) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::FadeTo { to },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    pub fn fade_in(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::FadeIn,
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    pub fn fade_out(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::FadeOut,
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    pub fn fill_color_to(self, to: Color) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::FillColorTo { to },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    pub fn stroke_color_to(self, to: Color) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::StrokeColorTo { to },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    pub fn stroke_width_to(self, to: f64) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::StrokeWidthTo { to },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    /// Manim-style **Write**: progressively draws the Mobject's path(s) along
    /// their arc length, then cross-fades from the outline to the final
    /// fill/stroke. If the Mobject has children (e.g. a text root or an
    /// equation), each child is drawn in a staggered sequence.
    pub fn write(self, duration: f64) -> AnimationBuilder {
        self.write_with_stroke_width(duration, None)
    }

    /// Same as [`write`](Self::write) but with an explicit outline stroke
    /// width used during the draw phase.
    pub fn write_with_stroke_width(
        self,
        duration: f64,
        stroke_width: Option<f64>,
    ) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Write { stroke_width },
            duration,
            rate_func: RateFunc::Smooth,
        }
    }

    /// Manim-style **Create**: progressively draws the Mobject's path(s) along
    /// their arc length in parallel (without character/element stagger).
    pub fn create(self, duration: f64) -> AnimationBuilder {
        self.create_with_stroke_width(duration, None)
    }

    /// Same as [`create`](Self::create) but with an explicit outline stroke width.
    pub fn create_with_stroke_width(
        self,
        duration: f64,
        stroke_width: Option<f64>,
    ) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Create { stroke_width },
            duration,
            rate_func: RateFunc::Smooth,
        }
    }

    /// Progressive erasure of the Mobject's path(s) and fill in parallel.
    pub fn uncreate(self, duration: f64) -> AnimationBuilder {
        self.uncreate_with_stroke_width(duration, None)
    }

    /// Same as [`uncreate`](Self::uncreate) but with an explicit outline stroke width.
    pub fn uncreate_with_stroke_width(
        self,
        duration: f64,
        stroke_width: Option<f64>,
    ) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Uncreate { stroke_width },
            duration,
            rate_func: RateFunc::Smooth,
        }
    }

    /// Staggered sequential erasure of the Mobject's path(s) and fill in reverse order (e.g. right-to-left).
    pub fn unwrite(self, duration: f64) -> AnimationBuilder {
        self.unwrite_with_stroke_width(duration, None)
    }

    /// Same as [`unwrite`](Self::unwrite) but with an explicit outline stroke width.
    pub fn unwrite_with_stroke_width(
        self,
        duration: f64,
        stroke_width: Option<f64>,
    ) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Unwrite { stroke_width },
            duration,
            rate_func: RateFunc::Smooth,
        }
    }

    /// Scale up from 0.0 to original size centered at current local position.
    pub fn grow_from_center(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::GrowFromCenter,
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    /// Scale down from current size to 0.0 centered at current local position.
    pub fn shrink_to_center(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::ShrinkToCenter,
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    /// Scale up from 0.0 and rotate 360 degrees concurrently.
    pub fn spin_in_from_nothing(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::SpinInFromNothing,
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    /// Temporarily scale up to 1.25x and highlight with GOLD color before returning to baseline.
    pub fn indicate(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Indicate {
                color: Some(Color::from_rgb8(255, 215, 0)),
                scale_factor: 1.25,
            },
            duration: 1.0,
            rate_func: RateFunc::ThereAndBack,
        }
    }

    /// Temporarily scale up and highlight with custom parameters before returning to baseline.
    pub fn indicate_with_color_and_scale(
        self,
        color: Option<Color>,
        scale_factor: f64,
    ) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Indicate {
                color,
                scale_factor,
            },
            duration: 1.0,
            rate_func: RateFunc::ThereAndBack,
        }
    }

    pub fn fade_transform(self, target: ObjectId) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::FadeTransform { target },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    pub fn wiggle(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Wiggle,
            duration: 1.0,
            rate_func: RateFunc::Linear,
        }
    }

    pub fn grow_from_point(self, px: f64, py: f64) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::GrowFromPoint { px, py },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    pub fn grow_from_edge(self, direction: &str) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::GrowFromEdge {
                direction: direction.to_string(),
            },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    pub fn draw_border_then_fill(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::DrawBorderThenFill,
            duration: 1.5,
            rate_func: RateFunc::Smooth,
        }
    }

    pub fn flash(
        self,
        color: Option<Color>,
        n_lines: u32,
        radius: f64,
    ) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Flash {
                color,
                n_lines,
                radius,
            },
            duration: 1.0,
            rate_func: RateFunc::ThereAndBack,
        }
    }

    pub fn circumscribe(self, color: Option<Color>) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Circumscribe { color },
            duration: 1.5,
            rate_func: RateFunc::ThereAndBack,
        }
    }
}
