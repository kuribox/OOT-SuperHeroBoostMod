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
│   │   ├── src/install.rs        install / uninstall of the mod folder
│   │   ├── src/ips.rs            IPS32 read + merge
│   │   ├── src/find.rs           mods folder, emulator program, game language
│   │   └── src/paths.rs          where the settings file lives
│   ├── shb-cli/                `shb`: profiles, install, uninstall, show, patch, lang
│   └── shb-gui/                `shb-gui`: the window (egui / eframe)
└── tools/parity_check.py       the Rust app against the Python app, byte for byte
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
out-of-range HUD scales, stray remap entries and the 0.4.x block sizes), 6/6 install trees identical.

`zstandard` is stubbed when it is not installed, because the config patching does not use it.

## Not ported yet

These are reported as notes in the install result instead of being silently skipped:

- the enemy speed / aggression packs (`_patch_packs`: the zstd dictionary packs under `payload/romfs_opts`),
- the GameCube UI diagram (`gamecube.py`: patches the game's own UI atlas, needs the dump),
- the downloadable game-wide prompt layout and its resource-size-table merge (`layouts.py`),
- the Python app's relink / rekarina / longshot rebuild flow and the self-updater,
- the six-language UI (the window has English and Spanish).

`cargo test -p shb-core` covers the offset table, the remap rules, the presets, the IPS reader, the mods
folder detection and a full install into a temp folder (36 tests).
