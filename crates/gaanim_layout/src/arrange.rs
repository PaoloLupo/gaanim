use gaanim_core::glam::DVec3;
use gaanim_math::{Bounds3D, SpatialTransform};
use crate::{Anchor, Direction};
use crate::positioning::{compute_next_to, transform_bounds};

/// Arranges items linearly along a direction with uniform spacing.
/// Returns the list of positions (translations) for each item.
pub fn arrange(
    items: &[(Bounds3D, SpatialTransform)],
    direction: Direction,
    spacing: f64,
    aligned_edge: Anchor,
    center_result: bool,
) -> Vec<DVec3> {
    if items.is_empty() {
        return Vec::new();
    }

    let mut updated_transforms = Vec::with_capacity(items.len());
    // Start the first item at its current transform
    updated_transforms.push(items[0].1);

    for i in 1..items.len() {
        let shift = compute_next_to(
            items[i].0,
            &items[i].1,
            items[i - 1].0,
            &updated_transforms[i - 1],
            direction,
            spacing,
            aligned_edge,
        );
        updated_transforms.push(items[i].1.shift_3d(shift));
    }

    let mut result: Vec<DVec3> = updated_transforms.iter().map(|t| t.translation).collect();

    if center_result {
        let mut min = DVec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut max = DVec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);

        for i in 0..items.len() {
            let item_bounds = transform_bounds(items[i].0, &updated_transforms[i]);
            min = min.min(item_bounds.min);
            max = max.max(item_bounds.max);
        }

        if min.x < max.x && min.y < max.y {
            let group_center = (min + max) * 0.5;
            for translation in &mut result {
                translation.x -= group_center.x;
                translation.y -= group_center.y;
            }
        }
    }

    result
}

/// Arranges items in a grid with configurable rows/cols.
pub fn arrange_in_grid(
    items: &[(Bounds3D, SpatialTransform)],
    rows: Option<usize>,
    cols: Option<usize>,
    h_spacing: f64,
    v_spacing: f64,
    cell_alignment: Anchor,
    center_result: bool,
) -> Vec<DVec3> {
    let n = items.len();
    if n == 0 {
        return Vec::new();
    }

    // Determine grid size
    let (num_rows, num_cols) = match (rows, cols) {
        (Some(r), Some(c)) => (r.max(1), c.max(1)),
        (Some(r), None) => {
            let r = r.max(1);
            (r, (n + r - 1) / r)
        }
        (None, Some(c)) => {
            let c = c.max(1);
            ((n + c - 1) / c, c)
        }
        (None, None) => {
            let c = (n as f64).sqrt().ceil() as usize;
            let c = c.max(1);
            ((n + c - 1) / c, c)
        }
    };

    let mut col_widths = vec![0.0f64; num_cols];
    let mut row_heights = vec![0.0f64; num_rows];
    let mut item_sizes = Vec::with_capacity(n);

    // Compute column widths and row heights
    for i in 0..n {
        let mut temp_transform = items[i].1;
        temp_transform.translation = DVec3::ZERO;
        let bounds_no_trans = transform_bounds(items[i].0, &temp_transform);
        let size = bounds_no_trans.size();

        let r = i / num_cols;
        let c = i % num_cols;

        if c < num_cols {
            col_widths[c] = col_widths[c].max(size.x);
        }
        if r < num_rows {
            row_heights[r] = row_heights[r].max(size.y);
        }

        item_sizes.push(bounds_no_trans);
    }

    // Column start positions (left edges)
    let mut col_starts = vec![0.0f64; num_cols];
    for c in 1..num_cols {
        col_starts[c] = col_starts[c - 1] + col_widths[c - 1] + h_spacing;
    }

    // Row start positions (top edges, Y decreases down)
    let mut row_starts = vec![0.0f64; num_rows];
    for r in 1..num_rows {
        row_starts[r] = row_starts[r - 1] - row_heights[r - 1] - v_spacing;
    }

    let mut result = vec![DVec3::ZERO; n];
    let mut group_min = DVec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut group_max = DVec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);

    for i in 0..n {
        let r = i / num_cols;
        let c = i % num_cols;

        if r >= num_rows || c >= num_cols {
            continue;
        }

        let left = col_starts[c];
        let right = left + col_widths[c];
        let top = row_starts[r];
        let bottom = top - row_heights[r];

        let cell_bounds = Bounds3D::new(DVec3::new(left, bottom, 0.0), DVec3::new(right, top, 0.0));
        let item_bounds = item_sizes[i];

        let p_item_local_align = cell_alignment.get_point(&item_bounds);
        let p_cell_align = cell_alignment.get_point(&cell_bounds);

        let t = p_cell_align - p_item_local_align;
        result[i] = t;

        // Bounding box of this item after applying the calculated translation
        let item_bounds_shifted = Bounds3D::new(item_bounds.min + t, item_bounds.max + t);
        group_min = group_min.min(item_bounds_shifted.min);
        group_max = group_max.max(item_bounds_shifted.max);
    }

    if center_result && group_min.x < group_max.x && group_min.y < group_max.y {
        let group_center = (group_min + group_max) * 0.5;
        for translation in &mut result {
            translation.x -= group_center.x;
            translation.y -= group_center.y;
        }
    }

    result
}

/// Vertical stack (top-to-bottom) — convenience for arrange(DOWN)
pub fn vstack(items: &[(Bounds3D, SpatialTransform)], spacing: f64) -> Vec<DVec3> {
    arrange(items, Direction::Down, spacing, Anchor::Center, true)
}

/// Horizontal stack (left-to-right) — convenience for arrange(RIGHT)
pub fn hstack(items: &[(Bounds3D, SpatialTransform)], spacing: f64) -> Vec<DVec3> {
    arrange(items, Direction::Right, spacing, Anchor::Center, true)
}
