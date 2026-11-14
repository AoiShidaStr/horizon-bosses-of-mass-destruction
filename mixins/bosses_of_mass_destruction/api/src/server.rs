//! The server half: each boss runs its attack cycle (`logic.rs`), tells the
//! clients around it what to animate (`net.rs`), and turns each attack into
//! damage, falling blocks and particles.
//!
//! State lives in this mod instead of a server attachment on purpose: a boss is
//! one mob with one cycle, and keeping it here means a cycle restarts cleanly
//! when a boss is despawned or a world is reopened (see `world_lifecycle`).
use crate::logic::{self, Attack, Boss, Event};
use crate::net::{Message, Phase};
use horizon_client::mod_api::{
    Compound, EntityTickListenedEvent, EntityTickPostEvent, Flow, Host, LivingConversionPreEvent,
    LivingDeathEvent, LivingIncomingDamageEvent, NativeBossBar, NativeEffect, NativeFalling,
    NativeServerPlayer, ParticleOptions, ParticleType, RegisterGoalsEvent, ServerTickEvent,
    ServerTickPhase, Tag, Text, UseItemOnEvent,
};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

/// Server ticks counted by `server_tick`.
static SERVER_TICKS: AtomicU64 = AtomicU64::new(0);

/// Whether the operator asked for a fight trace (`BOMD_TRACE=1` in the
/// server's environment): one stderr line per telegraph, landing, shield,
/// enrage and death, for checking a fight on a headless server.
fn tracing() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("BOMD_TRACE").is_some_and(|v| v != "0"))
}

fn trace(line: std::fmt::Arguments<'_>) {
    if tracing() {
        eprintln!("[bosses_of_mass_destruction] {line}");
    }
}

/// What the boss logic reads of a player: a copy of the few fields it needs.
/// `NativeServerPlayer` is `#[non_exhaustive]`, so keeping the logic on this
/// type also lets the tests below drive a whole fight with players in it.
#[derive(Clone, Debug)]
pub(crate) struct Target {
    pub uuid: u128,
    pub dimension: String,
    pub position: [f64; 3],
    /// The network entity id, 0 when unknown.
    pub entity_id: i32,
    pub health: f32,
    /// 0 survival, 1 creative, 2 adventure, 3 spectator.
    pub game_mode: i32,
}

impl From<&NativeServerPlayer> for Target {
    fn from(player: &NativeServerPlayer) -> Self {
        Self {
            uuid: player.uuid,
            dimension: player.dimension.clone(),
            position: player.position,
            entity_id: player.entity_id,
            health: player.health,
            game_mode: player.game_mode,
        }
    }
}

/// One boss's live state.
struct Runtime {
    state: logic::State,
    /// Where it was first seen: the Lich hovers there, and Teleport measures
    /// from it.
    anchor: [f64; 3],
    /// Where the volley's or the comet's marks landed last time.
    marks: Vec<[f64; 3]>,
    /// The ticks of the pillar/anbaric shield left (none now: see README).
    shield_ticks: u32,
    /// Whether it has already taken its one shield window.
    shielded_once: bool,
    /// The players its boss bar is shown to.
    viewers: HashSet<u128>,
    /// The bar last sent (progress in thousandths, colour): a bar is sent
    /// again only when it changed or to a player who just came near.
    bar_sent: Option<(u32, u8)>,
    /// The server tick (`SERVER_TICKS`) it last ticked on: a boss whose chunk
    /// unloaded stops ticking, and `server_tick` then drops its bar.
    last_seen: u64,
}

impl Runtime {
    fn new(boss: Boss, at: [f64; 3]) -> Self {
        Self {
            state: logic::State::new(boss),
            // The Lich hovers three blocks over where it was summoned.
            anchor: if boss == Boss::Lich {
                [at[0], at[1] + 3.0, at[2]]
            } else {
                at
            },
            marks: Vec::new(),
            shield_ticks: 0,
            shielded_once: false,
            viewers: HashSet::new(),
            bar_sent: None,
            last_seen: 0,
        }
    }
}

fn bosses() -> &'static Mutex<HashMap<(String, u32), Runtime>> {
    static BOSSES: OnceLock<Mutex<HashMap<(String, u32), Runtime>>> = OnceLock::new();
    BOSSES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Bosses that cannot be hurt right now, by network entity id: the damage hook
/// gets no host, so this is the only thing it can read.
fn shielded() -> &'static Mutex<HashSet<i32>> {
    static SHIELDED: OnceLock<Mutex<HashSet<i32>>> = OnceLock::new();
    SHIELDED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// The boss bar id of a boss, from its entity id: stable while it lives and
/// clear of the world's own bars.
fn bar_id(entity: u32) -> u128 {
    0xB0_55_0000_0000_0000_0000_0000_0000_0000u128 | u128::from(entity)
}

/// What one tick decided, applied after the state lock is dropped.
struct Plan {
    events: Vec<Event>,
    phase: logic::Phase,
    marks: Vec<[f64; 3]>,
    step_ticks: u32,
    step_length: u32,
    shield: bool,
    enraged: bool,
    health: f32,
}

/// A boss ticked by the game.
pub fn entity_tick_post(host: &mut dyn Host, event: &mut EntityTickPostEvent<'_>) {
    let Some(boss) = Boss::from_kind(event.kind) else {
        return;
    };
    let players: Vec<Target> = host.server_players().iter().map(Target::from).collect();
    tick_boss(host, event.dimension, event.entity, boss, &players);
}

/// The game only tells entity ticks when a mod asks for them.
pub fn entity_tick_listened(_host: &mut dyn Host, event: &mut EntityTickListenedEvent) {
    event.listened = true;
}

/// Our bosses have no goals: their whole behaviour is the cycle above. A
/// zombie's strolls, attacks and targeting are cleared, and it keeps still.
pub fn register_goals(event: &mut RegisterGoalsEvent<'_>) {
    let Some(boss) = Boss::from_kind(&event.mob.kind.id) else {
        return;
    };
    let _ = boss;
    // Its movement speed attribute is 0 (content/data/.../entity_type), so with
    // no goals it stays where it was summoned.
    event.mob.ai.goals.clear();
    event.mob.ai.targets.clear();
}

/// One server tick of one boss. `online` is every player on the server.
pub(crate) fn tick_boss(
    host: &mut dyn Host,
    dimension: &str,
    entity: u32,
    boss: Boss,
    online: &[Target],
) {
    let Some(handle) = host.entity(entity) else {
        // It died or left: `server_tick` drops its state and its bar once it
        // has stopped ticking.
        return;
    };
    let Some(pos) = handle.position(host) else {
        return;
    };
    let health = health_of(host, handle).unwrap_or_else(|| boss.max_health());
    // A dead boss still ticks through its death animation (about 20 ticks)
    // after `living_death` forgot it: it must not start a new cycle, raise
    // its shield or send its bar again (seen on the dedicated-server test).
    if health <= 0.0 {
        return;
    }
    let players = clients_near(online, dimension, pos, 64.0);

    // A zombie template hurt on Hard calls reinforcements of its own kind
    // (`Zombie.hurtServer`), which would clone the boss, and finalizeSpawn
    // rolls it a random chance. The first time a boss ticks (also after a
    // restart), that chance is set to 0.
    let first = bosses()
        .lock()
        .map(|map| !map.contains_key(&(dimension.to_owned(), entity)))
        .unwrap_or(false);
    if first {
        let _ = host.entity_set_attribute_base(handle, "minecraft:spawn_reinforcements", 0.0);
        // A zombie template despawns far from players, may be rolled a baby by
        // finalizeSpawn and picks up loot: a boss never does any of these.
        let mut nbt = Compound::new();
        nbt.insert("PersistenceRequired".into(), Tag::Byte(1));
        nbt.insert("IsBaby".into(), Tag::Byte(0));
        nbt.insert("CanPickUpLoot".into(), Tag::Byte(0));
        nbt.insert("CanBreakDoors".into(), Tag::Byte(0));
        // The Lich flies: no gravity, held at its hover height below. Both are
        // needed because the entity type copies a zombie's physics.
        if boss == Boss::Lich {
            nbt.insert("NoGravity".into(), Tag::Byte(1));
        }
        let merged = host.entity_merge_nbt(handle, nbt);
        trace(format_args!(
            "{boss:?} #{entity} first tick in {dimension} at {pos:?}, hp={health:.1}, nbt merged={merged}"
        ));
    }
    // A zombie template catches fire in daylight: the damage is refused in
    // `living_incoming_damage`, and the flames are put out here.
    if host.entity_fire_ticks(handle).unwrap_or(0) > 0 {
        let _ = host.entity_set_fire(handle, 0);
    }
    if boss == Boss::Lich {
        let anchor = bosses()
            .lock()
            .ok()
            .and_then(|map| map.get(&(dimension.to_owned(), entity)).map(|r| r.anchor))
            .unwrap_or(pos);
        if (pos[1] - anchor[1]).abs() > 0.01 {
            let _ = host.teleport_mob(dimension, entity, [pos[0], anchor[1], pos[2]]);
        }
    }

    let plan = {
        let Ok(mut map) = bosses().lock() else {
            return;
        };
        let runtime = map
            .entry((dimension.to_owned(), entity))
            .or_insert_with(|| Runtime::new(boss, pos));
        runtime.last_seen = SERVER_TICKS.load(Ordering::Relaxed);
        let mut events = Vec::new();
        runtime.state.tick(&mut events);
        if !runtime.state.enraged && runtime.state.should_enrage(health, boss.max_health()) {
            runtime.state.enraged = true;
            trace(format_args!("{boss:?} #{entity} enraged at {health:.1} hp"));
        }
        if !runtime.shielded_once && health <= boss.max_health() * 0.5 {
            runtime.shielded_once = true;
            runtime.shield_ticks = 100;
            trace(format_args!("{boss:?} #{entity} shield up at {health:.1} hp"));
        }
        if runtime.shield_ticks > 0 {
            runtime.shield_ticks -= 1;
            if runtime.shield_ticks == 0 {
                trace(format_args!("{boss:?} #{entity} shield down"));
            }
        }
        for event in &events {
            if matches!(event, Event::Telegraphed(_) | Event::Landed(_)) {
                trace(format_args!(
                    "{boss:?} #{entity} {event:?} hp={health:.1} players_near={}",
                    players.len()
                ));
            }
        }
        // The wind-up marks where the volley will fall, so a player can step
        // out of it: that is the tell.
        if events
            .iter()
            .any(|e| matches!(e, Event::Telegraphed(Attack::Volley | Attack::Comet)))
        {
            runtime.marks = players
                .iter()
                .filter(|p| p.dimension == dimension && targetable(p))
                .take(3)
                .map(|p| p.position)
                .collect();
        }
        Plan {
            events,
            phase: runtime.state.phase,
            marks: runtime.marks.clone(),
            step_ticks: runtime.state.ticks,
            step_length: runtime.state.phase_length(),
            shield: runtime.shield_ticks > 0,
            enraged: runtime.state.enraged,
            health,
        }
    };

    set_shielded(entity as i32, plan.shield);
    if plan.shield && plan.step_ticks % 5 == 0 {
        // The shield's tell: a ring of end rods, visible from a distance.
        ring(
            host,
            &ParticleOptions::simple(ParticleType::EndRod),
            pos,
            1.5,
            20,
        );
    }

    // The bar: one per boss, on every player within 64 blocks (red once
    // enraged, purple before). It is sent when it changes and to players who
    // just came near; players who walked away lose it.
    let progress = (plan.health / boss.max_health()).clamp(0.0, 1.0);
    let color: u8 = if plan.enraged { 2 } else { 5 };
    let key = ((progress * 1000.0).round() as u32, color);
    let near: HashSet<u128> = players.iter().map(|p| p.uuid).collect();
    let (gone, fresh, changed): (Vec<u128>, HashSet<u128>, bool) = bosses()
        .lock()
        .ok()
        .and_then(|mut map| {
            map.get_mut(&(dimension.to_owned(), entity)).map(|runtime| {
                let gone = runtime.viewers.difference(&near).copied().collect();
                let fresh = near.difference(&runtime.viewers).copied().collect();
                let changed = runtime.bar_sent != Some(key);
                runtime.viewers = near.clone();
                runtime.bar_sent = Some(key);
                (gone, fresh, changed)
            })
        })
        .unwrap_or_default();
    if changed || !fresh.is_empty() {
        let bar = NativeBossBar {
            name: Text::translate(boss.translation_key(), &[]),
            progress,
            color: color.into(),
            overlay: 0,
            darken: true,
            music: false,
            fog: false,
        };
        for player in &players {
            if changed || fresh.contains(&player.uuid) {
                let _ = host.boss_bar(player.uuid, bar_id(entity), Some(&bar));
            }
        }
    }
    for uuid in gone {
        let _ = host.boss_bar(uuid, bar_id(entity), None);
    }

    for event in &plan.events {
        match *event {
            Event::Telegraphed(attack) => {
                tell_attack(host, dimension, entity, boss, &players, pos, attack, plan.step_length);
            }
            Event::Landed(attack) => {
                land(host, dimension, entity, boss, &players, pos, attack, &plan.marks);
                // The active window: clients stop the wind-up pose and show the hit.
                let _ = crate::net::CHANNEL;
                for player in &players {
                    send_anim(host, player.uuid, entity, attack, Phase::Landed, 10);
                }
            }
            Event::Ended(attack) => {
                for player in &players {
                    send_anim(host, player.uuid, entity, attack, Phase::Ended, 0);
                }
            }
            Event::Next(_) => {}
        }
    }

    // Attacks that keep working through their active window.
    if plan.phase == logic::Phase::Active {
        tick_active(host, dimension, entity, boss, &players, pos, plan.step_ticks);
    }
}

/// The tell: a sound, particles where it will land, and the animation.
#[allow(clippy::too_many_arguments)]
fn tell_attack(
    host: &mut dyn Host,
    dimension: &str,
    entity: u32,
    boss: Boss,
    players: &[Target],
    pos: [f64; 3],
    attack: Attack,
    length: u32,
) {
    let flame = ParticleOptions::simple(ParticleType::Flame);
    match attack {
        Attack::Burst | Attack::Spike | Attack::RootSpikes | Attack::BladeSweep => {
            ring(host, &flame, pos, 5.0, 24);
            let _ = host.play_sound("minecraft:block.fire.ambient", pos, 4, 1.4, 0.8);
        }
        Attack::Wave => {
            let _ = host.play_sound("minecraft:block.lava.pop", pos, 4, 1.2, 0.9);
        }
        Attack::Anvil | Attack::Comet => {
            let smoke = ParticleOptions::simple(ParticleType::LargeSmoke);
            for player in players.iter().filter(|p| p.dimension == dimension && targetable(p)) {
                let at = [player.position[0], player.position[1] + 3.0, player.position[2]];
                let _ = host.send_particles(&smoke, None, at, [0.4, 0.4, 0.4], 0.01, 12);
            }
            let _ = host.play_sound("minecraft:entity.generic.explode", pos, 5, 0.8, 0.7);
        }
        Attack::Volley => {
            for mark in players.iter().filter(|p| p.dimension == dimension && targetable(p)).take(3) {
                let _ = host.send_particles(
                    &flame,
                    None,
                    [mark.position[0], mark.position[1] + 0.2, mark.position[2]],
                    [0.6, 0.1, 0.6],
                    0.02,
                    16,
                );
            }
            let _ = host.play_sound("minecraft:entity.blaze.shoot", pos, 5, 1.0, 0.6);
        }
        Attack::Minions => {
            let _ = host.play_sound("minecraft:entity.skeleton.ambient", pos, 5, 1.0, 0.7);
        }
        Attack::Teleport => {
            let portal = ParticleOptions::simple(ParticleType::Portal);
            let _ = host.send_particles(&portal, None, pos, [0.5, 1.0, 0.5], 0.1, 40);
            let _ = host.play_sound("minecraft:entity.enderman.teleport", pos, 5, 1.0, 1.0);
        }
        Attack::Punch | Attack::Swirl => {
            let _ = host.play_sound("minecraft:entity.zombie.attack_iron_door", pos, 5, 1.0, 0.6);
        }
        Attack::Laser => {
            let _ = host.play_sound("minecraft:entity.warden.sonic_boom", pos, 5, 0.7, 1.4);
        }
        Attack::Blindness => {
            let _ = host.play_sound("minecraft:entity.witch.ambient", pos, 5, 1.0, 0.8);
        }
        Attack::SporeRing | Attack::BlossomHeal => {
            let spore = ParticleOptions::simple(ParticleType::SporeBlossomAir);
            let _ = host.send_particles(&spore, None, pos, [2.0, 1.5, 2.0], 0.02, 40);
            let _ = host.play_sound("minecraft:block.spore_blossom.hurt", pos, 4, 1.0, 0.9);
        }
    }
    for player in players {
        send_anim(host, player.uuid, entity, attack, Phase::Telegraph, length);
    }
    let _ = boss;
}

/// The attack lands.
#[allow(clippy::too_many_arguments)]
fn land(
    host: &mut dyn Host,
    dimension: &str,
    entity: u32,
    boss: Boss,
    players: &[Target],
    pos: [f64; 3],
    attack: Attack,
    marks: &[[f64; 3]],
) {
    match attack {
        Attack::Burst => {
            ring(host, &ParticleOptions::simple(ParticleType::Lava), pos, 5.0, 40);
            hurt_near(host, dimension, players, pos, 6.0, 10.0, entity);
        }
        Attack::Wave => {
            // A line away from the boss, towards the nearest player.
            if let Some(target) = nearest(dimension, players, pos) {
                let steps = 12;
                for step in 0..=steps {
                    let t = step as f64 / steps as f64;
                    let at = [
                        pos[0] + (target.position[0] - pos[0]) * t,
                        pos[1] + 0.2,
                        pos[2] + (target.position[2] - pos[2]) * t,
                    ];
                    let _ = host.send_particles(
                        &ParticleOptions::simple(ParticleType::Flame),
                        None,
                        at,
                        [0.8, 0.4, 0.8],
                        0.02,
                        8,
                    );
                }
                hurt_near(host, dimension, players, target.position, 2.5, 8.0, entity);
            }
        }
        Attack::Spike | Attack::RootSpikes => {
            for player in players.iter().filter(|p| p.dimension == dimension && targetable(p)) {
                if distance(player.position, pos) > 14.0 {
                    continue;
                }
                if let Some(state) = host.default_state("minecraft:pointed_dripstone") {
                    let _ = host.spawn_falling_block(
                        dimension,
                        state,
                        [player.position[0], player.position[1] + 4.0, player.position[2]],
                        [0.0, -1.0, 0.0],
                        NativeFalling { drop_item: false, hurts: true },
                    );
                }
                hurt_near(host, dimension, players, player.position, 2.0, 7.0, entity);
            }
        }
        Attack::Anvil => {
            for player in players.iter().filter(|p| p.dimension == dimension && targetable(p)) {
                if distance(player.position, pos) > 20.0 {
                    continue;
                }
                if let Some(state) = host.default_state("minecraft:anvil") {
                    let _ = host.spawn_falling_block(
                        dimension,
                        state,
                        [player.position[0], player.position[1] + 8.0, player.position[2]],
                        [0.0, -0.8, 0.0],
                        NativeFalling { drop_item: false, hurts: true },
                    );
                }
            }
        }
        Attack::Volley => {
            for mark in marks {
                let _ = host.send_particles(
                    &ParticleOptions::simple(ParticleType::LargeSmoke),
                    None,
                    *mark,
                    [0.8, 0.4, 0.8],
                    0.05,
                    24,
                );
                hurt_near(host, dimension, players, *mark, 2.5, 9.0, entity);
            }
        }
        Attack::Comet => {
            for mark in marks.iter().take(1) {
                let _ = host.send_particles(
                    &ParticleOptions::simple(ParticleType::Explosion),
                    None,
                    *mark,
                    [1.5, 1.0, 1.5],
                    0.1,
                    40,
                );
                hurt_near(host, dimension, players, *mark, 4.0, 16.0, entity);
                let _ = host.play_sound("minecraft:entity.generic.explode", *mark, 5, 1.6, 0.8);
            }
        }
        Attack::Minions => {
            // Two skeletons at a time, and never more than four around it.
            let around = host.entities_in(
                dimension,
                [pos[0] - 16.0, pos[1] - 8.0, pos[2] - 16.0],
                [pos[0] + 16.0, pos[1] + 8.0, pos[2] + 16.0],
            );
            let skeletons = around
                .iter()
                .filter(|id| {
                    host.entity(**id)
                        .and_then(|e| e.kind(host))
                        .is_some_and(|kind| kind == "minecraft:skeleton")
                })
                .count();
            // The Lich hovers three blocks up: its skeletons stand on the ground.
            let ground = if boss == Boss::Lich { pos[1] - 3.0 } else { pos[1] };
            for i in 0..2 {
                if skeletons + i >= 4 {
                    break;
                }
                let at = [pos[0] + 1.5 * (i as f64 + 1.0), ground, pos[2] + 1.5];
                let _ = host.summon(dimension, "minecraft:skeleton", at, Compound::new());
            }
        }
        Attack::Teleport => {
            // Six blocks from where it was summoned, on the far side from the
            // nearest player: it stays over its arena instead of drifting off.
            let anchor = bosses()
                .lock()
                .ok()
                .and_then(|map| map.get(&(dimension.to_owned(), entity)).map(|r| r.anchor))
                .unwrap_or(pos);
            if let Some(target) = nearest(dimension, players, pos) {
                let (dx, dz) = (
                    anchor[0] - target.position[0],
                    anchor[2] - target.position[2],
                );
                let length = (dx * dx + dz * dz).sqrt();
                let (ux, uz) = if length < 0.001 {
                    (1.0, 0.0)
                } else {
                    (dx / length, dz / length)
                };
                let to = [anchor[0] + ux * 6.0, pos[1], anchor[2] + uz * 6.0];
                let _ = host.teleport_mob(dimension, entity, to);
                let _ = host.send_particles(
                    &ParticleOptions::simple(ParticleType::Portal),
                    None,
                    to,
                    [0.5, 1.0, 0.5],
                    0.1,
                    40,
                );
            }
        }
        Attack::Punch => {
            if let Some(target) = nearest(dimension, players, pos) {
                // A short dash, then the fist's own attack.
                let (dx, dz) = (
                    target.position[0] - pos[0],
                    target.position[2] - pos[2],
                );
                let length = (dx * dx + dz * dz).sqrt().max(0.001);
                let dash = length.min(3.0);
                let to = [
                    pos[0] + dx / length * dash,
                    pos[1],
                    pos[2] + dz / length * dash,
                ];
                let _ = host.teleport_mob(dimension, entity, to);
                if target.entity_id > 0 {
                    let _ = host.mob_attack(dimension, entity, target.entity_id as u32);
                }
            }
        }
        Attack::Swirl => {
            ring(host, &ParticleOptions::simple(ParticleType::Crit), pos, 6.0, 48);
            hurt_near(host, dimension, players, pos, 6.5, 7.0, entity);
        }
        Attack::Laser => {
            // The beam is drawn each tick while it is active (`tick_active`).
            let _ = host.play_sound("minecraft:block.beacon.activate", pos, 4, 1.0, 1.6);
        }
        Attack::Blindness => {
            for player in players.iter().filter(|p| p.dimension == dimension && targetable(p)) {
                if distance(player.position, pos) > 12.0 {
                    continue;
                }
                let _ = host.add_effect(
                    player.uuid,
                    &NativeEffect {
                        id: "minecraft:blindness".to_owned(),
                        amplifier: 0,
                        duration: 100,
                        ambient: false,
                        particles: true,
                    },
                );
            }
        }
        Attack::SporeRing => {
            let spore = ParticleOptions::simple(ParticleType::SporeBlossomAir);
            ring(host, &spore, pos, 8.0, 48);
            hurt_near(host, dimension, players, pos, 9.0, 8.0, entity);
            for player in players.iter().filter(|p| p.dimension == dimension && targetable(p)) {
                if distance(player.position, pos) > 9.0 {
                    continue;
                }
                let _ = host.add_effect(
                    player.uuid,
                    &NativeEffect {
                        id: "minecraft:poison".to_owned(),
                        amplifier: 1,
                        duration: 120,
                        ambient: false,
                        particles: true,
                    },
                );
            }
        }
        Attack::BladeSweep => {
            ring(host, &ParticleOptions::simple(ParticleType::Crit), pos, 6.0, 36);
            hurt_near(host, dimension, players, pos, 7.0, 12.0, entity);
        }
        Attack::BlossomHeal => {
            // It heals while its blossoms open: the tell is the ring of spores.
            let healed = (boss.max_health() * 0.04).min(boss.max_health());
            // Only when its health can be read: never heal blind.
            if let Some(now) = host.entity(entity).and_then(|e| health_of(host, e)) {
                let health = (now + healed).min(boss.max_health());
                let _ = host.set_mob_health(dimension, entity, health);
            }
            let _ = host.send_particles(
                &ParticleOptions::simple(ParticleType::HappyVillager),
                None,
                pos,
                [1.5, 2.0, 1.5],
                0.02,
                30,
            );
        }
    }
}

/// What keeps working during the active window: the Gauntlet's laser.
fn tick_active(
    host: &mut dyn Host,
    dimension: &str,
    entity: u32,
    boss: Boss,
    players: &[Target],
    pos: [f64; 3],
    ticks: u32,
) {
    if boss != Boss::Gauntlet {
        return;
    }
    // The current step is the laser only when the cycle is on it; a cheap
    // check is the animation the clients hold, so ask the state instead.
    let laser = bosses()
        .lock()
        .ok()
        .and_then(|map| {
            map.get(&(dimension.to_owned(), entity))
                .map(|r| matches!(r.state.step().attack, Attack::Laser))
        })
        .unwrap_or(false);
    if !laser {
        return;
    }
    let eye = [pos[0], pos[1] + 1.6, pos[2]];
    for player in players.iter().filter(|p| p.dimension == dimension && targetable(p)) {
        if distance(player.position, pos) > 24.0 {
            continue;
        }
        let to = [player.position[0], player.position[1] + 1.2, player.position[2]];
        if !host.clear_between(dimension, eye, to) {
            // Line of sight is what blocks the beam: cover is the answer.
            continue;
        }
        let beam = ParticleOptions::simple(ParticleType::EndRod);
        for step in 0..=16 {
            let t = step as f64 / 16.0;
            let at = [
                eye[0] + (to[0] - eye[0]) * t,
                eye[1] + (to[1] - eye[1]) * t,
                eye[2] + (to[2] - eye[2]) * t,
            ];
            let _ = host.send_particles(&beam, None, at, [0.05, 0.05, 0.05], 0.0, 1);
        }
        // Every fifth tick, so the hurt cooldown does not swallow it all.
        if ticks % 5 == 0 && player.entity_id > 0 {
            let _ = host.hurt_entity(
                dimension,
                player.entity_id as u32,
                3.0,
                "minecraft:magic",
                Some(entity),
            );
        }
    }
}

/// Once a server tick: count it, and drop the state and bar of bosses that
/// stopped ticking (their chunk unloaded, or they were removed without dying).
pub fn server_tick(host: &mut dyn Host, event: &mut ServerTickEvent<'_>) {
    // The hook fires at the start and at the end of every tick.
    if !matches!(event.phase, ServerTickPhase::ServerTickStart { .. }) {
        return;
    }
    let now = SERVER_TICKS.fetch_add(1, Ordering::Relaxed) + 1;
    if now % 20 != 0 {
        return;
    }
    let stale: Vec<((String, u32), HashSet<u128>)> = match bosses().lock() {
        Ok(mut map) => {
            let keys: Vec<(String, u32)> = map
                .iter()
                .filter(|(_, r)| now.saturating_sub(r.last_seen) > 40)
                .map(|(k, _)| k.clone())
                .collect();
            keys.into_iter()
                .filter_map(|k| map.remove(&k).map(|r| (k, r.viewers)))
                .collect()
        }
        Err(_) => return,
    };
    for ((_, entity), viewers) in stale {
        for uuid in viewers {
            let _ = host.boss_bar(uuid, bar_id(entity), None);
        }
        set_shielded(entity as i32, false);
    }
}

/// A boss about to die: forget it, drop its bar and tell the server.
pub fn living_death(host: &mut dyn Host, event: &mut LivingDeathEvent<'_>) -> Flow {
    let Some(boss) = Boss::from_kind(event.kind) else {
        return Flow::Continue;
    };
    // The bar went to every player near it; a dead boss's bar is cleared for
    // everyone who may still show it: its viewers, and whoever is online
    // (a bar is kept for a player who is off, so viewers are cleared too).
    let mut clear: HashSet<u128> = host.server_players().iter().map(|p| p.uuid).collect();
    if let Ok(mut map) = bosses().lock() {
        // The death hook names no level: drop this id in every level.
        map.retain(|(_, id), runtime| {
            if *id == event.entity {
                clear.extend(runtime.viewers.iter().copied());
                false
            } else {
                true
            }
        });
    }
    let cleared = clear.len();
    for uuid in clear {
        let _ = host.boss_bar(uuid, bar_id(event.entity), None);
    }
    set_shielded(event.entity as i32, false);
    trace(format_args!(
        "{boss:?} #{} died ({}), bar cleared for {cleared} players",
        event.entity, event.damage_type
    ));
    host.broadcast(Text::translate(
        "message.bosses_of_mass_destruction.boss_falls",
        &[Text::translate(boss.translation_key(), &[])],
    ));
    Flow::Continue
}

/// Damage to a shielded boss is refused, and its own burning, drowning and
/// falling are put out: a zombie template would burn in daylight and turn into
/// a drowned.
pub fn living_incoming_damage(event: &mut LivingIncomingDamageEvent<'_>) -> Flow {
    if Boss::from_kind(event.kind).is_some()
        && matches!(
            event.damage_type,
            "minecraft:on_fire" | "minecraft:in_fire" | "minecraft:drown" | "minecraft:fall"
        )
    {
        return Flow::Cancel;
    }
    let shielded = shielded()
        .lock()
        .map(|set| set.contains(&event.entity))
        .unwrap_or(false);
    if shielded {
        trace(format_args!(
            "#{} shield refused {} damage ({})",
            event.entity, event.amount, event.damage_type
        ));
        return Flow::Cancel;
    }
    Flow::Continue
}

/// A zombie template converts to a drowned when it drowns; our bosses never
/// change into another mob.
pub fn living_conversion_pre(event: &mut LivingConversionPreEvent<'_>) -> Flow {
    if Boss::from_kind(event.kind).is_some() {
        Flow::Cancel
    } else {
        Flow::Continue
    }
}

/// How far around an altar an awake boss of the same kind keeps a second one
/// from being called.
const ALTAR_CLEARANCE: f64 = 48.0;

/// Who uses an offering on an altar: the few fields `call_boss` needs, kept
/// apart from `NativeServerPlayer` (`#[non_exhaustive]`) so the tests can
/// make one.
pub(crate) struct Caller {
    pub uuid: u128,
    pub name: String,
    pub dimension: String,
    /// Creative: the offering is not used up.
    pub creative: bool,
}

/// A player uses an item on a block. An offering (`Boss::offering`) used on
/// the altar block (`logic::ALTAR`) wakes its boss over the altar; anything
/// else goes on as usual.
pub fn use_item_on(host: &mut dyn Host, event: &mut UseItemOnEvent<'_>) -> Flow {
    let Some(boss) = Boss::from_offering(&event.stack.item) else {
        return Flow::Continue;
    };
    if host.block(event.pos) != logic::ALTAR {
        return Flow::Continue;
    }
    let Some(player) = host.acting_player() else {
        return Flow::Continue;
    };
    let caller = Caller {
        uuid: player.uuid,
        name: player.name.clone(),
        dimension: player.dimension.clone(),
        creative: player.game_mode == 1,
    };
    call_boss(host, boss, event.pos, &caller)
}

/// Wakes `boss` over the altar at `altar` for `caller`, or tells them why
/// not. The use is always taken (`Flow::Cancel`): an eye of ender used on
/// the altar is not thrown, a skull or a spore blossom is not placed.
pub(crate) fn call_boss(host: &mut dyn Host, boss: Boss, altar: [i32; 3], caller: &Caller) -> Flow {
    let at = [
        f64::from(altar[0]) + 0.5,
        f64::from(altar[1]) + 1.0,
        f64::from(altar[2]) + 0.5,
    ];
    // Room over the altar for the boss to stand up.
    for dy in 1..=2 {
        let above = host.block([altar[0], altar[1] + dy, altar[2]]);
        if !matches!(
            above.as_str(),
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
        ) {
            trace(format_args!(
                "altar at {altar:?}, {boss:?} for {}: not enough room",
                caller.name
            ));
            let _ = host.tell(
                caller.uuid,
                Text::translate("message.bosses_of_mass_destruction.altar_needs_room", &[]),
            );
            return Flow::Cancel;
        }
    }
    let r = ALTAR_CLEARANCE;
    let near = host.entities_in(
        &caller.dimension,
        [at[0] - r, at[1] - r, at[2] - r],
        [at[0] + r, at[1] + r, at[2] + r],
    );
    let awake = near.into_iter().any(|id| {
        host.entity(id)
            .and_then(|e| host.entity_kind(e))
            .and_then(|kind| Boss::from_kind(&kind))
            == Some(boss)
    });
    if awake {
        trace(format_args!(
            "altar at {altar:?}, {boss:?} for {}: same boss already awake",
            caller.name
        ));
        let _ = host.tell(
            caller.uuid,
            Text::translate(
                "message.bosses_of_mass_destruction.boss_already_awake",
                &[Text::translate(boss.translation_key(), &[])],
            ),
        );
        return Flow::Cancel;
    }
    if !caller.creative && host.take_items(boss.offering(), 1) == 0 {
        // `take_items` only takes plain stacks: a renamed offering is refused.
        trace(format_args!(
            "altar at {altar:?}, {boss:?} for {}: no plain offering",
            caller.name
        ));
        let _ = host.tell(
            caller.uuid,
            Text::translate("message.bosses_of_mass_destruction.plain_offering_only", &[]),
        );
        return Flow::Cancel;
    }
    match host.summon(&caller.dimension, boss.kind(), at, Compound::new()) {
        Some(id) => {
            let soul = ParticleOptions::simple(ParticleType::SoulFireFlame);
            ring(host, &soul, at, 1.5, 24);
            let _ = host.play_sound("minecraft:entity.wither.spawn", at, 5, 1.0, 1.0);
            host.broadcast(Text::translate(
                "message.bosses_of_mass_destruction.boss_awakened",
                &[
                    Text::literal(caller.name.clone()),
                    Text::translate(boss.translation_key(), &[]),
                ],
            ));
            trace(format_args!(
                "{boss:?} #{id} called on the altar at {altar:?} by {}",
                caller.name
            ));
        }
        None => {
            if !caller.creative {
                let _ = host.give_items(boss.offering(), 1);
            }
            trace(format_args!(
                "altar at {altar:?}, {boss:?} for {}: summon failed",
                caller.name
            ));
            let _ = host.tell(
                caller.uuid,
                Text::translate("message.bosses_of_mass_destruction.altar_silent", &[]),
            );
        }
    }
    Flow::Cancel
}

fn set_shielded(entity: i32, on: bool) {
    if let Ok(mut set) = shielded().lock() {
        if on {
            set.insert(entity);
        } else {
            set.remove(&entity);
        }
    }
}

fn health_of(host: &mut dyn Host, entity: horizon_client::mod_api::EntityRef) -> Option<f32> {
    let nbt = entity.nbt(host)?;
    let tag = Tag::Compound(nbt);
    match tag.get("Health")? {
        Tag::Float(bits) => Some(f32::from_bits(*bits)),
        Tag::Double(bits) => Some(f64::from_bits(*bits) as f32),
        _ => None,
    }
}

/// The players within `radius` of `at`, in the same level. They all see the
/// boss bar and the animations.
fn clients_near(online: &[Target], dimension: &str, at: [f64; 3], radius: f64) -> Vec<Target> {
    online
        .iter()
        .filter(|p| p.dimension == dimension && distance(p.position, at) <= radius)
        .cloned()
        .collect()
}

/// Whether a boss attacks this player: alive, in survival or adventure.
fn targetable(player: &Target) -> bool {
    player.health > 0.0 && matches!(player.game_mode, 0 | 2)
}

fn nearest<'a>(dimension: &str, players: &'a [Target], at: [f64; 3]) -> Option<&'a Target> {
    players
        .iter()
        .filter(|p| p.dimension == dimension && targetable(p))
        .min_by(|a, b| {
            distance(a.position, at)
                .partial_cmp(&distance(b.position, at))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (dx, dy, dz) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// A ring of particles on the ground around `at`.
fn ring(host: &mut dyn Host, options: &ParticleOptions, at: [f64; 3], radius: f64, points: usize) {
    for step in 0..points {
        let angle = step as f64 / points as f64 * std::f64::consts::TAU;
        let point = [
            at[0] + radius * angle.cos(),
            at[1] + 0.2,
            at[2] + radius * angle.sin(),
        ];
        let _ = host.send_particles(options, None, point, [0.05, 0.05, 0.05], 0.0, 1);
    }
}

/// Damage every player within `radius` of `at`.
fn hurt_near(
    host: &mut dyn Host,
    dimension: &str,
    players: &[Target],
    at: [f64; 3],
    radius: f64,
    amount: f32,
    attacker: u32,
) {
    for player in players.iter().filter(|p| p.dimension == dimension && targetable(p)) {
        if player.entity_id <= 0 || !targetable(player) || distance(player.position, at) > radius {
            continue;
        }
        let _ = host.hurt_entity(
            dimension,
            player.entity_id as u32,
            amount,
            "minecraft:magic",
            Some(attacker),
        );
    }
}

fn send_anim(
    host: &mut dyn Host,
    player: u128,
    entity: u32,
    attack: Attack,
    phase: Phase,
    duration: u32,
) {
    let message = Message {
        entity,
        anim: attack.anim(),
        phase,
        duration: duration.min(u32::from(u16::MAX)) as u16,
    };
    let _ = host.net_send_to_player(player, crate::net::CHANNEL, &message.encode());
}

/// A whole fight against each boss, driven through the server half with a
/// test `Host` that records what the mod asked of the game (docs/native-mods.md,
/// "Testing a trait mod"). This is the headless check of the cycle, the shield,
/// the enrage, the boss bar and the death clean-up; it does not replace an
/// in-game test on a real server.
#[cfg(test)]
mod tests {
    use super::*;
    use horizon_client::mod_api::{
        BlockStateId, EntityRef, Hooks, ItemStack, NativeWorldEpoch,
        dispatch_entity_tick_listened, dispatch_living_conversion_pre, dispatch_living_death,
        dispatch_living_incoming_damage, dispatch_use_item_on,
    };
    use std::cell::Cell;

    /// The statics above are shared by every test thread: one fight at a time.
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    const PLAYER: u128 = 0xAB;
    const PLAYER_ID: i32 = 900;
    const DIM: &str = "minecraft:overworld";

    #[derive(Default)]
    struct World {
        kinds: HashMap<u32, String>,
        pos: HashMap<u32, [f64; 3]>,
        health: HashMap<u32, f32>,
        next_id: u32,
        /// The bar each player shows, by (player, bar id): (progress, colour).
        bars: HashMap<(u128, u128), (f32, i32)>,
        bar_packets: usize,
        anims: Vec<Message>,
        hurts: Vec<(u32, f32)>,
        effects: Vec<String>,
        attacks: usize,
        merged: Vec<Compound>,
        broadcasts: Cell<usize>,
        /// Blocks set by a test; air elsewhere.
        blocks: HashMap<[i32; 3], String>,
        /// How many plain offerings the acting player carries.
        carried: u32,
        told: Vec<u128>,
    }

    impl World {
        fn with_boss(id: u32, boss: Boss, at: [f64; 3]) -> Self {
            let mut world = World {
                next_id: 10_000,
                ..World::default()
            };
            world.kinds.insert(id, boss.kind().to_owned());
            world.pos.insert(id, at);
            world.health.insert(id, boss.max_health());
            world
        }
    }

    impl Host for World {
        fn entity(&self, id: u32) -> Option<EntityRef> {
            self.kinds
                .contains_key(&id)
                .then(|| EntityRef::new(NativeWorldEpoch::new(1), id))
        }
        fn entity_position(&self, entity: EntityRef) -> Option<[f64; 3]> {
            self.pos.get(&entity.id()).copied()
        }
        fn entity_kind(&self, entity: EntityRef) -> Option<String> {
            self.kinds.get(&entity.id()).cloned()
        }
        fn entity_nbt(&mut self, entity: EntityRef) -> Option<Compound> {
            let health = *self.health.get(&entity.id())?;
            let mut nbt = Compound::new();
            nbt.insert("Health".into(), Tag::Float(health.to_bits()));
            Some(nbt)
        }
        fn entity_merge_nbt(&mut self, _entity: EntityRef, nbt: Compound) -> bool {
            self.merged.push(nbt);
            true
        }
        fn entity_set_attribute_base(&mut self, _: EntityRef, _: &str, _: f64) -> bool {
            true
        }
        fn entity_fire_ticks(&self, _: EntityRef) -> Option<i32> {
            Some(0)
        }
        fn entity_set_fire(&mut self, _: EntityRef, _: i32) -> bool {
            true
        }
        fn teleport_mob(&mut self, _: &str, id: u32, pos: [f64; 3]) -> bool {
            self.pos.insert(id, pos);
            true
        }
        fn set_mob_health(&mut self, _: &str, id: u32, health: f32) -> bool {
            self.health.insert(id, health);
            true
        }
        fn boss_bar(&mut self, uuid: u128, id: u128, bar: Option<&NativeBossBar>) -> bool {
            self.bar_packets += 1;
            match bar {
                Some(bar) => {
                    self.bars.insert((uuid, id), (bar.progress, bar.color));
                }
                None => {
                    self.bars.remove(&(uuid, id));
                }
            }
            true
        }
        fn send_particles(
            &mut self,
            _: &ParticleOptions,
            _: Option<BlockStateId>,
            _: [f64; 3],
            _: [f64; 3],
            _: f64,
            _: i32,
        ) -> bool {
            true
        }
        fn play_sound(&mut self, _: &str, _: [f64; 3], _: usize, _: f32, _: f32) -> bool {
            true
        }
        fn net_send_to_player(
            &mut self,
            player: u128,
            channel: &str,
            data: &[u8],
        ) -> Result<(), String> {
            assert_eq!(player, PLAYER);
            assert_eq!(channel, crate::net::CHANNEL);
            self.anims
                .push(Message::decode(data).expect("a well-formed message"));
            Ok(())
        }
        fn server_players(&self) -> Vec<NativeServerPlayer> {
            Vec::new()
        }
        fn entities_in(&mut self, _: &str, min: [f64; 3], max: [f64; 3]) -> Vec<u32> {
            self.pos
                .iter()
                .filter(|(_, p)| (0..3).all(|i| p[i] >= min[i] && p[i] <= max[i]))
                .map(|(id, _)| *id)
                .collect()
        }
        fn summon(&mut self, _: &str, kind: &str, pos: [f64; 3], _: Compound) -> Option<u32> {
            self.next_id += 1;
            self.kinds.insert(self.next_id, kind.to_owned());
            self.pos.insert(self.next_id, pos);
            Some(self.next_id)
        }
        fn default_state(&self, _: &str) -> Option<BlockStateId> {
            None
        }
        fn mob_attack(&mut self, _: &str, _: u32, target: u32) -> bool {
            assert_eq!(target, PLAYER_ID as u32);
            self.attacks += 1;
            true
        }
        fn add_effect(&mut self, uuid: u128, effect: &NativeEffect) -> bool {
            assert_eq!(uuid, PLAYER);
            self.effects.push(effect.id.clone());
            true
        }
        fn hurt_entity(
            &mut self,
            _: &str,
            target: u32,
            amount: f32,
            _: &str,
            _: Option<u32>,
        ) -> bool {
            self.hurts.push((target, amount));
            true
        }
        fn clear_between(&self, _: &str, _: [f64; 3], _: [f64; 3]) -> bool {
            true
        }
        fn broadcast(&self, _line: Text) {
            self.broadcasts.set(self.broadcasts.get() + 1);
        }
        fn block(&self, pos: [i32; 3]) -> String {
            self.blocks
                .get(&pos)
                .cloned()
                .unwrap_or_else(|| "minecraft:air".to_owned())
        }
        fn take_items(&mut self, _: &str, count: u32) -> u32 {
            let took = count.min(self.carried);
            self.carried -= took;
            took
        }
        fn give_items(&mut self, _: &str, count: u32) -> u32 {
            self.carried += count;
            count
        }
        fn tell(&mut self, uuid: u128, _line: Text) -> bool {
            self.told.push(uuid);
            true
        }
    }

    fn player(at: [f64; 3]) -> Target {
        Target {
            uuid: PLAYER,
            dimension: DIM.to_owned(),
            position: at,
            entity_id: PLAYER_ID,
            health: 20.0,
            game_mode: 0,
        }
    }

    fn mods() -> Vec<Box<dyn Hooks>> {
        vec![crate::hooks()]
    }

    fn damage_refused(entity: u32, kind: &str, damage_type: &str) -> bool {
        let mut amount = 5.0;
        let mut canceled = false;
        dispatch_living_incoming_damage(
            &mods(),
            entity as i32,
            kind,
            damage_type,
            PLAYER_ID,
            PLAYER_ID,
            &mut amount,
            &mut canceled,
        );
        canceled
    }

    fn die(world: &mut World, entity: u32, kind: &str) -> bool {
        let mut cancel = false;
        dispatch_living_death(
            world,
            &mods(),
            entity,
            kind,
            "minecraft:player_attack",
            PLAYER_ID as u32,
            &mut cancel,
        );
        cancel
    }

    /// A full fight: every attack of the cycle is telegraphed and lands, the
    /// player is hurt, the bar follows the health, the shield window opens at
    /// half health, the enrage turns the bar red, and death clears it all.
    fn fight(boss: Boss, entity: u32) -> World {
        let at = [0.5, 64.0, 0.5];
        let kind = boss.kind();
        let mut world = World::with_boss(entity, boss, at);
        let online = [player([4.5, 64.0, 0.5])];
        let bar = (PLAYER, bar_id(entity));

        let cycle_ticks: u32 = boss
            .cycle()
            .iter()
            .map(|s| 20 + s.telegraph + s.active + s.recover)
            .sum();
        for _ in 0..cycle_ticks + 5 {
            tick_boss(&mut world, DIM, entity, boss, &online);
        }
        let told: HashSet<u8> = world
            .anims
            .iter()
            .filter(|m| m.phase == Phase::Telegraph && m.entity == entity)
            .map(|m| m.anim)
            .collect();
        let expected: HashSet<u8> = boss.cycle().iter().map(|s| s.attack.anim()).collect();
        assert_eq!(told, expected, "{boss:?} runs its whole cycle");
        assert!(
            world.anims.iter().any(|m| m.phase == Phase::Landed),
            "{boss:?} lands its attacks"
        );
        assert!(
            !world.hurts.is_empty() || world.attacks > 0,
            "{boss:?} hurts a player standing next to it"
        );
        assert!(world.hurts.iter().all(|(id, _)| *id == PLAYER_ID as u32));
        assert_eq!(world.bars.get(&bar).map(|b| b.1), Some(5), "{boss:?} shows its bar");
        // The bar is not resent every tick while nothing changes.
        assert!(
            world.bar_packets < 10,
            "{boss:?} sent {} bar packets",
            world.bar_packets
        );

        // Ordinary damage gets through; burning and drowning never do.
        assert!(!damage_refused(entity, kind, "minecraft:player_attack"));
        assert!(damage_refused(entity, kind, "minecraft:on_fire"));
        assert!(damage_refused(entity, kind, "minecraft:drown"));

        // Down to a third: the shield opens once, and the boss enrages.
        world.health.insert(entity, boss.max_health() * 0.3);
        tick_boss(&mut world, DIM, entity, boss, &online);
        assert!(
            damage_refused(entity, kind, "minecraft:player_attack"),
            "{boss:?} shielded"
        );
        let (progress, color) = world.bars[&bar];
        assert!((progress - 0.3).abs() < 0.01, "{boss:?} bar follows health");
        assert_eq!(color, 2, "{boss:?} bar turns red when enraged");
        for _ in 0..100 {
            tick_boss(&mut world, DIM, entity, boss, &online);
        }
        assert!(
            !damage_refused(entity, kind, "minecraft:player_attack"),
            "{boss:?} shield ends"
        );

        // Walking away takes the bar away; coming back shows it again.
        let far = [player([200.0, 64.0, 0.5])];
        tick_boss(&mut world, DIM, entity, boss, &far);
        assert!(!world.bars.contains_key(&bar), "{boss:?} bar leaves with the player");
        tick_boss(&mut world, DIM, entity, boss, &online);
        assert!(world.bars.contains_key(&bar));

        // Death: the bar is cleared, the state forgotten, the server told. The
        // kind is spelled as Horizon's death hook spells a modded mob.
        let death_kind = format!("minecraft:{kind}");
        assert!(
            !die(&mut world, entity, &death_kind),
            "the death goes through, so the loot drops"
        );
        assert!(!world.bars.contains_key(&bar), "{boss:?} bar cleared on death");
        assert_eq!(world.broadcasts.get(), 1);
        assert!(
            bosses().lock().unwrap().keys().all(|(_, id)| *id != entity),
            "{boss:?} state dropped"
        );
        // Its death animation still ticks it: it stays forgotten, unshielded
        // and without a bar.
        world.health.insert(entity, 0.0);
        let packets = world.bar_packets;
        for _ in 0..20 {
            tick_boss(&mut world, DIM, entity, boss, &online);
        }
        assert_eq!(world.bar_packets, packets, "{boss:?} no bar after death");
        assert!(bosses().lock().unwrap().keys().all(|(_, id)| *id != entity));
        assert!(!damage_refused(entity, kind, "minecraft:player_attack"));
        world
    }

    #[test]
    fn obsidilith_fight() {
        let _guard = serial();
        let world = fight(Boss::Obsidilith, 101);
        // Burst, wave and spike all hurt at this range.
        assert!(world.hurts.len() >= 3, "{:?}", world.hurts);
    }

    #[test]
    fn lich_fight_hovers_and_caps_its_minions() {
        let _guard = serial();
        let entity = 102;
        let world = fight(Boss::Lich, entity);
        // It hovers three blocks over where it was summoned.
        assert_eq!(world.pos[&entity][1], 67.0);
        assert!(
            world
                .merged
                .iter()
                .any(|nbt| Tag::Compound(nbt.clone()).get("NoGravity").is_some())
        );
        let skeletons = world
            .kinds
            .values()
            .filter(|k| *k == "minecraft:skeleton")
            .count();
        assert!((1..=4).contains(&skeletons), "{skeletons} skeletons");

        // Many cycles never raise more than four around it.
        let mut world = World::with_boss(103, Boss::Lich, [0.5, 64.0, 0.5]);
        let online = [player([4.5, 64.0, 0.5])];
        for _ in 0..5000 {
            tick_boss(&mut world, DIM, 103, Boss::Lich, &online);
        }
        let skeletons = world
            .kinds
            .values()
            .filter(|k| *k == "minecraft:skeleton")
            .count();
        assert!(skeletons <= 4, "{skeletons} skeletons");
        die(&mut world, 103, Boss::Lich.kind());
    }

    #[test]
    fn gauntlet_fight_punches_lasers_and_blinds() {
        let _guard = serial();
        let world = fight(Boss::Gauntlet, 104);
        assert!(world.attacks >= 1, "the punch uses the fist's own attack");
        assert!(world.effects.iter().any(|e| e == "minecraft:blindness"));
        // The laser burns while its window is open.
        assert!(
            world
                .hurts
                .iter()
                .filter(|(_, amount)| *amount == 3.0)
                .count()
                >= 2
        );
    }

    #[test]
    fn void_blossom_fight_poisons_and_heals() {
        let _guard = serial();
        let entity = 105;
        let mut world = World::with_boss(entity, Boss::VoidBlossom, [0.5, 64.0, 0.5]);
        world.health.insert(entity, 300.0);
        let online = [player([4.5, 64.0, 0.5])];
        for _ in 0..1000 {
            tick_boss(&mut world, DIM, entity, Boss::VoidBlossom, &online);
        }
        assert!(world.effects.iter().any(|e| e == "minecraft:poison"));
        assert!(world.health[&entity] > 300.0, "the blossom healed");
        die(&mut world, entity, Boss::VoidBlossom.kind());
        let world = fight(Boss::VoidBlossom, 106);
        assert!(!world.hurts.is_empty());
    }

    #[test]
    fn spectators_and_creative_players_are_left_alone() {
        let _guard = serial();
        let entity = 107;
        let mut world = World::with_boss(entity, Boss::Obsidilith, [0.5, 64.0, 0.5]);
        let mut watcher = player([3.5, 64.0, 0.5]);
        watcher.game_mode = 1;
        for _ in 0..600 {
            tick_boss(
                &mut world,
                DIM,
                entity,
                Boss::Obsidilith,
                std::slice::from_ref(&watcher),
            );
        }
        assert!(world.hurts.is_empty());
        // They still see the bar and the animations.
        assert!(world.bars.contains_key(&(PLAYER, bar_id(entity))));
        die(&mut world, entity, Boss::Obsidilith.kind());
    }

    fn caller(creative: bool) -> Caller {
        Caller {
            uuid: PLAYER,
            name: "Steve".to_owned(),
            dimension: DIM.to_owned(),
            creative,
        }
    }

    fn bosses_of(world: &World, boss: Boss) -> usize {
        world.kinds.values().filter(|k| *k == boss.kind()).count()
    }

    #[test]
    fn an_offering_on_the_altar_wakes_its_boss_once() {
        let _guard = serial();
        let altar = [3, 63, 3];
        for boss in [Boss::Obsidilith, Boss::Lich, Boss::Gauntlet, Boss::VoidBlossom] {
            let mut world = World {
                next_id: 20_000,
                carried: 2,
                ..World::default()
            };
            world.blocks.insert(altar, logic::ALTAR.to_owned());
            assert!(matches!(
                call_boss(&mut world, boss, altar, &caller(false)),
                Flow::Cancel
            ));
            assert_eq!(bosses_of(&world, boss), 1, "{boss:?} woke");
            let id = *world.kinds.keys().find(|id| world.kinds[*id] == boss.kind()).unwrap();
            assert_eq!(world.pos[&id], [3.5, 64.0, 3.5], "over the altar");
            assert_eq!(world.carried, 1, "one offering used up");
            assert_eq!(world.broadcasts.get(), 1);

            // A second call while it is awake nearby is refused, and keeps the
            // offering.
            call_boss(&mut world, boss, altar, &caller(false));
            assert_eq!(bosses_of(&world, boss), 1, "{boss:?} only once");
            assert_eq!(world.carried, 1);
            assert_eq!(world.told, vec![PLAYER]);
        }
    }

    #[test]
    fn the_altar_needs_room_and_a_plain_offering() {
        let _guard = serial();
        let altar = [0, 63, 0];
        let mut world = World::default();
        world.blocks.insert(altar, logic::ALTAR.to_owned());
        // No offering carried (a renamed one does not count): nothing wakes.
        call_boss(&mut world, Boss::Lich, altar, &caller(false));
        assert_eq!(bosses_of(&world, Boss::Lich), 0);
        // In creative nothing is used up.
        call_boss(&mut world, Boss::Lich, altar, &caller(true));
        assert_eq!(bosses_of(&world, Boss::Lich), 1);
        assert_eq!(world.carried, 0);
        // A block over the altar: refused.
        let mut world = World {
            carried: 1,
            ..World::default()
        };
        world.blocks.insert(altar, logic::ALTAR.to_owned());
        world.blocks.insert([0, 65, 0], "minecraft:stone".to_owned());
        call_boss(&mut world, Boss::Gauntlet, altar, &caller(false));
        assert_eq!(bosses_of(&world, Boss::Gauntlet), 0);
        assert_eq!(world.carried, 1);
    }

    #[test]
    fn other_items_and_blocks_are_left_alone() {
        let _guard = serial();
        let mut world = World::default();
        world.blocks.insert([0, 63, 0], logic::ALTAR.to_owned());
        let stack = |item: &str| ItemStack {
            item: item.into(),
            count: 1,
            damage: 0,
            extra: None,
        };
        for (item, pos) in [
            ("minecraft:diamond", [0, 63, 0]),
            ("minecraft:ender_eye", [5, 63, 5]),
        ] {
            let mut claimed = false;
            dispatch_use_item_on(&mut world, &mods(), &stack(item), pos, [0, 1, 0], &mut claimed);
            assert!(!claimed, "{item} at {pos:?}");
        }
        assert!(world.kinds.is_empty());
    }

    #[test]
    fn hooks_only_touch_this_mods_bosses() {
        let _guard = serial();
        let mut world = World::default();
        let mut listened = false;
        dispatch_entity_tick_listened(&mut world, &mods(), &mut listened);
        assert!(listened, "entity ticks are asked for");
        assert!(!damage_refused(5, Boss::Lich.kind(), "minecraft:player_attack"));
        assert!(
            !damage_refused(5, "minecraft:zombie", "minecraft:on_fire"),
            "a vanilla zombie still burns"
        );
        let mut cancel = false;
        dispatch_living_conversion_pre(
            &mut world,
            &mods(),
            5,
            "minecraft:zombie",
            "minecraft:drowned",
            &mut cancel,
        );
        assert!(!cancel, "a vanilla zombie still turns into a drowned");
        dispatch_living_conversion_pre(
            &mut world,
            &mods(),
            6,
            Boss::Gauntlet.kind(),
            "minecraft:drowned",
            &mut cancel,
        );
        assert!(cancel, "a boss never converts");
        die(&mut world, 5, "minecraft:zombie");
        assert_eq!(world.broadcasts.get(), 0);
    }
}
