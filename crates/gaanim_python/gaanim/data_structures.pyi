from typing import Any, Callable

from .gaanim_core import Anim, ColorLike, Composition, Drawable

class _Structure:
    @property
    def drawable(self) -> Drawable:
        """The structure as first built: its cells, index labels and guide.

        Place it (``place``, ``move_to``, ``next_to``: the structure forwards
        them) before the first operation and before adding pointers; cells
        and arrows made later go where it is then.
        """
        ...
    @property
    def values(self) -> list[Any]:
        """The values, in order, as the operations so far left them."""
        ...
    @property
    def items(self) -> list[Drawable]:
        """Every cell, in order: its box and its value as one drawable."""
        ...
    def __len__(self) -> int: ...
    def item(self, index: int) -> Drawable:
        """The cell at ``index`` (negative counts from the end) as one drawable.

        A cell keeps its identity: after a swap, ``item(0)`` is the cell that
        moved into slot 0. An index out of range raises ``IndexError``.
        """
        ...
    def cell(self, index: int) -> Drawable:
        """The box of the cell at ``index``, to fill or outline."""
        ...
    def label(self, index: int) -> Drawable:
        """The value text of the cell at ``index``."""
        ...
    def __getattr__(self, name: str) -> Any: ...

class ArrayAnimate:
    def compare(self, i: int, j: int, *, color: ColorLike | None = None, duration: float = 0.6) -> Composition:
        """Highlight cells ``i`` and ``j`` together (``scene.colors.accent`` by default)."""
        ...
    def swap(self, i: int, j: int, *, duration: float = 0.9) -> Composition:
        """Exchange cells ``i`` and ``j``: the first lifts over the second as they cross.

        The cells move rather than fade, so each keeps its identity. The same
        index twice raises ``ValueError``.
        """
        ...
    def insert(self, index: int, value: Any, *, duration: float = 0.8) -> Composition:
        """Make room at ``index`` (``len(array)`` appends) and drop ``value`` in."""
        ...
    def remove(self, index: int, *, duration: float = 0.8) -> Composition:
        """Lift cell ``index`` out and close the gap."""
        ...
    def __getattr__(self, name: str) -> Any: ...

class PointerAnimate:
    def move_to(self, index: int, *, duration: float = 0.5) -> Anim:
        """Slide the pointer under cell ``index`` (``len(array)`` is past the end)."""
        ...
    def __getattr__(self, name: str) -> Any: ...

class ArrayPointer:
    """A named arrow under a cell of an array, from ``Array.pointer``."""
    name: str
    animate: PointerAnimate
    @property
    def index(self) -> int:
        """The cell the pointer points at, after the moves so far."""
        ...
    @property
    def drawable(self) -> Drawable:
        """The arrow and its name as one drawable."""
        ...
    def __getattr__(self, name: str) -> Any: ...

class Array(_Structure):
    """An array of cells with optional indices and named pointers, from ``scene.viz.array``."""
    animate: ArrayAnimate
    def pointer(self, name: str, index: int = 0, *, color: ColorLike | None = None) -> ArrayPointer:
        """A named arrow under cell ``index``, below the indices.

        Add pointers after placing the array. ``pointer.animate.move_to(k)``
        slides it; an empty name raises ``ValueError``.
        """
        ...

class StackAnimate:
    def push(self, value: Any, *, duration: float = 0.5) -> Composition:
        """Drop ``value`` onto the top of the stack."""
        ...
    def pop(self, *, duration: float = 0.5) -> Composition:
        """Lift the top value off; an empty stack raises ``IndexError``."""
        ...
    def __getattr__(self, name: str) -> Any: ...

class Stack(_Structure):
    """A stack of cells that grows upward, from ``scene.viz.stack``."""
    animate: StackAnimate
    @property
    def top(self) -> Drawable:
        """The cell on top."""
        ...

class QueueAnimate:
    def enqueue(self, value: Any, *, duration: float = 0.5) -> Composition:
        """Add ``value`` at the back of the queue, on the right."""
        ...
    def dequeue(self, *, duration: float = 0.8) -> Composition:
        """Take the front value out on the left and move the rest up; an empty queue raises ``IndexError``."""
        ...
    def __getattr__(self, name: str) -> Any: ...

class Queue(_Structure):
    """A queue of cells, front on the left, from ``scene.viz.queue``."""
    animate: QueueAnimate

class LinkedListAnimate:
    def insert_after(self, index: int, value: Any, *, duration: float = 0.8) -> Composition:
        """Make room after node ``index`` and link a new node holding ``value`` there."""
        ...
    def remove(self, index: int, *, duration: float = 0.8) -> Composition:
        """Unlink node ``index`` and close the gap."""
        ...
    def __getattr__(self, name: str) -> Any: ...

class LinkedList(_Structure):
    """Nodes linked by arrows, from ``scene.viz.linked_list``."""
    animate: LinkedListAnimate
    def arrow(self, index: int) -> Drawable:
        """The arrow from node ``index`` to the next one."""
        ...
