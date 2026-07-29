use ::gaanim_core as engine_core;
use pyo3::prelude::*;

mod brush;
mod color;
mod pycanvas;
mod pydrawable;
mod pylayout;
mod transition;
mod updater;
mod value_tracker;

/// Register the `gaanim_core` builtin module.
pub fn register_inittab() {
    pyo3::append_to_inittab!(gaanim_core);
}

#[pymodule]
pub fn gaanim_core(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<transition::PyTransitionType>()?;
    m.add_class::<color::PyColor>()?;
    m.add_class::<brush::PyBrush>()?;
    m.add_class::<pylayout::PyAnchor>()?;
    m.add_class::<pylayout::PyDirection>()?;
    m.add_class::<pylayout::PyLayoutRegion>()?;
    m.add_class::<pylayout::PyFlow>()?;
    m.add_class::<pylayout::PyLayout>()?;
    m.add_class::<pylayout::PyGridLayout>()?;
    m.add_class::<pylayout::PyFrameLayout>()?;
    m.add_class::<pycanvas::PyTheme>()?;
    m.add_class::<pycanvas::PyCanvas>()?;
    m.add_class::<pycanvas::PyCamera>()?;
    m.add_class::<pycanvas::PyScene>()?;
    m.add_class::<pycanvas::PySlide>()?;
    m.add_class::<pydrawable::PyCanvasAnim>()?;
    m.add_class::<pydrawable::PyDrawable>()?;
    m.add_class::<pydrawable::PyFragmentSelection>()?;
    m.add_class::<updater::PyUpdater>()?;
    m.add_class::<value_tracker::PyValueTracker>()?;

    m.add(
        "GOLD",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0xFF, 0xD7, 0x00)),
    )?;
    m.add(
        "CORAL",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0xFF, 0x64, 0x64)),
    )?;
    m.add(
        "BLUE",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0x19, 0x32, 0x64)),
    )?;
    m.add("WHITE", color::PyColor(engine_core::peniko::Color::WHITE))?;
    m.add("BLACK", color::PyColor(engine_core::peniko::Color::BLACK))?;
    m.add(
        "RED",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0xE5, 0x4B, 0x4B)),
    )?;
    m.add(
        "GREEN",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0x4B, 0xE5, 0x7C)),
    )?;
    m.add(
        "YELLOW",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0xF5, 0xD0, 0x4B)),
    )?;
    m.add(
        "ORANGE",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0xFF, 0x9F, 0x43)),
    )?;
    m.add(
        "PURPLE",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0x9B, 0x59, 0xB6)),
    )?;
    m.add(
        "PINK",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0xFF, 0x7A, 0xB6)),
    )?;
    m.add(
        "GRAY",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0x80, 0x80, 0x80)),
    )?;
    m.add(
        "CYAN",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0x4B, 0xE5, 0xE5)),
    )?;
    m.add(
        "NAVY",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0x1B, 0x1F, 0x3B)),
    )?;
    m.add(
        "TEAL",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0x2E, 0x86, 0xAB)),
    )?;
    Ok(())
}
