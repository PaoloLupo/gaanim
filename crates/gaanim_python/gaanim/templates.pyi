"""Typed reusable Layout templates shipped with Gaanim."""

from __future__ import annotations

from typing import Any, Callable, Generic, ParamSpec, TypeVar

from .gaanim_core import Layout, Scene

P = ParamSpec("P")
R = TypeVar("R")

class LayoutTemplate(Generic[P, R]):
    """Signature-preserving callable wrapper for a Layout v2 template."""
    @property
    def slots(self) -> tuple[str, ...]: ...
    def __call__(self, scene: Scene, **slots: Any) -> R:
        """Bind validated named slots and build the template result.

        Example:
            root = lecture(scene, title=scene.text("Topic", role="title"), body=scene.text("Body"))
        """
        ...

def layout_template(function: Callable[P, R]) -> LayoutTemplate[P, R]:
    """Wrap a typed function as a signature-checked Layout v2 template.

    Example:
        @layout_template
        def centered(scene: Scene, *, content: Any) -> Layout:
            return scene.stack([content], width="fill", height="fill")
    """
    ...

def title_slide(scene: Scene, *, title: Any, subtitle: Any = None, footer: Any = None) -> Layout:
    """Build a centered title-slide layout inside the safe frame.

    Example:
        root = title_slide(scene, title=scene.text("Gaanim", role="title"))
    """
    ...

def lecture(scene: Scene, *, title: Any, body: Any, footer: Any = None) -> Layout:
    """Build a lecture layout whose body grows within Layout v2.

    Example:
        root = lecture(scene, title=scene.text("Topic", role="heading"), body=scene.text("Explanation"))
    """
    ...

def comparison(scene: Scene, *, title: Any, left: Any, right: Any, footer: Any = None) -> Layout:
    """Build a two-column comparison with equally growing sides.

    Example:
        root = comparison(scene, title=scene.text("Compare", role="heading"), left=scene.text("A"), right=scene.text("B"))
    """
    ...

def vertical_short(scene: Scene, *, title: Any, body: Any, caption: Any = None) -> Layout:
    """Build a portrait-safe vertical composition.

    Example:
        root = vertical_short(scene, title=scene.text("Short", role="title"), body=scene.text("Body"))
    """
    ...

def minimal(scene: Scene, *, content: Any) -> Layout:
    """Center one fitted item in the safe frame.

    Example:
        root = minimal(scene, content=scene.text("Focus"))
    """
    ...

def lower_third(scene: Scene, *, title: Any, subtitle: Any = None, background: Any = None) -> Layout:
    """Build a lower-third stack with optional background.

    Example:
        root = lower_third(scene, title=scene.text("Ada Lovelace", role="heading"))
    """
    ...

def credits(scene: Scene, *, title: Any = None, entries: Any, footer: Any = None) -> Layout:
    """Build a centered credits layout with a growing entries region.

    Example:
        root = credits(scene, entries=scene.text("Animation — Team"))
    """
    ...
