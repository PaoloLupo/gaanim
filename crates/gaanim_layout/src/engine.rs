//! Deterministic, renderer-independent layout tree and relational solver.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Add, Div, Mul, Neg, Sub};

use gaanim_core::glam::{DVec2, DVec3};
use gaanim_math::Bounds3D;
use kasuari::{Constraint as SolverConstraint, Expression as SolverExpression, RelationalOperator};
use kasuari::{Solver, Strength, Variable};
use taffy::{
    AlignContent, AlignItems, AlignSelf, AvailableSpace, Dimension, Display, FlexDirection,
    FlexWrap, GridAutoFlow, GridPlacement, JustifyContent, LengthPercentage, LengthPercentageAuto,
    NodeId, Position, Style, TaffyTree,
};

use crate::Anchor;

const EPSILON: f64 = 1.0e-6;

/// Stable identity for one node in a layout tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutId(pub u64);

/// How a box chooses its size on one axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeRule {
    /// Fit the content.
    Hug,
    /// Share the parent's free space by weight (or stretch across it).
    Fill(f64),
    /// A fixed length in scene units.
    Fixed(f64),
    /// A percentage (0–100) of the parent's content box.
    Percent(f64),
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
            Self::Percent(value) => Self::Percent(value.max(0.0)),
        }
    }
}

/// Grid track sizing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Track {
    Fixed(f64),
    Auto,
    Fraction(f64),
    /// A percentage (0–100) of the grid's content box.
    Percent(f64),
}

impl Track {
    fn sanitize(self) -> Self {
        match self {
            Self::Fixed(value) => Self::Fixed(value.max(0.0)),
            Self::Auto => Self::Auto,
            Self::Fraction(weight) => Self::Fraction(weight.max(EPSILON)),
            Self::Percent(value) => Self::Percent(value.max(0.0)),
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
    Stretch,
    Baseline,
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
    /// Space around the box inside its parent; may be negative.
    pub margin: Insets,
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
            margin: Insets::default(),
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
            margin: self.margin,
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
    /// Main-axis size before growing or shrinking; `None` uses the content
    /// (or zero for a growing item).
    pub basis: Option<f64>,
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
            basis: None,
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
    #[error("required layout constraints are incompatible: {0}")]
    Unsatisfiable(String),
    #[error("layout engine failed: {0}")]
    Engine(String),
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

/// Resolve a tree with Taffy (CSS flexbox and grid), then apply relational
/// constraints. Leaves are measured through [`IntrinsicMeasure`] at the size
/// their parent offers, so wrapping text breaks at its assigned width.
pub fn resolve_layout(
    root: &LayoutNode,
    viewport: Bounds3D,
    measurer: &impl IntrinsicMeasure,
    relations: &[LayoutConstraint],
) -> Result<ResolvedLayout, LayoutError> {
    validate_tree(root)?;
    let mut tree: TaffyTree<LayoutId> = TaffyTree::new();
    tree.disable_rounding();
    let available = DVec2::new(viewport.width(), viewport.height()).max(DVec2::ZERO);
    let root_id = build_node(&mut tree, root, None, None, None, available)?;

    let mut failure = None;
    tree.compute_layout_with_measure(
        root_id,
        taffy::Size {
            width: AvailableSpace::Definite(available.x as f32),
            height: AvailableSpace::Definite(available.y as f32),
        },
        |known, space, _, context, _| {
            let Some(id) = context.copied() else {
                return taffy::Size::ZERO;
            };
            let width_sensitive = measurer.is_width_sensitive(id);
            let limit = |known: Option<f32>, space: AvailableSpace| match (known, space) {
                (Some(value), _) => f64::from(value),
                (None, AvailableSpace::Definite(value)) => f64::from(value),
                (None, AvailableSpace::MinContent) if width_sensitive => 0.0,
                (None, _) => f64::INFINITY,
            };
            let max = DVec2::new(
                limit(known.width, space.width),
                limit(known.height, space.height),
            );
            let constraints = BoxConstraints {
                min: DVec2::ZERO,
                max,
            };
            match measurer.measure(id, constraints) {
                Ok(size) => taffy::Size {
                    width: known.width.unwrap_or(size.x as f32),
                    height: known.height.unwrap_or(size.y as f32),
                },
                Err(error) => {
                    failure.get_or_insert(error);
                    taffy::Size::ZERO
                }
            }
        },
    )
    .map_err(|error| LayoutError::Engine(error.to_string()))?;
    if let Some(error) = failure {
        return Err(error);
    }

    let mut resolved = ResolvedLayout {
        iterations: 1,
        ..ResolvedLayout::default()
    };
    let size = tree
        .layout(root_id)
        .map_err(|error| LayoutError::Engine(error.to_string()))?
        .size;
    let size = DVec2::new(f64::from(size.width), f64::from(size.height));
    let center = DVec2::new(viewport.center().x, viewport.center().y);
    let top_left = DVec2::new(center.x - size.x * 0.5, center.y + size.y * 0.5);
    place_node(&tree, root, root_id, top_left, &mut resolved)?;
    apply_relations(&mut resolved, relations)?;
    Ok(resolved)
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

fn length(value: f64) -> LengthPercentage {
    LengthPercentage::length(value.max(0.0) as f32)
}

fn dimension(rule: SizeRule) -> Dimension {
    match rule.sanitize() {
        SizeRule::Hug | SizeRule::Fill(_) => Dimension::auto(),
        SizeRule::Fixed(value) => Dimension::length(value as f32),
        SizeRule::Percent(value) => Dimension::percent((value / 100.0) as f32),
    }
}

fn limit(value: Option<f64>) -> Dimension {
    value
        .filter(|value| value.is_finite())
        .map_or(Dimension::auto(), |value| {
            Dimension::length(value.max(0.0) as f32)
        })
}

fn insets(value: Insets) -> taffy::Rect<LengthPercentage> {
    let value = value.sanitized();
    taffy::Rect {
        left: length(value.left),
        right: length(value.right),
        top: length(value.top),
        bottom: length(value.bottom),
    }
}

fn margins(value: Insets) -> taffy::Rect<LengthPercentageAuto> {
    let side = |value: f64| LengthPercentageAuto::length(value as f32);
    taffy::Rect {
        left: side(value.left),
        right: side(value.right),
        top: side(value.top),
        bottom: side(value.bottom),
    }
}

fn align_items(align: Align) -> AlignItems {
    match align {
        Align::Start => AlignItems::Start,
        Align::Center => AlignItems::Center,
        Align::End => AlignItems::End,
        Align::Stretch => AlignItems::Stretch,
        Align::Baseline => AlignItems::Baseline,
    }
}

fn justify_content(justify: Justify) -> JustifyContent {
    match justify {
        Justify::Start => JustifyContent::Start,
        Justify::Center => JustifyContent::Center,
        Justify::End => JustifyContent::End,
        Justify::Between => JustifyContent::SpaceBetween,
        Justify::Around => JustifyContent::SpaceAround,
        Justify::Evenly => JustifyContent::SpaceEvenly,
    }
}

fn track(track: Track) -> taffy::TrackSizingFunction {
    match track.sanitize() {
        Track::Fixed(value) => taffy::style_helpers::length(value as f32),
        Track::Auto => taffy::style_helpers::auto(),
        Track::Fraction(weight) => taffy::style_helpers::fr(weight as f32),
        Track::Percent(value) => taffy::style_helpers::percent((value / 100.0) as f32),
    }
}

/// Self-alignment in a cell or overlay from the child's anchor: the left,
/// center or right third horizontally, and the same vertically.
fn anchor_alignment(anchor: Anchor) -> (AlignSelf, AlignSelf) {
    let offset = anchor.to_offset();
    let axis = |value: f64, low: AlignSelf, high: AlignSelf| {
        if value < -0.5 {
            low
        } else if value > 0.5 {
            high
        } else {
            AlignSelf::Center
        }
    };
    (
        axis(offset.x, AlignSelf::Start, AlignSelf::End),
        // Taffy's block axis grows downward: "start" is the top.
        axis(offset.y, AlignSelf::End, AlignSelf::Start),
    )
}

/// Build one Taffy node. `parent` is the parent's kind, which decides how
/// `Fill` and the child's placement style translate; `None` is the root,
/// which fills the viewport on a `Fill` axis.
fn build_node(
    tree: &mut TaffyTree<LayoutId>,
    node: &LayoutNode,
    parent: Option<&LayoutNodeKind>,
    parent_style: Option<&LayoutStyle>,
    item: Option<&LayoutItemStyle>,
    viewport: DVec2,
) -> Result<NodeId, LayoutError> {
    let style = node.style.sanitized();
    let mut taffy_style = Style {
        size: taffy::Size {
            width: dimension(style.width),
            height: dimension(style.height),
        },
        min_size: taffy::Size {
            width: limit(style.min_width.or(Some(0.0))),
            height: limit(style.min_height.or(Some(0.0))),
        },
        max_size: taffy::Size {
            width: limit(style.max_width),
            height: limit(style.max_height),
        },
        aspect_ratio: style.aspect_ratio.map(|ratio| ratio as f32),
        padding: insets(style.padding),
        margin: margins(style.margin),
        gap: taffy::Size {
            width: length(style.gap.x),
            height: length(style.gap.y),
        },
        ..Style::default()
    };

    match &node.kind {
        LayoutNodeKind::Leaf => {}
        LayoutNodeKind::Row { wrap } | LayoutNodeKind::Column { wrap } => {
            taffy_style.display = Display::Flex;
            taffy_style.flex_direction = if matches!(node.kind, LayoutNodeKind::Row { .. }) {
                FlexDirection::Row
            } else {
                // Taffy's column runs downward, as the scene reads top to bottom.
                FlexDirection::Column
            };
            taffy_style.flex_wrap = if *wrap {
                FlexWrap::Wrap
            } else {
                FlexWrap::NoWrap
            };
            taffy_style.align_items = Some(align_items(style.align));
            // Wrapped lines sit where single-line content would.
            taffy_style.align_content = Some(match style.align {
                Align::Start | Align::Baseline => AlignContent::Start,
                Align::Center => AlignContent::Center,
                Align::End => AlignContent::End,
                Align::Stretch => AlignContent::Stretch,
            });
            taffy_style.justify_content = Some(justify_content(style.justify));
        }
        LayoutNodeKind::Grid {
            rows,
            columns,
            auto_flow,
        } => {
            taffy_style.display = Display::Grid;
            taffy_style.grid_template_rows =
                rows.iter().map(|value| track(*value).into()).collect();
            taffy_style.grid_template_columns =
                columns.iter().map(|value| track(*value).into()).collect();
            taffy_style.grid_auto_flow = match auto_flow {
                AutoFlow::Row => GridAutoFlow::Row,
                AutoFlow::Column => GridAutoFlow::Column,
            };
            taffy_style.align_items = Some(align_items(style.align));
            taffy_style.justify_items = Some(align_items(style.align));
            taffy_style.justify_content = Some(justify_content(style.justify));
        }
        LayoutNodeKind::Stack => {
            taffy_style.display = Display::Grid;
            taffy_style.grid_template_rows =
                vec![taffy::style_helpers::auto::<taffy::TrackSizingFunction>().into()];
            taffy_style.grid_template_columns =
                vec![taffy::style_helpers::auto::<taffy::TrackSizingFunction>().into()];
            taffy_style.align_items = Some(align_items(style.align));
            taffy_style.justify_items = Some(align_items(style.align));
        }
    }

    // How this node sits in its parent.
    match (parent, item) {
        (None, _) => {
            if matches!(style.width, SizeRule::Fill(_)) {
                taffy_style.size.width = Dimension::length(viewport.x as f32);
            }
            if matches!(style.height, SizeRule::Fill(_)) {
                taffy_style.size.height = Dimension::length(viewport.y as f32);
            }
        }
        (Some(_), Some(item)) if item.absolute => {
            taffy_style.position = Position::Absolute;
        }
        (Some(LayoutNodeKind::Row { .. } | LayoutNodeKind::Column { .. }), Some(item)) => {
            let horizontal = matches!(parent, Some(LayoutNodeKind::Row { .. }));
            let (main, cross) = if horizontal {
                (style.width, style.height)
            } else {
                (style.height, style.width)
            };
            let fill = match main {
                SizeRule::Fill(weight) => weight,
                _ => 0.0,
            };
            let grow = if item.grow > 0.0 { item.grow } else { fill };
            taffy_style.flex_grow = grow.max(0.0) as f32;
            taffy_style.flex_shrink = item.shrink.max(0.0) as f32;
            taffy_style.flex_basis = match item.basis {
                Some(basis) => Dimension::length(basis.max(0.0) as f32),
                // A growing item shares the free space by weight, ignoring
                // its own content size.
                None if grow > 0.0 => Dimension::length(0.0),
                None => Dimension::auto(),
            };
            if let Some(align) = item.align {
                taffy_style.align_self = Some(align_items(align));
            } else if matches!(cross, SizeRule::Fill(_)) {
                taffy_style.align_self = Some(AlignSelf::Stretch);
            }
        }
        (Some(LayoutNodeKind::Grid { .. } | LayoutNodeKind::Stack), Some(item)) => {
            if matches!(parent, Some(LayoutNodeKind::Grid { .. })) {
                let placement = |start: Option<usize>, span: usize| taffy::Line {
                    start: match start {
                        Some(index) => {
                            taffy::style_helpers::line::<GridPlacement>(index as i16 + 1)
                        }
                        None => GridPlacement::Auto,
                    },
                    end: taffy::style_helpers::span::<GridPlacement>(span.max(1) as u16),
                };
                taffy_style.grid_row = placement(item.row, item.row_span);
                taffy_style.grid_column = placement(item.column, item.column_span);
            } else {
                taffy_style.grid_row = taffy::Line {
                    start: taffy::style_helpers::line::<GridPlacement>(1),
                    end: GridPlacement::Auto,
                };
                taffy_style.grid_column = taffy_style.grid_row.clone();
            }
            // A stretching container fills the cell; otherwise the child's
            // anchor places it inside.
            let parent_align = parent_style.map_or(Align::Start, |style| style.align);
            let (justify, align) = match item.align.unwrap_or(parent_align) {
                Align::Stretch => (AlignSelf::Stretch, AlignSelf::Stretch),
                _ => anchor_alignment(item.anchor),
            };
            taffy_style.justify_self = Some(if matches!(style.width, SizeRule::Fill(_)) {
                AlignSelf::Stretch
            } else {
                justify
            });
            taffy_style.align_self = Some(if matches!(style.height, SizeRule::Fill(_)) {
                AlignSelf::Stretch
            } else {
                align
            });
        }
        (Some(LayoutNodeKind::Leaf), _) | (Some(_), None) => {}
    }

    let result = if matches!(node.kind, LayoutNodeKind::Leaf) {
        tree.new_leaf_with_context(taffy_style, node.id)
    } else {
        let children = node
            .children
            .iter()
            .map(|child| {
                build_node(
                    tree,
                    &child.node,
                    Some(&node.kind),
                    Some(&node.style),
                    Some(&child.style),
                    viewport,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        tree.new_with_children(taffy_style, &children)
    };
    result.map_err(|error| LayoutError::Engine(error.to_string()))
}

/// Record the scene bounds of `node` and its subtree. `top_left` is the
/// node's top-left corner in scene coordinates (y up).
fn place_node(
    tree: &TaffyTree<LayoutId>,
    node: &LayoutNode,
    id: NodeId,
    top_left: DVec2,
    resolved: &mut ResolvedLayout,
) -> Result<(), LayoutError> {
    let engine = |error: taffy::TaffyError| LayoutError::Engine(error.to_string());
    let layout = tree.layout(id).map_err(engine)?;
    let size = DVec2::new(f64::from(layout.size.width), f64::from(layout.size.height));
    let bounds = Bounds3D::new_2d(
        top_left.x,
        top_left.y - size.y,
        top_left.x + size.x,
        top_left.y,
    );
    resolved.boxes.insert(
        node.id,
        ResolvedBox {
            bounds,
            clip: None,
            scale: DVec3::ONE,
        },
    );
    let padding = node.style.sanitized().padding;
    let content = Bounds3D::new_2d(
        bounds.min.x + padding.left,
        bounds.min.y + padding.bottom,
        bounds.max.x - padding.right,
        bounds.max.y - padding.top,
    );
    let children = tree.children(id).map_err(engine)?;
    for (child, child_id) in node.children.iter().zip(children) {
        let child_layout = tree.layout(child_id).map_err(engine)?;
        let child_size = DVec2::new(
            f64::from(child_layout.size.width),
            f64::from(child_layout.size.height),
        );
        let mut child_top_left = if child.style.absolute {
            // Anchored inside the parent's content box.
            let anchor = child.style.anchor.to_offset();
            let center = DVec2::new(
                content.center().x + anchor.x * (content.width() - child_size.x) * 0.5,
                content.center().y + anchor.y * (content.height() - child_size.y) * 0.5,
            );
            DVec2::new(center.x - child_size.x * 0.5, center.y + child_size.y * 0.5)
        } else {
            DVec2::new(
                top_left.x + f64::from(child_layout.location.x),
                top_left.y - f64::from(child_layout.location.y),
            )
        };
        child_top_left += child.style.offset.truncate();
        place_node(tree, &child.node, child_id, child_top_left, resolved)?;
    }
    Ok(())
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
    fn aspect_ratio_derives_the_free_axis_from_a_fixed_one() {
        let mut leaf = LayoutNode::leaf(LayoutId(1));
        leaf.style.width = SizeRule::Fixed(120.0);
        leaf.style.aspect_ratio = Some(3.0);
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
