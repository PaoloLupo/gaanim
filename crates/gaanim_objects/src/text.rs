use bevy::prelude::Component;

/// Component: Represents plain text to be rendered into vector shapes.
#[derive(Component, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextContent {
    /// The string content to render.
    pub text: String,
    /// The name of the font family.
    pub font_family: String,
    /// The font size in screen/world points.
    pub font_size: f64,
}

impl Default for TextContent {
    fn default() -> Self {
        Self {
            text: String::new(),
            font_family: "New Computer Modern".into(),
            font_size: 24.0,
        }
    }
}

/// Component: Represents a LaTeX/Typst mathematical formula expression.
#[derive(Component, Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MathContent {
    /// The raw math expression source string.
    pub source: String,
}

/// Component: Represents a full Typst markup document to compile into vector art.
#[derive(Component, Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TypstDocument {
    /// The raw Typst markup code document.
    pub source: String,
}
