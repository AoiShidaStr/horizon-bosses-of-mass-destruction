//! Bosses of Mass Destruction for Horizon: the native (trait mod) half.
//!
//! A Horizon port of Bosses of Mass Destruction by Barribob and miyo6032
//! (LGPL-3.0-only, see LICENSE and NOTICE.md in the mod folder). The attack
//! logic here is a new, simplified implementation written for Horizon's mod
//! API; no Java code or GeckoLib data was translated (README.md, "Status").
//!
//! The launcher links this crate into the game and calls `hooks()`
//! (docs/native-mods.md, "Trait mods"). The same `Hooks` serve a dedicated
//! server (only the server hooks fire there) and a client (the drawing hooks).
pub mod logic;
pub mod net;

mod client;
mod model;
mod server;

use horizon_client::mod_api::{
    ClientTickPostEvent, EntityTickListenedEvent, EntityTickPostEvent, Flow, Hooks, Host,
    LayerDefinitionsEvent, LivingConversionPreEvent, LivingDeathEvent,
    LivingIncomingDamageEvent, MobSubmitEvent, NetFromServerEvent, RegisterGoalsEvent,
    ServerTickEvent, UseItemOnEvent,
};

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Mod)
}

struct Mod;

impl Hooks for Mod {
    // Server side.
    fn register_goals(&self, event: &mut RegisterGoalsEvent<'_>) {
        server::register_goals(event);
    }

    fn entity_tick_listened(&self, host: &mut dyn Host, event: &mut EntityTickListenedEvent) {
        server::entity_tick_listened(host, event);
    }

    fn entity_tick_post(&self, host: &mut dyn Host, event: &mut EntityTickPostEvent<'_>) {
        server::entity_tick_post(host, event);
    }

    fn server_tick(&self, host: &mut dyn Host, event: &mut ServerTickEvent<'_>) {
        server::server_tick(host, event);
    }

    fn living_death(&self, host: &mut dyn Host, event: &mut LivingDeathEvent<'_>) -> Flow {
        server::living_death(host, event)
    }

    fn living_incoming_damage(&self, event: &mut LivingIncomingDamageEvent<'_>) -> Flow {
        server::living_incoming_damage(event)
    }

    fn living_conversion_pre(&self, _host: &mut dyn Host, event: &mut LivingConversionPreEvent<'_>) -> Flow {
        server::living_conversion_pre(event)
    }

    fn use_item_on(&self, host: &mut dyn Host, event: &mut UseItemOnEvent<'_>) -> Flow {
        server::use_item_on(host, event)
    }

    // Client side.
    fn layer_definitions(&self, event: &mut LayerDefinitionsEvent<'_>) {
        client::layer_definitions(event);
    }

    fn mob_submit(&self, host: &dyn Host, event: &mut MobSubmitEvent<'_>) -> Flow {
        client::mob_submit(host, event)
    }

    fn net_from_server(&self, _host: &mut dyn Host, event: &mut NetFromServerEvent<'_>) {
        client::net_from_server(event);
    }

    fn client_tick_post(&self, _host: &mut dyn Host, _event: &mut ClientTickPostEvent) {
        client::tick();
    }
}
