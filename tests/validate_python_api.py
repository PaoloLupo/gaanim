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
    space = scene.number_plane(x_axis, y_axis, width=520.0, height=280.0)

    x = module.Expr.var("x")
    amplitude = scene.parameter(1.0)
    space.plot(amplitude.expr() * x.sin())
    space.parametric(lambda t: (t, t * t), (-1.0, 1.0), samples=32)
    space.implicit(lambda px, py: px * px + py * py - 1.0, resolution=(16, 16))
    space.vector_field(lambda px, py: (-py, px), resolution=(4, 4))
    space.tangent(lambda value: value * value, 1.0)
    space.riemann_sum(lambda value: value * value, (0.0, 2.0), rectangles=4)

    source = module.DataSource(
        {"id": ["a", "b"], "x": [0.0, 1.0], "y": [1.0, 2.0]},
        key="id",
    )
    space.line(source, "x", "y")
    source.replace({"id": ["a", "b"], "x": [0.0, 1.0], "y": [2.0, 3.0]})
    if source.version != 1:
        failures.append("DataSource.version did not advance after replace")

    coordinate = space.coord(1.0, 2.0)
    coordinate.place(scene.dot(3.0))
    local = space.data_to_local(1.0, 2.0)
    round_trip = space.local_to_data(*local)
    if abs(round_trip[0] - 1.0) > 1e-9 or abs(round_trip[1] - 2.0) > 1e-9:
        failures.append("CoordinateSpace data/local round trip failed")

    linear_space = scene.axes(
        module.Axis.linear(-4.0, 4.0),
        module.Axis.linear(-3.0, 3.0),
    )
    if len(linear_space.animate_view((-2.0, 2.0), (-1.5, 1.5))) != 2:
        failures.append("CoordinateSpace.animate_view did not return pan/zoom animations")

    space_3d = scene.axes_3d(
        module.Axis.linear(-2.0, 2.0),
        module.Axis.linear(-2.0, 2.0),
        module.Axis.linear(-2.0, 2.0),
        size=(4.0, 4.0, 4.0),
    )
    space_3d.surface(lambda px, py: px * px - py * py, resolution=(8, 8))
    space_3d.parametric(lambda t: (t, t * t, t * t * t), (-1.0, 1.0), samples=16)
    space_3d.vector_field(lambda px, py, pz: (-py, px, -pz), resolution=(2, 2, 2))

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
    chained_text = scene.text("Chain").fill(module.WHITE).at(0.0, 0.0)
    if not isinstance(chained_text, module.Text):
        failures.append("Text fluent styling or positioning erased the Text subtype")
    try:
        scene.text("$unbalanced")
    except ValueError:
        pass
    else:
        failures.append("Scene.text accepted an unbalanced math delimiter")
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
    missing.extend(documented_text_api_failures(tree))

    if missing:
        print("Stub declarations missing from gaanim.gaanim_core:", file=sys.stderr)
        print("\n".join(f"- {name}" for name in missing), file=sys.stderr)
        return 1

    print("Python stub matches the native extension surface.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
