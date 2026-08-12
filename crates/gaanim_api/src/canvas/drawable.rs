//! DrawableHandle — ergonomic mobject handle with fluent configuration.

use std::collections::HashMap;
use std::sync::Arc;

use gaanim_animation::AxisMask;
use gaanim_core::ObjectId;
use gaanim_core::glam::{DQuat, DVec2, DVec3, EulerRot};
use gaanim_core::kurbo::BezPath;
use gaanim_core::peniko::{Brush, Color};
use gaanim_layout::{Anchor, Direction};
use gaanim_objects::prelude::GltfAnimationMetadata;

use crate::anim::{AnimationType, DrawAnimationConfig, TextSelectionEffect};
use crate::canvas::ops::{
    FragmentRevealStyle, Op, SharedCanvasState, SharedObjectSpec, UpdaterPreset,
};
use crate::canvas::types::{Anim, LayoutOp, ObjectSpec, OptDuration, SpawnKind};

/// An ergonomic handle to a mobject on a Canvas.
///
/// - Instant setters return `Self` (fluent): `obj.fill(RED).at(0,0)`.
/// - Animation methods return `Anim` and auto-enqueue the animation on the
///   active segment's sequential track. They accept an optional duration:
///     - `obj.fade_in()`       — default 1.0s
///     - `obj.fade_in(2.0)`    — 2.0s
///     - `obj.fade_in(None)`   — default 1.0s (explicit)
///   You can still chain `.duration()` or any other `Anim` method after.
#[derive(Debug, Clone)]
pub struct DrawableHandle {
    pub id: ObjectId,
    pub(crate) spec: SharedObjectSpec,
    pub(crate) state: SharedCanvasState,
    pub(crate) segment_idx: usize,
    named_parts: Option<Arc<HashMap<String, DrawableHandle>>>,
    gltf_animations: Option<Arc<Vec<GltfAnimationMetadata>>>,
    style_targets: Arc<Vec<SharedObjectSpec>>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SvgPartError {
    #[error("this drawable has no named SVG or glTF parts")]
    NotSvg,
    #[error("unknown SVG part '{id}'; available ids: {available}")]
    Unknown { id: String, available: String },
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GltfAnimationError {
    #[error("this drawable has no glTF Actions")]
    NotGltf,
    #[error("unknown glTF Action '{name}'; available Actions: {available}")]
    Unknown { name: String, available: String },
    #[error("glTF Action speed must be finite and greater than zero")]
    InvalidSpeed,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid 3D rotation axis '{axis}'; expected x, y, or z")]
pub struct RotationAxisError {
    pub axis: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("material operations require a native Primitive3D drawable")]
pub struct Primitive3DHandleError;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LayoutOwnershipError {
    #[error("drawable belongs to a different Scene")]
    ForeignScene,
    #[error("drawable already belongs to layout {owner:?}")]
    AlreadyManaged { owner: ObjectId },
    #[error(
        "layout owns this drawable's position; remove at/shift/next_to/align_to/to_edge/to_corner and use LayoutItem offset or absolute placement"
    )]
    PositionalOperation,
}

/// A deferred glyph selection inside a text-like [`DrawableHandle`].
#[derive(Debug, Clone)]
pub struct FragmentSelection {
    pub(crate) target: ObjectId,
    pub(crate) fragment: String,
    pub(crate) occurrence: Option<usize>,
    state: SharedCanvasState,
    layout_owner: Option<ObjectId>,
}

/// Split ordinary mathematical source into display terms when an equation has
/// not declared semantic tags. Operators stand alone; adjacent non-operator
/// characters (for example `c^2` or `2x`) remain one term.
fn math_source_terms(source: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    for ch in source.chars() {
        if ch.is_whitespace() {
            if !current.is_empty() {
                terms.push(std::mem::take(&mut current));
            }
        } else if matches!(ch, '=' | '+' | '-' | '*' | '/' | '(' | ')' | '[' | ']') {
            if !current.is_empty() {
                terms.push(std::mem::take(&mut current));
            }
            terms.push(ch.to_string());
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        terms.push(current);
    }
    terms
}

impl DrawableHandle {
    /// Returns the immutable structured authoring snapshot for unified text.
    pub fn text_spec(&self) -> Option<gaanim_text::prelude::TextSpec> {
        let spec = self.spec.lock().expect("object spec poisoned");
        match &spec.kind {
            SpawnKind::Text(text) => Some(text.clone()),
            _ => None,
        }
    }

    fn text_semantic_pairs(
        &self,
        target: &DrawableHandle,
        requested: Option<Vec<(String, String)>>,
    ) -> Vec<(String, Option<usize>, String, Option<usize>)> {
        let source_tags = self
            .spec
            .lock()
            .expect("object spec poisoned")
            .fragment_tags
            .clone();
        let target_tags = target
            .spec
            .lock()
            .expect("object spec poisoned")
            .fragment_tags
            .clone();
        let requested = requested.unwrap_or_else(|| {
            source_tags
                .iter()
                .filter_map(|(name, _, _)| {
                    target_tags
                        .iter()
                        .any(|(target_name, _, _)| target_name == name)
                        .then_some((name.clone(), name.clone()))
                })
                .collect()
        });
        requested
            .into_iter()
            .filter_map(|(source_name, target_name)| {
                let (_, source_fragment, source_occurrence) = source_tags
                    .iter()
                    .rev()
                    .find(|(name, _, _)| name == &source_name)?;
                let (_, target_fragment, target_occurrence) = target_tags
                    .iter()
                    .rev()
                    .find(|(name, _, _)| name == &target_name)?;
                Some((
                    source_fragment.clone(),
                    *source_occurrence,
                    target_fragment.clone(),
                    *target_occurrence,
                ))
            })
            .collect()
    }

    fn require_text_transition_target(
        &self,
        target: &DrawableHandle,
    ) -> Result<(), LayoutOwnershipError> {
        if !self.same_canvas(target) {
            return Err(LayoutOwnershipError::ForeignScene);
        }
        let source_owner = self.layout_owner();
        let target_owner = target.layout_owner();
        if source_owner.is_some() || target_owner.is_some() {
            if source_owner != target_owner {
                return Err(LayoutOwnershipError::AlreadyManaged {
                    owner: target_owner.or(source_owner).expect("one owner exists"),
                });
            }
        }
        Ok(())
    }

    /// General text/math morph. Semantic paths are paired before the existing
    /// order-preserving grapheme and shape matching stages.
    pub fn morph_to(
        &self,
        target: &DrawableHandle,
        duration: f64,
    ) -> Result<Anim, LayoutOwnershipError> {
        self.require_text_transition_target(target)?;
        let semantic_pairs = self.text_semantic_pairs(target, None);
        Ok(self.anim_dur(
            AnimationType::TextTransition {
                target: target.id,
                copy: false,
                semantic_pairs,
            },
            Some(duration),
        ))
    }

    /// Structured derivation step, replacing the equation-only scene method.
    pub fn step_to(
        &self,
        target: &DrawableHandle,
        matches: Option<Vec<(String, String)>>,
        duration: f64,
    ) -> Result<Anim, LayoutOwnershipError> {
        self.require_text_transition_target(target)?;
        let semantic_pairs = self.text_semantic_pairs(target, matches);
        Ok(self.anim_dur(
            AnimationType::TextTransition {
                target: target.id,
                copy: false,
                semantic_pairs,
            },
            Some(duration),
        ))
    }

    /// Expands text around one semantic path while new graphemes enter.
    pub fn expand_to(
        &self,
        target: &DrawableHandle,
        anchor: &str,
        duration: f64,
    ) -> Result<Anim, LayoutOwnershipError> {
        self.require_text_transition_target(target)?;
        let semantic_pairs = self
            .text_semantic_pairs(target, Some(vec![(anchor.to_string(), anchor.to_string())]))
            .into_iter()
            .take(1)
            .collect();
        Ok(self.anim_dur(
            AnimationType::TextTransition {
                target: target.id,
                copy: false,
                semantic_pairs,
            },
            Some(duration),
        ))
    }

    /// Replace the authoring snapshot while preserving this handle identity.
    /// Timeline materialization observes the incremented version.
    pub fn r#become(&self, text: gaanim_text::prelude::TextSpec, duration: Option<f64>) {
        let parts = text.parts();
        let mut spec = self.spec.lock().expect("object spec poisoned");
        if !matches!(spec.kind, SpawnKind::Text(_)) {
            return;
        }
        spec.kind = SpawnKind::Text(text);
        spec.fragment_tags.clear();
        spec.fragment_fills.clear();
        for part in parts {
            if let Some(color) = part.style.color {
                spec.fragment_fills.push((part.text.clone(), color));
            }
            spec.fragment_tags
                .push((part.path.join("."), part.text, Some(part.occurrence)));
        }
        let owner = spec.layout_owner;
        drop(spec);

        let Some(owner) = owner else {
            return;
        };
        let mut state = self.state.lock().expect("canvas state poisoned");
        let mut affected = vec![owner];
        let mut index = 0;
        while index < affected.len() {
            let child = affected[index];
            for (root, snapshot) in &state.latest_layouts {
                if snapshot.members.iter().any(|member| member.id == child)
                    && !affected.contains(root)
                {
                    affected.push(*root);
                }
            }
            index += 1;
        }
        for root in affected {
            let Some(previous) = state.latest_layouts.get(&root).cloned() else {
                continue;
            };
            let mut next = previous.clone();
            next.version = next.version.saturating_add(1);
            state.latest_layouts.insert(root, next.clone());
            state.active_mut().ops.push(Op::LayoutTransition {
                from_version: Some(previous.version),
                to: next,
                duration: duration.filter(|value| value.is_finite() && *value > 0.0),
                entering: None,
                leaving: None,
            });
        }
    }

    pub fn same_canvas(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }

    pub fn claim_layout(&self, owner: &DrawableHandle) -> Result<(), LayoutOwnershipError> {
        if !self.same_canvas(owner) {
            return Err(LayoutOwnershipError::ForeignScene);
        }
        let mut spec = self.spec.lock().expect("object spec poisoned");
        if spec.manual_position_animation
            || spec.layout_ops.iter().any(|op| {
                matches!(
                    op,
                    LayoutOp::SetTranslation(_)
                        | LayoutOp::MoveAnchorTo { .. }
                        | LayoutOp::NextTo { .. }
                        | LayoutOp::AlignTo { .. }
                        | LayoutOp::ToEdge { .. }
                        | LayoutOp::ToCorner { .. }
                )
            })
        {
            return Err(LayoutOwnershipError::PositionalOperation);
        }
        if let Some(existing) = spec.layout_owner
            && existing != owner.id
        {
            return Err(LayoutOwnershipError::AlreadyManaged { owner: existing });
        }
        spec.layout_owner = Some(owner.id);
        Ok(())
    }

    pub fn release_layout(&self, owner: &DrawableHandle) {
        let mut spec = self.spec.lock().expect("object spec poisoned");
        if spec.layout_owner == Some(owner.id) {
            spec.layout_owner = None;
        }
    }
    pub(crate) fn new(
        id: ObjectId,
        kind: SpawnKind,
        state: SharedCanvasState,
        segment_idx: usize,
    ) -> Self {
        Self {
            id,
            spec: std::sync::Arc::new(std::sync::Mutex::new(ObjectSpec::new(id, kind))),
            state,
            segment_idx,
            named_parts: None,
            gltf_animations: None,
            style_targets: Arc::new(Vec::new()),
        }
    }

    fn update_spec(&self, f: impl FnOnce(&mut ObjectSpec)) -> Self {
        f(&mut self.spec.lock().expect("object spec poisoned"));
        self.clone()
    }

    fn update_style(&self, f: impl Fn(&mut ObjectSpec)) -> Self {
        f(&mut self.spec.lock().expect("object spec poisoned"));
        for target in self.style_targets.iter() {
            f(&mut target.lock().expect("SVG part spec poisoned"));
        }
        self.clone()
    }

    /// Keep a generated reactive visual hidden until it is included in a
    /// `Canvas::play` animation.
    pub(crate) fn defer_visibility_until_play(&self) {
        self.spec
            .lock()
            .expect("object spec poisoned")
            .defer_visibility_until_play = true;
    }

    pub(crate) fn with_style_targets(mut self, targets: Vec<SharedObjectSpec>) -> Self {
        self.style_targets = Arc::new(targets);
        self
    }

    pub(crate) fn inherited_style_targets(&self) -> Vec<SharedObjectSpec> {
        std::iter::once(self.spec.clone())
            .chain(self.style_targets.iter().cloned())
            .collect()
    }

    pub(crate) fn with_svg_parts(mut self, parts: HashMap<String, DrawableHandle>) -> Self {
        self.named_parts = Some(Arc::new(parts));
        self
    }

    pub(crate) fn with_gltf_metadata(
        mut self,
        parts: HashMap<String, DrawableHandle>,
        animations: Vec<GltfAnimationMetadata>,
    ) -> Self {
        self.named_parts = Some(Arc::new(parts));
        self.gltf_animations = Some(Arc::new(animations));
        self
    }

    /// If this drawable is an axes, return its x/y ranges and full config.
    pub fn axes_info(
        &self,
    ) -> Option<((f64, f64, f64), (f64, f64, f64), crate::canvas::AxesConfig)> {
        let spec = self.spec.lock().ok()?;
        if let crate::canvas::SpawnKind::Axes {
            x_range,
            y_range,
            config,
        } = &spec.kind
        {
            Some((*x_range, *y_range, config.clone()))
        } else {
            None
        }
    }

    /// Resolve a named source group or path from an imported SVG.
    pub fn part(&self, id: &str) -> Result<DrawableHandle, SvgPartError> {
        let Some(parts) = &self.named_parts else {
            return Err(SvgPartError::NotSvg);
        };
        parts.get(id).cloned().ok_or_else(|| {
            let mut available = parts.keys().cloned().collect::<Vec<_>>();
            available.sort();
            SvgPartError::Unknown {
                id: id.to_owned(),
                available: available.join(", "),
            }
        })
    }

    /// Stable selectors available through [`DrawableHandle::part`].
    pub fn parts(&self) -> Vec<String> {
        let mut result = self
            .named_parts
            .as_ref()
            .map(|parts| parts.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        result.sort();
        result
    }

    /// Blender Action names embedded in an imported glTF model.
    pub fn animations(&self) -> Vec<String> {
        self.gltf_animations
            .as_ref()
            .map(|items| items.iter().map(|item| item.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Schedule a Blender Action using absolute timeline sampling.
    #[allow(clippy::too_many_arguments)]
    pub fn animation(
        &self,
        name: &str,
        duration: Option<f64>,
        speed: f64,
        looped: bool,
        reverse: bool,
        transition: f64,
        start_time: f64,
    ) -> Result<Anim, GltfAnimationError> {
        if !speed.is_finite() || speed <= 0.0 {
            return Err(GltfAnimationError::InvalidSpeed);
        }
        let Some(animations) = &self.gltf_animations else {
            return Err(GltfAnimationError::NotGltf);
        };
        let Some(metadata) = animations.iter().find(|animation| animation.name == name) else {
            return Err(GltfAnimationError::Unknown {
                name: name.to_owned(),
                available: animations
                    .iter()
                    .map(|animation| animation.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            });
        };
        let authored_duration = (metadata.duration / speed).max(0.0);
        Ok(Anim::queued(
            self.id,
            AnimationType::GltfAnimation {
                animation_index: metadata.index,
                source_duration: metadata.duration,
                speed,
                looped,
                reverse,
                transition: transition.max(0.0),
                start_time: start_time.max(0.0),
            },
            self.state.clone(),
            self.segment_idx,
        )
        .duration(duration.unwrap_or(authored_duration)))
    }

    fn push_layout(&self, op: LayoutOp) -> Self {
        self.update_spec(|spec| {
            let positional = matches!(
                op,
                LayoutOp::SetTranslation(_)
                    | LayoutOp::MoveAnchorTo { .. }
                    | LayoutOp::NextTo { .. }
                    | LayoutOp::AlignTo { .. }
                    | LayoutOp::ToEdge { .. }
                    | LayoutOp::ToCorner { .. }
            );
            assert!(
                !(positional && spec.layout_owner.is_some()),
                "layout owns this drawable's position; use LayoutItem offset or absolute placement"
            );
            spec.layout_ops.push(op);
        })
    }

    pub fn layout_owner(&self) -> Option<ObjectId> {
        self.spec.lock().expect("object spec poisoned").layout_owner
    }

    // -- Instant setters (return Self) --

    pub fn fill(self, color: Color) -> Self {
        self.fill_brush(Brush::Solid(color))
    }

    pub fn material(
        self,
        material: gaanim_scene::Material3D,
    ) -> Result<Self, Primitive3DHandleError> {
        {
            let mut spec = self.spec.lock().expect("object spec poisoned");
            let SpawnKind::Primitive3D(mesh) = &mut spec.kind else {
                return Err(Primitive3DHandleError);
            };
            mesh.material = Some(material);
            spec.material_animation_cursor = Some(material);
        }
        Ok(self)
    }

    pub fn material_to(
        &self,
        material: gaanim_scene::Material3D,
    ) -> Result<Anim, Primitive3DHandleError> {
        let from = {
            let mut spec = self.spec.lock().expect("object spec poisoned");
            let SpawnKind::Primitive3D(mesh) = &spec.kind else {
                return Err(Primitive3DHandleError);
            };
            let from = spec
                .material_animation_cursor
                .or(mesh.material)
                .unwrap_or_default();
            spec.material_animation_cursor = Some(material);
            from
        };
        Ok(self.anim(AnimationType::Material3DTo { from, to: material }))
    }

    pub fn fill_brush(self, brush: Brush) -> Self {
        self.update_style(|spec| {
            spec.fill = Some(brush.clone());
            spec.fill_overridden = true;
        })
    }

    pub fn no_fill(self) -> Self {
        self.update_style(|spec| {
            spec.fill = None;
            spec.fill_overridden = true;
        })
    }

    pub fn stroke(self, color: Color, width: f64) -> Self {
        self.stroke_brush(Brush::Solid(color), width)
    }

    pub fn stroke_brush(self, brush: Brush, width: f64) -> Self {
        self.update_style(|spec| {
            spec.stroke = Some((brush.clone(), width));
            spec.stroke_style = Some(gaanim_core::kurbo::Stroke::new(width));
            spec.stroke_overridden = true;
        })
    }

    /// Apply a complete native stroke, including cap, join, miter and dashes.
    pub fn stroke_with_style(self, brush: Brush, style: gaanim_core::kurbo::Stroke) -> Self {
        self.update_style(|spec| {
            spec.stroke = Some((brush.clone(), style.width));
            spec.stroke_style = Some(style.clone());
            spec.stroke_overridden = true;
        })
    }

    pub fn no_stroke(self) -> Self {
        self.update_style(|spec| {
            spec.stroke = None;
            spec.stroke_style = None;
            spec.stroke_overridden = true;
        })
    }

    /// Attach an ordered theme class to this drawable and its visual style targets.
    pub fn style_class(self, name: impl Into<String>) -> Result<Self, String> {
        let name = name.into();
        if name.is_empty()
            || name
                .chars()
                .any(|ch| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')))
        {
            return Err("style class names must use letters, digits, '_' or '-'".into());
        }
        Ok(self.update_style(|spec| spec.style_classes.push(name.clone())))
    }

    /// Add a soft outer glow. The effect is compiled into the retained vector fragment.
    pub fn glow(self, color: Color, radius: f64, intensity: f32) -> Self {
        self.update_style(|spec| {
            spec.glow = Some(gaanim_renderer::effects::Glow {
                color,
                radius,
                intensity,
            });
        })
    }

    /// Apply a soft vector blur to this drawable.
    pub fn blur(self, sigma: f64) -> Self {
        self.update_style(|spec| {
            spec.blur = Some(gaanim_renderer::effects::GaussianBlur { sigma });
        })
    }

    /// Add a soft shadow behind this drawable.
    pub fn shadow(self, color: Color, offset: DVec2, blur_radius: f64) -> Self {
        self.update_style(|spec| {
            spec.shadow = Some(gaanim_renderer::effects::DropShadow {
                color,
                offset,
                blur_radius,
            });
        })
    }

    /// Remove all visual effects while preserving fill and stroke styling.
    pub fn no_effects(self) -> Self {
        self.update_style(|spec| {
            spec.glow = None;
            spec.blur = None;
            spec.shadow = None;
        })
    }

    /// Clip this drawable to another drawable's vector geometry.
    ///
    /// The mask keeps its own visibility, so call `mask.no_fill().no_stroke()`
    /// when it should act only as clipping geometry.
    pub fn clip(self, mask: &DrawableHandle, rule: gaanim_core::peniko::Fill) -> Self {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::SetClip {
                target: self.id,
                mask: Some(mask.id),
                rule,
            });
        self
    }

    /// Remove a previously configured clipping mask.
    pub fn no_clip(self) -> Self {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::SetClip {
                target: self.id,
                mask: None,
                rule: gaanim_core::peniko::Fill::NonZero,
            });
        self
    }

    /// Colors every matching text or equation fragment in this drawable.
    ///
    /// Matching is case-insensitive and tolerant of math formatting, so a
    /// query such as `"mc"` can target `m c` in a Typst equation. Later calls
    /// take precedence when selections overlap.
    pub fn color_by(self, fragment: impl Into<String>, color: Color) -> Self {
        let fragment = fragment.into();
        self.update_spec(|spec| {
            if !fragment.is_empty() {
                spec.fragment_fills.push((fragment, color));
            }
        })
    }

    /// Creates a deferred selection for glyph-level styling and emphasis.
    pub fn select(&self, fragment: impl Into<String>) -> FragmentSelection {
        FragmentSelection {
            target: self.id,
            fragment: fragment.into(),
            occurrence: None,
            state: self.state.clone(),
            layout_owner: self.layout_owner(),
        }
    }

    /// Creates a deferred selection for one zero-based occurrence of a fragment.
    pub fn select_nth(&self, fragment: impl Into<String>, occurrence: usize) -> FragmentSelection {
        FragmentSelection {
            target: self.id,
            fragment: fragment.into(),
            occurrence: Some(occurrence),
            state: self.state.clone(),
            layout_owner: self.layout_owner(),
        }
    }

    /// Registers a semantic name for a text or equation fragment.
    pub fn define_tag(
        self,
        name: impl Into<String>,
        fragment: impl Into<String>,
        occurrence: Option<usize>,
    ) -> Self {
        let name = name.into();
        let fragment = fragment.into();
        self.update_spec(|spec| {
            if !name.trim().is_empty() && !fragment.trim().is_empty() {
                spec.fragment_tags.push((name, fragment, occurrence));
            }
        })
    }

    /// Selects a previously registered semantic tag.
    pub fn tag(&self, name: &str) -> Option<FragmentSelection> {
        let (fragment, occurrence) = self
            .spec
            .lock()
            .expect("object spec poisoned")
            .fragment_tags
            .iter()
            .rev()
            .find(|(tag, _, _)| tag == name)
            .map(|(_, fragment, occurrence)| (fragment.clone(), *occurrence))?;
        Some(FragmentSelection {
            target: self.id,
            fragment,
            occurrence,
            state: self.state.clone(),
            layout_owner: self.layout_owner(),
        })
    }

    /// Writes semantic terms in tag order instead of staggering individual
    /// glyphs. If `tags` is omitted, all declared tags are used in declaration
    /// order.
    pub fn write_by_parts(&self, paths: Option<Vec<String>>, duration: f64) -> Self {
        if !duration.is_finite() || duration <= 0.0 {
            return self.clone();
        }
        let spec = self.spec.lock().expect("object spec poisoned").clone();
        let declared = spec.fragment_tags;
        let terms: Vec<(String, Option<usize>)> = match paths {
            Some(names) => names
                .into_iter()
                .filter_map(|name| {
                    declared
                        .iter()
                        .rev()
                        .find(|(tag, _, _)| tag == &name)
                        .map(|(_, fragment, occurrence)| (fragment.clone(), *occurrence))
                })
                .collect(),
            None if !declared.is_empty() => declared
                .into_iter()
                .map(|(_, fragment, occurrence)| (fragment, occurrence))
                .collect(),
            None => match spec.kind {
                SpawnKind::Text(spec) => math_source_terms(&spec.plain_text())
                    .into_iter()
                    .map(|fragment| (fragment, None))
                    .collect(),
                _ => Vec::new(),
            },
        };
        if !terms.is_empty() {
            self.state.lock().expect("canvas state poisoned").segments[self.segment_idx]
                .ops
                .push(Op::WriteTerms {
                    target: self.id,
                    terms,
                    duration,
                });
        }
        self.clone()
    }

    /// Sets the initial value of a `ValueTracker` before the scene is compiled.
    /// Calling this on a regular drawable has no effect.
    pub fn set_value(self, value: f64) -> Self {
        self.update_spec(|spec| {
            if let SpawnKind::ValueTracker(current) = &mut spec.kind {
                *current = value;
            }
        })
    }

    pub fn opacity(self, op: f32) -> Self {
        self.update_style(|spec| {
            spec.opacity = op;
            spec.opacity_overridden = true;
        })
    }

    pub fn z_index(self, z: i32) -> Self {
        self.update_spec(|spec| spec.z_index = z)
    }

    pub fn at(self, x: f64, y: f64) -> Self {
        self.push_layout(LayoutOp::SetTranslation(DVec3::new(x, y, 0.0)))
    }

    /// 3D position in world space (perspective-aware).
    pub fn at_3d(self, x: f64, y: f64, z: f64) -> Self {
        self.push_layout(LayoutOp::SetTranslation(DVec3::new(x, y, z)))
    }

    /// Makes this drawable always face the camera (billboard) in 3D.
    /// Chainable: `scene.text("label").at_3d(x,y,z).billboard()`
    pub fn billboard(self) -> Self {
        self.update_spec(|spec| spec.billboard = true)
    }

    /// Makes this drawable a fixed HUD overlay (screen-space, not affected by 3D camera).
    /// Chainable: `scene.text("title").hud().at(0,300)`
    pub fn hud(self) -> Self {
        self.update_spec(|spec| spec.hud = true)
    }

    pub fn scaled(self, factor: f64) -> Self {
        self.push_layout(LayoutOp::SetScale(factor))
    }

    pub fn scaled_3d(self, x: f64, y: f64, z: f64) -> Self {
        self.push_layout(LayoutOp::SetScale3D(DVec3::new(x, y, z)))
    }

    pub fn rotated(self, radians: f64) -> Self {
        self.push_layout(LayoutOp::SetRotation(radians))
    }

    /// Set XYZ Euler rotation in radians.
    pub fn rotated_3d(self, x: f64, y: f64, z: f64) -> Self {
        self.push_layout(LayoutOp::SetRotation3D(DVec3::new(x, y, z)))
    }

    /// Set the scene-space pivot used by rotations and uniform scaling.
    ///
    /// This is the natural way to rotate a mechanism around a known hinge or
    /// disk center. The engine converts the point to the group's local anchor
    /// after all initial layout has been resolved.
    pub fn with_pivot(self, x: f64, y: f64) -> Self {
        self.push_layout(LayoutOp::SetPivot(DVec3::new(x, y, 0.0)))
    }

    pub fn with_pivot_3d(self, x: f64, y: f64, z: f64) -> Self {
        self.push_layout(LayoutOp::SetPivot(DVec3::new(x, y, z)))
    }

    /// Alias for [`Self::with_pivot`].
    pub fn pivot(self, x: f64, y: f64) -> Self {
        self.with_pivot(x, y)
    }

    pub fn at_anchor(self, x: f64, y: f64, anchor: Anchor) -> Self {
        self.push_layout(LayoutOp::MoveAnchorTo {
            target: DVec3::new(x, y, 0.0),
            anchor,
        })
    }

    pub fn next_to(self, reference: &DrawableHandle, direction: Direction, spacing: f64) -> Self {
        self.next_to_aligned(reference, direction, spacing, Anchor::Center)
    }

    pub fn next_to_aligned(
        self,
        reference: &DrawableHandle,
        direction: Direction,
        spacing: f64,
        aligned_edge: Anchor,
    ) -> Self {
        self.push_layout(LayoutOp::NextTo {
            reference: reference.id,
            direction,
            spacing,
            aligned_edge,
        })
    }

    pub fn align_to(
        self,
        reference: &DrawableHandle,
        target_anchor: Anchor,
        reference_anchor: Anchor,
    ) -> Self {
        self.push_layout(LayoutOp::AlignTo {
            reference: reference.id,
            target_anchor,
            reference_anchor,
        })
    }

    pub fn to_edge(self, direction: Direction, buff: f64) -> Self {
        self.push_layout(LayoutOp::ToEdge { direction, buff })
    }

    pub fn to_corner(self, corner: Anchor, buff: f64) -> Self {
        self.push_layout(LayoutOp::ToCorner { corner, buff })
    }

    // -- Internal helpers --

    fn anim(&self, ty: AnimationType) -> Anim {
        if matches!(
            &ty,
            AnimationType::TranslateBy { .. } | AnimationType::TranslateTo { .. }
        ) {
            let mut spec = self.spec.lock().expect("object spec poisoned");
            assert!(
                spec.layout_owner.is_none(),
                "layout owns this drawable's position; use LayoutItem offset or configure_item"
            );
            spec.manual_position_animation = true;
        }
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Anim::queued(self.id, ty, self.state.clone(), active_idx)
    }

    fn anim_dur(&self, ty: AnimationType, dur: Option<f64>) -> Anim {
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Anim::queued(self.id, ty, self.state.clone(), active_idx).with_duration(dur)
    }

    /// Animates a `ValueTracker` to `to`. Regular drawables ignore this lens.
    pub fn animate_value_to(&self, to: f64) -> Anim {
        self.anim(AnimationType::SignalFloat { to })
    }

    // -- Animation methods (return Anim, auto-enqueued) --
    //
    // Every method accepts an optional duration via `impl OptDuration`:
    //   obj.fade_in()        — default 1.0s
    //   obj.fade_in(2.0)     — 2.0s
    //   obj.fade_in(None)    — default 1.0s

    /// Build one typed animation that can target several drawable properties.
    /// The first property modifier activates the usual deferred auto-queue.
    pub fn animate(&self) -> Anim {
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Anim::properties(self.id, self.state.clone(), active_idx, self.spec.clone())
    }

    pub fn r#move(&self, dx: f64, dy: f64) -> Anim {
        self.anim(AnimationType::TranslateBy {
            delta: DVec3::new(dx, dy, 0.0),
        })
    }

    pub fn move_to(&self, x: f64, y: f64) -> Anim {
        self.anim(AnimationType::TranslateTo {
            to: DVec3::new(x, y, 0.0),
        })
    }

    pub fn move_3d(&self, dx: f64, dy: f64, dz: f64) -> Anim {
        self.anim(AnimationType::TranslateBy {
            delta: DVec3::new(dx, dy, dz),
        })
    }

    pub fn move_to_3d(&self, x: f64, y: f64, z: f64) -> Anim {
        self.anim(AnimationType::TranslateTo {
            to: DVec3::new(x, y, z),
        })
    }

    pub fn glide_to(&self, x: f64, y: f64) -> Anim {
        self.move_to(x, y)
    }

    pub fn scale(&self, factor: f64) -> Anim {
        self.anim(AnimationType::ScaleUniform { factor })
    }

    pub fn scale_to_3d(&self, x: f64, y: f64, z: f64) -> Anim {
        self.anim(AnimationType::ScaleTo {
            to: DVec3::new(x, y, z),
        })
    }

    pub fn rotate(&self, rad: f64) -> Anim {
        let pivot = self.spec.lock().ok().and_then(|spec| {
            spec.layout_ops.iter().rev().find_map(|op| match op {
                LayoutOp::SetPivot(p) => Some(*p),
                _ => None,
            })
        });
        self.anim(AnimationType::RotateBy {
            angle_radians: rad,
            pivot,
        })
    }

    pub fn rotate_by_3d(&self, axis: &str, radians: f64) -> Result<Anim, RotationAxisError> {
        let delta = match axis.to_ascii_lowercase().as_str() {
            "x" => DQuat::from_rotation_x(radians),
            "y" => DQuat::from_rotation_y(radians),
            "z" => DQuat::from_rotation_z(radians),
            _ => {
                return Err(RotationAxisError {
                    axis: axis.to_owned(),
                });
            }
        };
        Ok(self.anim(AnimationType::RotateBy3D { delta }))
    }

    /// Animate to an XYZ Euler orientation in radians.
    pub fn rotate_to_3d(&self, x: f64, y: f64, z: f64) -> Anim {
        self.anim(AnimationType::RotateTo {
            to: DQuat::from_euler(EulerRot::XYZ, x, y, z),
        })
    }

    pub fn rotate_about_point(&self, x: f64, y: f64, rad: f64) -> Anim {
        self.anim(AnimationType::RotateBy {
            angle_radians: rad,
            pivot: Some(DVec3::new(x, y, 0.0)),
        })
    }

    pub fn fade_in(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::FadeIn, dur.into_opt())
    }

    /// Appears while moving from `direction` toward its final position.
    pub fn fade_in_from(&self, direction: Direction, distance: f64, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::FadeInFrom {
                offset: direction.to_vector() * distance.max(0.0),
            },
            dur.into_opt(),
        )
    }

    pub fn fade_out(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::FadeOut, dur.into_opt())
    }

    pub fn fade_to(&self, alpha: f32) -> Anim {
        self.anim(AnimationType::FadeTo { to: alpha })
    }

    pub fn write(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::Write {
                config: DrawAnimationConfig::default(),
            },
            dur.into_opt(),
        )
    }

    pub fn create(&self, dur: impl OptDuration) -> Anim {
        if matches!(
            &self.spec.lock().expect("object spec poisoned").kind,
            SpawnKind::Primitive3D(..)
        ) {
            return self.anim_dur(AnimationType::Create3D, dur.into_opt());
        }
        self.anim_dur(
            AnimationType::Create {
                config: DrawAnimationConfig::default(),
            },
            dur.into_opt(),
        )
    }

    pub fn unwrite(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::Unwrite {
                config: DrawAnimationConfig::default(),
            },
            dur.into_opt(),
        )
    }

    pub fn uncreate(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::Uncreate {
                config: DrawAnimationConfig::default(),
            },
            dur.into_opt(),
        )
    }

    pub fn grow_from_center(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::GrowFromCenter, dur.into_opt())
    }

    pub fn shrink_to_center(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::ShrinkToCenter, dur.into_opt())
    }

    pub fn spin_in_from_nothing(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::SpinInFromNothing, dur.into_opt())
    }

    pub fn grow_from_point(&self, px: f64, py: f64) -> Anim {
        self.anim(AnimationType::GrowFromPoint { px, py })
    }

    pub fn grow_from_edge(&self, dir: &str) -> Anim {
        self.anim(AnimationType::GrowFromEdge {
            direction: dir.to_string(),
        })
    }

    pub fn draw_border_then_fill(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::DrawBorderThenFill {
                config: DrawAnimationConfig::default(),
            },
            dur.into_opt(),
        )
    }

    pub fn indicate(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::Indicate {
                color: None,
                scale_factor: 1.1,
            },
            dur.into_opt(),
        )
    }

    pub fn circumscribe(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::Circumscribe { color: None }, dur.into_opt())
    }

    pub fn flash(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::Flash {
                color: None,
                n_lines: 16,
                radius: 100.0,
            },
            dur.into_opt(),
        )
    }

    pub fn wiggle(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::Wiggle, dur.into_opt())
    }

    pub fn move_along_path(&self, path: BezPath) -> Anim {
        self.anim(AnimationType::MoveAlongPath {
            path,
            path_target: None,
        })
    }

    pub fn move_along_drawable(&self, target: &DrawableHandle) -> Anim {
        self.anim(AnimationType::MoveAlongPath {
            path: BezPath::new(),
            path_target: Some(target.id),
        })
    }

    pub fn fade_transform(&self, target: &DrawableHandle) -> Anim {
        self.anim(AnimationType::FadeTransform { target: target.id })
    }

    pub fn transform(&self, target: &DrawableHandle) -> Anim {
        self.anim(AnimationType::Transform { target: target.id })
    }

    pub fn replacement_transform(&self, target: &DrawableHandle) -> Anim {
        self.anim(AnimationType::ReplacementTransform { target: target.id })
    }

    // -- Reactive methods --

    /// Attach a preset updater that runs every frame.
    ///
    /// Use `UpdaterPreset` variants: `Orbit`, `AdvanceX`, `Bob`, `Rotate`, `Pulse`.
    pub fn add_updater(&self, preset: UpdaterPreset) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachUpdater {
                target: self.id,
                preset,
            });
    }

    /// Queue a custom updater while retaining it with the deferred canvas.
    ///
    /// This is primarily used by language bindings. Keeping the updater in the
    /// operation makes repeated canvas compilation (preview followed by export)
    /// deterministic and avoids process-global callback registries.
    #[doc(hidden)]
    pub fn add_custom_updater(&self, updater: gaanim_animation::Updater) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachCustomUpdater {
                target: self.id,
                updater,
            });
    }

    /// Remove any updater attached to this entity.
    pub fn remove_updater(&self) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::RemoveUpdater(self.id));
    }

    /// Copy the source entity's Y position each frame (after updaters run).
    pub fn bind_y_from(&self, source: &DrawableHandle) {
        self.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachPositionBinding {
                target: self.id,
                source: source.id,
                axes: AxisMask::Y,
            });
    }

    /// Copy the source entity's X position each frame (after updaters run).
    pub fn bind_x_from(&self, source: &DrawableHandle) {
        self.bind_position_from(source, AxisMask::X);
    }

    /// Keep this drawable centered on `source` each frame. The drawable is
    /// deferred until an entry animation is included in `Canvas::play`.
    ///
    /// This is an exact XY position binding. It is useful for labels, markers,
    /// and accents that should travel with an independently animated object.
    pub fn attach_to(&self, source: &DrawableHandle) {
        self.bind_position_from(source, AxisMask::XY);
    }

    /// Follow `source` while preserving a scene-space `(x, y)` offset. The
    /// drawable is deferred until an entry animation is included in
    /// `Canvas::play`.
    pub fn follow_to(&self, source: &DrawableHandle, offset_x: f64, offset_y: f64) {
        self.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachPositionFollow {
                target: self.id,
                source: source.id,
                offset: DVec3::new(offset_x, offset_y, 0.0),
            });
    }

    /// Copy the source entity's position on specified axes each frame.
    pub fn bind_position_from(&self, source: &DrawableHandle, axes: AxisMask) {
        self.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachPositionBinding {
                target: self.id,
                source: source.id,
                axes,
            });
    }
}

impl FragmentSelection {
    fn push(&self, op: Op) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(op);
    }

    fn animate(self, effect: TextSelectionEffect, duration: impl OptDuration) -> Anim {
        let duration = duration.into_opt();
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Anim::queued(
            self.target,
            AnimationType::TextSelection {
                fragment: self.fragment,
                occurrence: self.occurrence,
                effect,
            },
            self.state,
            active_idx,
        )
        .with_duration(duration)
    }

    /// Instantly colors this selected fragment.
    pub fn fill(self, color: Color) -> Self {
        if !self.fragment.trim().is_empty() {
            self.push(Op::FragmentFill {
                target: self.target,
                fragment: self.fragment.clone(),
                occurrence: self.occurrence,
                color,
            });
        }
        self
    }

    /// Emphasizes this fragment without affecting the surrounding text.
    pub fn indicate(self, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Indicate, duration)
    }

    pub fn pulse(self, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Pulse, duration)
    }

    pub fn wiggle(self, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Wiggle, duration)
    }

    pub fn wave(self, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Wave, duration)
    }

    pub fn highlight(self, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Highlight, duration)
    }

    pub fn focus(self, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Focus, duration)
    }

    pub fn brace(self, label: String, above: bool, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Brace { label, above }, duration)
    }

    pub fn annotate(self, label: String, offset: DVec3, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Annotate { label, offset }, duration)
    }

    /// Reveals this fragment with `Fade`, `Wipe`, or `FromBelow`.
    pub fn reveal(self, style: FragmentRevealStyle, duration: impl OptDuration) -> Anim {
        let effect = match style {
            FragmentRevealStyle::Fade => TextSelectionEffect::RevealFade,
            FragmentRevealStyle::Wipe => TextSelectionEffect::RevealWipe,
            FragmentRevealStyle::FromBelow => TextSelectionEffect::RevealFromBelow,
        };
        self.animate(effect, duration)
    }

    /// Strikes through this fragment and fades it from the equation.
    pub fn cancel(self, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Cancel, duration)
    }

    /// Animates this fragment's fill to `color`.
    pub fn color_to(self, color: Color, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::ColorTo(color), duration)
    }

    /// Morphs this selection into `target`, pairing matching glyphs in order.
    /// Unpaired glyphs remain unchanged; use equal-sized fragments for the
    /// clearest equation derivations.
    pub fn morph_to(
        self,
        target: &FragmentSelection,
        duration: impl OptDuration,
    ) -> Result<Anim, LayoutOwnershipError> {
        if !Arc::ptr_eq(&self.state, &target.state) {
            return Err(LayoutOwnershipError::ForeignScene);
        }
        if self.layout_owner != target.layout_owner
            && (self.layout_owner.is_some() || target.layout_owner.is_some())
        {
            return Err(LayoutOwnershipError::AlreadyManaged {
                owner: target
                    .layout_owner
                    .or(self.layout_owner)
                    .expect("one owner exists"),
            });
        }
        let duration = duration.into_opt();
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Ok(Anim::queued(
            self.target,
            AnimationType::TextSelectionTransform {
                target: target.target,
                source_fragment: self.fragment,
                source_occurrence: self.occurrence,
                target_fragment: target.fragment.clone(),
                target_occurrence: target.occurrence,
                copy: false,
            },
            self.state,
            active_idx,
        )
        .with_duration(duration))
    }

    /// Copies this selection to `target` while keeping both parent texts
    /// visible. This is the structured-text replacement for equation-term
    /// copying.
    pub fn copy_to(
        self,
        target: &FragmentSelection,
        duration: impl OptDuration,
    ) -> Result<Anim, LayoutOwnershipError> {
        if !Arc::ptr_eq(&self.state, &target.state) {
            return Err(LayoutOwnershipError::ForeignScene);
        }
        if self.layout_owner != target.layout_owner
            && (self.layout_owner.is_some() || target.layout_owner.is_some())
        {
            return Err(LayoutOwnershipError::AlreadyManaged {
                owner: target
                    .layout_owner
                    .or(self.layout_owner)
                    .expect("one owner exists"),
            });
        }
        let duration = duration.into_opt();
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Ok(Anim::queued(
            self.target,
            AnimationType::TextSelectionTransform {
                target: target.target,
                source_fragment: self.fragment,
                source_occurrence: self.occurrence,
                target_fragment: target.fragment.clone(),
                target_occurrence: target.occurrence,
                copy: true,
            },
            self.state,
            active_idx,
        )
        .with_duration(duration))
    }
}
