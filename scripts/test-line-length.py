#!/usr/bin/env python3
"""Regression cases for the production line-length gate."""
import importlib.util
import pathlib
import unittest

spec = importlib.util.spec_from_file_location("length", pathlib.Path(__file__).with_name("check-line-length.py"))
length = importlib.util.module_from_spec(spec)
spec.loader.exec_module(length)


class Gate(unittest.TestCase):
    def test_new_code_and_literals_fail(self):
        for line in ["fn " + "x" * 150 + "() {}", 'const X: &str = "' + "x" * 150 + '";']:
            self.assertEqual(list(length.violations(line, [])), [1])

    def test_exact_literal_allowance_is_counted(self):
        line = 'const X: &str = "' + "x" * 150 + '";'
        self.assertEqual(list(length.violations(line, [line])), [])
        self.assertEqual(list(length.violations(line + "\n" + line, [line])), [2])
        self.assertEqual(list(length.violations(line.replace("xxx", "yyy", 1), [line])), [1])

    def test_tests_and_comments_are_excluded(self):
        source = "// " + "x" * 150 + "\n#[cfg(test)]\nmod tests { const X: &str = \"" + "x" * 150 + "\"; }"
        self.assertEqual(list(length.violations(source, [])), [])


if __name__ == "__main__":
    unittest.main()
