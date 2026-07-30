"""Plantilla completa para una sustentación de tesis en Gaanim.

Desarrollo:
    gaanim thesis_presentation.py

Presentación:
    gaanim --present --monitor 1 thesis_presentation.py

Reemplaza los textos, datos y notas entre corchetes. Cada ``slide.step()``
define una pausa controlada por el expositor.
"""

import os

from gaanim import WHITE, Scene, ThesisTemplate


scene = Scene(1920, 1080, margin=72)
scene.canvas.set_safe_area(top=52, right=72, bottom=52, left=72)

# Identidad visual ------------------------------------------------------------
# Tw Cen MT se detecta en C:\Windows\Fonts\TCM_____.TTF. Para usar otra copia
# licenciada, define GAANIM_TW_CEN_FONT o pasa font_path="assets/TwCenMT.ttf".
# El logo debe ser un SVG blanco; si se omite aparece un marcador vectorial.
design = ThesisTemplate(
    scene,
    font_path=os.environ.get("GAANIM_TW_CEN_FONT"),
    logo=os.environ.get("GAANIM_THESIS_LOGO"),
    background="#1601FC",
    institution="UNIVERSIDAD CATÓLICA SAN PABLO",
    faculty="FACULTAD DE ARQUITECTURA, COMPUTACIÓN E INGENIERÍAS",
    school="ESCUELA PROFESIONAL DE INGENIERÍA CIVIL",
)
ACCENT = scene.canvas.color("accent")
PRESENTATION_BLUE = scene.canvas.color("chart")
PRESENTATION_PANEL = scene.canvas.color("panel")


def title_in(slide, text):
    return design.title(slide, text)


def thesis_point(text, y, *, color=WHITE):
    marker = scene.dot(8).fill(ACCENT).at(-680, y + 4)
    body = scene.paragraph(
        text,
        1240,
        font_size=34,
        line_spacing=1.15,
    ).fill(color).at(80, y)
    return marker, body


# 1. Portada -----------------------------------------------------------------
cover = design.cover(
    "MARCO DE TRABAJO PARA LA\n"
    "IMPLEMENTACIÓN AUTOMATIZADA DEL\n"
    "DISEÑO DE MUROS DE ALBAÑILERÍA\n"
    "CONFINADA",
    "PAOLO CESAR GUILLEN LUPO  •  PAMELA BANDA ALARTA",
    "AGOSTO 2026",
    notes="[Saluda, preséntate y enuncia la pregunta central de la investigación.]",
)
scene.wait(0.6)
cover.step("inicio")


# 2. Ruta de la exposición ----------------------------------------------------
agenda = scene.slide(
    "Ruta de la exposición",
    notes="[Explica la estructura. No leas la lista; anticipa el hilo argumental.]",
    layout="title_content",
)
title_in(agenda, "Ruta de la exposición")
agenda_items = [
    thesis_point("1. Problema y motivación", 170),
    thesis_point("2. Objetivos y fundamento teórico", 70),
    thesis_point("3. Metodología y desarrollo", -30),
    thesis_point("4. Resultados, conclusiones y trabajo futuro", -130),
]
for index, (marker, item) in enumerate(agenda_items, start=1):
    scene.play(
        [
            marker.fade_in().duration(0.18),
            item.write().duration(0.22),
        ]
    )
    agenda.step(f"agenda-{index}")


# 3. Problema ---------------------------------------------------------------
problem = scene.slide(
    "Problema",
    notes="[Describe la brecha observable, su impacto y la evidencia que la cuantifica.]",
    layout="two_columns",
)
title_in(problem, "Problema de investigación")
problem_text = scene.paragraph(
    "[Describe aquí el problema en dos o tres frases. Conecta el contexto, la brecha "
    "actual y la consecuencia que justifica investigar.]",
    700,
    font_size=34,
    line_spacing=1.25,
).at(-430, -10)
problem_data = scene.bar_chart(
    [28, 46, 73],
    labels=["Base", "Actual", "Brecha"],
    width=650,
    height=360,
).at(450, -20)
scene.play(
    [
        problem_text.write().duration(0.55),
        problem_data.fade_in().duration(0.45),
    ]
)
problem.step("evidencia")


# 4. Objetivos ---------------------------------------------------------------
objectives = scene.slide(
    "Objetivos",
    notes="[Formula el objetivo general como respuesta directa al problema.]",
    layout="title_content",
)
title_in(objectives, "Objetivos")
scene.paragraph(
    "Objetivo general: [verbo en infinitivo + aporte + contexto + criterio de éxito].",
    1370,
    font_size=38,
).fill(ACCENT).at(0, 180)
specific_objectives = [
    thesis_point("OE1. [Caracterizar o diagnosticar el estado inicial.]", 60),
    thesis_point("OE2. [Diseñar o implementar la propuesta.]", -50),
    thesis_point("OE3. [Evaluar la propuesta con métricas verificables.]", -160),
]
for index, (marker, item) in enumerate(specific_objectives, start=1):
    scene.play(
        [
            marker.fade_in().duration(0.2),
            item.write().duration(0.28),
        ]
    )
    objectives.step(f"objetivo-{index}")


# 5. Fundamento teórico -------------------------------------------------------
theory = scene.slide(
    "Fundamento teórico",
    notes="[Define solo los conceptos que usarás después para interpretar resultados.]",
    layout="title_content",
)
title_in(theory, "Modelo conceptual")
model = scene.equation("Y = alpha + beta_1 X_1 + beta_2 X_2 + epsilon").fill(WHITE).at(0, 70)
model_note = scene.paragraph(
    "[Explica qué representa cada variable y cuál es la relación que tu tesis contrasta.]",
    1240,
    align="center",
    font_size=34,
).at(0, -150)
scene.play(
    [
        model.write().duration(0.55),
        model_note.fade_in().duration(0.35),
    ]
)
theory.step("modelo")


# 6. Metodología --------------------------------------------------------------
method = scene.slide(
    "Metodología",
    notes="[Justifica cada etapa y menciona población, muestra, instrumentos y análisis.]",
    layout="title_content",
)
title_in(method, "Diseño metodológico")
method_x = [-630, -210, 210, 630]
method_names = ["Diagnóstico", "Diseño", "Ejecución", "Evaluación"]
for x, name in zip(method_x, method_names):
    scene.rounded_rect(300, 150, 24).fill(PRESENTATION_PANEL).stroke(
        PRESENTATION_BLUE, 4
    ).at(x, -20)
    scene.text(name).fill(WHITE).at(x, -20)
method_arrows = [
    scene.arrow(-465, -20, -375, -20).stroke(ACCENT, 7),
    scene.arrow(-45, -20, 45, -20).stroke(ACCENT, 7),
    scene.arrow(375, -20, 465, -20).stroke(ACCENT, 7),
]
scene.play([arrow.create().duration(0.45) for arrow in method_arrows])
method.step("proceso")


# 7. Resultados ---------------------------------------------------------------
results = scene.slide(
    "Resultados",
    notes="[Declara primero el hallazgo; después muestra la evidencia y su magnitud.]",
    layout="two_columns",
)
title_in(results, "Resultado principal")
result_chart = scene.bar_chart(
    [42, 61, 86],
    labels=["Inicial", "Piloto", "Final"],
    width=760,
    height=410,
).at(-380, -20)
result_text = scene.paragraph(
    "Hallazgo clave: [escribe una afirmación cuantificada]. "
    "La intervención produjo [magnitud del cambio] bajo [condiciones].",
    700,
    font_size=36,
    line_spacing=1.25,
).fill(WHITE).at(480, 20)
scene.play(
    [
        result_chart.fade_in().duration(0.5),
        result_text.write().duration(0.5),
    ]
)
results.step("hallazgo-principal")


# 8. Conclusiones -------------------------------------------------------------
conclusions = scene.slide(
    "Conclusiones",
    notes="[Cada conclusión debe responder a un objetivo y estar respaldada por resultados.]",
    layout="title_content",
)
title_in(conclusions, "Conclusiones")
conclusion_items = [
    thesis_point("1. [Respuesta concreta al problema de investigación.]", 120),
    thesis_point("2. [Aporte teórico, metodológico o tecnológico.]", 0),
    thesis_point("3. [Limitación principal y alcance correcto del resultado.]", -120),
]
for index, (marker, item) in enumerate(conclusion_items, start=1):
    scene.play(
        [
            marker.fade_in().duration(0.2),
            item.write().duration(0.28),
        ]
    )
    conclusions.step(f"conclusion-{index}")


# 9. Cierre -------------------------------------------------------------------
closing = scene.slide(
    "Preguntas",
    notes="[Agradece. Durante preguntas usa O en Presenter View para saltar a cualquier tema.]",
    layout="closing",
)
scene.title("Gracias").fill(ACCENT).at(0, 180)
scene.subtitle("[Una frase final que resuma el aporte de la tesis]").fill(WHITE).at(0, 40)
scene.text("[correo@universidad.edu]").fill(PRESENTATION_BLUE).at(0, -100)
questions = scene.text("Preguntas").fill(WHITE).at(0, -240)
scene.play([questions.write().duration(0.45)])
closing.step("preguntas")
scene.wait(1.0)


if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(
        snapshots,
        [0.2, 0.8, 1.6, 2.15, 2.95, 3.45, 3.95, 4.5, 5.3],
    )
elif output := os.environ.get("GAANIM_EXPORT"):
    scene.export(output, fps=60, quality="production")
else:
    scene.render()
