#!/usr/bin/env python3
"""Guard production/test separation against Rust literal and attribute traps."""
import importlib.util
import pathlib
import unittest

spec = importlib.util.spec_from_file_location('duplication', pathlib.Path(__file__).with_name('check-duplication.py'))
duplication = importlib.util.module_from_spec(spec)
spec.loader.exec_module(duplication)


class Separation(unittest.TestCase):
    def test_inline_module_preserves_production_after_it(self):
        source = '\n'.join(['fn before() {}', '#[cfg(test)]', 'mod tests {',
            'let raw = r##"} #[cfg(test)] {"##;',
            "let brace = '}'; /* outer { /* nested } */ } */", '}', 'fn after() {}'])
        production, tests = duplication.split(source)
        self.assertIn('fn before()', production)
        self.assertIn('fn after()', production)
        self.assertNotIn('let raw', production)
        self.assertIn('let raw', tests)
        self.assertNotIn('fn after()', tests)

    def test_individual_tests_and_test_imports(self):
        production, tests = duplication.split('#[cfg(test)] use crate::{A, B};\n#[test] fn case() {}\n#[tokio::test] async fn async_case() {}\nfn live() {}')
        self.assertNotIn('fn case', production)
        self.assertIn('use crate', tests)
        self.assertIn('async_case', tests)
        self.assertIn('fn live', production)

    def test_hook_features_are_production(self):
        source = '#[cfg(any(test, feature = "test-hooks"))] fn hook() {}\n#[cfg(feature = "test-hooks")] fn other() {}\n#[cfg(not(test))] fn live() {}'
        production, tests = duplication.split(source)
        self.assertEqual(tests, '')
        self.assertEqual(production, source)

    def test_test_and_feature_conjunction_is_test_only(self):
        production, tests = duplication.split('#[cfg(all(test, feature = "test-hooks"))] mod tests { fn case() {} }')
        self.assertFalse(production.strip())
        self.assertIn('fn case', tests)

    def test_lifetimes_and_escaped_strings_do_not_hide_items(self):
        source = 'fn live<' + "'a>(x: &'a str) {}" + '\n#[test] fn case() { let s = "escaped \\" }"; }'
        production, tests = duplication.split(source)
        self.assertIn('fn live', production)
        self.assertIn('fn case', tests)


if __name__ == '__main__':
    unittest.main()
