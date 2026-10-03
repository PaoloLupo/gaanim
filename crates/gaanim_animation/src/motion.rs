//! Motion authored as data: multi-stop keyframes, analytic throws with
//! bounces and inertial glides to a snap point.
//!
//! Each one lowers to a [`CustomAnimation`] over the clip's progress, so an
//! exact seek and continuous playback evaluate the same pure function.

use gaanim_core::{
    glam::{DVec2, DVec3},
    peniko::Brush,
};
use gaanim_math::RateFunc;

use crate::{CustomAnimation, CustomBaseline, CustomChannel, CustomValues};

/// How keyframed positions are joined between stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Spatial {
    /// Straight segments, like Remotion's `interpolate`.
    #[default]
    Linear,
    /// A centripetal Catmull-Rom curve through every stop, like After
    /// Effects' spatial Bézier keyframes.
    CatmullRom,
}

/// Values of one channel at each stop; `None` keeps the value the target
/// has when the animation starts.
pub type Stops<T> = Option<Vec<Option<T>>>;

/// One clip with several stops and an easing per segment between them.
#[derive(Debug, Clone, Default)]
pub struct Keyframes {
    /// Stop instants as fractions of the clip, from `0` to `1`.
    pub times: Vec<f64>,
    /// Translation of the drawable's origin, like `move_to` on an object
    /// created at the origin.
    pub position: Stops<DVec3>,
    /// Displacement from the translation the clip starts at; `None` stops
    /// keep that translation. Exclusive with `position`.
    pub offset: Stops<DVec3>,
    /// Absolute rotation about Z, in radians.
    pub rotation: Stops<f64>,
    pub scale: Stops<DVec3>,
    pub opacity: Stops<f32>,
    pub fill: Stops<Brush>,
    pub stroke: Stops<Brush>,
    /// One easing per segment; empty means linear segments.
    pub easings: Vec<RateFunc>,
    pub spatial: Spatial,
}

impl Keyframes {
    /// The channels these keyframes write.
    pub fn channels(&self) -> Vec<CustomChannel> {
        let mut channels = Vec::new();
        if self.position.is_some() || self.offset.is_some() {
            channels.push(CustomChannel::Position);
        }
        if self.rotation.is_some() {
            channels.push(CustomChannel::Rotation);
        }
        if self.scale.is_some() {
            channels.push(CustomChannel::Scale);
        }
        if self.opacity.is_some() {
            channels.push(CustomChannel::Opacity);
        }
        if self.fill.is_some() {
            channels.push(CustomChannel::Fill);
        }
        if self.stroke.is_some() {
            channels.push(CustomChannel::Stroke);
        }
        channels
    }

    /// Checks stop instants, channel lengths, values and easings.
    pub fn validate(&self) -> Result<(), String> {
        validate_times(&self.times, self.easings.len())?;
        let stops = self.times.len();
        let lengths = [
            ("position", self.position.as_ref().map(Vec::len)),
            ("offset", self.offset.as_ref().map(Vec::len)),
            ("rotation", self.rotation.as_ref().map(Vec::len)),
            ("scale", self.scale.as_ref().map(Vec::len)),
            ("opacity", self.opacity.as_ref().map(Vec::len)),
            ("fill", self.fill.as_ref().map(Vec::len)),
            ("stroke", self.stroke.as_ref().map(Vec::len)),
        ];
        if lengths.iter().all(|(_, length)| length.is_none()) {
            return Err("keyframes() requires at least one channel".into());
        }
        if self.position.is_some() && self.offset.is_some() {
            return Err("keyframes() takes position= or offset=, not both".into());
        }
        for (name, length) in lengths {
            if let Some(length) = length
                && length != stops
            {
                return Err(format!(
                    "keyframes() {name} has {length} values for {stops} times"
                ));
            }
        }
        let finite = |values: &Stops<DVec3>| {
            values
                .iter()
                .flatten()
                .flatten()
                .all(|value| value.is_finite())
        };
        if !finite(&self.position)
            || !finite(&self.offset)
            || !finite(&self.scale)
            || self
                .rotation
                .iter()
                .flatten()
                .flatten()
                .any(|value| !value.is_finite())
        {
            return Err("keyframes() values must be finite".into());
        }
        if self
            .opacity
            .iter()
            .flatten()
            .flatten()
            .any(|value| !(0.0..=1.0).contains(value))
        {
            return Err("keyframes() opacity must be within [0, 1]".into());
        }
        for paints in [&self.fill, &self.stroke].into_iter().flatten() {
            for paint in paints.iter().flatten() {
                crate::paint::validate_paint(paint).map_err(str::to_owned)?;
            }
            for pair in paints.windows(2) {
                if let [Some(from), Some(to)] = pair {
                    crate::paint::validate_paint_transition(from, to).map_err(str::to_owned)?;
                }
            }
        }
        Ok(())
    }

    /// The final fill and stroke, when they are set by the last stop.
    pub fn final_paints(&self) -> (Option<Brush>, Option<Brush>) {
        let last =
            |paints: &Stops<Brush>| paints.as_ref().and_then(|paints| paints.last()?.clone());
        (last(&self.fill), last(&self.stroke))
    }

    /// The animation from `start`, which fills the stops left `None`.
    pub fn animation(&self, start: &CustomBaseline) -> Result<CustomAnimation, String> {
        self.validate()?;
        let filled = |stops: &Stops<DVec3>, current: DVec3| {
            stops.as_ref().map(|stops| {
                stops
                    .iter()
                    .map(|value| value.unwrap_or(current))
                    .collect::<Vec<_>>()
            })
        };
        let origin = start.transform.translation;
        let position = filled(&self.position, origin).or_else(|| {
            self.offset.as_ref().map(|stops| {
                stops
                    .iter()
                    .map(|value| origin + value.unwrap_or(DVec3::ZERO))
                    .collect()
            })
        });
        let scale = filled(&self.scale, start.transform.scale);
        let turn = z_rotation(start.transform.rotation);
        let rotation = self.rotation.as_ref().map(|stops| {
            stops
                .iter()
                .map(|value| value.unwrap_or(turn))
                .collect::<Vec<_>>()
        });
        let opacity = self.opacity.as_ref().map(|stops| {
            stops
                .iter()
                .map(|value| value.unwrap_or(start.opacity))
                .collect::<Vec<_>>()
        });
        let paints = |stops: &Stops<Brush>, current: Option<&Brush>, name: &str| {
            stops
                .as_ref()
                .map(|stops| {
                    stops
                        .iter()
                        .map(|value| {
                            value.clone().or_else(|| current.cloned()).ok_or_else(|| {
                                format!("keyframes() {name} needs a first value: the drawable has no {name}")
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()
        };
        let fill = paints(&self.fill, start.fill.as_ref(), "fill")?;
        let stroke = paints(&self.stroke, start.stroke.brush.as_ref(), "stroke")?;
        for paints in [&fill, &stroke].into_iter().flatten() {
            for pair in paints.windows(2) {
                crate::paint::validate_paint_transition(&pair[0], &pair[1])
                    .map_err(str::to_owned)?;
            }
        }
        let times = self.times.clone();
        let easings = self.easings.clone();
        let spatial = self.spatial;
        CustomAnimation::new(self.channels(), move |alpha| {
            let (index, local) = segment(&times, alpha);
            let eased = easings
                .get(index)
                .map_or(local, |easing| easing.evaluate(local));
            let lerp = |from: f64, to: f64| from + (to - from) * eased;
            Ok(CustomValues {
                position: position.as_ref().map(|stops| match spatial {
                    Spatial::Linear => stops[index].lerp(stops[index + 1], eased),
                    Spatial::CatmullRom => catmull_rom(stops, index, eased),
                }),
                rotation: rotation
                    .as_ref()
                    .map(|stops| lerp(stops[index], stops[index + 1])),
                scale: scale
                    .as_ref()
                    .map(|stops| stops[index].lerp(stops[index + 1], eased)),
                opacity: opacity.as_ref().map(|stops| {
                    (lerp(stops[index] as f64, stops[index + 1] as f64) as f32).clamp(0.0, 1.0)
                }),
                fill: fill.as_ref().map(|stops| {
                    crate::paint::interpolate_paint(&stops[index], &stops[index + 1], eased)
                }),
                stroke: stroke.as_ref().map(|stops| {
                    crate::paint::interpolate_paint(&stops[index], &stops[index + 1], eased)
                }),
                ..Default::default()
            })
        })
    }
}

/// Keyframes of a single number, such as a `Parameter`.
#[derive(Debug, Clone, Default)]
pub struct ScalarKeyframes {
    pub times: Vec<f64>,
    /// `None` keeps the value at the start.
    pub values: Vec<Option<f64>>,
    pub easings: Vec<RateFunc>,
}

impl ScalarKeyframes {
    pub fn validate(&self) -> Result<(), String> {
        validate_times(&self.times, self.easings.len())?;
        if self.values.len() != self.times.len() {
            return Err(format!(
                "keyframes() has {} values for {} times",
                self.values.len(),
                self.times.len()
            ));
        }
        if self.values.iter().flatten().any(|value| !value.is_finite()) {
            return Err("keyframes() values must be finite".into());
        }
        Ok(())
    }

    /// Every value set by a stop.
    pub fn values(&self) -> impl Iterator<Item = f64> + '_ {
        self.values.iter().flatten().copied()
    }

    /// The value at the end, from `start`.
    pub fn end(&self, start: f64) -> f64 {
        self.values.last().copied().flatten().unwrap_or(start)
    }

    /// Value at `alpha` (clip progress from 0 to 1), from `start`.
    pub fn sampler(&self, start: f64) -> impl Fn(f64) -> f64 + Send + Sync + 'static {
        let values: Vec<f64> = self
            .values
            .iter()
            .map(|value| value.unwrap_or(start))
            .collect();
        let times = self.times.clone();
        let easings = self.easings.clone();
        move |alpha| {
            let (index, local) = segment(&times, alpha);
            let eased = easings
                .get(index)
                .map_or(local, |easing| easing.evaluate(local));
            values[index] + (values[index + 1] - values[index]) * eased
        }
    }
}

fn validate_times(times: &[f64], easings: usize) -> Result<(), String> {
    if times.len() < 2 {
        return Err("keyframes() requires at least two times".into());
    }
    if times[0] != 0.0 || times[times.len() - 1] != 1.0 {
        return Err(
            "keyframes() times are fractions of the animation: the first must be 0 and the last 1"
                .into(),
        );
    }
    if times.iter().any(|time| !time.is_finite()) {
        return Err("keyframes() times must be finite".into());
    }
    if times.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("keyframes() times must increase".into());
    }
    if easings > 1 && easings != times.len() - 1 {
        return Err(format!(
            "keyframes() takes one easing, or one per segment ({}), not {easings}",
            times.len() - 1
        ));
    }
    Ok(())
}

/// The segment holding `alpha` and the progress through it. `alpha` is
/// clamped to the clip, so an overshooting easing holds the end stops.
fn segment(times: &[f64], alpha: f64) -> (usize, f64) {
    let alpha = alpha.clamp(0.0, 1.0);
    let last = times.len() - 2;
    let index = times[1..=last].partition_point(|time| *time <= alpha);
    let (from, to) = (times[index], times[index + 1]);
    (index, ((alpha - from) / (to - from)).clamp(0.0, 1.0))
}

/// Point `u` of the way from stop `index` to the next on a centripetal
/// Catmull-Rom curve. The ends continue straight, as if mirrored.
fn catmull_rom(stops: &[DVec3], index: usize, u: f64) -> DVec3 {
    let p1 = stops[index];
    let p2 = stops[index + 1];
    let p0 = if index > 0 {
        stops[index - 1]
    } else {
        p1 * 2.0 - p2
    };
    let p3 = stops.get(index + 2).copied().unwrap_or(p2 * 2.0 - p1);
    // Knot spacing by the square root of each chord avoids cusps and loops.
    let knot = |a: DVec3, b: DVec3| a.distance(b).sqrt().max(1e-6);
    let t0 = 0.0;
    let t1 = t0 + knot(p0, p1);
    let t2 = t1 + knot(p1, p2);
    let t3 = t2 + knot(p2, p3);
    if p1.distance(p2) < 1e-12 {
        return p1;
    }
    let t = t1 + (t2 - t1) * u;
    let mix = |a: DVec3, b: DVec3, from: f64, to: f64| {
        a * ((to - t) / (to - from)) + b * ((t - from) / (to - from))
    };
    let a1 = mix(p0, p1, t0, t1);
    let a2 = mix(p1, p2, t1, t2);
    let a3 = mix(p2, p3, t2, t3);
    let b1 = mix(a1, a2, t0, t2);
    let b2 = mix(a2, a3, t1, t3);
    mix(b1, b2, t1, t2)
}

/// Absolute rotation about Z of a rotation that only turns about Z.
fn z_rotation(rotation: gaanim_core::glam::DQuat) -> f64 {
    2.0 * rotation.z.atan2(rotation.w)
}

/// A bounce lower than this ends a throw, in scene units.
const REST_HEIGHT: f64 = 1e-3;
/// Most bounces a throw makes before it rests.
const MAX_BOUNCES: usize = 64;

/// A thrown object: a ballistic arc that bounces on a floor until it rests.
/// Offsets are relative to the start, so it plays from wherever the object
/// is.
#[derive(Debug, Clone, PartialEq)]
pub struct Throw {
    velocity: DVec2,
    gravity: f64,
    /// Start time and upward speed of each arc after the first, all from
    /// the floor.
    bounces: Vec<(f64, f64)>,
    /// Height of the floor below the start, or `None` without a floor.
    drop: Option<f64>,
    duration: f64,
}

impl Throw {
    /// `velocity` in units per second, `gravity` in units per second squared
    /// (downward), `drop` the floor's height relative to the object's lowest
    /// point (zero or negative), `restitution` the share of speed kept by
    /// each bounce. Without a floor the throw lasts until the object falls
    /// back to its starting height.
    pub fn new(
        velocity: DVec2,
        gravity: f64,
        drop: Option<f64>,
        restitution: f64,
    ) -> Result<Self, String> {
        if !velocity.is_finite() {
            return Err("throw() velocity must be finite".into());
        }
        if !(gravity.is_finite() && gravity > 0.0) {
            return Err("throw() gravity must be positive".into());
        }
        if !(0.0..1.0).contains(&restitution) {
            return Err("throw() restitution must be within [0, 1)".into());
        }
        let Some(drop) = drop else {
            if velocity.y <= 0.0 {
                return Err("throw() without a floor needs an upward velocity, so the object comes back to its starting height".into());
            }
            return Ok(Self {
                velocity,
                gravity,
                bounces: Vec::new(),
                drop: None,
                duration: 2.0 * velocity.y / gravity,
            });
        };
        if !drop.is_finite() {
            return Err("throw() floor must be finite".into());
        }
        if drop > 1e-9 {
            return Err("throw() starts below its floor".into());
        }
        let drop = drop.min(0.0);
        // First contact: vy t - g t² / 2 = drop.
        let first =
            (velocity.y + (velocity.y * velocity.y - 2.0 * gravity * drop).sqrt()) / gravity;
        let mut speed = (gravity * first - velocity.y) * restitution;
        let mut time = first;
        let mut bounces = Vec::new();
        while speed * speed / (2.0 * gravity) > REST_HEIGHT && bounces.len() < MAX_BOUNCES {
            bounces.push((time, speed));
            time += 2.0 * speed / gravity;
            speed *= restitution;
        }
        Ok(Self {
            velocity,
            gravity,
            bounces,
            drop: Some(drop),
            duration: time,
        })
    }

    /// Seconds until the object rests (or falls back without a floor).
    pub fn duration(&self) -> f64 {
        self.duration
    }

    /// Offset from the start `seconds` into the throw.
    pub fn offset(&self, seconds: f64) -> DVec2 {
        let seconds = seconds.clamp(0.0, self.duration);
        let x = self.velocity.x * seconds;
        let arc = |elapsed: f64, from: f64, speed: f64| {
            from + speed * elapsed - 0.5 * self.gravity * elapsed * elapsed
        };
        let y = match self
            .bounces
            .iter()
            .rev()
            .find(|(start, _)| *start <= seconds)
        {
            Some(&(start, speed)) => arc(seconds - start, self.drop.unwrap_or(0.0), speed),
            None => arc(seconds, 0.0, self.velocity.y),
        };
        let y = match self.drop {
            Some(drop) if seconds >= self.duration => drop,
            Some(drop) => y.max(drop),
            None if seconds >= self.duration => 0.0,
            None => y,
        };
        DVec2::new(x, y)
    }

    pub fn animation(&self, start: &CustomBaseline) -> Result<CustomAnimation, String> {
        let origin = start.transform.translation;
        let throw = self.clone();
        CustomAnimation::new(vec![CustomChannel::Position], move |alpha| {
            let offset = throw.offset(alpha.clamp(0.0, 1.0) * throw.duration);
            Ok(CustomValues {
                position: Some(origin + offset.extend(0.0)),
                ..Default::default()
            })
        })
    }
}

/// Where an inertial glide may come to rest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Snap {
    /// Any point with this x.
    X(f64),
    Point(DVec2),
}

/// Remaining share of the glide when an inertial glide ends.
const INERTIA_REST: f64 = 1e-3;

/// A glide that decelerates exponentially, like a flicked object, and ends
/// at the snap point nearest to where it would rest.
#[derive(Debug, Clone, PartialEq)]
pub struct Inertia {
    velocity: DVec2,
    friction: f64,
    snap: Vec<Snap>,
}

impl Inertia {
    /// `velocity` in units per second, `friction` the deceleration rate per
    /// second: the speed falls by `e` every `1 / friction` seconds.
    pub fn new(velocity: DVec2, friction: f64, snap: Vec<Snap>) -> Result<Self, String> {
        if !velocity.is_finite() {
            return Err("inertia() velocity must be finite".into());
        }
        if !(friction.is_finite() && friction > 0.0) {
            return Err("inertia() friction must be positive".into());
        }
        if snap.iter().any(|snap| match snap {
            Snap::X(x) => !x.is_finite(),
            Snap::Point(point) => !point.is_finite(),
        }) {
            return Err("inertia() snap points must be finite".into());
        }
        Ok(Self {
            velocity,
            friction,
            snap,
        })
    }

    /// Seconds until the glide is within 0.1% of its end.
    pub fn duration(&self) -> f64 {
        (1.0 / INERTIA_REST).ln() / self.friction
    }

    /// Where a glide from `start` ends: its natural rest, moved to the
    /// nearest snap point.
    pub fn rest(&self, start: DVec2) -> DVec2 {
        let natural = start + self.velocity / self.friction;
        self.snap
            .iter()
            .map(|snap| match *snap {
                Snap::X(x) => DVec2::new(x, natural.y),
                Snap::Point(point) => point,
            })
            .min_by(|a, b| a.distance(natural).total_cmp(&b.distance(natural)))
            .unwrap_or(natural)
    }

    /// Share of the glide covered `seconds` in, reaching exactly 1 at the
    /// end.
    fn covered(&self, seconds: f64) -> f64 {
        let end = 1.0 - (-self.friction * self.duration()).exp();
        (1.0 - (-self.friction * seconds.clamp(0.0, self.duration())).exp()) / end
    }

    pub fn animation(&self, start: &CustomBaseline) -> Result<CustomAnimation, String> {
        let origin = start.transform.translation;
        let rest = self.rest(origin.truncate());
        let inertia = self.clone();
        CustomAnimation::new(vec![CustomChannel::Position], move |alpha| {
            let covered = inertia.covered(alpha.clamp(0.0, 1.0) * inertia.duration());
            let at = origin.truncate().lerp(rest, covered);
            Ok(CustomValues {
                position: Some(at.extend(origin.z)),
                ..Default::default()
            })
        })
    }
}

/// Motion that `Drawable.animate` describes with data instead of targets.
#[derive(Debug, Clone)]
pub enum Motion {
    Keyframes(Box<Keyframes>),
    Throw(Throw),
    Inertia(Inertia),
}

impl Motion {
    pub fn channels(&self) -> Vec<CustomChannel> {
        match self {
            Self::Keyframes(keyframes) => keyframes.channels(),
            Self::Throw(_) | Self::Inertia(_) => vec![CustomChannel::Position],
        }
    }

    /// The animation of a target in state `start`.
    pub fn animation(&self, start: &CustomBaseline) -> Result<CustomAnimation, String> {
        match self {
            Self::Keyframes(keyframes) => keyframes.animation(start),
            Self::Throw(throw) => throw.animation(start),
            Self::Inertia(inertia) => inertia.animation(start),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Keyframes(_) => "Keyframes",
            Self::Throw(_) => "Throw",
            Self::Inertia(_) => "Inertia",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_math::SpatialTransform;

    fn baseline(at: DVec3) -> CustomBaseline {
        CustomBaseline {
            transform: SpatialTransform {
                translation: at,
                ..Default::default()
            },
            opacity: 1.0,
            fill: None,
            stroke: Default::default(),
        }
    }

    fn close(a: DVec3, b: DVec3) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn keyframes_pass_through_every_stop_at_its_time() {
        for spatial in [Spatial::Linear, Spatial::CatmullRom] {
            let keyframes = Keyframes {
                times: vec![0.0, 0.35, 0.7, 1.0],
                position: Some(vec![
                    None,
                    Some(DVec3::new(0.0, 2.5, 0.0)),
                    Some(DVec3::new(2.5, 0.0, 0.0)),
                    Some(DVec3::new(4.0, 0.0, 0.0)),
                ]),
                scale: Some(vec![
                    Some(DVec3::ONE),
                    Some(DVec3::ONE),
                    Some(DVec3::new(1.25, 0.8, 1.0)),
                    Some(DVec3::ONE),
                ]),
                easings: vec![
                    RateFunc::EaseOut(gaanim_math::EasingCurve::Quadratic),
                    RateFunc::EaseIn(gaanim_math::EasingCurve::Quadratic),
                    RateFunc::Smooth,
                ],
                spatial,
                ..Default::default()
            };
            let animation = keyframes
                .animation(&baseline(DVec3::new(-4.0, 0.0, 0.0)))
                .unwrap();
            let at = |alpha| animation.evaluate(alpha).unwrap();
            assert!(close(at(0.0).position.unwrap(), DVec3::new(-4.0, 0.0, 0.0)));
            assert!(close(at(0.35).position.unwrap(), DVec3::new(0.0, 2.5, 0.0)));
            assert!(close(at(0.7).position.unwrap(), DVec3::new(2.5, 0.0, 0.0)));
            assert!(close(at(0.7).scale.unwrap(), DVec3::new(1.25, 0.8, 1.0)));
            assert!(close(at(1.0).position.unwrap(), DVec3::new(4.0, 0.0, 0.0)));
            assert!(at(0.5).rotation.is_none());
        }
    }

    #[test]
    fn offsets_move_from_where_the_clip_starts() {
        let keyframes = Keyframes {
            times: vec![0.0, 0.5, 1.0],
            offset: Some(vec![None, Some(DVec3::new(3.0, 0.0, 0.0)), None]),
            ..Default::default()
        };
        let animation = keyframes
            .animation(&baseline(DVec3::new(0.0, 2.27, 0.0)))
            .unwrap();
        let at = |alpha| animation.evaluate(alpha).unwrap().position.unwrap();
        assert!(close(at(0.5), DVec3::new(3.0, 2.27, 0.0)));
        assert!(close(at(1.0), DVec3::new(0.0, 2.27, 0.0)));
        let both = Keyframes {
            position: Some(vec![None, None, None]),
            ..keyframes
        };
        assert!(both.validate().unwrap_err().contains("not both"));
    }

    #[test]
    fn catmull_rom_curves_through_the_stops_instead_of_cornering() {
        let stops = [
            DVec3::new(-2.0, 0.0, 0.0),
            DVec3::new(0.0, 2.0, 0.0),
            DVec3::new(2.0, 0.0, 0.0),
        ];
        let mid = catmull_rom(&stops, 0, 0.5);
        let straight = stops[0].lerp(stops[1], 0.5);
        // The curve bulges outward of the straight chord toward the peak.
        assert!(mid.y > straight.y);
        // Tangent at the middle stop is continuous: both sides head the same way.
        let before = catmull_rom(&stops, 0, 0.999) - catmull_rom(&stops, 0, 0.998);
        let after = catmull_rom(&stops, 1, 0.002) - catmull_rom(&stops, 1, 0.001);
        assert!(before.normalize().dot(after.normalize()) > 0.999);
    }

    #[test]
    fn keyframes_reject_bad_times_and_lengths() {
        let mut keyframes = Keyframes {
            times: vec![0.0, 0.5, 1.0],
            opacity: Some(vec![Some(0.0), Some(1.0), Some(0.5)]),
            ..Default::default()
        };
        assert!(keyframes.validate().is_ok());
        keyframes.times = vec![0.0, 1.0, 1.0];
        assert!(keyframes.validate().unwrap_err().contains("increase"));
        keyframes.times = vec![0.0, 0.5, 0.9];
        assert!(keyframes.validate().unwrap_err().contains("last 1"));
        keyframes.times = vec![0.0, 1.0];
        assert!(
            keyframes
                .validate()
                .unwrap_err()
                .contains("3 values for 2 times")
        );
        keyframes.times = vec![0.0, 0.5, 1.0];
        keyframes.easings = vec![RateFunc::Linear; 3];
        assert!(
            keyframes
                .validate()
                .unwrap_err()
                .contains("one per segment")
        );
        keyframes.easings.clear();
        keyframes.opacity = None;
        assert!(
            keyframes
                .validate()
                .unwrap_err()
                .contains("at least one channel")
        );
    }

    #[test]
    fn scalar_keyframes_hold_their_values_at_each_stop() {
        let keyframes = ScalarKeyframes {
            times: vec![0.0, 0.5, 1.0],
            values: vec![None, Some(80.0), Some(100.0)],
            easings: vec![RateFunc::Smooth],
        };
        keyframes.validate().unwrap();
        let sample = keyframes.sampler(10.0);
        assert_eq!(sample(0.0), 10.0);
        assert_eq!(sample(0.5), 80.0);
        assert_eq!(sample(1.0), 100.0);
        assert_eq!(sample(0.25), 45.0);
        assert_eq!(keyframes.end(10.0), 100.0);
    }

    #[test]
    fn a_throw_bounces_lower_each_time_and_rests_on_the_floor() {
        let throw = Throw::new(DVec2::new(3.0, 6.0), 9.8, Some(-3.0), 0.55).unwrap();
        let rest = throw.offset(throw.duration());
        assert!((rest.y + 3.0).abs() < 1e-12);
        assert!((rest.x - 3.0 * throw.duration()).abs() < 1e-9);
        // Never below the floor, and the bounces shrink.
        let mut peaks = Vec::new();
        for window in throw.bounces.windows(2) {
            let (start, speed) = window[0];
            assert!(window[1].1 < speed);
            let top = throw.offset(start + speed / 9.8);
            peaks.push(top.y);
        }
        assert!(peaks.windows(2).all(|pair| pair[1] < pair[0]));
        for step in 0..=1000 {
            let offset = throw.offset(throw.duration() * step as f64 / 1000.0);
            assert!(offset.y >= -3.0 - 1e-12);
        }
        // Continuous through each bounce.
        for &(start, _) in &throw.bounces {
            let before = throw.offset(start - 1e-7);
            let after = throw.offset(start + 1e-7);
            assert!(before.distance(after) < 1e-4);
        }
        // First apex follows from v² / 2g.
        let apex = throw.offset(6.0 / 9.8);
        assert!((apex.y - 36.0 / (2.0 * 9.8)).abs() < 1e-9);
    }

    #[test]
    fn a_throw_without_a_floor_falls_back_to_its_height() {
        let throw = Throw::new(DVec2::new(1.0, 4.9), 9.8, None, 0.5).unwrap();
        assert!((throw.duration() - 1.0).abs() < 1e-12);
        assert!(close(
            throw.offset(1.0).extend(0.0),
            DVec3::new(1.0, 0.0, 0.0)
        ));
        assert!(Throw::new(DVec2::new(1.0, -1.0), 9.8, None, 0.5).is_err());
        assert!(Throw::new(DVec2::new(1.0, 1.0), 9.8, Some(0.5), 0.5).is_err());
        assert!(Throw::new(DVec2::new(1.0, 1.0), 9.8, Some(-1.0), 1.0).is_err());
    }

    #[test]
    fn a_dead_throw_rests_at_first_contact() {
        let throw = Throw::new(DVec2::new(0.0, 0.0), 2.0, Some(-1.0), 0.0).unwrap();
        assert!((throw.duration() - 1.0).abs() < 1e-12);
        assert!(throw.bounces.is_empty());
    }

    #[test]
    fn inertia_glides_to_the_nearest_snap() {
        let inertia = Inertia::new(
            DVec2::new(4.0, 0.0),
            3.0,
            vec![Snap::X(-2.0), Snap::X(0.0), Snap::X(2.0)],
        )
        .unwrap();
        // Natural rest at 4/3, nearest snap 2.
        assert_eq!(inertia.rest(DVec2::ZERO), DVec2::new(2.0, 0.0));
        let animation = inertia.animation(&baseline(DVec3::ZERO)).unwrap();
        let at = |alpha| animation.evaluate(alpha).unwrap().position.unwrap();
        assert!(close(at(0.0), DVec3::ZERO));
        assert!(close(at(1.0), DVec3::new(2.0, 0.0, 0.0)));
        // Fast first, then slow: half the way in a tenth of the time.
        assert!(at(0.1).x > 0.49 * 2.0 && at(0.2).x > 0.7 * 2.0);
        let free = Inertia::new(DVec2::new(0.0, 3.0), 1.5, Vec::new()).unwrap();
        assert_eq!(free.rest(DVec2::new(1.0, 0.0)), DVec2::new(1.0, 2.0));
        let point = Inertia::new(
            DVec2::new(3.0, 3.0),
            1.0,
            vec![Snap::Point(DVec2::new(3.0, 2.0)), Snap::Point(DVec2::ZERO)],
        )
        .unwrap();
        assert_eq!(point.rest(DVec2::ZERO), DVec2::new(3.0, 2.0));
    }
}
