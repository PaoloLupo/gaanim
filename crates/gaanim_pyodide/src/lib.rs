//! The `gaanim.gaanim_core` extension module of the web playground.
//!
//! Pyodide loads Python extensions as Emscripten side modules, so this cdylib
//! links `gaanim_python` for `wasm32-unknown-emscripten`; its `#[pymodule]`
//! exports `PyInit_gaanim_core`. Unlike the application, which registers the
//! module as a builtin of its embedded interpreter, Pyodide imports it from
//! the `gaanim` package (`scripts/build_playground.py` packages both). The
//! module adds the playground's host functions (`gaanim_python::playground`).
//! On other targets this crate is empty.

#[cfg(target_os = "emscripten")]
pub use gaanim_python::gaanim_core;
