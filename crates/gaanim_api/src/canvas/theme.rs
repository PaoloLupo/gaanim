use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use gaanim_core::kurbo::{Cap, Join, Stroke};
use gaanim_core::peniko::{Brush, Color};
use gaanim_text::prelude::{TextConfig, TextRole, TextStyle};

use super::{ObjectSpec, SpawnKind, ThemeError};

/// A paint used by a theme rule. Named values resolve against the theme's
/// semantic/custom color tokens first and CSS Color 4 second.
#[derive(Debug, Clone, PartialEq)]
pub enum ThemePaint {
    Color(Color),
    Brush(Brush),
    Named(String),
}

impl From<Color> for ThemePaint {
    fn from(value: Color) -> Self {
        Self::Color(value)
    }
}

impl From<Brush> for ThemePaint {
    fn from(value: Brush) -> Self {
        Self::Brush(value)
    }
}

impl From<String> for ThemePaint {
    fn from(value: String) -> Self {
        Self::Named(value)
    }
}

impl From<&str> for ThemePaint {
    fn from(value: &str) -> Self {
        Self::Named(value.to_string())
    }
}

/// Complete stroke used by theme rules and individual drawable overrides.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeStrokeStyle {
    pub paint: ThemePaint,
    pub style: Stroke,
}

impl ThemeStrokeStyle {
    pub fn new(paint: impl Into<ThemePaint>, width: f64) -> Self {
        Self {
            paint: paint.into(),
            style: Stroke::new(width),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.style.width.is_finite() || self.style.width < 0.0 {
            return Err("stroke width must be finite and non-negative".into());
        }
        if !self.style.miter_limit.is_finite() || self.style.miter_limit <= 0.0 {
            return Err("stroke miter_limit must be finite and positive".into());
        }
        if !self.style.dash_offset.is_finite()
            || self
                .style
                .dash_pattern
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err("stroke dashes and dash_offset must be finite and non-negative".into());
        }
        Ok(())
    }

    pub fn with_cap(mut self, cap: Cap) -> Self {
        self.style.start_cap = cap;
        self.style.end_cap = cap;
        self
    }

    pub fn with_join(mut self, join: Join) -> Self {
        self.style.join = join;
        self
    }
}

/// Property-wise visual overlay associated with a theme selector.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThemeStyle {
    pub fill: Option<ThemePaint>,
    pub stroke: Option<ThemeStrokeStyle>,
    pub opacity: Option<f32>,
    pub text: Option<TextStyle>,
}

impl ThemeStyle {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .opacity
            .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err("style opacity must be finite and between 0 and 1".into());
        }
        if let Some(stroke) = &self.stroke {
            stroke.validate()?;
        }
        Ok(())
    }

    fn overlay(&mut self, other: &Self) {
        if other.fill.is_some() {
            self.fill = other.fill.clone();
        }
        if other.stroke.is_some() {
            self.stroke = other.stroke.clone();
        }
        if other.opacity.is_some() {
            self.opacity = other.opacity;
        }
        if let Some(text) = &other.text {
            self.text = Some(match &self.text {
                Some(base) => merge_text_style(base, text),
                None => text.clone(),
            });
        }
    }
}

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

/// Named spacing and layout values consumed by reusable templates.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutTokens {
    values: HashMap<String, f64>,
}

impl Default for LayoutTokens {
    fn default() -> Self {
        Self {
            values: HashMap::from([
                ("space_xs".into(), 8.0),
                ("space_sm".into(), 16.0),
                ("space_md".into(), 24.0),
                ("space_lg".into(), 32.0),
                ("page_padding".into(), 48.0),
                ("page_padding_wide".into(), 72.0),
                ("page_padding_x".into(), 64.0),
                ("column_gap".into(), 40.0),
                ("vertical_padding".into(), 96.0),
                ("vertical_padding_x".into(), 56.0),
                ("lower_third_offset".into(), 240.0),
            ]),
        }
    }
}

impl LayoutTokens {
    pub fn get(&self, name: &str) -> Result<f64, String> {
        self.values
            .get(name)
            .copied()
            .ok_or_else(|| format!("unknown layout token '{name}'"))
    }

    pub fn set(&mut self, values: &HashMap<String, f64>) -> Result<(), String> {
        for (name, value) in values {
            if name.trim().is_empty() || !value.is_finite() || *value < 0.0 {
                return Err(format!(
                    "layout token '{name}' must have a non-empty name and finite non-negative value"
                ));
            }
            self.values.insert(name.clone(), *value);
        }
        Ok(())
    }
}

/// Complete visual theme: semantic colors, typography, and optional font files.
#[derive(Debug, Clone)]
pub struct CanvasTheme {
    pub name: String,
    pub palette: ThemePalette,
    pub text: TextConfig,
    /// Advanced structured-text overlays by semantic role.
    pub text_styles: HashMap<TextRole, TextStyle>,
    /// Semantic and user-defined color tokens.
    pub colors: HashMap<String, Color>,
    /// Family, exact-type, part, and class selector rules.
    pub styles: HashMap<String, ThemeStyle>,
    /// Ordered categorical colors used by charts.
    pub series: Vec<Color>,
    /// Ordered continuous colors used by heatmaps.
    pub heatmap: Vec<Color>,
    pub fonts: Vec<ThemeFont>,
    pub layout: LayoutTokens,
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
            "deck" => "presentation",
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
            text_styles: HashMap::new(),
            colors: HashMap::new(),
            styles: HashMap::new(),
            series: vec![palette.chart, palette.accent, palette.title, palette.muted],
            heatmap: vec![palette.background, palette.chart, palette.accent],
            fonts: Vec::new(),
            layout: LayoutTokens::default(),
        };
        result.sync_text_colors();
        result.sync_color_tokens();
        result.install_default_rules();
        result
    }

    fn install_default_rules(&mut self) {
        self.styles.insert(
            "shape".into(),
            ThemeStyle {
                fill: Some(ThemePaint::Named("accent".into())),
                ..Default::default()
            },
        );
        self.styles.insert(
            "line".into(),
            ThemeStyle {
                stroke: Some(ThemeStrokeStyle::new("foreground", 3.0)),
                ..Default::default()
            },
        );
        self.styles.insert(
            "plot".into(),
            ThemeStyle {
                stroke: Some(ThemeStrokeStyle::new("chart", 3.0)),
                ..Default::default()
            },
        );
        self.styles.insert(
            "expression_readout".into(),
            ThemeStyle {
                fill: Some(ThemePaint::Named("foreground".into())),
                ..Default::default()
            },
        );
        for (part, token, width) in [
            ("axis", "foreground", 3.0),
            ("grid", "rule", 1.0),
            ("minor_grid", "rule", 0.6),
            ("ticks", "foreground", 2.0),
        ] {
            self.styles.insert(
                format!("axes/{part}"),
                ThemeStyle {
                    stroke: Some(ThemeStrokeStyle::new(token, width)),
                    ..Default::default()
                },
            );
        }
        self.styles.insert(
            "axes/numbers".into(),
            ThemeStyle {
                fill: Some(ThemePaint::Named("foreground".into())),
                text: Some(TextStyle {
                    // Tick numbers must remain readable after a 1080p frame is
                    // scaled down inside a player or presentation viewport.
                    size: Some(self.text.roles[&TextRole::Caption].size),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        self.styles.insert(
            "axes/labels".into(),
            ThemeStyle {
                fill: Some(ThemePaint::Named("foreground".into())),
                text: Some(TextStyle {
                    size: Some(self.text.roles[&TextRole::Label].size),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
    }

    fn sync_color_tokens(&mut self) {
        self.colors
            .insert("background".into(), self.palette.background);
        self.colors
            .insert("foreground".into(), self.palette.foreground);
        self.colors
            .insert("primary".into(), self.palette.foreground);
        self.colors.insert("body".into(), self.palette.foreground);
        self.colors.insert("muted".into(), self.palette.muted);
        self.colors.insert("secondary".into(), self.palette.muted);
        self.colors.insert("title".into(), self.palette.title);
        self.colors.insert("accent".into(), self.palette.accent);
        self.colors.insert("chart".into(), self.palette.chart);
        self.colors.insert("panel".into(), self.palette.panel);
        self.colors.insert("header".into(), self.palette.header);
        self.colors.insert("rule".into(), self.palette.rule);
        self.colors.insert("border".into(), self.palette.rule);
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
            .get_mut(&TextRole::Kicker)
            .unwrap()
            .fill_color = self.palette.accent;
        self.text
            .roles
            .get_mut(&TextRole::Heading)
            .unwrap()
            .fill_color = self.palette.title;
        self.text
            .roles
            .get_mut(&TextRole::Caption)
            .unwrap()
            .fill_color = self.palette.muted;
        for role in [
            TextRole::Body,
            TextRole::Label,
            TextRole::Math,
            TextRole::Code,
        ] {
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
                _ if role.trim().is_empty() => {
                    return Err("theme color token names must not be empty".into());
                }
                _ => {
                    self.colors.insert(role.clone(), *color);
                }
            }
        }
        self.sync_text_colors();
        self.sync_color_tokens();
        Ok(())
    }

    /// Resolve a semantic color token, including the public aliases accepted
    /// by `set_colors`.
    pub fn color(&self, role: &str) -> Result<Color, String> {
        self.colors
            .get(role)
            .copied()
            .ok_or_else(|| format!("unknown theme color token '{role}'"))
    }

    pub fn set_text_styles(&mut self, styles: &HashMap<TextRole, TextStyle>) -> Result<(), String> {
        for (role, style) in styles {
            gaanim_text::prelude::TextSpec::new(
                vec!["x".into()],
                Some(*role),
                style.clone(),
                gaanim_text::prelude::TextFlow::default(),
            )
            .map_err(|error| error.to_string())?;
            self.text_styles.insert(*role, style.clone());
        }
        Ok(())
    }

    pub fn set_styles(&mut self, styles: &HashMap<String, ThemeStyle>) -> Result<(), String> {
        for (selector, style) in styles {
            validate_selector(selector)?;
            style.validate()?;
            if let Some(fill) = &style.fill {
                self.resolve_paint(fill).map_err(|_| {
                    format!("theme selector '{selector}' references an unknown fill token or color")
                })?;
            }
            if let Some(stroke) = &style.stroke {
                self.resolve_paint(&stroke.paint).map_err(|_| {
                    format!(
                        "theme selector '{selector}' references an unknown stroke token or color"
                    )
                })?;
            }
            self.styles.insert(selector.clone(), style.clone());
        }
        Ok(())
    }

    pub(crate) fn resolve_paint(&self, paint: &ThemePaint) -> Result<Brush, String> {
        match paint {
            ThemePaint::Color(color) => Ok(Brush::Solid(*color)),
            ThemePaint::Brush(brush) => Ok(brush.clone()),
            ThemePaint::Named(name) => self.color(name).map(Brush::Solid).or_else(|_| {
                Color::from_str(name)
                    .map(Brush::Solid)
                    .map_err(|error| error.to_string())
            }),
        }
    }

    /// Materialize the active cascade without mutating the authored spec.
    pub fn resolve_object(&self, spec: &ObjectSpec) -> Result<ObjectSpec, String> {
        let mut resolved = spec.clone();
        let mut style = ThemeStyle::default();
        if let Some(rule) = self.styles.get(spawn_family(&spec.kind)) {
            style.overlay(rule);
        }
        if let Some(rule) = self.styles.get(spawn_name(&spec.kind)) {
            style.overlay(rule);
        }
        if let Some(rule) = spec
            .theme_selector
            .as_deref()
            .and_then(|selector| self.styles.get(selector))
        {
            style.overlay(rule);
        }
        for class in &spec.style_classes {
            if let Some(rule) = self.styles.get(&format!(".{class}")) {
                style.overlay(rule);
            }
        }
        if !spec.fill_overridden
            && let Some(fill) = &style.fill
        {
            resolved.fill = Some(self.resolve_paint(fill)?);
            resolved.fill_overridden = true;
        }
        if !spec.stroke_overridden
            && let Some(stroke) = &style.stroke
        {
            resolved.stroke = Some((self.resolve_paint(&stroke.paint)?, stroke.style.width));
            resolved.stroke_style = Some(stroke.style.clone());
            resolved.stroke_overridden = true;
        }
        if !spec.opacity_overridden
            && let Some(opacity) = style.opacity
        {
            resolved.opacity = opacity;
        }
        if let SpawnKind::Text(text) = &mut resolved.kind {
            let mut merged = self
                .text_styles
                .get(&text.role)
                .cloned()
                .unwrap_or_default();
            if let Some(selector_text) = style.text {
                merged = merge_text_style(&merged, &selector_text);
            }
            text.style = merge_text_style(&merged, &text.style);
        }
        if let SpawnKind::Axes { config, .. } = &mut resolved.kind {
            for (part, color, width) in [
                ("axis", &mut config.axis_color, &mut config.axis_width),
                ("grid", &mut config.grid_color, &mut config.grid_width),
                ("ticks", &mut config.tick_color, &mut config.tick_width),
            ] {
                if let Some(stroke) = self
                    .styles
                    .get(&format!("axes/{part}"))
                    .and_then(|style| style.stroke.as_ref())
                {
                    *color = match self.resolve_paint(&stroke.paint)? {
                        Brush::Solid(color) => color,
                        _ => return Err(format!("axes/{part} requires a solid stroke color")),
                    };
                    *width = stroke.style.width;
                }
            }
            for (part, color, size) in [
                ("numbers", &mut config.number_color, &mut config.number_size),
                ("labels", &mut config.label_color, &mut config.label_size),
            ] {
                if let Some(rule) = self.styles.get(&format!("axes/{part}")) {
                    if let Some(fill) = &rule.fill {
                        *color = match self.resolve_paint(fill)? {
                            Brush::Solid(color) => color,
                            _ => return Err(format!("axes/{part} requires a solid text color")),
                        };
                    }
                    if let Some(text) = &rule.text {
                        if let Some(text_color) = text.color {
                            *color = text_color;
                        }
                        if text.size.is_some() {
                            *size = text.size;
                        }
                    }
                }
            }
        }
        Ok(resolved)
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
                    TextRole::Kicker,
                    TextRole::Heading,
                    TextRole::Body,
                    TextRole::Caption,
                    TextRole::Label,
                ],
                "all" => &[
                    TextRole::Title,
                    TextRole::Subtitle,
                    TextRole::Kicker,
                    TextRole::Heading,
                    TextRole::Body,
                    TextRole::Caption,
                    TextRole::Label,
                    TextRole::Math,
                    TextRole::Code,
                ],
                "title" => &[TextRole::Title],
                "subtitle" => &[TextRole::Subtitle],
                "kicker" => &[TextRole::Kicker],
                "heading" => &[TextRole::Heading],
                "body" => &[TextRole::Body],
                "caption" => &[TextRole::Caption],
                "label" => &[TextRole::Label],
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
        "kicker" => Ok(TextRole::Kicker),
        "heading" => Ok(TextRole::Heading),
        "body" => Ok(TextRole::Body),
        "caption" => Ok(TextRole::Caption),
        "label" => Ok(TextRole::Label),
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

fn validate_selector(selector: &str) -> Result<(), String> {
    if selector.is_empty()
        || selector.split('/').any(|part| part.is_empty())
        || selector.chars().any(|ch| {
            !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-' | '/' | '.'))
        })
        || (selector.starts_with('.') && selector.len() == 1)
    {
        return Err(format!(
            "invalid theme selector '{selector}'; use snake_case families/types, part paths, or .classes"
        ));
    }
    Ok(())
}

fn merge_text_style(base: &TextStyle, overlay: &TextStyle) -> TextStyle {
    TextStyle {
        font: overlay.font.clone().or_else(|| base.font.clone()),
        math_font: overlay.math_font.clone().or_else(|| base.math_font.clone()),
        fallbacks: if overlay.fallbacks.is_empty() {
            base.fallbacks.clone()
        } else {
            overlay.fallbacks.clone()
        },
        size: overlay.size.or(base.size),
        weight: overlay.weight.or(base.weight),
        italic: overlay.italic.or(base.italic),
        color: overlay.color.or(base.color),
        stroke_color: overlay.stroke_color.or(base.stroke_color),
        stroke_width: overlay.stroke_width.or(base.stroke_width),
        opacity: overlay.opacity.or(base.opacity),
        letter_spacing: overlay.letter_spacing.or(base.letter_spacing),
        word_spacing: overlay.word_spacing.or(base.word_spacing),
        decorations: if overlay.decorations.is_empty() {
            base.decorations.clone()
        } else {
            overlay.decorations.clone()
        },
        baseline: overlay.baseline.or(base.baseline),
    }
}

fn spawn_family(kind: &SpawnKind) -> &'static str {
    match kind {
        SpawnKind::Circle(_)
        | SpawnKind::Rect(_, _)
        | SpawnKind::RoundedRect(_, _, _)
        | SpawnKind::Square(_)
        | SpawnKind::Dot(_)
        | SpawnKind::Ellipse(_, _)
        | SpawnKind::Polygon(_)
        | SpawnKind::Star { .. }
        | SpawnKind::RegularPolygon { .. }
        | SpawnKind::Sector { .. }
        | SpawnKind::Annulus { .. }
        | SpawnKind::Checkmark(_)
        | SpawnKind::Cross(_)
        | SpawnKind::RightAngle(_) => "shape",
        SpawnKind::Line(_, _, _, _)
        | SpawnKind::Arrow(_, _, _, _)
        | SpawnKind::DashedLine { .. }
        | SpawnKind::DoubleArrow { .. }
        | SpawnKind::Brace { .. }
        | SpawnKind::Arc { .. }
        | SpawnKind::CurvedArrow(_, _, _, _, _)
        | SpawnKind::CurvedArrowArc { .. }
        | SpawnKind::Dimension { .. }
        | SpawnKind::Polyline(_)
        | SpawnKind::Bezier { .. }
        | SpawnKind::Curve(_)
        | SpawnKind::TracedPathLine
        | SpawnKind::TrackingLine => "line",
        SpawnKind::Text(_) | SpawnKind::Typst { .. } | SpawnKind::ExpressionReadout { .. } => {
            "text"
        }
        SpawnKind::Axes { .. } | SpawnKind::Axes3D { .. } => "axes",
        SpawnKind::ExpressionPlot { .. }
        | SpawnKind::DataMark { .. }
        | SpawnKind::Polyline3D { .. }
        | SpawnKind::LineSegments3D { .. }
        | SpawnKind::TracedPath3DLine => "plot",
        _ => "component",
    }
}

fn spawn_name(kind: &SpawnKind) -> &'static str {
    match kind {
        SpawnKind::Circle(_) => "circle",
        SpawnKind::Rect(_, _) => "rect",
        SpawnKind::RoundedRect(_, _, _) => "rounded_rect",
        SpawnKind::Square(_) => "square",
        SpawnKind::Dot(_) => "dot",
        SpawnKind::Ellipse(_, _) => "ellipse",
        SpawnKind::Line(_, _, _, _) => "line",
        SpawnKind::Arrow(_, _, _, _) => "arrow",
        SpawnKind::DashedLine { .. } => "dashed_line",
        SpawnKind::DoubleArrow { .. } => "double_arrow",
        SpawnKind::Polygon(_) => "polygon",
        SpawnKind::Star { .. } => "star",
        SpawnKind::RegularPolygon { .. } => "regular_polygon",
        SpawnKind::Sector { .. } => "sector",
        SpawnKind::Annulus { .. } => "annulus",
        SpawnKind::Brace { .. } => "brace",
        SpawnKind::Checkmark(_) => "checkmark",
        SpawnKind::Cross(_) => "cross",
        SpawnKind::RightAngle(_) => "right_angle",
        SpawnKind::Arc { .. } => "arc",
        SpawnKind::CurvedArrow(_, _, _, _, _) | SpawnKind::CurvedArrowArc { .. } => "curved_arrow",
        SpawnKind::Dimension { .. } => "dimension",
        SpawnKind::Polyline(_) => "polyline",
        SpawnKind::Bezier { .. } => "bezier",
        SpawnKind::Curve(_) => "curve",
        SpawnKind::ExpressionPlot { .. } => "expression_plot",
        SpawnKind::ExpressionReadout { .. } => "expression_readout",
        SpawnKind::DataMark { .. } => "data_mark",
        SpawnKind::Axes { .. } => "axes",
        SpawnKind::Axes3D { .. } => "axes_3d",
        SpawnKind::SurfaceMesh { .. } => "surface",
        SpawnKind::Primitive3D(_) => "primitive_3d",
        SpawnKind::Polyline3D { .. } => "polyline_3d",
        SpawnKind::LineSegments3D { .. } => "line_segments_3d",
        SpawnKind::GltfNode { .. } => "gltf_node",
        SpawnKind::GltfModel { .. } => "gltf_model",
        SpawnKind::Text(_) => "text",
        SpawnKind::Typst { .. } => "typst",
        SpawnKind::Image { .. } => "image",
        SpawnKind::SvgPath(_) => "svg_path",
        SpawnKind::Group(_) | SpawnKind::GroupNoCenter(_) => "group",
        SpawnKind::ValueTracker(_) => "value_tracker",
        SpawnKind::TracedPathLine => "traced_path",
        SpawnKind::TrackingLine => "tracking_line",
        SpawnKind::TracedPath3DLine => "traced_path_3d",
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_remains_generic_but_thesis_alias_is_removed() {
        assert_eq!(CanvasTheme::builtin("deck").unwrap().name, "presentation");
        assert!(CanvasTheme::builtin("thesis").is_err());
    }

    #[test]
    fn layout_tokens_are_inherited_and_overridable() {
        let mut theme = CanvasTheme::builtin("presentation").unwrap();
        assert_eq!(theme.layout.get("page_padding").unwrap(), 48.0);
        theme
            .layout
            .set(&HashMap::from([
                ("page_padding".to_string(), 60.0),
                ("brand_rhythm".to_string(), 18.0),
            ]))
            .unwrap();
        assert_eq!(theme.layout.get("page_padding").unwrap(), 60.0);
        assert_eq!(theme.layout.get("brand_rhythm").unwrap(), 18.0);
        assert!(
            theme
                .layout
                .set(&HashMap::from([("bad".to_string(), f64::NAN)]))
                .is_err()
        );
    }

    #[test]
    fn custom_tokens_and_ordered_classes_resolve_without_overwriting_fluent_style() {
        let mut theme = CanvasTheme::builtin("paper").unwrap();
        let brand = rgb(0x123456);
        let warning = rgb(0xDC2626);
        theme
            .set_colors(&HashMap::from([
                ("brand".into(), brand),
                ("warning".into(), warning),
            ]))
            .unwrap();
        theme
            .set_styles(&HashMap::from([
                (
                    "shape".into(),
                    ThemeStyle {
                        fill: Some(ThemePaint::Named("brand".into())),
                        ..Default::default()
                    },
                ),
                (
                    ".warning".into(),
                    ThemeStyle {
                        fill: Some(ThemePaint::Named("warning".into())),
                        ..Default::default()
                    },
                ),
            ]))
            .unwrap();

        let mut spec = ObjectSpec::new(
            gaanim_core::ObjectId::from_parts(1, 1),
            SpawnKind::Circle(10.0),
        );
        spec.style_classes.push("warning".into());
        let resolved = theme.resolve_object(&spec).unwrap();
        assert_eq!(resolved.fill, Some(Brush::Solid(warning)));

        spec.fill = Some(Brush::Solid(Color::BLACK));
        spec.fill_overridden = true;
        let resolved = theme.resolve_object(&spec).unwrap();
        assert_eq!(resolved.fill, Some(Brush::Solid(Color::BLACK)));
    }

    #[test]
    fn structured_text_overlay_merges_property_by_property() {
        let mut theme = CanvasTheme::builtin("paper").unwrap();
        theme
            .set_text_styles(&HashMap::from([(
                TextRole::Body,
                TextStyle {
                    font: Some("Inter".into()),
                    size: Some(30.0),
                    weight: Some(500),
                    ..Default::default()
                },
            )]))
            .unwrap();
        let text = gaanim_text::prelude::TextSpec::new(
            vec!["content".into()],
            Some(TextRole::Body),
            TextStyle {
                size: Some(42.0),
                italic: Some(true),
                ..Default::default()
            },
            gaanim_text::prelude::TextFlow::default(),
        )
        .unwrap();
        let spec = ObjectSpec::new(
            gaanim_core::ObjectId::from_parts(2, 1),
            SpawnKind::Text(text),
        );
        let resolved = theme.resolve_object(&spec).unwrap();
        let SpawnKind::Text(text) = resolved.kind else {
            panic!("expected text")
        };
        assert_eq!(text.style.font.as_deref(), Some("Inter"));
        assert_eq!(text.style.size, Some(42.0));
        assert_eq!(text.style.weight, Some(500));
        assert_eq!(text.style.italic, Some(true));
    }

    #[test]
    fn axes_part_rules_control_strokes_and_label_metrics() {
        let mut theme = CanvasTheme::builtin("nord").unwrap();
        theme
            .set_styles(&HashMap::from([(
                "axes/labels".into(),
                ThemeStyle {
                    fill: Some(ThemePaint::Named("accent".into())),
                    text: Some(TextStyle {
                        size: Some(36.0),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )]))
            .unwrap();
        let spec = ObjectSpec::new(
            gaanim_core::ObjectId::from_parts(3, 1),
            SpawnKind::Axes {
                x_range: (-1.0, 1.0, 1.0),
                y_range: (-1.0, 1.0, 1.0),
                config: crate::canvas::AxesConfig::default(),
            },
        );
        let resolved = theme.resolve_object(&spec).unwrap();
        let SpawnKind::Axes { config, .. } = resolved.kind else {
            panic!("expected axes")
        };
        assert_eq!(config.axis_color, theme.palette.foreground);
        assert_eq!(config.grid_color, theme.palette.rule);
        assert_eq!(config.label_color, theme.palette.accent);
        assert_eq!(config.label_size, Some(36.0));
    }

    #[test]
    fn builtin_axis_typography_is_readable_at_1080p() {
        let theme = CanvasTheme::builtin("nord").unwrap();
        let number_size = theme.styles["axes/numbers"]
            .text
            .as_ref()
            .and_then(|style| style.size)
            .unwrap();
        let label_size = theme.styles["axes/labels"]
            .text
            .as_ref()
            .and_then(|style| style.size)
            .unwrap();

        assert_eq!(number_size, 32.0);
        assert_eq!(label_size, 36.0);
    }
}
