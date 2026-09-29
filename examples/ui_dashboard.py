"""A dashboard built only from boxes: sidebar, header, KPI cards, a bar chart and a list."""

import os

from gaanim import BoxStyle, Scene, component

scene = Scene(frame=(16, 9), theme="paper", margin=0.3)
L = scene.layout
L.classes(
    panel=BoxStyle(padding="20px", gap="12px", radius="16px", background="white", border="#e2e8f0",
                   border_width="1px"),
    caption=BoxStyle(font_size="16px", color="#64748b"),
    nav=BoxStyle(direction="row", gap="10px", padding=("8px", "12px"), radius="8px", font_size="18px",
                 color="#cbd5e1", align="center", width="fill"),
)


@component
def kpi(scene, *, label: str, value: str, delta: str, up: bool = True):
    badge = L.box(delta, font_size="15px", padding=("2px", "8px"), radius="full",
                  background="#dcfce7" if up else "#fee2e2", color="#166534" if up else "#991b1b")
    return L.box(L.box(label, class_="caption"),
                 L.row(L.box(value, font_size="34px", weight=700), badge, gap="10px", align="center"),
                 class_="panel", grow=1)


def dot(color):
    return L.box(width="10px", height="10px", radius="full", background=color)


sidebar = L.column(
    L.box("Métricas", font_size="24px", weight=700, color="white", padding=("4px", "12px")),
    L.box(dot("#818cf8"), "Resumen", class_="nav", background="#312e81", color="white"),
    L.box(dot("#475569"), "Ventas", class_="nav"),
    L.box(dot("#475569"), "Clientes", class_="nav"),
    L.box(dot("#475569"), "Ajustes", class_="nav"),
    gap="8px", padding="18px", width="220px", height="fill", background="#1e1b4b", radius="18px",
)
header = L.row(
    L.column(L.box("Resumen trimestral", font_size="30px", weight=700),
             L.box("Actualizado hace 5 min", class_="caption"), gap="2px"),
    L.row(L.box("Buscar…", font_size="17px", color="#94a3b8", padding=("8px", "16px"), radius="full",
                background="#f1f5f9", width="220px"),
          L.box("AL", font_size="16px", weight=700, color="white", background="#6366f1", radius="full",
                width="40px", height="40px", align="center", justify="center"),
          gap="12px", align="center"),
    justify="between", align="center", width="fill",
)
kpis = L.row(
    kpi(scene, label="Ingresos", value="$48.2k", delta="+12%"),
    kpi(scene, label="Pedidos", value="1 284", delta="+4%"),
    kpi(scene, label="Devoluciones", value="2.1%", delta="-0.3%", up=False),
    gap="16px", width="fill",
)
# Bar heights are percentages of the plot area, which takes the panel's free height.
heights = [40, 61, 49, 80, 69, 100, 88]
bars = L.row(*[L.box(width="fill", height=f"{h}%", radius="6px", background="#a5b4fc")
               for h in heights], gap="10px", align="end", width="fill", grow=1)
days = L.row(*[L.box(d, class_="caption", width="fill", align="center") for d in "LMMJVSD"],
             gap="10px", width="fill")
chart = L.box(L.box("Ventas por día", font_size="20px", weight=700), bars, days,
              class_="panel", grow=2, height="fill")
orders = L.box(L.box("Últimos pedidos", font_size="20px", weight=700),
               *[L.row(L.box(name, font_size="17px"), L.box(amount, font_size="17px", weight=700),
                       justify="between", width="fill")
                 for name, amount in (("Lucía P.", "$320"), ("Marco T.", "$85"), ("Ana R.", "$1 040"))],
               class_="panel", grow=1, height="fill")
content = L.column(header, kpis, L.row(chart, orders, gap="16px", width="fill", grow=1),
                   gap="18px", grow=1, height="fill")
app = L.row(sidebar, content, gap="22px", within="safe", width="fill", height="fill")

scene.play([app.animate.fade_in().duration(0.5)])
scene.stop("panel")

# The list grows and the new order is highlighted; the panel reflows around it.
new = L.row(L.box("Nuevo: Sofía G.", font_size="17px", weight=700, color="#3730a3"),
            L.box("$640", font_size="17px", weight=700, color="#3730a3"),
            justify="between", width="fill", padding=("6px", "10px"), radius="8px", background="#e0e7ff")
orders.add(new, at=1, duration=0.6)
bars[-1].set(background="#6366f1", duration=0.4)
scene.stop("nuevo pedido")

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.5, 0.8, 1.1, 1.5])
else:
    scene.render()
