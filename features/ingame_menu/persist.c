#include "menu.h"
#include "../../core/config.h"

#ifndef SHB_DBG_DUMP
#define SHB_DBG_DUMP 0
#endif

/* Saving the menu's settings: an explicit "Save settings" row writes save:/shbmenu.bin; the settings are loaded
 * back, once per run, the first time the game opens a slot's progress.sav to read it - the earliest moment the
 * save volume is mounted, and before the HUD is built, so the saved layout is the one that gets built.
 *
 * File: "SHBM", version, n, 0, then the configuration block as the configurator installed it (base) and as the
 * player left it (user), n bytes each (the block without its 16-byte header). On load the user part is applied
 * only when the base part equals the module's own installed block: when the configurator installed something
 * else since, its settings win. So the saved settings stay until the configurator or this menu changes them.
 *
 * Writing is the delicate part: the game commits its own files to the save volume right after it writes a slot's
 * progress.sav, and a write of ours that lands in that window races it. The game's write-open is seen here, and
 * our write waits until it is long past. */

typedef unsigned char u8;

typedef struct { void *h; } FsFile;
typedef struct { int flags; } FsWriteOpt;
extern u32 fs_create_file(const char *path, long long size);
extern u32 fs_open_file(FsFile *out, const char *path, int mode);
extern u32 fs_read_file(unsigned long long *out, FsFile f, long long off, void *buf, unsigned long long size);
extern u32 fs_write_file(FsFile f, long long off, const void *buf, unsigned long long size, const FsWriteOpt *opt);
extern u32 fs_set_file_size(FsFile f, long long size);
extern void fs_close_file(FsFile f);

#define BLK_MAX 200
#define HDR 16
#define SAFE_READS 360u          /* input reads (about 6 s) after the game's own save before ours is written */
#define MAX_LOAD_TRIES 6u

static u8 g_base[BLK_MAX];       /* the block as installed, before any change */
static u8 g_saved[BLK_MAX];      /* the block as last loaded or saved: what a restart would give */
static u32 g_n;                  /* its length (the block without the 16-byte header) */
static u8 g_ready, g_loaded, g_req, g_failed;
static u32 g_load_tries, g_calls, g_last_game_save;
static u8 g_buf[HDR + 2 * BLK_MAX];

#define IM_MAGIC3 0x50455253u
u32 g_persist_dbg[6] = {IM_MAGIC3, IM_MAGIC3, IM_MAGIC3, IM_MAGIC3, IM_MAGIC3, IM_MAGIC3};
enum { P_WRITES, P_WRITE_FAIL, P_LOADS, P_SKIPPED, P_MISSES, P_LAST_OPEN };

/* Takes the installed block, once, before the menu can change anything. */
void im_persist_init(void) {
    if (g_ready) return;
    g_ready = 1;
    const u32 size = g_shbcfg.size;
    if (size <= HDR || size - HDR > BLK_MAX) return;
    g_n = size - HDR;
    const volatile u8 *src = (const volatile u8 *)&g_shbcfg;
    for (u32 i = 0; i < g_n; i++) g_base[i] = g_saved[i] = src[HDR + i];
}

static int differs(const u8 *ref) {
    const volatile u8 *src = (const volatile u8 *)&g_shbcfg;
    for (u32 i = 0; i < g_n; i++) if (src[HDR + i] != ref[i]) return 1;
    return 0;
}

int im_persist_state(void) {
    if (g_req) return 2;
    if (g_failed) return 3;
    return differs(g_saved) ? 1 : 0;
}

void im_persist_request(void) { if (g_n) { g_req = 1; g_failed = 0; } }

#define FILE_PATH "save:/shbmenu.bin"

static void file_write(void) {
    static FsFile f;
    static FsWriteOpt opt;
    g_buf[0] = 'S'; g_buf[1] = 'H'; g_buf[2] = 'B'; g_buf[3] = 'M';
    g_buf[4] = 1; g_buf[5] = g_buf[6] = g_buf[7] = 0;
    g_buf[8] = (u8)g_n; g_buf[9] = (u8)(g_n >> 8); g_buf[10] = g_buf[11] = 0;
    g_buf[12] = g_buf[13] = g_buf[14] = g_buf[15] = 0;
    const volatile u8 *src = (const volatile u8 *)&g_shbcfg;
    for (u32 i = 0; i < g_n; i++) { g_buf[HDR + i] = g_base[i]; g_buf[HDR + g_n + i] = src[HDR + i]; }
    const u32 total = HDR + 2 * g_n;
    char path[24];
    for (u32 i = 0; ; i++) { path[i] = FILE_PATH[i]; if (!FILE_PATH[i]) break; }
    f = (FsFile){0};
    fs_create_file(path, total);                         /* fails when it is already there */
    u32 r = fs_open_file(&f, path, 2 | 4);
    g_persist_dbg[P_LAST_OPEN] = IM_MAGIC3 + r;
    if (r) { g_persist_dbg[P_WRITE_FAIL]++; g_failed = 1; return; }
    opt = (FsWriteOpt){1};                               /* flush */
    fs_set_file_size(f, total);
    r = fs_write_file(f, 0, g_buf, total, &opt);
    fs_close_file(f);
    if (r) { g_persist_dbg[P_WRITE_FAIL]++; g_failed = 1; return; }
    g_persist_dbg[P_WRITES]++;
    for (u32 i = 0; i < g_n; i++) g_saved[i] = src[HDR + i];
}

static void file_read(void) {
    static FsFile f;
    static unsigned long long got;
    char path[24];
    for (u32 i = 0; ; i++) { path[i] = FILE_PATH[i]; if (!FILE_PATH[i]) break; }
    f = (FsFile){0};
    got = 0;
    g_load_tries++;
    u32 r = fs_open_file(&f, path, 1);
    g_persist_dbg[P_LAST_OPEN] = IM_MAGIC3 + r;
    if (r) { g_persist_dbg[P_MISSES]++; return; }        /* none yet (or not mounted yet): the installed settings stay */
    r = fs_read_file(&got, f, 0, g_buf, sizeof g_buf);
    fs_close_file(f);
    g_loaded = 1;
    const u32 n = (u32)g_buf[8] | ((u32)g_buf[9] << 8);
    if (r || got < HDR + 2 * n || g_buf[0] != 'S' || g_buf[1] != 'H' || g_buf[2] != 'B' || g_buf[3] != 'M' ||
        g_buf[4] != 1 || n != g_n) { g_persist_dbg[P_MISSES]++; return; }
    for (u32 i = 0; i < n; i++)
        if (g_buf[HDR + i] != g_base[i]) { g_persist_dbg[P_SKIPPED]++; return; }   /* installed again since: the installer wins */
    volatile u8 *dst = (volatile u8 *)&g_shbcfg;
    for (u32 i = 0; i < n; i++) { dst[HDR + i] = g_buf[HDR + n + i]; g_saved[i] = g_buf[HDR + n + i]; }
    g_persist_dbg[P_LOADS]++;
}

/* Hook id 18: nn::fs::OpenFile(handle, path, mode), for every file the game opens. */
void ingame_menu_on_open(const char *path, int mode) {
    const u64 a = (u64)path;
    if (a < 0x10000ull || a >= 0x60000000000ull) return;
    if (path[0] != 's' || path[1] != 'a' || path[2] != 'v' || path[3] != 'e' || path[4] != ':') return;
    static const char PRE[] = "save:/slot_";
    u32 i = 0;
    for (; PRE[i]; i++) if (path[i] != PRE[i]) return;
    im_persist_init();
    if (mode & 2) g_last_game_save = g_calls;            /* the game is saving a slot: its commit follows */
    else if (!g_loaded && g_n && g_load_tries < MAX_LOAD_TRIES) file_read();
}

#if SHB_DBG_DUMP
/* Test aid (debug builds only): the debug server writes IM_MAGIC3 + 1 / + 2 here to make the module do what the
 * game's own opens of a slot's progress.sav would (+1: a write, +2: a first read), so the paths can be proven
 * without playing through a save. */
u32 g_persist_test = IM_MAGIC3;
static void test_poll(void) {
    const u32 t = g_persist_test - IM_MAGIC3;
    if (!t) return;
    g_persist_test = IM_MAGIC3;
    if (t == 1) g_last_game_save = g_calls;
    else if (t == 2) { g_loaded = 0; g_load_tries = 0; file_read(); }
}
#else
static void test_poll(void) {}
#endif

/* Every gameplay input read: counts time and writes the file once it is safe. */
void im_persist_poll(void) {
    g_calls++;
    test_poll();
    if (g_req && g_n && g_calls - g_last_game_save > SAFE_READS) {
        g_req = 0;
        file_write();
    }
}
