"""Structured, selectable, and algebra-aware matrices for Gaanim."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable, Iterable, Iterator, Mapping, Sequence

from .gaanim_core import Anim, Drawable, MatrixOrder, Text


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

    def __init__(self, selection: "MatrixSelection", *, order: Any, stagger: float, seed: int):
        self._selection = selection
        self._ordered = selection._ordered(order, seed)
        self._stagger = _non_negative(stagger, "stagger")
        self._animations = [cell.animate().delay(index * self._stagger) for index, (_, cell) in enumerate(self._ordered)]

    def _apply(self, name: str, *args: Any) -> "MatrixSelectionAnimation":
        self._animations = [getattr(animation, name)(*args) for animation in self._animations]
        return self

    def fill(self, color: Any) -> "MatrixSelectionAnimation": return self._apply("fill", color)
    def color(self, color: Any) -> "MatrixSelectionAnimation": return self._apply("color", color)
    def opacity(self, value: float) -> "MatrixSelectionAnimation": return self._apply("opacity", value)
    def scale(self, factor: float) -> "MatrixSelectionAnimation": return self._apply("scale", factor)
    def rotate(self, radians: float) -> "MatrixSelectionAnimation": return self._apply("rotate", radians)
    def duration(self, seconds: float) -> "MatrixSelectionAnimation": return self._apply("duration", seconds)
    def ease(self, name: str) -> "MatrixSelectionAnimation": return self._apply("ease", name)

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

    def animate(self, *, order: Any = "simultaneous", stagger: float = 0.0, seed: int = 0) -> MatrixSelectionAnimation:
        return MatrixSelectionAnimation(self, order=order, stagger=stagger, seed=seed)

    def _effect(self, name: str, duration: float | None, order: Any, stagger: float, seed: int) -> list[Anim]:
        stagger = _non_negative(stagger, "stagger")
        animations = []
        for index, (_, cell) in enumerate(self._ordered(order, seed)):
            animation = getattr(cell, name)(duration)
            animations.append(animation.delay(index * stagger))
        return animations

    def write(self, duration: float | None = None, *, order: Any = "row_major", stagger: float = 0.05, seed: int = 0) -> list[Anim]:
        return self._effect("write", duration, order, stagger, seed)
    def create(self, duration: float | None = None, *, order: Any = "row_major", stagger: float = 0.05, seed: int = 0) -> list[Anim]:
        return self._effect("create", duration, order, stagger, seed)
    def fade_in(self, duration: float | None = None, *, order: Any = "simultaneous", stagger: float = 0.0, seed: int = 0) -> list[Anim]:
        return self._effect("fade_in", duration, order, stagger, seed)
    def fade_out(self, duration: float | None = None, *, order: Any = "simultaneous", stagger: float = 0.0, seed: int = 0) -> list[Anim]:
        return self._effect("fade_out", duration, order, stagger, seed)
    def indicate(self, duration: float | None = None, *, order: Any = "simultaneous", stagger: float = 0.0, seed: int = 0) -> list[Anim]:
        return self._effect("indicate", duration, order, stagger, seed)
    def wiggle(self, duration: float | None = None, *, order: Any = "simultaneous", stagger: float = 0.0, seed: int = 0) -> list[Anim]:
        return self._effect("wiggle", duration, order, stagger, seed)

    def offset(self, dx: float, dy: float, *, animate: float | None = None) -> "MatrixSelection":
        if not all(isinstance(value, (int, float)) and float(value) == float(value) for value in (dx, dy)):
            raise ValueError("matrix selection offset must be finite")
        for position, (_, cell) in enumerate(self._entries):
            self._matrix._grid.configure_item(cell, offset=(float(dx), float(dy)), animate=animate if position == len(self._entries) - 1 else None)
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


class Matrix:
    """Persistent matrix composed from real Gaanim drawables and Layout v2."""

    def __init__(self, scene: Any, values: list[list[Any]], cells: list[list[Drawable]], grid: Any, root: Any,
                 delimiters: tuple[Drawable, ...], row_labels: tuple[Drawable, ...],
                 column_labels: tuple[Drawable, ...], options: Mapping[str, Any]):
        self._scene, self._values, self._cells = scene, values, cells
        self._grid, self._root, self._delimiters = grid, root, delimiters
        self._row_labels, self._column_labels = row_labels, column_labels
        self._options = dict(options)

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

    def __getattr__(self, name: str) -> Any: return getattr(self._root, name)
    def at(self, *args: Any, **kwargs: Any) -> "Matrix": self._root.at(*args, **kwargs); return self
    def fill(self, color: Any) -> "Matrix": self._root.fill(color); return self
    def opacity(self, value: float) -> "Matrix": self._root.opacity(value); return self
    def scaled(self, factor: float) -> "Matrix": self._root.scaled(factor); return self

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

    def set(self, row: int, column: int, value: Any, *, animate: float | None = None) -> Drawable:
        row = _normalize_index(row, self.nrows); column = _normalize_index(column, self.ncols)
        entry, cell = _make_cell(self._scene, value, row, column, self._options)
        old = self._cells[row][column]
        replacement = self._grid.replace(old, cell, animate=animate)
        self._values[row][column], self._cells[row][column] = entry, replacement
        return replacement

    def become(self, data: Any, *, animate: float | None = 1.0) -> list[Anim]:
        rows = _matrix_rows(data)
        options = dict(self._options)
        if options.get("row_labels") is not None and len(options["row_labels"]) != len(rows): options.pop("row_labels")
        if rows and options.get("column_labels") is not None and len(options["column_labels"]) != len(rows[0]): options.pop("column_labels")
        target = _build_matrix(self._scene, rows, **options)
        animations = [] if animate is None else [self._root.fade_out(animate), target._root.fade_in(animate)]
        if animate is None: self._root.opacity(0.0)
        self.__dict__.update(target.__dict__)
        return animations

    def insert_row(self, index: int, values: Sequence[Any], *, animate: float | None = 1.0) -> list[Anim]:
        data = [row[:] for row in self._values]; data.insert(index, list(values)); return self.become(data, animate=animate)
    def remove_row(self, index: int, *, animate: float | None = 1.0) -> list[Anim]:
        data = [row[:] for row in self._values]; data.pop(_normalize_index(index, self.nrows)); return self.become(data, animate=animate)
    def insert_column(self, index: int, values: Sequence[Any], *, animate: float | None = 1.0) -> list[Anim]:
        if len(values) != self.nrows: raise ValueError("inserted column length must match matrix rows")
        data = [row[:] for row in self._values]
        for row, value in zip(data, values): row.insert(index, value)
        return self.become(data, animate=animate)
    def remove_column(self, index: int, *, animate: float | None = 1.0) -> list[Anim]:
        column = _normalize_index(index, self.ncols); data = [row[:] for row in self._values]
        for row in data: row.pop(column)
        return self.become(data, animate=animate)
    def swap_rows(self, first: int, second: int, *, animate: float | None = 1.0) -> list[Anim]:
        data = [row[:] for row in self._values]; first = _normalize_index(first, self.nrows); second = _normalize_index(second, self.nrows); data[first], data[second] = data[second], data[first]; return self.become(data, animate=animate)
    def swap_columns(self, first: int, second: int, *, animate: float | None = 1.0) -> list[Anim]:
        first = _normalize_index(first, self.ncols); second = _normalize_index(second, self.ncols); data = [row[:] for row in self._values]
        for row in data: row[first], row[second] = row[second], row[first]
        return self.become(data, animate=animate)
    def reorder_rows(self, order: Sequence[int], *, animate: float | None = 1.0) -> list[Anim]:
        normalized = [_normalize_index(index, self.nrows) for index in order]
        if sorted(normalized) != list(range(self.nrows)): raise ValueError("row order must be a permutation")
        return self.become([self._values[index][:] for index in normalized], animate=animate)
    def reorder_columns(self, order: Sequence[int], *, animate: float | None = 1.0) -> list[Anim]:
        normalized = [_normalize_index(index, self.ncols) for index in order]
        if sorted(normalized) != list(range(self.ncols)): raise ValueError("column order must be a permutation")
        return self.become([[row[index] for index in normalized] for row in self._values], animate=animate)

    def morph_to(self, target: "Matrix", *, match: str = "auto", duration: float = 1.0, stagger: float = 0.0) -> list[Anim]:
        if match not in {"auto", "key", "value", "position"}: raise ValueError("match must be auto, key, value, or position")
        pairs, source_only, target_only = _match_entries(self, target, match)
        animations = [source.transform(destination).duration(duration).delay(index * stagger) for index, (source, destination) in enumerate(pairs)]
        animations.extend(cell.fade_out(duration) for cell in source_only)
        animations.extend(cell.fade_in(duration) for cell in target_only)
        return animations

    def to_sympy(self) -> Any: return _sympy().Matrix([[_entry_value(value) for value in row] for row in self._values])
    def add(self, other: "Matrix", **options: Any) -> "MatrixDerivation": return _derive_binary(self, other, "add", **options)
    def subtract(self, other: "Matrix", **options: Any) -> "MatrixDerivation": return _derive_binary(self, other, "subtract", **options)
    def matmul(self, other: "Matrix", **options: Any) -> "MatrixDerivation": return _derive_binary(self, other, "matmul", **options)
    def hadamard(self, other: "Matrix", **options: Any) -> "MatrixDerivation": return _derive_binary(self, other, "hadamard", **options)
    def scale_by(self, scalar: Any, **options: Any) -> "MatrixDerivation": return _derive_unary(self, "scale", scalar=scalar, **options)
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

    def animate(self, *, duration: float = 0.7, order: Any = "row_major", stagger: float = 0.04) -> list[Anim]:
        results = self.result if isinstance(self.result, tuple) else (self.result,)
        animations: list[Anim] = []
        for result in results:
            if isinstance(result, Matrix): animations.extend(result.entries.fade_in(duration, order=order, stagger=stagger))
            else: animations.append(result.write(duration))
        return animations


def _build_matrix(scene: Any, data: Any, **options: Any) -> Matrix:
    values = _matrix_rows(data)
    _validate_rectangular(values)
    row_gap = _non_negative(float(options.get("row_gap", 24.0)), "row_gap")
    column_gap = _non_negative(float(options.get("column_gap", 24.0)), "column_gap")
    delimiter_gap = _non_negative(float(options.get("delimiter_gap", 12.0)), "delimiter_gap")
    delimiter = options.get("delimiters", "brackets")
    row_labels, column_labels = options.get("row_labels"), options.get("column_labels")
    cell_mode, label_mode = options.get("cell_mode", "math"), options.get("label_mode", "math")
    if cell_mode not in {"math", "text"}: raise ValueError("cell_mode must be math or text")
    if label_mode not in {"math", "text"}: raise ValueError("label_mode must be math or text")
    if row_labels is not None and len(row_labels) != len(values): raise ValueError("row_labels length must match matrix rows")
    if column_labels is not None and len(column_labels) != len(values[0]): raise ValueError("column_labels length must match matrix columns")

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
            items.append(scene.item(cell, row=row + row_offset, column=column + entry_column_offset, align="center"))
        cells.append(cell_row)
    if row_labels is not None:
        for row, value in enumerate(row_labels):
            _, label = _make_cell(scene, value, row, -1, {**options, "cell_mode": options.get("label_mode", "math"), "entry_style": options.get("label_style")})
            row_label_cells.append(label)
            items.append(scene.item(label, row=row + row_offset, column=0, align="center"))
    if column_labels is not None:
        for column, value in enumerate(column_labels):
            _, label = _make_cell(scene, value, -1, column, {**options, "cell_mode": options.get("label_mode", "math"), "entry_style": options.get("label_style")})
            column_label_cells.append(label)
            items.append(scene.item(label, row=0, column=column + entry_column_offset, align="center"))
    delimiter_cells = []
    requested_size = options.get("delimiter_size")
    size = float(requested_size) if requested_size is not None else max(72.0, 60.0 * len(values))
    weight = int(options.get("delimiter_weight", 300))
    if not 100 <= weight <= 900: raise ValueError("delimiter_weight must be between 100 and 900")
    if has_delimiters:
        left = scene.text(pair[delimiter][0], size=size, weight=weight)
        right = scene.text(pair[delimiter][1], size=size, weight=weight)
        delimiter_cells.extend((left, right))
        delimiter_offset = max(0.0, column_gap - delimiter_gap) * 0.5
        items.append(scene.item(left, row=row_offset, column=label_columns, row_span=len(values), align="center", offset=(delimiter_offset, 0.0)))
        items.append(scene.item(right, row=row_offset, column=entry_column_offset + len(values[0]), row_span=len(values), align="center", offset=(-delimiter_offset, 0.0)))
    column_count = entry_column_offset + len(values[0]) + delimiter_columns
    grid = scene.grid(items, rows=["auto"] * (len(values) + row_offset), columns=["auto"] * column_count,
                      row_gap=row_gap, column_gap=column_gap, align="center")
    root = grid
    stored_options = dict(options)
    stored_options.update(row_gap=row_gap, column_gap=column_gap, delimiter_gap=delimiter_gap, delimiters=delimiter)
    return Matrix(scene, values, cells, grid, root, tuple(delimiter_cells), tuple(row_label_cells), tuple(column_label_cells), stored_options)


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
    return entry, scene.equation(_typst_expr(entry.value, options.get("numeric_format", "g")), **kwargs)


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


def _typst_expr(value: Any, numeric_format: str = "g") -> str:
    if isinstance(value, float): return format(value, numeric_format)
    if isinstance(value, (int, str)): return str(value)
    if value.__class__.__module__.startswith("sympy"):
        sp = _sympy()
        if value.is_Rational and value.q != 1: return f"({value.p})/({value.q})"
        if value.is_Pow: return f"({_typst_expr(value.base)})^({_typst_expr(value.exp)})"
        if value.is_Add: return " + ".join(_typst_expr(arg) for arg in value.as_ordered_terms()).replace("+ -", "- ")
        if value.is_Mul: return " ".join(_typst_expr(arg) for arg in value.as_ordered_factors())
        if isinstance(value, sp.Function): return f"{value.func.__name__}({_typst_expr(value.args[0])})"
        return str(value)
    return str(value)


def _result_matrix(source: Matrix, value: Any) -> Matrix:
    options = {key: item for key, item in source._options.items() if key not in {"row_labels", "column_labels"}}
    return _build_matrix(source._scene, value.tolist(), **options)


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
        text = source._scene.equation(f"{result} = {_typst_expr(value)}")
        visual = text; steps = [MatrixStep(operation, expression=_typst_expr(value))]
    return MatrixDerivation(value, visual, steps)


__all__ = ["Matrix", "MatrixEntry", "MatrixSelection", "MatrixSelectionAnimation", "MatrixStep", "MatrixDerivation", "MatrixAlgebraError"]
