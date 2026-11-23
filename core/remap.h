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

enum { PB_L, PB_X, PB_R, PB_Y, PB_A, PB_ZR, PB_B, PB_ZL, PB_L3, PB_R3, PB_DR, PB_N, PB_NONE = 0xFF };
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
    ROLE_ARMORY,     /* VBTN_ARMORY: held, the stick / D-pad changes sword and shield (features/quick_select) */
    ROLE_BACK,       /* VBTN_BACK: the game's own B (let go of a ledge, cancel ...) without using the B item */
    ROLE_N
};

/* A HID bit no controller sets in this game (the Joy-Con SL of the left one, unused with a full controller):
 * the shield role's press, read by the manual shield only. */
#define VBTN_SHIELD (1ull << 24)
/* The armory role's press (the left Joy-Con's SR, also unused with a full controller): read and stripped by
 * features/quick_select, the last input handler. */
#define VBTN_ARMORY (1ull << 25)
/* The item selector role's press (the right Joy-Con's SL, unused with a full controller): with the remap the selector
 * has its own virtual button, so it no longer shares L with item 2. features/button_mapping turns it into the game's R
 * (which opens the selector for ZR) and strips it. Without the remap the selector is still L (bm_l_mode = 2). */
#define VBTN_SELECTOR (1ull << 26)
/* The back role's press (the right Joy-Con's SR, unused with a full controller): features/button_mapping turns it into
 * the game's B (the item-1 button) after its own item logic, and only where that press is not an item use: while Link
 * is busy with something else (hanging, climbing, swimming ...) or when the B slot holds no item. */
#define VBTN_BACK (1ull << 27)
/* The item selector is on: remap_on() and a button holds the role, or the old L selector (bm_l_mode = 2). */
int remap_selector_on(void);
/* The old L-as-selector mode: L itself is the selector (no remap): L holds no item, its HUD part is hidden. */
int remap_l_selector(void);

/* HID bit of each physical button, in PB_ order */
extern const u64 REMAP_BIT[PB_N];

/* D-pad right (PB_DR) is the game's Navi call, so it is not a plain remappable button: a short press keeps calling
 * Navi, and only a long press does the role in ShbConfig.bm_role_dr (the app offers the roles that need no HUD art,
 * armory and the item selector above all). The press is held back until it is known which one it is: released
 * before REMAP_DR_HOLD HUD frames it reaches the game as a short pulse, held longer it never does. */
#define REMAP_DR_HOLD 20u
u32 remap_role_of(u32 phys);        /* the role on a physical button (bm_role_dr for PB_DR) */

/* A, B, X, Y and D-pad right (PB_DR) can also have a role for the LONG press, in ShbConfig.bm_hold (A, B, X, Y) and
 * bm_role_dr. A button with a hold role is held back until it is known which press it is (REMAP_DR_HOLD HUD frames):
 * released before, its short role goes to the game as a short pulse; held longer, the hold role is pressed and the
 * short one never is. 0 = the button has no hold role. */
u32 remap_hold_role(u32 phys);
int remap_on(void);                 /* the player's own table is in use */
u32 remap_role_phys(u32 role);      /* the physical button holding a role, PB_NONE if none */
u64 remap_role_virt(u32 role);      /* the virtual (game) button of a role */
u64 remap_apply(u64 buttons);       /* physical -> virtual buttons (only the remapped bits change) */
/* Pause menu, Items tab: the physical button that assigns to each of the item slots B / L / R (PB_*), PB_NONE
 * when the slot has no button. Menu A / B / ZL / ZR / + / - are the game's own, so a slot on one of those is
 * assigned with a free one of X / Y / L / R instead. */
void remap_menu_buttons(u32 out[3]);

#endif
