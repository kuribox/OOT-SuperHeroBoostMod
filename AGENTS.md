# AGENTS.md — Super Hero Boost

Working agreement for every agent that touches this repo. It is the first file to read after this one:
**`docs/AGENT-BRIEF.md`** (how the mod works, condensed). Deep references: `docs/GUIA-DESARROLLO.md`
(architecture, hooks, HUD, build) and `docs/RE-NOTES.md` (addresses, formats, what failed and why).

Target: the **Ocarina of Time remake** for Nintendo Switch 2, update **v131072**, `main` Build ID
`E0F3CCFAF50065AE4EA01EB5AEB13B1A45DA3348`. Current version: **0.9.3** (`app/version.py`).

## Rules (non-negotiable)

1. **Branch first, and everything lands in `development`.** Work always starts on a branch (`feat/…`,
   `fix/…`, `docs/…`). Once the user says it is good, the branch merges into **`development`** with `--no-ff`,
   so it stays visible: `git checkout development && git merge --no-ff <branch>`. `development` reaches `main`
   only when a release is cut, also with `--no-ff` (`git checkout main && git merge --no-ff development`).
   Never commit to `main` or `development` directly, and never merge on your own initiative.
2. **Code first, tests after.** Implement and build, then prove it in the emulator or a recomp. A successful
   build is not evidence that a feature works. Report what was verified and what was only assumed.
3. **Live tests go through the emulator's debug server** (memory reads/writes and synthetic input), not by
   clicking around. Recipe below.
4. **Any live test that writes equipment or GameData risks the save.** Copy
   `<emulator>\user\nand\user\save` (the game's own backup folder `user\save_backup_*` is not automatic) before
   the test.
5. **Never commit the dump, `app/payload/` or generated romfs data.** `.gitignore` covers it;
   `python tools/check_no_game_data.py` checks it.
6. **Keep the module relocation-free.** No jump tables (`-fno-jump-tables`), no tail calls
   (`-fno-optimize-sibling-calls`), no pointers in data. The NSO loader has no relocations and ReLink's
   translator mis-splits functions with tail calls; `build_core.py` enforces both.
7. **Append config fields only at the end** of `ShbConfig` (`core/config.h`) — the app patches by offset.
8. Code, comments and docs in English; answer the user in the language they write in (Spanish).

## Where things are on this PC

| What | Path | Notes |
|---|---|---|
| Repository | `F:\project\super-hero-boost` | `origin` = github.com/kuribox/OOT-SuperHeroBoostMod (https, credentials cached; `gh` is **not** installed) |
| Test emulator | `E:\Switch2\Emu\Fritchii-v4.1-Windows-x64\Fritchii-v4.1` | `fritchii.exe` (GUI), `lichii.exe` (debug launch); mods in `user\load\0400486024DEC000` (`zz-Super-Hero-Boost` installed); `user\config\qt-config.ini` holds `load_directory`; log `user\log\eden_log.txt` |
| Game dump | `E:\Switch2\Roms\Zelda Ocarina.nup` | 25.6 GB; `E:/Switch2/Roms/NX/Zelda Ocarina.nx` is the path in the emulator's recent files |
| Longshot | `F:\Switch2\Longshot\rc` | static recomp: `bin\longshot.exe`, `image.bin`, `mods\`, `.shb-backup\`, `logs\` |
| ReLink | `F:\Switch2\Relink` | static recomp: `bin\relink-game.exe`, `.relink_exefs`, `ReLink.exe` |
| Rekarina | `F:\Switch2\rekarina` | `game\` (exefs — includes the stock `main`) + `setup\` |
| LLVM toolchain | `C:\Program Files\LLVM\bin` | clang 23.1.5, ld.lld, llvm-objcopy/size/nm — works for the module |
| Stock `main` | `F:\Switch2\rekarina\game\exefs\main` | NSO0, Build ID matches the supported update |
| Devtool | `F:\project\oot\vendor\devtool\oot-remake-devtool-main` | `lichii.py`: `DebugClient`, `HostMemory`; needed by `live_check.py`, `hud_tune.py` |

The paths `docs/GUIA-DESARROLLO.md` names under `F:\project\oot` (`tools\llvm-mingw…`, `data\stock-exefs\main`,
`vendor\devtool`) no longer exist on this PC. Use the table above through the environment variables:

```bash
export SHB_LLVM="C:/Program Files/LLVM/bin"
export SHB_STOCK_MAIN="F:/Switch2/rekarina/game/exefs/main"
export SHB_DEVTOOL="F:/project/oot/vendor/devtool/oot-remake-devtool-main"
export SHB_EMU="E:/Switch2/Emu/Fritchii-v4.1-Windows-x64/Fritchii-v4.1"
export SHB_GAME="E:/Switch2/Roms/Zelda Ocarina.nup"
```

## Build and test loop

```bash
# 1. module + IPS for the emulator/standalone variant the app installs (~2 s)
python tools/build_core.py --features controls,manual_shield,stamina,songbook,button_mapping,quick_select \
    --slots --compat --base 0x4F80000
# --debug-dump keeps the save:/slot_00/shbdebug.bin diagnostic (it creates files in the player's save
# volume): test builds only, never the release payload.
# 2. install with the app or its CLI (this overwrites ShbConfig with the chosen profile)
python app/main.py --cli --emu "$SHB_EMU" --profile recommended
# 3. run, load the save, read diagnostics, take screenshots
python tools/live_check.py --load-a 4 --keep --linger 3
```

`--compat --base 0x4F80000` is the variant the installer copies. If `SHB_STOCK_MAIN` is wrong the build stops
on the first hook-site check with the found/expected instruction; that is the intended failure.

### Loading the game (the user's procedure)

1. Start `lichii.exe -g "<nup>"` with `EDEN_NX2_DEBUG_SERVER=127.0.0.1:6545` (`live_check.py` already does).
2. Wait for the title screen (~35 s).
3. Press **A four times, ~6 s apart**. That is what opens the first save slot and starts the game; presses
   sent earlier are lost.
4. Wait ~30 s for the save to load. A black loading screen is a small PNG, gameplay is >150 kB — that is how
   `live_check.py` decides it is in game.

Useful tools once it is running: `tools/hud_tune.py` (HUD look and options live), `tools/disasm.py`
(disassemble `main`), `tools/memory` probes in `tools/` for actor/player state. The debug server serves one
request at a time: while an input sequence runs, reads wait — measure through `HostMemory` instead.

## Recompilations

- `exefs` mods are ignored by design: only `mods/<name>/romfs` is read at run time. Code changes mean a rebuild.
- Longshot/ReLink place the module as `subsdk9` at `main + 0x4F80000`, the module's link base, so the hook
  branches reach it without relocations.
- Rekarina builds for `main + 0x5200000` with `--rekarina` (synthetic `eh_frame_hdr` seed table).

## Reference material (external)

- **https://github.com/Kabosune/oot-remake-modding** — modding docs for this game: symbol/command lists for build
  `e0f3ccfa` (`symbols/e0f3ccfa-commands.csv`, `.ld`, `.toml`), structure notes. Search the symbol list before
  reverse-engineering a function by hand.
- **https://github.com/platinumagpie/oot-remake-devtool** — the devtool: `lichii.DebugClient`, `HostMemory`, GameData
  and BYML helpers. Local clone at `F:\project\oot\vendor\devtool\oot-remake-devtool-main` (`SHB_DEVTOOL`).
- Both are reference only: no code from them is bundled, and the game dump is never committed or shared.

## Known gaps (November 2026)

- Longshot's installed builder is `LongshotBuilder` **v3.0.0-rc.7 (.NET)** under
  `%LOCALAPPDATA%\LongshotLauncher\toolchains\v3.0.0-rc.7\LongshotBuilder`; `tools/install_longshot.py` and
  `app/recomp.py` look for the older python builder (`tools/nx2/make_game.py`). A rebuild through them will
  fail until the builder matches — check before promising a Longshot rebuild.
- The Longshot builder keeps its work at `%LOCALAPPDATA%\LongshotLauncher\work` (`image/`, `sa/gen/`,
  `objcache/`); the lifted C chunks appear there during a build, not in the game folder.
- `gh` is missing: open PRs and releases through `git push` + the GitHub web UI, or install the CLI.
