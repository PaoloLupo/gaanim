//! Deterministic particle emitters: `scene.fx.particles` and its presets.
//!
//! An emitter is a drawable whose members are one path per color and fade
//! level. Its particles are a closed-form function of the timeline time (see
//! [`gaanim_math::particles`]), rebuilt every frame by the renderer, so seeks
//! and exports reproduce playback exactly.

use gaanim_core::ObjectId;
use gaanim_core::glam::DVec2;
use gaanim_core::peniko::Color;
use gaanim_math::particles::ParticleSystem;
pub use gaanim_math::particles::{EmitterShape, ParticleShape};

use super::types::SpawnKind;
use super::{DrawableHandle, SceneModel};

/// Most particles one emitter may keep alive from its continuous stream
/// (`rate` times the longest life).
pub const MAX_LIVE_PARTICLES: f64 = 100_000.0;
/// Most particles in a single burst.
pub const MAX_BURST: u32 = 100_000;
/// Most colors a particle may pick from.
pub const MAX_PARTICLE_COLORS: usize = 16;
/// Colors sampled from a gradient: each particle takes one of them.
pub const GRADIENT_PARTICLE_COLORS: usize = 8;

/// Where an emitter releases its particles: a shape placed at a point or
/// on a drawable.
#[derive(Debug, Clone)]
pub struct Emitter {
    pub shape: EmitterShape,
    /// Scene position, or the offset from `anchor` when there is one.
    pub position: DVec2,
    /// A drawable whose position the emitter follows.
    pub anchor: Option<DrawableHandle>,
}

impl Emitter {
    pub fn new(shape: EmitterShape) -> Result<Self, String> {
        if !shape.is_valid() {
            return Err("emitter dimensions must be finite and non-negative".to_string());
        }
        Ok(Self {
            shape,
            position: DVec2::ZERO,
            anchor: None,
        })
    }

    /// The same emitter at the scene point `(x, y)`.
    pub fn at_point(mut self, x: f64, y: f64) -> Result<Self, String> {
        if !x.is_finite() || !y.is_finite() {
            return Err("emitter position must be finite".to_string());
        }
        self.position = DVec2::new(x, y);
        self.anchor = None;
        Ok(self)
    }

    /// The same emitter following `target`, offset by `(dx, dy)`.
    pub fn at_drawable(
        mut self,
        target: &DrawableHandle,
        dx: f64,
        dy: f64,
    ) -> Result<Self, String> {
        if !dx.is_finite() || !dy.is_finite() {
            return Err("emitter offset must be finite".to_string());
        }
        self.position = DVec2::new(dx, dy);
        self.anchor = Some(target.clone());
        Ok(self)
    }
}

/// The colors particles pick from.
#[derive(Debug, Clone, PartialEq)]
pub enum ParticleColors {
    /// Each particle takes one of these colors at random.
    Palette(Vec<Color>),
    /// Each particle takes a color at random along these gradient stops
    /// (`offset` in `[0, 1]`), sampled in [`GRADIENT_PARTICLE_COLORS`] steps.
    Gradient(Vec<(f32, Color)>),
}

impl ParticleColors {
    /// The distinct colors a particle may take.
    pub fn resolve(&self) -> Result<Vec<Color>, String> {
        let colors = match self {
            Self::Palette(colors) => colors.clone(),
            Self::Gradient(stops) => {
                if stops.is_empty() {
                    Vec::new()
                } else {
                    let mut stops = stops.clone();
                    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
                    (0..GRADIENT_PARTICLE_COLORS)
                        .map(|index| {
                            let t = (index as f32 + 0.5) / GRADIENT_PARTICLE_COLORS as f32;
                            gradient_color(&stops, t)
                        })
                        .collect()
                }
            }
        };
        if colors.is_empty() {
            return Err("particles need at least one color".to_string());
        }
        if colors.len() > MAX_PARTICLE_COLORS {
            return Err(format!(
                "particles take at most {MAX_PARTICLE_COLORS} colors, got {}",
                colors.len()
            ));
        }
        Ok(colors)
    }
}

fn gradient_color(stops: &[(f32, Color)], t: f32) -> Color {
    let first = stops[0];
    if t <= first.0 {
        return first.1;
    }
    for pair in stops.windows(2) {
        let ((a, from), (b, to)) = (pair[0], pair[1]);
        if t <= b {
            let span = b - a;
            let local = if span > f32::EPSILON {
                (t - a) / span
            } else {
                1.0
            };
            return gaanim_core::interpolate_color(from, to, local as f64);
        }
    }
    stops[stops.len() - 1].1
}

/// How an emitter releases, moves and draws its particles.
///
/// Ranges are `(low, high)`; each particle draws uniformly inside them from
/// its seed.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleOptions {
    /// Continuous particles per second from the declaration; 0 emits only
    /// bursts.
    pub rate: f64,
    /// Seconds of continuous emission; `None` never stops.
    pub duration: Option<f64>,
    pub lifetime: (f64, f64),
    /// Initial speed in scene units per second.
    pub speed: (f64, f64),
    /// Mean heading in radians (0 right, `pi / 2` up).
    pub direction: f64,
    /// Full angle of the cone of headings around `direction`.
    pub spread: f64,
    pub gravity: DVec2,
    /// Linear drag per second.
    pub drag: f64,
    /// Diameter or side in scene units.
    pub size: (f64, f64),
    /// Size multiplier reached at the end of each life.
    pub size_end: f64,
    /// Fraction of each life over which a particle fades out at its end.
    pub fade: f64,
    /// Rotation speed in radians per second.
    pub spin: (f64, f64),
    /// Horizontal sway amplitude in scene units.
    pub flutter: f64,
    pub shape: ParticleShape,
    pub colors: ParticleColors,
    pub seed: u64,
}

impl Default for ParticleOptions {
    fn default() -> Self {
        Self {
            rate: 30.0,
            duration: None,
            lifetime: (0.6, 1.2),
            speed: (1.0, 2.0),
            direction: std::f64::consts::FRAC_PI_2,
            spread: std::f64::consts::TAU,
            gravity: DVec2::ZERO,
            drag: 0.0,
            size: (0.03, 0.06),
            size_end: 1.0,
            fade: 0.3,
            spin: (0.0, 0.0),
            flutter: 0.0,
            shape: ParticleShape::Circle,
            colors: ParticleColors::Palette(vec![Color::WHITE]),
            seed: 0,
        }
    }
}

/// The palette of [`ParticleOptions::confetti`].
pub fn confetti_palette() -> Vec<Color> {
    vec![
        Color::from_rgb8(0xFF, 0xD1, 0x66),
        Color::from_rgb8(0xEF, 0x47, 0x6F),
        Color::from_rgb8(0x06, 0xD6, 0xA0),
        Color::from_rgb8(0x11, 0x8A, 0xB2),
        Color::from_rgb8(0x9B, 0x5D, 0xE5),
        Color::from_rgb8(0xF7, 0x8C, 0x6B),
    ]
}

impl ParticleOptions {
    /// Paper strips thrown upward in a cone that spin, sway and fall.
    pub fn confetti() -> Self {
        Self {
            rate: 0.0,
            lifetime: (2.6, 3.6),
            speed: (6.0, 10.0),
            direction: std::f64::consts::FRAC_PI_2,
            spread: 0.9,
            gravity: DVec2::new(0.0, -6.0),
            drag: 1.4,
            size: (0.12, 0.2),
            fade: 0.15,
            spin: (-9.0, 9.0),
            flutter: 0.25,
            shape: ParticleShape::Rect,
            colors: ParticleColors::Palette(confetti_palette()),
            ..Self::default()
        }
    }

    fn validate(&self) -> Result<(), String> {
        let range = |name: &str, (low, high): (f64, f64), positive: bool| {
            let ok = low.is_finite()
                && high.is_finite()
                && low <= high
                && if positive { low > 0.0 } else { low >= 0.0 };
            if ok {
                Ok(())
            } else {
                Err(format!(
                    "{name} must be finite {} with low <= high, got ({low}, {high})",
                    if positive {
                        "and positive"
                    } else {
                        "and non-negative"
                    }
                ))
            }
        };
        if !self.rate.is_finite() || self.rate < 0.0 {
            return Err(format!(
                "rate must be finite and non-negative, got {}",
                self.rate
            ));
        }
        if let Some(duration) = self.duration
            && (!duration.is_finite() || duration < 0.0)
        {
            return Err(format!(
                "duration must be finite and non-negative, got {duration}"
            ));
        }
        range("lifetime", self.lifetime, true)?;
        range("speed", self.speed, false)?;
        range("size", self.size, false)?;
        if !self.spin.0.is_finite() || !self.spin.1.is_finite() {
            return Err("spin must be finite".to_string());
        }
        let finite = [
            ("direction", self.direction),
            ("spread", self.spread),
            ("gravity", self.gravity.x),
            ("gravity", self.gravity.y),
            ("flutter", self.flutter),
        ];
        if let Some((name, _)) = finite.iter().find(|(_, value)| !value.is_finite()) {
            return Err(format!("{name} must be finite"));
        }
        if !self.drag.is_finite() || self.drag < 0.0 {
            return Err(format!(
                "drag must be finite and non-negative, got {}",
                self.drag
            ));
        }
        if !self.size_end.is_finite() || self.size_end < 0.0 {
            return Err(format!(
                "size_end must be finite and non-negative, got {}",
                self.size_end
            ));
        }
        if !(0.0..=1.0).contains(&self.fade) {
            return Err(format!("fade must be between 0 and 1, got {}", self.fade));
        }
        if self.rate * self.lifetime.1 > MAX_LIVE_PARTICLES {
            return Err(format!(
                "rate * lifetime keeps up to {} particles alive; the limit is {MAX_LIVE_PARTICLES}",
                self.rate * self.lifetime.1
            ));
        }
        Ok(())
    }
}

/// What a particle emitter spawns; the compiler fills in its times.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleSpawn {
    /// Emission, motion and look; `start`, `stop` and `colors` are set when
    /// the emitter compiles.
    pub system: ParticleSystem,
    pub position: DVec2,
    pub anchor: Option<ObjectId>,
    pub colors: Vec<Color>,
    pub duration: Option<f64>,
    /// Particles burst at the declaration.
    pub initial_burst: u32,
}

/// `Ok` when `count` particles fit in one burst.
pub(crate) fn check_burst(count: u32) -> Result<(), String> {
    if count == 0 || count > MAX_BURST {
        Err(format!(
            "a burst emits between 1 and {MAX_BURST} particles, got {count}"
        ))
    } else {
        Ok(())
    }
}

impl SceneModel {
    /// A deterministic particle emitter: a drawable whose particles leave
    /// `emitter` continuously at `options.rate` from the cursor, and in
    /// bursts (`Drawable::burst`, `Anim::burst`). Every particle is a pure
    /// function of the timeline time, its index and `options.seed`.
    pub fn particles(
        &mut self,
        emitter: &Emitter,
        options: ParticleOptions,
    ) -> Result<DrawableHandle, String> {
        self.spawn_particles(emitter, options, 0)
    }

    /// Confetti from `origin`: `count` pieces burst at the cursor (none when
    /// 0), with [`ParticleOptions::confetti`] or the given `options`.
    pub fn confetti(
        &mut self,
        emitter: &Emitter,
        count: u32,
        options: ParticleOptions,
    ) -> Result<DrawableHandle, String> {
        if count > 0 {
            check_burst(count)?;
        }
        self.spawn_particles(emitter, options, count)
    }

    fn spawn_particles(
        &mut self,
        emitter: &Emitter,
        options: ParticleOptions,
        initial_burst: u32,
    ) -> Result<DrawableHandle, String> {
        options.validate()?;
        if !emitter.shape.is_valid() {
            return Err("emitter dimensions must be finite and non-negative".to_string());
        }
        if let Some(anchor) = &emitter.anchor
            && !self.owns_drawable(anchor)
        {
            return Err("the emitter must follow a drawable of this scene".to_string());
        }
        let colors = options.colors.resolve()?;
        let system = ParticleSystem {
            emitter: emitter.shape,
            rate: options.rate,
            start: 0.0,
            stop: None,
            bursts: Vec::new(),
            lifetime: options.lifetime,
            speed: options.speed,
            direction: options.direction,
            spread: options.spread,
            gravity: options.gravity,
            drag: options.drag,
            size: options.size,
            size_end: options.size_end,
            fade: options.fade,
            spin: options.spin,
            flutter: options.flutter,
            shape: options.shape,
            colors: colors.len(),
            seed: options.seed,
        };
        let result = self.spawn(SpawnKind::Particles(Box::new(ParticleSpawn {
            system,
            position: emitter.position,
            anchor: emitter.anchor.as_ref().map(|anchor| anchor.id),
            colors,
            duration: options.duration,
            initial_burst,
        })));
        {
            // The layers carry the particle colors; themes do not restyle them.
            let mut spec = result.spec.lock().expect("object spec poisoned");
            spec.fill = None;
            spec.fill_overridden = true;
            spec.stroke = None;
            spec.stroke_overridden = true;
        }
        Ok(result)
    }
}

/// The longest life of the particles of `kind`, when it is an emitter.
pub(crate) fn emitter_longest_life(kind: &SpawnKind) -> Option<f64> {
    match kind {
        SpawnKind::Particles(particles) => Some(particles.system.longest_life()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_resolve_from_palettes_and_gradients() {
        let palette = ParticleColors::Palette(vec![Color::WHITE, Color::BLACK]);
        assert_eq!(palette.resolve().unwrap(), [Color::WHITE, Color::BLACK]);
        assert!(ParticleColors::Palette(Vec::new()).resolve().is_err());
        assert!(
            ParticleColors::Palette(vec![Color::WHITE; 17])
                .resolve()
                .is_err()
        );
        let gradient = ParticleColors::Gradient(vec![(1.0, Color::WHITE), (0.0, Color::BLACK)]);
        let colors = gradient.resolve().unwrap();
        assert_eq!(colors.len(), GRADIENT_PARTICLE_COLORS);
        // Sorted by offset: dark first, light last.
        let lightness = |color: Color| color.to_rgba8().r;
        assert!(lightness(colors[0]) < 40 && lightness(colors[7]) > 215);
    }

    #[test]
    fn options_are_validated() {
        let mut model = SceneModel::new(640, 360);
        let point = Emitter::new(EmitterShape::Point).unwrap();
        assert!(model.particles(&point, ParticleOptions::default()).is_ok());
        let invalid = [
            ParticleOptions {
                rate: -1.0,
                ..ParticleOptions::default()
            },
            ParticleOptions {
                lifetime: (0.0, 1.0),
                ..ParticleOptions::default()
            },
            ParticleOptions {
                speed: (2.0, 1.0),
                ..ParticleOptions::default()
            },
            ParticleOptions {
                fade: 1.5,
                ..ParticleOptions::default()
            },
            ParticleOptions {
                rate: 1e6,
                ..ParticleOptions::default()
            },
        ];
        for options in invalid {
            assert!(model.particles(&point, options).is_err());
        }
        assert!(
            Emitter::new(EmitterShape::Circle {
                radius: -1.0,
                edge: false
            })
            .is_err()
        );
        assert!(
            model
                .confetti(&point, MAX_BURST + 1, ParticleOptions::confetti())
                .is_err()
        );
        let foreign = SceneModel::new(640, 360).circle(0.2);
        let anchored = point.at_drawable(&foreign, 0.0, 0.0).unwrap();
        assert!(
            model
                .particles(&anchored, ParticleOptions::default())
                .is_err()
        );
    }
}
