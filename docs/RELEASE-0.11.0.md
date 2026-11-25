# SHB-OcarinaTweaks 0.11.0 (Super Hero Boost)

Everything since 0.10.0. Install 0.11.0 and **rebuild the game once** on Longshot, Hookshot, ReLink or Rekarina: the
module changed. The configuration block is the same size (0x68 bytes), so an older app still writes it and the new
options simply stay off.

If you only want the new options and play in an emulator, installing over 0.10.0 is enough.

**Downloads.** `SHB-OcarinaTweaks.exe` for Windows, `SHB-OcarinaTweaks-linux` and `SHB-OcarinaTweaks-macos.zip`: the
same Rust app (the macOS binary is not signed — right-click > Open the first time; on Linux, `chmod +x` it). The
Python app is published next to it as `SHB-OcarinaTweaks-legacy.exe`, `SHB-OcarinaTweaks-legacy-linux` and
`SHB-OcarinaTweaks-legacy-macos.dmg`. `SuperHeroBoost-0.11.0-presets.zip` installs the mod without an app.

> **Heads-up for anyone on 0.10.0:** that release was built with an older copy of the module (the payload the build
> workflow downloads had not been refreshed before the tag), so its app and its presets carry a module from before
> 0.10.0. 0.11.0 is built from the refreshed payload; if you are on 0.10.0, install 0.11.0 and rebuild once.

**Health bars (beta)**

- **A red bar over every enemy.** A small bar above each enemy's head, at the height the model really has (the engine
  carries it per actor, so a scaled or big enemy gets its own), filling down as the enemy loses life.
- **A named bar for dungeon bosses.** Gohma, King Dodongo, Barinade, Phantom Ganon, Volvagia, Morpha, Bongo Bongo,
  Twinrova, Ganondorf and Ganon get a wide bar at the bottom of the screen with the boss's name instead of a head bar.
  Their sub-actors (Gohma's eggs, Bongo Bongo's hands, Morpha's tentacles, Barinade's jellyfish) do not get one;
  Twinrova's two witches keep their own head bars until they merge, and the merged actor carries the big one.
- **Two separate switches**, in the **HUD** page of both apps, in the six languages: *Red bar over every enemy* and
  *Big bar with the boss's name*. Off by default. Visual only: they write nothing to the save.
- **It holds still.** The engine hands only a subset of the actors to the physics update each frame, and that subset
  changes every frame, so a bar rebuilt every frame blinked whenever an enemy moved. Each row now lives for a few
  frames after its last update and stays on the same actor, and the draw keeps a grace window for hit stops and area
  loads.
- **The boss bar does not depend on the camera.** It is picked from the enemy table alone, so it does not blink out
  when the boss walks off camera, to the far end of the arena or into a cutscene, and it holds through a phase change.
  It sits low on the screen (moved down from where the first beta put it, where it covered too much of the fight).
- **No depth test.** The bars are a flat overlay: a bar can show over geometry that hides the enemy. That is how the
  feature is built today.
- **Open:** the boss bar reads the boss actor's own Life record, and a boss with several bodies or phases (Bongo
  Bongo, Barinade, Twinrova, Ganon's second half) may keep the fight's real pool somewhere else. That needs one fight
  per boss. The enemy bars and the boss bar's size, place and name were checked in game.

**The crow flocks are no longer multiplied — they crashed the game**

This is a fix, not a balance change. The enemy duplication doubles the spawn counts of the field spawners, and at
**15 -> 30 crows at once it crashed Longshot**: a null entry in the engine's actor-handle pool, leaving Lon Lon Ranch
with Epona, and random crashes in Hyrule Field later. **A cap of 20 crashed as well**, so it is not a matter of the
number being too big by a little: the crow spawner itself does not survive being doubled. It was found by bisecting
the mod's romfs files with override mods, and the suspects were ruled out one by one (the Stalchildren, the Banc
clones); restoring only the crow spawner files, with the rest of the mod untouched, made the crashes stop.

So the generator now leaves **every crow flock's spawner alone** (`Enemy_Guay_*`, `Enemy_Guey_*`): they keep the
game's own numbers. The crows still get everything else the mod does, and every other enemy is still doubled.

**Item location changes**

The former *Great Fairy rewards* option is now **Item location changes** and moves one more pair of items:

- The **poe buyer's 500-point prize** is the **Ice Arrows** instead of the bottle.
- The **last big chest of the Gerudo Training Ground** holds the **bottle** instead of the Ice Arrows.
- The three magic rewards move as before (Hyrule Castle child: Nayru's Love; Death Mountain Crater: the magic spin
  attack; Desert Colossus: double magic).

Each file was read back identical to the game's apart from those two strings. **Not played through:** no other chest in
the game hands out a bottle, so whether a chest can give one is unproven until someone opens it. With the duplicated
enemies on as well, the dungeon keeps its clones and the chest change on top.

**Fixes**

- **The menu's main-item button keeps its item.** Using a slot item and opening the menu showed the used item on the
  main button until the item box caught up; the HUD now holds the item it expects back until the box shows it.
- **A Longshot crash with HylianSurvivors' romfs is gone.** The collectibles counter reads the game's save-image
  manager, and one of its slots held a value that merely looked like an address (1.1 TB, past Longshot's heap):
  the read faulted inside `hud_on_code`. A save image is a heap object that sits next to its manager, so the guard
  now demands a word-aligned slot within 16 GB of it, and the stray value fails the first check.
- **The Rust app keeps your folders when it saves.** Saving the settings merged nothing and wrote over the folders,
  the target and the language the window keeps in the same file; it merges into the file now instead. The folder on
  screen is remembered, and a folder or builder you type in stays when the field loses focus.

**For builders**

- After building a module, write its new `ours.ips` into `main` before rebuilding the game, or the rebuilt game jumps
  into the middle of a function: the hooks in `.longshot_exefs/main` hard-code each symbol's address, and any code
  change can move a function. `rebuild --mode install` does it; `--mode recompile` does not. See `docs/RE-NOTES.md`.

**Not changed**

- The stamina system, the button remap, the HUD layouts and the enemy multipliers work as they did in 0.10.0.
- The preset profiles are the same ones, with the new options off.
