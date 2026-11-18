# XNet — a Horizon port

**XNet** is a Minecraft mod. Advanced item/liquid/energy/... networking. This repository is a **port of it to [Horizon](https://github.com/horizon-rust/horizon)**, the Rust engine for Minecraft Java 26.3. It carries the complete source of the port plus the content files it ships, and installs as a single `.hmod` package.

Upstream: [McJtyMods/XNet](https://github.com/McJtyMods/XNet) — licence **MIT**.

| | |
|---|---|
| Upstream mod | XNet ([source](https://github.com/McJtyMods/XNet)) |
| Ported to | Horizon 0b14f8a, Minecraft Java 26.3, mod API `0.2` |
| Package | `xnet.hmod` (install it with the Horizon launcher) |
| Licence | **MIT** — the upstream mod's licence; see [Licence and credit](#licence-and-credit) |

## What this port implements

### What this slice does

- **`xnet:controller`** — the network's controller. Eight channels; each is `none` or `item` and
  holds connector sides with a mode, a tick interval, a priority and a filter.
- **`xnet:netcable`** (item `xnet:netcable_blue`) and **`xnet:connector`** (item
  `xnet:connector_blue`) — the run of cable and connectors. The controller owns the network of the
  connector it touches, as upstream's `findNeighbourConnector` requires.
- **The transfer** — an extractor connector hands one item per operation to the channel's
  inserters, highest priority first; a fresh connector moves one item every 10 ticks (upstream's
  default). An item nothing accepts stays where it is.
- **The commands** — `/xnet info` reads a controller's network, channels and connector sides, or a
  connector's channels; `/xnet channel|connector|remove|speed|priority|filter|unfilter|blacklist`
  edit a channel (operator level 2), standing in for the controller screen.

## Not yet ported

- **A block entity cannot be ticked from outside, and a write inside its tick is reverted** → the
- **No network/graph helper** (ARCHITECTURE.md §3) → the port walks its own adjacency, as
- **No helper to move a stack atomically** → `native_move_items` moves one item at a time and puts
- **Energy, fluid and logic channels**, and the controller's RF cost with them (the `block_entity_energy`
- **The advanced connector** (faster interval list from 5 ticks, 500,000 FE).
- **The controller and connector screens**, including the channel copy/paste and the filter slots.
- **The other four cable colours** and the routing colour with routers and wireless routers.
- **Antennas, facades and the redstone proxy** (the jar's `antenna`, `antenna_base`,

## Installing

1. Install the [Horizon launcher](https://github.com/horizon-rust/horizon/releases) (0.2.70 or newer).
2. Add `xnet.hmod` to the instance's mods.
3. The package is pinned to a specific Horizon build; the launcher checks that pin.

## Licence and credit

XNet was created by its upstream authors; the code, models and assets in the original mod
remain theirs and are used here under **MIT**. The Rust in this repository is a clean-room
rewrite for Horizon, not a translation of the original source.

- `LICENSE` — the upstream licence text.
- `LICENSE-NOTICE.md` — what this port carries and where it came from.
- The upstream project: https://github.com/McJtyMods/XNet

## Technical details

```
horizon-mod.toml   package manifest (id, version, api, [native] pin)
Cargo.toml         the crate; depends on `horizon-mod-api` from the fetched SDK
src/               lib.rs (the one `impl Hooks`), runtime.rs (pure logic + tests), glue
assets/ data/      the content the package ships
tools/             content generators and the e2e scenario
```

- Build: `cargo horizon-mod build` (packages; it does not compile against the game).
- Test: `cargo test`, `cargo clippy --all-targets -- -D warnings`.
- In-game: `ports/tools/launcher-game.sh run xnet` in the campaign tree, or install the
  `.hmod` and play; the harness scenario is `tools/e2e.sh`.

This repository is generated from the campaign's port at `ports/xnet/`; the port's own
`notes.md` is the full record (decisions, traps, evidence).
