//! Treemaps: nested rectangles whose areas are proportional to values.
//!
//! Rows give a path of names, from the outermost group to the leaf, and a
//! value. Every group's value is the sum of its rows. Each level is laid out
//! with the squarified algorithm of Bruls, Huizing and van Wijk (2000):
//! siblings, largest first, fill strips along the shorter side of what is
//! left, each strip closed when adding another rectangle would make the
//! strip's rectangles less square. Equal values keep the order of their
//! first row, so a layout is the same on every run.

use std::collections::HashMap;

use kurbo::Rect;

/// One rectangle of a treemap: a group or a leaf.
#[derive(Debug, Clone, PartialEq)]
pub struct TreemapCell {
    /// The names from the outermost group to this cell.
    pub path: Vec<String>,
    /// The sum of the values of the rows under this cell.
    pub value: f64,
    /// Where the cell lies, in the treemap's coordinates (y up).
    pub rect: Rect,
    /// The index of the enclosing group in the cells, `None` at the top.
    pub parent: Option<usize>,
    /// Whether the cell is a row of the data rather than a group.
    pub leaf: bool,
}

impl TreemapCell {
    /// The cell's key: its path joined with `/`.
    pub fn key(&self) -> String {
        self.path.join("/")
    }

    /// Nesting depth: 0 for the outermost groups.
    pub fn depth(&self) -> usize {
        self.path.len().saturating_sub(1)
    }
}

/// Spacing of a treemap's cells.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TreemapSpacing {
    /// Gap between sibling cells.
    pub gap: f64,
    /// Space a group keeps inside its border around its children.
    pub padding: f64,
    /// Extra space a group keeps at its top, for its label.
    pub header: f64,
}

impl Default for TreemapSpacing {
    fn default() -> Self {
        Self {
            gap: 0.0,
            padding: 0.0,
            header: 0.0,
        }
    }
}

/// Invalid treemap data.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TreemapError {
    #[error("a treemap needs at least one row with a positive value")]
    Empty,
    #[error("row {row} has an empty path")]
    EmptyPath { row: usize },
    #[error("row {row} has the value {value}; values must be finite and not negative")]
    InvalidValue { row: usize, value: f64 },
    #[error("'{path}' is both a row and a group of other rows")]
    LeafAndGroup { path: String },
    #[error("the treemap's rectangle must have a positive width and height")]
    InvalidRect,
}

#[derive(Default)]
struct Node {
    path: Vec<String>,
    value: f64,
    children: Vec<usize>,
    leaf: bool,
}

/// Lay out `rows`, each a path and a value, inside `rect`.
///
/// Rows with the same path add up. Cells come out parents first, then their
/// children, depth first in layout order. Cells with a value of zero are
/// left out.
pub fn treemap(
    rows: &[(Vec<String>, f64)],
    rect: Rect,
    spacing: TreemapSpacing,
) -> Result<Vec<TreemapCell>, TreemapError> {
    if !(rect.width() > 0.0 && rect.height() > 0.0) {
        return Err(TreemapError::InvalidRect);
    }
    // Node 0 is the root; children keep the order of their first row.
    let mut nodes = vec![Node::default()];
    let mut index: HashMap<Vec<String>, usize> = HashMap::new();
    for (row, (path, value)) in rows.iter().enumerate() {
        if path.is_empty() {
            return Err(TreemapError::EmptyPath { row });
        }
        if !value.is_finite() || *value < 0.0 {
            return Err(TreemapError::InvalidValue { row, value: *value });
        }
        let mut parent = 0;
        for depth in 0..path.len() {
            let prefix = path[..=depth].to_vec();
            let leaf = depth + 1 == path.len();
            let node = match index.get(&prefix) {
                Some(&node) => node,
                None => {
                    nodes.push(Node {
                        path: prefix.clone(),
                        ..Node::default()
                    });
                    let node = nodes.len() - 1;
                    nodes[parent].children.push(node);
                    index.insert(prefix.clone(), node);
                    node
                }
            };
            if leaf {
                nodes[node].leaf = true;
            }
            nodes[node].value += value;
            parent = node;
        }
        nodes[0].value += value;
    }
    if let Some(node) = nodes
        .iter()
        .find(|node| node.leaf && !node.children.is_empty())
    {
        return Err(TreemapError::LeafAndGroup {
            path: node.path.join("/"),
        });
    }
    if nodes[0].value <= 0.0 {
        return Err(TreemapError::Empty);
    }
    let mut cells = Vec::new();
    lay_out(&nodes, 0, rect, None, spacing, &mut cells);
    Ok(cells)
}

fn lay_out(
    nodes: &[Node],
    node: usize,
    area: Rect,
    parent: Option<usize>,
    spacing: TreemapSpacing,
    cells: &mut Vec<TreemapCell>,
) {
    let mut children: Vec<usize> = nodes[node]
        .children
        .iter()
        .copied()
        .filter(|child| nodes[*child].value > 0.0)
        .collect();
    // Largest first; the sort is stable, so equal values keep their order.
    children.sort_by(|a, b| nodes[*b].value.total_cmp(&nodes[*a].value));
    let values: Vec<f64> = children.iter().map(|child| nodes[*child].value).collect();
    for (child, rect) in children.iter().zip(squarify(&values, area)) {
        let rect = inset(rect, spacing.gap / 2.0);
        cells.push(TreemapCell {
            path: nodes[*child].path.clone(),
            value: nodes[*child].value,
            rect,
            parent,
            leaf: nodes[*child].leaf,
        });
        let me = cells.len() - 1;
        if !nodes[*child].children.is_empty() {
            let inner = Rect::new(
                rect.x0 + spacing.padding,
                rect.y0 + spacing.padding,
                rect.x1 - spacing.padding,
                rect.y1 - spacing.padding - spacing.header,
            );
            let inner = if inner.width() > 0.0 && inner.height() > 0.0 {
                inner
            } else {
                Rect::from_center_size(rect.center(), (0.0, 0.0))
            };
            lay_out(nodes, *child, inner, Some(me), spacing, cells);
        }
    }
}

fn inset(rect: Rect, by: f64) -> Rect {
    if rect.width() <= 2.0 * by || rect.height() <= 2.0 * by {
        return Rect::from_center_size(rect.center(), (0.0, 0.0));
    }
    Rect::new(rect.x0 + by, rect.y0 + by, rect.x1 - by, rect.y1 - by)
}

/// Squarified rectangles for `values` (largest first works best) filling
/// `area` (y up: strips run from the top or the left).
pub fn squarify(values: &[f64], area: Rect) -> Vec<Rect> {
    let total: f64 = values.iter().sum();
    let mut rects = Vec::with_capacity(values.len());
    if values.is_empty() || total.is_nan() || total <= 0.0 {
        return values
            .iter()
            .map(|_| Rect::from_center_size(area.center(), (0.0, 0.0)))
            .collect();
    }
    // Areas in the units of `area`.
    let scale = area.area() / total;
    let areas: Vec<f64> = values.iter().map(|value| value * scale).collect();
    let mut free = area;
    let mut start = 0;
    while start < areas.len() {
        let side = free.width().min(free.height());
        let mut end = start + 1;
        let mut best = worst(&areas[start..end], side);
        while end < areas.len() {
            let next = worst(&areas[start..=end], side);
            if next > best {
                break;
            }
            best = next;
            end += 1;
        }
        let strip: f64 = areas[start..end].iter().sum();
        if free.width() >= free.height() {
            // A column at the left of what is left, filled from the top.
            let width = if free.height() > 0.0 {
                strip / free.height()
            } else {
                0.0
            };
            let mut top = free.y1;
            for area in &areas[start..end] {
                let height = if width > 0.0 { area / width } else { 0.0 };
                rects.push(Rect::new(free.x0, top - height, free.x0 + width, top));
                top -= height;
            }
            free.x0 = (free.x0 + width).min(free.x1);
        } else {
            // A row at the top of what is left, filled from the left.
            let height = if free.width() > 0.0 {
                strip / free.width()
            } else {
                0.0
            };
            let mut left = free.x0;
            for area in &areas[start..end] {
                let width = if height > 0.0 { area / height } else { 0.0 };
                rects.push(Rect::new(left, free.y1 - height, left + width, free.y1));
                left += width;
            }
            free.y1 = (free.y1 - height).max(free.y0);
        }
        start = end;
    }
    rects
}

/// The worst aspect ratio of `areas` laid in one strip along `side`.
fn worst(areas: &[f64], side: f64) -> f64 {
    let sum: f64 = areas.iter().sum();
    if sum <= 0.0 || side <= 0.0 {
        return f64::INFINITY;
    }
    let largest = areas.iter().copied().fold(0.0, f64::max);
    let smallest = areas.iter().copied().fold(f64::INFINITY, f64::min);
    let side2 = side * side;
    let sum2 = sum * sum;
    (side2 * largest / sum2).max(sum2 / (side2 * smallest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn squarify_fills_the_area_in_proportion() {
        // The example of Bruls et al.: a 6 x 4 rectangle.
        let values = [6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0];
        let rects = squarify(&values, Rect::new(0.0, 0.0, 6.0, 4.0));
        assert_eq!(rects.len(), values.len());
        for (rect, value) in rects.iter().zip(values) {
            assert!(close(rect.area(), value), "{rect:?} for {value}");
            assert!(rect.x0 >= -1e-9 && rect.x1 <= 6.0 + 1e-9);
            assert!(rect.y0 >= -1e-9 && rect.y1 <= 4.0 + 1e-9);
        }
        // The first two share the first column, each 3 x 2.
        assert!(close(rects[0].width(), 3.0) && close(rects[0].height(), 2.0));
        assert!(close(rects[1].x0, 0.0) && close(rects[1].y1, 2.0));
        // No two overlap.
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                let overlap = a.intersect(*b);
                assert!(overlap.area() < 1e-9, "{a:?} and {b:?}");
            }
        }
    }

    #[test]
    fn groups_sum_their_rows_and_hold_their_children() {
        let rows = vec![
            (path(&["Europe", "France"]), 3.0),
            (path(&["Asia", "China"]), 18.0),
            (path(&["Europe", "Germany"]), 4.0),
            (path(&["Asia", "Japan"]), 4.0),
            (path(&["Europe", "France"]), 1.0),
        ];
        let cells = treemap(
            &rows,
            Rect::new(0.0, 0.0, 10.0, 6.0),
            TreemapSpacing::default(),
        )
        .unwrap();
        let keys: Vec<String> = cells.iter().map(TreemapCell::key).collect();
        assert_eq!(
            keys,
            [
                "Asia",
                "Asia/China",
                "Asia/Japan",
                "Europe",
                "Europe/France",
                "Europe/Germany"
            ]
        );
        let europe = &cells[3];
        assert!(close(europe.value, 8.0) && !europe.leaf && europe.parent.is_none());
        assert!(close(cells[4].value, 4.0) && cells[4].leaf);
        assert_eq!(cells[4].parent, Some(3));
        let total: f64 = cells
            .iter()
            .filter(|cell| cell.leaf)
            .map(|cell| cell.rect.area())
            .sum();
        assert!(close(total, 60.0));
        for child in cells.iter().filter(|cell| cell.parent.is_some()) {
            let (parent, child) = (cells[child.parent.unwrap()].rect, child.rect);
            assert!(
                child.x0 >= parent.x0 - 1e-9
                    && child.y0 >= parent.y0 - 1e-9
                    && child.x1 <= parent.x1 + 1e-9
                    && child.y1 <= parent.y1 + 1e-9,
                "{child:?} outside {parent:?}"
            );
        }
    }

    #[test]
    fn spacing_insets_cells_and_reserves_headers() {
        let rows = vec![(path(&["a", "x"]), 1.0), (path(&["a", "y"]), 1.0)];
        let spacing = TreemapSpacing {
            gap: 0.2,
            padding: 0.1,
            header: 0.5,
        };
        let cells = treemap(&rows, Rect::new(0.0, 0.0, 4.0, 4.0), spacing).unwrap();
        let group = cells[0].rect;
        assert!(close(group.x0, 0.1) && close(group.y1, 3.9));
        for child in &cells[1..] {
            assert!(child.rect.y1 <= group.y1 - 0.1 - 0.5 - 0.1 + 1e-9);
            assert!(child.rect.x0 >= group.x0 + 0.1);
        }
    }

    #[test]
    fn invalid_rows_are_rejected() {
        let rect = Rect::new(0.0, 0.0, 1.0, 1.0);
        let spacing = TreemapSpacing::default();
        assert_eq!(treemap(&[], rect, spacing), Err(TreemapError::Empty));
        assert_eq!(
            treemap(&[(path(&["a"]), 0.0)], rect, spacing),
            Err(TreemapError::Empty)
        );
        assert_eq!(
            treemap(&[(Vec::new(), 1.0)], rect, spacing),
            Err(TreemapError::EmptyPath { row: 0 })
        );
        assert!(matches!(
            treemap(&[(path(&["a"]), f64::NAN)], rect, spacing),
            Err(TreemapError::InvalidValue { row: 0, .. })
        ));
        assert_eq!(
            treemap(
                &[(path(&["a"]), 1.0), (path(&["a", "b"]), 1.0)],
                rect,
                spacing
            ),
            Err(TreemapError::LeafAndGroup {
                path: "a".to_owned()
            })
        );
        assert_eq!(
            treemap(
                &[(path(&["a"]), 1.0)],
                Rect::new(0.0, 0.0, 0.0, 1.0),
                spacing
            ),
            Err(TreemapError::InvalidRect)
        );
    }

    #[test]
    fn zero_values_are_left_out_and_equal_values_keep_their_order() {
        let rows = vec![
            (path(&["b"]), 1.0),
            (path(&["zero"]), 0.0),
            (path(&["a"]), 1.0),
        ];
        let cells = treemap(
            &rows,
            Rect::new(0.0, 0.0, 2.0, 1.0),
            TreemapSpacing::default(),
        )
        .unwrap();
        let keys: Vec<String> = cells.iter().map(TreemapCell::key).collect();
        assert_eq!(keys, ["b", "a"]);
    }
}
