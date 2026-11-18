#!/usr/bin/env python3
"""Regression checks for corruption detection and legitimate question marks."""

import importlib.util
import pathlib
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("checker", "scripts/check-code-page.py")
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class CodePageTests(unittest.TestCase):
    def test_corruption(self):
        for text in ["TF-2b ?3.29.4", "LIS01-A2 ??6.3.3", "60?86400 seconds", "§2.15.5.3?4", "(?3"]:
            for source in [f'// {text}', f'/* outer /* nested */ {text} */', f'let x = "{text}";', f'let x = r##"{text}"##;']:
                with self.subTest(source=source):
                    self.assertTrue(list(checker.hits("test.rs", source)))
            self.assertTrue(list(checker.hits("test.md", text)))

    def test_legitimate_syntax(self):
        for source in [
            'db.prepare("SELECT x FROM t WHERE a = ?1")?;',
            'db.execute(r#"INSERT INTO t VALUES (?1, ?2)"#, [])?;',
            'format!("{selection} WHERE ?1 IS NULL");',
            'query_row("SELECT ?1", [], f)?;',
            'let x = "https://example.org/1?2";',
            'Regex::new(r"( ?1|2?3)")?;',
            'RegexBuilder::new("( ?1)");',
            'foo()?; let x = \'?\'; let life: &\'a str;',
        ]:
            with self.subTest(source=source):
                self.assertEqual(list(checker.hits("test.rs", source)), [])

    def test_exemptions_do_not_hide_neighboring_damage(self):
        source = '// https://example.org/1?2 section ?3\nlet x = "SELECT 60?86400";'
        self.assertEqual([line for line, _ in checker.hits("test.rs", source)], [1, 2])

    def test_multiline_offsets(self):
        source = 'let x = r###"first\nsecond ?2\n"###;\n/*\nthird 3?4\n*/'
        self.assertEqual([line for line, _ in checker.hits("test.rs", source)], [2, 5])

    def test_tracked_files_diagnostics(self):
        script = pathlib.Path("scripts/check-code-page.py").resolve()
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            subprocess.run(["git", "init", "-q", directory], check=True)
            (root / "note.md").write_text("title\nsection ?2\n", encoding="utf-8")
            (root / "untracked.md").write_text("section ?3", encoding="utf-8")
            subprocess.run(["git", "add", "note.md"], cwd=root, check=True)
            def run():
                return subprocess.run([sys.executable, str(script)], cwd=root, capture_output=True, text=True)
            result = run()
            self.assertEqual(result.returncode, 1)
            self.assertIn("note.md:2:", result.stdout)
            self.assertNotIn("untracked", result.stdout)
            (root / "note.md").write_text("section 2\n", encoding="utf-8")
            self.assertEqual(run().returncode, 0)


if __name__ == "__main__":
    unittest.main()
