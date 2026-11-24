# GameBanana update 0.10.0

Same procedure as 0.9.3 (`references/gamebanana-update.md` of the project skill): edit the submission, attach the
presets zip (no executables), add the update post.

## 1. Update: title and text

**Update name**

```
0.10.0 - Holds on L and R, the D-pad right always waits for its hold, and multipliers to x6
```

**Update description**

```
0.10.0 adds a long press on L and R next to A, B, X and Y, with a wait-or-no-wait choice per held button (the D-pad
right always waits: a tap is Navi, the hold is the role). Shortcut roles can sit on two buttons, the Controls card
lists the game's own actions that no button has, the corner minimap and the interaction guide bar hide properly, and
the custom item HUD is its own option. Enemy health and damage taken multipliers now go up to x6. Under the hood the
durations are real seconds whatever the frame rate, and the module writes nothing into your save folder.

IMPORTANT - REBUILD ONCE
The module changed: reinstall the profile (emulator) or rebuild the game once with 0.10.0 (Longshot / Hookshot /
ReLink / Rekarina).

The app is in Rust now (one executable, no Python) and it builds the game itself; the Linux and macOS downloads are
built again, with the Python app published next to them as the -legacy files.
```

**Changelog entries (Bulk Add, one per line, no commas)**

```
Long press on L and R next to A B X and Y
Wait or no wait per held button and the D-pad right always waits
Shortcut roles can sit on two buttons
Controls card lists the game's actions that no button has
Corner minimap and interaction guide bar hide properly
Custom item HUD is its own option
Enemy health and damage taken multipliers up to x6
Durations are real seconds whatever the frame rate
The module writes nothing into the save folder
```

## 2. File to attach

`release/SuperHeroBoost-0.10.0-presets.zip` (15 named profiles, no executables). Upload the copy published on the
GitHub release so the two hashes match.

SHA-256: `9b2ceae3b92e6c96c0e4e16f8323da17739fc40832a5f72ee5279fd1d763711e`

## 3. Submission

- **Title:** `SHB-OcarinaTweaks - Stamina, HUD, difficulty & +` (unchanged). **Version:** `0.10.0`.
- **Description:** `docs/GAMEBANANA-DESCRIPTION.txt`. Screenshots and cover unchanged (the app screenshots in
  `docs/capturas/` still show the Python window; a later pass can add the Rust ones).
