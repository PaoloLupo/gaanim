use crate::tween::DeltaTime;
use bevy::prelude::{Children, Component};
use gaanim_core::ObjectId;
use gaanim_core::glam::{DMat4, DQuat, DVec2, DVec3};
use gaanim_core::kurbo::{BezPath, Shape};
use gaanim_math::SpatialTransform;
use gaanim_scene::prelude::{ChildOf, Entity, World};
use gaanim_scene::{LocalBounds, Path2D, PathSource, WorldBounds};
use std::sync::Arc;
use std::sync::Mutex;

type UpdaterFn = Arc<dyn Fn(f64, f64, Entity, &mut World) -> bool + Send + Sync>;
type UpdaterResetFn = Arc<dyn Fn(Entity, &mut World) -> bool + Send + Sync>;

#[derive(bevy::prelude::Resource, Debug, Clone, Copy)]
pub struct PlaybackState {
    pub is_playing: bool,
    pub scaled_dt: f64,
    /// Posición absoluta del cabezal de la timeline.
    pub current_time: f64,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            is_playing: true,
            scaled_dt: 0.0,
            current_time: 0.0,
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
    /// Posición absoluta de la timeline alcanzada por este updater.
    pub elapsed: f64,
    /// Instante absoluto a partir del cual comienza a ejecutarse.
    pub start_at: f64,
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
            .field("start_at", &self.start_at)
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
            start_at: 0.0,
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
            start_at: 0.0,
            stop_at: None,
            time_based: true,
            fixed_dt: Some(fixed_dt),
            simulation_elapsed: 0.0,
            accumulator: 0.0,
            reset: Some(Arc::new(reset)),
            initial_translation: None,
        })
    }

    /// Desplaza el inicio del updater a un instante absoluto de la timeline.
    #[doc(hidden)]
    pub fn starting_at(mut self, start_at: f64) -> Self {
        self.start_at = if start_at.is_finite() {
            start_at.max(0.0)
        } else {
            0.0
        };
        self
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
    let mut simulation_jobs = Vec::new();
    let mut frame_jobs = Vec::new();
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
        updater.elapsed = next_elapsed;

        let previous_active_elapsed = (previous_elapsed - updater.start_at).max(0.0);
        let next_active_elapsed = (next_elapsed - updater.start_at).max(0.0);
        let effective_dt = (next_active_elapsed - previous_active_elapsed).max(0.0);

        if let Some(fixed_dt) = updater.fixed_dt {
            let total = updater.accumulator + effective_dt;
            let steps = fixed_step_count(total, fixed_dt);
            updater.accumulator = (total - steps as f64 * fixed_dt).max(0.0);
            let first_elapsed = updater.simulation_elapsed + fixed_dt;
            updater.simulation_elapsed += steps as f64 * fixed_dt;
            if steps > 0 {
                simulation_jobs.push(UpdaterJob {
                    entity,
                    func: updater.func.clone(),
                    reset: None,
                    reset_translation: None,
                    dt: fixed_dt,
                    first_elapsed,
                    steps,
                });
            }
        } else if next_elapsed + f64::EPSILON >= updater.start_at {
            frame_jobs.push(UpdaterJob {
                entity,
                func: updater.func.clone(),
                reset: None,
                reset_translation: None,
                dt: effective_dt,
                first_elapsed: next_active_elapsed,
                steps: 1,
            });
        }
    }

    // Deterministic simulations establish the frame state first. Ordinary
    // updaters may derive parameters (forces, labels, readouts) from that state
    // and therefore must observe it in the same frame regardless of ECS
    // archetype iteration order.
    run_updater_jobs(world, simulation_jobs);
    run_updater_jobs(world, frame_jobs);
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
    let mut simulation_jobs = Vec::new();
    let mut frame_jobs = Vec::new();
    let mut query = world.query::<(Entity, &mut Updater, Option<&SpatialTransform>)>();
    for (entity, mut updater, transform) in query.iter_mut(world) {
        let elapsed = updater
            .stop_at
            .map(|stop_at| target_time.min(stop_at))
            .unwrap_or(target_time);
        updater.elapsed = elapsed;
        let active_elapsed = (elapsed - updater.start_at).max(0.0);

        if let Some(fixed_dt) = updater.fixed_dt {
            if updater.initial_translation.is_none() {
                updater.initial_translation = transform.map(|transform| transform.translation);
            }
            let steps = fixed_step_count(active_elapsed, fixed_dt);
            updater.simulation_elapsed = steps as f64 * fixed_dt;
            updater.accumulator = (active_elapsed - updater.simulation_elapsed).max(0.0);
            simulation_jobs.push(UpdaterJob {
                entity,
                func: updater.func.clone(),
                reset: updater.reset.clone(),
                reset_translation: updater.initial_translation,
                dt: fixed_dt,
                first_elapsed: fixed_dt,
                steps,
            });
        } else if elapsed + f64::EPSILON >= updater.start_at {
            frame_jobs.push(UpdaterJob {
                entity,
                func: updater.func.clone(),
                reset: None,
                reset_translation: None,
                dt: 0.0,
                first_elapsed: active_elapsed,
                steps: 1,
            });
        }
    }

    run_updater_jobs(world, simulation_jobs);
    run_updater_jobs(world, frame_jobs);
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
    /// Instante absoluto asociado a cada punto acumulado.
    pub sample_times: Vec<f64>,
    /// Límite máximo de puntos a conservar (None para ilimitados).
    pub max_points: Option<usize>,
    /// Distancia mínima para añadir un nuevo punto.
    pub min_distance: f64,
    /// Instante absoluto a partir del cual comienza el rastro.
    pub start_at: f64,
    /// Tiempo de vida de cada punto. `None` conserva el historial completo.
    pub dissipating_time: Option<f64>,
}

impl TracedPath {
    /// Crea un nuevo componente TracedPath
    pub fn new(source: Entity, min_distance: f64, max_points: Option<usize>) -> Self {
        Self {
            source,
            points: Vec::new(),
            sample_times: Vec::new(),
            max_points,
            min_distance,
            start_at: 0.0,
            dissipating_time: None,
        }
    }

    /// Desplaza el inicio del rastro a un instante absoluto de la timeline.
    #[doc(hidden)]
    pub fn starting_at(mut self, start_at: f64) -> Self {
        self.start_at = start_at.max(0.0);
        self
    }

    /// Limita el historial a una ventana temporal móvil.
    pub fn with_dissipating_time(mut self, dissipating_time: Option<f64>) -> Self {
        self.dissipating_time = dissipating_time;
        self
    }
}

fn path_from_points(points: &[DVec3]) -> BezPath {
    let mut path = BezPath::new();
    if let Some(first) = points.first() {
        path.move_to(gaanim_core::kurbo::Point::new(first.x, first.y));
        for point in &points[1..] {
            path.line_to(gaanim_core::kurbo::Point::new(point.x, point.y));
        }
    }
    path
}

fn expire_samples(
    points: &mut Vec<DVec3>,
    sample_times: &mut Vec<f64>,
    current_time: f64,
    dissipating_time: Option<f64>,
) -> bool {
    if sample_times.len() != points.len() {
        sample_times.resize(points.len(), current_time);
    }
    let Some(dissipating_time) = dissipating_time else {
        return false;
    };
    let cutoff = current_time - dissipating_time;
    let expired = sample_times.partition_point(|sample_time| *sample_time < cutoff);
    if expired == 0 {
        return false;
    }
    points.drain(0..expired);
    sample_times.drain(0..expired);
    true
}

/// Sistema exclusivo que lee la posición del source de cada TracedPath y regenera su Path2D.
pub fn traced_path_system(world: &mut World) {
    let current_time = world
        .get_resource::<PlaybackState>()
        .map(|state| state.current_time)
        .unwrap_or(0.0);
    // 1. Extraemos de forma inmutable los datos de los TracedPath que necesitamos
    let mut trace_jobs = Vec::new();
    let mut query = world.query::<(Entity, &TracedPath)>();
    for (trace_entity, traced_path) in query.iter(world) {
        trace_jobs.push((
            trace_entity,
            traced_path.source,
            traced_path.min_distance,
            traced_path.max_points,
            traced_path.start_at,
            traced_path.dissipating_time,
        ));
    }

    let mut trace_updates = Vec::new();

    // 2. Procesamos cada trace_job consultando el world de forma secuencial y limpia
    for (trace_entity, source_entity, min_distance, max_points, start_at, dissipating_time) in
        trace_jobs
    {
        let source_pos = entity_world_matrix(source_entity, world)
            .map(|matrix| matrix.transform_point3(DVec3::ZERO))
            .map(|position| tracking_world_to_local(trace_entity, position, world));

        // Obtenemos acceso mutable al TracedPath específico de forma aislada
        if let Some(mut traced_path) = world.get_mut::<TracedPath>(trace_entity) {
            let mut changed = false;
            if current_time + f64::EPSILON < start_at {
                changed = !traced_path.points.is_empty();
                traced_path.points.clear();
                traced_path.sample_times.clear();
            } else {
                let (points, sample_times) = {
                    let traced_path = &mut *traced_path;
                    (&mut traced_path.points, &mut traced_path.sample_times)
                };
                changed |= expire_samples(points, sample_times, current_time, dissipating_time);

                if let Some(pos) = source_pos {
                    let should_add = match traced_path.points.last() {
                        Some(last_point) => last_point.distance(pos) >= min_distance,
                        None => true,
                    };

                    if should_add {
                        traced_path.points.push(pos);
                        traced_path.sample_times.push(current_time);
                        changed = true;
                        if let Some(max) = max_points {
                            if traced_path.points.len() > max {
                                let overflow = traced_path.points.len() - max;
                                traced_path.points.drain(0..overflow);
                                traced_path.sample_times.drain(0..overflow);
                            }
                        }
                    }
                }
            }
            if changed {
                trace_updates.push((trace_entity, path_from_points(&traced_path.points)));
            }
        }
    }

    // 3. Aplicamos los nuevos paths
    for (trace_entity, path) in trace_updates {
        write_path(world, trace_entity, path);
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
    pub sample_times: Vec<f64>,
    pub max_points: Option<usize>,
    pub min_distance: f64,
    pub colormap: Option<gaanim_core::ColorMap>,
    pub start_at: f64,
    pub dissipating_time: Option<f64>,
}

impl TracedPath3D {
    pub fn new(
        source: Entity,
        min_distance: f64,
        max_points: Option<usize>,
        colormap: Option<gaanim_core::ColorMap>,
    ) -> Self {
        Self {
            source,
            points: Vec::new(),
            sample_times: Vec::new(),
            max_points,
            min_distance,
            colormap,
            start_at: 0.0,
            dissipating_time: None,
        }
    }

    #[doc(hidden)]
    pub fn starting_at(mut self, start_at: f64) -> Self {
        self.start_at = start_at.max(0.0);
        self
    }

    pub fn with_dissipating_time(mut self, dissipating_time: Option<f64>) -> Self {
        self.dissipating_time = dissipating_time;
        self
    }
}

fn colormap_rgba(map: &gaanim_core::ColorMap, t: f32) -> [f32; 4] {
    let color = map
        .sample(f64::from(t))
        .unwrap_or(gaanim_core::peniko::Color::WHITE)
        .to_rgba8();
    [
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
        f32::from(color.a) / 255.0,
    ]
}

/// Sistema que actualiza `LineListData` para cada `TracedPath3D`.
pub fn traced_path_3d_system(world: &mut World) {
    let current_time = world
        .get_resource::<PlaybackState>()
        .map(|state| state.current_time)
        .unwrap_or(0.0);
    // Snapshot jobs
    let mut jobs: Vec<(Entity, Entity, f64, Option<usize>, f64, Option<f64>)> = Vec::new();
    let mut query = world.query::<(Entity, &TracedPath3D)>();
    for (e, t) in query.iter(world) {
        jobs.push((
            e,
            t.source,
            t.min_distance,
            t.max_points,
            t.start_at,
            t.dissipating_time,
        ));
    }
    for (trace_entity, source_entity, min_distance, max_points, start_at, dissipating_time) in jobs
    {
        let source_pos = world
            .get::<SpatialTransform>(source_entity)
            .map(|t| t.translation);

        // Mutate TracedPath3D points and expire the old tail even if the source
        // did not move enough to add a new sample.
        let update = {
            if let Some(mut tp) = world.get_mut::<TracedPath3D>(trace_entity) {
                let mut changed = false;
                if current_time + f64::EPSILON < start_at {
                    changed = !tp.points.is_empty();
                    tp.points.clear();
                    tp.sample_times.clear();
                } else {
                    let (points, sample_times) = {
                        let tp = &mut *tp;
                        (&mut tp.points, &mut tp.sample_times)
                    };
                    changed |= expire_samples(points, sample_times, current_time, dissipating_time);
                    let should_push = source_pos.is_some_and(|pos| match tp.points.last() {
                        Some(last) => last.distance(pos) >= min_distance,
                        None => true,
                    });
                    if should_push {
                        let pos = source_pos.expect("checked above");
                        tp.points.push(pos);
                        tp.sample_times.push(current_time);
                        changed = true;
                        if let Some(max) = max_points {
                            if tp.points.len() > max {
                                let overflow = tp.points.len() - max;
                                tp.points.drain(0..overflow);
                                tp.sample_times.drain(0..overflow);
                            }
                        }
                    }
                }
                changed.then(|| (tp.points.clone(), tp.colormap.clone()))
            } else {
                None
            }
        };

        if let Some((points_snapshot, colormap_clone)) = update {
            // Build LineList points and optional vertex colors
            let line_points: Vec<[f32; 3]> = points_snapshot
                .iter()
                .map(|p| [p.x as f32, p.y as f32, p.z as f32])
                .collect();
            let vertex_colors: Option<Vec<[f32; 4]>> = colormap_clone.as_ref().map(|map| {
                let n = line_points.len();
                (0..n)
                    .map(|i| {
                        let t = if n > 1 {
                            i as f32 / (n - 1) as f32
                        } else {
                            0.0
                        };
                        colormap_rgba(map, t)
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
    /// A normalized point inside an entity's local bounds plus a local-space offset.
    EntityAnchor {
        entity: Entity,
        normalized: DVec3,
        offset: DVec3,
    },
    /// Point whose coordinates are evaluated from native scalar expressions.
    Expression {
        x: TrackingScalar,
        y: TrackingScalar,
    },
    /// Point evaluated in the local frame of an entity, including hierarchy transforms.
    LocalExpression {
        space: Entity,
        x: TrackingScalar,
        y: TrackingScalar,
        z: TrackingScalar,
    },
    /// Data value mapped through a number line in an entity's local frame.
    LocalNumberLine {
        space: Entity,
        map: crate::reactive::ScalarMap,
        length: f64,
        value: TrackingScalar,
        normal_offset: TrackingScalar,
    },
    /// Reactive scene-space displacement from another endpoint.
    Offset {
        origin: Box<TrackingEndpoint>,
        dx: TrackingScalar,
        dy: TrackingScalar,
    },
    /// Affine interpolation between two reactive endpoints.
    Between {
        from: Box<TrackingEndpoint>,
        to: Box<TrackingEndpoint>,
        alpha: f64,
        offset: DVec3,
    },
    /// Polar point around another reactive endpoint.
    Polar {
        origin: Box<TrackingEndpoint>,
        radius: TrackingScalar,
        angle: TrackingScalar,
    },
}

/// Scalar expression with its construction-time parameter ids resolved to ECS entities.
#[derive(Debug, Clone)]
pub struct TrackingScalar {
    pub source: crate::reactive::ScalarSource,
    pub parameters: Vec<(ObjectId, Entity)>,
}

impl TrackingScalar {
    pub fn evaluate(&self, world: &World) -> Option<f64> {
        crate::reactive::ResolvedScalarSource {
            source: self.source.clone(),
            parameters: self.parameters.clone(),
        }
        .evaluate(world)
    }
}

/// A ray used by angular annotations: either a fixed direction or another endpoint.
#[derive(Debug, Clone)]
pub enum TrackingRay {
    Direction(DVec3),
    Endpoint(Box<TrackingEndpoint>),
}

/// Whether a following offset is expressed in scene axes or the source endpoint frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowOffsetSpace {
    World,
    Local,
}

/// Keeps a drawable centered on any reactive endpoint.
#[derive(Component, Debug, Clone)]
pub struct EndpointFollow {
    pub endpoint: TrackingEndpoint,
    pub offset: DVec3,
    pub offset_space: FollowOffsetSpace,
}

/// Angular sweep selection used by an angle dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AngleSweep {
    Minor,
    Major,
    Clockwise,
    CounterClockwise,
}

/// Solid arrowhead selection for angular annotations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AngleArrowheads {
    None,
    Start,
    End,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackingAnglePart {
    Arc,
    Arrows,
    Extensions,
}

/// Regenerates one visible part of an angular dimension.
#[derive(Component, Debug, Clone)]
pub struct TrackingAngle {
    pub vertex: TrackingEndpoint,
    pub from: TrackingRay,
    pub to: TrackingRay,
    pub radius: f64,
    pub sweep: AngleSweep,
    pub arrowheads: AngleArrowheads,
    pub part: TrackingAnglePart,
}

/// Drives a scalar signal from an angular dimension.
#[derive(Component, Debug, Clone)]
pub struct EndpointAngle {
    pub vertex: TrackingEndpoint,
    pub from: TrackingRay,
    pub to: TrackingRay,
    pub sweep: AngleSweep,
    pub scale: f64,
}

/// Places an angular annotation on the bisector of its visible sweep.
#[derive(Component, Debug, Clone)]
pub struct AngleLabelPlacement {
    pub label: Entity,
    pub vertex: TrackingEndpoint,
    pub from: TrackingRay,
    pub to: TrackingRay,
    pub radius: f64,
    pub gap: f64,
    pub sweep: AngleSweep,
    pub orientation: DimensionLabelOrientation,
}

/// Solid head for a reactive vector annotation.
#[derive(Component, Debug, Clone)]
pub struct TrackingVectorHead {
    pub from: TrackingEndpoint,
    pub to: TrackingEndpoint,
    pub length: f64,
    pub width: f64,
}

/// Copies a source drawable's world rotation into a target with ratio and phase.
#[derive(Component, Debug, Clone)]
pub struct RotationBinding {
    pub source: Entity,
    pub ratio: f64,
    pub phase: f64,
}

/// Converts a source rotation into target translation along a fixed scene axis.
#[derive(Component, Debug, Clone)]
pub struct RotationTranslationBinding {
    pub source: Entity,
    pub axis: DVec3,
    pub scale: f64,
    pub base_position: Option<DVec3>,
    pub base_angle: Option<f64>,
}

/// Propiedad de una entidad que un `SampledSeriesDriver` puede controlar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampledProperty {
    TranslateX,
    TranslateY,
    TranslateZ,
    RotateZ,
    UniformScale,
    Opacity,
    Signal,
}

/// Interpolación entre muestras consecutivas de una serie muestreada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampledInterpolation {
    /// Mantiene el valor de la última muestra alcanzada.
    Step,
    /// Interpola linealmente entre las dos muestras circundantes.
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
#[error(
    "sampled series requires non-empty finite non-decreasing times, matching finite values, and finite scale/offset"
)]
pub struct InvalidSampledSeries;

/// Conduce una propiedad de la entidad a lo largo de una serie muestreada
/// `(times, values)` como función pura del tiempo absoluto de la timeline.
///
/// A diferencia de un updater de Python (callback por frame) o de una
/// simulación de paso fijo, este componente no acumula estado: evaluar en un
/// instante cualquiera reproduce siempre el mismo resultado, por lo que los
/// seeks y el scrub en pausa son exactos sin replay.
///
/// Para `TranslateX`/`TranslateY`/`TranslateZ`/`RotateZ` la salida es
/// relativa al valor autorizado, capturado de forma lazy en la primera
/// evaluación: `base + offset + scale * muestra`. Para `UniformScale`,
/// `Opacity` y `Signal` la salida es absoluta: `offset + scale * muestra`.
#[derive(Component, Clone)]
pub struct SampledSeriesDriver {
    pub times: Arc<[f64]>,
    pub values: Arc<[f64]>,
    pub interpolation: SampledInterpolation,
    pub property: SampledProperty,
    pub scale: f64,
    pub offset: f64,
    /// Instante absoluto de la timeline a partir del cual corre la serie.
    pub start_at: f64,
    /// Tiempo de corte opcional: a partir de aquí el driver queda congelado
    /// en el valor que tenía a ese instante (semántica de `RemoveUpdater`).
    pub stop_at: Option<f64>,
    /// Valor base capturado en la primera evaluación (propiedades relativas).
    pub base: Option<f64>,
}

impl std::fmt::Debug for SampledSeriesDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SampledSeriesDriver")
            .field("samples", &self.times.len())
            .field("interpolation", &self.interpolation)
            .field("property", &self.property)
            .field("scale", &self.scale)
            .field("offset", &self.offset)
            .field("start_at", &self.start_at)
            .field("stop_at", &self.stop_at)
            .field("base", &self.base)
            .finish()
    }
}

impl SampledSeriesDriver {
    /// Pure absolute-time sampling, shared by playback and frozen reactive targets.
    pub fn sample_at(&self, time: f64) -> f64 {
        let time = self.stop_at.map_or(time, |stop| time.min(stop));
        self.offset + self.scale * self.sample((time - self.start_at).max(0.0))
    }
    /// Valida y construye el driver. Los tiempos deben ser finitos y no
    /// decrecientes; los valores, finitos y de la misma longitud que los tiempos.
    pub fn new(
        times: Vec<f64>,
        values: Vec<f64>,
        property: SampledProperty,
        interpolation: SampledInterpolation,
        scale: f64,
        offset: f64,
    ) -> Result<Self, InvalidSampledSeries> {
        if times.is_empty()
            || times.len() != values.len()
            || !times.iter().all(|time| time.is_finite())
            || !values.iter().all(|value| value.is_finite())
            || !times.windows(2).all(|window| window[0] <= window[1])
            || !scale.is_finite()
            || !offset.is_finite()
        {
            return Err(InvalidSampledSeries);
        }
        Ok(Self {
            times: times.into(),
            values: values.into(),
            interpolation,
            property,
            scale,
            offset,
            start_at: 0.0,
            stop_at: None,
            base: None,
        })
    }

    /// Desplaza el inicio del driver a un instante absoluto de la timeline.
    #[doc(hidden)]
    pub fn starting_at(mut self, start_at: f64) -> Self {
        self.start_at = if start_at.is_finite() {
            start_at.max(0.0)
        } else {
            0.0
        };
        self
    }

    /// Valor muestreado en `elapsed` segundos desde `start_at`, con clamp a
    /// los extremos de la serie fuera de rango.
    fn sample(&self, elapsed: f64) -> f64 {
        let last = self.times.len() - 1;
        if elapsed <= self.times[0] {
            return self.values[0];
        }
        if elapsed >= self.times[last] {
            return self.values[last];
        }
        let index = self.times.partition_point(|time| *time <= elapsed) - 1;
        match self.interpolation {
            SampledInterpolation::Step => self.values[index],
            SampledInterpolation::Linear => {
                let t0 = self.times[index];
                let t1 = self.times[index + 1];
                if t1 <= t0 {
                    self.values[index + 1]
                } else {
                    let alpha = (elapsed - t0) / (t1 - t0);
                    self.values[index] * (1.0 - alpha) + self.values[index + 1] * alpha
                }
            }
        }
    }
}

/// Keeps a float signal synchronized with the XY distance between two endpoints.
#[derive(Component, Debug, Clone)]
pub struct EndpointDistance {
    pub from: TrackingEndpoint,
    pub to: TrackingEndpoint,
    pub scale: f64,
}

/// Orientation policy for a reactive dimension annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimensionLabelOrientation {
    Upright,
    Aligned,
}

/// Places a dimension annotation at the midpoint of its displaced baseline.
#[derive(Component, Debug, Clone)]
pub struct DimensionLabelPlacement {
    pub label: Entity,
    pub from: TrackingEndpoint,
    pub to: TrackingEndpoint,
    pub offset: f64,
    pub gap: f64,
    pub orientation: DimensionLabelOrientation,
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

/// A live, axis-aligned frame around one or more scene objects.
///
/// `from` and `to` contain compiled object ids (including text glyph ids).
/// Timeline lenses update `progress`; the derived-geometry system resolves
/// both sets from their current world bounds every frame, so a retarget can
/// interpolate while either endpoint continues moving.
#[derive(Component, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SurroundingRect {
    pub from: Vec<gaanim_core::ObjectId>,
    pub to: Vec<gaanim_core::ObjectId>,
    pub progress: f64,
    /// Padding ordered as top, right, bottom, left.
    pub padding: [f64; 4],
    pub corner_radius: f64,
    pub last_bounds: Option<gaanim_math::Bounds3D>,
}

impl SurroundingRect {
    pub fn new(targets: Vec<gaanim_core::ObjectId>, padding: [f64; 4], corner_radius: f64) -> Self {
        Self {
            from: targets.clone(),
            to: targets,
            progress: 1.0,
            padding,
            corner_radius,
            last_bounds: None,
        }
    }
}

fn object_set_bounds(
    ids: &[gaanim_core::ObjectId],
    entities: &std::collections::HashMap<gaanim_core::ObjectId, Entity>,
    world: &World,
) -> Option<gaanim_math::Bounds3D> {
    ids.iter()
        .filter_map(|id| entities.get(id))
        .filter_map(|entity| resolve_entity_bounds(*entity, world))
        .reduce(|left, right| left.union(&right))
}

fn padded_bounds(bounds: gaanim_math::Bounds3D, padding: [f64; 4]) -> gaanim_math::Bounds3D {
    let [top, right, bottom, left] = padding;
    gaanim_math::Bounds3D::new_2d(
        bounds.min.x - left,
        bounds.min.y - bottom,
        bounds.max.x + right,
        bounds.max.y + top,
    )
}

fn lerp_bounds(
    from: gaanim_math::Bounds3D,
    to: gaanim_math::Bounds3D,
    progress: f64,
) -> gaanim_math::Bounds3D {
    let progress = progress.clamp(0.0, 1.0);
    gaanim_math::Bounds3D::new(
        from.min.lerp(to.min, progress),
        from.max.lerp(to.max, progress),
    )
}

/// Rebuild all live surrounding rectangles after layout and transform
/// propagation, immediately before normal bounds extraction.
pub fn surrounding_rect_system(world: &mut World) {
    let entities = {
        let mut query = world.query::<(Entity, &gaanim_scene::MobjectId)>();
        query
            .iter(world)
            .map(|(entity, id)| (id.0, entity))
            .collect::<std::collections::HashMap<_, _>>()
    };
    let frames = {
        let mut query = world.query::<(Entity, &SurroundingRect)>();
        query
            .iter(world)
            .map(|(entity, frame)| (entity, frame.clone()))
            .collect::<Vec<_>>()
    };

    for (entity, frame) in frames {
        let from = object_set_bounds(&frame.from, &entities, world)
            .map(|bounds| padded_bounds(bounds, frame.padding));
        let to = object_set_bounds(&frame.to, &entities, world)
            .map(|bounds| padded_bounds(bounds, frame.padding));
        let bounds = match (from, to) {
            (Some(from), Some(to)) => Some(lerp_bounds(from, to, frame.progress)),
            (Some(bounds), None) | (None, Some(bounds)) => Some(bounds),
            (None, None) => frame.last_bounds,
        };
        let Some(bounds) = bounds else {
            write_path(world, entity, BezPath::new());
            continue;
        };

        let radius = frame
            .corner_radius
            .min(bounds.width().abs() * 0.5)
            .min(bounds.height().abs() * 0.5)
            .max(0.0);
        let path = if radius > 0.0 {
            gaanim_core::kurbo::RoundedRect::new(
                bounds.min.x,
                bounds.min.y,
                bounds.max.x,
                bounds.max.y,
                radius,
            )
            .to_path(0.1)
        } else {
            gaanim_core::kurbo::Rect::new(bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y)
                .to_path(0.1)
        };
        write_path(world, entity, path);
        if let Some(mut local) = world.get_mut::<LocalBounds>(entity) {
            local.0 = bounds;
        }
        if let Some(mut live) = world.get_mut::<SurroundingRect>(entity) {
            live.last_bounds = Some(bounds);
        }
    }
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
        let from_pos = resolve_tracking_endpoint(&line.from, world);
        let to_pos = resolve_tracking_endpoint(&line.to, world);

        if let (Some(from), Some(to)) = (from_pos, to_pos) {
            let inverse = entity_world_matrix(entity, world)
                .unwrap_or(DMat4::IDENTITY)
                .inverse();
            let from = inverse.transform_point3(from);
            let to = inverse.transform_point3(to);
            let mut path = BezPath::new();
            path.move_to(gaanim_core::kurbo::Point::new(from.x, from.y));
            path.line_to(gaanim_core::kurbo::Point::new(to.x, to.y));
            updates.push((entity, path));
        }
    }

    for (entity, path) in updates {
        write_path(world, entity, path);
    }
}

/// Resolve the current world-space position of a reactive endpoint.
pub fn resolve_tracking_endpoint(ep: &TrackingEndpoint, world: &World) -> Option<DVec3> {
    match ep {
        TrackingEndpoint::Static(pos) => Some(*pos),
        TrackingEndpoint::Entity(entity) => {
            entity_world_matrix(*entity, world).map(|matrix| matrix.transform_point3(DVec3::ZERO))
        }
        TrackingEndpoint::EntityAnchor {
            entity,
            normalized,
            offset,
        } => {
            let bounds = world.get::<LocalBounds>(*entity)?.0;
            let center = bounds.center();
            let half = bounds.size() * 0.5;
            let local = center + half * *normalized + *offset;
            entity_world_matrix(*entity, world).map(|matrix| matrix.transform_point3(local))
        }
        TrackingEndpoint::Expression { x, y } => {
            Some(DVec3::new(x.evaluate(world)?, y.evaluate(world)?, 0.0))
        }
        TrackingEndpoint::LocalExpression { space, x, y, z } => {
            let local = DVec3::new(x.evaluate(world)?, y.evaluate(world)?, z.evaluate(world)?);
            entity_world_matrix(*space, world).map(|matrix| matrix.transform_point3(local))
        }
        TrackingEndpoint::LocalNumberLine {
            space,
            map,
            length,
            value,
            normal_offset,
        } => {
            let local_x = (map.evaluate(value.evaluate(world)?)? - 0.5) * *length;
            let local = DVec3::new(local_x, normal_offset.evaluate(world)?, 0.0);
            entity_world_matrix(*space, world).map(|matrix| matrix.transform_point3(local))
        }
        TrackingEndpoint::Offset { origin, dx, dy } => {
            let origin = resolve_tracking_endpoint(origin, world)?;
            Some(origin + DVec3::new(dx.evaluate(world)?, dy.evaluate(world)?, 0.0))
        }
        TrackingEndpoint::Between {
            from,
            to,
            alpha,
            offset,
        } => {
            let from = resolve_tracking_endpoint(from, world)?;
            let to = resolve_tracking_endpoint(to, world)?;
            Some(from.lerp(to, *alpha) + *offset)
        }
        TrackingEndpoint::Polar {
            origin,
            radius,
            angle,
        } => {
            let origin = resolve_tracking_endpoint(origin, world)?;
            let radius = radius.evaluate(world)?;
            let angle = angle.evaluate(world)?;
            Some(origin + DVec3::new(radius * angle.cos(), radius * angle.sin(), 0.0))
        }
    }
}

/// Resolve an endpoint plus an offset expressed in world or endpoint-local axes.
pub fn resolve_tracking_endpoint_with_offset(
    endpoint: &TrackingEndpoint,
    offset: DVec3,
    offset_space: FollowOffsetSpace,
    world: &World,
) -> Option<DVec3> {
    let origin = resolve_tracking_endpoint(endpoint, world)?;
    let offset = match offset_space {
        FollowOffsetSpace::World => offset,
        FollowOffsetSpace::Local => endpoint_basis(endpoint, world).transform_vector3(offset),
    };
    Some(origin + offset)
}

/// Resolve current world bounds without depending on propagation system order.
pub fn resolve_entity_bounds(entity: Entity, world: &World) -> Option<gaanim_math::Bounds3D> {
    let own = world.get::<LocalBounds>(entity).and_then(|local| {
        entity_world_matrix(entity, world).map(|matrix| local.0.transform_mat4(&matrix))
    });
    let descendants = world
        .get::<Children>(entity)
        .into_iter()
        .flat_map(|children| children.iter())
        .filter_map(|child| resolve_entity_bounds(*child, world));
    own.into_iter()
        .chain(descendants)
        .reduce(|left, right| left.union(&right))
        .or_else(|| world.get::<WorldBounds>(entity).map(|bounds| bounds.0))
}

fn endpoint_basis(ep: &TrackingEndpoint, world: &World) -> DMat4 {
    match ep {
        TrackingEndpoint::Entity(entity) | TrackingEndpoint::EntityAnchor { entity, .. } => {
            entity_world_matrix(*entity, world).unwrap_or(DMat4::IDENTITY)
        }
        TrackingEndpoint::LocalExpression { space, .. }
        | TrackingEndpoint::LocalNumberLine { space, .. } => {
            entity_world_matrix(*space, world).unwrap_or(DMat4::IDENTITY)
        }
        TrackingEndpoint::Offset { origin, .. } | TrackingEndpoint::Polar { origin, .. } => {
            endpoint_basis(origin, world)
        }
        _ => DMat4::IDENTITY,
    }
}

fn resolve_tracking_ray(ray: &TrackingRay, vertex: DVec3, world: &World) -> Option<DVec3> {
    let vector = match ray {
        TrackingRay::Direction(direction) => *direction,
        TrackingRay::Endpoint(endpoint) => resolve_tracking_endpoint(endpoint, world)? - vertex,
    };
    let xy = vector.truncate();
    (xy.length_squared() > 1e-12).then(|| DVec3::new(xy.x, xy.y, 0.0).normalize())
}

fn normalized_angle(angle: f64) -> f64 {
    angle.rem_euclid(std::f64::consts::TAU)
}

fn selected_angle_sweep(from_angle: f64, to_angle: f64, mode: AngleSweep) -> f64 {
    let ccw = normalized_angle(to_angle - from_angle);
    match mode {
        AngleSweep::CounterClockwise => ccw,
        AngleSweep::Clockwise => {
            if ccw <= f64::EPSILON {
                0.0
            } else {
                ccw - std::f64::consts::TAU
            }
        }
        AngleSweep::Minor => {
            if ccw <= std::f64::consts::PI {
                ccw
            } else {
                ccw - std::f64::consts::TAU
            }
        }
        AngleSweep::Major => {
            if ccw <= f64::EPSILON {
                0.0
            } else if ccw <= std::f64::consts::PI {
                ccw - std::f64::consts::TAU
            } else {
                ccw
            }
        }
    }
}

fn resolve_angle(
    vertex: &TrackingEndpoint,
    from: &TrackingRay,
    to: &TrackingRay,
    sweep: AngleSweep,
    world: &World,
) -> Option<(DVec3, f64, f64)> {
    let vertex = resolve_tracking_endpoint(vertex, world)?;
    let from = resolve_tracking_ray(from, vertex, world)?;
    let to = resolve_tracking_ray(to, vertex, world)?;
    let start = from.y.atan2(from.x);
    let end = to.y.atan2(to.x);
    Some((vertex, start, selected_angle_sweep(start, end, sweep)))
}

fn write_path(world: &mut World, entity: Entity, path: BezPath) {
    let path = Arc::new(path);
    let reveal = world
        .get::<crate::writing::PathReveal>(entity)
        .map(|progress| progress.0)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let visible = crate::writing::path_at_reveal(&path, reveal);
    if let Some(mut path_comp) = world.get_mut::<Path2D>(entity) {
        path_comp.0 = visible;
    }
    if let Some(mut source) = world.get_mut::<PathSource>(entity) {
        source.0 = path.clone();
    }
    if let Some(mut bounds) = world.get_mut::<LocalBounds>(entity) {
        if path.elements().is_empty() {
            bounds.0 = gaanim_math::Bounds3D::new_2d(0.0, 0.0, 0.0, 0.0);
        } else {
            let rect = gaanim_core::kurbo::Shape::bounding_box(path.as_ref());
            bounds.0 = gaanim_math::Bounds3D::new_2d(
                rect.x0 - 12.0,
                rect.y0 - 12.0,
                rect.x1 + 12.0,
                rect.y1 + 12.0,
            );
        }
    }
}

/// Convert a world-space point into the current local space of an entity.
pub fn tracking_world_to_local(entity: Entity, point: DVec3, world: &World) -> DVec3 {
    entity_world_matrix(entity, world)
        .filter(|matrix| matrix.determinant().abs() > f64::EPSILON)
        .map(|matrix| matrix.inverse().transform_point3(point))
        .unwrap_or(point)
}

fn entity_world_matrix(entity: Entity, world: &World) -> Option<DMat4> {
    let mut chain = Vec::new();
    let mut current = entity;
    for _ in 0..256 {
        chain.push(world.get::<SpatialTransform>(current)?.to_mat4());
        let Some(parent) = world
            .get::<ChildOf>(current)
            .map(|relation| relation.parent())
        else {
            let mut matrix = DMat4::IDENTITY;
            for local in chain.iter().rev() {
                matrix *= *local;
            }
            return Some(matrix);
        };
        current = parent;
    }
    None
}

/// Update distance-backed signals after authored transforms and custom updaters.
pub fn endpoint_distance_system(world: &mut World) {
    let mut updates = Vec::new();
    let mut query = world.query::<(Entity, &EndpointDistance)>();
    for (entity, distance) in query.iter(world) {
        if let (Some(from), Some(to)) = (
            resolve_tracking_endpoint(&distance.from, world),
            resolve_tracking_endpoint(&distance.to, world),
        ) {
            updates.push((
                entity,
                from.truncate().distance(to.truncate()) * distance.scale,
            ));
        }
    }
    for (entity, value) in updates {
        if let Some(mut signal) = world.get_mut::<crate::signals::FloatSignal>(entity) {
            signal.value = value;
        }
    }
}

/// Keep dimension annotations centered on their displaced dimension baseline.
pub fn dimension_label_placement_system(world: &mut World) {
    let mut updates = Vec::new();
    let mut query = world.query::<&DimensionLabelPlacement>();
    for placement in query.iter(world) {
        let (Some(from), Some(to)) = (
            resolve_tracking_endpoint(&placement.from, world),
            resolve_tracking_endpoint(&placement.to, world),
        ) else {
            continue;
        };
        let delta = to - from;
        let length = delta.truncate().length();
        if length <= f64::EPSILON {
            updates.push((placement.label, from, 0.0));
            continue;
        }
        let direction = delta.truncate() / length;
        let normal = gaanim_core::glam::DVec2::new(-direction.y, direction.x);
        let side = if placement.offset < 0.0 { -1.0 } else { 1.0 };
        let displacement = placement.offset + side * placement.gap;
        let midpoint =
            (from + to) * 0.5 + DVec3::new(normal.x * displacement, normal.y * displacement, 0.0);
        let mut angle = match placement.orientation {
            DimensionLabelOrientation::Upright => 0.0,
            DimensionLabelOrientation::Aligned => direction.y.atan2(direction.x),
        };
        if placement.orientation == DimensionLabelOrientation::Aligned && angle.cos() < 0.0 {
            angle += std::f64::consts::PI;
        }
        if angle > std::f64::consts::PI {
            angle -= std::f64::consts::TAU;
        } else if angle < -std::f64::consts::PI {
            angle += std::f64::consts::TAU;
        }
        updates.push((placement.label, midpoint, angle));
    }

    for (label, world_position, world_angle) in updates {
        let (local_position, local_angle) = if let Some(parent) = world
            .get::<ChildOf>(label)
            .map(|relation| relation.parent())
            .and_then(|parent| entity_world_matrix(parent, world))
        {
            let (_, parent_rotation, _) = parent.to_scale_rotation_translation();
            let (_, _, parent_angle) = parent_rotation.to_euler(gaanim_core::glam::EulerRot::XYZ);
            (
                parent.inverse().transform_point3(world_position),
                world_angle - parent_angle,
            )
        } else {
            (world_position, world_angle)
        };
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(label) {
            transform.translation = local_position;
            transform.rotation = DQuat::from_rotation_z(local_angle);
        }
    }
}

/// Follow arbitrary endpoints after authored transforms and custom updaters.
pub fn endpoint_follow_system(world: &mut World) {
    let mut updates = Vec::new();
    let mut query = world.query::<(Entity, &EndpointFollow)>();
    for (entity, follow) in query.iter(world) {
        let Some(mut position) = resolve_tracking_endpoint(&follow.endpoint, world) else {
            continue;
        };
        position += match follow.offset_space {
            FollowOffsetSpace::World => follow.offset,
            FollowOffsetSpace::Local => {
                endpoint_basis(&follow.endpoint, world).transform_vector3(follow.offset)
            }
        };
        updates.push((entity, position));
    }
    for (entity, world_position) in updates {
        let local = world
            .get::<ChildOf>(entity)
            .map(|relation| relation.parent())
            .and_then(|parent| entity_world_matrix(parent, world))
            .filter(|matrix| matrix.determinant().abs() > f64::EPSILON)
            .map(|matrix| matrix.inverse().transform_point3(world_position))
            .unwrap_or(world_position);
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
            transform.translation = local;
        }
    }
}

/// Regenerate arc, arrowhead, and extension geometry for angular dimensions.
pub fn tracking_angle_system(world: &mut World) {
    let mut updates = Vec::new();
    let mut query = world.query::<(Entity, &TrackingAngle)>();
    for (entity, angle) in query.iter(world) {
        let mut path = BezPath::new();
        let Some((vertex, start, sweep)) =
            resolve_angle(&angle.vertex, &angle.from, &angle.to, angle.sweep, world)
        else {
            updates.push((entity, path));
            continue;
        };
        let local = |point: DVec3| tracking_world_to_local(entity, point, world);
        let radial = |a: f64, r: f64| vertex + DVec3::new(r * a.cos(), r * a.sin(), 0.0);
        match angle.part {
            TrackingAnglePart::Arc => {
                let segments =
                    ((sweep.abs() / (std::f64::consts::PI / 36.0)).ceil() as usize).clamp(1, 144);
                for index in 0..=segments {
                    let a = start + sweep * index as f64 / segments as f64;
                    let point = local(radial(a, angle.radius));
                    if index == 0 {
                        path.move_to((point.x, point.y));
                    } else {
                        path.line_to((point.x, point.y));
                    }
                }
            }
            TrackingAnglePart::Extensions => {
                for a in [start, start + sweep] {
                    let inner = local(radial(a, angle.radius * 0.76));
                    let outer = local(radial(a, angle.radius * 1.12));
                    path.move_to((inner.x, inner.y));
                    path.line_to((outer.x, outer.y));
                }
            }
            TrackingAnglePart::Arrows => {
                let mut arrow = |at_start: bool| {
                    let angle_at = if at_start { start } else { start + sweep };
                    let tip_world = radial(angle_at, angle.radius);
                    let tangent = DVec2::new(-angle_at.sin(), angle_at.cos())
                        * if sweep >= 0.0 { 1.0 } else { -1.0 };
                    let inward = if at_start { tangent } else { -tangent };
                    let normal = DVec2::new(-inward.y, inward.x);
                    let tip = local(tip_world);
                    let base_world = tip_world + DVec3::new(inward.x * 11.0, inward.y * 11.0, 0.0);
                    let left = local(base_world + DVec3::new(normal.x * 5.5, normal.y * 5.5, 0.0));
                    let right = local(base_world - DVec3::new(normal.x * 5.5, normal.y * 5.5, 0.0));
                    path.move_to((tip.x, tip.y));
                    path.line_to((left.x, left.y));
                    path.line_to((right.x, right.y));
                    path.close_path();
                };
                if matches!(
                    angle.arrowheads,
                    AngleArrowheads::Start | AngleArrowheads::Both
                ) {
                    arrow(true);
                }
                if matches!(
                    angle.arrowheads,
                    AngleArrowheads::End | AngleArrowheads::Both
                ) {
                    arrow(false);
                }
            }
        }
        updates.push((entity, path));
    }
    for (entity, path) in updates {
        write_path(world, entity, path);
    }
}

/// Update angle-backed readout signals.
pub fn endpoint_angle_system(world: &mut World) {
    let mut updates = Vec::new();
    let mut query = world.query::<(Entity, &EndpointAngle)>();
    for (entity, angle) in query.iter(world) {
        if let Some((_, _, sweep)) =
            resolve_angle(&angle.vertex, &angle.from, &angle.to, angle.sweep, world)
        {
            updates.push((entity, sweep.abs() * angle.scale));
        }
    }
    for (entity, value) in updates {
        if let Some(mut signal) = world.get_mut::<crate::signals::FloatSignal>(entity) {
            signal.value = value;
        }
    }
}

/// Place angle annotations on the visible sweep bisector.
pub fn angle_label_placement_system(world: &mut World) {
    let mut updates = Vec::new();
    let mut query = world.query::<&AngleLabelPlacement>();
    for placement in query.iter(world) {
        let Some((vertex, start, sweep)) = resolve_angle(
            &placement.vertex,
            &placement.from,
            &placement.to,
            placement.sweep,
            world,
        ) else {
            continue;
        };
        let bisector = start + sweep * 0.5;
        let position = vertex
            + DVec3::new(
                (placement.radius + placement.gap) * bisector.cos(),
                (placement.radius + placement.gap) * bisector.sin(),
                0.0,
            );
        let mut rotation = match placement.orientation {
            DimensionLabelOrientation::Upright => 0.0,
            DimensionLabelOrientation::Aligned => bisector + std::f64::consts::FRAC_PI_2,
        };
        if placement.orientation == DimensionLabelOrientation::Aligned && rotation.cos() < 0.0 {
            rotation += std::f64::consts::PI;
        }
        updates.push((placement.label, position, rotation));
    }
    for (label, world_position, world_rotation) in updates {
        let (position, rotation) = if let Some(parent) = world
            .get::<ChildOf>(label)
            .map(|relation| relation.parent())
            .and_then(|parent| entity_world_matrix(parent, world))
        {
            let (_, parent_rotation, _) = parent.to_scale_rotation_translation();
            let (_, _, parent_angle) = parent_rotation.to_euler(gaanim_core::glam::EulerRot::XYZ);
            (
                parent.inverse().transform_point3(world_position),
                world_rotation - parent_angle,
            )
        } else {
            (world_position, world_rotation)
        };
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(label) {
            transform.translation = position;
            transform.rotation = DQuat::from_rotation_z(rotation);
        }
    }
}

/// Regenerate solid triangular heads for reactive vectors.
pub fn tracking_vector_head_system(world: &mut World) {
    let mut updates = Vec::new();
    let mut query = world.query::<(Entity, &TrackingVectorHead)>();
    for (entity, head) in query.iter(world) {
        let mut path = BezPath::new();
        let (Some(from), Some(to)) = (
            resolve_tracking_endpoint(&head.from, world),
            resolve_tracking_endpoint(&head.to, world),
        ) else {
            updates.push((entity, path));
            continue;
        };
        let delta = (to - from).truncate();
        if delta.length_squared() > 1e-12 {
            let direction = delta.normalize();
            let normal = DVec2::new(-direction.y, direction.x);
            let base = to - DVec3::new(direction.x * head.length, direction.y * head.length, 0.0);
            let tip = tracking_world_to_local(entity, to, world);
            let left = tracking_world_to_local(
                entity,
                base + DVec3::new(
                    normal.x * head.width * 0.5,
                    normal.y * head.width * 0.5,
                    0.0,
                ),
                world,
            );
            let right = tracking_world_to_local(
                entity,
                base - DVec3::new(
                    normal.x * head.width * 0.5,
                    normal.y * head.width * 0.5,
                    0.0,
                ),
                world,
            );
            path.move_to((tip.x, tip.y));
            path.line_to((left.x, left.y));
            path.line_to((right.x, right.y));
            path.close_path();
        }
        updates.push((entity, path));
    }
    for (entity, path) in updates {
        write_path(world, entity, path);
    }
}

fn world_z_angle(entity: Entity, world: &World) -> Option<f64> {
    let matrix = entity_world_matrix(entity, world)?;
    Some(matrix.x_axis.y.atan2(matrix.x_axis.x))
}

/// Apply rotation and rotation-to-translation bindings in the connector phase.
pub fn mechanism_binding_system(world: &mut World) {
    let mut rotations = Vec::new();
    let mut rotation_query = world.query::<(Entity, &RotationBinding)>();
    for (entity, binding) in rotation_query.iter(world) {
        if let Some(source_angle) = world_z_angle(binding.source, world) {
            rotations.push((entity, source_angle * binding.ratio + binding.phase));
        }
    }
    for (entity, world_angle) in rotations {
        let parent_angle = world
            .get::<ChildOf>(entity)
            .map(|relation| relation.parent())
            .and_then(|parent| world_z_angle(parent, world))
            .unwrap_or(0.0);
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
            transform.rotation = DQuat::from_rotation_z(world_angle - parent_angle);
        }
    }

    let mut translations = Vec::new();
    let mut initialized = Vec::new();
    let mut query = world.query::<(Entity, &RotationTranslationBinding)>();
    for (entity, binding) in query.iter(world) {
        let Some(angle) = world_z_angle(binding.source, world) else {
            continue;
        };
        let Some(position) =
            entity_world_matrix(entity, world).map(|matrix| matrix.transform_point3(DVec3::ZERO))
        else {
            continue;
        };
        let base_position = binding.base_position.unwrap_or(position);
        let base_angle = binding.base_angle.unwrap_or(angle);
        if binding.base_position.is_none() || binding.base_angle.is_none() {
            initialized.push((entity, base_position, base_angle));
        }
        translations.push((
            entity,
            base_position
                + binding.axis.normalize_or_zero() * ((angle - base_angle) * binding.scale),
        ));
    }
    for (entity, base_position, base_angle) in initialized {
        if let Some(mut binding) = world.get_mut::<RotationTranslationBinding>(entity) {
            binding.base_position = Some(base_position);
            binding.base_angle = Some(base_angle);
        }
    }
    for (entity, world_position) in translations {
        let local = world
            .get::<ChildOf>(entity)
            .map(|relation| relation.parent())
            .and_then(|parent| entity_world_matrix(parent, world))
            .map(|matrix| matrix.inverse().transform_point3(world_position))
            .unwrap_or(world_position);
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
            transform.translation = local;
        }
    }
}

/// Sistema exclusivo que evalúa cada `SampledSeriesDriver` como función pura
/// del tiempo absoluto de la timeline (`PlaybackState.current_time`).
///
/// Debe correr después de `updater_system` y antes de los bindings de posición
/// para que el resto de la fase Updaters observe el resultado del mismo frame.
/// No se salta en pausa: el scrub en pausa cambia `current_time` y el driver
/// debe reflejarlo (la evaluación es idempotente).
pub fn sampled_series_system(world: &mut World) {
    let current_time = world
        .get_resource::<PlaybackState>()
        .map(|state| state.current_time)
        .unwrap_or(0.0);

    let mut updates: Vec<(Entity, SampledProperty, f64)> = Vec::new();
    let mut new_bases: Vec<(Entity, f64)> = Vec::new();
    {
        let mut query = world.query::<(Entity, &SampledSeriesDriver)>();
        for (entity, driver) in query.iter(world) {
            if driver.times.is_empty() || driver.values.len() != driver.times.len() {
                continue;
            }
            let relative = matches!(
                driver.property,
                SampledProperty::TranslateX
                    | SampledProperty::TranslateY
                    | SampledProperty::TranslateZ
                    | SampledProperty::RotateZ
            );
            let base = if relative {
                driver.base.unwrap_or_else(|| {
                    let captured = match driver.property {
                        SampledProperty::TranslateX => world
                            .get::<SpatialTransform>(entity)
                            .map(|transform| transform.translation.x),
                        SampledProperty::TranslateY => world
                            .get::<SpatialTransform>(entity)
                            .map(|transform| transform.translation.y),
                        SampledProperty::TranslateZ => world
                            .get::<SpatialTransform>(entity)
                            .map(|transform| transform.translation.z),
                        SampledProperty::RotateZ => world
                            .get::<SpatialTransform>(entity)
                            .map(|transform| transform.rotation.to_scaled_axis().z),
                        _ => None,
                    };
                    let value = captured.unwrap_or(0.0);
                    new_bases.push((entity, value));
                    value
                })
            } else {
                0.0
            };

            let effective_time = match driver.stop_at {
                Some(stop_at) => current_time.min(stop_at),
                None => current_time,
            };
            let elapsed = (effective_time - driver.start_at).max(0.0);
            let sampled = driver.sample(elapsed);
            let output = if relative {
                base + driver.offset + driver.scale * sampled
            } else {
                driver.offset + driver.scale * sampled
            };
            updates.push((entity, driver.property, output));
        }
    }

    for (entity, base) in new_bases {
        if let Some(mut driver) = world.get_mut::<SampledSeriesDriver>(entity) {
            driver.base = Some(base);
        }
    }

    for (entity, property, output) in updates {
        match property {
            SampledProperty::TranslateX => {
                if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                    transform.translation.x = output;
                }
            }
            SampledProperty::TranslateY => {
                if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                    transform.translation.y = output;
                }
            }
            SampledProperty::TranslateZ => {
                if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                    transform.translation.z = output;
                }
            }
            SampledProperty::RotateZ => {
                if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                    transform.rotation = DQuat::from_rotation_z(output);
                }
            }
            SampledProperty::UniformScale => {
                if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                    transform.scale = DVec3::splat(output.max(0.0));
                }
            }
            SampledProperty::Opacity => {
                if let Some(mut opacity) = world.get_mut::<gaanim_scene::Opacity>(entity) {
                    opacity.0 = output.clamp(0.0, 1.0) as f32;
                }
            }
            SampledProperty::Signal => {
                if let Some(mut signal) = world.get_mut::<crate::signals::FloatSignal>(entity) {
                    signal.value = output;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::BuildChildrenTransformExt;
    use gaanim_core::kurbo::Shape;

    #[test]
    fn surrounding_rect_unions_live_bounds_and_interpolates_edges() {
        let mut world = World::new();
        let left_id = ObjectId::from_parts(1, 1);
        let right_id = ObjectId::from_parts(2, 1);
        world.spawn((
            gaanim_scene::MobjectId(left_id),
            SpatialTransform::new_2d(-20.0, 5.0),
            LocalBounds(gaanim_math::Bounds3D::new_2d(-10.0, -5.0, 10.0, 5.0)),
        ));
        world.spawn((
            gaanim_scene::MobjectId(right_id),
            SpatialTransform::new_2d(80.0, 25.0),
            LocalBounds(gaanim_math::Bounds3D::new_2d(-20.0, -10.0, 20.0, 10.0)),
        ));
        let empty = Arc::new(BezPath::new());
        let frame = world
            .spawn((
                gaanim_scene::MobjectId(ObjectId::from_parts(3, 1)),
                SpatialTransform::default(),
                Path2D(empty.clone()),
                PathSource(empty),
                LocalBounds(gaanim_math::Bounds3D::default()),
                SurroundingRect {
                    from: vec![left_id],
                    to: vec![right_id],
                    progress: 0.5,
                    padding: [2.0, 4.0, 6.0, 8.0],
                    corner_radius: 8.0,
                    last_bounds: None,
                },
            ))
            .id();

        surrounding_rect_system(&mut world);

        let bounds = world.get::<LocalBounds>(frame).unwrap().0;
        assert_eq!(bounds, gaanim_math::Bounds3D::new_2d(7.0, 1.5, 49.0, 24.5));
        assert!(world.get::<PathSource>(frame).unwrap().0.elements().len() > 4);

        let right_entity = world
            .query::<(Entity, &gaanim_scene::MobjectId)>()
            .iter(&world)
            .find_map(|(entity, id)| (id.0 == right_id).then_some(entity))
            .unwrap();
        world
            .get_mut::<SpatialTransform>(right_entity)
            .unwrap()
            .translation
            .x += 20.0;
        surrounding_rect_system(&mut world);
        assert_eq!(world.get::<LocalBounds>(frame).unwrap().0.max.x, 59.0);
    }

    #[test]
    fn regenerated_paths_preserve_create_reveal_progress() {
        let mut world = World::new();
        let empty = Arc::new(BezPath::new());
        let entity = world
            .spawn((
                Path2D(empty.clone()),
                PathSource(empty),
                LocalBounds(gaanim_math::Bounds3D::default()),
                crate::writing::PathReveal(0.25),
            ))
            .id();
        let mut full_path = BezPath::new();
        full_path.move_to((0.0, 0.0));
        full_path.line_to((100.0, 0.0));

        write_path(&mut world, entity, full_path);

        let source = world.get::<PathSource>(entity).unwrap();
        let visible = world.get::<Path2D>(entity).unwrap();
        assert_eq!(source.0.bounding_box().x1, 100.0);
        assert!(
            visible.0.bounding_box().x1 < 30.0,
            "every regenerated path must keep the partial Create trim"
        );
    }

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

    #[test]
    fn seek_rebuilds_fixed_simulations_before_dependent_frame_updaters() {
        let mut world = World::new();
        let simulated = Arc::new(Mutex::new(0.0));
        let observed = Arc::new(Mutex::new(0.0));

        let observed_value = observed.clone();
        let simulated_for_observer = simulated.clone();
        world.spawn((
            SpatialTransform::default(),
            Updater::new(move |_dt, _elapsed, _entity, _world| {
                *observed_value.lock().unwrap() = *simulated_for_observer.lock().unwrap();
                true
            }),
        ));

        let simulated_step = simulated.clone();
        let simulated_reset = simulated.clone();
        world.spawn((
            SpatialTransform::default(),
            Updater::new_simulation(
                move |dt, _elapsed, _entity, _world| {
                    *simulated_step.lock().unwrap() += dt;
                    true
                },
                move |_entity, _world| {
                    *simulated_reset.lock().unwrap() = 0.0;
                    true
                },
                0.25,
            )
            .unwrap(),
        ));

        seek_updaters(&mut world, 1.0);
        assert_eq!(*simulated.lock().unwrap(), 1.0);
        assert_eq!(
            *observed.lock().unwrap(),
            1.0,
            "derived parameters must observe the reconstructed simulation state"
        );
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
    fn simulation_waits_for_its_authored_start_time() {
        let mut world = World::new();
        let entity = spawn_accelerating_simulation(&mut world, 0.01);
        let updater = world
            .get::<Updater>(entity)
            .unwrap()
            .clone()
            .starting_at(2.0);
        world.entity_mut(entity).insert(updater);

        seek_updaters(&mut world, 1.5);
        assert_eq!(x(&world, entity), 0.0);

        seek_updaters(&mut world, 2.5);
        let after_half_second = x(&world, entity);
        assert!(after_half_second > 0.0);

        seek_updaters(&mut world, 1.0);
        assert_eq!(x(&world, entity), 0.0);

        seek_updaters(&mut world, 2.5);
        assert!((x(&world, entity) - after_half_second).abs() < 1e-12);
    }

    #[test]
    fn traced_path_starts_at_its_cursor_and_expires_old_samples() {
        let mut world = World::new();
        world.insert_resource(PlaybackState::default());
        let source = world.spawn(SpatialTransform::default()).id();
        let traced_path = TracedPath::new(source, 0.1, None)
            .starting_at(1.0)
            .with_dissipating_time(Some(1.0));
        let trace = world
            .spawn((traced_path, Path2D(Arc::new(BezPath::new()))))
            .id();

        world.resource_mut::<PlaybackState>().current_time = 0.5;
        traced_path_system(&mut world);
        assert!(world.get::<TracedPath>(trace).unwrap().points.is_empty());

        world.resource_mut::<PlaybackState>().current_time = 1.0;
        traced_path_system(&mut world);
        assert_eq!(world.get::<TracedPath>(trace).unwrap().sample_times, [1.0]);

        world
            .get_mut::<SpatialTransform>(source)
            .unwrap()
            .translation
            .x = 1.0;
        world.resource_mut::<PlaybackState>().current_time = 1.5;
        traced_path_system(&mut world);

        world
            .get_mut::<SpatialTransform>(source)
            .unwrap()
            .translation
            .x = 2.0;
        world.resource_mut::<PlaybackState>().current_time = 2.1;
        traced_path_system(&mut world);

        let traced_path = world.get::<TracedPath>(trace).unwrap();
        assert_eq!(traced_path.sample_times, [1.5, 2.1]);
        assert_eq!(
            traced_path
                .points
                .iter()
                .map(|point| point.x)
                .collect::<Vec<_>>(),
            [1.0, 2.0]
        );
    }

    #[test]
    fn traced_path_samples_the_source_in_the_trace_local_space() {
        let mut world = World::new();
        world.insert_resource(PlaybackState::default());

        let source_parent = world
            .spawn(
                SpatialTransform::new_2d(120.0, -35.0)
                    .with_rotation_2d(std::f64::consts::FRAC_PI_2)
                    .with_scale_2d(2.0, 3.0),
            )
            .id();
        let source = world
            .spawn(SpatialTransform::new_2d(10.0, 4.0))
            .set_parent_in_place(source_parent)
            .id();

        let trace_parent = world
            .spawn(SpatialTransform::new_2d(-40.0, 25.0).with_scale_2d(0.5, 2.0))
            .id();
        let trace = world
            .spawn((
                SpatialTransform::new_2d(6.0, -8.0),
                TracedPath::new(source, 0.0, None),
                Path2D(Arc::new(BezPath::new())),
            ))
            .set_parent_in_place(trace_parent)
            .id();

        traced_path_system(&mut world);

        let expected_world = entity_world_matrix(source, &world)
            .unwrap()
            .transform_point3(DVec3::ZERO);
        let expected_local = tracking_world_to_local(trace, expected_world, &world);
        let sampled = world.get::<TracedPath>(trace).unwrap().points[0];
        assert!(sampled.distance(expected_local) < 1e-9);

        let path = world.get::<Path2D>(trace).unwrap();
        assert!(matches!(
            path.0.elements().first(),
            Some(gaanim_core::kurbo::PathEl::MoveTo(point))
                if DVec3::new(point.x, point.y, 0.0).distance(expected_local) < 1e-9
        ));
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

    #[test]
    fn anchored_endpoint_uses_current_nested_rotation_and_scale() {
        let mut world = World::new();
        let parent_transform = SpatialTransform::new_2d(10.0, 20.0)
            .with_rotation_2d(std::f64::consts::FRAC_PI_2)
            .scale_uniform(2.0);
        let parent = world.spawn(parent_transform).id();
        let child = world
            .spawn((
                SpatialTransform::new_2d(5.0, 0.0),
                LocalBounds(gaanim_math::Bounds3D::new_2d(-2.0, -1.0, 2.0, 1.0)),
                ChildOf(parent),
            ))
            .id();
        let endpoint = TrackingEndpoint::EntityAnchor {
            entity: child,
            normalized: DVec3::new(1.0, 1.0, 0.0),
            offset: DVec3::new(1.0, 0.0, 0.0),
        };
        let expected = parent_transform.to_mat4().transform_point3(
            SpatialTransform::new_2d(5.0, 0.0)
                .to_mat4()
                .transform_point3(DVec3::new(3.0, 1.0, 0.0)),
        );
        let actual = resolve_tracking_endpoint(&endpoint, &world).unwrap();
        assert!(actual.distance(expected) < 1e-9);

        world
            .get_mut::<SpatialTransform>(parent)
            .unwrap()
            .translation
            .x += 7.0;
        let moved = resolve_tracking_endpoint(&endpoint, &world).unwrap();
        assert!((moved.x - actual.x - 7.0).abs() < 1e-9);
    }

    #[test]
    fn entity_bounds_include_renderable_descendants_of_structural_roots() {
        let mut world = World::new();
        let root = world.spawn(SpatialTransform::new_2d(10.0, 20.0)).id();
        world.spawn((
            SpatialTransform::new_2d(5.0, -3.0),
            LocalBounds(gaanim_math::Bounds3D::new_2d(-2.0, -1.0, 2.0, 1.0)),
            ChildOf(root),
        ));

        let bounds = resolve_entity_bounds(root, &world).expect("descendant bounds");
        assert!((bounds.min.x - 13.0).abs() < 1e-9);
        assert!((bounds.max.x - 17.0).abs() < 1e-9);
        assert!((bounds.min.y - 16.0).abs() < 1e-9);
        assert!((bounds.max.y - 18.0).abs() < 1e-9);
    }

    #[test]
    fn entity_bounds_fall_back_to_propagated_structural_bounds() {
        let mut world = World::new();
        let expected = gaanim_math::Bounds3D::new_2d(-345.0, -95.0, -175.0, 75.0);
        let root = world.spawn(WorldBounds(expected)).id();

        assert_eq!(resolve_entity_bounds(root, &world), Some(expected));
    }

    #[test]
    fn endpoint_distance_updates_signal_with_scale() {
        let mut world = World::new();
        let signal = world
            .spawn((
                crate::signals::FloatSignal::new(0.0),
                EndpointDistance {
                    from: TrackingEndpoint::Static(DVec3::new(0.0, 0.0, 0.0)),
                    to: TrackingEndpoint::Static(DVec3::new(3.0, 4.0, 8.0)),
                    scale: 2.0,
                },
            ))
            .id();
        endpoint_distance_system(&mut world);
        assert_eq!(
            world
                .get::<crate::signals::FloatSignal>(signal)
                .unwrap()
                .value,
            10.0
        );
    }

    #[test]
    fn zero_distance_and_negative_dimension_offset_are_stable() {
        let mut world = World::new();
        let point = DVec3::new(7.0, -3.0, 11.0);
        let signal = world
            .spawn((
                crate::signals::FloatSignal::new(123.0),
                EndpointDistance {
                    from: TrackingEndpoint::Static(point),
                    to: TrackingEndpoint::Static(point),
                    scale: 0.5,
                },
            ))
            .id();
        endpoint_distance_system(&mut world);
        assert_eq!(
            world
                .get::<crate::signals::FloatSignal>(signal)
                .unwrap()
                .value,
            0.0
        );

        let label = world.spawn(SpatialTransform::default()).id();
        world.spawn(DimensionLabelPlacement {
            label,
            from: TrackingEndpoint::Static(DVec3::new(0.0, 0.0, 0.0)),
            to: TrackingEndpoint::Static(DVec3::new(10.0, 0.0, 0.0)),
            offset: -20.0,
            gap: 5.0,
            orientation: DimensionLabelOrientation::Upright,
        });
        dimension_label_placement_system(&mut world);
        let transform = world.get::<SpatialTransform>(label).unwrap();
        assert!((transform.translation.x - 5.0).abs() < 1e-9);
        assert!((transform.translation.y + 25.0).abs() < 1e-9);
        assert!(transform.z_angle().abs() < 1e-9);
    }

    #[test]
    fn aligned_dimension_label_stays_readable() {
        let mut world = World::new();
        let label = world.spawn(SpatialTransform::default()).id();
        world.spawn(DimensionLabelPlacement {
            label,
            from: TrackingEndpoint::Static(DVec3::new(10.0, 0.0, 0.0)),
            to: TrackingEndpoint::Static(DVec3::new(-10.0, 0.0, 0.0)),
            offset: 20.0,
            gap: 5.0,
            orientation: DimensionLabelOrientation::Aligned,
        });
        dimension_label_placement_system(&mut world);
        let transform = world.get::<SpatialTransform>(label).unwrap();
        assert!((transform.translation.y + 25.0).abs() < 1e-9);
        assert!(transform.z_angle().abs() < 1e-9);
    }

    #[test]
    fn expression_between_and_polar_endpoints_are_reactive() {
        let mut world = World::new();
        let parameter_id = ObjectId::from_raw(42);
        let parameter = world
            .spawn(crate::signals::FloatSignal::new(
                std::f64::consts::FRAC_PI_2,
            ))
            .id();
        let scalar = TrackingScalar {
            source: crate::reactive::ScalarSource::signal(parameter_id),
            parameters: vec![(parameter_id, parameter)],
        };
        let polar = TrackingEndpoint::Polar {
            origin: Box::new(TrackingEndpoint::Static(DVec3::new(10.0, 20.0, 0.0))),
            radius: TrackingScalar {
                source: crate::reactive::ScalarSource::constant(5.0),
                parameters: Vec::new(),
            },
            angle: scalar,
        };
        let point = resolve_tracking_endpoint(&polar, &world).unwrap();
        assert!(point.distance(DVec3::new(10.0, 25.0, 0.0)) < 1e-9);

        let midpoint = TrackingEndpoint::Between {
            from: Box::new(TrackingEndpoint::Static(DVec3::ZERO)),
            to: Box::new(polar),
            alpha: 0.5,
            offset: DVec3::new(1.0, -1.0, 0.0),
        };
        let point = resolve_tracking_endpoint(&midpoint, &world).unwrap();
        assert!(point.distance(DVec3::new(6.0, 11.5, 0.0)) < 1e-9);

        let offset = TrackingEndpoint::Offset {
            origin: Box::new(TrackingEndpoint::Static(DVec3::new(-2.0, 8.0, 0.0))),
            dx: TrackingScalar {
                source: crate::reactive::ScalarSource::constant(4.0),
                parameters: Vec::new(),
            },
            dy: TrackingScalar {
                source: crate::reactive::ScalarSource::signal(parameter_id),
                parameters: vec![(parameter_id, parameter)],
            },
        };
        let point = resolve_tracking_endpoint(&offset, &world).unwrap();
        assert!(point.distance(DVec3::new(2.0, 8.0 + std::f64::consts::FRAC_PI_2, 0.0,)) < 1e-9);
    }

    #[test]
    fn angle_signal_supports_minor_and_major_sweeps() {
        let mut world = World::new();
        let minor = world
            .spawn((
                crate::signals::FloatSignal::new(0.0),
                EndpointAngle {
                    vertex: TrackingEndpoint::Static(DVec3::ZERO),
                    from: TrackingRay::Direction(DVec3::X),
                    to: TrackingRay::Direction(DVec3::Y),
                    sweep: AngleSweep::Minor,
                    scale: 180.0 / std::f64::consts::PI,
                },
            ))
            .id();
        let major = world
            .spawn((
                crate::signals::FloatSignal::new(0.0),
                EndpointAngle {
                    vertex: TrackingEndpoint::Static(DVec3::ZERO),
                    from: TrackingRay::Direction(DVec3::X),
                    to: TrackingRay::Direction(DVec3::Y),
                    sweep: AngleSweep::Major,
                    scale: 180.0 / std::f64::consts::PI,
                },
            ))
            .id();
        endpoint_angle_system(&mut world);
        assert!(
            (world
                .get::<crate::signals::FloatSignal>(minor)
                .unwrap()
                .value
                - 90.0)
                .abs()
                < 1e-9
        );
        assert!(
            (world
                .get::<crate::signals::FloatSignal>(major)
                .unwrap()
                .value
                - 270.0)
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn local_expression_endpoint_respects_space_transform() {
        let mut world = World::new();
        let space = world
            .spawn(
                SpatialTransform::new_2d(10.0, 20.0)
                    .with_scale_2d(2.0, 3.0)
                    .with_rotation_2d(std::f64::consts::FRAC_PI_2),
            )
            .id();
        let scalar = |value| TrackingScalar {
            source: crate::reactive::ScalarSource::constant(value),
            parameters: Vec::new(),
        };
        let endpoint = TrackingEndpoint::LocalExpression {
            space,
            x: scalar(4.0),
            y: scalar(5.0),
            z: scalar(0.0),
        };

        let resolved = resolve_tracking_endpoint(&endpoint, &world).unwrap();
        assert!(resolved.distance(DVec3::new(-5.0, 28.0, 0.0)) < 1e-9);
    }

    #[test]
    fn endpoint_follow_converts_world_position_into_parent_space() {
        let mut world = World::new();
        let parent = world.spawn(SpatialTransform::new_2d(10.0, 20.0)).id();
        let target = world
            .spawn((
                SpatialTransform::default(),
                EndpointFollow {
                    endpoint: TrackingEndpoint::Static(DVec3::new(30.0, 50.0, 0.0)),
                    offset: DVec3::new(2.0, -3.0, 0.0),
                    offset_space: FollowOffsetSpace::World,
                },
            ))
            .id();
        world.entity_mut(target).set_parent_in_place(parent);
        endpoint_follow_system(&mut world);
        let transform = world.get::<SpatialTransform>(target).unwrap();
        assert!(transform.translation.distance(DVec3::new(22.0, 27.0, 0.0)) < 1e-9);
    }

    #[test]
    fn tracking_line_respects_active_path_reveal() {
        let mut world = World::new();
        let empty = Arc::new(BezPath::new());
        let line = world
            .spawn((
                SpatialTransform::default(),
                Path2D(empty.clone()),
                PathSource(empty),
                LocalBounds(gaanim_math::Bounds3D::default()),
                crate::writing::PathReveal(0.5),
                TrackingLine::new(
                    TrackingEndpoint::Static(DVec3::ZERO),
                    TrackingEndpoint::Static(DVec3::new(100.0, 0.0, 0.0)),
                ),
            ))
            .id();

        tracking_line_system(&mut world);

        let source = world.get::<PathSource>(line).expect("full tracking path");
        assert!(matches!(
            source.0.elements().last(),
            Some(gaanim_core::kurbo::PathEl::LineTo(point)) if (point.x - 100.0).abs() < 1e-9
        ));
        let visible = world.get::<Path2D>(line).expect("revealed tracking path");
        assert!(matches!(
            visible.0.elements().last(),
            Some(gaanim_core::kurbo::PathEl::LineTo(point)) if (point.x - 50.0).abs() < 1e-9
        ));
    }

    fn sampled_world() -> World {
        let mut world = World::new();
        world.insert_resource(PlaybackState::default());
        world
    }

    #[test]
    fn sampled_series_rejects_invalid_input() {
        assert!(
            SampledSeriesDriver::new(
                vec![],
                vec![],
                SampledProperty::TranslateX,
                SampledInterpolation::Linear,
                1.0,
                0.0
            )
            .is_err()
        );
        assert!(
            SampledSeriesDriver::new(
                vec![0.0, 1.0],
                vec![1.0],
                SampledProperty::TranslateX,
                SampledInterpolation::Linear,
                1.0,
                0.0
            )
            .is_err()
        );
        assert!(
            SampledSeriesDriver::new(
                vec![1.0, 0.0],
                vec![1.0, 2.0],
                SampledProperty::TranslateX,
                SampledInterpolation::Linear,
                1.0,
                0.0
            )
            .is_err()
        );
        assert!(
            SampledSeriesDriver::new(
                vec![0.0, f64::NAN],
                vec![1.0, 2.0],
                SampledProperty::TranslateX,
                SampledInterpolation::Linear,
                1.0,
                0.0
            )
            .is_err()
        );
    }

    #[test]
    fn sampled_series_translation_is_relative_to_lazy_base() {
        let mut world = sampled_world();
        let entity = world
            .spawn(SpatialTransform::new_2d(-40.0, 10.0))
            .insert(
                SampledSeriesDriver::new(
                    vec![0.0, 1.0, 2.0],
                    vec![0.0, 4.0, -2.0],
                    SampledProperty::TranslateX,
                    SampledInterpolation::Linear,
                    10.0,
                    0.0,
                )
                .unwrap()
                .starting_at(1.0),
            )
            .id();

        // Antes de start_at la serie corre desde elapsed = 0 → muestra values[0].
        world.resource_mut::<PlaybackState>().current_time = 0.5;
        sampled_series_system(&mut world);
        assert_eq!(
            world.get::<SpatialTransform>(entity).unwrap().translation,
            DVec3::new(-40.0, 10.0, 0.0)
        );

        // elapsed = 1.5 interpola entre 4.0 y -2.0 → 1.0.
        world.resource_mut::<PlaybackState>().current_time = 2.5;
        sampled_series_system(&mut world);
        assert_eq!(
            world.get::<SpatialTransform>(entity).unwrap().translation.x,
            -40.0 + 10.0
        );

        // Fuera de rango: clamp al último valor (-2.0) aunque el tiempo siga.
        world.resource_mut::<PlaybackState>().current_time = 9.0;
        sampled_series_system(&mut world);
        assert_eq!(
            world.get::<SpatialTransform>(entity).unwrap().translation.x,
            -40.0 - 20.0
        );
    }

    #[test]
    fn sampled_series_step_interpolation_holds_previous_value() {
        let mut world = sampled_world();
        let entity = world
            .spawn(SpatialTransform::default())
            .insert(
                SampledSeriesDriver::new(
                    vec![0.0, 1.0],
                    vec![1.0, 3.0],
                    SampledProperty::TranslateY,
                    SampledInterpolation::Step,
                    1.0,
                    0.0,
                )
                .unwrap(),
            )
            .id();

        world.resource_mut::<PlaybackState>().current_time = 0.999;
        sampled_series_system(&mut world);
        assert_eq!(
            world.get::<SpatialTransform>(entity).unwrap().translation.y,
            1.0
        );

        world.resource_mut::<PlaybackState>().current_time = 1.0;
        sampled_series_system(&mut world);
        assert_eq!(
            world.get::<SpatialTransform>(entity).unwrap().translation.y,
            3.0
        );
    }

    #[test]
    fn sampled_series_absolute_properties_and_determinism_across_seeks() {
        let mut world = sampled_world();
        let scaled = world
            .spawn(SpatialTransform::default())
            .insert(
                SampledSeriesDriver::new(
                    vec![0.0, 1.0],
                    vec![1.0, 3.0],
                    SampledProperty::UniformScale,
                    SampledInterpolation::Linear,
                    0.5,
                    0.0,
                )
                .unwrap(),
            )
            .id();
        let signalled = world
            .spawn(crate::signals::FloatSignal::new(0.0))
            .insert(
                SampledSeriesDriver::new(
                    vec![0.0, 2.0],
                    vec![-1.0, 1.0],
                    SampledProperty::Signal,
                    SampledInterpolation::Linear,
                    2.0,
                    5.0,
                )
                .unwrap(),
            )
            .id();

        let evaluate = |world: &mut World, time: f64| {
            world.resource_mut::<PlaybackState>().current_time = time;
            sampled_series_system(world);
            (
                world.get::<SpatialTransform>(scaled).unwrap().scale.x,
                world
                    .get::<crate::signals::FloatSignal>(signalled)
                    .unwrap()
                    .value,
            )
        };

        let forward = evaluate(&mut world, 0.5);
        assert_eq!(forward, (1.0, 4.0));
        let later = evaluate(&mut world, 1.0);
        assert_eq!(later, (1.5, 5.0));

        // Seek hacia atrás y de nuevo hacia adelante: misma salida, sin replay.
        evaluate(&mut world, 0.0);
        assert_eq!(evaluate(&mut world, 0.5), forward);
        assert_eq!(evaluate(&mut world, 1.0), later);
        assert_eq!(evaluate(&mut world, 9.0), (1.5, 7.0));
    }

    #[test]
    fn sampled_series_drives_rotation_and_opacity() {
        let mut world = sampled_world();
        let rotated = world
            .spawn(SpatialTransform::default().with_rotation_2d(0.5))
            .insert(
                SampledSeriesDriver::new(
                    vec![0.0, 1.0],
                    vec![0.0, 1.0],
                    SampledProperty::RotateZ,
                    SampledInterpolation::Linear,
                    1.0,
                    0.0,
                )
                .unwrap(),
            )
            .id();
        let faded = world
            .spawn(SpatialTransform::default())
            .insert(gaanim_scene::Opacity(1.0))
            .insert(
                SampledSeriesDriver::new(
                    vec![0.0, 1.0],
                    vec![1.0, -1.0],
                    SampledProperty::Opacity,
                    SampledInterpolation::Linear,
                    1.0,
                    0.0,
                )
                .unwrap(),
            )
            .id();

        world.resource_mut::<PlaybackState>().current_time = 1.0;
        sampled_series_system(&mut world);

        let rotation = world.get::<SpatialTransform>(rotated).unwrap().rotation;
        assert!((rotation.to_scaled_axis().z - 1.5).abs() < 1e-12);
        // La opacidad se clampea a [0, 1] aunque la serie la cruce.
        assert_eq!(world.get::<gaanim_scene::Opacity>(faded).unwrap().0, 0.0);
    }
}
