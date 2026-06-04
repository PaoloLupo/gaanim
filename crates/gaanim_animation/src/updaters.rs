use bevy::prelude::{Commands, Component, Entity, Query, Res};
use std::sync::Arc;
use crate::tween::DeltaTime;

/// Componente que define una función de actualización continua para una entidad
/// que se ejecuta cada frame durante SceneSet::Updaters.
#[derive(Component)]
pub struct Updater {
    /// La función de actualización que se ejecuta cada frame.
    /// Recibe: delta_time (dt), total_elapsed, entity y Commands.
    /// Retorna `true` para seguir ejecutándose, `false` para ser removido automáticamente.
    pub func: Arc<dyn Fn(f64, f64, Entity, &mut Commands) -> bool + Send + Sync>,
    /// Tiempo total acumulado desde que se añadió este updater.
    pub elapsed: f64,
    /// Si es verdadero, el updater se pausa si la simulación está pausada.
    pub time_based: bool,
}

impl Updater {
    /// Crea una nueva instancia de Updater a partir de un closure.
    pub fn new(func: impl Fn(f64, f64, Entity, &mut Commands) -> bool + Send + Sync + 'static) -> Self {
        Self {
            func: Arc::new(func),
            elapsed: 0.0,
            time_based: true,
        }
    }
}

/// Sistema Bevy que ejecuta todos los updaters activos en las entidades cada frame.
pub fn updater_system(
    mut commands: Commands,
    dt: Res<DeltaTime>,
    mut query: Query<(Entity, &mut Updater)>,
) {
    let dt_val = dt.dt;
    let mut to_remove = Vec::new();

    for (entity, mut updater) in &mut query {
        updater.elapsed += dt_val;
        let keep = (updater.func)(dt_val, updater.elapsed, entity, &mut commands);
        if !keep {
            to_remove.push(entity);
        }
    }

    for entity in to_remove {
        commands.entity(entity).remove::<Updater>();
    }
}
