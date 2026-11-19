#!/usr/bin/env python3
"""Make the Windows icon analyte.ico from the app icon SVGs, drawn by headless Microsoft Edge.

Run from anywhere with `python -I scripts/make-app-icon.py [--edge PATH]`. It
uses only the Python standard library and an installed Microsoft Edge (no new
dependency). Frames 16 and 20 come from the hand-tuned `-16`/`-20` files and
24, 32, 48 and 256 from the large icon, never the other way round
(docs/design/HANDOFF.md, "Logo"). Each frame is a PNG with transparent
corners; Windows reads PNG frames at every size.
"""

import argparse
import pathlib
import struct
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
ICONS = ROOT / "crates" / "analyte" / "assets" / "icons"
OUTPUT = ICONS / "analyte.ico"
LARGE = "analyte-app-icon.svg"
FRAMES = [
    (16, "analyte-app-icon-16.svg"),
    (20, "analyte-app-icon-20.svg"),
    (24, LARGE),
    (32, LARGE),
    (48, LARGE),
    (256, LARGE),
]
EDGE = [
    pathlib.Path(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
    pathlib.Path(r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"),
]
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def png_size(data):
    """Return (width, height) of a 32-bit RGBA PNG, or raise if it is anything else."""
    if data[:8] != PNG_SIGNATURE or data[12:16] != b"IHDR":
        raise ValueError("not a PNG")
    width, height, depth, colour = struct.unpack(">IIBB", data[16:26])
    if (depth, colour) != (8, 6):
        raise ValueError(f"PNG is not 8-bit RGBA (depth {depth}, colour type {colour})")
    return width, height


def draw(edge, svg, size, folder):
    """Draw `svg` at `size` x `size` pixels with a transparent background; return the PNG bytes."""
    page = folder / f"frame-{size}.html"
    shot = folder / f"frame-{size}.png"
    page.write_text(
        "<!doctype html><html><head><meta charset=\"utf-8\"><style>"
        "html,body{margin:0;background:transparent}img{display:block}"
        f"</style></head><body><img src=\"{svg.as_uri()}\" width=\"{size}\" height=\"{size}\" alt=\"\">"
        "</body></html>",
        encoding="utf-8",
    )
    subprocess.run(
        [
            str(edge),
            "--headless=new",
            "--disable-gpu",
            "--hide-scrollbars",
            "--no-first-run",
            "--force-device-scale-factor=1",
            "--force-color-profile=srgb",
            "--default-background-color=00000000",
            f"--window-size={size},{size}",
            f"--user-data-dir={folder / 'profile'}",
            "--allow-file-access-from-files",
            f"--screenshot={shot}",
            page.as_uri(),
        ],
        check=True,
        capture_output=True,
        timeout=120,
    )
    data = shot.read_bytes()
    if png_size(data) != (size, size):
        raise ValueError(f"{svg.name} at {size} px came out {png_size(data)}")
    return data


def pack(frames):
    """An ICO file holding each (size, png) frame, smallest first."""
    header = struct.pack("<HHH", 0, 1, len(frames))
    offset = len(header) + 16 * len(frames)
    entries = b""
    for size, png in frames:
        # A 256 px frame is written as 0 in the one-byte width and height.
        side = 0 if size == 256 else size
        entries += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(png), offset)
        offset += len(png)
    return header + entries + b"".join(png for _, png in frames)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--edge", type=pathlib.Path, help="path to msedge.exe")
    edge = parser.parse_args().edge or next((path for path in EDGE if path.is_file()), None)
    if edge is None or not edge.is_file():
        sys.exit("Microsoft Edge not found; pass --edge with the path to msedge.exe")
    with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as temp:
        folder = pathlib.Path(temp)
        frames = [(size, draw(edge, ICONS / name, size, folder)) for size, name in FRAMES]
    OUTPUT.write_bytes(pack(frames))
    print(f"{OUTPUT.relative_to(ROOT).as_posix()}: " + ", ".join(f"{size} px from {name}" for size, name in FRAMES))


if __name__ == "__main__":
    main()
