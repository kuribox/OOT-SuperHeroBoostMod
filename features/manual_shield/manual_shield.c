/* manual shield: ZL toggles the lock-on camera, L holds the guard while it is
 * active and R3 cancels targeting.
 *
 * The two halves are:
 *   - manual_shield_on_input: consumes the physical ZL (target toggle) and
 *     synthesises native ZL while our toggle is on; L is only consumed while the
 *     lock-on camera is active, so it keeps its normal game function otherwise;
 *   - manual_shield_on_frame: keeps the native guard state off unless L is held
 *     with the camera active (the game would otherwise raise the shield while
 *     targeting), draws the shield on guard entry and scopes the AS Bool14 setter
 *     through the small assembly wrappers in manual_shield.S.
 *
 * The option is read from ShbConfig.manual_shield; with it off both halves are
 * pass-through and the stock game behaviour is kept.
 */
#include "../../core/dispatch.h"
#include "../../core/hid.h"
#include "../../core/config.h"
#include "../../core/remap.h"

typedef unsigned char u8;

#define MS_BTN_L   0x40ull
#define MS_BTN_R3  0x20ull
#define MS_BTN_ZL  0x100ull
/* The guard button: L in the classic layout; with the button remap, the shield role's own virtual bit (L stays
 * an item slot then). */
static u64 ms_guard_bit(void) { return remap_on() ? VBTN_SHIELD : MS_BTN_L; }

int player_dead(void);
u64 player_get(void);
int hud_ingame(void);
int player_state_is(const char *name);
int player_state_starts(const char *prefix);
const char *player_state_name(void);

/* main-relative functions (core/link.ld) */
extern void *resolve_player(void *);
extern u64 get_guard_component(void *);
extern void native_draw_equipment(void *, u32);
extern u32 as_int(void *, u32);
extern u32 as_bool(void *, u32);
extern char GAMEDATA_MGR[];                                /* main+0x4E842A8: GameData manager pointer */
extern u32 read_enum(void *, u32 *, void *, u32);          /* main+0x17BF90 */

/* GameData option: System > Options > Targeting Type (base-game enum). */
#define GD_OPTIONS_STRUCT 0x834d9d9eu
#define GD_LOCKON_FIELD   0xc713c7dau
#define LOCKON_HOLD       0x60fc6939u   /* 1627154745 */
#define LOCKON_TOGGLE     0x212dc0e7u   /* 556646631 */

/* State shared with the assembly wrappers (do not rename). */
u64 g_ms_state;          /* player state returned by resolve_player (x20/x21 in the guard sites) */
u64 g_ms_iface;          /* the player's AS interface (setter x0, guard component owner) */
u64 g_ms_equipment;      /* actor table +0x200: shield/equipment request target */
u8 g_ms_active;          /* manual shield option enabled (read by the wrappers) */
u8 g_ms_allowed;         /* this frame's guard decision (read by the setter wrapper) */
u8 g_ms_held;            /* guard request for this frame (L while targeting) */
u8 g_ms_carrying;        /* Link is carrying/lifting something */
u8 g_ms_targeting;       /* toggle mode: targeting latched by a ZL press */
u8 g_ms_zl_raw;          /* physical ZL in the latest input sample */
u8 g_ms_l_raw;           /* physical L in the latest input sample */
u8 g_ms_zl_prev;         /* previous frame's physical ZL, for the toggle edge */
u8 g_ms_prev_l;          /* previous sampled L (diagnostic) */
u8 g_ms_l_latch;         /* the current L press belongs to the shield (until L is let go) */
u8 g_ms_last_allowed;    /* previous frame's decision, for the shield draw */
u32 g_ms_toggle_mode = 1;/* 1 = game option Toggle Target, 0 = Hold Target */
u32 g_ms_target_value = 0xFFFFFFFFu;  /* diagnostic: raw GameData enum value */
u32 g_ms_tick;           /* scheduler frames (manual_shield_on_frame calls) */
u32 g_ms_last_tick;      /* tick at the previous gameplay HID read */
u32 g_ms_stale;          /* gameplay HID reads without a scheduler tick */
u32 g_ms_state_class;    /* last proof about targeting: 1 LockOn* state, 2 plain locomotion, 0 none */
u8 g_ms_retarget;        /* frames left with the synthesised target press (next target after a death) */
u8 g_ms_retry;           /* automatic presses since the last real one (one per lost target) */
u32 g_ms_press_tick;     /* tick of the last real target press, to ignore the game's lag */
u8 g_ms_drop;            /* consecutive plain locomotion frames (proof that the target is gone) */
u8 g_ms_drop_target;     /* we were targeting when those frames started */
u32 g_ms_retargets;      /* diagnostic: automatic target presses */

u32 hud_frame_count(void);
extern u32 g_sw_cancel_frame;   /* features/controls: frame the player dropped the target (long press), 0 none */
u64 attention_locked_actor(void);   /* features/controls: the actor the game has locked on, or 0 */
u64 g_ms_lock_actor;            /* the enemy locked on in the previous frame (automatic next target) */
u8 g_ms_r3_prev;                /* previous R3 (the shield's cancel button without the swap) */
u8 g_ms_r3_own;                 /* this R3 press cancelled the target: hidden from the game until let go */
u8 g_ms_cancel;                 /* reads left with our target press that drops the target */

/* The enemy is dead (its Life record at 0), or gone: its component chain no longer reads as an actor. */
static u32 ms_actor_dead(u64 actor) {
    if (actor < 0x10000 || (actor & 7)) return 1;
    u64 table = *(volatile u64 *)(actor + 0x270);
    if (table < 0x10000 || (table & 7)) return 1;
    u64 life = *(volatile u64 *)(table + 0x2B8);
    if (life < 0x10000 || (life & 7)) return 1;
    u64 rec = *(volatile u64 *)(life + 0x1C10);
    if (rec < 0x10000 || (rec & 7)) return 1;
    return *(volatile int *)(rec + 8) <= 0;
}

/* Diagnostic counters for tools/live_check (not used by the game). */
u32 g_ms_dbg[8] = {0x10000000u, 0, 0, 0, 0, 0, 0, 0};
/* 0 reads, 1 guard, 2 targeting, 3 ZL toggles, 4 R3 cancels, 5 stale reads, 6 frames, 7 guard entries */

static u64 rd(u64 p, unsigned off) { return *(volatile u64 *)(p + off); }

/* Gameplay (not a cutscene/event and the player exists). Movement can pause for
 * a few frames during a hit reaction, so this deliberately does not use
 * player_active(): the guard must survive those frames. */
static u32 ms_gameplay_state(void) {
    if (player_dead() || !player_get()) return 0;
    if (!hud_ingame()) return 0;   /* pause menu / item overlay covers the game */
    const char *n = player_state_name();
    if (!n || !n[0]) return 0;
    if (n[0] == 'E' && n[1] == 'v' && n[2] == 't') return 0;   /* EvtWait / event camera */
    return 1;
}

/* Read System > Options > Targeting Type from the live GameData struct. The
 * manager keeps a hash -> index map for each type (this is the option struct
 * 0x834d9d9e with the LockOnType enum member). read_enum resolves the member. */
static void refresh_target_mode(void) {
    g_ms_target_value = 0xFFFFFFFFu;
    u64 g = *(volatile u64 *)GAMEDATA_MGR;
#ifdef SHB_REKARINA
    if (g < 0x100000000ULL) { g_ms_toggle_mode = 1; return; }   /* (Rekarina: guest memory starts at 4 GB) */
#else
    if (g < 0x10000ULL) { g_ms_toggle_mode = 1; return; }
#endif
    u32 *map = *(volatile u32 **)(g + 0xC20);
    u32 slots = *(volatile u32 *)(g + 0xC2C);
    u32 count = *(volatile u32 *)(g + 0xC10);
    u8 *entries = *(volatile u8 **)(g + 0xC18);
    if (!map || !slots || slots > 1048576u || !entries) { g_ms_toggle_mode = 1; return; }
    u32 start = GD_OPTIONS_STRUCT % slots, at = start;
    do {
        if (map[at * 2] == GD_OPTIONS_STRUCT) {
            u32 index = map[at * 2 + 1];
            if (index >= count) break;
            u8 *entry = entries + (u64)index * 32;
            /* BSS out parameters: Rekarina's guest code must not receive a C stack local's address. */
            static struct { void *entry; u32 index; u32 active; } d;
            static u32 value;
            d.entry = entry;
            d.index = 0;
            d.active = *(volatile u32 *)(entry + 24) >= 2;
            value = LOCKON_TOGGLE;
            if (!read_enum((void *)g, &value, &d, GD_LOCKON_FIELD)) { g_ms_toggle_mode = 1; return; }
            g_ms_target_value = value;
            g_ms_toggle_mode = value == LOCKON_TOGGLE;
            return;
        }
        if (!map[at * 2]) break;
        at = at + 1 == slots ? 0 : at + 1;
    } while (at != start);
    g_ms_toggle_mode = 1;
}

static u64 current_guard_interface(u64 actor) {
    u64 table = rd(actor, 0x270);
    if (!table) return 0;
    u64 anim = rd(table, 0x40);
    if (!anim) return 0;
    u64 p = rd(anim, 0x18);
    if (!p) return 0;
    p = rd(p, 0x18);
    if (!p) return 0;
    return rd(p, 0x40);
}

static u32 player_carrying(u64 actor) {
    if (!actor) return 0;
    u64 table = rd(actor, 0x270);
    if (!table) return 0;
    u64 lifter = rd(table, 0x2C8);
    if (!lifter) return 0;
    return *(volatile u32 *)(lifter + 0x2B0) != 0;
}

/* Actions that keep both hands (or a weapon) busy; guard must not fight them. */
static u32 guard_action_busy(u64 state) {
    if (!state) return 0;
    for (u32 i = 0; i < 2; i++) {
        const char *name = (const char *)rd(state, 0x238 + i * 0xD8);
        if (!name) continue;
        u64 first = *(const volatile u64 *)name;
        if (first == 0x6d657449646c6f48ull || *(const volatile u32 *)name == 0x7466694c ||
            *(const volatile u32 *)name == 0x62617247 ||
            first == 0x6772616843776f42ull || first == 0x746f6f6853776f42ull ||
            first == 0x616f4c6552776f42ull || first == 0x746f68736b6f6f48ull ||
            first == 0x6e6172656d6f6f42ull || first == 0x6573726f486d6941ull)
            return 1;
    }
    return 0;
}

static u32 guard_busy(u64 iface, u64 state) {
    if (!iface) return 1;
    if (g_ms_carrying || guard_action_busy(state)) return 1;
    if (as_bool((void *)iface, 62) || as_bool((void *)iface, 86)) return 1;
    if (as_int((void *)iface, 1) == 2) return 1;
    return 0;
}

/* Is Link targeting right now? Toggle Target: our latch (or the native lock-on state, the only signal when the
 * R3 swap leaves the target to the game). Hold Target: only while the target button is held; the game's
 * LockOn* states linger after it is let go and also follow an attack, so they do not count. */
static u32 ms_target_active(void) {
    /* The game's own lock link (features/controls): valid while an enemy is locked, through attacks and hit
     * reactions too. The LockOn* action names are not used: they also show up near enemies without a target. In
     * Hold Target the held button also counts (the target camera without an enemy). */
    if (attention_locked_actor()) return 1;
    return !g_ms_toggle_mode && g_ms_zl_raw;
}

/* What the current action proves about targeting. The targeting states are all prefixed "LockOn" (LockOnWait,
 * LockOnMove, LockOnTurn...), while the plain locomotion states only exist without a target. Attacks (CutS1,
 * ChargeWait...), guards, jumps and hit reactions prove nothing: they are shared by both and must keep the
 * previous value, otherwise the target latch drops in the middle of an attack and a fresh L press reaches the
 * game's item selector. */
static u32 ms_target_class(void) {
    if (player_state_starts("LockOn")) return 1;
    if (player_state_is("Wait") || player_state_is("Move") || player_state_is("Walk") ||
        player_state_is("Brake") || player_state_is("Turn") || player_state_is("Idle")) return 2;
    return 0;
}

static void set_native_bool(u64 iface, u32 id, u32 value) {
    u64 vtable = *(volatile u64 *)iface;
    if (!vtable) return;
    void (*setter)(void *, u32, u32) = (void (*)(void *, u32, u32))*(volatile u64 *)(vtable + 0x88);
    if (setter) setter((void *)iface, id, value);
}

/* Target tracking (the toggle latch and the automatic next target after a death) is shared
 * by two independent options: it runs when either is on. Only the manual shield itself owns L, re-creates
 * ZL and touches the guard. */
static u32 ms_tracking(void) {
    return g_shbcfg.manual_shield || g_shbcfg.auto_target;
}

void manual_shield_on_input(u32 site, void *a0, void *a1, void *a2, u32 stock_result) {
    (void)a2;
    if (site != 3 || !a0) return;
    if (!ms_tracking()) {
        g_ms_active = 0;
        g_ms_held = 0;
        g_ms_allowed = 0;
        g_ms_targeting = 0;
        g_ms_zl_raw = 0;
        g_ms_l_raw = 0;
        g_ms_zl_prev = 0;
        g_ms_prev_l = 0;
        g_ms_l_latch = 0;
        g_ms_state_class = 0;
        g_ms_retarget = 0;
        g_ms_retry = 0;
        g_ms_drop = 0;
        g_ms_drop_target = 0;
        return;
    }
    const u32 ms = g_shbcfg.manual_shield;
    g_ms_active = (u8)(ms != 0);
    if (!ms) { g_ms_held = 0; g_ms_allowed = 0; g_ms_l_latch = 0; g_ms_last_allowed = 0; }
    u32 count = hid_sample_count(a1, stock_result);
    if (count == 0 || count > 64) return;
    HidView view = {a0, count, 0};

    /* The scheduler frame tells us which reads belong to a new frame. While the
     * player scheduler is paused (menus) this path still runs: pass every button
     * through untouched so the pause menu keeps its own L/ZL navigation. */
    u32 tick = g_ms_tick;
    if (tick == g_ms_last_tick) ++g_ms_stale;
    else { g_ms_last_tick = tick; g_ms_stale = 0; }
    if (g_ms_stale > 8) {
        g_ms_zl_raw = 0;
        g_ms_l_raw = 0;
        g_ms_held = 0;
        g_ms_allowed = 0;
        g_ms_dbg[5]++;
        /* A hit stop can pause the player scheduler for a few frames without leaving gameplay. While the
         * shield is up, or a target is held, L is still ours there: consume it so the default L/item
         * function cannot fire in the middle of the hit or the attack. */
        if (ms && (g_ms_last_allowed || g_ms_targeting || g_ms_l_latch) && ms_gameplay_state()) {
            for (u32 i = 0; i < count; i++) {
                u64 b = hid_get_buttons(&view, i);
                hid_set_buttons(&view, i, b & ~ms_guard_bit());
            }
        }
        return;
    }
    if (player_dead()) {   /* death ragdoll / game over: never filter the screen's input */
        g_ms_zl_raw = 0;
        g_ms_l_raw = 0;
        g_ms_held = 0;
        g_ms_allowed = 0;
        g_ms_targeting = 0;
        g_ms_state_class = 0;
        g_ms_retarget = 0;
        g_ms_retry = 0;
        g_ms_drop = 0;
        g_ms_drop_target = 0;
        g_ms_last_allowed = 0;
        return;
    }
    if (!ms_gameplay_state()) {   /* title, loading, pause or event camera: keep the state from firing */
        g_ms_zl_raw = 0;
        g_ms_l_raw = 0;
        g_ms_held = 0;
        g_ms_allowed = 0;
        return;
    }

    u64 all = hid_merge_buttons(&view);
    u8 zl = (u8)((all & MS_BTN_ZL) != 0);
    u8 l = (u8)((all & ms_guard_bit()) != 0);
    u8 r3 = (u8)((all & MS_BTN_R3) != 0);
    g_ms_zl_raw = zl;
    g_ms_l_raw = l;
    g_ms_prev_l = l;
    /* The shield's R3 cancel (without the ZL/R3 swap): a press of R3 while locked drops the target with a short
     * press of the target button of ours (the game's toggle lets go), and R3 itself does not reach the game. */
    /* (not with the remap: R3 is whatever the player chose, the camera by default) */
    if (ms && !remap_on() && !g_shbcfg.target_r3 && g_ms_toggle_mode && r3 && !g_ms_r3_prev && attention_locked_actor()) {
        g_ms_r3_own = 1;
        g_ms_cancel = 4;
        g_sw_cancel_frame = hud_frame_count() | 1u;   /* (the automatic next target ignores this drop) */
        g_ms_dbg[4]++;
    }
    if (!r3) g_ms_r3_own = 0;
    g_ms_r3_prev = r3;
    g_ms_dbg[1] = g_ms_held;
    g_ms_dbg[2] = g_ms_targeting;

    /* L belongs to the manual shield only while Link is targeting or the shield is up (including its hit
     * reactions); with no target it reaches the game and keeps its default function. */
    u32 toggle = g_ms_toggle_mode;
    u32 guard_active = g_ms_held != 0;
    u32 own_l = ms && (ms_target_active() || guard_active || g_ms_last_allowed || g_ms_l_latch);
    /* An L press that started as ours stays ours until it is let go (letting go of the target while
     * holding L must not open the item selector halfway through the press). */
    g_ms_l_latch = (u8)(l && own_l);
    (void)toggle;
    /* The target button always reaches the game as the player presses it (the game's own Hold/Toggle decides);
     * only the presses of ours below are added. (It used to be re-created from a latch fed by the LockOn*
     * action names, which also show up near enemies: the game then locked on by itself.) */
    for (u32 i = 0; i < count; i++) {
        u64 b = hid_get_buttons(&view, i);
        /* While targeting or guarding L is ours, including during shield-hit reactions
         * (Guard/Repelled/GuardBreak): otherwise the default L (item/selector) fires for a frame in the
         * middle of the hit. */
        if (own_l) b &= ~ms_guard_bit();
        if (g_ms_r3_own) b &= ~MS_BTN_R3;
        /* The target died: press target again so the game picks the next one. The game sees ZL (controls.c
         * already swapped R3 into it), and a short hold is one press for the toggle. Also the R3 cancel. */
        if (g_ms_retarget || g_ms_cancel) b |= MS_BTN_ZL;
        hid_set_buttons(&view, i, b);
    }
    if (g_ms_cancel) g_ms_cancel--;
    g_ms_dbg[0]++;
}

void manual_shield_on_frame(void *scheduler) {
    g_ms_iface = 0;
    g_ms_equipment = 0;
    g_ms_carrying = 0;
    g_ms_allowed = 0;
    if (!ms_tracking()) {
        g_ms_active = 0;
        g_ms_state = 0;
        g_ms_tick = 0;
        g_ms_last_tick = 0;
        g_ms_stale = 0;
        g_ms_held = 0;
        g_ms_targeting = 0;
        g_ms_zl_raw = 0;
        g_ms_l_raw = 0;
        g_ms_zl_prev = 0;
        g_ms_prev_l = 0;
        g_ms_l_latch = 0;
        g_ms_last_allowed = 0;
        g_ms_toggle_mode = 1;
        g_ms_target_value = 0xFFFFFFFFu;
        g_ms_state_class = 0;
        g_ms_retarget = 0;
        g_ms_retry = 0;
        g_ms_drop = 0;
        g_ms_drop_target = 0;
        return;
    }
    const u32 ms = g_shbcfg.manual_shield;
    g_ms_active = (u8)(ms != 0);   /* the guard wrappers (manual_shield.S) act only for the shield itself */
    ++g_ms_tick;
    g_ms_dbg[6]++;
    g_ms_held = 0;
    if (g_ms_retarget) --g_ms_retarget;

    /* The game's Targeting Type can change in System > Options. Refresh it a
     * few times per second so the ZL behaviour follows the setting. */
    if (g_ms_tick == 1u || (g_ms_tick % 10u) == 0u) refresh_target_mode();

    /* Toggle Target: the target button (ZL, or R3 when target_r3 swaps them and controls.c has already
     * exchanged the bits) is a latched toggle, one rising edge per press. Hold Target: the game itself owns
     * the hold; keep our latch off. */
    if (ms_gameplay_state()) {
        u8 zl = g_ms_zl_raw;
        if (g_ms_retarget) zl = 0;   /* this frame's press is ours, not the player's */
        if (g_ms_toggle_mode) {
            /* The current action is the first word: LockOn* proves a target, Wait/Move/Walk/... prove there
             * is none, and the shared attack, guard, jump and hit states say nothing. */
            u32 cls = ms_target_class();
            if (cls == 1) {
                g_ms_targeting = 1;
                g_ms_retry = 0;
                g_ms_drop = 0;
            } else if (cls == 2) {
                /* The game let go of the target without our button: the target died or vanished. Press
                 * target again so the game locks on to the next enemy; it does nothing when there is none.
                 * Only after the plain state lasts a few frames (a one-frame Move in the middle of a
                 * targeting transition must not cancel a target that is still there), and once per lost
                 * target, so a failed press does not turn into a loop. */
                if (g_ms_drop < 200u) g_ms_drop++;
                g_ms_targeting = 0;
            } else {
                g_ms_drop = 0;   /* attacks, guards, jumps and hit reactions: no proof either way */
            }
            g_ms_state_class = cls;
            /* Automatic next target. The game's own lock link (features/controls) says which enemy is locked;
             * the LockOn* action names do not, they also show up near enemies without a target. When the
             * game lets go of an enemy that has died, press target once so it locks on to the next one (it
             * does nothing when there is none). A target the player dropped (a long press of the target
             * button) is never followed by a press of ours. */
            {
                u64 cur = attention_locked_actor();
                if (cur) {
                    g_ms_lock_actor = cur;
                } else if (g_ms_lock_actor) {
                    u64 lost = g_ms_lock_actor;
                    g_ms_lock_actor = 0;
                    u32 dropped = g_sw_cancel_frame && hud_frame_count() - g_sw_cancel_frame < 90u;
                    if (g_shbcfg.auto_target && !dropped && !zl && ms_actor_dead(lost)) {
                        g_ms_retarget = 3;
                        g_ms_press_tick = g_ms_tick;
                        g_ms_retargets++;
                    }
                }
            }
            /* The player's own press (the game's toggle): one rising edge per press. */
            if (zl && !g_ms_zl_prev) {
                g_ms_targeting ^= 1u;
                g_ms_dbg[3]++;
                g_ms_press_tick = g_ms_tick;
                g_ms_retry = 0;
            }
        } else {
            /* Hold Target: the button itself says whether Link is targeting, keep the latch off. */
            g_ms_targeting = 0;
            g_ms_state_class = 0;
            g_ms_retarget = 0;
            g_ms_drop = 0;
        }
        g_ms_zl_prev = zl;
    } else {
        g_ms_zl_raw = 0;
        g_ms_l_raw = 0;
        g_ms_zl_prev = 0;
        g_ms_targeting = 0;
        g_ms_state_class = 0;
        g_ms_retarget = 0;
        g_ms_retry = 0;
        g_ms_drop = 0;
        g_ms_drop_target = 0;
        g_ms_last_allowed = 0;
    }

    if (!scheduler) return;
    u64 state = (u64)resolve_player(scheduler);
    if (!state) { g_ms_state = 0; return; }
    g_ms_state = state;
    u64 actor = rd(state, 0x18);
    if (!actor) return;
    g_ms_carrying = (u8)player_carrying(actor);
    g_ms_held = (u8)(!g_ms_carrying && g_ms_l_raw && ms_target_active());
    u64 component = get_guard_component((void *)actor);
    if (!component) return;

    u64 iface = current_guard_interface(actor);
    if (iface) g_ms_iface = iface;
    u64 table = rd(actor, 0x270);
    u64 equipment = table ? rd(table, 0x200) : 0;
    if (iface && equipment) g_ms_equipment = equipment;

    /* Without the manual shield the guard stays the game's own: no flags, no setter, nothing drawn. */
    if (!ms) { g_ms_held = 0; g_ms_last_allowed = 0; return; }

    /* Carrying or lifting something: the guard is off anyway, and the flag writes below (clearing the player's state
     * flag and the guard component flag every frame) made the game throw the object by itself a moment after the lift
     * (found live: with the option off the jar stayed in his hands). Leave the game's flags alone meanwhile. */
    if (g_ms_carrying) { g_ms_allowed = 0; g_ms_last_allowed = 0; return; }

    /* The option is standalone: no stamina gate (the stamina feature still
     * charges its own held/hit costs if it is enabled). */
    u32 held = g_ms_held != 0;
    u32 alive = ms_gameplay_state();
    u32 allowed = iface && held && alive && !guard_busy(iface, state);
    g_ms_allowed = (u8)(allowed != 0);

    /* Match the native guard flag without requesting target/camera. */
    u32 *state_flags = (u32 *)(state + 0xE50);
    if (allowed) *state_flags |= 0x80100u;
    else *state_flags &= ~0x80000u;
    u32 *flags = (u32 *)(component + 0x24);
    if (allowed) *flags |= 0x20u;
    else *flags &= ~0x20u;

    /* The stock setter would raise the shield on targeting; the wrapper keeps it
     * scoped to this interface + Bool14 and follows g_ms_allowed. */
    if (iface) set_native_bool(iface, 14, allowed);

    /* Draw only the shield: targeting must not make the hook draw the sword. */
    if (allowed && !g_ms_last_allowed) {
        if (equipment && (!iface || as_int((void *)iface, 1) != 2)) {
            native_draw_equipment((void *)equipment, 4);
            g_ms_dbg[7]++;
        }
    }
    g_ms_last_allowed = (u8)(allowed != 0);
}
