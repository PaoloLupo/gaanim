"""Compose presentation sections without wrapping or subclassing Scene."""

from __future__ import annotations

from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal, TypeAlias, get_args

if TYPE_CHECKING:
    from .gaanim_core import (
        Anim, BackgroundLike, Composition, Drawable, Easing, Layout, Paint, Parameter,
        Scene, Segment,
        TextStyle, Transition,
    )


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

    def __init__(
        self, key: str, steps: Sequence[SectionStep], *, title: str | None = None,
    ) -> None:
        if not isinstance(key, str) or not key.strip():
            raise ValueError("section key must be a nonempty string")
        if title is not None and (not isinstance(title, str) or not title.strip()):
            raise ValueError("section title must be a nonempty string or None")
        steps = tuple(steps)
        if not steps:
            raise ValueError("a section requires at least one step")
        if not all(isinstance(step, SectionStep) for step in steps):
            raise TypeError("section steps must be SectionStep instances")
        self._key = key
        self._title = title
        self._steps = steps
        self._visits = 0

    @property
    def key(self) -> str:
        """Stable section identity supplied by the author."""
        return self._key

    @property
    def title(self) -> str:
        """Display name for navigation; the key when no title was given."""
        return self._title if self._title is not None else self._key

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


NavigationState: TypeAlias = Literal["done", "current", "upcoming"]
NAVIGATION_STATES: tuple[NavigationState, ...] = get_args(NavigationState)


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


def _navigation_entries(sections: Sequence[object]) -> tuple[NavigationEntry, ...]:
    """Accept Section instances, keys, or (key, title) pairs, in order."""
    if isinstance(sections, (str, bytes)):
        raise TypeError("sections must be a sequence of Section, key or (key, title)")
    entries: list[NavigationEntry] = []
    for index, section in enumerate(sections):
        if isinstance(section, Section):
            key, title = section.key, section.title
        elif isinstance(section, str):
            key = title = section
        elif (isinstance(section, tuple) and len(section) == 2
              and all(isinstance(part, str) for part in section)):
            key, title = section
        else:
            raise TypeError("sections must be a sequence of Section, key or (key, title)")
        if not key.strip():
            raise ValueError("section keys must be nonempty strings")
        if any(entry.key == key for entry in entries):
            raise ValueError(f"section key {key!r} appears more than once")
        entries.append(NavigationEntry(key, title, index))
    if not entries:
        raise ValueError("navigation requires at least one section")
    return tuple(entries)


def _entry_index(entries: tuple[NavigationEntry, ...], target: object) -> int:
    """Resolve an entry, key, Section, SectionProgress or 0-based index."""
    if isinstance(target, (NavigationEntry, Section, SectionProgress)):
        target = target.key
    if isinstance(target, bool):
        raise TypeError("section target must be a key, index, Section or SectionProgress")
    if isinstance(target, int):
        if not 0 <= target < len(entries):
            raise IndexError(f"section index {target} is outside 0..{len(entries) - 1}")
        return target
    if isinstance(target, str):
        for entry in entries:
            if entry.key == target:
                return entry.index
        raise KeyError(f"unknown section key {target!r}")
    raise TypeError("section target must be a key, index, Section or SectionProgress")


def _check_state(state: str) -> NavigationState:
    if state not in NAVIGATION_STATES:
        raise ValueError(f"unknown state {state!r}; expected one of {NAVIGATION_STATES}")
    return state


def _state(index: int, current: int | None) -> NavigationState:
    if current is None or index > current:
        return "upcoming"
    return "current" if index == current else "done"


# Roles of the default "technical" theme, used after `set_theme(None)`.
_FALLBACK_COLORS = {
    "foreground": "#E6E6E6", "muted": "#A0A0A0", "accent": "#F2A541", "rule": "#707070",
}


def _theme_color(scene: Scene, role: str) -> Paint:
    try:
        return scene.canvas.color(role)
    except ValueError:
        return _FALLBACK_COLORS[role]


def _default_agenda_styles(scene: Scene) -> dict[str, TextStyle]:
    from .gaanim_core import TextStyle

    def color(role: str) -> Paint:
        return _theme_color(scene, role)

    return {
        "done": TextStyle(color=color("foreground")),
        "current": TextStyle(color=color("accent"), weight=700),
        "upcoming": TextStyle(color=color("muted")),
    }


class Agenda:
    """Section index whose entries are done, current or upcoming.

    Entries sit ``pitch`` apart (down a column or along a row), left-aligned
    on their anchor. Each entry holds one drawable per state stacked in place;
    only the one for its state is visible and transitions cross-fade them, so
    a state may change weight, color, content or shape. An optional marker
    sits before the current entry and slides with it.
    """

    def __init__(
        self,
        scene: Scene,
        sections: Sequence[object],
        *,
        current: object | None = None,
        direction: str = "column",
        pitch: float = 0.5,
        styles: Mapping[str, TextStyle] | None = None,
        item: Callable[[Scene, NavigationEntry, str], Drawable] | None = None,
        marker: Callable[[Scene], Drawable] | None = None,
        marker_gap: float = 0.3,
    ) -> None:
        from .gaanim_core import Anchor

        if direction not in ("column", "row"):
            raise ValueError('agenda direction must be "column" or "row"')
        if not pitch > 0:
            raise ValueError("agenda pitch must be positive")
        for name, builder in (("item", item), ("marker", marker)):
            if builder is not None and not callable(builder):
                raise TypeError(f"agenda {name} must be callable")
        resolved = _default_agenda_styles(scene)
        for state, style in (styles or {}).items():
            resolved[_check_state(state)] = style
        self._entries = _navigation_entries(sections)
        self._current = None if current is None else _entry_index(self._entries, current)
        self._step = (0.0, -pitch) if direction == "column" else (pitch, 0.0)
        self._variants: list[dict[str, Drawable]] = []
        # Holder opacity is bound to a parameter: animating it then reveals
        # and hides whole drawables, groups included.
        self._visibility: list[dict[str, Parameter]] = []
        self._items: list[Drawable] = []
        members = []
        for entry in self._entries:
            variants: dict[str, Drawable] = {}
            visibility: dict[str, Parameter] = {}
            holders = []
            for state in NAVIGATION_STATES:
                if item is None:
                    drawable = scene.text(entry.title, style=resolved[state])
                else:
                    drawable = item(scene, entry, state)
                variants[state] = drawable.move_to(0, 0, Anchor.LEFT)
                shown = _state(entry.index, self._current) == state
                visibility[state] = scene.viz.parameter(1.0 if shown else 0.0)
                holders.append(scene.geometry.group([drawable]).opacity(visibility[state]))
            x, y = self._position(entry.index)
            group = scene.geometry.group(holders).move_to(x, y, Anchor.LEFT)
            self._variants.append(variants)
            self._visibility.append(visibility)
            self._items.append(group)
            members.append(group)
        self._marker = None
        self._marker_visible = None
        if marker is not None:
            x, y = self._position(0 if self._current is None else self._current)
            self._marker_visible = scene.viz.parameter(0.0 if self._current is None else 1.0)
            self._marker = marker(scene).move_to(x - marker_gap, y)
            members.append(scene.geometry.group([self._marker]).opacity(self._marker_visible))
        self._root = scene.geometry.group(members)

    @property
    def root(self) -> Drawable:
        """Group of every entry and the marker; move, persist or fade it as a whole."""
        return self._root

    @property
    def entries(self) -> tuple[NavigationEntry, ...]:
        """Section entries in display order."""
        return self._entries

    @property
    def marker(self) -> Drawable | None:
        """Marker before the current entry, when a marker builder was given."""
        return self._marker

    @property
    def current(self) -> NavigationEntry | None:
        """Current entry at the authoring cursor, or None before the first section."""
        return None if self._current is None else self._entries[self._current]

    def state(self, target: SectionTarget) -> NavigationState:
        """State of an entry at the authoring cursor: done, current or upcoming."""
        return _state(_entry_index(self._entries, target), self._current)

    def item(self, target: object) -> Drawable:
        """Group holding every state drawable of an entry."""
        return self._items[_entry_index(self._entries, target)]

    def items(self, state: NavigationState) -> tuple[Drawable, ...]:
        """Entry groups in state at the authoring cursor, in display order."""
        _check_state(state)
        return tuple(group for index, group in enumerate(self._items)
                     if _state(index, self._current) == state)

    def variant(self, target: SectionTarget, state: NavigationState) -> Drawable:
        """Drawable shown for an entry while it is in state; style it freely."""
        return self._variants[_entry_index(self._entries, target)][_check_state(state)]

    def focus(self, target: object | None) -> Agenda:
        """Make target current immediately; None marks every entry upcoming."""
        for change in self._changes(self._target(target)):
            change.apply()
        return self

    def advance(self, steps: int = 1) -> Agenda:
        """Move the current entry immediately, clamped to the entries."""
        for change in self._changes(self._advanced(steps)):
            change.apply()
        return self

    @property
    def animate(self) -> AgendaAnimate:
        """Timeline transitions for scene.play; each call also moves the state."""
        return AgendaAnimate(self)

    def _position(self, index: int) -> tuple[float, float]:
        return (self._step[0] * index, self._step[1] * index)

    def _target(self, target: object | None) -> int | None:
        return None if target is None else _entry_index(self._entries, target)

    def _advanced(self, steps: int) -> int:
        if isinstance(steps, bool) or not isinstance(steps, int):
            raise TypeError("steps must be an integer")
        start = -1 if self._current is None else self._current
        return min(max(start + steps, 0), len(self._entries) - 1)

    def _changes(self, current: int | None) -> list[_Change]:
        """Changes from the present state to current, which becomes the state."""
        previous, self._current = self._current, current
        changes = [_Change(parameter, 1.0 if _state(index, current) == state else 0.0)
                   for index, visibility in enumerate(self._visibility)
                   for state, parameter in visibility.items()]
        if self._marker is not None:
            changes.append(_Change(self._marker_visible, 0.0 if current is None else 1.0))
            if current is not None and previous is not None and current != previous:
                (x0, y0), (x1, y1) = self._position(previous), self._position(current)
                changes.append(_Change(self._marker, shift=(x1 - x0, y1 - y0)))
            elif current is not None and previous is None:
                (x0, y0), (x1, y1) = self._position(0), self._position(current)
                if current:
                    changes.append(_Change(self._marker, shift=(x1 - x0, y1 - y0)))
        return changes


class _Change:
    """One parameter value or marker shift, applied immediately or animated."""

    def __init__(self, target: object, value: float | None = None,
                 shift: tuple[float, float] | None = None) -> None:
        self._target = target
        self._value = value
        self._shift = shift

    def apply(self) -> None:
        if self._shift is None:
            self._target.set(self._value)
        else:
            self._target.shift_by(*self._shift)

    def animation(self) -> Anim:
        if self._shift is None:
            return self._target.animate.set(self._value)
        return self._target.animate.shift_by(*self._shift)


class AgendaAnimate:
    """Animated state changes of an Agenda; each call also moves its state."""

    def __init__(self, agenda: Agenda) -> None:
        self._agenda = agenda

    def focus(self, target: object | None, *, easing: Easing | None = None) -> Composition:
        """Cross-fade every entry into its state for target and slide the marker.

        easing applies to every cross-fade and to the marker slide.
        """
        return self._play(self._agenda._target(target), easing)

    def advance(self, steps: int = 1, *, easing: Easing | None = None) -> Composition:
        """Like focus, for the entry steps after the current one, clamped."""
        return self._play(self._agenda._advanced(steps), easing)

    def _play(self, current: int | None, easing: Easing | None) -> Composition:
        from .gaanim_core import parallel

        animations = [change.animation() for change in self._agenda._changes(current)]
        if easing is not None:
            animations = [animation.easing(easing) for animation in animations]
        return parallel(*animations)


class ProgressRail:
    """Progress track with a clipped fill, section marks or segments, and captions.

    A continuous rail is one track with a mark at every section boundary. A
    segmented rail has one track and fill per section, separated by gap; a
    section fills completely once the presentation moves past it. Captions
    name each section above its span and take the color of its state.
    Every part is an ordinary drawable that can be restyled or animated.
    """

    def __init__(
        self,
        scene: Scene,
        sections: Sequence[object] | None = None,
        *,
        length: float = 12.0,
        thickness: float = 0.08,
        orientation: str = "horizontal",
        segmented: bool = False,
        gap: float = 0.08,
        value: float = 0.0,
        track_color: Paint | None = None,
        fill_color: Paint | None = None,
        mark_color: Paint | None = None,
        marks: bool = True,
        captions: bool = False,
        caption_style: TextStyle | None = None,
        caption_colors: Mapping[str, Paint] | None = None,
        caption_gap: float = 0.12,
        label: str | None = None,
        label_style: TextStyle | None = None,
        track: Callable[[Scene, float, float], Drawable] | None = None,
        mark: Callable[[Scene, NavigationEntry], Drawable] | None = None,
        caption: Callable[[Scene, NavigationEntry], Drawable] | None = None,
    ) -> None:
        from .gaanim_core import Anchor, Direction, TextAnchor

        if orientation not in ("horizontal", "vertical"):
            raise ValueError('rail orientation must be "horizontal" or "vertical"')
        if not length > 0 or not thickness > 0:
            raise ValueError("rail length and thickness must be positive")
        for name, builder in (("track", track), ("mark", mark), ("caption", caption)):
            if builder is not None and not callable(builder):
                raise TypeError(f"rail {name} must be callable")
        self._entries = None if sections is None else _navigation_entries(sections)
        if (segmented or captions) and self._entries is None:
            raise ValueError("segments and captions require sections")
        count = len(self._entries) if self._entries is not None else 1
        spans = count if segmented else 1
        if segmented and not 0 <= gap < length / max(count - 1, 1):
            raise ValueError("rail gap must be non-negative and leave room for every segment")
        span = (length - gap * (spans - 1)) / spans if segmented else length
        horizontal = orientation == "horizontal"

        def color(role: str) -> Paint:
            return _theme_color(scene, role)

        self._horizontal = horizontal
        self._count = count

        def along(offset: float) -> tuple[float, float]:
            # Offsets run from the start of the rail; the rail is centered.
            return (offset - length / 2, 0.0) if horizontal else (0.0, offset - length / 2)

        tracks, fills = [], []
        level = _clamp_progress(value)
        for index in range(spans):
            width, height = (span, thickness) if horizontal else (thickness, span)
            if track is None:
                shape = scene.geometry.rounded_rect(width, height, thickness / 2).no_stroke()
                shape.fill(track_color if track_color is not None else color("rule"))
            else:
                shape = track(scene, width, height)
            shape.move_to(*along(index * (span + gap) + span / 2))
            tracks.append(shape)
            fills.append(scene.geometry.fill_level(
                shape,
                fill_color if fill_color is not None else color("accent"),
                self._segment_levels(level, spans)[index],
                # Fills grow from the named edge: left to right, bottom to top.
                direction="left" if horizontal else "up",
                keep_outline=False,
            ))
        self._tracks = tuple(tracks)
        self._fills = tuple(fills)
        parts = [*tracks, *fills]

        self._marks: tuple[Drawable, ...] = ()
        if marks and not segmented and self._entries is not None:
            built = []
            for entry in self._entries[1:]:
                if mark is None:
                    dot = scene.geometry.circle(thickness * 0.9).no_stroke().fill(
                        mark_color if mark_color is not None else color("muted"))
                else:
                    dot = mark(scene, entry)
                built.append(dot.move_to(*along(length * entry.index / count)))
            self._marks = tuple(built)
            parts.extend(built)

        self._caption_colors = {
            "done": color("muted"), "current": color("accent"), "upcoming": color("muted"),
        }
        for state, paint in (caption_colors or {}).items():
            self._caption_colors[_check_state(state)] = paint
        self._captions: tuple[Drawable, ...] = ()
        self._current = self._section_at(level)
        if captions:
            built = []
            section_span = span if segmented else length / count
            section_step = span + gap if segmented else section_span
            for entry in self._entries:
                if caption is None:
                    text = scene.text(entry.title, style=caption_style).fill(
                        self._caption_colors[_state(entry.index, self._current)])
                else:
                    text = caption(scene, entry)
                x, y = along(entry.index * section_step)
                if horizontal:
                    y += thickness / 2 + caption_gap
                else:
                    x += thickness / 2 + caption_gap
                # Default captions share a baseline whatever their descenders.
                text.move_to(x, y, TextAnchor.BASELINE_LEFT if caption is None
                             else Anchor.BOTTOM_LEFT)
                built.append(text)
            self._captions = tuple(built)
            parts.extend(built)

        self._label = None
        if label is not None:
            first = tracks[0]
            if horizontal:
                self._label = scene.text(label, style=label_style).next_to(
                    first, Direction.UP, thickness * 3 + (0.3 if captions else 0.0),
                    aligned_edge=Anchor.LEFT)
            else:
                self._label = scene.text(label, style=label_style).next_to(
                    tracks[-1], Direction.UP, thickness * 3)
            parts.append(self._label)
        self._root = scene.geometry.group(parts)

    @property
    def root(self) -> Drawable:
        """Group of every part; move, persist or fade it as a whole."""
        return self._root

    @property
    def track(self) -> Drawable:
        """Background shape of a continuous rail, or of the first segment."""
        return self._tracks[0]

    @property
    def tracks(self) -> tuple[Drawable, ...]:
        """Background shapes, one per segment; each is also its fill mask."""
        return self._tracks

    @property
    def fill(self) -> Drawable:
        """Fill of a continuous rail, or of the first segment."""
        return self._fills[0]

    @property
    def fills(self) -> tuple[Drawable, ...]:
        """Fills clipped to each track."""
        return self._fills

    @property
    def marks(self) -> tuple[Drawable, ...]:
        """Continuous rails: one mark at the start of every section after the first."""
        return self._marks

    @property
    def captions(self) -> tuple[Drawable, ...]:
        """Section names, one per section, colored by state."""
        return self._captions

    @property
    def label(self) -> Drawable | None:
        """Optional title text."""
        return self._label

    def fraction(self, progress: float | SectionProgress) -> float:
        """Whole-rail fraction for a number in [0, 1] or a SectionProgress.

        A SectionProgress of a listed section counts the sections before it
        plus its own share of steps; any other uses its own fraction.
        """
        if isinstance(progress, SectionProgress):
            for entry in self._entries or ():
                if entry.key == progress.key:
                    return (entry.index + progress.fraction) / len(self._entries)
            return progress.fraction
        return _clamp_progress(progress)

    def to(self, progress: float | SectionProgress) -> ProgressRail:
        """Set the fills and caption colors immediately."""
        for change in self._changes(progress):
            change.apply()
        return self

    def enter(self, target: object) -> ProgressRail:
        """Make a section current with nothing of it filled yet, immediately.

        Sections before it fill completely; it and later sections are empty.
        """
        for change in self._enter(target):
            change.apply()
        return self

    @property
    def animate(self) -> ProgressRailAnimate:
        """Timeline changes of the fills and captions, for scene.play."""
        return ProgressRailAnimate(self)

    def _segment_levels(self, fraction: float, spans: int) -> list[float]:
        return [min(max(fraction * spans - index, 0.0), 1.0) for index in range(spans)]

    def _section_at(self, fraction: float) -> int | None:
        if fraction <= 0.0:
            return None
        # A section is current from just after its start through its end.
        position = fraction * self._count
        return min(max(int(-(-position // 1)) - 1, 0), self._count - 1)

    def _enter(self, target: object) -> list[_RailChange]:
        if self._entries is None:
            raise ValueError("enter requires a rail built with sections")
        index = _entry_index(self._entries, target)
        return self._changes(index / len(self._entries), index)

    def _changes(self, progress: float | SectionProgress,
                 current: int | None = None) -> list[_RailChange]:
        fraction = self.fraction(progress)
        levels = self._segment_levels(fraction, len(self._fills))
        changes = [_RailChange(fill, level=level) for fill, level in zip(self._fills, levels)]
        if current is None and isinstance(progress, SectionProgress):
            keys = [entry.key for entry in self._entries or ()]
            if progress.key in keys:
                current = keys.index(progress.key)
        self._current = self._section_at(fraction) if current is None else current
        changes += [_RailChange(text, paint=self._caption_colors[_state(index, self._current)])
                    for index, text in enumerate(self._captions)]
        return changes


class _RailChange:
    def __init__(self, target: Drawable, *, level: float | None = None,
                 paint: Paint | None = None) -> None:
        self._target = target
        self._level = level
        self._paint = paint

    def apply(self) -> None:
        if self._level is not None:
            self._target.set_fill_level(self._level)
        else:
            self._target.fill(self._paint)

    def animation(self) -> Anim:
        if self._level is not None:
            return self._target.animate.fill_level(self._level)
        return self._target.animate.fill(self._paint)


def _clamp_progress(value: float) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise TypeError("progress must be a number or SectionProgress")
    if value != value:
        raise ValueError("progress must not be NaN")
    return min(max(float(value), 0.0), 1.0)


class ProgressRailAnimate:
    """Animated changes of a ProgressRail."""

    def __init__(self, rail: ProgressRail) -> None:
        self._rail = rail

    def to(self, progress: float | SectionProgress) -> Composition:
        """Animate every fill, and caption colors, to the rail fraction of progress."""
        return self._play(self._rail._changes(progress))

    def enter(self, target: object) -> Composition:
        """Animate to target becoming current with nothing of it filled yet."""
        return self._play(self._rail._enter(target))

    @staticmethod
    def _play(changes: list[_RailChange]) -> Composition:
        from .gaanim_core import parallel

        return parallel(*[change.animation() for change in changes])


class SceneSections:
    """Scene-owned section navigation, available as ``scene.sections``."""

    def __init__(self, scene: Scene) -> None:
        self._scene = scene

    def agenda(self, sections: Sequence[object], current: object | None = None,
               **options: object) -> Agenda:
        """Create an Agenda; see Agenda for options."""
        return Agenda(self._scene, sections, current=current, **options)

    def progress_rail(self, sections: Sequence[object] | None = None,
                      **options: object) -> ProgressRail:
        """Create a ProgressRail; see ProgressRail for options."""
        return ProgressRail(self._scene, sections, **options)
