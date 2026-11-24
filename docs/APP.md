# Companion app (SHB-OcarinaTweaks)

The companion app exists twice, and both are kept:

| Folder | App | Notes |
|---|---|---|
| `app/` | the **current app**, Rust (cargo workspace: `shb-core`, `shb-cli`, `shb-gui`) | one executable, the window and the command line; carries the payload; rebuilds recompiled games and Rekarina itself. Windows is the only OS it has been run on. See [app/README.md](../app/README.md) |
| `app-legacy/` | the **first app**, Python (Tkinter / CustomTkinter) | `python app-legacy/main.py`, or the PyInstaller bundle; still the Linux and macOS download. It is also the reference `app/tools/parity_check.py` compares the Rust app against |

Both read and write the same settings file (`SHB-OcarinaTweaks.json`) and install the same folder, so they can be
swapped at any time. The page names and the texts below apply to both: the Rust window draws the Python app's pages
from the same `i18n.py` strings.

The app does **not** compile anything. The mod ships precompiled as the standalone variant (`app/payload/standalone`)
and reads a small config block (`core/config.h`, magic `SHBCFG01`) when the game loads.
The app only:

1. finds the emulator's mod folder (`user/load/0400486024DEC000`, also accepting the load folder, the title folder or
   a custom `load_directory` from `qt-config.ini`),
2. patches the config bytes inside the prebuilt `subsdk9`: stamina on/off, stamina style, per-action costs,
   combat only, enemy health, damage taken, rupee counter / clock / 24 h, sprint speed, button mapping style,
   layout and L mode
   (field table in [GUIA-DESARROLLO.md](GUIA-DESARROLLO.md#33-runtime-configuration-coreconfigh)),
3. copies the button textures for the chosen style (Switch / PlayStation / Xbox) into the mod's romfs; for PlayStation
   or Xbox it also installs the game's own prompts, the External font and the controller picture, downloaded once
   (~70 MB) from the GameBanana controller-UI submission and cached in the user folder. Its resource size table is
   merged with ours entry by entry, so the bigger font budget and our Common-layout/enemy sizes are all kept,
4. with "no heart drops", copies the generated romfs option (if that optional file was not built from the user's game
   dump, the app installs everything else and shows a note instead of failing),
5. writes everything as the mod folder `zz-Super-Hero-Boost` and warns about other exefs mods or button-icon mods
   that may conflict.

## Files

| File | Purpose |
|---|---|
| `app-legacy/main.py` | window, navigation and `--cli` mode |
| `app-legacy/presets.py` | complete game profiles |
| `app-legacy/installer.py` | install logic and config patching |
| `app-legacy/updater.py` | GitHub Releases update check, download and self-replace |
| `app-legacy/version.py` | project version (shown in the window and used by the release tools) |
| `app-legacy/paths.py` | portable/per-user settings paths (Windows, macOS, Linux) |
| `app-legacy/i18n.py` | texts in en / es / pt / fr / de / it |

The Rust app keeps the same split in `app/crates/`: `shb-core` is `installer.py` + `presets.py` + `recomp.py` +
`rekarina.py` + `updater.py` in one library (`settings.rs`, `config.rs`, `roles.rs`, `presets.rs`, `install.rs`,
`game.rs`, `rebuild.rs`, `recomp_build.rs`, `rekarina.rs`, `rekarina_build.rs`, `update.rs`…), `shb-cli` is `main.py`
without the window, and `shb-gui` is the window.

Settings are remembered in `SHB-OcarinaTweaks.json` next to the program; if that folder is read-only, in the
per-user configuration folder (see the README). The app never needs administrator rights.

## Game profiles

A profile sets **every** gameplay option. The fixed profiles include the HUD, the button mapping and the Songbook,
except **Manual Shield**, which installs only the shield. Changing any option afterwards switches the profile to
**Custom** and keeps your value. The button style (Switch / PlayStation / Xbox), layout, L mode, the language, the
emulator folder and the two HUD size sliders are not part of a profile; the button style is global (HUD and the
game's own prompts).

**Recommended** is the default and it is the author's own setup: ×2 enemy health and damage, no heart drops,
hardcore stamina, the beta enemy mods (duplicated placements, every component and the bosses at ×2, enemies that
alert the others and are more aggressive), the simple HUD, the button mapping, the manual shield with the targeting
assists, R3 lock-on and the L3 sprint toggle. It installs the experimental enemy mods, so back up the save.
**Recommended (no beta enemies)** is the same setup without them.

The rest of the profiles step away from it one direction at a time: down to vanilla (Base, Manual Shield, World
changes), around the classic Hero Mode numbers (Hero Mode, Hero + Stamina, Hero Explorer) and up to Super Hero,
Ultra Hero and the two beta combinations.

The language selector translates the window, and it is also the fallback for the few labels the mod draws in game
(the jump label, the A fallbacks and the pause menu's `Assign:` legend): those follow the **game's language** when the
installer can read it from the emulator settings or from `language.txt` next to a recompiled game, so the HUD matches
the game's own prompts. `--lang` on the command line forces one.

Sprint is **×1.25** in every profile and in Custom. It is only a starting point and can be changed by hand in the
Stamina section; the profiles never use ×1.5 or more.

Every profile that has the pack's buttons (all but Manual Shield) also puts the **Armory** on the **held D-pad right**
(a short press still calls Navi): choosing the profile gives the arrow that role and takes the Armory off any other
button. The profiles built on the tuned controls (Recommended, Recommended no beta enemies, Super Hero, Ultra Hero,
Beta All) also bring the author's **Triggers** button table: items on the triggers (R, ZL, ZR), the item selector on B,
the shield on L, sprint on L3, target on R3. The other profiles keep the table you have (only the options they imply
follow). Pick any table by hand under Controls.

| Profile | Stamina | Stamina style | Enemy HP | Damage | No hearts | Enemy mods | Sprint | HUD | Buttons | Shield |
|---|---|---|---|---|---|---|---|---|---|---|
| **Recommended** | on | Hardcore | ×2 | ×2 | on | duplicate + ×2 (bosses ×2), aggressive | ×1.25 | on | on | on |
| Recommended (no beta enemies) | on | Hardcore | ×2 | ×2 | on | — | ×1.25 | on | on | on |
| Souls controls | on | Hardcore (bar) | ×1 | ×1 | off | — | ×1.25 | on | on | on |
| Souls controls + Hero Mode | on | Hardcore (bar) | ×2 | ×2 | on | — | ×1.25 | on | on | on |
| Base | off | — | ×1 | ×1 | off | — | ×1.25 | on | on | off |
| Manual Shield | off | — | ×1 | ×1 | off | — | ×1.25 | off | off | on |
| Hero Mode | off | — | ×1.5 | ×2 | on | — | ×1.25 | on | on | on |
| Hero + Stamina | on | Hardcore | ×1.5 | ×2 | on | — | ×1.25 | on | on | on |
| Hero Explorer | on | Sprint only | ×1.5 | ×2 | on | — | ×1.25 | on | on | on |
| Super Hero | on | Hardcore | ×2.5 | ×2.5 | on | ×1.5 (bosses ×1.5) | ×1.25 | on | on | on |
| Ultra Hero | on | Hardcore | ×3 | ×3 | on | duplicate + ×2 (bosses ×2) | ×1.25 | on | on | on |
| Survival | on | Hardcore | ×2 | ×2 | on | — | ×1.25 | on | on | on |
| World changes | off | — | ×1 | ×1 | off | — | ×1.25 | on | on | off |
| Beta: duplicated + fast enemies | off | — | ×1 | ×1 | off | duplicate + ×2 (bosses ×2) | ×1.25 | on | on | off |
| Beta: everything | on | Hardcore | ×3 | ×3 | on | duplicate + ×3, maximum aggression | ×1.25 | on | on | on |
| Custom | your values | your values | your values | your values | your values | your values | your values | your values | your values | your values |

"Sprint only" means only sprinting drains stamina. "Hardcore" means every enabled action drains it. "No hearts" means
dropped hearts become green rupees (the optional romfs file; see the note below). The Souls profiles show the stamina
as a bar; the rest keep the ring.

## Window

- **Home**: profile list (the only profile selector), explanation of the active profile, emulator folder and install
  status.
- **Stamina**: on/off, stamina style, what costs stamina (the checkboxes update automatically for each style), sprint
  speed and the stamina display (ring around the hero, or a segmented bar under the magic gauge; yellow, red while
  recharging).
- **Difficulty**: enemy health, damage taken, heart and magic-jar drops (both become green rupees) and the two beta
  enemy tools, generated from the user's dump at build time and installed as extra romfs files:
  - **Duplicate enemies (Option A)**: clones generic enemy placements in `Banc` and doubles field auto-spawn counts.
    Bosses, minibosses and scripted enemies are excluded.
  - **Enemy speed**: movement (all base speeds and attack motions), animation playback, attack cadence and
    projectile speed, each with eleven multipliers from ×0.5 to ×6 (plus off). Movement scales every speed
    together, so enemies do not slide or overshoot; attack cadence scales the interval tables and the action
    timeouts (as before).
  They can break puzzles, quests or performance. Back up the save and test one at a time.
- **Controls**: target with R3 (swaps ZL and R3; the app recommends setting the game's Targeting Type to **Toggle**),
  sprint while L3 is held (the game's sprint animation, without rolling), the manual shield, button mapping
  (on/off, style, layout, L mode) and **Quick select**. Button mapping is one switch for the whole card: off,
  nothing is remapped and the options that ride on it (target, L3 sprint, manual shield, the target pair) are not
  written to the mod at all. **Custom item HUD** is its item side: on, our own B / L / R / ZR buttons are drawn and
  items 1, 2 and 3 can go on any button; off, the buttons are still remapped but the player's item HUD and the game's
  item logic stay untouched (an item button keeps the game's own press). Some roles may sit on two buttons at
  once (jump, sword, the item selector, the armory and Back); the HUD draws a role once, on its first button.
  Under the summary, the controls card says which of the game's own actions (jump, sword, action, target) have no
  button at all.
- **HUD**: rupee counter, clock (**off / 12 h / 24 h**, one selector), HUD layout, the area collectibles counter and
  **hide the minimap** (the corner map is not drawn while playing; the map screen still works).
- **HUD layout**: **Wide** (the four native-style plates at the bottom), **Simple** (the four counters at the bottom,
  no background) and **Minimal** (rupees and time above the minimap, no plates; the
  Skulltula plate is left to the game, and the current area's heart-piece counter appears next to the native
  Skulltula counter on the Minus map).
- **Area collectibles counter**: adds a third native-style plate below the rupee/clock row with the
  Gold Skulltulas and heart pieces of the current Banc, for example `SKULL 3/8  COR 1/2`. It reads the live
  `HeartPieceAnchor_GameDataPath` flags, `Sequence_CurrentBanc` and, for the Skulltulas, the save's persistent
  64-bit key set (`Game` / `Bool64bitKey`), so the count no longer resets when you leave and re-enter an area.
  The heart count only includes pieces that the game associates with a map anchor; some chest/minigame pieces
  without one are not counted yet.
- **Miscellaneous**: Songbook autoplay (select a learned song in the game's native songbook with ZL and it plays
  automatically, effect included; not the Scarecrow's Song).
- **World changes**, inside *Miscellaneous*:
  - **Great Fairy rewards**: the three magic rewards are reordered. Hyrule Castle (child) gives **Nayru's Love**,
    Death Mountain Crater gives the **magic spin attack**, and Desert Colossus gives **double magic**. Each fountain
    keeps its own Great Fairy and its own dialogue; the reward line is switched to the new power. The Din fairy's
    overview dialogue still describes the original order, so trust the fountain itself.
  - **Farore's Wind outdoors**: allows placing Farore's Wind warp points outside dungeons. The real forbidden areas
    inside dungeons (for example the FireTemple areas) stay blocked. It is a patched event flow, installed as a romfs
    file.
- **About**: credits, the material/third-party notice and **Check for updates**.

The bottom bar always shows the install state and the **Install / Uninstall** buttons. Changes are saved
automatically (250 ms debounce).

## Quick select

The **HUD & buttons** section has a **Quick select** card. It enables the module's D-pad switches:

- **Boots**: hold D-pad Down to alternate between the Iron Boots and Hover Boots (from the default boots it picks
  the first one you own).
- **Mask**: hold D-pad Up to alternate between the Lens of Truth and the mask you were wearing.
- Only items in your inventory are used. The module calls the game's own equip request, so the change is applied
  immediately; writing the String64 alone only affects the next load.
- A short tap keeps the game's normal D-pad behaviour.
- The named profiles turn both on; uncheck them here to use a profile without quick select. It writes the live
  equipment, so keep a save backup.

## Manual shield

The **Controls** section has a **Manual shield** card (off by default):

- **ZL** follows the game's **System > Options > Targeting Type**: hold to target in **Hold Target**, press to toggle
  in **Toggle Target**. The mod reads the option live.
- **L**: while the manual shield is enabled, L is reserved: it raises the shield only with the target camera and never
  triggers its item/selector/default function, including during shield-hit reactions.
- **R3**: cancels targeting (unless the target-on-R3 swap is enabled).
- It is a control preference, not part of a profile, and it works independently of the stamina system.

## Updates

On startup the app asks GitHub for the latest release of
[OOT-SuperHeroBoostMod](https://github.com/kuribox/OOT-SuperHeroBoostMod/releases/latest) in a background thread. If
the tag is newer than `app-legacy/version.py`, a bar appears under the header with **Download / Later**; it is also
available as **Check for updates** in *About*.

- The download is the release asset for the platform (`SHB-OcarinaTweaks.exe` on Windows,
  `SHB-OcarinaTweaks-linux` on Linux, `SHB-OcarinaTweaks-macos.dmg` on macOS; the older asset names are
  still accepted). It is written to a
  `.part` file, its SHA-256 is checked against the asset digest from the GitHub API (when present), and only then
  renamed.
- A frozen Windows build in a writable folder replaces itself: a small detached PowerShell script waits for the app
  to exit, copies the new executable over the old one and starts it again. If the folder is read-only (for example
  `Program Files`), or the app runs from source, the file is saved to
  `%LOCALAPPDATA%\SHB-OcarinaTweaks\updates` and the app shows its path.
- A network error or a rate limit never blocks the window: the automatic check is silent, and only the manual
  **Check for updates** shows a message.

The updater is only active in the GUI (`--cli` does not check), and it needs no GitHub account or token.

## Command line

The same install code is available without a window:

```
python app-legacy/main.py --cli --emu "EMULATOR_PATH" --profile recommended
python app-legacy/main.py --cli --emu "EMULATOR_PATH" --preset 3 --off jump,shoot --combat-only
python app-legacy/main.py --cli --emu "EMULATOR_PATH" --profile hero-explorer --rupees --clock --clock24
python app-legacy/main.py --cli --emu "EMULATOR_PATH" --profile recommended --no-songbook
python app-legacy/main.py --cli --emu "EMULATOR_PATH" --profile base --enemy-move 1 --enemy-anim 2 --enemy-attack 2 --enemy-proj 2 --stamina-bar
python app-legacy/main.py --cli --emu "EMULATOR_PATH" --profile recommended --lang en
python app-legacy/main.py --list-profiles
```

`--profile` applies a complete profile first; the individual flags override it. `--preset` keeps the numeric stamina
style used by older scripts (0 hardcore, 1 combat only, 2 sprint only, 3 advanced).

## Build

Run this on each target OS:

```
python tools/make_release.py     # module + button textures + optional romfs options
python tools/build_app.py        # release/SHB-OcarinaTweaks[.exe|.app]
```

`build_app.py` uses the platform's separator for `--add-data` and writes the platform's native output. macOS needs
PyInstaller from the same Python installation used to build; Linux produces a plain executable.

The whole payload is bundled with the app: the module, the repository romfs and the button textures for the three
styles. The `romfs` options generated from the game are only installed when their option is enabled, and a missing one
stops nothing else.

## Linux

The app itself is plain Python + Tkinter and builds on Linux:

```
sudo apt install python3 python3-tk
python3 -m pip install --user pyinstaller
python3 tools/build_app.py          # release/SHB-OcarinaTweaks
```

`make_release.py` (the mod payload) is a separate step: it needs the LLVM toolchain and the game dump, so the usual
flow is to build the payload once (on any machine that has them) and then build the app on each target OS. PyInstaller
does not cross-compile.

To build the payload on Linux too, install `clang`, `lld` and the LLVM binutils, then set `SHB_LLVM` (folder with
`clang`, `ld.lld`, `llvm-objcopy`, `llvm-size`, `llvm-nm`), `SHB_STOCK_MAIN` (the game's `main`), `SHB_ROMFS` (the
`romfs` dump) and `SHB_DEVTOOL` (the BYML reader used by `make_romfs_opts.py`).

## No-heart-drop file

The "no hearts" option changes a game data table (`DropGlobalParam`) through romfs; it is not code. The repository does
not redistribute that table. `tools/make_romfs_opts.py` generates it from the user's own game dump (this project uses
the dump folder, or `SHB_ROMFS`), and `make_release.py` keeps an existing `app/payload/romfs_opts` when
rebuilding. That file is only needed at build time: the release payload already includes the generated copy, so users
do not need a dump. If the file is missing, the app installs everything else and shows a note.

## Longshot and Hookshot

The app can install into a static recompilation too. Pick **Hookshot / Longshot** in the **Install** section of the
sidebar and choose the game folder (the one with `bin/longshot.exe` and `.longshot_exefs`).

- The first **Install** runs the builder's update with a progress bar: 25–45 minutes, the game must
  be closed. The builder is detected from the launcher; pick its folder (the one with `tools/nx2/make_game.py`) if it
  is not.
- Once the game is built, **Install** only applies the profile, the configuration block and the selected romfs files
  (seconds, no rebuild). Use **Reinstall and recompile** after an app/mod update.
- If the builder cannot find the Vulkan/VMA headers, the app points `RX_CPM` at its packaged `tools/cpm`; if they are
  still missing, it reuses the game's existing `bin/longshot_vk.dll` / `.so` through `LONGSHOT_VK_PREBUILT`.
- The actions are in the bottom bar:
  - **Install / Apply settings**: **Install** copies the mod and compiles the game; once the mod is in the game it
    becomes **Apply settings** (the profile, the configuration block and the romfs files only).
  - **Recompile** (only while the mod is in the game): runs the builder and nothing else. No mod file is copied,
    patched or rewritten, so what you changed by hand in the game folder is what gets compiled.
  - **Reinstall and recompile** (only while the mod is in the game): the same as **Uninstall** followed by
    **Install**: every original file comes back, then everything is copied again with the chosen options and the game
    is compiled. Hand changes in the game folder are lost.
  - **Uninstall**: restores every original file.

  The Install section shows the game folder, the detected builder (a **Builder…** button appears only if it is not found) and the
  progress/log of a rebuild.
- **Uninstall** restores the original executable, image and `main` from `<game>/.shb-backup` and removes the mod.
- The builder's `exefs` patches are not applied at run time (the game is compiled ahead of time), so the module must
  be built in; this is what the app does. See [LONGSHOT.md](LONGSHOT.md).

`tools/install_longshot.py` is the same procedure for scripts and CI.

## Rekarina

Rekarina is the OoT Switch 2 static recompilation. Pick **Rekarina** in the **Install** section and choose its game
folder (the one with `Rekarina.exe`, `rekarina.ini` and `exefs/main`). The app finds `setup/<version>` beside it.

- Rekarina is **never edited**. The app runs its own tools against a work folder in `<game>/.shb-rekarina`:
  `tools/cheats.py` and `tools/exefs_patches.py` (the mod's IPS plus every other mod's code patches),
  `recomp/gen.py --modules main,subsdk0,sdk,subsdk9`, `tools/build_game.py` with the release's LLVM, Ninja and deps,
  and then copies the produced `rekarina.exe` over `Rekarina.exe`.
- Two small runners are written into the work folder (never into Rekarina): one imports its `build_game.py` to add
  `-DGAME_NS2 -DGAME_OOT` (the flags it only adds when it picks the expected table itself), the other imports its
  `gen.py` and adds the hook resume points as extra `main` function seeds. Rekarina's lifter dispatches only to
  function starts; our code hooks return to `site+4`, and the split keeps the normal fall-through intact.
- The mod's `.ips` stays in `mods/zz-Super-Hero-Boost/exefs`: Rekarina's runtime turns the generated code set on
  only when the enabled mod folder holds the patch file named by `build-cache/exefs.status` (runtime/mods.c
  `mods_exefs_switch`). Removing it loads `subsdk9` but never branches into it.
- The module is built for Rekarina's layout (`main+0x5200000`; the app checks the base from the real module sizes)
  and carries a synthetic `eh_frame_hdr` seed table (`tools/build_core.py --rekarina`) so Rekarina's lifter can find
  its functions, plus a minimal dynamic symbol table (`DT_SYMTAB/DT_STRTAB/DT_HASH`) that its loader requires.
  Rekarina's runtime loads the extra module and initialises it like any other.
- The mod's romfs goes to `mods/zz-Super-Hero-Boost/romfs`; Rekarina merges the mods' resource size tables at run
  time. The app puts the mod first in `mods_order` (the top mod wins a file) and moves stale Super Hero Boost code
  out of other mods (for example a merged `0-M` carrying the old `subsdk9`/IPS), so Rekarina stops marking that mod
  as PLUGIN and its romfs loads.
- **Settings without a rebuild.** The module's config block lives in its data; applying a profile patches
  `exefs/subsdk9` and replaces the expected SHA-256 stored in `Rekarina.exe`, so it takes seconds. A full rebuild is
  needed only the first time or after the module changes (10–20 minutes, ~6 GB of free space).
- **Prompts font.** On Rekarina the app never installs the PlayStation style's `Font2/…External…bfotf.zs`. Rekarina
  switches the game's prompts (and the song screen's six note buttons) to PlayStation / Xbox by finding its own font in
  memory while the game runs; a mod file for that font shadows it (its log: `a RomFS mod replaces Font2/…External…`,
  `the game's button font was not found in memory`, `in use: nintendo`). Our HUD button textures and the pause legend do
  not need it. The Reinstall / Apply settings button rewrites the mod folder without the font.
- **Merged mods.** A mod merger joins the ExeFS patches of every mod into one IPS inside the mod folder. Rekarina's converter (`tools/exefs_patches.py`) takes a patch file whole: one record it cannot lift (for example a code change that alters the CPU flags differently from the game, as `young-link-rides-epona`'s patch at `main+0x482DFC` does) makes it reject the file, and our hooks go with it (`rekarina_patch_failed`). Before the conversion the app moves every record that is not ours (our sites are the ones of the payload's `ours.ips`) into `exefs/<id>-other-mods.ips`, which the converter classifies as *other* and skips. Our file is byte for byte the payload's. The other mod's romfs part keeps working; its code patch cannot run on Rekarina anyway.
- **Uninstall** restores `Rekarina.exe`, the old module, the moved mod code, the status files and the mod order,
  then deletes `<game>/.shb-rekarina`.
- In the app, **Install / Apply settings**, **Recompile**, **Reinstall and recompile**, **Uninstall** and **Play**
  (opens `Rekarina.exe`) work exactly as for Longshot. **Recompile** lifts and compiles with the module that is in
  `exefs/` (not the payload) and replaces `Rekarina.exe`; nothing else is touched.
