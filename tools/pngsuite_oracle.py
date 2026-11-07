#!/usr/bin/env python3
"""PngSuite oracle for the PNG format, run locally (Pillow; never a
dependency of the plugin).

  pngsuite_oracle.py     writes tests/fixtures/pngsuite/expected.txt: for
                         each PngSuite file, its size and the FNV-1a
                         64-bit hash of its pixels as 8-bit RGBA, rows from the top (16-bit
                         samples by their high byte), as Pillow decodes it;
                         `error` for the files PngSuite marks corrupt (x*).
"""
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
SUITE = ROOT / 'tests/fixtures/pngsuite'


def rgba(image: Image.Image, depth: int) -> bytes:
    """8-bit RGBA, 16-bit samples by their high byte. Grey images are
    converted here, `tRNS` as the PNG specification says: Pillow compares
    a 1, 2 or 4-bit file's transparent value unscaled with its scaled
    samples, and its own conversion of 16-bit grey clips."""
    if image.mode not in ('L', 'I;16', 'I;16B', 'I'):
        return image.convert('RGBA').tobytes()
    clear = image.info.get('transparency')
    if image.mode == 'L' and clear is not None and depth < 8:
        clear = clear * 255 // ((1 << depth) - 1)
    values = getattr(image, 'get_flattened_data', image.getdata)()
    out = bytearray()
    for v in values:
        g = v if image.mode == 'L' else min(max(v, 0), 65535) >> 8
        out += bytes((g, g, g, 0 if v == clear else 255))
    return bytes(out)


def fnv(data: bytes) -> str:
    """FNV-1a, 64 bits, as hex: what the Rust test computes too."""
    value = 0xcbf29ce484222325
    for byte in data:
        value = ((value ^ byte) * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return f'{value:016x}'


def main() -> None:
    lines = []
    for path in sorted(SUITE.glob('*.png')):
        if path.name.startswith('x'):
            lines.append(f'{path.name} error')
            continue
        with Image.open(path) as image:
            image.load()
            depth = path.read_bytes()[24]
            digest = fnv(rgba(image, depth))
            lines.append(f'{path.name} {image.width} {image.height} {digest}')
    (SUITE / 'expected.txt').write_text('\n'.join(lines) + '\n')


if __name__ == '__main__':
    main()
