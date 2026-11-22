# Longshot, Hookshot and ReLink

Hookshot, Longshot and ReLink are **not emulators**: they are static recompilations of the game. The console's code is
translated to C and compiled into a native program (`bin/hookshot`, `bin/longshot`, `bin/relink-game`; `.exe` on
Windows); the prepared guest image lives in `image.bin`, next to it. The app tells them apart by the exefs folder the game keeps
(`.longshot_exefs`, `.hookshot_exefs`, `.relink_exefs`), and uses the matching builder, environment variables
(`LONGSHOT_*` / `RELINK_*`) and file names.

That changes how a code mod is installed.

## Why an `exefs` mod does not work

The official mod layout is the emulator one:

```
<game>/mods/<name>/romfs/...      files that replace (or add to) the game's RomFS
<game>/mods/<name>/exefs/...      ignored
```

`host/mods.c` in the builder says it plainly:

> exefs patches (.pchtxt, .ips) are not applied: the code is compiled ahead of time from the original instructions,
> and the game folder has no module map (image.json) to place data patches with. A mod's exefs folder is logged as
> left out.

The log line is `mods: <name>: exefs patches are left out (the code is compiled ahead of time)`. Patching
`.longshot_exefs/main` by hand does nothing either: that folder is only the source of the build, and the translated
code is already inside the executable.

So:

- **RomFS only** (button textures, patched event flows, resource size tables, enemy packs): copy the romfs files to
  `<game>/mods/zz-Super-Hero-Boost/romfs/`. That is all the recompiler reads at run time. The HUD, stamina, item
  buttons and collectible counter are code, so they will not appear (and the item buttons also need their textures
  here).
- **The whole mod**: rebuild the game with the mod's code included in the translation.

> **Load order.** ReLink and the emulators resolve a file by taking the mod whose path sorts **last**
> (`zz-...` > `02-...`). That is why the folder is `zz-Super-Hero-Boost`: its resource size table must win over
> other mods' tables (a costume pack or the controller UI ship one too). If a mod sorts after this one and brings its
> own table, the app merges both when you install and warns you in the result.

## Rebuilding the game with the mod

The builder translates every module it finds in `.longshot_exefs` (`rtld`, `main`, `subsdk0`…`subsdk9`, `sdk`), so
the module goes in as a normal extra module:

1. Patch the 26 hook sites in `.longshot_exefs/main` (the same IPS patches the emulator version uses; the only
   difference is that the text segment starts at file offset `0x104` instead of `0x100`).
2. Copy the prebuilt `subsdk9` to `.longshot_exefs/subsdk9`.
3. Run the builder's update:

```bat
python -I nx2/make_game.py --update "F:\Switch2\Longshot" --work "F:\Switch2\temp"
```

`tools/install_longshot.py` automates all of it (backups, patched exefs, configuration, romfs mod folder and the
builder with its own Python). The companion app has the same procedure in its **Hookshot / Longshot** target, with a
progress bar; its **Install** applies the profile and romfs once the game is built, **Recompile** runs this
update again without copying anything, and **Reinstall and recompile** starts from a clean game.

```bash
# reuse the profile and the romfs files of an existing emulator install
python tools/install_longshot.py --game "F:\Switch2\Longshot" ^
    --config "E:\...\zz-Super-Hero-Boost\exefs\subsdk9" ^
    --romfs  "E:\...\zz-Super-Hero-Boost\romfs"

# or the payload defaults (Switch buttons, no optional romfs files)
python tools/install_longshot.py --game "F:\Switch2\Longshot"
```

The update takes about as long as the first build (25–45 minutes on the reference PC: most of it is the GPU
shaders). It replaces `bin/longshot.exe` and `image.bin`; the romfs and saves are kept.

### If the builder cannot find the Vulkan headers

The update also compiles `longshot_vk.dll` (the Vulkan back end). A builder package ships the Vulkan, SPIR-V and VMA
headers in `tools/cpm`; the installer points `RX_CPM` there when that folder exists. If the builder still stops with
`no headers for longshot_vk.dll (missing Vulkan, VMA)`, the installer reuses the renderer already in the game
(`bin/longshot_vk.dll` or `bin/longshot_vk.so`) through `LONGSHOT_VK_PREBUILT`: it copies that file into the build's
work folder first, because the builder updates in place and cannot copy a file onto itself. Without an existing
renderer, install `vulkan-headers`, `spirv-headers` and `vulkan-memory-allocator` (or set the three
`LONGSHOT_*_HEADERS` variables / `RX_CPM` to a CPM cache) and run the update again.

### Why this works

The recompiler's module order is `rtld`, `main`, `subsdk0`…`subsdk9`, `sdk`, page-aligned. With `main` at
`0x80020000`, `subsdk9` lands at `0x84FA0000`, which is `main + 0x4F80000`: exactly the address the module is linked
at. The hook branches are PC-relative, so they reach it without any relocation.

The lift was checked with the patched exefs before building:

```
image: rtld@0x80000000, main@0x80020000, subsdk0@0x84EC0000, subsdk9@0x84FA0000, sdk@0x8547E000
subsdk9   72 functions, 100.00% complete, 12707 instructions, 0 untranslated
```

The module has no `DT_INIT` (init `0`), so the runtime does not need to call it; the hooks enter its code directly.

### Configuration

The configuration is a block inside `subsdk9`'s `.data` (magic `SHBCFG01`, 72 bytes, see
[GUIA-DESARROLLO.md](GUIA-DESARROLLO.md)). **The prebuilt module carries the factory block: stamina and songbook on,
HUD, button mapping and collectible counter off.** That is why a plain rebuild seems to show only the stamina bar.
Use `--config` with the module of an existing emulator install (that is what the app wrote) or patch the block by
hand before building.

The configuration also sits in `image.bin`, which is loaded into the guest at run time, so it can be changed **without
rebuilding** once the game has been built:

```bash
python tools/install_longshot.py --game "F:\Switch2\Longshot" --config "...\subsdk9" --config-only
```

The romfs side (button textures, Great Fairy, Farore, no heart drops, enemy options) is a separate copy in
`<game>/mods/zz-Super-Hero-Boost/romfs`; it can be replaced at any time with the game closed.

## Verified on the reference build

- The recompiled `bin/longshot.exe` boots to the title screen at 60 fps with `subsdk9`'s code in it.
- Longshot's `HOS_WATCH` breakpoint on `g_wrap_entry[3]` (`0x84FB4C34`) counted **148 writes** during the first
  seconds, with the value `10000000` read from the module's `.data`: the module's input hook is running in the
  recompiled game.
- The mods log shows `mods: Super-Hero-Boost: 33 RomFS files` and the resource size table and `DropGlobalParam`
  overrides being used.

## Reverting

The first install snapshots everything the install and the builder's finish step can change into
`<game>/.shb-backup/`: `files/` holds the original copies and `manifest.json` lists them, plus the files that did not
exist yet (they are deleted again on uninstall). That covers `exefs/main`, the game executable (`bin/longshot.exe`,
`bin/hookshot*`, `bin/relink-game.exe`), `image.bin`, `longshot_prepared.bin` / `relink_prepared.bin`, the launcher in
the game folder (`longshot.exe`, `hookshot*`, `ReLink.exe`), the renderer (`longshot_vk.dll`/`.so`, `relink_vk.dll`),
`SDL2.dll`, `SDL3.dll`, the ffmpeg `av*`/`sw*` DLLs, ReLink's `amd_fidelityfx_vk.dll` and VC runtime, the item-wheel
assets and `exefs/subsdk9`. The snapshot is made once: installing again never overwrites the originals, and the exefs
folder name is recorded so it is renamed back.

The builders' generated files (shader, program and pipeline caches, and their own `mods/00-dump-repair`) are **not
copied** (some are gigabytes): the manifest marks them as regenerated and the uninstall deletes them, so a cache of
the modded build cannot meet the restored game. `--uninstall` copies the originals back, deletes the files and caches
that were not there before and removes `<game>/mods/Super-Hero-Boost`. Backups made by older versions (flat files or a
version 2 manifest) are still restored. Nothing has to be recompiled when that backup exists.

## Rebuilding keeps the renderer the game was built with

The builders compile the renderer from the same sources with compile-time options. Longshot and Hookshot can be built
with **NVIDIA DLSS** (and `--fsr` for AMD FSR 4); ReLink keeps its DLSS choice in `launcher.ini`. A plain
`make_game.py --update` does **not** set those options: it compiles the renderer without DLSS and deletes the `ngx`
runtime, which leaves a game that was built with DLSS without it (and can stop it where DLSS was selected). The app
reads the launcher settings (`%LOCALAPPDATA%\LongshotLauncher|HookshotLauncher\settings.json` + `builds.json`, or
`%LOCALAPPDATA%\ReLink\launcher.ini`), points `LONGSHOT_NGX_SDK` / `RELINK_NGX_SDK` at the SDK the launcher keeps and
passes `--fsr` when the game had FSR. If the game had DLSS and no SDK is found, the build stops with a message
instead of silently removing DLSS.

The build also checks the game's module id (`e0f3ccfa` for the supported update) before patching `exefs/main`: a
different game version is refused instead of patched at the wrong offsets.

## The builder folder

The game's flavor decides which launcher is searched: Longshot in `<launcher>\toolchains\<version>\LongshotBuilder`,
Hookshot in `<launcher>\toolchains\<version>\HookshotBuilder` and ReLink in
`<launcher>\versions\<version>\ReLink`. `<launcher>` is the launcher's data folder: `%LOCALAPPDATA%\<Launcher>`
on Windows, `$XDG_DATA_HOME/<Launcher>` (or `~/.local/share/<Launcher>`) on Linux and
`~/Library/Application Support/<Launcher>` on macOS; the known spellings and any folder whose name contains the
flavor are accepted. A ReLink or Hookshot builder is never picked for a Longshot game (and the settings that decide
DLSS/FSR are read from the same launcher). The detected path is shown under the game path (**Builder detected: …
**).

On Linux and macOS the builders produce `bin/longshot`, `bin/hookshot` and `bin/relink-game` (no `.exe`), and the
app runs `make_game.py` with the builder's own `tools/python/bin/python3` (or a system `python3` when the app is
packaged), so the same install/rebuild/uninstall works there.

The builder entry is optional: it only appears when no builder was found, when one was chosen by hand
(**Builder set by you: …**) or after **Change…**; **Auto** clears it and returns to detection.
`LONGSHOT_BUILDER`, `HOOKSHOT_BUILDER` and `RELINK_BUILDER` override it, as does `--builder` in
`tools/install_longshot.py`.

## A running game

The builder replaces `bin/longshot.exe` and the other files at its final step. On Windows a running copy cannot be
replaced, so the builder stops (at about 90%) with "Longshot is still running from this folder" if one is open. The
app now closes any copy running from that folder (the root launcher, the game in `bin/`, an older
`longshot-game.exe`, a visible or hidden window) before the build, checks while it runs and closes any copy opened
meanwhile, so the swap is not blocked. Only the fallback when the copy cannot be closed (another user's or an
elevated process) still asks the player to close it. On Linux and macOS a running executable can be replaced and
the builder skips that check.

## The 0.4.0 crash (guest 0x38)

`-Oz` (enabled for the 0.4.0 module) turns on LLVM's machine outliner, which moved the `p >= 0x10000` guard of many
pointer checks into a shared `OUTLINED_FUNCTION_n` that ends in `cmp` + `ret`; the callers then branched on the
flags. Longshot's recompiler drops the flags of a `cmp` that ends a function, so after the call the `b.lo` used stale
flags: the guard stopped working and a null pointer reached `ldr x8, [x8, #0x38]` in `hud_on_code` (`read
0x100000000038, guest 0x38`, thread `BaseProcExecutor5`). `tools/build_core.py` now compiles with
`-mllvm -enable-machine-outliner=never`, so those comparisons stay inline and no call sits between `cmp` and the
branch. The module is baked into the image: the game has to be rebuilt once for the fix to apply.

## Hookshot and ReLink

Hookshot is the original project; Longshot is its continuation. Both ship the same builder (`HookshotBuilder` or
`LongshotBuilder`, with the `tools` folder beside it) and the same `nx2/make_game.py --update`. Pass its folder with
`--builder`; the rest of the procedure is identical.

ReLink is another fork of the same framework (version in `relink_version.txt`, builder under
`%LOCALAPPDATA%\ReLink\versions\<version>\ReLink`). Its game is `bin/relink-game.exe`, its exefs `.relink_exefs`, its
launcher `ReLink.exe`, and it reads the same `mods/<name>/romfs` layout, so the install and the uninstall are the
same; only the names and the `RELINK_*` environment change.
