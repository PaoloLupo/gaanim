use std::collections::HashMap;
use std::sync::Arc;

use gaanim_core::peniko::Color;
use gaanim_text::prelude::{TextConfig, TextRole};

use super::ThemeError;

/// Semantic colors shared by text and presentation components.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemePalette {
    pub background: Color,
    pub foreground: Color,
    pub muted: Color,
    pub title: Color,
    pub accent: Color,
    pub chart: Color,
    pub panel: Color,
    pub header: Color,
    pub rule: Color,
}

/// An embedded font registered when the canvas is compiled.
#[derive(Debug, Clone)]
pub struct ThemeFont {
    pub family: String,
    pub bytes: Arc<[u8]>,
}

/// Complete visual theme: semantic colors, typography, and optional font files.
#[derive(Debug, Clone)]
pub struct CanvasTheme {
    pub name: String,
    pub palette: ThemePalette,
    pub text: TextConfig,
    pub fonts: Vec<ThemeFont>,
}

impl CanvasTheme {
    pub const BUILTIN_NAMES: &'static [&'static str] = &[
        "technical",
        "presentation",
        "paper",
        "dracula",
        "nord",
        "solarized-dark",
        "solarized-light",
        "gruvbox-dark",
        "tokyo-night",
        "catppuccin-mocha",
        "catppuccin-latte",
    ];

    /// A neutral custom theme that can be overridden role by role.
    pub fn custom(name: impl Into<String>) -> Self {
        Self::from_palette(
            name,
            ThemePalette {
                background: rgb(0x0B1018),
                foreground: rgb(0xE2E8F0),
                muted: rgb(0x94A3B8),
                title: rgb(0xF8FAFC),
                accent: rgb(0x5B8FC9),
                chart: rgb(0x4C78A8),
                panel: rgb(0x101620),
                header: rgb(0x162B46),
                rule: rgb(0x5B7088),
            },
        )
    }

    pub fn builtin(name: &str) -> Result<Self, ThemeError> {
        let normalized = name.to_ascii_lowercase();
        let canonical = match normalized.as_str() {
            "scientific" => "technical",
            "thesis" | "deck" => "presentation",
            "light" => "paper",
            "gruvbox" => "gruvbox-dark",
            "tokyo" => "tokyo-night",
            "catppuccin" | "mocha" => "catppuccin-mocha",
            "latte" => "catppuccin-latte",
            other => other,
        };
        let palette = match canonical {
            "technical" => ThemePalette {
                background: rgb(0x0B1018),
                foreground: rgb(0xE2E8F0),
                muted: rgb(0x94A3B8),
                title: rgb(0xF8FAFC),
                accent: rgb(0x5B8FC9),
                chart: rgb(0x4C78A8),
                panel: rgb(0x101620),
                header: rgb(0x162B46),
                rule: rgb(0x5B7088),
            },
            "presentation" => ThemePalette {
                background: rgb(0x070B16),
                foreground: rgb(0xF4F7FB),
                muted: rgb(0x9EACC3),
                title: rgb(0xFFD166),
                accent: rgb(0xFFD166),
                chart: rgb(0x5B8FFF),
                panel: rgb(0x10182B),
                header: rgb(0x17233D),
                rule: rgb(0x384766),
            },
            "paper" => ThemePalette {
                background: Color::WHITE,
                foreground: Color::BLACK,
                muted: Color::BLACK,
                title: Color::BLACK,
                accent: rgb(0x2563EB),
                chart: rgb(0x2563EB),
                panel: rgb(0xF1F5F9),
                header: rgb(0xE2E8F0),
                rule: rgb(0xB8C4D4),
            },
            "dracula" => ThemePalette {
                background: rgb(0x282A36),
                foreground: rgb(0xF8F8F2),
                muted: rgb(0x6272A4),
                title: rgb(0xBD93F9),
                accent: rgb(0xFF79C6),
                chart: rgb(0x8BE9FD),
                panel: rgb(0x343746),
                header: rgb(0x44475A),
                rule: rgb(0x6272A4),
            },
            "nord" => ThemePalette {
                background: rgb(0x2E3440),
                foreground: rgb(0xECEFF4),
                muted: rgb(0xD8DEE9),
                title: rgb(0x88C0D0),
                accent: rgb(0x88C0D0),
                chart: rgb(0x81A1C1),
                panel: rgb(0x3B4252),
                header: rgb(0x434C5E),
                rule: rgb(0x4C566A),
            },
            "solarized-dark" => ThemePalette {
                background: rgb(0x002B36),
                foreground: rgb(0xEEE8D5),
                muted: rgb(0x93A1A1),
                title: rgb(0xB58900),
                accent: rgb(0x2AA198),
                chart: rgb(0x268BD2),
                panel: rgb(0x073642),
                header: rgb(0x0A4654),
                rule: rgb(0x586E75),
            },
            "solarized-light" => ThemePalette {
                background: rgb(0xFDF6E3),
                foreground: rgb(0x073642),
                muted: rgb(0x657B83),
                title: rgb(0xB58900),
                accent: rgb(0x2AA198),
                chart: rgb(0x268BD2),
                panel: rgb(0xEEE8D5),
                header: rgb(0xE5DDC8),
                rule: rgb(0x93A1A1),
            },
            "gruvbox-dark" => ThemePalette {
                background: rgb(0x282828),
                foreground: rgb(0xEBDBB2),
                muted: rgb(0xA89984),
                title: rgb(0xFABD2F),
                accent: rgb(0xFE8019),
                chart: rgb(0x83A598),
                panel: rgb(0x3C3836),
                header: rgb(0x504945),
                rule: rgb(0x665C54),
            },
            "tokyo-night" => ThemePalette {
                background: rgb(0x1A1B26),
                foreground: rgb(0xC0CAF5),
                muted: rgb(0x787C99),
                title: rgb(0x7AA2F7),
                accent: rgb(0xBB9AF7),
                chart: rgb(0x7DCFFF),
                panel: rgb(0x24283B),
                header: rgb(0x292E42),
                rule: rgb(0x3B4261),
            },
            "catppuccin-mocha" => ThemePalette {
                background: rgb(0x1E1E2E),
                foreground: rgb(0xCDD6F4),
                muted: rgb(0xA6ADC8),
                title: rgb(0xCBA6F7),
                accent: rgb(0xF5C2E7),
                chart: rgb(0x89B4FA),
                panel: rgb(0x313244),
                header: rgb(0x45475A),
                rule: rgb(0x585B70),
            },
            "catppuccin-latte" => ThemePalette {
                background: rgb(0xEFF1F5),
                foreground: rgb(0x4C4F69),
                muted: rgb(0x8C8FA1),
                title: rgb(0x8839EF),
                accent: rgb(0xEA76CB),
                chart: rgb(0x1E66F5),
                panel: rgb(0xE6E9EF),
                header: rgb(0xDCE0E8),
                rule: rgb(0x9CA0B0),
            },
            _ => return Err(ThemeError { name: name.into() }),
        };
        Ok(Self::from_palette(canonical, palette))
    }

    pub fn from_palette(name: impl Into<String>, palette: ThemePalette) -> Self {
        let mut result = Self {
            name: name.into(),
            palette,
            text: TextConfig::default(),
            fonts: Vec::new(),
        };
        result.sync_text_colors();
        result
    }

    pub fn sync_text_colors(&mut self) {
        self.text
            .roles
            .get_mut(&TextRole::Title)
            .unwrap()
            .fill_color = self.palette.title;
        self.text
            .roles
            .get_mut(&TextRole::Subtitle)
            .unwrap()
            .fill_color = self.palette.muted;
        self.text
            .roles
            .get_mut(&TextRole::Caption)
            .unwrap()
            .fill_color = self.palette.muted;
        for role in [TextRole::Body, TextRole::Math, TextRole::Code] {
            self.text.roles.get_mut(&role).unwrap().fill_color = self.palette.foreground;
        }
    }

    pub fn set_colors(&mut self, colors: &HashMap<String, Color>) -> Result<(), String> {
        for (role, color) in colors {
            match role.as_str() {
                "background" => self.palette.background = *color,
                "foreground" | "primary" | "body" => self.palette.foreground = *color,
                "muted" | "secondary" => self.palette.muted = *color,
                "title" => self.palette.title = *color,
                "accent" => self.palette.accent = *color,
                "chart" => self.palette.chart = *color,
                "panel" => self.palette.panel = *color,
                "header" => self.palette.header = *color,
                "rule" | "border" => self.palette.rule = *color,
                _ => return Err(format!("unknown theme color role '{role}'")),
            }
        }
        self.sync_text_colors();
        Ok(())
    }

    /// Resolve a semantic color token, including the public aliases accepted
    /// by `set_colors`.
    pub fn color(&self, role: &str) -> Result<Color, String> {
        match role {
            "background" => Ok(self.palette.background),
            "foreground" | "primary" | "body" => Ok(self.palette.foreground),
            "muted" | "secondary" => Ok(self.palette.muted),
            "title" => Ok(self.palette.title),
            "accent" => Ok(self.palette.accent),
            "chart" => Ok(self.palette.chart),
            "panel" => Ok(self.palette.panel),
            "header" => Ok(self.palette.header),
            "rule" | "border" => Ok(self.palette.rule),
            _ => Err(format!("unknown theme color role '{role}'")),
        }
    }

    /// Return actionable readability warnings without rejecting expressive
    /// themes. Empty means the core projected-text combinations pass.
    pub fn validate(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        check_contrast(
            &mut warnings,
            "foreground",
            self.palette.foreground,
            "background",
            self.palette.background,
            4.5,
        );
        check_contrast(
            &mut warnings,
            "title",
            self.palette.title,
            "background",
            self.palette.background,
            3.0,
        );
        check_contrast(
            &mut warnings,
            "muted",
            self.palette.muted,
            "background",
            self.palette.background,
            3.0,
        );
        check_contrast(
            &mut warnings,
            "foreground",
            self.palette.foreground,
            "panel",
            self.palette.panel,
            4.5,
        );
        for (role, style) in &self.text.roles {
            if !style.size.is_finite() || style.size <= 0.0 {
                warnings.push(format!("{role:?} size must be positive and finite"));
            }
            if style.font_family.trim().is_empty() {
                warnings.push(format!("{role:?} font family must not be empty"));
            }
        }
        warnings
    }

    pub fn set_fonts(&mut self, fonts: &HashMap<String, String>) -> Result<(), String> {
        for (role, family) in fonts {
            let roles: &[TextRole] = match role.as_str() {
                "text" => &[
                    TextRole::Title,
                    TextRole::Subtitle,
                    TextRole::Body,
                    TextRole::Caption,
                ],
                "all" => &[
                    TextRole::Title,
                    TextRole::Subtitle,
                    TextRole::Body,
                    TextRole::Caption,
                    TextRole::Math,
                    TextRole::Code,
                ],
                "title" => &[TextRole::Title],
                "subtitle" => &[TextRole::Subtitle],
                "body" => &[TextRole::Body],
                "caption" => &[TextRole::Caption],
                "math" => &[TextRole::Math],
                "code" => &[TextRole::Code],
                _ => return Err(format!("unknown theme font role '{role}'")),
            };
            for role in roles {
                self.text.roles.get_mut(role).unwrap().font_family = family.clone();
            }
        }
        Ok(())
    }

    pub fn set_sizes(&mut self, sizes: &HashMap<String, f64>) -> Result<(), String> {
        for (role, size) in sizes {
            if !size.is_finite() || *size <= 0.0 {
                return Err(format!("theme size '{role}' must be positive and finite"));
            }
            let role = text_role(role)?;
            self.text.roles.get_mut(&role).unwrap().size = *size;
        }
        Ok(())
    }
}

fn text_role(role: &str) -> Result<TextRole, String> {
    match role {
        "title" => Ok(TextRole::Title),
        "subtitle" => Ok(TextRole::Subtitle),
        "body" => Ok(TextRole::Body),
        "caption" => Ok(TextRole::Caption),
        "math" => Ok(TextRole::Math),
        "code" => Ok(TextRole::Code),
        _ => Err(format!("unknown theme text role '{role}'")),
    }
}

fn rgb(value: u32) -> Color {
    Color::from_rgb8(
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    )
}

fn check_contrast(
    warnings: &mut Vec<String>,
    foreground_name: &str,
    foreground: Color,
    background_name: &str,
    background: Color,
    minimum: f64,
) {
    let ratio = contrast_ratio(foreground, background);
    if ratio < minimum {
        warnings.push(format!(
            "{foreground_name} on {background_name} has {ratio:.2}:1 contrast; expected at least {minimum:.1}:1"
        ));
    }
}

fn contrast_ratio(a: Color, b: Color) -> f64 {
    let a = relative_luminance(a);
    let b = relative_luminance(b);
    let (lighter, darker) = if a >= b { (a, b) } else { (b, a) };
    (lighter + 0.05) / (darker + 0.05)
}

fn relative_luminance(color: Color) -> f64 {
    let rgba = color.to_rgba8();
    [rgba.r, rgba.g, rgba.b]
        .into_iter()
        .map(|channel| {
            let channel = f64::from(channel) / 255.0;
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        })
        .zip([0.2126, 0.7152, 0.0722])
        .map(|(channel, weight)| channel * weight)
        .sum()
}
