from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from typing import Literal, TypeAlias
from .gaanim_core import (
    Anim, BackgroundLike, Composition, Drawable, Layout, Paint, Scene, Segment,
    TextStyle, Transition,
)

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
    def __init__(self, key: str, steps: Sequence[SectionStep], *, title: str | None = None) -> None:
        """Snapshot steps; empty key/steps/title raise ValueError, invalid steps TypeError."""
        ...
    @property
    def key(self) -> str:
        """Stable author-defined identity."""
        ...
    @property
    def title(self) -> str:
        """Display name for agendas; the key when no title was given."""
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

NavigationState: TypeAlias = Literal["done", "current", "upcoming"]

@dataclass(frozen=True)
class NavigationEntry:
    """One section as shown by navigation: stable key, display title, 0-based index."""
    key: str
    title: str
    index: int

SectionLike: TypeAlias = Section | str | tuple[str, str]
"""A Section (key and title), a key used as its own title, or a (key, title) pair."""
SectionTarget: TypeAlias = NavigationEntry | Section | SectionProgress | str | int
"""An entry, its key, its Section or SectionProgress, or its 0-based index."""

class Agenda:
    """Section index whose entries are done, current or upcoming.

    Entries sit ``pitch`` apart down a column or along a row, each placed by
    its left-center anchor. An entry holds one drawable per state stacked in
    place; only the one for its state is visible and transitions cross-fade
    them, so a state may change weight, color, content or shape. Defaults
    use theme colors: ``done`` in ``foreground``, ``current`` in ``accent``
    weight 700, ``upcoming`` in ``muted``. ``styles`` replaces any of them;
    ``item(scene, entry, state)`` replaces the text and is called once per
    entry and state. ``marker(scene)`` builds a drawable placed
    ``marker_gap`` before the current entry that slides with it and is
    hidden while no entry is current. Unknown states and duplicate or empty
    keys raise ``ValueError``; unknown keys ``KeyError``.
    """
    def __init__(
        self,
        scene: Scene,
        sections: Sequence[SectionLike],
        *,
        current: SectionTarget | None = None,
        direction: Literal["column", "row"] = "column",
        pitch: float = 0.5,
        styles: Mapping[NavigationState, TextStyle] | None = None,
        item: Callable[[Scene, NavigationEntry, NavigationState], Drawable] | None = None,
        marker: Callable[[Scene], Drawable] | None = None,
        marker_gap: float = 0.3,
    ) -> None: ...
    @property
    def root(self) -> Drawable:
        """Group of every entry and the marker; move, persist or fade it as a whole."""
        ...
    @property
    def entries(self) -> tuple[NavigationEntry, ...]: ...
    @property
    def marker(self) -> Drawable | None: ...
    @property
    def current(self) -> NavigationEntry | None:
        """Current entry at the authoring cursor, or None before the first section."""
        ...
    def state(self, target: SectionTarget) -> NavigationState:
        """State of an entry at the authoring cursor."""
        ...
    def item(self, target: SectionTarget) -> Drawable:
        """Group holding every state drawable of an entry."""
        ...
    def items(self, state: NavigationState) -> tuple[Drawable, ...]:
        """Entry groups in state at the authoring cursor, in display order."""
        ...
    def variant(self, target: SectionTarget, state: NavigationState) -> Drawable:
        """Drawable shown for an entry while it is in state; style it freely."""
        ...
    def focus(self, target: SectionTarget | None) -> Agenda:
        """Make target current immediately; None marks every entry upcoming."""
        ...
    def advance(self, steps: int = 1) -> Agenda:
        """Move the current entry immediately, clamped to the entries."""
        ...
    @property
    def animate(self) -> AgendaAnimate:
        """Timeline transitions for scene.play; each call also moves the state."""
        ...

class AgendaAnimate:
    def focus(self, target: SectionTarget | None) -> Composition:
        """Cross-fade every entry into its state for target and slide the marker."""
        ...
    def advance(self, steps: int = 1) -> Composition:
        """Like focus, for the entry steps after the current one, clamped."""
        ...

class ProgressRail:
    """Progress track with a clipped fill, section marks or segments, and captions.

    A continuous rail is one track with a mark at every section boundary. A
    ``segmented`` rail has one track and fill per section, ``gap`` apart; a
    section is full once the presentation moves past it. ``captions`` name
    each section above its span, on a shared baseline, colored by state
    (theme ``muted``/``accent``/``muted`` unless ``caption_colors`` says
    otherwise). Track, fill and mark colors default to the theme roles
    ``rule``, ``accent`` and ``muted``. ``track(scene, width, height)``,
    ``mark(scene, entry)`` and ``caption(scene, entry)`` replace default
    shapes; every track is also its fill mask. Horizontal rails fill left to
    right and vertical rails upward. Segments or captions without sections,
    non-positive sizes and unknown orientations raise ``ValueError``.
    """
    def __init__(
        self,
        scene: Scene,
        sections: Sequence[SectionLike] | None = None,
        *,
        length: float = 12.0,
        thickness: float = 0.08,
        orientation: Literal["horizontal", "vertical"] = "horizontal",
        segmented: bool = False,
        gap: float = 0.08,
        value: float = 0.0,
        track_color: Paint | None = None,
        fill_color: Paint | None = None,
        mark_color: Paint | None = None,
        marks: bool = True,
        captions: bool = False,
        caption_style: TextStyle | None = None,
        caption_colors: Mapping[NavigationState, Paint] | None = None,
        caption_gap: float = 0.12,
        label: str | None = None,
        label_style: TextStyle | None = None,
        track: Callable[[Scene, float, float], Drawable] | None = None,
        mark: Callable[[Scene, NavigationEntry], Drawable] | None = None,
        caption: Callable[[Scene, NavigationEntry], Drawable] | None = None,
    ) -> None: ...
    @property
    def root(self) -> Drawable:
        """Group of every part; move, pin as HUD, persist or fade it as a whole."""
        ...
    @property
    def track(self) -> Drawable:
        """Track of a continuous rail, or of the first segment."""
        ...
    @property
    def tracks(self) -> tuple[Drawable, ...]: ...
    @property
    def fill(self) -> Drawable:
        """Fill of a continuous rail, or of the first segment."""
        ...
    @property
    def fills(self) -> tuple[Drawable, ...]: ...
    @property
    def marks(self) -> tuple[Drawable, ...]:
        """Continuous rails: one mark at the start of every section after the first."""
        ...
    @property
    def captions(self) -> tuple[Drawable, ...]: ...
    @property
    def label(self) -> Drawable | None: ...
    def fraction(self, progress: float | SectionProgress) -> float:
        """Whole-rail fraction: numbers clamp to [0, 1]; a SectionProgress of a
        listed section counts the sections before it plus its share of steps."""
        ...
    def to(self, progress: float | SectionProgress) -> ProgressRail:
        """Set fills and caption colors immediately."""
        ...
    def enter(self, target: SectionTarget) -> ProgressRail:
        """Make target current with nothing of it filled yet, immediately."""
        ...
    @property
    def animate(self) -> ProgressRailAnimate: ...

class ProgressRailAnimate:
    def to(self, progress: float | SectionProgress) -> Composition:
        """Animate fills and caption colors to the rail fraction of progress."""
        ...
    def enter(self, target: SectionTarget) -> Composition:
        """Animate to target becoming current with nothing of it filled yet."""
        ...

class SceneSections:
    """Section navigation available as ``scene.sections``."""
    def agenda(
        self,
        sections: Sequence[SectionLike],
        current: SectionTarget | None = None,
        *,
        direction: Literal["column", "row"] = "column",
        pitch: float = 0.5,
        styles: Mapping[NavigationState, TextStyle] | None = None,
        item: Callable[[Scene, NavigationEntry, NavigationState], Drawable] | None = None,
        marker: Callable[[Scene], Drawable] | None = None,
        marker_gap: float = 0.3,
    ) -> Agenda: ...
    def progress_rail(
        self,
        sections: Sequence[SectionLike] | None = None,
        *,
        length: float = 12.0,
        thickness: float = 0.08,
        orientation: Literal["horizontal", "vertical"] = "horizontal",
        segmented: bool = False,
        gap: float = 0.08,
        value: float = 0.0,
        track_color: Paint | None = None,
        fill_color: Paint | None = None,
        mark_color: Paint | None = None,
        marks: bool = True,
        captions: bool = False,
        caption_style: TextStyle | None = None,
        caption_colors: Mapping[NavigationState, Paint] | None = None,
        caption_gap: float = 0.12,
        label: str | None = None,
        label_style: TextStyle | None = None,
        track: Callable[[Scene, float, float], Drawable] | None = None,
        mark: Callable[[Scene, NavigationEntry], Drawable] | None = None,
        caption: Callable[[Scene, NavigationEntry], Drawable] | None = None,
    ) -> ProgressRail: ...
