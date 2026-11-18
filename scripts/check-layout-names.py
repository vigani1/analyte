#!/usr/bin/env python3
"""Enforce ADR 0012 file names and direct Rust file counts.

Source and test folders share the rules: no file stem repeats its folder, a
folder holds at most ten direct Rust files, and no entry uses a topic prefix in
place of a folder: `hl7_send.rs` beside `hl7.rs` or `hl7/`, or two siblings
starting `hl7_`, belong in an `hl7/` folder. The prefix rule covers every
folder of a crate's module trees, from its `src/` or `tests/` folder down.
"""
from collections import defaultdict
from pathlib import Path
import subprocess
import sys


MODULE_ROOTS = ("src", "tests")


def module_root(path):
    """Index of the `src/` or `tests/` folder whose module tree holds the path, if any."""
    for index, part in enumerate(path.parts[:-1]):
        if part in MODULE_ROOTS:
            return index
    return None


def prefix_violations(folders):
    """Report entries whose `<topic>_` prefix stands in for a `<topic>/` folder."""
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
    tree_entries = defaultdict(set)
    errors = []
    for name in files:
        path = Path(name)
        if not name.startswith(("crates/", "fuzz/")) or path.suffix != ".rs":
            continue
        folder = path.parent.as_posix()
        folders[folder].append(name)
        if path.stem == path.parent.name or path.stem.startswith(path.parent.name + "_"):
            errors.append(f"{name}: file name repeats its folder")
        root = module_root(path)
        if root is not None:
            # Each source or test folder lists its file stems and its child folders.
            if path.stem != "mod":
                tree_entries[folder].add(path.stem)
            for depth in range(root + 1, len(path.parts) - 1):
                tree_entries[Path(*path.parts[:depth]).as_posix()].add(path.parts[depth])
    for folder, names in folders.items():
        if len(names) > 10 and folder not in exceptions:
            errors.append(f"{folder}: {len(names)} direct Rust files exceeds ten; group by role")
    for folder in exceptions:
        if len(folders[folder]) <= 10:
            errors.append(f"{folder}: stale layout exception")
    errors.extend(prefix_violations(tree_entries))
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
