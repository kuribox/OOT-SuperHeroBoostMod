# SHB-OcarinaTweaks 0.9.0 (Super Hero Boost)

Everything since 0.8.0. Install 0.9.0 and **rebuild the game once** on Longshot, Hookshot, ReLink or Rekarina: the
module changed.

**Controls**

- **Button remap.** Every physical button (L, R, ZL, ZR, X, Y, A, B, L3 and R3) gets a role: Jump, Sword, Action,
  Target, Item B / L / R, the game's own item (ZR), Center camera, Sprint, Shield, Item selector, Armory or nothing
  (at most four item buttons; items never on L3 / R3, the selector only on L / R, Target only on ZL / ZR / L3 / R3).
  The old switches (manual shield, target on R3, L3 sprint, L mode) now come from the table. The app has a remap editor
  with a controller preview and an in-game HUD preview in the chosen button style and layout, and five control
  presets: **Classic**, **Action at the bottom**, **Manual shield**, **Souls (target on R3)** and **Switch GameCube**.
- **A remapped button does only its role, and only while you play.** Menus, the pause menu, dialogues and shops get
  the real buttons, so A / B still confirm and go back; the remap stays on through hit stops, so a button never lets its
  native action through. The context prompt follows the button that holds the Action role.
- **Armory.** A button with the Armory role opens the game's own item drawer while it is held, with the swords and
  shields Link owns and can use at his age; the stick or D-pad moves the cursor and letting go equips it. It is a
  reimplementation of what the community mod Hyrule Armory Sync does. A button with the **Shield** role shows the
  equipped shield on the HUD.
- **Quick mask** goes back from the Lens of Truth to the last mask you wore.
- **Item buttons fixed.** The ZR item follows the game's item set (Child / Adult), so it no longer shows a stale icon;
  the Deku stick stays in hand; the ZR item can go on an empty slot; an item is no longer dropped right after you assign
  it. The pause menu is detected steadily: assigned items are not undone any more, and the L3 toggle sprint and the
  stamina lock no longer touch the input under a menu.
- **L3 sprint on Rekarina.** The input handlers read only the samples the console call returned (Rekarina returns one),
  so the sprint starts, stops and toggles as it does in the emulator.

**HUD**

- **Three layouts:** **Wide** (the native plates), **Simple** (the four counters in one row on a soft translucent bar)
  and **Plain** (the same row with no bar). The old Minimal layout is gone; a saved Minimal becomes Plain.
- **Size sliders** (50-200 %) for the controls cluster and for the counter row. Each keeps its screen corner (controls
  top right, counters bottom left); the CLI has `--hud-ctrl-scale` and `--hud-count-scale`.
- **Widescreen.** The HUD buttons with their labels, and the lower counters with their icons and plates, are grouped so
  they move together with Longshot's "Screen's edges" setting (HaseoSora).
- **Switch GameCube preset** with its button art and controller diagram (HaseoSora).
- **Collectibles counter** copies the save's key set, counts the heart pieces placed in the world and looks GameData
  up by hash, so it stays right with romfs mods that shift the tables.
- **PlayStation / Xbox prompts.** The PlayStation button font fits the loader's size budget and the game-wide layout
  skips the External font that does not fit. The pause menu's assign legend has no backing plate and sits higher.

**Difficulty and enemies**

- **Aggression options:** *Alert radius* (how far an enemy that spots you alerts the others) and an *Aggression*
  level: Normal, Aggressive or Maximum (shorter shared pause, longer chases, fighting off screen). Bosses are not
  changed, and the options combine with the speed multipliers instead of replacing them.
- **Duplicated enemies:** wall and ceiling enemies (Skullwalltula, perched Keese, Jabu-Jabu's Tailpasaran) and stage
  pieces are no longer cloned, so nothing hangs in the air or sits inside a wall.
- **Enemy health is scaled exactly once**, however many enemies are loaded and whatever the game resets.
- **Profiles.** **Recommended** is now the author's own setup (x2 health and damage, no heart drops, hardcore
  stamina, beta enemy mods with aggression, the simple HUD, the manual shield with the targeting assists, R3 lock-on and
  the L3 sprint toggle, sprint x1.5) and the other profiles follow it; *Recommended (no beta enemies)* drops the enemy
  mods.

**Rekarina**

- **Rekarina supported.** Rekarina is the Switch 2 static recompilation that loads ExeFS modules and takes romfs mods.
  The app adds the mod's module as `subsdk9` to the lift, compiles it with Rekarina's own `gen.py` and `build_game.py`,
  converts the mod's IPS with its `tools/exefs_patches.py`, installs the romfs as `mods/zz-Super-Hero-Boost/romfs`,
  puts the mod first in `mods_order` and replaces `Rekarina.exe` with the new build (backed up). Nothing in the Rekarina
  folder is edited.
- **Settings without a rebuild.** Applying a profile patches `exefs/subsdk9` and the expected SHA-256 stored in
  `Rekarina.exe`, so changing options takes seconds; the 10-20 minute rebuild is only needed the first time or when the
  module changes. Uninstall restores the exe, the module, the old mod code, the status files and the mod order.
- **Old merged code disabled safely.** A merged mod folder (for example the Mod Merger's `0-M`) that still carries an old
  `subsdk9` / IPS is moved aside (backed up), so Rekarina stops marking the whole mod as PLUGIN and its romfs loads.
- **Module data without pointers.** The build fails on any pointer table in the module's data, the cause of crashes
  when the module sits at Rekarina's address; the Armory skips the game call that truncates module pointers.

**App**

- **Renamed to SHB-OcarinaTweaks** (the executable, the window, the settings file and the release files:
  `SHB-OcarinaTweaks.exe`, `SHB-OcarinaTweaks-linux`, `SHB-OcarinaTweaks-macos.dmg`). Settings saved under the old name
  are still read, and the updater still recognises the old release file names.
- **New interface:** CustomTkinter theme, a **Presets** page, target cards, inline help and a Help section, one
  **Controls** section with the button mapping, a stamina preview, drop-down lists that no longer click through, and
  the two HUD size sliders.
