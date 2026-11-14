#!/usr/bin/env python3
"""Draw original pixel-art texture sheets for the four code-built boss models."""

import os
import random
import struct
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "content", "assets", "bosses_of_mass_destruction",
                   "textures", "entity")
SIZE = 256


def write_png(path, pixels):
    stride = SIZE * 4
    raw = b"".join(b"\x00" + bytes(pixels[y * stride:(y + 1) * stride]) for y in range(SIZE))

    def chunk(kind, data):
        body = kind + data
        return (struct.pack(">I", len(data)) + body
                + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF))

    header = struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0)
    png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
           + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))
    with open(path, "wb") as f:
        f.write(png)


class Sheet:
    def __init__(self):
        self.x = 0
        self.y = 0
        self.row = 0

    def box(self, pixels, rng, size, base, shadow, highlight):
        dx, dy, dz = size
        w, h = 2 * dz + 2 * dx, dz + dy
        if self.x + w > SIZE:
            self.x = 0
            self.y += self.row
            self.row = 0
        if self.y + h > SIZE:
            raise ValueError(f"{size} box exceeds the {SIZE}x{SIZE} model sheet")
        u, v = self.x, self.y
        self.x += w
        self.row = max(self.row, h)

        for py in range(v, v + h):
            for px in range(u, u + w):
                edge = px in (u, u + w - 1) or py in (v, v + h - 1)
                colour = shadow if edge else base
                if not edge and rng.random() < 0.075:
                    colour = highlight if rng.random() < 0.65 else shadow
                self.pixel(pixels, px, py, colour)
        return (u, v, dx, dy, dz)

    @staticmethod
    def pixel(pixels, x, y, colour):
        if 0 <= x < SIZE and 0 <= y < SIZE:
            at = (y * SIZE + x) * 4
            pixels[at:at + 4] = (*colour, 255)

    @classmethod
    def rect(cls, pixels, x, y, w, h, colour):
        for py in range(y, y + h):
            for px in range(x, x + w):
                cls.pixel(pixels, px, py, colour)

    @staticmethod
    def front(box):
        u, v, dx, dy, dz = box
        return u + dz, v + dz, dx, dy


def layout(name, seed):
    rng = random.Random(seed)
    pixels = bytearray(SIZE * SIZE * 4)
    sheet = Sheet()
    b = lambda size, base, shadow, highlight: sheet.box(
        pixels, rng, size, base, shadow, highlight
    )

    if name == "obsidilith":
        dark = (35, 25, 48)
        shadow = (13, 10, 22)
        glint = (88, 65, 110)
        legs = b((22, 20, 16), dark, shadow, glint)
        torso = b((26, 24, 20), dark, shadow, glint)
        head = b((14, 14, 14), dark, shadow, glint)
        core = b((6, 6, 2), (143, 72, 204), (58, 34, 96), (220, 158, 255))
        shards = [b((6, 10, 6), dark, shadow, (119, 72, 154)) for _ in range(2)]
        arms = [b((8, 30, 8), dark, shadow, glint) for _ in range(2)]
        x, y, w, h = Sheet.front(head)
        Sheet.rect(pixels, x + 3, y + 6, 3, 2, (193, 111, 255))
        Sheet.rect(pixels, x + 8, y + 6, 3, 2, (193, 111, 255))
        x, y, w, h = Sheet.front(core)
        Sheet.rect(pixels, x + 1, y + 1, w - 2, h - 2, (197, 112, 247))
        Sheet.rect(pixels, x + 2, y + 2, w - 4, h - 4, (238, 196, 255))
        for box in (legs, torso, *shards, *arms):
            x, y, w, h = Sheet.front(box)
            for offset in range(2, min(w, h) - 2, 7):
                Sheet.pixel(pixels, x + offset, y + offset, (131, 81, 168))
                Sheet.pixel(pixels, x + offset + 1, y + offset + 1, (131, 81, 168))

    elif name == "lich":
        robe = (52, 39, 78)
        robe_shadow = (24, 19, 41)
        robe_glint = (93, 73, 125)
        body = b((18, 36, 16), robe, robe_shadow, robe_glint)
        head = b((12, 12, 12), (202, 197, 177), (115, 113, 106), (239, 230, 202))
        crown = b((16, 4, 16), (198, 149, 53), (104, 70, 26), (255, 222, 106))
        arms = [b((6, 26, 6), robe, robe_shadow, robe_glint) for _ in range(2)]
        staff = b((3, 40, 3), (91, 61, 43), (46, 34, 31), (179, 132, 74))
        orb = b((8, 8, 8), (56, 179, 219), (20, 72, 111), (181, 249, 255))
        x, y, w, h = Sheet.front(body)
        Sheet.rect(pixels, x + w // 2 - 1, y + 5, 2, h - 10, (101, 190, 218))
        x, y, w, h = Sheet.front(head)
        Sheet.rect(pixels, x + 2, y + 5, 2, 2, (65, 190, 224))
        Sheet.rect(pixels, x + 8, y + 5, 2, 2, (65, 190, 224))
        Sheet.rect(pixels, x + 5, y + 9, 3, 1, (88, 87, 82))
        x, y, w, h = Sheet.front(crown)
        Sheet.rect(pixels, x, y + 1, w, 2, (241, 197, 76))
        for px in (x + 3, x + 7, x + 12):
            Sheet.rect(pixels, px, y, 2, 2, (255, 231, 139))
        x, y, w, h = Sheet.front(orb)
        Sheet.rect(pixels, x + 2, y + 2, 4, 4, (143, 238, 255))
        Sheet.rect(pixels, x + 3, y + 3, 2, 2, (226, 255, 255))
        for box in (staff, *arms):
            x, y, w, h = Sheet.front(box)
            Sheet.rect(pixels, x + w // 2, y + 4, 1, max(1, h - 8), (126, 91, 54))

    elif name == "gauntlet":
        metal = (143, 145, 145)
        shadow = (65, 68, 72)
        glint = (205, 200, 176)
        wrist = b((14, 16, 14), metal, shadow, glint)
        palm = b((22, 22, 14), (116, 121, 126), shadow, glint)
        gem = b((8, 8, 4), (223, 147, 42), (105, 58, 24), (255, 219, 104))
        fingers = [b((5, 16, 10), metal, shadow, glint) for _ in range(4)]
        thumb = b((8, 14, 12), metal, shadow, glint)
        x, y, w, h = Sheet.front(palm)
        Sheet.rect(pixels, x + 2, y + 3, w - 4, 2, (179, 173, 151))
        Sheet.rect(pixels, x + 3, y + 8, 2, 8, (76, 80, 84))
        Sheet.rect(pixels, x + 8, y + 8, 2, 8, (76, 80, 84))
        Sheet.rect(pixels, x + 13, y + 8, 2, 8, (76, 80, 84))
        x, y, w, h = Sheet.front(gem)
        Sheet.rect(pixels, x + 2, y + 2, w - 4, h - 4, (255, 185, 56))
        Sheet.rect(pixels, x + 3, y + 3, w - 6, h - 6, (255, 238, 143))
        for box in (wrist, thumb, *fingers):
            x, y, w, h = Sheet.front(box)
            Sheet.rect(pixels, x + 1, y + h // 3, w - 2, 1, (190, 151, 70))

    else:
        stem = b((6, 30, 6), (50, 93, 63), (22, 45, 37), (111, 151, 79))
        bulb = b((20, 16, 20), (35, 78, 71), (14, 41, 43), (80, 136, 102))
        petals = [b((8, 16, 3), (169, 57, 136), (73, 29, 86), (244, 125, 196))
                  for _ in range(6)]
        eye = b((8, 8, 8), (140, 62, 188), (48, 27, 86), (232, 153, 255))
        x, y, w, h = Sheet.front(bulb)
        Sheet.rect(pixels, x + 5, y + 4, 3, 2, (146, 92, 161))
        Sheet.rect(pixels, x + 12, y + 4, 3, 2, (146, 92, 161))
        Sheet.rect(pixels, x + 8, y + 10, 4, 2, (190, 102, 171))
        x, y, w, h = Sheet.front(eye)
        Sheet.rect(pixels, x + 2, y + 2, 4, 4, (209, 113, 236))
        Sheet.rect(pixels, x + 3, y + 3, 2, 2, (255, 221, 255))
        for box in (stem, *petals):
            x, y, w, h = Sheet.front(box)
            Sheet.pixel(pixels, x + w // 2, y + h // 2, (255, 177, 225))

    return pixels


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, seed in (("obsidilith", 11), ("lich", 23), ("gauntlet", 37), ("void_blossom", 51)):
        path = os.path.join(OUT, name + ".png")
        write_png(path, layout(name, seed))
        print("wrote", os.path.relpath(path))


if __name__ == "__main__":
    main()
