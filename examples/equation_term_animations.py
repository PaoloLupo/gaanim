"""Four semantic equation animations: terms, reveal, indication, and focus."""

from gaanim import BLACK, BLUE, GOLD, GRAY, WHITE, Scene


scene = Scene(1280, 720, background=BLACK)

title = scene.title("Ecuaciones por significado, no por glifos").fill(WHITE).at(0, 270)
formula = scene.equation(
    "E = m c^2",
    tags={
        "energy": "E",
        "equals": "=",
        "mass": "m",
        "light_speed": "c^2",
    },
).at(0, 70)
formula.tag("mass").fill(GOLD)
formula.tag("light_speed").fill(BLUE)

caption = scene.text("1. Escribir cada término semántico").fill(GRAY).at(0, -100)

scene.play([title.write(), caption.fade_in()])
# 1. E, =, m y c² aparecen como términos completos y ordenados.
formula.write_by_term(duration=1.8)
scene.wait(0.4)

# 2. Vuelve a introducir un fragmento usando uno de los presets de revelado.
caption.fade_out(duration=0.2)
caption = scene.text("2. Revelar c² desde abajo").fill(GRAY).at(0, -100)
scene.play([caption.fade_in()])
formula.reveal_fragment("c^2", style="from_below", duration=0.65)
scene.wait(0.35)

# 3. El tag evita repetir el fragmento crudo en la llamada de animación.
caption.fade_out(duration=0.2)
caption = scene.text("3. Indicar la masa").fill(GRAY).at(0, -100)
scene.play([caption.fade_in()])
formula.indicate_tag("mass", duration=0.65)
scene.wait(0.35)

# 4. El foco atenúa todo salvo los tags que cuentan la idea actual.
caption.fade_out(duration=0.2)
caption = scene.text("4. Enfocar los términos que intervienen").fill(GRAY).at(0, -100)
scene.play([caption.fade_in()])
scene.focus_equation(
    formula,
    ["mass", "light_speed"],
    duration=0.7,
    dim_opacity=0.18,
)
scene.wait(1.0)

scene.render()
