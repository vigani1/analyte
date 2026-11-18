#!/usr/bin/env python3
"""Check that every test named in a protocol checklist exists and runs.

A backticked name in a checklist is a test reference. Two exact markers change
its meaning, and each marks only the one name that follows it:

- ``Planned test: `name` `` names future work. It is not checked, and it must
  not name an existing test (then it is a real reference).
- ``Manual test: `name` `` names an ``#[ignore]`` test that a person runs on
  purpose. It must exist and be ignored.

Every other reference must name a test that CI runs: an ignored test does not
back a checklist row unless the checklist says it is manual. Test names are
short behaviour names, so a referenced name must be defined only once in the
workspace; otherwise another test of the same name could hide a missing one.
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
# Configuration values written in backticks in checklists; not test references.
CONFIG_VALUES = {"law_manager", "independent_per_transaction"}
REFERENCE = re.compile(r"(?:(Planned|Manual) test: )?`([^`]+)`")


def test_functions(source):
    """Map each attributed or table-macro test to whether it is ignored."""
    functions = {}
    for match in re.finditer(
        r"((?:#\[[^\]]*\]\s*)+)(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)\s*\(",
        source,
    ):
        attributes = match[1]
        if re.search(r"#\[(?:tokio::)?test(?:\s*\([^]]*\))?\s*\]", attributes):
            functions[match[2]] = bool(re.search(r"#\[ignore\b", attributes))
    for name in re.findall(r"\b(?:negative|bad_order_reply|row)!\s*\(\s*(\w+)", source):
        functions[name] = False
    return functions


def references(text):
    """Yield (marker, name) for each test reference; marker is None, Planned or Manual."""
    for paragraph in text.split("\n\n"):
        for marker, token in REFERENCE.findall(paragraph):
            name = token.rsplit("::", 1)[-1]
            if re.fullmatch(r"[a-z][a-z0-9_]+", name) and name not in CONFIG_VALUES:
                yield marker or None, name


def check(checklists, functions, definitions=None):
    """Return (checked reference names, errors) for {label: checklist text}.

    ``definitions`` maps a test name to how many files define it.
    """
    definitions = definitions or {}
    checked = set()
    errors = []
    for label, text in checklists.items():
        for marker, name in references(text):
            ignored = functions.get(name)
            if marker == "Planned":
                if ignored is not None:
                    errors.append(f"{label}: planned test {name} exists; reference it as a test")
                continue
            checked.add(name)
            if ignored is None:
                errors.append(f"{label}: missing test {name}")
            elif definitions.get(name, 1) > 1:
                errors.append(f"{label}: test {name} is defined {definitions[name]} times; rename one")
            elif marker == "Manual" and not ignored:
                errors.append(f"{label}: manual test {name} is not #[ignore]")
            elif marker is None and ignored:
                errors.append(f"{label}: test {name} is #[ignore]; mark it Manual test or run it")
    return checked, errors


def main():
    functions = {}
    definitions = {}
    for path in (ROOT / "crates").rglob("*.rs"):
        for name, ignored in test_functions(path.read_text(encoding="utf-8")).items():
            functions[name] = functions.get(name, True) and ignored
            definitions[name] = definitions.get(name, 0) + 1
    checklists = {
        path.relative_to(ROOT).as_posix(): path.read_text(encoding="utf-8")
        for path in sorted((ROOT / "docs/protocols").rglob("checklist.md"))
    }
    checked, errors = check(checklists, functions, definitions)
    if errors:
        print("\n".join(errors))
        return 1
    print(f"Protocol checklists: {len(checked)} named tests, 0 missing")
    return 0


if __name__ == "__main__":
    sys.exit(main())
