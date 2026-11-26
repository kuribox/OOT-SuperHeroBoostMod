# SHB-OcarinaTweaks 0.12.1 (Super Hero Boost)

Everything in 0.12.0, plus the **in-game settings menu**, which 0.12.0 left out of its module by mistake: the code was
merged into `main` but `tools/make_release.py` did not list the feature, so the 0.12.0 module, presets and app carry no
menu. 0.12.1 builds it into all three variants (emulator / standalone, ReLink and Rekarina). Install 0.12.1 and **rebuild
the game once** on Longshot, Hookshot, ReLink or Rekarina; an emulator profile needs a reinstall. The configuration block
is still 0x68 bytes, so an older app keeps working.

**In-game settings menu**

- **Open and close it with `-` and `R3` pressed together.** `B` closes it too. The game keeps running behind it (there is no
  pause), and while it is open every button and both sticks are taken away from the game.
- **Three pages, `L` / `R` change the page:** *CONTROLS* (the button table: the role of every button, the long presses
  with their wait, and the options that belong to a role: manual shield / tap to switch / next target under the target
  role, the toggle under the sprint), *HUD* (rupees, clock, 24 h, collectibles, counter mode, minimap, the two scales,
  layout, simple HUD, item HUD, the enemy and boss bars, the stamina bar) and *GAMEPLAY* (stamina and its preset with the
  custom costs, sprint speed, enemy life, damage, songbook, quick boots, quick mask). `Up` / `Down` choose a row, `Left` /
  `Right` (or `A`) change it. Texts in English and Spanish.
- **A change is live** and writes the same bytes the app would, including the derived ones (the HUD positions follow what
  is shown; the sprint and shield flags follow the table). The button art style is not offered: it is a romfs choice made at
  install time.
- **It remembers itself.** Every change is saved to `save:/shbmenu.bin`, **a small file the mod creates in the game's save
  volume** (the first mod feature that writes there in normal play). It is applied at the next start only while the
  configuration installed in the game is the one the file was made on: installing anything else with the app wins, and the
  file is ignored.
- **A lone `-` or `R3` reaches the game 150 ms late**, because the module holds each one back to see whether the other one
  follows. A lone `+` still pauses as always.
- **On Longshot, Hookshot and ReLink the launcher's own overlay opens with `+` and `-`**; that is why the menu uses `-` and `R3`.
- The menu is painted by the module itself on the game's picture (a hook on the NVN entry points). The module is 20 KB bigger
  and reserves 4 MiB of memory for the picture. Text is ASCII only (no accents).
- **Checked in the emulator.** The recompilations run the same module; reports from them are welcome.

**Carried over from 0.12.0**

- The Rust window keeps *Item selector* in every button's role list when the item HUD is off (the Python app always did).
- The HUD's item side can be switched off while the game runs: the module hides the item buttons and plates and gives the
  game's own item box back; the rupee / clock / collectible plates are put away when their switches change.
- Known: with the item HUD off the module still counts the item selector as an item role, so the option is listed but does
  nothing in game.

**Downloads.** `SHB-OcarinaTweaks.exe` for Windows, `SHB-OcarinaTweaks-linux` and `SHB-OcarinaTweaks-macos.zip`: the
same Rust app (the macOS binary is not signed — right-click > Open the first time; on Linux, `chmod +x` it). The
Python app is published next to it as `SHB-OcarinaTweaks-legacy.exe`, `SHB-OcarinaTweaks-legacy-linux` and
`SHB-OcarinaTweaks-legacy-macos.dmg`. `SuperHeroBoost-0.12.1-presets.zip` installs the mod without an app.
