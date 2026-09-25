"""Check that the shipped Python stub matches the embedded PyO3 surface."""

from __future__ import annotations

import ast
import importlib
import inspect
import math
from pathlib import Path
import sys
import typing


ROOT = Path(__file__).resolve().parents[1]
STUB = ROOT / "crates" / "gaanim_python" / "gaanim" / "gaanim_core.pyi"
TYPE_CHECKING_ONLY = {
    "CurvePoint",
    "CurveControl",
    "CurveCommand",
    "ColorLike",
    "ColorMapLike",
    "Paint",
}
TEXT_API_CLASSES = {
    "TextAnchor",
    "TextStyle",
    "TextFlow",
    "TextSelection",
    "TextQuery",
    "Text",
}
EDITORIAL_SCENE_MEMBERS = {
    "badge",
    "chip",
    "card",
    "banner",
    "lower_third",
    "stat_card",
    "quote_card",
    "section_header",
}
REMOVED_TEXT_SCENE_MEMBERS = {
    "title",
    "subtitle",
    "paragraph",
    "transform_equation",
    "copy_equation_terms",
    "expand_equation",
    "replace_term",
    "step_equation",
    "transform_matching_tex",
    "transform_matching_text",
    "focus_equation",
    "brace_label",
    "annotate_tag",
}
REMOVED_TEXT_DRAWABLE_MEMBERS = {
    "color_by",
    "select",
    "tag",
    "indicate_tag",
    "cancel_term",
    "reveal_fragment",
    "write_by_term",
}


def declared_members(node: ast.ClassDef) -> set[str]:
    members = {
        child.name
        for child in node.body
        if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef))
    }
    members.update(
        child.target.id
        for child in node.body
        if isinstance(child, ast.AnnAssign) and isinstance(child.target, ast.Name)
    )
    return members


def is_type_alias(node: ast.AnnAssign) -> bool:
    """Return whether an annotated assignment exists only for static typing."""
    annotation = node.annotation
    return (
        isinstance(annotation, ast.Name) and annotation.id == "TypeAlias"
    ) or (
        isinstance(annotation, ast.Attribute) and annotation.attr == "TypeAlias"
    )


def documented_text_api_failures(tree: ast.Module) -> list[str]:
    """Require useful docs and runnable examples for the new text callables."""
    failures: list[str] = []
    for node in tree.body:
        callables: list[tuple[str, ast.FunctionDef | ast.AsyncFunctionDef]] = []
        if (
            isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
            and node.name in {"part", "parts"}
        ):
            callables.append((node.name, node))
        elif isinstance(node, ast.ClassDef) and node.name in TEXT_API_CLASSES:
            for child in node.body:
                if not isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    continue
                if child.name.startswith("_") and child.name != "__init__":
                    continue
                if any(
                    isinstance(decorator, ast.Name) and decorator.id in {"property", "overload"}
                    for decorator in child.decorator_list
                ):
                    continue
                callables.append((f"{node.name}.{child.name}", child))
        elif isinstance(node, ast.ClassDef) and node.name == "Typography":
            callables.extend(
                (f"Typography.{child.name}", child)
                for child in node.body
                if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef))
                and child.name in {"__call__", "equation"}
            )

        for name, callable_node in callables:
            doc = ast.get_docstring(callable_node, clean=False)
            if not doc:
                failures.append(f"{name} is missing a docstring")
            elif "Example:" not in doc:
                failures.append(f"{name} is missing an Example: block")
    return failures


def documented_editorial_api_failures(tree: ast.Module) -> list[str]:
    """Require user-facing stub docs and examples for every editorial factory."""
    failures: list[str] = []
    slide_kit = next(
        (node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "SlideKit"),
        None,
    )
    if slide_kit is None:
        return ["SlideKit is missing from the Python stub"]
    methods = {
        child.name: child
        for child in slide_kit.body
        if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef))
    }
    for name in EDITORIAL_SCENE_MEMBERS:
        method = methods.get(name)
        if method is None:
            failures.append(f"SlideKit.{name} is missing from the stub")
            continue
        doc = ast.get_docstring(method, clean=False)
        if not doc:
            failures.append(f"SlideKit.{name} is missing a docstring")
        elif "Example:" not in doc:
            failures.append(f"SlideKit.{name} is missing an Example: block")
    return failures


def validate_runtime_type_aliases(module: object) -> list[str]:
    """Public aliases documented for annotations must resolve at runtime."""
    failures: list[str] = []
    package = importlib.import_module("gaanim")
    playable = getattr(package, "Playable", None)
    if playable is None or "Playable" not in package.__all__:
        return ["gaanim.Playable is not exported at runtime"]
    scene = module.Scene(frame=(16, 9))
    anim = scene.geometry.dot(0.1).animate.fade_in()
    if not isinstance(anim, playable) or not isinstance(module.parallel(anim), playable):
        failures.append("gaanim.Playable does not match Anim and Composition")
    if isinstance(1.0, playable):
        failures.append("gaanim.Playable accepted a non-playable value")
    for name in ("NavigationState", "SectionLike", "SectionTarget"):
        if getattr(package, name, None) is None or name not in package.__all__:
            failures.append(f"gaanim.{name} is not exported at runtime")
    if typing.get_args(getattr(package, "NavigationState", None)) != ("done", "current", "upcoming"):
        failures.append("gaanim.NavigationState does not list done, current and upcoming")
    return failures


def validate_timeline_cursor_contract(module: object) -> list[str]:
    """Expose the authoring cursor and absolute stop times to scripts."""
    failures: list[str] = []
    scene = module.Scene(frame=(16, 9))
    if scene.cursor != 0.0 or scene.stops != []:
        failures.append("a new Scene must start at cursor 0 without stops")
    scene.wait(1.5)
    scene.stop("reveal")
    scene.segment("details")
    scene.wait(0.5)
    scene.stop()
    stops = scene.stops
    if abs(scene.cursor - 2.0) > 1e-9:
        failures.append("Scene.cursor did not follow waits across segments")
    if [(stop.name, stop.time, stop.segment) for stop in stops] != [
        ("reveal", 1.5, stops[0].segment if stops else None),
        (None, 2.0, "details"),
    ] or not all(isinstance(stop, module.SceneStop) for stop in stops):
        failures.append("Scene.stops did not report named/anonymous stops in absolute time")
    return failures


def validate_theme_typography_contract(module: object) -> list[str]:
    """Theme font directories and the theme-wide markup default."""
    failures: list[str] = []
    if module.Theme().text_markup is not True:
        failures.append("Theme().text_markup must default to True")
    literal = module.Theme(text_markup=False)
    if module.Theme(literal).text_markup is not False:
        failures.append("a derived Theme did not keep text_markup")
    scene = module.Scene(frame=(16, 9), theme=literal)
    # Unbalanced markers are only valid when markup is off.
    scene.text("tb:dist_comp *x")
    scene.text.measure("_a")
    scene.slides.badge("_vel_max")
    try:
        scene.text("*x", markup=True)
    except ValueError:
        pass
    else:
        failures.append("an explicit markup=True did not override the theme")
    try:
        module.Theme(font_dir="this/font/dir/does/not/exist")
    except OSError:
        pass
    else:
        failures.append("Theme(font_dir=...) accepted a missing directory")
    return failures


def validate_editorial_contract(module: object) -> list[str]:
    """Exercise the editorial kit without starting the renderer."""
    failures: list[str] = []
    scene = module.Scene(frame=(16, 9), margin=0.48)
    signature = inspect.signature(module.SlideKit.section_header)
    if signature.parameters["rule"].default is not False:
        failures.append("Scene.section_header must hide its rule by default")
    components = (
        scene.slides.badge("Ready", variant="success"),
        scene.slides.chip("Live", variant="danger", appearance="solid"),
        scene.slides.card("Result", "The solver converged.", "12 ms"),
        scene.slides.banner("Simulation complete", position="bottom"),
        scene.slides.lower_third("Ada Lovelace", "Mathematician", kicker="Speaker"),
        scene.slides.stat_card("98%", "Accuracy", delta="+4.2%", variant="success"),
        scene.slides.quote_card("Clarity matters.", "Gaanim"),
        scene.slides.section_header("Method", kicker="02", align="center"),
    )
    if not all(isinstance(component, module.Drawable) for component in components):
        failures.append("one or more editorial factories did not return Drawable")
    animations = [component.animate.fade_in().duration(0.1) for component in components]
    if not all(isinstance(animation, module.Anim) for animation in animations):
        failures.append("one or more editorial groups did not preserve Drawable animations")
    else:
        scene.play(animations)

    invalid_calls = (
        lambda: scene.slides.badge(""),
        lambda: scene.slides.badge("x", variant="unknown"),
        lambda: scene.slides.chip("x", appearance="glass"),
        lambda: scene.slides.card("x", width=20.0, padding=(12.0, 4.0)),
        lambda: scene.slides.banner("x", position="center"),
        lambda: scene.slides.lower_third("x", side="center"),
        lambda: scene.slides.stat_card("", "label"),
        lambda: scene.slides.quote_card("x", width=float("nan")),
        lambda: scene.slides.section_header("x", align="justify"),
    )
    for call in invalid_calls:
        try:
            call()
        except ValueError:
            pass
        else:
            failures.append("an editorial factory accepted invalid authored input")
    styled = (
        scene.slides.badge("_vel_max", font="Libertinus Serif", weight=700, markup=False),
        scene.slides.chip(
            "*draft", style=module.TextStyle(italic=True, letter_spacing=0.02), markup=False
        ),
    )
    if not all(isinstance(component, module.Drawable) for component in styled):
        failures.append("styled badge/chip labels did not return Drawable")
    for call in (
        lambda: scene.slides.badge("_offset"),
        lambda: scene.slides.chip("x", weight=0),
    ):
        try:
            call()
        except ValueError:
            pass
        else:
            failures.append("badge/chip accepted unbalanced markup or an invalid weight")
    plain = scene.text.measure("Ready", role="label")
    bold = scene.text.measure("Ready", role="label", weight=700)
    tracked = scene.text.measure(
        "Ready", role="label", style=module.TextStyle(letter_spacing=0.1)
    )
    literal = scene.text.measure("*Ready*", role="label", markup=False)
    if not (tracked[0] > plain[0] and literal[0] > bold[0] > 0.0):
        failures.append(
            f"Typography.measure ignored weight, style or markup: "
            f"{plain} {bold} {tracked} {literal}"
        )
    if hasattr(module.Scene, "caption"):
        failures.append("removed Scene.caption remains public")
    try:
        scene.slides.badge("legacy", 0.0, 0.0)
    except (TypeError, AttributeError):
        pass
    else:
        failures.append("Scene.badge still accepts legacy positional coordinates")
    return failures


def validate_visualization_contract(module: object) -> list[str]:
    """Exercise the native builders without starting the renderer."""
    failures: list[str] = []
    scene = module.Scene(frame=(16, 9))
    x_axis = module.Axis.linear(-4.0, 4.0).ticks(1.0).label("x")
    y_axis = module.Axis.symlog(-10.0, 10.0, threshold=1.0).label("y", position="top")
    space = scene.viz.cartesian_2d(x_axis, y_axis, width=520.0, height=280.0)

    selective = scene.viz.cartesian_2d(
        x_axis,
        y_axis,
        width=520.0,
        height=280.0,
        grid=False,
        axes=False,
        ticks=False,
        numbers=False,
        labels=False,
        x_grid=True,
        y_axis=True,
    )
    for layer_name in ("grid", "minor_grid", "axes", "ticks", "numbers", "labels"):
        if not isinstance(selective.layer(layer_name), module.Drawable):
            failures.append(f"disabled Cartesian layer {layer_name!r} is not addressable")

    complex_space = scene.viz.complex(
        grid=False,
        axes=False,
        ticks=False,
        numbers=False,
        labels=False,
        x_labels=True,
    )
    if not isinstance(complex_space.layer("labels"), module.Drawable):
        failures.append("ComplexSpace did not apply Cartesian visibility options")

    polar = scene.viz.polar(
        module.Axis.linear(0.0, 4.0).ticks(1.0).label("r"),
        grid=False,
        rings=True,
        axes=False,
        numbers=False,
        labels=False,
    )
    for layer_name in ("grid", "axes", "numbers", "labels"):
        if not isinstance(polar.layer(layer_name), module.Drawable):
            failures.append(f"disabled polar layer {layer_name!r} is not addressable")

    line = scene.viz.number_line(
        module.Axis.linear(0.0, 4.0).ticks(1.0).label("t"),
        axis_visible=False,
        ticks=False,
        numbers=False,
        labels=False,
    )
    for layer_name in ("axis", "ticks", "numbers", "labels"):
        if not isinstance(line.layer(layer_name), module.Drawable):
            failures.append(f"disabled number-line layer {layer_name!r} is not addressable")

    amplitude = scene.viz.parameter(1.0)
    space.plot(lambda x, value: value * math.sin(x), inputs=[amplitude])
    space.plot(
        lambda x, value: value * math.sin(x),
        derivative=lambda x, value: value * math.cos(x),
        inputs=[amplitude],
    )
    readout = scene.viz.readout(lambda value: value * 2.0, inputs=[amplitude], label="$a$")
    variable = scene.viz.variable(1.0, label="$k$")
    if not isinstance(readout, module.Drawable) or not isinstance(variable, module.Drawable):
        failures.append("reactive readouts must be Drawable instances")
    if hasattr(module, "Expr") or hasattr(module, "_Expr") or hasattr(module, "ValueTracker"):
        failures.append("legacy reactive classes remain public")
    if not isinstance(module.computed(lambda value: value, inputs=[amplitude]), module.Computed):
        failures.append("computed() did not return Computed")
    try:
        module.computed(lambda: 1.0, inputs=[amplitude])
        failures.append("computed() accepted a callback with the wrong arity")
    except TypeError:
        pass
    async def asynchronous(value: float) -> float:
        return value
    try:
        module.computed(asynchronous, inputs=[amplitude])
        failures.append("computed() accepted an asynchronous callback")
    except TypeError:
        pass
    foreign = module.Scene().viz.parameter(2.0)
    try:
        scene.viz.readout(lambda value: value, inputs=[foreign])
        failures.append("readout accepted a Parameter from another Scene")
    except ValueError:
        pass
    space.parametric(
        lambda t, scale: (t, scale * t * t),
        (-1.0, 1.0),
        samples=32,
        inputs=[amplitude],
    )
    space.implicit(lambda px, py: px * px + py * py - 1.0, resolution=(16, 16))
    field = space.field(lambda px, py: (-py, px))
    arrows = field.arrows(resolution=(4, 4), colormap="viridis")
    streams = field.streamlines(seeds=(4, 3), max_time=0.4)
    if field.dimensions != 2 or field.evaluation != "python":
        failures.append("2D VectorField did not retain its Python callback evaluator")
    if not isinstance(arrows.drawable(), module.Drawable):
        failures.append("ArrowVectorField did not materialize a Drawable")
    if not streams.flow(0.5):
        failures.append("StreamLines.flow did not produce finite animations")
    field.advect(scene.geometry.dot(2.0), (1.0, 0.0), duration=0.5, max_time=0.4)
    particles = field.particles(3, duration=0.5, max_time=0.3)
    if not particles.flow() or not isinstance(particles.drawable(), module.Drawable):
        failures.append("FlowParticles did not expose drawable and flow handles")
    for aggregate in (arrows, streams, particles):
        for method in (
            "create", "write", "fade_in", "fade_out", "uncreate", "unwrite",
            "grow_from_center", "shrink_to_center",
        ):
            animation = getattr(aggregate.animate, method)().duration(0.1)
            if not isinstance(animation, module.Anim):
                failures.append(
                    f"{type(aggregate).__name__}.animate.{method} did not return Anim"
                )
            if hasattr(aggregate, method):
                failures.append(f"legacy {type(aggregate).__name__}.{method} remains public")
    if len(module.ColorMap.names("matplotlib")) != 39:
        failures.append("Matplotlib ColorMap catalog is incomplete")
    if len(module.ColorMap.names("scientific")) != 39:
        failures.append("Scientific ColorMap catalog is incomplete")
    space.tangent(lambda value: value * value, 1.0)
    space.riemann_sum(lambda value: value * value, (0.0, 2.0), rectangles=4)

    chart_spec = module.ChartSpec(
        {"id": ["a", "b"], "x": [0.0, 1.0], "y": [1.0, 2.0]}, key="id"
    ).mark("line").encode(x="x", y="y")
    chart = scene.viz.chart(chart_spec)
    if not isinstance(chart.drawable(), module.Drawable):
        failures.append("Scene.chart did not materialize a Drawable hierarchy")

    coordinate = space.coord(1.0, 2.0)
    coordinate.place(scene.geometry.dot(3.0))
    local = space.data_to_local(1.0, 2.0)
    round_trip = space.local_to_data(*local)
    if abs(round_trip[0] - 1.0) > 1e-9 or abs(round_trip[1] - 2.0) > 1e-9:
        failures.append("CoordinateSpace data/local round trip failed")
    scene_point = space.data_to_scene(1.0, 2.0)
    if not isinstance(scene_point, module.PointRef):
        failures.append("CoordinateSpace.data_to_scene did not return a PointRef")
    scene.geometry.dot(3.0).follow(scene_point, offset=(0.0, 0.3))
    try:
        space.data_to_scene(float("nan"), 0.0)
    except ValueError:
        pass
    else:
        failures.append("CoordinateSpace.data_to_scene accepted non-finite data")

    linear_space = scene.viz.cartesian_2d(
        module.Axis.linear(-4.0, 4.0),
        module.Axis.linear(-3.0, 3.0),
    )
    view_animation = linear_space.animate.view_to((-2.0, 2.0), (-1.5, 1.5))
    if not isinstance(view_animation, module.Anim):
        failures.append("CoordinateSpace.animate.view_to did not return Anim")
    for method, args in (
        ("create", ()),
        ("write", ()),
        ("fade_in", ()),
        ("fade_out", ()),
        ("move_to", (10.0, 20.0)),
        ("scale_to", (1.2,)),
        ("rotate_to", (0.1,)),
    ):
        result = getattr(linear_space.animate, method)(*args)
        if method in {"create", "write", "fade_in", "fade_out"}:
            result = result.duration(0.25)
        if not isinstance(result, module.Anim):
            failures.append(f"CoordinateSpace.animate.{method} did not return Anim")
    if hasattr(linear_space, "animate_view"):
        failures.append("legacy CoordinateSpace.animate_view remains public")

    space_3d = scene.viz.cartesian_3d(
        module.Axis.linear(-2.0, 2.0),
        module.Axis.linear(-2.0, 2.0),
        module.Axis.linear(-2.0, 2.0),
        size=(4.0, 4.0, 4.0),
        grid=False,
        axes=False,
        ticks=False,
        numbers=False,
        labels=False,
        xy_grid=True,
        z_axis=True,
    )
    for layer_name in ("grid", "axes", "ticks", "numbers", "labels"):
        if not isinstance(space_3d.layer(layer_name), module.Drawable):
            failures.append(f"disabled 3D layer {layer_name!r} is not addressable")
    space_3d.surface(
        lambda px, py, scale: scale * (px * px - py * py),
        resolution=(8, 8),
        inputs=[amplitude],
    )
    space_3d.parametric(
        lambda t, scale: (t, scale * t * t, t * t * t),
        (-1.0, 1.0),
        samples=16,
        inputs=[amplitude],
    )
    field_3d = space_3d.field(lambda px, py, pz: (-py, px, -pz))
    field_3d.arrows(resolution=(2, 2, 2), colormap="batlow")
    field_3d.streamlines(seeds=(2, 2, 2), max_time=0.25)
    field_3d.particles(2, duration=0.4, max_time=0.2)

    for removed in ("axes", "number_plane", "axes_3d", "polar_plane", "complex_plane"):
        if hasattr(module.Scene, removed):
            failures.append(f"v1 Scene.{removed} remains public")
    for removed in ("line", "step", "area", "scatter", "bars", "histogram", "box_plot", "violin", "error_bars", "heatmap"):
        if hasattr(module.CoordinateSpace, removed):
            failures.append(f"tabular CoordinateSpace.{removed} remains public")
    if hasattr(module.CoordinateSpace, "function"):
        failures.append("legacy CoordinateSpace.function alias remains public")

    for removed in ("plot", "get_graph", "function_graph", "parametric_curve", "bar_chart"):
        if hasattr(module.Scene, removed):
            failures.append(f"legacy Scene.{removed} remains public")
    removed_visualization_compat = {
        module.Geometry: (
            "_legacy_function_graph",
            "_legacy_parametric_curve",
            "_legacy_axes",
            "_legacy_axes_3d",
            "_legacy_plot",
            "_legacy_get_graph",
            "_legacy_plot_parametric_curve",
        ),
        module.Visualization: ("_legacy_bar_chart",),
        module.Drawable: (
            "_legacy_coords_to_point",
            "_legacy_point_to_coords",
            "_legacy_get_x_axis",
            "_legacy_get_y_axis",
            "_legacy_get_axes",
            "_legacy_add_coordinates",
            "_legacy_get_graph",
        ),
    }
    for owner, names in removed_visualization_compat.items():
        for removed in names:
            if hasattr(owner, removed):
                failures.append(f"legacy {owner.__name__}.{removed} remains public")
    for removed in REMOVED_TEXT_SCENE_MEMBERS:
        if hasattr(module.Scene, removed):
            failures.append(f"removed Scene.{removed} remains public")
    for removed in REMOVED_TEXT_DRAWABLE_MEMBERS:
        if hasattr(module.Drawable, removed):
            failures.append(f"removed Drawable.{removed} remains public")

    compact_parts = module.parts(mass_left="m", gravity="g sin(theta)")
    if not isinstance(compact_parts, module.TextParts):
        failures.append("parts() did not return TextParts")
    compact_text = scene.text("$-", compact_parts, "$")
    if len(compact_text.parts) != 2:
        failures.append("TextParts did not preserve both ordered semantic entries")
    for name in ("mass_left", "gravity"):
        if name not in compact_text.parts:
            failures.append(f"TextParts entry {name!r} was not expanded by Scene.text")
        if not isinstance(compact_text[name], module.TextSelection):
            failures.append(f"TextParts entry {name!r} was not selectable")

    nested_compact = module.part(
        "formula", "$", module.parts(left="a", right="b"), "$"
    )
    nested_text = scene.text(nested_compact)
    for path in ("formula.left", "formula.right"):
        if path not in nested_text.parts:
            failures.append(f"TextParts did not expand inside part(): {path}")

    if compact_text.become("$", module.parts(first="x", second="y"), "$") is not None:
        failures.append("Text.become with TextParts did not preserve its None return")
    for name in ("first", "second"):
        if name not in compact_text.parts:
            failures.append(f"Text.become did not install TextParts entry {name!r}")

    equation = scene.text.equation(
        module.part("sum_force", "sum F_t"),
        "=",
        module.parts(mass="m", acceleration="a_t"),
        size=42,
    )
    if not isinstance(equation, module.Text):
        failures.append("Scene.equation did not return Text")
    for name in ("sum_force", "mass", "acceleration"):
        if name not in equation.parts:
            failures.append(f"Scene.equation lost semantic part {name!r}")
        if not isinstance(equation[name], module.TextSelection):
            failures.append(f"Scene.equation part {name!r} was not selectable")
    if not isinstance(equation.animate.write().duration(0.5), module.Anim):
        failures.append("Scene.equation did not preserve Text animations")
    codex_equation = scene.text.equation(
        module.parts(gravity="g sin(theta)", acceleration="theta''")
    )
    codex_animations = (
        codex_equation["gravity"].animate.indicate().duration(0.3),
        codex_equation["acceleration"].animate.fill(module.GOLD).duration(0.3),
    )
    if not all(isinstance(animation, module.Anim) for animation in codex_animations):
        failures.append("Typst/Codex semantic selections did not return animations")
    else:
        scene.play(list(codex_animations))
    try:
        scene.text.equation()
    except ValueError:
        pass
    else:
        failures.append("Scene.equation accepted empty content")

    for invalid_parts in (
        lambda: module.parts(),
        lambda: module.parts(**{"": "m"}),
        lambda: module.parts(empty=""),
    ):
        try:
            invalid_parts()
        except ValueError:
            pass
        else:
            failures.append("parts() accepted empty names or content")
    try:
        module.parts(mass=1)
    except TypeError:
        pass
    else:
        failures.append("parts() accepted a non-string value")
    mapped_parts = scene.text(module.parts({"tb:dist": "d", "x-1": "x"}))
    if len(mapped_parts.parts) != 2 or any(
        name not in mapped_parts.parts for name in ("tb:dist", "x-1")
    ):
        failures.append("parts(mapping) did not create parts with string names")
    try:
        module.parts({"a": "x"}, b="y")
    except ValueError:
        pass
    else:
        failures.append("parts() accepted a mapping mixed with keyword entries")

    formula = module.part(
        "formula", "$E = ", module.part("mass", "m", color=module.GOLD), " c^2$"
    )
    text = scene.text("Energy: ", formula)
    if not isinstance(text, module.Text):
        failures.append("Scene.text did not return Text")
    if not isinstance(text["formula"]["mass"], module.TextSelection):
        failures.append("nested semantic text selection did not return TextSelection")
    if "mass" not in text.parts or "formula.mass" not in text.parts:
        failures.append("Text.parts membership did not resolve semantic part names")
    if not isinstance(text["formula"]["mass"].fill(module.RED), module.TextSelection):
        failures.append("TextSelection.fill did not preserve the semantic selection")
    configured_write = text.animate.write().duration(0.6)
    if not isinstance(configured_write, module.Anim):
        failures.append("Text.write did not support post-effect duration configuration")
    else:
        scene.play([configured_write])
    selection_anim = text["formula"]["mass"].animate.indicate()
    if not isinstance(text["formula"]["mass"].animate, module.TextSelectionAnimation):
        failures.append("TextSelection.animate did not return its typed proxy")
    if not isinstance(selection_anim, module.Anim):
        failures.append("TextSelection.indicate did not return Anim")
    else:
        scene.play([selection_anim])
    selection_color = text["formula"]["mass"].animate.fill(module.RED).duration(0.4)
    selection_opacity = text["formula"]["mass"].animate.opacity(0.6).duration(0.4)
    selection_compound = text["formula"]["mass"].animate.fill(module.BLUE).opacity(0.8)
    if not all(
        isinstance(anim, module.Anim)
        for anim in (selection_color, selection_opacity, selection_compound)
    ):
        failures.append("TextSelection color/opacity animations did not return Anim")
    else:
        scene.play([selection_color, selection_opacity])
        scene.play([selection_compound])
    try:
        text["formula"]["mass"].animate.scale_by(2.0)
    except (TypeError, AttributeError):
        pass
    else:
        failures.append("TextSelection.animate accepted an unsupported scale target")
    target_text = scene.text(
        "Energy: ",
        module.part(
            "formula", "$E = ", module.part("mass", "m", color=module.BLUE), " c^2$"
        ),
    )
    text_transition = text.animate.transform_to(target_text)
    if not isinstance(text_transition, module.Anim):
        failures.append("Text.animate.transform_to did not return Anim")
    else:
        scene.play([text_transition])
    border_fill = text.animate.draw_border_then_fill().duration(0.3)
    if not isinstance(border_fill, module.Anim):
        failures.append("Text.animate.draw_border_then_fill did not return Anim")
    if hasattr(text, "draw_border_then_fill"):
        failures.append("legacy Text.draw_border_then_fill remains public")
    chained_text = scene.text("Chain").fill(module.WHITE).move_to(
        0.0, 0.0, module.Anchor.TOP_LEFT
    )
    if not isinstance(chained_text, module.Text):
        failures.append("Text fluent styling or positioning erased the Text subtype")
    scaled_equation = scene.text.equation("x^2 d x").scale_by(3).move_to(0.0, 0.0)
    if not isinstance(scaled_equation, module.Text):
        failures.append("Text.scale_by erased the Text subtype before baseline positioning")
    baseline_text = scene.text("Baseline").move_to(
        0.0, 0.0, anchor=module.TextAnchor.BASELINE_LEFT
    )
    baseline_equation = scene.text.equation("x_1 = 2").move_to(
        0.0, 0.0, anchor=module.TextAnchor.BASELINE_RIGHT
    )
    if not all(isinstance(value, module.Text) for value in (baseline_text, baseline_equation)):
        failures.append("TextAnchor positioning did not preserve Text/Equation handles")
    anchored_drawable = scene.geometry.rect(40.0, 20.0).move_to(20.0, 10.0, anchor=module.Anchor.RIGHT)
    if not isinstance(anchored_drawable, module.Drawable):
        failures.append("Drawable.at with an anchor did not return Drawable")
    reference_drawable = scene.geometry.dot(6.0).move_to(-25.0, 15.0)
    centered_drawable = scene.geometry.rect(40.0, 20.0).move_to(reference_drawable)
    if not isinstance(centered_drawable, module.Drawable):
        failures.append("Drawable.at with a reference did not return Drawable")
    centered_text = scene.text("Centered").move_to(reference_drawable)
    if not isinstance(centered_text, module.Text):
        failures.append("Text.at with a reference erased the Text subtype")
    centered_primitive = scene.geometry.cube().move_to(reference_drawable)
    if not isinstance(centered_primitive, module.Primitive3D):
        failures.append("Primitive3D.at with a reference erased the Primitive3D subtype")
    try:
        scene.geometry.rect(40.0, 20.0).move_to(reference_drawable, anchor=module.Anchor.TOP)
    except TypeError:
        pass
    else:
        failures.append("Drawable.at accepted an anchor with a reference")
    try:
        scene.geometry.rect(40.0, 20.0).move_to(10.0)
    except TypeError:
        pass
    else:
        failures.append("Drawable.at accepted a numeric x without y")
    try:
        scene.geometry.rect(40.0, 20.0).move_to(
            0.0, 0.0, anchor=module.TextAnchor.BASELINE_CENTER
        )
    except TypeError:
        pass
    else:
        failures.append("Drawable.at accepted a TextAnchor")
    if not isinstance(
        anchored_drawable.move_to(80.0, 40.0, anchor=module.Anchor.TOP_RIGHT),
        module.Drawable,
    ):
        failures.append("Drawable.move_to with an anchor did not remain immediate")
    if not isinstance(
        anchored_drawable.animate.move_to(80.0, 40.0, anchor=module.Anchor.TOP_RIGHT),
        module.Anim,
    ):
        failures.append("Anim.move_to with an anchor did not return Anim")
    try:
        scene.text("$unbalanced")
    except ValueError:
        pass
    else:
        failures.append("Scene.text accepted an unbalanced math delimiter")
    inline_markup = scene.text(
        "Normal, _emphasis_, *strong*, *_both_*, snake_case y $x_1 * 5$."
    )
    if not isinstance(inline_markup, module.Text):
        failures.append("Scene.text inline markup did not return Text")
    try:
        scene.text("*unbalanced")
    except ValueError:
        pass
    else:
        failures.append("Scene.text accepted unbalanced inline markup")
    return failures


def validate_layout_detach_contract(module: object) -> list[str]:
    """A reused layout child can detach and regain positional operations."""
    failures: list[str] = []
    scene = module.Scene(frame=(16, 9))
    scene.segment("cover", background=module.BLUE)
    title = scene.text("Reusable title", role="title")
    body = scene.text("Body")
    page = scene.layout.column([title, body], width="fill", align="center")
    scene.segment("detail", module.Transition.cross_fade(0.2))
    scene.reuse(title)
    page.detach(title)
    try:
        movement = title.move_to(0.0, 120.0)
    except module.LayoutOwnershipError:
        failures.append("Layout.detach did not release positional ownership")
    else:
        if not isinstance(movement, module.Text):
            failures.append("a detached child move_to was not immediate")
    return failures


def validate_reactive_connector_contract(module: object) -> list[str]:
    """Exercise anchored endpoints, bars, springs, and rich dimensions."""
    failures: list[str] = []
    scene = module.Scene(frame=(16, 9))
    frame = scene.geometry.rect(240.0, 120.0)
    for line in [
        scene.geometry.line(length=3.0).next_to(frame, module.Direction.DOWN, spacing=0.2),
        scene.geometry.line(length=2.0, direction=module.Direction.UP),
        scene.geometry.line((0.0, 0.0), (1.0, 0.0)),
        scene.geometry.line(0.0, 0.0, 1.0, 0.0),
    ]:
        if not isinstance(line, module.Drawable):
            failures.append("Geometry.line did not return a Drawable")
    for kwargs in [
        {"length": value} for value in [0.0, -1.0, math.inf, math.nan]
    ] + [
        {"length": 2.0, "direction": module.Direction.custom(0.0, 0.0, 0.0)},
        {"length": 2.0, "direction": module.Direction.custom(1.0, 0.0, 1.0)},
    ]:
        try:
            scene.geometry.line(**kwargs)
        except ValueError:
            pass
        else:
            failures.append(f"Geometry.line accepted invalid geometry: {kwargs}")
    for args, kwargs in [
        ((), {}),
        ((3.0,), {}),
        ((), {"direction": module.Direction.UP}),
        (((0.0, 0.0), (1.0, 0.0)), {"length": 2.0}),
        ((0.0, 0.0, 1.0, 0.0), {"length": 2.0}),
        (((0.0, 0.0), (1.0, 0.0)), {"direction": module.Direction.UP}),
    ]:
        try:
            scene.geometry.line(*args, **kwargs)
        except TypeError:
            pass
        else:
            failures.append("Geometry.line accepted mixed or incomplete arguments")
    corner = frame.anchor_point(module.Anchor.TOP_RIGHT, offset=(5.0, -3.0))
    if not isinstance(corner, module.AnchorPoint):
        failures.append("Drawable.anchor_point did not return AnchorPoint")

    bar = scene.mechanics.bar_between((-180.0, 120.0), corner, width=9.0)
    spring = scene.mechanics.spring_between(
        frame, corner, coils=6, amplitude=10.0, start_straight=8.0, end_straight=14.0
    )
    if not isinstance(bar, module.Drawable) or not isinstance(spring, module.Drawable):
        failures.append("reactive bar or spring did not return Drawable")
    try:
        scene.mechanics.spring_between((0.0, 0.0), (100.0, 0.0), start_straight=-1.0)
    except ValueError:
        pass
    else:
        failures.append("Scene.spring_between accepted a negative start_straight")

    dimension = scene.mechanics.dimension_between(
        frame.anchor_point(module.Anchor.LEFT),
        frame.anchor_point(module.Anchor.RIGHT),
        45.0,
        label="$W_f$",
        show_value=True,
        format=".1f",
        unit="mm",
        scale=0.5,
        label_gap=12.0,
        label_orientation="aligned",
        extension_style="dashed",
        line_width=3.0,
        dash_length=12.0,
        gap_length=8.0,
    )
    if not isinstance(dimension, module.Dimension):
        failures.append("Scene.dimension_between did not return Dimension")
    elif not all(
        isinstance(part, module.Drawable)
        for part in (
            dimension.line,
            dimension.extensions,
            dimension.label,
            dimension.number,
            dimension.unit,
        )
    ):
        failures.append("Dimension did not expose its line/label/number/unit parts")

    physical_width = scene.viz.parameter(2.5)
    semantic_dimension = scene.mechanics.dimension_between(
        frame.anchor_point(module.Anchor.LEFT),
        frame.anchor_point(module.Anchor.RIGHT),
        65.0,
        label="$W$",
        value=physical_width,
        format=".1f",
        unit="m",
        scale=100.0,
    )
    if not isinstance(semantic_dimension.number, module.Drawable):
        failures.append("dimension_between(value=parameter) did not imply a number readout")
    value_animation = physical_width.animate.set(4.0).duration(0.4)
    if not isinstance(value_animation, module.Anim):
        failures.append("dimension value Parameter did not remain animatable")
    else:
        scene.play([value_animation])

    try:
        scene.mechanics.dimension_between((0.0, 0.0), (1.0, 0.0), 10.0, value="invalid")
    except TypeError:
        pass
    else:
        failures.append("Scene.dimension_between accepted an invalid semantic value")

    for side in ("left", "right", "above", "below"):
        scene.mechanics.dimension_between(
            (0.0, 0.0), (0.0, 1.0), -0.5, side=side, show_value=True,
            font="Cascadia Mono", weight=600,
            label_style=module.TextStyle(italic=True),
        )
    for invalid in (
        {"line_width": 0.0},
        {"extension_style": "dots"},
        {"dash_length": 0.0},
        {"gap_length": float("nan")},
        {"side": "up"},
        {"weight": 0},
    ):
        try:
            scene.mechanics.dimension_between((0.0, 0.0), (1.0, 0.0), 10.0, **invalid)
        except ValueError:
            pass
        else:
            failures.append(f"Scene.dimension_between accepted invalid options: {invalid}")

    try:
        scene.mechanics.dimension_between((0.0, 0.0), (1.0, 0.0), 10.0, scale=0.0)
    except ValueError:
        pass
    else:
        failures.append("Scene.dimension_between accepted a non-positive scale")
    return failures


def validate_camera_rig_contract(module: object) -> list[str]:
    """Exercise the semantic camera facade and removed compatibility surface."""
    failures: list[str] = []
    scene = module.Scene(frame=(16, 9))
    marker = scene.geometry.dot(5.0)
    parameter = scene.viz.parameter(1.0)
    point = scene.geometry.point_ref(
        module.computed(lambda value: value * 20.0, inputs=[parameter]),
        10.0,
    )

    state_2d = scene.camera.state_2d(center=(12.0, -8.0), zoom=1.25, rotation=0.1)
    state_3d = scene.camera.state_3d((4.0, 3.0, 8.0), (0.0, 0.0, 0.0), fov_y=0.8)
    captured = scene.camera.capture()
    saved = scene.camera.save("overview")
    if not all(
        isinstance(state, module.CameraState)
        for state in (state_2d, state_3d, captured, saved)
    ):
        failures.append("camera state factories did not return CameraState")

    constraint = scene.camera.bind_2d(center=point, zoom=parameter)
    if not isinstance(constraint, module.CameraConstraint):
        failures.append("Camera.bind_2d did not return CameraConstraint")
    constraint.disable()
    constraint.enable()
    scene.camera.bind_3d(eye=(4.0, 3.0, 8.0), target=point, fov_y=0.8).disable()

    immediate = (
        scene.camera.pan_to(point), scene.camera.zoom_to(parameter),
        scene.camera.rotate_to(module.computed(lambda value: value * 0.1, inputs=[parameter])),
        scene.camera.frame_to([marker], margin=(10.0, 20.0), dynamic=True),
        scene.camera.look_at((4.0, 3.0, 8.0), point), scene.camera.orthographic(1.0),
        scene.camera.reset(), scene.camera.to(state_2d), scene.camera.restore("overview"),
    )
    if not all(isinstance(value, module.Camera) for value in immediate):
        failures.append("one or more direct camera operations were not immediate")
    animations = (
        scene.camera.animate.pan_to(point), scene.camera.animate.zoom_to(parameter),
        scene.camera.animate.follow(point, offset=(2.0, 3.0), lag=0.2),
        scene.camera.animate.shake(), scene.camera.animate.to(state_2d),
    )
    if not all(isinstance(animation, module.Anim) for animation in animations):
        failures.append("one or more camera rig operations did not return Anim")

    for removed in (
        "camera_pan_to",
        "camera_zoom_to",
        "camera_frame_to",
        "camera_rotate_to",
        "camera_follow",
        "camera_shake",
    ):
        if hasattr(module.Scene, removed):
            failures.append(f"removed Scene.{removed} remains public")

    invalid_calls = (
        lambda: scene.camera.perspective(3.141592653589793),
        lambda: scene.camera.look_at((0.0, 0.0, 0.0), (0.0, 0.0, 0.0)),
        lambda: scene.camera.bind_2d(center=point, influence=1.5),
        lambda: scene.camera.state_2d(zoom=0.0),
        lambda: scene.camera.restore("missing"),
        lambda: scene.camera.save(""),
        lambda: module.Scene(frame=(16, 9)).camera.to(state_2d),
    )
    for call in invalid_calls:
        try:
            call()
        except ValueError:
            pass
        else:
            failures.append("camera validation accepted an invalid authored value")
    return failures


def validate_camera_motion_contract(module: object) -> list[str]:
    """Exercise trauma shake and exponential zoom (CA-01, CA-02)."""
    failures: list[str] = []
    scene = module.Scene(frame=(16, 9))
    marker = scene.geometry.dot(0.2)
    camera = scene.camera.animate
    animations = (
        camera.shake(trauma=0.8, decay=1.5, frequency=12, rotation=0.02, seed=0),
        camera.shake(0.2, 6),
        camera.shake(amplitude=0.3, seed=4),
        camera.zoom_to(8.0, interpolation="exponential"),
        camera.zoom_to(2.0, interpolation="linear"),
        camera.frame_to(marker, 0.5, interpolation="linear"),
    )
    if not all(isinstance(animation, module.Anim) for animation in animations):
        failures.append("camera shake/zoom interpolation did not return Anim")
    invalid_calls = (
        lambda: camera.shake(trauma=1.5),
        lambda: camera.shake(decay=-1.0),
        lambda: camera.shake(-0.1, 4.0),
        lambda: camera.zoom_to(2.0, interpolation="cubic"),
        lambda: camera.frame_to(marker, interpolation="log"),
    )
    for call in invalid_calls:
        try:
            call()
        except ValueError:
            pass
        else:
            failures.append("camera motion accepted an invalid shake or interpolation")
    return failures


def validate_matrix_contract(module: object) -> list[str]:
    """Exercise matrix construction, selectors, ordering, and mutations."""
    failures: list[str] = []
    scene = module.Scene(frame=(16, 9))
    matrix = scene.viz.matrix([[1, 2, 3], [4, 5, 6]], delimiters="parentheses")
    if matrix.shape != (2, 3):
        failures.append("Scene.matrix returned the wrong shape")
    if not isinstance(matrix[0, 0], module.Drawable):
        failures.append("a matrix cell is not a Drawable")
    if matrix[1, :].coordinates != ((1, 0), (1, 1), (1, 2)):
        failures.append("matrix row selection returned wrong coordinates")
    if matrix.diagonal().coordinates != ((0, 0), (1, 1)):
        failures.append("matrix diagonal selection returned wrong coordinates")
    ordered = matrix.entries.animate.write(order="spiral_in", stagger=0.01).duration(0.1)
    if len(ordered) != 6 or not all(isinstance(animation, module.Anim) for animation in ordered):
        failures.append("matrix ordered animation did not return one Anim per cell")
    if hasattr(matrix.entries.animate, "color") or hasattr(matrix.entries.animate, "ease"):
        failures.append("matrix selection animation still exposes legacy styling or easing")
    target = scene.viz.matrix([[1, 2, 3], [4, 5, 7]])
    morph = matrix.morph_to(target, stagger=0.01).duration(0.2).easing(module.Easing.SMOOTH)
    if not isinstance(morph, type(ordered)):
        failures.append("Matrix.morph_to did not return a configurable compound animation")
    try:
        matrix.morph_to(target, duration=0.2)
    except TypeError:
        pass
    else:
        failures.append("Matrix.morph_to still accepts an embedded duration")
    random_a = module.MatrixOrder.order(2, 3, matrix.entries.coordinates, "random", 7)
    random_b = module.MatrixOrder.order(2, 3, matrix.entries.coordinates, "random", 7)
    if random_a != random_b:
        failures.append("matrix random order is not reproducible")
    replacement = matrix.set(0, 1, "x")
    if not isinstance(replacement, module.Drawable):
        failures.append("Matrix.set did not return the replacement Drawable")
    try:
        scene.viz.matrix([[1, 2], [3]])
    except ValueError:
        pass
    else:
        failures.append("Scene.matrix accepted ragged data")
    return failures


def validate_matrix_stub_typing() -> list[str]:
    """Keep the matrix facade drawable-compatible and derivations specialized."""
    matrix_stub = STUB.with_name("matrix.pyi")
    tree = ast.parse(matrix_stub.read_text(encoding="utf-8"), filename=str(matrix_stub))
    matrix_class = next(
        (node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "Matrix"),
        None,
    )
    if matrix_class is None:
        return ["matrix.pyi does not declare Matrix"]
    failures: list[str] = []
    if not any(isinstance(base, ast.Name) and base.id == "Drawable" for base in matrix_class.bases):
        failures.append("Matrix stub does not expose delegated Drawable methods")
    algebra_methods = {
        "add", "subtract", "matmul", "hadamard", "scale_by", "transpose",
        "determinant", "inverse", "rank", "trace", "rref", "lu", "qr", "eigen",
    }
    for member in matrix_class.body:
        if isinstance(member, ast.FunctionDef) and member.name in algebra_methods:
            if not isinstance(member.returns, ast.Subscript):
                failures.append(f"Matrix.{member.name} returns an unspecialized MatrixDerivation")
    return failures


def validate_vector_geometry_contract(module) -> list[str]:
    """Exercise the public varargs, clipping, and fill-level contracts."""
    failures: list[str] = []
    scene = module.Scene(frame=(16, 9))
    left = scene.geometry.circle(60).move_to(-25, 0)
    right = scene.geometry.circle(60).move_to(25, 0)

    for name, result in (
        ("union", scene.geometry.union(left, right)),
        ("intersection", scene.geometry.intersection(left, right)),
        ("difference", scene.geometry.difference(left, right)),
        ("xor", scene.geometry.xor(left, right)),
    ):
        if not isinstance(result, module.Drawable):
            failures.append(f"Scene.{name} did not return Drawable")

    live = scene.geometry.union(left, right, live=True)
    try:
        live.move_to(0, 0)
    except ValueError:
        pass
    else:
        failures.append("live boolean accepted an independent position")

    clipped = scene.geometry.rect(100, 80).clip(left, invert=True)
    if not isinstance(clipped, module.Drawable):
        failures.append("Drawable.clip(invert=True) did not return Drawable")

    level = scene.geometry.fill_level(left.opacity(0), module.BLUE, 0.25, keep_outline=True)
    if not isinstance(level.set_fill_level(0.5), module.Drawable):
        failures.append("Drawable.set_fill_level did not return Drawable")
    if not isinstance(level.animate.fill_level(0.75), module.Anim):
        failures.append("Anim.fill_level did not return Anim")

    for call in (
        lambda: scene.geometry.union(left),
        lambda: scene.geometry.fill_level(left, module.BLUE, -0.1),
        lambda: level.animate.fill_level(1.1),
    ):
        try:
            call()
        except ValueError:
            pass
        else:
            failures.append("vector geometry API accepted invalid input")
    return failures


def validate_composable_properties_contract(module) -> list[str]:
    """Exercise ownership, bindings and custom validation through the real host."""
    failures: list[str] = []

    def rejected(label, action, errors=(ValueError, TypeError, RuntimeError)):
        try:
            action()
        except errors:
            return
        failures.append(label)

    scene = module.Scene()
    radius = scene.viz.parameter(1.0)
    area = module.computed(lambda r: r*r, inputs=[radius])
    doubled = module.computed(lambda a: 2*a, inputs=[area])
    timed = module.computed(lambda a, t: a+t, inputs=[doubled, scene.time])
    scene.viz.readout(timed)
    dot = scene.geometry.circle(0.2).move_to(radius, area).opacity(0.8)
    rejected("relative translation accepted a linked position", lambda: dot.shift_by(1, 0))
    rejected("native animation accepted a linked position", lambda: scene.play(dot.animate.move_to(0, 0)))
    scene.play(dot.animate.opacity(0.5))
    dot.move_to(0, 0)
    scene.play(dot.animate.move_to(radius, area))

    foreign = module.Scene().viz.parameter(1.0)
    hidden_foreign = module.computed(lambda a: a, inputs=[module.computed(lambda a: a, inputs=[foreign])])
    rejected("nested computed bypassed readout ownership", lambda: scene.viz.readout(hidden_foreign))
    rejected("nested computed bypassed setter ownership", lambda: dot.opacity(hidden_foreign))
    rejected("nested computed bypassed animated target ownership", lambda: dot.animate.opacity(hidden_foreign))
    other = module.Scene().geometry.circle(0.2)
    rejected("animated anchor accepted another Scene", lambda: dot.animate.move_to(other.anchor_point()))

    text = scene.text("Reactive")
    if text.move_to(radius, area).opacity(doubled).scale_to(radius) is not text:
        failures.append("reactive text setters lost the Text handle")
    text.move_to(0, 0).opacity(1).scale_to(1)

    native = dot.animate.custom(lambda alpha: {"position": (alpha, alpha*alpha), "opacity": 1-alpha/2}, channels=("position", "opacity"))
    scene.play(module.parallel(native.duration(1), dot.animate.fill(module.BLUE)))
    rejected("custom mixed with a native setter", lambda: dot.animate.custom(lambda a: {"opacity": a}, channels=("opacity",)).opacity(0.5))
    rejected("custom accepted duplicate channels", lambda: dot.animate.custom(lambda a: {"opacity": a}, channels=("opacity", "opacity")))
    rejected("custom accepted unknown channels", lambda: dot.animate.custom(lambda a: {"unknown": a}, channels=("unknown",)))
    rejected("custom accepted a wrong callback signature", lambda: dot.animate.custom(lambda: {}, channels=("opacity",)))

    async def async_callback(alpha):
        return {"opacity": alpha}
    rejected("custom accepted an async callback", lambda: dot.animate.custom(async_callback, channels=("opacity",)))
    for value in ({}, {"opacity": math.nan}, {"opacity": 0.5, "scale": 1.0}):
        rejected("custom accepted invalid callback values", lambda value=value: scene.play(dot.animate.custom(lambda a: value, channels=("opacity",))))

    def mutate_drawable(alpha):
        dot.fill(module.GOLD)
        return {"opacity": 0.5}

    def mutate_parameter(alpha):
        radius.set(9)
        return {"opacity": 0.5}

    def mutate_scene(alpha):
        scene.geometry.circle(0.1)
        return {"opacity": 0.5}

    for callback in (mutate_drawable, mutate_parameter, mutate_scene):
        rejected("custom callback mutated authoring state", lambda callback=callback: scene.play(dot.animate.custom(callback, channels=("opacity",))))
    rejected("custom callback read a locked scene", lambda: scene.play(dot.animate.custom(lambda a: {"opacity": scene.canvas.frame_width}, channels=("opacity",))))
    rejected("custom callback read mutable parameter state", lambda: scene.play(dot.animate.custom(lambda a: {"opacity": radius.current}, channels=("opacity",))))
    if radius.current == 9:
        failures.append("rejected custom callback changed its Parameter")

    paint = module.Brush.linear([module.BLUE, module.GOLD], start=(-1, 0), end=(1, 0))
    scene.play(dot.animate.fill(color=paint).stroke(color=paint, width=0.05))
    target = scene.geometry.circle(0.2).move_to(1, 1)
    scene.play(dot.animate.move_to(target).opacity(1))
    return failures


def validate_composition_contract(module) -> list[str]:
    failures: list[str] = []
    for legacy in ("AnimationGroup", "Succession", "LaggedStart"):
        if hasattr(module, legacy):
            failures.append(f"legacy composition helper {legacy} remains public")

    scene = module.Scene(frame=(16, 9))
    dot = scene.geometry.dot(8)
    first = dot.animate.shift_by(20, 0).duration(1.0)
    second = dot.animate.shift_by(20, 0).duration(1.0)
    overlapping = module.parallel(first, second)
    try:
        scene.play(overlapping)
    except ValueError:
        pass
    else:
        failures.append("parallel same-channel animations were not rejected")

    plan = module.sequence(first, second).defaults(easing=module.Easing.LINEAR)
    schedule = plan.schedule()
    if not isinstance(plan, module.Composition):
        failures.append("sequence did not return Composition")
    if not isinstance(schedule, module.Schedule) or schedule.span != 2.0:
        failures.append("sequence schedule did not resolve the expected span")
    if len(schedule.entries) != 2 or schedule.entries[1].start != 1.0:
        failures.append("sequence schedule offsets are incorrect")
    if not isinstance(schedule.entries[0].path, tuple):
        failures.append("ScheduleEntry.path is not immutable")
    scene.play(plan)

    single = scene.geometry.circle(6).animate.fade_in().duration(0.1)
    scene.play(single)
    try:
        scene.play(scene.geometry.circle(6).animate.fade_in(), lag=0.1)
    except TypeError:
        pass
    else:
        failures.append("Scene.play still accepts legacy lag=")

    stretched = module.stagger(
        scene.geometry.dot(4).animate.fade_in(),
        scene.geometry.dot(4).animate.fade_in(),
        each=0.5,
    ).stretch(3.0)
    if stretched.schedule().span != 3.0:
        failures.append("Composition.stretch did not produce the requested span")
    return failures


def validate_easing_contract(module) -> list[str]:
    failures: list[str] = []
    preset_names = (
        "LINEAR", "SMOOTH", "DOUBLE_SMOOTH", "THERE_AND_BACK",
        "LINGERING", "RUNNING_START", "EXPONENTIAL_DECAY", "NOT_QUITE_THERE",
    )
    curve_names = (
        "QUADRATIC", "CUBIC", "QUARTIC", "QUINTIC", "EXPONENTIAL",
        "SINE", "CIRCULAR", "BACK", "ELASTIC", "BOUNCE",
    )
    for name in preset_names:
        if not isinstance(getattr(module.Easing, name, None), module.Easing):
            failures.append(f"Easing.{name} is missing or has the wrong type")
    for name in curve_names:
        if not isinstance(getattr(module.EasingCurve, name, None), module.EasingCurve):
            failures.append(f"EasingCurve.{name} is missing or has the wrong type")

    curves = [getattr(module.EasingCurve, name) for name in curve_names]
    easings = [
        *(module.Easing.ease_in(curve) for curve in curves),
        *(module.Easing.ease_out(curve) for curve in curves),
        *(module.Easing.ease_in_out(curve) for curve in curves),
        module.Easing.spring(stiffness=90, damping=12),
        module.Easing.steps(4),
        module.Easing.mirror(module.Easing.SMOOTH),
        module.Easing.there_and_back(pause=0.25),
        module.Easing.cubic_bezier(0.25, 0.1, 0.25, 1.0),
    ]
    if not all(isinstance(easing, module.Easing) for easing in easings):
        failures.append("one or more Easing factories returned the wrong type")

    for call in (
        lambda: module.Easing.spring(stiffness=0, damping=12),
        lambda: module.Easing.spring(stiffness=90, damping=-1),
        lambda: module.Easing.spring(stiffness=math.nan, damping=12),
        lambda: module.Easing.steps(0),
        lambda: module.Easing.steps(-1),
        lambda: module.Easing.there_and_back(pause=-0.1),
        lambda: module.Easing.there_and_back(pause=1.1),
        lambda: module.Easing.cubic_bezier(-0.1, 0, 0.5, 1),
        lambda: module.Easing.cubic_bezier(0.1, 0, 1.1, 1),
        lambda: module.Easing.cubic_bezier(0.1, math.inf, 0.9, 1),
    ):
        try:
            call()
        except (TypeError, ValueError, OverflowError):
            pass
        else:
            failures.append("an Easing factory accepted invalid input")

    try:
        module.Easing()
    except TypeError:
        pass
    else:
        failures.append("Easing unexpectedly exposes a public constructor")
    try:
        module.Easing.SMOOTH.custom = 1
    except (AttributeError, TypeError):
        pass
    else:
        failures.append("Easing instances are mutable")

    for call in (
        lambda: module.Easing.ease_in("cubic"),
        lambda: module.Easing.mirror("smooth"),
        lambda: module.Easing.spring(stiffness="fast", damping=12),
        lambda: module.Easing.steps(1.5),
    ):
        try:
            call()
        except (TypeError, ValueError):
            pass
        else:
            failures.append("an Easing factory accepted an incorrect type")

    scene = module.Scene(frame=(16, 9))
    dot = scene.geometry.dot(8)
    anim = dot.animate.shift_by(10, 0)
    for legacy in (
        "ease", "rate", "smooth", "spring", "linear", "steps", "color",
        "stroke_color",
    ):
        if hasattr(anim, legacy):
            failures.append(f"Anim still exposes legacy member {legacy}")
    for legacy in ("at_anchor", "color"):
        if hasattr(dot, legacy):
            failures.append(f"Drawable still exposes legacy member {legacy}")

    for call in (
        lambda: anim.easing("smooth"),
        lambda: scene.play([anim], rate="linear"),
        lambda: module.sequence(anim).defaults(rate="linear"),
        lambda: module.sequence(anim).schedule(easing=module.Easing.LINEAR),
        lambda: dot.animate.fade_in(0.2),
        lambda: scene.text("x").animate.write(0.2),
        lambda: scene.viz.parameter(0).animate.set(1, 0.2),
        lambda: scene.camera.animate.pan_to(0, 0, 0.2),
        lambda: scene.geometry.circle(r=8),
        lambda: scene.geometry.rect(w=10, h=10),
    ):
        try:
            call()
        except (AttributeError, TypeError):
            pass
        else:
            failures.append("a removed easing, duration, or abbreviated API was accepted")

    for invalid_seconds in (-1.0, math.nan, math.inf):
        try:
            anim.duration(seconds=invalid_seconds)
        except ValueError:
            pass
        else:
            failures.append("Anim.duration accepted invalid seconds")
        try:
            anim.delay(seconds=invalid_seconds)
        except ValueError:
            pass
        else:
            failures.append("Anim.delay accepted invalid seconds")
    return failures


def validate_scene_capability_surface(module) -> list[str]:
    """Keep Scene limited to orchestration and scene-owned capabilities."""
    expected = {
        "assets", "camera", "canvas", "geometry", "layout", "mechanics",
        "media", "slides", "text", "viz", "fade_out_all", "link", "persist",
        "play", "release", "render", "reuse", "sections", "segment", "snapshots", "stop",
        "wait", "time", "cursor", "stops", "random", "noise",
        "voiceover", "live_take", "narration_script",
    }
    actual = {name for name in dir(module.Scene) if not name.startswith("_")}
    failures = []
    if actual != expected:
        failures.append(
            f"Scene public surface differs: missing={sorted(expected - actual)}, "
            f"unexpected={sorted(actual - expected)}"
        )

    scene = module.Scene(frame=(16, 9))
    first = scene.geometry.circle(20)
    second = scene.geometry.circle(20).move_to(10, 0)
    if not isinstance(scene.geometry.union(first, second), module.Drawable):
        failures.append("repeated geometry capability access did not share the Scene model")

    foreign = module.Scene(frame=(16, 9)).geometry.circle(20)
    try:
        scene.geometry.union(first, foreign)
    except ValueError:
        pass
    else:
        failures.append("a capability accepted a drawable owned by another Scene")
    return failures


def validate_section_and_arrow_contract(module) -> list[str]:
    from gaanim import Section, SectionStep

    failures = []
    scene = module.Scene(frame=(16, 9))
    seen = []
    def body(original):
        if original is not scene:
            failures.append("Section replaced the native Scene")
        arrow = original.geometry.arrow(0, 0, 2, 0, head_length=0.18,
            head_width=0.15, body_width=0.036, max_head_ratio=0.3).no_stroke()
        original.play(arrow.animate.create(), duration=0.1)
        original.stop()
        original.wait(0.1)
        original.stop()

    section = Section("contract", [SectionStep(name="one", build=body),
        SectionStep(name="two", build=body, notes="notes")])
    for _ in range(2):
        handles = section.build(scene, on_enter=lambda s, p: seen.append(p))
        if not all(isinstance(handle, module.Segment) for handle in handles):
            failures.append("Section did not return native segment handles")
    if [p.fraction for p in seen] != [0.5, 1, 0.5, 1]:
        failures.append("Section entry progress changed with internal stops")
    for kwargs in ({"head_length": 0}, {"head_width": float("nan")},
                   {"body_width": -1}, {"max_head_ratio": 1.1},
                   {"max_head_ratio": 0}):
        try:
            scene.geometry.arrow(0, 0, 1, 0, **kwargs)
        except ValueError:
            pass
        else:
            failures.append(f"arrow accepted invalid dimensions: {kwargs}")
        for name, args in (("curved_arrow", (0, 0, 1, 0, 0.8)),
                           ("curved_arrow_arc", (0, 0, 1, 0, 1.2))):
            try:
                getattr(scene.geometry, name)(*args, **kwargs)
            except ValueError:
                pass
            else:
                failures.append(f"{name} accepted invalid dimensions: {kwargs}")
    for name, args in (("curved_arrow", (0, 0, 1, 0, 0.8)),
                       ("curved_arrow_arc", (0, 0, 1, 0, 1.2))):
        getattr(scene.geometry, name)(*args, head_length=0.3, head_width=0.24,
                                      body_width=0.06, max_head_ratio=0.3)
    return failures


def validate_section_navigation_contract(module) -> list[str]:
    from gaanim import Agenda, ProgressRail, Section, SectionStep

    failures = []
    scene = module.Scene(frame=(16, 9))
    step = SectionStep(name="one", build=lambda scene: scene.wait(0.1))
    sections = [Section("intro", [step], title="Introducción"), Section("method", [step, step])]

    def row(scene, entry, state):
        number = scene.text(f"{entry.index + 1:02d}").move_to(0, 0)
        return scene.geometry.group([number, scene.text(entry.title).move_to(0.6, 0)])

    agenda = scene.sections.agenda(sections, item=row, pitch=0.6,
                                   marker=lambda scene: scene.geometry.rect(0.05, 0.4))
    if not isinstance(agenda, Agenda) or not isinstance(agenda.root, module.Drawable):
        failures.append("scene.sections.agenda did not return an Agenda with a root drawable")
    if agenda.current is not None or agenda.items("upcoming") != tuple(
            agenda.item(i) for i in range(2)):
        failures.append("an agenda without current did not start upcoming")
    rail = scene.sections.progress_rail(sections, segmented=True, captions=True)
    if not isinstance(rail, ProgressRail) or len(rail.fills) != 2 or len(rail.captions) != 2:
        failures.append("a segmented rail did not build one fill and caption per section")
    seen = []

    def enter(scene, progress):
        seen.append(rail.fraction(progress))
        scene.play(rail.animate.to(progress), duration=0.1)

    for section in sections:
        scene.play([agenda.animate.focus(section), rail.animate.enter(section)], duration=0.1)
        section.build(scene, on_enter=enter)
    if agenda.state("intro") != "done" or agenda.current.key != "method":
        failures.append("agenda focus did not move the current entry")
    if seen != [0.5, 0.75, 1.0]:
        failures.append(f"rail fractions did not follow section progress: {seen}")
    agenda.advance(-5)
    if agenda.current.key != "intro":
        failures.append("agenda.advance did not clamp to the first entry")
    for build in (lambda: scene.sections.agenda([]),
                  lambda: scene.sections.agenda(["a", "a"]),
                  lambda: scene.sections.agenda(["a"], styles={"active": None}),
                  lambda: scene.sections.progress_rail(segmented=True),
                  lambda: scene.sections.progress_rail(["a"], length=0)):
        try:
            build()
        except ValueError:
            pass
        else:
            failures.append("section navigation accepted invalid options")
    try:
        agenda.focus("missing")
    except KeyError:
        pass
    else:
        failures.append("agenda.focus accepted an unknown key")
    return failures


def validate_reactive_fill_level_contract(module) -> list[str]:
    from gaanim import computed

    scene = module.Scene(frame=(16, 9))
    amount = scene.viz.parameter(0)
    fraction = computed(lambda value: value / 100, inputs=[amount])
    mask = scene.geometry.rect(2, 2)
    fill = scene.geometry.fill_level(mask, "blue", fraction, keep_outline=False)
    failures = []
    try:
        fill.animate.fill_level(0.5)
    except ValueError:
        pass
    else:
        failures.append("bound fill accepted a conflicting level animation")
    scene.play(amount.animate.set(80), duration=1)
    fill.set_fill_level(0.4)
    scene.play(fill.animate.fill_level(0.6), duration=1)
    fill.set_fill_level(amount)
    foreign = module.Scene(frame=(16, 9)).viz.parameter(0.3)
    for operation in (lambda: fill.set_fill_level(foreign),
                      lambda: scene.geometry.fill_level(mask, "blue", foreign),
                      lambda: mask.set_fill_level(amount),
                      lambda: fill.set_fill_level(float("nan"))):
        try:
            operation()
        except ValueError:
            pass
        else:
            failures.append("reactive fill accepted an invalid target/source")
    return failures


def validate_polyline_connector_contract(module):
    failures = []
    scene = module.Scene(frame=(16, 9))
    left = scene.geometry.rect(2, 1).move_to(-3, 0)
    right = scene.geometry.rect(2, 1).move_to(3, 0)
    connector = scene.geometry.connector(
        left.anchor_point(module.Anchor.RIGHT), right.anchor_point(module.Anchor.LEFT),
        via=[(0, -1)], max_head_ratio=0.3,
    )
    scene.play(connector.animate.create().duration(0.2))
    scene.play(right.animate.shift_by(0, 1).duration(0.2))
    scene.geometry.connector(left, right)
    scene.geometry.connector((0, 0), (0, 0), via=[(0, 0)])
    foreign = module.Scene(frame=(16, 9)).geometry.rect(1, 1)
    for operation in (
        lambda: scene.geometry.connector(foreign, right),
        lambda: scene.geometry.connector(left, right, via=[foreign.anchor_point(module.Anchor.LEFT)]),
        lambda: scene.geometry.connector(left, right, head_length=0),
        lambda: scene.geometry.connector(left, right, body_width=float("nan")),
        lambda: scene.geometry.connector(left, right, max_head_ratio=1.1),
        lambda: scene.geometry.connector((float("nan"), 0), right),
    ):
        try:
            operation()
        except ValueError:
            pass
        else:
            failures.append("connector accepted invalid dimensions or foreign endpoints")
    return failures


def validate_layout_card_ports_contract(module):
    failures = []
    scene = module.Scene(frame=(16, 9))
    child = scene.geometry.rect(1, 1)
    nested = scene.layout.column([child])
    card = scene.layout.card([nested], padding=0.2, background="white", border="black",
                             ports={"in": module.Anchor.LEFT, "out": (module.Anchor.RIGHT, (0.1, 0))})
    if card.count != 1 or card.background is None:
        failures.append("card background must not count as content")
    if card.move_to(-2, 0) is not card or card.shift_by(0.1, 0) is not card:
        failures.append("positioning lost the Layout subclass")
    if card.with_port("bottom", module.Anchor.BOTTOM) is not card:
        failures.append("with_port lost the Layout subclass")
    scene.geometry.connector(card.port("out"), (5, 0))
    scene.play(card.animate.fade_in().duration(0.2))
    added = scene.geometry.rect(2, 1)
    card.add(added)
    card.remove(added)
    if scene.layout.stack([]).background is not None:
        failures.append("ordinary layout unexpectedly has a background")
    try:
        card.port("missing")
    except KeyError:
        pass
    else:
        failures.append("unknown port must raise KeyError")
    for operation in (
        lambda: card.with_port("in", module.Anchor.RIGHT),
        lambda: card.with_port("", module.Anchor.LEFT),
        lambda: card.with_port("bad", module.Anchor.LEFT, offset=(float("nan"), 0)),
        lambda: scene.layout.card([], radius=-1),
        lambda: scene.layout.card([], border_width=float("nan")),
        lambda: scene.layout.card([], direction="diagonal"),
        lambda: scene.layout.card([], ports={"": module.Anchor.LEFT}),
    ):
        try:
            operation()
        except ValueError:
            pass
        else:
            failures.append("card or port accepted invalid arguments")
    return failures


def validate_narration_contract(module: object) -> list[str]:
    """Voiceover takes set the pace; a live take turns stops into holds."""
    import json
    import tempfile
    import warnings
    import wave

    failures: list[str] = []
    with tempfile.TemporaryDirectory() as directory:
        narration = Path(directory) / "narration"
        narration.mkdir()

        def record(key: str, seconds: float, sidecar: dict) -> None:
            with wave.open(str(narration / f"{key}.wav"), "wb") as take:
                take.setnchannels(1)
                take.setsampwidth(2)
                take.setframerate(8000)
                take.writeframes(b"\0\0" * int(8000 * seconds))
            (narration / f"{key}.json").write_text(json.dumps(sidecar), encoding="utf-8")

        record("intro", 4.0, {"markers": {"recta": 1.5}})
        record("clase", 6.0, {"holds": [2.0]})

        scene = module.Scene(frame=(16, 9))
        scene.assets.assets_dir(directory)
        scene.wait(0.5)
        with scene.voiceover("intro", text="la recta y su pendiente") as vo:
            if not vo.recorded or vo.duration != 4.0 or vo.start != 0.5:
                failures.append("a recorded voiceover must report its take's start and length")
            if vo.until("recta") != 1.5:
                failures.append("Voiceover.until did not use the tapped marker")
            vo.wait_until("recta")
            if abs(scene.cursor - 2.0) > 1e-9:
                failures.append("Voiceover.wait_until did not advance to the marker")
            with warnings.catch_warnings(record=True) as caught:
                warnings.simplefilter("always")
                vo.wait_until("tangente")
            if not any(issubclass(w.category, UserWarning) for w in caught):
                failures.append("a missing marker must emit a UserWarning")
        if abs(scene.cursor - 4.5) > 1e-9:
            failures.append("leaving a voiceover block must wait for the rest of the take")
        try:
            vo.wait_until("recta")
        except ValueError:
            pass
        else:
            failures.append("a finished voiceover accepted another marker")

        estimated = scene.voiceover("resumen", text="uno dos tres cuatro cinco")
        if estimated.recorded or abs(estimated.duration - 2.4) > 1e-9:
            failures.append("an unrecorded voiceover must be estimated from its text")
        estimated.finish()

        (Path(directory) / "guion.md").write_text(
            "# Guion\n\n## desde-guion\nuno dos tres cuatro cinco\n", encoding="utf-8"
        )
        scene.narration_script("guion.md")
        scripted = scene.voiceover("desde-guion")
        if scripted.text != "uno dos tres cuatro cinco" or abs(scripted.duration - 2.4) > 1e-9:
            failures.append("a voiceover without text must read its narration script section")
        scripted.finish()
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            scene.voiceover("sin-seccion").finish()
        if not any(issubclass(w.category, UserWarning) for w in caught):
            failures.append("a key missing from the narration script must warn")

        scene.live_take("clase")
        start = scene.cursor
        scene.wait(1.0)
        scene.stop()
        if abs(scene.cursor - (start + 3.0)) > 1e-9 or scene.stops:
            failures.append("a recorded live take must replace stops with its holds")
        for invalid in ("a/b", "intro"):
            try:
                scene.voiceover(invalid)
            except ValueError:
                pass
            else:
                failures.append(f"voiceover accepted the key {invalid!r}")
    return failures


def validate_text_animator_contract(module) -> list[str]:
    """Text range animator, masked reveals, blur-in and tracking (TX-01/02/05)."""
    failures: list[str] = []
    scene = module.Scene(frame=(16, 9))
    title = scene.text("uno dos\ntres")
    wave = title.animator(by="word", shape="round", order="random", seed=3)
    if wave.set(offset=(0, -0.4), opacity=0.0, scale=0.6, rotation=0.2) is not wave:
        failures.append("TextAnimator.set must return the animator")
    if not isinstance(wave.animate, module.TextAnimatorAnimation):
        failures.append("TextAnimator.animate did not return its typed proxy")
    for anim in (
        wave.animate.sweep().duration(1.2),
        wave.animate.sweep(1.0, 0.0, stagger=0.05),
        title.animate.reveal(by="line", style="slide_up", mask=True, stagger=0.06),
        title.animate.reveal(by="word", style="blur", stagger=0.04),
        title.animate.conceal(by="line", style="slide_up"),
        title.animate.blur_in(sigma=0.3, by="grapheme", stagger=0.02),
        title.animate.tracking(0.0),
    ):
        if not isinstance(anim, module.Anim):
            failures.append(f"text animator preset returned {type(anim).__name__}, not Anim")
    if title.tracking(0.4) is not title:
        failures.append("Text.tracking must return the Text")
    if not isinstance(title.words[0].animate.reveal("from_below"), module.Anim):
        failures.append("the text selection reveal(style) form stopped working")
    rejected = (
        (ValueError, lambda: title.animator(shape="wobble")),
        (ValueError, lambda: title.animator(by="sentence")),
        (ValueError, lambda: wave.set(opacity=2.0)),
        (ValueError, lambda: title.animate.reveal(style="wipe")),
        (ValueError, lambda: title.animate.blur_in(sigma=-1.0)),
        (TypeError, lambda: title.words[0].animate.reveal("fade", by="word")),
        (TypeError, lambda: scene.geometry.circle(1.0).animate.blur_in()),
        (ValueError, lambda: title.animate.opacity(0.5).tracking(0.2)),
    )
    for error, build in rejected:
        try:
            build()
        except error:
            pass
        else:
            failures.append(f"a text animator call did not raise {error.__name__}")
    scene.play([title.animate.reveal(by="line"), wave.animate.sweep()])
    return failures


def main() -> int:
    tree = ast.parse(STUB.read_text(encoding="utf-8"), filename=str(STUB))
    module = importlib.import_module("gaanim.gaanim_core")
    missing: list[str] = []

    try:
        module.Scene(1280, 720)
    except TypeError:
        pass
    else:
        missing.append("Scene still accepts the removed positional pixel constructor")

    try:
        module.Scene(frame=(0, 9))
    except ValueError:
        pass
    else:
        missing.append("Scene accepted a non-positive logical frame")

    for node in tree.body:
        if isinstance(node, ast.ClassDef):
            native_class = getattr(module, node.name, None)
            if native_class is None:
                missing.append(node.name)
                continue
            for member in declared_members(node):
                if not hasattr(native_class, member):
                    missing.append(f"{node.name}.{member}")
        elif (
            isinstance(node, ast.AnnAssign)
            and isinstance(node.target, ast.Name)
            and not is_type_alias(node)
            and node.target.id not in TYPE_CHECKING_ONLY
            and not hasattr(module, node.target.id)
        ):
            missing.append(node.target.id)
        elif (
            isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
            and node.name in {"part", "parts", "parallel", "sequence", "stagger"}
        ):
            if not hasattr(module, node.name):
                missing.append(node.name)

    missing.extend(validate_visualization_contract(module))
    missing.extend(validate_layout_detach_contract(module))
    missing.extend(validate_reactive_connector_contract(module))
    missing.extend(validate_camera_rig_contract(module))
    missing.extend(validate_camera_motion_contract(module))
    missing.extend(validate_matrix_contract(module))
    missing.extend(validate_matrix_stub_typing())
    missing.extend(validate_vector_geometry_contract(module))
    missing.extend(validate_composition_contract(module))
    missing.extend(validate_composable_properties_contract(module))
    missing.extend(validate_easing_contract(module))
    missing.extend(validate_scene_capability_surface(module))
    missing.extend(validate_section_and_arrow_contract(module))
    missing.extend(validate_section_navigation_contract(module))
    missing.extend(validate_reactive_fill_level_contract(module))
    missing.extend(validate_polyline_connector_contract(module))
    missing.extend(validate_layout_card_ports_contract(module))
    missing.extend(validate_editorial_contract(module))
    missing.extend(validate_theme_typography_contract(module))
    missing.extend(validate_timeline_cursor_contract(module))
    missing.extend(validate_narration_contract(module))
    missing.extend(validate_text_animator_contract(module))
    missing.extend(validate_runtime_type_aliases(module))
    missing.extend(documented_text_api_failures(tree))
    missing.extend(documented_editorial_api_failures(tree))

    if missing:
        print("Stub declarations missing from gaanim.gaanim_core:", file=sys.stderr)
        print("\n".join(f"- {name}" for name in missing), file=sys.stderr)
        return 1

    print("Python stub matches the embedded PyO3 surface.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
