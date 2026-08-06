"""Piecewise path motion and arrow creation using Scene primitives."""

from gaanim import BLACK, BLUE, GREEN, RED, WHITE, YELLOW, Scene


def main():
    scene = Scene(1920, 1080, background=BLACK)
    corners = [(-300, -150), (300, -150), (300, 150), (-300, 150), (-300, -150)]
    guides = [
        scene.line(x1, y1, x2, y2).stroke(YELLOW, 2).opacity(0.45)
        for (x1, y1), (x2, y2) in zip(corners, corners[1:])
    ]
    traveler = scene.circle(28).fill(BLUE).stroke(WHITE, 2).at(*corners[0])
    second = scene.dot(14).fill(RED).at(-420, -280)
    arrow = scene.arrow(-600, 260, 0, 260).stroke(GREEN, 4)

    scene.play([guide.create().duration(0.5) for guide in guides], lag=0.08)
    scene.play([traveler.grow_from_center().duration(0.5), second.grow_from_center().duration(0.5), arrow.create().duration(0.8)])
    for start, end in zip(corners, corners[1:]):
        scene.play([traveler.move(end[0] - start[0], end[1] - start[1]).duration(0.7).linear()])
    scene.play([second.move(840, 0).duration(1.2).smooth()])
    scene.wait(0.5)
    scene.render()


if __name__ == "__main__":
    main()
