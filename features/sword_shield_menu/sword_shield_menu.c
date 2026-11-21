/* Sword / shield quick menu: hold a button and pick the equipped sword or shield without the pause menu.
 *
 *   hold the button        open the drawer (after OPEN_FRAMES)
 *   D-pad up / down        change row: swords or shields
 *   D-pad left / right     move through the row
 *   release the button     equip the selected cell and close
 *
 * The first cell of each row is empty: selecting it unequips that category (the item stays in the pouch).
 * Only items the player owns are listed (IsGet.<actor>, the same check the quick select uses), and the age
 * rules apply: Kokiri sword for the child, Master / Giant / Biggoron for the adult; Deku and Hylian shields
 * for both, Mirror Shield for the adult. The broken Giant's Knife is not listed as a sword.
 *
 * The drawer is drawn by core/hud.c (drawer_update): this feature only owns the state and the strings.
 * Input runs as an *early* handler (before button_mapping's remap), so the physical button is consumed before
 * the remap turns it into a virtual one. The drawer button and the D-pad never reach the game while it is open.
 *
 * Equipment is written the same way the quick select does: GameData String64, live value and the copy the
 * menu/save keep, then the game's own equip request (the path the pause menu uses). The code is duplicated on
 * purpose so the feature builds without the quick select.
 */
#include "../../core/dispatch.h"
#include "../../core/config.h"
#include "../../core/hid.h"
#include "../../core/remap.h"

#define GD_S64_EQUIP_WEAPON 23u   /* core/hud.c: PlayerStatus.EquipWeapon */
#define GD_S64_EQUIP_SHIELD 22u   /* core/hud.c: PlayerStatus.EquipShield */

extern char GAMEDATA_MGR[];       /* link.ld: main+0x4E842A8 */
extern char ITEM_MGR_SLOT[];      /* link.ld: main+0x4C7E0D0 (-> item/equipment manager) */
extern void equip_request(void *pouch, void *name_ref, u32 kind, u32 arg3);
extern void equip_refresh(void *pouch, void *name_ref, u32 one);

int hud_owns_known(const char *actor);   /* core/hud.c: IsGet.<actor>; -1 when the flag does not exist */
int hud_is_child(void);                 /* core/hud.c: Sequence_IsChildhood, -1 unknown */
u32 hud_frame_count(void);
int player_active(void);
int player_dead(void);
int hud_game_covered(void);              /* a menu covers the game */

#define OPEN_FRAMES  12u   /* the button must be held this many frames to open (0.2 s) */
#define REPEAT_DELAY 16u   /* frames before the D-pad repeats a move */
#define REPEAT_RATE   6u
#define MAX_CELLS     5     /* empty + up to four swords */

#define DPAD_LEFT  0x1000ull
#define DPAD_UP    0x2000ull
#define DPAD_RIGHT 0x4000ull
#define DPAD_DOWN  0x8000ull
#define DPAD_ALL   (DPAD_LEFT | DPAD_UP | DPAD_RIGHT | DPAD_DOWN)

#define AGE_CHILD 1u
#define AGE_ADULT 2u

typedef struct { const char *name; const char *label; unsigned char ages; } Entry;

/* Order as in the pause menu. */
static const Entry SWORDS[] = {
    {"Weapon_Sword_Kokiri",   "Kokiri",   AGE_CHILD},
    {"Weapon_Sword_Master",   "Master",   AGE_ADULT},
    {"Weapon_Sword_Giant",    "Giant",    AGE_ADULT},
    {"Weapon_Sword_Biggoron", "Biggoron", AGE_ADULT},
};
static const Entry SHIELDS[] = {
    {"Armor_Shield_Deku",   "Deku",   AGE_CHILD | AGE_ADULT},
    {"Armor_Shield_Hylian", "Hylian", AGE_CHILD | AGE_ADULT},
    {"Armor_Shield_Mirror", "Mirror", AGE_ADULT},
};
#define SWORD_N ((int)(sizeof(SWORDS) / sizeof(SWORDS[0])))
#define SHIELD_N ((int)(sizeof(SHIELDS) / sizeof(SHIELDS[0])))

static int g_open;
static int g_row, g_sel;
static int g_n[2];
static const char *g_lbl[2][MAX_CELLS];
static char g_name[2][MAX_CELLS][64];
static int g_eq[2][MAX_CELLS];
static u32 g_hold, g_latch, g_release, g_last_frame;
static u64 g_prev_dpad;
static u32 g_repeat, g_repeat_t;

static int same(const char *a, const char *b) {
    u32 i = 0;
    while (a[i] && a[i] == b[i]) i++;
    return a[i] == b[i];
}
static void copy64(char *dst, const char *src) {
    u32 i = 0;
    for (; i < 63u && src[i]; i++) dst[i] = src[i];
    dst[i] = 0;
}

/* ---- GameData String64 (same layout as features/quick_select) ---- */
static void gd_put_group(unsigned char *rec, u32 ptr_off, u32 buf_off, const char *name) {
    u64 ptr = *(volatile u64 *)(rec + ptr_off);
    unsigned char *val = ptr ? (unsigned char *)ptr : rec + buf_off;
    u32 n = 0;
    while (name[n] && n < 63u) { val[n] = (unsigned char)name[n]; n++; }
    val[n] = 0;
    for (u32 i = n + 1u; i < 64u; i++) val[i] = 0;
    if (ptr) {   /* keep the record's inline buffer in step as well */
        for (u32 i = 0; i <= n; i++) rec[buf_off + i] = val[i];
        for (u32 i = n + 1u; i < 64u; i++) rec[buf_off + i] = 0;
    }
}
static void gd_set_string(u32 index, const char *name) {
    u64 mgr = *(volatile u64 *)GAMEDATA_MGR;
    if (!mgr) return;
    u64 table = mgr + 8ull + 0x60ull * 16ull;      /* String64 */
    u32 count = *(volatile u32 *)(table + 8);
    u64 recs = *(volatile u64 *)(table + 0x10);
    if (!recs || index >= count) return;
    unsigned char *rec = (unsigned char *)(recs + (u64)index * 0x108ull);
    gd_put_group(rec, 0x68, 0x74, name);   /* live value */
    gd_put_group(rec, 0xB8, 0xC4, name);   /* the copy the menu and the save keep */
}
static void gd_get_string(u32 index, char *out, u32 cap) {
    u64 mgr = *(volatile u64 *)GAMEDATA_MGR;
    out[0] = 0;
    if (!mgr) return;
    u64 table = mgr + 8ull + 0x60ull * 16ull;
    u32 count = *(volatile u32 *)(table + 8);
    u64 recs = *(volatile u64 *)(table + 0x10);
    if (!recs || index >= count) return;
    const unsigned char *rec = (const unsigned char *)(recs + (u64)index * 0x108ull);
    u64 ptr = *(volatile u64 *)(rec + 0x68);
    const char *val = ptr ? (const char *)ptr : (const char *)(rec + 0x74);
    u32 i = 0;
    while (val[i] && i + 1u < cap) { out[i] = val[i]; i++; }
    out[i] = 0;
}

/* Apply a name through the game's own equip request (the pause menu's path). An empty name unequips. */
typedef struct { const char *name; u64 len; } NameRef;
static void apply_equip(const char *name) {
    u64 slot = *(volatile u64 *)ITEM_MGR_SLOT;
    u64 mgr = slot ? *(volatile u64 *)slot : 0;
    if (!mgr || !name || !name[0]) return;   /* unequip: the empty String64 is enough (see close_menu) */
    NameRef ref = { name, 64u };
    equip_request((void *)mgr, &ref, 3u, 0u);
    equip_refresh((void *)mgr, &ref, 1u);
}

/* Add one row's owned entries. Returns the number of cells, blank first. */
static int build_row(int row) {
    const Entry *tab = row ? SHIELDS : SWORDS;
    int tab_n = row ? SHIELD_N : SWORD_N;
    int child = hud_is_child();
    int n = 1;   /* the empty cell */
    g_name[row][0][0] = 0;
    g_lbl[row][0] = "None";
    g_eq[row][0] = 0;
    for (int i = 0; i < tab_n && n < MAX_CELLS; i++) {
        if (child == 1 && !(tab[i].ages & AGE_CHILD)) continue;
        if (child == 0 && !(tab[i].ages & AGE_ADULT)) continue;
        if (hud_owns_known(tab[i].name) == 0) continue;   /* known and not owned; -1 = no flag: listed */
        copy64(g_name[row][n], tab[i].name);
        g_lbl[row][n] = tab[i].label;
        g_eq[row][n] = 0;
        n++;
    }
    return n;
}

/* Mark the equipped cell of a row (String64 23 = weapon, 22 = shield). */
static void mark_equipped(int row) {
    char cur[64];
    gd_get_string(row ? GD_S64_EQUIP_SHIELD : GD_S64_EQUIP_WEAPON, cur, sizeof cur);
    for (int i = 0; i < g_n[row]; i++) g_eq[row][i] = cur[0] && same(cur, g_name[row][i]);
}

static void select_equipped(int row) {
    g_sel = 0;
    for (int i = 0; i < g_n[row]; i++) if (g_eq[row][i]) { g_sel = i; break; }
}

static void open_menu(void) {
    g_n[0] = build_row(0);
    g_n[1] = build_row(1);
    mark_equipped(0);
    mark_equipped(1);
    if (g_n[0] <= 1 && g_n[1] <= 1) return;   /* nothing to choose: stay closed */
    if (g_shbcfg.ssm_flags & 1u) g_row = 0;
    if (g_row < 0 || g_row > 1) g_row = 0;
    select_equipped(g_row);
    g_open = 1;
    g_prev_dpad = 0;
    g_repeat = g_repeat_t = 0;
}

static void close_menu(int apply) {
    if (apply && g_open) {
        const char *name = g_name[g_row][g_sel];
        gd_set_string(g_row ? GD_S64_EQUIP_SHIELD : GD_S64_EQUIP_WEAPON, name);
        apply_equip(name);
    }
    g_open = 0;
    g_latch = 1;
    g_release = 0;
    g_prev_dpad = 0;
}

static void move(int dir) {
    int n = g_n[g_row];
    if (n <= 0 || dir == 0) return;
    int s = g_sel + dir;
    if (s < 0) s = n - 1;
    if (s >= n) s = 0;
    g_sel = s;
}

void ssm_on_input(u32 site, void *a0, void *a1, void *a2, u32 stock_result) {
    (void)a2;
    if (site != 3 || !a0 || !g_shbcfg.ssm_enabled) {
        if (g_open) close_menu(0);
        return;
    }
    u32 count = (u32)(unsigned long)a1;
    if (count == 0 || count > 64) count = stock_result;
    if (count == 0 || count > 64) return;

    HidView view = {a0, count, 0};
    u64 raw = hid_merge_buttons(&view);

    u32 now = hud_frame_count();
    u32 new_frame = now != g_last_frame;
    if (new_frame) g_last_frame = now;

    /* The button is the remap's drawer role. With the remap on and no button assigned there is no drawer;
     * the classic layout (no remap) falls back to L. */
    u32 pb = remap_role_phys(ROLE_DRAWER);
    if (pb >= PB_N) {
        if (remap_on()) { if (g_open) close_menu(0); return; }
        pb = PB_L;   /* PB_L = 0 */
    }
    u64 bit = REMAP_BIT[pb];

    int playing = player_active() && !player_dead() && !hud_game_covered();
    if (!playing) {
        if (g_open) close_menu(0);
        g_hold = 0;
        g_latch = 0;
        g_prev_dpad = 0;
        return;
    }

    u64 buttons = raw;
    /* Consume the button before the remap sees it; while open, the D-pad too. */
    if (g_open || (buttons & bit)) {
        u64 mask = bit | (g_open ? DPAD_ALL : 0);
        for (u32 i = 0; i < count; i++) {
            u64 b = hid_get_buttons(&view, i);
            u64 v = b & ~mask;
            if (v != b) hid_set_buttons(&view, i, v);
        }
    }

    if (!(buttons & bit)) {
        if (g_open) close_menu(1);
        g_hold = 0;
        if (g_release < 0x10000000u) g_release++;
        if (g_latch && g_release >= 4u) { g_latch = 0; g_release = 0; }
        g_prev_dpad = 0;
        return;
    }
    g_release = 0;

    if (!g_open) {
        if (g_latch) return;
        if (new_frame) g_hold++;
        if (g_hold >= OPEN_FRAMES) {
            g_hold = 0;
            open_menu();
        }
        return;
    }
    if (!new_frame) return;

    /* navigation: an edge moves at once; holding repeats after REPEAT_DELAY, then every REPEAT_RATE */
    u64 dpad = raw & DPAD_ALL;
    u64 edge = dpad & ~g_prev_dpad;
    g_prev_dpad = dpad;
    if (edge & DPAD_UP) { g_row = 0; select_equipped(0); g_repeat = g_repeat_t = 0; }
    else if (edge & DPAD_DOWN) { g_row = 1; select_equipped(1); g_repeat = g_repeat_t = 0; }
    else if (edge & DPAD_LEFT) { move(-1); g_repeat = g_repeat_t = 0; }
    else if (edge & DPAD_RIGHT) { move(1); g_repeat = g_repeat_t = 0; }
    else if (dpad & (DPAD_LEFT | DPAD_RIGHT)) {
        if (++g_repeat_t >= (g_repeat ? REPEAT_RATE : REPEAT_DELAY)) {
            g_repeat = 1;
            g_repeat_t = 0;
            move((dpad & DPAD_RIGHT) ? 1 : -1);
        }
    } else {
        g_repeat = g_repeat_t = 0;
    }
}

/* ---- read-only API for core/hud.c ---- */
int ssm_hud_open(void) { return g_open; }
int ssm_hud_row(void) { return g_row; }
int ssm_hud_sel(void) { return g_sel; }
int ssm_hud_cells(int row) { return row == 1 ? g_n[1] : g_n[0]; }
const char *ssm_hud_cell(int row, int i) {
    if (row < 0 || row > 1 || i < 0 || i >= MAX_CELLS) return 0;
    return g_lbl[row][i];
}
int ssm_hud_equipped(int row, int i) {
    if (row < 0 || row > 1 || i < 0 || i >= MAX_CELLS) return 0;
    return g_eq[row][i];
}
