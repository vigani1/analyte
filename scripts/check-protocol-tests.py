#!/usr/bin/env python3
"""Check that every test named in a protocol checklist exists.

A backticked name in a checklist is a test reference. The exact marker
``Planned test: `name` `` names future work and marks only the one name that
follows it: that name is not checked, and it must not name an existing test
(then it is a real reference).
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
# Configuration values written in backticks in checklists; not test references.
CONFIG_VALUES = {"law_manager", "independent_per_transaction"}
REFERENCE = re.compile(r"(Planned test: )?`([^`]+)`")


def test_functions(source):
    """Collect attributed tests and the repository's explicit test-table macros."""
    functions = set()
    for match in re.finditer(
        r"((?:#\[[^\]]*\]\s*)+)(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)\s*\(",
        source,
    ):
        if re.search(r"#\[(?:tokio::)?test(?:\s*\([^]]*\))?\s*\]", match[1]):
            functions.add(match[2])
    functions.update(re.findall(r"\b(?:negative|bad_order_reply|row)!\s*\(\s*(\w+)", source))
    return functions


def references(text):
    """Yield (planned, name) for each test reference in one checklist."""
    for paragraph in text.split("\n\n"):
        for marker, token in REFERENCE.findall(paragraph):
            name = token.rsplit("::", 1)[-1]
            if re.fullmatch(r"[a-z][a-z0-9_]+", name) and name not in CONFIG_VALUES:
                yield bool(marker), name


def check(checklists, functions):
    """Return (checked reference names, errors) for {label: checklist text}."""
    checked = set()
    errors = []
    for label, text in checklists.items():
        for planned, name in references(text):
            if planned:
                if name in functions:
                    errors.append(f"{label}: planned test {name} exists; reference it as a test")
                continue
            checked.add(name)
            if name not in functions:
                errors.append(f"{label}: missing test {name}")
    return checked, errors


def main():
    sources = [path.read_text(encoding="utf-8") for path in (ROOT / "crates").rglob("*.rs")]
    functions = set().union(*(test_functions(source) for source in sources))
    checklists = {
        path.relative_to(ROOT).as_posix(): path.read_text(encoding="utf-8")
        for path in sorted((ROOT / "docs/protocols").rglob("checklist.md"))
    }
    checked, errors = check(checklists, functions)
    if errors:
        print("\n".join(errors))
        return 1
    print(f"Protocol checklists: {len(checked)} named tests, 0 missing")
    return 0


if __name__ == "__main__":
    sys.exit(main())
