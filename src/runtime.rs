// XNet slice 1 — the item channel's pure logic (the standalone crate lib,
// injected into the game as `crate::game::xnet`). No game access lives here.
//
// Upstream a controller owns eight channels; a channel of type `item` holds
// connector/side pairs whose settings say whether they insert into or extract
// from the inventory on that side, how fast, at what priority and through which
// filter. A channel's tick walks its extractors and hands each operation's stack
// to the channel's inserters, highest priority first.

use std::collections::BTreeSet;

/// One of a block's six sides, in vanilla's `Direction.values()` order.
pub type Face = usize;

/// A block position, as the glue reads it from the level.
pub type Pos = [i32; 3];

pub const FACE_COUNT: usize = 6;

pub const DOWN: Face = 0;
pub const UP: Face = 1;
pub const NORTH: Face = 2;
pub const SOUTH: Face = 3;
pub const WEST: Face = 4;
pub const EAST: Face = 5;

/// `Direction.values()`'s names, which the commands and the readout use.
pub const FACE_NAMES: [&str; FACE_COUNT] = ["down", "up", "north", "south", "west", "east"];

/// Upstream `ChannelInfo.MAX_CHANNELS`: a controller edits eight channels.
pub const MAX_CHANNELS: usize = 8;

/// The screen offers the tick interval in fives (`Constants.SPEEDS` 10..200,
/// `ADVANCED_SPEEDS` 5..200) and stores it divided by five, so only multiples
/// of five exist.
pub const INTERVAL_STEP: u32 = 5;

/// The fastest operation an advanced connector can be set to.
pub const MIN_INTERVAL: u32 = 5;

/// The slowest operation the screen offers.
pub const MAX_INTERVAL: u32 = 200;

/// `ItemConnectorSettings.speed = 2` with the screen's `speed * 5`: a fresh
/// connector moves one item every ten ticks.
pub const DEFAULT_INTERVAL: u32 = 10;

/// `StackMode.SINGLE`, the default: one item per operation.
pub const SINGLE_AMOUNT: u32 = 1;

/// The most one operation carries: `tickItemHandler`'s `maxamount` of 64, which
/// caps `StackMode.STACK`'s `getMaxStackSize()` and `COUNT`'s amount.
pub const OPERATION_CAPACITY: u32 = 64;

/// `ItemConnectorSettings.priority = 0`.
pub const DEFAULT_PRIORITY: i32 = 0;

/// The face's unit vector, as `Direction.normal()` gives it.
pub const fn face_normal(face: Face) -> [i32; 3] {
    match face {
        DOWN => [0, -1, 0],
        UP => [0, 1, 0],
        NORTH => [0, 0, -1],
        SOUTH => [0, 0, 1],
        WEST => [-1, 0, 0],
        EAST => [1, 0, 0],
        _ => [0, 0, 0],
    }
}

/// The position one step from `pos` through `face`.
pub const fn step(pos: Pos, face: Face) -> Pos {
    let [dx, dy, dz] = face_normal(face);
    [pos[0] + dx, pos[1] + dy, pos[2] + dz]
}

/// The side across a block from `face`.
pub const fn opposite(face: Face) -> Face {
    match face {
        DOWN => UP,
        UP => DOWN,
        NORTH => SOUTH,
        SOUTH => NORTH,
        WEST => EAST,
        EAST => WEST,
        _ => face,
    }
}

/// The face a command names (`down`..`east`), if it names one.
pub fn face_named(name: &str) -> Option<Face> {
    FACE_NAMES.iter().position(|known| *known == name)
}

/// What a channel carries. Upstream's `IChannelType` registry, reduced to the
/// two kinds this slice knows: `none` (an unused channel) and `item`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChannelType {
    /// `noneChannelType`: the channel is unused.
    #[default]
    None,
    /// `xnet.item`: items move through it.
    Item,
}

impl ChannelType {
    /// The kind a command or a saved setting names, if it names one.
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "none" => Some(Self::None),
            "item" => Some(Self::Item),
            _ => None,
        }
    }

    /// The kind's command/readout name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Item => "item",
        }
    }
}

/// How much one operation carries: upstream's `StackMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StackMode {
    /// `SINGLE`: one item.
    #[default]
    Single,
    /// `STACK`: the whole slot, up to [`OPERATION_CAPACITY`].
    Stack,
    /// `COUNT`: the connector's `extractAmount`.
    Count,
}

impl StackMode {
    /// The mode a command or a saved setting names, if it names one.
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "single" => Some(Self::Single),
            "stack" => Some(Self::Stack),
            "count" => Some(Self::Count),
            _ => None,
        }
    }

    /// The mode's command/readout name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Stack => "stack",
            Self::Count => "count",
        }
    }
}

/// How a channel picks among its inserters: upstream's `ChannelMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChannelMode {
    /// `PRIORITY`: the highest priority first, every operation.
    #[default]
    Priority,
    /// `ROUNDROBIN`: each operation starts one destination further on.
    RoundRobin,
}

impl ChannelMode {
    /// The mode a command or a saved setting names, if it names one.
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "priority" => Some(Self::Priority),
            "roundrobin" => Some(Self::RoundRobin),
            _ => None,
        }
    }

    /// The mode's command/readout name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Priority => "priority",
            Self::RoundRobin => "roundrobin",
        }
    }
}

/// What a connector does with the inventory on its side: upstream's
/// `InsExtMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransferMode {
    /// Inserts what the channel carries into its side's inventory.
    Insert,
    /// Pulls from its side's inventory into the channel.
    Extract,
}

impl TransferMode {
    /// The mode a command or a saved setting names, if it names one. Upstream
    /// spells the two `ins` and `ext`.
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "ins" => Some(Self::Insert),
            "ext" => Some(Self::Extract),
            _ => None,
        }
    }

    /// The mode's command/readout name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Insert => "ins",
            Self::Extract => "ext",
        }
    }
}

/// One connector side's item settings: upstream `ItemConnectorSettings`, of
/// which this slice keeps the mode, the interval, the priority and the filter
/// (the rest — stack mode, count, extract mode, the tag/meta/component toggles —
/// are gaps recorded in `notes.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemSettings {
    mode: TransferMode,
    interval: u32,
    priority: i32,
    blacklist: bool,
    filters: Vec<String>,
    stack_mode: StackMode,
    extract_amount: u32,
    count: Option<u32>,
}

impl ItemSettings {
    /// Fresh settings for one connector side, with upstream's defaults.
    pub fn new(mode: TransferMode) -> Self {
        ItemSettings {
            mode,
            interval: DEFAULT_INTERVAL,
            priority: DEFAULT_PRIORITY,
            blacklist: false,
            filters: Vec::new(),
            stack_mode: StackMode::Single,
            extract_amount: SINGLE_AMOUNT,
            count: None,
        }
    }

    /// The same settings at another tick interval, clamped to what the screen
    /// offers and rounded down to a multiple of five, as `update` stores it.
    pub fn with_interval(self, ticks: u32) -> Self {
        let clamped = ticks.clamp(MIN_INTERVAL, MAX_INTERVAL);
        ItemSettings {
            interval: (clamped / INTERVAL_STEP) * INTERVAL_STEP,
            ..self
        }
    }

    /// The same settings with another stack mode.
    pub fn with_stack_mode(self, stack_mode: StackMode) -> Self {
        ItemSettings { stack_mode, ..self }
    }

    /// The same settings with another `StackMode.COUNT` amount, as a whole number of
    /// items from 1 to [`OPERATION_CAPACITY`] (upstream's `maxamount` caps it anyway).
    pub fn with_extract_amount(self, amount: u32) -> Self {
        ItemSettings {
            extract_amount: amount.clamp(SINGLE_AMOUNT, OPERATION_CAPACITY),
            ..self
        }
    }

    /// The same settings with a stock-keeping count (an extractor keeps that many,
    /// an inserter stocks that many), or with none.
    pub fn with_count(self, count: Option<u32>) -> Self {
        ItemSettings { count, ..self }
    }

    /// The same settings at another priority.
    pub fn with_priority(self, priority: i32) -> Self {
        ItemSettings { priority, ..self }
    }

    /// The same settings with `item` in the filter, added once.
    pub fn with_filter(self, item: &str) -> Self {
        if self.filters.iter().any(|known| known == item) {
            return self;
        }
        let mut filters = self.filters;
        filters.push(item.to_string());
        ItemSettings { filters, ..self }
    }

    /// The same settings without `item` in the filter.
    pub fn without_filter(self, item: &str) -> Self {
        let filters = self
            .filters
            .into_iter()
            .filter(|known| known != item)
            .collect();
        ItemSettings { filters, ..self }
    }

    /// The same settings with an empty filter, which accepts everything.
    pub fn without_filters(self) -> Self {
        ItemSettings {
            filters: Vec::new(),
            ..self
        }
    }

    /// The same settings with the filter list inverted (a blacklist).
    pub fn with_blacklist(self, blacklist: bool) -> Self {
        ItemSettings { blacklist, ..self }
    }

    pub const fn mode(&self) -> TransferMode {
        self.mode
    }

    /// The ticks between two operations of this connector.
    pub const fn interval(&self) -> u32 {
        self.interval
    }

    pub const fn priority(&self) -> i32 {
        self.priority
    }

    pub const fn stack_mode(&self) -> StackMode {
        self.stack_mode
    }

    /// The `StackMode.COUNT` amount, 1 to [`OPERATION_CAPACITY`].
    pub const fn extract_amount(&self) -> u32 {
        self.extract_amount
    }

    /// The stock-keeping count, when one is set.
    pub const fn count(&self) -> Option<u32> {
        self.count
    }

    /// How many items one operation takes from a slot holding `in_slot` of this
    /// item, when the source holds `matching_total` of it in all. `SINGLE` takes
    /// one, `STACK` the whole slot, `COUNT` the configured amount — each capped by
    /// the slot, by [`OPERATION_CAPACITY`] (upstream's `maxamount`) and, with a
    /// stock-keeping count, by what may leave above it.
    pub fn take_amount(&self, in_slot: u32, matching_total: u32) -> u32 {
        let wanted = match self.stack_mode {
            StackMode::Single => SINGLE_AMOUNT,
            StackMode::Stack => OPERATION_CAPACITY,
            StackMode::Count => self.extract_amount,
        }
        .min(OPERATION_CAPACITY);
        let take = wanted.min(in_slot);
        match self.count {
            Some(count) => take.min(matching_total.saturating_sub(count)),
            None => take,
        }
    }

    /// How many of `amount` a destination may still receive, when it already holds
    /// `matching_total` of this item: an inserter's count stocks it to that many.
    pub fn accept_amount(&self, amount: u32, matching_total: u32) -> u32 {
        match self.count {
            Some(count) => amount.min(count.saturating_sub(matching_total)),
            None => amount,
        }
    }

    pub const fn blacklist(&self) -> bool {
        self.blacklist
    }

    /// The filter's item ids, in the order they were added.
    pub fn filters(&self) -> Vec<&str> {
        self.filters.iter().map(String::as_str).collect()
    }

    /// Whether `item` passes this connector's filter: an empty filter accepts
    /// everything, and a blacklist inverts the list (`getMatcher`).
    pub fn matches(&self, item: &str) -> bool {
        if self.filters.is_empty() {
            return true;
        }
        let listed = self.filters.iter().any(|known| known == item);
        listed != self.blacklist
    }

    /// This side's settings as one comma-separated field list (see
    /// [`Channel::save`]): mode, interval, priority, `w`/`b`, stack mode, the
    /// `COUNT` amount, the stock count (`-` for none), then the filters.
    fn save(&self) -> String {
        let mut fields = vec![
            self.mode.name().to_string(),
            self.interval.to_string(),
            self.priority.to_string(),
            if self.blacklist { "b" } else { "w" }.to_string(),
            self.stack_mode.name().to_string(),
            self.extract_amount.to_string(),
            self.count
                .map_or_else(|| "-".to_string(), |count| count.to_string()),
        ];
        fields.extend(self.filters.iter().cloned());
        fields.join(",")
    }

    /// The settings a saved field list wrote, or `None` when it is damaged.
    ///
    /// A 0.2.1 save wrote the four settings fields and then the filters, with no
    /// stack mode; a saved stack list is recognised by its fifth field naming one,
    /// so an older world keeps its connector settings instead of losing them.
    fn load(fields: &[&str]) -> Option<Self> {
        let [mode, interval, priority, list, rest @ ..] = fields else {
            return None;
        };
        let (stack_mode, extract_amount, count, filters) = match rest {
            [stack, amount, count_field, filters @ ..] => match StackMode::named(stack) {
                Some(stack_mode) => (
                    stack_mode,
                    amount
                        .parse::<u32>()
                        .ok()?
                        .clamp(SINGLE_AMOUNT, OPERATION_CAPACITY),
                    match *count_field {
                        "-" => None,
                        number => Some(number.parse::<u32>().ok()?),
                    },
                    filters,
                ),
                None => (StackMode::Single, SINGLE_AMOUNT, None, rest),
            },
            _ => (StackMode::Single, SINGLE_AMOUNT, None, rest),
        };
        Some(ItemSettings {
            mode: TransferMode::named(mode)?,
            interval: interval
                .parse::<u32>()
                .ok()?
                .clamp(MIN_INTERVAL, MAX_INTERVAL),
            priority: priority.parse::<i32>().ok()?,
            blacklist: match *list {
                "b" => true,
                "w" => false,
                _ => return None,
            },
            filters: filters.iter().map(|filter| (*filter).to_string()).collect(),
            stack_mode,
            extract_amount,
            count,
        })
    }
}

/// One connector side on a channel: upstream's `SidedConsumer` and its
/// `IConnectorSettings` — the side is the face that looks at the inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub connector: Pos,
    pub side: Face,
    pub settings: ItemSettings,
}

impl Entry {
    /// The connector/side this entry serves, which is how a channel keys it.
    const fn key(&self) -> (Pos, Face) {
        (self.connector, self.side)
    }

    fn save(&self) -> String {
        format!(
            "{},{},{},{},{}",
            self.connector[0],
            self.connector[1],
            self.connector[2],
            FACE_NAMES[self.side],
            self.settings.save()
        )
    }

    fn load(fields: &[&str]) -> Option<Self> {
        let [x, y, z, side, settings @ ..] = fields else {
            return None;
        };
        Some(Entry {
            connector: [x.parse().ok()?, y.parse().ok()?, z.parse().ok()?],
            side: face_named(side)?,
            settings: ItemSettings::load(settings)?,
        })
    }
}

/// One controller channel: its kind, its distribution mode and its connector
/// sides. Upstream's `ChannelInfo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    kind: ChannelType,
    mode: ChannelMode,
    entries: Vec<Entry>,
}

impl Channel {
    /// An unused channel of `kind` with no connector sides.
    pub const fn new(kind: ChannelType) -> Self {
        Channel {
            kind,
            mode: ChannelMode::Priority,
            entries: Vec::new(),
        }
    }

    pub const fn kind(&self) -> ChannelType {
        self.kind
    }

    /// How this channel picks among its inserters.
    pub const fn mode(&self) -> ChannelMode {
        self.mode
    }

    /// The same channel with another distribution mode. The round-robin offset is
    /// session state (held by the caller), so it restarts with the mode, as
    /// upstream's `update` resets it.
    pub fn with_mode(self, mode: ChannelMode) -> Self {
        Channel { mode, ..self }
    }

    /// The connector sides on this channel, in the order they were added.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The settings of one connector side, if it is on this channel.
    pub fn entry(&self, connector: Pos, side: Face) -> Option<&ItemSettings> {
        self.entries
            .iter()
            .find(|entry| entry.key() == (connector, side))
            .map(|entry| &entry.settings)
    }

    /// The channel with `entry` added, or with its settings replaced when that
    /// connector side is already on it.
    pub fn with_entry(mut self, entry: Entry) -> Self {
        match self
            .entries
            .iter_mut()
            .find(|known| known.key() == entry.key())
        {
            Some(known) => *known = entry,
            None => self.entries.push(entry),
        }
        self
    }

    /// The channel without that connector side.
    pub fn without(mut self, connector: Pos, side: Face) -> Self {
        self.entries
            .retain(|entry| entry.key() != (connector, side));
        self
    }

    /// Whether no connector side is on this channel.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The channel's connector sides as one saved string: entries separated by
    /// `;`, an entry's fields by `,`, its filters after the eighth field. A
    /// resource id holds none of those characters, so the format cannot be
    /// ambiguous.
    pub fn save(&self) -> String {
        self.entries
            .iter()
            .map(Entry::save)
            .collect::<Vec<String>>()
            .join(";")
    }

    /// The channel of `kind` a saved string wrote. A damaged entry is dropped
    /// rather than read as something else, so a hand-edited or older save loads
    /// what it can and never panics.
    pub fn load(kind: ChannelType, saved: &str) -> Self {
        let entries = saved
            .split(';')
            .filter(|entry| !entry.is_empty())
            .filter_map(|entry| {
                let fields: Vec<&str> = entry.split(',').collect();
                Entry::load(&fields)
            })
            .collect();
        Channel {
            kind,
            mode: ChannelMode::Priority,
            entries,
        }
    }
}

/// One controller: eight channels, as upstream's `ControllerData`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Controller {
    channels: [Channel; MAX_CHANNELS],
}

impl Controller {
    /// A controller with eight unused channels.
    pub fn new() -> Self {
        Controller {
            channels: [const { Channel::new(ChannelType::None) }; MAX_CHANNELS],
        }
    }

    /// Channel `index`, or `None` past the eighth.
    pub fn channel(&self, index: usize) -> Option<&Channel> {
        self.channels.get(index)
    }

    /// Every channel with its index.
    pub fn channels(&self) -> impl Iterator<Item = (usize, &Channel)> {
        self.channels.iter().enumerate()
    }

    /// The controller with channel `index` replaced by `channel`, entries and
    /// all; past the eighth it is ignored.
    pub fn with_channel(mut self, index: usize, channel: Channel) -> Self {
        if let Some(slot) = self.channels.get_mut(index) {
            *slot = channel;
        }
        self
    }
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}

/// An extractor's operation phase: the sum of its block coordinates and side,
/// which spreads a level's connectors over the interval instead of firing them
/// all on one tick (upstream ticks each channel on a saved `delay` counter,
/// which a block-entity write inside the tick would lose at this pin). It may be
/// negative; [`is_operation_tick`] takes it modulo the interval.
pub fn phase_of(pos: Pos, side: Face) -> i64 {
    i64::from(pos[0]) + i64::from(pos[1]) + i64::from(pos[2]) + side as i64
}

/// Whether this tick is the extractor's operation tick: one every `interval`
/// ticks, offset by the extractor's [`phase_of`].
pub fn is_operation_tick(game_time: u64, phase: i64, interval: u64) -> bool {
    if interval == 0 {
        return false;
    }
    let elapsed = i64::try_from(game_time).unwrap_or(i64::MAX);
    let interval = i64::try_from(interval).unwrap_or(i64::MAX);
    elapsed.saturating_add(phase).rem_euclid(interval) == 0
}

/// The order one operation tries the channel's destinations in: the inserters,
/// highest priority first, then block position and side so the same channel
/// always gives the same order, and never the extracting side itself.
pub fn destination_order(entries: &[Entry], source: &Entry) -> Vec<usize> {
    let mut order: Vec<usize> = (0..entries.len())
        .filter(|&index| {
            entries[index].settings.mode() == TransferMode::Insert
                && entries[index].key() != source.key()
        })
        .collect();
    order.sort_by(|&left, &right| {
        let (left, right) = (&entries[left], &entries[right]);
        right
            .settings
            .priority()
            .cmp(&left.settings.priority())
            .then_with(|| left.key().cmp(&right.key()))
    });
    order
}

/// The order one operation tries a channel's destinations in, for its mode:
/// [`ChannelMode::Priority`] always starts at the highest priority, while
/// [`ChannelMode::RoundRobin`] starts `offset` places further on and wraps
/// (upstream's `insertStack` starts at `(j + roundRobinOffset) % consumersSize`).
#[must_use]
pub fn destination_plan(
    entries: &[Entry],
    source: &Entry,
    mode: ChannelMode,
    offset: usize,
) -> Vec<usize> {
    let priority = destination_order(entries, source);
    match mode {
        ChannelMode::Priority => priority,
        ChannelMode::RoundRobin if !priority.is_empty() => {
            let mut plan = priority;
            let start = offset % plan.len();
            plan.rotate_left(start);
            plan
        }
        ChannelMode::RoundRobin => priority,
    }
}

/// The round-robin offset after one operation that put items into `used`
/// destinations: upstream advances it once per destination that accepted
/// something, and never otherwise.
#[must_use]
pub const fn next_offset(offset: usize, destinations: usize, used: usize) -> usize {
    if destinations == 0 {
        0
    } else {
        offset.saturating_add(used) % destinations
    }
}

/// One cable walk's visit budget: the most cells a single traversal reads. A cable network is/// player-built, so its size is not ours to trust; the walk stops at the budget and the caller
/// sees the part it reached (the review finding). 4,096 cells is far past a usable network and
/// bounds one walk's block reads.
pub const MAX_NETWORK_VISITS: usize = 4_096;

/// The most controllers one level remembers for `/xnet info`'s readout. A player-built
/// controller set is bounded work for the level tick's prune, and a set that has grown past it
/// keeps its lowest positions (a stable order across ticks).
pub const MAX_KNOWN_CONTROLLERS: usize = 256;

/// `Commands.LEVEL_GAMEMASTERS`: an operator may diagnose any network.
pub const DIAGNOSTIC_PERMISSION: u8 = 2;

/// The reach that makes a diagnostic the ordinary interaction path (the server's block reach,
/// rounded up: 4.5 standing, 5 in creative).
pub const DIAGNOSTIC_REACH: f64 = 6.0;

/// The contiguous run of wires `start` belongs to, in visit order, reading at most `budget`
/// cells. `is_wire` answers whether a position is part of the network.
#[must_use]
pub fn walk_network(start: Pos, budget: usize, is_wire: impl Fn(Pos) -> bool) -> Vec<Pos> {
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    if budget == 0 {
        return order;
    }
    seen.insert(start);
    order.push(start);
    let mut index = 0;
    while index < order.len() {
        let current = order[index];
        index += 1;
        for face in 0..FACE_COUNT {
            if order.len() >= budget {
                return order;
            }
            let beside = step(current, face);
            if is_wire(beside) && seen.insert(beside) {
                order.push(beside);
            }
        }
    }
    order
}

/// Whether a saved connector is still on the live network.
#[must_use]
pub fn on_network(network: &BTreeSet<Pos>, connector: Pos) -> bool {
    network.contains(&connector)
}

/// Drop the remembered controllers whose block is gone, then keep the set at `cap`. `live`
/// answers whether `pos` still holds a controller; the level's own block read is the caller's.
pub fn prune_known(known: &mut BTreeSet<Pos>, live: impl Fn(Pos) -> bool, cap: usize) {
    known.retain(|pos| live(*pos));
    while known.len() > cap {
        let Some(last) = known.last().copied() else {
            break;
        };
        known.remove(&last);
    }
}

/// Whether a command source may read a network diagnostic: an operator anywhere, the console
/// (no player behind it) anywhere, everyone else only within interaction reach of the target.
/// An unreadable distance is not close.
#[must_use]
pub fn diagnostics_permitted(permission: Option<u8>, distance: Option<f64>) -> bool {
    match permission {
        None => true,
        Some(level) if level >= DIAGNOSTIC_PERMISSION => true,
        Some(_) => {
            distance.is_some_and(|distance| distance.is_finite() && distance <= DIAGNOSTIC_REACH)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn faces_follow_vanillas_direction_order() {
        assert_eq!(FACE_NAMES, ["down", "up", "north", "south", "west", "east"]);
        for (index, name) in FACE_NAMES.iter().enumerate() {
            assert_eq!(face_named(name), Some(index), "{name}");
        }
        assert_eq!(face_named("sideways"), None);
        assert_eq!(face_normal(NORTH), [0, 0, -1]);
        assert_eq!(face_normal(EAST), [1, 0, 0]);
        assert_eq!(face_normal(SOUTH), [0, 0, 1]);
        assert_eq!(opposite(WEST), EAST);
        assert_eq!(step([4, 5, 6], UP), [4, 6, 6]);
        assert_eq!(step([4, 5, 6], WEST), [3, 5, 6]);
    }

    #[test]
    fn the_channel_count_and_the_default_rate_come_from_the_jar() {
        // `ChannelInfo.MAX_CHANNELS`.
        assert_eq!(MAX_CHANNELS, 8);
        // `ItemConnectorSettings.speed = 2`, and the screen shows `speed * 5`
        // ticks (`Constants.SPEEDS` starts at 10): one item every 10 ticks.
        assert_eq!(DEFAULT_INTERVAL, 10);
        // `Constants.SPEEDS` runs 10..200 in fives; the advanced connector's
        // list (`ADVANCED_SPEEDS`) starts at 5.
        assert_eq!(INTERVAL_STEP, 5);
        assert_eq!(MIN_INTERVAL, 5);
        assert_eq!(MAX_INTERVAL, 200);
        // `StackMode.SINGLE`, the default: one item per operation.
        assert_eq!(SINGLE_AMOUNT, 1);
        // `ItemConnectorSettings.priority = 0`.
        assert_eq!(DEFAULT_PRIORITY, 0);
    }

    #[test]
    fn an_operation_runs_every_interval_ticks_offset_by_position() {
        // The extractor at (1, 100, 0) on its west side: 1 + 100 + 0 + 4 == 105.
        let phase = phase_of([1, 100, 0], WEST);
        assert_eq!(phase, 105);
        assert!(is_operation_tick(5, phase, DEFAULT_INTERVAL as u64));
        assert!(!is_operation_tick(6, phase, DEFAULT_INTERVAL as u64));
        assert!(is_operation_tick(15, phase, DEFAULT_INTERVAL as u64));
        // Two sides of the same connector do not fire together.
        assert_ne!(phase_of([1, 100, 0], EAST), phase);
        // A slower connector runs on its own cadence: phase 15, interval 20.
        assert!(is_operation_tick(5, 15, 20));
        assert!(!is_operation_tick(15, 15, 20));
        assert!(is_operation_tick(25, 15, 20));
        // Negative coordinates (the End's arena is at the origin, but a world's
        // negative half must not panic or shift).
        assert_eq!(phase_of([-3, 64, -7], DOWN), 54);
        assert!(is_operation_tick(0, 54, 6));
        assert!(is_operation_tick(0, -10, 10));
        assert!(!is_operation_tick(0, -10, 6));
    }

    #[test]
    fn the_interval_is_clamped_and_rounded_as_the_screen_stores_it() {
        // `ItemConnectorSettings.update` divides the screen's ticks by five, so
        // only multiples of five exist and anything else rounds down.
        assert_eq!(
            ItemSettings::new(TransferMode::Extract).interval(),
            DEFAULT_INTERVAL
        );
        assert_eq!(
            ItemSettings::new(TransferMode::Extract)
                .with_interval(20)
                .interval(),
            20
        );
        assert_eq!(
            ItemSettings::new(TransferMode::Extract)
                .with_interval(13)
                .interval(),
            10
        );
        assert_eq!(
            ItemSettings::new(TransferMode::Extract)
                .with_interval(0)
                .interval(),
            MIN_INTERVAL
        );
        assert_eq!(
            ItemSettings::new(TransferMode::Extract)
                .with_interval(4)
                .interval(),
            MIN_INTERVAL
        );
        assert_eq!(
            ItemSettings::new(TransferMode::Extract)
                .with_interval(9_999)
                .interval(),
            MAX_INTERVAL
        );
        assert_eq!(
            ItemSettings::new(TransferMode::Extract)
                .with_interval(7)
                .interval(),
            5
        );
    }

    #[test]
    fn a_filter_matches_everything_until_one_is_set() {
        let open = ItemSettings::new(TransferMode::Extract);
        assert!(open.matches("minecraft:stone"));
        assert!(open.matches("minecraft:dirt"));

        let only_stone = open.with_filter("minecraft:stone");
        assert_eq!(only_stone.filters(), ["minecraft:stone"]);
        assert!(only_stone.matches("minecraft:stone"));
        assert!(!only_stone.matches("minecraft:dirt"));

        // Adding the same item twice keeps one entry; removing it reopens the
        // filter.
        let twice = only_stone.clone().with_filter("minecraft:stone");
        assert_eq!(twice.filters().len(), 1);
        assert!(
            twice
                .without_filter("minecraft:stone")
                .matches("minecraft:dirt")
        );

        // A blacklist inverts the list, as upstream's `blacklist` toggle does.
        let blacklist = only_stone.clone().with_blacklist(true);
        assert!(!blacklist.matches("minecraft:stone"));
        assert!(blacklist.matches("minecraft:dirt"));
        assert_eq!(blacklist.with_blacklist(false), only_stone);

        // Emptying the list reopens the filter whatever it held.
        let both = only_stone.clone().with_filter("minecraft:dirt");
        assert_eq!(both.filters(), ["minecraft:stone", "minecraft:dirt"]);
        assert!(both.clone().without_filters().filters().is_empty());
        assert!(both.without_filters().matches("minecraft:dirt"));
    }

    #[test]
    fn destinations_are_the_inserters_by_priority_and_never_the_source() {
        let source = Entry {
            connector: [1, 100, 0],
            side: WEST,
            settings: ItemSettings::new(TransferMode::Extract),
        };
        let entries = [
            source.clone(),
            Entry {
                connector: [4, 100, 0],
                side: EAST,
                settings: ItemSettings::new(TransferMode::Insert),
            },
            Entry {
                connector: [9, 100, 0],
                side: UP,
                settings: ItemSettings::new(TransferMode::Insert).with_priority(5),
            },
            Entry {
                connector: [7, 100, 0],
                side: DOWN,
                settings: ItemSettings::new(TransferMode::Insert).with_priority(5),
            },
            Entry {
                connector: [6, 100, 0],
                side: NORTH,
                settings: ItemSettings::new(TransferMode::Extract),
            },
        ];
        // Highest priority first, then block position and side, so the order is
        // the same on every tick and in every world load.
        assert_eq!(destination_order(&entries, &source), vec![3, 2, 1]);
        // Read in another order, the same destinations come out in the same order.
        let mut reversed = entries;
        reversed.reverse();
        assert_eq!(destination_order(&reversed, &source), vec![1, 2, 3]);
        // A channel of one extractor has nowhere to put anything.
        assert!(destination_order(std::slice::from_ref(&source), &source).is_empty());
    }

    #[test]
    fn a_channel_keeps_one_entry_per_connector_side() {
        let west = Entry {
            connector: [1, 100, 0],
            side: WEST,
            settings: ItemSettings::new(TransferMode::Extract),
        };
        let east = Entry {
            connector: [1, 100, 0],
            side: EAST,
            settings: ItemSettings::new(TransferMode::Insert),
        };
        let mut channel = Channel::new(ChannelType::Item);
        assert!(channel.is_empty());
        channel = channel.with_entry(west.clone());
        channel = channel.with_entry(east.clone());
        assert_eq!(
            channel.entries().len(),
            2,
            "two sides of one connector are two entries"
        );
        // The same connector and side replaces its settings instead of adding.
        let faster = Entry {
            settings: west.settings.clone().with_interval(20),
            ..west.clone()
        };
        channel = channel.with_entry(faster.clone());
        assert_eq!(channel.entries().len(), 2);
        assert_eq!(channel.entry([1, 100, 0], WEST), Some(&faster.settings));
        assert_eq!(channel.entry([1, 100, 0], NORTH), None);
        channel = channel.without([1, 100, 0], WEST);
        assert_eq!(channel.entry([1, 100, 0], WEST), None);
        channel = channel.without([1, 100, 0], EAST);
        assert!(channel.is_empty());
    }

    #[test]
    fn a_controller_has_eight_channels_and_ignores_the_ninth() {
        let mut controller = Controller::new();
        assert_eq!(
            controller.channel(0).map(Channel::kind),
            Some(ChannelType::None)
        );
        assert_eq!(
            controller.channel(MAX_CHANNELS - 1).map(Channel::kind),
            Some(ChannelType::None)
        );
        assert_eq!(controller.channel(MAX_CHANNELS), None);
        controller = controller.with_channel(2, Channel::new(ChannelType::Item));
        assert_eq!(
            controller.channel(2).map(Channel::kind),
            Some(ChannelType::Item)
        );
        // Out of range is ignored rather than panicking.
        controller = controller.with_channel(MAX_CHANNELS, Channel::new(ChannelType::Item));
        assert_eq!(controller.channel(MAX_CHANNELS), None);
        controller = controller.with_channel(2, Channel::new(ChannelType::None));
        assert_eq!(
            controller.channel(2).map(Channel::kind),
            Some(ChannelType::None)
        );
        // A whole channel replaces one, entries and all; the ninth is ignored.
        let item = Channel::new(ChannelType::Item).with_entry(Entry {
            connector: [2, 100, 0],
            side: UP,
            settings: ItemSettings::new(TransferMode::Insert),
        });
        controller = controller.with_channel(3, item.clone());
        assert_eq!(controller.channel(3), Some(&item));
        controller = controller.with_channel(MAX_CHANNELS, item);
        assert_eq!(controller.channel(MAX_CHANNELS), None);
    }

    #[test]
    fn the_channel_settings_round_trip_through_the_saved_string() {
        let extract = Entry {
            connector: [1, 100, 0],
            side: WEST,
            settings: ItemSettings::new(TransferMode::Extract)
                .with_interval(20)
                .with_filter("minecraft:stone")
                .with_filter("minecraft:dirt")
                .with_blacklist(true),
        };
        let insert = Entry {
            connector: [4, 100, -2],
            side: EAST,
            settings: ItemSettings::new(TransferMode::Insert).with_priority(3),
        };
        let channel = Channel::new(ChannelType::Item)
            .with_entry(extract.clone())
            .with_entry(insert.clone());
        let controller = Controller::new().with_channel(0, channel);
        let saved = controller.channel(0).expect("channel 0").save();
        let restored = Channel::load(ChannelType::Item, &saved);
        assert_eq!(restored.entries().len(), 2);
        assert_eq!(restored.entry([1, 100, 0], WEST), Some(&extract.settings));
        assert_eq!(restored.entry([4, 100, -2], EAST), Some(&insert.settings));

        // An empty channel and a damaged string are both a channel with no
        // entries, never a panic.
        assert_eq!(Channel::new(ChannelType::Item).save(), "");
        assert!(Channel::load(ChannelType::Item, "").is_empty());
        assert!(Channel::load(ChannelType::Item, "garbage;1,2").is_empty());
        assert_eq!(
            Channel::load(ChannelType::Item, "1,100,0,west,ext,10,0,w,minecraft:stone")
                .entries()
                .len(),
            1
        );
        assert!(Channel::load(ChannelType::Item, "1,100,0,west,nonsense,10,0,w").is_empty());
    }

    #[test]
    fn channel_and_mode_names_round_trip() {
        assert_eq!(ChannelType::named("item"), Some(ChannelType::Item));
        assert_eq!(ChannelType::named("none"), Some(ChannelType::None));
        assert_eq!(ChannelType::named("energy"), None);
        assert_eq!(ChannelType::Item.name(), "item");
        assert_eq!(ChannelType::None.name(), "none");
        assert_eq!(TransferMode::named("ins"), Some(TransferMode::Insert));
        assert_eq!(TransferMode::named("ext"), Some(TransferMode::Extract));
        assert_eq!(TransferMode::named("insert"), None);
        assert_eq!(TransferMode::Insert.name(), "ins");
        assert_eq!(TransferMode::Extract.name(), "ext");
    }

    /// The review finding: a cable network is player-built and unbounded, so the walk stops at
    /// its visit budget instead of reading every cell of a sprawling net. The control case
    /// walks a whole small net, so the budget is not simply "nothing".
    #[test]
    fn the_cable_walk_stops_at_its_budget() {
        // A line of 1,000 cables east of the start.
        let is_wire = |pos: Pos| (0..=1_000).contains(&pos[0]) && pos[1] == 0 && pos[2] == 0;
        let whole = walk_network([0, 0, 0], 2_000, is_wire);
        assert_eq!(whole.len(), 1_001, "a small net is walked whole");
        assert_eq!(whole.first(), Some(&[0, 0, 0]));
        assert!(whole.contains(&[1_000, 0, 0]));

        let bounded = walk_network([0, 0, 0], 10, is_wire);
        assert_eq!(bounded.len(), 10, "the budget caps the visited cells");
        assert!(
            !bounded.contains(&[1_000, 0, 0]),
            "the far end is not reached"
        );
        assert!(walk_network([0, 0, 0], 0, is_wire).is_empty(), "no visits");
        // A start that is not a cable still answers itself, as upstream's blob does.
        assert_eq!(walk_network([5, 5, 5], 10, |_| false), vec![[5, 5, 5]]);
    }

    /// The review finding: a saved destination is only a destination while its connector is
    /// still on the controller's live network.
    #[test]
    fn a_destination_counts_only_while_it_is_on_the_network() {
        let network: BTreeSet<Pos> = [[0, 0, 0], [1, 0, 0]].into_iter().collect();
        assert!(on_network(&network, [1, 0, 0]));
        assert!(!on_network(&network, [2, 0, 0]), "a removed connector");
    }

    /// The review finding: the known-controller set kept every position it ever saw. A prune
    /// drops the ones whose block is gone and keeps the set at the cap.
    #[test]
    fn the_known_controller_set_is_pruned_and_capped() {
        let mut known: BTreeSet<Pos> = (0..5).map(|x| [x, 0, 0]).collect();
        // Only the first three still hold a controller.
        prune_known(&mut known, |pos| pos[0] < 3, 8);
        assert_eq!(known.len(), 3);
        assert!(known.iter().all(|pos| pos[0] < 3), "the dead are gone");

        // No cap has to bite a live set.
        let mut capped: BTreeSet<Pos> = (0..10).map(|x| [x, 0, 0]).collect();
        prune_known(&mut capped, |_| true, 4);
        assert_eq!(capped.len(), 4, "the set stays bounded");
        assert!(capped.iter().any(|pos| pos[0] == 0), "the order is stable");
    }

    /// Slice 2: `StackMode` says how much one operation carries (`fetchItem`'s `s`,
    /// capped by the `maxamount` of 64).
    #[test]
    fn a_stack_mode_says_how_much_one_operation_carries() {
        let single = ItemSettings::new(TransferMode::Extract);
        assert_eq!(single.stack_mode(), StackMode::Single);
        assert_eq!(single.extract_amount(), SINGLE_AMOUNT);
        assert_eq!(single.count(), None);
        assert_eq!(single.take_amount(64, 64), 1);

        // STACK takes the whole slot, up to the operation cap of 64.
        let stack = single.clone().with_stack_mode(StackMode::Stack);
        assert_eq!(stack.take_amount(64, 64), 64);
        assert_eq!(
            stack.take_amount(20, 20),
            20,
            "never more than the slot holds"
        );
        assert_eq!(stack.take_amount(99, 99), OPERATION_CAPACITY);

        // COUNT takes exactly the configured amount, capped by the slot and the cap.
        let counted = single
            .clone()
            .with_stack_mode(StackMode::Count)
            .with_extract_amount(16);
        assert_eq!(counted.take_amount(64, 64), 16);
        assert_eq!(counted.take_amount(10, 10), 10);
        assert_eq!(counted.extract_amount(), 16);

        // The amount is a whole number of items, 1..=the operation cap.
        assert_eq!(single.clone().with_extract_amount(0).extract_amount(), 1);
        assert_eq!(
            single.clone().with_extract_amount(999).extract_amount(),
            OPERATION_CAPACITY
        );

        assert_eq!(StackMode::named("single"), Some(StackMode::Single));
        assert_eq!(StackMode::named("stack"), Some(StackMode::Stack));
        assert_eq!(StackMode::named("count"), Some(StackMode::Count));
        assert_eq!(StackMode::named("all"), None);
        assert_eq!(StackMode::Count.name(), "count");
    }

    /// Slice 2: `count` on an extractor keeps that many matching items in the source;
    /// on an inserter it stocks the destination to that many (`tickItemHandler`,
    /// `insertStack`).
    #[test]
    fn the_stock_count_limits_both_ends() {
        let keeping = ItemSettings::new(TransferMode::Extract).with_count(Some(5));
        assert_eq!(keeping.count(), Some(5));
        // 20 matching items in the source, 5 to keep: 15 may leave.
        assert_eq!(keeping.take_amount(20, 20), 1, "SINGLE still takes one");
        assert_eq!(
            keeping
                .clone()
                .with_stack_mode(StackMode::Stack)
                .take_amount(20, 20),
            15
        );
        // At or under the count nothing may leave, whatever the slot holds.
        assert_eq!(keeping.take_amount(5, 5), 0);
        assert_eq!(keeping.take_amount(4, 4), 0);
        assert_eq!(
            keeping
                .clone()
                .with_stack_mode(StackMode::Stack)
                .take_amount(5, 5),
            0
        );

        let stocking = ItemSettings::new(TransferMode::Insert).with_count(Some(5));
        assert_eq!(stocking.accept_amount(4, 3), 2, "5 stocked, 3 there: 2 fit");
        assert_eq!(stocking.accept_amount(64, 0), 5);
        assert_eq!(stocking.accept_amount(1, 5), 0, "already stocked");
        assert_eq!(stocking.accept_amount(1, 9), 0);
        // No count is no limit.
        assert_eq!(
            ItemSettings::new(TransferMode::Insert).accept_amount(64, 500),
            64
        );
        // A cleared count is a cleared limit.
        assert_eq!(stocking.clone().with_count(None).count(), None);
        assert_eq!(stocking.with_count(None).accept_amount(64, 500), 64);
    }

    /// Slice 2: `ROUNDROBIN` starts each operation one destination further on, while
    /// `PRIORITY` always starts at the highest (`insertStack`).
    #[test]
    fn roundrobin_rotates_the_destination_order() {
        let source = Entry {
            connector: [0, 0, 0],
            side: WEST,
            settings: ItemSettings::new(TransferMode::Extract),
        };
        let entries = [
            source.clone(),
            Entry {
                connector: [5, 0, 0],
                side: EAST,
                settings: ItemSettings::new(TransferMode::Insert),
            },
            Entry {
                connector: [6, 0, 0],
                side: EAST,
                settings: ItemSettings::new(TransferMode::Insert),
            },
            Entry {
                connector: [7, 0, 0],
                side: EAST,
                settings: ItemSettings::new(TransferMode::Insert),
            },
        ];
        // The priority order of the three inserters is 1, 2, 3.
        assert_eq!(
            destination_plan(&entries, &source, ChannelMode::Priority, 0),
            vec![1, 2, 3]
        );
        // PRIORITY ignores an offset: it always starts at the first destination.
        assert_eq!(
            destination_plan(&entries, &source, ChannelMode::Priority, 2),
            vec![1, 2, 3]
        );
        // ROUNDROBIN rotates by the offset and wraps.
        assert_eq!(
            destination_plan(&entries, &source, ChannelMode::RoundRobin, 1),
            vec![2, 3, 1]
        );
        assert_eq!(
            destination_plan(&entries, &source, ChannelMode::RoundRobin, 2),
            vec![3, 1, 2]
        );
        assert_eq!(
            destination_plan(&entries, &source, ChannelMode::RoundRobin, 3),
            vec![1, 2, 3]
        );
        assert_eq!(
            destination_plan(&entries, &source, ChannelMode::RoundRobin, 4),
            vec![2, 3, 1]
        );

        // The offset advances once per destination that accepted an item.
        assert_eq!(next_offset(0, 3, 1), 1);
        assert_eq!(next_offset(2, 3, 1), 0);
        assert_eq!(next_offset(0, 3, 2), 2);
        assert_eq!(
            next_offset(2, 3, 0),
            2,
            "a refused operation does not rotate"
        );
        assert_eq!(next_offset(0, 0, 0), 0);

        assert_eq!(ChannelMode::named("priority"), Some(ChannelMode::Priority));
        assert_eq!(
            ChannelMode::named("roundrobin"),
            Some(ChannelMode::RoundRobin)
        );
        assert_eq!(ChannelMode::named("round-robin"), None);
        assert_eq!(ChannelMode::RoundRobin.name(), "roundrobin");
    }

    /// Slice 2: the new settings survive a save, and a 0.2.1 save (no stack/amount/count
    /// fields) still loads with the defaults instead of being dropped.
    #[test]
    fn the_amount_settings_round_trip_and_old_saves_still_load() {
        let stacked = ItemSettings::new(TransferMode::Extract)
            .with_stack_mode(StackMode::Stack)
            .with_count(Some(5))
            .with_filter("minecraft:stone");
        let counted = ItemSettings::new(TransferMode::Insert)
            .with_stack_mode(StackMode::Count)
            .with_extract_amount(16)
            .with_count(Some(3));
        let channel = Channel::new(ChannelType::Item)
            .with_mode(ChannelMode::RoundRobin)
            .with_entry(Entry {
                connector: [1, 100, 0],
                side: WEST,
                settings: stacked.clone(),
            })
            .with_entry(Entry {
                connector: [4, 100, 0],
                side: EAST,
                settings: counted.clone(),
            });
        assert_eq!(channel.mode(), ChannelMode::RoundRobin);
        let restored = Channel::load(ChannelType::Item, &channel.save());
        assert_eq!(restored.entry([1, 100, 0], WEST), Some(&stacked));
        assert_eq!(restored.entry([4, 100, 0], EAST), Some(&counted));

        // The 0.2.1 shape: mode,interval,priority,w|b,filters…
        let old = Channel::load(ChannelType::Item, "1,100,0,west,ext,20,3,w,minecraft:stone");
        let settings = old.entry([1, 100, 0], WEST).expect("the old entry loads");
        assert_eq!(settings.interval(), 20);
        assert_eq!(settings.stack_mode(), StackMode::Single);
        assert_eq!(settings.extract_amount(), 1);
        assert_eq!(settings.count(), None);
        assert_eq!(settings.filters(), ["minecraft:stone"]);
        // A damaged new-shape entry is dropped, never read as something else.
        assert!(
            Channel::load(ChannelType::Item, "1,100,0,west,ext,10,0,w,stack,oops,-").is_empty()
        );
    }

    /// The review finding: `/xnet info` reported private network positions and filter settings
    /// at any coordinates. A source with reach may read its own network, an operator anywhere,
    /// and the console anywhere; an unreadable distance is refused.
    #[test]
    fn a_remote_diagnostic_needs_reach_or_permission() {
        assert!(!diagnostics_permitted(
            Some(0),
            Some(DIAGNOSTIC_REACH + 1.0)
        ));
        assert!(diagnostics_permitted(Some(0), Some(DIAGNOSTIC_REACH)));
        assert!(
            diagnostics_permitted(Some(0), Some(2.0)),
            "at the controller"
        );
        assert!(diagnostics_permitted(
            Some(DIAGNOSTIC_PERMISSION),
            Some(500.0)
        ));
        assert!(diagnostics_permitted(None, Some(500.0)), "the console");
        assert!(diagnostics_permitted(None, None), "the console");
        assert!(!diagnostics_permitted(Some(0), None), "unreadable");
        assert!(
            !diagnostics_permitted(Some(0), Some(f64::NAN)),
            "not a distance"
        );
    }
}
