//! Cartesian axis ticks that follow the view window set by `view_to`.
//!
//! A space is built with one tick set per axis (generation 0). When `view_to`
//! targets a window whose automatic step differs, a new generation with that
//! step is built over the target window. Every generation carries the
//! `(ln view scale, generation)` anchors of its axis, and the scene fades each
//! one from the live view scale, so immediate views, animations, seeks and
//! rewinds all show the ticks of the nearest authored view.

use std::sync::{Arc, Mutex};

use gaanim_core::glam::DVec2;
use gaanim_core::kurbo::{BezPath, Point, Shape};
use gaanim_core::peniko::Color;
use gaanim_math::Bounds3D;
use gaanim_visualization::{CartesianSpace, Scale};

use super::ops::Op;
use super::visualization::{CoordinateSpaceHandle, VisualizationError};
use gaanim_core::ObjectId;

use super::{DrawableHandle, SceneModel, SpawnKind};

/// `(ln view scale, generation)` for every authored view of one axis.
pub(crate) type TickAnchors = Arc<Mutex<Vec<(f64, u32)>>>;

pub(crate) const TICK_COUNT: usize = 7;
pub(crate) const NUMBER_GAP: f64 = 0.12;

/// Regeneration state shared by every clone of a Cartesian space handle.
#[derive(Debug)]
pub(crate) struct CartesianTicks {
    space: CartesianSpace,
    number_scale: f64,
    /// Public layer groups: major grid, minor grid, ticks, numbers.
    layers: [DrawableHandle; 4],
    /// Generation-0 grid/tick paths holding both axes until a regeneration
    /// needs them split per axis.
    joint: Option<[DrawableHandle; 3]>,
    /// Clip shared by every grid and tick path.
    line_mask: DrawableHandle,
    /// Clip shared by every tick number.
    number_mask: DrawableHandle,
    /// Public axis-line path, the style reference for zoomed-out axis lines.
    axis_line: DrawableHandle,
    /// Operations already moved next to the space's declaration.
    relocated_ops: usize,
    /// Spawns that new grid, tick and number drawables are declared before,
    /// so they stack like generation 0: grid < axes < ticks < numbers.
    stack_before: [ObjectId; 3],
    /// Local extent that grid lines of each axis must span: the unzoomed
    /// plot plus every authored view of the other axis.
    reach: [(f64, f64); 2],
    axes: [AxisTicks; 2],
}

#[derive(Debug)]
struct AxisTicks {
    anchors: TickAnchors,
    generations: Vec<TickGeneration>,
}

#[derive(Debug)]
struct TickGeneration {
    id: u32,
    step: f64,
    coverage: (f64, f64),
    /// Major grid, minor grid and tick paths; `None` while generation 0 is joint.
    paths: Option<[DrawableHandle; 3]>,
    /// Axis line beyond the original domain, for zoomed-out or panned views.
    axis_line: Option<DrawableHandle>,
    /// Local extent spanned by this generation's grid lines.
    cross: (f64, f64),
    /// Group receiving this generation's numbers. Generation 0 uses the flat
    /// public numbers layer and fades each number; later ones fade the group.
    numbers: DrawableHandle,
    number_values: Vec<f64>,
}

/// One axis's share of a tick generation, in unzoomed space-local coordinates.
struct AxisPartGeometry {
    major: BezPath,
    minor: BezPath,
    ticks: BezPath,
    /// Label, data anchor on the axis, and unzoomed offset from it.
    numbers: Vec<(f64, String, Point, DVec2)>,
}

impl CartesianTicks {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        space: CartesianSpace,
        number_scale: f64,
        layers: [DrawableHandle; 4],
        joint: [DrawableHandle; 3],
        joint_wrappers: [DrawableHandle; 3],
        line_mask: DrawableHandle,
        number_mask: DrawableHandle,
        axis_line: DrawableHandle,
        number_handles: [Vec<DrawableHandle>; 2],
        number_values: [Vec<f64>; 2],
    ) -> Self {
        let numbers_layer = layers[3].clone();
        let axis = |index: usize, handles: &[DrawableHandle], number_values: Vec<f64>| {
            let spec = if index == 0 {
                &space.map.x
            } else {
                &space.map.y
            };
            let anchors: TickAnchors = Arc::new(Mutex::new(vec![(0.0, 0)]));
            for number in handles {
                mark_level(number, index as u8, 0, &anchors);
            }
            AxisTicks {
                anchors,
                generations: vec![TickGeneration {
                    id: 0,
                    step: spec.resolved_tick_step(TICK_COUNT),
                    coverage: spec.domain(),
                    paths: None,
                    axis_line: None,
                    cross: if index == 0 {
                        (-space.map.frame.height * 0.5, space.map.frame.height * 0.5)
                    } else {
                        (-space.map.frame.width * 0.5, space.map.frame.width * 0.5)
                    },
                    numbers: numbers_layer.clone(),
                    number_values,
                }],
            }
        };
        let [x_values, y_values] = number_values;
        let axes = [
            axis(0, &number_handles[0], x_values),
            axis(1, &number_handles[1], y_values),
        ];
        // Until a split, the joint paths fade with the x axis only; their
        // y lines always belong to generation 0.
        for wrapper in &joint_wrappers {
            mark_level(wrapper, 0, 0, &axes[0].anchors);
        }
        let first_number = number_handles.iter().flatten().next();
        let stack_before = [
            axis_line.id,
            first_number.map_or(numbers_layer.id, |number| number.id),
            numbers_layer.id,
        ];
        let frame = space.map.frame;
        let reach = [
            (-frame.height * 0.5, frame.height * 0.5),
            (-frame.width * 0.5, frame.width * 0.5),
        ];
        Self {
            space,
            number_scale,
            layers,
            joint: Some(joint),
            line_mask,
            number_mask,
            axis_line,
            relocated_ops: 0,
            stack_before,
            reach,
            axes,
        }
    }
}

/// Stacking layer of a drawn tick part: 0 grid, 1 ticks and axis lines,
/// 2 numbers; `None` for groups and every non-spawn operation.
fn tick_stack_layer(op: &Op) -> Option<usize> {
    let Op::Spawn(spec) = op else {
        return None;
    };
    let spec = spec.lock().expect("object spec poisoned");
    match &spec.kind {
        SpawnKind::SvgPath(path) => match path.id.as_str() {
            "CoordinateMajorGrid" | "CoordinateMinorGrid" => Some(0),
            "CoordinateTicks" | "CoordinateAxes" => Some(1),
            _ => None,
        },
        SpawnKind::Text(_) => Some(2),
        _ => None,
    }
}

fn mark_level(handle: &DrawableHandle, axis: u8, generation: u32, anchors: &TickAnchors) {
    handle
        .spec
        .lock()
        .expect("tick spec poisoned")
        .coordinate_tick_level = Some((axis, generation, anchors.clone()));
}

fn same_step(left: f64, right: f64) -> bool {
    (left - right).abs() <= left.abs().max(right.abs()) * 1e-9
}

fn covers(coverage: (f64, f64), domain: (f64, f64)) -> bool {
    let slack = (coverage.1 - coverage.0).abs() * 1e-9;
    coverage.0 <= domain.0 + slack && domain.1 <= coverage.1 + slack
}

/// A target window plus half its span on each side, so small pans reuse it.
fn padded(domain: (f64, f64)) -> (f64, f64) {
    let half = (domain.1 - domain.0) * 0.5;
    (domain.0 - half, domain.1 + half)
}

impl SceneModel {
    /// Set the view window of `space` at the cursor and rebuild automatic
    /// axis ticks for it.
    ///
    /// Axes without a fixed `ticks(step)` get ticks, grid lines and numbers
    /// with the step chosen for the new window; they cross-fade with the
    /// previous ones as the view scale changes. Fixed steps are kept.
    pub fn coordinate_view_to(
        &mut self,
        space: &CoordinateSpaceHandle,
        x_domain: (f64, f64),
        y_domain: (f64, f64),
    ) -> Result<(), VisualizationError> {
        space.view_to(x_domain, y_domain)?;
        space.add_view_to_curves([x_domain, y_domain]);
        self.retick_coordinate_view(space, [x_domain, y_domain])
    }

    /// Describe a view-window animation of `space` and rebuild automatic axis
    /// ticks for its target window, as [`Self::coordinate_view_to`] does.
    pub fn coordinate_view_to_animation(
        &mut self,
        space: &CoordinateSpaceHandle,
        x_domain: (f64, f64),
        y_domain: (f64, f64),
    ) -> Result<super::types::Anim, VisualizationError> {
        let animation = space.view_to_animation(x_domain, y_domain)?;
        space.add_view_to_curves([x_domain, y_domain]);
        self.retick_coordinate_view(space, [x_domain, y_domain])?;
        Ok(animation)
    }

    fn retick_coordinate_view(
        &mut self,
        space: &CoordinateSpaceHandle,
        domains: [(f64, f64); 2],
    ) -> Result<(), VisualizationError> {
        let Some(shared) = space.ticks.clone() else {
            return Ok(());
        };
        let mut ticks = shared.lock().expect("tick state poisoned");
        // Build in the segment that owns the space, not the one authoring the view.
        let segment = space.view.segment_idx;
        let (previous_segment, first_op) = {
            let mut state = self.state.lock().expect("canvas state poisoned");
            let first_op = state.segments[segment].ops.len();
            (std::mem::replace(&mut state.active_idx, segment), first_op)
        };
        let result = (|| {
            Self::widen_tick_reach(&mut ticks, domains)?;
            for (axis, domain) in domains.into_iter().enumerate() {
                self.retick_axis(&mut ticks, axis, domain)?;
            }
            self.stretch_tick_generations(&mut ticks)
        })();
        let mut state = self.state.lock().expect("canvas state poisoned");
        state.active_idx = previous_segment;
        // Declare the new ticks as if they were built with the space: compile
        // then attaches them before any view change, in unzoomed coordinates.
        // Drawn parts are spawned beside generation 0's parts of the same
        // layer, since spawn order breaks render-order ties.
        let ops = &mut state.segments[segment].ops;
        let spawn_index = |ops: &[Op], id: ObjectId| {
            ops.iter().position(|op| {
                matches!(op, Op::Spawn(spec) if spec.lock().expect("object spec poisoned").id == id)
            })
        };
        let mut rest = Vec::new();
        let mut stacked: [Vec<Op>; 3] = Default::default();
        for op in ops.drain(first_op..) {
            match tick_stack_layer(&op) {
                Some(layer) => stacked[layer].push(op),
                None => rest.push(op),
            }
        }
        for (layer, moved) in stacked.into_iter().enumerate() {
            match spawn_index(ops, ticks.stack_before[layer]) {
                Some(at) => {
                    ops.splice(at..at, moved);
                }
                None => rest.extend(moved),
            }
        }
        if let Some(root_index) = spawn_index(ops, space.root.id) {
            let at = root_index + 1 + ticks.relocated_ops;
            ticks.relocated_ops += rest.len();
            ops.splice(at..at, rest);
        } else {
            ops.extend(rest);
        }
        result
    }

    fn retick_axis(
        &mut self,
        ticks: &mut CartesianTicks,
        axis: usize,
        target: (f64, f64),
    ) -> Result<(), VisualizationError> {
        let spec = if axis == 0 {
            &ticks.space.map.x
        } else {
            &ticks.space.map.y
        };
        if !spec.has_auto_ticks() || !matches!(spec.scale(), Scale::Linear | Scale::Time) {
            return Ok(());
        }
        let original = spec.domain();
        let ln_scale = ((original.1 - original.0) / (target.1 - target.0)).ln();
        let step = spec
            .with_domain(target.0, target.1)?
            .resolved_tick_step(TICK_COUNT);
        let existing = ticks.axes[axis]
            .generations
            .iter()
            .position(|generation| same_step(generation.step, step));
        let index = match existing {
            Some(index) => {
                if !covers(ticks.axes[axis].generations[index].coverage, target) {
                    self.split_joint_ticks(ticks)?;
                    self.extend_tick_generation(ticks, axis, index, target)?;
                }
                index
            }
            None => {
                self.split_joint_ticks(ticks)?;
                self.push_tick_generation(ticks, axis, step, padded(target))?
            }
        };
        let id = ticks.axes[axis].generations[index].id;
        let mut anchors = ticks.axes[axis]
            .anchors
            .lock()
            .expect("tick anchors poisoned");
        anchors.retain(|(value, _)| (value - ln_scale).abs() > 1e-12);
        anchors.push((ln_scale, id));
        anchors.sort_by(|left, right| left.0.total_cmp(&right.0));
        Ok(())
    }

    /// Unzoomed offset of a tick number from its anchor on the axis.
    fn tick_number_offset(&self, ticks: &CartesianTicks, axis: usize, label: &str) -> DVec2 {
        if axis == 0 {
            let style = ticks.space.map.x.style_value();
            DVec2::new(
                0.0,
                -(style.tick_length
                    + NUMBER_GAP
                    + self.x_tick_label_extra_offset(label, ticks.number_scale)),
            )
        } else {
            DVec2::new(
                -(ticks.space.map.y.style_value().tick_length + NUMBER_GAP),
                0.0,
            )
        }
    }

    /// Ticks, grid lines and numbers of one axis over `coverage` at `step`.
    /// Perpendicular grid lines span `cross` in local units.
    fn axis_part_geometry(
        &self,
        ticks: &CartesianTicks,
        axis: usize,
        step: f64,
        coverage: (f64, f64),
        cross: (f64, f64),
    ) -> Result<AxisPartGeometry, VisualizationError> {
        let space = &ticks.space;
        let spec = if axis == 0 {
            &space.map.x
        } else {
            &space.map.y
        };
        let values = spec
            .with_domain(coverage.0, coverage.1)?
            .ticks(step)?
            .ticks_values(TICK_COUNT)?;
        let x_cross = space.map.x.crossing_value();
        let y_cross = space.map.y.crossing_value();
        let origin = space.map.data_to_local(x_cross, y_cross)?;
        let style = spec.style_value();
        let visibility = space.visibility;
        let (grid, tick_marks, numbers) = if axis == 0 {
            (visibility.x_grid, visibility.x_ticks, visibility.x_numbers)
        } else {
            (visibility.y_grid, visibility.y_ticks, visibility.y_numbers)
        };
        let mut geometry = AxisPartGeometry {
            major: BezPath::new(),
            minor: BezPath::new(),
            ticks: BezPath::new(),
            numbers: Vec::new(),
        };
        for tick in values {
            let half = style.tick_length * if tick.major { 0.5 } else { 0.3 };
            let target = if tick.major {
                &mut geometry.major
            } else {
                &mut geometry.minor
            };
            if axis == 0 {
                let x = space.map.data_to_local(tick.value, y_cross)?.x;
                if grid {
                    // Top to bottom, like the reveal of generation 0.
                    target.move_to(Point::new(x, cross.1));
                    target.line_to(Point::new(x, cross.0));
                }
                if tick_marks {
                    geometry.ticks.move_to(Point::new(x, origin.y - half));
                    geometry.ticks.line_to(Point::new(x, origin.y + half));
                }
                if numbers && tick.major && !tick.label.is_empty() {
                    let offset = self.tick_number_offset(ticks, 0, &tick.label);
                    geometry.numbers.push((
                        tick.value,
                        tick.label,
                        Point::new(x, origin.y),
                        offset,
                    ));
                }
            } else {
                let y = space.map.data_to_local(x_cross, tick.value)?.y;
                if grid {
                    target.move_to(Point::new(cross.0, y));
                    target.line_to(Point::new(cross.1, y));
                }
                if tick_marks {
                    geometry.ticks.move_to(Point::new(origin.x - half, y));
                    geometry.ticks.line_to(Point::new(origin.x + half, y));
                }
                if numbers && tick.major && !tick.label.is_empty() && tick.value != x_cross {
                    let offset = self.tick_number_offset(ticks, 1, &tick.label);
                    geometry.numbers.push((
                        tick.value,
                        tick.label,
                        Point::new(origin.x, y),
                        offset,
                    ));
                }
            }
        }
        Ok(geometry)
    }

    /// Grow each axis's grid-line reach to the other axis's target window.
    fn widen_tick_reach(
        ticks: &mut CartesianTicks,
        domains: [(f64, f64); 2],
    ) -> Result<(), VisualizationError> {
        let map = &ticks.space.map;
        let (x_cross, y_cross) = (map.x.crossing_value(), map.y.crossing_value());
        let y_window = (
            map.data_to_local(x_cross, domains[1].0)?.y,
            map.data_to_local(x_cross, domains[1].1)?.y,
        );
        let x_window = (
            map.data_to_local(domains[0].0, y_cross)?.x,
            map.data_to_local(domains[0].1, y_cross)?.x,
        );
        // x grid lines are vertical and follow the y window, and vice versa.
        for (axis, window) in [(0, y_window), (1, x_window)] {
            let window = (window.0.min(window.1), window.0.max(window.1));
            ticks.reach[axis] = union(ticks.reach[axis], window);
        }
        Ok(())
    }

    /// Rebuild grid lines of generations that no longer reach every view.
    fn stretch_tick_generations(
        &mut self,
        ticks: &mut CartesianTicks,
    ) -> Result<(), VisualizationError> {
        for axis in 0..2 {
            let reach = ticks.reach[axis];
            for index in 0..ticks.axes[axis].generations.len() {
                if covers(ticks.axes[axis].generations[index].cross, reach) {
                    continue;
                }
                self.split_joint_ticks(ticks)?;
                let generation = &ticks.axes[axis].generations[index];
                let (step, coverage) = (generation.step, generation.coverage);
                let paths = generation
                    .paths
                    .clone()
                    .expect("generation 0 is split before stretching");
                let geometry = self.axis_part_geometry(ticks, axis, step, coverage, reach)?;
                for (path, part) in
                    paths
                        .iter()
                        .zip([geometry.major, geometry.minor, geometry.ticks])
                {
                    replace_path(path, part);
                }
                ticks.axes[axis].generations[index].cross = reach;
            }
        }
        Ok(())
    }

    fn tick_number_drawables(
        &mut self,
        ticks: &CartesianTicks,
        axis: usize,
        numbers: &[(f64, String, Point, DVec2)],
    ) -> Vec<DrawableHandle> {
        let color = if axis == 0 {
            ticks.space.map.x.style_value().number_color
        } else {
            ticks.space.map.y.style_value().number_color
        };
        numbers
            .iter()
            .map(|(_, label, anchor, offset)| {
                let handle = self
                    .text(label)
                    .fill(color)
                    .scale_to(ticks.number_scale)
                    .move_to(anchor.x + offset.x, anchor.y + offset.y);
                let mut spec = handle.spec.lock().expect("number spec poisoned");
                spec.coordinate_view_role = Some(gaanim_scene::CoordinateViewRole::Label);
                spec.coordinate_label_offset = Some(*offset);
                drop(spec);
                handle
            })
            .collect()
    }

    /// A path styled like generation 0's `reference`, wrapped for fading and
    /// attached to `layer`.
    #[allow(clippy::too_many_arguments)]
    fn tick_path(
        &mut self,
        path: BezPath,
        reference: &DrawableHandle,
        layer: &DrawableHandle,
        name: &str,
        axis: u8,
        generation: u32,
        anchors: &TickAnchors,
        line_mask: &DrawableHandle,
    ) -> DrawableHandle {
        let bounds = path_bounds(&path, reference);
        let handle = self.visualization_path(path, bounds, Color::TRANSPARENT, 0.0, name);
        copy_stroke(reference, &handle);
        let handle = handle.clip(line_mask, gaanim_core::peniko::Fill::NonZero);
        let wrapper = self.group_no_center(&[&handle]);
        mark_level(&wrapper, axis, generation, anchors);
        self.attach_tick_part(layer, &wrapper);
        handle
    }

    fn attach_tick_part(&mut self, layer: &DrawableHandle, part: &DrawableHandle) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachToGroupLocal {
                group: layer.id,
                child: part.id,
            });
    }

    /// Give generation 0 separate x and y paths so each axis can fade alone.
    fn split_joint_ticks(&mut self, ticks: &mut CartesianTicks) -> Result<(), VisualizationError> {
        let Some(joint) = ticks.joint.take() else {
            return Ok(());
        };
        let frame = ticks.space.map.frame;
        let crosses = [
            (-frame.height * 0.5, frame.height * 0.5),
            (-frame.width * 0.5, frame.width * 0.5),
        ];
        let mut parts = Vec::with_capacity(2);
        for (axis, cross) in crosses.into_iter().enumerate() {
            let generation = &ticks.axes[axis].generations[0];
            parts.push(self.axis_part_geometry(
                ticks,
                axis,
                generation.step,
                generation.coverage,
                cross,
            )?);
        }
        let y_part = parts.pop().expect("y tick geometry");
        let x_part = parts.pop().expect("x tick geometry");
        // The joint paths become the x half of generation 0.
        for (path, geometry) in joint
            .iter()
            .zip([&x_part.major, &x_part.minor, &x_part.ticks])
        {
            replace_path(path, geometry.clone());
        }
        let anchors = ticks.axes[1].anchors.clone();
        let line_mask = ticks.line_mask.clone();
        let names = [
            "CoordinateMajorGrid",
            "CoordinateMinorGrid",
            "CoordinateTicks",
        ];
        let mut y_paths = Vec::with_capacity(3);
        for (index, geometry) in [y_part.major, y_part.minor, y_part.ticks]
            .into_iter()
            .enumerate()
        {
            let layer = ticks.layers[index].clone();
            y_paths.push(self.tick_path(
                geometry,
                &joint[index],
                &layer,
                names[index],
                1,
                0,
                &anchors,
                &line_mask,
            ));
        }
        ticks.axes[0].generations[0].paths = Some(joint);
        ticks.axes[1].generations[0].paths = Some(y_paths.try_into().expect("three y tick paths"));
        Ok(())
    }

    fn push_tick_generation(
        &mut self,
        ticks: &mut CartesianTicks,
        axis: usize,
        step: f64,
        coverage: (f64, f64),
    ) -> Result<usize, VisualizationError> {
        let id = ticks.axes[axis].generations.len() as u32;
        let cross = ticks.reach[axis];
        let geometry = self.axis_part_geometry(ticks, axis, step, coverage, cross)?;
        let anchors = ticks.axes[axis].anchors.clone();
        let reference = ticks.axes[axis].generations[0]
            .paths
            .clone()
            .expect("generation 0 is split before regeneration");
        let line_mask = ticks.line_mask.clone();
        let names = [
            "CoordinateMajorGrid",
            "CoordinateMinorGrid",
            "CoordinateTicks",
        ];
        let mut paths = Vec::with_capacity(3);
        for (index, path) in [geometry.major, geometry.minor, geometry.ticks]
            .into_iter()
            .enumerate()
        {
            let layer = ticks.layers[index].clone();
            paths.push(self.tick_path(
                path,
                &reference[index],
                &layer,
                names[index],
                axis as u8,
                id,
                &anchors,
                &line_mask,
            ));
        }
        let axis_line = self.tick_axis_line(ticks, axis, id, coverage)?;
        let number_handles = self.tick_number_drawables(ticks, axis, &geometry.numbers);
        let refs: Vec<&DrawableHandle> = number_handles.iter().collect();
        // Clips reach only the members a group has when it is clipped.
        let numbers = self
            .group_no_center(&refs)
            .clip(&ticks.number_mask, gaanim_core::peniko::Fill::NonZero);
        mark_level(&numbers, axis as u8, id, &anchors);
        let numbers_layer = ticks.layers[3].clone();
        self.attach_tick_part(&numbers_layer, &numbers);
        ticks.axes[axis].generations.push(TickGeneration {
            id,
            step,
            coverage,
            paths: Some(paths.try_into().expect("three tick paths")),
            axis_line,
            cross,
            numbers,
            number_values: geometry.numbers.iter().map(|number| number.0).collect(),
        });
        Ok(ticks.axes[axis].generations.len() - 1)
    }

    /// Widen a generation to cover a panned window with the same step.
    fn extend_tick_generation(
        &mut self,
        ticks: &mut CartesianTicks,
        axis: usize,
        index: usize,
        target: (f64, f64),
    ) -> Result<(), VisualizationError> {
        let generation = &ticks.axes[axis].generations[index];
        let padded = padded(target);
        let coverage = (
            generation.coverage.0.min(padded.0),
            generation.coverage.1.max(padded.1),
        );
        let step = generation.step;
        let cross = union(generation.cross, ticks.reach[axis]);
        let geometry = self.axis_part_geometry(ticks, axis, step, coverage, cross)?;
        let paths = generation
            .paths
            .clone()
            .expect("extended generations have split paths");
        for (path, part) in paths
            .iter()
            .zip([geometry.major, geometry.minor, geometry.ticks])
        {
            replace_path(path, part);
        }
        let known = generation.number_values.clone();
        let generation_id = generation.id;
        match generation.axis_line.clone() {
            Some(line) => {
                if let Some(path) = self.axis_line_path(ticks, axis, coverage)? {
                    replace_path(&line, path);
                }
            }
            None => {
                let line = self.tick_axis_line(ticks, axis, generation_id, coverage)?;
                ticks.axes[axis].generations[index].axis_line = line;
            }
        }
        let fresh: Vec<_> = geometry
            .numbers
            .into_iter()
            .filter(|number| !known.iter().any(|value| same_step(*value, number.0)))
            .collect();
        let handles = self.tick_number_drawables(ticks, axis, &fresh);
        let numbers = ticks.axes[axis].generations[index].numbers.clone();
        let anchors = ticks.axes[axis].anchors.clone();
        for handle in &handles {
            let handle = &handle
                .clone()
                .clip(&ticks.number_mask, gaanim_core::peniko::Fill::NonZero);
            if index == 0 {
                // Generation 0 fades per number inside the flat public layer.
                mark_level(handle, axis as u8, 0, &anchors);
            }
            self.attach_tick_part(&numbers, handle);
        }
        let generation = &mut ticks.axes[axis].generations[index];
        generation.coverage = coverage;
        generation.cross = cross;
        generation
            .number_values
            .extend(fresh.iter().map(|number| number.0));
        Ok(())
    }

    /// The axis line of `axis` over `coverage`, or `None` when the line is
    /// hidden or the original line already spans the coverage.
    fn axis_line_path(
        &self,
        ticks: &CartesianTicks,
        axis: usize,
        coverage: (f64, f64),
    ) -> Result<Option<BezPath>, VisualizationError> {
        let map = &ticks.space.map;
        let (visible, original) = if axis == 0 {
            (ticks.space.visibility.x_axis, map.x.domain())
        } else {
            (ticks.space.visibility.y_axis, map.y.domain())
        };
        if !visible || covers(original, coverage) {
            return Ok(None);
        }
        let (x_cross, y_cross) = (map.x.crossing_value(), map.y.crossing_value());
        let mut path = BezPath::new();
        if axis == 0 {
            path.move_to(map.data_to_local(coverage.0, y_cross)?);
            path.line_to(map.data_to_local(coverage.1, y_cross)?);
        } else {
            path.move_to(map.data_to_local(x_cross, coverage.0)?);
            path.line_to(map.data_to_local(x_cross, coverage.1)?);
        }
        Ok(Some(path))
    }

    /// A faded axis line for a generation reaching beyond the original domain.
    fn tick_axis_line(
        &mut self,
        ticks: &CartesianTicks,
        axis: usize,
        generation: u32,
        coverage: (f64, f64),
    ) -> Result<Option<DrawableHandle>, VisualizationError> {
        let Some(path) = self.axis_line_path(ticks, axis, coverage)? else {
            return Ok(None);
        };
        let anchors = ticks.axes[axis].anchors.clone();
        let reference = ticks.axis_line.clone();
        let layer = ticks.layers[2].clone();
        let line_mask = ticks.line_mask.clone();
        Ok(Some(self.tick_path(
            path,
            &reference,
            &layer,
            "CoordinateAxes",
            axis as u8,
            generation,
            &anchors,
            &line_mask,
        )))
    }
}

fn union(left: (f64, f64), right: (f64, f64)) -> (f64, f64) {
    (left.0.min(right.0), left.1.max(right.1))
}

fn replace_path(handle: &DrawableHandle, path: BezPath) {
    let update = |kind: &mut SpawnKind| {
        if let SpawnKind::SvgPath(svg) = kind {
            if !path.elements().is_empty() {
                let bounds = path.bounding_box();
                svg.bounds = svg.bounds.union(&Bounds3D::new_2d(
                    bounds.x0, bounds.y0, bounds.x1, bounds.y1,
                ));
            }
            svg.path = path.clone();
        }
    };
    update(&mut handle.spec.lock().expect("tick path spec poisoned").kind);
    // Compilation uses the declaration frozen by the first play or wait.
    if let Some(frozen) = handle
        .state
        .lock()
        .expect("canvas state poisoned")
        .frozen_spawn_specs
        .get_mut(&handle.id)
    {
        update(&mut frozen.kind);
    }
}

fn path_bounds(path: &BezPath, reference: &DrawableHandle) -> Bounds3D {
    let base = match &reference.spec.lock().expect("tick path spec poisoned").kind {
        SpawnKind::SvgPath(svg) => svg.bounds,
        _ => Bounds3D::new_2d(0.0, 0.0, 0.0, 0.0),
    };
    if path.elements().is_empty() {
        return base;
    }
    let bounds = path.bounding_box();
    base.union(&Bounds3D::new_2d(
        bounds.x0, bounds.y0, bounds.x1, bounds.y1,
    ))
}

/// Copy the stroke and theme state of `reference`, including user styling
/// applied to generation 0 before the view change.
fn copy_stroke(reference: &DrawableHandle, target: &DrawableHandle) {
    let (kind_stroke, stroke, stroke_style, overridden, selector) = {
        let spec = reference.spec.lock().expect("tick path spec poisoned");
        let kind_stroke = match &spec.kind {
            SpawnKind::SvgPath(svg) => Some(svg.stroke.clone()),
            _ => None,
        };
        (
            kind_stroke,
            spec.stroke.clone(),
            spec.stroke_style.clone(),
            spec.stroke_overridden,
            spec.theme_selector.clone(),
        )
    };
    let mut spec = target.spec.lock().expect("tick path spec poisoned");
    if let (SpawnKind::SvgPath(svg), Some(kind_stroke)) = (&mut spec.kind, kind_stroke) {
        svg.stroke = kind_stroke;
    }
    spec.stroke = stroke;
    spec.stroke_style = stroke_style;
    spec.stroke_overridden = overridden;
    spec.theme_selector = selector;
}

#[cfg(test)]
mod tests {
    use gaanim_visualization::Axis;

    use super::*;

    #[test]
    fn regenerated_parts_stack_with_their_generation_zero_layer() {
        let mut canvas = SceneModel::new(16.0, 9.0);
        let space = canvas
            .coordinate_axes(
                Axis::linear(0.0, 0.4).unwrap(),
                Axis::linear(0.0, 4.0).unwrap(),
                Some(9.0),
                Some(6.0),
                true,
            )
            .unwrap();
        canvas.wait(0.2);
        canvas
            .coordinate_view_to(&space, (0.0, 0.2), (0.0, 4.0))
            .unwrap();

        let ticks = space.ticks.as_ref().unwrap().lock().unwrap();
        let generation = &ticks.axes[0].generations[1];
        let [major, _, tick] = generation.paths.clone().unwrap();
        let number = match &generation.numbers.spec.lock().unwrap().kind {
            SpawnKind::Group(members) | SpawnKind::GroupNoCenter(members) => members[0],
            other => panic!("expected a numbers group, got {other:?}"),
        };
        let state = canvas.state.lock().unwrap();
        let ops = &state.segments[space.view.segment_idx].ops;
        let at = |id: ObjectId| {
            ops.iter()
                .position(|op| matches!(op, Op::Spawn(spec) if spec.lock().unwrap().id == id))
                .unwrap()
        };
        // Spawn order breaks render-order ties: grid < axes < ticks < numbers.
        assert!(at(major.id) < at(ticks.axis_line.id));
        assert!(at(ticks.axis_line.id) < at(tick.id));
        assert!(at(tick.id) < at(ticks.stack_before[1]));
        assert!(at(ticks.stack_before[1]) < at(number));
        assert!(at(number) < at(ticks.layers[3].id));
    }
}
