use std::sync::Arc;

use peniko::Color;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ColorMapError {
    #[error("unknown colormap '{0}'")]
    Unknown(String),
    #[error("a colormap requires at least two colors")]
    TooFewColors,
    #[error(
        "colormap positions must match the colors, be finite and strictly increasing from 0 to 1"
    )]
    InvalidPositions,
    #[error("colormap sample positions must be finite")]
    InvalidSample,
    #[error("colormap sample count must be greater than zero")]
    InvalidCount,
    #[error("colormap alpha must be finite and between 0 and 1")]
    InvalidAlpha,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BuiltinColorMap {
    pub name: &'static str,
    pub category: &'static str,
    pub categorical: bool,
    pub colors: &'static [[u8; 3]],
}

include!("colormap_data.rs");

#[derive(Debug, Clone, Copy, PartialEq)]
struct ColorStop {
    offset: f64,
    color: Color,
}

/// A reusable continuous or categorical mapping from normalized values to colors.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorMap {
    name: Option<String>,
    category: Option<String>,
    categorical: bool,
    stops: Arc<[ColorStop]>,
}

impl ColorMap {
    /// Resolve a built-in map. Lookup is ASCII case-insensitive.
    pub fn named(name: &str) -> Result<Self, ColorMapError> {
        let builtin = BUILTINS
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| ColorMapError::Unknown(name.to_owned()))?;
        let last = builtin.colors.len() - 1;
        let stops = builtin
            .colors
            .iter()
            .enumerate()
            .map(|(index, [r, g, b])| ColorStop {
                offset: index as f64 / last as f64,
                color: Color::from_rgb8(*r, *g, *b),
            })
            .collect::<Vec<_>>();
        Ok(Self {
            name: Some(builtin.name.to_owned()),
            category: Some(builtin.category.to_owned()),
            categorical: builtin.categorical,
            stops: stops.into(),
        })
    }

    /// Build a continuous custom map. Positions default to uniform spacing.
    pub fn from_colors(
        colors: Vec<Color>,
        positions: Option<Vec<f64>>,
    ) -> Result<Self, ColorMapError> {
        if colors.len() < 2 {
            return Err(ColorMapError::TooFewColors);
        }
        let positions = positions.unwrap_or_else(|| {
            let last = colors.len() - 1;
            (0..colors.len())
                .map(|index| index as f64 / last as f64)
                .collect()
        });
        if positions.len() != colors.len()
            || positions.iter().any(|value| !value.is_finite())
            || positions.first().copied() != Some(0.0)
            || positions.last().copied() != Some(1.0)
            || positions.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(ColorMapError::InvalidPositions);
        }
        Ok(Self {
            name: None,
            category: None,
            categorical: false,
            stops: positions
                .into_iter()
                .zip(colors)
                .map(|(offset, color)| ColorStop { offset, color })
                .collect::<Vec<_>>()
                .into(),
        })
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn category(&self) -> Option<&str> {
        self.category.as_deref()
    }

    pub fn is_categorical(&self) -> bool {
        self.categorical
    }

    pub fn sample(&self, position: f64) -> Result<Color, ColorMapError> {
        if !position.is_finite() {
            return Err(ColorMapError::InvalidSample);
        }
        let position = position.clamp(0.0, 1.0);
        if self.categorical {
            let index =
                ((position * self.stops.len() as f64).floor() as usize).min(self.stops.len() - 1);
            return Ok(self.stops[index].color);
        }
        let upper = self
            .stops
            .partition_point(|stop| stop.offset < position)
            .min(self.stops.len() - 1);
        if upper == 0 {
            return Ok(self.stops[0].color);
        }
        let left = self.stops[upper - 1];
        let right = self.stops[upper];
        let width = right.offset - left.offset;
        let local = if width <= f64::EPSILON {
            0.0
        } else {
            (position - left.offset) / width
        };
        Ok(crate::interpolate_color(left.color, right.color, local))
    }

    pub fn colors(&self, count: usize) -> Result<Vec<Color>, ColorMapError> {
        if count == 0 {
            return Err(ColorMapError::InvalidCount);
        }
        if self.categorical {
            return Ok((0..count)
                .map(|index| self.stops[index % self.stops.len()].color)
                .collect());
        }
        if count == 1 {
            return Ok(vec![self.sample(0.5)?]);
        }
        (0..count)
            .map(|index| self.sample(index as f64 / (count - 1) as f64))
            .collect()
    }

    /// Sample RGBA values normalized to `0..1`, ready for vertex-color buffers.
    pub fn rgba_f32(&self, count: usize) -> Result<Vec<[f32; 4]>, ColorMapError> {
        self.colors(count).map(|colors| {
            colors
                .into_iter()
                .map(|color| {
                    let rgba = color.to_rgba8();
                    [
                        f32::from(rgba.r) / 255.0,
                        f32::from(rgba.g) / 255.0,
                        f32::from(rgba.b) / 255.0,
                        f32::from(rgba.a) / 255.0,
                    ]
                })
                .collect()
        })
    }

    pub fn reversed(&self) -> Self {
        let stops = self
            .stops
            .iter()
            .rev()
            .map(|stop| ColorStop {
                offset: 1.0 - stop.offset,
                color: stop.color,
            })
            .collect::<Vec<_>>();
        Self {
            name: self.name.as_ref().map(|name| format!("{name}_r")),
            category: self.category.clone(),
            categorical: self.categorical,
            stops: stops.into(),
        }
    }

    pub fn with_alpha(&self, alpha: f64) -> Result<Self, ColorMapError> {
        if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
            return Err(ColorMapError::InvalidAlpha);
        }
        let stops = self
            .stops
            .iter()
            .map(|stop| {
                let rgba = stop.color.to_rgba8();
                ColorStop {
                    offset: stop.offset,
                    color: Color::from_rgba8(
                        rgba.r,
                        rgba.g,
                        rgba.b,
                        (f64::from(rgba.a) * alpha).round() as u8,
                    ),
                }
            })
            .collect::<Vec<_>>();
        Ok(Self {
            name: self.name.clone(),
            category: self.category.clone(),
            categorical: self.categorical,
            stops: stops.into(),
        })
    }

    pub fn names(category: Option<&str>) -> Vec<&'static str> {
        BUILTINS
            .iter()
            .filter(|map| {
                category.is_none_or(|category| map.category.eq_ignore_ascii_case(category))
            })
            .map(|map| map.name)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_makie_scientific_and_matplotlib_catalogues() {
        assert_eq!(ColorMap::names(Some("matplotlib")).len(), 39);
        assert_eq!(ColorMap::names(Some("scientific")).len(), 39);
        assert!(ColorMap::named("VIRIDIS").is_ok());
        assert!(ColorMap::named("batlow").is_ok());
        assert!(ColorMap::named("batlow10").is_err());
    }

    #[test]
    fn samples_known_viridis_endpoints_and_reversal() {
        let map = ColorMap::named("viridis").unwrap();
        assert_eq!(
            map.sample(0.0).unwrap().to_rgba8(),
            Color::from_rgb8(68, 1, 84).to_rgba8()
        );
        assert_eq!(
            map.sample(1.0).unwrap().to_rgba8(),
            Color::from_rgb8(253, 231, 37).to_rgba8()
        );
        assert_eq!(
            map.reversed().sample(0.0).unwrap().to_rgba8(),
            map.sample(1.0).unwrap().to_rgba8()
        );
    }

    #[test]
    fn custom_positions_and_alpha_are_validated() {
        let map = ColorMap::from_colors(vec![Color::BLACK, Color::WHITE], None).unwrap();
        assert_eq!(map.colors(3).unwrap().len(), 3);
        assert_eq!(
            map.with_alpha(0.5)
                .unwrap()
                .sample(0.5)
                .unwrap()
                .to_rgba8()
                .a,
            128
        );
        assert!(ColorMap::from_colors(vec![Color::BLACK], None).is_err());
        assert!(map.sample(f64::NAN).is_err());
    }
}
