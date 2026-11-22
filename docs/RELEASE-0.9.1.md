# SHB-OcarinaTweaks 0.9.1 (Super Hero Boost)

Everything since 0.9.0. Install 0.9.1 and **rebuild the game once** on Longshot, Hookshot, ReLink or Rekarina: the
module changed.

**Controls**

- **D-pad right is a button too (long press).** A short press still calls **Navi**, as in the game. Holding the arrow
  does the role you give it: Jump, Sword, Center camera, Sprint, Shield, **Item selector** or **Armory**. The app lists
  it as the last row of Controls. It is not part of the control presets: picking one keeps the arrow's role.
- **The item selector goes on any button but the sticks, and no longer takes Item 2 away.** It has its own virtual
  button now, so Item 2 and the selector can be on the table at once (before, setting one dropped the other).
- **New control preset: Triggers.** Items on the triggers (R, ZL and ZR), the item selector on B, the shield on L,
  sprint on L3, target on R3. The profiles built on the tuned controls (**Recommended**, Recommended without beta
  enemies, **Super Hero**, **Ultra Hero** and **Beta All**) bring this table.
- **Armory on the held D-pad right in the profiles.** Every profile that has the pack's buttons (all but Manual Shield)
  puts the Armory on the arrow and takes it off any other button. The rest of your table stays yours.
- **Sprint ×1.25 in every profile.** (It was ×1.5 in the named profiles.)

**App**

- **Sprint speed is back in the Gameplay page.** The option had disappeared from the window; the module always had it.
- **New drop-down lists.** The selects open their own list: the wheel scrolls the list and never the page, a click
  outside only closes it, the arrows and Enter work, and it closes when the page scrolls under it. No more white edge.
- **Recompile and Reinstall and recompile** for Longshot, Hookshot, ReLink and Rekarina, shown only while the mod is in
  the game. *Recompile* only runs the builder: it copies and patches nothing, so what you did by hand stays. *Reinstall
  and recompile* uninstalls, copies everything again and builds.

**Rekarina**

- **Merged mods.** A mod merger puts every mod's ExeFS patches in one IPS; Rekarina's converter takes a patch file
  whole, so one record it cannot lift made it drop our hooks (`rekarina_patch_failed`). The app now moves the other
  mods' records to `exefs/<id>-other-mods.ips` before it converts, and says which record was rejected.
- **PlayStation / Xbox prompts, the song screen included.** The app no longer installs its own copy of the game's
  button font on Rekarina: a mod file for that font stopped Rekarina from switching the prompts while the game runs
  (the ocarina's six note buttons stayed on the Switch ones). Apply the settings once to remove it.

Known gaps: the D-pad right long press, the free selector and the Rekarina prompts were checked in the emulator and
in the logs, not yet on the three recompilations. The long-press delay is a fixed 20 HUD frames.
