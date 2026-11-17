#!/usr/bin/env python3
"""Exercise checklist test discovery with synthetic Rust source."""
import runpy
import pathlib
import unittest

collect = runpy.run_path(str(pathlib.Path(__file__).with_name("check-protocol-tests.py")))["test_functions"]


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


if __name__ == "__main__":
    unittest.main()
