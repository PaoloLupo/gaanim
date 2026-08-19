"""Export a Scene to the format selected by the filename extension."""

import sys

from gaanim import BLACK, BLUE, GOLD, WHITE, Scene


def main():
    outputs = {"a": "output_youtube.mp4", "b": "output_overlay.webm", "c": "output_tiktok.mp4"}
    option = sys.argv[1].lstrip("-").lower() if len(sys.argv) > 1 else "a"
    if option not in outputs:
        raise SystemExit("Usage: gaanim examples/export_demo.py [a|b|c]")

    scene = Scene(1920, 1080, background=BLACK)
    equation = scene.equation("integral_a^b f(x) d x = F(b) - F(a)").fill(WHITE).at(0, 100)
    circle = scene.circle(120).stroke(GOLD, 6).no_fill().at(-250, -150)
    rect = scene.rect(200, 120).fill(BLUE).at(250, -150)

    scene.play([equation.write().duration(1.5), circle.create().duration(1.2), rect.grow_from_center().duration(1.2)])
    scene.play([circle.move(100, 0).duration(1.0), rect.rotate(1.5708).duration(1.0)])
    scene.wait(1.0)
    scene.export(outputs[option], fps=30)


if __name__ == "__main__":
    main()
