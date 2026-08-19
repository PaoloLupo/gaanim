//! Public, renderer-independent matrix indexing and animation ordering.

use std::collections::{HashMap, HashSet};

/// Rectangular matrix dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatrixShape {
    pub rows: usize,
    pub columns: usize,
}

impl MatrixShape {
    pub fn new(rows: usize, columns: usize) -> Result<Self, MatrixError> {
        if rows == 0 || columns == 0 {
            return Err(MatrixError::Empty);
        }
        Ok(Self { rows, columns })
    }

    pub fn contains(self, row: usize, column: usize) -> bool {
        row < self.rows && column < self.columns
    }

    pub fn coordinates(self) -> Vec<MatrixIndex> {
        (0..self.rows)
            .flat_map(|row| (0..self.columns).map(move |column| MatrixIndex { row, column }))
            .collect()
    }
}

/// Zero-based coordinate of one matrix entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MatrixIndex {
    pub row: usize,
    pub column: usize,
}

/// Built-in traversal used when staggering entry animations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatrixOrder {
    Simultaneous,
    RowMajor,
    ColumnMajor,
    MainDiagonal,
    AntiDiagonal,
    SpiralIn,
    SpiralOut,
    Random { seed: u64 },
}

/// Validation errors shared by the Rust and Python matrix facades.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MatrixError {
    #[error("matrix data must contain at least one row and one column")]
    Empty,
    #[error("matrix rows must all contain {expected} entries; row {row} contains {actual}")]
    Ragged {
        row: usize,
        expected: usize,
        actual: usize,
    },
    #[error("matrix index ({row}, {column}) is outside shape {rows}x{columns}")]
    OutOfBounds {
        row: usize,
        column: usize,
        rows: usize,
        columns: usize,
    },
    #[error("explicit matrix animation order contains a duplicate coordinate ({row}, {column})")]
    DuplicateOrder { row: usize, column: usize },
}

pub fn validate_rows<T>(rows: &[Vec<T>]) -> Result<MatrixShape, MatrixError> {
    let columns = rows.first().map_or(0, Vec::len);
    let shape = MatrixShape::new(rows.len(), columns)?;
    for (row, values) in rows.iter().enumerate() {
        if values.len() != columns {
            return Err(MatrixError::Ragged {
                row,
                expected: columns,
                actual: values.len(),
            });
        }
    }
    Ok(shape)
}

/// Deterministically order an arbitrary selection of coordinates.
pub fn order_indices(
    shape: MatrixShape,
    selected: &[MatrixIndex],
    order: MatrixOrder,
) -> Result<Vec<MatrixIndex>, MatrixError> {
    let mut seen = HashSet::with_capacity(selected.len());
    for index in selected {
        if !shape.contains(index.row, index.column) {
            return Err(MatrixError::OutOfBounds {
                row: index.row,
                column: index.column,
                rows: shape.rows,
                columns: shape.columns,
            });
        }
        if !seen.insert(*index) {
            return Err(MatrixError::DuplicateOrder {
                row: index.row,
                column: index.column,
            });
        }
    }

    let mut result = selected.to_vec();
    match order {
        MatrixOrder::Simultaneous | MatrixOrder::RowMajor => result.sort(),
        MatrixOrder::ColumnMajor => result.sort_by_key(|index| (index.column, index.row)),
        MatrixOrder::MainDiagonal => {
            result.sort_by_key(|index| (index.row + index.column, index.row, index.column))
        }
        MatrixOrder::AntiDiagonal => result.sort_by_key(|index| {
            (
                index.row + (shape.columns - 1 - index.column),
                index.row,
                index.column,
            )
        }),
        MatrixOrder::SpiralIn | MatrixOrder::SpiralOut => {
            let positions: HashMap<_, _> = spiral(shape)
                .into_iter()
                .enumerate()
                .map(|(position, index)| (index, position))
                .collect();
            result.sort_by_key(|index| positions[index]);
            if matches!(order, MatrixOrder::SpiralOut) {
                result.reverse();
            }
        }
        MatrixOrder::Random { seed } => {
            result.sort_by_key(|index| splitmix64(seed ^ coordinate_key(*index)));
        }
    }
    Ok(result)
}

fn coordinate_key(index: MatrixIndex) -> u64 {
    ((index.row as u64) << 32) | index.column as u64
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn spiral(shape: MatrixShape) -> Vec<MatrixIndex> {
    let (mut top, mut bottom, mut left, mut right) = (
        0isize,
        shape.rows as isize - 1,
        0isize,
        shape.columns as isize - 1,
    );
    let mut result = Vec::with_capacity(shape.rows * shape.columns);
    while top <= bottom && left <= right {
        for column in left..=right {
            result.push(MatrixIndex {
                row: top as usize,
                column: column as usize,
            });
        }
        top += 1;
        for row in top..=bottom {
            result.push(MatrixIndex {
                row: row as usize,
                column: right as usize,
            });
        }
        right -= 1;
        if top <= bottom {
            for column in (left..=right).rev() {
                result.push(MatrixIndex {
                    row: bottom as usize,
                    column: column as usize,
                });
            }
            bottom -= 1;
        }
        if left <= right {
            for row in (top..=bottom).rev() {
                result.push(MatrixIndex {
                    row: row as usize,
                    column: left as usize,
                });
            }
            left += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_ragged_data() {
        assert_eq!(validate_rows::<i32>(&[]), Err(MatrixError::Empty));
        assert!(matches!(
            validate_rows(&[vec![1, 2], vec![3]]),
            Err(MatrixError::Ragged { row: 1, .. })
        ));
    }

    #[test]
    fn orders_columns_diagonals_and_spirals() {
        let shape = MatrixShape::new(2, 3).unwrap();
        let all = shape.coordinates();
        let columns = order_indices(shape, &all, MatrixOrder::ColumnMajor).unwrap();
        assert_eq!(columns[1], MatrixIndex { row: 1, column: 0 });
        let diagonal = order_indices(shape, &all, MatrixOrder::MainDiagonal).unwrap();
        assert_eq!(diagonal[0], MatrixIndex { row: 0, column: 0 });
        assert_eq!(diagonal[1], MatrixIndex { row: 0, column: 1 });
        let spiral = order_indices(shape, &all, MatrixOrder::SpiralIn).unwrap();
        assert_eq!(
            spiral,
            vec![
                MatrixIndex { row: 0, column: 0 },
                MatrixIndex { row: 0, column: 1 },
                MatrixIndex { row: 0, column: 2 },
                MatrixIndex { row: 1, column: 2 },
                MatrixIndex { row: 1, column: 1 },
                MatrixIndex { row: 1, column: 0 },
            ]
        );
    }

    #[test]
    fn random_order_is_seeded() {
        let shape = MatrixShape::new(4, 4).unwrap();
        let all = shape.coordinates();
        let first = order_indices(shape, &all, MatrixOrder::Random { seed: 7 }).unwrap();
        let second = order_indices(shape, &all, MatrixOrder::Random { seed: 7 }).unwrap();
        let other = order_indices(shape, &all, MatrixOrder::Random { seed: 8 }).unwrap();
        assert_eq!(first, second);
        assert_ne!(first, other);
    }
}
