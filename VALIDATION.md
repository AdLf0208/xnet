# XNet slice 1 — validation evidence

> The 0.1.0 record (the retired `.hnmod`). The current package is **0.2.0**, the SDK format; its
> ladder is in [`notes.md`](notes.md#migration-to-the-sdk-format) §6, with the logs in
> `ports/_shared/evidence/xnet/sdk-*.log`.

Every step below ran through `ports/tools/build-slot.sh` (a lease per step,
`CARGO_INCREMENTAL=0`, no bare cargo, no `--target-dir`), on pin
**`1d63e0a5a37a7a708eb0b32b49bf0dfeb28ed8a9`** with mixins
**`5a64af938ec86b00d53d16a408aabc0038fb3e57`** (schema 6). Logs are in
[`../_shared/evidence/xnet/`](../../_shared/evidence/xnet/).

## Ladder (cheapest first)

| # | Step | Command | Evidence | Result |
|---|---|---|---|---|
| 1 | runtime RED | `cargo test --locked --offline` (port) | `runtime-red.log` | `exit=101`, 151 `error[E…]` — the API did not exist yet |
| 2 | runtime GREEN | `cargo fmt --all --check`, `cargo clippy --all-targets --locked --offline -- -D warnings`, `cargo test --locked --offline` | `runtime-green.log` | `fmt_exit=0`, `clippy_exit=0`, **10 passed**, 0 failed |
| 3 | stage | `python3 tools/horizon.py stage` | the plan's 2 modules / 5 injections | `stage_changed: true`, 93 s |
| 4 | check | `python3 tools/horizon.py check` | `check.log` + `check-cargo.log` | `cargo_exit: 0`, **0 warnings** |
| 5 | embedded tests | `python3 tools/horizon.py test` (`--lib game::xnet::tests`) | `test-embedded.log` + `test-embedded-cargo.log` | **10 passed**, 0 failed, 1240 filtered |
| 6 | fast build | `python3 tools/horizon.py build` | `build.log` + `build-cargo.log` | `cargo_exit: 0`, 0 warnings, 10m21s |
| 7 | content carry | `python3 tools/carry_content.py --check` | terminal | `ok 40 files, 31074 bytes` |
| 8 | packages | `tools/package.sh` | `package.log` | `.hmod` 29,775 B / 41 entries; `.hnmod` 34,383 B / 12 entries; both pass `cargo horizon-mod check` |
| 9 | content load | `tools/hmod-load.sh` (shared game, `--dev-mod`) | `e2e-hmod.log` | PASS + EXIT:0, with the no-`--dev-mod` negative control in the same log |
| 10 | e2e | `build-slot.sh e2e -- tools/e2e.sh` (`EMBEDDED=1 CONTROL=1`) | `e2e.log` | **PASS + EXIT:0** |
| 11 | selftest | `SELFTEST=1 CONTROL=0 tools/e2e.sh` | `e2e-selftest.log` | **FAIL: SELFTEST: the channel moved 16 stone in 160 ticks, so the check is alive**, EXIT:1 |
| 12 | log audit | `python3 ports/tools/check-e2e-logs.py xnet --strict` | terminal | `logs=3 ok=3 missing-exit=0 bad-exit=0`, exit 0 |
| 13 | skills gate | `python3 ports/tools/check-skills.py --notes ports/xnet` | terminal | `ok xnet/notes.md: Skills line present`, exit 0 |

## Packages

| File | Size | SHA-256 | Entries |
|---|---:|---|---:|
| `content/xnet.hmod` | 29,775 B | `35401f7a6698b5a9ce9f5b3d0b92aa7a2fbbfe0f6290585c18ebf35ab45c5a2c` | 41 |
| `mixins/xnet/data/xnet-content.hmod` | 29,775 B | the same (the embedded copy is the built content, byte for byte) | 41 |
| `xnet.hnmod` | 34,383 B | `f7fd98c935b2d743e6aa6936ba294ae04da54afc4683d8dafbf56829579ea13c` | 12 |

`python3 tools/package.py` re-run gives the same bytes, so the bundle is deterministic. The
`.hnmod` carries `mixins/xnet/{LICENSE,NOTICE.md}` and
`mixins/xnet/data/xnet-content.hmod`.

## What the e2e proves, and how it could fail

| Leg | Claim | How it could fail |
|---|---|---|
| control | without the embedded file the ids are unknown | if the content came from anywhere else, `setblock`/`give` would resolve — the log shows `Unknown block type` / `Unknown item` |
| A | 20 stone move at one item per 10 ticks | 160 ticks moved **15** in the recorded run (16 in the earlier one); the assertion is 13..19, and `SELFTEST=1` fails when it is not 0 |
| A2 | `/xnet speed 40` gives one item per 40 ticks | 200 ticks moved 5 (3..7 allowed); at 10 ticks it would be ~20 |
| A3 | a filter decides what the extractor takes | a whitelist for dirt moved 0; clearing it emptied the source; a blacklist for stone moved 0 and left the source intact |
| B | a cable with no controller moves nothing | the source kept its 4 stone |
| B2 | a controller touching only a cable has no network | `/xnet info` says `network none`, and the source kept its 4 stone although the channel was configured |
| C | a full destination loses nothing | the source kept 4 stone, the target took 0 stone and kept all 1,728 dirt |
| C2 | the higher-priority inserter takes the items | with priorities 5 and 0 the south chest got 4 and the east 0; after raising the east to 9 it got 4 |
| D | the channel, its sides and its interval survive a relaunch | after quitting and relaunching, the readout still shows both entries at 10 ticks and 4 more stone moved |
| E | one file installs both halves | the `.hnmod` carries the content, and the only `.hmod` in the run's home is the embedded copy |

## Known-unproven in this slice

**The controller's RF cost** is not implemented (see `notes.md`, deviations): the pin offers no
  way to charge another block entity in a test, so the item channel runs unpowered here.
**The block models** are simple (a 4-pixel cable core and arms, a 6-pixel connector core, a cube
  controller); no screenshot leg checks them. `render_shape` is `MODEL` and the blockstates are
  multipart over the six joined sides.
