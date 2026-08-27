"""Four semantic equation animations: terms, reveal, indication, and focus."""

from gaanim import BLACK, CORAL, GOLD, GRAY, WHITE, Scene, part


scene = Scene(1280, 720, background=BLACK)

title = scene.text("Ecuaciones por significado, no por glifos", role="title").fill(WHITE).at(0, 270)
formula = scene.text.equation(
    part("energy", "E"), part("equals", "="),
    part("mass", "m"), part("light_speed", "c^2")
).at(0, 70)
formula["mass"].fill(GOLD)
# CORAL keeps c² legible on the black canvas; reveal_fragment does not change color.
formula["light_speed"].fill(CORAL)

caption = scene.text("1. Escribir cada término semántico").fill(GRAY).at(0, -100)

scene.play([title.write(), caption.fade_in()])
# 1. E, =, m y c² aparecen como términos completos y ordenados.
scene.play([formula.write(1.8, by="part")])
scene.wait(0.4)

# 2. Vuelve a introducir un fragmento usando uno de los presets de revelado.
caption.fade_out(duration=0.2)
caption = scene.text("2. Revelar c² desde abajo").fill(GRAY).at(0, -100)
scene.play([caption.fade_in()])
scene.play([formula["light_speed"].reveal(style="from_below", duration=0.65)])
scene.wait(0.35)

# 3. El tag evita repetir el fragmento crudo en la llamada de animación.
caption.fade_out(duration=0.2)
caption = scene.text("3. Indicar la masa").fill(GRAY).at(0, -100)
scene.play([caption.fade_in()])
scene.play([formula["mass"].indicate(duration=0.65)])
scene.wait(0.35)

# 4. El foco atenúa todo salvo los tags que cuentan la idea actual.
caption.fade_out(duration=0.2)
caption = scene.text("4. Enfocar los términos que intervienen").fill(GRAY).at(0, -100)
scene.play([caption.fade_in()])
scene.play([
    formula["mass"].focus(duration=0.7),
    formula["light_speed"].focus(duration=0.7),
])
scene.wait(1.0)

scene.render()
