"""Check that the shipped Python stub matches the embedded PyO3 surface."""

from __future__ import annotations

import ast
import importlib
import inspect
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
STUB = ROOT / "crates" / "gaanim_python" / "gaanim" / "gaanim_core.pyi"
TYPE_CHECKING_ONLY = {
    "CurvePoint",
    "CurveControl",
    "CurveCommand",
    "ColorLike",
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
        elif isinstance(node, ast.ClassDef) and node.name == "Scene":
            callables.extend(
                (f"Scene.{child.name}", child)
                for child in node.body
                if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef))
                and child.name in {"text", "equation"}
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
    scene = next(
        (node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "Scene"),
        None,
    )
    if scene is None:
        return ["Scene is missing from the Python stub"]
    methods = {
        child.name: child
        for child in scene.body
        if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef))
    }
    for name in EDITORIAL_SCENE_MEMBERS:
        method = methods.get(name)
        if method is None:
            failures.append(f"Scene.{name} is missing from the stub")
            continue
        doc = ast.get_docstring(method, clean=False)
        if not doc:
            failures.append(f"Scene.{name} is missing a docstring")
        elif "Example:" not in doc:
            failures.append(f"Scene.{name} is missing an Example: block")
    return failures


def validate_editorial_contract(module: object) -> list[str]:
    """Exercise the editorial kit without starting the renderer."""
    failures: list[str] = []
    scene = module.Scene(1280, 720, margin=48)
    signature = inspect.signature(module.Scene.section_header)
    if signature.parameters["rule"].default is not False:
        failures.append("Scene.section_header must hide its rule by default")
    components = (
        scene.badge("Ready", variant="success"),
        scene.chip("Live", variant="danger", appearance="solid"),
        scene.card("Result", "The solver converged.", "12 ms"),
        scene.banner("Simulation complete", position="bottom"),
        scene.lower_third("Ada Lovelace", "Mathematician", kicker="Speaker"),
        scene.stat_card("98%", "Accuracy", delta="+4.2%", variant="success"),
        scene.quote_card("Clarity matters.", "Gaanim"),
        scene.section_header("Method", kicker="02", align="center"),
    )
    if not all(isinstance(component, module.Drawable) for component in components):
        failures.append("one or more editorial factories did not return Drawable")
    animations = [component.fade_in(duration=0.1) for component in components]
    if not all(isinstance(animation, module.Anim) for animation in animations):
        failures.append("one or more editorial groups did not preserve Drawable animations")
    else:
        scene.play(animations)

    invalid_calls = (
        lambda: scene.badge(""),
        lambda: scene.badge("x", variant="unknown"),
        lambda: scene.chip("x", appearance="glass"),
        lambda: scene.card("x", width=20.0, padding=(12.0, 4.0)),
        lambda: scene.banner("x", position="center"),
        lambda: scene.lower_third("x", side="center"),
        lambda: scene.stat_card("", "label"),
        lambda: scene.quote_card("x", width=float("nan")),
        lambda: scene.section_header("x", align="justify"),
    )
    for call in invalid_calls:
        try:
            call()
        except ValueError:
            pass
        else:
            failures.append("an editorial factory accepted invalid authored input")
    if hasattr(module.Scene, "caption"):
        failures.append("removed Scene.caption remains public")
    try:
        scene.badge("legacy", 0.0, 0.0)
    except TypeError:
        pass
    else:
        failures.append("Scene.badge still accepts legacy positional coordinates")
    return failures


def validate_visualization_contract(module: object) -> list[str]:
    """Exercise the native builders without starting the renderer."""
    failures: list[str] = []
    scene = module.Scene(640, 360)
    x_axis = module.Axis.linear(-4.0, 4.0).ticks(1.0).label("x")
    y_axis = module.Axis.symlog(-10.0, 10.0, threshold=1.0).label("y")
    space = scene.cartesian_2d(x_axis, y_axis, width=520.0, height=280.0)

    amplitude = scene.parameter(1.0)
    from gaanim import math as gm
    space.function(lambda x: amplitude * gm.sin(x))
    readout = scene.readout(lambda: amplitude * 2.0, label="$a$")
    variable = scene.variable(1.0, label="$k$")
    if not isinstance(readout, module.Drawable) or not isinstance(variable, module.Drawable):
        failures.append("reactive readouts must be Drawable instances")
    if hasattr(module, "Expr") or hasattr(module, "ValueTracker"):
        failures.append("legacy reactive classes remain public")
    space.parametric(lambda t: (t, t * t), (-1.0, 1.0), samples=32)
    space.implicit(lambda px, py: px * px + py * py - 1.0, resolution=(16, 16))
    space.vector_field(lambda px, py: (-py, px), resolution=(4, 4))
    space.tangent(lambda value: value * value, 1.0)
    space.riemann_sum(lambda value: value * value, (0.0, 2.0), rectangles=4)

    chart_spec = module.ChartSpec(
        {"id": ["a", "b"], "x": [0.0, 1.0], "y": [1.0, 2.0]}, key="id"
    ).mark("line").encode(x="x", y="y")
    chart = scene.chart(chart_spec)
    if not isinstance(chart.drawable(), module.Drawable):
        failures.append("Scene.chart did not materialize a Drawable hierarchy")

    coordinate = space.coord(1.0, 2.0)
    coordinate.place(scene.dot(3.0))
    local = space.data_to_local(1.0, 2.0)
    round_trip = space.local_to_data(*local)
    if abs(round_trip[0] - 1.0) > 1e-9 or abs(round_trip[1] - 2.0) > 1e-9:
        failures.append("CoordinateSpace data/local round trip failed")

    linear_space = scene.cartesian_2d(
        module.Axis.linear(-4.0, 4.0),
        module.Axis.linear(-3.0, 3.0),
    )
    if len(linear_space.animate_view((-2.0, 2.0), (-1.5, 1.5))) != 2:
        failures.append("CoordinateSpace.animate_view did not return pan/zoom animations")

    space_3d = scene.cartesian_3d(
        module.Axis.linear(-2.0, 2.0),
        module.Axis.linear(-2.0, 2.0),
        module.Axis.linear(-2.0, 2.0),
        size=(4.0, 4.0, 4.0),
    )
    space_3d.surface(lambda px, py: px * px - py * py, resolution=(8, 8))
    space_3d.parametric(lambda t: (t, t * t, t * t * t), (-1.0, 1.0), samples=16)
    space_3d.vector_field(lambda px, py, pz: (-py, px, -pz), resolution=(2, 2, 2))

    for removed in ("axes", "number_plane", "axes_3d", "polar_plane", "complex_plane"):
        if hasattr(module.Scene, removed):
            failures.append(f"v1 Scene.{removed} remains public")
    for removed in ("line", "step", "area", "scatter", "bars", "histogram", "box_plot", "violin", "error_bars", "heatmap"):
        if hasattr(module.CoordinateSpace, removed):
            failures.append(f"tabular CoordinateSpace.{removed} remains public")

    for removed in ("plot", "get_graph", "function_graph", "parametric_curve", "bar_chart"):
        if hasattr(module.Scene, removed):
            failures.append(f"legacy Scene.{removed} remains public")
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

    if compact_text.become(
        "$", module.parts(first="x", second="y"), "$", duration=0.1
    ) is not None:
        failures.append("Text.become with TextParts did not preserve its None return")
    for name in ("first", "second"):
        if name not in compact_text.parts:
            failures.append(f"Text.become did not install TextParts entry {name!r}")

    equation = scene.equation(
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
    if not isinstance(equation.write(0.5, by="part"), module.Anim):
        failures.append("Scene.equation did not preserve Text animations")
    codex_equation = scene.equation(
        module.parts(gravity="g sin(theta)", acceleration="theta''")
    )
    codex_animations = (
        codex_equation["gravity"].indicate(0.3),
        codex_equation["acceleration"].color_to(module.GOLD, duration=0.3),
    )
    if not all(isinstance(animation, module.Anim) for animation in codex_animations):
        failures.append("Typst/Codex semantic selections did not return animations")
    else:
        scene.play(list(codex_animations))
    try:
        scene.equation()
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
    positional_write = text.write(0.6, by="word")
    if not isinstance(positional_write, module.Anim):
        failures.append("Text.write did not accept positional duration")
    else:
        scene.play([positional_write])
    selection_anim = text["formula"]["mass"].indicate()
    if not isinstance(selection_anim, module.Anim):
        failures.append("TextSelection.indicate did not return Anim")
    else:
        scene.play([selection_anim])
    selection_color = text["formula"]["mass"].color_to(module.RED, duration=0.4)
    selection_opacity = text["formula"]["mass"].opacity_to(0.6, duration=0.4)
    selection_compound = text["formula"]["mass"].animate().fill(module.BLUE).opacity(0.8)
    if not all(
        isinstance(anim, module.Anim)
        for anim in (selection_color, selection_opacity, selection_compound)
    ):
        failures.append("TextSelection color/opacity animations did not return Anim")
    else:
        scene.play([selection_color, selection_opacity, selection_compound])
    try:
        text["formula"]["mass"].animate().scale(2.0)
    except TypeError:
        pass
    else:
        failures.append("TextSelection.animate accepted an unsupported scale target")
    target_text = scene.text(
        "Energy: ",
        module.part(
            "formula", "$E = ", module.part("mass", "m", color=module.BLUE), " c^2$"
        ),
    )
    text_transition = text.step_to(target_text)
    if not isinstance(text_transition, module.Anim):
        failures.append("Text.step_to did not return Anim")
    else:
        scene.play([text_transition])
    chained_text = scene.text("Chain").fill(module.WHITE).at(
        0.0, 0.0, module.Anchor.TOP_LEFT
    )
    if not isinstance(chained_text, module.Text):
        failures.append("Text fluent styling or positioning erased the Text subtype")
    baseline_text = scene.text("Baseline").at(
        0.0, 0.0, anchor=module.TextAnchor.BASELINE_LEFT
    )
    baseline_equation = scene.equation("x_1 = 2").at_anchor(
        0.0, 0.0, module.TextAnchor.BASELINE_RIGHT
    )
    if not all(isinstance(value, module.Text) for value in (baseline_text, baseline_equation)):
        failures.append("TextAnchor positioning did not preserve Text/Equation handles")
    anchored_drawable = scene.rect(40.0, 20.0).at(20.0, 10.0, anchor=module.Anchor.RIGHT)
    if not isinstance(anchored_drawable, module.Drawable):
        failures.append("Drawable.at with an anchor did not return Drawable")
    reference_drawable = scene.dot(6.0).at(-25.0, 15.0)
    centered_drawable = scene.rect(40.0, 20.0).at(reference_drawable)
    if not isinstance(centered_drawable, module.Drawable):
        failures.append("Drawable.at with a reference did not return Drawable")
    centered_text = scene.text("Centered").at(reference_drawable)
    if not isinstance(centered_text, module.Text):
        failures.append("Text.at with a reference erased the Text subtype")
    centered_primitive = scene.cube().at(reference_drawable)
    if not isinstance(centered_primitive, module.Primitive3D):
        failures.append("Primitive3D.at with a reference erased the Primitive3D subtype")
    try:
        scene.rect(40.0, 20.0).at(reference_drawable, anchor=module.Anchor.TOP)
    except TypeError:
        pass
    else:
        failures.append("Drawable.at accepted an anchor with a reference")
    try:
        scene.rect(40.0, 20.0).at(10.0)
    except TypeError:
        pass
    else:
        failures.append("Drawable.at accepted a numeric x without y")
    try:
        scene.rect(40.0, 20.0).at(
            0.0, 0.0, anchor=module.TextAnchor.BASELINE_CENTER
        )
    except TypeError:
        pass
    else:
        failures.append("Drawable.at accepted a TextAnchor")
    if not isinstance(
        anchored_drawable.move_to(80.0, 40.0, anchor=module.Anchor.TOP_RIGHT),
        module.Anim,
    ):
        failures.append("Drawable.move_to with an anchor did not return Anim")
    if not isinstance(
        anchored_drawable.animate().move_to(80.0, 40.0, anchor=module.Anchor.TOP_RIGHT),
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
    scene = module.Scene(640, 360)
    scene.segment("cover")
    title = scene.text("Reusable title", role="title")
    body = scene.text("Body")
    page = scene.column([title, body], width="fill", align="center")
    scene.segment("detail", module.Transition.cross_fade(0.2))
    scene.reuse(title)
    page.detach(title)
    try:
        movement = title.move_to(0.0, 120.0)
    except module.LayoutOwnershipError:
        failures.append("Layout.detach did not release positional ownership")
    else:
        if not isinstance(movement, module.Anim):
            failures.append("a detached child move_to did not return Anim")
    return failures


def validate_reactive_connector_contract(module: object) -> list[str]:
    """Exercise anchored endpoints, bars, springs, and rich dimensions."""
    failures: list[str] = []
    scene = module.Scene(640, 360)
    frame = scene.rect(240.0, 120.0)
    corner = frame.anchor_point(module.Anchor.TOP_RIGHT, offset=(5.0, -3.0))
    if not isinstance(corner, module.AnchorPoint):
        failures.append("Drawable.anchor_point did not return AnchorPoint")

    bar = scene.bar_between((-180.0, 120.0), corner, width=9.0)
    spring = scene.spring_between(
        frame, corner, coils=6, amplitude=10.0, start_straight=8.0, end_straight=14.0
    )
    if not isinstance(bar, module.Drawable) or not isinstance(spring, module.Drawable):
        failures.append("reactive bar or spring did not return Drawable")
    try:
        scene.spring_between((0.0, 0.0), (100.0, 0.0), start_straight=-1.0)
    except ValueError:
        pass
    else:
        failures.append("Scene.spring_between accepted a negative start_straight")

    dimension = scene.dimension_between(
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

    physical_width = scene.parameter(2.5)
    semantic_dimension = scene.dimension_between(
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
    value_animation = physical_width.animate_to(4.0, duration=0.4)
    if not isinstance(value_animation, module.Anim):
        failures.append("dimension value Parameter did not remain animatable")
    else:
        scene.play([value_animation])

    try:
        scene.dimension_between((0.0, 0.0), (1.0, 0.0), 10.0, value="invalid")
    except TypeError:
        pass
    else:
        failures.append("Scene.dimension_between accepted an invalid semantic value")

    for invalid in (
        {"line_width": 0.0},
        {"extension_style": "dots"},
        {"dash_length": 0.0},
        {"gap_length": float("nan")},
    ):
        try:
            scene.dimension_between((0.0, 0.0), (1.0, 0.0), 10.0, **invalid)
        except ValueError:
            pass
        else:
            failures.append(f"Scene.dimension_between accepted invalid options: {invalid}")

    try:
        scene.dimension_between((0.0, 0.0), (1.0, 0.0), 10.0, scale=0.0)
    except ValueError:
        pass
    else:
        failures.append("Scene.dimension_between accepted a non-positive scale")
    return failures


def validate_camera_rig_contract(module: object) -> list[str]:
    """Exercise the semantic camera facade and removed compatibility surface."""
    failures: list[str] = []
    scene = module.Scene(640, 360)
    marker = scene.dot(5.0)
    parameter = scene.parameter(1.0)
    point = scene.point_ref(parameter * 20.0, 10.0)

    constraint = scene.camera.bind_2d(center=point, zoom=parameter)
    if not isinstance(constraint, module.CameraConstraint):
        failures.append("Camera.bind_2d did not return CameraConstraint")
    constraint.disable()
    constraint.enable()
    scene.camera.bind_3d(eye=(4.0, 3.0, 8.0), target=point, fov_y=0.8).disable()

    animations = (
        scene.camera.pan_to(point, duration=0.0),
        scene.camera.zoom_to(parameter, duration=0.0),
        scene.camera.rotate_to(parameter * 0.1, duration=0.0),
        scene.camera.frame_to([marker], margin=(10.0, 20.0), dynamic=True, duration=0.0),
        scene.camera.follow(point, offset=(2.0, 3.0), lag=0.2, duration=0.1),
        scene.camera.look_at((4.0, 3.0, 8.0), point, duration=0.0),
        scene.camera.orthographic(1.0, duration=0.0),
        scene.camera.reset(duration=0.0),
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
    )
    for call in invalid_calls:
        try:
            call()
        except ValueError:
            pass
        else:
            failures.append("camera validation accepted an invalid authored value")
    return failures


def validate_matrix_contract(module: object) -> list[str]:
    """Exercise matrix construction, selectors, ordering, and mutations."""
    failures: list[str] = []
    scene = module.Scene(640, 360)
    matrix = scene.matrix([[1, 2, 3], [4, 5, 6]], delimiters="parentheses")
    if matrix.shape != (2, 3):
        failures.append("Scene.matrix returned the wrong shape")
    if not isinstance(matrix[0, 0], module.Drawable):
        failures.append("a matrix cell is not a Drawable")
    if matrix[1, :].coordinates != ((1, 0), (1, 1), (1, 2)):
        failures.append("matrix row selection returned wrong coordinates")
    if matrix.diagonal().coordinates != ((0, 0), (1, 1)):
        failures.append("matrix diagonal selection returned wrong coordinates")
    ordered = matrix.entries.write(0.1, order="spiral_in", stagger=0.01)
    if len(ordered) != 6 or not all(isinstance(animation, module.Anim) for animation in ordered):
        failures.append("matrix ordered animation did not return one Anim per cell")
    random_a = module.MatrixOrder.order(2, 3, matrix.entries.coordinates, "random", 7)
    random_b = module.MatrixOrder.order(2, 3, matrix.entries.coordinates, "random", 7)
    if random_a != random_b:
        failures.append("matrix random order is not reproducible")
    replacement = matrix.set(0, 1, "x", animate=0.1)
    if not isinstance(replacement, module.Drawable):
        failures.append("Matrix.set did not return the replacement Drawable")
    try:
        scene.matrix([[1, 2], [3]])
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


def main() -> int:
    tree = ast.parse(STUB.read_text(encoding="utf-8"), filename=str(STUB))
    module = importlib.import_module("gaanim.gaanim_core")
    missing: list[str] = []

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
            and node.name in {"part", "parts"}
        ):
            if not hasattr(module, node.name):
                missing.append(node.name)

    missing.extend(validate_visualization_contract(module))
    missing.extend(validate_layout_detach_contract(module))
    missing.extend(validate_reactive_connector_contract(module))
    missing.extend(validate_camera_rig_contract(module))
    missing.extend(validate_matrix_contract(module))
    missing.extend(validate_matrix_stub_typing())
    missing.extend(validate_editorial_contract(module))
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
