//! SVG import into gaanim vector paths.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use gaanim_core::kurbo::{BezPath, Point, Shape};
use gaanim_core::peniko::{Brush, Color, Extend, Gradient};
use gaanim_math::Bounds3D;
use gaanim_scene::StrokeBrush;

const DEJAVU_SANS_BOLD: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf");

/// SVG user units (CSS pixels) that make one logical scene unit.
///
/// This is the same 100 px → 1 unit factor the pixel-to-logical-unit migration
/// applied to every other authored length (a 3 px default stroke became 0.03, a
/// 40 px body font 0.40). Imported geometry, stroke widths, gradient radii,
/// clip paths and filter lengths therefore keep their proportions relative to
/// native Gaanim objects: a 200×100 px document spans 2×1 units.
pub const SVG_PIXELS_PER_UNIT: f64 = 100.0;

/// Convert an SVG length (already expressed in document user units) to scene units.
fn svg_length(value: f64) -> f64 {
    value / SVG_PIXELS_PER_UNIT
}

/// A fully resolved SVG path ready to spawn as an engine mobject.
#[derive(Debug, Clone)]
pub struct SvgPath {
    pub id: String,
    pub path: BezPath,
    pub bounds: Bounds3D,
    pub fill: Option<Brush>,
    pub stroke: StrokeBrush,
}

/// A parsed SVG document. Basic SVG shapes have already been normalized to paths.
#[derive(Debug, Clone)]
pub struct SvgDocument {
    pub root: SvgGroup,
}

/// A source SVG group whose children retain their original nesting.
#[derive(Debug, Clone)]
pub struct SvgGroup {
    pub id: String,
    pub opacity: f32,
    /// Resolved vector clip geometry in the same scene coordinates as children.
    pub clip_path: Option<BezPath>,
    /// Common `feGaussianBlur` approximation supported by the vector renderer.
    pub blur_sigma: Option<f64>,
    /// Common `feDropShadow` approximation supported by the vector renderer.
    pub shadow: Option<SvgShadow>,
    pub children: Vec<SvgNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SvgShadow {
    pub color: Color,
    pub offset_x: f64,
    pub offset_y: f64,
    pub blur_radius: f64,
}

impl SvgGroup {
    /// Union of every descendant path's bounds, in document coordinates.
    pub fn bounds(&self) -> Option<Bounds3D> {
        self.children
            .iter()
            .filter_map(|child| match child {
                SvgNode::Group(group) => group.bounds(),
                SvgNode::Path(path) => Some(path.bounds),
            })
            .reduce(|a, b| a.union(&b))
    }
}

/// One addressable node in an imported SVG hierarchy.
#[derive(Debug, Clone)]
pub enum SvgNode {
    Group(SvgGroup),
    Path(Box<SvgPath>),
}

#[derive(Debug, thiserror::Error)]
pub enum SvgLoadError {
    #[error("could not read SVG '{path}': {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not parse SVG '{path}': {source}")]
    Parse { path: PathBuf, source: usvg::Error },
    #[error("SVG '{path}' contains duplicate id '{id}'")]
    DuplicateId { path: PathBuf, id: String },
}

impl SvgDocument {
    /// Load an SVG while retaining source groups and named paths.
    ///
    /// Coordinates are converted to logical scene units at
    /// [`SVG_PIXELS_PER_UNIT`] document pixels per unit, centered on the
    /// document's `width`/`height` (or `viewBox` when those are absent).
    ///
    /// `usvg` resolves basic shapes, CSS styles, `<use>`, `viewBox`, and nested
    /// transforms before this conversion. Raster `<image>` nodes and advanced
    /// filters are intentionally skipped in this first vector-only importer.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, SvgLoadError> {
        let path = path.as_ref();
        let data = std::fs::read(path).map_err(|source| SvgLoadError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let options = usvg::Options {
            resources_dir: path.parent().map(Path::to_path_buf),
            fontdb: svg_font_database(),
            ..Default::default()
        };
        let tree =
            usvg::Tree::from_data(&data, &options).map_err(|source| SvgLoadError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
        let size = tree.size();
        let mut ids = HashSet::new();
        let root = collect_group(
            tree.root(),
            size.width() as f64,
            size.height() as f64,
            path,
            &mut ids,
        )?;
        Ok(Self { root })
    }
}

fn svg_font_database() -> Arc<usvg::fontdb::Database> {
    static FONT_DATABASE: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    Arc::clone(FONT_DATABASE.get_or_init(|| {
        let mut database = usvg::fontdb::Database::new();
        // SVG text must not change shape when a host has a different collection
        // of system fonts. Register Gaanim's deterministic face first so an
        // explicit `DejaVu Sans` request always resolves to these exact bytes.
        database.load_font_data(DEJAVU_SANS_BOLD.to_vec());
        database.load_system_fonts();
        // The generic `sans-serif` family defaults to Arial, which many Linux
        // hosts lack; text asking for it would then disappear. Resolve it to
        // the bundled face instead, the same on every machine.
        database.set_sans_serif_family("DejaVu Sans");
        Arc::new(database)
    }))
}

fn collect_group(
    group: &usvg::Group,
    width: f64,
    height: f64,
    source_path: &Path,
    ids: &mut HashSet<String>,
) -> Result<SvgGroup, SvgLoadError> {
    collect_group_transformed(group, width, height, source_path, ids, None)
}

fn collect_group_transformed(
    group: &usvg::Group,
    width: f64,
    height: f64,
    source_path: &Path,
    ids: &mut HashSet<String>,
    outer_transform: Option<usvg::Transform>,
) -> Result<SvgGroup, SvgLoadError> {
    register_id(group.id(), source_path, ids)?;
    let mut children = Vec::new();
    for node in group.children() {
        match node {
            usvg::Node::Group(group) => children.push(SvgNode::Group(collect_group_transformed(
                group,
                width,
                height,
                source_path,
                ids,
                outer_transform,
            )?)),
            usvg::Node::Path(path) => {
                register_id(path.id(), source_path, ids)?;
                if path.is_visible()
                    && let Some(path) = convert_path(path, width, height, outer_transform)
                {
                    children.push(SvgNode::Path(Box::new(path)));
                }
            }
            usvg::Node::Text(text) => {
                let text_transform = outer_transform
                    .map(|outer| compose_transform(text.abs_transform(), outer))
                    .unwrap_or_else(|| text.abs_transform());
                let mut text_group = collect_group_transformed(
                    text.flattened(),
                    width,
                    height,
                    source_path,
                    ids,
                    Some(text_transform),
                )?;
                if text_group.id != text.id() {
                    register_id(text.id(), source_path, ids)?;
                }
                text_group.id = text.id().to_owned();
                if !text_group.children.is_empty() {
                    children.push(SvgNode::Group(text_group));
                }
            }
            _ => {}
        }
    }
    Ok(SvgGroup {
        id: group.id().to_owned(),
        opacity: group.opacity().get(),
        clip_path: group.clip_path().and_then(|clip| {
            // The clip's shapes live in the group's user space.
            let space =
                compose_transform(group.abs_transform(), outer_transform.unwrap_or_default());
            convert_clip_path(clip, space, width, height)
        }),
        blur_sigma: group_blur(group),
        shadow: group_shadow(group),
        children,
    })
}

fn group_blur(group: &usvg::Group) -> Option<f64> {
    let scale = transform_scale(group.abs_transform());
    group
        .filters()
        .iter()
        .flat_map(|filter| filter.primitives())
        .filter_map(|primitive| match primitive.kind() {
            usvg::filter::Kind::GaussianBlur(blur) => Some(svg_length(
                f64::from((blur.std_dev_x().get() + blur.std_dev_y().get()) * 0.5) * scale,
            )),
            _ => None,
        })
        .next_back()
}

fn group_shadow(group: &usvg::Group) -> Option<SvgShadow> {
    let scale = transform_scale(group.abs_transform());
    group
        .filters()
        .iter()
        .flat_map(|filter| filter.primitives())
        .filter_map(|primitive| match primitive.kind() {
            usvg::filter::Kind::DropShadow(shadow) => Some(SvgShadow {
                color: svg_color(shadow.color(), shadow.opacity().get()),
                offset_x: svg_length(f64::from(shadow.dx()) * scale),
                offset_y: -svg_length(f64::from(shadow.dy()) * scale),
                blur_radius: svg_length(
                    f64::from((shadow.std_dev_x().get() + shadow.std_dev_y().get()) * 0.5) * scale,
                ),
            }),
            _ => None,
        })
        .next_back()
}

fn transform_scale(transform: usvg::Transform) -> f64 {
    f64::from((transform.sx * transform.sy - transform.kx * transform.ky).abs()).sqrt()
}

fn register_id(
    id: &str,
    source_path: &Path,
    ids: &mut HashSet<String>,
) -> Result<(), SvgLoadError> {
    if !id.is_empty() && !ids.insert(id.to_owned()) {
        return Err(SvgLoadError::DuplicateId {
            path: source_path.to_path_buf(),
            id: id.to_owned(),
        });
    }
    Ok(())
}

fn convert_path(
    path: &usvg::Path,
    width: f64,
    height: f64,
    outer_transform: Option<usvg::Transform>,
) -> Option<SvgPath> {
    let transform = path.abs_transform();
    let bez = convert_path_data(path.data(), transform, outer_transform, width, height);
    if bez.elements().is_empty() {
        return None;
    }

    let bounds = bez.bounding_box();
    // Gradients are in the path's user space, like its points.
    let space = compose_transform(transform, outer_transform.unwrap_or_default());
    let fill = path
        .fill()
        .and_then(|fill| paint_to_brush(fill.paint(), fill.opacity().get(), width, height, space));
    let stroke = path.stroke().and_then(|stroke| {
        paint_to_brush(stroke.paint(), stroke.opacity().get(), width, height, space).map(|brush| {
            let scale =
                transform_scale(transform) * outer_transform.map(transform_scale).unwrap_or(1.0);
            StrokeBrush {
                brush: Some(brush),
                style: gaanim_core::kurbo::Stroke::new(svg_length(
                    f64::from(stroke.width().get()) * scale,
                )),
            }
        })
    });

    Some(SvgPath {
        id: path.id().to_owned(),
        path: bez,
        bounds: Bounds3D::new_2d(bounds.x0, bounds.y0, bounds.x1, bounds.y1),
        fill,
        stroke: stroke.unwrap_or_else(StrokeBrush::transparent),
    })
}

/// The brush of `paint`, whose coordinates are in the user space that
/// `space` maps to document pixels.
fn paint_to_brush(
    paint: &usvg::Paint,
    opacity: f32,
    width: f64,
    height: f64,
    space: usvg::Transform,
) -> Option<Brush> {
    match paint {
        usvg::Paint::Color(color) => Some(Brush::Solid(svg_color(*color, opacity))),
        usvg::Paint::LinearGradient(gradient) => {
            let transform = compose_transform(gradient.transform(), space);
            let (x1, y1) = transform_point(gradient.x1(), gradient.y1(), transform);
            // An affine map keeps a linear gradient linear, but not its axis
            // perpendicular to the bands: the gradient of the color's
            // parameter maps by the inverse transpose.
            let (dx, dy) = (gradient.x2() - gradient.x1(), gradient.y2() - gradient.y1());
            let length = dx * dx + dy * dy;
            let det = transform.sx * transform.sy - transform.kx * transform.ky;
            if length <= f32::EPSILON || det.abs() <= f32::EPSILON {
                return gradient
                    .stops()
                    .last()
                    .map(|stop| Brush::Solid(stop_color(stop, opacity)));
            }
            let (dx, dy) = (dx / length, dy / length);
            let (gx, gy) = (
                (transform.sy * dx - transform.ky * dy) / det,
                (-transform.kx * dx + transform.sx * dy) / det,
            );
            let span = gx * gx + gy * gy;
            let start = scene_point((x1, y1), width, height);
            let end = scene_point((x1 + gx / span, y1 + gy / span), width, height);
            let stops = gradient_stops(gradient.stops(), opacity);
            Some(Brush::Gradient(
                Gradient::new_linear(start, end)
                    .with_extend(convert_spread(gradient.spread_method()))
                    .with_stops(stops.as_slice()),
            ))
        }
        usvg::Paint::RadialGradient(gradient) => {
            let transform = compose_transform(gradient.transform(), space);
            let focus = scene_point(
                transform_point(gradient.fx(), gradient.fy(), transform),
                width,
                height,
            );
            let center = scene_point(
                transform_point(gradient.cx(), gradient.cy(), transform),
                width,
                height,
            );
            // A non-uniform scale would turn the circles into ellipses; the
            // radii take the transform's mean scale.
            let scale = transform_scale(transform) as f32 / SVG_PIXELS_PER_UNIT as f32;
            let stops = gradient_stops(gradient.stops(), opacity);
            Some(Brush::Gradient(
                Gradient::new_two_point_radial(
                    focus,
                    gradient.fr().get() * scale,
                    center,
                    gradient.r().get() * scale,
                )
                .with_extend(convert_spread(gradient.spread_method()))
                .with_stops(stops.as_slice()),
            ))
        }
        usvg::Paint::Pattern(_) => None,
    }
}

fn gradient_stops(stops: &[usvg::Stop], opacity: f32) -> Vec<(f32, Color)> {
    stops
        .iter()
        .map(|stop| (stop.offset().get(), stop_color(stop, opacity)))
        .collect()
}

fn stop_color(stop: &usvg::Stop, opacity: f32) -> Color {
    svg_color(stop.color(), stop.opacity().get() * opacity)
}

fn svg_color(color: usvg::Color, opacity: f32) -> Color {
    Color::from_rgba8(
        color.red,
        color.green,
        color.blue,
        (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

fn convert_spread(spread: usvg::SpreadMethod) -> Extend {
    match spread {
        usvg::SpreadMethod::Pad => Extend::Pad,
        usvg::SpreadMethod::Reflect => Extend::Reflect,
        usvg::SpreadMethod::Repeat => Extend::Repeat,
    }
}

fn convert_clip_path(
    clip: &usvg::ClipPath,
    space: usvg::Transform,
    width: f64,
    height: f64,
) -> Option<BezPath> {
    let mut result = BezPath::new();
    collect_clip_nodes(
        clip.root(),
        Some(compose_transform(clip.transform(), space)),
        width,
        height,
        &mut result,
    );
    (!result.is_empty()).then_some(result)
}

fn collect_clip_nodes(
    group: &usvg::Group,
    outer_transform: Option<usvg::Transform>,
    width: f64,
    height: f64,
    result: &mut BezPath,
) {
    for node in group.children() {
        match node {
            usvg::Node::Group(group) => {
                collect_clip_nodes(group, outer_transform, width, height, result)
            }
            usvg::Node::Path(path) => {
                let converted = convert_path_data(
                    path.data(),
                    path.abs_transform(),
                    outer_transform,
                    width,
                    height,
                );
                result.extend(converted.elements().iter().copied());
            }
            usvg::Node::Text(text) => {
                collect_clip_nodes(text.flattened(), outer_transform, width, height, result)
            }
            _ => {}
        }
    }
}

fn convert_path_data(
    data: &usvg::tiny_skia_path::Path,
    transform: usvg::Transform,
    outer_transform: Option<usvg::Transform>,
    width: f64,
    height: f64,
) -> BezPath {
    let to_scene = |point: usvg::tiny_skia_path::Point| {
        let transformed = transform_point(point.x, point.y, transform);
        let transformed = outer_transform
            .map(|outer| transform_point(transformed.0, transformed.1, outer))
            .unwrap_or(transformed);
        scene_point(transformed, width, height)
    };
    let mut bez = BezPath::new();
    for segment in data.segments() {
        match segment {
            usvg::tiny_skia_path::PathSegment::MoveTo(point) => bez.move_to(to_scene(point)),
            usvg::tiny_skia_path::PathSegment::LineTo(point) => bez.line_to(to_scene(point)),
            usvg::tiny_skia_path::PathSegment::QuadTo(p1, p2) => {
                bez.quad_to(to_scene(p1), to_scene(p2));
            }
            usvg::tiny_skia_path::PathSegment::CubicTo(p1, p2, p3) => {
                bez.curve_to(to_scene(p1), to_scene(p2), to_scene(p3));
            }
            usvg::tiny_skia_path::PathSegment::Close => bez.close_path(),
        }
    }
    bez
}

fn transform_point(x: f32, y: f32, transform: usvg::Transform) -> (f32, f32) {
    (
        x * transform.sx + y * transform.kx + transform.tx,
        x * transform.ky + y * transform.sy + transform.ty,
    )
}

/// Compose transforms so the result applies `first`, then `second`.
fn compose_transform(first: usvg::Transform, second: usvg::Transform) -> usvg::Transform {
    usvg::Transform {
        sx: second.sx * first.sx + second.kx * first.ky,
        kx: second.sx * first.kx + second.kx * first.sy,
        ky: second.ky * first.sx + second.sy * first.ky,
        sy: second.ky * first.kx + second.sy * first.sy,
        tx: second.sx * first.tx + second.kx * first.ty + second.tx,
        ty: second.ky * first.tx + second.sy * first.ty + second.ty,
    }
}

/// Map an SVG document point (Y down, origin top-left) to centered scene units (Y up).
fn scene_point((x, y): (f32, f32), width: f64, height: f64) -> Point {
    Point::new(
        svg_length(f64::from(x) - width * 0.5),
        svg_length(height * 0.5 - f64::from(y)),
    )
}

#[cfg(test)]
mod tests {
    use super::{SVG_PIXELS_PER_UNIT, SvgDocument, SvgLoadError, SvgNode, svg_font_database};
    use gaanim_core::kurbo::Shape;
    use gaanim_core::peniko::Brush;

    #[test]
    fn generic_sans_serif_resolves_to_the_bundled_face() {
        let database = svg_font_database();
        assert_eq!(
            database.family_name(&usvg::fontdb::Family::SansSerif),
            "DejaVu Sans"
        );
        assert!(
            database
                .query(&usvg::fontdb::Query {
                    families: &[usvg::fontdb::Family::SansSerif],
                    ..Default::default()
                })
                .is_some(),
            "sans-serif text must find a face even without system fonts"
        );
    }

    #[test]
    fn bundled_dejavu_bold_wins_over_system_fonts() {
        let database = svg_font_database();
        let id = database
            .query(&usvg::fontdb::Query {
                families: &[usvg::fontdb::Family::Name("DejaVu Sans")],
                weight: usvg::fontdb::Weight::BOLD,
                ..usvg::fontdb::Query::default()
            })
            .expect("bundled DejaVu Sans Bold should resolve");
        let face = database.face(id).expect("resolved face should exist");
        assert!(matches!(face.source, usvg::fontdb::Source::Binary(_)));
    }

    #[test]
    fn group_bounds_cover_nested_paths() {
        let temp = std::env::temp_dir().join("gaanim_svg_bounds_test.svg");
        std::fs::write(
            &temp,
            r##"<svg width="80" height="80" xmlns="http://www.w3.org/2000/svg">
                <circle cx="40" cy="40" r="36" fill="#FFD166"/>
                <g><rect x="30" y="30" width="10" height="10" fill="#17233D"/></g>
            </svg>"##,
        )
        .unwrap();
        let document = SvgDocument::load(&temp).unwrap();
        let bounds = document.root.bounds().expect("paths have bounds");
        assert!((bounds.width() - 0.72).abs() < 0.005, "{bounds:?}");
        assert!((bounds.height() - 0.72).abs() < 0.005, "{bounds:?}");
    }

    #[test]
    fn imports_shapes_transforms_and_solid_styles() {
        let temp = std::env::temp_dir().join("gaanim_svg_import_test.svg");
        std::fs::write(
            &temp,
            r##"<svg width="100" height="60" xmlns="http://www.w3.org/2000/svg">
                <g id="shapes" transform="translate(10 5)" opacity=".5">
                  <rect id="box" width="20" height="10" fill="#ff0000" stroke="#0000ff" stroke-width="2"/>
                  <circle id="dot" cx="40" cy="20" r="5" fill="#00ff00"/>
                </g>
              </svg>"##,
        )
        .unwrap();
        let document = SvgDocument::load(&temp).unwrap();
        std::fs::remove_file(temp).unwrap();
        let SvgNode::Group(shapes) = &document.root.children[0] else {
            panic!("expected source group");
        };
        assert_eq!(shapes.id, "shapes");
        assert_eq!(shapes.opacity, 0.5);
        assert_eq!(shapes.children.len(), 2);
        let SvgNode::Path(box_path) = &shapes.children[0] else {
            panic!("expected box path");
        };
        assert_eq!(box_path.id, "box");
        assert!(box_path.fill.is_some());
        assert!(box_path.stroke.brush.is_some());
        let SvgNode::Path(dot_path) = &shapes.children[1] else {
            panic!("expected dot path");
        };
        assert_eq!(dot_path.id, "dot");
    }

    #[test]
    fn rejects_duplicate_group_and_path_ids() {
        let temp = std::env::temp_dir().join("gaanim_svg_duplicate_id_test.svg");
        std::fs::write(
            &temp,
            r##"<svg width="40" height="40" xmlns="http://www.w3.org/2000/svg">
                <g id="same"><circle id="same" cx="20" cy="20" r="10"/></g>
              </svg>"##,
        )
        .unwrap();
        let error = SvgDocument::load(&temp).unwrap_err();
        std::fs::remove_file(temp).unwrap();
        assert!(matches!(
            error,
            SvgLoadError::DuplicateId { id, .. } if id == "same"
        ));
    }

    #[test]
    fn imports_gradients_clip_paths_and_outlined_text() {
        let temp = std::env::temp_dir().join("gaanim_svg_advanced_import_test.svg");
        std::fs::write(
            &temp,
            r##"<svg width="240" height="120" xmlns="http://www.w3.org/2000/svg">
                <defs>
                  <linearGradient id="sky" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="180" y2="0">
                    <stop offset="0" stop-color="#2563eb"/>
                    <stop offset="1" stop-color="#a855f7" stop-opacity=".75"/>
                  </linearGradient>
                  <radialGradient id="sun" gradientUnits="userSpaceOnUse" cx="190" cy="35" r="24">
                    <stop offset="0" stop-color="#ffffff"/>
                    <stop offset="1" stop-color="#f59e0b"/>
                  </radialGradient>
                  <clipPath id="window"><rect x="20" y="15" width="200" height="90" rx="18"/></clipPath>
                  <filter id="soft">
                    <feDropShadow dx="4" dy="5" stdDeviation="3" flood-color="#111827" flood-opacity=".6"/>
                    <feGaussianBlur stdDeviation="1.5"/>
                  </filter>
                </defs>
                <g id="card" clip-path="url(#window)" filter="url(#soft)">
                  <rect id="background" width="240" height="120" fill="url(#sky)"/>
                  <circle id="orb" cx="190" cy="35" r="24" fill="url(#sun)"/>
                  <text id="label" x="35" y="82" font-family="sans-serif" font-size="22"
                        fill="#ffffff">Advanced SVG</text>
                </g>
              </svg>"##,
        )
        .unwrap();
        let document = SvgDocument::load(&temp).unwrap();
        std::fs::remove_file(temp).unwrap();

        let SvgNode::Group(card) = &document.root.children[0] else {
            panic!("expected clipped card group");
        };
        assert_eq!(card.id, "card");
        let clip = card
            .clip_path
            .as_ref()
            .expect("clip path should be retained");
        assert!(clip.bounding_box().width() > 1.9);
        assert!((card.blur_sigma.unwrap() - 0.015).abs() < 1e-9);
        let shadow = card
            .shadow
            .as_ref()
            .expect("drop shadow should be retained");
        assert!((shadow.offset_x - 0.04).abs() < 1e-9, "{shadow:?}");
        assert!((shadow.offset_y + 0.05).abs() < 1e-9, "{shadow:?}");
        assert!((shadow.blur_radius - 0.03).abs() < 1e-9, "{shadow:?}");

        let SvgNode::Path(background) = &card.children[0] else {
            panic!("expected gradient background");
        };
        assert!(matches!(background.fill, Some(Brush::Gradient(_))));
        let SvgNode::Path(orb) = &card.children[1] else {
            panic!("expected radial-gradient orb");
        };
        assert!(matches!(orb.fill, Some(Brush::Gradient(_))));

        let SvgNode::Group(label) = &card.children[2] else {
            panic!("expected outlined text group");
        };
        assert_eq!(label.id, "label");
        assert!(!label.children.is_empty());
    }

    #[test]
    fn gradients_and_clips_follow_transforms_and_the_view_box() {
        use gaanim_core::peniko::GradientKind;
        // 100 px per unit: the view box puts the document origin at the center.
        let document = load_temp(
            "gaanim_svg_transformed_paints",
            r##"<svg width="400" height="200" viewBox="-200 -100 400 200" xmlns="http://www.w3.org/2000/svg">
                <defs>
                  <radialGradient id="r"><stop offset="0" stop-color="#f00"/><stop offset="1" stop-color="#00f"/></radialGradient>
                  <linearGradient id="l" gradientUnits="userSpaceOnUse" x1="-20" y1="0" x2="20" y2="0">
                    <stop offset="0" stop-color="#ff0"/><stop offset="1" stop-color="#0ff"/>
                  </linearGradient>
                  <clipPath id="c"><rect x="-10" y="-10" width="20" height="20"/></clipPath>
                </defs>
                <g transform="translate(100 0) scale(2)"><circle id="orb" r="20" fill="url(#r)"/></g>
                <g transform="translate(-100 50) rotate(90)"><rect id="bar" x="-20" y="-5" width="40" height="10" fill="url(#l)"/></g>
                <g id="clipped" transform="translate(0 -50)" clip-path="url(#c)"><circle r="30" fill="#0a0"/></g>
              </svg>"##,
        );
        let path = |id: &str| {
            fn find<'a>(group: &'a super::SvgGroup, id: &str) -> Option<&'a super::SvgPath> {
                group.children.iter().find_map(|node| match node {
                    SvgNode::Path(path) if path.id == id => Some(path.as_ref()),
                    SvgNode::Group(group) => find(group, id),
                    _ => None,
                })
            }
            find(&document.root, id).expect("path")
        };
        let Some(Brush::Gradient(orb)) = &path("orb").fill else {
            panic!("radial gradient");
        };
        let GradientKind::Radial(radial) = orb.kind else {
            panic!("radial gradient");
        };
        assert!((radial.end_center.x - 1.0).abs() < 1e-6, "{radial:?}");
        assert!(radial.end_center.y.abs() < 1e-6, "{radial:?}");
        assert!((radial.end_radius - 0.4).abs() < 1e-5, "{radial:?}");

        // Rotated a quarter turn, the bands run across the y axis: y down in
        // the document is y up in the scene.
        let Some(Brush::Gradient(bar)) = &path("bar").fill else {
            panic!("linear gradient");
        };
        let GradientKind::Linear(linear) = bar.kind else {
            panic!("linear gradient");
        };
        assert!((linear.start.x + 1.0).abs() < 1e-6 && (linear.end.x + 1.0).abs() < 1e-6);
        assert!((linear.start.y - -0.3).abs() < 1e-6, "{linear:?}");
        assert!((linear.end.y - -0.7).abs() < 1e-6, "{linear:?}");

        fn clipped(group: &super::SvgGroup) -> Option<&super::SvgGroup> {
            if group.clip_path.is_some() {
                return Some(group);
            }
            group.children.iter().find_map(|node| match node {
                SvgNode::Group(group) => clipped(group),
                _ => None,
            })
        }
        let clipped = clipped(&document.root).expect("clipped group");
        let clip = clipped.clip_path.as_ref().expect("clip").bounding_box();
        assert!(
            (clip.center().x).abs() < 1e-6 && (clip.center().y - 0.5).abs() < 1e-6,
            "{clip:?}"
        );
        assert!((clip.width() - 0.2).abs() < 1e-6, "{clip:?}");
    }

    fn load_temp(name: &str, contents: &str) -> SvgDocument {
        let temp = std::env::temp_dir().join(format!("{name}_{}.svg", std::process::id()));
        std::fs::write(&temp, contents).unwrap();
        let document = SvgDocument::load(&temp).unwrap();
        std::fs::remove_file(temp).unwrap();
        document
    }

    #[test]
    fn imports_one_logical_unit_per_hundred_svg_pixels_centered_on_origin() {
        assert_eq!(SVG_PIXELS_PER_UNIT, 100.0);
        let document = load_temp(
            "gaanim_svg_logical_scale_test",
            r##"<svg width="200" height="100" xmlns="http://www.w3.org/2000/svg">
                <rect id="full" width="200" height="100" fill="#ff0000"/>
                <rect id="corner" x="150" y="0" width="50" height="50" fill="#00ff00"/>
              </svg>"##,
        );
        let bounds = document.root.bounds().expect("paths have bounds");
        assert!((bounds.min.x + 1.0).abs() < 1e-6, "{bounds:?}");
        assert!((bounds.max.x - 1.0).abs() < 1e-6, "{bounds:?}");
        assert!((bounds.min.y + 0.5).abs() < 1e-6, "{bounds:?}");
        assert!((bounds.max.y - 0.5).abs() < 1e-6, "{bounds:?}");

        // Top-right corner in SVG (Y down) lands top-right in the scene (Y up).
        let SvgNode::Path(corner) = &document.root.children[1] else {
            panic!("expected corner path");
        };
        let corner = corner.bounds;
        let extents = [corner.min.x, corner.max.x, corner.min.y, corner.max.y];
        for (actual, expected) in extents.into_iter().zip([0.5, 1.0, 0.0, 0.5]) {
            assert!((actual - expected).abs() < 1e-6, "{corner:?}");
        }

        // A viewBox-only document uses the viewBox size as its pixel size.
        let document = load_temp(
            "gaanim_svg_viewbox_scale_test",
            r##"<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg">
                <rect width="100" height="100" fill="#ff0000"/>
              </svg>"##,
        );
        let bounds = document.root.bounds().expect("paths have bounds");
        assert!((bounds.width() - 1.0).abs() < 1e-6, "{bounds:?}");
        assert!((bounds.height() - 1.0).abs() < 1e-6, "{bounds:?}");
    }

    #[test]
    fn stroke_widths_and_gradient_radii_scale_with_the_geometry() {
        let document = load_temp(
            "gaanim_svg_logical_stroke_scale_test",
            r##"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
                <defs>
                  <radialGradient id="glow" gradientUnits="userSpaceOnUse" cx="50" cy="50" r="40">
                    <stop offset="0" stop-color="#ffffff"/>
                    <stop offset="1" stop-color="#000000"/>
                  </radialGradient>
                </defs>
                <rect id="plain" x="10" y="10" width="30" height="30" fill="none"
                      stroke="#000000" stroke-width="3"/>
                <g transform="scale(2)">
                  <rect id="scaled" x="30" y="5" width="10" height="10" fill="none"
                        stroke="#000000" stroke-width="3"/>
                </g>
                <circle id="orb" cx="50" cy="50" r="40" fill="url(#glow)"/>
              </svg>"##,
        );
        let path = |index: usize| match &document.root.children[index] {
            SvgNode::Path(path) => path.clone(),
            SvgNode::Group(group) => match &group.children[0] {
                SvgNode::Path(path) => path.clone(),
                SvgNode::Group(_) => panic!("unexpected nested group"),
            },
        };
        let plain = path(0);
        assert_eq!(plain.id, "plain");
        assert!((plain.stroke.style.width - 0.03).abs() < 1e-9);
        let scaled = path(1);
        assert_eq!(scaled.id, "scaled");
        assert!((scaled.stroke.style.width - 0.06).abs() < 1e-9);
        assert!((scaled.bounds.width() - 0.2).abs() < 1e-6);

        let orb = path(2);
        let Some(Brush::Gradient(gradient)) = &orb.fill else {
            panic!("expected radial gradient fill");
        };
        let gaanim_core::peniko::GradientKind::Radial(radial) = gradient.kind else {
            panic!("expected radial gradient kind, got {:?}", gradient.kind);
        };
        assert!((f64::from(radial.end_radius) - 0.4).abs() < 1e-6);
        assert!(radial.end_center.x.abs() < 1e-6 && radial.end_center.y.abs() < 1e-6);
    }
}
