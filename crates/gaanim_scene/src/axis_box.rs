//! Where a 3D coordinate space puts its grid planes and axes on its box.
//!
//! Seen from the camera, a box has three faces in front of its content and
//! three behind it, and an outline of six edges. As in plotting libraries,
//! each grid plane goes on the face behind the content, so that it never
//! crosses what the box holds, and each axis, with its ticks, numbers and
//! title, on an edge of the outline, where its numbers stand clear of the
//! box. [`AxisBoxLayout::for_view`] picks those faces and edges from the
//! camera alone, so an exact seek shows what playback does; the parts jump to
//! their new sides when the camera crosses the plane through the box's middle.
//!
//! The parts are built on the box's lowest faces and on the edges from its
//! lowest corner, which [`AxisBoxSides::Fixed`] keeps, and
//! [`axis_box_system`] moves them after transform propagation, as the HUD
//! and parallax pins do.

use bevy::prelude::{ChildOf, Children, Component, Entity, Query, Res};
use gaanim_core::glam::{DMat4, DVec3};
use gaanim_math::{GlobalSpatialTransform, SpatialTransform};

use crate::components::{CoordinateLabelOffset, CoordinateViewRole};

/// Where a 3D coordinate space puts its grid planes and axes on its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum AxisBoxSides {
    /// Each grid plane on the face behind the box's content and each axis on
    /// an edge of the box's outline, as the camera shows the box (see
    /// [`AxisBoxLayout::for_view`]).
    #[default]
    View,
    /// The grid planes on the box's lowest faces and the axes along the
    /// edges from its lowest corner, whatever the camera.
    Fixed,
}

/// The sides of a box that its grid planes and axes take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisBoxLayout {
    /// Per dimension, whether the grid plane across it lies on the box's
    /// highest face across it rather than its lowest.
    pub planes: [bool; 3],
    /// Per axis and other dimension, whether the axis's edge lies on that
    /// dimension's highest side; an axis's own entry is `false`.
    pub edges: [[bool; 3]; 3],
    /// Per axis, the dimension its ticks cross it along: y for x, and x for
    /// y and z, but the vertical axis's across the wall it lies on, so that
    /// where it meets another axis their end ticks cross instead of lying on
    /// top of each other.
    pub ticks: [usize; 3],
}

impl AxisBoxLayout {
    /// The box's lowest faces and the edges from its lowest corner.
    pub const FIXED: Self = Self {
        planes: [false; 3],
        edges: [[false; 3]; 3],
        ticks: Self::TICKS,
    };

    /// The dimension each axis's ticks cross it along at the lowest corner.
    pub const TICKS: [usize; 3] = [1, 0, 0];

    /// The faces and edges `camera` shows best of the box between `bounds`
    /// in the coordinates that `space` maps to the world.
    ///
    /// A grid plane takes the face across its dimension farther from the
    /// camera. Of the dimensions, the one the camera's `up` points along most
    /// is the vertical one, and the plane across it the floor: the two other
    /// axes run along the floor's edges nearer the camera, and the vertical
    /// axis along the edge of the outline on the left of the screen. A box
    /// the camera cannot place, such as one scaled to nothing, keeps
    /// [`Self::FIXED`].
    pub fn for_view(camera: &gaanim_math::Camera, space: DMat4, [low, high]: [DVec3; 2]) -> Self {
        let determinant = space.determinant();
        if !determinant.is_finite() || determinant.abs() < 1e-12 {
            return Self::FIXED;
        }
        let to_space = space.inverse();
        let center = (low + high) * 0.5;
        let forward = camera.rotation * DVec3::NEG_Z;
        let perspective = matches!(
            camera.projection,
            gaanim_math::Projection::Perspective { .. }
        );
        // Toward the camera from the box's middle, in the box's coordinates.
        let toward = if perspective {
            to_space.transform_point3(camera.position) - center
        } else {
            to_space.transform_vector3(-forward)
        };
        if !toward.is_finite() {
            return Self::FIXED;
        }
        // A camera level with the middle, up to rounding, keeps the lowest
        // face.
        let level = 1e-9 * toward.length();
        let planes = std::array::from_fn(|dimension| toward[dimension] < -level);

        let up = camera.up.normalize_or_zero();
        let alignment = |dimension: usize| {
            let mut unit = DVec3::ZERO;
            unit[dimension] = 1.0;
            space
                .transform_vector3(unit)
                .normalize_or_zero()
                .dot(up)
                .abs()
        };
        // On a tie the later dimension, so that z wins over x and y.
        let vertical = (0..3)
            .max_by(|&a, &b| alignment(a).total_cmp(&alignment(b)))
            .unwrap_or(2);
        let (a, b) = ((vertical + 1) % 3, (vertical + 2) % 3);
        let (a, b) = (a.min(b), a.max(b));

        let mut edges = [[false; 3]; 3];
        for (axis, other) in [(a, b), (b, a)] {
            edges[axis][other] = !planes[other];
            edges[axis][vertical] = planes[vertical];
        }
        // The vertical axis takes one of the outline's two vertical edges,
        // each on a wall: the one drawn farther left.
        let right = camera.rotation * DVec3::X;
        let screen_x = |sides: [(usize, bool); 2]| {
            let mut point = center;
            for (dimension, high_side) in sides {
                point[dimension] = if high_side {
                    high[dimension]
                } else {
                    low[dimension]
                };
            }
            let relative = space.transform_point3(point) - camera.position;
            let across = relative.dot(right);
            if perspective {
                across / relative.dot(forward)
            } else {
                across
            }
        };
        let on_a = [(a, planes[a]), (b, !planes[b])];
        let on_b = [(a, !planes[a]), (b, planes[b])];
        let (chosen, wall) = if screen_x(on_b) < screen_x(on_a) {
            (on_b, b)
        } else {
            (on_a, a)
        };
        for (dimension, high_side) in chosen {
            edges[vertical][dimension] = high_side;
        }
        let mut ticks = Self::TICKS;
        ticks[vertical] = wall;
        Self {
            planes,
            edges,
            ticks,
        }
    }

    /// How far the grid plane across `normal` lies from the box's lowest
    /// face across it.
    pub fn plane_offset(&self, normal: usize, [low, high]: [DVec3; 2]) -> DVec3 {
        let mut offset = DVec3::ZERO;
        if self.planes[normal] {
            offset[normal] = high[normal] - low[normal];
        }
        offset
    }

    /// How far `axis`'s edge lies from the edge along it from the box's
    /// lowest corner.
    pub fn edge_offset(&self, axis: usize, [low, high]: [DVec3; 2]) -> DVec3 {
        let mut offset = DVec3::ZERO;
        for dimension in 0..3 {
            if dimension != axis && self.edges[axis][dimension] {
                offset[dimension] = high[dimension] - low[dimension];
            }
        }
        offset
    }

    /// The turn that brings `axis`'s ticks, built across it along
    /// [`Self::TICKS`], to cross it along [`Self::ticks`]: a quarter turn
    /// about the axis's edge from the box's lowest corner, or `None`.
    pub fn tick_turn(&self, axis: usize, [low, _]: [DVec3; 2]) -> Option<DMat4> {
        let (from, to) = (Self::TICKS[axis], self.ticks[axis]);
        if from == to {
            return None;
        }
        let unit = |dimension: usize| {
            let mut unit = DVec3::ZERO;
            unit[dimension] = 1.0;
            unit
        };
        let along = unit(axis);
        let angle = std::f64::consts::FRAC_PI_2 * unit(from).cross(unit(to)).dot(along).signum();
        Some(
            DMat4::from_translation(low)
                * DMat4::from_axis_angle(along, angle)
                * DMat4::from_translation(-low),
        )
    }

    /// Whether the grid line on `edge` of its plane is drawn: not where an
    /// axis in `axes` lies, which draws that edge, and not on the edge it
    /// shares with another plane that has a line there and comes first, so
    /// that no two lines lie on top of each other.
    pub fn shows_plane_edge(&self, edge: PlaneEdge, axes: [bool; 3]) -> bool {
        let PlaneEdge {
            normal,
            across,
            high,
            twin,
        } = edge;
        let along = 3 - normal - across;
        let plane_side = self.planes[normal];
        if axes[along]
            && self.edges[along][across] == high
            && self.edges[along][normal] == plane_side
        {
            return false;
        }
        let shared = high == self.planes[across];
        !(shared && twin[usize::from(plane_side)] && across < normal)
    }

    /// Whether the number at the lowest or `high`est end of `axis` is
    /// written: not where the edge of an axis that comes first ends at the
    /// same corner and writes its own number there, as `numbered` says by
    /// axis and end, so that two numbers never pile up on a corner.
    pub fn shows_corner_number(&self, axis: usize, high: bool, numbered: [[bool; 2]; 3]) -> bool {
        let mut corner = self.edges[axis];
        corner[axis] = high;
        !(0..axis).any(|other| {
            (0..3)
                .filter(|&dimension| dimension != other)
                .all(|dimension| self.edges[other][dimension] == corner[dimension])
                && numbered[other][usize::from(corner[other])]
        })
    }
}

/// A grid line on an edge of its plane: the plane across `normal`, at the
/// lowest or `high`est end of `across`, the other dimension in the plane
/// being the line's direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaneEdge {
    pub normal: usize,
    pub across: usize,
    pub high: bool,
    /// Whether the plane across `across` is drawn and has a line on its
    /// edge at the lowest (`[0]`) or highest (`[1]`) end of `normal`: the
    /// edge this one shares with it when both lie behind the box.
    pub twin: [bool; 2],
}

/// Internal: a part of a 3D coordinate space's box that follows the camera
/// under [`AxisBoxSides::View`]. It is built on the box's lowest face across
/// its plane or on the edge of its axis from the box's lowest corner, and
/// [`axis_box_system`] moves it to the face or edge
/// [`AxisBoxLayout::for_view`] picks. Its parent's coordinates are the box's.
#[doc(hidden)]
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct AxisBoxPart {
    /// The lowest and highest corners of the box.
    pub bounds: [DVec3; 2],
    pub role: AxisBoxRole,
}

/// What a part of a box is (see [`AxisBoxPart`]).
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisBoxRole {
    /// The grid lines inside the plane across `normal`.
    Plane { normal: usize },
    /// The grid line on an edge of a plane, which shows where
    /// [`AxisBoxLayout::shows_plane_edge`] says so for the drawn `axes`.
    PlaneEdge { edge: PlaneEdge, axes: [bool; 3] },
    /// An axis line along `axis`.
    Axis { axis: usize },
    /// The ticks across `axis`, built across it along
    /// [`AxisBoxLayout::TICKS`].
    Ticks { axis: usize },
    /// The number at the lowest or `high`est end of `axis`, which shows
    /// where [`AxisBoxLayout::shows_corner_number`] says so; the billboard
    /// system places it.
    CornerNumber {
        axis: usize,
        high: bool,
        numbered: [[bool; 2]; 3],
    },
}

/// Internal: whether a part of a box shows (see [`AxisBoxPart`]). It
/// multiplies into the propagated opacity like [`crate::Presence`], so a
/// hidden grid line or number draws nothing.
#[doc(hidden)]
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisBoxShown(pub bool);

/// System: move every [`AxisBoxPart`] to the side of its box the camera
/// shows best, and show or hide the grid lines on the box's edges and the
/// numbers on its corners.
///
/// Runs after transform propagation and the HUD and parallax pins, and
/// recomposes each part's world transform from its local one under its
/// parent's moved by the part's offset, so the result is the same whether or
/// not propagation ran this frame. A part on its lowest side gets exactly
/// what propagation wrote.
#[allow(clippy::too_many_arguments)]
pub fn axis_box_system(
    camera: Option<Res<gaanim_math::ResolvedCamera>>,
    parts: Query<(Entity, &AxisBoxPart, &ChildOf)>,
    mut shown: Query<&mut AxisBoxShown>,
    children_query: Query<&'static Children>,
    mut transforms: Query<(
        &SpatialTransform,
        &mut GlobalSpatialTransform,
        Option<&crate::ShapeDeform>,
    )>,
    view_roles: Query<&'static CoordinateViewRole>,
    label_offsets: Query<&'static CoordinateLabelOffset>,
    parents: Query<&'static ChildOf>,
) {
    let Some(camera) = camera else { return };
    for (entity, part, parent) in &parts {
        let Ok((_, parent_global, _)) = transforms.get(parent.parent()) else {
            continue;
        };
        let parent_global = *parent_global;
        let layout = AxisBoxLayout::for_view(&camera, parent_global.mat4, part.bounds);
        let show = |shown: &mut Query<&mut AxisBoxShown>, value: bool| {
            if let Ok(mut flag) = shown.get_mut(entity)
                && flag.0 != value
            {
                flag.0 = value;
            }
        };
        let offset = match part.role {
            AxisBoxRole::Plane { normal }
            | AxisBoxRole::PlaneEdge {
                edge: PlaneEdge { normal, .. },
                ..
            } => layout.plane_offset(normal, part.bounds),
            AxisBoxRole::Axis { axis } | AxisBoxRole::Ticks { axis } => {
                layout.edge_offset(axis, part.bounds)
            }
            AxisBoxRole::CornerNumber {
                axis,
                high,
                numbered,
            } => {
                show(&mut shown, layout.shows_corner_number(axis, high, numbered));
                continue;
            }
        };
        let turn = match part.role {
            AxisBoxRole::Ticks { axis } => layout.tick_turn(axis, part.bounds),
            _ => None,
        };
        if let AxisBoxRole::PlaneEdge { edge, axes } = part.role {
            show(&mut shown, layout.shows_plane_edge(edge, axes));
        }
        // 3D parts draw from `mat4`; the 2D transform keeps the shift.
        let mat4 = parent_global.mat4 * DMat4::from_translation(offset);
        let moved = GlobalSpatialTransform {
            affine_2d: parent_global.affine_2d
                * gaanim_core::kurbo::Affine::translate((offset.x, offset.y)),
            mat4: turn.map_or(mat4, |turn| mat4 * turn),
        };
        crate::systems::propagate_transforms_recursive(
            entity,
            Some(moved),
            true,
            &mut transforms,
            &crate::systems::Propagation {
                children_query: &children_query,
                view_roles: &view_roles,
                label_offsets: &label_offsets,
                parents: &parents,
                stale: None,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{Schedule, World};

    fn camera_at(eye: DVec3, up: DVec3) -> gaanim_math::Camera {
        let mut camera = gaanim_math::Camera::perspective_3d(1280, 720, 0.8);
        camera.look_at(eye, DVec3::ZERO, up).unwrap();
        camera
    }

    const BOX: [DVec3; 2] = [DVec3::splat(-1.0), DVec3::splat(1.0)];

    #[test]
    fn planes_take_the_faces_behind_and_axes_the_outline() {
        // From the front right and above, z up: the walls at x = -1 and
        // y = 1 and the floor at z = -1 lie behind.
        let camera = camera_at(DVec3::new(6.0, -5.0, 4.0), DVec3::Z);
        let layout = AxisBoxLayout::for_view(&camera, DMat4::IDENTITY, BOX);
        assert_eq!(layout.planes, [false, true, false]);
        // x along the floor's front edge (y = -1), y along its right one
        // (x = 1), z up the outline's left edge (x = -1, y = -1).
        assert_eq!(layout.edges[0], [false, false, false]);
        assert_eq!(layout.edges[1], [true, false, false]);
        assert_eq!(layout.edges[2], [false, false, false]);
        // z on the wall across x, its ticks across x.
        assert_eq!(layout.ticks, [1, 0, 0]);

        // Around the other side, everything turns with it.
        let camera = camera_at(DVec3::new(-6.0, 5.0, 4.0), DVec3::Z);
        let layout = AxisBoxLayout::for_view(&camera, DMat4::IDENTITY, BOX);
        assert_eq!(layout.planes, [true, false, false]);
        assert_eq!(layout.edges[0], [false, true, false]);
        assert_eq!(layout.edges[1], [false, false, false]);
        assert_eq!(layout.edges[2], [true, true, false]);

        // From below, the floor is the top face.
        let camera = camera_at(DVec3::new(6.0, -5.0, -4.0), DVec3::Z);
        let layout = AxisBoxLayout::for_view(&camera, DMat4::IDENTITY, BOX);
        assert_eq!(layout.planes, [false, true, true]);
        assert_eq!(layout.edges[0], [false, false, true]);
    }

    #[test]
    fn the_vertical_follows_the_cameras_up() {
        // y up: the floor is across y, and the vertical axis is y.
        let camera = camera_at(DVec3::new(6.0, 4.0, 5.0), DVec3::Y);
        let layout = AxisBoxLayout::for_view(&camera, DMat4::IDENTITY, BOX);
        assert_eq!(layout.planes, [false, false, false]);
        // x along the floor's front edge (z = 1), z along its right one
        // (x = 1), both on the floor at y = -1.
        assert_eq!(layout.edges[0], [false, false, true]);
        assert_eq!(layout.edges[2], [true, false, false]);
        assert!(!layout.edges[1][0] || !layout.edges[1][2]);
    }

    #[test]
    fn the_layout_reads_the_box_through_its_transform() {
        // The box turned half a turn about z shows the camera its other
        // side.
        let camera = camera_at(DVec3::new(6.0, -5.0, 4.0), DVec3::Z);
        let turned = DMat4::from_rotation_z(std::f64::consts::PI);
        let layout = AxisBoxLayout::for_view(&camera, turned, BOX);
        assert_eq!(layout.planes, [true, false, false]);
        // Scaled to nothing, it keeps its lowest sides.
        let flat = DMat4::from_scale(DVec3::new(1.0, 0.0, 1.0));
        assert_eq!(
            AxisBoxLayout::for_view(&camera, flat, BOX),
            AxisBoxLayout::FIXED
        );
    }

    #[test]
    fn orthographic_cameras_read_their_direction() {
        let mut camera = gaanim_math::Camera::ortho_2d(1280, 720);
        camera
            .look_at(DVec3::new(-6.0, -5.0, 4.0), DVec3::ZERO, DVec3::Z)
            .unwrap();
        let layout = AxisBoxLayout::for_view(&camera, DMat4::IDENTITY, BOX);
        assert_eq!(layout.planes, [true, true, false]);
        // The default 2D camera looks down z with y up: everything stays.
        let flat = gaanim_math::Camera::ortho_2d(1280, 720);
        let layout = AxisBoxLayout::for_view(&flat, DMat4::IDENTITY, BOX);
        assert_eq!(layout.planes, [false, false, false]);
    }

    #[test]
    fn no_grid_line_lies_under_an_axis_or_another_plane() {
        // Every line on a plane's edge, with ticks at both ends everywhere.
        let edges: Vec<PlaneEdge> = (0..3)
            .flat_map(|normal| {
                (0..3)
                    .filter(move |&across| across != normal)
                    .flat_map(move |across| {
                        [false, true].map(|high| PlaneEdge {
                            normal,
                            across,
                            high,
                            twin: [true, true],
                        })
                    })
            })
            .collect();
        let eyes = [
            DVec3::new(6.0, -5.0, 4.0),
            DVec3::new(-6.0, 5.0, 4.0),
            DVec3::new(-6.0, -5.0, -4.0),
            DVec3::new(6.0, 5.0, -4.0),
        ];
        let layouts = eyes
            .map(|eye| AxisBoxLayout::for_view(&camera_at(eye, DVec3::Z), DMat4::IDENTITY, BOX))
            .into_iter()
            .chain([AxisBoxLayout::FIXED]);
        for layout in layouts {
            // An edge as its direction and its sides along the two others.
            let mut drawn: Vec<(usize, [bool; 3])> = Vec::new();
            for axis in 0..3 {
                drawn.push((axis, layout.edges[axis]));
            }
            for edge in &edges {
                if !layout.shows_plane_edge(*edge, [true; 3]) {
                    continue;
                }
                let along = 3 - edge.normal - edge.across;
                let mut sides = [false; 3];
                sides[edge.across] = edge.high;
                sides[edge.normal] = layout.planes[edge.normal];
                assert!(
                    !drawn.contains(&(along, sides)),
                    "{layout:?}: {edge:?} lies on a drawn edge"
                );
                drawn.push((along, sides));
            }
            // The outline and the edges behind are all drawn: three axes
            // and six grid lines.
            assert_eq!(drawn.len(), 9, "{layout:?}");
        }
    }

    #[test]
    fn a_corner_keeps_one_number() {
        let numbered = [[true; 2]; 3];
        // From the front right, x ends at the front corner (x = 1, y = -1),
        // where y starts, and z starts where x does (x = y = -1).
        let camera = camera_at(DVec3::new(6.0, -5.0, 4.0), DVec3::Z);
        let layout = AxisBoxLayout::for_view(&camera, DMat4::IDENTITY, BOX);
        assert!(layout.shows_corner_number(0, false, numbered));
        assert!(layout.shows_corner_number(0, true, numbered));
        assert!(!layout.shows_corner_number(1, false, numbered));
        assert!(layout.shows_corner_number(1, true, numbered));
        assert!(!layout.shows_corner_number(2, false, numbered));
        assert!(layout.shows_corner_number(2, true, numbered));
        // Without x's numbers, y and z keep theirs.
        let unnumbered_x = [[false; 2], [true; 2], [true; 2]];
        assert!(layout.shows_corner_number(1, false, unnumbered_x));
        assert!(layout.shows_corner_number(2, false, unnumbered_x));
        // At the lowest corner, only x's.
        let fixed = AxisBoxLayout::FIXED;
        assert!(!fixed.shows_corner_number(1, false, numbered));
        assert!(!fixed.shows_corner_number(2, false, numbered));
        assert!(!fixed.shows_corner_number(2, false, unnumbered_x));
        assert!(fixed.shows_corner_number(1, false, unnumbered_x));
    }

    #[test]
    fn parts_move_to_their_sides_and_back() {
        let mut world = World::new();
        let camera = camera_at(DVec3::new(-6.0, 5.0, 4.0), DVec3::Z);
        world.insert_resource(gaanim_math::ResolvedCamera::new(
            camera,
            gaanim_math::CameraViewport::default(),
        ));
        let space = world
            .spawn((
                SpatialTransform::default(),
                GlobalSpatialTransform::default(),
            ))
            .id();
        let spawn = |world: &mut World, role| {
            world
                .spawn((
                    SpatialTransform::default(),
                    GlobalSpatialTransform::default(),
                    AxisBoxPart { bounds: BOX, role },
                    AxisBoxShown(true),
                    ChildOf(space),
                ))
                .id()
        };
        let wall = spawn(&mut world, AxisBoxRole::Plane { normal: 0 });
        let z_axis = spawn(&mut world, AxisBoxRole::Axis { axis: 2 });
        let z_ticks = spawn(&mut world, AxisBoxRole::Ticks { axis: 2 });
        // The floor's line at y = 1, along x: the back edge it shares with
        // the wall across y, which comes first and draws it.
        let shared = spawn(
            &mut world,
            AxisBoxRole::PlaneEdge {
                edge: PlaneEdge {
                    normal: 2,
                    across: 1,
                    high: false,
                    twin: [true, true],
                },
                axes: [true; 3],
            },
        );
        let mut schedule = Schedule::default();
        schedule.add_systems(axis_box_system);
        schedule.run(&mut world);
        let at = |world: &World, entity| {
            world
                .get::<GlobalSpatialTransform>(entity)
                .unwrap()
                .mat4
                .transform_point3(DVec3::ZERO)
        };
        assert_eq!(at(&world, wall), DVec3::new(2.0, 0.0, 0.0));
        assert_eq!(at(&world, z_axis), DVec3::new(2.0, 2.0, 0.0));
        // From here the wall across y lies at y = -1 too, so the floor's
        // line there is the back edge they share, which the wall draws.
        assert_eq!(
            world.get::<AxisBoxShown>(shared),
            Some(&AxisBoxShown(false))
        );
        // z's ticks, built across x at x = y = -1, cross the edge it takes:
        // along x from here, where z lies on the wall across x.
        let tick_end = |world: &World| {
            world
                .get::<GlobalSpatialTransform>(z_ticks)
                .unwrap()
                .mat4
                .transform_point3(DVec3::new(-1.1, -1.0, 0.0))
        };
        assert!((tick_end(&world) - DVec3::new(0.9, 1.0, 0.0)).length() < 1e-12);

        // From the front left, z takes one of the outline's vertical edges;
        // on the wall across y, its ticks cross along y.
        let front_left = camera_at(DVec3::new(-6.0, -5.0, 4.0), DVec3::Z);
        world.resource_mut::<gaanim_math::ResolvedCamera>().camera = front_left;
        schedule.run(&mut world);
        let layout = AxisBoxLayout::for_view(&front_left, DMat4::IDENTITY, BOX);
        let edge = DVec3::new(
            if layout.edges[2][0] { 1.0 } else { -1.0 },
            if layout.edges[2][1] { 1.0 } else { -1.0 },
            0.0,
        );
        let across = tick_end(&world) - edge;
        assert!((across.length() - 0.1).abs() < 1e-9, "{across:?}");
        assert!(across.z.abs() < 1e-12);
        let along = if layout.ticks[2] == 0 {
            across.x
        } else {
            across.y
        };
        assert!((along.abs() - 0.1).abs() < 1e-9, "{layout:?}: {across:?}");

        // From the front right, the wall goes back to x = -1.
        world.resource_mut::<gaanim_math::ResolvedCamera>().camera =
            camera_at(DVec3::new(6.0, -5.0, 4.0), DVec3::Z);
        schedule.run(&mut world);
        assert_eq!(at(&world, wall), DVec3::ZERO);
        assert_eq!(at(&world, z_axis), DVec3::ZERO);
        // The floor's line at y = -1 now lies under the x axis.
        assert_eq!(
            world.get::<AxisBoxShown>(shared),
            Some(&AxisBoxShown(false))
        );
    }
}
