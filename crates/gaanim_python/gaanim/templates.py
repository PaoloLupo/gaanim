"""Reusable, signature-checked Layout v2 templates.

Templates intentionally remain ordinary Python callables: projects can keep
them beside scene code, type-check them, compose them, and inspect their
signatures without a second configuration language.
"""

from __future__ import annotations

from functools import update_wrapper
from inspect import Signature, signature
from typing import Any, Callable, Generic, ParamSpec, TypeVar

P = ParamSpec("P")
R = TypeVar("R")


class LayoutTemplate(Generic[P, R]):
    """Validated callable produced by :func:`layout_template`."""

    def __init__(self, function: Callable[P, R]) -> None:
        self.function = function
        self.signature: Signature = signature(function)
        parameters = tuple(self.signature.parameters.values())
        if not parameters or parameters[0].name != "scene":
            raise TypeError("a layout template must declare `scene` as its first parameter")
        update_wrapper(self, function)

    @property
    def slots(self) -> tuple[str, ...]:
        return tuple(tuple(self.signature.parameters)[1:])

    def __call__(self, scene: Any, **slots: Any) -> R:
        try:
            bound = self.signature.bind(scene, **slots)
        except TypeError as error:
            raise TypeError(f"invalid slots for template {self.__name__}: {error}") from error
        bound.apply_defaults()
        return self.function(*bound.args, **bound.kwargs)


def layout_template(function: Callable[P, R]) -> LayoutTemplate[P, R]:
    """Turn a typed Python function into a reusable Layout v2 template."""

    return LayoutTemplate(function)


def _present(*values: Any) -> list[Any]:
    return [value for value in values if value is not None]


def _token(scene: Any, name: str) -> float:
    return scene.canvas.layout_token(name)


@layout_template
def title_slide(scene: Any, *, title: Any, subtitle: Any = None, footer: Any = None) -> Any:
    return scene.layout.column(
        _present(title, subtitle, footer),
        within="safe",
        width="fill",
        height="fill",
        padding=(_token(scene, "page_padding_wide"), _token(scene, "page_padding_x")),
        gap=_token(scene, "space_lg"),
        align="center",
        justify="center",
    )


@layout_template
def lecture(scene: Any, *, title: Any, body: Any, footer: Any = None) -> Any:
    return scene.layout.column(
        _present(title, scene.layout.item(body, grow=1, align="stretch"), footer),
        within="safe",
        width="fill",
        height="fill",
        padding=_token(scene, "page_padding"),
        gap=_token(scene, "space_lg"),
        align="stretch",
        justify="start",
    )


@layout_template
def comparison(
    scene: Any,
    *,
    title: Any,
    left: Any,
    right: Any,
    footer: Any = None,
) -> Any:
    columns = scene.layout.row(
        [scene.layout.item(left, grow=1, fit="contain"), scene.layout.item(right, grow=1, fit="contain")],
        width="fill",
        height="fill",
        gap=_token(scene, "column_gap"),
        align="stretch",
    )
    return scene.layout.column(
        _present(title, scene.layout.item(columns, grow=1, align="stretch"), footer),
        within="safe",
        width="fill",
        height="fill",
        padding=_token(scene, "page_padding"),
        gap=_token(scene, "space_lg"),
        align="stretch",
    )


@layout_template
def vertical_short(scene: Any, *, title: Any, body: Any, caption: Any = None) -> Any:
    return scene.layout.column(
        _present(title, scene.layout.item(body, grow=1, fit="contain"), caption),
        within="safe",
        width="fill",
        height="fill",
        padding=(_token(scene, "vertical_padding"), _token(scene, "vertical_padding_x")),
        gap=_token(scene, "space_lg"),
        align="stretch",
        justify="between",
    )


@layout_template
def minimal(scene: Any, *, content: Any) -> Any:
    return scene.layout.stack(
        [scene.layout.item(content, fit="contain")],
        within="safe",
        width="fill",
        height="fill",
        padding=_token(scene, "space_lg"),
        align="center",
    )


@layout_template
def lower_third(scene: Any, *, title: Any, subtitle: Any = None, background: Any = None) -> Any:
    copy = scene.layout.column(_present(title, subtitle), gap=_token(scene, "space_xs"), align="start")
    return scene.layout.stack(
        _present(
            scene.layout.item(background, absolute=True, fit="stretch") if background is not None else None,
            scene.layout.item(
                copy,
                anchor=None,
                offset=(0, -_token(scene, "lower_third_offset")),
            ),
        ),
        within="safe",
        width="fill",
        height="fill",
        align="stretch",
    )


@layout_template
def credits(scene: Any, *, title: Any = None, entries: Any, footer: Any = None) -> Any:
    return scene.layout.column(
        _present(title, scene.layout.item(entries, grow=1, align="center"), footer),
        within="safe",
        width="fill",
        height="fill",
        padding=(_token(scene, "page_padding_wide"), _token(scene, "page_padding_x")),
        gap=_token(scene, "space_md"),
        align="center",
        justify="center",
    )


__all__ = [
    "LayoutTemplate",
    "layout_template",
    "title_slide",
    "lecture",
    "comparison",
    "vertical_short",
    "minimal",
    "lower_third",
    "credits",
]
