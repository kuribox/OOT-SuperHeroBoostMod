#include "../../core/dispatch.h"
#include "../../core/hid.h"
#include "../../core/clock.h"
#include "../../core/config.h"
#include "../../core/remap.h"

/* button_mapping (F3, "buttonMapping"): our own extra item buttons on the HUD.
 *
 * Controls on this game: X jump, A dodge, Y sword, B / L / R / ZR are the free
 * buttons. In-game R equips an item into the HUD item box and ZR uses it
 * (hold and release). The stock function at 0x1211C00 is what that equip ends up
 * calling; its third argument is the item index (inventory slot order:
 * 0 stick, 1 nut, 3 boomerang ...).
 *
 * buttonMapping: B, L and R can each hold an item; an empty slot keeps its normal
 * behaviour. ZR is always the game's own item (what the R selector / pause menu A
 * put in the box); a slot never changes it. Pressing an assigned slot:
 *   1. EQUIP   - the slot's item is put in the game's item box (equip_now);
 *   2. HOLD    - ZR is held while the slot button is held;
 *   3. GAP     - once Link is free again for a while the use is over;
 *   4. RESTORE - the ZR item goes back in the box (retried until the box has it).
 * ZR pressed before that puts the ZR item back first (ZRBACK) and replays the press.
 * Only the player's own picks change the ZR item, so slot assignments are never
 * shuffled by what the box holds.
 */
#define SLOT_BASE 0x10000000u
u32 g_slot_count[4] = {SLOT_BASE, SLOT_BASE, SLOT_BASE, SLOT_BASE};

#define LOG_N 24
typedef struct { u32 site; u32 result; unsigned long long a0, a1, a2; } SlotLog;
SlotLog g_slot_log[LOG_N] = {{0x7FFFFFFFu, 0, 0, 0, 0}};   /* non-zero: lives in .data */
u32 g_slot_log_n = SLOT_BASE;

enum { SLOT_B = 0, SLOT_L = 1, SLOT_R = 2, SLOT_ZR = 3, SLOT_N = 4 };
int hud_button_swap(void);              /* core/hud.c: the game's Jump/B swap option is on */
#define BTN_RSTICK 0x0020ull   /* right stick click (R3) */
static const u64 SLOT_MASK[SLOT_N] = {BTN_B, BTN_L, BTN_R, BTN_ZR};
/* With the game's Jump/B swap, the B slot is the X button and B is jump. */
static u64 slot_mask(u32 s) {
    return (s == SLOT_B && hud_button_swap()) ? BTN_X : SLOT_MASK[s];
}
/* Manual shield owns L (L is the guard while the target camera is active), so
 * the L item slot is off while that option is enabled. The assignment is kept,
 * so turning the manual shield off restores it. */
static int slot_enabled(int s) {
    return !(s == SLOT_L && g_shbcfg.manual_shield && !remap_on());   /* remap: the shield has its own button */
}

/* Item index per slot, -1 = empty (button stays normal): the set of Link's current age. Assigned in the pause
 * menu's Items tab with X / L / R; empty at first. (.data) */
int g_assign[SLOT_N] = {-1, -1, -1, -1};
/* Both sets (0 child, 1 adult) for B / L / R, kept per save file (save_write / save_read). g_cur_age: the set
 * g_assign holds (-1: none yet, taken from g_set on the next gameplay frame). */
int g_set[2][SLOT_ZR] = {{-1, -1, -1}, {-1, -1, -1}};
int g_cur_age = -1;
static int g_check_owned;   /* frames left to drop slot items the save does not have (after a set is taken) */
int hud_is_child(void);          /* core/hud.c: Sequence_IsChildhood (-1: no GameData) */
int hud_box_child(void);         /* core/hud.c: the item box the game uses is the Child one (its item set) */
int hud_item_owned(int item);    /* core/hud.c: IsGet.<item> */

enum { PH_IDLE = 0, PH_EQUIP, PH_HOLD, PH_GAP, PH_RESTORE, PH_ZRBACK };
#define EQUIP_TICKS 4
#define HOLD_MIN 3
#define EQUIP_MAX 16
#define GAP_TICKS 20           /* input frames Link must be free after a use before the ZR item goes back */
#define GAP_BOOMERANG 90       /* the boomerang flies back while Link is free: wait for it */
/* items that stay in Link's hand after a use (not given back on their own, see PH_GAP) */
static int held_item(int item) { return item == 0; }   /* 0: Deku stick */
/* A while standing still with an item on B: B's own "put away" (sword, item in hand) is on A then. A reaches the
 * game first (talk, open, check ...); when Link is still standing A_PUTAWAY_WAIT reads later, A did nothing and the
 * B button's own press is sent for A_PUTAWAY_PRESS reads. */
#define A_PUTAWAY_WAIT 8
#define A_PUTAWAY_PRESS 4
static int g_a_watch, g_a_press;
static int g_zr_tap;           /* ZRBACK: ZR was let go before the box had the ZR item: replay the press */
static int g_zr_inject;        /* input frames ZR is still pressed for that replay */

int g_pending = -1;        /* item index to force on the next stock call (old R-press method, unused) */
int g_saved = -1;          /* item box content before a slot use (the ZR item) */
int g_borrowed;            /* a slot item is in the box; the ZR item goes back when ZR is pressed */
int g_borrow_item = -1;    /* the slot item in the box */
int g_zr_item = -1;        /* item the game's own buttons last selected */
int g_phase = PH_IDLE;
int g_active = -1;
u32 g_phase_t;
u64 g_cl_prev;
u32 g_cl_dbg[4] = {SLOT_BASE, SLOT_BASE, SLOT_BASE, SLOT_BASE};  /* presses, overrides, restores */

void hud_slot_item(u32 slot, int item);
int player_active(void);
int player_dead(void);                   /* core/player.c: death ragdoll / game over */
int player_state_is(const char *name);   /* core/player.c: Link's current action state */
int player_state_starts(const char *prefix);
const char *player_state_name(void);
char g_cl_refused[24];   /* diagnostic: Link's state when a slot press was ignored (not free) */
int hud_zr_item(void);
void hud_zr_hold(int hold);
int hud_ingame(void);   /* core/hud.c: 0 while a menu covers the game (pause ...) */
int hud_ingame_known(void);   /* core/hud.c: hud_ingame() reads the HUD (else it always says 1) */
int hud_game_covered(void);   /* core/hud.c: a menu covers the game (pause flag below, or the HUD flag) */
extern u32 g_game_paused;    /* core/hud.c: published copy of g_paused */
int hud_menu_items(void);   /* core/hud.c: the pause menu's Items tab is open */
int hud_menu_item(void);    /* core/hud.c: item index highlighted there (-1: none / not owned) */
void hud_menu_tick(void);   /* core/hud.c: draw the slot cluster on the menu page */
void hud_refresh_button_swap(void);   /* core/hud.c: re-read the game's Jump/B option */
void hud_menu_decide(void); /* core/hud.c: play the highlighted cell's Decide animation (the game's A feedback) */
int hud_menu_decided(void); /* core/hud.c: that animation is playing */
void hud_menu_zr(int item); /* core/hud.c: show an item on ZR now */
void hud_menu_off(void);    /* core/hud.c: not on the Items tab: hide the assign legend */
static int g_menu_a_item = -1, g_menu_a_wait;
static u32 g_menu_assign_t = 1000;   /* menu reads since the last assignment */
static int g_paused;        /* + pressed in gameplay: the pause menu is up */
/* A menu swap with ZR (X / L / R on the ZR item): the game's item box gets this item (-1 empty) once gameplay runs
 * again; -2 none. g_pending_snap: the box when it was made (a different box then = the player picked a ZR item with
 * A meanwhile, and that pick wins). g_sync_t: frames left for the box to take it before box changes count as the
 * player's picks again. */
static int g_box_pending = -2, g_sync_t;
static int g_a_pick = -1;        /* item A was pressed on in the menu after that swap (-1 none) */
static int g_menu_box = -3;
static int g_a_recent;           /* menu frames left since A was pressed */      /* the game's box on the previous menu frame (-3: not in the menu) */
int hud_box_item(void);          /* core/hud.c: the game's item box from GameData (-1 empty, -2 unknown) */
int hud_box_set(int item);       /* core/hud.c: write it (-1 empty) */
extern u32 g_songbook_active;    /* features/songbook: the ocarina is open */
/* The game's item box: GameData (exact, also under the menu); the HUD icon's item when GameData has no index for it. */
static int read_box(void) {
    int b = hud_box_item();
    return b != -2 ? b : hud_zr_item();
}
/* ZR's item is now `item` (the player's pick): the B / L / R button that had it gets ZR's old item, so no item is
 * on two buttons. */
#define ZR_UNKNOWN (-3)   /* another save / age: its box is taken as it is, nothing swapped */
static void zr_take(int item) {
    int old = g_borrowed ? g_saved : g_zr_item;
    if (item == old) return;
    if (item >= 0 && old != ZR_UNKNOWN) for (int s = 0; s < SLOT_ZR; s++) if (g_assign[s] == item) g_assign[s] = old;
    g_zr_item = item;
    if (g_borrowed) g_saved = item;
}

void hud_box_empty(int child);   /* core/hud.c */
static u32 g_run, g_run_frame;
u32 hud_frame_count(void);  /* core/hud.c */
#define BTN_PLUS 0x0400ull   /* A pressed on this item: did the game put it in its box? */   /* core/hud.c: keep showing the ZR item while a slot item borrows the item box */
/* The game's own "put this item in the item box" call, as the R button makes it (main+0x33C9AB4):
 * main+0x1211C00(item manager, item set, item index, 1, 1); manager = **(main+0x4C7E0D0),
 * set = byte +0x5C0 of **(main+0x4C7E2A8). Calling it directly equips without pressing R (R opens the selector). */
extern char SLOTFN[], ITEM_MGR_SLOT[], ITEM_SET_SLOT[];
u32 g_equip_dbg[2];   /* direct equips done, skipped (no manager) */
static void equip_now(int item) {
    u64 slot = *(u64 *)ITEM_MGR_SLOT, sets = *(u64 *)ITEM_SET_SLOT;
    u64 mgr = slot ? *(u64 *)slot : 0, so = sets ? *(u64 *)sets : 0;
    if (!mgr || !so || item < 0) { g_equip_dbg[1]++; return; }
    u64 set = *(volatile unsigned char *)(so + 0x5C0);
    ((void (*)(u64, u64, u64, u64, u64))(void *)SLOTFN)(mgr, set, (u64)item, 1, 1);
    g_equip_dbg[0]++;
}
/* Put an item in the box, or empty it (-1). */
static void box_put(int item) {
    if (item >= 0) { equip_now(item); return; }
    int ch = hud_box_child();   /* the box the game uses (its item set), not Sequence_IsChildhood */
    if (ch >= 0) hud_box_empty(ch);
}
/* No slot item in the box any more: the ZR item is the game's own again. */
static void unborrow(void) {
    g_borrowed = 0;
    hud_zr_hold(0);
}   /* core/hud.c: item index of the game's own (hidden) item box, -1 = unknown */

/* Items a slot can cycle through (inventory slot order) and their ItemPattern icon
 * frame, measured in game with the BUTTON_PROBE build (docs/RE-NOTES.md). */
static const int ITEM_CYCLE[] = {-1, 1, 2, 3, 4, 5, 6, 7, 9, 10, 13, 14, 18, 30, 31};
#define ITEM_CYCLE_N ((int)(sizeof(ITEM_CYCLE) / sizeof(ITEM_CYCLE[0])))
__attribute__((unused)) static int icon_of(int item) {
    switch (item) {
    case 0: return 0;   case 1: return 6;   case 2: return 4;   case 3: return 3;
    case 4: return 1;   case 5: return 33;  case 6: return 34;  case 7: return 32;
    case 9: return 5;   case 10: return 8;  case 13: return 2;  case 14: return 23;
    case 18: return 36; case 30: return 24; case 31: return 9;
    default: return 0;
    }
}
static u64 g_swallow;
__attribute__((unused)) static void cycle_slot(int s) {
    int pos = 0;
    for (int i = 0; i < ITEM_CYCLE_N; i++) if (ITEM_CYCLE[i] == g_assign[s]) pos = i;
    g_assign[s] = ITEM_CYCLE[(pos + 1) % ITEM_CYCLE_N];
}

/* ---- temporary probe: learn item index -> icon frame (BUTTON_PROBE) ---- */
extern float g_last_pat_frame, g_last_num_frame;
extern u32 g_pat_changes;
typedef struct { u32 idx; u32 result; float pat; float num; } MapEntry;
MapEntry g_map[40] = {{0x7FFFFFFFu, 0, 0, 0}};
u32 g_map_n = SLOT_BASE;
static u32 g_probe_t, g_probe_idx;
static int g_probe_wait;
#ifdef BUTTON_PROBE
static void probe_step(void) {
    g_probe_t++;
    if (g_probe_t < 3600) return;              /* let the game settle first (~30 s) */
    u32 phase = (g_probe_t - 3600) % 180;
    if (phase == 0 && g_probe_idx < 32) {
        g_pending = (int)g_probe_idx;
        g_phase = PH_EQUIP; g_phase_t = 0; g_active = 0;
        g_probe_wait = 1;
    }
    if (phase == 150 && g_probe_wait) {
        MapEntry *m = &g_map[(g_map_n - SLOT_BASE) % 40];
        g_map_n++;
        m->idx = g_probe_idx;
        m->result = g_slot_log[(g_slot_log_n - SLOT_BASE - 1) % LOG_N].result;
        m->pat = g_last_pat_frame;
        m->num = g_last_num_frame;
        g_probe_idx++;
        g_probe_wait = 0;
    }
}
#endif

static void set_all(const HidView *v, u64 clear, u64 set) {
    for (u32 i = 0; i < v->count; i++) {
        u64 b = hid_get_buttons(v, i);
        hid_set_buttons(v, i, (b & ~clear) | set);
    }
}

static void dbg_tick(void);   /* diagnostic dump (below, beside the save code) */

/* ---- button remap (core/remap.h) ----
 * Runs before every other input handler (tools/build_core.py FEATURE_INPUT_EARLY): the buttons the player pressed
 * become the virtual ones of their roles, so every feature after it (and the game) sees the default layout. Only
 * while Link plays: menus (pause, map, game over), the title screen and the ocarina keep the real buttons, as the
 * game's own prompts there name them. The game fills the samples again on every read, so this is done once per read. */
u32 g_remap_dbg[3];   /* reads remapped, reads left alone, last physical -> virtual (low bits) */
u32 g_remap_live;     /* the last read was remapped: the HUD prompts name the roles' buttons (core/hud.c) */
/* Held buttons: D-pad right (its short press is the game's Navi call) and A / B / X / Y (whose short press is their own
 * role) can have a role for the LONG press (remap_hold_role). Per button: the frame the press went down, the replay
 * reads left of a short press, and whether the long press already fired. */
#define HOLD_N 5
static const unsigned char HOLD_PB[HOLD_N] = {PB_DR, PB_A, PB_B, PB_X, PB_Y};
static u32 hold_since[HOLD_N], hold_pulse[HOLD_N];
static unsigned char hold_fired[HOLD_N];
u32 g_remap_dr[2];   /* diagnostics: short presses replayed, long presses that fired a role (all held buttons) */
u32 g_remap_hold[HOLD_N];   /* diagnostics: long presses that fired, per button: DR A B X Y */
static void hold_reset(void) {
    for (u32 h = 0; h < HOLD_N; h++) { hold_since[h] = hold_pulse[h] = 0; hold_fired[h] = 0; }
}
void button_remap_on_input(u32 site, void *a0, void *a1, void *a2, u32 stock_result) {
    (void)a2;
    if (site != 3 || !a0) return;
    if (!remap_on()) { g_remap_live = 0; return; }
    u32 count = hid_sample_count(a1, stock_result);
    if (count == 0 || count > 64) return;
    /* Gameplay: the player updated recently. player_active() alone drops for a few frames in hit stops and
     * transitions, and a real button must never reach the game there with its own function. */
    static u32 last_active;
    u32 now = hud_frame_count();
    if (player_active()) last_active = now ? now : 1;
    int playing = last_active && now - last_active < 60u;
    /* (no g_paused here: the pause menu clears hud_ingame by itself, and a remap must never stay off because a
     * pause flag was missed) */
    /* A menu covers the game, or the pause menu is up (+ pressed, until gameplay runs again): the real buttons.
     * Both: the HUD flag alone is not steady through the whole pause menu, and a remap switching on and off
     * there turned one press into two (the second unassigned the item). */
    int covered = hud_game_covered();
    /* Evt*: Link is in an event (talking, a shop, a chest, a cutscene). The game's text boxes and shop menus read
     * A / B as next / back, so the real buttons reach them: a remapped action on B confirmed purchases there. */
    if (!playing || covered || g_songbook_active || player_dead() || player_state_starts("Evt")) {
        g_remap_dbg[1]++;
        g_remap_live = 0;
        hold_reset();
        return;
    }
    g_remap_live = 1;
    HidView view = {a0, count, 0};
    /* Held buttons: short press = the button's own role (Navi for D-pad right), long press = its hold role. The press is
     * held back until it is known which: released early it is replayed as a short pulse (the physical bit goes through
     * the remap, so the short role fires), held it never reaches the game as the short one. */
    u64 add = 0, clear = 0, pulse_add = 0, all = 0;
    for (u32 i = 0; i < count; i++) all |= hid_get_buttons(&view, i);
    for (u32 h = 0; h < HOLD_N; h++) {
        u32 p = HOLD_PB[h], role = remap_hold_role(p);
        u64 bit = REMAP_BIT[p];
        if (role == ROLE_NONE || role >= ROLE_N) { hold_since[h] = hold_pulse[h] = 0; hold_fired[h] = 0; continue; }
        if (all & bit) {
            if (!hold_since[h]) { hold_since[h] = now ? now : 1; hold_fired[h] = 0; hold_pulse[h] = 0; }
            if (now - hold_since[h] >= REMAP_DR_HOLD) {
                if (!hold_fired[h]) { g_remap_dr[1]++; g_remap_hold[h]++; }
                hold_fired[h] = 1;
                add |= remap_role_virt(role);
                clear |= bit;                       /* the short role lets go, the hold role takes over */
            } else if (p == PB_DR) {
                clear |= bit;                       /* Navi (the arrow's short press) waits to know which press it is */
            }                                       /* A / B / X / Y: the short role goes through at once, no delay */
        } else {
            if (p == PB_DR && hold_since[h] && !hold_fired[h]) { hold_pulse[h] = 3; g_remap_dr[0]++; }   /* a short press of the arrow: let the game see it now, for three reads */
            hold_since[h] = 0;
            hold_fired[h] = 0;
        }
        if (hold_pulse[h] && !(all & bit)) { pulse_add |= bit; hold_pulse[h]--; }
    }
    for (u32 i = 0; i < count; i++) {
        u64 b = hid_get_buttons(&view, i);
        u64 v = remap_apply((b & ~clear) | pulse_add) | add;
        if (v != b) hid_set_buttons(&view, i, v);
        if (i == 0) g_remap_dbg[2] = (u32)(b & 0x3FF) | ((u32)(v & 0x3FF) << 16);
    }
    g_remap_dbg[0]++;
}

/* Every input call the module sees, per site: hits, arguments, count and the first buttons word. */
u64 g_dbg_a0[5], g_dbg_a1[5];
u32 g_dbg_cnt[5], g_dbg_btn[5], g_dbg_hits[5];

/* The back role (VBTN_BACK, core/remap.h): seen in this read, and whether it may reach the game as B now. */
static u32 g_back_seen, g_back_ok;
u32 g_back_dbg[2];   /* diagnostics: reads with the back role down, reads where it was turned into B */
static void bm_input(u32 site, void *a0, void *a1, void *a2, u32 stock_result);
void button_mapping_on_input(u32 site, void *a0, void *a1, void *a2, u32 stock_result) {
    g_back_seen = 0;
    g_back_ok = 1;
    bm_input(site, a0, a1, a2, stock_result);
    if (site != 3 || !a0 || !g_back_seen) return;
    g_back_dbg[0]++;
    if (!g_back_ok) return;
    u32 count = hid_sample_count(a1, stock_result);
    if (count == 0 || count > 64) return;
    HidView view = {a0, count, 0};
    u64 bit = remap_role_virt(ROLE_ITEM1);   /* the game's B (X with the Jump/B swap) */
    for (u32 i = 0; i < count; i++) hid_set_buttons(&view, i, hid_get_buttons(&view, i) | bit);
    g_back_dbg[1]++;
}

static void bm_input(u32 site, void *a0, void *a1, void *a2, u32 stock_result) {
    if (site < 5) {
        HidView dv = {a0, 0, 0};
        u32 dcount = hid_sample_count(a1, stock_result);
        if (dcount > 64) dcount = 0;
        g_dbg_hits[site]++;
        g_dbg_a0[site] = (u64)(unsigned long)a0;
        g_dbg_a1[site] = (u64)(unsigned long)a1;
        g_dbg_cnt[site] = dcount;
        dv.count = dcount;
        g_dbg_btn[site] = (u32)hid_get_buttons(&dv, 0);
    }
    dbg_tick();
    (void)a2;
    if (site != 3 || !a0) return;
    u32 count = hid_sample_count(a1, stock_result);
    if (count == 0 || count > 64) return;
    HidView view = {a0, count, 0};
    /* the shield role's virtual bit is the manual shield's (it ran before): the game never sees it */
    /* and the selector role's (remap): remembered here, turned into the game's R further down */
    u32 sel_held = 0;
    static u32 sel_prev;
    for (u32 i = 0; i < count; i++) {
        u64 b = hid_get_buttons(&view, i);
        if (b & VBTN_SELECTOR) sel_held = 1;
        if (b & VBTN_BACK) g_back_seen = 1;
        if (b & (VBTN_SHIELD | VBTN_SELECTOR | VBTN_BACK)) hid_set_buttons(&view, i, b & ~(VBTN_SHIELD | VBTN_SELECTOR | VBTN_BACK));
    }
    u32 sel_was = sel_prev;
    sel_prev = sel_held;

    /* Death ragdoll / game over: do not remap or swallow anything, the game's screen needs every button. */
    if (player_dead()) {
        g_phase = PH_IDLE; g_swallow = 0;
        g_cl_prev = hid_merge_buttons(&view);
        return;
    }

    /* Link's age picks the set; an item the player does not have (a new game, an older save) leaves its slot */
    if (g_shbcfg.button_mapping && player_active()) {
        int ch = hud_is_child();
        if (ch >= 0) {
            int age = ch ? 0 : 1;
            if (age != g_cur_age) {
                if (g_cur_age >= 0) for (int s = 0; s < SLOT_ZR; s++) g_set[g_cur_age][s] = g_assign[s];
                for (int s = 0; s < SLOT_ZR; s++) g_assign[s] = g_set[age][s];
                g_cur_age = age;
                g_check_owned = shb_frames(30);   /* a set was just taken: drop what this save does not have (a few frames) */
                /* each age has its own item box: nothing of the other age's is borrowed in it */
                if (g_borrowed) unborrow();
                g_phase = PH_IDLE;
                g_zr_item = ZR_UNKNOWN; g_box_pending = -2; g_sync_t = 0;
            }
            /* Only right after a set is taken (save loaded, age changed), never while playing: an item assigned from
             * the pause menu is one the menu shows, and the game's IsGet flags are not a full record (an item that
             * never played its "you got" scene keeps it off). Re-checked for a few frames while GameData settles. */
            if (g_check_owned > 0) {
                g_check_owned--;
                for (int s = 0; s < SLOT_ZR; s++) if (g_assign[s] >= 0 && !hud_item_owned(g_assign[s])) g_assign[s] = -1;
            }
        }
    }

    /* L as the game's item selector (app option): L never holds an item */
    if (remap_l_selector()) g_assign[SLOT_L] = -1;

    /* The item box the game uses (its item set) switched without the age flag: the other box is taken as it is,
     * like an age change (nothing borrowed in it, no swap with the slots). */
    if (g_shbcfg.button_mapping) {
        static int box_set = -1;
        int bs = hud_box_child();
        if (bs >= 0 && bs != box_set) {
            if (box_set >= 0) {
                if (g_borrowed) unborrow();
                g_phase = PH_IDLE;
                g_zr_item = ZR_UNKNOWN; g_box_pending = -2; g_sync_t = 0;
            }
            box_set = bs;
        }
    }

    /* Publish the slots to the HUD (core/hud.c): B, L, R as assigned (ZR mirrors the game's own box). */
    for (int s = 0; s < SLOT_N; s++)
        hud_slot_item((u32)s, !slot_enabled(s) ? -1 : (s == SLOT_ZR && g_assign[s] < 0 ? (g_zr_item >= 0 ? g_zr_item : -1) : g_assign[s]));

    u64 buttons = hid_merge_buttons(&view);
    u64 pressed = buttons & ~g_cl_prev;
    g_cl_prev = buttons;

    /* Pause menu, Items tab: X, L and R assign the highlighted item to the B, L and R slots (like the item buttons;
     * B stays "back"). An item already on another slot swaps with it. The game's ZL / ZR still change tabs. */
    /* The pause menu is open only after + was pressed in gameplay, until gameplay runs again (the Items panel exists
     * from boot and looks "open" on the title screen and while loading, where its cells are not ready). */
    {   /* The pause menu is up from the + press until gameplay runs again: 10 HUD frames in a row in which the
         * player was updated (Link is frozen under the menu, so his frames never count there, whatever the HUD
         * frame count or the HUD's own "in game" flag do). Reads that land on the same frame count neither way:
         * the game can read the pad more than once per frame. */
        u32 f = hud_frame_count();
        if ((pressed & BTN_PLUS) && player_active()) { g_paused = 1; g_run = 0; }
        else if (g_paused && f != g_run_frame) {
            g_run = player_active() ? g_run + 1 : 0;
            if (g_run > 10) g_paused = 0;
        }
        g_run_frame = f;
    }
    g_game_paused = (u32)g_paused;
    int menu = g_paused && hud_menu_items();
    if (g_menu_assign_t < 1000) g_menu_assign_t++;
    if (g_shbcfg.button_mapping && !menu) { hud_menu_off(); g_menu_box = -3; }
    if (g_shbcfg.button_mapping && menu) {
        /* -> SLOT_B, SLOT_L, SLOT_R: the slot's own button when the menu lets it (X / Y / L / R), else a free one
         * (classic layout: X, L, R). No button = the slot is on no button: nothing to assign. */
        u64 MENU_BTN[SLOT_ZR], menu_mask = 0;
        {
            u32 mb[3];
            remap_menu_buttons(mb);
            for (int s = 0; s < SLOT_ZR; s++) {
                MENU_BTN[s] = mb[s] < PB_N ? REMAP_BIT[mb[s]] : 0;
                menu_mask |= MENU_BTN[s];
            }
        }
        for (int s = 0; s < SLOT_ZR; s++) {
            if (!MENU_BTN[s] || !(pressed & MENU_BTN[s])) continue;
            if (s == SLOT_L && (remap_l_selector() || (g_shbcfg.manual_shield && !remap_on()))) continue;   /* L: selector / shield */
            int it = hud_menu_item();
            if (it < 0) continue;
            int old = g_assign[s];
            /* the same button again: unassign (the button works as normal). Only a real second press: not within
             * 20 reads of the assignment, so a repeated edge of the same press never undoes it. */
            if (old == it && g_menu_assign_t <= 20) continue;   /* (the same press seen again: nothing) */
            if (old == it) g_assign[s] = -1;
            else {
                /* each item on one button only: the one that had it gets this button's old item (ZR too) */
                for (int o = 0; o < SLOT_ZR; o++) if (o != s && g_assign[o] == it) g_assign[o] = old;
                int zr = g_borrowed ? g_saved : g_zr_item;
                /* The ZR item onto an empty button: the game cannot be left with an empty box (it puts the item
                 * back when the menu closes, which undid this assignment), so the item stays on ZR as well. */
                if (it == zr && old < 0) {
                    g_assign[s] = it;
                    g_cl_dbg[3]++;
                    g_menu_assign_t = 0;
                    hud_menu_decide();
                    continue;
                }
                if (it == zr) {
                    g_a_pick = -1;
                    g_zr_item = old;
                    if (g_borrowed) g_saved = old;   /* goes back in the box after the use */
                    else {
                        g_box_pending = old;         /* the game's own equip once gameplay runs again */
                        hud_box_set(old);            /* and the box itself now, so the menu closes with it */
                        g_menu_box = read_box();     /* our own change, not the player's pick */
                    }
                    hud_menu_zr(old);
                }
                g_assign[s] = it;
            }
            g_cl_dbg[3]++;
            g_menu_assign_t = 0;
            hud_menu_decide();
        }
        /* A is the game's own: it puts the item in its box (ZR). When the game's box changes here, that is the
         * player's pick: the button that had it gets the old ZR item. Otherwise the box is checked again on close
         * against the item A was pressed on. */
        {
            int mb = read_box();
            if (g_a_recent > 0) g_a_recent--;
            if (g_menu_box != -3 && mb != g_menu_box && mb != -2 && g_a_recent > 0 &&
                !(g_borrowed && mb == g_saved)) {   /* only right after an A: the game's own changes are not picks */
                zr_take(mb);
                g_box_pending = -2; g_a_pick = -1;
                hud_menu_zr(mb);
            }
            g_menu_box = mb;
        }
        if (pressed & BTN_A) {
            g_a_recent = 40;
            g_menu_a_item = hud_menu_item(); g_menu_a_wait = 20;
            if (g_box_pending != -2 && g_menu_a_item >= 0) g_a_pick = g_menu_a_item;
        }
        if (g_menu_a_wait > 0) {   /* show the pick on ZR at once */
            g_menu_a_wait--;
            if (g_menu_a_item >= 0 && hud_menu_decided()) { hud_menu_zr(g_menu_a_item); g_menu_a_wait = 0; }
        }
        for (int s = 0; s < SLOT_ZR; s++) hud_slot_item((u32)s, slot_enabled(s) ? g_assign[s] : -1);   /* show a swap at once */
        hud_refresh_button_swap();
        hud_menu_tick();
        set_all(&view, menu_mask, 0);
        g_phase = PH_IDLE; g_swallow = 0;
        return;
    }

    /* L as the game's item selector: outside menus, L does what R does (opens the selector for ZR).
     * The R slot is suppressed for this frame so an item assigned to R is not used as well. */
    u32 l_selector = 0;
    if (g_shbcfg.button_mapping && (!g_shbcfg.manual_shield || remap_on()) && remap_selector_on() && hud_ingame() &&
        !(buttons & BTN_R)) {
        if (remap_on() && sel_held) {   /* the role's own button: L stays whatever its own role is */
            set_all(&view, 0, BTN_R);
            buttons |= BTN_R;
            if (!sel_was) pressed |= BTN_R;
            l_selector = 1;
        } else if (!remap_on() && (buttons & BTN_L)) {   /* the old mode: L itself */
            u32 was_pressed = (pressed & BTN_L) != 0;
            set_all(&view, BTN_L, BTN_R);
            buttons = (buttons & ~BTN_L) | BTN_R;
            pressed &= ~BTN_L;
            if (was_pressed) pressed |= BTN_R;
            l_selector = 1;
        }
    }

    /* Link must be free to act: standing or moving. Menus, the ocarina, dialogues, the item selector, climbing,
     * swimming ... keep every button as the game means it (B = back / cancel there). A use in progress goes on. */
    int ingame = player_active() && hud_ingame();
    int free_now = ingame && (player_state_is("Wait") || player_state_is("Move") ||
                              player_state_starts("LockOn"));   /* LockOn*: ZL held (target or free), aiming */
    int using = g_phase == PH_EQUIP || g_phase == PH_HOLD || g_phase == PH_ZRBACK;
    /* The back role is the game's B only where B is not an item use: Link busy with something else (hanging, climbing,
     * swimming ... where the slot logic below leaves every button alone) or no item on the B slot. */
    g_back_ok = !g_shbcfg.button_mapping || (!free_now && !using) || g_assign[SLOT_B] < 0;
    /* With our item buttons the game's item selector opens only from L in the selector mode: R (and L as an item
     * button) never reach the game while Link plays, whatever he is doing (sword, hookshot, bow ...). Events
     * (dialogues, shops) and the ocarina (L / R are notes there) keep them. */
    u64 no_sel = 0;
    if (g_shbcfg.button_mapping && ingame && !player_state_starts("Evt") && !g_songbook_active) {
        if (!l_selector) no_sel |= BTN_R;
        if (!(g_shbcfg.bm_l_mode == 2 && !remap_on())) no_sel |= BTN_L;   /* L is the selector only in the old mode */
    }
    if (!g_shbcfg.button_mapping || (!free_now && !using)) {
        if (no_sel) set_all(&view, no_sel, 0);
        if (g_phase == PH_GAP) g_phase_t = 0;   /* the gap counts free frames in a row */
        if (ingame && (pressed & (slot_mask(SLOT_B) | slot_mask(SLOT_L) | slot_mask(SLOT_R)))) {   /* remember why a slot button did nothing */
            const char *n = player_state_name();
            for (int i = 0; i < 23; i++) g_cl_refused[i] = n[i];
        }
        g_swallow = 0;
        return;
    }
    if (!ingame) {   /* a menu opened mid-use (pause ...): stop holding anything */
        g_phase = PH_IDLE; g_swallow = 0;
        return;
    }

    /* What the game has in its item box. With no slot item in it, that is the ZR item (the player's own pick:
     * R selector, pause menu A, the save). While a slot item borrows it, a different item there means the player
     * picked a new one: that becomes the ZR item and nothing is put back. The slots are never changed here. */
    int box = read_box();
    if (g_box_pending != -2 && !g_paused) {   /* a menu swap with ZR: the box gets ZR's new item now */
        int want = g_box_pending;
        g_box_pending = -2;
        if (g_a_pick >= 0 && box == g_a_pick) zr_take(box);   /* the player put that item on ZR with A after it */
        else { g_zr_item = want; box_put(want); g_sync_t = 120; }
        g_a_pick = -1;
    }
    if (box == -2) {
        /* unknown (no GameData yet): nothing to compare */
    } else if (!g_borrowed) {
        if (g_sync_t > 0) {   /* waiting for the box to take the menu swap (the game sets it from the menu on close) */
            if (box == g_zr_item) g_sync_t = 0;
            else if (--g_sync_t % 8 == 0) box_put(g_zr_item);
        } else if (box != g_zr_item) {
            zr_take(box);     /* the player's pick (R selector, pause menu A) */
        }
    } else if (!using && box >= 0 && box != g_borrow_item && box != g_saved) {
        zr_take(box);         /* a pick while a slot item was in the box: nothing is put back */
        unborrow(); g_phase = PH_IDLE;
    }

    g_swallow &= buttons;
    if (g_swallow) set_all(&view, g_swallow, 0);
    buttons &= ~g_swallow;
    pressed &= ~g_swallow;

    u64 all_slot = 0;
    for (int s = 0; s < SLOT_N; s++)
        if (g_assign[s] >= 0 && slot_enabled(s) && !(s == SLOT_R && l_selector)) all_slot |= slot_mask(s);

    if (!using) {   /* idle, or a slot item waiting to give the box back */
        for (int s = 0; s < SLOT_N; s++) {
            if (!slot_enabled(s)) continue;   /* manual shield: L never uses an item */
            if (s == SLOT_R && l_selector) continue;   /* this R came from L (item selector) */
            if ((pressed & slot_mask(s)) && g_assign[s] >= 0) {
                if (!g_borrowed) { g_saved = g_zr_item; g_borrowed = 1; hud_zr_hold(1); }
                g_active = s;
                g_borrow_item = g_assign[s];
                if (box != g_assign[s]) equip_now(g_assign[s]);   /* already there (used again): no swap */
                g_phase = PH_EQUIP;
                g_phase_t = 0;
                g_cl_dbg[0]++;
                break;
            }
        }
        /* ZR pressed while a slot item still borrows the box: put the ZR item back first, then let ZR through */
        if (g_phase != PH_EQUIP && g_borrowed && (pressed & BTN_ZR)) {
            box_put(g_saved);
            g_phase = PH_ZRBACK;
            g_phase_t = 0;
            g_zr_tap = 0;
            g_cl_dbg[2]++;
        }
    }

    /* A standing still: put away (B's own function, B holds an item) */
    u64 put_away = 0;
    if (g_assign[SLOT_B] >= 0 && slot_enabled(SLOT_B)) {
        if ((pressed & BTN_A) && g_phase != PH_HOLD && player_state_is("Wait")) g_a_watch = 1;
        else if (g_a_watch) {
            if (!player_state_is("Wait")) g_a_watch = 0;        /* A did something (roll, talk ...) */
            else if (++g_a_watch > A_PUTAWAY_WAIT) { g_a_watch = 0; g_a_press = A_PUTAWAY_PRESS; }
        }
        if (g_a_press > 0) { g_a_press--; put_away = slot_mask(SLOT_B); }
    } else { g_a_watch = 0; g_a_press = 0; }

    /* Assigned slot buttons never reach the game directly: the item is used through ZR. */
    u64 set = put_away, clear = all_slot | no_sel;
    switch (g_phase) {
    case PH_EQUIP:                             /* let the equip settle before using it */
        /* until the game's box really has the slot item (else the ZR item would be used), at most EQUIP_MAX */
        ++g_phase_t;
        if ((box == g_borrow_item && g_phase_t >= EQUIP_TICKS) || g_phase_t >= EQUIP_MAX) { g_phase = PH_HOLD; g_phase_t = 0; }
        break;
    case PH_HOLD:                              /* ZR follows the slot button (aim while held, throw on release) */
        /* a tap still uses the item: ZR is held at least HOLD_MIN reads */
        if ((buttons & slot_mask((u32)g_active)) || g_phase_t < HOLD_MIN) { set = BTN_ZR; g_phase_t++; }
        else { g_phase = PH_GAP; g_phase_t = 0; }
        break;
    case PH_GAP:                               /* the use is over once Link is free for a while (nothing mid-use) */
        if (!g_borrowed) { g_phase = PH_IDLE; break; }
        /* An item Link keeps in his hand (the Deku stick): giving the box back makes the game put it away, so it
         * stays borrowed until the player presses ZR (ZRBACK puts the ZR item back first) or uses another slot.
         * The HUD keeps showing the ZR item meanwhile (hud_zr_hold). Found live: the native stick stays out in
         * Wait; ours was put away ~0.3 s after Link stood still. */
        if (held_item(g_borrow_item)) { g_phase_t = 0; break; }
        /* only standing or walking: aiming (bow, hookshot, slingshot) is a LockOn* state too, and the item must
         * stay in the box until the shot is over (else the ZR item is drawn or shot instead) */
        g_phase_t = (player_state_is("Wait") || player_state_is("Move")) ? g_phase_t + 1 : 0;
        if (g_phase_t >= shb_frames((u32)(g_borrow_item == 3 ? GAP_BOOMERANG : GAP_TICKS))) {
            box_put(g_saved);
            g_phase = PH_RESTORE;
            g_phase_t = 0;
        }
        break;
    case PH_RESTORE:                           /* the ZR item goes back in the box */
        if (box == g_saved) { unborrow(); g_zr_item = g_saved; g_phase = PH_IDLE; }
        else if (++g_phase_t > 40) { g_phase = PH_GAP; g_phase_t = 0; }   /* not taken now: try again later */
        else if (g_phase_t % 8 == 0) box_put(g_saved);
        break;
    case PH_ZRBACK:                            /* hold ZR back until the box has the ZR item again */
        clear |= BTN_ZR;
        if (!(buttons & BTN_ZR)) g_zr_tap = 1; /* let go meanwhile: the press is replayed */
        if (box == g_saved) {
            unborrow(); g_zr_item = g_saved;
            g_phase = PH_IDLE;
            if (g_zr_tap) g_zr_inject = 4;
            else clear &= ~BTN_ZR;             /* still held: it reaches the game from now on */
        } else if (++g_phase_t > 30) {
            /* the box did not take it: ZR never uses the slot item; this press is dropped, the restore goes on */
            g_swallow |= BTN_ZR;
            g_phase = PH_GAP; g_phase_t = 0;
        } else if (g_phase_t % 8 == 0) {
            box_put(g_saved);
        }
        break;
    default:                                   /* idle: a slot item still in the box goes back after the gap */
        if (g_borrowed) { g_phase = PH_GAP; g_phase_t = 0; }
        else g_phase = PH_IDLE;
        break;
    }
    if (g_zr_inject > 0) { g_zr_inject--; set |= BTN_ZR; clear &= ~BTN_ZR; }
    set_all(&view, clear, set);
}

/* Called before the stock item function: may replace its item index (x2). */
u64 button_mapping_pre(u32 site, void *a0, void *a1, u64 a2) {
    (void)a0; (void)a1;
    if (site != 3) return a2;
    if (g_pending >= 0) {
        g_cl_dbg[1]++;
        u64 r = (u64)(long long)g_pending;
        g_pending = -1;
        return r;
    }
    /* a natural call (R selector): the box is read back from the HUD (box sync), which swaps the buttons */
    return a2;
}

void button_mapping_on_call(u32 site, void *a0, void *a1, void *a2, u32 stock_result) {
    if (site < 4) g_slot_count[site]++;
    SlotLog *e = &g_slot_log[(g_slot_log_n - SLOT_BASE) % LOG_N];
    g_slot_log_n++;
    e->site = site;
    e->result = stock_result;
    e->a0 = (unsigned long long)a0;
    e->a1 = (unsigned long long)a1;
    e->a2 = (unsigned long long)a2;
}


/* ---- per save file: save:/slot_NN/ootmaster.bin next to the game's slot_NN/progress.sav ----
 * The game opens progress.sav to read (mode 1) when it loads that save, and to write (mode 6) when it saves into
 * that slot (it rotates between slots), then commits the "save" mount. Our file is written right then, so the
 * same commit takes it, and read when that progress.sav is loaded. Format: "OOTM", version, child B/L/R,
 * adult B/L/R (signed bytes, -1 empty). */
typedef struct { void *h; } FsFile;
typedef struct { int flags; } FsWriteOpt;
extern u32 fs_create_file(const char *path, long long size);
extern u32 fs_open_file(FsFile *out, const char *path, int mode);
extern u32 fs_read_file(unsigned long long *out, FsFile f, long long off, void *buf, unsigned long long size);
extern u32 fs_write_file(FsFile f, long long off, const void *buf, unsigned long long size, const FsWriteOpt *opt);
extern u32 fs_set_file_size(FsFile f, long long size);
extern void fs_close_file(FsFile f);
u32 g_save_dbg[6];   /* writes, write failures, reads, read misses, last open result, last slot */
static int our_path(const char *path, char *out) {   /* save:/slot_NN/progress.sav -> save:/slot_NN/ootmaster.bin */
    static const char PRE[] = "save:/slot_", SAV[] = "/progress.sav", OURS[] = "/ootmaster.bin";
    u32 i = 0;
    for (; PRE[i]; i++) if (path[i] != PRE[i]) return 0;
    if (path[i] < '0' || path[i] > '9' || path[i + 1] < '0' || path[i + 1] > '9') return 0;
    u32 slash = i + 2;
    for (u32 j = 0; SAV[j]; j++) if (path[slash + j] != SAV[j]) return 0;
    if (path[slash + 13]) return 0;
    for (u32 j = 0; j < slash; j++) out[j] = path[j];
    for (u32 j = 0; ; j++) { out[slash + j] = OURS[j]; if (!OURS[j]) break; }
    g_save_dbg[5] = (u32)((path[i] - '0') * 10 + (path[i + 1] - '0'));
    return 1;
}
static void save_write(const char *ours) {
    static signed char buf[16] = {'O', 'O', 'T', 'M', 1};
    static FsFile f;
    static FsWriteOpt opt;
    f = (FsFile){0};
    if (g_cur_age >= 0) for (int s = 0; s < SLOT_ZR; s++) g_set[g_cur_age][s] = g_assign[s];
    for (int a = 0; a < 2; a++) for (int s = 0; s < SLOT_ZR; s++) buf[8 + a * 4 + s] = (signed char)g_set[a][s];
    fs_create_file(ours, sizeof buf);   /* fails when it is already there */
    u32 r = fs_open_file(&f, ours, 2 | 4);
    g_save_dbg[4] = r;
    if (r) { g_save_dbg[1]++; return; }
    opt = (FsWriteOpt){1};   /* flush */
    fs_set_file_size(f, sizeof buf);
    r = fs_write_file(f, 0, buf, sizeof buf, &opt);
    fs_close_file(f);
    g_save_dbg[r ? 1 : 0]++;
}
static void save_read(const char *ours) {
    static signed char buf[16];
    static FsFile f;
    static unsigned long long got;
    f = (FsFile){0};
    got = 0;
    for (int a = 0; a < 2; a++) for (int s = 0; s < SLOT_ZR; s++) g_set[a][s] = -1;
    for (int s = 0; s < SLOT_ZR; s++) g_assign[s] = -1;
    g_cur_age = -1;   /* the set is taken on the first gameplay frame (the age is not loaded yet) */
    if (g_borrowed) unborrow();   /* another save: its own item box */
    g_phase = PH_IDLE;
    g_zr_item = ZR_UNKNOWN; g_box_pending = -2; g_sync_t = 0;
    u32 r = fs_open_file(&f, ours, 1);
    g_save_dbg[4] = r;
    if (r) { g_save_dbg[3]++; return; }   /* none: empty sets */
    r = fs_read_file(&got, f, 0, buf, sizeof buf);
    fs_close_file(f);
    if (r || got < 14 || buf[0] != 'O' || buf[1] != 'O' || buf[2] != 'T' || buf[3] != 'M' || buf[4] != 1) { g_save_dbg[3]++; return; }
    for (int a = 0; a < 2; a++)
        for (int s = 0; s < SLOT_ZR; s++) {
            int v = buf[8 + a * 4 + s];
            g_set[a][s] = v >= 0 && v < 64 ? v : -1;
        }
    g_save_dbg[2]++;
}
/* ---- diagnostic dump: <slot>/shbdebug.bin. Which wrappers ran, the input calls per site, the
 * save hook's results and the config block the module reads.
 *
 * It is a test tool, not a feature: it CREATES FILES IN THE GAME'S SAVE VOLUME, from the input
 * hook, on a timer. A release build must not do that at all — a write landing while
 * gmd::SaveDataMgr is committing its own files (fs: save: published N files) races it, and the
 * crash it leaves is a release of a half-built object somewhere else entirely. Turn it on only
 * for a build you are diagnosing: build_core.py --debug-dump. ---- */
#ifndef SHB_DBG_DUMP
#define SHB_DBG_DUMP 0
#endif
#if SHB_DBG_DUMP
extern unsigned int g_wrap_entry[5], g_slot_entry[4], g_code_entry[32];
extern u32 g_save_dbg[6];
static u32 g_dbg_tick, g_dbg_done;

static void dbg_write_path(const char *path) {
    static unsigned char buf[440];
    static FsFile f;
    static FsWriteOpt opt;
    f = (FsFile){0};
    for (int i = 0; i < 440; i++) buf[i] = 0;
    static const char MAGIC[] = "SHBDBG01";
    for (int i = 0; i < 8; i++) buf[i] = (unsigned char)MAGIC[i];
#define DBG32(off, val) do { unsigned int _v = (unsigned int)(val); buf[(off)] = (unsigned char)_v; \
    buf[(off) + 1] = (unsigned char)(_v >> 8); buf[(off) + 2] = (unsigned char)(_v >> 16); \
    buf[(off) + 3] = (unsigned char)(_v >> 24); } while (0)
    DBG32(8, 1);
    DBG32(12, 440);
    for (int i = 0; i < 5; i++)  DBG32(16 + 4 * i, g_wrap_entry[i] - 0x10000000u);
    for (int i = 0; i < 4; i++)  DBG32(36 + 4 * i, g_slot_entry[i] - 0x10000000u);
    for (int i = 0; i < 32; i++) DBG32(52 + 4 * i, g_code_entry[i] - 0x10000000u);
    for (int i = 0; i < 6; i++)  DBG32(180 + 4 * i, g_save_dbg[i]);
    {
        const unsigned char *cfg = (const unsigned char *)(const void *)&g_shbcfg;
        for (int i = 0; i < 76; i++) buf[204 + i] = cfg[i];
    }
    for (int s = 0; s < 5; s++) {
        int b = 280 + s * 32;
        DBG32(b + 0, g_dbg_hits[s]);
        DBG32(b + 4, (unsigned int)(g_dbg_a0[s] & 0xFFFFFFFFu));
        DBG32(b + 8, (unsigned int)(g_dbg_a0[s] >> 32));
        DBG32(b + 12, (unsigned int)(g_dbg_a1[s] & 0xFFFFFFFFu));
        DBG32(b + 16, g_dbg_cnt[s]);
        DBG32(b + 20, g_dbg_btn[s]);
    }
#undef DBG32
    fs_create_file(path, 440);               /* fails when it is already there */
    u32 r = fs_open_file(&f, path, 2 | 4);
    if (r) return;
    opt = (FsWriteOpt){1};                    /* flush */
    fs_set_file_size(f, 440);
    fs_write_file(f, 0, buf, 440, &opt);
    fs_close_file(f);
}
#endif   /* SHB_DBG_DUMP */

static void dbg_tick(void) {
#if SHB_DBG_DUMP
    if (++g_dbg_tick < 600) return;          /* about every 10 s of input polling */
    g_dbg_tick = 0;
    if (g_dbg_done >= 60) return;
    g_dbg_done++;
    dbg_write_path("save:/slot_00/shbdebug.bin");
#endif
}

/* core/hud.c, hook on nn::fs::OpenFile (main+0x3C42590): any file the game opens */
void button_mapping_on_open(const char *path, int mode) {
    if (path[0] != 's' || !g_shbcfg.button_mapping) return;   /* not save: (content: is the romfs) */
    char ours[48];
    if (!our_path(path, ours)) return;
    if (mode & 2) save_write(ours);
    else save_read(ours);
#if SHB_DBG_DUMP
    {   /* dump next to the save the game just opened (the save volume is mounted here) */
        char dbg[48];
        u32 n = 0, slash = 0;
        while (ours[n]) { dbg[n] = ours[n]; n++; }
        for (u32 i = 0; i < n; i++) if (dbg[i] == '/') slash = i;
        static const char TAIL[] = "/shbdebug.bin";
        for (u32 j = 0; ; j++) { dbg[slash + j] = TAIL[j]; if (!TAIL[j]) break; }
        dbg_write_path(dbg);
    }
#endif
}
