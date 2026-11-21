#!/usr/bin/env python3
"""Regression checks for the Markdown link checker."""

import importlib.util
import sys
import unittest

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("checker", "scripts/check-links.py")
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)

FILES = {
    "docs/a.md": "# Title\n\n## Two words\n\n## Two words\n",
    "docs/images/shot.png": "",
    "crates/core": "",
}


def run(text, path="docs/b.md"):
    return list(checker.problems(path, text, FILES.__contains__, FILES.__getitem__))


class LinkTests(unittest.TestCase):
    def test_resolving_links_pass(self):
        for text in [
            "[a](a.md)", "[a](./a.md#two-words)", "[a](a.md#two-words-1)", "[t](#here)\n# Here",
            "![s](images/shot.png)", '<img src="images/shot.png">', "[c](../crates/core/)",
            "[w](https://example.org/x.md)", "[m](mailto:someone@example.org)", "[r]: a.md",
        ]:
            with self.subTest(text=text):
                self.assertEqual(run(text), [])

    def test_broken_links_fail(self):
        for text, message in [
            ("[a](gone.md)", "dead link gone.md"),
            ("![s](images/missing.png)", "dead link images/missing.png"),
            ('<img src="missing.svg">', "dead link missing.svg"),
            ("[a](a.md#nowhere)", "missing anchor a.md#nowhere"),
            ("[t](#nowhere)", "missing anchor #nowhere"),
            ("[r]: gone.md", "dead link gone.md"),
        ]:
            with self.subTest(text=text):
                self.assertEqual(run(text), [(1, message)])

    def test_code_is_not_checked_and_lines_are_kept(self):
        text = "```\n[a](gone.md)\n```\n`[b](gone.md)`\n[c](gone.md)"
        self.assertEqual(run(text), [(5, "dead link gone.md")])


if __name__ == "__main__":
    unittest.main()
