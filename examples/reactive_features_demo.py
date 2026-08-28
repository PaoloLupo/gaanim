"""Reactive updaters, tracking lines, and traced paths using Scene."""

from gaanim import BLACK, BLUE, CORAL, CYAN, GOLD, GREEN, ORANGE, WHITE, Scene, Updater


def main():
    scene = Scene(frame=(16, 9), background=BLACK)
    title = scene.text("Gaanim Reactive Engine", role="title").fill(WHITE).move_to(0, 3)
    orbiting_dot = scene.geometry.dot(0.1875).fill(ORANGE).move_to(1.875, 0)
    orbiting_dot.add_updater(Updater.orbit(0, 0, 1.875, 2.0))
    trail = scene.geometry.traced_path(orbiting_dot).stroke(CYAN, 0.05).no_fill()
    radius = scene.geometry.tracking_line((0, 0), orbiting_dot).stroke(BLUE, 0.025).no_fill()

    bobbing_dot = scene.geometry.dot(0.175).fill(GOLD).move_to(3.75, 0)
    bobbing_dot.add_updater(Updater.bob(0.625, 0.5))
    pulsing_square = scene.geometry.square(0.75).fill(CORAL).move_to(-3.75, -1.375)
    pulsing_square.add_updater(Updater.pulse(1.0, 1.3, 1.0))
    pulsing_square.add_updater(Updater.rotate(1.5))
    follower = scene.geometry.dot(0.125).fill(GREEN).move_to(-2.25, -1.375)
    follower.add_updater(Updater.advance_x(0.5625))

    scene.play([
        title.animate.write().duration(1.0),
        orbiting_dot.animate.grow_from_center().duration(0.5),
        trail.animate.fade_in().duration(0.5),
        radius.animate.create().duration(0.5),
        bobbing_dot.animate.grow_from_center().duration(0.5),
        pulsing_square.animate.create().duration(0.5),
        follower.animate.fade_in().duration(0.5),
    ])
    scene.wait(5.0)
    orbiting_dot.remove_updater()
    bobbing_dot.remove_updater()
    pulsing_square.remove_updater()
    follower.remove_updater()
    scene.render()


if __name__ == "__main__":
    main()
