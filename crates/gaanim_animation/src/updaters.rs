use crate::tween::DeltaTime;
use bevy::prelude::{Children, Component, DetectChangesMut, With};
use gaanim_core::ObjectId;
use gaanim_core::glam::{DMat4, DQuat, DVec2, DVec3};
use gaanim_core::kurbo::{BezPath, Shape};
use gaanim_math::SpatialTransform;
use gaanim_scene::prelude::{ChildOf, Entity, World};
use gaanim_scene::{LocalBounds, Path2D, PathSource, StepRate, WorldBounds, stepped};
use std::collections::{HashMap, HashSet};
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
    /// Set by a seek that already brought the updaters to `current_time`,
    /// as a seek through stepped time does: `updater_system` leaves them
    /// for the rest of the frame.
    pub updaters_current: bool,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            is_playing: true,
            scaled_dt: 0.0,
            current_time: 0.0,
            updaters_current: false,
        }
    }
}

/// Time of ambient motion: shader backgrounds and post-processing.
///
/// A presentation resting at a stop leaves the timeline still, but its
/// backgrounds should not freeze: while it rests, `rest` gathers the wall
/// clock and ambient motion runs at the timeline's time plus `rest`. It is
/// never reset, so the motion carries on without a jump when the timeline
/// moves again. Outside presentations it stays zero, and previews, seeks and
/// exports draw ambient motion at the exact timeline time.
#[derive(bevy::prelude::Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct AmbientClock {
    /// Seconds the presentation spent resting so far.
    pub rest: f64,
}

impl AmbientClock {
    /// Longest wall-clock step taken in one frame, so a stall does not jump.
    pub const MAX_STEP: f64 = 0.1;

    /// Ambient time at timeline time `time`.
    pub fn at(&self, time: f64) -> f64 {
        time + self.rest
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
        let mut debug = f.debug_struct("Updater");
        debug
            .field("elapsed", &self.elapsed)
            .field("start_at", &self.start_at)
            .field("stop_at", &self.stop_at)
            .field("time_based", &self.time_based)
            .field("fixed_dt", &self.fixed_dt)
            .field("simulation_elapsed", &self.simulation_elapsed)
            .field("accumulator", &self.accumulator)
            .field("has_reset", &self.reset.is_some())
            .field("initial_translation", &self.initial_translation);
        if gaanim_core::fingerprint::identity_debug() {
            debug
                .field("func", &gaanim_core::fingerprint::identity(&*self.func))
                .field(
                    "reset",
                    &self
                        .reset
                        .as_ref()
                        .map(|reset| gaanim_core::fingerprint::identity(&**reset)),
                );
        }
        debug.finish_non_exhaustive()
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

    /// Whether this is a fixed-step simulation, which owns its entity's
    /// translation: a seek restores the starting translation and replays it.
    pub fn is_simulation(&self) -> bool {
        self.fixed_dt.is_some()
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
        if let Some(initial_translation) = job.reset_translation
            && let Some(mut transform) = world.get_mut::<SpatialTransform>(job.entity)
        {
            transform.translation = initial_translation;
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
    let mut query = world.query::<(
        Entity,
        &mut Updater,
        Option<&SpatialTransform>,
        Option<&StepRate>,
    )>();
    for (entity, mut updater, transform, step) in query.iter_mut(world) {
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

        // A stepped drawable's updaters see its time on its grid.
        let previous_active_elapsed = (stepped(step, previous_elapsed) - updater.start_at).max(0.0);
        let next_active_elapsed = (stepped(step, next_elapsed) - updater.start_at).max(0.0);
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
    let mut query = world.query::<(
        Entity,
        &mut Updater,
        Option<&SpatialTransform>,
        Option<&StepRate>,
    )>();
    for (entity, mut updater, transform, step) in query.iter_mut(world) {
        let elapsed = updater
            .stop_at
            .map(|stop_at| target_time.min(stop_at))
            .unwrap_or(target_time);
        updater.elapsed = elapsed;
        let active_elapsed = (stepped(step, elapsed) - updater.start_at).max(0.0);

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
    let (is_playing, current) = world
        .get_resource::<PlaybackState>()
        .map_or((true, false), |s| (s.is_playing, s.updaters_current));

    if !is_playing || current {
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
            transform.rotation *= delta_rot;
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

        if let Some(pos) = target_pos
            && let Some(mut transform) = world.get_mut::<SpatialTransform>(entity)
        {
            let target_dest = pos + offset;
            if smoothing <= 0.0 {
                transform.translation = target_dest;
            } else {
                let t = (1.0 - (-dt / smoothing).exp()).clamp(0.0, 1.0);
                transform.translation = transform.translation.lerp(target_dest, t);
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

/// A smooth curve through the samples of a trace: a centripetal Catmull-Rom
/// spline as cubic Béziers, so a trace sampled once a frame reads as the
/// curve its source moved along, without the loops and overshoots of a
/// uniform spline where samples are unevenly spaced.
pub fn traced_path_geometry(points: &[DVec3]) -> BezPath {
    use gaanim_core::kurbo::{Point, Vec2};
    // Samples closer than this to the previous one add nothing a stroke can
    // show, and a segment that short gives the stroke's start or end no
    // direction: the renderer then leaves its outline open.
    const MERGE_DISTANCE: f64 = 1e-4;
    let mut path = BezPath::new();
    let mut kept: Vec<Point> = Vec::with_capacity(points.len());
    for (index, point) in points.iter().enumerate() {
        let point = Point::new(point.x, point.y);
        let close = |last: &Point| last.distance(point) < MERGE_DISTANCE;
        if kept.last().is_some_and(close) {
            // The tail stays where it expires and the head where its
            // source is, so a sample next to the head gives way.
            if index + 1 < points.len() {
                continue;
            }
            while kept.len() > 1 && kept.last().is_some_and(close) {
                kept.pop();
            }
            if kept.last().is_some_and(close) {
                continue;
            }
        }
        kept.push(point);
    }
    let points = kept;
    let Some(first) = points.first() else {
        return path;
    };
    path.move_to(*first);
    if points.len() < 3 {
        for point in &points[1..] {
            path.line_to(*point);
        }
        return path;
    }
    // At an end, the curve leaves along the parabola through the three
    // samples there, spaced by their distances: as the tail expires toward
    // its next sample the first one is short, and evenly spaced it would
    // turn back.
    let end_control = |end: Point, next: Point, after: Point| -> Point {
        let (u, v) = (next - end, after - next);
        let (a, b) = (u.hypot(), v.hypot());
        end + (u * ((2.0 * a + b) / (a + b)) - v * (a * a / (b * (a + b)))) / 3.0
    };
    // The control point from `from` toward `to`, past `before`.
    let control = |before: Point, from: Point, to: Point| -> Point {
        let (d1, d2) = ((from - before).hypot().sqrt(), (to - from).hypot().sqrt());
        if d1 < 1e-9 || d2 < 1e-9 {
            return from + (to - from) / 3.0;
        }
        let blend: Vec2 = (to.to_vec2() * (d1 * d1) - before.to_vec2() * (d2 * d2)
            + from.to_vec2() * (2.0 * d1 * d1 + 3.0 * d1 * d2 + d2 * d2))
            / (3.0 * d1 * (d1 + d2));
        blend.to_point()
    };
    let last = points.len() - 1;
    for index in 0..last {
        let (from, to) = (points[index], points[index + 1]);
        let start = if index == 0 {
            end_control(from, to, points[2])
        } else {
            control(points[index - 1], from, to)
        };
        let end = if index + 1 == last {
            end_control(to, from, points[last - 2])
        } else {
            control(points[index + 2], to, from)
        };
        path.curve_to(start, end, to);
    }
    path
}

/// Drop the samples older than `dissipating_time` at `current_time`; the
/// trace then starts where its source was at that cutoff. Whether any was
/// dropped.
pub fn expire_trace_samples(
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
    // The trace starts where its source was at the cutoff, between the last
    // expired sample and the next one, so it shortens smoothly.
    if expired < points.len() {
        let (before, after) = (expired - 1, expired);
        let span = sample_times[after] - sample_times[before];
        let alpha = if span > f64::EPSILON {
            ((cutoff - sample_times[before]) / span).clamp(0.0, 1.0)
        } else {
            1.0
        };
        points[before] = points[before].lerp(points[after], alpha);
        sample_times[before] = cutoff;
        points.drain(0..before);
        sample_times.drain(0..before);
    } else {
        points.drain(0..expired);
        sample_times.drain(0..expired);
    }
    true
}

/// Posición del origen de `source` en el espacio local del rastro 2D. La usan
/// tanto la reproducción como la reconstrucción tras un seek.
pub fn traced_source_position(trace: Entity, source: Entity, world: &World) -> Option<DVec3> {
    entity_world_matrix(source, world)
        .map(|matrix| matrix.transform_point3(DVec3::ZERO))
        .map(|position| tracking_world_to_local(trace, position, world))
}

/// Evalúa en `time` los sistemas que posicionan entidades de forma reactiva,
/// en el mismo orden que la fase `Updaters`, sin avanzar los updaters. La
/// reconstrucción de rastros tras un seek lo usa para muestrear las fuentes
/// tal como se mueven en reproducción (series muestreadas, bindings, `follow`).
pub fn evaluate_reactive_positions(world: &mut World, time: f64) {
    if let Some(mut playback) = world.get_resource_mut::<PlaybackState>() {
        playback.current_time = time;
    }
    sampled_series_system(world);
    crate::apply_property_bindings(world, time);
    crate::signals::curve_bindings_pre_pass_system(world);
    crate::signals::position_binding_system(world);
    mechanism_binding_system(world);
    endpoint_follow_system(world);
    crate::signals::followed_position_binding_system(world);
}

/// Whether anything but animation clips and property bindings can move
/// drawables in `world`: the updaters or the other systems of
/// [`evaluate_reactive_positions`]. A property binding sets its target from
/// the time and signals alone, so without these a drawable that no binding
/// targets holds still once its clips have ended.
pub fn has_reactive_positions(world: &mut World) -> bool {
    reactive_positions_on(world, |_| true)
}

/// Whether a reactive position sits on one of `entities`: each moves the
/// entity it is on, so the others are left where clips put them.
pub fn reactive_positions_move(
    world: &mut World,
    entities: &std::collections::HashSet<Entity>,
) -> bool {
    reactive_positions_on(world, |entity| entities.contains(&entity))
}

fn reactive_positions_on(world: &mut World, moves: impl Fn(Entity) -> bool) -> bool {
    fn any<C: Component>(world: &mut World, moves: &impl Fn(Entity) -> bool) -> bool {
        world
            .query_filtered::<Entity, With<C>>()
            .iter(world)
            .any(moves)
    }
    any::<Updater>(world, &moves)
        || any::<SampledSeriesDrivers>(world, &moves)
        || any::<crate::signals::PointOnCurve>(world, &moves)
        || any::<crate::signals::TangentOnCurve>(world, &moves)
        || any::<crate::signals::NormalOnCurve>(world, &moves)
        || any::<crate::signals::CurvatureOnCurve>(world, &moves)
        || any::<crate::signals::PositionBinding>(world, &moves)
        || any::<RotationBinding>(world, &moves)
        || any::<RotationTranslationBinding>(world, &moves)
        || any::<EndpointFollow>(world, &moves)
        || any::<TrackingAngle>(world, &moves)
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
        let source_pos = traced_source_position(trace_entity, source_entity, world);

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
                changed |=
                    expire_trace_samples(points, sample_times, current_time, dissipating_time);

                if let Some(pos) = source_pos {
                    let should_add = match traced_path.points.last() {
                        Some(last_point) => last_point.distance(pos) >= min_distance,
                        None => true,
                    };

                    if should_add {
                        traced_path.points.push(pos);
                        traced_path.sample_times.push(current_time);
                        changed = true;
                        if let Some(max) = max_points
                            && traced_path.points.len() > max
                        {
                            let overflow = traced_path.points.len() - max;
                            traced_path.points.drain(0..overflow);
                            traced_path.sample_times.drain(0..overflow);
                        }
                    }
                }
            }
            if changed {
                trace_updates.push((trace_entity, traced_path_geometry(&traced_path.points)));
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
#[allow(clippy::type_complexity)]
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
                    changed |=
                        expire_trace_samples(points, sample_times, current_time, dissipating_time);
                    let should_push = source_pos.is_some_and(|pos| match tp.points.last() {
                        Some(last) => last.distance(pos) >= min_distance,
                        None => true,
                    });
                    if should_push {
                        let pos = source_pos.expect("checked above");
                        tp.points.push(pos);
                        tp.sample_times.push(current_time);
                        changed = true;
                        if let Some(max) = max_points
                            && tp.points.len() > max
                        {
                            let overflow = tp.points.len() - max;
                            tp.points.drain(0..overflow);
                            tp.sample_times.drain(0..overflow);
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
        z: TrackingScalar,
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
        alpha: TrackingScalar,
        offset: DVec3,
    },
    /// Polar point around another reactive endpoint.
    Polar {
        origin: Box<TrackingEndpoint>,
        radius: TrackingScalar,
        angle: TrackingScalar,
    },
    /// Where the circle of `radius` around `entity`'s origin, in its local
    /// frame, meets the line toward another endpoint (see [`rim`]).
    Rim {
        entity: Entity,
        toward: Box<TrackingEndpoint>,
        radius: f64,
    },
    /// A pixel of an image's or a video's source, from its top-left corner
    /// with y down, through the media's frame, crop and transforms.
    MediaPixel { entity: Entity, x: f64, y: f64 },
    /// Where the main camera draws another endpoint on the frame, in the
    /// units a HUD drawable is placed in (see
    /// [`gaanim_math::Camera::hud_point`]).
    Screen(Box<TrackingEndpoint>),
}

impl TrackingEndpoint {
    /// Whether this endpoint is, or is built on, a point on the screen.
    pub fn reads_the_screen(&self) -> bool {
        match self {
            Self::Screen(_) => true,
            Self::Offset { origin, .. } | Self::Polar { origin, .. } => origin.reads_the_screen(),
            Self::Between { from, to, .. } => from.reads_the_screen() || to.reads_the_screen(),
            Self::Rim { toward, .. } => toward.reads_the_screen(),
            _ => false,
        }
    }
}

/// The scene point that shows source pixel `(x, y)` of the image or video
/// `entity`: through its [`gaanim_scene::MediaFrame`], read as the timeline
/// left it, so an animated crop moves the point in the same frame.
pub fn media_pixel_point(entity: Entity, x: f64, y: f64, world: &World) -> Option<DVec3> {
    let local = world.get::<gaanim_scene::MediaFrame>(entity)?.geometry().1
        * gaanim_core::kurbo::Point::new(x, y);
    entity_world_matrix(entity, world)
        .map(|matrix| matrix.transform_point3(DVec3::new(local.x, local.y, 0.0)))
}

/// Component: an invisible quad over a rectangle of source pixels of an
/// image or video, kept on them as the media's frame, crop and transforms
/// change. Its path and bounds are in scene coordinates, as a surrounding
/// rectangle's are, so frames, notations and spotlights can target it.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct MediaRegion {
    pub media: Entity,
    /// Source pixels, from the top-left corner with y down.
    pub rect: gaanim_core::kurbo::Rect,
}

impl MediaRegion {
    /// The region's corners in scene coordinates, clockwise from the
    /// top-left pixel.
    pub fn corners(&self, world: &World) -> Option<[DVec3; 4]> {
        let rect = self.rect;
        let corner = |x, y| media_pixel_point(self.media, x, y, world);
        Some([
            corner(rect.x0, rect.y0)?,
            corner(rect.x1, rect.y0)?,
            corner(rect.x1, rect.y1)?,
            corner(rect.x0, rect.y1)?,
        ])
    }
}

/// The closed quad through `corners` and its bounds.
pub fn media_region_shape(corners: [DVec3; 4]) -> (BezPath, gaanim_math::Bounds3D) {
    let mut path = BezPath::new();
    path.move_to((corners[0].x, corners[0].y));
    for corner in &corners[1..] {
        path.line_to((corner.x, corner.y));
    }
    path.close_path();
    let min = corners
        .iter()
        .copied()
        .reduce(DVec3::min)
        .unwrap_or_default();
    let max = corners
        .iter()
        .copied()
        .reduce(DVec3::max)
        .unwrap_or_default();
    (
        path,
        gaanim_math::Bounds3D::new_2d(min.x, min.y, max.x, max.y),
    )
}

/// Puts every [`MediaRegion`] back on its pixels, before the surrounding
/// rectangles that frame it measure it.
pub fn media_region_system(world: &mut World) {
    let regions = world
        .query::<(Entity, &MediaRegion)>()
        .iter(world)
        .map(|(entity, region)| (entity, *region))
        .collect::<Vec<_>>();
    for (entity, region) in regions {
        let Some(corners) = region.corners(world) else {
            continue;
        };
        let (path, bounds) = media_region_shape(corners);
        // A still region keeps its path, so its fragment is not rebuilt.
        if world
            .get::<Path2D>(entity)
            .is_some_and(|current| *current.0 == path)
        {
            continue;
        }
        write_path(world, entity, path);
        if let Some(mut local) = world.get_mut::<LocalBounds>(entity) {
            local.0 = bounds;
        }
    }
}

/// Scalar expression with its construction-time parameter ids resolved to ECS entities.
#[derive(Debug, Clone)]
pub struct TrackingScalar {
    pub source: crate::reactive::ScalarSource,
    pub parameters: Vec<(ObjectId, Entity)>,
}

impl TrackingScalar {
    /// The value of the `FloatSignal` on `entity`, known to expressions as `id`.
    pub fn signal(id: ObjectId, entity: Entity) -> Self {
        Self {
            source: crate::reactive::ScalarSource::signal(id),
            parameters: vec![(id, entity)],
        }
    }

    pub fn evaluate(&self, world: &World) -> Option<f64> {
        crate::reactive::ResolvedScalarSource {
            source: self.source.clone(),
            parameters: self.parameters.clone(),
        }
        .evaluate(world)
    }
}

impl From<f64> for TrackingScalar {
    fn from(value: f64) -> Self {
        Self {
            source: crate::reactive::ScalarSource::constant(value),
            parameters: Vec::new(),
        }
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
    /// Timeline time the follow starts at, the cursor where it was set:
    /// before it the drawable keeps its authored transform.
    pub since: f64,
}

impl EndpointFollow {
    /// Whether the follow places its drawable at timeline time `time`.
    pub fn holds_at(&self, time: f64) -> bool {
        time + 1e-9 >= self.since
    }
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
    /// Relleno: el valor elige un color de la rampa de `SampledColors`.
    Fill,
    /// Orientación 3D completa: cada muestra es un cuaternión en el marco del
    /// padre, interpolado por el arco más corto.
    Orientation,
}

/// Rampa de un canal `Fill`: el valor muestreado se lee en `domain` y elige
/// un color de `ramp`, con clamp a sus extremos.
#[derive(Debug, Clone, PartialEq)]
pub struct SampledColors {
    pub ramp: crate::falloff::ColorRamp,
    pub domain: (f64, f64),
}

impl SampledColors {
    /// Color del valor `value`. Un dominio sin anchura da el primer color.
    pub fn at(&self, value: f64) -> gaanim_core::peniko::Color {
        let (low, high) = self.domain;
        let t = if high > low {
            (value - low) / (high - low)
        } else {
            0.0
        };
        self.ramp.at(t)
    }
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
///
/// Antes de `start_at` el driver no escribe: el canal queda en manos de lo
/// que lo animara antes (un `fade_in`, un `.animate`), salvo que el driver
/// se cree con `holding_before_start`.
///
/// Una entidad guarda sus drivers en `SampledSeriesDrivers`, un canal por
/// propiedad, de modo que conducir `x` e `y` por separado no se pisa.
#[derive(Clone)]
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
    /// Rampa de colores del canal `Fill`.
    pub colors: Option<SampledColors>,
    /// Mantener la primera muestra también antes de `start_at`.
    pub hold_before_start: bool,
    /// Cuaterniones del canal `Orientation`, uno por instante.
    pub orientations: Option<Arc<[DQuat]>>,
}

impl std::fmt::Debug for SampledSeriesDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("SampledSeriesDriver");
        debug
            .field("samples", &self.times.len())
            .field("interpolation", &self.interpolation)
            .field("property", &self.property)
            .field("scale", &self.scale)
            .field("offset", &self.offset)
            .field("start_at", &self.start_at)
            .field("stop_at", &self.stop_at)
            .field("base", &self.base)
            .field("colors", &self.colors)
            .field("hold_before_start", &self.hold_before_start)
            .field(
                "orientations",
                &self
                    .orientations
                    .as_ref()
                    .map(|orientations| orientations.len()),
            );
        if gaanim_core::fingerprint::identity_debug() {
            debug
                .field("times", &self.times)
                .field("values", &self.values);
        }
        debug.finish()
    }
}

impl SampledSeriesDriver {
    /// Las muestras de traslación y rotación pasan a ser valores absolutos
    /// (`offset + scale * muestra`) en lugar de sumarse a la pose declarada.
    pub fn absolute(mut self) -> Self {
        self.base = Some(0.0);
        self
    }

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
            colors: None,
            hold_before_start: false,
            orientations: None,
        })
    }

    /// Un driver del canal `Orientation`: en cada instante de `times`, la
    /// orientación absoluta `orientations` (normalizada) en el marco del
    /// padre. `Linear` interpola por el arco más corto (slerp).
    pub fn orientations(
        times: Vec<f64>,
        orientations: Vec<DQuat>,
        interpolation: SampledInterpolation,
    ) -> Result<Self, InvalidSampledSeries> {
        if orientations.len() != times.len()
            || !orientations
                .iter()
                .all(|rotation| rotation.is_finite() && rotation.length_squared() > 1e-12)
        {
            return Err(InvalidSampledSeries);
        }
        let mut driver = Self::new(
            times,
            vec![0.0; orientations.len()],
            SampledProperty::Orientation,
            interpolation,
            1.0,
            0.0,
        )?;
        driver.orientations = Some(orientations.into_iter().map(DQuat::normalize).collect());
        driver.base = Some(0.0);
        Ok(driver)
    }

    /// Orientación en `elapsed` segundos desde `start_at`, con clamp a los
    /// extremos de la serie; `None` fuera del canal `Orientation`.
    fn sample_orientation(&self, elapsed: f64) -> Option<DQuat> {
        let orientations = self.orientations.as_ref()?;
        let last = self.times.len() - 1;
        if elapsed <= self.times[0] {
            return Some(orientations[0]);
        }
        if elapsed >= self.times[last] {
            return Some(orientations[last]);
        }
        let index = self.times.partition_point(|time| *time <= elapsed) - 1;
        let (from, to) = (orientations[index], orientations[index + 1]);
        Some(match self.interpolation {
            SampledInterpolation::Step => from,
            SampledInterpolation::Linear => {
                let (t0, t1) = (self.times[index], self.times[index + 1]);
                if t1 <= t0 {
                    to
                } else {
                    // The shorter way round: q and -q are the same turn.
                    let to = if from.dot(to) < 0.0 { -to } else { to };
                    from.slerp(to, (elapsed - t0) / (t1 - t0))
                }
            }
        })
    }

    /// Mantiene la primera muestra antes de `start_at` en lugar de dejar el
    /// canal a lo que lo animara antes (los valores en vivo de una votación).
    pub fn holding_before_start(mut self) -> Self {
        self.hold_before_start = true;
        self
    }

    /// Si la serie ya empezó en `time`.
    pub fn has_started(&self, time: f64) -> bool {
        time + f64::EPSILON >= self.start_at
    }

    /// Da al driver la rampa de su canal `Fill`: el valor
    /// `offset + scale * muestra` se lee en `domain` (por defecto, el rango
    /// de la serie) y elige un color de `colors`.
    pub fn with_colors(
        mut self,
        colors: Vec<gaanim_core::peniko::Color>,
        domain: Option<(f64, f64)>,
    ) -> Result<Self, InvalidSampledSeries> {
        if colors.is_empty() {
            return Err(InvalidSampledSeries);
        }
        let domain = match domain {
            Some((low, high)) if low.is_finite() && high.is_finite() && low < high => (low, high),
            Some(_) => return Err(InvalidSampledSeries),
            None => self
                .values
                .iter()
                .map(|value| self.offset + self.scale * value)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), value| {
                    (low.min(value), high.max(value))
                }),
        };
        self.colors = Some(SampledColors {
            ramp: crate::falloff::ColorRamp(colors),
            domain,
        });
        Ok(self)
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

/// Drivers de series muestreadas de una entidad, como mucho uno por
/// `SampledProperty`. Canales distintos (por ejemplo `TranslateX` y
/// `TranslateY`) son independientes y conviven.
#[derive(Component, Debug, Clone, Default)]
pub struct SampledSeriesDrivers(pub Vec<SampledSeriesDriver>);

impl From<SampledSeriesDriver> for SampledSeriesDrivers {
    fn from(driver: SampledSeriesDriver) -> Self {
        Self(vec![driver])
    }
}

impl SampledSeriesDrivers {
    /// Añade el driver. Sobre la misma propiedad, reemplaza los drivers que
    /// empiezan en su mismo instante o después y releva a los anteriores
    /// desde su inicio, sin borrar lo que condujeron antes.
    pub fn attach(&mut self, driver: SampledSeriesDriver) {
        self.0.retain(|existing| {
            existing.property != driver.property
                || existing.start_at + f64::EPSILON < driver.start_at
        });
        self.0.push(driver);
    }

    /// Primer driver del canal `property`, si existe.
    pub fn get(&self, property: SampledProperty) -> Option<&SampledSeriesDriver> {
        self.0.iter().find(|driver| driver.property == property)
    }

    /// Driver que conduce el canal `property` en `time`: el último que ya
    /// empezó, o uno que mantenga su primera muestra antes de empezar.
    pub fn active(&self, property: SampledProperty, time: f64) -> Option<&SampledSeriesDriver> {
        let channel = || {
            self.0
                .iter()
                .filter(move |driver| driver.property == property)
        };
        channel()
            .filter(|driver| driver.has_started(time))
            .max_by(|left, right| left.start_at.total_cmp(&right.start_at))
            .or_else(|| {
                channel()
                    .filter(|driver| driver.hold_before_start)
                    .min_by(|left, right| left.start_at.total_cmp(&right.start_at))
            })
    }

    /// Congela en `time` los canales que ya corrían en ese instante
    /// (semántica de `RemoveUpdater`); los adjuntados después no se tocan.
    pub fn stop_at(&mut self, time: f64) {
        for driver in self.0.iter_mut().filter(|driver| driver.start_at <= time) {
            driver.stop_at = Some(driver.stop_at.map_or(time, |stop| stop.min(time)));
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

/// Scene-space side on which a dimension is drawn, independent of the
/// `from` → `to` direction that decides the sign of a raw offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimensionSide {
    Left,
    Right,
    Above,
    Below,
}

impl DimensionSide {
    /// Signed offset along the dimension normal `(-dy, dx)` that places a
    /// dimension `distance` away on this side of the `from` → `to` segment.
    ///
    /// When the segment runs along the requested direction (for example
    /// `Above` on a vertical dimension) no side is preferred and the offset
    /// stays positive.
    pub fn signed_offset(self, distance: f64, from: DVec2, to: DVec2) -> f64 {
        let delta = to - from;
        let normal = DVec2::new(-delta.y, delta.x);
        let wanted = match self {
            Self::Left => DVec2::NEG_X,
            Self::Right => DVec2::X,
            Self::Above => DVec2::Y,
            Self::Below => DVec2::NEG_Y,
        };
        let alignment = normal.dot(wanted);
        let distance = distance.abs();
        if alignment < -1e-9 * delta.length() {
            -distance
        } else {
            distance
        }
    }

    /// Resolve an authored offset: with a side, only its magnitude matters.
    pub fn resolve(side: Option<Self>, offset: f64, from: DVec2, to: DVec2) -> f64 {
        side.map_or(offset, |side| side.signed_offset(offset, from, to))
    }
}

/// Places a dimension annotation at the midpoint of its displaced baseline.
#[derive(Component, Debug, Clone)]
pub struct DimensionLabelPlacement {
    pub label: Entity,
    pub from: TrackingEndpoint,
    pub to: TrackingEndpoint,
    pub offset: f64,
    /// Scene side that overrides the sign of `offset` every frame.
    pub side: Option<DimensionSide>,
    pub gap: f64,
    pub orientation: DimensionLabelOrientation,
    /// Keep upright labels clear of steep lines by their width, not only their height.
    pub clear_label_width: bool,
}

/// Componente que regenera un `Path2D` de línea recta entre dos endpoints cada frame.
///
/// A diferencia de una animación de línea (que interpola de A a B en una duración),
/// `TrackingLine` actualiza continuamente sus endpoints para seguir entidades.
#[derive(Component)]
pub struct TrackingLine {
    pub from: TrackingEndpoint,
    pub to: TrackingEndpoint,
    /// Dash and gap lengths, for a dashed line.
    pub dashes: Option<(f64, f64)>,
}

/// Filled arrow following a polyline of reactive world-space endpoints.
#[derive(Component)]
pub struct TrackingConnector {
    pub points: Vec<TrackingEndpoint>,
    pub head_length: f64,
    pub head_width: f64,
    pub body_width: f64,
    pub max_head_ratio: Option<f64>,
    /// Visible fraction of the polyline measured from the tail, animated by
    /// `grow_arrow`. The head keeps its full size and rides the tip.
    pub progress: f64,
    /// The static points are the connector's own geometry, in its local
    /// frame, instead of positions in the scene: a box places it like any
    /// other shape and it moves with it.
    pub local: bool,
}

/// A lit 3D line between two reactive endpoints. Its mesh lies along Y, one
/// unit long and centered on its origin; every frame `tracking_line_system`
/// lays it from `from` toward `to` by its transform alone, so the mesh never
/// changes.
#[derive(Component)]
pub struct TrackingLine3D {
    pub from: TrackingEndpoint,
    pub to: TrackingEndpoint,
    /// How far toward `to` the line reaches from `from`, as a fraction of the
    /// way, which `create` animates.
    pub progress: f64,
}

impl TrackingLine3D {
    pub fn new(from: TrackingEndpoint, to: TrackingEndpoint) -> Self {
        Self {
            from,
            to,
            progress: 1.0,
        }
    }
}

/// The transform, in the frame of a parent whose world matrix is `parent`,
/// that lays a [`TrackingLine3D`]'s unit mesh from `from` toward `to`, as
/// far as `progress` of the way, both points in the scene. Its sides keep
/// their thickness. A line too short to see shrinks to nothing, caps and all.
pub fn line_3d_transform(parent: DMat4, from: DVec3, to: DVec3, progress: f64) -> SpatialTransform {
    let inverse = parent.inverse();
    let from = inverse.transform_point3(from);
    let to = inverse.transform_point3(to);
    let end = from.lerp(to, progress.clamp(0.0, 1.0));
    let span = end - from;
    let length = span.length();
    if !length.is_finite() || length <= 1e-9 || !from.is_finite() {
        return SpatialTransform {
            translation: if from.is_finite() { from } else { DVec3::ZERO },
            scale: DVec3::ZERO,
            ..SpatialTransform::default()
        };
    }
    SpatialTransform {
        translation: (from + end) * 0.5,
        rotation: DQuat::from_rotation_arc(DVec3::Y, span / length),
        scale: DVec3::new(1.0, length, 1.0),
        ..SpatialTransform::default()
    }
}

/// A card background reads its parent's resolved local layout box.
#[derive(Component)]
pub struct LayoutBackground {
    pub container: Entity,
    pub radius: f64,
}

/// Immutable layout-box history, evaluated at the absolute playback cursor.
#[derive(Component, Default)]
pub struct LayoutBoundsTrack(Vec<(f64, f64, gaanim_math::Bounds3D, gaanim_math::Bounds3D)>);

impl LayoutBoundsTrack {
    fn at(&self, time: f64) -> Option<gaanim_math::Bounds3D> {
        let &(start, duration, from, to) = self
            .0
            .iter()
            .rev()
            .find(|sample| sample.0 <= time)
            .or_else(|| self.0.first())?;
        let progress = if duration > 0.0 {
            gaanim_math::RateFunc::Smooth.evaluate(((time - start) / duration).clamp(0.0, 1.0))
        } else {
            1.0
        };
        Some(lerp_bounds(from, to, progress))
    }
}

pub fn record_layout_bounds(
    world: &mut World,
    entity: Entity,
    time: f64,
    duration: f64,
    bounds: gaanim_math::Bounds3D,
) {
    if let Some(mut track) = world.get_mut::<LayoutBoundsTrack>(entity) {
        let from = track.at(time).unwrap_or(bounds);
        track.0.push((time, duration, from, bounds));
    } else {
        world
            .entity_mut(entity)
            .insert(LayoutBoundsTrack(vec![(time, 0.0, bounds, bounds)]));
    }
}

pub fn connector_path(
    points: &[DVec3],
    head_length: f64,
    head_width: f64,
    body_width: f64,
    ratio: Option<f64>,
    progress: f64,
) -> BezPath {
    use gaanim_core::kurbo::{Point, Stroke, StrokeOpts, stroke};
    let mut points = points.to_vec();
    if points.iter().any(|p| !p.is_finite()) || progress.is_nan() {
        return BezPath::new();
    }
    points.dedup_by(|a, b| (a.truncate() - b.truncate()).length_squared() < 1e-20);
    if points.len() < 2 {
        return BezPath::new();
    }
    if progress < 1.0 {
        return partial_connector_path(
            &points,
            head_length,
            head_width,
            body_width,
            ratio,
            progress.max(0.0),
        );
    }
    let tip = points[points.len() - 1];
    let delta = (tip - points[points.len() - 2]).truncate();
    let length = delta.length();
    let direction = delta / length;
    let head = head_length.min(length * ratio.unwrap_or(1.0));
    let half_head = head_width * (head / head_length) * 0.5;
    let shoulder = tip.truncate() - direction * head;
    let normal = gaanim_core::glam::DVec2::new(-direction.y, direction.x);
    let mut centerline = BezPath::new();
    centerline.move_to((points[0].x, points[0].y));
    for p in &points[1..points.len() - 1] {
        centerline.line_to((p.x, p.y));
    }
    centerline.line_to((shoulder.x, shoulder.y));
    let mut style = Stroke::new(body_width);
    style.start_cap = gaanim_core::kurbo::Cap::Butt;
    style.end_cap = gaanim_core::kurbo::Cap::Butt;
    style.join = gaanim_core::kurbo::Join::Miter;
    let mut path = stroke(centerline.iter(), &style, &StrokeOpts::default(), 0.001);
    let left = shoulder + normal * half_head;
    let right = shoulder - normal * half_head;
    path.move_to(Point::new(left.x, left.y));
    path.line_to((tip.x, tip.y));
    path.line_to((right.x, right.y));
    path.close_path();
    path
}

/// A connector grown to `progress` of its polyline length from the tail.
///
/// The head keeps the size it has on the complete connector and spans the
/// last `head` units of the visible spine, so the tip travels along the path
/// (turning smoothly through waypoints) instead of the arrow being scaled.
/// Until the visible length reaches the head size, only a proportionally
/// smaller head is drawn.
fn partial_connector_path(
    points: &[DVec3],
    head_length: f64,
    head_width: f64,
    body_width: f64,
    ratio: Option<f64>,
    progress: f64,
) -> BezPath {
    use gaanim_core::glam::DVec2;
    use gaanim_core::kurbo::{Point, Stroke, StrokeOpts, stroke};
    let points: Vec<DVec2> = points.iter().map(|p| p.truncate()).collect();
    let mut cumulative = vec![0.0];
    for pair in points.windows(2) {
        cumulative.push(cumulative[cumulative.len() - 1] + (pair[1] - pair[0]).length());
    }
    let total = cumulative[cumulative.len() - 1];
    let visible = total * progress.min(1.0);
    if visible <= 1e-9 {
        return BezPath::new();
    }
    let last_segment = total - cumulative[cumulative.len() - 2];
    let full_head = head_length.min(last_segment * ratio.unwrap_or(1.0));
    let head = full_head.min(visible);
    let point_at = |distance: f64| -> DVec2 {
        let index = cumulative
            .windows(2)
            .position(|w| distance <= w[1])
            .unwrap_or(points.len() - 2);
        let span = cumulative[index + 1] - cumulative[index];
        let t = if span > 0.0 {
            ((distance - cumulative[index]) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        points[index].lerp(points[index + 1], t)
    };
    let tip = point_at(visible);
    let shoulder_distance = visible - head;
    let shoulder = point_at(shoulder_distance);
    let chord = tip - shoulder;
    let direction = if chord.length_squared() > 1e-20 {
        chord.normalize()
    } else {
        (points[1] - points[0]).normalize_or_zero()
    };
    let normal = DVec2::new(-direction.y, direction.x);
    let half_head = head_width * (head / head_length) * 0.5;

    let mut path = BezPath::new();
    if shoulder_distance > 1e-9 {
        let mut centerline = BezPath::new();
        centerline.move_to((points[0].x, points[0].y));
        for (point, distance) in points.iter().zip(&cumulative).skip(1) {
            if *distance >= shoulder_distance {
                break;
            }
            centerline.line_to((point.x, point.y));
        }
        centerline.line_to((shoulder.x, shoulder.y));
        let mut style = Stroke::new(body_width);
        style.start_cap = gaanim_core::kurbo::Cap::Butt;
        style.end_cap = gaanim_core::kurbo::Cap::Butt;
        style.join = gaanim_core::kurbo::Join::Miter;
        path = stroke(centerline.iter(), &style, &StrokeOpts::default(), 0.001);
    }
    let left = shoulder + normal * half_head;
    let right = shoulder - normal * half_head;
    path.move_to(Point::new(left.x, left.y));
    path.line_to((tip.x, tip.y));
    path.line_to((right.x, right.y));
    path.close_path();
    path
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
    /// What is drawn inside the padded bounds.
    #[cfg_attr(feature = "serde", serde(default))]
    pub shape: SurroundingShape,
    /// A hand-drawn notation drawn instead of `shape`.
    #[cfg_attr(feature = "serde", serde(default))]
    pub notation: Option<gaanim_math::RoughNotation>,
    /// Radial lines drawn instead of `shape`.
    #[cfg_attr(feature = "serde", serde(default))]
    pub rays: Option<SurroundingRays>,
}

/// Radial lines around the targets of a [`SurroundingRect`], as a flash
/// draws them: `count` lines `length` long, evenly spaced from the right and
/// starting on the circle that holds the padded bounds' larger side.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SurroundingRays {
    pub count: u32,
    pub length: f64,
}

impl SurroundingRays {
    /// The lines around `bounds`, one sub-path each.
    pub fn path(self, bounds: gaanim_math::Bounds3D) -> BezPath {
        let center = bounds.center();
        let inner = bounds.width().abs().max(bounds.height().abs()) * 0.5;
        let mut path = BezPath::new();
        for index in 0..self.count {
            let angle = std::f64::consts::TAU * f64::from(index) / f64::from(self.count);
            let (sin, cos) = angle.sin_cos();
            let at = |radius: f64| (center.x + radius * cos, center.y + radius * sin);
            path.move_to(at(inner));
            path.line_to(at(inner + self.length));
        }
        path
    }
}

/// What a [`SurroundingRect`] draws inside its padded bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SurroundingShape {
    /// The frame: a rectangle, rounded by `corner_radius`.
    #[default]
    Frame,
    /// A single line along the bottom edge, as an underline.
    Underline,
    /// One frame per rendered line of the targets (text glyphs), padded and
    /// rounded like `Frame`, in the frame of their text: it turns with the
    /// text. It follows `to`; retargets do not interpolate it.
    Lines,
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
            shape: SurroundingShape::Frame,
            notation: None,
            rays: None,
        }
    }

    /// `rays` around the targets, starting `gap` away from their bounds.
    pub fn rays(targets: Vec<gaanim_core::ObjectId>, gap: f64, rays: SurroundingRays) -> Self {
        Self {
            rays: Some(rays),
            ..Self::new(targets, [gap; 4], 0.0)
        }
    }

    /// A hand-drawn `notation` around the targets' bounds grown by `padding`.
    pub fn rough(
        targets: Vec<gaanim_core::ObjectId>,
        padding: [f64; 4],
        notation: gaanim_math::RoughNotation,
    ) -> Self {
        Self {
            notation: Some(notation),
            ..Self::new(targets, padding, 0.0)
        }
    }

    /// One frame around each rendered line of the glyphs `targets`, grown by
    /// `padding` and rounded by `corner_radius` (see
    /// [`SurroundingShape::Lines`]).
    pub fn lines(
        targets: Vec<gaanim_core::ObjectId>,
        padding: [f64; 4],
        corner_radius: f64,
    ) -> Self {
        Self {
            shape: SurroundingShape::Lines,
            ..Self::new(targets, padding, corner_radius)
        }
    }

    /// A line under the targets, `gap` scene units below them and as wide as
    /// they are plus `overhang` on each side.
    pub fn underline(targets: Vec<gaanim_core::ObjectId>, gap: f64, overhang: f64) -> Self {
        Self {
            shape: SurroundingShape::Underline,
            ..Self::new(targets, [0.0, overhang, gap, overhang], 0.0)
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

/// A rectangle rounded by `corner_radius`, capped at half its smaller side.
fn frame_path(rect: gaanim_core::kurbo::Rect, corner_radius: f64) -> BezPath {
    let radius = corner_radius
        .min(rect.width().abs() * 0.5)
        .min(rect.height().abs() * 0.5)
        .max(0.0);
    if radius > 0.0 {
        gaanim_core::kurbo::RoundedRect::from_rect(rect, radius).to_path(0.1)
    } else {
        rect.to_path(0.1)
    }
}

/// One frame per rendered line of the glyphs `ids`, grown by `padding` and
/// rounded by `corner_radius`, in world coordinates. The glyphs are grouped
/// into lines with their text's world rotation undone, and the frames turn
/// back with it.
fn line_frames(
    ids: &[gaanim_core::ObjectId],
    entities: &std::collections::HashMap<gaanim_core::ObjectId, Entity>,
    world: &World,
    padding: [f64; 4],
    corner_radius: f64,
) -> BezPath {
    let glyphs: Vec<(Entity, gaanim_math::Bounds3D, DMat4)> = ids
        .iter()
        .filter_map(|id| entities.get(id))
        .filter_map(|&entity| {
            let local = world.get::<LocalBounds>(entity)?.0;
            Some((entity, local, entity_world_matrix(entity, world)?))
        })
        .collect();
    let Some(&(first, _, first_matrix)) = glyphs.first() else {
        return BezPath::new();
    };
    // The lines run along the text's x axis: the first glyph's parent's.
    let basis = world
        .get::<ChildOf>(first)
        .and_then(|parent| entity_world_matrix(parent.parent(), world))
        .unwrap_or(first_matrix);
    let angle = basis.x_axis.y.atan2(basis.x_axis.x);
    let unturn = DMat4::from_rotation_z(-angle);
    let upright: Vec<gaanim_math::text_lines::LineGlyph> = glyphs
        .iter()
        .map(|(_, local, matrix)| {
            let bounds = local.transform_mat4(&(unturn * *matrix));
            gaanim_math::text_lines::LineGlyph {
                min: bounds.min.truncate(),
                max: bounds.max.truncate(),
                selected: true,
            }
        })
        .collect();
    let turn = gaanim_core::kurbo::Affine::rotate(angle);
    let [top, right, bottom, left] = padding;
    let mut path = BezPath::new();
    for line in gaanim_math::text_lines::group_lines(&upright) {
        let Some((low, high)) = line.selected_x else {
            continue;
        };
        let rect = gaanim_core::kurbo::Rect::new(
            low - left,
            line.min_y - bottom,
            high + right,
            line.max_y + top,
        );
        path.extend(turn * frame_path(rect, corner_radius));
    }
    path
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
        if frame.shape == SurroundingShape::Lines {
            let path = line_frames(
                &frame.to,
                &entities,
                world,
                frame.padding,
                frame.corner_radius,
            );
            let bounds = (!path.elements().is_empty()).then(|| {
                let rect = path.bounding_box();
                gaanim_math::Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1)
            });
            write_path(world, entity, path);
            if let Some(bounds) = bounds {
                if let Some(mut local) = world.get_mut::<LocalBounds>(entity) {
                    local.0 = bounds;
                }
                if let Some(mut live) = world.get_mut::<SurroundingRect>(entity) {
                    live.last_bounds = Some(bounds);
                }
            }
            continue;
        }
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

        if let Some(rays) = frame.rays {
            write_path(world, entity, rays.path(bounds));
            if let Some(mut live) = world.get_mut::<SurroundingRect>(entity) {
                live.last_bounds = Some(bounds);
            }
            continue;
        }
        if let Some(notation) = frame.notation {
            let path = notation.path(gaanim_core::kurbo::Rect::new(
                bounds.min.x,
                bounds.min.y,
                bounds.max.x,
                bounds.max.y,
            ));
            write_path(world, entity, path);
            if let Some(mut live) = world.get_mut::<SurroundingRect>(entity) {
                live.last_bounds = Some(bounds);
            }
            continue;
        }
        if frame.shape == SurroundingShape::Underline {
            let mut line = BezPath::new();
            line.move_to((bounds.min.x, bounds.min.y));
            line.line_to((bounds.max.x, bounds.min.y));
            write_path(world, entity, line);
            let bounds = gaanim_math::Bounds3D::new_2d(
                bounds.min.x,
                bounds.min.y,
                bounds.max.x,
                bounds.min.y,
            );
            if let Some(mut local) = world.get_mut::<LocalBounds>(entity) {
                local.0 = bounds;
            }
            if let Some(mut live) = world.get_mut::<SurroundingRect>(entity) {
                live.last_bounds = Some(bounds);
            }
            continue;
        }
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
        Self {
            from,
            to,
            dashes: None,
        }
    }

    /// The same line, dashed: `dash` long and `gap` apart.
    pub fn dashed(mut self, dash: f64, gap: f64) -> Self {
        self.dashes = Some((dash, gap));
        self
    }
}

/// Give every layout box its box at `time`, and fit each box's background
/// to it. Measuring a scene runs this too, since a box's background is a
/// placeholder until then.
pub fn resolve_layout_boxes(world: &mut World, time: f64) {
    let mut boxes = world.query::<(&LayoutBoundsTrack, &mut LocalBounds)>();
    for (track, mut bounds) in boxes.iter_mut(world) {
        if let Some(value) = track.at(time) {
            bounds.set_if_neq(LocalBounds(value));
        }
    }
    let mut updates = Vec::new();
    let mut backgrounds = world.query::<(Entity, &LayoutBackground)>();
    for (entity, background) in backgrounds.iter(world) {
        if let Some(bounds) = world.get::<LocalBounds>(background.container) {
            let b = bounds.0;
            let radius = background
                .radius
                .min(b.width().max(0.0) * 0.5)
                .min(b.height().max(0.0) * 0.5);
            updates.push((
                entity,
                gaanim_core::kurbo::RoundedRect::new(b.min.x, b.min.y, b.max.x, b.max.y, radius)
                    .to_path(0.001),
            ));
        }
    }
    for (entity, path) in updates {
        write_path(world, entity, path);
    }
}

/// Sistema exclusivo que resuelve los endpoints de cada TrackingLine y regenera su Path2D.
pub fn tracking_line_system(world: &mut World) {
    let time = world
        .get_resource::<PlaybackState>()
        .map_or(0.0, |state| state.current_time);
    resolve_layout_boxes(world, time);
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
            let (from, to) = (
                gaanim_core::kurbo::Point::new(from.x, from.y),
                gaanim_core::kurbo::Point::new(to.x, to.y),
            );
            let path = match line.dashes {
                Some((dash, gap)) => gaanim_objects::primitives::dash_path(from, to, dash, gap),
                None => {
                    let mut path = BezPath::new();
                    path.move_to(from);
                    path.line_to(to);
                    path
                }
            };
            updates.push((entity, path));
        }
    }

    let mut connectors = world.query::<(Entity, &TrackingConnector)>();
    for (entity, connector) in connectors.iter(world) {
        let points: Option<Vec<_>> = connector
            .points
            .iter()
            .map(|p| resolve_tracking_endpoint(p, world))
            .collect();
        let mut path = points
            .map(|points| {
                connector_path(
                    &points,
                    connector.head_length,
                    connector.head_width,
                    connector.body_width,
                    connector.max_head_ratio,
                    connector.progress,
                )
            })
            .unwrap_or_default();
        if connector.local {
            updates.push((entity, path));
            continue;
        }
        let inverse = entity_world_matrix(entity, world)
            .unwrap_or(DMat4::IDENTITY)
            .inverse();
        if inverse.is_finite() {
            path.apply_affine(gaanim_core::kurbo::Affine::new([
                inverse.x_axis.x,
                inverse.x_axis.y,
                inverse.y_axis.x,
                inverse.y_axis.y,
                inverse.w_axis.x,
                inverse.w_axis.y,
            ]));
        } else {
            path = BezPath::new();
        }
        updates.push((entity, path));
    }
    for (entity, path) in updates {
        write_path(world, entity, path);
    }

    let mut lines_3d = world.query::<(Entity, &TrackingLine3D)>();
    let mut placements = Vec::new();
    for (entity, line) in lines_3d.iter(world) {
        if let (Some(from), Some(to)) = (
            resolve_tracking_endpoint(&line.from, world),
            resolve_tracking_endpoint(&line.to, world),
        ) {
            let parent = world
                .get::<ChildOf>(entity)
                .and_then(|relation| entity_world_matrix(relation.parent(), world))
                .unwrap_or(DMat4::IDENTITY);
            placements.push((entity, line_3d_transform(parent, from, to, line.progress)));
        }
    }
    for (entity, placement) in placements {
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
            transform.set_if_neq(placement);
        }
    }
}

/// A drawable a layout replaced by `copy` from timeline second `since` on:
/// a text it rewrapped at another width. What follows the drawable follows
/// it until then and the copy from then on.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ReplacedBy {
    pub copy: Entity,
    pub since: f64,
}

/// What stands for `entity` at timeline second `time`: the copy that
/// replaced it by then (see [`ReplacedBy`]), however many times it was
/// replaced, or `entity` itself.
pub fn stand_in_at(entity: Entity, time: f64, world: &World) -> Entity {
    let mut current = entity;
    // A copy is spawned after what it replaces, so the chain has no cycle;
    // the bound only guards a malformed one.
    for _ in 0..64 {
        match world.get::<ReplacedBy>(current) {
            Some(replaced) if time + 1e-9 >= replaced.since => current = replaced.copy,
            _ => break,
        }
    }
    current
}

/// [`stand_in_at`] at the playhead.
pub fn stand_in(entity: Entity, world: &World) -> Entity {
    let time = world
        .get_resource::<PlaybackState>()
        .map_or(0.0, |state| state.current_time);
    stand_in_at(entity, time, world)
}

/// Where the circle of `radius` around the origin of the frame `matrix`
/// meets the line from there toward the world point `toward`: an ellipse
/// when the frame stretches. The origin when `toward` is there or the frame
/// is degenerate.
pub fn rim(matrix: DMat4, toward: DVec3, radius: f64) -> DVec3 {
    let center = matrix.transform_point3(DVec3::ZERO);
    let inverse = matrix.inverse();
    if !inverse.is_finite() {
        return center;
    }
    let local = inverse.transform_point3(toward);
    let direction = DVec3::new(local.x, local.y, 0.0);
    let length = direction.length();
    if length > 1e-12 {
        matrix.transform_point3(direction * (radius / length))
    } else {
        center
    }
}

/// Resolve the current world-space position of a reactive endpoint.
pub fn resolve_tracking_endpoint(ep: &TrackingEndpoint, world: &World) -> Option<DVec3> {
    match ep {
        TrackingEndpoint::Static(pos) => Some(*pos),
        TrackingEndpoint::Entity(entity) => {
            let entity = stand_in(*entity, world);
            entity_world_matrix(entity, world).map(|matrix| matrix.transform_point3(DVec3::ZERO))
        }
        TrackingEndpoint::EntityAnchor {
            entity,
            normalized,
            offset,
        } => {
            let entity = stand_in(*entity, world);
            let bounds = world.get::<LocalBounds>(entity)?.0;
            let center = bounds.center();
            let half = bounds.size() * 0.5;
            let local = center + half * *normalized + *offset;
            entity_world_matrix(entity, world).map(|matrix| matrix.transform_point3(local))
        }
        TrackingEndpoint::Expression { x, y, z } => Some(DVec3::new(
            x.evaluate(world)?,
            y.evaluate(world)?,
            z.evaluate(world)?,
        )),
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
            Some(from.lerp(to, alpha.evaluate(world)?) + *offset)
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
        TrackingEndpoint::Rim {
            entity,
            toward,
            radius,
        } => {
            let matrix = entity_world_matrix(stand_in(*entity, world), world)?;
            let toward = resolve_tracking_endpoint(toward, world)?;
            Some(rim(matrix, toward, *radius))
        }
        TrackingEndpoint::MediaPixel { entity, x, y } => media_pixel_point(*entity, *x, *y, world),
        TrackingEndpoint::Screen(point) => {
            let point = resolve_tracking_endpoint(point, world)?;
            let camera = world
                .get_resource::<gaanim_math::ResolvedCamera>()
                .map(|resolved| resolved.camera)
                .or_else(|| world.get_resource::<gaanim_math::Camera>().copied())?;
            Some(camera.hud_point(point).extend(0.0))
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
            entity_world_matrix(stand_in(*entity, world), world).unwrap_or(DMat4::IDENTITY)
        }
        TrackingEndpoint::MediaPixel { entity, .. } => {
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
    let trim = world.get::<crate::writing::PathTrimWindow>(entity);
    // Pieces of one stroke, such as the passes of a hand-drawn notation,
    // are drawn one after another as the timeline's own reveal does.
    let visible = match world.get::<gaanim_scene::PathRevealOrder>(entity) {
        Some(order @ gaanim_scene::PathRevealOrder::Sequential)
            if trim.is_none() && reveal < 1.0 - 1e-9 =>
        {
            Arc::new(order.trim(&path, reveal))
        }
        _ => crate::writing::visible_path(&path, reveal, trim),
    };
    // These systems rewrite every tracked path each frame; writing only what
    // differs keeps unchanged paths out of the renderer's re-encoding and the
    // timeline's restore set.
    if let Some(mut path_comp) = world.get_mut::<Path2D>(entity) {
        path_comp.set_if_neq(Path2D(visible));
    }
    if let Some(mut source) = world.get_mut::<PathSource>(entity) {
        source.set_if_neq(PathSource(path.clone()));
    }
    if let Some(mut bounds) = world.get_mut::<LocalBounds>(entity) {
        let local = if path.elements().is_empty() {
            gaanim_math::Bounds3D::new_2d(0.0, 0.0, 0.0, 0.0)
        } else {
            // The path's own box, as every other shape has: frames, anchors
            // and measurements read it.
            let rect = gaanim_core::kurbo::Shape::bounding_box(path.as_ref());
            gaanim_math::Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1)
        };
        bounds.set_if_neq(LocalBounds(local));
    }
}

/// Convert a world-space point into the current local space of an entity.
pub fn tracking_world_to_local(entity: Entity, point: DVec3, world: &World) -> DVec3 {
    entity_world_matrix(entity, world)
        .filter(|matrix| matrix.determinant().abs() > f64::EPSILON)
        .map(|matrix| matrix.inverse().transform_point3(point))
        .unwrap_or(point)
}

/// World matrix of `entity` composed from its `SpatialTransform` chain,
/// independent of transform propagation order.
pub fn entity_world_matrix(entity: Entity, world: &World) -> Option<DMat4> {
    let mut chain = Vec::new();
    let mut hud = false;
    let mut parallax = None;
    let mut current = entity;
    for _ in 0..256 {
        chain.push(world.get::<SpatialTransform>(current)?.to_mat4());
        hud |= world.get::<gaanim_scene::HudOverlay>(current).is_some();
        // The outermost layer wins, as in `pin_parallax_layers_system`.
        if let Some(layer) = world.get::<gaanim_scene::ParallaxLayer>(current) {
            parallax = Some(layer.depth);
        }
        let Some(parent) = world
            .get::<ChildOf>(current)
            .map(|relation| relation.parent())
        else {
            let pin_matrix = |pin: gaanim_core::kurbo::Affine| {
                gaanim_math::GlobalSpatialTransform::from_local(&SpatialTransform::from_affine_2d(
                    &pin,
                ))
                .mat4
            };
            let mut matrix = DMat4::IDENTITY;
            // HUD overlays sit where the camera pins them on the output frame.
            if hud && let Some(pin) = gaanim_scene::world_hud_pin(world) {
                matrix = pin_matrix(pin);
            }
            // A HUD stays on screen even inside a parallax layer.
            if !hud
                && let Some(pin) =
                    parallax.and_then(|depth| gaanim_scene::world_parallax_pin(world, depth))
            {
                matrix *= pin_matrix(pin);
            }
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
        let offset = DimensionSide::resolve(
            placement.side,
            placement.offset,
            from.truncate(),
            to.truncate(),
        );
        let side = if offset < 0.0 { -1.0 } else { 1.0 };
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
        // Dimension labels keep `gap` between the line and their nearest
        // edge: their box center moves out by the half extent across the
        // line, which for an upright label on a steep line is its half width.
        // Other placements (force labels) measure `gap` to the label origin.
        let (extent, center) = if placement.clear_label_width {
            label_box(placement.label, world)
                .map(|(half, center)| {
                    let extent = match placement.orientation {
                        DimensionLabelOrientation::Upright => {
                            normal.x.abs() * half.x + normal.y.abs() * half.y
                        }
                        DimensionLabelOrientation::Aligned => half.y,
                    };
                    (extent, center)
                })
                .unwrap_or((0.0, DVec2::ZERO))
        } else {
            (0.0, DVec2::ZERO)
        };
        let displacement = offset + side * (placement.gap + extent);
        let target =
            (from + to) * 0.5 + DVec3::new(normal.x * displacement, normal.y * displacement, 0.0);
        let center = DQuat::from_rotation_z(angle) * center.extend(0.0);
        updates.push((placement.label, target - center, angle));
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

/// Half size and center of `label`'s box in its own frame: rotated with it
/// and placed at its origin, at world scale.
fn label_box(label: Entity, world: &World) -> Option<(DVec2, DVec2)> {
    let (_, rotation, translation) =
        entity_world_matrix(label, world)?.to_scale_rotation_translation();
    let frame = DMat4::from_rotation_translation(rotation, translation).inverse();
    let bounds = bounds_in_frame(label, &frame, world)?;
    Some((bounds.size().truncate() * 0.5, bounds.center().truncate()))
}

fn bounds_in_frame(entity: Entity, frame: &DMat4, world: &World) -> Option<gaanim_math::Bounds3D> {
    let own = world.get::<LocalBounds>(entity).and_then(|local| {
        entity_world_matrix(entity, world).map(|matrix| local.0.transform_mat4(&(*frame * matrix)))
    });
    let descendants = world
        .get::<Children>(entity)
        .into_iter()
        .flat_map(|children| children.iter())
        .filter_map(|child| bounds_in_frame(*child, frame, world));
    own.into_iter()
        .chain(descendants)
        .reduce(|left, right| left.union(&right))
}

/// Follow arbitrary endpoints after authored transforms and custom updaters.
pub fn endpoint_follow_system(world: &mut World) {
    follow_endpoints(world, |_| true);
}

/// System: drawables that follow a point on the screen
/// ([`TrackingEndpoint::Screen`]) move where this frame's camera draws it,
/// once the camera is resolved: [`endpoint_follow_system`] runs before it
/// and reads the camera of the frame before.
pub fn screen_follow_system(world: &mut World) {
    follow_endpoints(world, TrackingEndpoint::reads_the_screen);
}

/// Place each [`EndpointFollow`] whose endpoint `wanted` accepts.
fn follow_endpoints(world: &mut World, wanted: impl Fn(&TrackingEndpoint) -> bool) {
    let current_time = world
        .get_resource::<PlaybackState>()
        .map_or(0.0, |state| state.current_time);
    let mut updates = Vec::new();
    let mut query = world.query::<(Entity, &EndpointFollow)>();
    for (entity, follow) in query.iter(world) {
        if !follow.holds_at(current_time) || !wanted(&follow.endpoint) {
            continue;
        }
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
                    let base_world = tip_world + DVec3::new(inward.x * 0.11, inward.y * 0.11, 0.0);
                    let left =
                        local(base_world + DVec3::new(normal.x * 0.055, normal.y * 0.055, 0.0));
                    let right =
                        local(base_world - DVec3::new(normal.x * 0.055, normal.y * 0.055, 0.0));
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

/// The turn of `entity` about Z in the scene: its own and its parents'.
///
/// Each one counts the angle a `rotate_to(Parameter)` sets it to, whole turns
/// included, else its rotation (up to two turns either way), plus the turns
/// procedural layers such as a spinning `Updater.rotate` add, which only the
/// propagated world state carries.
fn turned_angle(
    entity: Entity,
    world: &World,
    time: f64,
    bound: &HashMap<Entity, f64>,
) -> Option<f64> {
    world.get::<SpatialTransform>(entity)?;
    let live = |signal: Entity| {
        world
            .get::<crate::signals::FloatSignal>(signal)
            .map(|signal| signal.value)
    };
    let mut angle = 0.0;
    let mut current = entity;
    for _ in 0..256 {
        let Some(transform) = world.get::<SpatialTransform>(current) else {
            break;
        };
        angle += bound.get(&current).copied().unwrap_or_else(|| {
            let rotation = transform.rotation;
            2.0 * rotation.z.atan2(rotation.w)
        });
        if let Some(motion) = world.get::<crate::procedural::ProceduralMotion>(current) {
            angle += motion.offset_with(time, &live).rotation;
        }
        let Some(parent) = world
            .get::<ChildOf>(current)
            .map(|relation| relation.parent())
        else {
            break;
        };
        current = parent;
    }
    Some(angle)
}

/// The angle each `rotate_to(Parameter)` sets its target to at `time`.
fn bound_rotations(world: &mut World, time: f64) -> HashMap<Entity, f64> {
    let mut query = world.query::<&crate::PropertyBinding>();
    query
        .iter(world)
        .filter(|binding| time >= binding.start && binding.end.is_none_or(|end| time < end))
        .filter_map(|binding| {
            binding
                .source
                .rotation_z(world, time)
                .map(|angle| (binding.target, angle))
        })
        .collect()
}

/// Bound targets whose rotation `entity`'s world angle reads: the entity itself
/// and its ancestors.
fn bound_in_chain(mut entity: Entity, bound: &HashSet<Entity>, world: &World) -> Vec<Entity> {
    let mut found = Vec::new();
    let mut visited = HashSet::new();
    while visited.insert(entity) {
        if bound.contains(&entity) {
            found.push(entity);
        }
        let Some(parent) = world
            .get::<ChildOf>(entity)
            .map(|relation| relation.parent())
        else {
            break;
        };
        entity = parent;
    }
    found
}

/// Apply rotation and rotation-to-translation bindings in the connector phase.
pub fn mechanism_binding_system(world: &mut World) {
    let time = world
        .get_resource::<PlaybackState>()
        .map_or(0.0, |state| state.current_time);
    let mut turns = bound_rotations(world, time);
    let mut rotation_query = world.query::<(Entity, &RotationBinding)>();
    let mut pending: Vec<(Entity, RotationBinding)> = rotation_query
        .iter(world)
        .map(|(entity, binding)| (entity, binding.clone()))
        .collect();
    // Write each rotation before the bindings that read it (a gear train
    // `c.bind_rotation_from(b)`, `b.bind_rotation_from(a)`, or a wheel on a
    // bound carrier), so a whole chain is current in one pass instead of
    // lagging one frame per link. A cycle is written in declaration order.
    let bound: HashSet<Entity> = pending.iter().map(|(entity, _)| *entity).collect();
    let mut done = HashSet::new();
    while !pending.is_empty() {
        let ready = pending.iter().position(|(entity, binding)| {
            let parent = world
                .get::<ChildOf>(*entity)
                .map(|relation| relation.parent());
            bound_in_chain(binding.source, &bound, world)
                .into_iter()
                .chain(parent.map_or_else(Vec::new, |parent| bound_in_chain(parent, &bound, world)))
                .all(|dependency| dependency == *entity || done.contains(&dependency))
        });
        let (entity, binding) = pending.remove(ready.unwrap_or(0));
        done.insert(entity);
        let Some(source_angle) = turned_angle(binding.source, world, time, &turns) else {
            continue;
        };
        let world_angle = source_angle * binding.ratio + binding.phase;
        let parent_angle = world
            .get::<ChildOf>(entity)
            .map(|relation| relation.parent())
            .and_then(|parent| turned_angle(parent, world, time, &turns))
            .unwrap_or(0.0);
        let local = world_angle - parent_angle;
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
            transform.rotation = DQuat::from_rotation_z(local);
        }
        // A bound rotation drives the next link with its whole turns.
        turns.insert(entity, local);
    }

    let mut translations = Vec::new();
    let mut initialized = Vec::new();
    let mut query = world.query::<(Entity, &RotationTranslationBinding)>();
    for (entity, binding) in query.iter(world) {
        let Some(angle) = turned_angle(binding.source, world, time, &turns) else {
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

    let mut updates: Vec<(
        Entity,
        SampledProperty,
        f64,
        Option<gaanim_core::peniko::Color>,
    )> = Vec::new();
    let mut new_bases: Vec<(Entity, usize, f64)> = Vec::new();
    let mut orientations: Vec<(Entity, DQuat)> = Vec::new();
    {
        let mut query = world.query::<(Entity, &SampledSeriesDrivers)>();
        for (entity, drivers) in query.iter(world) {
            for (channel, driver) in drivers.0.iter().enumerate() {
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
                        new_bases.push((entity, channel, value));
                        value
                    })
                } else {
                    0.0
                };
                if !drivers
                    .active(driver.property, current_time)
                    .is_some_and(|active| std::ptr::eq(active, driver))
                {
                    continue;
                }

                let effective_time = match driver.stop_at {
                    Some(stop_at) => current_time.min(stop_at),
                    None => current_time,
                };
                let elapsed = (effective_time - driver.start_at).max(0.0);
                if let Some(orientation) = driver.sample_orientation(elapsed) {
                    orientations.push((entity, orientation));
                    continue;
                }
                let sampled = driver.sample(elapsed);
                let output = if relative {
                    base + driver.offset + driver.scale * sampled
                } else {
                    driver.offset + driver.scale * sampled
                };
                let color = driver.colors.as_ref().map(|colors| colors.at(output));
                updates.push((entity, driver.property, output, color));
            }
        }
    }

    for (entity, channel, base) in new_bases {
        if let Some(mut drivers) = world.get_mut::<SampledSeriesDrivers>(entity)
            && let Some(driver) = drivers.0.get_mut(channel)
        {
            driver.base = Some(base);
        }
    }

    for (entity, property, output, color) in updates {
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
            SampledProperty::Fill => {
                if let Some(color) = color {
                    set_sampled_fill(world, entity, color);
                }
            }
            SampledProperty::Orientation => {}
        }
    }

    for (entity, orientation) in orientations {
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity)
            && transform.rotation != orientation
        {
            transform.rotation = orientation;
        }
    }
}

/// Pinta `color` en la entidad y en sus descendientes que tienen relleno, de
/// modo que un grupo se colorea entero.
fn set_sampled_fill(world: &mut World, entity: Entity, color: gaanim_core::peniko::Color) {
    let brush = gaanim_core::peniko::Brush::Solid(color);
    if let Some(mut fill) = world.get_mut::<gaanim_scene::FillBrush>(entity)
        && fill.0.as_ref().is_some_and(|current| *current != brush)
    {
        fill.0 = Some(brush);
    }
    let children: Vec<Entity> = world
        .get::<Children>(entity)
        .map(|children| children.to_vec())
        .unwrap_or_default();
    for child in children {
        set_sampled_fill(world, child, color);
    }
}

#[cfg(test)]
mod tests;
