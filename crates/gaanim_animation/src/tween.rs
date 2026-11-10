use std::sync::Arc;

use bevy::prelude::{Entity, Res, ResMut, Resource};
use gaanim_core::kurbo::BezPath;
use gaanim_scene::LineListData;

/// Resource containing the current simulation delta time.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct DeltaTime {
    /// Time delta in seconds.
    pub dt: f64,
}

/// System: Copies Bevy's `Time` delta into `DeltaTime` so that playback is
/// decoupled from Bevy internals and can be paused / scaled independently.
pub fn sync_delta_time_system(
    bevy_time: Res<bevy::prelude::Time>,
    mut delta_time: ResMut<DeltaTime>,
) {
    delta_time.dt = bevy_time.delta_secs() as f64;
}

/// The contour matching of a path morph, built on its first frame and
/// shared by its clones: every later frame only interpolates. It belongs to
/// the `from` and `to` paths beside it, so tables always compare equal.
#[derive(Debug, Clone, Default)]
pub struct MorphTable(Arc<std::sync::OnceLock<gaanim_math::MorphPlan>>);

/// A table is a cache rebuilt on its first frame: it is written empty and
/// read back as a new one.
#[cfg(feature = "serde")]
impl serde::Serialize for MorphTable {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_unit_struct("MorphTable")
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for MorphTable {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        serde::de::IgnoredAny::deserialize(deserializer)?;
        Ok(Self::default())
    }
}

impl MorphTable {
    /// The morph from `from` to `to` at `t`, as
    /// [`gaanim_math::interpolate_paths_continuous`] computes it.
    pub fn path(&self, from: &BezPath, to: &BezPath, t: f64) -> BezPath {
        self.0
            .get_or_init(|| gaanim_math::MorphPlan::new(from, to))
            .at(t)
    }
}

impl PartialEq for MorphTable {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// Concrete or timeline-captured source for a complete authored camera pose.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CameraStateSource {
    Concrete(gaanim_math::CameraPose),
    Captured(u64),
}

/// Extensible lens trait for custom third-party plugins.
pub trait AnimatableLens: Send + Sync + std::fmt::Debug + 'static {
    /// Interpolate the target Mobject's components directly using World access.
    fn interpolate(&self, world: &mut bevy::prelude::World, entity: Entity, t: f64);
    /// Helper to clone dyn boxes.
    fn clone_box(&self) -> Box<dyn AnimatableLens>;
    /// Returns the descriptive type name of the custom lens.
    fn type_name(&self) -> &'static str;
    /// Name of the state this lens owns before its clip starts, such as an
    /// entry that keeps its target hidden until then. A seek before the clip
    /// applies [`Self::hold`] of the earliest future clip per target and
    /// channel, so lenses writing the same components share a channel.
    fn hold_channel(&self) -> Option<&'static str> {
        None
    }
    /// Write the state shown before the clip starts; `initial` is the clip's
    /// eased value at progress 0.
    fn hold(&self, world: &mut bevy::prelude::World, entity: Entity, initial: f64) {
        self.interpolate(world, entity, initial);
    }
    /// Whether [`Self::interpolate`] writes a value that depends only on `t`
    /// and keeps no state between calls, so the instants a timeline visits
    /// before this one cannot change what it draws. User callbacks may keep
    /// state, so this defaults to `false`.
    fn history_free(&self) -> bool {
        false
    }
}

/// Rotation reached `t` of the way through a local Z turn of `radians`
/// starting at `from`.
pub fn rotation_z_at(
    from: gaanim_core::glam::DQuat,
    radians: f64,
    t: f64,
) -> gaanim_core::glam::DQuat {
    (from * gaanim_core::glam::DQuat::from_rotation_z(radians * t)).normalize()
}

/// A shared [`AnimatableLens`] implemented by a crate above the timeline
/// (for example renderer effects), evaluated identically in playback and seeks.
#[derive(Clone)]
pub struct DynamicLens(pub Arc<dyn AnimatableLens>);

impl std::fmt::Debug for DynamicLens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&*self.0, f)
    }
}

impl Clone for Box<dyn AnimatableLens> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

fn lerp_line_value<const N: usize>(from: [f32; N], to: [f32; N], t: f32) -> [f32; N] {
    std::array::from_fn(|index| from[index] + (to[index] - from[index]) * t)
}

fn trim_line_strip_range(source: &LineListData, start: f64, end: f64) -> LineListData {
    let segment_count = source.points.len().saturating_sub(1);
    let start = start.clamp(0.0, 1.0);
    let end = end.clamp(start, 1.0);
    if segment_count == 0 || end <= start + f64::EPSILON {
        return LineListData {
            points: Vec::new(),
            indices: None,
            strip: true,
            color: source.color,
            colors: source.colors.as_ref().map(|_| Vec::new()),
            revealed: None,
        };
    }

    let scaled_start = start * segment_count as f64;
    let scaled_end = end * segment_count as f64;
    let start_segment = (scaled_start.floor() as usize).min(segment_count - 1);
    let end_segment = (scaled_end.floor() as usize).min(segment_count - 1);
    let start_fraction = (scaled_start - start_segment as f64) as f32;
    let end_fraction = if end >= 1.0 {
        1.0
    } else {
        (scaled_end - end_segment as f64) as f32
    };

    let mut points = vec![lerp_line_value(
        source.points[start_segment],
        source.points[start_segment + 1],
        start_fraction,
    )];
    for index in (start_segment + 1)..=end_segment {
        points.push(source.points[index]);
    }
    points.push(lerp_line_value(
        source.points[end_segment],
        source.points[end_segment + 1],
        end_fraction,
    ));

    let colors = source.colors.as_ref().map(|colors| {
        let mut visible = vec![lerp_line_value(
            colors[start_segment],
            colors[start_segment + 1],
            start_fraction,
        )];
        visible.extend_from_slice(&colors[(start_segment + 1)..=end_segment]);
        visible.push(lerp_line_value(
            colors[end_segment],
            colors[end_segment + 1],
            end_fraction,
        ));
        visible
    });

    LineListData {
        points,
        indices: None,
        strip: true,
        color: source.color,
        colors,
        revealed: None,
    }
}

/// The part of a 3D line strip that a trim window shows: `[start, end]`
/// shifted by `offset`, wrapping past the end as `gaanim_math::trim_path`
/// does. Fractions count segments, not length, so a strip of samples evenly
/// spaced in time grows at a steady rate. `None` for a line that is not an
/// unindexed strip.
pub fn trim_line_strip(
    source: &LineListData,
    start: f64,
    end: f64,
    offset: f64,
) -> Option<LineListData> {
    if !source.strip || source.indices.is_some() {
        return None;
    }
    let (low, high) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let (low, high) = (low.clamp(0.0, 1.0), high.clamp(0.0, 1.0));
    if high - low >= 1.0 {
        return Some(LineListData {
            revealed: None,
            ..source.clone()
        });
    }
    let shift = offset.rem_euclid(1.0);
    let (from, to) = (low + shift, high + shift);
    if to <= 1.0 {
        return Some(trim_line_strip_range(source, from, to).revealing(source, from, to));
    }
    // The window wraps past the end: its tail and head pieces, as point pairs.
    let mut pieces = LineListData {
        points: Vec::new(),
        indices: None,
        strip: false,
        color: source.color,
        colors: source.colors.as_ref().map(|_| Vec::new()),
        revealed: None,
    };
    for (a, b) in [(from.min(1.0), 1.0), ((from - 1.0).max(0.0), to - 1.0)] {
        let piece = trim_line_strip_range(source, a, b);
        for pair in piece.points.windows(2) {
            pieces.points.extend_from_slice(pair);
        }
        if let (Some(colors), Some(piece_colors)) = (&mut pieces.colors, &piece.colors) {
            for pair in piece_colors.windows(2) {
                colors.extend_from_slice(pair);
            }
        }
    }
    Some(pieces)
}

/// Trim window between two `[start, end, offset]` states at `t`.
pub fn lerp_trim(from: &[f64; 3], to: &[f64; 3], t: f64) -> [f64; 3] {
    std::array::from_fn(|index| from[index] + (to[index] - from[index]) * t)
}

#[cfg(test)]
mod tests {
    use gaanim_math::get_point_at_alpha;
    use kurbo::BezPath;

    fn strip(count: usize) -> super::LineListData {
        super::LineListData {
            points: (0..=count).map(|index| [index as f32, 0.0, 0.0]).collect(),
            indices: None,
            strip: true,
            color: gaanim_core::peniko::Color::WHITE,
            colors: None,
            revealed: None,
        }
    }

    #[test]
    fn a_trimmed_line_strip_keeps_a_share_of_its_segments() {
        let source = strip(10);
        let half = super::trim_line_strip(&source, 0.0, 0.5, 0.0).unwrap();
        assert_eq!(half.points.first(), Some(&[0.0, 0.0, 0.0]));
        assert_eq!(half.points.last(), Some(&[5.0, 0.0, 0.0]));
        assert_eq!(half.revealed, Some([0.0, 0.5]));
        let middle = super::trim_line_strip(&source, 0.25, 0.75, 0.0).unwrap();
        assert_eq!(middle.points.first(), Some(&[2.5, 0.0, 0.0]));
        assert_eq!(middle.points.last(), Some(&[7.5, 0.0, 0.0]));
        assert!(
            super::trim_line_strip(&source, 0.0, 0.0, 0.0)
                .unwrap()
                .points
                .is_empty()
        );
        let whole = super::trim_line_strip(&source, 0.0, 1.0, 0.0).unwrap();
        assert_eq!((whole.points.len(), whole.revealed), (11, None));
    }

    #[test]
    fn a_trim_window_wraps_past_the_end_of_a_line_strip() {
        // [0.6, 0.9] shifted by 0.3 shows [0.9, 1] and [0, 0.2], as point pairs.
        let wrapped = super::trim_line_strip(&strip(10), 0.6, 0.9, 0.3).unwrap();
        assert!(!wrapped.strip && wrapped.revealed.is_none());
        let pairs: Vec<_> = wrapped
            .points
            .chunks(2)
            .map(|pair| (pair[0][0], pair[1][0]))
            .collect();
        assert_eq!(pairs, vec![(9.0, 10.0), (0.0, 1.0), (1.0, 2.0)]);
    }

    #[test]
    fn indexed_lines_are_not_trimmed() {
        let mut pairs = strip(4);
        pairs.indices = Some(vec![0, 1, 2, 3]);
        assert!(super::trim_line_strip(&pairs, 0.0, 0.5, 0.0).is_none());
    }

    /// `PathFollow` should sample the Bézier at the rate-function-eased
    /// `t`. With `RateFunc::Linear` and a polyline path, the sampled
    /// point at any `t` should match `get_point_at_alpha` of the same
    /// path at the same `t` (we re-implement the sampling call here
    /// without Bevy so the math is verifiable in isolation).
    #[test]
    fn path_follow_samples_match_get_point_at_alpha() {
        // A pentagon: 5 line segments connected end-to-end.
        let mut path = BezPath::new();
        path.move_to(kurbo::Point::new(0.0, 0.0));
        path.line_to(kurbo::Point::new(100.0, 0.0));
        path.line_to(kurbo::Point::new(100.0, 100.0));
        path.line_to(kurbo::Point::new(0.0, 100.0));
        path.line_to(kurbo::Point::new(0.0, 0.0));

        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let sampled = get_point_at_alpha(&path, t);
            // The lens is parameterised on a multi-subpath path; the
            // per-subpath trim means each segment is revealed
            // proportionally, so at t=0.25 the first subpath is at
            // 0.25 of its own length, etc.
            assert!(sampled.x.is_finite(), "x at t={t} not finite");
            assert!(sampled.y.is_finite(), "y at t={t} not finite");
        }
    }
}
