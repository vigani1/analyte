#!/usr/bin/env python3
"""Check executable test references; explicitly Planned paragraphs are future work."""
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
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


sources = [p.read_text(encoding="utf-8") for p in (ROOT / "crates").rglob("*.rs")]
functions = set().union(*(test_functions(source) for source in sources))
missing = []
references = set()
for path in sorted((ROOT / "docs/protocols").rglob("checklist.md")):
    for paragraph in path.read_text(encoding="utf-8").split("\n\n"):
        if "Planned" in paragraph:
            continue
        for token in re.findall(r"`([^`]+)`", paragraph):
            name = token.rsplit("::", 1)[-1]
            if not re.fullmatch(r"[a-z][a-z0-9_]+", name):
                continue
            # Configuration values are not test references.
            if name in {"law_manager", "independent_per_transaction"}:
                continue
            references.add(name)
            if name not in functions:
                missing.append(f"{path.relative_to(ROOT)}: missing test {name}")
if missing:
    raise SystemExit("\n".join(missing))
print(f"Protocol checklists: {len(references)} named tests, 0 missing")
