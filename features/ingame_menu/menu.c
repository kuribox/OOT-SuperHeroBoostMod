#include "menu.h"
#include "../../core/hid.h"
#include "../../core/config.h"
#include "../../core/remap.h"
#include "../../core/clock.h"

/* The menu's state, its input and its painting. See ingame_menu.c for how the picture reaches the screen.
 *
 * Opening and closing: + and - together, while playing. The two buttons are held back for a few reads so the game
 * never sees the pause or the map when it is a chord (a lone press is replayed to the game afterwards). While the
 * menu is open every button and both sticks are removed from the game's input.
 *
 * Controls: up / down choose a row, left / right (or A) change it, L / R change the page, B or + and - close.
 *
 * The pages mirror the configurator: the button table (a role per button, the long presses, and the options that
 * belong to a role: manual shield / tap to switch / next target under the target, the toggle under the sprint, the
 * wait under a long press), the HUD (the counters, the layout, the scales, the buttons' look), and the gameplay
 * (stamina, difficulty, quick items). A change writes the same bytes the configurator would, including the ones it
 * derives (the HUD's block positions follow what is shown; the sprint / shield flags follow the table). */

typedef unsigned char u8;
typedef unsigned short u16;
typedef signed short s16;

#ifndef SHB_DBG_DUMP
#define SHB_DBG_DUMP 0
#endif

int hud_ingame(void);
int hud_ingame_known(void);
int hud_game_covered(void);
void im_persist_request(void);
int im_persist_state(void);
/* core/hud.c: drops the item assigned to an item slot (0 B, 1 L, 2 R), from every age's set */
void hud_unassign_item(int slot);
void hud_items_off(void);   /* core/hud.c: the game's own item box shows its item again */

#define PANEL_W 352             /* the picture, in font units (one unit = f pixels) */
#define PANEL_H 180
#define VIS_ROWS 10             /* rows shown at once */

/* ---- config access -------------------------------------------------------------------------------------------- */

#define OFF(field) ((int)__builtin_offsetof(ShbConfig, field))
static volatile u8 *b8(int off) { return (volatile u8 *)&g_shbcfg + off; }

/* ---- roles ---------------------------------------------------------------------------------------------------- */

/* Keys of the table: 0..9 the buttons (core/remap.h PB_ order L X R Y A ZR B ZL L3 R3), 10 the D-pad right's long
 * press, 11..14 the long press of A B X Y, 15..16 the long press of L R. */
enum { K_BTN_N = 10, K_DR = 10, K_HOLD0 = 11, K_N = 17 };

/* The button a long press belongs to (hold index 0..5 = A B X Y L R) */
static const u8 HOLD_BTN[6] = {PB_A, PB_B, PB_X, PB_Y, PB_L, PB_R};
/* Where each long press keeps its short press's "wait" bit in bm_hold_wait: DR, A, B, X, Y, L, R */
#define WAIT_BIT(h) (1u << ((h) + 1))

static volatile u8 *role_ptr(int k) {
    if (k < K_BTN_N) return &g_shbcfg.bm_role[k];
    if (k == K_DR) return &g_shbcfg.bm_role_dr;
    if (k < K_HOLD0 + 4) return &g_shbcfg.bm_hold[k - K_HOLD0];
    return &g_shbcfg.bm_hold_lr[k - K_HOLD0 - 4];
}

static int role_dup_ok(int r) { return r == ROLE_JUMP || r == ROLE_SWORD || r == ROLE_SELECTOR || r == ROLE_ARMORY || r == ROLE_BACK; }
static int role_blocks_hold(int r) {
    return r == ROLE_SHIELD || r == ROLE_TARGET || r == ROLE_SPRINT || r == ROLE_SELECTOR || r == ROLE_ARMORY ||
           r == ROLE_ITEM1 || r == ROLE_ITEM2 || r == ROLE_ITEM3 || r == ROLE_ITEM_MAIN;
}
static int role_is_item(int r) { return r >= ROLE_ITEM1 && r <= ROLE_ITEM_MAIN; }

/* What a key may hold (the configurator's role_allowed) */
static int role_allowed(int r, int k) {
    if (r == ROLE_NONE) return 1;
    if (k >= K_HOLD0) return r == ROLE_JUMP || r == ROLE_SWORD || r == ROLE_ACTION || r == ROLE_CAMERA || r == ROLE_BACK;
    if (k == K_DR) {
        return r == ROLE_JUMP || r == ROLE_SWORD || r == ROLE_CAMERA || r == ROLE_SPRINT || r == ROLE_SHIELD ||
               r == ROLE_SELECTOR || r == ROLE_ARMORY || r == ROLE_BACK;
    }
    const int stick = k == PB_L3 || k == PB_R3;
    if (role_is_item(r) && !g_shbcfg.hud_custom_items) return 0;   /* the item roles live on the item HUD */
    if (r == ROLE_TARGET) return k == PB_ZL || k == PB_ZR || stick;
    if (r == ROLE_ACTION) return k == PB_A || k == PB_B || k == PB_X || k == PB_Y;
    if (role_is_item(r) || r == ROLE_SELECTOR) return !stick;
    return 1;
}

static int role_holder(int r, int except) {
    for (int k = 0; k < K_N; k++) if (k != except && *role_ptr(k) == r) return k;
    return -1;
}

/* Whatever follows from the table: the sprint flag, the manual shield, the holds a button's role rules out. */
static void roles_derive(void) {
    for (int h = 0; h < 6; h++)
        if (role_blocks_hold(*role_ptr(HOLD_BTN[h])) && *role_ptr(K_HOLD0 + h)) *role_ptr(K_HOLD0 + h) = ROLE_NONE;
    int sprint = 0, shield = 0;
    for (int k = 0; k < K_N; k++) { int r = *role_ptr(k); if (r == ROLE_SPRINT) sprint = 1; if (r == ROLE_SHIELD) shield = 1; }
    g_shbcfg.sprint_l3 = (u8)sprint;
    g_shbcfg.target_r3 = 0;
    g_shbcfg.bm_l_mode = 0;
    if (g_shbcfg.bm_layout != 3) g_shbcfg.bm_layout = 0;
    if (shield) g_shbcfg.manual_shield = 1;
    if (!g_shbcfg.manual_shield)
        for (int k = 0; k < K_N; k++) if (*role_ptr(k) == ROLE_SHIELD) *role_ptr(k) = ROLE_NONE;
    g_shbcfg.bm_hold_wait |= 1;     /* the D-pad right's short press is Navi's call: it always waits */
}

/* Gives key k the role r the way the configurator does: a role held elsewhere moves to where this key's old role
 * can go (or is dropped); the roles that may repeat just repeat. */
static void role_set(int k, int r) {
    const int old = *role_ptr(k);
    if (r == old) return;
    u8 before[K_BTN_N];
    for (int i = 0; i < K_BTN_N; i++) before[i] = *role_ptr(i);
    if (r != ROLE_NONE && !role_dup_ok(r)) {
        const int o = role_holder(r, k);
        if (o >= 0) *role_ptr(o) = (u8)(role_allowed(old, o) ? old : ROLE_NONE);
    }
    *role_ptr(k) = (u8)r;
    roles_derive();
    /* A button that held an item slot and now does something else gives its item up: left assigned it would
     * stay drawn over the new role's icon. */
    for (int i = 0; i < K_BTN_N; i++)
        if (before[i] >= ROLE_ITEM1 && before[i] <= ROLE_ITEM3 && *role_ptr(i) != before[i])
            hud_unassign_item(before[i] - ROLE_ITEM1);
}

static void role_step(int k, int dir) {
    int r = *role_ptr(k);
    for (int n = 0; n < ROLE_N; n++) {
        r = (r + ROLE_N + dir) % ROLE_N;
        /* A role that cannot repeat and is already on another button is not offered: it is freed by changing the
         * button that holds it. (Stepping onto it used to swap the two and moved the table under the cursor.) */
        if (role_allowed(r, k) && (r == ROLE_NONE || role_dup_ok(r) || role_holder(r, k) < 0)) { role_set(k, r); return; }
    }
}

/* ---- the HUD layout the configurator writes for the counters shown -------------------------------------------- */

static int iround(double v) { return (int)(v < 0 ? v - 0.5 : v + 0.5); }

/* The four blocks (rupee, clock, skulltulas, heart pieces) are placed left to right with no hole for one that is
 * off; the layout fields come with the mode. Same numbers as installer.patch_config. */
static void hud_relayout(void) {
    static const double PLATE_W[4] = {466.4, 466.4, 425.9, 425.9};
    static const int SIMPLE_ADV[4] = {250, 240, 230, 230};
    const int mode = g_shbcfg.hud_mode > 2 ? 0 : g_shbcfg.hud_mode;
    int slots[4], n = 0;
    if (g_shbcfg.hud_rupees) slots[n++] = 0;
    if (g_shbcfg.hud_clock) slots[n++] = 1;
    if (g_shbcfg.hud_collectibles) { slots[n++] = 2; slots[n++] = 3; }
    int pos[4] = {-32768, -32768, -32768, -32768};
    if (mode) {
        double x = -1685;
        for (int i = 0; i < n; i++) { pos[slots[i]] = iround(x); x += SIMPLE_ADV[slots[i]]; }
        g_shbcfg.hud_sx = 100; g_shbcfg.hud_sy = 48; g_shbcfg.hud_tscale = 50; g_shbcfg.hud_iscale = 45;
        g_shbcfg.hud_plates = mode == 1 ? 2 : 0;
        g_shbcfg.hud_y = -1010; g_shbcfg.hud_ioff = -140; g_shbcfg.hud_tx = 10; g_shbcfg.hud_ioff2 = -130;
    } else {
        double x = -1518;
        for (int i = 0; i < n; i++) {
            pos[slots[i]] = iround(x);
            if (i + 1 < n) x += (PLATE_W[slots[i]] + PLATE_W[slots[i + 1]]) * 0.5 + 24;
        }
        g_shbcfg.hud_sx = 46; g_shbcfg.hud_sy = 62; g_shbcfg.hud_tscale = 0; g_shbcfg.hud_iscale = 85;
        g_shbcfg.hud_plates = 1;
        g_shbcfg.hud_y = -960; g_shbcfg.hud_ioff = -165; g_shbcfg.hud_tx = 30; g_shbcfg.hud_ioff2 = -165;
    }
    for (int i = 0; i < 4; i++) g_shbcfg.hud_x[i] = (s16)pos[i];
    g_shbcfg.hud_map_hearts = 0;
    g_shbcfg.hud_flags = 1;
    g_shbcfg.hud_mode = (u8)mode;
}

/* ---- the button table switch ---------------------------------------------------------------------------------- */

/* What switching the table off clears (the configurator's "no mapping" branch) is kept, so switching it back on
 * restores it. */
static u8 g_shadow[24];
static u8 g_shadow_ok;
static void controls_save(u8 *o) {
    for (int k = 0; k < K_BTN_N; k++) *o++ = *role_ptr(k);
    *o++ = g_shbcfg.bm_role_dr;
    for (int k = 0; k < 4; k++) *o++ = g_shbcfg.bm_hold[k];
    *o++ = g_shbcfg.bm_hold_lr[0]; *o++ = g_shbcfg.bm_hold_lr[1];
    *o++ = g_shbcfg.bm_hold_wait; *o++ = g_shbcfg.manual_shield; *o++ = g_shbcfg.auto_target;
    *o++ = g_shbcfg.switch_target; *o++ = g_shbcfg.sprint_l3_toggle; *o++ = g_shbcfg.hud_custom_items;
}
static void controls_load(const u8 *o) {
    for (int k = 0; k < K_BTN_N; k++) *role_ptr(k) = *o++;
    g_shbcfg.bm_role_dr = *o++;
    for (int k = 0; k < 4; k++) g_shbcfg.bm_hold[k] = *o++;
    g_shbcfg.bm_hold_lr[0] = *o++; g_shbcfg.bm_hold_lr[1] = *o++;
    g_shbcfg.bm_hold_wait = *o++; g_shbcfg.manual_shield = *o++; g_shbcfg.auto_target = *o++;
    g_shbcfg.switch_target = *o++; g_shbcfg.sprint_l3_toggle = *o++; g_shbcfg.hud_custom_items = *o++;
}

static void mapping_set(int on) {
    if (on == (g_shbcfg.button_mapping != 0)) return;
    if (on) {
        g_shbcfg.button_mapping = 1;
        g_shbcfg.bm_remap = 1;
        if (g_shadow_ok) controls_load(g_shadow);
        roles_derive();
    } else {
        controls_save(g_shadow);
        g_shadow_ok = 1;
        g_shbcfg.button_mapping = 0;
        g_shbcfg.bm_remap = 0;
        g_shbcfg.bm_l_mode = 0;
        g_shbcfg.manual_shield = g_shbcfg.target_r3 = g_shbcfg.sprint_l3 = 0;
        g_shbcfg.auto_target = g_shbcfg.switch_target = g_shbcfg.sprint_l3_toggle = 0;
        g_shbcfg.bm_role_dr = 0;
        for (int k = 0; k < 4; k++) g_shbcfg.bm_hold[k] = 0;
        g_shbcfg.bm_hold_lr[0] = g_shbcfg.bm_hold_lr[1] = 0;
        g_shbcfg.bm_hold_wait = 0;
        g_shbcfg.hud_custom_items = 0;
    }
}

/* ---- rows ----------------------------------------------------------------------------------------------------- */

enum { PAGE_CTRL, PAGE_HUD, PAGE_GAME, PAGE_N };
enum {
    R_MAP = 1,
    R_SAVE = 2,
    R_ROLE = 10,            /* + key 0..9 */
    R_DRHOLD = 20,
    R_HOLD = 21,            /* + hold 0..5 */
    R_HWAIT = 27,           /* + hold 0..5 */
    R_SHIELD = 33, R_SWITCH, R_AUTO, R_SPRINTTOG,
    R_H_RUPEES = 40, R_H_CLOCK, R_H_CLOCK24, R_H_COLLECT, R_H_MODE, R_H_MINIMAP, R_H_CTRLSCALE, R_H_COUNTSCALE,
    R_H_STYLE, R_H_LAYOUT, R_H_SIMPLE, R_H_ITEMS, R_H_EBARS, R_H_BBAR, R_H_STBAR,
    R_G_STAMINA = 60, R_G_PRESET, R_G_COSTJ, R_G_COSTA, R_G_COSTS, R_G_COSTB, R_G_COSTSH, R_G_COSTR, R_G_COMBAT,
    R_G_SPRINTSPD, R_G_EHP, R_G_DMG, R_G_SONGBOOK, R_G_QBOOTS, R_G_QMASK
};
#define MAX_ROWS 48

static int lang_es(void) { return g_shbcfg.lang == 1; }
#define T(en, es) (lang_es() ? (es) : (en))

static int page_rows(int page, u8 *rows) {
    int n = 0;
    if (page == PAGE_CTRL) {
        rows[n++] = R_MAP;
        if (!g_shbcfg.button_mapping) return n;
        for (int k = 0; k < K_BTN_N; k++) {
            rows[n++] = (u8)(R_ROLE + k);
            const int r = *role_ptr(k);
            if (r == ROLE_TARGET) { rows[n++] = R_SHIELD; rows[n++] = R_SWITCH; rows[n++] = R_AUTO; }
            if (r == ROLE_SPRINT) rows[n++] = R_SPRINTTOG;
            for (int h = 0; h < 6; h++) {
                if (HOLD_BTN[h] != k || role_blocks_hold(r)) continue;
                rows[n++] = (u8)(R_HOLD + h);
                if (*role_ptr(K_HOLD0 + h)) rows[n++] = (u8)(R_HWAIT + h);
            }
        }
        rows[n++] = R_DRHOLD;
    } else if (page == PAGE_HUD) {
        static const u8 H[] = {R_H_RUPEES, R_H_CLOCK, R_H_CLOCK24, R_H_COLLECT, R_H_MODE, R_H_MINIMAP, R_H_CTRLSCALE,
                               R_H_COUNTSCALE, R_H_LAYOUT, R_H_SIMPLE, R_H_ITEMS, R_H_EBARS, R_H_BBAR, R_H_STBAR};
        for (u32 i = 0; i < sizeof H; i++) rows[n++] = H[i];
    } else {
        rows[n++] = R_G_STAMINA;
        if (g_shbcfg.stamina_enabled) {
            rows[n++] = R_G_PRESET;
            if (g_shbcfg.preset == 3) {
                static const u8 C[] = {R_G_COSTJ, R_G_COSTA, R_G_COSTS, R_G_COSTB, R_G_COSTSH, R_G_COSTR, R_G_COMBAT};
                for (u32 i = 0; i < sizeof C; i++) rows[n++] = C[i];
            }
        }
        rows[n++] = R_G_SPRINTSPD; rows[n++] = R_G_EHP; rows[n++] = R_G_DMG;
        rows[n++] = R_G_SONGBOOK; rows[n++] = R_G_QBOOTS; rows[n++] = R_G_QMASK;
    }
    return n;
}

static int row_is_sub(int id) {
    return id == R_SHIELD || id == R_SWITCH || id == R_AUTO || id == R_SPRINTTOG || (id >= R_HWAIT && id < R_HWAIT + 6) ||
           (id >= R_G_COSTJ && id <= R_G_COMBAT);
}

static const char *page_label(int p) {
    if (p == PAGE_CTRL) return T("CONTROLS", "CONTROLES");
    if (p == PAGE_HUD) return "HUD";
    return T("GAMEPLAY", "JUEGO");
}

static const char *btn_name(int k) {
    switch (k) {
    case PB_L: return "L";
    case PB_X: return "X";
    case PB_R: return "R";
    case PB_Y: return "Y";
    case PB_A: return "A";
    case PB_ZR: return "ZR";
    case PB_B: return "B";
    case PB_ZL: return "ZL";
    case PB_L3: return "L3";
    default: return "R3";
    }
}

static const char *role_name(int r) {
    switch (r) {
    case ROLE_NONE: return "-";
    case ROLE_JUMP: return T("Jump", "Saltar");
    case ROLE_SWORD: return T("Sword", "Espada");
    case ROLE_ACTION: return T("Action", "Accion");
    case ROLE_TARGET: return T("Target", "Fijar");
    case ROLE_ITEM1: return T("Item 1", "Objeto 1");
    case ROLE_ITEM2: return T("Item 2", "Objeto 2");
    case ROLE_ITEM3: return T("Item 3", "Objeto 3");
    case ROLE_ITEM_MAIN: return T("Main item", "Obj. principal");
    case ROLE_CAMERA: return T("Center camera", "Centrar camara");
    case ROLE_SPRINT: return T("Sprint", "Sprint");
    case ROLE_SHIELD: return T("Shield", "Escudo");
    case ROLE_SELECTOR: return T("Item selector", "Selector objetos");
    case ROLE_ARMORY: return T("Armory", "Armeria");
    default: return T("Back", "Atras");
    }
}

/* Writes the row's label (without the indentation) into b */
static void row_label(int id, char *b) {
    const char *s = "";
    int n = 0;
    if (id >= R_ROLE && id < R_ROLE + K_BTN_N) {
        s = btn_name(id - R_ROLE);
    } else if (id >= R_HOLD && id < R_HOLD + 6) {
        const char *bn = btn_name(HOLD_BTN[id - R_HOLD]);
        while (*bn) b[n++] = *bn++;
        s = T(" held", " mantenido");
    } else if (id == R_DRHOLD) {
        s = T("D-pad right held", "Cruceta der. mantenida");
    } else if (id >= R_HWAIT && id < R_HWAIT + 6) {
        s = T("Wait for the hold", "Esperar al mantener");
    } else {
        switch (id) {
        case R_SAVE: s = T("Save settings", "Guardar ajustes"); break;
        case R_MAP: s = T("Button mapping", "Mapeo de botones"); break;
        case R_SHIELD: s = T("Manual shield", "Escudo manual"); break;
        case R_SWITCH: s = T("Tap: next enemy", "Toque: siguiente enemigo"); break;
        case R_AUTO: s = T("Next one when it dies", "Siguiente al morir"); break;
        case R_SPRINTTOG: s = T("Toggle", "Alternar"); break;
        case R_H_RUPEES: s = T("Rupee counter", "Contador de rupias"); break;
        case R_H_CLOCK: s = T("Clock", "Reloj"); break;
        case R_H_CLOCK24: s = T("24 hour clock", "Reloj de 24 horas"); break;
        case R_H_COLLECT: s = T("Skulltulas and hearts", "Skulltulas y corazones"); break;
        case R_H_MODE: s = T("HUD layout", "Estilo del HUD"); break;
        case R_H_MINIMAP: s = T("Hide minimap", "Ocultar minimapa"); break;
        case R_H_CTRLSCALE: s = T("Buttons size", "Tamano de los botones"); break;
        case R_H_COUNTSCALE: s = T("Counters size", "Tamano de los contadores"); break;
        case R_H_STYLE: s = T("Button style", "Estilo de botones"); break;
        case R_H_LAYOUT: s = T("Face buttons", "Botones principales"); break;
        case R_H_SIMPLE: s = T("Assignable buttons only", "Solo botones asignables"); break;
        case R_H_ITEMS: s = T("Item HUD", "HUD de objetos"); break;
        case R_H_EBARS: s = T("Enemy health bars", "Barras de vida de enemigos"); break;
        case R_H_BBAR: s = T("Boss health bar", "Barra de vida del jefe"); break;
        case R_H_STBAR: s = T("Stamina display", "Indicador de estamina"); break;
        case R_G_STAMINA: s = T("Stamina", "Estamina"); break;
        case R_G_PRESET: s = T("Stamina preset", "Modo de estamina"); break;
        case R_G_COSTJ: s = T("Jump costs", "Saltar cuesta"); break;
        case R_G_COSTA: s = T("Attack costs", "Atacar cuesta"); break;
        case R_G_COSTS: s = T("Sprint costs", "Correr cuesta"); break;
        case R_G_COSTB: s = T("Block costs", "Bloquear cuesta"); break;
        case R_G_COSTSH: s = T("Shoot costs", "Disparar cuesta"); break;
        case R_G_COSTR: s = T("Roll costs", "Rodar cuesta"); break;
        case R_G_COMBAT: s = T("Only in combat", "Solo en combate"); break;
        case R_G_SPRINTSPD: s = T("Sprint speed", "Velocidad al correr"); break;
        case R_G_EHP: s = T("Enemy health", "Vida de enemigos"); break;
        case R_G_DMG: s = T("Damage taken", "Dano recibido"); break;
        case R_G_SONGBOOK: s = T("Songbook", "Libro de canciones"); break;
        case R_G_QBOOTS: s = T("Quick boots", "Botas rapidas"); break;
        default: s = T("Quick mask", "Mascara rapida"); break;
        }
    }
    while (*s) b[n++] = *s++;
    b[n] = 0;
}

/* The percentages the configurator offers */
static const u16 LAD_MULT[8] = {100, 150, 200, 250, 300, 400, 500, 600};
static const u16 LAD_SPEED[7] = {100, 125, 150, 175, 200, 250, 300};

static void ladder_step(volatile u16 *v, const u16 *lad, int n, int dir) {
    int i = 0;
    while (i + 1 < n && lad[i + 1] <= *v) i++;
    i += dir;
    if (i < 0) i = 0;
    if (i >= n) i = n - 1;
    *v = lad[i];
}

static void toggle8(volatile u8 *p) { *p = *p ? 0 : 1; }

static int scale_of(int off) { int v = *b8(off); return v ? v : 100; }
static void scale_step(int off, int dir) {
    int v = scale_of(off) + dir * 5;
    if (v < 50) v = 50;
    if (v > 200) v = 200;
    *b8(off) = (u8)v;
}

static void enum_step(volatile u8 *p, int n, int dir) { *p = (u8)((*p + n + dir) % n); }

/* Changes the row by one step (dir -1 / +1). The caller repaints. */
static void row_change(int id, int dir) {
    if (id >= R_ROLE && id < R_ROLE + K_BTN_N) { role_step(id - R_ROLE, dir); return; }
    if (id == R_DRHOLD) { role_step(K_DR, dir); return; }
    if (id >= R_HOLD && id < R_HOLD + 6) {
        const int h = id - R_HOLD;
        role_step(K_HOLD0 + h, dir);
        if (*role_ptr(K_HOLD0 + h)) g_shbcfg.bm_hold_wait &= (u8)~WAIT_BIT(h);   /* a new hold: the short role acts at once */
        return;
    }
    if (id >= R_HWAIT && id < R_HWAIT + 6) { g_shbcfg.bm_hold_wait ^= (u8)WAIT_BIT(id - R_HWAIT); return; }
    switch (id) {
    case R_MAP: mapping_set(!g_shbcfg.button_mapping); break;
    case R_SHIELD: toggle8(&g_shbcfg.manual_shield); roles_derive(); break;
    case R_SWITCH: toggle8(&g_shbcfg.switch_target); break;
    case R_AUTO: toggle8(&g_shbcfg.auto_target); break;
    case R_SPRINTTOG: toggle8(&g_shbcfg.sprint_l3_toggle); break;
    case R_H_RUPEES: toggle8(&g_shbcfg.hud_rupees); hud_relayout(); break;
    case R_H_CLOCK: toggle8(&g_shbcfg.hud_clock); hud_relayout(); break;
    case R_H_CLOCK24: toggle8(&g_shbcfg.clock_24h); break;
    case R_H_COLLECT: toggle8(&g_shbcfg.hud_collectibles); hud_relayout(); break;
    case R_H_MODE: enum_step(&g_shbcfg.hud_mode, 3, dir); hud_relayout(); break;
    case R_H_MINIMAP: toggle8(&g_shbcfg.hide_minimap); break;
    case R_H_CTRLSCALE: scale_step(OFF(hud_ctrl_scale), dir); break;
    case R_H_COUNTSCALE: scale_step(OFF(hud_count_scale), dir); break;
    case R_H_STYLE: enum_step(&g_shbcfg.button_style, 3, dir); break;
    case R_H_LAYOUT:
        if (g_shbcfg.bm_remap) g_shbcfg.bm_layout = g_shbcfg.bm_layout == 3 ? 0 : 3;   /* the table keeps the face buttons: Nintendo or GameCube */
        else enum_step(&g_shbcfg.bm_layout, 4, dir);
        break;
    case R_H_SIMPLE: toggle8(&g_shbcfg.bm_simple); break;
    case R_H_ITEMS:
        toggle8(&g_shbcfg.hud_custom_items);
        if (!g_shbcfg.hud_custom_items) {   /* the game's own item HUD: no button may keep an item role */
            for (int k = 0; k < K_N; k++) if (role_is_item(*role_ptr(k))) *role_ptr(k) = ROLE_NONE;
            for (int s = 0; s < 3; s++) hud_unassign_item(s);
            hud_items_off();
            roles_derive();
        }
        break;
    case R_H_EBARS: toggle8(&g_shbcfg.enemy_bars); break;
    case R_H_BBAR: toggle8(&g_shbcfg.boss_bar); break;
    case R_H_STBAR: toggle8(&g_shbcfg.stamina_bar); break;
    case R_G_STAMINA: toggle8(&g_shbcfg.stamina_enabled); break;
    case R_G_PRESET: enum_step(&g_shbcfg.preset, 4, dir); break;
    case R_G_COSTJ: toggle8(&g_shbcfg.cost_jump); break;
    case R_G_COSTA: toggle8(&g_shbcfg.cost_attack); break;
    case R_G_COSTS: toggle8(&g_shbcfg.cost_sprint); break;
    case R_G_COSTB: toggle8(&g_shbcfg.cost_block); break;
    case R_G_COSTSH: toggle8(&g_shbcfg.cost_shoot); break;
    case R_G_COSTR: toggle8(&g_shbcfg.cost_roll); break;
    case R_G_COMBAT: toggle8(&g_shbcfg.combat_only); break;
    case R_G_SPRINTSPD: ladder_step(&g_shbcfg.sprint_speed_pct, LAD_SPEED, 7, dir); break;
    case R_G_EHP: ladder_step(&g_shbcfg.enemy_hp_pct, LAD_MULT, 8, dir); break;
    case R_G_DMG: ladder_step(&g_shbcfg.damage_taken_pct, LAD_MULT, 8, dir); break;
    case R_G_SONGBOOK: toggle8(&g_shbcfg.songbook_enabled); break;
    case R_G_QBOOTS: toggle8(&g_shbcfg.quick_boots); break;
    case R_G_QMASK: toggle8(&g_shbcfg.quick_mask); break;
    default: break;
    }
}

static int put_u32(char *b, u32 v) {
    char t[10]; int n = 0, k = 0;
    do { t[n++] = (char)('0' + v % 10); v /= 10; } while (v);
    while (n) b[k++] = t[--n];
    return k;
}

static int put_pct(char *b, u32 v) { int n = put_u32(b, v); b[n++] = '%'; b[n] = 0; return n; }

#define V_ONOFF 1
#define V_TEXT 2
#define V_ON 0x10
/* The row's value as text; returns how it is shown (on / off colours or plain). */
static int row_value(int id, char *b) {
    const char *s = 0;
    int on = -1;
    int n = 0;
    if (id == R_SAVE) {                       /* what a restart would give, against what is set now */
        const int st = im_persist_state();
        s = st == 0 ? T("Saved", "Guardado") : (st == 1 ? T("Not saved: press A", "Sin guardar: pulsa A")
          : (st == 2 ? T("Saving...", "Guardando...") : T("Could not save", "No se pudo guardar")));
        while (*s) b[n++] = *s++;
        b[n] = 0;
        return st == 0 ? (V_ONOFF | V_ON) : (st == 2 ? V_TEXT : V_ONOFF);
    }
    if ((id >= R_ROLE && id < R_ROLE + K_BTN_N) || id == R_DRHOLD || (id >= R_HOLD && id < R_HOLD + 6)) {
        const int k = id == R_DRHOLD ? K_DR : (id >= R_HOLD ? K_HOLD0 + (id - R_HOLD) : id - R_ROLE);
        s = role_name(*role_ptr(k));
    } else if (id >= R_HWAIT && id < R_HWAIT + 6) {
        on = (g_shbcfg.bm_hold_wait & WAIT_BIT(id - R_HWAIT)) != 0;
    } else {
        switch (id) {
        case R_MAP: on = g_shbcfg.button_mapping != 0; break;
        case R_SHIELD: on = g_shbcfg.manual_shield != 0; break;
        case R_SWITCH: on = g_shbcfg.switch_target != 0; break;
        case R_AUTO: on = g_shbcfg.auto_target != 0; break;
        case R_SPRINTTOG: on = g_shbcfg.sprint_l3_toggle != 0; break;
        case R_H_RUPEES: on = g_shbcfg.hud_rupees != 0; break;
        case R_H_CLOCK: on = g_shbcfg.hud_clock != 0; break;
        case R_H_CLOCK24: on = g_shbcfg.clock_24h != 0; break;
        case R_H_COLLECT: on = g_shbcfg.hud_collectibles != 0; break;
        case R_H_MODE: s = g_shbcfg.hud_mode == 1 ? "Simple" : (g_shbcfg.hud_mode == 2 ? T("Plain", "Plano") : T("Wide", "Ancho")); break;
        case R_H_MINIMAP: on = g_shbcfg.hide_minimap != 0; break;
        case R_H_CTRLSCALE: put_pct(b, (u32)scale_of(OFF(hud_ctrl_scale))); return V_TEXT;
        case R_H_COUNTSCALE: put_pct(b, (u32)scale_of(OFF(hud_count_scale))); return V_TEXT;
        case R_H_STYLE: s = g_shbcfg.button_style == 1 ? "Xbox" : (g_shbcfg.button_style == 2 ? "PlayStation" : "Switch"); break;
        case R_H_LAYOUT: s = g_shbcfg.bm_layout == 3 ? "GameCube" : (g_shbcfg.bm_layout == 1 ? "A/B" : (g_shbcfg.bm_layout == 2 ? "Xbox" : "Nintendo")); break;
        case R_H_SIMPLE: on = g_shbcfg.bm_simple != 0; break;
        case R_H_ITEMS: s = g_shbcfg.hud_custom_items ? T("Ours (B/L/R/ZR)", "Propio (B/L/R/ZR)") : T("Game's", "Del juego"); break;
        case R_H_EBARS: on = g_shbcfg.enemy_bars != 0; break;
        case R_H_BBAR: on = g_shbcfg.boss_bar != 0; break;
        case R_H_STBAR: s = g_shbcfg.stamina_bar ? T("Bar", "Barra") : T("Ring", "Anillo"); break;
        case R_G_STAMINA: on = g_shbcfg.stamina_enabled != 0; break;
        case R_G_PRESET:
            s = g_shbcfg.preset == 0 ? T("Hardcore", "Extremo")
              : (g_shbcfg.preset == 1 ? T("Combat only", "Solo combate")
              : (g_shbcfg.preset == 2 ? T("Sprint only", "Solo correr") : T("Advanced", "Avanzado")));
            break;
        case R_G_COSTJ: on = g_shbcfg.cost_jump != 0; break;
        case R_G_COSTA: on = g_shbcfg.cost_attack != 0; break;
        case R_G_COSTS: on = g_shbcfg.cost_sprint != 0; break;
        case R_G_COSTB: on = g_shbcfg.cost_block != 0; break;
        case R_G_COSTSH: on = g_shbcfg.cost_shoot != 0; break;
        case R_G_COSTR: on = g_shbcfg.cost_roll != 0; break;
        case R_G_COMBAT: on = g_shbcfg.combat_only != 0; break;
        case R_G_SPRINTSPD: put_pct(b, g_shbcfg.sprint_speed_pct); return V_TEXT;
        case R_G_EHP: put_pct(b, g_shbcfg.enemy_hp_pct); return V_TEXT;
        case R_G_DMG: put_pct(b, g_shbcfg.damage_taken_pct); return V_TEXT;
        case R_G_SONGBOOK: on = g_shbcfg.songbook_enabled != 0; break;
        case R_G_QBOOTS: on = g_shbcfg.quick_boots != 0; break;
        case R_G_QMASK: on = g_shbcfg.quick_mask != 0; break;
        default: break;
        }
    }
    if (on >= 0) s = on ? T("ON", "SI") : "OFF";
    if (!s) s = "";
    while (*s) b[n++] = *s++;
    b[n] = 0;
    return on >= 0 ? (V_ONOFF | (on ? V_ON : 0)) : V_TEXT;
}

/* ---- state ---------------------------------------------------------------------------------------------------- */

#define IM_MAGIC2 0x4D454E55u
u32 g_menu_dbg[8] = {IM_MAGIC2, IM_MAGIC2, IM_MAGIC2, IM_MAGIC2, IM_MAGIC2, IM_MAGIC2, IM_MAGIC2, IM_MAGIC2};
enum { M_OPENS, M_CHORDS, M_PAINTS, M_NAVS, M_CHANGES, M_HELD, M_PRESSED, M_TAPS };   /* M_TAPS: lone + / - replayed */

volatile u8 g_open;                  /* 1 while the menu is open (read by the debug tools too) */
static u8 g_page, g_sel;
static u8 g_top;
static volatile u32 g_ver = 1;
static u32 g_painted_ver;
static int g_painted_f, g_painted_st = -1;
static u64 g_prev, g_hide;
#if SHB_DBG_DUMP
u32 g_menu_ring[64][2] = {{1}};   /* the first 64 changes of the merged buttons: buttons, call number */
u32 g_menu_ring_n = 1;
static u32 g_calls;
#endif
static u32 g_rep_t[4], g_rep_last[4];   /* when a direction went down / last repeated (ms; 0 = up) */

int im_menu_open(void) { return g_open; }
static void touch(void) { g_ver++; }

/* ---- input ---------------------------------------------------------------------------------------------------- */

#define B_A     0x0001ull
#define B_B     0x0002ull
#define B_L     0x0040ull
#define B_R     0x0080ull
#define B_PLUS  0x0400ull
#define B_MINUS 0x0800ull
#define B_LEFT  0x1000ull
#define B_UP    0x2000ull
#define B_RIGHT 0x4000ull
#define B_DOWN  0x8000ull
#define B_CHORD (B_PLUS | B_MINUS)
/* Times are real milliseconds (core/clock.c): the game reads input about twice per frame, and the frame rate varies. */
#define REP_DELAY_MS 400u   /* a direction held this long starts repeating */
#define REP_EVERY_MS 90u
#define CHORD_WAIT_MS 150u  /* + or - is held back this long, waiting for the other one */
#define TAP_READS 4u        /* reads a lone + or - is replayed for */

static void strip(const HidView *v, u64 mask, int sticks) {
    for (u32 i = 0; i < v->count; i++) {
        hid_set_buttons(v, i, hid_get_buttons(v, i) & ~mask);
        if (sticks) {
            volatile int *s = (volatile int *)((u8 *)v->samples + (u64)i * HID_STRIDE + 0x10);
            s[0] = 0; s[1] = 0; s[2] = 0; s[3] = 0;
        }
    }
}
static void add_buttons(const HidView *v, u64 mask) {
    for (u32 i = 0; i < v->count; i++) hid_set_buttons(v, i, hid_get_buttons(v, i) | mask);
}

static void rows_clamp(int n) {
    if (g_sel >= n) g_sel = (u8)(n - 1);
    if (g_sel < g_top) g_top = g_sel;
    if (g_sel >= g_top + VIS_ROWS) g_top = (u8)(g_sel - VIS_ROWS + 1);
    if (g_top + VIS_ROWS > n) g_top = (u8)(n > VIS_ROWS ? n - VIS_ROWS : 0);
}

static void move_row(int d) {
    u8 rows[MAX_ROWS];
    const int n = page_rows(g_page, rows);
    g_sel = (u8)((g_sel + n + d) % n);
    rows_clamp(n);
    g_menu_dbg[M_NAVS]++;
    touch();
}

static void change_row(int d) {
    u8 rows[MAX_ROWS];
    int n = page_rows(g_page, rows);
    if (g_sel >= n) return;
    const u8 id = rows[g_sel];
    row_change(id, d);
    im_persist_request();                 /* every change is kept: the mod saves it by itself */
    n = page_rows(g_page, rows);          /* the change can add or remove rows: the cursor stays on the same one */
    for (int i = 0; i < n; i++) if (rows[i] == id) { g_sel = (u8)i; break; }
    rows_clamp(n);
    g_menu_dbg[M_CHANGES]++;
    touch();
}

static void move_page(int d) {
    g_page = (u8)((g_page + PAGE_N + d) % PAGE_N);
    g_sel = g_top = 0;
    g_menu_dbg[M_NAVS]++;
    touch();
}

/* the lone-press bookkeeping of + and - (menu closed) */
static u32 g_t0[2];          /* when the press went down (ms) */
static u8 g_wait[2];         /* the press is being held back */
static u8 g_passed[2];       /* the press was let through to the game: it is passed while held */
static u8 g_pulse[2];        /* reads left of a replayed tap */

void im_persist_init(void);
void im_persist_poll(void);

#if SHB_DBG_DUMP
/* Test aid (debug builds only): the debug server writes IM_MAGIC2 + (key << 8 | 1 for +1, 0 for -1) here to make the
 * menu do a role change exactly as the row would, so the table logic can be proven without driving the cursor. */
u32 g_menu_test = IM_MAGIC2;
static void test_poll(void) {
    const u32 c = g_menu_test - IM_MAGIC2;
    if (!c) return;
    g_menu_test = IM_MAGIC2;
    if (c & 0x20000) { extern int hud_box_set(int item); g_menu_dbg[M_TAPS] = IM_MAGIC2 + (u32)hud_box_set((int)((c >> 8) & 0xFF)); }   /* test aid: item in the game's box */
    else if (c & 0x10000) row_change((int)((c >> 8) & 0xFF), (c & 1) ? 1 : -1);   /* any row, by id */
    else role_step((int)((c >> 8) & 0xFF), (c & 1) ? 1 : -1);
    im_persist_request();
    touch();
}
#else
static void test_poll(void) {}
#endif

void ingame_menu_on_input(u32 site, void *a0, void *a1, void *a2, u32 stock_result) {
    (void)site; (void)a2;
    im_persist_init();
    im_persist_poll();
    test_poll();
    const u32 count = hid_sample_count(a1, stock_result);
    HidView v = {a0, count, 0};
    /* The newest few samples, not all 16: the game's buffer keeps the last 16 states, so the OR of all of them
     * stays set for 16 reads after a release and an edge would arrive late (a short tap would look held). */
    u64 held = 0;
    for (u32 i = 0; i < (count < 3 ? count : 3); i++) held |= hid_get_buttons(&v, i);
    const u64 pressed = held & ~g_prev;
    const u64 released = g_prev & ~held;
    g_prev = held;
#if SHB_DBG_DUMP
    g_calls++;
    if ((pressed || released) && g_menu_ring_n <= 64) {
        u32 *e = g_menu_ring[g_menu_ring_n - 1];
        g_menu_ring_n++;
        e[0] = (u32)held; e[1] = g_calls;
    }
#endif
    g_menu_dbg[M_HELD] = (u32)held + IM_MAGIC2;

    const u32 now = shb_clock_ms();
    const int playing = hud_ingame_known() && hud_ingame() && !hud_game_covered();
    static const u64 PM[2] = {B_PLUS, B_MINUS};
    u64 hold_back = 0, replay = 0;
    int chord = 0;

    if (g_open) {
        if ((pressed & B_CHORD) && (held & B_CHORD) == B_CHORD) chord = 1;
    } else if (!playing) {
        g_wait[0] = g_wait[1] = 0; g_passed[0] = g_passed[1] = 0; g_pulse[0] = g_pulse[1] = 0;
    } else {
        for (int i = 0; i < 2; i++) {
            if (pressed & PM[i]) { g_wait[i] = 1; g_t0[i] = now; g_passed[i] = 0; }
            if ((held & PM[i]) && !g_passed[i]) {
                if (now - g_t0[i] <= CHORD_WAIT_MS) hold_back |= PM[i];
                else g_passed[i] = 1;                         /* held long enough: it is not a chord */
            }
            if ((released & PM[i]) && !g_passed[i] && g_wait[i]) { g_pulse[i] = TAP_READS; g_wait[i] = 0; g_menu_dbg[M_TAPS]++; }
            if (!(held & PM[i])) { g_wait[i] = 0; g_passed[i] = 0; }
            if (g_pulse[i] && !(held & PM[i])) { replay |= PM[i]; g_pulse[i]--; }
        }
        if ((held & B_CHORD) == B_CHORD && g_wait[0] && g_wait[1] && !g_passed[0] && !g_passed[1]) chord = 1;
    }

    if (chord) {
        g_menu_dbg[M_CHORDS]++;
        g_hide = B_CHORD;
        g_wait[0] = g_wait[1] = 0; g_passed[0] = g_passed[1] = 0; g_pulse[0] = g_pulse[1] = 0;
        hold_back = 0; replay = 0;
        if (g_open) {
            g_open = 0;
        } else if (playing) {
            g_open = 1;                                   /* it opens on the page and row it was left on */
            g_menu_dbg[M_OPENS]++;
        }
        touch();
    } else if (g_open) {
        u64 act = pressed;
        static const u64 DIRS[4] = {B_UP, B_DOWN, B_LEFT, B_RIGHT};
        for (int i = 0; i < 4; i++) {
            if (!(held & DIRS[i])) { g_rep_t[i] = 0; continue; }
            if (!g_rep_t[i]) { g_rep_t[i] = g_rep_last[i] = now ? now : 1; continue; }
            if (now - g_rep_t[i] >= REP_DELAY_MS && now - g_rep_last[i] >= REP_EVERY_MS) { act |= DIRS[i]; g_rep_last[i] = now; }
        }
        g_menu_dbg[M_PRESSED] = (u32)act + IM_MAGIC2;
        if (act & B_UP) move_row(-1);
        if (act & B_DOWN) move_row(1);
        if (act & B_LEFT) change_row(-1);
        if (act & (B_RIGHT | B_A)) change_row(1);
        if (act & B_L) move_page(-1);
        if (act & B_R) move_page(1);
        if (act & B_B) { g_open = 0; touch(); }
    }

    g_hide &= held;                                           /* until both are let go */
    if (g_open) strip(&v, 0xFFFFFFFFFFFFFFFFull, 1);
    else {
        if (g_hide | hold_back) strip(&v, g_hide | hold_back, 0);
        if (replay) add_buttons(&v, replay);
    }
}

/* ---- painting ------------------------------------------------------------------------------------------------- */

static u32 small_float(float f, int mbits) {
    u32 bits;
    __builtin_memcpy(&bits, &f, 4);
    if (bits >> 31) return 0;
    int e = (int)((bits >> 23) & 0xFF) - 127 + 15;
    u32 m = (bits & 0x7FFFFF) >> (23 - mbits);
    if (e <= 0) return 0;
    if (e >= 31) return (30u << mbits) | ((1u << mbits) - 1);
    return ((u32)e << mbits) | m;
}

/* A colour given as the shade it should show on screen (0..1): the buffer is linear and the game's final pass lifts
 * it by about 1.4 (measured with a test pattern: 0.05 -> 17, 0.30 -> 111 of 255). */
static u32 col(float r, float g, float b) {
    const float k = 0.72f;
    return small_float(r * k, 6) | (small_float(g * k, 6) << 11) | (small_float(b * k, 5) << 22);
}

/* 5x7 glyphs for ' '..'~', one byte per column, bit 0 on top */
static const u8 FONT[95][5] = {
    {0x00,0x00,0x00,0x00,0x00},{0x00,0x00,0x5F,0x00,0x00},{0x00,0x07,0x00,0x07,0x00},{0x14,0x7F,0x14,0x7F,0x14},
    {0x24,0x2A,0x7F,0x2A,0x12},{0x23,0x13,0x08,0x64,0x62},{0x36,0x49,0x55,0x22,0x50},{0x00,0x05,0x03,0x00,0x00},
    {0x00,0x1C,0x22,0x41,0x00},{0x00,0x41,0x22,0x1C,0x00},{0x14,0x08,0x3E,0x08,0x14},{0x08,0x08,0x3E,0x08,0x08},
    {0x00,0x50,0x30,0x00,0x00},{0x08,0x08,0x08,0x08,0x08},{0x00,0x60,0x60,0x00,0x00},{0x20,0x10,0x08,0x04,0x02},
    {0x3E,0x51,0x49,0x45,0x3E},{0x00,0x42,0x7F,0x40,0x00},{0x42,0x61,0x51,0x49,0x46},{0x21,0x41,0x45,0x4B,0x31},
    {0x18,0x14,0x12,0x7F,0x10},{0x27,0x45,0x45,0x45,0x39},{0x3C,0x4A,0x49,0x49,0x30},{0x01,0x71,0x09,0x05,0x03},
    {0x36,0x49,0x49,0x49,0x36},{0x06,0x49,0x49,0x29,0x1E},{0x00,0x36,0x36,0x00,0x00},{0x00,0x56,0x36,0x00,0x00},
    {0x08,0x14,0x22,0x41,0x00},{0x14,0x14,0x14,0x14,0x14},{0x00,0x41,0x22,0x14,0x08},{0x02,0x01,0x51,0x09,0x06},
    {0x32,0x49,0x79,0x41,0x3E},{0x7E,0x11,0x11,0x11,0x7E},{0x7F,0x49,0x49,0x49,0x36},{0x3E,0x41,0x41,0x41,0x22},
    {0x7F,0x41,0x41,0x22,0x1C},{0x7F,0x49,0x49,0x49,0x41},{0x7F,0x09,0x09,0x09,0x01},{0x3E,0x41,0x49,0x49,0x7A},
    {0x7F,0x08,0x08,0x08,0x7F},{0x00,0x41,0x7F,0x41,0x00},{0x20,0x40,0x41,0x3F,0x01},{0x7F,0x08,0x14,0x22,0x41},
    {0x7F,0x40,0x40,0x40,0x40},{0x7F,0x02,0x0C,0x02,0x7F},{0x7F,0x04,0x08,0x10,0x7F},{0x3E,0x41,0x41,0x41,0x3E},
    {0x7F,0x09,0x09,0x09,0x06},{0x3E,0x41,0x51,0x21,0x5E},{0x7F,0x09,0x19,0x29,0x46},{0x46,0x49,0x49,0x49,0x31},
    {0x01,0x01,0x7F,0x01,0x01},{0x3F,0x40,0x40,0x40,0x3F},{0x1F,0x20,0x40,0x20,0x1F},{0x3F,0x40,0x38,0x40,0x3F},
    {0x63,0x14,0x08,0x14,0x63},{0x07,0x08,0x70,0x08,0x07},{0x61,0x51,0x49,0x45,0x43},{0x00,0x7F,0x41,0x41,0x00},
    {0x02,0x04,0x08,0x10,0x20},{0x00,0x41,0x41,0x7F,0x00},{0x04,0x02,0x01,0x02,0x04},{0x40,0x40,0x40,0x40,0x40},
    {0x00,0x01,0x02,0x04,0x00},{0x20,0x54,0x54,0x54,0x78},{0x7F,0x48,0x44,0x44,0x38},{0x38,0x44,0x44,0x44,0x20},
    {0x38,0x44,0x44,0x48,0x7F},{0x38,0x54,0x54,0x54,0x18},{0x08,0x7E,0x09,0x01,0x02},{0x0C,0x52,0x52,0x52,0x3E},
    {0x7F,0x08,0x04,0x04,0x78},{0x00,0x44,0x7D,0x40,0x00},{0x20,0x40,0x44,0x3D,0x00},{0x7F,0x10,0x28,0x44,0x00},
    {0x00,0x41,0x7F,0x40,0x00},{0x7C,0x04,0x18,0x04,0x78},{0x7C,0x08,0x04,0x04,0x78},{0x38,0x44,0x44,0x44,0x38},
    {0x7C,0x14,0x14,0x14,0x08},{0x08,0x14,0x14,0x18,0x7C},{0x7C,0x08,0x04,0x04,0x08},{0x48,0x54,0x54,0x54,0x20},
    {0x04,0x3F,0x44,0x40,0x20},{0x3C,0x40,0x40,0x20,0x7C},{0x1C,0x20,0x40,0x20,0x1C},{0x3C,0x40,0x30,0x40,0x3C},
    {0x44,0x28,0x10,0x28,0x44},{0x0C,0x50,0x50,0x50,0x3C},{0x44,0x64,0x54,0x4C,0x44},{0x00,0x08,0x36,0x41,0x00},
    {0x00,0x00,0x7F,0x00,0x00},{0x00,0x41,0x36,0x08,0x00},{0x10,0x08,0x08,0x10,0x08}
};

#define CELL 6                  /* a glyph's advance: 5 columns and a gap */
#define ROW_H 12

typedef struct { u32 *px; int stride; int f; } Canvas;

static void rect(const Canvas *c, int x, int y, int w, int h, u32 color) {   /* units */
    const int f = c->f;
    for (int yy = y * f; yy < (y + h) * f; yy++) {
        u32 *row = c->px + (u64)yy * c->stride;
        for (int xx = x * f; xx < (x + w) * f; xx++) row[xx] = color;
    }
}

static int text_len(const char *s) { int n = 0; while (s[n]) n++; return n; }

static void text(const Canvas *c, int x, int y, const char *s, u32 color) {  /* units */
    for (; *s; s++, x += CELL) {
        int ch = (unsigned char)*s;
        if (ch < 32 || ch > 126) ch = '?';
        const u8 *g = FONT[ch - 32];
        for (int ci = 0; ci < 5; ci++)
            for (int bit = 0; bit < 7; bit++)
                if (g[ci] & (1 << bit)) rect(c, x + ci, y + bit, 1, 1, color);
    }
}

void im_menu_paint(u32 *px, int stride_px, int f, int *pw, int *ph) {
    if (f < 1) f = 1;
    if (f > 4) f = 4;
    *pw = PANEL_W * f;
    *ph = PANEL_H * f;
    const int pst = im_persist_state();
    if (g_painted_ver == g_ver && g_painted_f == f && g_painted_st == pst) return;
    g_painted_ver = g_ver;
    g_painted_f = f;
    g_painted_st = pst;
    g_menu_dbg[M_PAINTS]++;

    const Canvas c = {px, stride_px, f};
    const u32 BG = col(0.07f, 0.08f, 0.13f), BORDER = col(1.0f, 0.82f, 0.12f), GOLD = col(1.0f, 0.82f, 0.12f);
    const u32 TEXT = col(1.0f, 1.0f, 1.0f), DIM = col(0.62f, 0.64f, 0.72f), SELBG = col(0.26f, 0.22f, 0.08f);
    const u32 TABBG = col(0.20f, 0.22f, 0.36f), ON = col(0.40f, 0.95f, 0.45f), OFF = col(0.95f, 0.38f, 0.38f);
    const u32 LINE = col(0.30f, 0.32f, 0.45f);

    rect(&c, 0, 0, PANEL_W, PANEL_H, BG);
    rect(&c, 0, 0, PANEL_W, 2, BORDER);
    rect(&c, 0, PANEL_H - 2, PANEL_W, 2, BORDER);
    rect(&c, 0, 0, 2, PANEL_H, BORDER);
    rect(&c, PANEL_W - 2, 0, 2, PANEL_H, BORDER);

    text(&c, 8, 8, "SUPER HERO BOOST", GOLD);
    {   /* whether what is set now is what a restart gives */
        const int st = im_persist_state();
        const char *t = st == 0 ? T("saved", "guardado") : (st == 2 || st == 1 ? T("saving...", "guardando...") : T("not saved", "sin guardar"));
        text(&c, PANEL_W - 10 - text_len(t) * CELL, 8, t, st == 0 ? ON : (st == 3 ? OFF : DIM));
    }

    int x = 8;
    for (int p = 0; p < PAGE_N; p++) {
        const char *name = page_label(p);
        const int w = (text_len(name) + 2) * CELL;
        if (p == g_page) rect(&c, x, 20, w, 11, TABBG);
        text(&c, x + CELL, 22, name, p == g_page ? TEXT : DIM);
        x += w + 4;
    }
    rect(&c, 8, 34, PANEL_W - 16, 1, LINE);

    u8 rows[MAX_ROWS];
    const int n = page_rows(g_page, rows);
    for (int i = 0; i < VIS_ROWS; i++) {
        const int r = g_top + i;
        if (r >= n) break;
        const int id = rows[r], y = 40 + i * ROW_H, sel = r == g_sel, sub = row_is_sub(id);
        if (sel) rect(&c, 4, y - 2, PANEL_W - 8, ROW_H, SELBG);
        char label[48], val[40], shown[48], full[48];
        row_label(id, label);
        int k = 0;
        if (sub) { shown[k++] = ' '; shown[k++] = '-'; shown[k++] = ' '; }
        for (int j = 0; label[j] && k < 44; j++) shown[k++] = label[j];
        shown[k] = 0;
        text(&c, 10, y, shown, sel ? TEXT : (sub ? DIM : TEXT));
        const int kind = row_value(id, val);
        int m = 0;
        if (sel) { full[m++] = '<'; full[m++] = ' '; }
        for (int j = 0; val[j] && m < 42; j++) full[m++] = val[j];
        if (sel) { full[m++] = ' '; full[m++] = '>'; }
        full[m] = 0;
        u32 vcol = GOLD;
        if (kind & V_ONOFF) vcol = (kind & V_ON) ? ON : OFF;
        text(&c, PANEL_W - 10 - m * CELL, y, full, vcol);
    }
    if (g_top > 0) text(&c, PANEL_W - 14, 22, "^", GOLD);
    if (g_top + VIS_ROWS < n) text(&c, PANEL_W - 14, 22, "v", GOLD);

    rect(&c, 8, 40 + VIS_ROWS * ROW_H + 2, PANEL_W - 16, 1, LINE);
    text(&c, 8, 40 + VIS_ROWS * ROW_H + 8, T("UP/DOWN select  LEFT/RIGHT change  L/R page  B close",
                                              "ARR/ABA elegir  IZQ/DER cambiar  L/R pagina  B cerrar"), DIM);
}
