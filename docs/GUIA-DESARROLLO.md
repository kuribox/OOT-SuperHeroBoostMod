# Super Hero Boost: development guide

How the mod is built, compiled, installed and tested, and how to change each part. Game: console remake, update
v131072, `main` Build ID `E0F3CCFAF50065AE4EA01EB5AEB13B1A45DA3348`.
The test emulator loads mods from its `LoadDir` (configured with `load_directory` in `qt-config.ini`; the tools accept
the folder with `--emu` or the `SHB_EMU` environment variable).

Related documents:
- [RE-NOTES.md](RE-NOTES.md): reverse-engineering findings (addresses, formats, what worked and what did not).
- [APP.md](APP.md): the configuration app.
- [BUTTON-MAPPING.md](BUTTON-MAPPING.md): button mapping design (buttonMapping).
- [LONGSHOT.md](LONGSHOT.md): installing in Hookshot/Longshot (static recompilations).

---

## 1. Overview

The mod is two pieces loaded next to the game by the emulator:

| Piece | What it is | Where it is generated |
|---|---|---|
| `exefs/subsdk9` | Our code module (NSO, ARM64, no relocations) | `tools/build_core.py` |
| `exefs/<BuildID>.ips` | IPS32 patch over `main`: each hook site jumps into our module | `tools/build_core.py` |
| `romfs/...` (optional) | Modified game data (for example "no heart drops") | `tools/make_romfs_opts.py` |

The **app** (`release/SHB-OcarinaTweaks` on Windows/macOS/Linux) compiles nothing: it carries the prebuilt module,
writes the user's options into a configuration block inside `subsdk9` and copies everything into the mod folder
(`zz-Super-Hero-Boost`).

Change flow:

```
edit core/ or features/  ->  build_core.py (test)  ->  test in the emulator  ->  make_release.py  ->  build_app.py
```

## 2. Repository layout

```
core/                common module code
  module.S           MOD0 header and minimal dynamic section
  link.ld            linking: module base and game function addresses (main-relative)
  hooks.S            hook wrappers (controller input, slots, code hooks)
  dispatch.c/.h      dispatches input events to the features (generated table, no pointers)
  hid.c/.h           reads/writes buttons and sticks in the HID samples
  config.c/.h        SHBCFG01 configuration block (patched by the app)
  player.c           player and enemy tracking: action state, life, combat, life scaling, shield
  hud.c              the whole custom HUD: stamina ring, clock, rupee counter
  mem.c, slots.c ... utilities
features/
  stamina/           stamina (costs, exhaustion, regeneration), sprint multiplier
  songbook/          song selected in the native songbook -> native notes
  button_mapping/    buttonMapping: own B/L/R/ZR item buttons (see BUTTON-MAPPING.md)
  probe/             diagnostic feature
tools/               build, release, live tests and analysis (see section 9)
app/                 configuration app (Tkinter) + prebuilt payload
  main.py            window, navigation and --cli mode
  presets.py         complete game profiles
  installer.py       installation and configuration block patching
  paths.py           portable / per-user paths (Windows, macOS, Linux)
  i18n.py            texts in six languages
docs/                this guide and the notes
build/               outputs and screenshots (ignored by git)
release/             app binary (ignored by git)
```

External paths used by the tools (on this PC, all configurable through environment variables):
- LLVM toolchain: `F:\project\oot\tools\llvm-mingw\llvm-mingw-20261104-ucrt-x86_64\bin` (`SHB_LLVM`; on Linux, a
  folder with `clang`, `ld.lld`, `llvm-objcopy`, `llvm-size` and `llvm-nm`).
- Original `main` (read-only, to verify hooks and disassemble): `F:\project\oot\data\stock-exefs\main`
  (`SHB_STOCK_MAIN`).
- romfs dump: your dump folder (`SHB_ROMFS`).
- Debug server client and BYML/GameData readers (development only): `SHB_DEVTOOL`.
- zstd dictionary of the UI archive: `F:\project\oot\data\zsdic1.bin`

## 3. How the module works

### 3.1 Constraints
- **No relocations**: no table in data may hold addresses (they would be link addresses, not real ones). Names are
  stored as `char[]` and returned from code; handler lists are generated as direct calls
  (`build/native/dispatch_gen.c`). Compiled with `-fno-jump-tables` because clang turned `switch` into absolute
  pointer tables.
- Game functions and data are referenced as symbols in `core/link.ld` (offsets inside `main`). With the module base
  (`__base`) the linker produces correct PC-relative branches.
- Module base: `0x4F80000` (the "standalone" variant). `make_release.py` builds it.

### 3.2 Hook types (all in `core/hooks.S`, sites in `tools/hook_sites.py`)
| Type | How it is patched | Use |
|---|---|---|
| `INPUT_HOOK` | the game's `bl` to a controller reader becomes a `bl` to our wrapper, which calls the original reader and then `mod_on_input` | controller input (item buttons, stamina) |
| `WRAP_HOOK` (slots) | `bl 0x1211C00` (equip item) → our wrapper with a pre-hook that can change the item | buttonMapping |
| `CODE_HOOK` | the `pacibsp` instruction at a function entry becomes a `b` to our wrapper; it saves registers, calls `mod_on_code(id, x0, x1, x2, regs)`, restores them (changes in `regs` are applied), runs `pacibsp` and returns to `site+4` | most hooks |

Current code hooks (`CODE_SITES`):

| id | site (main) | what it is | who uses it |
|---|---|---|---|
| 0 | 0x2B30DE0 | per-frame scheduler update | `hud_frame()` (the whole HUD) |
| 1 | 0x33BDCE0 | magic-gauge controller init | HUD anchor (`g_mp`) |
| 2 | 0x43A6F8 | UI resource build | `hud_prepare()`: injects panes into PlayerStatus_00 |
| 3 | 0x2D2D90 | `set_frame(anim, frame)` | animation observation (diagnostic) |
| 4 | 0x152F8F0 | layout part instance creation | diagnostic |
| 5 | 0x2C215E0 | physics-actor movement update (x0 = actor) | `player_track()`: player and enemies |
| 6-9 | 0x27F7D00, 0x27F7F90, 0x27FB0C0, 0x27FA200 | player shield reaction nodes | stamina cost when blocking |
| 10 | 0x9352F0 | ground movement (x0 player, x1 direction, s0 speed) | sprint multiplier |
| 13 | 0x2894340 | ocarina mode enter (x0 = ocarina/songbook context) | `songbook_on_ocarina(1, ctx)` |
| 14 | 0x28947F0 | ocarina mode leave | `songbook_on_ocarina(0, ctx)` |

In the callback, `regs` points to the saved registers: `x0` at `+0x10`, `x30` (return address) at `+0x08`, `d0` at
`+0xB0`. Example: the sprint checks `x30 == main+0x934994` (the player's own call only) and multiplies `s0`.

**Adding a new code hook:**
1. Check with `python tools/disasm.py <dir> 0x10` that the function starts with `pacibsp` (0xD503237F).
2. `tools/hook_sites.py`: add `(site, "hook_x", id)` to `CODE_SITES`.
3. `core/link.ld`: `RES_X = site + 4;`
4. `core/hooks.S`: `CODE_HOOK hook_x, id, RES_X`
5. `core/hud.c`, `hud_on_code()`: handle `id`. If the logic lives in a feature, declare it `__attribute__((weak))`.
6. `build_core.py` verifies that the site has the expected instruction before generating the IPS.

### 3.3 Runtime configuration (`core/config.h`)
`ShbConfig` block with magic `SHBCFG01` in `.data`. The app finds the magic in `subsdk9` and writes the fields; the mod
reads them every frame (`g_shbcfg`). **Fields are only appended at the end** (use `reserved`), never reordered.

| offset | field | app (`installer.patch_config`) |
|---|---|---|
| 16 | `stamina_enabled` | Stamina checkbox |
| 17 | `preset` (0 hardcore, 1 combat only, 2 sprint only, 3 advanced) | Preset |
| 18-23 | `cost_jump/attack/sprint/block/shoot/roll` | What costs stamina |
| 24 | `combat_only` | Combat only |
| 25 | reserved (legacy) | — |
| 26 | `enemy_hp_pct` (u16) | Enemy health |
| 28 | `damage_taken_pct` (u16) | Damage taken |
| 30-32 | `hud_rupees`, `hud_clock`, `clock_24h` | Always-visible HUD (in the app the clock is a selector: off / 12 h / 24 h) |
| 33 | `no_heart_drop` (informational; the romfs does it) | No heart drops |
| 34 | `sprint_speed_pct` (u16, default 125) | Sprint speed |
| 36 | `songbook_enabled` | Song selected in the songbook -> the hero plays it |
| 37 | `button_mapping` | Item buttons (button mapping) |
| 38 | `button_style` (0 Switch, 1 Xbox, 2 PlayStation) | Button style |
| 39 | `bm_simple` | Simple layout (assignable buttons only) |
| 40 | `bm_l_mode` (0 item, 2 the game's item selector) | What the L button does |
| 41 | `quick_boots` | Boots quick select |
| 42 | `quick_mask` | Mask quick select |
| 43 | `hud_collectibles` | Per-area collectible counter |
| 44-51 | `hud_x[4]` (s16: rupee, clock, skull, heart; -32768 = off) | HUD block placement |
| 52 | `stamina_bar` (0 ring, 1 segmented bar) | Stamina display |
| 53 | `hud_flags` (1 = the layout fields below are valid) | HUD layout version |
| 54 | `hud_mode` (0 wide, 1 simple, 2 map; informational) | HUD layout chosen in the app |
| 55-60 | `hud_sx`, `hud_sy`, `hud_tscale`, `hud_iscale`, `hud_plates`, `hud_map_hearts` | Plate/text/icon scales, plate mode (0 none, 1 all, 2 one bar), heart counter on the Minus map |
| 62-63 | `hud_y` (s16) | HUD row y |
| 64-65 | `hud_ioff` (s16) | Rupee icon x offset |
| 66-67 | `hud_tx` (s16) | Text x offset |
| 68-69 | `hud_ioff2` (s16) | Clock/collectible icon x offset |
| 70 | `manual_shield` | Manual shield checkbox |
| 71 | `target_r3` | Swap ZL/R3 (target on R3; Toggle targeting recommended) |
| 72 | `sprint_l3` | L3 (left stick click) sprint while pressed (injects A, roll suppressed) |
| 73-75 | padding (struct alignment) | — |

**Adding a new option:** field in `config.h` (taking bytes from `reserved`), default value in `config.c`, write in
`installer.patch_config` (`f = magic + 16`), field in `installer.Settings`, flag in `main.cli`, control in `main.py`
(variable, saved in `cfg`, `Settings(...)`) and texts in `app/i18n.py` for the 6 languages (`en es pt fr de it`).

## 4. Features

### 4.1 Stamina (`features/stamina/stamina_hooks.c`, `stamina.c`)
- Runs inside the gameplay controller read (site 3). Costs come from the player's **actions** (`player_track` reads
  his state at `player+0x244`: `Rolling`, `Jump`, `Cut...`, `Guard...`), not from the buttons.
- Capacity: 75 + 75/17 per heart from the 4th onward (up to 150).
- Sprint = A held in the `Move` state for 12 frames: costs 7/s.
- Exhausted (yellow): actions are blocked according to the preset (buttons are removed from the input) and the player
  walks.
- Recharge after 0.8 s without actions (1.8 s in combat, 2.5 s after exhaustion), at 24/s (48/s after exhaustion).
- Shield blocking: hooks 6-9 and the `Guard*Hit/Break/Reaction/Repel` states (not `GuardFailHit`) raise a counter
  (`player_guard_seq`) that survives short pauses (the hit arrives during the player's reaction, when his update
  stops for a few frames).
- Combat: a living enemy closer than 10 m that approaches or is on top of the player (5 s grace).
- Constants: `#define` at the top of `stamina_hooks.c` and `stamina_default_config()`.

### 4.2 Sprint multiplier (hook 10, `stamina_speed_hook`)
Multiplies the player's ground movement speed while A is held and stamina has not removed it. Rolling, swimming and
climbing do not go through that call. The implementation is original: it only reads the return address and multiplies
the speed argument.

### 4.3 Enemy health and damage taken (`core/player.c`)
- Enemy = actor whose life resource is named `Enemy_*`. When first seen, its maximum and current life are scaled
  (`life+0x1C10` current, `+0x1C18` maximum, s32 value at `+8`). Diagnostic: `g_hp_log`.
- Damage taken: when the player's life drops, the extra share is subtracted (the extra share alone never kills).
- Life values for each enemy: `Pack/Actor/<Enemy>.pack.zs` → `Life/LifeParameters/*.bgyml` (`MaxLife`).

### 4.4 Songbook (`features/songbook/songbook.c`)
- Hooks the ocarina mode enter/leave (ids 13 and 14) to store the context (`x0`).
- On the gameplay input hook (site 3) it reads `ctx+0x22C` (selected song) and `ctx+0x264+idx` (learned). If the
  selection changes, it waits 20 frames and emits that song's note sequence.
- It does not call the internal playback functions: it injects the native note buttons (L, R, A, Y, X) into the HID
  samples. The game recognises the melody and plays the sound and animation on its own.
- Learned songs only. Song 12 (Scarecrow, composed by the player) is not played.
- Sequences derived from `GameParameter/MelodyScoreBook/Default.game__ocarina__MelodyScoreBook.bgyml` (12 entries).

### 4.5 Manual shield (`features/manual_shield/`)
- Optional (`manual_shield`); with it off every hook is pass-through and the stock controls stay.
- **Targeting Type:** `refresh_target_mode()` reads the base-game option live from GameData (struct
  `0x834d9d9e`, member `0xc713c7da`, via `read_enum` at `main+0x17BF90`, manager at `main+0x4E842A8`). Values:
  `0x60fc6939` Hold Target, `0x212dc0e7` Toggle Target. It refreshes every 10 scheduler frames.
- Input (site 3):
  - **Hold Target:** physical ZL stays native (the game itself holds the target); the shield request only applies
    while ZL/`LockOn` is active.
  - **Toggle Target:** physical ZL is consumed, a rising edge latches targeting, and native ZL is synthesised while the
    latch is on. The shield request only applies while targeting.
  - **R3** cancels targeting in both modes (unless the target-on-R3 swap is enabled).
  - While the manual shield is enabled, L is always consumed by the input filter, including during shield-hit
    reactions (`Guard*`, `Repelled`, `GuardBreak`); otherwise the default L/item function could fire mid-hit. The
    buttonMapping L slot is disabled too (the assignment is kept); turning the manual shield off restores it.
- Frame (code hook 0): `manual_shield_on_frame()` updates the ZL gesture/latch once per scheduler frame (repeated HID
  reads cannot repeat it), caches the player state and AS interface and keeps the native guard flags and AS Bool14 off
  unless L is held with the target camera active. On guard entry it requests the shield draw only (not the sword).
- The guard uses the stock locomotion/animation path: the feature only decides whether the guard is allowed. Busy
  actions (carrying, bow, hookshot, boomerang, horse aim) and a dead/inactive player keep it off. Menus keep their
  normal L/ZL navigation because the input filter only runs during gameplay.
- Three extra mid-function sites in `tools/hook_sites.py` (`EXTRA_SITES`, only patched when the feature is built):
  the AS Bool setter (`main+0x37BA100`) and the two target-guard flag stores (`main+0x972E18`, `main+0x97320C`). Their
  wrappers are in `manual_shield.S`; the resume addresses are linker symbols in `core/link.ld`.
- ZL follows the game's Hold/Toggle option and the L guard is gated with the target camera.

### 4.5.1 Controls (`features/controls/`)
- Optional input remaps; it runs **before** `manual_shield` and `stamina` (the feature list order is
  `controls,manual_shield,stamina,...`) so those see the swapped buttons and the synthetic A.
- `target_r3`: exchanges the physical ZL and R3 bits in gameplay only. R3 then targets; the app recommends setting
  System > Options > Targeting Type to **Toggle** (press R3 to start/stop targeting).
  Menus and loading screens keep the native buttons.
- `sprint_l3`: the stamina feature reads L3 (`0x10`) and injects A while it is held, so the game plays its own
  sprint (animation included). When the A press arrives while the stick is already pushed, the stick is centred for
  two frames, which is the "hold A, then move" case: Link sprints without rolling. The toggle also ends by itself:
  `L3_IDLE_FRAMES` (12) rendered frames with the stick at rest, but only after Link has moved since the toggle (a
  toggle pressed standing still waits for the first step). No A is injected, and a toggled sprint is dropped, while
  Link carries or lifts something (`g_ms_carrying`, from the manual shield frame hook): the injected A threw the
  jar, and again on every pickup while the toggle stayed on. It charges the sprint cost and
  uses the same speed multiplier; gameplay only.
- Config bytes 71 and 72 (see the runtime table above); `installer.patch_config` writes them at `f+55`/`f+56` when
  the module config size is at least 73.

### 4.6 No heart drops (romfs)
`tools/make_romfs_opts.py` generates, from the game's own dump (`SHB_ROMFS`, your dump folder),
`GameParameter/DropGlobalParam/...` with `DropGlobalParam.HpActor` (the actor that drops as a "heart") pointing to the
green rupee. It is only needed at build time: the payload/release already includes the generated file, so the end user
does not need the dump. The app copies that romfs if the option is enabled; if it is missing, it installs the rest and
shows a note without blocking.

## 5. Custom HUD (`core/hud.c`)

### 5.1 How panes are added to the game's HUD
1. Hook 2 (`hud_prepare`) intercepts the build of the `PlayerStatus_00` layout and gives the engine a **private copy of
   its metadata** (`build_root`) with extra panes: pictures `P_ShbPic_000..` and texts `T_ShbTxt_00..05`. Each name
   carries its CityHash64 (generated by `build_core.py` in `hud_names.h`).
2. Each pane is described in several layout tables (common list `+0x20`, picture/text `+0x40`, pictures `+0x60`,
   texts `+0x80`). Format details are in RE-NOTES ("Layout file format", "Rupee counter / clock HUD").
   **Important:** in a picture's common descriptor, `+0x40 = 0xFFFF` and `+0x42 = index in the picture list`. The other
   way round, the engine searches garbage lists on every query (hundreds of thousands of invalid reads and a black
   load of ~2 minutes).
3. **Textures:** each texture uses a copy of a material with a texture (the one from `P_SubItemBg_00`) whose sampler
   points to another texture name in the `Common` archive (`add_material`). Textures used: `NumBaseSide_00/01`
   (plate), `Nt_RupeeIconNew_00` (rupee), `Circle64_00` (circles), `White8_00_BC4L` (rays).
4. Every frame, `hud_frame()` places the panes by writing their record (`+0x60/+0x64` scale, `+0x68` rotation,
   `+0x6C/+0x70` position, set `|= 0x6F`) and their colour (pane object `+0x20`, RGBA float).
5. Text: the game's `set_text` (`main+0x439520`). Each field has two panes and is written to the hidden one to avoid
   flicker.
6. Outside gameplay (menus, loading) everything is hidden once and left alone; when the layout is rebuilt the caches
   are cleared (`plates_reset`).

### 5.2 Clock and rupees
- Native rupee-counter plate, drawn twice (mirrored) to have chevrons on both sides.
- Time: `World_CurrentDateTime` (GameData) or the world clock; sun from 6:00 to 18:00, moon (half moon) the rest.
- Rupees: GameData Int `PlayerStatus.CurrentRupee` (record 962) and wallet capacity (976).
- Adjustable look: `g_hudp[]` (indices documented in `hud.c`).

### 5.3 Stamina ring and bar
Ring: about 230 overlapping `Circle64_00` panes (48 on the inner ring, 64 on the outer one) that form a continuous
tube, with a black track behind; the last dot sits at the exact fill angle. It follows the player by projecting his
position with the camera matrix (`anchor_update`). Look: `g_ringp[]`.

Souls-style bar (`stamina_bar` = 1): aligned with the magic gauge (768 x 32 layout units, left edge under it) and
split into the same **16 segments** as the magic gauge. It is drawn as flat segments over a dark strip; no texture is
scaled, so draining removes whole segments and only the boundary one shrinks. The look is fixed: yellow normally, red
while recharging from empty. Capacity grows with the hearts (75 at 3 hearts, 150 at 20), so the bar grows to the right
from half to full width and the segment count goes from 8 to 16; the left edge stays under the magic gauge. The game's
D-pad guide cross always moves down 60 units while the bar is shown so it does not touch it. Position and size live in
`g_barp[]` (centre x, top y, width, height) for live tuning only.

### 5.4 Tuning the look live
With the game open through `live_check.py --keep` (or `run`), without recompiling:
```
python tools/hud_tune.py get
python tools/hud_tune.py set 0=-1518 3=0.46 --shot build/tune.png --crop 0,600,400,720
python tools/hud_tune.py set 6=0.0 --sym g_ringp
python tools/hud_tune.py cfg rupees=1 clock=1 clock24=0
```
When the look is right, copy the values into the `g_hudp` / `g_ringp` initialisers.

## 6. Build, release and the app

```
python tools/build_core.py --features controls,manual_shield,stamina,songbook,button_mapping --slots --base 0x4F80000   # test module in build/native
python tools/make_release.py     # builds the module in app/payload + button textures + optional romfs
python tools/build_app.py        # native app binary (PyInstaller). Close the app first.
```

`build_app.py` works on Windows, macOS and Linux, but PyInstaller does not cross-compile: run it on each target OS.
The app uses Tkinter and portable paths (`app/paths.py`); it needs no administrator permissions.

The app (`app/main.py` UI, `app/presets.py` profiles, `app/installer.py` logic, `app/i18n.py` texts):
1. finds the emulator's mods folder (honours `load_directory`);
2. copies the standalone variant and the textures of the chosen button style, and patches the configuration block;
3. warns about other exefs mods that may conflict.

It also has a command-line mode (the same install code):
```
python app/main.py --cli --emu "EMULATOR_PATH" --profile hero-stamina --preset 0 --rupees --clock --sprint-speed 125
```

## 7. Testing in the emulator

`tools/live_check.py` starts the test emulator with the debug server (`EDEN_NX2_DEBUG_SERVER=127.0.0.1:6545`), waits
for the title screen, presses A 4 times, waits for the save to load (~30 s), takes screenshots in `build/` and prints
the module diagnostics (symbols read from `build/native/module.elf`).

```
python tools/live_check.py --load-a 4 --keep --linger 3     # leaves the game open
```

Key points:
- Install first (app or `--cli`) and **then** re-run `build_core.py` with the same variant, so `module.elf` (the
  symbols) matches what is installed.
- The debug server serves one request at a time: while an input (`input_sequence`) is running, reads wait. To measure
  during an input, read the process memory with the devtool's `HostMemory` (as `hud_tune.py` and the speed scripts do).
- Writing memory live: `HostMemory(pid).calibrate(client, [main_base])` and then `write(address, bytes)`.
- The emulator log (`user/log/eden_log.txt`) should stay clean: many `Unmapped Read/Write` lines mean a descriptor or
  pointer bug and slow the game down heavily.
- Conflicting third-party mods (HUD counters, other stamina system versions, other button mappings...) must be
  disabled in the emulator.

## 8. Investigating the game (recipe)

| I need | How |
|---|---|
| See the code at an address | `python tools/disasm.py 0x9352f0 0x40` |
| Who calls a function | look for `bl` to that address in the `main` text (see RE-NOTES scripts) |
| Actor parameters | `Pack/Actor/<Actor>.pack.zs` (zstd with `pack.zsdic` from `Pack/ZsDic.pack.zs`, SARC, BYML) |
| RSDB tables (items, weapons) | `RSDB/*.rstbl.byml.zs` (zstd with `zs.zsdic`) |
| UI layouts | `UI/Layout2Archive/Common...blarc.zs` (SARC of `.buil`, BFFH format in RE-NOTES) |
| UI textures | `Common...bntx.zs`: `tools/bntx_probe.py` lists, `tools/bntx_decode.py` decodes BC4 (block height 4 for 128x128) |
| A value in memory | `DebugClient.search(...)` + read neighbours; bisect by writing values with `HostMemory` |
| Player state | `player+0x244` (action name), `actor+0x31C` position (effectively read-only), `actor+0x388` velocity (output) |

Function names are not in `main` in plain text (the engine uses hashes); clues come from the data (AINB, BYML) and
from addresses already used by other mods.

## 9. Tools (`tools/`)

| Tool | Purpose |
|---|---|
| `build_core.py` | compiles the module and its IPS; generates `hud_names.h` and `dispatch_gen.c` |
| `make_release.py` / `build_app.py` | app payload / executable |
| `make_romfs_opts.py` | optional romfs (no heart drops) |
| `make_enemy_opts.py` | beta: generates the enemy duplication, the movement/attack pack base + manifest and the eleven animation folders from the dump |
| `features/songbook` | native songbook song -> injected notes |
| `check_no_game_data.py` | checks that git has no dump files or generated payloads |
| `hook_sites.py` | list of hook sites |
| `live_check.py` | start, load save, capture and read diagnostics |
| `hud_tune.py` | tune the HUD and options live, screenshots |
| `disasm.py` | disassemble `main` |
| `bntx_probe.py`, `bntx_decode.py` | UI textures |
| `pane_dump.py`, `gauge_dump.py`, `text_probe.py`, `clock_probe.py` | live HUD dumps |
| `enemy_probe.py`, `find_player.py`, `player_strings.py`, `action_names.py` | actors and player states |
| `e2e_matrix.py` | full test: installs each configuration with the binary (no window), starts the game, performs actions and checks that stamina behaves as that configuration promises (`build/e2e/results.txt`) |
| `make_restbl.py` | resource size table (UI memory budget) |
| `lib/` | `nso.py` (NSO/IPS), `sarc.py`, `cityhash.py` (own implementation of the public algorithm), `winmove.py` |

## 10. Licences and references
- The debug server client, `HostMemory` and the BYML/GameData readers are development tools and are not distributed
  with the mod.
- The rest of the code (module, HUD, stamina, button mapping, app and installer) is original. See [AUDIT.md](AUDIT.md).
