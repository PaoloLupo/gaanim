//! Composable regions and grids for editorial/video layouts.

use gaanim_core::glam::DVec3;
use gaanim_layout::Anchor;
use gaanim_math::Bounds3D;

use super::DrawableHandle;

/// A rectangular safe area in canvas coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutRegion {
    pub bounds: Bounds3D,
}

impl LayoutRegion {
    pub fn width(self) -> f64 {
        self.bounds.width()
    }

    pub fn height(self) -> f64 {
        self.bounds.height()
    }

    pub fn anchor_point(self, anchor: Anchor) -> DVec3 {
        anchor.get_point(&self.bounds)
    }

    /// Pins an anchor of `drawable` to the matching anchor in this region.
    pub fn place(self, drawable: DrawableHandle, anchor: Anchor) -> DrawableHandle {
        let point = self.anchor_point(anchor);
        drawable.at_anchor(point.x, point.y, anchor)
    }

    /// Returns a safe area inset from every edge.
    pub fn inset(self, top: f64, right: f64, bottom: f64, left: f64) -> Self {
        let left = left.max(0.0).min(self.width());
        let right = right.max(0.0).min(self.width() - left);
        let bottom = bottom.max(0.0).min(self.height());
        let top = top.max(0.0).min(self.height() - bottom);
        Self {
            bounds: Bounds3D::new_2d(
                self.bounds.min.x + left,
                self.bounds.min.y + bottom,
                self.bounds.max.x - right,
                self.bounds.max.y - top,
            ),
        }
    }

    /// Divides this region into equal cells. Row zero is the top row.
    pub fn grid(self, rows: usize, columns: usize, row_gap: f64, column_gap: f64) -> GridLayout {
        GridLayout::new(self, rows, columns, row_gap, column_gap)
    }

    /// Divides this region using fixed and fractional tracks.
    pub fn grid_with_tracks(
        self,
        row_tracks: Vec<GridTrack>,
        column_tracks: Vec<GridTrack>,
        row_gap: f64,
        column_gap: f64,
    ) -> GridLayout {
        GridLayout::with_tracks(self, row_tracks, column_tracks, row_gap, column_gap)
    }
}

/// A grid track with either a fixed canvas-unit size or a share of free space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridTrack {
    Fixed(f64),
    Fraction(f64),
}

impl GridTrack {
    fn sanitized(self) -> Self {
        match self {
            Self::Fixed(value) => Self::Fixed(value.max(0.0)),
            Self::Fraction(value) => Self::Fraction(value.max(0.0)),
        }
    }
}

/// A rectangular grid whose cells can be combined with row/column spans.
#[derive(Debug, Clone, PartialEq)]
pub struct GridLayout {
    pub region: LayoutRegion,
    pub rows: usize,
    pub columns: usize,
    pub row_gap: f64,
    pub column_gap: f64,
    pub row_tracks: Vec<GridTrack>,
    pub column_tracks: Vec<GridTrack>,
}

impl GridLayout {
    pub fn new(
        region: LayoutRegion,
        rows: usize,
        columns: usize,
        row_gap: f64,
        column_gap: f64,
    ) -> Self {
        Self::with_tracks(
            region,
            vec![GridTrack::Fraction(1.0); rows.max(1)],
            vec![GridTrack::Fraction(1.0); columns.max(1)],
            row_gap,
            column_gap,
        )
    }

    pub fn with_tracks(
        region: LayoutRegion,
        mut row_tracks: Vec<GridTrack>,
        mut column_tracks: Vec<GridTrack>,
        row_gap: f64,
        column_gap: f64,
    ) -> Self {
        if row_tracks.is_empty() {
            row_tracks.push(GridTrack::Fraction(1.0));
        }
        if column_tracks.is_empty() {
            column_tracks.push(GridTrack::Fraction(1.0));
        }
        for track in &mut row_tracks {
            *track = track.sanitized();
        }
        for track in &mut column_tracks {
            *track = track.sanitized();
        }
        Self {
            region,
            rows: row_tracks.len(),
            columns: column_tracks.len(),
            row_gap: row_gap.max(0.0),
            column_gap: column_gap.max(0.0),
            row_tracks,
            column_tracks,
        }
    }

    fn resolved_tracks(tracks: &[GridTrack], available: f64) -> Vec<f64> {
        let fixed: f64 = tracks
            .iter()
            .map(|track| match track {
                GridTrack::Fixed(size) => *size,
                GridTrack::Fraction(_) => 0.0,
            })
            .sum();
        let fractions: f64 = tracks
            .iter()
            .map(|track| match track {
                GridTrack::Fixed(_) => 0.0,
                GridTrack::Fraction(weight) => *weight,
            })
            .sum();
        let free = (available - fixed).max(0.0);
        tracks
            .iter()
            .map(|track| match track {
                GridTrack::Fixed(size) => *size,
                GridTrack::Fraction(weight) if fractions > 0.0 => free * *weight / fractions,
                GridTrack::Fraction(_) => 0.0,
            })
            .collect()
    }

    pub fn cell(&self, row: usize, column: usize) -> Option<LayoutRegion> {
        self.area(row, column, 1, 1)
    }

    pub fn area(
        &self,
        row: usize,
        column: usize,
        row_span: usize,
        column_span: usize,
    ) -> Option<LayoutRegion> {
        if row >= self.rows || column >= self.columns || row_span == 0 || column_span == 0 {
            return None;
        }
        let row_end = row.checked_add(row_span)?;
        let column_end = column.checked_add(column_span)?;
        if row_end > self.rows || column_end > self.columns {
            return None;
        }
        let widths = Self::resolved_tracks(
            &self.column_tracks,
            self.region.width() - self.column_gap * self.columns.saturating_sub(1) as f64,
        );
        let heights = Self::resolved_tracks(
            &self.row_tracks,
            self.region.height() - self.row_gap * self.rows.saturating_sub(1) as f64,
        );
        let min_x = self.region.bounds.min.x
            + widths[..column].iter().sum::<f64>()
            + self.column_gap * column as f64;
        let max_x = min_x
            + widths[column..column_end].iter().sum::<f64>()
            + self.column_gap * column_end.saturating_sub(column + 1) as f64;
        let max_y = self.region.bounds.max.y
            - heights[..row].iter().sum::<f64>()
            - self.row_gap * row as f64;
        let min_y = max_y
            - heights[row..row_end].iter().sum::<f64>()
            - self.row_gap * row_end.saturating_sub(row + 1) as f64;
        Some(LayoutRegion {
            bounds: Bounds3D::new_2d(min_x, min_y, max_x, max_y),
        })
    }
}

/// Safe frame plus conventional editorial regions.
///
/// `header` and `footer` may have zero height. `content` occupies the remaining
/// area and every region can create nested grids of its own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameLayout {
    pub frame: LayoutRegion,
    pub header: LayoutRegion,
    pub content: LayoutRegion,
    pub footer: LayoutRegion,
    pub gap: f64,
}

/// Named editorial compositions derived from the safe frame dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutPreset {
    Lecture,
    Comparison,
    VerticalShort,
    Minimal,
}

impl FrameLayout {
    pub fn new(frame: Bounds3D, header_height: f64, footer_height: f64, gap: f64) -> Self {
        let header_height = header_height.max(0.0).min(frame.height());
        let remaining = (frame.height() - header_height).max(0.0);
        let footer_height = footer_height.max(0.0).min(remaining);
        let gap = gap.max(0.0);
        let header_bottom = frame.max.y - header_height;
        let footer_top = frame.min.y + footer_height;
        let content_min_y =
            (footer_top + if footer_height > 0.0 { gap } else { 0.0 }).min(header_bottom);
        let content_max_y =
            (header_bottom - if header_height > 0.0 { gap } else { 0.0 }).max(content_min_y);
        let region = |min_y, max_y| LayoutRegion {
            bounds: Bounds3D::new_2d(frame.min.x, min_y, frame.max.x, max_y),
        };
        Self {
            frame: LayoutRegion { bounds: frame },
            header: region(header_bottom, frame.max.y),
            content: region(content_min_y, content_max_y),
            footer: region(frame.min.y, footer_top),
            gap,
        }
    }

    /// Creates an editorial composition whose dimensions scale with the frame.
    pub fn preset(frame: Bounds3D, preset: LayoutPreset) -> Self {
        let height = frame.height();
        let (header, footer, gap) = match preset {
            LayoutPreset::Lecture => (height * 0.18, height * 0.08, height * 0.03),
            LayoutPreset::Comparison => (height * 0.16, height * 0.07, height * 0.03),
            LayoutPreset::VerticalShort => (height * 0.20, height * 0.10, height * 0.035),
            LayoutPreset::Minimal => (0.0, 0.0, height * 0.02),
        };
        Self::new(frame, header, footer, gap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_layout_reserves_header_footer_and_gaps() {
        let layout = FrameLayout::new(
            Bounds3D::new_2d(-500.0, -300.0, 500.0, 300.0),
            100.0,
            50.0,
            20.0,
        );
        assert_eq!(layout.header.height(), 100.0);
        assert_eq!(layout.footer.height(), 50.0);
        assert_eq!(layout.content.bounds.min.y, -230.0);
        assert_eq!(layout.content.bounds.max.y, 180.0);
    }

    #[test]
    fn grid_supports_cells_and_spans() {
        let grid = LayoutRegion {
            bounds: Bounds3D::new_2d(0.0, 0.0, 100.0, 100.0),
        }
        .grid(2, 2, 10.0, 10.0);
        let top_left = grid.cell(0, 0).unwrap();
        let bottom_row = grid.area(1, 0, 1, 2).unwrap();
        assert_eq!(top_left.width(), 45.0);
        assert_eq!(top_left.bounds.min.y, 55.0);
        assert_eq!(bottom_row.width(), 100.0);
        assert_eq!(bottom_row.bounds.max.y, 45.0);
    }

    #[test]
    fn grid_resolves_fixed_and_fractional_tracks() {
        let grid = LayoutRegion {
            bounds: Bounds3D::new_2d(0.0, 0.0, 400.0, 100.0),
        }
        .grid_with_tracks(
            vec![GridTrack::Fraction(1.0)],
            vec![
                GridTrack::Fixed(100.0),
                GridTrack::Fraction(1.0),
                GridTrack::Fraction(2.0),
            ],
            0.0,
            10.0,
        );
        assert_eq!(grid.cell(0, 0).unwrap().width(), 100.0);
        assert!((grid.cell(0, 1).unwrap().width() - 280.0 / 3.0).abs() < 1e-9);
        assert!((grid.cell(0, 2).unwrap().width() - 560.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn vertical_preset_scales_from_safe_frame_height() {
        let preset = FrameLayout::preset(
            Bounds3D::new_2d(0.0, 0.0, 400.0, 1000.0),
            LayoutPreset::VerticalShort,
        );
        assert_eq!(preset.header.height(), 200.0);
        assert_eq!(preset.footer.height(), 100.0);
        assert_eq!(preset.gap, 35.0);
    }
}
