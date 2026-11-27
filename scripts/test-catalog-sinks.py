#!/usr/bin/env python3
"""Regression checks for the catalog sink checker."""

import importlib.util
import sys
import unittest

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("checker", "scripts/check-catalog-sinks.py")
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)

HEADER = "use analyte_i18n::{message as tr_args, text as tr};\n"


def run(body):
    return [line for line, _ in checker.problems(HEADER + "fn f() {\n" + body + "\n}\n")]


class SinkTests(unittest.TestCase):
    def test_lookups_in_sinks_fail(self):
        for body in [
            'let p = std::path::Path::new(&root)\n    .join(tr("a"));',
            'let p = Path::new(tr("a"));',
            'path.push(tr_args("a", []));',
            'let c = Command::new(tr("a"));',
            'command.arg(tr("a"));',
            'command.args([tr("a")]);',
            'let v = std::env::var_os(tr("a"));',
            'let h = HeaderValue::from_str(&tr("a"));',
            'response.header(tr("a"), "x");',
            'let f = File::create(tr("a"));',
            'std::fs::write(tr("a"), b"");',
            'let f = path.with_file_name(analyte_i18n::text("a"));',
            'let f = folder.join(nonzero_count("a", 1));',
        ]:
            with self.subTest(body=body):
                self.assertNotEqual(run(body), [])

    def test_a_tainted_name_fails(self):
        self.assertEqual(run('let name = tr("a");\nlet p = Path::new(&name);'), [4])

    def test_ordinary_text_passes(self):
        for body in [
            'let p = Path::new(r"C:\\Windows").join("System32");',
            'rows.push(tr("a"));',
            'let s = words.join(tr("separator"));',
            'command.arg("--name").arg(&value);',
            'let p = Path::new("tr(x)");',
            'out.push_str(tr("a"));',
        ]:
            with self.subTest(body=body):
                self.assertEqual(run(body), [])

    def test_taint_stays_in_its_function(self):
        source = HEADER + 'fn a() { let name = tr("a"); }\nfn b(name: &str) { Path::new(name); }\n'
        self.assertEqual(list(checker.problems(source)), [])


if __name__ == "__main__":
    unittest.main()
