//! Code blocks: the Typst markup of a highlighted `raw` block and the
//! tmTheme that colors its tokens.
//!
//! Typst highlights `raw` with syntect and paints every token whose theme
//! color differs from the theme's foreground; the rest keep the text fill.
//! A block therefore sets its text fill to the plain color and hands Typst a
//! theme whose foreground is that same color.

use std::io::Cursor;

use gaanim_core::peniko::Color;

/// Monospaced family of the `code` text role and of code blocks, and the one
/// a block falls back to when its font is missing. Gaanim embeds it (see
/// [`crate::font::FontRegistry`]), so code looks the same on every machine
/// and on the web.
pub const CODE_FALLBACK_FONT: &str = "JetBrains Mono";

/// Colors of the token classes a code block tells apart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CodeColors {
    /// Text no class colors: names, operators and punctuation.
    pub foreground: Color,
    /// Keywords and storage words (`def`, `fn`, `return`, `let`).
    pub keyword: Color,
    pub string: Color,
    /// Numbers and language constants (`42`, `True`, `None`).
    pub number: Color,
    pub comment: Color,
    /// Names of functions where they are defined or called.
    pub function: Color,
    /// Names of types and classes.
    pub type_name: Color,
}

/// Least contrast (WCAG 2) of a code block's plain text with its background.
pub const PLAIN_CONTRAST: f64 = 4.5;

/// Least contrast of a highlighted token, comments included, with the
/// background: tokens may be quieter than plain text, never unreadable.
pub const TOKEN_CONTRAST: f64 = 3.0;

impl CodeColors {
    /// Token colors for a block drawn over `background`, from a theme's
    /// colors: keywords take the accent, comments the muted color (or a mix
    /// of the text and the background when muted is the text color), names
    /// of functions and types the title color, and strings and numbers a
    /// green and an orange that read over a dark or a light background.
    ///
    /// Every color reads over `background`: a `foreground` below
    /// [`PLAIN_CONTRAST`] becomes white or black, names of functions and
    /// types whose title color is below [`TOKEN_CONTRAST`] take the plain
    /// color, and the other tokens below it move toward white or black (see
    /// [`with_contrast`]). Colors that already read are kept as they are.
    pub fn derive(
        foreground: Color,
        background: Color,
        accent: Color,
        muted: Color,
        title: Color,
    ) -> Self {
        let dark = is_dark(background);
        let foreground = if contrast(foreground, background) >= PLAIN_CONTRAST {
            foreground
        } else if dark {
            Color::WHITE
        } else {
            Color::BLACK
        };
        let comment =
            if distance(muted, foreground) < 0.12 || contrast(muted, background) < TOKEN_CONTRAST {
                mix(foreground, background, 0.45)
            } else {
                muted
            };
        let (string, number) = if dark {
            (rgb(0xA9C77E), rgb(0xE08A68))
        } else {
            (rgb(0x3A7D2C), rgb(0xB4531F))
        };
        let token = |color| with_contrast(color, background, TOKEN_CONTRAST);
        // A title that does not read is usually the other theme's text color:
        // names keep the plain text color rather than a gray step toward it.
        let name = if contrast(title, background) >= TOKEN_CONTRAST {
            title
        } else {
            foreground
        };
        Self {
            foreground,
            keyword: token(accent),
            string,
            number,
            comment: token(comment),
            function: name,
            type_name: name,
        }
    }

    /// The tmTheme (an Apple property list) that paints each class. Its
    /// foreground is `self.foreground`, so plain tokens keep the text fill.
    pub fn tm_theme(&self) -> String {
        let foreground = hex(self.foreground);
        let rules = [
            ("Comment", "comment", self.comment),
            (
                "Keyword",
                "keyword, storage.type, storage.modifier",
                self.keyword,
            ),
            // Operators and punctuation stay plain, even inside keywords.
            (
                "Operator",
                "keyword.operator, punctuation.separator, punctuation.accessor",
                self.foreground,
            ),
            ("String", "string", self.string),
            // The decimal point of a number is punctuation inside it, and
            // must outweigh the operator rule above.
            (
                "Number",
                "constant.numeric, constant.numeric punctuation.separator, constant.language, \
                 constant.character, constant.other",
                self.number,
            ),
            (
                "Function",
                "entity.name.function, variable.function, support.function",
                self.function,
            ),
            (
                "Type",
                "entity.name.type, entity.name.class, entity.other.inherited-class, \
                 support.type, support.class, storage.type.primitive",
                self.type_name,
            ),
        ];
        let mut xml = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
             \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\">\n<dict>\n<key>name</key><string>Gaanim</string>\n\
             <key>settings</key>\n<array>\n",
        );
        xml.push_str(&format!(
            "<dict><key>settings</key><dict><key>foreground</key><string>{foreground}</string>\
             </dict></dict>\n"
        ));
        for (name, scope, color) in rules {
            xml.push_str(&format!(
                "<dict><key>name</key><string>{name}</string><key>scope</key>\
                 <string>{scope}</string><key>settings</key><dict><key>foreground</key>\
                 <string>{}</string></dict></dict>\n",
                hex(color)
            ));
        }
        xml.push_str("</array>\n</dict>\n</plist>\n");
        xml
    }
}

/// A tmTheme document checked to be one, with its own base colors.
#[derive(Debug, Clone, PartialEq)]
pub struct TmTheme {
    pub xml: String,
    /// The theme's foreground, for the text no token rule colors.
    pub foreground: Option<Color>,
    /// The theme's background, for the block behind the code.
    pub background: Option<Color>,
}

impl TmTheme {
    /// Read a tmTheme, rejecting documents syntect cannot load.
    pub fn parse(xml: &str) -> Result<Self, String> {
        let theme = syntect::highlighting::ThemeSet::load_from_reader(&mut Cursor::new(xml))
            .map_err(|error| format!("not a tmTheme: {error}"))?;
        let color = |color: syntect::highlighting::Color| {
            Color::from_rgba8(color.r, color.g, color.b, color.a)
        };
        Ok(Self {
            xml: xml.to_owned(),
            foreground: theme.settings.foreground.map(color),
            background: theme.settings.background.map(color),
        })
    }
}

/// How a code block colors its tokens.
#[derive(Debug, Clone, PartialEq)]
pub enum CodeHighlight {
    /// Token classes in these colors.
    Colors(CodeColors),
    /// An author's tmTheme document.
    TmTheme(String),
    /// Every token in the plain color.
    Off,
}

/// A code block's text, laid out by Typst as a `raw` block.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeMarkup<'a> {
    pub source: &'a str,
    /// A syntax name or file extension Typst knows (`python`, `rs`); others
    /// stay plain.
    pub language: &'a str,
    /// Text size before `raw` scales it to 0.8, in Typst points.
    pub font_size: f64,
    /// Width the lines may take, in Typst points.
    pub width: f64,
    /// Monospaced family, before [`CODE_FALLBACK_FONT`].
    pub font: Option<&'a str>,
    /// Color of plain text.
    pub foreground: Color,
    pub highlight: CodeHighlight,
}

impl CodeMarkup<'_> {
    /// The Typst source of the block.
    pub fn typst(&self) -> String {
        let fonts = match self.font {
            Some(font) if font != CODE_FALLBACK_FONT => {
                format!("(\"{}\", \"{CODE_FALLBACK_FONT}\")", escape(font))
            }
            _ => format!("\"{CODE_FALLBACK_FONT}\""),
        };
        let theme = match &self.highlight {
            CodeHighlight::Colors(colors) => format!(
                "bytes(\"{}\")",
                escape(
                    &CodeColors {
                        foreground: self.foreground,
                        ..*colors
                    }
                    .tm_theme()
                )
            ),
            CodeHighlight::TmTheme(xml) => format!("bytes(\"{}\")", escape(xml)),
            CodeHighlight::Off => "none".to_owned(),
        };
        format!(
            "#set text(fill: rgb(\"{}\"), size: {}pt)\n\
             #show raw: set text(font: {fonts})\n\
             #set raw(theme: {theme})\n\
             #block(width: {}pt)[#raw(\"{}\", block: true, lang: \"{}\")]",
            hex(self.foreground),
            self.font_size,
            self.width,
            escape(self.source),
            escape(self.language),
        )
    }
}

/// The Typst source of a code block's caption, such as its language: `text`
/// in the bundled monospace, `size` Typst points tall, slightly tracked.
pub fn code_caption_typst(text: &str, size: f64, color: Color) -> String {
    format!(
        "#set text(font: \"{CODE_FALLBACK_FONT}\", size: {size}pt, fill: rgb(\"{}\"), \
         tracking: 0.08em)\n#\"{}\"",
        hex(color),
        escape(text)
    )
}

/// `value` as the inside of a Typst string literal.
fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

const fn rgb(value: u32) -> Color {
    Color::from_rgb8((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

/// `#RRGGBB`, or `#RRGGBBAA` for a translucent color.
fn hex(color: Color) -> String {
    let rgba = color.to_rgba8();
    if rgba.a == u8::MAX {
        format!("#{:02X}{:02X}{:02X}", rgba.r, rgba.g, rgba.b)
    } else {
        format!("#{:02X}{:02X}{:02X}{:02X}", rgba.r, rgba.g, rgba.b, rgba.a)
    }
}

/// Whether light text reads better than dark text over `color`.
pub fn is_dark(color: Color) -> bool {
    let rgba = color.to_rgba8();
    let luminance =
        (0.2126 * f64::from(rgba.r) + 0.7152 * f64::from(rgba.g) + 0.0722 * f64::from(rgba.b))
            / 255.0;
    luminance < 0.5
}

/// Contrast ratio of two colors (WCAG 2), from 1 for equal colors to 21 for
/// black on white.
pub fn contrast(a: Color, b: Color) -> f64 {
    let (a, b) = (relative_luminance(a), relative_luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `color`, or the first step from it toward white over a dark `background`
/// (black over a light one) whose contrast with `background` reaches
/// `minimum`. A color that already reads is returned unchanged.
pub fn with_contrast(color: Color, background: Color, minimum: f64) -> Color {
    let target = if is_dark(background) {
        Color::WHITE
    } else {
        Color::BLACK
    };
    (0..=10)
        .map(|step| mix(color, target, f64::from(step) / 10.0))
        .find(|candidate| contrast(*candidate, background) >= minimum)
        .unwrap_or(target)
}

/// WCAG relative luminance of the sRGB color, from 0 to 1.
fn relative_luminance(color: Color) -> f64 {
    let rgba = color.to_rgba8();
    let linear = |channel: u8| {
        let value = f64::from(channel) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(rgba.r) + 0.7152 * linear(rgba.g) + 0.0722 * linear(rgba.b)
}

/// Largest channel difference of two colors, from 0 to 1.
fn distance(a: Color, b: Color) -> f64 {
    let (a, b) = (a.to_rgba8(), b.to_rgba8());
    [(a.r, b.r), (a.g, b.g), (a.b, b.b)]
        .into_iter()
        .map(|(a, b)| f64::from(a.abs_diff(b)) / 255.0)
        .fold(0.0, f64::max)
}

/// `a` moved `amount` of the way to `b`, channel by channel.
fn mix(a: Color, b: Color, amount: f64) -> Color {
    let (a, b) = (a.to_rgba8(), b.to_rgba8());
    let channel = |a: u8, b: u8| {
        (f64::from(a) + (f64::from(b) - f64::from(a)) * amount)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color::from_rgb8(channel(a.r, b.r), channel(a.g, b.g), channel(a.b, b.b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typst_compiler::compiled_glyph_fills;

    const FOREGROUND: Color = rgb(0xE6E6E6);

    fn colors() -> CodeColors {
        CodeColors {
            foreground: FOREGROUND,
            keyword: rgb(0xF2A541),
            string: rgb(0xA9C77E),
            number: rgb(0xE08A68),
            comment: rgb(0x8A8A8A),
            function: rgb(0x7AA2F7),
            type_name: rgb(0xC9A8E0),
        }
    }

    fn markup(source: &str, highlight: CodeHighlight) -> String {
        CodeMarkup {
            source,
            language: "python",
            font_size: 20.0,
            width: 600.0,
            font: None,
            foreground: FOREGROUND,
            highlight,
        }
        .typst()
    }

    /// The color of the glyphs of each character of `source`, in order.
    fn painted(source: &str, highlight: CodeHighlight) -> Vec<(char, Color)> {
        compiled_glyph_fills(&markup(source, highlight))
            .into_iter()
            .map(|(character, fill)| (character, fill.expect("every glyph is filled")))
            .collect()
    }

    fn color_of(glyphs: &[(char, Color)], text: &str, word: &str) -> Color {
        let visible: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let start = visible.find(word).expect("word in the source");
        glyphs[visible[..start].chars().count()].1
    }

    #[test]
    fn generated_themes_load_with_their_foreground() {
        let theme = TmTheme::parse(&colors().tm_theme()).expect("a valid tmTheme");
        assert_eq!(theme.foreground, Some(FOREGROUND));
        assert_eq!(theme.background, None);
        assert!(TmTheme::parse("<plist>not a theme").is_err());
    }

    #[test]
    fn highlighted_blocks_paint_tokens_in_their_class_color() {
        let source = "def area(r):\n    # circle\n    return 3.14 * r * r, \"cm\"";
        let glyphs = painted(source, CodeHighlight::Colors(colors()));
        let colors = colors();
        assert_eq!(color_of(&glyphs, source, "def"), colors.keyword);
        assert_eq!(color_of(&glyphs, source, "area"), colors.function);
        assert_eq!(color_of(&glyphs, source, "#circle"), colors.comment);
        assert_eq!(color_of(&glyphs, source, "return"), colors.keyword);
        assert_eq!(color_of(&glyphs, source, "3.14"), colors.number);
        assert_eq!(color_of(&glyphs, source, ".14"), colors.number);
        assert_eq!(color_of(&glyphs, source, "\"cm\""), colors.string);
        // Operators and plain names keep the text color.
        assert_eq!(color_of(&glyphs, source, "*"), FOREGROUND);
        assert_eq!(color_of(&glyphs, source, ",\"cm"), FOREGROUND);
    }

    #[test]
    fn unhighlighted_blocks_and_unknown_languages_stay_plain() {
        let source = "def area(r): return 3.14";
        let unknown = CodeMarkup {
            source,
            language: "not-a-language",
            font_size: 20.0,
            width: 600.0,
            font: None,
            foreground: FOREGROUND,
            highlight: CodeHighlight::Colors(colors()),
        }
        .typst();
        let fills = compiled_glyph_fills(&unknown);
        assert!(!fills.is_empty());
        assert!(fills.iter().all(|(_, fill)| *fill == Some(FOREGROUND)));
        let off = painted(source, CodeHighlight::Off);
        assert!(off.iter().all(|(_, color)| *color == FOREGROUND));
    }

    #[test]
    fn highlighting_keeps_every_glyph_and_its_character() {
        // Token matching across code blocks relies on the same glyphs, in the
        // same order, whether or not they are colored.
        let source = "velocity += acceleration * dt\nposition += velocity * dt";
        let plain = painted(source, CodeHighlight::Off);
        let colored = painted(source, CodeHighlight::Colors(colors()));
        let characters = |glyphs: &[(char, Color)]| glyphs.iter().map(|g| g.0).collect::<String>();
        assert_eq!(characters(&plain), characters(&colored));
        assert_eq!(
            characters(&plain),
            source
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
        );
    }

    #[test]
    fn author_themes_color_tokens_their_way() {
        let theme = CodeColors {
            keyword: rgb(0xFF0000),
            ..colors()
        }
        .tm_theme();
        let source = "return x";
        let glyphs = painted(source, CodeHighlight::TmTheme(theme));
        assert_eq!(color_of(&glyphs, source, "return"), rgb(0xFF0000));
        assert_eq!(color_of(&glyphs, source, "x"), FOREGROUND);
    }

    #[test]
    fn missing_fonts_fall_back_to_the_bundled_monospace() {
        let source = "x = 1";
        let with = |font| {
            compiled_glyph_fills(
                &CodeMarkup {
                    source,
                    language: "python",
                    font_size: 20.0,
                    width: 600.0,
                    font,
                    foreground: FOREGROUND,
                    highlight: CodeHighlight::Off,
                }
                .typst(),
            )
            .len()
        };
        assert_eq!(with(Some("No Such Mono")), with(None));
        assert!(markup(source, CodeHighlight::Off).contains("\"JetBrains Mono\""));
    }

    #[test]
    fn derived_colors_follow_the_background() {
        let dark = CodeColors::derive(
            FOREGROUND,
            rgb(0x1C1C1C),
            rgb(0xF2A541),
            rgb(0xA0A0A0),
            rgb(0xFAFAFA),
        );
        assert_eq!(dark.keyword, rgb(0xF2A541));
        assert_eq!(dark.comment, rgb(0xA0A0A0));
        assert!(!is_dark(dark.string));

        // A muted color equal to the text gives comments a mix instead.
        let light = CodeColors::derive(
            Color::BLACK,
            Color::WHITE,
            rgb(0x2563EB),
            Color::BLACK,
            Color::BLACK,
        );
        assert_ne!(light.comment, Color::BLACK);
        assert!(is_dark(light.string) && is_dark(light.number));
    }

    fn assert_reads(colors: &CodeColors, background: Color) {
        assert!(contrast(colors.foreground, background) >= PLAIN_CONTRAST);
        for token in [
            colors.keyword,
            colors.string,
            colors.number,
            colors.comment,
            colors.function,
            colors.type_name,
        ] {
            assert!(
                contrast(token, background) >= TOKEN_CONTRAST,
                "{token:?} over {background:?}"
            );
        }
    }

    #[test]
    fn contrast_follows_wcag() {
        assert!((contrast(Color::BLACK, Color::WHITE) - 21.0).abs() < 1e-9);
        assert!((contrast(FOREGROUND, FOREGROUND) - 1.0).abs() < 1e-9);
        // A color that reads is kept; one that does not moves toward black
        // over white until it reads.
        assert_eq!(with_contrast(Color::BLACK, Color::WHITE, 3.0), Color::BLACK);
        let amber = with_contrast(rgb(0xF2A541), Color::WHITE, 3.0);
        assert_ne!(amber, rgb(0xF2A541));
        assert!(contrast(amber, Color::WHITE) >= 3.0);
    }

    #[test]
    fn derived_colors_read_over_a_background_of_the_other_lightness() {
        // A light theme's colors over a dark panel.
        let dark = rgb(0x14161B);
        let colors = CodeColors::derive(
            Color::BLACK,
            dark,
            rgb(0x2563EB),
            Color::BLACK,
            Color::BLACK,
        );
        assert_eq!(colors.foreground, Color::WHITE);
        assert_reads(&colors, dark);

        // The dark `technical` colors over white.
        let colors = CodeColors::derive(
            FOREGROUND,
            Color::WHITE,
            rgb(0xF2A541),
            rgb(0xA0A0A0),
            rgb(0xFAFAFA),
        );
        assert_eq!(colors.foreground, Color::BLACK);
        assert_reads(&colors, Color::WHITE);
        // Names in the other theme's text color fall back to plain text.
        assert_eq!(colors.function, colors.foreground);

        // Over the panel they were made for, they are kept as they are.
        let panel = rgb(0x1C1C1C);
        let colors = CodeColors::derive(
            FOREGROUND,
            panel,
            rgb(0xF2A541),
            rgb(0xA0A0A0),
            rgb(0xFAFAFA),
        );
        assert_eq!(
            (
                colors.foreground,
                colors.keyword,
                colors.comment,
                colors.function
            ),
            (FOREGROUND, rgb(0xF2A541), rgb(0xA0A0A0), rgb(0xFAFAFA))
        );
        assert_reads(&colors, panel);
    }
}
