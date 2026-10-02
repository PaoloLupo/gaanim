//! Offset paths: a shape grown or shrunk by a distance, like After Effects'
//! *Offset Paths*.

use gaanim_core::kurbo::{self, BezPath, PathEl};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::float::simplify::SimplifyShape;
use i_overlay::mesh::outline::offset::OutlineOffset;
use i_overlay::mesh::style::{LineJoin, OutlineStyle};

use crate::boolean::{BooleanOp, apply, bezpath_to_shape_with_tolerance, shapes_to_bezpath};

/// How an offset outline turns at a corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OffsetJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

impl OffsetJoin {
    fn overlay(self) -> LineJoin<f64> {
        match self {
            // Corners sharper than this angle are cut, like a miter limit.
            Self::Miter => LineJoin::Miter(0.25 * std::f64::consts::PI),
            Self::Round => LineJoin::Round(0.05),
            Self::Bevel => LineJoin::Bevel,
        }
    }

    fn kurbo(self) -> kurbo::Join {
        match self {
            Self::Miter => kurbo::Join::Miter,
            Self::Round => kurbo::Join::Round,
            Self::Bevel => kurbo::Join::Bevel,
        }
    }
}

/// Tolerance of the stroked outlines, in scene units.
const OFFSET_TOLERANCE: f64 = 1e-3;

fn split(path: &BezPath) -> (BezPath, BezPath) {
    let mut closed = BezPath::new();
    let mut open = BezPath::new();
    let mut current = Vec::new();
    let mut flush = |current: &mut Vec<PathEl>, is_closed: bool| {
        if current.len() > 1 {
            let target = if is_closed { &mut closed } else { &mut open };
            for element in current.drain(..) {
                target.push(element);
            }
        }
        current.clear();
    };
    for element in path.elements() {
        match element {
            PathEl::MoveTo(_) => {
                flush(&mut current, false);
                current.push(*element);
            }
            PathEl::ClosePath => {
                current.push(*element);
                flush(&mut current, true);
            }
            _ => current.push(*element),
        }
    }
    flush(&mut current, false);
    (closed, open)
}

fn outline(path: &BezPath, distance: f64, join: OffsetJoin) -> BezPath {
    kurbo::stroke(
        path.iter(),
        &kurbo::Stroke::new(2.0 * distance)
            .with_join(join.kurbo())
            .with_caps(kurbo::Cap::Butt)
            .with_miter_limit(4.0),
        &kurbo::StrokeOpts::default(),
        OFFSET_TOLERANCE,
    )
}

fn merged(result: crate::boolean::BooleanResult) -> BezPath {
    let mut path = BezPath::new();
    for part in result.paths {
        path.extend(part);
    }
    path
}

/// `path` moved outward by `amount` (inward when negative). Closed
/// sub-paths grow or shrink; open ones become the outline of a band
/// `|amount|` to each side.
pub fn offset_path(path: &BezPath, amount: f64, join: OffsetJoin) -> BezPath {
    if amount == 0.0 || !amount.is_finite() {
        return path.clone();
    }
    let (closed, open) = split(path);
    let distance = amount.abs();
    let mut result = BezPath::new();
    if !closed.elements().is_empty() {
        // Normalized into outer contours and holes first, so each moves the
        // right way; shrinking past a shape's inner radius leaves nothing.
        let shapes = bezpath_to_shape_with_tolerance(&closed, OFFSET_TOLERANCE)
            .simplify_shape(FillRule::NonZero);
        let style = OutlineStyle::new(amount).line_join(join.overlay());
        for shape in shapes.outline(&style) {
            result.extend(shapes_to_bezpath(&shape));
        }
    }
    if !open.elements().is_empty() {
        result.extend(merged(apply(
            &outline(&open, distance, join),
            &BezPath::new(),
            BooleanOp::Union,
        )));
    }
    result
}

/// `copies` offsets of `path`, `amount` apart: the first `amount` from it.
pub fn offset_copies(path: &BezPath, amount: f64, join: OffsetJoin, copies: u32) -> BezPath {
    let mut result = BezPath::new();
    for copy in 1..=copies.max(1) {
        result.extend(offset_path(path, amount * copy as f64, join));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;

    fn square() -> BezPath {
        kurbo::Rect::new(-1.0, -1.0, 1.0, 1.0).to_path(1e-3)
    }

    #[test]
    fn offsets_grow_and_shrink_closed_shapes() {
        let grown = offset_path(&square(), 0.5, OffsetJoin::Miter);
        let bounds = grown.bounding_box();
        assert!((bounds.width() - 3.0).abs() < 1e-3, "{bounds:?}");
        assert!((grown.area().abs() - 9.0).abs() < 1e-2);
        let round = offset_path(&square(), 0.5, OffsetJoin::Round).area().abs();
        assert!(round < 9.0 && round > 8.5, "{round}");
        let shrunk = offset_path(&square(), -0.5, OffsetJoin::Miter);
        assert!((shrunk.area().abs() - 1.0).abs() < 1e-2);
        assert!(
            offset_path(&square(), -2.0, OffsetJoin::Miter)
                .elements()
                .is_empty()
        );
        // Shrinking a circle past its radius leaves nothing, not a speck.
        let circle = kurbo::Circle::new((0.0, 0.0), 0.6).to_path(1e-3);
        assert!(
            offset_path(&circle, -0.7, OffsetJoin::Round)
                .elements()
                .is_empty()
        );
        // A ring's hole grows when the ring shrinks.
        let mut ring = kurbo::Circle::new((0.0, 0.0), 2.0).to_path(1e-3);
        ring.extend(
            kurbo::Circle::new((0.0, 0.0), 1.0)
                .to_path(1e-3)
                .reverse_subpaths(),
        );
        let thinner = offset_path(&ring, -0.25, OffsetJoin::Round).area().abs();
        let expected = std::f64::consts::PI * (1.75 * 1.75 - 1.25 * 1.25);
        assert!((thinner - expected).abs() < 0.02, "{thinner} vs {expected}");
    }

    #[test]
    fn open_paths_become_bands_and_copies_nest() {
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((2.0, 0.0));
        let band = offset_path(&line, 0.25, OffsetJoin::Bevel);
        assert!((band.area().abs() - 1.0).abs() < 1e-2);
        let rings = offset_copies(&square(), 0.25, OffsetJoin::Miter, 3);
        let outer = rings.bounding_box();
        assert!((outer.width() - 3.5).abs() < 1e-3, "{outer:?}");
        let moves = rings
            .elements()
            .iter()
            .filter(|element| matches!(element, PathEl::MoveTo(_)))
            .count();
        assert_eq!(moves, 3);
    }
}
