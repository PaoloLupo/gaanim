"""Runtime values of the type aliases declared in ``gaanim_core.pyi``.

The stub declares aliases such as ``Paint`` and ``ColorLike`` on
``gaanim.gaanim_core`` for type checkers. The native module defines classes,
not these unions, so ``import gaanim`` installs them on it (:func:`install`):
``from gaanim.gaanim_core import Paint`` then works when a script runs too.
Each value is the stub's expression verbatim; ``tests/validate_python_api.py``
checks that they stay equal.
"""

from typing import Any, Literal, Sequence, TypeAlias

from .animation_types import Playable, ScalarSource
from .gaanim_core import (
    AnchorPoint,
    Background,
    Brush,
    Color,
    ColorMap,
    Direction,
    Drawable,
    Field,
    PointRef,
    TextPart,
    TextParts,
    Value,
)

CurvePoint: TypeAlias = tuple[float, float]
CurveControl: TypeAlias = CurvePoint | Literal["auto"] | None
CurveCommand: TypeAlias = tuple[str, Sequence[CurvePoint | CurveControl]]
BlendModeName: TypeAlias = Literal[
    "normal",
    "multiply",
    "screen",
    "overlay",
    "darken",
    "lighten",
    "color_dodge",
    "color_burn",
    "hard_light",
    "soft_light",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
    "add",
]
ThemeName: TypeAlias = Literal[
    "technical",
    "presentation",
    "paper",
    "dracula",
    "nord",
    "solarized-dark",
    "solarized-light",
    "gruvbox-dark",
    "tokyo-night",
    "catppuccin-mocha",
    "catppuccin-latte",
    "scientific",
    "deck",
    "light",
    "gruvbox",
    "tokyo",
    "catppuccin",
    "mocha",
    "latte",
]
ColorLike: TypeAlias = Color | str | tuple[int, int, int] | tuple[int, int, int, int]
ColorMapLike: TypeAlias = ColorMap | str
Paint: TypeAlias = ColorLike | Brush
BackgroundLike: TypeAlias = Paint | Background
Length: TypeAlias = float | str
SizeRule: TypeAlias = float | str
Track: TypeAlias = float | str
Padding: TypeAlias = Length | tuple[Length, Length] | tuple[Length, Length, Length] | tuple[Length, Length, Length, Length]
Margin: TypeAlias = Length | tuple[Length | Literal["auto"], ...]
Align: TypeAlias = Literal["start", "center", "end", "stretch", "baseline"]
Justify: TypeAlias = Literal["start", "center", "end", "between", "around", "evenly"]
Fit: TypeAlias = Literal["none", "contain", "cover", "stretch", "scale_down"]
AnchorName: TypeAlias = Literal["center", "top", "bottom", "left", "right", "top_left", "top_right", "bottom_left", "bottom_right"]
Shadow: TypeAlias = bool | dict[str, Any]
StaggerOrigin: TypeAlias = Literal["start", "end", "center", "edges", "random"] | tuple[float, float]
Endpoint: TypeAlias = Drawable | AnchorPoint | PointRef | tuple[float, float] | tuple[float, float, float]
AngleRay: TypeAlias = Direction | Endpoint
TextRole: TypeAlias = Literal["title", "subtitle", "kicker", "heading", "body", "caption", "label", "code", "math"]
TextWrap: TypeAlias = Literal["auto", False] | float
TextAlign: TypeAlias = Literal["left", "center", "right", "justify"]
TextOverflow: TypeAlias = Literal["visible", "clip", "ellipsis"]
TextBox: TypeAlias = Literal["line", "cap", "ink"]
TextDirection: TypeAlias = Literal["auto", "ltr", "rtl"]
TextContent: TypeAlias = str | TextPart | TextParts
EncodingLike: TypeAlias = str | Field | Value
ChartMark: TypeAlias = Literal["point", "line", "step", "area", "bar", "histogram", "box", "violin", "error_bar", "heatmap", "surface"]

__all__ = [
    "CurvePoint",
    "CurveControl",
    "CurveCommand",
    "BlendModeName",
    "ThemeName",
    "ColorLike",
    "ColorMapLike",
    "Paint",
    "BackgroundLike",
    "Length",
    "SizeRule",
    "Track",
    "Padding",
    "Margin",
    "Align",
    "Justify",
    "Fit",
    "AnchorName",
    "Shadow",
    "StaggerOrigin",
    "Endpoint",
    "AngleRay",
    "TextRole",
    "TextWrap",
    "TextAlign",
    "TextOverflow",
    "TextBox",
    "TextDirection",
    "TextContent",
    "EncodingLike",
    "ChartMark",
    "Playable",
    "ScalarSource",
]


def install(module: object) -> None:
    """Give ``module`` (the native ``gaanim_core``) every alias it lacks."""
    for name in __all__:
        if not hasattr(module, name):
            setattr(module, name, globals()[name])
