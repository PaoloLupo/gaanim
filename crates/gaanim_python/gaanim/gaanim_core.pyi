"""Typed public API for Gaanim.

The examples in these stubs are intended to be copied into a small Scene
script. All camera durations are in seconds; 3D angles are in radians.
"""

from __future__ import annotations

from typing import Any, Callable, ClassVar, Literal, Mapping, Optional, Self, Sequence, TypeAlias, overload

CurvePoint: TypeAlias = tuple[float, float]
"""A coordinate pair used by :meth:`Scene.path` and :meth:`Scene.curve`."""

CurveControl: TypeAlias = CurvePoint | Literal["auto"] | None
"""A Bézier control point, an automatically reflected handle, or a collapsed handle."""

CurveCommand: TypeAlias = tuple[str, Sequence[CurvePoint | CurveControl]]
"""A ``Scene.path`` or ``Scene.curve`` command and its arguments."""

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

class StrokeStyle:
    def __init__(
        self,
        paint: Paint,
        width: float = 2.0,
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
    ) -> None:
        """Create or derive a centralized visual theme.

        Rules use family/type/part selectors or ``.classes``. Text values reuse
        the structured ``TextStyle`` overlay. Invalid selectors, tokens, roles,
        metrics, or font files raise ``ValueError`` or ``OSError``.

        Example:
            Theme()
        """
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
    """Persistent row, column, grid, or stack that owns child translation.

    Layout is itself a ``Drawable``. Positional fluent methods on managed
    children raise ``LayoutOwnershipError``; use ``configure_item`` offsets.
    """
    count: int
    def add(self, child: Drawable | Layout | LayoutItem, *, at: Optional[int] = None, animate: Optional[float] = None) -> Drawable:
        """Insert a direct child and return it; ``animate`` is seconds for reflow.

        Raises ``IndexError`` for an invalid index and ``LayoutOwnershipError``
        when the child is positioned manually, foreign, or already managed.
        """
        ...
    def remove(self, child: Drawable | Layout, *, animate: Optional[float] = None) -> None:
        """Remove a direct child and release its positional ownership."""
        ...
    def detach(self, child: Drawable | Layout, *, animate: Optional[float] = None) -> None:
        """Release a direct child from the layout without hiding it.

        The child preserves its world position, opacity, and scene membership,
        so positional methods such as ``move_to`` are valid immediately after
        this call. ``animate`` optionally reflows the remaining children in
        seconds. A non-member raises ``ValueError``.

        Example:
            scene.reuse(title)
            page.detach(title)
            scene.play([title.move_to(0.0, 200.0)])
        """
        ...
    def replace(self, old: Drawable | Layout, new: Drawable | Layout | LayoutItem, *, animate: Optional[float] = None) -> Drawable:
        """Replace a direct child, returning the replacement after optional reflow."""
        ...
    def reflow(self, *, animate: Optional[float] = None) -> None:
        """Resolve external geometry changes; ``animate`` transitions in seconds."""
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
        animate: Optional[float] = None,
    ) -> None:
        """Update container rules and queue deterministic reflow.

        Numeric geometry uses canvas units; ``aspect_ratio`` must be positive.
        ``wrap`` is valid only for rows and columns. Invalid values raise
        ``ValueError``.
        """
        ...
    def configure_item(self, child: Drawable | Layout, *, grow: Optional[float] = None, shrink: Optional[float] = None, align: Optional[Align] = None, row: Optional[int] = None, column: Optional[int] = None, row_span: Optional[int] = None, column_span: Optional[int] = None, absolute: Optional[bool] = None, anchor: Optional[Anchor] = None, offset: Optional[tuple[float, float]] = None, fit: Optional[Fit] = None, animate: Optional[float] = None) -> None:
        """Update direct-child rules and optionally animate the resulting reflow."""
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

class Anim:
    def fill(self, color: ColorLike) -> Anim:
        """Target a solid fill color in a compound ``Drawable.animate()`` animation.

        Text glyphs interpolate independently from their current fills, so
        fragment-specific colors converge to this target. On ``Primitive3D``
        this targets the PBR material base color instead.
        """
        ...
    def color(self, color: ColorLike) -> Anim:
        """Target currently visible vector paints, including every text glyph.

        Glyphs interpolate from their own current colors; any fragment-specific
        colors are replaced by the common target when the animation completes.
        On ``Primitive3D`` this targets the PBR material base color.
        """
        ...
    def stroke(self, color: ColorLike, width: float) -> Anim:
        """Target vector stroke color and width, including every text glyph.

        This is unavailable for ``Primitive3D``.
        """
        ...
    def stroke_color(self, color: ColorLike) -> Anim:
        """Target only the vector stroke color, including every text glyph."""
        ...
    def material(self, material: Material3D) -> Anim:
        """Target every animatable PBR channel of a native Primitive3D."""
        ...
    def opacity(self, value: float) -> Anim:
        """Target drawable opacity, clamped to the 0..1 range."""
        ...
    def move(self, dx: float, dy: float) -> Anim:
        """Target a relative 2D translation in a compound property animation."""
        ...
    def move_to(self, x: float, y: float, anchor: Optional[Anchor] = None) -> Anim:
        """Target an absolute 2D position, placing ``anchor`` at ``(x, y)``.

        Omitting ``anchor`` uses ``Anchor.CENTER``. The returned animation can
        be chained with other property targets.
        """
        ...
    def move_3d(self, dx: float, dy: float, dz: float) -> Anim:
        """Target a relative 3D translation in scene units."""
        ...
    def move_to_3d(self, x: float, y: float, z: float) -> Anim:
        """Target an absolute 3D position in scene units."""
        ...
    def scale(self, factor: float) -> Anim:
        """Multiply the current uniform scale by ``factor``."""
        ...
    def scale_to(self, factor: float) -> Anim:
        """Target an absolute uniform scale."""
        ...
    def scale_to_3d(self, x: float, y: float, z: float) -> Anim:
        """Target absolute scale independently on three axes."""
        ...
    def rotate(self, radians: float) -> Anim:
        """Target a relative Z rotation in radians."""
        ...
    def rotate_to(self, radians: float) -> Anim:
        """Target an absolute Z rotation in radians."""
        ...
    def rotate_by_3d(self, axis: Literal["x", "y", "z"], radians: float) -> Anim:
        """Target a relative rotation around one 3D axis."""
        ...
    def rotate_to_3d(self, x: float, y: float, z: float) -> Anim:
        """Target an absolute XYZ Euler orientation in radians."""
        ...
    def duration(self, d: float) -> Anim:
        """Configure this animation with duration.

        Example:
            result = animation.duration(1.0)
        """
        ...
    def ease(self, name: str) -> Anim:
        """Configure this animation with ease.

        Example:
            result = animation.ease("example")
        """
        ...
    def rate(self, name: str) -> Anim:
        """Configure this animation with rate.

        Example:
            result = animation.rate("example")
        """
        ...
    def delay(self, sec: float) -> Anim:
        """Configure this animation with delay.

        Example:
            result = animation.delay(1.0)
        """
        ...
    def steps(self, n: int) -> Anim:
        """Configure this animation with steps.

        Example:
            result = animation.steps(1)
        """
        ...
    def spring(self) -> Anim:
        """Configure this animation with spring.

        Example:
            result = animation.spring()
        """
        ...
    def smooth(self) -> Anim:
        """Configure this animation with smooth.

        Example:
            result = animation.smooth()
        """
        ...
    def linear(self) -> Anim:
        """Configure this animation with linear.

        Example:
            result = animation.linear()
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
    def animate(self) -> Anim:
        """Start a typed compound property animation.

        Chain transform, opacity, fill, stroke, color, or material targets and
        pass the result to ``scene.play``. All selected channels share timing
        and easing and run concurrently.

        Example:
            scene.play([drawable.animate().move_to(120, 0).fill(BLUE).duration(1.5)])
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
        """Apply stroke to this drawable and return the result.

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
    def glow(self, color: Color, radius: float = 16.0, intensity: float = 1.0) -> Drawable:
        """Apply glow to this drawable and return the result.

        Example:
            result = drawable.glow(BLUE)
        """
        ...
    def blur(self, sigma: float = 4.0) -> Drawable:
        """Apply blur to this drawable and return the result.

        Example:
            result = drawable.blur()
        """
        ...
    def shadow(
        self,
        color: Color,
        x: float = 8.0,
        y: float = -8.0,
        blur: float = 6.0,
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
    ) -> Drawable:
        """Apply clip to this drawable and return the result.

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
    def opacity(self, op: float) -> Self:
        """Apply opacity to this drawable and return the result.

        Example:
            result = drawable.opacity(1.0)
        """
        ...
    def z_index(self, z: int) -> Self:
        """Apply z index to this drawable and return the result.

        Example:
            result = drawable.z_index(1)
        """
        ...
    def at(self, x: float, y: float, anchor: Optional[Anchor] = None) -> Self:
        """Place ``anchor`` at ``(x, y)`` and return this drawable.

        Omitting ``anchor`` uses ``Anchor.CENTER`` and preserves the existing
        center-based behavior. The optional anchor can be passed positionally
        or by keyword.

        Example:
            result = drawable.at(1.0, 1.0, Anchor.TOP_LEFT)
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
    def at_coordinate(self, coordinate: CoordinateRef) -> Drawable:
        """Place this drawable at a symbolic coordinate owned by a coordinate space."""
        ...
    def at_3d(self, x: float, y: float, z: float) -> Self:
        """Place the drawable at a 3D world-space position.

        Coordinates are interpreted by the perspective camera. The method is
        chainable and returns the same ``Drawable``.

        Example:
            dot = scene.dot(8).fill(RED).at_3d(1.0, 2.0, 0.5)
        """
        ...
    def billboard(self) -> Self:
        """Keep a 3D drawable facing the perspective camera.

        This is useful for labels and markers attached to a 3D scene. The
        method is chainable and returns the same ``Drawable``.

        Example:
            label = scene.text("origin").at_3d(0.0, 1.0, 0.0).billboard()
        """
        ...
    def hud(self) -> Self:
        """Pin the drawable to the screen as a fixed HUD overlay.

        HUD drawables use screen-space coordinates and are not affected by
        the 3D camera. Use ``.at(x, y)`` after ``.hud()`` to position them in
        the viewport. The method is chainable and returns the same
        ``Drawable``.

        Example:
            title = scene.text("glTF demo").hud().at(0.0, 300.0)
        """
        ...
    def scaled(self, factor: float) -> Self:
        """Apply scaled to this drawable and return the result.

        Example:
            result = drawable.scaled(1.0)
        """
        ...
    def scaled_3d(self, x: float, y: float, z: float) -> Self:
        """Scale independently on three axes and preserve the specialized handle.

        Example:
            label = scene.text("depth").scaled_3d(1.0, 1.0, 0.5)
        """
        ...
    def rotated(self, radians: float) -> Self:
        """Apply rotated to this drawable and return the result.

        Example:
            result = drawable.rotated(1.0)
        """
        ...
    def rotated_3d(self, x: float, y: float, z: float) -> Self:
        """Apply Euler rotation in radians and preserve the specialized handle.

        Example:
            label = scene.text("axis").rotated_3d(0.0, 0.0, 0.5)
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
    def at_anchor(self, x: float, y: float, anchor: Anchor) -> Self:
        """Apply at anchor to this drawable and return the result.

        Example:
            result = drawable.at_anchor(1.0, 1.0, Anchor.CENTER)
        """
        ...
    def next_to(
        self,
        reference: Drawable,
        direction: Direction,
        spacing: float = 24.0,
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
    def to_edge(self, direction: Direction, buff: float = 24.0) -> Self:
        """Apply to edge to this drawable and return the result.

        Example:
            result = drawable.to_edge(Direction.RIGHT)
        """
        ...
    def to_corner(self, corner: Anchor, buff: float = 24.0) -> Self:
        """Apply to corner to this drawable and return the result.

        Example:
            result = drawable.to_corner(Anchor.CENTER)
        """
        ...
    def move(self, dx: float, dy: float) -> Anim:
        """Create a move animation for this drawable.

        Example:
            result = drawable.move(1.0, 1.0)
        """
        ...
    def move_to(self, x: float, y: float, anchor: Optional[Anchor] = None) -> Anim:
        """Create a move to animation for this drawable.

        The selected ``anchor`` arrives at ``(x, y)``; omitting it uses
        ``Anchor.CENTER``.

        Example:
            result = drawable.move_to(1.0, 1.0)
        """
        ...
    def move_3d(self, dx: float, dy: float, dz: float) -> Anim: ...
    def move_to_3d(self, x: float, y: float, z: float) -> Anim: ...
    def glide_to(self, x: float, y: float) -> Anim:
        """Create a glide to animation for this drawable.

        Example:
            result = drawable.glide_to(1.0, 1.0)
        """
        ...
    def scale(self, factor: float) -> Anim:
        """Create a scale animation for this drawable.

        Example:
            result = drawable.scale(1.0)
        """
        ...
    def scale_to_3d(self, x: float, y: float, z: float) -> Anim: ...
    def rotate(self, rad: float) -> Anim:
        """Create a rotate animation for this drawable.

        Example:
            result = drawable.rotate(1.0)
        """
        ...
    def rotate_by_3d(self, axis: Literal["x", "y", "z"], radians: float) -> Anim: ...
    def rotate_to_3d(self, x: float, y: float, z: float) -> Anim: ...
    def fade_in(self, duration: Optional[float] = None) -> Anim:
        """Create a fade in animation for this drawable.

        Use this animation in ``scene.play(...)`` to reveal a generated
        reactive visual.

        Example:
            result = drawable.fade_in()
        """
        ...
    def fade_in_from(
        self,
        direction: Direction,
        distance: float = 48.0,
        duration: Optional[float] = None,
    ) -> Anim:
        """Create a fade in from animation for this drawable.

        Example:
            result = drawable.fade_in_from(Direction.RIGHT)
        """
        ...
    def fade_out(self, duration: Optional[float] = None) -> Anim:
        """Create a fade out animation for this drawable.

        Example:
            result = drawable.fade_out()
        """
        ...
    def fade_to(self, alpha: float) -> Anim:
        """Create a fade to animation for this drawable.

        Example:
            result = drawable.fade_to(1.0)
        """
        ...
    def write(self, duration: Optional[float] = None) -> Anim:
        """Create a write animation for this drawable.

        Example:
            result = drawable.write()
        """
        ...
    def create(self, duration: Optional[float] = None) -> Anim:
        """Create a create animation for this drawable.

        Example:
            result = drawable.create()
        """
        ...
    def unwrite(self, duration: Optional[float] = None) -> Anim:
        """Create a unwrite animation for this drawable.

        Example:
            result = drawable.unwrite()
        """
        ...
    def uncreate(self, duration: Optional[float] = None) -> Anim:
        """Create a uncreate animation for this drawable.

        Example:
            result = drawable.uncreate()
        """
        ...
    def grow_from_center(self, duration: Optional[float] = None) -> Anim:
        """Create a grow from center animation for this drawable.

        Example:
            result = drawable.grow_from_center()
        """
        ...
    def shrink_to_center(self, duration: Optional[float] = None) -> Anim:
        """Create a shrink to center animation for this drawable.

        Example:
            result = drawable.shrink_to_center()
        """
        ...
    def spin_in_from_nothing(self, duration: Optional[float] = None) -> Anim:
        """Create a spin in from nothing animation for this drawable.

        Example:
            result = drawable.spin_in_from_nothing()
        """
        ...
    def draw_border_then_fill(self, duration: Optional[float] = None) -> Anim:
        """Create a draw border then fill animation for this drawable.

        Example:
            result = drawable.draw_border_then_fill()
        """
        ...
    def indicate(self, duration: Optional[float] = None) -> Anim:
        """Create a subtle upward hop around the drawable's visual center.

        Example:
            result = drawable.indicate()
        """
        ...
    def wiggle(self, duration: Optional[float] = None) -> Anim:
        """Create a wiggle animation for this drawable.

        Example:
            result = drawable.wiggle()
        """
        ...
    def move_along_path(self, target: Drawable) -> Anim:
        """Create a move along path animation for this drawable.

        Example:
            result = drawable.move_along_path(target)
        """
        ...
    def fade_transform(self, target: Drawable) -> Anim:
        """Create a fade transform animation for this drawable.

        Example:
            result = drawable.fade_transform(target)
        """
        ...
    def transform(self, target: Drawable) -> Anim:
        """Create a transform animation for this drawable.

        Example:
            result = drawable.transform(target)
        """
        ...
    def replacement_transform(self, target: Drawable) -> Anim:
        """Create a replacement transform animation for this drawable.

        Example:
            result = drawable.replacement_transform(target)
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
    def _legacy_coords_to_point(self, x: float, y: float) -> tuple[float, float]:
        """Use coords to point on this Drawable or create the requested value.

        Example:
            result = axes.coords_to_point(1.0, 1.0)
        """
        ...
    def _legacy_point_to_coords(self, point: tuple[float, float]) -> tuple[float, float]:
        """Use point to coords on this Drawable or create the requested value.

        Example:
            result = axes.point_to_coords((0.0, 0.0))
        """
        ...
    def _legacy_get_x_axis(self) -> Drawable:
        """Apply get x axis to this drawable and return the result.

        Example:
            result = axes.get_x_axis()
        """
        ...
    def _legacy_get_y_axis(self) -> Drawable:
        """Apply get y axis to this drawable and return the result.

        Example:
            result = axes.get_y_axis()
        """
        ...
    def _legacy_get_axes(self) -> Drawable:
        """Apply get axes to this drawable and return the result.

        Example:
            result = axes.get_axes()
        """
        ...
    def _legacy_add_coordinates(self) -> Drawable:
        """Apply add coordinates to this drawable and return the result.

        Example:
            result = axes.add_coordinates()
        """
        ...

TextRole: TypeAlias = Literal["title", "subtitle", "heading", "body", "caption", "label", "code", "math"]
TextWrap: TypeAlias = Literal["auto", False] | float
TextAlign: TypeAlias = Literal["left", "center", "right", "justify"]
TextOverflow: TypeAlias = Literal["visible", "clip", "ellipsis"]
TextDirection: TypeAlias = Literal["auto", "ltr", "rtl"]
TextGrouping: TypeAlias = Literal["grapheme", "word", "line", "part"]

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
ScalarSource: TypeAlias = float | Parameter | Variable | _Expr
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
    def material_to(self, material: Material3D) -> Anim: ...
    def write(self, duration: Optional[float] = None) -> Anim:
        """Unsupported for meshes; use :meth:`Drawable.create`."""
        ...

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

        Sizes and spacing use canvas/Typst points. Invalid non-positive sizes
        raise ``ValueError``. Outer width, height, padding, fit, and growth are
        intentionally controlled by Layout v2.

        Example:
            body = TextStyle(font="Inter", size=32, color=WHITE)
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
    ) -> None:
        """Configure wrapping and line composition inside a measured Text leaf.

        ``"auto"`` consumes the width offered by Layout v2 or the safe frame;
        ``False`` keeps one line except for explicit newlines; a number caps the
        typographic width. Invalid widths, spacing, or line counts raise
        ``ValueError``.

        Example:
            flow = TextFlow(wrap="auto", align="justify", line_spacing=1.25)
        """
        ...

class TextPart:
    """Immutable named subtree created by :func:`part`."""

class TextParts:
    """Immutable ordered group of plain named parts created by :func:`parts`."""

TextContent: TypeAlias = str | TextPart | TextParts

def parts(**content: str) -> TextParts:
    """Build an ordered group of plain semantic text parts.

    The keyword order is preserved. Inside ``$...$`` math, adjacent parts are
    separated as distinct Typst tokens while retaining Typst's native tight
    spacing. Use explicit ``part`` values when local styling or nested content
    is needed. Calling ``parts()`` without entries, using an
    empty name, or producing wholly empty content raises ``ValueError``; a
    non-string value raises ``TypeError``.

    Example:
        terms = parts(mass="m", gravity="g sin(theta)")
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
    def animate(self) -> Anim:
        """Start a compound animation scoped to the selected glyphs.

        Only ``fill``/``color`` and ``opacity`` targets are supported; other
        property channels raise ``TypeError``.

        Example:
            scene.play([formula["mass"].animate().fill(RED).opacity(0.6)])
        """
        ...
    def color_to(self, color: Color, duration: Optional[float] = None) -> Anim:
        """Animate only the selected glyph fills to ``color``.

        Example:
            scene.play([formula["mass"].color_to(RED, duration=0.6)])
        """
        ...
    def opacity_to(self, opacity: float, duration: Optional[float] = None) -> Anim:
        """Animate selected glyph opacity to a finite value within 0..1.

        Example:
            scene.play([formula["mass"].opacity_to(0.5, duration=0.6)])
        """
        ...
    def indicate(self, duration: Optional[float] = None) -> Anim:
        """Emphasize selected glyphs without changing their measured size.

        Example:
            scene.play([copy["concept"].indicate()])
        """
        ...
    def pulse(self, duration: Optional[float] = None) -> Anim:
        """Pulse selected glyphs without causing layout reflow.

        Example:
            scene.play([copy.words[1].pulse()])
        """
        ...
    def wiggle(self, duration: Optional[float] = None) -> Anim:
        """Wiggle selected glyphs without changing intrinsic measurement.

        Example:
            scene.play([copy["warning"].wiggle()])
        """
        ...
    def wave(self, duration: Optional[float] = None) -> Anim:
        """Apply a wave emphasis to the selected glyph sequence.

        Example:
            scene.play([copy.words[0:3].wave()])
        """
        ...
    def highlight(self, duration: Optional[float] = None) -> Anim:
        """Highlight the selected glyph sequence.

        Example:
            scene.play([copy.lines[0].highlight()])
        """
        ...
    def focus(self, duration: Optional[float] = None) -> Anim:
        """Focus attention on this selection without making it a Layout leaf.

        Example:
            scene.play([formula["mass"].focus()])
        """
        ...
    def cancel(self, duration: Optional[float] = None) -> Anim:
        """Strike through and dim the selected glyphs.

        The mark remains with the owning ``Text`` until its next replacing
        ``morph_to()``, ``step_to()`` or ``expand_to()`` transition, where both
        the mark and canceled glyphs fade out. Returns an ``Anim`` for
        ``scene.play()``; ``duration=None`` uses the animation default.

        Example:
            scene.play([formula["obsolete"].cancel(duration=0.6)])
            scene.play([formula.step_to(simplified, duration=0.8)])
        """
        ...
    def reveal(self, *, style: Literal["fade", "wipe", "from_below"] = "fade", duration: Optional[float] = None) -> Anim:
        """Reveal only this selection using a deterministic fragment preset.

        Example:
            scene.play([formula["answer"].reveal(style="wipe")])
        """
        ...
    def morph_to(self, target: TextSelection, *, duration: Optional[float] = None) -> Anim:
        """Morph this selection into another selection.

        Example:
            scene.play([source["term"].morph_to(target["term"])])
        """
        ...
    def copy_to(self, target: TextSelection, *, duration: Optional[float] = None) -> Anim:
        """Copy this selection toward another selection while preserving its source.

        Example:
            scene.play([source["term"].copy_to(target["term"])])
        """
        ...
    def brace(self, label: str, *, above: bool = False, duration: Optional[float] = None) -> Anim:
        """Attach an animated brace and label to this selection.

        Example:
            scene.play([formula["mass"].brace("mass")])
        """
        ...
    def annotate(self, label: str, *, offset: tuple[float, float] = (120.0, 80.0), duration: Optional[float] = None) -> Anim:
        """Attach an animated annotation at a local canvas-unit offset.

        Example:
            scene.play([formula["mass"].annotate("converted energy", offset=(100, 60))])
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

class Text(Drawable):
    """Structured, Layout-v2-measurable vector text and mathematics."""
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
    def write(self, duration: Optional[float] = None, *, by: TextGrouping = "grapheme", order: Literal["forward", "reverse", "center", "random"] = "forward", stagger: float = 0.0) -> Anim:
        """Write text over an optional positional duration, grouped as requested.

        Example:
            scene.play([copy.write(0.8, by="word", stagger=0.06)])
        """
        ...
    def type_in(self, duration: Optional[float] = None, *, by: TextGrouping = "grapheme", order: Literal["forward", "reverse", "center", "random"] = "forward", stagger: float = 0.04) -> Anim:
        """Type text over an optional positional duration with deterministic grouping.

        Example:
            scene.play([copy.type_in(1.0, by="word")])
        """
        ...
    def reveal(self, duration: Optional[float] = None, *, by: TextGrouping = "grapheme", order: Literal["forward", "reverse", "center", "random"] = "forward", stagger: float = 0.0) -> Anim:
        """Reveal structured text over an optional positional duration.

        Example:
            scene.play([copy.reveal(0.7, by="line")])
        """
        ...
    def fade_in(self, duration: Optional[float] = None) -> Anim:
        """Fade the complete Text into the scene.

        Example:
            scene.play([copy.fade_in()])
        """
        ...
    def slide_in(self, direction: Literal["up", "down", "left", "right"] = "up", *, distance: float = 24.0, duration: Optional[float] = None) -> Anim:
        """Fade and slide Text from a direction by a canvas-unit distance.

        Example:
            scene.play([copy.slide_in("up")])
        """
        ...
    def unwrite(self, duration: Optional[float] = None) -> Anim:
        """Remove Text by reversing its writing animation.

        Example:
            scene.play([copy.unwrite()])
        """
        ...
    def erase(self, duration: Optional[float] = None) -> Anim:
        """Erase Text using its vector writing order.

        Example:
            scene.play([copy.erase()])
        """
        ...
    def fade_out(self, duration: Optional[float] = None) -> Anim:
        """Fade the complete Text out of the scene.

        Example:
            scene.play([copy.fade_out()])
        """
        ...
    def slide_out(self, direction: Literal["up", "down", "left", "right"] = "down", *, distance: float = 24.0, duration: Optional[float] = None) -> Anim:
        """Remove Text with a directional slide/fade exit preset.

        Example:
            scene.play([copy.slide_out("down")])
        """
        ...
    def indicate(self, duration: Optional[float] = None) -> Anim:
        """Indicate Text transiently without causing reflow.

        Example:
            scene.play([copy.indicate()])
        """
        ...
    def pulse(self, duration: Optional[float] = None) -> Anim:
        """Pulse Text transiently without changing its measurement.

        Example:
            scene.play([copy.pulse()])
        """
        ...
    def wiggle(self, duration: Optional[float] = None) -> Anim:
        """Wiggle Text transiently without changing Layout geometry.

        Example:
            scene.play([copy.wiggle()])
        """
        ...
    def wave(self, duration: Optional[float] = None) -> Anim:
        """Apply a wave emphasis to the complete Text.

        Example:
            scene.play([copy.wave()])
        """
        ...
    def highlight(self, duration: Optional[float] = None) -> Anim:
        """Circumscribe the complete Text as a highlight.

        Example:
            scene.play([copy.highlight()])
        """
        ...
    def focus(self, duration: Optional[float] = None) -> Anim:
        """Focus the complete Text transiently.

        Example:
            scene.play([copy.focus()])
        """
        ...
    def cancel(self, duration: Optional[float] = None) -> Anim:
        """Strike through and dim the complete Text.

        The mark is retired by the next replacing text transition. Returns an
        ``Anim`` for ``scene.play()``; ``duration=None`` uses the animation
        default.

        Example:
            scene.play([copy.cancel(duration=0.6)])
        """
        ...
    def brace(self, label: str, *, above: bool = False, duration: Optional[float] = None) -> Anim:
        """Attach a brace and label to the complete Text.

        Example:
            scene.play([formula.brace("identity", above=True)])
        """
        ...
    def annotate(self, label: str, *, offset: tuple[float, float] = (120.0, 80.0), duration: Optional[float] = None) -> Anim:
        """Attach an animated annotation to the complete Text.

        Example:
            scene.play([formula.annotate("important result")])
        """
        ...
    def morph_to(self, target: Text, *, match: Literal["auto", "semantic", "grapheme", "shape"] = "auto", duration: float = 1.0) -> Anim:
        """Morph into another Text, matching semantic paths before glyphs and shapes.

        Foreign scenes or incompatible Layout owners raise
        ``LayoutOwnershipError``.

        Example:
            scene.play([source.morph_to(target, match="auto")])
        """
        ...
    def step_to(self, target: Text, *, matches: Optional[Mapping[str, str] | Sequence[tuple[str, str]]] = None, duration: float = 1.0) -> Anim:
        """Advance a structured derivation, replacing ``Scene.step_equation``.

        Example:
            scene.play([first.step_to(second, matches={"left": "result"})])
        """
        ...
    def expand_to(self, target: Text, *, anchor: str = "part", duration: float = 1.0) -> Anim:
        """Expand toward another Text around a shared semantic part.

        Example:
            scene.play([short.expand_to(long, anchor="formula.mass")])
        """
        ...
    def become(self, *content: TextContent, role: Optional[TextRole] = None, style: Optional[TextStyle] = None, flow: Optional[TextFlow] = None, duration: float = 1.0) -> None:
        """Replace structured content while retaining Text identity and reflowing owners.

        The text version and all owning Layout snapshots are incremented. An
        invalid delimiter or content tree raises ``ValueError``.

        Example:
            copy.become("Resultado: ", part("value", "$42$", color=GOLD))
        """
        ...

class Canvas:
    """Visual viewport configuration owned by a Scene."""
    width: int
    height: int
    background: Optional[Color]
    theme: Optional[str]
    def set_theme(self, theme: str | Theme) -> None:
        """Apply a built-in color scheme or a custom Theme."""
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
        """Configure the canvas with set preset.

        Example:
            scene.canvas.set_preset(None)
        """
        ...
class Segment:
    def bind(self, **slots: Any) -> Layout:
        """Bind this segment's template slots and return its root Layout.

        Missing or extra slots raise ``TypeError``; a segment without a
        template raises ``ValueError``.
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
    @overload
    def pan_to(self, x: float, y: float, duration: float = 1.0) -> Anim:
        """Configure the camera with pan to.

        Example:
            scene.camera.pan_to(1.0, 1.0)
        """
        ...
    @overload
    def pan_to(self, target: Endpoint, duration: float = 1.0) -> Anim: ...
    def zoom_to(self, zoom: ScalarSource, duration: float = 1.0) -> Anim:
        """Configure the camera with zoom to.

        Example:
            scene.camera.zoom_to(1.0)
        """
        ...
    def frame_to(
        self,
        targets: Drawable | Sequence[Drawable],
        margin: float | tuple[float, float] | tuple[float, float, float, float] | None = None,
        duration: float = 1.0,
        *,
        dynamic: bool = False,
    ) -> Anim:
        """Configure the camera with frame to.

        Example:
            scene.camera.frame_to(target)
        """
        ...
    def rotate_to(self, angle: ScalarSource, duration: float = 1.0) -> Anim:
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
        duration: float = 1.0,
    ) -> Anim:
        """Configure the camera with follow.

        Example:
            scene.camera.follow(target)
        """
        ...
    def shake(
        self,
        amplitude: float = 12.0,
        frequency: float = 8.0,
        duration: float = 0.5,
    ) -> Anim:
        """Configure the camera with shake.

        Example:
            scene.camera.shake()
        """
        ...
    def look_at(
        self,
        eye: Endpoint,
        target: Endpoint,
        up: Optional[tuple[float, float, float]] = None,
        duration: float = 1.0,
    ) -> Anim:
        """Aim the camera from ``eye`` toward ``target``.

        Args:
            eye: Camera position in world coordinates.
            target: World-space point to look at.
            up: World-up direction. Defaults to ``(0, 1, 0)``.
            duration: Transition duration in seconds. Use ``0`` for setup.

        Example:
            scene.camera.look_at((7, 5, 6), (0, 0, 0), duration=0.0)
        """
        ...

    def orbit(
        self,
        delta_yaw: float,
        delta_pitch: float,
        duration: float = 1.0,
    ) -> Anim:
        """Orbit around the current look-at target.

        ``delta_yaw`` and ``delta_pitch`` are radians. Positive duration
        animates the orbit on the scene timeline.

        Example:
            scene.camera.orbit(delta_yaw=0.6, delta_pitch=0.12, duration=1.0)
        """
        ...

    def perspective(
        self,
        fov_y: float,
        near: float = 0.1,
        far: float = 1000.0,
        duration: float = 1.0,
    ) -> Anim:
        """Use perspective projection with a vertical field of view.

        ``fov_y`` is in radians and must be positive. ``near`` and ``far``
        are positive clipping distances with ``near < far``.

        Example:
            scene.camera.perspective(0.785, near=0.1, far=1000.0, duration=0.0)
        """
        ...

    def dolly(self, factor: float, duration: float = 1.0) -> Anim:
        """Move toward or away from the current target.

        A factor below ``1`` moves closer; a factor above ``1`` moves farther.
        The factor must be finite and positive.

        Example:
            scene.camera.dolly(factor=0.85, duration=0.6)
        """
        ...

    def orthographic(self, zoom: float = 1.0, duration: float = 0.0) -> Anim:
        """Select orthographic projection; ``zoom`` must be positive."""
        ...

    def reset(self, duration: float = 1.0) -> Anim:
        """Restore the default 2D pose, up vector, target, and projection."""
        ...

    def bind_2d(
        self,
        *,
        center: Optional[Endpoint] = None,
        zoom: Optional[ScalarSource] = None,
        rotation: Optional[ScalarSource] = None,
        influence: Optional[ScalarSource] = None,
        enabled: bool = True,
    ) -> CameraConstraint:
        """Bind orthographic channels to native reactive sources.

        ``rotation`` is in radians. ``zoom`` must evaluate to a finite positive
        value and ``influence`` defaults to ``1`` (``None`` at runtime means
        the default). At least one channel is required.
        """
        ...

    def bind_3d(
        self,
        *,
        eye: Optional[Endpoint] = None,
        target: Optional[Endpoint] = None,
        fov_y: Optional[ScalarSource] = None,
        up: tuple[float, float, float] = (0.0, 1.0, 0.0),
        influence: Optional[ScalarSource] = None,
        enabled: bool = True,
    ) -> CameraConstraint:
        """Bind perspective pose or FOV channels to native reactive sources.

        ``fov_y`` is a vertical field of view in radians. ``up`` must be
        finite and non-zero. At least one of ``eye``, ``target``, or ``fov_y``
        is required.
        """
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
    ) -> Axis: ...
    def label(self, text: str) -> Axis: ...
    def crossing(self, value: Literal["auto", "zero", "min", "max", "minimum", "maximum"] | float) -> Axis: ...
    def style(
        self,
        *,
        color: Optional[ColorLike] = None,
        width: Optional[float] = None,
        tick_length: Optional[float] = None,
        tick_width: Optional[float] = None,
        number_color: Optional[ColorLike] = None,
        label_color: Optional[ColorLike] = None,
    ) -> Axis: ...
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
    def colors(self, colors: Sequence[Color]) -> Scale:
        """Return a copy with an explicit ordered color range."""
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
        """Create a discrete legend guide."""
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
        """Return a copy using the requested mark and mark-specific options."""
        ...
    def encode(self, *, x: Optional[EncodingLike] = None, y: Optional[EncodingLike] = None, z: Optional[EncodingLike] = None, color: Optional[EncodingLike] = None, size: Optional[EncodingLike] = None, opacity: Optional[EncodingLike] = None, label: Optional[EncodingLike] = None) -> ChartSpec:
        """Return a copy with validated positional and visual channel encodings."""
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

class _Expr:
    """Internal traced scalar returned by reactive arithmetic."""
    def __init__(self, value: float) -> None: ...
    def __neg__(self) -> _Expr: ...
    def __add__(self, other: object) -> _Expr: ...
    def __radd__(self, other: object) -> _Expr: ...
    def __sub__(self, other: object) -> _Expr: ...
    def __rsub__(self, other: object) -> _Expr: ...
    def __mul__(self, other: object) -> _Expr: ...
    def __rmul__(self, other: object) -> _Expr: ...
    def __truediv__(self, other: object) -> _Expr: ...
    def __rtruediv__(self, other: object) -> _Expr: ...
    def __pow__(self, other: object, modulo: object = None) -> _Expr: ...
    def __rpow__(self, other: object, modulo: object = None) -> _Expr: ...
    def __abs__(self) -> _Expr: ...
    def sin(self) -> _Expr: ...
    def cos(self) -> _Expr: ...
    def tan(self) -> _Expr: ...
    def exp(self) -> _Expr: ...
    def log(self) -> _Expr: ...
    def sqrt(self) -> _Expr: ...
    def abs(self) -> _Expr: ...
    def pow(self, exponent: object) -> _Expr: ...
    def min(self, other: object) -> _Expr: ...
    def max(self, other: object) -> _Expr: ...
    def clamp(self, minimum: object, maximum: object) -> _Expr: ...
    def if_positive(self, when_true: object, when_false: object) -> _Expr: ...

class Parameter:
    """An animatable scalar usable directly in traced ``gaanim.math`` expressions."""
    @property
    def current(self) -> float: ...
    def set(self, value: float) -> None: ...
    def animate_to(self, value: float, duration: Optional[float] = None) -> Anim: ...
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
    def remove_updater(self) -> None: ...
    def __neg__(self) -> _Expr: ...
    def __add__(self, other: object) -> _Expr: ...
    def __radd__(self, other: object) -> _Expr: ...
    def __sub__(self, other: object) -> _Expr: ...
    def __rsub__(self, other: object) -> _Expr: ...
    def __mul__(self, other: object) -> _Expr: ...
    def __rmul__(self, other: object) -> _Expr: ...
    def __truediv__(self, other: object) -> _Expr: ...
    def __rtruediv__(self, other: object) -> _Expr: ...
    def __pow__(self, other: object, modulo: object = None) -> _Expr: ...
    def __rpow__(self, other: object, modulo: object = None) -> _Expr: ...
    def __abs__(self) -> _Expr: ...

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

class Variable(Drawable):
    """A visible ``Parameter`` with an equation-aligned reactive readout group."""
    @property
    def current(self) -> float: ...
    def set(self, value: float) -> None: ...
    def animate_to(self, value: float, duration: Optional[float] = None) -> Anim: ...
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
    def __neg__(self) -> _Expr: ...
    def __add__(self, other: object) -> _Expr: ...
    def __radd__(self, other: object) -> _Expr: ...
    def __sub__(self, other: object) -> _Expr: ...
    def __rsub__(self, other: object) -> _Expr: ...
    def __mul__(self, other: object) -> _Expr: ...
    def __rmul__(self, other: object) -> _Expr: ...
    def __truediv__(self, other: object) -> _Expr: ...
    def __rtruediv__(self, other: object) -> _Expr: ...
    def __pow__(self, other: object, modulo: object = None) -> _Expr: ...
    def __rpow__(self, other: object, modulo: object = None) -> _Expr: ...
    def __abs__(self) -> _Expr: ...

_TracedScalar: TypeAlias = float | Parameter | Variable | _Expr

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

class Chart:
    """Materialized chart with stable marks, axes, grid, and guide layers."""
    def drawable(self) -> Drawable:
        """Return the root drawable used by layout and generic transforms."""
        ...
    def layer(self, name: Literal["marks", "axes", "grid", "guides"]) -> Drawable:
        """Return one semantic layer without exposing its internal batches."""
        ...
    def at(self, x: float, y: float) -> Chart:
        """Return a chart handle translated in the 2D canvas plane."""
        ...
    def at_3d(self, x: float, y: float, z: float) -> Chart:
        """Return a chart handle translated in world coordinates."""
        ...
    def scaled(self, factor: float) -> Chart:
        """Return a uniformly scaled chart handle."""
        ...
    def create(self, duration: Optional[float] = None) -> Anim:
        """Animate the chart root with the native create transition."""
        ...
    def write(self, duration: Optional[float] = None) -> Anim:
        """Animate the chart root with the native write transition."""
        ...
    def fade_in(self, duration: Optional[float] = None) -> Anim:
        """Fade all vector and native 3D mesh layers of the chart into the scene."""
        ...
    def fade_out(self, duration: Optional[float] = None) -> Anim:
        """Fade all vector and native 3D mesh layers of the chart out of the scene."""
        ...
    def to(self, target: ChartSpec, *, match_: Literal["key", "index"] = "key", fallback: Literal["error", "crossfade"] = "error") -> Anim:
        """Build a deterministic transition after validating identity and morph compatibility."""
        ...
    def inspect(self, fields: Sequence[str], *, format: Optional[str] = None) -> Chart:
        """Enable preview-only inspection metadata for the selected data fields."""
        ...
    @property
    def inspection_enabled(self) -> bool:
        """Report whether preview inspection was enabled for this handle."""
        ...

class CoordinateSpace:
    def drawable(self) -> Drawable: ...
    def at(self, x: float, y: float) -> CoordinateSpace: ...
    def scaled(self, factor: float) -> CoordinateSpace: ...
    def rotated(self, radians: float) -> CoordinateSpace: ...
    def create(self, duration: Optional[float] = None) -> Anim: ...
    def fade_in(self, duration: Optional[float] = None) -> Anim: ...
    def fade_out(self, duration: Optional[float] = None) -> Anim: ...
    def animate_view(self, x_domain: tuple[float, float], y_domain: tuple[float, float], *, duration: float = 1.0) -> list[Anim]: ...
    def coord(self, x: float, y: float) -> CoordinateRef: ...
    def data_to_local(self, x: float, y: float) -> tuple[float, float]: ...
    def local_to_data(self, x: float, y: float) -> tuple[float, float]: ...
    def layer(self, name: Literal["grid", "major_grid", "minor_grid", "axis", "axes", "ticks", "numbers", "labels"]) -> Drawable: ...
    def plot(
        self,
        function: Callable[[float], _TracedScalar],
        domain: Optional[tuple[float, float]] = None,
        *,
        samples: Optional[int] = None,
        tolerance: float = 0.75,
        derivative: int = 0,
    ) -> Drawable: ...
    def function(
        self,
        function: Callable[[float], _TracedScalar],
        domain: Optional[tuple[float, float]] = None,
        *,
        samples: Optional[int] = None,
        tolerance: float = 0.75,
        derivative: int = 0,
    ) -> Drawable:
        """Sample a scalar function in this space; reactive expressions remain native."""
        ...
    def parametric(self, function: Callable[[float], tuple[float, float]], domain: tuple[float, float], *, samples: Optional[int] = None, tolerance: float = 0.75) -> Drawable: ...
    def implicit(self, function: Callable[[float, float], float], *, resolution: tuple[int, int] = (96, 64)) -> Drawable: ...
    def contour(self, function: Callable[[float, float], float], levels: Sequence[float], *, resolution: tuple[int, int] = (96, 64)) -> Drawable: ...
    def vector_field(self, function: Callable[[float, float], tuple[float, float]], *, resolution: tuple[int, int] = (20, 12), max_length: float = 28.0) -> Drawable: ...
    def projections(self, x: float, y: float) -> Drawable: ...
    def secant(self, function: Callable[[float], float], x0: float, x1: float) -> Drawable: ...
    def tangent(self, function: Callable[[float], float], x: float, *, length: Optional[float] = None, dx: Optional[float] = None) -> Drawable: ...
    def normal(self, function: Callable[[float], float], x: float, *, length: Optional[float] = None, dx: Optional[float] = None) -> Drawable: ...
    def area_under(self, function: Callable[[float], float], domain: tuple[float, float], *, samples: int = 160, baseline: float = 0.0) -> Drawable: ...
    def riemann_sum(self, function: Callable[[float], float], domain: tuple[float, float], *, rectangles: int = 12, method: Literal["left", "midpoint", "middle", "right"] = "midpoint", baseline: float = 0.0) -> Drawable: ...

class NumberLine:
    """A one-dimensional typed coordinate space with scale-aware labels and reactive points."""
    def drawable(self) -> Drawable: ...
    def coord(self, value: float) -> CoordinateRef: ...
    def data_to_local(self, value: float) -> float: ...
    def point_ref(
        self,
        value: _TracedScalar,
        *,
        normal_offset: Optional[_TracedScalar] = None,
    ) -> PointRef:
        """Map a scalar into a reactive point in the line's local frame.

        ``normal_offset`` is measured perpendicular to the line in local canvas
        units; ``None`` means zero. Continuous scales are supported. A
        categorical axis raises ``ValueError`` for reactive scalar values.
        """
        ...
    def function(
        self,
        function: Callable[[float], _TracedScalar],
        domain: Optional[tuple[float, float]] = None,
        *,
        normal_scale: float = 120.0,
        reveal: Optional[_TracedScalar] = None,
        samples: Optional[int] = None,
        tolerance: float = 0.75,
    ) -> Drawable:
        """Plot a traced scalar function perpendicular to this number line.

        Function outputs ``-1`` and ``1`` map to ``-normal_scale`` and
        ``normal_scale`` local canvas units. The callable runs once to capture
        a native expression. When ``reveal`` is provided, it is the exact
        data-space end of the visible curve, allowing a point and the path to
        share one ``Parameter`` without arc-length drift. Sampling and
        parameter updates remain in Rust.
        Invalid domains, sampling settings, or non-positive scales raise
        ``ValueError``.
        """
        ...
    def layer(self, name: Literal["axis", "ticks", "numbers", "labels"]) -> Drawable: ...
    def create(self, duration: Optional[float] = None) -> Anim: ...
    def write(self, duration: Optional[float] = None) -> Anim: ...

class PolarSpace:
    def drawable(self) -> Drawable: ...
    def coord(self, radius: float, angle: float) -> CoordinateRef: ...
    def layer(self, name: Literal["grid", "axes", "numbers"]) -> Drawable: ...
    def plot(self, function: Callable[[float], float], domain: tuple[float, float] = (0.0, 6.283185307179586), *, samples: int = 360) -> Drawable: ...
    def create(self, duration: Optional[float] = None) -> Anim: ...

class CoordinateSpace3D:
    def drawable(self) -> Drawable: ...
    def layer(self, name: Literal["grid", "axes", "ticks", "numbers", "labels"]) -> Drawable:
        """Return an independently animatable scale-aware 3D axes layer."""
        ...
    def at_3d(self, x: float, y: float, z: float) -> CoordinateSpace3D: ...
    def scaled(self, factor: float) -> CoordinateSpace3D: ...
    def create(self, duration: Optional[float] = None) -> Anim: ...
    def data_to_local(self, x: float, y: float, z: float) -> tuple[float, float, float]: ...
    def local_to_data(self, x: float, y: float, z: float) -> tuple[float, float, float]: ...
    def surface(self, function: Callable[[float, float], float], *, resolution: tuple[int, int] = (64, 48)) -> Drawable: ...
    def parametric(self, function: Callable[[float], tuple[float, float, float]], domain: tuple[float, float], *, samples: int = 320) -> Drawable: ...
    def vector_field(self, function: Callable[[float, float, float], tuple[float, float, float]], *, resolution: tuple[int, int, int] = (8, 8, 6), max_length: float = 24.0) -> Drawable: ...

Cartesian2D: TypeAlias = CoordinateSpace
Cartesian3D: TypeAlias = CoordinateSpace3D
ComplexSpace: TypeAlias = CoordinateSpace

class Scene:
    def __init__(
        self,
        width: int = 1280,
        height: int = 720,
        background: Optional[Color] = None,
        margin: Optional[float] = None,
        theme: Optional[str | Theme] = None,
    ) -> None:
        """Create a scene and optionally install its centralized theme.

        An explicit background wins over the theme background. Unknown theme
        names and invalid values raise ``ValueError``.
        """
        ...
    def parameter(self, initial: float) -> Parameter: ...
    def chart(self, spec: ChartSpec) -> Chart:
        """Materialize an immutable declarative chart using batched semantic layers."""
        ...
    def cartesian_2d(self, x: Axis, y: Axis, *, width: Optional[float] = None, height: Optional[float] = None, grid: bool = True) -> Cartesian2D:
        """Create a typed 2D Cartesian space for scientific functions and geometry."""
        ...
    def cartesian_3d(self, x: Axis, y: Axis, z: Axis, *, size: tuple[float, float, float] = (10.0, 8.0, 6.0), grid: bool = True) -> Cartesian3D:
        """Create a typed scale-aware 3D Cartesian space with independent layers."""
        ...
    def polar(self, radial: Axis, *, radius: float = 220.0, angle_divisions: int = 12) -> PolarSpace:
        """Create a typed polar space for scientific functions and geometry."""
        ...
    def complex(self, x: Optional[Axis] = None, y: Optional[Axis] = None, *, width: Optional[float] = None, height: Optional[float] = None) -> ComplexSpace:
        """Create a Cartesian complex plane with real and imaginary axes."""
        ...
    def readout(self, source: _TracedScalar | Callable[[], _TracedScalar], *, label: Optional[str] = None, format: str = ".2f", prefix: str = "", suffix: str = "", unit: Optional[str] = None, font_size: Optional[float] = None, color: Optional[Color] = None, invalid: str = "—") -> Readout:
        """Create a native numeric display with equally spaced, baseline-aligned terms.

        ``color`` applies to the label, reactive value, and unit and remains in
        effect when the number changes or the timeline seeks.
        """
        ...
    def variable(self, initial: float, *, label: str, format: str = ".2f", prefix: str = "", suffix: str = "", unit: Optional[str] = None, font_size: Optional[float] = None, color: Optional[Color] = None, invalid: str = "—") -> Variable:
        """Create an animatable scalar displayed as an aligned equation row.

        ``color`` applies to every visible term, including the changing value.
        """
        ...
    def number_line(self, axis: Axis, *, length: Optional[float] = None) -> NumberLine:
        """Create a standalone typed number line using the axis scale and formatting."""
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
        """Use brand on this Scene or create the requested value.

        Example:
            scene.brand()
        """
        ...
    def row(self, children: Sequence[Drawable | Layout | LayoutItem], *, gap: float = 24.0, padding: Padding = 0.0, width: SizeRule = "hug", height: SizeRule = "hug", align: Align = "center", justify: Justify = "start", wrap: bool = False, within: Optional[Literal["safe", "frame"]] = None) -> Layout:
        """Create a horizontal Layout v2 container in canvas units.

        ``width`` and ``height`` accept fixed values, ``"hug"``, or ``"fill"``.
        Responsive text keeps the width offered by its final row allocation,
        so tight glyph bounds do not trigger a second, narrower composition.
        Ownership errors are raised before render as ``LayoutOwnershipError``.
        """
        ...
    def column(self, children: Sequence[Drawable | Layout | LayoutItem], *, gap: float = 24.0, padding: Padding = 0.0, width: SizeRule = "hug", height: SizeRule = "hug", align: Align = "start", justify: Justify = "start", wrap: bool = False, within: Optional[Literal["safe", "frame"]] = None) -> Layout:
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
    def constrain(self, *constraints: LayoutConstraint, animate: Optional[float] = None) -> ConstraintSet:
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
    def assets_dir(self, path: str) -> None:
        """Use assets dir on this Scene or create the requested value.

        Example:
            scene.assets_dir("example")
        """
        ...
    def preload(self, paths: Sequence[str]) -> None:
        """Use preload on this Scene or create the requested value.

        Example:
            scene.preload(["assets/example.svg"])
        """
        ...
    def load_project(self, path: str = "gaanim.toml") -> None:
        """Use load project on this Scene or create the requested value.

        Example:
            scene.load_project()
        """
        ...
    def reload_assets(self) -> None:
        """Use reload assets on this Scene or create the requested value.

        Example:
            scene.reload_assets()
        """
        ...
    def audio(
        self,
        path: str,
        *,
        start: Optional[float] = None,
        duration: Optional[float] = None,
        volume: float = 1.0,
        fade_in: float = 0.0,
        fade_out: float = 0.0,
    ) -> None:
        """Use audio on this Scene or create the requested value.

        Example:
            scene.audio("example")
        """
        ...
    def circle(self, r: float) -> Drawable:
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
    def rect(self, w: float, h: float) -> Drawable:
        """Create a rect drawable in the scene.

        Example:
            result = scene.rect(1.0, 1.0)
        """
        ...
    def rounded_rect(self, w: float, h: float, r: float) -> Drawable:
        """Create a rounded rect drawable in the scene.

        Example:
            result = scene.rounded_rect(1.0, 1.0, 1.0)
        """
        ...
    def square(self, s: float) -> Drawable:
        """Create a square drawable in the scene.

        Example:
            result = scene.square(1.0)
        """
        ...
    def dot(self, r: float) -> Drawable:
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
    def line(self, x1: float, y1: float, x2: float, y2: float) -> Drawable:
        """Create a line drawable in the scene.

        Example:
            result = scene.line(1.0, 1.0, 1.0, 1.0)
        """
        ...
    def arrow(self, x1: float, y1: float, x2: float, y2: float) -> Drawable:
        """Create a arrow drawable in the scene.

        Example:
            result = scene.arrow(1.0, 1.0, 1.0, 1.0)
        """
        ...
    def dashed_line(
        self, x1: float, y1: float, x2: float, y2: float, *, dash_length: float = 16.0, gap_length: float = 10.0
    ) -> Drawable:
        """Create a dashed line drawable in the scene.

        Example:
            result = scene.dashed_line(1.0, 1.0, 1.0, 1.0)
        """
        ...
    def double_arrow(
        self, x1: float, y1: float, x2: float, y2: float, *, head_length: Optional[float] = None, head_width: Optional[float] = None
    ) -> Drawable:
        """Create a double arrow drawable in the scene.

        Example:
            result = scene.double_arrow(1.0, 1.0, 1.0, 1.0)
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
    def curved_arrow(self, x1: float, y1: float, x2: float, y2: float, angle: float) -> Drawable:
        """Create a curved arrow drawable in the scene.

        Example:
            result = scene.curved_arrow(1.0, 1.0, 1.0, 1.0, 1.0)
        """
        ...
    def curved_arrow_arc(self, cx: float, cy: float, radius: float, start_angle: float, sweep_angle: float) -> Drawable:
        """Create a curved arrow arc drawable in the scene.

        Example:
            result = scene.curved_arrow_arc(1.0, 1.0, 40.0, 1.0, 1.0)
        """
        ...
    def dimension(self, x1: float, y1: float, x2: float, y2: float, offset: float) -> Drawable:
        """Create a dimension drawable in the scene.

        Example:
            result = scene.dimension(1.0, 1.0, 1.0, 1.0, 1.0)
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
    def _legacy_function_graph(self, function: Callable[[float], float], x: tuple[float, float], samples: int = 160) -> Drawable:
        """Create a function graph drawable in the scene.

        Example:
            result = scene.function_graph(lambda x: x, (0.0, 0.0))
        """
        ...
    def _legacy_parametric_curve(self, function: Callable[[float], tuple[float, float]], t: tuple[float, float], samples: int = 240) -> Drawable:
        """Create a parametric curve drawable in the scene.

        Example:
            result = scene.parametric_curve(lambda t: (t, t), (0.0, 0.0))
        """
        ...
    def _legacy_axes(
        self,
        x: tuple[float, float, float] | tuple[float, float] | None = None,
        y: tuple[float, float, float] | tuple[float, float] | None = None,
        *,
        x_range: tuple[float, float] | tuple[float, float, float] | Sequence[float] | None = None,
        y_range: tuple[float, float] | tuple[float, float, float] | Sequence[float] | None = None,
        x_length: float | None = None,
        y_length: float | None = None,
        tips: bool = True,
        auto_fit: bool = True,
        axis_config: dict | None = None,
        x_axis_config: dict | None = None,
        y_axis_config: dict | None = None,
        grid: bool = True,
        ticks: bool = True,
        numbers: bool = True,
        labels: bool = True,
        x_axis: bool = True,
        y_axis: bool = True,
        x_grid: Optional[bool] = None,
        y_grid: Optional[bool] = None,
        x_ticks: Optional[bool] = None,
        y_ticks: Optional[bool] = None,
        x_numbers: Optional[bool] = None,
        y_numbers: Optional[bool] = None,
        x_label: Optional[str] = None,
        y_label: Optional[str] = None,
        axis_color: Optional[Color] = None,
        grid_color: Optional[Color] = None,
        tick_color: Optional[Color] = None,
        number_color: Optional[Color] = None,
        label_color: Optional[Color] = None,
        axis_width: float = 3.0,
        grid_width: float = 1.0,
        tick_width: float = 2.0,
        tick_length: float = 8.0,
    ) -> Drawable:
        """Create a axes drawable in the scene.

        Example:
            result = scene.axes()
        """
        ...
    def _legacy_axes_3d(
        self,
        x: tuple[float, float, float] | tuple[float, float] | Sequence[float] | None = None,
        y: tuple[float, float, float] | tuple[float, float] | Sequence[float] | None = None,
        z: tuple[float, float, float] | tuple[float, float] | Sequence[float] | None = None,
        x_range: tuple[float, float, float] | tuple[float, float] | Sequence[float] | None = None,
        y_range: tuple[float, float, float] | tuple[float, float] | Sequence[float] | None = None,
        z_range: tuple[float, float, float] | tuple[float, float] | Sequence[float] | None = None,
        grid: bool = True,
        ticks: bool = True,
        numbers: bool = True,
        labels: bool = True,
        x_axis: bool = True,
        y_axis: bool = True,
        z_axis: bool = True,
        xy_grid: Optional[bool] = None,
        xz_grid: Optional[bool] = None,
        yz_grid: Optional[bool] = None,
        x_ticks: Optional[bool] = None,
        y_ticks: Optional[bool] = None,
        z_ticks: Optional[bool] = None,
        x_numbers: Optional[bool] = None,
        y_numbers: Optional[bool] = None,
        z_numbers: Optional[bool] = None,
        x_label: Optional[str] = None,
        y_label: Optional[str] = None,
        z_label: Optional[str] = None,
        label_mode: Literal["billboard", "hud"] = "billboard",
        axis_color: Optional[Color] = None,
        grid_color: Optional[Color] = None,
        tick_color: Optional[Color] = None,
        number_color: Optional[Color] = None,
        label_color: Optional[Color] = None,
        axis_width: float = 3.0,
        grid_width: float = 1.0,
        tick_width: float = 2.0,
        tick_length: float = 8.0,
        auto_fit: bool = True,
        x_length: Optional[float] = None,
        y_length: Optional[float] = None,
        z_length: Optional[float] = None,
        tips: bool = True,
    ) -> Drawable:
        """Create 3D Cartesian axes and optional grid planes.

        ``x``, ``y`` and ``z`` accept ``(min, max)`` or
        ``(min, max, step)``. The ``*_range`` names are equivalent aliases.
        Use ``label_mode="billboard"`` for camera-facing labels or
        ``label_mode="hud"`` for fixed screen-space labels.

        Example:
            axes = scene.axes_3d(
                x_range=(-5, 5, 1),
                y_range=(-5, 5, 1),
                z_range=(-3, 3, 1),
                x_label="x", y_label="y", z_label="z",
            )

        Ranges must be finite with ``min < max`` and a positive step.
        """
        ...
    def _legacy_plot(self, axes: Drawable, func: Callable[[float], float], x: tuple[float, float], samples: int = 160) -> Drawable:
        """Create a plot drawable in the scene.

        Example:
            result = scene.plot(None, lambda x: x, (0.0, 0.0))
        """
        ...
    def _legacy_plot_parametric_curve(self, axes: Drawable, func: Callable[[float], tuple[float, float]], t: tuple[float, float], samples: int = 160) -> Drawable:
        """Create a plot parametric curve drawable in the scene.

        Example:
            result = scene.plot_parametric_curve(None, lambda t: (t, t), (0.0, 0.0))
        """
        ...
    def _legacy_get_graph(self, axes: Drawable, func: Callable[[float], float], x: tuple[float, float], samples: int = 160) -> Drawable:
        """Create a get graph drawable in the scene.

        Example:
            result = scene.get_graph(None, lambda x: x, (0.0, 0.0))
        """
        ...
    def text(
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
        color: Optional[Color] = None,
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
        """Create structured vector text, paragraphs, mathematics, or mixed content.

        ``*strong*`` selects bold text and ``_emphasis_`` selects italic text;
        escape literal markers as ``\\*`` and ``\\_``. Markers inside
        ``$...$`` remain math syntax, and ``\\$`` emits a literal dollar.
        Unbalanced or crossed markup, unbalanced math, duplicate sibling part
        names, and invalid metrics raise ``ValueError``. Direct keywords
        override reusable style/flow objects. Responsive wrapping consumes the
        Layout-v2 width offer or the scene safe frame; outer box dimensions
        remain Layout properties.

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
        color: Optional[Color] = None,
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

        The content is wrapped internally as ``$ ... $`` and accepts the same
        semantic parts, styles, flow options, selections, and animations as
        :meth:`text`. Omit the surrounding math delimiters. The spaces next to
        those delimiters are preserved to select Typst block math. Every
        content boundary becomes ordinary Typst whitespace, so ``"="`` does
        not need a written trailing space. Empty content and invalid
        mathematics raise ``ValueError``.

        Example:
            equation = scene.equation(
                part("sum_force", "sum F_t"),
                "=",
                parts(mass="m", acceleration="a_t"),
            )
            scene.play([equation.write(1.0, by="part")])
        """
        ...
    def typst(self, source: str, *, width: Optional[str | float | int] = None) -> Drawable:
        """Create a typst drawable in the scene.

        Example:
            result = scene.typst("example")
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
    def image(
        self,
        path: str,
        *,
        width: Optional[float] = None,
        height: Optional[float] = None,
        fit: str = "contain",
        crop: Optional[tuple[float, float, float, float]] = None,
    ) -> Drawable:
        """Create a image drawable in the scene.

        Example:
            result = scene.image("assets/example.svg")
        """
        ...
    def svg(self, path: str) -> Drawable:
        """Create a svg drawable in the scene.

        Example:
            result = scene.svg("assets/example.svg")
        """
        ...
    def gltf(self, path: str, *, scene: str | int | None = None) -> Drawable:
        """Import a local glTF 2.0 ``.gltf`` or ``.glb`` model."""
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
    def callout(
        self,
        text: str,
        target: Drawable,
        *,
        offset: tuple[float, float] = (160.0, 96.0),
        width: float = 240.0,
        height: float = 72.0,
        background: Optional[Color] = None,
        color: Optional[Color] = None,
    ) -> Drawable:
        """Create a callout drawable in the scene.

        Example:
            result = scene.callout("example", target)
        """
        ...
    def caption(
        self,
        text: str,
        *,
        position: Literal["top", "bottom"] = "bottom",
        width: float = 720.0,
        height: float = 92.0,
        margin: float = 32.0,
        background: Optional[Color] = None,
        color: Optional[Color] = None,
    ) -> Drawable:
        """Create a caption drawable in the scene.

        Example:
            result = scene.caption("example")
        """
        ...
    def title_card(
        self,
        title: str,
        subtitle: Optional[str] = None,
        *,
        width: float = 760.0,
        height: float = 320.0,
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
        width: float = 720.0,
        gap: float = 68.0,
        bullet_radius: float = 8.0,
        bullet_color: Optional[Color] = None,
        color: Optional[Color] = None,
    ) -> Drawable:
        """Create a bullets drawable in the scene.

        Example:
            result = scene.bullets(["Example"])
        """
        ...
    def _legacy_bar_chart(
        self,
        values: Sequence[float],
        *,
        labels: Optional[Sequence[str]] = None,
        width: float = 640.0,
        height: float = 320.0,
        gap: float = 20.0,
        color: Optional[Color] = None,
    ) -> Drawable:
        """Create a bar chart drawable in the scene.

        Example:
            result = scene.bar_chart([1.0, 2.0])
        """
        ...
    def table(
        self,
        headers: Sequence[str],
        rows: Sequence[Sequence[str]],
        *,
        width: float = 760.0,
        row_height: float = 58.0,
        header_background: Optional[Color] = None,
        rule_color: Optional[Color] = None,
        color: Optional[Color] = None,
    ) -> Drawable:
        """Create a table drawable in the scene.

        Example:
            result = scene.table(["Example"], [["Example"]])
        """
        ...
    def code(
        self,
        source: str,
        *,
        language: str = "text",
        width: float = 760.0,
        height: float = 300.0,
        font_size: float = 20.0,
        background: Optional[Color] = None,
        color: Optional[Color] = None,
        accent: Optional[Color] = None,
    ) -> Drawable:
        """Create a code drawable in the scene.

        Example:
            result = scene.code("example")
        """
        ...
    def segment(
        self,
        name: str,
        transition: Optional[Transition] = None,
        *,
        notes: Optional[str] = None,
        template: Optional[Callable[..., Layout]] = None,
    ) -> Segment:
        """Create and activate a named structural segment.

        Example:
            result = scene.segment("example")
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
    def wait(self, d: float) -> None:
        """Schedule wait on the scene timeline.

        Example:
            scene.wait(1.0)
        """
        ...
    def stop(self, name: Optional[str] = None) -> None:
        """Pause interactive playback at the current timeline position.

        Export ignores stops and renders the timeline continuously.
        """
        ...
    def play(self, anims: Sequence[Anim], lag: Optional[float] = None) -> None:
        """Schedule play on the scene timeline.

        Example:
            scene.play([animation])
        """
        ...
    def fade_out_all(self, d: float) -> None:
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
    def export(
        self,
        path: str,
        fps: Optional[int] = None,
        *,
        transparent: Optional[bool] = None,
        quality: Optional[Literal["draft", "standard", "production"]] = None,
        aspect_ratio: Optional[Literal["youtube", "tiktok", "instagram", "custom"]] = None,
        width: Optional[int] = None,
        height: Optional[int] = None,
        start_time: Optional[float] = None,
        end_time: Optional[float] = None,
        segment: Optional[str] = None,
        crf: Optional[int] = None,
        encoder: Literal["auto", "libx264", "nvenc", "amf", "qsv", "vaapi"] = "auto",
        speed: Optional[Literal["fast", "balanced", "best"]] = None,
    ) -> None:
        """Export the scene output.

        Example:
            scene.export("output.webp")
        """
        ...
    def snapshots(self, directory: str, times: Sequence[float]) -> int:
        """Snapshots the scene output.

        Example:
            result = scene.snapshots("example", [0.0, 1.0])
        """
        ...
    # Reactive geometry helpers
    def point_on_curve(self, curve: Drawable, tracker: Parameter) -> Drawable:
        """Create a hidden point-on-curve drawable; reveal it in ``scene.play``.

        Example:
            result = scene.point_on_curve(curve, None)
        """
        ...
    def tangent_on_curve(self, curve: Drawable, tracker: Parameter, length: float = 80.0) -> Drawable:
        """Create a hidden tangent drawable; reveal it in ``scene.play``.

        Example:
            result = scene.tangent_on_curve(curve, None)
        """
        ...
    def normal_on_curve(self, curve: Drawable, tracker: Parameter, length: float = 80.0) -> Drawable:
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
        min_distance: float = 1.0,
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
            dot = scene.dot(7).at_3d(1, 0, 0)
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
    def point_ref(self, x: _TracedScalar, y: _TracedScalar) -> PointRef:
        """Create a non-rendered point whose coordinates react to scalar expressions."""
        ...
    def offset_point(self, origin: Endpoint, dx: _TracedScalar, dy: _TracedScalar) -> PointRef:
        """Create a point offset from a moving origin by reactive scene-space components."""
        ...
    def point_between(self, from_: Endpoint, to: Endpoint, *, alpha: float = 0.5, offset: tuple[float, float] = (0.0, 0.0)) -> PointRef:
        """Create an affine point between endpoints plus a world-space offset."""
        ...
    def polar_point(self, origin: Endpoint, radius: _TracedScalar, angle: _TracedScalar) -> PointRef:
        """Create a reactive polar point; angle is measured in radians."""
        ...
    def bar_between(
        self,
        from_: Endpoint,
        to: Endpoint,
        *,
        width: float = 8.0,
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
        amplitude: float = 12.0,
        crossing: float = 0.0,
        start_straight: float = 12.0,
        end_straight: float = 12.0,
    ) -> Drawable:
        """Create a hidden reactive helical spring; reveal it in ``scene.play``.

        Endpoints may also be ``AnchorPoint`` references inside transformed
        groups. The native helix is regenerated every frame, preserving its
        radius while its pitch deforms with the distance.
        ``crossing`` ranges from 0 to 1: higher values make each turn fold
        back briefly, creating e-like visual crossings.
        ``start_straight`` and ``end_straight`` are non-negative scene-unit
        lengths of the straight segments at each endpoint; both default to 12.
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
        value: Optional[_TracedScalar] = None,
        format: str = ".2f",
        unit: Optional[str] = None,
        scale: float = 1.0,
        label_gap: float = 10.0,
        label_orientation: Literal["upright", "aligned"] = "upright",
        font_size: Optional[float] = None,
        color: Optional[Color] = None,
        line_width: float = 3.0,
        extension_style: Literal["solid", "dashed"] = "solid",
        dash_length: float = 12.0,
        gap_length: float = 8.0,
    ) -> Dimension:
        """Create a reactive technical dimension and optional annotation.

        The line follows fixed points, drawable origins, or anchored points.
        ``label`` remains symbolic; ``show_value`` adds the current XY distance
        multiplied by ``scale`` and formatted with ``format``/``unit``. Passing
        ``value`` (a number, ``Parameter``, ``Variable``, or traced expression)
        implies the numeric readout and takes precedence over both measured
        distance and ``scale`` while the dimension geometry keeps following its
        endpoints.
        ``label_orientation`` keeps text horizontal or aligned while avoiding
        upside-down labels. ``color`` initializes the extension lines,
        solid triangular arrowheads and the complete annotation, including its
        reactive value. Math labels and reactive values share one 48-unit typographic baseline by default, including
        subscripted formulas. ``line_width`` controls the filled line geometry;
        dashed extensions use ``dash_length`` and ``gap_length``. Invalid
        metrics, extension styles, or orientation raise ``ValueError``.

        Example:
            width = scene.dimension_between(
                left, right, 45, label="$W_f$", show_value=True, unit="mm"
            )
        """
        ...
    def angle_between(
        self,
        vertex: Endpoint,
        from_: AngleRay,
        to: AngleRay,
        *,
        radius: float = 64.0,
        label: Optional[str] = None,
        show_value: bool = False,
        format: str = ".1f",
        unit: Literal["deg", "rad"] = "deg",
        sweep: Literal["minor", "major", "cw", "ccw"] = "minor",
        arrowheads: Literal["none", "start", "end", "both"] = "both",
        label_gap: float = 12.0,
        label_orientation: Literal["upright", "aligned"] = "upright",
        show_extensions: bool = True,
        font_size: Optional[float] = None,
        color: Optional[Color] = None,
    ) -> AngleDimension:
        """Create a same-frame angular dimension from fixed directions or endpoints.

        ``color`` applies to the arc, arrows, label, reactive value, and unit.
        Degenerate rays hide the geometry; invalid modes or metrics raise ``ValueError``.
        """
        ...
    def vector_between(self, from_: Endpoint, to: Endpoint, *, label: Optional[str] = None, show_value: bool = False, format: str = ".1f", unit: Optional[str] = None, scale: float = 1.0, label_gap: float = 14.0, font_size: Optional[float] = None, color: Optional[Color] = None) -> ForceVector:
        """Create a reactive vector with accessible shaft, solid head, and readout parts.

        ``color`` applies to the vector and every readout term, including the
        reactive numeric value after updates and seeks.
        """
        ...
    def force_at(self, origin: Endpoint, magnitude: _TracedScalar, *, direction: _TracedScalar = 0.0, visual_scale: float = 1.0, label: Optional[str] = None, show_value: bool = False, format: str = ".1f", unit: str = "N", label_gap: float = 14.0, font_size: Optional[float] = None, color: Optional[Color] = None) -> ForceVector:
        """Create a reactive force from physical magnitude and direction in radians.

        ``visual_scale`` converts physical units into scene units and must be
        positive. The optional readout reports the physical magnitude, and
        ``color`` also applies to that changing number.
        """
        ...
    def force_from_components(self, origin: Endpoint, fx: _TracedScalar, fy: _TracedScalar, *, visual_scale: float = 1.0, label: Optional[str] = None, show_value: bool = False, format: str = ".1f", unit: str = "N", label_gap: float = 14.0, font_size: Optional[float] = None, color: Optional[Color] = None) -> ForceVector:
        """Create a reactive force from physical X/Y components relative to a moving origin.

        ``color`` applies to the force and the complete reactive readout.
        """
        ...
    def support_at(self, point: Endpoint, *, kind: Literal["fixed", "pin", "roller", "simple", "guided", "prismatic", "cable", "spring"] = "pin", direction: Optional[Direction] = None, size: float = 48.0, ground_length: float = 70.0, color: Optional[Color] = None) -> Support:
        """Create a theme-aware vector support following ``point``.

        Direction runs from the base toward the connection; sizes are scene units.
        """
        ...
    def fixed_support(self, point: Endpoint, *, direction: Optional[Direction] = None, size: float = 48.0, ground_length: float = 70.0, color: Optional[Color] = None) -> Support:
        """Create a fixed or ceiling support with plate and consistent hatching."""
        ...
    def pin_support(self, point: Endpoint, *, direction: Optional[Direction] = None, size: float = 48.0, ground_length: float = 70.0, color: Optional[Color] = None) -> Support:
        """Create a triangular pinned support with a circular joint."""
        ...
    def roller_support(self, point: Endpoint, *, direction: Optional[Direction] = None, size: float = 48.0, ground_length: float = 70.0, color: Optional[Color] = None) -> Support:
        """Create a triangular support on two aligned rollers."""
        ...
    def guided_support(self, point: Endpoint, *, direction: Optional[Direction] = None, size: float = 48.0, ground_length: float = 70.0, color: Optional[Color] = None) -> Support:
        """Create a guided carriage support aligned with ``direction``."""
        ...
    def joint_at(self, point: Endpoint, *, kind: Literal["revolute", "prismatic"] = "revolute", axis: Optional[Direction] = None, size: float = 36.0, color: Optional[Color] = None) -> Drawable:
        """Create a standalone reactive revolute or prismatic joint symbol."""
        ...
    def gear(self, radius: float, teeth: int, *, bore_radius: float = 8.0, color: Optional[Color] = None) -> Drawable:
        """Create an editorial gear silhouette; geometry is illustrative, not manufacturing involute."""
        ...
    def rack(self, length: float, teeth: int, *, color: Optional[Color] = None) -> Drawable:
        """Create an editorial straight rack with evenly spaced teeth."""
        ...
    def cam_profile(self, samples: Sequence[tuple[float, float]], *, bore_radius: float = 8.0, color: Optional[Color] = None) -> Drawable:
        """Create a closed radial cam from ``(angle_radians, radius)`` samples."""
        ...
    def contact_on_curve(self, curve: Drawable, tracker: Parameter | Variable, *, tangent_length: float = 80.0, normal_length: float = 80.0) -> Drawable:
        """Group a reactive contact point, tangent, and normal on a sampled curve."""
        ...
    def moment_about(self, center: Endpoint, radius: float, *, direction: Literal["cw", "ccw"] = "ccw", label: Optional[str] = None, color: Optional[Color] = None) -> Drawable:
        """Create a curved moment arrow that follows a reactive center."""
        ...
    def coordinate_frame_at(self, origin: Endpoint, x_direction: Direction, *, length: float = 70.0, labels: Optional[tuple[str, str]] = None, color: Optional[Color] = None) -> Drawable:
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
