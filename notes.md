# XNet → Horizon — slice 1: the item channel

> **Current package: 0.2.2** — slice 2, the item channel's amount and destination policy; see
> [`## Slice 2`](#slice-2--the-item-channels-amount-and-destination-policy-022) at the end.
>
> **Format: the SDK package (2026-11-18), version 0.2.0.** This port was migrated from the
> retired `.hnmod` container to one flat `.hmod` (content and code together) after Horizon's
> mod-API merge; see [`## Migration to the SDK format`](#migration-to-the-sdk-format) at the end
> of this file for the file map, the hook mapping, the ladder and the gaps. Everything above that
> section is the 0.1.0 slice record and stays valid as history.

Port of **XNet 1.21-7.0.7** (`McJtyMods/XNet`, **MIT**), Minecraft 1.21.1 NeoForge,
from the exact ATM10 jar `xnet-1.21-7.0.7.jar`, **611,577 B**, SHA-256
`0f393a4bff91a90e0665ec6d66ff649e45f9c33311dc038e4a5df0b154ea9d80`.

**Class + tier:** `content` / **`machines`** (`ports/ledger.csv`; provenance
`ports/_shared/source-map/atm10-sources.csv` row `xnet`, core, P0). Approach per Horizon's
`docs/modpacks.md`: a machines mod is a block entity plus a capability-style transfer on its own
tick, so this is a native slice with a content half (a controller, a cable, a connector, their
items, models, textures, lang, recipes, loot and tags) rather than a data-only carry.

**Licence:** **MIT → rank 0** (permissive). Read from the jar's own
`META-INF/neoforge.mods.toml` (`license="MIT License
https://github.com/McJtyMods/XNet/blob/1.21_neo/LICENCE"`); the upstream `LICENSE` at the analysed
commit is the MIT text, carried verbatim in [`LICENSE`](LICENSE) with
[`LICENSE-NOTICE.md`](LICENSE-NOTICE.md). Local-only until the maintainer approves publishing
(ARCHITECTURE.md §8); a published port travels under MIT with this licence and its complete
source.

**Why now:** the licence-first queue puts a rank-0 mechanics mod ahead of copyleft and
unknown-licence work. XNet is core (its required parents are `rftoolsbase`, our
`ported-partial`, and `mcjtylib`, a library mapped to the pin), and its item transfer needs no API
the pin does not already have: `native_container_slots` and `native_move_items` (API 48) are
exactly the "move one item, refuse it back" primitives a connector needs.

**Upstream analysis:** repository <https://github.com/McJtyMods/XNet>, branch `1.21_neo`, commit
`559b1d959811cf04021b51810f353d8c048cd65e` (`gradle.properties`: `version=1.21-7.0.7`; shallow
clone in `ports/_shared/upstream/xnet`, deleted after the findings were written). The behaviour
extracted — the network, the eight channels, the connector/side settings, the rate, the priority
order, the filters and the controller's power cost — is in [`FINDINGS.md`](FINDINGS.md) with the
file and line each number comes from. No Java was copied or translated and the jar was not
decompiled.

## What slice 1 does

- **The controller** (`xnet:controller`). A block entity that saves **eight channels**
  (`ChannelInfo.MAX_CHANNELS`). A channel is `none` or `item` and holds connector/side entries,
  each with a mode (insert/extract), a tick interval, a priority and a filter — upstream's
  `ItemConnectorSettings` reduced to the fields this slice operates.
- **The network.** The controller owns the contiguous run of cable and connector blocks through
  the **connector it touches**, exactly as upstream's `findNeighbourConnector` does (it scans its
  six neighbours for a `ConnectorBlock`). A controller beside a bare cable therefore has no
  network and its channels never run — one of the e2e controls.
- **The connector** (`xnet:connector`) is the block that reaches an inventory: a channel entry
  names the connector **and the side that faces the container**, which is upstream's
  `SidedConsumer`. One connector can serve two inventories on two sides.
- **The transfer.** After the level's tick each item channel runs its extractors that are due:
  the first matching stack in the inventory on the extractor's side is handed to the channel's
  inserters **highest priority first**, one item per operation (`StackMode.SINGLE`). An item no
  destination accepts stays where it is, and the next matching slot is tried, as upstream's
  `tickItemHandler` does.
- **The rate.** A fresh connector moves **one item every 10 ticks**: upstream's
  `ItemConnectorSettings.speed = 2` with the screen's `speed * 5` ticks (`Constants.SPEEDS` starts
  at 10). `/xnet speed` sets it from 5 to 200 ticks in fives, exactly the screen's list.
- **The filter.** Upstream's 18 ghost slots reduced to an item-id list: empty accepts everything,
  a list is a whitelist, and the `blacklist` toggle inverts it (`getMatcher`).
- **The readout and the settings path.** `/xnet info <x> <y> <z>` prints a controller's network,
  its channels and every connector side (side, mode, rate, priority, filter) or a connector's
  network and the channels that use it. `/xnet channel|connector|remove|speed|priority|filter|
  unfilter|blacklist` stand in for the controller screen, which is a gap; they are operator-only
  (level 2), like a command that changes a machine's settings.

### Deliberate deviations

| Upstream | Here | Why |
|---|---|---|
| The controller burns 1 RF/t per enabled channel and 2 RF per operation from a 100,000 FE buffer (`setup/Config.java:141-147`, `TileEntityController.tickServer:290-300`) | the controller runs **unpowered**; the item channel's rate is proved without the gate | the pin has no helper to charge another block entity (only the `block_entity_energy` capability hook, and no FE source in this slice), so a power gate here would make the channel untestable and unusable. The energy slice adds the buffer, the capability and the cost |
| A channel ticks on a saved `delay` counter (`ItemChannelSettings.tick:188-195`) | the phase is derived from the connector's position and side (`phase_of`) | a block-entity write inside its own tick is reverted at this pin (ARCHITECTURE.md §3). The rate is identical: one operation every `5 × speed` ticks |
| Five cable colours, which never join across colours | one colour: the network is the contiguous cable/connector run | a second block state and the colour on the item are content work, not behaviour; the slice's control ("no controller") is what the brief asked to prove |
| The controller has `facing` and `error` states and the cables have per-side `none/cable/block` arms and `waterlogged` | the controller has no state; the cable and connector join their neighbours through `connects_to` (a `chorus_plant` template) | the states are visual; the functional connection is the block adjacency the network walk uses. The error state marks a controller on more than one network, which this slice does not detect |
| `StackMode.STACK`/`COUNT`, `extractAmount`, `count` (stock keeping), `ExtractMode.RANDOM`/`ORDER`, `ROUNDROBIN` channel mode, the tag/meta/component filter toggles, redstone gates | `SINGLE`, first slot, `PRIORITY`, item-id filters | later slices; the data model keeps the entry per connector/side, so they are additive |
| The connector's enabled sides (`ConnectorData`, all six on) | not modelled | upstream gates the *visual* arm and `isConnectable`; a channel entry's side comes from the controller screen either way |
| The controller and connector screens, the advanced connector, energy/fluid/logic channels, routers, wireless routers, antennas, facades, redstone proxies | absent | recorded in the gap list below |

### API gaps used, and how

- **A block entity cannot be ticked from outside, and a write inside its tick is reverted** → the
  transfer runs in `level_tick_post` (API 23) and the settings are written from the command hook,
  never during the controller's own tick.
- **No network/graph helper** (ARCHITECTURE.md §3) → the port walks its own adjacency, as
  `moderndynamics` and `laserio` do; it is a dozen lines and needs no capability.
- **No helper to move a stack atomically** → `native_move_items` moves one item at a time and puts
  a refused item back, which is exactly `ItemHandlerHelper.insertItem`'s contract for
  `StackMode.SINGLE`.

## Slice boundary and gaps

Implemented: the controller, cable and connector blocks and their items with carried content; the
eight-channel model saved in the controller's block entity; the connector/side entries with mode,
interval, priority and filter; the network walk and the controller's connector rule; the item
operation (first matching slot, priority order, one item, refused items stay); the default
10-tick rate and `/xnet speed`; the whitelist/blacklist filter; `/xnet info`; persistence across a
relaunch; one `.hnmod` that installs both halves.

Not implemented (planned slices):

1. **Energy, fluid and logic channels**, and the controller's RF cost with them (the `block_entity_energy`
   capability exists at the pin; the missing piece is a way to charge a controller in a test).
2. **The advanced connector** (faster interval list from 5 ticks, 500,000 FE).
3. **The controller and connector screens**, including the channel copy/paste and the filter slots.
4. **The other four cable colours** and the routing colour with routers and wireless routers.
5. **Antennas, facades and the redstone proxy** (the jar's `antenna`, `antenna_base`,
   `antenna_dish`, `facade`, `redstone_proxy`, `redstone_proxy_upd`, `terminal`).
6. **`StackMode.STACK`/`COUNT`, `extractAmount`, `count` (stock keeping), `ExtractMode.RANDOM`/`ORDER`,
   `ROUNDROBIN` channel mode**, the tag/meta/component filter toggles and the redstone gates.
7. **The recipe unlock advancements** (`data/xnet/advancement/recipes/misc/*`, 38 files) — the
   recipes craft, the recipe book does not unlock them.
8. **The controller recipe's `rftoolsbase:machine_frame`** resolves only with our `rftoolsbase`
   port loaded; alone, that recipe cannot match (upstream has the same mandatory dependency).

Known limits: the controller has no `facing`/`error` state and the cable/connector draw no arm
towards the inventory they serve (only towards cables and the controller); the connector's model
is a plain 6-pixel core rather than upstream's custom cable loader model; the controller's screen
texture animates (carried `.mcmeta`) but the block has no screen to draw.

Cost of the network model: every tick, `level_tick_post` scans the level's block entities for
controllers, walks each controller's network (a flood fill over its cables) and decodes its eight
channels from NBT. Upstream keeps the network in a maintained blob and caches the channel
settings, so a network of hundreds of cables pays a walk per tick here. For this slice's sizes
(one controller, a handful of cables) that is nothing; a later slice should cache the walk per
controller and invalidate it when the network's blocks change.

## Upstream defects fixed

**None found in this slice.** What was checked and why each is not a defect:

- The `netcable`/`connector` loot tables drop the item matching the block's `color` state
  (`data/xnet/loot_table/blocks/netcable.json`); with one colour the port drops its item
  unconditionally — a simplification of an unreachable branch, not a fix.
- `ItemConnectorSettings.setSpeed` turns a stored `0` into `4` (`:307-309`), which the screen
  cannot produce (its list starts at 10, the advanced list at 5) — unreachable, so the port clamps
  to 5..200 instead of inventing a fourth behaviour.
- `ItemChannelSettings.tick` resets `delay` to `1200` and acts when `delay % 5 == 0`
  (`:188-195`); the first tick acts, and `d = delay / 5` falls by one every five ticks, so the
  interval is `5 × speed` as the screen says. Consistent, so it is reproduced (as a
  position-derived phase).
- The controller recipe's `rftoolsbase:machine_frame` is a declared mandatory dependency
  (`neoforge.mods.toml`), not a broken reference.

## Skills

Skills: superpowers/using-superpowers,brainstorming,writing-plans,test-driven-development,systematic-debugging,verification-before-completion; BeautifulRust/rust-best-practices,rust-port

## Migration to the SDK format

*Recipe followed from `ports/imfast/notes.md` → `## Migration to the SDK format` (the first
migration) and `ports/integratedtunnels/notes.md` → the same section (the first content + native
one), 2026-11-18. Version **0.1.0 → 0.2.0**; **PR #903** (the `.hnmod`) is superseded.*

**Pins.** SDK nightly **`0b14f8acfd97fa2c750e42795249dcc981178ca4`** (surface + api-source cached
at `ports/_shared/sdk/0b14f8a/`), mixins **`e59da9094f0c2fdda6c627fe9eff22dec6a906f0`** (was
`5a64af93…`), launcher 0.2.78, `package_format` 6, `content_api` 0.2. The old campaign pin
`1d63e0a` does not apply to a package build.

**Why.** Horizon's mod-API merge (#16941, launcher ≥ 0.2.70) retired `.hnmod`, and the registry
closed every `.hnmod` submission on 2026-11-18 with one reason ("republish this as a native
`.hmod` with the SDK from the next Nightly"). The new unit is **one flat `.hmod`** — content,
native code, or both — so this port no longer needs the AE2 embedded-content trick: `assets/` and
`data/` sit beside `src/`, and one package installs both halves.

**The tool is run directly in the port.** `ports/tools/build-slot.sh` speaks the retired staging
tool (`tools/horizon.py`, `mixins/`), which this layout no longer has, so every command below ran
in `ports/xnet` with `CARGO_INCREMENTAL=0` and no `--target-dir` (only the lease wrapper
`BUILD_LOCAL=1 build-slot.sh run … -- bash -c '…'` was used, so the machine's slot accounting still
sees the job). This is a deliberate, format-forced exception to the build-slot rule and should end
when the runner learns the SDK layout.

### 1. Files created

| Path | What it is |
|---|---|
| `horizon-mod.toml` | the only manifest and the only identity: `id = "xnet"`, `version = "0.2.0"`, `api = "0.2"`, `side = "both"`, `capabilities = ["registries"]`, and `[native]` pinned to `0b14f8a` / `e59da90` |
| `Cargo.toml` | one crate; its only dependency is `horizon-mod-api` from `.horizon/sdk/` |
| `src/lib.rs` | the one `#[mod_api::horizon_mod] impl Hooks`: `register`, `level_tick_post`, `register_commands`, `run_mod_command` |
| `src/network.rs` | the game glue (the old `support.rs`): the controller's block-entity **Part**, the per-tick queue, the network walk, the item pass, the readouts and the command handlers |
| `src/runtime.rs` | copied byte-for-byte from `mixins/xnet/runtime.rs` (`cp`, then `cmp`); sha256 `83c29c26e0f3911248d9c6c07f9d599c476ac2fb148f9382f232650f34217d2f` |
| `.gitignore` | `/target`, `/.horizon`, `/Cargo.lock` |
| `assets/xnet/`, `data/xnet/` | the carried content, moved from `content/` (same files, no separate build) |
| `README.md` | rewritten: it described `content/` and the `.hnmod` (trap 25: a stale root document is packed) |

### 2. Files deleted (and where the evidence went)

| Deleted | Why | Kept as evidence |
|---|---|---|
| `mixins/xnet/` (plan, 5 helpers, `support.rs`, `runtime.rs`, `data/xnet-content.hmod`) | the container is retired; `runtime.rs` moved to `src/` | `ports/_shared/evidence/xnet/retired-hnmod-0.1.0/mixins/` |
| `xnet.hnmod` (34,383 B) | unloadable on launcher ≥ 0.2.70 | same folder |
| `content/horizon-mod.toml`, `content/xnet.hmod` (29,775 B) | one manifest and one package now | same folder, `content-old/` |
| `compatibility.json` | `[native]` in the manifest is the pin now | same folder |
| `rust-toolchain.toml` | the pinned Horizon commit and the launcher decide the toolchain | same folder |
| `tools/horizon.py`, `tools/package.py`, `tools/package.sh`, `tools/carry_content.py`, `tools/hmod-load.sh`, `tools/audit/` | staging, `.hnmod` packaging, the content carry and the old content-load check are gone | same folder, `tools/` |
| `PLAN.md` (the mixin plan) | describes injections that no longer exist | same folder |
| `Cargo.lock` | not needed without registry deps, and it changes the artifact (trap 5) | not kept |

**Kept, rewritten:** `tools/e2e.sh` — the scenario and every assertion are format-independent; its
launch path now takes `GAME=<launcher-built horizon-client>` and has no `--dev-mod`, no embedded
copy and no `.horizon-cache` check (section E asserts the one package instead).

### 3. Manifest field mapping (this port)

| Old (`.hnmod` + `content/`) | New (one `.hmod`) |
|---|---|
| `horizon-mixin.toml` `[mod] id = "xnet"` | `id` (also the resource namespace) |
| `[mod] version = "0.1.0"` (bundle) and `0.1.0` (content) | `version = "0.2.0"` — one version for the one package |
| `[mod] priority = 100` | gone; ordering is `#[listener(priority = …)]` |
| `[compatibility] horizon_revision = "1d63e0a5…"` | `[native] horizon = "0b14f8ac…"` |
| mixins `5a64af938ec8…` | `[native] mixins = "e59da9094f0c…"` |
| `[[module]] runtime` + `[[module]] state` (`support.rs`) | `src/runtime.rs` and `src/network.rs` — ordinary Rust modules; no plan section, no support module |
| 5 `[[injection]]` entries | 2 `Hooks` methods for the tick and the commands + 1 Part (`BlockEntityPart`: `block_entity_ticks`, `block_entity_tick`) (table below) |
| `horizon-native.json` | gone (the manifest is the only one) |
| `content/horizon-mod.toml` | one `horizon-mod.toml` at the package root; `assets/` and `data/` beside it |
| — | `side = "both"` (the controller is placed and ticked by an integrated server, which belongs to the client installation), `capabilities = ["registries"]` (fixed shared value) |

### 4. Old injection → new surface

| Old injection (helper) | New surface | Notes |
|---|---|---|
| `embedded_content.rs` (`content_root` HEAD, `native_data!`) | **gone** | one package carries `assets/` + `data/`; nothing is written to `.horizon-cache` and no dev-list append is needed |
| `block_entity.rs` (`block_entity` HEAD → `Some(("xnet:controller", Compound::new()))`) | **gone** | `registrar.block_entity_type("controller").blocks(&["controller"]).build()` declares the type, and a declared type starts with an empty compound — which is all a fresh controller is. The Part is attached for the tick, not for initial data |
| `level_tick.rs` (`level_tick_post`, HEAD; scanned `game.world.block_entities`) | `BlockEntityPart::block_entity_ticks` (`event.ticks = true`) + `BlockEntityPart::block_entity_tick` (queue `event.pos`) + `Hooks::level_tick_post` (drain the level and run the pass) | there is no level-wide block-entity list in the SDK, so a controller queues itself from its own tick exactly as `integratedtunnels`' cable does |
| `register.rs` (`register_commands`, HEAD) | `Hooks::register_commands` → `Command::new(usage)` / `Command::op(usage, 2)` | real usages now: `xnet info <pos:block_pos>`, `xnet channel <pos:block_pos> <channel:integer> <kind:word>`, … (unchanged on the command line) |
| `perform.rs` (`perform_command`, HEAD, free-text `split_whitespace`) | `Hooks::run_mod_command` → `Flow` | `perform_command` no longer exists; the invocation carries parsed `literals` and typed `arguments` (`BlockPos`, `Integer`, `String`) |
| `support.rs` `game.native_block(pos)` | `level.block(host, pos) == Some(host.block_key("xnet:…"))` | `BlockKey` is the engine's identity; resolved once per pass into a `Keys` |
| `support.rs` `game.world.block_entities` scan (controllers, readout) | the port's own per-level set filled by the controller tick (`Controllers::known`) | see the behaviour notes below |
| `support.rs` `game.native_container_slots` + `native_move_items` | the `ITEMS` capability (`LevelRef::capability`) with `ResourceHandler::size/resource/extract/insert_any/insert`, in `host.transaction` | the side-filtered handler replaces the slot view; one transaction per operation |
| `support.rs` `game.native_feedback(Text)` | `host.commands().feedback(Text::literal(...))` | the command source's silence/track-output rules apply |
| `support.rs` `game.world.modified.insert((x>>4, z>>4))` | gone | `BlockEntityRef::merge_data` saves and tells clients, inside the caller's transaction |
| — | `Hooks::register` | `block_entity_type(...)` + `block_entity_part(&kind, ControllerEntity::new(Arc::clone(&…)))` |

### 5. What changed in the glue's behaviour (not in `runtime.rs`)

- **The pass is queued, not scanned.** 0.1.0 walked the level's block entities every tick to find
  controllers; the SDK has no such list, so a controller queues itself in its own block-entity
  tick and `level_tick_post` drains that level. Same cadence, less work (a controller in an
  unloaded chunk no longer re-walks a network), and no behaviour depends on the scan.
- **One transaction per item operation.** The extract, the insert into the destination and — on a
  refusal — the re-insert at the same slot share one transaction, and a source that cannot take
  the item back aborts it, so nothing leaves a container without arriving somewhere. Upstream's
  contract ("a refused item stays in its slot") is unchanged.
- **The stack moves, not just its id.** `native_move_items` took an item-id filter; the port now
  extracts the `ItemResource` it found and inserts that, so components travel with the stack. The
  filter is still by item id, as upstream's ghost slots are.
- **The readout's controller list is the tick's own knowledge.** `/xnet info <connector>` walks the
  controllers whose block entity has ticked at least once (checked against the level on read),
  because there is no level-wide list; before a controller's first tick the readout says "no
  channel uses it".
- **The level-tick pass waits rather than dropping work.** `LevelRef::capability` answers only for
  the level the server ticks now, so `level_tick_post` returns before draining when the level is
  not in place; the queue is drained on the next tick.
- **Commands are typed.** The game parses and permission-checks a usage before `run_mod_command`,
  so `/xnet channel 1 100 1 0 item` is unchanged but a malformed one is refused by the game's own
  usage message instead of XNet's line.
- **The settings write is a merge of all eight channel keys** (`none` for an unused channel), which
  cannot leave a removed channel behind; 0.1.0 mutated the entity and marked the chunk by hand.

### 6. Ladder measured for 0.2.0

Run in the port, `CARGO_INCREMENTAL=0`, in the order below (logs in
`ports/_shared/evidence/xnet/`):

| Step | Command | Result |
|---|---|---|
| Runtime unit tests | `cargo test` | **10 passed**, 0 failed (`sdk-test.log`) |
| Lints | `cargo clippy --all-targets -- -D warnings` | exit 0, **0 warnings** (`sdk-clippy.log`) |
| Formatting | `rustfmt --check --config skip_children=true --edition 2024 src/lib.rs src/network.rs` | exit 0 (`sdk-fmt.log`); `cargo fmt --check` is **not** usable here — trap 27 |
| Package | `rm -f Cargo.lock; cargo horizon-mod build` | `target/horizon-mod/xnet.hmod`, exit 0 (`sdk-build.log`) |
| Registry check | `cargo horizon-mod check xnet.hmod` | `xnet 0.2.0 (native-mod, format native-1/0b14f8acfd97fa2c750e42795249dcc981178ca4/e59da9094f0c2fdda6c627fe9eff22dec6a906f0, for 26.3)`, **57,572 B**, sha256 `22a8a691797197cda70edb65f34dae8f1dacd82ceb68fc721124207128a485d0`, **exit 0** (`sdk-check.log`, `sdk-check-json.log`) |
| What it uses | `unzip -p xnet.hmod uses.json` (the `--uses` flag prints only the summary in this build — trap 30) | **258 entries**, including `hook:level_tick_post`, `hook:register_commands`, `hook:run_mod_command`, `hook:block_entity_ticks`, `hook:block_entity_tick`, `part:block`, `part:block_entity`, `capability:horizon:items`, `handle:BlockEntityRef::merge_data`, `trait:HostTransaction`, `trait:ResourceHandler` |
| Fits the pinned build | `cargo horizon-mod fits xnet.hmod --build nightly` | `0b14f8a  made for`, exit 0 (`sdk-fits.log`) |
| Reproducible | `cargo horizon-mod build` again on the same sources | byte-identical: 57,572 B, same sha256 (`sdk-rebuild.log`) |
| Fresh clone | `.horizon/`, `target/` and `Cargo.lock` removed, then `cargo horizon-mod build` | the SDK fetched itself; same 57,572 B and sha256 (`sdk-fresh-clone.log`) |
| `runtime.rs` untouched | `cmp src/runtime.rs <retired>` and `unzip -p xnet.hmod src/runtime.rs \| sha256sum` | `83c29c26…17d2f` in all three places (`sdk-runtime-sha256.txt`) |
| Package contents | `unzip -l xnet.hmod` | **49 entries**: manifest, `Cargo.toml`, `LICENSE`, `LICENSE-NOTICE.md`, `README.md`, 40 content files under `assets/xnet/` + `data/xnet/`, `src/lib.rs`, `src/network.rs`, `src/runtime.rs`, `uses.json`. No `Cargo.lock`, no `.horizon/`, no retired path |
| In-game e2e | `GAME=<launcher-built> tools/e2e.sh` | **not run**: `horizon-launcher` is not installed here, so no game carries the package (`sdk-e2e-blocked.txt`); the scenario is kept and adapted |
| Skills line | `python3 ports/tools/check-skills.py --notes ports/xnet` | `Skills line present`, **SKILLS GATE: PASS** |

### 7. Traps, all hit in this migration (27–33 are new to this port)

The first fifteen are the reference's (`ports/imfast/notes.md` §6) and were all re-confirmed:
a leftover old path refuses the package, a 38-char pin reads as missing, `check` with no file
checks wasm, `fmt --all` formats the SDK, a packed `Cargo.lock` changes the artifact, `side` is the
installation side, `capabilities = ["registries"]` is mandatory, `runtime.rs` must be `pub mod`,
session state must be `Mutex`, `#[non_exhaustive]` wildcards, context types changed shape,
`commands()` is `WorldOnly`, no `rust-toolchain.toml`, `horizon-mod build` does pack a native
project, and the e2e needs a launcher-built game. `integratedtunnels`' 16–26 (host names are
backend methods, containers are the `ITEMS` capability, `JavaString` is not re-exported, a part is
a trait impl, one `Default` mod instance with `Arc<Mutex<…>>`, no level handle in a block-entity
tick, `set_block_property`, `perform_command` is gone, the embedded pattern is unnecessary, every
root document is packed) were all re-confirmed too. New here:

27. **`cargo fmt` (and `rustfmt src/lib.rs`) rewrites the byte-identical `src/runtime.rs`**, because
    rustfmt follows `mod` declarations. It did here, and `cmp` caught it (line 274). `cargo fmt
    --check` therefore fails on this crate with diffs **only in `runtime.rs`** — do not "fix" them:
    the campaign rule is byte-identical carry-over. Gate formatting with
    `rustfmt --check --config skip_children=true --edition 2024 src/lib.rs src/network.rs`, and if
    the carried file is ever rewritten, restore it from
    `ports/_shared/evidence/xnet/retired-hnmod-0.1.0/mixins/xnet/runtime.rs` and `cmp` it.
28. **`Compound` is `BTreeMap<JavaString, Tag>` and `JavaString: From<&str>` only.**
    `data.insert(key.into(), …)` with a `String` key does not compile (`From<String>` is not
    implemented): use `key.as_str().into()`. Reading a key is `nbt.get(&name.into())`, which
    allocates — a handful per tick here, and `Tag::get(&str)` avoids it on a `Tag`.
29. **No level-wide block-entity list.** `LevelRef` has no `block_entities()`; a port that used to
    scan `game.world.block_entities` must queue itself from its own block-entity tick, which needs
    a `BlockEntityPart` — and that needs the block entity type declared in `register`, because a
    content block JSON only declares the *block*.
30. **`cargo horizon-mod check <file> --uses` prints only the summary** in this build (0.2.78); the
    entry list is `uses.json` **inside** the package (`unzip -p xnet.hmod uses.json`).
31. **`block_entity_type(id).blocks(&[…])` names blocks by their bare name** (`"controller"`), not
    the namespaced id; the level side compares `BlockKey`s from `host.block_key("xnet:controller")`.
32. **A `run_mod_command` must check its literals before doing anything else.** Resolving the
    acting level first answered "no level to act in" to another mod's command (caught in review
    here, not in game): `if !matches!(literals.first(), Some(&"xnet")) { return Flow::Continue }`.
33. **Content moves to the package root.** `assets/` and `data/` must sit beside `src/`; a
    remaining `content/` is "unexpected package path" (the same refusal as trap 1, from the content
    side). Move the directories *before* the first build, not after.

### 8. Changelog / supersede text (for the resubmission that replaces #903)

> **0.2.0 — republished as a native `.hmod` (the SDK format).** Horizon's mod-API merge retired the
> `.hnmod` container, so this release replaces the closed submission **#903** with one flat `.hmod`
> that carries the controller, the cable, the connector, both items, their assets, recipes, loot and
> tags, and the channel code together — the embedded-content workaround of 0.1.0 (and its
> `.horizon-cache` write) is gone, and one file installs both halves. Built against SDK nightly
> `0b14f8a` (mixins `e59da90`, api 0.2, format native-1). Behaviour is unchanged: an item channel
> moves one item per operation at the jar's default of one every 10 ticks from the inventory behind
> an extractor to the one behind an inserter, highest priority first, and `/xnet info` reports a
> controller's network, channels and connector sides (or a connector's channels). The pass is now
> queued from each controller's block-entity tick instead of scanning the level, each item move is
> one transaction, and `/xnet …` is a set of typed command usages handled by `run_mod_command`.
> `src/runtime.rs` is byte-identical to 0.1.0 and keeps its 10 tests.

### 9. Gaps this migration adds to the slice's list

- **The in-game e2e is not re-run** (trap 15/26): a native package is compiled into the client by
  `horizon-launcher native-build <port> --instance <i> --trust-native-code`, and no launcher is
  installed here. The 0.1.0 run (`evidence/xnet/e2e.log`, `e2e-selftest.log`) stays the behaviour's
  evidence; `tools/e2e.sh` keeps every assertion and now takes `GAME=`. The old control leg
  ("the same binary with the embedded file made unavailable") has no equivalent — the package is
  compiled in, not loaded from a file — so section E asserts the one package instead.
- **The controller set the readout walks is per session and pruned only on read.** A controller
  removed from the world stays in the set (it is checked against the level before use, so it is
  never run and never listed); memory grows with the controllers ever seen in a session.
- **`/xnet info <connector>` sees only controllers that have ticked at least once** since the
  world opened (one tick after placement), where 0.1.0's scan saw every loaded block entity.
- Everything else in the 0.1.0 gap list (energy/fluid/logic channels, the advanced connector, both
  screens, the other four cable colours and the routers, antennas/facades/redstone proxy,
  stack/count modes, the recipe-unlock advancements, the `rftoolsbase` recipe) is unchanged.

Skills: superpowers/using-superpowers,brainstorming,writing-plans,test-driven-development,systematic-debugging,verification-before-completion; BeautifulRust/rust-best-practices,rust-port

## Verification (0.1.0, the retired `.hnmod`)

Pin **`1d63e0a5a37a7a708eb0b32b49bf0dfeb28ed8a9`**, mixins
**`5a64af938ec86b00d53d16a408aabc0038fb3e57`** (schema 6). Every build, test and e2e ran through
`ports/tools/build-slot.sh` (`CARGO_INCREMENTAL=0`, no bare cargo, no `--target-dir`); the in-game
runs used the pin-matched shared `horizon-ctl`
(`ports/_shared/bin/horizon-1d63e0a5/horizon-ctl`) and the port-built fast game, built and run in
**one** e2e lease. The last two e2e runs went local because the second machine was unreachable
(`build-slot.sh` said so in the log); the earlier ones forwarded.

### Ladder

| Step | Command (through the slot runner) | Result |
|---|---|---|
| runtime RED | `cargo test --locked --offline` (port) | `exit=101`, **151** `error[E…]` — the API did not exist yet (`evidence/xnet/runtime-red.log`) |
| runtime GREEN | `cargo fmt --all --check`, `cargo clippy --all-targets --locked --offline -- -D warnings`, `cargo test --locked --offline` | `fmt_exit=0`, `clippy_exit=0`, **10 passed**, 0 failed — `evidence/xnet/runtime-green.log` |
| stage | `python3 tools/horizon.py stage` | `stage_changed: true`, ~95 s, the plan's **2 modules / 5 injections** accepted by the planner |
| check | `python3 tools/horizon.py check` | `cargo_exit: 0`, **0 warnings** — `evidence/xnet/check.log` |
| embedded tests | `python3 tools/horizon.py test` (`--lib game::xnet::tests`) | **10 passed**, 0 failed, 1240 filtered — `target/horizon-test.log` |
| fast build | `python3 tools/horizon.py build` | `cargo_exit: 0` — `evidence/xnet/build.log` |
| content `.hmod` | `tools/package.sh` (`cargo-horizon-mod build` + `check`) | **29,775 B**, SHA-256 `35401f7a6698b5a9ce9f5b3d0b92aa7a2fbbfe0f6290585c18ebf35ab45c5a2c`, **41 entries**, `xnet 0.1.0 (mod, API 0.2, both)`, "no code: content only" — `evidence/xnet/package.log` |
| native `.hnmod` | `tools/package.sh` (`tools/package.py`) | **34,383 B**, SHA-256 `f7fd98c935b2d743e6aa6936ba294ae04da54afc4683d8dafbf56829579ea13c`, **12 entries**; re-packaging reproduces the same bytes; `horizon-native.json` pins `horizon` `1d63e0a5…`, `mixins` `5a64af93…`, `game` 26.3 — same log |
| native check | `cargo horizon-mod check xnet.hnmod --kind native-mod` | `xnet 0.1.0 (native-mod, format native-1/1d63e0a5…/5a64af93…, for 26.3)` — same log |
| content carry check | `python3 tools/carry_content.py --check` | `ok 40 files, 31074 bytes` |
| `.hmod` load check | `tools/hmod-load.sh` (shared game, `--dev-mod content/xnet.hmod`) | `PASS`: the package registers `xnet:controller`, `xnet:connector`, `xnet:netcable` and both items; the launch **without** `--dev-mod` says `Unknown block type 'xnet:controller'` / `Unknown item 'xnet:connector_blue'`; `EXIT:0` — `evidence/xnet/e2e-hmod.log` |
| e2e | `build-slot.sh e2e … -- tools/e2e.sh` (`EMBEDDED=1 CONTROL=1`) | **PASS**, `EXIT:0` — `evidence/xnet/e2e.log` |
| selftest | `SELFTEST=1 CONTROL=0 tools/e2e.sh`, same lease | **`FAIL: SELFTEST: the channel moved 16 stone in 160 ticks, so the check is alive`**, `EXIT:1` — `evidence/xnet/e2e-selftest.log` |
| e2e log audit | `python3 ports/tools/check-e2e-logs.py xnet --strict` | `logs=3 ok=3 missing-exit=0 bad-exit=0`, exit **0** |
| skills gate | `python3 ports/tools/check-skills.py --notes ports/xnet` | `ok xnet/notes.md: Skills line present`, `SKILLS GATE: PASS`, exit **0** |
| submission gate | `python3 ports/tools/upstream_ready.py xnet` | **exit 1**, blocked on two things outside this port's tree: `status is pending, not ported` (the ledger row, which this job may not edit) and `add-on whose required parent(s) rftoolsbase are not in the registry` (the upstream `horizon-mods` registry, a submission question). Everything it can read from the port passes: `licence MIT reviewable`, `artifact: ports/xnet/xnet.hnmod`, `artifact built after its sources`, `notes.md has the Skills line`, `pinned to 1d63e0a5`, `e2e: e2e-hmod.log: PASS + EXIT:0` |

### In-game transcript, one file (quoted from `evidence/xnet/e2e.log`)

```
== launch (The End arena, native bundle with embedded content) ==
== control: the same game with no embedded content file ==
  launch: no --dev-mod; the bundle carries its own content
  setblock reply: {"chat":["Unknown block type 'xnet:controller'","...k 4 100 0 xnet:controller<--[HERE]"],"ok":true}
  give reply: {"chat":["Unknown item 'xnet:connector_blue'","give @s xnet:connector_blue 1<--[HERE]"],"ok":true}
  without the embedded file: no block, no item (block 4,100,0 = minecraft:air)
  launch: no --dev-mod; the bundle carries its own content
  embedded content: 29775 B at /tmp/opencode/xnet-home.x6fen8/.horizon-cache/xnet-embedded-content.hmod, identical to content/xnet.hmod
== build the networks ==
  cable at 2,100,0: xnet:netcable[down=false,east=true,north=false,south=false,up=false,west=true]
== A: the item channel moves 20 stone at the jar's default rate ==
  XNet controller at 1,100,1: channel 0 1,100,0 west extract, 1 item every 10 ticks, priority 0, filter all
  XNet controller at 1,100,1: channel 0 item, 2 connector(s)
  XNet controller at 1,100,1: network 1,100,0 2,100,0 3,100,0 4,100,0, 1 channel(s) enabled
  XNet connector at 1,100,0: channel 0 1,100,0 west extract, 1 item every 10 ticks, priority 0, filter all
  XNet connector at 1,100,0: network 1,100,0 2,100,0 3,100,0 4,100,0
  source before: 15, target before: 5
  160 ticks moved 15: source 0, target 20
== A2: /xnet speed 40 slows the same extractor to one item per 40 ticks ==
  XNet controller at 1,100,1: channel 0 1,100,0 west extract, 1 item every 10 ticks, priority 0, filter all
  200 ticks at 40 ticks an operation moved 5 (expected about 5)
== A3: a filter decides what the extractor takes ==
  XNet controller at 1,100,1: channel 0 1,100,0 west extract, 1 item every 10 ticks, priority 0, filter minecraft:dirt minecraft:cobblestone
  XNet controller at 1,100,1: channel 0 1,100,0 west extract, 1 item every 10 ticks, priority 0, filter minecraft:dirt
  whitelist dirt over 100 ticks moved 0
  filter cleared: source 0, target 44
  blacklist stone over 100 ticks moved 0, source 4
== B: a cable with no controller moves nothing (control) ==
  source: 4, target: 0
== B2: a controller that touches only a cable has no network (control) ==
  XNet controller at 15,100,1: network none, 1 channel(s) enabled
  source: 4, target: 0
== C: a full destination refuses the items (control) ==
  XNet controller at 24,100,1: channel 0 27,100,0 east insert, 1 item every 10 ticks, priority 0, filter all
  source: 4, target stone: 0, target dirt: 1728
== C2: the higher-priority inserter takes the items ==
  XNet controller at 30,100,1: network 30,100,0 31,100,0 32,100,0 32,100,1 33,100,0, 1 channel(s) enabled
  XNet controller at 30,100,1: channel 0 32,100,1 south insert, 1 item every 10 ticks, priority 5, filter all
  XNet controller at 30,100,1: channel 0 33,100,0 east insert, 1 item every 10 ticks, priority 0, filter all
  priority 5 wins: south 4, east 0
  priority 9 wins: south 4, east 4
== D: the channel, its sides and its interval survive a relaunch ==
  after the relaunch: target 52 (expected 52)
== E: one file — the .hnmod carries the content, and it is the only content mod ==
  content mods in the run's home: /tmp/opencode/xnet-home.x6fen8/.horizon-cache/xnet-embedded-content.hmod
PASS: an item channel moved 20 stone through a cable run … the higher-priority inserter took the items and the lower one only after the priorities changed … and the content came from the embedded copy inside the one delivered .hnmod and was unknown when it could not be written
EXIT:0
```

The `PASS:` line is abridged with `…` because it is one long sentence; the log has it whole.

That is the fail-capable set this slice needs: **with** the one `.hnmod` installed and no
`--dev-mod` the blocks, the items and the transfer all work; **without** the embedded file (write
impossible) both ids are unknown; and `SELFTEST=1` flips the rate assertion so the same scenario
fails with a `FAIL:` verdict and `EXIT:1`.

## Review fixes (2026-11-18 review, rebuilt 2026-11-19 at 0.2.1)

Skills: superpowers/using-superpowers, systematic-debugging, test-driven-development, verification-before-completion; BeautifulRust/rust-best-practices, rust-port

**Findings and fixes** (`src/network.rs`):

1. **202 — unbounded, repeated cable walk.** `runtime::walk_network(start, budget, is_wire)` walks
   with a `BTreeSet` and stops at `MAX_NETWORK_VISITS = 4_096`; `run_controllers` reads the
   controller's channels **first** and skips a controller with no non-empty item channel, so a
   controller with nothing to transfer never walks its net.
2. **266 — stale destination.** `run_operation` re-checks `on_network(network, target.connector)` for
   every destination at transfer time (and `run_channel` for the source), so a saved connector whose
   cable was removed receives nothing.
3. **546 — remote diagnostics.** `runtime::diagnostics_permitted(permission, distance)` gates
   `/xnet info` (operator level 2 or the console anywhere, everyone else within `DIAGNOSTIC_REACH` = 6
   blocks), so a network's positions and filter settings are not readable from across the world.
4. **68 — controller positions accumulate.** `runtime::prune_known` drops positions whose block is no
   longer a controller and caps the set at `MAX_KNOWN_CONTROLLERS = 256`; `known_controllers` (the
   readout's only consumer) prunes while it reads.

**Tests.** `runtime::tests::{the_cable_walk_stops_at_its_budget,
a_destination_counts_only_while_it_is_on_the_network, the_known_controller_set_is_pruned_and_capped,
a_remote_diagnostic_needs_reach_or_permission}` — each was run against a stub of the old behaviour
(unbounded walk, always-on-network, no prune, always-allowed) and failed there.

**A harness-facing defect the in-game leg found (fixed here).** `<item:word>` cannot carry
`minecraft:dirt` (a `word` ends at `:`), so `/xnet filter`/`unfilter` never parsed; both now take
`<item:text>` (the usage's last argument). A3 (the filter phase) fails before this change and passes
after it.

**Ladder (0.2.1).** `cargo test` 14 passed · clippy `-D warnings` exit 0 · `fmt --check` exit 0 · two
builds byte-identical · `check` exit 0 (`xnet 0.2.1 …`) · `fits --build 0b14f8a` exit 0.

**In game.** `launcher-game.sh run xnet` → `PASS` + `E2E EXIT:0` (A–E, including the filter phase and
the one-package check); control → `FAIL: the connector … did not register` + `EXIT:1`. Evidence:
`ports/_shared/evidence/xnet-e2e/`.

**Package.** `xnet.hmod` **60,761 B**, sha256
`f0522c24886eef6fe18f3288b288fecf582be08815a41ef5f3e0719b8e6d341d`.

## Slice 2 — the item channel's amount and destination policy (0.2.2)

*The gap this closes*, quoted from [`## Slice boundary and gaps`](#slice-boundary-and-gaps) item 6:

> **`StackMode.STACK`/`COUNT`, `extractAmount`, `count` (stock keeping), `ExtractMode.RANDOM`/`ORDER`,
> `ROUNDROBIN` channel mode**, the tag/meta/component filter toggles and the redstone gates.

The slice takes the **amount one operation carries** (`StackMode.SINGLE`/`STACK`/`COUNT` with
`extractAmount`), the **stock-keeping `count`** on either end, and the **destination policy**
(`ChannelMode.PRIORITY`/`ROUNDROBIN`). The extract-mode slot cursor (`FIRST`/`RND`/`ORDER`), the
tag/meta/component filter toggles and the redstone gates stay gaps — the first needs a saved
per-connector slot cursor, which a block-entity write inside the tick loses at this pin.

**Why the `machines` tier fits it.** It is the item channel's own tick and its saved settings: no
block, item, model or menu is added, and the whole slice is the rules the existing controller,
cable and connector already run. Upstream's `ItemChannelSettings.tickItemHandler`, `insertStack`
and `fetchItem`, and `ItemConnectorSettings`' fields, are the behaviour; the parity legs add no
family the jar does not have.

**Pin.** Horizon `0a875da91dbb0fdcb7ba8c42c11341aa229af51a` + mixins
`e59da9094f0c2fdda6c627fe9eff22dec6a906f0` (Nightly 2026-11-20). `cargo horizon-mod update
--horizon 0a875da` reported **"none of the 261 items it uses changed"**, so the repin itself is
behaviour-free; the package's `[native]` block now names it.

### The rules, and where they come from

| Rule | Upstream | Here |
|---|---|---|
| `StackMode.SINGLE` | `fetchItem` s = 1 | one item per operation (0.2.1's behaviour) |
| `StackMode.STACK` | s = `stack.getMaxStackSize()`, capped by `fetchItem`'s `maxamount = 64` | `take_amount` = 64, capped by the slot |
| `StackMode.COUNT` | s = `extractAmount`, null → 1 | `extract_amount`, stored 1..=64 (`OPERATION_CAPACITY`) |
| extractor `count` | `tickItemHandler`: `countItems(handler, matcher)` must reach `count`, and only `amount - count` leaves | `take_amount` = min(wanted, slot, matching − count) |
| inserter `count` | `insertStack`: `toinsert = min(total, count - countItems(destination, matcher))` | `accept_amount` = min(amount, count − matching) |
| the count's matcher | `countItems` counts what `getMatcher` accepts; an empty filter accepts everything | `count_matching` counts what the settings' filter accepts (so a `count` with no filter limits the whole container, as upstream) |
| `ChannelMode.PRIORITY` | `insertStack` sets `roundRobinOffset = 0` | `destination_plan` ignores the offset |
| `ChannelMode.ROUNDROBIN` | offset advances once per destination that accepted an item | `next_offset` after the operation; the offset is session state, as upstream's field is in memory |

The saved entry string gained three fields after the priority (`stack`, `amount`, `count`, `-` for
none); `ItemSettings::load` recognises them by the fifth field naming a stack mode, so a 0.2.1 save
loads with the defaults instead of losing its connector settings. The channel's mode is a third
block-entity key (`chN.mode`), written by the same merge that writes the entries.

Commands (the controller screen is still a gap): `/xnet stack <pos> <ch> <conn> <side>
<single|stack|count>`, `/xnet amount … <1..64>`, `/xnet count … <n|->`, `/xnet mode <pos> <ch>
<priority|roundrobin>`. The readout appends the mode and the count: `…, priority 0, filter all,
single`, `…, count`, `…, single, keep 5`, `…, single, stock 5`. The channel line now ends with its
distribution mode (`channel 0 item, 3 connector(s), roundrobin`).

### Red → green

The four tests were written first and run against a stub that reproduced 0.2.1's behaviour (one
item per operation, no count, no rotation, the old save format), so the red is a behavioural
failure, not a missing symbol — `ports/_shared/evidence/xnet/slice2-test-red.log`:

```
test runtime::tests::a_stack_mode_says_how_much_one_operation_carries ... FAILED
test runtime::tests::the_stock_count_limits_both_ends ... FAILED
test runtime::tests::roundrobin_rotates_the_destination_order ... FAILED
test runtime::tests::the_amount_settings_round_trip_and_old_saves_still_load ... FAILED
---- a_stack_mode_says_how_much_one_operation_carries ----
panicked at src/runtime.rs:1109: assertion `left == right` failed
  left: 1
 right: 64
---- the_stock_count_limits_both_ends ----
panicked at src/runtime.rs:1149: assertion `left == right` failed
  left: 1
 right: 15
---- roundrobin_rotates_the_destination_order ----
panicked at src/runtime.rs:1220: assertion `left == right` failed
  left: [1, 2, 3]
 right: [2, 3, 1]
---- the_amount_settings_round_trip_and_old_saves_still_load ----
panicked at src/runtime.rs:1279: assertion `left == right` failed
  left:  Some(ItemSettings { …, filter minecraft:stone, stack_mode: Single, extract_amount: 1, count: None })
 right: Some(ItemSettings { …, filter minecraft:stone, stack_mode: Stack, extract_amount: 1, count: Some(5) })
test result: FAILED. 14 passed; 4 failed
```

Green: `18 passed; 0 failed` (`slice2-test.log`). The glue's new pieces are `count_matching`
(upstream's `countItems`), `move_up_to` (one transaction per operation: extract, insert across the
plan's destinations as far as each accepts, put back what did not arrive) and the per-level,
per-channel round-robin offsets in `Controllers` (pruned with the known-controller set).

### Ladder (0.2.2)

| Step | Command (through `build-slot.sh run --step …`) | Result |
|---|---|---|
| runtime RED | `cargo test` against the stub | 14 passed, **4 failed** — `slice2-test-red.log` |
| runtime GREEN | `cargo test` | **18 passed**, 0 failed — `slice2-test.log`, `sdk-test.log` |
| lints | `cargo clippy --all-targets -- -D warnings` | exit 0, 0 warnings — `slice2-clippy.log` |
| formatting | `rustfmt --check --config skip_children=true --edition 2024 src/{lib,network,runtime}.rs` | `fmt_exit=0` — `slice2-fmt.log` (trap 27: `cargo fmt` follows `mod`s) |
| package | `rm -f Cargo.lock; cargo horizon-mod build` ×2 | `target/horizon-mod/xnet.hmod`, **`cmp` byte-identical**, **67,820 B**, sha256 `305e8976…b67`, 53 entries, no packed `Cargo.lock` — `slice2-build.log` |
| check | `cargo horizon-mod check xnet.hmod` | `xnet 0.2.2 (native-mod, format native-1/0a875da…/e59da909…, for 26.3)`, **67,820 B**, sha256 `305e8976…b67`, exit 0 — `slice2-check.log` |
| fits the pin | `cargo horizon-mod fits xnet.hmod --build 0a875da` | `0a875da  made for` |
| fits another build | `cargo horizon-mod fits xnet.hmod --build 0b14f8a` | `0b14f8a  fits` — `slice2-fits.log` |
| package audit | `python3 ports/tools/verify-sdk-package.py xnet` | **1/1** (package, runtime byte-identical, no `Cargo.lock`, 18 tests, 0 clippy warnings, check, fits, skills gate) |
| in game | `launcher-game.sh run xnet` (offscreen) | **PASS + `EXIT:0`** — `evidence/xnet-e2e/e2e-slice2.log` |
| control | `GAME=$CONTROL_GAME tools/e2e.sh` | `FAIL: the connector at 1,100,0 did not register: minecraft:air`, `EXIT:1` — `e2e-slice2-control.log` |
| selftest (A) | `SELFTEST=1` | `FAIL: SELFTEST: the channel moved 16 stone in 160 ticks…`, `EXIT:1` — `e2e-slice2-selftest.log` |
| selftest (slice 2) | `SELFTEST=amount` (F1 with the stack mode left at single) | `FAIL: one operation moved 2 of 20 items, not the whole slot`, `EXIT:1` — `e2e-slice2-selftest-amount.log` |
| log audit | `python3 ports/tools/check-e2e-logs.py xnet-e2e --strict` | `logs=6 ok=6 missing-exit=0 bad-exit=0`, exit 0 |
| skills gate | `python3 ports/tools/check-skills.py --notes ports/xnet` | `Skills line present`, PASS |

### In game (quoted from `e2e-slice2.log`)

```
== F: how much one operation carries, and where it goes ==
== F1: stack mode moves the whole slot in one operation ==
  XNet controller at 37,100,1: channel 0 37,100,0 west extract, 64 items every 200 ticks, priority 0, filter all, stack
  one operation at 200 ticks moved 20 of 20 (single would move 1)
== F2: count mode takes the configured amount per operation ==
  XNet controller at 37,100,1: channel 0 37,100,0 west extract, 16 items every 10 ticks, priority 0, filter all, count
  100 ticks at 16 an operation moved 160 (single would be about 10)
== F3: a stock count caps the destination ==
  XNet controller at 37,100,1: channel 0 40,100,0 east insert, 1 item every 10 ticks, priority 0, filter all, single, stock 5
  stock 5 over 200 ticks: target 5, source 15
== F4: a count on the extractor keeps items in the source ==
  XNet controller at 37,100,1: channel 0 37,100,0 west extract, 1 item every 10 ticks, priority 0, filter all, single, keep 5
  keep 5 over 200 ticks: source 5, target 15
== F5: round robin uses both inserters; priority fills one ==
  XNet controller at 37,100,1: channel 0 item, 3 connector(s), roundrobin
  round robin: east 10, south 10
  XNet controller at 37,100,1: channel 0 item, 3 connector(s), priority
  priority: east 0, south 20
== F6: an unknown stack mode is refused and changes nothing ==
== D: the channel, its sides and its interval survive a relaunch ==
  XNet controller at 37,100,1: channel 0 item, 3 connector(s), roundrobin
  XNet controller at 37,100,1: channel 0 37,100,0 west extract, 16 items every 10 ticks, priority 0, filter all, count
  after the relaunch the count-mode channel moved 4 (expected 4)
```

Each assertion can fail, and each has its pair: the whole default leg is the differential for the
control leg (a game without the package cannot register the connector), `SELFTEST=amount` is F1's
counterfactual (the same phase with the stack mode left at `single` moves 2, not 20), and F5's
priority half is the differential *inside* the passing run (the same three inserters, the same
20 items, all to one destination).

### Upstream defects fixed

The parity legs found three carried artefacts that could never work at this pin. Each was proved
from the game's own log, fixed minimally, and is proved by an in-game assertion:

1. **The recipes were in the vanilla ingredient form, which this pin does not parse.** The server
   logged `recipes: invalid ingredient (unresolved tag or malformed input): {"item":"minecraft:redstone"}`
   for every key of all three files: the pin's `Ingredient::parse` reads a string (`"minecraft:x"`)
   or `"#tag"`, not the 1.21.1 `{ "item": … }` / `{ "tag": … }` objects. Fixed by writing the three
   files in the pin's compact form; the declared patterns, keys and results are unchanged.
   A user would notice nothing but a recipe that never appeared in the recipe book.
2. **The `netcable` and `connector` loot tables dropped a block id that has no item** (`"item": false`
   on both blocks, as upstream declares), so breaking a cable dropped nothing. Fixed to drop
   `xnet:netcable_blue` and `xnet:connector_blue` — upstream's table drops the item of the block's
   colour, and this slice has one colour.
3. **The recipes name `c:` convention tags, which no mod at this pin declares** (`c:dyes/blue`,
   `c:chests`, `c:ingots/gold`, `c:nuggets/gold`), so even in the right form they could not match.
   Under NeoForge the loader supplies them. Fixed by declaring the four tags in this package with
   their vanilla members, which also makes them readable by other mods (the ruling's "declare it,
   so the ecosystem finds it under either name"). `c:chests` carries the chest, trapped chest and
   barrel; the game proves the tag is read, not one item: the connector crafts with either chest and
   not with stone.

### Coverage (the parity rule, 2026-11-20)

| Family | State |
|---|---|
| `xnet:controller` block + item | `tested` (P: give, place, loot; the readout in A/F) |
| `xnet:netcable`, `xnet:connector` blocks | `tested` (A/B/C setblock and read-back; P: both refuse an item id, as their files declare; loot drops the blue item) |
| `xnet:netcable_blue`, `xnet:connector_blue` items | `tested` (P: give and read back, craft, loot) — **they do not place their block**: `blocked(no block-item link for a content item at this pin; upstream's colour item is a BaseBlockItem)`, next slice |
| recipe `netcable_blue` (16) | `tested` (P: crafted, id + count + string and gold nugget consumed; red dye control) |
| recipe `connector_blue` (1) | `tested` (P: crafted with a chest and with a trapped chest, chest consumed; stone and red-dye controls) |
| recipe `controller` | `blocked(rftoolsbase:machine_frame)` — the control proves it crafts nothing without its parent |
| loot tables (`netcable`, `connector`, `controller`) | `tested` (P: `loot give` each, the declared item arrives) |
| tags `xnet:cables`, `xnet:connectors` | `tested` (P: `execute if items … #tag` with a dirt negative) |
| tag `minecraft:mineable/pickaxe` | `tested` (P: the placed controller matches, dirt does not) |
| tags `c:dyes/blue`, `c:chests`, `c:ingots/gold`, `c:nuggets/gold` | `tested` via the recipes (a member matches, a non-member does not) |
| commands `/xnet info`, `channel`, `connector`, `remove`, `speed`, `priority`, `filter`, `unfilter`, `blacklist` | `tested` (A, A2, A3, C2, D) |
| commands `/xnet stack`, `amount`, `count`, `mode` | `tested` (F1–F6) |
| item channel: rate, filter, priority | `tested` (A, A2, A3, C2) |
| item channel: stack mode, count mode, stock/keep, round robin | `tested` (F1–F5, D) |
| item channel persistence | `tested` (D) |
| energy / fluid / logic channels, the controller's RF cost | `not ported` (gap 1) |
| advanced connector | `not ported` (gap 2) |
| controller and connector screens | `not ported` (gap 3) |
| the other four cable colours, routers, wireless routers | `not ported` (gap 4) |
| antennas, facades, redstone proxy | `not ported` (gap 5) |
| `ExtractMode.RND`/`ORDER`, the tag/meta/component filter toggles, redstone gates | `not ported` (gap 6, the rest of it) |
| recipe unlock advancements | `not ported` (gap 7) |

### Limits this slice adds

- A round-robin offset lives in the port's session state, not in the controller's block entity (a
  write inside the tick is reverted at this pin). It restarts at the first destination when the
  world loads or the channel's kind/mode changes, which is what upstream's in-memory field does.
- `StackMode.STACK` takes up to 64, the operation cap; an item whose real max stack is 16 would be
  taken as one 16-stack from the slot, which is what the container allows anyway.
- `extract_amount` is stored 1..=64 (`fetchItem` caps at 64), so a `/xnet amount 100` behaves as 64
  instead of being rejected.
- The `count` is the matcher's total, and an empty filter matches everything: an inserter with
  `stock 5` and no filter stocks the whole destination to five items of any kind, as upstream's
  `countItems(destination, getMatcher())` does.

Skills: superpowers/using-superpowers,brainstorming,writing-plans,test-driven-development,systematic-debugging,verification-before-completion; BeautifulRust/rust-best-practices
