"""Example: Advanced Animations Demo.

Demonstrates all 7 new advanced animations:
  - Create (parallel draw)
  - Uncreate (parallel erase)
  - Unwrite (staggered reverse erase)
  - GrowFromCenter (scale from 0)
  - ShrinkToCenter (scale to 0)
  - SpinInFromNothing (scale + 360-deg rotation)
  - Indicate (temporary highlight and scale)
"""

from gaanim import BLUE, CORAL, GOLD, Scene


def main():
    print("[Gaanim Python] Initializing Advanced Animations Demo Scene...")
    scene = Scene(width=1280, height=720, title="Gaanim — Advanced Animations Demo")

    # 1. Spawn a series of distinct mobjects
    print("[Gaanim Python] Spawning shapes, text and equation...")
    square = scene.rectangle(150.0, 150.0).stroke(CORAL, 6.0).no_fill().at(200.0, 200.0)
    circle = scene.circle(75.0).fill(BLUE).at(640.0, 200.0)
    title_text = scene.title("Advanced Animations").at(150.0, 450.0)
    math_eq = scene.equation("f(x) = integral_(-oo)^oo e^(-x^2) d x").at(150.0, 580.0)

    # 2. Play creation / entry animations in parallel
    print("[Gaanim Python] Phase 1: Playing entry animations (Create, Grow, Spin)...")
    scene.play(
        square.animate().create(duration=2.5).smooth(),
        circle.animate().grow_from_center().duration(1.8).spring(),
        title_text.animate().spin_in_from_nothing().duration(2.2).smooth(),
        math_eq.animate().write(duration=3.0).linear(),
    )

    # Wait for 1 second
    scene.wait(1.0)

    # 3. Play emphasis animations (Indicate)
    print("[Gaanim Python] Phase 2: Playing emphasis animations (Indicate)...")
    scene.play(
        circle.animate().indicate(color=GOLD, scale_factor=1.3).duration(1.5),
        math_eq.animate().indicate(color=CORAL, scale_factor=1.2).duration(1.5),
    )

    # Wait for 1 second
    scene.wait(1.0)

    # 4. Play exit / destruction animations (Uncreate, Shrink, Unwrite)
    print("[Gaanim Python] Phase 3: Playing exit/destruction animations (Uncreate, Shrink, Unwrite)...")
    scene.play(
        square.animate().uncreate(duration=2.5).smooth(),
        circle.animate().shrink_to_center().duration(1.8).spring(),
        title_text.animate().unwrite(duration=2.2).smooth(),
        math_eq.animate().unwrite(duration=3.0).linear(),
    )

    # Final wait before exit
    scene.wait(1.0)

    print("[Gaanim Python] Launching GPU window render...")
    scene.render()


if __name__ == "__main__":
    main()
