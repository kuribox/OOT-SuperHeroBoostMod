# SHB-OcarinaTweaks 0.9.2 (Super Hero Boost)


A fix for the area counter of the HUD. Install 0.9.2 and **rebuild the game once** on Longshot, Hookshot, ReLink or
Rekarina: the module changed.

- **Heart pieces in chests count now.** The heart counter knew the pieces lying in the world and the rewards, but
  not the chests that hold one, so an area whose only piece was a chest read 0/0 even after you opened it (found in
  Zora's Domain). The ten chests with a heart piece (Gerudo Fortress and Valley, Graveyard, Dampe's race, Hyrule
  Field, the Market minigame, Zora's Domain) are in the total and count once opened.

Known gaps: checked in the emulator in Zora's Domain (child); the other nine chests, and the three recompilations, were not
played. Chests whose reward is decided by an event, not by the map data, are not in the total.
