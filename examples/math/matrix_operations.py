"""Matrix operations: exact sums, products, inverses, determinants and decompositions with SymPy.

Every operation of a matrix returns a ``MatrixDerivation`` whose ``result``
is a new matrix (or an equation, for a scalar) and whose ``animations()``
fill it in. They need the optional ``gaanim[algebra]`` extra, which installs
SymPy. Each label is the call that computes the row.
"""

import os

from gaanim import Matrix, Scene, Zones

WRITE = 0.8
CELL = 0.6
STAGGER = 0.1
HOLD = 0.8
# A 2 x 2 result fills in four entries, one STAGGER apart; a scalar is
# written at once.
MATRICES = WRITE + CELL + 3 * STAGGER + HOLD
SCALARS = WRITE + CELL + HOLD
# Entries are set larger than the theme's math size (0.44) so that each
# matrix reads at a glance; gaps, delimiters, symbols and scalar results grow
# to match. The results inherit the operands' options, cell_factory included.
SIZE = 0.66
GROWTH = SIZE / 0.44

scene = Scene()
accent = scene.canvas.color("accent")
A = [[1, 2], [3, 4]]
B = [[2, 0], [1, 2]]


def entry(value, row, column):
    """A cell_factory: the entry as Typst math of SIZE, a fraction written inline (3/2).

    Inline fractions keep the height of the other entries, which the
    delimiters are sized for.
    """
    return scene.text.equation(str(value).replace("/", "\\/"), size=SIZE)


def matrix(data):
    """A 2 x 2 scene.viz.matrix with entries of SIZE and its gaps and delimiters scaled to them."""
    return scene.viz.matrix(
        data,
        cell_factory=entry,
        row_gap=0.24 * GROWTH,
        column_gap=0.3 * GROWTH,
        delimiter_gap=0.12 * GROWTH,
        delimiter_size=1.2 * GROWTH,
    )


def symbol(math):
    return scene.text.equation(math, size=SIZE)


def table(rows, tracks=("1fr", "2fr")):
    """Lay out one row per (label, build), the label on the left.

    ``build()`` returns the operands, the derivation and the symbol between
    them and the result. Each row writes its operands, then plays the
    derivation's animations.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=len(rows), columns=list(tracks), column_gap="48px", padding="page_padding")
    )
    operands, solutions = [], []
    for index, (label, build) in enumerate(rows):
        scene.text(label, role="code", markup=False).place(grid[2 * index], anchor="right")
        items, derivation = build()
        results = derivation.result if isinstance(derivation.result, tuple) else (derivation.result,)
        for result in results:
            if not isinstance(result, Matrix):
                result.scale_to(GROWTH)
        # A matrix is laid out through its drawable, the root of its grid.
        shown = [getattr(item, "drawable", item) for item in [*items, *results]]
        grid[2 * index + 1].arrange(*shown, gap="24px", justify="start")
        operands.extend(getattr(item, "drawable", item) for item in items)
        solutions.extend(derivation.animations(stagger=STAGGER))
    scene.play([operand.animate.write().duration(WRITE) for operand in operands])
    scene.play([anim.duration(CELL) for anim in solutions])
    scene.wait(HOLD)


def binary(operation, sign):
    """Operands a, sign, b and "=", with a.<operation>(b)."""
    a, b = matrix(A), matrix(B)
    return [a, symbol(sign), b, symbol("=")], getattr(a, operation)(b)


def unary(compute, data=A):
    """Operand a and an arrow, with compute(a)."""
    a = matrix(data)
    return [a, symbol("arrow.r")], compute(a)


# Entry by entry: sum, difference, Hadamard product and a scalar multiple.
scene.segment("elementwise")
table([
    ("a.add(b)", lambda: binary("add", "+")),
    ("a.subtract(b)", lambda: binary("subtract", "-")),
    ("a.hadamard(b)", lambda: binary("hadamard", "dot.o")),
    ("a.scalar_multiply(2)", lambda: unary(lambda a: a.scalar_multiply(2))),
])


def product():
    """a b, with the row and column that make the first entry in the accent."""
    items, derivation = binary("matmul", "dot")
    a, _, b, _ = items
    a.row(0).fill(accent)
    b.column(0).fill(accent)
    derivation.result[0, 0].fill(accent)
    return items, derivation


# Products, transposes and the reduced row echelon form...
scene.segment("products")
table([
    ("a.matmul(b)", product),
    ("a.transpose()", lambda: unary(lambda a: a.transpose())),
    ("a.rref()", lambda: unary(lambda a: a.rref())),
])

# ...are exact: an inverse keeps its fractions unless exact=False evaluates
# it to a precision.
scene.segment("inverses")
table(
    [
        ("a.inverse()", lambda: unary(lambda a: a.inverse())),
        ("a.inverse(exact=False,\nprecision=3)", lambda: unary(lambda a: a.inverse(exact=False, precision=3))),
    ],
    tracks=("5fr", "9fr"),
)

# Scalar results are equations.
scene.segment("scalars")
table([
    ("a.determinant()", lambda: unary(lambda a: a.determinant())),
    ("a.trace()", lambda: unary(lambda a: a.trace())),
])

# Decompositions return two matrices: L and U, Q and R, eigenvalues and
# eigenvectors.
scene.segment("decompositions")
table(
    [
        ("a.lu()", lambda: unary(lambda a: a.lu())),
        ("a.qr()", lambda: unary(lambda a: a.qr(), [[3, 0], [4, 5]])),
        ("a.eigen()", lambda: unary(lambda a: a.eigen(), [[2, 1], [1, 2]])),
    ],
    tracks=("1fr", "4fr"),
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid-way and settled, in each segment.
    scene.snapshots(snapshots, [
        WRITE + 0.4, MATRICES - 0.1,
        MATRICES + WRITE + 0.4, 2 * MATRICES - 0.1,
        2 * MATRICES + WRITE + 0.4, 3 * MATRICES - 0.1,
        3 * MATRICES + WRITE + 0.3, 3 * MATRICES + SCALARS - 0.1,
        3 * MATRICES + SCALARS + WRITE + 0.4, 4 * MATRICES + SCALARS - 0.1,
    ])
else:
    scene.render()
