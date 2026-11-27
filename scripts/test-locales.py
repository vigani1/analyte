#!/usr/bin/env python3
"""Regression checks for the catalog checker's per-file rules."""

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("checker", "scripts/check-locales.py")
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


def read(text):
    with tempfile.TemporaryDirectory() as folder:
        checker.ROOT = Path(folder)
        (Path(folder) / "ui.ftl").write_text(text, encoding="utf-8", newline="")
        return checker.catalog(Path(folder))


class CatalogTests(unittest.TestCase):
    def test_control_characters_fail(self):
        # A French catalog once had colons replaced by U+0001.
        for character in ["\x01", "\x00", "\x1f", "\x7f", "\x0b"]:
            with self.subTest(character=hex(ord(character))):
                _, _, errors = read(f"a = Port{character} 1\n")
                self.assertEqual(errors, [f"ui.ftl:1: control character U+{ord(character):04X}"])

    def test_tab_and_line_ends_pass(self):
        _, _, errors = read("a = one\tcell\r\nb = two\n")
        self.assertEqual(errors, [])

    def test_variables_are_read_from_every_variant_and_not_comments(self):
        _, variables, _ = read(
            "# $ignored\n"
            "a = { $count ->\n    [one] { $number } item\n   *[other] { $number } items\n}\n"
            "b = { $state -> [added] Added *[saved] Saved }: { $name }\n"
        )
        self.assertEqual(variables, {"a": {"count", "number"}, "b": {"state", "name"}})


if __name__ == "__main__":
    unittest.main()
