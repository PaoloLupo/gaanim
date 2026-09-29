"""Reusable, signature-checked components built from boxes."""

from __future__ import annotations

from typing import Any, Callable, Generic, ParamSpec, TypeVar

from .gaanim_core import Box, Scene

P = ParamSpec("P")
R = TypeVar("R")

class Component(Generic[P, R]):
    """Validated callable produced by :func:`component`."""
    @property
    def slots(self) -> tuple[str, ...]: ...
    def __call__(self, scene: Scene, *args: Any, **slots: Any) -> R:
        """Check the arguments against the function signature and build a new result."""
        ...

def component(function: Callable[P, R]) -> Component[P, R]:
    """Turn a typed function ``(scene, ...) -> Box`` into a reusable component.

    Every call builds new, independent drawables.

    Example:
        @component
        def tag(scene: Scene, *, text: str, color: str = "#4f46e5") -> Box:
            return scene.layout.box(text, padding=("4px", "12px"), radius="full",
                                    background=color, color="white")

        row = scene.layout.row(tag(scene, text="Nuevo"), tag(scene, text="Beta"), gap="8px")
    """
    ...

def title_slide(scene: Scene, *, title: Any, subtitle: Any = None, footer: Any = None) -> Box:
    """Build a centered title-slide layout inside the safe frame.

    Example:
        root = title_slide(scene, title=scene.text("Gaanim", role="title"))
    """
    ...

def lecture(scene: Scene, *, title: Any, body: Any, footer: Any = None) -> Box:
    """Build a lecture layout whose body grows within the safe area.

    Example:
        root = lecture(scene, title=scene.text("Topic", role="heading"), body=scene.text("Explanation"))
    """
    ...

def comparison(scene: Scene, *, title: Any, left: Any, right: Any, footer: Any = None) -> Box:
    """Build a two-column comparison with equally growing sides.

    Example:
        root = comparison(scene, title=scene.text("Compare", role="heading"), left=scene.text("A"), right=scene.text("B"))
    """
    ...

def vertical_short(scene: Scene, *, title: Any, body: Any, caption: Any = None) -> Box:
    """Build a portrait-safe vertical composition.

    Example:
        root = vertical_short(scene, title=scene.text("Short", role="title"), body=scene.text("Body"))
    """
    ...

def minimal(scene: Scene, *, content: Any) -> Box:
    """Center one fitted item in the safe frame.

    Example:
        root = minimal(scene, content=scene.text("Focus"))
    """
    ...

def lower_third(scene: Scene, *, title: Any, subtitle: Any = None, background: Any = None) -> Box:
    """Build a lower-third stack with optional background.

    Example:
        root = lower_third(scene, title=scene.text("Ada Lovelace", role="heading"))
    """
    ...

def credits(scene: Scene, *, title: Any = None, entries: Any, footer: Any = None) -> Box:
    """Build a centered credits layout with a growing entries region.

    Example:
        root = credits(scene, entries=scene.text("Animation — Team"))
    """
    ...
