# Button mapping (buttonMapping): design and notes

Super Hero Boost's own module. It adds **B, L, R and ZR** item buttons to the HUD and lets the player assign items to
them from the pause menu. It uses no third-party mod: the module, the button art and the textures are generated in this
project.

## Features

| Feature | How it works |
|---|---|
| Extra item buttons B / L / R / ZR | the slot's item is equipped into the game's item box (item index forced on the `main+0x1211C00` call) and ZR is held while the button is held |
| Button styles | three texture sets: Switch (`OotBtnS_*`), Xbox (`OotBtnX_*`) and PlayStation (`OotBtnP_*`); style byte 0 / 1 / 2. They only change the button mapping's own pictures |
| Button positions | `bm_layout` (config offset 25): 0 Nintendo, 1 A/B swapped, 2 Xbox (A/B and X/Y swapped), 3 Switch GameCube (A center, B lower left, X upper right, Y above-left). Select **Switch** under Button style to pair it with the monochrome Switch button art. `face_place()` in `core/hud.c` moves where each face button (and its item, label) is drawn on the HUD and the pause menu; the logical buttons do not change, and the game's Jump/B swap is applied first |
| Game prompts (optional) | a separate app choice (`game_layout`: none / PlayStation / Xbox). Only when it is set, the installer downloads (once, ~70 MB) the game's own button glyphs, External font and controller picture from the GameBanana controller-UI mod and merges its resource size table with ours. Longshot and ReLink already ship their own prompts, so the default is none |
| Extended layout | B, L, R, ZR, X, Y, A and the sword |
| Simple layout | only the assignable B, L, R and ZR buttons; hides X, Y, A and the sword and repositions L / R / ZR in place with B centred and closer, in a zigzag, both on the HUD and on the pause menu's Items tab |
| L mode: item | L is one more item slot |
| L mode: selector | L opens the game's item selector; the L button is hidden in both layouts and the menu legend removes L |
| Pause assignment | Items tab: B / L / R on the highlighted item; an item already in another slot is swapped |
| Jump/B swap | if the game's System > Options **Swap Jump Controls** is on, the B slot moves to **X**: X triggers the item, the icon is drawn over the X button, the X button texture and the menu legend follow, the B button becomes an unlettered jump button, and the **Jump** label moves from X to B |
| Icons | `PaItemBtn_00` parts with the item icon and its shadow, like the native item box |
| Counts | the item buttons show the quantity of consumables (sticks, nuts, bombs, seeds, bombchus, beans and arrows): the box's own `T_ItemNum_00` text is written from GameData `PouchData.<item>.StockNum`, and its pane stays hidden for items without a quantity |

## Button remap (0.9.0)

The player chooses what each of the ten buttons X, Y, A, B, L, R, ZL, ZR, L3 and R3 does (app: Controls > Controller
buttons, with presets: classic, action at the bottom, manual shield, souls). Roles: jump, sword, action, target,
center camera, sprint, shield, item selector, item 1 / 2 / 3, the main item (the game's own item box) or nothing.
Each role is on one button at most, so at most **four** buttons hold an item. Limits: items never on L3 / R3 (they
have no HUD art), the item selector on any button but the sticks (it is the virtual L whichever button holds it), target only on ZL, ZR, L3 or R3, action only on A, B, X or Y
(the game's context prompt next to Link, `DoCommand_00`, draws a face button; the mod shows the one that holds
action there). Menus, dialogues and shops (Link in an `Evt*` state) keep the real buttons.

The old switches are roles now: a shield button turns the manual shield on, a sprint button the L3-style sprint
(its "toggle" option on that row), a selector button L's item-selector mode, and target on R3 replaces the ZL/R3
swap. Switch target and next target are options of the target row. The button positions option (`bm_layout`) is
gone: the table places everything (the installer writes 0).

| Piece | How it works |
|---|---|
| Config | `bm_remap` (struct 79) and `bm_role[10]` (struct 80..89, physical order L X R Y A ZR B ZL L3 R3, `ROLE_*` from `core/remap.h`). The block grows to 0x5C; an older app leaves them 0 = classic layout. The installer also writes the options the table implies (`manual_shield`, `sprint_l3`, `bm_l_mode`, `target_r3` = 0) |
| Input | `button_remap_on_input` (features/button_mapping) runs **before every other input handler** (`FEATURE_INPUT_EARLY` in `tools/build_core.py`). Every remapped bit of a sample is cleared and only the *virtual* buttons of the pressed buttons' roles are set, so a button never does its own native action, only its role. The virtual button of a role is the one it has in the classic layout (camera = R3, sprint = L3, selector = L), so the item slots, manual shield, target options, L3 sprint and the game keep seeing the classic layout |
| Shield | its own virtual bit, `VBTN_SHIELD` (HID bit 24, unused with a full controller): only the manual shield reads it (`ms_guard_bit()`), and button_mapping strips it before the game. L stays an item slot |
| When | while Link plays: the player updated in the last 60 HUD frames (not only `player_active()`, which drops in hit stops), the HUD is up, not paused, not the ocarina, not dead. Menus, the title screen, game over and the ocarina keep the real buttons, which is what the game's own prompts name |
| Jump/B swap | the classic layout keeps it (item 1 on X, jump on B). With a remap the player's table wins |
| HUD | each part is a role (`PART_ROLE` in `core/hud.c`); `sb_xy()` places it on the physical button holding that role (ZL gets a spot mirroring ZR) and its texture is that button's letter. A role on no button (or on a stick) hides its part. Target, camera, sprint, shield and the selector have no HUD part |
| Pause menu | items 1..3 are assigned with their own button when the Items tab allows it (X / Y / L / R); a slot on A / B / ZL / ZR uses a free one of those instead (`remap_menu_buttons()`); A still puts the item in the game's box. The legend is built from the same table with the game's button glyphs |

## D-pad right (long press)

The eleventh button, `PB_DR`, is the D-pad's right arrow. In the game it calls **Navi**, and that stays: a short press
still reaches the game. Only the **long press** is a role, in `ShbConfig.bm_role_dr` (appended after the HUD scales,
struct 92; the struct is 0x60 bytes now, an older app leaves the byte 0 = no role).

- The roles it takes are the ones that need no HUD button: jump, sword, camera, sprint, shield, **item selector** and
  **armory** (`_DR_ROLES` in `app/installer.py`). No item slots, target or action, like L3 / R3.
- `button_remap_on_input` holds the press back: released before `REMAP_DR_HOLD` (20) HUD frames it is replayed to
  the game as a three-read pulse (Navi answers a little late, after the release); held longer the arrow is cleared for
  good and the role's virtual button is set for as long as it is held. State is reset whenever the remap is off
  (menus, dialogues, `Evt*`). With no role it passes through untouched.
- Diagnostics: `g_remap_dr[0]` counts replayed short presses, `[1]` long presses that fired the role.
- The app lists the arrow as the last row of Controls, with a note under it. It is not part of the presets: choosing
  one keeps the arrow's role, and the preset buttons ignore it when they decide which preset is active.
- Selector and item 2 share the virtual L, so a selector on the arrow takes L's item 2 away (the table drops it).

## Armory (role)

A button with the **Armory** role (any button) opens the game's own item drawer while it is held (the game sees R
held), listing the swords and shields Link owns and can use at his age (child: Kokiri Sword, Deku and Hylian Shield;
adult: the other swords, Hylian and Mirror Shield) instead of items: the same window, blur and slowed game. The cells
form two rows, swords above and shields below; the stick moves in a row or between the rows, and letting the button go
equips the gear under the cursor, the way the pause menu does (nothing changes when the cursor did not move). The
cursor reopens on the gear chosen last. A clone of the community mod Hyrule Armory Sync (drawer on L), reimplemented in
features/quick_select/armory_drawer.c with call-replacement hooks (`BL_SITES` in tools/hook_sites.py, wrappers in
armory.S) on the drawer and list-build code, plus the drawer's per-frame update (`hook_ad_update`). The sort header
("By type / Sort") still shows: its panes are not found by name after the drawer is built.

- The role's press is its own virtual bit, `VBTN_ARMORY` (HID bit 25, unused with a full controller), read and
  stripped by features/quick_select, the last input handler.
- Equip: only the game's request, `equip_request(pouch, name, 3, 0)` + `equip_refresh(name, 1)`, the pair the pause
  menu's equip handler (main+0x33BA470) uses for gear. The String64 record must **not** be written first:
  `equip_request` compares the name with what Link wears and does nothing when they are equal; it writes
  PlayerStatus.EquipWeapon / EquipShield itself (checked live: the shield model changes at once).
- Owned: `PouchData.<actor>.StockNum > 0` (what the pause menu lists; `hud_pouch_has()`), or IsGet. IsGet alone is not
  enough: it stays off for gear that never played its "you got" scene. The eight gear StockNum indices were added to
  `GD_STOCK` (core/gd_index.h; tools/gen_gd_index.py lists them in `GEAR`).

The swap option is read live from GameData (OptionData struct `0x834d9d9e`, Bool member `0x3B0FC63F`, via
`read_bool` at `main+0x179F70`). It is refreshed every HUD frame and while the pause menu is open, so changing it in
**System > Options** takes effect without reinstalling.

The labels the module draws itself — the jump label, the A fallbacks (`Attack` / `Roll` when the game's control
guide is not up) and the `Assign:` title of the pause menu legend — follow the **game's language** (en / es / pt /
fr / de / it). The installer reads it from the emulator (`[System] language_index`, or `system_language` in
Ryujinx) or from `language.txt` next to a recompiled game, and writes it in config byte 75; when it cannot be read
the app's own language is used. The game's own prompts already come in the game's language, so the HUD stays
consistent. `--lang` on the command line overrides it.

The module is enabled by the `button_mapping` byte in the configuration (`core/config.h`): style at offset 38, simple
layout at 39 and L mode at 40.

## HUD parts

- Four clones of the native item box (`Pa_SelectItem_00`, asset `PaItemBtn_00`) are appended to `PlayerStatus_00` with
  the corrected common descriptor (`+0x40 = 0xFFFF`, `+0x42` = part index). They are created, `find_part` finds them,
  they have all 11 animations (ItemPattern, StockNum, NumMax, NumMin, IsInvalid, Type, ItemChange, Select, UnSelect,
  Decide, Assign) and they draw. `PART_N = 4`, `slots_update()` in `core/hud.c`.
- The icons are textures loaded by name at runtime. `PaItemBtn_00` contains the `Pa_ReplaceIcon_00` part, whose two
  panes (icon + shadow) use the placeholder texture `FBLayout_00_ASTC4x4`. The HUD replaces it with
  `UI/Tex/Icon/<Item_Name>.bntx.zs` and `UI/Tex/IconSh/<name>_Sh`.
- **How the game sets an icon** (native HUD at `main+0x33B7E80`):
  1. it allocates a 0x40-byte icon controller, sets its vtable (`[main+0x4C973B0] + 0x10`) and fields, links it into
     the part's list and zeroes `+0x30` / `+0x38`;
  2. `main+0xA37320(controller, part)` creates the icon and shadow texture replacers at `controller+0x30` / `+0x38`
     (0x58 bytes each, from the engine heap);
  3. `main+0xA73920(controller->+0x30, &"Item_X", 0)` and `main+0xA73920(controller->+0x38, &"Item_X_Sh", 0)`.
- Calling `main+0xA73920` on the object at `nested ReplaceIcon frame + 0x6A8` overflowed the stack (game frozen):
  that object is not a ready replacer. The direct call stays behind `SLOT_ICON_DIRECT 0`.
- **Solution (commit f5ad284 and later):** the controllers are built by the game's own routine
  `main+0x33B7DA0(widget, heap)` (the Agony Stone icon one: it looks for `Pa_ReplaceIcon_00` in `widget+0x20`, builds
  the controller and sets `Item_AgonyStone`). It is called with a stand-in widget per button **during the PlayerStatus
  HUD init** (hook 12 at `main+0x33F5D20`, x0 HUD, x1 heap): that heap only accepts allocations during the init (later
  it returned a null controller and the game crashed). Hook 11 at `main+0xA37320` captures the created controller.
  The icons are then changed at any time with `main+0xA73920` on `controller+0x30` (name) and `+0x38` (`name_Sh`).

## Item index -> icon

- `main+0x592B20(manager, &name, item)` returns the item's actor name, which is also its icon texture name.
  `manager = **(main+0x4C7E0D0)`, default name `*(main+0x4C7DAD0)` (the HUD does the same before setting its icon).
- Item index = index of the equip call `main+0x1211C00`. Read live:
  0 Item_DekuStick, 1 Item_DekuNut, 2 Item_Bomb, 3 Item_Boomerang, 4 Weapon_Range_SlingShot, 5 Item_Hookshot,
  6 Item_Longshot, 7 Item_Hammer, 9 Item_Bombchu, 10 Item_DinFire, 11 Item_FaroreWind, 12 Item_NayruLove,
  13 Item_MagicBean, 14-17, 50 Item_Bottle_<content>, 18 Item_FishingRod, 30 Weapon_Range_Bow, 31 Item_WeirdEgg ..
  44 Item_ClaimCheck (trade items), 46 Item_Arrow_Fire, 47 Item_Arrow_Ice, 48 Item_Arrow_Light
  (8, 19-29, 45: empty in this save).
- **Bottles**: the five bottle cells are item indices **14, 15, 16, 17 and 50** (grid order BottleA..BottleE), not
  normal items. Their contents are the Enum
  `Shop_SelectBottleContentItem` (`Empty, RedPotion, GreenPotion, BluePotion, BlueFire, Fish, Letter, BigPoe, Poe,
  Bug, Fairy, MaxMilk, HalfMilk`); the five content records in GameData are the unnamed Enum indices **60, 61, 62,
  63, 64** for BottleA..BottleE (confirmed live: assigning cells 14-17 sets the box to BottleA..D, and cell 50 sets
  it to BottleE). The HUD reads the slot and draws `Item_Bottle_<content>` (`Empty` -> `Item_Bottle`), so the button
  shows the bottle that was assigned even though the item index itself resolves to the current bottle (e.g.
  `Item_Bottle_RedPotion`).
- **ZR** is the game's own item box (`PlayerStatus.AssignableItem.<age>`, an Enum: `BottleA..BottleE` when it holds
  a bottle). Its own HUD icon name is stale for the 2nd..5th bottle, so the HUD takes the box's `BottleA..E` value,
  reads the matching content record (60..64) and draws the same `Item_Bottle_<content>` icon.
- `ItemPattern` is the **framing** of the icon inside the button (frame 0 draws it oversized), not the item choice;
  the measured item -> frame table is applied per item (`item_pattern()` in `core/hud.c`). `ItemChange` is the icon's
  pop-in animation (frame 0 = hidden). `Type` changes nothing visible.

## Textures

`tools/make_buttons.py` draws the three button sets with shapes and text, without game material. Each style produces
its own `OotBtn*_*` textures and the installer copies only the set for the chosen style.

The game's own prompts (the `OcarinaButton*_00` and `KeyTexFont*_00` textures inside `Common.Product.100...bntx`,
the External font glyphs and the controller picture) are game data and are not drawn here. For PlayStation or Xbox
the app downloads the community controller-UI archive from GameBanana on demand and caches it; the Switch style keeps
the game's files. Its `ResourceSizeTable` raises the External font entry (`0x3A08` -> `0x5000`), which is why it is
merged with ours instead of replacing it: a single table sizes every resource, so copying theirs would drop our bigger
Common layout budget and the enemy-pack sizes.

### Switch GameCube artwork

Select **Switch** for button style, **Switch GameCube** for button positions and **Extended** for the full
face cluster. The monochrome A and B are different sizes, with a vertical curved X and a horizontal curved Y.
The text-only Jump and action variants use the same button outlines.

Once the standard payload has been staged, use the game's UI zstd dictionary (dictionary ID 1) to generate the
extra textures before running `tools/build_app.py`:

```powershell
py -3.12 -m pip install Pillow zstandard
py -3.12 tools/make_gamecube_buttons.py --dictionary "C:\path\to\zsdic1.bin"
```

**Nintendo (default)** is the original button placement. Selecting **Switch GameCube** applies the
GameCube diagram on the System tab's **Basic Controls** screen the next time you install or apply
settings, as long as the game's original UI archive can be read. Nothing else depends on it: when the
archive is not available the install still completes and the app reports the diagram as skipped.
Other button positions use the existing **Game button prompts** setting, which is **None**
by default. The diagram does not change the controller bindings.

Longshot reads the original UI archive from the game folder's `romfs`. For an emulator or a separate
ROMFS dump, set `SHB_ROMFS` to the folder containing `UI/Layout2Archive` before launching the installer.
The installer patches a copy of the supported original atlas. It verifies the original texture hashes
before applying the three replacements. The full game atlas and the game's dictionary are not in this
repository. Recognized older standalone GameCube diagram files are renamed with a `.disabled` suffix
when applying settings so they don't override the new selection. Move any custom diagram add-on out of
the mods folder if it also replaces the atlas.

Release builds stage the GameCube HUD textures in `tools/make_release.py`. Set `SHB_ROMFS` to your own
dump to read the UI dictionary from `Pack/ZsDic.pack.zs`, or point `SHB_UI_DICTIONARY` at the extracted
dictionary.
