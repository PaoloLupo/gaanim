from gaanim import RED, Anchor, Canvas, Direction

c = Canvas(width=1920, height=1080)
title = c.title("Hola").to_edge(Direction.UP, 100.0)
box = (
    c.rounded_rect(220.0, 90.0, 12.0)
    .next_to(title, Direction.DOWN, 24.0)
    .no_fill()
    .z_index(-1)
)
label = c.text("Mundo").fill(RED).align_to(box, Anchor.CENTER)

c.play(
    [
        title.write().spring(),
        box.write(),
        label.write().spring(),
    ],
    lag=1.0,
)


c.render()
