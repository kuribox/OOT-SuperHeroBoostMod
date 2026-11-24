# Switch target: hand-off notes (0.6.x, branch combat-targeting)

Goal: a button that moves the lock-on to **another** enemy. The game has no native switch and does not
auto-switch: the red marker it draws over an enemy is visual only.

## Environment

- Repo `F:\project\super-hero-boost`, branch `combat-targeting`, module `features/manual_shield/manual_shield.c`.
- Test emulator: `E:\Switch2\Emu\Fritchii-v4.1-Windows-x64\Fritchii-v4.1`, ROM `E:\Switch2\Roms\Zelda Ocarina.nup`,
  debug server on `127.0.0.1:6545`.
- Devtool: `F:\project\oot\vendor\devtool\oot-remake-devtool-main` (`lichii.DebugClient`, `HostMemory`, `lichii_pids`).
  `tools/live_check.py` has `PORT`, `log_addr("main")`, `symbols()` (from `build/native/module.elf`).
- Toolchain: `F:\project\oot\tools\llvm-mingw\llvm-mingw-20261104-ucrt-x86_64\bin` (no `llvm-mc`; `llvm-objdump`
  has no `-b binary`).
- Installed mod: `<emulator>\user\load\0400486024DEC000\zz-Super-Hero-Boost` (`exefs/subsdk9` + IPS).
- Live config (user): `manual_shield=1, target_r3=1, sprint_l3=1, auto_target=1, switch_target=0`,
  game option **Toggle** targeting.

## Build / test loop

```
python tools/make_release.py                      # builds app/payload (standalone + relink) and build/native
python tools/build_core.py --features controls,manual_shield,stamina,songbook,button_mapping,quick_select --slots
# patch the user's ShbConfig (build/ms_cfg.bin) into app/payload/standalone/subsdk9 at the SHBCFG01 offset,
# copy subsdk9 + ours.ips into the mod folder, kill lichii.exe and start it with
#   EDEN_NX2_DEBUG_SERVER=127.0.0.1:6545 lichii.exe -g "<ROM>"
```

`app-legacy/main.py --cli --emu <emulator> ...` installs with a profile, but that overwrites the ShbConfig; the
config-block patch above keeps the user's settings (76 bytes, offsets below).

## What already works

- Target latch: `g_ms_targeting` mirrors the game. `LockOn*` state = targeting; `Wait/Move/Walk/Brake/Turn/Idle`
  = not targeting; attacks (`CutS1`, `ChargeWait`), guard and hit states keep the previous value. Fixed the
  "L during an attack opened the item selector" bug.
- `auto_target`: when the game drops the target without the player pressing the button (enemy died), a short
  synthesised target press (ZL bit) locks the next enemy. Works.
- `switch_target`: cancel + walk towards the nearest other enemy + lock again. Prototype moved the lock from a
  3.13 m enemy to the 1.03 m one, but in real fights it often re-locks the same enemy. **Off by default.**

## What does not work

- No enemy actor pointer exists in the player state (`0x4000` bytes), the player actor (`0x1000`), the component
  table or its 85 components (first `0x2000` each) while locked.
- The player state **caches the target's positions** at `state+0x1BF0` (three points: `+0x1BF0` xyz, `+0x1BFC` same
  with y+1, `+0x1C08` xyz again, `+0x1C14` a second point of the target). The camera components copy it. It is
  rewritten every frame: writing another enemy's position has no effect beyond a few microseconds.
- The game does not switch targets with either stick (tested by injecting input), and pressing the target button
  again in Toggle mode cancels.

## Leads

1. **Code that touches the cached block.** In the running `main`, offset = `address - 0x80020000`, read at
   `log_addr("main") + offset`:
   - 32-bit stores at `+0x1BF0`: `main+0x1634FF8` (field copy inside one object: `ldr w12,[x26,#0x5c];
     str w12,[x26,#0x1bf0]` — discarded), `main+0x229B21C`, `main+0x36CAB70` (**not disassembled yet**; start here).
   - 64-bit loads/stores at `+0x1BF0`: `0x704430, 0x706EF0, 0x86522C, 0xC32F74, 0xD7A3B0, 0x21FFA14, 0x29BFC34,
     0x29BFC5C, 0x2E71EAC, 0x359A8A8, 0x36DA840, 0x3824FBC`.
2. **The game's own target-slot commands** (`symbols/e0f3ccfa-commands.csv` in
   https://github.com/Kabosune/oot-remake-modding, cloned at `%TEMP%\opencode\ootmod`):
   - `OneShotTargetSetFromActor`: vtable `0x84a60a68`, binder `0x805b9f20` (`main+0x59f20`), execute `main+0x88edd0`
     (verified `pacibsp`). Entry reads `[x0+0x28] -> [..]`, checks `w8 == 1` at `+0x10`, clears `0xc0000000` at `+0x18`.
   - `OneShotTargetCopyTargetSlot`: vtable `0x84a607c8`, binder `0x82972560`, execute `0x829724d0` (`main+0x29a4d0`
     did not start with `pacibsp`; re-check the offset).
   - `ExecuteLookIKArbiterActivateByTargetSlot`, `QueryTargetIsMatchTargetSlot`, `ExecutePlayerCameraLockOn` —
     the game has a **TargetSlot** concept; the lock-on almost certainly lives in one of those structures.
3. `symbols/e0f3ccfa.ld` / `.toml` (same repo) and `docs/structures.md`: camera view vtable `0x84c18a88` with the
   look-at point at `+0x78` (f64[3]) — useful to verify which enemy is locked. Actor rotation: `+0x328`, yaw =
   `atan2(+0x330, +0x348)`.

## Disassembly recipe (works, verified)

```python
raw = hm.read(base + offset, 0x100)            # HostMemory; base = log_addr("main")
open("a.bin", "wb").write(raw)
# llvm-objcopy -I binary -O elf64-littleaarch64 -B aarch64 a.bin a.elf
# llvm-objdump -D --triple=aarch64 a.elf
```
`-b binary` and `llvm-mc` are not available in the toolchain.

## Implementation once the slot is known

- Trigger: the **R3 bit after `controls.c` swaps ZL/R3** (physical R3 without the swap, physical ZL with it),
  only while targeting and in Toggle mode (`switch_target` config byte 74).
- Candidate: nearest alive `g_em` actor whose position (`actor+0x31C`) is not the cached target point
  (tolerance 0.6 m); death check `[[actor+0x270]+0x2B8]+0x1C10 -> +8 > 0`.
- Write the slot/actor reference the game expects (see lead 1/2), then clear the press. Verify with
  `g_ms_targeting`, the cached block at `state+0x1BF0` and the camera look-at.
- If a direct write is not possible, the game command `OneShotTargetSetFromActor` would have to be called with a
  valid command instance and its binder output — last resort.

## ShbConfig offsets (76 bytes, magic `SHBCFG01`)

70 manual_shield, 71 target_r3, 72 sprint_l3, 73 auto_target, 74 switch_target, 75 lang (the mod's HUD labels:
0 en, 1 es, 2 pt, 3 fr, 4 de, 5 it).
