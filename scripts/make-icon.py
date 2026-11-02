#!/usr/bin/env python3
"""Generate res/tree.ico — a rounded slate tile with a white download arrow.

Pure stdlib: PNG frames (Vista+ ICO) built with zlib, drawn at 4x and box-filtered
so the edges are clean at every size.
"""

import struct
import zlib
from pathlib import Path

SIZES = [16, 24, 32, 48, 64, 128, 256]
SS = 4  # supersampling factor


def png(width: int, height: int, rgba: bytes) -> bytes:
    raw = b"".join(
        b"\x00" + rgba[y * width * 4 : (y + 1) * width * 4] for y in range(height)
    )

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def blend(dst, i, r, g, b, a):
    """Alpha-composite (r,g,b,a in 0..1) onto the float buffer at pixel i."""
    da = dst[i + 3]
    na = a + da * (1 - a)
    if na <= 0:
        return
    for k, v in enumerate((r, g, b)):
        dst[i + k] = (v * a + dst[i + k] * da * (1 - a)) / na
    dst[i + 3] = na


def draw(size: int) -> bytearray:
    n = size * SS
    buf = [0.0] * (n * n * 4)
    radius = n * 0.22
    cx = cy = n / 2

    for y in range(n):
        for x in range(n):
            i = (y * n + x) * 4
            # rounded-rect coverage via distance to the inner rect
            dx = max(abs(x - cx) - (n / 2 - radius), 0.0)
            dy = max(abs(y - cy) - (n / 2 - radius), 0.0)
            if (dx * dx + dy * dy) ** 0.5 > radius:
                continue
            t = y / n
            r = 0.145 + (0.105 - 0.145) * t
            g = 0.165 + (0.125 - 0.165) * t
            b = 0.215 + (0.165 - 0.215) * t
            blend(buf, i, r, g, b, 1.0)

            def arrow(px, py):
                shaft_w = n * 0.135
                if n * 0.20 <= py <= n * 0.55 and abs(px - cx) <= shaft_w / 2:
                    return True
                if n * 0.50 <= py <= n * 0.735:
                    half = (n * 0.235) * (1 - (py - n * 0.50) / (n * 0.235))
                    if abs(px - cx) <= half:
                        return True
                if n * 0.775 <= py <= n * 0.835 and abs(px - cx) <= n * 0.245:
                    return True
                return False

            if arrow(x + 0.5, y + 0.5):
                blend(buf, i, 1.0, 1.0, 1.0, 1.0)

    out = bytearray(size * size * 4)
    for y in range(size):
        for x in range(size):
            acc = [0.0, 0.0, 0.0, 0.0]
            for sy in range(SS):
                for sx in range(SS):
                    src = ((y * SS + sy) * n + (x * SS + sx)) * 4
                    for k in range(4):
                        acc[k] += buf[src + k]
            dst = (y * size + x) * 4
            count = SS * SS
            a = acc[3] / count
            for k in range(3):
                out[dst + k] = int(max(0, min(255, (acc[k] / count) * 255)))
            out[dst + 3] = int(max(0, min(255, a * 255)))
    return out


def ico(frames):
    header = struct.pack("<HHH", 0, 1, len(frames))
    offset = 6 + 16 * len(frames)
    entries = b""
    payload = b""
    for size, data in frames:
        dim = 0 if size >= 256 else size
        entries += struct.pack(
            "<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset + len(payload)
        )
        payload += data
    return header + entries + payload


def main():
    out = Path(__file__).resolve().parent.parent / "crates" / "tree-win" / "res" / "tree.ico"
    frames = [(s, png(s, s, bytes(draw(s)))) for s in SIZES]
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(ico(frames))
    print(f"wrote {out} ({out.stat().st_size} bytes, {len(frames)} frames)")


if __name__ == "__main__":
    main()
