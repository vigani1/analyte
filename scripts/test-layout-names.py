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

    def test_test_folders_follow_source_name_rules(self):
        # Review 8 Y4: test trees use the same name rules as source folders.
        for name in ("hl7.rs", "hl7_send.rs"):
            self.assertTrue(layout.violations([f"crates/analyte/tests/hl7/{name}"], {}))
        files = [f"crates/steps/dialog/tests/case{i}.rs" for i in range(11)]
        self.assertTrue(layout.violations(files, {}))

    def test_test_topic_prefix_cannot_stand_in_for_a_folder(self):
        beside_file = ["crates/steps/dialog/tests/hl7.rs", "crates/steps/dialog/tests/hl7_boundaries.rs"]
        beside_folder = ["crates/analyte/tests/orders/hl7/query.rs", "crates/analyte/tests/orders/hl7_send.rs"]
        siblings = ["crates/protocols/hl7/tests/law_order.rs", "crates/protocols/hl7/tests/law_patient.rs"]
        folders = ["crates/protocols/hl7/tests/law/mod.rs", "crates/protocols/hl7/tests/law_order/mod.rs"]
        for files in (beside_file, beside_folder, siblings, folders):
            errors = layout.violations(files, {})
            self.assertTrue(errors, files)
            self.assertTrue(all("stands in for a folder" in error for error in errors), errors)
        grouped = [
            "crates/steps/dialog/tests/hl7/mod.rs",
            "crates/steps/dialog/tests/hl7/boundaries.rs",
            "crates/steps/dialog/tests/boundaries.rs",
            "crates/core/config/tests/hl7_editions.rs",
        ]
        self.assertFalse(layout.violations(grouped, {}))

    def test_source_folders_keep_their_own_prefix_rule(self):
        # The topic-prefix rule covers test trees; source stems that only share
        # a word with a sibling are judged by the folder-name rule alone.
        files = ["crates/analyte/src/runtime/test_hooks.rs", "crates/analyte/src/runtime/test_tracing.rs"]
        self.assertFalse(layout.violations(files, {}))


if __name__ == "__main__":
    unittest.main()
