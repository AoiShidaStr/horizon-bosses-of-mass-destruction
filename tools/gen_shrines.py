#!/usr/bin/env python3
"""Generate clean-room boss shrines, structure data and their offering chests."""

import gzip
import json
import os
import struct

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DATA = os.path.join(ROOT, "content", "data", "bosses_of_mass_destruction")

TAG_END, TAG_INT, TAG_LONG, TAG_STRING, TAG_LIST, TAG_COMPOUND, TAG_INT_ARRAY = 0, 3, 4, 8, 9, 10, 11


def _string(value):
    raw = value.encode("utf-8")
    return struct.pack(">H", len(raw)) + raw


def _tag(kind, payload):
    return kind, payload


def _int(value):
    return _tag(TAG_INT, struct.pack(">i", value))


def _long(value):
    return _tag(TAG_LONG, struct.pack(">q", value))


def _text(value):
    return _tag(TAG_STRING, _string(value))


def _int_array(values):
    return _tag(TAG_INT_ARRAY, struct.pack(">i", len(values)) + b"".join(struct.pack(">i", v) for v in values))


def _compound(values):
    body = b"".join(bytes((kind,)) + _string(key) + payload for key, (kind, payload) in values.items())
    return _tag(TAG_COMPOUND, body + bytes((TAG_END,)))


def _list(kind, values):
    if any(item[0] != kind for item in values):
        raise ValueError("NBT lists must have one element type")
    return _tag(TAG_LIST, bytes((kind,)) + struct.pack(">i", len(values)) + b"".join(payload for _, payload in values))


class Template:
    def __init__(self, size):
        self.size = size
        self.blocks = {}

    def put(self, x, y, z, block, **properties):
        if not (0 <= x < self.size[0] and 0 <= y < self.size[1] and 0 <= z < self.size[2]):
            raise ValueError(f"{block} at {(x, y, z)} lies outside {self.size}")
        self.blocks[(x, y, z)] = (block, tuple(sorted(properties.items())), None)

    def fill(self, low, high, block):
        for x in range(low[0], high[0] + 1):
            for y in range(low[1], high[1] + 1):
                for z in range(low[2], high[2] + 1):
                    self.put(x, y, z, block)

    def chest(self, x, y, z, loot_table, facing="north"):
        self.blocks[(x, y, z)] = (
            "chest",
            (("facing", facing), ("type", "single"), ("waterlogged", "false")),
            {"id": "minecraft:chest", "LootTable": loot_table, "LootTableSeed": _long(2413)},
        )

    def nbt(self):
        palette = []
        block_tags = []
        for pos, (block, properties, entity) in sorted(self.blocks.items()):
            state = (block, properties)
            if state not in palette:
                palette.append(state)
            entry = {
                "pos": _int_array(pos),
                "state": _int(palette.index(state)),
            }
            if entity:
                entry["nbt"] = _compound({
                    key: value if isinstance(value, tuple) else _text(value)
                    for key, value in entity.items()
                })
            block_tags.append(_compound(entry))

        palette_tags = []
        for block, properties in palette:
            state = {"id": _text("minecraft:" + block)}
            if properties:
                state["properties"] = _compound({key: _text(value) for key, value in properties})
            palette_tags.append(_compound(state))

        return _compound({
            "size": _int_array(self.size),
            "entities": _list(TAG_COMPOUND, []),
            "blocks": _list(TAG_COMPOUND, block_tags),
            "palette": _list(TAG_COMPOUND, palette_tags),
            "DataVersion": _int(5023),
        })


def lich_tower():
    g = Template((9, 12, 9))
    g.fill((1, 0, 1), (7, 0, 7), "stone_bricks")
    for x, z in ((1, 1), (7, 1), (1, 7), (7, 7)):
        for y in range(1, 10):
            g.put(x, y, z, "packed_ice" if y % 3 else "blue_ice")
    for x in range(2, 7):
        g.put(x, 1, 1, "stone_bricks")
        g.put(x, 1, 7, "stone_bricks")
    for z in range(2, 7):
        g.put(1, 1, z, "stone_bricks")
        g.put(7, 1, z, "stone_bricks")
    g.put(3, 2, 1, "stone_bricks")
    g.put(5, 2, 7, "stone_bricks")
    g.put(3, 3, 1, "soul_lantern", hanging="false")
    g.put(5, 3, 7, "soul_lantern", hanging="false")
    g.put(4, 0, 4, "crying_obsidian")
    g.chest(2, 1, 4, "bosses_of_mass_destruction:chests/lich_shrine", "east")
    g.put(4, 10, 4, "soul_sand")
    g.put(4, 11, 4, "soul_fire")
    return g


def obsidilith_arena():
    g = Template((15, 7, 15))
    for x in range(2, 13):
        for z in range(2, 13):
            if x in (2, 12) or z in (2, 12) or (x + z) % 5:
                g.put(x, 0, z, "end_stone_bricks" if (x + z) % 4 else "end_stone")
    for x, z in ((2, 2), (12, 2), (2, 12), (12, 12)):
        for y in range(1, 6):
            g.put(x, y, z, "obsidian" if y < 5 else "crying_obsidian")
    for x, z in ((7, 2), (7, 12), (2, 7), (12, 7)):
        g.put(x, 1, z, "purpur_pillar", axis="y")
    g.put(7, 0, 7, "crying_obsidian")
    g.chest(4, 1, 7, "bosses_of_mass_destruction:chests/obsidilith_shrine", "east")
    return g


def gauntlet_forge():
    g = Template((11, 8, 11))
    g.fill((1, 0, 1), (9, 0, 9), "polished_blackstone_bricks")
    for x in range(1, 10):
        for z in (1, 9):
            g.put(x, 1, z, "blackstone")
        for z in range(2, 9):
            if z in (1, 9):
                g.put(x, 2, z, "nether_bricks")
    for z in range(1, 10):
        for x in (1, 9):
            g.put(x, 1, z, "blackstone")
            g.put(x, 2, z, "nether_bricks")
    for x, z in ((2, 2), (8, 2), (2, 8), (8, 8)):
        for y in range(3, 7):
            g.put(x, y, z, "basalt", axis="y")
        g.put(x, 2, z, "magma_block")
    g.put(5, 0, 5, "crying_obsidian")
    g.put(3, 1, 5, "soul_lantern", hanging="false")
    g.put(7, 1, 5, "soul_lantern", hanging="false")
    g.chest(2, 1, 5, "bosses_of_mass_destruction:chests/gauntlet_shrine", "east")
    return g


def void_blossom_cave():
    g = Template((13, 7, 13))
    g.fill((2, 0, 2), (10, 0, 10), "moss_block")
    for x in range(2, 11):
        for z in range(2, 11):
            if x in (2, 10) or z in (2, 10) or (x + z) % 3 == 0:
                g.put(x, 6, z, "mossy_stone_bricks")
    for x, z in ((2, 2), (2, 10), (10, 2), (10, 10), (6, 2), (6, 10), (2, 6), (10, 6)):
        g.put(x, 1, z, "mossy_stone_bricks")
        if (x + z) % 3:
            g.put(x, 2, z, "mossy_stone_bricks")
    for x, z, block in (
        (3, 4, "flowering_azalea"), (9, 4, "azalea"),
        (4, 9, "moss_carpet"), (8, 3, "moss_carpet"),
        (6, 6, "crying_obsidian"),
    ):
        g.put(x, 1 if block != "crying_obsidian" else 0, z, block)
    g.put(6, 5, 6, "spore_blossom")
    g.chest(4, 1, 6, "bosses_of_mass_destruction:chests/void_blossom_shrine", "east")
    return g


SHRINES = {
    "lich": {
        "template": lich_tower,
        "biomes": ["minecraft:snowy_plains", "minecraft:snowy_taiga", "minecraft:ice_spikes",
                   "minecraft:grove", "minecraft:frozen_peaks", "minecraft:jagged_peaks"],
        "spacing": 32, "separation": 28, "salt": 6813041, "loot": "minecraft:wither_skeleton_skull",
        "step": "surface_structures", "height": {"absolute": 0}, "heightmap": "WORLD_SURFACE_WG",
    },
    "obsidilith": {
        "template": obsidilith_arena,
        "biomes": ["minecraft:end_highlands", "minecraft:end_midlands",
                   "minecraft:small_end_islands", "minecraft:end_barrens"],
        "spacing": 36, "separation": 32, "salt": 6813042, "loot": "minecraft:ender_eye",
        "step": "surface_structures", "height": {"absolute": 0}, "heightmap": "WORLD_SURFACE_WG",
    },
    "gauntlet": {
        "template": gauntlet_forge,
        "biomes": ["minecraft:nether_wastes", "minecraft:soul_sand_valley",
                   "minecraft:crimson_forest", "minecraft:warped_forest", "minecraft:basalt_deltas"],
        "spacing": 32, "separation": 28, "salt": 6813043, "loot": "minecraft:netherite_scrap",
        "step": "surface_structures", "height": {"absolute": 0}, "heightmap": "WORLD_SURFACE_WG",
    },
    "void_blossom": {
        "template": void_blossom_cave,
        "biomes": ["minecraft:dripstone_caves", "minecraft:lush_caves", "minecraft:deep_dark"],
        "spacing": 40, "separation": 36, "salt": 6813044, "loot": "minecraft:spore_blossom",
        "step": "underground_structures", "height": {"absolute": -42},
    },
}


def write_json(path, value):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(value, f, indent=2)
        f.write("\n")


def write_chest_loot(name, offering):
    write_json(os.path.join(DATA, "loot_table", "chests", name + "_shrine.json"), {
        "type": "minecraft:chest",
        "pools": [
            {"rolls": 1, "entries": [{"type": "minecraft:item", "name": offering}]},
            {"rolls": 2, "entries": [
                {"type": "minecraft:item", "name": "minecraft:gold_ingot", "weight": 2},
                {"type": "minecraft:item", "name": "minecraft:glowstone_dust", "weight": 2},
                {"type": "minecraft:item", "name": "minecraft:diamond", "weight": 1},
            ]},
        ],
    })


def generate():
    for name, spec in SHRINES.items():
        root = os.path.join(DATA, "worldgen")
        template_path = os.path.join(DATA, "structure", name + "_shrine", name + "_shrine.nbt")
        os.makedirs(os.path.dirname(template_path), exist_ok=True)
        tag, payload = spec["template"]().nbt()
        with gzip.GzipFile(template_path, "wb", mtime=0) as f:
            f.write(bytes((tag,)) + _string("") + payload)

        write_json(os.path.join(root, "template_pool", name + "_shrine.json"), {
            "fallback": "minecraft:empty",
            "elements": [{
                "weight": 1,
                "element": {
                    "element_type": "minecraft:single_pool_element",
                    "location": f"bosses_of_mass_destruction:{name}_shrine/{name}_shrine",
                    "projection": "rigid",
                },
            }],
        })
        structure = {
            "type": "minecraft:jigsaw",
            "biomes": f"#bosses_of_mass_destruction:has_{name}_shrine",
            "step": spec["step"],
            "spawn_overrides": {},
            "terrain_adaptation": "beard_thin",
            "start_pool": f"bosses_of_mass_destruction:{name}_shrine",
            "size": 1,
            "start_height": spec["height"],
            "max_distance_from_center": 80,
        }
        if "heightmap" in spec:
            structure["project_start_to_heightmap"] = spec["heightmap"]
        write_json(os.path.join(root, "structure", name + "_shrine.json"), structure)
        write_json(os.path.join(root, "structure_set", name + "_shrine.json"), {
            "structures": [{"structure": f"bosses_of_mass_destruction:{name}_shrine", "weight": 1}],
            "placement": {
                "type": "minecraft:random_spread",
                "spacing": spec["spacing"],
                "separation": spec["separation"],
                "salt": spec["salt"],
            },
        })
        write_json(os.path.join(DATA, "tags", "worldgen", "biome", f"has_{name}_shrine.json"),
                   {"values": spec["biomes"]})
        write_chest_loot(name, spec["loot"])
        print(f"generated {name}_shrine")


if __name__ == "__main__":
    generate()
