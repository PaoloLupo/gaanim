//! Seeded scatter: place boxes inside a region without overlapping each
//! other or a set of obstacles.
//!
//! [`scatter`] draws candidate centers from a [`SeededRng`] and keeps, for
//! each box, the valid candidate farthest from everything already placed
//! (Mitchell's best candidate), so the boxes spread over the region instead
//! of clustering. Larger boxes are placed first. When random candidates find
//! no room, a scan of the region looks for any position left, so a dense but
//! feasible layout still succeeds. The same inputs and seed always give the
//! same layout.

use gaanim_math::{Bounds3D, SeededRng};
use glam::DVec2;

/// Valid candidates compared for each box.
const CANDIDATES: usize = 40;
/// Random draws per box before scanning the region.
const DRAWS: usize = 3000;
/// Positions per axis of the fallback scan.
const SCAN: usize = 160;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ScatterError {
    #[error(
        "item {index} ({width:.3} × {height:.3}) is larger than the region ({region_width:.3} × {region_height:.3})"
    )]
    TooLarge {
        index: usize,
        width: f64,
        height: f64,
        region_width: f64,
        region_height: f64,
    },
    #[error(
        "item {index} does not fit: no room is left without overlapping the other items or the avoided shapes; use a smaller gap, a larger region or fewer items"
    )]
    NoRoom { index: usize },
    #[error("{0} must be finite and non-negative")]
    Invalid(&'static str),
}

/// Centers for boxes of `sizes` inside `region`, in the order of `sizes`.
///
/// Boxes keep at least `gap` between each other and from every rectangle of
/// `avoid`, and lie entirely inside `region`.
pub fn scatter(
    sizes: &[DVec2],
    region: Bounds3D,
    avoid: &[Bounds3D],
    gap: f64,
    seed: u64,
) -> Result<Vec<DVec2>, ScatterError> {
    if !gap.is_finite() || gap < 0.0 {
        return Err(ScatterError::Invalid("gap"));
    }
    let (min, max) = (
        DVec2::new(region.min.x, region.min.y),
        DVec2::new(region.max.x, region.max.y),
    );
    if !(min.is_finite() && max.is_finite()) {
        return Err(ScatterError::Invalid("region"));
    }
    for size in sizes {
        if !size.is_finite() || size.x < 0.0 || size.y < 0.0 {
            return Err(ScatterError::Invalid("item size"));
        }
    }
    let obstacles: Vec<Rect> = avoid
        .iter()
        .filter(|bounds| bounds.min.x.is_finite() && bounds.max.x.is_finite())
        .map(|bounds| Rect {
            center: DVec2::new(bounds.center().x, bounds.center().y),
            half: DVec2::new(bounds.width(), bounds.height()) / 2.0,
        })
        .collect();

    // Larger boxes first: they need the most room. Ties keep list order.
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|&a, &b| {
        let area = |index: usize| sizes[index].x * sizes[index].y;
        area(b).total_cmp(&area(a))
    });

    let mut rng = SeededRng::new(seed);
    let mut placed: Vec<Rect> = Vec::with_capacity(sizes.len());
    let mut centers = vec![DVec2::ZERO; sizes.len()];
    for index in order {
        let half = sizes[index] / 2.0;
        let (low, high) = (min + half, max - half);
        if low.x > high.x + 1e-12 || low.y > high.y + 1e-12 {
            return Err(ScatterError::TooLarge {
                index,
                width: sizes[index].x,
                height: sizes[index].y,
                region_width: max.x - min.x,
                region_height: max.y - min.y,
            });
        }
        let high = high.max(low);
        let clearance = |center: DVec2| -> Option<f64> {
            let rect = Rect { center, half };
            let mut nearest = f64::INFINITY;
            for other in placed.iter().chain(obstacles.iter()) {
                let distance = rect.distance(other)?;
                if distance < gap {
                    return None;
                }
                nearest = nearest.min(distance);
            }
            Some(nearest)
        };

        let mut best: Option<(f64, DVec2)> = None;
        let mut valid = 0;
        for _ in 0..DRAWS {
            let center = DVec2::new(rng.uniform(low.x, high.x), rng.uniform(low.y, high.y));
            if let Some(score) = clearance(center) {
                if best.is_none_or(|(best_score, _)| score > best_score) {
                    best = Some((score, center));
                }
                valid += 1;
                if valid == CANDIDATES {
                    break;
                }
            }
        }
        if best.is_none() {
            // Few positions remain: scan the region for the roomiest one.
            for row in 0..=SCAN {
                for column in 0..=SCAN {
                    let t = DVec2::new(column as f64, row as f64) / SCAN as f64;
                    let center = low + (high - low) * t;
                    if let Some(score) = clearance(center)
                        && best.is_none_or(|(best_score, _)| score > best_score)
                    {
                        best = Some((score, center));
                    }
                }
            }
        }
        let Some((_, center)) = best else {
            return Err(ScatterError::NoRoom { index });
        };
        placed.push(Rect { center, half });
        centers[index] = center;
    }
    Ok(centers)
}

#[derive(Debug, Clone, Copy)]
struct Rect {
    center: DVec2,
    half: DVec2,
}

impl Rect {
    /// Distance between the two boxes, or `None` when they overlap.
    fn distance(&self, other: &Rect) -> Option<f64> {
        let apart = (self.center - other.center).abs() - (self.half + other.half);
        if apart.x < 0.0 && apart.y < 0.0 {
            return None;
        }
        Some(apart.max(DVec2::ZERO).length())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region() -> Bounds3D {
        Bounds3D::new_2d(-8.0, -4.5, 8.0, 4.5)
    }

    fn rects(sizes: &[DVec2], centers: &[DVec2]) -> Vec<Rect> {
        sizes
            .iter()
            .zip(centers)
            .map(|(size, center)| Rect {
                center: *center,
                half: *size / 2.0,
            })
            .collect()
    }

    #[test]
    fn thirty_labels_keep_their_gap_and_avoid_the_face() {
        let sizes: Vec<DVec2> = (0..30)
            .map(|index| DVec2::new(1.0 + (index % 4) as f64 * 0.4, 0.5))
            .collect();
        let face = Bounds3D::new_2d(-1.5, -2.0, 1.5, 2.0);
        let centers = scatter(&sizes, region(), &[face], 0.2, 1).unwrap();
        let boxes = rects(&sizes, &centers);
        let face = Rect {
            center: DVec2::ZERO,
            half: DVec2::new(1.5, 2.0),
        };
        for (index, rect) in boxes.iter().enumerate() {
            assert!(rect.center.x - rect.half.x >= -8.0 - 1e-9);
            assert!(rect.center.x + rect.half.x <= 8.0 + 1e-9);
            assert!(rect.center.y - rect.half.y >= -4.5 - 1e-9);
            assert!(rect.center.y + rect.half.y <= 4.5 + 1e-9);
            assert!(rect.distance(&face).is_some_and(|d| d >= 0.2 - 1e-9));
            for other in &boxes[index + 1..] {
                assert!(rect.distance(other).is_some_and(|d| d >= 0.2 - 1e-9));
            }
        }
    }

    #[test]
    fn the_seed_decides_the_layout() {
        let sizes = vec![DVec2::new(1.0, 0.5); 8];
        let a = scatter(&sizes, region(), &[], 0.1, 7).unwrap();
        assert_eq!(a, scatter(&sizes, region(), &[], 0.1, 7).unwrap());
        assert_ne!(a, scatter(&sizes, region(), &[], 0.1, 8).unwrap());
    }

    #[test]
    fn a_crowded_region_still_fits_every_box() {
        // Twelve boxes cover 40% of a 6×3 region.
        let sizes = vec![DVec2::new(1.2, 0.5); 12];
        let region = Bounds3D::new_2d(0.0, 0.0, 6.0, 3.0);
        let centers = scatter(&sizes, region, &[], 0.1, 3).unwrap();
        let boxes = rects(&sizes, &centers);
        for (index, rect) in boxes.iter().enumerate() {
            for other in &boxes[index + 1..] {
                assert!(rect.distance(other).is_some_and(|d| d >= 0.1 - 1e-9));
            }
        }
    }

    #[test]
    fn impossible_layouts_name_the_item() {
        let big = scatter(&[DVec2::new(20.0, 1.0)], region(), &[], 0.0, 0);
        assert!(matches!(big, Err(ScatterError::TooLarge { index: 0, .. })));
        let crowded = scatter(&vec![DVec2::splat(3.0); 40], region(), &[], 0.2, 0);
        assert!(matches!(crowded, Err(ScatterError::NoRoom { .. })));
        assert!(matches!(
            scatter(&[DVec2::ONE], region(), &[], -1.0, 0),
            Err(ScatterError::Invalid("gap"))
        ));
        assert_eq!(
            scatter(&[], region(), &[], 0.2, 0).unwrap(),
            Vec::<DVec2>::new()
        );
    }
}
