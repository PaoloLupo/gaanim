//! Native 3D content drawn with Vello.
//!
//! Triangle meshes ([`TriangleMeshData`]) and line lists ([`LineListData`])
//! are projected through the scene camera onto the Vello canvas, sorted back
//! to front by their farthest point (painter's algorithm) and shaded on the
//! CPU. A mesh without a [`Material3D`] is unlit and double-sided; a mesh with
//! one is lit by the scene's [`Lighting3D`] in Gaanim's illustrative style
//! (see [`shade`]), hides its back faces, and sorts as one solid. Lines are
//! unlit.
//!
//! Colors are computed per vertex. A linear gradient reproduces the
//! interpolation of a triangle's vertex colors exactly when they vary along
//! one axis of color space, as lighting of one material does, so each
//! triangle fills with one. The result is a list of triangles and segments in
//! canvas coordinates, which the pipeline turns into ordinary drawables
//! beneath the 2D content.

use gaanim_core::glam::{DMat4, DVec3, DVec4};
use gaanim_core::{kurbo, peniko};
use gaanim_math::{Camera, GlobalSpatialTransform, Projection};
use gaanim_scene::{GlobalOpacity, Lighting3D, LineListData, Material3D, TriangleMeshData};

/// Width of 3D lines, in output pixels.
const LINE_WIDTH_PX: f64 = 1.5;
/// Width of the stroke that closes antialiasing seams between the opaque
/// triangles of a mesh, in output pixels.
const SEAM_WIDTH_PX: f64 = 1.5;

/// Maps world points onto the Vello canvas through a camera.
pub(crate) struct Projector {
    view: DMat4,
    view_proj: DMat4,
    /// Canvas pixels of the NDC square, then the inverse of the transform the
    /// 2D camera applies to the canvas.
    ndc_to_canvas: kurbo::Affine,
    near: f64,
    perspective: bool,
    /// Canvas units per output pixel.
    unit_per_px: f64,
    eye: DVec3,
    forward: DVec3,
}

impl Projector {
    pub(crate) fn new(camera: &Camera) -> Option<Self> {
        let width = f64::from(camera.viewport_width);
        let height = f64::from(camera.viewport_height);
        if width <= 0.0 || height <= 0.0 || camera.frame_width <= 0.0 {
            return None;
        }
        let (perspective, near) = match camera.projection {
            Projection::Perspective { near, .. } => (true, near.max(1e-4)),
            Projection::Orthographic { .. } => (false, f64::NEG_INFINITY),
        };
        // Under perspective the 2D camera stays at the origin, unrotated and
        // unzoomed (see `sync_gaanim_camera_to_bevy_system`); orthographic
        // cameras move the canvas themselves.
        let canvas = if perspective {
            let ppu = camera.pixels_per_unit();
            kurbo::Affine::translate((width * 0.5, height * 0.5))
                * kurbo::Affine::scale_non_uniform(ppu, -ppu)
        } else {
            camera.to_vello_transform()
        };
        let pixels_per_unit = canvas.determinant().abs().sqrt();
        if !pixels_per_unit.is_finite() || pixels_per_unit <= 0.0 {
            return None;
        }
        let ndc_to_pixels = kurbo::Affine::new([
            width * 0.5,
            0.0,
            0.0,
            -height * 0.5,
            width * 0.5,
            height * 0.5,
        ]);
        let view = camera.view_matrix();
        Some(Self {
            view,
            view_proj: camera.projection_matrix() * view,
            ndc_to_canvas: canvas.inverse() * ndc_to_pixels,
            near,
            perspective,
            unit_per_px: 1.0 / pixels_per_unit,
            eye: camera.position,
            forward: camera.rotation * DVec3::NEG_Z,
        })
    }

    /// Distance in front of the camera along its view axis.
    fn depth(&self, world: DVec3) -> f64 {
        -self.view.transform_point3(world).z
    }

    /// The canvas point of a world point in front of the near plane.
    fn project(&self, world: DVec3) -> Option<kurbo::Point> {
        let clip = self.view_proj * DVec4::new(world.x, world.y, world.z, 1.0);
        if self.perspective && clip.w <= 1e-9 {
            return None;
        }
        let ndc = kurbo::Point::new(clip.x / clip.w, clip.y / clip.w);
        let point = self.ndc_to_canvas * ndc;
        (point.x.is_finite() && point.y.is_finite()).then_some(point)
    }

    /// Direction from `point` towards the viewer.
    fn toward_viewer(&self, point: DVec3) -> DVec3 {
        if self.perspective {
            (self.eye - point).normalize_or_zero()
        } else {
            -self.forward
        }
    }

    /// The part of the segment `a`–`b` in front of the near plane.
    fn clip_segment(&self, a: DVec3, b: DVec3) -> Option<(DVec3, DVec3)> {
        if !self.perspective {
            return Some((a, b));
        }
        let (da, db) = (self.depth(a), self.depth(b));
        match (da >= self.near, db >= self.near) {
            (true, true) => Some((a, b)),
            (false, false) => None,
            (a_in, _) => {
                let t = (self.near - da) / (db - da);
                let cut = a.lerp(b, t);
                Some(if a_in { (a, cut) } else { (cut, b) })
            }
        }
    }
}

/// The paint of a primitive.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Paint {
    Solid(peniko::Color),
    /// Varies linearly from `from` at `start` to `to` at `end`, and is
    /// constant across that direction.
    Linear {
        start: kurbo::Point,
        end: kurbo::Point,
        from: peniko::Color,
        to: peniko::Color,
    },
}

impl Paint {
    /// The paint of points with linear vertex colors, as a linear gradient
    /// along the axis between the two most different colors.
    ///
    /// Vello interpolates gradients in sRGB, so the axis and each vertex's
    /// place on it are measured in sRGB: the gradient then reproduces the
    /// vertex colors exactly when they lie on one line, and neighbouring
    /// triangles agree along their shared edge. Its stops span every vertex,
    /// so none is clamped to an end color.
    fn interpolating(points: &[kurbo::Point], linear: &[[f32; 4]]) -> Self {
        let colors: Vec<[f32; 4]> = linear
            .iter()
            .map(|color| from_linear(*color).components)
            .collect();
        let (mut a, mut b, mut spread) = (0, 0, 0.0f32);
        for i in 0..colors.len() {
            for j in i + 1..colors.len() {
                let distance = color_distance(colors[i], colors[j]);
                if distance > spread {
                    (a, b, spread) = (i, j, distance);
                }
            }
        }
        let average = || peniko::Color::new(average_color(&colors));
        if spread < 1e-6 {
            return Self::Solid(average());
        }
        // Where each vertex lies between colors a (0) and b (1).
        let axis = [0, 1, 2, 3].map(|c| colors[b][c] - colors[a][c]);
        let t: Vec<f64> = colors
            .iter()
            .map(|color| {
                f64::from(
                    (0..4)
                        .map(|c| (color[c] - colors[a][c]) * axis[c])
                        .sum::<f32>()
                        / spread,
                )
            })
            .collect();
        // Gradient of the affine t(p) through the vertices.
        let gradient = match points {
            [p0, p1] => {
                let direction = *p1 - *p0;
                let length_squared = direction.hypot2();
                (length_squared > 1e-18).then(|| direction * ((t[1] - t[0]) / length_squared))
            }
            [p0, p1, p2] => {
                // g·(p1 - p0) = t1 - t0 and g·(p2 - p0) = t2 - t0.
                let (e1, e2) = (*p1 - *p0, *p2 - *p0);
                let det = e1.cross(e2);
                (det.abs() > 1e-18).then(|| {
                    kurbo::Vec2::new(
                        ((t[1] - t[0]) * e2.y - (t[2] - t[0]) * e1.y) / det,
                        ((t[2] - t[0]) * e1.x - (t[1] - t[0]) * e2.x) / det,
                    )
                })
            }
            _ => None,
        };
        let Some(gradient) = gradient.filter(|g| g.hypot2() > 1e-18) else {
            return Self::Solid(average());
        };
        let (low, high) = t.iter().fold((0.0f64, 1.0f64), |(low, high), &t| {
            (low.min(t), high.max(t))
        });
        let at = |position: f64| points[a] + gradient * (position / gradient.hypot2());
        let color = |position: f64| {
            let position = position as f32;
            peniko::Color::new(
                [0, 1, 2, 3].map(|c| (colors[a][c] + axis[c] * position).clamp(0.0, 1.0)),
            )
        };
        Self::Linear {
            start: at(low),
            end: at(high),
            from: color(low),
            to: color(high),
        }
    }

    /// Whether the paint covers nothing.
    fn is_transparent(&self) -> bool {
        match self {
            Self::Solid(color) => color.components[3] <= f32::EPSILON,
            Self::Linear { from, to, .. } => {
                from.components[3] <= f32::EPSILON && to.components[3] <= f32::EPSILON
            }
        }
    }

    /// Whether the paint is opaque everywhere.
    fn is_opaque(&self) -> bool {
        match self {
            Self::Solid(color) => color.components[3] >= 0.999,
            Self::Linear { from, to, .. } => {
                from.components[3] >= 0.999 && to.components[3] >= 0.999
            }
        }
    }

    pub(crate) fn brush(&self) -> peniko::Brush {
        match self {
            Self::Solid(color) => peniko::Brush::Solid(*color),
            Self::Linear {
                start,
                end,
                from,
                to,
            } => peniko::Brush::Gradient(
                peniko::Gradient::new_linear(*start, *end).with_stops([*from, *to]),
            ),
        }
    }
}

/// A projected primitive of one drawable.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Primitive {
    /// A filled triangle; `seam` strokes its edges to hide antialiasing gaps
    /// between opaque neighbours.
    Triangle {
        points: [kurbo::Point; 3],
        seam: bool,
    },
    Segment {
        points: [kurbo::Point; 2],
    },
    /// The visible faces of an opaque lit solid, drawn together: its
    /// silhouette in the item's (average) paint, then each face in its own
    /// paint. The silhouette beneath keeps what lies behind the solid from
    /// showing through the antialiased edges between faces.
    Solid {
        faces: Vec<([kurbo::Point; 3], Paint)>,
    },
}

/// A primitive with its paint and depth, ready to be sorted.
#[derive(Debug, Clone)]
pub(crate) struct Item<K> {
    pub key: K,
    /// Depth of the farthest point.
    pub depth: f64,
    pub paint: Paint,
    /// Opacity of the drawable, applied to all its primitives together, as
    /// one layer: overlapping primitives of a fading drawable do not show
    /// through one another.
    pub opacity: f32,
    pub primitive: Primitive,
}

/// One drawable's 3D content, as the ECS holds it.
pub(crate) struct Content<'a> {
    pub transform: &'a GlobalSpatialTransform,
    pub opacity: &'a GlobalOpacity,
    pub mesh: Option<&'a TriangleMeshData>,
    pub lines: Option<&'a LineListData>,
    pub material: Option<&'a Material3D>,
}

impl Projector {
    /// Project one drawable's content into `items`.
    pub(crate) fn push<K: Copy>(
        &self,
        key: K,
        content: &Content<'_>,
        lighting: &Lighting3D,
        items: &mut Vec<Item<K>>,
    ) {
        let opacity = content.opacity.0.clamp(0.0, 1.0);
        if opacity <= f32::EPSILON {
            return;
        }
        let model = content.transform.mat4;
        if let Some(mesh) = content.mesh {
            self.push_mesh(key, mesh, content.material, model, opacity, lighting, items);
        }
        if let Some(lines) = content.lines {
            self.push_lines(key, lines, model, opacity, items);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push_mesh<K: Copy>(
        &self,
        key: K,
        mesh: &TriangleMeshData,
        material: Option<&Material3D>,
        model: DMat4,
        opacity: f32,
        lighting: &Lighting3D,
        items: &mut Vec<Item<K>>,
    ) {
        let count = mesh.vertices.len();
        if count < 3 || mesh.indices.len() < 3 {
            return;
        }
        let world: Vec<DVec3> = mesh
            .vertices
            .iter()
            .map(|v| model.transform_point3(DVec3::new(v[0].into(), v[1].into(), v[2].into())))
            .collect();
        let colors = mesh.colors.as_ref().filter(|colors| colors.len() == count);
        let colors_of_mesh = colors;
        // Vertex colors carry the surface color; otherwise the material's or
        // the mesh's own color (white by default).
        let base = material
            .map(|material| material.color)
            .or(mesh.color)
            .unwrap_or(peniko::Color::WHITE);
        let lit = material.copied();
        let normals = lit.map(|_| vertex_normals(mesh, &world, model));
        let mut shaded: Vec<Option<[f32; 4]>> = vec![None; count];
        let first = items.len();
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let corners_index = triangle.map(|index| index as usize);
            if corners_index.iter().any(|&index| index >= count) {
                continue;
            }
            let corners = corners_index.map(|index| world[index]);
            let centroid = (corners[0] + corners[1] + corners[2]) / 3.0;
            let face = (corners[1] - corners[0]).cross(corners[2] - corners[0]);
            // Lit meshes hide back faces, as closed solids do.
            if lit.is_some() && face.dot(self.toward_viewer(centroid)) <= 0.0 {
                continue;
            }
            let Some(points) = corners
                .iter()
                .map(|corner| {
                    (!self.perspective || self.depth(*corner) >= self.near)
                        .then(|| self.project(*corner))
                        .flatten()
                })
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let colors = corners_index.map(|index| {
                *shaded[index].get_or_insert_with(|| {
                    let mut linear = match colors {
                        Some(colors) => colors[index],
                        None => to_linear(base),
                    };
                    if let (Some(material), Some(normals)) = (lit.as_ref(), normals.as_ref()) {
                        let normal = if normals[index] == DVec3::ZERO {
                            face.normalize_or_zero()
                        } else {
                            normals[index]
                        };
                        linear = shade(
                            linear,
                            normal,
                            self.toward_viewer(world[index]),
                            material,
                            lighting,
                        );
                    }
                    linear
                })
            });
            // A flat lit face has one color, lit as seen along the view axis,
            // so the triangles of one face share it and draw as one shape.
            let flat = match (lit.as_ref(), normals.as_ref()) {
                (Some(material), Some(normals)) if colors_of_mesh.is_none() => {
                    let [a, b, c] = corners_index.map(|index| normals[index]);
                    (a != DVec3::ZERO && a.abs_diff_eq(b, 1e-9) && a.abs_diff_eq(c, 1e-9))
                        .then(|| shade(to_linear(base), a, -self.forward, material, lighting))
                }
                _ => None,
            };
            let paint = match flat {
                Some(linear) => Paint::Solid(from_linear(linear)),
                None => Paint::interpolating(&points, &colors),
            };
            if paint.is_transparent() {
                continue;
            }
            let seam = paint.is_opaque();
            items.push(Item {
                key,
                depth: corners
                    .iter()
                    .map(|corner| self.depth(*corner))
                    .fold(f64::NEG_INFINITY, f64::max),
                paint,
                opacity,
                primitive: Primitive::Triangle {
                    points: [points[0], points[1], points[2]],
                    seam,
                },
            });
        }
        // An opaque lit mesh is a solid: back faces hidden, its faces never
        // overlap one another, so it sorts as one unit.
        let faces = &items[first..];
        if lit.is_some() && faces.len() > 1 && faces.iter().all(|item| item.paint.is_opaque()) {
            let depth = faces
                .iter()
                .map(|item| item.depth)
                .fold(f64::NEG_INFINITY, f64::max);
            let mut sum = [0.0f32; 4];
            let faces: Vec<([kurbo::Point; 3], Paint)> = items
                .drain(first..)
                .filter_map(|item| match item.primitive {
                    Primitive::Triangle { points, .. } => {
                        let color = match &item.paint {
                            Paint::Solid(color) => color.components,
                            Paint::Linear { from, to, .. } => {
                                [0, 1, 2, 3].map(|c| (from.components[c] + to.components[c]) * 0.5)
                            }
                        };
                        for c in 0..4 {
                            sum[c] += color[c];
                        }
                        Some((points, item.paint))
                    }
                    _ => None,
                })
                .collect();
            let underlay = peniko::Color::new(sum.map(|value| value / faces.len() as f32));
            items.push(Item {
                key,
                depth,
                paint: Paint::Solid(underlay),
                opacity,
                primitive: Primitive::Solid { faces },
            });
        }
    }

    fn push_lines<K: Copy>(
        &self,
        key: K,
        lines: &LineListData,
        model: DMat4,
        opacity: f32,
        items: &mut Vec<Item<K>>,
    ) {
        let count = lines.points.len();
        let colors = lines.colors.as_ref().filter(|colors| colors.len() == count);
        let base = to_linear(lines.color);
        let point = |index: usize| {
            let p = lines.points[index];
            model.transform_point3(DVec3::new(p[0].into(), p[1].into(), p[2].into()))
        };
        let pairs: Vec<(usize, usize)> = if lines.strip {
            (1..count).map(|index| (index - 1, index)).collect()
        } else if let Some(indices) = &lines.indices {
            indices
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[a, b]| (a as usize, b as usize))
                .filter(|&(a, b)| a < count && b < count)
                .collect()
        } else {
            (0..count / 2)
                .map(|pair| (pair * 2, pair * 2 + 1))
                .collect()
        };
        for (a, b) in pairs {
            let Some((start, end)) = self.clip_segment(point(a), point(b)) else {
                continue;
            };
            let (Some(p0), Some(p1)) = (self.project(start), self.project(end)) else {
                continue;
            };
            let ends = match colors {
                Some(colors) => [colors[a], colors[b]],
                None => [base, base],
            };
            let paint = Paint::interpolating(&[p0, p1], &ends);
            if paint.is_transparent() {
                continue;
            }
            items.push(Item {
                key,
                depth: self.depth(start).max(self.depth(end)),
                paint,
                opacity,
                primitive: Primitive::Segment { points: [p0, p1] },
            });
        }
    }

    /// Width of 3D lines in canvas units.
    pub(crate) fn line_width(&self) -> f64 {
        LINE_WIDTH_PX * self.unit_per_px
    }

    /// Width of the seam stroke of opaque triangles in canvas units.
    pub(crate) fn seam_width(&self) -> f64 {
        SEAM_WIDTH_PX * self.unit_per_px
    }
}

/// Sort items back to front; items at equal depth keep their order.
pub(crate) fn sort_back_to_front<K>(items: &mut [Item<K>]) {
    items.sort_by(|a, b| b.depth.total_cmp(&a.depth));
}

/// World-space shading normal of every vertex: the mesh's own normals, or
/// area-weighted face normals averaged over shared vertices.
fn vertex_normals(mesh: &TriangleMeshData, world: &[DVec3], model: DMat4) -> Vec<DVec3> {
    if let Some(normals) = mesh.normals.as_ref().filter(|n| n.len() == world.len()) {
        let normal_matrix = model.inverse().transpose();
        return normals
            .iter()
            .map(|n| {
                normal_matrix
                    .transform_vector3(DVec3::new(n[0].into(), n[1].into(), n[2].into()))
                    .normalize_or_zero()
            })
            .collect();
    }
    let mut normals = vec![DVec3::ZERO; world.len()];
    for triangle in mesh.indices.as_chunks::<3>().0 {
        let [a, b, c] = triangle.map(|index| index as usize);
        if a >= world.len() || b >= world.len() || c >= world.len() {
            continue;
        }
        let face = (world[b] - world[a]).cross(world[c] - world[a]);
        for index in [a, b, c] {
            normals[index] += face;
        }
    }
    normals.iter().map(|n| n.normalize_or_zero()).collect()
}

/// Direction towards the key light: above, to the left and in front, as
/// illustrations are lit.
const KEY_LIGHT: [f64; 3] = [-3.0, 6.0, 4.0];
/// How far the key light wraps past the terminator, softening it.
const KEY_WRAP: f32 = 0.35;
/// Light a surface receives when fully lit, beside the sky; a face in full
/// light shows about its own color.
const KEY_STRENGTH: f32 = 0.62;
/// Ambient light from the sky (above) and the ground (below).
const SKY: f32 = 0.5;
const GROUND: f32 = 0.3;
/// Tint of the sky, used for rim light and metal reflections.
const SKY_TINT: [f32; 3] = [0.86, 0.92, 1.0];
/// Strength of the rim light along silhouettes.
const RIM: f32 = 0.22;

/// Shade a lit surface point in Gaanim's illustrative style.
///
/// The material's color stays readable: a face in full light shows about its
/// own color and a face in shadow about half of it. Sky and ground light
/// brighten faces that look up and darken those that look down, a soft key
/// light from the upper left gives form, a faint rim light separates
/// silhouettes from the background, and polished materials catch a clean
/// highlight. Metals reflect the sky above instead of scattering light.
fn shade(
    base: [f32; 4],
    normal: DVec3,
    toward_viewer: DVec3,
    material: &Material3D,
    lighting: &Lighting3D,
) -> [f32; 4] {
    let metallic = material.metallic.clamp(0.0, 1.0);
    let polish = (1.0 - material.roughness.clamp(0.0, 1.0)).powi(2);
    let intensity = if lighting.enabled {
        lighting.intensity.max(0.0)
    } else {
        0.0
    };
    let light = DVec3::from_array(KEY_LIGHT).normalize();
    let facing_up = (0.5 + 0.5 * normal.y) as f32;
    let sky = GROUND + (SKY - GROUND) * facing_up;
    let key = (((normal.dot(light) as f32) + KEY_WRAP) / (1.0 + KEY_WRAP)).max(0.0);
    let diffuse = (sky + KEY_STRENGTH * key) * intensity;
    // A metal shows the sky it reflects, bright above and dark below.
    let reflection = (0.25 + 0.95 * facing_up * facing_up) * intensity;
    let facing = normal.dot(toward_viewer).clamp(0.0, 1.0) as f32;
    let rim = RIM * (1.0 - facing).powi(3) * intensity;
    let half = (light + toward_viewer).normalize_or_zero();
    let shininess = 8.0 + 248.0 * polish;
    let highlight =
        (normal.dot(half).max(0.0) as f32).powf(shininess) * polish * key.min(1.0) * intensity;
    let emissive = to_linear(material.emissive);
    let mut rgb = [0.0f32; 3];
    for i in 0..3 {
        let dielectric = base[i] * diffuse + highlight * 0.9;
        let metal = base[i] * reflection * SKY_TINT[i] + highlight * (0.35 + 0.65 * base[i]);
        rgb[i] = dielectric
            + (metal - dielectric) * metallic
            + rim * SKY_TINT[i] * (0.4 + 0.6 * base[i])
            + emissive[i] * material.emissive_strength.max(0.0);
    }
    [rgb[0], rgb[1], rgb[2], base[3]]
}

fn color_distance(a: [f32; 4], b: [f32; 4]) -> f32 {
    (0..4).map(|c| (a[c] - b[c]).powi(2)).sum()
}

fn average_color(colors: &[[f32; 4]]) -> [f32; 4] {
    let mut sum = [0.0; 4];
    for color in colors {
        for i in 0..4 {
            sum[i] += color[i];
        }
    }
    sum.map(|value| value / colors.len().max(1) as f32)
}

fn to_linear(color: peniko::Color) -> [f32; 4] {
    let [r, g, b, a] = color.components;
    [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b), a]
}

fn from_linear(linear: [f32; 4]) -> peniko::Color {
    peniko::Color::new([
        linear_to_srgb(linear[0]),
        linear_to_srgb(linear[1]),
        linear_to_srgb(linear[2]),
        linear[3].clamp(0.0, 1.0),
    ])
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera(projection: Projection) -> Camera {
        let mut camera = Camera::ortho_2d_frame(16.0, 9.0, 1920, 1080);
        camera.projection = projection;
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        camera
    }

    fn perspective() -> Projection {
        Projection::Perspective {
            fov_y: std::f64::consts::FRAC_PI_4,
            near: 0.1,
            far: 100.0,
        }
    }

    #[test]
    fn orthographic_projection_keeps_the_z0_plane_in_place() {
        let projector = Projector::new(&camera(Projection::Orthographic { zoom: 1.0 })).unwrap();
        let point = projector.project(DVec3::new(3.0, -2.0, 0.0)).unwrap();
        assert!((point.x - 3.0).abs() < 1e-9 && (point.y + 2.0).abs() < 1e-9);
    }

    #[test]
    fn perspective_shrinks_distant_points_toward_the_center() {
        let projector = Projector::new(&camera(perspective())).unwrap();
        let near = projector.project(DVec3::new(2.0, 1.0, 0.0)).unwrap();
        let far = projector.project(DVec3::new(2.0, 1.0, -10.0)).unwrap();
        assert!(far.x < near.x && far.y < near.y && far.x > 0.0);
        assert!(projector.project(DVec3::new(0.0, 0.0, 20.0)).is_none());
    }

    #[test]
    fn segments_crossing_the_near_plane_are_clipped() {
        let projector = Projector::new(&camera(perspective())).unwrap();
        let (start, end) = projector
            .clip_segment(DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 0.0, 20.0))
            .unwrap();
        assert_eq!(start, DVec3::ZERO);
        assert!((projector.depth(end) - 0.1).abs() < 1e-9);
        assert!(
            projector
                .clip_segment(DVec3::new(0.0, 0.0, 15.0), DVec3::new(1.0, 0.0, 20.0))
                .is_none()
        );
    }

    fn quad(material: Option<Material3D>, z: f32) -> TriangleMeshData {
        TriangleMeshData {
            vertices: vec![
                [-1.0, -1.0, z],
                [1.0, -1.0, z],
                [1.0, 1.0, z],
                [-1.0, 1.0, z],
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            normals: None,
            uvs: None,
            color: Some(peniko::Color::from_rgb8(200, 40, 40)),
            colors: None,
            material,
        }
    }

    fn items(content: &[(TriangleMeshData, Option<Material3D>)]) -> Vec<Item<usize>> {
        let projector = Projector::new(&camera(perspective())).unwrap();
        let transform = GlobalSpatialTransform::default();
        let opacity = GlobalOpacity(1.0);
        let mut items = Vec::new();
        for (key, (mesh, material)) in content.iter().enumerate() {
            projector.push(
                key,
                &Content {
                    transform: &transform,
                    opacity: &opacity,
                    mesh: Some(mesh),
                    lines: None,
                    material: material.as_ref(),
                },
                &Lighting3D::default(),
                &mut items,
            );
        }
        sort_back_to_front(&mut items);
        items
    }

    #[test]
    fn unlit_meshes_keep_their_color_and_draw_back_to_front() {
        let items = items(&[(quad(None, 0.0), None), (quad(None, -5.0), None)]);
        assert_eq!(items.len(), 4);
        assert!(items[..2].iter().all(|item| item.key == 1));
        assert!(items[2..].iter().all(|item| item.key == 0));
        let Paint::Solid(color) = items[0].paint else {
            panic!("an unlit mesh of one color is solid");
        };
        assert_eq!(color.to_rgba8().to_u8_array()[..3], [200, 40, 40]);
    }

    #[test]
    fn lit_meshes_are_shaded_solids_without_back_faces() {
        let material = Material3D::default();
        let front = items(&[(quad(Some(material), 0.0), Some(material))]);
        assert_eq!(front.len(), 1);
        let Primitive::Solid { faces } = &front[0].primitive else {
            panic!("an opaque lit mesh sorts as one solid");
        };
        assert_eq!(faces.len(), 2);
        assert_ne!(front[0].paint, Paint::Solid(Material3D::default().color));
        let mut back = quad(Some(material), 0.0);
        back.indices = vec![0, 2, 1, 0, 3, 2];
        assert!(items(&[(back, Some(material))]).is_empty());
    }

    #[test]
    fn line_strips_become_segments_with_averaged_vertex_colors() {
        let projector = Projector::new(&camera(Projection::Orthographic { zoom: 1.0 })).unwrap();
        let lines = LineListData {
            points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]],
            indices: None,
            strip: true,
            color: peniko::Color::WHITE,
            colors: Some(vec![
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
            ]),
        };
        let transform = GlobalSpatialTransform::default();
        let opacity = GlobalOpacity(0.5);
        let mut items = Vec::new();
        projector.push(
            0,
            &Content {
                transform: &transform,
                opacity: &opacity,
                mesh: None,
                lines: Some(&lines),
                material: None,
            },
            &Lighting3D::default(),
            &mut items,
        );
        assert_eq!(items.len(), 2);
        assert!(matches!(items[0].primitive, Primitive::Segment { .. }));
        let Paint::Linear { from, to, .. } = items[0].paint else {
            panic!("a segment between two colors is a gradient");
        };
        assert_eq!(from.to_rgba8().to_u8_array(), [255, 0, 0, 255]);
        assert_eq!(to.to_rgba8().to_u8_array(), [0, 0, 255, 255]);
        assert_eq!(items[0].opacity, 0.5);
    }
}
