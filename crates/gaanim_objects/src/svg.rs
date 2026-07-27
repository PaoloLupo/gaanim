//! SVG import into gaanim vector paths.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use gaanim_core::kurbo::{BezPath, Point, Shape};
use gaanim_core::peniko::{Brush, Color};
use gaanim_math::Bounds3D;
use gaanim_scene::StrokeBrush;

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
    pub children: Vec<SvgNode>,
}

/// One addressable node in an imported SVG hierarchy.
#[derive(Debug, Clone)]
pub enum SvgNode {
    Group(SvgGroup),
    Path(SvgPath),
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
    /// `usvg` resolves basic shapes, CSS styles, `<use>`, `viewBox`, and nested
    /// transforms before this conversion. Raster `<image>` nodes and advanced
    /// filters are intentionally skipped in this first vector-only importer.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, SvgLoadError> {
        let path = path.as_ref();
        let data = std::fs::read(path).map_err(|source| SvgLoadError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).map_err(|source| {
            SvgLoadError::Parse {
                path: path.to_path_buf(),
                source,
            }
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

fn collect_group(
    group: &usvg::Group,
    width: f64,
    height: f64,
    source_path: &Path,
    ids: &mut HashSet<String>,
) -> Result<SvgGroup, SvgLoadError> {
    register_id(group.id(), source_path, ids)?;
    let mut children = Vec::new();
    for node in group.children() {
        match node {
            usvg::Node::Group(group) => children.push(SvgNode::Group(collect_group(
                group,
                width,
                height,
                source_path,
                ids,
            )?)),
            usvg::Node::Path(path) => {
                register_id(path.id(), source_path, ids)?;
                if path.is_visible()
                    && let Some(path) = convert_path(path, width, height)
                {
                    children.push(SvgNode::Path(path));
                }
            }
            _ => {}
        }
    }
    Ok(SvgGroup {
        id: group.id().to_owned(),
        opacity: group.opacity().get(),
        children,
    })
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

fn convert_path(path: &usvg::Path, width: f64, height: f64) -> Option<SvgPath> {
    let transform = path.abs_transform();
    let to_scene = |p: usvg::tiny_skia_path::Point| {
        let x = p.x * transform.sx + p.y * transform.kx + transform.tx;
        let y = p.x * transform.ky + p.y * transform.sy + transform.ty;
        Point::new(f64::from(x) - width * 0.5, height * 0.5 - f64::from(y))
    };

    let mut bez = BezPath::new();
    for segment in path.data().segments() {
        match segment {
            usvg::tiny_skia_path::PathSegment::MoveTo(p) => bez.move_to(to_scene(p)),
            usvg::tiny_skia_path::PathSegment::LineTo(p) => bez.line_to(to_scene(p)),
            usvg::tiny_skia_path::PathSegment::QuadTo(p1, p2) => {
                bez.quad_to(to_scene(p1), to_scene(p2));
            }
            usvg::tiny_skia_path::PathSegment::CubicTo(p1, p2, p3) => {
                bez.curve_to(to_scene(p1), to_scene(p2), to_scene(p3));
            }
            usvg::tiny_skia_path::PathSegment::Close => bez.close_path(),
        }
    }
    if bez.elements().is_empty() {
        return None;
    }

    let bounds = bez.bounding_box();
    let fill = path
        .fill()
        .and_then(|fill| solid_paint(fill.paint(), fill.opacity().get()))
        .map(Brush::Solid);
    let stroke = path.stroke().and_then(|stroke| {
        solid_paint(stroke.paint(), stroke.opacity().get()).map(|color| {
            let scale =
                f64::from((transform.sx * transform.sy - transform.kx * transform.ky).abs()).sqrt();
            StrokeBrush::new(color, f64::from(stroke.width().get()) * scale)
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

fn solid_paint(paint: &usvg::Paint, opacity: f32) -> Option<Color> {
    let usvg::Paint::Color(color) = paint else {
        return None;
    };
    Some(Color::from_rgba8(
        color.red,
        color.green,
        color.blue,
        (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
    ))
}

#[cfg(test)]
mod tests {
    use super::{SvgDocument, SvgLoadError, SvgNode};

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
}
