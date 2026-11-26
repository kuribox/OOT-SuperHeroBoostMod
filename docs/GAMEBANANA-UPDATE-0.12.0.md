# GameBanana update 0.12.0

Same procedure as 0.11.0 (`references/gamebanana-update.md` of the project skill): edit the submission
(**version** `0.12.0`, **description** from `docs/GAMEBANANA-DESCRIPTION.txt`), attach the presets zip (no
executables, the copy published on the GitHub release so the two hashes match) and archive the previous file, then add
the update post. **Refresh the payload release before the tag** (see `AGENTS.md`, "Cutting a release").

## 1. Update: title and text

**Update name**

```
0.12.0 - Item selector fix in the app and groundwork for a live menu
```

**Update description**

```
0.12.0 is a small release. The Rust app keeps Item selector in every button's role list when the item HUD is off, like
the Python app always did, and the module's HUD can now put its item buttons and counters away while the game runs.
That is groundwork for an in-game settings menu that works in the emulator test builds only and is not part of this
release.

IMPORTANT - REBUILD ONCE
The module was rebuilt: reinstall the profile (emulator) or rebuild the game once with 0.12.0 (Longshot / Hookshot /
ReLink / Rekarina). The configuration block is the same size, so an older app still writes it.

Known: with the item HUD off the item selector is listed but does nothing in game.
```

**Changelog entries (Bulk Add, one per line, no commas)**

```
Rust app keeps Item selector in the role list when the item HUD is off
HUD can put the item buttons and counters away while the game runs
Module rebuilt: reinstall or rebuild once
```

## 2. File to attach

`SuperHeroBoost-0.12.0-presets.zip` (15 named profiles, no executables), the copy published on the GitHub release;
the previous file (0.11.0) is archived.

SHA-256: _fill in from the v0.12.0 release's `.sha256` asset_

## 3. Submission

- **Title:** `SHB-OcarinaTweaks - Stamina, HUD, difficulty & +` (unchanged). **Version:** `0.12.0`.
- **Description:** `docs/GAMEBANANA-DESCRIPTION.txt`, with the coloured note at the top: the presets are for
  emulators, and a static recompilation (Longshot, Hookshot, ReLink, Rekarina) needs the app from GitHub Releases.
- Screenshots and cover unchanged.
