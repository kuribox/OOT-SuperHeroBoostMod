#include "config.h"

/* Defaults = the original "hardcore" behaviour. Must be non-zero data so it stays in .data. */
volatile ShbConfig g_shbcfg = {
    {'S', 'H', 'B', 'C', 'F', 'G', '0', '1'}, 1, sizeof(ShbConfig),
    1, 0,
    1, 1, 1, 1, 1, 1, 0, 1,
    100, 100, 0, 0, 0, 0,
    125, 1,
    0, 0, 0, 0,
    {0},
    /* The beta health bars are off unless the app turns them on (struct 101 / 102). */
    .enemy_bars = 0,
    .boss_bar = 0,
};
