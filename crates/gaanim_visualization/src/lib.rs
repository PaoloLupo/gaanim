//! Coordinate systems and data visualization geometry for Gaanim.

pub mod axis;
pub mod chart;
pub mod data;
pub mod sampling;
pub mod space;
pub mod statistics;

pub use axis::{
    Axis, AxisError, AxisLabelPosition, AxisStyle, AxisStylePatch, Crossing, NumberFormat, Scale,
    Tick,
};
pub use chart::{
    BatchDatum, Channel, ChartError, ChartSpec, ConstantValue, DatumKey, DatumMatch, Encoding,
    GuideSpec, MarkBatch, MarkKind, MarkSpec, MarkTransition, MatchPolicy, ScaleKind, ScaleSpec,
    TransitionFallback, TransitionKind,
};
pub use data::{Column, DataError, DataSource, DataTable, DataValue};
pub use sampling::{
    SampledPath, Sampling, SamplingError, SurfaceMesh, VectorGlyph, implicit_contours,
    sample_expression, sample_function, sample_parametric, sample_surface, sample_vector_field,
};
pub use space::{
    CartesianSpace, ComplexSpace, CoordinateMap2D, CoordinateMap3D, LabelGeometry, NumberLine,
    PlotFrame, PolarSpace, SpaceGeometry2D, SpaceLayer,
};
pub use statistics::{
    BoxStats, DataMarkKind, Histogram, MarkError, NonFinitePolicy, RectMark, area_path, bars,
    box_stats, data_mark_path, error_bar_path, histogram, line_path, scatter_points, step_path,
    violin_path,
};
