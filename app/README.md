# Super Hero Boost companion — the current app

The configuration app of Super Hero Boost, in Rust: one executable (the window, and the command line when it gets a
command), with the mod's files inside it. It was written against the first app — now `app-legacy/`, Tkinter — and
keeps its payload, its `ShbConfig` bytes, its mod folder, its settings file and its texts, so the two are
interchangeable; `tools/parity_check.py` holds them to each other byte for byte. A behaviour changed in the legacy
app has to be changed here too, and the other way round.

```
app/
├── Cargo.toml                  workspace
├── crates/
│   ├── shb-core/               the library: everything but the screen
│   │   ├── src/settings.rs       the user's options + their JSON (the Python app's file)
│   │   ├── src/roles.rs          the button remap table, and what the module accepts
│   │   ├── src/config.rs         the ShbConfig block: the offset table and the writer
│   │   ├── src/presets.rs        the game profiles
│   │   ├── src/enemy.rs         the enemy speed / aggression packs (zstd, the game's dictionary)
│   │   ├── src/game.rs          a recompiled game (Longshot/Hookshot/ReLink): module, image.bin, romfs
│   │   ├── src/layouts.rs       the downloadable PlayStation/Xbox prompts + the resource table merge
│   │   ├── src/gamecube.rs      the GameCube diagram: the game's UI atlas, patched and recompressed
│   │   ├── src/rekarina.rs      a Rekarina install: the module's block, its hash in the exe, mods_order
│   │   ├── src/update.rs        the GitHub update check, the download and the restart
│   │   ├── src/romfs.rs          what the settings mean for the game's files, per target
│   │   ├── src/zstd.rs           the packed files
│   │   ├── src/install.rs        install / uninstall of the mod folder
│   │   ├── src/ips.rs            IPS32 read + merge
│   │   ├── src/find.rs           mods folder, emulator program, game language
│   │   └── src/paths.rs          where the settings file lives
│   ├── shb-cli/                the executable: the window alone, or profiles / install / rebuild / show / …
│   └── shb-gui/                the window (egui / eframe), a library the executable links
└── tools/
    ├── parity_check.py          the Rust app against the Python app, byte for byte
    ├── gen_i18n.py              the window's texts, from app-legacy/i18n.py
    └── gen_gamecube.py          the GameCube artwork, from app-legacy/gamecube_textures.py
```

## One executable, two faces

`cargo build --release -p shb-cli` writes `target/release/SHB-OcarinaTweaks.exe` — the whole app in one file, with the
payload inside it, the icon and the version in its properties:

```bash
SHB-OcarinaTweaks.exe                       # the window (a double click, a shortcut)
SHB-OcarinaTweaks.exe install --emu <dir>    # the CLI (see `--help`)
SHB-OcarinaTweaks.exe --gui                  # the window, said out loud, for a script
```

The window is `shb-gui`, a library: `shb-cli` links it, so there is no second binary and no way for the two to
disagree. The icon is `crates/shb-cli/assets/icon.ico` (the Python app's own picture), compiled into the file by
`crates/shb-cli/build.rs` with `llvm-rc`.

## Build and run

```bash
python tools/build_app.py       # from the repository root: cargo build --release -p shb-cli, then release/
# or, by hand:
cd app && cargo build --release
target/release/SHB-OcarinaTweaks.exe                                   # the window; SHB_EMU pre-fills the folder
target/release/SHB-OcarinaTweaks.exe profiles
target/release/SHB-OcarinaTweaks.exe install --emu "E:/Switch2/Emu/Fritchii-v4.1-Windows-x64/Fritchii-v4.1" \
    --profile recommended --payload ../app/payload
target/release/SHB-OcarinaTweaks.exe rebuild --game "F:/Switch2/Longshot/rc" --profile recommended
```

A long build holds the release executable, so a second build goes somewhere else: `CARGO_TARGET_DIR=target-dev cargo
build --release` (both folders are ignored by git).

The payload (`app/payload`) is not committed: point `--payload` at it, or set `SHB_PAYLOAD`. The CLI and the
window both look next to the program and then in `app/payload` of the checkout.

`SHB-OcarinaTweaks show --emu <folder>` prints what the installed module currently says, and `show --nso <file>` reads
any module image — that is how a settings change can be confirmed without starting the game.

The executable is built as a **window** program: double-clicking it opens the window and no console. A command prints
to the terminal that ran it (`console::attach` in `main.rs`); when its output is redirected it is left alone.

### Toolchain

On this PC there is no MSVC C++ workload (the BuildTools install has no `cl.exe`), so the toolchain is
`stable-x86_64-pc-windows-gnu`; `rustup-init -y --default-toolchain stable-x86_64-pc-windows-gnu` is enough.
On a machine with Visual Studio, the default `msvc` toolchain is fine, and it is what CI uses.

Two shims are needed for the GNU target here, both already in the repo or the machine:

- `~/.cargo/bin/dlltool.exe` — a copy of LLVM's `llvm-dlltool.exe`; the gnu target calls `dlltool` at link time.
- `app/.cargo/config.toml` — links with `rust-lld`. With the GNU `ld`, any binary that linked `wgpu` (or
  `glutin`) crashed **before `main`**: access violation, no output, no marker file written by the first line of
  `main`. `rust-lld` builds the same crates correctly. This only applies to the `x86_64-pc-windows-gnu` target.

`SHB-OcarinaTweaks.exe --version` runs without touching the graphics stack, which is the quickest way to tell a broken link
from a broken window.

## The two things worth reading first

1. **`config.rs`** — `off` is the whole ABI in one block of constants: every field of `core/config.h`, with its
   offset. `patch_config` writes through `Fields`, which bounds-checks every write against the size the module
   reports, so an older module only gets the fields it has. Nothing is written past `off::SIZE`.
2. **`settings.rs`** — the same JSON the Python app writes, read with `#[serde(default)]`: an old settings file
   loads, a newer one loses only the fields this build does not know. The enums (`HudMode`, `ButtonStyle`,
   `BmLayout`, `LMode`, `Lang`) serialize as the lowercase strings the Python app used.

## Parity with the Python app

`tools/parity_check.py` imports the Python app, builds the settings of every profile plus a batch of random
ones, patches the payload module with both implementations and compares the images; `--mode install` installs
both into throwaway folders and compares every installed file (module, IPS, button art, romfs options).

```bash
cargo build --release -p shb-cli
python tools/parity_check.py --app ../app-legacy --payload ../app/payload --mode all
```

Result (0.10.0, after the app moved to `app/`): 56/56 patched images identical (16 profiles + 40 random settings,
including out-of-range HUD scales, stray remap entries and the 0.4.x block sizes), **7/7 install trees identical** for
every profile, **7/7 recompiled games identical** (`main` with its hooks, the module, `image.bin` and romfs) and
**7/7 Rekarina installs identical**
(the module, `Rekarina.exe` with its hash rewritten, the `mods_order` line, the romfs and the marker file), the enemy
packs included: the `.zs` files are compared after decompression with the game's own dictionary (the Rust encoder is
not required to emit the same frame bytes, and Python's libzstd reads ours).

The install comparison needs the real `zstandard` module (`py -3.12 -m pip install zstandard`); without it the
profiles with enemy packs are skipped and the rest still runs, because the config patching does not use it.

## The window

The Python app's eight pages (Install, Presets, Gameplay, Difficulty, Controls, HUD, Misc, About), its palette
and its two fonts (Segoe UI and Palatino Linotype), its logo in the header, and **its texts**: `i18n.rs` uses
`i18n_table.rs`, generated by `tools/gen_i18n.py` from `app-legacy/i18n.py` **and the pages that keep their own texts**
(`remap_ui.py`, `texts_enemies.py`: 443 strings in the same six languages). `--check` fails when the table is stale,
and `tools/check_gui_keys.py` fails when the window asks for a key that is not in it — a missing key shows the key
itself on screen, which a screenshot does not make obvious. CI runs both.

The **?** windows open in one place (`CENTER_TOP`, with the window's own margins) and close on a click outside — but
not on the click that opened them, which is outside the window that did not exist yet when it landed
(`closes_help`; without that every "?" looked dead). A
click anywhere on a card selects it: the interaction is registered from the *previous* frame's rectangle, before the
contents are drawn, so the whole card responds (with the hand cursor) and the "?" inside it still gets its own click.
An interaction drawn over the card *after* the contents used to swallow the "?". The "?" itself is a bare gold mark
close to its sentence, without a frame. Every interactive widget shows the hand cursor. The chosen option of a
set is a gold pill with dark text (`widgets::pill`), and the theme's `selection` colours are dark gold/light gold for
the lists that still use them: cream text on gold was unreadable. The star of "Recomendado ★" is drawn
(`widgets::star`), and so are the flag of each language (`widgets::flag`), because the installed fonts carry neither
glyph.

The buttons of the remap list are drawn, not written: the same chip the preview paints. A long press (A, B, X, Y, L,
R and the D-pad right) is a row of its own, indented under its button and tagged **Hold**, with one sentence over the
list saying so; a hold the button cannot have is greyed out with the reason, and each hold has its own *skip the tap
action when held* checkbox.

The stamina display and the stamina styles are **options** (`ui.radio_value`) with their sentence under them, not
pills: a row of gold buttons for two exclusive choices read as two commands.

Every option carries the Python app's one-sentence explanation under it, and the gold **?** beside a control opens the
full text in a window. The three previews are drawn with egui's painter: the stamina ring or bar (Gameplay), the
controller with each button's role, and the in-game HUD cluster at the positions of `core/hud.c` (Controls).

The pages adapt to the window: the stamina card, the remap editor and the presets put their second column beside the
first when there is room for it (`widgets::two_columns`) and under it when there is not, the previews scale to the
width they are given, and the grids that hold six or sixteen things go from two columns to one. Nothing is a fixed
pixel width, so the window can be small without the content running out of the right edge.

Two environment variables help while working on the window: `SHB_PAGE=difficulty` opens straight on a page and
`SHB_WINDOW=1300x1700` opens a window that tall, which is how the screenshots in `docs/capturas` are taken.

The presets are the Python app's cards: every profile with the first sentence of what it changes, the rest behind a
"?", the active one in gold, and the same "Volver a aplicar el perfil" button.

The install page has the three targets: emulator, recompiled game and Rekarina. All three write the settings, and the
last two compile the game (the four actions: Install, Apply settings, Recompile, Reinstall and recompile). The
folders are looked at in a thread when they change or an action ends — a Rekarina install hashes a 350 MB exe to know
whether it is built — and every action ends in a window that says how it went; a failed install is rolled back.

## The payload travels inside it

The release binary carries the payload: `crates/shb-core/build.rs` turns `app/payload` into one `include_bytes!` per
file when the folder is there at build time (the CI stages it with `tools/ci_payload.py`), and the app unpacks it on
first use into `<data>/SHB-OcarinaTweaks/payload/<stamp>/` — `%LOCALAPPDATA%` on Windows, `~/Library/Caches` on macOS,
`$XDG_CACHE_HOME` on Linux. The stamp hashes the payload's content, so a rebuilt payload unpacks into a new folder
instead of mixing with the old one. `SHB-OcarinaTweaks.exe` (44 MB) installs the mod with nothing beside
them; a `payload/` folder next to the binary (or `SHB_PAYLOAD`) still wins, which is how the app is developed.

`SHB_NO_EMBED_PAYLOAD=1` builds without it, `SHB_PAYLOAD_DIR` embeds another folder. The executable then needs
`payload/` beside it, and the parity checker exercises that path.

## The rebuild, in this app

`shb rebuild --game <folder> --profile <name>` (and the window's rebuild button) drives every build from here — no
part of this app runs in Python:

* a static recompilation (Longshot, Hookshot, ReLink): `shb-core/src/recomp_build.rs` finds the builder the launcher
  installed (`%LOCALAPPDATA%\<Launcher>	oolchains\<version>\LongshotBuilder`, or one beside the game, or `--builder`),
  works out how the game was built (DLSS/FSR/XeSS from the launcher's `settings.json` + `builds.json`, or ReLink's
  `launcher.ini`, or the game's own `*_options.ini`), passes the builder its arguments and environment, and reads the
  `@@stage` / `@@progress` protocol it prints. The recompiler's `make_game.py` and its toolchain are external
  programs, the way a C project's compiler is: **no part of this app runs in Python** for a recompiled game.
* a Rekarina install: `shb-core/src/rekarina_build.rs` runs Rekarina's own three stages as Rekarina's tools (its
  private interpreter, `recomp/gen.py`, `tools/build_game.py`, `tools/cheats.py`, `tools/exefs_patches.py`), and does
  this app's part of it here: stage the ExeFS, convert the mods' patch tables, compute the extra `main` seeds our
  hooks need (74 mid-function resume points, the same the Python reference computes), write the expected-hash table
  with our module, check the base the lifter used, and replace the executable atomically.

## Everything the legacy app does

The install of every target (emulator, recompiled game, Rekarina), its uninstall, the rebuild in all four flavours,
the settings file, the presets and profiles, the enemy packs, the downloadable controller layouts, the GameCube UI
artwork (`shb-core/src/gamecube.rs`, cut from the game's own archive), the updater, and **Play**: the button opens the
emulator, the recompiled game's launcher or Rekarina's own executable, detached from the window (`shb play --emu <dir>`
from the CLI).

The GameCube diagram (`gamecube.rs`) is ported: the artwork comes from `gamecube_textures.rs`/`.bin`, generated from
`app-legacy/gamecube_textures.py` by `tools/gen_gamecube.py` (base85 + zlib decoded once, at generation time, so the Rust
side needs neither). `make_atlas` reads the game's own 244 MB UI archive (from `<install>/romfs` or `$SHB_ROMFS`),
checks a SHA-256 per region before touching it — a different archive is refused, never patched blind — and
recompresses at level 6. `disable_legacy_atlas` switches a standalone `GameCube-System-Controller` off the same way
the Python app does.

The PlayStation/Xbox prompt pack (`layouts.rs`) is ported: the GameBanana download, the ZIP extract of the three
files, the `RESTBL` parse/merge/compress and the atlas copy with the safe font from the payload. It runs last in the
romfs install, so its resource table wins the merge.

The updater is ported too: `shb update [--download|--apply] [--current <version>]` asks GitHub for the latest
release, picks this platform's asset, verifies the SHA-256 GitHub publishes and, on Windows, replaces the running
program through a detached PowerShell script. The window's About page drives the same code in a background thread.
The HTTP client uses the system TLS (schannel) because the rustls backend needs a C compiler this toolchain does
not have.

`cargo test -p shb-core` covers the offset table, the remap rules, the presets, the IPS reader, the mods folder
detection, the enemy packs (a real trained dictionary, scaled values read back) and a full install into a temp
folder (80 tests). `cargo test -p shb-gui` drives a dropdown headlessly (open it, hover an item, click it) and checks
that the HUD preview's coordinates are the ones `core/hud.c` uses. `tests/settings_compat.rs` loads the settings files
the legacy app wrote (`tests/data/`) and requires every key and value back unchanged.
