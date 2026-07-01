use gaanim_core::peniko;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::str::FromStr;

/// Native color type backed by `peniko::Color` (RGBA8 internally).
///
/// Construct via `Color(r, g, b, a?)`, `Color.from_hex("#RRGGBB")`, `Color.from_rgb(r, g, b)`,
/// or use one of the named module-level constants (`GOLD`, `CORAL`, `BLUE`, …).
///
/// Anywhere a `Color` is accepted you can also pass:
/// - A hex string: `"#FF0000"`, `"#F00"`, `"#FF000080"`
/// - A CSS color: `"red"`, `"rgb(255, 0, 0)"`, `"hsl(0, 100%, 50%)"`, …
/// - An `(r, g, b)` or `(r, g, b, a)` tuple of u8 values
#[pyclass(name = "Color", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone, Copy, Debug)]
pub struct PyColor(pub peniko::Color);

/// Accept `Color` objects, hex/CSS strings, and `(r, g, b)` / `(r, g, b, a)` tuples.
impl<'a, 'py> FromPyObject<'a, 'py> for PyColor {
    type Error = PyErr;

    fn extract(obj: pyo3::Borrowed<'a, 'py, PyAny>) -> Result<Self, Self::Error> {
        // 1. Already a PyColor instance — downcast to avoid recursive FromPyObject.
        if let Ok(bound) = obj.cast::<PyColor>() {
            let inner = bound.borrow().0;
            return Ok(PyColor(inner));
        }
        // 2. String → parse with the color crate's CSS Color 4 parser
        if let Ok(s) = obj.extract::<&str>() {
            let c = peniko::Color::from_str(s)
                .map_err(|e| PyValueError::new_err(format!("invalid color '{s}': {e}")))?;
            return Ok(PyColor(c));
        }
        // 3. Tuple (r, g, b) or (r, g, b, a)
        if let Ok((r, g, b)) = obj.extract::<(u8, u8, u8)>() {
            return Ok(PyColor(peniko::Color::from_rgb8(r, g, b)));
        }
        if let Ok((r, g, b, a)) = obj.extract::<(u8, u8, u8, u8)>() {
            return Ok(PyColor(peniko::Color::from_rgba8(r, g, b, a)));
        }
        Err(PyValueError::new_err(
            "expected a Color, hex string ('#FF0000', 'red'), or (r, g, b) tuple",
        ))
    }
}

#[pymethods]
impl PyColor {
    #[new]
    #[pyo3(signature = (r, g, b, a=None))]
    fn new(r: u8, g: u8, b: u8, a: Option<u8>) -> Self {
        let a = a.unwrap_or(0xFF);
        Self(peniko::Color::from_rgba8(r, g, b, a))
    }

    /// Parse a hex color string. Supports `#RGB`, `#RRGGBB`, `#RRGGBBAA` (with or without `#`).
    #[staticmethod]
    fn from_hex(s: &str) -> PyResult<Self> {
        let s = s.trim();
        let hex = s.strip_prefix('#').unwrap_or(s);
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16)
                    .map_err(|e| PyValueError::new_err(format!("invalid hex: {e}")))?;
                let g = u8::from_str_radix(&hex[1..2], 16)
                    .map_err(|e| PyValueError::new_err(format!("invalid hex: {e}")))?;
                let b = u8::from_str_radix(&hex[2..3], 16)
                    .map_err(|e| PyValueError::new_err(format!("invalid hex: {e}")))?;
                Ok(Self(peniko::Color::from_rgb8(r * 17, g * 17, b * 17)))
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16)
                    .map_err(|e| PyValueError::new_err(format!("invalid hex: {e}")))?;
                let g = u8::from_str_radix(&hex[2..4], 16)
                    .map_err(|e| PyValueError::new_err(format!("invalid hex: {e}")))?;
                let b = u8::from_str_radix(&hex[4..6], 16)
                    .map_err(|e| PyValueError::new_err(format!("invalid hex: {e}")))?;
                Ok(Self(peniko::Color::from_rgb8(r, g, b)))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16)
                    .map_err(|e| PyValueError::new_err(format!("invalid hex: {e}")))?;
                let g = u8::from_str_radix(&hex[2..4], 16)
                    .map_err(|e| PyValueError::new_err(format!("invalid hex: {e}")))?;
                let b = u8::from_str_radix(&hex[4..6], 16)
                    .map_err(|e| PyValueError::new_err(format!("invalid hex: {e}")))?;
                let a = u8::from_str_radix(&hex[6..8], 16)
                    .map_err(|e| PyValueError::new_err(format!("invalid hex: {e}")))?;
                Ok(Self(peniko::Color::from_rgba8(r, g, b, a)))
            }
            _ => Err(PyValueError::new_err(
                "hex color must be #RGB, #RRGGBB, or #RRGGBBAA",
            )),
        }
    }

    #[staticmethod]
    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self(peniko::Color::from_rgb8(r, g, b))
    }

    #[staticmethod]
    fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self(peniko::Color::from_rgba8(r, g, b, a))
    }

    #[getter]
    fn r(&self) -> u8 {
        self.0.to_rgba8().r
    }

    #[getter]
    fn g(&self) -> u8 {
        self.0.to_rgba8().g
    }

    #[getter]
    fn b(&self) -> u8 {
        self.0.to_rgba8().b
    }

    #[getter]
    fn a(&self) -> u8 {
        self.0.to_rgba8().a
    }

    fn __repr__(&self) -> String {
        let rgba = self.0.to_rgba8();
        if rgba.a == 0xFF {
            format!("Color(#{:02X}{:02X}{:02X})", rgba.r, rgba.g, rgba.b)
        } else {
            format!(
                "Color(#{:02X}{:02X}{:02X}{:02X})",
                rgba.r, rgba.g, rgba.b, rgba.a
            )
        }
    }

    fn __eq__(&self, other: &PyColor) -> bool {
        let a = self.0.to_rgba8();
        let b = other.0.to_rgba8();
        a.r == b.r && a.g == b.g && a.b == b.b && a.a == b.a
    }
}
