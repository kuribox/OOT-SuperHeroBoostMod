#include "../../core/dispatch.h"
#include "../../core/hid.h"
#include "stamina.h"
#include "../../core/config.h"

/* Stamina system hooks (independent implementation).
 *
 *  - Y (sword): costs stamina during combat; blocked at 0 in combat.
 *  - A (dodge): costs stamina only when it actually rolls, i.e. when pressed with the
 *    stick deflected (standing still A costs nothing).
 *  - Running (stick fully deflected) drains stamina gradually; at 0 the stick is halved so
 *    the player can only walk.
 *  - X (jump): costs stamina; with none left the jump is blocked.
 *  Once empty, stamina must recover to EXHAUST_RECOVER before running/jumping/rolling again.
 * Values are placeholders (stamina_default_config + the constants below).
 */

void hud_set_stamina(float value, float cap, float show);
u32 hud_frame_count(void);
void hud_set_tired(unsigned int t);
u32 player_take_events(void);
u32 player_guard_seq(void);
static u32 g_guard_seen, g_inactive_since;
static volatile u32 g_sprint_a;   /* A held and not taken away by the stamina rules (last input read) */

/* Ground-movement hook (main+0x9352F0, x0 player, x1 direction, s0 speed). Only the player's own call (it returns to
 * main+0x934994) is touched: while A (or L3, when enabled) is held his speed is multiplied by the sprint setting, as
 * the game's own sprint is "A held while moving". Rolling, swimming and climbing do not go through this call. */
extern char MAIN_SCHED_VT[];
u32 g_speed_dbg[2];   /* calls from the player's site, calls boosted */
void stamina_speed_hook(void *regs) {
    unsigned pct = g_shbcfg.sprint_speed_pct;
    if (pct == 100 || pct < 50 || pct > 400) return;
    u64 base = (u64)MAIN_SCHED_VT - 0x4B05318ull;
    u64 lr = *(u64 *)((unsigned char *)regs + 0x08);   /* saved x30 = return address of this call */
    if (lr != base + 0x934994ull) return;
    g_speed_dbg[0]++;
    if (!g_sprint_a) return;
    float *s0 = (float *)((unsigned char *)regs + 0xB0);   /* saved d0: the speed argument */
    *s0 = *s0 * (float)pct / 100.0f;
    g_speed_dbg[1]++;
}
int player_state_is(const char *name);
int player_state_starts(const char *prefix);
int player_active(void);
int player_dead(void);
int player_max_life(void);
int player_in_combat(void);
void player_mark_combat(void);

/* Capacity: 75 (= 75% of the original 100) up to 3 hearts; every heart after the 3rd adds 75/17, so 20 hearts = 150. */
#define STAMINA_BASE 75.0f
#define STAMINA_PER_HEART (STAMINA_BASE / 17.0f)
#define PEV_ROLL 1u
#define PEV_SIDESTEP 2u
#define PEV_JUMP 4u
#define PEV_ATTACK 8u
#define PEV_GUARDHIT 16u
#define PEV_SHOT 32u
#define PEV_BOOMERANG 64u
#define PEV_HAMMER 128u
#define PEV_BACKFLIP 256u

#define LIFE_QUARTERS 20u       /* placeholder: 5 hearts, not read from the game yet */
#define COMBAT_HOLD_FRAMES 180u
#define DT_FRAME (1.0f / 60.0f)

#define JUMP_COST 8.0f
#define RUN_DRAIN_PER_SEC 7.0f
#define EXHAUST_RECOVER 15.0f
#define REGEN_DELAY 0.8f             /* seconds without actions before refilling */
#define COMBAT_REGEN_DELAY 1.8f      /* ... while in combat (an enemy is on the player or he just fought) */
#define REGEN_PER_SEC 24.0f
#define TIRED_REGEN_DELAY 2.5f       /* exhausted (yellow): longer wait ... */
#define TIRED_REGEN_PER_SEC 48.0f    /* ... then twice as fast */
#define STICK_MAX 32767.0f
#define STICK_MOVE2 (0.30f * 0.30f * STICK_MAX * STICK_MAX)   /* deflected enough to roll/walk */
#define STICK_RUN2 (0.85f * 0.85f * STICK_MAX * STICK_MAX)    /* running */
#define STICK_FAST2 (0.55f * 0.55f * STICK_MAX * STICK_MAX)   /* limited when exhausted */

/* Diagnostics read by tools/live_check.py (in .data so they are file-backed). */
u64 g_st_seen = 0x100000000ull;  /* OR of every button mask seen at site 3 (+base) */
u32 g_st_count = 0x10000000u;
u32 g_st_dbg[6] = {0x10000000u, 0x10000000u, 0x10000000u, 0x10000000u, 0x10000000u, 0x10000000u};   /* rolls, jumps, run frames, exhausted */

static StaminaState g_stamina;
static StaminaConfig g_config;
static u32 g_initialized;
static u64 g_prev_buttons;
static u32 g_action_token;
static u32 g_combat_hold;
static u32 g_exhausted;
static u32 g_last_frame;
static u32 g_same_reads;

/* ---- configuration (core/config.h, patched by the companion app) ---- */
enum { K_JUMP, K_ATTACK, K_SPRINT, K_BLOCK, K_SHOOT, K_ROLL };
static int combat_only_mode(void) {
    return g_shbcfg.preset == 1 || (g_shbcfg.preset == 3 && g_shbcfg.combat_only);
}
static int sprint_enabled(void) {
    return g_shbcfg.preset == 3 ? g_shbcfg.cost_sprint : 1;   /* every other preset drains while sprinting */
}
/* does this kind of action cost stamina right now under the current preset? Sprinting is never limited to combat. */
static int cost_on(int kind) {
    unsigned p = g_shbcfg.preset;
    int on;
    if (kind == K_SPRINT) return sprint_enabled();
    if (p == 2) on = 0;                                      /* sprint only (sprint-only style): nothing else costs */
    else if (p == 3)                                         /* advanced: each one by hand */
        on = kind == K_JUMP ? g_shbcfg.cost_jump : kind == K_ATTACK ? g_shbcfg.cost_attack :
             kind == K_BLOCK ? g_shbcfg.cost_block : kind == K_SHOOT ? g_shbcfg.cost_shoot : g_shbcfg.cost_roll;
    else on = 1;                                             /* hardcore / hardcore combat only: everything */
    if (!on) return 0;
    if (kind == K_BLOCK) return 1;                           /* taking a hit on the shield is combat by definition (ranged attackers are not "near") */
    return !combat_only_mode() || player_in_combat();
}

static void ensure_init(void) {
    if (g_initialized) return;
    stamina_default_config(&g_config);
    stamina_init(&g_stamina, &g_config, LIFE_QUARTERS);
    g_stamina.cap = STAMINA_BASE;
    g_stamina.stamina = STAMINA_BASE;
    g_initialized = 1;
}

/* NpadFullKeyState: left stick = two s32 at +0x10 (x, y). Returns max squared deflection. */
int g_stick_dbg[16] = {0x7777};
static float stick_mag2(const HidView *v) {
    float best = 0.0f;
    for (u32 i = 0; i < v->count; i++) {
        volatile int *p = (volatile int *)((unsigned char *)v->samples + (u64)i * HID_STRIDE + 0x10);
        if (i < 8) { g_stick_dbg[i * 2] = p[0]; g_stick_dbg[i * 2 + 1] = p[1]; }
        float x = (float)p[0], y = (float)p[1];
        float m = x * x + y * y;
        if (m > best) best = m;
    }
    return best;
}

static void halve_stick(const HidView *v) {
    for (u32 i = 0; i < v->count; i++) {
        volatile int *p = (volatile int *)((unsigned char *)v->samples + (u64)i * HID_STRIDE + 0x10);
        p[0] = p[0] / 2;
        p[1] = p[1] / 2;
    }
}

/* L3 sprint: the game only plays the sprint animation while A is held, and pressing A while
 * already moving makes Link roll first. So L3 is injected as A, and on the frame the A press
 * arrives while the stick is pushed the stick is centred for two frames: the game sees the
 * press while standing, which is the "hold A then move" case (sprint, no roll). */
static u32 g_l3_a;         /* A is being injected for L3 */
static u32 g_l3_neutral;   /* frames left with the stick centred (avoid the roll) */
static u32 g_l3_frame;     /* last rendered frame seen here */

/* Toggle mode (sprint_l3_toggle): a press of L3 turns the sprint on, the next one off. It also stops when stamina
 * runs out, when the player presses A (their own roll / sprint takes over) and in events (dialogues, shops: an
 * injected A would answer them). */
static u32 g_l3_on, g_l3_prev;
/* A toggled sprint also ends when the player stops moving: armed once he has moved since the toggle (so a toggle
 * pressed standing still waits for the first step), then L3_IDLE_FRAMES rendered frames with the stick at rest. */
#define L3_IDLE_FRAMES 12u
static u32 g_l3_armed, g_l3_idle, g_l3_idle_frame;
/* Link is carrying or lifting something (jar, bush, rock): the sprint's A would throw it, so none is injected; the
 * sprint then is only the speed boost (g_sprint_a), so he runs with the object. Set by features/manual_shield (its frame
 * hook runs whatever the guard option is). */
extern unsigned char g_ms_carrying;
/* L3 is sprinting (held, or toggled on) */
static u32 l3_sprint(u64 b) {
    if (!g_shbcfg.sprint_l3) return 0;
    return g_shbcfg.sprint_l3_toggle ? g_l3_on : (b & BTN_L3) != 0;
}
int hud_game_covered(void);   /* core/hud.c: a menu covers the game */
static void l3_inject_a(const HidView *v, u64 buttons, u32 moving) {
    /* Never under a menu: an A held there is the menu's A (on the pause menu's Items tab it puts the highlighted
     * item in the item box, which took items off the mod's buttons). A toggled sprint stops with the pause. */
    if (hud_game_covered()) {
        g_l3_on = 0; g_l3_a = 0; g_l3_neutral = 0;
        g_l3_prev = (buttons & BTN_L3) != 0;
        return;
    }
    u32 l3 = (buttons & BTN_L3) != 0, want;
    /* Lifting or carrying: the object flag only turns on once the lift is done, so the lift states count too (their
     * first frames are exactly when an injected A would throw the object again). The toggled sprint is dropped as the
     * lift starts: it is what picked the object up (an injected A next to it), and left on it threw it again at once
     * and on every new pickup. Pressing the sprint button again while carrying only gives the speed. */
    u32 carrying = g_ms_carrying != 0 || player_state_starts("Lift") || player_state_starts("Grab") ||
                   player_state_starts("HoldItem");
    static u32 carry_prev;
    if (carrying && !carry_prev) g_l3_on = 0;
    carry_prev = carrying;
    if (g_shbcfg.sprint_l3 && g_shbcfg.sprint_l3_toggle) {
        if (l3 && !g_l3_prev) { g_l3_on = !g_l3_on; g_l3_armed = 0; g_l3_idle = 0; }
        if ((buttons & BTN_A) || g_exhausted || player_state_starts("Evt")) g_l3_on = 0;
        if (g_l3_on) {
            u32 f = hud_frame_count();
            if (moving) { g_l3_armed = 1; g_l3_idle = 0; }
            else if (g_l3_armed && f != g_l3_idle_frame) {   /* stopped: the sprint lets go by itself */
                g_l3_idle_frame = f;
                if (++g_l3_idle >= L3_IDLE_FRAMES) g_l3_on = 0;
            }
        }
        want = g_l3_on;
    } else {
        g_l3_on = 0;
        want = g_shbcfg.sprint_l3 && l3;
    }
    g_l3_prev = l3;
    if (!want || (buttons & BTN_A) || carrying) {   /* (carrying: no A, it would throw the object) */
        g_l3_a = 0;
        g_l3_neutral = 0;
        return;
    }
    u32 now = hud_frame_count();
    u32 new_frame = now != g_l3_frame;
    g_l3_frame = now;
    if (!g_l3_a && moving) g_l3_neutral = 2;
    g_l3_a = 1;
    for (u32 i = 0; i < v->count; i++)
        hid_set_buttons(v, i, hid_get_buttons(v, i) | BTN_A);
    if (g_l3_neutral) {
        for (u32 i = 0; i < v->count; i++) {
            volatile int *p = (volatile int *)((unsigned char *)v->samples + (u64)i * HID_STRIDE + 0x10);
            p[0] = 0;
            p[1] = 0;
        }
        if (new_frame) g_l3_neutral--;
    }
}

#define RUN_HOLD_FRAMES 12u
static u32 g_run_hold;
u32 g_st_guard;           /* diagnostic: guard hits that cost stamina */
float g_st_lowest = 1.0e9f;   /* diagnostic: lowest stamina right after a guard hit */
static void spend(float cost) {
    g_stamina.stamina -= cost * g_stamina.drain_multiplier;
    if (g_stamina.stamina < 0.0f) g_stamina.stamina = 0.0f;
    g_stamina.idle_seconds = 0.0f;
}

void stamina_on_input(u32 site, void *a0, void *a1, void *a2, u32 stock_result) {
    (void)a2;
    ensure_init();
    if (!a0) return;

    /* Same argument reading as the input wrappers: a1 is the sample count. */
    u32 count = hid_sample_count(a1, stock_result);
    if (count == 0 || count > 64) return;

    HidView view = {a0, count, 0};

    if (site != 3) return;   /* only site 3 runs during gameplay */
    if (player_dead()) {     /* death ragdoll / game over: never filter the menu's buttons */
        g_sprint_a = 0;
        g_l3_a = 0;
        g_l3_on = 0;
        g_l3_neutral = 0;
        player_take_events();
        hud_set_tired(0);
        hud_set_stamina(g_stamina.stamina, g_stamina.cap, 0.0f);
        return;
    }
    if (!g_shbcfg.stamina_enabled) {   /* stamina system switched off in the config */
        u64 b = hid_merge_buttons(&view);
        l3_inject_a(&view, b, stick_mag2(&view) > STICK_MOVE2);
        g_sprint_a = ((b & BTN_A) || l3_sprint(b)) != 0;
        player_take_events();
        hud_set_tired(0);
        hud_set_stamina(g_stamina.stamina, g_stamina.cap, 0.0f);
        return;
    }

    u64 buttons = hid_merge_buttons(&view);
    g_st_seen |= buttons;
    g_st_count = count;
    u64 pressed = buttons & ~g_prev_buttons;
    g_prev_buttons = buttons;

    float m2 = stick_mag2(&view);
    u32 moving = m2 > STICK_MOVE2;
    /* Sprint: A held, or L3 held when the app option is on (L3 is injected as A further down,
     * so the game's sprint animation plays; the stick is centred on the press to avoid the roll). */
    u32 sprint_held = ((buttons & BTN_A) || l3_sprint(buttons)) != 0;
    u32 running = sprint_held && moving;

    /* State advances once per rendered frame (the reader can run more than once). */
    u32 now = hud_frame_count();
    u32 frames = now - g_last_frame;
    int new_frame = (now == 0) || frames != 0;
    if (frames > 4) frames = 1;
    if (frames == 0 && now != 0) g_same_reads++; else g_same_reads = 0;
    g_last_frame = now;
    /* Outside the world (menus, pause, game over, dead) nothing of this runs. */
    if (!(player_active() && g_same_reads <= 8)) {
        player_take_events();
        g_l3_a = 0;
        g_l3_neutral = 0;
        /* shield hits survive a short pause (the player's hit reaction); after a real break (menu, loading) they are dropped */
        if (!g_inactive_since) g_inactive_since = now ? now : 1;
        if (now - g_inactive_since > 60u) g_guard_seen = player_guard_seq();
        g_combat_hold = 0;
        hud_set_tired(0);
        hud_set_stamina(g_stamina.stamina, g_stamina.cap, 0.0f);
        return;
    }
    g_inactive_since = 0;
    float dt = (now == 0 ? 1u : frames) * DT_FRAME;

    if (g_exhausted && g_stamina.stamina >= g_stamina.cap - 0.5f) g_exhausted = 0;   /* must refill completely */
    if (!g_exhausted && g_stamina.stamina <= 0.0f) g_exhausted = 1;

    if (new_frame) {
        int mq = player_max_life();
        if (mq >= 4) {                                  /* quarters -> hearts */
            int hearts = mq / 4;
            if (hearts < 3) hearts = 3;
            if (hearts > 20) hearts = 20;
            float cap = STAMINA_BASE + (float)(hearts - 3) * STAMINA_PER_HEART;
            if (cap != g_stamina.cap) g_stamina.cap = cap;
        }
        if (g_stamina.stamina > g_stamina.cap) g_stamina.stamina = g_stamina.cap;
        /* Costs come from the player's ACTIONS (player_track), not from button presses. */
        u32 ev = player_take_events();
        if ((ev & PEV_ATTACK) && cost_on(K_ATTACK)) {
            g_combat_hold = COMBAT_HOLD_FRAMES;
            g_stamina.in_combat = 1;   /* the first swing already counts */
            stamina_begin_action(&g_stamina, &g_config, ACTION_SLASH, 0, 0, ++g_action_token);
            g_st_dbg[4]++;
        }
        if ((ev & PEV_ROLL) && cost_on(K_ROLL)) {
            g_combat_hold = COMBAT_HOLD_FRAMES;
            spend(g_config.roll_cost);
            g_st_dbg[0]++;
        }
        if ((ev & PEV_BACKFLIP) && cost_on(K_ROLL)) {
            g_combat_hold = COMBAT_HOLD_FRAMES;
            spend(g_config.backflip_cost);
        }
        if ((ev & PEV_SIDESTEP) && cost_on(K_ROLL)) {
            g_combat_hold = COMBAT_HOLD_FRAMES;
            spend(g_config.sidestep_cost);
        }
        if ((ev & PEV_SHOT) && cost_on(K_SHOOT)) {   /* bow / slingshot: half an attack */
            g_combat_hold = COMBAT_HOLD_FRAMES;
            spend(g_config.weapon_cost[0] * 0.5f);
        }
        if ((ev & PEV_BOOMERANG) && cost_on(K_ATTACK)) {
            g_combat_hold = COMBAT_HOLD_FRAMES;
            spend(g_config.weapon_cost[0] * g_config.move_cost_scale[1]);
        }
        if ((ev & PEV_HAMMER) && cost_on(K_ATTACK)) {
            g_combat_hold = COMBAT_HOLD_FRAMES;
            spend(g_config.weapon_cost[2]);
        }
        u32 gseq = player_guard_seq(), ghits = gseq - g_guard_seen;
        g_guard_seen = gseq;
        if (ghits && ghits < 8 && cost_on(K_BLOCK)) {   /* a hit on the shield costs stamina */
            g_combat_hold = COMBAT_HOLD_FRAMES;
            spend(g_config.block_base_cost);
            g_st_guard++;
            player_mark_combat();
            if (g_stamina.stamina < g_st_lowest) g_st_lowest = g_stamina.stamina;
        }
        if ((ev & PEV_JUMP) && cost_on(K_JUMP)) {
            spend(JUMP_COST);
            g_st_dbg[1]++;
        }
        u32 in_combat = g_combat_hold != 0;
        if (g_combat_hold) g_combat_hold--;
        g_stamina.in_combat = in_combat;

        /* Sprint: A held while the move state is active drains gradually. */
        /* A roll starts with A + stick while the player is still in "Move" for a few frames: that is not a sprint.
         * Sprinting only counts once A has been held for RUN_HOLD_FRAMES in the Move state. */
        if (running && player_state_is("Move")) g_run_hold += (frames ? frames : 1); else g_run_hold = 0;
        u32 draining = g_run_hold >= RUN_HOLD_FRAMES && !g_exhausted && g_stamina.stamina > 0.0f && cost_on(K_SPRINT);
        if (draining) {
            g_stamina.stamina -= RUN_DRAIN_PER_SEC * dt * g_stamina.drain_multiplier;
            if (g_stamina.stamina < 0.0f) g_stamina.stamina = 0.0f;
            g_stamina.idle_seconds = 0.0f;
            g_st_dbg[2]++;
        }
        /* Refill: after a short pause without any action (moving is fine); once exhausted the
         * pause is longer but the refill is twice as fast. spend()/drain reset idle_seconds. */
        g_stamina.idle_seconds += dt;
        float wait = g_exhausted ? TIRED_REGEN_DELAY : (in_combat || player_in_combat()) ? COMBAT_REGEN_DELAY : REGEN_DELAY;
        if (g_stamina.idle_seconds >= wait)
            g_stamina.stamina += (g_exhausted ? TIRED_REGEN_PER_SEC : REGEN_PER_SEC) * dt;
        if (g_stamina.stamina > g_stamina.cap) g_stamina.stamina = g_stamina.cap;        if (g_stamina.stamina <= 0.0f) g_exhausted = 1;
        g_st_dbg[3] = 0x10000000u + g_exhausted;
        g_st_dbg[5] = 0x10000000u + (u32)(player_in_combat() != 0);   /* diagnostic: in combat now */

        hud_set_tired(g_exhausted);
        hud_set_stamina(g_stamina.stamina, g_stamina.cap,
                        (in_combat || draining || g_exhausted || g_stamina.stamina < g_stamina.cap - 0.5f)
                            ? 1.0f : 0.0f);
    }
    /* L3: inject A so the game's own sprint (and its animation) plays, without the roll. */
    l3_inject_a(&view, buttons, moving);

    /* Filters on the input the game sees (every call, not only the first of a frame). */
    u64 strip = 0;
    unsigned preset = g_shbcfg.preset;
    int lock = g_exhausted;                       /* exhausted (yellow): the costly actions are blocked */
    if (combat_only_mode() && !player_in_combat()) lock = 0;
    if (lock) {
        if (preset == 2) strip |= BTN_A;          /* sprint only: no sprinting (A) */
        else if (preset == 3) {                   /* advanced: block only what costs */
            if (g_shbcfg.cost_jump) strip |= BTN_X;
            if (g_shbcfg.cost_roll || g_shbcfg.cost_sprint) strip |= BTN_A;
            if (g_shbcfg.cost_attack) strip |= BTN_Y;
            if (g_shbcfg.cost_block) strip |= BTN_ZL;
            if (g_shbcfg.cost_attack || g_shbcfg.cost_shoot) strip |= BTN_B | BTN_L | BTN_R | BTN_ZR;
        } else strip |= BTN_A | BTN_B | BTN_X | BTN_Y | BTN_L | BTN_R | BTN_ZL | BTN_ZR;   /* only walking is left */
    } else if (g_exhausted && sprint_enabled()) {
        strip |= BTN_A;                           /* combat-only presets: sprinting is still limited out of combat */
    }
    if (g_stamina.in_combat && g_stamina.stamina <= 0.0f && cost_on(K_ATTACK)) strip |= BTN_Y;   /* no sword at 0 */
    /* Dialogues, shops and other events (Evt* states) choose with the stick and confirm with A / B: never filtered. */
    if (player_state_starts("Evt")) { strip = 0; lock = 0; }
    /* Menus (pause, map ...) keep every button: the pause menu assigns items with X / Y / L / R. */
    if (hud_game_covered()) { strip = 0; lock = 0; }
    if (lock && preset != 2 && m2 > STICK_FAST2) halve_stick(&view);   /* exhausted: walk only */
    g_sprint_a = sprint_held && !(strip & BTN_A);
    if (strip) {
        for (u32 i = 0; i < count; i++) {
            u64 b = hid_get_buttons(&view, i);
            hid_set_buttons(&view, i, b & ~strip);
        }
    }
}
