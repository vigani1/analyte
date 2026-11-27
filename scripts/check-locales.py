#!/usr/bin/env python3
"""Check embedded UI message IDs and report translation coverage.

Also fails when a catalog file holds a control character (below U+0020 other
than tab, LF and CR, or U+007F), when a translation has a message ID English
lacks, or when a translated message uses other Fluent variables than English.
"""

from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[1]
MESSAGE = re.compile(r"^([a-z][a-z0-9-]*)\s*=", re.MULTILINE)
USE = re.compile(
    r'\b(?:analyte_i18n::(?:text|message|count)|tr|tr_args|tr_count|nonzero_count)'
    r'\(\s*"([a-z][a-z0-9-]*)"'
)
# The i18n crate formats shared calendar names with its own `message`.
SHARED_USE = re.compile(r'\bmessage\(\s*"([a-z][a-z0-9-]*)"')
SHARED = Path("crates") / "core" / "i18n" / "src" / "lib.rs"
REFERENCE = re.compile(r"\{\s*([a-z][a-z0-9-]*)\s*\}")
VARIABLE = re.compile(r"\$([a-zA-Z][a-zA-Z0-9_-]*)")
CONTROL = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")


def production_sources():
    """Read both programs' Rust sources and Askama templates."""
    for crate in ("analyte", "app"):
        root = ROOT / "crates" / crate
        yield from (root / "templates").rglob("*.html")
        for path in (root / "src").rglob("*.rs"):
            if "tests" not in path.parts and path.name != "tests.rs":
                yield path


def control_characters(name, source):
    """Report each control character a catalog file must not hold."""
    for number, line in enumerate(source.split("\n"), 1):
        for found in CONTROL.finditer(line):
            yield f"{name}:{number}: control character U+{ord(found[0]):04X}"


def catalog(folder):
    """Each message's references and variables, plus the file problems found."""
    messages = {}
    variables = {}
    errors = []
    for ftl in sorted(folder.glob("*.ftl")):
        source = ftl.read_text(encoding="utf-8")
        name = ftl.relative_to(ROOT).as_posix()
        errors.extend(control_characters(name, source))
        found = list(MESSAGE.finditer(source))
        for index, match in enumerate(found):
            key = match[1]
            end = found[index + 1].start() if index + 1 < len(found) else len(source)
            if key in messages:
                errors.append(f"{name}: duplicate message {key}")
            body = "\n".join(
                line for line in source[match.end():end].split("\n")
                if not line.lstrip().startswith("#")
            )
            messages[key] = set(REFERENCE.findall(body))
            variables[key] = set(VARIABLE.findall(body))
    return messages, variables, errors


def main():
    locales = ROOT / "locales"
    english, english_variables, errors = catalog(locales / "en")
    uses = {}
    for source in production_sources():
        for key in USE.findall(source.read_text(encoding="utf-8")):
            uses.setdefault(key, source.relative_to(ROOT))
    shared = (ROOT / SHARED).read_text(encoding="utf-8")
    shared = shared.split("#[cfg(test)]", 1)[0]
    for key in SHARED_USE.findall(shared):
        uses.setdefault(key, SHARED)
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
        translated, variables, problems = catalog(language)
        if language.name != "en":
            errors.extend(problems)
        unknown = translated.keys() - english.keys()
        if unknown:
            errors.append(f"{language.name}: unknown message IDs: {', '.join(sorted(unknown))}")
        for key in sorted(translated.keys() & english.keys()):
            if variables[key] != english_variables[key]:
                found = ", ".join(sorted(variables[key])) or "none"
                wanted = ", ".join(sorted(english_variables[key])) or "none"
                errors.append(f"{language.name}: {key} uses variables {found}, English {wanted}")
        common = len(translated.keys() & english.keys())
        coverage = common * 100 / max(len(english), 1)
        print(f"{language.name}: {common}/{len(english)} messages ({coverage:.1f}%)")
    for error in errors:
        print(error)
    return bool(errors)


if __name__ == "__main__":
    raise SystemExit(main())
