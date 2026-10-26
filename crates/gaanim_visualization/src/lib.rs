//! Coordinate systems and data visualization geometry for Gaanim.

pub mod axis;
pub mod bar_race;
pub mod chart;
pub mod data;
pub mod geo;
pub mod graph;
pub mod sampling;
pub mod sankey;
pub mod space;
pub mod statistics;
pub mod treemap;
pub mod vector_field;

pub use axis::{
    Axis, AxisError, AxisLabelPosition, AxisStyle, AxisStylePatch, Crossing, NumberFormat, Scale,
    Tick,
};
pub use bar_race::{
    BarRaceError, BarRaceModel, BarRaceSlots, BarRaceState, MAX_RANK_SMOOTHING, ValueFormat,
    interpolate_label, label_opacity,
};
pub use chart::{
    ArcDatum, BatchDatum, Channel, ChartError, ChartSpec, ColorRowGroups, ConstantValue,
    DEFAULT_POINT_RADIUS, DatumKey, DatumMatch, Encoding, GuideSpec, MarkBatch, MarkKind, MarkSpec,
    MarkTransition, MatchPolicy, POINT_RADIUS_RANGE, ScaleKind, ScaleSpec, TransitionFallback,
    TransitionKind,
};
pub use data::{Column, DataError, DataSource, DataTable, DataValue};
pub use geo::{
    GeoError, GeoLine, GeoPolygon, Graticule, GraticuleLabel, GraticuleOptions, Longitudes,
    MERCATOR_MAX_LATITUDE, MapSide, MapView, Projection,
};
pub use graph::{GraphLayout, graph_layout};
pub use sampling::{
    SampledPath, Sampling, SamplingError, SurfaceMesh, VectorGlyph, implicit_contours,
    sample_function, sample_parametric, sample_parametric_surface, sample_surface,
    sample_vector_field,
};
pub use sankey::{
    SankeyAlign, SankeyError, SankeyLayout, SankeyLink, SankeyNode, SankeyOptions, ribbon, sankey,
};
pub use space::{
    Cartesian3DVisibility, CartesianSpace, CartesianVisibility, ComplexSpace, CoordinateMap2D,
    CoordinateMap3D, LabelGeometry, NumberLine, NumberLineVisibility, PlotFrame, PolarSpace,
    PolarVisibility, SpaceGeometry2D, SpaceLayer,
};
pub use statistics::{
    BoxStats, DataMarkKind, Histogram, MarkError, NonFinitePolicy, RectMark, area_path, bars,
    box_stats, data_mark_path, error_bar_path, histogram, line_path, scatter_points, step_path,
    violin_path,
};
pub use treemap::{TreemapCell, TreemapError, TreemapSpacing, squarify, treemap};
pub use vector_field::{
    FieldSample, StreamDirection, Streamline, StreamlineOptions, VectorField, VectorFieldError,
};
