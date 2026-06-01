"""Example: Native Role-Based Color Theme Demo.

Demonstrates the premium native theme system:
  - Creating a scene with a predefined theme (Theme.DRACULA, Theme.GRUVBOX, Theme.LIGHT, etc.)
  - How spawned shapes, text, and equations automatically adopt theme.primary/secondary colors by default
  - Setting a theme dynamically using scene.set_theme(...)
  - Customizing and animating elements using scene.theme.accent, secondary, and muted roles
"""

from gaanim import Scene, Theme


def main():
    print("[Gaanim Python] Spawning Scene with a native GRUVBOX theme...")
    # Initialize the scene with the retro Gruvbox theme (background is warm dark gray)
    scene = Scene(
        width=1280,
        height=720,
        title="Gaanim — Theme & Semantic Color Roles",
        theme=Theme.GRUVBOX
    )

    # 1. Elements dynamically adopt the active theme's colors
    # Spawn text which automatically uses the theme's primary color (EBDBB2)
    title_text = scene.title("Gruvbox Role-Based Palette").at(0.0, 200.0)

    # Spawn an equation that automatically uses theme's primary color
    math_eq = scene.equation("H | psi angle.r = E | psi angle.r").at(0.0, -180.0)

    # Spawn a circle filled with the theme's secondary accent color (green B8BB26)
    circle = scene.circle(75.0).fill(scene.theme.secondary).at(-250.0, -10.0)

    # Spawn a square outlined in the theme's prominent accent color (orange FE8019)
    square = scene.rectangle(150.0, 150.0).stroke(scene.theme.accent, 6.0).no_fill().at(250.0, -10.0)

    # 2. Play animations in parallel
    print("[Gaanim Python] Queueing animations...")
    scene.play(
        circle.animate().grow_from_center().duration(1.5).spring(),
        square.animate().create(duration=2.0).smooth(),
        title_text.animate().write(duration=1.5).linear(),
        math_eq.animate().spin_in_from_nothing().duration(2.0).smooth(),
    )

    scene.wait(1.5)

    # 3. Dynamic Theme Switching: Switch to Dracula theme mid-scene!
    # Changing the theme updates the clear color and the available semantic roles.
    print("[Gaanim Python] Switching active theme to DRACULA...")
    scene.set_theme(Theme.DRACULA)

    # Indicate our elements with Dracula's prominent accent (Pink FF79C6)
    scene.play(
        circle.animate().indicate(color=scene.theme.accent, scale_factor=1.35).duration(1.5),
        math_eq.animate().indicate(color=scene.theme.secondary, scale_factor=1.2).duration(1.5),
    )

    scene.wait(1.0)

    print("[Gaanim Python] Launching GPU window render...")
    scene.render()


if __name__ == "__main__":
    main()
