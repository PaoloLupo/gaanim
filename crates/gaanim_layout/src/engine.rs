//! Deterministic, renderer-independent layout tree and relational solver.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Add, Div, Mul, Neg, Sub};

use gaanim_core::glam::{DVec2, DVec3};
use gaanim_math::Bounds3D;
use kasuari::{Constraint as SolverConstraint, Expression as SolverExpression, RelationalOperator};
use kasuari::{Solver, Strength, Variable};

use crate::Anchor;

const EPSILON: f64 = 1.0e-6;

/// Stable identity for one node in a layout tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutId(pub u64);

/// How a box chooses its size on one axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeRule {
    Hug,
    Fill(f64),
    Fixed(f64),
}

impl Default for SizeRule {
    fn default() -> Self {
        Self::Hug
    }
}

impl SizeRule {
    fn sanitize(self) -> Self {
        match self {
            Self::Hug => Self::Hug,
            Self::Fill(weight) => Self::Fill(weight.max(EPSILON)),
            Self::Fixed(value) => Self::Fixed(value.max(0.0)),
        }
    }
}

/// Grid track sizing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Track {
    Fixed(f64),
    Auto,
    Fraction(f64),
}

impl Track {
    fn sanitize(self) -> Self {
        match self {
            Self::Fixed(value) => Self::Fixed(value.max(0.0)),
            Self::Auto => Self::Auto,
            Self::Fraction(weight) => Self::Fraction(weight.max(EPSILON)),
        }
    }
}

/// CSS-style top/right/bottom/left insets in scene units.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Insets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Insets {
    pub const fn all(value: f64) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub const fn symmetric(vertical: f64, horizontal: f64) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    fn sanitized(self) -> Self {
        Self {
            top: self.top.max(0.0),
            right: self.right.max(0.0),
            bottom: self.bottom.max(0.0),
            left: self.left.max(0.0),
        }
    }

    fn horizontal(self) -> f64 {
        self.left + self.right
    }

    fn vertical(self) -> f64 {
        self.top + self.bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    Between,
    Around,
    Evenly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FitMode {
    #[default]
    None,
    Contain,
    Cover,
    Stretch,
    ScaleDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutoFlow {
    #[default]
    Row,
    Column,
}

/// Layout algorithm owned by a container node.
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutNodeKind {
    Leaf,
    Row {
        wrap: bool,
    },
    Column {
        wrap: bool,
    },
    Grid {
        rows: Vec<Track>,
        columns: Vec<Track>,
        auto_flow: AutoFlow,
    },
    Stack,
}

/// Container-level style.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutStyle {
    pub width: SizeRule,
    pub height: SizeRule,
    pub min_width: Option<f64>,
    pub max_width: Option<f64>,
    pub min_height: Option<f64>,
    pub max_height: Option<f64>,
    pub padding: Insets,
    pub gap: DVec2,
    pub align: Align,
    pub justify: Justify,
    pub aspect_ratio: Option<f64>,
}

impl Default for LayoutStyle {
    fn default() -> Self {
        Self {
            width: SizeRule::Hug,
            height: SizeRule::Hug,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            padding: Insets::default(),
            gap: DVec2::ZERO,
            align: Align::Start,
            justify: Justify::Start,
            aspect_ratio: None,
        }
    }
}

impl LayoutStyle {
    fn sanitized(&self) -> Self {
        let finite =
            |value: Option<f64>| value.filter(|value| value.is_finite()).map(|v| v.max(0.0));
        Self {
            width: self.width.sanitize(),
            height: self.height.sanitize(),
            min_width: finite(self.min_width),
            max_width: finite(self.max_width),
            min_height: finite(self.min_height),
            max_height: finite(self.max_height),
            padding: self.padding.sanitized(),
            gap: self.gap.max(DVec2::ZERO),
            align: self.align,
            justify: self.justify,
            aspect_ratio: self
                .aspect_ratio
                .filter(|value| value.is_finite() && *value > 0.0),
        }
    }
}

/// Placement metadata owned by the parent for one child.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutItemStyle {
    pub grow: f64,
    pub shrink: f64,
    pub align: Option<Align>,
    pub row: Option<usize>,
    pub column: Option<usize>,
    pub row_span: usize,
    pub column_span: usize,
    pub absolute: bool,
    pub anchor: Anchor,
    pub offset: DVec3,
    pub fit: FitMode,
}

impl Default for LayoutItemStyle {
    fn default() -> Self {
        Self {
            grow: 0.0,
            shrink: 1.0,
            align: None,
            row: None,
            column: None,
            row_span: 1,
            column_span: 1,
            absolute: false,
            anchor: Anchor::Center,
            offset: DVec3::ZERO,
            fit: FitMode::None,
        }
    }
}

/// One child edge in the tree.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutChild {
    pub node: Box<LayoutNode>,
    pub style: LayoutItemStyle,
}

/// Declarative layout node. Leaves are measured by [`IntrinsicMeasure`].
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutNode {
    pub id: LayoutId,
    pub kind: LayoutNodeKind,
    pub style: LayoutStyle,
    pub children: Vec<LayoutChild>,
}

impl LayoutNode {
    pub fn leaf(id: LayoutId) -> Self {
        Self {
            id,
            kind: LayoutNodeKind::Leaf,
            style: LayoutStyle::default(),
            children: Vec::new(),
        }
    }

    pub fn container(id: LayoutId, kind: LayoutNodeKind, children: Vec<LayoutChild>) -> Self {
        Self {
            id,
            kind,
            style: LayoutStyle::default(),
            children,
        }
    }
}

/// Minimum and maximum size offered to a measurable leaf.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxConstraints {
    pub min: DVec2,
    pub max: DVec2,
}

impl BoxConstraints {
    pub fn tight(size: DVec2) -> Self {
        let size = size.max(DVec2::ZERO);
        Self {
            min: size,
            max: size,
        }
    }

    pub fn loosen(self) -> Self {
        Self {
            min: DVec2::ZERO,
            max: self.max,
        }
    }

    pub fn constrain(self, size: DVec2) -> DVec2 {
        size.max(self.min).min(self.max)
    }
}

/// Bridge implemented by the API/compiler for text, vector and media leaves.
pub trait IntrinsicMeasure {
    fn measure(&self, id: LayoutId, constraints: BoxConstraints) -> Result<DVec2, LayoutError>;

    /// Whether measuring this leaf can change when its offered width changes.
    /// Paragraphs and other wrapping content should return `true`.
    fn is_width_sensitive(&self, _id: LayoutId) -> bool {
        false
    }
}

/// Final geometry for one node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedBox {
    pub bounds: Bounds3D,
    pub clip: Option<Bounds3D>,
    pub scale: DVec3,
}

impl ResolvedBox {
    pub fn size(self) -> DVec2 {
        DVec2::new(self.bounds.width(), self.bounds.height())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutDiagnostic {
    pub constraint: usize,
    pub residual: f64,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedLayout {
    pub boxes: BTreeMap<LayoutId, ResolvedBox>,
    pub diagnostics: Vec<LayoutDiagnostic>,
    pub iterations: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("layout node {0:?} is declared more than once")]
    DuplicateNode(LayoutId),
    #[error("layout references unknown node {0:?}")]
    UnknownNode(LayoutId),
    #[error("grid needs at least one row and one column")]
    EmptyGrid,
    #[error("grid item for node {0:?} is outside the configured tracks")]
    GridOutOfBounds(LayoutId),
    #[error("grid item for node {0:?} overlaps another explicitly placed item")]
    GridCollision(LayoutId),
    #[error("grid has no free cell for node {0:?}")]
    GridNoSpace(LayoutId),
    #[error("required layout constraints are incompatible: {0}")]
    Unsatisfiable(String),
    #[error("layout did not converge after {iterations} measurement passes")]
    NonConvergentLayout { iterations: usize },
    #[error("intrinsic measurement failed for node {id:?}: {message}")]
    Measure { id: LayoutId, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LayoutAttribute {
    Left,
    Right,
    Top,
    Bottom,
    CenterX,
    CenterY,
    Width,
    Height,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutVariable {
    pub node: LayoutId,
    pub attribute: LayoutAttribute,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayoutExpression {
    pub constant: f64,
    pub terms: BTreeMap<LayoutVariable, f64>,
}

impl LayoutExpression {
    pub fn variable(node: LayoutId, attribute: LayoutAttribute) -> Self {
        Self {
            constant: 0.0,
            terms: BTreeMap::from([(LayoutVariable { node, attribute }, 1.0)]),
        }
    }

    fn value(&self, layout: &ResolvedLayout) -> Result<f64, LayoutError> {
        let mut value = self.constant;
        for (variable, coefficient) in &self.terms {
            let box_ = layout
                .boxes
                .get(&variable.node)
                .ok_or(LayoutError::UnknownNode(variable.node))?;
            value += coefficient * attribute_value(*box_, variable.attribute);
        }
        Ok(value)
    }
}

impl From<f64> for LayoutExpression {
    fn from(value: f64) -> Self {
        Self {
            constant: value,
            terms: BTreeMap::new(),
        }
    }
}

impl Add for LayoutExpression {
    type Output = Self;
    fn add(mut self, rhs: Self) -> Self {
        self.constant += rhs.constant;
        for (term, coefficient) in rhs.terms {
            *self.terms.entry(term).or_default() += coefficient;
        }
        self
    }
}

impl Sub for LayoutExpression {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        self + -rhs
    }
}

impl Neg for LayoutExpression {
    type Output = Self;
    fn neg(mut self) -> Self {
        self.constant = -self.constant;
        for coefficient in self.terms.values_mut() {
            *coefficient = -*coefficient;
        }
        self
    }
}

impl Mul<f64> for LayoutExpression {
    type Output = Self;
    fn mul(mut self, rhs: f64) -> Self {
        self.constant *= rhs;
        for coefficient in self.terms.values_mut() {
            *coefficient *= rhs;
        }
        self
    }
}

impl Div<f64> for LayoutExpression {
    type Output = Self;
    fn div(self, rhs: f64) -> Self {
        self * rhs.recip()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintRelation {
    Equal,
    LessOrEqual,
    GreaterOrEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConstraintStrength {
    #[default]
    Required,
    Strong,
    Medium,
    Weak,
}

impl ConstraintStrength {
    fn solver(self) -> Strength {
        match self {
            Self::Required => Strength::REQUIRED,
            Self::Strong => Strength::STRONG,
            Self::Medium => Strength::MEDIUM,
            Self::Weak => Strength::WEAK,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutConstraint {
    pub lhs: LayoutExpression,
    pub relation: ConstraintRelation,
    pub rhs: LayoutExpression,
    pub strength: ConstraintStrength,
    pub label: Option<String>,
}

impl LayoutConstraint {
    pub fn equal(lhs: LayoutExpression, rhs: LayoutExpression) -> Self {
        Self {
            lhs,
            relation: ConstraintRelation::Equal,
            rhs,
            strength: ConstraintStrength::Required,
            label: None,
        }
    }

    pub fn with_strength(mut self, strength: ConstraintStrength) -> Self {
        self.strength = strength;
        self
    }
}

/// Resolve a tree and then apply relational constraints. Width-sensitive leaf
/// measurement is repeated until geometry stabilizes.
pub fn resolve_layout(
    root: &LayoutNode,
    viewport: Bounds3D,
    measurer: &impl IntrinsicMeasure,
    relations: &[LayoutConstraint],
) -> Result<ResolvedLayout, LayoutError> {
    validate_tree(root)?;
    let mut previous = BTreeMap::new();
    for iteration in 1..=8 {
        let mut resolved = ResolvedLayout::default();
        let available = DVec2::new(viewport.width(), viewport.height());
        let size = measure_node(
            root,
            BoxConstraints {
                min: DVec2::ZERO,
                max: available,
            },
            measurer,
            &mut resolved,
        )?;
        place_node(
            root,
            DVec2::new(viewport.center().x, viewport.center().y),
            size,
            measurer,
            &mut resolved,
        )?;
        apply_relations(&mut resolved, relations)?;
        resolved.iterations = iteration;

        let stable = resolved.boxes.iter().all(|(id, box_)| {
            previous.get(id).is_some_and(|old: &ResolvedBox| {
                (old.bounds.min - box_.bounds.min).length() <= EPSILON
                    && (old.bounds.max - box_.bounds.max).length() <= EPSILON
            })
        }) && previous.len() == resolved.boxes.len();
        if stable || iteration == 1 && !contains_width_sensitive(root, measurer) {
            return Ok(resolved);
        }
        previous = resolved.boxes.clone();
        if iteration == 8 {
            return Err(LayoutError::NonConvergentLayout { iterations: 8 });
        }
    }
    unreachable!()
}

/// Apply relational constraints to an already resolved set of boxes.
///
/// This is useful for constraints that cross independent layout roots. The
/// same deterministic variable ordering, explicit weak stays and diagnostic
/// reporting used by [`resolve_layout`] are preserved.
pub fn solve_constraints(
    layout: &mut ResolvedLayout,
    relations: &[LayoutConstraint],
) -> Result<(), LayoutError> {
    apply_relations(layout, relations)
}

fn contains_width_sensitive(node: &LayoutNode, measurer: &impl IntrinsicMeasure) -> bool {
    matches!(node.style.width, SizeRule::Fill(_))
        || matches!(node.kind, LayoutNodeKind::Leaf) && measurer.is_width_sensitive(node.id)
        || node
            .children
            .iter()
            .any(|child| contains_width_sensitive(&child.node, measurer))
}

fn validate_tree(root: &LayoutNode) -> Result<(), LayoutError> {
    fn visit(node: &LayoutNode, ids: &mut BTreeSet<LayoutId>) -> Result<(), LayoutError> {
        if !ids.insert(node.id) {
            return Err(LayoutError::DuplicateNode(node.id));
        }
        if let LayoutNodeKind::Grid { rows, columns, .. } = &node.kind
            && (rows.is_empty() || columns.is_empty())
        {
            return Err(LayoutError::EmptyGrid);
        }
        for child in &node.children {
            visit(&child.node, ids)?;
        }
        Ok(())
    }
    visit(root, &mut BTreeSet::new())
}

fn measure_node(
    node: &LayoutNode,
    constraints: BoxConstraints,
    measurer: &impl IntrinsicMeasure,
    resolved: &mut ResolvedLayout,
) -> Result<DVec2, LayoutError> {
    let style = node.style.sanitized();
    let inner_max = (constraints.max
        - DVec2::new(style.padding.horizontal(), style.padding.vertical()))
    .max(DVec2::ZERO);
    let child_constraints = BoxConstraints {
        min: DVec2::ZERO,
        max: inner_max,
    };
    let intrinsic = match &node.kind {
        LayoutNodeKind::Leaf => measurer.measure(node.id, child_constraints)?,
        LayoutNodeKind::Row { .. } => {
            let mut width: f64 = 0.0;
            let mut height: f64 = 0.0;
            let mut count = 0usize;
            for child in node.children.iter().filter(|child| !child.style.absolute) {
                let size = measure_node(&child.node, child_constraints, measurer, resolved)?;
                width += size.x;
                height = height.max(size.y);
                count += 1;
            }
            width += style.gap.x * count.saturating_sub(1) as f64;
            DVec2::new(width, height)
        }
        LayoutNodeKind::Column { .. } => {
            let mut width: f64 = 0.0;
            let mut height: f64 = 0.0;
            let mut count = 0usize;
            for child in node.children.iter().filter(|child| !child.style.absolute) {
                let size = measure_node(&child.node, child_constraints, measurer, resolved)?;
                width = width.max(size.x);
                height += size.y;
                count += 1;
            }
            height += style.gap.y * count.saturating_sub(1) as f64;
            DVec2::new(width, height)
        }
        LayoutNodeKind::Grid {
            rows,
            columns,
            auto_flow,
        } => {
            let (widths, heights) = grid_tracks(
                node, rows, columns, *auto_flow, inner_max, measurer, resolved,
            )?;
            DVec2::new(
                widths.iter().sum::<f64>() + style.gap.x * widths.len().saturating_sub(1) as f64,
                heights.iter().sum::<f64>() + style.gap.y * heights.len().saturating_sub(1) as f64,
            )
        }
        LayoutNodeKind::Stack => {
            let mut size = DVec2::ZERO;
            for child in node.children.iter().filter(|child| !child.style.absolute) {
                size = size.max(measure_node(
                    &child.node,
                    child_constraints,
                    measurer,
                    resolved,
                )?);
            }
            size
        }
    } + DVec2::new(style.padding.horizontal(), style.padding.vertical());

    let mut size = DVec2::new(
        resolve_axis(style.width, intrinsic.x, constraints.max.x),
        resolve_axis(style.height, intrinsic.y, constraints.max.y),
    );
    size.x = clamp_axis(size.x, style.min_width, style.max_width);
    size.y = clamp_axis(size.y, style.min_height, style.max_height);
    if let Some(ratio) = style.aspect_ratio {
        if matches!(style.width, SizeRule::Fixed(_) | SizeRule::Fill(_)) {
            size.y = size.x / ratio;
        } else {
            size.x = size.y * ratio;
        }
    }
    size.x = clamp_axis(size.x, style.min_width, style.max_width);
    size.y = clamp_axis(size.y, style.min_height, style.max_height);
    Ok(constraints.constrain(size))
}

fn resolve_axis(rule: SizeRule, intrinsic: f64, available: f64) -> f64 {
    match rule.sanitize() {
        SizeRule::Hug => intrinsic,
        SizeRule::Fill(_) => available,
        SizeRule::Fixed(value) => value,
    }
}

fn clamp_axis(value: f64, min: Option<f64>, max: Option<f64>) -> f64 {
    value
        .max(min.unwrap_or(0.0))
        .min(max.unwrap_or(f64::INFINITY))
}

fn place_node(
    node: &LayoutNode,
    center: DVec2,
    size: DVec2,
    measurer: &impl IntrinsicMeasure,
    resolved: &mut ResolvedLayout,
) -> Result<(), LayoutError> {
    let style = node.style.sanitized();
    let bounds = Bounds3D::new_2d(
        center.x - size.x * 0.5,
        center.y - size.y * 0.5,
        center.x + size.x * 0.5,
        center.y + size.y * 0.5,
    );
    resolved.boxes.insert(
        node.id,
        ResolvedBox {
            bounds,
            clip: None,
            scale: DVec3::ONE,
        },
    );
    if matches!(node.kind, LayoutNodeKind::Leaf) {
        return Ok(());
    }
    let content = Bounds3D::new_2d(
        bounds.min.x + style.padding.left,
        bounds.min.y + style.padding.bottom,
        bounds.max.x - style.padding.right,
        bounds.max.y - style.padding.top,
    );
    match &node.kind {
        LayoutNodeKind::Row { wrap } => {
            place_linear(node, content, true, *wrap, measurer, resolved)?
        }
        LayoutNodeKind::Column { wrap } => {
            place_linear(node, content, false, *wrap, measurer, resolved)?
        }
        LayoutNodeKind::Grid {
            rows,
            columns,
            auto_flow,
        } => place_grid(node, content, rows, columns, *auto_flow, measurer, resolved)?,
        LayoutNodeKind::Stack => {
            for child in node.children.iter().filter(|child| !child.style.absolute) {
                place_overlay(child, content, measurer, resolved)?;
            }
        }
        LayoutNodeKind::Leaf => {}
    }
    for child in node.children.iter().filter(|child| child.style.absolute) {
        place_overlay(child, content, measurer, resolved)?;
    }
    Ok(())
}

fn place_linear(
    node: &LayoutNode,
    content: Bounds3D,
    horizontal: bool,
    wrap: bool,
    measurer: &impl IntrinsicMeasure,
    resolved: &mut ResolvedLayout,
) -> Result<(), LayoutError> {
    let style = node.style.sanitized();
    let children: Vec<_> = node
        .children
        .iter()
        .filter(|child| !child.style.absolute)
        .collect();
    if children.is_empty() {
        return Ok(());
    }
    let available = DVec2::new(content.width(), content.height());
    let mut sizes = Vec::with_capacity(children.len());
    for child in &children {
        sizes.push(measure_node(
            &child.node,
            BoxConstraints {
                min: DVec2::ZERO,
                max: available,
            },
            measurer,
            resolved,
        )?);
    }
    let main_available = if horizontal { available.x } else { available.y };
    let gap = if horizontal { style.gap.x } else { style.gap.y };
    let initial_main_used: f64 = sizes
        .iter()
        .map(|size| if horizontal { size.x } else { size.y })
        .sum::<f64>()
        + gap * children.len().saturating_sub(1) as f64;
    if wrap && initial_main_used > main_available {
        return place_wrapped(
            node, content, horizontal, &children, &sizes, measurer, resolved,
        );
    }

    let grow_weights: Vec<f64> = children
        .iter()
        .map(|child| {
            if child.style.grow > 0.0 {
                child.style.grow
            } else {
                let rule = if horizontal {
                    child.node.style.width
                } else {
                    child.node.style.height
                };
                match rule {
                    SizeRule::Fill(weight) => weight.max(0.0),
                    _ => 0.0,
                }
            }
        })
        .collect();
    let grow_total: f64 = grow_weights.iter().sum();
    if grow_total > 0.0 {
        let fixed = sizes
            .iter()
            .zip(&grow_weights)
            .filter(|(_, weight)| **weight <= 0.0)
            .map(|(size, _)| if horizontal { size.x } else { size.y })
            .sum::<f64>()
            + gap * children.len().saturating_sub(1) as f64;
        let flexible = (main_available - fixed).max(0.0);
        for ((child, size), weight) in children.iter().zip(&mut sizes).zip(&grow_weights) {
            if *weight <= 0.0 {
                continue;
            }
            let assigned = flexible * *weight / grow_total;
            let max = if horizontal {
                DVec2::new(assigned, available.y)
            } else {
                DVec2::new(available.x, assigned)
            };
            let measured = measure_node(
                &child.node,
                BoxConstraints {
                    min: DVec2::ZERO,
                    max,
                },
                measurer,
                resolved,
            )?;
            if horizontal {
                size.x = assigned;
                size.y = measured.y;
            } else {
                size.x = measured.x;
                size.y = assigned;
            }
        }
    } else if initial_main_used > main_available {
        let deficit = initial_main_used - main_available;
        let shrink_total = children
            .iter()
            .zip(&sizes)
            .map(|(child, size)| {
                child.style.shrink.max(0.0) * if horizontal { size.x } else { size.y }
            })
            .sum::<f64>();
        if shrink_total > 0.0 {
            for (child, size) in children.iter().zip(&mut sizes) {
                let main = if horizontal { size.x } else { size.y };
                let contribution = child.style.shrink.max(0.0) * main;
                let assigned = (main - deficit * contribution / shrink_total).max(0.0);
                let max = if horizontal {
                    DVec2::new(assigned, available.y)
                } else {
                    DVec2::new(available.x, assigned)
                };
                let measured = measure_node(
                    &child.node,
                    BoxConstraints {
                        min: DVec2::ZERO,
                        max,
                    },
                    measurer,
                    resolved,
                )?;
                if horizontal {
                    size.x = assigned;
                    size.y = measured.y;
                } else {
                    size.x = measured.x;
                    size.y = assigned;
                }
            }
        }
    }
    let used: f64 = sizes
        .iter()
        .map(|size| if horizontal { size.x } else { size.y })
        .sum::<f64>()
        + gap * children.len().saturating_sub(1) as f64;
    let (mut cursor, actual_gap) = justify_cursor(
        node.style.justify,
        content,
        horizontal,
        used,
        gap,
        children.len(),
    );
    for ((child, size), index) in children.iter().zip(sizes).zip(0..) {
        let main = if horizontal { size.x } else { size.y };
        let cross = aligned_cross(
            content,
            horizontal,
            size,
            child.style.align.unwrap_or(style.align),
        );
        let center = if horizontal {
            DVec2::new(cursor + main * 0.5, cross)
        } else {
            DVec2::new(cross, cursor - main * 0.5)
        } + child.style.offset.truncate();
        let mut placed_size = size;
        if child.style.align.unwrap_or(style.align) == Align::Stretch {
            if horizontal {
                placed_size.y = content.height()
            } else {
                placed_size.x = content.width()
            }
        }
        place_node(&child.node, center, placed_size, measurer, resolved)?;
        cursor += if horizontal {
            main + actual_gap
        } else {
            -(main + actual_gap)
        };
        let _ = index;
    }
    Ok(())
}

fn place_wrapped(
    node: &LayoutNode,
    content: Bounds3D,
    horizontal: bool,
    children: &[&LayoutChild],
    sizes: &[DVec2],
    measurer: &impl IntrinsicMeasure,
    resolved: &mut ResolvedLayout,
) -> Result<(), LayoutError> {
    let gap = node.style.gap;
    let mut cursor = DVec2::new(content.min.x, content.max.y);
    let mut line_cross: f64 = 0.0;
    let mut first_in_line = true;
    for (child, size) in children.iter().zip(sizes) {
        let main_gap = if first_in_line {
            0.0
        } else if horizontal {
            gap.x
        } else {
            gap.y
        };
        if horizontal && !first_in_line && cursor.x + main_gap + size.x > content.max.x {
            cursor.x = content.min.x;
            cursor.y -= line_cross + gap.y;
            line_cross = 0.0;
            first_in_line = true;
        } else if !horizontal && !first_in_line && cursor.y - main_gap - size.y < content.min.y {
            cursor.y = content.max.y;
            cursor.x += line_cross + gap.x;
            line_cross = 0.0;
            first_in_line = true;
        }
        if !first_in_line {
            if horizontal {
                cursor.x += gap.x;
            } else {
                cursor.y -= gap.y;
            }
        }
        let center = if horizontal {
            DVec2::new(cursor.x + size.x * 0.5, cursor.y - size.y * 0.5)
        } else {
            DVec2::new(cursor.x + size.x * 0.5, cursor.y - size.y * 0.5)
        } + child.style.offset.truncate();
        place_node(&child.node, center, *size, measurer, resolved)?;
        if horizontal {
            cursor.x += size.x;
            line_cross = line_cross.max(size.y);
        } else {
            cursor.y -= size.y;
            line_cross = line_cross.max(size.x);
        }
        first_in_line = false;
    }
    Ok(())
}

fn justify_cursor(
    justify: Justify,
    content: Bounds3D,
    horizontal: bool,
    used: f64,
    gap: f64,
    count: usize,
) -> (f64, f64) {
    let available = if horizontal {
        content.width()
    } else {
        content.height()
    };
    let free = (available - used).max(0.0);
    let start = if horizontal {
        content.min.x
    } else {
        content.max.y
    };
    match justify {
        Justify::Start => (start, gap),
        Justify::Center => (
            if horizontal {
                start + free * 0.5
            } else {
                start - free * 0.5
            },
            gap,
        ),
        Justify::End => (
            if horizontal {
                start + free
            } else {
                start - free
            },
            gap,
        ),
        Justify::Between if count > 1 => (start, gap + free / (count - 1) as f64),
        Justify::Around => {
            let extra = free / count.max(1) as f64;
            (
                if horizontal {
                    start + extra * 0.5
                } else {
                    start - extra * 0.5
                },
                gap + extra,
            )
        }
        Justify::Evenly => {
            let extra = free / (count + 1) as f64;
            (
                if horizontal {
                    start + extra
                } else {
                    start - extra
                },
                gap + extra,
            )
        }
        _ => (start, gap),
    }
}

fn aligned_cross(content: Bounds3D, horizontal: bool, size: DVec2, align: Align) -> f64 {
    if horizontal {
        match align {
            Align::Start => content.max.y - size.y * 0.5,
            Align::Center | Align::Stretch => content.center().y,
            Align::End => content.min.y + size.y * 0.5,
        }
    } else {
        match align {
            Align::Start => content.min.x + size.x * 0.5,
            Align::Center | Align::Stretch => content.center().x,
            Align::End => content.max.x - size.x * 0.5,
        }
    }
}

fn grid_tracks(
    node: &LayoutNode,
    rows: &[Track],
    columns: &[Track],
    auto_flow: AutoFlow,
    available: DVec2,
    measurer: &impl IntrinsicMeasure,
    resolved: &mut ResolvedLayout,
) -> Result<(Vec<f64>, Vec<f64>), LayoutError> {
    let mut widths = vec![0.0; columns.len()];
    let mut heights = vec![0.0; rows.len()];
    for (index, track) in columns.iter().map(|track| track.sanitize()).enumerate() {
        if let Track::Fixed(value) = track {
            widths[index] = value;
        }
    }
    for (index, track) in rows.iter().map(|track| track.sanitize()).enumerate() {
        if let Track::Fixed(value) = track {
            heights[index] = value;
        }
    }
    let children: Vec<_> = node
        .children
        .iter()
        .filter(|child| !child.style.absolute)
        .collect();
    let positions = grid_positions(&children, rows.len(), columns.len(), auto_flow)?;
    for (child, (row, column)) in children.iter().zip(positions) {
        let size = measure_node(
            &child.node,
            BoxConstraints {
                min: DVec2::ZERO,
                max: available,
            },
            measurer,
            resolved,
        )?;
        if child.style.column_span.max(1) == 1 && matches!(columns[column], Track::Auto) {
            widths[column] = widths[column].max(size.x);
        }
        if child.style.row_span.max(1) == 1 && matches!(rows[row], Track::Auto) {
            heights[row] = heights[row].max(size.y);
        }
    }
    distribute_fraction_tracks(columns, available.x, node.style.gap.x, &mut widths);
    distribute_fraction_tracks(rows, available.y, node.style.gap.y, &mut heights);
    Ok((widths, heights))
}

fn distribute_fraction_tracks(tracks: &[Track], available: f64, gap: f64, sizes: &mut [f64]) {
    let occupied = sizes.iter().sum::<f64>() + gap * sizes.len().saturating_sub(1) as f64;
    let free = (available - occupied).max(0.0);
    let total: f64 = tracks
        .iter()
        .map(|track| match track.sanitize() {
            Track::Fraction(weight) => weight,
            _ => 0.0,
        })
        .sum();
    if total > 0.0 {
        for (index, track) in tracks.iter().map(|track| track.sanitize()).enumerate() {
            if let Track::Fraction(weight) = track {
                sizes[index] = free * weight / total;
            }
        }
    }
}

fn place_grid(
    node: &LayoutNode,
    content: Bounds3D,
    rows: &[Track],
    columns: &[Track],
    auto_flow: AutoFlow,
    measurer: &impl IntrinsicMeasure,
    resolved: &mut ResolvedLayout,
) -> Result<(), LayoutError> {
    let available = DVec2::new(content.width(), content.height());
    let (widths, heights) = grid_tracks(
        node, rows, columns, auto_flow, available, measurer, resolved,
    )?;
    let children: Vec<_> = node
        .children
        .iter()
        .filter(|child| !child.style.absolute)
        .collect();
    let positions = grid_positions(&children, rows.len(), columns.len(), auto_flow)?;
    for (child, (row, column)) in children.iter().zip(positions) {
        let row_end = row + child.style.row_span.max(1);
        let column_end = column + child.style.column_span.max(1);
        if row_end > rows.len() || column_end > columns.len() {
            return Err(LayoutError::GridOutOfBounds(child.node.id));
        }
        let min_x =
            content.min.x + widths[..column].iter().sum::<f64>() + node.style.gap.x * column as f64;
        let max_x = min_x
            + widths[column..column_end].iter().sum::<f64>()
            + node.style.gap.x * column_end.saturating_sub(column + 1) as f64;
        let max_y =
            content.max.y - heights[..row].iter().sum::<f64>() - node.style.gap.y * row as f64;
        let min_y = max_y
            - heights[row..row_end].iter().sum::<f64>()
            - node.style.gap.y * row_end.saturating_sub(row + 1) as f64;
        let cell = Bounds3D::new_2d(min_x, min_y, max_x, max_y);
        let measured = measure_node(
            &child.node,
            BoxConstraints {
                min: DVec2::ZERO,
                max: DVec2::new(cell.width(), cell.height()),
            },
            measurer,
            resolved,
        )?;
        let align = child.style.align.unwrap_or(node.style.align);
        let size = if align == Align::Stretch {
            DVec2::new(cell.width(), cell.height())
        } else {
            measured.min(DVec2::new(cell.width(), cell.height()))
        };
        let offset = child.style.anchor.to_offset();
        let center = DVec2::new(
            cell.center().x + offset.x * (cell.width() - size.x) * 0.5,
            cell.center().y + offset.y * (cell.height() - size.y) * 0.5,
        ) + child.style.offset.truncate();
        place_node(&child.node, center, size, measurer, resolved)?;
    }
    Ok(())
}

fn grid_positions(
    children: &[&LayoutChild],
    rows: usize,
    columns: usize,
    auto_flow: AutoFlow,
) -> Result<Vec<(usize, usize)>, LayoutError> {
    let mut occupied = vec![false; rows * columns];
    let mut positions = vec![None; children.len()];
    let fits =
        |occupied: &[bool], row: usize, column: usize, row_span: usize, column_span: usize| {
            row + row_span <= rows
                && column + column_span <= columns
                && (row..row + row_span)
                    .all(|r| (column..column + column_span).all(|c| !occupied[r * columns + c]))
        };
    let mark =
        |occupied: &mut [bool], row: usize, column: usize, row_span: usize, column_span: usize| {
            for r in row..row + row_span {
                for c in column..column + column_span {
                    occupied[r * columns + c] = true;
                }
            }
        };

    // Reserve fully explicit items first so auto-placement never steals their
    // cells merely because an automatic item appeared earlier in the list.
    for (index, child) in children.iter().enumerate() {
        let (Some(row), Some(column)) = (child.style.row, child.style.column) else {
            continue;
        };
        let row_span = child.style.row_span.max(1);
        let column_span = child.style.column_span.max(1);
        if row + row_span > rows || column + column_span > columns {
            return Err(LayoutError::GridOutOfBounds(child.node.id));
        }
        if !fits(&occupied, row, column, row_span, column_span) {
            return Err(LayoutError::GridCollision(child.node.id));
        }
        mark(&mut occupied, row, column, row_span, column_span);
        positions[index] = Some((row, column));
    }

    for (index, child) in children.iter().enumerate() {
        if positions[index].is_some() {
            continue;
        }
        let row_span = child.style.row_span.max(1);
        let column_span = child.style.column_span.max(1);
        let mut candidates = Vec::with_capacity(rows * columns);
        match (child.style.row, child.style.column) {
            (Some(row), None) => {
                if row >= rows {
                    return Err(LayoutError::GridOutOfBounds(child.node.id));
                }
                candidates.extend((0..columns).map(|column| (row, column)));
            }
            (None, Some(column)) => {
                if column >= columns {
                    return Err(LayoutError::GridOutOfBounds(child.node.id));
                }
                candidates.extend((0..rows).map(|row| (row, column)));
            }
            (None, None) => match auto_flow {
                AutoFlow::Row => {
                    candidates.extend(
                        (0..rows).flat_map(|row| (0..columns).map(move |column| (row, column))),
                    );
                }
                AutoFlow::Column => {
                    candidates.extend(
                        (0..columns).flat_map(|column| (0..rows).map(move |row| (row, column))),
                    );
                }
            },
            (Some(_), Some(_)) => unreachable!(),
        }
        let Some((row, column)) = candidates
            .into_iter()
            .find(|(row, column)| fits(&occupied, *row, *column, row_span, column_span))
        else {
            return Err(LayoutError::GridNoSpace(child.node.id));
        };
        mark(&mut occupied, row, column, row_span, column_span);
        positions[index] = Some((row, column));
    }
    Ok(positions.into_iter().map(Option::unwrap).collect())
}

fn place_overlay(
    child: &LayoutChild,
    content: Bounds3D,
    measurer: &impl IntrinsicMeasure,
    resolved: &mut ResolvedLayout,
) -> Result<(), LayoutError> {
    let available = DVec2::new(content.width(), content.height());
    let size = measure_node(
        &child.node,
        BoxConstraints {
            min: DVec2::ZERO,
            max: available,
        },
        measurer,
        resolved,
    )?;
    let anchor = child.style.anchor.to_offset();
    let center = DVec2::new(
        content.center().x + anchor.x * (content.width() - size.x) * 0.5,
        content.center().y + anchor.y * (content.height() - size.y) * 0.5,
    ) + child.style.offset.truncate();
    place_node(&child.node, center, size, measurer, resolved)
}

#[derive(Clone, Copy)]
struct SolverBox {
    left: Variable,
    bottom: Variable,
    width: Variable,
    height: Variable,
}

fn apply_relations(
    layout: &mut ResolvedLayout,
    relations: &[LayoutConstraint],
) -> Result<(), LayoutError> {
    if relations.is_empty() {
        return Ok(());
    }
    let mut solver = Solver::new();
    let mut variables = BTreeMap::new();
    for (order, (id, box_)) in layout.boxes.iter().enumerate() {
        let vars = SolverBox {
            left: Variable::new(),
            bottom: Variable::new(),
            width: Variable::new(),
            height: Variable::new(),
        };
        variables.insert(*id, vars);
        let b = box_.bounds;
        let tie = Strength::new((Strength::WEAK.value() - order as f64 * EPSILON).max(EPSILON));
        for constraint in [
            SolverConstraint::new(
                vars.width.into(),
                RelationalOperator::GreaterOrEqual,
                Strength::REQUIRED,
            ),
            SolverConstraint::new(
                vars.height.into(),
                RelationalOperator::GreaterOrEqual,
                Strength::REQUIRED,
            ),
            solver_eq(vars.left.into(), b.min.x, tie),
            solver_eq(vars.bottom.into(), b.min.y, tie),
            solver_eq(vars.width.into(), b.width(), tie),
            solver_eq(vars.height.into(), b.height(), tie),
        ] {
            solver
                .add_constraint(constraint)
                .map_err(|error| LayoutError::Unsatisfiable(error.to_string()))?;
        }
    }
    for (index, relation) in relations.iter().enumerate() {
        let lhs = solver_expression(&relation.lhs, &variables)?;
        let rhs = solver_expression(&relation.rhs, &variables)?;
        let operator = match relation.relation {
            ConstraintRelation::Equal => RelationalOperator::Equal,
            ConstraintRelation::LessOrEqual => RelationalOperator::LessOrEqual,
            ConstraintRelation::GreaterOrEqual => RelationalOperator::GreaterOrEqual,
        };
        solver
            .add_constraint(SolverConstraint::new(
                lhs - rhs,
                operator,
                relation.strength.solver(),
            ))
            .map_err(|error| {
                let nodes = relation
                    .lhs
                    .terms
                    .keys()
                    .chain(relation.rhs.terms.keys())
                    .map(|variable| variable.node.0.to_string())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", ");
                let label = relation
                    .label
                    .as_deref()
                    .map_or_else(|| format!("constraint #{index}"), |label| label.to_string());
                LayoutError::Unsatisfiable(format!(
                    "{label} involving nodes [{nodes}] ({:?}): {error}",
                    relation.relation
                ))
            })?;
    }
    let mut values = BTreeMap::new();
    for (variable, value) in solver.fetch_changes() {
        values.insert(*variable, *value);
    }
    for (id, vars) in variables {
        let left = values.get(&vars.left).copied().unwrap_or(0.0);
        let bottom = values.get(&vars.bottom).copied().unwrap_or(0.0);
        let width = values.get(&vars.width).copied().unwrap_or(0.0).max(0.0);
        let height = values.get(&vars.height).copied().unwrap_or(0.0).max(0.0);
        if let Some(box_) = layout.boxes.get_mut(&id) {
            box_.bounds = Bounds3D::new_2d(left, bottom, left + width, bottom + height);
        }
    }
    for (index, relation) in relations.iter().enumerate() {
        if relation.strength == ConstraintStrength::Required {
            continue;
        }
        let lhs = relation.lhs.value(layout)?;
        let rhs = relation.rhs.value(layout)?;
        let residual = match relation.relation {
            ConstraintRelation::Equal => (lhs - rhs).abs(),
            ConstraintRelation::LessOrEqual => (lhs - rhs).max(0.0),
            ConstraintRelation::GreaterOrEqual => (rhs - lhs).max(0.0),
        };
        if residual > EPSILON {
            layout.diagnostics.push(LayoutDiagnostic {
                constraint: index,
                residual,
                message: relation
                    .label
                    .clone()
                    .unwrap_or_else(|| "soft constraint was relaxed".to_string()),
            });
        }
    }
    Ok(())
}

fn solver_eq(expression: SolverExpression, value: f64, strength: Strength) -> SolverConstraint {
    SolverConstraint::new(expression - value, RelationalOperator::Equal, strength)
}

fn solver_expression(
    expression: &LayoutExpression,
    variables: &BTreeMap<LayoutId, SolverBox>,
) -> Result<SolverExpression, LayoutError> {
    let mut result = SolverExpression::from_constant(expression.constant);
    for (variable, coefficient) in &expression.terms {
        let vars = variables
            .get(&variable.node)
            .ok_or(LayoutError::UnknownNode(variable.node))?;
        result += attribute_expression(*vars, variable.attribute) * *coefficient;
    }
    Ok(result)
}

fn attribute_expression(vars: SolverBox, attribute: LayoutAttribute) -> SolverExpression {
    match attribute {
        LayoutAttribute::Left => vars.left.into(),
        LayoutAttribute::Right => SolverExpression::from(vars.left) + vars.width,
        LayoutAttribute::Top => SolverExpression::from(vars.bottom) + vars.height,
        LayoutAttribute::Bottom => vars.bottom.into(),
        LayoutAttribute::CenterX => {
            SolverExpression::from(vars.left) + SolverExpression::from(vars.width) * 0.5
        }
        LayoutAttribute::CenterY => {
            SolverExpression::from(vars.bottom) + SolverExpression::from(vars.height) * 0.5
        }
        LayoutAttribute::Width => vars.width.into(),
        LayoutAttribute::Height => vars.height.into(),
    }
}

fn attribute_value(box_: ResolvedBox, attribute: LayoutAttribute) -> f64 {
    match attribute {
        LayoutAttribute::Left => box_.bounds.min.x,
        LayoutAttribute::Right => box_.bounds.max.x,
        LayoutAttribute::Top => box_.bounds.max.y,
        LayoutAttribute::Bottom => box_.bounds.min.y,
        LayoutAttribute::CenterX => box_.bounds.center().x,
        LayoutAttribute::CenterY => box_.bounds.center().y,
        LayoutAttribute::Width => box_.bounds.width(),
        LayoutAttribute::Height => box_.bounds.height(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Measure(BTreeMap<LayoutId, DVec2>);
    impl IntrinsicMeasure for Measure {
        fn measure(&self, id: LayoutId, constraints: BoxConstraints) -> Result<DVec2, LayoutError> {
            Ok(constraints.constrain(*self.0.get(&id).unwrap_or(&DVec2::ZERO)))
        }
    }

    fn child(id: u64, size: DVec2) -> (LayoutChild, (LayoutId, DVec2)) {
        let id = LayoutId(id);
        (
            LayoutChild {
                node: Box::new(LayoutNode::leaf(id)),
                style: LayoutItemStyle::default(),
            },
            (id, size),
        )
    }

    #[test]
    fn row_distributes_fill_and_padding_without_coordinates() {
        let (a, ma) = child(1, DVec2::new(100.0, 40.0));
        let (mut b, mb) = child(2, DVec2::new(80.0, 60.0));
        b.style.grow = 1.0;
        let mut root =
            LayoutNode::container(LayoutId(0), LayoutNodeKind::Row { wrap: false }, vec![a, b]);
        root.style.width = SizeRule::Fill(1.0);
        root.style.height = SizeRule::Fill(1.0);
        root.style.padding = Insets::all(10.0);
        root.style.gap = DVec2::splat(20.0);
        root.style.align = Align::Center;
        let measure = Measure(BTreeMap::from([ma, mb]));
        let layout = resolve_layout(
            &root,
            Bounds3D::new_2d(-250.0, -100.0, 250.0, 100.0),
            &measure,
            &[],
        )
        .unwrap();
        assert_eq!(layout.boxes[&LayoutId(0)].size(), DVec2::new(500.0, 200.0));
        assert!((layout.boxes[&LayoutId(2)].bounds.width() - 360.0).abs() < EPSILON);
    }

    #[test]
    fn grid_resolves_auto_fixed_fraction_and_spans() {
        let (mut a, ma) = child(1, DVec2::new(90.0, 30.0));
        a.style.column = Some(0);
        let (mut b, mb) = child(2, DVec2::new(40.0, 40.0));
        b.style.column = Some(1);
        b.style.align = Some(Align::Stretch);
        let mut root = LayoutNode::container(
            LayoutId(0),
            LayoutNodeKind::Grid {
                rows: vec![Track::Auto],
                columns: vec![Track::Auto, Track::Fraction(1.0), Track::Fixed(50.0)],
                auto_flow: AutoFlow::Row,
            },
            vec![a, b],
        );
        root.style.width = SizeRule::Fill(1.0);
        root.style.height = SizeRule::Fill(1.0);
        root.style.gap = DVec2::new(10.0, 0.0);
        let layout = resolve_layout(
            &root,
            Bounds3D::new_2d(0.0, 0.0, 300.0, 100.0),
            &Measure(BTreeMap::from([ma, mb])),
            &[],
        )
        .unwrap();
        assert!((layout.boxes[&LayoutId(1)].bounds.width() - 90.0).abs() < EPSILON);
        assert!((layout.boxes[&LayoutId(2)].bounds.width() - 140.0).abs() < EPSILON);
    }

    #[test]
    fn required_relations_move_boxes_and_soft_conflicts_report() {
        let (a, ma) = child(1, DVec2::new(50.0, 20.0));
        let (b, mb) = child(2, DVec2::new(50.0, 20.0));
        let mut root = LayoutNode::container(LayoutId(0), LayoutNodeKind::Stack, vec![a, b]);
        root.style.width = SizeRule::Fill(1.0);
        root.style.height = SizeRule::Fill(1.0);
        let a_right = LayoutExpression::variable(LayoutId(1), LayoutAttribute::Right);
        let b_left = LayoutExpression::variable(LayoutId(2), LayoutAttribute::Left);
        let relations = [
            LayoutConstraint::equal(b_left.clone(), a_right + 12.0.into()),
            LayoutConstraint::equal(b_left, 999.0.into()).with_strength(ConstraintStrength::Weak),
        ];
        let layout = resolve_layout(
            &root,
            Bounds3D::new_2d(-100.0, -100.0, 100.0, 100.0),
            &Measure(BTreeMap::from([ma, mb])),
            &relations,
        )
        .unwrap();
        assert!(
            (layout.boxes[&LayoutId(2)].bounds.min.x
                - layout.boxes[&LayoutId(1)].bounds.max.x
                - 12.0)
                .abs()
                < EPSILON
        );
        assert_eq!(layout.diagnostics.len(), 1);
    }

    #[test]
    fn conflicting_required_constraints_fail_before_render() {
        let leaf = LayoutNode::leaf(LayoutId(1));
        let left = LayoutExpression::variable(LayoutId(1), LayoutAttribute::Left);
        let error = resolve_layout(
            &leaf,
            Bounds3D::new_2d(0.0, 0.0, 100.0, 100.0),
            &Measure(BTreeMap::from([(LayoutId(1), DVec2::new(10.0, 10.0))])),
            &[
                LayoutConstraint::equal(left.clone(), 10.0.into()),
                LayoutConstraint::equal(left, 20.0.into()),
            ],
        )
        .unwrap_err();
        assert!(matches!(error, LayoutError::Unsatisfiable(_)));
    }

    struct ResponsiveMeasure {
        calls: RefCell<BTreeMap<LayoutId, Vec<f64>>>,
    }

    impl IntrinsicMeasure for ResponsiveMeasure {
        fn measure(&self, id: LayoutId, constraints: BoxConstraints) -> Result<DVec2, LayoutError> {
            let width = constraints.max.x.max(1.0);
            self.calls.borrow_mut().entry(id).or_default().push(width);
            Ok(constraints.constrain(DVec2::new(width, 1000.0 / width)))
        }

        fn is_width_sensitive(&self, _id: LayoutId) -> bool {
            true
        }
    }

    #[test]
    fn weighted_grow_remeasures_wrapping_leaves_at_assigned_width() {
        let (mut left, _) = child(1, DVec2::ZERO);
        left.style.grow = 2.0;
        let (mut right, _) = child(2, DVec2::ZERO);
        right.style.grow = 3.0;
        let mut root = LayoutNode::container(
            LayoutId(0),
            LayoutNodeKind::Row { wrap: false },
            vec![left, right],
        );
        root.style.width = SizeRule::Fill(1.0);
        root.style.height = SizeRule::Fill(1.0);
        root.style.gap = DVec2::splat(20.0);
        let measure = ResponsiveMeasure {
            calls: RefCell::new(BTreeMap::new()),
        };
        let layout = resolve_layout(
            &root,
            Bounds3D::new_2d(0.0, 0.0, 500.0, 200.0),
            &measure,
            &[],
        )
        .unwrap();

        assert!((layout.boxes[&LayoutId(1)].bounds.width() - 192.0).abs() < EPSILON);
        assert!((layout.boxes[&LayoutId(2)].bounds.width() - 288.0).abs() < EPSILON);
        assert!(
            measure.calls.borrow()[&LayoutId(1)]
                .iter()
                .any(|width| (*width - 192.0).abs() < EPSILON)
        );
        assert!(layout.iterations >= 2);
    }

    #[test]
    fn shrink_prevents_non_wrapping_rows_from_overflowing() {
        let (left, ml) = child(1, DVec2::new(80.0, 20.0));
        let (right, mr) = child(2, DVec2::new(80.0, 20.0));
        let mut root = LayoutNode::container(
            LayoutId(0),
            LayoutNodeKind::Row { wrap: false },
            vec![left, right],
        );
        root.style.width = SizeRule::Fill(1.0);
        let layout = resolve_layout(
            &root,
            Bounds3D::new_2d(0.0, 0.0, 100.0, 40.0),
            &Measure(BTreeMap::from([ml, mr])),
            &[],
        )
        .unwrap();

        assert!((layout.boxes[&LayoutId(1)].bounds.width() - 50.0).abs() < EPSILON);
        assert!((layout.boxes[&LayoutId(2)].bounds.width() - 50.0).abs() < EPSILON);
        assert!(layout.boxes[&LayoutId(2)].bounds.max.x <= 100.0 + EPSILON);
    }

    #[test]
    fn required_size_limits_win_over_aspect_preference() {
        let mut leaf = LayoutNode::leaf(LayoutId(1));
        leaf.style.width = SizeRule::Fixed(120.0);
        leaf.style.aspect_ratio = Some(2.0);
        leaf.style.max_height = Some(40.0);
        let layout = resolve_layout(
            &leaf,
            Bounds3D::new_2d(0.0, 0.0, 200.0, 200.0),
            &Measure(BTreeMap::from([(LayoutId(1), DVec2::splat(10.0))])),
            &[],
        )
        .unwrap();
        assert_eq!(layout.boxes[&LayoutId(1)].size(), DVec2::new(120.0, 40.0));
    }

    #[test]
    fn grid_column_flow_uses_column_major_auto_placement() {
        let (a, ma) = child(1, DVec2::splat(10.0));
        let (b, mb) = child(2, DVec2::splat(10.0));
        let mut root = LayoutNode::container(
            LayoutId(0),
            LayoutNodeKind::Grid {
                rows: vec![Track::Fraction(1.0), Track::Fraction(1.0)],
                columns: vec![Track::Fraction(1.0), Track::Fraction(1.0)],
                auto_flow: AutoFlow::Column,
            },
            vec![a, b],
        );
        root.style.width = SizeRule::Fill(1.0);
        root.style.height = SizeRule::Fill(1.0);
        root.style.align = Align::Stretch;
        let layout = resolve_layout(
            &root,
            Bounds3D::new_2d(0.0, 0.0, 200.0, 200.0),
            &Measure(BTreeMap::from([ma, mb])),
            &[],
        )
        .unwrap();
        assert_eq!(layout.boxes[&LayoutId(1)].bounds.center().x, 50.0);
        assert_eq!(layout.boxes[&LayoutId(2)].bounds.center().x, 50.0);
        assert!(layout.boxes[&LayoutId(1)].bounds.center().y > 100.0);
        assert!(layout.boxes[&LayoutId(2)].bounds.center().y < 100.0);
    }

    #[test]
    fn grid_reserves_explicit_spans_before_auto_placement() {
        let (automatic, ma) = child(1, DVec2::splat(10.0));
        let (mut explicit, me) = child(2, DVec2::splat(10.0));
        explicit.style.row = Some(0);
        explicit.style.column = Some(0);
        explicit.style.row_span = 2;
        let mut root = LayoutNode::container(
            LayoutId(0),
            LayoutNodeKind::Grid {
                rows: vec![Track::Fraction(1.0), Track::Fraction(1.0)],
                columns: vec![Track::Fraction(1.0), Track::Fraction(1.0)],
                auto_flow: AutoFlow::Row,
            },
            vec![automatic, explicit],
        );
        root.style.width = SizeRule::Fill(1.0);
        root.style.height = SizeRule::Fill(1.0);
        root.style.align = Align::Stretch;
        let layout = resolve_layout(
            &root,
            Bounds3D::new_2d(0.0, 0.0, 200.0, 200.0),
            &Measure(BTreeMap::from([ma, me])),
            &[],
        )
        .unwrap();

        assert_eq!(layout.boxes[&LayoutId(1)].bounds.center().x, 150.0);
        assert_eq!(layout.boxes[&LayoutId(2)].bounds.center().x, 50.0);
        assert_eq!(layout.boxes[&LayoutId(2)].bounds.height(), 200.0);
    }

    #[test]
    fn stronger_relations_win_and_repeated_solves_are_identical() {
        let leaf = LayoutNode::leaf(LayoutId(1));
        let left = LayoutExpression::variable(LayoutId(1), LayoutAttribute::Left);
        let relations = [
            LayoutConstraint::equal(left.clone(), 25.0.into())
                .with_strength(ConstraintStrength::Strong),
            LayoutConstraint::equal(left, 80.0.into()).with_strength(ConstraintStrength::Medium),
        ];
        let measure = Measure(BTreeMap::from([(LayoutId(1), DVec2::new(20.0, 10.0))]));
        let first = resolve_layout(
            &leaf,
            Bounds3D::new_2d(0.0, 0.0, 100.0, 100.0),
            &measure,
            &relations,
        )
        .unwrap();
        assert!((first.boxes[&LayoutId(1)].bounds.min.x - 25.0).abs() < EPSILON);
        assert_eq!(first.diagnostics.len(), 1);

        for _ in 0..8 {
            let repeated = resolve_layout(
                &leaf,
                Bounds3D::new_2d(0.0, 0.0, 100.0, 100.0),
                &measure,
                &relations,
            )
            .unwrap();
            assert_eq!(repeated.boxes, first.boxes);
            assert_eq!(repeated.diagnostics, first.diagnostics);
        }
    }
}
