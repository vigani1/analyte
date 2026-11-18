#!/usr/bin/env python3
"""Detect lossy Windows code-page saves using only the Python standard library."""

import pathlib
import re
import subprocess
import sys


SUSPECT = re.compile(r"(?<=[ (?])\?+[0-9]|\?\?[0-9]|[0-9]\?[0-9]")
URL = re.compile(r"[a-zA-Z][a-zA-Z0-9+.-]*://[^\s<>\"`]+")
SQL = re.compile(r"(?:^\s*(?:SELECT|INSERT|UPDATE|DELETE|WITH|PRAGMA|CREATE|ALTER|DROP|REPLACE|VALUES|EXPLAIN)\b|\bWHERE\s)", re.I)


def rust_text(source):
    """Yield comments and string contents with offsets, skipping Rust operators.

    Handle nested block comments, escaped strings, raw strings and character
    literals. Lifetimes are left as code rather than consumed as characters.
    """
    token = re.compile(r'''//[^\n]*|/\*|(?:br|cr|r)(\#*)"|(?:b|c)?"|'(?:\\(?:[^\n]|u\{[^}]*\})|[^'\\\n])' ''', re.X)
    pos = 0
    while match := token.search(source, pos):
        start = match.start()
        opening = match.group()
        pos = match.end()
        if opening.startswith("'"):
            continue
        if opening.startswith("//"):
            yield start, opening, False
            continue
        if opening == "/*":
            depth = 1
            while depth and (nested := re.search(r"/\*|\*/", source[pos:])):
                pos += nested.end()
                depth += 1 if nested.group() == "/*" else -1
            yield start, source[start:pos], False
            continue
        content_start = pos
        if match.group(1) is not None:
            end = source.find('"' + match.group(1), pos)
            end = len(source) if end < 0 else end
            pos = end + 1 + len(match.group(1))
        else:
            closing = re.search(r'\\[\s\S]|"', source[pos:])
            while closing and closing.group() != '"':
                pos += closing.end()
                closing = re.search(r'\\[\s\S]|"', source[pos:])
            end = pos + closing.start() if closing else len(source)
            pos = end + 1
        # Regex constructors have syntax in which question marks are meaningful.
        regex = re.search(r"\b(?:Regex|RegexBuilder|RegexSet)::new\s*\(\s*$", source[max(0, start - 120):start])
        if not regex:
            yield content_start, source[content_start:end], True


def hits(path, source):
    regions = rust_text(source) if path.endswith(".rs") else [(0, source, False)]
    for offset, content, string in regions:
        urls = [match.span() for match in URL.finditer(content)]
        sql = string and SQL.search(content)
        for match in SUSPECT.finditer(content):
            if any(a <= match.start() < b for a, b in urls):
                continue
            # Exempt only numbered SQL parameters, never ranges or doubled '?'.
            if sql and re.fullmatch(r"\?[0-9]", match.group()):
                continue
            line = source.count("\n", 0, offset + match.start()) + 1
            yield line, source.splitlines()[line - 1]


def main():
    # No exceptions: reword a legitimate hit instead of exempting it.
    files = subprocess.check_output(["git", "ls-files", "-z", "--", "*.md", "*.rs"]).decode("utf-8").split("\0")
    failed = False
    for path in filter(None, files):
        source = pathlib.Path(path).read_text(encoding="utf-8")
        for number, line in hits(path, source):
            print(f"{path}:{number}: possible code-page corruption: {line.strip()}")
            failed = True
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
