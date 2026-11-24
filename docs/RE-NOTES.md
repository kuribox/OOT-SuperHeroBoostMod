# RE notes (super-hero-boost)

Build under test: title `0400486024DEC000`, update 131072, main build id
`E0F3CCFAF50065AE4EA01EB5AEB13B1A45DA3348`. The test emulator loads mods from
its configured `LoadDir` (see the `load_directory` entry in `qt-config.ini`).

## Load layout (confirmed from logs)

- main loads at `0x8043C000` (size `0x4EA0000`) in the last run.
- subsdk0 follows main at `main + 0x4EA0000`.
- subsdk9 (ours) loads at `main + 0x4F80000`, right after subsdk0.
- Our module is linked at `0x4F80000` (main-relative), so offsets in the IPS and
  in `bl`/`b` targets are main-relative. This matches the emulator.

## Static recompilations (Hookshot and Longshot)

- They are not emulators: `bin/longshot.exe` is the game built as native code, and `image.bin` holds the guest image.
  `host/mods.c` ignores `exefs` patches by design and only reads `mods/<name>/romfs`.
- The builder's module order is `rtld, main, subsdk0..subsdk9, sdk`, page aligned. Adding our `subsdk9` to
  `.longshot_exefs` places it at `main + 0x4F80000` (0x84FA0000 with main at 0x80020000), which is the module's link
  base.
- Verified with the builder's lift: subsdk9 72 functions, 100% translated, 0 untranslated; the image contains the
  config block at `0x84FB2010`. `hos_image.c` leaves subsdk9 out of the DT_INIT list (init 0), which is fine: the
  hooks enter its code directly.
- Installing means rebuilding (`nx2/make_game.py --update`), not copying files. See [LONGSHOT.md](LONGSHOT.md).
- HUD crash on a map load (HyruleField_Child): `hud_frame → plates_update → name_index` dereferenced a name pointer
  (`0x6a4cff8b47b`) from a game layout that was being rebuilt (guest LR `0x84FA6D24`). Fixed by range-checking the
  name-table entries in `name_index`/`pane_record`. (An extra HUD-init gate was tried and removed: it could leave the
  HUD off when the two init hooks ran in the other order; the pointer checks alone fix the crash without touching the
  normal drawing path.)
- The Longshot save-start crash was a **stale save-descriptor slot**: `kset_refresh()` walks the GameData manager's
  8 save descriptors at `mgr+0xE00 + i*0x80` and read `magic` from each non-null pointer. In Longshot slot 5 held
  `0x6a4cff8b47b` (7.3 TB) and the dereference faulted. The earlier backtrace mapping was wrong: the top guest LR
  `0x84FA6940` is the return address after `bl hud_frame` in `hud_on_code`, not the pause-menu init. The crash is now
  fixed by range-checking every GameData pointer (`mgr`, save slots, table records, String64 values) with the 6 TB
  `vptr` before dereferencing; the emulator's heap is below 1 TB and is unaffected.
- The valid-pointer range must cover **Longshot's** heap and stack (4.4-4.8 TB); the emulator's is below 1 TB. The first
  1 TB limit made the layout repair reject valid names, and the id-4 diagnostic compared the bad 7.3 TB name. 6 TB
  accepts the valid pointers and rejects `0x6a4cff8b47b`; `str_eq` and `hud_on_settext` validate their pointers too.
  Keeping the module text at `0x10000` made the following builds recompile **14 of 479** chunks (~5 min).
- The stale layout also reaches the **game's** `find_part`/`ui_find_sub`, which do not check the names. The module
  repairs invalid name slots (replacing them with a valid empty name) before those calls and waits two frames after
  a HUD init; `ui_find_sub_safe`/`icon_list_ok` repair both `part+0x80` and the owner layout. These checks are
  defensive (they did not cause the crash above) and stay for the emulator and Longshot.
- The module text grew to `0x10000` when the first guards were added, shifting the `sdk` by one page: that build
  recompiled 71 of 479 chunks. Later code changes that keep the same text size only recompile the module chunks.

## NSO for subsdk9 (fixed in F0)

- The loader needs the compressed sizes at `0x60/0x64/0x68` set to the raw
  sizes. Zero makes it read nothing for the segments (crash on load).
- No hash at `0xA0` (flags == 0). Module id at `0x40`.
- MOD0 offsets must come from real section addresses (`core/link.ld`), and data
  must be page-padded so bss starts where MOD0 says.

## Input hook sites

Five stock sites read input (`INPUT_SITES` in tools/hook_sites.py). Each calls one of five
import stubs in main:

| site | stub | status |
|---|---|---|
| 0x34694F8 | 0x3C427F0 | hooked, **never runs** in boot or gameplay |
| 0x3469524 | 0x3C42810 | hooked, never runs |
| 0x3469550 | 0x3C42820 | hooked, never runs |
| 0x346966C | 0x3C42800 | hooked, **the gameplay input path** (~60 calls/s, 16 samples, B/A bits seen) |
| 0x3469698 | 0x3C42830 | hooked, never runs |

Each stub is `adrp x16 / ldr x17,[x16,#off] / add x16 / br x17`, a jump through a
GOT slot. Slots 0,1,2,4 still point at PLT0 (main+0x3C41C50, lazy binding) until
first use; slot 3 is already bound to a function in `sdk`.

### Root cause of the old "site 3 crash" (solved)

`bl STOCKn` with `.equ STOCKn, <const>` assembles to a constant offset, not a
PC-relative branch, so the wrapper jumped to `wrapper_pc + 0x3C42800`
(= subsdk9 base + constant). This was never a GOT problem. Fix: STOCK0..4,
SLOTFN, PLT0 and GOT0..4 are linker symbols in `core/link.ld`, so the
relocation is PC-relative. The first call through a still-lazy slot is
tail-called with the game's own LR (hooks.S) in case the dynamic linker finds the
caller module from LR; this part is a precaution and has not been shown to be needed.

Consequence: only site 3 sees gameplay input. The four item-button slot sites
(`bl 0x1211C00`) also never ran in the live check, so F3 needed its own RE.
## stamina (F2) status

- Stamina state machine in `features/stamina/stamina.c` (moved from `core/`).
  Values are **placeholders** written as clean-room defaults; they have not been
  balance-tested against the final gameplay tuning yet.
- Handler `features/stamina/stamina_hooks.c`: site 0 counts combat frames, charges
  the slash cost on B press, and when stamina is empty removes B/ZR from the input
  buffer at every hooked site.
- Capacity uses a fixed `LIFE_QUARTERS = 20` (5 hearts). Reading the real heart
  count from game memory is still open.
- Verified only at boot level: frame 3000 at ~76 s, no backtrace. The attack
  block itself is **not** verified in game yet (no HUD, no input playback).

## Probe result (F1)

- With sites 0, 1, 2, 4 hooked, the game runs at ~60 fps (frame 3000 at ~72 s).
- Two HLE service asserts appear in the log (`nx2:gsv 903`, `prepo:u 10500`).
  They are emulator service stubs, not from our hooks; the game keeps running.


## HUD and item buttons (F3/F4) findings

- HUD part registration: hook main+0x43A6F8 (UI resource build, x0 handle, x1 config).
  We give PlayerStatus_00 a private metadata copy with one extra part
  `Pa_ShbStamina_00` (clone of the stock gauge descriptors; CityHash64 of the
  name in tools/lib/cityhash.py, checked against all 40 stock names) and a private
  copy of the gauge file with a flat white fill tinted yellow. Frame drive from
  main+0x2B30DE0 (scheduler vtable main+0x4B05318). Verified live: bar renders under
  the hearts and tracks stamina. The magic gauge is not used.
- HID button bits are the standard ones: A 0x1, B 0x2, X 0x4, Y 0x8, L 0x40, R 0x80,
  ZL 0x100, ZR 0x200 (core/hid.h had A/B swapped until this commit).
- In this game R equips an item into the HUD item box and ZR uses it. R (and L) end
  up in slot site 3 (0x33C9AB4 -> 0x1211C00); x2 is the item index (inventory order,
  1 = deku nut, 3 = boomerang). Overriding x2 before the stock call equips that item.
  button_mapping: X/L/R -> press R (equip with override) -> hold ZR while held ->
  restore ZR's previous item. Verified: nut use pose, boomerang tutorial.
- Not done: pause-menu assignment, slot icons in the HUD, context A/ZR prompts,
  edit mode, adult/child separate sets, stamina on roll/jump action hooks.

## Controls and item icons

- Real controls on this game: X jump, A dodge/roll, Y sword, B/L/R/ZR free. The
  item buttons are B, L, R, ZR (an empty slot keeps its normal behaviour). stamina costs:
  Y = slash, A = roll; with stamina 0 in combat both are removed from the input.
- Assignment (replaces the pause-menu assignment, whose hooks 9-13 turned out to be
  UI constructors): hold ZL and press B / L / R to cycle that slot's item (ZR is not
  assignable that way: ZL+ZR is target-and-use). Assignments are not saved yet and
  there is one set (no child/adult split yet).
- Item index -> ItemPattern icon frame (measured with `BUTTON_PROBE=1 python
  tools/build_core.py ...` + `tools/live_check.py --linger 110`): 0:0 1:6 2:4 3:3 4:1
  5:33 6:34 7:32 9:5 10:8 13:2 14:23 18:36 30:24 31:9. Indices that keep the previous
  frame are not owned in that save.
- PaItemBtn_00 (HUD item button) animations: ItemPattern (icon), StockNum, NumMax,
  NumMin, IsInvalid, Type, ItemChange, Select/UnSelect/Decide/Assign.
- HUD slot icons are NOT done: cloning Pa_SelectItem_00 descriptors (1 or 4 clones)
  produces panes (positions visible in the pane array) and the instances do reach
  hud_prepare, but find_part never lists them as children, so they cannot be driven.
  The stamina gauge clone works. Next idea: find what registers a part in the child
  list (find_part walks a list at owner+0x128/+0x130, entries name at +0x148) and why
  item-button instances are skipped.

## Slot circles attempt (HUD) - blocked

- Texture `Circle64_00` exists in the Common UI archive (stock), so a gauge clone with
  that texture and a full gauge draws a tinted circle (private gauge copy per part,
  distinct asset names; pool 0x40000).
- The layout accepts exactly ONE appended part (the stamina bar): with 5 appended gauge
  parts only the first is ever found by find_part (slots_bound 0). Appending item-button
  clones failed the same way, so the limit is not the part type.
- Reusing four filler parts of the stock layout (Pa_AgonyStone_00, Pa_ReplaceSubItem_00/01,
  Pa_GuideCross_00: re-parented to the root, renamed, descriptors replaced by the gauge
  ones) froze the game at 0 fps. `PART_N` in core/hud.c is back to 1 (stable). The code
  path for PART_N=5 is still in place (build_root, part_replaces).
- Ideas left: find what sizes the child list (allocation near the 0x1525B60 core init)
  and enlarge it; or draw all four circles from one private gauge file with extra panes.

## Multi-part limit investigation (HUD)

- The layout's children list (layout+0x128 sentinel, first at +0x130, entry name at +0x148) lists the
  7 stock parts plus ONLY the first appended one, even with 5 appended parts whose table counts
  (common 38, parts 12) and pane records are correct. All appended instances do reach hud_prepare
  (private gauge copies were built), so the child creation runs but the later ones are never
  registered. Per-call config copies did not help. Instance creation: loop at 0x1525CEC (panes of
  type part) -> 0x152F8F0 / 0x152F990; registration happens in the caller.
- Next candidates: 0x152F990..0x152FD60 (child layout creation) and what registers the entry
  (0x215DC0); or avoid the limit by making the single appended part a custom multi-pane layout.

## Own-layout route: what the archive shows

- `UI/Layout2Archive/Common.Product.100.Nin_Ounce_NVN2.blarc.zs` = zstd (dictionary tools data zsdic1.bin)
  of a SARC with 151 `.buil` files (tools/lib/sarc.py reads it; PlayerStatus_00.buil, PaItemBtn_00.buil,
  PaMpGauge_00.buil, PaReplaceIcon_00.buil ...). In the file, pointers are offsets; the engine adds a base.
- PlayerStatus_00.buil UIL root (file offset 0xF8) has 7 tables of (names, descs, hashes, count):
  +0x20 common panes (33), +0x40 (12), +0x60 (10), +0x80 (2), +0xA0 (0), +0xC0 parts (7), +0xE0 (0).
  Only +0x20 and +0xC0 are extended by the runtime clone; the other tables (12/10/2 entries) may hold
  per-part bindings that the second and later appended parts miss. Not yet checked.
- Item icons are NOT static textures: PaReplaceIcon_00's icon pane uses the runtime texture
  `FBLayout_00_ASTC4x4` (rendered by the game) and ItemPattern selects the item. So the icons on top of
  the slot circles have to come from engine item-button/replace-icon parts; a custom layout alone can
  draw circles, bars and placeholder art but not the item icons.

## Multi-part findings (latest)

- Part instantiation (0x152F8F0) runs for all 5 appended panes, but the child frame object it receives is
  NULL for every appended part after the first (hook id 4 logs pane index and child). Handles are
  distinct and 0x6F60 apart. Growing the Common archive budget in ResourceSizeTable (tools/make_restbl.py,
  +3 MB) did not change it (possibly not applied: other mods' tables).
- Compiler trap fixed: clang turned `switch` returning strings into a lookup table of absolute pointers
  (wrong at runtime in this relocation-free module). CFLAGS now has -fno-jump-tables.
- find_anim returns a shared "not found" object (not null): compare with find_anim(part, "<nonsense>").
- Repurposing stock filler parts (Pa_AgonyStone_00, Pa_ReplaceSubItem_00/01, Pa_GuideCross_00) as item buttons
  (only their part descriptor replaced by Pa_SelectItem_00's, names/parents kept) crashes during load
  (PC=4). PART_N=1 is the stable setting; the code for >1 is still in hud.c (build_root).

## Layout file format (.buil / BFFH) - what is known

- File = header (0x00..0xA8: magic BFFH, file size at +8, section directory at +0x10), two sections
  declared by `BFSI` records at 0xA8 ("Default": data at 0xF8) and 0xD0 ("StringPool"), then the UIL root
  at 0xF8. All internal pointers are file offsets (u64); the engine rebases them at load (no visible
  relocation table: its loader knows the schema). Hence a layout writer must follow the schema exactly.
- UIL root (PlayerStatus_00): name/desc/hash tables {names, descs, hashes, count u32 @+24, 8}:
  +0x20 common panes (33, desc 68 B), +0x40 picture panes (12), +0x60 pictures (10), +0x80 text (2),
  +0xA0 (0), +0xC0 parts (7, desc 88 B), +0xE0 (0). After +0x120: array descriptors {ptr, count, size}:
  +0x130 36 entries (each -> {offset, 0, count, size}: animation/parameter blocks), +0x150 13 hashes
  (parameter names: HeartPos, AgonyStonePos, IsMpGauge, IsItemVisible, IsOpenShortCut, ...),
  +0x1D0 pool 9x16 B, +0x1E0 2x4, +0x1F0 5x8, +0x200 screen size 3840x2160 + ptr, +0x210 12x4, +0x220 32x1.
- Not found: why only one appended part per layout is built. Pane setup code at 0x1529990..0x1529D00 reads
  per-type pools via [root+0x1E8]/[root+0x1F4] indexed by a pool index taken from the part/pane
  descriptor; the appended parts reuse the gauge's descriptor, so this index may need its own pool entry
  (untested: extend the +0x1D0..+0x220 pools).

## Pool extension test (negative)

- Extending the root pools (+0x1D0, +0x1E0, +0x1F0, +0x210, +0x220: count+5, copies of the last entry) with
  five appended gauge parts changed nothing: child frame objects are still NULL for parts 2..5.
  The limit is not in those pools. PART_N stays 1 (stable). extend_pools() remains in hud.c for reference.

## Rupee counter / clock HUD (native look)
- Look copied from the game's own rupee counter: plate textures NumBaseSide_00 (fill) / NumBaseSide_01 (edge), BC4
  507x80 with a chevron on the left end; each is drawn twice (second copy mirrored, scale x = -1) so both ends get
  the chevron. Rupee icon = Nt_RupeeIconNew_00. Sun = Circle64_00 + 8 rotated White8_00_BC4L rays; moon = two
  Circle64_00 (crescent). All live in Common.Product.100.Nin_Ounce_NVN2.bntx.zs (no sun/moon texture exists there,
  nor in the title or notes resources).
- Materials: root+0x130 = {u64 *materials, count, stride}. Picture/text-list entry (+0x40 table) +0x14 = material
  index. A textured material starts with a sampler list {ptr, count}; sampler = {u32 flags, pad, u64 texture name}.
  Textures resolve by name at layout build, so a deep copy of a textured material (P_SubItemBg_00 uses #31,
  MinigameSubItemBase_00) with a new name, appended to the array, gives a picture any texture of the archive.
- Common pane desc +0x40 / +0x42: for a picture +0x40 must be 0xFFFF and +0x42 = index in the picture list. A value
  in +0x40 is an index into a by-name extra-data table ([layout+0x68], lookup main+0x14EEDA0 via 0x43AC20 /
  0x152D080) that the engine searches on every lookup: our old swapped values made it read garbage lists, which
  produced hundreds of thousands of "Unmapped Read/Write" log lines at load (black screen ~2 min, 30-100 MB log).
- Tint: pane object (record +8) +0x20 = RGBA floats, works for pictures and text panes.
- Tuning: g_hudp[] in core/hud.c; tools/hud_tune.py set/get/shot/cfg writes it live (and the HUD options).
  `g_hudp[20]`/`[21]` are the clock plate x/y, so the clock sits to the right of the rupee counter.

## Stamina ring as a dot tube (replaces the 128 gauge parts)
- The ring is now ~230 plain picture panes (P_ShbPic_020..) with the stock Circle64_00 texture: 48 dots on ring 1
  and 64 on ring 2, overlapping so they read as one continuous tube with round ends. A slightly bigger black dot
  behind each one is the empty track and the border. The fill's last dot sits at the exact fill angle (small-angle
  sin/cos rotation of the precomputed dot table), so the end moves smoothly. Yellow when exhausted.
- No gauge file copies or part instances any more (PART_N = 0): lighter to build and to draw.
- Looked at and dropped: SaveGradation_00 (128x128) is an angular (conic) gradient, the usual base for a
  threshold-filled ring, but the game only uses it for the save icon's rotating glow (PaSaveLoadIcon_00, 3
  samplers in one material); using it as a fill would need the ui shader's combiner/threshold settings.
- Live tuning: tools/hud_tune.py set <i>=<v> --sym g_ringp (radii, thickness, border, track alpha/brightness).

## Segmented stamina bar under the magic gauge
- The magic gauge's fill texture is green, so tinting it (blue, amber...) turns it muddy; the bar therefore draws
  its own **flat segments** (`White8_00_BC4L` tinted from the config) over a clean dark strip, with one pane per
  segment. It uses **16 segments**, the same maximum as the magic gauge, so the separators line up with it. When
  stamina drops, whole segments disappear and only the boundary segment shrinks: no texture is ever squashed.
- Capacity grows with the hearts (75 at 3 hearts, 150 at 20), so the bar grows to the right from half to full width
  and the visible segments go from 8 to 16; the left edge stays under the magic gauge.
- Default size and position match the magic gauge: 768 x 32 layout units, left edge aligned with it and placed just
  below it (`g_barp[]`, tunable with `tools/hud_tune.py` at development time only).
- Yellow normally, red while recharging from empty (`g_tired`, which the stamina core keeps set until the bar is
  full again). The colours are fixed constants.
- `stamina_bar` selects it; the ring is hidden with the same flag. The game's D-pad guide cross (`Pa_GuideCross_00`,
  boots/masks/Navi/ocarina) always moves down 60 layout units while the bar is shown so it does not touch the bar.

## Enemy speed components (beta romfs mod)
- Three independent options (movement, animation playback, attack cadence), each with eleven multipliers from x0.5 to
  x6 (plus off). The animation payload ships one pre-generated folder per multiplier; movement and attack are patched
  from the manifest at install time.
- Movement and attack are floats in the enemy `.pack.zs` (Byml): movement in `BSA/MovementParam` walk/run/dash fields,
  attack in `Game/BSA/ActionSeqContainer` timeouts/intervals and `GameBalance/AttackInterval` tables. The same pack
  usually holds both, so the payload ships the 89 original packs once plus a JSON manifest of float offsets; the app
  patches the chosen multipliers and recompresses at install time with the shipped `pack.zsdic`.
- Animation is the FrameController playback rate at `body+0x04` in the `ASB ` element array; the payload ships three
  ready-made `AS/Enemy_*.root.asb.zs` folders. Bosses, minibosses and scripted actors are excluded.
- Old `enemy_speed` boolean profiles still work: they map to all three components at x2.

## Songbook (native song list) - Nov 14

- The remake's songbook is the native one opened with ZL (and the pause-menu song list). It shows only learned songs.
- Ocarina enter/leave are function entries at `main+0x2894340` and `main+0x28947F0` (`pacibsp`, resume +4). `x0` is
  the ocarina/songbook context.
- Songbook fields in that context (observed from the song-list code): selected song index at `+0x22C`, learned flag
  byte per song at `+0x264 + index` (1 = learned), page/state at `+0x278`, UI state at `+0x230`, busy flag at
  `+0x114`.
- Song data: `GameParameter/MelodyScoreBook/Default.game__ocarina__MelodyScoreBook.bgyml`. It has 12 entries
  (Lullaby, Epona, Saria, Sun, Time, Storms, Minuet, Bolero, Serenade, Requiem, Nocturne, Prelude). Each
  `MelodyScore` is a list of note hashes.
- Note hashes map to the five remake buttons: `0xCC...` (3433458314)=Y, `0x313...` (825411668)=X,
  `0x54D...` (1423767502)=A, `0x982...` (2552959238)=R, `0x63C...` (1673550086)=L.
- `features/songbook` does not call the internal playback routines. It reads the selected index/learned flag, waits
  20 frames, then injects the note buttons for 3 frames each with 4-frame gaps through the gameplay input hook. The
  game recognises the melody with its normal logic (sound, animation, effects).
- Scarecrow's Song (index 12) is player-composed and is intentionally not injected.

## Ocarina UI and songbook flow (Nov 14, live)

- D-pad **Left** opens the ocarina. The footer shows `Ver partituras ZL`, `Tocar una nota L R Y X A`
  and `Cancelar B`. ZL is **held** to show the songbook; releasing it returns to the ocarina page.
- Ocarina context fields: page `+0x278` (0 = ocarina page, 1 = song list, 2 = preview), state `+0x230`,
  busy `+0x114`, selected song `+0x22C`, preview/highlighted song `+0x220` (13 = none), learned flags
  `+0x264 + index`.
- Melody buffers: display notes `+0x158` (8 x u32 hash), matcher sequence `+0x478`, ring index `+0x25C`,
  total count `+0x250`. Hashes: A 1423767502, X 825411668, Y 3433458314, L 1673550086, R 2552959238.
- The ocarina state machine (`main+0x3296E20`) is called from `main+0x32923D4` **only when page 0 and the
  "finished playing" flag `+0x26602` bit 0 is set** (or `+0x273` bit 0 with another flag). Hooking that
  function is therefore useless for starting a melody: the flag has to be written from a per-frame hook.
- The matcher is `main+0x32980F0(ctx)`: compares `+0x478` with MelodyScoreBook starting at the last note
  (`+0x25C - 1`, or 7 when 0) and walks backwards. It returns 1 on a match; the caller then sets
  `busy = 2` and clears `+0x244`, `+0x265FC` and `+0x26602`.
- On the preview page the game stores the highlighted song at `+0x220`; pressing A/R/X there plays a
  preview only. The song effect needs the melody on page 0.

## Beta enemy mods (Nov 14)

Both mods are generated from the user's own dump by `tools/make_enemy_opts.py`; nothing is redistributed.

- **Duplication (Option A)**: `Banc/*.bcett.byml.zs` keeps an `Actors` array; a generic `Enemy_*` entry is
  cloned with a new `Hash`, its `Phive/Placement/ID` set to the same value, the **same `SRTHash`** (same spatial cell,
  so the game does not cull it with a wrong cell) and a small `Translate` offset. Entries with `Links` and every
  boss/miniboss/scripted actor are skipped; a plain `Name` is kept (it is a debug label).
  `Scene/Component/AutoPlacementMgrParam/*.bgyml` doubles `CreateActorCountMin/MaxAtOnce` and
  `MinExistActorCountToCreate`, and halves `IntervalToCreateFrame*`. Rewritten BYMLs grow, so
  `System/Resource/ResourceSizeTable...rsizetable.zs` is patched by CRC32 (`binascii.crc32` of the
  path without `.zs`) to the new uncompressed size. When the buttonMapping payload exists, the enemy table starts
  from that one so it keeps the bigger UI budget (the installed enemy table overwrites it).
- **Speed levels**: `Pack/Actor/Enemy_*.pack.zs` scales `BSA/MovementParam` (`Walk/Run/Dash/Escape/Special/Strafe/
  LiftWalk/WalkBackwards`) by the chosen factor, and divides `GameBalance/AttackInterval/*` plus the explicit
  `ActionSeqContainer` cooldown fields. `AttackIntervalRatio` is **not** touched: it multiplies the
  global interval table, so scaling both would give ×4.
- **Animation**: `AS/Enemy_*.root.asb.zs` (ASB, magic `ASB `): header `0x14` = element count, `0x18` =
  element array, stride `0x24`. Each element starts with a `u16` type; type **12** is the frame
  controller and its payload offset is the `u32` at element `+0x08`. The playback rate is the `f32` at
  payload `+0x04` (normally `1.0`, with 0.8/1.1/1.2 variations; `0.0` = paused/instant) and is
  multiplied by 2. Earlier attempts read a different table (`0x0C/0x10`, stride `0x30`), which is why
  they saw only zeros.

## Sprint speed, shield cost, enemy health (Nov 14)
- Sprint speed: the player's ground movement is `main+0x9352F0(player, direction, float speed)`; his own call is the
  `bl` at main+0x934990 (returns to main+0x934994). Hook 10 sits on the function entry and, only for that return
  address, multiplies s0 (saved d0) by `sprint_speed_pct` while A is held and not stripped by the stamina rules.
  Measured: run ~5.6 m/s, native sprint ~7 m/s, x2 -> ~14.5 m/s. Default 125 %. Writing position (actor+0x31C) or
  velocity (actor+0x388) does nothing (both are outputs); RollingMaxSpeed does not drive the sprint.
- L3 sprint and the sprint animation (Nov 16): holding A plays a faster run (7 m/s), but only when A is
  already held before the stick moves; pressing A while moving rolls first. So `sprint_l3` injects A into
  the HID state, and when the press arrives with the stick already pushed it centres the stick for two
  frames (the "hold A, then move" case). L3 then sprints with the game's own sprint animation and never
  rolls: measured 7.3-8.0 m/s against 5.1-5.6 m/s running. Forcing the action-state name (comp+0x244) to
  `MoveDash` changes the pose too, but it is sticky and freezes the movement state machine: do not use it.
- Shield cost: the hit arrives during the player's hit reaction, when his movement update pauses (player_active() false
  for a few frames) and queued events were dropped. Shield hits are now a counter (player_guard_seq) kept across
  pauses shorter than 60 frames; GuardFailHit (the hit went through) no longer counts.
- Enemy health: works (g_hp_log). Small enemies with MaxLife 1 scale to 3; the large enemy with MaxLife 6 only takes
  damage on its root (IsHeadshotOnlyDamage, bone Tail_1). The starting sword has EquipmentPerformance 1.
- Both multipliers are a plain percentage the module does the maths on (`enemy_hp_pct`, `damage_taken_pct`: u16,
  no cap in `enemy_scale` / `hp_mul` / the damage share), so the app's ladder is the only limit: it now runs
  ×1, ×1.5, ×2, ×2.5, ×3, ×4, ×5, ×6 (600 %) in both apps, and ×6 was read back from a running module
  (`SHBCFG01+26` = 600, `+28` = 600) with the game in gameplay.
- Stamina refill wait: 0.8 s, 1.8 s in combat, 2.5 s after exhaustion.

## Great Fairy rewards and Farore's Wind outdoors (Nov 14)
- Fountain -> flow mapping (from `Banc/<stage>/*_Dynamic.bcett.byml`, `MapIconFacilityIsOpen`):
  `HyruleCastle` -> `Dm_FairyFountain_Power`; `DeathMountainTrail` -> `Dm_FairyFountain_Din`;
  `DeathMountainCrater` -> `Dm_FairyFountain_Wisdom`; `DesertColossus` -> `Dm_FairyFountain_Nayru`;
  `ZoraFountain` -> `Dm_FairyFountain_Frore`; `GanonCastle` -> `Dm_FairyFountain_Courage`.
- Each flow carries two item strings: a `SwitchEvent` param `GameDataName = IsGet.<item>` (already received?)
  and a `SubFlowEvent` to `Dm_SY_GetInEvent:GetInEvent_Demo_NotLookatObjectEntry` with `ActorName = <item>`.
  The reward line is an `EventTalk` param `MessageId = EventFlowMsg/<fountain>:<fountain>_0004`.
  `Dm_FairyFountain_Power/Wisdom/Nayru` can be remapped by changing those three only; the `evfl` Python library
  rewrites the file and is byte-identical when nothing changes. The `ForbidSetWarpPointArea` actor only appears in
  FireTemple, so it is unrelated to the outdoor rule.
- Farore's Wind: `Event/EventFlow/WarpEvent.bfevfl` entry `FaroreWind_SetWarpPoint` starts with a
  `SwitchEvent` on `EventQueryIsForbidSetWarpPoint`: `0` = `DisableSetWarpPoint` (real forbidden area),
  `1` = `DisableSetWarpPointDungeon` (message "you cannot place a warp point outside a dungeon"),
  `2` = set the point. Pointing case 1 at the same event as case 2 allows outdoor points; case 0 stays blocked.
  The `FaroreWind_Warp` entry has no dungeon check, only the MP check.
- `Mals/<lang>.Product.110.sarc.zs` holds `EventFlowMsg/*.msbt`. The Din fairy's sister overview (messages 7/9/10)
  still names the original rewards; fixing it needs an MSBT TXT2 rebuild (not done in this beta).

## Area collectible HUD counter (Nov 14)
- Current area: GameData `Sequence_CurrentBanc` is **String64 index 31** (`HyruleField_Child`, `Market_Child`,
  ...); the live string is at `+0x74` of a 0x108-byte record, same layout as the quick-select equipment slots.
- Gold Skulltulas: `IsGoldSkullDead` is a **Struct** with 100 Bool members. The member name is
  `IsGoldSkullDead_<placement hash as unsigned decimal>` and the full Bool entry is
  `IsGoldSkullDead.IsGoldSkullDead_<hash>`. Those Bool entries are also in the GameDataList's `Bool` section,
  so the live value is the Bool table record at index `position in that section` (stride 0x18, live byte at +0x16).
  114 Gold Skulltula actors map to 100 unique flags (adult/child Banc variants repeat some).
- **Those flags are runtime only** (Nov 15): the game sets them when the token drops and starts every Banc load
  with all of them false, so counting them alone reads 0 after leaving an area and coming back (heart flags are
  real save flags and survive). The durable record is the save entry **`Game`**, type `Bool64bitKey`, hash
  `0xA3DB7114`: a header u64 (`0x00283696ACC3DD10`), then the collected actor hashes **ascending**, then a 0.
  Live copy = the loaded `progress.sav` image: `gmd::GameDataMgr` (main+0x4E842A8) holds it in the save
  descriptor array at **+0xE80** (0x80-byte stride; magic `0x01020304` at +0, data offset u32 at +8). The entry's
  offset comes from the save index (`(u32 hash, u32 value)` pairs from +0x20 up to that data offset).
  `hud.c` caches the key array, binary-searches it per Skulltula and also remembers pickups seen this session,
  so a new token counts at once and still counts after the reload.
- Heart pieces: `HeartPieceAnchor` actors with `Dynamic.HeartPieceAnchor_GameDataPath` name a Bool flag
  (e.g. `DeathMountain_Enemy_Scrub_Business_HeartPiece_01`); 21 anchors / 16 unique flags. World
  `Obj_Heart_Piece` actors count by actor hash in the key set. Chests whose `Dynamic.Drop__DropActor` is
  `Obj_Heart_Piece` (10 in the Bancs: Gerudo Fortress/Valley, Graveyard, Dampe race, Hyrule Field, Market minigame,
  Zora's Domain) record their opening in the Bool `IsTreasureBoxOpen.<actor Hash>` (name found by hashing; 7 distinct
  hashes exist in the Bool table). Before this they were missing from the area total, so an area whose only piece
  was a chest read 0/0 after picking it up. Not verified in game: the table was rebuilt and the module builds, the
  live run did not reach gameplay.
- `tools/make_collectibles.py` scans the Bancs and writes `build/collectibles_data.h` (43 Banc names,
  135 Bool indexes + the 135 aligned 64-bit actor hashes) before the core build; `hud.c` includes it and reads
  the Bool table and the save key set live. If the dump is missing it writes an empty table and the plate stays
  hidden.

## Equipment slots: value **and** copy (Nov 15)
- The `PlayerStatus.Equip*` String64 records hold three SafeStrings: default (+0x24), **value** (+0x74, pointer
  at +0x68) and **copy** (+0xC4, pointer at +0xB8). Writing only the value updates the menu icon but Link keeps
  the gear he had; the apply path needs the copy as well. Writing both makes the change take effect at once
  (verified with the mask: Bunny -> Lens of Truth removes the ears, back restores them) and keeps the save
  consistent. `quick_select.c` writes both groups.
- Ownership for the "only if you have it" rule: `IsGet.<actor name>` in the Bool table, looked up through
  `core/gd_index.h`'s `GD_ISGET` (hash = murmur3("IsGet." + actor)); `hud_owns_name()` does the lookup.
## buttonMapping: per-save assignments (Nov 2026)

- Save files: mount `save:`, `slot_00`..`slot_11/progress.sav` (+ `caption.sav`), `account.sav`, `option.sav`.
  The game opens `progress.sav` with mode 1 when it loads that save and mode 6 (write | append) when it saves,
  and it rotates between slots (loaded slot_02, saved to slot_04). Then it commits the mount.
- Hook: the `nn::fs::OpenFile` import stub, main+0x3C42590 (`adrp x16` re-run from our wrapper with the
  `GOT_FSOPEN` page, then main+0x3C42594). Our own file `slot_NN/ootmaster.bin` (16 bytes: "OOTM", version 1,
  child B/L/R, adult B/L/R) is written right when the game opens that slot's progress.sav to write, and read
  when it opens it to read. Other nn::fs stubs used: CloseFile 0x3C425B0, CreateFile 0x3C425D0, WriteFile
  0x3C425E0, SetFileSize 0x3C42940, ReadFile(size_t *out, ...) 0x3C43050 (all main+, game 1.1.1).
- GameData Bool table (k = 0): 0x18-byte records in GameDataList order, live value at +0x16.
  `Sequence_IsChildhood` = 1 while Link is a child; `IsGet.<actor name>` = the item is owned.
  `core/gd_index.h` (tools/gen_gd_index.py) holds their indices for GameDataList.Product.110.
- `PlayerStatus.AssignableItem.Child` / `.Adult` (Enum) are the game's own item box (ZR) per age.
- The item box (ZR) per age is `PlayerStatus.AssignableItem.Child` / `.Adult` (Enum table k = 6, 0x38-byte
  records, live value at +0x1C = murmur3 of the value name). Writing "Empty" there empties the box and the HUD
  follows. The value -> item actor comes from RSDB `PouchActorInfo` (`AssignableItem` column), in
  `core/gd_index.h` (`GD_ACTOR_BOX`). The HUD controller path (+0x150 -> +0x1F0 -> +0x38) is not reliable
  for the box name (garbage in some saves); it is only the fallback now.
- In the pause menu, A sets the box only when the menu closes (GameData unchanged until then).
- **Which box (Child / Adult) is the game's item set, not `Sequence_IsChildhood`.** The R selector passes byte
  `+0x5C0` of `**(main+0x4C7E2A8)` to the equip call (main+0x33C9A04 → 0x1211C00); the box getter main+0x17B6E0
  names the key from it: `set & 1` indexes `{"Adult", "Child"}` at main+0x3F13A20 (1 = `.Child`, 0 = `.Adult`).
  Found live (Nov 22): IsChildhood 1 but set 0 — the game read and wrote `.Adult` (record 59; ZR fired the bow)
  while the mod read `.Child` (record 65, a stale BottleA), so the ZR icon stayed on the old bottle and a slot
  restore put that bottle back. `hud_box_child()` (core/hud.c) picks the record for every box read/write.
- Enum records (0x38 bytes): `+0x18` default, `+0x1C` live, `+0x20` copy; the menu write updates live and copy.
  The table's `+0x18` hash→index map confirms Child = 65, Adult = 59 for Product.110.

## Item quantities on the buttonMapping HUD (Nov 17)

- The stock item box's count is the text pane `T_ItemNum_00` inside `N_Num_00` (`PaItemBtn_00.buil`, which is
  the asset of `Pa_SelectItem_00`). The button clones already have both; the count was simply never written.
- Current quantities are GameData **Int** `PouchData.<item>.StockNum`. The Int table is k = 2 in the manager
  (`mgr + 8 + 0x60 * 2`), records of `0x28` bytes, live value at `+0x1C` (the devtool's `gdlive.py`
  LAYOUT). `tools/gen_gd_index.py` writes `GD_STOCK[] = {murmur3(item actor name), Int index}` from the 70
  `PouchData.Item_*` entries; the bow has no own counter (uses `Item_Arrow_Normal`) and the slingshot uses
  `Item_DekuSeed`.
- The HUD resolves the index only when a button's item changes, then reads the Int each frame and writes the text
  only when the number changes. `N_Num_00` is hidden when the item has no quantity. Only an allowlist of stackable
  items shows a count (stick, nut, bomb, seeds, bombchu, bean, bow and elemental arrows): every item, unique ones
  included, has a `PouchData.StockNum`, so a generic lookup put "1" on the boomerang, hookshot, etc. ZR is the game's
  own box item and does not go through the button slots: its count comes from `hud_zr_item()`.

## Hiding the corner minimap (Nov 17)

- The corner map is the `MapMenu_00` layout (`Common.Product.100...blarc`). It has no text of its own while playing,
  so the set-text hook never saw it: the frame is taken from the layout instead. Hook id 4 (part instantiation)
  already reads `layout + 0xF8 -> UIL resource -> +0x18` (the asset name); the left of the layout, `layout + 0xB0`,
  is its frame (the PlayerStatus layout uses the same field). The first `MapMenu_00` part instantiation gives the
  minimap layout.
- While playing (PlayerStatus frame `+0x1A4` bit `0x20000`) the option **moves** `N_MiniMap_00`, `N_MiniMapCap_00`,
  `N_Common_00` (player arrow and map icons), `N_MapIcon_00`, `P_MiniMapFrame_00` and `N_WorldMap_00` down out of the
  screen (`+0x70`, position only, `-4000`: negative y goes down, the same sign the guide bar uses) and puts their
  saved y back the frame a menu covers the game.
- **The scale must not be touched here.** Zeroing it (`+0x60`/`+0x64`) is what the option did until Nov 24: it hid
  the corner map, but the map screen draws with *these same pane records* and the game does not restore their scale
  when it opens, so the map screen came up with no map at all - the player reported exactly that. The y is written
  back the moment the game reports a menu (so before the map screen draws) and only the position moves while
  playing. Judged in game: the corner map is gone and nothing else changes; the map screen was not re-photographed
  after the change (the option is off by default and needs a capture with it on both ways).
- The layout is re-bound when the captured frame changes (an area load rebuilds it), and the saved y is taken at
  that moment only, so the game's own transform during the map screen never becomes the restore value.
- Diagnostics: `g_minimap_dbg[0]` = frame, `[1]`/`[2]` = the first two pane records, `[3]` = 1 while the map is
  moved out. `g_shbcfg + 77` is the option's byte (write it live to compare on/off).

## Magic jar drops (Nov 17)

- `DropGlobalParam` has `HpActor` (heart) and `MpActor` (magic jar, `Obj_MagicJar_Small`). Pointing either at
  `Obj_Rupee_Green` blocks that drop with a same-size string-index swap (heart: no_heart_drop, magic: no_magic_drop).
- Both options edit the same file, so when both are on the installer uses one combined copy
  (`no_heart_magic_drop`); copying the two single files one after the other would leave only the last one.

## External font: the loader's size budget (Nov 22)

- The game's button glyphs (A B X Y L R ZL ZR = U+E040..U+E047) live in
  `Font2/Scalable/External.Nin_Ounce_NVN2.bfotf.zs`: zstd with the game dictionary (`zs.zsdic`) over an 8-byte
  header (`w0 = 0x18029A7F ^ key`, `w1` carries the unpacked size) and an XOR-obfuscated OTF (`OTTO`). A
  font that replaces the button glyphs with the community layout's is 15632 bytes unpacked against the vanilla 14560.
- **The loader (Owl, inside the system NSS) aborts with a null call when the unpacked OTF is too big**, not when a
  glyph is wrong: `Unmapped Read64 @ 0x0` at `Owl.nss+0xA7F6EC`, then `pc 0x4`. The crash happens while the
  resource loader is reading the `Font2/S...` path (R06/R07 hold that string), ~15 s into boot.
- The budget was measured by bisection: the vanilla font padded with zeros boots at 14636 bytes and aborts at
  14676. Useful limit **14640**. It does not come from the ResourceSizeTable: the installed table already reserves
  0x5000 (20480) for the font and the game still aborts.
- The full PlayStation font cannot fit (15632). The safe build is the **vanilla font with only the eight button
  charstrings replaced and the OpenType `name` table dropped** (524 bytes the game never reads): 14232 bytes, boots,
  and the game draws the PS glyphs (verified in a shop dialogue: B = ✕ and the "previous dialogue" R1).
  `tools/make_button_font.py` builds it into `app/payload/button_romfs/playstation/`; `tools/font_variant.py`
  repeats the bisection (`--set list:uniE046,uniE047`, `--pad N`, `--boot`).
- The frame may be compressed **without** the dictionary: the game's decompressor accepts it, so the app does not
  need to ship `zs.zsdic`.

## Collectible counter and GameData indices (Nov 22)

- **Record indices shift with romfs mods.** Paraglider ships its own `GameDataList.Product.110` with 3 more Bools
  and 1 more Int, so every Bool after its additions moved by 3: the stock-list index of `Sequence_IsChildhood`
  (10836) read another flag (live index 10839), and every IsGet / IsGoldSkullDead / heart-reward index was off.
  Other types kept their counts. Fix: each GameData table has the game's own open-addressing map (table `+0x18`:
  `{u32 hash, u32 index}` slots, table `+0x24`: slot count, start at `hash % slots`, linear probe, empty = 0;
  the game's probe is at main+0x5890DC). `gd_find(k, hash)` in core/hud.c uses it; Bool, Int, the item box Enum
  and `Sequence_CurrentBanc` are looked up by hash now. Still index-based (counts unchanged by Paraglider):
  String64 equip records, the bottle Enums 60..64, UInt64 `World_CurrentDateTime`, quick_select's String64.
- Stock-list hashes used: Int rupees `0xA77921D7` (962), wallet `0xE0010A50` (976); String64 `Sequence_CurrentBanc`
  `0x1D6189DA` (31); Enum box Child `0xA0029837` (65) / Adult `0xBC32CE24` (59); Bool `Sequence_IsChildhood`
  `0xCEA220D3`. Compare a mod's list against the live counts (`mgr + 8 + 0x60*k + 8`) when something reads wrong.
- **The save buffer at manager `+0xE80` is shared.** The descriptors at `+0xE00` (0x80 bytes each: progress.sav,
  option.sav, slot_NN/caption.sav, account.sav; name at `+0x24`) all point at one buffer; a few seconds after the
  save loads it holds a small file (data offset `0x2A0`, no `Game` entry). The counter now copies the key set
  (`g_kset_copy`, up to 4096 keys; this save has 705) whenever the buffer holds progress.sav, checks every frame
  (the data offset changes when the file does) and keeps the copy otherwise. Before, it dropped to 0.
- **Heart pieces**: world pieces are actors `Obj_Heart_Piece` / `Obj_GerudoValley_Heart_Piece`; their actor hash is
  added to the same `Game` key set when picked up (checked: both Kakariko pieces present). `HeartPieceAnchor`s with
  a `HeartPieceAnchor_GameDataPath` are NPC / minigame / scrub rewards (16 paths, their Bool is the record); those
  without a path only mark the world pieces. A world piece picked this session counts once the game saves.
- `build/collectibles_data.h` stores Bool key hashes (`COLLECT_FLAG_HASHES`); hud.c refuses an older index table.

## Context prompt next to Link (`DoCommand_00`, Nov 22)

- The "(A) Hablar" prompt is layout `DoCommand_00` in `UI/Layout2Archive/Common...blarc`. It has one block per
  face button (`N_A_00`, `N_B_00`, `N_X_00`, `N_Y_00`). Each block draws a diamond: a backing `P_Base?_00`, the
  other three letters greyed at alpha 0.13 (`P_B_00`/`P_X_00`/`P_Y_00` for A, etc.), and the key bubble with
  `T_Key?_00` (the button glyph) and `T_Guide?_00` (the action text, captured as `g_aguide_text`).
- The game drives the blocks by **virtual** button. With the remap, core/hud.c `dc_update()` hides the backing and
  the grey letters (scale 0). It also writes the glyph of the physical button that holds the block's role
  (`remap_role_phys`) into `T_Key?_00`: through set_text when the prompt comes up, and by rewriting the game's own
  set_text call in the text hook. The glyphs are the button font's U+E040.. (A B X Y L R ZL ZR), so the PS style
  follows. Action is limited to A/B/X/Y in the app for that reason.
- Checked live: action on A shows ○ Hablar, on X △ Hablar, and physical X talks.
- **The remap is off in events.** Talking or a shop puts Link in an `Evt*` state (`EvtWait`) while the player stays
  active and the HUD is not covered, so the remap used to stay on: action on B confirmed shop purchases.
  `button_remap_on_input` now leaves the real buttons in `Evt*` too (A next, B back) and publishes `g_remap_live`;
  the prompt glyphs follow it (own glyph back when the remap is off). Checked live with action on X: X talks,
  the real A advances the text, the real B picks "Adiós" (no purchase), and △ Hablar comes back afterwards.

## No pointer tables in module data (Nov 22)

- `subsdk9` is linked at a fixed base and the loader applies **no relocations**. Longshot, ReLink and the emulator
  place it exactly at that base, so an absolute pointer stored in `.rodata`/`.data` happens to be valid there.
  Rekarina links it for `main + 0x5200000` and hands the blob to its own loader: such a pointer arrives
  **truncated to 32 bits** and every read through it faults.
- `armory_drawer.c` had `static const char *const PART[2] = {"Pa_SortWindow_00", "Pa_Guide_00"}`, and
  `quick_select.c`'s `first_owned_mask()` eight mask names. Fix by returning the literal from code or by using
  `char names[N][LEN]` (an array of arrays is bytes, not pointers). `tools/build_core.py` now fails the build
  (`check_pointer_data`) when any object keeps an `R_AARCH64_ABS64` in `.rodata*`/`.data*`; `llvm-objdump -r`
  reports those blocks as `.rodata` or `.rela.rodata` depending on the object, so the check normalises both.
- That was a real bug, but **it was not the armory crash**: after removing the table the same fault came back with
  `x0 = 0x05212177`, i.e. the address of the module's own string (`0x7005212177`) with the top bits gone. The
  truncation happens when a **module pointer is handed to the game's `main`**: `header_show` -> `ui_get` ->
  `main+0x43c4a0` (find-pane-by-name), while `find_part()` in the same function survives the crossing. With
  `SHB_REKARINA`, `ui_get()` now stops after `find_part()`; if it misses, the armory keeps the sort header visible
  (cosmetic) instead of calling the game with a module pointer.

## HUD modes and the simple bar (Nov 22)

- The app's `hud_mode` is now **wide / simple / plain**. `simple` and `plain` share the same four-block row
  (`hud_x`), the same scales and the clock's sun/moon (`hud_mode >= 1` test in `hud.c`); only `hud_plates`
  differs: 2 draws the bar, 0 leaves the row bare. The old "minimal" (map) mode is gone and its config byte 2
  now means `plain`, so a configuration saved with minimal lands on a sane mode instead of failing.
- The bar is its own picture pane `PIC_BG` (the last one, `P_ShbPic_274`): a clone of the sub-item picture with
  the flat white texture (`White8_00_BC4L`) tinted black. Its geometry is `g_bg_tune` = {x shift, extra width on
  the right, alpha, height}: -165, +10, 0.42, 1.05 puts the left edge on the screen edge and the right one just
  past the hearts number. The wide plates are no longer drawn in `plates == 2` (that was the old "one bar plate").
- The skull / heart icons get `1.30x` in the one-row modes: their art carries more transparent padding than the
  rupee and sun, so they read smaller at the same scale.
- Pause menu: `hud_menu_legend` no longer shows its backing plate (the `P_ShbPic_399` clone is scaled to zero),
  and `g_mb_lay` moved to {-740, -90} (a little up and right of the old spot).

## Widescreen HUD groups (Nov 22, PR #10 by HaseoSora)

- A widescreen planner ("Screen's edges" in Longshot / the HUD-edges patches) shifts each root-level pane on its
  own, so the button cluster and the counter row came apart: the spacing between elements is a *local* position,
  and every pane was being moved independently. The fix groups them under three non-drawing panes that are cloned
  from the layout root's first pane (the stock, non-drawing one): `N_ShbActions_00`, `N_ShbCounters_00` and
  `N_ShbStaminaRing_00` (CityHash64 of the names, same function as `tools/lib/cityhash.py`).
- The UIL pane tree is two u32s in each pane descriptor: **`+0x28` (40) = parent pane index** and
  **`+0x34` (52) = next sibling index**, with the root's first pane carrying the end of the whole list. A parent's
  descendants must be **one contiguous run**, so `group_hud_panes()` reorders the pane table (`root+0x20/+0x28/+0x30`,
  after the `nc` stock panes) and rewrites every `+40`/`+52`, the root's `first+52` and each part descriptor's `+64`
  (part -> pane index). The picture/text tables keep their own order: `+0x3C`/`+0x42` index them and do not move.
- Children then hold *canvas coordinates minus the group origin*: `ACTIONS_X = 1800` (right-hand cluster) and
  `counters_x() = -1800` (the counters row is on the left in every current HUD mode; the sign used to follow the
  removed right-side "map" layout, and byte 2 is `plain` now). Every placement that writes a HUD position subtracts
  the matching origin: `put_text`, `plates_update` (plates, icons, sun/moon, `PIC_BG`) and `slots_draw`.
- The simple row's soft bar (`PIC_BG`) belongs to the counters group, otherwise the numbers travel and the bar
  stays. The stamina bar's own panes (`PIC_BAR`..`PIC_BG`) stay at root level: they are placed around the bar's
  own anchor (`g_barp`), not the counter row.
- `group_hud_panes()` failing returns 0 from `build_root()`, i.e. no mod HUD at all (`g_hud_dbg[9] = MARK|5`);
  a partial tree is not left behind.
- The three group names must be a `char names[3][20]` array, not `const char *names[]`: the pointer table is an
  `R_AARCH64_ABS64` in `.rodata` and `tools/build_core.py` rejects the module (no relocations). This is the first
  thing the build caught when the branch was merged.

## Switch GameCube preset (Nov 22, PR #10 by HaseoSora)

- `bm_layout` byte 25 = **3** adds the GameCube face-button geometry (`g_sb_gamecube`: A centre, B lower left,
  X upper right, Y above-left) and `face_place()` skips the A/B swap for it. The buttons keep their logical roles;
  only where they are drawn moves. The `OotBtnG_*` art is generated at release time by
  `tools/make_gamecube_buttons.py`, which needs the game's UI zstd dictionary (`SHB_ROMFS` or
  `SHB_UI_DICTIONARY`) because it decompresses the stock `OotBtnS_*` BNTX files as templates.
- The System tab's Basic Controls diagram lives in `UI/Layout2Archive/Common.Product.100.Nin_Ounce_NVN2.bntx.zs`:
  a 244,460,000-byte BNTX compressed to 64,862,732 bytes. `app-legacy/gamecube.py` decompresses it, verifies the sha256 of
  the three textures it replaces (two `Controller_Pro` half-diagrams and one more), patches them in place and
  recompresses at level 6 (72,937,387 bytes, ~10 s). No dictionary is needed to read or write that file.
- An emulator install has no romfs dump on disk, so `prepare_gamecube_layout()` turns a missing/unsupported atlas
  into an install **note** (`n_gamecube_atlas_skipped`) instead of failing: the HUD art works without the diagram.
  A `GameCube-System-Controller` add-on that replaces the same atlas is renamed `.disabled` only while the preset
  is selected.

## Input samples: use what GetNpadStates returns (Nov 22)

- The gameplay read (site 3, main+0x346966C) calls `GetNpadStates(out, 16, id)` into a **persistent** 16-sample
  buffer inside a game object (`x8 + 0 .. 0x280`, stride 0x28) and keeps the return value (samples written).
- The emulator fills all 16 and returns 16. **Rekarina's runtime writes only `out[0]` and returns 1**
  (`setup/<ver>/runtime/ov_input.c`, `npad_states`: "the newest state only"). Slots 1..15 are never refreshed there.
- Every input handler used the requested count (16) and ORed all samples, so on Rekarina it read and kept its own old
  writes. Sprint: `l3_inject_a` wrote A into all 16; slots 1..15 kept it, the merged buttons said "A held" forever,
  so the injection into slot 0 stopped (no sprint animation), `g_sprint_a` stayed on (speed + drain) and the toggle
  could only end when exhaustion stripped A from every slot.
- Fix: `hid_sample_count(a1, stock_result)` (core/hid.c) gives the returned count when it is 1..requested; every
  handler uses it. Emulator unchanged (16). Rekarina's runtime ships as source in `setup/<ver>/runtime/`: read it
  before reverse-engineering any HLE'd SDK call.

## HUD scale sliders (Nov 22)

- Config `hud_ctrl_scale` (struct 90) and `hud_count_scale` (struct 91), percent, 0 = 100. They use the struct's old
  tail padding, so `sizeof(ShbConfig)` stays 0x5C; the app writes them when the block is at least 0x5C and clamps to
  50..200 (`installer.hud_scale`). CLI: `--hud-ctrl-scale`, `--hud-count-scale`.
- `hud_group_scale()` (core/hud.c, every HUD frame) scales the group panes `N_ShbActions_00` / `N_ShbCounters_00`
  (record scale +0x60/+0x64, position +0x6C/+0x70), so every child (plates, icons, texts, sun/moon, the simple bar)
  scales with it. The group's position moves by `(1 - s) * L`, L = the nearest screen corner in group-local units
  (controls `(120, 1080)` top right, counters `(-120, -1080)` bottom left), so the corner stays put and the
  margins grow with the scale. A position another writer set (widescreen planner) becomes the new base; at 100%
  nothing is written.
- Checked live in the emulator: 150/150 -> controls group (1740, -540), counters (-1740, 540); 60/60 -> (1848, 432) /
  (-1848, -432); installed through the CLI at 130/80 the module applied it from the first frame; the pause menu is
  unaffected. At 150% the wide counter row reaches the minimap's left edge.

## A Longshot crash at the intro: writing into save:/ (Nov 23)

- Report from a player (Longshot v3.1.1, 23 s uptime, map "Title and intro"): `0xc0000005` read in thread
  `SceneLowPriority` at `longshot.exe+0x1927c87e`, right after `gmd::SaveDataMgr: fs: save: published 2 files` and
  after the intro movie ended (there `nn::movie::Finalize` is an unimplemented import). Without the mod it does not
  happen, per the reporter.
- Reading the guest dump: `main` is at `0x80020000` on Longshot **and** in the emulator, so `0x80794090` is
  `main+0x774090` (the bytes match stock `main`: no hook of ours is there). What matters is `x20 = 0x80794090`, i.e.
  a **code address used as an object pointer**: `main+0x7740f8` is `ldr x8,[x0]; ldr x8,[x8,#0x30]; blr x8`, so it
  read the "vtable" out of instruction bytes (`x8 = 0xb5000235f9413e75`) and faulted. The enclosing function
  (`main+0x773660`) is a refcounted release (`ldadd -1`, destroy at 0) of an entry in a handle table
  (`[[main+0x4c82d60]] + 0x9f8`, stride 0x28): a half-built object being released.
- The mod was the only extra code writing into the save volume at that moment: `dbg_tick()`
  (`features/button_mapping/button_mapping.c`) ran on every input read and every 600 reads did
  `fs_create_file` + `fs_write_file` of `save:/slot_00/shbdebug.bin` (plus one dump per `progress.sav` open). It had
  left 23 of those files across the save folders of the test emulator. That machine ran the intro uncapped (log:
  1200 presents at 200 fps, `frame_rate_cap=0`, `vsync=0`), so the frame-counted timer fired every ~3 s instead of
  10 s — inside `gmd::SaveDataMgr`'s commit window. Timing, not a bad pointer of ours.
- Fix: the dump is a **test build only**. `build_core.py --debug-dump` defines `SHB_DBG_DUMP`; a release module has
  no `SHBDBG01`/`shbdebug` strings and creates no files. Verified in the emulator: boot, save load, HUD live at
  frame 5400, and every stale `shbdebug.bin` in `user/nand/user/save` kept its mtime.
- Not proven: that the race caused that crash. It is the only mod-side I/O in the window and the stack shows a
  corrupt object being released, but reproducing it needs the 200 fps timing.
- The player's own `longshot_log.txt` (same build, same crash) adds four things: the mod's **input wrapper really
  runs there** (`hid: the game reads npad 0, lr 0x84fa04b8` = `subsdk9+0x4B8`, inside `hook_input3`); the guest
  runs at **37 fps with Longshot's "real-time step" on, ratio 1.603** (`vi: real-time step on`), so that machine
  never reaches 60 and the guest's frame step is scaled; the crash follows `nn::movie::Finalize`, which is an
  **unimplemented import** (so are `movie::Initialize` and `IsInitialized`), by about a second and right after
  `gmd::SaveDataMgr: fs: save: published 2 files`; and no other mod is installed.
- Because of that frame step: on 0.9.x, every duration counted in frames ran **1.62x long** on that machine
  (60/37). That is what `core/clock.c` fixes, but only if Longshot scales `CNTPCT_EL0` with the same step — one
  test on a recomp answers it (`g_clock_dbg[1]` against the log's `game's frame rate`).
- Test-protocol trap: on a recomp, **deleting `mods/zz-Super-Hero-Boost` removes only the RomFS**; the module and
  the 26 hook sites are baked into `image.bin`/`bin/longshot.exe` by the rebuild. "It does not crash without the
  mod" only clears the mod as a whole if that test rebuilt the game from a clean exefs.

## Prompts: the context prompt keeps the game's keys, the interaction bar is left alone

- The context prompt next to Link is layout `DoCommand_00`: one block per face button (`N_A_00`..`N_Y_00`) with the
  diamond backing `P_Base?_00`, the action `T_Guide?_00` and the key `T_Key?_00`. `dc_update()` hides the backing and
  the three greyed letters and writes the key of the button that holds the block's role (action / item 1 or jump
  with the Jump/B swap / sword), gated on `g_remap_live`; `dc_key_override()` catches the game writing those panes.
  A second writer for the same panes (`guide_keys_update`, commit c173c15) fought it - it ignored `g_remap_live`,
  so a dialogue showed a world glyph - and was removed.
- The interaction guide (the "Blandir ZR" / "Guardar B" bar): rewriting its keys was tried and reverted (the game
  re-sets the panes, two writers blink; a shared glyph buffer made every entry show the same letter). It is now
  **hidden by default whenever the button-mapping HUD is on, with no option** (`gi_*` in core/hud.c).
  - The bar is ONE container layout: `RootPane`, `N_All_00`, `W_Bg_00` (the dark plate), `A_Align_00`,
    `Pa_GuideReverse_00`, `Pa_Guide_00..04`. The entries (`N_InOut_00`, `T_Guide_00`, `T_Icon_00..04`) are its child
    layouts. A child layout's `+0` / `+8` are its previous / next sibling and the first child's `+0` is the list head
    `container + 0x128`: that is how the container is found (it is **not** `*(u64 *)layout`, which is a sibling).
  - The pause menu's footer legend has entries with the same pane names but another container, so the container is
    recognised by `W_Bg_00` + `Pa_GuideReverse_00` and only then touched. The text hook only finds it (a `T_Icon_NN`
    / `T_Guide_NN` / `T_Btn_NN` write); not every entry is written through that hook.
  - **Hide = move the container's panes (all but `RootPane`) to y = -4000, nothing else.** Scale 0 + tint alpha 0 +
    y = 4000 hid it too, but froze the game the moment an item other than the stick was used (stock `development`
    does not: A/B with the same ZL, R, ZR sequence over the debug server). Position only is stable.
  - Debug: `g_gi_hidden` = panes moved last frame (0 with the bar on screen = container not found), `g_gi_off` = 1
    stops it live. Panes written from outside the module (debug server) are re-set by the game's per-frame
    animation, so a hide can only be judged from the module's own per-frame write.

## HUD counts: the ZR button followed the borrowed item (Nov 23)

- Symptom from the player: using an item changed the number shown on *another* item's button (e.g. the slingshot's
  ammo), and it came back when the use ended.
- Cause: `zr_mirror()` (core/hud.c) keeps the ZR button showing the game's item box. While a slot item *borrows*
  the box (`g_zr_hold`, features/button_mapping), the box holds the borrowed item; the icon was frozen but the
  item index (`g_zr_index`) was not, and the number drawn next to the button is `gd_int(item_stock_index(g_zr_index))`.
  Proven live: with a deku stick borrowed, `g_zr_index` was 0 and the ZR button showed the stick count (4) instead
  of the slingshot's (2/6). Fixed by freezing the index too.
- The *flicker* seen while trying to fix it came from writing the count text every frame: two writers alternate on
  the same pane (the game updates item-count panes by name, and our cloned parts repeat those names). The text is
  written when our value changes again; `hud_on_settext` catches a game write into one of our buttons' count panes
  and puts our value back in the same call (`g_num_fix_n` counts it; 0 in a standing-still session, so the ZR freeze
  is what the player saw fixed).

## Frames are not milliseconds: the module's own clock (Nov 23)

- Every duration in the mod was written in frames with a hard-coded 1/60 s (`DT_FRAME`, `SW_HOLD 18`,
  `GAP_TICKS 20`, `RUN_HOLD_FRAMES`, `L3_IDLE_FRAMES`, `g_check_owned = 30`, the 300-frame combat window). That is
  only right when the game runs at 60: the crash report that started this had the intro at ~200 presents/s, and
  the emulator here runs the game at 40-48 fps while it settles, so the same build ticks 20%..230% off.
- The source is `CNTPCT_EL0`, not a wall clock and not a new import: `sdk` reads it 45 times (the
  `nn::os::GetSystemTick` path), so every target that runs the game at all — emulator, Longshot, ReLink,
  Rekarina — necessarily translates that instruction. `main` itself never reads it (0 hits), which is why the
  game's own timing survives HLE. `core/clock.c` (`shb_clock_tick` from `hud_frame`, `shb_dt_frame`,
  `shb_frames`).
- **The rate must be read, not assumed.** Fritchii implements `CNTFRQ_EL0` = 31 250 000 Hz; with the console's
  19.2 MHz the module's clock ran **+62.9%** against the host's wall clock over 10 s. Reading `CNTFRQ_EL0` and
  dividing by it: **-0.06%** over 10 s, `g_clock_dbg[1]` 17.8-25.3 ms/frame while the emulator presented ~44-48 fps.
- Verified live: stamina set to 10 and sampled twice — `38.33 -> 74.53` in 1.50 s = **24.12/s** with
  `REGEN_PER_SEC 24`, while the module reported 41 fps. The old frame-based code would have given 16.4/s there.
- Left in frames on purpose: the input sample count of one read, the two-frame stick centring of the L3 sprint,
  `g_same_reads`, and the ocarina/UI animation counters — those are per-read or per-presented-frame facts, not
  durations.
- Unverified: Longshot/ReLink. If a translator ever refuses `mrs CNTPCT_EL0` the value cannot be plausible
  (1..100 ms per frame) and every helper falls back to 1/60 per frame, i.e. exactly the old behaviour.

## Button mapping: the feature and its item side (Nov 24)

- The card was one switch (`ShbConfig.button_mapping`) doing two things, and turning it off only removed the item
  HUD: the app still wrote `bm_remap = 1` and the whole table, so the target, the L3 sprint and the manual shield
  kept working. It is now **two switches**:
  - `button_mapping` — the whole remap. `remap_on()` is `bm_remap == 1 && button_mapping`, so with it off nothing
    is remapped at all; the app writes `bm_remap = 0` and zeroes the options that ride on the table
    (`manual_shield`, `target_r3`, `sprint_l3`, `sprint_l3_toggle`, `auto_target`, `switch_target`, `bm_role_dr`,
    `bm_hold`). The table itself stays in the app's settings file, so turning it back on restores it.
  - `hud_custom_items` (Custom item HUD, `ShbConfig` offset 97, app `f + 81`) — the item side: our own
    B / L / R / ZR buttons (`core/hud.c` `BTNMAP` / `PART_TOT`) and items 1, 2 and 3 assignable. `remap_items()` is
    `button_mapping && hud_custom_items`. Off, `remap_apply()` leaves the press of a button that holds an *item*
    role exactly as pressed (the game's own item button) and every other role is still remapped; the editor does
    not offer the item roles and the game's item HUD is not touched.
- The item logic of `features/button_mapping/` (slot presses, the ZR borrow, the menu assignment) is gated on
  `remap_items()`, not on the master switch; the guide bar hiding and the context-prompt keys follow the remap
  (`core/hud.c` `BMREMAP`).
- **`sizeof(ShbConfig)` is 0x64 both before and after the new byte** (97 bytes rounded to the struct's 4-byte
  alignment), so an installer size check cannot tell them apart: `f + 81` is tail padding on an older module and
  writing 0/1 there is harmless, and an older module reads it as 0 (no item side).
- The `manual-shield` profile used to be `button_mapping = False` with `manual_shield = True`; with the master
  switch off that no longer exists, so it is `button_mapping = True, hud_items = False` (the shield role on L, no
  item buttons of ours).

## Simple / plain HUD row: the counters the player turns off (Nov 24)

- The installer used to force all four blocks on in the simple and plain layouts (`rupees = clock = collectibles =
  True`, positions -1685 / -1435 / -1195 / -965): the switches in the HUD card did nothing there. Each block now
  follows its own switch and the visible ones are packed from `HUD_SIMPLE_X0 = -1685` with `HUD_SIMPLE_ADV =
  (250, 240, 230, 230)` (the room each block takes in that row: rupee 250, clock 240, skull 230), so turning one off
  moves the rest left.
- The soft bar of the simple layout no longer uses fixed pads around the first and last block: the icons and
  texts are measured first (`row_text_w`, digit advance at the current text scale), each icon sits a fixed
  30 layout units left of its own number (`row_ext` records what is drawn) and the bar is placed over those
  extents plus 34 + `g_bg_tune[1]` each side. With the old formula a short number left its rupee icon far
  away and a long one (a clock time) ran past the end of the bar. `g_bg_tune[0]` is now a 0-centred shift
  (it was -165, a compensation for the old formula).
- The block switches drive `show_r` / `show_c` / `show_s` / `show_h`, which hide the text, icons and plates;
  the wide layout keeps its per-block plates and the tuned icon offsets.
- Diagnostic: `g_hud_bg_dbg[0..3]` = the bar's x0, x1 and the first and last visible block x.

## Held L and R, wait or no wait, and buttons with no hold (Nov 24)

- `ShbConfig` grows by three bytes after `hud_custom_items` (struct 97): `bm_hold_lr[2]` (98 / 99, the long press of
  L and R) and `bm_hold_wait` (100, one bit per held button: 0 DR, 1 A, 2 B, 3 X, 4 Y, 5 L, 6 R). **`sizeof` is 0x68
  now** (101 bytes, rounded to the struct's 4-byte alignment; it was 0x64 for 98), so the installer's size gate for
  these bytes is `>= 0x66` and an older module (0x64) is simply not written there. The app's `f + 82..84` are those
  bytes (`f` = struct offset - 16).
- Wait (bit 1) is the old behaviour of the arrow generalised: the short press is cleared from the read while it may
  still be a hold and replayed as a 3-read pulse if released early. No wait (bit 0) is the old behaviour of A / B / X / Y.
  The arrow used to wait unconditionally and that is what it is again: bit 0 of the byte is not a choice of the
  player's (both apps force it in `hold_wait_byte`, and the D-pad right row offers no checkbox, only a note), because
  its short press is the game's Navi call. A config that still reads `bm_hold_wait == 0` lets Navi answer at once and
  the hold come after, which is why the byte is forced rather than defaulted.
- The Rust window's D-pad right key is painted (a cross with the arrow in its right arm): the "▶" character is not in
  the window's fonts and came out as an empty box where the button should be.
- Verified over the debug server (L short = camera with Back held, wait on; R short = sword with jump held, wait
  off): a 6-frame tap of L bumps `g_remap_dr[0]` (replayed), a 6-frame tap of R does not; a 70-frame hold of either
  bumps its own `g_remap_hold[]` slot (5 and 6).
- `remap_hold_role()` returns none when the button's own role is shield / target / sprint / selector / armory / an
  item, and when the hold role itself is one of those (the arrow is exempt). The app enforces the same in
  `normalize_roles()`, so the table, the editor and the module agree; the module check is the backstop for a config
  written by hand.
