# SHB-OcarinaTweaks 0.10.0 (Super Hero Boost)

Everything since 0.9.3. Install 0.10.0 and **rebuild the game once** on Longshot, Hookshot, ReLink or Rekarina: the
module changed (its configuration block grew to 0x68 bytes, with the hold of L and R and a wait bit per held button).

If you only want the new HUD and controls and play in an emulator, installing over 0.9.3 is enough.

**A new app, in Rust**

SHB-OcarinaTweaks is a Rust program now: **one executable** that opens the window when you double-click it and is the
command line when you give it a command (`install`, `uninstall`, `show`, `patch`, `lang`, `game`, `rebuild`, `play`,
`update`). It needs no Python and no Tk, and it opens **no terminal window** behind it. It carries the mod's files
inside it (27 MB), so it installs with nothing else on the disk; a `payload/` folder beside the program still wins,
which is handy when you want to try a different module.

It is a clone of the Python app, not a new behaviour: the same pages, the same profiles, the same previews, the same
texts in six languages, and `app/tools/parity_check.py` holds it to the Python app byte for byte (the patched modules,
the installed folders, a recompiled game and a Rekarina install). The Python app is now `app-legacy/` in the
repository: it keeps working, it is what the Linux and macOS downloads still are, and both apps read and write the same
settings file.

It also **compiles the game itself**, where the old app ran its own scripts:

- **Longshot, Hookshot and ReLink**: it finds the builder your launcher installed, works out the game's DLSS / FSR /
  XeSS options from the launcher's files, and drives the builder with a progress bar and a Cancel button.
- **Rekarina**: it runs Rekarina's own lift and compile, and does the app's part (the ExeFS, the patch tables, the
  extra `main` seeds, the expected hashes, the base check) itself.

The four actions are the same (Install, Apply settings, Recompile, Reinstall and recompile), and a failed install now
**leaves nothing behind**: the game goes back to what it was. Every action ends in a window that says whether it
worked or failed and why, instead of finishing in silence.

The install page checks the folders once and keeps the answer: it says at once when a folder is not an emulator, not a
recompiled game, or a Rekarina without its `setup/`, and the actions stay disabled until it is right. It looks again
when the folder changes or an action ends, not on every frame (that could hash a 350 MB executable while you moved the
mouse).

The profiles apply when you click them, and only what the profile decides: the stamina display (ring or bar), the R3
target, the L3 sprint, the button style, the layout, the language and the HUD sizes stay yours.

**Controls**

- **A long press on L and R too.** Every button that can be held has a row: A, B, X, Y, L and R, plus the D-pad right.
  A hold can be Jump, Sword, Action, Center camera or Back.
- **No hold where the button already acts held.** A button whose own role is the shield, the target, the sprint, the
  item selector, the armory or an item has no long press, because those roles act while held or at once; the row says
  why and is greyed out. The D-pad right keeps its hold: its short press is Navi's call, not a role of ours.
- **Wait or no wait, per held button.** *Skip the tap action when held* decides how a press is told apart. On: the tap
  waits about a third of a second to know it is not a hold, so every tap is a little late and a hold never also fires
  the tap. Off: the tap acts at once and the hold comes on top. The arrow waits by default (a hold does not call Navi);
  the face buttons, L and R act at once, as they always did.
- **Shortcut roles may repeat.** Jump, Sword, the item selector, the Armory and Back can sit on two buttons (jump on X
  and on R, for instance), and the HUD draws each one on its first button. The Controls card also lists, in amber, the
  game's own actions that **no** button has (jump, sword, action, target), or says that all of them have one.
- **Back moved to the held X** in the **Action at the bottom** and **Triggers** tables: the held B is blocked there (B
  is the action or the item selector). Applying a preset clears the wait of the button it holds, so the normal action
  fires and then the hold.
- **Button mapping off turns its whole card off.** The manual shield, the R3 target, the L3 sprint and the targeting
  assists kept working with the mapping off; they follow it now.
- **The item HUD is its own option.** *Custom item HUD* adds our B, L, R and ZR item buttons and lets items 1-3 sit on
  remapped buttons. Off, the rest of the remap still works, a button with an item role keeps the game's own press, and
  neither your item HUD nor the game's item logic is touched.

**HUD**

- **Hiding the corner minimap works.** The option used to zero the scale of the map's panes, and the map screen draws
  with those same panes: opening the map showed nothing. The corner map is now moved out of the screen while you play
  and put back when a menu covers the game.
- **The interaction guide bar is hidden** while the button-mapping HUD is on: its container moves off screen, nothing
  of its scale or transparency is written.
- **The simple and plain rows pack.** Each counter follows its own switch and the visible ones pack from the left; the
  soft bar is sized from what is drawn, and every icon sits right before its number (a long clock no longer ran past
  the bar, a short rupee count no longer left its icon far away).

**Difficulty**

- **The multipliers go up to ×6.** Enemy health and damage taken now run ×1, ×1.5, ×2, ×2.5, ×3, ×4, ×5 and ×6, in
  both apps: the module scales from the percentage, so the ladder was the only limit.

**Module**

- **Durations are real seconds now.** Every duration used to be a number of frames at a fixed 1/60 s, which is only
  right when the game runs at 60. On a machine that runs it faster or slower everything timed by the mod moved by that
  factor. The sprint drain, the action costs and the recharge (0.8 s idle, 1.8 s in combat, 2.5 s after exhaustion)
  last what they say whatever the frame rate (measured live in the emulator: 24/s of refill at 41 and at 50 fps, where
  counting frames gave 16 and 20). A long press is 0.3 s, and so are the target-switch hold, the pause before the ZR
  item goes back into the box, the item-set check after loading a save and the 5 s "in combat" window.
- **The clock is the console's own.** The module reads the game's counter (`CNTPCT_EL0`, the one `sdk` uses for
  `nn::os::GetSystemTick`) and its rate (`CNTFRQ_EL0`): no new import, no file, nothing to enable. If a target ever
  reports an implausible frame time the mod falls back to 1/60 per frame, so it cannot end up worse than before.
- **No file is written into your save.** Some builds left a `shbdebug.bin` diagnostic next to every save the mod
  touched, written from the input hook, on a timer, inside the game's own save-commit window. It now exists only in a
  test build (`build_core.py --debug-dump`): the release module creates no files at all.
- **It checks what it reads from the game.** The HUD layout hook checks the layout before it copies anything (a layout
  that does not look like ours gets the engine's own config back), the save path is checked before it is read, and the
  button-swap lookup runs every fourth frame instead of every frame.
- **The ZR button keeps its item.** Using an item changed the number on another item's button; and an item used from a
  slot (the slingshot, the bow, a bomb) is no longer put away by the game right after the use: it stays until ZR or
  another slot takes the box.
- **The context prompt has one writer.** A second writer fought the first on the same pane (a "Talk" prompt came out
  with the wrong glyph); the prompt's keys follow the remap from one place.

**Not changed**

- The 60 fps numbers: at 60 fps the same thresholds, the same recharge and the same costs as before. The gameplay, the
  profiles and the configuration format are untouched, so an existing setup keeps working, and a module older than this
  one simply leaves the new controls (the hold on L and R, the wait bits) at zero.
