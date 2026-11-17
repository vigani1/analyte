#!/usr/bin/env python3
"""Test ADR 0012 name and folder checks with synthetic inventories."""
import importlib.util
import unittest

spec = importlib.util.spec_from_file_location("layout", "scripts/check-layout-names.py")
layout = importlib.util.module_from_spec(spec)
spec.loader.exec_module(layout)


class LayoutTests(unittest.TestCase):
    def test_repeated_folder_name(self):
        for name in ("astm.rs", "astm_frame.rs"):
            self.assertTrue(layout.violations([f"crates/protocols/astm/{name}"], {}))
        self.assertFalse(layout.violations(["crates/protocols/astm/frame.rs"], {}))

    def test_folder_limit_and_exception(self):
        files = [f"crates/core/model/src/field{i}.rs" for i in range(11)]
        self.assertFalse(layout.violations(files[:10], {}))
        self.assertTrue(layout.violations(files, {}))
        exceptions = {"crates/core/model/src": "Synthetic justified exception"}
        self.assertFalse(layout.violations(files, exceptions))
        self.assertTrue(layout.violations(files[:10], exceptions))


if __name__ == "__main__":
    unittest.main()
