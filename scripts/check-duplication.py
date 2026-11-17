#!/usr/bin/env python3
"""Measure production and test Rust separately, including inline unit tests."""
import argparse
import pathlib
import re
import shutil
import subprocess
import tempfile


def code_mask(source):
    """Hide comments and literals while preserving offsets and newlines."""
    token = re.compile(r"//[^\n]*|/\*|(?:br|cr|r)(\#*)\"|(?:b|c)?\"|'(?:\\(?:[^\n]|u\{[^}]*\})|[^'\\\n])'")
    masked = list(source)
    pos = 0
    while match := token.search(source, pos):
        start, pos = match.span()
        opening = match.group()
        if opening == '/*':
            depth = 1
            while depth:
                nested = re.search(r'/\*|\*/', source[pos:])
                if not nested:
                    raise ValueError('unterminated Rust comment')
                pos += nested.end()
                depth += 1 if nested.group() == '/*' else -1
        elif opening.startswith('//') or opening.startswith("'"):
            pass
        elif match.group(1) is not None:
            closing = '"' + match.group(1)
            end = source.find(closing, pos)
            if end < 0:
                raise ValueError('unterminated Rust raw string')
            pos = end + len(closing)
        else:
            while True:
                closing = re.search(r'\\[\s\S]|"', source[pos:])
                if not closing:
                    raise ValueError('unterminated Rust string')
                pos += closing.end()
                if closing.group() == '"':
                    break
        masked[start:pos] = ['\n' if c == '\n' else ' ' for c in source[start:pos]]
    return ''.join(masked)


def closing(mask, start, left, right):
    depth = 0
    for pos in range(start, len(mask)):
        if mask[pos] == left:
            depth += 1
        elif mask[pos] == right:
            depth -= 1
            if depth == 0:
                return pos + 1
    raise ValueError('unbalanced Rust item')


def test_regions(source):
    """Find test-only attributed items; braces inside literals cannot end items."""
    mask = code_mask(source)
    regions = []
    covered = 0
    for match in re.finditer(r'#\s*\[', mask):
        if match.start() < covered:
            continue
        start = match.start()
        end = closing(mask, match.end() - 1, '[', ']')
        attr = mask[match.end():end - 1]
        # Quoted feature names are masked, so test-hooks does not match test.
        if not (re.search(r'^\s*(?:tokio::)?test\s*$', attr)
                or (re.match(r'\s*cfg\s*\(', attr) and re.search(r'\btest\b', attr)
                    and not re.search(r'\b(?:not|any)\s*\(', attr))):
            continue
        while True:
            next_pos = end
            while next_pos < len(mask) and mask[next_pos].isspace():
                next_pos += 1
            if mask.startswith('#[', next_pos):
                end = closing(mask, next_pos + 1, '[', ']')
            else:
                break
        delimiter = re.search(r'[;{]', mask[end:])
        if not delimiter:
            raise ValueError('test attribute has no item')
        item = end + delimiter.start()
        end = item + 1 if mask[item] == ';' else closing(mask, item, '{', '}')
        regions.append((start, end))
        covered = end
    return regions


def split(source):
    regions = test_regions(source)
    production, tests, cursor = [], [], 0
    for start, end in regions:
        production.append(source[cursor:start])
        tests.append(source[start:end])
        cursor = end
    production.append(source[cursor:])
    return '\n'.join(production), '\n'.join(tests)



def prepare(root, output):
    sources = {p: p.read_text(encoding='utf-8') for p in (root / 'crates').rglob('*.rs') if 'target' not in p.parts}
    test_files = {p for p in sources if 'tests' in p.parts or p.name == 'tests.rs' or p.stem.endswith('_tests')}
    # Follow cfg(test) out-of-line modules, including their child files.
    for path, source in sources.items():
        for start, end in test_regions(source):
            declaration = re.search(r'\bmod\s+(\w+)\s*;', code_mask(source[start:end]))
            if declaration:
                owner = path.parent if path.name in ('mod.rs', 'lib.rs', 'main.rs') else path.parent / path.stem
                base = owner / declaration.group(1)
                test_files.update(p for p in sources if p == base.with_suffix('.rs') or base in p.parents)
    exclusions = {line for line in (root / 'scripts/duplication-exclusions.txt').read_text().splitlines() if line and not line.startswith('#')}
    for path, source in sources.items():
        relative = path.relative_to(root)
        production, tests = ('', source) if path in test_files else split(source)
        for kind, content in [('production', production), ('tests', tests)]:
            if content.strip() and not (kind == 'production' and relative.as_posix() in exclusions):
                target = output / kind / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(content, encoding='utf-8', newline='\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--test-threshold', type=float, default=2.95)
    args = parser.parse_args()
    root = pathlib.Path(__file__).resolve().parent.parent
    runner = shutil.which('npx.cmd') or shutil.which('npx')
    if not runner:
        raise SystemExit('npx is required for pinned jscpd 5.4.2')
    with tempfile.TemporaryDirectory(prefix='analyte-duplication-') as temp:
        output = pathlib.Path(temp)
        prepare(root, output)
        for kind, threshold in [('production', 0.3), ('tests', args.test_threshold)]:
            print(f'{kind}: duplication threshold {threshold}%', flush=True)
            subprocess.run([runner, '--yes', 'jscpd@5.4.2', '--format', 'rust', '--min-lines', '8', '--min-tokens', '70', '--threshold', str(threshold), '--reporters', 'console', str(output / kind)], check=True)


if __name__ == '__main__':
    main()
