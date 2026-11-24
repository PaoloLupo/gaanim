"""Opaque canvas whose background ``--transparent`` drops, used by the export alpha smoke test.

The scene and its segment both have opaque backgrounds; an export with
``--transparent`` keeps only the drawables, so the corner is transparent
and the rectangle in the middle stays opaque.
"""

from gaanim import GOLD, Scene


scene = Scene(frame=(16, 9), background="#0f172a", margin=0)
scene.segment("only", background="#334155")
scene.geometry.rect(4, 3).fill(GOLD).no_stroke()
scene.wait(0.2)
scene.render()
