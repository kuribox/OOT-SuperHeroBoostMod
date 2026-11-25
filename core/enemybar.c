#include "enemybar.h"
#include "config.h"

/* Enemy / boss health bars: the per-frame table of live hostile actors and the two name tables behind it.
 *
 * The collection rides on the actor update core/player.c already reads (hook main+0x2C215E0): every physics
 * actor passes through player_track(), and enemy_check() hands the hostile ones here with their Life component.
 * Life gives the values the module already uses for the enemy-health multiplier: current at Life+0x1C10, max at
 * Life+0x1C18, both a record whose s32 value sits at +8. The world position is the actor's (actor+0x31C, the
 * same f32[3] the stamina anchor projects).
 *
 * Two tables map a Life resource name to what the HUD needs: the boss keys (the name that gets the big on-screen
 * bar) and the head heights. Both are arrays of structs with inline char[] names - never pointer tables, the
 * module has no relocations (a pointer table truncates to 32 bits on Rekarina). */

typedef unsigned long long u64;
typedef unsigned int u32;

extern volatile ShbConfig g_shbcfg;
u32 hud_frame_count(void);

ShbEnemyRow g_eb[SHB_EB_MAX];
u32 g_eb_n;
u32 g_eb_frame;

ShbEbLog g_eb_log[16];
u32 g_eb_log_n = 0x10000000u;

static u64 rd(u64 p, u32 off) { return p ? *(volatile u64 *)(p + off) : 0; }

static int eq(const char *a, const char *b) {
    u32 i = 0;
    for (; a[i]; i++) if (a[i] != b[i]) return 0;
    return b[i] == 0;
}
/* does `s` start with `p`? */
static int pfx(const char *s, const char *p) {
    u32 i = 0;
    for (; p[i]; i++) if (s[i] != p[i]) return 0;
    return 1;
}

/* ---- bosses with a global bar ----------------------------------------------------------------
 * The eight dungeon bosses, plus Ganon's second half (its own actor) and Ganondorf. Everything else -
 * a boss's sub-actors (Gohma_Egg, BongoBongo_LeftHand, Morpha_Tentacle, Barinade_Jellyfish,
 * Dodongo_SpawnBabyDodongo), Twinrova's witches Kotake and Koume, and the minibosses (MiddleBoss_*,
 * IronKnuckle_*, DeadHand, Guay_Boss) - is a normal enemy: a bar over its head, no global bar.
 * Kotake and Koume get their head bars and, once the two merge, the merged actor carries the Twinrova bar. */
typedef struct {
    char key[40];                    /* Life resource name, exactly as the pack names it */
    char nm[SHB_EB_LANG][26];        /* display name, same order as ShbConfig.lang (accent-free on purpose:
                                      * the game's Normal font is used, and a wrong accent byte draws a box) */
} BossRec;

static const BossRec g_boss[] = {
    {"Enemy_DungeonBoss_Gohma", {{"Gohma", "Gohma", "Gohma", "Gohma", "Gohma", "Gohma"}}},
    {"Enemy_DungeonBoss_Dodongo", {{"King Dodongo", "Rey Dodongo", "Rei Dodongo", "Roi Dodongo",
                                    "Koenig Dodongo", "Re Dodongo"}}},
    {"Enemy_DungeonBoss_Barinade", {{"Barinade", "Barinade", "Barinade", "Barinade", "Barinade", "Barinade"}}},
    {"Enemy_DungeonBoss_PhantomGanon", {{"Phantom Ganon", "Ganon Fantasma", "Ganon Fantasma", "Ganon Fantome",
                                         "Phantom-Ganon", "Ganon Fantasma"}}},
    {"Enemy_DungeonBoss_Volvagia", {{"Volvagia", "Volvagia", "Volvagia", "Volvagia", "Volvagia", "Volvagia"}}},
    {"Enemy_DungeonBoss_Morpha", {{"Morpha", "Morpha", "Morpha", "Morpha", "Morpha", "Morpha"}}},
    {"Enemy_DungeonBoss_BongoBongo", {{"Bongo Bongo", "Bongo Bongo", "Bongo Bongo", "Bongo Bongo",
                                       "Bongo Bongo", "Bongo Bongo"}}},
    {"Enemy_DungeonBoss_Twinrova", {{"Twinrova", "Twinrova", "Twinrova", "Twinrova", "Twinrova", "Twinrova"}}},
    {"Enemy_DungeonBoss_Ganondorf", {{"Ganondorf", "Ganondorf", "Ganondorf", "Ganondorf", "Ganondorf", "Ganondorf"}}},
    {"Enemy_DungeonBoss_Ganon_SecondHalfBattle", {{"Ganon", "Ganon", "Ganon", "Ganon", "Ganon", "Ganon"}}},
    {"Enemy_DungeonBoss_Ganon", {{"Ganon", "Ganon", "Ganon", "Ganon", "Ganon", "Ganon"}}},
};
#define BOSS_N ((int)(sizeof(g_boss) / sizeof(g_boss[0])))

int shb_boss_index(const char *name) {
    for (int i = 0; i < BOSS_N; i++) if (eq(name, g_boss[i].key)) return i;
    return -1;
}
int shb_bar_skip(const char *name) {
    if (!pfx(name, "Enemy_DungeonBoss_")) return 0;
    if (eq(name, "Enemy_DungeonBoss_Kotake") || eq(name, "Enemy_DungeonBoss_Koume")) return 0;
    return shb_boss_index(name) < 0;
}
const char *shb_boss_key(int idx) { return idx >= 0 && idx < BOSS_N ? g_boss[idx].key : ""; }
const char *shb_boss_name(int idx, u32 lang) {
    if (idx < 0 || idx >= BOSS_N) return "";
    return g_boss[idx].nm[lang < SHB_EB_LANG ? lang : 0];
}

/* ---- head heights ------------------------------------------------------------------------------
 * Metres above the actor position (actor+0x31C, the model's origin) where the bar sits. Longest matching
 * prefix wins, so a variant name (Enemy_Keese_Fire, Enemy_Stalfos_Weak) follows its base entry and a specific
 * entry can override one. Sizes are the model's rough height; they only decide where the bar floats, and
 * g_eb_tune[0] in core/hud.c scales the whole table without a rebuild. */
typedef struct { char key[34]; float h; } HeightRec;

static const HeightRec g_hgt[] = {
    {"Enemy_Keese", 0.55f}, {"Enemy_Dekubaba", 1.55f}, {"Enemy_Moblin", 2.60f},
    {"Enemy_Stalfos_Weak", 1.70f}, {"Enemy_Stalfos", 2.25f}, {"Enemy_Lizalfos", 2.00f},
    {"Enemy_Dinolfos", 2.00f}, {"Enemy_Wolfos", 1.90f}, {"Enemy_Octorok", 1.00f},
    {"Enemy_Peahat_Larva", 0.55f}, {"Enemy_Peahat", 2.00f}, {"Enemy_Guay", 0.75f},
    {"Enemy_Poe_Big", 2.20f}, {"Enemy_Poe", 1.70f}, {"Enemy_ReDead", 2.10f},
    {"Enemy_Gibdo", 2.10f}, {"Enemy_Floormaster", 1.60f}, {"Enemy_Wallmaster", 1.60f},
    {"Enemy_Tektite", 0.85f}, {"Enemy_Skullwalltula", 1.20f}, {"Enemy_Skulltula", 1.20f},
    {"Enemy_Dodongo", 1.80f}, {"Enemy_Bubble", 0.90f}, {"Enemy_LikeLike", 1.80f},
    {"Enemy_Bari", 1.00f}, {"Enemy_Biri", 0.80f}, {"Enemy_Shabom", 0.75f},
    {"Enemy_Freezard", 1.60f}, {"Enemy_TorchSlug", 1.10f}, {"Enemy_Beamos", 2.00f},
    {"Enemy_Anubis", 1.90f}, {"Enemy_Amos", 2.00f}, {"Enemy_IronKnuckle", 2.60f},
    {"Enemy_DeadHand", 1.70f}, {"Enemy_FlareDancer", 1.80f}, {"Enemy_MiddleBoss_DarkLink", 1.85f},
    {"Enemy_MiddleBoss_GerudoThief", 1.85f}, {"Enemy_MiddleBoss_Lizalfos", 2.00f},
    {"Enemy_MiddleBoss_BigOcto", 2.00f}, {"Enemy_MiddleBoss_Poe_Meg", 1.70f},
    {"Enemy_MiddleBoss_FlareDancer", 1.80f}, {"Enemy_Scrub", 0.95f},
    {"Enemy_Stalchild", 1.30f}, {"Enemy_SkullKid", 1.60f}, {"Enemy_Spike", 1.70f},
    {"Enemy_Tailpasaran", 0.80f}, {"Enemy_Tentacle", 1.00f}, {"Enemy_ShellBlade", 0.80f},
    {"Enemy_FlyingPot", 1.20f}, {"Enemy_CrazyFloorTile", 0.60f}, {"Enemy_Leever", 0.70f},
    {"Enemy_Stinger", 0.70f}, {"Enemy_TrickyDoor", 1.60f},
    /* the head bars of the bosses that also get a global bar are hidden; their heights matter only for the
     * witches (which never get a global bar) */
    {"Enemy_DungeonBoss_Kotake", 1.90f}, {"Enemy_DungeonBoss_Koume", 1.90f},
    {"Enemy_DungeonBoss_Gohma", 2.60f}, {"Enemy_DungeonBoss_Dodongo", 2.20f},
    {"Enemy_DungeonBoss_Barinade", 2.40f}, {"Enemy_DungeonBoss_PhantomGanon", 2.50f},
    {"Enemy_DungeonBoss_Volvagia", 2.80f}, {"Enemy_DungeonBoss_Morpha", 1.60f},
    {"Enemy_DungeonBoss_BongoBongo", 2.80f}, {"Enemy_DungeonBoss_Twinrova", 2.20f},
    {"Enemy_DungeonBoss_Ganondorf", 2.50f}, {"Enemy_DungeonBoss_Ganon", 3.50f},
};
#define HGT_N ((u32)(sizeof(g_hgt) / sizeof(g_hgt[0])))
#define HGT_DEFAULT 1.50f

float shb_enemy_height(const char *name) {
    u32 best = 0, len = 0;
    for (u32 i = 0; i < HGT_N; i++) {
        if (!pfx(name, g_hgt[i].key)) continue;
        u32 n = 0;
        while (g_hgt[i].key[n]) n++;
        if (n >= len) { len = n; best = i; }
    }
    return len ? g_hgt[best].h : HGT_DEFAULT;
}

/* ---- collection ---------------------------------------------------------------------------------
 * The table persists across frames: the engine does not hand every actor to the physics update on every
 * frame (measured: 6..10 of ~12 live enemies per frame, and a different subset each time), so a table that
 * started over each frame made an enemy's bar vanish on the frames its actor did not update - the player saw
 * it as blinking while the enemy moved. A row stays valid for EB_ROW_GRACE frames after its last update and
 * the HUD draws any row that fresh; the position is at most ~0.1 s old, which is invisible. */
static void eb_log(const char *name, int boss) {
    for (u32 i = 0; i < 16; i++)
        if (eq(g_eb_log[i].name, name)) return;      /* already seen (an empty entry never matches) */
    ShbEbLog *e = &g_eb_log[(g_eb_log_n - 0x10000000u) % 16];
    g_eb_log_n++;
    u32 i = 0;
    for (; name[i] && i < 39; i++) e->name[i] = name[i];
    e->name[i] = 0;
    e->boss = boss;
}

void shb_enemybar_note(u64 actor, u64 life, const char *name) {
    if (!g_shbcfg.enemy_bars && !g_shbcfg.boss_bar) return;
    if (!actor || !life || !name) return;
    u64 rc = rd(life, 0x1C10), rm = rd(life, 0x1C18);
    if (!rc || !rm) return;
    int cur = *(volatile int *)(rc + 8), mx = *(volatile int *)(rm + 8);
    if (cur <= 0 || mx <= 0) return;                       /* dead (the ragdoll keeps updating) or not spawned */
    u32 f = hud_frame_count() | 1u;
    g_eb_frame = f;                     /* diagnostic: the frame of the last update */
    ShbEnemyRow *e = 0;
    for (u32 i = 0; i < g_eb_n; i++)
        if (g_eb[i].actor == actor) { e = &g_eb[i]; break; }
    int fresh = 0;
    if (!e) {
        if (g_eb_n < SHB_EB_MAX) e = &g_eb[g_eb_n++];
        else {                                             /* full: take the row seen longest ago */
            e = &g_eb[0];
            for (u32 i = 1; i < SHB_EB_MAX; i++) if ((int)(g_eb[i].seen - e->seen) < 0) e = &g_eb[i];
        }
        fresh = 1;
    }
    e->actor = actor; e->life = life; e->name = name;
    const volatile float *p = (const volatile float *)(actor + 0x31C);
    e->x = p[0]; e->y = p[1]; e->z = p[2];
    e->cur = cur; e->max = mx;
    /* The actor carries its own height at +0x3C8 (measured live: Link 1.578, Enemy_Guay 0.268,
     * Enemy_Stalchild_Big 2.183; four Guays agree, so it is the body, not a stray float). That beats any
     * per-type table and follows scaled variants; the table stays as the fallback for the actors where the
     * field holds something implausible. */
    {
        float hh = *(const volatile float *)(actor + 0x3C8);
        if (!(hh > 0.05f && hh < 30.0f)) hh = shb_enemy_height(name);
        e->h = hh;
    }
    if (fresh || e->name != name) {   /* first sight of this row, or the actor now holds another enemy */
        e->boss = shb_boss_index(name);
        eb_log(name, e->boss);
    }
    e->seen = f;
}
