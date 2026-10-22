//! Imported glTF models on the timeline: their nodes' rest poses, the rig
//! that skins and morphs their meshes, and their animations played as clips.
//!
//! A play of an animation ([`ClipPlay`]) is a timeline clip whose lens
//! ([`GltfClipLens`]) writes the pose of the model's nodes and the weights of
//! their morph targets as a function of time alone: seeks land exactly, and
//! a recording keeps the matrices and weights it produced, so players never
//! evaluate the animation. A play with a
//! `transition` fades from the pose of the play before it on the same
//! model, which keeps running meanwhile, to its own. A play that starts
//! while the model's previous one still runs (one `Scene.launch` started)
//! takes over from it: the timeline applies clips in start order, so the
//! later play's pose is the one shown.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::{Component, Entity, World};
use gaanim_core::ObjectId;
use gaanim_core::glam::DMat4;
use gaanim_math::SpatialTransform;
use gaanim_objects::gltf::{GltfAnimations, NodePose};

/// What the root of an imported model declares: its nodes, in the order of
/// [`gaanim_objects::gltf::GltfModel::nodes`], and its skinned and morphed
/// primitives.
#[derive(Debug, Clone)]
pub struct GltfRigSpec {
    pub nodes: Arc<[ObjectId]>,
    /// Each skinned primitive, the nodes of its joints and their inverse
    /// bind matrices.
    pub skins: Vec<(ObjectId, Arc<[usize]>, Arc<[DMat4]>)>,
    /// Each primitive with morph targets and the node whose weights it
    /// takes.
    pub morphs: Vec<(ObjectId, usize)>,
}

impl GltfRigSpec {
    /// The rig with its drawables replaced by their copies in `copies`.
    pub(crate) fn remapped(
        &self,
        copies: &std::collections::HashMap<ObjectId, crate::canvas::DrawableHandle>,
    ) -> Self {
        let copy = |id: &ObjectId| copies.get(id).map_or(*id, |handle| handle.id);
        Self {
            nodes: self.nodes.iter().map(copy).collect(),
            skins: self
                .skins
                .iter()
                .map(|(mesh, joints, bind)| (copy(mesh), Arc::clone(joints), Arc::clone(bind)))
                .collect(),
            morphs: self
                .morphs
                .iter()
                .map(|(primitive, node)| (copy(primitive), *node))
                .collect(),
        }
    }
}

/// The entities of an imported model's nodes, on its root, which its
/// animations move, and those of its primitives with morph targets, with
/// the node whose weights each takes.
#[derive(Component, Debug, Clone)]
pub struct GltfRig {
    pub nodes: Arc<[Entity]>,
    pub morphs: Arc<[(Entity, usize)]>,
}

/// One play of an animation: where it starts on the timeline, for how
/// long, and how the animation's own time runs through it.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipPlay {
    pub clip: usize,
    /// Timeline seconds.
    pub start: f64,
    pub duration: f64,
    pub speed: f64,
    pub looped: bool,
    pub reverse: bool,
    /// Seconds into the animation where the play starts.
    pub offset: f64,
}

impl ClipPlay {
    /// The animation's time `elapsed` seconds into the play: running at its
    /// speed from its offset, wrapped when it loops and held at its ends
    /// otherwise, and run backwards when reversed.
    pub fn clip_time(&self, animations: &GltfAnimations, elapsed: f64) -> f64 {
        let length = animations.clips[self.clip].duration;
        if !(length > 0.0) {
            return 0.0;
        }
        let time = self.offset + elapsed.max(0.0) * self.speed;
        let time = if self.looped {
            time.rem_euclid(length)
        } else {
            time.clamp(0.0, length)
        };
        if self.reverse { length - time } else { time }
    }
}

/// One play of a model's animation as the timeline schedules it.
#[derive(Debug, Clone)]
struct ScheduledPlay {
    start: f64,
    duration: f64,
    rate_func: gaanim_math::RateFunc,
    lens: Arc<GltfClipLens>,
}

/// The plays of each imported model and the nodes of each, by object:
/// where a model's parts are at any instant, as its animations put them.
#[derive(Debug, Clone, Default)]
pub(crate) struct GltfPoses {
    /// By model root, in the order they were scheduled.
    plays: HashMap<ObjectId, Vec<ScheduledPlay>>,
    /// By node: its model's root and its index among the model's nodes.
    nodes: HashMap<ObjectId, (ObjectId, usize)>,
    /// By model root: its primitives with morph targets, each with the
    /// index of its node.
    morphs: HashMap<ObjectId, Arc<[(ObjectId, usize)]>>,
    /// By primitive: the weights of its morph targets it was declared with,
    /// which it shows before any animation moves them.
    declared: HashMap<ObjectId, Arc<[f32]>>,
}

impl GltfPoses {
    /// Note that `lens` plays on the model of `root` from timeline second
    /// `start`, for `duration` seconds eased by `rate_func`.
    pub(crate) fn played(
        &mut self,
        root: ObjectId,
        start: f64,
        duration: f64,
        rate_func: gaanim_math::RateFunc,
        lens: Arc<GltfClipLens>,
    ) {
        self.plays.entry(root).or_default().push(ScheduledPlay {
            start,
            duration,
            rate_func,
            lens,
        });
    }

    /// Note the nodes of the model whose root is `root`, by their index.
    pub(crate) fn rigged(&mut self, root: ObjectId, nodes: impl IntoIterator<Item = ObjectId>) {
        for (index, node) in nodes.into_iter().enumerate() {
            self.nodes.insert(node, (root, index));
        }
    }

    /// Note the primitives with morph targets of the model whose root is
    /// `root`, each with the index of its node and the weights it was
    /// declared with.
    pub(crate) fn morphed(&mut self, root: ObjectId, morphs: Vec<(ObjectId, usize, Arc<[f32]>)>) {
        if morphs.is_empty() {
            return;
        }
        let mut primitives = Vec::with_capacity(morphs.len());
        for (primitive, node, declared) in morphs {
            self.declared.insert(primitive, declared);
            primitives.push((primitive, node));
        }
        self.morphs.insert(root, primitives.into());
    }

    /// The primitives with morph targets of the model whose root is `root`.
    pub(crate) fn morphs_of(&self, root: ObjectId) -> Arc<[(ObjectId, usize)]> {
        self.morphs
            .get(&root)
            .cloned()
            .unwrap_or_else(|| Arc::new([]))
    }

    /// The weights `primitive` was declared with.
    pub(crate) fn declared_weights(&self, primitive: ObjectId) -> Option<Arc<[f32]>> {
        self.declared.get(&primitive).cloned()
    }

    /// Whether an animation of its model moves `node`.
    pub(crate) fn animates(&self, node: ObjectId) -> bool {
        self.nodes
            .get(&node)
            .and_then(|(root, index)| Some((self.plays.get(root)?, *index)))
            .is_some_and(|(plays, index)| plays.iter().any(|play| play.lens.moves(index)))
    }

    /// The local matrix an animation gives `node` at timeline second `time`:
    /// that of the latest play started by then that moves it, as the
    /// timeline evaluates it; `None` before any.
    pub(crate) fn local_at(&self, node: ObjectId, time: f64) -> Option<DMat4> {
        let (root, index) = self.nodes.get(&node)?;
        let play = self
            .plays
            .get(root)?
            .iter()
            .filter(|play| play.start <= time + 1e-9 && play.lens.moves(*index))
            .max_by(|a, b| a.start.total_cmp(&b.start))?;
        let progress = if play.duration > 0.0 {
            ((time - play.start) / play.duration).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let eased = play.rate_func.evaluate(progress);
        let pose = play.lens.pose(eased * play.duration);
        Some(pose.get(*index)?.to_mat4())
    }
}

/// Plays an animation of an imported model; the clip's target is the
/// model's root, whose [`GltfRig`] names the nodes it moves.
#[derive(Debug, Clone)]
pub struct GltfClipLens {
    animations: Arc<GltfAnimations>,
    play: ClipPlay,
    /// The play this one fades from over `transition` seconds, or the rest
    /// pose when there is none.
    previous: Option<ClipPlay>,
    transition: f64,
    /// The nodes either play moves, which this lens writes.
    nodes: Arc<[usize]>,
    /// The nodes whose weights either play moves, which this lens writes.
    morphed: Arc<[usize]>,
    /// The weights of every node's morph targets as the play starts, which
    /// a first play fades from: the file's, or those a script gave the
    /// model.
    rest_weights: Arc<[Vec<f64>]>,
    /// Those the model was declared with, which it shows before its first
    /// play.
    declared_weights: Arc<[Vec<f64>]>,
}

impl GltfClipLens {
    pub fn new(
        animations: Arc<GltfAnimations>,
        play: ClipPlay,
        previous: Option<ClipPlay>,
        transition: f64,
    ) -> Self {
        let transition = if transition.is_finite() {
            transition.max(0.0)
        } else {
            0.0
        };
        let mut nodes = animations.animated_nodes(play.clip);
        let mut morphed = animations.morphed_nodes(play.clip);
        if transition > 0.0
            && let Some(previous) = &previous
        {
            nodes.extend(animations.animated_nodes(previous.clip));
            nodes.sort_unstable();
            nodes.dedup();
            morphed.extend(animations.morphed_nodes(previous.clip));
            morphed.sort_unstable();
            morphed.dedup();
        }
        let rest_weights: Arc<[Vec<f64>]> = animations.weights.clone().into();
        Self {
            animations,
            play,
            previous,
            transition,
            nodes: nodes.into(),
            morphed: morphed.into(),
            declared_weights: Arc::clone(&rest_weights),
            rest_weights,
        }
    }

    /// This play of a model declared with the weights `declared` of every
    /// node's morph targets, which start the play at `rest`, instead of the
    /// file's.
    pub fn with_weights(mut self, declared: Vec<Vec<f64>>, rest: Vec<Vec<f64>>) -> Self {
        self.declared_weights = declared.into();
        self.rest_weights = rest.into();
        self
    }

    /// Whether the play writes the weights of node `node`'s morph targets.
    pub fn weighs(&self, node: usize) -> bool {
        self.morphed.contains(&node)
    }

    /// Whether the play writes the pose of node `node`.
    pub fn moves(&self, node: usize) -> bool {
        self.nodes.contains(&node)
    }

    /// The weights of every node's morph targets `elapsed` seconds into the
    /// play, faded from the previous play's as [`Self::pose`] fades poses.
    pub fn weights(&self, elapsed: f64) -> Vec<Vec<f64>> {
        let animations = &*self.animations;
        let mut weights = self.rest_weights.to_vec();
        if self.morphed.is_empty() {
            return weights;
        }
        animations.sample_weights(
            self.play.clip,
            self.play.clip_time(animations, elapsed),
            &mut weights,
        );
        if self.transition > 0.0 && elapsed < self.transition {
            let mut before = self.rest_weights.to_vec();
            if let Some(previous) = &self.previous {
                let since = self.play.start - previous.start + elapsed;
                animations.sample_weights(
                    previous.clip,
                    previous.clip_time(animations, since),
                    &mut before,
                );
            }
            let fade = (elapsed / self.transition).clamp(0.0, 1.0);
            let fade = fade * fade * (3.0 - 2.0 * fade);
            for &node in self.morphed.iter() {
                if let (Some(from), Some(to)) = (before.get(node), weights.get_mut(node)) {
                    for (to, from) in to.iter_mut().zip(from) {
                        *to = from + (*to - from) * fade;
                    }
                }
            }
        }
        weights
    }

    /// The pose of every node `elapsed` seconds into the play.
    pub fn pose(&self, elapsed: f64) -> Vec<NodePose> {
        let animations = &*self.animations;
        let mut pose = animations.rest.clone();
        animations.sample(
            self.play.clip,
            self.play.clip_time(animations, elapsed),
            &mut pose,
        );
        if self.transition > 0.0 && elapsed < self.transition {
            let mut before = animations.rest.clone();
            if let Some(previous) = &self.previous {
                // The previous play keeps running while this one fades in.
                let since = self.play.start - previous.start + elapsed;
                animations.sample(
                    previous.clip,
                    previous.clip_time(animations, since),
                    &mut before,
                );
            }
            let weight = (elapsed / self.transition).clamp(0.0, 1.0);
            // Ease the weight, as cross-fades between actions do.
            let weight = weight * weight * (3.0 - 2.0 * weight);
            for &node in self.nodes.iter() {
                if let (Some(from), Some(to)) = (before.get(node), pose.get(node)) {
                    pose[node] = from.lerp(to, weight);
                }
            }
        }
        pose
    }

    /// Write `pose` into the nodes this lens moves, and `weights` into the
    /// primitives of the nodes whose weights it moves.
    fn write(&self, world: &mut World, root: Entity, pose: &[NodePose], weights: &[Vec<f64>]) {
        let Some(rig) = world.get::<GltfRig>(root) else {
            return;
        };
        let entities = Arc::clone(&rig.nodes);
        let morphs = Arc::clone(&rig.morphs);
        for &(entity, node) in morphs.iter() {
            let (true, Some(weights)) = (self.morphed.contains(&node), weights.get(node)) else {
                continue;
            };
            let weights: Vec<f32> = weights.iter().map(|weight| *weight as f32).collect();
            if let Some(mut held) = world.get_mut::<gaanim_scene::MorphWeights>(entity)
                && held.0[..] != weights[..]
            {
                held.0 = weights.into();
            }
        }
        for &node in self.nodes.iter() {
            let (Some(&entity), Some(pose)) = (entities.get(node), pose.get(node)) else {
                continue;
            };
            let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) else {
                continue;
            };
            if transform.translation != pose.translation
                || transform.rotation != pose.rotation
                || transform.scale != pose.scale
            {
                transform.translation = pose.translation;
                transform.rotation = pose.rotation;
                transform.scale = pose.scale;
            }
        }
    }
}

/// Moves the weights of a primitive's morph targets from `from` to `to`,
/// as `animate.morph` does; before its first `morph`, the primitive shows
/// those it was `declared` with.
#[derive(Debug, Clone)]
pub struct MorphWeightsLens {
    pub declared: Arc<[f32]>,
    pub from: Arc<[f32]>,
    pub to: Arc<[f32]>,
}

impl gaanim_animation::AnimatableLens for MorphWeightsLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        let t = t as f32;
        let weights: Vec<f32> = self
            .from
            .iter()
            .zip(self.to.iter())
            .map(|(from, to)| from + (to - from) * t)
            .collect();
        if let Some(mut held) = world.get_mut::<gaanim_scene::MorphWeights>(entity)
            && held.0[..] != weights[..]
        {
            held.0 = weights.into();
        }
    }

    fn clone_box(&self) -> Box<dyn gaanim_animation::AnimatableLens> {
        Box::new(self.clone())
    }

    fn type_name(&self) -> &'static str {
        "MorphWeights"
    }

    /// Before its first `morph`, a primitive shows the weights it was
    /// declared with, as before its model's first animation: an animation
    /// between them replays over it.
    fn hold_channel(&self) -> Option<&'static str> {
        Some("morph_weights")
    }

    fn hold(&self, world: &mut World, entity: Entity, _initial: f64) {
        if let Some(mut held) = world.get_mut::<gaanim_scene::MorphWeights>(entity)
            && held.0[..] != self.declared[..]
        {
            held.0 = Arc::clone(&self.declared);
        }
    }

    fn history_free(&self) -> bool {
        true
    }
}

impl gaanim_animation::AnimatableLens for GltfClipLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        let elapsed = t * self.play.duration;
        let pose = self.pose(elapsed);
        let weights = self.weights(elapsed);
        self.write(world, entity, &pose, &weights);
    }

    fn clone_box(&self) -> Box<dyn gaanim_animation::AnimatableLens> {
        Box::new(self.clone())
    }

    fn type_name(&self) -> &'static str {
        "GltfAnimation"
    }

    /// Before its first play, a model shows its rest pose, with the weights
    /// it was declared with.
    fn hold_channel(&self) -> Option<&'static str> {
        Some("gltf_pose")
    }

    fn hold(&self, world: &mut World, entity: Entity, _initial: f64) {
        let rest = self.animations.rest.clone();
        self.write(world, entity, &rest, &self.declared_weights);
    }

    fn history_free(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_core::glam::{DQuat, DVec3};
    use gaanim_objects::gltf::{
        ChannelInterpolation, ChannelTarget, GltfChannel, GltfClip, GltfWeightsChannel,
    };

    /// One node with two morph targets, at rest at weights 0 and 0.5, and
    /// two animations that move it along x: to 1 in a second, and to -1.
    /// The first also takes its weights to 1 and 0.
    fn animations() -> Arc<GltfAnimations> {
        let clip = |name: &str, to: f64| GltfClip {
            name: name.to_owned(),
            duration: 1.0,
            channels: vec![GltfChannel {
                node: 0,
                target: ChannelTarget::Translation,
                interpolation: ChannelInterpolation::Linear,
                times: vec![0.0, 1.0],
                values: vec![[0.0; 4], [to, 0.0, 0.0, 0.0]],
            }],
            weights: Vec::new(),
        };
        let mut right = clip("Right", 1.0);
        right.weights.push(GltfWeightsChannel {
            node: 0,
            interpolation: ChannelInterpolation::Linear,
            times: vec![0.0, 1.0],
            values: vec![0.0, 0.0, 1.0, 0.0],
            count: 2,
        });
        Arc::new(GltfAnimations {
            rest: vec![NodePose {
                translation: DVec3::new(0.0, 5.0, 0.0),
                rotation: DQuat::IDENTITY,
                scale: DVec3::splat(2.0),
            }],
            weights: vec![vec![0.0, 0.5]],
            clips: vec![right, clip("Left", -1.0)],
        })
    }

    fn play(clip: usize, start: f64) -> ClipPlay {
        ClipPlay {
            clip,
            start,
            duration: 2.0,
            speed: 1.0,
            looped: false,
            reverse: false,
            offset: 0.0,
        }
    }

    /// The sample wolf, compiled: Idle then Walk, a second each, as
    /// `scene.play(..., duration=1)` plays them.
    fn compiled_wolf() -> (
        bevy::prelude::World,
        gaanim_timeline::timeline::Timeline,
        Entity,
        Arc<GltfAnimations>,
    ) {
        compiled_wolf_with(|canvas, wolf| {
            for (name, transition) in [("Idle", 0.0), ("Walk", 0.3)] {
                let play = wolf
                    .animation(name, None, 1.0, true, false, transition, 0.0)
                    .unwrap();
                canvas
                    .play_items_configured(vec![PlayItem::Animation(play)], Some(1.0), None)
                    .unwrap();
            }
        })
    }

    use crate::canvas::{DrawableHandle, PlayItem, SceneModel};

    /// The sample wolf, its plays scheduled by `schedule`, compiled.
    fn compiled_wolf_with(
        schedule: impl FnOnce(&mut SceneModel, &DrawableHandle),
    ) -> (
        bevy::prelude::World,
        gaanim_timeline::timeline::Timeline,
        Entity,
        Arc<GltfAnimations>,
    ) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/fixtures/assets/Wolf.glb");
        let mut canvas = SceneModel::new(640, 360);
        let wolf = canvas.gltf(&path, &Default::default()).unwrap();
        let animations = Arc::clone(
            &gaanim_objects::gltf::GltfModel::load(&path, &Default::default())
                .unwrap()
                .animations,
        );
        schedule(&mut canvas, &wolf);
        let mut world = bevy::prelude::World::new();
        world.insert_resource(gaanim_timeline::timeline::Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        let mut timeline = world
            .remove_resource::<gaanim_timeline::timeline::Timeline>()
            .unwrap();
        timeline.add_keyframe(
            0.0,
            gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
        );
        let root = world
            .query::<(Entity, &GltfRig)>()
            .iter(&world)
            .map(|(entity, _)| entity)
            .next()
            .expect("the model's root carries its rig");
        (world, timeline, root, animations)
    }

    /// The local transform of every node of the wolf's rig.
    fn poses(world: &World, root: Entity) -> Vec<SpatialTransform> {
        world
            .get::<GltfRig>(root)
            .unwrap()
            .nodes
            .iter()
            .map(|node| *world.get::<SpatialTransform>(*node).unwrap())
            .collect()
    }

    #[test]
    fn a_compiled_model_rigs_its_skin_and_its_plays_last_what_play_gives_them() {
        let (mut world, timeline, root, animations) = compiled_wolf();
        assert_eq!(world.get::<GltfRig>(root).unwrap().nodes.len(), 11);
        let skins: Vec<usize> = world
            .query::<&gaanim_scene::MeshSkin>()
            .iter(&world)
            .map(|skin| skin.joints.len())
            .collect();
        assert_eq!(skins, [9]);
        // Each play lasts the second `play` gives it, not the animation's.
        let mut plays: Vec<(f64, f64)> = timeline
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                gaanim_timeline::clip::ClipPayload::Animation(animation)
                    if matches!(
                        &animation.lens,
                        gaanim_timeline::clip::PropertyLensSpec::Dynamic(lens)
                            if lens.0.type_name() == "GltfAnimation"
                    ) =>
                {
                    Some((clip.start, clip.duration))
                }
                _ => None,
            })
            .collect();
        plays.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(plays, [(0.0, 1.0), (1.0, 1.0)]);
        assert_eq!(animations.clips.len(), 8);
    }

    #[test]
    fn seeks_land_on_the_same_pose_in_any_order() {
        let (mut world, mut timeline, root, animations) = compiled_wolf();
        // Before its plays, past them and in the fade between them.
        let times = [0.0, 0.6, 1.15, 1.8, 0.3, 1.8, 1.15, 0.6, 0.0];
        let mut seen: Vec<(f64, Vec<SpatialTransform>)> = Vec::new();
        for time in times {
            timeline.seek(&mut world, time);
            let pose = poses(&world, root);
            if let Some((_, first)) = seen.iter().find(|(at, _)| *at == time) {
                assert_eq!(first, &pose, "a seek to {time} after others");
            } else {
                seen.push((time, pose));
            }
        }
        // Mid Walk, a leg follows the animation, looped at Walk's length.
        let walk = animations.clip("Walk").unwrap();
        let leg = animations.clips[walk]
            .channels
            .iter()
            .find(|channel| channel.target == ChannelTarget::Rotation)
            .unwrap();
        let length = animations.clips[walk].duration;
        let expected = DQuat::from_array(leg.sample(0.8_f64.rem_euclid(length)));
        let pose = &seen.iter().find(|(at, _)| *at == 1.8).unwrap().1;
        assert!(pose[leg.node].rotation.dot(expected).abs() > 1.0 - 1e-9);
    }

    /// A play that starts while the model's launched one still runs takes
    /// over from it, fading from it: from then on, and after both end, the
    /// pose is the new play's alone.
    #[test]
    fn a_new_play_takes_over_from_a_launched_one() {
        let (mut world, mut timeline, root, animations) = compiled_wolf_with(|canvas, wolf| {
            let walk = wolf
                .animation("Walk", Some(4.0), 1.0, true, false, 0.0, 0.0)
                .unwrap();
            canvas
                .launch_composition_configured(
                    crate::canvas::Composition::leaf(PlayItem::Animation(walk)),
                    None,
                    None,
                )
                .unwrap();
            canvas.wait(1.0);
            let run = wolf
                .animation("Run", None, 1.0, true, false, 0.3, 0.0)
                .unwrap();
            canvas
                .play_items_configured(vec![PlayItem::Animation(run)], Some(1.0), None)
                .unwrap();
            // The walk stopped where the run took over: nothing runs past
            // the scene's end.
            assert_eq!(canvas.launched_past_end_warning(), None);
        });
        let run = ClipPlay {
            clip: animations.clip("Run").unwrap(),
            start: 1.0,
            duration: 1.0,
            speed: 1.0,
            looped: true,
            reverse: false,
            offset: 0.0,
        };
        let alone = GltfClipLens::new(Arc::clone(&animations), run, None, 0.0);
        let moved = animations.animated_nodes(alone.play.clip);
        for time in [1.5, 1.95, 3.0] {
            timeline.seek(&mut world, time);
            let pose = poses(&world, root);
            let expected = alone.pose((time - 1.0_f64).min(1.0));
            for &node in &moved {
                assert!(
                    pose[node].translation.distance(expected[node].translation) < 1e-9
                        && pose[node].rotation.dot(expected[node].rotation).abs() > 1.0 - 1e-9,
                    "node {node} at {time}"
                );
            }
        }
    }

    #[test]
    fn plays_wrap_hold_and_reverse_the_animation_time() {
        let animations = animations();
        let mut looped = play(0, 0.0);
        looped.looped = true;
        assert!((looped.clip_time(&animations, 1.25) - 0.25).abs() < 1e-12);
        assert_eq!(play(0, 0.0).clip_time(&animations, 1.25), 1.0);
        let mut reversed = play(0, 0.0);
        reversed.reverse = true;
        assert!((reversed.clip_time(&animations, 0.25) - 0.75).abs() < 1e-12);
        let mut fast = play(0, 0.0);
        fast.speed = 2.0;
        fast.offset = 0.25;
        assert_eq!(fast.clip_time(&animations, 0.25), 0.75);
    }

    #[test]
    fn a_play_moves_only_what_its_animation_moves() {
        let lens = GltfClipLens::new(animations(), play(0, 0.0), None, 0.0);
        let pose = lens.pose(0.5);
        // The translation follows the channel, which sets all of it; the
        // scale, which no channel moves, keeps the rest pose.
        assert_eq!(pose[0].translation, DVec3::new(0.5, 0.0, 0.0));
        assert_eq!(pose[0].scale, DVec3::splat(2.0));
    }

    #[test]
    fn a_play_sets_the_weights_its_animation_moves_and_fades_them_in() {
        let lens = GltfClipLens::new(animations(), play(0, 0.0), None, 0.0);
        assert_eq!(lens.weights(0.5), [vec![0.5, 0.0]]);
        // A play without weights channels leaves the rest weights.
        let left = GltfClipLens::new(animations(), play(1, 0.0), None, 0.0);
        assert_eq!(left.weights(0.5), [vec![0.0, 0.5]]);
        // The first play fades from the rest weights: halfway through its
        // fade, at 0.2 s, the weights are halfway from rest to 0.2 and 0.
        let faded = GltfClipLens::new(animations(), play(0, 0.0), None, 0.4);
        let halfway = &faded.weights(0.2)[0];
        assert!((halfway[0] - 0.1).abs() < 1e-12 && (halfway[1] - 0.25).abs() < 1e-12);
    }

    /// Weights set with `morph` start the primitive, `animate.morph`
    /// moves one of them alone, and a `morph` after the declaration cuts
    /// at the cursor; seeks land on the same weights in any order.
    #[test]
    fn morph_sets_and_animates_the_weights_of_named_targets() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/fixtures/assets/Blob.glb");
        let mut canvas = SceneModel::new(640, 360);
        let blob = canvas.gltf(&path, &Default::default()).unwrap();
        assert_eq!(blob.morph_targets(), ["Squash", "Spikes", "Lean"]);
        blob.morph("Squash", 0.4).unwrap();
        let spikes = blob
            .animate()
            .morph("Spikes", 1.0)
            .unwrap()
            .rate_func(gaanim_math::RateFunc::Linear);
        canvas
            .play_items_configured(vec![PlayItem::Animation(spikes)], Some(1.0), None)
            .unwrap();
        blob.morph("Squash", 0.0).unwrap();
        canvas.wait(1.0);
        // Names the file does not give, drawables without targets and
        // weights that are not numbers are refused.
        assert_eq!(
            blob.morph("Twist", 1.0).unwrap_err(),
            crate::canvas::MorphTargetError::Unknown {
                name: "Twist".into(),
                available: "Squash, Spikes, Lean".into(),
            }
        );
        assert_eq!(
            canvas.circle(1.0).morph("Squash", 1.0).unwrap_err(),
            crate::canvas::MorphTargetError::NoTargets
        );
        assert_eq!(
            blob.animate().morph("Squash", f64::NAN).unwrap_err(),
            crate::canvas::MorphTargetError::InvalidWeight
        );

        let mut world = bevy::prelude::World::new();
        world.insert_resource(gaanim_timeline::timeline::Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        let mut timeline = world
            .remove_resource::<gaanim_timeline::timeline::Timeline>()
            .unwrap();
        timeline.add_keyframe(
            0.0,
            gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
        );
        let mut weights = |time: f64| -> Vec<f32> {
            timeline.seek(&mut world, time);
            let held: Vec<Vec<f32>> = world
                .query::<&gaanim_scene::MorphWeights>()
                .iter(&world)
                .map(|weights| weights.0.to_vec())
                .collect();
            assert_eq!(held.len(), 1);
            held[0].iter().map(|w| (w * 1e4).round() / 1e4).collect()
        };
        assert_eq!(weights(0.0), [0.4, 0.0, 0.0]);
        assert_eq!(weights(0.5), [0.4, 0.5, 0.0]);
        assert_eq!(weights(1.5), [0.0, 1.0, 0.0]);
        assert_eq!(weights(0.25), [0.4, 0.25, 0.0]);
        assert_eq!(weights(0.0), [0.4, 0.0, 0.0]);
        assert_eq!(weights(1.5), [0.0, 1.0, 0.0]);
    }

    /// Weights set with `morph` hold until an animation of the model moves
    /// them, and a `morph` after the animation starts where it ends.
    #[test]
    fn animations_and_morph_take_over_each_others_weights() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/fixtures/assets/Blob.glb");
        let mut canvas = SceneModel::new(640, 360);
        let blob = canvas.gltf(&path, &Default::default()).unwrap();
        blob.morph("Squash", 0.4).unwrap();
        canvas.wait(1.0);
        // Wobble grows the spikes in full at its middle and ends round.
        let wobble = blob
            .animation("Wobble", None, 1.0, false, false, 0.0, 0.0)
            .unwrap();
        canvas
            .play_items_configured(vec![PlayItem::Animation(wobble)], Some(2.0), None)
            .unwrap();
        let spikes = blob
            .animate()
            .morph("Spikes", 1.0)
            .unwrap()
            .rate_func(gaanim_math::RateFunc::Linear);
        canvas
            .play_items_configured(vec![PlayItem::Animation(spikes)], Some(1.0), None)
            .unwrap();

        let mut world = bevy::prelude::World::new();
        world.insert_resource(gaanim_timeline::timeline::Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        let mut timeline = world
            .remove_resource::<gaanim_timeline::timeline::Timeline>()
            .unwrap();
        timeline.add_keyframe(
            0.0,
            gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
        );
        let mut weights = |time: f64| -> Vec<f32> {
            timeline.seek(&mut world, time);
            let held: Vec<Vec<f32>> = world
                .query::<&gaanim_scene::MorphWeights>()
                .iter(&world)
                .map(|weights| weights.0.to_vec())
                .collect();
            held[0]
                .iter()
                .map(|w| (w * 1e4).round() / 1e4 + 0.0)
                .collect()
        };
        assert_eq!(weights(0.5), [0.4, 0.0, 0.0]);
        assert_eq!(weights(2.0), [0.0, 1.0, 0.0]);
        assert_eq!(weights(3.5), [0.0, 0.5, 0.0]);
        assert_eq!(weights(0.5), [0.4, 0.0, 0.0]);
    }

    #[test]
    fn a_transition_fades_from_the_previous_play_as_it_keeps_running() {
        let previous = play(0, 0.0);
        let lens = GltfClipLens::new(animations(), play(1, 0.5), Some(previous), 0.4);
        // At its start it shows the previous play, half a second in.
        assert!((lens.pose(0.0)[0].translation.x - 0.5).abs() < 1e-12);
        // Halfway through the fade, between the two.
        let halfway = lens.pose(0.2)[0].translation.x;
        let (from, to) = (0.7, -0.2);
        assert!((halfway - (from + to) * 0.5).abs() < 1e-12, "{halfway}");
        // After it, its own animation alone.
        assert!((lens.pose(0.6)[0].translation.x + 0.6).abs() < 1e-12);
    }
}
