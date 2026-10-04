"""Lit 3D meshes under an orbiting camera, for the runtime performance harness.

6 x scale spheres (about 1000 triangles each), as many cubes and a
subdivided floor, shaded and turning while the camera orbits, so every
frame projects, shades and sorts every triangle again. 3D draws through
Vello after the CPU projection in `gaanim_renderer::three_d`, so this
measures that path. Set GAANIM_BENCHMARK_SCALE to add meshes; pass it to
`tests/benchmark_runtime.py --scene examples/performance_3d.py`.
"""

import math
import os

from gaanim import BLUE, CORAL, CYAN, GOLD, GREEN, NAVY, PINK, Easing, Material3D, Scene


FPS = 30
FRAME_COUNT = max(30, int(os.environ.get("GAANIM_BENCHMARK_FRAMES", "300")))
DURATION = FRAME_COUNT / FPS
SCALE = max(1, int(os.environ.get("GAANIM_BENCHMARK_SCALE", "1")))
MESHES = 6 * SCALE

scene = Scene(frame=(16, 9), background=NAVY)
scene.geometry.lighting_3d("studio", intensity=1.0)
colors = (BLUE, GOLD, GREEN, CORAL, CYAN, PINK)

floor = scene.geometry.plane(
    16, 12, subdivisions=(24, 18), material=Material3D.matte("#5A6A9C")
).move_to_3d(0, -1.6, 0)
meshes = []
for index in range(MESHES):
    angle = math.tau * index / MESHES
    radius = 3.0 + 1.2 * (index % 2)
    x, z = radius * math.cos(angle), radius * math.sin(angle)
    color = colors[index % len(colors)]
    sphere = scene.geometry.sphere(
        0.7, segments=32, rings=16,
        material=Material3D.metal(color) if index % 2 else Material3D.matte(color),
    ).move_to_3d(x, -0.4, z)
    cube = scene.geometry.cube(0.7, material=Material3D.matte(color)).move_to_3d(x * 0.55, 0.6, z * 0.55)
    meshes.extend([sphere, cube])

scene.camera.perspective(fov_y=0.785, near=0.1, far=1000)
scene.camera.look_at(eye=(9, 6, 11), target=(0, -0.5, 0))

motion_duration = max(0.1, DURATION)
scene.play(
    [scene.camera.animate.orbit(delta_yaw=1.2, delta_pitch=0.1).duration(motion_duration).easing(Easing.SMOOTH)]
    + [
        mesh.animate.rotate_by_3d("y", 2.0 * (1 if index % 2 else -1)).duration(motion_duration)
        for index, mesh in enumerate(meshes)
    ]
)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scenario = os.environ.get("GAANIM_BENCHMARK_SCENARIO", "seek")
    if scenario == "preview":
        times = [min(index / FPS, DURATION) for index in range(FRAME_COUNT)]
    else:
        # Same coprime stride as performance_benchmark.py: random access
        # without a PRNG.
        times = [((index * 37) % FRAME_COUNT) / FPS for index in range(FRAME_COUNT)]
    scene.snapshots(snapshot_dir, times)
else:
    scene.render()
