"""Check that the shipped Python stub matches the native extension surface."""

from __future__ import annotations

import ast
import importlib
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
TEXT_API_CLASSES = {"TextStyle", "TextFlow", "TextSelection", "TextQuery", "Text"}
REMOVED_TEXT_SCENE_MEMBERS = {
    "title",
    "subtitle",
    "paragraph",
    "equation",
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
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == "part":
            callables.append(("part", node))
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
                ("Scene.text", child)
                for child in node.body
                if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef))
                and child.name == "text"
            )

        for name, callable_node in callables:
            doc = ast.get_docstring(callable_node, clean=False)
            if not doc:
                failures.append(f"{name} is missing a docstring")
            elif "Example:" not in doc:
                failures.append(f"{name} is missing an Example: block")
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

    formula = module.part("formula", "$E = ", module.part("mass", "m"), " c^2$")
    text = scene.text("Energy: ", formula)
    if not isinstance(text, module.Text):
        failures.append("Scene.text did not return Text")
    if not isinstance(text["formula"]["mass"], module.TextSelection):
        failures.append("nested semantic text selection did not return TextSelection")
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
    target_text = scene.text("Energy: ", module.part("formula", "$E = mc^2$"))
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
    anchored_drawable = scene.rect(40.0, 20.0).at(20.0, 10.0, anchor=module.Anchor.RIGHT)
    if not isinstance(anchored_drawable, module.Drawable):
        failures.append("Drawable.at with an anchor did not return Drawable")
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
    )
    if not isinstance(dimension, module.Dimension):
        failures.append("Scene.dimension_between did not return Dimension")
    elif not all(
        isinstance(part, module.Drawable)
        for part in (dimension.line, dimension.label, dimension.number, dimension.unit)
    ):
        failures.append("Dimension did not expose its line/label/number/unit parts")

    try:
        scene.dimension_between((0.0, 0.0), (1.0, 0.0), 10.0, scale=0.0)
    except ValueError:
        pass
    else:
        failures.append("Scene.dimension_between accepted a non-positive scale")
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
        elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == "part":
            if not hasattr(module, node.name):
                missing.append(node.name)

    missing.extend(validate_visualization_contract(module))
    missing.extend(validate_layout_detach_contract(module))
    missing.extend(validate_reactive_connector_contract(module))
    missing.extend(documented_text_api_failures(tree))

    if missing:
        print("Stub declarations missing from gaanim.gaanim_core:", file=sys.stderr)
        print("\n".join(f"- {name}" for name in missing), file=sys.stderr)
        return 1

    print("Python stub matches the native extension surface.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
