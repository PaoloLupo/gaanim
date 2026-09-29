"""Reusable, signature-checked components built from boxes.

A component is an ordinary Python function whose first parameter is the
scene and whose keyword parameters are its slots. Projects keep components
beside their scene code, type-check them, compose them and call them as many
times as they need: every call builds new, independent drawables.
"""

from __future__ import annotations

from functools import update_wrapper
from inspect import Signature, signature
from typing import Any, Callable, Generic, ParamSpec, TypeVar

P = ParamSpec("P")
R = TypeVar("R")


class Component(Generic[P, R]):
    """Validated callable produced by :func:`component`."""

    def __init__(self, function: Callable[P, R]) -> None:
        self.function = function
        self.signature: Signature = signature(function)
        parameters = tuple(self.signature.parameters.values())
        if not parameters or parameters[0].name != "scene":
            raise TypeError("a component must declare `scene` as its first parameter")
        update_wrapper(self, function)

    @property
    def slots(self) -> tuple[str, ...]:
        return tuple(tuple(self.signature.parameters)[1:])

    def __call__(self, scene: Any, *args: Any, **slots: Any) -> R:
        try:
            bound = self.signature.bind(scene, *args, **slots)
        except TypeError as error:
            raise TypeError(f"invalid arguments for component {self.__name__}: {error}") from error
        bound.apply_defaults()
        return self.function(*bound.args, **bound.kwargs)


def component(function: Callable[P, R]) -> Component[P, R]:
    """Turn a typed Python function ``(scene, ...) -> Box`` into a reusable component."""

    return Component(function)


def _present(*values: Any) -> list[Any]:
    return [value for value in values if value is not None]


def _page(scene: Any, *children: Any, **props: Any) -> Any:
    """A column that fills the safe area of the frame."""
    return scene.layout.column(*children, within="safe", width="fill", height="fill", **props)


def _token(scene: Any, name: str) -> float:
    return scene.canvas.layout_token(name)


@component
def title_slide(scene: Any, *, title: Any, subtitle: Any = None, footer: Any = None) -> Any:
    return _page(
        scene,
        _present(title, subtitle, footer),
        padding=(_token(scene, "page_padding_wide"), _token(scene, "page_padding_x")),
        gap=_token(scene, "space_lg"),
        align="center",
        justify="center",
    )


@component
def lecture(scene: Any, *, title: Any, body: Any, footer: Any = None) -> Any:
    return _page(
        scene,
        _present(title, body.item(grow=1, align_self="stretch"), footer),
        padding=_token(scene, "page_padding"),
        gap=_token(scene, "space_lg"),
        align="stretch",
    )


@component
def comparison(
    scene: Any,
    *,
    title: Any,
    left: Any,
    right: Any,
    footer: Any = None,
) -> Any:
    columns = scene.layout.row(
        left.item(grow=1, fit="contain"),
        right.item(grow=1, fit="contain"),
        width="fill",
        height="fill",
        gap=_token(scene, "column_gap"),
        align="stretch",
        grow=1,
        align_self="stretch",
    )
    return _page(
        scene,
        _present(title, columns, footer),
        padding=_token(scene, "page_padding"),
        gap=_token(scene, "space_lg"),
        align="stretch",
    )


@component
def vertical_short(scene: Any, *, title: Any, body: Any, caption: Any = None) -> Any:
    return _page(
        scene,
        _present(title, body.item(grow=1, fit="contain"), caption),
        padding=(_token(scene, "vertical_padding"), _token(scene, "vertical_padding_x")),
        gap=_token(scene, "space_lg"),
        align="stretch",
        justify="between",
    )


@component
def minimal(scene: Any, *, content: Any) -> Any:
    return scene.layout.stack(
        content.item(fit="contain", anchor="center"),
        within="safe",
        width="fill",
        height="fill",
        padding=_token(scene, "space_lg"),
    )


@component
def lower_third(scene: Any, *, title: Any, subtitle: Any = None, background: Any = None) -> Any:
    copy = scene.layout.column(
        _present(title, subtitle),
        gap=_token(scene, "space_xs"),
        offset=(0, -_token(scene, "lower_third_offset")),
    )
    return scene.layout.stack(
        _present(
            background.item(absolute=True, fit="stretch") if background is not None else None,
            copy,
        ),
        within="safe",
        width="fill",
        height="fill",
        align="stretch",
    )


@component
def credits(scene: Any, *, title: Any = None, entries: Any, footer: Any = None) -> Any:
    return _page(
        scene,
        _present(title, entries.item(grow=1, align_self="center"), footer),
        padding=(_token(scene, "page_padding_wide"), _token(scene, "page_padding_x")),
        gap=_token(scene, "space_md"),
        align="center",
        justify="center",
    )


__all__ = [
    "Component",
    "component",
    "title_slide",
    "lecture",
    "comparison",
    "vertical_short",
    "minimal",
    "lower_third",
    "credits",
]
