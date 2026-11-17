#!/usr/bin/env python3
"""Bound production Rust lines; grandfather exact existing literal lines only."""
import collections
import importlib.util
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("duplication", ROOT / "scripts/check-duplication.py")
duplication = importlib.util.module_from_spec(spec)
spec.loader.exec_module(duplication)


def candidates(source):
    """Ignore test-only items and comments, preserving physical line numbers."""
    masked = duplication.code_mask(source)
    regions = duplication.test_regions(source)
    offset = 0
    for number, (line, code) in enumerate(zip(source.splitlines(True), masked.splitlines(True)), 1):
        test = any(start <= offset < end for start, end in regions)
        offset += len(line)
        text = line.rstrip()
        if not test and len(text) > 140 and not text.lstrip().startswith("//"):
            yield number, text, text != code.rstrip()


def violations(source, allowed):
    remaining = collections.Counter(allowed)
    for number, text, literal in candidates(source):
        if literal and remaining[text]:
            remaining[text] -= 1
        else:
            yield number


def main():
    allowed = json.loads((ROOT / "scripts/line-length-allowlist.json").read_text(encoding="utf-8"))
    failures = []
    for path in sorted((ROOT / "crates").rglob("*.rs")):
        relative = path.relative_to(ROOT).as_posix()
        if "tests" in path.parts or "testing" in path.parts:
            continue
        for number in violations(path.read_text(encoding="utf-8"), allowed.get(relative, [])):
            failures.append(f"{relative}:{number}: production line exceeds 140 characters")
    if failures:
        raise SystemExit("\n".join(failures))
    print("Production line lengths passed (140 characters; exact legacy literals allowed)")


if __name__ == "__main__":
    main()
