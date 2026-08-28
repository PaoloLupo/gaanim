"""Lorenz attractor 3D — replica GLMakie con APIs genéricas nuevas.

Makie original:
    attractor = Lorenz(dt=0.01, σ=10, ρ=28, β=8/3)
    lines(points, color=colors, colormap=:inferno, transparency=true,
          axis=(type=Axis3, limits=(-30,30,-30,30,0,50)))
    record 1:120 (50 pasos/frame) + azimuth = 1.7π + 0.3*sin(2π*frame/120)

Gaanim ahora (genérico, no Lorenz-específico):
    - scene.geometry.polyline_3d(points, colormap="inferno") → per-vertex gradient en un draw call
    - dot.add_updater_fn(callback) → callback(pos, dt, elapsed) -> (x,y,z) genérico
    - scene.geometry.traced_path_3d(dot, colormap="inferno", max_points=6000) → trail reactivo 3D

Ejecuta:
    .venv/Scripts/Activate.ps1; cargo run -p gaanim_launcher -- examples/lorenz_3d_demo.py
"""

import math
import os
from gaanim import Axis, Scene, Color, WHITE, GOLD, CYAN

# ---------------------------------------------------------------------------
# 1. Escena y ejes (límites Makie: -30..30, -30..30, 0..50)
# ---------------------------------------------------------------------------
scene = Scene(frame=(16, 9), background=Color(10, 10, 14))

axes = scene.viz.cartesian_3d(
    Axis.linear(-30, 30).ticks(10).label("x").style(color=WHITE),
    Axis.linear(-30, 30).ticks(10).label("y").style(color=WHITE),
    Axis.linear(0, 50).ticks(10).label("z").style(color=WHITE),
    size=(0.5, 0.5, 0.416667),
    grid=True,
).move_to_3d(0, 0, 25)

title = scene.text("Lorenz — GLMakie con APIs genéricas").fill(WHITE).hud().move_to(0, 4.166667)
subtitle = scene.text("polyline colormap + dot + updater genérico + traced_path_3d").fill(Color(180,180,190)).hud().move_to(0, 3.833333)
info = scene.text("azimuth oscila $1.7pi ± 0.3$ como Makie  •  colormap inferno por tiempo").fill(GOLD).hud().move_to(0, -4.166667)

# ---------------------------------------------------------------------------
# 2. Opción A: Estático con colormap (un solo draw call, per-vertex)
#    Equivalente a Makie `lines(points, colormap=:inferno)` precomputado.
# ---------------------------------------------------------------------------
# Precomputar 6000 puntos idéntico a Makie (Euler dt=0.01, 50 pasos ×120 frames)
class Lorenz:
    def __init__(self, dt=0.01, sigma=10.0, rho=28.0, beta=8.0/3.0, x=1.0, y=1.0, z=1.0):
        self.dt, self.sigma, self.rho, self.beta = dt, sigma, rho, beta
        self.x, self.y, self.z = x, y, z
    def step(self):
        dx = self.sigma*(self.y - self.x)
        dy = self.x*(self.rho - self.z) - self.y
        dz = self.x*self.y - self.beta*self.z
        self.x += self.dt*dx; self.y += self.dt*dy; self.z += self.dt*dz
        return (self.x, self.y, self.z)

lor = Lorenz()
points = []
for _ in range(120):
    for _ in range(50):
        points.append(lor.step())

# Ahora con la nueva API: un solo polyline con gradiente inferno por vértice
# (antes había que partir en 60 segmentos manuales)
# Fondo estático inicialmente oculto para que se vea la generación progresiva
static_trail = scene.geometry.polyline_3d(points, colormap="inferno")
static_trail.opacity(0.0)  # aparece al final como referencia tenue

# También funciona con lista explícita de colores:
# colors = [Color(255,0,0) if i%2==0 else Color(0,255,0) for i in range(len(points))]
# scene.geometry.polyline_3d(points, colors=colors)

# ---------------------------------------------------------------------------
# 3. Opción B: Dinámico reactivo (fiel a Makie `record` + `push!(points, step!)`)
#    Dot que integra Lorenz vía updater genérico + trail 3D que crece solo.
# ---------------------------------------------------------------------------
dot = scene.geometry.dot(0.058333).fill(WHITE).move_to_3d(1, 1, 1).billboard()

def reset_lorenz():
    # El estado está en la posición del dot, que Gaanim restaura automáticamente.
    pass


def lorenz_step(pos, dt, elapsed):
    """Un paso Lorenz; Gaanim controla su frecuencia independientemente del FPS."""
    x, y, z = pos
    sigma, rho, beta = 10.0, 28.0, 8.0/3.0
    integration_dt = 0.01
    dx = sigma*(y - x)
    dy = x*(rho - z) - y
    dz = x*y - beta*z
    x += integration_dt*dx
    y += integration_dt*dy
    z += integration_dt*dz
    return (x, y, z)

dot.add_updater_fn(lorenz_step, reset=reset_lorenz, fixed_dt=1.0/600.0)

# Trail reactivo 3D con colormap inferno por tiempo (como Makie `color=colors`)
# max_points ~6000, min_distance filtra puntos muy cercanos para performance
# Este es el que GENERA progresivamente el Lorenz a medida que el dot avanza
trail = scene.geometry.traced_path_3d(dot, colormap="inferno", max_points=6000, min_distance=0.35)

# Marcadores
origin_tag = scene.text("origen").fill(Color(120,200,120)).move_to_3d(0,0,2).billboard().scale_to(0.45)
head_glow = scene.geometry.dot(0.083333).fill(Color(255,255,220)).move_to_3d(1,1,1).billboard()
head_glow.bind_position_from(dot, "xyz")  # sigue al dot en XYZ (perspectiva)
head_glow.opacity(0.35)

# ---------------------------------------------------------------------------
# 4. Cámara perspectiva (makie viewmode=:fit, limits, azimuth)
# ---------------------------------------------------------------------------
scene.camera.perspective(fov_y=0.785, near=0.1, far=1000)
scene.play([scene.camera.animate.look_at(eye=(-55, -70, 60), target=(0, 0, 25)).duration(1.2)])

# ---------------------------------------------------------------------------
# 5. Timeline
# ---------------------------------------------------------------------------
scene.play([axes.animate.create().duration(1.0), title.animate.write().duration(0.8)])
scene.play([subtitle.animate.fade_in().duration(0.6)])

# El trail dinámico ya está creciendo vía updater+traced_path_3d;
# inicialmente solo se ve el trail reactivo creciendo; el estático aparece al final.
scene.play([
    dot.animate.fade_in().duration(0.4),
    trail.animate.fade_in().duration(0.4),
    head_glow.animate.opacity(0.35).duration(0.4),
    origin_tag.animate.fade_in().duration(0.5),
])
scene.wait(1.0)  # deja que el atractor se despliegue 1s (dot ya traza)

# Azimuth Makie: 1.7π ±0.3 sin(2π*frame/120) → lo aproximamos con órbitas
# Durante estas órbitas el dot sigue integrando Lorenz y el trail crece
scene.play([scene.camera.animate.orbit(delta_yaw=0.6, delta_pitch=0.08).duration(1.8)])
scene.play([scene.camera.animate.orbit(delta_yaw=-0.35, delta_pitch=-0.05).duration(1.4)])
scene.play([scene.camera.animate.orbit(delta_yaw=0.45, delta_pitch=0.06).duration(1.6)])
scene.play([scene.camera.animate.dolly(factor=0.88).duration(0.9)])
scene.wait(2.0)  # trail sigue creciendo mientras la cámara orbita
# Al final, mostramos el trazo estático completo como referencia tenue (antes estaba visible desde t=0 y tapaba la generación)
scene.play([static_trail.animate.opacity(0.22)])
scene.play([scene.camera.animate.dolly(factor=1.08).duration(0.9)])
scene.wait(2.0)

if os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [0.0, 1.2, 2.0, 4.0, 6.5])

scene.render()
