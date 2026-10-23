//! Maps: longitude and latitude laid on a rectangle of the scene.
//!
//! A [`MapView`] is a window of longitude and latitude, a [`Projection`]
//! and the rectangle of the scene the window fills. Both projections keep
//! longitude and latitude apart (a meridian is a vertical line and a
//! parallel a horizontal one) and use one scale for both, so the map keeps
//! its proportions: a degree of longitude is as wide as a degree of
//! latitude is tall at the equator.

use gaanim_core::kurbo::{BezPath, Point, Rect};

/// Latitude where a Mercator map of the whole world is square. Mercator
/// frames stop there.
pub const MERCATOR_MAX_LATITUDE: f64 = 85.051_128_779_806_59;

/// Latitude data is clamped to on a Mercator map, whose poles lie infinitely
/// far away. It lies far outside any frame, where drawings are clipped.
const MERCATOR_DATA_LATITUDE: f64 = 89.9;

/// Room past the frame within which a ring still counts as shown: a stroke
/// on a ring just outside the frame reaches into it.
const SHOWN_MARGIN: f64 = 0.05;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GeoError {
    #[error(
        "map latitudes must be two different finite values between -{max} and {max}",
        max = degrees(*.0)
    )]
    InvalidLatitudes(f64),
    #[error("map longitudes must be two finite values, west below east, at most 360° apart")]
    InvalidLongitudes,
    #[error("the map's region must have a finite, positive width and height")]
    InvalidRegion,
    #[error(
        "the map would span {0:.0}° of longitude, more than 360°; give lon=(west, east) or a taller region"
    )]
    TooWide(f64),
    #[error("map coordinates must be finite")]
    NonFinite,
    #[error("a polygon ring needs at least 3 points")]
    ShortRing,
    #[error("a line needs at least 2 points")]
    ShortLine,
    #[error("graticule steps must be finite and positive")]
    InvalidStep,
    #[error("hemispheres must be four letters: north, south, east and west")]
    InvalidHemispheres,
}

/// How a map flattens longitude and latitude.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Projection {
    /// Conformal: shapes keep their angles, and areas grow toward the poles.
    #[default]
    Mercator,
    /// Plate carrée: latitude is as linear as longitude.
    Equirectangular,
}

impl Projection {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "mercator" => Some(Self::Mercator),
            "equirectangular" => Some(Self::Equirectangular),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Mercator => "mercator",
            Self::Equirectangular => "equirectangular",
        }
    }

    /// The latitude a frame may reach on either side of the equator.
    pub fn max_latitude(self) -> f64 {
        match self {
            Self::Mercator => MERCATOR_MAX_LATITUDE,
            Self::Equirectangular => 90.0,
        }
    }

    /// The height of latitude `lat` above the equator, in degrees of
    /// longitude.
    pub fn ordinate(self, lat: f64) -> f64 {
        match self {
            Self::Mercator => lat
                .clamp(-MERCATOR_DATA_LATITUDE, MERCATOR_DATA_LATITUDE)
                .to_radians()
                .tan()
                .asinh()
                .to_degrees(),
            Self::Equirectangular => lat,
        }
    }

    /// The latitude drawn at `ordinate`.
    pub fn latitude(self, ordinate: f64) -> f64 {
        match self {
            Self::Mercator => ordinate.to_radians().sinh().atan().to_degrees(),
            Self::Equirectangular => ordinate,
        }
    }
}

/// The longitudes a map shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Longitudes {
    /// The west and east edges.
    Range(f64, f64),
    /// The longitude in the middle; the region's width sets the edges.
    Center(f64),
}

/// A side of a map's frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapSide {
    Left,
    Right,
    Top,
    Bottom,
}

impl MapSide {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "top" => Some(Self::Top),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }

    /// Whether the side labels meridians (top and bottom) rather than
    /// parallels.
    pub fn labels_meridians(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }
}

/// The meridians and parallels a [`MapView::graticule`] draws, and which it
/// labels.
#[derive(Debug, Clone, PartialEq)]
pub struct GraticuleOptions {
    /// Degrees between meridians and between parallels.
    pub step: (f64, f64),
    /// Degrees between labeled meridians and between labeled parallels:
    /// lines at multiples of these are labeled.
    pub every: (f64, f64),
    /// The sides that carry labels.
    pub sides: Vec<MapSide>,
    /// The letters of north, south, east and west.
    pub hemispheres: [char; 4],
}

impl GraticuleOptions {
    /// Lines every `step` degrees, each labeled, on the left and bottom
    /// sides, with the letters N, S, E and W.
    pub fn new(step: (f64, f64)) -> Self {
        Self {
            step,
            every: step,
            sides: vec![MapSide::Left, MapSide::Bottom],
            hemispheres: ['N', 'S', 'E', 'W'],
        }
    }

    /// The four letters of `letters`, in the order north, south, east and
    /// west.
    pub fn parse_hemispheres(letters: &str) -> Result<[char; 4], GeoError> {
        let letters: Vec<char> = letters.chars().collect();
        letters
            .try_into()
            .ok()
            .filter(|letters: &[char; 4]| letters.iter().all(|letter| !letter.is_whitespace()))
            .ok_or(GeoError::InvalidHemispheres)
    }
}

/// The label of a meridian or a parallel, where its line meets a side.
#[derive(Debug, Clone, PartialEq)]
pub struct GraticuleLabel {
    pub text: String,
    pub side: MapSide,
    /// Where the line meets the side: its x for the top and bottom sides,
    /// its y for the left and right ones.
    pub at: f64,
}

/// The lines of a graticule, as one path, and their labels.
#[derive(Debug, Clone)]
pub struct Graticule {
    /// The meridians, then the parallels, each across the frame.
    pub lines: BezPath,
    pub labels: Vec<GraticuleLabel>,
}

/// A ring or line of `(lon, lat)` points.
pub type GeoLine = Vec<(f64, f64)>;

/// A polygon: its outer ring, then rings that cut holes in it or add parts
/// apart from it.
pub type GeoPolygon = Vec<GeoLine>;

/// A window of longitude and latitude laid on a rectangle of the scene.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapView {
    projection: Projection,
    west: f64,
    east: f64,
    south: f64,
    north: f64,
    /// The rectangle the window fills, in scene units with y up.
    frame: Rect,
}

impl MapView {
    /// The map of the latitudes `lat` and the longitudes `lon` that fits
    /// `region`. Given its edges, the map is as large as fits and centered;
    /// given its middle longitude, the map fills the region, its latitudes
    /// fix the scale, and the region's width sets the other longitudes.
    pub fn fit(
        projection: Projection,
        lat: (f64, f64),
        lon: Longitudes,
        region: Rect,
    ) -> Result<Self, GeoError> {
        let max = projection.max_latitude();
        let (south, north) = (lat.0.min(lat.1), lat.0.max(lat.1));
        if !(lat.0.is_finite() && lat.1.is_finite())
            || south == north
            || south < -max
            || north > max
        {
            return Err(GeoError::InvalidLatitudes(max));
        }
        let finite = [region.x0, region.y0, region.x1, region.y1]
            .iter()
            .all(|value| value.is_finite());
        if !finite || region.width() <= 0.0 || region.height() <= 0.0 {
            return Err(GeoError::InvalidRegion);
        }
        let region = region.abs();
        let tall = projection.ordinate(north) - projection.ordinate(south);
        let (west, east, frame) = match lon {
            Longitudes::Range(west, east) => {
                if !(west.is_finite() && east.is_finite()) || west >= east || east - west > 360.0 {
                    return Err(GeoError::InvalidLongitudes);
                }
                let wide = east - west;
                let scale = (region.width() / wide).min(region.height() / tall);
                let frame = Rect::from_center_size(region.center(), (wide * scale, tall * scale));
                (west, east, frame)
            }
            Longitudes::Center(center) => {
                if !center.is_finite() {
                    return Err(GeoError::InvalidLongitudes);
                }
                let wide = region.width() * tall / region.height();
                if wide > 360.0 * (1.0 + 1e-9) {
                    return Err(GeoError::TooWide(wide));
                }
                (center - wide / 2.0, center + wide / 2.0, region)
            }
        };
        Ok(Self {
            projection,
            west,
            east,
            south,
            north,
            frame,
        })
    }

    pub fn projection(&self) -> Projection {
        self.projection
    }

    /// The west and east edges, in degrees.
    pub fn longitudes(&self) -> (f64, f64) {
        (self.west, self.east)
    }

    /// The south and north edges, in degrees.
    pub fn latitudes(&self) -> (f64, f64) {
        (self.south, self.north)
    }

    /// The rectangle of the scene the map fills.
    pub fn frame(&self) -> Rect {
        self.frame
    }

    /// Scene units per degree of longitude.
    pub fn scale(&self) -> f64 {
        self.frame.width() / (self.east - self.west)
    }

    /// `lon` moved by whole turns to the copy nearest the middle of the map.
    pub fn wrap(&self, lon: f64) -> f64 {
        let middle = (self.west + self.east) / 2.0;
        lon + 360.0 * ((middle - lon) / 360.0).round()
    }

    /// Where `(lon, lat)` is drawn, with the copy of `lon` nearest the
    /// middle of the map.
    pub fn point(&self, lon: f64, lat: f64) -> Point {
        self.project(self.wrap(lon), lat)
    }

    /// Where `(lon, lat)` is drawn, with `lon` as it is.
    fn project(&self, lon: f64, lat: f64) -> Point {
        let scale = self.scale();
        let base = self.projection.ordinate(self.south);
        Point::new(
            self.frame.x0 + (lon - self.west) * scale,
            self.frame.y0 + (self.projection.ordinate(lat) - base) * scale,
        )
    }

    /// One path through every ring of `polygons`.
    ///
    /// Each polygon's first ring winds counterclockwise and the others
    /// clockwise, whichever way they were given, so under the non-zero rule a
    /// later ring inside the first cuts a hole in it and one apart from it is
    /// filled. See [`Self::line_path`] for rings that cross the antimeridian
    /// or lie outside the map, and for `tolerance`.
    pub fn area_path(&self, polygons: &[GeoPolygon], tolerance: f64) -> Result<BezPath, GeoError> {
        let mut path = BezPath::new();
        for polygon in polygons {
            for (index, ring) in polygon.iter().enumerate() {
                let mut ring = open_ring(ring)?;
                unwrap_longitudes(&mut ring);
                // The projections keep the order of longitudes and of
                // latitudes, so a ring winds on the map as it does in degrees.
                if (signed_area(&ring) > 0.0) != (index == 0) {
                    ring.reverse();
                }
                self.trace(&mut path, &ring, true, tolerance);
            }
        }
        Ok(path)
    }

    /// One path through every line of `lines`, joining each one's last point
    /// to its first when `closed`.
    ///
    /// A line that crosses the antimeridian stays whole: each point takes the
    /// copy of its longitude nearest the point before. It is drawn once for
    /// each whole turn that brings it into the map, so a coast past 180° east
    /// shows at the west edge too, and left out where the map never shows it.
    /// A positive `tolerance`, in scene units, drops points that move the
    /// drawn line less than that.
    pub fn line_path(
        &self,
        lines: &[GeoLine],
        closed: bool,
        tolerance: f64,
    ) -> Result<BezPath, GeoError> {
        let mut path = BezPath::new();
        for line in lines {
            let mut line = if closed {
                open_ring(line)?
            } else {
                if line.len() < 2 {
                    return Err(GeoError::ShortLine);
                }
                check_finite(line)?;
                line.clone()
            };
            unwrap_longitudes(&mut line);
            self.trace(&mut path, &line, closed, tolerance);
        }
        Ok(path)
    }

    /// Add to `path` each copy of `line`, whole turns apart, that the map
    /// shows.
    fn trace(&self, path: &mut BezPath, line: &[(f64, f64)], closed: bool, tolerance: f64) {
        let (low, high) = line.iter().fold(
            (f64::INFINITY, f64::NEG_INFINITY),
            |(low, high), &(lon, _)| (low.min(lon), high.max(lon)),
        );
        let first = ((self.west - high) / 360.0).ceil() as i64;
        let last = ((self.east - low) / 360.0).floor() as i64;
        let shown = self.frame.inflate(SHOWN_MARGIN, SHOWN_MARGIN);
        for turn in first..=last {
            let shift = 360.0 * turn as f64;
            let points: Vec<Point> = line
                .iter()
                .map(|&(lon, lat)| self.project(lon + shift, lat))
                .collect();
            let bounds = points
                .iter()
                .fold(Rect::from_points(points[0], points[0]), |bounds, point| {
                    bounds.union_pt(*point)
                });
            if bounds.intersect(shown).is_zero_area() && !touches(bounds, shown) {
                continue;
            }
            let points = if tolerance > 0.0 {
                simplify(&points, closed, tolerance)
            } else {
                points
            };
            if points.len() < if closed { 3 } else { 2 } {
                continue;
            }
            path.move_to(points[0]);
            for point in &points[1..] {
                path.line_to(*point);
            }
            if closed {
                path.close_path();
            }
        }
    }

    /// A step for the graticule: a round number of degrees that draws about
    /// eight lines across the longer side of the map.
    pub fn graticule_step(&self) -> f64 {
        const STEPS: [f64; 16] = [
            0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 20.0, 30.0, 45.0, 60.0,
            90.0,
        ];
        let span = (self.east - self.west).max(self.north - self.south);
        let rough = span / 8.0;
        STEPS
            .into_iter()
            .min_by(|a, b| (a / rough).ln().abs().total_cmp(&(b / rough).ln().abs()))
            .unwrap_or(10.0)
    }

    /// Meridians and parallels every `options.step` degrees across the
    /// frame, and labels for those at multiples of `options.every` on the
    /// sides `options.sides`. Lines on the frame's edges are left out.
    pub fn graticule(&self, options: &GraticuleOptions) -> Result<Graticule, GeoError> {
        let positive = |value: f64| value.is_finite() && value > 0.0;
        if ![
            options.step.0,
            options.step.1,
            options.every.0,
            options.every.1,
        ]
        .into_iter()
        .all(positive)
        {
            return Err(GeoError::InvalidStep);
        }
        let mut lines = BezPath::new();
        let mut labels = Vec::new();
        let [north, south, east, west] = options.hemispheres;
        for lon in multiples(self.west, self.east, options.step.0) {
            let x = self.project(lon, self.south).x;
            lines.move_to((x, self.frame.y1));
            lines.line_to((x, self.frame.y0));
            if is_multiple(lon, options.every.0) {
                let text = longitude_label(lon, east, west);
                for side in options.sides.iter().filter(|side| side.labels_meridians()) {
                    labels.push(GraticuleLabel {
                        text: text.clone(),
                        side: *side,
                        at: x,
                    });
                }
            }
        }
        for lat in multiples(self.south, self.north, options.step.1) {
            let y = self.project(self.west, lat).y;
            lines.move_to((self.frame.x0, y));
            lines.line_to((self.frame.x1, y));
            if is_multiple(lat, options.every.1) {
                let text = latitude_label(lat, north, south);
                for side in options.sides.iter().filter(|side| !side.labels_meridians()) {
                    labels.push(GraticuleLabel {
                        text: text.clone(),
                        side: *side,
                        at: y,
                    });
                }
            }
        }
        Ok(Graticule { lines, labels })
    }
}

/// Whether two rectangles share an edge or a corner, which a zero-area
/// intersection does not tell apart from lying apart.
fn touches(a: Rect, b: Rect) -> bool {
    a.x0 <= b.x1 && b.x0 <= a.x1 && a.y0 <= b.y1 && b.y0 <= a.y1
}

fn check_finite(points: &[(f64, f64)]) -> Result<(), GeoError> {
    if points
        .iter()
        .all(|(lon, lat)| lon.is_finite() && lat.is_finite())
    {
        Ok(())
    } else {
        Err(GeoError::NonFinite)
    }
}

/// `ring` without the copy of its first point that closes it, as GeoJSON
/// gives rings.
fn open_ring(ring: &[(f64, f64)]) -> Result<GeoLine, GeoError> {
    check_finite(ring)?;
    let mut ring = ring.to_vec();
    if ring.len() > 1 && ring.first() == ring.last() {
        ring.pop();
    }
    if ring.len() < 3 {
        return Err(GeoError::ShortRing);
    }
    Ok(ring)
}

/// Move each longitude by whole turns to the copy nearest the one before,
/// so a line that crosses the antimeridian runs on instead of jumping back.
fn unwrap_longitudes(line: &mut [(f64, f64)]) {
    for index in 1..line.len() {
        let previous = line[index - 1].0;
        let lon = &mut line[index].0;
        *lon += 360.0 * ((previous - *lon) / 360.0).round();
    }
}

/// Twice the area a ring encloses, positive when it winds counterclockwise.
fn signed_area(ring: &[(f64, f64)]) -> f64 {
    let count = ring.len();
    (0..count)
        .map(|index| {
            let (x0, y0) = ring[index];
            let (x1, y1) = ring[(index + 1) % count];
            x0 * y1 - x1 * y0
        })
        .sum()
}

/// The points of `points` that move the line by `tolerance` or more
/// (Douglas–Peucker), keeping the ends. A closed ring keeps its first point
/// and the point farthest from it.
fn simplify(points: &[Point], closed: bool, tolerance: f64) -> Vec<Point> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    let mut spans = Vec::new();
    if closed {
        let farthest = (1..points.len())
            .max_by(|a, b| {
                points[*a]
                    .distance_squared(points[0])
                    .total_cmp(&points[*b].distance_squared(points[0]))
            })
            .unwrap_or(1);
        keep[0] = true;
        keep[farthest] = true;
        spans.push((0, farthest));
        spans.push((farthest, points.len()));
    } else {
        keep[0] = true;
        keep[points.len() - 1] = true;
        spans.push((0, points.len() - 1));
    }
    while let Some((start, end)) = spans.pop() {
        // A closed ring's last span ends back at its first point.
        let end_point = points[end % points.len()];
        let mut far = (0.0, start);
        for index in start + 1..end {
            let distance = segment_distance(points[index], points[start], end_point);
            if distance > far.0 {
                far = (distance, index);
            }
        }
        if far.0 >= tolerance {
            keep[far.1] = true;
            spans.push((start, far.1));
            spans.push((far.1, end));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(point, keep)| keep.then_some(*point))
        .collect()
}

fn segment_distance(point: Point, start: Point, end: Point) -> f64 {
    let along = end - start;
    let length = along.hypot2();
    if length <= f64::EPSILON {
        return point.distance(start);
    }
    let t = ((point - start).dot(along) / length).clamp(0.0, 1.0);
    point.distance(start + along * t)
}

/// The multiples of `step` strictly between `low` and `high`.
fn multiples(low: f64, high: f64, step: f64) -> impl Iterator<Item = f64> {
    let slack = step * 1e-9;
    let first = ((low + slack) / step).ceil() as i64;
    let last = ((high - slack) / step).floor() as i64;
    // Each value is a whole multiple, not a running sum, so 0.1 steps land
    // on round numbers.
    (first..=last).map(move |index| index as f64 * step)
}

fn is_multiple(value: f64, step: f64) -> bool {
    let ratio = value / step;
    (ratio - ratio.round()).abs() < 1e-6
}

/// `value` in degrees: whole degrees without decimals, others with as many
/// as they need, up to six.
fn degrees(value: f64) -> String {
    let value = value.abs();
    if (value - value.round()).abs() < 1e-9 {
        format!("{:.0}°", value.round())
    } else {
        let text = format!("{value:.6}");
        format!("{}°", text.trim_end_matches('0').trim_end_matches('.'))
    }
}

fn latitude_label(lat: f64, north: char, south: char) -> String {
    if lat.abs() < 1e-9 {
        return "0°".to_owned();
    }
    let letter = if lat > 0.0 { north } else { south };
    format!("{}{letter}", degrees(lat))
}

/// The label of the meridian at `lon`, read as a longitude between 180° W
/// and 180° E whatever turn it is drawn on.
fn longitude_label(lon: f64, east: char, west: char) -> String {
    let lon = lon - 360.0 * (lon / 360.0).round();
    if lon.abs() < 1e-9 {
        return "0°".to_owned();
    }
    if (lon.abs() - 180.0).abs() < 1e-9 {
        return "180°".to_owned();
    }
    let letter = if lon > 0.0 { east } else { west };
    format!("{}{letter}", degrees(lon))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn mercator_ordinates_round_trip_and_stretch_toward_the_poles() {
        let mercator = Projection::Mercator;
        for lat in [-80.0, -19.0, 0.0, 1.0, 45.0, 85.0] {
            assert!(near(mercator.latitude(mercator.ordinate(lat)), lat));
        }
        assert_eq!(mercator.ordinate(0.0), 0.0);
        // Near the equator a degree of latitude is a degree of longitude;
        // at 60° it is twice as tall.
        assert!((mercator.ordinate(1.0) - 1.0).abs() < 1e-4);
        let at_60 = mercator.ordinate(60.5) - mercator.ordinate(59.5);
        assert!((at_60 - 2.0).abs() < 1e-3, "{at_60}");
        // The world is square at the frame limit, and data past it is clamped.
        assert!((mercator.ordinate(MERCATOR_MAX_LATITUDE) - 180.0).abs() < 1e-9);
        assert_eq!(
            mercator.ordinate(90.0),
            mercator.ordinate(MERCATOR_DATA_LATITUDE)
        );
        assert!(mercator.ordinate(-90.0).is_finite());
        assert_eq!(Projection::Equirectangular.ordinate(37.5), 37.5);
        assert_eq!(Projection::from_name("mercator"), Some(mercator));
        assert_eq!(Projection::from_name("robinson"), None);
    }

    #[test]
    fn a_map_fits_its_region_with_one_scale_for_both_directions() {
        let region = Rect::new(-7.0, -3.0, 7.0, 3.0);
        // Given the middle longitude, the latitudes fix the scale and the
        // map fills the region.
        let map = MapView::fit(
            Projection::Mercator,
            (1.0, -19.0),
            Longitudes::Center(-80.8),
            region,
        )
        .unwrap();
        assert_eq!(map.frame(), region);
        assert_eq!(map.latitudes(), (-19.0, 1.0));
        let (west, east) = map.longitudes();
        assert!(near((west + east) / 2.0, -80.8));
        let tall = Projection::Mercator.ordinate(1.0) - Projection::Mercator.ordinate(-19.0);
        assert!(near(map.scale(), 6.0 / tall));
        assert!(near(map.point(-80.8, 1.0).y, 3.0));
        assert!(near(map.point(-80.8, -19.0).y, -3.0));
        assert!(near(map.point(-80.8, 0.0).x, 0.0));
        assert!(near(map.point(west, 0.0).x, -7.0));

        // Given both edges, the map is as large as fits, centered.
        let map = MapView::fit(
            Projection::Equirectangular,
            (0.0, 10.0),
            Longitudes::Range(0.0, 10.0),
            region,
        )
        .unwrap();
        assert_eq!(map.frame(), Rect::new(-3.0, -3.0, 3.0, 3.0));
        assert!(near(map.scale(), 0.6));
        let corner = map.point(10.0, 10.0);
        assert!(near(corner.x, 3.0) && near(corner.y, 3.0));
    }

    #[test]
    fn maps_reject_frames_they_cannot_draw() {
        let region = Rect::new(0.0, 0.0, 16.0, 9.0);
        let fit = |projection, lat, lon| MapView::fit(projection, lat, lon, region);
        assert_eq!(
            fit(Projection::Mercator, (0.0, 0.0), Longitudes::Center(0.0)),
            Err(GeoError::InvalidLatitudes(MERCATOR_MAX_LATITUDE))
        );
        assert!(fit(Projection::Mercator, (-89.0, 0.0), Longitudes::Center(0.0)).is_err());
        assert!(
            fit(
                Projection::Equirectangular,
                (-89.0, 0.0),
                Longitudes::Center(0.0)
            )
            .is_ok()
        );
        assert!(
            fit(
                Projection::Mercator,
                (f64::NAN, 0.0),
                Longitudes::Center(0.0)
            )
            .is_err()
        );
        assert_eq!(
            fit(
                Projection::Mercator,
                (0.0, 1.0),
                Longitudes::Range(5.0, 5.0)
            ),
            Err(GeoError::InvalidLongitudes)
        );
        assert!(
            fit(
                Projection::Mercator,
                (0.0, 1.0),
                Longitudes::Range(0.0, 361.0)
            )
            .is_err()
        );
        assert!(
            fit(
                Projection::Mercator,
                (0.0, 1.0),
                Longitudes::Range(160.0, 200.0)
            )
            .is_ok()
        );
        // A thin band of latitude in a wide region would go round the
        // world more than once.
        assert!(matches!(
            fit(Projection::Mercator, (-80.0, 80.0), Longitudes::Center(0.0)),
            Err(GeoError::TooWide(_))
        ));
        assert_eq!(
            MapView::fit(
                Projection::Mercator,
                (0.0, 1.0),
                Longitudes::Center(0.0),
                Rect::new(0.0, 0.0, 0.0, 1.0)
            ),
            Err(GeoError::InvalidRegion)
        );
    }

    fn world() -> MapView {
        MapView::fit(
            Projection::Equirectangular,
            (-90.0, 90.0),
            Longitudes::Range(-180.0, 180.0),
            Rect::new(-180.0, -90.0, 180.0, 90.0),
        )
        .unwrap()
    }

    fn subpaths(path: &BezPath) -> Vec<Vec<(f64, f64)>> {
        let mut rings: Vec<Vec<(f64, f64)>> = Vec::new();
        for element in path.elements() {
            match element {
                gaanim_core::kurbo::PathEl::MoveTo(point) => rings.push(vec![(point.x, point.y)]),
                gaanim_core::kurbo::PathEl::LineTo(point) => {
                    rings.last_mut().unwrap().push((point.x, point.y));
                }
                _ => {}
            }
        }
        rings
    }

    #[test]
    fn longitudes_take_the_copy_nearest_the_middle_of_the_map() {
        let pacific = MapView::fit(
            Projection::Equirectangular,
            (-10.0, 10.0),
            Longitudes::Range(160.0, 200.0),
            Rect::new(0.0, 0.0, 40.0, 20.0),
        )
        .unwrap();
        assert!(near(pacific.point(-170.0, 0.0).x, 30.0));
        assert!(near(pacific.point(190.0, 0.0).x, 30.0));
        assert!(near(pacific.point(170.0, 0.0).x, 10.0));
        assert!(near(pacific.wrap(-170.0), 190.0));
    }

    #[test]
    fn polygons_wind_their_outer_ring_counterclockwise_and_the_rest_clockwise() {
        let map = world();
        let square = |x: f64, y: f64, size: f64| -> GeoLine {
            vec![
                (x, y),
                (x, y + size),
                (x + size, y + size),
                (x + size, y),
                (x, y),
            ]
        };
        // Given clockwise, with a closing point and a hole given counterclockwise.
        let mut hole = square(2.0, 2.0, 2.0);
        hole.reverse();
        let path = map
            .area_path(&[vec![square(0.0, 0.0, 6.0), hole]], 0.0)
            .unwrap();
        let rings = subpaths(&path);
        assert_eq!(rings.len(), 2);
        assert_eq!(rings[0].len(), 4, "the closing point is dropped");
        assert!(signed_area(&rings[0]) > 0.0);
        assert!(signed_area(&rings[1]) < 0.0);
        assert_eq!(
            map.area_path(&[vec![vec![(0.0, 0.0), (1.0, 1.0)]]], 0.0),
            Err(GeoError::ShortRing)
        );
        assert_eq!(
            map.line_path(&[vec![(0.0, f64::NAN), (1.0, 1.0)]], false, 0.0),
            Err(GeoError::NonFinite)
        );
        assert_eq!(
            map.line_path(&[vec![(0.0, 0.0)]], false, 0.0),
            Err(GeoError::ShortLine)
        );
    }

    #[test]
    fn a_ring_across_the_antimeridian_shows_on_both_edges() {
        let map = world();
        // An island from 170° E to 170° W.
        let island = vec![(170.0, -5.0), (-170.0, -5.0), (-170.0, 5.0), (170.0, 5.0)];
        let rings = subpaths(&map.area_path(&[vec![island]], 0.0).unwrap());
        assert_eq!(rings.len(), 2, "{rings:?}");
        let spans: Vec<(f64, f64)> = rings
            .iter()
            .map(|ring| {
                ring.iter().fold(
                    (f64::INFINITY, f64::NEG_INFINITY),
                    |(low, high), &(x, _)| (low.min(x), high.max(x)),
                )
            })
            .collect();
        assert!(spans.contains(&(170.0, 190.0)), "{spans:?}");
        assert!(spans.contains(&(-190.0, -170.0)), "{spans:?}");

        // A ring the map never shows is left out.
        let peru = MapView::fit(
            Projection::Mercator,
            (-19.0, 1.0),
            Longitudes::Range(-82.0, -68.0),
            Rect::new(-7.0, -3.0, 7.0, 3.0),
        )
        .unwrap();
        let far = vec![(10.0, 40.0), (12.0, 40.0), (12.0, 42.0)];
        assert!(peru.area_path(&[vec![far]], 0.0).unwrap().is_empty());
        let line = peru
            .line_path(&[vec![(-81.0, 0.0), (-70.0, -18.0)]], false, 0.0)
            .unwrap();
        assert_eq!(subpaths(&line).len(), 1);
    }

    #[test]
    fn a_tolerance_drops_points_that_barely_move_the_line() {
        let map = world();
        let wiggly: GeoLine = (0..=100)
            .map(|index| (index as f64 * 0.1, if index % 2 == 0 { 0.0 } else { 0.001 }))
            .collect();
        let exact = subpaths(&map.line_path(&[wiggly.clone()], false, 0.0).unwrap());
        assert_eq!(exact[0].len(), 101);
        let simple = subpaths(&map.line_path(&[wiggly], false, 0.01).unwrap());
        assert_eq!(simple[0], vec![(0.0, 0.0), (10.0, 0.0)]);
        // A closed ring keeps enough points to enclose something.
        let ring: GeoLine = vec![
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 10.0),
            (5.0, 10.001),
            (0.0, 10.0),
        ];
        let simple = subpaths(&map.area_path(&[vec![ring]], 0.01).unwrap());
        assert_eq!(simple[0].len(), 4, "{simple:?}");
    }

    #[test]
    fn graticules_draw_round_meridians_and_parallels_and_label_them() {
        let map = MapView::fit(
            Projection::Mercator,
            (-19.0, 1.0),
            Longitudes::Range(-105.0, -55.0),
            Rect::new(-7.0, -3.0, 7.0, 3.0),
        )
        .unwrap();
        assert_eq!(map.graticule_step(), 5.0);
        let mut options = GraticuleOptions::new((5.0, 5.0));
        options.every = (10.0, 5.0);
        let graticule = map.graticule(&options).unwrap();
        // Meridians from 100° W to 60° W (not the edges), parallels at 0°,
        // 5° S, 10° S and 15° S.
        assert_eq!(subpaths(&graticule.lines).len(), 9 + 4);
        let texts: Vec<(&str, MapSide)> = graticule
            .labels
            .iter()
            .map(|label| (label.text.as_str(), label.side))
            .collect();
        assert_eq!(
            texts,
            [
                ("100°W", MapSide::Bottom),
                ("90°W", MapSide::Bottom),
                ("80°W", MapSide::Bottom),
                ("70°W", MapSide::Bottom),
                ("60°W", MapSide::Bottom),
                ("15°S", MapSide::Left),
                ("10°S", MapSide::Left),
                ("5°S", MapSide::Left),
                ("0°", MapSide::Left),
            ]
        );
        assert!(near(graticule.labels[2].at, map.point(-80.0, 0.0).x));
        assert!(near(graticule.labels[8].at, map.point(0.0, 0.0).y));

        options.hemispheres = GraticuleOptions::parse_hemispheres("NSEO").unwrap();
        options.sides = vec![MapSide::Top, MapSide::Right];
        options.every = (20.0, 10.0);
        let texts: Vec<String> = map
            .graticule(&options)
            .unwrap()
            .labels
            .into_iter()
            .map(|label| label.text)
            .collect();
        assert_eq!(texts, ["100°O", "80°O", "60°O", "10°S", "0°"]);
        assert!(GraticuleOptions::parse_hemispheres("NSE").is_err());
        options.step = (0.0, 5.0);
        assert_eq!(map.graticule(&options).unwrap_err(), GeoError::InvalidStep);
    }

    #[test]
    fn labels_read_longitudes_between_the_antimeridians_and_keep_fractions() {
        assert_eq!(longitude_label(190.0, 'E', 'W'), "170°W");
        assert_eq!(longitude_label(-180.0, 'E', 'W'), "180°");
        assert_eq!(longitude_label(360.0, 'E', 'W'), "0°");
        assert_eq!(longitude_label(12.5, 'E', 'W'), "12.5°E");
        assert_eq!(latitude_label(-0.25, 'N', 'S'), "0.25°S");
        assert_eq!(latitude_label(45.0, 'N', 'S'), "45°N");
        let fine: Vec<f64> = multiples(-0.35, 0.35, 0.1).collect();
        assert_eq!(fine.len(), 7);
        assert_eq!(fine[3], 0.0);
    }
}
