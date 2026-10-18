//! Filled outlines in the 3D pass ([`PlaneFillData`]): text, equations and
//! shapes lying in a drawable's plane, drawn from their quadratic curves per
//! pixel with the Slug algorithm. A pixel's coverage comes from the winding
//! numbers of a horizontal and a vertical ray through it, whose crossings
//! with the curves are solved exactly, so edges stay sharp at any size and
//! under perspective, and the depth buffer hides them behind meshes pixel by
//! pixel. Each outline's curves are split into horizontal and vertical bands
//! so that a pixel only tests the curves of the two bands it lies in.
//!
//! An outline filled with a gradient reads it per pixel from a record of
//! its geometry and a ramp of its colors, sampled as Vello samples them, so
//! it matches the 2D drawing.
//!
//! Outlines cast shadows: the shadow maps hold the texels an outline covers
//! at least half of, where it is at least half opaque.
//!
//! An outline clipped to a contour, as a stroke aligned inside or outside
//! its shape is, multiplies its coverage by that contour's, or by what the
//! contour leaves, as Vello's clip layers do.
//!
//! [`SHADER`] is a WGSL port of the reference shaders of the Slug algorithm
//! (<https://github.com/EricLengyel/Slug>), MIT licensed, copyright 2017 by
//! Eric Lengyel.

use std::collections::HashMap;
use std::sync::Arc;

use gaanim_core::glam::DVec3;
use gaanim_core::kurbo::{self, Affine, BezPath, PathEl, Point};
use gaanim_core::peniko;
use gaanim_scene::PlaneFillData;

use crate::three_d::{push_f32s, to_linear};

/// Bytes of an outline's instance (`FillIn` in [`SHADER`]): its box, its
/// placement in the plane and the offset to its clip's units, its bands'
/// transform, where its bands start, how many there are and its gradient's
/// record, its linear color, and its clip's bands' transform, start, counts
/// and side.
pub(crate) const GLYPH_BYTES: usize = 128;
/// Most bands an outline is split into, each way.
const MAX_BANDS: usize = 16;
/// How close quadratics follow a cubic, as a fraction of its outline's size.
const CUBIC_TOLERANCE: f64 = 1.0 / 4000.0;
/// Colors of a gradient's ramp, as Vello samples them.
const RAMP: usize = 512;
/// `vec4<f32>` texels of a gradient's record (`fill_paints` in [`SHADER`]).
const PAINT_TEXELS: usize = 4;

/// What the shader reads of a [`PlaneFillData`]: an instance per outline
/// drawn, and the curves and bands of its distinct outlines.
pub(crate) struct Fill {
    pub(crate) instances: Vec<u8>,
    pub(crate) count: u32,
    /// `vec4<f32>` texels of control points (`fill_curves` in [`SHADER`]).
    pub(crate) curves: Vec<u8>,
    /// `u32` words of band headers and curve lists (`fill_bands`).
    pub(crate) bands: Vec<u8>,
    /// `vec4<f32>` texels of the gradients' records (`fill_paints`); a
    /// texel of zeros when no outline has a gradient, as a binding needs one.
    pub(crate) paints: Vec<u8>,
    /// `u32` premultiplied sRGB colors of the gradients' ramps
    /// (`fill_ramps`); one when none has.
    pub(crate) ramps: Vec<u8>,
    /// The corners of the outlines' boxes in the plane.
    pub(crate) corners: Vec<DVec3>,
    pub(crate) opaque: bool,
}

/// Where an outline's bands start and how it reads them.
#[derive(Clone, Copy)]
struct Packed {
    /// Word of its band header.
    start: u32,
    /// The last vertical and horizontal band.
    last: [u32; 2],
    bounds: [f32; 4],
    /// Maps its units to band indexes: scale x and y, offset x and y.
    transform: [f32; 4],
}

/// The curves and bands of `fill`'s outlines, each distinct outline once;
/// `None` when no outline encloses anything.
pub(crate) fn build(fill: &PlaneFillData) -> Option<Fill> {
    let mut curves: Vec<[f32; 4]> = Vec::new();
    let mut bands: Vec<u32> = Vec::new();
    // Outlines by the hash of their elements, with the outline to compare.
    let mut known: HashMap<u64, Vec<(Arc<BezPath>, Option<Packed>)>> = HashMap::new();
    // The same outline again, as a text's repeated letters often are.
    let mut by_address: HashMap<*const BezPath, Option<Packed>> = HashMap::new();
    let mut instances = Vec::with_capacity(fill.glyphs.len() * GLYPH_BYTES);
    let mut corners = Vec::new();
    let mut count = 0;
    let mut opaque = true;
    let mut paints: Vec<[f32; 4]> = Vec::new();
    let mut ramps: Vec<u32> = Vec::new();
    // Ramps by their stops, which the gradients of a text's letters share.
    let mut ramp_of: HashMap<Vec<u32>, u32> = HashMap::new();
    let mut packed_of = |outline: &Arc<BezPath>| {
        *by_address.entry(Arc::as_ptr(outline)).or_insert_with(|| {
            let entries = known.entry(outline_hash(outline)).or_default();
            match entries.iter().find(|(known, _)| **known == **outline) {
                Some((_, packed)) => *packed,
                None => {
                    let packed = pack(&contours(outline), &mut curves, &mut bands);
                    entries.push((Arc::clone(outline), packed));
                    packed
                }
            }
        })
    };
    for glyph in &fill.glyphs {
        let Some(packed) = packed_of(&glyph.outline) else {
            continue;
        };
        // A contour that encloses nothing keeps nothing inside it and
        // everything outside.
        let clip = match &glyph.clip {
            None => None,
            Some(clip) => match packed_of(&clip.outline) {
                Some(packed) => Some((clip, packed)),
                None if clip.outside => None,
                None => continue,
            },
        };
        let placement = glyph.placement.as_coeffs();
        if !placement.iter().all(|value| value.is_finite()) {
            continue;
        }
        let [x0, y0, x1, y1] = packed.bounds;
        for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y1)] {
            let point = glyph.placement * Point::new(x.into(), y.into());
            corners.push(DVec3::new(point.x, point.y, 0.0));
        }
        let [a, b, c, d, e, f] = placement.map(|value| value as f32);
        let color = to_linear(glyph.color);
        // Its gradient's record, counted from 1; 0 for its color.
        let paint = glyph.gradient.as_ref().and_then(|(gradient, to_gradient)| {
            let record = paint_record(gradient, *to_gradient, &mut ramps, &mut ramp_of)?;
            paints.extend(record);
            opaque &= gradient
                .stops
                .iter()
                .all(|stop| stop.color.components[3] >= 0.999);
            Some((paints.len() / PAINT_TEXELS) as u32)
        });
        if paint.is_none() {
            opaque &= color[3] >= 0.999;
        }
        let start = instances.len();
        let offset = clip.map_or([0.0; 2], |(clip, _)| {
            [clip.offset.x as f32, clip.offset.y as f32]
        });
        push_f32s(&mut instances, &packed.bounds);
        push_f32s(&mut instances, &[a, b, c, d, e, f, offset[0], offset[1]]);
        push_f32s(&mut instances, &packed.transform);
        for word in [
            packed.start,
            packed.last[0],
            packed.last[1],
            paint.unwrap_or(0),
        ] {
            instances.extend_from_slice(&word.to_le_bytes());
        }
        push_f32s(&mut instances, &color);
        match clip {
            Some((clip, clipping)) => {
                push_f32s(&mut instances, &clipping.transform);
                let side = if clip.outside { 2 } else { 1 };
                for word in [clipping.start, clipping.last[0], clipping.last[1], side] {
                    instances.extend_from_slice(&word.to_le_bytes());
                }
            }
            None => instances.resize(instances.len() + 32, 0),
        }
        debug_assert_eq!(instances.len() - start, GLYPH_BYTES);
        count += 1;
    }
    if count == 0 {
        return None;
    }
    let mut curve_bytes = Vec::with_capacity(curves.len() * 16);
    for texel in &curves {
        push_f32s(&mut curve_bytes, texel);
    }
    let mut band_bytes = Vec::with_capacity(bands.len() * 4);
    for word in &bands {
        band_bytes.extend_from_slice(&word.to_le_bytes());
    }
    if paints.is_empty() {
        paints.push([0.0; 4]);
        ramps.push(0);
    }
    let mut paint_bytes = Vec::with_capacity(paints.len() * 16);
    for texel in &paints {
        push_f32s(&mut paint_bytes, texel);
    }
    let ramp_bytes = ramps.iter().flat_map(|color| color.to_le_bytes()).collect();
    Some(Fill {
        instances,
        count,
        curves: curve_bytes,
        bands: band_bytes,
        paints: paint_bytes,
        ramps: ramp_bytes,
        corners,
        opaque,
    })
}

/// The record of `gradient` in `fill_paints` (see [`SHADER`]), mapped from
/// an outline's units by `to_gradient`: that map's linear part's columns and
/// its offset, the gradient's kind and extend, its geometry, and its radii
/// and ramp, which is added to `ramps` unless one of the same stops is
/// there. `None` for a gradient without stops or with a non-finite value.
fn paint_record(
    gradient: &peniko::Gradient,
    to_gradient: Affine,
    ramps: &mut Vec<u32>,
    ramp_of: &mut HashMap<Vec<u32>, u32>,
) -> Option<[[f32; 4]; PAINT_TEXELS]> {
    let stops: Vec<(f32, peniko::color::PremulColor<peniko::color::Srgb>)> = gradient
        .stops
        .iter()
        .map(|stop| {
            (
                stop.offset,
                stop.color
                    .to_alpha_color::<peniko::color::Srgb>()
                    .premultiply(),
            )
        })
        .collect();
    if stops.is_empty() {
        return None;
    }
    let key: Vec<u32> = stops
        .iter()
        .flat_map(|(offset, color)| {
            std::iter::once(offset.to_bits()).chain(color.components.map(f32::to_bits))
        })
        .collect();
    let ramp = *ramp_of.entry(key).or_insert_with(|| {
        let index = (ramps.len() / RAMP) as u32;
        ramps.extend(ramp(&stops));
        index
    });
    let [a, b, c, d, e, f] = to_gradient.as_coeffs().map(|value| value as f32);
    let extend = match gradient.extend {
        peniko::Extend::Pad => 0.0,
        peniko::Extend::Repeat => 1.0,
        peniko::Extend::Reflect => 2.0,
    };
    let point = |point: kurbo::Point| [point.x as f32, point.y as f32];
    let (kind, [x0, y0], [x1, y1], radii) = match gradient.kind {
        peniko::GradientKind::Linear(line) => (0.0, point(line.start), point(line.end), [0.0; 2]),
        peniko::GradientKind::Radial(circles) => (
            1.0,
            point(circles.start_center),
            point(circles.end_center),
            [circles.start_radius, circles.end_radius],
        ),
        peniko::GradientKind::Sweep(sweep) => (
            2.0,
            point(sweep.center),
            [
                sweep.start_angle / std::f32::consts::TAU,
                sweep.end_angle / std::f32::consts::TAU,
            ],
            [0.0; 2],
        ),
    };
    let record = [
        [a, b, c, d],
        [e, f, kind, extend],
        [x0, y0, x1, y1],
        [radii[0], radii[1], ramp as f32, 0.0],
    ];
    record
        .iter()
        .flatten()
        .all(|value| value.is_finite())
        .then_some(record)
}

/// The [`RAMP`] colors of `stops`, premultiplied sRGB bytes, as Vello's
/// ramps sample them: each between the stops around it, interpolated in
/// premultiplied sRGB.
fn ramp(
    stops: &[(f32, peniko::color::PremulColor<peniko::color::Srgb>)],
) -> impl Iterator<Item = u32> + '_ {
    (0..RAMP).map(move |index| {
        let at = index as f32 / (RAMP - 1) as f32;
        let color = match stops.iter().position(|(offset, _)| *offset > at) {
            None => stops[stops.len() - 1].1,
            Some(0) => stops[0].1,
            Some(next) => {
                let (from, start) = stops[next - 1];
                let (to, end) = stops[next];
                if to - from < 1e-9 {
                    end
                } else {
                    start.lerp_rect(end, (at - from) / (to - from))
                }
            }
        };
        let bytes = color.to_rgba8();
        u32::from_le_bytes([bytes.r, bytes.g, bytes.b, bytes.a])
    })
}

fn outline_hash(outline: &BezPath) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for element in outline.elements() {
        let points: &[Point] = match element {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => &[*p],
            PathEl::QuadTo(a, b) => &[*a, *b],
            PathEl::CurveTo(a, b, c) => &[*a, *b, *c],
            PathEl::ClosePath => &[],
        };
        std::mem::discriminant(element).hash(&mut hasher);
        for point in points {
            (point.x.to_bits(), point.y.to_bits()).hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// The closed contours of `outline` as quadratic curves, each starting where
/// the one before it ends: lines as curves whose middle control point is
/// their end, cubics split into quadratics that follow them within a
/// [`CUBIC_TOLERANCE`] of the outline's size.
fn contours(outline: &BezPath) -> Vec<Vec<[Point; 3]>> {
    let bounds = kurbo::Shape::bounding_box(outline);
    let tolerance = bounds.width().max(bounds.height()) * CUBIC_TOLERANCE;
    let mut contours = Vec::new();
    let mut current: Vec<[Point; 3]> = Vec::new();
    let (mut start, mut last) = (Point::ZERO, Point::ZERO);
    let mut close = |current: &mut Vec<[Point; 3]>, last: &mut Point, start: Point| {
        if *last != start {
            current.push([*last, start, start]);
        }
        if !current.is_empty() {
            contours.push(std::mem::take(current));
        }
        *last = start;
    };
    for element in outline.elements() {
        match *element {
            PathEl::MoveTo(point) => {
                close(&mut current, &mut last, start);
                (start, last) = (point, point);
            }
            PathEl::LineTo(point) => {
                if point != last {
                    current.push([last, point, point]);
                    last = point;
                }
            }
            PathEl::QuadTo(control, point) => {
                if !(control == last && point == last) {
                    current.push([last, control, point]);
                    last = point;
                }
            }
            PathEl::CurveTo(a, b, point) => {
                let cubic = kurbo::CubicBez::new(last, a, b, point);
                let quads: Vec<kurbo::QuadBez> = if tolerance > 0.0 {
                    cubic.to_quads(tolerance).map(|(_, _, quad)| quad).collect()
                } else {
                    Vec::new()
                };
                let pieces = quads.len();
                for (index, quad) in quads.into_iter().enumerate() {
                    // Exactly where the previous piece ends, and the
                    // cubic's own end last.
                    let end = if index + 1 == pieces { point } else { quad.p2 };
                    current.push([last, quad.p1, end]);
                    last = end;
                }
                if last != point {
                    current.push([last, point, point]);
                    last = point;
                }
            }
            PathEl::ClosePath => close(&mut current, &mut last, start),
        }
    }
    close(&mut current, &mut last, start);
    contours
}

/// Append `contours`' control points to `curves` and their bands to `bands`.
/// A contour of n curves takes n + 1 texels: a curve's first two points in
/// one, its third, where the next starts, in the next one.
fn pack(
    contours: &[Vec<[Point; 3]>],
    curves: &mut Vec<[f32; 4]>,
    bands: &mut Vec<u32>,
) -> Option<Packed> {
    let mut items: Vec<(u32, [[f32; 2]; 3])> = Vec::new();
    let (mut low, mut high) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
    for contour in contours {
        for curve in contour {
            let points = curve.map(|point| [point.x as f32, point.y as f32]);
            for point in points {
                low = [low[0].min(point[0]), low[1].min(point[1])];
                high = [high[0].max(point[0]), high[1].max(point[1])];
            }
            items.push((curves.len() as u32, points));
            let [p1, p2, _] = points;
            curves.push([p1[0], p1[1], p2[0], p2[1]]);
        }
        if let Some(last) = contour.last() {
            curves.push([last[2].x as f32, last[2].y as f32, 0.0, 0.0]);
        }
    }
    let size = [high[0] - low[0], high[1] - low[1]];
    if items.is_empty() || !(size[0] > 0.0 && size[1] > 0.0) || !size.iter().all(|s| s.is_finite())
    {
        return None;
    }
    // Bands overlap by a 1024th of the outline, as the reference does.
    let overlap = size[0].max(size[1]) / 1024.0;
    // Horizontal bands split y and hold the curves that are not horizontal
    // lines, by their greatest x first; vertical ones the other way.
    let horizontal = split(&items, 1, low[1], size[1], overlap);
    let vertical = split(&items, 0, low[0], size[0], overlap);
    let start = bands.len() as u32;
    let header = 2 * (horizontal.len() + vertical.len());
    bands.resize(bands.len() + header, 0);
    // Bands that hold the same curves share their list.
    let mut lists: HashMap<Vec<u32>, u32> = HashMap::new();
    for (index, list) in horizontal.iter().chain(&vertical).enumerate() {
        let offset = *lists.entry(list.clone()).or_insert_with(|| {
            let offset = bands.len() as u32 - start;
            bands.extend_from_slice(list);
            offset
        });
        bands[start as usize + 2 * index] = list.len() as u32;
        bands[start as usize + 2 * index + 1] = offset;
    }
    let scale = [
        vertical.len() as f32 / size[0],
        horizontal.len() as f32 / size[1],
    ];
    Some(Packed {
        start,
        last: [vertical.len() as u32 - 1, horizontal.len() as u32 - 1],
        bounds: [low[0], low[1], high[0], high[1]],
        transform: [scale[0], scale[1], -low[0] * scale[0], -low[1] * scale[1]],
    })
}

/// The bands of `items` across `axis` (1 for horizontal bands, which split
/// y), from `low` over `size`: as many, up to [`MAX_BANDS`], as keep the
/// fullest band smallest, each listing the curves that reach into it, by
/// their greatest coordinate along the other axis first.
fn split(
    items: &[(u32, [[f32; 2]; 3])],
    axis: usize,
    low: f32,
    size: f32,
    overlap: f32,
) -> Vec<Vec<u32>> {
    let other = 1 - axis;
    // A line along the bands never crosses their rays.
    let crossing: Vec<&(u32, [[f32; 2]; 3])> = items
        .iter()
        .filter(|(_, points)| {
            !(points[0][axis] == points[1][axis] && points[1][axis] == points[2][axis])
        })
        .collect();
    let reach = |points: &[[f32; 2]; 3]| {
        let values = points.map(|point| point[axis]);
        (
            values[0].min(values[1]).min(values[2]),
            values[0].max(values[1]).max(values[2]),
        )
    };
    let span = |count: usize, (from, to): (f32, f32)| {
        let band = size / count as f32;
        let first = ((from - overlap - low) / band).floor().max(0.0) as usize;
        let last = (((to + overlap - low) / band).floor().max(0.0) as usize).min(count - 1);
        first.min(count - 1)..=last
    };
    let fullest = |count: usize| {
        let mut sizes = vec![0_usize; count];
        for (_, points) in &crossing {
            for band in span(count, reach(points)) {
                sizes[band] += 1;
            }
        }
        sizes.into_iter().max().unwrap_or(0)
    };
    // Each band more costs a little upload, so it must pay for itself.
    let count = (1..=MAX_BANDS)
        .min_by(|&a, &b| {
            let cost = |count: usize| fullest(count) as f32 + count as f32 * 0.25;
            cost(a).total_cmp(&cost(b))
        })
        .unwrap_or(1);
    let mut bands = vec![Vec::new(); count];
    for (texel, points) in &crossing {
        for band in span(count, reach(points)) {
            bands[band].push((*texel, points));
        }
    }
    bands
        .into_iter()
        .map(|mut band| {
            let greatest = |points: &[[f32; 2]; 3]| {
                points[0][other].max(points[1][other]).max(points[2][other])
            };
            band.sort_by(|(_, a), (_, b)| greatest(b).total_cmp(&greatest(a)));
            band.into_iter().map(|(texel, _)| texel).collect()
        })
        .collect()
}

/// The coverage of the pixel at `point` of an outline drawn `pixels_per_unit`
/// pixels per unit of its own along x and y, computed as [`SHADER`] does.
#[cfg(test)]
pub(crate) fn coverage(
    fill: &Fill,
    glyph: usize,
    point: [f32; 2],
    pixels_per_unit: [f32; 2],
) -> f32 {
    let word = |bytes: &[u8], index: usize| {
        u32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap())
    };
    let float = |bytes: &[u8], index: usize| f32::from_bits(word(bytes, index));
    let instance = &fill.instances[glyph * GLYPH_BYTES..(glyph + 1) * GLYPH_BYTES];
    let band = [12, 13, 14, 15].map(|index| float(instance, index));
    let [start, last_x, last_y] = [16, 17, 18].map(|index| word(instance, index));
    let texel =
        |index: u32| [0, 1, 2, 3].map(|part| float(&fill.curves, index as usize * 4 + part));
    let index = [
        ((point[0] * band[0] + band[2]) as i32).clamp(0, last_x as i32) as u32,
        ((point[1] * band[1] + band[3]) as i32).clamp(0, last_y as i32) as u32,
    ];
    let root_code = |y1: f32, y2: f32, y3: f32| {
        let i1 = y1.to_bits() >> 31;
        let i2 = y2.to_bits() >> 30;
        let i3 = y3.to_bits() >> 29;
        let shift = (i2 & 2) | (i1 & !2);
        let shift = (i3 & 4) | (shift & !4);
        (0x2E74_u32 >> shift) & 0x0101
    };
    // Crossings along x of the curve relative to the pixel; `axis` 1 swaps
    // x and y.
    let solve = |p: [f32; 6], axis: usize| {
        let (along, across) = (axis, 1 - axis);
        let a = [p[0] - 2.0 * p[2] + p[4], p[1] - 2.0 * p[3] + p[5]];
        let b = [p[0] - p[2], p[1] - p[3]];
        let d = (b[across] * b[across] - a[across] * p[across])
            .max(0.0)
            .sqrt();
        let (mut t1, mut t2) = ((b[across] - d) / a[across], (b[across] + d) / a[across]);
        if a[across].abs() < 1.0 / 65536.0 {
            t1 = p[across] * 0.5 / b[across];
            t2 = t1;
        }
        let at = |t: f32| (a[along] * t - b[along] * 2.0) * t + p[along];
        [at(t1), at(t2)]
    };
    let mut cov = [0.0_f32; 2];
    let mut weight = [0.0_f32; 2];
    for axis in 0..2 {
        let (along, across) = (axis, 1 - axis);
        let entry = start as usize
            + 2 * if axis == 0 {
                index[1] as usize
            } else {
                last_y as usize + 1 + index[0] as usize
            };
        let count = word(&fill.bands, entry);
        let list = start as usize + word(&fill.bands, entry + 1) as usize;
        for item in 0..count as usize {
            let at = word(&fill.bands, list + item);
            let [x1, y1, x2, y2] = texel(at);
            let [x3, y3, ..] = texel(at + 1);
            let p = [
                x1 - point[0],
                y1 - point[1],
                x2 - point[0],
                y2 - point[1],
                x3 - point[0],
                y3 - point[1],
            ];
            if p[along].max(p[along + 2]).max(p[along + 4]) * pixels_per_unit[along] < -0.5 {
                break;
            }
            let code = root_code(p[across], p[across + 2], p[across + 4]);
            if code != 0 {
                let r = solve(p, axis).map(|root| root * pixels_per_unit[along]);
                // The vertical ray counts crossings the other way round.
                let sign = if axis == 0 { 1.0 } else { -1.0 };
                if code & 1 != 0 {
                    cov[axis] += sign * (r[0] + 0.5).clamp(0.0, 1.0);
                    weight[axis] = weight[axis].max((1.0 - r[0].abs() * 2.0).clamp(0.0, 1.0));
                }
                if code > 1 {
                    cov[axis] -= sign * (r[1] + 0.5).clamp(0.0, 1.0);
                    weight[axis] = weight[axis].max((1.0 - r[1].abs() * 2.0).clamp(0.0, 1.0));
                }
            }
        }
    }
    let combined = (cov[0] * weight[0] + cov[1] * weight[1]).abs()
        / (weight[0] + weight[1]).max(1.0 / 65536.0);
    combined.max(cov[0].abs().min(cov[1].abs())).clamp(0.0, 1.0)
}

/// The vertex and fragment shaders of filled outlines, appended to
/// [`crate::three_d`]'s shader, whose frame, draw and shading they use.
pub(crate) const SHADER: &str = r#"
// Filled outlines drawn from their curves: a WGSL port of the reference
// shaders of the Slug algorithm, MIT License, copyright 2017 by Eric Lengyel
// (https://github.com/EricLengyel/Slug).

// The outlines' control points: a curve's first two in a texel, its third
// in the first two components of the next one, where the next curve of its
// contour starts.
@group(2) @binding(0) var<storage, read> fill_curves: array<vec4<f32>>;
// Each outline's bands, horizontal ones first: the count of their curves
// and where their list starts, from the outline's first word; then the
// lists, of the curves' first texels.
@group(2) @binding(1) var<storage, read> fill_bands: array<u32>;
// Each gradient fill's record, four texels: the map from its outline's
// units to the gradient's (the linear part's columns, then the offset), the
// gradient's kind (0 linear, 1 radial, 2 sweep) and extend (0 pad, 1
// repeat, 2 reflect), its points (a sweep's center and its start and end
// as fractions of a turn), and its radii and ramp.
@group(2) @binding(2) var<storage, read> fill_paints: array<vec4<f32>>;
// The gradients' ramps, 512 premultiplied sRGB colors each, as Vello's.
@group(2) @binding(3) var<storage, read> fill_ramps: array<u32>;

struct FillIn {
    // The outline's box in its own units.
    @location(0) rect: vec4<f32>,
    // Its placement in the drawable's plane: the linear part's columns,
    // then the offset, then the offset from its units to its clip's.
    @location(1) linear: vec4<f32>,
    @location(2) offset: vec4<f32>,
    // Scale and offset from its units to band indexes.
    @location(3) band: vec4<f32>,
    // Its first band word, its last vertical and horizontal band, and its
    // gradient's record, counted from 1 (0 for its color).
    @location(4) glyph: vec4<u32>,
    // Linear color.
    @location(5) color: vec4<f32>,
    // Its clip's bands' transform, and its first band word, its last
    // vertical and horizontal band, and 1 to keep what lies inside it, 2
    // outside, 0 for no clip.
    @location(6) clip_band: vec4<f32>,
    @location(7) clip: vec4<u32>,
}

struct FillOut {
    @invariant @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat) color: vec4<f32>,
    // Where the fragment lies in the outline's units.
    @location(1) outline: vec2<f32>,
    @location(2) @interpolate(flat) band: vec4<f32>,
    @location(3) @interpolate(flat) glyph: vec4<u32>,
    @location(4) world: vec3<f32>,
    @location(5) @interpolate(flat) clip_band: vec4<f32>,
    @location(6) @interpolate(flat) clip_glyph: vec4<u32>,
    @location(7) @interpolate(flat) clip_offset: vec2<f32>,
}

const FILL_CORNERS = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 1.0),
);

// How much nearer outlines lie than they are, relative to their depth: an
// outline on a surface, as a label on a floor, stays in front of it.
const FILL_DEPTH_BIAS: f32 = 1e-5;

// A corner of an outline's box, moved out by half a pixel of the image
// along the box's diagonal (the reference's dynamic dilation), so that every
// pixel the outline touches is shaded.
@vertex
fn vertex_fill(@builtin(vertex_index) index: u32, in: FillIn) -> FillOut {
    var corners = FILL_CORNERS;
    let corner = corners[index];
    let outward = corner * 2.0 - 1.0;
    let outline = mix(in.rect.xy, in.rect.zw, corner);
    let linear = mat2x2<f32>(in.linear.xy, in.linear.zw);
    let plane = linear * outline + in.offset.xy;
    let normal = linear * outward;
    // Rows of the map from the plane to the image's clip coordinates.
    let mvp = frame.view_proj * draw.model;
    let r0 = vec4<f32>(mvp[0].x, mvp[1].x, mvp[2].x, mvp[3].x);
    let r1 = vec4<f32>(mvp[0].y, mvp[1].y, mvp[2].y, mvp[3].y);
    let r3 = vec4<f32>(mvp[0].w, mvp[1].w, mvp[2].w, mvp[3].w);
    let m0 = frame.ndc_x.x * r0 + frame.ndc_x.y * r1 + frame.ndc_x.z * r3;
    let m1 = frame.ndc_y.x * r0 + frame.ndc_y.y * r1 + frame.ndc_y.z * r3;
    let dim = vec2<f32>(frame.ndc_x.w, frame.ndc_y.w) * 2.0;
    let n = normalize_or_zero(vec3<f32>(normal, 0.0)).xy;
    let s = dot(r3.xy, plane) + r3.w;
    let t = dot(r3.xy, n);
    let u = (s * dot(m0.xy, n) - t * (dot(m0.xy, plane) + m0.w)) * dim.x;
    let v = (s * dot(m1.xy, n) - t * (dot(m1.xy, plane) + m1.w)) * dim.y;
    let s2 = s * s;
    let st = s * t;
    let uv = u * u + v * v;
    // At most the box's own size: seen edge on, half a pixel is far away.
    let size = max(in.rect.z - in.rect.x, in.rect.w - in.rect.y);
    var d = size;
    let denominator = uv - st * st;
    if denominator > 0.0 {
        d = clamp(s2 * (st + sqrt(uv)) / denominator, 0.0, size);
    }
    if s <= 0.0 {
        d = 0.0;
    }
    let world = (draw.model * vec4<f32>(plane + normal * d, 0.0, 1.0)).xyz;
    var out: FillOut;
    let clip = image_clip(world);
    out.clip = vec4<f32>(clip.xy, clip.z * (1.0 + FILL_DEPTH_BIAS), clip.w);
    out.color = in.color;
    out.outline = outline + outward * d;
    out.band = in.band;
    out.glyph = in.glyph;
    out.world = world;
    out.clip_band = in.clip_band;
    out.clip_glyph = in.clip;
    out.clip_offset = in.offset.zw;
    return out;
}

// Which roots of a curve, relative to the pixel, cross the ray: bit 0 for
// the first, bit 8 for the second, from the signs of its control points.
fn fill_root_code(y1: f32, y2: f32, y3: f32) -> u32 {
    let i1 = bitcast<u32>(y1) >> 31u;
    let i2 = bitcast<u32>(y2) >> 30u;
    let i3 = bitcast<u32>(y3) >> 29u;
    var shift = (i2 & 2u) | (i1 & ~2u);
    shift = (i3 & 4u) | (shift & ~4u);
    return (0x2E74u >> shift) & 0x0101u;
}

// Where a curve, relative to the pixel, crosses y = 0, along x.
fn fill_solve_horizontal(p12: vec4<f32>, p3: vec2<f32>) -> vec2<f32> {
    let a = p12.xy - p12.zw * 2.0 + p3;
    let b = p12.xy - p12.zw;
    let d = sqrt(max(b.y * b.y - a.y * p12.y, 0.0));
    var t1 = (b.y - d) / a.y;
    var t2 = (b.y + d) / a.y;
    // Nearly linear: solve -2b t + c = 0.
    if abs(a.y) < 1.0 / 65536.0 {
        t1 = p12.y * 0.5 / b.y;
        t2 = t1;
    }
    return vec2<f32>((a.x * t1 - b.x * 2.0) * t1 + p12.x, (a.x * t2 - b.x * 2.0) * t2 + p12.x);
}

// Where a curve, relative to the pixel, crosses x = 0, along y.
fn fill_solve_vertical(p12: vec4<f32>, p3: vec2<f32>) -> vec2<f32> {
    let a = p12.xy - p12.zw * 2.0 + p3;
    let b = p12.xy - p12.zw;
    let d = sqrt(max(b.x * b.x - a.x * p12.x, 0.0));
    var t1 = (b.x - d) / a.x;
    var t2 = (b.x + d) / a.x;
    if abs(a.x) < 1.0 / 65536.0 {
        t1 = p12.x * 0.5 / b.x;
        t2 = t1;
    }
    return vec2<f32>((a.y * t1 - b.y * 2.0) * t1 + p12.y, (a.y * t2 - b.y * 2.0) * t2 + p12.y);
}

// How much of the pixel at `coord`, in the outline's units, the outline
// covers, with the nonzero rule.
// Pixels per unit along each ray at `coord`: across the pixel's footprint
// in the outline's units, through its middle, along that axis. Facing the
// camera that is `fwidth`; seen at a grazing angle under perspective the
// footprint is a long slanted parallelogram, whose width along x `fwidth`
// overstates, smearing the text sideways. Called in uniform control flow,
// as derivatives must be.
fn fill_pixels(coord: vec2<f32>) -> vec2<f32> {
    let across_x = dpdx(coord);
    let across_y = dpdy(coord);
    let area = abs(across_x.x * across_y.y - across_x.y * across_y.x);
    let reach = max(abs(across_x), abs(across_y));
    let chord = max(area / max(reach.yx, vec2<f32>(1e-30)), fwidth(coord) * 1e-3);
    return 1.0 / chord;
}

// The coverage at `coord` of the outline whose bands `band` and `glyph`
// locate, at `pixels` per unit (`fill_pixels`).
fn fill_coverage(coord: vec2<f32>, band: vec4<f32>, glyph: vec4<u32>, pixels: vec2<f32>) -> f32 {
    let last = vec2<i32>(i32(glyph.y), i32(glyph.z));
    let index = clamp(vec2<i32>(coord * band.xy + band.zw), vec2<i32>(0), last);
    let start = glyph.x;

    var xcov = 0.0;
    var xwgt = 0.0;
    let h_entry = start + 2u * u32(index.y);
    let h_count = fill_bands[h_entry];
    let h_list = start + fill_bands[h_entry + 1u];
    for (var i = 0u; i < h_count; i = i + 1u) {
        let at = fill_bands[h_list + i];
        let p12 = fill_curves[at] - vec4<f32>(coord, coord);
        let p3 = fill_curves[at + 1u].xy - coord;
        // Sorted by their greatest x: the rest lie left of the pixel.
        if max(max(p12.x, p12.z), p3.x) * pixels.x < -0.5 {
            break;
        }
        let code = fill_root_code(p12.y, p12.w, p3.y);
        if code != 0u {
            let r = fill_solve_horizontal(p12, p3) * pixels.x;
            if (code & 1u) != 0u {
                xcov += saturate(r.x + 0.5);
                xwgt = max(xwgt, saturate(1.0 - abs(r.x) * 2.0));
            }
            if code > 1u {
                xcov -= saturate(r.y + 0.5);
                xwgt = max(xwgt, saturate(1.0 - abs(r.y) * 2.0));
            }
        }
    }

    var ycov = 0.0;
    var ywgt = 0.0;
    let v_entry = start + 2u * (glyph.z + 1u + u32(index.x));
    let v_count = fill_bands[v_entry];
    let v_list = start + fill_bands[v_entry + 1u];
    for (var i = 0u; i < v_count; i = i + 1u) {
        let at = fill_bands[v_list + i];
        let p12 = fill_curves[at] - vec4<f32>(coord, coord);
        let p3 = fill_curves[at + 1u].xy - coord;
        if max(max(p12.y, p12.w), p3.y) * pixels.y < -0.5 {
            break;
        }
        let code = fill_root_code(p12.x, p12.z, p3.x);
        if code != 0u {
            let r = fill_solve_vertical(p12, p3) * pixels.y;
            if (code & 1u) != 0u {
                ycov -= saturate(r.x + 0.5);
                ywgt = max(ywgt, saturate(1.0 - abs(r.x) * 2.0));
            }
            if code > 1u {
                ycov += saturate(r.y + 0.5);
                ywgt = max(ywgt, saturate(1.0 - abs(r.y) * 2.0));
            }
        }
    }

    // Both rays' coverage, weighted by how near their crossings are; the
    // absolute values take either winding direction.
    let coverage = max(
        abs(xcov * xwgt + ycov * ywgt) / max(xwgt + ywgt, 1.0 / 65536.0),
        min(abs(xcov), abs(ycov)),
    );
    return saturate(coverage);
}

// Vello's extend modes, on a gradient's parameter.
fn fill_extend(t: f32, mode: u32) -> f32 {
    switch mode {
        case 0u: {
            return clamp(t, 0.0, 1.0);
        }
        case 1u: {
            return fract(t);
        }
        default: {
            return abs(t - 2.0 * round(0.5 * t));
        }
    }
}

fn fill_srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

// The linear color of gradient record `paint` at `at`, in the outline's
// units; transparent where a radial gradient has no circle through it.
fn fill_gradient(paint: u32, at: vec2<f32>) -> vec4<f32> {
    let base = (paint - 1u) * 4u;
    let m = fill_paints[base];
    let o = fill_paints[base + 1u];
    let g = fill_paints[base + 2u];
    let h = fill_paints[base + 3u];
    let p = m.xy * at.x + m.zw * at.y + o.xy;
    var t = 0.0;
    switch u32(o.z) {
        case 0u: {
            let d = g.zw - g.xy;
            t = dot(p - g.xy, d) / max(dot(d, d), 1e-20);
        }
        case 1u: {
            // The greatest t whose circle, from the start one toward the
            // end one, passes through p with a radius of at least 0.
            let dc = g.zw - g.xy;
            let dr = h.y - h.x;
            let q = p - g.xy;
            let a = dot(dc, dc) - dr * dr;
            let b = dot(q, dc) + h.x * dr;
            let c = dot(q, q) - h.x * h.x;
            if abs(a) < 1e-9 {
                if abs(b) < 1e-9 {
                    return vec4<f32>(0.0);
                }
                t = c / (2.0 * b);
                if h.x + t * dr < 0.0 {
                    return vec4<f32>(0.0);
                }
            } else {
                let discriminant = b * b - a * c;
                if discriminant < 0.0 {
                    return vec4<f32>(0.0);
                }
                let root = sqrt(discriminant);
                let high = max((b + root) / a, (b - root) / a);
                let low = min((b + root) / a, (b - root) / a);
                if h.x + high * dr >= 0.0 {
                    t = high;
                } else if h.x + low * dr >= 0.0 {
                    t = low;
                } else {
                    return vec4<f32>(0.0);
                }
            }
        }
        default: {
            var turn = atan2(p.y - g.y, p.x - g.x) / 6.283185307179586;
            if turn < 0.0 {
                turn += 1.0;
            }
            t = (turn - g.z) / (g.w - g.z);
        }
    }
    let slot = u32(round(fill_extend(t, u32(o.w)) * 511.0));
    let color = unpack4x8unorm(fill_ramps[u32(h.z) * 512u + slot]);
    if color.a <= 0.0 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(fill_srgb_to_linear(color.rgb / color.a), color.a);
}

@fragment
fn fragment_fill(in: FillOut) -> @location(0) vec4<f32> {
    let pixels = fill_pixels(in.outline);
    let coverage = fill_clipped(in.outline, in.band, in.glyph, in.clip_offset, in.clip_band, in.clip_glyph, pixels);
    var color = in.color;
    if in.glyph.w != 0u {
        color = fill_gradient(in.glyph.w, in.outline);
    }
    if draw.material.z > 0.5 {
        // Lit as the plane, from the side the viewer sees.
        let viewer = toward_viewer(in.world);
        var normal = normalize_or_zero((draw.normal * vec4<f32>(0.0, 0.0, 1.0, 0.0)).xyz);
        if dot(normal, viewer) < 0.0 {
            normal = -normal;
        }
        color = shade(color, normal, normal, viewer, in.world, in.clip.xy);
    }
    if coverage <= 0.0 {
        discard;
    }
    return output(color) * coverage;
}

// The coverage of an outline at `coord`, kept within its clip, or outside
// it (see `FillIn`).
fn fill_clipped(
    coord: vec2<f32>,
    band: vec4<f32>,
    glyph: vec4<u32>,
    clip_offset: vec2<f32>,
    clip_band: vec4<f32>,
    clip: vec4<u32>,
    pixels: vec2<f32>,
) -> f32 {
    let coverage = fill_coverage(coord, band, glyph, pixels);
    if clip.w == 0u || coverage <= 0.0 {
        return coverage;
    }
    let inside = fill_coverage(coord + clip_offset, clip_band, clip, pixels);
    return coverage * select(1.0 - inside, inside, clip.w == 1u);
}

struct FillShadowOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) @interpolate(flat) alpha: f32,
    @location(1) outline: vec2<f32>,
    @location(2) @interpolate(flat) band: vec4<f32>,
    @location(3) @interpolate(flat) glyph: vec4<u32>,
    @location(4) @interpolate(flat) clip_band: vec4<f32>,
    @location(5) @interpolate(flat) clip: vec4<u32>,
    @location(6) @interpolate(flat) clip_offset: vec2<f32>,
}

// An outline's box in a shadow map layer, grown by a 32nd of its size so
// that the texels along its edges are tested too.
@vertex
fn vertex_fill_shadow(@builtin(vertex_index) index: u32, in: FillIn) -> FillShadowOut {
    var corners = FILL_CORNERS;
    let corner = corners[index];
    let size = max(in.rect.z - in.rect.x, in.rect.w - in.rect.y);
    let outline = mix(in.rect.xy, in.rect.zw, corner) + (corner * 2.0 - 1.0) * size / 32.0;
    let linear = mat2x2<f32>(in.linear.xy, in.linear.zw);
    let plane = linear * outline + in.offset.xy;
    var out: FillShadowOut;
    out.clip_position = caster.view_proj * (draw.model * vec4<f32>(plane, 0.0, 1.0));
    out.alpha = in.color.a;
    out.outline = outline;
    out.band = in.band;
    out.glyph = in.glyph;
    out.clip_band = in.clip_band;
    out.clip = in.clip;
    out.clip_offset = in.offset.zw;
    return out;
}

// The pixels in which an outline hides what lies behind it from the
// translucent layers drawn after it: those it covers at least half of, at
// least half opaque, its drawable's opacity included. Its color writes no
// depth, as its edges blend over what lies behind them.
@fragment
fn fragment_fill_depth(in: FillOut) {
    var alpha = in.color.a;
    if in.glyph.w != 0u {
        alpha = fill_gradient(in.glyph.w, in.outline).a;
    }
    let pixels = fill_pixels(in.outline);
    let coverage = fill_clipped(in.outline, in.band, in.glyph, in.clip_offset, in.clip_band, in.clip_glyph, pixels);
    if coverage * alpha * draw.emissive.w < 0.5 {
        discard;
    }
}

// The texels an outline casts a shadow from: those it covers at least half
// of, at least half opaque, its drawable's opacity included.
@fragment
fn fragment_fill_shadow(in: FillShadowOut) {
    var alpha = in.alpha;
    if in.glyph.w != 0u {
        alpha = fill_gradient(in.glyph.w, in.outline).a;
    }
    let pixels = fill_pixels(in.outline);
    let coverage = fill_clipped(in.outline, in.band, in.glyph, in.clip_offset, in.clip_band, in.clip, pixels);
    if coverage * alpha * draw.emissive.w < 0.5 {
        discard;
    }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_core::kurbo::{Circle, Rect, Shape};
    use gaanim_scene::PlaneGlyph;

    #[test]
    fn gradients_are_recorded_with_ramps_shared_by_their_stops() {
        let (red, blue) = (
            peniko::Color::from_rgb8(255, 0, 0),
            peniko::Color::from_rgb8(0, 0, 255),
        );
        let gradient =
            Arc::new(peniko::Gradient::new_linear((0.0, 0.0), (1.0, 0.0)).with_stops([red, blue]));
        let square = Arc::new(Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.0));
        let glyph = |x: f64, gradient: Option<Arc<peniko::Gradient>>| PlaneGlyph {
            outline: Arc::clone(&square),
            placement: Affine::translate((x, 0.0)),
            color: red,
            gradient: gradient.map(|gradient| (gradient, Affine::translate((x, 0.0)))),
            clip: None,
        };
        let fill = build(&PlaneFillData {
            glyphs: vec![
                glyph(0.0, None),
                glyph(2.0, Some(Arc::clone(&gradient))),
                glyph(4.0, Some(gradient)),
            ],
        })
        .unwrap();
        // Each outline names its record, counted from 1, or 0 for its color.
        let paint = |glyph: usize| {
            let at = glyph * GLYPH_BYTES + 76;
            u32::from_le_bytes(fill.instances[at..at + 4].try_into().unwrap())
        };
        assert_eq!([paint(0), paint(1), paint(2)], [0, 1, 2]);
        assert_eq!(fill.paints.len(), 2 * PAINT_TEXELS * 16);
        // Both records share one ramp, red to blue in premultiplied sRGB.
        assert_eq!(fill.ramps.len(), RAMP * 4);
        assert_eq!(fill.ramps[..4], [255, 0, 0, 255]);
        assert_eq!(fill.ramps[(RAMP - 1) * 4..], [0, 0, 255, 255]);
        let middle = &fill.ramps[(RAMP / 2) * 4..(RAMP / 2) * 4 + 4];
        assert!(
            (126..=129).contains(&middle[0]) && middle[1] == 0,
            "{middle:?}"
        );
        // The second record's map starts the gradient at its outline's corner.
        let texel = |index: usize| -> [f32; 4] {
            std::array::from_fn(|lane| {
                let at = index * 16 + lane * 4;
                f32::from_le_bytes(fill.paints[at..at + 4].try_into().unwrap())
            })
        };
        assert_eq!(texel(PAINT_TEXELS + 1)[..2], [4.0, 0.0]);
    }

    fn fill_of(outlines: &[BezPath]) -> Fill {
        let fill = PlaneFillData {
            glyphs: outlines
                .iter()
                .map(|outline| PlaneGlyph {
                    outline: Arc::new(outline.clone()),
                    placement: Affine::IDENTITY,
                    color: peniko::Color::WHITE,
                    gradient: None,
                    clip: None,
                })
                .collect(),
        };
        build(&fill).unwrap()
    }

    #[test]
    fn a_square_covers_its_inside_and_half_of_its_edge() {
        let square = Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.0);
        let fill = fill_of(&[square]);
        // 64 pixels per unit.
        let at = |x: f32, y: f32| coverage(&fill, 0, [x, y], [64.0; 2]);
        assert_eq!(at(0.5, 0.5), 1.0);
        assert_eq!(at(0.1, 0.9), 1.0);
        assert_eq!(at(-0.1, 0.5), 0.0);
        assert_eq!(at(0.5, 1.2), 0.0);
        // On the left edge, half a pixel in.
        assert!((at(0.0, 0.5) - 0.5).abs() < 0.01, "{}", at(0.0, 0.5));
        assert!((at(1.0 - 0.25 / 64.0, 0.5) - 0.75).abs() < 0.01);
    }

    #[test]
    fn a_ring_leaves_its_hole_empty_and_a_cubic_circle_stays_round() {
        // Kurbo's circles are cubics, split into quadratics here.
        let mut ring = Circle::new((0.0, 0.0), 1.0).to_path(1e-3);
        let hole = Circle::new((0.0, 0.0), 0.5).to_path(1e-3);
        ring.extend(hole.reverse_subpaths());
        let fill = fill_of(&[ring]);
        let at = |x: f32, y: f32| coverage(&fill, 0, [x, y], [200.0; 2]);
        assert_eq!(at(0.0, 0.0), 0.0);
        assert_eq!(at(0.75, 0.0), 1.0);
        assert_eq!(at(0.0, -0.75), 1.0);
        assert_eq!(at(0.53, 0.53), 1.0);
        assert_eq!(at(1.1, 0.0), 0.0);
        // The edge lies within a quarter of a pixel of the circle.
        for angle in [0.3_f32, 1.1, 2.0, 4.0] {
            let (sin, cos) = angle.sin_cos();
            let edge = at(cos, sin);
            assert!((edge - 0.5).abs() < 0.25, "{angle}: {edge}");
        }
    }

    #[test]
    fn a_stem_seen_at_a_grazing_angle_stays_solid() {
        // A stem 0.1 wide and 1 tall, drawn 100 pixels per unit across and
        // 4 along its height, as a plane seen 88 degrees from its normal.
        let stem = Rect::new(0.0, 0.0, 0.1, 1.0).to_path(0.0);
        let fill = fill_of(&[stem]);
        let at = |x: f32, y: f32| coverage(&fill, 0, [x, y], [100.0, 4.0]);
        assert_eq!(at(0.05, 0.5), 1.0);
        assert_eq!(at(0.05, 0.25), 1.0);
        assert_eq!(at(0.15, 0.5), 0.0);
        // Its top lies a fifth of a pixel above: 0.7 covered.
        assert!((at(0.05, 0.95) - 0.7).abs() < 0.05, "{}", at(0.05, 0.95));
    }

    #[test]
    fn outlines_alike_share_their_curves_and_bands() {
        let square = Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.0);
        let one = fill_of(std::slice::from_ref(&square));
        let three = fill_of(&[square.clone(), square.clone(), square]);
        assert_eq!(three.count, 3);
        assert_eq!(three.curves.len(), one.curves.len());
        assert_eq!(three.bands.len(), one.bands.len());
        assert_eq!(three.instances.len(), 3 * GLYPH_BYTES);
    }

    #[test]
    fn bands_split_a_detailed_outline() {
        // A comb of 12 teeth: one band each way would hold every curve.
        let mut comb = BezPath::new();
        comb.move_to((0.0, 0.0));
        for tooth in 0..12 {
            let x = f64::from(tooth);
            comb.line_to((x, 4.0));
            comb.line_to((x + 0.5, 4.0));
            comb.line_to((x + 0.5, 1.0));
            comb.line_to((x + 1.0, 1.0));
        }
        comb.line_to((12.0, 0.0));
        comb.close_path();
        let fill = fill_of(std::slice::from_ref(&comb));
        let word = |index: usize| {
            u32::from_le_bytes(fill.bands[index * 4..index * 4 + 4].try_into().unwrap())
        };
        let instance = &fill.instances[..GLYPH_BYTES];
        let last_x = u32::from_le_bytes(instance[68..72].try_into().unwrap());
        let last_y = u32::from_le_bytes(instance[72..76].try_into().unwrap());
        assert!(last_x > 0, "the vertical bands split x");
        let curves = (fill.curves.len() / 16) as u32;
        // A horizontal ray through the teeth crosses all their sides, but
        // a vertical band holds the few curves around its teeth.
        let fullest = (0..=last_x as usize)
            .map(|band| word(2 * (last_y as usize + 1 + band)))
            .max()
            .unwrap();
        assert!(fullest < curves / 4, "{fullest} of {curves}");
        // Every tooth and every gap reads right.
        let at = |x: f32, y: f32| coverage(&fill, 0, [x, y], [32.0; 2]);
        for tooth in 0..12 {
            let x = tooth as f32;
            assert_eq!(at(x + 0.25, 3.0), 1.0, "tooth {tooth}");
            assert_eq!(at(x + 0.75, 3.0), 0.0, "gap {tooth}");
            assert_eq!(at(x + 0.75, 0.5), 1.0, "base {tooth}");
        }
    }
}
