/* Controls: optional button swaps and the target switch. They run before the other input features.
 *
 *  - target_r3: physical R3 and ZL are exchanged. R3 then holds (or toggles, per the game option)
 *    the target camera; ZL keeps R3's native function. The app recommends Toggle targeting for this.
 *  - switch_target: while Link is locked on, a tap of the target button (ZL, or R3 with the swap) moves the
 *    lock to the enemy the game marks in red, the next candidate; holding it drops the target.
 *
 * Sprint on L3 is handled by the stamina feature (it sets the sprint flag without injecting A, so
 * Link does not roll). Only gameplay is touched: menus and loading screens keep their native buttons.
 *
 * How the switch works (found in main 1.1.1): the player's attention component (player actor -> component
 * table +0x270 -> +0x158) keeps the targetable candidates in an array, count at +0x78, array at +0x80, guarded by
 * the mutex at +0x98. Each element -> [elem] -> +0x8 is the actor. The AttentionSystem (main+0x4E24B28) locks
 * on element 0 every frame (main+0x210A60: ActorLink::set(&att->+0xB68, elem0 actor)); element 1 is the next
 * candidate, the one with the red marker. Swapping elements 0 and 1 moves the lock; the game redraws the camera,
 * the lock triangles and the marker by itself.
 */
#include "../../core/dispatch.h"
#include "../../core/hid.h"
#include "../../core/clock.h"
#include "../../core/config.h"
#include "../../core/remap.h"

typedef unsigned char u8;

#define CT_BTN_R3  0x0020ull
#define CT_BTN_ZL  0x0100ull

int player_dead(void);
int hud_ingame(void);
extern u64 g_player_actor;          /* core/player.c */
extern char ATTENTION_SYS[];        /* main+0x4E24B28: AttentionSystem * */
extern void os_mutex_lock(void *);
extern void os_mutex_unlock(void *);
extern char GOT_MUTEX_LOCK[], GOT_MUTEX_UNLOCK[], PLT0[];

#define SW_HOLD   18u   /* frames (0.3 s) the target button must be held, while locked, to drop the target */
#define SW_INJECT 4u    /* reads with the synthesised target press that makes the game's toggle let go */

u32 hud_frame_count(void);
u8 g_sw_own;        /* the current press of the target button is ours (it started while locked) */
u8 g_sw_prev;       /* the target button in the previous read (press edge) */
u8 g_sw_long;       /* this press was held long enough: it drops the target instead of switching */
u8 g_sw_inject;     /* reads left with our own target press */
u32 g_sw_start;     /* frame the press started */
u32 g_sw_cancels;   /* diagnostic: targets dropped by a long press */
u32 g_sw_cancel_frame;   /* frame of the last long-press drop: the automatic next target ignores that drop */
u32 g_sw_count;     /* diagnostic: switches done */
u32 g_sw_tries;     /* diagnostic: switch presses while locked */

static u64 rdq(u64 a) { return *(volatile u64 *)a; }
static int heap_ptr(u64 p) { return p >= 0x10000 && !(p & 7); }

/* The attention component of the player, or 0. */
static u64 attention_comp(void) {
    u64 pa = g_player_actor;
    if (!heap_ptr(pa)) return 0;
    u64 table = rdq(pa + 0x270);
    if (!heap_ptr(table)) return 0;
    u64 comp = rdq(table + 0x158);
    return heap_ptr(comp) ? comp : 0;
}

static u64 elem_actor(u64 elem) {
    if (!heap_ptr(elem)) return 0;
    u64 inner = rdq(elem);
    return heap_ptr(inner) ? rdq(inner + 8) : 0;
}

/* The AttentionSystem's lock link is valid and points at the actor in element 0: Link is locked on it. */
static int locked_on(u64 actor) {
    u64 att = rdq((u64)ATTENTION_SYS);
    if (!heap_ptr(att) || !actor) return 0;
    u32 id = *(volatile u32 *)(att + 0xB78);
    u64 entry = rdq(att + 0xB70);
    if (id == 0xFFFFFFFFu || !heap_ptr(entry)) return 0;
    return *(volatile u32 *)(entry + 0x48) == id && rdq(entry + 0x40) == actor;
}

/* The actor the game has locked on (the AttentionSystem's lock link, when it is valid), or 0. Also used by the
 * automatic next target (features/manual_shield): the player's LockOn* action names also show up without a
 * target (battle stance near enemies, after an attack), this link does not. */
u64 attention_locked_actor(void) {
    u64 att = rdq((u64)ATTENTION_SYS);
    if (!heap_ptr(att)) return 0;
    u32 id = *(volatile u32 *)(att + 0xB78);
    u64 entry = rdq(att + 0xB70);
    if (id == 0xFFFFFFFFu || !heap_ptr(entry) || *(volatile u32 *)(entry + 0x48) != id) return 0;
    u64 actor = rdq(entry + 0x40);
    return heap_ptr(actor) ? actor : 0;
}

/* Link is locked on the first candidate right now. */
static int locked_now(void) {
    u64 comp = attention_comp();
    if (!comp || !*(volatile u32 *)(comp + 0x78)) return 0;
    u64 arr = rdq(comp + 0x80);
    return heap_ptr(arr) && locked_on(elem_actor(rdq(arr)));
}

/* The import stubs are lazily bound: only call them once the game has (its GOT no longer holds PLT0). */
static int mutex_ready(void) {
    return rdq((u64)GOT_MUTEX_LOCK) != (u64)PLT0 && rdq((u64)GOT_MUTEX_UNLOCK) != (u64)PLT0;
}

/* Move the lock to the next candidate. 1 when the lock is on an enemy and there was another one. */
static int switch_to_next(void) {
    u64 comp = attention_comp();
    if (!comp) return 0;
    u32 n = *(volatile u32 *)(comp + 0x78);
    u64 arr = rdq(comp + 0x80);
    if (n < 2 || n > 256 || !heap_ptr(arr)) return 0;
    if (!locked_on(elem_actor(rdq(arr)))) return 0;
    g_sw_tries++;
    int guard = mutex_ready();
    if (guard) os_mutex_lock((void *)(comp + 0x98));
    int done = 0;
    n = *(volatile u32 *)(comp + 0x78);   /* (again, under the lock) */
    if (n >= 2) {
        volatile u64 *a = (volatile u64 *)arr;
        u64 e0 = a[0], e1 = a[1];
        if (elem_actor(e1)) { a[0] = e1; a[1] = e0; done = 1; }
    }
    if (guard) os_mutex_unlock((void *)(comp + 0x98));
    if (done) g_sw_count++;
    return done;
}

void controls_on_input(u32 site, void *a0, void *a1, void *a2, u32 stock_result) {
    (void)a2;
    if (site != 3 || !a0) return;
    if (!g_shbcfg.target_r3 && !g_shbcfg.switch_target) { g_sw_own = 0; g_sw_prev = 0; return; }
    u32 count = hid_sample_count(a1, stock_result);
    if (count == 0 || count > 64) return;
    /* Keep menus, pause and the game-over screen native. */
    if (player_dead() || !hud_ingame()) { g_sw_own = 0; g_sw_inject = 0; return; }

    HidView view = {a0, count, 0};
    u64 any = 0;
    for (u32 i = 0; i < count; i++) {
        u64 b = hid_get_buttons(&view, i);
        if (g_shbcfg.target_r3 && !remap_on()) {   /* (remap: target is on any button) */
            u64 zl = b & CT_BTN_ZL, r3 = b & CT_BTN_R3;
            b &= ~(CT_BTN_ZL | CT_BTN_R3);
            if (r3) b |= CT_BTN_ZL;
            if (zl) b |= CT_BTN_R3;
            hid_set_buttons(&view, i, b);
        }
        any |= b;
    }

    if (!g_shbcfg.switch_target) { g_sw_own = 0; g_sw_prev = 0; g_sw_inject = 0; return; }
    /* The target button (the ZL bit as the game sees it, after the swap). A press that starts while Link is
     * locked on is ours and hidden from the game: let go quickly, it switches to the red-marked enemy; held for
     * SW_HOLD frames, a short press of our own reaches the game, whose toggle then drops the target. A press with
     * no target is the game's own (it locks on). In Hold targeting Link is locked only while the button is held,
     * so no new press can start while locked and nothing changes there. */
    u8 now = (any & CT_BTN_ZL) != 0;
    u32 f = hud_frame_count();
    if (now && !g_sw_prev && locked_now()) {
        g_sw_own = 1;
        g_sw_long = 0;
        g_sw_start = f;
    }
    if (g_sw_own) {
        if (now) {
            if (!g_sw_long && f - g_sw_start >= shb_frames(SW_HOLD)) {   /* held: drop the target */
                g_sw_long = 1;
                g_sw_inject = SW_INJECT;
                g_sw_cancels++;
                g_sw_cancel_frame = f ? f : 1;
            }
        } else {
            if (!g_sw_long) switch_to_next();                  /* tapped: next target */
            g_sw_own = 0;
        }
    }
    g_sw_prev = now;
    if (g_sw_own || g_sw_inject) {
        for (u32 i = 0; i < count; i++) {
            u64 b = hid_get_buttons(&view, i) & ~CT_BTN_ZL;
            if (g_sw_inject) b |= CT_BTN_ZL;
            hid_set_buttons(&view, i, b);
        }
        if (g_sw_inject) g_sw_inject--;
    }
}
