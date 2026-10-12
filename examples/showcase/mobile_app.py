"""A phone chat app built from boxes: a clipped device, bubbles that wrap and a composer that types.

The phone is a column of boxes with a border, a radius, a shadow and clipping;
each message is a box whose text wraps inside ``max_width`` and whose
``align_self`` pushes it to its side. A reply is typed into the composer and
sent, then the other person types back: each new bubble makes the column
reflow. Layered boxes (``stack``) hold the composer's placeholder over the
typed text and the presence line's two states, which fade into each other.
"""

import os

from gaanim import BoxStyle, Direction, Scene

scene = Scene(theme="paper", background="#e2e8f0")
L = scene.layout

INK = "#0f172a"
SLATE = "#475569"
HINT = "#94a3b8"
INDIGO = "#4f46e5"
FIELD = "#f1f5f9"

bubble = BoxStyle(padding=("14px", "20px"), radius="24px", font_size="30px", max_width="350px", line_spacing=1.4)
theirs = bubble.but(background="white", color=INK)
mine = bubble.but(background=INDIGO, color="white")


def message(text, *, me=False):
    # align_self pushes a bubble to its side of the column; its text wraps inside max_width.
    return L.box(text, style=mine if me else theirs, align_self="end" if me else "start")


status = L.row(L.box("9:41", font_size="22px", weight=700), L.box("5G  100%", font_size="22px"),
               justify="between", width="fill", padding=("0px", "12px"))
# Two presence states layered in one stack, each anchored to its left edge;
# "typing…" waits with a transparent fill.
GREEN, CLEAR = "#16a34a", "#16a34a00"
online = scene.text("online", size=0.2, color=GREEN)
typing = scene.text("typing…", size=0.2, color=CLEAR)
presence = L.stack(online.item(anchor="left"), typing.item(anchor="left"), width="fill")
header = L.row(
    L.box("MR", font_size="24px", weight=700, color="white", background="#f97316", radius="full",
          width="60px", height="60px", align="center", justify="center"),
    L.column(L.box("María Ruiz", font_size="30px", weight=700, color=INK), presence, gap="4px"),
    gap="16px", align="center", width="fill", padding=("12px", "4px"),
)
chat = L.column(
    message("Hi! Did you see the new design?"),
    message("Yes! It is all boxes: padding, gap and radius.", me=True),
    message("And no coordinates?"),
    gap="14px", width="fill", grow=1, padding="18px", background=FIELD, radius="26px",
)
# The composer's field stacks the placeholder over the reply typed into it.
REPLY = "Not one. Just rows!"
placeholder = scene.text("Message…", size=0.23, color=HINT)
draft = scene.text(REPLY, size=0.23, color=INK)
send = L.box("→", font_size="28px", color="white", background=INDIGO, radius="full", width="62px", height="62px",
             align="center", justify="center")
composer = L.row(
    L.stack(placeholder.item(anchor="left"), draft.item(anchor="left"), grow=1, padding=("14px", "22px"),
            radius="full", background=FIELD),
    send, gap="14px", align="center", width="fill",
)
phone = L.column(status, header, chat, composer, gap="14px", padding=("22px", "24px"),
                 width="540px", height="920px", background="white", radius="64px",
                 border=INK, border_width="14px", clip=True,
                 shadow={"color": "#0f172a40", "y": "-24px", "blur": "56px"})
pitch = L.column(
    L.box("Chat, beautifully simple", font_size="68px", weight=700, color=INK),
    L.box("Every bubble is a box, and the conversation makes room for each new message. No coordinates.",
          font_size="32px", color=SLATE, max_width="720px", line_spacing=1.45),
    gap="22px",
)
page = L.row(phone, pitch, gap="110px", align="center", justify="center", width="fill", height="fill")

scene.play([phone.animate.fade_in_from(Direction.DOWN, distance=0.6).duration(0.8),
            pitch.reveal(each=0.1, duration=0.5)])
scene.wait(0.4)
scene.stop("chat")

# Typing a reply: the placeholder gives way to the typewriter, the reply is
# sent and its bubble joins the conversation.
scene.play(placeholder.animate.opacity(0).duration(0.2))
scene.play(draft.animate.typewriter(cps=20, keep_cursor=False))
scene.play(send.animate.indicate().duration(0.4))
chat.add(message(REPLY, me=True), duration=0.5, advance=False)
scene.play([draft.animate.opacity(0).duration(0.3), placeholder.animate.opacity(1).duration(0.3)])
scene.wait(0.4)

# María types back: her presence line says so until her bubble arrives.
scene.play([online.animate.fill(CLEAR).duration(0.3), typing.animate.fill(GREEN).duration(0.3)])
scene.wait(0.9)
chat.add(message("It looks amazing. Ship it!"), duration=0.5, advance=False)
scene.play([online.animate.fill(GREEN).duration(0.3), typing.animate.fill(CLEAR).duration(0.3)])
scene.wait(1.0)
scene.stop("replies")

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.4, 1.6, 2.4, 2.8, 3.3, 4.2, 4.8, 5.8])
else:
    scene.render()
