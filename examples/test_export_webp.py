"""Quick smoke test for WebP export and performance improvements."""

import sys

from gaanim import BLUE, GOLD, Scene


def main():
    preset = sys.argv[1] if len(sys.argv) > 1 else "webp"

    print(f"Smoke test: {preset}")

    scene = Scene(width=1920, height=1080, title="Performance / WebP Export Test")

    eq = scene.equation("integral_a^b f(x) d x = F(b) - F(a)").at(0, 0).scale(1.2)
    circle = scene.circle(120).stroke(GOLD, 6).no_fill().at(-250, 0)
    rect = scene.rectangle(200, 120).fill(BLUE).at(250, 0)

    scene.play(eq.write(1.5))
    scene.play(circle.create(1.0), rect.fade_in_anim().duration(1.0))
    scene.play(
        circle.shift_anim(100, 0).duration(0.5),
        rect.rotate_anim(3.14 / 2).duration(0.5),
    )
    scene.wait(0.5)

    if preset == "webp":
        scene.export(
            "output_test.webp", fps=30, quality="draft", aspect_ratio="youtube"
        )
    elif preset == "mp4_draft":
        scene.export(
            "output_test_draft.mp4", fps=30, quality="draft", aspect_ratio="youtube"
        )
    elif preset == "mp4_standard":
        scene.export(
            "output_test_standard.mp4",
            fps=60,
            quality="standard",
            aspect_ratio="youtube",
        )
    elif preset == "webm":
        scene.export(
            "output_test.webm",
            fps=30,
            quality="draft",
            transparent=True,
            aspect_ratio="youtube",
        )
    elif preset == "gif":
        scene.export("output_test.gif", fps=15, quality="draft", aspect_ratio="youtube")
    elif preset == "png":
        scene.export("output_test.png", fps=30, quality="draft", aspect_ratio="youtube")
    else:
        print(f"Unknown preset: {preset}")
        print("Options: webp, mp4_draft, mp4_standard, webm, gif, png")


if __name__ == "__main__":
    main()
