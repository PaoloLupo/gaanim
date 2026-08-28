"""Mini-lección visual sobre el caos en un péndulo doble."""

import math
import os
from dataclasses import dataclass, field
from pathlib import Path

from gaanim import BLACK, GOLD, RED, WHITE, Direction, Scene, Transition


ROOT = Path(__file__).resolve().parent
scene = Scene(frame=(16, 9), background=BLACK, margin=0.6)
scene.assets.load_project(str(ROOT / "gaanim.toml"))
scene.canvas.set_theme("paper")


# ---------------------------------------------------------------------------
# Motor físico determinista
# ---------------------------------------------------------------------------
@dataclass
class PendulumState:
    theta1: float
    omega1: float
    theta2: float
    omega2: float


@dataclass
class DoublePendulum:
    """A double pendulum with a replay-safe Runge–Kutta integrator."""

    px: float
    py: float
    L1: float = 250.0
    L2: float = 240.0
    theta1_0: float = 0.72
    theta2_0: float = -0.42
    g: float = 980.0
    damping: float = 0.006
    m1: float = 1.0
    m2: float = 1.0
    state: PendulumState = field(init=False)

    def __post_init__(self):
        self.state = PendulumState(self.theta1_0, 0.0, self.theta2_0, 0.0)

    def positions(self):
        theta1 = self.state.theta1
        theta2 = self.state.theta2
        x1 = self.px + self.L1 * math.sin(theta1)
        y1 = self.py - self.L1 * math.cos(theta1)
        x2 = x1 + self.L2 * math.sin(theta2)
        y2 = y1 - self.L2 * math.cos(theta2)
        return (x1, y1, 0.0), (x2, y2, 0.0)

    def derivatives(self, values):
        theta1, omega1, theta2, omega2 = values
        delta = theta1 - theta2
        denominator = 2.0 * self.m1 + self.m2 - self.m2 * math.cos(2.0 * delta)

        alpha1 = (
            -self.g * (2.0 * self.m1 + self.m2) * math.sin(theta1)
            - self.m2 * self.g * math.sin(theta1 - 2.0 * theta2)
            - 2.0 * math.sin(delta) * self.m2 * (
                omega2**2 * self.L2 + omega1**2 * self.L1 * math.cos(delta)
            )
        ) / (self.L1 * denominator)

        alpha2 = (
            2.0
            * math.sin(delta)
            * (
                omega1**2 * self.L1 * (self.m1 + self.m2)
                + self.g * (self.m1 + self.m2) * math.cos(theta1)
                + omega2**2 * self.L2 * self.m2 * math.cos(delta)
            )
        ) / (self.L2 * denominator)

        return (
            omega1,
            alpha1 - self.damping * omega1,
            omega2,
            alpha2 - self.damping * omega2,
        )

    def rk4_step(self, values, dt):
        k1 = self.derivatives(values)
        k2 = self.derivatives(
            tuple(value + 0.5 * dt * slope for value, slope in zip(values, k1))
        )
        k3 = self.derivatives(
            tuple(value + 0.5 * dt * slope for value, slope in zip(values, k2))
        )
        k4 = self.derivatives(
            tuple(value + dt * slope for value, slope in zip(values, k3))
        )
        return tuple(
            value + (dt / 6.0) * (slope1 + 2.0 * slope2 + 2.0 * slope3 + slope4)
            for value, slope1, slope2, slope3, slope4 in zip(values, k1, k2, k3, k4)
        )

    def reset(self):
        self.state = PendulumState(self.theta1_0, 0.0, self.theta2_0, 0.0)

    def advance(self, _position, dt, _elapsed):
        values = (
            self.state.theta1,
            self.state.omega1,
            self.state.theta2,
            self.state.omega2,
        )
        self.state = PendulumState(*self.rk4_step(values, dt))
        return self.positions()[0]

    def follow_second_mass(self, _position, _dt, _elapsed):
        return self.positions()[1]

    def label_position(self, index, dx, dy):
        x, y, _z = self.positions()[index]
        return (x + dx, y + dy, 0.0)


@dataclass
class PendulumVisual:
    support: object
    hinge: object
    bob1: object
    bob2: object
    rod1: object
    rod2: object
    length1: object
    length2: object
    angle1: object
    angle2: object
    trail: object
    label1: object
    label2: object


def make_visual(
    system,
    *,
    bob1_color=GOLD,
    bob2_color,
    trail_color,
    scale=1.0,
    label1_text="$m_1$",
    label2_text="$m_2$",
    label_offset=(-70.0, 0.0),
):
    """Create a color-coded diagram whose rods and labels follow the physics."""
    initial_pos1, initial_pos2 = system.positions()
    hinge = scene.geometry.dot(10 * scale).fill(WHITE).move_to(system.px, system.py)
    support = scene.mechanics.fixed_support(
        hinge,
        direction=Direction.DOWN,
        size=42 * scale,
        ground_length=68 * scale,
    )
    bob1 = (
        scene.geometry.circle(28 * scale)
        .fill(bob1_color)
        .stroke(WHITE, 4 * scale)
        .move_to(*initial_pos1[:2])
    )
    bob2 = (
        scene.geometry.circle(34 * scale)
        .fill(bob2_color)
        .stroke(WHITE, 4 * scale)
        .move_to(*initial_pos2[:2])
    )
    rod1 = scene.geometry.tracking_line(hinge, bob1).no_fill().stroke(bob1_color, 6 * scale)
    rod2 = scene.geometry.tracking_line(bob1, bob2).no_fill().stroke(bob2_color, 6 * scale)
    length1 = scene.mechanics.dimension_between(
        hinge,
        bob1,
        34 * scale,
        label="$L_1$",
        label_gap=30 * scale,
    ).fill(bob1_color)
    length2 = scene.mechanics.dimension_between(
        bob1,
        bob2,
        38 * scale,
        label="$L_2$",
        label_gap=30 * scale,
    ).fill(bob2_color)
    angle1 = scene.mechanics.angle_between(
        hinge,
        Direction.DOWN,
        bob1,
        radius=62 * scale,
        label="$theta_1$",
        arrowheads="end",
        show_extensions=False,
        color=bob1_color,
    )
    angle2 = scene.mechanics.angle_between(
        bob1,
        Direction.DOWN,
        bob2,
        radius=56 * scale,
        label="$theta_2$",
        arrowheads="end",
        show_extensions=False,
        color=bob2_color,
    )
    trail = (
        scene.geometry.traced_path(bob2, dissipating_time=3.5)
        .no_fill()
        .stroke(trail_color, 3 * scale)
    )
    label1 = scene.text(label1_text, role="subtitle").fill(bob1_color).move_to(
        initial_pos1[0] + label_offset[0] * scale,
        initial_pos1[1] + label_offset[1] * scale,
    )
    label2 = scene.text(label2_text, role="subtitle").fill(bob2_color).move_to(
        initial_pos2[0] + label_offset[0] * scale,
        initial_pos2[1] + label_offset[1] * scale,
    )
    return PendulumVisual(
        support,
        hinge,
        bob1,
        bob2,
        rod1,
        rod2,
        length1,
        length2,
        angle1,
        angle2,
        trail,
        label1,
        label2,
    )


def reveal_visual(visual):
    scene.play(
        [
            visual.support.animate.fade_in().duration(0.35),
            visual.hinge.animate.grow_from_center().duration(0.35),
            visual.rod1.animate.fade_in().duration(0.45),
            visual.rod2.animate.fade_in().duration(0.45),
            visual.length1.animate.fade_in().duration(0.45),
            visual.length2.animate.fade_in().duration(0.45),
            visual.angle1.animate.fade_in().duration(0.45),
            visual.angle2.animate.fade_in().duration(0.45),
            visual.bob1.animate.grow_from_center().duration(0.55),
            visual.bob2.animate.grow_from_center().duration(0.55),
            visual.label1.animate.write().duration(0.45),
            visual.label2.animate.write().duration(0.45),
        ]
    )


def activate_visual(system, visual, scale=1.0, label_offset=(-70.0, 0.0)):
    visual.bob1.add_updater_fn(system.advance, reset=system.reset, fixed_dt=1.0 / 240.0)
    visual.bob2.add_updater_fn(system.follow_second_mass)
    visual.label1.follow(
        visual.bob1,
        offset=(label_offset[0] * scale, label_offset[1] * scale),
    )
    visual.label2.follow(
        visual.bob2,
        offset=(label_offset[0] * scale, label_offset[1] * scale),
    )


def deactivate_visual(visual):
    visual.bob1.remove_updater()
    visual.bob2.remove_updater()
    visual.label1.remove_updater()
    visual.label2.remove_updater()


def make_panel(title_text, lines, accent):
    background = (
        scene.geometry.rounded_rect(520, 475, 26)
        .fill("#121A2B")
        .stroke("#344563", 2)
    )
    content = scene.layout.column(
        [
            scene.text(title_text, role="subtitle").fill(accent),
            *[scene.text(line, role="subtitle").fill(WHITE) for line in lines],
        ],
        width="fill",
        height="fill",
        padding=32,
        gap=18,
        align="center",
        justify="center",
    )
    panel = scene.layout.stack(
        [scene.layout.item(background, fit="stretch"), content],
        width=520,
        height=475,
        align="stretch",
    )
    return scene.layout.stack(
        [scene.layout.item(panel, absolute=True, offset=(625, -35))],
        within="safe",
        width="fill",
        height="fill",
    ), content


# ---------------------------------------------------------------------------
# Segmento 1 — La pregunta
# ---------------------------------------------------------------------------
scene.segment("intro", notes="Plantear la pregunta: reglas exactas, resultado impredecible.")
title = scene.text("PÉNDULO DOBLE", role="title").fill(WHITE)
subtitle = scene.text(
    "Un ejemplo de las capacidades",
    role="subtitle",
).fill("#A9B1D6").scale_to(0.78)
header = scene.layout.column(
    [title, subtitle],
    width=1200,
    height="hug",
    gap=18,
    align="center",
    justify="center",
)
header_layer = scene.layout.stack(
    [scene.layout.item(header, absolute=True, offset=(0, 420))],
    within="safe",
    width="fill",
    height="fill",
)
question = scene.text(
    "¿Puede un sistema con reglas exactas volverse impredecible?",
    role="subtitle",
).fill(GOLD).move_to(0, 40)
scene.play([title.animate.write().duration(0.8), subtitle.animate.fade_in().duration(0.6)])
scene.play([question.animate.fade_in().duration(0.7)])
scene.wait(0.8)
scene.persist(header_layer)
scene.stop("question")


# ---------------------------------------------------------------------------
# Segmento 2 — Construir el modelo
# ---------------------------------------------------------------------------
scene.segment(
    "model",
    Transition.cross_fade(0.8),
    notes="Nombrar las masas, las longitudes y el acoplamiento.",
)
model_system = DoublePendulum(-245.0, 185.0, L1=270.0, L2=260.0)
model_visual = make_visual(model_system, bob2_color=RED, trail_color="#F7768E")
guide = scene.geometry.line(-245.0, 200.0, -245.0, -330.0).stroke("#3A4864", 3)
angle1 = scene.geometry.arc(
    -245.0,
    185.0,
    86.0,
    -math.pi / 2.0,
    -math.pi / 2.0 + model_system.theta1_0,
).no_fill().stroke(GOLD, 3)
angle1_label = scene.text("$theta_1$").fill(GOLD).move_to(-165.0, 140.0)
angle2 = scene.geometry.arc(
    *model_system.positions()[0][:2],
    76.0,
    -math.pi / 2.0,
    -math.pi / 2.0 + model_system.theta2_0,
).no_fill().stroke(RED, 3)
angle2_label = scene.text("$theta_2$").fill(RED).move_to(40.0, -70.0)
panel_layer, panel_content = make_panel(
    "EL MODELO",
    ["$L_1 = L_2$", "$m_1 = m_2$", "Cada brazo", "transmite energía."],
    "#7DCFFF",
)
model_caption = scene.text(
    "$m_1$ se mueve… y convierte su posición en el nuevo pivote de $m_2$",
    role="subtitle",
).fill(WHITE).move_to(-245.0, -400.0).scale_to(0.78)

scene.play(
    [
        panel_layer.animate.fade_in().duration(0.5),
        guide.animate.fade_in().duration(0.4),
        angle1.animate.create().duration(0.5),
        angle2.animate.create().duration(0.5),
        angle1_label.animate.write().duration(0.4),
        angle2_label.animate.write().duration(0.4),
    ]
)
reveal_visual(model_visual)
scene.play([model_caption.animate.fade_in().duration(0.5)])
scene.stop("model-ready")
scene.wait(0.8)


# ---------------------------------------------------------------------------
# Segmento 3 — La dinámica
# ---------------------------------------------------------------------------
scene.segment(
    "motion",
    Transition.cross_fade(0.8),
    notes="Observar el intercambio de movimiento entre ambos brazos.",
)
scene.reuse(
    model_visual.support,
    model_visual.hinge,
    model_visual.rod1,
    model_visual.rod2,
    model_visual.bob1,
    model_visual.bob2,
)
scene.reuse(
    model_visual.length1,
    model_visual.length2,
    model_visual.angle1,
    model_visual.angle2,
    model_visual.label1,
    model_visual.label2,
)
motion_caption = scene.text(
    "La estela de $m_2$ hace visible una trayectoria que no se repite",
    role="subtitle",
).fill("#F7768E").move_to(-245.0, -400.0).scale_to(0.82)
activate_visual(model_system, model_visual)
scene.play(
    [
        model_caption.animate.fade_out().duration(0.3),
        motion_caption.animate.fade_in().duration(0.5),
        model_visual.trail.animate.fade_in().duration(0.5),
    ]
)
scene.wait(8.0)
deactivate_visual(model_visual)
scene.stop("motion-observed")


# ---------------------------------------------------------------------------
# Segmento 4 — Sensibilidad a las condiciones iniciales
# ---------------------------------------------------------------------------
scene.segment(
    "sensitivity",
    Transition.cross_fade(0.9),
    notes="Comparar dos sistemas casi idénticos y dejar que la diferencia crezca.",
)
comparison_title = scene.text(
    "SENSIBILIDAD A LAS CONDICIONES INICIALES",
    role="subtitle",
).fill(GOLD).move_to(0.0, 310.0).scale_to(0.72)
comparison_background = (
    scene.geometry.rounded_rect(520, 420, 26)
    .fill("#121A2B")
    .stroke("#344563", 2)
)
comparison_content = scene.layout.column(
    [
        scene.text("DOS SISTEMAS", role="subtitle").fill(WHITE),
        scene.text("$theta_2^A(0) = -1.150$", role="subtitle").fill("#7DCFFF"),
        scene.text("$theta_2^B(0) = -1.145$", role="subtitle").fill("#F7768E"),
        scene.text("$Delta theta_2 = 0.005$", role="subtitle").fill(GOLD),
        scene.text("misma ley · mismo pivote", role="subtitle").fill("#A9B1D6"),
    ],
    width="fill",
    height="fill",
    padding=28,
    gap=18,
    align="center",
    justify="center",
)
comparison_panel = scene.layout.stack(
    [scene.layout.item(comparison_background, fit="stretch"), comparison_content],
    width=520,
    height=420,
    align="stretch",
)
comparison_panel_layer = scene.layout.stack(
    [scene.layout.item(comparison_panel, absolute=True, offset=(625.0, -20.0))],
    within="safe",
    width="fill",
    height="fill",
)
delta_caption = scene.text(
    "Solo cambiamos $theta_2$ al inicio",
    role="subtitle",
).fill(WHITE).move_to(-245.0, -360.0).scale_to(0.78)
separation_caption = scene.text(
    "Al principio coinciden. Después, sus trayectorias se separan.",
    role="subtitle",
).fill("#A9B1D6").move_to(-245.0, -430.0).scale_to(0.72)

system_a = DoublePendulum(
    -285.0,
    120.0,
    L1=220.0,
    L2=210.0,
    # Mayor energía inicial: la diferencia pequeña entra en una región caótica.
    theta1_0=1.45,
    theta2_0=-1.150,
)
system_b = DoublePendulum(
    -285.0,
    120.0,
    L1=220.0,
    L2=210.0,
    theta1_0=1.45,
    theta2_0=-1.145,
)
visual_a = make_visual(
    system_a,
    bob1_color="#7DCFFF",
    bob2_color="#7DCFFF",
    trail_color="#7DCFFF",
    scale=0.78,
    label1_text="$A_1$",
    label2_text="$A_2$",
    label_offset=(-72.0, 0.0),
)
visual_b = make_visual(
    system_b,
    bob1_color="#F7768E",
    bob2_color="#F7768E",
    trail_color="#F7768E",
    scale=0.78,
    label1_text="$B_1$",
    label2_text="$B_2$",
    label_offset=(44.0, 0.0),
)
separation_line = (
    scene.geometry.tracking_line(visual_a.bob2, visual_b.bob2)
    .no_fill()
    .stroke(WHITE, 4)
    .z_index(3)
)
separation_label = scene.text("$Delta x(t)$", role="subtitle").fill(WHITE).move_to(-245.0, 80.0)
separation_label.follow(
    scene.geometry.point_between(visual_a.bob2, visual_b.bob2),
    offset=(0.0, 34.0),
)

scene.play(
    [
        comparison_title.animate.write().duration(0.6),
        comparison_panel_layer.animate.fade_in().duration(0.5),
        delta_caption.animate.fade_in().duration(0.5),
    ]
)
reveal_visual(visual_a)
reveal_visual(visual_b)
activate_visual(system_a, visual_a, scale=0.78, label_offset=(-72.0, 0.0))
activate_visual(system_b, visual_b, scale=0.78, label_offset=(44.0, 0.0))
scene.play([separation_caption.animate.fade_in().duration(0.5)])
scene.wait(1.2)
scene.play(
    [
        visual_a.trail.animate.fade_in().duration(0.5),
        visual_b.trail.animate.fade_in().duration(0.5),
        separation_line.animate.fade_in().duration(0.5),
        separation_label.animate.fade_in().duration(0.5),
    ]
)
scene.stop("comparison-start")
scene.wait(20.0)
deactivate_visual(visual_a)
deactivate_visual(visual_b)


# ---------------------------------------------------------------------------
# Segmento 5 — Cierre
# ---------------------------------------------------------------------------
scene.release(header_layer)
scene.segment(
    "conclusion",
    Transition.fade_through(0.8, BLACK),
    notes="Cerrar con la diferencia entre determinismo y predictibilidad.",
)
closing_title = scene.text("CAOS NO ES AZAR", role="title").fill(GOLD)
closing_body = scene.text(
    "Las ecuaciones siguen siendo deterministas.",
    role="subtitle",
).fill(WHITE)
closing_formula = scene.text(
    "Misma ley + mínima diferencia → trayectorias distintas",
    role="subtitle",
).fill("#7DCFFF").scale_to(0.82)
closing_footer = scene.text(
    "El péndulo doble convierte la sensibilidad en una experiencia visible.",
    role="subtitle",
).fill("#A9B1D6").scale_to(0.72)
closing = scene.layout.column(
    [closing_title, closing_body, closing_formula, closing_footer],
    width=1250,
    height="hug",
    gap=26,
    align="center",
    justify="center",
)
closing_layer = scene.layout.stack(
    [scene.layout.item(closing, absolute=True, offset=(0, 35))],
    within="safe",
    width="fill",
    height="fill",
)
scene.play([closing_layer.animate.fade_in().duration(0.8)])
scene.wait(2.0)


if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.5, 2.5, 6.0, 11.0, 15.0, 20.0, 24.0, 26.0])
scene.render()
