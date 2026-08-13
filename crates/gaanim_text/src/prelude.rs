pub use crate::GaanimTextPlugin;
pub use crate::config::{RoleStyle, TextConfig, TextRole};
pub use crate::font::{FontRegistry, OutlineCollector};
pub use crate::shaper::{
    HierarchyChild, ShapedGlyph, compile_text_to_hierarchy, compile_text_to_path, shape_text,
};
pub use crate::structured::{
    InlineSegment, TextAlign, TextAnchor, TextContent, TextDirection, TextFlow, TextOverflow,
    TextPart, TextPartInfo, TextSpec, TextSpecError, TextStyle, TextWrap, flatten_content,
    parse_inline_math, rendered_text,
};
pub use crate::typst_compiler::{
    GaanimTypstWorld, TextMetrics, compile_typst_to_hierarchy, measure_typst,
};
