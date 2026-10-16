//! Categorical palettes: the colors that tell categories apart.
//!
//! A palette is an ordered list of colors handed out to categories in turn.
//! When categories outnumber its colors it goes round again, each lap a
//! little lighter or darker than the last, so no two categories share a color.

use crate::{ColorMap, interpolate_color};
use peniko::Color;

const fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Built-in palettes, in the order [`Palette::names`] lists them.
const BUILTINS: &[(&str, &[Color])] = &[
    (
        "gaanim",
        &[
            rgb(0x358FD3),
            rgb(0x63D1C5),
            rgb(0xFFC857),
            rgb(0xE85D75),
            rgb(0x9B7FE0),
            rgb(0xF28E2B),
            rgb(0x59A14F),
            rgb(0xB07AA1),
            rgb(0x9C755F),
            rgb(0xBAB0AC),
        ],
    ),
    (
        "tableau10",
        &[
            rgb(0x4E79A7),
            rgb(0xF28E2B),
            rgb(0xE15759),
            rgb(0x76B7B2),
            rgb(0x59A14F),
            rgb(0xEDC948),
            rgb(0xB07AA1),
            rgb(0xFF9DA7),
            rgb(0x9C755F),
            rgb(0xBAB0AC),
        ],
    ),
    // Okabe and Ito's colors for color-blind viewers, without black, which
    // vanishes on a dark background.
    (
        "okabe_ito",
        &[
            rgb(0xE69F00),
            rgb(0x56B4E9),
            rgb(0x009E73),
            rgb(0xF0E442),
            rgb(0x0072B2),
            rgb(0xD55E00),
            rgb(0xCC79A7),
        ],
    ),
    (
        "set2",
        &[
            rgb(0x66C2A5),
            rgb(0xFC8D62),
            rgb(0x8DA0CB),
            rgb(0xE78AC3),
            rgb(0xA6D854),
            rgb(0xFFD92F),
            rgb(0xE5C494),
            rgb(0xB3B3B3),
        ],
    ),
    (
        "dark2",
        &[
            rgb(0x1B9E77),
            rgb(0xD95F02),
            rgb(0x7570B3),
            rgb(0xE7298A),
            rgb(0x66A61E),
            rgb(0xE6AB02),
            rgb(0xA6761D),
            rgb(0x666666),
        ],
    ),
    (
        "paired",
        &[
            rgb(0xA6CEE3),
            rgb(0x1F78B4),
            rgb(0xB2DF8A),
            rgb(0x33A02C),
            rgb(0xFB9A99),
            rgb(0xE31A1C),
            rgb(0xFDBF6F),
            rgb(0xFF7F00),
            rgb(0xCAB2D6),
            rgb(0x6A3D9A),
            rgb(0xFFFF99),
            rgb(0xB15928),
        ],
    ),
];

/// How far each lap past the first moves the colors toward white or black.
const LAP_SHIFT: f64 = 0.22;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PaletteError {
    #[error("a palette needs at least one color")]
    Empty,
    #[error("unknown palette '{0}'; the palettes are {names}", names = Palette::names().join(", "))]
    Unknown(String),
    #[error("a palette sampled from a colormap needs at least one color")]
    InvalidCount,
}

/// An ordered list of categorical colors; see the module documentation.
#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    name: Option<String>,
    colors: Vec<Color>,
}

impl Default for Palette {
    /// The `gaanim` palette.
    fn default() -> Self {
        Self::named("gaanim").expect("the gaanim palette is built in")
    }
}

impl Palette {
    /// A palette of `colors`, in order.
    pub fn new(colors: Vec<Color>) -> Result<Self, PaletteError> {
        if colors.is_empty() {
            return Err(PaletteError::Empty);
        }
        Ok(Self { name: None, colors })
    }

    /// A built-in palette, or a categorical colormap (`tab10`, `tab20`…),
    /// by name; ASCII case is ignored.
    pub fn named(name: &str) -> Result<Self, PaletteError> {
        let wanted = name.trim();
        if let Some((name, colors)) = BUILTINS
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(wanted))
        {
            return Ok(Self {
                name: Some((*name).to_owned()),
                colors: colors.to_vec(),
            });
        }
        let map = ColorMap::named(wanted)
            .ok()
            .filter(ColorMap::is_categorical)
            .ok_or_else(|| PaletteError::Unknown(name.to_owned()))?;
        let colors = map.stop_colors();
        Ok(Self {
            name: map.name().map(str::to_owned),
            colors,
        })
    }

    /// `count` colors spread evenly along `map`, from one end to the other.
    pub fn from_colormap(map: &ColorMap, count: usize) -> Result<Self, PaletteError> {
        if count == 0 {
            return Err(PaletteError::InvalidCount);
        }
        let colors = map.colors(count).map_err(|_| PaletteError::InvalidCount)?;
        Ok(Self { name: None, colors })
    }

    /// The names [`Self::named`] accepts.
    pub fn names() -> Vec<&'static str> {
        BUILTINS
            .iter()
            .map(|(name, _)| *name)
            .chain(
                ColorMap::names(None)
                    .into_iter()
                    .filter(|name| ColorMap::named(name).is_ok_and(|map| map.is_categorical())),
            )
            .collect()
    }

    /// The built-in name, if the palette is one unchanged.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The palette's own colors, one lap.
    pub fn base_colors(&self) -> &[Color] {
        &self.colors
    }

    pub fn len(&self) -> usize {
        self.colors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }

    /// The color of the category at `index`. Past the last color the
    /// palette starts again, each lap moved further toward white (odd laps)
    /// or black (even laps), alpha kept.
    pub fn color(&self, index: usize) -> Color {
        let base = self.colors[index % self.colors.len()];
        let lap = index / self.colors.len();
        if lap == 0 {
            return base;
        }
        let shift = (LAP_SHIFT * lap.div_ceil(2) as f64).min(0.66);
        let toward = if lap % 2 == 1 {
            Color::WHITE
        } else {
            Color::BLACK
        };
        let alpha = base.to_rgba8().a;
        let mixed = interpolate_color(base, toward, shift).to_rgba8();
        Color::from_rgba8(mixed.r, mixed.g, mixed.b, alpha)
    }

    /// The colors of the first `count` categories.
    pub fn colors(&self, count: usize) -> Vec<Color> {
        (0..count).map(|index| self.color(index)).collect()
    }

    /// The same colors in the opposite order.
    pub fn reversed(&self) -> Self {
        Self {
            name: None,
            colors: self.colors.iter().rev().copied().collect(),
        }
    }

    /// The same colors starting `steps` later: `rotate(1)` makes the second
    /// color the first. Negative steps turn the other way.
    pub fn rotate(&self, steps: isize) -> Self {
        let mut colors = self.colors.clone();
        let len = colors.len() as isize;
        colors.rotate_left(steps.rem_euclid(len) as usize);
        Self { name: None, colors }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_palettes_resolve_ignoring_case_and_include_categorical_colormaps() {
        assert_eq!(Palette::named("Okabe_Ito").unwrap().len(), 7);
        assert_eq!(Palette::named("tab10").unwrap().len(), 10);
        assert!(Palette::named("viridis").is_err());
        assert!(Palette::names().contains(&"tab20"));
        assert_eq!(Palette::default().name(), Some("gaanim"));
        assert!(matches!(Palette::new(Vec::new()), Err(PaletteError::Empty)));
    }

    #[test]
    fn later_laps_never_repeat_a_color() {
        let palette = Palette::named("dark2").unwrap();
        let colors: Vec<_> = palette.colors(palette.len() * 4);
        for (index, color) in colors.iter().enumerate() {
            assert!(
                colors[..index]
                    .iter()
                    .all(|earlier| earlier.to_rgba8() != color.to_rgba8()),
                "category {index} repeats a color"
            );
        }
        assert_eq!(colors[0], palette.base_colors()[0]);
    }

    #[test]
    fn rotation_and_reversal_reorder_the_colors() {
        let palette = Palette::new(vec![Color::WHITE, Color::BLACK, rgb(0xFF0000)]).unwrap();
        assert_eq!(palette.rotate(1).color(0), Color::BLACK);
        assert_eq!(palette.rotate(-1).color(0), rgb(0xFF0000));
        assert_eq!(palette.reversed().color(0), rgb(0xFF0000));
    }

    #[test]
    fn a_colormap_gives_evenly_spread_colors() {
        let map = ColorMap::named("viridis").unwrap();
        let palette = Palette::from_colormap(&map, 3).unwrap();
        assert_eq!(palette.color(0), map.sample(0.0).unwrap());
        assert_eq!(palette.color(2), map.sample(1.0).unwrap());
        assert!(Palette::from_colormap(&map, 0).is_err());
    }
}
