use ::gaanim_core as engine_core;
use pyo3::prelude::*;

pyo3::create_exception!(
    gaanim_core,
    LayoutOwnershipError,
    pyo3::exceptions::PyException
);

mod brush;
mod color;
mod py3d;
mod pycanvas;
mod pydrawable;
mod pylayout;
mod pymatrix;
mod pystyle;
mod pytext;
mod transition;
mod updater;
mod visualization;

/// Register the `gaanim_core` builtin module.
pub fn register_inittab() {
    pyo3::append_to_inittab!(gaanim_core);
}

#[pymodule]
pub fn gaanim_core(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add(
        "LayoutOwnershipError",
        _py.get_type::<LayoutOwnershipError>(),
    )?;
    m.add_class::<transition::PyTransitionType>()?;
    m.add_class::<color::PyColor>()?;
    m.add_class::<color::PyColorMap>()?;
    m.add_class::<brush::PyBrush>()?;
    m.add_class::<brush::PyBackground>()?;
    m.add_class::<pylayout::PyAnchor>()?;
    m.add_class::<pylayout::PyDirection>()?;
    m.add_class::<pylayout::PyLayoutExpression>()?;
    m.add_class::<pylayout::PyLayoutConstraint>()?;
    m.add_class::<pylayout::PyConstraintSet>()?;
    m.add_class::<pycanvas::PyTheme>()?;
    m.add_class::<pycanvas::PyCanvas>()?;
    m.add_class::<pycanvas::PyCamera>()?;
    m.add_class::<pycanvas::PyCameraConstraint>()?;
    m.add_class::<pycanvas::PyScene>()?;
    m.add_class::<pycanvas::PySegment>()?;
    m.add_class::<pydrawable::PyCanvasAnim>()?;
    m.add_class::<pydrawable::PyAnchorPoint>()?;
    m.add_class::<pydrawable::PyDrawable>()?;
    m.add_class::<pycanvas::PyPointRef>()?;
    m.add_class::<pycanvas::PyDimension>()?;
    m.add_class::<pycanvas::PyAngleDimension>()?;
    m.add_class::<pycanvas::PySurroundingRect>()?;
    m.add_class::<pycanvas::PyForceVector>()?;
    m.add_class::<pycanvas::PySupport>()?;
    m.add_class::<py3d::PyMaterial3D>()?;
    m.add_class::<py3d::PyPrimitive3D>()?;
    m.add_class::<pytext::PyTextStyle>()?;
    m.add_class::<pytext::PyTextAnchor>()?;
    m.add_class::<pystyle::PyStrokeStyle>()?;
    m.add_class::<pystyle::PyStyle>()?;
    m.add_class::<pystyle::PyAxesStyle>()?;
    m.add_class::<pytext::PyTextFlow>()?;
    m.add_class::<pytext::PyTextPart>()?;
    m.add_class::<pytext::PyTextParts>()?;
    m.add_class::<pytext::PyTextQuery>()?;
    m.add_class::<pytext::PyTextSelection>()?;
    m.add_class::<pytext::PyText>()?;
    m.add_function(wrap_pyfunction!(pytext::text_part, m)?)?;
    m.add_function(wrap_pyfunction!(pytext::text_parts, m)?)?;
    m.add_class::<pylayout::PyLayoutItem>()?;
    m.add_class::<pylayout::PyLayout>()?;
    m.add_class::<pymatrix::PyMatrixOrder>()?;
    m.add_class::<updater::PyUpdater>()?;
    m.add_class::<visualization::PyAxis>()?;
    m.add_class::<visualization::PyScale>()?;
    m.add_class::<visualization::PyField>()?;
    m.add_class::<visualization::PyValue>()?;
    m.add_class::<visualization::PyGuide>()?;
    m.add_class::<visualization::PyChartSpec>()?;
    m.add_class::<visualization::PyChart>()?;
    m.add_class::<visualization::PyExpr>()?;
    m.add_class::<visualization::PyParameter>()?;
    m.add_class::<visualization::PyReadout>()?;
    m.add_class::<visualization::PyVariable>()?;
    m.add_class::<visualization::PyCoordinateRef>()?;
    m.add_class::<visualization::PyCoordinateSpace>()?;
    m.add_class::<visualization::PyCoordinateSpace3D>()?;
    m.add_class::<visualization::PyVectorField>()?;
    m.add_class::<visualization::PyArrowVectorField>()?;
    m.add_class::<visualization::PyStreamLines>()?;
    m.add_class::<visualization::PyFlowParticles>()?;
    m.add_class::<visualization::PyNumberLine>()?;
    m.add_class::<visualization::PyPolarSpace>()?;
    m.add_class::<visualization::PyDataTable>()?;
    m.add_class::<visualization::PyDataSource>()?;
    m.add(
        "Cartesian2D",
        _py.get_type::<visualization::PyCoordinateSpace>(),
    )?;
    m.add(
        "Cartesian3D",
        _py.get_type::<visualization::PyCoordinateSpace3D>(),
    )?;
    m.add(
        "ComplexSpace",
        _py.get_type::<visualization::PyCoordinateSpace>(),
    )?;

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
