//! Interpolation of per-object renderer effects (glow, blur, shadow).

use bevy::prelude::{Entity, World};
use gaanim_core::glam::DVec2;
use gaanim_core::peniko::Color;
use gaanim_renderer::effects::{DropShadow, GaussianBlur, Glow};

/// The effects an object carries at one instant.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectState {
    pub glow: Option<Glow>,
    pub blur: Option<GaussianBlur>,
    pub shadow: Option<DropShadow>,
}

/// Lens between two effect states. An effect that is absent on one side
/// fades from or to an invisible version of the other side, so glows and
/// shadows grow in and blur-ins start sharp or end sharp.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectLens {
    pub from: EffectState,
    pub to: EffectState,
}

fn mix(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn mix_color(a: Color, b: Color, t: f64) -> Color {
    gaanim_core::interpolate_color(a, b, t)
}

fn transparent(color: Color) -> Color {
    color.with_alpha(0.0)
}

impl EffectLens {
    pub fn glow_at(&self, t: f64) -> Option<Glow> {
        let (from, to) = match (&self.from.glow, &self.to.glow) {
            (None, None) => return None,
            (Some(from), Some(to)) => (from.clone(), to.clone()),
            (Some(from), None) => (
                from.clone(),
                Glow {
                    intensity: 0.0,
                    ..from.clone()
                },
            ),
            (None, Some(to)) => (
                Glow {
                    intensity: 0.0,
                    ..to.clone()
                },
                to.clone(),
            ),
        };
        let glow = Glow {
            radius: mix(from.radius, to.radius, t),
            intensity: mix(from.intensity as f64, to.intensity as f64, t) as f32,
            color: mix_color(from.color, to.color, t),
        };
        (glow.intensity > 0.0 && glow.radius > 0.0).then_some(glow)
    }

    pub fn blur_at(&self, t: f64) -> Option<GaussianBlur> {
        let from = self.from.blur.map_or(0.0, |blur| blur.sigma);
        let to = self.to.blur.map_or(0.0, |blur| blur.sigma);
        if self.from.blur.is_none() && self.to.blur.is_none() {
            return None;
        }
        let sigma = mix(from, to, t);
        (sigma > 1e-9).then_some(GaussianBlur { sigma })
    }

    pub fn shadow_at(&self, t: f64) -> Option<DropShadow> {
        let (from, to) = match (&self.from.shadow, &self.to.shadow) {
            (None, None) => return None,
            (Some(from), Some(to)) => (from.clone(), to.clone()),
            (Some(from), None) => (
                from.clone(),
                DropShadow {
                    color: transparent(from.color),
                    ..from.clone()
                },
            ),
            (None, Some(to)) => (
                DropShadow {
                    color: transparent(to.color),
                    offset: DVec2::ZERO,
                    ..to.clone()
                },
                to.clone(),
            ),
        };
        let shadow = DropShadow {
            offset: from.offset.lerp(to.offset, t),
            blur_radius: mix(from.blur_radius, to.blur_radius, t),
            color: mix_color(from.color, to.color, t),
        };
        (shadow.color.components[3] > 0.0).then_some(shadow)
    }
}

impl gaanim_animation::AnimatableLens for EffectLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        let Ok(mut target) = world.get_entity_mut(entity) else {
            return;
        };
        match self.glow_at(t) {
            Some(glow) => {
                target.insert(glow);
            }
            None => {
                target.remove::<Glow>();
            }
        }
        match self.blur_at(t) {
            Some(blur) => {
                target.insert(blur);
            }
            None => {
                target.remove::<GaussianBlur>();
            }
        }
        match self.shadow_at(t) {
            Some(shadow) => {
                target.insert(shadow);
            }
            None => {
                target.remove::<DropShadow>();
            }
        }
    }

    fn clone_box(&self) -> Box<dyn gaanim_animation::AnimatableLens> {
        Box::new(self.clone())
    }

    fn type_name(&self) -> &'static str {
        "Effects"
    }

    // Snapshots do not record these components.
    fn hold_channel(&self) -> Option<&'static str> {
        Some("Effects")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_grow_in_and_fade_out() {
        let glow = Glow {
            radius: 0.5,
            intensity: 2.0,
            color: Color::WHITE,
        };
        let lens = EffectLens {
            from: EffectState::default(),
            to: EffectState {
                glow: Some(glow.clone()),
                blur: None,
                shadow: Some(DropShadow::default()),
            },
        };
        assert_eq!(lens.glow_at(0.0), None);
        assert_eq!(lens.glow_at(0.5).unwrap().intensity, 1.0);
        assert_eq!(lens.glow_at(1.0), Some(glow));
        assert_eq!(lens.shadow_at(0.0), None);
        let half = lens.shadow_at(0.5).unwrap();
        assert!(half.offset.length() > 0.0);
        assert_eq!(lens.shadow_at(1.0), Some(DropShadow::default()));

        let unblur = EffectLens {
            from: EffectState {
                blur: Some(GaussianBlur { sigma: 0.3 }),
                ..Default::default()
            },
            to: EffectState::default(),
        };
        assert!((unblur.blur_at(0.5).unwrap().sigma - 0.15).abs() < 1e-12);
        assert_eq!(unblur.blur_at(1.0), None);
    }
}
