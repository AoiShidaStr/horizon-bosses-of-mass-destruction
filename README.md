# Bosses of Mass Destruction for Horizon

A Horizon port of [Bosses of Mass Destruction](https://github.com/barribob/bosses-of-mass-destruction)
by Barribob and miyo6032 (LGPL-3.0-only). It adds four end-game bosses with
fixed, learnable attack cycles: **Obsidilith**, **Lich**, **Gauntlet** and
**Void Blossom**.

It replaces Mowzie's Mobs and Born in Chaos for Saison 2's end-game bosses:
those two are All-Rights-Reserved and cannot be ported, while BoMD is LGPL.

- Content mod `.hmod` (`side = "both"`): four living entity types (zombie
  template, own attributes), four loot tables, names and placeholder textures.
- Native trait mod `.hnmod`: attack state machine, telegraphs, boss bars,
  shield and enrage phases, damage/effects, and the client-side box models.

This is a clean-room reimplementation of the bosses' *behaviour*, not a
translation of the original Kotlin code or GeckoLib assets. See `NOTICE.md`.

## Install (dedicated server + the 5-6 clients)

Both halves use the id `bosses_of_mass_destruction`. Every side gets the
**same** `.hmod`: it is a `side = "both"` content mod, so a client with a
different version is refused.

**Dedicated server** (the operator runs these; `--trust-native-code` is the
operator's own decision and is never passed by an agent):

1. Unpack `bosses_of_mass_destruction.hnmod` into a folder (or use
   `mixins/bosses_of_mass_destruction/` from this folder) and build a server
   engine with it, together with the server's other native mods:
   `horizon-launcher native-build <mods folder> --server-output <new dir> --version <tag> --trust-native-code`
   (docs/native-mods.md, "Standalone server export"). The bundle pins
   Horizon `a85af177`.
2. `horizon-server --dir <server> config set engine.path=<new dir>/horizon-engine`.
3. Put `bosses_of_mass_destruction.hmod` in `<server>/horizon-mods/` and add
   `mod.bosses_of_mass_destruction:true` to `<server>/horizon-mods.txt`.
4. `horizon-server --dir <server> restart`.

**Each client**:

1. Put `bosses_of_mass_destruction.hmod` in `horizon-mods/` and
   `bosses_of_mass_destruction.hnmod` in `horizon-native-mods/` of the
   instance (or install both from the launcher's Mods page), and turn the mod
   on in Options > Mods.
2. Build the native half into the game from the launcher's Mods page
   ("Native build" > Build), so the box models and attack poses are drawn.
   Without the native half a client should still join (the entity types come
   from the `.hmod`) and draw the bosses with the zombie template's model;
   that case is untested.

## Starting a fight

Without commands (survival players): build an **altar**, a block of
**crying obsidian** with two free blocks above it, and right-click its top
with the boss's **offering**. One offering is used up (not in creative) and
the boss wakes over the altar; everyone on the server is told who woke it.

| Boss | Offering |
|------|----------|
| Obsidilith | Eye of Ender |
| Lich | Wither Skeleton Skull |
| Gauntlet | Netherite Scrap (the Obsidilith drops some) |
| Void Blossom | Spore Blossom |

The altar refuses (and keeps the offering) while a boss of the same kind is
awake within 48 blocks, when the two blocks above it are not empty, or when
the offering is renamed or otherwise not a plain stack. Using these items on
crying obsidian therefore never throws the eye or places the skull/blossom.
The altar is this port's own mechanic, not the original mod's.

Operators can still summon directly:

```
/summon bosses_of_mass_destruction:obsidilith
/summon bosses_of_mass_destruction:lich
/summon bosses_of_mass_destruction:gauntlet
/summon bosses_of_mass_destruction:void_blossom
```

Set `BOMD_TRACE=1` in the server's environment to log each telegraph,
landing, shield, enrage, altar call and death to stderr (off by default).

## Status

### Done and verified on a dedicated server

Smoke test (`tools/vps/smoke.sh`): a scratch dedicated server running a
`--no-default-features` engine built with this mod (Horizon `a85af177`, dev
profile, launcher-equivalent `horizon-native-game` package from
`tools/vps/stage.sh`), `HORIZON_NO_AUDIO=1`, one survival bot from
`horizon-bots` standing next to each boss, commands over the console. For
**each of the four bosses** it showed:

- `/summon` works; the content mod's entity type and loot table load.
- The whole four-attack cycle runs in order and loops (trace lines
  `Telegraphed(...)` / `Landed(...)`), and the bot takes damage
  (e.g. 1000 -> 956.5 hp over one Obsidilith cycle).
- At 50% health the shield opens: `/damage` is refused ("Target is
  invulnerable"), then it drops; the enrage triggers at its threshold.
- Burning is put out (`Fire` set to 200 reads 0 a second later; no daylight
  damage).
- Lich: hovers 3 blocks up (`NoGravity`), teleports, raises skeletons (capped).
- Gauntlet: dashes onto the player. Void Blossom: heals (100 -> 116 hp).
- Death: the death hook fires, the boss bar is cleared, "<Boss> falls." is
  broadcast, and the loot table drops (3 stacks; the Gauntlet's landed in the
  bot's inventory because it dies on top of it).

Altar smoke test (`tools/vps/smoke_altar.sh`, same engine build): for each
boss a survival bot right-clicks a crying-obsidian altar holding two
offerings. The boss wakes once over the altar and runs its cycle, the bot is
left with one offering, a second bot's click while the boss is awake is
refused and keeps both offerings, and an Eye of Ender used on plain stone
wakes nothing.

Fixed by the smoke test:

- Horizon's death hook names a modded mob `minecraft:bosses_of_mass_destruction:<boss>`
  (`game::native_hooks::mob_kind` puts `minecraft:` before every id; still so
  at `f23176af`): `Boss::from_kind` reads that spelling too.
- A dead boss ticks through its death animation: it no longer starts a new
  cycle, shield or bar then.

Loot:

- Each boss drops its own **named trophy** next to the old stacks (`custom_name`
  and `lore` through the loot table's `set_name` / `set_lore` functions, both
  implemented by Horizon's loot engine): Obsidilith's Heart (crying obsidian),
  Lich's Crown (skeleton skull), Gauntlet's Fist (anvil) and Void Bloom
  (amethyst cluster). The trophies are plain vanilla items with a name, so they
  are displayable and never break a recipe.
- Client names are translated in `lang/fr_fr.json` (Liche, Gantelet, Floraison
  du Vide); the English names stay in `lang/en_us.json`.

Code:

- Server half (`api/src/server.rs`): cycles with telegraph (particles, sound,
  animation message), active window and recovery (`api/src/logic.rs`);
  Obsidilith burst ring / fire wave / dripstone spike / anvil drop; Lich
  volley / comet / skeleton summon (max four) / teleport; Gauntlet dash punch /
  spinning AoE / line-of-sight laser / blindness; Void Blossom root spikes /
  poison spore ring / blade sweep / healing blossom. Boss bar per player within
  64 blocks (red once enraged), cleared on death, unload and walking away.
  Zombie-template pitfalls handled: goals cleared, movement speed 0,
  burning/drowning/fall damage refused, drowned conversion refused,
  reinforcements 0, persistent, never a baby.
- Client half (`client.rs`, `model.rs`): four code-built `LayerDefinition`
  models, idle motion, a pose per attack animation, hurt/death tint. Compiles
  in the default-features (client) build; **not looked at on a real client**
  (the VPS has no GPU).
- One mod channel (`bosses_of_mass_destruction:anim`, 8-byte message).
- 19 unit tests (`cargo test -p mod-bosses_of_mass_destruction`): cycle logic,
  message codec, a whole fight per boss through a test `Host`, and the altar
  (wakes once, uses one offering, refuses without room/offering, ignores
  other items and blocks).
- `cargo horizon-mod check` passes for both files.

### Not done (known gaps)

- Client visuals (models, poses, particles, boss bar look) are unverified:
  a human needs to look on a real client.- No original structures (Lich tower, Obsidilith arena, Gauntlet cavern, Void
  Blossom cave), no kill-count spawning, no eternal night: bosses come from the
  altar or from `/summon`. **This is the next gap to close** (boss shrines that
  generate and hold the altar): the jigsaw + `.nbt` path exists in the engine
  (`horizon-ref` structures, as the Graveyard port uses), but an NBT template
  written by hand could not be worldgen-tested in this session.
- The trophies are renamed vanilla items, not new registered items with their
  own textures and uses.
- No spawn eggs or custom summon items: Horizon's spawn eggs spawn the mob
  named by the egg item itself, so a data-only egg for a modded mob is not
  possible; the altar (vanilla items on crying obsidian) replaces it.
- No GeckoLib model/animation parity: original box geometry, placeholder
  textures. No multipart hitboxes. No original utility blocks or items.
- Timings, damage and particles are this port's own design.
- Bosses from the current NeoForge world are not carried over.
- The smoke test used a dev-profile engine (slower ticks than a release
  build); a release-profile export has not been run.

## Build

Light content work (local):

```
python tools/pack.py        # target/bosses_of_mass_destruction.hmod/.hnmod
cargo horizon-mod check target/bosses_of_mass_destruction.hmod
cargo horizon-mod check target/bosses_of_mass_destruction.hnmod
```

Native crate, on the build VPS (worktree `/opt/build/wt/bosses_of_mass_destruction`
at `a85af177`, shared `CARGO_TARGET_DIR=/opt/build/target`):

```
tools/vps/stage.sh   # native-api/<id> + crates/native-game, as the launcher stages them
tools/vps/build.sh   # tests, server engine (--no-default-features), client check, horizon-bots
tools/vps/smoke.sh   # the dedicated-server smoke test (needs prepared resources)
tools/vps/smoke_altar.sh  # the altar, with bots right-clicking it
```

`tools/check.sh` type-checks the crate against a local `horizon-ref`.

Pins: Horizon `a85af177a82504f357ff10d5d40d0b7230e2ac32`, native hooks API
`415`, game `26.3`. It stays on `a85af177` (not `f23176af`) because the other
Saison 2 native mods and the build VPS are on it, and one server engine builds
them all at one revision; the mod API it uses is unchanged between the two.

## License

LGPL-3.0-only (`LICENSE`). The GPL v3 text the LGPL builds on is in
`LICENSE-GPL-3.0.txt`. Credits and the record of what this port contains are
in `NOTICE.md`.
