#!/usr/bin/env python3
"""Keep out-of-line modules and imports before bodies in Rust front files."""
import importlib.util
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('duplication', ROOT / 'scripts/check-duplication.py')
duplication = importlib.util.module_from_spec(spec)
spec.loader.exec_module(duplication)


def violations(source):
    mask = duplication.code_mask(source)
    depth, depths = 0, []
    for char in mask:
        depths.append(depth)
        if char == '{':
            depth += 1
        elif char == '}':
            depth -= 1
    declarations = []
    pattern = r'(?m)^(?:pub(?:\([^\n]*?\))?\s+)?(?:mod\s+\w+\s*;|use\s+)'
    for match in re.finditer(pattern, mask):
        if depths[match.start()] == 0:
            declarations.append((match.start(), mask.find(';', match.end() - 1) + 1))
    covered = set()
    for start, end in declarations:
        covered.update(range(start, end))
    body = next((i for i, char in enumerate(mask)
                 if char == '{' and depths[i] == 0 and i not in covered), len(mask))
    return [source.count('\n', 0, start) + 1 for start, _ in declarations if start > body]


def main():
    failures = []
    for owner in ('crates', 'fuzz'):
        for path in (ROOT / owner).rglob('*.rs'):
            if path.name not in ('lib.rs', 'mod.rs', 'main.rs') or 'target' in path.parts:
                continue
            for line in violations(path.read_text(encoding='utf-8')):
                failures.append(f'{path.relative_to(ROOT)}:{line}: declaration after first item body')
    if failures:
        raise SystemExit('\n'.join(failures))
    print('Front-file declarations passed')


if __name__ == '__main__':
    main()
