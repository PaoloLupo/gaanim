"""Typed access to the bundled Tailwind CSS color palette."""

from typing import Mapping
from .gaanim_core import Color

class TailwindPalette:
    version: str
    red: Mapping[int, Color]
    orange: Mapping[int, Color]
    amber: Mapping[int, Color]
    yellow: Mapping[int, Color]
    lime: Mapping[int, Color]
    green: Mapping[int, Color]
    emerald: Mapping[int, Color]
    teal: Mapping[int, Color]
    cyan: Mapping[int, Color]
    sky: Mapping[int, Color]
    blue: Mapping[int, Color]
    indigo: Mapping[int, Color]
    violet: Mapping[int, Color]
    purple: Mapping[int, Color]
    fuchsia: Mapping[int, Color]
    pink: Mapping[int, Color]
    rose: Mapping[int, Color]
    slate: Mapping[int, Color]
    gray: Mapping[int, Color]
    zinc: Mapping[int, Color]
    neutral: Mapping[int, Color]
    stone: Mapping[int, Color]
    mauve: Mapping[int, Color]
    olive: Mapping[int, Color]
    mist: Mapping[int, Color]
    taupe: Mapping[int, Color]
    def __getitem__(self, family: str) -> Mapping[int, Color]: ...
    def families(self) -> tuple[str, ...]: ...

tailwind: TailwindPalette
