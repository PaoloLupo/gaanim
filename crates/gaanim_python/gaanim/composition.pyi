"""Animation composition helpers shipped with Gaanim."""

from __future__ import annotations

from typing import Iterable, Union

from .gaanim_core import Anim

AnimLike = Union[Anim, Iterable["AnimLike"]]

def AnimationGroup(*anims: AnimLike) -> list[Anim]:
    """Play the given animations in parallel, as one readable unit.

    Accepts nested groups; everything is flattened without changing delays.

    Example:
        scene.play(AnimationGroup(box.create(), label.write()))
    """
    ...

def LaggedStart(*anims: AnimLike, lag: float = 0.1) -> list[Anim]:
    """Start each animation ``lag`` seconds after the previous one.

    Equivalent to ``scene.play(anims, lag=...)`` but composable: the result
    can be nested inside ``Succession`` or ``AnimationGroup``.

    Example:
        scene.play(LaggedStart(*[b.grow_from_center() for b in buildings], lag=0.08))
    """
    ...

def Succession(*steps: AnimLike) -> list[Anim]:
    """Play each step after the previous one finishes.

    Each argument is a step: a single ``Anim`` or a group (e.g. the result of
    ``LaggedStart``). The next step starts when the longest member of the
    current step has finished.

    Example:
        scene.play(Succession(title.write(), box.create(), label.fade_in()))
    """
    ...
