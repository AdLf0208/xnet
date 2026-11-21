// Game-side glue for XNet's controller, connectors and cables: the controller's
// block entity, the network walk, the per-tick item pass and the `/xnet`
// commands (the retired module `xnet_state`).
//
// Upstream a controller owns one network: it inherits the colour of an adjacent
// connector (`findNeighbourConnector`) and its channels hold connector/side
// pairs whose item settings say insert or extract, how fast, at what priority
// and through which filter. After the level's tick each channel runs the
// extractors that are due, handing one item per operation to the channel's
// inserters, highest priority first.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use mod_api::{
    BlockEntityPart, BlockEntityTickEvent, BlockEntityTicksEvent, BlockKey, BlockPart, Capability,
    Command, CommandArgumentValue, CommandInvocation, Compound, Direction, Flow, Host,
    HostTransaction, ITEMS, ItemResource, LevelRef, RegisterCommandsEvent, ResourceHandler,
    ServerHost, Tag, Text,
};

use crate::runtime::{
    Channel, ChannelMode, ChannelType, Controller, Entry, FACE_COUNT, FACE_NAMES, ItemSettings,
    MAX_CHANNELS, MAX_KNOWN_CONTROLLERS, MAX_NETWORK_VISITS, OPERATION_CAPACITY, Pos,
    SINGLE_AMOUNT, StackMode, TransferMode, destination_plan, diagnostics_permitted, face_named,
    is_operation_tick, next_offset, on_network, opposite, phase_of, prune_known, step,
    walk_network,
};

/// The controller block (and its item).
pub(crate) const CONTROLLER: &str = "xnet:controller";
/// The connector block; its item is `xnet:connector_blue`.
pub(crate) const CONNECTOR: &str = "xnet:connector";
/// The cable block; its item is `xnet:netcable_blue`.
pub(crate) const CABLE: &str = "xnet:netcable";

/// The engine's block identities, resolved once per pass (`BlockKey` is the
/// engine's own identity: comparing it costs no string).
struct Keys {
    controller: BlockKey,
    connector: BlockKey,
    cable: BlockKey,
}

/// The blocks this mod is about, or `None` while the game has not registered them.
fn keys(host: &dyn Host) -> Option<Keys> {
    Some(Keys {
        controller: host.block_key(CONTROLLER)?,
        connector: host.block_key(CONNECTOR)?,
        cable: host.block_key(CABLE)?,
    })
}

/// Controllers known to the level tick, per level.
///
/// `pending` is drained at the end of each level tick. `known` keeps every
/// controller whose block entity has ticked at least once: the engine has no
/// level-wide block-entity list, so the `/xnet info` readout walks this set and
/// checks each position against the level before using it.
#[derive(Default)]
pub(crate) struct Controllers {
    pending: HashMap<LevelRef, BTreeSet<Pos>>,
    known: HashMap<LevelRef, BTreeSet<Pos>>,
    /// Each channel's round-robin offset, by controller and channel: session state,
    /// as upstream keeps `roundRobinOffset` on the channel's settings object in
    /// memory. A write into the block entity from the pass would be reverted at this
    /// pin, and upstream's offset restarts at zero on load too.
    offsets: HashMap<(LevelRef, Pos, usize), usize>,
}

impl Controllers {
    /// Note `pos` as a controller of `level`, queued for this tick's pass.
    fn push(&mut self, level: LevelRef, pos: Pos) {
        self.pending.entry(level).or_default().insert(pos);
        self.known.entry(level).or_default().insert(pos);
    }

    /// The controllers queued for `level`, in position order (empty afterwards).
    pub(crate) fn take(&mut self, level: LevelRef) -> BTreeSet<Pos> {
        self.pending.remove(&level).unwrap_or_default()
    }

    /// Channel `channel`'s round-robin offset for the controller at `pos`.
    pub(crate) fn offset(&self, level: LevelRef, pos: Pos, channel: usize) -> usize {
        self.offsets
            .get(&(level, pos, channel))
            .copied()
            .unwrap_or(0)
    }

    /// Remember `offset` as channel `channel`'s round-robin offset.
    pub(crate) fn remember_offset(
        &mut self,
        level: LevelRef,
        pos: Pos,
        channel: usize,
        offset: usize,
    ) {
        self.offsets.insert((level, pos, channel), offset);
    }

    /// Every controller of `level` seen so far, in position order.
    fn known(&self, level: LevelRef) -> Vec<Pos> {
        self.known
            .get(&level)
            .map(|positions| positions.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Forget the controllers of `level` that no longer hold a controller block, and keep the
    /// set at its cap: a player can place and remove controllers for a whole session, and the
    /// readout's index is not a place for that history to live. Their round-robin offsets go
    /// with them.
    fn prune(&mut self, level: LevelRef, still_there: impl Fn(Pos) -> bool) {
        if let Some(known) = self.known.get_mut(&level) {
            prune_known(known, still_there, MAX_KNOWN_CONTROLLERS);
            if known.is_empty() {
                self.known.remove(&level);
            }
        }
        let live = self.known.get(&level);
        self.offsets.retain(|(offset_level, pos, _), _| {
            *offset_level != level || live.is_some_and(|live| live.contains(pos))
        });
    }
}

/// The part attached to the controller's block entity type: it makes a
/// controller tick, and its tick only queues it for the level tick's end.
pub(crate) struct ControllerEntity {
    controllers: Arc<Mutex<Controllers>>,
}

impl ControllerEntity {
    pub(crate) fn new(controllers: Arc<Mutex<Controllers>>) -> ControllerEntity {
        ControllerEntity { controllers }
    }
}

#[mod_api::horizon_part]
impl BlockPart for ControllerEntity {}

#[mod_api::horizon_part]
impl BlockEntityPart for ControllerEntity {
    /// A controller ticks, so the level tick's end can run its channels.
    fn block_entity_ticks(&self, _host: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    /// One controller tick: queue it for the level tick's end. The tick itself
    /// writes no block entity data; the pass at the end of the tick does.
    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        let Some(level) = level_in_place(host) else {
            return;
        };
        // Only this expression runs under the lock: one insert on the port's own
        // state, which cannot panic and so cannot poison it.
        self.controllers
            .lock()
            .expect("the controller queue lock is held for one insert")
            .push(level, event.pos);
    }
}

/// The level the server is ticking now: the one a block-entity tick runs in.
fn level_in_place(host: &dyn ServerHost) -> Option<LevelRef> {
    host.levels()
        .list_levels()
        .into_iter()
        .find(|&level| level.in_place(host))
}

/// The three keys one channel saves under: its kind, its connector sides and its
/// distribution mode.
fn channel_keys(index: usize) -> (String, String, String) {
    (
        format!("ch{index}.type"),
        format!("ch{index}"),
        format!("ch{index}.mode"),
    )
}

/// The string a compound holds under `name`, if it holds one.
fn text(nbt: &Compound, name: &str) -> Option<String> {
    match nbt.get(&name.into()) {
        Some(Tag::String(value)) => Some(value.to_string_lossy()),
        _ => None,
    }
}

/// The channels a controller's block entity holds; a fresh controller has eight
/// unused ones.
fn read_controller(nbt: &Compound) -> Controller {
    let mut controller = Controller::new();
    for index in 0..MAX_CHANNELS {
        let (type_key, entries_key, mode_key) = channel_keys(index);
        let Some(kind) = text(nbt, &type_key).and_then(|name| ChannelType::named(&name)) else {
            continue;
        };
        if kind == ChannelType::None {
            continue;
        }
        let saved = text(nbt, &entries_key).unwrap_or_default();
        let mode = text(nbt, &mode_key)
            .and_then(|name| ChannelMode::named(&name))
            .unwrap_or_default();
        controller = controller.with_channel(index, Channel::load(kind, &saved).with_mode(mode));
    }
    controller
}

/// The data that sets every channel of `controller` in its block entity, in
/// merge form: an unused channel is written as `none`, which the reader skips,
/// so a merge can never leave a channel the player removed behind.
fn controller_data(controller: &Controller) -> Compound {
    let mut data = Compound::new();
    for index in 0..MAX_CHANNELS {
        let (type_key, entries_key, mode_key) = channel_keys(index);
        let (kind, saved, mode) = match controller.channel(index) {
            Some(channel) => (channel.kind(), channel.save(), channel.mode()),
            None => (ChannelType::None, String::new(), ChannelMode::Priority),
        };
        data.insert(type_key.as_str().into(), Tag::String(kind.name().into()));
        data.insert(mode_key.as_str().into(), Tag::String(mode.name().into()));
        if kind != ChannelType::None && !saved.is_empty() {
            data.insert(
                entries_key.as_str().into(),
                Tag::String(saved.as_str().into()),
            );
        }
    }
    data
}

/// A controller's channels, read out of its block entity; `None` when `pos`
/// holds no controller or its entity has no data yet.
fn controller_at(host: &dyn Host, keys: &Keys, level: LevelRef, pos: Pos) -> Option<Controller> {
    if level.block(host, pos) != Some(keys.controller) {
        return None;
    }
    let entity = host.block_entity(level, pos)?;
    entity.data(host).map(|data| read_controller(&data))
}

/// Whether `pos` holds a cable or a connector, the two blocks a network is made
/// of (upstream's blob holds both; the controller is the network's provider).
fn is_wire(host: &dyn Host, keys: &Keys, level: LevelRef, pos: Pos) -> bool {
    matches!(level.block(host, pos), Some(block) if block == keys.cable || block == keys.connector)
}

/// The contiguous run of cables and connectors `start` belongs to, in a stable order, read
/// through [`walk_network`]'s visit budget. One colour only in this slice, so adjacency is the
/// whole rule upstream's `ChunkBlob.createCableSegment` applies to one colour; the budget is
/// what keeps a player-built net from turning one tick into a whole-server walk.
fn network_of(host: &dyn Host, keys: &Keys, level: LevelRef, start: Pos) -> Vec<Pos> {
    walk_network(start, MAX_NETWORK_VISITS, |pos| {
        is_wire(host, keys, level, pos)
    })
}

/// The network a controller owns: the run through the connector it touches, or
/// none when it touches no connector (upstream's `findNeighbourConnector` looks
/// for a `ConnectorBlock`; a controller beside a bare cable has no network and
/// its channels never run).
fn controller_network(
    host: &dyn Host,
    keys: &Keys,
    level: LevelRef,
    controller: Pos,
) -> Option<Vec<Pos>> {
    let mut connectors: Vec<Pos> = (0..FACE_COUNT)
        .map(|face| step(controller, face))
        .filter(|&beside| level.block(host, beside) == Some(keys.connector))
        .collect();
    connectors.sort_unstable();
    let start = connectors.first().copied()?;
    Some(network_of(host, keys, level, start))
}

/// How many items a container holds that pass `settings`' filter (upstream's
/// `countItems`, which a stock-keeping count reads). An empty filter matches
/// everything, as `getMatcher` does, so a count with no filter limits the whole
/// container.
fn count_matching(
    host: &mut dyn ServerHost,
    container: &Capability<dyn ResourceHandler<ItemResource>>,
    settings: &ItemSettings,
) -> u32 {
    let mut total: u32 = 0;
    for slot in 0..container.size(host) {
        let Some(resource) = container.resource(host, slot) else {
            continue;
        };
        if settings.matches(&resource.item().id(host)) {
            total = total.saturating_add(container.amount(host, slot));
        }
    }
    total
}

/// One extractor's operation: the channel it belongs to, its connector sides, the
/// controller's live network and the destination order this operation tries.
struct Operation<'a> {
    level: LevelRef,
    network: &'a BTreeSet<Pos>,
    entries: &'a [Entry],
    plan: &'a [usize],
}

/// One channel of one controller, for this tick: where it lives, its network and the
/// session state its round-robin offset is kept in.
struct ChannelRun<'a> {
    level: LevelRef,
    network: &'a BTreeSet<Pos>,
    controller: Pos,
    index: usize,
    controllers: &'a Mutex<Controllers>,
    game_time: u64,
}

/// Run one extractor's operation: the first matching slot whose items a destination
/// accepts, up to what the settings say one operation carries (`StackMode`), handed
/// to the channel's inserters in the operation's plan order. An item no destination
/// accepts stays where it is, and the next matching slot is tried, as upstream's
/// `tickItemHandler` does. Returns how many destinations accepted something, which
/// advances a round-robin channel's offset.
fn run_operation(host: &mut dyn ServerHost, operation: &Operation<'_>, index: usize) -> usize {
    let source = &operation.entries[index];
    let from = step(source.connector, source.side);
    let from_face = Direction::ALL[opposite(source.side)];
    if operation.plan.is_empty() {
        return 0;
    }
    let Some(source_container) = operation
        .level
        .capability(host, &ITEMS, from, Some(from_face))
    else {
        return 0;
    };
    for slot in 0..source_container.size(host) {
        let Some(resource) = source_container.resource(host, slot) else {
            continue;
        };
        // The filter is by item id, as upstream's ghost slots are: the stack
        // itself (components and all) is what moves.
        let item = resource.item().id(host);
        if !source.settings.matches(&item) {
            continue;
        }
        // A stock-keeping count needs the whole source's total of this item, not
        // just the slot's; with no count it is never read.
        let matching_total = match source.settings.count() {
            Some(_) => count_matching(host, &source_container, &source.settings),
            None => 0,
        };
        let take = source
            .settings
            .take_amount(source_container.amount(host, slot), matching_total);
        if take == 0 {
            continue;
        }
        let used = move_up_to(
            host,
            operation,
            &source_container,
            slot,
            &resource,
            &item,
            take,
        );
        if used > 0 {
            return used;
        }
    }
    0
}

/// Move up to `take` of `resource` from `slot` of `from` into the plan's destinations,
/// in order, each as far as its own stock count allows; whatever no destination takes
/// stays in its slot. One transaction, as upstream's `insertStack` plus its
/// `fetchItem` correction leaves the source with only what was inserted. Returns how
/// many destinations accepted an item.
fn move_up_to(
    host: &mut dyn ServerHost,
    operation: &Operation<'_>,
    from: &Capability<dyn ResourceHandler<ItemResource>>,
    slot: usize,
    resource: &ItemResource,
    item: &str,
    take: u32,
) -> usize {
    // Where this operation may go, with each destination's room: resolved before the
    // transaction, because a capability is a read of the level.
    let mut targets: Vec<(Capability<dyn ResourceHandler<ItemResource>>, u32)> = Vec::new();
    let mut unplaced = take;
    for &destination in operation.plan {
        if unplaced == 0 {
            break;
        }
        let target = &operation.entries[destination];
        // A saved connector is only a destination while it is still on the
        // controller's live network: its cable or connector may be gone since the
        // channel was saved (the review finding).
        if !target.settings.matches(item) || !on_network(operation.network, target.connector) {
            continue;
        }
        let to = step(target.connector, target.side);
        let to_face = Direction::ALL[opposite(target.side)];
        let Some(container) = operation.level.capability(host, &ITEMS, to, Some(to_face)) else {
            continue;
        };
        let stocked = match target.settings.count() {
            Some(_) => count_matching(host, &container, &target.settings),
            None => 0,
        };
        let room = target.settings.accept_amount(unplaced, stocked);
        if room == 0 {
            continue;
        }
        unplaced -= room;
        targets.push((container, room));
    }
    if targets.is_empty() {
        return 0;
    }
    host.transaction(|host, tx| {
        let taken = from.extract(host, tx, slot, resource, take);
        if taken == 0 {
            return Err(());
        }
        let mut inserted = 0;
        let mut used = 0;
        for (container, room) in &targets {
            let allowed = (*room).min(taken - inserted);
            if allowed == 0 {
                continue;
            }
            let went = container.insert_any(host, tx, resource, allowed);
            if went > 0 {
                used += 1;
            }
            inserted += went;
            if inserted >= taken {
                break;
            }
        }
        // Whatever did not arrive stays in its slot, as `ItemHandlerHelper.insertItem`'s
        // contract leaves it; a source that cannot take it back aborts the transaction,
        // so nothing leaves a container without arriving somewhere.
        let keep = taken - inserted;
        if keep > 0 && from.insert(host, tx, slot, resource, keep) != keep {
            return Err(());
        }
        Ok(used)
    })
    .unwrap_or(0)
}

/// Run every extractor of one channel that is due this tick and still on the
/// controller's network, advancing the channel's round-robin offset as upstream's
/// `insertStack` does.
fn run_channel(host: &mut dyn ServerHost, run: &ChannelRun<'_>, channel: &Channel) {
    let entries = channel.entries();
    if channel.is_empty() {
        return;
    }
    for (index, entry) in entries.iter().enumerate() {
        if entry.settings.mode() != TransferMode::Extract
            || !on_network(run.network, entry.connector)
            || !is_operation_tick(
                run.game_time,
                phase_of(entry.connector, entry.side),
                u64::from(entry.settings.interval()),
            )
        {
            continue;
        }
        // Only this expression runs under the lock: one read of the port's own state.
        let offset = run
            .controllers
            .lock()
            .expect("the round-robin lock is held for one read")
            .offset(run.level, run.controller, run.index);
        let plan = destination_plan(entries, entry, channel.mode(), offset);
        let operation = Operation {
            level: run.level,
            network: run.network,
            entries,
            plan: &plan,
        };
        let used = run_operation(host, &operation, index);
        if used > 0 {
            let next = next_offset(offset, plan.len(), used);
            run.controllers
                .lock()
                .expect("the round-robin lock is held for one write")
                .remember_offset(run.level, run.controller, run.index, next);
        }
    }
}

/// After the level's tick: run every item channel of every controller queued
/// this tick.
pub(crate) fn run_controllers(
    host: &mut dyn ServerHost,
    level: LevelRef,
    queued: BTreeSet<Pos>,
    controllers: &Mutex<Controllers>,
) {
    if queued.is_empty() {
        return;
    }
    let Some(keys) = keys(host) else {
        return;
    };
    let Some(game_time) = level.game_time(host) else {
        return;
    };
    let game_time = game_time.max(0) as u64;
    for controller_pos in queued {
        // Read the channels out before the transfer borrows the game mutably, and before the
        // network walk: a controller with no item channel has no transfer to make, so its
        // cable net is never read (the review finding).
        let Some(channels) = controller_at(host, &keys, level, controller_pos).map(|controller| {
            controller
                .channels()
                .filter(|(_, channel)| channel.kind() == ChannelType::Item)
                .map(|(index, channel)| (index, channel.clone()))
                .collect::<Vec<(usize, Channel)>>()
        }) else {
            continue;
        };
        if channels.iter().all(|(_, channel)| channel.is_empty()) {
            continue;
        }
        let Some(network) = controller_network(host, &keys, level, controller_pos) else {
            continue;
        };
        let network: BTreeSet<Pos> = network.into_iter().collect();
        for (index, channel) in channels {
            run_channel(
                host,
                &ChannelRun {
                    level,
                    network: &network,
                    controller: controller_pos,
                    index,
                    controllers,
                    game_time,
                },
                &channel,
            );
        }
    }
}

/// A mode's word in the readout: upstream stores `ins`/`ext` and the screen says
/// Insert/Extract.
fn mode_word(mode: TransferMode) -> &'static str {
    match mode {
        TransferMode::Insert => "insert",
        TransferMode::Extract => "extract",
    }
}

/// How much one operation carries, as the readout words it: `SINGLE` one item, `STACK`
/// the whole slot up to the operation cap, `COUNT` the configured amount.
fn amount_phrase(settings: &ItemSettings) -> String {
    let amount = match settings.stack_mode() {
        StackMode::Single => SINGLE_AMOUNT,
        StackMode::Stack => OPERATION_CAPACITY,
        StackMode::Count => settings.extract_amount(),
    };
    let noun = if amount == 1 { "item" } else { "items" };
    format!("{amount} {noun}")
}

/// A connector side's stack mode and stock count, as the readout adds them: the
/// mode's name, and `keep` for an extractor's or `stock` for an inserter's count.
fn stock_phrase(entry: &Entry) -> String {
    let mut phrase = entry.settings.stack_mode().name().to_string();
    if let Some(count) = entry.settings.count() {
        let word = match entry.settings.mode() {
            TransferMode::Extract => "keep",
            TransferMode::Insert => "stock",
        };
        phrase.push_str(&format!(", {word} {count}"));
    }
    phrase
}

/// A connector side's settings as one readout line: side, mode, rate, priority,
/// filter, and how much one operation carries.
fn settings_line(entry: &Entry) -> String {
    let filter = match (
        entry.settings.filters().is_empty(),
        entry.settings.blacklist(),
    ) {
        (true, _) => "all".to_string(),
        (false, false) => entry.settings.filters().join(" "),
        (false, true) => format!("not {}", entry.settings.filters().join(" ")),
    };
    format!(
        "{x},{y},{z} {side} {mode}, {amount} every {interval} ticks, priority {priority}, filter {filter}, {stock}",
        x = entry.connector[0],
        y = entry.connector[1],
        z = entry.connector[2],
        side = FACE_NAMES[entry.side],
        mode = mode_word(entry.settings.mode()),
        amount = amount_phrase(&entry.settings),
        interval = entry.settings.interval(),
        priority = entry.settings.priority(),
        stock = stock_phrase(entry),
    )
}

/// The network members as one readout field.
fn members(network: &[Pos]) -> String {
    if network.is_empty() {
        return "none".to_string();
    }
    network
        .iter()
        .map(|member| format!("{},{},{}", member[0], member[1], member[2]))
        .collect::<Vec<String>>()
        .join(" ")
}

/// Answer the command's source (its silence, track-output and swapped-in player
/// rules apply, as for any command).
fn feedback(host: &mut dyn ServerHost, line: &str) {
    let _ = host.commands().feedback(Text::literal(line));
}

/// The controllers of `level` the level tick has seen, checked against the level
/// (a position whose controller is gone is left out of the readout).
fn known_controllers(
    host: &mut dyn ServerHost,
    keys: &Keys,
    level: LevelRef,
    controllers: &Mutex<Controllers>,
) -> Vec<Pos> {
    let mut guard = controllers
        .lock()
        .expect("the controller list lock is held for one vist");
    // The readout is the one consumer of this index, so it is also where the index is cleaned:
    // a position whose controller is gone is dropped instead of filtered again on every run,
    // and the set stays at its cap (the review finding).
    let live = |pos: Pos| level.block(&*host, pos) == Some(keys.controller);
    guard.prune(level, live);
    guard.known(level)
}

/// The refusal `/xnet info` answers a source that is too far away and not an operator.
const INFO_DENIED: &str =
    "XNet: too far away; stand within 6 blocks of the network or ask an operator";

/// Whether the command's source may read a network at `pos`: the acting player needs reach or
/// an operator's permission (the network is private: its positions and filters are the owner's
/// build), and the console may read anywhere.
fn diagnostics_allowed(host: &dyn ServerHost, pos: Pos) -> bool {
    let Some(player) = host.players().acting_player() else {
        return diagnostics_permitted(None, None);
    };
    let permission = player.permission_level(host);
    let distance = player
        .entity()
        .distance_to_sqr(
            host,
            [
                f64::from(pos[0]) + 0.5,
                f64::from(pos[1]) + 0.5,
                f64::from(pos[2]) + 0.5,
            ],
        )
        .map(f64::sqrt);
    diagnostics_permitted(permission, distance)
}

/// A controller's readout: its network, its channels and every connector side.
fn controller_info(host: &mut dyn ServerHost, keys: &Keys, level: LevelRef, pos: Pos) {
    let Some(controller) = controller_at(host, keys, level, pos) else {
        feedback(host, "XNet: no controller there.");
        return;
    };
    let network = controller_network(host, keys, level, pos).unwrap_or_default();
    let enabled = controller
        .channels()
        .filter(|(_, channel)| channel.kind() != ChannelType::None)
        .count();
    feedback(
        host,
        &format!(
            "XNet controller at {},{},{}: network {}, {enabled} channel(s) enabled",
            pos[0],
            pos[1],
            pos[2],
            members(&network)
        ),
    );
    for (index, channel) in controller.channels() {
        if channel.kind() == ChannelType::None {
            continue;
        }
        feedback(
            host,
            &format!(
                "XNet controller at {},{},{}: channel {index} {}, {} connector(s), {}",
                pos[0],
                pos[1],
                pos[2],
                channel.kind().name(),
                channel.entries().len(),
                channel.mode().name()
            ),
        );
        for entry in channel.entries() {
            feedback(
                host,
                &format!(
                    "XNet controller at {},{},{}: channel {index} {}",
                    pos[0],
                    pos[1],
                    pos[2],
                    settings_line(entry)
                ),
            );
        }
    }
}

/// A connector's readout: its network, and every controller channel that uses it.
fn connector_info(
    host: &mut dyn ServerHost,
    keys: &Keys,
    level: LevelRef,
    pos: Pos,
    controllers: &Mutex<Controllers>,
) {
    let network = network_of(host, keys, level, pos);
    feedback(
        host,
        &format!(
            "XNet connector at {},{},{}: network {}",
            pos[0],
            pos[1],
            pos[2],
            members(&network)
        ),
    );
    let mut listed = 0;
    for controller_pos in known_controllers(host, keys, level, controllers) {
        let Some(controller) = controller_at(host, keys, level, controller_pos) else {
            continue;
        };
        for (index, channel) in controller.channels() {
            for entry in channel.entries() {
                if entry.connector != pos {
                    continue;
                }
                listed += 1;
                feedback(
                    host,
                    &format!(
                        "XNet connector at {},{},{}: channel {index} {}",
                        pos[0],
                        pos[1],
                        pos[2],
                        settings_line(entry)
                    ),
                );
            }
        }
    }
    if listed == 0 {
        feedback(
            host,
            &format!(
                "XNet connector at {},{},{}: no channel uses it",
                pos[0], pos[1], pos[2]
            ),
        );
    }
}

/// Declare `/xnet info` and the operator commands that edit a channel. `info` is
/// a readout for every player; the editors stand in for the controller screen,
/// which is a gap in this slice, so they are operator-only (level 2), like a
/// command that changes a machine's settings.
pub(crate) fn register_commands(event: &mut RegisterCommandsEvent<'_>) {
    event
        .commands
        .push(Command::new("xnet info <pos:block_pos>"));
    for usage in [
        "xnet channel <pos:block_pos> <channel:integer> <kind:word>",
        "xnet connector <pos:block_pos> <channel:integer> <connector:block_pos> <side:word> <mode:word>",
        "xnet remove <pos:block_pos> <channel:integer> <connector:block_pos> <side:word>",
        "xnet speed <pos:block_pos> <channel:integer> <connector:block_pos> <side:word> <ticks:integer>",
        "xnet priority <pos:block_pos> <channel:integer> <connector:block_pos> <side:word> <priority:integer>",
        "xnet stack <pos:block_pos> <channel:integer> <connector:block_pos> <side:word> <stack:word>",
        "xnet amount <pos:block_pos> <channel:integer> <connector:block_pos> <side:word> <amount:integer>",
        "xnet count <pos:block_pos> <channel:integer> <connector:block_pos> <side:word> <count:word>",
        "xnet mode <pos:block_pos> <channel:integer> <mode:word>",
        // The item is a `text` argument: an item id contains `:`, which a `word` argument ends at.
        "xnet filter <pos:block_pos> <channel:integer> <connector:block_pos> <side:word> <item:text>",
        "xnet unfilter <pos:block_pos> <channel:integer> <connector:block_pos> <side:word> <item:text>",
        "xnet blacklist <pos:block_pos> <channel:integer> <connector:block_pos> <side:word> <on:word>",
    ] {
        event.commands.push(Command::op(usage, 2));
    }
}

/// The `pos`-style argument of a command as a block position.
fn arg_pos(command: &CommandInvocation, name: &str) -> Option<Pos> {
    match command.argument(name)? {
        CommandArgumentValue::BlockPos(pos) => Some(*pos),
        _ => None,
    }
}

/// An `integer` argument of a command.
fn arg_int(command: &CommandInvocation, name: &str) -> Option<i32> {
    match command.argument(name)? {
        CommandArgumentValue::Integer(value) => Some(*value),
        _ => None,
    }
}

/// A `word` argument of a command.
fn arg_word<'a>(command: &'a CommandInvocation, name: &str) -> Option<&'a str> {
    match command.argument(name)? {
        CommandArgumentValue::String(word) => Some(word),
        _ => None,
    }
}

/// The four arguments every channel editor shares: the controller, its channel,
/// the connector and the side facing the container.
fn entry_args(command: &CommandInvocation) -> Option<(Pos, usize, Pos, usize)> {
    let index = usize::try_from(arg_int(command, "channel")?).ok()?;
    let side = face_named(arg_word(command, "side")?)?;
    Some((
        arg_pos(command, "pos")?,
        index,
        arg_pos(command, "connector")?,
        side,
    ))
}

/// The level a command acts in: the acting player's own.
fn acting_level(host: &dyn ServerHost) -> Option<LevelRef> {
    let player = host.players().acting_player()?;
    player.level(host)
}

/// Replace one channel of the controller at `pos` with what `edit` returns.
fn edit_channel(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: Pos,
    index: usize,
    edit: impl FnOnce(Channel) -> Option<Channel>,
) -> Result<Channel, &'static str> {
    let Some(keys) = keys(host) else {
        return Err("XNet: the blocks are not registered.");
    };
    let Some(controller) = controller_at(host, &keys, level, pos) else {
        return Err("XNet: no controller there.");
    };
    let Some(channel) = controller.channel(index) else {
        return Err("XNet: the channel is 0 to 7.");
    };
    let Some(updated) = edit(channel.clone()) else {
        return Err("XNet: that connector side is not on the channel.");
    };
    let next = controller.with_channel(index, updated.clone());
    let Some(entity) = host.block_entity(level, pos) else {
        return Err("XNet: no controller there.");
    };
    entity
        .merge_data(host, controller_data(&next))
        .map_err(|_| "XNet: the controller refused the settings.")?;
    Ok(updated)
}

/// Restart a channel's round-robin offset, as upstream's `update` does when the
/// channel's settings change.
fn reset_offset(controllers: &Mutex<Controllers>, level: LevelRef, pos: Pos, index: usize) {
    controllers
        .lock()
        .expect("the round-robin lock is held for one write")
        .remember_offset(level, pos, index, 0);
}

/// Run one `/xnet` command: the readout and the settings path that stands in for
/// the controller screen.
pub(crate) fn run_command(
    host: &mut dyn ServerHost,
    controllers: &Mutex<Controllers>,
    command: &CommandInvocation,
) -> Flow {
    let literals: Vec<&str> = command.literals.iter().map(String::as_str).collect();
    if !matches!(literals.first(), Some(&"xnet")) {
        return Flow::Continue;
    }
    let Some(level) = acting_level(host) else {
        feedback(host, "XNet: no level to act in.");
        return Flow::Cancel;
    };
    match literals.as_slice() {
        ["xnet", "info"] => {
            let Some(pos) = arg_pos(command, "pos") else {
                feedback(host, "Usage: /xnet info <x> <y> <z>");
                return Flow::Cancel;
            };
            // A network's positions and its filters are its owner's build, so the diagnostic is
            // a nearby read or an operator's, not a remote probe (the review finding).
            if !diagnostics_allowed(host, pos) {
                feedback(host, INFO_DENIED);
                return Flow::Cancel;
            }
            let block = keys(host).zip(level.block(host, pos));
            match block {
                Some((keys, block)) if block == keys.controller => {
                    controller_info(host, &keys, level, pos);
                }
                Some((keys, block)) if block == keys.connector => {
                    connector_info(host, &keys, level, pos, controllers);
                }
                _ => feedback(host, "XNet: no controller or connector at that position."),
            }
            Flow::Cancel
        }
        ["xnet", "channel"] => {
            let (Some(pos), Some(index), Some(kind)) = (
                arg_pos(command, "pos"),
                arg_int(command, "channel"),
                arg_word(command, "kind"),
            ) else {
                feedback(
                    host,
                    "Usage: /xnet channel <x> <y> <z> <channel> <none|item>",
                );
                return Flow::Cancel;
            };
            let Ok(index) = usize::try_from(index) else {
                feedback(host, "XNet: the channel is 0 to 7.");
                return Flow::Cancel;
            };
            let Some(kind) = ChannelType::named(kind) else {
                feedback(host, "XNet: the channel kind is none or item.");
                return Flow::Cancel;
            };
            // Changing the kind replaces the channel's settings, as the channel
            // editor does; naming the same kind keeps them.
            let edited = edit_channel(host, level, pos, index, |channel| {
                Some(if channel.kind() == kind {
                    channel
                } else {
                    Channel::new(kind)
                })
            });
            match edited {
                Ok(_) => {
                    // A new kind is new settings, so the offset restarts with them.
                    reset_offset(controllers, level, pos, index);
                    feedback(
                        host,
                        &format!("XNet: channel {index} is now {}", kind.name()),
                    )
                }
                Err(message) => feedback(host, message),
            }
            Flow::Cancel
        }
        ["xnet", "connector"] => {
            let Some((pos, index, connector, side)) = entry_args(command) else {
                feedback(
                    host,
                    "Usage: /xnet connector <x> <y> <z> <channel> <cx> <cy> <cz> <side> <ins|ext>",
                );
                return Flow::Cancel;
            };
            let Some(mode) = arg_word(command, "mode").and_then(TransferMode::named) else {
                feedback(host, "XNet: the mode is ins or ext.");
                return Flow::Cancel;
            };
            let entry = Entry {
                connector,
                side,
                settings: ItemSettings::new(mode),
            };
            let edited = edit_channel(host, level, pos, index, |channel| {
                Some(channel.with_entry(entry))
            });
            match edited {
                Ok(_) => feedback(
                    host,
                    &format!(
                        "XNet: channel {index} uses {},{},{} {} as an {} side",
                        connector[0],
                        connector[1],
                        connector[2],
                        FACE_NAMES[side],
                        mode_word(mode)
                    ),
                ),
                Err(message) => feedback(host, message),
            }
            Flow::Cancel
        }
        ["xnet", "remove"] => {
            let Some((pos, index, connector, side)) = entry_args(command) else {
                feedback(
                    host,
                    "Usage: /xnet remove <x> <y> <z> <channel> <cx> <cy> <cz> <side>",
                );
                return Flow::Cancel;
            };
            let edited = edit_channel(host, level, pos, index, |channel| {
                channel.entry(connector, side)?;
                Some(channel.without(connector, side))
            });
            match edited {
                Ok(_) => feedback(
                    host,
                    &format!(
                        "XNet: channel {index} no longer uses {},{},{} {}",
                        connector[0], connector[1], connector[2], FACE_NAMES[side]
                    ),
                ),
                Err(message) => feedback(host, message),
            }
            Flow::Cancel
        }
        ["xnet", "speed"] => {
            let Some((pos, index, connector, side)) = entry_args(command) else {
                feedback(
                    host,
                    "Usage: /xnet speed <x> <y> <z> <channel> <cx> <cy> <cz> <side> <ticks>",
                );
                return Flow::Cancel;
            };
            let Some(ticks) = arg_int(command, "ticks").and_then(|ticks| u32::try_from(ticks).ok())
            else {
                feedback(host, "XNet: the interval is 5 to 200 ticks.");
                return Flow::Cancel;
            };
            let edited = edit_channel(host, level, pos, index, |channel| {
                let settings = channel.entry(connector, side)?.clone().with_interval(ticks);
                Some(channel.with_entry(Entry {
                    connector,
                    side,
                    settings,
                }))
            });
            match edited {
                Ok(channel) => {
                    let shown = channel
                        .entry(connector, side)
                        .map_or(0, ItemSettings::interval);
                    feedback(
                        host,
                        &format!(
                            "XNet: {},{},{} {} moves {SINGLE_AMOUNT} item every {shown} ticks",
                            connector[0], connector[1], connector[2], FACE_NAMES[side]
                        ),
                    );
                }
                Err(message) => feedback(host, message),
            }
            Flow::Cancel
        }
        ["xnet", "priority"] => {
            let Some((pos, index, connector, side)) = entry_args(command) else {
                feedback(
                    host,
                    "Usage: /xnet priority <x> <y> <z> <channel> <cx> <cy> <cz> <side> <priority>",
                );
                return Flow::Cancel;
            };
            let Some(priority) = arg_int(command, "priority") else {
                feedback(host, "XNet: the priority is a whole number.");
                return Flow::Cancel;
            };
            let edited = edit_channel(host, level, pos, index, |channel| {
                let settings = channel
                    .entry(connector, side)?
                    .clone()
                    .with_priority(priority);
                Some(channel.with_entry(Entry {
                    connector,
                    side,
                    settings,
                }))
            });
            match edited {
                Ok(_) => feedback(
                    host,
                    &format!(
                        "XNet: {},{},{} {} has priority {priority}",
                        connector[0], connector[1], connector[2], FACE_NAMES[side]
                    ),
                ),
                Err(message) => feedback(host, message),
            }
            Flow::Cancel
        }
        ["xnet", "stack"] => {
            let Some((pos, index, connector, side)) = entry_args(command) else {
                feedback(
                    host,
                    "Usage: /xnet stack <x> <y> <z> <channel> <cx> <cy> <cz> <side> <single|stack|count>",
                );
                return Flow::Cancel;
            };
            let Some(stack) = arg_word(command, "stack").and_then(StackMode::named) else {
                feedback(host, "XNet: the stack mode is single, stack or count.");
                return Flow::Cancel;
            };
            let edited = edit_channel(host, level, pos, index, |channel| {
                let settings = channel
                    .entry(connector, side)?
                    .clone()
                    .with_stack_mode(stack);
                Some(channel.with_entry(Entry {
                    connector,
                    side,
                    settings,
                }))
            });
            match edited {
                Ok(channel) => {
                    let shown = channel
                        .entry(connector, side)
                        .map_or_else(String::new, amount_phrase);
                    feedback(
                        host,
                        &format!(
                            "XNet: {},{},{} {} moves {shown} per operation ({})",
                            connector[0],
                            connector[1],
                            connector[2],
                            FACE_NAMES[side],
                            stack.name()
                        ),
                    );
                }
                Err(message) => feedback(host, message),
            }
            Flow::Cancel
        }
        ["xnet", "amount"] => {
            let Some((pos, index, connector, side)) = entry_args(command) else {
                feedback(
                    host,
                    "Usage: /xnet amount <x> <y> <z> <channel> <cx> <cy> <cz> <side> <amount>",
                );
                return Flow::Cancel;
            };
            let Some(amount) =
                arg_int(command, "amount").and_then(|amount| u32::try_from(amount).ok())
            else {
                feedback(host, "XNet: the amount is 1 to 64 items.");
                return Flow::Cancel;
            };
            let edited = edit_channel(host, level, pos, index, |channel| {
                let settings = channel
                    .entry(connector, side)?
                    .clone()
                    .with_extract_amount(amount);
                Some(channel.with_entry(Entry {
                    connector,
                    side,
                    settings,
                }))
            });
            match edited {
                Ok(channel) => {
                    let shown = channel
                        .entry(connector, side)
                        .map_or(0, ItemSettings::extract_amount);
                    feedback(
                        host,
                        &format!(
                            "XNet: {},{},{} {} takes {shown} items per operation in count mode",
                            connector[0], connector[1], connector[2], FACE_NAMES[side]
                        ),
                    );
                }
                Err(message) => feedback(host, message),
            }
            Flow::Cancel
        }
        ["xnet", "count"] => {
            let Some((pos, index, connector, side)) = entry_args(command) else {
                feedback(
                    host,
                    "Usage: /xnet count <x> <y> <z> <channel> <cx> <cy> <cz> <side> <count|->",
                );
                return Flow::Cancel;
            };
            let Some(word) = arg_word(command, "count") else {
                feedback(host, "XNet: a whole number, or - to clear the count.");
                return Flow::Cancel;
            };
            let count = match word {
                "-" | "none" => None,
                number => match number.parse::<u32>() {
                    Ok(count) => Some(count),
                    Err(_) => {
                        feedback(host, "XNet: a whole number, or - to clear the count.");
                        return Flow::Cancel;
                    }
                },
            };
            let edited = edit_channel(host, level, pos, index, |channel| {
                let settings = channel.entry(connector, side)?.clone().with_count(count);
                Some(channel.with_entry(Entry {
                    connector,
                    side,
                    settings,
                }))
            });
            match edited {
                Ok(channel) => {
                    let shown = channel
                        .entry(connector, side)
                        .and_then(ItemSettings::count)
                        .map_or_else(
                            || "has no count".to_string(),
                            |count| format!("keeps {count}"),
                        );
                    feedback(
                        host,
                        &format!(
                            "XNet: {},{},{} {} {shown}",
                            connector[0], connector[1], connector[2], FACE_NAMES[side]
                        ),
                    );
                }
                Err(message) => feedback(host, message),
            }
            Flow::Cancel
        }
        ["xnet", "mode"] => {
            let (Some(pos), Some(index)) = (arg_pos(command, "pos"), arg_int(command, "channel"))
            else {
                feedback(
                    host,
                    "Usage: /xnet mode <x> <y> <z> <channel> <priority|roundrobin>",
                );
                return Flow::Cancel;
            };
            let Ok(index) = usize::try_from(index) else {
                feedback(host, "XNet: the channel is 0 to 7.");
                return Flow::Cancel;
            };
            let Some(mode) = arg_word(command, "mode").and_then(ChannelMode::named) else {
                feedback(host, "XNet: the channel mode is priority or roundrobin.");
                return Flow::Cancel;
            };
            let edited = edit_channel(host, level, pos, index, |channel| {
                Some(channel.with_mode(mode))
            });
            match edited {
                Ok(_) => {
                    // Upstream's `update` resets the offset when the channel's settings
                    // change, so the next operation starts at the first destination.
                    reset_offset(controllers, level, pos, index);
                    feedback(
                        host,
                        &format!("XNet: channel {index} distributes by {}", mode.name()),
                    );
                }
                Err(message) => feedback(host, message),
            }
            Flow::Cancel
        }
        ["xnet", "unfilter"] => {
            let Some((pos, index, connector, side)) = entry_args(command) else {
                feedback(
                    host,
                    "Usage: /xnet unfilter <x> <y> <z> <channel> <cx> <cy> <cz> <side> <item>",
                );
                return Flow::Cancel;
            };
            let Some(item) = arg_word(command, "item") else {
                feedback(host, "XNet: name the item to take out of the filter.");
                return Flow::Cancel;
            };
            let item = item.to_string();
            let edited = edit_channel(host, level, pos, index, |channel| {
                let settings = channel
                    .entry(connector, side)?
                    .clone()
                    .without_filter(&item);
                Some(channel.with_entry(Entry {
                    connector,
                    side,
                    settings,
                }))
            });
            match edited {
                Ok(channel) => feedback(
                    host,
                    &format!(
                        "XNet: {},{},{} {} filter {}",
                        connector[0],
                        connector[1],
                        connector[2],
                        FACE_NAMES[side],
                        filter_line(channel.entry(connector, side))
                    ),
                ),
                Err(message) => feedback(host, message),
            }
            Flow::Cancel
        }
        ["xnet", "filter"] => {
            let Some((pos, index, connector, side)) = entry_args(command) else {
                feedback(
                    host,
                    "Usage: /xnet filter <x> <y> <z> <channel> <cx> <cy> <cz> <side> <item|->",
                );
                return Flow::Cancel;
            };
            let Some(item) = arg_word(command, "item") else {
                feedback(host, "XNet: name an item, or - to clear the filter.");
                return Flow::Cancel;
            };
            let item = item.to_string();
            let edited = edit_channel(host, level, pos, index, |channel| {
                let settings = channel.entry(connector, side)?.clone();
                let settings = if item == "-" {
                    settings.without_filters()
                } else {
                    settings.with_filter(&item)
                };
                Some(channel.with_entry(Entry {
                    connector,
                    side,
                    settings,
                }))
            });
            match edited {
                Ok(channel) => feedback(
                    host,
                    &format!(
                        "XNet: {},{},{} {} filter {}",
                        connector[0],
                        connector[1],
                        connector[2],
                        FACE_NAMES[side],
                        filter_line(channel.entry(connector, side))
                    ),
                ),
                Err(message) => feedback(host, message),
            }
            Flow::Cancel
        }
        ["xnet", "blacklist"] => {
            let Some((pos, index, connector, side)) = entry_args(command) else {
                feedback(
                    host,
                    "Usage: /xnet blacklist <x> <y> <z> <channel> <cx> <cy> <cz> <side> <on|off>",
                );
                return Flow::Cancel;
            };
            let Some(on) = arg_word(command, "on").and_then(|word| match word {
                "on" => Some(true),
                "off" => Some(false),
                _ => None,
            }) else {
                feedback(host, "XNet: the filter is on or off.");
                return Flow::Cancel;
            };
            let edited = edit_channel(host, level, pos, index, |channel| {
                let settings = channel.entry(connector, side)?.clone().with_blacklist(on);
                Some(channel.with_entry(Entry {
                    connector,
                    side,
                    settings,
                }))
            });
            match edited {
                Ok(_) => feedback(
                    host,
                    &format!(
                        "XNet: {},{},{} {} filter is now a {}",
                        connector[0],
                        connector[1],
                        connector[2],
                        FACE_NAMES[side],
                        if on { "blacklist" } else { "whitelist" }
                    ),
                ),
                Err(message) => feedback(host, message),
            }
            Flow::Cancel
        }
        _ => Flow::Continue,
    }
}

/// A connector side's filter as the readout words it: `all` when nothing is
/// filtered, the whitelist, or `not …` for a blacklist.
fn filter_line(settings: Option<&ItemSettings>) -> String {
    match settings {
        Some(settings) if !settings.filters().is_empty() => settings.filters().join(" "),
        _ => "all".to_string(),
    }
}
