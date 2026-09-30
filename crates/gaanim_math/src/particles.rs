//! Deterministic particles in closed form.
//!
//! A [`ParticleSystem`] never steps a simulation. Particle `i` of the
//! continuous stream is born at `start + i / rate`, each burst emits its
//! particles at its own time, and every attribute (lifetime, speed, heading,
//! size, color, spin) is drawn from a [`SeededRng`] keyed by
//! `(seed, stream, index)`. Motion under gravity and linear drag has an exact
//! solution, so the state at any time is a pure function of that time: a seek
//! and continuous playback agree exactly, and evaluating a frame only visits
//! the particles that can be alive then.

use glam::DVec2;
use kurbo::{BezPath, Point};

use crate::random::SeededRng;

/// Seconds of travel a [`ParticleShape::Streak`] stretches along its velocity.
pub const STREAK_TIME: f64 = 0.05;
/// Sway frequency of a fluttering particle, in cycles per second.
pub const FLUTTER_FREQUENCY: f64 = 1.3;

/// Where particles leave from, relative to the emitter origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EmitterShape {
    /// Every particle leaves from the origin.
    Point,
    /// Uniformly inside a disc of `radius`, or on its rim when `edge`.
    Circle { radius: f64, edge: bool },
    /// Uniformly inside a `width` x `height` rectangle centred on the origin.
    Rect { width: f64, height: f64 },
    /// Uniformly along a segment of `length` centred on the origin, turned
    /// by `angle` radians.
    Line { length: f64, angle: f64 },
}

impl EmitterShape {
    /// The offset for the uniform draws `u` and `v` in `[0, 1)`.
    pub fn sample(self, u: f64, v: f64) -> DVec2 {
        match self {
            Self::Point => DVec2::ZERO,
            Self::Circle { radius, edge } => {
                let distance = if edge { radius } else { radius * v.sqrt() };
                DVec2::from_angle(u * std::f64::consts::TAU) * distance
            }
            Self::Rect { width, height } => DVec2::new((u - 0.5) * width, (v - 0.5) * height),
            Self::Line { length, angle } => DVec2::from_angle(angle) * ((u - 0.5) * length),
        }
    }

    /// Whether every dimension is finite and non-negative.
    pub fn is_valid(self) -> bool {
        let ok = |value: f64| value.is_finite() && value >= 0.0;
        match self {
            Self::Point => true,
            Self::Circle { radius, .. } => ok(radius),
            Self::Rect { width, height } => ok(width) && ok(height),
            Self::Line { length, angle } => ok(length) && angle.is_finite(),
        }
    }
}

/// The outline drawn for each particle; its size is the diameter or side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParticleShape {
    Circle,
    Square,
    /// A 2:1 strip, like a piece of confetti.
    Rect,
    Triangle,
    /// A thin spark stretched along the velocity by [`STREAK_TIME`] seconds
    /// of travel; it ignores spin.
    Streak,
}

impl ParticleShape {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "circle" => Self::Circle,
            "square" => Self::Square,
            "rect" => Self::Rect,
            "triangle" => Self::Triangle,
            "streak" => Self::Streak,
            _ => return None,
        })
    }
}

/// One live particle at the evaluated time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    pub position: DVec2,
    pub velocity: DVec2,
    /// Diameter or side, after `size_end`.
    pub size: f64,
    /// Rotation in radians.
    pub angle: f64,
    /// Opacity left by the fade, in `(0, 1]`.
    pub alpha: f64,
    /// Index of the particle's color, below [`ParticleSystem::colors`].
    pub color: usize,
    pub age: f64,
    pub lifetime: f64,
}

/// Emission, motion and look of a set of deterministic particles.
///
/// Times are timeline seconds. Ranges are `(low, high)` and each particle
/// draws uniformly inside them.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleSystem {
    pub emitter: EmitterShape,
    /// Continuous particles per second; 0 emits only bursts.
    pub rate: f64,
    /// When continuous emission starts.
    pub start: f64,
    /// When continuous emission stops (exclusive); `None` never stops.
    pub stop: Option<f64>,
    /// `(time, count)` of every burst, in the order they were added.
    pub bursts: Vec<(f64, u32)>,
    pub lifetime: (f64, f64),
    /// Initial speed, in scene units per second.
    pub speed: (f64, f64),
    /// Mean heading in radians (0 points right, `pi / 2` up).
    pub direction: f64,
    /// Full angle of the cone of headings around `direction`.
    pub spread: f64,
    /// Constant acceleration, in scene units per second squared.
    pub gravity: DVec2,
    /// Linear drag coefficient `k` (per second): `dv/dt = gravity - k v`.
    pub drag: f64,
    pub size: (f64, f64),
    /// Size multiplier reached at the end of each life.
    pub size_end: f64,
    /// Fraction of each life, at its end, over which the particle fades out.
    pub fade: f64,
    /// Rotation speed range, in radians per second.
    pub spin: (f64, f64),
    /// Horizontal sway amplitude, in scene units.
    pub flutter: f64,
    pub shape: ParticleShape,
    /// Number of colors a particle picks from.
    pub colors: usize,
    pub seed: u64,
}

impl Default for ParticleSystem {
    fn default() -> Self {
        Self {
            emitter: EmitterShape::Point,
            rate: 30.0,
            start: 0.0,
            stop: None,
            bursts: Vec::new(),
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
            colors: 1,
            seed: 0,
        }
    }
}

fn mix(value: u64) -> u64 {
    SeededRng::new(value).next_u64()
}

fn lerp((low, high): (f64, f64), t: f64) -> f64 {
    low + (high - low) * t
}

/// Displacement after `age` seconds from velocity `v0`, under `gravity` and
/// linear drag `k`: `v0 (1 - e^(-k t)) / k + gravity (t - (1 - e^(-k t)) / k) / k`,
/// which tends to `v0 t + gravity t^2 / 2` without drag.
pub fn displacement(v0: DVec2, gravity: DVec2, drag: f64, age: f64) -> DVec2 {
    if drag <= 0.0 {
        return v0 * age + gravity * (0.5 * age * age);
    }
    let kt = drag * age;
    // (1 - e^(-kt)) / k without cancellation.
    let decay = -(-kt).exp_m1() / drag;
    let settle = if kt < 1e-4 {
        age * age * (0.5 - kt / 6.0 + kt * kt / 24.0)
    } else {
        (age - decay) / drag
    };
    v0 * decay + gravity * settle
}

/// Velocity after `age` seconds; the derivative of [`displacement`].
pub fn velocity(v0: DVec2, gravity: DVec2, drag: f64, age: f64) -> DVec2 {
    if drag <= 0.0 {
        return v0 + gravity * age;
    }
    let kt = drag * age;
    v0 * (-kt).exp() + gravity * (-(-kt).exp_m1() / drag)
}

impl ParticleSystem {
    /// The longest possible life.
    pub fn longest_life(&self) -> f64 {
        self.lifetime.0.max(self.lifetime.1)
    }

    /// Particle `index` of `stream` (0 is the continuous stream, `b + 1`
    /// burst `b`), born at `birth` from `origin`, as it is at `time`; `None`
    /// before its birth or after its death.
    pub fn particle(
        &self,
        stream: u64,
        index: u64,
        birth: f64,
        time: f64,
        origin: DVec2,
    ) -> Option<Particle> {
        let age = time - birth;
        if age.is_nan() || age < 0.0 {
            return None;
        }
        let mut rng = SeededRng::new(mix(mix(mix(self.seed) ^ stream) ^ index));
        let lifetime = lerp(self.lifetime, rng.next_f64());
        if age >= lifetime {
            return None;
        }
        let speed = lerp(self.speed, rng.next_f64());
        let heading = self.direction + (rng.next_f64() - 0.5) * self.spread;
        let (u, v) = (rng.next_f64(), rng.next_f64());
        let size = lerp(self.size, rng.next_f64());
        let color = ((rng.next_f64() * self.colors as f64) as usize).min(self.colors.max(1) - 1);
        let turn = rng.next_f64() * std::f64::consts::TAU;
        let spin = lerp(self.spin, rng.next_f64());
        let phase = rng.next_f64() * std::f64::consts::TAU;

        let v0 = DVec2::from_angle(heading) * speed;
        let mut position =
            origin + self.emitter.sample(u, v) + displacement(v0, self.gravity, self.drag, age);
        let mut velocity = velocity(v0, self.gravity, self.drag, age);
        if self.flutter != 0.0 {
            let omega = std::f64::consts::TAU * FLUTTER_FREQUENCY;
            position.x += self.flutter * ((omega * age + phase).sin() - phase.sin());
            velocity.x += self.flutter * omega * (omega * age + phase).cos();
        }
        let progress = age / lifetime;
        let alpha = if self.fade > 0.0 {
            ((1.0 - progress) / self.fade).min(1.0)
        } else {
            1.0
        };
        Some(Particle {
            position,
            velocity,
            size: size * (1.0 + (self.size_end - 1.0) * progress),
            angle: turn + spin * age,
            alpha,
            color,
            age,
            lifetime,
        })
    }

    /// Visit every particle alive at `time`. `origin` gives the emitter
    /// position at a birth time. Returns how many candidates were evaluated,
    /// which is at most the particles born within the longest life before
    /// `time`, plus two per stream.
    pub fn for_each_live(
        &self,
        time: f64,
        mut origin: impl FnMut(f64) -> DVec2,
        mut visit: impl FnMut(Particle),
    ) -> usize {
        let longest = self.longest_life();
        let mut evaluated = 0;
        if self.rate > 0.0 && time >= self.start {
            // One index of slack on each side absorbs rounding; `particle`
            // decides who is alive.
            let first = ((time - longest - self.start) * self.rate).ceil() - 1.0;
            let last = ((time - self.start) * self.rate).floor() + 1.0;
            let mut index = first.max(0.0) as u64;
            while (index as f64) <= last {
                let birth = self.start + index as f64 / self.rate;
                if birth > time || self.stop.is_some_and(|stop| birth >= stop) {
                    break;
                }
                evaluated += 1;
                if let Some(particle) = self.particle(0, index, birth, time, origin(birth)) {
                    visit(particle);
                }
                index += 1;
            }
        }
        for (burst, &(at, count)) in self.bursts.iter().enumerate() {
            if at > time || time - at >= longest {
                continue;
            }
            let from = origin(at);
            for index in 0..count as u64 {
                evaluated += 1;
                if let Some(particle) = self.particle(burst as u64 + 1, index, at, time, from) {
                    visit(particle);
                }
            }
        }
        evaluated
    }

    /// Every particle alive at `time` from a fixed origin.
    pub fn live(&self, time: f64, origin: DVec2) -> Vec<Particle> {
        let mut particles = Vec::new();
        self.for_each_live(time, |_| origin, |particle| particles.push(particle));
        particles
    }
}

/// Append the outline of `particle`, drawn as `shape`, to `path`.
pub fn append_particle(path: &mut BezPath, shape: ParticleShape, particle: &Particle) {
    let center = Point::new(particle.position.x, particle.position.y);
    let half = particle.size * 0.5;
    if half.is_nan() || half <= 0.0 || !half.is_finite() {
        return;
    }
    let rotate = |dx: f64, dy: f64, angle: f64| {
        let (sin, cos) = angle.sin_cos();
        center + kurbo::Vec2::new(dx * cos - dy * sin, dx * sin + dy * cos)
    };
    let polygon = |path: &mut BezPath, points: &[Point]| {
        path.move_to(points[0]);
        for point in &points[1..] {
            path.line_to(*point);
        }
        path.close_path();
    };
    match shape {
        ParticleShape::Circle => {
            // Four cubic quarter arcs.
            const KAPPA: f64 = 0.552_284_749_830_793_4;
            let k = half * KAPPA;
            let (x, y) = (center.x, center.y);
            path.move_to((x + half, y));
            path.curve_to((x + half, y + k), (x + k, y + half), (x, y + half));
            path.curve_to((x - k, y + half), (x - half, y + k), (x - half, y));
            path.curve_to((x - half, y - k), (x - k, y - half), (x, y - half));
            path.curve_to((x + k, y - half), (x + half, y - k), (x + half, y));
            path.close_path();
        }
        ParticleShape::Square | ParticleShape::Rect => {
            let height = if shape == ParticleShape::Rect {
                half * 0.5
            } else {
                half
            };
            let angle = particle.angle;
            polygon(
                path,
                &[
                    rotate(-half, -height, angle),
                    rotate(half, -height, angle),
                    rotate(half, height, angle),
                    rotate(-half, height, angle),
                ],
            );
        }
        ParticleShape::Triangle => {
            let angle = particle.angle;
            let corner = |turn: f64| {
                let theta = angle + turn * std::f64::consts::TAU / 3.0;
                rotate(half * theta.cos(), half * theta.sin(), 0.0)
            };
            polygon(path, &[corner(0.0), corner(1.0), corner(2.0)]);
        }
        ParticleShape::Streak => {
            let speed = particle.velocity.length();
            let angle = if speed > 1e-9 {
                particle.velocity.y.atan2(particle.velocity.x)
            } else {
                particle.angle
            };
            let length = (speed * STREAK_TIME).max(particle.size) * 0.5;
            let width = half * 0.5;
            polygon(
                path,
                &[
                    rotate(length, 0.0, angle),
                    rotate(0.0, width, angle),
                    rotate(-length, 0.0, angle),
                    rotate(0.0, -width, angle),
                ],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sparks() -> ParticleSystem {
        ParticleSystem {
            emitter: EmitterShape::Circle {
                radius: 0.2,
                edge: false,
            },
            rate: 60.0,
            lifetime: (0.6, 1.2),
            speed: (2.0, 4.0),
            gravity: DVec2::new(0.0, -3.0),
            drag: 0.8,
            size: (0.03, 0.08),
            spin: (-2.0, 2.0),
            colors: 4,
            seed: 9,
            bursts: vec![(0.5, 80)],
            ..ParticleSystem::default()
        }
    }

    #[test]
    fn closed_form_matches_a_fine_integration() {
        let (v0, gravity) = (DVec2::new(3.0, 2.0), DVec2::new(0.0, -3.0));
        for drag in [0.0, 1e-7, 0.8, 3.0] {
            // RK4 on dv/dt = g - k v, dp/dt = v.
            let (mut p, mut v) = (DVec2::ZERO, v0);
            let steps = 20_000;
            let dt = 1.5 / steps as f64;
            let accel = |v: DVec2| gravity - drag * v;
            for _ in 0..steps {
                let (k1v, k1p) = (accel(v), v);
                let (k2v, k2p) = (accel(v + k1v * dt / 2.0), v + k1v * dt / 2.0);
                let (k3v, k3p) = (accel(v + k2v * dt / 2.0), v + k2v * dt / 2.0);
                let (k4v, k4p) = (accel(v + k3v * dt), v + k3v * dt);
                p += (k1p + 2.0 * k2p + 2.0 * k3p + k4p) * dt / 6.0;
                v += (k1v + 2.0 * k2v + 2.0 * k3v + k4v) * dt / 6.0;
            }
            let closed = displacement(v0, gravity, drag, 1.5);
            assert!((closed - p).length() < 1e-9, "drag {drag}: {closed} vs {p}");
            let speed = velocity(v0, gravity, drag, 1.5);
            assert!((speed - v).length() < 1e-9, "drag {drag}: {speed} vs {v}");
        }
        assert_eq!(displacement(v0, gravity, 0.8, 0.0), DVec2::ZERO);
    }

    #[test]
    fn a_seek_is_exact_in_any_order() {
        let system = sparks();
        let times = [0.0, 0.25, 0.5, 0.51, 1.0, 1.7, 3.0];
        let forward: Vec<_> = times.iter().map(|&t| system.live(t, DVec2::ZERO)).collect();
        for (index, &time) in times.iter().enumerate().rev() {
            assert_eq!(system.live(time, DVec2::ZERO), forward[index]);
        }
        // The same seed reproduces the particles; another one does not.
        assert_eq!(system.clone().live(1.0, DVec2::ZERO), forward[4]);
        let reseeded = ParticleSystem {
            seed: 10,
            ..system.clone()
        };
        assert_ne!(reseeded.live(1.0, DVec2::ZERO), forward[4]);
    }

    #[test]
    fn births_follow_the_rate_and_bursts() {
        let system = ParticleSystem {
            lifetime: (1.0, 1.0),
            fade: 0.0,
            bursts: vec![(2.0, 50)],
            ..sparks()
        };
        // At 0.5 s, particles born at 0, 1/60 .. 30/60 are alive.
        assert_eq!(system.live(0.5, DVec2::ZERO).len(), 31);
        // In the steady state a 1 s life keeps exactly 60 alive.
        assert_eq!(system.live(1.5, DVec2::ZERO).len(), 60);
        // The burst adds its particles at 2.0 s and they die at 3.0 s.
        assert_eq!(system.live(2.0, DVec2::ZERO).len(), 110);
        assert_eq!(system.live(2.99, DVec2::ZERO).len(), 110);
        assert_eq!(system.live(3.0, DVec2::ZERO).len(), 60);
        // Nothing before the start, and nothing after a stop plus a life.
        let window = ParticleSystem {
            start: 1.0,
            stop: Some(2.0),
            bursts: Vec::new(),
            ..system
        };
        assert!(window.live(0.9, DVec2::ZERO).is_empty());
        assert_eq!(window.live(1.5, DVec2::ZERO).len(), 31);
        assert!(window.live(3.0, DVec2::ZERO).is_empty());
    }

    #[test]
    fn a_frame_only_visits_particles_that_can_be_alive() {
        let system = ParticleSystem {
            rate: 500.0,
            bursts: vec![(1.0, 2000), (400.0, 2000)],
            ..sparks()
        };
        // Far into the timeline, the cost stays that of one life of emission.
        let mut live = 0;
        let evaluated = system.for_each_live(399.5, |_| DVec2::ZERO, |_| live += 1);
        assert!(evaluated <= (500.0 * 1.2) as usize + 3, "{evaluated}");
        assert!(live > 0 && live <= evaluated);
        // A burst is visited only during its longest life.
        let evaluated = system.for_each_live(400.5, |_| DVec2::ZERO, |_| {});
        assert!(
            evaluated <= (500.0 * 1.2) as usize + 3 + 2000,
            "{evaluated}"
        );
    }

    #[test]
    fn attributes_stay_in_their_ranges() {
        let system = ParticleSystem {
            fade: 0.5,
            size_end: 0.0,
            ..sparks()
        };
        let mut seen_colors = [false; 4];
        system.for_each_live(
            2.0,
            |_| DVec2::new(1.0, -1.0),
            |particle| {
                assert!((0.6..1.2).contains(&particle.lifetime));
                assert!(particle.age < particle.lifetime);
                assert!(particle.alpha > 0.0 && particle.alpha <= 1.0);
                let progress = particle.age / particle.lifetime;
                assert!(particle.size <= 0.08 * (1.0 - progress) + 1e-12);
                seen_colors[particle.color] = true;
            },
        );
        assert!(seen_colors.iter().all(|&seen| seen));
        // Particles leave from inside the emitter around the origin.
        let born = system.particle(0, 7, 7.0 / 60.0, 7.0 / 60.0, DVec2::new(1.0, -1.0));
        let born = born.unwrap();
        assert!((born.position - DVec2::new(1.0, -1.0)).length() <= 0.2 + 1e-12);
        assert_eq!(born.alpha, 1.0);
    }

    #[test]
    fn every_shape_draws_a_closed_outline() {
        let particle = sparks().live(0.3, DVec2::ZERO)[0];
        for shape in [
            ParticleShape::Circle,
            ParticleShape::Square,
            ParticleShape::Rect,
            ParticleShape::Triangle,
            ParticleShape::Streak,
        ] {
            let mut path = BezPath::new();
            append_particle(&mut path, shape, &particle);
            let bounds = kurbo::Shape::bounding_box(&path);
            assert!(bounds.width() > 0.0 && bounds.height() > 0.0, "{shape:?}");
            assert!(bounds.contains(Point::new(particle.position.x, particle.position.y)));
        }
        assert_eq!(ParticleShape::parse("rect"), Some(ParticleShape::Rect));
        assert_eq!(ParticleShape::parse("blob"), None);
    }

    /// `cargo test -p gaanim_math particles_benchmark -- --ignored --nocapture`
    /// times one frame of 2000 live particles: evaluation and outlines.
    #[test]
    #[ignore = "timing benchmark; run explicitly"]
    fn particles_benchmark_2000() {
        let system = ParticleSystem {
            rate: 0.0,
            lifetime: (4.0, 6.0),
            fade: 0.3,
            bursts: vec![(0.0, 2000)],
            ..sparks()
        };
        let frames = 240;
        let started = std::time::Instant::now();
        let mut drawn = 0;
        for frame in 0..frames {
            // Dispersed seeks, as the runtime harness does.
            let time = ((frame * 37) % frames) as f64 / 60.0;
            let mut path = BezPath::new();
            system.for_each_live(
                time,
                |_| DVec2::ZERO,
                |particle| {
                    drawn += 1;
                    append_particle(&mut path, ParticleShape::Circle, &particle);
                },
            );
            std::hint::black_box(&path);
        }
        let per_frame = started.elapsed().as_secs_f64() / frames as f64;
        assert_eq!(drawn, 2000 * frames);
        println!(
            "2000 particles: {:.1} us per frame ({:.0} ns per particle)",
            per_frame * 1e6,
            per_frame * 1e9 / 2000.0
        );
    }
}
