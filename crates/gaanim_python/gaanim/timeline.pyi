from typing import Literal, Sequence

from .gaanim_core import ColorLike, Drawable, Text

TimelineSide = Literal["above", "below", "auto"]

class TimelineEvent:
    """One event of a timeline, from ``Timeline.event``.

    ``dot`` sits on the axis, ``stem`` runs from it to the label, and
    ``label`` is a group of the knockout plate (when asked for), ``title``
    and ``body``. ``side`` is ``"above"`` or ``"below"`` and ``lane`` the
    lane its label starts in, 0 nearest the axis.
    """
    date: float
    side: str
    lane: int
    dot: Drawable
    stem: Drawable
    label: Drawable
    title: Text
    body: Text | None
    def __init__(self, date: float, side: str, lane: int, dot: Drawable, stem: Drawable, label: Drawable, title: Drawable, body: Drawable | None) -> None: ...
    @property
    def parts(self) -> list[Drawable]:
        """The dot, the stem and the label, in the order a reveal shows them."""
        ...

class Timeline:
    """An axis of dates whose events stack in lanes that do not overlap, from ``scene.viz.timeline``."""
    start: float
    end: float
    y: float
    axis: Drawable
    events: list[TimelineEvent]
    def __init__(self, scene: object, start: float, end: float, *, x_range: tuple[float, float], y: float, breaks: Sequence[tuple[float, float]], lane_height: float, gap: float, label_width: float | None, color: object) -> None: ...
    def x(self, date: float) -> float:
        """The scene x of ``date``, piecewise linear between the breaks; a date outside the timeline raises ``ValueError``."""
        ...
    def event(self, date: float, title: str, body: str | None = None, *, side: TimelineSide = "auto", color: ColorLike | None = None, knockout: bool = True) -> TimelineEvent:
        """Mark ``date`` with a dot, a stem and a label in the first free lane of ``side``.

        The label is ``title`` in bold over ``body``, aligned with the stem.
        It grows to the right, or to the left when it would pass the end of
        the axis, and takes the first lane of its side where it keeps
        ``gap`` from every label and reservation there; a tall label takes
        as many lanes as it needs. ``"auto"`` takes the side whose free lane
        is nearer the axis and alternates when both are as near. With
        ``knockout`` a plate of the background color behind the label hides
        the stems and lines that pass under it. ``color`` paints the dot and
        the stem (the timeline's color by default). When a reservation in
        every lane blocks the way it grows, it grows the other way if it fits
        within the axis there; a label
        that fits on no side it may take raises ``ValueError``.
        """
        ...
    def reserve(self, start: float, end: float, *, side: Literal["above", "below"], lane: int | None = None) -> Timeline:
        """Keep the dates from ``start`` to ``end`` free on ``side``, in one ``lane`` or in every lane, for what is drawn apart."""
        ...
