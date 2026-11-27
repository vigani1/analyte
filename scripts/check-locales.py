#!/usr/bin/env python3
"""Check embedded UI message IDs and report translation coverage."""

from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[1]
MESSAGE = re.compile(r"^([a-z][a-z0-9-]*)\s*=", re.MULTILINE)
USE = re.compile(
    r'\b(?:analyte_i18n::(?:text|message|count)|tr|tr_args|tr_count|nonzero_count)'
    r'\(\s*"([a-z][a-z0-9-]*)"'
)
REFERENCE = re.compile(r"\{\s*([a-z][a-z0-9-]*)\s*\}")


def production_sources():
    """Read both programs' Rust sources and Askama templates."""
    for crate in ("analyte", "app"):
        root = ROOT / "crates" / crate
        yield from (root / "templates").rglob("*.html")
        for path in (root / "src").rglob("*.rs"):
            if "tests" not in path.parts and path.name != "tests.rs":
                yield path


def catalog(folder):
    messages = {}
    errors = []
    for ftl in sorted(folder.glob("*.ftl")):
        source = ftl.read_text(encoding="utf-8")
        found = list(MESSAGE.finditer(source))
        for index, match in enumerate(found):
            key = match[1]
            end = found[index + 1].start() if index + 1 < len(found) else len(source)
            if key in messages:
                errors.append(f"{ftl.relative_to(ROOT)}: duplicate message {key}")
            messages[key] = set(REFERENCE.findall(source[match.end():end]))
    return messages, errors


def main():
    locales = ROOT / "locales"
    english, errors = catalog(locales / "en")
    uses = {}
    for source in production_sources():
        for key in USE.findall(source.read_text(encoding="utf-8")):
            uses.setdefault(key, source.relative_to(ROOT))
    pending = list(uses)
    while pending:
        key = pending.pop()
        for reference in english.get(key, set()) - uses.keys():
            pending.append(reference)
            uses[reference] = f"Fluent reference from {key}"
    for key in sorted(uses.keys() - english.keys()):
        errors.append(f"{uses[key]}: English message missing: {key}")
    for key in sorted(english.keys() - uses.keys()):
        errors.append(f"locales/en: unused message: {key}")

    if not english:
        errors.append("locales/en: source catalog is empty")
    for language in sorted(path for path in locales.iterdir() if path.is_dir()):
        translated, duplicates = catalog(language)
        if language.name != "en":
            errors.extend(duplicates)
        unknown = translated.keys() - english.keys()
        if unknown:
            errors.append(f"{language.name}: unknown message IDs: {', '.join(sorted(unknown))}")
        common = len(translated.keys() & english.keys())
        coverage = common * 100 / max(len(english), 1)
        print(f"{language.name}: {common}/{len(english)} messages ({coverage:.1f}%)")
    for error in errors:
        print(error)
    return bool(errors)


if __name__ == "__main__":
    raise SystemExit(main())
