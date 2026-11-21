"""Linear data structures for explaining algorithms: arrays, stacks, queues and linked lists.

Each structure is a row (or, for a stack, a column) of cells, each cell a
box and its value grouped into one drawable. Operations move those cells,
so a cell keeps its identity through swaps, insertions and removals, and
return a ``Composition`` to play with ``scene.play``, alone or next to other
animations such as a ``Code`` highlight.

Place a structure (``arr.place(zone)``, ``arr.move_to(x, y)``) before its
first operation and before adding pointers: cells and arrows made later are
put where the structure is then.
"""

from __future__ import annotations

from typing import Any, Callable, Iterable

from .gaanim_core import Composition, Drawable, parallel, sequence

_LIFT = 0.8  # Of a cell's size, how far a swap lifts its cells apart.


def _finite_positive(value: Any, name: str) -> float:
    number = float(value)
    if not (number > 0 and number != float("inf")):
        raise ValueError(f"{name} must be a positive number, not {value!r}")
    return number


def _non_negative(value: Any, name: str) -> float:
    number = float(value)
    if not (number >= 0 and number != float("inf")):
        raise ValueError(f"{name} must be zero or more, not {value!r}")
    return number


class _Linear:
    """Cells along one axis: ``"x"`` left to right, ``"y"`` bottom to top."""

    _kind = "structure"

    def __init__(self, scene: Any, values: Iterable[Any], *, axis: str, cell_size: float, gap: float,
                 indices: bool, format: Callable[[Any], str] | None) -> None:
        self._scene = scene
        self._axis = axis
        self._size = _finite_positive(cell_size, "cell_size")
        self._gap = _non_negative(gap, "gap")
        self._pitch = self._size + self._gap
        self._format = format or str
        self._indices = bool(indices)
        self._values: list[Any] = list(values)
        count = len(self._values)
        self._slots = max(count, 1)
        # An invisible guide over the first slots: where the cells sit, wherever
        # the structure is placed.
        span = self._pitch * self._slots - self._gap
        width, height = (span, self._size) if axis == "x" else (self._size, span)
        self._guide = scene.geometry.rect(width, height).opacity(0.0)
        self._items: list[Drawable] = []
        self._boxes: list[Drawable] = []
        self._labels: list[Drawable] = []
        self._index_labels: list[Drawable] = []
        for slot, value in enumerate(self._values):
            self._new_item(value, self._local_slot(slot))
        if self._indices:
            for slot in range(count):
                self._index_labels.append(self._new_index(slot, self._local_slot(slot)))
        self._root = scene.geometry.group(self._guide, *self._extra_members(), *self._items,
                                          *self._index_labels)

    # -- geometry --------------------------------------------------------

    def _local_slot(self, slot: int) -> tuple[float, float]:
        offset = (slot - (self._slots - 1) / 2) * self._pitch
        return (offset, 0.0) if self._axis == "x" else (0.0, offset)

    def _slot(self, slot: int) -> tuple[float, float]:
        """Where ``slot`` is now, measured from the guide."""
        box = self._guide.bounds()
        if self._axis == "x":
            return box.left + self._size / 2 + slot * self._pitch, box.y
        return box.x, box.bottom + self._size / 2 + slot * self._pitch

    def _step(self, slots: float) -> tuple[float, float]:
        distance = slots * self._pitch
        return (distance, 0.0) if self._axis == "x" else (0.0, distance)

    def _extra_members(self) -> list[Drawable]:
        return []

    # -- parts -----------------------------------------------------------

    def _new_item(self, value: Any, at: tuple[float, float], index: int | None = None) -> Drawable:
        scene, colors, size = self._scene, self._scene.colors, self._size
        box = scene.geometry.rounded_rect(size, size, size * 0.14).fill(colors.panel).stroke(colors.rule, 0.03)
        label = scene.text(self._format(value), size=size * 0.4)
        item = scene.geometry.group(box, label).move_to(*at)
        position = len(self._items) if index is None else index
        self._items.insert(position, item)
        self._boxes.insert(position, box)
        self._labels.insert(position, label)
        return item

    def _new_index(self, slot: int, at: tuple[float, float]) -> Drawable:
        scene = self._scene
        label = scene.text(str(slot), role="code", size=self._size * 0.26, color=scene.colors.muted)
        return label.move_to(at[0], at[1] - self._size * 0.78)

    def _check(self, index: int, *, allow_end: bool = False) -> int:
        count = len(self._items)
        limit = count + 1 if allow_end else count
        if not isinstance(index, int) or isinstance(index, bool):
            raise TypeError(f"an index is an int, not {type(index).__name__}")
        if index < 0:
            index += count
        if not 0 <= index < limit:
            raise IndexError(f"index {index} is out of range for a {self._kind} of {count}")
        return index

    @property
    def drawable(self) -> Drawable:
        """The structure as first built: its cells, index labels and guide."""
        return self._root

    @property
    def values(self) -> list[Any]:
        """The values, in order, as the operations so far left them."""
        return list(self._values)

    def __len__(self) -> int:
        return len(self._items)

    def item(self, index: int) -> Drawable:
        """The cell at ``index``: its box and its value as one drawable."""
        return self._items[self._check(index)]

    def cell(self, index: int) -> Drawable:
        """The box of the cell at ``index``, to fill or outline."""
        return self._boxes[self._check(index)]

    def label(self, index: int) -> Drawable:
        """The value text of the cell at ``index``."""
        return self._labels[self._check(index)]

    @property
    def items(self) -> list[Drawable]:
        """Every cell, in order."""
        return list(self._items)

    def __getattr__(self, name: str) -> Any:
        if name.startswith("_"):
            raise AttributeError(name)
        value = getattr(self._root, name)
        if not callable(value):
            return value

        def fluent(*args: Any, **kwargs: Any) -> Any:
            result = value(*args, **kwargs)
            return self if result is self._root else result

        return fluent

    # -- shared operations -------------------------------------------------

    def _shift(self, items: Iterable[Drawable], slots: float, duration: float) -> list[Any]:
        dx, dy = self._step(slots)
        return [item.animate.shift_by(dx, dy).duration(duration) for item in items]

    def _enter(self, item: Drawable, offset: tuple[float, float], duration: float) -> Composition:
        """Fade ``item`` in while it moves by ``offset`` into place."""
        dx, dy = offset
        if dx or dy:
            item.shift_by(-dx, -dy)
        return parallel(item.animate.fade_in().duration(duration),
                        item.animate.shift_by(dx, dy).duration(duration))

    def _leave(self, item: Drawable, offset: tuple[float, float], duration: float) -> Composition:
        dx, dy = offset
        return parallel(item.animate.fade_out().duration(duration),
                        item.animate.shift_by(dx, dy).duration(duration))


class _Animate:
    """The operations of a structure, and its drawable's own animations."""

    def __init__(self, owner: _Linear) -> None:
        self._owner = owner

    def __getattr__(self, name: str) -> Any:
        if name.startswith("_"):
            raise AttributeError(name)
        return getattr(self._owner._root.animate, name)


class ArrayAnimate(_Animate):
    def compare(self, i: int, j: int, *, color: Any = None, duration: float = 0.6) -> Composition:
        """Highlight cells ``i`` and ``j`` together, as a comparison does."""
        array = self._owner
        color = array._scene.colors.accent if color is None else color
        return parallel(*(array.item(index).animate.indicate(color=color).duration(duration) for index in (i, j)))

    def swap(self, i: int, j: int, *, duration: float = 0.9) -> Composition:
        """Exchange cells ``i`` and ``j``: the first lifts over the second as they cross."""
        array = self._owner
        i, j = array._check(i), array._check(j)
        if i == j:
            raise ValueError("swap needs two different cells")
        if i > j:
            i, j = j, i
        first, second = array._items[i], array._items[j]
        third = duration / 3
        lift = array._size * _LIFT
        up, across = (0.0, lift), array._step(j - i)
        if array._axis == "y":
            up = (lift, 0.0)

        # The first cell lifts out, crosses over and settles; the second slides
        # across beneath it, clear of the indices and pointers.
        motion = parallel(
            sequence(first.animate.shift_by(*up).duration(third),
                     first.animate.shift_by(*across).duration(third),
                     first.animate.shift_by(-up[0], -up[1]).duration(third)),
            second.animate.shift_by(-across[0], -across[1]).duration(third).delay(third),
        )
        for parts in (array._items, array._boxes, array._labels, array._values):
            parts[i], parts[j] = parts[j], parts[i]
        return motion

    def insert(self, index: int, value: Any, *, duration: float = 0.8) -> Composition:
        """Make room at ``index`` and put ``value`` there; ``len`` appends."""
        array = self._owner
        index = array._check(index, allow_end=True)
        half = duration / 2
        moving = array._items[index:]
        room = array._shift(moving, 1, half)
        item = array._new_item(value, array._slot(index), index)
        array._values.insert(index, value)
        entering = [array._enter(item, (0.0, -array._size * 0.6), half)]
        if array._indices:
            slot = len(array._index_labels)
            label = array._new_index(slot, array._slot(slot))
            array._index_labels.append(label)
            entering.append(label.animate.fade_in().duration(half))
        return sequence(parallel(*room), parallel(*entering)) if room else parallel(*entering)

    def remove(self, index: int, *, duration: float = 0.8) -> Composition:
        """Take cell ``index`` out and close the gap."""
        array = self._owner
        index = array._check(index)
        half = duration / 2
        item = array._items[index]
        leaving = [array._leave(item, (0.0, -array._size * 0.6), half)]
        for parts in (array._items, array._boxes, array._labels, array._values):
            del parts[index]
        closing = array._shift(array._items[index:], -1, half)
        if array._indices and array._index_labels:
            closing.append(array._index_labels.pop().animate.fade_out().duration(half))
        return sequence(parallel(*leaving), parallel(*closing)) if closing else parallel(*leaving)


class ArrayPointer:
    """A named arrow under a cell of an array, from ``Array.pointer``."""

    def __init__(self, array: "Array", name: str, index: int, color: Any) -> None:
        scene = array._scene
        self._array = array
        self._index = array._check(index)
        x, y = array._slot(self._index)
        top = y - array._size / 2 - (array._size * 0.52 if array._indices else array._size * 0.12)
        length = array._size * 0.5
        color = scene.colors.accent if color is None else color
        arrow = scene.geometry.arrow(x, top - length, x, top).fill(color)
        label = scene.text(name, role="code", size=array._size * 0.3, color=color)
        label.move_to(x, top - length - array._size * 0.25)
        self.name = name
        self._root = scene.geometry.group(arrow, label)
        self.animate = PointerAnimate(self)

    @property
    def index(self) -> int:
        """The cell the pointer points at, after the moves so far."""
        return self._index

    @property
    def drawable(self) -> Drawable:
        """The arrow and its name as one drawable."""
        return self._root

    def __getattr__(self, name: str) -> Any:
        if name.startswith("_"):
            raise AttributeError(name)
        return getattr(self._root, name)


class PointerAnimate:
    def __init__(self, pointer: ArrayPointer) -> None:
        self._pointer = pointer

    def move_to(self, index: int, *, duration: float = 0.5) -> Any:
        """Slide the pointer under cell ``index``."""
        pointer = self._pointer
        array = pointer._array
        target = array._check(index, allow_end=True)
        dx, dy = array._step(target - pointer._index)
        pointer._index = target
        return pointer._root.animate.shift_by(dx, dy).duration(duration)

    def __getattr__(self, name: str) -> Any:
        if name.startswith("_"):
            raise AttributeError(name)
        return getattr(self._pointer._root.animate, name)


class Array(_Linear):
    """An array of cells with optional indices and named pointers, from ``scene.viz.array``."""

    _kind = "array"

    def __init__(self, scene: Any, values: Iterable[Any], **options: Any) -> None:
        super().__init__(scene, values, axis="x", **options)
        self.animate = ArrayAnimate(self)

    def pointer(self, name: str, index: int = 0, *, color: Any = None) -> ArrayPointer:
        """A named arrow under cell ``index``; ``pointer.animate.move_to(k)`` slides it."""
        if not str(name).strip():
            raise ValueError("a pointer needs a name")
        return ArrayPointer(self, str(name), index, color)


class StackAnimate(_Animate):
    def push(self, value: Any, *, duration: float = 0.5) -> Composition:
        """Drop ``value`` onto the top of the stack."""
        stack = self._owner
        slot = len(stack._items)
        item = stack._new_item(value, stack._slot(slot))
        stack._values.append(value)
        return stack._enter(item, (0.0, -stack._size * _LIFT), duration)

    def pop(self, *, duration: float = 0.5) -> Composition:
        """Lift the top value off the stack."""
        stack = self._owner
        if not stack._items:
            raise IndexError("pop from an empty stack")
        item = stack._items.pop()
        stack._boxes.pop()
        stack._labels.pop()
        stack._values.pop()
        return stack._leave(item, (0.0, stack._size * _LIFT), duration)


class Stack(_Linear):
    """A stack of cells that grows upward, from ``scene.viz.stack``."""

    _kind = "stack"

    def __init__(self, scene: Any, values: Iterable[Any], **options: Any) -> None:
        super().__init__(scene, values, axis="y", **options)
        self.animate = StackAnimate(self)

    @property
    def top(self) -> Drawable:
        """The cell on top."""
        return self.item(-1)


class QueueAnimate(_Animate):
    def enqueue(self, value: Any, *, duration: float = 0.5) -> Composition:
        """Add ``value`` at the back of the queue, on the right."""
        queue = self._owner
        slot = len(queue._items)
        item = queue._new_item(value, queue._slot(slot))
        queue._values.append(value)
        return queue._enter(item, (-queue._size * _LIFT, 0.0), duration)

    def dequeue(self, *, duration: float = 0.8) -> Composition:
        """Take the front value out on the left, and move the rest up."""
        queue = self._owner
        if not queue._items:
            raise IndexError("dequeue from an empty queue")
        half = duration / 2
        item = queue._items.pop(0)
        queue._boxes.pop(0)
        queue._labels.pop(0)
        queue._values.pop(0)
        leaving = queue._leave(item, (-queue._size * _LIFT, 0.0), half)
        moving = queue._shift(queue._items, -1, half)
        return sequence(leaving, parallel(*moving)) if moving else leaving


class Queue(_Linear):
    """A queue of cells, front on the left, from ``scene.viz.queue``."""

    _kind = "queue"

    def __init__(self, scene: Any, values: Iterable[Any], **options: Any) -> None:
        super().__init__(scene, values, axis="x", **options)
        self.animate = QueueAnimate(self)


class LinkedListAnimate(_Animate):
    def insert_after(self, index: int, value: Any, *, duration: float = 0.8) -> Composition:
        """Link a new node holding ``value`` after node ``index``."""
        nodes = self._owner
        index = nodes._check(index)
        half = duration / 2
        count = len(nodes._items)
        room = nodes._shift(nodes._items[index + 1:], 1, half)
        room += nodes._shift(nodes._arrows[index + 1:], 1, half)
        item = nodes._new_item(value, nodes._slot(index + 1), index + 1)
        nodes._values.insert(index + 1, value)
        # The arrow out of `index` now reaches the new node; a new arrow links
        # it to the node after, or the old last node to it.
        start = index + 1 if index < count - 1 else index
        arrow = nodes._new_arrow(start)
        nodes._arrows.insert(start, arrow)
        entering = [nodes._enter(item, (0.0, -nodes._size * 0.6), half), arrow.animate.fade_in().duration(half)]
        return sequence(parallel(*room), parallel(*entering)) if room else parallel(*entering)

    def remove(self, index: int, *, duration: float = 0.8) -> Composition:
        """Unlink node ``index`` and close the gap."""
        nodes = self._owner
        index = nodes._check(index)
        half = duration / 2
        count = len(nodes._items)
        item = nodes._items[index]
        leaving = [nodes._leave(item, (0.0, -nodes._size * 0.6), half)]
        if count > 1:
            gone = index if index < count - 1 else index - 1
            leaving.append(nodes._arrows.pop(gone).animate.fade_out().duration(half))
        for parts in (nodes._items, nodes._boxes, nodes._labels, nodes._values):
            del parts[index]
        closing = nodes._shift(nodes._items[index:], -1, half)
        closing += nodes._shift(nodes._arrows[index:], -1, half)
        return sequence(parallel(*leaving), parallel(*closing)) if closing else parallel(*leaving)


class LinkedList(_Linear):
    """Nodes linked by arrows, from ``scene.viz.linked_list``."""

    _kind = "linked list"

    def __init__(self, scene: Any, values: Iterable[Any], **options: Any) -> None:
        self._arrows: list[Drawable] = []
        super().__init__(scene, values, axis="x", **options)
        self.animate = LinkedListAnimate(self)

    def _extra_members(self) -> list[Drawable]:
        self._arrows = [self._arrow_between(self._local_slot(k), self._local_slot(k + 1))
                        for k in range(len(self._values) - 1)]
        return self._arrows

    def _arrow_between(self, start: tuple[float, float], end: tuple[float, float]) -> Drawable:
        reach = self._size / 2 + self._gap * 0.12
        return self._scene.geometry.arrow(start[0] + reach, start[1], end[0] - reach, end[1]).fill(
            self._scene.colors.muted)

    def _new_arrow(self, slot: int) -> Drawable:
        return self._arrow_between(self._slot(slot), self._slot(slot + 1))

    def arrow(self, index: int) -> Drawable:
        """The arrow from node ``index`` to the next one."""
        if not 0 <= index < len(self._arrows):
            raise IndexError(f"arrow {index} is out of range for {len(self._arrows)} arrows")
        return self._arrows[index]


def _options(cell_size: float, gap: float, indices: bool, format: Any) -> dict[str, Any]:
    return {"cell_size": cell_size, "gap": gap, "indices": indices, "format": format}


def _build_array(scene: Any, values: Iterable[Any], *, cell_size: float = 0.9, gap: float = 0.12,
                 indices: bool = False, format: Callable[[Any], str] | None = None) -> Array:
    return Array(scene, values, **_options(cell_size, gap, indices, format))


def _build_stack(scene: Any, values: Iterable[Any] = (), *, cell_size: float = 0.9, gap: float = 0.08,
                 format: Callable[[Any], str] | None = None) -> Stack:
    return Stack(scene, values, **_options(cell_size, gap, False, format))


def _build_queue(scene: Any, values: Iterable[Any] = (), *, cell_size: float = 0.9, gap: float = 0.12,
                 format: Callable[[Any], str] | None = None) -> Queue:
    return Queue(scene, values, **_options(cell_size, gap, False, format))


def _build_linked_list(scene: Any, values: Iterable[Any], *, cell_size: float = 0.9, gap: float = 0.7,
                       format: Callable[[Any], str] | None = None) -> LinkedList:
    return LinkedList(scene, values, **_options(cell_size, gap, False, format))
