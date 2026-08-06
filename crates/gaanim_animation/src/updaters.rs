use crate::tween::DeltaTime;
use bevy::prelude::{Component, Entity, World};
use gaanim_core::glam::DVec3;
use gaanim_core::kurbo::BezPath;
use gaanim_math::SpatialTransform;
use gaanim_scene::Path2D;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(bevy::prelude::Resource, Debug, Clone, Copy)]
pub struct PlaybackState {
    pub is_playing: bool,
    pub scaled_dt: f64,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            is_playing: true,
            scaled_dt: 0.0,
        }
    }
}

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
    /// Tiempo de corte opcional: a partir de aquí el updater queda congelado.
    pub stop_at: Option<f64>,
    /// Si es verdadero, el updater se pausa si la simulación está pausada.
    pub time_based: bool,
}

impl Updater {
    /// Crea una nueva instancia de Updater a partir de un closure.
    pub fn new(
        func: impl Fn(f64, f64, Entity, &mut World) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            func: Arc::new(func),
            elapsed: 0.0,
            stop_at: None,
            time_based: true,
        }
    }
}

/// Sistema Bevy que ejecuta todos los updaters activos con acceso exclusivo al World.
pub fn updater_system(world: &mut World) {
    let dt = world
        .get_resource::<PlaybackState>()
        .map(|s| s.scaled_dt)
        .filter(|dt| *dt > 0.0)
        .or_else(|| world.get_resource::<DeltaTime>().map(|d| d.dt))
        .unwrap_or(0.0);
    let is_playing = world
        .get_resource::<PlaybackState>()
        .map(|s| s.is_playing)
        .unwrap_or(true);

    let mut updates = Vec::new();

    // Consultamos los updaters para extraer sus Arcs y ejecutarlos sin colisiones de préstamos de Bevy.
    let mut query = world.query::<(Entity, &mut Updater)>();
    for (entity, mut updater) in query.iter_mut(world) {
        if updater.time_based && !is_playing {
            continue;
        }
        let previous_elapsed = updater.elapsed;
        let next_elapsed = if let Some(stop_at) = updater.stop_at {
            (previous_elapsed + dt).min(stop_at)
        } else {
            previous_elapsed + dt
        };
        updater.elapsed = next_elapsed;
        updates.push((
            entity,
            updater.func.clone(),
            next_elapsed - previous_elapsed,
            next_elapsed,
        ));
    }

    let mut to_remove = Vec::new();
    for (entity, func, effective_dt, elapsed) in updates {
        // Ejecutar la clausura con acceso exclusivo al World.
        let keep = func(effective_dt, elapsed, entity, world);
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
                let current_y = world
                    .get::<SpatialTransform>(entity)
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
                let current_scale = world
                    .get::<SpatialTransform>(entity)
                    .map(|t| t.scale)
                    .unwrap_or(DVec3::ONE);
                *scale_cache = Some(current_scale);
                current_scale
            }
        };

        if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
            let half_diff = (max_scale - min_scale) / 2.0;
            let mid = min_scale + half_diff;
            let multiplier =
                mid + half_diff * (elapsed * frequency * 2.0 * std::f64::consts::PI).sin();
            transform.scale = s0 * multiplier;
        }
        true
    })
}

/// Hace que la entidad siga la posición de otra entidad con un desplazamiento y suavizado.
pub fn follow_updater(target: Entity, offset: DVec3, smoothing: f64) -> Updater {
    Updater::new(move |dt, _elapsed, entity, world| {
        let target_pos = world.get::<SpatialTransform>(target).map(|t| t.translation);

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

/// Mueve la posición X de la entidad a velocidad constante cada frame.
/// Útil para crear un "dot proyección" que avanza horizontalmente.
pub fn advance_x_updater(speed: f64) -> Updater {
    let initial_x = Mutex::new(None);
    Updater::new(move |_dt, elapsed, entity, world| {
        let mut x_cache = initial_x.lock().unwrap();
        let x0 = match *x_cache {
            Some(val) => val,
            None => {
                let current_x = world
                    .get::<SpatialTransform>(entity)
                    .map(|t| t.translation.x)
                    .unwrap_or(0.0);
                *x_cache = Some(current_x);
                current_x
            }
        };

        if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
            transform.translation.x = x0 + speed * elapsed;
        }
        true
    })
}

/// Componente que rastrea la posición de una entidad y genera un Path2D continuo de su trayectoria.
#[derive(Component)]
pub struct TracedPath {
    /// La entidad objetivo cuya posición se rastrea.
    pub source: Entity,
    /// Historial de puntos acumulados.
    pub points: Vec<DVec3>,
    /// Límite máximo de puntos a conservar (None para ilimitados).
    pub max_points: Option<usize>,
    /// Distancia mínima para añadir un nuevo punto.
    pub min_distance: f64,
    /// Tasa de disipación.
    pub dissipation: f64,
}

impl TracedPath {
    /// Crea un nuevo componente TracedPath
    pub fn new(source: Entity, min_distance: f64, max_points: Option<usize>) -> Self {
        Self {
            source,
            points: Vec::new(),
            max_points,
            min_distance,
            dissipation: 0.0,
        }
    }
}

/// Sistema exclusivo que lee la posición del source de cada TracedPath y regenera su Path2D.
pub fn traced_path_system(world: &mut World) {
    // 1. Extraemos de forma inmutable los datos de los TracedPath que necesitamos
    let mut trace_jobs = Vec::new();
    let mut query = world.query::<(Entity, &TracedPath)>();
    for (trace_entity, traced_path) in query.iter(world) {
        trace_jobs.push((
            trace_entity,
            traced_path.source,
            traced_path.min_distance,
            traced_path.max_points,
        ));
    }

    let mut trace_updates = Vec::new();

    // 2. Procesamos cada trace_job consultando el world de forma secuencial y limpia
    for (trace_entity, source_entity, min_distance, max_points) in trace_jobs {
        let source_pos = world
            .get::<SpatialTransform>(source_entity)
            .map(|t| t.translation);

        if let Some(pos) = source_pos {
            // Obtenemos acceso mutable al TracedPath específico de forma aislada
            if let Some(mut traced_path) = world.get_mut::<TracedPath>(trace_entity) {
                let should_add = match traced_path.points.last() {
                    Some(last_point) => last_point.distance(pos) >= min_distance,
                    None => true,
                };

                if should_add {
                    traced_path.points.push(pos);
                    if let Some(max) = max_points {
                        if traced_path.points.len() > max {
                            traced_path.points.remove(0);
                        }
                    }

                    // Regenerar el path
                    let mut path = gaanim_core::kurbo::BezPath::new();
                    if !traced_path.points.is_empty() {
                        path.move_to(gaanim_core::kurbo::Point::new(
                            traced_path.points[0].x,
                            traced_path.points[0].y,
                        ));
                        for pt in &traced_path.points[1..] {
                            path.line_to(gaanim_core::kurbo::Point::new(pt.x, pt.y));
                        }
                    }
                    trace_updates.push((trace_entity, path));
                }
            }
        }
    }

    // 3. Aplicamos los nuevos paths
    for (trace_entity, path) in trace_updates {
        if let Some(mut path_comp) = world.get_mut::<Path2D>(trace_entity) {
            path_comp.0 = std::sync::Arc::new(path);
        }
    }
}

// ==========================================
// TRACKING LINE — línea reactiva entre dos endpoints
// ==========================================

/// Un endpoint de una `TrackingLine`: posición estática o entidad dinámica.
#[derive(Debug, Clone)]
pub enum TrackingEndpoint {
    /// Posición fija en el espacio.
    Static(DVec3),
    /// Posición del centro de una entidad (lee SpatialTransform cada frame).
    Entity(Entity),
}

/// Componente que regenera un `Path2D` de línea recta entre dos endpoints cada frame.
///
/// A diferencia de una animación de línea (que interpola de A a B en una duración),
/// `TrackingLine` actualiza continuamente sus endpoints para seguir entidades.
#[derive(Component)]
pub struct TrackingLine {
    pub from: TrackingEndpoint,
    pub to: TrackingEndpoint,
}

impl TrackingLine {
    pub fn new(from: TrackingEndpoint, to: TrackingEndpoint) -> Self {
        Self { from, to }
    }
}

/// Sistema exclusivo que resuelve los endpoints de cada TrackingLine y regenera su Path2D.
pub fn tracking_line_system(world: &mut World) {
    let mut updates = Vec::new();

    let mut query = world.query::<(Entity, &TrackingLine)>();
    for (entity, line) in query.iter(world) {
        let from_pos = resolve_endpoint(&line.from, world);
        let to_pos = resolve_endpoint(&line.to, world);

        if let (Some(from), Some(to)) = (from_pos, to_pos) {
            let mut path = BezPath::new();
            path.move_to(gaanim_core::kurbo::Point::new(from.x, from.y));
            path.line_to(gaanim_core::kurbo::Point::new(to.x, to.y));
            updates.push((entity, path));
        }
    }

    for (entity, path) in updates {
        if let Some(mut path_comp) = world.get_mut::<Path2D>(entity) {
            path_comp.0 = std::sync::Arc::new(path);
        }
    }
}

fn resolve_endpoint(ep: &TrackingEndpoint, world: &World) -> Option<DVec3> {
    match ep {
        TrackingEndpoint::Static(pos) => Some(*pos),
        TrackingEndpoint::Entity(entity) => world
            .get::<SpatialTransform>(*entity)
            .map(|t| t.translation),
    }
}
