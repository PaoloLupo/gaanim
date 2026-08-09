use crate::tween::DeltaTime;
use bevy::prelude::{Component, Entity, World};
use gaanim_core::glam::DVec3;
use gaanim_core::kurbo::BezPath;
use gaanim_math::SpatialTransform;
use gaanim_scene::Path2D;
use std::sync::Arc;
use std::sync::Mutex;

type UpdaterFn = Arc<dyn Fn(f64, f64, Entity, &mut World) -> bool + Send + Sync>;
type UpdaterResetFn = Arc<dyn Fn(Entity, &mut World) -> bool + Send + Sync>;

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
#[derive(Component, Clone)]
pub struct Updater {
    /// La función de actualización que recibe delta time, tiempo total transcurrido,
    /// la entidad del updater y acceso exclusivo al World de Bevy.
    /// Retorna `true` para seguir ejecutándose, `false` para ser removido automáticamente.
    pub func: UpdaterFn,
    /// Tiempo total acumulado desde que se añadió este updater.
    pub elapsed: f64,
    /// Tiempo de corte opcional: a partir de aquí el updater queda congelado.
    pub stop_at: Option<f64>,
    /// Si es verdadero, el updater se pausa si la simulación está pausada.
    pub time_based: bool,
    /// Paso fijo de una simulación determinista. `None` conserva el updater
    /// tradicional, evaluado una vez por frame.
    fixed_dt: Option<f64>,
    /// Tiempo efectivamente integrado por una simulación de paso fijo.
    simulation_elapsed: f64,
    /// Fracción de tiempo pendiente, menor que `fixed_dt`.
    accumulator: f64,
    /// Reinicia el estado externo antes de reconstruir una simulación en un seek.
    reset: Option<UpdaterResetFn>,
    /// Posición local inicial restaurada antes de reproducir una simulación.
    initial_translation: Option<DVec3>,
}

impl std::fmt::Debug for Updater {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Updater")
            .field("elapsed", &self.elapsed)
            .field("stop_at", &self.stop_at)
            .field("time_based", &self.time_based)
            .field("fixed_dt", &self.fixed_dt)
            .field("simulation_elapsed", &self.simulation_elapsed)
            .field("accumulator", &self.accumulator)
            .field("has_reset", &self.reset.is_some())
            .field("initial_translation", &self.initial_translation)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
#[error("fixed_dt must be finite and greater than zero")]
pub struct InvalidFixedStep;

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
            fixed_dt: None,
            simulation_elapsed: 0.0,
            accumulator: 0.0,
            reset: None,
            initial_translation: None,
        }
    }

    /// Crea un updater de simulación determinista.
    ///
    /// La función `func` se ejecuta en subpasos constantes de `fixed_dt`,
    /// independientemente del frame rate. `reset` debe restaurar cualquier estado
    /// externo capturado por `func`; se invoca antes de reproducir desde cero
    /// durante un seek de la timeline.
    pub fn new_simulation(
        func: impl Fn(f64, f64, Entity, &mut World) -> bool + Send + Sync + 'static,
        reset: impl Fn(Entity, &mut World) -> bool + Send + Sync + 'static,
        fixed_dt: f64,
    ) -> Result<Self, InvalidFixedStep> {
        if !fixed_dt.is_finite() || fixed_dt <= 0.0 {
            return Err(InvalidFixedStep);
        }

        Ok(Self {
            func: Arc::new(func),
            elapsed: 0.0,
            stop_at: None,
            time_based: true,
            fixed_dt: Some(fixed_dt),
            simulation_elapsed: 0.0,
            accumulator: 0.0,
            reset: Some(Arc::new(reset)),
            initial_translation: None,
        })
    }
}

#[derive(Clone)]
struct UpdaterJob {
    entity: Entity,
    func: UpdaterFn,
    reset: Option<UpdaterResetFn>,
    reset_translation: Option<DVec3>,
    dt: f64,
    first_elapsed: f64,
    steps: usize,
}

fn fixed_step_count(total: f64, fixed_dt: f64) -> usize {
    // La tolerancia evita perder un subpaso exacto por error de representación.
    ((total + fixed_dt * 1e-9) / fixed_dt).floor() as usize
}

fn run_updater_jobs(world: &mut World, jobs: Vec<UpdaterJob>) {
    let mut to_remove = Vec::new();
    for job in jobs {
        if let Some(initial_translation) = job.reset_translation {
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(job.entity) {
                transform.translation = initial_translation;
            }
        }
        if let Some(reset) = job.reset
            && !reset(job.entity, world)
        {
            to_remove.push(job.entity);
            continue;
        }

        let mut keep = true;
        for step in 0..job.steps {
            let elapsed = job.first_elapsed + step as f64 * job.dt;
            if !(job.func)(job.dt, elapsed, job.entity, world) {
                keep = false;
                break;
            }
        }
        if !keep {
            to_remove.push(job.entity);
        }
    }

    for entity in to_remove {
        if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
            entity_mut.remove::<Updater>();
        }
    }
}

/// Avanza todos los updaters por una cantidad explícita de tiempo.
///
/// Los updaters tradicionales se evalúan una vez. Las simulaciones usan tantos
/// subpasos fijos como correspondan y conservan el residuo para el siguiente frame.
pub fn advance_updaters_by(world: &mut World, dt: f64) {
    let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
    let mut jobs = Vec::new();
    let mut query = world.query::<(Entity, &mut Updater, Option<&SpatialTransform>)>();
    for (entity, mut updater, transform) in query.iter_mut(world) {
        if updater.fixed_dt.is_some() && updater.initial_translation.is_none() {
            updater.initial_translation = transform.map(|transform| transform.translation);
        }
        let previous_elapsed = updater.elapsed;
        let next_elapsed = if let Some(stop_at) = updater.stop_at {
            (previous_elapsed + dt).min(stop_at)
        } else {
            previous_elapsed + dt
        };
        let effective_dt = (next_elapsed - previous_elapsed).max(0.0);
        updater.elapsed = next_elapsed;

        if let Some(fixed_dt) = updater.fixed_dt {
            let total = updater.accumulator + effective_dt;
            let steps = fixed_step_count(total, fixed_dt);
            updater.accumulator = (total - steps as f64 * fixed_dt).max(0.0);
            let first_elapsed = updater.simulation_elapsed + fixed_dt;
            updater.simulation_elapsed += steps as f64 * fixed_dt;
            if steps > 0 {
                jobs.push(UpdaterJob {
                    entity,
                    func: updater.func.clone(),
                    reset: None,
                    reset_translation: None,
                    dt: fixed_dt,
                    first_elapsed,
                    steps,
                });
            }
        } else {
            jobs.push(UpdaterJob {
                entity,
                func: updater.func.clone(),
                reset: None,
                reset_translation: None,
                dt: effective_dt,
                first_elapsed: next_elapsed,
                steps: 1,
            });
        }
    }

    run_updater_jobs(world, jobs);
}

/// Reconstruye todos los updaters en un instante absoluto de la timeline.
///
/// Los updaters de simulación ejecutan `reset` y reproducen desde cero con su
/// paso fijo. Los updaters tradicionales conservan la semántica histórica:
/// una evaluación con `dt = 0` y el tiempo absoluto solicitado.
pub fn seek_updaters(world: &mut World, target_time: f64) {
    let target_time = if target_time.is_finite() {
        target_time.max(0.0)
    } else {
        0.0
    };
    let mut jobs = Vec::new();
    let mut query = world.query::<(Entity, &mut Updater, Option<&SpatialTransform>)>();
    for (entity, mut updater, transform) in query.iter_mut(world) {
        let elapsed = updater
            .stop_at
            .map(|stop_at| target_time.min(stop_at))
            .unwrap_or(target_time);
        updater.elapsed = elapsed;

        if let Some(fixed_dt) = updater.fixed_dt {
            if updater.initial_translation.is_none() {
                updater.initial_translation = transform.map(|transform| transform.translation);
            }
            let steps = fixed_step_count(elapsed, fixed_dt);
            updater.simulation_elapsed = steps as f64 * fixed_dt;
            updater.accumulator = (elapsed - updater.simulation_elapsed).max(0.0);
            jobs.push(UpdaterJob {
                entity,
                func: updater.func.clone(),
                reset: updater.reset.clone(),
                reset_translation: updater.initial_translation,
                dt: fixed_dt,
                first_elapsed: fixed_dt,
                steps,
            });
        } else {
            jobs.push(UpdaterJob {
                entity,
                func: updater.func.clone(),
                reset: None,
                reset_translation: None,
                dt: 0.0,
                first_elapsed: elapsed,
                steps: 1,
            });
        }
    }

    run_updater_jobs(world, jobs);
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

    if !is_playing {
        return;
    }
    advance_updaters_by(world, dt);
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
// TRACED PATH 3D — trail reactivo en 3D (LineList)
// ==========================================

/// Componente que acumula la trayectoria 3D de una entidad como `LineListData`.
///
/// A diferencia de `TracedPath` (2D `Path2D`), este componente actualiza directamente
/// `LineListData` + `colors` por vértice, permitiendo colormap tipo Makie.
#[derive(Component)]
pub struct TracedPath3D {
    pub source: Entity,
    pub points: Vec<DVec3>,
    pub max_points: Option<usize>,
    pub min_distance: f64,
    pub colormap: Option<String>,
}

impl TracedPath3D {
    pub fn new(
        source: Entity,
        min_distance: f64,
        max_points: Option<usize>,
        colormap: Option<String>,
    ) -> Self {
        Self {
            source,
            points: Vec::new(),
            max_points,
            min_distance,
            colormap,
        }
    }
}

fn colormap_rgba(name: &str, t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    // Palettes as u8 RGB
    let palette: Vec<(u8, u8, u8)> = match name {
        "inferno" => vec![
            (0, 0, 4),
            (31, 12, 72),
            (85, 15, 109),
            (136, 34, 106),
            (168, 50, 88),
            (210, 72, 55),
            (233, 100, 28),
            (249, 157, 87),
            (247, 209, 61),
            (252, 255, 164),
        ],
        "viridis" => vec![
            (68, 1, 84),
            (59, 82, 139),
            (33, 144, 140),
            (94, 201, 98),
            (253, 231, 37),
        ],
        "plasma" => vec![
            (13, 8, 135),
            (126, 3, 168),
            (203, 70, 121),
            (248, 149, 64),
            (240, 249, 33),
        ],
        _ => vec![(255, 255, 255), (255, 255, 255)],
    };
    let scaled = t * (palette.len() - 1) as f32;
    let idx = scaled.floor() as usize;
    let f = scaled - idx as f32;
    let (r, g, b) = if idx >= palette.len() - 1 {
        palette[palette.len() - 1]
    } else {
        let (r0, g0, b0) = palette[idx];
        let (r1, g1, b1) = palette[idx + 1];
        (
            (r0 as f32 + (r1 as f32 - r0 as f32) * f) as u8,
            (g0 as f32 + (g1 as f32 - g0 as f32) * f) as u8,
            (b0 as f32 + (b1 as f32 - b0 as f32) * f) as u8,
        )
    };
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

/// Sistema que actualiza `LineListData` para cada `TracedPath3D`.
pub fn traced_path_3d_system(world: &mut World) {
    // Snapshot jobs
    let mut jobs: Vec<(Entity, Entity, f64, Option<usize>)> = Vec::new();
    let mut query = world.query::<(Entity, &TracedPath3D)>();
    for (e, t) in query.iter(world) {
        jobs.push((e, t.source, t.min_distance, t.max_points));
    }
    for (trace_entity, source_entity, min_distance, max_points) in jobs {
        let source_pos = world
            .get::<SpatialTransform>(source_entity)
            .map(|t| t.translation);
        if let Some(pos) = source_pos {
            // Check and push
            let should_push = {
                if let Some(tp) = world.get::<TracedPath3D>(trace_entity) {
                    match tp.points.last() {
                        Some(last) => last.distance(pos) >= min_distance,
                        None => true,
                    }
                } else {
                    false
                }
            };
            if !should_push {
                continue;
            }
            // Mutate TracedPath3D points
            let (points_snapshot, colormap_clone) = {
                if let Some(mut tp) = world.get_mut::<TracedPath3D>(trace_entity) {
                    tp.points.push(pos);
                    if let Some(max) = max_points {
                        if tp.points.len() > max {
                            let overflow = tp.points.len() - max;
                            tp.points.drain(0..overflow);
                        }
                    }
                    (tp.points.clone(), tp.colormap.clone())
                } else {
                    continue;
                }
            };
            // Build LineList points and optional vertex colors
            let line_points: Vec<[f32; 3]> = points_snapshot
                .iter()
                .map(|p| [p.x as f32, p.y as f32, p.z as f32])
                .collect();
            let vertex_colors: Option<Vec<[f32; 4]>> = colormap_clone.as_deref().map(|name| {
                let n = line_points.len();
                (0..n)
                    .map(|i| {
                        let t = if n > 1 {
                            i as f32 / (n - 1) as f32
                        } else {
                            0.0
                        };
                        colormap_rgba(name, t)
                    })
                    .collect()
            });

            if let Some(mut line) = world.get_mut::<gaanim_scene::LineListData>(trace_entity) {
                line.points = line_points;
                line.colors = vertex_colors;
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn_accelerating_simulation(world: &mut World, fixed_dt: f64) -> Entity {
        let velocity = Arc::new(Mutex::new(0.0));
        let step_velocity = velocity.clone();
        let reset_velocity = velocity.clone();
        let updater = Updater::new_simulation(
            move |dt, _elapsed, entity, world| {
                let mut velocity = step_velocity.lock().unwrap();
                *velocity += dt;
                if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                    transform.translation.x += *velocity * dt;
                }
                true
            },
            move |_entity, _world| {
                *reset_velocity.lock().unwrap() = 0.0;
                true
            },
            fixed_dt,
        )
        .unwrap();
        world.spawn((SpatialTransform::default(), updater)).id()
    }

    fn x(world: &World, entity: Entity) -> f64 {
        world.get::<SpatialTransform>(entity).unwrap().translation.x
    }

    #[test]
    fn fixed_step_simulation_is_independent_of_render_frame_partitioning() {
        let mut one_frame = World::new();
        let one_frame_entity = spawn_accelerating_simulation(&mut one_frame, 1.0 / 240.0);
        advance_updaters_by(&mut one_frame, 1.0 / 30.0);

        let mut four_frames = World::new();
        let four_frames_entity = spawn_accelerating_simulation(&mut four_frames, 1.0 / 240.0);
        for _ in 0..4 {
            advance_updaters_by(&mut four_frames, 1.0 / 120.0);
        }

        assert!(
            (x(&one_frame, one_frame_entity) - x(&four_frames, four_frames_entity)).abs() < 1e-12
        );
    }

    #[test]
    fn simulation_seek_resets_and_replays_external_state() {
        let mut world = World::new();
        let entity = spawn_accelerating_simulation(&mut world, 0.01);

        advance_updaters_by(&mut world, 1.0);
        let at_one_second = x(&world, entity);

        seek_updaters(&mut world, 0.5);
        let first_half_second = x(&world, entity);
        assert!(first_half_second < at_one_second);

        seek_updaters(&mut world, 1.0);
        assert!((x(&world, entity) - at_one_second).abs() < 1e-12);

        seek_updaters(&mut world, 0.5);
        assert!((x(&world, entity) - first_half_second).abs() < 1e-12);
    }

    #[test]
    fn simulation_rejects_invalid_fixed_steps() {
        for fixed_dt in [0.0, -0.01, f64::NAN, f64::INFINITY] {
            let result = Updater::new_simulation(
                |_dt, _elapsed, _entity, _world| true,
                |_entity, _world| true,
                fixed_dt,
            );
            assert_eq!(result.unwrap_err(), InvalidFixedStep);
        }
    }
}
