#!/usr/bin/env python3
"""Require the CI fuzz matrix to cover every manifest binary exactly once."""
import pathlib
import re
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
manifest = tomllib.loads((ROOT / "fuzz/Cargo.toml").read_text(encoding="utf-8"))
workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
matrix = re.search(r"        target: \[(.*?)\]", workflow)
if matrix is None:
    raise SystemExit("Fuzz target matrix missing")
targets = [name.strip() for name in matrix[1].split(",")]
expected = sorted(binary["name"] for binary in manifest["bin"])
if sorted(targets) != expected:
    raise SystemExit(f"Fuzz matrix {targets} differs from manifest {expected}")
print(f"Fuzz matrix covers all {len(expected)} manifest targets")
