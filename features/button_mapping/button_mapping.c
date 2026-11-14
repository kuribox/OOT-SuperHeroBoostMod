#include "../../core/dispatch.h"
#include "../../core/hid.h"
#include "../../core/config.h"

/* button_mapping (F3, "buttonMapping"): our own extra item buttons on the HUD.
 *
 * Controls on this game: X jump, A dodge, Y sword, B / L / R / ZR are the free
 * buttons. In-game R equips an item into the HUD item box and ZR uses it
 * (hold and release). The stock function at 0x1211C00 is what that equip ends up
 * calling; its third argument is the item index (inventory slot order:
 * 0 stick, 1 nut, 3 boomerang ...).
 *
 * buttonMapping: B, L, R and ZR can each hold an item; an empty slot keeps its
 * normal behaviour. Pressing an assigned one:
 *   1. EQUIP   - R is pressed for a few frames and at the stock call (slot site 3,
 *                pre-hook) the item index is replaced by the slot's item;
 *   2. HOLD    - ZR is held while the slot button is held;
 *   3. RESTORE - on release the item that was in the box before is re-equipped.
 */
#define SLOT_BASE 0x10000000u
u32 g_slot_count[4] = {SLOT_BASE, SLOT_BASE, SLOT_BASE, SLOT_BASE};

#define LOG_N 24
typedef struct { u32 site; u32 result; unsigned long long a0, a1, a2; } SlotLog;
SlotLog g_slot_log[LOG_N] = {{0x7FFFFFFFu, 0, 0, 0, 0}};   /* non-zero: lives in .data */
u32 g_slot_log_n = SLOT_BASE;

enum { SLOT_B = 0, SLOT_L = 1, SLOT_R = 2, SLOT_ZR = 3, SLOT_N = 4 };
#define BTN_RSTICK 0x0020ull   /* right stick click (R3) */
static const u64 SLOT_MASK[SLOT_N] = {BTN_B, BTN_L, BTN_R, BTN_ZR};

/* Item index per slot, -1 = empty (button stays normal): the set of Link's current age. Assigned in the pause
 * menu's Items tab with X / L / R; empty at first. (.data) */
int g_assign[SLOT_N] = {-1, -1, -1, -1};
/* Both sets (0 child, 1 adult) for B / L / R, kept per save file (save_write / save_read). g_cur_age: the set
 * g_assign holds (-1: none yet, taken from g_set on the next gameplay frame). */
int g_set[2][SLOT_ZR] = {{-1, -1, -1}, {-1, -1, -1}};
int g_cur_age = -1;
int hud_is_child(void);          /* core/hud.c: Sequence_IsChildhood (-1: no GameData) */
int hud_item_owned(int item);    /* core/hud.c: IsGet.<item> */

enum { PH_IDLE = 0, PH_EQUIP, PH_HOLD, PH_GAP, PH_RESTORE, PH_ZRBACK };
#define EQUIP_TICKS 4
#define GAP_TICKS 20

int g_pending = -1;        /* item index to force on the next stock call (old R-press method, unused) */
/* L as the shield (app option bm_l_mode 1): L holds ZL for the game, the native guard, while core/hooks.S
 * hook_zl_press drops ZL's press edge (the lock-on onto a target happens on that press), so the shield goes up
 * without targeting anyone. */
int g_l_shield_held;       /* L held as the shield, from the newest new sample */
u32 g_zl_press_block;      /* read by hook_zl_press: drop "ZL pressed" while L is the shield */
static u64 g_l_clock;      /* clock of the newest sample looked at */
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
int hud_menu_items(void);   /* core/hud.c: the pause menu's Items tab is open */
int hud_menu_item(void);    /* core/hud.c: item index highlighted there (-1: none / not owned) */
void hud_menu_tick(void);   /* core/hud.c: draw the slot cluster on the menu page */
void hud_menu_decide(void); /* core/hud.c: play the highlighted cell's Decide animation (the game's A feedback) */
int hud_menu_decided(void); /* core/hud.c: that animation is playing */
void hud_menu_zr(int item); /* core/hud.c: show an item on ZR now */
void hud_menu_off(void);    /* core/hud.c: not on the Items tab: hide the assign legend */
static int g_menu_a_item = -1, g_menu_a_wait;
static int g_paused;        /* + pressed in gameplay: the pause menu is up */

static int g_box_pending = -2;   /* the item box gets this when gameplay is back: an item, -1 empty, -2 nothing */
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

void button_mapping_on_input(u32 site, void *a0, void *a1, void *a2, u32 stock_result) {
    (void)a2;
    if (site != 3 || !a0) return;
    u32 count = (u32)(unsigned long)a1;
    if (count == 0 || count > 64) count = stock_result;
    if (count == 0 || count > 64) return;
    HidView view = {a0, count, 0};

    /* Death ragdoll / game over: do not remap or swallow anything, the game's screen needs every button. */
    if (player_dead()) {
        g_l_shield_held = 0; g_zl_press_block = 0;
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
            }
            for (int s = 0; s < SLOT_ZR; s++) if (g_assign[s] >= 0 && !hud_item_owned(g_assign[s])) g_assign[s] = -1;
        }
    }

    /* L as the shield or the game's item selector (app option): L never holds an item */
    if (g_shbcfg.bm_l_mode) g_assign[SLOT_L] = -1;

    /* Publish the slots to the HUD (core/hud.c): B, L, R as assigned (ZR mirrors the game's own box). */
    for (int s = 0; s < SLOT_N; s++)
        hud_slot_item((u32)s, s == SLOT_ZR && g_assign[s] < 0 ? g_zr_item : g_assign[s]);

    u64 buttons = hid_merge_buttons(&view);
    /* L held as the shield. The game reads the same samples several times and L is removed from them below, so only
     * samples newer than the last ones looked at count; with none new, the previous state stays. */
    if (g_shbcfg.button_mapping && g_shbcfg.bm_l_mode == 1) {
        u64 newest = g_l_clock;
        int any = 0, held = 0;
        for (u32 i = 0; i < view.count; i++) {
            u64 clk = hid_get_clock(&view, i);
            if (clk <= g_l_clock) continue;
            u64 b = hid_get_buttons(&view, i);
            if (clk >= newest) { newest = clk; held = (b & BTN_L) && !(b & BTN_ZL); }
            any = 1;
        }
        if (any) { g_l_clock = newest; g_l_shield_held = held; }
        g_zl_press_block = (u32)g_l_shield_held;
    }
    u64 pressed = buttons & ~g_cl_prev;
    g_cl_prev = buttons;

    /* Pause menu, Items tab: X, L and R assign the highlighted item to the B, L and R slots (like the item buttons;
     * B stays "back"). An item already on another slot swaps with it. The game's ZL / ZR still change tabs. */
    /* The pause menu is open only after + was pressed in gameplay, until gameplay runs again (the Items panel exists
     * from boot and looks "open" on the title screen and while loading, where its cells are not ready). */
    {   /* gameplay runs while the HUD's frame count moves (it stops under the menu) */
        u32 f = hud_frame_count();
        g_run = f != g_run_frame ? g_run + 1 : 0;
        g_run_frame = f;
        if ((pressed & BTN_PLUS) && player_active()) g_paused = 1;
        else if (g_run > 10) g_paused = 0;
    }
    int menu = g_paused && hud_menu_items();
    if (g_shbcfg.button_mapping && !menu) hud_menu_off();
    if (g_shbcfg.button_mapping && menu) {
        static const u64 MENU_BTN[SLOT_ZR] = {BTN_X, BTN_L, BTN_R};   /* -> SLOT_B, SLOT_L, SLOT_R */
        for (int s = 0; s < SLOT_ZR; s++) {
            if (!(pressed & MENU_BTN[s])) continue;
            if (s == SLOT_L && g_shbcfg.bm_l_mode) continue;   /* L is the shield / item selector: not assignable */
            int it = hud_menu_item();
            if (it < 0) continue;
            int old = g_assign[s];
            if (old == it) g_assign[s] = -1;   /* the same button again: unassign (the button works as normal) */
            else {
                /* each item on one button only: the one that had it gets this button's old item (ZR too) */
                for (int o = 0; o < SLOT_ZR; o++) if (o != s && g_assign[o] == it) g_assign[o] = old;
                if (it == g_zr_item) { g_zr_item = old; g_box_pending = old; hud_menu_zr(old); }
                g_assign[s] = it;
            }
            g_cl_dbg[3]++;
            hud_menu_decide();
        }
        /* A is the game's own: it puts the item in its box (ZR) and plays the cell's Decide animation */
        if (pressed & BTN_A) { g_menu_a_item = hud_menu_item(); g_menu_a_wait = 20; }
        if (g_menu_a_wait > 0) {
            g_menu_a_wait--;
            if (g_menu_a_item >= 0 && hud_menu_decided()) {   /* the game took it: a button that had it gets the old box item */
                int it = g_menu_a_item, old = g_zr_item != it ? g_zr_item : -1;
                for (int o = 0; o < SLOT_ZR; o++) if (g_assign[o] == it) g_assign[o] = old;
                g_zr_item = it; g_box_pending = -2; hud_menu_zr(it); g_menu_a_wait = 0;
            }
        }
        for (int s = 0; s < SLOT_ZR; s++) hud_slot_item((u32)s, g_assign[s]);   /* show a swap at once */
        hud_menu_tick();
        set_all(&view, BTN_X | BTN_L | BTN_R, 0);
        g_phase = PH_IDLE; g_swallow = 0;
        return;
    }

    /* L as the shield: the game never sees L (it would open the item selector) but ZL instead; its press edge is
     * dropped by hook_zl_press, so the guard comes up without locking onto a target */
    if (g_shbcfg.button_mapping && g_shbcfg.bm_l_mode == 1 && hud_ingame() && (buttons & BTN_L)) {
        set_all(&view, BTN_L, BTN_ZL);
        buttons &= ~BTN_L;
        pressed &= ~BTN_L;
    }

    /* L as the game's item selector: outside menus, L does what R does (opens the selector for ZR).
     * The R slot is suppressed for this frame so an item assigned to R is not used as well. */
    u32 l_selector = 0;
    if (g_shbcfg.button_mapping && g_shbcfg.bm_l_mode == 2 && hud_ingame() &&
        (buttons & BTN_L) && !(buttons & BTN_R)) {
        u32 was_pressed = (pressed & BTN_L) != 0;
        set_all(&view, BTN_L, BTN_R);
        buttons = (buttons & ~BTN_L) | BTN_R;
        pressed &= ~BTN_L;
        if (was_pressed) pressed |= BTN_R;
        l_selector = 1;
    }

    /* Link must be free to act: standing or moving. Menus, the ocarina, dialogues, the item selector, climbing,
     * swimming ... keep every button as the game means it (B = back / cancel there). A use in progress goes on. */
    int ingame = player_active() && hud_ingame();
    int free_now = ingame && (player_state_is("Wait") || player_state_is("Move") ||
                              player_state_starts("LockOn"));   /* LockOn*: ZL held (target or free), aiming */
    if (!g_shbcfg.button_mapping || (!free_now && g_phase == PH_IDLE)) {
        if (ingame && (pressed & (BTN_B | BTN_L | BTN_R))) {   /* remember why a slot button did nothing */
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

    /* A swap with ZR made in the menu: the game's item box takes the button's old item (or empties) now that
     * gameplay is back (in the menu the game sets the box from its own selection when it closes). */
    if (g_box_pending != -2 && !g_paused) {
        if (g_borrowed) { g_borrowed = 0; hud_zr_hold(0); }
        if (g_box_pending >= 0) equip_now(g_box_pending);
        else { int ch = hud_is_child(); if (ch >= 0) hud_box_empty(ch); }
        g_zr_item = g_box_pending;
        g_box_pending = -2;
    }

    /* What the game has in its item box. While a slot item borrows it, a different item there means the player
     * picked a new one (R selector / pause menu): that becomes the ZR item and nothing is put back. */
    int box = hud_zr_item();
    if (!g_borrowed) {
        /* a new item in the box (selector, menu, an older save): the button that also had it gets the box's old
         * item, so no item sits on two buttons */
        if (box != g_zr_item) {
            if (box >= 0) for (int s = 0; s < SLOT_ZR; s++) if (g_assign[s] == box) g_assign[s] = g_zr_item;
            g_zr_item = box;
        }
    } else if (g_phase == PH_IDLE && box >= 0 && box != g_borrow_item && box != g_saved) {
        g_borrowed = 0; g_zr_item = box; hud_zr_hold(0);
    }

    g_swallow &= buttons;
    if (g_swallow) set_all(&view, g_swallow, 0);
    buttons &= ~g_swallow;
    pressed &= ~g_swallow;

    u64 all_slot = 0;
    for (int s = 0; s < SLOT_N; s++)
        if (g_assign[s] >= 0 && !(s == SLOT_R && l_selector)) all_slot |= SLOT_MASK[s];

    if (g_phase == PH_IDLE) {
        for (int s = 0; s < SLOT_N; s++) {
            if (s == SLOT_R && l_selector) continue;   /* this R came from L (item selector) */
            if ((pressed & SLOT_MASK[s]) && g_assign[s] >= 0) {
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
        /* ZR pressed while a slot item borrows the box: put the ZR item back first, then let ZR through */
        if (g_phase == PH_IDLE && g_borrowed && (pressed & BTN_ZR) && g_saved >= 0) {
            equip_now(g_saved);
            g_phase = PH_ZRBACK;
            g_phase_t = 0;
            g_cl_dbg[2]++;
        }
    }

    /* Assigned slot buttons never reach the game directly: the item is used through ZR. */
    u64 set = 0, clear = all_slot;
    switch (g_phase) {
    case PH_EQUIP:                             /* let the equip settle before using it */
        if (++g_phase_t >= EQUIP_TICKS) g_phase = PH_HOLD;
        break;
    case PH_HOLD:                              /* ZR follows the slot button (aim while held, throw on release) */
        if (buttons & SLOT_MASK[g_active]) set = BTN_ZR;
        else g_phase = PH_IDLE;                /* the item stays in the box: nothing is swapped mid-use */
        break;
    case PH_ZRBACK:                            /* hold ZR back until the box has the ZR item again */
        clear |= BTN_ZR;
        if (box == g_saved || ++g_phase_t > 30) {
            g_borrowed = 0; hud_zr_hold(0);
            g_phase = PH_IDLE;                 /* ZR (still held by the player) reaches the game from now on */
        } else if (g_phase_t % 8 == 0) {
            equip_now(g_saved);
        }
        break;
    default:
        g_phase = PH_IDLE;
        break;
    }
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
    g_zr_item = (int)a2;   /* a natural call: remember the box item */
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
    signed char buf[16] = {'O', 'O', 'T', 'M', 1};
    if (g_cur_age >= 0) for (int s = 0; s < SLOT_ZR; s++) g_set[g_cur_age][s] = g_assign[s];
    for (int a = 0; a < 2; a++) for (int s = 0; s < SLOT_ZR; s++) buf[8 + a * 4 + s] = (signed char)g_set[a][s];
    FsFile f = {0};
    fs_create_file(ours, sizeof buf);   /* fails when it is already there */
    u32 r = fs_open_file(&f, ours, 2 | 4);
    g_save_dbg[4] = r;
    if (r) { g_save_dbg[1]++; return; }
    FsWriteOpt opt = {1};   /* flush */
    fs_set_file_size(f, sizeof buf);
    r = fs_write_file(f, 0, buf, sizeof buf, &opt);
    fs_close_file(f);
    g_save_dbg[r ? 1 : 0]++;
}
static void save_read(const char *ours) {
    signed char buf[16];
    for (int a = 0; a < 2; a++) for (int s = 0; s < SLOT_ZR; s++) g_set[a][s] = -1;
    for (int s = 0; s < SLOT_ZR; s++) g_assign[s] = -1;
    g_cur_age = -1;   /* the set is taken on the first gameplay frame (the age is not loaded yet) */
    FsFile f = {0};
    u32 r = fs_open_file(&f, ours, 1);
    g_save_dbg[4] = r;
    if (r) { g_save_dbg[3]++; return; }   /* none: empty sets */
    unsigned long long got = 0;
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
/* core/hud.c, hook on nn::fs::OpenFile (main+0x3C42590): any file the game opens */
void button_mapping_on_open(const char *path, int mode) {
    if (path[0] != 's' || !g_shbcfg.button_mapping) return;   /* not save: (content: is the romfs) */
    char ours[48];
    if (!our_path(path, ours)) return;
    if (mode & 2) save_write(ours);
    else save_read(ours);
}
