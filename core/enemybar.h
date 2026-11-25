#ifndef SHB_ENEMYBAR_H
#define SHB_ENEMYBAR_H

/* Enemy / boss health bars (feat/enemy-hp-bars).
 *
 * core/enemybar.c collects, once per frame, every live hostile actor the physics update hands us
 * (core/player.c player_track) with its Life values and world position; core/hud.c projects them and draws
 * the bars with the HUD picture panes. Nothing here calls into the game: the table is plain data. */

#define SHB_EB_MAX 24      /* enemies remembered per frame (the HUD draws the nearest few) */
#define SHB_EB_LANG 6      /* same order as ShbConfig.lang: en es pt fr de it */
#define SHB_EB_GRACE 10    /* frames a row stays usable after its last update (the actor update skips actors) */

typedef struct {
    unsigned long long actor;   /* physics actor */
    unsigned long long life;    /* Life component */
    const char *name;           /* Life resource name, e.g. "Enemy_Keese" (game memory, read only) */
    float x, y, z;              /* actor position (actor+0x31C): the feet / model origin */
    int cur, max;               /* Life current / max (+8 of the two records) */
    int boss;                   /* index in the boss table, -1 = a normal enemy (head bar only) */
    float h;                    /* head height above the actor position, metres */
    unsigned int seen;          /* frame this row was written */
} ShbEnemyRow;

extern ShbEnemyRow g_eb[SHB_EB_MAX];
extern unsigned int g_eb_n;      /* rows valid this frame */
extern unsigned int g_eb_frame;  /* frame the table belongs to */

/* called from player_track() for every non-player actor */
void shb_enemybar_note(unsigned long long actor, unsigned long long life, const char *name);

/* -1 = not a boss (no global bar); >= 0 = index in the boss table */
int shb_boss_index(const char *name);
/* 0 = the name is unknown, read it from the caller's own boss row */
const char *shb_boss_key(int idx);
const char *shb_boss_name(int idx, unsigned int lang);

/* 1 for a boss's sub-actor (Gohma_Egg, BongoBongo_LeftHand, Morpha_Tentacle, Barinade_Jellyfish,
 * Dodongo_SpawnBabyDodongo...): it is part of the boss fight, so it gets neither a head bar nor a global one.
 * Twinrova's witches are NOT sub-actors: each one carries a head bar until the two merge. */
int shb_bar_skip(const char *name);

/* head height of an actor, metres above its position (longest matching prefix of a table) */
float shb_enemy_height(const char *name);

/* diagnostics: every distinct Life name seen with a bar configured (name + boss index) */
typedef struct { char name[40]; int boss; } ShbEbLog;
extern ShbEbLog g_eb_log[16];
extern unsigned int g_eb_log_n;

#endif
