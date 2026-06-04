use bevy::prelude::{Component, Entity, World};
use std::sync::Arc;
use std::sync::Mutex;
use gaanim_math::SpatialTransform;
use gaanim_core::glam::DVec3;
use crate::tween::DeltaTime;

/// Componente que define una función de actualización continua para una entidad.
/// Se ejecuta cada frame durante SceneSet::Updaters.
#[derive(Component)]
pub struct Updater {
    /// La función de actualización que recibe delta time, tiempo total transcurrido,
    /// la entidad del updater y acceso exclusivo al World de Bevy.
    /// Retorna `true` para seguir ejecutándose, `false` para ser removido automáticamente.
    pub func: Arc<dyn Fn(f64, f64, Entity, &mut World) -> bool + Send + Sync>,
    /// Tiempo total acumulado desde que se añadió este updater.
    pub elapsed: f64,
    /// Si es verdadero, el updater se pausa si la simulación está pausada.
    pub time_based: bool,
}

impl Updater {
    /// Crea una nueva instancia de Updater a partir de un closure.
    pub fn new(func: impl Fn(f64, f64, Entity, &mut World) -> bool + Send + Sync + 'static) -> Self {
        Self {
            func: Arc::new(func),
            elapsed: 0.0,
            time_based: true,
        }
    }
}

/// Sistema Bevy que ejecuta todos los updaters activos con acceso exclusivo al World.
pub fn updater_system(world: &mut World) {
    let dt = world.get_resource::<DeltaTime>().map(|d| d.dt).unwrap_or(0.0);

    let mut updates = Vec::new();
    
    // Consultamos los updaters para extraer sus Arcs y ejecutarlos sin colisiones de préstamos de Bevy.
    let mut query = world.query::<(Entity, &mut Updater)>();
    for (entity, mut updater) in query.iter_mut(world) {
        updater.elapsed += dt;
        updates.push((entity, updater.func.clone(), updater.elapsed));
    }

    let mut to_remove = Vec::new();
    for (entity, func, elapsed) in updates {
        // Ejecutar la clausura con acceso exclusivo al World.
        let keep = func(dt, elapsed, entity, world);
        if !keep {
            to_remove.push(entity);
        }
    }

    // Remover los updaters que terminaron su ciclo
    for entity in to_remove {
        if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
            entity_mut.remove::<Updater>();
        }
    }
}

// ==========================================
// PRESET UPDATERS (Funciones de ayuda)
// ==========================================

/// Hace oscilar la posición Y de la entidad con una onda senoidal (efecto de flotación).
pub fn bob_updater(amplitude: f64, frequency: f64) -> Updater {
    let initial_y = Mutex::new(None);
    Updater::new(move |_dt, elapsed, entity, world| {
        let mut y_cache = initial_y.lock().unwrap();
        let y0 = match *y_cache {
            Some(val) => val,
            None => {
                let current_y = world.get::<SpatialTransform>(entity)
                    .map(|t| t.translation.y)
                    .unwrap_or(0.0);
                *y_cache = Some(current_y);
                current_y
            }
        };

        if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
            let offset = amplitude * (elapsed * frequency * 2.0 * std::f64::consts::PI).sin();
            transform.translation.y = y0 + offset;
        }
        true
    })
}

/// Rota continuamente la entidad sobre su eje Z.
pub fn rotate_updater(speed: f64) -> Updater {
    Updater::new(move |dt, _elapsed, entity, world| {
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
            let delta_rot = gaanim_core::glam::DQuat::from_rotation_z(speed * dt);
            transform.rotation = transform.rotation * delta_rot;
        }
        true
    })
}

/// Hace orbitar la entidad alrededor de un punto central con un radio y velocidad dados.
pub fn orbit_updater(center: DVec3, radius: f64, speed: f64) -> Updater {
    Updater::new(move |_dt, elapsed, entity, world| {
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
            let angle = elapsed * speed;
            let x = center.x + radius * angle.cos();
            let y = center.y + radius * angle.sin();
            transform.translation = DVec3::new(x, y, transform.translation.z);
        }
        true
    })
}

/// Hace oscilar la escala de la entidad entre un mínimo y un máximo.
pub fn pulse_updater(min_scale: f64, max_scale: f64, frequency: f64) -> Updater {
    let initial_scale = Mutex::new(None);
    Updater::new(move |_dt, elapsed, entity, world| {
        let mut scale_cache = initial_scale.lock().unwrap();
        let s0 = match *scale_cache {
            Some(val) => val,
            None => {
                let current_scale = world.get::<SpatialTransform>(entity)
                    .map(|t| t.scale)
                    .unwrap_or(DVec3::ONE);
                *scale_cache = Some(current_scale);
                current_scale
            }
        };

        if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
            let half_diff = (max_scale - min_scale) / 2.0;
            let mid = min_scale + half_diff;
            let multiplier = mid + half_diff * (elapsed * frequency * 2.0 * std::f64::consts::PI).sin();
            transform.scale = s0 * multiplier;
        }
        true
    })
}

/// Hace que la entidad siga la posición de otra entidad con un desplazamiento y suavizado.
pub fn follow_updater(target: Entity, offset: DVec3, smoothing: f64) -> Updater {
    Updater::new(move |dt, _elapsed, entity, world| {
        let target_pos = world.get::<SpatialTransform>(target)
            .map(|t| t.translation);

        if let Some(pos) = target_pos {
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                let target_dest = pos + offset;
                if smoothing <= 0.0 {
                    transform.translation = target_dest;
                } else {
                    let t = (1.0 - (-dt / smoothing).exp()).clamp(0.0, 1.0);
                    transform.translation = transform.translation.lerp(target_dest, t);
                }
            }
        }
        true
    })
}
