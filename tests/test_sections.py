"""Pure authoring tests; do not import the host-owned native runtime."""

import importlib.util
from pathlib import Path
import sys
import unittest

SOURCE = Path(__file__).resolve().parents[1] / "crates/gaanim_python/gaanim/sections.py"
spec = importlib.util.spec_from_file_location("section_helpers_under_test", SOURCE)
sections = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = sections
spec.loader.exec_module(sections)
Section, SectionStep = sections.Section, sections.SectionStep


class SceneRecorder:
    def __init__(self):
        self.events = []

    def segment(self, name, transition, **metadata):
        handle = object()
        self.events.append(("segment", name, transition, metadata, handle))
        return handle

    def stop(self):
        self.events.append(("stop",))


class SectionTests(unittest.TestCase):
    def test_entry_order_metadata_identity_stops_and_repeated_visits(self):
        scene = SceneRecorder()
        progress = []

        def body(original):
            self.assertIs(original, scene)
            scene.events.append(("body",))
            scene.stop()
            scene.stop()

        def enter(original, context):
            self.assertIs(original, scene)
            self.assertEqual(scene.events[-1][0], "segment")
            self.assertIs(context.segment, scene.events[-1][-1])
            progress.append(context)
            scene.events.append(("entry",))

        section = Section("context", [
            SectionStep(name="A", build=body, notes="speaker notes", background="white"),
            SectionStep(name="B", build=body, transition="fade"),
        ])
        handles = section.build(scene, on_enter=enter)
        section.build(scene, on_enter=enter)
        self.assertEqual([p.fraction for p in progress], [0.5, 1, 0.5, 1])
        self.assertEqual([p.visit for p in progress], [1, 1, 2, 2])
        opened = [e for e in scene.events if e[0] == "segment"]
        self.assertEqual(len({e[1] for e in opened}), 4)
        self.assertEqual(handles, tuple(e[-1] for e in opened[:2]))
        self.assertEqual(opened[0][3]["notes"], "speaker notes")
        self.assertEqual(opened[0][3]["background"], "white")
        self.assertEqual(opened[1][2], "fade")
        self.assertEqual([e[0] for e in scene.events[:5]],
                         ["segment", "entry", "body", "stop", "stop"])

    def test_invalid_input_has_no_authoring_side_effects(self):
        with self.assertRaises(ValueError):
            Section("empty", [])
        with self.assertRaises(TypeError):
            Section("invalid", [lambda scene: None])
        with self.assertRaises(ValueError):
            SectionStep(name=" ", build=lambda scene: None)
        with self.assertRaises(TypeError):
            SectionStep(name="A", build=None)
        scene = SceneRecorder()
        section = Section("A", [SectionStep(name="a", build=lambda scene: None)])
        with self.assertRaises(TypeError):
            section.build(scene, on_enter=1)
        self.assertEqual(scene.events, [])

    def test_builder_exception_propagates_and_next_visit_is_unique(self):
        def fail(scene):
            raise RuntimeError("content failed")
        scene = SceneRecorder()
        section = Section("A", [SectionStep(name="a", build=fail)])
        for _ in range(2):
            with self.assertRaisesRegex(RuntimeError, "content failed"):
                section.build(scene)
        self.assertNotEqual(scene.events[0][1], scene.events[1][1])


if __name__ == "__main__":
    unittest.main()
