use gaanim_api::matrix::{order_indices, MatrixIndex, MatrixOrder, MatrixShape};
use pyo3::prelude::*;

/// Native deterministic ordering helper used by the Python matrix facade.
#[pyclass(name = "MatrixOrder", module = "gaanim_core", frozen)]
pub struct PyMatrixOrder;

#[pymethods]
impl PyMatrixOrder {
    #[staticmethod]
    #[pyo3(signature = (rows, columns, coordinates, order, seed=0))]
    fn order(
        rows: usize,
        columns: usize,
        coordinates: Vec<(usize, usize)>,
        order: &str,
        seed: u64,
    ) -> PyResult<Vec<(usize, usize)>> {
        let shape = MatrixShape::new(rows, columns)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        let order = match order {
            "row_major" => MatrixOrder::RowMajor,
            "column_major" => MatrixOrder::ColumnMajor,
            "main_diagonal" | "diagonal" => MatrixOrder::MainDiagonal,
            "anti_diagonal" => MatrixOrder::AntiDiagonal,
            "spiral_in" => MatrixOrder::SpiralIn,
            "spiral_out" => MatrixOrder::SpiralOut,
            "random" => MatrixOrder::Random { seed },
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "matrix order must be simultaneous, row_major, column_major, main_diagonal, anti_diagonal, spiral_in, spiral_out, random, a coordinate sequence, or callable",
                ));
            }
        };
        order_indices(
            shape,
            &coordinates
                .into_iter()
                .map(|(row, column)| MatrixIndex { row, column })
                .collect::<Vec<_>>(),
            order,
        )
        .map(|indices| {
            indices
                .into_iter()
                .map(|index| (index.row, index.column))
                .collect()
        })
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
    }
}
