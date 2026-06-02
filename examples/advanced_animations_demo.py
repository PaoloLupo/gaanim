"""Example: Advanced Animations Demo with Themes.

Demuestra las 7 nuevas animaciones avanzadas utilizando el sistema de temas dinámicos
y enseña las reglas de temporización del motor Gaanim (paralelo vs secuencial).
"""

from gaanim import Scene, Theme


def main():
    # Inicializar la escena con el tema premium Dracula (fondo morado oscuro)
    # NOTA: Gaanim utiliza un sistema de cola diferida. Definir animaciones aquí
    # solo las registra en un Timeline. Al llamar a scene.render(), se compila
    # todo en Rust y se abre la ventana GPU para reproducirlas visualmente.
    scene = Scene(
        width=1280,
        height=720,
        title="Gaanim — Advanced Animations & Timing Demo",
        theme=Theme.DRACULA,
    )

    # 1. Spawning de mobjects con estilos dinámicos del tema Dracula
    # Rectángulo (cuadrado) delineado con el acento rosa de Dracula
    square = (
        scene.rectangle(150.0, 150.0)
        .stroke(scene.theme.accent, 6.0)
        .no_fill()
        .at(300.0, -20.0)
    )

    # Círculo relleno con el color secundario cian de Dracula
    circle = scene.circle(75.0).fill(scene.theme.secondary).at(-300.0, -20.0)

    # Texto de título que adopta automáticamente el color primario del tema (blanco apagado)
    title_text = scene.title("Advanced Animations").at(0.0, 180.0)

    # Ecuación cuadrática elegante en Typst que adopta el color primario del tema
    math_eq = scene.equation("f(x) = x^2 - 2x + 1").at(0.0, -20.0)

    # =========================================================================
    # REGLA DE TEMPORIZACIÓN 1: ANIMACIONES EN PARALELO (SIMULTÁNEAS)
    # Pasar múltiples animaciones dentro de una misma llamada a scene.play(...)
    # hace que todas se reproduzcan al mismo tiempo de forma paralela.
    # =========================================================================
    scene.play(
        square.animate().create(duration=2.5).smooth(),
        # GrowFromCenter usa spring (resorte elástico) para una entrada dinámica muy llamativa
        circle.animate().grow_from_center().duration(1.8).spring(),
        title_text.animate().spin_in_from_nothing().duration(2.2).smooth(),
        math_eq.animate().write(duration=3.0).linear(),
    )

    # =========================================================================
    # REGLA DE TEMPORIZACIÓN 2: ANIMACIONES SECUENCIALES (UNA TRAS OTRA)
    # Cada llamada individual y sucesiva a scene.play() o scene.wait() se encola
    # para ejecutarse estrictamente una después de la otra de forma cronológica.
    # =========================================================================
    scene.wait(1.0)  # Espera secuencial de 1.0 segundos

    # Fase de énfasis en paralelo
    scene.play(
        circle.animate()
        .indicate(color=scene.theme.accent, scale_factor=1.3)
        .duration(1.5),
        math_eq.animate()
        .indicate(color=scene.theme.secondary, scale_factor=1.2)
        .duration(1.5),
    )

    scene.wait(1.0)  # Otra espera secuencial de 1.0 segundos

    # Fase de salida / destrucción en paralelo
    scene.play(
        square.animate().uncreate(duration=2.5).smooth(),
        # Para encoger a cero (ShrinkToCenter), usamos una curva .smooth() (suave).
        # Evitamos usar .spring() en la desaparición ya que los resortes oscilan alrededor
        # de cero, lo que generaría escalas negativas e inversiones de forma abruptas.
        circle.animate().shrink_to_center().duration(1.8).smooth(),
        title_text.animate().unwrite(duration=2.2).smooth(),
        math_eq.animate().unwrite(duration=3.0).linear(),
    )

    scene.wait(1.0)  # Espera final antes de terminar la línea de tiempo

    # 2. Renderizar usando la pipeline GPU Vulkan nativa
    # Esta llamada compila el timeline en Rust, abre la ventana interactiva y reproduce
    # todas las fases que hemos encolado secuencialmente arriba.
    scene.render()


if __name__ == "__main__":
    main()
