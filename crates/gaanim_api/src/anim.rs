use gaanim_core::ObjectId;
use gaanim_core::glam::{DVec3, DQuat};
use gaanim_core::peniko::Color;
use gaanim_math::RateFunc;

/// High-level, developer-friendly animation types that do not require explicitly defining
/// the initial "from" properties (resolved dynamically at timeline playback scheduling).
#[derive(Debug, Clone)]
pub enum AnimationType {
    TranslateTo { to: DVec3 },
    TranslateBy { delta: DVec3 },
    RotateTo { to: DQuat },
    RotateBy { angle_radians: f64 },
    ScaleTo { to: DVec3 },
    ScaleUniform { factor: f64 },
    FadeTo { to: f32 },
    FadeIn,
    FadeOut,
    FillColorTo { to: Color },
    StrokeColorTo { to: Color },
    StrokeWidthTo { to: f64 },
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
}

