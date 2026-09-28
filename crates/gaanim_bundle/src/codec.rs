//! Lossless binary encoding of the vector and paint values a bundle stores.
//!
//! Numbers are written by their bit patterns (floats) or as LEB128 varints
//! (integers), little-endian, so every value reads back bit for bit. The
//! encoding is the bundle's own and does not follow the in-memory layout of
//! any library type, so it stays readable when those types change.

use std::sync::Arc;

use gaanim_core::{kurbo, peniko};

use crate::BundleError;

/// Append-only byte writer.
#[derive(Default)]
pub struct Writer {
    pub(crate) buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    pub fn u8(&mut self, value: u8) {
        self.buf.push(value);
    }

    pub fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    pub fn var(&mut self, mut value: u64) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                self.buf.push(byte);
                return;
            }
            self.buf.push(byte | 0x80);
        }
    }

    pub fn len(&mut self, value: usize) {
        self.var(value as u64);
    }

    pub fn ivar(&mut self, value: i64) {
        self.var(((value << 1) ^ (value >> 63)) as u64);
    }

    pub fn f32(&mut self, value: f32) {
        self.buf.extend_from_slice(&value.to_bits().to_le_bytes());
    }

    pub fn f64(&mut self, value: f64) {
        self.buf.extend_from_slice(&value.to_bits().to_le_bytes());
    }

    pub fn bytes(&mut self, value: &[u8]) {
        self.len(value.len());
        self.buf.extend_from_slice(value);
    }

    pub fn str(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    pub fn option<T>(&mut self, value: Option<T>, write: impl FnOnce(&mut Self, T)) {
        match value {
            None => self.u8(0),
            Some(value) => {
                self.u8(1);
                write(self, value);
            }
        }
    }

    pub fn point(&mut self, point: kurbo::Point) {
        self.f64(point.x);
        self.f64(point.y);
    }

    pub fn affine(&mut self, affine: kurbo::Affine) {
        for coefficient in affine.as_coeffs() {
            self.f64(coefficient);
        }
    }

    pub fn rect(&mut self, rect: kurbo::Rect) {
        self.f64(rect.x0);
        self.f64(rect.y0);
        self.f64(rect.x1);
        self.f64(rect.y1);
    }
}

/// Cursor over encoded bytes.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

type Result<T> = std::result::Result<T, BundleError>;

fn corrupt(what: &str) -> BundleError {
    BundleError::Corrupt(what.to_owned())
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// A reader over `data` that resumes at byte `pos`.
    pub fn at(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    /// Bytes read so far.
    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(count)
            .filter(|end| *end <= self.data.len())
            .ok_or_else(|| corrupt("unexpected end of data"))?;
        let bytes = &self.data[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn bool(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(corrupt("invalid boolean")),
        }
    }

    pub fn var(&mut self) -> Result<u64> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = self.u8()?;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(corrupt("varint too long"))
    }

    pub fn len(&mut self) -> Result<usize> {
        let value = self.var()?;
        usize::try_from(value)
            .ok()
            .filter(|len| *len <= self.data.len().saturating_sub(self.pos).saturating_mul(8) + 64)
            .ok_or_else(|| corrupt("length out of range"))
    }

    pub fn ivar(&mut self) -> Result<i64> {
        let value = self.var()?;
        Ok(((value >> 1) as i64) ^ -((value & 1) as i64))
    }

    pub fn u32(&mut self) -> Result<u32> {
        u32::try_from(self.var()?).map_err(|_| corrupt("u32 out of range"))
    }

    pub fn f32(&mut self) -> Result<f32> {
        let bytes: [u8; 4] = self.take(4)?.try_into().expect("four bytes");
        Ok(f32::from_bits(u32::from_le_bytes(bytes)))
    }

    pub fn f64(&mut self) -> Result<f64> {
        let bytes: [u8; 8] = self.take(8)?.try_into().expect("eight bytes");
        Ok(f64::from_bits(u64::from_le_bytes(bytes)))
    }

    pub fn bytes(&mut self) -> Result<&'a [u8]> {
        let len = self.len()?;
        self.take(len)
    }

    pub fn str(&mut self) -> Result<&'a str> {
        std::str::from_utf8(self.bytes()?).map_err(|_| corrupt("invalid UTF-8"))
    }

    pub fn option<T>(&mut self, read: impl FnOnce(&mut Self) -> Result<T>) -> Result<Option<T>> {
        match self.u8()? {
            0 => Ok(None),
            1 => read(self).map(Some),
            _ => Err(corrupt("invalid option tag")),
        }
    }

    pub fn point(&mut self) -> Result<kurbo::Point> {
        Ok(kurbo::Point::new(self.f64()?, self.f64()?))
    }

    pub fn affine(&mut self) -> Result<kurbo::Affine> {
        let mut coefficients = [0.0; 6];
        for coefficient in &mut coefficients {
            *coefficient = self.f64()?;
        }
        Ok(kurbo::Affine::new(coefficients))
    }

    pub fn rect(&mut self) -> Result<kurbo::Rect> {
        Ok(kurbo::Rect::new(
            self.f64()?,
            self.f64()?,
            self.f64()?,
            self.f64()?,
        ))
    }
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

pub fn write_path(w: &mut Writer, path: &kurbo::BezPath) {
    let elements = path.elements();
    w.len(elements.len());
    for element in elements {
        match *element {
            kurbo::PathEl::MoveTo(p) => {
                w.u8(0);
                w.point(p);
            }
            kurbo::PathEl::LineTo(p) => {
                w.u8(1);
                w.point(p);
            }
            kurbo::PathEl::QuadTo(p1, p2) => {
                w.u8(2);
                w.point(p1);
                w.point(p2);
            }
            kurbo::PathEl::CurveTo(p1, p2, p3) => {
                w.u8(3);
                w.point(p1);
                w.point(p2);
                w.point(p3);
            }
            kurbo::PathEl::ClosePath => w.u8(4),
        }
    }
}

pub fn read_path(r: &mut Reader<'_>) -> Result<kurbo::BezPath> {
    let count = r.len()?;
    let mut elements = Vec::with_capacity(count.min(1 << 20));
    for _ in 0..count {
        elements.push(match r.u8()? {
            0 => kurbo::PathEl::MoveTo(r.point()?),
            1 => kurbo::PathEl::LineTo(r.point()?),
            2 => kurbo::PathEl::QuadTo(r.point()?, r.point()?),
            3 => kurbo::PathEl::CurveTo(r.point()?, r.point()?, r.point()?),
            4 => kurbo::PathEl::ClosePath,
            _ => return Err(corrupt("invalid path element")),
        });
    }
    Ok(kurbo::BezPath::from_vec(elements))
}

// ---------------------------------------------------------------------------
// Strokes and fills
// ---------------------------------------------------------------------------

pub fn write_fill(w: &mut Writer, fill: peniko::Fill) {
    w.u8(match fill {
        peniko::Fill::NonZero => 0,
        peniko::Fill::EvenOdd => 1,
    });
}

pub fn read_fill(r: &mut Reader<'_>) -> Result<peniko::Fill> {
    match r.u8()? {
        0 => Ok(peniko::Fill::NonZero),
        1 => Ok(peniko::Fill::EvenOdd),
        _ => Err(corrupt("invalid fill rule")),
    }
}

fn write_cap(w: &mut Writer, cap: kurbo::Cap) {
    w.u8(match cap {
        kurbo::Cap::Butt => 0,
        kurbo::Cap::Square => 1,
        kurbo::Cap::Round => 2,
    });
}

fn read_cap(r: &mut Reader<'_>) -> Result<kurbo::Cap> {
    match r.u8()? {
        0 => Ok(kurbo::Cap::Butt),
        1 => Ok(kurbo::Cap::Square),
        2 => Ok(kurbo::Cap::Round),
        _ => Err(corrupt("invalid line cap")),
    }
}

pub fn write_stroke(w: &mut Writer, stroke: &kurbo::Stroke) {
    w.f64(stroke.width);
    w.u8(match stroke.join {
        kurbo::Join::Bevel => 0,
        kurbo::Join::Miter => 1,
        kurbo::Join::Round => 2,
    });
    w.f64(stroke.miter_limit);
    write_cap(w, stroke.start_cap);
    write_cap(w, stroke.end_cap);
    w.len(stroke.dash_pattern.len());
    for dash in &stroke.dash_pattern {
        w.f64(*dash);
    }
    w.f64(stroke.dash_offset);
}

pub fn read_stroke(r: &mut Reader<'_>) -> Result<kurbo::Stroke> {
    let mut stroke = kurbo::Stroke::new(r.f64()?);
    stroke.join = match r.u8()? {
        0 => kurbo::Join::Bevel,
        1 => kurbo::Join::Miter,
        2 => kurbo::Join::Round,
        _ => return Err(corrupt("invalid line join")),
    };
    stroke.miter_limit = r.f64()?;
    stroke.start_cap = read_cap(r)?;
    stroke.end_cap = read_cap(r)?;
    let dashes = r.len()?;
    stroke.dash_pattern.clear();
    for _ in 0..dashes {
        stroke.dash_pattern.push(r.f64()?);
    }
    stroke.dash_offset = r.f64()?;
    Ok(stroke)
}

// ---------------------------------------------------------------------------
// Colors and brushes
// ---------------------------------------------------------------------------

pub fn write_color(w: &mut Writer, color: peniko::Color) {
    for component in color.components {
        w.f32(component);
    }
}

pub fn read_color(r: &mut Reader<'_>) -> Result<peniko::Color> {
    Ok(peniko::Color::new([r.f32()?, r.f32()?, r.f32()?, r.f32()?]))
}

fn color_space_tag(tag: peniko::color::ColorSpaceTag) -> u8 {
    use peniko::color::ColorSpaceTag as T;
    match tag {
        T::Srgb => 0,
        T::LinearSrgb => 1,
        T::Lab => 2,
        T::Lch => 3,
        T::Hsl => 4,
        T::Hwb => 5,
        T::Oklab => 6,
        T::Oklch => 7,
        T::DisplayP3 => 8,
        T::A98Rgb => 9,
        T::ProphotoRgb => 10,
        T::Rec2020 => 11,
        T::AcesCg => 12,
        T::XyzD50 => 13,
        T::XyzD65 => 14,
        T::Aces2065_1 => 15,
        _ => 255,
    }
}

fn read_color_space_tag(r: &mut Reader<'_>) -> Result<peniko::color::ColorSpaceTag> {
    use peniko::color::ColorSpaceTag as T;
    Ok(match r.u8()? {
        0 => T::Srgb,
        1 => T::LinearSrgb,
        2 => T::Lab,
        3 => T::Lch,
        4 => T::Hsl,
        5 => T::Hwb,
        6 => T::Oklab,
        7 => T::Oklch,
        8 => T::DisplayP3,
        9 => T::A98Rgb,
        10 => T::ProphotoRgb,
        11 => T::Rec2020,
        12 => T::AcesCg,
        13 => T::XyzD50,
        14 => T::XyzD65,
        15 => T::Aces2065_1,
        _ => return Err(corrupt("unknown color space")),
    })
}

fn write_dynamic_color(w: &mut Writer, color: &peniko::color::DynamicColor) {
    w.u8(color_space_tag(color.cs));
    let missing = color.flags.missing();
    w.u8((0..4).fold(0u8, |bits, ix| {
        bits | (u8::from(missing.contains(ix)) << ix)
    }));
    for component in color.components {
        w.f32(component);
    }
}

fn read_dynamic_color(r: &mut Reader<'_>) -> Result<peniko::color::DynamicColor> {
    let cs = read_color_space_tag(r)?;
    let bits = r.u8()?;
    let mut missing = peniko::color::Missing::EMPTY;
    for ix in 0..4 {
        if bits & (1 << ix) != 0 {
            missing.insert(ix);
        }
    }
    let components = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
    Ok(peniko::color::DynamicColor {
        cs,
        flags: peniko::color::Flags::from_missing(missing),
        components,
    })
}

fn write_extend(w: &mut Writer, extend: peniko::Extend) {
    w.u8(match extend {
        peniko::Extend::Pad => 0,
        peniko::Extend::Repeat => 1,
        peniko::Extend::Reflect => 2,
    });
}

fn read_extend(r: &mut Reader<'_>) -> Result<peniko::Extend> {
    match r.u8()? {
        0 => Ok(peniko::Extend::Pad),
        1 => Ok(peniko::Extend::Repeat),
        2 => Ok(peniko::Extend::Reflect),
        _ => Err(corrupt("invalid extend mode")),
    }
}

fn write_gradient(w: &mut Writer, gradient: &peniko::Gradient) {
    match &gradient.kind {
        peniko::GradientKind::Linear(position) => {
            w.u8(0);
            w.point(position.start);
            w.point(position.end);
        }
        peniko::GradientKind::Radial(position) => {
            w.u8(1);
            w.point(position.start_center);
            w.f32(position.start_radius);
            w.point(position.end_center);
            w.f32(position.end_radius);
        }
        peniko::GradientKind::Sweep(position) => {
            w.u8(2);
            w.point(position.center);
            w.f32(position.start_angle);
            w.f32(position.end_angle);
        }
    }
    write_extend(w, gradient.extend);
    w.u8(color_space_tag(gradient.interpolation_cs));
    w.u8(match gradient.hue_direction {
        peniko::color::HueDirection::Shorter => 0,
        peniko::color::HueDirection::Longer => 1,
        peniko::color::HueDirection::Increasing => 2,
        peniko::color::HueDirection::Decreasing => 3,
        _ => 0,
    });
    w.u8(match gradient.interpolation_alpha_space {
        peniko::InterpolationAlphaSpace::Premultiplied => 0,
        peniko::InterpolationAlphaSpace::Unpremultiplied => 1,
    });
    w.len(gradient.stops.len());
    for stop in gradient.stops.iter() {
        w.f32(stop.offset);
        write_dynamic_color(w, &stop.color);
    }
}

fn read_gradient(r: &mut Reader<'_>) -> Result<peniko::Gradient> {
    let kind = match r.u8()? {
        0 => peniko::GradientKind::Linear(peniko::LinearGradientPosition {
            start: r.point()?,
            end: r.point()?,
        }),
        1 => peniko::GradientKind::Radial(peniko::RadialGradientPosition {
            start_center: r.point()?,
            start_radius: r.f32()?,
            end_center: r.point()?,
            end_radius: r.f32()?,
        }),
        2 => peniko::GradientKind::Sweep(peniko::SweepGradientPosition {
            center: r.point()?,
            start_angle: r.f32()?,
            end_angle: r.f32()?,
        }),
        _ => return Err(corrupt("invalid gradient kind")),
    };
    let extend = read_extend(r)?;
    let interpolation_cs = read_color_space_tag(r)?;
    let hue_direction = match r.u8()? {
        0 => peniko::color::HueDirection::Shorter,
        1 => peniko::color::HueDirection::Longer,
        2 => peniko::color::HueDirection::Increasing,
        3 => peniko::color::HueDirection::Decreasing,
        _ => return Err(corrupt("invalid hue direction")),
    };
    let interpolation_alpha_space = match r.u8()? {
        0 => peniko::InterpolationAlphaSpace::Premultiplied,
        1 => peniko::InterpolationAlphaSpace::Unpremultiplied,
        _ => return Err(corrupt("invalid alpha space")),
    };
    let count = r.len()?;
    let mut stops = peniko::ColorStops::default();
    for _ in 0..count {
        stops.0.push(peniko::ColorStop {
            offset: r.f32()?,
            color: read_dynamic_color(r)?,
        });
    }
    Ok(peniko::Gradient {
        kind,
        extend,
        interpolation_cs,
        hue_direction,
        interpolation_alpha_space,
        stops,
    })
}

/// Tables that deduplicate shared values while encoding.
pub trait Interner {
    fn path(&mut self, path: &Arc<kurbo::BezPath>) -> u32;
    fn image(&mut self, image: &peniko::ImageData) -> u32;
}

/// Tables of shared values while decoding.
pub trait Resolver {
    fn path(&self, index: u32) -> Result<Arc<kurbo::BezPath>>;
    fn image(&self, index: u32) -> Result<peniko::ImageData>;
}

pub fn write_sampler(w: &mut Writer, sampler: &peniko::ImageSampler) {
    write_extend(w, sampler.x_extend);
    write_extend(w, sampler.y_extend);
    w.u8(match sampler.quality {
        peniko::ImageQuality::Low => 0,
        peniko::ImageQuality::Medium => 1,
        peniko::ImageQuality::High => 2,
    });
    w.f32(sampler.alpha);
}

pub fn read_sampler(r: &mut Reader<'_>) -> Result<peniko::ImageSampler> {
    let x_extend = read_extend(r)?;
    let y_extend = read_extend(r)?;
    let quality = match r.u8()? {
        0 => peniko::ImageQuality::Low,
        1 => peniko::ImageQuality::Medium,
        2 => peniko::ImageQuality::High,
        _ => return Err(corrupt("invalid image quality")),
    };
    Ok(peniko::ImageSampler {
        x_extend,
        y_extend,
        quality,
        alpha: r.f32()?,
    })
}

pub fn write_image_brush(w: &mut Writer, tables: &mut impl Interner, brush: &peniko::ImageBrush) {
    w.var(u64::from(tables.image(&brush.image)));
    write_sampler(w, &brush.sampler);
}

pub fn read_image_brush(r: &mut Reader<'_>, tables: &impl Resolver) -> Result<peniko::ImageBrush> {
    let image = tables.image(r.u32()?)?;
    Ok(peniko::ImageBrush {
        image,
        sampler: read_sampler(r)?,
    })
}

pub fn write_brush(w: &mut Writer, tables: &mut impl Interner, brush: &peniko::Brush) {
    match brush {
        peniko::Brush::Solid(color) => {
            w.u8(0);
            write_color(w, *color);
        }
        peniko::Brush::Gradient(gradient) => {
            w.u8(1);
            write_gradient(w, gradient);
        }
        peniko::Brush::Image(image) => {
            w.u8(2);
            write_image_brush(w, tables, image);
        }
    }
}

pub fn read_brush(r: &mut Reader<'_>, tables: &impl Resolver) -> Result<peniko::Brush> {
    match r.u8()? {
        0 => Ok(peniko::Brush::Solid(read_color(r)?)),
        1 => Ok(peniko::Brush::Gradient(read_gradient(r)?)),
        2 => Ok(peniko::Brush::Image(read_image_brush(r, tables)?)),
        _ => Err(corrupt("invalid brush")),
    }
}

pub fn write_image_data(w: &mut Writer, image: &peniko::ImageData) {
    w.u8(match image.format {
        peniko::ImageFormat::Rgba8 => 0,
        peniko::ImageFormat::Bgra8 => 1,
        _ => 0,
    });
    w.u8(match image.alpha_type {
        peniko::ImageAlphaType::Alpha => 0,
        peniko::ImageAlphaType::AlphaPremultiplied => 1,
    });
    w.var(u64::from(image.width));
    w.var(u64::from(image.height));
    w.bytes(image.data.data());
}

pub fn read_image_data(r: &mut Reader<'_>) -> Result<peniko::ImageData> {
    let format = match r.u8()? {
        0 => peniko::ImageFormat::Rgba8,
        1 => peniko::ImageFormat::Bgra8,
        _ => return Err(corrupt("invalid image format")),
    };
    let alpha_type = match r.u8()? {
        0 => peniko::ImageAlphaType::Alpha,
        1 => peniko::ImageAlphaType::AlphaPremultiplied,
        _ => return Err(corrupt("invalid alpha type")),
    };
    let width = r.u32()?;
    let height = r.u32()?;
    let bytes = r.bytes()?.to_vec();
    Ok(peniko::ImageData {
        data: peniko::Blob::new(Arc::new(bytes)),
        format,
        alpha_type,
        width,
        height,
    })
}

// ---------------------------------------------------------------------------
// Blend modes
// ---------------------------------------------------------------------------

pub fn write_blend(w: &mut Writer, blend: peniko::BlendMode) {
    w.u8(blend.mix as u8);
    w.u8(blend.compose as u8);
}

pub fn read_blend(r: &mut Reader<'_>) -> Result<peniko::BlendMode> {
    use peniko::{Compose, Mix};
    let mix = match r.u8()? {
        0 => Mix::Normal,
        1 => Mix::Multiply,
        2 => Mix::Screen,
        3 => Mix::Overlay,
        4 => Mix::Darken,
        5 => Mix::Lighten,
        6 => Mix::ColorDodge,
        7 => Mix::ColorBurn,
        8 => Mix::HardLight,
        9 => Mix::SoftLight,
        10 => Mix::Difference,
        11 => Mix::Exclusion,
        12 => Mix::Hue,
        13 => Mix::Saturation,
        14 => Mix::Color,
        15 => Mix::Luminosity,
        _ => return Err(corrupt("invalid mix mode")),
    };
    let compose = match r.u8()? {
        0 => Compose::Clear,
        1 => Compose::Copy,
        2 => Compose::Dest,
        3 => Compose::SrcOver,
        4 => Compose::DestOver,
        5 => Compose::SrcIn,
        6 => Compose::DestIn,
        7 => Compose::SrcOut,
        8 => Compose::DestOut,
        9 => Compose::SrcAtop,
        10 => Compose::DestAtop,
        11 => Compose::Xor,
        12 => Compose::Plus,
        13 => Compose::PlusLighter,
        _ => return Err(corrupt("invalid compose mode")),
    };
    Ok(peniko::BlendMode { mix, compose })
}

/// Write a composed Vello scene, such as the frame of a Lottie animation:
/// its encoded streams as they are, and its gradients and images late bound.
/// Glyph runs are not written; Gaanim draws text as paths.
pub fn write_scene(
    w: &mut Writer,
    tables: &mut impl Interner,
    scene: &vello::Scene,
) -> std::result::Result<(), &'static str> {
    use vello_encoding::Patch;
    let encoding = scene.encoding();
    let resources = &encoding.resources;
    if !resources.glyph_runs.is_empty() || !resources.glyphs.is_empty() {
        return Err("glyph runs");
    }
    for count in [
        encoding.n_paths,
        encoding.n_path_segments,
        encoding.n_clips,
        encoding.n_open_clips,
        encoding.flags,
    ] {
        w.var(u64::from(count));
    }
    w.bytes(bytemuck::cast_slice(&encoding.path_tags));
    w.bytes(bytemuck::cast_slice(&encoding.path_data));
    w.bytes(bytemuck::cast_slice(&encoding.draw_tags));
    w.bytes(bytemuck::cast_slice(&encoding.draw_data));
    w.bytes(bytemuck::cast_slice(&encoding.transforms));
    w.bytes(bytemuck::cast_slice(&encoding.styles));
    w.len(resources.color_stops.len());
    for stop in &resources.color_stops {
        w.f32(stop.offset);
        write_dynamic_color(w, &stop.color);
    }
    w.len(resources.patches.len());
    for patch in &resources.patches {
        match patch {
            Patch::Ramp {
                draw_data_offset,
                stops,
                extend,
            } => {
                w.u8(0);
                w.len(*draw_data_offset);
                w.len(stops.start);
                w.len(stops.end);
                write_extend(w, *extend);
            }
            Patch::Image {
                draw_data_offset,
                image,
            } => {
                w.u8(1);
                w.len(*draw_data_offset);
                w.var(u64::from(tables.image(image)));
            }
            Patch::GlyphRun { .. } => return Err("glyph runs"),
        }
    }
    Ok(())
}

fn read_pod<T: bytemuck::Pod>(r: &mut Reader<'_>) -> Result<Vec<T>> {
    let bytes = r.bytes()?;
    if bytes.len() % std::mem::size_of::<T>() != 0 {
        return Err(corrupt("scene stream has a partial element"));
    }
    Ok(bytemuck::pod_collect_to_vec(bytes))
}

/// Read a scene written by [`write_scene`].
pub fn read_scene(r: &mut Reader<'_>, tables: &impl Resolver) -> Result<vello::Scene> {
    use vello_encoding::Patch;
    let mut scene = vello::Scene::new();
    let encoding = scene.encoding_mut();
    encoding.n_paths = r.u32()?;
    encoding.n_path_segments = r.u32()?;
    encoding.n_clips = r.u32()?;
    encoding.n_open_clips = r.u32()?;
    encoding.flags = r.u32()?;
    encoding.path_tags = read_pod(r)?;
    encoding.path_data = read_pod(r)?;
    encoding.draw_tags = read_pod(r)?;
    encoding.draw_data = read_pod(r)?;
    encoding.transforms = read_pod(r)?;
    encoding.styles = read_pod(r)?;
    let draw_bytes = encoding.draw_data.len() * 4;
    let stops = r.len()?;
    let mut color_stops = Vec::with_capacity(stops.min(1 << 16));
    for _ in 0..stops {
        let offset = r.f32()?;
        let color = read_dynamic_color(r)?;
        color_stops.push(peniko::ColorStop { offset, color });
    }
    let patches = r.len()?;
    let mut resolved = Vec::with_capacity(patches.min(1 << 16));
    for _ in 0..patches {
        let patch = match r.u8()? {
            0 => {
                let draw_data_offset = r.len()?;
                let start = r.len()?;
                let end = r.len()?;
                if start > end || end > color_stops.len() {
                    return Err(corrupt("gradient stops out of range"));
                }
                Patch::Ramp {
                    draw_data_offset,
                    stops: start..end,
                    extend: read_extend(r)?,
                }
            }
            1 => Patch::Image {
                draw_data_offset: r.len()?,
                image: tables.image(r.u32()?)?,
            },
            _ => return Err(corrupt("unknown scene resource")),
        };
        let offset = match &patch {
            Patch::Ramp {
                draw_data_offset, ..
            }
            | Patch::Image {
                draw_data_offset, ..
            } => *draw_data_offset,
            Patch::GlyphRun { .. } => 0,
        };
        if offset >= draw_bytes.max(1) {
            return Err(corrupt("scene resource outside the draw data"));
        }
        resolved.push(patch);
    }
    encoding.resources.color_stops = color_stops;
    encoding.resources.patches = resolved;
    Ok(scene)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tables(Vec<peniko::ImageData>);

    impl Interner for Tables {
        fn path(&mut self, _: &Arc<kurbo::BezPath>) -> u32 {
            0
        }
        fn image(&mut self, image: &peniko::ImageData) -> u32 {
            self.0.push(image.clone());
            (self.0.len() - 1) as u32
        }
    }

    impl Resolver for Tables {
        fn path(&self, _: u32) -> Result<Arc<kurbo::BezPath>> {
            unreachable!()
        }
        fn image(&self, index: u32) -> Result<peniko::ImageData> {
            Ok(self.0[index as usize].clone())
        }
    }

    #[test]
    fn values_read_back_bit_for_bit() {
        let mut w = Writer::new();
        for value in [0u64, 1, 127, 128, u64::MAX] {
            w.var(value);
        }
        for value in [0i64, -1, 63, -64, i64::MIN, i64::MAX] {
            w.ivar(value);
        }
        w.f64(-0.0);
        w.f64(f64::from_bits(0x7ff8_0000_dead_beef));
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        for value in [0u64, 1, 127, 128, u64::MAX] {
            assert_eq!(r.var().unwrap(), value);
        }
        for value in [0i64, -1, 63, -64, i64::MIN, i64::MAX] {
            assert_eq!(r.ivar().unwrap(), value);
        }
        assert_eq!(r.f64().unwrap().to_bits(), (-0.0f64).to_bits());
        assert_eq!(r.f64().unwrap().to_bits(), 0x7ff8_0000_dead_beef);
        assert!(r.is_empty());
    }

    #[test]
    fn paths_strokes_and_brushes_round_trip() {
        let mut path = kurbo::BezPath::new();
        path.move_to((0.1, -2.0));
        path.line_to((3.0, 4.5));
        path.quad_to((1.0, 1.0), (2.0, 0.0));
        path.curve_to((0.3, 0.2), (0.1, 0.9), (1.0 / 3.0, 7.0));
        path.close_path();
        let mut stroke = kurbo::Stroke::new(0.07)
            .with_caps(kurbo::Cap::Round)
            .with_join(kurbo::Join::Bevel)
            .with_dashes(0.25, [0.1, 0.05]);
        stroke.miter_limit = 3.5;
        let gradient = peniko::Gradient::new_radial((1.0, 2.0), 3.5)
            .with_stops([
                peniko::Color::from_rgba8(255, 0, 0, 128),
                peniko::Color::WHITE,
            ])
            .with_extend(peniko::Extend::Reflect);
        let image = peniko::ImageData {
            data: peniko::Blob::new(Arc::new(vec![1u8, 2, 3, 4])),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::AlphaPremultiplied,
            width: 1,
            height: 1,
        };
        let brushes = [
            peniko::Brush::Solid(peniko::Color::new([0.1, 0.2, 0.3, 0.4])),
            peniko::Brush::Gradient(gradient),
            peniko::Brush::Image(peniko::ImageBrush::new(image).with_alpha(0.5)),
        ];

        let mut tables = Tables(Vec::new());
        let mut w = Writer::new();
        write_path(&mut w, &path);
        write_stroke(&mut w, &stroke);
        for brush in &brushes {
            write_brush(&mut w, &mut tables, brush);
        }
        write_blend(
            &mut w,
            peniko::BlendMode::new(peniko::Mix::Screen, peniko::Compose::DestIn),
        );
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        assert_eq!(read_path(&mut r).unwrap(), path);
        assert_eq!(read_stroke(&mut r).unwrap(), stroke);
        for brush in &brushes {
            let read = read_brush(&mut r, &tables).unwrap();
            match (brush, &read) {
                (peniko::Brush::Image(a), peniko::Brush::Image(b)) => {
                    assert_eq!(a.image.data.data(), b.image.data.data());
                    assert_eq!(a.sampler, b.sampler);
                }
                _ => assert_eq!(&read, brush),
            }
        }
        assert_eq!(
            read_blend(&mut r).unwrap(),
            peniko::BlendMode::new(peniko::Mix::Screen, peniko::Compose::DestIn)
        );
        assert!(r.is_empty());
    }

    #[test]
    fn composed_scenes_round_trip_to_the_same_drawing() {
        let mut scene = vello::Scene::new();
        let gradient = peniko::Gradient::new_linear((0.0, 0.0), (4.0, 1.0)).with_stops([
            peniko::Color::from_rgba8(255, 0, 0, 200),
            peniko::Color::from_rgba8(0, 80, 255, 255),
        ]);
        let circle = kurbo::Circle::new((1.0, 1.0), 2.0);
        scene.fill(
            peniko::Fill::EvenOdd,
            kurbo::Affine::rotate(0.3),
            &gradient,
            None,
            &circle,
        );
        scene.push_clip_layer(
            peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            &kurbo::Rect::new(-1.0, -1.0, 3.0, 2.0),
        );
        scene.stroke(
            &kurbo::Stroke::new(0.25),
            kurbo::Affine::translate((0.5, 0.0)),
            peniko::Color::WHITE,
            None,
            &kurbo::Line::new((0.0, 0.0), (3.0, 2.0)),
        );
        let image = peniko::ImageData {
            data: peniko::Blob::new(Arc::new(vec![9u8, 8, 7, 255, 1, 2, 3, 255])),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::Alpha,
            width: 2,
            height: 1,
        };
        scene.draw_image(&peniko::ImageBrush::new(image), kurbo::Affine::scale(0.5));
        scene.pop_layer();

        let mut tables = Tables(Vec::new());
        let mut w = Writer::new();
        write_scene(&mut w, &mut tables, &scene).unwrap();
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        let decoded = read_scene(&mut r, &tables).unwrap();
        assert!(r.is_empty());
        assert_eq!(
            crate::digest::scene_digest(&scene),
            crate::digest::scene_digest(&decoded)
        );
        assert!(!scene.encoding().resources.patches.is_empty());
    }
}
