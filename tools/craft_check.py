#!/usr/bin/env python3
"""Craft the XNet recipes the package carries, in a real crafting table.

Called by `tools/e2e.sh` with the open game's control socket. The harness's
crafting table exposes the 3x3 grid as slots 100-108 (row-major) and the result
as 200 (`ctl screen`). For each recipe this:

1. resets the table and clears the inventory;
2. gives exactly the pattern's ingredients;
3. places them with right-clicks (one item per slot);
4. asserts the result's **id and count**;
5. shift-clicks the result into the inventory and asserts it arrived;
6. asserts the grid is empty and the distinctive ingredients were consumed.

The controller's recipe needs `rftoolsbase:machine_frame`, which this package
does not ship (upstream declares it a mandatory dependency too), so that one is
checked the other way round: with every other ingredient in place and a stick in
the frame's slot the table must craft nothing. A table that crafts whatever it
is handed fails that control.

    python3 tools/craft_check.py SOCK CTL X Y Z
"""
import json
import subprocess
import sys

RESULT = 200


def main() -> int:
    sock, ctl, x, y, z = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5]

    def run(*args):
        out = subprocess.run([ctl, "--socket", sock, *args], text=True, capture_output=True)
        try:
            return json.loads(out.stdout)
        except json.JSONDecodeError:
            raise SystemExit(f"ctl {' '.join(args)}: {out.stdout}{out.stderr}")

    def wait(ticks=4):
        run("wait", str(ticks))

    def slots():
        screen = run("screen")
        if "slots" not in screen:
            raise SystemExit(f"no menu is open: {screen}")
        return screen["slots"]

    def find(item):
        for slot in slots():
            if slot.get("item") == item:
                return slot["slot"]
        raise SystemExit(f"missing ingredient {item} in the inventory")

    def result():
        stack = [s for s in slots() if s["slot"] == RESULT]
        if not stack or not stack[0].get("item"):
            return None
        return stack[0]

    def grid_is_empty():
        return all(not s.get("item") for s in slots() if 100 <= s["slot"] <= 108)

    def count(item):
        return run("inventory").get("counts", {}).get(item, 0)

    def place(item, targets):
        # One pickup per target: a max-stack-1 ingredient cannot fill two slots
        # from one held stack, while a stack of four can.
        for target in targets:
            slot = find(item)
            run("click-slot", str(slot))
            wait(1)
            run("click-slot", str(target), "right")
            wait(1)
            run("click-slot", str(slot))
            wait(1)

    def setup(ingredients):
        try:
            run("close")
        except SystemExit:
            pass
        wait(4)
        # Replace the table so its block entity, and therefore the grid, is fresh.
        run("setblock", x, y, z, "minecraft:air")
        run("setblock", x, y, z, "minecraft:crafting_table")
        wait(2)
        run("command", f"tp @s {float(x) - 0.5} {y} {float(z) + 0.5}")
        wait(3)
        run("command", "clear @s")
        for item, n in ingredients:
            run("give", item, str(n))
        wait(3)
        run("interact", x, y, z)
        wait(5)

    failures = []

    def check(label, ingredients, placements, want_item, want_count, consumed):
        setup(ingredients)
        for item, targets in placements:
            place(item, targets)
        got = result()
        print(f"  {label} -> {json.dumps(got)}")
        if not got or got.get("item") != want_item or got.get("count") != want_count:
            grid = [s for s in slots() if 100 <= s["slot"] <= 108]
            print(f"    grid: {json.dumps(grid)}")
            failures.append(f"{label}: expected {want_count} {want_item}, got {got}")
            return
        run("click-slot", str(RESULT), "shift")
        wait(4)
        made = count(want_item)
        if made < want_count:
            failures.append(f"{label}: the crafted stack reached the inventory as {made}")
        if not grid_is_empty():
            failures.append(f"{label}: the grid still holds ingredients after the craft")
        for item in consumed:
            left = count(item)
            if left != 0:
                failures.append(f"{label}: {item} was not consumed ({left} left)")
        run("command", "clear @s")
        wait(2)

    def empty_control(label, ingredients, placements):
        setup(ingredients)
        for item, targets in placements:
            place(item, targets)
        got = result()
        print(f"  {label} -> {json.dumps(got)}")
        if got is not None:
            failures.append(f"{label}: the control crafted {got['item']} x{got['count']}")

    # netcable_blue: s 1 s / r g r / s r s -> 16
    check(
        "16 cables: 4 string + blue dye + gold nugget + 3 redstone",
        [
            ("minecraft:string", 4),
            ("minecraft:blue_dye", 1),
            ("minecraft:gold_nugget", 1),
            ("minecraft:redstone", 3),
        ],
        [
            ("minecraft:string", [100, 102, 106, 108]),
            ("minecraft:blue_dye", [101]),
            ("minecraft:gold_nugget", [104]),
            ("minecraft:redstone", [103, 105, 107]),
        ],
        "xnet:netcable_blue",
        16,
        ["minecraft:string", "minecraft:gold_nugget"],
    )

    # connector_blue: 1 C 1 / r g r / 1 r 1 -> 1
    check(
        "connector: 4 blue dye + chest + gold ingot + 3 redstone",
        [
            ("minecraft:blue_dye", 4),
            ("minecraft:chest", 1),
            ("minecraft:gold_ingot", 1),
            ("minecraft:redstone", 3),
        ],
        [
            ("minecraft:blue_dye", [100, 102, 106, 108]),
            ("minecraft:chest", [101]),
            ("minecraft:gold_ingot", [104]),
            ("minecraft:redstone", [103, 105, 107]),
        ],
        "xnet:connector_blue",
        1,
        ["minecraft:chest", "minecraft:gold_ingot"],
    )

    # The chest slot names the `c:chests` tag, so its other members match too and
    # a block outside it does not: that is the tag, not one hard-coded item.
    check(
        "connector with a trapped chest (the same c:chests tag)",
        [
            ("minecraft:blue_dye", 4),
            ("minecraft:trapped_chest", 1),
            ("minecraft:gold_ingot", 1),
            ("minecraft:redstone", 3),
        ],
        [
            ("minecraft:blue_dye", [100, 102, 106, 108]),
            ("minecraft:trapped_chest", [101]),
            ("minecraft:gold_ingot", [104]),
            ("minecraft:redstone", [103, 105, 107]),
        ],
        "xnet:connector_blue",
        1,
        ["minecraft:trapped_chest"],
    )
    empty_control(
        "connector with stone instead of a chest -> nothing",
        [
            ("minecraft:blue_dye", 4),
            ("minecraft:stone", 1),
            ("minecraft:gold_ingot", 1),
            ("minecraft:redstone", 3),
        ],
        [
            ("minecraft:blue_dye", [100, 102, 106, 108]),
            ("minecraft:stone", [101]),
            ("minecraft:gold_ingot", [104]),
            ("minecraft:redstone", [103, 105, 107]),
        ],
    )
    # `c:dyes/blue` is the blue dye, not any dye.
    empty_control(
        "cable with red dye -> nothing",
        [
            ("minecraft:string", 4),
            ("minecraft:red_dye", 1),
            ("minecraft:gold_nugget", 1),
            ("minecraft:redstone", 3),
        ],
        [
            ("minecraft:string", [100, 102, 106, 108]),
            ("minecraft:red_dye", [101]),
            ("minecraft:gold_nugget", [104]),
            ("minecraft:redstone", [103, 105, 107]),
        ],
    )

    # The controller needs rftoolsbase:machine_frame, which is not in this
    # package: with a stick in its slot the table must craft nothing.
    empty_control(
        "controller without its machine frame -> nothing",
        [
            ("minecraft:repeater", 2),
            ("minecraft:comparator", 1),
            ("minecraft:redstone", 2),
            ("minecraft:iron_ingot", 2),
            ("minecraft:gold_ingot", 1),
            ("minecraft:stick", 1),
        ],
        [
            ("minecraft:repeater", [100, 102]),
            ("minecraft:comparator", [101]),
            ("minecraft:redstone", [103, 105]),
            ("minecraft:iron_ingot", [106, 108]),
            ("minecraft:gold_ingot", [107]),
            ("minecraft:stick", [104]),
        ],
    )

    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    print("  every carried recipe crafts as declared; the controller is blocked on rftoolsbase")
    return 0


if __name__ == "__main__":
    sys.exit(main())
