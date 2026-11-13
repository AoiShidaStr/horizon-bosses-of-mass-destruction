#!/usr/bin/env python3
"""Generate the four boss textures (original placeholder art, no third-party assets).

Each texture is 64x64 and is painted with a small deterministic palette so the
boxes in the code-built models (api/src/model.rs) read as separate materials.
Run: python tools/make_textures.py
"""
import os
import random
import struct
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "content", "assets", "bosses_of_mass_destruction",
                   "textures", "entity")


def write_png(path, width, height, pixels):
    stride = width * 4
    raw = b"".join(b"\x00" + bytes(pixels[y * stride:(y + 1) * stride]) for y in range(height))

    def chunk(kind, data):
        body = kind + data
        return (struct.pack(">I", len(data)) + body
                + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF))

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
           + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))
    with open(path, "wb") as f:
        f.write(png)


def texture(seed, base, dark, glow, veins):
    """A 64x64 stone/void/cloth texture: base colour, noise, a glow region and veins."""
    rng = random.Random(seed)
    w = h = 64
    px = [[0, 0, 0, 255] for _ in range(w * h)]

    def put(x, y, colour, jitter=8):
        if 0 <= x < w and 0 <= y < h:
            r, g, b = colour
            j = rng.randint(-jitter, jitter)
            px[y * w + x] = [max(0, min(255, r + j)), max(0, min(255, g + j)),
                             max(0, min(255, b + j)), 255]

    for y in range(h):
        for x in range(w):
            shade = rng.random()
            colour = base if shade > 0.25 else dark
            put(x, y, colour, 10)
    # Glow bands (what the model uses for the core / eye region).
    for y in range(8, 40):
        for x in range(4, 28):
            if (x + y) % 7 < 2:
                put(x, y, glow, 6)
    # Veins across the whole sheet.
    for _ in range(veins):
        x, y = rng.randrange(w), rng.randrange(h)
        for _ in range(rng.randint(6, 22)):
            put(x, y, glow, 4)
            x += rng.choice((-1, 0, 1))
            y += rng.choice((-1, 0, 1))
    # A dark 1px border around every 16px cell so box edges stay legible.
    for y in range(h):
        for x in range(w):
            if x % 16 == 0 or y % 16 == 0:
                r, g, b, _ = px[y * w + x]
                px[y * w + x] = [int(r * 0.72), int(g * 0.72), int(b * 0.72), 255]
    return w, h, sum(px, [])


PALETTES = {
    # Obsidilith: purple-black obsidian with a violet crystal core.
    "obsidilith": (11, (38, 28, 52), (16, 12, 26), (168, 96, 232), 90),
    # Lich: bleached bone and cold blue soul-fire.
    "lich": (23, (206, 202, 186), (128, 126, 116), (86, 200, 226), 40),
    # Gauntlet: iron and gold with an amber core.
    "gauntlet": (37, (176, 168, 156), (92, 88, 84), (240, 176, 72), 55),
    # Void Blossom: deep teal petals with magenta eyes.
    "void_blossom": (51, (26, 64, 66), (10, 30, 34), (236, 90, 188), 75),
}


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, (seed, base, dark, glow, veins) in PALETTES.items():
        w, h, pixels = texture(seed, base, dark, glow, veins)
        path = os.path.join(OUT, name + ".png")
        write_png(path, w, h, pixels)
        print("wrote", os.path.relpath(path))


if __name__ == "__main__":
    main()
