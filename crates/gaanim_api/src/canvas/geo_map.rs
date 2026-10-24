//! Maps: drawings of longitude and latitude laid out by a [`MapView`].
//!
//! A map is not a drawable. It draws plain drawables in scene coordinates,
//! as an [`super::Axonometric`] does, and keeps an invisible frame where it
//! lies: drawings that reach past the frame are clipped to it.

use std::path::{Path, PathBuf};

use gaanim_core::glam::DVec2;
use gaanim_core::kurbo::{BezPath, Rect, Shape};
use gaanim_core::peniko::{Brush, Color, Fill};
use gaanim_math::Bounds3D;
use gaanim_objects::prelude::SvgPath;
use gaanim_renderer::effects::StrokeAlign;
use gaanim_scene::StrokeBrush;
use gaanim_text::prelude::{TextFlow, TextRole, TextSpec, TextStyle};
use gaanim_visualization::{GeoError, GeoLine, GeoPolygon, GraticuleOptions, MapSide, MapView};
use serde_json::{Map, Value};

use super::{Anchor, DrawableHandle, SceneModel, SpawnKind};

/// Points that move a GeoJSON drawing less than this, in scene units, are
/// dropped: about a quarter of a pixel in a 1080p frame.
const GEOJSON_TOLERANCE: f64 = 0.002;

/// A drawing this far inside the frame or farther needs no clip: a stroke
/// on its edge stays within the frame.
const CLIP_MARGIN: f64 = 0.05;

/// Radius of the dots of GeoJSON points.
const POINT_RADIUS: f64 = 0.05;

/// Gap between a graticule label, its line and the frame's edge.
const LABEL_GAP: f64 = 0.06;

/// Room between two graticule labels side by side, so a row of them reads
/// as separate labels.
const LABEL_SPACING: f64 = 0.2;

/// Colors without a theme; a theme's `map/...` styles replace them.
const AREA_FILL: Color = Color::from_rgb8(0x2A, 0x2A, 0x2A);
const AREA_STROKE: Color = Color::from_rgb8(0x70, 0x70, 0x70);
const LINE_STROKE: Color = Color::from_rgb8(0xE6, 0xE6, 0xE6);
const LABEL_FILL: Color = Color::from_rgb8(0xA0, 0xA0, 0xA0);

#[derive(Debug, thiserror::Error)]
pub enum GeoMapError {
    #[error(transparent)]
    Geo(#[from] GeoError),
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid GeoJSON: {0}")]
    Json(String),
    #[error("GeoJSON feature {feature}: {error}")]
    Feature { feature: usize, error: GeoError },
    #[error("feature {feature} has no property {key:?} that is a string or a number")]
    MissingKey { feature: usize, key: String },
    #[error("features {first} and {second} share the {key:?} {value:?}")]
    DuplicateKey {
        key: String,
        value: String,
        first: usize,
        second: usize,
    },
}

/// A map: where its window of longitude and latitude lies in the scene,
/// and the invisible frame its drawings are clipped to.
#[derive(Debug, Clone)]
pub struct GeoMapHandle {
    view: MapView,
    frame: DrawableHandle,
}

/// A graticule: its lines, as one drawable, and its labels, as a group.
#[derive(Debug, Clone)]
pub struct MapGraticule {
    /// The lines, then the labels.
    pub group: DrawableHandle,
    pub lines: DrawableHandle,
    pub labels: DrawableHandle,
}

/// One drawable per feature of a GeoJSON document, in the document's order.
#[derive(Debug, Clone)]
pub struct MapFeatures {
    pub group: DrawableHandle,
    pub features: Vec<MapFeature>,
}

#[derive(Debug, Clone)]
pub struct MapFeature {
    pub drawable: DrawableHandle,
    /// The feature's `id`, when it has one.
    pub id: Option<Value>,
    pub properties: Map<String, Value>,
}

impl MapFeature {
    /// The feature's properties as a JSON object.
    pub fn properties_json(&self) -> String {
        Value::Object(self.properties.clone()).to_string()
    }
}

impl MapFeatures {
    /// Each feature's `key` property as text, in order: the feature's `id`
    /// for a key `"id"` its properties lack. Every feature needs one, and no
    /// two may share it.
    pub fn keys(&self, key: &str) -> Result<Vec<String>, GeoMapError> {
        let mut keys: Vec<String> = Vec::with_capacity(self.features.len());
        for (index, feature) in self.features.iter().enumerate() {
            let value = feature
                .properties
                .get(key)
                .or(feature.id.as_ref().filter(|_| key == "id"));
            let text = match value {
                Some(Value::String(text)) => text.clone(),
                Some(Value::Number(number)) => number.to_string(),
                _ => {
                    return Err(GeoMapError::MissingKey {
                        feature: index,
                        key: key.to_owned(),
                    });
                }
            };
            if let Some(first) = keys.iter().position(|known| *known == text) {
                return Err(GeoMapError::DuplicateKey {
                    key: key.to_owned(),
                    value: text,
                    first,
                    second: index,
                });
            }
            keys.push(text);
        }
        Ok(keys)
    }
}

impl GeoMapHandle {
    pub fn view(&self) -> &MapView {
        &self.view
    }

    /// The invisible rectangle where the map lies, which clips its drawings.
    pub fn frame(&self) -> &DrawableHandle {
        &self.frame
    }

    /// Where `(lon, lat)` is drawn, in scene units.
    pub fn point(&self, lon: f64, lat: f64) -> Result<DVec2, GeoError> {
        if !(lon.is_finite() && lat.is_finite()) {
            return Err(GeoError::NonFinite);
        }
        let point = self.view.point(lon, lat);
        Ok(DVec2::new(point.x, point.y))
    }

    /// Move `drawable` to `(lon, lat)`, then `offset` further in scene units:
    /// its `anchor` point goes there, or without one its center (a group
    /// about an origin, its origin).
    pub fn place(
        &self,
        drawable: DrawableHandle,
        lon: f64,
        lat: f64,
        anchor: Option<Anchor>,
        offset: DVec2,
    ) -> Result<DrawableHandle, GeoError> {
        if !offset.is_finite() {
            return Err(GeoError::NonFinite);
        }
        let point = self.point(lon, lat)? + offset;
        Ok(match anchor {
            Some(anchor) => drawable.at_anchor(point.x, point.y, anchor),
            None => drawable.move_to_default(point.x, point.y),
        })
    }

    /// Whether a drawing within `extent` stays clear of the frame's edges.
    fn holds(&self, extent: Rect) -> bool {
        let inside = self.view.frame().inflate(-CLIP_MARGIN, -CLIP_MARGIN);
        inside.width() > 0.0
            && inside.height() > 0.0
            && extent.x0 >= inside.x0
            && extent.x1 <= inside.x1
            && extent.y0 >= inside.y0
            && extent.y1 <= inside.y1
    }
}

/// A GeoJSON geometry, read but not drawn yet.
enum Geometry {
    Points(Vec<(f64, f64)>),
    Lines(Vec<GeoLine>),
    Polygons(Vec<GeoPolygon>),
    Collection(Vec<Geometry>),
}

struct Feature {
    id: Option<Value>,
    properties: Map<String, Value>,
    geometry: Option<Geometry>,
}

/// What a geometry draws: paths already projected, so a bad ring fails
/// before anything is drawn.
enum Drawing {
    Area(BezPath),
    Line(BezPath),
    Points(Vec<(f64, f64)>),
    Collection(Vec<Drawing>),
}

impl SceneModel {
    /// A map drawn with `view`, and its invisible frame.
    pub fn geo_map(&mut self, view: MapView) -> GeoMapHandle {
        let rect = view.frame();
        let frame = self.spawn(SpawnKind::SvgPath(Box::new(SvgPath {
            id: "MapFrame".into(),
            path: rect.to_path(0.1),
            bounds: Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1),
            fill: None,
            stroke: StrokeBrush::new(Color::TRANSPARENT, 0.0),
        })));
        frame
            .spec
            .lock()
            .expect("map frame spec poisoned")
            .theme_selector = Some("map/frame".into());
        GeoMapHandle { view, frame }
    }

    /// One filled drawing of `polygons`, each an outer ring and rings that
    /// cut holes in it or add parts apart from it.
    pub fn map_polygon(
        &mut self,
        map: &GeoMapHandle,
        polygons: &[GeoPolygon],
    ) -> Result<DrawableHandle, GeoError> {
        let path = map.view.area_path(polygons, 0.0)?;
        Ok(self.map_drawing(map, Drawing::Area(path)))
    }

    /// One line through `lines`, each closed back to its start when `closed`.
    pub fn map_polyline(
        &mut self,
        map: &GeoMapHandle,
        lines: &[GeoLine],
        closed: bool,
    ) -> Result<DrawableHandle, GeoError> {
        let path = map.view.line_path(lines, closed, 0.0)?;
        Ok(self.map_drawing(map, Drawing::Line(path)))
    }

    /// The meridians and parallels `options` asks for, and their labels
    /// inside the frame: a label leaves the frame's edge and its line a
    /// small gap, and one that would cover another or leave the frame is
    /// left out.
    pub fn map_graticule(
        &mut self,
        map: &GeoMapHandle,
        options: &GraticuleOptions,
    ) -> Result<MapGraticule, GeoError> {
        let graticule = map.view.graticule(options)?;
        let frame = map.view.frame();
        let lines = self.spawn(SpawnKind::SvgPath(Box::new(SvgPath {
            id: "MapGraticule".into(),
            path: graticule.lines,
            bounds: Bounds3D::new_2d(frame.x0, frame.y0, frame.x1, frame.y1),
            fill: None,
            stroke: StrokeBrush::new(AREA_STROKE, 0.006),
        })));
        lines
            .spec
            .lock()
            .expect("graticule spec poisoned")
            .theme_selector = Some("map/graticule".into());

        let style = self.map_label_style();
        // Parallels first: a corner keeps the label of the side read first.
        let mut placed: Vec<Rect> = Vec::new();
        let mut handles = Vec::new();
        let mut labels = graticule.labels;
        labels.sort_by_key(|label| label.side.labels_meridians());
        for label in labels {
            let spec = TextSpec::new(
                vec![label.text.as_str().into()],
                None,
                style.clone(),
                TextFlow::default(),
            )
            .expect("graticule labels are plain text");
            let Ok((width, height)) = self.measure_text_spec(&spec) else {
                continue;
            };
            let (anchor, x, y) = match label.side {
                MapSide::Bottom => (
                    Anchor::BottomLeft,
                    label.at + LABEL_GAP,
                    frame.y0 + LABEL_GAP,
                ),
                MapSide::Top => (Anchor::TopLeft, label.at + LABEL_GAP, frame.y1 - LABEL_GAP),
                MapSide::Left => (
                    Anchor::BottomLeft,
                    frame.x0 + LABEL_GAP,
                    label.at + LABEL_GAP,
                ),
                MapSide::Right => (
                    Anchor::BottomRight,
                    frame.x1 - LABEL_GAP,
                    label.at + LABEL_GAP,
                ),
            };
            // The anchor's offset runs from -1 (left, bottom) to 1.
            let offset = anchor.to_offset();
            let x0 = x - (offset.x + 1.0) / 2.0 * width;
            let y0 = y - (offset.y + 1.0) / 2.0 * height;
            let rect = Rect::new(x0, y0, x0 + width, y0 + height);
            let gap = LABEL_GAP / 2.0;
            let inside = rect.x0 >= frame.x0
                && rect.x1 <= frame.x1 - gap
                && rect.y0 >= frame.y0
                && rect.y1 <= frame.y1 - gap;
            let crowded = placed.iter().any(|other| {
                !other
                    .inflate(LABEL_SPACING, gap)
                    .intersect(rect)
                    .is_zero_area()
            });
            if !inside || crowded {
                continue;
            }
            placed.push(rect);
            // With a theme, its `map/labels` style gives the label its type
            // and color when the scene compiles, as another theme does
            // after a change of theme; without one, the label is muted.
            let handle = if self.theme_style.is_some() {
                let handle = self.text(&label.text);
                handle
                    .spec
                    .lock()
                    .expect("graticule label spec poisoned")
                    .theme_selector = Some("map/labels".into());
                handle
            } else {
                self.text_spec(spec).fill(LABEL_FILL)
            };
            handles.push(handle.at_anchor(x, y, anchor));
        }
        let refs: Vec<&DrawableHandle> = handles.iter().collect();
        let labels = self.group(&refs);
        let group = self.group(&[&lines, &labels]);
        Ok(MapGraticule {
            group,
            lines,
            labels,
        })
    }

    /// The features of the GeoJSON file at `path`, found like other assets.
    pub fn map_geojson_file(
        &mut self,
        map: &GeoMapHandle,
        path: impl AsRef<Path>,
    ) -> Result<MapFeatures, GeoMapError> {
        let path = self.resolve_asset_path(path);
        let text =
            std::fs::read_to_string(&path).map_err(|source| GeoMapError::Read { path, source })?;
        self.map_geojson(map, &text)
    }

    /// A drawable per feature of the GeoJSON document `text` (a feature
    /// collection, a feature or a bare geometry), in its order: polygons
    /// fill, lines stroke, points are dots, and a feature without geometry
    /// is an empty group. Nothing is drawn when any feature is invalid.
    pub fn map_geojson(
        &mut self,
        map: &GeoMapHandle,
        text: &str,
    ) -> Result<MapFeatures, GeoMapError> {
        let document: Value =
            serde_json::from_str(text).map_err(|error| GeoMapError::Json(error.to_string()))?;
        let features = read_document(&document)?;
        let mut drawings = Vec::with_capacity(features.len());
        for (index, feature) in features.iter().enumerate() {
            let drawing = feature
                .geometry
                .as_ref()
                .map(|geometry| project(&map.view, geometry))
                .transpose()
                .map_err(|error| GeoMapError::Feature {
                    feature: index,
                    error,
                })?;
            drawings.push(drawing);
        }
        let mut drawn = Vec::with_capacity(features.len());
        for (feature, drawing) in features.into_iter().zip(drawings) {
            let drawable = match drawing {
                Some(drawing) => self.map_drawing(map, drawing),
                None => self.group(&[]),
            };
            drawn.push(MapFeature {
                drawable,
                id: feature.id,
                properties: feature.properties,
            });
        }
        let refs: Vec<&DrawableHandle> = drawn.iter().map(|feature| &feature.drawable).collect();
        let group = self.group(&refs);
        Ok(MapFeatures {
            group,
            features: drawn,
        })
    }

    fn map_drawing(&mut self, map: &GeoMapHandle, drawing: Drawing) -> DrawableHandle {
        let (path, area) = match drawing {
            Drawing::Area(path) => (path, true),
            Drawing::Line(path) => (path, false),
            Drawing::Points(positions) => {
                let dots = self
                    .points(positions.clone(), POINT_RADIUS)
                    .expect("map points are finite and the radius positive");
                let extent = positions
                    .iter()
                    .fold(
                        Rect::from_points(positions[0], positions[0]),
                        |extent, point| extent.union_pt(*point),
                    )
                    .inflate(POINT_RADIUS, POINT_RADIUS);
                return self.clip_to_map(map, dots, extent);
            }
            Drawing::Collection(drawings) => {
                let parts: Vec<DrawableHandle> = drawings
                    .into_iter()
                    .map(|drawing| self.map_drawing(map, drawing))
                    .collect();
                let refs: Vec<&DrawableHandle> = parts.iter().collect();
                return self.group(&refs);
            }
        };
        // An empty drawing still needs a place: the middle of the frame.
        let extent = if path.elements().is_empty() {
            let center = map.view.frame().center();
            Rect::from_points(center, center)
        } else {
            path.bounding_box()
        };
        let (fill, stroke, name) = if area {
            (
                Some(Brush::Solid(AREA_FILL)),
                StrokeBrush::new(AREA_STROKE, 0.012),
                "MapArea",
            )
        } else {
            (None, StrokeBrush::new(LINE_STROKE, 0.02), "MapLine")
        };
        let handle = self.spawn(SpawnKind::SvgPath(Box::new(SvgPath {
            id: name.into(),
            path,
            bounds: Bounds3D::new_2d(extent.x0, extent.y0, extent.x1, extent.y1),
            fill,
            stroke,
        })));
        handle
            .spec
            .lock()
            .expect("map drawing spec poisoned")
            .theme_selector = Some(if area { "map/area" } else { "map/line" }.into());
        // Neighbors share their border: a centered stroke draws it once, where
        // the default inside stroke would draw each side's half apart.
        let handle = if area {
            handle.stroke_align(StrokeAlign::Center)
        } else {
            handle
        };
        self.clip_to_map(map, handle, extent)
    }

    fn clip_to_map(
        &mut self,
        map: &GeoMapHandle,
        handle: DrawableHandle,
        extent: Rect,
    ) -> DrawableHandle {
        if map.holds(extent) {
            handle
        } else {
            handle.clip(&map.frame, Fill::NonZero)
        }
    }

    /// The type graticule labels take, to measure them as they will draw:
    /// the theme's body text under its `map/labels` style (font, size and
    /// the rest), or without a theme a smaller body text.
    fn map_label_style(&self) -> TextStyle {
        let body = self.themed_text_config().roles[&TextRole::Body].size;
        let Some(theme) = self.theme_style.as_ref() else {
            return TextStyle {
                size: Some(body * 0.8),
                ..TextStyle::default()
            };
        };
        let base = theme
            .text_styles
            .get(&TextRole::Body)
            .cloned()
            .unwrap_or_default();
        match theme
            .styles
            .get("map/labels")
            .and_then(|rule| rule.text.as_ref())
        {
            Some(text) => super::theme::merge_text_style(&base, text),
            None => base,
        }
    }
}

/// Project a geometry's rings and lines; GeoJSON drawings drop points that
/// barely move them.
fn project(view: &MapView, geometry: &Geometry) -> Result<Drawing, GeoError> {
    Ok(match geometry {
        Geometry::Polygons(polygons) => Drawing::Area(view.area_path(polygons, GEOJSON_TOLERANCE)?),
        Geometry::Lines(lines) => Drawing::Line(view.line_path(lines, false, GEOJSON_TOLERANCE)?),
        Geometry::Points(points) if points.is_empty() => Drawing::Collection(Vec::new()),
        Geometry::Points(points) => {
            let mut positions = Vec::with_capacity(points.len());
            for &(lon, lat) in points {
                if !(lon.is_finite() && lat.is_finite()) {
                    return Err(GeoError::NonFinite);
                }
                let point = view.point(lon, lat);
                positions.push((point.x, point.y));
            }
            Drawing::Points(positions)
        }
        Geometry::Collection(geometries) => Drawing::Collection(
            geometries
                .iter()
                .map(|geometry| project(view, geometry))
                .collect::<Result<_, _>>()?,
        ),
    })
}

fn read_document(document: &Value) -> Result<Vec<Feature>, GeoMapError> {
    match document.get("type").and_then(Value::as_str) {
        Some("FeatureCollection") => document
            .get("features")
            .and_then(Value::as_array)
            .ok_or_else(|| shape("a FeatureCollection needs a \"features\" array"))?
            .iter()
            .map(read_feature)
            .collect(),
        Some("Feature") => Ok(vec![read_feature(document)?]),
        Some(_) => Ok(vec![Feature {
            id: None,
            properties: Map::new(),
            geometry: Some(read_geometry(document)?),
        }]),
        None => Err(shape("a GeoJSON object needs a \"type\"")),
    }
}

fn shape(message: &str) -> GeoMapError {
    GeoMapError::Json(message.to_owned())
}

fn read_feature(feature: &Value) -> Result<Feature, GeoMapError> {
    if feature.get("type").and_then(Value::as_str) != Some("Feature") {
        return Err(shape(
            "a FeatureCollection holds objects of type \"Feature\"",
        ));
    }
    let properties = match feature.get("properties") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(properties)) => properties.clone(),
        Some(_) => {
            return Err(shape(
                "a feature's \"properties\" must be an object or null",
            ));
        }
    };
    let geometry = match feature.get("geometry") {
        None | Some(Value::Null) => None,
        Some(geometry) => Some(read_geometry(geometry)?),
    };
    Ok(Feature {
        id: feature.get("id").cloned(),
        properties,
        geometry,
    })
}

fn read_geometry(geometry: &Value) -> Result<Geometry, GeoMapError> {
    let kind = geometry
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| shape("a geometry needs a \"type\""))?;
    if kind == "GeometryCollection" {
        return geometry
            .get("geometries")
            .and_then(Value::as_array)
            .ok_or_else(|| shape("a GeometryCollection needs a \"geometries\" array"))?
            .iter()
            .map(read_geometry)
            .collect::<Result<_, _>>()
            .map(Geometry::Collection);
    }
    let coordinates = geometry
        .get("coordinates")
        .ok_or_else(|| GeoMapError::Json(format!("a {kind} needs \"coordinates\"")))?;
    let positions = |value: &Value| -> Result<GeoLine, GeoMapError> {
        array(value, kind)?
            .iter()
            .map(|value| position(value, kind))
            .collect()
    };
    let polygon = |value: &Value| -> Result<GeoPolygon, GeoMapError> {
        array(value, kind)?.iter().map(positions).collect()
    };
    Ok(match kind {
        "Point" => Geometry::Points(vec![position(coordinates, kind)?]),
        "MultiPoint" => Geometry::Points(positions(coordinates)?),
        "LineString" => Geometry::Lines(vec![positions(coordinates)?]),
        "MultiLineString" => Geometry::Lines(
            array(coordinates, kind)?
                .iter()
                .map(positions)
                .collect::<Result<_, _>>()?,
        ),
        "Polygon" => Geometry::Polygons(vec![polygon(coordinates)?]),
        "MultiPolygon" => Geometry::Polygons(
            array(coordinates, kind)?
                .iter()
                .map(polygon)
                .collect::<Result<_, _>>()?,
        ),
        other => {
            return Err(GeoMapError::Json(format!(
                "unknown geometry type {other:?}"
            )));
        }
    })
}

fn array<'a>(value: &'a Value, kind: &str) -> Result<&'a Vec<Value>, GeoMapError> {
    value
        .as_array()
        .ok_or_else(|| GeoMapError::Json(format!("the coordinates of a {kind} must be arrays")))
}

/// A `[lon, lat]` position; an elevation after them is ignored.
fn position(value: &Value, kind: &str) -> Result<(f64, f64), GeoMapError> {
    let numbers = array(value, kind)?;
    match (
        numbers.first().and_then(Value::as_f64),
        numbers.get(1).and_then(Value::as_f64),
    ) {
        (Some(lon), Some(lat)) => Ok((lon, lat)),
        _ => Err(GeoMapError::Json(format!(
            "a {kind} position must be [longitude, latitude]"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_visualization::{Longitudes, Projection};

    fn map(canvas: &mut SceneModel) -> GeoMapHandle {
        let view = MapView::fit(
            Projection::Equirectangular,
            (-10.0, 10.0),
            Longitudes::Range(-20.0, 20.0),
            Rect::new(-4.0, -2.0, 4.0, 2.0),
        )
        .unwrap();
        canvas.geo_map(view)
    }

    fn path_of(drawable: &DrawableHandle) -> BezPath {
        match &drawable.spec.lock().unwrap().kind {
            SpawnKind::SvgPath(path) => path.path.clone(),
            other => panic!("not a path: {other:?}"),
        }
    }

    fn clip_of(drawable: &DrawableHandle) -> bool {
        let state = drawable.state.lock().unwrap();
        state.active().ops.iter().any(|op| {
            matches!(op, super::super::ops::Op::SetClip { target, mask: Some(_), .. } if *target == drawable.id)
        })
    }

    #[test]
    fn maps_draw_in_scene_units_and_clip_only_what_reaches_past_the_frame() {
        let mut canvas = SceneModel::new(640, 360);
        let map = map(&mut canvas);
        // 0.2 scene units per degree.
        assert_eq!(map.point(10.0, 5.0).unwrap(), DVec2::new(2.0, 1.0));
        assert!(map.point(f64::NAN, 0.0).is_err());
        let square = vec![(-5.0, -5.0), (5.0, -5.0), (5.0, 5.0), (-5.0, 5.0)];
        let inside = canvas.map_polygon(&map, &[vec![square]]).unwrap();
        assert_eq!(
            path_of(&inside).bounding_box(),
            Rect::new(-1.0, -1.0, 1.0, 1.0)
        );
        assert!(!clip_of(&inside));
        let across = canvas
            .map_polyline(&map, &[vec![(-30.0, 0.0), (0.0, 0.0)]], false)
            .unwrap();
        assert!(clip_of(&across));
        assert!(canvas.map_polygon(&map, &[vec![vec![(0.0, 0.0)]]]).is_err());

        let theme =
            |drawable: &DrawableHandle| drawable.spec.lock().unwrap().theme_selector.clone();
        assert_eq!(theme(&inside).as_deref(), Some("map/area"));
        assert_eq!(theme(&across).as_deref(), Some("map/line"));
        assert_eq!(theme(map.frame()).as_deref(), Some("map/frame"));
    }

    #[test]
    fn geojson_features_draw_in_order_and_fail_before_drawing_anything() {
        let mut canvas = SceneModel::new(640, 360);
        let map = map(&mut canvas);
        let document = r#"{
            "type": "FeatureCollection",
            "features": [
                {"type": "Feature", "id": 7, "properties": {"name": "Box"},
                 "geometry": {"type": "Polygon", "coordinates": [[[-5, -5], [5, -5], [5, 5], [-5, 5], [-5, -5]]]}},
                {"type": "Feature", "properties": {"name": "Road"},
                 "geometry": {"type": "LineString", "coordinates": [[-15, 0, 120], [15, 0, 80]]}},
                {"type": "Feature", "properties": {"name": "Towns"},
                 "geometry": {"type": "MultiPoint", "coordinates": [[0, 0], [1, 1]]}},
                {"type": "Feature", "properties": null, "geometry": null}
            ]
        }"#;
        let features = canvas.map_geojson(&map, document).unwrap();
        assert_eq!(features.features.len(), 4);
        assert_eq!(
            path_of(&features.features[0].drawable).bounding_box(),
            Rect::new(-1.0, -1.0, 1.0, 1.0)
        );
        assert!(matches!(
            features.features[2].drawable.spec.lock().unwrap().kind,
            SpawnKind::Points { .. }
        ));
        assert!(matches!(
            features.features[3].drawable.spec.lock().unwrap().kind,
            SpawnKind::Group(_)
        ));
        assert_eq!(features.features[0].id, Some(Value::from(7)));
        assert!(matches!(
            features.keys("name"),
            Err(GeoMapError::MissingKey { feature: 3, .. })
        ));
        let two = MapFeatures {
            group: features.group.clone(),
            features: features.features[..2].to_vec(),
        };
        assert_eq!(two.keys("name").unwrap(), ["Box", "Road"]);
        assert!(matches!(
            two.keys("id"),
            Err(GeoMapError::MissingKey { feature: 1, .. })
        ));
        let mut twins = two.clone();
        twins.features[1].properties = twins.features[0].properties.clone();
        assert!(matches!(
            twins.keys("name"),
            Err(GeoMapError::DuplicateKey {
                first: 0,
                second: 1,
                ..
            })
        ));

        let specs = canvas.state.lock().unwrap().all_drawables.len();
        let broken = r#"{"type": "FeatureCollection", "features": [
            {"type": "Feature", "properties": {}, "geometry": {"type": "Point", "coordinates": [0, 0]}},
            {"type": "Feature", "properties": {}, "geometry": {"type": "Polygon", "coordinates": [[[0, 0], [1, 1]]]}}
        ]}"#;
        assert!(matches!(
            canvas.map_geojson(&map, broken),
            Err(GeoMapError::Feature {
                feature: 1,
                error: GeoError::ShortRing
            })
        ));
        assert_eq!(canvas.state.lock().unwrap().all_drawables.len(), specs);
        for bad in [
            "[1, 2]",
            r#"{"type": "Polygon"}"#,
            r#"{"type": "Hexagon", "coordinates": []}"#,
            r#"{"type": "Point", "coordinates": ["a", 0]}"#,
            r#"{"type": "FeatureCollection", "features": [{"type": "Point", "coordinates": [0, 0]}]}"#,
            "not json",
        ] {
            assert!(canvas.map_geojson(&map, bad).is_err(), "{bad}");
        }
        // A bare geometry is one feature without properties.
        let bare = canvas
            .map_geojson(&map, r#"{"type": "Point", "coordinates": [0, 0]}"#)
            .unwrap();
        assert_eq!(bare.features.len(), 1);
    }

    #[test]
    fn graticule_labels_stay_inside_the_frame_without_covering_each_other() {
        let mut canvas = SceneModel::new(640, 360);
        let map = map(&mut canvas);
        let count = |graticule: &MapGraticule| match &graticule.labels.spec.lock().unwrap().kind {
            SpawnKind::Group(labels) => labels.len(),
            other => panic!("labels are a group, not {other:?}"),
        };
        let mut options = GraticuleOptions::new((5.0, 5.0));
        options.sides = vec![MapSide::Left, MapSide::Bottom, MapSide::Right];
        // A meridian a scene unit apart: each of the 7 meridians and the 3
        // parallels on both sides has room.
        let graticule = canvas.map_graticule(&map, &options).unwrap();
        assert_eq!(count(&graticule), 7 + 3 * 2);
        // A meridian every 0.2 units leaves no room for every label.
        options.step = (1.0, 1.0);
        options.every = (1.0, 1.0);
        let dense = canvas.map_graticule(&map, &options).unwrap();
        assert!(
            (1..39 + 19 * 2).contains(&count(&dense)),
            "{}",
            count(&dense)
        );
        options.sides.clear();
        assert_eq!(count(&canvas.map_graticule(&map, &options).unwrap()), 0);
        assert_eq!(
            graticule
                .lines
                .spec
                .lock()
                .unwrap()
                .theme_selector
                .as_deref(),
            Some("map/graticule")
        );
    }

    /// The labels of a graticule, as the theme resolves them.
    fn resolved_labels(
        canvas: &SceneModel,
        graticule: &MapGraticule,
    ) -> Vec<super::super::ObjectSpec> {
        let SpawnKind::Group(ids) = graticule.labels.spec.lock().unwrap().kind.clone() else {
            panic!("labels are a group");
        };
        let state = canvas.state.lock().unwrap();
        let theme = canvas.theme_style.as_ref().unwrap();
        ids.iter()
            .map(|id| {
                let spec = state.object_specs[id].lock().unwrap().clone();
                theme.resolve_object(&spec).unwrap()
            })
            .collect()
    }

    #[test]
    fn graticule_labels_take_their_type_and_color_from_the_theme() {
        let red = Color::from_rgb8(0xFF, 0x00, 0x00);
        let mut canvas = SceneModel::new(16.0, 9.0);
        let mut theme = crate::canvas::CanvasTheme::builtin("paper").unwrap();
        theme
            .set_styles(&std::collections::HashMap::from([(
                "map/labels".to_string(),
                crate::canvas::ThemeStyle {
                    text: Some(TextStyle {
                        font: Some("DejaVu Sans Mono".into()),
                        size: Some(0.4),
                        color: Some(red),
                        ..TextStyle::default()
                    }),
                    ..Default::default()
                },
            )]))
            .unwrap();
        canvas.apply_theme(theme);
        let map = map(&mut canvas);
        let graticule = canvas
            .map_graticule(&map, &GraticuleOptions::new((5.0, 5.0)))
            .unwrap();
        let labels = resolved_labels(&canvas, &graticule);
        assert!(!labels.is_empty());
        for label in &labels {
            let SpawnKind::Text(text) = &label.kind else {
                panic!("a label is a text");
            };
            assert_eq!(text.style.font.as_deref(), Some("DejaVu Sans Mono"));
            assert_eq!(text.style.size, Some(0.4));
            assert_eq!(text.style.color, Some(red));
            // No fill of its own covers the rule's color.
            assert!(!label.fill_overridden);
        }

        let mut world = bevy::prelude::World::new();
        world.insert_resource(gaanim_timeline::timeline::Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        let mut glyphs = world.query::<(
            &gaanim_scene::components::TextSpan,
            &gaanim_scene::FillBrush,
        )>();
        let colors: Vec<Color> = glyphs
            .iter(&world)
            .filter_map(|(_, fill)| match fill.0.as_ref() {
                Some(Brush::Solid(color)) => Some(*color),
                _ => None,
            })
            .collect();
        assert!(
            !colors.is_empty() && colors.iter().all(|color| *color == red),
            "{colors:?}"
        );
    }

    #[test]
    fn graticule_labels_are_muted_at_caption_size_by_default() {
        let mut canvas = SceneModel::new(16.0, 9.0);
        canvas.apply_theme(crate::canvas::CanvasTheme::builtin("paper").unwrap());
        let map = map(&mut canvas);
        let graticule = canvas
            .map_graticule(&map, &GraticuleOptions::new((5.0, 5.0)))
            .unwrap();
        let theme = canvas.theme_style.clone().unwrap();
        for label in resolved_labels(&canvas, &graticule) {
            let SpawnKind::Text(text) = &label.kind else {
                panic!("a label is a text");
            };
            assert_eq!(
                text.style.size,
                Some(theme.text.roles[&TextRole::Caption].size)
            );
            assert_eq!(label.fill, Some(Brush::Solid(theme.palette.muted)));
        }
    }
}
