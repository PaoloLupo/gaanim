use bevy::prelude::Resource;
use std::collections::HashMap;

/// Standard typographic roles for vector text in Gaanim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextRole {
    Title,
    Subtitle,
    Heading,
    Body,
    Caption,
    Label,
    Math,
    Code,
}

/// Styling configuration for a specific typographic role.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoleStyle {
    pub font_family: String,
    pub size: f64,
    pub fill_color: gaanim_core::peniko::Color,
}

/// Global resource to manage default styles, fonts, and sizes for all text roles.
#[derive(Resource, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextConfig {
    pub roles: HashMap<TextRole, RoleStyle>,
}

impl Default for TextConfig {
    fn default() -> Self {
        let mut roles = HashMap::new();
        // Body copy and inline/display equations share one scientific scale.
        // Titles and captions remain distinct semantic roles.
        let body_size = 32.0;

        // Scientific default text face, bundled through FontRegistry.
        roles.insert(
            TextRole::Title,
            RoleStyle {
                font_family: "New Computer Modern".to_string(),
                size: 64.0,
                fill_color: gaanim_core::peniko::Color::WHITE,
            },
        );

        // Subtitle role
        roles.insert(
            TextRole::Subtitle,
            RoleStyle {
                font_family: "New Computer Modern".to_string(),
                size: 48.0,
                fill_color: gaanim_core::peniko::Color::WHITE,
            },
        );

        roles.insert(
            TextRole::Heading,
            RoleStyle {
                font_family: "New Computer Modern".to_string(),
                size: 40.0,
                fill_color: gaanim_core::peniko::Color::WHITE,
            },
        );

        // Body text role
        roles.insert(
            TextRole::Body,
            RoleStyle {
                font_family: "New Computer Modern".to_string(),
                size: body_size,
                fill_color: gaanim_core::peniko::Color::WHITE,
            },
        );

        // Caption role
        roles.insert(
            TextRole::Caption,
            RoleStyle {
                font_family: "New Computer Modern".to_string(),
                size: 24.0,
                fill_color: gaanim_core::peniko::Color::WHITE,
            },
        );

        roles.insert(
            TextRole::Label,
            RoleStyle {
                font_family: "New Computer Modern".to_string(),
                size: 28.0,
                fill_color: gaanim_core::peniko::Color::WHITE,
            },
        );

        // Mathematical / Equation role. Match Body so text labels and
        // equations compose on the same baseline scale by default.
        roles.insert(
            TextRole::Math,
            RoleStyle {
                font_family: "New Computer Modern Math".to_string(),
                size: body_size,
                fill_color: gaanim_core::peniko::Color::WHITE,
            },
        );

        // Monospace code role
        roles.insert(
            TextRole::Code,
            RoleStyle {
                font_family: "Consolas".to_string(),
                size: 28.0,
                fill_color: gaanim_core::peniko::Color::WHITE,
            },
        );

        Self { roles }
    }
}
