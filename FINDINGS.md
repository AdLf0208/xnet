# XNet 1.21-7.0.7 — upstream findings (item channel)

Read from the public repository **<https://github.com/McJtyMods/XNet>**, branch `1.21_neo`,
commit **`559b1d959811cf04021b51810f353d8c048cd65e`** ("Bump RFToolsBase dependency",
2026-06-16), whose `gradle.properties` says `version=1.21-7.0.7` — the version in the pack jar
`ports/_shared/licence-cache/xnet-1.21-7.0.7.jar` (`META-INF/neoforge.mods.toml`:
`modId="xnet"`, `license="MIT License https://github.com/McJtyMods/XNet/blob/1.21_neo/LICENCE"`).
Clone kept at `ports/_shared/upstream/xnet` while the slice was written and deleted after the
notes were recorded. No Java was copied or translated and the jar was not decompiled; what
follows is behaviour, re-implemented in Rust against the pinned Horizon hooks.

## The network

- A **controller** is the provider of exactly one network. On placement and on a neighbour
  change it scans its six neighbours for a **connector** and inherits that connector's colour;
  it then registers itself as the network provider of that blob
  (`modules/controller/blocks/TileEntityController.java:1051-1111`,
  `findNeighbourConnector`). A controller that touches only a cable has **no** network and its
  channels never run.
- **Cables and connectors** form the network: two of them are joined when they are adjacent and
  of the same colour (`modules/cables/blocks/NetCableBlock.java:60-70`,
  `ConnectorBlock.getConnectorType:240-252`). Colours do not mix (`assets/rftoolsbase/…/simple/
  cables.json`: "Red, green, blue, and yellow cables do not connect to each other, even when
  placed next to each other"). The routing colour is a separate network used by routers.
- A **connector** is placed against the block it serves. It has six independently enabled
  sides (`modules/cables/data/ConnectorData.java:9`, `EMPTY = 0x3f` — all six on) and one
  `ConsumerId` per connector (`ConnectorTileEntity`, `IConnectorTile`). A side is *connectable*
  when the neighbour is a container, an energy block, a fluid handler, a redstone component or
  a controller (`ConnectorBlock.isConnectable:254-300`).
- Channel entries are keyed by **`SidedConsumer`** — a connector id **plus the side that faces
  the inventory** (`rftoolsbase` API, `ChannelInfo.connectors`). So one connector can serve two
  inventories on two sides with two independent settings.
- The controller keeps **8 channels** (`modules/controller/ChannelInfo.java:34`,
  `MAX_CHANNELS = 8`). A channel has a type (`none`/`item`/`energy`/`fluid`/`logic`), a mode
  (`PRIORITY`/`ROUNDROBIN`) and its connectors. `getConnectors` drops an entry whose connector
  is no longer on the controller's network (`TileEntityController:1027-1041`), so cutting the
  cable stops that connector.

## The item channel's rate (the number this slice had to get right)

- `ItemChannelSettings.tick:187-205`: the channel decrements a `delay` counter that resets to
  `200 * 6 = 1200`; it acts only when `delay % 5 == 0`, and then `d = delay / 5`. `d` falls by
  one every five ticks, so an extractor whose `d % speed == 0` runs **once every `5 × speed`
  ticks**.
- `ItemConnectorSettings` defaults (`:48-58`): `itemMode = INS`, `extractMode = FIRST`,
  **`speed = 2`**, `stackMode = SINGLE`, `priority = 0`, `extractAmount = null` (→ 1 at
  `:277-279`), `count = null`. So a fresh connector moves **one item every 10 ticks** — the
  number the slice and its e2e pin.
- The screen offers `Constants.SPEEDS` = 10, 20, … 200 **ticks** (`apiimpl/Constants.java:5-8`)
  and divides by five before storing (`ItemConnectorSettings.update:317`), so the GUI value is
  the tick interval; the advanced connector's list starts at 5 (`ADVANCED_SPEEDS`, `:11-14`).
- `StackMode` decides what one operation carries: `SINGLE` = 1, `STACK` = the item's max stack,
  `COUNT` = `extractAmount` (`ItemChannelSettings.fetchItem:421-444`).
- `tickItemHandler:267-319`: it fetches the first **matching** stack from the extractor's slots
  (wrapping from `startIdx`), inserts it, and **breaks after the first insert that moved
  anything**; a refused insert advances to the next slot and tries again. One operation
  therefore moves one stack, and an item nothing accepts is never taken.
- `insertStack:321-396`: destinations are the channel's `INS` connectors, **sorted by priority,
  highest first** (`updateCache:480`); `PRIORITY` mode always starts at the first one
  (`roundRobinOffset = 0`), `ROUNDROBIN` advances an offset per successful insert. A destination
  is skipped when its own matcher rejects the stack, when its redstone gate is closed, or when
  its `count` is already reached. `ItemHandlerHelper.insertItem` returns what did not fit, and
  only what was actually inserted is removed from the source.
- `getMatcher:223-248`: 18 ghost filter slots; an empty list matches everything; otherwise the
  filter list decides, `blacklist` inverts it, and `tagsMode`/`metaMode`/`componentMode` widen
  or narrow what counts as the same item.

## The controller's power

- `TileEntityController.tickServer:272-311`: the controller first returns if it is on more than
  one network (the `error` block state), then pays `controllerRFT` (0/tick), then per **enabled
  channel** pays `controllerChannelRFT` = **1 RF/tick**, and each operation pays
  `controllerOperationRFT` = **2 RF** (`setup/Config.java:141-147`). The buffer holds
  `controllerMaxRF` = 100,000 FE and takes `controllerRfPerTick` = 1,000/tick.
- `checkAndConsumeRF:316-327` refuses when the buffer is short, so an unpowered controller's
  channels do not tick at all.

## Content the jar carries for these three blocks

| id | kind | evidence |
|---|---|---|
| `xnet:controller` | block, states `error`, `facing` | `assets/xnet/blockstates/controller.json`; `ControllerModule.CONTROLLER`; item `xnet:controller` |
| `xnet:netcable` | block, states `color`, six `none/cable/block` arms, `waterlogged` | `blockstates/netcable.json`, `NetCableBlock`; **no item of its own** — `netcable_blue` (16 per craft) places it |
| `xnet:connector` | block, same states | `blockstates/connector.json`, `ConnectorBlock`; item `connector_blue` |
| `xnet:netcable_blue`, `xnet:connector_blue` | items | `models/item/netcable_blue.json`, `connector_blue.json`; `ColorBlockItem` sets the colour |
| recipes | `controller` (comparator, repeater, redstone, gold/iron ingots, `rftoolsbase:machine_frame`), `netcable_blue` (string, blue dye, gold nugget, redstone → 16), `connector_blue` (blue dye, chest, gold ingot, redstone) | `data/xnet/recipe/*.json` |
| loot | `netcable`/`connector` drop the item of their colour; `controller` drops itself with its data | `data/xnet/loot_table/blocks/*.json` |
| tags | `xnet:cables`, `xnet:connectors`, `minecraft:mineable/pickaxe` | `data/xnet/tags/item/*.json`, `data/minecraft/tags/block/mineable/pickaxe.json` |
| lang | `block.xnet.controller` = "Controller", `block.xnet.connector`, `item.xnet.connector_blue` = "Blue Connector", `item.xnet.netcable_blue` = "Blue Network Cable", `itemGroup.xnet` = "XNet" | `assets/xnet/lang/en_us.json` |

## What the slice does not take

Energy, fluid and logic channels; the advanced connector (`advanced_connector`, faster speed
list, 500,000 FE); routers, wireless routers, antennas and routing cables; facades; redstone
proxies; the controller and connector screens; `count` (stock keeping), `ROUNDROBIN` mode,
`RANDOM`/`ORDER` extract modes, and the tag/meta/component filter toggles. All are recorded in
`notes.md` as gaps of the slice, not of the mod.
