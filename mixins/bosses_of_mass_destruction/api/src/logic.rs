//! The bosses' attack cycles, as plain data and timings: no game types, so the
//! pattern is the same wherever it runs and can be tested on its own
//! (`rustc --test src/logic.rs`).
//!
//! Every attack is telegraphed: a wind-up the player can see (particles, sounds,
//! a model pose), a short active window, then a recovery long enough to answer.
//! The cycle always runs in the same order, so a player learns it and can plan;
//! only the timings tighten when a boss is enraged.

/// The block a boss is called on: an offering (`Boss::offering`) used on it
/// wakes that boss over it. Crying obsidian: found in ruined portals and
/// bartered from piglins, so a survival player can build an altar.
pub const ALTAR: &str = "minecraft:crying_obsidian";

/// A boss of this mod.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boss {
    Obsidilith,
    Lich,
    Gauntlet,
    VoidBlossom,
}

impl Boss {
    /// The entity kind, as the content mod declares it.
    pub fn kind(self) -> &'static str {
        match self {
            Boss::Obsidilith => "bosses_of_mass_destruction:obsidilith",
            Boss::Lich => "bosses_of_mass_destruction:lich",
            Boss::Gauntlet => "bosses_of_mass_destruction:gauntlet",
            Boss::VoidBlossom => "bosses_of_mass_destruction:void_blossom",
        }
    }

    /// The health its content-mod entity type declares, for the bar's progress
    /// when the saved health cannot be read.
    pub fn max_health(self) -> f32 {
        match self {
            Boss::Obsidilith => 300.0,
            Boss::Lich => 250.0,
            Boss::Gauntlet => 220.0,
            Boss::VoidBlossom => 400.0,
        }
    }

    /// The boss's name for the boss bar.
    pub fn title(self) -> &'static str {
        match self {
            Boss::Obsidilith => "Obsidilith",
            Boss::Lich => "Lich",
            Boss::Gauntlet => "Gauntlet",
            Boss::VoidBlossom => "Void Blossom",
        }
    }

    pub fn from_kind(kind: &str) -> Option<Boss> {
        // Some hooks (`living_death` and the others fed by Horizon's
        // `game::native_hooks::mob_kind`, which puts `minecraft:` before every
        // id) name a modded mob `minecraft:bosses_of_mass_destruction:lich`.
        // Seen on the dedicated-server smoke test against a85af177; f23176af
        // still does it. That spelling is read too.
        let kind = match kind.strip_prefix("minecraft:") {
            Some(rest) if rest.contains(':') => rest,
            _ => kind,
        };
        [
            Boss::Obsidilith,
            Boss::Lich,
            Boss::Gauntlet,
            Boss::VoidBlossom,
        ]
        .into_iter()
        .find(|b| b.kind() == kind)
    }

    /// The offering that calls this boss when a player uses it on an altar
    /// ([`ALTAR`]). This port's own way to start a fight without operator
    /// commands; the original mod's structures and spawners are not ported.
    /// Each is a vanilla item a survival player can get, and the earlier
    /// bosses' loot leads to the later offerings (the Obsidilith drops
    /// netherite scrap, the Gauntlet's).
    pub fn offering(self) -> &'static str {
        match self {
            Boss::Obsidilith => "minecraft:ender_eye",
            Boss::Lich => "minecraft:wither_skeleton_skull",
            Boss::Gauntlet => "minecraft:netherite_scrap",
            Boss::VoidBlossom => "minecraft:spore_blossom",
        }
    }

    /// The boss an offering calls, if any.
    pub fn from_offering(item: &str) -> Option<Boss> {
        [
            Boss::Obsidilith,
            Boss::Lich,
            Boss::Gauntlet,
            Boss::VoidBlossom,
        ]
        .into_iter()
        .find(|b| b.offering() == item)
    }

    /// At this share of its health a boss is enraged: its cycle keeps the same
    /// order and only speeds up.
    pub fn enrage_at(self) -> f32 {
        match self {
            Boss::Obsidilith => 0.5,
            Boss::Lich => 0.5,
            Boss::Gauntlet => 0.4,
            Boss::VoidBlossom => 0.35,
        }
    }

    /// The boss's cycle, in the order it always runs.
    pub fn cycle(self) -> &'static [Step] {
        const OBSIDILITH: [Step; 4] = [
            Step::new(Attack::Burst, 40, 20, 60),
            Step::new(Attack::Wave, 30, 20, 50),
            Step::new(Attack::Spike, 25, 15, 50),
            Step::new(Attack::Anvil, 35, 10, 60),
        ];
        const LICH: [Step; 4] = [
            Step::new(Attack::Volley, 30, 20, 40),
            Step::new(Attack::Comet, 45, 10, 50),
            Step::new(Attack::Minions, 30, 10, 60),
            Step::new(Attack::Teleport, 15, 5, 30),
        ];
        const GAUNTLET: [Step; 4] = [
            Step::new(Attack::Punch, 20, 10, 40),
            Step::new(Attack::Swirl, 35, 20, 50),
            Step::new(Attack::Laser, 50, 30, 60),
            Step::new(Attack::Blindness, 20, 10, 40),
        ];
        const VOID_BLOSSOM: [Step; 4] = [
            Step::new(Attack::RootSpikes, 30, 15, 40),
            Step::new(Attack::SporeRing, 35, 20, 45),
            Step::new(Attack::BladeSweep, 40, 25, 50),
            Step::new(Attack::BlossomHeal, 30, 10, 50),
        ];
        match self {
            Boss::Obsidilith => &OBSIDILITH,
            Boss::Lich => &LICH,
            Boss::Gauntlet => &GAUNTLET,
            Boss::VoidBlossom => &VOID_BLOSSOM,
        }
    }
}

/// What a boss is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attack {
    // Obsidilith: stationary, block and fire based.
    Burst,
    Wave,
    Spike,
    Anvil,
    // Lich: flying, missiles.
    Volley,
    Comet,
    Minions,
    Teleport,
    // Gauntlet: flying fist.
    Punch,
    Swirl,
    Laser,
    Blindness,
    // Void Blossom: stationary flower.
    RootSpikes,
    SporeRing,
    BladeSweep,
    BlossomHeal,
}

impl Attack {
    /// The animation id sent to clients (`net::ANIM_*`).
    pub fn anim(self) -> u8 {
        match self {
            Attack::Burst => 1,
            Attack::Wave => 2,
            Attack::Spike => 3,
            Attack::Anvil => 4,
            Attack::Volley => 5,
            Attack::Comet => 6,
            Attack::Minions => 7,
            Attack::Teleport => 8,
            Attack::Punch => 9,
            Attack::Swirl => 10,
            Attack::Laser => 11,
            Attack::Blindness => 12,
            Attack::RootSpikes => 13,
            Attack::SporeRing => 14,
            Attack::BladeSweep => 15,
            Attack::BlossomHeal => 16,
        }
    }
}

/// One attack of a cycle: its wind-up, its active window and its recovery, in
/// ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub attack: Attack,
    pub telegraph: u32,
    pub active: u32,
    pub recover: u32,
}

impl Step {
    pub const fn new(attack: Attack, telegraph: u32, active: u32, recover: u32) -> Self {
        Self {
            attack,
            telegraph,
            active,
            recover,
        }
    }

    /// The step's timings with an enraged boss's speed-up (60%, never below
    /// one tick, so a hit always has a frame to be seen).
    pub fn enraged(self) -> Self {
        let speed = |ticks: u32| ((ticks * 3) / 5).max(1);
        Self {
            attack: self.attack,
            telegraph: speed(self.telegraph),
            active: speed(self.active),
            recover: speed(self.recover),
        }
    }
}

/// The phase of one attack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Between two attacks: a beat the player breathes in.
    Idle,
    /// Wind-up: the tell.
    Telegraph,
    /// The attack lands.
    Active,
    /// After the hit, before the next wind-up.
    Recover,
}

/// What the server half does when a phase changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The wind-up started: play the tell, send the animation.
    Telegraphed(Attack),
    /// The attack lands: do its damage, blocks and projectiles.
    Landed(Attack),
    /// The attack window closed.
    Ended(Attack),
    /// The cycle moved on to another attack.
    Next(Attack),
}

/// One boss's pattern, ticking on the server.
#[derive(Clone, Debug)]
pub struct State {
    pub boss: Boss,
    pub phase: Phase,
    /// Where in the cycle the next (or current) attack is.
    pub index: usize,
    /// Ticks spent in the current phase.
    pub ticks: u32,
    /// Cycles completed, for the boss's own bookkeeping.
    pub cycle: u32,
    pub enraged: bool,
    /// Whether it cannot be hurt this tick (the Obsidilith's pillar phase).
    pub shielded: bool,
    /// How many attacks it has begun, for a bar or a log.
    pub attacks: u32,
}

impl State {
    pub fn new(boss: Boss) -> Self {
        Self {
            boss,
            phase: Phase::Idle,
            index: 0,
            ticks: 0,
            cycle: 0,
            enraged: false,
            shielded: false,
            attacks: 0,
        }
    }

    /// The attack this step of the cycle holds.
    pub fn step(&self) -> Step {
        let step = self.boss.cycle()[self.index % self.boss.cycle().len()];
        if self.enraged { step.enraged() } else { step }
    }

    /// How many ticks the current phase lasts.
    pub fn phase_length(&self) -> u32 {
        match self.phase {
            Phase::Idle => 20,
            Phase::Telegraph => self.step().telegraph,
            Phase::Active => self.step().active,
            Phase::Recover => self.step().recover,
        }
    }

    /// The health share below which it enrages (`Boss::enrage_at`).
    pub fn should_enrage(&self, health: f32, max_health: f32) -> bool {
        max_health > 0.0 && health / max_health <= self.boss.enrage_at()
    }

    /// Advance a tick; `events` collects what the server half must do.
    ///
    /// The cycle is Idle → Telegraph → Active → Recover → Idle (next step).
    /// Nothing skips an attack and nothing reorders one: that is what makes the
    /// pattern learnable, so a hit that would kill the boss does not cancel the
    /// attack it already telegraphed.
    pub fn tick(&mut self, events: &mut Vec<Event>) {
        self.ticks += 1;
        if self.ticks < self.phase_length() {
            return;
        }
        self.ticks = 0;
        match self.phase {
            Phase::Idle => {
                self.phase = Phase::Telegraph;
                self.attacks = self.attacks.saturating_add(1);
                events.push(Event::Telegraphed(self.step().attack));
            }
            Phase::Telegraph => {
                self.phase = Phase::Active;
                events.push(Event::Landed(self.step().attack));
            }
            Phase::Active => {
                self.phase = Phase::Recover;
                events.push(Event::Ended(self.step().attack));
            }
            Phase::Recover => {
                self.phase = Phase::Idle;
                self.index = (self.index + 1) % self.boss.cycle().len();
                if self.index == 0 {
                    self.cycle = self.cycle.saturating_add(1);
                }
                events.push(Event::Next(self.step().attack));
            }
        }
    }

    /// How far through the wind-up the boss is (0 at the start, 1 when it
    /// lands), for tells drawn each tick.
    pub fn telegraph_progress(&self) -> f32 {
        if self.phase != Phase::Telegraph {
            return 1.0;
        }
        let length = self.phase_length().max(1);
        self.ticks as f32 / length as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_cycle_covers_four_distinct_attacks() {
        for boss in [Boss::Obsidilith, Boss::Lich, Boss::Gauntlet, Boss::VoidBlossom] {
            let cycle = boss.cycle();
            assert_eq!(cycle.len(), 4, "{boss:?}");
            let unique: HashSet<u8> = cycle.iter().map(|s| s.attack.anim()).collect();
            assert_eq!(unique.len(), 4, "{boss:?} repeats an attack");
            for step in cycle {
                assert!(step.telegraph >= 10, "{boss:?} telegraphs too briefly");
                assert!(step.recover >= 20, "{boss:?} gives no time to answer");
            }
        }
    }

    #[test]
    fn kinds_round_trip() {
        for boss in [Boss::Obsidilith, Boss::Lich, Boss::Gauntlet, Boss::VoidBlossom] {
            assert_eq!(Boss::from_kind(boss.kind()), Some(boss));
            // The spelling the death hook uses for a modded mob.
            assert_eq!(Boss::from_kind(&format!("minecraft:{}", boss.kind())), Some(boss));
        }
        assert_eq!(Boss::from_kind("minecraft:zombie"), None);
        assert_eq!(Boss::from_kind("minecraft:lich"), None);
        assert_eq!(Boss::from_kind("other_mod:lich"), None);
    }

    #[test]
    fn each_boss_has_its_own_offering() {
        let all = [Boss::Obsidilith, Boss::Lich, Boss::Gauntlet, Boss::VoidBlossom];
        let offerings: HashSet<&str> = all.iter().map(|b| b.offering()).collect();
        assert_eq!(offerings.len(), 4);
        for boss in all {
            assert_eq!(Boss::from_offering(boss.offering()), Some(boss));
        }
        assert_eq!(Boss::from_offering("minecraft:crying_obsidian"), None);
        assert_eq!(Boss::from_offering("minecraft:diamond"), None);
    }

    #[test]
    fn the_cycle_runs_in_order_and_comes_back() {
        let mut state = State::new(Boss::Obsidilith);
        let mut seen = Vec::new();
        let mut events = Vec::new();
        // Idle (20) + telegraph (40) + active (20) + recover (60) per attack.
        let per_attack = 20 + 40 + 20 + 60;
        for _ in 0..per_attack * 4 {
            state.tick(&mut events);
            for event in events.drain(..) {
                if let Event::Telegraphed(attack) = event {
                    seen.push(attack);
                }
            }
        }
        assert_eq!(
            seen,
            vec![Attack::Burst, Attack::Wave, Attack::Spike, Attack::Anvil, Attack::Burst],
            "the cycle must be fixed and repeat"
        );
        assert_eq!(state.cycle, 1);
        assert_eq!(state.attacks, 5);
    }

    #[test]
    fn a_telegraph_is_long_enough_to_read() {
        let mut state = State::new(Boss::Gauntlet);
        let mut events = Vec::new();
        state.tick(&mut events);
        // The first step's idle ends, the wind-up starts.
        while state.phase == Phase::Idle {
            state.tick(&mut events);
        }
        assert_eq!(state.phase, Phase::Telegraph);
        let read = state.phase_length();
        assert!(read >= 20, "a 1s tell at the least");
        assert!(events.iter().all(|e| !matches!(e, Event::Landed(_))));
    }

    #[test]
    fn enraged_timings_are_shorter_but_never_zero() {
        for boss in [Boss::Obsidilith, Boss::Lich, Boss::Gauntlet, Boss::VoidBlossom] {
            let mut state = State::new(boss);
            state.enraged = true;
            let step = state.step();
            let normal = boss.cycle()[0];
            assert!(step.telegraph < normal.telegraph);
            assert!(step.telegraph >= 1 && step.active >= 1 && step.recover >= 1);
        }
    }

    #[test]
    fn enrage_thresholds_use_health_share() {
        let state = State::new(Boss::Obsidilith);
        assert!(!state.should_enrage(151.0, 300.0));
        assert!(state.should_enrage(150.0, 300.0));
        assert!(!state.should_enrage(1.0, 0.0), "a boss without health cannot be enraged");
    }

    #[test]
    fn every_attack_has_its_own_animation_id() {
        let all = [
            Attack::Burst,
            Attack::Wave,
            Attack::Spike,
            Attack::Anvil,
            Attack::Volley,
            Attack::Comet,
            Attack::Minions,
            Attack::Teleport,
            Attack::Punch,
            Attack::Swirl,
            Attack::Laser,
            Attack::Blindness,
            Attack::RootSpikes,
            Attack::SporeRing,
            Attack::BladeSweep,
            Attack::BlossomHeal,
        ];
        let ids: HashSet<u8> = all.iter().map(|a| a.anim()).collect();
        assert_eq!(ids.len(), all.len());
        assert!(ids.iter().all(|id| *id != 0), "0 is idle");
    }
}
