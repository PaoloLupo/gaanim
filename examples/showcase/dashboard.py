"""An analytics dashboard built only from boxes, revealed piece by piece and updated live.

A sidebar, a header, three KPI cards, a bar chart and a list of recent orders
are rows and columns of boxes on the paper theme: no coordinates anywhere.
The dashboard reveals itself piece by piece and the bars rise. Then a new
order arrives: the list makes room for it, the KPIs change and Sunday's bar
grows and lights up, each change reflowing the boxes around it.
"""

import os

from gaanim import BoxStyle, Scene

scene = Scene(theme="paper")
L = scene.layout

INK = "#0f172a"
SLATE = "#64748b"
LINE = "#e2e8f0"
NIGHT = "#1e1b4b"
INDIGO = "#6366f1"
LAVENDER = "#a5b4fc"

L.classes(
    panel=BoxStyle(padding=("28px", "32px"), gap="16px", radius="22px", background="white", border=LINE,
                   border_width="2px", shadow={"color": "#0f172a14", "y": "-6px", "blur": "24px"}),
    caption=BoxStyle(font_size="24px", color=SLATE),
    heading=BoxStyle(font_size="30px", weight=700, color=INK),
    nav=BoxStyle(direction="row", gap="14px", padding=("12px", "18px"), radius="12px", font_size="27px",
                 color="#cbd5e1", align="center", width="fill"),
)


def dot(color, size="14px"):
    return L.box(width=size, height=size, radius="full", background=color)


def badge(text, up=True):
    """A pill with the change of a KPI, green when it is good news."""
    return L.box(text, font_size="22px", weight=700, padding=("4px", "12px"), radius="full",
                 background="#dcfce7" if up else "#fee2e2", color="#166534" if up else "#991b1b")


def kpi(label, value, delta, up=True):
    """A KPI card around ``value``, a box or a rolling number."""
    return L.box(L.box(label, class_="caption"), L.row(value, badge(delta, up), gap="16px", align="center"),
                 class_="panel", grow=1)


def order(name, amount, **style):
    return L.row(L.box(name, font_size="26px", **style), L.box(amount, font_size="26px", weight=700, **style),
                 justify="between", width="fill")


sidebar = L.column(
    L.row(dot(INDIGO, "22px"), L.box("Metrics", font_size="36px", weight=700, color="white"), gap="14px",
          align="center", padding=("6px", "18px", "22px")),
    L.box(dot("#818cf8"), "Overview", class_="nav", background="#312e81", color="white"),
    L.box(dot("#475569"), "Sales", class_="nav"),
    L.box(dot("#475569"), "Customers", class_="nav"),
    L.box(dot("#475569"), "Settings", class_="nav"),
    gap="10px", padding="24px", width="300px", height="fill", background=NIGHT, radius="26px",
)
header = L.row(
    L.column(L.box("Quarterly overview", font_size="46px", weight=700, color=INK),
             L.box("Updated 5 minutes ago", class_="caption"), gap="4px"),
    L.row(L.box("Search…", font_size="25px", color="#94a3b8", padding=("12px", "24px"), radius="full",
                background="#f1f5f9", width="300px"),
          L.box("AL", font_size="24px", weight=700, color="white", background=INDIGO, radius="full",
                width="60px", height="60px", align="center", justify="center"),
          gap="18px", align="center"),
    justify="between", align="center", width="fill",
)
# Rolling numbers count to new values like an odometer.
revenue_number = scene.viz.rolling_number(48.2, decimals=1, prefix="$", suffix="k", font_size=0.45, weight=700,
                                          color=INK)
orders_number = scene.viz.rolling_number(1284, group_separator=",", font_size=0.45, weight=700, color=INK)
kpis = L.row(
    kpi("Revenue", revenue_number, "+12%"),
    kpi("Orders", orders_number, "+4%"),
    kpi("Returns", L.box("2.1%", font_size="54px", weight=700, color=INK), "−0.3%", up=False),
    gap="24px", width="fill",
)

# Bar heights are shares of the row of bars, which starts flat at the bottom
# of the plot area and rises to fill it once the dashboard is in place.
HEIGHTS = [40, 61, 49, 80, 69, 92, 84]
bars = L.row(*[L.box(width="fill", height=f"{height}%", radius="10px", background=LAVENDER) for height in HEIGHTS],
             gap="16px", align="end", width="fill", height="4%")
plot = L.column(bars, justify="end", width="fill", grow=1)
days = L.row(*[L.box(day, class_="caption", width="fill", align="center") for day in "MTWTFSS"],
             gap="16px", width="fill")
chart = L.box(L.box("Sales by day", class_="heading"), plot, days, class_="panel", grow=2, height="fill")
CUSTOMERS = [("Lucía P.", "$320"), ("Marco T.", "$85"), ("Ana R.", "$1,040"), ("Kenji M.", "$210"),
             ("Priya S.", "$560")]
orders = L.box(L.box("Recent orders", class_="heading"), *[order(name, amount) for name, amount in CUSTOMERS],
               class_="panel", grow=1, height="fill")
content = L.column(header, kpis, L.row(chart, orders, gap="24px", width="fill", grow=1),
                   gap="26px", grow=1, height="fill")
app = L.row(sidebar, content, gap="30px", padding="36px", width="fill", height="fill")

# reveal() shows every piece in draw order: backgrounds fade and texts rise.
# Rolling numbers show from their declaration, so they start transparent.
counters = [revenue_number.visual, orders_number.visual]
for counter in counters:
    counter.opacity(0)
scene.play([app.reveal(each=0.015, duration=0.35, rules="fade")]
           + [counter.animate.opacity(1).duration(0.4).delay(0.5) for counter in counters])
# set() with a duration animates the reflow: the row grows and its bars with it.
bars.set(height="100%", duration=0.9)
scene.wait(0.6)
scene.stop("overview")

# A new order: the list makes room at the top, the KPIs count up and
# Sunday's bar grows past Saturday's and lights up, all at once
# (advance=False starts the next change at the same time).
new = order("New: Sofía G.", "$640", color="#3730a3")
new.set(padding=("10px", "14px"), radius="12px", background="#e0e7ff")
orders.add(new, at=1, duration=0.8, advance=False)
bars[-1].set(height="100%", background=INDIGO, duration=0.8, advance=False)
for counter in (revenue_number, orders_number):
    counter.fill(INDIGO)
scene.play([revenue_number.count_to(48.8, duration=0.8), orders_number.count_to(1285, duration=0.8)])
scene.wait(1.0)
scene.stop("new order")

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.4, 0.9, 1.6, 2.4, 3.1, 3.5, 4.5])
else:
    scene.render()
