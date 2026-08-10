//! Compile — replay Canvas ops into SceneBuilder/Bevy.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use gaanim_core::ObjectId;
use gaanim_core::glam::{DVec2, DVec3};
use gaanim_core::kurbo::{Point, Rect, Shape, Vec2};
use gaanim_core::peniko::Color as PenikoColor;
use gaanim_math::{Bounds3D, GlobalSpatialTransform};
use gaanim_scene::{
    FillBrush, GlobalOpacity, GltfModelRoot, GltfNodeBinding, GltfNodeWrapper, GroupMarker,
    LocalBounds, MobjectId, ObjectTag, Opacity, RenderLayer, RenderOrder, StrokeBrush, Visible,
    WorldBounds,
};
use gaanim_timeline::clip::SceneId;
use gaanim_timeline::timeline::{SegmentMetadata, SegmentStop, Timeline};

use crate::anim::{AnimationBuilder, AnimationType};
use crate::builder::{EquationTransitionMode, MobjectRef, MobjectState, SceneBuilder};
use crate::canvas::canvas_impl::Canvas;
use crate::canvas::ops::{CanvasEndpoint, FragmentRevealStyle, Op, Segment};
use crate::canvas::types::{
    AxesConfig, LayoutOp, LayoutWithin, ObjectSpec, ParagraphOptions, ParagraphOverflow, SpawnKind,
    TextAlign,
};

use gaanim_animation::{
    CurvatureOnCurve, NormalOnCurve, PointOnCurve, PositionBinding, TangentOnCurve, TracedPath,
    TrackingEndpoint, TrackingLine, Updater,
};
use gaanim_math::{RateFunc, SpatialTransform};

struct CompiledLayoutMeasure(BTreeMap<gaanim_layout::LayoutId, DVec2>);

impl gaanim_layout::IntrinsicMeasure for CompiledLayoutMeasure {
    fn measure(
        &self,
        id: gaanim_layout::LayoutId,
        constraints: gaanim_layout::BoxConstraints,
    ) -> Result<DVec2, gaanim_layout::LayoutError> {
        Ok(constraints.constrain(*self.0.get(&id).unwrap_or(&DVec2::ZERO)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompiledObjectScope {
    Segment(SceneId),
    Persistent,
}

#[derive(Clone, Copy, Debug)]
enum SceneObjectScopeAction {
    Reuse,
    Persist,
    Release,
}

fn escape_typst_string(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

fn typst_foreground_for_background(background: gaanim_core::peniko::Color) -> &'static str {
    let rgba = background.to_rgba8();
    let luminance =
        (0.2126 * f64::from(rgba.r) + 0.7152 * f64::from(rgba.g) + 0.0722 * f64::from(rgba.b))
            / 255.0;
    if luminance > 0.5 { "000000" } else { "ffffff" }
}

pub(crate) fn split_text_math(text: &str) -> Vec<(bool, String)> {
    let mut segments: Vec<(bool, String)> = Vec::new();
    let mut buf = String::new();
    let mut in_math = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next == '$' {
                    chars.next();
                    buf.push('$');
                } else if next == '\\' {
                    chars.next();
                    buf.push('\\');
                } else {
                    buf.push(c);
                }
            } else {
                buf.push(c);
            }
        } else if c == '$' {
            let is_double = chars.peek() == Some(&'$');
            if is_double {
                chars.next();
            }
            if !in_math {
                if !buf.is_empty() {
                    segments.push((false, std::mem::take(&mut buf)));
                }
                in_math = true;
            } else {
                if !buf.is_empty() {
                    segments.push((true, std::mem::take(&mut buf)));
                } else {
                    segments.push((true, String::new()));
                }
                in_math = false;
            }
        } else {
            buf.push(c);
        }
    }
    if !buf.is_empty() {
        if in_math {
            // Unclosed `$` — treat the opening delimiter and trailing content as literal text.
            segments.push((false, format!("${buf}")));
        } else {
            segments.push((false, buf));
        }
    }
    if segments.is_empty() {
        segments.push((false, String::new()));
    }
    segments
}

pub(crate) fn typst_inline_content(text: &str) -> String {
    if !text.contains('$') {
        return format!("#text(\"{}\")", escape_typst_string(text));
    }
    let segments = split_text_math(text);
    let has_math = segments.iter().any(|(is_math, _)| *is_math);
    if !has_math {
        return format!("#text(\"{}\")", escape_typst_string(text));
    }
    let mut parts = Vec::new();
    for (is_math, content) in segments {
        if content.is_empty() {
            continue;
        }
        if is_math {
            let trimmed = content.trim();
            if trimmed.is_empty() {
                continue;
            }
            parts.push(format!("${}$", trimmed));
        } else {
            parts.push(format!("#text(\"{}\")", escape_typst_string(&content)));
        }
    }
    if parts.is_empty() {
        return format!("#text(\"{}\")", escape_typst_string(text));
    }
    parts.join("")
}

fn color_to_hex(color: gaanim_core::peniko::Color) -> String {
    let rgba = color.to_rgba8();
    format!("{:02x}{:02x}{:02x}", rgba.r, rgba.g, rgba.b)
}

pub(crate) fn text_inline_typst_source(text: &str, color: gaanim_core::peniko::Color) -> String {
    let content = typst_inline_content(text);
    let hex = color_to_hex(color);
    format!(
        "#set page(width: auto, height: auto, margin: 0pt)\n\
         #set text(fill: rgb(\"{hex}\"))\n\
         #align(left)[{content}]"
    )
}

pub(crate) fn paragraph_typst_source(
    text: &str,
    options: &ParagraphOptions,
    font_size: f64,
    color: gaanim_core::peniko::Color,
) -> String {
    let width = options.width.unwrap_or(640.0).max(1.0);
    let leading = font_size * (options.line_spacing.max(1.0) - 1.0);
    let (alignment, justify) = match options.align {
        TextAlign::Left => ("left", false),
        TextAlign::Center => ("center", false),
        TextAlign::Right => ("right", false),
        TextAlign::Justify => ("left", true),
    };
    let hex = color_to_hex(color);
    let content = format!("#align({alignment})[{}]", typst_inline_content(text));
    let content = if let Some(max_lines) = options.max_lines.filter(|lines| *lines > 0) {
        let height = font_size * options.line_spacing.max(1.0) * max_lines as f64;
        let clip = matches!(options.overflow, ParagraphOverflow::Clip);
        format!("#block(width: 100%, height: {height}pt, clip: {clip})[{content}]")
    } else {
        content
    };
    format!(
        "#set page(width: {width}pt, height: auto, margin: 0pt)\n\
         #set text(fill: rgb(\"{hex}\"))\n\
         #set par(justify: {justify}, leading: {leading}pt)\n\
         {content}",
    )
}

impl Canvas {
    fn axis_path(
        builder: &mut SceneBuilder<'_, '_, '_>,
        path: gaanim_core::kurbo::BezPath,
        bounds: Bounds3D,
        color: PenikoColor,
        width: f64,
        tag: &str,
    ) -> MobjectRef {
        let path = gaanim_objects::prelude::SvgPath {
            id: tag.to_owned(),
            path,
            bounds,
            fill: None,
            stroke: StrokeBrush::new(color, width),
        };
        builder.svg_path(&path).spawn()
    }

    fn axis_text(
        builder: &mut SceneBuilder<'_, '_, '_>,
        text: &str,
        x: f64,
        y: f64,
        color: PenikoColor,
    ) -> MobjectRef {
        let label = builder.body(text);
        if let Some(state) = builder.states.get_mut(label.id) {
            state.transform = state.transform.shift_2d(x, y);
            builder
                .commands
                .entity(state.entity)
                .insert(state.transform);
        }
        builder.select(label, text).set_fill(color);
        label
    }

    fn styled_axes(
        builder: &mut SceneBuilder<'_, '_, '_>,
        x_range: (f64, f64, f64),
        y_range: (f64, f64, f64),
        config: &AxesConfig,
        frame_bounds: Bounds3D,
    ) -> MobjectRef {
        let (x_min, x_max, x_step) = x_range;
        let (y_min, y_max, y_step) = y_range;
        // Manim-compatible sizing: x_length/y_length override auto_fit, otherwise map to safe_frame.
        // x_length/y_length are manim units (default frame 14.222x8), convert to scene units via avail_w/h
        let avail_w = frame_bounds.width().max(1.0);
        let avail_h = frame_bounds.height().max(1.0);
        let manim_frame_w: f64 = 14.222222222222221;
        let manim_frame_h: f64 = 8.0;
        let (scale_x, scale_y, x_center, y_center) = match (config.x_length, config.y_length) {
            (Some(xl), Some(yl)) => {
                let scene_xl = xl * avail_w / manim_frame_w;
                let scene_yl = yl * avail_h / manim_frame_h;
                let sx = scene_xl / (x_max - x_min).max(1e-9);
                let sy = scene_yl / (y_max - y_min).max(1e-9);
                (sx, sy, (x_min + x_max) * 0.5, (y_min + y_max) * 0.5)
            }
            (Some(xl), None) => {
                let scene_xl = xl * avail_w / manim_frame_w;
                let s = scene_xl / (x_max - x_min).max(1e-9);
                (s, s, (x_min + x_max) * 0.5, (y_min + y_max) * 0.5)
            }
            (None, Some(yl)) => {
                let scene_yl = yl * avail_h / manim_frame_h;
                let s = scene_yl / (y_max - y_min).max(1e-9);
                (s, s, (x_min + x_max) * 0.5, (y_min + y_max) * 0.5)
            }
            (None, None) if config.auto_fit => {
                let data_w = (x_max - x_min).max(1e-9);
                let data_h = (y_max - y_min).max(1e-9);
                let s = (avail_w / data_w).min(avail_h / data_h);
                (s, s, (x_min + x_max) * 0.5, (y_min + y_max) * 0.5)
            }
            (None, None) => (1.0, 1.0, 0.0, 0.0),
        };
        let sx = |x: f64| (x - x_center) * scale_x;
        let sy = |y: f64| (y - y_center) * scale_y;
        let bounds = if config.auto_fit || config.x_length.is_some() || config.y_length.is_some() {
            Bounds3D::new_2d(sx(x_min), sy(y_min), sx(x_max), sy(y_max))
        } else {
            Bounds3D::new_2d(x_min, y_min, x_max, y_max)
        };
        let mut children = Vec::new();
        let x_axis_in_range = y_min <= 0.0 && y_max >= 0.0;
        let y_axis_in_range = x_min <= 0.0 && x_max >= 0.0;

        let mut grid = gaanim_core::kurbo::BezPath::new();
        if config.grid && config.x_grid {
            let mut x = (x_min / x_step).ceil() * x_step;
            while x <= x_max + 1e-9 {
                if x.abs() > 1e-9 {
                    grid.move_to(Point::new(sx(x), sy(y_min)));
                    grid.line_to(Point::new(sx(x), sy(y_max)));
                }
                x += x_step;
            }
        }
        if config.grid && config.y_grid {
            let mut y = (y_min / y_step).ceil() * y_step;
            while y <= y_max + 1e-9 {
                if y.abs() > 1e-9 {
                    grid.move_to(Point::new(sx(x_min), sy(y)));
                    grid.line_to(Point::new(sx(x_max), sy(y)));
                }
                y += y_step;
            }
        }
        if !grid.elements().is_empty() {
            children.push(Self::axis_path(
                builder,
                grid,
                bounds,
                config.grid_color,
                config.grid_width,
                "AxesGrid",
            ));
        }

        let mut axes = gaanim_core::kurbo::BezPath::new();
        if config.x_axis && x_axis_in_range {
            axes.move_to(Point::new(sx(x_min), sy(0.0)));
            axes.line_to(Point::new(sx(x_max), sy(0.0)));
        }
        if config.y_axis && y_axis_in_range {
            axes.move_to(Point::new(sx(0.0), sy(y_min)));
            axes.line_to(Point::new(sx(0.0), sy(y_max)));
        }
        if !axes.elements().is_empty() {
            children.push(Self::axis_path(
                builder,
                axes,
                bounds,
                config.axis_color,
                config.axis_width,
                "AxesLines",
            ));
        }
        // Manim-like arrow tips at positive ends
        if config.tips {
            let tip_len: f64 = 10.0;
            let tip_half_w: f64 = 5.0;
            if config.x_axis && x_axis_in_range {
                let mut tip = gaanim_core::kurbo::BezPath::new();
                let tx = sx(x_max);
                let ty = sy(0.0);
                tip.move_to(Point::new(tx + tip_len, ty));
                tip.line_to(Point::new(tx - tip_len * 0.3, ty - tip_half_w));
                tip.line_to(Point::new(tx - tip_len * 0.3, ty + tip_half_w));
                tip.close_path();
                let tip_path = gaanim_objects::prelude::SvgPath {
                    id: "AxesTips".to_string(),
                    path: tip,
                    bounds,
                    fill: Some(gaanim_core::peniko::Brush::Solid(config.axis_color)),
                    stroke: StrokeBrush::transparent(),
                };
                children.push(builder.svg_path(&tip_path).spawn());
            }
            if config.y_axis && y_axis_in_range {
                let mut tip = gaanim_core::kurbo::BezPath::new();
                let tx = sx(0.0);
                let ty = sy(y_max);
                tip.move_to(Point::new(tx, ty + tip_len));
                tip.line_to(Point::new(tx - tip_half_w, ty - tip_len * 0.3));
                tip.line_to(Point::new(tx + tip_half_w, ty - tip_len * 0.3));
                tip.close_path();
                let tip_path = gaanim_objects::prelude::SvgPath {
                    id: "AxesTips".to_string(),
                    path: tip,
                    bounds,
                    fill: Some(gaanim_core::peniko::Brush::Solid(config.axis_color)),
                    stroke: StrokeBrush::transparent(),
                };
                children.push(builder.svg_path(&tip_path).spawn());
            }
        }

        let tick_half = config.tick_length * 0.5;
        let mut ticks = gaanim_core::kurbo::BezPath::new();
        if config.ticks && config.x_ticks && x_axis_in_range {
            let mut x = (x_min / x_step).ceil() * x_step;
            while x <= x_max + 1e-9 {
                ticks.move_to(Point::new(sx(x), sy(0.0) - tick_half));
                ticks.line_to(Point::new(sx(x), sy(0.0) + tick_half));
                x += x_step;
            }
        }
        if config.ticks && config.y_ticks && y_axis_in_range {
            let mut y = (y_min / y_step).ceil() * y_step;
            while y <= y_max + 1e-9 {
                ticks.move_to(Point::new(sx(0.0) - tick_half, sy(y)));
                ticks.line_to(Point::new(sx(0.0) + tick_half, sy(y)));
                y += y_step;
            }
        }
        if !ticks.elements().is_empty() {
            children.push(Self::axis_path(
                builder,
                ticks,
                bounds,
                config.tick_color,
                config.tick_width,
                "AxesTicks",
            ));
        }

        if config.numbers && config.x_numbers && x_axis_in_range {
            let mut x = (x_min / x_step).ceil() * x_step;
            while x <= x_max + 1e-9 {
                let value = if x.abs() < 1e-9 { 0.0 } else { x };
                let text = format!("{value}");
                children.push(Self::axis_text(
                    builder,
                    &text,
                    sx(x),
                    sy(0.0) - tick_half - 14.0,
                    config.number_color,
                ));
                x += x_step;
            }
        }
        if config.numbers && config.y_numbers && y_axis_in_range {
            let mut y = (y_min / y_step).ceil() * y_step;
            while y <= y_max + 1e-9 {
                if y.abs() > 1e-9 || !config.x_numbers {
                    let value = if y.abs() < 1e-9 { 0.0 } else { y };
                    let text = format!("{value}");
                    let estimated_width = text.chars().count() as f64 * 9.0;
                    children.push(Self::axis_text(
                        builder,
                        &text,
                        sx(0.0) - tick_half - 8.0 - estimated_width * 0.5,
                        sy(y),
                        config.number_color,
                    ));
                }
                y += y_step;
            }
        }

        if config.labels {
            if let Some(label) = config.x_label.as_deref().filter(|_| x_axis_in_range) {
                children.push(Self::axis_text(
                    builder,
                    label,
                    sx(x_max) + 18.0,
                    sy(0.0) - tick_half - 2.0,
                    config.label_color,
                ));
            }
            if let Some(label) = config.y_label.as_deref().filter(|_| y_axis_in_range) {
                children.push(Self::axis_text(
                    builder,
                    label,
                    sx(0.0) + tick_half + 12.0,
                    sy(y_max) + 12.0,
                    config.label_color,
                ));
            }
        }

        builder.group(&children)
    }

    fn styled_axes_3d(
        builder: &mut SceneBuilder<'_, '_, '_>,
        x_range: (f64, f64, f64),
        y_range: (f64, f64, f64),
        z_range: (f64, f64, f64),
        config: &crate::canvas::types::Axes3DConfig,
        _frame_bounds: Bounds3D,
    ) -> MobjectRef {
        let (x_min, x_max, x_step) = x_range;
        let (y_min, y_max, y_step) = y_range;
        let (z_min, z_max, z_step) = z_range;
        let (scale_x, scale_y, scale_z, x_center, y_center, z_center) =
            match (config.x_length, config.y_length, config.z_length) {
                (Some(xl), Some(yl), Some(zl)) => {
                    let sx = xl / (x_max - x_min).max(1e-9);
                    let sy = yl / (y_max - y_min).max(1e-9);
                    let sz = zl / (z_max - z_min).max(1e-9);
                    (
                        sx,
                        sy,
                        sz,
                        (x_min + x_max) * 0.5,
                        (y_min + y_max) * 0.5,
                        (z_min + z_max) * 0.5,
                    )
                }
                _ => (1.0, 1.0, 1.0, 0.0, 0.0, 0.0),
            };
        let sx = |x: f64| (x - x_center) * scale_x;
        let sy = |y: f64| (y - y_center) * scale_y;
        let sz = |z: f64| (z - z_center) * scale_z;
        let bounds = Bounds3D::new_3d(
            sx(x_min),
            sy(y_min),
            sz(z_min),
            sx(x_max),
            sy(y_max),
            sz(z_max),
        );
        let mut children = Vec::new();

        // Helper to push a 3D line list
        let mut push_line_list = |points: Vec<[f32; 3]>, color: peniko::Color, _tag: &str| {
            if points.len() < 2 {
                return;
            }
            let mref = builder.spawn_line_list(points, color);
            // Tag for debugging
            if let Some(state) = builder.states.get_mut(mref.id) {
                state.bounds = bounds;
            }
            children.push(mref);
        };

        // 3 grid planes
        if config.grid {
            // XY plane at z=0 (if z range includes 0)
            if config.xy_grid && z_min <= 0.0 && z_max >= 0.0 {
                let z0 = sz(0.0) as f32;
                {
                    let mut x = (x_min / x_step).ceil() * x_step;
                    while x <= x_max + 1e-9 {
                        if x.abs() > 1e-9 {
                            let fx = sx(x) as f32;
                            let y0 = sy(y_min) as f32;
                            let y1 = sy(y_max) as f32;
                            let points = vec![[fx, y0, z0], [fx, y1, z0]];
                            push_line_list(points, config.grid_color, "Axes3DGridXY");
                        }
                        x += x_step;
                    }
                }
                {
                    let mut y = (y_min / y_step).ceil() * y_step;
                    while y <= y_max + 1e-9 {
                        if y.abs() > 1e-9 {
                            let fy = sy(y) as f32;
                            let x0 = sx(x_min) as f32;
                            let x1 = sx(x_max) as f32;
                            let points = vec![[x0, fy, z0], [x1, fy, z0]];
                            push_line_list(points, config.grid_color, "Axes3DGridXY");
                        }
                        y += y_step;
                    }
                }
            }
            // XZ plane at y=0
            if config.xz_grid && y_min <= 0.0 && y_max >= 0.0 {
                let y0 = sy(0.0) as f32;
                {
                    let mut x = (x_min / x_step).ceil() * x_step;
                    while x <= x_max + 1e-9 {
                        if x.abs() > 1e-9 {
                            let fx = sx(x) as f32;
                            let z0 = sz(z_min) as f32;
                            let z1 = sz(z_max) as f32;
                            let points = vec![[fx, y0, z0], [fx, y0, z1]];
                            push_line_list(points, config.grid_color, "Axes3DGridXZ");
                        }
                        x += x_step;
                    }
                }
                {
                    // reuse z step for grid density in XZ
                    let mut z = (z_min / z_step).ceil() * z_step;
                    while z <= z_max + 1e-9 {
                        if z.abs() > 1e-9 {
                            let fz = sz(z) as f32;
                            let x0 = sx(x_min) as f32;
                            let x1 = sx(x_max) as f32;
                            let points = vec![[x0, y0, fz], [x1, y0, fz]];
                            push_line_list(points, config.grid_color, "Axes3DGridXZ");
                        }
                        z += z_step;
                    }
                }
            }
            // YZ plane at x=0
            if config.yz_grid && x_min <= 0.0 && x_max >= 0.0 {
                let x0 = sx(0.0) as f32;
                {
                    let mut y = (y_min / y_step).ceil() * y_step;
                    while y <= y_max + 1e-9 {
                        if y.abs() > 1e-9 {
                            let fy = sy(y) as f32;
                            let z0 = sz(z_min) as f32;
                            let z1 = sz(z_max) as f32;
                            let points = vec![[x0, fy, z0], [x0, fy, z1]];
                            push_line_list(points, config.grid_color, "Axes3DGridYZ");
                        }
                        y += y_step;
                    }
                }
                {
                    let mut z = (z_min / z_step).ceil() * z_step;
                    while z <= z_max + 1e-9 {
                        if z.abs() > 1e-9 {
                            let fz = sz(z) as f32;
                            let y0 = sy(y_min) as f32;
                            let y1 = sy(y_max) as f32;
                            let points = vec![[x0, y0, fz], [x0, y1, fz]];
                            push_line_list(points, config.grid_color, "Axes3DGridYZ");
                        }
                        z += z_step;
                    }
                }
            }
        }

        // Axes lines (3)
        {
            let mut axes_points = Vec::new();
            if config.x_axis && y_min <= 0.0 && y_max >= 0.0 && z_min <= 0.0 && z_max >= 0.0 {
                axes_points.push([sx(x_min) as f32, sy(0.0) as f32, sz(0.0) as f32]);
                axes_points.push([sx(x_max) as f32, sy(0.0) as f32, sz(0.0) as f32]);
            }
            if config.y_axis && x_min <= 0.0 && x_max >= 0.0 && z_min <= 0.0 && z_max >= 0.0 {
                axes_points.push([sx(0.0) as f32, sy(y_min) as f32, sz(0.0) as f32]);
                axes_points.push([sx(0.0) as f32, sy(y_max) as f32, sz(0.0) as f32]);
            }
            if config.z_axis && x_min <= 0.0 && x_max >= 0.0 && y_min <= 0.0 && y_max >= 0.0 {
                axes_points.push([sx(0.0) as f32, sy(0.0) as f32, sz(z_min) as f32]);
                axes_points.push([sx(0.0) as f32, sy(0.0) as f32, sz(z_max) as f32]);
            }
            if !axes_points.is_empty() {
                // Each pair is a segment, need to split into separate line lists per axis to avoid connecting
                for chunk in axes_points.chunks(2) {
                    if chunk.len() == 2 {
                        push_line_list(chunk.to_vec(), config.axis_color, "Axes3DLines");
                    }
                }
            }
        }

        // Ticks (short segments perpendicular)
        let tick_half = (config.tick_length * 0.5) as f32;
        if config.ticks {
            if config.x_ticks && y_min <= 0.0 && y_max >= 0.0 && z_min <= 0.0 && z_max >= 0.0 {
                let mut x = (x_min / x_step).ceil() * x_step;
                while x <= x_max + 1e-9 {
                    let fx = sx(x) as f32;
                    let y0 = sy(0.0) as f32;
                    let z0 = sz(0.0) as f32;
                    let points = vec![[fx, y0 - tick_half, z0], [fx, y0 + tick_half, z0]];
                    push_line_list(points, config.tick_color, "Axes3DTicks");
                    x += x_step;
                }
            }
            if config.y_ticks && x_min <= 0.0 && x_max >= 0.0 && z_min <= 0.0 && z_max >= 0.0 {
                let mut y = (y_min / y_step).ceil() * y_step;
                while y <= y_max + 1e-9 {
                    let fy = sy(y) as f32;
                    let x0 = sx(0.0) as f32;
                    let z0 = sz(0.0) as f32;
                    let points = vec![[x0 - tick_half, fy, z0], [x0 + tick_half, fy, z0]];
                    push_line_list(points, config.tick_color, "Axes3DTicks");
                    y += y_step;
                }
            }
            if config.z_ticks && x_min <= 0.0 && x_max >= 0.0 && y_min <= 0.0 && y_max >= 0.0 {
                let mut z = (z_min / z_step).ceil() * z_step;
                while z <= z_max + 1e-9 {
                    let fz = sz(z) as f32;
                    let x0 = sx(0.0) as f32;
                    let y0 = sy(0.0) as f32;
                    let points = vec![[x0 - tick_half, y0, fz], [x0 + tick_half, y0, fz]];
                    push_line_list(points, config.tick_color, "Axes3DTicks");
                    z += z_step;
                }
            }
        }

        // Numbers and labels as billboarded text
        let mut add_text = |text: &str, x: f64, y: f64, z: f64, color: peniko::Color| {
            let label = builder.body(text);
            // Clone child info before mutable borrow ends
            let child_entities: Vec<bevy::prelude::Entity> = builder
                .states
                .get(label.id)
                .map(|s| s.child_spans.iter().map(|c| c.entity).collect())
                .unwrap_or_default();
            if let Some(state) = builder.states.get_mut(label.id) {
                // Position in 3D
                state.transform = state
                    .transform
                    .shift_3d(gaanim_core::glam::DVec3::new(x, y, z));
                builder
                    .commands
                    .entity(state.entity)
                    .insert(state.transform);
                // Billboard handling — applied to parent mobject entity so it acts as 3D anchor
                if config.label_mode == crate::canvas::types::LabelMode::Billboard {
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(gaanim_scene::Billboard);
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(gaanim_scene::Mesh3DMarker);
                } else {
                    // HUD labels are fixed screen-space; keep as Vello2D with high z
                    // so they render on top of 3D after the perspective composition.
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(gaanim_scene::HudOverlay);
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(gaanim_scene::RenderOrder {
                            z_index: 1000,
                            ..Default::default()
                        });
                    for child in &child_entities {
                        builder
                            .commands
                            .entity(*child)
                            .insert(gaanim_scene::HudOverlay);
                        builder
                            .commands
                            .entity(*child)
                            .insert(gaanim_scene::RenderOrder {
                                z_index: 1000,
                                ..Default::default()
                            });
                    }
                }
            }
            builder.select(label, text).set_fill(color);
            children.push(label);
        };

        if config.numbers {
            if config.x_numbers && y_min <= 0.0 && y_max >= 0.0 && z_min <= 0.0 && z_max >= 0.0 {
                let mut x = (x_min / x_step).ceil() * x_step;
                while x <= x_max + 1e-9 {
                    let value = if x.abs() < 1e-9 { 0.0 } else { x };
                    let text = format!("{value}");
                    add_text(
                        &text,
                        sx(x),
                        sy(0.0) - config.tick_length * 0.5 - 0.25,
                        sz(0.0),
                        config.number_color,
                    );
                    x += x_step;
                }
            }
            if config.y_numbers && x_min <= 0.0 && x_max >= 0.0 && z_min <= 0.0 && z_max >= 0.0 {
                let mut y = (y_min / y_step).ceil() * y_step;
                while y <= y_max + 1e-9 {
                    if y.abs() > 1e-9 || !config.x_numbers {
                        let value = if y.abs() < 1e-9 { 0.0 } else { y };
                        let text = format!("{value}");
                        add_text(
                            &text,
                            sx(0.0) - config.tick_length * 0.5 - 0.35,
                            sy(y),
                            sz(0.0),
                            config.number_color,
                        );
                    }
                    y += y_step;
                }
            }
            if config.z_numbers && x_min <= 0.0 && x_max >= 0.0 && y_min <= 0.0 && y_max >= 0.0 {
                let mut z = (z_min / z_step).ceil() * z_step;
                while z <= z_max + 1e-9 {
                    if z.abs() > 1e-9 {
                        let value = if z.abs() < 1e-9 { 0.0 } else { z };
                        let text = format!("{value}");
                        add_text(&text, sx(0.0) - 0.35, sy(0.0), sz(z), config.number_color);
                    }
                    z += z_step;
                }
            }
        }

        if config.labels {
            if let Some(label) = config
                .x_label
                .as_deref()
                .filter(|_| y_min <= 0.0 && y_max >= 0.0 && z_min <= 0.0 && z_max >= 0.0)
            {
                add_text(label, sx(x_max) + 0.4, sy(0.0), sz(0.0), config.label_color);
            }
            if let Some(label) = config
                .y_label
                .as_deref()
                .filter(|_| x_min <= 0.0 && x_max >= 0.0 && z_min <= 0.0 && z_max >= 0.0)
            {
                add_text(label, sx(0.0), sy(y_max) + 0.4, sz(0.0), config.label_color);
            }
            if let Some(label) = config
                .z_label
                .as_deref()
                .filter(|_| x_min <= 0.0 && x_max >= 0.0 && y_min <= 0.0 && y_max >= 0.0)
            {
                add_text(label, sx(0.0), sy(0.0), sz(z_max) + 0.4, config.label_color);
            }
        }

        // Ensure a light exists for PBR visibility (once per scene)
        // Use a unique check to avoid spawning many lights if axes_3d is called multiple times
        // For now, spawn a single directional light at world origin
        builder.commands.spawn((
            bevy::prelude::DirectionalLight {
                illuminance: 10000.0,
                shadows_enabled: false,
                ..Default::default()
            },
            bevy::prelude::Transform::from_xyz(4.0, 8.0, 4.0)
                .looking_at(bevy::prelude::Vec3::ZERO, bevy::prelude::Vec3::Y),
        ));

        // Create a 3D-aware group that has both SpatialTransform and Bevy Transform
        // to avoid B0004 hierarchy warnings for mesh children
        let group_id = builder.next_id();
        let group_entity = builder
            .commands
            .spawn((
                GroupMarker,
                MobjectId(group_id),
                SpatialTransform::default(),
                GlobalSpatialTransform::default(),
                bevy::prelude::Transform::default(),
                bevy::prelude::GlobalTransform::default(),
                bevy::prelude::Visibility::default(),
                Opacity(1.0),
                GlobalOpacity(1.0),
                LocalBounds(bounds),
                WorldBounds(bounds),
                RenderOrder::default(),
                Visible,
                ObjectTag("Axes3D".to_string()),
            ))
            .id();
        builder.tag_entity(group_entity);
        let state = MobjectState {
            path: std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: None,
            stroke: StrokeBrush::transparent(),
            entity: group_entity,
            child_spans: Vec::new(),
            children: children.iter().map(|c| c.id).collect(),
            parent: None,
            exclude_from_parent_draw: false,
        };
        builder.states.insert(group_id, state);
        for child in &children {
            if let Some(child_state) = builder.states.get_mut(child.id) {
                child_state.parent = Some(group_id);
                builder
                    .commands
                    .entity(child_state.entity)
                    .set_parent_in_place(group_entity);
            }
        }
        builder.ensure_track(group_id);
        for child in &children {
            builder.ensure_track(child.id);
        }
        MobjectRef { id: group_id }
    }

    fn visual_leaf_ids(builder: &SceneBuilder<'_, '_, '_>, root: ObjectId) -> Vec<ObjectId> {
        let mut leaves = Vec::new();
        let mut stack = vec![root];
        let mut visited = HashSet::new();
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            let Some(state) = builder.states.get(id) else {
                continue;
            };
            let mut children = state.children.clone();
            children.extend(state.child_spans.iter().map(|child| child.id));
            if children.is_empty() {
                leaves.push(id);
            } else {
                stack.extend(children);
            }
        }
        leaves
    }

    fn mask_path_in_world(
        builder: &SceneBuilder<'_, '_, '_>,
        root: ObjectId,
    ) -> gaanim_core::kurbo::BezPath {
        let mut result = gaanim_core::kurbo::BezPath::new();
        for id in Self::visual_leaf_ids(builder, root) {
            let Some(state) = builder.states.get(id) else {
                continue;
            };
            let mut path = (*state.path).clone();
            path.apply_affine(builder.get_world_transform(id).to_affine_2d());
            result.extend(path);
        }
        result
    }

    pub fn compile_into<'w, 's>(
        &self,
        commands: &mut Commands<'w, 's>,
        timeline: &mut Timeline,
        font_registry: &gaanim_text::font::FontRegistry,
        text_config: &gaanim_text::prelude::TextConfig,
    ) {
        let manifest = self.segment_manifest();
        timeline.set_segments(
            manifest
                .segments
                .into_iter()
                .map(|segment| SegmentMetadata {
                    id: segment.id.raw(),
                    name: segment.name,
                    notes: segment.notes,
                    start_time: segment.start_time,
                    end_time: segment.end_time,
                    stops: segment
                        .stops
                        .into_iter()
                        .map(|stop| SegmentStop {
                            name: stop.name,
                            time: stop.time,
                        })
                        .collect(),
                })
                .collect(),
        );
        let segments = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .clone();
        let mut builder = SceneBuilder::new(commands, timeline, font_registry, text_config);
        let mut scene_ids: Vec<SceneId> = Vec::new();
        let mut id_map: HashMap<ObjectId, ObjectId> = HashMap::new();
        let mut object_scopes: HashMap<ObjectId, CompiledObjectScope> = HashMap::new();
        let mut camera_position = DVec3::ZERO;
        let mut camera_zoom = 1.0;
        let mut camera_rotation = gaanim_core::glam::DQuat::IDENTITY;
        let mut camera_target = DVec3::ZERO;
        let mut camera_fov: Option<(f64, f64, f64)> = None; // (fov_y, near, far) if perspective
        let mut cancellation_marks: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
        let mut canceled_term_children: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
        let mut deferred_visibility: HashSet<ObjectId> = HashSet::new();
        let mut revealed_deferred: HashSet<ObjectId> = HashSet::new();
        // Raw bounds for the canvas background (visual, no margin).
        let raw_bounds = self.units.frame_bounds(self.width, self.height);
        // Inset bounds for layout operations (to_edge, to_corner respect margin).
        let m = &self.margin;
        let frame_bounds = Bounds3D::new_2d(
            raw_bounds.min.x + m.left,
            raw_bounds.min.y + m.bottom,
            raw_bounds.max.x - m.right,
            raw_bounds.max.y - m.top,
        );
        let bg_color = self.background.unwrap_or(gaanim_core::peniko::Color::WHITE);

        for seg in &segments {
            let previous_scene = seg
                .prev_segment
                .and_then(|index| scene_ids.get(index).copied());
            let scene_id = builder.begin_scene(&seg.name);
            scene_ids.push(scene_id);
            Self::replay_seg(
                &mut builder,
                seg,
                scene_id,
                previous_scene,
                &mut id_map,
                &mut object_scopes,
                frame_bounds,
                raw_bounds,
                text_config,
                bg_color,
                &mut camera_position,
                &mut camera_zoom,
                &mut camera_rotation,
                &mut camera_target,
                &mut camera_fov,
                &mut cancellation_marks,
                &mut canceled_term_children,
                &mut deferred_visibility,
                &mut revealed_deferred,
            );
            builder.end_scene();
        }

        for (i, seg) in segments.iter().enumerate() {
            if let Some(prev) = seg.prev_segment
                && prev < i
                && i < scene_ids.len()
                && prev < scene_ids.len()
                && let Some(tr) = &seg.transition
            {
                builder
                    .timeline
                    .connect(scene_ids[prev], scene_ids[i], tr.clone());
            }
        }

        // Insert canvas background resource so the renderer draws a visible
        // canvas boundary, distinguishing the canvas area from the window.
        // Uses raw_bounds (no margin) — the visual background covers the full canvas.
        builder
            .commands
            .insert_resource(gaanim_renderer::pipeline::CanvasBackground {
                color: bg_color,
                bounds: raw_bounds,
            });

        // Clear with the canvas color as well. The drawable background is
        // world-space geometry and can be rotated by the camera; using the
        // same clear color prevents Bevy's window clear color from showing
        // through at the viewport edges during that rotation.
        let rgba = bg_color.to_rgba8();
        builder
            .commands
            .insert_resource(ClearColor(Color::srgba_u8(rgba.r, rgba.g, rgba.b, rgba.a)));
    }

    pub fn compile(&self, world: &mut World) {
        let mut timeline = world
            .remove_resource::<Timeline>()
            .expect("Timeline missing");
        let mut font_registry = world
            .remove_resource::<gaanim_text::font::FontRegistry>()
            .expect("FontRegistry missing");
        let mut text_config = world
            .remove_resource::<gaanim_text::prelude::TextConfig>()
            .expect("TextConfig missing");
        if self.theme.is_some() {
            text_config = self.themed_text_config();
        } else {
            let bg_color = self.background.unwrap_or(gaanim_core::peniko::Color::WHITE);
            let default_fg = if typst_foreground_for_background(bg_color) == "000000" {
                gaanim_core::peniko::Color::BLACK
            } else {
                gaanim_core::peniko::Color::WHITE
            };
            for role_style in text_config.roles.values_mut() {
                role_style.fill_color = default_fg;
            }
        }
        self.register_theme_fonts(&mut font_registry);
        let mut commands = world.commands();
        self.compile_into(&mut commands, &mut timeline, &font_registry, &text_config);
        world.insert_resource(timeline);
        world.insert_resource(font_registry);
        world.insert_resource(text_config);
    }

    fn replay_seg(
        builder: &mut SceneBuilder,
        seg: &Segment,
        scene_id: SceneId,
        previous_scene: Option<SceneId>,
        id_map: &mut HashMap<ObjectId, ObjectId>,
        object_scopes: &mut HashMap<ObjectId, CompiledObjectScope>,
        frame_bounds: Bounds3D,
        raw_frame_bounds: Bounds3D,
        text_config: &gaanim_text::prelude::TextConfig,
        scene_background: gaanim_core::peniko::Color,
        camera_position: &mut DVec3,
        camera_zoom: &mut f64,
        camera_rotation: &mut gaanim_core::glam::DQuat,
        camera_target: &mut DVec3,
        camera_fov: &mut Option<(f64, f64, f64)>,
        cancellation_marks: &mut HashMap<ObjectId, Vec<ObjectId>>,
        canceled_term_children: &mut HashMap<ObjectId, Vec<ObjectId>>,
        deferred_visibility: &mut HashSet<ObjectId>,
        revealed_deferred: &mut HashSet<ObjectId>,
    ) {
        let scene_start = builder.current_time;
        let transform_targets = Self::transform_targets(&seg.ops);
        for op in &seg.ops {
            match op {
                Op::Spawn(spec) => {
                    let spec = spec.lock().expect("object spec poisoned").clone();
                    if spec.defer_visibility_until_play {
                        deferred_visibility.insert(spec.id);
                    }
                    let actual = Self::spawn_one(
                        builder,
                        &spec,
                        id_map,
                        frame_bounds,
                        text_config,
                        scene_background,
                    );
                    if spec.exclude_from_parent_draw {
                        if let Some(state) = builder.states.get_mut(actual.id) {
                            state.exclude_from_parent_draw = true;
                        }
                    }
                    id_map.insert(spec.id, actual.id);
                    object_scopes.insert(spec.id, CompiledObjectScope::Segment(scene_id));
                    // Compilation creates every entity up front so arbitrary timeline seeks
                    // remain possible. An object declared after earlier animations must still
                    // stay hidden until the playhead reaches its declaration point.
                    if spec.defer_visibility_until_play {
                        if let Some(state) = builder.states.get(actual.id).cloned() {
                            builder.hide_visuals_now(&state);
                        }
                    } else if !transform_targets.contains(&spec.id)
                        && builder.current_time > scene_start + 1e-9
                        && let Some(state) = builder.states.get(actual.id).cloned()
                    {
                        builder.hide_visuals_now(&state);
                        builder.schedule_show_now(actual.id);
                    }
                    if transform_targets.contains(&spec.id) {
                        if let Some(state) = builder.states.get(actual.id).cloned() {
                            builder.hide_visuals_now(&state);
                            builder.schedule_hide_hierarchy(actual.id);
                        }
                    }
                }
                Op::Animate { anim, active } => {
                    if *active {
                        Self::reveal_deferred_on_play(
                            builder,
                            deferred_visibility,
                            revealed_deferred,
                            anim,
                            id_map,
                        );
                        if let Some(anim) = Self::remap_anim(anim, id_map) {
                            if anim.anim_type.is_camera() {
                                let start = builder.current_time;
                                Self::schedule_camera_animation(
                                    builder,
                                    frame_bounds,
                                    camera_position,
                                    camera_zoom,
                                    camera_rotation,
                                    camera_target,
                                    camera_fov,
                                    &anim,
                                    start,
                                );
                                builder.wait(anim.delay.max(0.0) + anim.duration.max(0.0));
                            } else {
                                builder.play(anim);
                            }
                        }
                    }
                }
                Op::Play(anims) => {
                    for anim in anims {
                        Self::reveal_deferred_on_play(
                            builder,
                            deferred_visibility,
                            revealed_deferred,
                            anim,
                            id_map,
                        );
                    }
                    let remapped: Vec<AnimationBuilder> = anims
                        .iter()
                        .filter_map(|anim| Self::remap_anim(anim, id_map))
                        .collect();
                    let start = builder.current_time;
                    let max_duration = remapped
                        .iter()
                        .map(|anim| anim.delay.max(0.0) + anim.duration.max(0.0))
                        .fold(0.0, f64::max);
                    for anim in remapped {
                        if anim.anim_type.is_camera() {
                            Self::schedule_camera_animation(
                                builder,
                                frame_bounds,
                                camera_position,
                                camera_zoom,
                                camera_rotation,
                                camera_target,
                                camera_fov,
                                &anim,
                                start,
                            );
                        } else {
                            builder.play_at_current_time(anim);
                        }
                    }
                    builder.current_time = start + max_duration;
                }
                Op::FragmentFill {
                    target,
                    fragment,
                    occurrence,
                    color,
                } => {
                    if let Some(&target) = id_map.get(target) {
                        builder
                            .select_occurrence(MobjectRef { id: target }, fragment, *occurrence)
                            .set_fill(*color);
                    }
                }
                Op::FragmentIndicate {
                    target,
                    fragment,
                    occurrence,
                    color,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    canceled_term_children
                        .entry(*target)
                        .or_default()
                        .extend(children.iter().copied());
                    let anims = children
                        .into_iter()
                        .map(|target| AnimationBuilder {
                            target,
                            anim_type: AnimationType::Indicate {
                                color: *color,
                                scale_factor: 1.1,
                            },
                            duration: *duration,
                            rate_func: RateFunc::ThereAndBack,
                            delay: 0.0,
                        })
                        .collect();
                    builder.play_parallel(anims);
                }
                Op::FragmentReveal {
                    target,
                    fragment,
                    occurrence,
                    style,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    let anims = children
                        .into_iter()
                        .map(|target| AnimationBuilder {
                            target,
                            anim_type: match style {
                                FragmentRevealStyle::Fade => AnimationType::FadeIn,
                                FragmentRevealStyle::Wipe => AnimationType::Write {
                                    config: Default::default(),
                                },
                                FragmentRevealStyle::FromBelow => AnimationType::FadeInFrom {
                                    offset: DVec3::new(0.0, -24.0, 0.0),
                                },
                            },
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        })
                        .collect();
                    builder.play_parallel(anims);
                }
                Op::CancelFragment {
                    target,
                    fragment,
                    occurrence,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    let parent_transform = id_map
                        .get(target)
                        .and_then(|parent| builder.states.get(*parent))
                        .map(|state| state.transform);
                    let strike_color = children
                        .iter()
                        .find_map(|child| {
                            builder
                                .states
                                .get(*child)
                                .and_then(|state| match &state.fill {
                                    Some(gaanim_core::peniko::Brush::Solid(color)) => Some(*color),
                                    _ => None,
                                })
                        })
                        .unwrap_or(PenikoColor::WHITE);
                    let bounds = children
                        .iter()
                        .filter_map(|child| {
                            let state = builder.states.get(*child)?;
                            // Textual child bounds have already been centered
                            // into their parent's local coordinate system by
                            // the shaper. Applying the child's transform here
                            // would subtract that center a second time and put
                            // the strike near the canvas corner.
                            let bounds = state.bounds;
                            Some(match parent_transform {
                                Some(parent) => bounds.transform_2d(&parent.to_affine_2d()),
                                None => bounds,
                            })
                        })
                        .reduce(|bounds, next| bounds.union(&next));
                    if let Some(bounds) = bounds {
                        let pad = (bounds.width() * 0.08).max(3.0);
                        let strike = builder
                            .line(
                                Point::new(bounds.min.x - pad, bounds.min.y - pad * 0.25),
                                Point::new(bounds.max.x + pad, bounds.max.y + pad * 0.25),
                            )
                            .no_fill()
                            .stroke(strike_color, 3.0)
                            .spawn();
                        cancellation_marks
                            .entry(*target)
                            .or_default()
                            .push(strike.id);
                        builder.play(AnimationBuilder {
                            target: strike.id,
                            anim_type: AnimationType::Create {
                                config: Default::default(),
                            },
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                    } else {
                        builder.wait(*duration);
                    }
                }
                Op::BraceLabel {
                    target,
                    fragment,
                    occurrence,
                    label,
                    above,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    let parent = id_map
                        .get(target)
                        .and_then(|id| builder.states.get(*id))
                        .map(|s| s.transform);
                    let bounds = children
                        .iter()
                        .filter_map(|id| builder.states.get(*id).map(|s| s.bounds))
                        .map(|b| {
                            parent
                                .map(|p| b.transform_2d(&p.to_affine_2d()))
                                .unwrap_or(b)
                        })
                        .reduce(|a, b| a.union(&b));
                    if let Some(bounds) = bounds {
                        let color = children
                            .iter()
                            .find_map(|id| {
                                builder.states.get(*id).and_then(|s| match &s.fill {
                                    Some(gaanim_core::peniko::Brush::Solid(c)) => Some(*c),
                                    _ => None,
                                })
                            })
                            .unwrap_or(PenikoColor::WHITE);
                        let side = if *above { 1.0 } else { -1.0 };
                        let y = if *above {
                            bounds.max.y + 12.0
                        } else {
                            bounds.min.y - 12.0
                        };
                        let brace = builder
                            .brace(
                                Point::new(bounds.min.x, y),
                                Point::new(bounds.max.x, y),
                                -side * 10.0,
                            )
                            .no_fill()
                            .stroke(color, 2.0)
                            .spawn();
                        let style = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                        let label_ref = builder.text(label, &style.font_family, style.size);
                        if let Some(state) = builder.states.get_mut(label_ref.id) {
                            state.transform.translation =
                                DVec3::new(bounds.center().x, y + side * 25.0, 0.0);
                            builder
                                .commands
                                .entity(state.entity)
                                .insert(state.transform);
                        }
                        builder.play_parallel(vec![
                            AnimationBuilder {
                                target: brace.id,
                                anim_type: AnimationType::Create {
                                    config: Default::default(),
                                },
                                duration: *duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            },
                            AnimationBuilder {
                                target: label_ref.id,
                                anim_type: AnimationType::FadeIn,
                                duration: *duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            },
                        ]);
                    }
                }
                Op::AnnotateFragment {
                    target,
                    fragment,
                    occurrence,
                    label,
                    offset,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    let parent = id_map
                        .get(target)
                        .and_then(|id| builder.states.get(*id))
                        .map(|s| s.transform);
                    let bounds = children
                        .iter()
                        .filter_map(|id| builder.states.get(*id).map(|s| s.bounds))
                        .map(|b| {
                            parent
                                .map(|p| b.transform_2d(&p.to_affine_2d()))
                                .unwrap_or(b)
                        })
                        .reduce(|a, b| a.union(&b));
                    if let Some(bounds) = bounds {
                        let position = bounds.center() + *offset;
                        let style = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                        let label_ref = builder.text(label, &style.font_family, style.size);
                        let label_size = builder
                            .states
                            .get(label_ref.id)
                            .map(|state| state.bounds.size())
                            .unwrap_or(DVec3::new(80.0, 24.0, 0.0));
                        let toward_label_x = if offset.x >= 0.0 { -1.0 } else { 1.0 };
                        let toward_label_y = if offset.y >= 0.0 { -1.0 } else { 1.0 };
                        // Attach at the label corner nearest the term, so the
                        // leader line never crosses the annotation text.
                        let label_anchor = position
                            + DVec3::new(
                                toward_label_x * label_size.x * 0.46,
                                toward_label_y * label_size.y * 0.42,
                                0.0,
                            );
                        if let Some(state) = builder.states.get_mut(label_ref.id) {
                            state.transform.translation = position;
                            builder
                                .commands
                                .entity(state.entity)
                                .insert(state.transform);
                        }
                        let line = builder
                            .line(
                                Point::new(bounds.center().x, bounds.center().y),
                                Point::new(label_anchor.x, label_anchor.y),
                            )
                            .no_fill()
                            .stroke(PenikoColor::WHITE, 2.0)
                            .spawn();
                        // Text glyph transforms are local to their equation.
                        // Use an invisible scene-space proxy so the leader
                        // starts at the tag rather than a canvas corner.
                        let proxy = builder.dot(0.01).no_fill().no_stroke().spawn();
                        if let Some(proxy_state) = builder.states.get_mut(proxy.id) {
                            proxy_state.transform.translation = bounds.center();
                            builder
                                .commands
                                .entity(proxy_state.entity)
                                .insert(proxy_state.transform);
                        }
                        if let (Some(line_state), Some(proxy_state)) =
                            (builder.states.get(line.id), builder.states.get(proxy.id))
                        {
                            if let Some(parent_id) = id_map.get(target).copied()
                                && let Some(parent_state) = builder.states.get(parent_id)
                            {
                                builder.commands.entity(proxy_state.entity).insert(
                                    PositionBinding::with_offset(
                                        parent_state.entity,
                                        gaanim_animation::AxisMask::XY,
                                        bounds.center() - parent_state.transform.translation,
                                    ),
                                );
                            }
                            builder
                                .commands
                                .entity(line_state.entity)
                                .insert(TrackingLine::new(
                                    TrackingEndpoint::Entity(proxy_state.entity),
                                    TrackingEndpoint::Static(label_anchor),
                                ));
                        }
                        builder.play_parallel(vec![
                            AnimationBuilder {
                                target: line.id,
                                // TrackingLine regenerates its path each
                                // frame, so a path-draw clip cannot hide it
                                // before its scheduled start. FadeIn keeps it
                                // invisible until the annotation begins.
                                anim_type: AnimationType::FadeIn,
                                duration: *duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            },
                            AnimationBuilder {
                                target: label_ref.id,
                                anim_type: AnimationType::FadeIn,
                                duration: *duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            },
                        ]);
                    }
                }
                Op::WriteTerms {
                    target,
                    terms,
                    duration,
                } => {
                    let term_duration = *duration / terms.len() as f64;
                    for (fragment, occurrence) in terms {
                        let anims = Self::fragment_child_ids(
                            builder,
                            id_map,
                            *target,
                            fragment,
                            *occurrence,
                        )
                        .into_iter()
                        .map(|target| AnimationBuilder {
                            target,
                            anim_type: AnimationType::Write {
                                config: Default::default(),
                            },
                            duration: term_duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        })
                        .collect();
                        builder.play_parallel(anims);
                    }
                }
                Op::FocusEquation {
                    target,
                    terms,
                    dim_opacity,
                    duration,
                } => {
                    let focused = terms
                        .iter()
                        .flat_map(|(fragment, occurrence)| {
                            Self::fragment_child_ids(
                                builder,
                                id_map,
                                *target,
                                fragment,
                                *occurrence,
                            )
                        })
                        .collect::<std::collections::HashSet<_>>();
                    let all = id_map
                        .get(target)
                        .and_then(|target| builder.states.get(*target))
                        .map(|state| state.children.clone())
                        .unwrap_or_default();
                    let mut anims = Vec::with_capacity(all.len() + focused.len());
                    for child in all {
                        if focused.contains(&child) {
                            anims.push(AnimationBuilder {
                                target: child,
                                anim_type: AnimationType::Indicate {
                                    color: None,
                                    scale_factor: 1.12,
                                },
                                duration: *duration,
                                rate_func: RateFunc::ThereAndBack,
                                delay: 0.0,
                            });
                        } else {
                            anims.push(AnimationBuilder {
                                target: child,
                                anim_type: AnimationType::FadeTo { to: *dim_opacity },
                                duration: *duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            });
                        }
                    }
                    builder.play_parallel(anims);
                }
                Op::FragmentFillTo {
                    target,
                    fragment,
                    occurrence,
                    color,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    let anims = children
                        .into_iter()
                        .map(|target| AnimationBuilder {
                            target,
                            anim_type: AnimationType::FillColorTo { to: *color },
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        })
                        .collect();
                    builder.play_parallel(anims);
                }
                Op::FragmentTransform {
                    source,
                    source_fragment,
                    source_occurrence,
                    target,
                    target_fragment,
                    target_occurrence,
                    duration,
                } => {
                    let sources = Self::fragment_child_ids(
                        builder,
                        id_map,
                        *source,
                        source_fragment,
                        *source_occurrence,
                    );
                    let targets = Self::fragment_child_ids(
                        builder,
                        id_map,
                        *target,
                        target_fragment,
                        *target_occurrence,
                    );
                    if sources.is_empty() || targets.is_empty() {
                        bevy::prelude::warn!(
                            "equation fragment transform could not resolve '{source_fragment}' -> '{target_fragment}'"
                        );
                    } else if let (Some(&source_parent), Some(&target_parent)) =
                        (id_map.get(source), id_map.get(target))
                    {
                        builder.play_equation_transition(
                            source_parent,
                            target_parent,
                            vec![(sources, targets)],
                            *duration,
                            EquationTransitionMode::Copy,
                            false,
                        );
                    }
                }
                Op::TaggedTransform {
                    source,
                    target,
                    pairs,
                    duration,
                } => {
                    let mut semantic_groups = Vec::new();
                    for (source_fragment, source_occurrence, target_fragment, target_occurrence) in
                        pairs
                    {
                        let sources = Self::fragment_child_ids(
                            builder,
                            id_map,
                            *source,
                            source_fragment,
                            *source_occurrence,
                        );
                        let targets = Self::fragment_child_ids(
                            builder,
                            id_map,
                            *target,
                            target_fragment,
                            *target_occurrence,
                        );
                        if sources.is_empty() || targets.is_empty() {
                            bevy::prelude::warn!(
                                "equation tag transform could not resolve '{source_fragment}' -> '{target_fragment}'"
                            );
                        } else {
                            semantic_groups.push((sources, targets));
                        }
                    }
                    if let (Some(&source_parent), Some(&target_parent)) =
                        (id_map.get(source), id_map.get(target))
                    {
                        builder.play_equation_transition(
                            source_parent,
                            target_parent,
                            semantic_groups,
                            *duration,
                            EquationTransitionMode::Copy,
                            false,
                        );
                    }
                }
                Op::ExpandEquation {
                    source,
                    target,
                    source_fragment,
                    source_occurrence,
                    target_fragment,
                    target_occurrence,
                    duration,
                } => {
                    Self::fade_cancellation_marks(builder, cancellation_marks, *source, *duration);
                    Self::fade_canceled_term_children(
                        builder,
                        canceled_term_children,
                        *source,
                        *duration,
                    );
                    let sources = Self::fragment_child_ids(
                        builder,
                        id_map,
                        *source,
                        source_fragment,
                        *source_occurrence,
                    );
                    let targets = Self::fragment_child_ids(
                        builder,
                        id_map,
                        *target,
                        target_fragment,
                        *target_occurrence,
                    );
                    if let (Some(&source_parent), Some(&target_parent)) =
                        (id_map.get(source), id_map.get(target))
                    {
                        if sources.is_empty() || targets.is_empty() {
                            bevy::prelude::warn!(
                                "equation expansion could not resolve '{source_fragment}' -> '{target_fragment}'"
                            );
                        } else {
                            builder.play_equation_transition(
                                source_parent,
                                target_parent,
                                vec![(sources, targets)],
                                *duration,
                                EquationTransitionMode::Replace,
                                true,
                            );
                        }
                    }
                }
                Op::StepEquation {
                    source,
                    target,
                    pairs,
                    duration,
                } => {
                    Self::fade_cancellation_marks(builder, cancellation_marks, *source, *duration);
                    Self::fade_canceled_term_children(
                        builder,
                        canceled_term_children,
                        *source,
                        *duration,
                    );
                    if let (Some(&source_parent), Some(&target_parent)) =
                        (id_map.get(source), id_map.get(target))
                    {
                        let semantic_groups = pairs
                            .iter()
                            .filter_map(
                                |(
                                    source_fragment,
                                    source_occurrence,
                                    target_fragment,
                                    target_occurrence,
                                )| {
                                    let sources = Self::fragment_child_ids(
                                        builder,
                                        id_map,
                                        *source,
                                        source_fragment,
                                        *source_occurrence,
                                    );
                                    let targets = Self::fragment_child_ids(
                                        builder,
                                        id_map,
                                        *target,
                                        target_fragment,
                                        *target_occurrence,
                                    );
                                    if sources.is_empty() || targets.is_empty() {
                                        bevy::prelude::warn!(
                                            "equation step could not resolve semantic match '{source_fragment}' -> '{target_fragment}'"
                                        );
                                        None
                                    } else {
                                        Some((sources, targets))
                                    }
                                },
                            )
                            .collect();
                        builder.play_equation_transition(
                            source_parent,
                            target_parent,
                            semantic_groups,
                            *duration,
                            EquationTransitionMode::Replace,
                            true,
                        );
                    }
                }
                Op::TransformMatching {
                    source,
                    target,
                    mode,
                    semantic_pairs,
                    duration,
                } => {
                    Self::fade_cancellation_marks(builder, cancellation_marks, *source, *duration);
                    Self::fade_canceled_term_children(
                        builder,
                        canceled_term_children,
                        *source,
                        *duration,
                    );
                    if let (Some(&src), Some(&dst)) = (id_map.get(source), id_map.get(target)) {
                        if mode == "tex" {
                            let semantic_groups = semantic_pairs
                                .iter()
                                .filter_map(
                                    |(
                                        source_fragment,
                                        source_occurrence,
                                        target_fragment,
                                        target_occurrence,
                                    )| {
                                        let sources = Self::fragment_child_ids(
                                            builder,
                                            id_map,
                                            *source,
                                            source_fragment,
                                            *source_occurrence,
                                        );
                                        let targets = Self::fragment_child_ids(
                                            builder,
                                            id_map,
                                            *target,
                                            target_fragment,
                                            *target_occurrence,
                                        );
                                        (!sources.is_empty() && !targets.is_empty())
                                            .then_some((sources, targets))
                                    },
                                )
                                .collect();
                            builder.play_equation_transition(
                                src,
                                dst,
                                semantic_groups,
                                *duration,
                                EquationTransitionMode::Replace,
                                true,
                            );
                        } else {
                            builder.play_transform_matching(
                                src,
                                dst,
                                gaanim_math::matching::MatchingMode::Shapes,
                                *duration,
                                RateFunc::Smooth,
                            );
                        }
                    }
                }
                Op::LayoutTransition {
                    from_version: _,
                    to,
                    duration,
                    entering,
                    leaving,
                } => {
                    let container = &to.container;
                    let members = &to.members;
                    let spec = &to.spec;
                    let Some(container) = id_map.get(container).copied() else {
                        continue;
                    };
                    let members: Vec<(ObjectId, gaanim_layout::LayoutItemStyle)> = members
                        .iter()
                        .filter_map(|member| {
                            id_map
                                .get(&member.id)
                                .copied()
                                .map(|id| (id, member.style.clone()))
                        })
                        .filter(|(member, _)| builder.states.get(*member).is_some())
                        .collect();

                    // A layout group may gain children after it was first
                    // declared. Reparent them before arranging: this keeps a
                    // nested layout's transform attached to its visual tree,
                    // rather than merely moving its invisible group root.
                    for (member, _) in &members {
                        let parent = builder.states.get(*member).and_then(|state| state.parent);
                        if parent != Some(container) {
                            builder.add_to_group(
                                MobjectRef { id: container },
                                MobjectRef { id: *member },
                            );
                        }
                    }
                    if let Some(state) = builder.states.get_mut(container) {
                        state.children = members.iter().map(|(id, _)| *id).collect();
                    }
                    let before: HashMap<ObjectId, SpatialTransform> = members
                        .iter()
                        .filter_map(|(member, _)| {
                            builder
                                .states
                                .get(*member)
                                .map(|state| (*member, state.transform))
                        })
                        .collect();

                    let root_id = gaanim_layout::LayoutId(container.as_raw());
                    let mut measures = BTreeMap::new();
                    let children = members
                        .iter()
                        .filter_map(|(member, item_style)| {
                            let state = builder.states.get(*member)?;
                            let mut transform = state.transform;
                            transform.translation = DVec3::ZERO;
                            let bounds = gaanim_layout::transform_bounds(state.bounds, &transform);
                            let layout_id = gaanim_layout::LayoutId(member.as_raw());
                            measures.insert(
                                layout_id,
                                DVec2::new(bounds.width().max(0.0), bounds.height().max(0.0)),
                            );
                            Some(gaanim_layout::LayoutChild {
                                node: Box::new(gaanim_layout::LayoutNode::leaf(layout_id)),
                                style: item_style.clone(),
                            })
                        })
                        .collect();
                    let mut root =
                        gaanim_layout::LayoutNode::container(root_id, spec.kind.clone(), children);
                    root.style = spec.style.clone();
                    let viewport = match spec.within {
                        LayoutWithin::Safe => frame_bounds,
                        LayoutWithin::Frame => raw_frame_bounds,
                        LayoutWithin::Intrinsic => frame_bounds,
                    };
                    let resolved = gaanim_layout::resolve_layout(
                        &root,
                        viewport,
                        &CompiledLayoutMeasure(measures),
                        &[],
                    )
                    .unwrap_or_else(|error| panic!("layout resolution failed: {error}"));
                    let root_box = resolved.boxes.get(&root_id).copied().unwrap_or(
                        gaanim_layout::ResolvedBox {
                            bounds: Bounds3D::new_2d(0.0, 0.0, 0.0, 0.0),
                            clip: None,
                            scale: DVec3::ONE,
                        },
                    );
                    let root_center = root_box.bounds.center();
                    let local_root_bounds = Bounds3D::new_2d(
                        -root_box.bounds.width() * 0.5,
                        -root_box.bounds.height() * 0.5,
                        root_box.bounds.width() * 0.5,
                        root_box.bounds.height() * 0.5,
                    );
                    if let Some(state) = builder.states.get_mut(container) {
                        state.bounds = local_root_bounds;
                        state.transform.translation = root_center;
                        builder
                            .commands
                            .entity(state.entity)
                            .insert((LocalBounds(local_root_bounds), state.transform));
                    }

                    let mut targets = Vec::new();
                    let mut cover_clips = Vec::new();
                    for (member, item_style) in &members {
                        let Some(target_box) = resolved
                            .boxes
                            .get(&gaanim_layout::LayoutId(member.as_raw()))
                            .copied()
                        else {
                            continue;
                        };
                        let Some(state) = builder.states.get_mut(*member) else {
                            continue;
                        };
                        let mut zero_translation = state.transform;
                        zero_translation.translation = DVec3::ZERO;
                        let intrinsic =
                            gaanim_layout::transform_bounds(state.bounds, &zero_translation);
                        let target_center = target_box.bounds.center() - root_center;
                        let intrinsic_center = intrinsic.center();
                        let mut target = state.transform;
                        target.translation = target_center - intrinsic_center;
                        let sx = target_box.bounds.width() / intrinsic.width().max(1.0e-9);
                        let sy = target_box.bounds.height() / intrinsic.height().max(1.0e-9);
                        let fit = match item_style.fit {
                            gaanim_layout::FitMode::None => DVec3::ONE,
                            gaanim_layout::FitMode::Contain => DVec3::splat(sx.min(sy)),
                            gaanim_layout::FitMode::Cover => DVec3::splat(sx.max(sy)),
                            gaanim_layout::FitMode::Stretch => DVec3::new(sx, sy, 1.0),
                            gaanim_layout::FitMode::ScaleDown => DVec3::splat(sx.min(sy).min(1.0)),
                        };
                        target.scale *= fit;
                        if matches!(item_style.fit, gaanim_layout::FitMode::Cover) {
                            cover_clips.push((*member, target_box.bounds));
                        }
                        targets.push((*member, target));
                        state.transform = target;
                        builder.commands.entity(state.entity).insert(target);
                    }
                    for (member, clip_bounds) in cover_clips {
                        let world_path = Rect::new(
                            clip_bounds.min.x,
                            clip_bounds.min.y,
                            clip_bounds.max.x,
                            clip_bounds.max.y,
                        )
                        .to_path(0.1);
                        for leaf in Self::visual_leaf_ids(builder, member) {
                            let Some(state) = builder.states.get(leaf) else {
                                continue;
                            };
                            let mut local_path = world_path.clone();
                            local_path.apply_affine(
                                builder.get_world_transform(leaf).to_affine_2d().inverse(),
                            );
                            builder.commands.entity(state.entity).insert(
                                gaanim_renderer::effects::ClipMask {
                                    path: local_path,
                                    rule: gaanim_core::peniko::Fill::NonZero,
                                },
                            );
                        }
                    }
                    let Some(duration) = duration else {
                        continue;
                    };

                    // Arrangement writes the final transforms. Restore the
                    // layout visible at the current timeline cursor, then let
                    // the regular animation machinery interpolate to the new
                    // arrangement and advance its cursor.
                    for (member, transform) in before {
                        if let Some(state) = builder.states.get_mut(member) {
                            state.transform = transform;
                            builder.commands.entity(state.entity).insert(transform);
                        }
                    }
                    let mut animations: Vec<AnimationBuilder> = targets
                        .into_iter()
                        .flat_map(|(target, to)| {
                            [
                                AnimationBuilder {
                                    target,
                                    anim_type: AnimationType::TranslateTo { to: to.translation },
                                    duration: *duration,
                                    rate_func: RateFunc::Smooth,
                                    delay: 0.0,
                                },
                                AnimationBuilder {
                                    target,
                                    anim_type: AnimationType::ScaleTo { to: to.scale },
                                    duration: *duration,
                                    rate_func: RateFunc::Smooth,
                                    delay: 0.0,
                                },
                            ]
                        })
                        .collect();
                    if let Some(entering) = entering
                        .as_ref()
                        .and_then(|member| id_map.get(member))
                        .copied()
                    {
                        animations.push(AnimationBuilder {
                            target: entering,
                            anim_type: AnimationType::FadeIn,
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                    }
                    if let Some(leaving) = leaving
                        .as_ref()
                        .and_then(|member| id_map.get(member))
                        .copied()
                    {
                        animations.push(AnimationBuilder {
                            target: leaving,
                            anim_type: AnimationType::FadeOut,
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                    }
                    builder.play_parallel(animations);
                }
                Op::LayoutConstraints {
                    constraints,
                    duration,
                } => {
                    let remap_expression = |expression: &gaanim_layout::LayoutExpression| {
                        let mut mapped = gaanim_layout::LayoutExpression::from(expression.constant);
                        for (variable, coefficient) in &expression.terms {
                            let source = ObjectId::from_raw(variable.node.0);
                            let Some(target) = id_map.get(&source).copied() else {
                                continue;
                            };
                            mapped = mapped
                                + gaanim_layout::LayoutExpression::variable(
                                    gaanim_layout::LayoutId(target.as_raw()),
                                    variable.attribute,
                                ) * *coefficient;
                        }
                        mapped
                    };
                    let mapped: Vec<_> = constraints
                        .iter()
                        .map(|constraint| gaanim_layout::LayoutConstraint {
                            lhs: remap_expression(&constraint.lhs),
                            relation: constraint.relation,
                            rhs: remap_expression(&constraint.rhs),
                            strength: constraint.strength,
                            label: constraint.label.clone(),
                        })
                        .collect();
                    let referenced: std::collections::BTreeSet<_> = mapped
                        .iter()
                        .flat_map(|constraint| {
                            constraint
                                .lhs
                                .terms
                                .keys()
                                .chain(constraint.rhs.terms.keys())
                                .map(|variable| variable.node)
                        })
                        .collect();
                    let mut resolved = gaanim_layout::ResolvedLayout::default();
                    for layout_id in &referenced {
                        let object = ObjectId::from_raw(layout_id.0);
                        let Some(state) = builder.states.get(object) else {
                            continue;
                        };
                        resolved.boxes.insert(
                            *layout_id,
                            gaanim_layout::ResolvedBox {
                                bounds: gaanim_layout::transform_bounds(
                                    state.bounds,
                                    &state.transform,
                                ),
                                clip: None,
                                scale: DVec3::ONE,
                            },
                        );
                    }
                    gaanim_layout::solve_constraints(&mut resolved, &mapped).unwrap_or_else(
                        |error| panic!("layout constraint resolution failed: {error}"),
                    );

                    let mut targets = Vec::new();
                    for layout_id in referenced {
                        let object = ObjectId::from_raw(layout_id.0);
                        let Some(target_box) = resolved.boxes.get(&layout_id).copied() else {
                            continue;
                        };
                        let Some(state) = builder.states.get_mut(object) else {
                            continue;
                        };
                        let current =
                            gaanim_layout::transform_bounds(state.bounds, &state.transform);
                        let sx = target_box.bounds.width() / current.width().max(1.0e-9);
                        let sy = target_box.bounds.height() / current.height().max(1.0e-9);
                        let mut target = state.transform;
                        target.translation += target_box.bounds.center() - current.center();
                        target.scale *= DVec3::new(sx, sy, 1.0);
                        targets.push((object, state.transform, target));
                        state.transform = target;
                        builder.commands.entity(state.entity).insert(target);
                    }
                    let Some(duration) = duration else {
                        continue;
                    };
                    let mut animations = Vec::new();
                    for (object, before, target) in targets {
                        if let Some(state) = builder.states.get_mut(object) {
                            state.transform = before;
                            builder.commands.entity(state.entity).insert(before);
                        }
                        animations.push(AnimationBuilder {
                            target: object,
                            anim_type: AnimationType::TranslateTo {
                                to: target.translation,
                            },
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                        animations.push(AnimationBuilder {
                            target: object,
                            anim_type: AnimationType::ScaleTo { to: target.scale },
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                    }
                    builder.play_parallel(animations);
                }
                Op::Wait(d) => builder.wait(*d),
                Op::CameraPosition { to, duration, .. } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraPosition {
                                    from: *camera_position,
                                    to: *to,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_position = *to;
                    builder.wait(*duration);
                }
                Op::CameraZoom { to, duration, .. } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraZoom {
                                    from: *camera_zoom,
                                    to: *to,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_zoom = *to;
                    builder.wait(*duration);
                }
                Op::CameraRotation { to, duration } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraRotation {
                                    from: *camera_rotation,
                                    to: *to,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_rotation = *to;
                    builder.wait(*duration);
                }
                Op::CameraFrame {
                    target,
                    margin,
                    duration,
                } => {
                    let Some(actual) = id_map.get(target).copied() else {
                        continue;
                    };
                    let Some(state) = builder.states.get(actual) else {
                        continue;
                    };
                    let bounds = state
                        .bounds
                        .transform_2d(&builder.get_world_transform(actual).to_affine_2d());
                    let width = (bounds.width() + margin * 2.0).max(1.0);
                    let height = (bounds.height() + margin * 2.0).max(1.0);
                    let zoom = (frame_bounds.width() / width)
                        .min(frame_bounds.height() / height)
                        .max(0.01);
                    let center = bounds.center();
                    for lens in [
                        gaanim_timeline::clip::PropertyLensSpec::CameraPosition {
                            from: *camera_position,
                            to: center,
                        },
                        gaanim_timeline::clip::PropertyLensSpec::CameraZoom {
                            from: *camera_zoom,
                            to: zoom,
                        },
                    ] {
                        builder.timeline.add_clip(
                            builder.default_track,
                            builder.current_time,
                            *duration,
                            gaanim_timeline::clip::ClipPayload::Animation(
                                gaanim_timeline::clip::AnimationSpec {
                                    target: gaanim_core::ObjectId::from_parts(0, 1),
                                    lens,
                                    rate_func: gaanim_math::RateFunc::Smooth,
                                    delay: 0.0,
                                    label: None,
                                },
                            ),
                        );
                    }
                    *camera_position = center;
                    *camera_zoom = zoom;
                    builder.wait(*duration);
                }
                Op::CameraFollow { target, duration } => {
                    let Some(actual) = id_map.get(target).copied() else {
                        continue;
                    };
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraFollow {
                                    target: actual,
                                },
                                rate_func: gaanim_math::RateFunc::Linear,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    if let Some(state) = builder.states.get(actual) {
                        camera_position.x = state.transform.translation.x;
                        camera_position.y = state.transform.translation.y;
                    }
                    builder.wait(*duration);
                }
                Op::CameraShake {
                    amplitude,
                    frequency,
                    duration,
                } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraShake {
                                    origin: *camera_position,
                                    amplitude: *amplitude,
                                    frequency: *frequency,
                                },
                                rate_func: gaanim_math::RateFunc::Linear,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    builder.wait(*duration);
                }
                Op::CameraLookAt {
                    eye,
                    target,
                    up,
                    duration,
                } => {
                    let from_eye = *camera_position;
                    let to_eye = *eye;
                    let from_target = *camera_target;
                    let to_target = *target;
                    // Compute rotations via look_at
                    let from_rot = *camera_rotation;
                    let view_to = gaanim_core::glam::DMat4::look_at_rh(to_eye, to_target, *up);
                    let to_rot = view_to.inverse().to_scale_rotation_translation().1;
                    // Position clip
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraPosition {
                                    from: from_eye,
                                    to: to_eye,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    // Target clip
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraTarget {
                                    from: from_target,
                                    to: to_target,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    // Rotation clip
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraRotation {
                                    from: from_rot,
                                    to: to_rot,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_position = to_eye;
                    *camera_target = to_target;
                    *camera_rotation = to_rot;
                    builder.wait(*duration);
                }
                Op::CameraOrbit {
                    delta_yaw,
                    delta_pitch,
                    duration,
                } => {
                    // Compute destination via spherical orbit around target
                    let mut temp_cam = gaanim_math::Camera::ortho_2d(1, 1);
                    temp_cam.position = *camera_position;
                    temp_cam.target = *camera_target;
                    temp_cam.rotation = *camera_rotation;
                    temp_cam.up = gaanim_core::glam::DVec3::Y;
                    temp_cam.projection = if let Some((fov, near, far)) = *camera_fov {
                        gaanim_math::Projection::Perspective {
                            fov_y: fov,
                            near,
                            far,
                        }
                    } else {
                        gaanim_math::Projection::Orthographic { zoom: *camera_zoom }
                    };
                    let from_pos = *camera_position;
                    let from_rot = *camera_rotation;
                    temp_cam.orbit_around_target(*delta_yaw, *delta_pitch);
                    let to_pos = temp_cam.position;
                    let to_rot = temp_cam.rotation;
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraPosition {
                                    from: from_pos,
                                    to: to_pos,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraRotation {
                                    from: from_rot,
                                    to: to_rot,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_position = to_pos;
                    *camera_rotation = to_rot;
                    builder.wait(*duration);
                }
                Op::CameraPerspective {
                    fov_y,
                    near,
                    far,
                    duration,
                } => {
                    let (from_fov, from_near, from_far) =
                        (*camera_fov).unwrap_or((std::f64::consts::FRAC_PI_4, 0.1, 1000.0));
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraPerspective {
                                    from_fov,
                                    to_fov: *fov_y,
                                    from_near,
                                    to_near: *near,
                                    from_far,
                                    to_far: *far,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_fov = Some((*fov_y, *near, *far));
                    builder.wait(*duration);
                }
                Op::CameraDolly { factor, duration } => {
                    let from_pos = *camera_position;
                    let dir = from_pos - *camera_target;
                    let to_pos = *camera_target + dir * (*factor).clamp(0.1, 10.0);
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraPosition {
                                    from: from_pos,
                                    to: to_pos,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_position = to_pos;
                    builder.wait(*duration);
                }
                Op::SetClip { target, mask, rule } => {
                    let Some(target) = id_map.get(target).copied() else {
                        continue;
                    };
                    let target_leaves = Self::visual_leaf_ids(builder, target);
                    if let Some(mask) = mask {
                        let Some(mask) = id_map.get(mask).copied() else {
                            continue;
                        };
                        let mask_world = Self::mask_path_in_world(builder, mask);
                        for leaf in target_leaves {
                            let Some(state) = builder.states.get(leaf) else {
                                continue;
                            };
                            let mut local_path = mask_world.clone();
                            local_path.apply_affine(
                                builder.get_world_transform(leaf).to_affine_2d().inverse(),
                            );
                            builder.commands.entity(state.entity).insert(
                                gaanim_renderer::effects::ClipMask {
                                    path: local_path,
                                    rule: *rule,
                                },
                            );
                        }
                    } else {
                        for leaf in target_leaves {
                            if let Some(state) = builder.states.get(leaf) {
                                builder
                                    .commands
                                    .entity(state.entity)
                                    .remove::<gaanim_renderer::effects::ClipMask>();
                            }
                        }
                    }
                }
                Op::Stop => builder.stop(),
                Op::Show(id) => {
                    if let Some(id) = id_map.get(id).copied()
                        && let Some(st) = builder.states.get_mut(id)
                    {
                        builder.commands.entity(st.entity).insert(Visible);
                    }
                }
                Op::Hide(id) => {
                    if let Some(id) = id_map.get(id).copied()
                        && let Some(st) = builder.states.get_mut(id)
                    {
                        builder.commands.entity(st.entity).remove::<Visible>();
                    }
                }
                Op::Remove(id) => {
                    if let Some(id) = id_map.get(id).copied()
                        && let Some(st) = builder.states.get(id)
                    {
                        builder.commands.entity(st.entity).despawn();
                    }
                }
                Op::AttachToGroup { group, child } => {
                    if let (Some(group), Some(child)) =
                        (id_map.get(group).copied(), id_map.get(child).copied())
                        && builder.states.get(group).is_some()
                        && builder.states.get(child).is_some()
                    {
                        builder.add_to_group(MobjectRef { id: group }, MobjectRef { id: child });
                    }
                }
                Op::PlaceAtCoordinate {
                    space,
                    target,
                    local,
                } => {
                    if let (Some(space), Some(target)) =
                        (id_map.get(space).copied(), id_map.get(target).copied())
                        && builder.states.get(space).is_some()
                        && builder.states.get(target).is_some()
                    {
                        builder.add_to_group(MobjectRef { id: space }, MobjectRef { id: target });
                        let view_local = builder
                            .states
                            .get(space)
                            .map(|s| s.transform)
                            .unwrap_or_default();
                        let inv = view_local.to_affine_2d().inverse();
                        let desired_point = Point::new(local.x, local.y);
                        let local_point = inv * desired_point;
                        if let Some(state) = builder.states.get_mut(target) {
                            state.transform.translation =
                                DVec3::new(local_point.x, local_point.y, local.z);
                            // Preserve any existing rotation/scale from add_to_group (which is identity for dots)
                            // but ensure translation is correct.
                            builder
                                .commands
                                .entity(state.entity)
                                .insert(state.transform);
                        }
                    }
                }
                Op::Reuse(target) => Self::apply_scene_object_scope(
                    builder,
                    *target,
                    SceneObjectScopeAction::Reuse,
                    scene_id,
                    previous_scene,
                    seg.transition.as_ref(),
                    scene_start,
                    id_map,
                    object_scopes,
                ),
                Op::Persist(target) => Self::apply_scene_object_scope(
                    builder,
                    *target,
                    SceneObjectScopeAction::Persist,
                    scene_id,
                    previous_scene,
                    seg.transition.as_ref(),
                    scene_start,
                    id_map,
                    object_scopes,
                ),
                Op::Release(target) => Self::apply_scene_object_scope(
                    builder,
                    *target,
                    SceneObjectScopeAction::Release,
                    scene_id,
                    previous_scene,
                    seg.transition.as_ref(),
                    scene_start,
                    id_map,
                    object_scopes,
                ),

                // -- Reactive ops --
                Op::AttachUpdater { target, preset } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let updater: Updater = preset
                            .clone()
                            .into_updater()
                            .starting_at(builder.current_time);
                        builder.commands.entity(st.entity).insert(updater);
                    }
                }

                Op::RemoveUpdater(target) => {
                    if let Some(target_id) = id_map.get(target).copied() {
                        builder.schedule_remove_updater(target_id);
                    }
                }

                Op::AttachTracedPath {
                    target,
                    source,
                    min_distance,
                    max_points,
                    dissipating_time,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(source_id) = id_map.get(source).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(source_st) = builder.states.get(source_id)
                    {
                        let traced = TracedPath::new(source_st.entity, *min_distance, *max_points)
                            .starting_at(builder.current_time)
                            .with_dissipating_time(*dissipating_time);
                        builder.commands.entity(target_st.entity).insert(traced);
                    }
                }

                Op::AttachPositionBinding {
                    target,
                    source,
                    axes,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(source_id) = id_map.get(source).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(source_st) = builder.states.get(source_id)
                    {
                        let binding = PositionBinding::new(source_st.entity, *axes);
                        builder.commands.entity(target_st.entity).insert(binding);
                    }
                }

                Op::AttachPositionFollow {
                    target,
                    source,
                    offset,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(source_id) = id_map.get(source).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(source_st) = builder.states.get(source_id)
                    {
                        builder.commands.entity(target_st.entity).insert(
                            PositionBinding::with_offset(
                                source_st.entity,
                                gaanim_animation::AxisMask::XY,
                                *offset,
                            ),
                        );
                    }
                }

                Op::AttachTrackingLine { target, from, to } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let resolve_endpoint = |ep: &CanvasEndpoint| -> TrackingEndpoint {
                            match ep {
                                CanvasEndpoint::Static(pos) => TrackingEndpoint::Static(*pos),
                                CanvasEndpoint::Entity(oid) => {
                                    if let Some(rid) = id_map.get(oid).copied() {
                                        if let Some(s) = builder.states.get(rid) {
                                            TrackingEndpoint::Entity(s.entity)
                                        } else {
                                            TrackingEndpoint::Static(DVec3::ZERO)
                                        }
                                    } else {
                                        TrackingEndpoint::Static(DVec3::ZERO)
                                    }
                                }
                            }
                        };
                        let line = TrackingLine::new(resolve_endpoint(from), resolve_endpoint(to));
                        builder.commands.entity(st.entity).insert(line);
                    }
                }

                Op::AttachTrackingSpring {
                    target,
                    from,
                    to,
                    coils,
                    amplitude,
                    crossing,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let resolve_endpoint = |ep: &CanvasEndpoint| -> TrackingEndpoint {
                            match ep {
                                CanvasEndpoint::Static(pos) => TrackingEndpoint::Static(*pos),
                                CanvasEndpoint::Entity(oid) => id_map
                                    .get(oid)
                                    .and_then(|rid| builder.states.get(*rid))
                                    .map(|state| TrackingEndpoint::Entity(state.entity))
                                    .unwrap_or(TrackingEndpoint::Static(DVec3::ZERO)),
                            }
                        };
                        let from = resolve_endpoint(from);
                        let to = resolve_endpoint(to);
                        let coils = *coils;
                        let amplitude = *amplitude;
                        let crossing = *crossing;
                        let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                            let endpoint_position = |endpoint: &TrackingEndpoint| match endpoint {
                                TrackingEndpoint::Static(position) => *position,
                                TrackingEndpoint::Entity(entity) => world
                                    .get::<SpatialTransform>(*entity)
                                    .map(|transform| transform.translation)
                                    .unwrap_or(DVec3::ZERO),
                            };
                            let from = endpoint_position(&from);
                            let to = endpoint_position(&to);
                            // Rebuild the projected helix every frame so an animated
                            // endpoint changes the spring pitch in lockstep.
                            gaanim_objects::primitives::spring_path(
                                Point::new(from.x, from.y),
                                Point::new(to.x, to.y),
                                coils,
                                amplitude,
                                crossing,
                            )
                        });
                        builder.commands.entity(st.entity).insert(redraw);
                    }
                }

                Op::AttachTrackingDimension {
                    target,
                    from,
                    to,
                    offset,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let resolve_endpoint = |ep: &CanvasEndpoint| -> TrackingEndpoint {
                            match ep {
                                CanvasEndpoint::Static(pos) => TrackingEndpoint::Static(*pos),
                                CanvasEndpoint::Entity(oid) => id_map
                                    .get(oid)
                                    .and_then(|rid| builder.states.get(*rid))
                                    .map(|state| TrackingEndpoint::Entity(state.entity))
                                    .unwrap_or(TrackingEndpoint::Static(DVec3::ZERO)),
                            }
                        };
                        let from = resolve_endpoint(from);
                        let to = resolve_endpoint(to);
                        let offset = *offset;
                        let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                            let endpoint_position = |endpoint: &TrackingEndpoint| match endpoint {
                                TrackingEndpoint::Static(position) => *position,
                                TrackingEndpoint::Entity(entity) => world
                                    .get::<SpatialTransform>(*entity)
                                    .map(|transform| transform.translation)
                                    .unwrap_or(DVec3::ZERO),
                            };
                            let from = endpoint_position(&from);
                            let to = endpoint_position(&to);
                            gaanim_objects::primitives::dimension_path(
                                Point::new(from.x, from.y),
                                Point::new(to.x, to.y),
                                offset,
                            )
                        });
                        builder.commands.entity(st.entity).insert(redraw);
                    }
                }

                Op::AttachTrackerArc {
                    target,
                    tracker,
                    center,
                    radius,
                    start_angle,
                    sweep_scale,
                    sweep_offset,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        let tracker_entity = tracker_st.entity;
                        let center = Point::new(center.0, center.1);
                        let radius = *radius;
                        let start_angle = *start_angle;
                        let sweep_scale = *sweep_scale;
                        let sweep_offset = *sweep_offset;
                        let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                            let value = world
                                .get::<gaanim_animation::FloatSignal>(tracker_entity)
                                .map(|signal| signal.value)
                                .unwrap_or(0.0);
                            gaanim_objects::primitives::curved_arrow_arc(
                                gaanim_core::ObjectId::from_raw(0),
                                center,
                                radius,
                                start_angle,
                                value * sweep_scale + sweep_offset,
                            )
                            .path
                            .0
                            .as_ref()
                            .clone()
                        });
                        builder.commands.entity(target_st.entity).insert(redraw);
                    }
                }
                Op::AttachPointOnCurve {
                    target,
                    curve,
                    tracker,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(curve_id) = id_map.get(curve).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(PointOnCurve::new(curve_st.entity, tracker_st.entity));
                    }
                }
                Op::AttachTangentOnCurve {
                    target,
                    curve,
                    tracker,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(curve_id) = id_map.get(curve).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(TangentOnCurve::new(curve_st.entity, tracker_st.entity));
                    }
                }
                Op::AttachNormalOnCurve {
                    target,
                    curve,
                    tracker,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(curve_id) = id_map.get(curve).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(NormalOnCurve::new(curve_st.entity, tracker_st.entity));
                    }
                }
                Op::AttachCurvatureOnCurve {
                    target,
                    curve,
                    tracker,
                    window,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(curve_id) = id_map.get(curve).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(CurvatureOnCurve::new(
                                curve_st.entity,
                                tracker_st.entity,
                                *window,
                            ));
                    }
                }
                Op::AttachCustomUpdater { target, updater } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(state) = builder.states.get(target_id)
                    {
                        builder
                            .commands
                            .entity(state.entity)
                            .insert(updater.clone().starting_at(builder.current_time));
                    }
                }
                Op::AttachTracedPath3D {
                    target,
                    source,
                    min_distance,
                    max_points,
                    colormap,
                    dissipating_time,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(source_id) = id_map.get(source).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(source_st) = builder.states.get(source_id)
                    {
                        let comp = gaanim_animation::TracedPath3D::new(
                            source_st.entity,
                            *min_distance,
                            *max_points,
                            colormap.clone(),
                        )
                        .starting_at(builder.current_time)
                        .with_dissipating_time(*dissipating_time);
                        builder.commands.entity(target_st.entity).insert(comp);
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_scene_object_scope(
        builder: &mut SceneBuilder,
        logical_target: ObjectId,
        action: SceneObjectScopeAction,
        scene_id: SceneId,
        previous_scene: Option<SceneId>,
        transition: Option<&gaanim_timeline::transition::TransitionType>,
        scene_start: f64,
        id_map: &HashMap<ObjectId, ObjectId>,
        object_scopes: &mut HashMap<ObjectId, CompiledObjectScope>,
    ) {
        let Some(&target) = id_map.get(&logical_target) else {
            return;
        };
        builder.manage_hierarchy_membership(target);
        let current_time = builder.current_time;
        let at_scene_start = (current_time - scene_start).abs() <= 1e-9;
        let transition_duration = transition.map(|value| value.duration()).unwrap_or(0.0);
        let current_scope = if builder.is_persistent(target) {
            CompiledObjectScope::Persistent
        } else {
            object_scopes
                .get(&logical_target)
                .copied()
                .unwrap_or(CompiledObjectScope::Segment(scene_id))
        };
        let visible_before = current_scope == CompiledObjectScope::Persistent
            || previous_scene
                .is_some_and(|previous| current_scope == CompiledObjectScope::Segment(previous));

        let next_scope = match action {
            SceneObjectScopeAction::Reuse if current_scope == CompiledObjectScope::Persistent => {
                CompiledObjectScope::Persistent
            }
            SceneObjectScopeAction::Reuse => {
                if at_scene_start && visible_before && transition_duration > 0.0 {
                    builder.schedule_scene_membership(target, None, scene_start);
                    builder.schedule_scene_membership(
                        target,
                        Some(scene_id),
                        scene_start + transition_duration,
                    );
                } else {
                    builder.schedule_scene_membership(target, Some(scene_id), current_time);
                }
                builder.set_hierarchy_persistent(target, false);
                CompiledObjectScope::Segment(scene_id)
            }
            SceneObjectScopeAction::Persist => {
                if current_scope != CompiledObjectScope::Persistent {
                    if at_scene_start && !visible_before && transition_duration > 0.0 {
                        builder.schedule_scene_membership(target, Some(scene_id), scene_start);
                        builder.schedule_scene_membership(
                            target,
                            None,
                            scene_start + transition_duration,
                        );
                    } else {
                        builder.schedule_scene_membership(target, None, current_time);
                    }
                    builder.set_hierarchy_persistent(target, true);
                }
                CompiledObjectScope::Persistent
            }
            SceneObjectScopeAction::Release => {
                if at_scene_start
                    && current_scope == CompiledObjectScope::Persistent
                    && transition_duration > 0.0
                {
                    builder.schedule_scene_membership(
                        target,
                        Some(scene_id),
                        scene_start + transition_duration,
                    );
                } else if current_scope != CompiledObjectScope::Segment(scene_id) {
                    builder.schedule_scene_membership(target, Some(scene_id), current_time);
                }
                builder.set_hierarchy_persistent(target, false);
                CompiledObjectScope::Segment(scene_id)
            }
        };

        let hierarchy: HashSet<ObjectId> = builder.hierarchy_ids(target).into_iter().collect();
        for (logical, actual) in id_map {
            if hierarchy.contains(actual) {
                object_scopes.insert(*logical, next_scope);
            }
        }
    }

    fn transform_targets(ops: &[Op]) -> std::collections::HashSet<ObjectId> {
        let mut targets = std::collections::HashSet::new();
        for op in ops {
            match op {
                Op::Animate { anim, active } if *active => match &anim.anim_type {
                    AnimationType::Transform { target }
                    | AnimationType::ReplacementTransform { target }
                    | AnimationType::FadeTransform { target } => {
                        targets.insert(*target);
                    }
                    _ => {}
                },
                Op::Play(anims) => {
                    for anim in anims {
                        match &anim.anim_type {
                            AnimationType::Transform { target }
                            | AnimationType::ReplacementTransform { target }
                            | AnimationType::FadeTransform { target } => {
                                targets.insert(*target);
                            }
                            _ => {}
                        }
                    }
                }
                Op::TransformMatching { target, .. } => {
                    targets.insert(*target);
                }
                _ => {}
            }
        }
        targets
    }

    fn reveal_deferred_on_play(
        builder: &mut SceneBuilder,
        deferred_visibility: &HashSet<ObjectId>,
        revealed_deferred: &mut HashSet<ObjectId>,
        anim: &AnimationBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
    ) {
        if !deferred_visibility.contains(&anim.target)
            || !Self::animation_reveals_deferred(&anim.anim_type)
            || !revealed_deferred.insert(anim.target)
        {
            return;
        }

        let Some(&actual) = id_map.get(&anim.target) else {
            return;
        };

        // FadeIn/FadeInFrom already author their own root opacity lens, but
        // composite deferred objects still need their descendants restored.
        // Other animations need an instantaneous reveal so Create/Write/
        // movement animations can be used directly in scene.play as the entry
        // point.
        if matches!(
            &anim.anim_type,
            AnimationType::FadeIn | AnimationType::FadeInFrom { .. }
        ) {
            builder
                .schedule_show_descendants_at(actual, builder.current_time + anim.delay.max(0.0));
            return;
        }

        builder.schedule_show_at(actual, builder.current_time + anim.delay.max(0.0));
    }

    fn animation_reveals_deferred(anim_type: &AnimationType) -> bool {
        match anim_type {
            AnimationType::FadeOut
            | AnimationType::Unwrite { .. }
            | AnimationType::Uncreate { .. }
            | AnimationType::ShrinkToCenter => false,
            AnimationType::FadeTo { to } => *to > 0.0,
            _ => true,
        }
    }

    fn add_camera_lens(
        builder: &mut SceneBuilder,
        start: f64,
        anim: &AnimationBuilder,
        lens: gaanim_timeline::clip::PropertyLensSpec,
    ) {
        builder.timeline.add_clip(
            builder.default_track,
            start + anim.delay.max(0.0),
            anim.duration.max(0.0),
            gaanim_timeline::clip::ClipPayload::Animation(gaanim_timeline::clip::AnimationSpec {
                target: gaanim_core::ObjectId::from_parts(0, 1),
                lens,
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: Some("Camera".to_string()),
            }),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn schedule_camera_animation(
        builder: &mut SceneBuilder,
        frame_bounds: Bounds3D,
        camera_position: &mut DVec3,
        camera_zoom: &mut f64,
        camera_rotation: &mut gaanim_core::glam::DQuat,
        camera_target: &mut DVec3,
        camera_fov: &mut Option<(f64, f64, f64)>,
        anim: &AnimationBuilder,
        start: f64,
    ) {
        use gaanim_timeline::clip::PropertyLensSpec;

        match &anim.anim_type {
            AnimationType::CameraPosition { to } => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraPosition {
                        from: *camera_position,
                        to: *to,
                    },
                );
                *camera_position = *to;
            }
            AnimationType::CameraZoom { to } => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraZoom {
                        from: *camera_zoom,
                        to: *to,
                    },
                );
                *camera_zoom = *to;
            }
            AnimationType::CameraRotation { to } => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraRotation {
                        from: *camera_rotation,
                        to: *to,
                    },
                );
                *camera_rotation = *to;
            }
            AnimationType::CameraFrame { target, margin } => {
                let Some(state) = builder.states.get(*target) else {
                    return;
                };
                let bounds = state
                    .bounds
                    .transform_2d(&builder.get_world_transform(*target).to_affine_2d());
                let width = (bounds.width() + margin * 2.0).max(1.0);
                let height = (bounds.height() + margin * 2.0).max(1.0);
                let zoom = (frame_bounds.width() / width)
                    .min(frame_bounds.height() / height)
                    .max(0.01);
                let center = bounds.center();
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraPosition {
                        from: *camera_position,
                        to: center,
                    },
                );
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraZoom {
                        from: *camera_zoom,
                        to: zoom,
                    },
                );
                *camera_position = center;
                *camera_zoom = zoom;
            }
            AnimationType::CameraFollow { target } => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraFollow { target: *target },
                );
                if let Some(state) = builder.states.get(*target) {
                    camera_position.x = state.transform.translation.x;
                    camera_position.y = state.transform.translation.y;
                }
            }
            AnimationType::CameraShake {
                amplitude,
                frequency,
            } => Self::add_camera_lens(
                builder,
                start,
                anim,
                PropertyLensSpec::CameraShake {
                    origin: *camera_position,
                    amplitude: *amplitude,
                    frequency: *frequency,
                },
            ),
            AnimationType::CameraLookAt { eye, target, up } => {
                let to_rotation = gaanim_core::glam::DMat4::look_at_rh(*eye, *target, *up)
                    .inverse()
                    .to_scale_rotation_translation()
                    .1;
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraPosition {
                        from: *camera_position,
                        to: *eye,
                    },
                );
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraTarget {
                        from: *camera_target,
                        to: *target,
                    },
                );
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraRotation {
                        from: *camera_rotation,
                        to: to_rotation,
                    },
                );
                *camera_position = *eye;
                *camera_target = *target;
                *camera_rotation = to_rotation;
            }
            AnimationType::CameraOrbit {
                delta_yaw,
                delta_pitch,
            } => {
                let mut camera = gaanim_math::Camera::ortho_2d(1, 1);
                camera.position = *camera_position;
                camera.target = *camera_target;
                camera.rotation = *camera_rotation;
                camera.up = DVec3::Y;
                camera.projection = if let Some((fov_y, near, far)) = *camera_fov {
                    gaanim_math::Projection::Perspective { fov_y, near, far }
                } else {
                    gaanim_math::Projection::Orthographic { zoom: *camera_zoom }
                };
                camera.orbit_around_target(*delta_yaw, *delta_pitch);
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraPosition {
                        from: *camera_position,
                        to: camera.position,
                    },
                );
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraRotation {
                        from: *camera_rotation,
                        to: camera.rotation,
                    },
                );
                *camera_position = camera.position;
                *camera_rotation = camera.rotation;
            }
            AnimationType::CameraPerspective { fov_y, near, far } => {
                let (from_fov, from_near, from_far) =
                    camera_fov.unwrap_or((std::f64::consts::FRAC_PI_4, 0.1, 1000.0));
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraPerspective {
                        from_fov,
                        to_fov: *fov_y,
                        from_near,
                        to_near: *near,
                        from_far,
                        to_far: *far,
                    },
                );
                *camera_fov = Some((*fov_y, *near, *far));
            }
            AnimationType::CameraDolly { factor } => {
                let direction = *camera_position - *camera_target;
                let destination = *camera_target + direction * factor.clamp(0.1, 10.0);
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraPosition {
                        from: *camera_position,
                        to: destination,
                    },
                );
                *camera_position = destination;
            }
            _ => {}
        }
    }

    fn remap_anim(
        anim: &AnimationBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
    ) -> Option<AnimationBuilder> {
        let target = if anim.anim_type.is_camera() {
            anim.target
        } else {
            *id_map.get(&anim.target)?
        };
        let anim_type = match &anim.anim_type {
            AnimationType::CameraFrame { target, margin } => AnimationType::CameraFrame {
                target: *id_map.get(target)?,
                margin: *margin,
            },
            AnimationType::CameraFollow { target } => AnimationType::CameraFollow {
                target: *id_map.get(target)?,
            },
            AnimationType::FadeTransform { target } => AnimationType::FadeTransform {
                target: *id_map.get(target)?,
            },
            AnimationType::Transform { target } => AnimationType::Transform {
                target: *id_map.get(target)?,
            },
            AnimationType::ReplacementTransform { target } => AnimationType::ReplacementTransform {
                target: *id_map.get(target)?,
            },
            AnimationType::MoveAlongPath { path, path_target } => AnimationType::MoveAlongPath {
                path: path.clone(),
                path_target: path_target.map(|id| *id_map.get(&id).unwrap_or(&id)),
            },
            other => other.clone(),
        };
        Some(AnimationBuilder {
            target,
            anim_type,
            duration: anim.duration,
            delay: anim.delay,
            rate_func: anim.rate_func.clone(),
        })
    }

    fn fragment_child_ids(
        builder: &mut SceneBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
        target: ObjectId,
        fragment: &str,
        occurrence: Option<usize>,
    ) -> Vec<ObjectId> {
        let Some(&target) = id_map.get(&target) else {
            return Vec::new();
        };
        builder
            .select_occurrence(MobjectRef { id: target }, fragment, occurrence)
            .child_ids
    }

    fn fade_cancellation_marks(
        builder: &mut SceneBuilder,
        cancellation_marks: &mut HashMap<ObjectId, Vec<ObjectId>>,
        source: ObjectId,
        transition_duration: f64,
    ) {
        let Some(marks) = cancellation_marks.remove(&source) else {
            return;
        };
        let duration = (transition_duration * 0.25).clamp(0.12, 0.3);
        for target in marks {
            builder.play_at_current_time(AnimationBuilder {
                target,
                anim_type: AnimationType::FadeOut,
                duration,
                rate_func: RateFunc::Smooth,
                delay: 0.0,
            });
        }
    }

    fn fade_canceled_term_children(
        builder: &mut SceneBuilder,
        canceled_term_children: &mut HashMap<ObjectId, Vec<ObjectId>>,
        source: ObjectId,
        transition_duration: f64,
    ) {
        let Some(children) = canceled_term_children.remove(&source) else {
            return;
        };
        let duration = (transition_duration * 0.25).clamp(0.12, 0.3);
        for target in children {
            builder.play_at_current_time(AnimationBuilder {
                target,
                anim_type: AnimationType::FadeOut,
                duration,
                rate_func: RateFunc::Smooth,
                delay: 0.0,
            });
        }
    }

    fn spawn_one(
        builder: &mut SceneBuilder,
        spec: &ObjectSpec,
        id_map: &HashMap<ObjectId, ObjectId>,
        frame_bounds: Bounds3D,
        text_config: &gaanim_text::prelude::TextConfig,
        scene_background: gaanim_core::peniko::Color,
    ) -> MobjectRef {
        match &spec.kind {
            SpawnKind::Circle(r) => {
                let b = builder.circle(*r);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Rect(w, h) => {
                let b = builder.rectangle(*w, *h);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::RoundedRect(w, h, r) => {
                let b = builder.rounded_rect(*w, *h, *r);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Square(sz) => {
                let b = builder.square(*sz);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Dot(r) => {
                let b = builder.dot(*r);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Ellipse(rx, ry) => {
                let b = builder.ellipse(*rx, *ry);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Line(x1, y1, x2, y2) => {
                let b = builder.line(Point::new(*x1, *y1), Point::new(*x2, *y2));
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Arrow(x1, y1, x2, y2) => {
                let b = builder.arrow(Point::new(*x1, *y1), Point::new(*x2, *y2));
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::DashedLine {
                start,
                end,
                dash_length,
                gap_length,
            } => {
                let b = builder.dashed_line(
                    Point::new(start.0, start.1),
                    Point::new(end.0, end.1),
                    *dash_length,
                    *gap_length,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::DoubleArrow {
                start,
                end,
                head_length,
                head_width,
            } => {
                let b = builder.double_arrow(
                    Point::new(start.0, start.1),
                    Point::new(end.0, end.1),
                    *head_length,
                    *head_width,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Polygon(points) => {
                let points: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
                let b = builder.polygon(&points);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Star {
                points,
                outer_radius,
                inner_radius,
            } => {
                let b = builder.star(*points, *outer_radius, *inner_radius);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::RegularPolygon { sides, radius } => {
                let b = builder.regular_polygon(*sides, *radius);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Sector {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                let b = builder.sector(
                    Point::new(center.0, center.1),
                    *radius,
                    *start_angle,
                    *sweep_angle,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Annulus {
                outer_radius,
                inner_radius,
            } => {
                let b = builder.annulus(*outer_radius, *inner_radius);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Brace { start, end, height } => {
                let b = builder.brace(
                    Point::new(start.0, start.1),
                    Point::new(end.0, end.1),
                    *height,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Checkmark(size) => {
                let b = builder.checkmark(*size);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Cross(size) => {
                let b = builder.cross(*size);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::RightAngle(arm_length) => {
                let b = builder.right_angle(*arm_length);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Arc {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                let b = builder.arc(
                    Point::new(center.0, center.1),
                    Vec2::new(*radius, *radius),
                    *start_angle,
                    *sweep_angle,
                    0.0,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::CurvedArrow(x1, y1, x2, y2, angle) => {
                let b = builder.curved_arrow(Point::new(*x1, *y1), Point::new(*x2, *y2), *angle);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::CurvedArrowArc {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                let b = builder.curved_arrow_arc(
                    Point::new(center.0, center.1),
                    *radius,
                    *start_angle,
                    *sweep_angle,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Dimension { start, end, offset } => {
                let dx = end.0 - start.0;
                let dy = end.1 - start.1;
                let length = dx.hypot(dy);
                if length <= f64::EPSILON {
                    let b = builder.line(Point::new(start.0, start.1), Point::new(end.0, end.1));
                    let mr = Self::finish_spawn_builder(b, spec);
                    Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                    mr
                } else {
                    let normal = (-dy / length, dx / length);
                    let dimension_start =
                        Point::new(start.0 + normal.0 * *offset, start.1 + normal.1 * *offset);
                    let dimension_end =
                        Point::new(end.0 + normal.0 * *offset, end.1 + normal.1 * *offset);
                    let color = PenikoColor::from_rgb8(0x80, 0x80, 0x80);
                    let extension_a = builder
                        .line(Point::new(start.0, start.1), dimension_start)
                        .no_fill()
                        .stroke(color, 2.0)
                        .spawn();
                    let extension_b = builder
                        .line(Point::new(end.0, end.1), dimension_end)
                        .no_fill()
                        .stroke(color, 2.0)
                        .spawn();
                    let measurement = builder
                        .double_arrow(dimension_start, dimension_end, Some(12.0), Some(10.0))
                        .fill(color)
                        .no_stroke()
                        .spawn();
                    let group = builder.group(&[extension_a, extension_b, measurement]);
                    Self::post_apply(builder, group.id, spec, id_map, frame_bounds);
                    group
                }
            }
            SpawnKind::Polyline(points) => {
                let points: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
                let b = builder.open_path(&points);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Bezier {
                start,
                controls,
                end,
            } => {
                let mut path = gaanim_core::kurbo::BezPath::new();
                path.move_to(Point::new(start.0, start.1));
                match controls.as_slice() {
                    [control] => {
                        path.quad_to(Point::new(control.0, control.1), Point::new(end.0, end.1))
                    }
                    [control1, control2] => path.curve_to(
                        Point::new(control1.0, control1.1),
                        Point::new(control2.0, control2.1),
                        Point::new(end.0, end.1),
                    ),
                    _ => path.line_to(Point::new(end.0, end.1)),
                }
                let rect = path.bounding_box();
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "Bezier".to_string(),
                    path,
                    bounds: Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1),
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                };
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Curve(elements) => {
                let mut path = gaanim_core::kurbo::BezPath::new();
                let mut cursor = Point::ORIGIN;
                let mut subpath_start = Point::ORIGIN;
                let mut last_quad = None;
                let mut last_cubic = None;
                let resolve = |point: (f64, f64), relative: bool, cursor: Point| {
                    if relative {
                        Point::new(cursor.x + point.0, cursor.y + point.1)
                    } else {
                        Point::new(point.0, point.1)
                    }
                };
                for element in elements {
                    match element {
                        crate::canvas::CurveElement::Move { to, relative } => {
                            cursor = resolve(*to, *relative, cursor);
                            subpath_start = cursor;
                            path.move_to(cursor);
                            last_quad = None;
                            last_cubic = None;
                        }
                        crate::canvas::CurveElement::Line { to, relative } => {
                            cursor = resolve(*to, *relative, cursor);
                            path.line_to(cursor);
                            last_quad = None;
                            last_cubic = None;
                        }
                        crate::canvas::CurveElement::Quad {
                            control,
                            to,
                            relative,
                        } => {
                            let end = resolve(*to, *relative, cursor);
                            let control = match control {
                                crate::canvas::CurveControl::None => end,
                                crate::canvas::CurveControl::Auto => last_quad
                                    .map(|p: Point| {
                                        Point::new(2.0 * cursor.x - p.x, 2.0 * cursor.y - p.y)
                                    })
                                    .unwrap_or(cursor),
                                crate::canvas::CurveControl::Point(point) => {
                                    resolve(*point, *relative, cursor)
                                }
                            };
                            path.quad_to(control, end);
                            cursor = end;
                            last_quad = Some(control);
                            last_cubic = None;
                        }
                        crate::canvas::CurveElement::Cubic {
                            control_start,
                            control_end,
                            to,
                            relative,
                        } => {
                            let end = resolve(*to, *relative, cursor);
                            let start = match control_start {
                                crate::canvas::CurveControl::None => cursor,
                                crate::canvas::CurveControl::Auto => last_cubic
                                    .map(|p: Point| {
                                        Point::new(2.0 * cursor.x - p.x, 2.0 * cursor.y - p.y)
                                    })
                                    .unwrap_or(cursor),
                                crate::canvas::CurveControl::Point(point) => {
                                    resolve(*point, *relative, cursor)
                                }
                            };
                            let finish = match control_end {
                                crate::canvas::CurveControl::None => end,
                                crate::canvas::CurveControl::Auto => end,
                                crate::canvas::CurveControl::Point(point) => {
                                    resolve(*point, *relative, cursor)
                                }
                            };
                            path.curve_to(start, finish, end);
                            cursor = end;
                            last_cubic = Some(finish);
                            last_quad = None;
                        }
                        crate::canvas::CurveElement::Close { smooth } => {
                            if *smooth {
                                let control = last_cubic
                                    .or(last_quad)
                                    .map(|p: Point| {
                                        Point::new(2.0 * cursor.x - p.x, 2.0 * cursor.y - p.y)
                                    })
                                    .unwrap_or(cursor);
                                path.curve_to(control, subpath_start, subpath_start);
                            } else {
                                path.close_path();
                            }
                            cursor = subpath_start;
                            last_quad = None;
                            last_cubic = None;
                        }
                    }
                }
                let rect = path.bounding_box();
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "Curve".to_string(),
                    path,
                    bounds: Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1),
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                };
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::ExpressionPlot {
                map,
                expression,
                variable,
                domain,
                sampling,
            } => {
                let parameter_entities: Vec<(gaanim_core::ObjectId, bevy::prelude::Entity)> =
                    expression
                        .parameter_ids()
                        .into_iter()
                        .filter_map(|logical| {
                            let actual = id_map.get(&logical).copied()?;
                            let entity = builder.states.get(actual)?.entity;
                            Some((logical, entity))
                        })
                        .collect();
                let mut context = gaanim_expr::EvalContext::new();
                for (logical, _) in &parameter_entities {
                    if let Some(actual) = id_map.get(logical).copied()
                        && let Some(value) = builder.float_signals.get(&actual).copied()
                    {
                        context.set_parameter(*logical, value);
                    }
                }
                let path = gaanim_visualization::sample_expression(
                    map, expression, variable, *domain, *sampling, &context,
                )
                .map(|sampled| sampled.to_bez_path())
                .unwrap_or_default();
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "ExpressionPlot".to_owned(),
                    path,
                    bounds: map.frame.bounds(),
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                };
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                if !parameter_entities.is_empty()
                    && let Some(state) = builder.states.get(mr.id)
                {
                    let map = map.clone();
                    let expression = expression.clone();
                    let variable = variable.clone();
                    let domain = *domain;
                    let sampling = *sampling;
                    let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                        let mut context = gaanim_expr::EvalContext::new();
                        for (logical, entity) in &parameter_entities {
                            if let Some(signal) =
                                world.get::<gaanim_animation::FloatSignal>(*entity)
                            {
                                context.set_parameter(*logical, signal.value);
                            }
                        }
                        gaanim_visualization::sample_expression(
                            &map,
                            &expression,
                            &variable,
                            domain,
                            sampling,
                            &context,
                        )
                        .map(|sampled| sampled.to_bez_path())
                        .unwrap_or_default()
                    });
                    builder.commands.entity(state.entity).insert(redraw);
                }
                mr
            }
            SpawnKind::DataMark { map, source, kind } => {
                let path = gaanim_visualization::data_mark_path(map, &source.snapshot(), kind)
                    .unwrap_or_default();
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "DataMark".to_owned(),
                    path: path.clone(),
                    bounds: map.frame.bounds(),
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                };
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                if let Some(state) = builder.states.get(mr.id) {
                    let map = map.clone();
                    let source = source.clone();
                    let kind = kind.clone();
                    let initial_version = source.version();
                    let cache = Arc::new(Mutex::new((initial_version, path)));
                    let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |_world| {
                        let version = source.version();
                        let mut cached = cache.lock().expect("data mark cache poisoned");
                        if cached.0 != version {
                            cached.1 = gaanim_visualization::data_mark_path(
                                &map,
                                &source.snapshot(),
                                &kind,
                            )
                            .unwrap_or_default();
                            cached.0 = version;
                        }
                        cached.1.clone()
                    });
                    builder.commands.entity(state.entity).insert(redraw);
                }
                mr
            }
            SpawnKind::Axes {
                x_range,
                y_range,
                config,
            } => {
                let axes = Self::styled_axes(builder, *x_range, *y_range, config, frame_bounds);
                Self::post_apply(builder, axes.id, spec, id_map, frame_bounds);
                axes
            }
            SpawnKind::Axes3D {
                x_range,
                y_range,
                z_range,
                config,
            } => {
                let axes = Self::styled_axes_3d(
                    builder,
                    *x_range,
                    *y_range,
                    *z_range,
                    config,
                    frame_bounds,
                );
                Self::post_apply(builder, axes.id, spec, id_map, frame_bounds);
                axes
            }
            SpawnKind::SurfaceMesh {
                vertices,
                indices,
                color,
                colors,
            } => {
                let colors = colors.as_ref().map(|colors| {
                    colors
                        .iter()
                        .map(|color| {
                            let rgba = color.to_rgba8();
                            [
                                rgba.r as f32 / 255.0,
                                rgba.g as f32 / 255.0,
                                rgba.b as f32 / 255.0,
                                rgba.a as f32 / 255.0,
                            ]
                        })
                        .collect()
                });
                let mref = builder.spawn_triangle_mesh_with_colors(
                    vertices.clone(),
                    indices.clone(),
                    *color,
                    colors,
                );
                Self::post_apply(builder, mref.id, spec, id_map, frame_bounds);
                mref
            }
            SpawnKind::Polyline3D { points, colors } => {
                let base_color = spec
                    .stroke
                    .as_ref()
                    .and_then(|(brush, _)| match brush {
                        gaanim_core::peniko::Brush::Solid(color) => Some(*color),
                        _ => None,
                    })
                    .or_else(|| {
                        spec.fill.as_ref().and_then(|brush| match brush {
                            gaanim_core::peniko::Brush::Solid(color) => Some(*color),
                            _ => None,
                        })
                    })
                    .unwrap_or(gaanim_core::peniko::Color::from_rgb8(20, 20, 20));
                let mref = if let Some(cols) = colors {
                    // Convert peniko::Color Vec to linear RGBA f32 for vertex colors
                    let cols_f32: Vec<[f32; 4]> = cols
                        .iter()
                        .map(|c| {
                            let rgba = c.to_rgba8();
                            [
                                rgba.r as f32 / 255.0,
                                rgba.g as f32 / 255.0,
                                rgba.b as f32 / 255.0,
                                rgba.a as f32 / 255.0,
                            ]
                        })
                        .collect();
                    if cols_f32.len() == points.len() {
                        builder.spawn_line_strip_with_colors(
                            points.clone(),
                            base_color,
                            Some(cols_f32),
                        )
                    } else {
                        // Mismatched lengths: fallback to uniform color (ignore per-vertex)
                        builder.spawn_line_strip(points.clone(), base_color)
                    }
                } else {
                    builder.spawn_line_strip(points.clone(), base_color)
                };
                Self::post_apply(builder, mref.id, spec, id_map, frame_bounds);
                mref
            }
            SpawnKind::LineSegments3D { points, colors } => {
                let base_color = spec
                    .stroke
                    .as_ref()
                    .and_then(|(brush, _)| match brush {
                        gaanim_core::peniko::Brush::Solid(color) => Some(*color),
                        _ => None,
                    })
                    .or_else(|| {
                        spec.fill.as_ref().and_then(|brush| match brush {
                            gaanim_core::peniko::Brush::Solid(color) => Some(*color),
                            _ => None,
                        })
                    })
                    .unwrap_or(gaanim_core::peniko::Color::from_rgb8(20, 20, 20));
                let colors = colors.as_ref().map(|colors| {
                    colors
                        .iter()
                        .map(|color| {
                            let rgba = color.to_rgba8();
                            [
                                rgba.r as f32 / 255.0,
                                rgba.g as f32 / 255.0,
                                rgba.b as f32 / 255.0,
                                rgba.a as f32 / 255.0,
                            ]
                        })
                        .collect::<Vec<_>>()
                });
                let mref = builder.spawn_line_list_with_colors(
                    points.clone(),
                    base_color,
                    colors.filter(|colors| colors.len() == points.len()),
                );
                Self::post_apply(builder, mref.id, spec, id_map, frame_bounds);
                mref
            }
            SpawnKind::GltfNode {
                node_index,
                path,
                bounds,
            } => {
                let id = builder.next_id();
                let transform = SpatialTransform::identity();
                let entity = builder
                    .commands
                    .spawn((
                        GltfNodeWrapper {
                            node_index: *node_index,
                            path: path.clone(),
                        },
                        GroupMarker,
                        MobjectId(id),
                        transform,
                        GlobalSpatialTransform::from_local(&transform),
                        Transform::default(),
                        Visibility::default(),
                        Opacity::default(),
                        GlobalOpacity::default(),
                        LocalBounds(*bounds),
                        WorldBounds(*bounds),
                        RenderLayer::Wgpu3D,
                        RenderOrder::default(),
                        Visible,
                    ))
                    .id();
                builder.tag_entity(entity);
                builder.states.insert(
                    id,
                    MobjectState {
                        path: std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
                        bounds: *bounds,
                        transform,
                        opacity: 1.0,
                        fill: None,
                        stroke: StrokeBrush::transparent(),
                        entity,
                        child_spans: Vec::new(),
                        children: Vec::new(),
                        parent: None,
                        exclude_from_parent_draw: false,
                    },
                );
                let mref = MobjectRef { id };
                Self::post_apply(builder, id, spec, id_map, frame_bounds);
                mref
            }
            SpawnKind::GltfModel {
                path,
                scene_index,
                bounds,
                nodes,
                animation_names,
            } => {
                let id = builder.next_id();
                let transform = SpatialTransform::identity();
                let entity = builder
                    .commands
                    .spawn((
                        GroupMarker,
                        MobjectId(id),
                        transform,
                        GlobalSpatialTransform::from_local(&transform),
                        Transform::default(),
                        Visibility::default(),
                        Opacity::default(),
                        GlobalOpacity::default(),
                        LocalBounds(*bounds),
                        WorldBounds(*bounds),
                        RenderLayer::Wgpu3D,
                        RenderOrder::default(),
                        Visible,
                    ))
                    .id();
                builder.tag_entity(entity);

                let mapped = nodes
                    .iter()
                    .filter_map(|(index, _, node_path, canvas_id)| {
                        let compiled_id = id_map.get(canvas_id).copied()?;
                        let wrapper = builder.states.get(compiled_id)?.entity;
                        Some((*index, node_path.clone(), compiled_id, wrapper))
                    })
                    .collect::<Vec<_>>();
                let node_entities = mapped
                    .iter()
                    .map(|(index, _, _, wrapper)| (*index, *wrapper))
                    .collect::<HashMap<_, _>>();
                let canvas_ids = mapped
                    .iter()
                    .map(|(index, _, canvas_id, _)| (*index, *canvas_id))
                    .collect::<HashMap<_, _>>();
                for (node_index, parent_index, _, canvas_id) in nodes {
                    let Some(compiled_id) = id_map.get(canvas_id).copied() else {
                        continue;
                    };
                    let Some(state) = builder.states.get_mut(compiled_id) else {
                        continue;
                    };
                    let parent_entity = parent_index
                        .and_then(|parent| node_entities.get(&parent).copied())
                        .unwrap_or(entity);
                    state.parent = parent_index
                        .and_then(|parent| canvas_ids.get(&parent).copied())
                        .or(Some(id));
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(ChildOf(parent_entity));
                    let _ = node_index;
                }
                builder.commands.entity(entity).insert(GltfModelRoot {
                    path: path.clone(),
                    scene_index: *scene_index,
                    nodes: mapped
                        .iter()
                        .map(|(node_index, node_path, _, wrapper)| GltfNodeBinding {
                            node_index: *node_index,
                            path: node_path.clone(),
                            wrapper: *wrapper,
                        })
                        .collect(),
                    animation_names: animation_names.clone(),
                });
                let child_ids = mapped.iter().map(|(_, _, id, _)| *id).collect::<Vec<_>>();
                builder.states.insert(
                    id,
                    MobjectState {
                        path: std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
                        bounds: *bounds,
                        transform,
                        opacity: 1.0,
                        fill: None,
                        stroke: StrokeBrush::transparent(),
                        entity,
                        child_spans: Vec::new(),
                        children: child_ids,
                        parent: None,
                        exclude_from_parent_draw: false,
                    },
                );
                let mref = MobjectRef { id };
                Self::post_apply(builder, id, spec, id_map, frame_bounds);
                mref
            }
            SpawnKind::Text(t) => {
                let role = gaanim_text::prelude::TextRole::Body;
                let styled_spec =
                    Self::with_default_text_fill(spec, text_config.roles[&role].fill_color);
                let color = match styled_spec.fill {
                    Some(gaanim_core::peniko::Brush::Solid(c)) => c,
                    _ => text_config.roles[&role].fill_color,
                };
                let has_inline_math = split_text_math(t)
                    .iter()
                    .any(|(is_math, c)| *is_math && !c.trim().is_empty());
                let mr = if has_inline_math {
                    let style = &text_config.roles[&role];
                    let source = text_inline_typst_source(t, color);
                    builder.typst(
                        &source,
                        false,
                        Some(&style.font_family),
                        None,
                        Some(style.size),
                        None,
                    )
                } else {
                    builder.spawn_text(t, role)
                };
                Self::post_apply(builder, mr.id, &styled_spec, id_map, frame_bounds);
                Self::apply_fragment_fills(builder, mr, &styled_spec);
                mr
            }
            SpawnKind::Paragraph { text, options } => {
                let body = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                let font_size = options.font_size.unwrap_or(body.size).max(1.0);
                let font_family = options.font_family.as_deref().unwrap_or(&body.font_family);
                let mut paragraph_spec = spec.clone();
                if !paragraph_spec.fill_overridden {
                    paragraph_spec.fill = Some(gaanim_core::peniko::Brush::Solid(body.fill_color));
                    paragraph_spec.fill_overridden = true;
                }
                let color = match paragraph_spec.fill {
                    Some(gaanim_core::peniko::Brush::Solid(c)) => c,
                    _ => body.fill_color,
                };
                let mut resolved_options = options.clone();
                if resolved_options.width.is_none() {
                    resolved_options.width = Some(frame_bounds.width().max(1.0));
                }
                let source = paragraph_typst_source(text, &resolved_options, font_size, color);
                let mr = builder.typst(
                    &source,
                    false,
                    Some(font_family),
                    None,
                    Some(font_size),
                    None,
                );
                Self::post_apply(builder, mr.id, &paragraph_spec, id_map, frame_bounds);
                Self::apply_fragment_fills(builder, mr, &paragraph_spec);
                mr
            }
            SpawnKind::Title(t) => {
                let role = gaanim_text::prelude::TextRole::Title;
                let styled_spec =
                    Self::with_default_text_fill(spec, text_config.roles[&role].fill_color);
                let color = match styled_spec.fill {
                    Some(gaanim_core::peniko::Brush::Solid(c)) => c,
                    _ => text_config.roles[&role].fill_color,
                };
                let has_inline_math = split_text_math(t)
                    .iter()
                    .any(|(is_math, c)| *is_math && !c.trim().is_empty());
                let mr = if has_inline_math {
                    let style = &text_config.roles[&role];
                    let source = text_inline_typst_source(t, color);
                    builder.typst(
                        &source,
                        false,
                        Some(&style.font_family),
                        None,
                        Some(style.size),
                        None,
                    )
                } else {
                    builder.spawn_text(t, role)
                };
                Self::post_apply(builder, mr.id, &styled_spec, id_map, frame_bounds);
                Self::apply_fragment_fills(builder, mr, &styled_spec);
                mr
            }
            SpawnKind::Subtitle(t) => {
                let role = gaanim_text::prelude::TextRole::Subtitle;
                let styled_spec =
                    Self::with_default_text_fill(spec, text_config.roles[&role].fill_color);
                let color = match styled_spec.fill {
                    Some(gaanim_core::peniko::Brush::Solid(c)) => c,
                    _ => text_config.roles[&role].fill_color,
                };
                let has_inline_math = split_text_math(t)
                    .iter()
                    .any(|(is_math, c)| *is_math && !c.trim().is_empty());
                let mr = if has_inline_math {
                    let style = &text_config.roles[&role];
                    let source = text_inline_typst_source(t, color);
                    builder.typst(
                        &source,
                        false,
                        Some(&style.font_family),
                        None,
                        Some(style.size),
                        None,
                    )
                } else {
                    builder.spawn_text(t, role)
                };
                Self::post_apply(builder, mr.id, &styled_spec, id_map, frame_bounds);
                Self::apply_fragment_fills(builder, mr, &styled_spec);
                mr
            }
            SpawnKind::Equation(f) => {
                let mr = builder.equation(f);
                let math = &text_config.roles[&gaanim_text::prelude::TextRole::Math];
                let styled_spec = Self::with_default_text_fill(spec, math.fill_color);
                Self::post_apply(builder, mr.id, &styled_spec, id_map, frame_bounds);
                Self::apply_fragment_fills(builder, mr, &styled_spec);
                mr
            }
            SpawnKind::Typst { source, page_width } => {
                let body = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                let foreground = typst_foreground_for_background(scene_background);
                let page_directive = if let Some(w) = page_width {
                    // ponytail: raw interpolation into Typst source — reject control chars to avoid injection
                    if w.trim().is_empty() || w.contains(['\n', '\r', ';', '"', '\'']) {
                        panic!("invalid Typst page width: {w:?}");
                    }
                    format!("#set page(width: {w}, height: auto, margin: 0pt)\n")
                } else {
                    "#set page(height: auto, margin: 0pt)\n".to_string()
                };
                let source =
                    format!("{page_directive}#set text(fill: rgb(\"{foreground}\"))\n{source}");
                let mr = builder.typst(&source, false, Some(&body.font_family), None, None, None);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                Self::apply_fragment_fills(builder, mr, spec);
                mr
            }
            SpawnKind::Image { image, view } => {
                let b = builder.image(image.clone(), *view);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::SvgPath(path) => {
                let b = builder.svg_path(path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Group(ids) => {
                let refs: Vec<MobjectRef> = ids
                    .iter()
                    .filter_map(|id| id_map.get(id).copied().map(|id| MobjectRef { id }))
                    .collect();
                let mr = builder.group(&refs);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::GroupNoCenter(ids) => {
                let refs: Vec<MobjectRef> = ids
                    .iter()
                    .filter_map(|id| id_map.get(id).copied().map(|id| MobjectRef { id }))
                    .collect();
                let mr = builder.group_identity(&refs);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::ValueTracker(initial) => {
                // Spawn a FloatSignal entity (no visual output).
                let new_id = builder.next_id();
                let entity = builder
                    .commands
                    .spawn((
                        gaanim_scene::MobjectId(new_id),
                        gaanim_animation::FloatSignal::new(*initial),
                    ))
                    .id();
                builder.tag_entity(entity);
                builder.states.insert(
                    new_id,
                    MobjectState {
                        path: std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
                        bounds: Bounds3D::default(),
                        transform: SpatialTransform::default(),
                        opacity: 1.0,
                        fill: None,
                        stroke: StrokeBrush::default(),
                        entity,
                        child_spans: Vec::new(),
                        children: Vec::new(),
                        parent: None,
                        exclude_from_parent_draw: false,
                    },
                );
                builder.float_signals.insert(new_id, *initial);
                MobjectRef { id: new_id }
            }
            SpawnKind::TracedPathLine => {
                // Spawn a minimal line (0,0)→(0,0). TracedPath will overwrite its Path2D.
                let b = builder.line(Point::new(0.0, 0.0), Point::new(0.0, 0.0));
                let mr = Self::finish_spawn_builder(b, spec);
                mr
            }
            SpawnKind::TrackingLine => {
                // Spawn a minimal line (0,0)→(0,0). TrackingLine will overwrite its Path2D.
                let b = builder.line(Point::new(0.0, 0.0), Point::new(0.0, 0.0));
                let mr = Self::finish_spawn_builder(b, spec);
                mr
            }
            SpawnKind::TracedPath3DLine => {
                // Empty 3D line placeholder; TracedPath3D will fill its LineListData.
                let mref = builder.spawn_line_strip(vec![], gaanim_core::peniko::Color::WHITE);
                Self::post_apply(builder, mref.id, spec, id_map, frame_bounds);
                mref
            }
        }
    }

    fn finish_spawn_builder<'b, 'w, 's, 'a>(
        mut b: crate::builder::MobjectSpawnBuilder<'b, 'w, 's, 'a>,
        spec: &ObjectSpec,
    ) -> MobjectRef {
        if spec.stroke_overridden {
            if let Some((ref brush, w)) = spec.stroke {
                b = b.stroke_brush(brush.clone(), w);
            } else {
                b = b.no_stroke();
            }
        }
        if spec.fill_overridden {
            if let Some(ref f) = spec.fill {
                b = b.fill_brush(f.clone());
            } else {
                b = b.no_fill();
            }
        }
        b = b.opacity(spec.opacity).z_index(spec.z_index);
        b.spawn_with_effects(spec.glow.clone(), spec.blur, spec.shadow.clone())
    }

    /// Applies deferred glyph-level color overrides after the normal object
    /// style has been propagated to the compiled text hierarchy.
    fn apply_fragment_fills(builder: &mut SceneBuilder, target: MobjectRef, spec: &ObjectSpec) {
        for (fragment, color) in &spec.fragment_fills {
            builder.select(target, fragment).set_fill(*color);
        }
    }

    fn with_default_text_fill(spec: &ObjectSpec, color: PenikoColor) -> ObjectSpec {
        let mut styled = spec.clone();
        if !styled.fill_overridden {
            styled.fill = Some(gaanim_core::peniko::Brush::Solid(color));
            styled.fill_overridden = true;
        }
        styled
    }

    fn post_apply(
        builder: &mut SceneBuilder,
        id: ObjectId,
        spec: &ObjectSpec,
        id_map: &HashMap<ObjectId, ObjectId>,
        frame_bounds: Bounds3D,
    ) {
        let mut child_spans = Vec::new();
        if let Some(st) = builder.states.get_mut(id) {
            child_spans = st.child_spans.clone();
            let is_textual_hierarchy = !child_spans.is_empty();
            if spec.stroke_overridden {
                if let Some((ref brush, w)) = spec.stroke {
                    let sb = StrokeBrush {
                        brush: Some(brush.clone()),
                        style: gaanim_core::kurbo::Stroke::new(w),
                    };
                    st.stroke = sb.clone();
                    if !is_textual_hierarchy {
                        builder.commands.entity(st.entity).insert(sb);
                    }
                } else {
                    st.stroke = StrokeBrush::transparent();
                    if !is_textual_hierarchy {
                        builder
                            .commands
                            .entity(st.entity)
                            .insert(StrokeBrush::transparent());
                    }
                }
            }
            if spec.fill_overridden {
                if let Some(ref f) = spec.fill {
                    st.fill = Some(f.clone());
                    if !is_textual_hierarchy {
                        builder
                            .commands
                            .entity(st.entity)
                            .insert(FillBrush(Some(f.clone())));
                    }
                } else {
                    st.fill = None;
                    if !is_textual_hierarchy {
                        builder
                            .commands
                            .entity(st.entity)
                            .insert(FillBrush::transparent());
                    }
                }
            }
            if spec.opacity != 1.0 {
                st.opacity = spec.opacity;
                builder
                    .commands
                    .entity(st.entity)
                    .insert(Opacity(spec.opacity));
            }
            if spec.z_index != 0 {
                builder.commands.entity(st.entity).insert(RenderOrder {
                    z_index: spec.z_index,
                    ..Default::default()
                });
            }
        }
        if spec.fill_overridden {
            for child in &child_spans {
                if let Some(child_state) = builder.states.get_mut(child.id) {
                    child_state.fill = spec.fill.clone();
                    if let Some(ref f) = spec.fill
                        && child_state.stroke.brush.is_some()
                    {
                        child_state.stroke.brush = Some(f.clone());
                        builder
                            .commands
                            .entity(child.entity)
                            .insert(child_state.stroke.clone());
                    }
                }
                builder
                    .commands
                    .entity(child.entity)
                    .insert(if let Some(ref f) = spec.fill {
                        FillBrush(Some(f.clone()))
                    } else {
                        FillBrush::transparent()
                    });
            }
        }
        if spec.opacity != 1.0 {
            for child in &child_spans {
                if let Some(child_state) = builder.states.get_mut(child.id) {
                    child_state.opacity = spec.opacity;
                }
                builder
                    .commands
                    .entity(child.entity)
                    .insert(Opacity(spec.opacity));
            }
        }
        if spec.stroke_overridden {
            for child in &child_spans {
                let sb = if let Some((ref brush, w)) = spec.stroke {
                    StrokeBrush {
                        brush: Some(brush.clone()),
                        style: gaanim_core::kurbo::Stroke::new(w),
                    }
                } else {
                    StrokeBrush::transparent()
                };
                if let Some(child_state) = builder.states.get_mut(child.id) {
                    child_state.stroke = sb.clone();
                }
                builder.commands.entity(child.entity).insert(sb);
            }
        }
        let effect_targets = if child_spans.is_empty() {
            builder
                .states
                .get(id)
                .map(|state| vec![state.entity])
                .unwrap_or_default()
        } else {
            child_spans.iter().map(|child| child.entity).collect()
        };
        for entity in effect_targets {
            let mut commands = builder.commands.entity(entity);
            if let Some(glow) = &spec.glow {
                commands.insert(glow.clone());
            }
            if let Some(blur) = spec.blur {
                commands.insert(blur);
            }
            if let Some(shadow) = &spec.shadow {
                commands.insert(shadow.clone());
            }
        }
        // Billboard / HUD chaining (.billboard() / .hud())
        if spec.billboard {
            if let Some(state) = builder.states.get(id) {
                builder
                    .commands
                    .entity(state.entity)
                    .insert(gaanim_scene::Billboard);
                builder
                    .commands
                    .entity(state.entity)
                    .insert(bevy::prelude::Transform::default());
            }
        }
        if spec.hud {
            // HUD is screen-space fixed but still rendered via Vello2D.
            // Keeping RenderLayer::Vello2D ensures it participates in the
            // main Vello pass which is now drawn AFTER the 3D meshes (order
            // 1 vs 0) and after background suppression for perspective, so
            // HUD appears on top of 3D. HudOverlay is retained as a marker
            // for potential future dedicated overlay pass.
            let targets = if child_spans.is_empty() {
                builder
                    .states
                    .get(id)
                    .map(|s| vec![s.entity])
                    .unwrap_or_default()
            } else {
                child_spans.iter().map(|c| c.entity).collect::<Vec<_>>()
            };
            for entity in targets {
                builder
                    .commands
                    .entity(entity)
                    .insert(gaanim_scene::HudOverlay);
                // Keep Vello2D so the element is rendered; ensure high z for HUD.
                builder
                    .commands
                    .entity(entity)
                    .insert(gaanim_scene::RenderOrder {
                        z_index: 1000,
                        ..Default::default()
                    });
            }
            if let Some(state) = builder.states.get(id) {
                builder
                    .commands
                    .entity(state.entity)
                    .insert(gaanim_scene::HudOverlay);
                builder
                    .commands
                    .entity(state.entity)
                    .insert(gaanim_scene::RenderOrder {
                        z_index: 1000,
                        ..Default::default()
                    });
            }
        }
        Self::apply_layout(builder, id, spec, id_map, frame_bounds);
    }

    fn apply_layout(
        builder: &mut SceneBuilder,
        id: ObjectId,
        spec: &ObjectSpec,
        id_map: &HashMap<ObjectId, ObjectId>,
        frame_bounds: Bounds3D,
    ) {
        if spec.layout_ops.is_empty() {
            return;
        }

        let Some(state) = builder.states.get(id) else {
            return;
        };
        let bounds = state.bounds;
        let original_transform = state.transform;
        let entity = state.entity;
        let mut transform = original_transform;
        let mut pivot_in_scene = None;

        for op in &spec.layout_ops {
            match op {
                LayoutOp::SetTranslation(translation) => {
                    transform.translation = *translation;
                }
                LayoutOp::SetScale(factor) => {
                    transform.scale = original_transform.scale * *factor;
                }
                LayoutOp::SetScale3D(scale) => {
                    transform.scale = original_transform.scale * *scale;
                }
                LayoutOp::SetRotation(radians) => {
                    transform.rotation = gaanim_core::glam::DQuat::from_rotation_z(*radians);
                }
                LayoutOp::SetRotation3D(euler) => {
                    transform.rotation = gaanim_core::glam::DQuat::from_euler(
                        gaanim_core::glam::EulerRot::XYZ,
                        euler.x,
                        euler.y,
                        euler.z,
                    );
                }
                LayoutOp::SetPivot(pivot) => {
                    pivot_in_scene = Some(*pivot);
                }
                LayoutOp::MoveAnchorTo { target, anchor } => {
                    transform =
                        gaanim_layout::compute_move_to(bounds, &transform, *target, *anchor);
                }
                LayoutOp::NextTo {
                    reference,
                    direction,
                    spacing,
                    aligned_edge,
                } => {
                    let Some(reference_id) = id_map.get(reference).copied() else {
                        bevy::prelude::warn!(
                            "Canvas layout skipped: reference object {:?} was not spawned before {:?}",
                            reference,
                            spec.id
                        );
                        continue;
                    };
                    let Some(reference_state) = builder.states.get(reference_id) else {
                        bevy::prelude::warn!(
                            "Canvas layout skipped: missing state for reference object {:?}",
                            reference_id
                        );
                        continue;
                    };
                    let reference_transform = builder.get_world_transform(reference_id);
                    let shift = gaanim_layout::compute_next_to_new(
                        bounds,
                        &transform,
                        reference_state.bounds,
                        &reference_transform,
                        *direction,
                        *spacing,
                        *aligned_edge,
                    );
                    transform = transform.shift_3d(shift);
                }
                LayoutOp::AlignTo {
                    reference,
                    target_anchor,
                    reference_anchor,
                } => {
                    let Some(reference_id) = id_map.get(reference).copied() else {
                        bevy::prelude::warn!(
                            "Canvas layout skipped: reference object {:?} was not spawned before {:?}",
                            reference,
                            spec.id
                        );
                        continue;
                    };
                    let Some(reference_state) = builder.states.get(reference_id) else {
                        bevy::prelude::warn!(
                            "Canvas layout skipped: missing state for reference object {:?}",
                            reference_id
                        );
                        continue;
                    };
                    let reference_transform = builder.get_world_transform(reference_id);
                    let shift = gaanim_layout::compute_align_to_new(
                        bounds,
                        &transform,
                        reference_state.bounds,
                        &reference_transform,
                        *target_anchor,
                        *reference_anchor,
                    );
                    transform = transform.shift_3d(shift);
                }
                LayoutOp::ToEdge { direction, buff } => {
                    transform = gaanim_layout::compute_to_edge(
                        bounds,
                        &transform,
                        *direction,
                        *buff,
                        frame_bounds,
                    );
                }
                LayoutOp::ToCorner { corner, buff } => {
                    transform = gaanim_layout::compute_to_corner(
                        bounds,
                        &transform,
                        *corner,
                        *buff,
                        frame_bounds,
                    );
                }
            }
        }

        if let Some(pivot) = pivot_in_scene {
            // SpatialTransform stores anchors in local coordinates, while the
            // public API accepts the stable scene-space point users see.
            transform.anchor = pivot - transform.translation;
        }

        if transform != original_transform {
            if let Some(state) = builder.states.get_mut(id) {
                state.transform = transform;
            }
            builder.commands.entity(entity).insert(transform);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::world::CommandQueue;
    use gaanim_core::peniko::Brush;
    use gaanim_layout::Anchor;
    use gaanim_scene::LocalBounds;
    use gaanim_timeline::snapshot::WorldSnapshot;

    #[test]
    fn camera_and_drawable_play_compile_at_the_same_start_time() {
        let mut canvas = Canvas::new(640, 360);
        let marker = canvas.circle(20.0);
        let marker_anim = marker.fade_in(2.0);
        let camera_anim = canvas.camera_orbit(0.4, 0.1, 2.0);
        canvas.play(vec![marker_anim, camera_anim]);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        let parallel_clips: Vec<_> = timeline
            .clips
            .values()
            .filter(|clip| (clip.duration - 2.0).abs() < 1e-9)
            .collect();
        assert_eq!(parallel_clips.len(), 3); // fade + orbit position + orbit rotation
        assert!(parallel_clips.iter().all(|clip| clip.start.abs() < 1e-9));
        assert!((timeline.cached_duration - 2.0).abs() < 1e-9);
    }

    #[test]
    fn object_declared_after_wait_stays_hidden_until_its_declaration_time() {
        let mut canvas = Canvas::new(640, 360);
        canvas.wait(1.0);
        canvas.circle(20.0);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));

        timeline.seek(&mut world, 0.5);
        assert!(
            world
                .query::<&Opacity>()
                .iter(&world)
                .all(|opacity| opacity.0 == 0.0),
            "late-declared object leaked before its declaration time"
        );

        timeline.seek(&mut world, 1.0);
        assert!(
            world
                .query::<&Opacity>()
                .iter(&world)
                .any(|opacity| opacity.0 == 1.0),
            "late-declared object did not become visible at its declaration time"
        );
    }

    #[test]
    fn visual_effects_are_attached_to_compiled_drawables() {
        let mut canvas = Canvas::new(640, 360);
        canvas
            .circle(60.0)
            .glow(PenikoColor::WHITE, 18.0, 1.2)
            .blur(5.0)
            .shadow(
                PenikoColor::BLACK,
                gaanim_core::glam::DVec2::new(8.0, -8.0),
                7.0,
            );

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        assert_eq!(
            world
                .query::<&gaanim_renderer::effects::Glow>()
                .iter(&world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query::<&gaanim_renderer::effects::GaussianBlur>()
                .iter(&world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query::<&gaanim_renderer::effects::DropShadow>()
                .iter(&world)
                .count(),
            1
        );
    }

    #[test]
    fn clip_mask_uses_another_drawables_world_geometry() {
        let mut canvas = Canvas::new(640, 360);
        let target = canvas.rect(300.0, 160.0).at(80.0, 0.0);
        let mask = canvas.circle(55.0).at(80.0, 0.0);
        target.clip(&mask, gaanim_core::peniko::Fill::NonZero);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let masks = world
            .query::<&gaanim_renderer::effects::ClipMask>()
            .iter(&world)
            .collect::<Vec<_>>();
        assert_eq!(masks.len(), 1);
        assert!(!masks[0].path.is_empty());
        let bounds = masks[0].path.bounding_box();
        assert!((bounds.center().x).abs() < 1e-6);
        assert!((bounds.width() - 110.0).abs() < 1e-3);
    }

    #[test]
    fn segments_show_only_the_active_segment_on_seek() {
        let red = PenikoColor::from_rgb8(255, 0, 0);
        let blue = PenikoColor::from_rgb8(0, 0, 255);
        let mut canvas = Canvas::new(640, 360);
        canvas.segment("first", None).unwrap();
        canvas.text("First segment").fill(red);
        canvas.wait(1.0);
        canvas.segment("second", None).unwrap();
        canvas.text("Second segment").fill(blue);
        canvas.wait(1.0);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));

        let visible_for = |world: &mut World, color| {
            world
                .query::<(&FillBrush, Option<&gaanim_scene::Visible>)>()
                .iter(world)
                .find_map(|(fill, visible)| {
                    matches!(&fill.0, Some(Brush::Solid(found)) if *found == color)
                        .then_some(visible.is_some())
                })
                .expect("colored segment object should exist")
        };

        timeline.seek(&mut world, 0.0);
        assert!(visible_for(&mut world, red));
        assert!(!visible_for(&mut world, blue));

        timeline.seek(&mut world, 1.0);
        assert!(!visible_for(&mut world, red));
        assert!(visible_for(&mut world, blue));
    }

    #[test]
    fn justified_paragraph_compiles_to_vector_glyphs() {
        let mut canvas = Canvas::new(640, 360);
        canvas.paragraph(
            "Este párrafo debe ocupar varias líneas y conservar glifos vectoriales.",
            ParagraphOptions {
                width: 180.0,
                align: TextAlign::Justify,
                line_spacing: 1.25,
                font_size: Some(28.0),
                font_family: None,
                max_lines: None,
                overflow: ParagraphOverflow::Clip,
            },
        );

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<&LocalBounds>();
        let visible_bounds = query
            .iter(&world)
            .filter(|bounds| bounds.0.width() > 0.0 && bounds.0.height() > 0.0)
            .count();
        assert!(visible_bounds > 5, "paragraph should produce vector glyphs");
    }

    #[test]
    fn paragraph_max_lines_emits_a_clipped_text_box() {
        let source = paragraph_typst_source(
            "A bounded paragraph",
            &ParagraphOptions {
                width: Some(240.0),
                align: TextAlign::Left,
                line_spacing: 1.2,
                font_size: Some(30.0),
                font_family: None,
                max_lines: Some(2),
                overflow: ParagraphOverflow::Clip,
            },
            30.0,
            gaanim_core::peniko::Color::WHITE,
        );

        assert!(source.contains("height: 72pt"));
        assert!(source.contains("clip: true"));
    }

    #[test]
    fn equation_fragment_fill_overrides_matching_vector_glyphs() {
        let highlight = gaanim_core::peniko::Color::from_rgb8(255, 180, 0);
        let mut canvas = Canvas::new(640, 360);
        canvas.equation("E = m c^2").color_by("m", highlight);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<&gaanim_scene::FillBrush>();
        assert!(query.iter(&world).any(|fill| {
            matches!(
                &fill.0,
                Some(gaanim_core::peniko::Brush::Solid(color)) if *color == highlight
            )
        }));
    }

    #[test]
    fn text_fragment_fill_overrides_matching_vector_glyphs() {
        let highlight = gaanim_core::peniko::Color::from_rgb8(64, 180, 255);
        let mut canvas = Canvas::new(640, 360);
        canvas
            .text("Energy depends on mass")
            .color_by("mass", highlight);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<&gaanim_scene::FillBrush>();
        assert!(query.iter(&world).any(|fill| {
            matches!(
                &fill.0,
                Some(gaanim_core::peniko::Brush::Solid(color)) if *color == highlight
            )
        }));
    }

    #[test]
    fn paper_theme_applies_role_fills_to_text_glyphs() {
        let mut canvas = Canvas::new(640, 360);
        canvas
            .set_theme("paper")
            .expect("paper is a built-in theme");
        canvas.title("Heading");
        canvas.subtitle("Subheading");
        canvas.text("Body copy");
        canvas.equation("x = y");

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = canvas.themed_text_config();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let fills: Vec<_> = world
            .query::<&gaanim_scene::FillBrush>()
            .iter(&world)
            .filter_map(|fill| match fill.0 {
                Some(gaanim_core::peniko::Brush::Solid(color)) => Some(color),
                _ => None,
            })
            .collect();

        assert!(fills.contains(&PenikoColor::BLACK));
        assert!(!fills.contains(&PenikoColor::WHITE));
    }

    #[test]
    fn fragment_transform_moves_selected_vector_glyphs() {
        let mut canvas = Canvas::new(640, 360);
        let source = canvas.equation("E = m c^2");
        let target = canvas.equation("p = m v");
        source.select("m").transform_to(&target.select("m"), 0.8);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Translation { .. },
                        ..
                    }
                )
            )
        }));
    }

    #[test]
    fn named_fragment_tag_resolves_to_a_vector_selection() {
        let highlight = gaanim_core::peniko::Color::from_rgb8(64, 180, 255);
        let mut canvas = Canvas::new(640, 360);
        let formula = canvas.equation("E = m c^2").define_tag("mass", "m", None);
        formula
            .tag("mass")
            .expect("registered tag should resolve")
            .fill(highlight);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<&gaanim_scene::FillBrush>();
        assert!(query.iter(&world).any(|fill| {
            matches!(
                &fill.0,
                Some(gaanim_core::peniko::Brush::Solid(color)) if *color == highlight
            )
        }));
    }

    #[test]
    fn cancel_term_places_a_diagonal_strike_over_the_selected_glyphs() {
        let strike_color = PenikoColor::WHITE;
        let mut canvas = Canvas::new(640, 360);
        let formula = canvas
            .equation("x + 3 = 7")
            .define_tag("constant", "3", None);
        formula
            .tag("constant")
            .expect("registered tag should resolve")
            .cancel(0.6);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<(&gaanim_scene::StrokeBrush, &LocalBounds)>();
        let bounds = query
            .iter(&world)
            .find_map(|(stroke, bounds)| {
                matches!(&stroke.brush, Some(gaanim_core::peniko::Brush::Solid(color)) if *color == strike_color)
                    .then_some(bounds.0)
            })
            .expect("cancel should spawn a coral strikethrough");
        assert!(bounds.center().x.abs() < 100.0);
        assert!(bounds.width() > 0.0 && bounds.height() > 0.0);
    }

    #[test]
    fn tagged_equation_transform_moves_shared_tags() {
        let mut canvas = Canvas::new(640, 360);
        let source = canvas
            .equation("E = m c^2")
            .define_tag("mass", "m", None)
            .at(0.0, 70.0);
        let target = canvas
            .equation("p = m v")
            .define_tag("mass", "m", None)
            .at(0.0, -90.0);
        canvas.transform_equation_tags(&source, &target, None, 0.8);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Translation { from, to },
                        ..
                    }
                ) if from.y > to.y
            )
        }));
    }

    #[test]
    fn equation_expansion_morphs_semantic_terms_without_cross_fading_pairs() {
        let mut canvas = Canvas::new(640, 360);
        let source = canvas.equation("E = m c^2").define_tag("mass", "m", None);
        let target = canvas
            .equation("E = (m_1 + m_2) c^2")
            .define_tag("mass", "(m_1 + m_2)", None);
        canvas.expand_equation_tag(&source, &target, "mass", 0.8);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        let semantic_morphs = timeline
            .clips
            .values()
            .filter(|clip| {
                matches!(
                    &clip.payload,
                    gaanim_timeline::clip::ClipPayload::Animation(
                        gaanim_timeline::clip::AnimationSpec {
                            lens: gaanim_timeline::clip::PropertyLensSpec::PathMorph { .. },
                            label: Some(label),
                            ..
                        }
                    ) if label == "EquationSemanticMorph"
                )
            })
            .count();
        assert!(semantic_morphs >= 3, "shared and tagged terms should morph");
        assert!(timeline.clips.values().all(|clip| {
            !matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { .. },
                        label: Some(label),
                        ..
                    }
                ) if clip.duration > 0.0 && label == "EquationSemanticMorph"
            )
        }));
        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Scale { from, to },
                        label: Some(label),
                        ..
                    }
                ) if label == "EquationEmerge"
                    && (clip.start - 0.16).abs() < 1e-9
                    && *from == gaanim_core::glam::DVec3::ZERO
                    && *to != gaanim_core::glam::DVec3::ZERO
            )
        }));
        assert!(timeline.clips.values().all(|clip| {
            !matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { .. },
                        label: Some(label),
                        ..
                    }
                ) if label == "EquationEmerge" && clip.duration > 0.0
            )
        }));
    }

    #[test]
    fn equation_step_prioritizes_semantic_tags_then_matches_common_glyphs() {
        let mut canvas = Canvas::new(640, 360);
        let source = canvas.equation("x + 3 = 7").define_tag("result", "7", None);
        let target = canvas.equation("x = 4").define_tag("result", "4", None);
        canvas.step_equation_with_matches(
            &source,
            &target,
            Some(vec![("result".to_string(), "result".to_string())]),
            0.8,
        );

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        let semantic_morphs = timeline
            .clips
            .values()
            .filter(|clip| {
                matches!(
                    &clip.payload,
                    gaanim_timeline::clip::ClipPayload::Animation(
                        gaanim_timeline::clip::AnimationSpec {
                            lens: gaanim_timeline::clip::PropertyLensSpec::PathMorph { .. },
                            label: Some(label),
                            ..
                        }
                    ) if label == "EquationSemanticMorph"
                )
            })
            .count();
        assert!(
            semantic_morphs >= 3,
            "x and = should auto-match while the result tag forces 7 -> 4"
        );
        let handoffs = timeline
            .clips
            .values()
            .filter(|clip| {
                matches!(
                    &clip.payload,
                    gaanim_timeline::clip::ClipPayload::Animation(
                        gaanim_timeline::clip::AnimationSpec {
                            lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { .. },
                            label: Some(label),
                            ..
                        }
                    ) if label == "EquationHandoff" && clip.duration == 0.0
                )
            })
            .count();
        assert!(handoffs >= semantic_morphs * 2);
        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Scale { to, .. },
                        label: Some(label),
                        ..
                    }
                ) if label == "EquationCollapse"
                    && *to == gaanim_core::glam::DVec3::ZERO
            )
        }));
        assert!(timeline.clips.values().all(|clip| {
            !matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { .. },
                        label: Some(label),
                        ..
                    }
                ) if label == "EquationCollapse" && clip.duration > 0.0
            )
        }));
    }

    #[test]
    fn equation_step_handoff_is_exact_and_target_remains_animatable() {
        let mut canvas = Canvas::new(640, 360);
        let source = canvas.equation("x + 3 = 7").define_tag("result", "7", None);
        let target = canvas.equation("x = 4").define_tag("result", "4", None);
        canvas.step_equation(&source, &target, 0.8);
        canvas.play(vec![target.indicate(0.4)]);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, 0.8);

        let child_opacities: Vec<Vec<f32>> = world
            .query::<(
                Option<&bevy::prelude::ChildOf>,
                Option<&bevy::prelude::Children>,
            )>()
            .iter(&world)
            .filter_map(|(parent, children)| {
                (parent.is_none()).then_some(children?).map(|children| {
                    children
                        .iter()
                        .filter_map(|child| world.get::<Opacity>(child).map(|opacity| opacity.0))
                        .collect()
                })
            })
            .collect();
        assert_eq!(child_opacities.len(), 2);
        assert!(
            child_opacities
                .iter()
                .any(|values| values.iter().all(|value| *value == 0.0))
        );
        assert!(
            child_opacities
                .iter()
                .any(|values| values.iter().all(|value| *value > 0.99))
        );

        timeline.seek(&mut world, 1.0);
        assert!(
            world
                .query::<&Opacity>()
                .iter(&world)
                .filter(|opacity| opacity.0 > 0.99)
                .count()
                >= 3
        );
    }

    #[test]
    fn tagged_equation_copy_keeps_opacity_changes_instantaneous() {
        let mut canvas = Canvas::new(640, 360);
        let source = canvas
            .equation("E = m c^2")
            .define_tag("mass", "m", None)
            .at(0.0, 70.0);
        let target = canvas
            .equation("p = m v")
            .define_tag("mass", "m", None)
            .at(0.0, -90.0);
        canvas.copy_equation_terms(&source, &target, Some(vec!["mass".to_string()]), 0.8);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::PathMorph { .. },
                        label: Some(label),
                        ..
                    }
                ) if label == "EquationSemanticMorph"
            )
        }));
        assert!(timeline.clips.values().all(|clip| {
            !matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { .. },
                        ..
                    }
                ) if clip.duration > 0.0
            )
        }));
    }

    #[test]
    fn tagged_equation_copy_preserves_both_visible_equations_after_seek() {
        let mut canvas = Canvas::new(640, 360);
        let title = canvas.text("One variable");
        let source = canvas
            .equation("E = m c^2")
            .define_tag("mass", "m", None)
            .at(0.0, 70.0);
        let target = canvas
            .equation("p = m v")
            .define_tag("mass", "m", None)
            .at(0.0, -90.0);
        canvas.play(vec![
            title.write(1.0),
            source.write(1.0),
            target.fade_in(1.0),
        ]);
        canvas.wait(0.5);
        canvas.copy_equation_terms(&source, &target, Some(vec!["mass".to_string()]), 0.9);
        canvas.wait(0.25);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, 2.55);

        let textual_roots: Vec<_> = world
            .query::<(
                bevy::prelude::Entity,
                Option<&bevy::prelude::ChildOf>,
                Option<&bevy::prelude::Children>,
            )>()
            .iter(&world)
            .filter_map(|(entity, parent, children)| {
                (parent.is_none() && children.is_some()).then_some(entity)
            })
            .collect();
        assert_eq!(textual_roots.len(), 3);
        for root in textual_roots {
            let children = world.get::<bevy::prelude::Children>(root).unwrap();
            assert!(children.iter().all(|child| {
                world
                    .get::<Opacity>(child)
                    .is_some_and(|opacity| opacity.0 > 0.99)
            }));
        }
    }

    #[test]
    fn fade_in_from_down_schedules_opacity_and_upward_translation() {
        let mut canvas = Canvas::new(640, 360);
        let label = canvas.text("Aparece desde abajo");
        label.fade_in_from(crate::canvas::Direction::Down, 72.0, 0.8);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { from, to },
                        ..
                    }
                ) if *from == 0.0 && *to == 1.0
            )
        }));
        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Translation { from, to },
                        ..
                    }
                ) if from.y < to.y
            )
        }));
    }

    #[test]
    fn text_with_inline_math_compiles_to_vector_glyphs() {
        let mut canvas = Canvas::new(640, 360);
        canvas.text("Energia $E = m c^2$ es famosa");
        canvas.text("Valor $1/2$ y $sqrt(2)$");
        canvas.text("Angulo $alpha + beta$");
        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);
        let mut world = world;
        queue.apply(&mut world);
        let fills: Vec<_> = world
            .query::<&gaanim_scene::FillBrush>()
            .iter(&world)
            .filter_map(|f| f.0.clone())
            .collect();
        assert!(
            fills.len() > 10,
            "text with inline math should produce vector glyphs, got {}",
            fills.len()
        );
        assert!(
            fills.iter().all(|b| match b {
                gaanim_core::peniko::Brush::Solid(c) => *c != gaanim_core::peniko::Color::BLACK,
                _ => true,
            }),
            "inline math glyphs should not be black on black background"
        );
        assert!(
            world
                .query::<&LocalBounds>()
                .iter(&world)
                .any(|b| b.0.width() > 50.0)
        );
    }

    #[test]
    fn paragraph_with_inline_math_compiles_to_vector_glyphs() {
        let mut canvas = Canvas::new(640, 360);
        canvas.paragraph(
            "La energia $E = m c^2$ relaciona masa y energia en $x^2 + y^2 = z^2$.",
            ParagraphOptions {
                width: 500.0,
                align: TextAlign::Left,
                line_spacing: 1.2,
                font_size: Some(28.0),
                ..Default::default()
            },
        );
        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);
        let mut world = world;
        queue.apply(&mut world);
        let fills: Vec<_> = world
            .query::<&gaanim_scene::FillBrush>()
            .iter(&world)
            .filter_map(|f| f.0.clone())
            .collect();
        assert!(
            fills.len() > 15,
            "paragraph with inline math should produce vector glyphs, got {}",
            fills.len()
        );
        assert!(
            fills.iter().all(|b| match b {
                gaanim_core::peniko::Brush::Solid(c) => *c != gaanim_core::peniko::Color::BLACK,
                _ => true,
            }),
            "inline math paragraph glyphs should not be black on black background"
        );
    }

    #[test]
    fn inline_math_helpers_handle_escapes_and_doubles() {
        assert_eq!(
            typst_inline_content("a $x$ b"),
            "#text(\"a \")$x$#text(\" b\")"
        );
        assert_eq!(typst_inline_content("sin math"), "#text(\"sin math\")");
        assert_eq!(typst_inline_content("$E=mc^2$"), "$E=mc^2$");
        assert_eq!(
            typst_inline_content("Escapado \\$ literal $x$"),
            "#text(\"Escapado $ literal \")$x$"
        );
        assert_eq!(split_text_math("a $x$ b $y$ c").len(), 5);
        let para = paragraph_typst_source(
            "Hola $x^2$ mundo",
            &ParagraphOptions {
                width: Some(400.0),
                align: TextAlign::Left,
                ..Default::default()
            },
            32.0,
            gaanim_core::peniko::Color::WHITE,
        );
        assert!(
            para.contains("$x^2$"),
            "paragraph source should embed math, got {para}"
        );
        assert!(para.contains("#text(\"Hola \")"));
        let txt = text_inline_typst_source("prueba $x^2$ fin", gaanim_core::peniko::Color::WHITE);
        assert!(txt.contains("$x^2$"));
        let txt2 =
            text_inline_typst_source("$alpha + beta = 1$", gaanim_core::peniko::Color::WHITE);
        assert!(txt2.contains("$alpha + beta = 1$"));
    }

    #[test]
    fn layout_reflow_animates_displaced_members_and_fades_the_insertion() {
        let mut canvas = Canvas::new(640, 360);
        let first = canvas.rect(80.0, 30.0);
        let second = canvas.rect(80.0, 30.0);
        let container = canvas.group(&[&first]);
        canvas.reflow_layout(
            &container,
            &[&first],
            LayoutKind::Column,
            20.0,
            None,
            None,
            None,
            None,
            None,
            false,
            false,
            "center",
        );
        canvas.set_group_members(&container, &[&first, &second]);
        canvas.reflow_layout(
            &container,
            &[&first, &second],
            LayoutKind::Column,
            20.0,
            Some(0.5),
            Some(&second),
            None,
            None,
            None,
            false,
            false,
            "center",
        );

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        assert!(timeline.clips.values().any(|clip| matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(gaanim_timeline::clip::AnimationSpec {
                lens: gaanim_timeline::clip::PropertyLensSpec::Translation { .. },
                ..
            })
        )));
        assert!(timeline.clips.values().any(|clip| matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { from, to },
                    ..
                }
            ) if *from == 0.0 && *to == 1.0
        )));
    }
}
