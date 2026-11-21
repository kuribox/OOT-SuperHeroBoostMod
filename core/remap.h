#ifndef SHB_REMAP_H
#define SHB_REMAP_H

/* Button remap (buttonMapping): the player chooses what each of the ten buttons does.
 *
 * Physical buttons are numbered like the HUD's button parts (core/hud.c SB_*), then ZL, L3 and R3. Roles are what
 * a button does. Each role has a "virtual" button: the button that has that role in the default layout, which is
 * the one the game and the rest of the mod understand (X jumps, Y is the sword, A the action, ZL targets, R3
 * centres the camera, L3 sprints, B / L / R the item slots and ZR the game's own item box). remap_apply() turns the
 * buttons the player pressed into the virtual ones before any other feature reads them, so the item slots, the
 * manual shield, the target options and the L3 sprint all keep working on roles, wherever the player put them.
 *
 * The shield role (the manual shield's guard) has a virtual button of its own, VBTN_SHIELD, that only
 * features/manual_shield reads (button_mapping strips it before the game sees it); so L stays an item slot.
 *
 * ShbConfig.bm_remap = 0 keeps the classic layout (the game's Jump/B swap moves item 1 to X, as before);
 * 1 uses ShbConfig.bm_role[physical] = role. At most four buttons hold an item: there are four item roles and
 * each role is on one button at most (the app enforces it, remap_role_phys() takes the first one). */

typedef unsigned int u32;
typedef unsigned long long u64;

enum { PB_L, PB_X, PB_R, PB_Y, PB_A, PB_ZR, PB_B, PB_ZL, PB_L3, PB_R3, PB_N, PB_NONE = 0xFF };
enum {
    ROLE_NONE = 0,
    ROLE_JUMP,       /* virtual X (B with the game's Jump/B swap) */
    ROLE_SWORD,      /* virtual Y */
    ROLE_ACTION,     /* virtual A: context action, roll */
    ROLE_TARGET,     /* virtual ZL */
    ROLE_ITEM1,      /* virtual B (X with the swap): item slot "B" */
    ROLE_ITEM2,      /* virtual L: item slot "L" */
    ROLE_ITEM3,      /* virtual R: item slot "R" */
    ROLE_ITEM_MAIN,  /* virtual ZR: the game's own item box */
    ROLE_CAMERA,     /* virtual R3: the game centres the camera */
    ROLE_SPRINT,     /* virtual L3: the stamina feature's sprint (ShbConfig.sprint_l3, toggle or hold) */
    ROLE_SHIELD,     /* VBTN_SHIELD: the manual shield's guard (ShbConfig.manual_shield) */
    ROLE_SELECTOR,   /* virtual L in the item-selector mode (ShbConfig.bm_l_mode = 2): opens the game's selector */
    ROLE_DRAWER,     /* the sword / shield quick menu (features/sword_shield_menu); the game never sees the button */
    ROLE_N
};

/* A HID bit no controller sets in this game (the Joy-Con SL of the left one, unused with a full controller):
 * the shield role's press, read by the manual shield only. */
#define VBTN_SHIELD (1ull << 24)

/* HID bit of each physical button, in PB_ order */
extern const u64 REMAP_BIT[PB_N];

int remap_on(void);                 /* the player's own table is in use */
u32 remap_role_phys(u32 role);      /* the physical button holding a role, PB_NONE if none */
u64 remap_role_virt(u32 role);      /* the virtual (game) button of a role */
u64 remap_apply(u64 buttons);       /* physical -> virtual buttons (only the remapped bits change) */
/* Pause menu, Items tab: the physical button that assigns to each of the item slots B / L / R (PB_*), PB_NONE
 * when the slot has no button. Menu A / B / ZL / ZR / + / - are the game's own, so a slot on one of those is
 * assigned with a free one of X / Y / L / R instead. */
void remap_menu_buttons(u32 out[3]);

#endif
