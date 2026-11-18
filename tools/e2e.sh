#!/usr/bin/env bash
# In-game check for XNet slice 1: one item channel moving items from the chest
# behind one connector to the chest behind another, through the controller and a
# cable run, at the jar's own rate.
#
# The End arena (stone floor at y=99, everything at y=100): an empty level keeps
# ambient item entities out of the container counts.
#
#  A  chest (0) - connector (1) - cable (2) - cable (3) - connector (4) - chest (5),
#     controller at (1,100,1) touching connector (1,100,0) — upstream's rule: a
#     controller owns the network of a *connector* it touches. Channel 0 is an
#     item channel with connector (1) west = extract and connector (4) east =
#     insert. 20 stone move one item per 10-tick operation (the jar's default:
#     `ItemConnectorSettings.speed = 2`, the screen's 10 ticks).
#  A2 `/xnet speed … 40` slows the same extractor to one item per 40 ticks.
#  A3 a whitelist filter that names dirt moves nothing; clearing it moves again;
#     a blacklist naming stone moves nothing either.
#  B  chest (7) - connector (8) - cable (9) - connector (10) - chest (11), NO
#     controller: nothing moves (control: a cable with no controller is inert).
#  B2 chest (13) - connector (14) - cable (15) - cable (16) - connector (17) -
#     chest (18) with the controller at (15,100,1) touching only a *cable*:
#     nothing moves (control: upstream's `findNeighbourConnector` wants a
#     connector, so this controller has no network at all).
#  C  chest (23) - connector (24) - cable (25) - cable (26) - connector (27) -
#     chest (28, full of dirt): nothing moves and the source keeps its items
#     (control: a refused item stays in its slot).
#  D  a relaunch keeps the channel, its sides and its interval with the controller.
#  E  one package: the delivered `xnet.hmod` (the SDK format) carries the content
#     under `assets/` and `data/` and the code under `src/`, and the run installed
#     it as the one mod of the game — there is no second content file to load.
#
# The SDK format: a native package is compiled into the client by the launcher
# (`horizon-launcher native-build <port> --instance <i> --trust-native-code`), so
# this scenario needs that game: `GAME=<launcher-built horizon-client>
# tools/e2e.sh`. There is no `--dev-mod` and no `.hnmod` any more, and nothing is
# written to `.horizon-cache`.
#
# SELFTEST=1 flips A's first assertion (the target must still be empty), so the
# scenario must FAIL; that proves the check can fail.
#
# Every mod command is typed with `ctl chat "/cmd"` because `ctl command` runs
# the console path and bypasses the native command hook.
set -euo pipefail
export LC_ALL=C

CLONE=/home/adlf/Desktop/all_the_mods_10_rust_rewrite
PORT=$CLONE/ports/xnet
CTL=${CTL:-$CLONE/ports/_shared/bin/horizon-1d63e0a5/horizon-ctl}
GAME=${GAME:-}
RES=${RES:-${HORIZON_RES:-/home/adlf/Desktop/horizon/.local/horizon-client/26.3}}
SOCK=${SOCK:-/tmp/opencode/xnet-e2e.sock}
SELFTEST=${SELFTEST:-0}
KEEP=${KEEP:-0}
WORLD=XNetE2E

[ -x "$CTL" ] || { echo "missing horizon-ctl: $CTL" >&2; exit 1; }
[ -n "$GAME" ] || {
    echo "missing GAME: this scenario needs a game the launcher built with the package" >&2
    echo "  horizon-launcher native-build $PORT --instance <i> --trust-native-code" >&2
    exit 1
}
[ -x "$GAME" ] || { echo "missing horizon-client: $GAME" >&2; exit 1; }
[ -d "$RES/assets" ] || { echo "missing prepared resources: $RES" >&2; exit 1; }


SCRATCH=$(mktemp -d /tmp/opencode/xnet-home.XXXXXX)
export HORIZON_HOME=$SCRATCH/game
export HORIZON_LAUNCHER_HOME=$SCRATCH/launcher
export XDG_DATA_HOME=$SCRATCH/data
export XDG_CONFIG_HOME=$SCRATCH/config
export HORIZON_NO_AUDIO=1
mkdir -p "$(dirname "$SOCK")" "$XDG_CONFIG_HOME"
rm -f "$SOCK"

ctl() { "$CTL" --socket "$SOCK" "$@"; }
say() { ctl chat "/$1" >/dev/null; }
fail() { echo "FAIL: $*" >&2; ctl logs --lines 80 >&2 || true; exit 1; }

# count X Y Z ITEM: how many of ITEM the container at X,Y,Z holds.
count() {
    ctl container "$1" "$2" "$3" \
        | jq -r --arg i "$4" '[.slots[] | select(.item == $i) | .count] | add // 0'
}

# The `/xnet info` lines for a block, echoed and matched by TEXT.
expect_info() { # expect_info X Y Z TEXT
    say "xnet info $1 $2 $3"
    ctl wait 3 >/dev/null
    lines=$(ctl messages 12 | jq -r '.chat[]' | grep "at $1,$2,$3:" | head -8)
    echo "$lines" | sed 's/^/  /'
    echo "$lines" | grep -q -- "$4" || fail "expected /xnet info to say '$4'"
}

# Move COUNT stone into the source chest and wait for the transfer to finish.
refill() { # refill X Y Z COUNT
    ctl container "$1" "$2" "$3" minecraft:stone "$4" >/dev/null
    ctl wait 4
}

cleanup() {
    local status=$?
    ctl quit >/dev/null 2>&1 || true
    sleep 1
    if [ "$KEEP" = 1 ]; then
        echo "kept scratch: $SCRATCH" >&2
    else
        rm -rf "$SCRATCH" 2>/dev/null || true
    fi
    echo "EXIT:$status"
}
trap cleanup EXIT

launch() { # launch [extra args]
    rm -f "$SOCK"
    cd "$SCRATCH"
    local args=(--game "$GAME" --world "$WORLD" --seed 20261105 --creative
                --headless --wait --resources "$RES")
    echo "  launch: no --dev-mod; the launcher built the game with the package in it" >&2
    "$CTL" --socket "$SOCK" launch "${args[@]}" "$@" >/dev/null
    ctl wait-ready 120 >/dev/null
    ctl wait 40
}

# A stone floor at y=99 in the End with clear air above, the player on it.
arena() {
    ctl command "execute in minecraft:the_end run tp @s 0.5 100 3.5"
    local loaded=false
    for _ in $(seq 1 120); do
        ctl wait 5
        if [ "$(ctl block 0 99 0 | jq -r '.loaded // false')" = true ]; then
            loaded=true
            break
        fi
    done
    [ "$loaded" = true ] || fail "the End arena chunk never loaded"
    ctl command "execute in minecraft:the_end run fill -8 99 -8 48 99 8 minecraft:stone"
    ctl command "execute in minecraft:the_end run fill -8 100 -8 48 108 8 minecraft:air"
    ctl command "execute in minecraft:the_end run kill @e[type=!player]"
    ctl wait 20
}

block_at() { # block_at X Z BLOCK
    ctl command "execute in minecraft:the_end run setblock $1 100 $2 $3"
}

echo "== launch (The End arena; the launcher built the package into the game) =="
launch
arena

echo "== build the networks =="
# A: chest - connector - cable - cable - connector - chest, controller on the connector.
block_at 0 0 minecraft:chest
block_at 1 0 xnet:connector
block_at 2 0 xnet:netcable
block_at 3 0 xnet:netcable
block_at 4 0 xnet:connector
block_at 5 0 minecraft:chest
block_at 1 1 xnet:controller

# B: the same run with no controller at all.
block_at 7 0 minecraft:chest
block_at 8 0 xnet:connector
block_at 9 0 xnet:netcable
block_at 10 0 xnet:connector
block_at 11 0 minecraft:chest

# B2: a controller that touches only a cable.
block_at 13 0 minecraft:chest
block_at 14 0 xnet:connector
block_at 15 0 xnet:netcable
block_at 16 0 xnet:netcable
block_at 17 0 xnet:connector
block_at 18 0 minecraft:chest
block_at 15 1 xnet:controller

# C: a full destination.
block_at 23 0 minecraft:chest
block_at 24 0 xnet:connector
block_at 25 0 xnet:netcable
block_at 26 0 xnet:netcable
block_at 27 0 xnet:connector
block_at 28 0 minecraft:chest
block_at 24 1 xnet:controller
ctl wait 8

for pos in "1 0" "4 0" "8 0" "10 0" "14 0" "17 0" "24 0" "27 0"; do
    set -- $pos
    state=$(ctl block "$1" 100 "$2" | jq -r '.state // .name // "unknown"')
    case "$state" in
        xnet:connector*) ;;
        *) fail "the connector at $1,100,$2 did not register: $state" ;;
    esac
done
for pos in "1 1" "15 1" "24 1"; do
    set -- $pos
    state=$(ctl block "$1" 100 "$2" | jq -r '.state // .name // "unknown"')
    case "$state" in
        xnet:controller*) ;;
        *) fail "the controller at $1,100,$2 did not register: $state" ;;
    esac
done
cable=$(ctl block 2 100 0 | jq -r '.state // .name // "unknown"')
echo "  cable at 2,100,0: $cable"
case "$cable" in xnet:netcable*) ;; *) fail "the cable did not register: $cable" ;; esac

refill 0 100 0 20
refill 7 100 0 4
refill 13 100 0 4
refill 23 100 0 4
for _ in $(seq 1 27); do ctl container 28 100 0 minecraft:dirt 64 >/dev/null; done
[ "$(ctl container 28 100 0 | jq '[.slots[] | select(. == null)] | length')" = 0 ] \
    || fail "the full target chest is not full"
ctl wait 4

echo "== A: the item channel moves 20 stone at the jar's default rate =="
say "xnet channel 1 100 1 0 item"
ctl wait 2 >/dev/null
say "xnet connector 1 100 1 0 1 100 0 west ext"
ctl wait 2 >/dev/null
say "xnet connector 1 100 1 0 4 100 0 east ins"
ctl wait 2 >/dev/null
expect_info 1 100 1 "network 1,100,0 2,100,0 3,100,0 4,100,0"
expect_info 1 100 1 "1 channel(s) enabled"
expect_info 1 100 1 "channel 0 item, 2 connector(s)"
expect_info 1 100 1 "1,100,0 west extract, 1 item every 10 ticks, priority 0, filter all"
expect_info 1 100 1 "4,100,0 east insert, 1 item every 10 ticks, priority 0, filter all"
expect_info 1 100 0 "channel 0 1,100,0 west extract"
echo "  source before: $(count 0 100 0 minecraft:stone), target before: $(count 5 100 0 minecraft:stone)"

# 160 ticks at one item per 10 ticks: 16 operations, plus or minus the phase.
before=$(count 5 100 0 minecraft:stone)
ctl wait 160 >/dev/null
after=$(count 5 100 0 minecraft:stone)
moved=$(( after - before ))
echo "  160 ticks moved $moved: source $(count 0 100 0 minecraft:stone), target $after"
if [ "$SELFTEST" = 1 ]; then
    [ "$moved" = 0 ] || fail "SELFTEST: the channel moved $moved stone in 160 ticks, so the check is alive"
else
    [ "$moved" -ge 13 ] || fail "only $moved items in 160 ticks, expected about 16"
    [ "$moved" -le 19 ] || fail "$moved items in 160 ticks: the channel is not one per 10 ticks"
fi
for _ in $(seq 1 120); do
    [ "$(count 5 100 0 minecraft:stone)" = 20 ] && break
    ctl wait 20 >/dev/null
done
echo "  source after: $(count 0 100 0 minecraft:stone), target after: $(count 5 100 0 minecraft:stone)"
[ "$(count 0 100 0 minecraft:stone)" = 0 ] || fail "the source chest is not empty"
[ "$(count 5 100 0 minecraft:stone)" = 20 ] || fail "the target chest did not receive all 20 stone"

echo "== A2: /xnet speed 40 slows the same extractor to one item per 40 ticks =="
refill 0 100 0 20
say "xnet speed 1 100 1 0 1 100 0 west 40"
ctl wait 3 >/dev/null
expect_info 1 100 1 "1,100,0 west extract, 1 item every 40 ticks"
before=$(count 5 100 0 minecraft:stone)
ctl wait 200 >/dev/null
after=$(count 5 100 0 minecraft:stone)
moved=$(( after - before ))
echo "  200 ticks at 40 ticks an operation moved $moved (expected about 5)"
[ "$moved" -ge 3 ] || fail "only $moved items in 200 ticks at 40 ticks an operation"
[ "$moved" -le 7 ] || fail "$moved items in 200 ticks at 40 ticks an operation"
say "xnet speed 1 100 1 0 1 100 0 west 10"
ctl wait 3 >/dev/null
expect_info 1 100 1 "1,100,0 west extract, 1 item every 10 ticks"

echo "== A3: a filter decides what the extractor takes =="
refill 0 100 0 4
say "xnet filter 1 100 1 0 1 100 0 west minecraft:dirt"
ctl wait 3 >/dev/null
expect_info 1 100 1 "filter minecraft:dirt"
say "xnet filter 1 100 1 0 1 100 0 west minecraft:cobblestone"
ctl wait 2 >/dev/null
expect_info 1 100 1 "filter minecraft:dirt minecraft:cobblestone"
say "xnet unfilter 1 100 1 0 1 100 0 west minecraft:cobblestone"
ctl wait 3 >/dev/null
expect_info 1 100 1 "1,100,0 west extract, 1 item every 10 ticks, priority 0, filter minecraft:dirt"
before=$(count 5 100 0 minecraft:stone)
ctl wait 100 >/dev/null
after=$(count 5 100 0 minecraft:stone)
echo "  whitelist dirt over 100 ticks moved $(( after - before ))"
[ "$(( after - before ))" = 0 ] || fail "a whitelist for dirt still took stone"
say "xnet filter 1 100 1 0 1 100 0 west -"
ctl wait 3 >/dev/null
expect_info 1 100 1 "1,100,0 west extract, 1 item every 10 ticks, priority 0, filter all"
for _ in $(seq 1 60); do
    [ "$(count 0 100 0 minecraft:stone)" = 0 ] && break
    ctl wait 20 >/dev/null
done
[ "$(count 0 100 0 minecraft:stone)" = 0 ] || fail "clearing the filter did not let the stone move"
echo "  filter cleared: source 0, target $(count 5 100 0 minecraft:stone)"

say "xnet filter 1 100 1 0 1 100 0 west minecraft:stone"
ctl wait 2 >/dev/null
say "xnet blacklist 1 100 1 0 1 100 0 west on"
ctl wait 3 >/dev/null
expect_info 1 100 1 "filter not minecraft:stone"
before=$(count 5 100 0 minecraft:stone)
refill 0 100 0 4
ctl wait 100 >/dev/null
after=$(count 5 100 0 minecraft:stone)
echo "  blacklist stone over 100 ticks moved $(( after - before )), source $(count 0 100 0 minecraft:stone)"
[ "$(( after - before ))" = 0 ] || fail "a blacklist for stone still took stone"
[ "$(count 0 100 0 minecraft:stone)" = 4 ] || fail "a blacklist for stone emptied the source"
say "xnet blacklist 1 100 1 0 1 100 0 west off"
ctl wait 2 >/dev/null
say "xnet filter 1 100 1 0 1 100 0 west -"
ctl wait 3 >/dev/null
expect_info 1 100 1 "1,100,0 west extract, 1 item every 10 ticks, priority 0, filter all"

echo "== B: a cable with no controller moves nothing (control) =="
ctl wait 100 >/dev/null
echo "  source: $(count 7 100 0 minecraft:stone), target: $(count 11 100 0 minecraft:stone)"
[ "$(count 7 100 0 minecraft:stone)" = 4 ] || fail "the controllerless source lost stone"
[ "$(count 11 100 0 minecraft:stone)" = 0 ] || fail "stone moved without a controller"

echo "== B2: a controller that touches only a cable has no network (control) =="
say "xnet channel 15 100 1 0 item"
ctl wait 2 >/dev/null
say "xnet connector 15 100 1 0 14 100 0 west ext"
ctl wait 2 >/dev/null
say "xnet connector 15 100 1 0 17 100 0 east ins"
ctl wait 3 >/dev/null
expect_info 15 100 1 "network none"
ctl wait 100 >/dev/null
echo "  source: $(count 13 100 0 minecraft:stone), target: $(count 18 100 0 minecraft:stone)"
[ "$(count 13 100 0 minecraft:stone)" = 4 ] || fail "the source lost stone to a controller with no network"
[ "$(count 18 100 0 minecraft:stone)" = 0 ] || fail "stone moved through a controller that touches only a cable"

echo "== C: a full destination refuses the items (control) =="
say "xnet channel 24 100 1 0 item"
ctl wait 2 >/dev/null
say "xnet connector 24 100 1 0 24 100 0 west ext"
ctl wait 2 >/dev/null
say "xnet connector 24 100 1 0 27 100 0 east ins"
ctl wait 3 >/dev/null
expect_info 24 100 1 "27,100,0 east insert"
ctl wait 100 >/dev/null
echo "  source: $(count 23 100 0 minecraft:stone), target stone: $(count 28 100 0 minecraft:stone), target dirt: $(count 28 100 0 minecraft:dirt)"
[ "$(count 23 100 0 minecraft:stone)" = 4 ] || fail "the source lost stone into a full target"
[ "$(count 28 100 0 minecraft:stone)" = 0 ] || fail "the full target took stone"
[ "$(count 28 100 0 minecraft:dirt)" = 1728 ] || fail "the full target lost its dirt"

echo "== C2: the higher-priority inserter takes the items =="
# A second destination on the same run: the connector at (32,100,1) hangs off the
# cable at (32,100,0) and serves the chest at (32,100,2) through its south side.
block_at 29 0 minecraft:chest
block_at 30 0 xnet:connector
block_at 31 0 xnet:netcable
block_at 32 0 xnet:netcable
block_at 33 0 xnet:connector
block_at 34 0 minecraft:chest
block_at 32 1 xnet:connector
block_at 32 2 minecraft:chest
block_at 30 1 xnet:controller
ctl wait 8
say "xnet channel 30 100 1 0 item"
ctl wait 2 >/dev/null
say "xnet connector 30 100 1 0 30 100 0 west ext"
ctl wait 2 >/dev/null
say "xnet connector 30 100 1 0 33 100 0 east ins"
ctl wait 2 >/dev/null
say "xnet connector 30 100 1 0 32 100 1 south ins"
ctl wait 2 >/dev/null
say "xnet priority 30 100 1 0 32 100 1 south 5"
ctl wait 3 >/dev/null
expect_info 30 100 1 "32,100,1 south insert, 1 item every 10 ticks, priority 5"
expect_info 30 100 1 "33,100,0 east insert, 1 item every 10 ticks, priority 0"
refill 29 100 0 4
for _ in $(seq 1 60); do
    [ "$(count 32 100 2 minecraft:stone)" = 4 ] && break
    ctl wait 20 >/dev/null
done
echo "  priority 5 wins: south $(count 32 100 2 minecraft:stone), east $(count 34 100 0 minecraft:stone)"
[ "$(count 32 100 2 minecraft:stone)" = 4 ] || fail "the priority-5 inserter did not take the items"
[ "$(count 34 100 0 minecraft:stone)" = 0 ] || fail "the priority-0 inserter took items a higher priority wanted"
say "xnet priority 30 100 1 0 33 100 0 east 9"
ctl wait 3 >/dev/null
expect_info 30 100 1 "33,100,0 east insert, 1 item every 10 ticks, priority 9"
refill 29 100 0 4
for _ in $(seq 1 60); do
    [ "$(count 34 100 0 minecraft:stone)" = 4 ] && break
    ctl wait 20 >/dev/null
done
echo "  priority 9 wins: south $(count 32 100 2 minecraft:stone), east $(count 34 100 0 minecraft:stone)"
[ "$(count 34 100 0 minecraft:stone)" = 4 ] || fail "the higher-priority inserter did not win after the change"
[ "$(count 32 100 2 minecraft:stone)" = 4 ] || fail "the lower-priority inserter took items the higher one wanted"

echo "== D: the channel, its sides and its interval survive a relaunch =="
ctl quit >/dev/null
sleep 2
launch
expect_info 1 100 1 "channel 0 item, 2 connector(s)"
expect_info 1 100 1 "1,100,0 west extract, 1 item every 10 ticks"
expect_info 1 100 1 "4,100,0 east insert, 1 item every 10 ticks"
before=$(count 5 100 0 minecraft:stone)
refill 0 100 0 4
for _ in $(seq 1 60); do
    [ "$(count 5 100 0 minecraft:stone)" = $(( before + 4 )) ] && break
    ctl wait 20 >/dev/null
done
echo "  after the relaunch: target $(count 5 100 0 minecraft:stone) (expected $(( before + 4 )))"
[ "$(count 5 100 0 minecraft:stone)" = $(( before + 4 )) ] || fail "the channel did not keep working across the relaunch"

echo "== E: one package — the .hmod carries the content and the code =="
[ ! -e "$PORT/xnet.hnmod" ] || fail "the retired .hnmod is still in the port"
python3 - "$PORT/xnet.hmod" "$PORT/src/runtime.rs" <<'PY'
import hashlib
import sys
import zipfile

package, source = sys.argv[1], sys.argv[2]
required = (
    "horizon-mod.toml",
    "Cargo.toml",
    "src/lib.rs",
    "src/network.rs",
    "src/runtime.rs",
    "assets/xnet/blockstates/controller.json",
    "data/xnet/horizon/block/controller.json",
)
with zipfile.ZipFile(package) as archive:
    names = archive.namelist()
    missing = [name for name in required if name not in names]
    if missing:
        raise SystemExit(f"  FAIL: {package} does not carry {', '.join(missing)}")
    packed = hashlib.sha256(archive.read("src/runtime.rs")).hexdigest()
on_disk = hashlib.sha256(open(source, "rb").read()).hexdigest()
if packed != on_disk:
    raise SystemExit(f"  FAIL: the packaged runtime.rs is {packed}, the source {on_disk}")
print(f"  {package}: {len(names)} entries, src/runtime.rs sha256 {packed}")
PY

echo "PASS: an item channel moved 20 stone through a cable run from one connector to the other at the jar's default one item per 10 ticks, /xnet speed 40 slowed it to one per 40 ticks, a whitelist and a blacklist filter each decided what it took, the higher-priority inserter took the items and the lower one only after the priorities changed, the channel survived a relaunch, a network with no controller / a controller that touches only a cable / a full destination moved nothing and lost nothing, and the one delivered .hmod carried the content and the code together, with the packaged runtime.rs byte-identical to its source"
