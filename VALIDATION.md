# XNet — validation evidence

Current package: **0.2.2** (slice 2, the item channel's amount and destination policy). Pin
**Horizon `0a875da91dbb0fdcb7ba8c42c11341aa229af51a`** + mixins
**`e59da9094f0c2fdda6c627fe9eff22dec6a906f0`** (Nightly 2026-11-20); the SDK is fetched into
`.horizon/sdk/` by every `cargo horizon-mod` command and is never packed. Logs for the current
version are in [`../_shared/evidence/xnet/`](../../_shared/evidence/xnet/) (`slice2-*.log`,
`sdk-test.log`) and [`../_shared/evidence/xnet-e2e/`](../../_shared/evidence/xnet-e2e/)
(`e2e-slice2*.log`).

## Ladder (0.2.2), cheapest first

| # | Step | Command | Evidence | Result |
|---|---|---|---|---|
| 1 | runtime RED | `cargo test` against a stub of 0.2.1's behaviour | `slice2-test-red.log` | 14 passed, **4 failed** (assertion failures, quoted in `notes.md`) |
| 2 | runtime GREEN | `cargo test` | `slice2-test.log`, `sdk-test.log` | **18 passed**, 0 failed |
| 3 | lints | `cargo clippy --all-targets -- -D warnings` | `slice2-clippy.log` | exit 0, **0 warnings** |
| 4 | formatting | `rustfmt --check --config skip_children=true --edition 2024 src/{lib,network,runtime}.rs` | `slice2-fmt.log` | `fmt_exit=0` |
| 5 | package | `rm -f Cargo.lock; cargo horizon-mod build`, twice | `slice2-build.log` | **`cmp` byte-identical**, 67,820 B |
| 6 | check | `cargo horizon-mod check xnet.hmod` | `slice2-check.log` | `xnet 0.2.2 (native-mod, format native-1/0a875da…/e59da909…, for 26.3)`, exit 0 |
| 7 | fits the pin | `cargo horizon-mod fits xnet.hmod --build 0a875da` | `slice2-fits.log` | `0a875da  made for` |
| 8 | fits another build | `cargo horizon-mod fits xnet.hmod --build 0b14f8a` | `slice2-fits.log` | `0b14f8a  fits` |
| 9 | package audit | `python3 ports/tools/verify-sdk-package.py xnet` | terminal | **1/1** |
| 10 | in game | `ports/tools/launcher-game.sh run xnet` (offscreen) | `xnet-e2e/e2e-slice2.log` | **PASS + `EXIT:0`** |
| 11 | control | `GAME=$CONTROL_GAME tools/e2e.sh` | `xnet-e2e/e2e-slice2-control.log` | `FAIL: the connector … did not register`, `EXIT:1` |
| 12 | selftest (A) | `SELFTEST=1 tools/e2e.sh` | `xnet-e2e/e2e-slice2-selftest.log` | `FAIL: SELFTEST: the channel moved 16 stone in 160 ticks…`, `EXIT:1` |
| 13 | selftest (slice 2) | `SELFTEST=amount tools/e2e.sh` | `xnet-e2e/e2e-slice2-selftest-amount.log` | `FAIL: one operation moved 2 of 20 items, not the whole slot`, `EXIT:1` |
| 14 | log audit | `python3 ports/tools/check-e2e-logs.py xnet-e2e --strict` | terminal | `logs=6 ok=6 missing-exit=0 bad-exit=0`, exit 0 |
| 15 | skills gate | `python3 ports/tools/check-skills.py --notes ports/xnet --strict` | terminal | `Skills line present`, PASS |

Every compile/test step ran through `ports/tools/build-slot.sh run --job W1-xnet --port ports/xnet
--step …` with `CARGO_INCREMENTAL=0` (the SDK layout has no staging step, so `build-slot.sh e2e`
cannot rebuild it — the leg is `run --step e2e` after `launcher-game.sh build`). The game legs ran
locally because the script launches `--offscreen` (the crafting phase's `click-slot` needs a
renderer; the far box has none).

## Package

| File | Size | SHA-256 | Entries |
|---|---:|---|---:|
| `xnet.hmod` | 67,820 B | `305e8976a5982ae8ea17deae0267f85befb0f737bf1da7eebe52e439289dfb67` | 53 |

No `Cargo.lock`, no `.horizon/`, no retired `.hnmod` path. Two builds from the same sources are
byte-identical, and the packaged `src/runtime.rs` is byte-identical to the tree's
(`6fb8646e08e3…`).

## What the in-game leg proves, and how each check could fail

| Leg | Claim | How it could fail |
|---|---|---|
| A | 20 stone move at one item per 10 ticks | 160 ticks moved 16; the assertion is 13..19, and `SELFTEST=1` fails when the count is not 0 |
| A2 | `/xnet speed 40` gives one item per 40 ticks | 200 ticks moved 5 (3..7 allowed) |
| A3 | a filter decides what the extractor takes | a whitelist for dirt moved 0; clearing it emptied the source; a blacklist for stone moved 0 and left the source intact |
| B, B2 | no controller, or a controller touching only a cable, moves nothing | the source kept its items; the readout says `network none` |
| C, C2 | a full destination loses nothing; priority decides | 4 stone stayed; priority 5 took all four and priority 0 none, then the reverse after the change |
| F1 | `StackMode.STACK` moves the whole slot | one operation moved 20 of 20; `SELFTEST=amount` leaves the mode at `single` and the same phase moves **2**, so that run fails — the counterfactual |
| F2 | `StackMode.COUNT` + amount 16 moves 16 an operation | 100 ticks moved 160 (single would be about 10) |
| F3 | an inserter's `count` stocks the destination | target 5, source 15; without the count the source empties |
| F4 | an extractor's `count` keeps items in the source | source 5, target 15; without the count the source empties |
| F5 | `ROUNDROBIN` uses both inserters, `PRIORITY` fills one | round robin: east 10, south 10; the same channel on priority: east 0, south 20 — the differential is inside the passing run |
| F6 | an unknown stack mode is refused and changes nothing | the readout still says `single` after `/xnet stack … sideways` |
| D | the channel, its sides, rate, stack mode and distribution mode survive a relaunch | both controllers still report their settings and move items afterwards |
| P | the carried items, recipes, loot and tags work | 3/3 items give and read back, both cable blocks refuse an item id, an unknown id is refused, the controller item places its block; the cable (16) and connector recipes craft with their ingredients consumed, a trapped chest crafts the same connector while stone does not, red dye crafts no cable, and the controller crafts nothing without `rftoolsbase:machine_frame`; 3/3 loot tables drop their item; `#xnet:cables`, `#xnet:connectors` and `#minecraft:mineable/pickaxe` match their members and not dirt |
| E | one package installs both halves | the packaged `src/runtime.rs` is byte-identical to the source and the required tag files are inside |

## Known-unproven in this port

- **The controller's RF cost** (energy channels) is not implemented: the pin offers no way to charge
  another block entity in a test, so the item channel runs unpowered (recorded deviation).
- **`xnet:netcable_blue` and `xnet:connector_blue` do not place their blocks.** A content item
  cannot be given a block link at this pin (`Item.block` is a native declaration), so the items are
  plain items and the blocks are placed by `/setblock` or the creative tab. The next slice.
- **The controller and connector screens** are absent; the `/xnet …` commands stand in for them.
- **The models** are simple (a 4-pixel cable core and arms, a 6-pixel connector core, a cube
  controller); no screenshot leg checks them.
