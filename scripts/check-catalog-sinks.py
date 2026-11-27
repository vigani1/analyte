#!/usr/bin/env python3
"""Fail when translated text reaches a path, command, env var, header or file name.

A catalog message is wording for people. A translator may change any of it, so
a message must never become part of something a program reads: a file path, a
command or its arguments, an environment variable, an HTTP header, or a file
name. See docs/develop/adding-a-language.md.

How it works, per Rust source file under crates/ (test code included):

* A *lookup* is a call to an `analyte_i18n` function (`text`, `message`,
  `count`, or the name a `use analyte_i18n::...` line gives it in that file,
  usually `tr`, `tr_args`, `tr_count`) or to `nonzero_count`, which wraps it.
* A *tainted name* is a variable bound by `let NAME = <expression>;` whose
  expression contains a lookup, earlier in the same function. Only plain `let`
  bindings are followed; values that pass through struct fields or function
  returns are not.
* A *sink* is one of the calls in `SINKS`. Its argument list is read up to the
  matching parenthesis, across lines, and fails the check when it contains a
  lookup or a tainted name.

`.join(` and `.push(` are sinks only when the expression they are called on
(followed back over chained lines) mentions a path (`Path`, `path`, `dir`,
`root`, `folder`, `file`), because `Vec::push` and `Vec<String>::join` with
translated text are ordinary text.
"""

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]

# Calls whose arguments a program, the OS or a peer reads.
SINKS = [
    r"\bPath::new\(",
    r"\bPathBuf::from\(",
    r"\.with_file_name\(",
    r"\.set_file_name\(",
    r"\.with_extension\(",
    r"\.set_extension\(",
    r"\bCommand::new\(",
    r"\.arg\(",
    r"\.args\(",
    r"\benv::var\(",
    r"\benv::var_os\(",
    r"\benv::set_var\(",
    r"\benv::remove_var\(",
    r"\.env\(",
    r"\.env_remove\(",
    r"\bHeaderName::from_\w+\(",
    r"\bHeaderValue::from_\w+\(",
    r"\.header\(",
    r"\.insert_header\(",
    r"\bFile::create\(",
    r"\bFile::open\(",
    r"\bfs::\w+\(",
    r"\.open\(",
]
SINK = re.compile("|".join(f"(?:{pattern})" for pattern in SINKS))
# `Vec<String>` has `join` and `push` too, so these count only on a path.
PATH_CALL = re.compile(r"\.(?:join|push)\(")
PATHISH = re.compile(r"Path|path|dir|root|folder|file")
IMPORT = re.compile(r"use\s+analyte_i18n::(\{[^}]*\}|\w+(?:\s+as\s+\w+)?)\s*;")
ALIAS = re.compile(r"(\w+)(?:\s+as\s+(\w+))?")
FUNCTION = re.compile(r"\bfn\s+\w+")
LET = re.compile(r"\blet\s+(?:mut\s+)?(\w+)\s*(?::[^=;]+)?=")


def lookup_pattern(source):
    """Names that look up a catalog message in this file."""
    names = {"nonzero_count"}
    for found in IMPORT.finditer(source):
        for alias in ALIAS.finditer(found[1].strip("{}")):
            if alias[1] in {"text", "message", "count"}:
                names.add(alias[2] or alias[1])
    plain = "|".join(sorted(map(re.escape, names)))
    return re.compile(rf"\banalyte_i18n::\w+\(|(?<![\w.:])(?:{plain})\(")


def skip_string(source, index):
    """The index just past the string literal that opens at `index`."""
    index += 1
    while index < len(source) and source[index] != '"':
        index += 2 if source[index] == "\\" else 1
    return index + 1


def arguments(source, start):
    """The text between the parenthesis opened before `start` and its match."""
    depth = 1
    index = start
    while index < len(source) and depth:
        character = source[index]
        if character == '"':
            index = skip_string(source, index)
            continue
        if character == "(":
            depth += 1
        elif character == ")":
            depth -= 1
        index += 1
    return source[start:index - 1]


def statement(source, start):
    """The text of a `let` statement from `start` to its closing semicolon."""
    depth = 0
    index = start
    while index < len(source):
        character = source[index]
        if character == '"':
            index = skip_string(source, index)
            continue
        if character in "([{":
            depth += 1
        elif character in ")]}":
            depth -= 1
            if depth < 0:
                break
        elif character == ";" and depth == 0:
            break
        index += 1
    return source[start:index]


def receiver(source, start):
    """The expression a method call at `start` is made on, across chained lines."""
    line_start = source.rfind("\n", 0, start) + 1
    text = source[line_start:start]
    while not text.strip() and line_start > 0:
        previous = source.rfind("\n", 0, line_start - 1) + 1
        text = source[previous:line_start - 1] + text
        line_start = previous
    return re.split(r"=|\blet\b|\breturn\b", text.strip())[-1]


def without_strings(text):
    """`text` with every string literal emptied, so names inside them don't count."""
    return re.sub(r'"(?:\\.|[^"\\])*"', '""', text)


def problems(source):
    """Yield (line, message) for each catalog lookup that reaches a sink."""
    lookup = lookup_pattern(source)
    functions = [found.start() for found in FUNCTION.finditer(source)]
    lets = [
        (found.start(), found[1])
        for found in LET.finditer(source)
        if lookup.search(statement(source, found.end()))
    ]
    sinks = list(SINK.finditer(source))
    sinks += [
        found
        for found in PATH_CALL.finditer(source)
        if PATHISH.search(receiver(source, found.start()))
    ]
    for found in sorted(sinks, key=lambda match: match.start()):
        text = without_strings(arguments(source, found.end()))
        body = max((start for start in functions if start < found.start()), default=0)
        tainted = {name for start, name in lets if body <= start < found.start()}
        if lookup.search(text) or any(re.search(rf"\b{name}\b", text) for name in tainted):
            line = source.count("\n", 0, found.start()) + 1
            call = found[0].rstrip("(")
            yield line, f"catalog text reaches {call}(...)"


def main():
    failed = False
    for path in sorted((ROOT / "crates").rglob("*.rs")):
        if "target" in path.parts:
            continue
        for line, message in problems(path.read_text(encoding="utf-8")):
            print(f"{path.relative_to(ROOT).as_posix()}:{line}: {message}")
            failed = True
    if failed:
        print("Catalog text must not build paths, commands, env vars, headers or file names.")
        return 1
    print("Catalog sinks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
