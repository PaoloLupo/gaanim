"""Data structures: an array sorted by bubble sort with pointers, a stack, a queue and a linked list.

scene.viz.array draws cells with indices and named pointers; compare, swap,
insert and remove move the cells, so each keeps its identity. scene.viz.stack,
scene.viz.queue and scene.viz.linked_list add push and pop, enqueue and
dequeue, and insert_after and remove. Every operation is a Composition to
play next to other animations.
"""

import os

from gaanim import Scene, Zones

scene = Scene()
L = scene.layout
seeks = []


def page(label):
    """The page split into the structure's zone and its code label below."""
    zone, bottom = L.zones(Zones.rows(["1fr", "96px"], padding="page_padding"))
    scene.text(label, role="code").place(bottom)
    return zone


# Bubble sort: compare neighbours with pointers i and j, swap when out of order.
scene.segment("bubble_sort")
zone = page("arr.animate.compare(j, j + 1) · arr.animate.swap(j, j + 1)")
arr = scene.viz.array([5, 2, 9, 1, 6], indices=True).place(zone)
p, q = arr.pointer("j", 0), arr.pointer("j+1", 1)
start = scene.cursor
for end in range(len(arr) - 1, 0, -1):
    for k in range(end):
        scene.play([arr.animate.compare(k, k + 1)])
        if arr.values[k] > arr.values[k + 1]:
            scene.play([arr.animate.swap(k, k + 1)])
        if k + 1 < end:
            scene.play([p.animate.move_to(k + 1), q.animate.move_to(k + 2)])
    arr.cell(end).fill(scene.colors.success)
    if end > 1:
        scene.play([p.animate.move_to(0), q.animate.move_to(1)])
arr.cell(0).fill(scene.colors.success)
scene.wait(0.6)
seeks.extend([start + 1.0, start + 3.0, scene.cursor - 0.1])

# A stack grows upward; a queue takes values in on the right and out on the left.
scene.segment("stack_and_queue")
zone = page("stack.animate.push(x) · queue.animate.dequeue()")
left, right = zone.split(Zones.columns(2, gap="48px"))
stack = scene.viz.stack(["a", "b"]).place(left)
queue = scene.viz.queue([1, 2, 3]).place(right)
scene.play([stack.animate.push("c"), queue.animate.enqueue(4)])
scene.play([stack.animate.pop(), queue.animate.dequeue()])
start = scene.cursor
scene.play([stack.animate.push("d"), queue.animate.dequeue()])
scene.wait(0.4)
seeks.extend([start + 0.3, scene.cursor - 0.1])

# A linked list makes room and links the new node; remove unlinks one.
scene.segment("linked_list")
zone = page("nodes.animate.insert_after(0, 5) · nodes.animate.remove(2)")
nodes = scene.viz.linked_list([3, 8, 4]).place(zone)
start = scene.cursor
scene.play([nodes.animate.insert_after(0, 5)])
nodes.cell(1).fill(scene.colors.accent)
scene.play([nodes.animate.remove(2)])
scene.wait(0.4)
seeks.extend([start + 0.3, start + 0.7, scene.cursor - 0.1])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
