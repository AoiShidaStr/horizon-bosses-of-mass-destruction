//! The client half: the four bosses are drawn with the models built in
//! `model.rs`, posed from the animation state the server sends (`net.rs`), in
//! place of the zombie model of the entity type they copy.
use crate::logic::Boss;
use crate::model;
use crate::net::{Message, Phase};
use horizon_client::mod_api::{
    BlockEntityDraw, Flow, Host, LayerDefinitionsEvent, MobSubmitEvent, ModelPart, NetFromServerEvent,
};
use std::{
    collections::HashMap,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU32, Ordering},
    },
};

/// The client's own tick counter, started with the world, used to time the
/// attack animations it was told about.
static TICKS: AtomicU32 = AtomicU32::new(0);

/// A baked model per layer id: baking reads the layer's mesh and builds its
/// polygons, so it is done once and reused each frame.
fn models() -> &'static Mutex<HashMap<String, ModelPart>> {
    static MODELS: OnceLock<Mutex<HashMap<String, ModelPart>>> = OnceLock::new();
    MODELS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// What a boss is animating, as the last message said.
#[derive(Clone, Copy)]
struct Anim {
    anim: u8,
    phase: Phase,
    start: u32,
    duration: u32,
}

fn anims() -> &'static Mutex<HashMap<u32, Anim>> {
    static ANIMS: OnceLock<Mutex<HashMap<u32, Anim>>> = OnceLock::new();
    ANIMS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Count a client tick (the `tick` hook runs once each).
pub fn tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
}

/// Register the four models (once, when the game asks for its layers).
pub fn layer_definitions(event: &mut LayerDefinitionsEvent<'_>) {
    event.layers.extend(model::layers());
}

/// A boss's animation, from the message the server sent, timed to how far the
/// phase has come with the client's own clock.
fn current(entity: u32) -> Option<(Anim, f32)> {
    let anim = *anims().lock().ok()?.get(&entity)?;
    let now = TICKS.load(Ordering::Relaxed);
    let elapsed = now.saturating_sub(anim.start);
    let length = anim.duration.max(1);
    let progress = (elapsed as f32 / length as f32).clamp(0.0, 1.0);
    Some((anim, progress))
}

/// The server told us a boss started, landed or ended an attack.
pub fn net_from_server(event: &mut NetFromServerEvent<'_>) {
    if event.channel != crate::net::CHANNEL {
        return;
    }
    let Some(message) = Message::decode(event.data) else {
        return;
    };
    let Ok(mut map) = anims().lock() else {
        return;
    };
    match message.phase {
        // A new wind-up replaces whatever played before.
        Phase::Telegraph => {
            map.insert(
                message.entity,
                Anim {
                    anim: message.anim,
                    phase: message.phase,
                    start: TICKS.load(Ordering::Relaxed),
                    duration: u32::from(message.duration),
                },
            );
        }
        Phase::Landed => {
            if let Some(anim) = map.get_mut(&message.entity) {
                anim.phase = message.phase;
                anim.start = TICKS.load(Ordering::Relaxed);
                anim.duration = u32::from(message.duration);
            }
        }
        Phase::Ended => {
            map.remove(&message.entity);
        }
    }
}

/// Draw a boss with its own model; `Flow::Cancel` tells the game not to draw
/// the copied zombie underneath.
pub fn mob_submit(host: &dyn Host, event: &mut MobSubmitEvent<'_>) -> Flow {
    let kind = event.mob.kind;
    let (Some(layer), Some(texture)) = (model::layer_for(kind), model::texture_for(kind)) else {
        return Flow::Continue;
    };
    let Some(boss) = Boss::from_kind(kind) else {
        return Flow::Continue;
    };
    let Some(model) = baked(host, layer) else {
        return Flow::Continue;
    };

    let mut model = model;
    model.reset_all_poses();
    let (anim, progress) = current(event.mob.id)
        .map(|(anim, progress)| (Some(anim), progress))
        .unwrap_or((None, 0.0));
    pose(&mut model, boss, event.mob.age, anim.map(|a| (a.anim, a.phase)), progress);

    let draw: &mut BlockEntityDraw = event.draw;
    draw.push();
    // Vanilla's renderer frame: from the entity's feet to the model's own
    // (+y down, front towards -z).
    draw.scale([1.0, -1.0, -1.0]);
    draw.translate([0.0, -1.501, 0.0]);
    if event.mob.death_time > 0 {
        draw.rotate_about(
            [0.0, 0.0, 1.0],
            (event.mob.death_time as f32 * 4.5).min(90.0),
            [0.0, 24.0, 0.0],
        );
    }
    // A hurt flash tints the model white for a moment, as vanilla's overlay does.
    let tint = if event.mob.hurt_time > 0 { 0xFFB0B0 } else { 0xFFFFFF };
    host.draw_model_part(draw, &model, texture, tint);
    draw.pop();
    Flow::Cancel
}

/// A baked copy of `layer`, cached; `None` until the game has registered it.
fn baked(host: &dyn Host, layer: &str) -> Option<ModelPart> {
    let mut cache = models().lock().ok()?;
    if let Some(cached) = cache.get(layer) {
        return Some(cached.clone());
    }
    let baked = host.bake_layer(layer)?;
    cache.insert(layer.to_owned(), baked.clone());
    Some(baked)
}

/// Pose the model for this frame: idle motion, then the attack the server
/// announced, timed by `progress` (0 at the start of the phase, 1 at its end).
fn pose(
    model: &mut ModelPart,
    boss: Boss,
    age: f32,
    anim: Option<(u8, Phase)>,
    progress: f32,
) {
    // Idle: the whole model breathes, and the head turns a little.
    let breath = (age * 0.06).sin();
    if let Some(body) = model.get_child_mut("body") {
        body.y += breath * 0.6;
    }
    let swing = anim
        .filter(|(_, phase)| *phase == Phase::Telegraph)
        .map(|_| -progress * 0.9)
        .unwrap_or(0.0);

    match boss {
        Boss::Obsidilith => {
            for (name, mirror) in [("left_arm", 1.0), ("right_arm", -1.0)] {
                if let Some(arm) = model.get_child_mut(name) {
                    arm.x_rot = swing * mirror;
                }
            }
            if let Some(head) = model.get_child_mut("head") {
                head.y_rot = (age * 0.03).sin() * 0.15;
            }
        }
        Boss::Lich => {
            // It floats: a slow drift instead of legs.
            if let Some(body) = model.get_child_mut("body") {
                body.y += (age * 0.08).sin() * 1.2;
            }
            if let Some(orb) = model.get_child_mut("orb") {
                orb.y_rot = age * 0.08;
                orb.x_rot = (age * 0.05).sin() * 0.2;
                if anim.is_some() {
                    orb.x_scale = 1.0 + progress * 0.6;
                    orb.y_scale = orb.x_scale;
                    orb.z_scale = orb.x_scale;
                }
            }
            for (name, mirror) in [("left_arm", 1.0), ("right_arm", -1.0)] {
                if let Some(arm) = model.get_child_mut(name) {
                    arm.z_rot = swing * 0.5 * mirror;
                }
            }
        }
        Boss::Gauntlet => {
            // The fingers curl on a punch, and spread on the swirl.
            for index in 0..4 {
                if let Some(finger) = model.get_child_mut(&format!("finger_{index}")) {
                    finger.x_rot = swing + progress * 0.5;
                }
            }
            if let Some(thumb) = model.get_child_mut("thumb") {
                thumb.z_rot = swing * 0.6;
            }
            if let Some(gem) = model.get_child_mut("gem") {
                gem.y_rot = age * 0.12;
                if anim.is_some() {
                    gem.x_scale = 1.0 + progress * 0.8;
                    gem.y_scale = gem.x_scale;
                    gem.z_scale = gem.x_scale;
                }
            }
        }
        Boss::VoidBlossom => {
            // The petals open on a wind-up and snap shut when the attack lands.
            let open = 0.25 + if anim.is_some() { progress * 0.7 } else { 0.0 };
            for index in 0..6 {
                if let Some(petal) = model.get_child_mut(&format!("petal_{index}")) {
                    petal.x_rot = -(0.4 + open);
                }
            }
            if let Some(eye) = model.get_child_mut("eye") {
                eye.y_rot = age * 0.07;
                if anim.is_some() {
                    eye.x_scale = 1.0 + progress * 0.5;
                    eye.y_scale = eye.x_scale;
                    eye.z_scale = eye.x_scale;
                }
            }
        }
    }
}
