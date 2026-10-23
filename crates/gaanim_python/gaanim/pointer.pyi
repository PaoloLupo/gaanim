from typing import Literal, Sequence

from .gaanim_core import Anim, AnchorName, ColorLike, Composition, Drawable, Zone

PointerStyle = Literal["arrow", "hand", "text"]
PointerTarget = tuple[float, float] | tuple[float, float, float] | Drawable | Zone

class Pointer:
    """A mouse pointer for software demonstrations, from ``scene.ui.pointer``.

    A pointer made at a 2D point moves between points, objects and zones of
    the plane; one made at a 3D point (``at=(x, y, z)``) faces the camera as
    a billboard and moves between 3D points.
    """
    def __init__(self, scene: object, drawable: Drawable, at: tuple[float, ...]) -> None: ...
    @property
    def drawable(self) -> Drawable:
        """The group that draws the pointer, with the tip as its pivot: fade it in, scale it or style it as any drawable."""
        ...
    @property
    def position(self) -> tuple[float, ...]:
        """Where the tip is now: ``(x, y)``, or ``(x, y, z)`` for a 3D pointer."""
        ...
    @property
    def animate(self) -> PointerAnimator:
        """The moves of a demonstration, as animations for ``scene.play``."""
        ...
    def jump(self, target: PointerTarget, *, anchor: AnchorName = "center") -> Pointer:
        """Put the tip on ``target`` at once, without moving there."""
        ...

class PointerAnimator:
    """``pointer.animate``: each method returns something ``scene.play`` takes and updates where the pointer is."""
    def __init__(self, pointer: Pointer) -> None: ...
    def to(self, target: PointerTarget, *, anchor: AnchorName = "center", arc: float = 0.0) -> Anim:
        """Move the tip to ``target``: a point, a ``Drawable`` (its ``anchor``) or a ``Zone``.

        ``arc`` bends the way so that its middle lies that fraction of the
        distance to the left of the straight line (negative to the right);
        0 goes straight. A 3D pointer goes straight. A target in the other
        space (2D or 3D) or an ``arc`` for a 3D pointer raise ``ValueError``.
        """
        ...
    def click(self, *, color: ColorLike | None = None, radius: float = 0.3) -> Composition:
        """A ripple that grows to ``radius`` and fades at the tip while the pointer presses and lets go (0.44 s)."""
        ...
    def drag(self, target: PointerTarget, obj: Drawable | None = None, *, anchor: AnchorName = "center", duration: float = 1.0) -> Composition:
        """Move the tip to ``target`` in ``duration`` seconds while ``obj`` travels the same way."""
        ...
