//! Imported glTF models on the timeline: their nodes' rest poses, the rig
//! that skins their meshes, and their animations played as clips.
//!
//! A play of an animation ([`ClipPlay`]) is a timeline clip whose lens
//! ([`GltfClipLens`]) writes the pose of the model's nodes as a function of
//! time alone: seeks land exactly, and a recording keeps the matrices it
//! produced, so players never evaluate the animation. A play with a
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
/// [`gaanim_objects::gltf::GltfModel::nodes`], and its skinned primitives.
#[derive(Debug, Clone)]
pub struct GltfRigSpec {
    pub nodes: Arc<[ObjectId]>,
    /// Each skinned primitive, the nodes of its joints and their inverse
    /// bind matrices.
    pub skins: Vec<(ObjectId, Arc<[usize]>, Arc<[DMat4]>)>,
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
        }
    }
}

/// The entities of an imported model's nodes, on its root, which its
/// animations move.
#[derive(Component, Debug, Clone)]
pub struct GltfRig {
    pub nodes: Arc<[Entity]>,
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
        if transition > 0.0
            && let Some(previous) = &previous
        {
            nodes.extend(animations.animated_nodes(previous.clip));
            nodes.sort_unstable();
            nodes.dedup();
        }
        Self {
            animations,
            play,
            previous,
            transition,
            nodes: nodes.into(),
        }
    }

    /// Whether the play writes the pose of node `node`.
    pub fn moves(&self, node: usize) -> bool {
        self.nodes.contains(&node)
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

    /// Write `pose` into the nodes this lens moves.
    fn write(&self, world: &mut World, root: Entity, pose: &[NodePose]) {
        let Some(rig) = world.get::<GltfRig>(root) else {
            return;
        };
        let entities = Arc::clone(&rig.nodes);
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

impl gaanim_animation::AnimatableLens for GltfClipLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        let pose = self.pose(t * self.play.duration);
        self.write(world, entity, &pose);
    }

    fn clone_box(&self) -> Box<dyn gaanim_animation::AnimatableLens> {
        Box::new(self.clone())
    }

    fn type_name(&self) -> &'static str {
        "GltfAnimation"
    }

    /// Before its first play, a model shows its rest pose.
    fn hold_channel(&self) -> Option<&'static str> {
        Some("gltf_pose")
    }

    fn hold(&self, world: &mut World, entity: Entity, _initial: f64) {
        let rest = self.animations.rest.clone();
        self.write(world, entity, &rest);
    }

    fn history_free(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_core::glam::{DQuat, DVec3};
    use gaanim_objects::gltf::{ChannelInterpolation, ChannelTarget, GltfChannel, GltfClip};

    /// One node, and two animations that move it along x: to 1 in a
    /// second, and to -1.
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
        };
        Arc::new(GltfAnimations {
            rest: vec![NodePose {
                translation: DVec3::new(0.0, 5.0, 0.0),
                rotation: DQuat::IDENTITY,
                scale: DVec3::splat(2.0),
            }],
            clips: vec![clip("Right", 1.0), clip("Left", -1.0)],
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
