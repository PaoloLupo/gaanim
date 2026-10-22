"""Generate the glTF models the morph target and normal map examples load.

Writes, into ``examples/3d/assets`` (and ``Blob.glb`` into
``docs/fixtures/assets`` too, for the reference's snippets):

- ``Blob.glb``: a ball 2 units across about its origin, with three morph
  targets, ``Squash``, ``Spikes`` and ``Lean``, and two animations:
  ``Bounce``, which hops it and squashes it where it lands, and ``Wobble``,
  which grows its spikes and sways it.
- ``Bricks.glb``: a brick panel in two scenes, ``Plain`` and ``NormalMap``,
  both with the same color texture, the second with a normal map baked
  from the bricks' height and no tangents, so that the importer computes
  them.

Run it from the repository root with ``python tools/generate_gltf_samples.py``.
It needs only the standard library.
"""

import json
import math
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "examples" / "3d" / "assets"
DOCS_ASSETS = ROOT / "docs" / "fixtures" / "assets"

FLOAT = 5126
UNSIGNED_SHORT = 5123
ARRAY_BUFFER = 34962
ELEMENT_ARRAY_BUFFER = 34963


class Glb:
    """A glTF document and its binary buffer, written as one ``.glb``."""

    def __init__(self):
        self.doc = {
            "asset": {"version": "2.0", "generator": "gaanim tools/generate_gltf_samples.py"},
            "scene": 0,
            "scenes": [{"nodes": []}],
            "nodes": [],
            "meshes": [],
            "materials": [],
            "accessors": [],
            "bufferViews": [],
            "buffers": [{"byteLength": 0}],
        }
        self.bin = bytearray()

    def view(self, data, target=None):
        """Append ``data``, 4-byte aligned, as a buffer view; its index."""
        while len(self.bin) % 4:
            self.bin.append(0)
        view = {"buffer": 0, "byteOffset": len(self.bin), "byteLength": len(data)}
        if target is not None:
            view["target"] = target
        self.bin.extend(data)
        self.doc["bufferViews"].append(view)
        return len(self.doc["bufferViews"]) - 1

    def accessor(self, values, kind, component=FLOAT, target=None, bounds=False):
        """An accessor of ``values``, tuples of ``kind`` (``VEC3``…) or numbers."""
        width = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[kind]
        flat = [v for value in values for v in (value if width > 1 else (value,))]
        code = "f" if component == FLOAT else "H"
        accessor = {
            "bufferView": self.view(struct.pack(f"<{len(flat)}{code}", *flat), target),
            "componentType": component,
            "count": len(values),
            "type": kind,
        }
        if bounds:
            columns = [[value[i] if width > 1 else value for value in values] for i in range(width)]
            accessor["min"] = [min(column) for column in columns]
            accessor["max"] = [max(column) for column in columns]
        self.doc["accessors"].append(accessor)
        return len(self.doc["accessors"]) - 1

    def image(self, png):
        """A texture of the PNG bytes ``png``; its index."""
        self.doc.setdefault("images", []).append(
            {"bufferView": self.view(png), "mimeType": "image/png"}
        )
        self.doc.setdefault("textures", []).append({"source": len(self.doc["images"]) - 1})
        return len(self.doc["textures"]) - 1

    def write(self, path):
        self.doc["buffers"][0]["byteLength"] = len(self.bin)
        while len(self.bin) % 4:
            self.bin.append(0)
        text = json.dumps(self.doc, separators=(",", ":")).encode()
        text += b" " * (-len(text) % 4)
        chunks = (
            struct.pack("<II", len(text), 0x4E4F534A) + text
            + struct.pack("<II", len(self.bin), 0x004E4942) + bytes(self.bin)
        )
        path.write_bytes(struct.pack("<III", 0x46546C67, 2, 12 + len(chunks)) + chunks)


def png(width, height, pixel):
    """PNG bytes of an RGB image whose pixel at column ``x``, row ``y`` is ``pixel(x, y)``."""
    rows = b"".join(
        b"\x00" + bytes(channel for x in range(width) for channel in pixel(x, y))
        for y in range(height)
    )

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")
    )


def sub(a, b):
    return tuple(x - y for x, y in zip(a, b))


def normalized(v):
    length = math.sqrt(sum(x * x for x in v)) or 1.0
    return tuple(x / length for x in v)


def smooth_normals(positions, indices):
    """The normals of a mesh: its faces' about each place, so seams stay smooth."""
    place = {}
    sums = []
    of = []
    for p in positions:
        key = tuple(round(x, 5) for x in p)
        if key not in place:
            place[key] = len(sums)
            sums.append([0.0, 0.0, 0.0])
        of.append(place[key])
    for a, b, c in zip(indices[0::3], indices[1::3], indices[2::3]):
        u, v = sub(positions[b], positions[a]), sub(positions[c], positions[a])
        face = (u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0])
        for corner in (a, b, c):
            for i in range(3):
                sums[of[corner]][i] += face[i]
    return [normalized(sums[of[index]]) for index in range(len(positions))]


def icosphere(subdivisions):
    """A unit sphere of triangles: an icosahedron whose faces are split in
    four ``subdivisions`` times, its corners pushed out to the sphere. Its
    12 first vertices are the icosahedron's corners."""
    golden = (1 + math.sqrt(5)) / 2
    positions = [normalized(v) for v in [
        (-1, golden, 0), (1, golden, 0), (-1, -golden, 0), (1, -golden, 0),
        (0, -1, golden), (0, 1, golden), (0, -1, -golden), (0, 1, -golden),
        (golden, 0, -1), (golden, 0, 1), (-golden, 0, -1), (-golden, 0, 1),
    ]]
    faces = [
        (0, 11, 5), (0, 5, 1), (0, 1, 7), (0, 7, 10), (0, 10, 11),
        (1, 5, 9), (5, 11, 4), (11, 10, 2), (10, 7, 6), (7, 1, 8),
        (3, 9, 4), (3, 4, 2), (3, 2, 6), (3, 6, 8), (3, 8, 9),
        (4, 9, 5), (2, 4, 11), (6, 2, 10), (8, 6, 7), (9, 8, 1),
    ]
    for _ in range(subdivisions):
        middles = {}

        def middle(a, b):
            key = (min(a, b), max(a, b))
            if key not in middles:
                middles[key] = len(positions)
                positions.append(normalized(tuple((x + y) / 2 for x, y in zip(positions[a], positions[b]))))
            return middles[key]

        split = []
        for a, b, c in faces:
            ab, bc, ca = middle(a, b), middle(b, c), middle(c, a)
            split += [(a, ab, ca), (b, bc, ab), (c, ca, bc), (ab, bc, ca)]
        faces = split
    return positions, [index for face in faces for index in face]


def blob():
    """The ball, its targets and its animations."""
    # A ball 2 units across about its origin, its spikes on the
    # icosahedron's corners, which are vertices of the mesh.
    positions, indices = icosphere(4)
    normals = list(positions)
    spikes = positions[:12]

    def squash(p):
        # Wider and flatter, its bottom where it was.
        return (p[0] * 1.3, (p[1] + 1) * 0.6 - 1, p[2] * 1.3)

    def spiky(p):
        reach = 1 + 0.6 * sum(max(0.0, sum(a * b for a, b in zip(p, s))) ** 24 for s in spikes)
        return tuple(x * reach for x in p)

    def lean(p):
        # Its top toward +x, its bottom where it was.
        return (p[0] + 0.7 * ((p[1] + 1) / 2) ** 2, p[1], p[2])

    targets = []
    for shape in (squash, spiky, lean):
        moved = [shape(p) for p in positions]
        turned = smooth_normals(moved, indices)
        targets.append((
            [sub(m, p) for m, p in zip(moved, positions)],
            [sub(t, n) for t, n in zip(turned, normals)],
        ))

    glb = Glb()
    attributes = {
        "POSITION": glb.accessor(positions, "VEC3", target=ARRAY_BUFFER, bounds=True),
        "NORMAL": glb.accessor(normals, "VEC3", target=ARRAY_BUFFER),
    }
    primitive = {
        "attributes": attributes,
        "indices": glb.accessor(indices, "SCALAR", UNSIGNED_SHORT, ELEMENT_ARRAY_BUFFER),
        "material": 0,
        "targets": [
            {
                "POSITION": glb.accessor(moved, "VEC3", target=ARRAY_BUFFER, bounds=True),
                "NORMAL": glb.accessor(turned, "VEC3", target=ARRAY_BUFFER),
            }
            for moved, turned in targets
        ],
    }
    glb.doc["materials"].append({
        "name": "Clay",
        "pbrMetallicRoughness": {
            "baseColorFactor": [0.92, 0.36, 0.22, 1.0],
            "metallicFactor": 0.0,
            "roughnessFactor": 0.45,
        },
    })
    glb.doc["meshes"].append({
        "name": "Blob",
        "primitives": [primitive],
        "weights": [0.0, 0.0, 0.0],
        "extras": {"targetNames": ["Squash", "Spikes", "Lean"]},
    })
    glb.doc["nodes"].append({"name": "Blob", "mesh": 0})
    glb.doc["scenes"][0]["nodes"].append(0)

    def animation(name, duration, keys, at):
        """An animation of ``keys + 1`` keyframes over ``duration`` seconds;
        ``at(t)`` gives the height and the three weights at ``t``."""
        times = [duration * key / keys for key in range(keys + 1)]
        values = [at(t) for t in times]
        heights = [(0.0, height, 0.0) for height, *_ in values]
        weights = [w for _, *each in values for w in each]
        input_ = glb.accessor(times, "SCALAR", bounds=True)
        samplers = [
            {"input": input_, "output": glb.accessor(heights, "VEC3"), "interpolation": "LINEAR"},
            {"input": input_, "output": glb.accessor(weights, "SCALAR"), "interpolation": "LINEAR"},
        ]
        glb.doc.setdefault("animations", []).append({
            "name": name,
            "samplers": samplers,
            "channels": [
                {"sampler": 0, "target": {"node": 0, "path": "translation"}},
                {"sampler": 1, "target": {"node": 0, "path": "weights"}},
            ],
        })

    def bounce(t):
        # On the ground, where it rests, around 0 and 1.2 s, squashed most
        # as it lands; stretched as it leaves and lands, round at the top
        # of its hop.
        period, ground = 1.2, 0.15
        t %= period
        if t < ground or t > period - ground:
            since = min(t, period - t) / ground
            return (0.0, 0.6 - 0.85 * math.sin(since * math.pi / 2), 0.0, 0.0)
        s = (t - ground) / (period - 2 * ground)
        return (1.1 * 4 * s * (1 - s), -0.25 * (1 - 4 * s * (1 - s)), 0.0, 0.0)

    def wobble(t):
        s = t / 2.0
        return (0.0, 0.0, math.sin(math.pi * s) ** 2, 0.7 * math.sin(2 * math.pi * s))

    animation("Bounce", 1.2, 48, bounce)
    animation("Wobble", 2.0, 40, wobble)
    glb.write(ASSETS / "Blob.glb")


def bricks(size):
    """A brick wall on a ``size``-texel square image: the height of each
    texel, from 0 in the mortar to 1 on the bricks, and its color."""
    rows, per_row, mortar, bevel = 6, 3, 0.035, 0.05
    tones = [(168, 82, 58), (184, 96, 66), (152, 74, 54), (176, 104, 74), (160, 88, 62)]
    grout = (196, 188, 172)

    def ease(x):
        x = min(max(x, 0.0), 1.0)
        return x * x * (3 - 2 * x)

    def texel(u, v):
        row = int(v * rows)
        shift = 0.5 / per_row if row % 2 else 0.0
        x = ((u + shift) * per_row) % 1.0
        y = (v * rows) % 1.0
        column = int((u + shift) * per_row) % per_row
        # How far into the brick, in the image's units, from its nearest edge.
        inside = min(x / per_row, (1 - x) / per_row, y / rows, (1 - y) / rows) - mortar / 2
        # A gentle swell across each brick, which catches the light too.
        swell = 0.06 * math.sin(math.pi * x) * math.sin(math.pi * y)
        height = ease(inside / bevel) * (1.0 + swell)
        tone = tones[(row * 7 + column * 3) % len(tones)]
        share = ease(inside / (bevel * 0.4))
        color = tuple(round(g + (t - g) * share) for g, t in zip(grout, tone))
        return height, color

    texels = [[texel((x + 0.5) / size, (y + 0.5) / size) for x in range(size)] for y in range(size)]
    return [[h for h, _ in row] for row in texels], [[c for _, c in row] for row in texels]


def bricks_panel():
    """A brick panel, with a normal map in one scene and without in the other."""
    size, strength = 256, 6.0
    heights, colors = bricks(size)

    def normal(x, y):
        # Slopes along the image's columns and up its rows: glTF's normal
        # maps point y up the image.
        left, right = heights[y][max(x - 1, 0)], heights[y][min(x + 1, size - 1)]
        above, below = heights[max(y - 1, 0)][x], heights[min(y + 1, size - 1)][x]
        dx, up = (right - left) / 2, (above - below) / 2
        n = normalized((-strength * dx, -strength * up, 1.0))
        return tuple(round((c * 0.5 + 0.5) * 255) for c in n)

    glb = Glb()
    color = glb.image(png(size, size, lambda x, y: colors[y][x]))
    texture = glb.image(png(size, size, normal))
    for name, mapped in (("Plain", False), ("NormalMap", True)):
        material = {
            "name": f"{name}Bricks",
            "pbrMetallicRoughness": {
                "baseColorTexture": {"index": color},
                "metallicFactor": 0.0,
                "roughnessFactor": 0.7,
            },
        }
        if mapped:
            material["normalTexture"] = {"index": texture, "scale": 1.0}
        glb.doc["materials"].append(material)
    # A square two units on a side facing +z, its UVs with v down, centered
    # on the origin of each scene.
    positions = [(-1.0, -1.0, 0.0), (1.0, -1.0, 0.0), (1.0, 1.0, 0.0), (-1.0, 1.0, 0.0)]
    attributes = {
        "POSITION": glb.accessor(positions, "VEC3", target=ARRAY_BUFFER, bounds=True),
        "NORMAL": glb.accessor([(0.0, 0.0, 1.0)] * 4, "VEC3", target=ARRAY_BUFFER),
        "TEXCOORD_0": glb.accessor(
            [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)], "VEC2", target=ARRAY_BUFFER
        ),
    }
    indices = glb.accessor([0, 1, 2, 0, 2, 3], "SCALAR", UNSIGNED_SHORT, ELEMENT_ARRAY_BUFFER)
    glb.doc["scenes"] = []
    for index, name in enumerate(("Plain", "NormalMap")):
        glb.doc["meshes"].append({
            "name": name,
            "primitives": [{"attributes": attributes, "indices": indices, "material": index}],
        })
        glb.doc["nodes"].append({"name": name, "mesh": index})
        glb.doc["scenes"].append({"name": name, "nodes": [index]})
    glb.write(ASSETS / "Bricks.glb")


if __name__ == "__main__":
    ASSETS.mkdir(parents=True, exist_ok=True)
    blob()
    bricks_panel()
    (DOCS_ASSETS / "Blob.glb").write_bytes((ASSETS / "Blob.glb").read_bytes())
    for name in ("Blob.glb", "Bricks.glb"):
        print(f"{ASSETS / name}: {(ASSETS / name).stat().st_size} bytes")
