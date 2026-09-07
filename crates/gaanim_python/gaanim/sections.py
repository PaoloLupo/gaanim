"""Compose presentation sections without wrapping or subclassing Scene."""

from __future__ import annotations

from collections.abc import Callable, Sequence
from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .gaanim_core import BackgroundLike, Layout, Scene, Segment, Transition


@dataclass(frozen=True, kw_only=True)
class SectionStep:
    """Named content builder; the section opens its segment before calling build.

    The builder receives the original Scene and should author content and stops,
    not open additional segments. Transition, notes, template and background
    have the same meaning as their Scene.segment counterparts.
    """

    name: str
    build: Callable[[Scene], None]
    transition: Transition | None = None
    notes: str | None = None
    template: Callable[..., Layout] | None = None
    background: BackgroundLike | None = None

    def __post_init__(self) -> None:
        if not isinstance(self.name, str) or not self.name.strip():
            raise ValueError("step name must be a nonempty string")
        if not callable(self.build):
            raise TypeError("step build must be callable")
        if self.template is not None and not callable(self.template):
            raise TypeError("step template must be callable")


@dataclass(frozen=True)
class SectionProgress:
    """Immutable entry context. index and visit are 1-based; fraction is index/total.

    Stops inside a builder do not change this context. segment is the newly
    opened native handle and can bind template slots.
    """

    key: str
    step: SectionStep
    index: int
    total: int
    visit: int
    segment: Segment

    @property
    def fraction(self) -> float:
        """Content progress in (0, 1], measured on entry rather than elapsed time."""
        return self.index / self.total


class Section:
    """Reusable ordered content section with an explicit stable key.

    Names are scoped by key, visit and ordinal. Reuse the same Section instance
    to repeat a section; each build starts progress afresh. Keys must be unique
    among Section instances used in the same Scene.
    """

    def __init__(self, key: str, steps: Sequence[SectionStep]) -> None:
        if not isinstance(key, str) or not key.strip():
            raise ValueError("section key must be a nonempty string")
        steps = tuple(steps)
        if not steps:
            raise ValueError("a section requires at least one step")
        if not all(isinstance(step, SectionStep) for step in steps):
            raise TypeError("section steps must be SectionStep instances")
        self._key = key
        self._steps = steps
        self._visits = 0

    @property
    def key(self) -> str:
        """Stable section identity supplied by the author."""
        return self._key

    @property
    def steps(self) -> tuple[SectionStep, ...]:
        """Immutable ordered content definitions."""
        return self._steps

    def build(
        self,
        scene: Scene,
        *,
        on_enter: Callable[[Scene, SectionProgress], None] | None = None,
    ) -> tuple[Segment, ...]:
        """Open each segment, call on_enter, then build its content.

        The callback runs at the new segment cursor and may animate persistent
        navigation before content starts. Exceptions propagate; authored work
        is not rolled back, and a failed attempt consumes its visit number.
        Returns native Segment handles in order. No automatic stops are added.
        """
        if on_enter is not None and not callable(on_enter):
            raise TypeError("on_enter must be callable")
        self._visits += 1
        visit = self._visits
        segments = []
        for index, step in enumerate(self.steps, 1):
            segment = scene.segment(
                f"{self.key} · {visit} · {index} · {step.name}",
                step.transition,
                notes=step.notes,
                template=step.template,
                background=step.background,
            )
            segments.append(segment)
            if on_enter is not None:
                on_enter(scene, SectionProgress(
                    self.key, step, index, len(self.steps), visit, segment,
                ))
            step.build(scene)
        return tuple(segments)
