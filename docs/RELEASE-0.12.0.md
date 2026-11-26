# SHB-OcarinaTweaks 0.12.0 (Super Hero Boost)

Everything since 0.11.0. This is a small release: one fix in the Rust window and the groundwork for a live settings
menu that is **not part of it yet**. The module was rebuilt (a few HUD routines changed), so install 0.12.0 and
**rebuild the game once** on Longshot, Hookshot, ReLink or Rekarina; an emulator profile needs a reinstall. The
configuration block is still 0x68 bytes, so an older app keeps working.

**Fixes**

- **The item selector stays in the Rust window's role list when the item HUD is off.** The window grouped the selector
  with the item roles, so turning the custom item HUD off took *Item selector* out of every button's dropdown. The
  Python app never did that. The list now matches it: the selector follows the button remap, the item roles follow the
  item HUD, and a button keeps the role it already has so a saved table is never rewritten.
- **The HUD's item side can be switched off while the game runs.** The module now remembers how many item-button parts
  it built, hides them at once when the item side is off and gives the game's own item box back, and puts away the
  rupee / clock / collectible plates when their switches or positions change. Nothing changes for a normal install
  (the settings are read when the HUD is built, as before); it is what the in-game menu below stands on.

**Not in this release: the in-game menu (emulator test builds only)**

`-` and `R3` together open a settings menu drawn by the module (the button table with each role's options, the HUD and
the gameplay), which edits the configuration live and saves itself to the game's save volume. It is proven in the
emulator only and is **not** in the module this release ships: Longshot, ReLink and Rekarina do not use the driver's GPU
layer it draws through. It will be offered when it has been proven there.

**Known**

- With the item HUD off, the module still counts the item selector as an item role, so the option is listed but does
  nothing in game; honouring it there is a separate change.

**Downloads.** `SHB-OcarinaTweaks.exe` for Windows, `SHB-OcarinaTweaks-linux` and `SHB-OcarinaTweaks-macos.zip`: the
same Rust app (the macOS binary is not signed — right-click > Open the first time; on Linux, `chmod +x` it). The
Python app is published next to it as `SHB-OcarinaTweaks-legacy.exe`, `SHB-OcarinaTweaks-legacy-linux` and
`SHB-OcarinaTweaks-legacy-macos.dmg`. `SuperHeroBoost-0.12.0-presets.zip` installs the mod without an app.
