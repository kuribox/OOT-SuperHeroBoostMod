#include "remap.h"
#include "config.h"
#include "hid.h"

/* Button remap: see remap.h. Plain arrays only (no pointer tables: the module's data is not relocated). */

const u64 REMAP_BIT[PB_N] = {BTN_L, BTN_X, BTN_R, BTN_Y, BTN_A, BTN_ZR, BTN_B, BTN_ZL, BTN_L3, BTN_R3, 0x4000ull};
#define REMAP_ALL (BTN_L | BTN_X | BTN_R | BTN_Y | BTN_A | BTN_ZR | BTN_B | BTN_ZL | BTN_L3 | BTN_R3)

int hud_button_swap(void);   /* core/hud.c: the game's Jump/B swap option is on */

int remap_on(void) { return g_shbcfg.bm_remap == 1; }
int remap_l_selector(void) { return !remap_on() && g_shbcfg.bm_l_mode != 0; }
/* (the HUD places its button parts by remap_role_phys, which knows the short roles only: a hold role has no HUD art) */
static int hold_has(u32 role) {
    for (u32 p = 0; p < PB_N; p++) if (remap_hold_role(p) == role) return 1;
    return 0;
}
int remap_selector_on(void) {
    if (remap_on()) return remap_role_phys(ROLE_SELECTOR) != PB_NONE || hold_has(ROLE_SELECTOR);
    return g_shbcfg.bm_l_mode == 2;
}

/* The classic layout: every role on its own virtual button; the swap puts item 1 on X and jump on B. The options
 * of the old app (manual shield on L, L as the item selector, L3 sprint) are roles too. */
static u32 classic_phys(u32 role) {
    int sw = hud_button_swap();
    switch (role) {
    case ROLE_JUMP:      return sw ? PB_B : PB_X;
    case ROLE_SWORD:     return PB_Y;
    case ROLE_ACTION:    return PB_A;
    case ROLE_TARGET:    return PB_ZL;
    case ROLE_ITEM1:     return sw ? PB_X : PB_B;
    case ROLE_ITEM2:     return PB_L;
    case ROLE_ITEM3:     return PB_R;
    case ROLE_ITEM_MAIN: return PB_ZR;
    case ROLE_CAMERA:    return PB_R3;
    case ROLE_SPRINT:    return g_shbcfg.sprint_l3 ? PB_L3 : PB_NONE;
    case ROLE_SHIELD:    return g_shbcfg.manual_shield ? PB_L : PB_NONE;
    case ROLE_SELECTOR:  return g_shbcfg.bm_l_mode == 2 ? PB_L : PB_NONE;
    default:             return PB_NONE;
    }
}

u32 remap_role_of(u32 p) {
    if (p == PB_DR) return g_shbcfg.bm_role_dr;
    return p < PB_DR ? g_shbcfg.bm_role[p] : ROLE_NONE;
}

u32 remap_hold_role(u32 p) {
    switch (p) {
    case PB_DR: return g_shbcfg.bm_role_dr;
    case PB_A:  return g_shbcfg.bm_hold[0];
    case PB_B:  return g_shbcfg.bm_hold[1];
    case PB_X:  return g_shbcfg.bm_hold[2];
    case PB_Y:  return g_shbcfg.bm_hold[3];
    default:    return ROLE_NONE;
    }
}

u32 remap_role_phys(u32 role) {
    if (role == ROLE_NONE || role >= ROLE_N) return PB_NONE;
    if (!remap_on()) return classic_phys(role);
    for (u32 p = 0; p < PB_N; p++) if (remap_role_of(p) == role) return p;
    return PB_NONE;
}

u64 remap_role_virt(u32 role) {
    int sw = hud_button_swap();
    switch (role) {
    case ROLE_JUMP:      return sw ? BTN_B : BTN_X;
    case ROLE_SWORD:     return BTN_Y;
    case ROLE_ACTION:    return BTN_A;
    case ROLE_TARGET:    return BTN_ZL;
    case ROLE_ITEM1:     return sw ? BTN_X : BTN_B;
    case ROLE_ITEM2:     return BTN_L;
    case ROLE_ITEM3:     return BTN_R;
    case ROLE_ITEM_MAIN: return BTN_ZR;
    case ROLE_CAMERA:    return BTN_R3;
    case ROLE_SPRINT:    return BTN_L3;
    case ROLE_SHIELD:    return g_shbcfg.manual_shield ? (remap_on() ? VBTN_SHIELD : BTN_L) : 0;
    case ROLE_SELECTOR:  return remap_on() ? VBTN_SELECTOR : BTN_L;
    case ROLE_ARMORY:    return VBTN_ARMORY;
    case ROLE_BACK:      return VBTN_BACK;
    default:             return 0;
    }
}

u64 remap_apply(u64 b) {
    if (!remap_on()) return b;
    u64 out = b & ~(REMAP_ALL | VBTN_SHIELD | VBTN_ARMORY | VBTN_SELECTOR | VBTN_BACK);
    for (u32 p = 0; p < PB_DR; p++)   /* (PB_DR is the long press, button_mapping.c) */
        if (b & REMAP_BIT[p]) out |= remap_role_virt(g_shbcfg.bm_role[p]);
    return out;
}

/* Menu buttons usable for assigning: X / Y / L / R (A puts the item in the game's box, B is back, ZL / ZR change
 * tabs). First every slot keeps its own button when it is one of those, then the others take a free one: their
 * classic button first (X, L, R), then X, L, R, Y in that order. */
static int menu_usable(u32 p) { return p == PB_X || p == PB_Y || p == PB_L || p == PB_R; }

void remap_menu_buttons(u32 out[3]) {
    static const u32 SLOT_ROLE[3] = {ROLE_ITEM1, ROLE_ITEM2, ROLE_ITEM3};
    static const u32 CLASSIC[3] = {PB_X, PB_L, PB_R};
    static const u32 ORDER[4] = {PB_X, PB_L, PB_R, PB_Y};
    u32 taken = 0, phys[3];
    for (u32 s = 0; s < 3; s++) {
        phys[s] = remap_role_phys(SLOT_ROLE[s]);
        out[s] = PB_NONE;
        if (phys[s] != PB_NONE && menu_usable(phys[s]) && !(taken & (1u << phys[s]))) {
            out[s] = phys[s];
            taken |= 1u << phys[s];
        }
    }
    for (u32 s = 0; s < 3; s++) {
        if (out[s] != PB_NONE || phys[s] == PB_NONE) continue;
        u32 pick = CLASSIC[s];
        for (u32 i = 0; (taken & (1u << pick)) && i < 4; i++) pick = ORDER[i];
        if (taken & (1u << pick)) continue;   /* (cannot happen: three slots, four buttons) */
        out[s] = pick;
        taken |= 1u << pick;
    }
}
