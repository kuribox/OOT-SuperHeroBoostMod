# SHB-OcarinaTweaks 0.9.3 (Super Hero Boost)


Everything since 0.9.2. Install 0.9.3 and **rebuild the game once** on Longshot, Hookshot, ReLink or Rekarina: the
module changed.

**Controls**

- **Two actions on A, B, X and Y: tap and hold.** Every one of those four buttons has a second row in Controls, marked
  with a down arrow, for what it does when you **hold** it (about a third of a second). The short press acts **at
  once**, with no delay; when the hold fires, the short role lets go. Pick a hold role that does not mind the short one
  having happened (Back, Armory, the selector, the camera ...). It takes jump, sword, action, camera, sprint, shield,
  selector, armory and Back; not items or target. A preset only sets the hold rows it names (see Back below).
- **New role: Back (the game's B).** The game's own B: let go of a ledge, cancel, get off ... With the remap those
  actions went with the button that held Item 1. Back works on any button and in a hold, and it is not an item use: it
  acts while Link is busy with something else (hanging, climbing, swimming ...) or when the B slot holds no item, so
  it never fires an item by accident.
- **Back is in the presets that need it.** The **Triggers** and **Action at the bottom** tables (the first one is the
  base of Recommended, Recommended without beta enemies, Super Hero, Ultra Hero and Beta All) have no item on the B
  button, so **holding B** is now the Back role there. Choosing a preset sets only the hold rows it names: your other
  hold rows stay.

**Sprint (toggle)**

- **It lets go when you stop.** The toggled sprint now ends by itself when you stop moving (a toggle pressed standing
  still waits for your first step).
- **No more thrown objects.** The sprint pressed A for you, and that A threw the jar you had just lifted, again on every
  pickup while it stayed on. It no longer presses A while you lift or carry something; the sprint speed still applies,
  so you can run with the object.

**Fixes**

- **The manual shield threw lifted jars by itself.** With the manual shield on, the module cleared two of the player's
  guard flags every frame, and while Link carried something that made the game throw the object a moment after the
  lift (found live in the emulator: with the option off the jar stayed in his hands). It leaves those flags alone
  while Link carries something.
- **The app opens with the stamina off.** The stamina page crashed with a `NameError` when the system was off.
- **PlayStation button prompts in the whole game.** Installing the PlayStation layout copied the controller picture of
  the menu but not the font that draws the button glyphs of every text and prompt, so the menu showed a DualSense and
  the rest of the game kept the Switch buttons. The app now installs both (the font is built with the payload).

Known gaps: the hold roles, Back and the sprint changes were checked in the emulator (counters and a lifted jar), not
yet on the three recompilations; Back was not tried hanging from a real ledge. The hold delay is a fixed 20 HUD frames.
