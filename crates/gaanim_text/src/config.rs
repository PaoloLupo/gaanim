use bevy::prelude::Resource;
use std::collections::HashMap;

/// Standard typographic roles for vector text in Gaanim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextRole {
    Title,
    Subtitle,
    Body,
    Caption,
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

        // Standard Title role (Arial / sans-serif, large scale)
        roles.insert(
            TextRole::Title,
            RoleStyle {
                font_family: "Arial".to_string(),
                size: 64.0,
                fill_color: gaanim_core::peniko::Color::WHITE,
            },
        );

        // Subtitle role (Arial / sans-serif, medium scale)
        roles.insert(
            TextRole::Subtitle,
            RoleStyle {
                font_family: "Arial".to_string(),
                size: 48.0,
                fill_color: gaanim_core::peniko::Color::WHITE,
            },
        );

        // Body text role (Arial / sans-serif, standard scale)
        roles.insert(
            TextRole::Body,
            RoleStyle {
                font_family: "Arial".to_string(),
                size: 32.0,
                fill_color: gaanim_core::peniko::Color::WHITE,
            },
        );

        // Caption text role (Arial / sans-serif, small scale)
        roles.insert(
            TextRole::Caption,
            RoleStyle {
                font_family: "Arial".to_string(),
                size: 24.0,
                fill_color: gaanim_core::peniko::Color::WHITE,
            },
        );

        // Mathematical / Equation role (Typst default math font)
        roles.insert(
            TextRole::Math,
            RoleStyle {
                font_family: "New Computer Modern Math".to_string(),
                size: 48.0,
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
