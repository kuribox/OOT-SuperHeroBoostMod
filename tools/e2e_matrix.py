"""End-to-end test of the companion app: install each configuration with the built exe (headless), boot the game,
perform actions with injected input, and check the stamina behaviour against what the configuration promises.

Usage: python tools/e2e_matrix.py [scenario-name ...]    Results: build/e2e/results.txt
"""
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXE = ROOT / "release" / ("SHB-OcarinaTweaks.exe" if sys.platform == "win32" else "SHB-OcarinaTweaks")
EMU = os.environ.get("SHB_EMU", "E:/Switch2/Emu/Fritchii-v4.1-Windows-x64/Fritchii-v4.1")
OUT = ROOT / "build" / "e2e"
KEYS = ("jump", "attack", "sprint", "block", "shoot", "roll")

# action -> (steps, index of the measured step, event counter index or None)
ACTIONS = {
    "attack": (["Y:8"], 0, 2),
    "jump": (["X:8"], 0, 1),
    "roll": ([":40@0/1", "A:6@0/1"], 1, 0),
    "sprint": ([":30@0/1", "A:120@0/1"], 1, None),
    "shoot": (["ZR:100"], 0, 4),
}


def scenario(name, bm=True, stamina=True, preset=0, off=(), combat_only=False):
    return dict(name=name, bm=bm, stamina=stamina, preset=preset, off=set(off), combat_only=combat_only)


ALL = ["jump", "attack", "sprint", "block", "shoot", "roll"]
SCEN = [
    scenario("bm_hardcore"),
    scenario("no_bm_hardcore", bm=False),
    scenario("stamina_off", stamina=False),
    scenario("hardcore_combat_only", preset=1),
    scenario("sprint_only", preset=2),
    scenario("adv_jump_only", preset=3, off=[k for k in ALL if k != "jump"]),
    scenario("adv_attack_only", preset=3, off=[k for k in ALL if k != "attack"]),
    scenario("adv_sprint_only", preset=3, off=[k for k in ALL if k != "sprint"]),
    scenario("adv_shoot_only", preset=3, off=[k for k in ALL if k != "shoot"]),
    scenario("adv_roll_only", preset=3, off=[k for k in ALL if k != "roll"]),
    scenario("adv_block_only", preset=3, off=[k for k in ALL if k != "block"]),
    scenario("adv_all_combat_only", preset=3, combat_only=True),
]


def flags(sc):
    """what the configuration promises: (enabled per action, combat-only)"""
    if not sc["stamina"]:
        return {k: False for k in KEYS}, False
    p = sc["preset"]
    if p == 0:
        return {k: True for k in KEYS}, False
    if p == 1:
        return {k: True for k in KEYS}, True
    if p == 2:
        return {k: k == "sprint" for k in KEYS}, False
    return {k: k not in sc["off"] for k in KEYS}, sc["combat_only"]


def expected(sc, action, combat):
    en, co = flags(sc)
    if action == "sprint":
        return en["sprint"]                       # sprinting is never limited to combat
    return en[action] and (not co or combat)


def run(cmd, timeout):
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, cwd=ROOT)


def one(sc):
    subprocess.run(["powershell", "-NoProfile", "-Command", "Get-Process lichii -ErrorAction SilentlyContinue | Stop-Process -Force"])
    time.sleep(3)
    args = [str(EXE), "--cli", "--emu", EMU, "--preset", str(sc["preset"])]
    if not sc["bm"]:
        args.append("--no-button-mapping")
    if not sc["stamina"]:
        args.append("--no-stamina")
    if sc["preset"] == 3:
        if sc["off"]:
            args += ["--off", ",".join(sorted(sc["off"]))]
        if sc["combat_only"]:
            args.append("--combat-only")
    r = run(args, 120)
    time.sleep(4)
    ok = "installed in" in r.stdout
    # symbols of the installed module (one variant)
    run([sys.executable, "tools/build_core.py", "--features", "stamina,songbook,button_mapping", "--slots", "--compat",
         "--base", "0x4F80000"], 300)
    acts = list(ACTIONS)
    steps, pos = [], {}
    for a in acts:
        st, meas, ev = ACTIONS[a]
        pos[a] = len(steps) + meas
        steps += st
    out = run([sys.executable, "tools/live_check.py", "--press", ",".join(steps), "--linger", "1"], 900).stdout
    (OUT / f"{sc['name']}.log").write_text(out, encoding="utf-8")
    shutil.copy2(ROOT / "build" / "live_before.png", OUT / f"{sc['name']}.png")
    cap = float(re.search(r"stamina \(first float\): ([\-0-9.e]+)", out).group(1))
    blocks = re.split(r"\npress ", "\n" + out)[1:]
    meas = []
    for b in blocks:
        later = re.search(r"stamina 0\.7s later: ([\-0-9.e]+)", b)
        right = re.search(r"stamina right after press: ([\-0-9.e]+)", b)
        comb = re.search(r"combat (\d+) exhausted (\d+) counts rolls/jumps/runframes/attacks (\d+)/(\d+)/(\d+)/(\d+)", b)
        ev = re.search(r"events roll/jump/attack/guard/shot ([\d/]+)", b)
        meas.append(dict(later=float(later.group(1)) if later else None, right=float(right.group(1)) if right else None,
                         combat=int(comb.group(1)) if comb else None, runs=int(comb.group(5)) if comb else None,
                         ev=[int(x) for x in ev.group(1).split("/")] if ev else None))
    lines, bad = [], 0
    for a in acts:
        st, m, evi = ACTIONS[a]
        i = pos[a]
        if i >= len(meas) or meas[i]["later"] is None:
            lines.append(f"  {a:7s} NO DATA")
            bad += 1
            continue
        cur = meas[i]
        before = meas[i - 1] if i > 0 else None
        base_ev = before["ev"] if before and before["ev"] else [0] * 5
        triggered = True
        if evi is not None and cur["ev"]:
            triggered = cur["ev"][evi] > base_ev[evi]
        if a == "sprint":
            runs_delta = cur["runs"] - (before["runs"] if before and before["runs"] is not None else 0)
            observed = runs_delta > 5 or min(cur["right"], cur["later"]) < cap - 1.5
        else:
            observed = min(cur["right"], cur["later"]) < cap - 1.5
        combat = bool(cur["combat"])
        exp = expected(sc, a, combat)
        verdict = "ok" if (not triggered) or observed == exp else "MISMATCH"
        if verdict == "MISMATCH":
            bad += 1
        note = "" if triggered else "  (action did not trigger: not judged)"
        lines.append(f"  {a:7s} expected {'cost' if exp else 'free'}  observed {'cost' if observed else 'free'}  combat={int(combat)}  {verdict}{note}")
    head = f"[{'OK ' if bad == 0 and ok else 'BAD'}] {sc['name']}  (installed: {ok})"
    return head + "\n" + "\n".join(lines) + "\n"


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    sel = sys.argv[1:]
    res = OUT / "results.txt"
    if not sel:
        res.write_text("", encoding="utf-8")
    for sc in SCEN:
        if sel and sc["name"] not in sel:
            continue
        try:
            txt = one(sc)
        except Exception as e:
            txt = f"[ERR] {sc['name']}: {e}\n"
        print(txt, flush=True)
        with open(res, "a", encoding="utf-8") as f:
            f.write(txt)


if __name__ == "__main__":
    main()
