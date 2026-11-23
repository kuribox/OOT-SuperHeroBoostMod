# SHB-OcarinaTweaks 0.10 (Super Hero Boost)


Install 0.10 and **rebuild the game once** on Longshot, Hookshot, ReLink or Rekarina: the module changed.

**Durations are real seconds now**

Every duration in the mod used to be a number of frames at a fixed 1/60 s, which is only right when the game
runs at 60. On a machine that runs it faster or slower everything timed by the mod moved by that factor.

- **Stamina in real time.** The sprint drain, the action costs and the recharge (0.8 s idle, 1.8 s in combat,
  2.5 s after exhaustion) last what they say whatever the frame rate. Measured live in the emulator: 24/s of
  refill at 41 and at 50 fps, where counting frames gave 16 and 20.
- **The same for the thresholds.** A long press is 0.3 s, and so are the target-switch hold, the pause before
  the ZR item goes back into the box, the item-set check after loading a save and the 5 s "in combat" window.
- **The clock is the console's own.** The module reads the game's counter (`CNTPCT_EL0`, the one `sdk` uses for
  `nn::os::GetSystemTick`) and its rate from `CNTFRQ_EL0`. No new import, no file, nothing to enable. If a target
  ever reports an implausible frame time the mod falls back to the old 1/60 per frame, so it cannot end up worse
  than before.

**Nothing is written into your save**

- **The `shbdebug.bin` file is gone.** Some builds left a diagnostic file next to every save the mod touched —
  written from the input hook, on a timer, inside the game's own save-commit window. It now exists only in a
  test build (`build_core.py --debug-dump`): the release module creates no files at all.

**Not changed**

- The 60 fps numbers: at 60 fps the same thresholds, the same recharge and the same costs as before. The
  gameplay, the presets and the configuration format are untouched, so an existing setup keeps working.
