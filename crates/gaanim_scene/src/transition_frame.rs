//! Per-frame vector state of an active scene transition.
//!
//! The timeline evaluates every transition as a pure function of the playhead
//! and publishes the result here; renderers only read it. Masks and overlays
//! are plain `kurbo`/`peniko` geometry in world space, so they stay exact at
//! any resolution and need no intermediate textures. A shader transition is
//! the exception: it renders each side alone and blends them in one WGSL pass.

use std::collections::HashSet;
use std::sync::Arc;

use bevy::prelude::{Entity, Resource};
use gaanim_core::kurbo::{BezPath, Point};
use gaanim_core::peniko::{BlendMode, Brush, Fill};

/// World-space region in which one side of a transition stays visible.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionMask {
    /// Region that keeps the scene visible.
    pub path: BezPath,
    /// Fill rule used to interpret `path`.
    pub rule: Fill,
    /// Optional soft edge: a linear alpha ramp that is opaque at `.0` and
    /// transparent at `.1`, applied inside `path`.
    pub fade: Option<(Point, Point)>,
}

impl TransitionMask {
    /// A hard-edged mask using the non-zero fill rule.
    pub fn hard(path: BezPath) -> Self {
        Self {
            path,
            rule: Fill::NonZero,
            fade: None,
        }
    }
}

/// One layer drawn above every drawable while a transition overlay is active.
#[derive(Debug, Clone)]
pub struct TransitionOverlayLayer {
    /// Blend mode used to composite the layer over the frame.
    pub blend: BlendMode,
    /// World-space clip of the layer (the visible camera frame).
    pub clip: BezPath,
    /// Filled shapes and their paints, in world space.
    pub fills: Vec<(BezPath, Brush)>,
}

/// A WGSL transition that blends the outgoing and incoming sides, as plain
/// data: the renderer compiles it (see `gaanim_renderer::post_process`).
///
/// `source` defines `fn transition(uv: vec2<f32>) -> vec4<f32>` and reads
/// `gaanim_from(uv)`, `gaanim_to(uv)` and `progress`; `uniforms` names the
/// `f32` fields of `gaanim_uniforms`, whose `values` are fixed, and `data`
/// fills the storage array `gaanim_data` (e.g. a luma reveal map).
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionShader {
    pub source: String,
    pub uniforms: Vec<String>,
    pub values: Vec<f32>,
    pub data: Option<Vec<[f32; 4]>>,
}

/// The shader transition at the playhead and its eased progress.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionShaderFrame {
    pub shader: Arc<TransitionShader>,
    pub progress: f32,
}

/// Which side of the active transition a drawable belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransitionSide {
    /// Not part of either scene (persistent or untagged objects).
    #[default]
    None,
    /// Member of the outgoing scene.
    Outgoing,
    /// Member of the incoming scene.
    Incoming,
}

/// Resource: vector masks and overlays of the transition at the playhead.
///
/// Rewritten by every timeline seek; `Default` means "no transition".
#[derive(Resource, Debug, Clone, Default)]
pub struct SceneTransitionFrame {
    /// Entities of the outgoing scene (roots and descendants).
    pub outgoing: HashSet<Entity>,
    /// Entities of the incoming scene (roots and descendants).
    pub incoming: HashSet<Entity>,
    /// Visible region of the outgoing scene, when it is masked.
    pub outgoing_mask: Option<TransitionMask>,
    /// Visible region of the incoming scene, when it is masked.
    pub incoming_mask: Option<TransitionMask>,
    /// Timeline positions whose segment backgrounds belong to the outgoing and
    /// incoming sides. Present only while masks split the frame.
    pub backgrounds: Option<(f64, f64)>,
    /// Overlay layers drawn above every drawable.
    pub overlays: Vec<TransitionOverlayLayer>,
    /// A shader transition: each side is rendered alone and the shader
    /// blends them; drawables of neither side and overlays go above.
    pub shader: Option<TransitionShaderFrame>,
}

impl SceneTransitionFrame {
    /// Whether this frame changes nothing in the rendered output.
    pub fn is_empty(&self) -> bool {
        self.outgoing_mask.is_none()
            && self.incoming_mask.is_none()
            && self.overlays.is_empty()
            && self.shader.is_none()
    }

    /// Whether the frame splits its drawables into sides.
    fn splits(&self) -> bool {
        self.outgoing_mask.is_some() || self.incoming_mask.is_some() || self.shader.is_some()
    }

    /// Classify an entity, walking up its ancestors so untagged descendants
    /// inherit the scene of their root.
    pub fn side_of(
        &self,
        entity: Entity,
        mut parent_of: impl FnMut(Entity) -> Option<Entity>,
    ) -> TransitionSide {
        if !self.splits() {
            return TransitionSide::None;
        }
        let mut current = Some(entity);
        while let Some(entity) = current {
            if self.incoming.contains(&entity) {
                return TransitionSide::Incoming;
            }
            if self.outgoing.contains(&entity) {
                return TransitionSide::Outgoing;
            }
            current = parent_of(entity);
        }
        TransitionSide::None
    }

    /// Mask applied to a drawable on `side`, if any.
    pub fn mask_for(&self, side: TransitionSide) -> Option<&TransitionMask> {
        match side {
            TransitionSide::None => None,
            TransitionSide::Outgoing => self.outgoing_mask.as_ref(),
            TransitionSide::Incoming => self.incoming_mask.as_ref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_core::kurbo::{Rect, Shape};

    #[test]
    fn side_of_walks_ancestors_and_ignores_unmasked_frames() {
        let root = Entity::from_raw_u32(1).unwrap();
        let child = Entity::from_raw_u32(2).unwrap();
        let mut frame = SceneTransitionFrame::default();
        frame.incoming.insert(root);
        let parent = |entity: Entity| (entity == child).then_some(root);
        assert_eq!(frame.side_of(child, parent), TransitionSide::None);

        frame.incoming_mask = Some(TransitionMask::hard(
            Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.1),
        ));
        assert_eq!(frame.side_of(child, parent), TransitionSide::Incoming);
        assert!(frame.mask_for(TransitionSide::Outgoing).is_none());
        assert!(!frame.is_empty());
    }
}
