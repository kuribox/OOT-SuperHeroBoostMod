# GameBanana update 0.11.0

Same procedure as 0.10.0 (`references/gamebanana-update.md` of the project skill): edit the submission
(**version** `0.11.0`, **description** from `docs/GAMEBANANA-DESCRIPTION.txt`), attach the presets zip (no
executables, the copy published on the GitHub release so the two hashes match) and archive the previous file, then add
the update post. **Refresh the payload release before the tag** — 0.10.0 shipped a module from before 0.10.0 because
that asset was not updated (see `AGENTS.md`, "Cutting a release").

## 1. Update: title and text

**Update name**

```
0.11.0 - Enemy and boss health bars, the crow crash fixed, and item locations
```

**Update description**

```
0.11.0 adds the health bars as a beta: a small red bar over every enemy's head, at its real height, and a wide bar
with the boss's name at the bottom of the screen for the ten dungeon bosses and the final fights. Two separate
switches in the HUD page, off by default, visual only. The boss bar no longer depends on the camera, so it stays when
the boss is off screen, far away or in a cutscene, and the bars stop blinking while enemies move.

The crow flocks are left alone now, and that is a crash fix: doubling a crow spawner (15 -> 30 at once, and even 20)
crashed Longshot with a null entry in the engine's actor-handle pool, leaving Lon Lon Ranch with Epona and randomly in
Hyrule Field. Every other enemy is still doubled.

The former Great Fairy option is now Item location changes: the poe buyer's 500-point prize is the Ice Arrows instead
of the bottle, and the last big chest of the Gerudo Training Ground holds the bottle instead of the Ice Arrows. The
menu's main-item button keeps its item while the item box catches up, a Longshot crash with HylianSurvivors' romfs is
gone, and the Rust app keeps your folders when it saves its settings.

IMPORTANT - REBUILD ONCE
The module changed: reinstall the profile (emulator) or rebuild the game once with 0.11.0 (Longshot / Hookshot /
ReLink / Rekarina). The configuration block is the same size, so an older app still writes it and the new options stay
off.

0.10.0 was built with an older copy of the module on the release side (the build payload had not been refreshed), so
if you are on 0.10.0 install 0.11.0 and rebuild: its presets and its app carry the module from before 0.10.0.
```

**Changelog entries (Bulk Add, one per line, no commas)**

```
Health bars beta: a red bar over every enemy at its real height
Wide boss bar with the name for the ten dungeon bosses and the final fights
Enemy and boss bars as two separate switches off by default
Boss bar independent of the camera and held through phase changes
Bars stop blinking while enemies move
Crow flocks no longer duplicated: doubling them crashed Longshot
Item location changes: poe buyer prize is Ice Arrows and the Gerudo chest holds the bottle
Menu main-item button keeps its item while the item box catches up
Longshot crash with HylianSurvivors romfs fixed
Rust app keeps the folders target and language when it saves
```

## 2. File to attach

`release/SuperHeroBoost-0.11.0-presets.zip` (15 named profiles, no executables). Upload the copy published on the
GitHub release so the two hashes match; the previous file (0.10.0 / 0.9.3) is archived.

SHA-256: `<filled after the CI release: see the release asset>`

## 3. Submission

- **Title:** `SHB-OcarinaTweaks - Stamina, HUD, difficulty & +` (unchanged). **Version:** `0.11.0`.
- **Description:** `docs/GAMEBANANA-DESCRIPTION.txt`, with the coloured note at the top: the presets are for
  emulators, and a static recompilation (Longshot, Hookshot, ReLink, Rekarina) needs the app from GitHub Releases.
- Screenshots and cover unchanged.
