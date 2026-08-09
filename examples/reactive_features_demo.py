"""Reactive updaters, tracking lines, and traced paths using Scene."""

from gaanim import BLACK, BLUE, CORAL, CYAN, GOLD, GREEN, ORANGE, WHITE, Scene, Updater


def main():
    scene = Scene(1280, 720, background=BLACK)
    title = scene.title("Gaanim Reactive Engine").fill(WHITE).at(0, 240)
    orbiting_dot = scene.dot(15).fill(ORANGE).at(150, 0)
    orbiting_dot.add_updater(Updater.orbit(0, 0, 150, 2.0))
    trail = scene.traced_path(orbiting_dot).stroke(CYAN, 4).no_fill()
    radius = scene.tracking_line((0, 0), orbiting_dot).stroke(BLUE, 2).no_fill()

    bobbing_dot = scene.dot(14).fill(GOLD).at(300, 0)
    bobbing_dot.add_updater(Updater.bob(50, 0.5))
    pulsing_square = scene.square(60).fill(CORAL).at(-300, -110)
    pulsing_square.add_updater(Updater.pulse(0.7, 1.3, 1.0))
    pulsing_square.add_updater(Updater.rotate(1.5))
    follower = scene.dot(10).fill(GREEN).at(-180, -110)
    follower.add_updater(Updater.advance_x(45))

    scene.play([
        title.write().duration(1.0),
        orbiting_dot.grow_from_center().duration(0.5),
        trail.fade_in().duration(0.5),
        radius.create().duration(0.5),
        bobbing_dot.grow_from_center().duration(0.5),
        pulsing_square.create().duration(0.5),
        follower.fade_in().duration(0.5),
    ])
    scene.wait(5.0)
    orbiting_dot.remove_updater()
    bobbing_dot.remove_updater()
    pulsing_square.remove_updater()
    follower.remove_updater()
    scene.render()


if __name__ == "__main__":
    main()
