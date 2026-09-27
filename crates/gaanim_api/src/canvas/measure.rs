//! Measuring drawables while a scene is being authored.

use std::sync::{Arc, Mutex};

use bevy::ecs::world::CommandQueue;
use bevy::prelude::{Commands, World};
use gaanim_math::Bounds3D;
use gaanim_timeline::snapshot::WorldSnapshot;
use gaanim_timeline::timeline::Timeline;

use super::{DrawableHandle, SceneModel};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BoundsError {
    #[error("the drawable belongs to another Scene")]
    ForeignScene,
    #[error("the drawable's Scene is no longer alive")]
    SceneDropped,
    #[error("the drawable has no geometry at the current cursor")]
    Empty,
}

impl SceneModel {
    /// Share this scene so its drawables can measure themselves with
    /// [`DrawableHandle::bounds`].
    pub fn into_shared(self) -> Arc<Mutex<SceneModel>> {
        let state = self.state.clone();
        let shared = Arc::new(Mutex::new(self));
        state.lock().expect("canvas state poisoned").owner = Some(Arc::downgrade(&shared));
        shared
    }

    /// Scene-space box of `handle` and its descendants at the authoring
    /// cursor, as they would render there: layout, transforms, text shaping
    /// and animations that ended before the cursor all count. Geometry that
    /// reactive updaters rebuild every frame is measured as declared.
    ///
    /// This compiles the scene authored so far, so it costs a compilation.
    pub fn bounds_of(&self, handle: &DrawableHandle) -> Result<Bounds3D, BoundsError> {
        if !self.owns(handle) {
            return Err(BoundsError::ForeignScene);
        }
        let time = self.current_time();
        let segments = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .len();

        let mut fonts = gaanim_text::font::FontRegistry::without_system_fonts();
        self.register_theme_fonts(&mut fonts);
        let config = self.themed_text_config();
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut timeline = Timeline::new();
        let checkpoint = {
            let mut commands = Commands::new(&mut queue, &world);
            self.compile_resumable(
                &mut commands,
                &mut timeline,
                &fonts,
                &config,
                None,
                Some(segments),
                Vec::new(),
            )
        };
        queue.apply(&mut world);
        let runtime = checkpoint
            .and_then(|checkpoint| checkpoint.cursor.runtime_id(handle.id))
            .ok_or(BoundsError::Empty)?;
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, time);

        let entity = world
            .query::<(bevy::prelude::Entity, &gaanim_scene::MobjectId)>()
            .iter(&world)
            .find_map(|(entity, id)| (id.0 == runtime).then_some(entity))
            .ok_or(BoundsError::Empty)?;
        let bounds = gaanim_animation::updaters::resolve_entity_bounds(entity, &world)
            .ok_or(BoundsError::Empty)?;
        if !(bounds.min.is_finite() && bounds.max.is_finite()) || bounds.min.x > bounds.max.x {
            return Err(BoundsError::Empty);
        }
        Ok(bounds)
    }
}

impl DrawableHandle {
    /// Scene-space box of this drawable at its scene's authoring cursor; see
    /// [`SceneModel::bounds_of`]. The scene must have been shared with
    /// [`SceneModel::into_shared`].
    pub fn bounds(&self) -> Result<Bounds3D, BoundsError> {
        let owner = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .owner
            .clone()
            .ok_or(BoundsError::SceneDropped)?;
        let scene = owner.upgrade().ok_or(BoundsError::SceneDropped)?;
        let scene = scene.lock().expect("scene poisoned").clone();
        scene.bounds_of(self)
    }
}
