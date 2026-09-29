"""A phone chat screen: a clipped device, header, message bubbles and a composer, all boxes."""

import os

from gaanim import BoxStyle, Scene

scene = Scene(frame=(16, 9), theme="paper", background="#e2e8f0")
L = scene.layout
bubble = BoxStyle(padding=("10px", "14px"), radius="18px", font_size="19px", max_width="250px",
                  line_spacing=1.45)
theirs = bubble.but(background="white", color="#0f172a")
mine = bubble.but(background="#4f46e5", color="white")


def message(text, *, me=False):
    # align_self pushes a bubble to its side of the column; text wraps inside max_width.
    return L.box(text, style=mine if me else theirs, align_self="end" if me else "start")


status = L.row(L.box("9:41", font_size="15px", weight=700), L.box("5G  100%", font_size="15px"),
               justify="between", width="fill", padding=("0px", "8px"))
header = L.row(
    L.box("MR", font_size="16px", weight=700, color="white", background="#f97316", radius="full",
          width="40px", height="40px", align="center", justify="center"),
    L.column(L.box("María Ruiz", font_size="20px", weight=700),
             L.box("en línea", font_size="14px", color="#16a34a")),
    gap="12px", align="center", width="fill", padding=("10px", "0px"),
)
chat = L.column(
    message("¡Hola! ¿Viste el nuevo diseño?"),
    message("Sí, todo está hecho con cajas: padding, gap y radius.", me=True),
    message("¿Y sin coordenadas?"),
    gap="10px", width="fill", grow=1, padding="12px", background="#f1f5f9", radius="18px",
)
composer = L.row(
    L.box("Escribe un mensaje…", font_size="17px", color="#94a3b8", grow=1, padding=("10px", "16px"),
          radius="full", background="#f1f5f9"),
    L.box("→", font_size="18px", color="white", background="#4f46e5", radius="full", width="42px",
          height="42px", align="center", justify="center"),
    gap="10px", align="center", width="fill",
)
phone = L.column(status, header, chat, composer, gap="10px", padding=("16px", "18px"),
                 width="400px", height="780px", background="white", radius="48px",
                 border="#0f172a", border_width="10px", clip=True,
                 shadow={"color": "#0f172a40", "y": "-18px", "blur": "40px"})
caption = L.column(L.box("Interfaces sin coordenadas", font_size="44px", weight=700),
                   L.box("Cada burbuja es una caja; la columna las reacomoda al llegar mensajes.",
                         font_size="22px", color="#475569", max_width="520px", line_spacing=1.45),
                   gap="14px")
page = L.row(phone, caption, gap="80px", align="center", justify="center", within="safe", width="fill",
             height="fill")

scene.play([page.animate.fade_in().duration(0.5)])
scene.stop("chat")

# New messages fade in below the others.
chat.add(message("¡Exacto! Solo filas, columnas y estilos.", me=True), duration=0.5)
chat.add(message("Se ve increíble."), duration=0.5)
scene.stop("respuestas")

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.5, 0.75, 1.0, 1.5])
else:
    scene.render()
