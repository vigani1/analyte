#!/usr/bin/env python3
"""Check that every relative link and image in tracked Markdown resolves, using only the Python standard library."""

import os
import posixpath
import re
import subprocess
import sys


# Inline links and images, reference definitions and HTML image sources.
LINK = re.compile(r"\]\(\s*<?([^)\s>]+)>?(?:\s+\"[^\"]*\")?\s*\)|^\s*\[[^\]]+\]:\s*(\S+)|<img\b[^>]*\bsrc=\"([^\"]+)\"", re.M)
FENCE = re.compile(r"^ {0,3}(```|~~~).*?^ {0,3}\1[^\n]*$", re.M | re.S)
CODE = re.compile(r"(`+)(?:(?!\1).)+?\1", re.S)
HEADING = re.compile(r"^ {0,3}#{1,6}\s+(.*?)\s*#*\s*$", re.M)


def without_code(text):
    """Blank fenced blocks and inline code so example links are not checked; keep line numbers."""
    blank = lambda match: re.sub(r"[^\n]", " ", match.group())
    return CODE.sub(blank, FENCE.sub(blank, text))


def anchors(text):
    """Return GitHub's heading anchors: lower case, punctuation dropped, spaces as hyphens, repeats numbered."""
    seen = {}
    out = set()
    for match in HEADING.finditer(without_code(text)):
        heading = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", match.group(1))
        slug = re.sub(r"[^\w\- ]", "", heading.replace("`", "").strip().lower()).replace(" ", "-")
        count = seen.get(slug, 0)
        seen[slug] = count + 1
        out.add(slug if count == 0 else f"{slug}-{count}")
    return out


def problems(path, text, exists, read):
    """Yield (line, message) for each link in `text` (from `path`) that does not resolve."""
    body = without_code(text)
    for match in LINK.finditer(body):
        target = next(group for group in match.groups() if group)
        if re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", target):
            continue
        line = body.count("\n", 0, match.start()) + 1
        file_part, _, anchor = target.partition("#")
        resolved = posixpath.normpath(posixpath.join(posixpath.dirname(path), file_part)) if file_part else path
        if file_part and not exists(resolved):
            yield line, f"dead link {target}"
        elif anchor and resolved.endswith(".md") and anchor not in anchors(read(resolved) if file_part else text):
            yield line, f"missing anchor {target}"


def main():
    tracked = [p for p in subprocess.check_output(["git", "ls-files", "-z"]).decode("utf-8").split("\0") if p]
    known = set(tracked)
    for item in tracked:
        parent = posixpath.dirname(item)
        while parent and parent not in known:
            known.add(parent)
            parent = posixpath.dirname(parent)
    read = lambda p: open(p, encoding="utf-8").read()
    failed = False
    for path in (p for p in tracked if p.endswith(".md") and os.path.exists(p)):
        for line, message in problems(path, read(path), known.__contains__, read):
            print(f"{path}:{line}: {message}")
            failed = True
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
