from gaanim import BLACK, GRAY, GREEN, RED, WHITE, Anchor, Canvas, Direction

c = Canvas(width=1920, height=1080, background=GRAY)
title = c.title("Hola").to_edge(Direction.UP, 100.0)
box = c.rounded_rect(220.0, 90.0, 12.0).next_to(title, Direction.DOWN, 24.0).fill(BLACK)
label = c.text("Mundo").fill(WHITE).align_to(box, Anchor.CENTER)

c.play(
    [
        title.write(2.0).spring(),
        box.write(),
        label.write().spring(),
    ],
    lag=0.1,
)


c.render()
