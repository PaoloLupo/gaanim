//! Repeaters and duplicators: copies of a drawable placed by a cumulative
//! transform or a distribution, gathered in one group.

use gaanim_core::ObjectId;
use gaanim_core::glam::DVec2;

use super::types::{ObjectSpec, SpawnKind};
use super::{DrawableHandle, SceneModel};

/// Most copies one repeat or duplicate creates.
pub const MAX_COPIES: usize = 10_000;

/// Where the copies of a duplicated drawable go.
#[derive(Debug, Clone, PartialEq)]
pub enum Distribution {
    /// `columns` x `rows` cells `spacing` apart, centred on `center`, filled
    /// row by row from the top left.
    Grid {
        columns: usize,
        rows: usize,
        spacing: DVec2,
        center: DVec2,
    },
    /// `count` points on a circle, counter-clockwise from `start` radians;
    /// with `orient` each copy turns to face outward.
    Circle {
        count: usize,
        radius: f64,
        center: DVec2,
        start: f64,
        orient: bool,
    },
    /// `count` points evenly spaced by arc length along a polyline, both
    /// ends included; with `orient` each copy follows the direction.
    Along {
        points: Vec<DVec2>,
        count: usize,
        orient: bool,
    },
    /// `count` seeded uniform points inside `min`..`max`.
    Random {
        count: usize,
        min: DVec2,
        max: DVec2,
        seed: u64,
    },
    /// `count` points on a sunflower spiral: point `i` at radius
    /// `spacing * sqrt(i)`, turned by the golden angle.
    Phyllotaxis {
        count: usize,
        spacing: f64,
        center: DVec2,
    },
}

impl Distribution {
    /// Position and rotation (radians) of every copy.
    pub fn placements(&self) -> Vec<(DVec2, f64)> {
        match self {
            Self::Grid {
                columns,
                rows,
                spacing,
                center,
            } => {
                let origin = *center
                    - DVec2::new(
                        spacing.x * (*columns as f64 - 1.0) / 2.0,
                        -spacing.y * (*rows as f64 - 1.0) / 2.0,
                    );
                (0..*rows)
                    .flat_map(|row| {
                        (0..*columns).map(move |column| {
                            (
                                origin
                                    + DVec2::new(
                                        spacing.x * column as f64,
                                        -spacing.y * row as f64,
                                    ),
                                0.0,
                            )
                        })
                    })
                    .collect()
            }
            Self::Circle {
                count,
                radius,
                center,
                start,
                orient,
            } => (0..*count)
                .map(|index| {
                    let angle = start + std::f64::consts::TAU * index as f64 / *count as f64;
                    (
                        *center + DVec2::new(angle.cos(), angle.sin()) * *radius,
                        if *orient { angle } else { 0.0 },
                    )
                })
                .collect(),
            Self::Along {
                points,
                count,
                orient,
            } => along(points, *count, *orient),
            Self::Random {
                count,
                min,
                max,
                seed,
            } => {
                let mut state = *seed;
                let mut next = || {
                    // SplitMix64.
                    state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
                    let mut z = state;
                    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
                    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
                    z ^= z >> 31;
                    (z >> 11) as f64 / (1_u64 << 53) as f64
                };
                (0..*count)
                    .map(|_| {
                        let x = min.x + (max.x - min.x) * next();
                        let y = min.y + (max.y - min.y) * next();
                        (DVec2::new(x, y), 0.0)
                    })
                    .collect()
            }
            Self::Phyllotaxis {
                count,
                spacing,
                center,
            } => {
                let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
                (0..*count)
                    .map(|index| {
                        let angle = golden * index as f64;
                        let radius = spacing * (index as f64).sqrt();
                        (*center + DVec2::new(angle.cos(), angle.sin()) * radius, 0.0)
                    })
                    .collect()
            }
        }
    }

    /// Validate the distribution's parameters.
    pub fn validate(&self) -> Result<(), String> {
        let finite = |values: &[f64]| values.iter().all(|value| value.is_finite());
        let count = match self {
            Self::Grid {
                columns,
                rows,
                spacing,
                center,
            } => {
                if !finite(&[spacing.x, spacing.y, center.x, center.y]) {
                    return Err("grid spacing and center must be finite".to_string());
                }
                columns.saturating_mul(*rows)
            }
            Self::Circle {
                count,
                radius,
                center,
                start,
                ..
            } => {
                if !finite(&[*radius, center.x, center.y, *start]) {
                    return Err("circle radius, center and start must be finite".to_string());
                }
                *count
            }
            Self::Along { points, count, .. } => {
                if points.len() < 2 || points.iter().any(|point| !finite(&[point.x, point.y])) {
                    return Err("along() needs at least two finite points".to_string());
                }
                *count
            }
            Self::Random {
                count, min, max, ..
            } => {
                if !finite(&[min.x, min.y, max.x, max.y]) || min.x > max.x || min.y > max.y {
                    return Err("random bounds must be finite (xmin, ymin, xmax, ymax)".to_string());
                }
                *count
            }
            Self::Phyllotaxis {
                count,
                spacing,
                center,
            } => {
                if !finite(&[*spacing, center.x, center.y]) {
                    return Err("phyllotaxis spacing and center must be finite".to_string());
                }
                *count
            }
        };
        if count == 0 || count > MAX_COPIES {
            return Err(format!(
                "a distribution places between 1 and {MAX_COPIES} copies, got {count}"
            ));
        }
        Ok(())
    }
}

/// `count` points spaced evenly by arc length along `points`.
fn along(points: &[DVec2], count: usize, orient: bool) -> Vec<(DVec2, f64)> {
    let lengths: Vec<f64> = points
        .windows(2)
        .map(|pair| pair[0].distance(pair[1]))
        .collect();
    let total: f64 = lengths.iter().sum();
    (0..count)
        .map(|index| {
            let mut remaining = if count > 1 {
                total * index as f64 / (count - 1) as f64
            } else {
                0.0
            };
            for (segment, &length) in lengths.iter().enumerate() {
                let last = segment + 1 == lengths.len();
                if remaining <= length || last {
                    let t = if length > 0.0 {
                        (remaining / length).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let (a, b) = (points[segment], points[segment + 1]);
                    let direction = b - a;
                    let angle = if orient {
                        direction.y.atan2(direction.x)
                    } else {
                        0.0
                    };
                    return (a + direction * t, angle);
                }
                remaining -= length;
            }
            (points[0], 0.0)
        })
        .collect()
}

/// How each successive copy of [`SceneModel::repeat`] differs from the one
/// before it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RepeatStep {
    /// Added rotation, radians.
    pub rotate: f64,
    /// Scale factor.
    pub scale: f64,
    /// Added translation, scene units.
    pub offset: DVec2,
    /// Opacity of the first and the last copy, interpolated in between.
    pub opacity: (f32, f32),
    /// Point the copies turn and scale about; their own pivot when `None`.
    pub about: Option<DVec2>,
}

impl Default for RepeatStep {
    fn default() -> Self {
        Self {
            rotate: 0.0,
            scale: 1.0,
            offset: DVec2::ZERO,
            opacity: (1.0, 1.0),
            about: None,
        }
    }
}

impl SceneModel {
    /// `count` copies of `source`, each turned by `step.rotate`, scaled by
    /// `step.scale` and shifted by `step.offset` more than the one before,
    /// like After Effects' Repeater. `source` itself is copy 0. The copies
    /// are the members of the returned group, in order.
    pub fn repeat(
        &mut self,
        source: &DrawableHandle,
        count: usize,
        step: RepeatStep,
    ) -> Result<DrawableHandle, String> {
        if count == 0 || count > MAX_COPIES {
            return Err(format!(
                "count must be between 1 and {MAX_COPIES}, got {count}"
            ));
        }
        let finite = [
            step.rotate,
            step.scale,
            step.offset.x,
            step.offset.y,
            f64::from(step.opacity.0),
            f64::from(step.opacity.1),
        ]
        .iter()
        .all(|value| value.is_finite())
            && step.about.is_none_or(|about| about.is_finite());
        if !finite || step.scale <= 0.0 {
            return Err("repeat values must be finite and scale positive".to_string());
        }
        if !(0.0..=1.0).contains(&step.opacity.0) || !(0.0..=1.0).contains(&step.opacity.1) {
            return Err("opacities must be between 0 and 1".to_string());
        }
        self.check_owner(source)?;
        let mut copies = Vec::with_capacity(count);
        for index in 0..count {
            let mut copy = if index == 0 {
                source.clone()
            } else {
                self.clone_drawable(source)
            };
            let share = if count > 1 {
                index as f32 / (count - 1) as f32
            } else {
                0.0
            };
            let opacity = step.opacity.0 + (step.opacity.1 - step.opacity.0) * share;
            if index > 0 {
                let steps = index as f64;
                if step.offset != DVec2::ZERO {
                    copy = copy.shift_by(step.offset.x * steps, step.offset.y * steps);
                }
                if let Some(about) = step.about {
                    copy = copy.with_pivot(about.x, about.y);
                }
                if step.rotate != 0.0 {
                    copy = copy.rotate_by(step.rotate * steps);
                }
                if step.scale != 1.0 {
                    copy = copy.scale_by(step.scale.powf(steps));
                }
            }
            if opacity != 1.0 {
                copy = scale_opacity(copy, opacity);
            }
            copies.push(copy);
        }
        let refs: Vec<&DrawableHandle> = copies.iter().collect();
        Ok(self.group(&refs))
    }

    /// Copies of `source` at every placement of `distribution`, `source`
    /// itself moving to the first one. The copies are the members of the
    /// returned group, in order.
    pub fn duplicate(
        &mut self,
        source: &DrawableHandle,
        distribution: &Distribution,
    ) -> Result<DrawableHandle, String> {
        distribution.validate()?;
        self.check_owner(source)?;
        let placements = distribution.placements();
        let mut copies = Vec::with_capacity(placements.len());
        for (index, (position, rotation)) in placements.into_iter().enumerate() {
            let mut copy = if index == 0 {
                source.clone()
            } else {
                self.clone_drawable(source)
            };
            copy = copy.move_to_default(position.x, position.y);
            if rotation != 0.0 {
                copy = copy.rotate_by(rotation);
            }
            copies.push(copy);
        }
        let refs: Vec<&DrawableHandle> = copies.iter().collect();
        Ok(self.group(&refs))
    }

    fn check_owner(&self, source: &DrawableHandle) -> Result<(), String> {
        if self.owns_drawable(source) {
            Ok(())
        } else {
            Err("the drawable belongs to another scene".to_string())
        }
    }

    /// A new drawable declared like `source`, with copies of its members.
    fn clone_drawable(&mut self, source: &DrawableHandle) -> DrawableHandle {
        let spec = source.spec.lock().expect("object spec poisoned").clone();
        self.clone_spec(&spec)
    }

    fn clone_spec(&mut self, spec: &ObjectSpec) -> DrawableHandle {
        let mut members = Vec::new();
        let kind = match &spec.kind {
            SpawnKind::Group(ids) | SpawnKind::GroupNoCenter(ids) => {
                for id in ids {
                    let member = self
                        .state
                        .lock()
                        .expect("canvas state poisoned")
                        .object_specs
                        .get(id)
                        .cloned();
                    if let Some(member) = member {
                        let member_spec = member.lock().expect("object spec poisoned").clone();
                        members.push(self.clone_spec(&member_spec));
                    }
                }
                let ids: Vec<ObjectId> = members.iter().map(|member| member.id).collect();
                if matches!(spec.kind, SpawnKind::GroupNoCenter(_)) {
                    SpawnKind::GroupNoCenter(ids)
                } else {
                    SpawnKind::Group(ids)
                }
            }
            other => other.clone(),
        };
        let handle = self.spawn(kind.clone());
        {
            let mut target = handle.spec.lock().expect("object spec poisoned");
            let id = target.id;
            *target = spec.clone();
            target.id = id;
            target.kind = kind;
            // The copy is placed explicitly, not by the source's layout.
            target.layout_owner = None;
        }
        if members.is_empty() {
            handle
        } else {
            handle.with_style_targets(
                members
                    .iter()
                    .flat_map(|member| member.inherited_style_targets())
                    .collect(),
            )
        }
    }
}

fn scale_opacity(handle: DrawableHandle, factor: f32) -> DrawableHandle {
    let current = handle.spec.lock().expect("object spec poisoned").opacity;
    handle.opacity(f64::from(current * factor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distributions_place_their_copies() {
        let grid = Distribution::Grid {
            columns: 3,
            rows: 2,
            spacing: DVec2::new(1.0, 2.0),
            center: DVec2::ZERO,
        }
        .placements();
        assert_eq!(grid.len(), 6);
        assert_eq!(grid[0].0, DVec2::new(-1.0, 1.0));
        assert_eq!(grid[5].0, DVec2::new(1.0, -1.0));

        let circle = Distribution::Circle {
            count: 4,
            radius: 2.0,
            center: DVec2::new(1.0, 0.0),
            start: 0.0,
            orient: true,
        }
        .placements();
        assert!(circle[1].0.distance(DVec2::new(1.0, 2.0)) < 1e-12);
        assert!((circle[1].1 - std::f64::consts::FRAC_PI_2).abs() < 1e-12);

        let path = vec![DVec2::ZERO, DVec2::new(2.0, 0.0), DVec2::new(2.0, 2.0)];
        let along = Distribution::Along {
            points: path,
            count: 3,
            orient: true,
        }
        .placements();
        assert_eq!(along[1].0, DVec2::new(2.0, 0.0));
        assert_eq!(along[2].0, DVec2::new(2.0, 2.0));
        assert!((along[2].1 - std::f64::consts::FRAC_PI_2).abs() < 1e-12);

        let random = |seed| Distribution::Random {
            count: 50,
            min: DVec2::new(-1.0, -2.0),
            max: DVec2::new(1.0, 2.0),
            seed,
        };
        let a = random(3).placements();
        assert_eq!(a, random(3).placements());
        assert_ne!(a, random(4).placements());
        assert!(
            a.iter()
                .all(|(point, _)| point.x.abs() <= 1.0 && point.y.abs() <= 2.0)
        );

        let spiral = Distribution::Phyllotaxis {
            count: 10,
            spacing: 0.5,
            center: DVec2::ZERO,
        }
        .placements();
        assert_eq!(spiral[0].0, DVec2::ZERO);
        assert!((spiral[4].0.length() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn invalid_distributions_are_rejected() {
        let grid = |columns, rows| Distribution::Grid {
            columns,
            rows,
            spacing: DVec2::ONE,
            center: DVec2::ZERO,
        };
        assert!(grid(0, 3).validate().is_err());
        assert!(grid(200, 200).validate().is_err());
        assert!(grid(3, 3).validate().is_ok());
        assert!(
            Distribution::Along {
                points: vec![DVec2::ZERO],
                count: 3,
                orient: false,
            }
            .validate()
            .is_err()
        );
        assert!(
            Distribution::Random {
                count: 3,
                min: DVec2::new(1.0, 0.0),
                max: DVec2::new(0.0, 1.0),
                seed: 0,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn repeat_and_duplicate_group_copies_of_the_source() {
        let mut canvas = SceneModel::new(640, 360);
        let petal = canvas.ellipse(0.2, 0.8).move_to(0.0, 1.0);
        let petals = canvas
            .repeat(
                &petal,
                6,
                RepeatStep {
                    rotate: std::f64::consts::TAU / 6.0,
                    opacity: (1.0, 0.5),
                    about: Some(DVec2::ZERO),
                    ..RepeatStep::default()
                },
            )
            .unwrap();
        let members = match &petals.spec.lock().unwrap().kind {
            SpawnKind::Group(ids) => ids.clone(),
            other => panic!("expected a group, got {other:?}"),
        };
        assert_eq!(members.len(), 6);
        assert_eq!(members[0], petal.id);
        let opacity_of = |canvas: &SceneModel, id: &ObjectId| {
            canvas.state.lock().unwrap().object_specs[id]
                .lock()
                .unwrap()
                .opacity
        };
        assert_eq!(opacity_of(&canvas, &members[0]), 1.0);
        assert_eq!(opacity_of(&canvas, &members[5]), 0.5);
        assert!(canvas.repeat(&petal, 0, RepeatStep::default()).is_err());

        let dot = canvas.circle(0.1);
        let group = canvas
            .duplicate(
                &dot,
                &Distribution::Grid {
                    columns: 4,
                    rows: 3,
                    spacing: DVec2::splat(0.5),
                    center: DVec2::ZERO,
                },
            )
            .unwrap();
        let count = match &group.spec.lock().unwrap().kind {
            SpawnKind::Group(ids) => ids.len(),
            _ => 0,
        };
        assert_eq!(count, 12);

        let mut other = SceneModel::new(640, 360);
        assert!(other.repeat(&dot, 3, RepeatStep::default()).is_err());
    }
}
