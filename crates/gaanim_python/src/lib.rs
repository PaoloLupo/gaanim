use pyo3::prelude::*;

// Re-export peniko from the engine core crate under a different alias
// to avoid the name clash with our own pymodule.
use ::gaanim_core as engine;

mod animation;
mod color;
mod id;
mod mobject;
mod runtime;
mod scene;
mod selection;
mod theme;

/// Gaanim Python bindings — high-performance GPU-accelerated vector animation engine.
#[pymodule]
fn gaanim_core(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<scene::PyScene>()?;
    m.add_class::<mobject::PyMobject>()?;
    m.add_class::<selection::PySelection>()?;
    m.add_class::<selection::PySelectionAnim>()?;
    m.add_class::<animation::PyAnimationSpec>()?;
    m.add_class::<color::PyColor>()?;
    m.add_class::<id::PyObjectId>()?;
    m.add_class::<theme::PyTheme>()?;

    // Color palette — exposed as module-level constants.
    m.add(
        "GOLD",
        color::PyColor(engine::peniko::Color::from_rgb8(0xFF, 0xD7, 0x00)),
    )?;
    m.add(
        "CORAL",
        color::PyColor(engine::peniko::Color::from_rgb8(0xFF, 0x64, 0x64)),
    )?;
    m.add(
        "BLUE",
        color::PyColor(engine::peniko::Color::from_rgb8(0x19, 0x32, 0x64)),
    )?;
    m.add("WHITE", color::PyColor(engine::peniko::Color::WHITE))?;
    m.add("BLACK", color::PyColor(engine::peniko::Color::BLACK))?;
    m.add(
        "RED",
        color::PyColor(engine::peniko::Color::from_rgb8(0xE5, 0x4B, 0x4B)),
    )?;
    m.add(
        "GREEN",
        color::PyColor(engine::peniko::Color::from_rgb8(0x4B, 0xE5, 0x7C)),
    )?;
    m.add(
        "YELLOW",
        color::PyColor(engine::peniko::Color::from_rgb8(0xF5, 0xD0, 0x4B)),
    )?;
    m.add(
        "ORANGE",
        color::PyColor(engine::peniko::Color::from_rgb8(0xFF, 0x9F, 0x43)),
    )?;
    m.add(
        "PURPLE",
        color::PyColor(engine::peniko::Color::from_rgb8(0x9B, 0x59, 0xB6)),
    )?;
    m.add(
        "PINK",
        color::PyColor(engine::peniko::Color::from_rgb8(0xFF, 0x7A, 0xB6)),
    )?;
    m.add(
        "GRAY",
        color::PyColor(engine::peniko::Color::from_rgb8(0x80, 0x80, 0x80)),
    )?;
    m.add(
        "CYAN",
        color::PyColor(engine::peniko::Color::from_rgb8(0x4B, 0xE5, 0xE5)),
    )?;
    m.add(
        "NAVY",
        color::PyColor(engine::peniko::Color::from_rgb8(0x1B, 0x1F, 0x3B)),
    )?;
    m.add(
        "TEAL",
        color::PyColor(engine::peniko::Color::from_rgb8(0x2E, 0x86, 0xAB)),
    )?;

    Ok(())
}
