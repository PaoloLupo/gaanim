"""Typed public API for Gaanim.

The examples in these stubs are intended to be copied into a small Scene
script. All camera durations are in seconds; 3D angles are in radians.
"""

from __future__ import annotations

from typing import Callable, ClassVar, Literal, Optional, Sequence, TypeAlias, overload

CurvePoint: TypeAlias = tuple[float, float]
"""A coordinate pair used by :meth:`Scene.path` and :meth:`Scene.curve`."""

CurveControl: TypeAlias = CurvePoint | Literal["auto"] | None
"""A Bézier control point, an automatically reflected handle, or a collapsed handle."""

CurveCommand: TypeAlias = tuple[str, Sequence[CurvePoint | CurveControl]]
"""A ``Scene.path`` or ``Scene.curve`` command and its arguments."""

class Color:
    def __init__(self, r: int, g: int, b: int, a: int = 255) -> None:
        """Create a Color instance.

        Example:
            Color(30, 80, 160)
        """
        ...

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

class Theme:
    def __init__(
        self,
        base: Optional[str | Theme] = None,
        *,
        name: Optional[str] = None,
        colors: Optional[dict[str, ColorLike]] = None,
        fonts: Optional[dict[str, str]] = None,
        sizes: Optional[dict[str, float]] = None,
        font_files: Optional[dict[str, str]] = None,
    ) -> None:
        """Create a Theme instance.

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

class LayoutRegion:
    width: float
    height: float
    def place(self, drawable: Drawable, anchor: Anchor) -> Drawable:
        """Use place to configure or query this layout.

        Example:
            result = region.place(drawable, Anchor.CENTER)
        """
        ...
    def point(self, anchor: Anchor) -> tuple[float, float]:
        """Use point to configure or query this layout.

        Example:
            result = region.point(Anchor.CENTER)
        """
        ...
    def inset(
        self,
        value: float,
        right: Optional[float] = None,
        bottom: Optional[float] = None,
        left: Optional[float] = None,
    ) -> LayoutRegion:
        """Use inset to configure or query this layout.

        Example:
            result = region.inset(1.0)
        """
        ...
    def grid(
        self,
        rows: int = 1,
        columns: int = 1,
        row_gap: float = 0.0,
        column_gap: float = 0.0,
    ) -> GridLayout:
        """Use grid to configure or query this layout.

        Example:
            result = region.grid()
        """
        ...
    def grid_tracks(
        self,
        rows: Sequence[float | str],
        columns: Sequence[float | str],
        row_gap: float = 0.0,
        column_gap: float = 0.0,
    ) -> GridLayout:
        """Use grid tracks to configure or query this layout.

        Example:
            result = region.grid_tracks(2, 1.0)
        """
        ...
    def layout(
        self,
        kind: Literal["row", "column", "grid"] = "column",
        *,
        gap: float = 24.0,
        columns: int = 2,
        width: Optional[float] = None,
        height: Optional[float] = None,
        fit: Literal["none", "shrink"] = "none",
        wrap: bool = False,
        justify: Literal["start", "center", "end", "between"] = "center",
    ) -> Layout:
        """Use layout to configure or query this layout.

        Example:
            result = region.layout()
        """
        ...

class GridLayout:
    rows: int
    columns: int
    def cell(self, row: int, column: int) -> LayoutRegion:
        """Use cell to configure or query this layout.

        Example:
            result = grid.cell(1, 1)
        """
        ...
    def area(
        self,
        row: int,
        column: int,
        row_span: int = 1,
        column_span: int = 1,
    ) -> LayoutRegion:
        """Use area to configure or query this layout.

        Example:
            result = grid.area(1, 1)
        """
        ...

class FrameLayout:
    frame: LayoutRegion
    header: LayoutRegion
    content: LayoutRegion
    footer: LayoutRegion
    def column(self, index: int, count: int = 2, gap: float = 24.0) -> LayoutRegion:
        """Use column to configure or query this layout.

        Example:
            result = frame.column(1)
        """
        ...

class Flow:
    count: int
    def add(self, drawable: Drawable) -> None:
        """Use add to configure or query this layout.

        Example:
            flow.add(drawable)
        """
        ...
    def build(self) -> Drawable:
        """Use build to configure or query this layout.

        Example:
            result = flow.build()
        """
        ...

class Layout:
    count: int
    @property
    def drawable(self) -> Drawable:
        """Read the drawable value from this Layout.

        Example:
            value = layout.drawable
        """
        ...
    def add(
        self,
        child: Drawable | Layout,
        *,
        at: Optional[int] = None,
        animate: Optional[float] = None,
    ) -> Drawable:
        """Use add to configure or query this layout.

        Example:
            result = layout.add(child)
        """
        ...
    def remove(self, child: Drawable | Layout, *, animate: Optional[float] = None) -> None:
        """Use remove to configure or query this layout.

        Example:
            layout.remove(child)
        """
        ...
    def replace(
        self,
        old: Drawable | Layout,
        new: Drawable | Layout,
        *,
        animate: Optional[float] = None,
    ) -> Drawable:
        """Use replace to configure or query this layout.

        Example:
            result = layout.replace(old, new)
        """
        ...
    def reflow(self, *, animate: Optional[float] = None) -> None:
        """Use reflow to configure or query this layout.

        Example:
            layout.reflow()
        """
        ...
    def configure(
        self,
        *,
        kind: Optional[Literal["row", "column", "grid"]] = None,
        gap: Optional[float] = None,
        columns: Optional[int] = None,
        width: Optional[float] = None,
        height: Optional[float] = None,
        fit: Optional[Literal["none", "shrink"]] = None,
        wrap: Optional[bool] = None,
        justify: Optional[Literal["start", "center", "end", "between"]] = None,
        animate: Optional[float] = None,
    ) -> None:
        """Use configure to configure or query this layout.

        Example:
            layout.configure()
        """
        ...

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
        """Configure this animation with stroke width.

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
    def part(self, id: str) -> Drawable:
        """Return a named SVG part or glTF node by unique name/canonical path."""
        ...
    def parts(self) -> tuple[str, ...]: ...
    def animations(self) -> tuple[str, ...]: ...
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
    def fill(self, paint: Paint) -> Drawable:
        """Apply fill to this drawable and return the result.

        Example:
            result = drawable.fill(BLUE)
        """
        ...
    def no_fill(self) -> Drawable:
        """Apply no fill to this drawable and return the result.

        Example:
            result = drawable.no_fill()
        """
        ...
    def stroke(self, paint: Paint, width: float) -> Drawable:
        """Apply stroke to this drawable and return the result.

        Example:
            result = drawable.stroke(BLUE, 1.0)
        """
        ...
    def no_stroke(self) -> Drawable:
        """Apply no stroke to this drawable and return the result.

        Example:
            result = drawable.no_stroke()
        """
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
    def color_by(self, fragment: str, color: Color) -> Drawable:
        """Apply color by to this drawable and return the result.

        Example:
            result = drawable.color_by("example", BLUE)
        """
        ...
    def select(self, fragment: str, occurrence: Optional[int] = None) -> FragmentSelection:
        """Apply select to this drawable and return the result.

        Example:
            result = drawable.select("example")
        """
        ...
    def tag(self, name: str) -> FragmentSelection:
        """Apply tag to this drawable and return the result.

        Example:
            result = drawable.tag("example")
        """
        ...
    def indicate_tag(self, name: str, duration: Optional[float] = None) -> Drawable:
        """Apply indicate tag to this drawable and return the result.

        Example:
            result = drawable.indicate_tag("example")
        """
        ...
    def cancel_term(self, name: str, duration: Optional[float] = None) -> Drawable:
        """Apply cancel term to this drawable and return the result.

        Example:
            result = drawable.cancel_term("example")
        """
        ...
    def reveal_fragment(
        self,
        fragment: str,
        *,
        style: str = "fade",
        duration: Optional[float] = None,
        occurrence: Optional[int] = None,
    ) -> Drawable:
        """Apply reveal fragment to this drawable and return the result.

        Example:
            result = drawable.reveal_fragment("example")
        """
        ...
    def opacity(self, op: float) -> Drawable:
        """Apply opacity to this drawable and return the result.

        Example:
            result = drawable.opacity(1.0)
        """
        ...
    def z_index(self, z: int) -> Drawable:
        """Apply z index to this drawable and return the result.

        Example:
            result = drawable.z_index(1)
        """
        ...
    def at(self, x: float, y: float) -> Drawable:
        """Apply at to this drawable and return the result.

        Example:
            result = drawable.at(1.0, 1.0)
        """
        ...
    def at_3d(self, x: float, y: float, z: float) -> Drawable:
        """Place the drawable at a 3D world-space position.

        Coordinates are interpreted by the perspective camera. The method is
        chainable and returns the same ``Drawable``.

        Example:
            dot = scene.dot(8).fill(RED).at_3d(1.0, 2.0, 0.5)
        """
        ...
    def billboard(self) -> Drawable:
        """Keep a 3D drawable facing the perspective camera.

        This is useful for labels and markers attached to a 3D scene. The
        method is chainable and returns the same ``Drawable``.

        Example:
            label = scene.text("origin").at_3d(0.0, 1.0, 0.0).billboard()
        """
        ...
    def hud(self) -> Drawable:
        """Pin the drawable to the screen as a fixed HUD overlay.

        HUD drawables use screen-space coordinates and are not affected by
        the 3D camera. Use ``.at(x, y)`` after ``.hud()`` to position them in
        the viewport. The method is chainable and returns the same
        ``Drawable``.

        Example:
            title = scene.text("glTF demo").hud().at(0.0, 300.0)
        """
        ...
    def scaled(self, factor: float) -> Drawable:
        """Apply scaled to this drawable and return the result.

        Example:
            result = drawable.scaled(1.0)
        """
        ...
    def scaled_3d(self, x: float, y: float, z: float) -> Drawable: ...
    def rotated(self, radians: float) -> Drawable:
        """Apply rotated to this drawable and return the result.

        Example:
            result = drawable.rotated(1.0)
        """
        ...
    def rotated_3d(self, x: float, y: float, z: float) -> Drawable: ...
    def with_pivot(self, x: float, y: float) -> Drawable:
        """Apply with pivot to this drawable and return the result.

        Example:
            result = drawable.with_pivot(1.0, 1.0)
        """
        ...
    def with_pivot_3d(self, x: float, y: float, z: float) -> Drawable: ...
    def pivot(self, x: float, y: float) -> Drawable:
        """Apply pivot to this drawable and return the result.

        Example:
            result = drawable.pivot(1.0, 1.0)
        """
        ...
    def at_anchor(self, x: float, y: float, anchor: Anchor) -> Drawable:
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
    ) -> Drawable:
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
    ) -> Drawable:
        """Apply align to to this drawable and return the result.

        Example:
            result = drawable.align_to(reference, Anchor.CENTER)
        """
        ...
    def to_edge(self, direction: Direction, buff: float = 24.0) -> Drawable:
        """Apply to edge to this drawable and return the result.

        Example:
            result = drawable.to_edge(Direction.RIGHT)
        """
        ...
    def to_corner(self, corner: Anchor, buff: float = 24.0) -> Drawable:
        """Apply to corner to this drawable and return the result.

        Example:
            result = drawable.to_corner(Anchor.CENTER)
        """
        ...
    def vstack(self, gap: float = 24.0, align: Optional[Anchor] = None) -> Drawable:
        """Apply vstack to this drawable and return the result.

        Example:
            result = drawable.vstack()
        """
        ...
    def hstack(self, gap: float = 24.0, align: Optional[Anchor] = None) -> Drawable:
        """Apply hstack to this drawable and return the result.

        Example:
            result = drawable.hstack()
        """
        ...
    def move(self, dx: float, dy: float) -> Anim:
        """Create a move animation for this drawable.

        Example:
            result = drawable.move(1.0, 1.0)
        """
        ...
    def move_to(self, x: float, y: float) -> Anim:
        """Create a move to animation for this drawable.

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
    def write_by_term(
        self,
        *,
        tags: Optional[Sequence[str]] = None,
        duration: float = 1.0,
    ) -> Drawable:
        """Apply write by term to this drawable and return the result.

        Example:
            result = drawable.write_by_term()
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
        """Create a indicate animation for this drawable.

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
    def remove_updater(self) -> None:
        """Use remove updater on this Drawable or create the requested value.

        Example:
            drawable.remove_updater()
        """
        ...
    def bind_y_from(self, source: Drawable) -> None:
        """Use bind y from on this Drawable or create the requested value.

        Example:
            drawable.bind_y_from(source)
        """
        ...
    def bind_x_from(self, source: Drawable) -> None:
        """Use bind x from on this Drawable or create the requested value.

        Example:
            drawable.bind_x_from(source)
        """
        ...
    def attach_to(self, source: Drawable) -> None:
        """Use attach to on this Drawable or create the requested value.

        Example:
            drawable.attach_to(source)
        """
        ...
    def follow_to(self, source: Drawable, offset: tuple[float, float]) -> None:
        """Use follow to on this Drawable or create the requested value.

        Example:
            drawable.follow_to(source, (0.0, 0.0))
        """
        ...
    def bind_position_from(self, source: Drawable, axes: str = "xy") -> None:
        """Use bind position from on this Drawable or create the requested value.

        Example:
            drawable.bind_position_from(source)
        """
        ...
    # manim Axes compatibility — coords mapping and graph helpers (only valid when self is an axes)
    def coords_to_point(self, x: float, y: float) -> tuple[float, float]:
        """Use coords to point on this Drawable or create the requested value.

        Example:
            result = axes.coords_to_point(1.0, 1.0)
        """
        ...
    def point_to_coords(self, point: tuple[float, float]) -> tuple[float, float]:
        """Use point to coords on this Drawable or create the requested value.

        Example:
            result = axes.point_to_coords((0.0, 0.0))
        """
        ...
    @overload
    def plot(self, func: Callable[[float], float], x_range: tuple[float, float] | tuple[float, float, float], samples: int = 160) -> Drawable:
        """Apply plot to this drawable and return the result.

        Example:
            result = axes.plot(lambda x: x, (0.0, 0.0, 0.0))
        """
        ...
    @overload
    def plot(self, func: Callable[[float], float], x: tuple[float, float], samples: int = 160) -> Drawable:
        """Apply plot to this drawable and return the result.

        Example:
            result = axes.plot(lambda x: x, (0.0, 0.0))
        """
        ...
    def plot_parametric_curve(self, func: Callable[[float], tuple[float, float]], t_range: tuple[float, float] | None = None, t: tuple[float, float] | None = None, samples: int = 160) -> Drawable:
        """Apply plot parametric curve to this drawable and return the result.

        Example:
            result = axes.plot_parametric_curve(lambda x: x)
        """
        ...
    def get_graph(self, func: Callable[[float], float], x_range: tuple[float, float] | None = None, x: tuple[float, float] | None = None, samples: int = 160) -> Drawable:
        """Apply get graph to this drawable and return the result.

        Example:
            result = axes.get_graph(lambda x: x)
        """
        ...
    def get_x_axis(self) -> Drawable:
        """Apply get x axis to this drawable and return the result.

        Example:
            result = axes.get_x_axis()
        """
        ...
    def get_y_axis(self) -> Drawable:
        """Apply get y axis to this drawable and return the result.

        Example:
            result = axes.get_y_axis()
        """
        ...
    def get_axes(self) -> Drawable:
        """Apply get axes to this drawable and return the result.

        Example:
            result = axes.get_axes()
        """
        ...
    def add_coordinates(self) -> Drawable:
        """Apply add coordinates to this drawable and return the result.

        Example:
            result = axes.add_coordinates()
        """
        ...

class FragmentSelection:
    def fill(self, color: Color) -> FragmentSelection:
        """Apply fill to the selected fragment.

        Example:
            result = selection.fill(BLUE)
        """
        ...
    def indicate(self, duration: Optional[float] = None) -> FragmentSelection:
        """Apply indicate to the selected fragment.

        Example:
            result = selection.indicate()
        """
        ...
    def reveal(self, style: str = "fade", duration: Optional[float] = None) -> FragmentSelection:
        """Apply reveal to the selected fragment.

        Example:
            result = selection.reveal()
        """
        ...
    def cancel(self, duration: Optional[float] = None) -> FragmentSelection:
        """Apply cancel to the selected fragment.

        Example:
            result = selection.cancel()
        """
        ...
    def color_to(self, color: Color, duration: Optional[float] = None) -> FragmentSelection:
        """Apply color to to the selected fragment.

        Example:
            result = selection.color_to(BLUE)
        """
        ...
    def transform_to(self, target: FragmentSelection, duration: Optional[float] = None) -> FragmentSelection:
        """Apply transform to to the selected fragment.

        Example:
            result = selection.transform_to(target)
        """
        ...

class ValueTracker:
    current: float
    def get_value(self) -> float:
        """Use get value on this ValueTracker or create the requested value.

        Example:
            result = tracker.get_value()
        """
        ...
    def set_value(self, value: float) -> None:
        """Use set value on this ValueTracker or create the requested value.

        Example:
            tracker.set_value(1.0)
        """
        ...
    def animate_to(self, value: float) -> Anim:
        """Use animate to on this ValueTracker or create the requested value.

        Example:
            result = tracker.animate_to(1.0)
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
    def safe_area(self) -> LayoutRegion:
        """Configure the canvas with safe area.

        Example:
            result = scene.canvas.safe_area()
        """
        ...

class Slide:
    def step(self, name: Optional[str] = None) -> None:
        """Use step on this Slide or create the requested value.

        Example:
            slide.step()
        """
        ...
    def region(self, name: str) -> LayoutRegion:
        """Use region on this Slide or create the requested value.

        Example:
            result = slide.region("example")
        """
        ...

class Camera:
    def pan_to(self, x: float, y: float, duration: float = 1.0) -> None:
        """Configure the camera with pan to.

        Example:
            scene.camera.pan_to(1.0, 1.0)
        """
        ...
    def zoom_to(self, zoom: float, duration: float = 1.0) -> None:
        """Configure the camera with zoom to.

        Example:
            scene.camera.zoom_to(1.0)
        """
        ...
    def frame_to(
        self,
        target: Drawable,
        margin: float = 40.0,
        duration: float = 1.0,
    ) -> None:
        """Configure the camera with frame to.

        Example:
            scene.camera.frame_to(target)
        """
        ...
    def rotate_to(self, angle: float, duration: float = 1.0) -> None:
        """Configure the camera with rotate to.

        Example:
            scene.camera.rotate_to(1.0)
        """
        ...
    def follow(self, target: Drawable, duration: float = 1.0) -> None:
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
    ) -> None:
        """Configure the camera with shake.

        Example:
            scene.camera.shake()
        """
        ...
    def look_at(
        self,
        eye: tuple[float, float, float],
        target: tuple[float, float, float],
        up: Optional[tuple[float, float, float]] = None,
        duration: float = 1.0,
    ) -> None:
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
    ) -> None:
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
    ) -> None:
        """Use perspective projection with a vertical field of view.

        ``fov_y`` is in radians and must be positive. ``near`` and ``far``
        are positive clipping distances with ``near < far``.

        Example:
            scene.camera.perspective(0.785, near=0.1, far=1000.0, duration=0.0)
        """
        ...

    def dolly(self, factor: float, duration: float = 1.0) -> None:
        """Move toward or away from the current target.

        A factor below ``1`` moves closer; a factor above ``1`` moves farther.
        The factor must be finite and positive.

        Example:
            scene.camera.dolly(factor=0.85, duration=0.6)
        """
        ...

class Scene:
    def __init__(
        self,
        width: int = 1280,
        height: int = 720,
        background: Optional[Color] = None,
        margin: Optional[float] = None,
    ) -> None:
        """Create a Scene instance.

        Example:
            Scene()
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
    def frame_layout(
        self,
        header: float = 0.0,
        footer: float = 0.0,
        gap: float = 24.0,
    ) -> FrameLayout:
        """Use frame layout on this Scene or create the requested value.

        Example:
            result = scene.frame_layout()
        """
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
    def layout(
        self,
        kind: Literal["row", "column", "grid"] = "column",
        *,
        gap: float = 24.0,
        columns: int = 2,
        width: Optional[float] = None,
        height: Optional[float] = None,
        fit: Literal["none", "shrink"] = "none",
        wrap: bool = False,
        justify: Literal["start", "center", "end", "between"] = "center",
    ) -> Layout:
        """Use layout on this Scene or create the requested value.

        Example:
            result = scene.layout()
        """
        ...
    def layout_preset(
        self,
        name: Literal["lecture", "comparison", "vertical_short", "minimal"],
    ) -> FrameLayout:
        """Use layout preset on this Scene or create the requested value.

        Example:
            result = scene.layout_preset(None)
        """
        ...
    def flow(
        self,
        direction: Literal["vertical", "horizontal"] = "vertical",
        gap: float = 24.0,
        align: Optional[Anchor] = None,
    ) -> Flow:
        """Use flow on this Scene or create the requested value.

        Example:
            result = scene.flow()
        """
        ...
    def circle(self, r: float) -> Drawable:
        """Create a circle drawable in the scene.

        Example:
            result = scene.circle(1.0)
        """
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
    def function_graph(self, function: Callable[[float], float], x: tuple[float, float], samples: int = 160) -> Drawable:
        """Create a function graph drawable in the scene.

        Example:
            result = scene.function_graph(lambda x: x, (0.0, 0.0))
        """
        ...
    def parametric_curve(self, function: Callable[[float], tuple[float, float]], t: tuple[float, float], samples: int = 240) -> Drawable:
        """Create a parametric curve drawable in the scene.

        Example:
            result = scene.parametric_curve(lambda t: (t, t), (0.0, 0.0))
        """
        ...
    def axes(
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
    def axes_3d(
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
    def plot(self, axes: Drawable, func: Callable[[float], float], x: tuple[float, float], samples: int = 160) -> Drawable:
        """Create a plot drawable in the scene.

        Example:
            result = scene.plot(None, lambda x: x, (0.0, 0.0))
        """
        ...
    def plot_parametric_curve(self, axes: Drawable, func: Callable[[float], tuple[float, float]], t: tuple[float, float], samples: int = 160) -> Drawable:
        """Create a plot parametric curve drawable in the scene.

        Example:
            result = scene.plot_parametric_curve(None, lambda t: (t, t), (0.0, 0.0))
        """
        ...
    def get_graph(self, axes: Drawable, func: Callable[[float], float], x: tuple[float, float], samples: int = 160) -> Drawable:
        """Create a get graph drawable in the scene.

        Example:
            result = scene.get_graph(None, lambda x: x, (0.0, 0.0))
        """
        ...
    def text(self, s: str) -> Drawable:
        """Create a text drawable in the scene.

        Example:
            result = scene.text("example")
        """
        ...
    def paragraph(
        self,
        s: str,
        width: float,
        *,
        align: Literal["left", "center", "right", "justify"] = "left",
        line_spacing: float = 1.2,
        font_size: Optional[float] = None,
        font_family: Optional[str] = None,
        max_lines: Optional[int] = None,
        overflow: Literal["visible", "clip"] = "clip",
    ) -> Drawable:
        """Create a paragraph drawable in the scene.

        Example:
            result = scene.paragraph("example", 40.0)
        """
        ...
    def title(self, s: str) -> Drawable:
        """Create a title drawable in the scene.

       …21366 tokens truncated… Example:
            result = Transition.fade_through(1.0, BLUE)
        """
        ...
    def subtitle(self, s: str) -> Drawable:
        """Create a subtitle drawable in the scene.

        Example:
            result = scene.subtitle("example")
        """
        ...
    def equation(self, s: str, *, tags: Optional[dict[str, str]] = None) -> Drawable:
        """Create a equation drawable in the scene.

        Example:
            result = scene.equation("example")
        """
        ...
    def typst(self, source: str, *, width: Optional[str | float | int] = None) -> Drawable:
        """Create a typst drawable in the scene.

        Example:
            result = scene.typst("example")
        """
        ...
    def transform_equation(
        self,
        source: Drawable,
        target: Drawable,
        *,
        tags: Optional[Sequence[str]] = None,
        duration: float = 1.0,
    ) -> None:
        """Configure or query the scene with transform equation.

        Example:
            scene.transform_equation(source, target)
        """
        ...
    def expand_equation(
        self,
        source: Drawable,
        target: Drawable,
        *,
        tag: str,
        duration: float = 1.0,
    ) -> None:
        """Configure or query the scene with expand equation.

        Example:
            scene.expand_equation(source, target, tag="example")
        """
        ...
    def replace_term(
        self,
        source: Drawable,
        target: Drawable,
        *,
        tag: str,
        target_tag: Optional[str] = None,
        duration: float = 1.0,
    ) -> None:
        """Configure or query the scene with replace term.

        Example:
            scene.replace_term(source, target, tag="example")
        """
        ...
    def step_equation(
        self,
        source: Drawable,
        target: Drawable,
        *,
        duration: float = 1.0,
    ) -> None:
        """Configure or query the scene with step equation.

        Example:
            scene.step_equation(source, target)
        """
        ...
    def transform_matching_shapes(self, source: Drawable, target: Drawable, *, duration: float = 1.0) -> None:
        """Configure or query the scene with transform matching shapes.

        Example:
            scene.transform_matching_shapes(source, target)
        """
        ...
    def transform_matching_tex(self, source: Drawable, target: Drawable, *, duration: float = 1.0) -> None:
        """Configure or query the scene with transform matching tex.

        Example:
            scene.transform_matching_tex(source, target)
        """
        ...
    def transform_matching_text(self, source: Drawable, target: Drawable, *, duration: float = 1.0) -> None:
        """Configure or query the scene with transform matching text.

        Example:
            scene.transform_matching_text(source, target)
        """
        ...
    def transform_matching(self, source: Drawable, target: Drawable, *, mode: str = "shapes", duration: float = 1.0) -> None:
        """Configure or query the scene with transform matching.

        Example:
            scene.transform_matching(source, target)
        """
        ...
    def focus_equation(
        self,
        equation: Drawable,
        tags: Sequence[str],
        *,
        duration: float = 1.0,
        dim_opacity: float = 0.25,
    ) -> None:
        """Configure or query the scene with focus equation.

        Example:
            scene.focus_equation(equation, ["term"])
        """
        ...
    def brace_label(self, equation: Drawable, tag: str, label: str, *, above: bool = False, duration: float = 0.6) -> None:
        """Configure or query the scene with brace label.

        Example:
            scene.brace_label(equation, "example", "example")
        """
        ...
    def annotate_tag(
        self, equation: Drawable, tag: str, label: str, *, offset: tuple[float, float] = (120.0, 80.0), duration: float = 0.6
    ) -> None:
        """Configure or query the scene with annotate tag.

        Example:
            scene.annotate_tag(equation, "example", "example")
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
    def camera_pan_to(self, x: float, y: float, duration: float = 1.0) -> None:
        """Configure or query the scene with camera pan to.

        Example:
            scene.camera_pan_to(1.0, 1.0)
        """
        ...
    def camera_zoom_to(self, zoom: float, duration: float = 1.0) -> None:
        """Configure or query the scene with camera zoom to.

        Example:
            scene.camera_zoom_to(1.0)
        """
        ...
    def camera_frame_to(self, target: Drawable, margin: float = 40.0, duration: float = 1.0) -> None:
        """Configure or query the scene with camera frame to.

        Example:
            scene.camera_frame_to(target)
        """
        ...
    def camera_rotate_to(self, angle: float, duration: float = 1.0) -> None:
        """Configure or query the scene with camera rotate to.

        Example:
            scene.camera_rotate_to(1.0)
        """
        ...
    def camera_follow(self, target: Drawable, duration: float = 1.0) -> None:
        """Configure or query the scene with camera follow.

        Example:
            scene.camera_follow(target)
        """
        ...
    def camera_shake(self, amplitude: float = 12.0, frequency: float = 8.0, duration: float = 0.5) -> None:
        """Configure or query the scene with camera shake.

        Example:
            scene.camera_shake()
        """
        ...
    def group(self, members: Sequence[Drawable]) -> Drawable:
        """Create a group drawable in the scene.

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
    def bar_chart(
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
    def segment(self, name: str, transition: Optional[Transition] = None) -> int:
        """Schedule segment on the scene timeline.

        Example:
            result = scene.segment("example")
        """
        ...
    def link(self, from_: int, to: int, transition: Transition) -> None:
        """Schedule link on the scene timeline.

        Example:
            scene.link((0.0, 0.0), (0.0, 0.0), Transition.cut())
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
    def play(self, anims: Sequence[Anim], lag: Optional[float] = None) -> None:
        """Schedule play on the scene timeline.

        Example:
            scene.play([animation])
        """
        ...
    def slide(
        self,
        name: str,
        *,
        notes: Optional[str] = None,
        layout: Literal[
            "blank", "title", "cover", "title_content", "content", "agenda",
            "two_columns", "comparison", "section", "divider", "closing", "conclusion"
        ] = "blank",
    ) -> Slide:
        """Schedule slide on the scene timeline.

        Example:
            result = scene.slide("example")
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
        slide: Optional[str] = None,
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
    # Reactive objects
    def value_tracker(self, initial: float) -> ValueTracker:
        """Configure or query the scene with value tracker.

        Example:
            result = scene.value_tracker(1.0)
        """
        ...
    def point_on_curve(self, curve: Drawable, tracker: ValueTracker) -> Drawable:
        """Create a point on curve drawable in the scene.

        Example:
            result = scene.point_on_curve(curve, None)
        """
        ...
    def tangent_on_curve(self, curve: Drawable, tracker: ValueTracker, length: float = 80.0) -> Drawable:
        """Create a tangent on curve drawable in the scene.

        Example:
            result = scene.tangent_on_curve(curve, None)
        """
        ...
    def normal_on_curve(self, curve: Drawable, tracker: ValueTracker, length: float = 80.0) -> Drawable:
        """Create a normal on curve drawable in the scene.

        Example:
            result = scene.normal_on_curve(curve, None)
        """
        ...
    def curvature_on_curve(self, curve: Drawable, tracker: ValueTracker, window: float = 0.02) -> Drawable:
        """Create a curvature on curve drawable in the scene.

        Example:
            result = scene.curvature_on_curve(curve, None)
        """
        ...
    def always_redraw_arc(
        self,
        tracker: ValueTracker,
        cx: float,
        cy: float,
        radius: float,
        start_angle: float,
        sweep_scale: float = 1.0,
        sweep_offset: float = 0.0,
    ) -> Drawable:
        """Create a always redraw arc drawable in the scene.

        Example:
            result = scene.always_redraw_arc(None, 1.0, 1.0, 40.0, 1.0)
        """
        ...
    def traced_path(self, source: Drawable) -> Drawable:
        """Create a traced path drawable in the scene.

        Example:
            result = scene.traced_path(source)
        """
        ...
    def traced_path_3d(
        self,
        source: Drawable,
        *,
        colormap: Optional[str] = None,
        max_points: Optional[int] = None,
        min_distance: float = 0.1,
    ) -> Drawable:
        """Trace a moving drawable's 3D world-space position.

        ``max_points`` limits retained samples. ``min_distance`` ignores
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
        from_: Drawable | tuple[float, float],
        to: Drawable | tuple[float, float],
    ) -> Drawable:
        """Create a tracking line drawable in the scene.

        Example:
            result = scene.tracking_line(drawable, drawable)
        """
        ...
    def spring_between(
        self,
        from_: Drawable | tuple[float, float],
        to: Drawable | tuple[float, float],
        coils: int = 8,
        amplitude: float = 12.0,
    ) -> Drawable:
        """Create a reactive spring between two endpoints.

        Example:
            spring = scene.spring_between((0, 0), drawable)
        """
        ...
    def dimension_between(
        self,
        from_: Drawable | tuple[float, float],
        to: Drawable | tuple[float, float],
        offset: float,
    ) -> Drawable:
        """Create a dimension between drawable in the scene.

        Example:
            result = scene.dimension_between(drawable, drawable, 1.0)
        """
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
