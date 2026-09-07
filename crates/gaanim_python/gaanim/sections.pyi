from collections.abc import Callable, Sequence
from dataclasses import dataclass
from .gaanim_core import BackgroundLike, Layout, Scene, Segment, Transition

@dataclass(frozen=True, kw_only=True)
class SectionStep:
    """Content definition. Section opens the segment; build receives the original Scene.

    Builders author content and stops, not additional segments. Metadata is
    forwarded to Scene.segment. Empty names raise ValueError; noncallable
    builders or templates raise TypeError.
    """
    name: str
    build: Callable[[Scene], None]
    transition: Transition | None = None
    notes: str | None = None
    template: Callable[..., Layout] | None = None
    background: BackgroundLike | None = None

@dataclass(frozen=True)
class SectionProgress:
    """Entry context; index and visit are 1-based, segment is a native handle."""
    key: str
    step: SectionStep
    index: int
    total: int
    visit: int
    segment: Segment
    @property
    def fraction(self) -> float:
        """index/total in (0, 1]; internal stops do not advance progress."""
        ...

class Section:
    """Ordered authoring section. Reuse an instance for repeated visits.

    Keys must be unique among instances in a Scene. Segment names include the
    key, visit and ordinal. This helper does not add runtime navigation groups.
    """
    def __init__(self, key: str, steps: Sequence[SectionStep]) -> None:
        """Snapshot steps; empty key/steps raise ValueError, invalid steps TypeError."""
        ...
    @property
    def key(self) -> str:
        """Stable author-defined identity."""
        ...
    @property
    def steps(self) -> tuple[SectionStep, ...]:
        """Immutable ordered content definitions."""
        ...
    def build(self, scene: Scene, *, on_enter: Callable[[Scene, SectionProgress], None] | None = None) -> tuple[Segment, ...]:
        """Open segment, call on_enter(scene, progress), then run its builder.

        Returns native handles in order and inserts no stops. Noncallable
        on_enter raises TypeError before authoring. Exceptions propagate without
        rollback; failed attempts consume a visit number. Transition/background
        errors follow Scene.segment, including transitions on the first segment.
        """
        ...
