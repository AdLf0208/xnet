# XNet for Horizon — slice 1: the item channel

A Horizon port of [XNet](https://github.com/McJtyMods/XNet) 1.21-7.0.7 by McJty (MIT): a
controller, connectors and cables that move items between inventories on a channel.

One file installs both halves: **`xnet.hmod`** is the SDK package — the blocks, items, models,
textures, lang, recipes and loot under `assets/` and `data/`, and the Rust that runs them in
`src/`, compiled by the launcher against the pinned game build.

## What this slice does

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

## What it does not do yet

Energy, fluid and logic channels (and the controller's RF cost); the advanced connector; the
controller and connector screens; the other four cable colours, routers, wireless routers,
antennas, facades and the redstone proxy; stack/count modes, random/ordered extraction, round-robin
channels, the tag/meta/component filter toggles and the redstone gates. The full list, with the
reason for each, is in [`notes.md`](notes.md#slice-boundary-and-gaps).

## Building and testing

The package pins SDK nightly `0b14f8acfd97fa2c750e42795249dcc981178ca4` and horizon-mixins
`e59da9094f0c2fdda6c627fe9eff22dec6a906f0` (`horizon-mod.toml`). From this directory, with
`CARGO_INCREMENTAL=0` and no `--target-dir`:

```sh
cargo test                                   # the pure runtime: 10 tests, no game needed
cargo clippy --all-targets -- -D warnings
rustfmt --check --config skip_children=true --edition 2024 src/lib.rs src/network.rs

rm -f Cargo.lock                             # never package a lock (it changes the artifact)
cargo horizon-mod build                      # -> target/horizon-mod/xnet.hmod
cp target/horizon-mod/xnet.hmod ./xnet.hmod
cargo horizon-mod check xnet.hmod            # the registry's own check
cargo horizon-mod fits xnet.hmod --build nightly
```

The in-game scenario is [`tools/e2e.sh`](tools/e2e.sh); it needs a launcher-built game
(`GAME=<horizon-client>`) because a native package is compiled into the client by the launcher.

## Licence

MIT, as upstream. The upstream copyright and licence travel in [`LICENSE`](LICENSE) and
[`LICENSE-NOTICE.md`](LICENSE-NOTICE.md), and inside the `.hmod`. Local-only until the maintainer
approves publishing.
