# Agent brief: how this mod works

Condensed from `README.md`, `docs/GUIA-DESARROLLO.md`, `docs/LONGSHOT.md` and `docs/RE-NOTES.md`.
Read this before touching `core/` or `features/`; go to those files for the full detail and addresses.

Game: **Ocarina of Time remake**, Switch 2, update **v131072**, `main` Build ID
`E0F3CCFAF50065AE4EA01EB5AEB13B1A45DA3348`. Release **0.9.3**.

## 1. What the mod is

Two artifacts the emulator loads next to the game, plus an optional romfs:

| Piece | What it is | Built by |
|---|---|---|
| `exefs/subsdk9` | the code module: NSO, ARM64, **no relocations** | `tools/build_core.py` |
| `exefs/<BuildID>.ips` | IPS32 patch over `main`; each hook site jumps into the module | `tools/build_core.py` |
| `romfs/…` | modified game data (drops, fairy rewards, enemy packs, textures) | `tools/make_romfs_opts.py`, `make_enemy_opts.py`, … |

The app (`app/`, Tkinter) compiles nothing: it ships the prebuilt module in `app/payload/`, writes the user's
options into the config block inside `subsdk9`, and copies everything to `zz-Super-Hero-Boost`.

Change flow: `core/` or `features/` → `build_core.py` → emulator test → `make_release.py` → `build_app.py`.

## 2. Load layout and why the module is base-relative

`main` loads at `0x8043C000`; `subsdk0` follows it; our `subsdk9` loads at **`main + 0x4F80000`**, which is the
address the module is linked at (`core/link.ld`, `--base 0x4F80000`). All offsets in the IPS and in branch targets
are main-relative. The recompilers use the same order (`rtld, main, subsdk0…subsdk9, sdk`, page aligned), so
`subsdk9` lands at `main + 0x4F80000` there too and the branches work untouched.

Consequences that break builds when forgotten:

- No table in data may hold an address (it would be a link address). Names are `char[]` returned from code;
  handler lists are generated as **direct calls** (`build/native/dispatch_gen.c`).
- `switch` must not become a jump/lookup table → `-fno-jump-tables`.
- No tail calls → `-fno-optimize-sibling-calls`; ReLink's translator builds a function from everything reachable
  from its entry, and a tail call to a lower function merges the bodies, killing the caller's start.
- `-Oz`'s machine outliner must stay off (`-mllvm -enable-machine-outliner=never`): the outliner moved pointer
  guards into helpers that return on flags, and Longshot's recompiler drops those flags (the 0.4.0 crash).
- The NSO header needs the raw sizes at `0x60/0x64/0x68`; zero makes the loader read nothing (crash on load).

## 3. Hooks

All wrappers live in `core/hooks.S`; sites in `tools/hook_sites.py`; resume addresses as linker symbols in
`core/link.ld`.

| Type | Patch | Use |
|---|---|---|
| `INPUT_HOOK` | the game's `bl` to a controller reader becomes a `bl` to our wrapper, which calls the original and then `mod_on_input` | items, stamina, button mapping, quick select |
| `WRAP_HOOK` (slots) | `bl` to equip-item (`0x1211C00`) → pre-hook can rewrite the item index | buttonMapping |
| `CODE_HOOK` | the `pacibsp` entry becomes `b` to our wrapper; it saves registers, calls `mod_on_code(id, …)`, applies `regs` changes, runs `pacibsp`, returns to `site+4` | HUD, player tracking, songbook, shield |

Only **input site 3** (`0x346966C`) sees gameplay input (~60 calls/s, 16 samples); the other four never run in
boot or gameplay. In `regs`: `x0` at `+0x10`, `x30` (LR) at `+0x08`, `d0` at `+0xB0` — the sprint hook uses the LR
to multiply only the player's own call.

Important code hooks: `0x2B30DE0` (per-frame scheduler → the whole HUD), `0x43A6F8` (UI resource build → pane
injection), `0x2C215E0` (physics actor update → player/enemy tracking), `0x9352F0` (ground movement speed),
`0x2894340`/`0x28947F0` (ocarina enter/leave).

**Adding a code hook:** verify `pacibsp` with `python tools/disasm.py <dir> 0x10` → add `(site, name, id)` to
`CODE_SITES` → `RES_X = site + 4;` in `link.ld` → `CODE_HOOK hook_x, id, RES_X` in `hooks.S` → handle `id` in
`hud_on_code()` (declare feature logic `__attribute__((weak))`) → `build_core.py` re-checks the site before
writing the IPS.

## 4. Runtime configuration (`SHBCFG01`)

`ShbConfig` in `core/config.h` with magic `SHBCFG01` in `.data`; the app finds the magic in `subsdk9` and writes
fields at `magic + 16`. The mod reads it every frame (`g_shbcfg`). **Append fields, never reorder** (use
`reserved`). Offsets 16..75 cover stamina, difficulty, HUD layout, button mapping, quick select, manual shield,
target swap, L3 sprint, language — the table is in `docs/GUIA-DESARROLLO.md` §3.3.

**Adding an option** touches six places: `config.h` field, default in `config.c`, write in
`app/installer.patch_config`, field in `installer.Settings`, CLI flag in `app/main.py`, control + `cfg` entry in
`main.py`, and texts in `app/i18n.py` for the six languages (`en es pt fr de it`).

## 4.1 Time: a frame is not a millisecond

`core/clock.c` reads `CNTPCT_EL0` once per rendered frame (from `hud_frame`) and gives the features the real
frame duration: `shb_dt_frame()` for a duration in seconds, `shb_frames(n)` to turn a threshold written in 60 Hz
frames into the frames the game is actually running at (`shb_frames(18)` is 0.3 s at any rate; at 60 fps it
returns exactly 18). The rate comes from `CNTFRQ_EL0` — 19.2 MHz on the console, **31.25 MHz in Fritchii**, so a
hard-coded 19.2 makes every duration 63% off there — with 19.2 MHz as the fallback. If the counter is missing or
implausible, every helper returns the old 1/60-per-frame value and the module behaves as before.
`g_clock_dbg` = `{ok, last frame us, accumulated ms, cntfrq kHz, fps}`; read it live to see the rate.

## 5. Features (where the logic lives)

- **Stamina** — `features/stamina/`. Reads the player's **action state** (`player+0x244`: `Rolling`, `Jump`,
  `Cut…`, `Guard…`), not buttons. Capacity 75 + 75/17 per heart from the 4th (max 150). Exhaustion removes
  buttons from the input; recharge 0.8 s idle / 1.8 s combat / 2.5 s after exhaustion. Shield hits are counted by
  `player_guard_seq` across the hit-reaction pause (the player's update stops for a few frames).
- **Sprint** — speed multiplier at hook 10, only for the player's own call; `sprint_l3` injects A and centres the
  stick for two frames so the press lands as "hold A then move" and never rolls.
- **Enemy health / damage taken** — `core/player.c`; enemy = actor whose life resource is `Enemy_*`; life at
  `+0x1C10` (current) / `+0x1C18` (max), s32 at `+8`.
- **Songbook** — `features/songbook/`. Hooks ocarina enter/leave for the context, reads selected song `+0x22C`
  and learned flags `+0x264+idx`, then **injects native note buttons** (L/R/A/Y/X) through the input hook. The
  game's own matcher recognises the melody, so sound, animation and effect are stock. Learned songs only.
- **Manual shield** — `features/manual_shield/`. Reads the game's Targeting Type live from GameData
  (`0x834d9d9e`/`0xc713c7da` via `read_enum` at `main+0x17BF90`), keeps L reserved while guarding, cancels with
  R3. Guard uses the stock locomotion path: the feature only decides whether it is allowed.
- **Controls** — `features/controls/`: ZL↔R3 swap, L3 sprint. Runs **before** manual shield and stamina so they
  see the swapped buttons.
- **Button mapping** — `features/button_mapping/`: own B/L/R/ZR item buttons, per-age, per-save. The game keeps
  the item box (ZR) in `PlayerStatus.AssignableItem.Child/Adult`; assignments persist in the mod's own
  `slot_NN/ootmaster.bin` written when the game opens `progress.sav` (hook on `nn::fs::OpenFile`).
  Quantities come from `PouchData.<item>.StockNum` (Int table, live value `+0x1C`), allowlisted by stackable item.
- **Quick select** — `features/quick_select/`: D-pad down = Iron/Hover boots, up = Lens/mask. Writes the
  GameData `Equip*` String64 **value and copy** (`+0x74` and `+0xC4`) — value alone only changes the menu icon.
- **HUD** — `core/hud.c`: stamina ring (~230 picture panes) or the segmented bar under the magic gauge, clock,
  rupee counter, collectible counter, minimap hiding. See §6.
- **romfs options** — no heart drops / no magic jars (`DropGlobalParam.HpActor`/`MpActor` → green rupee, both at
  once = one combined file), Great Fairy reward order, Farore's Wind outdoors, enemy duplication/speed/animation.

## 6. HUD pane injection (the fragile part)

1. Hook `0x43A6F8` intercepts the `PlayerStatus_00` layout build and hands the engine a **private copy of its
   metadata** with extra panes `P_ShbPic_*` / `T_ShbTxt_*`; names carry their CityHash64 (`hud_names.h`).
2. Pane descriptors live in several tables (`+0x20` common, `+0x40` picture/text, `+0x60` pictures, `+0x80`
   texts). **In a picture's common descriptor `+0x40` must be `0xFFFF` and `+0x42` the index in the picture
   list** — the other way round the engine searches garbage lists (~2 min black load, hundreds of thousands of
   unmapped reads).
3. Textures: copy a material that has a texture (`P_SubItemBg_00`) and point its sampler at another name
   (`add_material`). Used: `NumBaseSide_00/01`, `Nt_RupeeIconNew_00`, `Circle64_00`, `White8_00_BC4L`.
4. Every frame `hud_frame()` writes each pane record (`+0x60/+0x64` scale, `+0x68` rotation, `+0x6C/+0x70`
   position, `|= 0x6F`) and the colour (pane object `+0x20`, RGBA float). Text via `set_text` (`main+0x439520`),
   written to the hidden twin pane to avoid flicker.
5. **Only ONE appended part per layout is ever registered** in the child list; appended parts 2..n get a NULL
   child frame. `PART_N = 1` is the stable setting and the ring is built from panes, not parts. Repurposing
   stock filler parts crashed on load; that code remains for reference only.
6. Outside gameplay everything is hidden once; a layout rebuild clears the caches (`plates_reset`).

Collectibles: Gold Skulltula flags are runtime-only; the durable record is the save entry `Game` (Bool64bitKey,
hash `0xA3DB7114`, ascending actor hashes + 0), read from the save buffer at manager `+0xE80` **and copied out**:
the game loads and writes every save file through that buffer, so it holds `progress.sav` only briefly.
Heart pieces: world pieces (`Obj_Heart_Piece`, `Obj_GerudoValley_Heart_Piece`) by actor hash in that key set,
rewards by their `HeartPieceAnchor_GameDataPath` Bool.

**GameData record indices are not stable.** A romfs mod with its own `GameDataList` (Paraglider: +3 Bool,
+1 Int) shifts them. Look keys up by hash with `gd_find(table, hash)` (core/hud.c, the game's own map);
never hard-code an index generated from the stock list for Bool or Int.

## 7. Recompilations

- **Longshot / Hookshot / ReLink** ignore `exefs` (the code is translated ahead of time). Romfs files work from
  `mods/zz-Super-Hero-Boost/romfs`; code needs a rebuild with the module as `subsdk9` in `.longshot_exefs` /
  `.relink_exefs` and the 26 hook sites patched into `main` (text starts at file `0x104` instead of `0x100`).
- Config lives in `image.bin`, so options change **without a rebuild** once the game is built
  (`install_longshot.py --config-only`).
- The installer snapshots everything it touches into `<game>/.shb-backup/` and restores it on uninstall; the
  builders' shader/pipeline caches are marked as regenerated and deleted, not restored.
- **Rekarina** (0.9.0): its launcher rejects code modules; the app adds `subsdk9` to the lift, compiles with
  Rekarina's own `gen.py`/`build_game.py`, converts the IPS with `tools/exefs_patches.py`, and links the module
  at `main+0x5200000` with `--rekarina` (synthetic `eh_frame_hdr`, minimal dynsym).
- Load order: the mod folder must sort **last** (`zz-…`) so its resource size table wins.

## 8. Testing

- `tools/live_check.py` launches `lichii.exe` with `EDEN_NX2_DEBUG_SERVER=127.0.0.1:6545`, waits for the title,
  presses **A four times ~6 s apart**, waits for the save, screenshots, and reads the module symbols from
  `build/native/module.elf`. Install first, then rebuild with the same variant so the ELF matches.
- `tools/hud_tune.py` tunes `g_hudp[]`/`g_ringp[]` and the HUD options live; copy the values into the
  initialisers when they look right.
- `tools/e2e_matrix.py` installs every profile with the app binary (no window), plays, and checks that stamina
  behaves as each profile promises (`build/e2e/results.txt`).
- Watch `user/log/eden_log.txt`: many `Unmapped Read/Write` lines mean a descriptor/pointer bug.

## 9. Documents

| File | Content |
|---|---|
| `docs/GUIA-DESARROLLO.md` | architecture, hooks, config table, HUD, build, tools |
| `docs/RE-NOTES.md` | addresses, formats, dead ends (read before re-investigating anything) |
| `docs/APP.md` | app, profiles, options |
| `docs/BUTTON-MAPPING.md` | button mapping design |
| `docs/LONGSHOT.md` | installing into the static recompilations |
| `docs/AUDIT.md`, `docs/CI.md` | third-party material audit, GitHub Actions builds |
