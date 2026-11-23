# Super Hero Boost companion — Rust port

A second implementation of the companion app (`app/`, Tkinter) in Rust: the same payload, the same
`ShbConfig` bytes, the same mod folder. It exists to be readable and to be tested against the Python app
byte for byte, not to replace it: the Python app still covers a few things this one does not (see
[Not ported yet](#not-ported-yet)).

```
app-rs/
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
│   ├── shb-cli/                `shb`: profiles, install, uninstall, show, patch, lang
│   └── shb-gui/                `shb-gui`: the window (egui / eframe)
└── tools/
    ├── parity_check.py          the Rust app against the Python app, byte for byte
    ├── gen_i18n.py              the window's texts, from app/i18n.py
    └── gen_gamecube.py          the GameCube artwork, from app/gamecube_textures.py
```

## Build and run

```bash
cd app-rs
cargo build --release
target/release/shb.exe profiles
target/release/shb.exe install --emu "E:/Switch2/Emu/Fritchii-v4.1-Windows-x64/Fritchii-v4.1" \
    --profile recommended --payload ../app/payload
target/release/shb-gui.exe          # the window; SHB_EMU pre-fills the emulator folder
```

The payload (`app/payload`) is not committed: point `--payload` at it, or set `SHB_PAYLOAD`. The CLI and the
window both look next to the program and then in `app/payload` of the checkout.

`shb show --emu <folder>` prints what the installed module currently says, and `shb show --nso <file>` reads any
module image — that is how a settings change can be confirmed without starting the game.

### Toolchain

On this PC there is no MSVC C++ workload (the BuildTools install has no `cl.exe`), so the toolchain is
`stable-x86_64-pc-windows-gnu`; `rustup-init -y --default-toolchain stable-x86_64-pc-windows-gnu` is enough.
On a machine with Visual Studio, the default `msvc` toolchain is fine, and it is what CI uses.

Two shims are needed for the GNU target here, both already in the repo or the machine:

- `~/.cargo/bin/dlltool.exe` — a copy of LLVM's `llvm-dlltool.exe`; the gnu target calls `dlltool` at link time.
- `app-rs/.cargo/config.toml` — links with `rust-lld`. With the GNU `ld`, any binary that linked `wgpu` (or
  `glutin`) crashed **before `main`**: access violation, no output, no marker file written by the first line of
  `main`. `rust-lld` builds the same crates correctly. This only applies to the `x86_64-pc-windows-gnu` target.

`shb-gui --version` runs without touching the graphics stack, which is the quickest way to tell a broken link
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
cargo build                                   # shb must exist in target/debug
python tools/parity_check.py --app ../app --payload ../app/payload --mode all
```

Result on this branch (2026-11-23): 56/56 patched images identical (16 profiles + 40 random settings, including
out-of-range HUD scales, stray remap entries and the 0.4.x block sizes), **16/16 install trees identical** for every
profile, **5/5 recompiled games identical** (module, `image.bin` and romfs) and **7/7 Rekarina installs identical**
(the module, `Rekarina.exe` with its hash rewritten, the `mods_order` line, the romfs and the marker file), the enemy
packs included: the `.zs` files are compared after decompression with the game's own dictionary (the Rust encoder is
not required to emit the same frame bytes, and Python's libzstd reads ours).

The install comparison needs the real `zstandard` module (`py -3.12 -m pip install zstandard`); without it the
profiles with enemy packs are skipped and the rest still runs, because the config patching does not use it.

## The window

The Python app's eight pages (Install, Presets, Gameplay, Difficulty, Controls, HUD, Misc, About), its palette
and its two fonts (Segoe UI and Palatino Linotype), its logo in the header, and **its texts**: `i18n.rs` uses
`i18n_table.rs`, generated by `tools/gen_i18n.py` from `app/i18n.py` **and the pages that keep their own texts**
(`remap_ui.py`, `texts_enemies.py`: 443 strings in the same six languages). `--check` fails when the table is stale,
and `tools/check_gui_keys.py` fails when the window asks for a key that is not in it — a missing key shows the key
itself on screen, which a screenshot does not make obvious. CI runs both.

The **?** windows open in one place (`CENTER_TOP`, with the window's own margins), close on a click outside, and a
click on a card's name or text selects it — an interaction drawn over the whole card used to swallow the "?". The "?"
itself is a bare gold mark, without a frame. Every interactive widget shows the hand cursor. The chosen option of a
set is a gold pill with dark text (`widgets::pill`), and the theme's `selection` colours are dark gold/light gold for
the lists that still use them: cream text on gold was unreadable. The star of "Recomendado ★" is drawn
(`widgets::star`), and so are the flag of each language (`widgets::flag`), because the installed fonts carry neither
glyph.

The buttons of the remap list are drawn, not written: the same chip the preview paints, with a "+" for the long press
of the face button above it.

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

The install page has the Python app's three targets: emulator, recompiled game and Rekarina. All three write the
settings; the two that need a compiler (a recompiled game's rebuild and Rekarina's lift) are Rekarina's and
Longshot's own tools, and this app does not start them.

## The rebuild goes through the Python app

Lifting and compiling the game stays where it is: `recomp.build` (Longshot/Hookshot's builder, its toolchain and its
environment) and `rekarina.build` (Rekarina's three stages) live in `app/`, and this port runs them instead of
copying them:

```bash
shb rebuild --game "F:/Switch2/Longshot/rc" --profile recommended      # rebuild, then the settings
shb rebuild --game "F:/Switch2/rekarina" --profile souls --no-rebuild  # settings only
```

`shb rebuild` runs `app/rebuild.py` (the small entry point added for this), streams the build's own output, and
returns its exit code; the window's Install page has the same action as "Recompilar" for the recompiled-game and
Rekarina targets, with the log in the card below. `SHB_PYTHON` picks the interpreter (`py -3.12` on Windows by
default) and `--app` the Python folder when it is not the payload's parent.

## Not ported yet

Nothing of the install is reimplemented: the two flows that need a compiler are run from the Python app through
`shb rebuild` (above), so there is no second copy of the builder logic to drift. The window and the CLI both say when
a game needs that build instead of writing a module the game would not load.

Known gap of the environment, not of this port: the builder the Python app drives (`tools/nx2/make_game.py`) is not
on this PC any more (the installed Longshot is the .NET `LongshotBuilder`), so a Longshot rebuild fails for the
Python app too until the two are matched.

The GameCube diagram (`gamecube.rs`) is ported: the artwork comes from `gamecube_textures.rs`/`.bin`, generated from
`app/gamecube_textures.py` by `tools/gen_gamecube.py` (base85 + zlib decoded once, at generation time, so the Rust
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
folder (57 tests). `cargo test -p shb-gui` drives a dropdown headlessly (open it, hover an item, click it) and checks
that the HUD preview's coordinates are the ones `core/hud.c` uses.
