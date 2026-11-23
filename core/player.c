#include "dispatch.h"
#include "config.h"
#include "clock.h"

/* Finds the player from the per-frame movement update of physics actors (hook at main+0x2C215E0,
 * x0 = actor) and turns his action-state name into events.
 * Verified live: [actor+0x270] = component table; table+0x2B8 = Life component whose resource
 * (life+0x10 -> +0x58) is named "Player" for the player; table+0x358 = player component; the current
 * action-state name is an inline string at player+0x244 ("Wait", "Move", "Jump", "Land",
 * "Rolling", "SideStep", "SideStepLand", "CutS1", "ChargeWait", "LockOnWait", ...). */
typedef unsigned long long u64;
typedef unsigned int u32;
u64 g_player_actor = 1;
u64 g_player_comp = 1;
u64 g_player_table = 1;
u32 g_player_seen = 0x10000000u;
int g_player_life = 0x7777;
int g_player_maxlife = 0;   /* max life in quarter hearts (Life+0x1C18 -> record, s32 at +8) */
static u32 g_active_frame;
static int g_prev_life = -1;
u32 g_dmg_dbg;
static int g_alive;
u32 hud_frame_count(void);

/* events (bit mask), consumed by features through player_take_events() */
#define PEV_ROLL 1u
#define PEV_SIDESTEP 2u
#define PEV_JUMP 4u
#define PEV_ATTACK 8u
#define PEV_GUARDHIT 16u
#define PEV_SHOT 32u
#define PEV_BOOMERANG 64u
#define PEV_HAMMER 128u
#define PEV_BACKFLIP 256u
static u32 g_pending;
static char g_prev[24];
static char g_cur[24] = {'W', 'a', 'i', 't'};
static char g_prev2[24], g_cur2[24];
static u32 g_combat_frame;   /* last frame a hostile enemy was near the player */   /* upper-body state name at player+0x34C (bow / slingshot) */
u32 g_ev_count[8] = {0x10000000u};
char g_names_seen[16][24] = {{1}};
u32 g_names_n;

static u64 rd(u64 p, unsigned off) { return *(volatile u64 *)(p + off); }

static int eq(const char *a, const char *b) {
    for (int i = 0; i < 24; i++) {
        if (a[i] != b[i]) return 0;
        if (!a[i]) return 1;
    }
    return 1;
}
static int has_hit(const char *a) {
    for (int i = 0; i < 20 && a[i]; i++) if (a[i] == 'H' && a[i + 1] == 'i' && a[i + 2] == 't') return 1;
    return 0;
}
static int has_word(const char *a, const char *w) {
    for (int i = 0; i < 20 && a[i]; i++) { int k = 0; while (w[k] && a[i + k] == w[k]) k++; if (!w[k]) return 1; }
    return 0;
}
static int starts(const char *a, const char *p) {
    for (int i = 0; p[i]; i++) if (a[i] != p[i]) return 0;
    return 1;
}

u32 g_ev[5];   /* test counters: roll group, jump, attack, guard hit, shot (events seen, costed or not) */
/* A shield hit can be seen twice (the game's reaction node and the state name): one hit = one event. */
u32 g_guard_calls[4];          /* diagnostic: calls of each hooked reaction node */
static u32 g_guard_frame;
static u32 g_guard_seq;
static void guard_hit(void) {
    u32 f = hud_frame_count();
    if (g_guard_frame && f - g_guard_frame < 40u) return;
    g_guard_frame = f ? f : 1;
    g_guard_seq++;
    g_ev[3]++;
}
/* Shield hits are counted, not queued as an event: the hit arrives while the player is in his hit reaction, when
 * his movement update (and so player_active()) pauses for a few frames and queued events are dropped. */
u32 player_guard_seq(void) { return g_guard_seq; }
void player_guard_event(u32 id) { g_guard_calls[id - 6]++; guard_hit(); }
char g_guard_names[16][24];  /* diagnostic: every distinct state name that starts with "Guard" */
u32 g_guard_n;
static void classify(const char *n) {
    if (starts(n, "Guard") || starts(n, "Shield")) {
        int known = 0;
        for (u32 k = 0; k < g_guard_n && k < 16; k++) if (eq(g_guard_names[k], n)) known = 1;
        if (!known && g_guard_n < 16) { for (int i = 0; i < 24; i++) g_guard_names[g_guard_n][i] = n[i]; g_guard_n++; }
    }
    if (eq(n, "Rolling")) { g_pending |= PEV_ROLL; g_ev[0]++; }
    else if (starts(n, "CutHammer")) { g_pending |= PEV_HAMMER; g_ev[2]++; }
    else if (starts(n, "BowShoot") || starts(n, "SlingShoot") || starts(n, "SlingshotShoot")) { g_pending |= PEV_SHOT; g_ev[4]++; }
    else if (starts(n, "BoomerangThrow") || eq(n, "BoomerangSt")) { g_pending |= PEV_BOOMERANG; g_ev[2]++; }
    else if (eq(n, "SideStep")) { g_pending |= PEV_SIDESTEP; g_ev[0]++; }
    else if (starts(n, "BackFlip")) { g_pending |= PEV_BACKFLIP; g_ev[0]++; }
    else if (eq(n, "Jump")) { g_pending |= PEV_JUMP; g_ev[1]++; }
    else if (starts(n, "Guard") && !has_word(n, "Fail") && (has_hit(n) || has_word(n, "Break") || has_word(n, "Reaction") || has_word(n, "Repel"))) guard_hit();
    else if (starts(n, "Cut") || starts(n, "ChargeAttack") || starts(n, "JumpCut") || starts(n, "Stab"))
        { g_pending |= PEV_ATTACK; g_ev[2]++; }
}

/* ---- combat = a hostile enemy close to the player ----
 * Every physics actor passes through player_track each frame. An actor whose Life resource is named
 * "Enemy_*" (friendly shopkeepers excluded), still alive, within ENEMY_RADIUS of the player marks combat;
 * the state then lasts 5 seconds after the last such enemy (grace, like the original design). */
#define ENEMY_RADIUS 10.0f
u32 g_enemy_dbg[4] = {0x10000000u};        /* enemies seen, close ones, ... */
float g_enemy_min2 = 1.0e8f;               /* squared distance of the nearest alive enemy seen (diagnostic) */
/* Awareness: an enemy counts as "fixed on the player" when it is within ENEMY_RADIUS and either already on top of him
 * (<= ENEMY_TOUCH) or has been getting closer during the last ENEMY_CHASE_FRAMES. Per-enemy memory (by actor
 * pointer) compares the squared distance over a short window. */
#define ENEMY_TOUCH 3.0f
#define ENEMY_CHASE_FRAMES 120u
#define ENEMY_WINDOW 6u
typedef struct { u64 actor; float d2; u32 stamp; u32 chase; } EnemyMem;
EnemyMem g_em[24];
u32 g_enemy_chasing;       /* diagnostic: enemies currently counted as aware (last evaluation) */

/* Enemy health multiplier (Life records: current at +0x1C10, max at +0x1C18, value s32 at +8, the same ones used for
 * the player). Each enemy is remembered with its Life max record, its vanilla max and the scaled max:
 *  - max == scaled: done (the current value is only kept at or under it);
 *  - max back at the vanilla value (the game reset it, e.g. a Keese settling down again): scaled again, and the
 *    current value only when it is not already above the vanilla max (it was scaled before);
 *  - anything else, or another Life record: a new enemy on a reused actor, scaled from scratch.
 * The table is a hash on the actor pointer (HP_N slots, HP_PROBE tried per actor); a new enemy takes an empty slot or
 * the one in its probe window not seen for the longest time: every live enemy passes here each frame, so a live one
 * is only dropped when far more enemies than any area loads share its window. (It used to be a 64-entry ring: past 64 enemies a scaled enemy was dropped, seen
 * again as new and multiplied again every time, which made swarms like Keese much tougher than asked.) */
#define HP_N 1024
#define HP_PROBE 32
typedef struct { u64 actor, mrec; int m0, smax; u32 seen; } EnemyHp;
static EnemyHp g_hp[HP_N];
u32 g_hp_dbg[4];   /* diagnostic: scalings done, last cur before, last max before, last max after */
/* diagnostic (read by tools): the last 16 enemies scaled: actor, Life name, max before / after */
typedef struct { u64 actor; char name[24]; int m0, m1; } HpLog;
HpLog g_hp_log[16] = {{1}};
u32 g_hp_log_n = 0x10000000u;
static const char *g_scaling_name;
static void hp_log(u64 actor, int m0, int m1) {
    HpLog *e = &g_hp_log[(g_hp_log_n - 0x10000000u) % 16];
    g_hp_log_n++;
    e->actor = actor; e->m0 = m0; e->m1 = m1;
    for (int i = 0; i < 23; i++) e->name[i] = g_scaling_name ? g_scaling_name[i] : 0;
    e->name[23] = 0;
}
static int hp_mul(int v, unsigned pct) { int r = (int)(((long long)v * pct + 50) / 100); return r < 1 ? 1 : r; }
static void enemy_scale(u64 actor, u64 life) {
    unsigned pct = g_shbcfg.enemy_hp_pct;
    if (pct == 100 || pct == 0) return;
    u64 rec = rd(life, 0x1C10), mrec = rd(life, 0x1C18);
    if (!rec || !mrec) return;
    volatile int *cur = (volatile int *)(rec + 8), *mx = (volatile int *)(mrec + 8);
    int m = *mx, c = *cur;
    if (m <= 0 || c <= 0) return;
    u32 now = hud_frame_count() | 1u;
    u32 h = (u32)(((actor >> 4) * 0x9E3779B97F4A7C15ull) >> 54);   /* 10 bits: HP_N slots */
    EnemyHp *e = 0, *old = &g_hp[h];
    for (u32 k = 0; k < HP_PROBE; k++) {
        EnemyHp *t = &g_hp[(h + k) & (HP_N - 1)];
        if (t->actor == actor) { e = t; break; }
        if (t->seen < old->seen) old = t;          /* (an empty slot has seen 0: taken first) */
        if (!t->actor) break;                      /* the actor would be in this slot or before it */
    }
    if (e && e->mrec == mrec) {
        e->seen = now;
        if (m == e->smax) {                              /* already scaled */
            if (c > m) *cur = m;
            return;
        }
        if (m == e->m0) {                                /* the game set the vanilla max again */
            int nc = c <= e->m0 ? hp_mul(c, pct) : c;    /* (a current value above it was scaled before) */
            if (nc > e->smax) nc = e->smax;
            *mx = e->smax;
            *cur = nc;
            g_hp_dbg[0]++;
            hp_log(actor, m, e->smax);
            return;
        }
    }
    if (!e) e = old;                                     /* a new enemy (or the slot of the longest unseen one) */
    int nm = hp_mul(m, pct), nc = hp_mul(c, pct);
    if (nc > nm) nc = nm;
    g_hp_dbg[0]++; g_hp_dbg[1] = (u32)c; g_hp_dbg[2] = (u32)m; g_hp_dbg[3] = (u32)nm;
    hp_log(actor, m, nm);
    *mx = nm;
    *cur = nc;
    e->actor = actor; e->mrec = mrec; e->m0 = m; e->smax = nm; e->seen = now;
}

static void enemy_check(u64 actor, u64 comp, u64 life, const char *n) {
    (void)comp;
    if (!(n[0] == 'E' && n[1] == 'n' && n[2] == 'e' && n[3] == 'm' && n[4] == 'y' && n[5] == '_')) return;
    if (starts(n, "Enemy_Scrub_Business")) return;          /* friendly shopkeeper */
    g_enemy_dbg[0]++;
    g_scaling_name = n;
    enemy_scale(actor, life);
    if (g_player_actor == 1) return;
    u64 rec = rd(life, 0x1C10);
    if (rec && *(volatile int *)(rec + 8) <= 0) return;      /* dead (ragdoll still updating) */
    const volatile float *e = (const volatile float *)(actor + 0x31C);
    const volatile float *l = (const volatile float *)(g_player_actor + 0x31C);
    float dx = e[0] - l[0], dy = e[1] - l[1], dz = e[2] - l[2];
    float d2 = dx * dx + dy * dy + dz * dz;
    if (d2 < g_enemy_min2) g_enemy_min2 = d2;
    if (d2 >= ENEMY_RADIUS * ENEMY_RADIUS) return;
    u32 f = hud_frame_count();
    EnemyMem *m = 0, *oldest = &g_em[0];
    for (int i = 0; i < 24; i++) {
        if (g_em[i].actor == actor) { m = &g_em[i]; break; }
        if (g_em[i].stamp < oldest->stamp) oldest = &g_em[i];
    }
    if (!m) { m = oldest; m->actor = actor; m->d2 = d2; m->stamp = f; m->chase = 0; }
    if (f - m->stamp >= ENEMY_WINDOW) {
        if (d2 < m->d2 - 0.4f) m->chase = f ? f : 1;   /* closer than a moment ago */
        m->d2 = d2;
        m->stamp = f;
    }
    int aware = d2 < ENEMY_TOUCH * ENEMY_TOUCH || (m->chase && f - m->chase < ENEMY_CHASE_FRAMES);
    if (aware) {
        g_enemy_dbg[1]++;
        g_combat_frame = f ? f : 1;
    }
}

void player_track(u64 actor) {
    if (!actor) return;
    u64 comp = rd(actor, 0x270);
    if (!comp) return;
    u64 life = rd(comp, 0x2B8);
    u64 pl = rd(comp, 0x358);
    if (!life) return;
    u64 res = rd(life, 0x10);
    if (!res) return;
    const char *n = (const char *)(res + 0x58);
    if (n[0] != 'P' || n[1] != 'l' || n[2] != 'a' || n[3] != 'y' || n[4] != 'e' || n[5] != 'r' || n[6]) {
        enemy_check(actor, comp, life, n);
        return;
    }
    if (!pl) return;
    g_player_actor = actor;
    g_player_table = comp;
    g_player_comp = pl;
    g_player_seen++;
    u32 nowf = hud_frame_count();
    if (nowf - g_active_frame > 3u) g_prev_life = -1;     /* not updated for a while (loading, menus): no comparison */
    g_active_frame = nowf;
    {   /* life: Life+0x1C10 -> record, value s32 at +8 (0 = dead) */
        u64 rec = rd(life, 0x1C10);
        if (rec) {
            volatile int *lp = (volatile int *)(rec + 8);
            int cur = *lp;
            unsigned pct = g_shbcfg.damage_taken_pct;
            if (pct > 100 && g_prev_life > 0 && cur > 0 && cur < g_prev_life) {   /* took damage: charge the extra share */
                int nl = cur - (int)(((long long)(g_prev_life - cur) * (pct - 100) + 50) / 100);
                if (nl < 1) nl = 1;                /* the extra share alone never kills: only the real hit can */
                *lp = nl; cur = nl;
                g_dmg_dbg++;
            }
            g_prev_life = cur;
            g_player_life = cur; g_alive = g_player_life > 0;
        }
        u64 mrec = rd(life, 0x1C18);
        if (mrec) g_player_maxlife = *(volatile int *)(mrec + 8);
    }

    const volatile char *s = (const volatile char *)(pl + 0x244);
    for (int i = 0; i < 23; i++) g_cur[i] = s[i];
    g_cur[23] = 0;
    {   /* second (upper-body) name: BowCharge / BowShoot, also used by the slingshot */
        const volatile char *s2 = (const volatile char *)(pl + 0x34C);
        for (int i = 0; i < 23; i++) g_cur2[i] = s2[i];
        g_cur2[23] = 0;
        if (!eq(g_cur2, g_prev2)) {
            for (int i = 0; i < 24; i++) g_prev2[i] = g_cur2[i];
            classify(g_cur2);
        }
    }
    if (!eq(g_cur, g_prev)) {
        for (int i = 0; i < 24; i++) g_prev[i] = g_cur[i];
        classify(g_cur);
        /* log first distinct names seen (diagnostic) */
        int known = 0;
        for (u32 k = 1; k < g_names_n + 1 && k < 16; k++) if (eq(g_names_seen[k], g_cur)) known = 1;
        if (!known && g_names_n < 15) {
            g_names_n++;
            for (int i = 0; i < 24; i++) g_names_seen[g_names_n][i] = g_cur[i];
        }
    }
}

u64 player_get(void) { return g_player_comp == 1 ? 0 : g_player_comp; }

/* pending action events since the last call */
u32 player_take_events(void) {
    u32 e = g_pending;
    g_pending = 0;
    return e;
}
/* current action-state name equals / starts with */
int player_state_is(const char *name) { return eq(g_cur, name); }
int player_state_starts(const char *prefix) { return starts(g_cur, prefix); }
const char *player_state_name(void) { return g_cur; }

/* 1 while the player exists, is alive and his update ran within the last frames (not in menus/pause) */
int player_active(void) {
    return g_alive && (hud_frame_count() - g_active_frame) <= 2;
}

/* Dead (death ragdoll / game over): all input must reach the game untouched. */
int player_dead(void) { return !g_alive; }

int player_max_life(void) { return g_player_maxlife; }

/* a hostile enemy was within ENEMY_RADIUS of the player in the last 5 seconds */
void player_mark_combat(void) { u32 f = hud_frame_count(); g_combat_frame = f ? f : 1; }

int player_in_combat(void) {
    return g_combat_frame != 0 && (hud_frame_count() - g_combat_frame) < shb_frames(300u);   /* 5 s, whatever the frame rate */
}
