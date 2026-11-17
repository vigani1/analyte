#!/usr/bin/env python3
"""Enforce ADR 0012 file names and direct Rust file counts.

Source and test folders share the rules: no file stem repeats its folder, and a
folder holds at most ten direct Rust files. Test folders also may not use a
topic prefix in place of a folder: `hl7_send.rs` beside `hl7.rs` or `hl7/`,
or two siblings starting `hl7_`, belong in an `hl7/` folder.
"""
from collections import defaultdict
from pathlib import Path
import subprocess
import sys


def is_test(path):
    """Whether a path lies in an integration test tree (a `tests/` folder)."""
    return "tests" in path.parts[:-1]


def prefix_violations(folders):
    """Report test entries whose `<topic>_` prefix stands in for a `<topic>/` folder."""
    errors = []
    for folder, entries in sorted(folders.items()):
        topics = defaultdict(list)
        for entry in entries:
            topic, separator, _ = entry.partition("_")
            if separator:
                topics[topic].append(entry)
        for topic, names in sorted(topics.items()):
            if len(names) > 1 or topic in entries:
                for name in sorted(names):
                    errors.append(
                        f"{folder}/{name}: prefix {topic}_ stands in for a folder; move into {topic}/"
                    )
    return errors


def violations(files, exceptions):
    folders = defaultdict(list)
    test_entries = defaultdict(set)
    errors = []
    for name in files:
        path = Path(name)
        if not name.startswith(("crates/", "fuzz/")) or path.suffix != ".rs":
            continue
        folder = path.parent.as_posix()
        folders[folder].append(name)
        if path.stem == path.parent.name or path.stem.startswith(path.parent.name + "_"):
            errors.append(f"{name}: file name repeats its folder")
        if is_test(path):
            # Each test folder lists its file stems and its child folders.
            if path.stem != "mod":
                test_entries[folder].add(path.stem)
            for depth in range(path.parts.index("tests") + 1, len(path.parts) - 1):
                test_entries[Path(*path.parts[:depth]).as_posix()].add(path.parts[depth])
    for folder, names in folders.items():
        if len(names) > 10 and folder not in exceptions:
            errors.append(f"{folder}: {len(names)} direct Rust files exceeds ten; group by role")
    for folder in exceptions:
        if len(folders[folder]) <= 10:
            errors.append(f"{folder}: stale layout exception")
    errors.extend(prefix_violations(test_entries))
    return errors


def main():
    exceptions = {}
    for line in Path("scripts/layout-allowlist.txt").read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        folder, separator, reason = line.partition(" | ")
        if not separator or not reason.strip() or folder in exceptions:
            raise ValueError(f"invalid layout exception: {line}")
        exceptions[folder] = reason
    files = subprocess.check_output(["git", "ls-files", "-z"], text=True).split("\0")
    errors = violations(files, exceptions)
    for error in errors:
        print(error)
    return bool(errors)


if __name__ == "__main__":
    sys.exit(main())
