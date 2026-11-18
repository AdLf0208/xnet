//! XNet for Horizon: a controller, its cables and connectors, and the item
//! channels that move items between the inventories they reach.
//!
//! XNet (see `FINDINGS.md`) gives a controller eight channels. A channel of type
//! `item` holds connector/side entries whose settings say whether they insert
//! into or extract from the inventory on that side, how fast, at what priority
//! and through which filter; the channel hands one item per operation from an
//! extractor's inventory to an inserter's, highest priority first.
//!
//! [`network`] is the game side: the controller's block entity and its tick, the
//! network walk, the per-tick channel pass and the `/xnet` commands. The rules
//! this glue calls are [`runtime`], carried over unchanged from the retired
//! `.hnmod` and still unit-tested on its own.

pub mod runtime;

mod network;

use std::sync::{Arc, Mutex};

use mod_api::{
    self, Flow, Hooks, HostSide, LevelTickPostEvent, RegisterCommandsEvent, Registrar,
    RegistrationContext, RunModCommandEvent, ServerHost,
};

use crate::network::{ControllerEntity, Controllers};

/// The mod's session state: the controllers its block entities ticked.
#[derive(Default)]
struct Mod {
    controllers: Arc<Mutex<Controllers>>,
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    /// Declare the controller's block entity type and attach the one part that
    /// makes it tick (it saves its eight channels there). A refusal here can only
    /// mean the same part attached twice, which is a broken build rather than a
    /// player's world.
    fn register(&self, registrar: &mut dyn Registrar, _context: &RegistrationContext) {
        let kind = registrar
            .block_entity_type("controller")
            .blocks(&["controller"])
            .build();
        let attached = registrar
            .block_entity_part(&kind, ControllerEntity::new(Arc::clone(&self.controllers)));
        debug_assert!(
            attached.is_ok(),
            "the controller part attaches once: {attached:?}"
        );
    }

    /// After the level's tick: run every item channel of the controllers that
    /// ticked. A client's tick runs nothing.
    fn level_tick_post(&self, host: HostSide, event: &mut LevelTickPostEvent) {
        let HostSide::Server(host) = host else {
            return;
        };
        // The pass reaches containers through this level's `ITEMS` capability, and
        // `LevelRef::capability` answers only for the level the server ticks now.
        // Outside that window the queue is left for the next tick rather than
        // draining work the pass could not do.
        if !event.level.in_place(host) {
            return;
        }
        let queued = {
            // Only this expression runs under the lock: one drain on the port's
            // own state, which cannot panic and so cannot poison it.
            self.controllers
                .lock()
                .expect("the controller queue lock is held for one drain")
                .take(event.level)
        };
        network::run_controllers(host, event.level, queued);
    }

    /// `/xnet info <pos>` for every player, and the operator commands that edit a
    /// channel (the controller screen is a gap).
    fn register_commands(&self, _host: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        network::register_commands(event);
    }

    /// Run the usages [`Hooks::register_commands`] declares, after the game has
    /// parsed and permission-checked them.
    fn run_mod_command(
        &self,
        host: &mut dyn ServerHost,
        event: &mut RunModCommandEvent<'_>,
    ) -> Flow {
        network::run_command(host, &self.controllers, event.command)
    }
}
