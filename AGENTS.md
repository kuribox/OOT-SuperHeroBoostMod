# AGENTS.md — Super Hero Boost

Working agreement for every agent that touches this repo. It is the first file to read after this one:
**`docs/AGENT-BRIEF.md`** (how the mod works, condensed). Deep references: `docs/GUIA-DESARROLLO.md`
(architecture, hooks, HUD, build) and `docs/RE-NOTES.md` (addresses, formats, what failed and why).

Target: the **Ocarina of Time remake** for Nintendo Switch 2, update **v131072**, `main` Build ID
`E0F3CCFAF50065AE4EA01EB5AEB13B1A45DA3348`. Current version: **0.9.3** (`app/version.py`).

## Rules (non-negotiable)

1. **Branch first, never commit to `main`.** Use `feat/…`, `fix/…`, `docs/…`. Merge into `main` only after
   the user says it is good, and use `--no-ff` so the branch stays visible:
   `git checkout main && git merge --no-ff <branch>`. Never merge on your own initiative.
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

## The Rust configurator (`app-rs/`, branch `feat/rust-app`)

A port of the app in Rust, byte-identical to the Python one (`tools/parity_check.py`: 56/56 patch images, 16/16
install trees, 2/2 Rekarina installs). Build: `export PATH="/c/Users/Kuri/.cargo/bin:$PATH"` then
`cargo build --release -p shb-gui` in `app-rs/`; the window is eframe/egui 0.37 with the **wgpu** backend (glow
segfaults here).

- **The install is native, and the payload travels in the binary.** `shb-core` writes the module, the config block,
  the IPS, the romfs and the enemy packs itself; nothing of the app runs in Python. `build.rs` embeds `app/payload`
  (1 169 files, 27 MB) so `shb.exe` / `shb-gui.exe` work alone, unpacking it once into
  `%LOCALAPPDATA%\SHB-OcarinaTweaks\payload\<content stamp>\`; a `payload/` folder beside the exe still wins.
- **The rebuild is native for recompiled games** (`shb-core/src/recomp_build.rs` + `rebuild.rs`): it finds the builder
  (Longshot's launcher keeps its toolchains under `%LOCALAPPDATA%\LongshotLauncher	oolchains\<version>\`), works out
  DLSS/FSR/XeSS from the launcher's files, and drives `tools/nx2/make_game.py`, reading its `@@stage`/`@@progress`
  lines. The recompiler itself stays an external program — that is its compiler, not ours.
- **Rekarina's rebuild is native too** (`shb-core/src/rekarina_build.rs`): it runs the setup's own `gen.py`,
  `build_game.py` and its converters (they are Rekarina's tools) but does this app's part — ExeFS staging, the
  patch tables, the extra `main` seeds, the expected hashes, the base check — in Rust. `app/rebuild.py` is unused.
  NSO sizes are `data`'s memory + decompressed + bss at `0x3c`, aligned to a page; modules are placed on **2 MiB**
  boundaries, which is what makes `subsdk9` land at `main+0x5200000`. A run against `F:\Switch2
ekarina` reports
  `exefs: … .ips (49 sites)`, `74 extra main seeds`, 553 generated C files in 4 modules and 701 compile steps —
  the same numbers the Python reference gives.
- **Verified end to end (2026-11-24)**: a Rekarina rebuild driven by `shb rebuild` produced a 374 MB `Rekarina.exe`,
  wrote the settings into the module and its SHA-256 into the executable, and left `state.json` with `rebuilt: true`;
  `shb game` afterwards patched the module and the hash again without rebuilding. The recomp driver reaches further:
  it found Longshot v3.1.1's own builder and started its build (which then failed on a scratch folder because that
  builder's compiler could not write temp files there — its toolchain, not ours).
- **A full rebuild needs the machine's memory**: `build_game` gates the compile on free RAM (one job when the emulator
  holds 9 GB). A test build must run in a game copy that keeps `exefs/`, `mods/`, `Rekarina.exe` and `rekarina.ini`
  beside the shared `setup/` (the romfs of a real install is 23 GB and the lift does not need it).
- Dev aids: `SHB_PAGE=<install|presets|gameplay|difficulty|controls|hud|misc|help|about>` opens a page directly,
  `SHB_WINDOW=WxH` sizes the window and `SHB_HELP=<key>` opens one help text — how the screenshots in
  `docs/capturas/` are taken. `tests` before a release build, and `Stop-Process -Name shb-gui -Force` before
  rebuilding (else `os error 5` on the copy).
- `tools/check_gui_keys.py` must pass after touching the window: a missing text key is shown raw on screen.

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

## Rust port of the companion app (`app-rs/`)

Branch `feat/rust-app`: a second implementation of the app, to learn Rust against a known-good reference. It is
**not** what the installer ships; the Python app in `app/` is.

- Layout: `crates/shb-core` (settings, `ShbConfig` offsets, remap, presets, install, IPS, path/language
  detection), `crates/shb-cli` (`shb`), `crates/shb-gui` (egui window, English/Spanish only).
- **Parity is the contract**: `py -3.12 app-rs/tools/parity_check.py --app app --payload app/payload --mode all`
  patches and installs with both implementations and compares bytes. On 2026-11-23: 56/56 images identical
  (16 profiles + 40 random settings), 16/16 install trees identical and 5/5 recompiled games identical (module,
  `image.bin`, romfs), the enemy packs included (those compare after decompression with the game's dictionary: two
  zstd encoders need not emit the same frame). Python 3.12 is the one with `zstandard`; without it the pack
  profiles are skipped. Change the Python `patch_config`, `_patch_packs` or `recomp.apply_settings` and this check
  must be rerun.
- The offset table is `config::off`; `patch_config` never writes past the size the module reports, so old blocks
  (0.4.x = 0x48) keep working.
- Enemy packs: `crates/shb-core/src/enemy.rs`, with the pure-Rust `structured-zstd` crate (the C-backed one needs
  a C compiler this toolchain does not have). The float scales are computed in `f64` and narrowed once, exactly
  like the Python app's `struct.pack_into`; doing the product in `f32` matched everything but the `x3`/`x1.5` attack
  cadence.
- Recompiled games: `crates/shb-core/src/game.rs` finds a Longshot/Hookshot/ReLink folder (`.longshot_exefs` +
  `bin/longshot.exe` …), backs up once into `.shb-backup`, writes the module, patches `image.bin` and copies the
  romfs. What it cannot do is the build: the code is compiled into the executable by the external builder, so the
  app says "needs a build" instead of pretending. Rekarina is detected (`rekarina.ini` + `exefs/main`) and not
  written.
- The window's texts are generated: `app-rs/tools/gen_i18n.py` turns `app/i18n.py` into
  `crates/shb-gui/src/i18n_table.rs` (389 keys × 6 languages). `--check` fails when it is stale; CI runs it.
- The updater (`crates/shb-core/src/update.rs`): GitHub's latest release, this platform's asset, the SHA-256 check
  and the PowerShell restart script. `ureq` runs with `native-tls` on purpose: the rustls backend needs `ring`,
  whose build wants a C compiler this toolchain does not have.
- Not ported: the two flows that run somebody else's compiler — the recompiled-game rebuild (Longshot/Hookshot/
  ReLink, whose installed builder is now the .NET one and not `tools/nx2/make_game.py`) and Rekarina's three-stage
  lift and compile. Everything around them is ported, including Rekarina's settings half (`rekarina.rs`: config
  block, the module hash in `Rekarina.exe`, `mods_order`, `build-cache/mods-built.txt`) and the GameCube diagram
  (`gamecube.rs`: the game's own UI atlas patched with the artwork generated from `app/gamecube_textures.py`).
- The PlayStation/Xbox prompt pack (`crates/shb-core/src/layouts.rs`) is ported: GameBanana download, ZIP extract
  of the three files, `RESTBL` parse/merge/compress (zstd 19) and the atlas copy, with the safe font from the
  payload. It is the last step of the romfs install, so its table wins the merge.
- Toolchain on this PC: `stable-x86_64-pc-windows-gnu` (there is no MSVC C++ workload). Two things make it work:
  `llvm-dlltool.exe` from LLVM copied to `~/.cargo/bin/dlltool.exe` (the gnu target needs `dlltool`), and
  `app-rs/.cargo/config.toml`, which links with `rust-lld`. GNU `ld` produced binaries that segfaulted **before
  `main`** (access violation, no output, not even in a build with only `wgpu` linked) — `rust-lld` builds the same
  crates correctly. Diagnose this kind of crash by writing a marker file as the first line of `main`: bare `winit`
  and bare `egui` windows run fine under GNU `ld`, so it is the link, not the code.
- egui 0.37 API: `eframe::App` implements `fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame)`, and
  panels are `egui::Panel::top/bottom("id").show(ui, ...)` + `egui::CentralPanel::default().show(ui, ...)` — not
  the older `TopBottomPanel` + `update(ctx)` shape.
