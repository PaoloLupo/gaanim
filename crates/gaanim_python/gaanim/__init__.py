"""Gaanim — GPU-accelerated vector animation engine."""

import warnings

from .gaanim_core import (
    Anchor,
    Direction,
    FrameLayout,
    Flow,
    GridLayout,
    LayoutRegion,
    Layout,
    BLACK,
    BLUE,
    CORAL,
    CYAN,
    GOLD,
    GRAY,
    GREEN,
    NAVY,
    ORANGE,
    PINK,
    PURPLE,
    RED,
    TEAL,
    WHITE,
    YELLOW,
    Anim,
    Color,
    Brush,
    Camera,
    Theme,
    ThesisTemplate,
    Drawable,
    Transition,
    Updater,
    ValueTracker,
    Scene,
    Slide,
)


def Canvas(*args, **kwargs):
    """Deprecated compatibility constructor; use :class:`Scene` instead."""
    warnings.warn(
        "Canvas is deprecated as the animation facade; use Scene instead. "
        "Use scene.canvas for viewport configuration.",
        DeprecationWarning,
        stacklevel=2,
    )
    return Scene(*args, **kwargs)


# manim-compatible Axes API: ax.plot(...) delegates to current scene
_current_scene = None

def _axes_plot(self, func=None, x_range=None, x=None, samples=160, **kwargs):
    # manim get_graph/plot without func is no-op (return self for compat)
    if func is None:
        return self
    global _current_scene
    if _current_scene is None:
        raise RuntimeError("No active Scene for axes.plot(); use scene.plot(axes, func, x_range)")
    xr = x_range if x_range is not None else x
    if xr is None:
        raise ValueError("x_range (or x) required for plot()")
    if isinstance(xr, (list, tuple)) and len(xr) == 3:
        xr = (float(xr[0]), float(xr[1]))
    elif isinstance(xr, (list, tuple)) and len(xr) == 2:
        xr = (float(xr[0]), float(xr[1]))
    else:
        raise ValueError("x_range must be (min, max) or (min, max, step)")
    return _current_scene.plot(self, func, xr, samples)

def _axes_plot_parametric(self, func, t_range=None, t=None, samples=160, **kwargs):
    global _current_scene
    if _current_scene is None:
        raise RuntimeError("No active Scene")
    tr = t_range if t_range is not None else t
    if tr is None:
        tr = (0, 2 * 3.141592653589793)
    if isinstance(tr, (list, tuple)) and len(tr) == 3:
        tr = (float(tr[0]), float(tr[1]))
    elif isinstance(tr, (list, tuple)) and len(tr) == 2:
        tr = (float(tr[0]), float(tr[1]))
    return _current_scene.plot_parametric_curve(self, func, tr, samples)

# Attach manim-compatible methods to Drawable (axes)
Drawable.plot = _axes_plot
Drawable.get_graph = _axes_plot
Drawable.plot_parametric_curve = _axes_plot_parametric
# coords_to_point / point_to_coords already via Rust, keep as is
# get_x_axis / get_y_axis / add_coordinates already via Rust

# manim Axes(x_range=..., y_range=..., x_length=..., y_length=..., tips=...) compat
_orig_axes = Scene.axes

def _norm_range(r):
    if r is None:
        return None
    if isinstance(r, (list, tuple)):
        if len(r) == 2:
            return (float(r[0]), float(r[1]), 1.0)
        if len(r) == 3:
            return (float(r[0]), float(r[1]), float(r[2]))
    raise TypeError("x_range/y_range must be (min, max) or (min, max, step)")

def _patched_axes(
    self,
    x_range=None,
    y_range=None,
    x=None,
    y=None,
    x_length=None,
    y_length=None,
    tips=True,
    auto_fit=True,
    axis_config=None,
    x_axis_config=None,
    y_axis_config=None,
    **kwargs,
):
    global _current_scene
    # manim aliases: x_range/y_range vs x/y, with manim defaults
    if x_range is not None:
        x = _norm_range(x_range)
    if y_range is not None:
        y = _norm_range(y_range)
    if x is not None:
        x = _norm_range(x)
    if y is not None:
        y = _norm_range(y)
    if x is None:
        x = (-7.11, 7.11, 1.0)
    if y is None:
        y = (-4.0, 4.0, 1.0)
    _current_scene = self
    return _orig_axes(
        self,
        x,
        y,
        auto_fit=auto_fit,
        x_length=x_length,
        y_length=y_length,
        tips=tips,
        axis_config=axis_config,
        x_axis_config=x_axis_config,
        y_axis_config=y_axis_config,
        **kwargs,
    )

Scene.axes = _patched_axes

__all__ = [
    "Scene",
    "Slide",
    "Canvas",
    "Drawable",
    "Anim",
    "Transition",
    "Color",
    "Brush",
    "Camera",
    "Theme",
    "ThesisTemplate",
    "Anchor",
    "Direction",
    "LayoutRegion",
    "Layout",
    "GridLayout",
    "FrameLayout",
    "Flow",
    "Updater",
    "ValueTracker",
    "GOLD",
    "CORAL",
    "BLUE",
    "WHITE",
    "BLACK",
    "RED",
    "GREEN",
    "YELLOW",
    "ORANGE",
    "PURPLE",
    "PINK",
    "GRAY",
    "CYAN",
    "NAVY",
    "TEAL",
]

__version__ = "0.1.0"
