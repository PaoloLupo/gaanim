use gaanim_core::glam::DVec3;
use gaanim_core::kurbo::Rect;

/// A 3D Axis-Aligned Bounding Box (AABB).
///
/// This serves as a dimension-agnostic bounding box for Mobjects and rendering pipelines.
/// Under 2D mode, the `z` coordinate is typically kept at 0.0, allowing a direct mapping to
/// a `kurbo::Rect` for Vello vector rendering, viewport culling, and hit testing.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Bounds3D {
    /// The minimum corner (minimum x, y, and z coordinates).
    pub min: DVec3,
    /// The maximum corner (maximum x, y, and z coordinates).
    pub max: DVec3,
}

impl Default for Bounds3D {
    fn default() -> Self {
        Self {
            min: DVec3::ZERO,
            max: DVec3::ZERO,
        }
    }
}

impl Bounds3D {
    /// Creates a new bounding box from its raw minimum and maximum coordinates.
    pub const fn new(min: DVec3, max: DVec3) -> Self {
        Self { min, max }
    }

    /// Creates a new 2D bounding box where the Z axis is zeroed out.
    pub fn new_2d(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        Self {
            min: DVec3::new(min_x, min_y, 0.0),
            max: DVec3::new(max_x, max_y, 0.0),
        }
    }

    /// Creates a new 3D bounding box specifying all coordinates.
    pub fn new_3d(min_x: f64, min_y: f64, min_z: f64, max_x: f64, max_y: f64, max_z: f64) -> Self {
        Self {
            min: DVec3::new(min_x, min_y, min_z),
            max: DVec3::new(max_x, max_y, max_z),
        }
    }

    /// Computes the center point of the bounding box.
    pub fn center(&self) -> DVec3 {
        (self.min + self.max) * 0.5
    }

    /// Computes the size vector of the bounding box.
    pub fn size(&self) -> DVec3 {
        self.max - self.min
    }

    /// The width along the X axis.
    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    /// The height along the Y axis.
    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }

    /// The depth along the Z axis.
    pub fn depth(&self) -> f64 {
        self.max.z - self.min.z
    }

    /// Merges this bounding box with another to create a new union bounding box.
    pub fn union(&self, other: &Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    /// Checks if this bounding box intersects with another bounding box.
    pub fn intersects(&self, other: &Self) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }

    /// Checks if a 3D point is inside the bounding box.
    pub fn contains(&self, point: DVec3) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    /// Projects this 3D bounding box to a standard 2D `kurbo::Rect`.
    pub fn to_rect_2d(&self) -> Rect {
        Rect::new(self.min.x, self.min.y, self.max.x, self.max.y)
    }

    /// Transforms the bounding box using a 2D affine transform.
    pub fn transform_2d(&self, transform: &gaanim_core::kurbo::Affine) -> Self {
        let p00 = *transform * gaanim_core::kurbo::Point::new(self.min.x, self.min.y);
        let p01 = *transform * gaanim_core::kurbo::Point::new(self.min.x, self.max.y);
        let p10 = *transform * gaanim_core::kurbo::Point::new(self.max.x, self.min.y);
        let p11 = *transform * gaanim_core::kurbo::Point::new(self.max.x, self.max.y);

        let min_x = p00.x.min(p01.x).min(p10.x).min(p11.x);
        let max_x = p00.x.max(p01.x).max(p10.x).max(p11.x);
        let min_y = p00.y.min(p01.y).min(p10.y).min(p11.y);
        let max_y = p00.y.max(p01.y).max(p10.y).max(p11.y);

        Self {
            min: DVec3::new(min_x, min_y, self.min.z),
            max: DVec3::new(max_x, max_y, self.max.z),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds3d_new_2d() {
        let b = Bounds3D::new_2d(0.0, 0.0, 10.0, 20.0);
        assert_eq!(b.min, DVec3::new(0.0, 0.0, 0.0));
        assert_eq!(b.max, DVec3::new(10.0, 20.0, 0.0));
    }

    #[test]
    fn bounds3d_center() {
        let b = Bounds3D::new_2d(0.0, 0.0, 10.0, 20.0);
        assert_eq!(b.center(), DVec3::new(5.0, 10.0, 0.0));
    }

    #[test]
    fn bounds3d_size_and_dimensions() {
        let b = Bounds3D::new_2d(1.0, 2.0, 11.0, 22.0);
        assert_eq!(b.size(), DVec3::new(10.0, 20.0, 0.0));
        assert_eq!(b.width(), 10.0);
        assert_eq!(b.height(), 20.0);
        assert_eq!(b.depth(), 0.0);
    }

    #[test]
    fn bounds3d_union() {
        let a = Bounds3D::new_2d(0.0, 0.0, 5.0, 5.0);
        let b = Bounds3D::new_2d(3.0, 3.0, 10.0, 10.0);
        let u = a.union(&b);
        assert_eq!(u.min, DVec3::new(0.0, 0.0, 0.0));
        assert_eq!(u.max, DVec3::new(10.0, 10.0, 0.0));
    }

    #[test]
    fn bounds3d_intersects_true() {
        let a = Bounds3D::new_2d(0.0, 0.0, 5.0, 5.0);
        let b = Bounds3D::new_2d(3.0, 3.0, 10.0, 10.0);
        assert!(a.intersects(&b));
    }

    #[test]
    fn bounds3d_intersects_false() {
        let a = Bounds3D::new_2d(0.0, 0.0, 1.0, 1.0);
        let b = Bounds3D::new_2d(2.0, 2.0, 3.0, 3.0);
        assert!(!a.intersects(&b));
    }

    #[test]
    fn bounds3d_contains() {
        let b = Bounds3D::new_2d(0.0, 0.0, 10.0, 10.0);
        assert!(b.contains(DVec3::new(5.0, 5.0, 0.0)));
        assert!(!b.contains(DVec3::new(11.0, 5.0, 0.0)));
    }

    #[test]
    fn bounds3d_to_rect_2d() {
        let b = Bounds3D::new_2d(1.0, 2.0, 3.0, 4.0);
        let r = b.to_rect_2d();
        assert!((r.x0 - 1.0).abs() < 1e-9);
        assert!((r.y0 - 2.0).abs() < 1e-9);
        assert!((r.x1 - 3.0).abs() < 1e-9);
        assert!((r.y1 - 4.0).abs() < 1e-9);
    }

    #[test]
    fn bounds3d_transform_2d_translation() {
        let b = Bounds3D::new_2d(0.0, 0.0, 1.0, 1.0);
        let transform = gaanim_core::kurbo::Affine::translate((5.0, -3.0));
        let bt = b.transform_2d(&transform);
        assert_eq!(bt.min, DVec3::new(5.0, -3.0, 0.0));
        assert_eq!(bt.max, DVec3::new(6.0, -2.0, 0.0));
    }
}
