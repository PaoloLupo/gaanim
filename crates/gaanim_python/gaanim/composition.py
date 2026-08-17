"""Animation composition helpers.

These helpers return flat ``list[Anim]`` values that ``scene.play`` already
accepts. Composition resolves to per-clip delays, which the engine compiles
into ordinary clips, so no extra timeline machinery is involved.

```python
scene.play(Succession(title.write(), box.create(), label.fade_in()))
scene.play(LaggedStart(*[b.grow_from_center() for b in buildings], lag=0.08))
```
"""

from __future__ import annotations

from typing import Iterable, Union

from .gaanim_core import Anim

AnimLike = Union[Anim, Iterable["AnimLike"]]


def _flatten(anims: Iterable[AnimLike]) -> list[Anim]:
    flat: list[Anim] = []
    for anim in anims:
        if isinstance(anim, Anim):
            flat.append(anim)
        else:
            flat.extend(_flatten(anim))
    return flat


def _shifted(anim: Anim, extra: float) -> Anim:
    if extra == 0.0:
        return anim
    _, delay = anim.timing
    return anim.delay(delay + extra)


def _span(members: list[Anim]) -> float:
    return max(duration + delay for duration, delay in (anim.timing for anim in members))


def AnimationGroup(*anims: AnimLike) -> list[Anim]:
    """Play the given animations in parallel, as one readable unit.

    Accepts nested groups; everything is flattened without changing delays.
    """
    return _flatten(anims)


def LaggedStart(*anims: AnimLike, lag: float = 0.1) -> list[Anim]:
    """Start each animation ``lag`` seconds after the previous one.

    Equivalent to ``scene.play(anims, lag=...)`` but composable: the result
    can be nested inside ``Succession`` or ``AnimationGroup``.
    """
    return [
        _shifted(anim, index * lag) for index, anim in enumerate(_flatten(anims))
    ]


def Succession(*steps: AnimLike) -> list[Anim]:
    """Play each step after the previous one finishes.

    Each argument is a step: a single ``Anim`` or a group (e.g. the result of
    ``LaggedStart``). The next step starts when the longest member of the
    current step has finished, honoring each animation's own duration.
    """
    result: list[Anim] = []
    cursor = 0.0
    for step in steps:
        members = [step] if isinstance(step, Anim) else _flatten(step)
        if not members:
            continue
        shifted = [_shifted(anim, cursor) for anim in members]
        result.extend(shifted)
        cursor += _span(shifted)
    return result
