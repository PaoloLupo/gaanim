"""Zones: one reusable template divides the frame, a zone or a box; objects stay free."""

import os

from gaanim import Scene, Zones

scene = Scene(frame=(16, 9), theme="paper", margin=0.4)
L = scene.layout
page = Zones.rows(["80px", "1fr", "60px"], names=["header", "body", "footer"], gap="12px")
split = Zones.columns(["1fr", "2fr"], names=["side", "main"], gap="20px")
quad = Zones.grid(rows=2, columns=2, gap="14px")

z = L.zones(page)
body = z["body"].split(split)
cells = body["main"].split(quad)
guides = [scene.geometry.rect(zone.width, zone.height).no_fill().stroke("#cbd5e1", 0.02).place(zone)
          for zone in (z["header"], z["footer"], body["side"], *cells)]

title = scene.text("Zonas", size=0.45).place(z["header"], anchor="left", padding=(0, "16px"))
note = scene.text("La misma plantilla, en tres escalas", size=0.3).place(z["footer"], anchor="right",
                                                                            padding=(0, "16px"))
ball = scene.geometry.circle(1).fill("#f97316").place(cells[0], fit="contain", padding="12px")
block = scene.geometry.square(4).fill("#0ea5e9").place(cells[1], fit="scale_down", padding="12px")
dots = [scene.geometry.circle(0.22).fill("#10b981") for _ in range(5)]
cells[2].arrange(*dots, gap="10px")
# The same template again, inside the last cell.
for zone, color in zip(cells[3].split(quad), ("#fde68a", "#fbcfe8", "#c7d2fe", "#bbf7d0")):
    scene.geometry.rect(zone.width, zone.height).fill(color).place(zone)
scene.wait(0.3)
scene.stop("zonas")

# Objects stay free: animate them into other zones.
scene.play([ball.animate.place(body["side"], fit="contain", padding="24px").duration(0.8)])
scene.play([block.animate.place(cells[0], fit="contain", padding="12px").duration(0.6)])
scene.stop("movidos")

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2, 0.7, 1.7])
else:
    scene.render()
