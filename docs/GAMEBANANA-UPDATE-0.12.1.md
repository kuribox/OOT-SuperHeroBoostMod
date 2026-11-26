# GameBanana update 0.12.1

Same procedure as 0.12.0 (`references/gamebanana-update.md` of the project skill).

## 1. Update: title and text

**Update name**

```
0.12.1 - In-game settings menu (- and R3)
```

**Update description**

```
0.12.1 adds the in-game settings menu, which was merged for 0.12.0 but missing from that release's module.

Press - and R3 together while playing: a menu opens over the game (B closes it too; the game keeps running behind it). Three pages, L / R change the page: CONTROLS (a role per button, the long presses and the options of each role), HUD (counters, layout, scales, item HUD, enemy and boss bars, stamina bar) and GAMEPLAY (stamina and its preset, sprint speed, enemy life, damage, songbook, quick boots, quick mask). Changes are live and are remembered in a small file, shbmenu.bin, that the mod creates in the game's save volume; installing another configuration with the app overrides it. A lone - or R3 reaches the game 150 ms late. On Longshot, Hookshot and ReLink the launcher's overlay uses + and -, which is why the menu uses - and R3.

IMPORTANT - REBUILD ONCE
The module changed: reinstall the profile (emulator) or rebuild the game once with 0.12.1 (Longshot / Hookshot / ReLink / Rekarina). The configuration block is the same size, so an older app still writes it.

Checked in the emulator; reports from the recompilations are welcome.
```

**Changelog entries (Bulk Add, one per line; a comma splits an entry and drops the rest, so none)**

```
In-game settings menu: - and R3 together open it and B closes it
Three pages: controls and HUD and gameplay edited live
Choices are remembered in shbmenu.bin in the save volume
Module rebuilt: reinstall or rebuild once
```

## 2. File to attach

`SuperHeroBoost-0.12.1-presets.zip` (15 named profiles, no executables), the copy published on the GitHub release; the
previous file (0.12.0) is archived.

SHA-256: `a958bf4ffd65eab79e682377b66cc31fee9b3fd8a61c85a9a983fda49ce98fa0` (the asset of the v0.12.1 release;
GameBanana stored it as `superheroboost-0121-presets.zip`, file id 1905672, MD5 `6181c8aa44acb7f8acb62a8023d6ace3`)
