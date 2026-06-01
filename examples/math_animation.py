"""Example: Mathematical formula animation.

Demonstrates the fluent Gaanim Python API:
  - Scene construction with viewport
  - Text and equation mobjects via Typst
  - Instant configuration chaining (fill, z_index, at, etc.)
  - Coordinated parallel animations via .animate() + .play()
  - Glyph-level selection and styling (math character highlighting)
  - Selection-based per-glyph shift animations
"""

from gaanim import BLUE, CORAL, GOLD, WHITE, Scene


def main():
    # 1. Initialize a high-performance Python GPU scene
    print("[Gaanim Python] Initializing GPU Scene...")
    scene = Scene(width=1280, height=720, title="Gaanim — Math Demo")

    # 2. Spawn a plain Title text (white by default, HarfBuzz-shaped)
    print("[Gaanim Python] Spawning Title text and Mathematical Equations...")
    title_text = scene.title("Gaanim Vector Engine")

    # 3. Spawn premium math formulas (rendered and compiled with Typst/NewCMMath)
    math_formula = scene.equation("E = m c^3")
    sum_formula = scene.equation("sum_(i=1)^n i = frac(n(n+1), 2)")

    # 4. Spawn a beautiful dark blue decorative circle in the background
    bg_circle = scene.circle(80).fill(BLUE).z_index(-10)

    # 5. Play coordinated initial spring animations in parallel
    print("[Gaanim Python] Queueing parallel animations...")
    scene.play(
        bg_circle.animate().scale(1.2).duration(1.5).spring(),
        title_text.animate().translate_to(-230.0, 240.0).duration(1.8).spring(),
        math_formula.animate().translate_to(-100.0, 60.0).duration(1.0).smooth(),
        sum_formula.animate().translate_to(-200.0, -150.0).duration(2.0).spring(),
    )

    # 6. Wait for a moment
    scene.wait(1.0)

    # 7. Select "m c^2" in the first equation and color it bright Gold
    print("[Gaanim Python] Performing semantic character selection and styling...")
    mc2 = scene.select(math_formula, "m c^2")
    scene.fill_selection(mc2, GOLD)

    # 8. Select "n(n+1)" in the fraction and color it bright Coral Red,
    #    then shift it up with a spring animation
    numerator = scene.select(sum_formula, "n(n+1)")
    scene.fill_selection(numerator, CORAL)

    # Build a per-glyph shift animation and play it
    sel_anim = scene.selection_anim(numerator, 0.0, 30.0)
    sel_anim.duration(1.5).spring()
    scene.play(sel_anim.build(scene))

    # 9. Final wait
    scene.wait(1.5)

    # 10. Render using Vulkan GPU pipeline
    print("[Gaanim Python] Starting native Vulkan GPU Renderer window...")
    scene.render()


if __name__ == "__main__":
    main()
