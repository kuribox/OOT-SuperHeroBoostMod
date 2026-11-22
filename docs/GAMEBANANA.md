# GameBanana listing

**Title:** Super Hero Boost - Stamina, HUD & item buttons

**Tagline:** Stamina, Hero Mode difficulty, HUD layouts, item buttons and quick select, all configurable from the app.

**Alternative taglines**

- Stamina, Hero Mode scaling, an always-visible HUD and your own item buttons.
- A complete gameplay and HUD overhaul you can configure without touching a single file.

## Summary

Super Hero Boost is a gameplay and HUD module for the *Ocarina of Time* remake (update **v131072**). Stamina,
difficulty, the HUD and the extra buttons are switches in a companion app, and the installer patches the module with
the profile you pick: no recompiling, no manual file editing. The optional world changes and beta mods are generated
from your own game files.

## Longshot / Hookshot notice

There has been a lot of feedback about Longshot problems since 0.4.0: crashes while loading a save or an
area, a rebuild that stopped near 90% ("Longshot is still running from this folder"), DLSS disappearing
after a rebuild, and the app picking a ReLink or Hookshot builder for a Longshot game. 0.5.0 fixed those; 0.8.0 includes them, the ReLink support, the new controls and enemy options, the button positions, the game prompts, the souls-like targeting and the item button work: the module no longer relies on condition flags across calls, DLSS/FSR are kept, the right builder is found on Windows/Linux/macOS, ReLink is supported and the app closes the running game. Rebuild the game once.

## Features

**Stamina**
- Four styles: hardcore (every action costs), combat only, sprint only, or advanced (choose which actions cost).
- Exhaustion and regeneration, with individual costs for rolling, jumping, attacking, shooting, sprinting and
  blocking with the shield.
- Stamina ring next to Link, or a segmented bar under the magic gauge (yellow, red while recharging).

**Difficulty**
- Enemy health and damage taken multipliers (x1 to x3).
- No heart drops: hearts become green rupees.

**Movement**
- Configurable sprint speed (+25% by default).

**HUD**
- Always-visible rupee counter and clock (off / 12 h / 24 h) with the game's native plate look.
- Per-area counter for Gold Skulltulas and heart pieces. It reads the save's persistent key set, so the count
  survives leaving and re-entering an area.
- Three layouts: **Wide** (four plates), **Simple** (the four counters at the bottom, no background) and **Minimal** (rupees and time above the minimap,
  heart pieces next to the native Skulltula counter on the Minus map).

**Buttons**
- B, L, R and ZR item buttons on the HUD, assignable from the pause menu's Items tab by pressing X / L / R.
- Switch, PlayStation and Xbox button styles, applied to the HUD and to the game's own prompts. PlayStation and Xbox
  download their archive once (~70 MB) from the GameBanana controller-UI mod; Switch needs nothing.
- Extended layout (B, L, R, ZR, X, Y, A and the sword) or simple layout (only the assignable buttons, with B
  centred).
- L as an item button or as the game's own item selector.
- Songbook autoplay: pick a learned song in the native songbook (ZL) and Link plays it with its effect.

**Controls**
- Target with R3: swaps ZL and R3 (R3 targets, ZL resets the camera); set Targeting Type to Toggle.
- Switch target: tap the target button to lock the enemy the game marks in red, hold it to drop the target.
- Next target: when the locked enemy dies, the target moves to the next enemy (its own option).
- Hold L3 to sprint with the game's sprint animation, without rolling.
- Manual shield: the target button always reaches the game; L reserved during hit reactions and hit-stop; R3 cancels unless the target-on-R3 swap is on.

**Quick select**
- Hold **D-pad Down** to switch between the **Iron Boots** and **Hover Boots**; hold **D-pad Up** to switch between
  the **Lens of Truth** and the **mask** you were wearing.
- Only items in your inventory are used, and the game's equipment is applied immediately.
- Every profile and every preset includes it.

**World changes**
- The three Great Fairy magic rewards are reordered: Hyrule Castle (child) gives **Nayru's Love**, Death Mountain
  Crater gives the **magic spin attack** and Desert Colossus gives **double magic**.
- **Farore's Wind** warp points can be placed outdoors.

**Profiles**
- Recommended, Souls controls, Souls controls + Hero Mode, Base, Manual Shield, Hero Mode, Hero + Stamina,
  Hero Explorer, Super Hero, Ultra Hero, Survival, World changes and two beta combinations, plus a fully custom setup.

**Beta (optional, generated from your own game files)**
- Enemy duplication (Option A).
- Enemy speed: movement (all base and attack speeds together), animation, attack cadence and projectile speed, each with eleven multipliers from x0.5 to x6 (plus off).

## Download

- **`SuperHeroBoost-0.8.0-presets.zip`** — one zip with **all 14 named profiles of the app**, each ready to install.
  **No executable inside.** SHA-256: $preset.
- The companion app **SuperHeroBoostConfig** is a third-party program, so it belongs in the game's **Tools** section
  (https://gamebanana.com/tools/games/7534), not in the Mods file. Source and binaries are also on the project's
  [Releases page](https://github.com/kuribox/OOT-SuperHeroBoostMod/releases).

Inside the zip: `README.txt` plus one numbered folder per profile (`01 Recommended`, `02 Base`, `03 Hero Mode`, …
`12 Beta - everything`). Every folder contains a `zz-Super-Hero-Boost` folder with the module (patched for that
profile), the IPS patch, the Switch button textures and the optional romfs files that profile uses (Great Fairy,
Farore, no-heart-drop, enemy duplication/speed). Pick one; only one profile can be installed at a time.

## How to use

1. **Requirements**: the game on update **v131072** and an emulator that loads mods from `user/load`. Back up your
   saves; on Windows the app closes the game before installing or rebuilding.
2. Download **`SuperHeroBoost-0.8.0-presets.zip`** and extract it.
3. Open the folder of the profile you want (`01 Recommended`, `02 Base`, …) and copy its
   **`zz-Super-Hero-Boost`** folder into `user/load/0400486024DEC000` (the folder that contains your mods; merge
   it if it already exists). Install one profile at a time.
4. Start the game.
5. In game:
   - **Item buttons**: open the pause menu, go to the **Items** tab, highlight an item and press **X** to put it
     on **B**, **L** on **L** and **R** on **R**. The game's own **ZR** box still works with **A**.
   - **Songbook**: pick a learned song in the native songbook (**ZL**) and Link plays it with its effect.
   - **HUD**: rupees and clock stay visible; under them you get the Gold Skulltula and heart-piece counter of the
     current area.
   - **Stamina**: the ring next to Link (or the bar under the magic gauge, if you pick that in the app) shows what
     is left; it refills when you stop.
6. To change options, use the app: pick a profile or tune each section and press **Install** over the same folder.
   To remove the mod, delete `zz-Super-Hero-Boost`.

The app saves your settings automatically, but changes only reach the game after pressing **Install**.

## Requirements

- The game, update **v131072**.
- An emulator that loads mods from `user/load/0400486024DEC000` (or a custom load folder, which the app also
  detects).
- Back up your save. On Windows the app closes a running game before installing and during a rebuild.

## Compatibility

- **Emulators: tested** on **lichii** and **Fritchii**.
- **Longshot: supported and tested** since 0.2.0. Longshot is a static recompilation, so the module has to be built
  into the game: the app's **Longshot** target rebuilds it and installs the profile and romfs options (25-45 minutes
  the first time; later code changes are incremental and profile/romfs changes need no rebuild). 0.6.0 includes
  the 0.5.0 crash fixes and every ReLink feature; rebuild the game once with 0.6.0.
- **Hookshot: expected to work, not tested yet.** It uses the same builder and `nx2/make_game.py --update`.
- **ReLink: supported.** `bin/relink-game`, `.relink_exefs` and the `RELINK_*` variables. Install/backup tested; the full build has not been verified on a real ReLink install yet.
- Real hardware: not tested.

## Credits

Add these to GameBanana's **Credits** module (the AI software providers are required by the AI content policy):

- **Kuribox** — creator: module, HUD, stamina system, item buttons and companion app —
  https://github.com/kuribox
- **Anthropic (Claude)** — AI software used as an assistant for coding, reverse engineering and testing —
  https://www.anthropic.com
- **DeepSeek** — AI software used as an assistant for coding, reverse engineering and testing —
  https://www.deepseek.com
- **Kabosune / oot-remake-modding** — modding documentation and help —
  https://github.com/Kabosune/oot-remake-modding
- **platinumagpie / oot-remake-devtool** — development tools used during research and testing —
  https://github.com/platinumagpie/oot-remake-devtool

The AI software was used as an assistant; the design, direction, integration and testing are the author's.

## Links

- Source and issues: https://github.com/kuribox/OOT-SuperHeroBoostMod
- App downloads: https://github.com/kuribox/OOT-SuperHeroBoostMod/releases
- Cover art: `docs/portada.png` (1920x1080) and `docs/portada-mini.jpg` (640x360, 16:9, for the listing thumbnail).
