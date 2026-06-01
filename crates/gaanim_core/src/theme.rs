use crate::peniko::Color;

/// A role-based color theme that defines the visual aesthetic of the scene.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Theme {
    pub background: Color,
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub muted: Color,
}

impl Theme {
    /// Creates a custom color theme with specific role mappings.
    pub fn new(
        background: Color,
        primary: Color,
        secondary: Color,
        accent: Color,
        muted: Color,
    ) -> Self {
        Self {
            background,
            primary,
            secondary,
            accent,
            muted,
        }
    }

    /// Default premium dark theme (Catppuccin Mocha inspired).
    pub fn dark() -> Self {
        Self {
            background: Color::from_rgb8(30, 30, 46),
            primary: Color::from_rgb8(205, 214, 244),
            secondary: Color::from_rgb8(137, 180, 250),
            accent: Color::from_rgb8(249, 226, 175),
            muted: Color::from_rgb8(166, 173, 200),
        }
    }

    /// Premium high-contrast light theme (Catppuccin Latte inspired).
    pub fn light() -> Self {
        Self {
            background: Color::from_rgb8(249, 249, 251),
            primary: Color::from_rgb8(30, 30, 46),
            secondary: Color::from_rgb8(23, 146, 148),
            accent: Color::from_rgb8(230, 69, 83),
            muted: Color::from_rgb8(156, 160, 176),
        }
    }

    /// Classic premium Dracula dark theme.
    pub fn dracula() -> Self {
        Self {
            background: Color::from_rgb8(40, 42, 54),
            primary: Color::from_rgb8(248, 248, 242),
            secondary: Color::from_rgb8(139, 233, 253),
            accent: Color::from_rgb8(255, 121, 198),
            muted: Color::from_rgb8(98, 114, 164),
        }
    }

    /// Warm, retro Gruvbox dark theme.
    pub fn gruvbox() -> Self {
        Self {
            background: Color::from_rgb8(40, 40, 40),
            primary: Color::from_rgb8(235, 219, 178),
            secondary: Color::from_rgb8(184, 187, 38),
            accent: Color::from_rgb8(254, 128, 25),
            muted: Color::from_rgb8(168, 153, 132),
        }
    }
}
