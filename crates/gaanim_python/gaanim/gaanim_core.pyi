from typing import ClassVar, Literal, Optional, overload

_RateFuncName = Literal[
    "linear",
    "smooth",
    "double_smooth",
    "lingering",
    "running_start",
    "spring",
    "spring_soft",
    "spring_bouncy",
    "ease_in",
    "ease_out",
    "ease_in_out",
    "back_in",
    "back_out",
    "back_in_out",
    "bounce_in",
    "bounce_out",
    "bounce_in_out",
    "elastic_in",
    "elastic_out",
    "elastic_in_out",
    "there_and_back",
    "there_and_back_with_pause",
    "exponential_decay",
    "not_quite_there",
]

_Direction = Literal["up", "down", "left", "right"]
_DirectionOrEdge = Literal["up", "down", "left", "right", "top", "bottom"]
_TextRole = Literal["title", "subtitle", "body", "caption", "code"]

class Scene:
    """Entry point for authoring a Gaanim scene.

    Holds a deferred op queue. Every call to a shape/play method records
    an operation; the actual scene is built only when ``render()`` is called,
    at which point the queue is drained into a Bevy ECS app and the Vello GPU
    window is shown.

    Usage::

        scene = Scene(width=1280, height=720, title="Demo")
        circle = scene.circle(80).fill(BLUE).at(100, 0)
        scene.play(circle.animate().scale(1.5).spring())
        scene.render()
    """

    def __init__(
        self,
        width: int = 1280,
        height: int = 720,
        title: Optional[str] = None,
        theme: Optional[Theme] = None,
    ) -> None:
        """Create a new scene.

        Args:
            width: Window width in pixels.
            height: Window height in pixels.
            title: Window title string.
            theme: Color theme (defaults to Theme.DARK).
        """

    @property
    def theme(self) -> Theme:
        """The active color theme for this scene."""

    @property
    def width(self) -> int:
        """Window width in pixels."""

    @property
    def height(self) -> int:
        """Window height in pixels."""

    @property
    def title_str(self) -> str:
        """Window title string."""

    def set_theme(self, theme: Theme) -> None:
        """Replace the active color theme.

        Args:
            theme: A Theme instance (e.g., Theme.DARK, Theme.LIGHT).
        """

    def __repr__(self) -> str: ...
    def background(self, color: Color) -> None:
        """Set the scene background color.

        Args:
            color: Fill color for the background.
        """

    def group(self, children: List[Mobject]) -> Mobject:
        """Create a group from a list of Mobjects.

        Args:
            children: List of Mobjects to group.
        Returns:
            A new Group Mobject.
        """

    def ungroup(self, group: Mobject) -> None:
        """Ungroup the given group and release its children.

        Args:
            group: The Group Mobject to dissolve.
        """

    # ---- shape spawners ----

    def circle(self, radius: float) -> Mobject:
        """Create a circle.

        Args:
            radius: Radius of the circle.
        Returns:
            A new Mobject handle.
        """

    def rectangle(self, width: float, height: float) -> Mobject:
        """Create a rectangle.

        Args:
            width: Width of the rectangle.
            height: Height of the rectangle.
        Returns:
            A new Mobject handle.
        """

    def rounded_rect(self, width: float, height: float, radius: float) -> Mobject:
        """Create a rectangle with rounded corners.

        Args:
            width: Width of the rectangle.
            height: Height of the rectangle.
            radius: Corner radius.
        Returns:
            A new Mobject handle.
        """

    def square(self, side: float) -> Mobject:
        """Create a square.

        Args:
            side: Side length.
        Returns:
            A new Mobject handle.
        """

    def dot(self, radius: float) -> Mobject:
        """Create a small filled circle (dot).

        Args:
            radius: Dot radius.
        Returns:
            A new Mobject handle.
        """

    def ellipse(self, rx: float, ry: float) -> Mobject:
        """Create an ellipse.

        Args:
            rx: Horizontal radius.
            ry: Vertical radius.
        Returns:
            A new Mobject handle.
        """

    def line(self, x1: float, y1: float, x2: float, y2: float) -> Mobject:
        """Create a line segment.

        Args:
            x1: Start point X.
            y1: Start point Y.
            x2: End point X.
            y2: End point Y.
        Returns:
            A new Mobject handle.
        """

    def arrow(self, x1: float, y1: float, x2: float, y2: float) -> Mobject:
        """Create an arrow (filled shaft from start to end).

        Args:
            x1: Start point X.
            y1: Start point Y.
            x2: End point X.
            y2: End point Y.
        Returns:
            A new Mobject handle.
        """

    def polygon(self, points: list[tuple[float, float]]) -> Mobject:
        """Create a polygon from a list of vertices.

        Args:
            points: List of (x, y) vertices.
        Returns:
            A new Mobject handle.
        """

    def star(self, n_points: int, outer_radius: float, inner_radius: float) -> Mobject:
        """Create a star shape.

        Args:
            n_points: Number of star points.
            outer_radius: Distance from center to outer vertices.
            inner_radius: Distance from center to inner vertices.
        Returns:
            A new Mobject handle.
        """

    def checkmark(self, size: float) -> Mobject:
        """Create a checkmark (✓) path.

        Args:
            size: Overall size (bounding box).
        Returns:
            A new Mobject handle.
        """

    def regular_polygon(self, n_sides: int, radius: float) -> Mobject:
        """Create a regular polygon.

        Args:
            n_sides: Number of sides (e.g., 3 = triangle, 5 = pentagon).
            radius: Distance from center to vertices.
        Returns:
            A new Mobject handle.
        """

    def text(self, content: str, role: Optional[_TextRole] = None) -> Mobject:
        """Create a text mobject with semantic role styling.

        Args:
            content: The text string.
            role: Semantic role (``"title"``, ``"subtitle"``, ``"body"``,
                ``"caption"``, ``"code"``). Defaults to ``"body"``.
        Returns:
            A new Mobject handle.
        """

    def title(self, content: str) -> Mobject:
        """Create a title text mobject (large, prominent).

        Args:
            content: The title string.
        Returns:
            A new Mobject handle.
        """

    def subtitle(self, content: str) -> Mobject:
        """Create a subtitle text mobject.

        Args:
            content: The subtitle string.
        Returns:
            A new Mobject handle.
        """

    def body(self, content: str) -> Mobject:
        """Create a body text mobject (normal paragraph).

        Args:
            content: The text string.
        Returns:
            A new Mobject handle.
        """

    def caption(self, content: str) -> Mobject:
        """Create a caption text mobject (small, subtle).

        Args:
            content: The caption string.
        Returns:
            A new Mobject handle.
        """

    def equation(self, formula: str) -> Mobject:
        """Create a mathematical equation via Typst compilation.

        Supports Typst math syntax, e.g. ``"E = m c^2"``,
        ``"sum_(i=1)^n i = frac(n(n+1), 2)"``.

        Args:
            formula: Typst math expression.
        Returns:
            A new Mobject handle.
        """

    def dashed_line(
        self,
        x1: float,
        y1: float,
        x2: float,
        y2: float,
        dash_length: float,
        gap_length: float,
    ) -> Mobject:
        """Create a dashed line segment.

        Args:
            x1: Start point X.
            y1: Start point Y.
            x2: End point X.
            y2: End point Y.
            dash_length: Length of each dash.
            gap_length: Length of each gap.
        Returns:
            A new Mobject handle.
        """

    def arc(
        self,
        cx: float,
        cy: float,
        rx: float,
        ry: float,
        start_angle: float,
        sweep_angle: float,
    ) -> Mobject:
        """Create an elliptical arc.

        Args:
            cx: Center X.
            cy: Center Y.
            rx: Horizontal radius.
            ry: Vertical radius.
            start_angle: Starting angle in radians.
            sweep_angle: Angular sweep in radians (positive = CCW).
        Returns:
            A new Mobject handle.
        """

    def arc_between_points(
        self,
        x1: float,
        y1: float,
        x2: float,
        y2: float,
        angle: float,
    ) -> Mobject:
        """Create an arc connecting two points with a given angular span.

        Args:
            x1: Start point X.
            y1: Start point Y.
            x2: End point X.
            y2: End point Y.
            angle: Desired angular span in radians (positive = CCW arc).
        Returns:
            A new Mobject handle.
        """

    def double_arrow(
        self,
        x1: float,
        y1: float,
        x2: float,
        y2: float,
        head_len: Optional[float] = None,
        head_width: Optional[float] = None,
    ) -> Mobject:
        """Create a double-headed arrow (arrowheads on both ends).

        Args:
            x1: Start point X.
            y1: Start point Y.
            x2: End point X.
            y2: End point Y.
            head_len: Arrowhead length (default: auto).
            head_width: Arrowhead width (default: auto).
        Returns:
            A new Mobject handle.
        """

    def sector(
        self,
        cx: float,
        cy: float,
        radius: float,
        start_angle: float,
        sweep_angle: float,
    ) -> Mobject:
        """Create a circular sector (pie slice).

        Args:
            cx: Center X.
            cy: Center Y.
            radius: Circle radius.
            start_angle: Starting angle in radians.
            sweep_angle: Angular sweep in radians.
        Returns:
            A new Mobject handle.
        """

    def annulus(self, outer_radius: float, inner_radius: float) -> Mobject:
        """Create a ring (annulus).

        Args:
            outer_radius: Outer circle radius.
            inner_radius: Inner circle radius.
        Returns:
            A new Mobject handle.
        """

    def surrounding_rectangle(
        self, width: float, height: float, corner_radius: float
    ) -> Mobject:
        """Create a rectangle with rounded corners (no fill, stroked).

        Intended as a highlight/surround box.

        Args:
            width: Rectangle width.
            height: Rectangle height.
            corner_radius: Corner radius.
        Returns:
            A new Mobject handle.
        """

    def background_rectangle(self, width: float, height: float) -> Mobject:
        """Create a filled background rectangle (low z-index).

        Args:
            width: Rectangle width.
            height: Rectangle height.
        Returns:
            A new Mobject handle.
        """

    def cross(self, size: float) -> Mobject:
        """Create a cross (X) shape from two diagonal lines.

        Args:
            size: Overall size.
        Returns:
            A new Mobject handle.
        """

    def right_angle(self, arm_length: float) -> Mobject:
        """Create a right-angle (L) shape.

        Args:
            arm_length: Length of each arm.
        Returns:
            A new Mobject handle.
        """

    def union(self, a: Mobject, b: Mobject) -> Mobject:
        """Boolean union of two mobject geometries.

        The source mobjects are left untouched.

        Args:
            a: First shape.
            b: Second shape.
        Returns:
            A new Mobject representing the union area.
        """

    def intersection(self, a: Mobject, b: Mobject) -> Mobject:
        """Boolean intersection of two mobject geometries.

        Args:
            a: First shape.
            b: Second shape.
        Returns:
            A new Mobject representing the overlapping area.
        """

    def difference(self, a: Mobject, b: Mobject) -> Mobject:
        """Boolean difference: area of A excluding the overlap with B.

        Args:
            a: First shape (source).
            b: Second shape (subtracted).
        Returns:
            A new Mobject representing the difference.
        """

    def exclusion(self, a: Mobject, b: Mobject) -> Mobject:
        """Boolean exclusion (XOR): area in A or B but not both.

        Args:
            a: First shape.
            b: Second shape.
        Returns:
            A new Mobject representing the symmetric difference.
        """

    # ---- timeline ----

    @overload
    def play(self, *anims: AnimSpec) -> None: ...
    @overload
    def play(self, *anims: None) -> None: ...
    def play(self, *anims: AnimSpec | None) -> None:
        """Enqueue one or more animations to play in parallel.

        ``None`` entries are silently skipped (useful for conditional
        animation slots).

        The timeline cursor advances by the maximum duration among all
        animations in the batch.

        Args:
            *anims: One or more AnimSpec objects from ``mob.animate()``,
                ``mob.shift_anim(dx, dy)``, ``selection_anim(...).build(s)``, etc.
        """

    def wait(self, duration: float) -> None:
        """Pause the timeline for a given duration (in seconds).

        Args:
            duration: Pause length in seconds.
        """

    def slide(self) -> None:
        """Insert a slide breakpoint marker in the timeline.

        This does not advance time; it is a hint for future slide-based
        rendering.
        """

    # ---- selection ----

    def select(self, parent: Mobject, query: str) -> Selection:
        """Create a selection handle for glyph-level access inside text.

        The query is matched against the parent mobject's text content.
        Use ``fill_selection``, ``set_stroke_selection``, or
        ``selection_anim`` to operate on the matched glyphs.

        Args:
            parent: The text or equation mobject to search within.
            query: Substring to match (exact character-range matching).
        Returns:
            A Selection handle.
        """

    def fill_selection(self, selection: Selection, color: Color) -> None:
        """Apply a fill color to all glyphs in a selection.

        Args:
            selection: Selection handle from ``scene.select()``.
            color: Fill color.
        """

    def set_stroke_selection(
        self, selection: Selection, color: Color, width: float
    ) -> None:
        """Apply a stroke to all glyphs in a selection.

        Args:
            selection: Selection handle from ``scene.select()``.
            color: Stroke color.
            width: Stroke width.
        """

    def selection_anim(
        self, selection: Selection, dx: float, dy: float
    ) -> SelectionAnim:
        """Create a coordinated shift animation over a selection of glyphs.

        Chain ``.duration(d)`` / ``.spring()`` / ``.smooth()`` then call
        ``.build(scene)`` to produce an AnimSpec for ``play()``.

        Args:
            selection: Selection handle.
            dx: Horizontal shift.
            dy: Vertical shift.
        Returns:
            A SelectionAnim builder.
        """

    # ---- rendering ----

    def render(self) -> None:
        """Drain the deferred op queue, build the scene, and show the GPU window.

        Blocking: returns when the window is closed. Releases the GIL while
        Bevy drives the Vello renderer.
        """

    def edit(self) -> None:
        """Drain the deferred op queue and open the interactive editor window.

        The editor adds inspector, hierarchy, and playback control panels
        over the Vello viewport. Click objects to inspect them. Blocking:
        returns when the window is closed.
        """

    def export(
        self,
        output_path: str,
        fps: int = 60,
        width: int | None = None,
        height: int | None = None,
        transparent: bool | None = None,
        aspect_ratio: str | None = None,
        quality: str | None = None,
        start_time: float | None = None,
        end_time: float | None = None,
    ) -> None:
        """Render and export the scene to a video, image sequence, or GIF.

        Pipes frames asynchronously to an optimized background FFmpeg encoder.
        Supports aspect ratio presets ('youtube', 'tiktok', 'instagram'),
        quality presets ('draft', 'standard', 'production'), and transparent WebM layers.
        """

class Mobject:
    """Handle to a scene mobject (shape, text, or equation).

    All setters return a **new handle** that shares the underlying spec
    with the original — mutations propagate to the deferred op queue
    and are resolved at render time.

    Usage::

        circle = scene.circle(80).fill(BLUE).at(100, 0).z_index(1)
        anim = circle.animate().scale(1.5).duration(2.0).spring()
        scene.play(anim)
    """

    @property
    def id(self) -> ObjectId:
        """The stable ObjectId assigned at creation time."""

    def __repr__(self) -> str: ...

    def __getitem__(self, index: int) -> Mobject:
        """Get the child Mobject at the given index if this is a Group.

        Args:
            index: The 0-based index.
        Returns:
            The child Mobject handle.
        """

    def __len__(self) -> int:
        """Get the number of children in this group.

        Returns:
            The number of children, or 0 if not a Group.
        """

    # ---- display configuration ----

    def fill(self, color: Color) -> Mobject:
        """Set the fill color.

        Args:
            color: Fill color.
        Returns:
            A new handle (same underlying mobject).
        """

    def no_fill(self) -> Mobject:
        """Remove the fill, making the interior transparent.

        Returns:
            A new handle.
        """

    def stroke(self, color: Color, width: float) -> Mobject:
        """Set the stroke (outline) color and width.

        Args:
            color: Stroke color.
            width: Stroke width in pixels.
        Returns:
            A new handle.
        """

    def no_stroke(self) -> Mobject:
        """Remove the stroke (outline).

        Returns:
            A new handle.
        """

    def opacity(self, opacity: float) -> Mobject:
        """Set the opacity (0.0 = fully transparent, 1.0 = fully opaque).

        Args:
            opacity: Opacity value in [0, 1].
        Returns:
            A new handle.
        """

    def z_index(self, z: int) -> Mobject:
        """Set the z-index for draw order (higher = on top).

        Args:
            z: Z-index value.
        Returns:
            A new handle.
        """

    # ---- transform ----

    def at(self, x: float, y: float) -> Mobject:
        """Set absolute 2D position (replaces any existing transform).

        Args:
            x: World-space X coordinate.
            y: World-space Y coordinate.
        Returns:
            A new handle.
        """

    def shift(self, dx: float, dy: float) -> Mobject:
        """Add a relative offset to the current position.

        Args:
            dx: Horizontal offset.
            dy: Vertical offset.
        Returns:
            A new handle.
        """

    def scale(self, factor: float) -> Mobject:
        """Scale uniformly by a factor.

        Args:
            factor: Scale factor (1.0 = no change).
        Returns:
            A new handle.
        """

    def rotate(self, radians: float) -> Mobject:
        """Rotate by a given angle (in radians).

        Args:
            radians: Rotation angle in radians.
        Returns:
            A new handle.
        """

    def next_to(
        self, reference: Mobject, direction: _Direction, spacing: float = 10.0
    ) -> Mobject:
        """Place this mobject adjacent to a reference mobject.

        The relative position is computed at spawn time based on the
        reference's bounds.

        Args:
            reference: The mobject to position relative to.
            direction: One of ``"up"``, ``"down"``, ``"left"``, ``"right"``.
            spacing: Gap in pixels between the two mobjects.
        Returns:
            A new handle.
        """

    # ---- animation constructors ----

    def animate(self) -> AnimSpec:
        """Begin a fluent animation spec for this mobject.

        Chain kind/parameters, duration, and rate function before passing
        to ``Scene.play(*anims)``::

            mob.animate().scale(1.5).duration(2.0).spring()

        Returns:
            An AnimSpec builder (defaults: shift(0,0), 1.0s, smooth).
        """

    def shift_anim(self, dx: float, dy: float) -> AnimSpec:
        """Create an animation that shifts this mobject by (dx, dy).

        Args:
            dx: Horizontal offset.
            dy: Vertical offset.
        Returns:
            An AnimSpec with duration=1.0, rate=smooth.
        """

    def translate_to_anim(self, x: float, y: float) -> AnimSpec:
        """Create an animation that moves this mobject to absolute position (x, y).

        Args:
            x: Target X.
            y: Target Y.
        Returns:
            An AnimSpec.
        """

    def scale_anim(self, factor: float) -> AnimSpec:
        """Create a uniform-scale animation.

        Args:
            factor: Target scale factor.
        Returns:
            An AnimSpec.
        """

    def rotate_anim(self, radians: float) -> AnimSpec:
        """Create a rotation animation.

        Args:
            radians: Rotation angle in radians.
        Returns:
            An AnimSpec.
        """

    def fade_in_anim(self) -> AnimSpec:
        """Create a fade-in animation (opacity 0→1).

        Returns:
            An AnimSpec.
        """

    def fade_out_anim(self) -> AnimSpec:
        """Create a fade-out animation (opacity 1→0).

        Returns:
            An AnimSpec.
        """

    def fade_to_anim(self, opacity: float) -> AnimSpec:
        """Create a fade-to-opacity animation.

        Args:
            opacity: Target opacity in [0, 1].
        Returns:
            An AnimSpec.
        """

    def fill_color_anim(self, color: Color) -> AnimSpec:
        """Create a fill-color transition animation.

        Args:
            color: Target fill color.
        Returns:
            An AnimSpec.
        """

    def stroke_color_anim(self, color: Color) -> AnimSpec:
        """Create a stroke-color transition animation.

        Args:
            color: Target stroke color.
        Returns:
            An AnimSpec.
        """

    # ---- complex animations ----

    def write(
        self, duration: float = 1.0, stroke_width: Optional[float] = None
    ) -> AnimSpec:
        """Manim-style Write: stroke draws progressively, then fill fades in.

        Elements are staggered for a hand-drawn effect.

        Args:
            duration: Total animation duration in seconds.
            stroke_width: Stroke width during draw phase (default: auto).
        Returns:
            An AnimSpec.
        """

    def create(
        self, duration: float = 1.0, stroke_width: Optional[float] = None
    ) -> AnimSpec:
        """Progressive draw animation in parallel (no stagger).

        All path elements draw simultaneously.

        Args:
            duration: Total duration in seconds.
            stroke_width: Stroke width during draw phase.
        Returns:
            An AnimSpec.
        """

    def uncreate(
        self, duration: float = 1.0, stroke_width: Optional[float] = None
    ) -> AnimSpec:
        """Progressive erasure of path(s) and fill in parallel.

        Inverse of ``create``.

        Args:
            duration: Total duration in seconds.
            stroke_width: Stroke width.
        Returns:
            An AnimSpec.
        """

    def unwrite(
        self, duration: float = 1.0, stroke_width: Optional[float] = None
    ) -> AnimSpec:
        """Staggered sequential erasure in reverse order.

        Inverse of ``write``.

        Args:
            duration: Total duration in seconds.
            stroke_width: Stroke width.
        Returns:
            An AnimSpec.
        """

    def grow_from_center(self) -> AnimSpec:
        """Scale up from 0 to full size, centered at local position.

        Returns:
            An AnimSpec.
        """

    def shrink_to_center(self) -> AnimSpec:
        """Scale down from full size to 0, centered at local position.

        Returns:
            An AnimSpec.
        """

    def spin_in_from_nothing(self) -> AnimSpec:
        """Scale up from 0 and rotate 360° concurrently.

        Returns:
            An AnimSpec.
        """

    def indicate(
        self,
        color: Optional[Color] = None,
        scale_factor: float = 1.25,
    ) -> AnimSpec:
        """Temporarily scale up and highlight, then return to baseline.

        Useful for drawing attention to an object.

        Args:
            color: Highlight color (default: accent from theme).
            scale_factor: Peak scale factor.
        Returns:
            An AnimSpec.
        """

    def fade_transform(self, target: Mobject) -> AnimSpec:
        """Fade out this mobject while fading in another concurrently.

        Args:
            target: The mobject to fade into.
        Returns:
            An AnimSpec.
        """

    def wiggle(self) -> AnimSpec:
        """Oscillating horizontal wiggle animation.

        Returns:
            An AnimSpec.
        """

    def grow_from_point(self, px: float, py: float) -> AnimSpec:
        """Scale from zero at a specific anchor point, growing outward.

        Args:
            px: Anchor X.
            py: Anchor Y.
        Returns:
            An AnimSpec.
        """

    def grow_from_edge(self, direction: _DirectionOrEdge) -> AnimSpec:
        """Scale from zero originating from a specific edge.

        Args:
            direction: One of ``"up"``, ``"down"``, ``"left"``, ``"right"``,
                ``"top"``, ``"bottom"``.
        Returns:
            An AnimSpec.
        """

    def draw_border_then_fill(self) -> AnimSpec:
        """Draw the outline first, then fill the shape.

        A two-phase sequential animation.

        Returns:
            An AnimSpec.
        """

    def flash(
        self,
        color: Optional[Color] = None,
        n_lines: int = 12,
        radius: float = 100.0,
    ) -> AnimSpec:
        """Lines radiating outward from the object (flash-of-insight effect).

        Args:
            color: Line color (default: accent from theme).
            n_lines: Number of radiant lines.
            radius: Line length in pixels.
        Returns:
            An AnimSpec.
        """

    def circumscribe(self, color: Optional[Color] = None) -> AnimSpec:
        """Highlight with a circumscribing shape that grows and fades.

        Args:
            color: Outline color (default: accent from theme).
        Returns:
            An AnimSpec.
        """

    def move_along_path(self, waypoints: list[tuple[float, float]], duration: float = 0.0) -> AnimSpec:
        """Move the target along a polyline defined by a list of waypoints.

        Adjacent waypoints are connected by line segments. The path is sampled
        at the rate-function-eased ``t`` (parametric, not arc-length uniform)
        and applied as the target's world-space translation.

        Args:
            waypoints: List of ``(x, y)`` tuples defining the trajectory.
                Must contain at least one point.
            duration: Duration in seconds. If ``<= 0``, defaults to 2.0s.
        Returns:
            An AnimSpec.
        """

    def grow_arrow(self, duration: float = 0.0) -> AnimSpec:
        """Specialized draw animation for ``Arrow`` mobjects.

        Traces the outline (70% of duration) then cross-fades the fill with a
        brief scale "punch" that emphasizes the arrowhead's arrival.

        Args:
            duration: Duration in seconds. If ``<= 0``, defaults to 1.5s.
        Returns:
            An AnimSpec.
        """

class AnimSpec:
    """A configured animation targeting a single Mobject.

    Built fluently from ``Mobject.animate()`` or shorthand methods.
    Pass to ``Scene.play(*specs)`` to enqueue on the timeline.

    Usage::

        anim = (mob.animate()
                .scale(1.5)
                .duration(2.0)
                .spring())
        scene.play(anim)
    """

    @property
    def target(self) -> ObjectId:
        """The ObjectId of the mobject this animation targets."""

    @property
    def duration_val(self) -> float:
        """The animation duration in seconds."""

    @property
    def rate_func_name(self) -> str:
        """Name of the current rate function (e.g. ``"smooth"``, ``"spring"``)."""

    def __repr__(self) -> str: ...

    # ---- timing ----

    def duration(self, d: float) -> AnimSpec:
        """Set the animation duration in seconds.

        Args:
            d: Duration in seconds.
        Returns:
            A new AnimSpec with the updated duration.
        """

    def rate_func(self, name: _RateFuncName) -> AnimSpec:
        """Set the rate function (easing curve) by name.

        Args:
            name: Rate function name (e.g. ``"smooth"``, ``"spring"``).
        Returns:
            A new AnimSpec.
        """

    def spring(self) -> AnimSpec:
        """Use a spring overshoot easing (stiffness=90, damping=12).

        Returns:
            A new AnimSpec.
        """

    def smooth(self) -> AnimSpec:
        """Use the default smooth easing (sinusoidal in-out).

        Returns:
            A new AnimSpec.
        """

    def linear(self) -> AnimSpec:
        """Use a constant-speed (linear) easing.

        Returns:
            A new AnimSpec.
        """

    def steps(self, n: int) -> AnimSpec:
        """Discrete step interpolation: clamps animation to n levels.

        Args:
            n: Number of discrete steps.
        Returns:
            A new AnimSpec.
        """

    def cubic_bezier(self, x1: float, y1: float, x2: float, y2: float) -> AnimSpec:
        """CSS-style cubic-bezier easing curve.

        Args:
            x1: First control point X.
            y1: First control point Y.
            x2: Second control point X.
            y2: Second control point Y.
        Returns:
            A new AnimSpec.
        """

    def mirror(self, inner_name: _RateFuncName) -> AnimSpec:
        """Mirror a named rate function (go to peak then back symmetrically).

        Args:
            inner_name: Name of the base rate function.
        Returns:
            A new AnimSpec.
        """

    def there_and_back_with_pause(self, pause_ratio: float) -> AnimSpec:
        """Animate forward, pause, then animate back.

        Args:
            pause_ratio: Fraction of duration spent at peak (0.0–0.9).
        Returns:
            A new AnimSpec.
        """

    # ---- animation kind (overrides previous kind) ----

    def shift(self, dx: float, dy: float) -> AnimSpec:
        """Change animation kind to a translation by (dx, dy).

        Args:
            dx: Horizontal offset.
            dy: Vertical offset.
        Returns:
            A new AnimSpec.
        """

    def translate_to(self, x: float, y: float) -> AnimSpec:
        """Change animation kind to a translation to absolute position (x, y).

        Args:
            x: Target X.
            y: Target Y.
        Returns:
            A new AnimSpec.
        """

    def scale(self, factor: float) -> AnimSpec:
        """Change animation kind to a uniform scale.

        Args:
            factor: Scale factor.
        Returns:
            A new AnimSpec.
        """

    def scale_to(self, factor: float) -> AnimSpec:
        """Change animation kind to a uniform scale to a target factor.

        Args:
            factor: Target scale factor.
        Returns:
            A new AnimSpec.
        """

    def rotate(self, radians: float) -> AnimSpec:
        """Change animation kind to a rotation by a given angle.

        Args:
            radians: Rotation angle in radians.
        Returns:
            A new AnimSpec.
        """

    def rotate_to(self, radians: float) -> AnimSpec:
        """Change animation kind to a rotation to an absolute angle.

        Args:
            radians: Target angle in radians.
        Returns:
            A new AnimSpec.
        """

    def fade_in(self) -> AnimSpec:
        """Change animation kind to fade-in (opacity 0→1).

        Returns:
            A new AnimSpec.
        """

    def fade_out(self) -> AnimSpec:
        """Change animation kind to fade-out (opacity 1→0).

        Returns:
            A new AnimSpec.
        """

    def fade_to(self, opacity: float) -> AnimSpec:
        """Change animation kind to a fade to target opacity.

        Args:
            opacity: Target opacity in [0, 1].
        Returns:
            A new AnimSpec.
        """

    def fill_color(self, color: Color) -> AnimSpec:
        """Change animation kind to a fill color transition.

        Args:
            color: Target fill color.
        Returns:
            A new AnimSpec.
        """

    def stroke_color(self, color: Color) -> AnimSpec:
        """Change animation kind to a stroke color transition.

        Args:
            color: Target stroke color.
        Returns:
            A new AnimSpec.
        """

    def stroke_width(self, width: float) -> AnimSpec:
        """Change animation kind to a stroke width transition.

        Args:
            width: Target stroke width.
        Returns:
            A new AnimSpec.
        """

    # ---- complex animations ----

    def write(
        self, duration: float = 1.0, stroke_width: Optional[float] = None
    ) -> AnimSpec:
        """Change animation kind to a Write (progressive draw + staggered fill).

        Args:
            duration: Total duration in seconds.
            stroke_width: Width during draw phase.
        Returns:
            A new AnimSpec.
        """

    def create(
        self, duration: float = 1.0, stroke_width: Optional[float] = None
    ) -> AnimSpec:
        """Change animation kind to a parallel Create (progressive draw).

        Args:
            duration: Total duration in seconds.
            stroke_width: Width during draw phase.
        Returns:
            A new AnimSpec.
        """

    def uncreate(
        self, duration: float = 1.0, stroke_width: Optional[float] = None
    ) -> AnimSpec:
        """Change animation kind to a parallel Uncreate (progressive erase).

        Args:
            duration: Total duration in seconds.
            stroke_width: Stroke width.
        Returns:
            A new AnimSpec.
        """

    def unwrite(
        self, duration: float = 1.0, stroke_width: Optional[float] = None
    ) -> AnimSpec:
        """Change animation kind to a staggered Unwrite (reverse of Write).

        Args:
            duration: Total duration in seconds.
            stroke_width: Stroke width.
        Returns:
            A new AnimSpec.
        """

    def grow_from_center(self) -> AnimSpec:
        """Change animation kind to grow-from-center (scale 0→1).

        Returns:
            A new AnimSpec.
        """

    def shrink_to_center(self) -> AnimSpec:
        """Change animation kind to shrink-to-center (scale 1→0).

        Returns:
            A new AnimSpec.
        """

    def spin_in_from_nothing(self) -> AnimSpec:
        """Change animation kind to spin-in (scale 0→1 + 360° rotation).

        Returns:
            A new AnimSpec.
        """

    def indicate(
        self,
        color: Optional[Color] = None,
        scale_factor: float = 1.25,
    ) -> AnimSpec:
        """Change animation kind to indicate (pulse highlight).

        Args:
            color: Highlight color.
            scale_factor: Peak scale.
        Returns:
            A new AnimSpec.
        """

    def fade_transform(self, target: Mobject) -> AnimSpec:
        """Change animation kind to fade-transform (cross-fade to target).

        Args:
            target: The mobject to transition into.
        Returns:
            A new AnimSpec.
        """

    def wiggle(self) -> AnimSpec:
        """Change animation kind to a horizontal wiggle.

        Returns:
            A new AnimSpec.
        """

    def grow_from_point(self, px: float, py: float) -> AnimSpec:
        """Change animation kind to grow from a specific point.

        Args:
            px: Anchor X.
            py: Anchor Y.
        Returns:
            A new AnimSpec.
        """

    def grow_from_edge(self, direction: _DirectionOrEdge) -> AnimSpec:
        """Change animation kind to grow from an edge.

        Args:
            direction: ``"up"``, ``"down"``, ``"left"``, ``"right"``, etc.
        Returns:
            A new AnimSpec.
        """

    def draw_border_then_fill(self) -> AnimSpec:
        """Change animation kind to draw outline first, then fill.

        Returns:
            A new AnimSpec.
        """

    def flash(
        self,
        color: Optional[Color] = None,
        n_lines: int = 12,
        radius: float = 100.0,
    ) -> AnimSpec:
        """Change animation kind to a radiant flash effect.

        Args:
            color: Line color.
            n_lines: Number of lines.
            radius: Line length.
        Returns:
            A new AnimSpec.
        """

    def circumscribe(self, color: Optional[Color] = None) -> AnimSpec:
        """Change animation kind to a circumscribing highlight shape.

        Args:
            color: Outline color.
        Returns:
            A new AnimSpec.
        """

    def move_along_path(self, waypoints: list[tuple[float, float]], duration: float = 0.0) -> AnimSpec:
        """Move the target along a polyline defined by a list of waypoints.

        Adjacent waypoints are connected by line segments. The path is sampled
        at the rate-function-eased ``t`` (parametric, not arc-length uniform)
        and applied as the target's world-space translation.

        Args:
            waypoints: List of ``(x, y)`` tuples defining the trajectory.
                Must contain at least one point.
            duration: Duration in seconds. If ``<= 0``, defaults to 2.0s.
        Returns:
            A new AnimSpec.
        """

    def grow_arrow(self, duration: float = 0.0) -> AnimSpec:
        """Specialized draw animation for ``Arrow`` mobjects.

        Traces the outline (70% of duration) then cross-fades the fill with a
        brief scale "punch" that emphasizes the arrowhead's arrival.

        Args:
            duration: Duration in seconds. If ``<= 0``, defaults to 1.5s.
        Returns:
            A new AnimSpec.
        """

class Color:
    """An RGBA color backed by peniko (8-bit per channel).

    Construct via ``Color(r, g, b, a?)``, ``Color.from_hex("#RRGGBB")``,
    or use one of the named constants (``BLUE``, ``GOLD``, …).

    Usage::

        c = Color(255, 0, 0)          # fully opaque red
        c = Color.from_hex("#FFD700")  # gold
    """

    @overload
    def __init__(self, r: int, g: int, b: int) -> None: ...
    @overload
    def __init__(self, r: int, g: int, b: int, a: int) -> None: ...
    def __init__(self, r: int, g: int, b: int, a: Optional[int] = None) -> None:
        """Create a color from 8-bit RGBA components.

        Args:
            r: Red channel (0–255).
            g: Green channel (0–255).
            b: Blue channel (0–255).
            a: Alpha channel (0–255, default: 255 = opaque).
        """

    @staticmethod
    def from_hex(s: str) -> Color:
        """Parse a hex color string.

        Supports ``#RGB``, ``#RRGGBB``, ``#RRGGBBAA`` (``#`` optional).

        Args:
            s: Hex color string.
        Returns:
            A Color instance.
        """

    @staticmethod
    def from_rgb(r: int, g: int, b: int) -> Color:
        """Create an opaque color from 8-bit RGB values.

        Args:
            r: Red channel (0–255).
            g: Green channel (0–255).
            b: Blue channel (0–255).
        Returns:
            A Color instance.
        """

    @staticmethod
    def from_rgba(r: int, g: int, b: int, a: int) -> Color:
        """Create a color from 8-bit RGBA values.

        Args:
            r: Red channel (0–255).
            g: Green channel (0–255).
            b: Blue channel (0–255).
            a: Alpha channel (0–255).
        Returns:
            A Color instance.
        """

    @property
    def r(self) -> int:
        """Red channel value (0–255)."""

    @property
    def g(self) -> int:
        """Green channel value (0–255)."""

    @property
    def b(self) -> int:
        """Blue channel value (0–255)."""

    @property
    def a(self) -> int:
        """Alpha channel value (0–255)."""

    def __repr__(self) -> str: ...
    def __eq__(self, other: object) -> bool: ...

class ObjectId:
    """Stable handle to a Mobject allocated by the scene.

    Contains an index and generation for safe reuse after deletion.
    """

    def __init__(self, index: int, generation: int) -> None:
        """Create an ObjectId from raw parts.

        Args:
            index: Unique index.
            generation: Generation counter (incremented on reuse).
        """

    @property
    def index(self) -> int:
        """The index part of this ID."""

    @property
    def generation(self) -> int:
        """The generation part of this ID."""

    def __repr__(self) -> str: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

class Selection:
    """A handle to a sub-selection of glyphs inside a Text or Equation.

    Created via ``Scene.select(parent, query)``. Use ``Scene.fill_selection``,
    ``Scene.set_stroke_selection``, or ``Scene.selection_anim`` to operate on
    the matched glyphs.
    """

    @property
    def parent(self) -> ObjectId:
        """The ObjectId of the parent mobject."""

    @property
    def query(self) -> str:
        """The substring query used to create this selection."""

    def __repr__(self) -> str: ...

class SelectionAnim:
    """Fluent builder for coordinated selection animations.

    Created via ``Scene.selection_anim(sel, dx, dy)``.
    Chain timing modifiers and call ``.build(scene)`` to get an ``AnimSpec``::

        anim = scene.selection_anim(sel, 0, 30).duration(1.5).spring().build(scene)
        scene.play(anim)
    """

    @property
    def duration_val(self) -> float:
        """The current animation duration in seconds."""

    def __repr__(self) -> str: ...
    def duration(self, d: float) -> SelectionAnim:
        """Set the animation duration.

        Args:
            d: Duration in seconds.
        Returns:
            A new SelectionAnim builder.
        """

    def spring(self) -> SelectionAnim:
        """Use spring overshoot easing.

        Returns:
            A new SelectionAnim builder.
        """

    def smooth(self) -> SelectionAnim:
        """Use smooth sinusoidal easing.

        Returns:
            A new SelectionAnim builder.
        """

    def linear(self) -> SelectionAnim:
        """Use linear (constant-speed) easing.

        Returns:
            A new SelectionAnim builder.
        """

    def rate_func(self, name: _RateFuncName) -> SelectionAnim:
        """Set the rate function by name.

        Args:
            name: Rate function name (e.g. ``"smooth"``, ``"spring"``).
        Returns:
            A new SelectionAnim builder.
        """

    def build(self, scene: Scene) -> AnimSpec:
        """Finalize the selection animation into an AnimSpec for play().

        Enqueues the internal selection shift op on the scene.

        Args:
            scene: The parent Scene.
        Returns:
            An AnimSpec representing the group animation.
        """

class Theme:
    """A role-based color theme with harmonized palettes.

    Pre-built themes available as class attributes::

        scene = Scene(theme=Theme.DRACULA)
        custom = Theme(background=Color(30, 30, 30), primary=..., ...)

    Attributes:
        DARK: Catppuccin Mocha-inspired dark theme.
        LIGHT: Catppuccin Latte-inspired light theme.
        DRACULA: Classic Dracula dark theme.
        GRUVBOX: Warm retro Gruvbox dark theme.
    """

    DARK: ClassVar[Theme]
    LIGHT: ClassVar[Theme]
    DRACULA: ClassVar[Theme]
    GRUVBOX: ClassVar[Theme]

    def __init__(
        self,
        background: Color,
        primary: Color,
        secondary: Color,
        accent: Color,
        muted: Color,
    ) -> None:
        """Create a custom color theme.

        Args:
            background: Scene background color.
            primary: Primary text/shapes color.
            secondary: Secondary accent color.
            accent: Highlight/accent color.
            muted: Muted/inactive color.
        """

    @property
    def background(self) -> Color:
        """Scene background color."""

    @property
    def primary(self) -> Color:
        """Primary text/shapes color."""

    @property
    def secondary(self) -> Color:
        """Secondary accent color."""

    @property
    def accent(self) -> Color:
        """Highlight/accent color."""

    @property
    def muted(self) -> Color:
        """Muted/inactive color."""

# ---- module-level color constants ----
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
