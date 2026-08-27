use gaanim_core::peniko::Color;
use gaanim_text::prelude::{TextAlign, TextFlow, TextRole, TextSpec, TextStyle, TextWrap};

use super::{Anchor, Direction, DrawableHandle, SceneModel};

const SUCCESS: Color = Color::from_rgb8(0x22, 0xC5, 0x5E);
const WARNING: Color = Color::from_rgb8(0xF5, 0x9E, 0x0B);
const DANGER: Color = Color::from_rgb8(0xEF, 0x44, 0x44);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorialVariant {
    #[default]
    Neutral,
    Accent,
    Success,
    Warning,
    Danger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorialAppearance {
    #[default]
    Soft,
    Solid,
    Outline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BannerPosition {
    #[default]
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LowerThirdSide {
    #[default]
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorialAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl EditorialAlign {
    fn text_align(self) -> TextAlign {
        match self {
            Self::Left => TextAlign::Left,
            Self::Center => TextAlign::Center,
            Self::Right => TextAlign::Right,
        }
    }

    fn anchor(self) -> Anchor {
        match self {
            Self::Left => Anchor::Left,
            Self::Center => Anchor::Center,
            Self::Right => Anchor::Right,
        }
    }

    fn x(self, width: f64, padding: f64) -> f64 {
        match self {
            Self::Left => -width * 0.5 + padding,
            Self::Center => 0.0,
            Self::Right => width * 0.5 - padding,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EditorialStyle {
    pub variant: EditorialVariant,
    pub appearance: EditorialAppearance,
    pub color: Option<Color>,
    pub background: Option<Color>,
    pub border: Option<Color>,
}

impl EditorialStyle {
    pub fn variant(mut self, variant: EditorialVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn appearance(mut self, appearance: EditorialAppearance) -> Self {
        self.appearance = appearance;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn border(mut self, color: Color) -> Self {
        self.border = Some(color);
        self
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum EditorialError {
    #[error("{field} must not be empty")]
    EmptyText { field: &'static str },
    #[error("{field} must be a finite positive number")]
    PositiveNumber { field: &'static str },
    #[error("{field} must be a finite non-negative number")]
    NonNegativeNumber { field: &'static str },
    #[error("width must be greater than twice the horizontal padding")]
    ContentWidth,
    #[error("could not measure editorial text: {0}")]
    Text(String),
}

#[derive(Debug, Clone)]
pub struct BadgeSpec {
    pub text: String,
    pub padding: (f64, f64),
    pub radius: Option<f64>,
    pub font_size: Option<f64>,
    pub min_width: Option<f64>,
    pub style: EditorialStyle,
}

impl BadgeSpec {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            padding: (18.0, 10.0),
            radius: None,
            font_size: None,
            min_width: None,
            style: EditorialStyle::default(),
        }
    }

    pub fn style(mut self, style: EditorialStyle) -> Self {
        self.style = style;
        self
    }
}

#[derive(Debug, Clone)]
pub struct ChipSpec {
    pub text: String,
    pub dot: bool,
    pub padding: (f64, f64),
    pub radius: Option<f64>,
    pub font_size: Option<f64>,
    pub style: EditorialStyle,
}

impl ChipSpec {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            dot: true,
            padding: (14.0, 8.0),
            radius: None,
            font_size: None,
            style: EditorialStyle::default(),
        }
    }

    pub fn style(mut self, style: EditorialStyle) -> Self {
        self.style = style;
        self
    }
}

#[derive(Debug, Clone)]
pub struct CardSpec {
    pub title: String,
    pub body: Option<String>,
    pub footer: Option<String>,
    pub width: f64,
    pub min_height: f64,
    pub padding: (f64, f64),
    pub gap: f64,
    pub radius: f64,
    pub style: EditorialStyle,
}

impl CardSpec {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: None,
            footer: None,
            width: 420.0,
            min_height: 180.0,
            padding: (28.0, 24.0),
            gap: 14.0,
            radius: 18.0,
            style: EditorialStyle::default(),
        }
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn footer(mut self, footer: impl Into<String>) -> Self {
        self.footer = Some(footer.into());
        self
    }

    pub fn style(mut self, style: EditorialStyle) -> Self {
        self.style = style;
        self
    }
}

#[derive(Debug, Clone)]
pub struct BannerSpec {
    pub title: String,
    pub subtitle: Option<String>,
    pub position: BannerPosition,
    pub width: Option<f64>,
    pub margin: f64,
    pub padding: (f64, f64),
    pub gap: f64,
    pub radius: f64,
    pub style: EditorialStyle,
}

impl BannerSpec {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            position: BannerPosition::Top,
            width: None,
            margin: 32.0,
            padding: (28.0, 18.0),
            gap: 8.0,
            radius: 14.0,
            style: EditorialStyle::default(),
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn style(mut self, style: EditorialStyle) -> Self {
        self.style = style;
        self
    }
}

#[derive(Debug, Clone)]
pub struct LowerThirdSpec {
    pub title: String,
    pub subtitle: Option<String>,
    pub kicker: Option<String>,
    pub side: LowerThirdSide,
    pub width: f64,
    pub margin: f64,
    pub padding: (f64, f64),
    pub gap: f64,
    pub radius: f64,
    pub style: EditorialStyle,
}

impl LowerThirdSpec {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            kicker: None,
            side: LowerThirdSide::Left,
            width: 520.0,
            margin: 32.0,
            padding: (28.0, 20.0),
            gap: 8.0,
            radius: 16.0,
            style: EditorialStyle::default(),
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn kicker(mut self, kicker: impl Into<String>) -> Self {
        self.kicker = Some(kicker.into());
        self
    }

    pub fn style(mut self, style: EditorialStyle) -> Self {
        self.style = style;
        self
    }
}

#[derive(Debug, Clone)]
pub struct StatCardSpec {
    pub value: String,
    pub label: String,
    pub delta: Option<String>,
    pub width: f64,
    pub min_height: f64,
    pub padding: (f64, f64),
    pub gap: f64,
    pub radius: f64,
    pub style: EditorialStyle,
}

impl StatCardSpec {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            delta: None,
            width: 280.0,
            min_height: 170.0,
            padding: (24.0, 20.0),
            gap: 8.0,
            radius: 18.0,
            style: EditorialStyle::default(),
        }
    }

    pub fn delta(mut self, delta: impl Into<String>) -> Self {
        self.delta = Some(delta.into());
        self
    }

    pub fn style(mut self, style: EditorialStyle) -> Self {
        self.style = style;
        self
    }
}

#[derive(Debug, Clone)]
pub struct QuoteCardSpec {
    pub quote: String,
    pub attribution: Option<String>,
    pub width: f64,
    pub padding: (f64, f64),
    pub gap: f64,
    pub radius: f64,
    pub style: EditorialStyle,
}

impl QuoteCardSpec {
    pub fn new(quote: impl Into<String>) -> Self {
        Self {
            quote: quote.into(),
            attribution: None,
            width: 620.0,
            padding: (32.0, 28.0),
            gap: 16.0,
            radius: 18.0,
            style: EditorialStyle::default(),
        }
    }

    pub fn attribution(mut self, attribution: impl Into<String>) -> Self {
        self.attribution = Some(attribution.into());
        self
    }

    pub fn style(mut self, style: EditorialStyle) -> Self {
        self.style = style;
        self
    }
}

#[derive(Debug, Clone)]
pub struct SectionHeaderSpec {
    pub title: String,
    pub kicker: Option<String>,
    pub subtitle: Option<String>,
    pub width: f64,
    pub align: EditorialAlign,
    pub rule: bool,
    pub padding: (f64, f64),
    pub gap: f64,
    pub radius: f64,
    pub style: EditorialStyle,
}

impl SectionHeaderSpec {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            kicker: None,
            subtitle: None,
            width: 720.0,
            align: EditorialAlign::Left,
            rule: false,
            padding: (24.0, 18.0),
            gap: 10.0,
            radius: 12.0,
            style: EditorialStyle::default(),
        }
    }

    pub fn kicker(mut self, kicker: impl Into<String>) -> Self {
        self.kicker = Some(kicker.into());
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn style(mut self, style: EditorialStyle) -> Self {
        self.style = style;
        self
    }
}

#[derive(Clone, Copy)]
struct ResolvedEditorialStyle {
    text: Color,
    compact_text: Color,
    background: Color,
    border: Color,
    appearance: EditorialAppearance,
}

#[derive(Clone)]
struct TextBlock {
    content: String,
    role: TextRole,
    size: Option<f64>,
    color: Color,
    height: f64,
    wrap: Option<f64>,
    align: EditorialAlign,
}

impl SceneModel {
    pub fn badge(&mut self, spec: BadgeSpec) -> Result<DrawableHandle, EditorialError> {
        validate_text("text", &spec.text)?;
        validate_padding(spec.padding)?;
        validate_optional_non_negative("radius", spec.radius)?;
        validate_optional_positive("font_size", spec.font_size)?;
        validate_optional_non_negative("min_width", spec.min_width)?;
        let style = self.resolve_editorial_style(spec.style);
        let (label_width, label_height) = self.measure_editorial_text(
            &spec.text,
            TextRole::Label,
            spec.font_size,
            style.compact_text,
            None,
        )?;
        let width = (label_width + spec.padding.0 * 2.0).max(spec.min_width.unwrap_or(0.0));
        let height = label_height + spec.padding.1 * 2.0;
        let radius = spec.radius.unwrap_or(height * 0.5);
        let panel = self.editorial_panel(width, height, radius, style);
        let label = self
            .editorial_text(
                &spec.text,
                TextRole::Label,
                spec.font_size,
                style.compact_text,
                None,
                EditorialAlign::Center,
            )?
            .at_anchor(0.0, 0.0, Anchor::Center);
        Ok(self.group(&[&panel, &label]))
    }

    pub fn chip(&mut self, spec: ChipSpec) -> Result<DrawableHandle, EditorialError> {
        validate_text("text", &spec.text)?;
        validate_padding(spec.padding)?;
        validate_optional_non_negative("radius", spec.radius)?;
        validate_optional_positive("font_size", spec.font_size)?;
        let style = self.resolve_editorial_style(spec.style);
        let (label_width, label_height) = self.measure_editorial_text(
            &spec.text,
            TextRole::Label,
            spec.font_size,
            style.compact_text,
            None,
        )?;
        let dot_diameter = if spec.dot { 10.0 } else { 0.0 };
        let dot_gap = if spec.dot { 8.0 } else { 0.0 };
        let content_width = label_width + dot_diameter + dot_gap;
        let width = content_width + spec.padding.0 * 2.0;
        let height = label_height.max(dot_diameter) + spec.padding.1 * 2.0;
        let radius = spec.radius.unwrap_or(height * 0.5);
        let panel = self.editorial_panel(width, height, radius, style);
        let start_x = -content_width * 0.5;
        let label_x = start_x + dot_diameter + dot_gap + label_width * 0.5;
        let label = self
            .editorial_text(
                &spec.text,
                TextRole::Label,
                spec.font_size,
                style.compact_text,
                None,
                EditorialAlign::Center,
            )?
            .at_anchor(label_x, 0.0, Anchor::Center);
        let mut members = vec![panel];
        if spec.dot {
            members.push(
                self.dot(dot_diameter * 0.5)
                    .fill(style.compact_text)
                    .no_stroke()
                    .move_to(start_x + dot_diameter * 0.5, 0.0),
            );
        }
        members.push(label);
        let refs: Vec<&DrawableHandle> = members.iter().collect();
        Ok(self.group(&refs))
    }

    pub fn card(&mut self, spec: CardSpec) -> Result<DrawableHandle, EditorialError> {
        validate_text("title", &spec.title)?;
        validate_optional_text("body", &spec.body)?;
        validate_optional_text("footer", &spec.footer)?;
        validate_box_metrics(
            spec.width,
            spec.min_height,
            spec.padding,
            spec.gap,
            spec.radius,
        )?;
        let style = self.resolve_editorial_style(spec.style);
        let inner = content_width(spec.width, spec.padding.0)?;
        let mut blocks = vec![self.editorial_block(
            &spec.title,
            TextRole::Heading,
            None,
            style.text,
            Some(inner),
            EditorialAlign::Left,
        )?];
        if let Some(body) = &spec.body {
            blocks.push(self.editorial_block(
                body,
                TextRole::Body,
                None,
                style.text,
                Some(inner),
                EditorialAlign::Left,
            )?);
        }
        if let Some(footer) = &spec.footer {
            blocks.push(self.editorial_block(
                footer,
                TextRole::Caption,
                None,
                style.text,
                Some(inner),
                EditorialAlign::Left,
            )?);
        }
        self.editorial_container(
            spec.width,
            spec.min_height,
            spec.padding,
            spec.gap,
            spec.radius,
            style,
            blocks,
        )
    }

    pub fn banner(&mut self, spec: BannerSpec) -> Result<DrawableHandle, EditorialError> {
        validate_text("title", &spec.title)?;
        validate_optional_text("subtitle", &spec.subtitle)?;
        validate_non_negative("margin", spec.margin)?;
        validate_padding(spec.padding)?;
        validate_non_negative("gap", spec.gap)?;
        validate_non_negative("radius", spec.radius)?;
        let safe_width = self.safe_frame().width() - spec.margin * 2.0;
        let width = spec.width.unwrap_or(safe_width);
        validate_positive("width", width)?;
        let inner = content_width(width, spec.padding.0)?;
        let style = self.resolve_editorial_style(spec.style);
        let mut blocks = vec![self.editorial_block(
            &spec.title,
            TextRole::Heading,
            None,
            style.text,
            Some(inner),
            EditorialAlign::Center,
        )?];
        if let Some(subtitle) = &spec.subtitle {
            blocks.push(self.editorial_block(
                subtitle,
                TextRole::Subtitle,
                None,
                style.text,
                Some(inner),
                EditorialAlign::Center,
            )?);
        }
        let banner = self.editorial_container(
            width,
            0.0,
            spec.padding,
            spec.gap,
            spec.radius,
            style,
            blocks,
        )?;
        Ok(banner.to_edge(
            match spec.position {
                BannerPosition::Top => Direction::Up,
                BannerPosition::Bottom => Direction::Down,
            },
            spec.margin,
        ))
    }

    pub fn lower_third(&mut self, spec: LowerThirdSpec) -> Result<DrawableHandle, EditorialError> {
        validate_text("title", &spec.title)?;
        validate_optional_text("subtitle", &spec.subtitle)?;
        validate_optional_text("kicker", &spec.kicker)?;
        validate_non_negative("margin", spec.margin)?;
        validate_box_metrics(spec.width, 0.0, spec.padding, spec.gap, spec.radius)?;
        let style = self.resolve_editorial_style(spec.style);
        let inner = content_width_with_inset(spec.width, spec.padding.0, 8.0)?;
        let mut blocks = Vec::new();
        if let Some(kicker) = &spec.kicker {
            blocks.push(self.editorial_block(
                kicker,
                TextRole::Kicker,
                None,
                style.compact_text,
                Some(inner),
                EditorialAlign::Left,
            )?);
        }
        blocks.push(self.editorial_block(
            &spec.title,
            TextRole::Heading,
            None,
            style.text,
            Some(inner),
            EditorialAlign::Left,
        )?);
        if let Some(subtitle) = &spec.subtitle {
            blocks.push(self.editorial_block(
                subtitle,
                TextRole::Subtitle,
                None,
                style.text,
                Some(inner),
                EditorialAlign::Left,
            )?);
        }
        let group = self.editorial_container(
            spec.width,
            0.0,
            spec.padding,
            spec.gap,
            spec.radius,
            style,
            blocks,
        )?;
        Ok(group.to_corner(
            match spec.side {
                LowerThirdSide::Left => Anchor::BottomLeft,
                LowerThirdSide::Right => Anchor::BottomRight,
            },
            spec.margin,
        ))
    }

    pub fn stat_card(&mut self, spec: StatCardSpec) -> Result<DrawableHandle, EditorialError> {
        validate_text("value", &spec.value)?;
        validate_text("label", &spec.label)?;
        validate_optional_text("delta", &spec.delta)?;
        validate_box_metrics(
            spec.width,
            spec.min_height,
            spec.padding,
            spec.gap,
            spec.radius,
        )?;
        let style = self.resolve_editorial_style(spec.style);
        let inner = content_width(spec.width, spec.padding.0)?;
        let mut blocks = vec![
            self.editorial_block(
                &spec.value,
                TextRole::Title,
                None,
                style.compact_text,
                Some(inner),
                EditorialAlign::Left,
            )?,
            self.editorial_block(
                &spec.label,
                TextRole::Label,
                None,
                style.text,
                Some(inner),
                EditorialAlign::Left,
            )?,
        ];
        if let Some(delta) = &spec.delta {
            blocks.push(self.editorial_block(
                delta,
                TextRole::Caption,
                None,
                style.compact_text,
                Some(inner),
                EditorialAlign::Left,
            )?);
        }
        self.editorial_container(
            spec.width,
            spec.min_height,
            spec.padding,
            spec.gap,
            spec.radius,
            style,
            blocks,
        )
    }

    pub fn quote_card(&mut self, spec: QuoteCardSpec) -> Result<DrawableHandle, EditorialError> {
        validate_text("quote", &spec.quote)?;
        validate_optional_text("attribution", &spec.attribution)?;
        validate_box_metrics(spec.width, 0.0, spec.padding, spec.gap, spec.radius)?;
        let style = self.resolve_editorial_style(spec.style);
        let inner = content_width_with_inset(spec.width, spec.padding.0, 14.0)?;
        let mut blocks = vec![self.editorial_block(
            &format!("“{}”", spec.quote),
            TextRole::Body,
            None,
            style.text,
            Some(inner),
            EditorialAlign::Left,
        )?];
        if let Some(attribution) = &spec.attribution {
            blocks.push(self.editorial_block(
                &format!("— {attribution}"),
                TextRole::Caption,
                None,
                style.compact_text,
                Some(inner),
                EditorialAlign::Right,
            )?);
        }
        self.editorial_container(
            spec.width,
            0.0,
            spec.padding,
            spec.gap,
            spec.radius,
            style,
            blocks,
        )
    }

    pub fn section_header(
        &mut self,
        spec: SectionHeaderSpec,
    ) -> Result<DrawableHandle, EditorialError> {
        validate_text("title", &spec.title)?;
        validate_optional_text("kicker", &spec.kicker)?;
        validate_optional_text("subtitle", &spec.subtitle)?;
        validate_box_metrics(spec.width, 0.0, spec.padding, spec.gap, spec.radius)?;
        let style = self.resolve_editorial_style(spec.style);
        let inner = content_width(spec.width, spec.padding.0)?;
        let mut blocks = Vec::new();
        if let Some(kicker) = &spec.kicker {
            blocks.push(self.editorial_block(
                kicker,
                TextRole::Kicker,
                None,
                style.compact_text,
                Some(inner),
                spec.align,
            )?);
        }
        blocks.push(self.editorial_block(
            &spec.title,
            TextRole::Heading,
            None,
            style.text,
            Some(inner),
            spec.align,
        )?);
        if let Some(subtitle) = &spec.subtitle {
            blocks.push(self.editorial_block(
                subtitle,
                TextRole::Subtitle,
                None,
                style.text,
                Some(inner),
                spec.align,
            )?);
        }
        let content_height = blocks.iter().map(|block| block.height).sum::<f64>()
            + spec.gap * blocks.len().saturating_sub(1) as f64;
        let height = content_height + spec.padding.1 * 2.0;
        let header = self.editorial_container(
            spec.width,
            0.0,
            spec.padding,
            spec.gap,
            spec.radius,
            style,
            blocks,
        )?;
        if spec.rule {
            let rule = self
                .line(
                    -spec.width * 0.5 + spec.padding.0,
                    -height * 0.5 + spec.padding.1 * 0.45,
                    spec.width * 0.5 - spec.padding.0,
                    -height * 0.5 + spec.padding.1 * 0.45,
                )
                .no_fill()
                .stroke(style.compact_text, 3.0);
            Ok(self.group(&[&header, &rule]))
        } else {
            Ok(header)
        }
    }

    fn resolve_editorial_style(&self, style: EditorialStyle) -> ResolvedEditorialStyle {
        let theme_color = |name: &str, fallback: Color| {
            self.theme_style
                .as_ref()
                .and_then(|theme| theme.color(name).ok())
                .unwrap_or(fallback)
        };
        let foreground = theme_color("foreground", Color::from_rgb8(0xE6, 0xED, 0xF5));
        let panel = theme_color("panel", Color::from_rgb8(0x10, 0x16, 0x20));
        let tone = match style.variant {
            EditorialVariant::Neutral => theme_color("muted", Color::from_rgb8(0x94, 0xA3, 0xB8)),
            EditorialVariant::Accent => theme_color("accent", Color::from_rgb8(0x5B, 0x8F, 0xC9)),
            EditorialVariant::Success => theme_color("success", SUCCESS),
            EditorialVariant::Warning => theme_color("warning", WARNING),
            EditorialVariant::Danger => theme_color("danger", DANGER),
        };
        let default_background = match style.appearance {
            EditorialAppearance::Soft => panel,
            EditorialAppearance::Solid => tone,
            EditorialAppearance::Outline => Color::TRANSPARENT,
        };
        let default_text = if style.appearance == EditorialAppearance::Solid {
            contrasting_text(tone)
        } else {
            foreground
        };
        let compact_text = style.color.unwrap_or_else(|| {
            if style.appearance == EditorialAppearance::Solid {
                default_text
            } else {
                tone
            }
        });
        ResolvedEditorialStyle {
            text: style.color.unwrap_or(default_text),
            compact_text,
            background: style.background.unwrap_or(default_background),
            border: style.border.unwrap_or(tone),
            appearance: style.appearance,
        }
    }

    fn editorial_panel(
        &mut self,
        width: f64,
        height: f64,
        radius: f64,
        style: ResolvedEditorialStyle,
    ) -> DrawableHandle {
        let panel = self.rounded_rect(width, height, radius);
        let panel = if style.appearance == EditorialAppearance::Outline
            && style.background == Color::TRANSPARENT
        {
            panel.no_fill()
        } else {
            panel.fill(style.background)
        };
        panel.stroke(style.border, 2.0)
    }

    fn editorial_block(
        &self,
        content: &str,
        role: TextRole,
        size: Option<f64>,
        color: Color,
        wrap: Option<f64>,
        align: EditorialAlign,
    ) -> Result<TextBlock, EditorialError> {
        let (_, height) = self.measure_editorial_text(content, role, size, color, wrap)?;
        Ok(TextBlock {
            content: content.to_owned(),
            role,
            size,
            color,
            height,
            wrap,
            align,
        })
    }

    fn editorial_container(
        &mut self,
        width: f64,
        min_height: f64,
        padding: (f64, f64),
        gap: f64,
        radius: f64,
        style: ResolvedEditorialStyle,
        blocks: Vec<TextBlock>,
    ) -> Result<DrawableHandle, EditorialError> {
        let block_height: f64 = blocks.iter().map(|block| block.height).sum();
        let gaps = gap * blocks.len().saturating_sub(1) as f64;
        let content_height = block_height + gaps;
        let height = min_height.max(content_height + padding.1 * 2.0);
        let panel = self.editorial_panel(width, height, radius, style);
        let mut members = vec![panel];
        let mut cursor = content_height * 0.5;
        for block in blocks {
            let y = cursor - block.height * 0.5;
            let x = block.align.x(width, padding.0);
            let text = self
                .editorial_text(
                    &block.content,
                    block.role,
                    block.size,
                    block.color,
                    block.wrap,
                    block.align,
                )?
                .at_anchor(x, y, block.align.anchor());
            members.push(text);
            cursor -= block.height + gap;
        }
        let refs: Vec<&DrawableHandle> = members.iter().collect();
        Ok(self.group(&refs))
    }

    fn editorial_text(
        &mut self,
        content: &str,
        role: TextRole,
        size: Option<f64>,
        color: Color,
        wrap: Option<f64>,
        align: EditorialAlign,
    ) -> Result<DrawableHandle, EditorialError> {
        let spec = TextSpec::new(
            vec![content.to_owned().into()],
            Some(role),
            TextStyle {
                size,
                color: Some(color),
                ..Default::default()
            },
            TextFlow {
                wrap: wrap.map(TextWrap::Width).unwrap_or(TextWrap::NoWrap),
                align: align.text_align(),
                ..Default::default()
            },
        )
        .map_err(|error| EditorialError::Text(error.to_string()))?;
        Ok(self.text_spec(spec))
    }

    fn measure_editorial_text(
        &self,
        content: &str,
        role: TextRole,
        size: Option<f64>,
        color: Color,
        wrap: Option<f64>,
    ) -> Result<(f64, f64), EditorialError> {
        self.measure_text(content, Some(role), size, None, Some(color), wrap)
            .map_err(EditorialError::Text)
    }
}

fn contrasting_text(color: Color) -> Color {
    let rgba = color.to_rgba8();
    let linear = |channel: u8| {
        let value = f64::from(channel) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * linear(rgba.r) + 0.7152 * linear(rgba.g) + 0.0722 * linear(rgba.b);
    if luminance > 0.45 {
        Color::BLACK
    } else {
        Color::WHITE
    }
}

fn validate_text(field: &'static str, value: &str) -> Result<(), EditorialError> {
    if value.trim().is_empty() {
        Err(EditorialError::EmptyText { field })
    } else {
        Ok(())
    }
}

fn validate_optional_text(
    field: &'static str,
    value: &Option<String>,
) -> Result<(), EditorialError> {
    if let Some(value) = value {
        validate_text(field, value)?;
    }
    Ok(())
}

fn validate_positive(field: &'static str, value: f64) -> Result<(), EditorialError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(EditorialError::PositiveNumber { field })
    }
}

fn validate_non_negative(field: &'static str, value: f64) -> Result<(), EditorialError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(EditorialError::NonNegativeNumber { field })
    }
}

fn validate_optional_positive(
    field: &'static str,
    value: Option<f64>,
) -> Result<(), EditorialError> {
    if let Some(value) = value {
        validate_positive(field, value)?;
    }
    Ok(())
}

fn validate_optional_non_negative(
    field: &'static str,
    value: Option<f64>,
) -> Result<(), EditorialError> {
    if let Some(value) = value {
        validate_non_negative(field, value)?;
    }
    Ok(())
}

fn validate_padding(padding: (f64, f64)) -> Result<(), EditorialError> {
    validate_non_negative("horizontal padding", padding.0)?;
    validate_non_negative("vertical padding", padding.1)
}

fn content_width(width: f64, horizontal_padding: f64) -> Result<f64, EditorialError> {
    let content = width - horizontal_padding * 2.0;
    if content.is_finite() && content > 0.0 {
        Ok(content)
    } else {
        Err(EditorialError::ContentWidth)
    }
}

fn content_width_with_inset(
    width: f64,
    horizontal_padding: f64,
    inset: f64,
) -> Result<f64, EditorialError> {
    let content = content_width(width, horizontal_padding)? - inset;
    if content.is_finite() && content > 0.0 {
        Ok(content)
    } else {
        Err(EditorialError::ContentWidth)
    }
}

fn validate_box_metrics(
    width: f64,
    min_height: f64,
    padding: (f64, f64),
    gap: f64,
    radius: f64,
) -> Result<(), EditorialError> {
    validate_positive("width", width)?;
    validate_non_negative("min_height", min_height)?;
    validate_padding(padding)?;
    validate_non_negative("gap", gap)?;
    validate_non_negative("radius", radius)?;
    content_width(width, padding.0).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::CanvasTheme;

    #[test]
    fn semantic_tokens_and_explicit_overrides_resolve() {
        let mut canvas = SceneModel::new(1280, 720);
        let mut theme = CanvasTheme::builtin("paper").unwrap();
        let custom_success = Color::from_rgb8(0x01, 0xA2, 0xB3);
        theme
            .set_colors(&std::collections::HashMap::from([(
                "success".to_owned(),
                custom_success,
            )]))
            .unwrap();
        canvas.theme_style = Some(theme);
        let success = canvas
            .resolve_editorial_style(EditorialStyle::default().variant(EditorialVariant::Success));
        assert_eq!(success.border, custom_success);

        let custom = Color::from_rgb8(1, 2, 3);
        let resolved = canvas.resolve_editorial_style(
            EditorialStyle::default()
                .variant(EditorialVariant::Danger)
                .color(custom)
                .background(custom)
                .border(custom),
        );
        assert_eq!(resolved.text, custom);
        assert_eq!(resolved.background, custom);
        assert_eq!(resolved.border, custom);
    }

    #[test]
    fn solid_contrast_selects_black_or_white() {
        assert_eq!(contrasting_text(Color::WHITE), Color::BLACK);
        assert_eq!(contrasting_text(Color::BLACK), Color::WHITE);

        let canvas = SceneModel::new(1280, 720);
        let resolved = canvas.resolve_editorial_style(
            EditorialStyle::default()
                .variant(EditorialVariant::Success)
                .appearance(EditorialAppearance::Solid),
        );
        assert_eq!(resolved.compact_text, contrasting_text(resolved.background));
    }

    #[test]
    fn specs_reject_empty_text_and_invalid_geometry() {
        let mut canvas = SceneModel::new(1280, 720);
        assert!(!SectionHeaderSpec::new("Title").rule);
        assert_eq!(
            canvas.badge(BadgeSpec::new("  ")).unwrap_err(),
            EditorialError::EmptyText { field: "text" }
        );
        let mut spec = CardSpec::new("Title");
        spec.width = 20.0;
        spec.padding = (12.0, 4.0);
        assert_eq!(canvas.card(spec).unwrap_err(), EditorialError::ContentWidth);
    }

    #[test]
    fn all_editorial_factories_return_group_drawables() {
        let mut canvas = SceneModel::new(1280, 720).margin_all(48.0);
        let banner = canvas.banner(BannerSpec::new("Banner")).unwrap();
        let lower_third = canvas.lower_third(LowerThirdSpec::new("Speaker")).unwrap();
        assert!(
            banner
                .spec
                .lock()
                .unwrap()
                .layout_ops
                .iter()
                .any(|op| matches!(op, crate::canvas::LayoutOp::ToEdge { .. }))
        );
        assert!(
            lower_third
                .spec
                .lock()
                .unwrap()
                .layout_ops
                .iter()
                .any(|op| matches!(op, crate::canvas::LayoutOp::ToCorner { .. }))
        );
        let handles = [
            canvas.badge(BadgeSpec::new("New")).unwrap(),
            canvas.chip(ChipSpec::new("Ready")).unwrap(),
            canvas.card(CardSpec::new("Card").body("Body")).unwrap(),
            banner,
            lower_third,
            canvas
                .stat_card(StatCardSpec::new("42", "Answers"))
                .unwrap(),
            canvas.quote_card(QuoteCardSpec::new("Clarity")).unwrap(),
            canvas
                .section_header(SectionHeaderSpec::new("Section"))
                .unwrap(),
        ];
        assert!(handles.iter().all(|handle| matches!(
            handle.spec.lock().unwrap().kind,
            super::super::SpawnKind::Group(_)
        )));
    }
}
