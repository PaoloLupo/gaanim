"""Static guard: Cartesian3D boxes in examples must be visible from their camera.

`Visualization.cartesian_3d(size=...)` is a world-space extent in logical scene
units. The pixel-to-logical-unit migration (f18299b) divided these sizes by the
old pixels-per-unit factor, shrinking the axes, grids and surfaces of
`axes_3d_demo`, `nonlinear_axes_3d`, `visualization_visibility` and
`lorenz_3d_demo` to a single dot at the centre of the frame. This test parses
the examples without importing the host-owned runtime.
"""

import ast
import math
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
FRAME_HEIGHT = 9.0
# The largest box edge must span at least this fraction of the camera distance.
# With the examples' 45 degree field of view that is ~12% of the frame height;
# the broken pixel-divided sizes were below 1%.
MIN_EXTENT_RATIO = 0.1


def _literal_tuple(node):
    try:
        value = ast.literal_eval(node)
    except ValueError:
        return None
    if isinstance(value, tuple) and all(isinstance(v, (int, float)) for v in value):
        return tuple(float(v) for v in value)
    return None


def _keyword(call, name):
    for keyword in call.keywords:
        if keyword.arg == name:
            return _literal_tuple(keyword.value)
    return None


def _scan(path):
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    sizes, distances = [], []
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call) or not isinstance(node.func, ast.Attribute):
            continue
        if node.func.attr == "cartesian_3d":
            size = _keyword(node, "size")
            if size is not None:
                sizes.append((node.lineno, size))
        elif node.func.attr == "look_at":
            eye, target = _keyword(node, "eye"), _keyword(node, "target")
            if eye is not None:
                target = target or (0.0, 0.0, 0.0)
                distances.append(math.dist(eye, target))
    return sizes, distances


class Cartesian3DExampleScaleTests(unittest.TestCase):
    def test_cartesian_3d_sizes_are_visible_from_the_camera(self):
        checked = 0
        for path in sorted((ROOT / "examples").rglob("*.py")):
            sizes, distances = _scan(path)
            # Judge against the farthest authored camera pose; without one the
            # logical frame height is the reference.
            reference = max(distances, default=FRAME_HEIGHT)
            for lineno, size in sizes:
                checked += 1
                with self.subTest(example=f"{path.relative_to(ROOT)}:{lineno}"):
                    self.assertEqual(len(size), 3)
                    self.assertGreaterEqual(
                        max(size) / reference,
                        MIN_EXTENT_RATIO,
                        f"cartesian_3d size={size} is tiny next to a camera "
                        f"{reference:.2f} units away; size is a world-space "
                        "extent, not pixels",
                    )
        self.assertGreater(checked, 0, "no cartesian_3d(size=...) examples found")

    def test_guard_rejects_pixel_divided_sizes(self):
        # The pre-fix axes_3d_demo values: size=(0.125, 0.125, 0.075) with
        # the camera at eye=(11, 8, 11) looking at the origin.
        self.assertLess(0.125 / math.dist((11, 8, 11), (0, 0, 0)), MIN_EXTENT_RATIO)


if __name__ == "__main__":
    unittest.main()
