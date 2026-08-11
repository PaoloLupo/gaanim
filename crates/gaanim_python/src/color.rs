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
    #[pyo3(signature = (value, g=None, b=None, a=None))]
    fn new(
        value: &Bound<'_, PyAny>,
        g: Option<u8>,
        b: Option<u8>,
        a: Option<u8>,
    ) -> PyResult<Self> {
        if let Ok(source) = value.extract::<&str>() {
            if g.is_some() || b.is_some() || a.is_some() {
                return Err(PyValueError::new_err(
                    "Color(css) does not accept separate g, b, or a components",
                ));
            }
            return peniko::Color::from_str(source).map(Self).map_err(|error| {
                PyValueError::new_err(format!("invalid color '{source}': {error}"))
            });
        }
        let r = value.extract::<u8>().map_err(|_| {
            PyValueError::new_err("Color expects a CSS color string or r, g, b integer components")
        })?;
        let (g, b) = match (g, b) {
            (Some(g), Some(b)) => (g, b),
            _ => {
                return Err(PyValueError::new_err(
                    "Color(r, g, b, a?) requires all three RGB components",
                ))
            }
        };
        Ok(Self(peniko::Color::from_rgba8(r, g, b, a.unwrap_or(0xFF))))
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

    /// Construct an HSL color. Saturation, lightness, and alpha use 0..1.
    #[staticmethod]
    #[pyo3(signature = (h, s, l, a=1.0))]
    fn from_hsl(h: f64, s: f64, l: f64, a: f64) -> PyResult<Self> {
        if !h.is_finite()
            || !s.is_finite()
            || !l.is_finite()
            || !a.is_finite()
            || !(0.0..=1.0).contains(&s)
            || !(0.0..=1.0).contains(&l)
            || !(0.0..=1.0).contains(&a)
        {
            return Err(PyValueError::new_err(
                "h must be finite; s, l, and a must be between 0 and 1",
            ));
        }
        let css = format!("hsl({h} {}% {}% / {a})", s * 100.0, l * 100.0);
        peniko::Color::from_str(&css)
            .map(Self)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Construct an OKLCH color. Lightness and alpha use 0..1; hue is degrees.
    #[staticmethod]
    #[pyo3(signature = (l, c, h, a=1.0))]
    fn from_oklch(l: f64, c: f64, h: f64, a: f64) -> PyResult<Self> {
        if !l.is_finite()
            || !c.is_finite()
            || !h.is_finite()
            || !a.is_finite()
            || !(0.0..=1.0).contains(&l)
            || c < 0.0
            || !(0.0..=1.0).contains(&a)
        {
            return Err(PyValueError::new_err(
                "l and a must be between 0 and 1; c must be finite and non-negative; h must be finite",
            ));
        }
        let css = format!("oklch({}% {c} {h} / {a})", l * 100.0);
        peniko::Color::from_str(&css)
            .map(Self)
            .map_err(|error| PyValueError::new_err(error.to_string()))
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
