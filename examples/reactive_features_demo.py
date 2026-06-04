"""Example: ValueTrackers, Updaters, TracedPaths and PassingFlashes.

Demonstrates:
  - ValueTracker (FloatSignal) for tracking dynamic float parameters.
  - DecimalNumber displaying the ValueTracker value dynamically.
  - ShowPassingFlash to run a bright neon segment along paths.
  - Preset updaters: Bob, Rotate, Orbit, Pulse, and Follow.
  - TracedPath tracing the trail of moving mobjects.
"""

import math

from gaanim import BLUE, CORAL, CYAN, GOLD, GREEN, ORANGE, RED, WHITE, Scene


def main():
    print("[Gaanim Python] Initializing Reactive Demo Scene...")
    scene = Scene(width=1280, height=720, title="Gaanim — Reactive Features & Updaters")

    # 1. Spawn a title
    title = scene.title("Gaanim Reactive Engine")

    # 2. Spawn a ValueTracker starting at 0.0
    tracker = scene.value_tracker(0.0)

    # 3. Create a DecimalNumber displaying the tracker value
    # Configured with 2 decimal places, a prefix, and Inter font
    counter = scene.decimal_number(
        tracker, num_decimals=2, prefix="Contador: ", font_size=40.0
    )
    counter.shift(-300, 200)

    # 4. Create an Orbiting Circle that leaves a trail (TracedPath)
    orbiting_dot = scene.circle(15).fill(ORANGE).z_index(5)
    # The dot orbits around center (0, 0) with radius 150 at speed 2.0 rad/s
    orbiting_dot.add_orbit_updater(scene, cx=0.0, cy=0.0, radius=150.0, speed=2.0)

    # Trace the trail of the orbiting dot with a beautiful cyan path
    trail = scene.traced_path(
        orbiting_dot, color=CYAN, width=4.0, min_distance=2.0, max_points=200
    )

    # 5. Create a bobbing star with follow
    bobbing_star = (
        scene.star(n_points=5, outer_radius=40, inner_radius=15)
        .fill(GOLD)
        .shift(300, 0)
    )
    bobbing_star.add_bob_updater(scene, amplitude=50.0, frequency=0.5)

    # Let a small green circle follow the star with some offset and smoothing
    follower_circle = scene.circle(12).fill(GREEN)
    follower_circle.add_follow_updater(
        scene, bobbing_star, ox=50.0, oy=0.0, smoothing=0.1
    )

    # 6. Create a pulsing and rotating square
    pulsing_square = scene.square(60).fill(CORAL).shift(-300, -100)
    pulsing_square.add_pulse_updater(scene, min_scale=0.7, max_scale=1.3, frequency=1.0)
    pulsing_square.add_rotate_updater(scene, speed=1.5)

    # 7. Create a static sine-like curve path to run a ShowPassingFlash on
    line_path = scene.line(-400, -250, 400, -250).stroke(BLUE, 2.0)

    # 8. Let's play the entry write animation for the title and counter
    print("[Gaanim Python] Playing entry animations...")
    scene.play(
        title.animate().write(duration=1.0), counter.animate().write(duration=1.0)
    )
    scene.wait(0.5)

    # 9. Animate the ValueTracker! This will automatically update the DecimalNumber!
    # The tracker value goes from 0.0 to 100.0 over 4.0 seconds, using spring rate func.
    # Concurrently, we run a neon passing flash along the blue line path.
    print("[Gaanim Python] Animating ValueTracker and running ShowPassingFlash...")
    scene.play(
        tracker.animate_to(100.0, duration=4.0).spring(),
        line_path.animate().show_passing_flash(duration=4.0, time_width=0.3),
    )

    # 10. Wait a bit, then animate the ValueTracker back to 0.0 using smooth easing
    print("[Gaanim Python] Animating ValueTracker back to 0.0...")
    scene.play(
        tracker.animate_to(0.0, duration=3.0).smooth(),
        line_path.animate().show_passing_flash(duration=3.0, time_width=0.1),
    )

    scene.wait(1.0)

    # 11. Run rendering window
    print("[Gaanim Python] Starting native Vulkan GPU Renderer window...")
    scene.edit()


if __name__ == "__main__":
    main()
