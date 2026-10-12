"""Structured, selectable, and algebra-aware matrices for Gaanim."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable, Iterable, Iterator, Mapping, Sequence

from .gaanim_core import Anim, Drawable, Easing, MatrixOrder, Text


@dataclass(frozen=True)
class MatrixEntry:
    """One matrix value with an optional stable morph key and local style."""

    value: Any
    key: str | None = None
    style: Any | None = None


@dataclass(frozen=True)
class MatrixStep:
    """Semantic step emitted by an algebraic matrix derivation."""

    kind: str
    sources: tuple[tuple[int, int], ...] = ()
    target: tuple[int, int] | None = None
    expression: str | None = None


class MatrixAlgebraError(ValueError):
    """Raised when exact symbolic matrix algebra cannot be completed."""


class MatrixSelectionAnimation(Sequence[Anim]):
    """Compound property animation over a matrix selection."""

    def __init__(self, selection: "MatrixSelection"):
        self._selection = selection
        self._animations: list[Anim] = []

    @classmethod
    def _from_animations(cls, animations: Iterable[Anim]) -> "MatrixSelectionAnimation":
        instance = cls.__new__(cls)
        instance._selection = None
        instance._animations = list(animations)
        return instance

    def _targets(self, order: Any, stagger: float, seed: int) -> list[Anim]:
        stagger = _non_negative(stagger, "stagger")
        # Without an order, a stagger runs row by row; "simultaneous" entries
        # start together whatever the stagger.
        if order is None: order = "row_major" if stagger else "simultaneous"
        if isinstance(order, str) and order == "simultaneous": stagger = 0.0
        return [cell.animate.delay(index * stagger) for index, (_, cell) in enumerate(self._selection._ordered(order, seed))]

    def _apply(self, name: str, *args: Any) -> "MatrixSelectionAnimation":
        self._animations = [getattr(animation, name)(*args) for animation in self._animations]
        return self

    def _action(self, name: str, *args: Any, order: Any, stagger: float, seed: int) -> "MatrixSelectionAnimation":
        self._animations = [getattr(animation, name)(*args) for animation in self._targets(order, stagger, seed)]
        return self

    def fill(self, color: Any, *, order: Any = None, stagger: float = 0.0, seed: int = 0) -> "MatrixSelectionAnimation": return self._action("fill", color, order=order, stagger=stagger, seed=seed)
    def opacity(self, value: float, *, order: Any = None, stagger: float = 0.0, seed: int = 0) -> "MatrixSelectionAnimation": return self._action("opacity", value, order=order, stagger=stagger, seed=seed)
    def scale_by(self, factor: float, *, order: Any = None, stagger: float = 0.0, seed: int = 0) -> "MatrixSelectionAnimation": return self._action("scale_by", factor, order=order, stagger=stagger, seed=seed)
    def rotate_by(self, radians: float, *, order: Any = None, stagger: float = 0.0, seed: int = 0) -> "MatrixSelectionAnimation": return self._action("rotate_by", radians, order=order, stagger=stagger, seed=seed)
    def write(self, *, order: Any = "row_major", stagger: float = 0.05, seed: int = 0) -> "MatrixSelectionAnimation": return self._action("write", order=order, stagger=stagger, seed=seed)
    def create(self, *, order: Any = "row_major", stagger: float = 0.05, seed: int = 0) -> "MatrixSelectionAnimation": return self._action("create", order=order, stagger=stagger, seed=seed)
    def fade_in(self, *, order: Any = None, stagger: float = 0.0, seed: int = 0) -> "MatrixSelectionAnimation": return self._action("fade_in", order=order, stagger=stagger, seed=seed)
    def fade_out(self, *, order: Any = None, stagger: float = 0.0, seed: int = 0) -> "MatrixSelectionAnimation": return self._action("fade_out", order=order, stagger=stagger, seed=seed)
    def indicate(self, *, order: Any = None, stagger: float = 0.0, seed: int = 0) -> "MatrixSelectionAnimation": return self._action("indicate", order=order, stagger=stagger, seed=seed)
    def wiggle(self, *, order: Any = None, stagger: float = 0.0, seed: int = 0) -> "MatrixSelectionAnimation": return self._action("wiggle", order=order, stagger=stagger, seed=seed)
    def duration(self, seconds: float) -> "MatrixSelectionAnimation": return self._apply("duration", seconds)
    def easing(self, easing: Easing) -> "MatrixSelectionAnimation": return self._apply("easing", easing)

    def __len__(self) -> int: return len(self._animations)
    def __getitem__(self, index: int | slice) -> Anim | list[Anim]: return self._animations[index]
    def __iter__(self) -> Iterator[Anim]: return iter(self._animations)


class MatrixSelection(Sequence[Drawable]):
    """Stable snapshot of matrix cells that can be styled or animated together."""

    def __init__(self, matrix: "Matrix", entries: Iterable[tuple[tuple[int, int], Drawable]]):
        self._matrix = matrix
        self._entries = tuple(entries)

    @property
    def coordinates(self) -> tuple[tuple[int, int], ...]: return tuple(index for index, _ in self._entries)
    @property
    def values(self) -> tuple[Any, ...]: return tuple(self._matrix._values[row][column] for (row, column), _ in self._entries)

    def __len__(self) -> int: return len(self._entries)
    def __getitem__(self, index: int | slice) -> Drawable | "MatrixSelection":
        value = self._entries[index]
        return MatrixSelection(self._matrix, value) if isinstance(index, slice) else value[1]
    def __iter__(self) -> Iterator[Drawable]: return (cell for _, cell in self._entries)

    def fill(self, color: Any) -> "MatrixSelection":
        for cell in self: cell.fill(color)
        return self

    def opacity(self, value: float) -> "MatrixSelection":
        for cell in self: cell.opacity(value)
        return self

    @property
    def animate(self) -> MatrixSelectionAnimation:
        return MatrixSelectionAnimation(self)

    def offset(self, dx: float, dy: float) -> "MatrixSelection":
        if not all(isinstance(value, (int, float)) and float(value) == float(value) for value in (dx, dy)):
            raise ValueError("matrix selection offset must be finite")
        for position, (_, cell) in enumerate(self._entries):
            cell.item(offset=(float(dx), float(dy)))
        return self

    def _ordered(self, order: Any, seed: int) -> list[tuple[tuple[int, int], Drawable]]:
        entries = list(self._entries)
        if callable(order):
            return sorted(entries, key=lambda item: order(*item[0]))
        if isinstance(order, Sequence) and not isinstance(order, (str, bytes)):
            positions = {tuple(coordinate): index for index, coordinate in enumerate(order)}
            if set(positions) != set(self.coordinates):
                raise ValueError("explicit matrix order must contain every selected coordinate exactly once")
            return sorted(entries, key=lambda item: positions[item[0]])
        name = str(order)
        if name == "simultaneous": return entries
        ordered = MatrixOrder.order(self._matrix.nrows, self._matrix.ncols, self.coordinates, name, seed)
        lookup = dict(entries)
        return [(coordinate, lookup[coordinate]) for coordinate in ordered]


@dataclass
class _MatrixLayout:
    """The contents of a matrix's grid: its values, drawables and tracks."""

    values: list[list[Any]]
    cells: list[list[Drawable]]
    delimiters: tuple[Drawable, ...]
    row_labels: tuple[Drawable, ...]
    column_labels: tuple[Drawable, ...]
    items: list[Drawable]
    tracks: dict[str, Any]
    options: dict[str, Any]


class Matrix:
    """Persistent matrix composed from real Gaanim drawables and a grid box.

    Edits change the grid's children in place, so the grid keeps the
    matrix's place, transform and animations.
    """

    def __init__(self, scene: Any, grid: Any, layout: _MatrixLayout):
        self._scene, self._grid, self._root = scene, grid, grid
        self._adopt(layout)

    def _adopt(self, layout: _MatrixLayout) -> None:
        self._values, self._cells = layout.values, layout.cells
        self._delimiters, self._row_labels, self._column_labels = layout.delimiters, layout.row_labels, layout.column_labels
        self._options = layout.options

    @property
    def nrows(self) -> int: return len(self._cells)
    @property
    def ncols(self) -> int: return len(self._cells[0])
    @property
    def shape(self) -> tuple[int, int]: return self.nrows, self.ncols
    @property
    def entries(self) -> MatrixSelection:
        return MatrixSelection(self, (((row, column), self._cells[row][column]) for row in range(self.nrows) for column in range(self.ncols)))
    @property
    def delimiters(self) -> tuple[Drawable, ...]: return self._delimiters
    @property
    def row_labels(self) -> tuple[Drawable, ...]: return self._row_labels
    @property
    def column_labels(self) -> tuple[Drawable, ...]: return self._column_labels
    @property
    def drawable(self) -> Drawable: return self._root

    def __getattr__(self, name: str) -> Any:
        if name.startswith("_"): raise AttributeError(name)
        value = getattr(self._root, name)
        if not callable(value): return value

        def fluent(*args: Any, **kwargs: Any) -> Any:
            result = value(*args, **kwargs)
            # A fluent setter returns the root box; chain on the Matrix instead.
            return self if result is self._root else result
        return fluent
    def move_to(self, *args: Any, **kwargs: Any) -> "Matrix": self._root.move_to(*args, **kwargs); return self
    def fill(self, color: Any) -> "Matrix": self._root.fill(color); return self
    def opacity(self, value: float) -> "Matrix": self._root.opacity(value); return self
    def scale_to(self, factor: float) -> "Matrix": self._root.scale_to(factor); return self
    def scale_by(self, factor: float) -> "Matrix": self._root.scale_by(factor); return self

    def __getitem__(self, key: tuple[Any, Any]) -> Drawable | MatrixSelection:
        if not isinstance(key, tuple) or len(key) != 2:
            raise TypeError("matrix indices must be matrix[row, column]")
        rows, row_scalar = _indices(key[0], self.nrows)
        columns, column_scalar = _indices(key[1], self.ncols)
        if row_scalar and column_scalar: return self._cells[rows[0]][columns[0]]
        return MatrixSelection(self, ((((row, column), self._cells[row][column])) for row in rows for column in columns))

    def row(self, index: int) -> MatrixSelection: return self[index, :]
    def column(self, index: int) -> MatrixSelection: return self[:, index]
    def block(self, rows: Any, columns: Any) -> MatrixSelection: return self[rows, columns]
    def diagonal(self, offset: int = 0) -> MatrixSelection:
        entries = []
        for row in range(self.nrows):
            column = row + offset
            if 0 <= column < self.ncols: entries.append(((row, column), self._cells[row][column]))
        return MatrixSelection(self, entries)
    def anti_diagonal(self, offset: int = 0) -> MatrixSelection:
        entries = []
        for row in range(self.nrows):
            column = self.ncols - 1 - row + offset
            if 0 <= column < self.ncols: entries.append(((row, column), self._cells[row][column]))
        return MatrixSelection(self, entries)
    def where(self, mask_or_predicate: Any) -> MatrixSelection:
        if callable(mask_or_predicate):
            keep = lambda row, column: bool(mask_or_predicate(self._values[row][column], row, column))
        else:
            mask = [list(row) for row in mask_or_predicate]
            _validate_rectangular(mask, expected=self.shape)
            keep = lambda row, column: bool(mask[row][column])
        return MatrixSelection(self, (((row, column), self._cells[row][column]) for row in range(self.nrows) for column in range(self.ncols) if keep(row, column)))

    def set(self, row: int, column: int, value: Any) -> Drawable:
        row = _normalize_index(row, self.nrows); column = _normalize_index(column, self.ncols)
        entry, cell = _make_cell(self._scene, value, row, column, self._options)
        old = self._cells[row][column]
        replacement = self._grid.replace(old, cell)
        self._values[row][column], self._cells[row][column] = entry, replacement
        return replacement

    def become(self, data: Any) -> "Matrix":
        rows = _matrix_rows(data)
        options = dict(self._options)
        if options.get("row_labels") is not None and len(options["row_labels"]) != len(rows): options.pop("row_labels")
        if rows and options.get("column_labels") is not None and len(options["column_labels"]) != len(rows[0]): options.pop("column_labels")
        explicit = options.get("delimiter_size")
        if explicit is not None and len(rows) != self.nrows:
            # An explicit size spans the rows it was given for: it grows and
            # shrinks with them as the automatic size does.
            options["delimiter_size"] = float(explicit) * _auto_delimiter_size(len(rows), options) / _auto_delimiter_size(self.nrows, options)
        layout = _matrix_layout(self._scene, rows, options)
        # The grid keeps its place, transform and animations; its children
        # and tracks change at the cursor.
        for child in (*(cell for row in self._cells for cell in row), *self._row_labels, *self._column_labels, *self._delimiters):
            self._grid.remove(child)
        self._grid.set(**layout.tracks)
        for item in layout.items:
            self._grid.add(item)
        self._adopt(layout)
        return self

    def insert_row(self, index: int, values: Sequence[Any]) -> "Matrix":
        data = [row[:] for row in self._values]; data.insert(index, list(values)); return self.become(data)
    def remove_row(self, index: int) -> "Matrix":
        data = [row[:] for row in self._values]; data.pop(_normalize_index(index, self.nrows)); return self.become(data)
    def insert_column(self, index: int, values: Sequence[Any]) -> "Matrix":
        if len(values) != self.nrows: raise ValueError("inserted column length must match matrix rows")
        data = [row[:] for row in self._values]
        for row, value in zip(data, values): row.insert(index, value)
        return self.become(data)
    def remove_column(self, index: int) -> "Matrix":
        column = _normalize_index(index, self.ncols); data = [row[:] for row in self._values]
        for row in data: row.pop(column)
        return self.become(data)
    def swap_rows(self, first: int, second: int) -> "Matrix":
        data = [row[:] for row in self._values]; first = _normalize_index(first, self.nrows); second = _normalize_index(second, self.nrows); data[first], data[second] = data[second], data[first]; return self.become(data)
    def swap_columns(self, first: int, second: int) -> "Matrix":
        first = _normalize_index(first, self.ncols); second = _normalize_index(second, self.ncols); data = [row[:] for row in self._values]
        for row in data: row[first], row[second] = row[second], row[first]
        return self.become(data)
    def reorder_rows(self, order: Sequence[int]) -> "Matrix":
        normalized = [_normalize_index(index, self.nrows) for index in order]
        if sorted(normalized) != list(range(self.nrows)): raise ValueError("row order must be a permutation")
        return self.become([self._values[index][:] for index in normalized])
    def reorder_columns(self, order: Sequence[int]) -> "Matrix":
        normalized = [_normalize_index(index, self.ncols) for index in order]
        if sorted(normalized) != list(range(self.ncols)): raise ValueError("column order must be a permutation")
        return self.become([[row[index] for index in normalized] for row in self._values])

    def morph_to(self, target: "Matrix", *, match: str = "auto", stagger: float = 0.0) -> MatrixSelectionAnimation:
        if match not in {"auto", "key", "value", "position"}: raise ValueError("match must be auto, key, value, or position")
        stagger = _non_negative(stagger, "stagger")
        pairs, source_only, target_only = _match_entries(self, target, match)
        # The morph ends on the target: it shows as the morph starts, each
        # paired entry hands over to its target entry where that one stands,
        # and the target's unpaired entries fade in.
        animations = [target._root.animate.opacity(1.0)]
        animations.extend(source.animate.replacement_transform_to(destination).delay(index * stagger) for index, (source, destination) in enumerate(pairs))
        animations.extend(cell.animate.fade_out() for cell in source_only)
        animations.extend(cell.animate.fade_in() for cell in target_only)
        for sources, targets in ((self._delimiters, target._delimiters), (self._row_labels, target._row_labels), (self._column_labels, target._column_labels)):
            if len(sources) == len(targets):
                animations.extend(source.animate.replacement_transform_to(destination) for source, destination in zip(sources, targets))
            else:
                animations.extend(source.animate.fade_out() for source in sources)
                animations.extend(destination.animate.fade_in() for destination in targets)
        return MatrixSelectionAnimation._from_animations(animations)

    def to_sympy(self) -> Any: return _sympy().Matrix([[_entry_value(value) for value in row] for row in self._values])
    def add(self, other: "Matrix", **options: Any) -> "MatrixDerivation": return _derive_binary(self, other, "add", **options)
    def subtract(self, other: "Matrix", **options: Any) -> "MatrixDerivation": return _derive_binary(self, other, "subtract", **options)
    def matmul(self, other: "Matrix", **options: Any) -> "MatrixDerivation": return _derive_binary(self, other, "matmul", **options)
    def hadamard(self, other: "Matrix", **options: Any) -> "MatrixDerivation": return _derive_binary(self, other, "hadamard", **options)
    def scalar_multiply(self, scalar: Any, **options: Any) -> "MatrixDerivation": return _derive_unary(self, "scale", scalar=scalar, **options)
    def transpose(self, **options: Any) -> "MatrixDerivation": return _derive_unary(self, "transpose", **options)
    def determinant(self, **options: Any) -> "MatrixDerivation": return _derive_unary(self, "determinant", **options)
    def inverse(self, **options: Any) -> "MatrixDerivation": return _derive_unary(self, "inverse", **options)
    def rank(self, **options: Any) -> "MatrixDerivation": return _derive_unary(self, "rank", **options)
    def trace(self, **options: Any) -> "MatrixDerivation": return _derive_unary(self, "trace", **options)
    def rref(self, **options: Any) -> "MatrixDerivation": return _derive_unary(self, "rref", **options)
    def lu(self, **options: Any) -> "MatrixDerivation": return _derive_unary(self, "lu", **options)
    def qr(self, **options: Any) -> "MatrixDerivation": return _derive_unary(self, "qr", **options)
    def eigen(self, **options: Any) -> "MatrixDerivation": return _derive_unary(self, "eigen", **options)


class MatrixDerivation:
    """Exact algebra value, visual result, and adaptive animation metadata."""

    def __init__(self, value: Any, result: Any, steps: Sequence[MatrixStep]):
        self.value, self.result, self.steps = value, result, tuple(steps)

    def animations(self, *, order: Any = "row_major", stagger: float = 0.04) -> MatrixSelectionAnimation:
        results = self.result if isinstance(self.result, tuple) else (self.result,)
        animations: list[Anim] = []
        for result in results:
            if isinstance(result, Matrix): animations.extend(result.entries.animate.fade_in(order=order, stagger=stagger))
            else: animations.append(result.animate.write())
        return MatrixSelectionAnimation._from_animations(animations)


# Layout defaults in logical scene units (a default frame is 16x9). They must
# match ``Visualization.matrix`` in gaanim_core.pyi; pixel-sized values here
# would place a small matrix far outside the frame.
_DEFAULT_ROW_GAP = 0.24
_DEFAULT_COLUMN_GAP = 0.24
_DEFAULT_DELIMITER_GAP = 0.12
# The theme's math size, which entries take without an ``entry_style`` size.
_DEFAULT_ENTRY_SIZE = 0.44


def _auto_delimiter_size(rows: int, options: Mapping[str, Any]) -> float:
    """Automatic delimiter font size, in logical units, spanning ``rows`` entries.

    It follows the entries' size (``entry_style``) and the gap between rows.
    """
    style = options.get("entry_style")
    entry = getattr(style, "size", None) or _DEFAULT_ENTRY_SIZE
    factor = float(entry) / _DEFAULT_ENTRY_SIZE
    row_gap = float(options.get("row_gap", _DEFAULT_ROW_GAP))
    return max(0.72 * factor, rows * 0.36 * factor + (rows - 1) * row_gap + 0.24 * factor)


def _build_matrix(scene: Any, data: Any, **options: Any) -> Matrix:
    layout = _matrix_layout(scene, _matrix_rows(data), options)
    return Matrix(scene, scene.layout.grid(*layout.items, **layout.tracks, align="center"), layout)


def _matrix_layout(scene: Any, data: Any, options: Mapping[str, Any]) -> _MatrixLayout:
    values = _matrix_rows(data)
    _validate_rectangular(values)
    row_gap = _non_negative(float(options.get("row_gap", _DEFAULT_ROW_GAP)), "row_gap")
    column_gap = _non_negative(float(options.get("column_gap", _DEFAULT_COLUMN_GAP)), "column_gap")
    delimiter_gap = _non_negative(float(options.get("delimiter_gap", _DEFAULT_DELIMITER_GAP)), "delimiter_gap")
    delimiter = options.get("delimiters", "brackets")
    row_labels, column_labels = options.get("row_labels"), options.get("column_labels")
    cell_mode, label_mode = options.get("cell_mode", "math"), options.get("label_mode", "math")
    if cell_mode not in {"math", "text"}: raise ValueError("cell_mode must be math or text")
    if label_mode not in {"math", "text"}: raise ValueError("label_mode must be math or text")
    if row_labels is not None and len(row_labels) != len(values): raise ValueError("row_labels length must match matrix rows")
    if column_labels is not None and len(column_labels) != len(values[0]): raise ValueError("column_labels length must match matrix columns")
    if options.get("fractions", "stacked") not in {"stacked", "inline"}: raise ValueError("fractions must be stacked or inline")

    pair = {"brackets": ("[", "]"), "parentheses": ("(", ")"), "braces": ("{", "}"),
            "bars": ("|", "|"), "double_bars": ("‖", "‖"), "none": (None, None)}
    if delimiter not in pair: raise ValueError("delimiters must be brackets, parentheses, braces, bars, double_bars, or none")
    has_delimiters = pair[delimiter][0] is not None

    cells: list[list[Drawable]] = []
    row_label_cells: list[Drawable] = []
    column_label_cells: list[Drawable] = []
    items = []
    row_offset = int(column_labels is not None)
    label_columns = int(row_labels is not None)
    delimiter_columns = int(has_delimiters)
    entry_column_offset = label_columns + delimiter_columns
    for row, source_row in enumerate(values):
        cell_row = []
        for column, value in enumerate(source_row):
            stored, cell = _make_cell(scene, value, row, column, options)
            values[row][column] = stored; cell_row.append(cell)
            items.append(cell.item(row=row + row_offset, column=column + entry_column_offset, align_self="center"))
        cells.append(cell_row)
    if row_labels is not None:
        for row, value in enumerate(row_labels):
            _, label = _make_cell(scene, value, row, -1, {**options, "cell_mode": options.get("label_mode", "math"), "entry_style": options.get("label_style")})
            row_label_cells.append(label)
            items.append(label.item(row=row + row_offset, column=0, align_self="center"))
    if column_labels is not None:
        for column, value in enumerate(column_labels):
            _, label = _make_cell(scene, value, -1, column, {**options, "cell_mode": options.get("label_mode", "math"), "entry_style": options.get("label_style")})
            column_label_cells.append(label)
            items.append(label.item(row=0, column=column + entry_column_offset, align_self="center"))
    delimiter_cells = []
    requested_size = options.get("delimiter_size")
    size = float(requested_size) if requested_size is not None else _auto_delimiter_size(len(values), options)
    weight = int(options.get("delimiter_weight", 300))
    if not 100 <= weight <= 900: raise ValueError("delimiter_weight must be between 100 and 900")
    if has_delimiters:
        left = scene.text(pair[delimiter][0], size=size, weight=weight)
        right = scene.text(pair[delimiter][1], size=size, weight=weight)
        delimiter_cells.extend((left, right))
        delimiter_offset = max(0.0, column_gap - delimiter_gap) * 0.5
        items.append(left.item(row=row_offset, column=label_columns, row_span=len(values), align_self="center", height=0, offset=(delimiter_offset, 0.0)))
        items.append(right.item(row=row_offset, column=entry_column_offset + len(values[0]), row_span=len(values), align_self="center", height=0, offset=(-delimiter_offset, 0.0)))
    column_count = entry_column_offset + len(values[0]) + delimiter_columns
    tracks = dict(rows=["auto"] * (len(values) + row_offset), columns=["auto"] * column_count, row_gap=row_gap, column_gap=column_gap)
    stored_options = dict(options)
    stored_options.update(row_gap=row_gap, column_gap=column_gap, delimiter_gap=delimiter_gap, delimiters=delimiter)
    return _MatrixLayout(values, cells, tuple(delimiter_cells), tuple(row_label_cells), tuple(column_label_cells), items, tracks, stored_options)


def _make_cell(scene: Any, value: Any, row: int, column: int, options: Mapping[str, Any]) -> tuple[Any, Drawable]:
    entry = value if isinstance(value, MatrixEntry) else MatrixEntry(value)
    if isinstance(entry.value, Drawable): return entry, entry.value
    factory = options.get("cell_factory")
    if factory is not None:
        result = factory(entry.value, row, column)
        if not isinstance(result, Drawable): raise TypeError("cell_factory must return Drawable")
        return entry, result
    style = entry.style or options.get("entry_style")
    kwargs = {"style": style} if style is not None else {}
    if options.get("cell_mode", "math") == "text": return entry, scene.text(str(entry.value), **kwargs)
    return entry, scene.text.equation(_typst_expr(entry.value, *_formats(options)), **kwargs)


def _matrix_rows(data: Any) -> list[list[Any]]:
    if data.__class__.__module__.startswith("sympy") and hasattr(data, "tolist"): return [list(row) for row in data.tolist()]
    try: return [list(row) for row in data]
    except TypeError as error: raise TypeError("matrix data must be a rectangular sequence of rows") from error


def _validate_rectangular(rows: Sequence[Sequence[Any]], expected: tuple[int, int] | None = None) -> None:
    if not rows or not rows[0]: raise ValueError("matrix data must contain at least one row and one column")
    columns = len(rows[0])
    if any(len(row) != columns for row in rows): raise ValueError("matrix rows must all have the same length")
    if expected is not None and (len(rows), columns) != expected: raise ValueError(f"matrix mask must have shape {expected[0]}x{expected[1]}")


def _indices(value: Any, length: int) -> tuple[list[int], bool]:
    if isinstance(value, int): return [_normalize_index(value, length)], True
    if isinstance(value, slice):
        start, stop, step = value.indices(length)
        if step != 1: raise ValueError("matrix slices require a step of 1")
        return list(range(start, stop)), False
    result = [_normalize_index(index, length) for index in value]
    return result, False


def _normalize_index(index: int, length: int) -> int:
    normalized = index + length if index < 0 else index
    if not 0 <= normalized < length: raise IndexError("matrix index out of range")
    return normalized


def _non_negative(value: float, name: str) -> float:
    if value != value or value < 0 or value == float("inf"): raise ValueError(f"{name} must be finite and non-negative")
    return value


def _entry_value(value: Any) -> Any: return value.value if isinstance(value, MatrixEntry) else value


def _entry_key(value: Any) -> str | None: return value.key if isinstance(value, MatrixEntry) else None


def _match_entries(source: Matrix, target: Matrix, mode: str) -> tuple[list[tuple[Drawable, Drawable]], list[Drawable], list[Drawable]]:
    unmatched_source = {(r, c) for r in range(source.nrows) for c in range(source.ncols)}
    unmatched_target = {(r, c) for r in range(target.nrows) for c in range(target.ncols)}
    pairs: list[tuple[Drawable, Drawable]] = []
    phases = ["key", "value", "position"] if mode == "auto" else [mode]
    for phase in phases:
        for coordinate in sorted(tuple(unmatched_source)):
            row, column = coordinate; source_value = source._values[row][column]
            candidates = []
            for candidate in unmatched_target:
                tr, tc = candidate; target_value = target._values[tr][tc]
                matches = (phase == "key" and _entry_key(source_value) is not None and _entry_key(source_value) == _entry_key(target_value)) or (phase == "value" and str(_entry_value(source_value)) == str(_entry_value(target_value))) or (phase == "position" and coordinate == candidate)
                if matches: candidates.append(candidate)
            if candidates:
                chosen = min(candidates, key=lambda item: (abs(item[0] - row) + abs(item[1] - column), item))
                pairs.append((source._cells[row][column], target._cells[chosen[0]][chosen[1]])); unmatched_source.remove(coordinate); unmatched_target.remove(chosen)
    return pairs, [source._cells[r][c] for r, c in sorted(unmatched_source)], [target._cells[r][c] for r, c in sorted(unmatched_target)]


def _sympy() -> Any:
    try: import sympy
    except ImportError as error: raise ImportError("matrix algebra requires the optional extra: uv pip install 'gaanim[algebra]'") from error
    return sympy


def _formats(options: Mapping[str, Any]) -> tuple[str, str]:
    """The ``numeric_format`` and ``fractions`` options of a matrix."""
    return options.get("numeric_format", "g"), options.get("fractions", "stacked")


def _typst_expr(value: Any, numeric_format: str = "g", fractions: str = "stacked") -> str:
    def expr(item: Any) -> str: return _typst_expr(item, numeric_format, fractions)
    if isinstance(value, float): return format(value, numeric_format)
    if isinstance(value, (int, str)): return str(value)
    if value.__class__.__module__.startswith("sympy"):
        sp = _sympy()
        if value.is_Rational and value.q != 1:
            # Inline, 3/2 keeps the height of the other entries.
            return f"{value.p}\\/{value.q}" if fractions == "inline" else f"({value.p})/({value.q})"
        if value.is_Pow: return f"({expr(value.base)})^({expr(value.exp)})"
        if value.is_Add: return " + ".join(expr(arg) for arg in value.as_ordered_terms()).replace("+ -", "- ")
        if value.is_Mul: return " ".join(expr(arg) for arg in value.as_ordered_factors())
        if isinstance(value, sp.Function): return f"{value.func.__name__}({expr(value.args[0])})"
        return str(value)
    return str(value)


def _result_matrix(source: Matrix, value: Any) -> Matrix:
    """A result matrix with the source's options (not its labels)."""
    options = {key: item for key, item in source._options.items() if key not in {"row_labels", "column_labels"}}
    rows = value.tolist()
    if options.get("delimiter_size") is not None and len(rows) != source.nrows:
        options["delimiter_size"] = float(options["delimiter_size"]) * _auto_delimiter_size(len(rows), options) / _auto_delimiter_size(source.nrows, options)
    return _build_matrix(source._scene, rows, **options)


def _algebra_options(value: Any, exact: bool, precision: int | None) -> Any:
    if exact: return value
    precision = 50 if precision is None else precision
    if precision < 2: raise ValueError("precision must be at least 2 digits")
    return value.evalf(precision) if hasattr(value, "evalf") else value


def _derive_binary(left: Matrix, right: Matrix, operation: str, *, exact: bool = True, precision: int | None = None) -> MatrixDerivation:
    a, b = left.to_sympy(), right.to_sympy()
    try:
        if operation == "add": value = a + b
        elif operation == "subtract": value = a - b
        elif operation == "matmul": value = a * b
        else: value = a.multiply_elementwise(b)
        value = _algebra_options(value, exact, precision)
    except Exception as error: raise MatrixAlgebraError(f"{operation} failed exactly: {error}; retry with exact=False, precision=N") from error
    steps = []
    for row in range(value.rows):
        for column in range(value.cols):
            sources = ((row, column),) if operation != "matmul" else tuple((row, k) for k in range(a.cols)) + tuple((k, column) for k in range(b.rows))
            steps.append(MatrixStep("dot_product" if operation == "matmul" else "combine", sources, (row, column), _typst_expr(value[row, column])))
    return MatrixDerivation(value, _result_matrix(left, value), steps)


def _derive_unary(source: Matrix, operation: str, *, exact: bool = True, precision: int | None = None, scalar: Any = None) -> MatrixDerivation:
    matrix = source.to_sympy(); sp = _sympy()
    try:
        if operation == "scale": value, result = matrix * sp.sympify(scalar), None
        elif operation == "transpose": value, result = matrix.T, None
        elif operation == "determinant": value, result = matrix.det(), "det"
        elif operation == "inverse": value, result = matrix.inv(), None
        elif operation == "rank": value, result = matrix.rank(), "rank"
        elif operation == "trace": value, result = matrix.trace(), "tr"
        elif operation == "rref": value, pivots = matrix.rref(); result = None
        elif operation == "lu":
            lower, upper, swaps = matrix.LUdecomposition(); value = (lower, upper, swaps); result = "tuple"
        elif operation == "qr":
            q, r = matrix.QRdecomposition(); value = (q, r); result = "tuple"
        else:
            vectors = matrix.eigenvects(); value = vectors; result = "eigen"
        value = _algebra_options(value, exact, precision) if not isinstance(value, (tuple, list)) else value
    except Exception as error: raise MatrixAlgebraError(f"{operation} failed exactly: {error}; retry with exact=False, precision=N") from error
    steps: list[MatrixStep] = []
    if operation in {"scale", "transpose", "inverse", "rref"}:
        visual = _result_matrix(source, value)
        kind = "row_operation" if operation in {"inverse", "rref"} else operation
        steps = [MatrixStep(kind, expression=operation)]
    elif result == "tuple":
        matrices = value[:2]
        visual = tuple(_result_matrix(source, item) for item in matrices)
        steps = [MatrixStep("decomposition", expression=operation.upper())]
    elif result == "eigen":
        eigenvalues = sp.Matrix([[item[0]] for item in value])
        eigenvectors = sp.Matrix.hstack(*(vector for _, _, basis in value for vector in basis)) if any(basis for _, _, basis in value) else sp.zeros(matrix.rows, 0)
        visual = (_result_matrix(source, eigenvalues), _result_matrix(source, eigenvectors))
        steps = [MatrixStep("characteristic_polynomial", expression=_typst_expr(matrix.charpoly().as_expr())), MatrixStep("eigenspaces")]
    else:
        # Typst names det and tr; rank is an operator it does not know.
        name = {"det": "det", "tr": "tr", "rank": 'op("rank")'}[result]
        expression = _typst_expr(value, *_formats(source._options))
        text = source._scene.text.equation(f"{name} = {expression}")
        visual = text; steps = [MatrixStep(operation, expression=expression)]
    return MatrixDerivation(value, visual, steps)


__all__ = ["Matrix", "MatrixEntry", "MatrixSelection", "MatrixSelectionAnimation", "MatrixStep", "MatrixDerivation", "MatrixAlgebraError"]
