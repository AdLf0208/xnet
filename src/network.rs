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
    Channel, ChannelType, Controller, Entry, FACE_COUNT, FACE_NAMES, ItemSettings, MAX_CHANNELS,
    Pos, SINGLE_AMOUNT, TransferMode, destination_order, face_named, is_operation_tick, opposite,
    phase_of, step,
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

    /// Every controller of `level` seen so far, in position order.
    fn known(&self, level: LevelRef) -> Vec<Pos> {
        self.known
            .get(&level)
            .map(|positions| positions.iter().copied().collect())
            .unwrap_or_default()
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

/// The two keys one channel saves under: its kind, and its connector sides.
fn channel_keys(index: usize) -> (String, String) {
    (format!("ch{index}.type"), format!("ch{index}"))
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
        let (type_key, entries_key) = channel_keys(index);
        let Some(kind) = text(nbt, &type_key).and_then(|name| ChannelType::named(&name)) else {
            continue;
        };
        if kind == ChannelType::None {
            continue;
        }
        let saved = text(nbt, &entries_key).unwrap_or_default();
        controller = controller.with_channel(index, Channel::load(kind, &saved));
    }
    controller
}

/// The data that sets every channel of `controller` in its block entity, in
/// merge form: an unused channel is written as `none`, which the reader skips,
/// so a merge can never leave a channel the player removed behind.
fn controller_data(controller: &Controller) -> Compound {
    let mut data = Compound::new();
    for index in 0..MAX_CHANNELS {
        let (type_key, entries_key) = channel_keys(index);
        let (kind, saved) = match controller.channel(index) {
            Some(channel) => (channel.kind(), channel.save()),
            None => (ChannelType::None, String::new()),
        };
        data.insert(type_key.as_str().into(), Tag::String(kind.name().into()));
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

/// The contiguous run of cables and connectors `start` belongs to, in a stable
/// order. One colour only in this slice, so adjacency is the whole rule
/// upstream's `ChunkBlob.createCableSegment` applies to one colour.
fn network_of(host: &dyn Host, keys: &Keys, level: LevelRef, start: Pos) -> Vec<Pos> {
    let mut seen: BTreeSet<Pos> = BTreeSet::new();
    let mut queue = vec![start];
    seen.insert(start);
    while let Some(pos) = queue.pop() {
        for face in 0..FACE_COUNT {
            let beside = step(pos, face);
            if is_wire(host, keys, level, beside) && seen.insert(beside) {
                queue.push(beside);
            }
        }
    }
    seen.into_iter().collect()
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

/// Run one extractor's operation: the first matching stack in the inventory on
/// its side, handed to the channel's inserters in priority order, one item
/// (`StackMode.SINGLE`). An item no destination accepts stays where it is, and
/// the next matching slot is tried, as upstream's `tickItemHandler` does.
fn run_operation(host: &mut dyn ServerHost, level: LevelRef, entries: &[Entry], index: usize) {
    let source = &entries[index];
    let from = step(source.connector, source.side);
    let from_face = Direction::ALL[opposite(source.side)];
    let order = destination_order(entries, source);
    if order.is_empty() {
        return;
    }
    let Some(source_container) = level.capability(host, &ITEMS, from, Some(from_face)) else {
        return;
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
        for &destination in &order {
            let target = &entries[destination];
            if !target.settings.matches(&item) {
                continue;
            }
            let to = step(target.connector, target.side);
            let to_face = Direction::ALL[opposite(target.side)];
            let Some(target_container) = level.capability(host, &ITEMS, to, Some(to_face)) else {
                continue;
            };
            if move_one(host, &source_container, slot, &resource, &target_container) {
                return;
            }
        }
    }
}

/// One item from `slot` of `from` into `to`, in one transaction: `true` when it
/// moved. A destination that refuses it leaves the item in its own slot, which
/// is upstream's `ItemHandlerHelper.insertItem` contract; a source that cannot
/// take it back aborts the transaction, so nothing leaves at all.
fn move_one(
    host: &mut dyn ServerHost,
    from: &Capability<dyn ResourceHandler<ItemResource>>,
    slot: usize,
    resource: &ItemResource,
    to: &Capability<dyn ResourceHandler<ItemResource>>,
) -> bool {
    host.transaction(|host, tx| {
        let taken = from.extract(host, tx, slot, resource, SINGLE_AMOUNT);
        if taken == 0 {
            return Err(());
        }
        if to.insert_any(host, tx, resource, taken) == taken {
            return Ok(true);
        }
        if from.insert(host, tx, slot, resource, taken) == taken {
            Ok(false)
        } else {
            Err(())
        }
    })
    .unwrap_or(false)
}

/// Run every extractor of one channel that is due this tick and still on the
/// controller's network.
fn run_channel(
    host: &mut dyn ServerHost,
    level: LevelRef,
    network: &[Pos],
    channel: &Channel,
    game_time: u64,
) {
    let entries = channel.entries();
    if channel.is_empty() {
        return;
    }
    for (index, entry) in entries.iter().enumerate() {
        if entry.settings.mode() != TransferMode::Extract
            || !network.contains(&entry.connector)
            || !is_operation_tick(
                game_time,
                phase_of(entry.connector, entry.side),
                u64::from(entry.settings.interval()),
            )
        {
            continue;
        }
        run_operation(host, level, entries, index);
    }
}

/// After the level's tick: run every item channel of every controller queued
/// this tick.
pub(crate) fn run_controllers(host: &mut dyn ServerHost, level: LevelRef, queued: BTreeSet<Pos>) {
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
        let Some(network) = controller_network(host, &keys, level, controller_pos) else {
            continue;
        };
        // Read the channels out before the transfer borrows the game mutably.
        let Some(channels) = controller_at(host, &keys, level, controller_pos).map(|controller| {
            controller
                .channels()
                .filter(|(_, channel)| channel.kind() == ChannelType::Item)
                .map(|(_, channel)| channel.clone())
                .collect::<Vec<Channel>>()
        }) else {
            continue;
        };
        for channel in channels {
            run_channel(host, level, &network, &channel, game_time);
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

/// A connector side's settings as one readout line: side, mode, rate, priority
/// and filter.
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
        "{},{},{} {} {}, {SINGLE_AMOUNT} item every {} ticks, priority {}, filter {filter}",
        entry.connector[0],
        entry.connector[1],
        entry.connector[2],
        FACE_NAMES[entry.side],
        mode_word(entry.settings.mode()),
        entry.settings.interval(),
        entry.settings.priority(),
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
    let seen = controllers
        .lock()
        .expect("the controller list lock is held for one copy")
        .known(level);
    seen.into_iter()
        .filter(|&pos| level.block(host, pos) == Some(keys.controller))
        .collect()
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
                "XNet controller at {},{},{}: channel {index} {}, {} connector(s)",
                pos[0],
                pos[1],
                pos[2],
                channel.kind().name(),
                channel.entries().len()
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
        "xnet filter <pos:block_pos> <channel:integer> <connector:block_pos> <side:word> <item:word>",
        "xnet unfilter <pos:block_pos> <channel:integer> <connector:block_pos> <side:word> <item:word>",
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
                Ok(_) => feedback(
                    host,
                    &format!("XNet: channel {index} is now {}", kind.name()),
                ),
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
