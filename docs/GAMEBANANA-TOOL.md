# GameBanana Tools submission

Use this for the **Tools** section of the game (https://gamebanana.com/tools/games/7534), separately from the mod.
GameBanana classifies third-party programs as Tools; the Mods file must stay free of executables.

**Title:** Super Hero Boost Config

**Tagline:** Configure and install Super Hero Boost: profiles, stamina, difficulty, HUD, item buttons and optional
world changes from one window.

**Version:** 0.8.0

## Summary

SuperHeroBoostConfig is the companion app for **Super Hero Boost**, a gameplay and HUD module for the *Ocarina of
Time* remake (update **v131072**). It edits the module's configuration, chooses the button textures and copies the
files into the emulator's mod folder. It also builds the module into **Longshot** (static recompilation), and it can
apply a new configuration without rebuilding.

The app is open source (MIT). It is built with Python and PyInstaller; the downloads are unsigned, so some antivirus
programs may show a generic false positive. The source, the build script and the release checksums are public.
Prebuilt apps: `SuperHeroBoostConfig.exe` (Windows), `SuperHeroBoostConfig-linux` (Linux x86_64) and
`SuperHeroBoostConfig-macos.dmg` (macOS Apple Silicon), on GitHub Releases.

## Features

- Profiles: Recommended, Souls controls, Souls controls + Hero Mode, Base, Hero Mode, Hero + Stamina, Hero Explorer,
  Super Hero, Ultra Hero, Survival, World changes, two beta combinations and a fully custom setup.
- Stamina style and per-action costs, enemy HP/damage multipliers, sprint speed, no-heart-drop.
- HUD: rupees, clock (12/24 h) and per-area collectible counter, in three layouts (Wide, Simple, Minimal).
- Controls: target with R3 (swaps ZL/R3), souls-like target switch (tap the target button for the enemy the game marks
  in red, hold it to release, next enemy when the locked one dies), hold L3 to sprint with the native animation, and
  the manual shield keeps L reserved.
- Quick select (normal feature): hold D-pad Down/Up to switch boots/masks; the named profiles include it.
- Button mapping: B / L / R / ZR, Switch / PlayStation / Xbox pictures, **Nintendo / A-B swapped / Xbox** button
  positions, extended or simple layout, L in item or selector mode, songbook autoplay.
- Game button prompts: a separate, optional **None / PlayStation / Xbox** choice in the HUD section; Longshot and
  ReLink already ship their own.
- **Play** button, themed scrollbars and, on a recompiled game, a red **Requires rebuild** tag on the main switch of
  each feature.
- Optional world changes (Great Fairy rewards, Farore outdoors) and beta enemy options (duplication, movement,
  animation, attack cadence and projectile speed), generated from your own game files.
- Install and uninstall into the emulator's `user/load/0400486024DEC000` folder.
- Checks GitHub Releases on startup, shows a download bar when a newer version exists and, on Windows, replaces
  itself and restarts (the SHA-256 of the downloaded asset is verified when GitHub provides it).
- Longshot target: patches `main`, copies the module to `.longshot_exefs`, installs the romfs options and runs the
  builder's update with progress, log and cancel. It keeps DLSS/FSR, closes a running game on Windows, detects the
  right builder and supports **Hookshot** (same builder) and **ReLink** (`.relink_exefs`, `bin/relink-game`). Under
  ReLink every feature works: the module is built without tail calls (the translator skips the rest of a function
  whose start is reached only through a backward tail call).
- Command-line mode with the same options.
- Spanish, English, Portuguese, French, German and Italian. The labels the mod draws in game (Jump / Attack / Roll and
  the pause menu's Assign:) follow the game's language; the app's language is the fallback.

## How to use

1. Requirements: the game on update **v131072**; an emulator that loads mods from `user/load`, or a Longshot /
   Hookshot installation with its builder's `tools` folder.
2. Back up your saves. On Windows the app closes the running game before installing/rebuilding.
3. Run `SuperHeroBoostConfig.exe`.
4. Pick a profile or tune each section, choose the emulator/game folder and press **Install** (or **Rebuild game**
   for Longshot).
5. Start the game. To remove the mod, press **Uninstall** or delete `zz-Super-Hero-Boost`.

## Files

- `SuperHeroBoostConfig.exe`
  SHA-256: `046f40cb94ddab8cca574dfb5fe2091291b04fb588579326260ef49ad3c1ca29`

The module payload is inside the executable; the app does not download anything.

## Links

- Source, issues and releases: https://github.com/kuribox/OOT-SuperHeroBoostMod
- Mod submission: **Super Hero Boost** (link the mod page here once it is listed).

## Credits

Add these to the **Credits** module:

- **Kuribox** — creator: app, module, HUD, stamina system and item buttons — https://github.com/kuribox
- **Anthropic (Claude)** — AI software used as an assistant for coding, reverse engineering and testing —
  https://www.anthropic.com
- **DeepSeek** — AI software used as an assistant for coding, reverse engineering and testing —
  https://www.deepseek.com
- **PixelSage / Hyrule Armory** — the original Armory (sword and shield quick select) —
  https://gamebanana.com/mods/740938
- **DoctorTwitch / Dualsense/XBOX/Steamdeck UI** — the PlayStation and Xbox button layouts the app downloads on
  request (not bundled) — https://gamebanana.com/mods/737074
- **Kabosune / oot-remake-modding** — modding documentation and help —
  https://github.com/Kabosune/oot-remake-modding
- **platinumagpie / oot-remake-devtool** — development tools used during research and testing —
  https://github.com/platinumagpie/oot-remake-devtool

## Moderation note

If a moderator asks about the executable: it is an open-source configuration tool, not the game mod, and it is
submitted in the Tools section on purpose. The Mods submission is a separate file with no executables. Include the
source link and the SHA-256 above, and offer a VirusTotal scan of the exact release binary.
