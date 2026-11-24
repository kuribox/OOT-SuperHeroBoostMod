#ifndef SHB_CONFIG_H
#define SHB_CONFIG_H

/* Runtime configuration block. It lives in the module's .data (file-backed), so the companion app
 * can change it by patching bytes in the prebuilt subsdk9 (find the magic, write the fields) without
 * compiling anything. Layout is append-only: never reorder, bump `version` only on incompatible changes. */
typedef struct {
    unsigned char magic[8];        /* "SHBCFG01" */
    unsigned int version;          /* 1 */
    unsigned int size;             /* sizeof(ShbConfig) */
    unsigned char stamina_enabled; /* 0 = stamina system off (no costs, no bar, no locks) */
    unsigned char preset;          /* 0 hardcore, 1 combat only, 2 sprint only, 3 advanced */
    unsigned char cost_jump;       /* advanced: which actions cost stamina */
    unsigned char cost_attack;
    unsigned char cost_sprint;
    unsigned char cost_block;
    unsigned char cost_shoot;
    unsigned char cost_roll;
    unsigned char combat_only;     /* advanced: costs only while in combat */
    unsigned char bm_layout;       /* buttonMapping: where the face buttons sit on the pad (the recompilers' own
                                    * button remap): 0 Nintendo, 1 A/B swapped, 2 Xbox (A/B and X/Y swapped),
                                    * 3 Switch GameCube (A centre, B lower left, X upper right, Y above-left).
                                    * Was a reserved 0 byte, so older configurations read as Nintendo. */
    /* v1 additions (append-only; older apps leave the defaults) */
    unsigned short enemy_hp_pct;   /* enemy health, percent of vanilla (100 = vanilla, 150 = Hero Mode) */
    unsigned short damage_taken_pct; /* damage the player receives, percent of vanilla (200 = Hero Mode) */
    unsigned char hud_rupees;      /* always-visible rupee counter */
    unsigned char hud_clock;       /* always-visible clock */
    unsigned char clock_24h;       /* clock in 24 h format */
    unsigned char no_heart_drop;   /* informational (done through a romfs file) */
    unsigned short sprint_speed_pct; /* sprint (A held while moving) speed, percent of vanilla (100 = vanilla, default 125) */
    unsigned char songbook_enabled;  /* play a learned song by selecting it in the native songbook */
    unsigned char button_mapping;    /* buttonMapping: our own item buttons B / L / R / ZR on the HUD */
    unsigned char button_style;      /* buttonMapping buttons: 0 Switch, 1 Xbox, 2 PlayStation */
    unsigned char bm_simple;         /* buttonMapping simple layout: only the assignable buttons (no B / A / X / Y) */
    unsigned char bm_l_mode;         /* buttonMapping L: 0 assignable, 2 the game's item selector */
    unsigned char quick_boots;       /* hold D-pad down to pick the boots */
    unsigned char quick_mask;        /* hold D-pad up to pick a mask */
    unsigned char hud_collectibles; /* per-area Skulltula / heart-piece counter */
    signed short hud_x[4];          /* installer: x of the rupee, clock, skull and heart blocks (-32768 = off) */
    unsigned char stamina_bar;      /* 0 = ring around the player, 1 = segmented horizontal bar */
    unsigned char hud_flags;        /* 1 = use the layout fields below (new app); 0 = wide defaults */
    unsigned char hud_mode;         /* app: 0 wide, 1 simple (soft bar), 2 plain (bare row) */
    unsigned char hud_sx;           /* plate scale x, percent */
    unsigned char hud_sy;           /* plate scale y, percent */
    unsigned char hud_tscale;       /* text scale, percent */
    unsigned char hud_iscale;       /* icon scale, percent */
    unsigned char hud_plates;       /* 0 no plates, 1 all (wide), 2 one bar plate (simple) */
    unsigned char hud_map_hearts;   /* 1 = area heart counter on the Minus map */
    signed short hud_y;             /* row y (layout units) */
    signed short hud_ioff;          /* rupee icon x offset */
    signed short hud_tx;            /* text x offset */
    signed short hud_ioff2;         /* clock/collectible icon x offset (wide: same as hud_ioff) */
    unsigned char manual_shield;    /* ZL holds the guard, L toggles targeting, R3 cancels it */
    unsigned char target_r3;        /* swap ZL and R3: target with R3 (recommended with Toggle targeting) */
    unsigned char sprint_l3;        /* L3 (left stick click) presses A while held, so Link sprints */
    unsigned char auto_target;      /* when the target dies, press target again to lock on to the next enemy */
    unsigned char switch_target;    /* the other shoulder button switches to the next target while targeting */
    unsigned char lang;             /* language of the mod's own HUD labels: the language the game is played in,
                                     * or the app's language when it cannot be read:
                                     * 0 English, 1 Spanish, 2 Portuguese, 3 French, 4 German, 5 Italian */
    unsigned char no_magic_drop;    /* informational (done through a romfs file): dropped magic jars become rupees */
    unsigned char hide_minimap;     /* hide the corner minimap while playing (the map screen still works) */
    unsigned char sprint_l3_toggle; /* sprint_l3: 0 sprint while L3 is held, 1 L3 turns the sprint on / off (it also
                                     * stops when stamina runs out or A is pressed) */
    /* 0.9.0: button remap (core/remap.h). The struct grows from 0x50 to 0x5A; older apps leave these 0 (classic). */
    unsigned char bm_remap;         /* 0 classic layout, 1 use bm_role */
    unsigned char bm_role[10];      /* role of each physical button, PB_ order: L X R Y A ZR B ZL L3 R3 (ROLE_*) */
    /* HUD scale, percent (0 = 100). They sit in what was the struct's tail padding, so sizeof stays 0x5C and an
     * older app leaves them 0. The groups keep their screen corner: controls top right, counters bottom left. */
    unsigned char hud_ctrl_scale;   /* the button cluster (N_ShbActions_00) */
    unsigned char hud_count_scale;  /* the counter row: rupees, clock, collectibles (N_ShbCounters_00) */
    /* D-pad right, held (core/remap.h PB_DR): the role of its long press. A short press stays the game's own (Navi).
     * 0 = none. Appended after the HUD scales: an older app leaves it 0. */
    unsigned char bm_role_dr;
    /* Held face buttons (core/remap.h): the role of the long press of A, B, X and Y, in that order. 0 = none. The
     * short press keeps its role in bm_role. Appended after bm_role_dr: an older app leaves them 0. */
    unsigned char bm_hold[4];
    /* Custom item HUD (Nov 24): 1 = our own item buttons on the HUD (B / L / R / ZR) and items 1, 2 and 3
     * assignable to remapped buttons; 0 = mapping only: the player's item HUD and the game's item logic stay
     * untouched and no button may hold an item role. Appended after bm_hold: an older app leaves it 0. */
    unsigned char hud_custom_items;
    /* Long press of L and R (bm_hold covers A, B, X, Y): the role of each, 0 = none. Appended after
     * hud_custom_items: an older app leaves them 0. */
    unsigned char bm_hold_lr[2];
    /* Per held button, 1 = the short press WAITS until it is known whether the button is held (a small delay on
     * its own role, but a hold never also fires it); 0 = the short role fires at once and the hold role comes on
     * top. Bit per button: 0 D-pad right, 1 A, 2 B, 3 X, 4 Y, 5 L, 6 R. */
    unsigned char bm_hold_wait;
} ShbConfig;

extern volatile ShbConfig g_shbcfg;

#endif
