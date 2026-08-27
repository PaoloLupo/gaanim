use crate::anim::{AnimationBuilder, AnimationType, TextSelectionEffect, ValueTrackerRef};
use bevy::prelude::{
    BuildChildrenTransformExt, Commands, Entity, GlobalTransform, Transform, Visibility,
};
use codex::{Def as CodexDef, ModifierSet};
use gaanim_core::ObjectId;
use gaanim_core::glam::DVec3;
use gaanim_core::kurbo::{self, Shape};
use gaanim_core::peniko::{Brush, Color};
use gaanim_layout::{Anchor, Direction};
use gaanim_math::matching::{MatchItem, MatchingConfig, MatchingMode, lcs_match};
use gaanim_math::{Bounds3D, EasingCurve, GlobalSpatialTransform, RateFunc, SpatialTransform};
use gaanim_objects::prelude::MobjectBundle;
use gaanim_scene::{
    FillBrush, GroupMarker, LineListData, LocalBounds, Mesh3DMarker, MobjectId, ObjectTag, Opacity,
    StrokeBrush, TriangleMeshData, Visible, WorldBounds,
};
use gaanim_text::font::FontRegistry;
use gaanim_text::shaper::{HierarchyChild, compile_text_to_hierarchy};
use gaanim_text::typst_compiler::compile_typst_to_hierarchy;
use gaanim_timeline::{
    clip::{AnimationSpec, ClipPayload, GltfAnimationSpec, PropertyLensSpec, SceneId, TrackId},
    scene::SceneMember,
    timeline::Timeline,
    transition::TransitionType,
};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::OnceLock;
use typst_syntax::ast::{MathFieldAccess, MathIdent, MathPrimes, MathShorthand};
use typst_syntax::{SyntaxNode, parse_math};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrawMode {
    Grow,
    BorderThenFill,
}

#[derive(Clone, Debug)]
struct DrawSchedule {
    mode: DrawMode,
    reversed: bool,
    staggered: bool,
    lag_ratio: f64,
    stroke_width: Option<f64>,
    auto_stroke_width: f64,
    fill_rate_func: gaanim_math::RateFunc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EquationTransitionMode {
    Replace,
    Copy,
}

#[derive(Clone, Debug, Default)]
struct EquationTransitionPlan {
    pairs: Vec<(ObjectId, ObjectId)>,
    leaving: Vec<ObjectId>,
    entering: Vec<ObjectId>,
}

/// Resolve a Typst math symbol access through the same Codex table used by
/// Typst itself. Unknown identifiers are intentionally left to the caller.
fn codex_math_symbol(access: &str) -> Option<&'static str> {
    let (name, modifiers) = access
        .split_once('.')
        .map_or((access, ""), |(name, modifiers)| (name, modifiers));
    let binding = codex::SYM.get(name)?;
    let CodexDef::Symbol(symbol) = binding.def else {
        return None;
    };
    symbol
        .get(ModifierSet::from_raw_dotted(modifiers))
        .map(|(value, _)| value)
}

fn codex_math_primes(count: usize) -> Option<String> {
    let access = match count {
        1 => "prime",
        2 => "prime.double",
        3 => "prime.triple",
        4 => "prime.quad",
        _ => {
            let prime = codex_math_symbol("prime")?;
            return Some(prime.repeat(count));
        }
    };
    codex_math_symbol(access).map(str::to_owned)
}

/// Convert authored Typst math into the characters its parser resolves before
/// layout. This covers named Codex symbols, official math shorthands, and the
/// dedicated prime syntax while preserving unknown identifiers verbatim.
fn typst_math_selection_source(source: &str) -> String {
    fn append_node(node: &SyntaxNode, output: &mut String) {
        if node.cast::<MathFieldAccess>().is_some() {
            let source = node.full_text();
            output.push_str(codex_math_symbol(source.as_str()).unwrap_or(source.as_str()));
            return;
        }
        if let Some(identifier) = node.cast::<MathIdent>() {
            let source = identifier.as_str();
            output.push_str(codex_math_symbol(source).unwrap_or(source));
            return;
        }
        if let Some(shorthand) = node.cast::<MathShorthand>() {
            output.push(shorthand.get());
            return;
        }
        if let Some(primes) = node.cast::<MathPrimes>() {
            if let Some(value) = codex_math_primes(primes.count()) {
                output.push_str(&value);
            } else {
                output.push_str(node.leaf_text());
            }
            return;
        }

        if node.children().next().is_none() {
            output.push_str(node.leaf_text());
        } else {
            for child in node.children() {
                append_node(child, output);
            }
        }
    }

    let root = parse_math(source);
    let mut output = String::with_capacity(source.len());
    append_node(&root, &mut output);
    output
}

/// Invert Codex's mathematical styling table so authored and rendered text
/// share one search domain without maintaining a parallel Unicode table.
fn standard_math_char(c: char) -> char {
    static REVERSE_STYLES: OnceLock<HashMap<char, char>> = OnceLock::new();
    let reverse = REVERSE_STYLES.get_or_init(|| {
        use codex::styling::MathStyle;

        const STYLES: &[MathStyle] = &[
            MathStyle::Bold,
            MathStyle::Italic,
            MathStyle::BoldItalic,
            MathStyle::Script,
            MathStyle::BoldScript,
            MathStyle::Fraktur,
            MathStyle::BoldFraktur,
            MathStyle::SansSerif,
            MathStyle::SansSerifBold,
            MathStyle::SansSerifItalic,
            MathStyle::SansSerifBoldItalic,
            MathStyle::Monospace,
            MathStyle::Isolated,
            MathStyle::Initial,
            MathStyle::Tailed,
            MathStyle::Stretched,
            MathStyle::Looped,
            MathStyle::DoubleStruck,
            MathStyle::DoubleStruckItalic,
            MathStyle::Chancery,
            MathStyle::BoldChancery,
            MathStyle::Roundhand,
            MathStyle::BoldRoundhand,
            MathStyle::Hebrew,
        ];

        let mut base_characters = ('0'..='9')
            .chain('A'..='Z')
            .chain('a'..='z')
            .collect::<BTreeSet<_>>();
        for (_, binding) in codex::SYM.iter() {
            let CodexDef::Symbol(symbol) = binding.def else {
                continue;
            };
            for (_, value, _) in symbol.variants() {
                base_characters.extend(value.chars());
            }
        }

        let mut reverse = HashMap::new();
        for base in base_characters {
            for &style in STYLES {
                let styled = codex::styling::to_style(base, style).collect::<Vec<_>>();
                if let [styled] = styled.as_slice()
                    && *styled != base
                {
                    reverse.entry(*styled).or_insert(base);
                }
            }
        }
        reverse
    });
    reverse.get(&c).copied().unwrap_or(c)
}

fn normalize_text_selection(source: &str) -> String {
    let mut normalized = String::new();
    for raw in source.chars() {
        if raw.is_whitespace() || raw == '^' || raw == '_' {
            continue;
        }
        normalized.extend(standard_math_char(raw).to_lowercase());
    }
    normalized
}

fn normalized_match_spans(
    normalized_text: &str,
    index_mapping: &[usize],
    normalized_query: &str,
) -> Vec<Vec<usize>> {
    let mut matches = Vec::new();
    if normalized_query.is_empty() {
        return matches;
    }

    let mut search_from = 0;
    while search_from < normalized_text.len() {
        let Some(relative_start) = normalized_text[search_from..].find(normalized_query) else {
            break;
        };
        let start = search_from + relative_start;
        let end = start + normalized_query.len();
        let mut span_indices = Vec::new();
        for byte_index in start..end {
            if let Some(&span_index) = index_mapping.get(byte_index)
                && !span_indices.contains(&span_index)
            {
                span_indices.push(span_index);
            }
        }
        matches.push(span_indices);
        search_from = end;
    }
    matches
}

fn adaptive_lag_ratio(item_count: usize) -> f64 {
    (4.0 / item_count.max(1) as f64).min(0.2)
}

/// Extracts a representative `Color` from a `peniko::Brush` for use as a
/// stroke color in the `Write` animation's auto-stroke fallback.
///
/// - `Brush::Solid(c)` → `Some(c)`
/// - `Brush::Gradient(g)` → first color stop, if any
/// - `Brush::Image(_)` → `None` (no meaningful single color)
fn extract_brush_color(brush: &Brush) -> Option<Color> {
    use gaanim_core::peniko::color::Srgb;
    match brush {
        Brush::Solid(c) => Some(*c),
        Brush::Gradient(g) => g.stops.first().map(|s| s.color.to_alpha_color::<Srgb>()),
        _ => None,
    }
}

/// Tracks the active hot state of an Mobject during scene construction.
/// This enables subsequent layouts and animations to automatically calculate
/// their offsets and "from" properties without manual user input.
#[derive(Debug, Clone)]
pub struct MobjectState {
    pub path: std::sync::Arc<gaanim_core::kurbo::BezPath>,
    pub bounds: Bounds3D,
    pub transform: SpatialTransform,
    pub opacity: f32,
    pub fill: Option<Brush>,
    pub stroke: StrokeBrush,
    pub entity: Entity,
    pub child_spans: Vec<HierarchyChild>,
    pub children: Vec<ObjectId>,
    pub parent: Option<ObjectId>,
    pub exclude_from_parent_draw: bool,
}

fn compose_child_paths(children: &[HierarchyChild]) -> std::sync::Arc<gaanim_core::kurbo::BezPath> {
    let mut merged = gaanim_core::kurbo::BezPath::new();
    for child in children {
        let mut path = (*child.path).clone();
        path.apply_affine(child.transform.to_affine_2d());
        merged.extend(path);
    }
    std::sync::Arc::new(merged)
}

/// A `Vec`-backed map from `ObjectId` to `MobjectState`.
///
/// IDs tend to be allocated sequentially by `SceneBuilder::next_id()`, but
/// the parent state is often inserted *after* its children (higher indices),
/// so gaps are handled gracefully with `Option`.
#[derive(Debug, Clone)]
pub struct MobjectStateMap {
    v: Vec<Option<MobjectState>>,
}

impl Default for MobjectStateMap {
    fn default() -> Self {
        Self::new()
    }
}

impl MobjectStateMap {
    pub fn new() -> Self {
        Self { v: Vec::new() }
    }

    pub fn insert(&mut self, id: ObjectId, state: MobjectState) {
        let idx = id.index() as usize;
        if idx >= self.v.len() {
            self.v.resize_with(idx + 1, || None);
        }
        self.v[idx] = Some(state);
    }

    pub fn get(&self, id: ObjectId) -> Option<&MobjectState> {
        self.v.get(id.index() as usize).and_then(|v| v.as_ref())
    }

    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut MobjectState> {
        self.v.get_mut(id.index() as usize).and_then(|v| v.as_mut())
    }

    pub fn contains_key(&self, id: ObjectId) -> bool {
        self.v
            .get(id.index() as usize)
            .and_then(|v| v.as_ref())
            .is_some()
    }

    pub fn remove(&mut self, id: ObjectId) {
        let idx = id.index() as usize;
        if idx < self.v.len() {
            self.v[idx] = None;
        }
    }
}

/// A lightweight reference handle to a spawned Mobject in the Scene.
#[derive(Clone, Copy, Debug)]
pub struct MobjectRef {
    pub id: ObjectId,
}

/// A selection of multiple child Mobjects (usually characters or shapes in a text/equation)
/// that can be styled or animated as a single coordinated group.
pub struct MobjectSelection<'a, 'w, 's, 'b> {
    pub builder: &'a mut SceneBuilder<'w, 's, 'b>,
    pub parent_id: ObjectId,
    pub child_ids: Vec<ObjectId>,
}

impl<'a, 'w, 's, 'b> MobjectSelection<'a, 'w, 's, 'b> {
    /// Instantly colors the fill of all selected symbols.
    ///
    /// If the entity already carries a stroke brush (for example the
    /// auto-stroke synthesized by a `Write` animation), the stroke is
    /// retinted to match the new fill so the progressive outline stays
    /// color-coordinated with the selection. The accompanying
    /// `PathCompletion` global reset in `play_write_internal` is what
    /// prevents the outline from being visible at frame 0, so updating
    /// the stroke here does not reintroduce that regression.
    pub fn set_fill(&mut self, color: Color) -> &mut Self {
        for child_id in &self.child_ids {
            if let Some(state) = self.builder.states.get_mut(*child_id) {
                state.fill = Some(Brush::Solid(color));
                self.builder
                    .commands
                    .entity(state.entity)
                    .insert(FillBrush(Some(Brush::Solid(color))));
                if state.stroke.brush.is_some() {
                    let width = state.stroke.style.width;
                    let new_stroke = StrokeBrush::new(color, width);
                    state.stroke = new_stroke.clone();
                    self.builder
                        .commands
                        .entity(state.entity)
                        .insert(new_stroke);
                }
            }
        }
        self
    }

    /// Instantly colors the outline stroke of all selected symbols.
    pub fn set_stroke(&mut self, color: Color, width: f64) -> &mut Self {
        for child_id in &self.child_ids {
            if let Some(state) = self.builder.states.get_mut(*child_id) {
                state.stroke = StrokeBrush::new(color, width);
                self.builder
                    .commands
                    .entity(state.entity)
                    .insert(StrokeBrush::new(color, width));
            }
        }
        self
    }

    /// Prepares a parallel coordinated animation sequence for all selected entities.
    pub fn animate(&mut self) -> CoordinatedAnimationBuilder<'_, 'w, 's, 'b> {
        CoordinatedAnimationBuilder {
            builder: self.builder,
            child_ids: self.child_ids.clone(),
            duration: 1.0,
            rate_func: gaanim_math::prelude::RateFunc::Smooth,
        }
    }
}

/// Fluent builder to configure and play parallel animations across a selection of Mobjects.
pub struct CoordinatedAnimationBuilder<'a, 'w, 's, 'b> {
    builder: &'a mut SceneBuilder<'w, 's, 'b>,
    child_ids: Vec<ObjectId>,
    duration: f64,
    rate_func: gaanim_math::prelude::RateFunc,
}

impl<'a, 'w, 's, 'b> CoordinatedAnimationBuilder<'a, 'w, 's, 'b> {
    pub fn duration(mut self, d: f64) -> Self {
        self.duration = d;
        self
    }

    pub fn rate_func(mut self, r: gaanim_math::prelude::RateFunc) -> Self {
        self.rate_func = r;
        self
    }

    pub fn smooth(mut self) -> Self {
        self.rate_func = gaanim_math::prelude::RateFunc::Smooth;
        self
    }

    pub fn linear(mut self) -> Self {
        self.rate_func = gaanim_math::prelude::RateFunc::Linear;
        self
    }

    pub fn spring(mut self) -> Self {
        self.rate_func = gaanim_math::prelude::RateFunc::Spring {
            stiffness: 90.0,
            damping: 12.0,
        };
        self
    }

    /// Play a shift/translation animation on all selected sub-elements in parallel.
    pub fn shift_2d(self, x: f64, y: f64) {
        let mut anims = Vec::new();
        for id in self.child_ids {
            anims.push(
                MobjectRef { id }
                    .shift_2d(x, y)
                    .duration(self.duration)
                    .rate_func(self.rate_func.clone()),
            );
        }
        self.builder.play_parallel(anims);
    }

    /// Play a fade out animation on all selected sub-elements in parallel.
    pub fn fade_out(self) {
        let mut anims = Vec::new();
        for id in self.child_ids {
            anims.push(
                MobjectRef { id }
                    .fade_out()
                    .duration(self.duration)
                    .rate_func(self.rate_func.clone()),
            );
        }
        self.builder.play_parallel(anims);
    }

    /// Play a scale animation on all selected sub-elements in parallel.
    pub fn scale_uniform(self, factor: f64) {
        let mut anims = Vec::new();
        for id in self.child_ids {
            anims.push(
                MobjectRef { id }
                    .scale_uniform(factor)
                    .duration(self.duration)
                    .rate_func(self.rate_func.clone()),
            );
        }
        self.builder.play_parallel(anims);
    }

    /// Play a fill color interpolation on all selected sub-elements in parallel.
    pub fn fill_color_to(self, color: Color) {
        let mut anims = Vec::new();
        for id in self.child_ids {
            anims.push(
                MobjectRef { id }
                    .fill_color_to(color)
                    .duration(self.duration)
                    .rate_func(self.rate_func.clone()),
            );
        }
        self.builder.play_parallel(anims);
    }
}

/// The high-level fluent API builder for constructing gaanim scenes.
///
/// Manages auto-incrementing ObjectId generation, relative layouts, active states,
/// and sequential/parallel animation clip registration on the Timeline clock.
pub struct SceneBuilder<'w, 's, 'a> {
    pub commands: &'a mut Commands<'w, 's>,
    pub timeline: &'a mut Timeline,
    pub font_registry: &'a FontRegistry,
    pub text_config: &'a gaanim_text::prelude::TextConfig,
    pub id_counter: u32,
    pub current_time: f64,
    pub states: MobjectStateMap,
    /// Typographic metrics retained for text-specific baseline positioning.
    pub text_metrics: HashMap<ObjectId, gaanim_text::prelude::TextMetrics>,
    pub default_track: TrackId,
    mobject_tracks: HashMap<ObjectId, TrackId>,
    mobject_names: HashMap<ObjectId, String>,
    next_track: u32,
    current_label: Option<String>,
    /// The currently active scene (None when outside any scene scope).
    pub current_scene: Option<SceneId>,
    /// Tracks the current value of each float signal / value tracker
    pub float_signals: HashMap<ObjectId, f64>,
    /// Objects whose scene membership is intentionally global at the current authoring cursor.
    persistent_objects: HashSet<ObjectId>,
    /// Objects whose membership has an explicit reuse/persist/release schedule in this scene.
    membership_managed_objects: HashSet<ObjectId>,
    /// Transient marks created by `TextSelection.cancel()`, keyed by their
    /// owning Text root so the next replacing transition can retire them.
    text_cancellation_marks: HashMap<ObjectId, Vec<ObjectId>>,
    /// Selected glyphs dimmed by `TextSelection.cancel()`. They leave with the
    /// same owning Text instead of lingering behind a replacement.
    text_canceled_term_children: HashMap<ObjectId, Vec<ObjectId>>,
}

impl<'w, 's, 'a> SceneBuilder<'w, 's, 'a> {
    fn register_textual_hierarchy(
        &mut self,
        parent_id: ObjectId,
        entity: Entity,
        bounds: Bounds3D,
        fill: Option<Brush>,
        stroke: StrokeBrush,
        child_spans: Vec<HierarchyChild>,
        metrics: Option<gaanim_text::prelude::TextMetrics>,
    ) -> MobjectRef {
        let parent_path = compose_child_paths(&child_spans);
        let child_ids = child_spans.iter().map(|child| child.id).collect();

        for child in &child_spans {
            self.tag_entity(child.entity);
            // Text and Typst are compiled as individual glyph entities. Keep
            // the ECS hierarchy in sync with `MobjectState::parent` so
            // transforms, opacity, and visibility applied to the textual
            // parent propagate to every glyph.
            self.commands
                .entity(child.entity)
                .set_parent_in_place(entity);
            let child_state = MobjectState {
                path: child.path.clone(),
                bounds: child.bounds,
                transform: child.transform,
                opacity: 1.0,
                fill: child.fill.clone(),
                stroke: child.stroke.clone(),
                entity: child.entity,
                child_spans: Vec::new(),
                children: Vec::new(),
                parent: Some(parent_id),
                exclude_from_parent_draw: false,
            };
            self.states.insert(child.id, child_state);
        }

        self.tag_entity(entity);
        let state = MobjectState {
            path: parent_path,
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill,
            stroke,
            entity,
            child_spans,
            children: child_ids,
            parent: None,
            exclude_from_parent_draw: false,
        };
        self.states.insert(parent_id, state);
        if let Some(metrics) = metrics {
            self.text_metrics.insert(parent_id, metrics);
        }

        MobjectRef { id: parent_id }
    }

    fn hide_child_spans_now(&mut self, children: &[HierarchyChild]) {
        for child in children {
            self.commands.entity(child.entity).insert(Opacity(0.0));
        }
    }

    pub(crate) fn hide_visuals_now(&mut self, state: &MobjectState) {
        self.commands.entity(state.entity).insert(Opacity(0.0));
        self.hide_child_spans_now(&state.child_spans);
    }

    pub(crate) fn schedule_show_hierarchy(
        &mut self,
        root_id: ObjectId,
        state: &MobjectState,
        parent_track: TrackId,
        time: f64,
    ) {
        // Restore every visual descendant. This matters for composite reactive
        // objects: their children may have been hidden individually before the
        // group itself receives its entry animation.
        let hierarchy = self.hierarchy_ids(root_id);
        for target in hierarchy {
            let opacity = if target == root_id {
                state.opacity
            } else {
                self.states
                    .get(target)
                    .map(|child_state| child_state.opacity)
                    .unwrap_or(1.0)
            };
            self.timeline.add_clip(
                parent_track,
                time,
                0.0,
                ClipPayload::Animation(AnimationSpec {
                    target,
                    lens: PropertyLensSpec::Opacity {
                        from: 0.0,
                        to: opacity,
                    },
                    rate_func: gaanim_math::RateFunc::Linear,
                    delay: 0.0,
                    label: None,
                }),
            );
        }
    }

    /// Make a previously hidden hierarchy visible at the current playhead.
    pub(crate) fn schedule_show_now(&mut self, root_id: ObjectId) {
        self.schedule_show_at(root_id, self.current_time);
    }

    /// Make a previously hidden hierarchy visible at an authored timeline time.
    pub(crate) fn schedule_show_at(&mut self, root_id: ObjectId, time: f64) {
        let Some(state) = self.states.get(root_id).cloned() else {
            return;
        };
        let track = self.ensure_track(root_id);
        self.schedule_show_hierarchy(root_id, &state, track, time);
    }

    /// Make only the requested hierarchy root visible. This is used by
    /// transform-only animations on aggregate groups so that moving a parent
    /// does not become an implicit entry animation for deferred descendants.
    pub(crate) fn schedule_show_root_at(&mut self, root_id: ObjectId, time: f64) {
        let Some(state) = self.states.get(root_id) else {
            return;
        };
        let opacity = state.opacity;
        let track = self.ensure_track(root_id);
        self.timeline.add_clip(
            track,
            time,
            0.0,
            ClipPayload::Animation(AnimationSpec {
                target: root_id,
                lens: PropertyLensSpec::Opacity {
                    from: 0.0,
                    to: opacity,
                },
                rate_func: gaanim_math::RateFunc::Linear,
                delay: 0.0,
                label: None,
            }),
        );
    }

    /// Schedule visibility for every descendant except the root. The root is
    /// omitted when its entry animation owns the root opacity, for example
    /// `FadeIn` and `FadeInFrom`.
    pub(crate) fn schedule_show_descendants_at(&mut self, root_id: ObjectId, time: f64) {
        if !self.states.contains_key(root_id) {
            return;
        }
        let track = self.ensure_track(root_id);
        for target in self
            .hierarchy_ids(root_id)
            .into_iter()
            .filter(|id| *id != root_id)
        {
            let opacity = self
                .states
                .get(target)
                .map(|child_state| child_state.opacity)
                .unwrap_or(1.0);
            self.timeline.add_clip(
                track,
                time,
                0.0,
                ClipPayload::Animation(AnimationSpec {
                    target,
                    lens: PropertyLensSpec::Opacity {
                        from: 0.0,
                        to: opacity,
                    },
                    rate_func: gaanim_math::RateFunc::Linear,
                    delay: 0.0,
                    label: None,
                }),
            );
        }
    }

    /// Schedule an instantaneous hide for a root and its generated text
    /// children. This keeps semantic segment transitions correct even though all
    /// entities are spawned up front for timeline seeking.
    pub(crate) fn schedule_hide_hierarchy(&mut self, root_id: ObjectId) {
        let Some(state) = self.states.get(root_id).cloned() else {
            return;
        };
        let track = self.ensure_track(root_id);
        let time = self.current_time;
        self.timeline.add_clip(
            track,
            time,
            0.0,
            ClipPayload::Animation(AnimationSpec {
                target: root_id,
                lens: PropertyLensSpec::Opacity {
                    from: state.opacity,
                    to: 0.0,
                },
                rate_func: gaanim_math::RateFunc::Linear,
                delay: 0.0,
                label: None,
            }),
        );
        for child in state.child_spans {
            let opacity = self
                .states
                .get(child.id)
                .map(|child_state| child_state.opacity)
                .unwrap_or(1.0);
            self.timeline.add_clip(
                track,
                time,
                0.0,
                ClipPayload::Animation(AnimationSpec {
                    target: child.id,
                    lens: PropertyLensSpec::Opacity {
                        from: opacity,
                        to: 0.0,
                    },
                    rate_func: gaanim_math::RateFunc::Linear,
                    delay: 0.0,
                    label: None,
                }),
            );
        }
    }

    /// Computes the true world-space transform of a Mobject by walking up the parent chain.
    /// This is necessary during group creation to calculate accurate world bounds for nested children.
    pub fn get_world_transform(&self, id: ObjectId) -> SpatialTransform {
        let mut current_id = Some(id);
        let mut world_affine = gaanim_core::kurbo::Affine::IDENTITY;

        while let Some(obj_id) = current_id {
            if let Some(state) = self.states.get(obj_id) {
                world_affine = state.transform.to_affine_2d() * world_affine;
                current_id = state.parent;
            } else {
                break;
            }
        }

        SpatialTransform::from_affine_2d(&world_affine)
    }

    /// Creates a new `SceneBuilder` wrapping the Bevy `Commands` context, `Timeline` resource, and `FontRegistry`.
    pub fn new(
        commands: &'a mut Commands<'w, 's>,
        timeline: &'a mut Timeline,
        font_registry: &'a FontRegistry,
        text_config: &'a gaanim_text::prelude::TextConfig,
    ) -> Self {
        // Ensure a default track exists on the timeline
        let default_track = if let Some(track_id) = timeline.tracks.keys().next() {
            track_id
        } else {
            timeline.add_track("Main Graphics", 0)
        };

        Self {
            commands,
            timeline,
            font_registry,
            text_config,
            id_counter: 0,
            current_time: 0.0,
            states: MobjectStateMap::new(),
            text_metrics: HashMap::new(),
            default_track,
            mobject_tracks: HashMap::new(),
            mobject_names: HashMap::new(),
            next_track: 0,
            current_label: None,
            current_scene: None,
            float_signals: HashMap::new(),
            persistent_objects: HashSet::new(),
            membership_managed_objects: HashSet::new(),
            text_cancellation_marks: HashMap::new(),
            text_canceled_term_children: HashMap::new(),
        }
    }

    /// Return a root and every visual descendant in its compiled hierarchy.
    pub(crate) fn hierarchy_ids(&self, root_id: ObjectId) -> Vec<ObjectId> {
        let mut ids = Vec::new();
        let mut stack = vec![root_id];
        let mut visited = HashSet::new();
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            ids.push(id);
            if let Some(state) = self.states.get(id) {
                stack.extend(state.children.iter().copied());
                stack.extend(state.child_spans.iter().map(|child| child.id));
            }
        }
        ids
    }

    /// Schedule a reversible scene-membership change for a complete hierarchy.
    pub(crate) fn schedule_scene_membership(
        &mut self,
        root_id: ObjectId,
        scene: Option<SceneId>,
        time: f64,
    ) {
        let track = self.ensure_track(root_id);
        for target in self.hierarchy_ids(root_id) {
            self.timeline.add_clip(
                track,
                time,
                0.0,
                ClipPayload::SetSceneMember { target, scene },
            );
        }
    }

    /// Track persistence while compiling so transforms do not localize global objects.
    pub(crate) fn set_hierarchy_persistent(&mut self, root_id: ObjectId, persistent: bool) {
        for id in self.hierarchy_ids(root_id) {
            if persistent {
                self.persistent_objects.insert(id);
            } else {
                self.persistent_objects.remove(&id);
            }
        }
    }

    pub(crate) fn is_persistent(&self, id: ObjectId) -> bool {
        self.persistent_objects.contains(&id)
    }

    pub(crate) fn manage_hierarchy_membership(&mut self, root_id: ObjectId) {
        self.membership_managed_objects
            .extend(self.hierarchy_ids(root_id));
    }

    fn has_managed_membership(&self, id: ObjectId) -> bool {
        self.membership_managed_objects.contains(&id)
    }

    /// Returns the per-mobject track for the given target, creating a new
    /// numbered track if this is the first time we see this ObjectId.
    pub(crate) fn ensure_track(&mut self, target: ObjectId) -> TrackId {
        let current_scene = self.current_scene;
        *self.mobject_tracks.entry(target).or_insert_with(|| {
            self.next_track += 1;
            let name = self
                .mobject_names
                .get(&target)
                .cloned()
                .unwrap_or_else(|| format!("Object {}", self.next_track));
            let track_id = self.timeline.add_track(&name, self.next_track as i32);
            if let Some(track) = self.timeline.tracks.get_mut(track_id) {
                track.object_id = Some(target);
                track.scene = current_scene;
            }
            track_id
        })
    }

    /// Begins a new scene scope. All mobjects spawned and animations scheduled
    /// after this call will belong to the scene until `end_scene()` is called.
    pub fn begin_scene(&mut self, name: &str) -> SceneId {
        self.membership_managed_objects.clear();
        let scene_id = self.timeline.add_scene(name);
        self.current_scene = Some(scene_id);
        // Insert SceneStart marker clip at the current time
        self.timeline.add_clip(
            self.default_track,
            self.current_time,
            0.0,
            ClipPayload::SceneStart(scene_id),
        );
        // Index the scene start time for O(log n) lookup in scene_at().
        self.timeline.index_scene(scene_id, self.current_time);
        scene_id
    }

    /// Ends the current scene scope.
    pub fn end_scene(&mut self) {
        if let Some(scene_id) = self.current_scene {
            self.timeline.add_clip(
                self.default_track,
                self.current_time,
                0.0,
                ClipPayload::SceneEnd(scene_id),
            );
            self.current_scene = None;
        }
    }

    /// Executes a closure within a scene scope, automatically handling begin/end.
    pub fn scene_scope<F>(&mut self, name: &str, f: F) -> SceneId
    where
        F: FnOnce(&mut Self),
    {
        let id = self.begin_scene(name);
        f(self);
        self.end_scene();
        id
    }

    /// Tags an entity with the current scene's `SceneMember` component,
    /// if currently inside a scene scope. Call this from ALL entity
    /// spawning paths (not just `MobjectSpawnBuilder::spawn()`).
    pub fn tag_entity(&mut self, entity: Entity) {
        if let Some(scene_id) = self.current_scene {
            self.commands.entity(entity).insert(SceneMember(scene_id));
        }
    }

    /// Records a transition from the current scene to a target scene.
    pub fn transition_to(&mut self, target: SceneId, transition: TransitionType) {
        let current = self
            .current_scene
            .expect("Must be inside a scene to call transition_to");
        self.timeline.connect(current, target, transition);
    }

    fn anim_label(ty: &AnimationType) -> &'static str {
        match ty {
            AnimationType::CameraState { .. }
            | AnimationType::CameraPosition { .. }
            | AnimationType::CameraPositionSource { .. }
            | AnimationType::CameraZoom { .. }
            | AnimationType::CameraZoomSource { .. }
            | AnimationType::CameraRotation { .. }
            | AnimationType::CameraRotationSource { .. }
            | AnimationType::CameraFrame { .. }
            | AnimationType::CameraFrameMany { .. }
            | AnimationType::CameraFollow { .. }
            | AnimationType::CameraFollowEndpoint { .. }
            | AnimationType::CameraShake { .. }
            | AnimationType::CameraLookAt { .. }
            | AnimationType::CameraLookAtSource { .. }
            | AnimationType::CameraOrbit { .. }
            | AnimationType::CameraPerspective { .. }
            | AnimationType::CameraOrthographic { .. }
            | AnimationType::CameraReset
            | AnimationType::CameraDolly { .. } => "Camera",
            AnimationType::GltfAnimation { .. } => "Action",
            AnimationType::Properties { .. } => "Properties",
            AnimationType::Write { .. } => "Write",
            AnimationType::Create { .. } => "Create",
            AnimationType::Create3D => "Create3D",
            AnimationType::Unwrite { .. } => "Unwrite",
            AnimationType::Uncreate { .. } => "Uncreate",
            AnimationType::TranslateTo { .. }
            | AnimationType::TranslateAnchorTo { .. }
            | AnimationType::TranslateToAnchorPoint { .. }
            | AnimationType::TranslateBy { .. } => "Move",
            AnimationType::RotateTo { .. }
            | AnimationType::RotateBy { .. }
            | AnimationType::RotateBy3D { .. } => "Rotate",
            AnimationType::ScaleTo { .. }
            | AnimationType::ScaleUniform { .. }
            | AnimationType::ScaleBy3D { .. } => "Scale",
            AnimationType::FadeTo { .. }
            | AnimationType::FadeIn
            | AnimationType::FadeOut
            | AnimationType::FadeInFrom { .. } => "Fade",
            AnimationType::FillColorTo { .. } => "Fill",
            AnimationType::TextSelectionProperties { .. } => "TextSelectionProperties",
            AnimationType::Material3DTo { .. } => "Material3D",
            AnimationType::FillLevelTo { .. } => "FillLevel",
            AnimationType::SurroundingRectRetarget { .. } => "Retarget",
            AnimationType::StrokeColorTo { .. } => "Stroke",
            AnimationType::StrokeWidthTo { .. } => "StrokeW",
            AnimationType::GrowFromCenter => "Grow",
            AnimationType::ShrinkToCenter => "Shrink",
            AnimationType::SpinInFromNothing => "SpinIn",
            AnimationType::Indicate { .. } => "Indicate",
            AnimationType::TextTransition { copy: true, .. }
            | AnimationType::TextSelectionTransform { copy: true, .. } => "CopyText",
            AnimationType::TextTransition { copy: false, .. }
            | AnimationType::TextSelectionTransform { copy: false, .. } => "MorphText",
            AnimationType::TextSelection { effect, .. } => match effect {
                TextSelectionEffect::Focus => "FocusText",
                TextSelectionEffect::Cancel => "CancelText",
                TextSelectionEffect::Brace { .. } => "BraceText",
                TextSelectionEffect::Annotate { .. } => "AnnotateText",
                _ => "TextSelection",
            },
            AnimationType::FadeTransform { .. }
            | AnimationType::Transform { .. }
            | AnimationType::ReplacementTransform { .. } => "Morph",
            AnimationType::Wiggle => "Wiggle",
            AnimationType::GrowFromPoint { .. } | AnimationType::GrowFromEdge { .. } => "Grow",
            AnimationType::DrawBorderThenFill { .. } => "DrawFill",
            AnimationType::Flash { .. } => "Flash",
            AnimationType::Circumscribe { .. } => "Circum",
            AnimationType::MoveAlongPath { .. } | AnimationType::MoveAlongPath3D { .. } => "Follow",
            AnimationType::GrowArrow => "Arrow",
            AnimationType::SignalFloat { .. } => "Signal",
            AnimationType::ShowPassingFlash { .. } => "ShowPassingFlash",
        }
    }

    /// Allocates a new, stable, auto-incremented ObjectId starting at index 0.
    pub fn next_id(&mut self) -> ObjectId {
        let id = ObjectId::from_parts(self.id_counter, 1);
        self.id_counter += 1;
        id
    }

    /// Advances the internal timeline playhead by the specified duration.
    pub fn wait(&mut self, duration: f64) {
        self.current_time += duration;
    }

    /// Schedules removal of a continuous `Updater` component at the current
    /// timeline time instead of applying it immediately during scene compile.
    pub(crate) fn schedule_remove_updater(&mut self, target: ObjectId) {
        let track = self.ensure_track(target);
        self.timeline.add_clip(
            track,
            self.current_time,
            0.0,
            ClipPayload::RemoveUpdater { target },
        );
    }

    /// Registers an explicit interactive stop at the current timeline playhead.
    pub fn stop(&mut self) {
        self.timeline.add_clip(
            self.default_track,
            self.current_time,
            0.0,
            ClipPayload::Stop,
        );
    }

    /// Sequences a single animation clip on the timeline and advances the playhead.
    pub fn play(&mut self, anim: AnimationBuilder) {
        let duration = anim.delay.max(0.0) + anim.duration;
        self.play_internal(anim);
        self.current_time += duration;
    }

    /// Plays multiple animation clips starting at the same time,
    /// advancing the playhead by the maximum duration among them.
    pub fn play_parallel(&mut self, anims: Vec<AnimationBuilder>) {
        let mut max_duration = 0.0;
        for anim in anims {
            let total_duration = anim.delay.max(0.0) + anim.duration;
            if total_duration > max_duration {
                max_duration = total_duration;
            }
            self.play_internal(anim);
        }
        self.current_time += max_duration;
    }

    /// Schedule an animation at the current cursor without advancing it.
    ///
    /// Used for transient helpers such as cancellation marks that must fade
    /// concurrently with the equation transition that follows them.
    pub(crate) fn play_at_current_time(&mut self, anim: AnimationBuilder) {
        self.play_internal(anim);
    }

    fn equation_pair_destination(
        &self,
        moving: ObjectId,
        destination: ObjectId,
    ) -> Option<SpatialTransform> {
        let moving_state = self.states.get(moving)?;
        let parent_world = moving_state
            .parent
            .map(|parent| self.get_world_transform(parent).to_affine_2d())
            .unwrap_or(gaanim_core::kurbo::Affine::IDENTITY);
        let destination_world = self.get_world_transform(destination).to_affine_2d();
        Some(SpatialTransform::from_affine_2d(
            &(parent_world.inverse() * destination_world),
        ))
    }

    fn plan_equation_transition(
        &self,
        source_parent: ObjectId,
        target_parent: ObjectId,
        semantic_groups: &[(Vec<ObjectId>, Vec<ObjectId>)],
        auto_match: bool,
    ) -> EquationTransitionPlan {
        let (Some(source_state), Some(target_state)) = (
            self.states.get(source_parent),
            self.states.get(target_parent),
        ) else {
            return EquationTransitionPlan::default();
        };
        let source_ids: Vec<_> = source_state
            .child_spans
            .iter()
            .map(|child| child.id)
            .collect();
        let target_ids: Vec<_> = target_state
            .child_spans
            .iter()
            .map(|child| child.id)
            .collect();
        let source_keys: HashMap<_, _> = source_state
            .child_spans
            .iter()
            .map(|child| (child.id, child.span.character.to_string()))
            .collect();
        let target_keys: HashMap<_, _> = target_state
            .child_spans
            .iter()
            .map(|child| (child.id, child.span.character.to_string()))
            .collect();

        let mut pairs = Vec::new();
        let mut used_source = HashSet::new();
        let mut used_target = HashSet::new();
        let mut considered_source = HashSet::new();
        let mut considered_target = HashSet::new();

        for (group_source, group_target) in semantic_groups {
            let group_source: Vec<_> = group_source
                .iter()
                .copied()
                .filter(|id| source_keys.contains_key(id) && !used_source.contains(id))
                .collect();
            let group_target: Vec<_> = group_target
                .iter()
                .copied()
                .filter(|id| target_keys.contains_key(id) && !used_target.contains(id))
                .collect();
            considered_source.extend(group_source.iter().copied());
            considered_target.extend(group_target.iter().copied());

            let source_group_keys: Vec<_> = group_source
                .iter()
                .map(|id| source_keys.get(id).cloned())
                .collect();
            let target_group_keys: Vec<_> = group_target
                .iter()
                .map(|id| target_keys.get(id).cloned())
                .collect();
            let mut group_pairs = Vec::new();
            for (source_index, target_index) in lcs_match(&source_group_keys, &target_group_keys) {
                let pair = (group_source[source_index], group_target[target_index]);
                used_source.insert(pair.0);
                used_target.insert(pair.1);
                group_pairs.push(pair);
            }

            let remaining_source: Vec<_> = group_source
                .iter()
                .copied()
                .filter(|id| !used_source.contains(id))
                .collect();
            let remaining_target: Vec<_> = group_target
                .iter()
                .copied()
                .filter(|id| !used_target.contains(id))
                .collect();
            let source_items: Vec<_> = remaining_source
                .iter()
                .enumerate()
                .filter_map(|(index, id)| {
                    self.build_match_item(*id, None).map(|mut item| {
                        item.index = index;
                        item
                    })
                })
                .collect();
            let target_items: Vec<_> = remaining_target
                .iter()
                .enumerate()
                .filter_map(|(index, id)| {
                    self.build_match_item(*id, None).map(|mut item| {
                        item.index = index;
                        item
                    })
                })
                .collect();
            let semantic_result = gaanim_math::matching::match_items(
                &source_items,
                &target_items,
                &MatchingConfig::default(),
            );
            for (source_index, target_index) in semantic_result.pairs {
                let pair = (
                    remaining_source[source_index],
                    remaining_target[target_index],
                );
                used_source.insert(pair.0);
                used_target.insert(pair.1);
                group_pairs.push(pair);
            }

            pairs.extend(group_pairs);
        }

        if auto_match {
            considered_source.extend(source_ids.iter().copied());
            considered_target.extend(target_ids.iter().copied());
            let remaining_source: Vec<_> = source_ids
                .iter()
                .copied()
                .filter(|id| !used_source.contains(id))
                .collect();
            let remaining_target: Vec<_> = target_ids
                .iter()
                .copied()
                .filter(|id| !used_target.contains(id))
                .collect();
            let source_items: Vec<_> = remaining_source
                .iter()
                .enumerate()
                .filter_map(|(index, id)| {
                    self.build_match_item(*id, source_keys.get(id).cloned())
                        .map(|mut item| {
                            item.index = index;
                            item
                        })
                })
                .collect();
            let target_items: Vec<_> = remaining_target
                .iter()
                .enumerate()
                .filter_map(|(index, id)| {
                    self.build_match_item(*id, target_keys.get(id).cloned())
                        .map(|mut item| {
                            item.index = index;
                            item
                        })
                })
                .collect();
            let result = gaanim_math::matching::match_items(
                &source_items,
                &target_items,
                &MatchingConfig {
                    mode: MatchingMode::Tex,
                    ..Default::default()
                },
            );
            for (source_index, target_index) in result.pairs {
                let pair = (
                    remaining_source[source_index],
                    remaining_target[target_index],
                );
                used_source.insert(pair.0);
                used_target.insert(pair.1);
                pairs.push(pair);
            }
        }

        let leaving = considered_source
            .into_iter()
            .filter(|id| !used_source.contains(id))
            .collect();
        let entering = considered_target
            .into_iter()
            .filter(|id| !used_target.contains(id))
            .collect();

        EquationTransitionPlan {
            pairs,
            leaving,
            entering,
        }
    }

    fn schedule_equation_pair_morph(
        &mut self,
        source: ObjectId,
        target: ObjectId,
        start: f64,
        duration: f64,
        mode: EquationTransitionMode,
    ) {
        let (Some(source_state), Some(target_state)) = (
            self.states.get(source).cloned(),
            self.states.get(target).cloned(),
        ) else {
            return;
        };
        let (moving, from_state, from_transform, to_state, to_transform) = match mode {
            EquationTransitionMode::Replace => {
                let Some(to_transform) = self.equation_pair_destination(source, target) else {
                    return;
                };
                (
                    source,
                    source_state.clone(),
                    source_state.transform,
                    target_state.clone(),
                    to_transform,
                )
            }
            EquationTransitionMode::Copy => {
                let Some(from_transform) = self.equation_pair_destination(target, source) else {
                    return;
                };
                (
                    target,
                    source_state.clone(),
                    from_transform,
                    target_state.clone(),
                    target_state.transform,
                )
            }
        };
        let track = self.ensure_track(moving);
        let rate_func = gaanim_math::RateFunc::Smooth;
        let add = |timeline: &mut Timeline, lens: PropertyLensSpec, label: Option<String>| {
            timeline.add_clip(
                track,
                start,
                duration,
                ClipPayload::Animation(AnimationSpec {
                    target: moving,
                    lens,
                    rate_func: rate_func.clone(),
                    delay: 0.0,
                    label,
                }),
            );
        };
        add(
            self.timeline,
            PropertyLensSpec::PathMorph {
                from: (*from_state.path).clone(),
                to: (*to_state.path).clone(),
            },
            Some("EquationSemanticMorph".to_string()),
        );
        add(
            self.timeline,
            PropertyLensSpec::Translation {
                from: from_transform.translation,
                to: to_transform.translation,
            },
            None,
        );
        add(
            self.timeline,
            PropertyLensSpec::Rotation {
                from: from_transform.rotation,
                to: to_transform.rotation,
            },
            None,
        );
        add(
            self.timeline,
            PropertyLensSpec::Scale {
                from: from_transform.scale,
                to: to_transform.scale,
            },
            None,
        );
        let from_fill = from_state
            .fill
            .as_ref()
            .and_then(extract_brush_color)
            .unwrap_or(Color::WHITE);
        let to_fill = to_state
            .fill
            .as_ref()
            .and_then(extract_brush_color)
            .unwrap_or(Color::WHITE);
        add(
            self.timeline,
            PropertyLensSpec::FillColor {
                from: from_fill,
                to: to_fill,
            },
            None,
        );

        if mode == EquationTransitionMode::Copy {
            self.timeline.add_clip(
                track,
                start,
                0.0,
                ClipPayload::Animation(AnimationSpec {
                    target,
                    lens: PropertyLensSpec::Opacity {
                        from: 0.0,
                        to: target_state.opacity,
                    },
                    rate_func: gaanim_math::RateFunc::Linear,
                    delay: 0.0,
                    label: None,
                }),
            );
        } else {
            let source_track = self.ensure_track(source);
            self.timeline.add_clip(
                source_track,
                start + duration,
                0.0,
                ClipPayload::Animation(AnimationSpec {
                    target: source,
                    lens: PropertyLensSpec::Opacity {
                        from: source_state.opacity,
                        to: 0.0,
                    },
                    rate_func: gaanim_math::RateFunc::Linear,
                    delay: 0.0,
                    label: Some("EquationHandoff".to_string()),
                }),
            );
            let target_track = self.ensure_track(target);
            self.timeline.add_clip(
                target_track,
                start + duration,
                0.0,
                ClipPayload::Animation(AnimationSpec {
                    target,
                    lens: PropertyLensSpec::Opacity {
                        from: 0.0,
                        to: target_state.opacity,
                    },
                    rate_func: gaanim_math::RateFunc::Linear,
                    delay: 0.0,
                    label: Some("EquationHandoff".to_string()),
                }),
            );
            if let Some(state) = self.states.get_mut(source) {
                state.opacity = 0.0;
            }
        }
    }

    fn equation_center_collapsed_transform(state: &MobjectState) -> SpatialTransform {
        let local_center = if state.path.elements().is_empty() {
            let center = state.bounds.center();
            gaanim_core::kurbo::Point::new(center.x, center.y)
        } else {
            let bounds = state.path.bounding_box();
            gaanim_core::kurbo::Point::new(
                (bounds.x0 + bounds.x1) * 0.5,
                (bounds.y0 + bounds.y1) * 0.5,
            )
        };
        let center_in_parent = state.transform.to_affine_2d() * local_center;
        let mut collapsed = state.transform;
        collapsed.scale = DVec3::ZERO;
        collapsed.translation.x = center_in_parent.x - collapsed.anchor.x;
        collapsed.translation.y = center_in_parent.y - collapsed.anchor.y;
        collapsed
    }

    fn schedule_equation_leaving(&mut self, source: ObjectId, start: f64, duration: f64) {
        let Some(state) = self.states.get(source).cloned() else {
            return;
        };
        let destination = Self::equation_center_collapsed_transform(&state);
        let track = self.ensure_track(source);
        for lens in [
            PropertyLensSpec::Translation {
                from: state.transform.translation,
                to: destination.translation,
            },
            PropertyLensSpec::Scale {
                from: state.transform.scale,
                to: destination.scale,
            },
        ] {
            self.timeline.add_clip(
                track,
                start,
                duration,
                ClipPayload::Animation(AnimationSpec {
                    target: source,
                    lens,
                    rate_func: gaanim_math::RateFunc::Smooth,
                    delay: 0.0,
                    label: Some("EquationCollapse".to_string()),
                }),
            );
        }
        self.timeline.add_clip(
            track,
            start + duration,
            0.0,
            ClipPayload::Animation(AnimationSpec {
                target: source,
                lens: PropertyLensSpec::Opacity {
                    from: state.opacity,
                    to: 0.0,
                },
                rate_func: gaanim_math::RateFunc::Linear,
                delay: 0.0,
                label: Some("EquationCollapseHandoff".to_string()),
            }),
        );
        if let Some(state) = self.states.get_mut(source) {
            state.opacity = 0.0;
            state.transform = destination;
        }
    }

    fn schedule_equation_entering(&mut self, target: ObjectId, start: f64, duration: f64) {
        let Some(state) = self.states.get(target).cloned() else {
            return;
        };
        let destination = state.transform;
        let origin = Self::equation_center_collapsed_transform(&state);
        let track = self.ensure_track(target);
        self.timeline.add_clip(
            track,
            start,
            0.0,
            ClipPayload::Animation(AnimationSpec {
                target,
                lens: PropertyLensSpec::Opacity {
                    from: 0.0,
                    to: state.opacity,
                },
                rate_func: gaanim_math::RateFunc::Linear,
                delay: 0.0,
                label: Some("EquationEmergeHandoff".to_string()),
            }),
        );
        for lens in [
            PropertyLensSpec::Translation {
                from: origin.translation,
                to: destination.translation,
            },
            PropertyLensSpec::Scale {
                from: origin.scale,
                to: destination.scale,
            },
        ] {
            self.timeline.add_clip(
                track,
                start,
                duration,
                ClipPayload::Animation(AnimationSpec {
                    target,
                    lens,
                    rate_func: gaanim_math::RateFunc::Smooth,
                    delay: 0.0,
                    label: Some("EquationEmerge".to_string()),
                }),
            );
        }
    }

    pub(crate) fn play_equation_transition(
        &mut self,
        source_parent: ObjectId,
        target_parent: ObjectId,
        semantic_groups: Vec<(Vec<ObjectId>, Vec<ObjectId>)>,
        duration: f64,
        mode: EquationTransitionMode,
        auto_match: bool,
    ) {
        if !duration.is_finite() || duration <= 0.0 {
            return;
        }
        if mode == EquationTransitionMode::Replace {
            self.fade_text_cancellation_artifacts(source_parent, duration);
        }
        let plan = self.plan_equation_transition(
            source_parent,
            target_parent,
            &semantic_groups,
            auto_match,
        );
        if plan.pairs.is_empty() && plan.leaving.is_empty() && plan.entering.is_empty() {
            return;
        }
        let target_ids: HashSet<_> = plan
            .pairs
            .iter()
            .map(|(_, target)| *target)
            .chain(plan.entering.iter().copied())
            .collect();
        for target in target_ids {
            if let Some(state) = self.states.get(target) {
                self.commands.entity(state.entity).insert(Opacity(0.0));
            }
        }

        let start = self.current_time;
        for (source, target) in plan.pairs {
            self.schedule_equation_pair_morph(source, target, start, duration, mode);
        }
        if mode == EquationTransitionMode::Replace {
            for source in plan.leaving {
                self.schedule_equation_leaving(source, start, duration * 0.55);
            }
        }
        for target in plan.entering {
            self.schedule_equation_entering(target, start + duration * 0.2, duration * 0.8);
        }
        self.current_time += duration;
    }

    fn fade_text_cancellation_artifacts(
        &mut self,
        source_parent: ObjectId,
        transition_duration: f64,
    ) {
        let duration = (transition_duration * 0.25).clamp(0.12, 0.3);
        let targets = self
            .text_cancellation_marks
            .remove(&source_parent)
            .into_iter()
            .flatten()
            .chain(
                self.text_canceled_term_children
                    .remove(&source_parent)
                    .into_iter()
                    .flatten(),
            )
            .collect::<HashSet<_>>();
        for target in targets {
            self.play_at_current_time(AnimationBuilder {
                target,
                anim_type: AnimationType::FadeOut,
                duration,
                rate_func: RateFunc::Smooth,
                delay: 0.0,
            });
        }
    }

    fn play_text_selection_focus_internal(
        &mut self,
        anim: AnimationBuilder,
        selected: &[ObjectId],
    ) {
        let selected = selected.iter().copied().collect::<HashSet<_>>();
        let all = self
            .states
            .get(anim.target)
            .map(|state| {
                state
                    .child_spans
                    .iter()
                    .map(|child| child.id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| selected.iter().copied().collect());
        for target in all {
            let anim_type = if selected.contains(&target) {
                AnimationType::Indicate {
                    color: None,
                    scale_factor: 1.12,
                }
            } else {
                AnimationType::FadeTo { to: 0.2 }
            };
            self.play_internal(AnimationBuilder {
                target,
                anim_type,
                duration: anim.duration,
                rate_func: if selected.contains(&target) {
                    RateFunc::ThereAndBack
                } else {
                    RateFunc::Smooth
                },
                delay: anim.delay,
            });
        }
    }

    fn text_selection_world_bounds(&self, targets: &[ObjectId]) -> Option<Bounds3D> {
        targets
            .iter()
            .filter_map(|target| {
                let state = self.states.get(*target)?;
                // Text child bounds are already expressed in the owning text's local
                // coordinate system. Applying the child's transform here would center
                // them a second time and shift selection decorations to the left.
                let world_transform = state
                    .parent
                    .map(|parent| self.get_world_transform(parent))
                    .unwrap_or_else(|| self.get_world_transform(*target));
                Some(state.bounds.transform_2d(&world_transform.to_affine_2d()))
            })
            .reduce(|bounds, next| bounds.union(&next))
    }

    fn text_selection_color(&self, targets: &[ObjectId]) -> Color {
        targets
            .iter()
            .find_map(|target| {
                self.states
                    .get(*target)
                    .and_then(|state| match &state.fill {
                        Some(Brush::Solid(color)) => Some(*color),
                        _ => None,
                    })
            })
            .unwrap_or(Color::WHITE)
    }

    fn play_text_selection_cancel_internal(
        &mut self,
        anim: AnimationBuilder,
        selected: &[ObjectId],
    ) {
        let Some(bounds) = self.text_selection_world_bounds(selected) else {
            return;
        };
        let pad = (bounds.width() * 0.08).max(3.0);
        let color = self.text_selection_color(selected);
        let strike = self
            .line(
                kurbo::Point::new(bounds.min.x - pad, bounds.min.y - pad * 0.25),
                kurbo::Point::new(bounds.max.x + pad, bounds.max.y + pad * 0.25),
            )
            .no_fill()
            .stroke(color, 3.0)
            .spawn();
        self.text_cancellation_marks
            .entry(anim.target)
            .or_default()
            .push(strike.id);
        self.text_canceled_term_children
            .entry(anim.target)
            .or_default()
            .extend(selected.iter().copied());
        self.play_internal(AnimationBuilder {
            target: strike.id,
            anim_type: AnimationType::Create {
                config: Default::default(),
            },
            duration: anim.duration,
            rate_func: anim.rate_func,
            delay: anim.delay,
        });
        for target in selected {
            self.play_internal(AnimationBuilder {
                target: *target,
                anim_type: AnimationType::FadeTo { to: 0.35 },
                duration: anim.duration,
                rate_func: RateFunc::Smooth,
                delay: anim.delay,
            });
        }
    }

    fn play_text_selection_brace_internal(
        &mut self,
        anim: AnimationBuilder,
        selected: &[ObjectId],
        label: String,
        above: bool,
    ) {
        let Some(bounds) = self.text_selection_world_bounds(selected) else {
            return;
        };
        let side = if above { 1.0 } else { -1.0 };
        let y = if above {
            bounds.max.y + 12.0
        } else {
            bounds.min.y - 12.0
        };
        let color = self.text_selection_color(selected);
        let brace = self
            .brace(
                kurbo::Point::new(bounds.min.x, y),
                kurbo::Point::new(bounds.max.x, y),
                -side * 10.0,
            )
            .no_fill()
            .stroke(color, 2.0)
            .spawn();
        let label_ref = self.text(&label, "Inter", 28.0);
        if let Some(state) = self.states.get_mut(label_ref.id) {
            state.transform.translation = DVec3::new(bounds.center().x, y + side * 25.0, 0.0);
            self.commands.entity(state.entity).insert(state.transform);
        }
        for (target, anim_type) in [
            (
                brace.id,
                AnimationType::Create {
                    config: Default::default(),
                },
            ),
            (label_ref.id, AnimationType::FadeIn),
        ] {
            self.play_internal(AnimationBuilder {
                target,
                anim_type,
                duration: anim.duration,
                rate_func: anim.rate_func.clone(),
                delay: anim.delay,
            });
        }
    }

    fn play_text_selection_annotate_internal(
        &mut self,
        anim: AnimationBuilder,
        selected: &[ObjectId],
        label: String,
        offset: DVec3,
    ) {
        let Some(bounds) = self.text_selection_world_bounds(selected) else {
            return;
        };
        let position = bounds.center() + offset;
        let label_ref = self.text(&label, "Inter", 28.0);
        if let Some(state) = self.states.get_mut(label_ref.id) {
            state.transform.translation = position;
            self.commands.entity(state.entity).insert(state.transform);
        }
        let color = self.text_selection_color(selected);
        let line = self
            .line(
                kurbo::Point::new(bounds.center().x, bounds.center().y),
                kurbo::Point::new(position.x, position.y),
            )
            .no_fill()
            .stroke(color, 2.0)
            .spawn();
        for (target, anim_type) in [
            (
                line.id,
                AnimationType::Create {
                    config: Default::default(),
                },
            ),
            (label_ref.id, AnimationType::FadeIn),
        ] {
            self.play_internal(AnimationBuilder {
                target,
                anim_type,
                duration: anim.duration,
                rate_func: anim.rate_func.clone(),
                delay: anim.delay,
            });
        }
    }

    fn play_text_selection_internal(
        &mut self,
        anim: AnimationBuilder,
        fragment: String,
        occurrence: Option<usize>,
        effect: TextSelectionEffect,
    ) {
        let selected = self
            .select_occurrence(MobjectRef { id: anim.target }, &fragment, occurrence)
            .child_ids;
        if selected.is_empty() {
            bevy::prelude::warn!(
                "text selection animation could not resolve fragment '{fragment}'"
            );
            return;
        }
        match effect {
            TextSelectionEffect::Focus => {
                self.play_text_selection_focus_internal(anim, &selected);
            }
            TextSelectionEffect::Cancel => {
                self.play_text_selection_cancel_internal(anim, &selected);
            }
            TextSelectionEffect::Brace { label, above } => {
                self.play_text_selection_brace_internal(anim, &selected, label, above);
            }
            TextSelectionEffect::Annotate { label, offset } => {
                self.play_text_selection_annotate_internal(anim, &selected, label, offset);
            }
            effect => {
                let count = selected.len();
                for (index, target) in selected.into_iter().enumerate() {
                    let (anim_type, rate_func, extra_delay) = match &effect {
                        TextSelectionEffect::Indicate => (
                            AnimationType::Indicate {
                                color: None,
                                scale_factor: 1.1,
                            },
                            RateFunc::ThereAndBack,
                            0.0,
                        ),
                        TextSelectionEffect::Pulse => (
                            AnimationType::Indicate {
                                color: None,
                                scale_factor: 1.16,
                            },
                            RateFunc::ThereAndBack,
                            0.0,
                        ),
                        TextSelectionEffect::Wiggle => {
                            (AnimationType::Wiggle, RateFunc::ThereAndBack, 0.0)
                        }
                        TextSelectionEffect::Wave => (
                            AnimationType::Wiggle,
                            RateFunc::ThereAndBack,
                            if count > 1 {
                                index as f64 * anim.duration * 0.35 / (count - 1) as f64
                            } else {
                                0.0
                            },
                        ),
                        TextSelectionEffect::Highlight => (
                            AnimationType::Circumscribe { color: None },
                            RateFunc::Smooth,
                            0.0,
                        ),
                        TextSelectionEffect::RevealFade => {
                            (AnimationType::FadeIn, RateFunc::Smooth, 0.0)
                        }
                        TextSelectionEffect::RevealWipe => (
                            AnimationType::Write {
                                config: Default::default(),
                            },
                            RateFunc::Smooth,
                            0.0,
                        ),
                        TextSelectionEffect::RevealFromBelow => (
                            AnimationType::FadeInFrom {
                                offset: DVec3::new(0.0, -24.0, 0.0),
                            },
                            RateFunc::Smooth,
                            0.0,
                        ),
                        TextSelectionEffect::ColorTo(color) => (
                            AnimationType::FillColorTo { to: *color },
                            RateFunc::Smooth,
                            0.0,
                        ),
                        TextSelectionEffect::OpacityTo(opacity) => (
                            AnimationType::FadeTo { to: *opacity },
                            RateFunc::Smooth,
                            0.0,
                        ),
                        TextSelectionEffect::Focus
                        | TextSelectionEffect::Cancel
                        | TextSelectionEffect::Brace { .. }
                        | TextSelectionEffect::Annotate { .. } => unreachable!(),
                    };
                    self.play_internal(AnimationBuilder {
                        target,
                        anim_type,
                        duration: anim.duration,
                        rate_func,
                        delay: anim.delay + extra_delay,
                    });
                }
            }
        }
    }

    /// Internal method to resolve and schedule a single animation clip.
    fn play_internal(&mut self, anim: AnimationBuilder) {
        if let AnimationType::Properties(properties) = anim.anim_type.clone() {
            if properties.is_empty() {
                return;
            }

            let visible_paints = self
                .states
                .get(anim.target)
                .map(|state| (state.fill.is_some(), state.stroke.brush.is_some()));
            let mut channels = Vec::new();
            if let Some(translation) = properties.translation {
                channels.push(match translation {
                    crate::anim::PropertyTranslation::To(to) => AnimationType::TranslateTo { to },
                    crate::anim::PropertyTranslation::ToAnchor { to, anchor } => {
                        AnimationType::TranslateAnchorTo { to, anchor }
                    }
                    crate::anim::PropertyTranslation::By(delta) => {
                        AnimationType::TranslateBy { delta }
                    }
                });
            }
            if let Some(rotation) = properties.rotation {
                channels.push(match rotation {
                    crate::anim::PropertyRotation::To(to) => AnimationType::RotateTo { to },
                    crate::anim::PropertyRotation::By2D { radians, pivot } => {
                        AnimationType::RotateBy {
                            angle_radians: radians,
                            pivot,
                        }
                    }
                    crate::anim::PropertyRotation::By3D(delta) => {
                        AnimationType::RotateBy3D { delta }
                    }
                });
            }
            if let Some(scale) = properties.scale {
                channels.push(match scale {
                    crate::anim::PropertyScale::To(to) => AnimationType::ScaleTo { to },
                    crate::anim::PropertyScale::Uniform(factor) => {
                        AnimationType::ScaleUniform { factor }
                    }
                    crate::anim::PropertyScale::By(factor) => AnimationType::ScaleBy3D { factor },
                });
            }
            if let Some(opacity) = properties.opacity {
                channels.push(AnimationType::FadeTo { to: opacity });
            }
            let fill = properties.fill.or_else(|| {
                properties
                    .visible_color
                    .filter(|_| visible_paints.is_some_and(|paints| paints.0))
            });
            if let Some(fill) = fill {
                channels.push(AnimationType::FillColorTo { to: fill });
            }
            // Width is deliberately scheduled before color so a missing stroke
            // starts at width zero before the color lens makes it visible.
            if let Some(width) = properties.stroke_width {
                channels.push(AnimationType::StrokeWidthTo { to: width });
            }
            let stroke_color = properties.stroke_color.or_else(|| {
                properties
                    .visible_color
                    .filter(|_| visible_paints.is_some_and(|paints| paints.1))
            });
            if let Some(stroke_color) = stroke_color {
                channels.push(AnimationType::StrokeColorTo { to: stroke_color });
            }
            if let Some((from, to)) = properties.material {
                channels.push(AnimationType::Material3DTo { from, to });
            }
            if let Some((from, to)) = properties.fill_level {
                channels.push(AnimationType::FillLevelTo { from, to });
            }

            for anim_type in channels {
                self.play_internal(AnimationBuilder {
                    target: anim.target,
                    anim_type,
                    duration: anim.duration,
                    delay: anim.delay,
                    rate_func: anim.rate_func.clone(),
                });
            }
            return;
        }

        // Text and Typst roots carry the aggregate paint state used by the
        // public Drawable API, but their visible geometry lives on glyph
        // children. Resolve paint lenses for every glyph as well so each one
        // interpolates from its own current fill/stroke (including fragment
        // overrides) while the root state remains authoritative for later
        // animations.
        if matches!(
            anim.anim_type,
            AnimationType::FillColorTo { .. }
                | AnimationType::StrokeColorTo { .. }
                | AnimationType::StrokeWidthTo { .. }
        ) {
            let child_ids = self
                .states
                .get(anim.target)
                .map(|state| {
                    state
                        .child_spans
                        .iter()
                        .map(|child| child.id)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            for child in child_ids {
                let mut child_anim = anim.clone();
                child_anim.target = child;
                self.play_internal(child_anim);
            }
        }

        self.current_label = Some(Self::anim_label(&anim.anim_type).to_string());
        let track = self.ensure_track(anim.target);

        if let AnimationType::SurroundingRectRetarget { from, to } = anim.anim_type.clone() {
            let mut resolve = |targets: Vec<crate::anim::BoundsTarget>| {
                targets
                    .into_iter()
                    .flat_map(|target| match target {
                        crate::anim::BoundsTarget::Drawable(id) => vec![id],
                        crate::anim::BoundsTarget::TextSelection {
                            target,
                            fragment,
                            occurrence,
                        } => {
                            self.select_occurrence(MobjectRef { id: target }, &fragment, occurrence)
                                .child_ids
                        }
                    })
                    .collect::<Vec<_>>()
            };
            let from = resolve(from);
            let to = resolve(to);
            self.timeline.add_clip(
                track,
                self.current_time + anim.delay,
                anim.duration,
                ClipPayload::Animation(AnimationSpec {
                    target: anim.target,
                    lens: PropertyLensSpec::SurroundingRectTargets { from, to },
                    rate_func: anim.rate_func,
                    delay: 0.0,
                    label: self.current_label.clone(),
                }),
            );
            return;
        }

        if let AnimationType::TextTransition {
            target,
            copy,
            semantic_pairs,
        } = anim.anim_type.clone()
        {
            let semantic_groups = semantic_pairs
                .into_iter()
                .filter_map(
                    |(source_fragment, source_occurrence, target_fragment, target_occurrence)| {
                        let sources = self
                            .select_occurrence(
                                MobjectRef { id: anim.target },
                                &source_fragment,
                                source_occurrence,
                            )
                            .child_ids;
                        let targets = self
                            .select_occurrence(
                                MobjectRef { id: target },
                                &target_fragment,
                                target_occurrence,
                            )
                            .child_ids;
                        (!sources.is_empty() && !targets.is_empty()).then_some((sources, targets))
                    },
                )
                .collect();
            let cursor = self.current_time;
            self.current_time += anim.delay.max(0.0);
            self.play_equation_transition(
                anim.target,
                target,
                semantic_groups,
                anim.duration,
                if copy {
                    EquationTransitionMode::Copy
                } else {
                    EquationTransitionMode::Replace
                },
                true,
            );
            self.current_time = cursor;
            return;
        }
        if let AnimationType::TextSelectionTransform {
            target,
            source_fragment,
            source_occurrence,
            target_fragment,
            target_occurrence,
            copy,
        } = anim.anim_type.clone()
        {
            let sources = self
                .select_occurrence(
                    MobjectRef { id: anim.target },
                    &source_fragment,
                    source_occurrence,
                )
                .child_ids;
            let targets = self
                .select_occurrence(
                    MobjectRef { id: target },
                    &target_fragment,
                    target_occurrence,
                )
                .child_ids;
            let cursor = self.current_time;
            self.current_time += anim.delay.max(0.0);
            self.play_equation_transition(
                anim.target,
                target,
                vec![(sources, targets)],
                anim.duration,
                if copy {
                    EquationTransitionMode::Copy
                } else {
                    EquationTransitionMode::Replace
                },
                false,
            );
            self.current_time = cursor;
            return;
        }
        if let AnimationType::TextSelection {
            fragment,
            occurrence,
            effect,
        } = anim.anim_type.clone()
        {
            self.play_text_selection_internal(anim, fragment, occurrence, effect);
            return;
        }
        if let AnimationType::TextSelectionProperties {
            fragment,
            occurrence,
            properties,
        } = anim.anim_type.clone()
        {
            let selected = self
                .select_occurrence(MobjectRef { id: anim.target }, &fragment, occurrence)
                .child_ids;
            for target in selected {
                self.play_internal(AnimationBuilder {
                    target,
                    anim_type: AnimationType::Properties(properties.clone()),
                    duration: anim.duration,
                    delay: anim.delay,
                    rate_func: anim.rate_func.clone(),
                });
            }
            return;
        }

        if let AnimationType::GltfAnimation {
            animation_index,
            source_duration,
            speed,
            looped,
            reverse,
            transition,
            start_time,
        } = &anim.anim_type
        {
            self.timeline.add_clip(
                track,
                self.current_time + anim.delay,
                anim.duration,
                ClipPayload::GltfAnimation(GltfAnimationSpec {
                    target: anim.target,
                    animation_index: *animation_index,
                    source_duration: *source_duration,
                    speed: *speed,
                    looped: *looped,
                    reverse: *reverse,
                    transition: *transition,
                    start_time: *start_time,
                }),
            );
            return;
        }

        // The Write/Create/Uncreate/Unwrite/SpinIn/Indicate animations expand into
        // multiple staggered or parallel sub-clips, so they have their own branches
        // that access the timeline multiple times. All other variants collapse to a
        // single clip below.
        if matches!(
            anim.anim_type,
            AnimationType::Write { .. }
                | AnimationType::Create { .. }
                | AnimationType::Unwrite { .. }
                | AnimationType::Uncreate { .. }
                | AnimationType::DrawBorderThenFill { .. }
        ) {
            self.play_draw_animation_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::SpinInFromNothing) {
            self.play_spin_in_from_nothing_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::FadeInFrom { .. }) {
            self.play_fade_in_from_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::Create3D) {
            self.play_create_3d_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::Indicate { .. }) {
            self.play_indicate_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::FadeTransform { .. }) {
            self.play_fade_transform_internal(anim, track);
            return;
        }
        if matches!(
            anim.anim_type,
            AnimationType::Transform { .. } | AnimationType::ReplacementTransform { .. }
        ) {
            self.play_transform_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::Wiggle) {
            self.play_wiggle_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::GrowFromPoint { .. }) {
            self.play_grow_from_point_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::GrowFromEdge { .. }) {
            self.play_grow_from_edge_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::Flash { .. }) {
            self.play_flash_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::Circumscribe { .. }) {
            self.play_circumscribe_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::MoveAlongPath { .. }) {
            self.play_move_along_path_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::MoveAlongPath3D { .. }) {
            self.play_move_along_path_3d_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::RotateBy { .. }) {
            self.play_rotate_by_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::GrowArrow) {
            self.play_grow_arrow_internal(anim, track);
            return;
        }

        let anchor_point_target = match anim.anim_type {
            AnimationType::TranslateToAnchorPoint { point } => {
                let Some(reference_state) = self.states.get(point.object) else {
                    bevy::prelude::warn!(
                        "Anchor-point animation skipped: missing reference object {:?}",
                        point.object
                    );
                    return;
                };
                let reference_transform = self.get_world_transform(point.object);
                let local = reference_state.bounds.center()
                    + reference_state.bounds.size() * 0.5 * point.normalized
                    + point.offset;
                Some(reference_transform.to_mat4().transform_point3(local))
            }
            _ => None,
        };

        let state = match self.states.get_mut(anim.target) {
            Some(s) => s,
            None => {
                bevy::prelude::warn!(
                    "Attempted to animate unregistered Mobject: {:?}",
                    anim.target
                );
                return;
            }
        };

        // Resolve lens and update our tracked local hot state
        let lens_spec = match anim.anim_type {
            AnimationType::TranslateTo { to } => {
                let from = state.transform.translation;
                state.transform.translation = to;
                PropertyLensSpec::Translation { from, to }
            }
            AnimationType::TranslateAnchorTo { to, anchor } => {
                let from = state.transform.translation;
                let to = gaanim_layout::compute_move_to(state.bounds, &state.transform, to, anchor)
                    .translation;
                state.transform.translation = to;
                PropertyLensSpec::Translation { from, to }
            }
            AnimationType::TranslateToAnchorPoint { .. } => {
                let from = state.transform.translation;
                let target = anchor_point_target
                    .expect("anchor-point target is precomputed for this animation variant");
                let to = gaanim_layout::compute_move_to(
                    state.bounds,
                    &state.transform,
                    target,
                    Anchor::Center,
                )
                .translation;
                state.transform.translation = to;
                PropertyLensSpec::Translation { from, to }
            }
            AnimationType::TranslateBy { delta } => {
                let from = state.transform.translation;
                let to = from + delta;
                state.transform.translation = to;
                PropertyLensSpec::Translation { from, to }
            }
            AnimationType::RotateTo { to } => {
                let from = state.transform.rotation;
                state.transform.rotation = to;
                PropertyLensSpec::Rotation { from, to }
            }
            AnimationType::RotateBy { .. } => {
                unreachable!("Expansion is dispatched in the early branch above")
            }
            AnimationType::RotateBy3D { delta } => {
                let from = state.transform.rotation;
                let to = (from * delta).normalize();
                state.transform.rotation = to;
                PropertyLensSpec::Rotation { from, to }
            }
            AnimationType::ScaleTo { to } => {
                let from = state.transform.scale;
                state.transform.scale = to;
                PropertyLensSpec::Scale { from, to }
            }
            AnimationType::ScaleUniform { factor } => {
                let from = state.transform.scale;
                let to = from * factor;
                state.transform.scale = to;
                PropertyLensSpec::Scale { from, to }
            }
            AnimationType::ScaleBy3D { factor } => {
                let from = state.transform.scale;
                let to = from * factor;
                state.transform.scale = to;
                PropertyLensSpec::Scale { from, to }
            }
            AnimationType::FadeTo { to } => {
                let from = state.opacity;
                state.opacity = to;
                PropertyLensSpec::Opacity { from, to }
            }
            AnimationType::FadeIn => {
                let from = 0.0;
                let to = 1.0;
                state.opacity = 1.0;
                self.commands.entity(state.entity).insert(Opacity(from));
                PropertyLensSpec::Opacity { from, to }
            }
            AnimationType::FadeInFrom { .. } => {
                unreachable!("FadeInFrom expands into fade and translation clips")
            }
            AnimationType::FadeOut => {
                let from = state.opacity;
                let to = 0.0;
                state.opacity = 0.0;
                PropertyLensSpec::Opacity { from, to }
            }
            AnimationType::FillColorTo { to } => {
                let from = match &state.fill {
                    Some(Brush::Solid(c)) => *c,
                    _ => {
                        let rgba = to.to_rgba8();
                        Color::from_rgba8(rgba.r, rgba.g, rgba.b, 0)
                    }
                };
                state.fill = Some(Brush::Solid(to));
                PropertyLensSpec::FillColor { from, to }
            }
            AnimationType::StrokeColorTo { to } => {
                let from = match &state.stroke.brush {
                    Some(Brush::Solid(c)) => *c,
                    _ => {
                        let rgba = to.to_rgba8();
                        Color::from_rgba8(rgba.r, rgba.g, rgba.b, 0)
                    }
                };
                state.stroke.brush = Some(Brush::Solid(to));
                PropertyLensSpec::StrokeColor { from, to }
            }
            AnimationType::StrokeWidthTo { to } => {
                let from = if state.stroke.brush.is_some() {
                    state.stroke.style.width
                } else {
                    0.0
                };
                state.stroke.style.width = to;
                PropertyLensSpec::StrokeWidth { from, to }
            }
            AnimationType::Material3DTo { from, to } => PropertyLensSpec::Material3D { from, to },
            AnimationType::FillLevelTo { from, to } => PropertyLensSpec::FillLevel { from, to },
            AnimationType::SurroundingRectRetarget { .. } => {
                unreachable!(
                    "surrounding-rectangle retargeting is dispatched before lens resolution"
                )
            }
            AnimationType::Properties(_) => {
                unreachable!("property animations expand before lens resolution")
            }
            AnimationType::GrowFromCenter => {
                let to = state.transform.scale;
                let from = gaanim_core::glam::DVec3::ZERO;
                // Pre-set the scale to 0.0 right now via deferred commands to avoid flickers
                let mut temp_transform = state.transform;
                temp_transform.scale = from;
                self.commands.entity(state.entity).insert(temp_transform);
                PropertyLensSpec::Scale { from, to }
            }
            AnimationType::ShrinkToCenter => {
                let from = state.transform.scale;
                let to = gaanim_core::glam::DVec3::ZERO;
                state.transform.scale = to;
                PropertyLensSpec::Scale { from, to }
            }
            AnimationType::CameraState { .. }
            | AnimationType::CameraPosition { .. }
            | AnimationType::CameraPositionSource { .. }
            | AnimationType::CameraZoom { .. }
            | AnimationType::CameraZoomSource { .. }
            | AnimationType::CameraRotation { .. }
            | AnimationType::CameraRotationSource { .. }
            | AnimationType::CameraFrame { .. }
            | AnimationType::CameraFrameMany { .. }
            | AnimationType::CameraFollow { .. }
            | AnimationType::CameraFollowEndpoint { .. }
            | AnimationType::CameraShake { .. }
            | AnimationType::CameraLookAt { .. }
            | AnimationType::CameraLookAtSource { .. }
            | AnimationType::CameraOrbit { .. }
            | AnimationType::CameraPerspective { .. }
            | AnimationType::CameraOrthographic { .. }
            | AnimationType::CameraReset
            | AnimationType::CameraDolly { .. }
            | AnimationType::GltfAnimation { .. }
            | AnimationType::Write { .. }
            | AnimationType::Create { .. }
            | AnimationType::Create3D
            | AnimationType::Unwrite { .. }
            | AnimationType::Uncreate { .. }
            | AnimationType::SpinInFromNothing
            | AnimationType::Indicate { .. }
            | AnimationType::TextTransition { .. }
            | AnimationType::TextSelection { .. }
            | AnimationType::TextSelectionProperties { .. }
            | AnimationType::TextSelectionTransform { .. }
            | AnimationType::FadeTransform { .. }
            | AnimationType::Wiggle
            | AnimationType::GrowFromPoint { .. }
            | AnimationType::GrowFromEdge { .. }
            | AnimationType::DrawBorderThenFill { .. }
            | AnimationType::Flash { .. }
            | AnimationType::Circumscribe { .. }
            | AnimationType::MoveAlongPath { .. }
            | AnimationType::MoveAlongPath3D { .. }
            | AnimationType::Transform { .. }
            | AnimationType::ReplacementTransform { .. }
            | AnimationType::GrowArrow => {
                unreachable!("Expansion is dispatched in the early branch above")
            }
            AnimationType::SignalFloat { to } => {
                let from = *self.float_signals.get(&anim.target).unwrap_or(&0.0);
                self.float_signals.insert(anim.target, to);
                PropertyLensSpec::SignalFloat { from, to }
            }
            AnimationType::ShowPassingFlash { time_width } => PropertyLensSpec::PathRange {
                from: 0.0,
                to: 1.0 + time_width,
                time_width,
            },
        };

        // Add the resolved clip to the Timeline resource.
        // The delay offsets the clip start within the segment but does not
        // advance the cursor (cursor tracks segment duration, not delay).
        let clip_start = self.current_time + anim.delay;
        self.timeline.add_clip(
            track,
            clip_start,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: lens_spec,
                rate_func: anim.rate_func,
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );
    }

    /// Internal: materialize a `Write` animation as one or more staggered
    /// sub-clip pairs. If the target has children (text/equation glyphs,
    /// group members), each child draws in sequence. If the target is a
    /// leaf (e.g. a single `circle`/`line`), one pair is scheduled on the
    /// target itself.
    ///
    /// Each item receives two clips:
    /// 1. `PathCompletion 0.0 -> 1.0` over the first `DRAW_RATIO` of
    ///    `item_duration` — progressively reveals the outline.
    /// 2. `FillDrawProgress 0.0 -> 1.0` over the remaining
    ///    `1 - DRAW_RATIO` of `item_duration`, starting right after the
    ///    path draw completes — cross-fades the fill in once the outline
    ///    is fully drawn.
    ///
    /// To prevent the "object fully visible from the start" bug, we also
    /// `insert(FillDrawProgress(0.0))` on every target entity right here
    /// (via the deferred command queue). By the time the first render
    /// frame runs, the fill alpha multiplier is `0.0` and the renderer
    /// will render an empty/invisible fill, so the user only ever sees
    /// Internal: generalize a draw/erase animation (Write, Create, Unwrite, Uncreate)
    /// as one or more staggered or parallel sub-clip sequences.
    fn draw_schedule_for(
        &self,
        anim: &AnimationBuilder,
        item_count: usize,
    ) -> Option<DrawSchedule> {
        let adaptive_lag = adaptive_lag_ratio(item_count);
        match &anim.anim_type {
            AnimationType::Write { config } => Some(DrawSchedule {
                mode: DrawMode::BorderThenFill,
                reversed: false,
                staggered: true,
                lag_ratio: config.lag_ratio.unwrap_or(adaptive_lag),
                stroke_width: config.stroke_width,
                auto_stroke_width: 1.0,
                fill_rate_func: gaanim_math::RateFunc::EaseOut(EasingCurve::Quadratic),
            }),
            AnimationType::Create { config } => Some(DrawSchedule {
                mode: DrawMode::Grow,
                reversed: false,
                staggered: true,
                lag_ratio: config.lag_ratio.unwrap_or(1.0),
                stroke_width: config.stroke_width,
                auto_stroke_width: 1.0,
                fill_rate_func: gaanim_math::RateFunc::EaseOut(EasingCurve::Quadratic),
            }),
            AnimationType::Unwrite { config } => Some(DrawSchedule {
                mode: DrawMode::BorderThenFill,
                reversed: true,
                staggered: true,
                lag_ratio: config.lag_ratio.unwrap_or(adaptive_lag),
                stroke_width: config.stroke_width,
                auto_stroke_width: 1.0,
                fill_rate_func: gaanim_math::RateFunc::EaseOut(EasingCurve::Quadratic),
            }),
            AnimationType::Uncreate { config } => Some(DrawSchedule {
                mode: DrawMode::Grow,
                reversed: true,
                staggered: true,
                lag_ratio: config.lag_ratio.unwrap_or(1.0),
                stroke_width: config.stroke_width,
                auto_stroke_width: 1.0,
                fill_rate_func: gaanim_math::RateFunc::EaseOut(EasingCurve::Quadratic),
            }),
            AnimationType::DrawBorderThenFill { config } => Some(DrawSchedule {
                mode: DrawMode::BorderThenFill,
                reversed: false,
                staggered: true,
                lag_ratio: config.lag_ratio.unwrap_or(adaptive_lag),
                stroke_width: config.stroke_width,
                auto_stroke_width: 2.0,
                fill_rate_func: gaanim_math::RateFunc::EaseOut(EasingCurve::Quadratic),
            }),
            _ => None,
        }
    }

    fn play_draw_animation_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        const DRAW_RATIO: f64 = 0.5;
        let start_time = self.current_time + anim.delay.max(0.0);

        let mut items: Vec<ObjectId> = {
            let state = match self.states.get(anim.target) {
                Some(s) => s,
                None => {
                    bevy::prelude::warn!(
                        "Attempted to animate unregistered Mobject: {:?}",
                        anim.target
                    );
                    return;
                }
            };
            // For text/equation hierarchies use child_spans, for groups
            // like axes use children. Collect visual leaves so a single
            // `axes.create()` animates grid→axes→ticks→numbers sequentially
            // instead of trying to trim the group's empty Path2D.
            if !state.child_spans.is_empty() {
                let mut children: Vec<ObjectId> =
                    state.child_spans.iter().map(|child| child.id).collect();
                children.sort_by(|a, b| {
                    let xa = self
                        .states
                        .get(*a)
                        .map(|s| s.transform.translation.x)
                        .unwrap_or(0.0);
                    let xb = self
                        .states
                        .get(*b)
                        .map(|s| s.transform.translation.x)
                        .unwrap_or(0.0);
                    xa.partial_cmp(&xb).unwrap_or(std::cmp::Ordering::Equal)
                });
                children
            } else if !state.children.is_empty() {
                // Collect visual leaves for groups (e.g. axes). This flattens
                // the 5-layer axes (grid, axes, ticks) plus any text glyph
                // leaves, preserving the creation order (grid first).
                let mut leaves = Vec::new();
                let mut stack = state.children.clone();
                let mut visited = std::collections::HashSet::new();
                while let Some(id) = stack.pop() {
                    if !visited.insert(id) {
                        continue;
                    }
                    let Some(child_state) = self.states.get(id) else {
                        continue;
                    };
                    if child_state.exclude_from_parent_draw {
                        continue;
                    }
                    // If this child itself has glyph spans, expand to glyphs
                    if !child_state.child_spans.is_empty() {
                        leaves.extend(child_state.child_spans.iter().map(|c| c.id));
                    } else if !child_state.children.is_empty() {
                        // Nested group – push its children to stack
                        stack.extend(child_state.children.iter().cloned());
                    } else {
                        leaves.push(id);
                    }
                }
                // Keep creation order: reverse because stack is LIFO and we
                // want grid→axes→ticks first as pushed in styled_axes.
                leaves.reverse();
                if leaves.is_empty() {
                    vec![anim.target]
                } else {
                    leaves
                }
            } else {
                vec![anim.target]
            }
        };

        let n = items.len();
        if n == 0 {
            return;
        }

        let Some(schedule) = self.draw_schedule_for(&anim, n) else {
            return;
        };

        if schedule.staggered && schedule.reversed {
            items.reverse();
        }

        let mut temporary_strokes = HashMap::new();
        for item_id in &items {
            if let Some(state) = self.states.get(*item_id)
                && state.stroke.brush.is_none()
            {
                let color = state
                    .fill
                    .as_ref()
                    .and_then(extract_brush_color)
                    .unwrap_or(Color::WHITE);
                let width = schedule.stroke_width.unwrap_or(schedule.auto_stroke_width);
                let new_stroke = StrokeBrush::new(color, width);
                self.commands.entity(state.entity).insert(new_stroke);
                temporary_strokes.insert(*item_id, width);
            }
        }

        for item_id in &items {
            if let Some(state) = self.states.get(*item_id) {
                if matches!(schedule.mode, DrawMode::BorderThenFill) {
                    let initial_fill = if schedule.reversed { 1.0 } else { 0.0 };
                    self.commands
                        .entity(state.entity)
                        .insert(gaanim_animation::FillDrawProgress(initial_fill));
                }

                // Insert pen-tip glow for forward draw animations.
                if !schedule.reversed {
                    self.commands
                        .entity(state.entity)
                        .insert(gaanim_animation::WriteTipGlow::default());
                }

                let initial_reveal = if schedule.reversed { 1.0 } else { 0.0 };
                self.commands
                    .entity(state.entity)
                    .insert(gaanim_animation::PathReveal(initial_reveal));

                if !schedule.reversed {
                    self.commands
                        .entity(state.entity)
                        .insert(gaanim_scene::components::Path2D(std::sync::Arc::new(
                            gaanim_core::kurbo::BezPath::new(),
                        )));
                }
            }
        }

        let lag_ratio = if schedule.staggered {
            schedule.lag_ratio
        } else {
            0.0
        };
        let item_duration = if schedule.staggered {
            anim.duration / (1.0 + (n as f64 - 1.0) * lag_ratio)
        } else {
            anim.duration
        };
        let lag_step = item_duration * lag_ratio;
        let min_step = 1e-6_f64.max(item_duration * 0.01);
        let draw_duration = (item_duration * DRAW_RATIO).max(min_step);
        let fill_duration = (item_duration * (1.0 - DRAW_RATIO)).max(min_step);

        for item_id in &items {
            self.timeline.add_clip(
                parent_track,
                start_time,
                min_step,
                ClipPayload::Animation(AnimationSpec {
                    target: *item_id,
                    lens: PropertyLensSpec::PathCompletion {
                        from: if schedule.reversed { 1.0 } else { 0.0 },
                        to: if schedule.reversed { 1.0 } else { 0.0 },
                    },
                    rate_func: anim.rate_func.clone(),
                    delay: 0.0,
                    label: self.current_label.clone(),
                }),
            );

            if matches!(schedule.mode, DrawMode::BorderThenFill) {
                self.timeline.add_clip(
                    parent_track,
                    start_time,
                    min_step,
                    ClipPayload::Animation(AnimationSpec {
                        target: *item_id,
                        lens: PropertyLensSpec::FillDrawProgress {
                            from: if schedule.reversed { 1.0 } else { 0.0 },
                            to: if schedule.reversed { 1.0 } else { 0.0 },
                        },
                        rate_func: anim.rate_func.clone(),
                        delay: 0.0,
                        label: self.current_label.clone(),
                    }),
                );
            }
        }

        for (i, item_id) in items.iter().enumerate() {
            let item_start = start_time + i as f64 * lag_step;

            match (schedule.mode, schedule.reversed) {
                (DrawMode::Grow, false) => {
                    self.timeline.add_clip(
                        parent_track,
                        item_start,
                        item_duration.max(min_step),
                        ClipPayload::Animation(AnimationSpec {
                            target: *item_id,
                            lens: PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 },
                            rate_func: anim.rate_func.clone(),
                            delay: 0.0,
                            label: self.current_label.clone(),
                        }),
                    );
                }
                (DrawMode::Grow, true) => {
                    self.timeline.add_clip(
                        parent_track,
                        item_start,
                        item_duration.max(min_step),
                        ClipPayload::Animation(AnimationSpec {
                            target: *item_id,
                            lens: PropertyLensSpec::PathCompletion { from: 1.0, to: 0.0 },
                            rate_func: anim.rate_func.clone(),
                            delay: 0.0,
                            label: self.current_label.clone(),
                        }),
                    );
                }
                (DrawMode::BorderThenFill, false) => {
                    let fill_start = item_start + draw_duration;
                    self.timeline.add_clip(
                        parent_track,
                        item_start,
                        draw_duration,
                        ClipPayload::Animation(AnimationSpec {
                            target: *item_id,
                            lens: PropertyLensSpec::FillDrawProgress { from: 0.0, to: 0.0 },
                            rate_func: anim.rate_func.clone(),
                            delay: 0.0,
                            label: self.current_label.clone(),
                        }),
                    );
                    self.timeline.add_clip(
                        parent_track,
                        item_start,
                        draw_duration,
                        ClipPayload::Animation(AnimationSpec {
                            target: *item_id,
                            lens: PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 },
                            rate_func: anim.rate_func.clone(),
                            delay: 0.0,
                            label: self.current_label.clone(),
                        }),
                    );
                    self.timeline.add_clip(
                        parent_track,
                        fill_start,
                        fill_duration,
                        ClipPayload::Animation(AnimationSpec {
                            target: *item_id,
                            lens: PropertyLensSpec::FillDrawProgress { from: 0.0, to: 1.0 },
                            rate_func: schedule.fill_rate_func.clone(),
                            delay: 0.0,
                            label: self.current_label.clone(),
                        }),
                    );
                }
                (DrawMode::BorderThenFill, true) => {
                    let draw_start = item_start + fill_duration;
                    self.timeline.add_clip(
                        parent_track,
                        item_start,
                        fill_duration,
                        ClipPayload::Animation(AnimationSpec {
                            target: *item_id,
                            lens: PropertyLensSpec::FillDrawProgress { from: 1.0, to: 0.0 },
                            rate_func: schedule.fill_rate_func.clone(),
                            delay: 0.0,
                            label: self.current_label.clone(),
                        }),
                    );
                    self.timeline.add_clip(
                        parent_track,
                        item_start,
                        fill_duration,
                        ClipPayload::Animation(AnimationSpec {
                            target: *item_id,
                            lens: PropertyLensSpec::PathCompletion { from: 1.0, to: 1.0 },
                            rate_func: anim.rate_func.clone(),
                            delay: 0.0,
                            label: self.current_label.clone(),
                        }),
                    );
                    self.timeline.add_clip(
                        parent_track,
                        draw_start,
                        draw_duration,
                        ClipPayload::Animation(AnimationSpec {
                            target: *item_id,
                            lens: PropertyLensSpec::PathCompletion { from: 1.0, to: 0.0 },
                            rate_func: anim.rate_func.clone(),
                            delay: 0.0,
                            label: self.current_label.clone(),
                        }),
                    );
                    self.timeline.add_clip(
                        parent_track,
                        draw_start,
                        draw_duration,
                        ClipPayload::Animation(AnimationSpec {
                            target: *item_id,
                            lens: PropertyLensSpec::FillDrawProgress { from: 0.0, to: 0.0 },
                            rate_func: anim.rate_func.clone(),
                            delay: 0.0,
                            label: self.current_label.clone(),
                        }),
                    );
                }
            }

            // The outline synthesized by Write/DrawBorderThenFill is a
            // temporary drawing aid, not part of the object's final style.
            // Fade its width while the fill appears so it cannot leave a
            // bright halo or blink on later seeks/transforms.
            if matches!(schedule.mode, DrawMode::BorderThenFill)
                && !schedule.reversed
                && let Some(&width) = temporary_strokes.get(item_id)
            {
                self.timeline.add_clip(
                    parent_track,
                    item_start + draw_duration,
                    fill_duration,
                    ClipPayload::Animation(AnimationSpec {
                        target: *item_id,
                        lens: PropertyLensSpec::StrokeWidth {
                            from: width,
                            to: 0.0,
                        },
                        rate_func: schedule.fill_rate_func.clone(),
                        delay: 0.0,
                        label: None,
                    }),
                );
            }

            if let Some(width) = schedule.stroke_width {
                self.timeline.add_clip(
                    parent_track,
                    item_start,
                    min_step,
                    ClipPayload::Animation(AnimationSpec {
                        target: *item_id,
                        lens: PropertyLensSpec::StrokeWidth {
                            from: width,
                            to: width,
                        },
                        rate_func: anim.rate_func.clone(),
                        delay: 0.0,
                        label: self.current_label.clone(),
                    }),
                );
            }
        }
    }

    /// Internal: materialize `SpinInFromNothing` as a simultaneous scale-up and 360-degree rotation.
    fn play_fade_in_from_internal(&mut self, anim: AnimationBuilder, _track: TrackId) {
        let offset = match anim.anim_type {
            AnimationType::FadeInFrom { offset } => offset,
            _ => return,
        };
        let Some(state) = self.states.get_mut(anim.target) else {
            return;
        };
        let final_position = state.transform.translation;
        state.transform.translation = final_position + offset;
        self.commands.entity(state.entity).insert(state.transform);

        // Reuse the well-tested opacity and translation paths. Both are
        // scheduled at the same cursor, so they run in parallel.
        self.play_internal(AnimationBuilder {
            target: anim.target,
            anim_type: AnimationType::FadeIn,
            duration: anim.duration,
            rate_func: anim.rate_func.clone(),
            delay: anim.delay,
        });
        self.play_internal(AnimationBuilder {
            target: anim.target,
            anim_type: AnimationType::TranslateTo { to: final_position },
            duration: anim.duration,
            rate_func: anim.rate_func,
            delay: anim.delay,
        });
    }

    fn play_create_3d_internal(&mut self, anim: AnimationBuilder, _track: TrackId) {
        self.play_internal(AnimationBuilder {
            target: anim.target,
            anim_type: AnimationType::FadeIn,
            duration: anim.duration,
            rate_func: anim.rate_func.clone(),
            delay: anim.delay,
        });
        self.play_internal(AnimationBuilder {
            target: anim.target,
            anim_type: AnimationType::GrowFromCenter,
            duration: anim.duration,
            rate_func: anim.rate_func,
            delay: anim.delay,
        });
    }

    fn play_spin_in_from_nothing_internal(
        &mut self,
        anim: AnimationBuilder,
        parent_track: TrackId,
    ) {
        let state = match self.states.get_mut(anim.target) {
            Some(s) => s,
            None => {
                bevy::prelude::warn!(
                    "Attempted to SpinInFromNothing unregistered Mobject: {:?}",
                    anim.target
                );
                return;
            }
        };

        let initial_scale = state.transform.scale;
        let initial_rotation = state.transform.rotation;

        // To avoid quaternion SLERP shortest-path 0-rotation logic issues for 360 degrees,
        // we split the rotation into two consecutive 180-degree clips (PI radians each).
        let mid_rotation =
            initial_rotation * gaanim_core::glam::DQuat::from_rotation_z(std::f64::consts::PI);
        let end_rotation = initial_rotation
            * gaanim_core::glam::DQuat::from_rotation_z(2.0 * std::f64::consts::PI);

        // Pre-set the scale to 0.0 right now via deferred commands to avoid first-frame flickers
        let mut temp_transform = state.transform;
        temp_transform.scale = gaanim_core::glam::DVec3::ZERO;
        self.commands.entity(state.entity).insert(temp_transform);

        // Update the final expected state at the end of scheduling
        state.transform.rotation = end_rotation;

        // 1. Unified scale clip (0.0 -> target_scale) over the full duration
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from: gaanim_core::glam::DVec3::ZERO,
                    to: initial_scale,
                },
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );

        // 2. Rotation part 1 (0 -> 180 deg) over first half
        let half_duration = anim.duration * 0.5;
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            half_duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Rotation {
                    from: initial_rotation,
                    to: mid_rotation,
                },
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );

        // 3. Rotation part 2 (180 -> 360 deg) over second half
        self.timeline.add_clip(
            parent_track,
            self.current_time + half_duration,
            half_duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Rotation {
                    from: mid_rotation,
                    to: end_rotation,
                },
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );
    }

    /// Internal: materialize `Indicate` as a temporary scale-up and color highlight.
    ///
    /// Indicate is symmetric: the object grows to `scale_factor`, highlights, then
    /// shrinks back to its original scale/color over the second half of the duration.
    /// We split it into two consecutive clips so the final state matches the initial
    /// state and subsequent animations start from the correct baseline.
    fn play_indicate_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let (highlight_color, scale_factor) = match anim.anim_type {
            AnimationType::Indicate {
                color,
                scale_factor,
            } => (color, scale_factor),
            _ => unreachable!(),
        };

        // Collect target ids: root target plus all child spans for coloring.
        let items: Vec<ObjectId> = {
            let state = match self.states.get(anim.target) {
                Some(s) => s,
                None => {
                    bevy::prelude::warn!(
                        "Attempted to Indicate unregistered Mobject: {:?}",
                        anim.target
                    );
                    return;
                }
            };
            if state.child_spans.is_empty() {
                vec![anim.target]
            } else {
                state.child_spans.iter().map(|child| child.id).collect()
            }
        };

        let half = anim.duration * 0.5;

        // 1. Hop upward and pulse the target around its visual center.
        let root_state = match self.states.get_mut(anim.target) {
            Some(s) => s,
            None => return,
        };
        // A glyph's bounds may already be shifted into its parent's coordinate
        // system, while its path remains in local coordinates. Use the path's
        // actual visual center as the pivot; using `state.bounds.center()` here
        // creates the diagonal down-right drift seen in equation selections.
        if root_state.transform.anchor == DVec3::ZERO {
            let pivot = if root_state.path.elements().is_empty() {
                root_state.bounds.center()
            } else {
                let bounds = root_state.path.bounding_box();
                DVec3::new(
                    (bounds.x0 + bounds.x1) * 0.5,
                    (bounds.y0 + bounds.y1) * 0.5,
                    0.0,
                )
            };
            root_state.transform.anchor = pivot;
            self.commands
                .entity(root_state.entity)
                .insert(root_state.transform);
        }
        let scale_from = root_state.transform.scale;
        let scale_to = scale_from * scale_factor;
        let translation_from = root_state.transform.translation;
        let jump_height = (root_state.bounds.height().abs() * 0.1).clamp(4.0, 10.0);
        let translation_peak = translation_from + DVec3::new(0.0, jump_height, 0.0);
        let ease_up = gaanim_math::RateFunc::EaseOut(EasingCurve::Quadratic);
        let ease_down = gaanim_math::RateFunc::EaseIn(EasingCurve::Quadratic);

        self.timeline.add_clip(
            parent_track,
            self.current_time,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Translation {
                    from: translation_from,
                    to: translation_peak,
                },
                rate_func: ease_up.clone(),
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );
        self.timeline.add_clip(
            parent_track,
            self.current_time + half,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Translation {
                    from: translation_peak,
                    to: translation_from,
                },
                rate_func: ease_down.clone(),
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );

        self.timeline.add_clip(
            parent_track,
            self.current_time,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from: scale_from,
                    to: scale_to,
                },
                rate_func: ease_up,
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );
        self.timeline.add_clip(
            parent_track,
            self.current_time + half,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from: scale_to,
                    to: scale_from,
                },
                rate_func: ease_down,
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );

        // 2. Color highlight on children (if requested) — highlight then revert
        if let Some(color) = highlight_color {
            for item_id in &items {
                if let Some(state) = self.states.get(*item_id) {
                    if let Some(Brush::Solid(c)) = &state.fill {
                        self.timeline.add_clip(
                            parent_track,
                            self.current_time,
                            half,
                            ClipPayload::Animation(AnimationSpec {
                                target: *item_id,
                                lens: PropertyLensSpec::FillColor {
                                    from: *c,
                                    to: color,
                                },
                                rate_func: gaanim_math::RateFunc::Linear,
                                delay: 0.0,
                                label: self.current_label.clone(),
                            }),
                        );
                        self.timeline.add_clip(
                            parent_track,
                            self.current_time + half,
                            half,
                            ClipPayload::Animation(AnimationSpec {
                                target: *item_id,
                                lens: PropertyLensSpec::FillColor {
                                    from: color,
                                    to: *c,
                                },
                                rate_func: gaanim_math::RateFunc::Linear,
                                delay: 0.0,
                                label: self.current_label.clone(),
                            }),
                        );
                    }
                    if let Some(Brush::Solid(c)) = &state.stroke.brush {
                        self.timeline.add_clip(
                            parent_track,
                            self.current_time,
                            half,
                            ClipPayload::Animation(AnimationSpec {
                                target: *item_id,
                                lens: PropertyLensSpec::StrokeColor {
                                    from: *c,
                                    to: color,
                                },
                                rate_func: gaanim_math::RateFunc::Linear,
                                delay: 0.0,
                                label: self.current_label.clone(),
                            }),
                        );
                        self.timeline.add_clip(
                            parent_track,
                            self.current_time + half,
                            half,
                            ClipPayload::Animation(AnimationSpec {
                                target: *item_id,
                                lens: PropertyLensSpec::StrokeColor {
                                    from: color,
                                    to: *c,
                                },
                                rate_func: gaanim_math::RateFunc::Linear,
                                delay: 0.0,
                                label: self.current_label.clone(),
                            }),
                        );
                    }
                }
            }
        }
    }

    fn play_fade_transform_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let target = match &anim.anim_type {
            AnimationType::FadeTransform { target } => *target,
            _ => return,
        };

        {
            let source_state = match self.states.get(anim.target) {
                Some(s) => s,
                None => return,
            };
            let from = source_state.opacity;
            self.timeline.add_clip(
                parent_track,
                self.current_time,
                anim.duration,
                ClipPayload::Animation(AnimationSpec {
                    target: anim.target,
                    lens: PropertyLensSpec::Opacity { from, to: 0.0 },
                    rate_func: anim.rate_func.clone(),
                    delay: 0.0,
                    label: self.current_label.clone(),
                }),
            );
            if let Some(source_state) = self.states.get_mut(anim.target) {
                source_state.opacity = 0.0;
            }
        }

        {
            let target_state = match self.states.get(target) {
                Some(s) => s,
                None => return,
            };
            let target_opacity = target_state.opacity;
            let target_entity = target_state.entity;
            self.timeline.add_clip(
                parent_track,
                self.current_time,
                anim.duration,
                ClipPayload::Animation(AnimationSpec {
                    target,
                    lens: PropertyLensSpec::Opacity {
                        from: 0.0,
                        to: target_opacity,
                    },
                    rate_func: anim.rate_func.clone(),
                    delay: 0.0,
                    label: self.current_label.clone(),
                }),
            );
            if let Some(target_state) = self.states.get_mut(target) {
                target_state.opacity = target_opacity;
            }
            self.commands.entity(target_entity).insert(Opacity(0.0));
        }
    }

    fn play_transform_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let (target, is_replacement) = match &anim.anim_type {
            AnimationType::Transform { target } => (*target, false),
            AnimationType::ReplacementTransform { target } => (*target, true),
            _ => return,
        };

        let source_state = match self.states.get(anim.target) {
            Some(s) => s.clone(),
            None => return,
        };

        let target_state = match self.states.get(target) {
            Some(s) => s.clone(),
            None => return,
        };
        let target_visual_opacity = target_state.opacity;
        let source_has_children =
            !source_state.child_spans.is_empty() || !source_state.children.is_empty();
        let target_has_children =
            !target_state.child_spans.is_empty() || !target_state.children.is_empty();
        let morph_end_time = self.current_time + anim.duration;

        // Carry the source into the current scene at the scene boundary. A
        // deferred ECS command would overwrite SceneMember in the t=0 snapshot
        // and hide it from its original segment immediately; waiting until the
        // morph itself would instead make it blink back after the transition.
        if let Some(scene_id) = self.current_scene
            && !self.is_persistent(anim.target)
            && !self.has_managed_membership(anim.target)
        {
            let membership_time = self
                .timeline
                .scene_index
                .iter()
                .find_map(|(time, id)| (*id == scene_id).then_some(time.0))
                .unwrap_or(self.current_time);
            self.timeline.add_clip(
                parent_track,
                membership_time,
                0.0,
                ClipPayload::SetSceneMember {
                    target: anim.target,
                    scene: Some(scene_id),
                },
            );
            for child in &source_state.child_spans {
                self.timeline.add_clip(
                    parent_track,
                    membership_time,
                    0.0,
                    ClipPayload::SetSceneMember {
                        target: child.id,
                        scene: Some(scene_id),
                    },
                );
            }
        }

        // Treat the target as a state template: it should stay hidden while
        // the source morphs toward its final geometry and styling.
        self.hide_visuals_now(&target_state);
        for descendant in self.hierarchy_ids(target) {
            if let Some(state) = self.states.get(descendant) {
                self.commands.entity(state.entity).insert(Opacity(0.0));
            }
        }

        // Text and math are rendered as child hierarchies. During a transform,
        // use the source root as a temporary flattened vector proxy. Scheduling
        // this swap on the timeline (instead of changing the initial ECS state)
        // preserves earlier Write/Create animations on the source children.
        if source_has_children {
            for child in self
                .hierarchy_ids(anim.target)
                .into_iter()
                .filter(|child| *child != anim.target)
            {
                let child_opacity = self
                    .states
                    .get(child)
                    .map(|state| state.opacity)
                    .unwrap_or(1.0);
                self.timeline.add_clip(
                    parent_track,
                    self.current_time,
                    0.0,
                    ClipPayload::Animation(AnimationSpec {
                        target: child,
                        lens: PropertyLensSpec::Opacity {
                            from: child_opacity,
                            to: 0.0,
                        },
                        rate_func: gaanim_math::RateFunc::Linear,
                        delay: 0.0,
                        label: None,
                    }),
                );
            }
        }

        // Zero-duration clip to re-hide the target at the morph start.
        // The seek-based playback restores a keyframe snapshot each frame
        // which resets the target's opacity to its pre-morph value; this
        // clip re-applies opacity 0 immediately after the restore.
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            0.0,
            ClipPayload::Animation(AnimationSpec {
                target,
                lens: PropertyLensSpec::Opacity {
                    from: target_state.opacity,
                    to: 0.0,
                },
                rate_func: gaanim_math::RateFunc::Linear,
                delay: 0.0,
                label: None,
            }),
        );
        for child in self
            .hierarchy_ids(target)
            .into_iter()
            .filter(|child| *child != target)
        {
            let child_opacity = self.states.get(child).map(|s| s.opacity).unwrap_or(1.0);
            self.timeline.add_clip(
                parent_track,
                self.current_time,
                0.0,
                ClipPayload::Animation(AnimationSpec {
                    target: child,
                    lens: PropertyLensSpec::Opacity {
                        from: child_opacity,
                        to: 0.0,
                    },
                    rate_func: gaanim_math::RateFunc::Linear,
                    delay: 0.0,
                    label: None,
                }),
            );
        }

        // Morph the path
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::PathMorph {
                    from: (*source_state.path).clone(),
                    to: (*target_state.path).clone(),
                },
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );

        // Morph the translation, rotation, scale
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Translation {
                    from: source_state.transform.translation,
                    to: target_state.transform.translation,
                },
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: None,
            }),
        );
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Rotation {
                    from: source_state.transform.rotation,
                    to: target_state.transform.rotation,
                },
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: None,
            }),
        );
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from: source_state.transform.scale,
                    to: target_state.transform.scale,
                },
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: None,
            }),
        );

        // Morph colors
        let source_fill = match &source_state.fill {
            Some(Brush::Solid(c)) => *c,
            _ => Color::WHITE,
        };
        let target_fill = match &target_state.fill {
            Some(Brush::Solid(c)) => *c,
            _ => Color::WHITE,
        };
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::FillColor {
                    from: source_fill,
                    to: target_fill,
                },
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: None,
            }),
        );

        let source_stroke = source_state
            .stroke
            .brush
            .as_ref()
            .and_then(extract_brush_color);
        let target_stroke = target_state
            .stroke
            .brush
            .as_ref()
            .and_then(extract_brush_color);

        // `StrokeColor` materializes a solid brush. Never schedule it for a
        // no-stroke → no-stroke morph, otherwise text proxies acquire the
        // white one-pixel halo that used to flash around the final word.
        let stroke_colors = match (source_stroke, target_stroke) {
            (Some(from), Some(to)) => Some((from, to)),
            (Some(from), None) => Some((from, from)),
            (None, Some(to)) => Some((to, to)),
            (None, None) => None,
        };
        if let Some((from, to)) = stroke_colors {
            self.timeline.add_clip(
                parent_track,
                self.current_time,
                anim.duration,
                ClipPayload::Animation(AnimationSpec {
                    target: anim.target,
                    lens: PropertyLensSpec::StrokeColor { from, to },
                    rate_func: anim.rate_func.clone(),
                    delay: 0.0,
                    label: None,
                }),
            );
        }

        if source_stroke.is_some() || target_stroke.is_some() {
            self.timeline.add_clip(
                parent_track,
                self.current_time,
                anim.duration,
                ClipPayload::Animation(AnimationSpec {
                    target: anim.target,
                    lens: PropertyLensSpec::StrokeWidth {
                        from: source_stroke
                            .map(|_| source_state.stroke.style.width)
                            .unwrap_or(0.0),
                        to: target_stroke
                            .map(|_| target_state.stroke.style.width)
                            .unwrap_or(0.0),
                    },
                    rate_func: anim.rate_func.clone(),
                    delay: 0.0,
                    label: None,
                }),
            );
        }

        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Opacity {
                    from: source_state.opacity,
                    to: target_visual_opacity,
                },
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: None,
            }),
        );

        // Zero-duration PathMorph at morph end: locks the source entity's path
        // to the target geometry. Without this, the seek-based snapshot restore
        // would reset the source path to its original shape after the morph.
        self.timeline.add_clip(
            parent_track,
            morph_end_time,
            0.0,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::PathMorph {
                    from: (*target_state.path).clone(),
                    to: (*target_state.path).clone(),
                },
                rate_func: gaanim_math::RateFunc::Linear,
                delay: 0.0,
                label: None,
            }),
        );

        if is_replacement {
            // ReplacementTransform swaps identities at the end: the source
            // disappears and the actual target hierarchy/entity becomes visible.
            self.timeline.add_clip(
                parent_track,
                morph_end_time,
                0.0,
                ClipPayload::Animation(AnimationSpec {
                    target: anim.target,
                    lens: PropertyLensSpec::Opacity {
                        from: source_state.opacity,
                        to: 0.0,
                    },
                    rate_func: gaanim_math::RateFunc::Linear,
                    delay: 0.0,
                    label: None,
                }),
            );
            if target_has_children {
                self.schedule_show_hierarchy(target, &target_state, parent_track, morph_end_time);
            } else {
                self.timeline.add_clip(
                    parent_track,
                    morph_end_time,
                    0.0,
                    ClipPayload::Animation(AnimationSpec {
                        target,
                        lens: PropertyLensSpec::Opacity {
                            from: 0.0,
                            to: target_visual_opacity,
                        },
                        rate_func: gaanim_math::RateFunc::Linear,
                        delay: 0.0,
                        label: None,
                    }),
                );
            }
        }

        // Update hot state of source to match target
        if let Some(state) = self.states.get_mut(anim.target) {
            state.transform = target_state.transform;
            state.bounds = target_state.bounds;
            state.opacity = if is_replacement {
                0.0
            } else {
                target_visual_opacity
            };
            state.fill = target_state.fill.clone();
            state.stroke = target_state.stroke.clone();
            state.path = target_state.path.clone();
            // Transform keeps the source ObjectId/entity and leaves it as a
            // flattened vector object. This makes subsequent transforms on the
            // original handle deterministic. ReplacementTransform deliberately
            // ends the source object's lifetime instead.
            state.child_spans.clear();
            state.children.clear();
        }
    }

    fn world_center_for_match(&self, id: ObjectId) -> (f64, f64) {
        // Use the glyph's transform world position, not its visual center.
        // For text, bounds.center() is the shape's center, not its typographic
        // origin; adding it would shift each glyph by half its width and break
        // the final alignment. For shapes, bounds.center() is ~0, so no effect.
        let world = self.get_world_transform(id).translation;
        (world.x, world.y)
    }

    fn build_match_item(&self, id: ObjectId, key: Option<String>) -> Option<MatchItem> {
        let state = self.states.get(id)?;
        let center = self.world_center_for_match(id);
        let fill = state.fill.as_ref().and_then(extract_brush_color);
        Some(MatchItem {
            index: 0, // caller sets
            path: (*state.path).clone(),
            center,
            fill,
            key,
        })
    }

    fn collect_glyph_match_data(&self, root: ObjectId) -> (Vec<ObjectId>, Vec<MatchItem>) {
        let Some(state) = self.states.get(root) else {
            return (Vec::new(), Vec::new());
        };
        if state.child_spans.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let mut ids = Vec::new();
        let mut items = Vec::new();
        for (pos, child) in state.child_spans.iter().enumerate() {
            if let Some(item) =
                self.build_match_item(child.id, Some(child.span.character.to_string()))
            {
                ids.push(child.id);
                let mut it = item;
                it.index = pos;
                items.push(it);
            }
        }
        (ids, items)
    }

    fn collect_leaf_match_data(&self, root: ObjectId) -> (Vec<ObjectId>, Vec<MatchItem>) {
        let Some(root_state) = self.states.get(root) else {
            return (Vec::new(), Vec::new());
        };
        // If glyph hierarchy, flatten glyphs
        if !root_state.child_spans.is_empty() {
            return self.collect_glyph_match_data(root);
        }
        // Collect leaf ids recursively
        let mut leaf_ids = Vec::new();
        if root_state.children.is_empty() {
            leaf_ids.push(root);
        } else {
            let mut stack = root_state.children.clone();
            let mut visited = std::collections::HashSet::new();
            while let Some(cid) = stack.pop() {
                if !visited.insert(cid) {
                    continue;
                }
                let Some(cstate) = self.states.get(cid) else {
                    continue;
                };
                if !cstate.child_spans.is_empty() {
                    for child in &cstate.child_spans {
                        leaf_ids.push(child.id);
                    }
                } else if !cstate.children.is_empty() {
                    stack.extend(cstate.children.iter().cloned());
                } else {
                    leaf_ids.push(cid);
                }
            }
            // Keep deterministic order: sort by world x then y
            leaf_ids.sort_by(|a, b| {
                let ca = self.world_center_for_match(*a);
                let cb = self.world_center_for_match(*b);
                ca.0.partial_cmp(&cb.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(ca.1.partial_cmp(&cb.1).unwrap_or(std::cmp::Ordering::Equal))
            });
        }
        let mut items = Vec::new();
        for (pos, id) in leaf_ids.iter().enumerate() {
            if let Some(mut it) = self.build_match_item(*id, None) {
                it.index = pos;
                items.push(it);
            }
        }
        (leaf_ids, items)
    }

    /// Transform matching — improved over Manim's TransformMatchingShapes/Tex.
    ///
    /// Auto-matches submobjects between `source` and `target`:
    /// - `Shapes` mode: geometry + position + color cost, Hungarian + shape hash bonus.
    /// - `Tex` mode: LCS on character keys first (order-preserving), then Hungarian
    ///   on remainder with tex penalty.
    /// Matched source leaves morph into target leaves (path, transform, colors);
    /// unmatched source leaves fade out, unmatched target leaves fade in.
    pub fn play_transform_matching(
        &mut self,
        source: ObjectId,
        target: ObjectId,
        mode: MatchingMode,
        duration: f64,
        rate_func: gaanim_math::RateFunc,
    ) {
        if !duration.is_finite() || duration <= 0.0 {
            return;
        }
        if self.states.get(source).is_none() || self.states.get(target).is_none() {
            return;
        }
        if mode == MatchingMode::Tex {
            self.play_equation_transition(
                source,
                target,
                Vec::new(),
                duration,
                EquationTransitionMode::Replace,
                true,
            );
            return;
        }

        // Gather match data
        let (src_ids, src_items) = match mode {
            MatchingMode::Tex => {
                let (ids, items) = self.collect_glyph_match_data(source);
                if !ids.is_empty() {
                    // For Tex, also need target glyph data to decide fallback
                    let (tids, _) = self.collect_glyph_match_data(target);
                    if !tids.is_empty() {
                        (ids, items)
                    } else {
                        self.collect_leaf_match_data(source)
                    }
                } else {
                    self.collect_leaf_match_data(source)
                }
            }
            MatchingMode::Shapes => self.collect_leaf_match_data(source),
        };
        let (dst_ids, dst_items) = match mode {
            MatchingMode::Tex => {
                let (ids, items) = self.collect_glyph_match_data(target);
                if !ids.is_empty() {
                    (ids, items)
                } else {
                    self.collect_leaf_match_data(target)
                }
            }
            MatchingMode::Shapes => self.collect_leaf_match_data(target),
        };

        if src_ids.is_empty() || dst_ids.is_empty() {
            return;
        }

        // Single-element fallback: if both are singletons (root leaf) we can
        // reuse the classic transform path for perfect continuity.
        // But generic matching also handles 1-1; keep generic for now.

        let config = MatchingConfig {
            mode,
            ..Default::default()
        };
        let result = gaanim_math::matching::match_items(&src_items, &dst_items, &config);

        let start = self.current_time;
        let end = start + duration;

        // Hide all dst leaves at start (zero-duration) — they will be revealed
        // via morph or fade-in. This prevents them being visible before morph.
        for &dst_id in &dst_ids {
            if let Some(state) = self.states.get(dst_id).cloned() {
                let from = state.opacity;
                let entity = state.entity;
                let track = self.ensure_track(dst_id);
                self.timeline.add_clip(
                    track,
                    start,
                    0.0,
                    ClipPayload::Animation(AnimationSpec {
                        target: dst_id,
                        lens: PropertyLensSpec::Opacity { from, to: 0.0 },
                        rate_func: gaanim_math::RateFunc::Linear,
                        delay: 0.0,
                        label: None,
                    }),
                );
                // Immediate ECS hide to avoid flash before first clip
                self.commands.entity(entity).insert(Opacity(0.0));
            }
        }

        // Build quick lookup for matched status
        let matched_src_set: std::collections::HashSet<usize> =
            result.pairs.iter().map(|(s, _)| *s).collect();
        let matched_dst_set: std::collections::HashSet<usize> =
            result.pairs.iter().map(|(_, d)| *d).collect();

        // For each matched pair, schedule morph on source leaf toward target leaf
        for (src_pos, dst_pos) in &result.pairs {
            let src_id = src_ids[*src_pos];
            let dst_id = dst_ids[*dst_pos];
            let Some(src_state) = self.states.get(src_id).cloned() else {
                continue;
            };
            let Some(dst_state) = self.states.get(dst_id).cloned() else {
                continue;
            };
            let track = self.ensure_track(src_id);

            // Compute final local transform for `src_id` so it lands exactly
            // on `dst_id`'s world transform (position, rotation, scale) within `src_id`'s parent space.
            let src_parent_affine = src_state
                .parent
                .map(|pid| self.get_world_transform(pid).to_affine_2d())
                .unwrap_or(gaanim_core::kurbo::Affine::IDENTITY);
            let dst_world_affine = self.get_world_transform(dst_id).to_affine_2d();

            let target_local_affine = src_parent_affine.inverse() * dst_world_affine;
            let target_transform = SpatialTransform::from_affine_2d(&target_local_affine);

            // PathMorph
            self.timeline.add_clip(
                track,
                start,
                duration,
                ClipPayload::Animation(AnimationSpec {
                    target: src_id,
                    lens: PropertyLensSpec::PathMorph {
                        from: (*src_state.path).clone(),
                        to: (*dst_state.path).clone(),
                    },
                    rate_func: rate_func.clone(),
                    delay: 0.0,
                    label: Some("TransformMatching".to_string()),
                }),
            );
            // Translation
            self.timeline.add_clip(
                track,
                start,
                duration,
                ClipPayload::Animation(AnimationSpec {
                    target: src_id,
                    lens: PropertyLensSpec::Translation {
                        from: src_state.transform.translation,
                        to: target_transform.translation,
                    },
                    rate_func: rate_func.clone(),
                    delay: 0.0,
                    label: None,
                }),
            );
            // Rotation
            self.timeline.add_clip(
                track,
                start,
                duration,
                ClipPayload::Animation(AnimationSpec {
                    target: src_id,
                    lens: PropertyLensSpec::Rotation {
                        from: src_state.transform.rotation,
                        to: target_transform.rotation,
                    },
                    rate_func: rate_func.clone(),
                    delay: 0.0,
                    label: None,
                }),
            );
            // Scale
            self.timeline.add_clip(
                track,
                start,
                duration,
                ClipPayload::Animation(AnimationSpec {
                    target: src_id,
                    lens: PropertyLensSpec::Scale {
                        from: src_state.transform.scale,
                        to: target_transform.scale,
                    },
                    rate_func: rate_func.clone(),
                    delay: 0.0,
                    label: None,
                }),
            );
            // FillColor
            let src_fill = src_state
                .fill
                .as_ref()
                .and_then(extract_brush_color)
                .unwrap_or(Color::WHITE);
            let dst_fill = dst_state
                .fill
                .as_ref()
                .and_then(extract_brush_color)
                .unwrap_or(Color::WHITE);
            self.timeline.add_clip(
                track,
                start,
                duration,
                ClipPayload::Animation(AnimationSpec {
                    target: src_id,
                    lens: PropertyLensSpec::FillColor {
                        from: src_fill,
                        to: dst_fill,
                    },
                    rate_func: rate_func.clone(),
                    delay: 0.0,
                    label: None,
                }),
            );
            // Stroke
            let src_stroke = src_state
                .stroke
                .brush
                .as_ref()
                .and_then(extract_brush_color);
            let dst_stroke = dst_state
                .stroke
                .brush
                .as_ref()
                .and_then(extract_brush_color);
            let stroke_colors = match (src_stroke, dst_stroke) {
                (Some(f), Some(t)) => Some((f, t)),
                (Some(f), None) => Some((f, f)),
                (None, Some(t)) => Some((t, t)),
                (None, None) => None,
            };
            if let Some((from, to)) = stroke_colors {
                self.timeline.add_clip(
                    track,
                    start,
                    duration,
                    ClipPayload::Animation(AnimationSpec {
                        target: src_id,
                        lens: PropertyLensSpec::StrokeColor { from, to },
                        rate_func: rate_func.clone(),
                        delay: 0.0,
                        label: None,
                    }),
                );
            }
            if src_stroke.is_some() || dst_stroke.is_some() {
                self.timeline.add_clip(
                    track,
                    start,
                    duration,
                    ClipPayload::Animation(AnimationSpec {
                        target: src_id,
                        lens: PropertyLensSpec::StrokeWidth {
                            from: src_state.stroke.style.width,
                            to: dst_state.stroke.style.width,
                        },
                        rate_func: rate_func.clone(),
                        delay: 0.0,
                        label: None,
                    }),
                );
            }

            // Lock path at end (continuous seek)
            self.timeline.add_clip(
                track,
                end,
                0.0,
                ClipPayload::Animation(AnimationSpec {
                    target: src_id,
                    lens: PropertyLensSpec::PathMorph {
                        from: (*dst_state.path).clone(),
                        to: (*dst_state.path).clone(),
                    },
                    rate_func: gaanim_math::RateFunc::Linear,
                    delay: 0.0,
                    label: None,
                }),
            );

            // Update hot state for source leaf to target's final appearance
            if let Some(state) = self.states.get_mut(src_id) {
                state.path = dst_state.path.clone();
                state.bounds = dst_state.bounds;
                state.transform = target_transform;
                state.fill = dst_state.fill.clone();
                state.stroke = dst_state.stroke.clone();
                // Keep opacity 1 for matched
                state.opacity = dst_state.opacity;
            }
        }

        // Unmatched source: fade out
        for (pos, &src_id) in src_ids.iter().enumerate() {
            if matched_src_set.contains(&pos) {
                continue;
            }
            if let Some(state) = self.states.get(src_id).cloned() {
                let track = self.ensure_track(src_id);
                self.timeline.add_clip(
                    track,
                    start,
                    duration,
                    ClipPayload::Animation(AnimationSpec {
                        target: src_id,
                        lens: PropertyLensSpec::Opacity {
                            from: state.opacity,
                            to: 0.0,
                        },
                        rate_func: rate_func.clone(),
                        delay: 0.0,
                        label: Some("TransformMatchingFade".to_string()),
                    }),
                );
                if let Some(s) = self.states.get_mut(src_id) {
                    s.opacity = 0.0;
                }
            }
        }

        // Unmatched dst: fade in (they are hidden at start)
        for (pos, &dst_id) in dst_ids.iter().enumerate() {
            if matched_dst_set.contains(&pos) {
                continue;
            }
            if let Some(state) = self.states.get(dst_id).cloned() {
                let track = self.ensure_track(dst_id);
                let target_opacity = state.opacity.max(1.0);
                self.timeline.add_clip(
                    track,
                    start,
                    duration,
                    ClipPayload::Animation(AnimationSpec {
                        target: dst_id,
                        lens: PropertyLensSpec::Opacity {
                            from: 0.0,
                            to: target_opacity,
                        },
                        rate_func: rate_func.clone(),
                        delay: 0.0,
                        label: Some("TransformMatchingFade".to_string()),
                    }),
                );
                if let Some(s) = self.states.get_mut(dst_id) {
                    s.opacity = target_opacity;
                }
                // Also ensure they become visible at end via schedule_show? opacity clip suffices
            }
        }

        self.current_time = end;
    }

    fn play_wiggle_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let state = match self.states.get(anim.target) {
            Some(s) => s,
            None => return,
        };
        let origin = state.transform.translation;
        let num_wiggles = 6;
        let step = anim.duration / num_wiggles as f64;
        let amplitude = 5.0;

        for i in 0..num_wiggles {
            let dir = if i % 2 == 0 { 1.0_f64 } else { -1.0_f64 };
            let offset_x = if i == num_wiggles - 1 {
                0.0
            } else {
                dir * amplitude
            };
            let from_x = if i == 0 {
                origin.x
            } else {
                origin.x - dir * amplitude
            };
            let to_x = origin.x + offset_x;

            self.timeline.add_clip(
                parent_track,
                self.current_time + i as f64 * step,
                step,
                ClipPayload::Animation(AnimationSpec {
                    target: anim.target,
                    lens: PropertyLensSpec::Translation {
                        from: gaanim_core::glam::DVec3::new(from_x, origin.y, origin.z),
                        to: gaanim_core::glam::DVec3::new(to_x, origin.y, origin.z),
                    },
                    rate_func: gaanim_math::RateFunc::Linear,
                    delay: 0.0,
                    label: self.current_label.clone(),
                }),
            );
        }
    }

    fn play_grow_from_point_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let (px, py) = match &anim.anim_type {
            AnimationType::GrowFromPoint { px, py } => (*px, *py),
            _ => return,
        };

        let state = match self.states.get_mut(anim.target) {
            Some(s) => s,
            None => return,
        };

        let target_scale = state.transform.scale;
        let target_pos = state.transform.translation;

        let from = gaanim_core::glam::DVec3::ZERO;
        state.transform.scale = from;
        let mut temp_transform = state.transform;
        temp_transform.scale = from;
        temp_transform.translation = gaanim_core::glam::DVec3::new(px, py, 0.0);
        self.commands.entity(state.entity).insert(temp_transform);

        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from,
                    to: target_scale,
                },
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );

        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Translation {
                    from: gaanim_core::glam::DVec3::new(px, py, 0.0),
                    to: target_pos,
                },
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );
    }

    fn play_grow_from_edge_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let direction = match &anim.anim_type {
            AnimationType::GrowFromEdge { direction } => direction.clone(),
            _ => return,
        };

        let state = match self.states.get_mut(anim.target) {
            Some(s) => s,
            None => return,
        };

        let target_scale = state.transform.scale;
        let target_pos = state.transform.translation;
        let bounds = state.bounds;

        let (edge_lx, edge_ly) = match direction.as_str() {
            "up" | "top" => (0.0, bounds.max.y),
            "down" | "bottom" => (0.0, bounds.min.y),
            "left" => (bounds.min.x, 0.0),
            "right" => (bounds.max.x, 0.0),
            _ => (0.0, 0.0),
        };
        let edge_world = target_pos + gaanim_core::glam::DVec3::new(edge_lx, edge_ly, 0.0);

        let from = gaanim_core::glam::DVec3::ZERO;
        state.transform.scale = from;
        let mut temp_transform = state.transform;
        temp_transform.scale = from;
        temp_transform.translation = edge_world;
        self.commands.entity(state.entity).insert(temp_transform);

        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from,
                    to: target_scale,
                },
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );

        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Translation {
                    from: edge_world,
                    to: target_pos,
                },
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );
    }

    fn play_flash_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let state = match self.states.get(anim.target) {
            Some(s) => s,
            None => return,
        };
        let original_opacity = state.opacity;
        let original_scale = state.transform.scale;

        let n_lines = match &anim.anim_type {
            AnimationType::Flash { n_lines, .. } => *n_lines,
            _ => 12,
        };
        let radius = match &anim.anim_type {
            AnimationType::Flash { radius, .. } => *radius,
            _ => 100.0,
        };
        let half = anim.duration * 0.5;

        // Fade out then back in (1.0 -> 0.0 -> 1.0)
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Opacity {
                    from: original_opacity,
                    to: 0.0,
                },
                rate_func: gaanim_math::RateFunc::Smooth,
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );
        self.timeline.add_clip(
            parent_track,
            self.current_time + half,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Opacity {
                    from: 0.0,
                    to: original_opacity,
                },
                rate_func: gaanim_math::RateFunc::Smooth,
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );

        // Quick scale pulse to amplify the "flash" effect.
        // Pulse up to scale_factor proportional to the number of lines for visual feedback.
        let scale_factor = 1.0 + (n_lines as f64 / 12.0) * 0.25;
        let scale_to = original_scale * scale_factor;
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from: original_scale,
                    to: scale_to,
                },
                rate_func: gaanim_math::RateFunc::Smooth,
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );
        self.timeline.add_clip(
            parent_track,
            self.current_time + half,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from: scale_to,
                    to: original_scale,
                },
                rate_func: gaanim_math::RateFunc::Smooth,
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );

        if let Some(s) = self.states.get_mut(anim.target) {
            s.opacity = original_opacity;
            s.transform.scale = original_scale;
        }
        let _ = radius; // reserved for future "radial line" geometry
    }

    fn play_circumscribe_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let color = match &anim.anim_type {
            AnimationType::Circumscribe { color } => *color,
            _ => None,
        };

        let state = match self.states.get(anim.target) {
            Some(s) => s,
            None => return,
        };
        let original_opacity = state.opacity;
        let original_scale = state.transform.scale;

        let half = anim.duration * 0.5;

        // Scale up then back down (1.0 -> 1.1 -> 1.0)
        let scale_to = original_scale * 1.1;
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from: original_scale,
                    to: scale_to,
                },
                rate_func: gaanim_math::RateFunc::Smooth,
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );
        self.timeline.add_clip(
            parent_track,
            self.current_time + half,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from: scale_to,
                    to: original_scale,
                },
                rate_func: gaanim_math::RateFunc::Smooth,
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );

        // Optional fill color highlight (like Indicate but without scale_factor difference).
        if let Some(c) = color
            && let Some(Brush::Solid(current)) = &state.fill
        {
            self.timeline.add_clip(
                parent_track,
                self.current_time,
                half,
                ClipPayload::Animation(AnimationSpec {
                    target: anim.target,
                    lens: PropertyLensSpec::FillColor {
                        from: *current,
                        to: c,
                    },
                    rate_func: gaanim_math::RateFunc::Linear,
                    delay: 0.0,
                    label: self.current_label.clone(),
                }),
            );
            self.timeline.add_clip(
                parent_track,
                self.current_time + half,
                half,
                ClipPayload::Animation(AnimationSpec {
                    target: anim.target,
                    lens: PropertyLensSpec::FillColor {
                        from: c,
                        to: *current,
                    },
                    rate_func: gaanim_math::RateFunc::Linear,
                    delay: 0.0,
                    label: self.current_label.clone(),
                }),
            );
        }

        let _ = original_opacity; // reserved if we want to fade in/out later

        if let Some(s) = self.states.get_mut(anim.target) {
            s.transform.scale = original_scale;
        }
    }

    /// Internal: schedule a `MoveAlongPath` animation. The target's
    /// translation is sampled from the Bézier path at the eased `t`
    /// (parametric, not arc-length uniform). Updates the tracked state
    /// so the final translation equals `path(1.0)`.
    fn play_move_along_path_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let (path_arg, path_target) = match &anim.anim_type {
            AnimationType::MoveAlongPath { path, path_target } => (path.clone(), *path_target),
            _ => unreachable!(),
        };

        let path = if let Some(target_id) = path_target {
            if let Some(state) = self.states.get(target_id) {
                let mut p = (*state.path).clone();
                let world_affine = self.get_world_transform(target_id).to_affine_2d();
                p.apply_affine(world_affine);
                p
            } else {
                path_arg
            }
        } else {
            path_arg
        };

        // Resolve and persist the final translation so subsequent
        // animations build on top of the new position.
        let end_point = gaanim_math::get_point_at_alpha(&path, 1.0);
        let end_translation = gaanim_core::glam::DVec3::new(end_point.x, end_point.y, 0.0);

        if let Some(state) = self.states.get_mut(anim.target) {
            state.transform.translation = end_translation;
            state.transform.anchor = gaanim_core::glam::DVec3::ZERO;
            self.commands.entity(state.entity).insert(state.transform);
        }

        let clip_start = self.current_time + anim.delay;
        self.timeline.add_clip(
            parent_track,
            clip_start,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::PathFollow { path },
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );
    }

    fn play_move_along_path_3d_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let points = match &anim.anim_type {
            AnimationType::MoveAlongPath3D { points } => points.clone(),
            _ => unreachable!(),
        };
        if points.len() < 2 || points.iter().any(|point| !point.is_finite()) {
            return;
        }
        let end_translation = *points.last().expect("validated non-empty path");
        if let Some(state) = self.states.get_mut(anim.target) {
            state.transform.translation = end_translation;
            state.transform.anchor = gaanim_core::glam::DVec3::ZERO;
            self.commands.entity(state.entity).insert(state.transform);
        }
        let clip_start = self.current_time + anim.delay;
        self.timeline.add_clip(
            parent_track,
            clip_start,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::PathFollow3D { points },
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );
    }

    fn play_rotate_by_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let (angle_radians, pivot) = match &anim.anim_type {
            AnimationType::RotateBy {
                angle_radians,
                pivot,
            } => (*angle_radians, *pivot),
            _ => unreachable!(),
        };

        let state = match self.states.get_mut(anim.target) {
            Some(s) => s,
            None => return,
        };

        let from_rot = state.transform.rotation;
        let to_rot = from_rot * gaanim_core::glam::DQuat::from_rotation_z(angle_radians);
        let from_trans = state.transform.translation;
        let to_trans = if let Some(p) = pivot {
            let rot = gaanim_core::glam::DQuat::from_rotation_z(angle_radians);
            p + rot * (from_trans - p)
        } else {
            from_trans
        };

        state.transform.rotation = to_rot;
        state.transform.translation = to_trans;

        if pivot.is_some() {
            state.transform.anchor = gaanim_core::glam::DVec3::ZERO;
            self.commands.entity(state.entity).insert(state.transform);
        }

        let clip_start = self.current_time + anim.delay;

        if let Some(p) = pivot {
            let r = (from_trans.x - p.x).hypot(from_trans.y - p.y);
            if r > 1e-6 {
                let theta0 = (from_trans.y - p.y).atan2(from_trans.x - p.x);
                let arc = gaanim_core::kurbo::Arc::new(
                    gaanim_core::kurbo::Point::new(p.x, p.y),
                    gaanim_core::kurbo::Vec2::new(r, r),
                    theta0,
                    angle_radians,
                    0.0,
                );
                let arc_path = arc.into_path(0.1);
                self.timeline.add_clip(
                    parent_track,
                    clip_start,
                    anim.duration,
                    ClipPayload::Animation(AnimationSpec {
                        target: anim.target,
                        lens: PropertyLensSpec::PathFollow { path: arc_path },
                        rate_func: anim.rate_func.clone(),
                        delay: 0.0,
                        label: self.current_label.clone(),
                    }),
                );
            }
        }

        if angle_radians.abs() > std::f64::consts::PI {
            let half_dur = anim.duration * 0.5;
            let mid_rot = from_rot * gaanim_core::glam::DQuat::from_rotation_z(angle_radians * 0.5);
            self.timeline.add_clip(
                parent_track,
                clip_start,
                half_dur,
                ClipPayload::Animation(AnimationSpec {
                    target: anim.target,
                    lens: PropertyLensSpec::Rotation {
                        from: from_rot,
                        to: mid_rot,
                    },
                    rate_func: anim.rate_func.clone(),
                    delay: 0.0,
                    label: self.current_label.clone(),
                }),
            );
            self.timeline.add_clip(
                parent_track,
                clip_start + half_dur,
                half_dur,
                ClipPayload::Animation(AnimationSpec {
                    target: anim.target,
                    lens: PropertyLensSpec::Rotation {
                        from: mid_rot,
                        to: to_rot,
                    },
                    rate_func: anim.rate_func.clone(),
                    delay: 0.0,
                    label: self.current_label.clone(),
                }),
            );
        } else {
            self.timeline.add_clip(
                parent_track,
                clip_start,
                anim.duration,
                ClipPayload::Animation(AnimationSpec {
                    target: anim.target,
                    lens: PropertyLensSpec::Rotation {
                        from: from_rot,
                        to: to_rot,
                    },
                    rate_func: anim.rate_func,
                    delay: 0.0,
                    label: self.current_label.clone(),
                }),
            );
        }
    }

    /// Internal: schedule a `GrowArrow` animation as a Create-style
    /// outline draw followed by a brief scale "punch" that emphasizes
    /// the arrowhead's arrival at the end of the trajectory.
    fn play_grow_arrow_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        if self.states.get(anim.target).is_none() {
            bevy::prelude::warn!(
                "Attempted to GrowArrow unregistered Mobject: {:?}",
                anim.target
            );
            return;
        }

        // (B) Set initial value via deferred commands to avoid first-frame
        // flash: insert FillDrawProgress(0.0) AND an empty Path2D so the
        // renderer sees "no fill, no path" before the timeline runs.
        if let Some(state) = self.states.get(anim.target) {
            self.commands
                .entity(state.entity)
                .insert(gaanim_animation::FillDrawProgress(0.0));
            self.commands
                .entity(state.entity)
                .insert(gaanim_scene::components::Path2D(std::sync::Arc::new(
                    gaanim_core::kurbo::BezPath::new(),
                )));
        }

        // Phase 1: 70% of duration draws the outline (PathCompletion
        // 0 -> 1). The fill is held hidden during the draw, then
        // cross-fades in over the last 30% to give the arrowhead
        // emphasis.
        let draw_duration = anim.duration * 0.7;
        let fill_duration = anim.duration * 0.3;

        // Hold the fill at 0 during the draw phase so the outline-only
        // stage is visible.
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            draw_duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::FillDrawProgress { from: 0.0, to: 0.0 },
                rate_func: gaanim_math::RateFunc::Linear,
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );

        // Trace the outline.
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            draw_duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 },
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );

        // Cross-fade the fill in over the last segment, then a brief
        // scale punch (1.0 -> 1.15 -> 1.0) to highlight the arrowhead.
        self.timeline.add_clip(
            parent_track,
            self.current_time + draw_duration,
            fill_duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::FillDrawProgress { from: 0.0, to: 1.0 },
                rate_func: gaanim_math::RateFunc::Smooth,
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );

        // Brief scale punch on the arrowhead (25% of the fill phase
        // each way). Yields a quick "pop" as the fill reveals.
        let punch_half = fill_duration * 0.5;
        let original_scale = self
            .states
            .get(anim.target)
            .map(|s| s.transform.scale)
            .unwrap_or(gaanim_core::glam::DVec3::ONE);
        let scale_to = original_scale * 1.15;
        self.timeline.add_clip(
            parent_track,
            self.current_time + draw_duration,
            punch_half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from: original_scale,
                    to: scale_to,
                },
                rate_func: gaanim_math::RateFunc::Smooth,
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );
        self.timeline.add_clip(
            parent_track,
            self.current_time + draw_duration + punch_half,
            punch_half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from: scale_to,
                    to: original_scale,
                },
                rate_func: gaanim_math::RateFunc::Smooth,
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );
    }

    /// Creates a hierarchical group of Mobjects.
    ///
    /// The group is a Mobject itself (with a GroupMarker component).
    /// The children are reparented under the group using Bevy's hierarchy while
    /// retaining their authored coordinate frame. The group keeps its visual
    /// center as the transform pivot, so rotations and scales remain centered
    /// without rewriting coordinates consumed by child updaters.
    pub fn group(&mut self, children: &[MobjectRef]) -> MobjectRef {
        let id = self.next_id();

        // 1. Calculate the collective bounds of the children in world space
        let mut union_bounds = Bounds3D::default();
        let mut has_bounds = false;

        for child in children {
            if let Some(state) = self.states.get(child.id) {
                // child world bounds = child local bounds transformed by its TRUE world transform
                let world_transform = self.get_world_transform(child.id);
                let world_bounds = state.bounds.transform_2d(&world_transform.to_affine_2d());
                if !has_bounds {
                    union_bounds = world_bounds;
                    has_bounds = true;
                } else {
                    union_bounds = union_bounds.union(&world_bounds);
                }
            }
        }

        // 2. Keep the group matrix at identity and use the visual center only as
        // its pivot. Re-centering every child would change the local coordinate
        // system seen by custom updaters after grouping.
        let center = if has_bounds {
            union_bounds.center()
        } else {
            gaanim_core::glam::DVec3::ZERO
        };
        let group_transform = SpatialTransform::identity().with_anchor(center);

        // 3. Spawn the group entity with GroupMarker, Opacity, WorldBounds etc.
        // Include Bevy Transform/Visibility so that any 3D child with GlobalTransform
        // satisfies Bevy's B0004 hierarchy validation (parent must have GlobalTransform).
        let group_entity = self
            .commands
            .spawn((
                GroupMarker,
                MobjectId(id),
                group_transform,
                gaanim_math::GlobalSpatialTransform::from_local(&group_transform),
                Transform::default(),
                Visibility::default(),
                Opacity(1.0),
                gaanim_scene::GlobalOpacity(1.0),
                LocalBounds(union_bounds),
                WorldBounds(union_bounds),
                gaanim_scene::RenderOrder::default(),
                Visible,
            ))
            .id();

        self.tag_entity(group_entity);

        // 4. Reparent children without changing the authored coordinates of
        // root members. A member already parented elsewhere is first flattened
        // to world space because it is leaving that previous coordinate frame.
        let mut child_ids = Vec::new();

        for child in children {
            child_ids.push(child.id);
            let child_world = self.get_world_transform(child.id);
            if let Some(state) = self.states.get_mut(child.id) {
                let child_local = if state.parent.is_none() {
                    state.transform
                } else {
                    child_world
                };

                state.transform = child_local;
                state.parent = Some(id); // Track parent for world transform calculation

                self.commands
                    .entity(state.entity)
                    .set_parent_in_place(group_entity)
                    .insert(child_local);
            }
        }

        // 5. Ensure tracks exist for group and children so the timeline can
        //    display the group hierarchy (group → children).
        self.ensure_track(id);
        for &child_id in &child_ids {
            self.ensure_track(child_id);
        }

        // 6. Store group state
        let group_state = MobjectState {
            path: std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
            bounds: union_bounds,
            transform: group_transform,
            opacity: 1.0,
            fill: None,
            stroke: gaanim_scene::StrokeBrush::transparent(),
            entity: group_entity,
            child_spans: Vec::new(),
            children: child_ids,
            parent: None,
            exclude_from_parent_draw: false,
        };
        self.states.insert(id, group_state);
        self.mobject_names
            .insert(id, format!("Group ({} children)", children.len()));

        MobjectRef { id }
    }

    /// Create a group that preserves children's local transforms (no auto-centering).
    /// Used for coordinate-space view/root so that `data_to_local` stays at the view's
    /// origin and plots remain aligned without baking a center offset into the hierarchy.
    pub fn group_identity(&mut self, children: &[MobjectRef]) -> MobjectRef {
        let id = self.next_id();
        let group_transform = SpatialTransform::default();
        // Union of children's local bounds (group is at identity, so local == world for children)
        let mut union_bounds = Bounds3D::default();
        let mut has_bounds = false;
        for child in children {
            if let Some(state) = self.states.get(child.id) {
                let child_bounds_in_group =
                    gaanim_layout::transform_bounds(state.bounds, &state.transform);
                if !has_bounds {
                    union_bounds = child_bounds_in_group;
                    has_bounds = true;
                } else {
                    union_bounds = union_bounds.union(&child_bounds_in_group);
                }
            }
        }
        if !has_bounds {
            union_bounds = Bounds3D::default();
        }
        let group_entity = self
            .commands
            .spawn((
                GroupMarker,
                MobjectId(id),
                group_transform,
                gaanim_math::GlobalSpatialTransform::from_local(&group_transform),
                Transform::default(),
                Visibility::default(),
                Opacity(1.0),
                gaanim_scene::GlobalOpacity(1.0),
                LocalBounds(union_bounds),
                WorldBounds(union_bounds),
                gaanim_scene::RenderOrder::default(),
                Visible,
            ))
            .id();
        self.tag_entity(group_entity);
        let mut child_ids = Vec::new();
        for child in children {
            child_ids.push(child.id);
            if let Some(state) = self.states.get_mut(child.id) {
                state.parent = Some(id);
                self.commands
                    .entity(state.entity)
                    .set_parent_in_place(group_entity);
                // keep child's transform as-is (already in group's local space)
            }
        }
        self.ensure_track(id);
        for &child_id in &child_ids {
            self.ensure_track(child_id);
        }
        let group_state = MobjectState {
            path: std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
            bounds: union_bounds,
            transform: group_transform,
            opacity: 1.0,
            fill: None,
            stroke: gaanim_scene::StrokeBrush::transparent(),
            entity: group_entity,
            child_spans: Vec::new(),
            children: child_ids,
            parent: None,
            exclude_from_parent_draw: false,
        };
        self.states.insert(id, group_state);
        self.mobject_names
            .insert(id, format!("GroupNoCenter ({} children)", children.len()));
        MobjectRef { id }
    }

    /// Adds a child mobject to an existing group, adjusting its local transform.
    pub fn add_to_group(&mut self, group: MobjectRef, child: MobjectRef) {
        let group_entity = match self.states.get(group.id) {
            Some(state) => state.entity,
            None => return,
        };

        // eprintln for debugging removed
        // For regular groups we preserve world position when reparenting (so the child
        // does not visually jump). For coordinate-space plots `exclude_from_parent_draw`
        // is set, meaning the child's geometry is already in the group's local data
        // space (e.g. `data_to_local`); preserving world would bake the group's world
        // offset into the child and make it stay at scene origin when the space moves.
        let child_exclude = self
            .states
            .get(child.id)
            .map(|state| state.exclude_from_parent_draw)
            .unwrap_or(false);
        let child_local = if child_exclude {
            self.states
                .get(child.id)
                .map(|state| state.transform)
                .unwrap_or_default()
        } else {
            // Reparent child - use world transforms for both to handle nested groups correctly.
            // Using only the group's local transform fails when the group itself is a child
            // of another group (e.g. `view` inside `root` for CoordinateSpace), which would
            // make plots appear offset by the view's world position (the 8,10 px gap seen
            // with area/riemann/sine).
            let group_world = self.get_world_transform(group.id);
            let child_world = self.get_world_transform(child.id);
            let inv_group_affine = group_world.to_affine_2d().inverse();
            let child_local_affine = inv_group_affine * child_world.to_affine_2d();
            SpatialTransform::from_affine_2d(&child_local_affine)
        };

        if let Some(child_state) = self.states.get_mut(child.id) {
            child_state.transform = child_local;
            child_state.parent = Some(group.id);
            self.commands
                .entity(child_state.entity)
                .set_parent_in_place(group_entity)
                .insert(child_local);
        }

        // Add to group children list
        let mut children = Vec::new();
        let mut group_entity = None;
        if let Some(group_state) = self.states.get_mut(group.id) {
            if !group_state.children.contains(&child.id) {
                group_state.children.push(child.id);
            }
            children = group_state.children.clone();
            group_entity = Some(group_state.entity);
        }

        if let Some(group_ent) = group_entity {
            // Recompute union bounds
            let mut union_min =
                gaanim_core::glam::DVec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
            let mut union_max = gaanim_core::glam::DVec3::new(
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            );

            // Let's compute union bounds in group's local space
            for &child_id in &children {
                if let Some(state) = self.states.get(child_id) {
                    let child_bounds_in_group =
                        gaanim_layout::transform_bounds(state.bounds, &state.transform);
                    union_min = union_min.min(child_bounds_in_group.min);
                    union_max = union_max.max(child_bounds_in_group.max);
                }
            }

            if union_min.x < union_max.x && union_min.y < union_max.y {
                let new_bounds = Bounds3D::new(union_min, union_max);
                if let Some(group_state) = self.states.get_mut(group.id) {
                    group_state.bounds = new_bounds;
                }
                self.commands
                    .entity(group_ent)
                    .insert(gaanim_scene::LocalBounds(new_bounds));
            }
        }
    }

    /// Removes one child from a group while preserving its world transform.
    ///
    /// Layout reflow uses this when a versioned tree no longer contains a
    /// member. The child remains alive and visible, but subsequent transforms
    /// are evaluated in world space instead of the former group's local space.
    pub fn remove_from_group(&mut self, group: MobjectRef, child: MobjectRef) {
        let is_direct_child = self
            .states
            .get(child.id)
            .is_some_and(|state| state.parent == Some(group.id));
        if !is_direct_child {
            return;
        }

        let child_world = self.get_world_transform(child.id);
        if let Some(child_state) = self.states.get_mut(child.id) {
            child_state.transform = child_world;
            child_state.parent = None;
            self.commands
                .entity(child_state.entity)
                .remove_parent_in_place()
                .insert(child_world);
        }
        if let Some(group_state) = self.states.get_mut(group.id) {
            group_state.children.retain(|id| *id != child.id);
        }
    }

    /// Discharges all children from the group and despawns the group container entity.
    ///
    /// The children's local transforms are adjusted to world space so they remain in their
    /// absolute positions without any jumps.
    pub fn ungroup(&mut self, group: MobjectRef) {
        let (children_ids, group_transform, group_parent) =
            if let Some(state) = self.states.get(group.id) {
                (state.children.clone(), state.transform, state.parent)
            } else {
                return;
            };

        // Capture group fill/stroke for MobjectState propagation to children
        let group_fill_clone = self.states.get(group.id).map(|s| s.fill.clone());
        let group_stroke_clone = self.states.get(group.id).map(|s| s.stroke.clone());

        let group_affine = group_transform.to_affine_2d();

        for child_id in children_ids.clone() {
            let child_world = if let Some(child_state) = self.states.get(child_id) {
                let child_local = child_state.transform;
                let child_world_affine = group_affine * child_local.to_affine_2d();
                SpatialTransform::from_affine_2d(&child_world_affine)
            } else {
                continue;
            };

            if let Some(state) = self.states.get_mut(child_id) {
                // Propagate fill/stroke to MobjectState for subsequent construction ops
                if let Some(ref f) = group_fill_clone {
                    state.fill = f.clone();
                }
                if let Some(ref s) = group_stroke_clone {
                    state.stroke = s.clone();
                }
                state.transform = child_world;
                state.parent = group_parent;
            }
        }

        // No ECS commands here — the timeline Ungroup clip handles all hierarchy
        // mutations during playback. This keeps parent-child relationships intact
        // in ECS so that group-level animations (shift, rotate, etc.) affect all
        // children, and style propagation works before the ungroup time.

        // Pre-compute each child's world-space transform so the runtime ungroup
        // clip can re-apply them on subsequent seek frames (after the group entity
        // has been despawned and animation clips would otherwise overwrite the
        // correct world positions with stale local-space values).
        let children_world_transforms: Vec<(ObjectId, SpatialTransform)> = children_ids
            .iter()
            .filter_map(|&child_id| {
                self.states
                    .get(child_id)
                    .map(|state| (child_id, state.transform))
            })
            .collect();

        self.timeline.add_clip(
            self.default_track,
            self.current_time,
            0.0,
            ClipPayload::Ungroup {
                group: group.id,
                children: children_ids,
                group_parent,
                group_transform,
                children_world_transforms,
            },
        );

        self.states.remove(group.id);
        self.mobject_names.remove(&group.id);
    }

    /// Spawns a ValueTracker (FloatSignal) with the given initial value.
    pub fn value_tracker(&mut self, initial: f64) -> ValueTrackerRef {
        let id = self.next_id();
        let entity = self
            .commands
            .spawn((
                gaanim_scene::MobjectId(id),
                gaanim_animation::signals::FloatSignal::new(initial),
            ))
            .id();
        self.tag_entity(entity);

        let state = MobjectState {
            path: std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
            bounds: Bounds3D::default(),
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: None,
            stroke: StrokeBrush::default(),
            entity,
            child_spans: Vec::new(),
            children: Vec::new(),
            parent: None,
            exclude_from_parent_draw: false,
        };
        self.states.insert(id, state);
        self.float_signals.insert(id, initial);

        ValueTrackerRef { id }
    }

    /// Spawns a circle primitive.
    pub fn circle(&mut self, radius: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::circle(id, radius);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a rectangle primitive.
    pub fn rectangle(&mut self, width: f64, height: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::rectangle(id, width, height);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a decoded raster image using its native pixel dimensions.
    pub fn image(
        &mut self,
        image: gaanim_core::peniko::ImageData,
        view: gaanim_objects::prelude::ImageView,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::image(id, image, view);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns one resolved vector path from an imported SVG document.
    pub fn svg_path(
        &mut self,
        path: &gaanim_objects::prelude::SvgPath,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let mut bundle = MobjectBundle::new(id, path.path.clone(), path.bounds);
        bundle.fill = FillBrush(path.fill.clone());
        bundle.stroke = path.stroke.clone();
        bundle.tag = ObjectTag(if path.id.is_empty() {
            "SvgPath".into()
        } else {
            format!("SvgPath#{}", path.id)
        });
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a rounded rectangle primitive.
    pub fn rounded_rect(
        &mut self,
        width: f64,
        height: f64,
        corner_radius: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::rounded_rect(id, width, height, corner_radius);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a line segment primitive.
    pub fn line(
        &mut self,
        start: kurbo::Point,
        end: kurbo::Point,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::line(id, start, end);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a custom closed polygon primitive.
    pub fn polygon(&mut self, points: &[kurbo::Point]) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::polygon(id, points);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a line tangent to a polyline `curve` at fractional
    /// position `t` in `[0.0, 1.0]`. The line has half-length `length`
    /// on either side of the tangent point. Falls back to a line of
    /// zero length (origin) if the curve is degenerate.
    pub fn tangent_line(
        &mut self,
        curve: &[kurbo::Point],
        t: f64,
        length: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle =
            gaanim_objects::primitives::tangent_line(id, curve, t, length).unwrap_or_else(|| {
                gaanim_objects::primitives::line(id, kurbo::Point::ORIGIN, kurbo::Point::ORIGIN)
            });
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a Cartesian `NumberPlane` with grid + axes.
    /// `x_range`, `y_range` are `(min, max, step)` tuples.
    pub fn number_plane(
        &mut self,
        x_range: (f64, f64, f64),
        y_range: (f64, f64, f64),
        axis_stroke: f64,
        grid_stroke: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::number_plane(
            id,
            x_range,
            y_range,
            axis_stroke,
            grid_stroke,
        );
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns an open path (polyline) primitive.
    pub fn open_path(&mut self, points: &[kurbo::Point]) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::open_path(id, points);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a 3D triangle mesh (surface) from world-space vertices and indices.
    pub fn spawn_triangle_mesh(
        &mut self,
        vertices: Vec<[f32; 3]>,
        indices: Vec<u32>,
        color: Option<Color>,
    ) -> MobjectRef {
        self.spawn_triangle_mesh_with_colors(vertices, indices, color, None)
    }

    pub fn spawn_triangle_mesh_with_colors(
        &mut self,
        vertices: Vec<[f32; 3]>,
        indices: Vec<u32>,
        color: Option<Color>,
        colors: Option<Vec<[f32; 4]>>,
    ) -> MobjectRef {
        self.spawn_triangle_mesh_data(TriangleMeshData {
            vertices,
            indices,
            normals: None,
            uvs: None,
            color,
            colors,
            material: None,
        })
    }

    pub fn spawn_triangle_mesh_data(&mut self, data: TriangleMeshData) -> MobjectRef {
        let id = self.next_id();
        // Compute bounds
        let mut min = gaanim_core::glam::DVec3::splat(f64::INFINITY);
        let mut max = gaanim_core::glam::DVec3::splat(f64::NEG_INFINITY);
        for v in &data.vertices {
            let p = gaanim_core::glam::DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64);
            min = min.min(p);
            max = max.max(p);
        }
        if min.x == f64::INFINITY {
            min = gaanim_core::glam::DVec3::ZERO;
            max = gaanim_core::glam::DVec3::ZERO;
        }
        let bounds = Bounds3D::new(min, max);
        let entity = self
            .commands
            .spawn((
                MobjectId(id),
                SpatialTransform::default(),
                GlobalSpatialTransform::default(),
                LocalBounds(bounds),
                WorldBounds::default(),
                data.clone(),
                Mesh3DMarker,
                Transform::default(),
                GlobalTransform::default(),
                Visibility::default(),
                Visible,
                Opacity(1.0),
                gaanim_scene::GlobalOpacity(1.0),
            ))
            .id();
        if let Some(material) = data.material {
            self.commands.entity(entity).insert(material);
        }
        self.tag_entity(entity);
        let state = MobjectState {
            path: std::sync::Arc::new(kurbo::BezPath::new()),
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: data.color.map(|c| Brush::Solid(c)),
            stroke: StrokeBrush::transparent(),
            entity,
            child_spans: Vec::new(),
            children: Vec::new(),
            parent: None,
            exclude_from_parent_draw: false,
        };
        self.states.insert(id, state);
        MobjectRef { id }
    }

    /// Spawns a 3D line list (axes, grids) from world-space points.
    /// If `colors` is Some and length matches `points`, per-vertex colors are used.
    pub fn spawn_line_list(&mut self, points: Vec<[f32; 3]>, color: Color) -> MobjectRef {
        self.spawn_lines(points, color, None, false)
    }

    /// Spawns a 3D line list with optional per-vertex colors (colormap).
    pub fn spawn_line_list_with_colors(
        &mut self,
        points: Vec<[f32; 3]>,
        color: Color,
        colors: Option<Vec<[f32; 4]>>,
    ) -> MobjectRef {
        self.spawn_lines(points, color, colors, false)
    }

    pub fn spawn_line_strip(&mut self, points: Vec<[f32; 3]>, color: Color) -> MobjectRef {
        self.spawn_lines(points, color, None, true)
    }

    pub fn spawn_line_strip_with_colors(
        &mut self,
        points: Vec<[f32; 3]>,
        color: Color,
        colors: Option<Vec<[f32; 4]>>,
    ) -> MobjectRef {
        self.spawn_lines(points, color, colors, true)
    }

    fn spawn_lines(
        &mut self,
        points: Vec<[f32; 3]>,
        color: Color,
        colors: Option<Vec<[f32; 4]>>,
        strip: bool,
    ) -> MobjectRef {
        let id = self.next_id();
        let mut min = gaanim_core::glam::DVec3::splat(f64::INFINITY);
        let mut max = gaanim_core::glam::DVec3::splat(f64::NEG_INFINITY);
        for v in &points {
            let p = gaanim_core::glam::DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64);
            min = min.min(p);
            max = max.max(p);
        }
        if min.x == f64::INFINITY {
            min = gaanim_core::glam::DVec3::ZERO;
            max = gaanim_core::glam::DVec3::ZERO;
        }
        let bounds = Bounds3D::new(min, max);
        let line_data = LineListData {
            points,
            indices: None,
            strip,
            color,
            colors,
        };
        let entity = self
            .commands
            .spawn((
                MobjectId(id),
                SpatialTransform::default(),
                GlobalSpatialTransform::default(),
                LocalBounds(bounds),
                WorldBounds::default(),
                line_data.clone(),
                gaanim_scene::LineListSource(line_data),
                Mesh3DMarker,
                Transform::default(),
                GlobalTransform::default(),
                Visibility::default(),
                Visible,
                Opacity(1.0),
                gaanim_scene::GlobalOpacity(1.0),
            ))
            .id();
        self.tag_entity(entity);
        let state = MobjectState {
            path: std::sync::Arc::new(kurbo::BezPath::new()),
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: None,
            stroke: StrokeBrush::transparent(),
            entity,
            child_spans: Vec::new(),
            children: Vec::new(),
            parent: None,
            exclude_from_parent_draw: false,
        };
        self.states.insert(id, state);
        MobjectRef { id }
    }

    /// Spawns a curved arrow primitive.
    pub fn curved_arrow(
        &mut self,
        start: kurbo::Point,
        end: kurbo::Point,
        angle: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::curved_arrow(id, start, end, angle);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a curved arrow from an explicit circular arc.
    ///
    /// The arrow tip is placed at `start_angle + sweep_angle`, allowing the
    /// same center/radius parameters to be shared with a circle or tracker.
    pub fn curved_arrow_arc(
        &mut self,
        center: kurbo::Point,
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::curved_arrow_arc(
            id,
            center,
            radius,
            start_angle,
            sweep_angle,
        );
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns an arrow starting at the origin.
    pub fn vector(&mut self, end: kurbo::Point) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        self.arrow(kurbo::Point::new(0.0, 0.0), end)
    }

    /// Spawns a decorative curly brace primitive.
    pub fn brace(
        &mut self,
        start: kurbo::Point,
        end: kurbo::Point,
        height: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::brace(id, start, end, height);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a horizontal or vertical number line with ticks and optional labels.
    pub fn number_line(
        &mut self,
        x_range: (f64, f64, f64),
        include_labels: bool,
        include_ticks: bool,
        vertical: bool,
    ) -> MobjectRef {
        let (min, max, step) = x_range;

        let mut path = kurbo::BezPath::new();
        let tick_len = 8.0;

        if vertical {
            path.move_to(kurbo::Point::new(0.0, min));
            path.line_to(kurbo::Point::new(0.0, max));

            if include_ticks {
                let mut y = (min / step).ceil() * step;
                while y <= max + 1e-9 {
                    path.move_to(kurbo::Point::new(-tick_len / 2.0, y));
                    path.line_to(kurbo::Point::new(tick_len / 2.0, y));
                    y += step;
                }
            }
        } else {
            path.move_to(kurbo::Point::new(min, 0.0));
            path.line_to(kurbo::Point::new(max, 0.0));

            if include_ticks {
                let mut x = (min / step).ceil() * step;
                while x <= max + 1e-9 {
                    path.move_to(kurbo::Point::new(x, -tick_len / 2.0));
                    path.line_to(kurbo::Point::new(x, tick_len / 2.0));
                    x += step;
                }
            }
        }

        let bounds = if vertical {
            Bounds3D::new_2d(-tick_len / 2.0, min, tick_len / 2.0, max)
        } else {
            Bounds3D::new_2d(min, -tick_len / 2.0, max, tick_len / 2.0)
        };

        let line_id = self.next_id();
        let mut line_bundle = MobjectBundle::new(line_id, path, bounds);
        line_bundle.fill = FillBrush(None);
        line_bundle.stroke = StrokeBrush {
            brush: Some(gaanim_core::peniko::Brush::Solid(
                gaanim_core::peniko::Color::WHITE,
            )),
            style: kurbo::Stroke::new(2.0),
        };
        line_bundle.tag = gaanim_scene::ObjectTag(if vertical {
            "VerticalNumberLineBase".into()
        } else {
            "NumberLineBase".into()
        });

        let line_ref = MobjectSpawnBuilder {
            builder: self,
            id: line_id,
            bundle: line_bundle,
            parent_entity: None,
        }
        .spawn();

        let mut children = vec![line_ref];

        if include_labels {
            let mut val = (min / step).ceil() * step;
            while val <= max + 1e-9 {
                let display_val = if val.abs() < 1e-9 { 0.0 } else { val };
                let label_str = format!("{}", display_val);
                let label_ref = self.spawn_text(&label_str, gaanim_text::prelude::TextRole::Body);

                if let Some(state) = self.states.get_mut(label_ref.id) {
                    if vertical {
                        // Number labels are centered on their local origin. Place the
                        // whole bounding box to the left of the tick, rather than
                        // applying a fixed shift that lets wide values cross the axis.
                        let x = -tick_len * 0.5 - 8.0 - state.bounds.width() * 0.5;
                        state.transform = state.transform.shift_2d(x, val);
                    } else {
                        state.transform = state.transform.shift_2d(val, -18.0);
                    }
                    self.commands.entity(state.entity).insert(state.transform);
                }

                children.push(label_ref);
                val += step;
            }
        }

        self.group(&children)
    }

    /// Spawns a pair of coordinate axes: a horizontal x-axis and a vertical y-axis.
    pub fn axes(
        &mut self,
        x_range: (f64, f64, f64),
        y_range: (f64, f64, f64),
        include_labels: bool,
        include_ticks: bool,
    ) -> MobjectRef {
        let x_axis = self.number_line(x_range, include_labels, include_ticks, false);
        let y_axis = self.number_line(y_range, include_labels, include_ticks, true);
        self.group(&[x_axis, y_axis])
    }

    /// Spawns a parametric curve.
    pub fn parametric_curve<F>(
        &mut self,
        t_range: (f64, f64),
        steps: usize,
        f: F,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a>
    where
        F: Fn(f64) -> kurbo::Point,
    {
        let (t_min, t_max) = t_range;
        let mut points = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let t = t_min + (t_max - t_min) * (i as f64 / steps as f64);
            points.push(f(t));
        }
        self.open_path(&points)
    }

    /// Spawns a function graph y = f(x).
    pub fn function_graph<F>(
        &mut self,
        x_range: (f64, f64),
        steps: usize,
        f: F,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a>
    where
        F: Fn(f64) -> f64,
    {
        self.parametric_curve(x_range, steps, |x| kurbo::Point::new(x, f(x)))
    }

    /// Spawns an arrow with a text label placed next to it.
    pub fn labeled_arrow(
        &mut self,
        start: kurbo::Point,
        end: kurbo::Point,
        label: &str,
        spacing: f64,
    ) -> MobjectRef {
        let arrow_ref = self.arrow(start, end).spawn();

        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let len = (dx * dx + dy * dy).sqrt();

        let mut label_pos = kurbo::Point::new((start.x + end.x) * 0.5, (start.y + end.y) * 0.5);
        if len > 1e-6 {
            let nx = -dy / len;
            let ny = dx / len;
            label_pos.x += nx * spacing;
            label_pos.y += ny * spacing;
        }

        let label_ref = self.spawn_text(label, gaanim_text::prelude::TextRole::Body);
        if let Some(state) = self.states.get_mut(label_ref.id) {
            state.transform = state.transform.shift_2d(label_pos.x, label_pos.y);
            self.commands.entity(state.entity).insert(state.transform);
        }

        self.group(&[arrow_ref, label_ref])
    }

    /// Spawns a symmetric star primitive.
    pub fn star(
        &mut self,
        n_points: u32,
        outer_radius: f64,
        inner_radius: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::star(id, n_points, outer_radius, inner_radius);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns an ellipse primitive.
    pub fn ellipse(&mut self, rx: f64, ry: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::ellipse(id, rx, ry);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a tiny dot primitive.
    pub fn dot(&mut self, radius: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::dot(id, radius);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a square primitive.
    pub fn square(&mut self, side_length: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::square(id, side_length);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a checkmark primitive.
    pub fn checkmark(&mut self, size: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::checkmark(id, size);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a directional arrow primitive.
    pub fn arrow(
        &mut self,
        start: kurbo::Point,
        end: kurbo::Point,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::arrow(id, start, end);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a regular polygon primitive.
    pub fn regular_polygon(
        &mut self,
        n_sides: u32,
        radius: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::regular_polygon(id, n_sides, radius);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a dashed line primitive.
    pub fn dashed_line(
        &mut self,
        start: kurbo::Point,
        end: kurbo::Point,
        dash_length: f64,
        gap_length: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle =
            gaanim_objects::primitives::dashed_line(id, start, end, dash_length, gap_length);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a circular/elliptical arc segment primitive.
    pub fn arc(
        &mut self,
        center: kurbo::Point,
        radii: kurbo::Vec2,
        start_angle: f64,
        sweep_angle: f64,
        x_rotation: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::arc(
            id,
            center,
            radii,
            start_angle,
            sweep_angle,
            x_rotation,
        );
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a smooth arc between two points with a given deflection angle.
    pub fn arc_between_points(
        &mut self,
        start: kurbo::Point,
        end: kurbo::Point,
        angle: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::arc_between_points(id, start, end, angle);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a double-headed arrow primitive.
    pub fn double_arrow(
        &mut self,
        start: kurbo::Point,
        end: kurbo::Point,
        head_len: Option<f64>,
        head_width: Option<f64>,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::double_arrow(id, start, end, head_len, head_width);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a circular sector (pie slice) primitive.
    pub fn sector(
        &mut self,
        center: kurbo::Point,
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle =
            gaanim_objects::primitives::sector(id, center, radius, start_angle, sweep_angle);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns an annulus (ring/donut) primitive.
    pub fn annulus(
        &mut self,
        outer_radius: f64,
        inner_radius: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::annulus(id, outer_radius, inner_radius);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a surrounding rectangle (no fill, stroke outline).
    pub fn surrounding_rectangle(
        &mut self,
        width: f64,
        height: f64,
        corner_radius: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle =
            gaanim_objects::primitives::surrounding_rectangle(id, width, height, corner_radius);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a background rectangle (filled, low z-index).
    pub fn background_rectangle(
        &mut self,
        width: f64,
        height: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::background_rectangle(id, width, height);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a cross (X) symbol primitive.
    pub fn cross(&mut self, size: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::cross(id, size);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a right-angle indicator primitive.
    pub fn right_angle(&mut self, arm_length: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::right_angle(id, arm_length);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a mobject from a list of pre-computed closed polylines.
    /// Used by boolean-operation replay to materialize the result geometry.
    pub fn polylines(
        &mut self,
        rings: Vec<Vec<kurbo::Point>>,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::polylines(id, &rings);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Compiles a Typst markup or math formula into a hierarchy of vector Mobjects.
    ///
    /// `text_font` and `math_font` are optional font family names. When `None`,
    /// Typst uses its bundled defaults (LibertinusSerif for text, NewCMMath for math).
    ///
    /// `text_size` and `math_size` are optional sizes in **pt**. When `None`, Typst
    /// uses its default (11pt). For a comfortable canvas size, 24pt–32pt is recommended.
    ///
    /// Returns a reference to the parent container of the compiled formula.
    pub fn typst(
        &mut self,
        source: &str,
        is_math: bool,
        text_font: Option<&str>,
        math_font: Option<&str>,
        text_size: Option<f64>,
        math_size: Option<f64>,
    ) -> MobjectRef {
        let text_font = text_font.or_else(|| (!is_math).then_some("New Computer Modern"));
        let parent_id = self.next_id();
        let style_color = self
            .text_config
            .roles
            .get(&gaanim_text::prelude::TextRole::Body)
            .map(|s| s.fill_color)
            .unwrap_or(gaanim_core::peniko::Color::WHITE);
        let fill = Some(gaanim_core::peniko::Brush::Solid(style_color));
        let stroke = gaanim_scene::StrokeBrush::transparent();

        // Extract mutable counter separately to avoid borrow conflict with `self.commands`.
        let id_counter = &mut self.id_counter;
        let next_id_fn = move || {
            let id = gaanim_core::ObjectId::from_parts(*id_counter, 1);
            *id_counter += 1;
            id
        };

        let mut child_spans = Vec::new();
        let (entity, bounds, metrics) = compile_typst_to_hierarchy(
            self.commands,
            self.font_registry,
            source,
            is_math,
            text_font,
            math_font,
            text_size,
            math_size,
            fill.clone(),
            stroke.clone(),
            parent_id,
            next_id_fn,
            &mut child_spans,
        );
        self.register_textual_hierarchy(
            parent_id,
            entity,
            bounds,
            fill,
            stroke,
            child_spans,
            Some(metrics),
        )
    }

    /// Convenience wrapper for `typst` that uses explicit fonts for both text and math.
    pub fn typst_with_fonts(
        &mut self,
        source: &str,
        is_math: bool,
        text_font: &str,
        math_font: &str,
    ) -> MobjectRef {
        self.typst(
            source,
            is_math,
            Some(text_font),
            Some(math_font),
            None,
            None,
        )
    }

    /// Compiles a plain text string into a hierarchy of vector character Mobjects.
    ///
    /// Shapes the text using HarfBuzz (`rustybuzz`) and extracts outlines via `ttf-parser`.
    /// `font_family` is the font name (e.g. "New Computer Modern", "sans-serif").
    /// `font_size` is the text size in pixels/points.
    ///
    /// Returns a reference to the parent container of the text.
    pub fn text(&mut self, content: &str, font_family: &str, font_size: f64) -> MobjectRef {
        let parent_id = self.next_id();
        let fill = Some(gaanim_core::peniko::Brush::Solid(
            gaanim_core::peniko::Color::WHITE,
        ));
        let stroke = gaanim_scene::StrokeBrush::transparent();

        // Extract mutable counter separately to avoid borrow conflict with `self.commands`.
        let id_counter = &mut self.id_counter;
        let next_id_fn = move || {
            let id = gaanim_core::ObjectId::from_parts(*id_counter, 1);
            *id_counter += 1;
            id
        };

        let mut child_spans = Vec::new();
        let (entity, bounds) = match compile_text_to_hierarchy(
            self.commands,
            self.font_registry,
            content,
            font_family,
            font_size,
            fill.clone(),
            stroke.clone(),
            parent_id,
            next_id_fn,
            &mut child_spans,
        ) {
            Ok(res) => res,
            Err(e) => {
                bevy::prelude::error!("Text compilation failed: {}", e);
                let bounds = Bounds3D::default();
                let bundle = MobjectBundle::new(parent_id, kurbo::BezPath::new(), bounds);
                let entity = self.commands.spawn(bundle).id();
                (entity, bounds)
            }
        };
        self.register_textual_hierarchy(
            parent_id,
            entity,
            bounds,
            Some(gaanim_core::peniko::Brush::Solid(
                gaanim_core::peniko::Color::WHITE,
            )),
            gaanim_scene::StrokeBrush::transparent(),
            child_spans,
            None,
        )
    }

    /// Spawns a vector text Mobject using the default styling of the requested `TextRole`.
    pub fn spawn_text(
        &mut self,
        content: &str,
        role: gaanim_text::prelude::TextRole,
    ) -> MobjectRef {
        let style = self
            .text_config
            .roles
            .get(&role)
            .cloned()
            .unwrap_or_else(|| gaanim_text::prelude::RoleStyle {
                font_family: "New Computer Modern".to_string(),
                size: 32.0,
                fill_color: gaanim_core::peniko::Color::WHITE,
            });

        let has_inline_math = crate::canvas::split_text_math(content)
            .iter()
            .any(|(is_math, c)| *is_math && !c.trim().is_empty());

        if has_inline_math {
            let source = crate::canvas::text_inline_typst_source(content, style.fill_color);
            return self.typst(
                &source,
                false,
                Some(&style.font_family),
                None,
                Some(style.size),
                None,
            );
        }

        let parent_id = self.next_id();
        let fill = Some(gaanim_core::peniko::Brush::Solid(style.fill_color));
        let stroke = gaanim_scene::StrokeBrush::transparent();

        let id_counter = &mut self.id_counter;
        let next_id_fn = move || {
            let id = gaanim_core::ObjectId::from_parts(*id_counter, 1);
            *id_counter += 1;
            id
        };

        let mut child_spans = Vec::new();
        let (entity, bounds) = match compile_text_to_hierarchy(
            self.commands,
            self.font_registry,
            content,
            &style.font_family,
            style.size,
            fill.clone(),
            stroke.clone(),
            parent_id,
            next_id_fn,
            &mut child_spans,
        ) {
            Ok(res) => res,
            Err(e) => {
                bevy::prelude::error!("Text compilation failed: {}", e);
                let bounds = Bounds3D::default();
                let bundle = MobjectBundle::new(parent_id, kurbo::BezPath::new(), bounds);
                let entity = self.commands.spawn(bundle).id();
                (entity, bounds)
            }
        };
        let result = self.register_textual_hierarchy(
            parent_id,
            entity,
            bounds,
            Some(gaanim_core::peniko::Brush::Solid(style.fill_color)),
            gaanim_scene::StrokeBrush::transparent(),
            child_spans,
            None,
        );
        self.mobject_names
            .insert(parent_id, format!("Text('{}')", content));
        result
    }

    /// Spawns a reactive DecimalNumber Mobject that displays and updates according to a ValueTracker signal.
    pub fn decimal_number(
        &mut self,
        signal_ref: ValueTrackerRef,
        num_decimals: usize,
        prefix: &str,
        suffix: &str,
        font_family: &str,
        font_size: f64,
    ) -> MobjectRef {
        let parent_id = self.next_id();
        let fill = Some(gaanim_core::peniko::Brush::Solid(
            gaanim_core::peniko::Color::WHITE,
        ));
        let stroke = gaanim_scene::StrokeBrush::transparent();

        let signal_entity_bevy = match self.states.get(signal_ref.id) {
            Some(state) => state.entity,
            None => {
                bevy::prelude::warn!("ValueTrackerRef id {:?} not found", signal_ref.id);
                let bundle =
                    MobjectBundle::new(parent_id, kurbo::BezPath::new(), Bounds3D::default());
                self.commands.spawn(bundle);
                return MobjectRef { id: parent_id };
            }
        };

        let initial_val = self
            .float_signals
            .get(&signal_ref.id)
            .cloned()
            .unwrap_or(0.0);
        let text = format!(
            "{}{:.width$}{}",
            prefix,
            initial_val,
            suffix,
            width = num_decimals
        );

        let (path, bounds) = match gaanim_text::shaper::compile_text_to_path(
            self.font_registry,
            &text,
            font_family,
            font_size,
        ) {
            Ok(res) => res,
            Err(e) => {
                bevy::prelude::error!("DecimalNumber initial text compilation failed: {}", e);
                (kurbo::BezPath::new(), Bounds3D::default())
            }
        };

        let mut bundle = MobjectBundle::new(parent_id, path, bounds);
        bundle.fill = gaanim_scene::FillBrush(fill.clone());
        bundle.stroke = stroke.clone();
        bundle.tag = gaanim_scene::ObjectTag(format!("DecimalNumber({})", text));

        let entity = self
            .commands
            .spawn(bundle)
            .insert(crate::DecimalNumber {
                signal_entity: signal_entity_bevy,
                num_decimals,
                prefix: prefix.to_string(),
                suffix: suffix.to_string(),
                font_family: font_family.to_string(),
                font_size,
                last_value: Some(initial_val),
            })
            .id();

        self.tag_entity(entity);

        let state = MobjectState {
            path: std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill,
            stroke,
            entity,
            child_spans: Vec::new(),
            children: Vec::new(),
            parent: None,
            exclude_from_parent_draw: false,
        };
        self.states.insert(parent_id, state);
        self.mobject_names
            .insert(parent_id, format!("DecimalNumber('{}')", text));

        MobjectRef { id: parent_id }
    }

    /// Selects a subset of characters/shapes in a text or equation Mobject by exact substring.
    /// Matching is case-insensitive, whitespace-insensitive, and tolerant of
    /// mathematical styling. If a literal query has no matches, Typst math
    /// identifiers, shorthands, and primes are resolved to their rendered
    /// Unicode representation through Typst's parser and Codex.
    pub fn select<'q>(
        &'q mut self,
        target: MobjectRef,
        substring: &str,
    ) -> MobjectSelection<'q, 'w, 's, 'a> {
        self.select_occurrence(target, substring, None)
    }

    /// Selects the zero-based occurrence of a substring. Passing `None`
    /// selects every non-overlapping occurrence. Literal matches take
    /// precedence over the Typst/Codex fallback.
    pub fn select_occurrence<'q>(
        &'q mut self,
        target: MobjectRef,
        substring: &str,
        occurrence: Option<usize>,
    ) -> MobjectSelection<'q, 'w, 's, 'a> {
        let mut child_ids = Vec::new();
        if let Some(state) = self.states.get(target.id) {
            let mut normalized_text = String::new();
            let mut index_mapping = Vec::new();

            for (span_idx, child) in state.child_spans.iter().enumerate() {
                let raw_c = child.span.character;
                if raw_c.is_whitespace() || raw_c == '^' || raw_c == '_' {
                    continue;
                }

                for lower in standard_math_char(raw_c).to_lowercase() {
                    let start_byte = normalized_text.len();
                    normalized_text.push(lower);
                    let end_byte = normalized_text.len();
                    for _ in start_byte..end_byte {
                        index_mapping.push(span_idx);
                    }
                }
            }

            let literal_query = normalize_text_selection(substring);
            let mut matches =
                normalized_match_spans(&normalized_text, &index_mapping, &literal_query);
            if matches.is_empty() {
                let typst_source = typst_math_selection_source(substring);
                let typst_query = normalize_text_selection(&typst_source);
                if typst_query != literal_query {
                    matches =
                        normalized_match_spans(&normalized_text, &index_mapping, &typst_query);
                }
            }

            let mut append_match = |span_indices: &[usize]| {
                for &span_idx in span_indices {
                    if let Some(child) = state.child_spans.get(span_idx) {
                        child_ids.push(child.id);
                    }
                }
            };
            if let Some(index) = occurrence {
                if let Some(span_indices) = matches.get(index) {
                    append_match(span_indices);
                }
            } else {
                for span_indices in &matches {
                    append_match(span_indices);
                }
            }
        }

        MobjectSelection {
            builder: self,
            parent_id: target.id,
            child_ids,
        }
    }

    /// Selects a subset of characters/shapes in a text or equation Mobject by a custom closure predicate.
    pub fn select_by<'q, F>(
        &'q mut self,
        target: MobjectRef,
        predicate: F,
    ) -> MobjectSelection<'q, 'w, 's, 'a>
    where
        F: Fn(&gaanim_scene::components::TextSpan) -> bool,
    {
        let mut child_ids = Vec::new();
        if let Some(state) = self.states.get(target.id) {
            for child in &state.child_spans {
                if predicate(&child.span) {
                    child_ids.push(child.id);
                }
            }
        }

        MobjectSelection {
            builder: self,
            parent_id: target.id,
            child_ids,
        }
    }

    /// Selects a subset of characters/shapes in a text or equation Mobject by a sequential character range.
    pub fn select_range<'q>(
        &'q mut self,
        target: MobjectRef,
        range: std::ops::Range<usize>,
    ) -> MobjectSelection<'q, 'w, 's, 'a> {
        let mut child_ids = Vec::new();
        if let Some(state) = self.states.get(target.id) {
            for child in &state.child_spans {
                if child.span.char_index >= range.start && child.span.char_index < range.end {
                    child_ids.push(child.id);
                }
            }
        }

        MobjectSelection {
            builder: self,
            parent_id: target.id,
            child_ids,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::world::CommandQueue;
    use bevy::prelude::World;

    fn square_path(offset: f64) -> std::sync::Arc<gaanim_core::kurbo::BezPath> {
        let mut path = gaanim_core::kurbo::BezPath::new();
        path.move_to((offset, 0.0));
        path.line_to((offset + 10.0, 0.0));
        path.line_to((offset + 10.0, 10.0));
        path.line_to((offset, 10.0));
        path.close_path();
        std::sync::Arc::new(path)
    }

    fn hierarchy_child(
        id: ObjectId,
        entity: Entity,
        path: std::sync::Arc<gaanim_core::kurbo::BezPath>,
        character: char,
    ) -> HierarchyChild {
        HierarchyChild {
            id,
            entity,
            span: gaanim_scene::components::TextSpan {
                character,
                char_index: 0,
                source_range: (0..character.len_utf8()).into(),
            },
            path,
            bounds: Bounds3D::default(),
            transform: SpatialTransform::default(),
            fill: Some(Brush::Solid(Color::WHITE)),
            stroke: StrokeBrush::transparent(),
        }
    }

    fn hierarchy_state(
        entity: Entity,
        path: std::sync::Arc<gaanim_core::kurbo::BezPath>,
        children: Vec<HierarchyChild>,
    ) -> MobjectState {
        MobjectState {
            path,
            bounds: Bounds3D::default(),
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: Some(Brush::Solid(Color::WHITE)),
            stroke: StrokeBrush::transparent(),
            entity,
            children: children.iter().map(|child| child.id).collect(),
            child_spans: children,
            parent: None,
            exclude_from_parent_draw: false,
        }
    }

    #[test]
    fn adaptive_lag_ratio_matches_manim_formula() {
        assert!((adaptive_lag_ratio(1) - 0.2).abs() < f64::EPSILON);
        assert!((adaptive_lag_ratio(2) - 0.2).abs() < f64::EPSILON);
        assert!((adaptive_lag_ratio(40) - 0.1).abs() < 1e-9);
    }

    #[test]
    fn equation_residual_collapses_into_its_visual_center() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let state = hierarchy_state(entity, square_path(40.0), Vec::new());
        let collapsed = SceneBuilder::equation_center_collapsed_transform(&state);
        let visual_center = gaanim_core::kurbo::Point::new(45.0, 5.0);
        let before = state.transform.to_affine_2d() * visual_center;
        let after = collapsed.to_affine_2d() * visual_center;

        assert_eq!(collapsed.scale, DVec3::ZERO);
        assert!((before.x - after.x).abs() < 1e-9);
        assert!((before.y - after.y).abs() < 1e-9);
        assert_eq!(after, visual_center);
    }

    #[test]
    fn typst_selection_source_uses_codex_and_typst_syntax() {
        assert_eq!(typst_math_selection_source("g sin(theta)"), "g sin(θ)");
        assert_eq!(typst_math_selection_source("theta''"), "θ″");
        assert_eq!(typst_math_selection_source("sum F_t"), "∑ F_t");
        assert_eq!(typst_math_selection_source("arrow.r.long"), "⟶");
        assert_eq!(typst_math_selection_source("a <= b"), "a ≤ b");
        assert_eq!(typst_math_selection_source("sin(x)"), "sin(x)");
    }

    #[test]
    fn typst_selection_resolves_semantic_source_that_renders_as_symbols() {
        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);
        let source = "$ - m text(fill: #rgb(\"ffd700\"), g sin(theta)) = m L theta'' $";
        let equation = builder.typst(source, false, None, None, Some(32.0), None);

        let gravity = builder
            .select_occurrence(equation, "g sin(theta)", Some(0))
            .child_ids;
        let acceleration = builder
            .select_occurrence(equation, "theta''", Some(0))
            .child_ids;
        let state = builder
            .states
            .get(equation.id)
            .expect("compiled equation state");
        let rendered = state
            .child_spans
            .iter()
            .map(|child| child.span.character)
            .collect::<String>();
        assert!(
            !gravity.is_empty(),
            "semantic Typst source must resolve even when theta renders as θ; rendered={rendered:?}"
        );
        assert!(
            !acceleration.is_empty(),
            "Typst prime syntax must resolve to its rendered Unicode glyph; rendered={rendered:?}"
        );
        assert!(gravity.len() < state.child_spans.len());
        assert!(acceleration.len() < state.child_spans.len());
        assert!(gravity.iter().all(|id| !acceleration.contains(id)));

        let symbols = builder.typst(
            "$sum F_t arrow.r.long theta.alt$",
            false,
            None,
            None,
            Some(32.0),
            None,
        );
        assert!(
            !builder
                .select_occurrence(symbols, "sum F_t", Some(0))
                .child_ids
                .is_empty()
        );
        assert!(
            !builder
                .select_occurrence(symbols, "arrow.r.long", Some(0))
                .child_ids
                .is_empty()
        );
        assert!(
            !builder
                .select_occurrence(symbols, "theta.alt", Some(0))
                .child_ids
                .is_empty()
        );
    }

    #[test]
    fn typst_selection_preserves_literal_precedence_and_occurrence() {
        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

        let repeated = builder.typst("$theta + theta$", false, None, None, Some(32.0), None);
        let first = builder
            .select_occurrence(repeated, "theta", Some(0))
            .child_ids;
        let second = builder
            .select_occurrence(repeated, "theta", Some(1))
            .child_ids;
        assert!(!first.is_empty());
        assert!(!second.is_empty());
        assert!(first.iter().all(|id| !second.contains(id)));

        let mixed = builder.typst(
            "#text(\"theta\") $theta$",
            false,
            None,
            None,
            Some(32.0),
            None,
        );
        let literal = builder.select(mixed, "theta").child_ids;
        let state = builder.states.get(mixed.id).expect("compiled mixed text");
        let selected_text = literal
            .iter()
            .filter_map(|id| {
                state
                    .child_spans
                    .iter()
                    .find(|child| child.id == *id)
                    .map(|child| child.span.character)
            })
            .collect::<String>();
        assert_eq!(selected_text, "theta");
    }

    #[test]
    fn indicate_hops_up_from_visual_center_without_diagonal_drift() {
        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

        let parent_id = builder.next_id();
        let target_id = builder.next_id();
        let entity = builder.commands.spawn_empty().id();
        builder.states.insert(
            target_id,
            MobjectState {
                path: square_path(40.0),
                // This mimics a glyph whose bounds have already been centered
                // relative to its textual parent, while its path remains local.
                bounds: Bounds3D::new_2d(0.0, 0.0, 10.0, 10.0),
                transform: SpatialTransform::identity(),
                opacity: 1.0,
                fill: Some(Brush::Solid(Color::WHITE)),
                stroke: StrokeBrush::transparent(),
                entity,
                child_spans: Vec::new(),
                children: Vec::new(),
                parent: Some(parent_id),
                exclude_from_parent_draw: false,
            },
        );

        builder.play(AnimationBuilder {
            target: target_id,
            anim_type: AnimationType::Indicate {
                color: None,
                scale_factor: 1.1,
            },
            duration: 1.0,
            delay: 0.0,
            rate_func: gaanim_math::RateFunc::ThereAndBack,
        });

        let state = builder.states.get(target_id).unwrap();
        assert_eq!(state.transform.anchor, DVec3::new(45.0, 5.0, 0.0));

        let translations: Vec<_> = builder
            .timeline
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                ClipPayload::Animation(AnimationSpec {
                    target,
                    lens: PropertyLensSpec::Translation { from, to },
                    ..
                }) if *target == target_id => Some((*from, *to)),
                _ => None,
            })
            .collect();
        assert_eq!(translations.len(), 2);
        assert_eq!(translations[0].0.x, translations[0].1.x);
        assert!(translations[0].1.y > translations[0].0.y);
        assert_eq!(translations[1].0, translations[0].1);
        assert_eq!(translations[1].1, translations[0].0);

        let scales: Vec<_> = builder
            .timeline
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                ClipPayload::Animation(AnimationSpec {
                    target,
                    lens: PropertyLensSpec::Scale { to, .. },
                    ..
                }) if *target == target_id => Some(*to),
                _ => None,
            })
            .collect();
        assert!(scales.iter().any(|scale| (scale.x - 1.1).abs() < 1e-9));
    }

    #[test]
    fn hierarchical_transform_flattens_once_and_preserves_source_identity() {
        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

        let source_id = builder.next_id();
        let source_child_id = builder.next_id();
        let target_id = builder.next_id();
        let target_child_id = builder.next_id();
        let source_entity = builder.commands.spawn_empty().id();
        let source_child_entity = builder.commands.spawn_empty().id();
        let target_entity = builder.commands.spawn_empty().id();
        let target_child_entity = builder.commands.spawn_empty().id();
        let source_path = square_path(0.0);
        let target_path = square_path(40.0);
        let source_child = hierarchy_child(
            source_child_id,
            source_child_entity,
            source_path.clone(),
            'A',
        );
        let target_child = hierarchy_child(
            target_child_id,
            target_child_entity,
            target_path.clone(),
            '∑',
        );

        builder.states.insert(
            source_child_id,
            hierarchy_state(source_child_entity, source_path.clone(), Vec::new()),
        );
        builder.states.insert(
            target_child_id,
            hierarchy_state(target_child_entity, target_path.clone(), Vec::new()),
        );
        builder.states.insert(
            source_id,
            hierarchy_state(source_entity, source_path, vec![source_child]),
        );
        builder.states.insert(
            target_id,
            hierarchy_state(target_entity, target_path.clone(), vec![target_child]),
        );

        builder.play(AnimationBuilder {
            target: source_id,
            anim_type: AnimationType::Transform { target: target_id },
            duration: 1.0,
            delay: 0.0,
            rate_func: gaanim_math::RateFunc::Linear,
        });

        let source_after = builder.states.get(source_id).expect("source state");
        assert_eq!(source_after.entity, source_entity);
        assert_eq!(source_after.path, target_path);
        assert!(source_after.child_spans.is_empty());

        let animated_path_targets: Vec<_> = builder
            .timeline
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                ClipPayload::Animation(AnimationSpec {
                    target,
                    lens: PropertyLensSpec::PathMorph { .. },
                    ..
                }) if clip.duration > 0.0 => Some(*target),
                _ => None,
            })
            .collect();
        assert_eq!(animated_path_targets, vec![source_id]);
        assert!(!animated_path_targets.contains(&source_child_id));
        assert!(!animated_path_targets.contains(&target_child_id));

        let synthesized_stroke = builder.timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                ClipPayload::Animation(AnimationSpec {
                    target,
                    lens: PropertyLensSpec::StrokeColor { .. }
                        | PropertyLensSpec::StrokeWidth { .. },
                    ..
                }) if *target == source_id
            )
        });
        assert!(
            !synthesized_stroke,
            "no-stroke text/math morph must not create a white outline"
        );
    }

    #[test]
    fn write_keeps_synthesized_outline_temporary() {
        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);
        let id = builder.next_id();
        let entity = builder.commands.spawn_empty().id();
        builder
            .states
            .insert(id, hierarchy_state(entity, square_path(0.0), Vec::new()));

        builder.play(AnimationBuilder {
            target: id,
            anim_type: AnimationType::Write {
                config: crate::anim::DrawAnimationConfig::default(),
            },
            duration: 1.0,
            delay: 0.0,
            rate_func: gaanim_math::RateFunc::Linear,
        });

        assert!(builder.states.get(id).unwrap().stroke.brush.is_none());
        assert!(builder.timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                ClipPayload::Animation(AnimationSpec {
                    target,
                    lens: PropertyLensSpec::StrokeWidth { to, .. },
                    ..
                }) if *target == id && *to == 0.0
            )
        }));
    }

    #[test]
    fn unstyled_group_preserves_child_fill_after_style_propagation() {
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

        let gold = Color::from_rgb8(0xff, 0xd7, 0x00);
        let child = builder.rectangle(40.0, 30.0).fill(gold).spawn();
        let child_entity = builder.states.get(child.id).unwrap().entity;
        let group = builder.group(&[child]);
        let group_entity = builder.states.get(group.id).unwrap().entity;

        drop(builder);
        drop(commands);
        queue.apply(&mut world);

        let expected = FillBrush::color(gold);
        assert_eq!(world.get::<FillBrush>(child_entity), Some(&expected));
        assert!(world.get::<FillBrush>(group_entity).is_none());

        let mut schedule = bevy::prelude::Schedule::default();
        schedule.add_systems(gaanim_scene::systems::style_propagation_system);
        schedule.run(&mut world);

        assert_eq!(world.get::<FillBrush>(child_entity), Some(&expected));
    }

    #[test]
    fn group_preserves_the_authored_child_coordinate_frame() {
        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

        let child = builder.rectangle(40.0, 30.0).at(120.0, -45.0).spawn();
        let authored = builder.states.get(child.id).unwrap().transform;
        let group = builder.group(&[child]);

        assert_eq!(builder.states.get(child.id).unwrap().transform, authored);
        assert_eq!(builder.get_world_transform(child.id), authored);
        assert_eq!(
            builder.states.get(group.id).unwrap().transform.to_mat4(),
            gaanim_core::glam::DMat4::IDENTITY,
            "creating a group must not replace the coordinate system used by child updaters"
        );
    }

    #[test]
    fn remove_from_group_preserves_world_transform_and_visibility() {
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

        let child = builder.rectangle(40.0, 30.0).at(120.0, -45.0).spawn();
        let child_entity = builder.states.get(child.id).unwrap().entity;
        let group = builder.group(&[child]);
        let before = builder.get_world_transform(child.id);

        builder.remove_from_group(group, child);

        let child_state = builder.states.get(child.id).unwrap();
        assert_eq!(child_state.parent, None);
        assert_eq!(child_state.transform, before);
        assert_eq!(child_state.opacity, 1.0);
        assert!(
            !builder
                .states
                .get(group.id)
                .unwrap()
                .children
                .contains(&child.id)
        );

        drop(builder);
        drop(commands);
        queue.apply(&mut world);
        assert!(
            world
                .get::<bevy::ecs::hierarchy::ChildOf>(child_entity)
                .is_none()
        );
        assert_eq!(world.get::<Opacity>(child_entity), Some(&Opacity(1.0)));
    }
}

/// Helper structure providing fluent configuration for Mobjects before spawning them.
pub struct MobjectSpawnBuilder<'b, 'w, 's, 'a> {
    pub builder: &'b mut SceneBuilder<'w, 's, 'a>,
    pub id: ObjectId,
    pub bundle: MobjectBundle,
    pub parent_entity: Option<Entity>,
}

impl<'b, 'w, 's, 'a> MobjectSpawnBuilder<'b, 'w, 's, 'a> {
    pub fn fill(mut self, color: Color) -> Self {
        self.bundle.fill = FillBrush(Some(Brush::Solid(color)));
        self
    }

    pub fn fill_brush(mut self, brush: Brush) -> Self {
        self.bundle.fill = FillBrush(Some(brush));
        self
    }

    pub fn no_fill(mut self) -> Self {
        self.bundle.fill = FillBrush(None);
        self
    }

    pub fn stroke(mut self, color: Color, width: f64) -> Self {
        self.bundle.stroke = StrokeBrush {
            brush: Some(Brush::Solid(color)),
            style: kurbo::Stroke::new(width),
        };
        self
    }

    pub fn stroke_brush(mut self, brush: Brush, width: f64) -> Self {
        self.bundle.stroke = StrokeBrush {
            brush: Some(brush),
            style: kurbo::Stroke::new(width),
        };
        self
    }

    pub fn stroke_with_style(mut self, brush: Brush, style: kurbo::Stroke) -> Self {
        self.bundle.stroke = StrokeBrush {
            brush: Some(brush),
            style,
        };
        self
    }

    pub fn no_stroke(mut self) -> Self {
        self.bundle.stroke = StrokeBrush::transparent();
        self
    }

    pub fn transform(mut self, transform: SpatialTransform) -> Self {
        self.bundle.transform = transform;
        self
    }

    pub fn translate(mut self, x: f64, y: f64) -> Self {
        self.bundle.transform = self.bundle.transform.shift_2d(x, y);
        self
    }

    pub fn scale(mut self, s: f64) -> Self {
        self.bundle.transform = self.bundle.transform.scale_uniform(s);
        self
    }

    pub fn rotate(mut self, radians: f64) -> Self {
        self.bundle.transform = self.bundle.transform.with_rotation_2d(radians);
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.bundle.opacity = Opacity(opacity);
        self
    }

    pub fn z_index(mut self, z: i32) -> Self {
        self.bundle.render_order.z_index = z;
        self
    }

    /// Positions this object adjacent to a reference object with Direction and Anchor support.
    pub fn next_to_new(
        mut self,
        reference: MobjectRef,
        direction: Direction,
        spacing: f64,
        aligned_edge: Anchor,
    ) -> Self {
        if let Some(ref_state) = self.builder.states.get(reference.id) {
            let shift = gaanim_layout::compute_next_to_new(
                self.bundle.bounds.0,
                &self.bundle.transform,
                ref_state.bounds,
                &ref_state.transform,
                direction,
                spacing,
                aligned_edge,
            );
            self.bundle.transform = self.bundle.transform.shift_3d(shift);
        }
        self
    }

    /// Aligns target_anchor on this object with ref_anchor on the reference object.
    pub fn align_to_new(
        mut self,
        reference: MobjectRef,
        target_anchor: Anchor,
        ref_anchor: Anchor,
    ) -> Self {
        if let Some(ref_state) = self.builder.states.get(reference.id) {
            let shift = gaanim_layout::compute_align_to_new(
                self.bundle.bounds.0,
                &self.bundle.transform,
                ref_state.bounds,
                &ref_state.transform,
                target_anchor,
                ref_anchor,
            );
            self.bundle.transform = self.bundle.transform.shift_3d(shift);
        }
        self
    }

    /// Position the object so that the specified anchor is at (x, y).
    pub fn at_anchor(mut self, x: f64, y: f64, anchor: Anchor) -> Self {
        self.bundle.transform = gaanim_layout::compute_move_to(
            self.bundle.bounds.0,
            &self.bundle.transform,
            gaanim_core::glam::DVec3::new(x, y, 0.0),
            anchor,
        );
        self
    }

    /// Position the object so its center is at (x, y). Default center-based positioning.
    pub fn at(self, x: f64, y: f64) -> Self {
        self.at_anchor(x, y, Anchor::Center)
    }

    /// Position at screen edge with buffer spacing.
    pub fn to_edge(mut self, direction: Direction, buff: f64) -> Self {
        let frame_bounds = Bounds3D::new(
            gaanim_core::glam::DVec3::new(-640.0, -360.0, 0.0),
            gaanim_core::glam::DVec3::new(640.0, 360.0, 0.0),
        );
        self.bundle.transform = gaanim_layout::compute_to_edge(
            self.bundle.bounds.0,
            &self.bundle.transform,
            direction,
            buff,
            frame_bounds,
        );
        self
    }

    /// Position at screen corner with buffer spacing.
    pub fn to_corner(mut self, corner: Anchor, buff: f64) -> Self {
        let frame_bounds = Bounds3D::new(
            gaanim_core::glam::DVec3::new(-640.0, -360.0, 0.0),
            gaanim_core::glam::DVec3::new(640.0, 360.0, 0.0),
        );
        self.bundle.transform = gaanim_layout::compute_to_corner(
            self.bundle.bounds.0,
            &self.bundle.transform,
            corner,
            buff,
            frame_bounds,
        );
        self
    }

    /// Establishes parent-child relationship via Bevy hierarchy systems.
    pub fn parent(mut self, parent: MobjectRef) -> Self {
        if let Some(parent_state) = self.builder.states.get(parent.id) {
            self.parent_entity = Some(parent_state.entity);
        }
        self
    }

    /// Finalizes the setup, spawning the Bevy ECS bundle and recording its tracked hot state in the SceneBuilder.
    pub fn spawn(self) -> MobjectRef {
        self.spawn_with_effects(None, None, None)
    }

    pub(crate) fn spawn_with_effects(
        self,
        glow: Option<gaanim_renderer::effects::Glow>,
        blur: Option<gaanim_renderer::effects::GaussianBlur>,
        shadow: Option<gaanim_renderer::effects::DropShadow>,
    ) -> MobjectRef {
        let mut entity_cmd = self.builder.commands.spawn(self.bundle.clone());
        let entity = entity_cmd.id();

        if let Some(parent) = self.parent_entity {
            entity_cmd.set_parent_in_place(parent);
        }
        if let Some(glow) = glow {
            entity_cmd.insert(glow);
        }
        if let Some(blur) = blur {
            entity_cmd.insert(blur);
        }
        if let Some(shadow) = shadow {
            entity_cmd.insert(shadow);
        }

        // Tag entity with the current scene if inside a scene scope
        if let Some(scene_id) = self.builder.current_scene {
            self.builder
                .commands
                .entity(entity)
                .insert(SceneMember(scene_id));
        }

        let state = MobjectState {
            path: self.bundle.path.0.clone(),
            bounds: self.bundle.bounds.0,
            transform: self.bundle.transform,
            opacity: self.bundle.opacity.0,
            fill: self.bundle.fill.0.clone(),
            stroke: self.bundle.stroke.clone(),
            entity,
            child_spans: Vec::new(),
            children: Vec::new(),
            parent: None,
            exclude_from_parent_draw: false,
        };
        self.builder.states.insert(self.id, state);
        self.builder
            .mobject_names
            .insert(self.id, self.bundle.tag.0.clone());

        MobjectRef { id: self.id }
    }
}
