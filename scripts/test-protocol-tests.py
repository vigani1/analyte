#!/usr/bin/env python3
"""Exercise checklist test discovery with synthetic Rust source."""
import runpy
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent
checker = runpy.run_path(str(pathlib.Path(__file__).with_name("check-protocol-tests.py")))
collect = checker["test_functions"]
check = checker["check"]


class Discovery(unittest.TestCase):
    def test_rejects_non_test_function(self):
        self.assertNotIn("helper", collect("fn helper() {}"))

    def test_attributes_and_macro_tests(self):
        source = """
#[test]
fn sync_test() {}
#[tokio::test(flavor = "multi_thread")]
async fn async_test() {}
#[test]
#[ignore]
fn ignored_test() {}
negative!(negative_case, anything);
bad_order_reply!(reply_case, anything);
row!(row_case, anything);
"""
        self.assertEqual(collect(source), {
            "sync_test", "async_test", "ignored_test", "negative_case", "reply_case", "row_case"
        })


class Planned(unittest.TestCase):
    def test_planned_marker_skips_only_its_own_name(self):
        text = "- [x] Row.\n  Tests: `real_test`, `renamed_test`.\n  Planned test: `future_test`.\n"
        checked, errors = check({"list.md": text}, {"real_test"})
        self.assertEqual(checked, {"real_test", "renamed_test"})
        self.assertEqual(errors, ["list.md: missing test renamed_test"])

    def test_planned_name_that_exists_is_rejected(self):
        _, errors = check({"list.md": "Planned test: `real_test`."}, {"real_test"})
        self.assertEqual(errors, ["list.md: planned test real_test exists; reference it as a test"])

    def test_other_planned_wording_is_checked(self):
        for text in ("Planned tests: `future_test`.", "Planned broader qualification: `future_test`."):
            _, errors = check({"list.md": text}, set())
            self.assertEqual(errors, ["list.md: missing test future_test"])

    def test_renamed_law_test_fails_the_gate(self):
        # The LAW enhanced-content paragraph ends in Planned lines; review 9 Z1
        # found the old paragraph-wide skip hid every reference in it.
        name = "law_tf2b_c14_requested_and_applied_priority_preserve_exact_code"
        path = ROOT / "docs/protocols/law/checklist.md"
        text = path.read_text(encoding="utf-8")
        self.assertTrue(re.search(rf"`{name}`[^\n]*\n(?:[^\n]+\n)*[^\n]*Planned test: `", text))
        sources = [p.read_text(encoding="utf-8") for p in (ROOT / "crates").rglob("*.rs")]
        functions = set().union(*(collect(source) for source in sources))
        checked, errors = check({"law": text}, functions)
        self.assertIn(name, checked)
        self.assertEqual(errors, [])
        renamed = [re.sub(rf"\bfn {name}\(", f"fn {name}_renamed(", source) for source in sources]
        self.assertNotEqual(renamed, sources)
        functions = set().union(*(collect(source) for source in renamed))
        _, errors = check({"law": text}, functions)
        self.assertEqual(errors, [f"law: missing test {name}"])


if __name__ == "__main__":
    unittest.main()
