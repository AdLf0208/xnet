// XNet slice 1 — the item channel's pure logic (the standalone crate lib,
// injected into the game as `crate::game::xnet`). No game access lives here.
//
// Upstream a controller owns eight channels; a channel of type `item` holds
// connector/side pairs whose settings say whether they insert into or extract
// from the inventory on that side, how fast, at what priority and through which
// filter. A channel's tick walks its extractors and hands each operation's stack
// to the channel's inserters, highest priority first.

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
    /// [`Channel::save`]): mode, interval, priority, `w`/`b`, then the filters.
    fn save(&self) -> String {
        let mut fields = vec![
            self.mode.name().to_string(),
            self.interval.to_string(),
            self.priority.to_string(),
            if self.blacklist { "b" } else { "w" }.to_string(),
        ];
        fields.extend(self.filters.iter().cloned());
        fields.join(",")
    }

    /// The settings a saved field list wrote, or `None` when it is damaged.
    fn load(fields: &[&str]) -> Option<Self> {
        let [mode, interval, priority, list, filters @ ..] = fields else {
            return None;
        };
        Some(ItemSettings {
            mode: TransferMode::named(mode)?,
            interval: interval.parse::<u32>().ok()?.clamp(MIN_INTERVAL, MAX_INTERVAL),
            priority: priority.parse::<i32>().ok()?,
            blacklist: match *list {
                "b" => true,
                "w" => false,
                _ => return None,
            },
            filters: filters.iter().map(|filter| (*filter).to_string()).collect(),
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

/// One controller channel: its kind and its connector sides. Upstream's
/// `ChannelInfo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    kind: ChannelType,
    entries: Vec<Entry>,
}

impl Channel {
    /// An unused channel of `kind` with no connector sides.
    pub const fn new(kind: ChannelType) -> Self {
        Channel {
            kind,
            entries: Vec::new(),
        }
    }

    pub const fn kind(&self) -> ChannelType {
        self.kind
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
        self.entries.retain(|entry| entry.key() != (connector, side));
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
        Channel { kind, entries }
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(ItemSettings::new(TransferMode::Extract).interval(), DEFAULT_INTERVAL);
        assert_eq!(ItemSettings::new(TransferMode::Extract).with_interval(20).interval(), 20);
        assert_eq!(ItemSettings::new(TransferMode::Extract).with_interval(13).interval(), 10);
        assert_eq!(ItemSettings::new(TransferMode::Extract).with_interval(0).interval(), MIN_INTERVAL);
        assert_eq!(ItemSettings::new(TransferMode::Extract).with_interval(4).interval(), MIN_INTERVAL);
        assert_eq!(ItemSettings::new(TransferMode::Extract).with_interval(9_999).interval(), MAX_INTERVAL);
        assert_eq!(ItemSettings::new(TransferMode::Extract).with_interval(7).interval(), 5);
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
        assert!(twice.without_filter("minecraft:stone").matches("minecraft:dirt"));

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
        assert_eq!(channel.entries().len(), 2, "two sides of one connector are two entries");
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
        assert_eq!(controller.channel(0).map(Channel::kind), Some(ChannelType::None));
        assert_eq!(controller.channel(MAX_CHANNELS - 1).map(Channel::kind), Some(ChannelType::None));
        assert_eq!(controller.channel(MAX_CHANNELS), None);
        controller = controller.with_channel(2, Channel::new(ChannelType::Item));
        assert_eq!(controller.channel(2).map(Channel::kind), Some(ChannelType::Item));
        // Out of range is ignored rather than panicking.
        controller = controller.with_channel(MAX_CHANNELS, Channel::new(ChannelType::Item));
        assert_eq!(controller.channel(MAX_CHANNELS), None);
        controller = controller.with_channel(2, Channel::new(ChannelType::None));
        assert_eq!(controller.channel(2).map(Channel::kind), Some(ChannelType::None));
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
}
