"""Export demo script.

Demonstrates Gaanim's state-of-the-art offline export engine.
Shows H.264 MP4 export, vertical aspect-ratio presets (TikTok/Shorts),
and transparent WebM overlays for premium educational content production!
"""

import sys
from gaanim import BLUE, GOLD, RED, Scene, CORAL, WHITE


def main():
    # Parse CLI arguments to choose preset options
    option = "a"
    if len(sys.argv) > 1:
        arg = sys.argv[1].lower().strip("-")
        if arg in ["a", "b", "c"]:
            option = arg
        else:
            print("Usage: python examples/export_demo.py [a|b|c]")
            print("  a: Premium YouTube standard MP4 (1080p, 60fps)")
            print("  b: Transparent WebM overlay (draft quality)")
            print("  c: TikTok/Shorts vertical MP4 (draft quality)")
            sys.exit(1)

    print("🎬 Creating educational scene...")
    scene = Scene(width=1920, height=1080, title="State-of-the-Art Export Demo")
    
    # 1. Spawn a math formula with typst (visually elegant!)
    eq = scene.equation("integral_a^b f(x) d x = F(b) - F(a)").at(0, 100).scale(1.2)
    
    # 2. Spawn a circle and a rectangle
    circle = scene.circle(120).stroke(GOLD, 6).no_fill().at(-250, -150)
    rect = scene.rectangle(200, 120).fill(BLUE).at(250, -150)
    
    # 3. Animate them
    scene.play(eq.write(2.0))
    scene.play(circle.create(1.5), rect.fade_in_anim().duration(1.5))
    
    scene.play(
        circle.shift_anim(100, 0).duration(1.0),
        rect.rotate_anim(3.14 / 2).duration(1.0)
    )
    scene.wait(1.0)
    
    # --- EXPORTS ---
    
    if option == "a":
        # A. Premium 1080p60 MP4 (YouTube Standard format)
        print("\n🎥 Render Option A: Standard YouTube Video (MP4)...")
        scene.export(
            "output_youtube.mp4",
            fps=60,
            aspect_ratio="youtube",
            quality="standard"
        )
    elif option == "b":
        # B. Transparent WebM Overlay (Super useful to import directly into Premiere/Davinci over real footage!)
        print("\n🎥 Render Option B: Transparent WebM Overlay (Draft)...")
        scene.export(
            "output_overlay.webm",
            fps=30,
            aspect_ratio="youtube",
            transparent=True,
            quality="draft"
        )
    elif option == "c":
        # C. TikTok/Shorts Vertical Preset (9:16)
        print("\n🎥 Render Option C: TikTok/Shorts Vertical Video (9:16, Draft)...")
        scene.export(
            "output_tiktok.mp4",
            fps=30,
            aspect_ratio="tiktok",
            quality="draft"
        )


if __name__ == "__main__":
    main()

