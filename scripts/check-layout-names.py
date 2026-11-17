#!/usr/bin/env python3
"""Enforce ADR 0012 file names and direct Rust file counts."""
from collections import defaultdict
from pathlib import Path
import subprocess
import sys


def violations(files, exceptions):
    folders = defaultdict(list)
    errors = []
    for name in files:
        path = Path(name)
        if not name.startswith(("crates/", "fuzz/")) or path.suffix != ".rs":
            continue
        folder = path.parent.as_posix()
        folders[folder].append(name)
        if path.stem == path.parent.name or path.stem.startswith(path.parent.name + "_"):
            errors.append(f"{name}: file name repeats its folder")
    for folder, names in folders.items():
        if len(names) > 10 and folder not in exceptions:
            errors.append(f"{folder}: {len(names)} direct Rust files exceeds ten; group by role")
    for folder in exceptions:
        if len(folders[folder]) <= 10:
            errors.append(f"{folder}: stale layout exception")
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
