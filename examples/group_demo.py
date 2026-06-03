"""Example: Mobject Grouping and Styling.

Demonstrates creating groups, styling the group dynamically,
indexing child elements, and playing coordinated animations.
"""

from gaanim import BLUE, GOLD, RED, Scene


def main():
    scene = Scene(
        width=1280,
        height=720,
        title="Gaanim - Group Demo",
    )

    # 1. Create individual shapes
    circle_left = scene.circle(radius=60.0).at(-100.0, 0.0)
    circle_right = scene.circle(radius=60.0).at(100.0, 0.0)
    square_top = scene.square(side=80.0).at(0.0, 100.0)

    print("Shapes created.")

    # 2. Group them together
    group = scene.group([circle_left, circle_right, square_top])

    print(f"Group created with {len(group)} children.")

    # 3. Assert index access works
    child_0 = group[0]
    print(f"First child index access works: {child_0}")

    # 4. Style the group (propagates to all children at runtime)
    group.fill(GOLD)

    # 5. Play animations on the group as a single unit
    scene.play(group.animate().shift(0.0, -100.0).duration(2.0).spring())

    # 6. Wait a bit
    scene.wait(1.0)

    # 7. Ungroup them
    scene.ungroup(group)
    print("Group dissolved. Children are now roots.")

    # 8. Render
    scene.edit()


if __name__ == "__main__":
    main()
