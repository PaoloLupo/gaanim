"""Typed public API for Gaanim.

The examples in these stubs are intended to be copied into a small Scene
script. All camera durations are in seconds; 3D angles are in radians.
"""

from __future__ import annotations

import os
from typing import Any, Callable, ClassVar, Literal, Mapping, Optional, Self, Sequence, TypeAlias, overload
from .matrix import Matrix
from .sections import SceneSections
from .animation_types import AnimationChannel, CustomAnimationValues

CurvePoint: TypeAlias = tuple[float, float]
"""A coordinate pair used by :meth:`Scene.path` and :meth:`Scene.curve`."""

CurveControl: TypeAlias = CurvePoint | Literal["auto"] | None
"""A Bézier control point, an automatically reflected handle, or a collapsed handle."""

CurveCommand: TypeAlias = tuple[str, Sequence[CurvePoint | CurveControl]]
"""A ``Scene.path`` or ``Scene.curve`` command and its arguments."""

class EasingCurve:
    """A discoverable curve family used by the typed ``Easing`` factories."""
    QUADRATIC: ClassVar[EasingCurve]
    CUBIC: ClassVar[EasingCurve]
    QUARTIC: ClassVar[EasingCurve]
    QUINTIC: ClassVar[EasingCurve]
    EXPONENTIAL: ClassVar[EasingCurve]
    SINE: ClassVar[EasingCurve]
    CIRCULAR: ClassVar[EasingCurve]
    BACK: ClassVar[EasingCurve]
    ELASTIC: ClassVar[EasingCurve]
    BOUNCE: ClassVar[EasingCurve]

class Easing:
    """An immutable, IDE-discoverable animation timing function.

    Use a preset such as ``Easing.SMOOTH`` or a validated factory such as
    ``Easing.spring(stiffness=90, damping=12)``. Instances only describe
    interpolation and never mutate an animation or timeline.
    """
    LINEAR: ClassVar[Easing]
    SMOOTH: ClassVar[Easing]
    DOUBLE_SMOOTH: ClassVar[Easing]
    THERE_AND_BACK: ClassVar[Easing]
    LINGERING: ClassVar[Easing]
    RUNNING_START: ClassVar[Easing]
    EXPONENTIAL_DECAY: ClassVar[Easing]
    NOT_QUITE_THERE: ClassVar[Easing]
    SMOOTH_SPRING: ClassVar[Easing]
    """Critically damped spring: fastest settle without overshoot."""
    GENTLE: ClassVar[Easing]
    """Soft, unhurried spring with a barely visible overshoot."""
    QUICK: ClassVar[Easing]
    """Brisk spring with a small overshoot that settles early."""
    SNAPPY: ClassVar[Easing]
    """Fast spring with a crisp 20% overshoot."""
    BOUNCY: ClassVar[Easing]
    """Playful spring with a 45% overshoot and visible rebounds."""
    @staticmethod
    def ease_in(curve: EasingCurve) -> Easing: ...
    @staticmethod
    def ease_out(curve: EasingCurve) -> Easing: ...
    @staticmethod
    def ease_in_out(curve: EasingCurve) -> Easing: ...
    @staticmethod
    def spring(
        stiffness: Optional[float] = None,
        damping: Optional[float] = None,
        *,
        mass: float = 1.0,
        velocity: float = 0.0,
        bounce: Optional[float] = None,
    ) -> Easing:
        """Create a spring, physically or perceptually.

        With ``bounce`` in ``[0, 1)`` the spring is perceptual: its peak
        overshoot is ``bounce`` of the distance travelled (0 is critically
        damped) and it settles within the animation's duration, so
        ``duration`` alone sets the pace. Otherwise ``stiffness`` (default 90)
        must be positive, ``damping`` (default 12) non-negative, and ``mass``
        positive; the clip spans five physical seconds as before.
        ``velocity`` is the initial speed in distances per clip duration.
        Every spring ends exactly on its target. Invalid values, or
        ``bounce`` combined with physical parameters, raise ``ValueError``.

        Example:
            logo.animate.scale_to(1.0).duration(0.6).easing(Easing.spring(bounce=0.35))
        """
        ...
    @staticmethod
    def back(overshoot: float = 1.70158, *, mode: Literal["in", "out", "in_out"] = "out") -> Easing:
        """Pull back (``in``) or overshoot (``out``) by a non-negative ``overshoot``."""
        ...
    @staticmethod
    def elastic(amplitude: float = 1.0, period: float = 0.3, *, mode: Literal["in", "out", "in_out"] = "out") -> Easing:
        """Oscillate like a plucked band; ``amplitude >= 1``, ``period > 0`` in clip fractions."""
        ...
    @staticmethod
    def bounce(strength: float = 1.0, *, mode: Literal["in", "out", "in_out"] = "out") -> Easing:
        """Bounce on arrival; ``strength`` blends from a cubic ease (0) to the classic bounce (1)."""
        ...
    @staticmethod
    def slow_mo(linear_ratio: float = 0.7, power: float = 0.7) -> Easing:
        """Rush in, glide through a slow linear middle, rush out (both arguments in ``[0, 1]``)."""
        ...
    @staticmethod
    def rough(strength: float = 1.0, points: int = 20, seed: int = 0) -> Easing:
        """Deterministic jittery ramp through ``points`` random knots, for flicker and glitches."""
        ...
    @staticmethod
    def squish(easing: Easing, start: float, end: float) -> Easing:
        """Run ``easing`` only between ``start`` and ``end`` (``0 <= start < end <= 1``), holding its ends outside."""
        ...
    @staticmethod
    def from_svg(path: str, samples: int = 256) -> Easing:
        """Sample an SVG path drawn in the unit square (x = time, y = progress).

        The path must run from x = 0 to x = 1 without moving backwards in x
        and stay within y in ``[-1, 2]``; otherwise ``ValueError`` is raised.

        Example:
            Easing.from_svg("M0,0 C0.3,0 0.2,1.2 1,1")
        """
        ...
    @staticmethod
    def steps(count: int, jump: Literal["start", "end", "none", "both"] = "end") -> Easing:
        """Create a discrete easing with CSS ``steps()`` jump positions."""
        ...
    @staticmethod
    def mirror(easing: Easing) -> Easing: ...
    @staticmethod
    def there_and_back(pause: float = 0.0) -> Easing:
        """Rise and return, optionally pausing at the peak for a fraction in ``[0, 1]``."""
        ...
    @staticmethod
    def cubic_bezier(x1: float, y1: float, x2: float, y2: float) -> Easing:
        """Create a finite CSS-style cubic Bézier; both X controls must be in ``[0, 1]``."""
        ...
    @staticmethod
    def custom(function: Callable[[float], float], samples: int = 256) -> Easing:
        """Sample ``function`` once into a lookup table interpolated linearly.

        ``function`` is called ``samples`` times, at evenly spaced ``t`` from
        0 to 1, when the easing is created; rendering never calls back into
        Python, so previews, seeks and exports agree. Values must be finite
        and within ``[-1, 2]`` (overshoot is allowed); otherwise, or when
        ``samples`` is outside ``[2, 65536]``, ``ValueError`` is raised.

        Example:
            Easing.custom(lambda t: 1 - (1 - t) ** 4, samples=256)
        """
        ...

class Color:
    @overload
    def __init__(self, value: str) -> None: ...
    @overload
    def __init__(self, r: int, g: int, b: int, a: int = 255) -> None: ...
    @staticmethod
    def from_hex(value: str) -> Color: ...
    @staticmethod
    def from_rgb(r: int, g: int, b: int) -> Color: ...
    @staticmethod
    def from_rgba(r: int, g: int, b: int, a: int) -> Color: ...
    @staticmethod
    def from_hsl(h: float, s: float, l: float, a: float = 1.0) -> Color:
        """Create HSL color; saturation, lightness, and alpha use 0..1."""
        ...
    @staticmethod
    def from_oklch(l: float, c: float, h: float, a: float = 1.0) -> Color:
        """Create perceptual OKLCH color; lightness and alpha use 0..1."""
        ...

    """A CSS Color 4 or explicit RGBA color.

    Examples include ``Color("#0f172a")``, ``Color("oklch(62% .2 260)")``
    and ``Color(15, 23, 42)``. Invalid syntax or component ranges raise
    ``ValueError``.
    """

ColorLike: TypeAlias = Color | str | tuple[int, int, int] | tuple[int, int, int, int]

class ColorMap:
    """A continuous or categorical map from normalized values to colors.

    Built-ins include the canonical Matplotlib and Scientific Colour Maps.
    Names are case-insensitive.
    """
    def __init__(self, name: str) -> None: ...
    @staticmethod
    def named(name: str) -> ColorMap: ...
    @staticmethod
    def from_colors(colors: Sequence[ColorLike], positions: Optional[Sequence[float]] = None) -> ColorMap: ...
    @staticmethod
    def names(category: Optional[Literal["matplotlib", "scientific"]] = None) -> list[str]: ...
    @property
    def name(self) -> Optional[str]: ...
    @property
    def category(self) -> Optional[str]: ...
    @property
    def categorical(self) -> bool: ...
    def sample(self, position: float) -> Color: ...
    def colors(self, count: int) -> list[Color]: ...
    def reversed(self) -> ColorMap: ...
    def with_alpha(self, alpha: float) -> ColorMap: ...

ColorMapLike: TypeAlias = ColorMap | str

class Brush:
    @staticmethod
    def solid(color: ColorLike) -> Brush:
        """Use solid on this Brush or create the requested value.

        Example:
            result = Brush.solid(BLUE)
        """
        ...
    @staticmethod
    def linear(
        colors: Sequence[ColorLike],
        *,
        start: tuple[float, float],
        end: tuple[float, float],
        extend: Literal["pad", "repeat", "reflect"] = "pad",
    ) -> Brush:
        """Use linear on this Brush or create the requested value.

        Example:
            result = Brush.linear([BLUE], start=(0.0, 0.0), end=(0.0, 0.0))
        """
        ...
    @staticmethod
    def radial(
        colors: Sequence[ColorLike],
        *,
        center: tuple[float, float] = (0.0, 0.0),
        radius: float,
        extend: Literal["pad", "repeat", "reflect"] = "pad",
    ) -> Brush:
        """Use radial on this Brush or create the requested value.

        Example:
            result = Brush.radial([BLUE], radius=40.0)
        """
        ...
    @staticmethod
    def sweep(
        colors: Sequence[ColorLike],
        *,
        center: tuple[float, float] = (0.0, 0.0),
        start_angle: float = 0.0,
        end_angle: float = 360.0,
        extend: Literal["pad", "repeat", "reflect"] = "pad",
    ) -> Brush:
        """Use sweep on this Brush or create the requested value.

        Example:
            result = Brush.sweep([BLUE])
        """
        ...

Paint: TypeAlias = ColorLike | Brush

class Background:
    def __init__(self, paint: Paint) -> None:
        """Wrap a solid or gradient Brush for use inside the full scene bounds."""
        ...
    @staticmethod
    def shader(source: str | os.PathLike[str], *, fallback: Optional[ColorLike] = None) -> Background:
        """Create a timeline-driven WGSL scene background.

        A string is inline WGSL. An ``os.PathLike`` value loads a WGSL asset
        immediately. ``source`` must define ``gaanim_background(uv, resolution,
        time)`` returning ``vec4<f32>``. ``time`` is the absolute timeline
        position in seconds, so preview, seeks, snapshots, and exports are deterministic.
        UV coordinates are normalized from the top-left and the shader covers
        the authored scene bounds at their effective viewport resolution.
        Legacy two-argument shaders remain accepted as static backgrounds.
        Invalid WGSL raises ``ValueError`` and an unreadable asset raises
        ``RuntimeError``. ``fallback`` is used outside the scene bounds, by
        native 3D clears, and if rasterization is unavailable.
        """
        ...
    @property
    def fallback(self) -> Color:
        """Return the representative clear and contrast color."""
        ...

BackgroundLike: TypeAlias = Paint | Background

class PostProcess:
    @staticmethod
    def shader(source: str | os.PathLike[str]) -> PostProcess:
        """Create a WGSL post-process applied to the rendered 2D scene.

        A string is inline WGSL; an ``os.PathLike`` value loads a WGSL asset
        immediately. ``source`` must define ``gaanim_post(uv: vec2<f32>,
        resolution: vec2<f32>, time: f32) -> vec4<f32>`` and may call
        ``gaanim_scene(uv)`` to sample everything drawn in 2D: background,
        text, shapes, images, and Lottie. ``uv`` is ``(0, 0)`` at the top-left
        corner of the camera frame, ``resolution`` is the frame size in output
        pixels, and ``time`` is absolute timeline seconds. Sampled colors are
        straight-alpha sRGB values and the result is clamped to ``[0, 1]``.
        Only the camera frame is processed; perspective 3D scenes and SVG
        output are drawn without it. Invalid WGSL raises ``ValueError`` and an
        unreadable asset raises ``RuntimeError``.
        """
        ...
    @property
    def source(self) -> str:
        """Return the WGSL source of the post-process function."""
        ...

class StrokeStyle:
    def __init__(
        self,
        paint: Paint,
        width: float = 0.02,
        *,
        cap: Literal["butt", "round", "square"] = "round",
        join: Literal["bevel", "miter", "round"] = "round",
        miter_limit: float = 4.0,
        dashes: Sequence[float] = (),
        dash_offset: float = 0.0,
    ) -> None:
        """Define a complete reusable stroke; invalid metrics raise ValueError."""
        ...

class Style:
    def __init__(
        self,
        *,
        fill: Optional[Paint] = None,
        stroke: Optional[StrokeStyle] = None,
        opacity: Optional[float] = None,
        text: Optional[TextStyle] = None,
    ) -> None:
        """Define a property-wise theme rule; strings may name theme tokens."""
        ...

class AxesStyle:
    def __init__(
        self,
        *,
        axis: Optional[StrokeStyle] = None,
        grid: Optional[StrokeStyle] = None,
        minor_grid: Optional[StrokeStyle] = None,
        ticks: Optional[StrokeStyle] = None,
        numbers: Optional[TextStyle] = None,
        labels: Optional[TextStyle] = None,
    ) -> None:
        """Define axis-part strokes and typography under an ``axes`` selector."""
        ...

class Theme:
    """Reusable semantic colors, typography, fonts, and Layout v2 tokens."""
    def __init__(
        self,
        base: Optional[str | Theme] = None,
        *,
        name: Optional[str] = None,
        colors: Optional[dict[str, ColorLike]] = None,
        fonts: Optional[dict[str, str]] = None,
        sizes: Optional[dict[str, float]] = None,
        text: Optional[dict[TextRole, TextStyle]] = None,
        styles: Optional[dict[str, Style | AxesStyle]] = None,
        series: Optional[Sequence[ColorLike]] = None,
        heatmap: Optional[Sequence[ColorLike]] = None,
        layout: Optional[dict[str, float]] = None,
        font_files: Optional[dict[str, str]] = None,
        font_dir: Optional[str | os.PathLike[str]] = None,
        text_markup: Optional[bool] = None,
    ) -> None:
        """Create or derive a centralized visual theme.

        Rules use family/type/part selectors or ``.classes``. Text values reuse
        the structured ``TextStyle`` overlay.

        ``font_dir`` embeds every ``.ttf``, ``.otf``, ``.ttc`` and ``.otc``
        file directly inside the directory (not subdirectories). Each face is
        resolved by the family, weight and style its file declares, so
        ``fonts={"text": "Inter"}`` with ``weight=700`` finds the bold file
        without naming it. ``font_files`` still registers single files.

        ``text_markup=False`` makes ``*`` and ``_`` literal by default in
        ``scene.text``, ``scene.text.measure``, ``badge`` and ``chip``; a call
        that passes ``markup=`` keeps its own choice. ``None`` keeps the
        base theme's value (``True`` for a new theme).

        Invalid selectors, tokens, roles or metrics, a ``font_dir`` without
        font files, or an unreadable font raise ``ValueError``; a missing
        file or directory raises ``OSError``.

        Example:
            Theme("paper", font_dir="assets/fonts", fonts={"text": "Inter"},
                  text_markup=False)
        """
        ...
    @property
    def text_markup(self) -> bool:
        """Default markup mode for text created while this theme is active."""
        ...
    @property
    def name(self) -> str:
        """Read the name value from this Theme.

        Example:
            value = theme.name
        """
        ...
    @staticmethod
    def schemes() -> list[str]:
        """Use schemes on this Theme or create the requested value.

        Example:
            result = Theme.schemes()
        """
        ...
    def color(self, role: str) -> Color:
        """Use color on this Theme or create the requested value.

        Example:
            result = theme.color("foreground")
        """
        ...
    def layout_token(self, name: str) -> float:
        """Return a named layout token in canvas units or raise ``ValueError``."""
        ...
    def validate(self) -> list[str]:
        """Use validate on this Theme or create the requested value.

        Example:
            result = theme.validate()
        """
        ...

class Anchor:
    CENTER: ClassVar[Anchor]
    TOP: ClassVar[Anchor]
    BOTTOM: ClassVar[Anchor]
    LEFT: ClassVar[Anchor]
    RIGHT: ClassVar[Anchor]
    TOP_LEFT: ClassVar[Anchor]
    TOP_RIGHT: ClassVar[Anchor]
    BOTTOM_LEFT: ClassVar[Anchor]
    BOTTOM_RIGHT: ClassVar[Anchor]

class TextAnchor:
    """Horizontal reference point on a Text object's typographic baseline."""
    BASELINE_LEFT: ClassVar[TextAnchor]
    BASELINE_CENTER: ClassVar[TextAnchor]
    BASELINE_RIGHT: ClassVar[TextAnchor]

class MatrixOrder:
    """Native deterministic ordering used by high-level matrix selections."""
    @staticmethod
    def order(rows: int, columns: int, coordinates: Sequence[tuple[int, int]], order: str, seed: int = 0) -> list[tuple[int, int]]:
        """Return selected zero-based coordinates in the requested seeded order."""
        ...

class AnchorPoint:
    """Non-rendered endpoint bound to a drawable's local bounds."""

class PointRef:
    """Non-rendered reactive XY point derived from endpoints or scalar expressions."""

class Direction:
    UP: ClassVar[Direction]
    DOWN: ClassVar[Direction]
    LEFT: ClassVar[Direction]
    RIGHT: ClassVar[Direction]
    UP_LEFT: ClassVar[Direction]
    UP_RIGHT: ClassVar[Direction]
    DOWN_LEFT: ClassVar[Direction]
    DOWN_RIGHT: ClassVar[Direction]
    @staticmethod
    def custom(x: float, y: float, z: float = 0.0) -> Direction:
        """Use custom on this Direction or create the requested value.

        Example:
            result = Direction.custom(1.0, 1.0)
        """
        ...

SizeRule: TypeAlias = float | Literal["hug", "fill"]
Track: TypeAlias = float | Literal["auto"] | str
Padding: TypeAlias = float | tuple[float, float] | tuple[float, float, float, float]
Align: TypeAlias = Literal["start", "center", "end", "stretch"]
Justify: TypeAlias = Literal["start", "center", "end", "between", "around", "evenly"]
Fit: TypeAlias = Literal["none", "contain", "cover", "stretch", "scale_down"]

class LayoutExpression:
    """Linear drawable geometry expression used to build Layout v2 constraints.

    Expressions may be added or subtracted and scaled only by finite scalars.
    Combining drawables from different scenes raises ``ValueError``.
    """
    def __add__(self, other: float | LayoutExpression) -> LayoutExpression: ...
    def __sub__(self, other: float | LayoutExpression) -> LayoutExpression: ...
    def __mul__(self, scalar: float) -> LayoutExpression: ...
    def __truediv__(self, scalar: float) -> LayoutExpression: ...
    def __eq__(self, other: object) -> LayoutConstraint: ...  # type: ignore[override]
    def __le__(self, other: float | LayoutExpression) -> LayoutConstraint: ...
    def __ge__(self, other: float | LayoutExpression) -> LayoutConstraint: ...

class LayoutConstraint:
    """Required or prioritized linear relation between drawable geometry."""
    def strong(self) -> LayoutConstraint:
        """Return a strong-priority copy of this constraint."""
        ...
    def medium(self) -> LayoutConstraint:
        """Return a medium-priority copy of this constraint."""
        ...
    def weak(self) -> LayoutConstraint:
        """Return a weak-priority copy reported by layout diagnostics if violated."""
        ...
    def named(self, label: str) -> LayoutConstraint:
        """Return a copy carrying ``label`` in conflict diagnostics."""
        ...

class ConstraintSet:
    """Handle returned after a scene registers one or more constraints."""
    count: int

class LayoutItem:
    """Immutable per-child grow, grid, absolute-placement, offset, and fit rules."""

class Layout(Drawable):
    def move_to(self, x: ScalarSource | tuple[float, float] | Drawable | AnchorPoint, y: Optional[ScalarSource] = None, anchor: Optional[Anchor] = None) -> Self:
        """Position the container using Drawable semantics and preserve this Layout."""
        ...
    def shift_by(self, dx: float, dy: float) -> Self:
        """Shift the container in scene units and preserve its layout methods."""
        ...
    @property
    def background(self) -> Optional[Drawable]:
        """Card background for independent styling; None for ordinary layouts.

        This child follows the outer layout box, including padding, and is
        excluded from count and content measurement.
        """
        ...
    @property
    def animate(self) -> Anim:
        """Return the pure animation proxy for the layout root."""
        ...
    """Persistent row, column, grid, or stack that owns child translation.

    Layout is itself a ``Drawable``: positioning, anchor, scale, rotation, and
    edge-placement methods transform the complete resolved container and all
    descendants. Reflow preserves those root transforms. Positional fluent
    methods on managed children raise ``LayoutOwnershipError``; use
    ``configure_item`` offsets.
    """
    count: int
    def add(self, child: Drawable | Layout | LayoutItem, *, at: Optional[int] = None) -> Drawable:
        """Insert a direct child immediately and return it.

        Raises ``IndexError`` for an invalid index and ``LayoutOwnershipError``
        when the child is positioned manually, foreign, or already managed.
        """
        ...
    def remove(self, child: Drawable | Layout) -> None:
        """Remove a direct child and release its positional ownership."""
        ...
    def detach(self, child: Drawable | Layout) -> None:
        """Release a direct child from the layout without hiding it.

        The child preserves its world position, opacity, and scene membership,
        so positional methods such as ``move_to`` are valid immediately after
        this call. The remaining children reflow immediately. A non-member
        raises ``ValueError``.

        Example:
            scene.reuse(title)
            page.detach(title)
            scene.play([title.animate.move_to(0.0, 200.0)])
        """
        ...
    def replace(self, old: Drawable | Layout, new: Drawable | Layout | LayoutItem) -> Drawable:
        """Replace a direct child, returning the replacement after optional reflow."""
        ...
    def reflow(self) -> None:
        """Resolve external geometry changes immediately."""
        ...
    def configure(
        self,
        *,
        gap: Optional[float] = None,
        padding: Optional[Padding] = None,
        width: Optional[SizeRule] = None,
        height: Optional[SizeRule] = None,
        min_width: Optional[float] = None,
        max_width: Optional[float] = None,
        min_height: Optional[float] = None,
        max_height: Optional[float] = None,
        aspect_ratio: Optional[float] = None,
        align: Optional[Align] = None,
        justify: Optional[Justify] = None,
        wrap: Optional[bool] = None,
        within: Optional[Literal["safe", "frame"]] = None,
    ) -> None:
        """Update container rules and queue deterministic reflow.

        Numeric geometry uses canvas units; ``aspect_ratio`` must be positive.
        ``wrap`` is valid only for rows and columns. Invalid values raise
        ``ValueError``.
        """
        ...
    def configure_item(self, child: Drawable | Layout, *, grow: Optional[float] = None, shrink: Optional[float] = None, align: Optional[Align] = None, row: Optional[int] = None, column: Optional[int] = None, row_span: Optional[int] = None, column_span: Optional[int] = None, absolute: Optional[bool] = None, anchor: Optional[Anchor] = None, offset: Optional[tuple[float, float]] = None, fit: Optional[Fit] = None) -> None:
        """Update direct-child rules and immediately apply the resulting reflow."""
        ...
    def diagnostics(self) -> list[str]:
        """Return soft-constraint diagnostics associated with this layout root."""
        ...

class LayoutOwnershipError(Exception):
    """Raised when a Layout cannot take or retain ownership of child position."""

class Transition:
    @staticmethod
    def cut() -> Transition:
        """Create a cut transition.

        Example:
            result = Transition.cut()
        """
        ...
    @staticmethod
    def cross_fade(duration: float) -> Transition:
        """Create a cross fade transition.

        Example:
            result = Transition.cross_fade(1.0)
        """
        ...
    @staticmethod
    def fade_through(duration: float, color: Color) -> Transition:
        """Create a fade through transition.

        Example:
            result = Transition.fade_through(1.0, BLUE)
        """
        ...
    @staticmethod
    def slide(duration: float, direction: str) -> Transition:
        """Create a slide transition.

        Example:
            result = Transition.slide(1.0, "right")
        """
        ...
    @staticmethod
    def zoom_through(
        duration: float,
        *,
        center: tuple[float, float] = (0.0, 0.0),
        max_zoom: float = 4.0,
    ) -> Transition:
        """Create a zoom through transition.

        Example:
            result = Transition.zoom_through(1.0)
        """
        ...
    @staticmethod
    def morph(duration: float, *, pairs: Sequence[tuple[Drawable, Drawable]] = ()) -> Transition:
        """Carry paired drawables from the outgoing segment into the incoming one.

        Each ``(source, target)`` pair shares one bounding box that travels
        from the source's box to the target's while the target fades in over
        the first half and the source fades out over the second, so the pair
        reads as one object changing place, size, color and shape. Unpaired
        content cross-fades. Pairs are resolved when the scene compiles, so
        pass them to ``scene.link`` once both segments are declared. Raises
        ``ValueError`` for a non-positive duration, a drawable paired with
        itself, or a drawable repeated on one side.

        Example:
            scene.link(overview, detail, Transition.morph(0.8, pairs=[(card, panel)]))
        """
        ...

class Anim:
    def crop(self, x: float, y: float, width: float, height: float, *, normalized: bool = False) -> Anim:
        """Animate an Image/Video source rectangle inside its fixed frame.

        Pixels use the original source's top-left origin; normalized=True uses
        fractions of original dimensions. Reject invalid rectangles or non-media
        targets with ValueError. Position, rotation and scale remain independent.
        """
        ...
    def custom(self, callback: Callable[[float], CustomAnimationValues], *, channels: Sequence[AnimationChannel]) -> Anim:
        """Describe a pure custom animation without scheduling or sampling it.

        The callback receives eased progress at the exact requested time and
        returns absolute local values for exactly the declared channels. It
        must be synchronous, deterministic, and must not mutate the scene.
        Easing may overshoot 0..1. Use duration/delay/easing and composition as
        with native animations; combine other setters using ``parallel``.
        Invalid channels, conflicting writes, non-finite values, or mutations
        raise errors. A runtime failure restores this clip's initial channel
        values and reports a diagnostic; exports fail explicitly.

        Example:
            scene.play(dot.animate.custom(lambda a: {"position": (3*a, a*a)}, channels=("position",)).duration(2))
        """
        ...
    def fill(self, color: Paint) -> Anim:
        """Target a vector fill paint in a compound ``Drawable.animate`` animation.

        Text glyphs interpolate independently from their current fills, so
        fragment-specific colors converge to this target. On ``Primitive3D``
        this targets the PBR material base color instead and requires a solid.
        Same-kind gradients interpolate geometry and normalized color stops;
        a solid can transition to a gradient. Incompatible gradient kinds raise
        ``ValueError``. Text selections retain their solid-color contract.
        """
        ...

    def stroke(self, color: Paint, width: float) -> Anim:
        """Target vector stroke paint and width, including every text glyph.

        Paint interpolation follows ``fill``; incompatible gradient kinds
        raise ``ValueError``. This is unavailable for ``Primitive3D``.
        """
        ...
    def material(self, material: Material3D) -> Anim:
        """Target every animatable PBR channel of a native Primitive3D."""
        ...
    def opacity(self, value: ScalarSource) -> Anim:
        """Target drawable opacity, clamped to the 0..1 range.

        Reactive targets are evaluated at the clip start and frozen for this
        interpolation. An already linked target channel must first be fixed
        with a numeric setter, or animated through its Parameter.
        """
        ...
    def set(self, value: float) -> Anim:
        """Target a finite Parameter or Variable value without scheduling it."""
        ...
    def transform_to(self, target: Drawable) -> Anim:
        """Morph in place to a same-scene target.

        Text transforms adopt the target's measured typographic baseline at
        the endpoint, including equations with scripts or limits. Absent fill
        and composed timing are preserved.
        """
        ...
    def fill_level(self, level: float) -> Anim:
        """Animate an unbound fill drawable to a normalized value in [0, 1].

        A bound fill raises ValueError: animate its source or first call
        set_fill_level(number) to end the binding.
        """
        ...
    def shift_by(self, dx: float, dy: float) -> Anim:
        """Target a relative 2D translation in a compound property animation."""
        ...
    @overload
    def move_to(self, reference: Drawable, /) -> Anim: ...
    @overload
    def move_to(self, point: AnchorPoint, /) -> Anim: ...
    @overload
    def move_to(self, x: ScalarSource, y: ScalarSource, anchor: Optional[Anchor] = None) -> Anim:
        """Target an absolute 2D position, placing ``anchor`` at ``(x, y)``.

        Omitting ``anchor`` uses ``Anchor.CENTER``. The returned animation can
        be chained with other property targets.


        Reactive targets are evaluated at the clip start and frozen for this
        interpolation. An already linked target channel must first be fixed
        with a numeric setter, or animated through its Parameter.
        """
        ...
    def shift_by_3d(self, dx: float, dy: float, dz: float) -> Anim:
        """Target a relative 3D translation in scene units."""
        ...
    def move_to_3d(self, x: ScalarSource, y: ScalarSource, z: ScalarSource) -> Anim:
        """Target an absolute 3D position in scene units.

        Reactive targets are evaluated at the clip start and frozen for this
        interpolation. An already linked target channel must first be fixed
        with a numeric setter, or animated through its Parameter.
        """
        ...
    def scale_by(self, factor: float) -> Anim:
        """Multiply the current uniform scale by ``factor``."""
        ...
    def scale_to(self, factor: ScalarSource) -> Anim:
        """Target an absolute uniform scale.

        Reactive targets are evaluated at the clip start and frozen for this
        interpolation. An already linked target channel must first be fixed
        with a numeric setter, or animated through its Parameter.
        """
        ...
    def scale_to_3d(self, x: ScalarSource, y: ScalarSource, z: ScalarSource) -> Anim:
        """Target absolute scale independently on three axes.

        Reactive targets are evaluated at the clip start and frozen for this
        interpolation. An already linked target channel must first be fixed
        with a numeric setter, or animated through its Parameter.
        """
        ...
    def scale_by_3d(self, x: float, y: float, z: float) -> Anim:
        """Multiply the current scale independently on three axes."""
        ...
    def rotate_by(self, radians: float) -> Anim:
        """Target a relative Z rotation in radians."""
        ...
    def rotate_to(self, radians: ScalarSource) -> Anim:
        """Target an absolute Z rotation in radians.

        Reactive targets are evaluated at the clip start and frozen for this
        interpolation. An already linked target channel must first be fixed
        with a numeric setter, or animated through its Parameter.
        """
        ...
    def rotate_by_3d(self, axis: Literal["x", "y", "z"], radians: float) -> Anim:
        """Target a relative rotation around one 3D axis."""
        ...
    def rotate_to_3d(self, x: ScalarSource, y: ScalarSource, z: ScalarSource) -> Anim:
        """Target an absolute XYZ Euler orientation in radians.

        Reactive targets are evaluated at the clip start and frozen for this
        interpolation. An already linked target channel must first be fixed
        with a numeric setter, or animated through its Parameter.
        """
        ...
    def fade_in(self) -> Anim:
        """Select the drawable fade-in effect; scheduling occurs in ``Scene.play``.

        The drawable stays hidden before the scheduled fade, including when
        declared after earlier animations or placed inside a group.
        """
        ...
    def fade_in_from(self, direction: Direction, distance: float = 0.48) -> Anim: ...
    def fade_out(self) -> Anim:
        """Select the drawable fade-out effect; scheduling occurs in ``Scene.play``."""
        ...
    def write(self, *, by: Literal["grapheme", "word", "line", "part"] = "grapheme", order: Literal["forward", "reverse", "center", "random"] = "forward", stagger: Optional[float] = None) -> Anim:
        """Trace paths with a constant logical stroke, then smoothly fade their fills.

        On text, ``by`` starts the glyphs of each grapheme, word, explicit line,
        or innermost semantic part together, with the segmentation of
        ``text.words``, ``text.lines``, and ``text.parts``; punctuation joins the
        neighboring group. ``order`` starts groups ``"forward"``, ``"reverse"``,
        from the ``"center"`` outward, or in a fixed ``"random"`` permutation.
        ``stagger`` is the lag ratio between groups: ``None`` uses adaptive
        sequential staggering and a number overrides it. A missing outline is
        synthesized at 0.03 logical units and removed as the authored fill
        appears. Raises ``ValueError`` after a property target or another effect.
        """
        ...
    def create(self) -> Anim:
        """Trace vector paths, then smoothly fade closed-shape fills.

        The trace occupies 70% of the configured duration and the fill uses a
        smooth alpha fade over the final 30%. Authored stroke width stays
        constant; a missing outline uses a temporary 0.03-logical-unit stroke.
        ``DoubleSmooth`` is the default easing. Mesh creation keeps its
        scale-and-fade behavior.
        """
        ...
    def unwrite(self) -> Anim: ...
    def uncreate(self) -> Anim: ...
    def grow_from_center(self) -> Anim: ...
    def grow_from_point(self, x: float, y: float) -> Anim:
        """Grow from zero scale while the scene point ``(x, y)`` stays fixed.

        The drawable scales up about that point and ends at its declared
        position and size. Raises ``ValueError`` for non-finite coordinates.

        Example:
            scene.play(badge.animate.grow_from_point(2, 1))
        """
        ...
    def grow_from_edge(self, direction: Direction) -> Anim:
        """Grow from zero scale while one side of the bounds stays fixed.

        ``Direction.DOWN`` keeps the bottom edge midpoint in place, so a bar
        rises from its baseline; diagonal directions pin a corner, and
        ``Direction.custom`` pins the matching point on the bounding box.

        Example:
            scene.play(bar.animate.grow_from_edge(Direction.DOWN))
        """
        ...
    def grow_arrow(self) -> Anim:
        """Grow an arrow from its tail, like Manim's ``GrowArrow`` but undistorted.

        The tail stays fixed and the tip travels along the arrow's straight or
        curved spine (the head turns along the arc tangent). The head emerges
        with its proportions intact over the first head length, then keeps its
        authored size while the shaft extends; stroke width never changes.
        Applies to ``scene.geometry.arrow``, ``curved_arrow`` and
        ``curved_arrow_arc``. Any other drawable, or an arrow reshaped by a
        transform, falls back to ``create()``. ``Smooth`` is the default easing.
        Raises ``TypeError`` on a text-selection proxy.
        """
        ...
    def shrink_to_center(self) -> Anim: ...
    def spin_in_from_nothing(self) -> Anim: ...
    def draw_border_then_fill(self) -> Anim: ...
    def circumscribe(self) -> Anim: ...
    def flash(self) -> Anim: ...
    def show_passing_flash(self, *, time_width: float = 0.2) -> Anim: ...
    def move_along(
        self,
        target: Drawable,
        *,
        orient: bool = False,
        rotate_offset: float = 0.0,
        start: float = 0.0,
        end: float = 1.0,
    ) -> Anim:
        """Travel along ``target``'s outline, optionally turning with it.

        With ``orient=True`` the drawable's rotation follows the path tangent
        plus ``rotate_offset`` radians, so a plane or arrow points where it
        goes. ``start`` and ``end`` select the travelled portion as arc-length
        fractions (``0 <= start < end <= 1``); otherwise ``ValueError``.

        Example:
            scene.play(plane.animate.move_along(route, orient=True).duration(3))
        """
        ...
    def glow(self, color: Optional[Color] = None, radius: float = 0.16, intensity: float = 1.0) -> Anim:
        """Animate the glow toward ``color``/``radius``/``intensity``; ``None`` fades it out.

        A drawable without glow grows it from zero intensity. Combines with
        other property targets such as ``scale_to``. Invalid values raise
        ``ValueError``; text selections raise ``TypeError``.

        Example:
            scene.play(orb.animate.glow(CYAN, radius=0.5, intensity=2.0).repeat(3, yoyo=True))
        """
        ...
    def blur(self, sigma: float = 0.04) -> Anim:
        """Animate the blur to ``sigma``; ``0`` ends sharp, which makes a blur-in.

        Example:
            hero.blur(0.3)
            scene.play(hero.animate.blur(0.0).duration(0.6))
        """
        ...
    def shadow(self, color: Optional[Color] = None, x: float = 0.08, y: float = -0.08, blur: float = 0.06) -> Anim:
        """Animate the drop shadow; ``None`` fades it out and a new one grows from under the drawable.

        Example:
            scene.play(card.animate.shadow(BLACK, 0, -0.25, 0.4).scale_to(1.04))
        """
        ...
    def trim(self, start: Optional[float] = None, end: Optional[float] = None, offset: Optional[float] = None) -> Anim:
        """Animate the visible window of the drawn path (see ``Drawable.trim``).

        Omitted values keep their current setting. ``offset`` slides the
        window and wraps around the path, so animating it makes a segment
        travel. Values outside ``[0, 1]`` for ``start``/``end`` raise
        ``ValueError``.

        Example:
            scene.play(ring.animate.trim(start=0.0, end=1.0))
        """
        ...
    def reveal(
        self,
        style: Optional[Literal["slide_up", "slide_down", "fade", "scale", "blur"]] = None,
        *,
        by: Literal["grapheme", "word", "line", "part"] = "line",
        mask: bool = True,
        stagger: float = 0.06,
    ) -> Anim:
        """Reveal a Text unit by unit, ``stagger`` seconds apart.

        ``style`` defaults to ``"slide_up"``: each unit rises one row height
        from behind a vector mask clipped to its row, so it works in SVG
        export too. ``"slide_down"`` falls from above; with ``mask=False``
        slides travel less and fade in instead of hiding behind the mask.
        ``"fade"``, ``"scale"`` and ``"blur"`` ignore ``mask``. Units are
        graphemes, words, explicit lines (``text.lines``) or semantic parts.
        ``easing`` eases each unit (ease-out cubic by default) and the
        duration covers the whole cascade; a stagger that does not fit is
        compressed. Glyphs hold their hidden state until the reveal starts,
        as with ``fade_in``. Seeks are exact.

        On a text selection (``text["x"].animate``) this is the selection
        reveal: ``style`` is ``"fade"`` (default), ``"wipe"`` or
        ``"from_below"`` and ``by``/``mask``/``stagger`` raise ``TypeError``.
        Non-Text drawables raise ``TypeError``; unknown styles or units raise
        ``ValueError``.

        Example:
            scene.play(title.animate.reveal(by="line", style="slide_up", stagger=0.06))
            scene.play(quote.animate.reveal(by="word", style="blur", stagger=0.04))
        """
        ...
    def conceal(
        self,
        style: Literal["slide_up", "slide_down", "fade", "scale", "blur"] = "slide_up",
        *,
        by: Literal["grapheme", "word", "line", "part"] = "line",
        mask: bool = True,
        stagger: float = 0.06,
    ) -> Anim:
        """Exit symmetric to ``reveal``: units leave in reading order and stay hidden.

        ``"slide_up"`` sends each unit up behind its row mask. Raises the
        same errors as ``reveal``.

        Example:
            scene.play(headline.animate.conceal(by="line", style="slide_up"))
        """
        ...
    def blur_in(
        self,
        sigma: float = 0.3,
        *,
        by: Literal["grapheme", "word", "line", "part"] = "grapheme",
        stagger: float = 0.02,
    ) -> Anim:
        """Bring a Text in unit by unit from a transparent Gaussian blur.

        ``sigma`` is the starting blur in scene units (the same effect as
        ``blur``) and ``stagger`` the delay in seconds between units; each
        unit clears its blur and fades in with an ease-out. Glyphs hold the
        blurred, transparent state until the animation starts. Non-Text drawables raise ``TypeError``; a negative
        ``sigma`` or ``stagger`` raises ``ValueError``.

        Example:
            scene.play(title.animate.blur_in(sigma=0.3, by="grapheme", stagger=0.02))
        """
        ...
    def tracking(self, value: float) -> Anim:
        """Animate the extra space between neighboring glyphs to ``value`` scene units.

        Glyphs shift along the text's baseline without a new layout: rows
        grow from their left edge, center or right edge according to the
        text alignment, and ``0`` restores the original spacing. Default
        easing is smooth. It combines with text animations that do not move
        glyphs, such as ``blur_in``; ``scene.play`` rejects two simultaneous
        animations that write the same glyph channel (for example ``tracking``
        with a sliding ``reveal``). Non-Text drawables raise ``TypeError``.

        Example:
            title.tracking(0.4)
            scene.play(title.animate.tracking(0.0).duration(1.2))
        """
        ...
    def path_arc(self, angle: float) -> Anim:
        """Travel this animation's ``move_to``/``shift_by`` along a circular arc.

        The arc turns by ``angle`` radians (positive is counterclockwise)
        between the start and end positions instead of the straight line.
        Without a translation target it raises ``ValueError``.

        Example:
            scene.play(ball.animate.move_to(4, 0).path_arc(math.pi / 3))
        """
        ...
    def fade_transform_to(self, target: Drawable) -> Anim:
        """Cross-fade to a same-scene target at the animation's composed start time."""
        ...
    def replacement_transform_to(self, target: Drawable) -> Anim:
        """Morph to and replace with a same-scene target, preserving absent fill and composed timing."""
        ...
    def indicate(self) -> Anim: ...
    def wiggle(self) -> Anim: ...
    def duration(self, seconds: float) -> Anim:
        """Return a copy configured to last finite, non-negative ``seconds``."""
        ...
    def easing(self, easing: Easing) -> Anim:
        """Return a copy using a typed timing function; scheduling remains deferred."""
        ...
    def delay(self, seconds: float) -> Anim:
        """Return a copy delayed by finite, non-negative ``seconds``."""
        ...
    def repeat(self, count: int, *, yoyo: bool = False, delay: float = 0.0) -> Anim:
        """Play this animation ``count`` times; ``duration`` and ``easing`` describe one cycle.

        With ``yoyo=True`` every other cycle plays backwards, so an even
        ``count`` ends where it started and later animations continue from
        there. ``delay`` seconds separate cycles. The total duration is
        ``count * duration + (count - 1) * delay``, which ``play`` and
        ``Composition.schedule`` use. ``count`` outside ``[1, 10000]`` or a
        negative ``delay`` raises ``ValueError``.

        Example:
            badge.animate.scale_to(1.08).duration(0.4).repeat(4, yoyo=True, delay=0.1)
        """
        ...
    def loop(self, mode: Literal["cycle", "pingpong", "offset"] = "cycle", *, until: float, delay: float = 0.0) -> Anim:
        """Repeat for as many whole cycles as fit in ``until`` seconds (at least one).

        ``cycle`` restarts each cycle, ``pingpong`` alternates direction, and
        ``offset`` continues from where the previous cycle ended, so
        ``rotate_by`` or ``shift_by`` keep accumulating. The loop stays finite,
        so seeks and exports are exact. Invalid values raise ``ValueError``.

        Example:
            arrow.animate.shift_by(0.3, 0).duration(0.5).loop("pingpong", until=4.0)
        """
        ...
    def lag_ratio(self, value: float) -> Anim:
        """Configure this animation with lag ratio.

        Example:
            result = animation.lag_ratio(1.0)
        """
        ...
    def stroke_width(self, value: float) -> Anim:
        """Target stroke width in a property animation or configure a draw animation.

        Example:
            result = animation.stroke_width(1.0)
        """
        ...
    def with_pen_tip(self) -> Anim:
        """Configure this animation with with pen tip.

        Example:
            result = animation.with_pen_tip()
        """
        ...
    def pivot(self, x: float, y: float) -> Anim:
        """Configure this animation with pivot.

        Example:
            result = animation.pivot(1.0, 1.0)
        """
        ...
    def about_point(self, x: float, y: float) -> Anim:
        """Configure this animation with about point.

        Example:
            result = animation.about_point(1.0, 1.0)
        """
        ...

class ScheduleEntry:
    """One immutable leaf in a locally resolved composition schedule."""
    @property
    def path(self) -> tuple[int, ...]: ...
    @property
    def kind(self) -> Literal["animation", "audio", "video", "lottie"]: ...
    @property
    def start(self) -> float: ...
    @property
    def duration(self) -> Optional[float]: ...
    @property
    def end(self) -> Optional[float]: ...

class Schedule:
    """Read-only timing inspection that neither schedules nor consumes leaves."""
    @property
    def span(self) -> float: ...
    @property
    def entries(self) -> tuple[ScheduleEntry, ...]: ...

class Composition:
    """Pure, immutable tree of animations and timeline-synchronized media."""
    def delay(self, seconds: float) -> Composition:
        """Return a copy whose complete subtree starts after ``seconds``."""
        ...
    def defaults(self, *, duration: Optional[float] = None, easing: Optional[Easing] = None) -> Composition:
        """Fill duration and easing only on descendant animations without overrides."""
        ...
    def stretch(self, seconds: float) -> Composition:
        """Rescale an animation-only subtree to an exact finite span; media are rejected."""
        ...
    def repeat(self, count: int, *, delay: float = 0.0) -> Composition:
        """Play the whole animation-only subtree ``count`` times, ``delay`` seconds apart.

        Each repetition starts from the state the previous one left, so
        relative animations accumulate. Media, ``count < 1`` or a negative
        ``delay`` raise ``ValueError``.

        Example:
            scene.play(parallel(a.animate.rotate_by(TAU), b.animate.shift_by(1, 0)).repeat(2))
        """
        ...
    def schedule(self, *, duration: Optional[float] = None) -> Schedule:
        """Resolve local offsets using the supplied outer defaults without scheduling."""
        ...

# Typing-only in this native stub; import it at runtime with ``from gaanim import Playable``.
Playable: TypeAlias = Anim | Audio | Video | VideoSegment | Lottie | Composition

def parallel(*items: Playable) -> Composition:
    """Compose one or more items at the same local origin."""
    ...

def sequence(*items: Playable, gap: float = 0.0) -> Composition:
    """Compose items consecutively; a bounded negative gap creates overlap."""
    ...

StaggerOrigin: TypeAlias = Literal["start", "end", "center", "edges", "random"] | tuple[float, float]

def stagger(
    *items: Playable,
    each: float = 0.1,
    total: Optional[float] = None,
    origin: Optional[StaggerOrigin] = None,
    grid: Optional[Literal["auto"] | tuple[int, int]] = None,
    easing: Optional[Easing] = None,
    seed: int = 0,
) -> Composition:
    """Offset items by ``index * each`` seconds, or by distance from ``origin``.

    With ``origin``, ``grid``, ``total`` or ``easing`` the delay of each item
    grows with its distance from ``origin``: the first item (``"start"``), the
    last (``"end"``), the center of the items (``"center"``), the outer edges
    moving inward (``"edges"``), a seeded random order (``"random"``), or an
    ``(x, y)`` scene point. Distances use the items' declared positions
    (``grid="auto"``, the default) or cells of an explicit
    ``grid=(rows, columns)``. ``each`` is the delay per spacing step and
    ``total`` instead fixes the whole spread; ``easing`` shapes it. Items
    whose position depends on a layout fall back to their index.

    Example:
        scene.play(stagger(*[d.animate.grow_from_center() for d in dots], each=0.03, origin="center"))
    """
    ...

def distribute(
    items: Sequence[Drawable],
    low: float,
    high: float,
    *,
    origin: Optional[StaggerOrigin] = None,
    grid: Optional[Literal["auto"] | tuple[int, int]] = None,
    easing: Optional[Easing] = None,
    seed: int = 0,
) -> list[float]:
    """Spread values from ``low`` to ``high`` over ``items`` by distance from ``origin``.

    Uses the same ordering as ``stagger`` and returns one value per item, so
    it distributes sizes, colors or opacities instead of start times.

    Example:
        for dot, size in zip(dots, distribute(dots, 0.4, 1.4, origin="edges")):
            dot.scale_by(size)
    """
    ...

class Audio:
    """A validated audio declaration activated explicitly by ``Scene.play``.

    Audio declarations are bound to their creating scene. A finite ``duration``
    contributes to the enclosing play duration; an open-ended declaration
    starts as background audio without extending the timeline.
    """

class Voiceover:
    """A voiceover block created by ``Scene.voiceover``.

    Markers are named instants of the take. A marker resolves, in order, from
    the times tapped while recording, then the Whisper transcript (the marker
    name must be the spoken word or phrase; case, accents and punctuation are
    ignored), then its proportional position in the block's text. Markers are
    searched after the previous one, so a repeated word matches its next
    occurrence; asking again for the same name returns the same time. A
    marker found nowhere warns and does not wait.
    """
    def __enter__(self) -> Voiceover: ...
    def __exit__(self, exc_type: object, exc: object, traceback: object) -> bool:
        """Wait for the rest of the take unless the block raised."""
        ...
    def wait_until(self, marker: str) -> None:
        """Advance the scene cursor to ``marker``.

        A marker already behind the cursor, or not found, does not wait.

        Raises:
            ValueError: If the block has finished.
        """
        ...
    def until(self, marker: str) -> float:
        """Seconds from the scene cursor to ``marker``, never negative.

        Use it as an animation duration that ends exactly on the marker.

        Example:
            scene.play([arrow.animate.move_to(2, 0)], duration=vo.until("pendiente"))
        """
        ...
    def finish(self) -> None:
        """Wait for the rest of the take and close the block; idempotent."""
        ...
    @property
    def key(self) -> str:
        """Take name, the file stem inside ``narration/``."""
        ...
    @property
    def text(self) -> Optional[str]:
        """Script of the block: ``text`` or the segment notes."""
        ...
    @property
    def start(self) -> float:
        """Absolute timeline second where the take starts."""
        ...
    @property
    def duration(self) -> float:
        """Take length in seconds, measured or estimated from the text."""
        ...
    @property
    def end(self) -> float:
        """Absolute timeline second where the take ends."""
        ...
    @property
    def remaining(self) -> float:
        """Seconds from the scene cursor to the end of the take."""
        ...
    @property
    def recorded(self) -> bool:
        """Whether an audio file backs the take; ``False`` means estimated."""
        ...

class Updater:
    """Preset updater — attach to a DrawableHandle via add_updater()."""
    @staticmethod
    def orbit(cx: float, cy: float, radius: float, speed: float) -> Updater:
        """Create an updater that will orbit the drawable each frame.

        Example:
            result = Updater.orbit(1.0, 1.0, 1.0, 1.0)
        """
        ...
    @staticmethod
    def advance_x(speed: float) -> Updater:
        """Create an updater that will advance x the drawable each frame.

        Example:
            result = Updater.advance_x(1.0)
        """
        ...
    @staticmethod
    def bob(amplitude: float, frequency: float) -> Updater:
        """Create an updater that will bob the drawable each frame.

        Example:
            result = Updater.bob(1.0, 1.0)
        """
        ...
    @staticmethod
    def rotate(speed: float) -> Updater:
        """Create an updater that will rotate the drawable each frame.

        Example:
            result = Updater.rotate(1.0)
        """
        ...
    @staticmethod
    def wiggle(
        *,
        position: float = 0.08,
        rotation: float = 0.0,
        scale: float = 0.0,
        frequency: float = 2.0,
        octaves: int = 2,
        seed: int = 0,
    ) -> Updater:
        """Layer organic, seeded jitter over the drawable's own animation.

        ``position`` (scene units), ``rotation`` (radians) and ``scale``
        (fraction) are the noise amplitudes; ``frequency`` sets how fast the
        jitter changes and ``octaves`` (1 to 8) adds finer detail. The offset
        starts at zero, is a pure function of timeline time, and adds to
        ``animate.move_to`` and other clips instead of replacing them.
        ``remove_updater()`` ends it. Invalid values raise ``ValueError``.

        Example:
            logo.add_updater(Updater.wiggle(position=0.08, rotation=0.03, frequency=2.0, seed=1))
        """
        ...
    @staticmethod
    def oscillate(
        channel: Literal["x", "y", "rotation", "scale", "opacity"],
        *,
        waveform: Literal["sine", "square", "triangle", "saw"] = "sine",
        frequency: float = 1.0,
        low: float = 0.0,
        high: float = 1.0,
        phase: float = 0.0,
    ) -> Updater:
        """Layer a periodic value between ``low`` and ``high`` on one channel.

        ``x``, ``y`` and ``rotation`` values are added to the animated value;
        ``scale`` and ``opacity`` values multiply it (opacity factors must lie
        in ``[0, 1]``). Every waveform starts at ``low``; ``phase`` shifts it
        in cycles. Like ``wiggle`` it is a pure function of timeline time and
        combines with animations. Invalid values raise ``ValueError``.

        Example:
            light.add_updater(Updater.oscillate("opacity", waveform="triangle", frequency=0.5, low=0.4, high=1.0))
        """
        ...
    @staticmethod
    def pulse(min_scale: float, max_scale: float, frequency: float) -> Updater:
        """Create an updater that will pulse the drawable each frame.

        Example:
            result = Updater.pulse(1.0, 1.0, 1.0)
        """
        ...

class Drawable:
    left: LayoutExpression
    right: LayoutExpression
    top: LayoutExpression
    bottom: LayoutExpression
    center_x: LayoutExpression
    center_y: LayoutExpression
    width: LayoutExpression
    height: LayoutExpression
    def part(self, id: str) -> Drawable:
        """Return a named SVG part or glTF node by unique name/canonical path."""
        ...

    def parts(self) -> tuple[str, ...]: ...
    def animations(self) -> tuple[str, ...]: ...
    @property
    def animate(self) -> Anim:
        """Start a typed compound property animation.

        Chain transform, opacity, fill, stroke, color, or material targets and
        pass the result to ``scene.play``. All selected channels share timing
        and easing and run concurrently.

        Example:
            scene.play([drawable.animate.move_to(120, 0).fill(BLUE).duration(1.5)])
        """
        ...
    def animation(
        self,
        name: str,
        *,
        duration: Optional[float] = None,
        speed: float = 1.0,
        loop: bool = False,
        reverse: bool = False,
        transition: float = 0.0,
        start_time: float = 0.0,
    ) -> Anim:
        """Sample a Blender Action deterministically on the scene timeline."""
        ...
    def fill(self, paint: Paint) -> Self:
        """Apply fill to this drawable and return the result.

        Example:
            result = drawable.fill(BLUE)
        """
        ...
    def no_fill(self) -> Self:
        """Apply no fill to this drawable and return the result.

        Example:
            result = drawable.no_fill()
        """
        ...
    def stroke(self, paint: Paint, width: float) -> Self:
        """Apply a stroke whose width is measured in logical scene units.

        On an imported SVG root, the width remains logical even when the
        source hierarchy is scaled to fit the scene.

        Example:
            result = drawable.stroke(BLUE, 1.0)
        """
        ...
    def stroke_style(self, style: StrokeStyle) -> Self:
        """Apply complete stroke geometry and return this drawable."""
        ...
    def no_stroke(self) -> Self:
        """Apply no stroke to this drawable and return the result.

        Example:
            result = drawable.no_stroke()
        """
        ...
    def style_class(self, name: str) -> Self:
        """Attach an ordered theme class; explicit fluent styles still win."""
        ...
    def glow(self, color: Color, radius: float = 0.16, intensity: float = 1.0) -> Drawable:
        """Apply glow to this drawable and return the result.

        Example:
            result = drawable.glow(BLUE)
        """
        ...
    def trim(
        self,
        start: Optional[float] = None,
        end: Optional[float] = None,
        offset: Optional[float] = None,
        mode: Optional[Literal["simultaneous", "sequential"]] = None,
    ) -> Self:
        """Show only the window ``[start, end]`` of the drawn path, shifted by ``offset``.

        Values are arc-length fractions; omitted ones keep their current
        setting (initially ``0``, ``1`` and ``0``). ``offset`` wraps around
        the end of the path. ``"simultaneous"`` (default) trims every
        sub-path and every drawn descendant at once; ``"sequential"`` trims
        the total length, so sub-paths appear one after another. Animate it
        with ``animate.trim(...)``.

        Example:
            logo.trim(end=0.0)
            scene.play(logo.animate.trim(end=1.0).duration(1.2))
        """
        ...
    def blur(self, sigma: float = 0.04) -> Drawable:
        """Apply blur to this drawable and return the result.

        Example:
            result = drawable.blur()
        """
        ...
    def shadow(
        self,
        color: Color,
        x: float = 0.08,
        y: float = -0.08,
        blur: float = 0.06,
    ) -> Drawable:
        """Apply shadow to this drawable and return the result.

        Example:
            result = drawable.shadow(BLUE)
        """
        ...
    def no_effects(self) -> Drawable:
        """Apply no effects to this drawable and return the result.

        Example:
            result = drawable.no_effects()
        """
        ...
    def clip(
        self,
        mask: Drawable,
        rule: Literal["nonzero", "evenodd"] = "nonzero",
        invert: bool = False,
    ) -> Drawable:
        """Dynamically clip this drawable to a vector mask and return it.

        The mask keeps following its own geometry and transforms. ``invert``
        uses the area outside the mask; invalid fill rules raise ``ValueError``.

        Example:
            result = drawable.clip(mask)
        """
        ...
    def no_clip(self) -> Drawable:
        """Apply no clip to this drawable and return the result.

        Example:
            result = drawable.no_clip()
        """
        ...
    def set_fill_level(self, level: ScalarSource) -> Drawable:
        """Set or bind this fill's normalized level and return the drawable.

        A reactive source follows its value at every seek; finite values are
        clamped to [0, 1]. A fixed number in [0, 1] ends the binding reversibly
        at the current cursor. Invalid fixed values, foreign sources and other
        drawable types raise ValueError. Animate the source while bound.
        """
        ...
    def opacity(self, op: ScalarSource) -> Self:
        """Apply opacity to this drawable and return the result.

        Example:
            result = drawable.opacity(1.0)

        Opacity multiplies down the hierarchy: a group's opacity scales its
        members without changing their own values, exactly as
        ``group.animate.opacity(...)`` does, so a group hidden with
        ``opacity(0)`` reappears when its opacity is animated back up.

        A reactive source binds this channel from the current cursor; another
        source replaces it and a numeric setter ends the binding reversibly.
        Sources must belong to this Scene. Animate the source Parameter while
        linked; direct animation or relative writes to this channel error.
        """
        ...
    def z_index(self, z: int) -> Self:
        """Set the stacking layer; higher values draw on top.

        A group's or text's ``z_index`` is added to every descendant's, so it
        moves the whole subtree. Ties keep creation order.

        Example:
            card = scene.geometry.group([box, label]).z_index(5)
        """
        ...
    @overload
    def move_to(self, reference: Drawable, /) -> Self: ...
    @overload
    def move_to(self, point: AnchorPoint, /) -> Self: ...
    @overload
    def move_to(self, x: ScalarSource, y: ScalarSource, anchor: Optional[Anchor] = None) -> Self:
        """Place this drawable at coordinates, another drawable, or an anchor point.

        Omitting ``anchor`` places ordinary drawables by their visual center.
        Coordinate-system roots instead place their authored local origin, so
        labels cannot displace the mathematical axes. The optional anchor can
        be passed positionally or by keyword. Passing one ``Drawable`` creates a deferred
        center-to-center layout relation; use ``follow`` or ``attach_to`` when
        the target must continue following an animated reference. Passing an
        ``AnchorPoint`` places this drawable's center on that transformed local
        anchor during initial layout. References and anchor points cannot be
        combined with ``y`` or ``anchor``.

        Example:
            result = drawable.move_to(1.0, 1.0, Anchor.TOP_LEFT)
            centered = label.move_to(drawable)
            corner_label = label.move_to(drawable.anchor_point(Anchor.TOP_RIGHT))


        A reactive source binds this channel from the current cursor; another
        source replaces it and a numeric setter ends the binding reversibly.
        Sources must belong to this Scene. Animate the source Parameter while
        linked; direct animation or relative writes to this channel error.
        """
        ...
    def anchor_point(
        self,
        anchor: Optional[Anchor] = None,
        *,
        offset: tuple[float, float] = (0.0, 0.0),
    ) -> AnchorPoint:
        """Create a reactive endpoint on this drawable.

        ``anchor`` selects one of the nine normalized local-bounds points;
        ``offset`` is measured in local scene units and follows parent
        translation, rotation, and scale. Non-finite offsets raise
        ``ValueError``.

        Example:
            corner = frame.anchor_point(Anchor.TOP_RIGHT)
        """
        ...
    def with_port(self, name: str, anchor: Anchor, *, offset: tuple[float, float] = (0.0, 0.0)) -> Self:
        """Define a unique named local anchor and return this same drawable.

        Empty/duplicate names and nonfinite offsets raise ValueError. A port
        follows bounds, reflow and parent transforms; definitions are immutable.
        """
        ...
    def port(self, name: str) -> AnchorPoint:
        """Return a named reactive endpoint; an unknown name raises KeyError."""
        ...
    def at_coordinate(self, coordinate: CoordinateRef) -> Drawable:
        """Place this drawable at a symbolic coordinate owned by a coordinate space."""
        ...
    def move_to_3d(self, x: ScalarSource, y: ScalarSource, z: ScalarSource) -> Self:
        """Place the drawable at a 3D world-space position.

        Coordinates are interpreted by the perspective camera. The method is
        chainable and returns the same ``Drawable``.

        Example:
            dot = scene.dot(8).fill(RED).move_to_3d(1.0, 2.0, 0.5)


        A reactive source binds this channel from the current cursor; another
        source replaces it and a numeric setter ends the binding reversibly.
        Sources must belong to this Scene. Animate the source Parameter while
        linked; direct animation or relative writes to this channel error.
        """
        ...
    def shift_by(self, dx: float, dy: float) -> Self:
        """Apply a relative 2D translation as an immediate cursor cut."""
        ...
    def shift_by_3d(self, dx: float, dy: float, dz: float) -> Self:
        """Apply a relative 3D translation as an immediate cursor cut."""
        ...
    def billboard(self) -> Self:
        """Keep a 3D drawable facing the perspective camera.

        This is useful for labels and markers attached to a 3D scene. The
        method is chainable and returns the same ``Drawable``.

        Example:
            label = scene.text("origin").move_to_3d(0.0, 1.0, 0.0).billboard()
        """
        ...
    def hud(self) -> Self:
        """Pin the drawable to the screen as a fixed HUD overlay.

        HUD drawables use screen-space coordinates and are not affected by
        the 3D camera. Use ``.move_to(x, y)`` after ``.hud()`` to position them in
        the viewport. The method is chainable and returns the same
        ``Drawable``.

        Example:
            title = scene.text("glTF demo").hud().move_to(0.0, 300.0)
        """
        ...
    def scale_by(self, factor: float) -> Self:
        """Multiply uniform scale immediately at the current cursor."""
        ...
    def scale_to(self, factor: ScalarSource) -> Self:
        """Apply scaled to this drawable and return the result.

        Example:
            result = drawable.scale_to(1.0)


        A reactive source binds this channel from the current cursor; another
        source replaces it and a numeric setter ends the binding reversibly.
        Sources must belong to this Scene. Animate the source Parameter while
        linked; direct animation or relative writes to this channel error.
        """
        ...
    def scale_by_3d(self, x: float, y: float, z: float) -> Self:
        """Multiply per-axis scale immediately at the current cursor."""
        ...
    def scale_to_3d(self, x: ScalarSource, y: ScalarSource, z: ScalarSource) -> Self:
        """Scale independently on three axes and preserve the specialized handle.

        Example:
            label = scene.text("depth").scale_to_3d(1.0, 1.0, 0.5)


        A reactive source binds this channel from the current cursor; another
        source replaces it and a numeric setter ends the binding reversibly.
        Sources must belong to this Scene. Animate the source Parameter while
        linked; direct animation or relative writes to this channel error.
        """
        ...
    def rotate_by(self, radians: float) -> Self:
        """Apply a relative Z rotation immediately at the current cursor."""
        ...
    def rotate_to(self, radians: ScalarSource) -> Self:
        """Apply rotated to this drawable and return the result.

        Example:
            result = drawable.rotate_to(1.0)


        A reactive source binds this channel from the current cursor; another
        source replaces it and a numeric setter ends the binding reversibly.
        Sources must belong to this Scene. Animate the source Parameter while
        linked; direct animation or relative writes to this channel error.
        """
        ...
    def rotate_by_3d(self, axis: Literal["x", "y", "z"], radians: float) -> Self:
        """Apply a relative 3D axis rotation immediately at the current cursor."""
        ...
    def rotate_to_3d(self, x: ScalarSource, y: ScalarSource, z: ScalarSource) -> Self:
        """Apply Euler rotation in radians and preserve the specialized handle.

        Example:
            label = scene.text("axis").rotate_to_3d(0.0, 0.0, 0.5)


        A reactive source binds this channel from the current cursor; another
        source replaces it and a numeric setter ends the binding reversibly.
        Sources must belong to this Scene. Animate the source Parameter while
        linked; direct animation or relative writes to this channel error.
        """
        ...
    def with_pivot(self, x: float, y: float) -> Self:
        """Apply with pivot to this drawable and return the result.

        Example:
            result = drawable.with_pivot(1.0, 1.0)
        """
        ...
    def with_pivot_3d(self, x: float, y: float, z: float) -> Self:
        """Set a three-dimensional transform pivot and preserve the handle type.

        Example:
            label = scene.text("orbit").with_pivot_3d(20.0, 0.0, 0.0)
        """
        ...
    def pivot(self, x: float, y: float) -> Self:
        """Apply pivot to this drawable and return the result.

        Example:
            result = drawable.pivot(1.0, 1.0)
        """
        ...
    def next_to(
        self,
        reference: Drawable,
        direction: Direction,
        spacing: float = 0.24,
        aligned_edge: Optional[Anchor] = None,
    ) -> Self:
        """Apply next to to this drawable and return the result.

        Example:
            result = drawable.next_to(reference, Direction.RIGHT)
        """
        ...
    def align_to(
        self,
        reference: Drawable,
        target_anchor: Anchor,
        reference_anchor: Optional[Anchor] = None,
    ) -> Self:
        """Apply align to to this drawable and return the result.

        Example:
            result = drawable.align_to(reference, Anchor.CENTER)
        """
        ...
    def to_edge(self, direction: Direction, buff: float = 0.24) -> Self:
        """Apply to edge to this drawable and return the result.

        Example:
            result = drawable.to_edge(Direction.RIGHT)
        """
        ...
    def to_corner(self, corner: Anchor, buff: float = 0.24) -> Self:
        """Apply to corner to this drawable and return the result.

        Example:
            result = drawable.to_corner(Anchor.CENTER)
        """
        ...
    # Reactive methods
    def add_updater(self, updater: Updater) -> None:
        """Use add updater on this Drawable or create the requested value.

        Example:
            drawable.add_updater(Updater.orbit(0.0, 0.0, 100.0, 1.0))
        """
        ...
    def add_updater_fn(
        self,
        callback: Callable[
            [tuple[float, float, float], float, float],
            tuple[float, float, float] | Sequence[float],
        ],
        *,
        reset: Callable[[], None] | None = None,
        fixed_dt: float | None = None,
    ) -> Drawable:
        """Attach a Python position updater or a deterministic simulation.

        ``callback(position, dt, elapsed)`` returns the new local ``(x, y, z)``
        position. For stateful simulations, provide both ``reset`` and a positive
        ``fixed_dt``. Gaanim calls ``reset()`` before replaying fixed substeps for
        timeline seeks and exports. Fixed-step simulations run before ordinary
        callbacks, so dependent force parameters and labels observe the rebuilt
        state in the same frame.

        Example:
            bob.add_updater_fn(step, reset=reset_state, fixed_dt=1 / 240)
        """
        ...
    def remove_updater(self) -> None:
        """Use remove updater on this Drawable or create the requested value.

        Example:
            drawable.remove_updater()
        """
        ...
    def drive_from_samples(
        self,
        times: Sequence[float],
        values: Sequence[float] | Sequence[tuple[float, float]],
        property: Literal["x", "y", "xy", "z", "rotation", "scale", "opacity", "signal"] = "x",
        *,
        interpolation: Literal["linear", "step"] = "linear",
        scale: float = 1.0,
        offset: float = 0.0,
    ) -> Drawable:
        """Drive a property along a sampled ``(times, values)`` series natively.

        Evaluated as a pure function of timeline time — no per-frame Python
        callbacks, exact under seeks and paused scrubbing. Translation axes
        and ``rotation`` are relative to the authored pose
        (``base + offset + scale * sample``); ``scale``, ``opacity``, and
        ``signal`` are absolute (``offset + scale * sample``). Samples outside
        the series clamp to its first/last value. ``times`` are relative to
        the timeline cursor where this call is made. Each property is an
        independent channel: driving ``"x"`` and then ``"y"`` keeps both,
        while driving the same property again replaces it. Detach with
        ``remove_updater()``.

        Example:
            times = [i * 0.02 for i in range(len(accel))]
            building.drive_from_samples(times, accel, "x", scale=520.0)

        ``property="xy"`` takes ``(x, y)`` pairs (tuples or two-element lists)
        and drives both translation axes at once, as the ``"x"`` and ``"y"``
        channels; ``scale`` and ``offset`` apply to both. Both series are
        validated before either is attached. Scalars with ``"xy"``, or pairs
        with another property, raise ``ValueError``::

            path = [(math.cos(t), math.sin(t)) for t in times]
            probe.drive_from_samples(times, path, "xy", scale=2.0)
        """
        ...
    def bind_y_from(self, source: Drawable) -> None:
        """Bind Y and defer this visual until its entry animation is played.

        Example:
            drawable.bind_y_from(source)
        """
        ...
    def bind_x_from(self, source: Drawable) -> None:
        """Bind X and defer this visual until its entry animation is played.

        Example:
            drawable.bind_x_from(source)
        """
        ...
    def attach_to(self, source: Drawable) -> None:
        """Attach and defer this visual until its entry animation is played.

        Example:
            drawable.attach_to(source)
        """
        ...
    def follow_to(self, source: Drawable, offset: tuple[float, float]) -> None:
        """Follow and defer this visual until its entry animation is played.

        Example:
            drawable.follow_to(source, (0.0, 0.0))
        """
        ...
    def follow(self, source: Endpoint, *, offset: tuple[float, float] = (0.0, 0.0), offset_space: Literal["world", "local"] = "world") -> Self:
        """Follow any endpoint in the same frame and return this drawable.

        World offsets remain screen-aligned; local offsets rotate and scale with
        drawable or anchored sources. Non-finite offsets and invalid modes error.
        """
        ...
    def bind_rotation_from(self, source: Drawable, *, ratio: float = 1.0, phase: float = 0.0) -> Self:
        """Copy source world rotation using ``source * ratio + phase`` in radians."""
        ...
    def bind_translation_from_rotation(self, source: Drawable, *, axis: Optional[Direction] = None, scale: float = 1.0) -> Self:
        """Translate along an axis by source rotation delta times ``scale``."""
        ...
    def bind_position_from(self, source: Drawable, axes: str = "xy") -> None:
        """Bind position and defer this visual until its entry animation is played.

        Example:
            drawable.bind_position_from(source)
        """
        ...
    # manim Axes compatibility — coords mapping and graph helpers (only valid when self is an axes)
class Dimension(Drawable):
    """Reactive technical dimension with independently styleable parts."""
    @property
    def line(self) -> Drawable: ...
    @property
    def extensions(self) -> Drawable:
        """Return the reactive extension-line group for independent styling."""
        ...
    @property
    def label(self) -> Optional[Drawable]: ...
    @property
    def number(self) -> Optional[Drawable]: ...
    @property
    def unit(self) -> Optional[Drawable]: ...

class AngleDimension(Drawable):
    """Reactive angular dimension with separately styleable visual and text parts."""
    @property
    def arc(self) -> Drawable: ...
    @property
    def arrows(self) -> Drawable: ...
    @property
    def extensions(self) -> Drawable: ...
    @property
    def label(self) -> Optional[Drawable]: ...
    @property
    def number(self) -> Optional[Drawable]: ...
    @property
    def unit(self) -> Optional[Drawable]: ...

class SurroundingRect(Drawable):
    """Live axis-aligned frame around drawable or text-selection bounds.

    The frame follows its current targets after movement, scaling, rotation,
    or layout. Its path and transform are owned by the binding; use
    :meth:`retarget` instead of positional or geometric Drawable operations.
    Visual styling, create/write, fade, opacity, and effects remain available.
    """
    def retarget(
        self,
        targets: Drawable | TextSelection | Sequence[Drawable | TextSelection],
    ) -> Anim:
        """Tween all four frame edges to new live targets and keep following them.

        Configure timing afterward with ``duration`` and ``easing``. The
        returned animation remains pure until passed to ``Scene.play``.
        Empty, foreign-scene, or invalid targets raise ``ValueError`` or
        ``TypeError``.
        """
        ...

class ForceVector(Drawable):
    """Reactive force/vector with independently styleable shaft, solid head, and readout parts."""
    @property
    def shaft(self) -> Drawable: ...
    @property
    def head(self) -> Drawable: ...
    @property
    def label(self) -> Optional[Drawable]: ...
    @property
    def number(self) -> Optional[Drawable]: ...
    @property
    def unit(self) -> Optional[Drawable]: ...

class Support(Drawable):
    """Editorial mechanical support with independently styleable vector parts."""
    @property
    def joint(self) -> Drawable: ...
    @property
    def body(self) -> Drawable: ...
    @property
    def ground(self) -> Drawable: ...
    @property
    def rollers(self) -> Drawable: ...
    @property
    def guides(self) -> Drawable: ...
    @property
    def hatching(self) -> Drawable: ...

Endpoint: TypeAlias = Drawable | AnchorPoint | PointRef | tuple[float, float] | tuple[float, float, float]
ScalarSource: TypeAlias = float | Parameter | Variable | Computed | TimeInput
AngleRay: TypeAlias = Direction | Endpoint

class Material3D:
    """PBR material whose numeric properties interpolate in linear space."""
    def __init__(
        self,
        color: ColorLike = WHITE,
        roughness: float = 0.55,
        metallic: float = 0.0,
        emissive: Optional[ColorLike] = None,
        emissive_strength: float = 0.0,
    ) -> None: ...
    @staticmethod
    def matte(color: ColorLike = WHITE) -> Material3D: ...
    @staticmethod
    def metal(color: ColorLike = WHITE) -> Material3D: ...
    @staticmethod
    def emissive(color: ColorLike = WHITE, strength: float = 1.0) -> Material3D: ...
    @property
    def color(self) -> Color: ...
    @property
    def roughness(self) -> float: ...
    @property
    def metallic(self) -> float: ...
    @property
    def emissive_color(self) -> Color: ...
    @property
    def emissive_strength(self) -> float: ...

class Primitive3D(Drawable):
    """Native indexed 3D mesh with an animatable PBR material."""
    def material(self, material: Material3D) -> Self: ...

class TextStyle:
    """Reusable visual and metric text style without outer box layout."""
    def __init__(
        self,
        *,
        font: Optional[str] = None,
        math_font: Optional[str] = None,
        fallbacks: Sequence[str] = (),
        size: Optional[float] = None,
        weight: Optional[int] = None,
        italic: Optional[bool] = None,
        color: Optional[Color] = None,
        stroke: Optional[Color] = None,
        stroke_width: Optional[float] = None,
        opacity: Optional[float] = None,
        letter_spacing: Optional[float] = None,
        word_spacing: Optional[float] = None,
        decorations: Sequence[str] = (),
        baseline: Optional[float] = None,
    ) -> None:
        """Create a reusable typography overlay.

        Sizes and spacing use scene units. Invalid non-positive sizes
        raise ``ValueError``. Outer width, height, padding, fit, and growth are
        intentionally controlled by Layout v2.

        Example:
            body = TextStyle(font="Inter", size=0.32, color=WHITE)
        """
        ...

class TextFlow:
    """Reusable internal line-composition options for Text."""
    def __init__(
        self,
        *,
        wrap: TextWrap = "auto",
        align: TextAlign = "left",
        line_spacing: float = 1.2,
        max_lines: Optional[int] = None,
        overflow: TextOverflow = "clip",
        direction: TextDirection = "auto",
        hyphenate: bool = False,
        lang: Optional[str] = None,
    ) -> None:
        """Configure wrapping and line composition inside a measured Text leaf.

        ``"auto"`` consumes the width offered by Layout v2 or the safe frame;
        ``False`` keeps one line except for explicit newlines; a number caps the
        typographic width. ``lang`` is a lowercase ISO 639 code (``"es"``,
        ``"en"``, …) that selects the hyphenation patterns used with
        ``hyphenate=True``; ``None`` keeps Typst's English default. Invalid
        widths, spacing, line counts, or language codes raise ``ValueError``.

        Example:
            flow = TextFlow(wrap="auto", align="justify", line_spacing=1.25)
            spanish = TextFlow(wrap=4.0, align="justify", hyphenate=True, lang="es")
        """
        ...

class TextPart:
    """Immutable named subtree created by :func:`part`."""

class TextParts:
    """Immutable ordered group of plain named parts created by :func:`parts`."""

TextContent: TypeAlias = str | TextPart | TextParts

def parts(mapping: Optional[Mapping[str, str]] = None, /, **content: str) -> TextParts:
    """Build an ordered group of plain semantic text parts.

    Name the parts either with a mapping, like :func:`part` takes its name as
    a string, or with keyword arguments as a shortcut. A mapping keeps its
    insertion order and accepts names that are not Python identifiers, such as
    ``"tb:dist"`` or ``"x-1"``. Inside ``$...$`` math, adjacent parts are
    separated as distinct Typst tokens while retaining Typst's native tight
    spacing. Use explicit ``part`` values when local styling or nested content
    is needed. Calling ``parts()`` without entries, mixing a mapping with
    keyword entries, repeating a name, using an empty name, or producing
    wholly empty content raises ``ValueError``; a non-mapping positional
    argument, a non-string name, or a non-string value raises ``TypeError``.

    Example:
        terms = parts(mass="m", gravity="g sin(theta)")
        labels = parts({"tb:dist": "d", "x-1": "x"})
        equation = scene.text("$", terms, "$")
    """
    ...

def part(
    name: str,
    *content: TextContent,
    style: Optional[TextStyle] = None,
    font: Optional[str] = None,
    math_font: Optional[str] = None,
    size: Optional[float] = None,
    weight: Optional[int] = None,
    italic: Optional[bool] = None,
    color: Optional[Color] = None,
    opacity: Optional[float] = None,
    letter_spacing: Optional[float] = None,
    word_spacing: Optional[float] = None,
    baseline: Optional[float] = None,
) -> TextPart:
    """Build a composable semantic text part with optional local style.

    Names must be non-empty and unique among siblings. Nested parts keep a
    stable semantic path used by selections and text transitions. Within
    ``$...$``, whitespace written at a content/part boundary becomes visible
    mathematical spacing instead of being discarded by Typst.

    Example:
        formula = part("formula", "$E = ", part("mass", "m", color=GOLD), " c^2$")
    """
    ...

class TextSelectionAnimation:
    """Pure typed proxy for one text selection."""
    def fill(self, color: Color) -> Anim: ...
    def opacity(self, value: float) -> Anim: ...
    def indicate(self) -> Anim: ...
    def wiggle(self) -> Anim: ...
    def pulse(self) -> Anim: ...
    def wave(self) -> Anim: ...
    def highlight(self) -> Anim: ...
    def focus(self) -> Anim: ...
    def cancel(self) -> Anim: ...
    def reveal(self, style: Literal["fade", "wipe", "from_below"] = "fade") -> Anim:
        """Reveal the selected glyphs with a fade, a stroke wipe, or a short rise.

        The rest of the text is unaffected, so a term can appear inside an
        equation that is already on screen. Raises ``ValueError`` for an
        unknown ``style``.

        Example:
            scene.play(eq["rhs"].animate.reveal("from_below"))
        """
        ...
    def brace(self, label: str = "", *, above: bool = False) -> Anim:
        """Draw a brace under the selection (over it with ``above=True``) and fade in ``label``.

        The brace and label are new scene objects that stay after the
        animation, colored like the selected glyphs.

        Example:
            scene.play(eq["mass"].animate.brace("masa"))
        """
        ...
    def annotate(self, label: str, offset: tuple[float, float] = (0.0, 0.6)) -> Anim:
        """Place ``label`` at ``offset`` from the selection with a leader line.

        The line and label stay after the animation. Raises ``ValueError``
        for a non-finite offset.

        Example:
            scene.play(eq["c"].animate.annotate("velocidad de la luz", offset=(0, 0.6)))
        """
        ...
    def morph_to(self, target: TextSelection) -> Anim: ...
    def copy_to(self, target: TextSelection) -> Anim: ...

class TextSelection:
    """Deferred grapheme, word, line, or semantic-part selection.

    Mathematical animations first match the authored fragment literally. If
    that produces no glyphs, Typst symbol names, modifiers, shorthands, and
    prime syntax are resolved to their rendered Unicode through Typst Codex;
    for example, ``g sin(theta)`` targets the rendered ``g sin(θ)``.
    """
    def __getitem__(self, name: str) -> TextSelection: ...
    def fill(self, color: Color) -> TextSelection:
        """Persistently color selected glyphs and invalidate metric state if needed.

        In mathematics, the selected part remains inside the same Typst
        equation, so changing its fill does not insert spacing or alter the
        positions of neighboring unstyled terms.

        Example:
            formula["mass"].fill(GOLD)
        """
        ...
    @property
    def animate(self) -> TextSelectionAnimation:
        """Return a pure animation proxy scoped to the selected glyphs.

        Only ``fill`` and ``opacity`` targets are supported; other
        property channels raise ``TypeError``.

        Example:
            scene.play([formula["mass"].animate.fill(RED).opacity(0.6)])
        """
        ...
class TextQuery:
    """Deferred indexable view over rendered text units."""
    def __len__(self) -> int: ...
    def __contains__(self, value: str) -> bool: ...
    @overload
    def __getitem__(self, index: int) -> TextSelection: ...
    @overload
    def __getitem__(self, index: slice) -> TextSelection: ...

class TextAnimator:
    """Range selector over the units of one Text (After Effects-style text animator).

    Create it with ``Text.animator``, define the "out" state with ``set`` and
    play it with ``animate.sweep()``. Each unit's influence is evaluated
    natively from its position in ``order`` and the selector ``shape``, so
    seeks are exact and nothing calls back into Python per frame.
    """
    def set(
        self,
        *,
        offset: Optional[tuple[float, float]] = None,
        opacity: Optional[float] = None,
        scale: Optional[float] = None,
        rotation: Optional[float] = None,
        blur: Optional[float] = None,
        tracking: Optional[float] = None,
        color: Optional[Color] = None,
    ) -> TextAnimator:
        """Define the state a unit reaches at full influence and return this animator.

        ``offset`` moves units in scene units, ``opacity`` (0..1) is
        absolute, ``scale`` and ``rotation`` (radians, clamped to less than
        half a turn) act around each unit's center, ``blur`` is a Gaussian
        sigma in scene units, ``tracking`` adds scene units between glyphs
        and ``color`` sets a solid fill. Omitted values keep what an earlier
        ``set`` defined; unset channels stay at rest. Sweeps capture the
        state when ``sweep()`` is called. Invalid values raise ``ValueError``.

        Example:
            wave = title.animator(by="grapheme", shape="smooth").set(offset=(0, -0.4), opacity=0.0)
        """
        ...
    @property
    def animate(self) -> TextAnimatorAnimation:
        """Typed proxy whose ``sweep()`` returns a composable ``Anim``."""
        ...

class TextAnimatorAnimation:
    """Animation proxy of a ``TextAnimator``."""
    def sweep(self, start: float = 0.0, end: float = 1.0, *, stagger: Optional[float] = None) -> Anim:
        """Move the selector range from ``start`` to ``end`` over the animation.

        ``0`` lies before the first unit and ``1`` after the last, so the
        default sweep crosses every unit once; ``sweep(1, 0)`` plays it
        backward. Units take their window in ``order``: with a reveal shape
        they move from the out state to rest, with ``"triangle"``/``"round"``
        a wave passes through them. ``stagger`` is the delay in seconds
        between units (``None`` staggers adaptively). ``easing`` eases each
        unit's transition (linear by default) and the result works in
        ``scene.play``, ``parallel``, ``sequence`` and ``stagger``. Entry
        sweeps hold their first frame until they start.

        Example:
            scene.play(wave.animate.sweep().duration(1.2))
        """
        ...

class Text(Drawable):
    """Structured, Layout-v2-measurable vector text and mathematics."""
    def glow(self, color: Color, radius: float = 0.16, intensity: float = 1.0) -> Self:
        """Apply glow while preserving Text chaining and typographic placement.

        Example:
            title.glow(BLUE).move_to(0.0, 0.0)
        """
        ...
    def blur(self, sigma: float = 0.04) -> Self:
        """Apply blur while preserving Text chaining and typographic placement.

        Example:
            label.blur(0.04).move_to(0.0, 0.0)
        """
        ...
    def shadow(
        self,
        color: Color,
        x: float = 0.08,
        y: float = -0.08,
        blur: float = 0.06,
    ) -> Self:
        """Apply shadow while preserving Text chaining and typographic placement.

        Example:
            title.shadow(BLACK).move_to(0.0, 0.0)
        """
        ...
    def no_effects(self) -> Self:
        """Remove visual effects while preserving the specialized Text handle.

        Example:
            title.no_effects().move_to(0.0, 0.0)
        """
        ...
    @overload
    def __getitem__(self, name: str) -> TextSelection: ...
    @overload
    def __getitem__(self, index: int | slice) -> TextSelection: ...
    @property
    def graphemes(self) -> TextQuery: ...
    @property
    def words(self) -> TextQuery: ...
    @property
    def lines(self) -> TextQuery: ...
    @property
    def parts(self) -> TextQuery: ...
    def animator(
        self,
        by: Literal["grapheme", "word", "line", "part"] = "grapheme",
        shape: Literal["square", "ramp", "smooth", "ease_in", "ease_out", "triangle", "round"] = "smooth",
        order: Literal["forward", "reverse", "center", "random"] = "forward",
        seed: int = 0,
    ) -> TextAnimator:
        """Create a range animator over this text's graphemes, words, explicit lines or parts.

        ``shape`` is the selector profile: ``"square"``, ``"ramp"``,
        ``"smooth"``, ``"ease_in"`` and ``"ease_out"`` move each unit from the
        out state to rest (a reveal); ``"triangle"`` and ``"round"`` rise to
        the out state and settle back (a wave). ``order`` sets which unit the
        range reaches first; ``"random"`` is a permutation fixed by ``seed``.
        Units follow ``text.graphemes``/``words``/``lines``/``parts`` and
        punctuation joins its neighbor. Invalid names raise ``ValueError``.

        Example:
            wave = title.animator(by="grapheme", shape="smooth", order="forward", seed=0)
            wave.set(offset=(0, -0.4), opacity=0.0, scale=0.6, rotation=0.2)
            scene.play(wave.animate.sweep().duration(1.2))
        """
        ...
    def tracking(self, value: float) -> Self:
        """Set the extra space between neighboring glyphs to ``value`` scene units now.

        Glyphs shift along the baseline without a new layout, anchored at
        the left edge, center or right edge of each row according to the
        text alignment; ``0`` restores the layout spacing. Animate it with
        ``animate.tracking(value)``. Returns this Text.

        Example:
            title.tracking(0.4)
            scene.play(title.animate.tracking(0.0))
        """
        ...
    @overload
    @overload
    def move_to(self, reference: Drawable, /) -> Self: ...
    @overload
    def move_to(self, point: AnchorPoint, /) -> Self: ...
    @overload
    def move_to(
        self,
        x: ScalarSource,
        y: ScalarSource,
        anchor: Optional[Anchor | TextAnchor] = None,
    ) -> Self:
        """Place this Text and preserve its specialized handle.

        ``anchor`` may be passed positionally or by keyword.

        A single visual line defaults to ``TextAnchor.BASELINE_CENTER``. A
        multiline block defaults to its visual center. Explicit
        ``TextAnchor`` values align the first line's baseline; geometric
        ``Anchor`` values retain bounds-based placement. When a text with
        explicit line breaks was created without ``flow`` or ``text_align``,
        an explicit anchor also aligns its lines: ``*_LEFT`` anchors left, ``*_RIGHT`` anchors right,
        and the others center. Layout-owned text raises
        ``LayoutOwnershipError``. Passing one ``Drawable`` aligns the
        text's visual center to the reference's center. Passing an
        ``AnchorPoint`` aligns the visual center to that transformed anchor;
        neither form creates a reactive follow relationship.

        Example:
            label.move_to(0.0, 0.4, TextAnchor.BASELINE_LEFT)
            label.move_to(marker)
            label.move_to(marker.anchor_point(Anchor.TOP))


        A reactive source binds this channel from the current cursor; another
        source replaces it and a numeric setter ends the binding reversibly.
        Sources must belong to this Scene. Animate the source Parameter while
        linked; direct animation or relative writes to this channel error.
        """
        ...
    def become(self, *content: TextContent, role: Optional[TextRole] = None, style: Optional[TextStyle] = None, flow: Optional[TextFlow] = None, markup: Optional[bool] = None) -> None:
        """Replace structured content while retaining Text identity and reflowing owners.

        The text version and all owning Layout snapshots are incremented.
        ``markup=None`` keeps the current markup mode. An invalid delimiter or
        content tree raises ``ValueError``.

        Example:
            copy.become("Resultado: ", part("value", "$42$", color=GOLD))
        """
        ...

class Canvas:
    """Resolution-independent logical frame configuration owned by a Scene."""
    @property
    def frame_width(self) -> float:
        """Logical frame width; 16.0 for the default widescreen scene."""
        ...
    @property
    def frame_height(self) -> float:
        """Logical frame height; 9.0 for the default widescreen scene."""
        ...
    @property
    def aspect_ratio(self) -> float:
        """Logical frame width divided by height."""
        ...
    @property
    def safe_width(self) -> float:
        """Logical width remaining after left and right safe-area margins."""
        ...
    @property
    def safe_height(self) -> float:
        """Logical height remaining after top and bottom safe-area margins."""
        ...
    background: Optional[BackgroundLike]
    @property
    def post(self) -> Optional[PostProcess]:
        """Return the scene post-process, or ``None`` when the scene has none."""
        ...
    @post.setter
    def post(self, value: Optional[PostProcess]) -> None:
        """Replace the scene post-process; ``None`` removes it.

        Segments that set their own ``post`` keep their override.
        """
        ...
    theme: Optional[str]
    def set_theme(self, theme: str | Theme) -> None:
        """Apply a built-in color scheme or a custom Theme."""
        ...
    def set_fonts(
        self,
        *,
        font: Optional[str] = None,
        math_font: Optional[str] = None,
        code_font: Optional[str] = None,
    ) -> None:
        """Override canvas-wide prose, math, and code font families.

        Supplied values override theme typography while object-level font
        options still win. Omitted values keep their existing override, and
        an empty family raises ``ValueError``.

        Example:
            scene.canvas.set_fonts(
                font="Inter",
                math_font="New Computer Modern Math",
                code_font="JetBrains Mono",
            )
        """
        ...
    def color(self, role: str) -> Color:
        """Resolve a semantic color from the active theme."""
        ...
    def layout_token(self, name: str) -> float:
        """Resolve a spacing/layout token from the theme or default scale."""
        ...
    def validate_theme(self) -> list[str]:
        """Configure the canvas with validate theme.

        Example:
            result = scene.canvas.validate_theme()
        """
        ...
    def set_margin(self, margin: float) -> None:
        """Configure the canvas with set margin.

        Example:
            scene.canvas.set_margin(1.0)
        """
        ...
    def set_safe_area(
        self,
        *,
        top: float = 0.0,
        right: float = 0.0,
        bottom: float = 0.0,
        left: float = 0.0,
    ) -> None:
        """Configure the canvas with set safe area.

        Example:
            scene.canvas.set_safe_area()
        """
        ...
    def set_preset(self, name: Literal["widescreen", "vertical", "square"]) -> None:
        """Replace the logical frame and safe area with a named composition preset.

        Example:
            scene.canvas.set_preset("vertical")
        """
        ...
class Segment:
    def bind(self, **slots: Any) -> Layout:
        """Bind this segment's template slots and return its root Layout.

        Missing or extra slots raise ``TypeError``; a segment without a
        template raises ``ValueError``.
        """
        ...

class SceneStop:
    """An interactive stop authored with ``scene.stop``."""
    @property
    def name(self) -> Optional[str]: ...
    @property
    def time(self) -> float:
        """Absolute timeline time in seconds."""
        ...
    @property
    def segment(self) -> str:
        """Name of the segment containing the stop."""
        ...

class CameraState:
    """Opaque reusable authored camera state owned by one Scene.

    States are created by ``Camera.state_2d``, ``Camera.state_3d``,
    ``Camera.capture``, or ``Camera.save`` and are consumed by ``Camera.to``.
    """
    ...

class CameraConstraint:
    """Persistent native camera binding with timeline-recorded activation."""
    def enable(self) -> None:
        """Enable the binding at the current timeline cursor."""
        ...
    def disable(self) -> None:
        """Disable the binding at the current timeline cursor."""
        ...

class Camera:
    @property
    def animate(self) -> CameraAnimation: ...
    def state_2d(self, center: tuple[float, float] = (0.0, 0.0), zoom: float = 1.0, rotation: float = 0.0) -> CameraState: ...
    def state_3d(self, eye: tuple[float, float, float], target: tuple[float, float, float], up: tuple[float, float, float] = (0.0, 1.0, 0.0), fov_y: float = 0.7853981633974483, near: float = 0.1, far: float = 1000.0) -> CameraState: ...
    def capture(self) -> CameraState: ...
    def save(self, name: str) -> CameraState: ...
    def to(self, state: CameraState) -> Camera: ...
    def restore(self, name: str) -> Camera: ...
    @overload
    def pan_to(self, x: float, y: float) -> Camera: ...
    @overload
    def pan_to(self, target: Endpoint) -> Camera: ...
    def zoom_to(self, zoom: ScalarSource) -> Camera: ...
    def frame_to(self, targets: Drawable | Sequence[Drawable], margin: float | tuple[float, float] | tuple[float, float, float, float] | None = None, *, dynamic: bool = False) -> Camera: ...
    def rotate_to(self, angle: ScalarSource) -> Camera: ...
    def look_at(self, eye: Endpoint, target: Endpoint, up: Optional[tuple[float, float, float]] = None) -> Camera: ...
    def perspective(self, fov_y: float, near: float = 0.1, far: float = 1000.0) -> Camera: ...
    def orthographic(self, zoom: float = 1.0) -> Camera: ...
    def reset(self) -> Camera: ...
    def bind_2d(self, *, center: Optional[Endpoint] = None, zoom: Optional[ScalarSource] = None, rotation: Optional[ScalarSource] = None, influence: Optional[ScalarSource] = None, enabled: bool = True) -> CameraConstraint: ...
    def bind_3d(self, *, eye: Optional[Endpoint] = None, target: Optional[Endpoint] = None, fov_y: Optional[ScalarSource] = None, up: tuple[float, float, float] = (0.0, 1.0, 0.0), influence: Optional[ScalarSource] = None, enabled: bool = True) -> CameraConstraint: ...

class CameraAnimation:
    def to(self, state: CameraState) -> Anim:
        """Animate to a reusable camera state and return a composable Anim.

        A state owned by another Scene raises ``ValueError``. Configure timing
        on the returned animation.
        """
        ...
    def restore(self, name: str) -> Anim:
        """Animate to a named saved state and return a composable Anim.

        Unknown names raise ``ValueError``.
        """
        ...
    @overload
    def pan_to(self, x: float, y: float) -> Anim:
        """Configure the camera with pan to.

        Example:
            scene.camera.pan_to(1.0, 1.0)
        """
        ...
    @overload
    def pan_to(self, target: Endpoint) -> Anim: ...
    def zoom_to(
        self,
        zoom: ScalarSource,
        *,
        interpolation: Literal["exponential", "linear"] = "exponential",
    ) -> Anim:
        """Animate the orthographic zoom; values above one zoom in.

        ``interpolation="exponential"`` (the default) evaluates
        ``z0 * (z1 / z0) ** p`` so the visible area changes by the same ratio
        every frame and a large zoom reads as constant speed.
        ``"linear"`` restores ``z0 + (z1 - z0) * p``. ``p`` is the eased
        progress, so ``.easing(...)`` still shapes the move. A non-positive
        constant zoom or an unknown interpolation raises ``ValueError``.

        Example:
            scene.camera.animate.zoom_to(8.0).duration(1.5)
        """
        ...
    def frame_to(
        self,
        targets: Drawable | Sequence[Drawable],
        margin: float | tuple[float, float] | tuple[float, float, float, float] | None = None,
        *,
        dynamic: bool = False,
        interpolation: Literal["exponential", "linear"] = "exponential",
    ) -> Anim:
        """Pan and zoom so ``targets`` fit the viewport with ``margin``.

        With ``interpolation="exponential"`` (the default) the zoom is
        exponential and the pan follows the change of visible width, so the
        view scales about a fixed point instead of drifting before it settles.
        ``"linear"`` interpolates position and zoom independently.

        Example:
            scene.camera.animate.frame_to([circle, label], margin=0.4).duration(1.0)
        """
        ...
    def rotate_to(self, angle: ScalarSource) -> Anim:
        """Configure the camera with rotate to.

        Example:
            scene.camera.rotate_to(1.0)
        """
        ...
    def follow(
        self,
        target: Endpoint,
        *,
        offset: tuple[float, float] = (0.0, 0.0),
        offset_space: Literal["world", "local"] = "world",
        lag: float = 0.0,
    ) -> Anim:
        """Configure the camera with follow.

        Example:
            scene.camera.follow(target)
        """
        ...
    def shake(
        self,
        amplitude: Optional[float] = None,
        frequency: Optional[float] = None,
        *,
        trauma: Optional[float] = None,
        decay: Optional[float] = None,
        rotation: Optional[float] = None,
        seed: Optional[int] = None,
    ) -> Anim:
        """Shake the camera deterministically and return it to rest.

        By default the shake follows the trauma model: ``trauma`` (``0..1``,
        default ``0.8``) decays by ``decay`` per second (default ``1.5``) and
        the displacement is proportional to ``trauma ** 2``. Translation (at
        most ``amplitude`` scene units at trauma 1, default ``0.4``) and roll
        (at most ``rotation`` radians, default ``0.02``) come from seeded
        coherent noise sampled at ``frequency`` Hz (default ``12``). The clip
        lasts ``trauma / decay`` seconds (one second when ``decay`` is zero);
        a shorter ``.duration()`` still releases smoothly to rest. The shake
        is a pure function of time, so seeks and exports match playback.

        Passing ``amplitude`` without any of ``trauma``, ``decay``,
        ``rotation`` or ``seed`` keeps the legacy sine shake: ``amplitude``
        is the peak offset, ``frequency`` counts oscillations per clip
        (default ``8``) and the clip lasts 0.5 s, so ``shake(0.2, 6)`` keeps
        its previous look. Negative values, or ``trauma`` above one, raise
        ``ValueError``.

        Example:
            scene.camera.animate.shake(trauma=0.8, decay=1.5, frequency=12, rotation=0.02, seed=0)
        """
        ...
    def look_at(
        self,
        eye: Endpoint,
        target: Endpoint,
        up: Optional[tuple[float, float, float]] = None,
    ) -> Anim:
        """Aim the camera from ``eye`` toward ``target``.

        Args:
            eye: Camera position in world coordinates.
            target: World-space point to look at.
            up: World-up direction. Defaults to ``(0, 1, 0)``.
        Configure timing afterward on the returned ``Anim``.
        """
        ...

    def orbit(
        self,
        delta_yaw: float,
        delta_pitch: float,
    ) -> Anim:
        """Orbit around the current look-at target.

        ``delta_yaw`` and ``delta_pitch`` are radians.

        Example:
            scene.camera.animate.orbit(delta_yaw=0.6, delta_pitch=0.12).duration(1.0)
        """
        ...

    def perspective(
        self,
        fov_y: float,
        near: float = 0.1,
        far: float = 1000.0,
    ) -> Anim:
        """Use perspective projection with a vertical field of view.

        ``fov_y`` is in radians and must be positive. ``near`` and ``far``
        are positive clipping distances with ``near < far``.

        Example:
            scene.camera.animate.perspective(0.785, near=0.1, far=1000.0)
        """
        ...

    def dolly(self, factor: float) -> Anim:
        """Move toward or away from the current target.

        A factor below ``1`` moves closer; a factor above ``1`` moves farther.
        The factor must be finite and positive.

        Example:
            scene.camera.animate.dolly(factor=0.85).duration(0.6)
        """
        ...

    def orthographic(self, zoom: float = 1.0) -> Anim:
        """Select orthographic projection; ``zoom`` must be positive."""
        ...

    def reset(self) -> Anim:
        """Restore the default 2D pose, up vector, target, and projection."""
        ...

class Axis:
    """Immutable scale, ticks, labels, crossing, and style specification."""
    @staticmethod
    def linear(minimum: float, maximum: float) -> Axis: ...
    @staticmethod
    def log(minimum: float, maximum: float, base: float = 10.0) -> Axis: ...
    @staticmethod
    def symlog(
        minimum: float,
        maximum: float,
        *,
        base: float = 10.0,
        threshold: float = 1.0,
    ) -> Axis: ...
    @staticmethod
    def power(minimum: float, maximum: float, exponent: float) -> Axis: ...
    @staticmethod
    def time(minimum_timestamp: float, maximum_timestamp: float) -> Axis: ...
    @staticmethod
    def category(values: Sequence[str]) -> Axis: ...
    def ticks(self, step: float) -> Axis: ...
    def auto_ticks(self) -> Axis: ...
    def minor_ticks(self, subdivisions: int) -> Axis: ...
    def numbers(
        self,
        format: Literal["auto", "fixed", "scientific", "percent", "fraction", "pi", "datetime"] = "auto",
        precision: int = 2,
        denominator: int = 4,
        pattern: Optional[str] = None,
    ) -> Axis:
        """Return a copy with the given tick-number format.

        ``precision`` applies to ``fixed``, ``scientific`` and ``percent``;
        ``denominator`` to ``fraction`` and ``pi``. Negative numbers use the
        typographic minus U+2212, and values that round to zero have no sign.
        """
        ...
    def label(
        self,
        text: str,
        *,
        position: Literal["start", "center", "middle", "mid", "end", "top", "bottom"] = "end",
    ) -> Axis:
        """Return a copy with a title clear of ticks.

        ``position`` locates it along the axis. The default ``end`` places an
        upright title beyond the positive endpoint; ``start`` uses the negative
        endpoint. ``center`` (also ``middle``/``mid``) centers x below its tick
        labels and y vertically at the left, rotated 90 degrees. ``top`` and
        ``bottom`` are aliases for vertical-axis endpoints.
        """
        ...
    def crossing(self, value: Literal["auto", "zero", "min", "max", "minimum", "maximum"] | float) -> Axis: ...
    def style(
        self,
        *,
        color: Optional[ColorLike] = None,
        width: Optional[float] = None,
        tick_length: Optional[float] = None,
        tick_width: Optional[float] = None,
        tick_color: Optional[ColorLike] = None,
        number_color: Optional[ColorLike] = None,
        label_color: Optional[ColorLike] = None,
    ) -> Axis:
        """Override selected properties; omitted values inherit from the active theme."""
        ...
    @property
    def domain(self) -> tuple[float, float]: ...

class Scale:
    """Immutable channel scale used by :class:`Field` encodings."""
    @staticmethod
    def linear(domain: Optional[tuple[float, float]] = None, *, clamp: bool = False) -> Scale:
        """Create a linear scale with an optional fixed finite domain."""
        ...
    @staticmethod
    def log(domain: Optional[tuple[float, float]] = None, *, base: float = 10.0, clamp: bool = False) -> Scale:
        """Create a positive logarithmic scale with a valid non-unit base."""
        ...
    @staticmethod
    def symlog(domain: Optional[tuple[float, float]] = None, *, base: float = 10.0, threshold: float = 1.0, clamp: bool = False) -> Scale:
        """Create a signed logarithmic scale with a linear zero region."""
        ...
    @staticmethod
    def power(domain: Optional[tuple[float, float]] = None, *, exponent: float = 1.0, clamp: bool = False) -> Scale:
        """Create a signed power scale with a finite non-zero exponent."""
        ...
    @staticmethod
    def time(domain: Optional[tuple[float, float]] = None, *, clamp: bool = False) -> Scale:
        """Create a timestamp scale whose numeric values are seconds."""
        ...
    @staticmethod
    def category(values: Optional[Sequence[str]] = None) -> Scale:
        """Create a categorical scale; omitted values are inferred from data order."""
        ...
    def colors(self, colors: Sequence[ColorLike]) -> Scale:
        """Return a copy with an explicit ordered color range.

        Entries accept Color, CSS/hex strings, and RGB/RGBA byte tuples.
        Mixed representations are allowed; invalid colors raise ValueError.
        """
        ...

class Field:
    """Reference a data column and optionally configure its channel scale."""
    def __init__(self, column: str, *, scale: Optional[Scale] = None) -> None:
        """Create a non-empty column encoding."""
        ...

class Value:
    """Wrap a finite number, string, or color as a constant encoding."""
    def __init__(self, value: float | str | Color) -> None:
        """Create a constant channel value; non-finite numbers raise ``ValueError``."""
        ...

class Guide:
    """Configuration for a legend or continuous colorbar."""
    @staticmethod
    def legend(*, title: Optional[str] = None) -> Guide:
        """Create a discrete legend guide.

        On a categorical ``color`` field the legend lists every category with
        a swatch of its color, in ``Scale.category`` order (or first
        appearance in the data) below the optional title.
        """
        ...
    @staticmethod
    def colorbar(*, title: Optional[str] = None) -> Guide:
        """Create a continuous colorbar guide."""
        ...
    @staticmethod
    def disabled() -> Guide:
        """Disable the guide for one encoded channel."""
        ...

EncodingLike: TypeAlias = str | Field | Value
ChartMark: TypeAlias = Literal["point", "line", "step", "area", "bar", "histogram", "box", "violin", "error_bar", "heatmap", "surface"]

class ChartSpec:
    """Immutable declarative chart spec that snapshots its input data eagerly."""
    def __init__(self, data: DataTable | DataSource | Mapping[str, Sequence[float | str | None]] | Any, *, key: Optional[str] = None) -> None:
        """Capture data and validate a non-null unique key when supplied."""
        ...
    def mark(self, kind: ChartMark, **options: float | str | Color) -> ChartSpec:
        """Return a copy using mark options; bars accept ``label_position`` (``outside``/``inside``), non-negative local ``label_offset``, and ``label_color``."""
        ...
    def encode(self, *, x: Optional[EncodingLike] = None, y: Optional[EncodingLike] = None, z: Optional[EncodingLike] = None, color: Optional[EncodingLike] = None, size: Optional[EncodingLike] = None, opacity: Optional[EncodingLike] = None, label: Optional[EncodingLike] = None) -> ChartSpec:
        """Return a copy with validated positional and visual channel encodings.

        For ``point`` marks ``size`` is the radius in scene units in 2D and 3D:
        ``Value(0.1)`` draws radius-0.1 points and the default is 0.06. A
        numeric ``Field`` maps its domain onto radii 0.03 to 0.12 with area
        proportional to the value. A ``radius`` mark option overrides ``size``.
        """
        ...
    def axes(self, *, x: Optional[Axis] = None, y: Optional[Axis] = None, z: Optional[Axis] = None) -> ChartSpec:
        """Return a copy with explicit positional axes; omitted axes are inferred.

        Inferred bar axes include the baseline and reserve an outer margin so
        the first and last bars do not touch the plot boundary. Explicit axis
        domains are preserved exactly.
        """
        ...
    def guides(self, *, color: Optional[Guide] = None, size: Optional[Guide] = None, opacity: Optional[Guide] = None) -> ChartSpec:
        """Return a copy with guides derived from the corresponding channel scales."""
        ...
    def validate(self) -> None:
        """Validate required channels, referenced columns, keys, scales, and options."""
        ...
    @property
    def key(self) -> Optional[str]:
        """Return the stable identity column used for semantic transitions."""
        ...
    def __len__(self) -> int: ...

class Random:
    """Seeded, platform-independent random stream created by ``scene.random``."""
    @property
    def seed(self) -> int: ...
    def uniform(self, low: float = 0.0, high: float = 1.0) -> float:
        """Draw a float in ``[low, high)``; ``high < low`` raises ``ValueError``."""
        ...
    def gauss(self, mean: float = 0.0, std: float = 1.0) -> float:
        """Draw a normal deviate; a negative ``std`` raises ``ValueError``."""
        ...
    def integer(self, low: int, high: int) -> int:
        """Draw an integer in ``[low, high)``; an empty range raises ``ValueError``."""
        ...
    def choice(self, items: Sequence[Any]) -> Any:
        """Pick one element; an empty sequence raises ``IndexError``."""
        ...
    def shuffle(self, items: list[Any]) -> None:
        """Shuffle ``items`` in place."""
        ...

class Computed:
    """Opaque deterministic scalar computed from explicit reactive inputs."""

class TimeInput:
    """The scene timeline time, valid only as an explicit reactive input."""

def computed(callback: Callable[..., float], *, inputs: Sequence[Parameter | Variable | Computed | TimeInput] = ()) -> Computed:
    """Create a deterministic scalar whose arguments follow ``inputs`` order.

    Inputs can include other Computed values. Shared dependencies reuse cached
    results for identical input snapshots; changing time or a leaf parameter
    invalidates dependent results. Consumers reject transitive references to
    another Scene. The callback must be synchronous and return a finite scalar.

    Example:
        area = computed(lambda r: math.pi*r*r, inputs=[radius])
        doubled = computed(lambda a: 2*a, inputs=[area])
    """
    ...

class Parameter:
    """An animatable scalar usable directly or as an explicit callback input."""
    @property
    def current(self) -> float: ...
    def set(self, value: float) -> None: ...
    @property
    def animate(self) -> Anim:
        """Return a pure proxy; use ``parameter.animate.set(value)`` and pass it to ``Scene.play``."""
        ...
    def add_updater_fn(
        self,
        callback: Callable[[float, float, float], float],
        *,
        reset: Callable[[], None] | None = None,
        fixed_dt: float | None = None,
    ) -> Parameter:
        """Drive the scalar as ``callback(current, dt, elapsed) -> value``.

        Pair ``reset`` with a positive ``fixed_dt`` for deterministic seeking
        and export. Fixed-step drawable simulations run first, so this callback
        can derive a force or readout from their same-frame state. The callback
        must return a finite number.
        """
        ...
    def drive_from_samples(
        self,
        times: Sequence[float],
        values: Sequence[float],
        *,
        interpolation: Literal["linear", "step"] = "linear",
        scale: float = 1.0,
        offset: float = 0.0,
    ) -> Parameter:
        """Drive this parameter's value along a sampled series, natively.

        The value becomes ``offset + scale * sample`` as a pure function of
        timeline time, so computed values, readouts, and reactive plots
        referencing this parameter follow the series without Python callbacks.
        ``times`` are relative to the timeline cursor where this call is made.

        Example:
            phase = scene.parameter(0.0)
            phase.drive_from_samples(times, values, scale=2.0 * math.pi)
        """
        ...
    def remove_updater(self) -> None: ...

class Readout(Drawable):
    """A reactive numeric display with equation spacing and baseline-aligned terms."""
    @property
    def label(self) -> Optional[Drawable]: ...
    @property
    def equals(self) -> Optional[Drawable]: ...
    @property
    def number(self) -> Drawable: ...
    @property
    def unit(self) -> Optional[Drawable]: ...

class RollingNumber(Drawable):
    """A numeric wheel display. Animate the value with animate.set or count_to.

    Use visual.animate for position, opacity and other drawable animations.
    Formatting is fixed at creation; movement is deterministic under seeks.
    """
    def move_to(self, x: Any, y: Any = None, anchor: Anchor | TextAnchor | None = None) -> RollingNumber:
        """Position the display and return this counter.

        Anchor uses the complete wheel window, including line_height padding;
        omitted anchor preserves Drawable's visual-center placement. TextAnchor
        uses the font baseline of settled digits, allowing alignment with Text.
        TextAnchor requires x and y; either coordinate may be a reactive scalar.
        """
        ...
    def fill(self, paint: Paint) -> RollingNumber:
        """Set the glyph fill and preserve the counter for fluent chaining."""
        ...
    def opacity(self, op: ScalarSource) -> RollingNumber:
        """Set or bind display opacity and return this counter."""
        ...
    @property
    def parameter(self) -> Parameter:
        """Underlying scalar, usable in computed inputs, readouts and sampled drivers."""
        ...
    @property
    def visual(self) -> Drawable:
        """The display drawable; use visual.animate for geometry and appearance animations."""
        ...
    @property
    def current(self) -> float:
        """Return the underlying Parameter's authoring-side current value."""
        ...
    def set(self, value: float, *, snap: bool = False) -> RollingNumber:
        """Set the numeric value immediately and return self; after declaration this is a reversible cut.

        The value is used exactly: a fraction of the smallest display unit
        leaves a wheel between two digits. ``snap=True`` first rounds it to
        the nearest value shown with ``decimals`` (61.7956 → 61.8 with
        ``decimals=1``), so ``current`` matches the display.
        Raise ValueError for non-finite values or abs(value)*10**decimals >= 1e15.
        """
        ...
    @property
    def animate(self) -> Anim:
        """Scalar animation proxy: animate.set(value).duration(seconds).easing(...)."""
        ...
    def count_to(self, value: float, *, duration: float = 1.0, snap: bool = False) -> Anim:
        """Build a count animation for scene.play; accepts Anim easing and delay modifiers.

        The target is used exactly, with no implicit rounding. ``snap=True``
        rounds it to the nearest value shown with ``decimals`` so the wheels
        settle on clean digits and ``current`` matches the display at the end,
        e.g. ``count_to(61.7956, snap=True)`` ends on 61.8 with ``decimals=1``.
        Raise ValueError for an out-of-range value or non-finite/negative duration.
        """
        ...

class Variable(Drawable):
    """A visible ``Parameter`` with an equation-aligned reactive readout group."""
    @property
    def current(self) -> float: ...
    def set(self, value: float) -> None: ...
    @property
    def animate(self) -> Anim:
        """Return a pure scalar animation proxy for ``set(value)``."""
        ...
    def add_updater_fn(
        self,
        callback: Callable[[float, float, float], float],
        *,
        reset: Callable[[], None] | None = None,
        fixed_dt: float | None = None,
    ) -> None:
        """Drive the visible scalar from a callback; deterministic mode requires reset and fixed_dt."""
        ...
    def remove_updater(self) -> None: ...
    @property
    def label(self) -> Optional[Drawable]: ...
    @property
    def equals(self) -> Optional[Drawable]: ...
    @property
    def number(self) -> Drawable: ...
    @property
    def unit(self) -> Optional[Drawable]: ...
_ReactiveScalar: TypeAlias = ScalarSource

class CoordinateRef:
    def place(self, drawable: Drawable) -> Drawable: ...

class DataTable:
    def __init__(self, columns: Mapping[str, Sequence[float | str | None]] | Any) -> None: ...
    def __len__(self) -> int: ...
    @property
    def columns(self) -> list[str]: ...

class DataSource:
    def __init__(self, data: DataTable | Mapping[str, Sequence[float | str | None]] | Any, *, key: Optional[str] = None) -> None: ...
    def replace(self, data: DataTable | Mapping[str, Sequence[float | str | None]] | Any) -> None: ...
    def append(self, data: DataTable | Mapping[str, Sequence[float | str | None]] | Any) -> None: ...
    @property
    def version(self) -> int: ...
    def __len__(self) -> int: ...

class ChartAnimation:
    def to(self, target: ChartSpec, *, match_: Literal["key", "index"] = "key", fallback: Literal["error", "crossfade"] = "error") -> Anim: ...

class Chart:
    """Materialized chart with stable marks, axes, grid, and guide layers."""
    def drawable(self) -> Drawable:
        """Return the root drawable used by layout and generic transforms."""
        ...
    def layer(self, name: Literal["marks", "axes", "grid", "guides", "labels"]) -> Drawable:
        """Return one semantic layer; ``axes`` includes grid, axes, ticks, numbers, and axis labels."""
        ...
    @property
    def animate(self) -> ChartAnimation: ...
    def move_to(self, x: float, y: float) -> Chart:
        """Return a chart handle translated in the 2D canvas plane."""
        ...
    def move_to_3d(self, x: float, y: float, z: float) -> Chart:
        """Return a chart handle translated in world coordinates."""
        ...
    def scale_to(self, factor: float) -> Chart:
        """Return a uniformly scaled chart handle."""
        ...
    def inspect(self, fields: Sequence[str], *, format: Optional[str] = None) -> Chart:
        """Enable preview-only inspection metadata for the selected data fields."""
        ...
    @property
    def inspection_enabled(self) -> bool:
        """Report whether preview inspection was enabled for this handle."""
        ...

class VectorField:
    """A reusable 2D or 3D vector-valued function bound to a coordinate space."""
    @property
    def dimensions(self) -> Literal[2, 3]: ...
    @property
    def evaluation(self) -> Literal["native", "python"]:
        """Report whether symbolic tracing succeeded or Python fallback is used."""
        ...
    def arrows(
        self,
        *,
        resolution: Optional[tuple[int, int] | tuple[int, int, int]] = None,
        min_length: float = 0.0,
        max_length: Optional[float] = None,
        length_scale: float = 1.0,
        width: float = 0.02,
        tip_length: Optional[float] = None,
        tip_width: Optional[float] = None,
        color: Optional[ColorLike] = None,
        colormap: Optional[ColorMapLike] = None,
        color_range: Optional[tuple[float, float]] = None,
    ) -> ArrowVectorField: ...
    def streamlines(
        self,
        *,
        seeds: Optional[tuple[int, int] | tuple[int, int, int]] = None,
        direction: Literal["forward", "backward", "both"] = "both",
        tolerance: float = 1e-4,
        min_step: float = 1e-5,
        max_step: float = 0.1,
        max_time: float = 3.0,
        max_length: Optional[float] = None,
        max_steps: int = 10_000,
        stagnation: float = 1e-10,
        padding: float = 0.05,
        separation: float = 0.035,
        width: float = 0.02,
        opacity: float = 1.0,
        color: Optional[ColorLike] = None,
        colormap: Optional[ColorMapLike] = None,
        color_range: Optional[tuple[float, float]] = None,
    ) -> StreamLines: ...
    def advect(
        self,
        target: Drawable,
        seed: tuple[float, float] | tuple[float, float, float],
        *,
        duration: float = 3.0,
        direction: Literal["forward", "backward", "both"] = "forward",
        tolerance: float = 1e-4,
        min_step: float = 1e-5,
        max_step: float = 0.1,
        max_time: float = 3.0,
        max_length: Optional[float] = None,
        max_steps: int = 10_000,
        stagnation: float = 1e-10,
        padding: float = 0.05,
    ) -> Anim:
        """Advect a drawable's center along one finite seekable trajectory."""
        ...
    def particles(
        self,
        count: int = 32,
        *,
        radius: Optional[float] = None,
        duration: float = 3.0,
        tolerance: float = 1e-4,
        min_step: float = 1e-5,
        max_step: float = 0.1,
        max_time: float = 3.0,
        max_length: Optional[float] = None,
        max_steps: int = 10_000,
        stagnation: float = 1e-10,
        padding: float = 0.05,
        color: Optional[ColorLike] = None,
        colormap: Optional[ColorMapLike] = None,
        color_range: Optional[tuple[float, float]] = None,
        opacity: float = 1.0,
    ) -> FlowParticles: ...

class ArrowVectorField:
    def drawable(self) -> Drawable: ...
    @property
    def animate(self) -> Anim: ...

class StreamLines:
    def drawable(self) -> Drawable: ...
    @property
    def animate(self) -> Anim: ...
    def flow(self, duration: float = 2.0, *, time_width: float = 0.15) -> list[Anim]:
        """Animate brighter moving highlights without clipping the persistent base lines."""
        ...

class FlowParticles:
    def drawable(self) -> Drawable: ...
    @property
    def animate(self) -> Anim: ...
    def flow(self) -> list[Anim]:
        """Return one finite seekable advection clip per retained particle."""
        ...

class CoordinateSpaceAnimation:
    def create(self) -> Anim:
        """Trace coordinate-space vector layers with resolution-independent strokes."""
        ...
    def write(self) -> Anim: ...
    def fade_in(self) -> Anim: ...
    def fade_out(self) -> Anim: ...
    def move_to(self, x: float, y: float) -> Anim: ...
    def scale_to(self, factor: float) -> Anim: ...
    def rotate_to(self, radians: float) -> Anim: ...
    def view_to(self, x_domain: tuple[float, float], y_domain: tuple[float, float]) -> Anim:
        """Animate the data-domain window, keeping axis text and stroke widths.

        The plot area keeps its position; axes, grids, ticks, numbers and
        data marks are clipped to it, and axis titles stay in place. Numbers and
        ``scatter_data`` markers keep their size and proportions.
        Axes, grids and plotted paths keep their stroke widths throughout the zoom.
        Axes with automatic ticks (``Axis.linear(a, b)`` without ``.ticks``)
        regenerate their grid lines, ticks and numbers for the target window:
        the new step cross-fades in while the old one fades out, and a
        zoom-out extends the axis and grid beyond the original domain. Axes
        with a fixed ``.ticks(step)`` keep that step. Style layers before the
        first ``view_to``: regenerated ticks copy the style they find then.
        Returns an unscheduled animation; once played, ``data_to_local``
        follows the new window.
        Raises ValueError for invalid domains or non-linear/time axes.
        """
        ...

class CoordinateSpace:
    """A Cartesian space (``Cartesian2D``) with addressable layers.

    Data marks built from it (``plot``, ``parametric``, ``scatter_data``,
    fields, bars and other statistical marks) are clipped to the plot area,
    also while ``view_to`` changes the window; call ``.no_clip()`` on a mark
    to let it overflow. Objects placed with ``at_coordinate`` are not
    clipped, nor are ``scene.viz.chart`` marks, whose inferred domains
    already fit their data.
    """
    def drawable(self) -> Drawable: ...
    @property
    def animate(self) -> CoordinateSpaceAnimation: ...
    def move_to(self, x: float, y: float) -> CoordinateSpace: ...
    def scale_to(self, factor: float) -> CoordinateSpace: ...
    def rotate_to(self, radians: float) -> CoordinateSpace: ...
    def view_to(self, x_domain: tuple[float, float], y_domain: tuple[float, float]) -> CoordinateSpace:
        """Set the data-domain window at the cursor and return this space.

        The plot area keeps its position; axes, grids, ticks, numbers and
        data marks are clipped to it, and axis titles stay in place. Numbers and
        ``scatter_data`` markers keep their size and proportions while
        following the view positions.
        Axes, grids and plotted paths retain their authored stroke widths.
        Axes with automatic ticks regenerate their grid lines, ticks and
        numbers for the new window; axes with a fixed ``.ticks(step)`` keep it.
        Curves from ``plot`` without ``domain=`` cover every x window authored
        with ``view_to`` (before or after the curve is declared), so they
        reach the edge of a zoomed-out view; an explicit ``domain=`` is always
        kept. ``plot`` and ``parametric`` curves, reactive ones included, are
        sampled with their tolerance divided by the largest view
        magnification so they stay smooth when zoomed in.
        Raises ValueError unless domains are finite and increasing on linear/time axes.
        """
        ...
    def coord(self, x: float, y: float) -> CoordinateRef: ...
    def data_to_local(self, x: float, y: float) -> tuple[float, float]:
        """Map data to this space's local coordinates through the view at the cursor."""
        ...
    def data_to_scene(self, x: float, y: float) -> PointRef:
        """Return a scene-space point at data ``(x, y)`` for objects outside the space.

        The point is resolved every frame through the live ``view_to`` window
        and the space's own move, scale and rotation, so labels, arrows and
        connectors that are not children of the space keep pointing at the
        data. Use it wherever an ``Endpoint`` is accepted, for example
        ``label.follow(plane.data_to_scene(2, 4), offset=(0, 0.3))``.
        Non-finite or out-of-scale data raises ``ValueError``.
        """
        ...
    def local_to_data(self, x: float, y: float) -> tuple[float, float]:
        """Map local coordinates to data through the view at the cursor."""
        ...
    def layer(self, name: Literal["grid", "major_grid", "minor_grid", "axis", "axes", "ticks", "numbers", "labels"]) -> Drawable: ...
    def plot(
        self,
        function: Callable[..., float],
        domain: Optional[tuple[float, float]] = None,
        *,
        samples: Optional[int] = None,
        tolerance: float = 0.0075,
        derivative: Optional[Callable[..., float]] = None,
        inputs: Sequence[Parameter | Variable | Computed | TimeInput] = (),
    ) -> Drawable: ...
    def parametric(self, function: Callable[..., tuple[float, float]], domain: tuple[float, float], *, samples: Optional[int] = None, tolerance: float = 0.0075, inputs: Sequence[Parameter | Variable | Computed | TimeInput] = ()) -> Drawable: ...
    def implicit(self, function: Callable[[float, float], float], *, resolution: tuple[int, int] = (96, 64)) -> Drawable: ...
    def contour(self, function: Callable[[float, float], float], levels: Sequence[float], *, resolution: tuple[int, int] = (96, 64)) -> Drawable: ...
    def field(self, function: Callable[..., tuple[float, float]], *, inputs: Sequence[Parameter | Variable | Computed | TimeInput] = ()) -> VectorField:
        """Evaluate a vector callback from coordinates followed by explicit inputs."""
        ...
    def projections(self, x: float, y: float) -> Drawable: ...
    def secant(self, function: Callable[[float], float], x0: float, x1: float) -> Drawable: ...
    def tangent(self, function: Callable[[float], float], x: float, *, length: Optional[float] = None, dx: Optional[float] = None) -> Drawable: ...
    def normal(self, function: Callable[[float], float], x: float, *, length: Optional[float] = None, dx: Optional[float] = None) -> Drawable: ...
    def area_under(self, function: Callable[[float], float], domain: tuple[float, float], *, samples: int = 160, baseline: float = 0.0) -> Drawable: ...
    def riemann_sum(self, function: Callable[[float], float], domain: tuple[float, float], *, rectangles: int = 12, method: Literal["left", "midpoint", "middle", "right"] = "midpoint", baseline: float = 0.0) -> Drawable: ...
    def plot_data(
        self,
        xs: Sequence[Optional[float]],
        ys: Sequence[Optional[float]],
        *,
        step: bool = False,
        baseline: Optional[float] = None,
        policy: Literal["gap", "drop", "error"] = "gap",
        color: Optional[Color] = None,
        width: Optional[float] = None,
    ) -> Drawable:
        """Plot a raw data series in this space's data coordinates.

        The curve follows the plane, so repositioning the space carries the
        series with it. ``None`` entries mark missing samples; ``policy``
        controls non-finite ones. Pass ``step=True`` for a step chart and
        ``baseline`` (data units) for a filled area.

        Example:
            plane = scene.cartesian_2d(Axis.linear(0, 30), Axis.linear(-0.4, 0.4))
            curve = plane.plot_data(times, accel, color=CYAN, width=0.04)
            scene.play(curve.animate.create().duration(2.0))
        """
        ...
    def scatter_data(
        self,
        xs: Sequence[Optional[float]],
        ys: Sequence[Optional[float]],
        *,
        radius: float = 0.06,
        policy: Literal["gap", "drop", "error"] = "gap",
        color: Optional[Color] = None,
    ) -> Drawable:
        """Plot a data series as scatter dots in this space's data coordinates.

        ``radius`` is in scene units; ``view_to`` moves the dot centers with the
        data window while keeping each marker circular at that radius.

        Example:
            dots = plane.scatter_data(periods, spectral_values, radius=0.07, color=GOLD)
        """
        ...

class NumberLine:
    """A one-dimensional typed coordinate space with scale-aware labels and reactive points."""
    def drawable(self) -> Drawable: ...
    @property
    def animate(self) -> Anim: ...
    def coord(self, value: float) -> CoordinateRef: ...
    def data_to_local(self, value: float) -> float: ...
    def point_ref(
        self,
        value: _ReactiveScalar,
        *,
        normal_offset: Optional[_ReactiveScalar] = None,
    ) -> PointRef:
        """Map a scalar into a reactive point in the line's local frame.

        ``normal_offset`` is measured perpendicular to the line in local canvas
        units; ``None`` means zero. Continuous scales are supported. A
        categorical axis raises ``ValueError`` for reactive scalar values.
        """
        ...
    def function(
        self,
        function: Callable[..., float],
        domain: Optional[tuple[float, float]] = None,
        *,
        normal_scale: float = 1.2,
        reveal: Optional[_ReactiveScalar] = None,
        samples: Optional[int] = None,
        tolerance: float = 0.0075,
        inputs: Sequence[Parameter | Variable | Computed | TimeInput] = (),
    ) -> Drawable:
        """Plot a scalar Python function perpendicular to this number line.

        Function outputs ``-1`` and ``1`` map to ``-normal_scale`` and
        ``normal_scale`` local canvas units. Coordinates are passed before the
        explicitly declared input values. When ``reveal`` is provided, it is the exact
        data-space end of the visible curve, allowing a point and the path to
        share one ``Parameter`` without arc-length drift. Sampling and
        parameter updates remain in Rust.
        Invalid domains, sampling settings, or non-positive scales raise
        ``ValueError``.
        """
        ...
    def layer(self, name: Literal["axis", "ticks", "numbers", "labels"]) -> Drawable: ...

class PolarSpace:
    def drawable(self) -> Drawable: ...
    @property
    def animate(self) -> Anim: ...
    def coord(self, radius: float, angle: float) -> CoordinateRef: ...
    def layer(self, name: Literal["grid", "axes", "numbers", "labels"]) -> Drawable:
        """Return a stable polar layer; disabled layers are empty Drawables."""
        ...
    def plot(self, function: Callable[[float], float], domain: tuple[float, float] = (0.0, 6.283185307179586), *, samples: int = 360) -> Drawable: ...

class CoordinateSpace3D:
    def drawable(self) -> Drawable: ...
    @property
    def animate(self) -> Anim: ...
    def layer(self, name: Literal["grid", "axes", "ticks", "numbers", "labels"]) -> Drawable:
        """Return an independently animatable scale-aware 3D axes layer."""
        ...
    def move_to_3d(self, x: float, y: float, z: float) -> CoordinateSpace3D: ...
    def scale_to(self, factor: float) -> CoordinateSpace3D: ...
    def data_to_local(self, x: float, y: float, z: float) -> tuple[float, float, float]: ...
    def local_to_data(self, x: float, y: float, z: float) -> tuple[float, float, float]: ...
    def surface(self, function: Callable[..., float], *, resolution: tuple[int, int] = (64, 48), inputs: Sequence[Parameter | Variable | Computed | TimeInput] = ()) -> Drawable: ...
    def parametric(self, function: Callable[..., tuple[float, float, float]], domain: tuple[float, float], *, samples: int = 320, inputs: Sequence[Parameter | Variable | Computed | TimeInput] = ()) -> Drawable: ...
    def field(self, function: Callable[..., tuple[float, float, float]], *, inputs: Sequence[Parameter | Variable | Computed | TimeInput] = ()) -> VectorField: ...

Cartesian2D: TypeAlias = CoordinateSpace
Cartesian3D: TypeAlias = CoordinateSpace3D
ComplexSpace: TypeAlias = CoordinateSpace

class Image(Drawable):
    """A local raster image with fluent framing and animatable source crop."""
    def frame(self, width: float, height: float, fit: Literal["contain", "cover", "stretch"] = "contain") -> Self:
        """Set a centered destination frame in scene units and return this object.

        Dimensions must be finite and positive. Later calls create reversible
        cuts at the scene cursor. Contain leaves transparent bands.
        """
        ...
    def crop(self, x: float, y: float, width: float, height: float, *, normalized: bool = False) -> Self:
        """Select source pixels within the fixed frame and return this object.

        Coordinates use the original source's top-left origin, with Y down.
        normalized=True uses fractions of its original dimensions. Invalid or
        out-of-bounds rectangles raise ValueError. Later calls are timeline cuts.
        """
        ...
    def quality(self, value: Literal["low", "medium", "high"]) -> Self:
        """Set raster sampling quality at the cursor; return this object.

        Invalid values raise ValueError. Video frame changes preserve quality.
        """
        ...
    @property
    def source_width(self) -> int:
        """Original source width in pixels, independent of crop and transforms."""
        ...
    @property
    def source_height(self) -> int:
        """Original source height in pixels, independent of crop and transforms."""
        ...

class Video(Drawable):
    """A local video drawable sharing the scene timeline with embedded audio.

    Use either one legacy Scene.play([video]) activation or finite segments.
    Mixing modes on one instance raises ValueError. Fluent setters retain Video.
    """
    def frame(self, width: float, height: float, fit: Literal["contain", "cover", "stretch"] = "contain") -> Self:
        """Set a centered destination frame in scene units and return this object.

        Dimensions must be finite and positive. Later calls create reversible
        cuts at the scene cursor. Contain leaves transparent bands.
        """
        ...
    def crop(self, x: float, y: float, width: float, height: float, *, normalized: bool = False) -> Self:
        """Select source pixels within the fixed frame and return this object.

        Coordinates use the original source's top-left origin, with Y down.
        normalized=True uses fractions of its original dimensions. Invalid or
        out-of-bounds rectangles raise ValueError. Later calls are timeline cuts.
        """
        ...
    def quality(self, value: Literal["low", "medium", "high"]) -> Self:
        """Set raster sampling quality at the cursor; return this object.

        Invalid values raise ValueError. Video frame changes preserve quality.
        """
        ...
    @property
    def source_width(self) -> int:
        """Original source width in pixels, independent of crop and transforms."""
        ...
    @property
    def source_height(self) -> int:
        """Original source height in pixels, independent of crop and transforms."""
        ...
    @property
    def source_duration(self) -> float:
        """Full source duration in seconds, before trim or speed changes."""
        ...
    @property
    def frame_rate(self) -> float:
        """Source frames per second used by deterministic timeline sampling."""
        ...
    def segment(self, *, start: float, end: float, speed: Optional[float] = None, audio: Optional[bool] = None, volume: Optional[float] = None) -> VideoSegment:
        """Declare a finite selection of absolute source seconds for Scene.play.

        Require 0 <= start < end <= source_duration. Omitted controls inherit
        the video's speed, audio and volume, but never its offset/duration/loop.
        Playback lasts (end-start)/speed scene seconds; gaps hold the last frame
        silently. Overlap on this video or use in another scene raises ValueError.
        Creation does not schedule playback. Make a new segment to repeat it.
        """
        ...

class VideoSegment:
    """A single-use, finite Video selection accepted by play and compositions.

    Cannot be stretched. Scheduling is atomic: invalid batches consume nothing.
    """

class Lottie(Drawable):
    """A transformable Lottie JSON or dotLottie composition activated by ``Scene.play``.

    Rendering stays vector-based through Velato/Vello and follows exact scene
    seeks. Unsupported source features may be omitted and are listed in
    ``warnings``. A declaration belongs to one scene and can be activated once.
    ``animate.write()`` and ``animate.create()`` reveal vector contours in
    parallel, then fade in their authored fills and any raster images. These
    introductions do not start source playback; sequence the clip after them
    to animate from its selected first frame.
    """
    @property
    def animation_ids(self) -> list[str]:
        """Animation IDs in manifest order; empty for a JSON source."""
        ...
    @property
    def theme_ids(self) -> list[str]:
        """Available theme IDs in manifest order; empty for JSON."""
        ...
    @property
    def state_machine_ids(self) -> list[str]:
        """Available machine IDs in manifest order; empty for JSON."""
        ...
    def set_theme(self, id: Optional[str] = None) -> Lottie:
        """Apply a static theme at the current cursor; None restores the base.

        Before activation, sets the initial theme. Returns this clip without
        advancing time. Invalid or unsupported themes raise ValueError.
        """
        ...
    def set_input(self, name: str, value: float | int | bool | str) -> Lottie:
        """Set a typed machine input at the cursor, or initially before play.

        Returns this clip without advancing time. Unknown inputs, mismatched
        types and non-finite numbers raise ValueError. Bool is distinct from
        numeric inputs; other Python objects raise TypeError.
        """
        ...
    def fire_event(self, name: str) -> Lottie:
        """Fire a declared event at the current cursor and return this clip.

        Requires activation by Scene.play. Invalid names or inactive clips
        raise ValueError. Same-time commands preserve declaration order and
        replay deterministically when seeking and exporting.
        """
        ...
    @property
    def source_width(self) -> int: ...
    @property
    def source_height(self) -> int: ...
    @property
    def frame_rate(self) -> float: ...
    @property
    def source_duration(self) -> float: ...
    @property
    def warnings(self) -> list[str]: ...

class Geometry:
    """Scene-owned factory for vector, path, boolean, 3D, and reactive geometry."""
    def circle(self, radius: float) -> Drawable:
        """Create a circle drawable in the scene.

        Example:
            result = scene.circle(1.0)
        """
        ...
    def cube(self, size: float = 2.0, *, material: Optional[Material3D] = None) -> Primitive3D:
        """Create a centered cube with flat faces."""
        ...
    def sphere(self, radius: float = 1.0, *, segments: int = 32, rings: int = 16, material: Optional[Material3D] = None) -> Primitive3D:
        """Create a smooth-shaded UV sphere."""
        ...
    def cylinder(self, radius: float = 1.0, height: float = 2.0, *, segments: int = 32, caps: bool = True, material: Optional[Material3D] = None) -> Primitive3D:
        """Create a Y-up cylinder."""
        ...
    def cone(self, radius: float = 1.0, height: float = 2.0, *, segments: int = 32, cap: bool = True, material: Optional[Material3D] = None) -> Primitive3D:
        """Create a Y-up cone."""
        ...
    def plane(self, width: float = 2.0, height: float = 2.0, *, subdivisions: tuple[int, int] = (1, 1), material: Optional[Material3D] = None) -> Primitive3D:
        """Create an XZ plane with upward-facing normals."""
        ...
    def lighting_3d(self, preset: Literal["studio", "none"] = "studio", intensity: float = 1.0, shadows: bool = True) -> None:
        """Configure the scene's single automatic 3D light rig."""
        ...
    def rect(self, width: float, height: float) -> Drawable:
        """Create a rect drawable in the scene.

        Example:
            result = scene.rect(1.0, 1.0)
        """
        ...
    def rounded_rect(self, width: float, height: float, radius: float) -> Drawable:
        """Create a rounded rect drawable in the scene.

        Example:
            result = scene.rounded_rect(1.0, 1.0, 1.0)
        """
        ...
    def surrounding_rect(
        self,
        targets: Drawable | TextSelection | Sequence[Drawable | TextSelection],
        *,
        padding: Padding = 0.12,
        corner_radius: float = 0.08,
    ) -> SurroundingRect:
        """Create a live outline around objects, text parts, or equation parts.

        ``padding`` accepts a scalar, ``(vertical, horizontal)``, or
        ``(top, right, bottom, left)`` in scene units. Targets are combined by
        their world-space axis-aligned bounds. The default theme foreground
        stroke is used with no fill. Invalid dimensions, empty targets, and
        targets from another Scene raise ``ValueError`` or ``TypeError``.
        """
        ...
    def square(self, s: float) -> Drawable:
        """Create a square drawable in the scene.

        Example:
            result = scene.square(1.0)
        """
        ...
    def dot(self, radius: float) -> Drawable:
        """Create a dot drawable in the scene.

        Example:
            result = scene.dot(1.0)
        """
        ...
    def ellipse(self, rx: float, ry: float) -> Drawable:
        """Create a ellipse drawable in the scene.

        Example:
            result = scene.ellipse(1.0, 1.0)
        """
        ...
    @overload
    def line(self, *, length: float, direction: Direction = Direction.RIGHT) -> Drawable:
        """Create a line centered at the origin with length in scene units.

        The default is horizontal. ``direction`` sets its orientation; a later
        ``next_to`` sets placement independently without changing its length.
        Length must be finite and positive; direction must be a finite nonzero
        2D vector. Invalid geometry raises ``ValueError``. Combining this form
        with endpoints or coordinates raises ``TypeError``.

        Example:
            rule = scene.geometry.line(length=3).next_to(title, Direction.DOWN, spacing=0.2)
        """
        ...
    @overload
    def line(self, p1: Endpoint, p2: Endpoint) -> Drawable: ...
    @overload
    def line(self, x1: float, y1: float, x2: float, y2: float) -> Drawable:
        """Create a line between fixed or reactive endpoints.

        The preferred two-argument form accepts 2D/3D tuples, drawables,
        ``PointRef`` values, and ``AnchorPoint`` values. Reference endpoints
        are resolved every frame, so the line follows moving objects. The
        four-coordinate form remains available for compatibility. Invalid
        endpoint shapes or mixed arities raise ``TypeError``.

        Example:
            result = scene.line((-100.0, 0.0), card.anchor_point(Anchor.LEFT))
        """
        ...
    def connector(self, start: Endpoint, end: Endpoint, *, via: Optional[Sequence[Endpoint]] = None, head_length: float = 0.18, head_width: float = 0.15, body_width: float = 0.036, max_head_ratio: Optional[float] = None) -> Drawable:
        """Filled reactive arrow through optional waypoints, in world units.

        Head length is capped to the last nonzero segment; width scales with it.
        References must belong to this Scene. Coincident points are ignored.
        One Drawable supports fill, opacity and create; no obstacle routing.
        """
        ...
    def arrow(self, x1: float, y1: float, x2: float, y2: float, *, head_length: Optional[float] = None, head_width: Optional[float] = None, body_width: Optional[float] = None, max_head_ratio: Optional[float] = None) -> Drawable:
        """Create a solid arrow with optional dimensions in scene units.

        Omitted dimensions use head length 0.18, head width 0.15 and body
        width 0.036. max_head_ratio in (0, 1] caps head length relative to arrow
        length and scales head width proportionally; None leaves it uncapped.
        Nonfinite endpoints or nonpositive/nonfinite dimensions raise
        ValueError. Coincident endpoints produce an empty path.

        Example:
            result = scene.geometry.arrow(-1, 0, 1, 0, head_length=0.18,
                head_width=0.15, body_width=0.036, max_head_ratio=0.3)
        """
        ...
    def dashed_line(
        self, x1: float, y1: float, x2: float, y2: float, *, dash_length: float = 0.16, gap_length: float = 0.10
    ) -> Drawable:
        """Create a dashed line drawable in the scene.

        ``create()`` draws the dashes one after another from the start point.

        Example:
            result = scene.dashed_line(1.0, 1.0, 1.0, 1.0)
        """
        ...
    def double_arrow(
        self, x1: float, y1: float, x2: float, y2: float, *, head_length: Optional[float] = None, head_width: Optional[float] = None
    ) -> Drawable:
        """Create a filled double-headed arrow in scene units.

        Omitted head metrics use 0.18 by 0.15 with a 0.036 body; heads shrink
        so they never overlap.

        Example:
            result = scene.geometry.double_arrow(-3, 0, 3, 0)
        """
        ...
    def points(self, positions: Sequence[tuple[float, float]], radius: float = 0.06) -> Drawable:
        """Create one drawable with a filled circle of ``radius`` at each position.

        Positions are scene coordinates and need no ``Cartesian2D``. Thousands
        of points stay a single object, so ``fill``, ``opacity``, ``move_to``
        and ``animate`` apply to the whole cloud; the theme styles it through
        the ``points`` shape role. An empty list, non-finite positions, or a
        non-positive radius raise ``ValueError``.

        Example:
            section = scene.geometry.points(poincare_points, radius=0.02).fill(GOLD)
        """
        ...
    def polygon(self, points: Sequence[tuple[float, float]]) -> Drawable:
        """Create a polygon drawable in the scene.

        Example:
            result = scene.polygon([(0.0, 0.0), (1.0, 1.0)])
        """
        ...
    def star(self, points: int, outer_radius: float, inner_radius: float) -> Drawable:
        """Create a star drawable in the scene.

        Example:
            result = scene.star(5, 40.0, 40.0)
        """
        ...
    def regular_polygon(self, sides: int, radius: float) -> Drawable:
        """Create a regular polygon drawable in the scene.

        Example:
            result = scene.regular_polygon(2, 40.0)
        """
        ...
    def sector(self, cx: float, cy: float, radius: float, start_angle: float, sweep_angle: float) -> Drawable:
        """Create a sector drawable in the scene.

        Example:
            result = scene.sector(1.0, 1.0, 40.0, 1.0, 1.0)
        """
        ...
    def annulus(self, outer_radius: float, inner_radius: float) -> Drawable:
        """Create a annulus drawable in the scene.

        Example:
            result = scene.annulus(40.0, 40.0)
        """
        ...
    def brace(self, x1: float, y1: float, x2: float, y2: float, height: float) -> Drawable:
        """Create a brace drawable in the scene.

        Example:
            result = scene.brace(1.0, 1.0, 1.0, 1.0, 40.0)
        """
        ...
    def checkmark(self, size: float) -> Drawable:
        """Create a checkmark drawable in the scene.

        Example:
            result = scene.checkmark(40.0)
        """
        ...
    def cross(self, size: float) -> Drawable:
        """Create a cross drawable in the scene.

        Example:
            result = scene.cross(40.0)
        """
        ...
    def right_angle(self, arm_length: float) -> Drawable:
        """Create a right angle drawable in the scene.

        Example:
            result = scene.right_angle(40.0)
        """
        ...
    def arc(self, cx: float, cy: float, radius: float, start_angle: float, sweep_angle: float) -> Drawable:
        """Create a arc drawable in the scene.

        Example:
            result = scene.arc(1.0, 1.0, 40.0, 1.0, 1.0)
        """
        ...
    def curved_arrow(self, x1: float, y1: float, x2: float, y2: float, angle: float, *, head_length: Optional[float] = None, head_width: Optional[float] = None, body_width: Optional[float] = None, max_head_ratio: Optional[float] = None) -> Drawable:
        """Create a curved arrow between two points deflected by ``angle`` radians.

        The sign of ``angle`` selects the bulge side. Dimensions work as in
        ``arrow``: omitted values use head length 0.18, head width 0.15 and
        body width 0.036 in scene units, and ``max_head_ratio`` in (0, 1] caps
        head length relative to the arc length, scaling head width with it.
        The head still shortens to fit short arcs and narrows on tight radii.
        Nonfinite coordinates or nonpositive/nonfinite dimensions raise
        ValueError.

        Example:
            result = scene.geometry.curved_arrow(-3, 0, 3, 0, 0.9,
                head_length=0.3, head_width=0.24, body_width=0.06)
        """
        ...
    def curved_arrow_arc(self, cx: float, cy: float, radius: float, start_angle: float, sweep_angle: float, *, head_length: Optional[float] = None, head_width: Optional[float] = None, body_width: Optional[float] = None, max_head_ratio: Optional[float] = None) -> Drawable:
        """Create a curved arrow along a circular arc; angles are in radians.

        Accepts the same dimensions, defaults and validation as ``curved_arrow``;
        ``max_head_ratio`` is relative to the arc length ``radius * |sweep_angle|``.

        Example:
            result = scene.geometry.curved_arrow_arc(0, 0, 2.5, 0.2, 1.8)
        """
        ...
    @overload
    def path(self, definition: Sequence[CurvePoint]) -> Drawable:
        """Create a path drawable in the scene.

        Example:
            result = scene.path([(0.0, 0.0), (1.0, 1.0)])
        """
        ...
    @overload
    def path(self, definition: Sequence[CurveCommand]) -> Drawable:
        """Create a path drawable in the scene.

        Example:
            result = scene.path([(0.0, 0.0), (1.0, 1.0)])
        """
        ...
    def polyline(self, points: Sequence[tuple[float, float]]) -> Drawable:
        """Create a polyline drawable in the scene.

        Example:
            result = scene.polyline([(0.0, 0.0), (1.0, 1.0)])
        """
        ...
    def polyline_3d(
        self,
        points: Sequence[tuple[float, float, float]],
        color: Optional[Color] = None,
        *,
        colors: Optional[Sequence[Color]] = None,
        colormap: Optional[str] = None,
    ) -> Drawable:
        """Create a 3D line strip from world-space points.

        Provide ``color`` for a uniform line, ``colors`` for one color per
        point, or ``colormap`` for ``"inferno"``, ``"viridis"``, or
        ``"plasma"``. The color list must match ``points`` in length.

        Example:
            path = scene.polyline_3d(
                [(-2, 0, 0), (0, 1, 1), (2, 0, 0)],
                colormap="viridis",
            )
        """
        ...
    def bezier(self, start: tuple[float, float], controls: Sequence[tuple[float, float]], end: tuple[float, float]) -> Drawable:
        """Create a bezier drawable in the scene.

        Example:
            result = scene.bezier((0.0, 0.0), [(-0.5, 1.0), (0.5, -1.0)], (0.0, 0.0))
        """
        ...
    def curve(self, commands: Sequence[CurveCommand]) -> Drawable:
        """Create a composed curve from ``move``, ``line``, ``quad``, ``cubic``, and close commands.

        Append ``_rel`` to a drawing command to use cursor-relative points.
        Quadratic and cubic controls accept a point, ``None``, or ``"auto"``.
        Use ``close`` or ``close_smooth`` with an empty argument sequence.
        """
        ...
    def transform_matching_shapes(self, source: Drawable, target: Drawable, *, duration: float = 1.0) -> None:
        """Configure or query the scene with transform matching shapes.

        Example:
            scene.transform_matching_shapes(source, target)
        """
        ...
    def transform_matching(self, source: Drawable, target: Drawable, *, mode: str = "shapes", duration: float = 1.0) -> None:
        """Configure or query the scene with transform matching.

        Example:
            scene.transform_matching(source, target)
        """
        ...
    def group(self, members: Sequence[Drawable]) -> Drawable:
        """Create a group drawable in the scene.

        Grouping preserves each member's authored local coordinates, including
        coordinates returned by ``add_updater_fn``. Existing visible members do
        not become hidden merely because the group also contains a deferred
        force or trace; ``write()`` and ``create()`` on the group explicitly
        reveal those deferred descendants.

        Example:
            result = scene.group([drawable])
        """
        ...
    def union(self, *operands: Drawable, live: bool = False, tolerance: float = 0.25, rule: Literal["nonzero", "evenodd"] = "nonzero") -> Drawable:
        """Return the union of at least two vector drawables.

        Sources remain available. With ``live=True`` the result follows source
        path and transform changes; invalid scenes, operands, rules, or
        tolerances raise ``ValueError``.
        """
        ...
    def intersection(self, *operands: Drawable, live: bool = False, tolerance: float = 0.25, rule: Literal["nonzero", "evenodd"] = "nonzero") -> Drawable:
        """Return the shared vector area of at least two drawables."""
        ...
    def difference(self, subject: Drawable, *clips: Drawable, live: bool = False, tolerance: float = 0.25, rule: Literal["nonzero", "evenodd"] = "nonzero") -> Drawable:
        """Subtract each clip from the subject in deterministic left-to-right order."""
        ...
    def xor(self, *operands: Drawable, live: bool = False, tolerance: float = 0.25, rule: Literal["nonzero", "evenodd"] = "nonzero") -> Drawable:
        """Return the symmetric difference of at least two vector drawables."""
        ...
    def fill_level(self, mask: Drawable, paint: Paint, level: Optional[ScalarSource] = None, *, direction: Literal["up", "down", "left", "right"] = "up", keep_outline: bool = True) -> Drawable:
        """Create a vector fill clipped to mask; None starts empty.

        A number sets a level in [0, 1]. Parameter, Variable and Computed sources
        bind the level and clamp finite samples to [0, 1]. Foreign sources and
        invalid fixed levels raise ValueError. Animate the source while bound;
        set_fill_level(number) ends the binding reversibly. keep_outline retains
        the mask as a visible outline when requested.
        """
        ...
    def point_on_curve(self, curve: Drawable, tracker: Parameter) -> Drawable:
        """Create a hidden point-on-curve drawable; reveal it in ``scene.play``.

        Example:
            result = scene.point_on_curve(curve, None)
        """
        ...
    def tangent_on_curve(self, curve: Drawable, tracker: Parameter, length: float = 0.8) -> Drawable:
        """Create a hidden tangent drawable; reveal it in ``scene.play``.

        Example:
            result = scene.tangent_on_curve(curve, None)
        """
        ...
    def normal_on_curve(self, curve: Drawable, tracker: Parameter, length: float = 0.8) -> Drawable:
        """Create a hidden normal drawable; reveal it in ``scene.play``.

        Example:
            result = scene.normal_on_curve(curve, None)
        """
        ...
    def curvature_on_curve(self, curve: Drawable, tracker: Parameter, window: float = 0.02) -> Drawable:
        """Create a hidden osculating-circle drawable; reveal it in ``scene.play``.

        Example:
            result = scene.curvature_on_curve(curve, None)
        """
        ...
    def always_redraw_arc(
        self,
        tracker: Parameter,
        cx: float,
        cy: float,
        radius: float,
        start_angle: float,
        sweep_scale: float = 1.0,
        sweep_offset: float = 0.0,
    ) -> Drawable:
        """Create a hidden always-redrawn arc; reveal it in ``scene.play``.

        Example:
            result = scene.always_redraw_arc(None, 1.0, 1.0, 40.0, 1.0)
        """
        ...
    def traced_path(
        self,
        source: Drawable,
        *,
        dissipating_time: Optional[float] = None,
        max_points: Optional[int] = None,
        min_distance: float = 0.01,
    ) -> Drawable:
        """Trace a moving drawable's position; reveal the trail in ``scene.play``.

        ``dissipating_time`` makes samples expire after the given number of
        seconds. ``max_points`` caps retained samples and ``min_distance``
        filters nearby samples. The trail remains hidden until a ``fade_in``
        animation is included in ``scene.play(...)``.

        Example:
            result = scene.traced_path(source, dissipating_time=2.0)
        """
        ...
    def traced_path_3d(
        self,
        source: Drawable,
        *,
        colormap: Optional[str] = None,
        dissipating_time: Optional[float] = None,
        max_points: Optional[int] = None,
        min_distance: float = 0.1,
    ) -> Drawable:
        """Trace a moving drawable's 3D world-space position; reveal it in ``scene.play``.

        ``dissipating_time`` makes samples expire after the given number of
        seconds. ``max_points`` limits retained samples. ``min_distance`` ignores
        samples that are closer than the given world-space distance. Supported
        colormaps are ``"inferno"``, ``"viridis"``, and ``"plasma"``.

        Example:
            dot = scene.dot(7).move_to_3d(1, 0, 0)
            dot.add_updater(Updater.orbit(0, 0, 1, 1.5))
            trail = scene.traced_path_3d(
                dot, colormap="viridis", max_points=600
            )
        """
    def tracking_line(
        self,
        from_: Endpoint,
        to: Endpoint,
    ) -> Drawable:
        """Create a hidden line whose endpoints react in the same frame.

        Endpoints may be fixed tuples, drawable origins, or ``AnchorPoint``
        references inside transformed hierarchies. Reveal the line in
        ``scene.play``.

        Example:
            result = scene.tracking_line(drawable, drawable)
        """
        ...
    def point_ref(self, x: _ReactiveScalar, y: _ReactiveScalar) -> PointRef:
        """Create a non-rendered point whose coordinates react to scalar sources."""
        ...
    def offset_point(self, origin: Endpoint, dx: _ReactiveScalar, dy: _ReactiveScalar) -> PointRef:
        """Create a point offset from a moving origin by reactive scene-space components."""
        ...
    def point_between(self, from_: Endpoint, to: Endpoint, *, alpha: float = 0.5, offset: tuple[float, float] = (0.0, 0.0)) -> PointRef:
        """Create an affine point between endpoints plus a world-space offset."""
        ...
    def polar_point(self, origin: Endpoint, radius: _ReactiveScalar, angle: _ReactiveScalar) -> PointRef:
        """Create a reactive polar point; angle is measured in radians."""
        ...

class Typography:
    """Callable scene-owned typography API for structured text, equations, Typst, measurement, and code."""
    def __call__(
        self,
        *content: TextContent,
        role: Optional[TextRole] = None,
        style: Optional[TextStyle] = None,
        flow: Optional[TextFlow] = None,
        font: Optional[str] = None,
        math_font: Optional[str] = None,
        size: Optional[float] = None,
        weight: Optional[int] = None,
        italic: Optional[bool] = None,
        color: Optional[ColorLike] = None,
        opacity: Optional[float] = None,
        letter_spacing: Optional[float] = None,
        word_spacing: Optional[float] = None,
        baseline: Optional[float] = None,
        wrap: Optional[TextWrap] = None,
        text_align: Optional[TextAlign] = None,
        line_spacing: Optional[float] = None,
        max_lines: Optional[int] = None,
        overflow: Optional[TextOverflow] = None,
        direction: Optional[TextDirection] = None,
        hyphenate: Optional[bool] = None,
        lang: Optional[str] = None,
        markup: Optional[bool] = None,
    ) -> Text:
        """Create structured vector text, paragraphs, mathematics, or mixed content.

        ``lang`` (for example ``"es"``) selects language-specific hyphenation
        and typography, as in ``TextFlow``; combine it with ``hyphenate=True``
        for justified Spanish paragraphs.

        ``color`` accepts Color, CSS/hex strings, RGB/RGBA byte tuples, or
        None to inherit the style/theme color.

        ``*strong*`` selects bold text and ``_emphasis_`` selects italic text;
        escape literal markers as ``\\*`` and ``\\_``. Markers inside
        ``$...$`` remain math syntax, and ``\\$`` emits a literal dollar.
        ``markup=False`` keeps every ``*`` and ``_`` literal (and their
        backslashes), for technical labels such as ``tb:dist_comp`` or
        ``X1_2``; ``$...$`` math still applies. ``markup=None`` uses the
        theme's ``text_markup`` (``True`` without a theme).

        Without ``flow`` or ``text_align``, the lines of a text with explicit
        line breaks take their horizontal alignment from the anchor of
        ``move_to``: left anchors align left, right anchors align right, and
        centered anchors center. An explicit ``flow`` or ``text_align``
        always wins.
        Unbalanced or crossed markup, unbalanced math, duplicate sibling part
        names, and invalid metrics raise ``ValueError``. Direct keywords
        override reusable style/flow objects. Responsive wrapping consumes the
        Layout-v2 width offer or the scene safe frame; outer box dimensions
        remain Layout properties. Default theme sizes, in scene units, are
        0.64 for title, 0.48 for subtitle/heading, 0.40 for body, 0.32 for
        caption, 0.36 for label/code, and 0.44 for math.

        Example:
            formula = part("formula", "$E = ", part("mass", "m", color=GOLD), " c^2$")
            copy = scene.text("La *energía* es ", formula, role="body", flow=TextFlow(align="justify"))
        """
        ...
    def equation(
        self,
        *content: TextContent,
        role: Optional[TextRole] = None,
        style: Optional[TextStyle] = None,
        flow: Optional[TextFlow] = None,
        font: Optional[str] = None,
        math_font: Optional[str] = None,
        size: Optional[float] = None,
        weight: Optional[int] = None,
        italic: Optional[bool] = None,
        color: Optional[ColorLike] = None,
        opacity: Optional[float] = None,
        letter_spacing: Optional[float] = None,
        word_spacing: Optional[float] = None,
        baseline: Optional[float] = None,
        wrap: Optional[TextWrap] = None,
        text_align: Optional[TextAlign] = None,
        line_spacing: Optional[float] = None,
        max_lines: Optional[int] = None,
        overflow: Optional[TextOverflow] = None,
        direction: Optional[TextDirection] = None,
        hyphenate: Optional[bool] = None,
    ) -> Text:
        """Create a standalone display equation as structured vector text.

        ``color`` accepts Color, CSS/hex strings, RGB/RGBA byte tuples, or
        None to inherit the style/theme color.

        The content is wrapped internally as ``$ ... $`` and accepts the same
        semantic parts, styles, flow options, selections, and animations as
        :meth:`text`. Omit the surrounding math delimiters. The spaces next to
        those delimiters are preserved to select Typst block math. Every
        content boundary becomes ordinary Typst whitespace, so ``"="`` does
        not need a written trailing space. Empty content and invalid
        mathematics raise ``ValueError``. With no explicit role or size, a
        display equation uses the 44-unit math default.

        Example:
            equation = scene.equation(
                part("sum_force", "sum F_t"),
                "=",
                parts(mass="m", acceleration="a_t"),
            )
            scene.play([equation.animate.write(by="part").duration(1.0)])
        """
        ...
    def typst(self, source: str | os.PathLike[str], *, width: Optional[str | float | int] = None) -> Drawable:
        """Create a Typst drawable from inline markup or a Typst asset.

        A string is compiled as inline Typst. An ``os.PathLike`` value loads a
        ``.typ`` asset; relative paths use :meth:`assets_dir`. The document
        keeps Typst's own proportions and is scaled so its default 11pt text
        is as large as the ``body`` text role; ``#set text(size: 22pt)`` is
        therefore twice the body size, and table insets and rule widths scale
        with it. ``width`` is a Typst page width (``"16cm"``, ``"800pt"``; a
        number means points) measured before that scaling. Empty inline
        source raises ``ValueError`` and an unreadable asset raises
        ``RuntimeError``.

        Example:
            from pathlib import Path
            result = scene.typst(Path("assets/title.typ"))
        """
        ...
    def measure(
        self,
        content: str,
        *,
        role: Optional[TextRole] = None,
        size: Optional[float] = None,
        font: Optional[str] = None,
        color: Optional[Color] = None,
        wrap: Optional[float] = None,
        weight: Optional[int] = None,
        style: Optional[TextStyle] = None,
        markup: Optional[bool] = None,
        flow: Optional[TextFlow] = None,
        line_spacing: Optional[float] = None,
    ) -> tuple[float, float]:
        """Measure laid-out text without spawning it.

        Uses the same pipeline that renders ``scene.text`` (role defaults from
        the active theme and Typst shaping) and returns ``(width, height)`` in
        scene units. ``wrap`` composes at a fixed line width; ``None``
        measures a single unwrapped block. ``style`` overlays a ``TextStyle``
        (weight, italic, spacing, …); ``size``, ``font``, ``weight`` and
        ``color`` override it. ``markup`` matches ``scene.text``: with markup
        on, ``*`` and ``_`` are markup and are not measured as characters;
        ``None`` uses the theme's ``text_markup``. ``flow`` measures with a
        ``TextFlow`` (alignment, line spacing, hyphenation, …); its ``"auto"``
        wrap measures unwrapped because no layout width is offered. ``wrap``
        and ``line_spacing`` override the flow. Empty content, an invalid
        weight, an invalid line spacing or unbalanced markup raise
        ``ValueError``.

        Example:
            width, height = scene.text.measure("PGA = 0.35 g", role="label")
            box = scene.geometry.rounded_rect(width + 0.56, height + 0.32, 0.14)
        """
        ...
    def code(
        self,
        source: str,
        *,
        language: str = "text",
        width: float = 7.6,
        height: float = 3.0,
        font_size: float = 0.2,
        background: Optional[Color] = None,
        color: Optional[Color] = None,
        accent: Optional[Color] = None,
    ) -> Drawable:
        """Create a code drawable in the scene.

        Example:
            result = scene.code("example")
        """
        ...

class LayoutBuilder:
    """Scene-owned factory for responsive layouts, items, constraints, and templates."""
    def card(self, children: Sequence[Drawable | Layout | LayoutItem], *, direction: Literal["column", "row", "stack"] = "column", gap: float = 0.24, padding: Padding = 0.0, width: SizeRule = "hug", height: SizeRule = "hug", align: Align = "center", justify: Justify = "start", background: Optional[Paint] = None, border: Optional[Paint] = None, border_width: float = 0.025, radius: float = 0.08, ports: Optional[dict[str, Anchor | tuple[Anchor, tuple[float, float]]]] = None) -> Layout:
        """Compose arbitrary children inside a persistent card in scene units.

        Background and border default to transparent. The rounded background
        follows the resolved outer box, including padding, through reflow and
        seek. It does not affect content measurement or count. Radius is capped
        to half the smaller box dimension. Style it via card.background.
        Ports map names to anchors or (anchor, offset) pairs. Invalid dimensions,
        names or offsets raise ValueError; content uses normal layout ownership.
        """
        ...
    def row(self, children: Sequence[Drawable | Layout | LayoutItem], *, gap: float = 0.24, padding: Padding = 0.0, width: SizeRule = "hug", height: SizeRule = "hug", align: Align = "center", justify: Justify = "start", wrap: bool = False, within: Optional[Literal["safe", "frame"]] = None) -> Layout:
        """Create a horizontal Layout v2 container in canvas units.

        ``width`` and ``height`` accept fixed values, ``"hug"``, or ``"fill"``.
        Responsive text keeps the width offered by its final row allocation,
        so tight glyph bounds do not trigger a second, narrower composition.
        Ownership errors are raised before render as ``LayoutOwnershipError``.
        """
        ...
    def column(self, children: Sequence[Drawable | Layout | LayoutItem], *, gap: float = 0.24, padding: Padding = 0.0, width: SizeRule = "hug", height: SizeRule = "hug", align: Align = "start", justify: Justify = "start", wrap: bool = False, within: Optional[Literal["safe", "frame"]] = None) -> Layout:
        """Create a vertical Layout v2 container with optional wrapping.

        Responsive text is composed at the width offered by the column, even
        when its visible glyph bounds are narrower.
        """
        ...
    def grid(self, children: Sequence[Drawable | Layout | LayoutItem], *, rows: int | Sequence[Track] = 1, columns: int | Sequence[Track] = 1, gap: float = 0.0, row_gap: Optional[float] = None, column_gap: Optional[float] = None, padding: Padding = 0.0, width: SizeRule = "hug", height: SizeRule = "hug", align: Align = "stretch", justify: Justify = "start", auto_flow: Literal["row", "column"] = "row", within: Optional[Literal["safe", "frame"]] = None) -> Layout:
        """Create a grid with fixed, ``"auto"``, or ``"<weight>fr"`` tracks.

        Explicit rows/columns and spans are reserved before deterministic
        auto-placement. Responsive text uses its final track allocation.
        Invalid tracks, collisions, or overflow raise errors.
        """
        ...
    def stack(self, children: Sequence[Drawable | Layout | LayoutItem], *, padding: Padding = 0.0, width: SizeRule = "hug", height: SizeRule = "hug", align: Align = "center", within: Optional[Literal["safe", "frame"]] = None) -> Layout:
        """Create an overlay Layout; use item anchors and offsets for placement.

        Responsive text retains the width offered by the overlay container.
        """
        ...
    def item(self, child: Drawable | Layout, *, grow: float = 0.0, shrink: float = 1.0, align: Optional[Align] = None, row: Optional[int] = None, column: Optional[int] = None, row_span: int = 1, column_span: int = 1, absolute: bool = False, anchor: Optional[Anchor] = None, offset: tuple[float, float] = (0.0, 0.0), fit: Fit = "none") -> LayoutItem:
        """Return per-child layout metadata without creating another Drawable.

        ``fit="cover"`` clips media to its allocated box; ``absolute=True``
        removes the item from normal flow. Negative grow/shrink values error.
        """
        ...
    def constrain(self, *constraints: LayoutConstraint) -> ConstraintSet:
        """Register prioritized linear relations and return their count.

        Conflicting required relations or cross-scene references raise
        ``ValueError`` immediately; ``animate`` is a transition duration.
        """
        ...
    def check_layout(self) -> list[str]:
        """Return current constraint and intrinsic-composition diagnostics.

        Invalid responsive text or Typst math is reported here without
        terminating editor hot reload.
        """
        ...
    def template(self, template: Callable[..., Layout], **slots: Any) -> Layout:
        """Instantiate a signature-checked Python template and return its root Layout."""
        ...

class MediaLibrary:
    """Scene-owned loader for image, SVG, glTF, video, Lottie, and audio assets."""
    def audio(
        self,
        path: str,
        *,
        duration: Optional[float] = None,
        volume: float = 1.0,
        fade_in: float = 0.0,
        fade_out: float = 0.0,
    ) -> Audio:
        """Declare a validated audio file for explicit playback.

        The declaration is inert until passed to ``Scene.play``. Playback then
        begins at that call's absolute timeline cursor and follows pause, seek,
        and speed in preview and MP4/WebM export. Invalid paths or timing values
        raise ``ValueError``.

        Example:
            music = scene.audio("music.ogg", volume=0.5)
            scene.play([music])
        """
        ...
    def image(
        self,
        path: str,
        *,
        width: Optional[float] = None,
        height: Optional[float] = None,
        fit: str = "contain",
        crop: Optional[tuple[float, float, float, float]] = None,
        quality: Literal["low", "medium", "high"] = "medium",
    ) -> Image:
        """Create a local Image with fluent framing and source metadata.

        ``quality`` selects Vello image sampling: ``"low"`` is nearest-like,
        ``"medium"`` is bilinear-like, and ``"high"`` requests bicubic sampling.
        Invalid fit, crop, dimensions, or quality values raise ``ValueError``.

        Example:
            result = scene.media.image("assets/example.png").frame(8, 4.5)
        """
        ...
    def video(
        self,
        path: str,
        *,
        width: Optional[float] = None,
        height: Optional[float] = None,
        fit: Literal["contain", "cover", "stretch"] = "contain",
        crop: Optional[tuple[float, float, float, float]] = None,
        quality: Literal["low", "medium", "high"] = "medium",
        offset: float = 0.0,
        duration: Optional[float] = None,
        loop: bool = False,
        speed: float = 1.0,
        audio: bool = True,
        volume: float = 1.0,
    ) -> Video:
        """Declare a timeline-synchronized MP4 drawable for explicit playback.

        The declaration is inert until included in ``Scene.play``; that call
        fixes the absolute start of both frames and embedded audio. ``offset``
        and ``duration`` select source seconds, ``loop`` repeats that interval,
        and ``speed`` preserves audio pitch. Finite non-looping video contributes
        its selected output duration to the play batch. Requires ``ffmpeg`` and
        ``ffprobe`` and raises ValueError for invalid ranges or RuntimeError for
        media failures.
        """
        ...
    def lottie(
        self,
        path: str,
        *,
        animation_id: Optional[str] = None,
        theme_id: Optional[str] = None,
        state_machine_id: Optional[str] = None,
        width: Optional[float] = None,
        height: Optional[float] = None,
        fit: Literal["contain", "cover", "stretch"] = "contain",
        offset: float = 0.0,
        duration: Optional[float] = None,
        loop: bool = False,
        speed: float = 1.0,
    ) -> Lottie:
        """Load local Lottie JSON or a dotLottie v1/v2 package as a vector drawable.

        Package selectors choose an animation, static theme or state machine.
        ``animation_id`` and ``state_machine_id`` are mutually exclusive; absent
        selectors use the manifest initial content, then the first animation.
        Machine activation does not extend the scene; use ``Scene.wait`` to
        author its duration. Machines reject non-default offset/duration/loop/
        speed options and control playback through their states instead.
        Packaged images are decoded in memory. Invalid packages, selectors,
        themes and unsupported machine features raise ``ValueError``.
        JSON does not accept package selectors.

        ``offset`` and ``duration`` select source seconds; ``speed`` must be
        positive. ``width`` and ``height`` use scene units and ``fit`` follows
        image/video sizing. Solid layers are rendered at the root and inside
        precompositions; external image assets in both locations are resolved
        relative to the JSON file. Linear and radial gradient fills/strokes
        preserve static or animated color and opacity stops. Omitted layer or
        shape-group transform positions default to zero translation and omitted
        scales to 100%. The selected first frame is displayed until the value is
        passed once to ``Scene.play``. Playback follows the scene clock from
        its scheduled start, including backward timeline seeks.
        ``animate.fade_in()``, ``animate.write()``, and ``animate.create()``
        can introduce that first frame before playback. Write/create trace
        vector contours in parallel, then fade in fills and raster images.
        Invalid options raise ``ValueError``; file, JSON, image, and unsafe
        importer failures raise ``RuntimeError`` rather than aborting the scene
        load.
        """
        ...
    def svg(self, path: str) -> Drawable:
        """Create an SVG hierarchy whose fluent stroke widths use logical scene units.

        Example:
            result = scene.svg("assets/example.svg")
        """
        ...
    def gltf(self, path: str, *, scene: str | int | None = None) -> Drawable:
        """Import a local glTF 2.0 ``.gltf`` or ``.glb`` model."""
        ...

class Visualization:
    """Scene-owned API for reactive values, coordinate spaces, charts, and matrices."""
    def rolling_number(
        self, value: float = 0.0, *, decimals: int = 0, min_digits: int = 1,
        group_separator: str = "", decimal_separator: str = ".",
        prefix: str = "", suffix: str = "", show_plus: bool = False,
        font_family: Optional[str] = None, weight: Optional[int] = None,
        font_size: float = 0.75, digit_spacing: float = 0.02, line_height: float = 1.25,
        mode: str = "odometer", direction: str = "up", color: Optional[Color] = None,
    ) -> RollingNumber:
        """Create a right-anchored rolling counter with fixed-width digit cells.

        decimals is 0..6; min_digits counts zero-padded integer positions (1..15;
        their sum is at most 15). Group/decimal separators are zero-or-one/one
        characters. Affixes are single-line, at most 256 UTF-8 bytes combined.
        Sizes and spacing use scene units; line_height is a digit-ink-height multiplier
        of at least 1. All dimensions must be finite, font_size positive and
        digit_spacing non-negative. Invalid options raise ValueError.

        odometer carries higher wheels in the final smallest unit;
        continuous turns all wheels at their place-value speed. direction is
        up or down for increasing magnitude; decreasing values reverse it.
        Negative values roll their magnitude with a static minus sign.
        Fractional smallest units intentionally show a wheel between digits;
        use representable endpoints for settled digits (no implicit rounding).
        In continuous mode, wheels spin freely while count_to/animate.set tweens
        the parameter and settle over the first and last 15% of each tween, so the
        final value reads cleanly; untweened sources (computed values, time, sample
        drivers) keep free continuous wheels.
        font_family=None inherits the scene's body font when compiled, including
        theme typography; an explicit family overrides it with normal font fallback.
        Families and weight (1..1000) resolve exactly like ``scene.text``: by the
        family stored in each font, including ``Theme(font_files=...)`` and variable
        fonts. Sources driven outside the
        finite abs(value)*10**decimals < 1e15 range display an em dash.
        """
        ...
    @property
    def time(self) -> TimeInput: ...
    def parameter(self, initial: float) -> Parameter: ...
    def chart(self, spec: ChartSpec) -> Chart:
        """Materialize an immutable declarative chart using batched semantic layers."""
        ...
    def cartesian_2d(
        self,
        x: Axis,
        y: Axis,
        *,
        width: Optional[float] = None,
        height: Optional[float] = None,
        grid: bool = True,
        axes: bool = True,
        ticks: bool = True,
        numbers: bool = True,
        labels: bool = True,
        x_axis: Optional[bool] = None,
        y_axis: Optional[bool] = None,
        x_grid: Optional[bool] = None,
        y_grid: Optional[bool] = None,
        x_ticks: Optional[bool] = None,
        y_ticks: Optional[bool] = None,
        x_numbers: Optional[bool] = None,
        y_numbers: Optional[bool] = None,
        x_labels: Optional[bool] = None,
        y_labels: Optional[bool] = None,
    ) -> Cartesian2D:
        """Create a typed 2D Cartesian space with configurable semantic layers.

        Global switches default to ``True``. A non-``None`` per-axis switch
        overrides its global value. ``numbers`` controls tick text while
        ``labels`` controls titles authored with ``Axis.label``. Disabled
        layers remain addressable as empty ``Drawable`` objects.
        """
        ...
    def cartesian_3d(
        self,
        x: Axis,
        y: Axis,
        z: Axis,
        *,
        size: tuple[float, float, float] = (10.0, 8.0, 6.0),
        grid: bool = True,
        axes: bool = True,
        ticks: bool = True,
        numbers: bool = True,
        labels: bool = True,
        x_axis: Optional[bool] = None,
        y_axis: Optional[bool] = None,
        z_axis: Optional[bool] = None,
        xy_grid: Optional[bool] = None,
        xz_grid: Optional[bool] = None,
        yz_grid: Optional[bool] = None,
        x_ticks: Optional[bool] = None,
        y_ticks: Optional[bool] = None,
        z_ticks: Optional[bool] = None,
        x_numbers: Optional[bool] = None,
        y_numbers: Optional[bool] = None,
        z_numbers: Optional[bool] = None,
        x_labels: Optional[bool] = None,
        y_labels: Optional[bool] = None,
        z_labels: Optional[bool] = None,
    ) -> Cartesian3D:
        """Create typed 3D axes with independently selectable planes and annotations.

        ``xy_grid``, ``xz_grid`` and ``yz_grid`` override ``grid`` when set;
        axis-specific ticks, numbers, and titles follow the same precedence.
        Hidden layers remain available as empty ``Drawable`` objects.
        """
        ...
    def polar(
        self,
        radial: Axis,
        *,
        radius: float = 2.2,
        angle_divisions: int = 12,
        grid: bool = True,
        axes: bool = True,
        numbers: bool = True,
        labels: bool = True,
        rings: Optional[bool] = None,
        spokes: Optional[bool] = None,
    ) -> PolarSpace:
        """Create a polar space with independently selectable rings and spokes.

        ``rings`` and ``spokes`` inherit ``grid`` when omitted. ``labels``
        controls the radial title from ``Axis.label``; ``numbers`` controls
        radial tick text.
        """
        ...
    def complex(
        self,
        x: Optional[Axis] = None,
        y: Optional[Axis] = None,
        *,
        width: Optional[float] = None,
        height: Optional[float] = None,
        grid: bool = True,
        axes: bool = True,
        ticks: bool = True,
        numbers: bool = True,
        labels: bool = True,
        x_axis: Optional[bool] = None,
        y_axis: Optional[bool] = None,
        x_grid: Optional[bool] = None,
        y_grid: Optional[bool] = None,
        x_ticks: Optional[bool] = None,
        y_ticks: Optional[bool] = None,
        x_numbers: Optional[bool] = None,
        y_numbers: Optional[bool] = None,
        x_labels: Optional[bool] = None,
        y_labels: Optional[bool] = None,
    ) -> ComplexSpace:
        """Create a configurable Cartesian complex plane.

        Visibility switches and per-axis precedence match ``cartesian_2d``;
        omitted axes retain the default ``Re`` and ``Im`` titles.
        """
        ...
    def readout(self, source: _ReactiveScalar | Callable[..., float], *, inputs: Sequence[Parameter | Variable | Computed | TimeInput] = (), label: Optional[str] = None, format: str = ".2f", prefix: str = "", suffix: str = "", unit: Optional[str] = None, font_size: Optional[float] = None, color: Optional[Color] = None, invalid: str = "invalid", decimal_separator: str = ".") -> Readout:
        """Create a native numeric display with equally spaced, baseline-aligned terms.

        The label, equality sign, number, and unit all use ``font_size``;
        omitting it selects the shared 0.48-unit reactive annotation size.
        ``color`` applies to the label, reactive value, and unit and remains in
        effect when the number changes or the timeline seeks.
        ``decimal_separator`` replaces the ``.`` between integer and fractional
        digits; ``","`` also turns ``,`` grouping into ``.`` (``1.234,50``).
        It must be one character that is not a digit, sign, space, ``e`` or
        ``%``; otherwise ``ValueError`` is raised.
        """
        ...
    def variable(self, initial: float, *, label: str, format: str = ".2f", prefix: str = "", suffix: str = "", unit: Optional[str] = None, font_size: Optional[float] = None, color: Optional[Color] = None, invalid: str = "invalid", decimal_separator: str = ".") -> Variable:
        """Create an animatable scalar displayed as an aligned equation row.

        Every visible term uses ``font_size``, or 0.48 units when omitted.
        ``color`` applies to every visible term, including the changing value.
        ``decimal_separator`` works as in ``readout`` (for example ``","``
        shows ``3,14``).
        """
        ...
    def number_line(
        self,
        axis: Axis,
        *,
        length: Optional[float] = None,
        axis_visible: bool = True,
        ticks: bool = True,
        numbers: bool = True,
        labels: bool = True,
    ) -> NumberLine:
        """Create a typed number line with independently visible components.

        ``numbers`` controls tick text and ``labels`` controls the title from
        ``Axis.label``. Disabled components remain addressable as empty layers.
        ``axis_visible`` avoids colliding with the existing ``axis`` argument.
        """
        ...
    def matrix(
        self,
        data: Any,
        *,
        row_gap: float = 0.24,
        column_gap: float = 0.24,
        delimiter_gap: float = 0.12,
        delimiters: Literal["brackets", "parentheses", "braces", "bars", "double_bars", "none"] = "brackets",
        delimiter_size: float | None = None,
        delimiter_weight: int = 300,
        row_labels: Sequence[Any] | None = None,
        column_labels: Sequence[Any] | None = None,
        label_mode: Literal["math", "text"] = "math",
        cell_mode: Literal["math", "text"] = "math",
        entry_style: Any | None = None,
        label_style: Any | None = None,
        cell_factory: Callable[[Any, int, int], Drawable] | None = None,
        numeric_format: str = "g",
    ) -> Matrix:
        """Create a selectable Layout-backed matrix.

        ``data`` must be a non-empty rectangular sequence or a SymPy matrix.
        Entries remain individual drawables; rows, columns, blocks and
        diagonals can therefore be animated independently. Invalid dimensions,
        ``row_gap`` and ``column_gap`` control automatic tracks. Delimiters
        accept a size and CSS-like weight from 100 through 900. Labels default
        to Typst math; ``cell_mode``/``label_mode`` may select plain text.
        ``cell_factory(value, row, column)`` can return a custom Drawable.
        Invalid dimensions, labels, modes, weights, delimiters, or factories
        raise ``ValueError``/``TypeError``. Returns :class:`gaanim.Matrix`.
        """
        ...

class SlideKit:
    """Scene-owned editorial component and presentation-branding toolkit."""
    def brand(
        self,
        *,
        logo: Optional[str] = None,
        footer: Optional[str] = None,
        slide_numbers: bool = True,
        rule: bool = True,
        show_on_cover: bool = False,
        logo_scale: float = 1.0,
    ) -> None:
        """Configure the logo, rule, footer and slide number drawn in every segment.

        The logo (SVG or raster) is fitted to 0.6 scene units tall and then
        multiplied by ``logo_scale``; it sits in the top-right safe corner
        above the slide content.

        Example:
            scene.slides.brand(logo="assets/logo.svg", footer="LAB · 2026")
        """
        ...
    def badge(
        self,
        text: str,
        *,
        variant: Literal["neutral", "accent", "success", "warning", "danger"] = "neutral",
        appearance: Literal["soft", "solid", "outline"] = "soft",
        padding: tuple[float, float] = (0.18, 0.10),
        radius: Optional[float] = None,
        font_size: Optional[float] = None,
        min_width: Optional[float] = None,
        color: Optional[Color] = None,
        background: Optional[Color] = None,
        border: Optional[Color] = None,
        font: Optional[str] = None,
        weight: Optional[int] = None,
        style: Optional[TextStyle] = None,
        markup: Optional[bool] = None,
    ) -> Drawable:
        """Create an auto-sized editorial badge at the scene origin.

        ``radius=None`` produces a pill. Semantic variants inherit Theme color
        tokens; explicit colors override them. Empty text or invalid finite
        geometry raises ``ValueError``. Position the returned group with
        ``.move_to(...)`` and animate it like any other ``Drawable``.

        The label uses the theme's ``label`` role. ``style`` overlays any
        ``TextStyle`` on it; ``font``, ``weight`` and ``font_size`` override
        that style, and a ``style`` color overrides the variant text color.
        ``markup=False`` keeps ``*`` and ``_`` literal, as in ``scene.text``.
        The panel is sized from the same text that is drawn. An invalid
        ``weight`` or unbalanced markup raises ``ValueError``.

        Example:
            tag = scene.badge("READY", variant="success").move_to(-3, 1.8)
            code = scene.badge("_vel_max", font="Cascadia Mono", weight=600, markup=False)
            scene.play(tag.animate.grow_from_center())
        """
        ...
    def chip(
        self,
        text: str,
        *,
        dot: bool = True,
        variant: Literal["neutral", "accent", "success", "warning", "danger"] = "neutral",
        appearance: Literal["soft", "solid", "outline"] = "soft",
        padding: tuple[float, float] = (0.14, 0.08),
        radius: Optional[float] = None,
        font_size: Optional[float] = None,
        color: Optional[Color] = None,
        background: Optional[Color] = None,
        border: Optional[Color] = None,
        font: Optional[str] = None,
        weight: Optional[int] = None,
        style: Optional[TextStyle] = None,
        markup: Optional[bool] = None,
    ) -> Drawable:
        """Create a compact auto-sized chip with an optional semantic dot.

        The result starts at the origin and is a normal animatable group.
        Unknown variants/appearances, empty text, or invalid geometry raise
        ``ValueError``. ``font``, ``weight``, ``style`` and ``markup`` style
        the label exactly as in :meth:`badge`.

        Example:
            chip = scene.chip("Live", variant="danger", appearance="solid")
        """
        ...
    def card(
        self,
        title: str,
        body: Optional[str] = None,
        footer: Optional[str] = None,
        *,
        width: float = 4.2,
        min_height: float = 1.8,
        padding: tuple[float, float] = (0.28, 0.24),
        gap: float = 0.14,
        radius: float = 0.18,
        variant: Literal["neutral", "accent", "success", "warning", "danger"] = "neutral",
        appearance: Literal["soft", "solid", "outline"] = "soft",
        color: Optional[Color] = None,
        background: Optional[Color] = None,
        border: Optional[Color] = None,
    ) -> Drawable:
        """Create an auto-height card with title, body, and footer text slots.

        Text wraps inside ``width`` using semantic Theme roles. Empty supplied
        slots or invalid dimensions raise ``ValueError``.

        Example:
            card = scene.card("Result", "The solver converged.", "12 ms")
        """
        ...
    def banner(
        self,
        title: str,
        subtitle: Optional[str] = None,
        *,
        position: Literal["top", "bottom"] = "top",
        width: Optional[float] = None,
        margin: float = 0.32,
        padding: tuple[float, float] = (0.28, 0.18),
        gap: float = 0.08,
        radius: float = 0.14,
        variant: Literal["neutral", "accent", "success", "warning", "danger"] = "neutral",
        appearance: Literal["soft", "solid", "outline"] = "soft",
        color: Optional[Color] = None,
        background: Optional[Color] = None,
        border: Optional[Color] = None,
    ) -> Drawable:
        """Create an auto-height banner anchored to a safe top or bottom edge.

        ``width=None`` fills the safe frame minus ``margin``. Empty text,
        invalid placement strings, and invalid dimensions raise ``ValueError``.

        Example:
            notice = scene.banner("Simulation complete", position="bottom")
        """
        ...
    def lower_third(
        self,
        title: str,
        subtitle: Optional[str] = None,
        *,
        kicker: Optional[str] = None,
        side: Literal["left", "right"] = "left",
        width: float = 5.2,
        margin: float = 0.32,
        padding: tuple[float, float] = (0.28, 0.20),
        gap: float = 0.08,
        radius: float = 0.16,
        variant: Literal["neutral", "accent", "success", "warning", "danger"] = "neutral",
        appearance: Literal["soft", "solid", "outline"] = "soft",
        color: Optional[Color] = None,
        background: Optional[Color] = None,
        border: Optional[Color] = None,
    ) -> Drawable:
        """Create a lower-third anchored to a safe bottom corner.

        Kicker, title, and subtitle use Theme text roles. Invalid side names,
        empty supplied slots, or invalid dimensions raise ``ValueError``.

        Example:
            speaker = scene.lower_third("Ada Lovelace", "Mathematician")
        """
        ...
    def stat_card(
        self,
        value: str,
        label: str,
        *,
        delta: Optional[str] = None,
        width: float = 2.8,
        min_height: float = 1.7,
        padding: tuple[float, float] = (0.24, 0.20),
        gap: float = 0.08,
        radius: float = 0.18,
        variant: Literal["neutral", "accent", "success", "warning", "danger"] = "neutral",
        appearance: Literal["soft", "solid", "outline"] = "soft",
        color: Optional[Color] = None,
        background: Optional[Color] = None,
        border: Optional[Color] = None,
    ) -> Drawable:
        """Create an auto-height metric card with value, label, and delta.

        The value and delta use the semantic variant tone. Empty fields or
        invalid dimensions raise ``ValueError``.

        Example:
            metric = scene.stat_card("98%", "Accuracy", delta="+4.2%", variant="success")
        """
        ...
    def quote_card(
        self,
        quote: str,
        attribution: Optional[str] = None,
        *,
        width: float = 6.2,
        padding: tuple[float, float] = (0.32, 0.28),
        gap: float = 0.16,
        radius: float = 0.18,
        variant: Literal["neutral", "accent", "success", "warning", "danger"] = "neutral",
        appearance: Literal["soft", "solid", "outline"] = "soft",
        color: Optional[Color] = None,
        background: Optional[Color] = None,
        border: Optional[Color] = None,
    ) -> Drawable:
        """Create a wrapped quotation card with optional attribution.

        Empty supplied text or invalid dimensions raise ``ValueError``; the
        returned group supports all normal ``Drawable`` animations.

        Example:
            quote = scene.quote_card("Simplicity is prerequisite for reliability.", "E. Dijkstra")
        """
        ...
    def section_header(
        self,
        title: str,
        *,
        kicker: Optional[str] = None,
        subtitle: Optional[str] = None,
        width: float = 7.2,
        align: Literal["left", "center", "right"] = "left",
        rule: bool = False,
        padding: tuple[float, float] = (0.24, 0.18),
        gap: float = 0.10,
        radius: float = 0.12,
        variant: Literal["neutral", "accent", "success", "warning", "danger"] = "neutral",
        appearance: Literal["soft", "solid", "outline"] = "soft",
        color: Optional[Color] = None,
        background: Optional[Color] = None,
        border: Optional[Color] = None,
    ) -> Drawable:
        """Create a section heading with optional kicker and subtitle.

        ``align`` controls all text slots. The horizontal accent rule is hidden
        by default and can be enabled with ``rule=True``. Unknown
        alignment/style strings, empty supplied slots, or invalid dimensions
        raise ``ValueError``.

        Example:
            heading = scene.section_header("Method", kicker="02", align="left")
        """
        ...
    def callout(
        self,
        text: str,
        target: Drawable,
        *,
        offset: tuple[float, float] = (1.6, 0.96),
        width: float = 2.4,
        height: float = 0.72,
        background: Optional[Color] = None,
        color: Optional[Color] = None,
    ) -> Drawable:
        """Create a callout drawable in the scene.

        Example:
            result = scene.callout("example", target)
        """
        ...
    def title_card(
        self,
        title: str,
        subtitle: Optional[str] = None,
        *,
        width: float = 7.6,
        height: float = 3.2,
        panel: bool = False,
        background: Optional[Color] = None,
        color: Optional[Color] = None,
        accent: Optional[Color] = None,
    ) -> Drawable:
        """Create a title card drawable in the scene.

        Example:
            result = scene.title_card("example")
        """
        ...
    def bullets(
        self,
        items: Sequence[str],
        *,
        width: float = 7.2,
        gap: float = 0.68,
        bullet_radius: float = 0.08,
        bullet_color: Optional[Color] = None,
        color: Optional[Color] = None,
    ) -> Drawable:
        """Create a bullets drawable in the scene.

        Example:
            result = scene.bullets(["Example"])
        """
        ...
    def table(
        self,
        headers: Sequence[str],
        rows: Sequence[Sequence[str]],
        *,
        width: float = 7.6,
        row_height: float = 0.58,
        header_background: Optional[Color] = None,
        rule_color: Optional[Color] = None,
        color: Optional[Color] = None,
    ) -> Drawable:
        """Create a table drawable in the scene.

        Example:
            result = scene.table(["Example"], [["Example"]])
        """
        ...

class Mechanics:
    """Scene-owned technical drawing and mechanism toolkit."""
    def dimension(self, x1: float, y1: float, x2: float, y2: float, offset: float) -> Drawable:
        """Create a static technical dimension offset perpendicularly by ``offset``.

        Uses 0.02-unit extension lines and the default ``double_arrow`` heads
        in scene units.

        Example:
            result = scene.mechanics.dimension(-3, 0, 3, 0, 0.6)
        """
        ...
    def bar_between(
        self,
        from_: Endpoint,
        to: Endpoint,
        *,
        width: float = 0.08,
    ) -> Drawable:
        """Create a round-capped reactive bar between two endpoints.

        ``width`` is measured in scene units and must be finite and positive.
        The returned drawable remains fully styleable.
        """
        ...
    def spring_between(
        self,
        from_: Endpoint,
        to: Endpoint,
        coils: int = 8,
        amplitude: float = 0.12,
        crossing: float = 0.0,
        start_straight: float = 0.12,
        end_straight: float = 0.12,
    ) -> Drawable:
        """Create a hidden reactive helical spring; reveal it in ``scene.play``.

        Endpoints may also be ``AnchorPoint`` references inside transformed
        groups. The native helix is regenerated every frame, preserving its
        radius while its pitch deforms with the distance.
        ``crossing`` ranges from 0 to 1: higher values make each turn fold
        back briefly, creating e-like visual crossings.
        ``start_straight`` and ``end_straight`` are non-negative scene-unit
        lengths of the straight segments at each endpoint; both default to 0.12.
        They are shortened proportionally when the endpoints are too close.
        Non-finite or negative straight lengths raise ``ValueError``.

        Example:
            spring = scene.spring_between((0, 0), drawable)
        """
        ...
    def dimension_between(
        self,
        from_: Endpoint,
        to: Endpoint,
        offset: float,
        *,
        label: Optional[str] = None,
        show_value: bool = False,
        value: Optional[_ReactiveScalar] = None,
        format: str = ".2f",
        unit: Optional[str] = None,
        scale: float = 1.0,
        label_gap: float = 0.10,
        label_orientation: Literal["upright", "aligned"] = "upright",
        font_size: Optional[float] = None,
        color: Optional[Color] = None,
        line_width: float = 0.03,
        extension_style: Literal["solid", "dashed"] = "solid",
        dash_length: float = 0.12,
        gap_length: float = 0.08,
        side: Optional[Literal["left", "right", "above", "below"]] = None,
        font: Optional[str] = None,
        weight: Optional[int] = None,
        label_style: Optional[TextStyle] = None,
    ) -> Dimension:
        """Create a reactive technical dimension and optional annotation.

        The line follows fixed points, drawable origins, or anchored points.
        ``label`` remains symbolic; ``show_value`` adds the current XY distance
        multiplied by ``scale`` and formatted with ``format``/``unit``. Passing
        ``value`` (a number, ``Parameter``, ``Variable``, or ``Computed``)
        implies the numeric readout and takes precedence over both measured
        distance and ``scale`` while the dimension geometry keeps following its
        endpoints.
        ``label_orientation`` keeps text horizontal or aligned while avoiding
        upside-down labels. Upright labels on steep lines move outward by the
        part of their width that exceeds their height, keeping the
        ``label_gap`` clearance of horizontal dimensions. ``color`` initializes the extension lines,
        solid triangular arrowheads and the complete annotation, including its
        reactive value. Math labels and reactive values share one 0.48-unit typographic baseline by default, including
        subscripted formulas. ``line_width`` controls the filled line geometry
        and sizes the arrowheads (six line widths long, capped for short spans);
        dashed extensions use ``dash_length`` and ``gap_length``.

        Without ``side``, the sign of ``offset`` picks the side relative to
        the ``from_`` → ``to`` direction (positive is to its left).
        ``side`` fixes it in scene terms instead, and ``offset`` becomes
        only the distance: the dimension stays on that side even when the
        endpoints swap or move past each other. When the line runs along the
        requested direction (``"above"`` on a vertical dimension), the
        offset is used as a positive distance.

        ``label_style`` overlays a ``TextStyle`` on the label, the value and
        the unit; ``font`` and ``weight`` override it, and ``font_size``
        overrides its size. Its color, when set, overrides ``color`` for the
        text only. Unset fields use the theme's body text. Invalid metrics,
        extension styles, orientation, side, or weight raise ``ValueError``.

        Example:
            width = scene.dimension_between(
                left, right, 0.45, label="$W_f$", show_value=True, unit="mm"
            )
            height = scene.mechanics.dimension_between(
                base, top, 0.45, side="right", show_value=True,
                font="Cascadia Mono", weight=600,
            )
        """
        ...
    def angle_between(
        self,
        vertex: Endpoint,
        from_: AngleRay,
        to: AngleRay,
        *,
        radius: float = 0.64,
        label: Optional[str] = None,
        show_value: bool = False,
        format: str = ".1f",
        unit: Literal["deg", "rad"] = "deg",
        sweep: Literal["minor", "major", "cw", "ccw"] = "minor",
        arrowheads: Literal["none", "start", "end", "both"] = "both",
        label_gap: float = 0.12,
        label_orientation: Literal["upright", "aligned"] = "upright",
        show_extensions: bool = True,
        font_size: Optional[float] = None,
        color: Optional[Color] = None,
    ) -> AngleDimension:
        """Create a same-frame angular dimension from fixed directions or endpoints.

        ``color`` applies to the arc, arrows, label, reactive value, and unit.
        All annotation terms default to the shared 0.48-unit reactive size.
        Degenerate rays hide the geometry; invalid modes or metrics raise ``ValueError``.
        """
        ...
    def vector_between(self, from_: Endpoint, to: Endpoint, *, label: Optional[str] = None, show_value: bool = False, format: str = ".1f", unit: Optional[str] = None, scale: float = 1.0, label_gap: float = 0.14, font_size: Optional[float] = None, color: Optional[Color] = None) -> ForceVector:
        """Create a reactive vector with accessible shaft, solid head, and readout parts.

        ``color`` applies to the vector and every readout term, including the
        reactive numeric value after updates and seeks. Labels, values, and
        units default to 0.48 scene units.
        """
        ...
    def force_at(self, origin: Endpoint, magnitude: _ReactiveScalar, *, direction: _ReactiveScalar = 0.0, visual_scale: float = 1.0, label: Optional[str] = None, show_value: bool = False, format: str = ".1f", unit: str = "N", label_gap: float = 0.14, font_size: Optional[float] = None, color: Optional[Color] = None) -> ForceVector:
        """Create a reactive force from physical magnitude and direction in radians.

        ``visual_scale`` converts physical units into scene units and must be
        positive. The optional readout reports the physical magnitude, and
        ``color`` also applies to that changing number. Its complete annotation
        row defaults to 0.48 scene units.
        """
        ...
    def force_from_components(self, origin: Endpoint, fx: _ReactiveScalar, fy: _ReactiveScalar, *, visual_scale: float = 1.0, label: Optional[str] = None, show_value: bool = False, format: str = ".1f", unit: str = "N", label_gap: float = 0.14, font_size: Optional[float] = None, color: Optional[Color] = None) -> ForceVector:
        """Create a reactive force from physical X/Y components relative to a moving origin.

        ``color`` applies to the force and the complete reactive readout, whose
        terms default to 0.48 scene units.
        """
        ...
    def support_at(self, point: Endpoint, *, kind: Literal["fixed", "pin", "roller", "simple", "guided", "prismatic", "cable", "spring"] = "pin", direction: Optional[Direction] = None, size: float = 0.48, ground_length: float = 0.70, color: Optional[Color] = None) -> Support:
        """Create a theme-aware vector support following ``point``.

        Direction runs from the base toward the connection; sizes are scene units.
        """
        ...
    def fixed_support(self, point: Endpoint, *, direction: Optional[Direction] = None, size: float = 0.48, ground_length: float = 0.70, color: Optional[Color] = None) -> Support:
        """Create a fixed or ceiling support with plate and consistent hatching."""
        ...
    def pin_support(self, point: Endpoint, *, direction: Optional[Direction] = None, size: float = 0.48, ground_length: float = 0.70, color: Optional[Color] = None) -> Support:
        """Create a triangular pinned support with a circular joint."""
        ...
    def roller_support(self, point: Endpoint, *, direction: Optional[Direction] = None, size: float = 0.48, ground_length: float = 0.70, color: Optional[Color] = None) -> Support:
        """Create a triangular support on two aligned rollers."""
        ...
    def guided_support(self, point: Endpoint, *, direction: Optional[Direction] = None, size: float = 0.48, ground_length: float = 0.70, color: Optional[Color] = None) -> Support:
        """Create a guided carriage support aligned with ``direction``."""
        ...
    def joint_at(self, point: Endpoint, *, kind: Literal["revolute", "prismatic"] = "revolute", axis: Optional[Direction] = None, size: float = 0.36, color: Optional[Color] = None) -> Drawable:
        """Create a standalone reactive revolute or prismatic joint symbol."""
        ...
    def gear(self, radius: float, teeth: int, *, bore_radius: float = 0.08, color: Optional[Color] = None) -> Drawable:
        """Create an editorial gear silhouette; geometry is illustrative, not manufacturing involute."""
        ...
    def rack(self, length: float, teeth: int, *, color: Optional[Color] = None) -> Drawable:
        """Create an editorial straight rack with evenly spaced teeth."""
        ...
    def cam_profile(self, samples: Sequence[tuple[float, float]], *, bore_radius: float = 0.08, color: Optional[Color] = None) -> Drawable:
        """Create a closed radial cam from ``(angle_radians, radius)`` samples."""
        ...
    def contact_on_curve(self, curve: Drawable, tracker: Parameter | Variable, *, tangent_length: float = 0.8, normal_length: float = 0.8) -> Drawable:
        """Group a reactive contact point, tangent, and normal on a sampled curve."""
        ...
    def moment_about(self, center: Endpoint, radius: float, *, direction: Literal["cw", "ccw"] = "ccw", label: Optional[str] = None, color: Optional[Color] = None) -> Drawable:
        """Create a curved moment arrow that follows a reactive center."""
        ...
    def coordinate_frame_at(self, origin: Endpoint, x_direction: Direction, *, length: float = 0.70, labels: Optional[tuple[str, str]] = None, color: Optional[Color] = None) -> Drawable:
        """Create a reactive orthogonal 2D coordinate frame at an endpoint."""
        ...

GOLD: Color
CORAL: Color
BLUE: Color
WHITE: Color
BLACK: Color
RED: Color
GREEN: Color
YELLOW: Color
ORANGE: Color
PURPLE: Color
PINK: Color
GRAY: Color
CYAN: Color
NAVY: Color
TEAL: Color

class AssetManager:
    """Scene-owned project asset resolution, preload, and reload controller."""
    def assets_dir(self, path: str) -> None:
        """Use assets dir on this Scene or create the requested value.

        Example:
            scene.assets_dir("example")
        """
        ...
    def preload(self, paths: Sequence[str]) -> None:
        """Validate local assets and cache raster, Lottie JSON and .lottie resources.

        dotLottie packages preload their default animation or state machine.
        Example:
            scene.preload(["assets/example.svg"])
        """
        ...
    def load_project(self, path: str | None = None) -> None:
        """Load a project manifest and set its asset directory.

        With no path, reads ``gaanim.toml`` beside the calling Python script.
        An explicit path is used as provided; assets are resolved relative to
        the selected manifest. Raises RuntimeError if it cannot be read.

        Example:
            scene.load_project()
        """
        ...
    def reload_assets(self) -> None:
        """Clear raster, Lottie JSON, dotLottie package and glTF asset caches.

        Existing clips retain their resources; subsequent loads see disk changes.
        Example:
            scene.reload_assets()
        """
        ...

class Scene:
    @property
    def time(self) -> TimeInput:
        """Return absolute timeline seconds as an explicit reactive input.

        This is the same source as ``scene.viz.time``. Pass it to ``computed``
        or an absolute property setter; it follows exact seeks and export.
        """
        ...
    def random(self, seed: int = 0) -> Random:
        """Return a seeded random stream for placing and varying objects.

        The same seed always yields the same values on every platform, so
        scenes stay reproducible across previews and exports.

        Example:
            rng = scene.random(seed=42)
            dots = [scene.geometry.dot().move_to(rng.uniform(-6, 6), rng.uniform(-3, 3)) for _ in range(40)]
        """
        ...
    def noise(
        self,
        *,
        frequency: float = 1.0,
        amplitude: float = 1.0,
        octaves: int = 1,
        seed: int = 0,
        center: float = 0.0,
    ) -> Computed:
        """Return smooth seeded noise over timeline time as a reactive scalar.

        The value is ``center`` plus fractal simplex noise within
        ``[-amplitude, amplitude]``; ``frequency`` sets how fast it changes
        and ``octaves`` (1 to 8) adds finer detail. It is evaluated natively
        each frame without calling Python, and playback, seeks and export
        agree. Invalid values raise ``ValueError``.

        Example:
            drift = scene.noise(frequency=0.6, amplitude=0.3, octaves=3, seed=5)
            logo.rotate_to(computed(lambda v: 0.1 * v, inputs=[drift]))
        """
        ...
    def __init__(
        self,
        *,
        frame: tuple[float, float] = (16.0, 9.0),
        background: Optional[BackgroundLike] = None,
        margin: Optional[float] = None,
        theme: Optional[str | Theme] = None,
        post: Optional[PostProcess] = None,
    ) -> None:
        """Create a resolution-independent scene in logical units.

        ``frame`` is ``(16, 9)`` by default, centered at the origin. Geometry,
        margins, text sizes, strokes, and effects use the same logical unit;
        output pixels are selected by the editor or exporter. ``post`` applies
        a ``PostProcess`` to every segment that does not override it.
        Non-finite or non-positive frame dimensions, invalid WGSL, and unknown
        themes raise ``ValueError``. The former ``Scene(width, height)`` pixel
        API is not accepted.
        """
        ...
    @property
    def canvas(self) -> Canvas:
        """Read the canvas value from this Scene.

        Example:
            value = scene.canvas
        """
        ...
    @property
    def camera(self) -> Camera:
        """Read the camera value from this Scene.

        Example:
            value = scene.camera
        """
        ...
    def segment(
        self,
        name: str,
        transition: Optional[Transition] = None,
        *,
        notes: Optional[str] = None,
        template: Optional[Callable[..., Layout]] = None,
        background: Optional[BackgroundLike] = None,
        post: Optional[PostProcess | Literal[False]] = None,
    ) -> Segment:
        """Create and activate a named structural segment.

        ``background`` accepts the same color, brush, or shader background as
        ``Scene`` and only applies while this segment is active. When omitted,
        the segment uses the scene background. ``post`` replaces the scene
        post-process while this segment is active; ``False`` draws the segment
        without post-processing and ``None`` inherits ``scene.canvas.post``. Any other
        value raises ``TypeError``. Empty or duplicate names, and a transition
        on the first segment, raise ``ValueError``.

        Example:
            result = scene.segment("example", background="#0f172a")
        """
        ...
    def link(self, from_: Segment, to: Segment, transition: Transition) -> None:
        """Schedule link on the scene timeline.

        Example:
            scene.link(intro, details, Transition.cut())
        """
        ...
    def reuse(self, object: Drawable, *others: Drawable) -> None:
        """Adopt drawables into the active segment at the current timeline cursor.

        At a segment boundary, a drawable visible in the preceding segment stays
        fixed while the automatic transition runs, then becomes content of the
        active segment. Calling this after ``play()`` or ``wait()`` takes effect
        at that instant. Reusing a persistent drawable keeps it persistent while
        registering it as active segment content.

        Raises:
            ValueError: If any drawable belongs to another ``Scene``.
        """
        ...
    def persist(self, object: Drawable, *others: Drawable) -> None:
        """Keep drawables global, visible, and animatable across future segments.

        Persistence begins at the current cursor and is not retroactive. Global
        drawables are excluded from automatic ``cross_fade``, ``slide``, and
        other segment transitions. An invisible drawable remains invisible until
        an explicit entry animation changes its opacity.

        Raises:
            ValueError: If any drawable belongs to another ``Scene``.
        """
        ...
    def release(self, object: Drawable, *others: Drawable) -> None:
        """End persistence and attach drawables to the active segment.

        When called at the beginning of a segment, a persistent drawable stays
        fixed during its incoming transition and becomes local when that
        transition finishes. ``release`` never hides or removes the drawable;
        its next segment transition treats it as ordinary outgoing content.

        Raises:
            ValueError: If any drawable belongs to another ``Scene``.
        """
        ...
    def wait(self, seconds: float) -> None:
        """Schedule wait on the scene timeline.

        Example:
            scene.wait(1.0)
        """
        ...
    def stop(self, name: Optional[str] = None) -> None:
        """Pause interactive playback at the current timeline position.

        At a segment boundary, the completed outgoing segment remains visible
        until playback advances; no trailing ``wait`` is required.
        Export ignores stops and renders the timeline continuously. After a
        recorded ``live_take`` starts, a stop instead waits for as long as the
        speaker paused there while recording.
        """
        ...
    def voiceover(
        self,
        key: str,
        *,
        text: Optional[str] = None,
        volume: float = 1.0,
    ) -> Voiceover:
        """Start a voiceover block at the cursor, timed by a narration take.

        The take is ``narration/<key>.wav`` (or ``.flac``, ``.mp3``, ``.m4a``,
        ``.aac``, ``.ogg``, ``.opus``) inside the asset directory, or beside
        the script when no asset directory is set. The editor's recorder
        writes it. A recorded take plays from the cursor and sets the block's
        length; until then the length is estimated from the block's text at
        about 150 words per minute, so the scene can be edited before
        recording. The text is ``text``, else the ``## <key>`` section of the
        narration script (``narration/script.md``, loaded automatically, or
        the file given to ``narration_script``), else the active segment's
        ``notes``; it is shown as the teleprompter while recording. With a
        script loaded, a key without a section warns. Leaving the ``with``
        block waits for the rest of the take, and ``render()`` extends the
        timeline so no take is cut off.

        Raises:
            ValueError: If ``key`` is not a plain file name (letters, digits,
                ``-``, ``_``, ``.``), is already used by this scene, the
                volume is negative, or the take cannot be read.

        Example:
            with scene.voiceover("intro", text="Hoy vemos la derivada") as vo:
                scene.play(title.animate.write())
                vo.wait_until("derivada")
                scene.play(curve.animate.create())
        """
        ...
    def narration_script(self, path: str) -> None:
        """Read voiceover texts from a Markdown script edited outside the code.

        Each ``## key`` heading holds the text of ``voiceover("key")``; a
        ``# Title`` ends a section, ``###`` headings are notes that are not
        read, and HTML comments are ignored. A relative path resolves in the
        asset directory (or beside the script without one). Without this
        call, ``narration/script.md`` is loaded automatically when it exists.
        Call it before the voiceovers that use it. Saving the file reloads
        the scene in the editor.

        Raises:
            ValueError: If the file cannot be read, a ``##`` heading is not a
                valid key, or a key appears twice.

        Example:
            scene.narration_script("guion.md")
            with scene.voiceover("intro") as vo:
                vo.wait_until("derivada")
        """
        ...
    def live_take(self, key: str = "live", *, volume: float = 1.0) -> None:
        """Start the scene's live take at the cursor.

        Record it from the editor's narration panel by presenting the scene
        and talking over it: playback pauses at every later ``stop()`` until
        you advance. With ``narration/<key>.*`` recorded, the take plays from
        here and each later ``stop()`` becomes a wait as long as the pause
        you made there, so the timeline plays straight through in sync with
        your voice. Without a take, stops stay interactive.

        Raises:
            ValueError: If the scene already has a live take, ``key`` is not
                a plain file name or is used by a voiceover, or the take
                cannot be read.

        Example:
            scene.live_take("clase")
            scene.play([title.animate.write()])
            scene.stop()
        """
        ...
    @property
    def cursor(self) -> float:
        """Current authoring cursor in absolute timeline seconds.

        Example:
            scene.play([title.animate.write()])
            reveal_time = scene.cursor
        """
        ...
    @property
    def stops(self) -> list[SceneStop]:
        """Stops authored so far, in timeline order, with absolute times.

        Read it at the end of the script, before ``render()``, to capture a
        snapshot at every pause. ``gaanim --diff --example <SCRIPT>
        --capture-stops`` does the same without script changes.

        Example:
            if "GAANIM_SNAPSHOTS" in os.environ:
                scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [s.time for s in scene.stops])
        """
        ...
    def play(
        self,
        items: Playable | Sequence[Playable],
        *,
        duration: Optional[float] = None,
        easing: Optional[Easing] = None,
    ) -> None:
        """Atomically schedule a leaf, implicit parallel batch, or composition tree.

        ``duration`` and ``easing`` are defaults overridden by explicit ``Anim``
        or nested group configuration. Reused, foreign, duplicate, or temporally
        overlapping channel writes raise ``ValueError`` without partial changes.
        """
        ...
    def fade_out_all(self, seconds: float) -> None:
        """Configure or query the scene with fade out all.

        Example:
            scene.fade_out_all(1.0)
        """
        ...
    def render(self) -> None:
        """Render the scene output.

        Example:
            scene.render()
        """
        ...
    def snapshots(self, directory: str, times: Sequence[float]) -> int:
        """Ask the attached Gaanim diff host to capture exact timeline seeks.

        ``directory`` must be the path supplied in ``GAANIM_SNAPSHOTS`` by
        ``gaanim --diff``. Returns the number of captured frames and raises
        ``RuntimeError`` when no snapshot host is attached or the path differs.
        """
        ...
    # Reactive geometry helpers
    @property
    def geometry(self) -> Geometry:
        """Return the scene-owned geometry capability."""
        ...

    @property
    def text(self) -> Typography:
        """Return the scene-owned text capability."""
        ...

    @property
    def layout(self) -> LayoutBuilder:
        """Return the scene-owned layout capability."""
        ...

    @property
    def sections(self) -> SceneSections:
        """Return section navigation: ``agenda`` and ``progress_rail``."""
        ...

    @property
    def media(self) -> MediaLibrary:
        """Return the scene-owned media capability."""
        ...

    @property
    def viz(self) -> Visualization:
        """Return the scene-owned viz capability."""
        ...

    @property
    def slides(self) -> SlideKit:
        """Return the scene-owned slides capability."""
        ...

    @property
    def mechanics(self) -> Mechanics:
        """Return the scene-owned mechanics capability."""
        ...

    @property
    def assets(self) -> AssetManager:
        """Return the scene-owned assets capability."""
        ...
