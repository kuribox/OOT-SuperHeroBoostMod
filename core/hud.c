/* Unified HUD (F4). Our own parts, drawn by the engine's UI.
 *
 * How new parts get into the engine (all done by our code):
 *  1. The game builds each UI resource through a function at main+0x43A6F8
 *     (code hook 2): x0 = handle, x1 = build config whose first pointer is the
 *     UIL resource metadata. We hand it a private copy of the PlayerStatus_00
 *     metadata with extra parts, each described by cloning a stock part
 *     descriptor (the gauge for the stamina bar, the item button for the slots).
 *     The metadata is replaced before the engine allocates the pane arrays, so
 *     the parts are registered normally and find_part() resolves them.
 *  2. The stamina part is an instance of the gauge asset; when the engine loads
 *     that instance we substitute a private copy of the gauge file with a flat
 *     white fill tinted yellow. Slot parts use the stock item-button asset.
 *  3. Each scheduler frame (code hook 0) the parts are bound lazily and their
 *     animations are driven: gauge GaugeRatio/GaugeMax/TraceRatio, item button
 *     ItemPattern (icon) and StockNum (count).
 * Features never draw: they publish values with hud_set_stamina()/hud_set_slot().
 * The magic gauge is not used and is left untouched.
 *
 * NOTE: the NSO has no relocations, so no static table may hold addresses;
 * names are returned by code (part_name) instead.
 */
#include "dispatch.h"
#include "hud_names.h"
#include "config.h"
#include "remap.h"
#include "gd_index.h"
#include "clock.h"

typedef unsigned long long u64;
typedef unsigned char u8;

extern char MAIN_SCHED_VT[];
extern void *find_part(void *node, const char *name);
extern void *find_anim(void *part, const char *name);
extern void set_frame(void *anim, float frame);
extern void tex_replace(void *replacer, const char **name, int flag);   /* main+0xA73920 */
extern void icon_widget_init(void *widget, void *heap);                 /* main+0x33B7DA0 */
extern void item_actor_name(void *mgr, const char **out, u64 item);     /* main+0x592B20 */
extern char ITEM_MGR_SLOT[], ITEM_NAME_DEFAULT[], ITEM_SET_SLOT[];
extern void *memcpy(void *d, const void *s, unsigned long n);
extern long set_text(void *pane, const void *ref, int zero);   /* stock text-pane setter (main+0x439520) */
extern char GAMEDATA_MGR[];                                      /* main+0x4E842A8: GameData manager pointer slot */
extern u32 read_bool(void *, u8 *, void *, u32);                 /* main+0x179F70: GameData struct Bool member */

#if __has_include("../build/collectibles_data.h")
#include "../build/collectibles_data.h"
#ifndef COLLECT_FLAG_HASHES
#error "build/collectibles_data.h holds Bool indices from an older generator: run tools/make_collectibles.py"
#endif
#else
#define COLLECT_STAGES 0
#define COLLECT_FLAGS 0
static const unsigned char g_collect_table[1] = {0};
static const unsigned int g_collect_flags[1] = {0};
static const unsigned long long g_collect_hash[1] = {0};
#endif
extern char WORLD_MGR[];                                         /* main+0x4E7EAB0: world manager pointer slot */
int player_active(void);

#define MARK 0x1000000000000000ull
/* Diagnostics (in .data, read by tools/live_check.py):
 * 0 mp controller, 1 frames, 2 bar, 3 ratio, 4 max, 5 trace,
 * 6 root clones, 7 gauge clones, 8 prepare calls, 9 last pane, 10 errors, 11 slots bound */
u64 g_hud_dbg[12] = {MARK, MARK, MARK, MARK, MARK, MARK, MARK, MARK, MARK, MARK, MARK, MARK};
/* Diagnostics for the collectible counter (read by tools/live_check.py):
 * 0 save image pointer, 1 key entry offset, 2 keys found, 3 skulls done/total, 4 refreshes, 5 candidates */
u64 g_kset_dbg[6] = {MARK, MARK, MARK, MARK, MARK, MARK};

static const char STOCK_TEX[] = "MpGaugeCenter_00";
static const char FLAT_TEX[] = "White8_00_BC4L";
static const char CIRCLE_TEX[] = "Circle64_00";

/* parts: 0 = stamina gauge, 1..4 = item slots (order: B, L, R, ZR) */
/* Every part is a clone of the engine's gauge widget (item-button clones are never
 * registered by the engine, see docs/RE-NOTES.md). Part 0 is the stamina bar;
 * parts 1..4 are the slot circles: the gauge's fill texture is replaced by the
 * stock Circle64_00 and the gauge is kept full, so it draws a tinted circle. The
 * art is a placeholder until the real assets arrive. */
#define PART_BASE 12   /* the classic parts: buttons and items; the pause menu gets these only */
#define PART_N 14  /* (12..13: the shield button and the shield, see PART_SHIELD)  buttonMapping: parts 0..6 buttons (L X R Y A ZR B), 7..11 items (L R Y ZR B); clones of the stock item box Pa_SelectItem_00 (asset PaItemBtn_00).
                    * (The old gauge-part ring is retired: the stamina ring is drawn with picture panes, RT1..) */
#define APPEND_ALL 1   /* 1 = append every part (only the first one gets listed by the
                        * engine); 0 = parts 1.. repurpose filler parts (froze the game) */
#define SLOT_N 4
#ifndef SWAP_FILL
#define SWAP_FILL 1  /* 1 = replace the gauge's fill texture with a flat white one */
#endif

#define RING_SIZE 0.34f   /* whole-ring scale (part scale) */
#define SEG_N 128   /* ring segments = appended gauge parts */
static const char *part_replaces(u32 i);
/* names live in .data as char arrays (no pointer tables: the module has no relocations) */
static char g_pnames[SEG_N][16] = {"Pa_ShbSeg_000", "Pa_ShbSeg_001", "Pa_ShbSeg_002", "Pa_ShbSeg_003", "Pa_ShbSeg_004", "Pa_ShbSeg_005", "Pa_ShbSeg_006", "Pa_ShbSeg_007", "Pa_ShbSeg_008", "Pa_ShbSeg_009", "Pa_ShbSeg_010", "Pa_ShbSeg_011", "Pa_ShbSeg_012", "Pa_ShbSeg_013", "Pa_ShbSeg_014", "Pa_ShbSeg_015", "Pa_ShbSeg_016", "Pa_ShbSeg_017", "Pa_ShbSeg_018", "Pa_ShbSeg_019", "Pa_ShbSeg_020", "Pa_ShbSeg_021", "Pa_ShbSeg_022", "Pa_ShbSeg_023", "Pa_ShbSeg_024", "Pa_ShbSeg_025", "Pa_ShbSeg_026", "Pa_ShbSeg_027", "Pa_ShbSeg_028", "Pa_ShbSeg_029", "Pa_ShbSeg_030", "Pa_ShbSeg_031", "Pa_ShbSeg_032", "Pa_ShbSeg_033", "Pa_ShbSeg_034", "Pa_ShbSeg_035", "Pa_ShbSeg_036", "Pa_ShbSeg_037", "Pa_ShbSeg_038", "Pa_ShbSeg_039", "Pa_ShbSeg_040", "Pa_ShbSeg_041", "Pa_ShbSeg_042", "Pa_ShbSeg_043", "Pa_ShbSeg_044", "Pa_ShbSeg_045", "Pa_ShbSeg_046", "Pa_ShbSeg_047", "Pa_ShbSeg_048", "Pa_ShbSeg_049", "Pa_ShbSeg_050", "Pa_ShbSeg_051", "Pa_ShbSeg_052", "Pa_ShbSeg_053", "Pa_ShbSeg_054", "Pa_ShbSeg_055", "Pa_ShbSeg_056", "Pa_ShbSeg_057", "Pa_ShbSeg_058", "Pa_ShbSeg_059", "Pa_ShbSeg_060", "Pa_ShbSeg_061", "Pa_ShbSeg_062", "Pa_ShbSeg_063", "Pa_ShbSeg_064", "Pa_ShbSeg_065", "Pa_ShbSeg_066", "Pa_ShbSeg_067", "Pa_ShbSeg_068", "Pa_ShbSeg_069", "Pa_ShbSeg_070", "Pa_ShbSeg_071", "Pa_ShbSeg_072", "Pa_ShbSeg_073", "Pa_ShbSeg_074", "Pa_ShbSeg_075", "Pa_ShbSeg_076", "Pa_ShbSeg_077", "Pa_ShbSeg_078", "Pa_ShbSeg_079", "Pa_ShbSeg_080", "Pa_ShbSeg_081", "Pa_ShbSeg_082", "Pa_ShbSeg_083", "Pa_ShbSeg_084", "Pa_ShbSeg_085", "Pa_ShbSeg_086", "Pa_ShbSeg_087", "Pa_ShbSeg_088", "Pa_ShbSeg_089", "Pa_ShbSeg_090", "Pa_ShbSeg_091", "Pa_ShbSeg_092", "Pa_ShbSeg_093", "Pa_ShbSeg_094", "Pa_ShbSeg_095", "Pa_ShbSeg_096", "Pa_ShbSeg_097", "Pa_ShbSeg_098", "Pa_ShbSeg_099", "Pa_ShbSeg_100", "Pa_ShbSeg_101", "Pa_ShbSeg_102", "Pa_ShbSeg_103", "Pa_ShbSeg_104", "Pa_ShbSeg_105", "Pa_ShbSeg_106", "Pa_ShbSeg_107", "Pa_ShbSeg_108", "Pa_ShbSeg_109", "Pa_ShbSeg_110", "Pa_ShbSeg_111", "Pa_ShbSeg_112", "Pa_ShbSeg_113", "Pa_ShbSeg_114", "Pa_ShbSeg_115", "Pa_ShbSeg_116", "Pa_ShbSeg_117", "Pa_ShbSeg_118", "Pa_ShbSeg_119", "Pa_ShbSeg_120", "Pa_ShbSeg_121", "Pa_ShbSeg_122", "Pa_ShbSeg_123", "Pa_ShbSeg_124", "Pa_ShbSeg_125", "Pa_ShbSeg_126", "Pa_ShbSeg_127"};
static char g_anames[SEG_N][16] = {"PaShbSeg_000", "PaShbSeg_001", "PaShbSeg_002", "PaShbSeg_003", "PaShbSeg_004", "PaShbSeg_005", "PaShbSeg_006", "PaShbSeg_007", "PaShbSeg_008", "PaShbSeg_009", "PaShbSeg_010", "PaShbSeg_011", "PaShbSeg_012", "PaShbSeg_013", "PaShbSeg_014", "PaShbSeg_015", "PaShbSeg_016", "PaShbSeg_017", "PaShbSeg_018", "PaShbSeg_019", "PaShbSeg_020", "PaShbSeg_021", "PaShbSeg_022", "PaShbSeg_023", "PaShbSeg_024", "PaShbSeg_025", "PaShbSeg_026", "PaShbSeg_027", "PaShbSeg_028", "PaShbSeg_029", "PaShbSeg_030", "PaShbSeg_031", "PaShbSeg_032", "PaShbSeg_033", "PaShbSeg_034", "PaShbSeg_035", "PaShbSeg_036", "PaShbSeg_037", "PaShbSeg_038", "PaShbSeg_039", "PaShbSeg_040", "PaShbSeg_041", "PaShbSeg_042", "PaShbSeg_043", "PaShbSeg_044", "PaShbSeg_045", "PaShbSeg_046", "PaShbSeg_047", "PaShbSeg_048", "PaShbSeg_049", "PaShbSeg_050", "PaShbSeg_051", "PaShbSeg_052", "PaShbSeg_053", "PaShbSeg_054", "PaShbSeg_055", "PaShbSeg_056", "PaShbSeg_057", "PaShbSeg_058", "PaShbSeg_059", "PaShbSeg_060", "PaShbSeg_061", "PaShbSeg_062", "PaShbSeg_063", "PaShbSeg_064", "PaShbSeg_065", "PaShbSeg_066", "PaShbSeg_067", "PaShbSeg_068", "PaShbSeg_069", "PaShbSeg_070", "PaShbSeg_071", "PaShbSeg_072", "PaShbSeg_073", "PaShbSeg_074", "PaShbSeg_075", "PaShbSeg_076", "PaShbSeg_077", "PaShbSeg_078", "PaShbSeg_079", "PaShbSeg_080", "PaShbSeg_081", "PaShbSeg_082", "PaShbSeg_083", "PaShbSeg_084", "PaShbSeg_085", "PaShbSeg_086", "PaShbSeg_087", "PaShbSeg_088", "PaShbSeg_089", "PaShbSeg_090", "PaShbSeg_091", "PaShbSeg_092", "PaShbSeg_093", "PaShbSeg_094", "PaShbSeg_095", "PaShbSeg_096", "PaShbSeg_097", "PaShbSeg_098", "PaShbSeg_099", "PaShbSeg_100", "PaShbSeg_101", "PaShbSeg_102", "PaShbSeg_103", "PaShbSeg_104", "PaShbSeg_105", "PaShbSeg_106", "PaShbSeg_107", "PaShbSeg_108", "PaShbSeg_109", "PaShbSeg_110", "PaShbSeg_111", "PaShbSeg_112", "PaShbSeg_113", "PaShbSeg_114", "PaShbSeg_115", "PaShbSeg_116", "PaShbSeg_117", "PaShbSeg_118", "PaShbSeg_119", "PaShbSeg_120", "PaShbSeg_121", "PaShbSeg_122", "PaShbSeg_123", "PaShbSeg_124", "PaShbSeg_125", "PaShbSeg_126", "PaShbSeg_127"};
static const u64 g_phash[SEG_N] = {HASH_PART0, HASH_PART1, HASH_PART2, HASH_PART3, HASH_PART4, HASH_PART5, HASH_PART6, HASH_PART7, HASH_PART8, HASH_PART9, HASH_PART10, HASH_PART11, HASH_PART12, HASH_PART13, HASH_PART14, HASH_PART15, HASH_PART16, HASH_PART17, HASH_PART18, HASH_PART19, HASH_PART20, HASH_PART21, HASH_PART22, HASH_PART23, HASH_PART24, HASH_PART25, HASH_PART26, HASH_PART27, HASH_PART28, HASH_PART29, HASH_PART30, HASH_PART31, HASH_PART32, HASH_PART33, HASH_PART34, HASH_PART35, HASH_PART36, HASH_PART37, HASH_PART38, HASH_PART39, HASH_PART40, HASH_PART41, HASH_PART42, HASH_PART43, HASH_PART44, HASH_PART45, HASH_PART46, HASH_PART47, HASH_PART48, HASH_PART49, HASH_PART50, HASH_PART51, HASH_PART52, HASH_PART53, HASH_PART54, HASH_PART55, HASH_PART56, HASH_PART57, HASH_PART58, HASH_PART59, HASH_PART60, HASH_PART61, HASH_PART62, HASH_PART63, HASH_PART64, HASH_PART65, HASH_PART66, HASH_PART67, HASH_PART68, HASH_PART69, HASH_PART70, HASH_PART71, HASH_PART72, HASH_PART73, HASH_PART74, HASH_PART75, HASH_PART76, HASH_PART77, HASH_PART78, HASH_PART79, HASH_PART80, HASH_PART81, HASH_PART82, HASH_PART83, HASH_PART84, HASH_PART85, HASH_PART86, HASH_PART87, HASH_PART88, HASH_PART89, HASH_PART90, HASH_PART91, HASH_PART92, HASH_PART93, HASH_PART94, HASH_PART95, HASH_PART96, HASH_PART97, HASH_PART98, HASH_PART99, HASH_PART100, HASH_PART101, HASH_PART102, HASH_PART103, HASH_PART104, HASH_PART105, HASH_PART106, HASH_PART107, HASH_PART108, HASH_PART109, HASH_PART110, HASH_PART111, HASH_PART112, HASH_PART113, HASH_PART114, HASH_PART115, HASH_PART116, HASH_PART117, HASH_PART118, HASH_PART119, HASH_PART120, HASH_PART121, HASH_PART122, HASH_PART123, HASH_PART124, HASH_PART125, HASH_PART126, HASH_PART127};
#ifdef BUTTON_MAPPING      /* built with features/button_mapping */
int hud_shield_parts(void);
#define PART_TOT (g_shbcfg.button_mapping ? (hud_shield_parts() ? PART_N : PART_BASE) : 0)   /* parts appended to PlayerStatus_00 */
#define BTNMAP (g_shbcfg.button_mapping != 0)
#else
#define PART_TOT 0         /* built without features/button_mapping: no button parts at all */
#define BTNMAP 0
#endif
static const char *part_name(u32 i) { return g_pnames[i % SEG_N]; }
static u64 part_hash(u32 i) { return g_phash[i % SEG_N]; }
__attribute__((unused, noinline)) static const char *part_replaces(u32 i) { (void)i; return 0; }
static const char *asset_name(u32 i) { return g_anames[i % SEG_N]; }
/* ring centre: where the player usually stands on screen (layout units, root-relative, Y up;
 * ~0.356 px per unit at 720p). Placeholder until the ring follows his projected position. */
static const float g_part_x[1] = {250.0f};   /* beside the player (right), not on him */
static const float g_part_y[1] = {-300.0f};
static const float g_part_rgb[1][3] = {{0.30f, 0.85f, 0.25f}};   /* green */
#define CIRCLE_CAP 40.0f

/* -------- private allocation pool (bss, never freed) -------- */
static u8 g_pool[0x4C0000] __attribute__((aligned(16)));
static u32 g_used;

static void *pool_alloc(u32 n) {
    n = (n + 15u) & ~15u;
    if (n > sizeof(g_pool) - g_used) { g_hud_dbg[10] += 1; return 0; }
    void *p = g_pool + g_used;
    g_used += n;
    return p;
}
static void *pool_clone(const void *src, u32 n) {
    void *p = pool_alloc(n);
    if (p) memcpy(p, src, n);
    return p;
}

static u64 q(const void *p, u32 off) { return *(const u64 *)((const u8 *)p + off); }
static u32 w(const void *p, u32 off) { return *(const u32 *)((const u8 *)p + off); }
static int vptr(u64 p);
static int str_eq(const char *a, const char *b) {
    if (!vptr((u64)a) || !vptr((u64)b)) return 0;
    for (u32 i = 0; i < 96; i++) {
        if (a[i] != b[i]) return 0;
        if (!a[i]) return 1;
    }
    return 0;
}

/* name table at root+table: names[], descs[], hashes[], count at +24 */
/* The lowest address a real pointer can have. Rekarina: below 4 GB is not guest memory. The emulators (and the
 * recompiled builds) load main and this module below 4 GB (Fritchii: main 0x8100D000, the module 0x85F8D000), and
 * the HUD's own layout copies live in the module: with the 4 GB floor there every one of them was rejected and the
 * button mapping HUD was never built. */
#ifdef SHB_REKARINA
#define VPTR_MIN 0x100000000ULL
#else
#define VPTR_MIN 0x10000ULL
#endif
static int vptr(u64 p) { return p >= VPTR_MIN && p < 0x60000000000ULL; }
/* Out/input structures handed to the game's lifted functions live in the module's BSS: a C stack local's address
 * is not a guest address in Rekarina. */
static struct { const void *s; u32 len, z; long long m1; } g_text_ref;
static int name_index(const u8 *root, u32 table, const char *name) {
    if (!root || !name || !vptr((u64)root)) return -1;
    u32 n = w(root, table + 24);
    if (n > 512) return -1;
    const u64 *names = (const u64 *)q(root, table);
    if (!names || !vptr((u64)names)) return -1;
    for (u32 i = 0; i < n; i++) {
        u64 p = names[i];
        if (!vptr(p)) continue;          /* a layout being rebuilt can hold a freed name */
        if (str_eq((const char *)p, name)) return (int)i;
    }
    return -1;
}

/* The game's find_part()/ui_find_sub() walk the same name table without checks: a layout being rebuilt can hold a
 * freed name and the game dereferences it. Repair those slots with a valid empty name before handing the layout to
 * the game, and give the layout a couple of frames after a HUD init to settle. */
extern u64 ui_owner_layout(u64 pane);
extern u64 ui_find_sub(const char *name, u64 part, u64 *out);
static u64 ptr(u64 p, u32 off);
static char g_empty_name[1];
static int layout_repair(u64 layout) {
    if (!vptr(layout)) return 0;
    u64 r = ptr(layout, 0xF8);
    if (!vptr(r)) return 0;
    u32 n = w((const void *)r, 0x38);
    if (n > 512) return 0;
    u64 names = q((const void *)r, 0x20);
    if (!vptr(names)) return 0;
    u64 *a = (u64 *)names;
    for (u32 i = 0; i < n; i++)
        if (!vptr(a[i])) a[i] = (u64)g_empty_name;
    return 1;
}
static u64 find_part_safe(void *node_p, const char *name) {
    u64 node = (u64)node_p;
    if (!vptr(node)) return 0;
    int ok = layout_repair(ptr(node, 0x80));
    if (layout_repair(ui_owner_layout(node + 0x40))) ok = 1;
    return ok ? (u64)find_part(node_p, name) : 0;
}
static u64 ui_find_sub_safe(const char *name, u64 part, u64 *out) {
    if (!vptr(part)) return 0;
    int ok = layout_repair(ptr(part, 0x80));
    if (layout_repair(ui_owner_layout(part + 0x40))) ok = 1;
    return ok ? ui_find_sub(name, part, out) : 0;
}

/* -------- step 1: PlayerStatus metadata with extra parts -------- */
static void extend_pools(u8 *root, u32 add);
/* Test (multi-part): grow the per-type pools at the end of the root header
 * ({u32 count, u32 stride, u64 ptr}, 16 B each at +0x1D0,+0x1E0,+0x1F0,+0x210,+0x220) by
 * `add` entries, each a copy of the last one, in case the part/pane setup indexes them. */
static void extend_pools(u8 *root, u32 add) {
    static const u32 offs[5] = {0x1D0, 0x1E0, 0x1F0, 0x210, 0x220};
    for (u32 i = 0; i < 5; i++) {
        u32 off = offs[i];
        u32 cnt = w(root, off), stride = w(root, off + 4);
        const u8 *old = (const u8 *)q(root, off + 8);
        if (!old || cnt == 0 || cnt > 256 || stride == 0 || stride > 64) continue;
        u8 *neu = pool_alloc((cnt + add) * stride);
        if (!neu) continue;
        memcpy(neu, old, cnt * stride);
        for (u32 j = 0; j < add; j++) memcpy(neu + (cnt + j) * stride, old + (cnt - 1) * stride, stride);
        *(u64 *)(root + off + 8) = (u64)neu;
        *(u32 *)(root + off) = cnt + add;
    }
}

/* ---- clock / rupee / collectible plates, drawn like the game's own number display ----
 * Text fields: 0 = clock, 1 = rupees (green), 2 = "/wallet" (white), 3 = area collectibles,
 * 4 = X button label (Jump), 5 = A button label (context).
 * Each field has two panes (field f uses panes f and f + FLD_N): the text is written to the hidden one. */
#define FLD_N 7
#define TXT_N (FLD_N * 2)
static const char g_font_normal[] = "Normal";
static const char g_font_ext[] = "External_00";   /* the button-glyph font */
static char g_tnames[TXT_N][16] = {"T_ShbTxt_00", "T_ShbTxt_01", "T_ShbTxt_02", "T_ShbTxt_03",
                                   "T_ShbTxt_04", "T_ShbTxt_05", "T_ShbTxt_06", "T_ShbTxt_07",
                                   "T_ShbTxt_08", "T_ShbTxt_09", "T_ShbTxt_10", "T_ShbTxt_11",
                                   "T_ShbTxt_12", "T_ShbTxt_13"};
static const u64 g_thash[TXT_N] = {HASH_TXT0, HASH_TXT1, HASH_TXT2, HASH_TXT3, HASH_TXT4, HASH_TXT5, HASH_TXT6,
                                   HASH_TXT7, HASH_TXT8, HASH_TXT9, HASH_TXT10, HASH_TXT11, HASH_TXT12, HASH_TXT13};
/* Picture panes. Each gets its own copy of the textured material of P_SubItemBg_00 with the game's
 * texture named below (all in the Common texture archive):
 *  0..3  clock plate:  fill, fill mirrored, edge, edge mirrored   (NumBaseSide_00 / _01, the plate of
 *  4..7  rupee plate:  same                                         the game's rupee counter)
 *  8     rupee icon    (Nt_RupeeIconNew_00, the game's green rupee)
 *  9     sun disc, 10..17 sun rays (Circle64_00 / White8_00_BC4L)
 *  18    moon disc, 19 moon shadow (both Circle64_00: a crescent)
 *  20..  stamina ring: overlapping Circle64_00 dots along two circles, drawn as a continuous tube.
 *        Order = draw order: dark track dots of ring 1 and ring 2 first, then the fill dots of ring 1 and
 *        ring 2 (each fill has one extra dot that sits exactly at the fill angle, so the end moves smoothly). */
#define RING1_DOTS 48
#define RING2_DOTS 64
#define RT1 20                              /* ring 1 track */
#define RT2 (RT1 + RING1_DOTS)              /* ring 2 track */
#define RF1 (RT2 + RING2_DOTS)              /* ring 1 fill (+1 end dot) */
#define RF2 (RF1 + RING1_DOTS + 1)          /* ring 2 fill (+1 end dot) */
#define PIC_COLL (RF2 + RING2_DOTS + 1)     /* first pane of the collectible plates */
#define PIC_BAR (PIC_COLL + 10)             /* stamina bar: outline, empty track, then the segments */
#define BAR_OUT 0                           /* (relative to PIC_BAR) */
#define BAR_BASE 1
#define BAR_SEG0 2
#define BAR_SEG 16
#define BAR_N (BAR_SEG0 + BAR_SEG)
#define PIC_BG (PIC_BAR + BAR_N)            /* simple layout: the translucent bar behind the four blocks */
#define PIC_N (PIC_BG + 1)
static char g_pinames[PIC_N][16];           /* "P_ShbPic_000".. filled by pic_names() */
static const u64 g_pihash[400] = {HASH_PIC_LIST};   /* CityHash64 of "P_ShbPic_000".. (tools/build_core.py) */
_Static_assert(PIC_N <= 400, "more picture panes than generated hashes");
#define TEX_N 7
static char g_texn[TEX_N][32] = {"NumBaseSide_00", "NumBaseSide_01", "Nt_RupeeIconNew_00", "Circle64_00",
                                 "White8_00_BC4L", "MapIconCollectSkullSign_00", "MapIconCollectHeartPiece_00"};
static const u8 g_pitex0[20] = {0, 0, 1, 1, 0, 0, 1, 1, 2, 3, 4, 4, 4, 4, 4, 4, 4, 4, 3, 3};
/* pane sizes in layout units (scaled at runtime); ring dots are 100 x 100 */
static const float g_pic_w0[20] = {1014, 1014, 1014, 1014, 1014, 1014, 1014, 1014, 120, 60, 9, 9, 9, 9, 9, 9, 9, 9, 76, 70};
static const float g_pic_h0[20] = {160, 160, 160, 160, 160, 160, 160, 160, 120, 60, 26, 26, 26, 26, 26, 26, 26, 26, 76, 70};
static u32 pic_tex(u32 k) {
    if (k >= PIC_BAR) return 4;   /* flat white panes: the bar draws its own segments in a solid colour */
    if (k >= PIC_COLL) {
        u32 j = k - PIC_COLL;
        if (j < 8) return ((j & 3u) < 2u) ? 0 : 1;   /* two plates: fill, fill mirrored, edge, edge mirrored */
        return j == 8 ? 5 : 6;                       /* skull icon, heart icon */
    }
    return k < 20 ? g_pitex0[k] : 3;
}
static float pic_w(u32 k) {
    if (k >= PIC_BAR) return 100.0f;   /* bar panes are scaled from a 100-unit base */
    if (k >= PIC_COLL) return (k - PIC_COLL) < 8 ? 1014.0f : 128.0f;
    return k < 20 ? g_pic_w0[k] : 100.0f;
}
static float pic_h(u32 k) {
    if (k >= PIC_BAR) return 100.0f;
    if (k >= PIC_COLL) return (k - PIC_COLL) < 8 ? 160.0f : 128.0f;
    return k < 20 ? g_pic_h0[k] : 100.0f;
}
static void pic_names(void) {
    for (u32 k = 0; k < PIC_N; k++) {
        static const char P[] = "P_ShbPic_";
        char *d = g_pinames[k];
        for (u32 i = 0; i < 9; i++) d[i] = P[i];
        d[9] = (char)('0' + k / 100); d[10] = (char)('0' + k / 10 % 10); d[11] = (char)('0' + k % 10); d[12] = 0;
    }
}
/* layout units (root-relative, Y up) */
#define HUD_X (-1450.0f)
#define HUD_Y_CLOCK (-880.0f)
#define HUD_Y_RUPEE (-990.0f)
#define HUD_Y_COLLECT (-960.0f)
/* Look parameters (in .data so they can be tuned live with tools/hud_tune.py):
 *  0 plate centre x, 1 clock row y, 2 rupee row y, 3 plate scale x, 4 plate scale y,
 *  5 icon x (from the plate centre), 6 icon scale, 7 text x (from the plate centre), 8 text y offset,
 *  9 advance of one digit (text units), 10 fill alpha, 11 edge alpha, 12 sun ray radius,
 *  13 text scale, 14 moon shadow offset, 15 edge brightness, 16 icon override (0 by the hour, 1 sun, 2 moon),
 *  17 moon scale, 18 moon shadow scale, 19 moon shadow brightness (matches the plate),
 *  20 clock plate x, 21 clock plate y (the clock sits to the right of the rupee counter),
 *  22 skull plate x, 23 skull plate y, 24 heart plate x, 25 heart plate y (to the right of the clock)
 * Values tuned live against the look of the game's own rupee counter. */
float g_hudp[26] = {-1518.0f, -860.0f, -960.0f, 0.46f, 0.62f, -165.0f, 0.85f, 30.0f, 0.0f,
                    29.0f, 0.9f, 0.6f, 30.0f, 0.7f, 14.0f, 0.45f, 0.0f, 0.75f, 0.75f, 0.09f,
                    -1000.0f, -960.0f, -660.0f, -960.0f, -190.0f, -960.0f};
/* HUD layout: the app writes the placement/scales in the configuration (wide, simple or map); these are the
 * tuned defaults used by older configurations (hud_flags == 0). */
static int tbl_add(u8 *root, u32 off, const char *name, void *desc, u64 hash) {
    u32 n = w(root, off + 24);
    for (u32 j = 0; j < 3; j++) {
        u64 *a = pool_alloc((n + 1) * 8);
        if (!a) return 0;
        memcpy(a, (const void *)q(root, off + j * 8), n * 8);
        a[n] = j == 0 ? (u64)name : j == 1 ? (u64)desc : hash;
        *(u64 *)(root + off + j * 8) = (u64)a;
    }
    *(u32 *)(root + off + 24) = n + 1;
    return 1;
}

/* Materials: root+0x130 = {u64 *materials, u32 count, u32 stride}. A picture's material index is at +0x14 of its
 * entry in the picture/text list (+0x40). A textured material starts with a sampler list {u64 *samplers, count};
 * each sampler is {u32 flags, u32 pad, u64 texture name}. The texture is looked up by name in the layout
 * archive's texture file when the layout is built. This adds a deep copy of material `src` (its bytes up to the
 * next material, internal pointers rebased) whose first sampler names `tex`; returns its index or -1. */
static int add_material_sz(u8 *root, const u8 *r, u32 src, const char *tex, u32 last_size);
static int add_material(u8 *root, const u8 *r, u32 src, const char *tex) { return add_material_sz(root, r, src, tex, 0); }
/* tex = 0 keeps the texture; last_size = byte size of `src` when it is the last material (no next one to measure) */
static int add_material_sz(u8 *root, const u8 *r, u32 src, const char *tex, u32 last_size) {
    u32 n = w(root, 0x138), on = w(r, 0x138);
    const u64 *arr = (const u64 *)q(root, 0x130), *oarr = (const u64 *)q(r, 0x130);
    if (!arr || !oarr || src >= on || (src + 1 >= on && !last_size) || n > 256) return -1;
    u64 a = oarr[src], e = src + 1 < on ? oarr[src + 1] : a + last_size;
    if (e <= a || e - a > 0x400) return -1;
    u32 sz = (u32)(e - a);
    u8 *m = pool_clone((const void *)a, sz);
    u64 *na = pool_alloc((n + 1) * 8);
    if (!m || !na) return -1;
    for (u32 i = 0; i + 8 <= sz; i += 8) {
        u64 v = q(m, i);
        if (v >= a && v < e) *(u64 *)(m + i) = (u64)m + (v - a);
    }
    u64 sl = q(m, 0);
    if (sl < (u64)m || sl >= (u64)m + sz) return -1;
    u64 sp = *(u64 *)sl;
    if (sp < (u64)m || sp + 16 > (u64)m + sz) return -1;
    if (tex) *(u64 *)(sp + 8) = (u64)tex;
    memcpy(na, arr, n * 8);
    na[n] = (u64)m;
    *(u64 *)(root + 0x130) = (u64)na;
    *(u32 *)(root + 0x138) = n + 1;
    return (int)n;
}

/* Our buttons' nested Pa_ReplaceIcon_00: the shadow pane (where the button is drawn) gets a copy of the ICON
 * pane's material (material 1, 0xF0 bytes, a different shader variant) instead of the shadow material, which
 * draws its texture washed out and translucent. Stock item boxes keep the stock asset. */
u32 g_repicon_dbg[2];   /* private copies handed out, prepares seen while armed */
static u8 *build_repicon(const u8 *r) {
    int si = name_index(r, 0x40, "P_ReplaceIconSh_00");
    u32 nb = w(r, 0x58);
    if (si < 0 || nb > 64 || w(r, 0x138) != 2) return 0;
    u8 *root = pool_clone(r, 0x228);
    if (!root) return 0;
    int mi = add_material_sz(root, r, 1, 0, 0xF0);
    if (mi < 0) return 0;
    u64 *descs = pool_alloc(nb * 8);
    u8 *d = pool_clone((const void *)((const u64 *)q(r, 0x48))[si], 0x88);
    if (!descs || !d) return 0;
    memcpy(descs, (const void *)q(r, 0x48), nb * 8);
    *(u32 *)(d + 0x14) = (u32)mi;
    descs[si] = (u64)d;
    *(u64 *)(root + 0x48) = (u64)descs;
    g_repicon_dbg[0]++;
    return root;
}
static u32 g_repicon_armed;

/* Current design: ONE appended part (stamina gauge, the layout takes exactly one) and
 * the slot parts are four stock filler parts turned into item buttons in place: same
 * pane slot, same name, same parent; only the part descriptor changes to the item
 * button's. They are driven by their original names. */
static const u8 *g_proto_c, *g_proto_p;   /* PlayerStatus' Pa_SelectItem_00 descriptors (set by build_root) */
u32 g_menu_build_dbg[4];   /* menu copies built, failures (step), panes, parts */
/* The pause menu's Items page (PaItemMenu_00) gets the same PART_N item-button parts appended (clones of the
 * PlayerStatus item box, asset PaItemBtn_00), so the cluster shows over the menu: the HUD layer is not drawn
 * while the menu is open. */
static u8 *build_menu(const u8 *r) {
    if (!g_proto_c || !g_proto_p) { g_menu_build_dbg[1] = 1; return 0; }
    if (name_index(r, 0x20, part_name(0)) >= 0) { g_menu_build_dbg[1] = 2; return 0; }
    u32 nc = w(r, 0x38), np = w(r, 0xD8);
    g_menu_build_dbg[2] = nc; g_menu_build_dbg[3] = np;
    if (nc > 256 || np > 128 || !q(r, 0xC8)) { g_menu_build_dbg[1] = 3; return 0; }
    u8 *root = pool_clone(r, 0x228);
    u8 *first = pool_clone((const void *)((const u64 *)q(r, 0x28))[0], 68);
    if (!root || !first) { g_menu_build_dbg[1] = 4; return 0; }
    u8 *cdk[PART_BASE], *pdk[PART_BASE];
    for (u32 k = 0; k < PART_BASE; k++) {
        cdk[k] = pool_clone(g_proto_c, 68);
        pdk[k] = pool_clone(g_proto_p, 88);
        if (!cdk[k] || !pdk[k]) { g_menu_build_dbg[1] = 4; return 0; }
        *(u32 *)(cdk[k] + 40) = 0;                    /* parent: root pane */
        *(u32 *)(cdk[k] + 52) = nc + k + 1;           /* subtree end = own index + 1 */
        *(u32 *)(cdk[k] + 64) = 0x7FFFF;
        cdk[k][56] |= 1;
        *(unsigned short *)(cdk[k] + 0x42) = (unsigned short)(np + k);
        *(float *)(cdk[k] + 0x10) = 0.0f; *(float *)(cdk[k] + 0x14) = 0.0f;   /* hidden until placed */
        *(u32 *)(pdk[k] + 64) = nc + k;               /* part -> index of its pane */
    }
    *(u32 *)(first + 52) = nc + PART_BASE;
    for (u32 t = 0; t < 2; t++) {
        u32 off = t ? 0xC0 : 0x20, n = t ? np : nc;
        for (u32 j = 0; j < 3; j++) {
            u64 *a = pool_alloc((n + PART_BASE) * 8);
            if (!a) { g_menu_build_dbg[1] = 5; return 0; }
            memcpy(a, (const void *)q(r, off + j * 8), n * 8);
            for (u32 k = 0; k < PART_BASE; k++)
                a[n + k] = j == 0 ? (u64)part_name(k) : j == 1 ? (u64)(t ? pdk[k] : cdk[k]) : part_hash(k);
            if (!t && j == 1) a[0] = (u64)first;
            *(u64 *)(root + off + j * 8) = (u64)a;
        }
        *(u32 *)(root + off + 24) = n + PART_BASE;
    }
    g_menu_build_dbg[0]++;
    return root;
}
/* The pause menu's root layout (PlusMenu_00) gets LEG_N text panes for the assign legend (hud_menu_legend):
 * clones of its own T_Text_00 (the Items page has no text pane to clone, and a text pane only works with its own
 * layout's fonts and materials). Names T_ShbTxt_14.. (the HUD uses _00.._13, in another layout). */
#define LEG_N 13   /* "Asignar:", then per pair: source button icon, arrow, slot icon */
static char g_lnames[LEG_N][16] = {"T_ShbTxt_14", "T_ShbTxt_15", "T_ShbTxt_16", "T_ShbTxt_17", "T_ShbTxt_18", "T_ShbTxt_19", "T_ShbTxt_20", "T_ShbTxt_21", "T_ShbTxt_22", "T_ShbTxt_23", "T_ShbTxt_24", "T_ShbTxt_25", "T_ShbTxt_26"};   /* arrays, not pointers: no relocations */
static const u64 g_lhash[LEG_N] = {HASH_TXT14, HASH_TXT15, HASH_TXT16, HASH_TXT17, HASH_TXT18, HASH_TXT19, HASH_TXT20, HASH_TXT21, HASH_TXT22, HASH_TXT23, HASH_TXT24, HASH_TXT25, HASH_TXT26};
/* icon panes use the game's button-glyph font (the guide bar's icons): text 0xE040 A, 0xE041 B, 0xE042 X,
 * 0xE043 Y, 0xE044 L, 0xE045 R, 0xE046 ZL, 0xE047 ZR (UTF-8) */
#define LEG_ICON(t) ((t) > 0 && (t) % 3 != 2)
#define LEG_PAIR_L 1   /* pairs: X > B, L > L, R > R, A > ZR */
static char g_legbar_name[16] = "P_ShbPic_399";   /* the legend's backing bar */
#define LEG_BASE "P_MessageBase_01"   /* 2200 x 360: the plate PlusMenu_00 puts its own texts on */
u32 g_plus_build_dbg[2];   /* copies built, failure step */
static u8 *build_plus(const u8 *r) {
    if (name_index(r, 0x20, g_lnames[0]) >= 0) { g_plus_build_dbg[1] = 2; return 0; }
    int ci = name_index(r, 0x20, "T_Text_00"), bi = name_index(r, 0x40, "T_Text_00"), ti = name_index(r, 0x80, "T_Text_00");
    int pci = name_index(r, 0x20, LEG_BASE), pbi = name_index(r, 0x40, LEG_BASE), ppi = name_index(r, 0x60, LEG_BASE);
    u32 nc = w(r, 0x38), nb = w(r, 0x58), nt = w(r, 0x98), np = w(r, 0x78);
    if (ci < 0 || bi < 0 || ti < 0 || pci < 0 || pbi < 0 || ppi < 0 || nc > 512) { g_plus_build_dbg[1] = 3; return 0; }
    u8 *root = pool_clone(r, 0x228);
    u8 *first = pool_clone((const void *)((const u64 *)q(r, 0x28))[0], 68);
    if (!root || !first) { g_plus_build_dbg[1] = 4; return 0; }
    {   /* the legend's backing plate first (drawn under the texts): a clone of the menu's own message plate */
        u8 *cd = pool_clone((const void *)((const u64 *)q(r, 0x28))[pci], 68);
        u8 *bd = pool_clone((const void *)((const u64 *)q(r, 0x48))[pbi], 0x88);
        u8 *pd = pool_clone((const void *)((const u64 *)q(r, 0x68))[ppi], 0x88);
        if (!cd || !bd || !pd) { g_plus_build_dbg[1] = 4; return 0; }
        *(u32 *)(pd + 0x10) = nb;
        *(u32 *)(cd + 40) = 0;
        *(u32 *)(cd + 52) = nc + 1;
        *(float *)(cd + 0x10) = 0.0f; *(float *)(cd + 0x14) = 0.0f;   /* hidden until placed */
        *(unsigned short *)(cd + 0x3c) = (unsigned short)nb;
        *(unsigned short *)(cd + 0x40) = 0xFFFF;
        *(unsigned short *)(cd + 0x42) = (unsigned short)np;
        if (!tbl_add(root, 0x20, g_legbar_name, cd, g_pihash[399]) || !tbl_add(root, 0x40, g_legbar_name, bd, g_pihash[399]) ||
            !tbl_add(root, 0x60, g_legbar_name, pd, g_pihash[399])) { g_plus_build_dbg[1] = 5; return 0; }
        nc++; nb++;   /* the texts follow it */
    }
    for (u32 t = 0; t < LEG_N; t++) {
        u8 *cd = pool_clone((const void *)((const u64 *)q(r, 0x28))[ci], 68);
        u8 *bd = pool_clone((const void *)((const u64 *)q(r, 0x48))[bi], 0x88);
        u8 *td = pool_clone((const void *)((const u64 *)q(r, 0x88))[ti], 0xA8);
        if (!cd || !bd || !td) { g_plus_build_dbg[1] = 4; return 0; }
        *(u64 *)(td + 0x18) = q(td, 0x10);   /* default text: the empty string */
        if (LEG_ICON(t)) *(u64 *)(td + 0x00) = (u64)g_font_ext;
        *(float *)(td + 0x98) = 1500.0f;
        *(float *)(cd + 0x2C) = 900.0f; *(float *)(cd + 0x30) = 90.0f;
        *(float *)(cd + 0x10) = 0.0f; *(float *)(cd + 0x14) = 0.0f;   /* hidden until placed */
        *(u32 *)(cd + 40) = 0;
        *(u32 *)(cd + 52) = nc + t + 1;
        *(unsigned short *)(cd + 0x3c) = (unsigned short)(nb + t);
        *(unsigned short *)(cd + 0x42) = (unsigned short)(nt + t);
        if (!tbl_add(root, 0x20, g_lnames[t], cd, g_lhash[t]) || !tbl_add(root, 0x40, g_lnames[t], bd, g_lhash[t]) ||
            !tbl_add(root, 0x80, g_lnames[t], td, g_lhash[t])) { g_plus_build_dbg[1] = 5; return 0; }
    }
    *(u32 *)(first + 52) = nc + LEG_N;   /* (nc counts the plate already) */
    ((u64 *)q(root, 0x28))[0] = (u64)first;
    {   /* per picture/text-list entry table: grow it */
        u32 oc = w(r, 0x210);
        const u8 *op = (const u8 *)q(r, 0x208);
        if (op && oc == nb - 1) {   /* (nb counts the plate already) */
            int *na = pool_alloc((oc + 1 + LEG_N) * 4);
            if (!na) { g_plus_build_dbg[1] = 5; return 0; }
            memcpy(na, op, oc * 4);
            for (u32 k = 0; k < 1 + LEG_N; k++) na[oc + k] = -1;
            *(u64 *)(root + 0x208) = (u64)na;
            *(u32 *)(root + 0x210) = oc + 1 + LEG_N;
        } else { g_plus_build_dbg[1] = 6; return 0; }
    }
    g_plus_build_dbg[0]++;
    return root;
}
/* Minus map (MapMenu_00): one extra picture pane, the heart-piece icon, using the map's own heart texture.
 * Placed by hud_on_settext() next to the native Skulltula counter when HUD mode 2 is on. */
u32 g_map_build_dbg[4];
static u8 *build_map(const u8 *r) {
    int ci = name_index(r, 0x20, "P_Collect_00"), bi = name_index(r, 0x40, "P_Collect_00"),
        pi = name_index(r, 0x60, "P_Collect_00");
    u32 nc = w(r, 0x38), nb = w(r, 0x58), np = w(r, 0x78);
    g_map_build_dbg[0] = 1;
    if (ci < 0 || bi < 0 || pi < 0 || nc > 512 || nb > 512 || np > 512) {
        g_map_build_dbg[1] = (u32)(ci & 0xFF) | ((u32)(bi & 0xFF) << 8) | ((u32)(pi & 0xFF) << 16);
        return 0;
    }
    u8 *root = pool_clone(r, 0x228);
    u8 *first = pool_clone((const void *)((const u64 *)q(r, 0x28))[0], 68);
    u8 *cd = pool_clone((const void *)((const u64 *)q(r, 0x28))[ci], 68);
    u8 *bd = pool_clone((const void *)((const u64 *)q(r, 0x48))[bi], 0x88);
    u8 *pd = pool_clone((const void *)((const u64 *)q(r, 0x68))[pi], 0x88);
    if (!root || !first || !cd || !bd || !pd) { g_map_build_dbg[1] = 2; return 0; }
    int mat = add_material(root, r, *(u32 *)(pd + 0x14), "MapIconCollectHeartPiece_00");
    g_map_build_dbg[2] = (u32)mat;
    if (mat < 0) { g_map_build_dbg[1] = 3; return 0; }
    *(u32 *)(pd + 0x14) = (u32)mat;
    *(float *)(cd + 0x10) = 0.0f; *(float *)(cd + 0x14) = 0.0f;   /* hidden until placed */
    *(u32 *)(cd + 40) = 0;
    *(u32 *)(cd + 52) = nc + 1;
    *(unsigned short *)(cd + 0x3c) = (unsigned short)nb;
    *(unsigned short *)(cd + 0x42) = (unsigned short)np;
    if (!tbl_add(root, 0x20, "P_ShbPic_398", cd, g_pihash[398]) ||
        !tbl_add(root, 0x40, "P_ShbPic_398", bd, g_pihash[398]) ||
        !tbl_add(root, 0x60, "P_ShbPic_398", pd, g_pihash[398])) { g_map_build_dbg[1] = 4; return 0; }
    *(u32 *)(first + 52) = nc + 1;
    ((u64 *)q(root, 0x28))[0] = (u64)first;
    {   /* per picture/text-list entry table: grow it */
        u32 oc = w(r, 0x210);
        const u8 *op = (const u8 *)q(r, 0x208);
        if (!op || oc != nb) { g_map_build_dbg[1] = 5; g_map_build_dbg[3] = oc * 4096u + nb; return 0; }
        int *na = pool_alloc((oc + 1) * 4);
        if (!na) return 0;
        memcpy(na, op, oc * 4);
        na[oc] = -1;
        *(u64 *)(root + 0x208) = (u64)na;
        *(u32 *)(root + 0x210) = oc + 1;
    }
    g_map_build_dbg[1] = 0;
    return root;
}
/* A nonzero parent origin lets widescreen planners anchor the whole cluster.
 * Child positions remain in canvas units at the call sites. */
#define ACTIONS_X 1800.0f
/* The counters row sits on the left in every HUD mode (the old right-side map layout is gone), so the
 * counters group's anchor stays on that side: a group anchored past the opposite edge would straddle the
 * whole screen and the planner would not move it as one cluster. */
static float counters_x(void) { return -1800.0f; }
static int action_field(u32 field) { return field == 4 || field == 5; }
/* ---- HUD group scale (hud_ctrl_scale / hud_count_scale) ----
 * Each group pane is scaled as a whole; its position moves so the screen corner nearest the group stays where it
 * is (local corner offset L: position = base + (1 - s) * L). A position another writer set (a widescreen
 * planner) becomes the new base, so it is never undone. At 100% nothing is written. */
static u64 pane_record(u64 frame, const char *name);
static void place(u64 rec, float x, float y, float sx, float sy, float rot);
static const char HG_NAME[2][20] = {"N_ShbActions_00", "N_ShbCounters_00"};
static const float HG_CORNER[2][2] = {{120.0f, 1080.0f}, {-120.0f, -1080.0f}};   /* top right / bottom left */
static float g_hg_base[2][2], g_hg_last[2][2];
static u32 g_hg_on[2];
float g_hg_dbg[2][3];   /* diagnostic: scale, x, y written per group */
static void hud_group_scale(u64 frame) {
    for (u32 g = 0; g < 2; g++) {
        u32 pct = g ? g_shbcfg.hud_count_scale : g_shbcfg.hud_ctrl_scale;
        if (!pct) pct = 100;
        if (pct < 25) pct = 25;
        if (pct > 250) pct = 250;
        if (pct == 100 && !g_hg_on[g]) continue;
        u64 rec = pane_record(frame, HG_NAME[g]);
        if (!rec) continue;
        float x = *(float *)(rec + 0x6C), y = *(float *)(rec + 0x70);
        if (!g_hg_on[g] || x != g_hg_last[g][0] || y != g_hg_last[g][1]) { g_hg_base[g][0] = x; g_hg_base[g][1] = y; }
        float s = (float)pct / 100.0f;
        float nx = g_hg_base[g][0] + (1.0f - s) * HG_CORNER[g][0], ny = g_hg_base[g][1] + (1.0f - s) * HG_CORNER[g][1];
        place(rec, nx, ny, s, s, 0.0f);
        g_hg_last[g][0] = nx; g_hg_last[g][1] = ny;
        g_hg_on[g] = pct != 100;
        g_hg_dbg[g][0] = s; g_hg_dbg[g][1] = nx; g_hg_dbg[g][2] = ny;
    }
}
static int counter_picture(u32 k) { return k < RT1 || (k >= PIC_COLL && k < PIC_BAR); }

static int group_hud_panes(u8 *root, u8 *first, u32 nc, u32 parts, u8 **part_desc) {
    const u32 pics = nc + parts, texts = pics + PIC_N, groups = texts + TXT_N;
    const float origins[] = {ACTIONS_X, counters_x(), 0.0f};
    static const char names[3][20] = {"N_ShbActions_00", "N_ShbCounters_00", "N_ShbStaminaRing_00"};   /* arrays, not pointers */
    const u64 hashes[] = {0xe209072c44f5dda5ULL, 0xf44ec55ca35671a1ULL, 0xac10de115037b4caULL};
    for (u32 g = 0; g < 3; g++) {
        u8 *group = pool_clone(first, 68);   /* the stock root is a non-drawing pane */
        if (!group) return 0;
        *(float *)(group + 0x10) = 1.0f;
        *(float *)(group + 0x14) = 1.0f;
        *(float *)(group + 28) = origins[g];
        *(float *)(group + 32) = 0.0f;
        *(u32 *)(group + 40) = 0;
        if (!tbl_add(root, 0x20, names[g], group, hashes[g])) return 0;
    }
    /* The UIL stores a parent's descendants in one contiguous subtree.
     * Picture, text and part table indices stay independent of this order. */
    u32 order[PART_N + PIC_N + TXT_N + 3], next = 0;
    order[next++] = groups;
    for (u32 k = 0; k < parts; k++) order[next++] = nc + k;
    for (u32 k = 0; k < TXT_N; k++) if (action_field(k % FLD_N)) order[next++] = texts + k;
    const u32 actions_end = next;
    order[next++] = groups + 1;
    for (u32 k = 0; k < PIC_N; k++) if (counter_picture(k)) order[next++] = pics + k;
    for (u32 k = 0; k < TXT_N; k++) if (!action_field(k % FLD_N)) order[next++] = texts + k;
    order[next++] = pics + PIC_BG;   /* the simple row's soft bar travels with the row */
    const u32 counters_end = next;
    /* Keep the ring's hundreds of dots from filling the root's edge-planner slots. */
    order[next++] = groups + 2;
    for (u32 k = RT1; k < PIC_COLL; k++) order[next++] = pics + k;
    const u32 ring_end = next;
    for (u32 k = PIC_BAR; k < PIC_BG; k++) order[next++] = pics + k;   /* the stamina bar keeps its own coordinates */

    for (u32 j = 0; j < 3; j++) {
        u64 *table = (u64 *)q(root, 0x20 + j * 8);
        u64 reordered[PART_N + PIC_N + TXT_N + 3];
        for (u32 k = 0; k < next; k++) reordered[k] = table[order[k]];
        memcpy(table + nc, reordered, next * sizeof(*table));
    }
    u64 *descs = (u64 *)q(root, 0x28);
    for (u32 k = 0; k < next; k++) {
        u8 *desc = (u8 *)descs[nc + k];
        if (k == 0 || k == actions_end || k == counters_end) {
            *(u32 *)(desc + 52) = nc + (k == 0 ? actions_end : k == actions_end ? counters_end : ring_end);
            continue;
        }
        *(u32 *)(desc + 52) = nc + k + 1;
        if (k < counters_end) {
            const u32 g = k < actions_end ? 0 : 1;
            *(u32 *)(desc + 40) = nc + (g ? actions_end : 0);
            *(float *)(desc + 28) = g ? 0.0f : 1400.0f - ACTIONS_X;
        }
        if (k > counters_end && k < ring_end) {
            *(u32 *)(desc + 40) = nc + counters_end;
            *(float *)(desc + 28) = 0.0f;
        }
        if (order[k] >= nc && order[k] < pics)
            *(u32 *)(part_desc[order[k] - nc] + 64) = nc + k;
    }
    *(u32 *)(first + 52) = nc + next;
    return 1;
}

static u8 *build_root(const u8 *r) {
    pic_names();
    if (name_index(r, 0x20, g_pinames[0]) >= 0) { g_hud_dbg[9] = MARK | 1; return 0; }
    u32 nc = w(r, 0x38), np = w(r, 0xD8);
    if (nc > 128 || np > 128) { g_hud_dbg[9] = MARK | 2; return 0; }
    int mci = name_index(r, 0x20, "Pa_SelectItem_00"), mpi = name_index(r, 0xC0, "Pa_SelectItem_00");
    if (mci < 0 || mpi < 0) { g_hud_dbg[9] = MARK | 3; return 0; }
    u8 *root = pool_clone(r, 0x228);
    u8 *first = pool_clone((const void *)((const u64 *)q(r, 0x28))[0], 68);
    if (!root || !first) { g_hud_dbg[9] = MARK | 4; return 0; }
    g_proto_c = (const u8 *)((const u64 *)q(r, 0x28))[mci];   /* stock item box: pane + part descriptors */
    g_proto_p = (const u8 *)((const u64 *)q(r, 0xC8))[mpi];
    /* PART_N appended gauge parts (the ring segments): pane nc+k, part descriptor np+k.
     * The engine finds each part's descriptor through cdesc+0x42, so it must name np+k. */
    u8 *cdk[PART_TOT], *pdk[PART_TOT];
    for (u32 k = 0; k < PART_TOT; k++) {
        cdk[k] = pool_clone((const void *)((const u64 *)q(r, 0x28))[mci], 68);
        pdk[k] = pool_clone((const void *)((const u64 *)q(r, 0xC8))[mpi], 88);
        if (!cdk[k] || !pdk[k]) { g_hud_dbg[9] = MARK | 4; return 0; }
        *(u32 *)(cdk[k] + 40) = 0;                    /* parent: root pane */
        *(u32 *)(cdk[k] + 52) = nc + k + 1;           /* subtree end = own index + 1 */
        *(u32 *)(cdk[k] + 64) = 0x7FFFF;
        cdk[k][56] |= 1;
        *(unsigned short *)(cdk[k] + 0x42) = (unsigned short)(np + k);
        /* keeps the stock item box's scale; placed at runtime (slots_update) */
        *(float *)(cdk[k] + 28) = -600.0f + 300.0f * (float)k;
        *(float *)(cdk[k] + 32) = 0.0f;
        *(u32 *)(pdk[k] + 64) = nc + k;               /* part -> index of its pane */
    }
    *(u32 *)(first + 52) = nc + PART_TOT + PIC_N + TXT_N;
    for (u32 t = 0; t < 2; t++) {
        u32 off = t ? 0xC0 : 0x20, n = t ? np : nc;
        for (u32 j = 0; j < 3; j++) {
            u64 *a = pool_alloc((n + PART_TOT) * 8);
            if (!a) { g_hud_dbg[9] = MARK | 5; return 0; }
            memcpy(a, (const void *)q(r, off + j * 8), n * 8);
            for (u32 k = 0; k < PART_TOT; k++)
                a[n + k] = j == 0 ? (u64)part_name(k) : j == 1 ? (u64)(t ? pdk[k] : cdk[k]) : part_hash(k);
            if (!t && j == 1) a[0] = (u64)first;
            *(u64 *)(root + off + j * 8) = (u64)a;
        }
        *(u32 *)(root + off + 24) = n + PART_TOT;
    }
    /* TXT_N text panes (clock, rupees): clones of the stock T_Assign_01 text pane. A text pane is described in
     * three tables: the common pane list (+0x20, desc +0x3c = index in the picture/text list, +0x42 = index
     * in the text list), the picture/text list (+0x40) and the text list (+0x80). */
    {
        int ci = name_index(r, 0x20, "T_ShortCut_00"), bi = name_index(r, 0x40, "T_ShortCut_00"),
            ti = name_index(r, 0x80, "T_ShortCut_00");
        if (ci < 0 || bi < 0 || ti < 0) { g_hud_dbg[9] = MARK | 6; return 0; }
        u32 npane = nc + PART_TOT, nb = w(r, 0x58), nt = w(r, 0x98), np = w(r, 0x78);
        /* picture panes (plates): clones of the stock sub-item backing picture P_SubItemBg_00 */
        int pci = name_index(r, 0x20, "P_SubItemBg_00"), pbi = name_index(r, 0x40, "P_SubItemBg_00"),
            ppi = name_index(r, 0x60, "P_SubItemBg_00");
        if (pci < 0 || pbi < 0 || ppi < 0) { g_hud_dbg[9] = MARK | 6; return 0; }
        int mats[TEX_N];
        u32 src_mat = w((const void *)((const u64 *)q(r, 0x48))[pbi], 0x14);   /* P_SubItemBg_00's textured material */
        for (u32 t = 0; t < TEX_N; t++) {
            mats[t] = add_material(root, r, src_mat, g_texn[t]);
            if (mats[t] < 0) { g_hud_dbg[9] = MARK | 8; return 0; }
        }
        for (u32 k = 0; k < PIC_N; k++) {
            u8 *cd = pool_clone((const void *)((const u64 *)q(r, 0x28))[pci], 68);
            u8 *bd = pool_clone((const void *)((const u64 *)q(r, 0x48))[pbi], 0x88);
            u8 *pd = pool_clone((const void *)((const u64 *)q(r, 0x68))[ppi], 0x88);
            if (!cd || !bd || !pd) { g_hud_dbg[9] = MARK | 4; return 0; }
            *(u32 *)(bd + 0x14) = (u32)mats[pic_tex(k)];
            *(u32 *)(pd + 0x10) = nb + k;
            *(u32 *)(cd + 40) = 0;
            *(u32 *)(cd + 52) = npane + k + 1;
            *(float *)(cd + 0x10) = 0.0f; *(float *)(cd + 0x14) = 0.0f;   /* hidden until placed */
            *(float *)(cd + 28) = HUD_X;
            *(float *)(cd + 32) = HUD_Y_RUPEE;
            *(float *)(cd + 0x2C) = pic_w(k); *(float *)(cd + 0x30) = pic_h(k);
            *(unsigned short *)(cd + 0x3c) = (unsigned short)(nb + k);
            /* like the stock picture: +0x40 = no extra-data entry (0xFFFF; any other value makes the engine search
             * a by-name list at that index on every lookup), +0x42 = index in the picture list */
            *(unsigned short *)(cd + 0x40) = 0xFFFF;
            *(unsigned short *)(cd + 0x42) = (unsigned short)(np + k);
            if (!tbl_add(root, 0x20, g_pinames[k], cd, g_pihash[k]) || !tbl_add(root, 0x40, g_pinames[k], bd, g_pihash[k]) ||
                !tbl_add(root, 0x60, g_pinames[k], pd, g_pihash[k])) { g_hud_dbg[9] = MARK | 5; return 0; }
        }
        npane += PIC_N; nb += PIC_N;
        for (u32 t = 0; t < TXT_N; t++) {
            u8 *cd = pool_clone((const void *)((const u64 *)q(r, 0x28))[ci], 68);
            u8 *bd = pool_clone((const void *)((const u64 *)q(r, 0x48))[bi], 0x88);
            u8 *td = pool_clone((const void *)((const u64 *)q(r, 0x88))[ti], 0xA8);
            if (!cd || !bd || !td) { g_hud_dbg[9] = MARK | 4; return 0; }
            /* the stock text panes use the button-glyph font "External_00": switch to the normal UI font */
            *(u64 *)(td + 0x00) = (u64)g_font_normal;
            *(u64 *)(td + 0x18) = q(td, 0x10);              /* default text: the empty string */
            *(float *)(td + 0x98) = 1500.0f;
            *(float *)(td + 0x08) = 64.0f; *(float *)(td + 0x0C) = 64.0f;
            *(float *)(cd + 0x2C) = 900.0f; *(float *)(cd + 0x30) = 90.0f;
            *(float *)(cd + 0x10) = 0.5f; *(float *)(cd + 0x14) = 0.5f;
            *(u32 *)(cd + 40) = 0;
            *(u32 *)(cd + 52) = npane + t + 1;
            *(float *)(cd + 28) = HUD_X + 40.0f;
            u32 fld = t % FLD_N;
            *(float *)(cd + 32) = fld == 0 ? HUD_Y_CLOCK : (fld == 3 || fld == 6) ? HUD_Y_COLLECT : HUD_Y_RUPEE;
            *(unsigned short *)(cd + 0x3c) = (unsigned short)(nb + t);
            *(unsigned short *)(cd + 0x42) = (unsigned short)(nt + t);
            if (!tbl_add(root, 0x20, g_tnames[t], cd, g_thash[t]) || !tbl_add(root, 0x40, g_tnames[t], bd, g_thash[t]) ||
                !tbl_add(root, 0x80, g_tnames[t], td, g_thash[t])) { g_hud_dbg[9] = MARK | 5; return 0; }
        }
    }
    {   /* per picture/text-list entry table (one int per entry of the +0x40 list; -1 = none): grow it */
        u32 oc = w(r, 0x210);
        const u8 *op = (const u8 *)q(r, 0x208);
        if (op && oc == w(r, 0x58)) {
            int *na = pool_alloc((oc + PIC_N + TXT_N) * 4);
            if (!na) { g_hud_dbg[9] = MARK | 5; return 0; }
            memcpy(na, op, oc * 4);
            for (u32 k = 0; k < PIC_N + TXT_N; k++) na[oc + k] = -1;
            *(u64 *)(root + 0x208) = (u64)na;
            *(u32 *)(root + 0x210) = oc + PIC_N + TXT_N;
        } else g_hud_dbg[9] = MARK | 7;
    }
    if (!group_hud_panes(root, first, nc, PART_TOT, pdk)) {
        g_hud_dbg[9] = MARK | 5;
        return 0;
    }
    g_hud_dbg[6] += 1;
    return root;
}
/* -------- step 2: private copy of the gauge file, flat white tinted -------- */
static float *g_pal_a[SEG_N], *g_pal_b[SEG_N];   /* tint words of each private gauge copy */
static u8 *build_gauge(const u8 *r, u32 k) {
    const u8 *base = r - 0xF8;
    u32 n = w(base, 8);
    if (w(base, 0) != 0x48464642u || n < 0x7500 || n > 0x10000) return 0;
    u8 *out = pool_clone(base, n);
    if (!out) return 0;
    /* the file stores absolute pointers into itself: rebase them */
    for (u32 i = 0; i + 8 <= n; i += 8) {
        u64 v = q(out, i);
        if (v >= (u64)base && v < (u64)base + n) *(u64 *)(out + i) = (u64)out + (v - (u64)base);
    }
    for (u32 i = 0; i + 8 <= n; i += 8) {
        u64 v = q(out, i);
        if (v < (u64)out || v >= (u64)out + n) continue;
        const char *s = (const char *)v;
        if (SWAP_FILL && str_eq(s, STOCK_TEX)) *(u64 *)(out + i) = (u64)FLAT_TEX;
        /* sparkle / flash / particle textures of the gauge become circles: the panes that
         * use them are repositioned as the slot circles (drive_circles) */
        else if (str_eq(s, "MpKiraKira_00") || str_eq(s, "MpKiraKira_01") || str_eq(s, "MpKiraKira_02") ||
                 str_eq(s, "MpGaugeFlash_01") || str_eq(s, "MpGaugeGlowParticle_00") ||
                 str_eq(s, "MpGaugeGlowParticle_01") || str_eq(s, "MpGaugeGlow_00") ||
                 str_eq(s, "MpGaugeGlow_01") || str_eq(s, "Kumo64_00_BC4L") ||
                 str_eq(s, "Half8_00_ASTC4x4") || str_eq(s, "Half8_01_BC4L"))
            *(u64 *)(out + i) = (u64)CIRCLE_TEX;
    }
    /* palette words (verified before touching): our tint instead of green */
    static const float YEL[3] = {1.0f, 0.85f, 0.10f};
    const float *c = k >= 64 ? YEL : g_part_rgb[0];   /* second set (k >= 64 = RING1_N + RING2_N) is yellow */
    float *a = (float *)(out + 0x74D8);
    if (a[0] == 1.0f && a[1] == 1.0f && a[2] == 1.0f && a[3] == 1.0f) { a[0] = c[0]; a[1] = c[1]; a[2] = c[2]; }
    float *b = (float *)(out + 0x74E8);
    if (b[1] > 0.5646f && b[1] < 0.5648f) { b[0] = c[0]; b[1] = c[1]; b[2] = c[2]; b[3] = 1.0f; }
    g_pal_a[k % SEG_N] = a;
    g_pal_b[k % SEG_N] = b;
    *(u64 *)(out + 0x110) = (u64)asset_name(k);
    g_hud_dbg[7] += 1;
    return out + 0xF8;
}

/* cache of replaced resources and the per-call config copies */
static struct { const u8 *src; u8 *copy; u32 kind; } g_res[140];
static u32 g_res_n;
static struct { void *handle; u8 config[0x100]; } g_calls[64];
static u32 g_calls_n;

u64 g_prep_dbg[20] = {1};   /* per part: handle, config, handle+0x28, handle+0x38 */

void *hud_prepare(void *handle, u8 *config) {
    g_hud_dbg[8] += 1;
    const u8 *r = (const u8 *)q(config, 0);
    if (!r || w(r, 0) != 0x004C4955u) return config;   /* "UIL\0" */
    const char *asset = (const char *)q(r, 0x18);
    const char *inst = (const char *)q(handle, 0x30);
    u32 kind = str_eq(asset, "PlayerStatus_00") ? 1 : BTNMAP && str_eq(asset, "PaItemMenu_00") ? 3 :
               BTNMAP && str_eq(asset, "PlusMenu_00") ? 4 :
               g_shbcfg.hud_map_hearts && str_eq(asset, "PaMapCollectNum_00") ? 5 : 0;
    /* one of our slot buttons: its nested Pa_ReplaceIcon_00 is built next and gets the private copy */
    for (u32 k = 0; !kind && k < (u32)PART_TOT; k++)
        if (str_eq(inst, part_name(k))) { g_repicon_armed = 1; return config; }
    if (!kind && g_repicon_armed && str_eq(asset, "PaReplaceIcon_00")) { g_repicon_armed = 0; g_repicon_dbg[1]++; kind = 2; }
    if (!kind) return config;
    u8 *rep = 0;
    u32 i;
    for (i = 0; i < g_res_n; i++)
        if (g_res[i].src == r && g_res[i].kind == kind) { rep = g_res[i].copy; break; }
    if (i == g_res_n) {
        if (g_res_n == 140) return config;
        rep = kind == 1 ? build_root(r) : kind == 3 ? build_menu(r) : kind == 4 ? build_plus(r) :
              kind == 5 ? build_map(r) : build_repicon(r);
        if (!rep) return config;
        g_res[i].src = r; g_res[i].copy = rep; g_res[i].kind = kind;
        g_res_n++;
    }
    if (kind == 2) {
        u32 s = (kind - 2) * 4;
        g_prep_dbg[s] = (u64)handle; g_prep_dbg[s + 1] = (u64)config;
        g_prep_dbg[s + 2] = q(handle, 0x28); g_prep_dbg[s + 3] = q(handle, 0x38);
    }
    /* A fresh config copy per call: the engine may keep the pointer, and the same
     * handle address can be reused for different parts. */
    u8 *cfg = pool_alloc(0x100);
    if (!cfg) return config;
    memcpy(cfg, config, 0x100);   /* the +0x80 field must survive */
    *(u64 *)cfg = (u64)rep;
    (void)g_calls_n; (void)g_calls;
    return cfg;
}

/* -------- step 3: per-frame drive -------- */
static void observe_stock_box(u64 parent);

static u64 g_mp;
static u64 g_frame_node;
static u32 g_ready_at;      /* g_tick after a HUD init at which drawing may resume (the layout settles) */
static float g_value = 50.0f, g_cap = 100.0f, g_show = 1.0f;
static float g_last_value = -1.0f, g_last_cap = -1.0f, g_last_show = -1.0f;
static u32 g_test_mode = 1;
static u32 g_tick;

/* slot parts */
typedef struct {
    u64 part, ratio, max, trace, pane;
    float icon, count, show;            /* wanted */
    float l_show, l_icon;               /* last applied */
    u32 done;
} SlotPart;
static SlotPart g_slot[SLOT_N];

void hud_set_stamina(float value, float cap, float show) {
    g_value = value; g_cap = cap; g_show = show;
    g_test_mode = 0;
}

/* slot i (0..3): icon = ItemPattern frame, count = StockNum (<0 hides the number),
 * show = 0/1 */
void hud_set_slot(u32 i, float icon, float count, float show) {
    if (i >= SLOT_N) return;
    g_slot[i].icon = icon; g_slot[i].count = count; g_slot[i].show = show;
}

static u64 ptr(u64 p, u32 off) { return *(u64 *)(p + off); }

static u64 pane_record(u64 frame, const char *name) {
    u64 layout = ptr(frame, 0x80);
    if (!layout || !vptr(layout)) return 0;
    const u8 *r = (const u8 *)ptr(layout, 0xF8);
    if (!r || !vptr((u64)r)) return 0;
    int i = name_index(r, 0x20, name);
    u64 arr = ptr(layout, 0x100);
    if (i < 0 || !arr || !vptr(arr)) return 0;
    return arr + (u64)i * 0xB0;
}

static void set_pane_show(u64 pane, float scale_y) {
    *(float *)(pane + 0x64) = scale_y;   /* local Y scale: 0 hides */
    *(u32 *)pane |= 0x6F;                /* mark the pane dirty */
}

u64 g_bind_dbg[4] = {MARK, MARK, MARK, MARK};   /* slot 0: part, pat, num, pane */
float g_pos_dbg[8];

/* ---- stamina ring: SEG_N appended gauge parts, each one radial tick of the ring ----
 * Every part is a private gauge copy. Its container pane (N_Gauge_01, index 1) is moved to
 * the part origin and rotated about it, so the bar (which extends left of the container's
 * origin) becomes a spoke at the wanted angle; the fill pane (P_Gauge_01, 17) is scaled
 * to show the segment's share of the value. */
/* octagon: side k is the bar of part k, tangent to a circle of apothem 190 (clockwise from the top).
 * Per side: container position (bar centre = apothem point, bar length 160) and rotation. */
#define RING1_N 30
#define RING2_N 34
#define SET_N 64   /* segments per colour set */
#define RING_SX1 0.1951f
#define RING_SX2 0.2429f
#define BAR_T1 1.6f           /* main ring fill thickness factor (natural height 32) */
#define BAR_T2 1.0f           /* outer ring: thinner */
#define BORDER_W 12.0f       /* dark border, outward only (no black lines inside the ring) */
static const float OCT_X[64] = {58.53f, 82.20f, 102.28f, 117.89f, 128.34f, 133.19f, 132.21f, 125.46f, 113.22f, 96.04f, 74.66f, 50.01f, 23.18f, -4.67f, -32.31f, -58.53f, -82.20f, -102.28f, -117.89f, -128.34f, -133.19f, -132.21f, -125.46f, -113.22f, -96.04f, -74.66f, -50.01f, -23.18f, 4.67f, 32.31f, 72.87f, 106.47f, 136.44f, 161.77f, 181.59f, 195.22f, 202.21f, 202.30f, 195.52f, 182.07f, 162.42f, 137.24f, 107.39f, 73.88f, 37.85f, 0.54f, -36.79f, -72.87f, -106.47f, -136.44f, -161.77f, -181.59f, -195.22f, -202.21f, -202.30f, -195.52f, -182.07f, -162.42f, -137.24f, -107.39f, -73.88f, -37.85f, -0.54f, 36.79f};
static const float OCT_Y[64] = {120.00f, 105.21f, 85.82f, 62.68f, 36.80f, 9.31f, -18.59f, -45.67f, -70.76f, -92.75f, -110.69f, -123.80f, -131.49f, -133.43f, -129.55f, -120.00f, -105.21f, -85.82f, -62.68f, -36.80f, -9.31f, 18.59f, 45.67f, 70.76f, 92.75f, 110.69f, 123.80f, 131.49f, 133.43f, 129.55f, 189.60f, 172.98f, 150.47f, 122.84f, 91.02f, 56.11f, 19.28f, -18.20f, -55.07f, -90.06f, -121.98f, -149.74f, -172.41f, -189.21f, -199.56f, -203.12f, -199.76f, -189.60f, -172.98f, -150.47f, -122.84f, -91.02f, -56.11f, -19.28f, 18.20f, 55.07f, 90.06f, 121.98f, 149.74f, 172.41f, 189.21f, 199.56f, 203.12f, 199.76f};
static const float OCT_R[64] = {0.00000f, -0.20944f, -0.41888f, -0.62832f, -0.83776f, -1.04720f, -1.25664f, -1.46608f, -1.67552f, -1.88496f, -2.09440f, -2.30383f, -2.51327f, -2.72271f, -2.93215f, -3.14159f, -3.35103f, -3.56047f, -3.76991f, -3.97935f, -4.18879f, -4.39823f, -4.60767f, -4.81711f, -5.02655f, -5.23599f, -5.44543f, -5.65487f, -5.86431f, -6.07375f, 0.00000f, -0.18480f, -0.36960f, -0.55440f, -0.73920f, -0.92400f, -1.10880f, -1.29360f, -1.47840f, -1.66320f, -1.84800f, -2.03280f, -2.21759f, -2.40239f, -2.58719f, -2.77199f, -2.95679f, -3.14159f, -3.32639f, -3.51119f, -3.69599f, -3.88079f, -4.06559f, -4.25039f, -4.43519f, -4.61999f, -4.80479f, -4.98959f, -5.17439f, -5.35919f, -5.54399f, -5.72879f, -5.91359f, -6.09839f};
typedef struct { u64 bar, ratio, max, trace, layout_arr, part_pane; u32 init; float l_fill, l_show; } Seg;
static Seg g_seg[SEG_N];
u32 g_seg_ok = 0x10000000u;

u32 g_len[3] = {0x7777, 0x7777, 0x7777};

static void bind_segs(u64 parent) {
    u32 ok = 0;
    for (u32 k = 0; k < SEG_N; k++) {
        Seg *s = &g_seg[k];
        if (s->bar) { ok++; continue; }
        u64 bar = (u64)find_part_safe((void *)parent, part_name(k));
        if (!bar) continue;
        u64 none = (u64)find_anim((void *)bar, "__no_such_anim__");
        u64 ratio = (u64)find_anim((void *)bar, "GaugeRatio");
        u64 max = (u64)find_anim((void *)bar, "GaugeMax");
        u64 trace = (u64)find_anim((void *)bar, "TraceRatio");
        if (!ratio || !max || !trace || ratio == none || max == none || trace == none) continue;
        u64 cl = ptr(bar, 0x80);
        u64 arr = cl ? ptr(cl, 0x100) : 0;
        if (!arr) continue;
        s->bar = bar; s->ratio = ratio; s->max = max; s->trace = trace; s->layout_arr = arr;
        s->part_pane = pane_record(parent, part_name(k));
        s->l_fill = -1.0f; s->l_show = -1.0f;
        g_hud_dbg[2] = MARK | bar;
        ok++;
    }
    g_seg_ok = 0x10000000u + ok;
}

static u32 g_tired;
void hud_set_tired(u32 t) { g_tired = t; }

/* ---- world anchor: project the player (beside his chest) to the screen like the native 3D-anchored UI ----
 * the player: physics actor position f32[3] at actor+0x31C. Camera: view = [[[[CAMMGR]]+0x28]]+0x78 with
 * CAMMGR = main+0x4C86BC0; matrix f32[3][4] row-major at view+0x08 (row0 right, row1 up, row2 back,
 * translation in column 3; the camera looks along -row2). Layout units: 3596 x 2022 for 16:9. */
extern u64 g_player_actor;
static float g_anchor_x, g_anchor_y, g_anchor_s;
static u32 g_anchor_ok;
u32 g_anchor_dbg[6] = {0x10000000u};
#define ANCHOR_TAN_HALF_FOV 0.4663f   /* tan(25 deg): 50 deg vertical FOV */
#define ANCHOR_ASPECT 1.7778f
#define ANCHOR_SIDE 0.54f             /* metres to the camera's right of the player */
#define ANCHOR_HEIGHT 0.55f           /* metres above his feet */
#define ANCHOR_REF_DIST 4.0f          /* camera radius at which the ring has RING_SIZE */

static void anchor_update(void) {
    g_anchor_ok = 0;
    if (g_player_actor == 1) return;
    u64 base = (u64)MAIN_SCHED_VT - 0x4B05318ull;
    u64 slot = *(volatile u64 *)(base + 0x4C86BC0ull);
    if (!slot) return;
    u64 mgr = *(volatile u64 *)slot;
    if (!mgr) return;
    g_anchor_dbg[1] = *(volatile u32 *)(mgr + 0x20);
    if (*(volatile u32 *)(mgr + 0x20) == 0) return;
    u64 cams = *(volatile u64 *)(mgr + 0x28);
    if (!cams) return;
    u64 cam0 = *(volatile u64 *)cams;
    if (!cam0) return;
    u64 view = *(volatile u64 *)(cam0 + 0x78);
    if (!view) return;
    if (*(volatile u64 *)view != base + 0x4BF8A88ull) { g_anchor_dbg[2]++; return; }
    const volatile float *m = (const volatile float *)(view + 0x08);   /* rows at +0, +4, +8 floats (stride 4) */
    const volatile float *pos = (const volatile float *)(g_player_actor + 0x31C);
    float rx = m[0], ry = m[1], rz = m[2];
    float px = pos[0] + rx * ANCHOR_SIDE;
    float py = pos[1] + ANCHOR_HEIGHT + ry * ANCHOR_SIDE;
    float pz = pos[2] + rz * ANCHOR_SIDE;
    float xc = m[0] * px + m[1] * py + m[2] * pz + m[3];
    float yc = m[4] * px + m[5] * py + m[6] * pz + m[7];
    float zc = m[8] * px + m[9] * py + m[10] * pz + m[11];
    float d = -zc;
    g_anchor_dbg[3] = (u32)(int)(d * 100.0f);
    if (d < 0.3f || d > 60.0f) return;
    float nx = xc / (d * ANCHOR_TAN_HALF_FOV * ANCHOR_ASPECT);
    float ny = yc / (d * ANCHOR_TAN_HALF_FOV);
    if (nx < -1.3f || nx > 1.3f || ny < -1.3f || ny > 1.3f) return;
    g_anchor_x = nx * 1798.0f;
    g_anchor_y = ny * 1011.0f;
    float sc = RING_SIZE * (ANCHOR_REF_DIST / d);
    if (sc < 0.12f) sc = 0.12f;
    if (sc > 0.9f) sc = 0.9f;
    g_anchor_s = sc;
    g_anchor_ok = 1;
    g_anchor_dbg[0]++;
    g_anchor_dbg[4] = (u32)(int)(g_anchor_x);
    g_anchor_dbg[5] = (u32)(int)(g_anchor_y);
}
static void drive_segs(void) {
    /* main ring: first 75 stamina (RING1_N segments); outer ring: what the 4th..20th hearts add,
     * 2 segments per heart (RING2_N), only the segments the current hearts unlock are shown. */
    float v1 = g_value < 75.0f ? g_value : 75.0f;
    float lit1_all = v1 / 75.0f * (float)RING1_N;
    float per2 = 75.0f / (float)RING2_N;                       /* stamina per outer segment */
    int avail2 = (int)((g_cap - 75.0f) / per2 + 0.5f);
    if (avail2 < 0) avail2 = 0;
    if (avail2 > RING2_N) avail2 = RING2_N;
    float lit2_all = g_value > 75.0f ? (g_value - 75.0f) / per2 : 0.0f;
    for (u32 k = 0; k < SEG_N; k++) {
        Seg *s = &g_seg[k];
        if (!s->bar) continue;
        if (!s->init) {
            set_frame((void *)s->max, 100.0f);      /* bar width is fixed */
            set_frame((void *)s->ratio, 473.0f);    /* fill pose = sits on the base */
            set_frame((void *)s->trace, 473.0f);
            s->init = 1;
        }
        u32 idx = k % SET_N;
        u32 ring2 = idx >= RING1_N;
        u32 j = ring2 ? idx - RING1_N : idx;
        float lit = (ring2 ? lit2_all : lit1_all) - (float)j;   /* 0..1 = this segment's fill */
        if (lit < 0.0f) lit = 0.0f;
        if (lit > 1.0f) lit = 1.0f;
        int unlocked = !ring2 || (int)j < avail2;
        float show = (g_show > 0.5f && g_anchor_ok && unlocked && ((k >= SET_N) == (g_tired != 0))) ? 1.0f : 0.0f;
        if (s->part_pane && g_anchor_ok) {   /* the whole ring moves with the player */
            u64 pp = s->part_pane;
            *(float *)(pp + 0x60) = g_anchor_s;
            *(float *)(pp + 0x64) = g_anchor_s;
            *(float *)(pp + 0x6C) = g_anchor_x;
            *(float *)(pp + 0x70) = g_anchor_y;
            *(u32 *)pp |= 0x6F;
        }
        u64 arr = s->layout_arr;
        u64 c1 = arr + 1 * 0xB0;     /* container: bar tangent to its circle */
        *(float *)(c1 + 0x60) = show * (ring2 ? RING_SX2 : RING_SX1);
        *(float *)(c1 + 0x64) = show;
        *(float *)(c1 + 0x68) = OCT_R[idx];      /* radians, counter-clockwise positive */
        *(float *)(c1 + 0x6C) = OCT_X[idx];
        *(float *)(c1 + 0x70) = OCT_Y[idx];
        *(u32 *)c1 |= 0x6F;
        u64 f = arr + 17 * 0xB0;     /* fill */
        *(float *)(f + 0x60) = lit * 1.025f;   /* full fill = the base length (1.07 x 160) so no dark gaps at the joints */
        float bt = ring2 ? BAR_T2 : BAR_T1;
        *(float *)(f + 0x64) = bt;
        *(float *)(f + 0x6C) = -380.0f - 5.6f;
        *(float *)(f + 0x70) = 0.0f;
        *(u32 *)f |= 0x6F;
        /* dark base: as thick as the fill plus a border that only sticks out OUTWARD (+y), so it never
         * paints over the neighbouring lit segments inside the ring */
        for (u32 bi = 0; bi < 2; bi++) {
            u64 bp = arr + (u64)(11 + bi) * 0xB0;
            *(float *)(bp + 0x60) = show * 1.07f;
            *(float *)(bp + 0x64) = show * (bt * 32.0f + BORDER_W) / 32.0f;
            *(float *)(bp + 0x6C) = -380.0f - 5.6f;
            *(float *)(bp + 0x70) = BORDER_W * 0.5f;
            *(u32 *)bp |= 0x6F;
        }
        static const u32 HIDE[1] = {8};   /* gauge shadow window */
        u64 h = arr + (u64)HIDE[0] * 0xB0;
        *(float *)(h + 0x60) = 0.0f;
        *(float *)(h + 0x64) = 0.0f;
        *(u32 *)h |= 0x6F;
    }
}

/* ---- clock / rupee plates ---- */
u64 g_txt_dbg[8] = {MARK, MARK, MARK, MARK, MARK, MARK, MARK, MARK};
static char g_tbuf[TXT_N][48];      /* ASCII: the engine's text setter takes 8-bit text + byte length */
static char g_tlast[FLD_N][48];
static u32 g_tfront[FLD_N], g_tpend[FLD_N], g_twait[FLD_N];
/* Setting the text of a pane leaves it blank for a frame. Each field has two panes: the new text goes to the
 * hidden one, and after it has been drawn a few frames the two swap (shown = Y scale > 0, hidden = 0). */
static void tint_rec(u64 rec, float r, float g, float b, float a) {
    u64 obj = *(u64 *)(rec + 8);   /* pane object: RGBA tint as four floats at +0x20 */
    if (obj < 0x10000) return;
    float *c = (float *)(obj + 0x20);
    c[0] = r; c[1] = g; c[2] = b; c[3] = a;
}
static void place(u64 rec, float x, float y, float sx, float sy, float rot) {
    *(float *)(rec + 0x60) = sx; *(float *)(rec + 0x64) = sy; *(float *)(rec + 0x68) = rot;
    *(float *)(rec + 0x6C) = x; *(float *)(rec + 0x70) = y;
    *(u32 *)rec |= 0x6F;   /* mark the pane dirty */
}
/* field f: shown text at (x, y) in colour rgb; on = 0 hides both panes */
float g_fld_sc[FLD_N] = {0.0f, 0.0f, 0.0f, 0.70f, 0.80f, 0.80f, 0.70f};   /* text scale per field (0 = the HUD text scale) */
static void put_text(u64 frame, u32 f, const char *ascii, float x, float y, u32 on, const float *rgb) {
    u64 rf = pane_record(frame, g_tnames[f + FLD_N * g_tfront[f]]);
    u64 rb = pane_record(frame, g_tnames[f + FLD_N * (g_tfront[f] ^ 1)]);
    g_txt_dbg[f] = MARK | rf;
    if (!rf || !rb) return;
    float ts = g_fld_sc[f] > 0.0f ? g_fld_sc[f] : g_hudp[13];
    tint_rec(rf, rgb[0], rgb[1], rgb[2], 1.0f);
    tint_rec(rb, rgb[0], rgb[1], rgb[2], 1.0f);
    if (g_tpend[f] && ++g_twait[f] >= 4) {   /* the hidden pane has drawn its new text: swap */
        u64 t = rf; rf = rb; rb = t;
        g_tfront[f] ^= 1; g_tpend[f] = 0;
    }
    x = on ? x - (action_field(f) ? ACTIONS_X : counters_x()) : 0.0f;
    place(rf, x, y, ts, on ? ts : 0.0f, 0.0f);
    place(rb, x, y, ts, 0.0f, 0.0f);
    if (g_tpend[f]) return;
    u32 n = 0;
    while (ascii[n] && n < 47) n++;
    int same = 1;
    for (u32 i = 0; i <= n; i++) if (g_tlast[f][i] != ascii[i]) same = 0;
    if (same) return;
    char *dst = g_tbuf[f + FLD_N * (g_tfront[f] ^ 1)];
    for (u32 i = 0; i < n; i++) dst[i] = ascii[i];
    dst[n] = 0;
    g_text_ref.s = dst; g_text_ref.len = n; g_text_ref.z = 0; g_text_ref.m1 = -1;
    set_text((void *)rb, &g_text_ref, 0);
    g_tpend[f] = 1; g_twait[f] = 0;
    for (u32 i = 0; i <= n; i++) g_tlast[f][i] = ascii[i];
    g_txt_dbg[3 + f] += 1;
}
u32 g_clk_dbg[4];   /* 0 GameData seconds, 1 world-object seconds */
static int world_seconds(void) {
    /* GameData World_CurrentDateTime (UInt64 table k = 24, record 5, 0x30 bytes each, value at +0x20):
     * day count in the high 32 bits, seconds of the day in the low 32 bits. */
    int gd = -1, wo = -1;
    u64 mgr = *(u64 *)GAMEDATA_MGR;
    if (vptr(mgr)) {
        u32 cnt = *(u32 *)(mgr + 8 + 0x60 * 24 + 8);
        u64 rec = *(u64 *)(mgr + 8 + 0x60 * 24 + 0x10);
        if (rec && cnt >= 6) gd = (int)(*(u64 *)(rec + 5 * 0x30 + 0x20) & 0xFFFFFFFFu);
    }
    /* the world clock object (same value, read through the game's own accessor) */
    u64 wm = *(u64 *)WORLD_MGR;
    u64 obj = wm ? *(u64 *)(wm + 0x578) : 0;
    if (obj) {
        typedef int (*fn_t)(u64);
        fn_t fn = (fn_t) * (u64 *)(*(u64 *)obj + 0x30);
        wo = fn(obj);
    }
    g_clk_dbg[0] = (u32)gd; g_clk_dbg[1] = (u32)wo;
    return wo >= 0 ? wo : gd;
}
static u32 put_int(char *p, int v) {
    char d[8]; u32 nd = 0, k = 0;
    if (v < 0) v = 0;
    if (v > 9999) v = 9999;
    for (;; v /= 10) { d[nd++] = (char)('0' + v % 10); if (v < 10) break; }
    while (nd) p[k++] = d[--nd];
    return k;
}
/* sun rays: unit directions every 45 degrees, clockwise from the top */
static const float RAY_DX[8] = {0.0f, 0.70711f, 1.0f, 0.70711f, 0.0f, -0.70711f, -1.0f, -0.70711f};
static const float RAY_DY[8] = {1.0f, 0.70711f, 0.0f, -0.70711f, -1.0f, -0.70711f, 0.0f, 0.70711f};
u32 g_hud_show[2] = {0x10000000u, 0x10000000u};   /* clock, rupees: 1 = drawn this frame */
static u64 g_pic_rec[PIC_N];
/* Simple layout's bar (plates == 2), tunable live with tools/hud_tune.py: 0 = x shift, 1 = extra width on the
 * right (negative shortens), 2 = alpha, 3 = height. */
float g_bg_tune[4] = {-165.0f, 10.0f, 0.42f, 1.05f};
static u32 g_plates_shown;
static void bar_reset(void);
/* the layout was rebuilt: drop cached pane records and force the texts to be set again */
static void plates_reset(void) {
    for (u32 k = 0; k < PIC_N; k++) g_pic_rec[k] = 0;
    bar_reset();
    for (u32 f = 0; f < FLD_N; f++) { g_tlast[f][0] = (char)0xFF; g_tpend[f] = 0; g_tfront[f] = 0; }
    g_plates_shown = 0;
}
static void plates_hide(u64 frame) {
    for (u32 k = 0; k < PIC_N; k++) {
        u64 rec = g_pic_rec[k];
        if (rec) place(rec, *(float *)(rec + 0x6C), g_hudp[2], 0.0f, 0.0f, 0.0f);
    }
    for (u32 t = 0; t < TXT_N; t++) {
        u64 rec = pane_record(frame, g_tnames[t]);
        if (rec) place(rec, *(float *)(rec + 0x6C), g_hudp[2], 0.0f, 0.0f, 0.0f);
    }
}
/* ---- GameData lookups by key hash ----
 * Record indices are NOT stable: a romfs mod that ships its own GameDataList (Paraglider adds 3 Bools and 1
 * Int) shifts every record after its additions, so an index generated from the stock list reads the wrong
 * flag (found live: Sequence_IsChildhood, IsGet.*, the collectible flags). Each table has the game's own
 * open-addressing map (table + 0x18: {u32 hash, u32 index} slots, table + 0x24: slot count; the game probes it
 * from hash % slots, linearly, at main+0x5890DC); looking keys up there gives the live index. */
#define GD_NONE 0xFFFFFFFFu
static u32 gd_find(u32 k, u32 h) {
    u64 mgr = *(u64 *)GAMEDATA_MGR;
    if (!vptr(mgr) || !h) return GD_NONE;
    u64 tab = mgr + 8 + 0x60ull * k;
    u64 map = *(volatile u64 *)(tab + 0x18);
    u32 slots = *(volatile u32 *)(tab + 0x24), cnt = *(volatile u32 *)(tab + 8);
    if (!vptr(map) || !slots) return GD_NONE;
    u32 i = h % slots;
    for (u32 n = 0; n < slots; n++) {
        u32 e = *(volatile u32 *)(map + (u64)i * 8);
        if (e == h) { u32 ix = *(volatile u32 *)(map + (u64)i * 8 + 4); return ix < cnt ? ix : GD_NONE; }
        if (!e) return GD_NONE;
        i = i + 1 >= slots ? 0 : i + 1;
    }
    return GD_NONE;
}
/* ---- per-area collectibles ----
 * Sequence_CurrentBanc is String64 record 31 (same layout as the equipment slots): the live string is at
 * +0x74 of a 0x108-byte record. The generated table has the Bool-table indexes of the Gold
 * Skulltula flags and of the HeartPieceAnchor_GameDataPath flags for every Banc.
 *
 * The Gold Skulltula *flags* are runtime only: the game sets IsGoldSkullDead_<actor> when the
 * token drops and starts every area load with them false, so counting them alone shows 0 after
 * leaving and coming back. The durable record is the save's "Game" entry (Bool64bitKey, hash
 * 0xA3DB7114): a sorted list of 64-bit keys with one key per collected Skulltula. Its live copy
 * is the loaded progress.sav image, which the manager keeps at +0xE80 (an array of 0x80-byte
 * save descriptors); the entry is "<header u64><ascending keys><0>" at the offset the save index
 * gives for that hash. We also remember pickups seen this session so a fresh token still counts
 * before the image catches up.
 *
 * That buffer is not progress.sav's own: the game loads and writes every save file through it
 * (option.sav, slot_NN/caption.sav, account.sav), so a few seconds after the save loads it holds a small
 * file without the "Game" entry. The keys are therefore copied out whenever the image has them, and the
 * copy is kept while the buffer holds another file (found live: the counter read 0 after that). */
#define GAME_KEY_HASH 0xa3db7114u
#define KSET_MAX 4096u
static u64 g_kset_copy[KSET_MAX];   /* the save's key set, as last seen in the image */
static u64 g_kset_arr;      /* first key (g_kset_copy once a key set was seen) */
static u32 g_kset_n;        /* keys in the copy */
static u32 g_kset_refreshes;
static u32 g_kset_age;      /* frames since the key set was last read */
static u64 g_kset_buf;      /* save image the cache was built from */
static u32 g_kset_doff;     /* its data offset when last read (another file in the buffer changes it) */
static u64 g_picked[128];   /* keys collected this session */
static u32 g_picked_n;

static int kset_has(u64 h) {
    if (!g_kset_arr || !h) return 0;
    u32 lo = 0, hi = g_kset_n;
    while (lo < hi) {
        u32 mid = (lo + hi) >> 1;
        u64 v = *(volatile u64 *)(g_kset_arr + (u64)mid * 8ull);
        if (v == h) return 1;
        if (v < h) lo = mid + 1; else hi = mid;
    }
    return 0;
}
static void picked_add(u64 h) {
    if (!h) return;
    for (u32 i = 0; i < g_picked_n; i++) if (g_picked[i] == h) return;
    if (g_picked_n < (u32)(sizeof g_picked / sizeof g_picked[0])) g_picked[g_picked_n++] = h;
}
static int picked_has(u64 h) {
    if (!h) return 0;
    for (u32 i = 0; i < g_picked_n; i++) if (g_picked[i] == h) return 1;
    return 0;
}
static void kset_refresh(void) {
    u64 mgr = *(volatile u64 *)GAMEDATA_MGR;
    if (!vptr(mgr)) return;
    u64 buf = 0;
    u32 seen = 0;
    for (u32 i = 0; i < 8; i++) {                    /* save descriptor array at +0xE00 */
        u64 p = *(volatile u64 *)(mgr + 0xe00ull + (u64)i * 0x80ull);
        if (!vptr(p)) continue;                      /* stale/freed slot: the manager array can hold garbage */
        u32 magic = *(volatile u32 *)p, d = *(volatile u32 *)(p + 8);
        if (magic == 0x01020304u && d > 0x20u && d < 0x100000u) { seen++; if (!buf) buf = p; }
    }
    g_kset_dbg[5] = MARK + seen;
    if (!buf) return;
    /* The buffer is re-read when it holds another file (its data offset changes) and now and then:
     * the game loads and writes every save file through it, and progress.sav may sit there only briefly. */
    u32 doff_now = *(volatile u32 *)(buf + 8);
    if (buf == g_kset_buf && doff_now == g_kset_doff && ++g_kset_age < 45u) return;
    g_kset_doff = doff_now;
    g_kset_age = 0;
    g_kset_buf = buf;
    g_kset_dbg[0] = MARK + buf;
    g_kset_dbg[4] = MARK + ++g_kset_refreshes;
    u32 data_off = *(volatile u32 *)(buf + 8), np = (data_off - 0x20u) / 8u;
    u32 off = 0;
    for (u32 i = 0; i < np; i++) {
        if (*(volatile u32 *)(buf + 0x20ull + (u64)i * 8ull) != GAME_KEY_HASH) continue;
        off = *(volatile u32 *)(buf + 0x24ull + (u64)i * 8ull);
        break;
    }
    g_kset_dbg[1] = MARK + off;
    /* not progress.sav right now (another save file in the buffer): keep the copy */
    if (off < data_off || off > data_off + 0x200000u) return;
    const unsigned char *p = (const unsigned char *)buf + (u64)off + 8ull;
    u64 prev = 0;
    u32 n = 0;
    for (; n < KSET_MAX; n++) {
        u64 v = *(volatile u64 *)(p + (u64)n * 8ull);
        if (!v || v <= prev) break;
        prev = v;
        g_kset_copy[n] = v;
    }
    if (n < g_kset_n) g_picked_n = 0;                /* another save (fewer keys): drop this session's list */
    g_kset_arr = (u64)g_kset_copy; g_kset_n = n;
    g_kset_dbg[2] = MARK + n;
}
static u32 collect_stage(char *out, u32 cap) {
    u64 mgr = *(u64 *)GAMEDATA_MGR;
    if (!vptr(mgr)) return 0;
    u64 tab = mgr + 8 + 0x60 * 16;                 /* String64 table */
    u32 cnt = *(u32 *)(tab + 8);
    u64 rec = *(u64 *)(tab + 0x10);
    u32 ix = gd_find(16, 0x1D6189DAu);              /* Sequence_CurrentBanc (index 31 in the stock list) */
    if (!vptr(rec) || ix >= cnt) return 0;
    const char *s = (const char *)(rec + (u64)ix * 0x108ull + 0x74ull);
    u32 n = 0;
    while (s[n] && n + 1 < cap) { out[n] = s[n]; n++; }
    out[n] = 0;
    return n;
}
/* a collectible's Bool by key hash (0: none) */
static u32 collect_bool(u64 recs, u32 count, u32 key) {
    u32 idx = key ? gd_find(0, key) : GD_NONE;
    if (!vptr(recs) || idx >= count) return 0;
    return *(volatile unsigned char *)(recs + (u64)idx * 0x18ull + 0x16ull) != 0;
}
static int collect_counts(u32 *sd, u32 *st, u32 *hd, u32 *ht) {
    *sd = *st = *hd = *ht = 0;
    char stage[40];
    if (!collect_stage(stage, sizeof stage)) return 0;
    /* some area loads use "Folder/Area" (HyruleField/HyruleField_Child): compare the area part only */
    const char *name = stage;
    for (u32 i = 0; stage[i]; i++) if (stage[i] == '/') name = stage + i + 1;
    kset_refresh();
    for (u32 i = 0; i < COLLECT_STAGES; i++) {
        const unsigned char *r = g_collect_table + (u64)i * 48ull;
        int hit = 0;
        for (u32 j = 0; j < 40; j++) {
            if (r[j] != (unsigned char)name[j]) break;
            if (!r[j]) { hit = 1; break; }
        }
        if (!hit) continue;
        u32 so = (u32)r[40] | ((u32)r[41] << 8), sn = r[42];
        u32 ho = (u32)r[44] | ((u32)r[45] << 8), hn = r[46];
        u64 mgr = *(u64 *)GAMEDATA_MGR;
        if (!vptr(mgr)) return 0;
        u32 cnt = *(u32 *)(mgr + 8 + 8);           /* Bool table */
        u64 recs = *(u64 *)(mgr + 8 + 0x10);
        for (u32 k = 0; k < sn && so + k < COLLECT_FLAGS; k++) {
            u64 h = g_collect_hash[so + k];
            u32 live = collect_bool(recs, cnt, g_collect_flags[so + k]);
            if (live) picked_add(h);
            *st += 1;
            *sd += (live || kset_has(h) || picked_has(h)) ? 1u : 0u;
        }
        /* heart pieces: a reward's Bool (live, persistent), or a world piece's actor hash in the key set
         * (flag 0: none). A world piece picked this session counts once the game saves. */
        for (u32 k = 0; k < hn && ho + k < COLLECT_FLAGS; k++) {
            u64 h = g_collect_hash[ho + k];
            *ht += 1;
            *hd += (collect_bool(recs, cnt, g_collect_flags[ho + k]) || kset_has(h)) ? 1u : 0u;
        }
        g_kset_dbg[3] = MARK + ((u64)*sd << 32) + *st;
        return (*st || *ht) ? 1 : 0;
    }
    return 0;
}
/* the wallet: Int keys 0xA77921D7 (rupees) and 0xE0010A50 (wallet size), indices 962 / 976 in the stock list */
static void hud_rupee_values(int *cur, int *mx) {
    *cur = 0; *mx = 200;
    u64 mgr = *(u64 *)GAMEDATA_MGR;
    if (vptr(mgr)) {
        u64 rec = *(u64 *)(mgr + 8 + 0x60 * 2 + 0x10);
        u32 ic = gd_find(2, 0xA77921D7u), im = gd_find(2, 0xE0010A50u);
        if (vptr(rec) && ic != GD_NONE) *cur = *(int *)(rec + (u64)ic * 0x28 + 0x1C);
        if (vptr(rec) && im != GD_NONE) *mx = *(int *)(rec + (u64)im * 0x28 + 0x1C);
    }
    if (*mx <= 0) *mx = 200;
}
static u32 hud_clock_text(char *b, int sec) {
    int h = sec / 3600, m = (sec / 60) % 60, hh = h;
    if (!g_shbcfg.clock_24h) { hh = h % 12; if (!hh) hh = 12; }
    u32 n = 0;
    if (g_shbcfg.clock_24h || hh >= 10) b[n++] = (char)('0' + hh / 10);
    b[n++] = (char)('0' + hh % 10);
    b[n++] = ':'; b[n++] = (char)('0' + m / 10); b[n++] = (char)('0' + m % 10);
    if (!g_shbcfg.clock_24h) { b[n++] = ' '; b[n++] = h < 12 ? 'A' : 'P'; b[n++] = 'M'; }
    b[n] = 0;
    return n;
}
static void plates_update(u64 frame) {
    for (u32 k = 0; k < PIC_N; k++)
        if (!g_pic_rec[k]) g_pic_rec[k] = pane_record(frame, g_pinames[k]);
    for (u32 k = 0; k < PIC_N; k++) if (!g_pic_rec[k]) return;
    u32 active = player_active() != 0;
    static u32 g_last_hm = 0xFFFFFFFFu;
    u32 hm = g_shbcfg.hud_mode;
    if (hm != g_last_hm) { if (g_plates_shown) plates_hide(frame); g_last_hm = hm; }
    if (!active) {
        g_hud_show[0] = g_hud_show[1] = 0x10000000u;
        if (g_plates_shown) { plates_hide(frame); g_plates_shown = 0; }
        return;
    }
    u32 clk = g_shbcfg.hud_clock, rup = g_shbcfg.hud_rupees;
    u32 sd = 0, st = 0, hd = 0, ht = 0;
    u32 col = g_shbcfg.hud_collectibles ? (u32)collect_counts(&sd, &st, &hd, &ht) : 0;
    (void)col;
    /* outside gameplay (loading, menus, cutscenes) nothing is touched except hiding the plates once:
     * setting texts there costs the engine a slow lookup per call */
    if (!clk && !rup && !col) {
        g_hud_show[0] = g_hud_show[1] = 0x10000000u;
        if (g_plates_shown) { plates_hide(frame); g_plates_shown = 0; }
        return;
    }
    g_plates_shown = 1;
    const float *P = g_hudp;
    u32 nf = g_shbcfg.hud_flags;
    float psx = nf && g_shbcfg.hud_sx ? g_shbcfg.hud_sx / 100.0f : P[3];
    float psy = nf && g_shbcfg.hud_sy ? g_shbcfg.hud_sy / 100.0f : P[4];
    float tsc = nf && g_shbcfg.hud_tscale ? g_shbcfg.hud_tscale / 100.0f : P[13];
    float isc = nf && g_shbcfg.hud_iscale ? g_shbcfg.hud_iscale / 100.0f : P[6];
    float ioff = nf ? (float)g_shbcfg.hud_ioff : P[5];
    float ioff2 = nf ? (float)g_shbcfg.hud_ioff2 : P[5];
    float txo = nf ? (float)g_shbcfg.hud_tx : P[7];
    float BY = nf && g_shbcfg.hud_y ? (float)g_shbcfg.hud_y : -960.0f;
    u32 plates = nf ? g_shbcfg.hud_plates : 1;
    float adv = P[9] * (tsc / P[13]);
    g_fld_sc[0] = g_fld_sc[1] = g_fld_sc[2] = tsc; g_fld_sc[3] = tsc; g_fld_sc[6] = tsc;
    int sec = clk ? world_seconds() : -1;
    if (sec < 0) clk = 0;
    g_hud_show[0] = 0x10000000u + clk; g_hud_show[1] = 0x10000000u + rup;
    /* Block positions come from the installer: the enabled blocks are packed left to right without holes. */
    int bxr = g_shbcfg.hud_x[0], bxc = g_shbcfg.hud_x[1], bxs = g_shbcfg.hud_x[2], bxh = g_shbcfg.hud_x[3];
    u32 show_r = rup && bxr != -32768, show_c = clk && bxc != -32768;
    u32 show_s = active && g_shbcfg.hud_collectibles && bxs != -32768;
    u32 show_h = active && g_shbcfg.hud_collectibles && bxh != -32768;
    const float counter_origin = counters_x();
    if (plates == 2) {   /* simple: one soft translucent bar behind the row of counters */
        int xl = bxr != -32768 ? bxr : bxc;
        int xr = bxh != -32768 ? bxh : (bxs != -32768 ? bxs : bxc);
        float x0 = (float)xl - 233.0f + g_bg_tune[0], x1 = (float)xr + 213.0f + g_bg_tune[0] + g_bg_tune[1];
        place(g_pic_rec[PIC_BG], (x0 + x1) * 0.5f - counter_origin, BY, (x1 - x0) / 100.0f, g_bg_tune[3], 0.0f);
        tint_rec(g_pic_rec[PIC_BG], 0.0f, 0.0f, 0.0f, g_bg_tune[2]);
    } else {
        place(g_pic_rec[PIC_BG], 0.0f, 0.0f, 0.0f, 0.0f, 0.0f);
    }
    if (plates == 1 && show_r) {
        float cx = (float)bxr;
        for (u32 j = 0; j < 4; j++) {
            u64 rec = g_pic_rec[4 + j];
            float sx = psx * ((j & 1) ? -1.0f : 1.0f);
            place(rec, cx - counter_origin, BY, sx, psy, 0.0f);
            if (j < 2) tint_rec(rec, 0.0f, 0.0f, 0.0f, P[10]);
            else tint_rec(rec, P[15], P[15], P[15], P[11]);
        }
    }
    if (plates == 1 && show_c) for (u32 j = 0; j < 4; j++) {
        u64 rec = g_pic_rec[j];
        float sx = psx * ((j & 1) ? -1.0f : 1.0f);
        place(rec, (float)bxc - counter_origin, BY, sx, psy, 0.0f);
        if (j < 2) tint_rec(rec, 0.0f, 0.0f, 0.0f, P[10]);
        else tint_rec(rec, P[15], P[15], P[15], P[11]);
    }
    if (plates == 1) for (u32 sec2 = 0; sec2 < 2; sec2++) {
        int bi = sec2 ? bxh : bxs;
        if (bi == -32768) continue;
        for (u32 j = 0; j < 4; j++) {
            u64 rec = g_pic_rec[PIC_COLL + sec2 * 4 + j];
            float sx = 0.42f * ((j & 1) ? -1.0f : 1.0f);
            place(rec, (float)bi - counter_origin, BY, sx, psy, 0.0f);
            if (j < 2) tint_rec(rec, 0.0f, 0.0f, 0.0f, P[10]);
            else tint_rec(rec, P[15], P[15], P[15], P[11]);
        }
    }
    /* collectible icons (the plates are only in wide mode) */
    float cisc = isc * (plates == 1 ? 1.0f : 1.30f);   /* the skull / heart art has more padding: it reads small */
    for (u32 sec2 = 0; sec2 < 2; sec2++) {
        int bi = sec2 ? bxh : bxs;
        if (bi == -32768 || !(sec2 ? show_h : show_s)) continue;
        u64 icn = g_pic_rec[PIC_COLL + 8 + sec2];
        place(icn, (float)bi + ioff2 - counter_origin, BY, cisc, cisc, 0.0f);
        tint_rec(icn, 1.0f, 1.0f, 1.0f, 1.0f);
    }
    if (show_r) {
        place(g_pic_rec[8], (float)bxr + ioff - counter_origin, BY, isc, isc, 0.0f);
        tint_rec(g_pic_rec[8], 1.0f, 1.0f, 1.0f, 1.0f);
    }
    if (sec < 0) sec = 0;
    sec %= 86400;
    if (plates || g_shbcfg.hud_mode >= 1) {   /* wide plate, or the simple / plain row (no plate) */
        u32 day = sec >= 6 * 3600 && sec < 18 * 3600;
        if (P[16] > 0.5f) day = P[16] < 1.5f;
        u32 sun = show_c && day, moon = show_c && !day;
        float ix = show_c ? (float)bxc + ioff2 : counter_origin, iy = BY;
        float ssc = plates == 1 ? 1.0f : isc;
        float ray = plates == 1 ? P[12] : P[12] * (isc / P[6]);
        float msc = plates == 1 ? P[17] : isc;
        float ssh = plates == 1 ? P[18] : isc * 0.85f;
        float mof = plates == 1 ? P[14] : P[14] * (isc / P[6]);
        static const float SUN[3] = {1.0f, 0.86f, 0.42f};
        place(g_pic_rec[9], ix - counter_origin, iy, sun ? ssc : 0.0f, sun ? ssc : 0.0f, 0.0f);
        tint_rec(g_pic_rec[9], SUN[0], SUN[1], SUN[2], 1.0f);
        for (u32 j = 0; j < 8; j++) {
            u64 rec = g_pic_rec[10 + j];
            place(rec, ix + RAY_DX[j] * ray - counter_origin, iy + RAY_DY[j] * ray, sun ? ssc : 0.0f, sun ? ssc : 0.0f,
                  -0.785398f * (float)j);
            tint_rec(rec, SUN[0], SUN[1], SUN[2], 1.0f);
        }
        float ms = moon ? msc : 0.0f, ss = moon ? ssh : 0.0f;
        place(g_pic_rec[18], ix - counter_origin, iy, ms, ms, 0.0f);
        tint_rec(g_pic_rec[18], 0.97f, 0.93f, 0.72f, 1.0f);
        place(g_pic_rec[19], ix + mof - counter_origin, iy + mof * 0.6f, ss, ss, 0.0f);
        tint_rec(g_pic_rec[19], P[19], P[19], P[19], 1.0f);
    }

    static const float WHITE[3] = {1.0f, 1.0f, 1.0f};
    static const float GREEN[3] = {0.38f, 0.95f, 0.25f};
    /* text panes are centre-aligned: P[7] is the centre of the text area (right of the icon) */
    float ty = P[8];
    {   /* clock */
        char b[16];
        hud_clock_text(b, sec);
        put_text(frame, 0, b, (float)bxc + txo, BY + ty, show_c, WHITE);
    }
    {   /* rupees: current / wallet capacity */
        int cur, mx;
        hud_rupee_values(&cur, &mx);
        char a[16], b[16];
        u32 na = put_int(a, cur), nb = 0;
        a[na] = 0;
        b[nb++] = '/';
        nb += put_int(b + nb, mx);
        b[nb] = 0;
        /* "1234/5000" centred on tx as a whole: green part and white part side by side (fixed digit advance;
        * ("/" is narrower than a digit: it counts as SLASH_W digits) */
        const float SLASH_W = 0.55f;
        float wa = (float)na, wb = (float)(nb - 1) + SLASH_W;
        float tx = (float)bxr + txo;
        float x0 = tx - adv * (wa + wb) * 0.5f;
        put_text(frame, 1, a, x0 + adv * wa * 0.5f, BY + ty, show_r, GREEN);
        put_text(frame, 2, b, x0 + adv * (wa + wb * 0.5f), BY + ty, show_r, WHITE);
    }
    {   /* per-area Skulltulas / heart pieces: collected / total, one section each */
        char b[16]; u32 n;
        n = put_int(b, (int)sd); b[n++] = '/'; n += put_int(b + n, (int)st); b[n] = 0;
        put_text(frame, 3, b, (float)bxs + txo, BY + ty, show_s, WHITE);
        n = put_int(b, (int)hd); b[n++] = '/'; n += put_int(b + n, (int)ht); b[n] = 0;
        put_text(frame, 6, b, (float)bxh + txo, BY + ty, show_h, WHITE);
    }
}
static void txt_update(u64 frame) {
    plates_update(frame);
}

/* ---- stamina ring: a continuous tube of overlapping Circle64_00 dots ----
 * Ring 1 = the first 75 stamina; ring 2 (outside) = what the extra hearts add, shown only as far as the
 * current capacity unlocks it. Behind each fill a slightly bigger dark track shows the empty part and gives
 * the tube a border. The fill starts at the top and runs clockwise; its last dot sits exactly at the fill
 * angle so the end moves smoothly. Sizes are in ring units, multiplied by the anchor scale (same units as
 * the old gauge ring). Tunable live (tools/hud_tune.py ring ...):
 *  0 ring 1 radius, 1 ring 2 radius, 2 ring 1 thickness, 3 ring 2 thickness, 4 track border,
 *  5 track alpha, 6 track brightness, 7 fill alpha */
float g_ringp[8] = {133.5f, 203.0f, 48.0f, 30.0f, 12.0f, 1.0f, 0.0f, 1.0f};
static float g_rc1[RING1_DOTS + 1], g_rs1[RING1_DOTS + 1], g_rc2[RING2_DOTS + 1], g_rs2[RING2_DOTS + 1];
static u32 g_ring_tab;
/* sin/cos of a small angle (|x| < 0.2) */
static void small_sincos(float x, float *s, float *c) {
    float x2 = x * x;
    *s = x * (1.0f - x2 / 6.0f * (1.0f - x2 / 20.0f));
    *c = 1.0f - x2 / 2.0f * (1.0f - x2 / 12.0f);
}
/* unit directions of dot k, clockwise from the top: (sin, cos) of k * 2pi / n */
static void ring_table(float *sn, float *cs, u32 n) {
    float ds, dc, s = 0.0f, c = 1.0f;
    small_sincos(6.2831853f / (float)n, &ds, &dc);
    for (u32 k = 0; k <= n; k++) {
        sn[k] = s; cs[k] = c;
        float s2 = s * dc + c * ds, c2 = c * dc - s * ds;
        s = s2; c = c2;
    }
}
/* one ring: track dots for the unlocked part, fill dots up to `fill` (both fractions of the full circle) */
static void ring_draw(u32 t0, u32 f0, u32 n, const float *sn, const float *cs, float r, float thick,
                      float unlocked, float fill, u32 on, const float *rgb) {
    const float *G = g_ringp;
    float cx = g_anchor_x, cy = g_anchor_y, sc = g_anchor_s;
    float R = r * sc, dt = (thick + 2.0f * G[4]) * sc / 100.0f, df = thick * sc / 100.0f;
    float ut = unlocked * (float)n, uf = fill * (float)n;
    for (u32 k = 0; k < n; k++) {
        u64 rec = g_pic_rec[t0 + k];
        u32 vis = on && (float)k <= ut + 0.001f && unlocked > 0.0f;
        place(rec, cx + R * sn[k], cy + R * cs[k], vis ? dt : 0.0f, vis ? dt : 0.0f, 0.0f);
        tint_rec(rec, G[6], G[6], G[6], G[5]);
    }
    for (u32 k = 0; k < n; k++) {
        u64 rec = g_pic_rec[f0 + k];
        u32 vis = on && fill > 0.0f && (float)k <= uf;
        place(rec, cx + R * sn[k], cy + R * cs[k], vis ? df : 0.0f, vis ? df : 0.0f, 0.0f);
        tint_rec(rec, rgb[0], rgb[1], rgb[2], G[7]);
    }
    {   /* end dot exactly at the fill angle: dot k rotated by the remaining fraction of a step */
        u32 k = (u32)uf;
        if (k > n) k = n;
        float fr = uf - (float)k, ds, dc;
        small_sincos(fr * 6.2831853f / (float)n, &ds, &dc);
        float s = sn[k] * dc + cs[k] * ds, c = cs[k] * dc - sn[k] * ds;
        u64 rec = g_pic_rec[f0 + n];
        u32 vis = on && fill > 0.0f;
        place(rec, cx + R * s, cy + R * c, vis ? df : 0.0f, vis ? df : 0.0f, 0.0f);
        tint_rec(rec, rgb[0], rgb[1], rgb[2], G[7]);
    }
}
static void ring_update(void) {
    for (u32 k = RT1; k < PIC_N; k++) if (!g_pic_rec[k]) return;   /* bound by plates_update */
    if (!g_ring_tab) {
        ring_table(g_rs1, g_rc1, RING1_DOTS);
        ring_table(g_rs2, g_rc2, RING2_DOTS);
        g_ring_tab = 1;
    }
    static const float GREEN[3] = {0.30f, 0.85f, 0.25f}, YELLOW[3] = {1.0f, 0.85f, 0.10f};
    const float *rgb = g_tired ? YELLOW : GREEN;
    u32 on = g_show > 0.5f && g_anchor_ok && !g_shbcfg.stamina_bar;
    /* ring 1: 0..75 stamina; ring 2: 75..150, unlocked up to the capacity */
    float f1 = g_value / 75.0f, f2 = (g_value - 75.0f) / 75.0f, u2 = (g_cap - 75.0f) / 75.0f;
    if (f1 > 1.0f) f1 = 1.0f;
    if (f1 < 0.0f) f1 = 0.0f;
    if (u2 > 1.0f) u2 = 1.0f;
    if (u2 < 0.0f) u2 = 0.0f;
    if (f2 > u2) f2 = u2;
    if (f2 < 0.0f) f2 = 0.0f;
    const float *G = g_ringp;
    ring_draw(RT1, RF1, RING1_DOTS, g_rs1, g_rc1, G[0], G[2], 1.0f, f1, on, rgb);
    ring_draw(RT2, RF2, RING2_DOTS, g_rs2, g_rc2, G[1], G[3], u2, f2, on, rgb);
}

/* ---- stamina bar, directly under the magic gauge ----
 * One flat pane per segment over a clean dark strip; when stamina goes down whole segments disappear and only
 * the boundary segment shrinks, so nothing is squashed. Yellow normally, red while recharging from empty.
 * Position and size default to the magic gauge's row (768 x 32 layout units, left edge aligned with it).
 *  0 centre x, 1 top y, 2 width, 3 height (layout units; live tuning with tools/hud_tune.py) */
float g_barp[4] = {-1362.0f, 772.0f, 768.0f, 32.0f};
#define BAR_UNIT 100.0f
/* the D-pad guide cross (boots / masks / Navi / ocarina, the game's own part) moves down to make room */
static u64 g_cross_rec;
static float g_cross_base, g_cross_last;
static u32 g_cross_applied;
static void bar_reset(void) { g_cross_rec = 0; g_cross_base = g_cross_last = 0.0f; g_cross_applied = 0; }
static void cross_shift(u64 frame, u32 on, float dy) {
    if (!on || dy <= 0.0f) {
        if (g_cross_applied && g_cross_rec) {
            *(float *)(g_cross_rec + 0x70) = g_cross_base;
            *(u32 *)g_cross_rec |= 0x6F;
            g_cross_applied = 0;
            g_cross_last = g_cross_base;
        }
        return;
    }
    if (!g_cross_rec) {
        g_cross_rec = pane_record(frame, "Pa_GuideCross_00");
        if (!g_cross_rec) return;
        g_cross_base = *(float *)(g_cross_rec + 0x70);
        g_cross_last = g_cross_base;
    }
    float y = *(float *)(g_cross_rec + 0x70);
    if (!g_cross_applied || y != g_cross_last) {   /* the game rewrote it: take that as the new base */
        g_cross_base = y;
        g_cross_applied = 1;
    }
    float ny = g_cross_base - dy;                  /* layout Y grows up: subtract to move down */
    if (y != ny) {
        *(float *)(g_cross_rec + 0x70) = ny;
        *(u32 *)g_cross_rec |= 0x6F;
    }
    g_cross_last = ny;
}
static void bar_update(u64 frame) {
    for (u32 k = PIC_BAR; k < PIC_N; k++) if (!g_pic_rec[k]) return;
    const float *B = g_barp;
    u32 on = g_shbcfg.stamina_bar && g_shbcfg.stamina_enabled && player_active();
    float full = B[2];
    float h = B[3];
    /* capacity grows with the hearts (75 at 3 hearts, 150 at 20): the bar grows to the right from half to the
     * full width, and the number of visible segments follows it (16 at full capacity, like the magic gauge) */
    float grow = g_cap / 150.0f;
    if (grow < 0.5f) grow = 0.5f;
    if (grow > 1.0f) grow = 1.0f;
    float w = full * grow;
    u32 n = (u32)((float)BAR_SEG * grow + 0.5f);
    if (n < 1) n = 1;
    if (n > BAR_SEG) n = BAR_SEG;
    float left = B[0] - full * 0.5f;
    float cx = left + w * 0.5f;
    float cy = B[1] - h * 0.5f;
    /* fixed look: yellow normally, red while recharging from empty (linear values) */
    static const float YELLOW[3] = {1.000f, 0.552f, 0.000f}, RED[3] = {1.000f, 0.027f, 0.027f};
    const float *fillrgb = g_tired ? RED : YELLOW;
    /* clean dark strip + one flat pane per segment: when stamina goes down whole segments disappear and only
     * the boundary segment shrinks, so no texture is ever squashed */
    u64 out = g_pic_rec[PIC_BAR + BAR_OUT];
    place(out, cx, cy, on ? (w + 4.0f) / BAR_UNIT : 0.0f, on ? (h + 4.0f) / BAR_UNIT : 0.0f, 0.0f);
    tint_rec(out, 0.004f, 0.004f, 0.006f, 0.90f);
    u64 base = g_pic_rec[PIC_BAR + BAR_BASE];
    place(base, cx, cy, on ? w / BAR_UNIT : 0.0f, on ? h / BAR_UNIT : 0.0f, 0.0f);
    tint_rec(base, 0.010f, 0.011f, 0.015f, 0.88f);
    float f = g_cap > 0.0f ? g_value / g_cap : 0.0f;
    if (f < 0.0f) f = 0.0f;
    if (f > 1.0f) f = 1.0f;
    float gap = 4.0f;
    float sw = (w - gap * (float)(n - 1)) / (float)n;
    if (sw < 1.0f) sw = 1.0f;
    float lit = f * (float)n;
    for (u32 i = 0; i < BAR_SEG; i++) {
        u64 rec = g_pic_rec[PIC_BAR + BAR_SEG0 + i];
        float part = i < n ? lit - (float)i : 0.0f;
        if (part > 1.0f) part = 1.0f;
        if (part < 0.0f) part = 0.0f;
        float fw = sw * part;
        u32 show = on && fw > 0.5f;
        place(rec, left + (sw + gap) * (float)i + fw * 0.5f, cy,
              show ? fw / BAR_UNIT : 0.0f, show ? h / BAR_UNIT : 0.0f, 0.0f);
        if (show) tint_rec(rec, fillrgb[0], fillrgb[1], fillrgb[2], 1.0f);
    }
    /* the game's D-pad guide cross always moves down while the bar is on screen */
    cross_shift(frame, on, 60.0f);
}

/* ---- buttonMapping: slot item buttons (appended PaItemBtn_00 parts) ----
 * Phase 1 test: bind the parts, place them, show a fixed icon per button through the stock ItemPattern animation. */
#define SA_N 11
static const char *slot_anim_name(u32 i) {
    switch (i) {
    case 0: return "ItemPattern"; case 1: return "StockNum"; case 2: return "NumMax"; case 3: return "NumMin";
    case 4: return "IsInvalid"; case 5: return "Type"; case 6: return "ItemChange"; case 7: return "Select";
    case 8: return "UnSelect"; case 9: return "Decide"; default: return "Assign";
    }
}
typedef struct { u64 part, anim[SA_N], rec, rep, ctrl, uctl, hide[9], ic, sh, num_rec; u32 icon_set, shown_name; int shown_item; int num_val, num_item, num_idx; char forced[32]; char num_txt[8]; } SlotBtn;
static SlotBtn g_sb[PART_N];
u64 g_sb_dbg[PART_N];       /* part object per button (diagnostic) */
int g_sb_stock[PART_N];     /* quantity read for each HUD button (diagnostic; -1 = no quantity) */
u32 g_sb_anim[PART_N];      /* bit i = anim i found */
u64 g_sb_rif[PART_N][2];    /* nested Pa_ReplaceIcon_00 frame, its layout (diagnostic) */
/* item per button (the game's item index, as passed to the equip call main+0x1211C00); -1 = empty */
/* parts 0..3: the buttons (our textures, drawn by the icon pane: full colour), parts 4..7: the items on them.
 * (The icon's shadow pane cannot carry the button: it draws only the texture's shape in a fixed shadow tone.) */
enum { SB_L, SB_X, SB_R, SB_Y, SB_A, SB_ZR, SB_B, SB_NBTN };
/* Game option: Jump and B swapped (System > Options). The mod moves the B item
 * slot to the X button: input, HUD position and legend all follow it. */
u32 g_btn_swap;
int hud_button_swap(void) { return g_btn_swap; }
int g_sb_item[PART_N] = {-1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1};   /* set by hud_slot_item() */
/* features/button_mapping publishes its slots here: slot 0 B, 1 L, 2 R, 3 ZR (the item the game has equipped) */
void hud_slot_item(u32 slot, int item) {
    static const u8 PART_OF[3] = {11, 7, 8};
    if (slot < 3) g_sb_item[PART_OF[slot]] = item;   /* ZR (slot 3) mirrors the game's own item box: zr_mirror() */
}
char g_sb_name_dbg[PART_N][32];   /* actor name resolved for each button (diagnostic) */
/* texture name forced on a part (first byte <= 1 = none): the button backgrounds (tools/make_buttons.py) */
char g_sb_force[PART_N][32] = {"OotBtnS_L", "OotBtnS_Blank", "OotBtnS_R", "OotBtnS_Y", "OotBtnS_A", "OotBtnS_ZR",
                               "OotBtnS_B", {1}, {1}, "Weapon_Sword_Kokiri", {1}, {1},
                               "OotBtnS_Blank", {1}};   /* 12: shield button, 13: the equipped shield */   /* Y: the sword (TODO: equipped one) */
/* button texture behind each slot's icon (UI/Tex/IconSh/<name>, tools/make_buttons.py) */
char g_sb_btn[PART_N][24] = {{0}};
/* ItemPattern = how the icon of each item is framed in the button (scale / offset); frame 0 draws it oversized.
 * Measured on the stock item box (docs/RE-NOTES.md, "Item index -> ItemPattern icon frame"); others use 6. */
static float item_pattern(int item) {
    switch (item) {
    case 0: return 0.0f;  case 1: return 6.0f;  case 2: return 4.0f;  case 3: return 3.0f;  case 4: return 1.0f;
    case 5: return 33.0f; case 6: return 34.0f; case 7: return 32.0f; case 9: return 5.0f;  case 10: return 8.0f;
    case 13: return 2.0f; case 14: case 15: case 16: case 17: case 50: return 23.0f;   /* bottle cells A..E */
    case 18: return 36.0f; case 30: return 24.0f; case 31: return 9.0f;
    default: return 6.0f;
    }
}
/* the game's own item index -> actor name (also the icon texture name in UI/Tex/Icon) */
static const char *item_name(int item) {
    u64 slot = *(u64 *)ITEM_MGR_SLOT;
    u64 mgr = slot ? *(u64 *)slot : 0;
    static const char *nm;
    nm = *(const char **)ITEM_NAME_DEFAULT;
    if (!mgr || item < 0) return 0;
    item_actor_name((void *)mgr, &nm, (u64)item);
    return nm && nm[0] ? nm : 0;
}
/* ---- bottles ----
 * The five bottle cells are item indices 14, 15, 16, 17 and 50 (grid order BottleA..BottleE). Their content is an
 * Enum in GameData (Shop_SelectBottleContentItem: Empty, RedPotion, GreenPotion, BluePotion, BlueFire, Fish,
 * Letter, BigPoe, Poe, Bug, Fairy, MaxMilk, HalfMilk); records 60..64 are BottleA..BottleE in that order. The bottle
 * item itself resolves to the current bottle (e.g. RedPotion), and the game's own hidden box icon is stale for the
 * 2nd..5th bottle, so the HUD reads the bottle's content record (or the box's BottleA..E Enum value) for its icon. */
static const u32 BOTTLE_VAL[5] = {0x22C7238Cu, 0xAC889FD5u, 0xA7D94611u, 0xB55EE4E3u, 0xD8CC3008u};   /* BottleA..E */
static const int BOTTLE_CELL[5] = {14, 15, 16, 17, 50};
static char g_bicon[32];
static u32 enum_value(u32 idx) {
    u64 mgr = *(u64 *)GAMEDATA_MGR;
    if (!vptr(mgr)) return 0;
    u32 cnt = *(volatile u32 *)(mgr + 8 + 0x60 * 6 + 8);
    u64 rec = *(volatile u64 *)(mgr + 8 + 0x60 * 6 + 0x10);
    if (!rec || idx >= cnt) return 0;
    return *(volatile u32 *)(rec + (u64)idx * 0x38 + 0x1C);
}
static const char *bottle_suffix(u32 v) {
    switch (v) {
    case 3938082919u: return "RedPotion";
    case 77412289u:   return "GreenPotion";
    case 4085503717u: return "BluePotion";
    case 472462615u:  return "BlueFire";
    case 511921912u:  return "Fish";
    case 254076936u:  return "Letter";
    case 3452048497u: return "BigPoe";
    case 34099102u:   return "Poe";
    case 1960193739u: return "Bug";
    case 1867113303u: return "Fairy";
    case 1932879911u: return "Milk";       /* MaxMilk */
    case 4009157808u: return "HalfMilk";
    default:          return 0;             /* Empty */
    }
}
static const char *bottle_icon_rec(u32 rec) {
    const char *s = bottle_suffix(enum_value(rec));
    if (!s) return "Item_Bottle";
    static const char P[] = "Item_Bottle_";
    u32 n = 0, j = 0;
    while (P[n]) { g_bicon[n] = P[n]; n++; }
    while (s[j] && n < 31) g_bicon[n++] = s[j++];
    g_bicon[n] = 0;
    return g_bicon;
}
static int bottle_pos(int cell) {
    for (u32 i = 0; i < 5; i++) if (cell == BOTTLE_CELL[i]) return (int)i;
    return -1;
}
static const char *bottle_icon(int cell) {
    int p = bottle_pos(cell);
    return p >= 0 ? bottle_icon_rec(60u + (u32)p) : 0;
}
static int bottle_box_pos(u32 v) {
    for (u32 i = 0; i < 5; i++) if (v == BOTTLE_VAL[i]) return (int)i;
    return -1;
}
/* around the stock item box (top right): above, left, below, right-above; layout units, root-relative */
/* buttonMapping default arrangement, top right: top row L X R, middle row (Y) (A) ZR, bottom (B) */
/* Gamepad diamond (measured on the default theme): X top, Y left, A right, B bottom around a centre,
 * L / R at the top corners, ZR to the right. Items sit up-left of their button so the button shows below. */
float g_sb_pos[PART_N][2] = {{1020.4f, 872.3f}, {1292.6f, 843.0f}, {1545.2f, 872.3f},     /* L X R */
                             {1129.6f, 680.0f}, {1455.6f, 680.0f}, {1677.2f, 680.0f},     /* Y A ZR */
                             {1292.6f, 517.0f},                                            /* B */
                             {995.1f, 897.6f}, {1519.9f, 897.6f}, {1104.3f, 705.3f}, {1651.9f, 705.3f}, {1267.3f, 542.3f},
                             {908.0f, 680.0f}, {882.7f, 705.3f}};                          /* shield button, shield */
float g_sb_scale = 0.62f;
/* Placement of a whole set around the centre of the X / Y / A / B diamond: {spread (distance from the centre),
 * size (icons and labels), dx, dy} in layout units. g_sb_lay: the HUD; g_mb_lay: the pause menu's Items page. */
#define SB_CX 1292.6f
#define SB_CY 680.0f
float g_sb_lay[4] = {0.75f, 0.85f, 130.0f, 70.0f};     /* compact, top right */
float g_mb_lay[4] = {0.55f, 0.6f, -740.0f, -90.0f};   /* between the item grid and the item preview, a little up-right */
/* Simple layout: only B / L / R / ZR. L, R and ZR keep their usual places and B moves to the centre of the
 * group, closer to them (no holes where X / Y / A were). Same coordinate space as g_sb_pos; the item of each
 * button keeps its up-left offset. */
static const float g_sb_simple[PART_N][2] = {
    {1120.4f, 872.3f}, {0.0f, 0.0f}, {1445.2f, 872.3f}, {0.0f, 0.0f}, {0.0f, 0.0f},
    {1677.2f, 680.0f}, {1292.6f, 680.0f},
    {1095.1f, 897.6f}, {1419.9f, 897.6f}, {0.0f, 0.0f}, {1651.9f, 705.3f}, {1267.3f, 705.3f},
    {0.0f, 0.0f}, {0.0f, 0.0f}};   /* (no shield in the simple layout) */
/* Switch GameCube face-button centers. Shoulder locations use the normal layout. */
static const float g_sb_gamecube[SB_NBTN][2] = {
    [SB_A] = {SB_CX, SB_CY},
    [SB_B] = {SB_CX - 148.0f, SB_CY - 68.0f},
    [SB_X] = {SB_CX + 142.0f, SB_CY + 30.0f},
    [SB_Y] = {SB_CX - 38.0f, SB_CY + 133.0f},
};
/* Pad layout (ShbConfig.bm_layout): the place of each face button on the
 * diamond. The game still sees the same logical buttons; only where we draw them moves. 0 Nintendo (A right,
 * B bottom, X top, Y left), 1 A/B swapped (A bottom, B right), 2 Xbox (A/B and X/Y swapped: X left, Y top). */
static u32 face_place(u32 k) {
    u32 l = g_shbcfg.bm_layout;
    if (l >= 1 && l != 3 && (k == SB_A || k == SB_B)) return k == SB_A ? SB_B : SB_A;
    if (l == 2 && (k == SB_X || k == SB_Y)) return k == SB_X ? SB_Y : SB_X;
    return k;
}
/* Button remap (core/remap.h): each part is a role (the part's button in the classic layout), drawn on the physical
 * button that holds that role. Parts 0..6 are the buttons, 7..11 the items on them. The classic layout is the
 * identity, except the game's Jump/B swap, which puts item 1 (part B) on X and jump (part X) on B. */
static const u8 PART_ROLE[PART_N] = {ROLE_ITEM2, ROLE_JUMP, ROLE_ITEM3, ROLE_SWORD, ROLE_ACTION, ROLE_ITEM_MAIN,
                                     ROLE_ITEM1, ROLE_ITEM2, ROLE_ITEM3, ROLE_SWORD, ROLE_ITEM_MAIN, ROLE_ITEM1,
                                     ROLE_SHIELD, ROLE_SHIELD};
#define SB_SHIELD 12   /* the shield button part (13: the shield on it) */
static const u8 PART_HOME[PART_N] = {SB_L, SB_X, SB_R, SB_Y, SB_A, SB_ZR, SB_B, SB_L, SB_R, SB_Y, SB_ZR, SB_B,
                                     SB_SHIELD, SB_SHIELD};
/* a button part (the button art), not an item drawn on one */
static int part_is_button(u32 k) { return k < SB_NBTN || k == SB_SHIELD; }
/* The shield button parts exist (PART_TOT) only with the button remap, the manual shield and a button (not a stick)
 * holding the Shield role: the config does not change while the game runs, so the layout is built once with them. */
int hud_shield_parts(void) {
    return remap_on() && g_shbcfg.manual_shield && remap_role_phys(ROLE_SHIELD) < PB_L3;
}
static u32 part_phys(u32 k) { return remap_role_phys(PART_ROLE[k % PART_N]); }
/* ZL has no part of its own in the classic layout: its place mirrors ZR's on the left of the diamond */
float g_pb_zl[2] = {908.0f, 680.0f};
/* button texture of a physical button (PB_ order; the style letter at [6] is replaced when it is applied) */
static const char PB_TEX[PB_N][14] = {"OotBtnS_L", "OotBtnS_X", "OotBtnS_R", "OotBtnS_Y", "OotBtnS_A", "OotBtnS_ZR",
                                      "OotBtnS_B", "OotBtnS_ZL", "OotBtnS_Blank", "OotBtnS_Blank"};   /* sticks: no art */
static const float *sb_button_pos(u32 button) {
    if (g_shbcfg.bm_layout == 3 &&
        (button == SB_A || button == SB_B || button == SB_X || button == SB_Y)) {
        return g_sb_gamecube[button];
    }
    return button == PB_ZL ? g_pb_zl : g_sb_pos[face_place(button)];
}
static void sb_xy(const float *lay, u32 k, int simple, float *x, float *y) {
    float bx = g_sb_pos[k][0], by = g_sb_pos[k][1];
    if (simple) {
        bx = g_sb_simple[k][0]; by = g_sb_simple[k][1];
    } else {
        /* the button a part sits on (items keep their offset from their button part), then the pad layout moves
         * that button (face buttons only) */
        u32 home = PART_HOME[k], on = part_phys(k);
        if (on >= PB_L3) on = home;   /* (no button, or a stick: the part is hidden anyway) */
        const float *position = sb_button_pos(on);
        bx = position[0] + (g_sb_pos[k][0] - g_sb_pos[home][0]);
        by = position[1] + (g_sb_pos[k][1] - g_sb_pos[home][1]);
    }
    *x = SB_CX + (bx - SB_CX) * lay[0] + lay[2];
    *y = SB_CY + (by - SB_CY) * lay[0] + lay[3];
}
u32 g_sb_big_items = 1;
/* per-part scale: buttons, then items (the item is drawn bigger than its button) */
float g_sb_sc[PART_N] = {0.34f, 0.34f, 0.34f, 0.34f, 0.34f, 0.34f, 0.34f, 0.38f, 0.38f, 0.38f, 0.38f, 0.38f,
                         0.34f, 0.38f};
u32 g_sb_hide_stock = 1;   /* (only while the item buttons are on) */   /* hide the game's own item box and its page dots: our buttons replace them */
static u64 g_stock_box_rec, g_stock_dots_rec;
/* animation frame forced on every button each frame, per animation (-1 = leave it); see slot_anim_name() */
float g_sb_animf[SA_N] = {-1.0f, -1.0f, -1.0f, -1.0f, -1.0f, -1.0f, -1.0f, -1.0f, -1.0f, -1.0f, -1.0f};
static volatile u32 g_sb_busy;
/* pane record of a pane inside a part's own layout */
static u64 part_child(u64 part, const char *name) {
    u64 cl = ptr(part, 0x80);
    const u8 *r = cl ? (const u8 *)ptr(cl, 0xF8) : 0;
    if (!r) return 0;
    int i = name_index(r, 0x20, name);
    return i < 0 ? 0 : ptr(cl, 0x100) + (u64)i * 0xB0;
}
static const char *hide_name(u32 i) {   /* the item box's own frame, glyph and count: not used by our buttons */
    switch (i) {
    case 0: return "P_HUDBg_00"; case 1: return "P_HUDDeco_01"; case 2: return "P_HUDBgSh_00";
    case 3: return "T_Assign_00"; case 4: return "P_ShortcutBg_01"; case 5: return "P_ShortcutDeco_02";
    case 6: return "P_ShortcutBgSh_01"; case 7: return "P_CursorGlowDecide_00"; default: return "N_Num_00";
    }
}
/* Item icons: the game's own icon setup (main+0x33B7DA0, used for the Agony Stone icon) is called with a small
 * stand-in "widget" whose +0x20 is our button part. It finds the part's Pa_ReplaceIcon_00, allocates and inits an
 * icon controller (main+0xA37320, hook 11 captures it) whose +0x30 / +0x38 are the icon / shadow texture
 * replacers; main+0xA73920 then loads UI/Tex/Icon/<name> and UI/Tex/IconSh/<name>_Sh into them. */
static u64 g_sb_widget[PART_N][12], g_mb_widget[PART_N][12];   /* stand-in widgets (the init writes +0x30..+0x48) */
static volatile u32 g_ctrl_capture;
static volatile u64 g_ctrl_captured;
u64 g_sb_ctrl_dbg[PART_N];
void hud_icon_ctrl_init(u64 ctrl) { if (g_ctrl_capture) g_ctrl_captured = ctrl; }
u64 g_hud_heap;   /* heap passed to the PlayerStatus HUD init (main+0x33F5D20, hook 12) */
static u64 g_sb_ctrl[PART_N];
u32 g_sb_ctrl_max = PART_N;
/* Hook 12 (entry of the PlayerStatus HUD init, x0 HUD object, x1 heap): the layout and our parts exist, and the
 * heap still accepts allocations (it does not afterwards), so the icon controllers are built here. */
u64 g_hud_obj;     /* the PlayerStatus HUD object (hook 12) */
static void hud_slots_init(u64 hud, u64 heap) {
    g_hud_heap = heap;
    g_hud_obj = hud;
    u64 frame = hud ? ptr(hud, 0x28) : 0;
    for (u32 k = 0; k < PART_N; k++) {
        g_sb_ctrl[k] = 0;
        if (k >= g_sb_ctrl_max || k >= (u32)PART_TOT) continue;   /* bisect: how many controllers the HUD init accepts */
        u64 p = frame ? (u64)find_part_safe((void *)frame, part_name(k)) : 0;
        if (!p || !heap) continue;
        for (u32 i = 0; i < 12; i++) g_sb_widget[k][i] = 0;
        g_sb_widget[k][4] = p;                       /* widget+0x20 = our button part */
        g_ctrl_captured = 0; g_ctrl_capture = 1; g_sb_busy = 1;
        icon_widget_init(g_sb_widget[k], (void *)heap);
        g_sb_busy = 0; g_ctrl_capture = 0;
        g_sb_ctrl[k] = g_ctrl_captured;
        g_sb_ctrl_dbg[k] = MARK | g_sb_ctrl[k];
    }
}
static void set_icon_btn(SlotBtn *b, const char *name, const char *btn);
static void set_icon(SlotBtn *b, const char *name) { set_icon_btn(b, name, 0); }
static void set_icon_btn(SlotBtn *b, const char *name, const char *btn) {
    if (!b->ctrl) return;
    u64 r1 = ptr(b->ctrl, 0x30), r2 = ptr(b->ctrl, 0x38);
    static char g_tex_sh[40];
    static const char *g_tex_a, *g_tex_c;
    u32 n = 0;
    while (name[n] && n < 32) { g_tex_sh[n] = name[n]; n++; }
    g_tex_sh[n] = '_'; g_tex_sh[n + 1] = 'S'; g_tex_sh[n + 2] = 'h'; g_tex_sh[n + 3] = 0;
    g_tex_a = name; g_tex_c = btn ? btn : g_tex_sh;
    g_sb_busy = 1;
    if (r1) tex_replace((void *)r1, &g_tex_a, 0);
    if (r2) tex_replace((void *)r2, &g_tex_c, 0);
    g_sb_busy = 0;
}
/* ZR shows what the game has in its own (hidden) item box: the name its icon replacer last loaded
 * ([[[[our part + 0x28] + 0x150] + 0x1F0] + 0x38], e.g. "Item_DekuNut"), copied into the ZR item part's
 * fixed texture. hud_zr_item() turns it back into the item index (features/button_mapping re-equips it). */
char g_zr_name[32];
static int g_zr_index = -1;
int hud_zr_item(void) { return g_zr_index; }
static int g_zr_hold;
void hud_zr_hold(int hold) { g_zr_hold = hold; }   /* features/button_mapping: a slot item borrows the item box */
static int box_item(void);
static u32 box_value(void);
static void zr_mirror(void) {
    u64 part = g_sb[0].part;   /* any of our bound parts: [part + 0x28] is the PlayerStatus frame it lives in */
    u64 fr = vptr(part) ? ptr(part, 0x28) : 0;
    u64 ctl = vptr(fr) ? ptr(fr, 0x150) : 0;
    u64 rp = vptr(ctl) ? ptr(ctl, 0x1F0) : 0;
    const char *nm = vptr(rp) ? (const char *)ptr(rp, 0x38) : 0;
    if (nm && !vptr((u64)nm)) nm = 0;
    /* the box from GameData (PlayerStatus.AssignableItem.<age>); the HUD's own icon name is the fallback */
    int bi = box_item();
    if (bi == -1) {   /* empty */
        g_zr_name[0] = 0; g_zr_index = -1;
        if (!g_zr_hold) g_sb_force[10][0] = 1;   /* hidden */
        return;
    }
    /* a bottle in the box: its own Enum value names the bottle (BottleA..E), the content is in the bottle record */
    int bp = bottle_box_pos(box_value());
    const char *disp = bp >= 0 ? bottle_icon_rec(60u + (u32)bp) : 0;
    if (bi >= 0) nm = item_name(bi);
    if (!disp && (!nm || (u64)nm < 0x10000 || (nm[0] != 'I' && nm[0] != 'W' && nm[0] != 'A'))) return;   /* Item_/Weapon_/Armor_ */
    /* 1. always track the box: name and item index */
    if (bp >= 0) {
        if (g_zr_index != BOTTLE_CELL[bp] || !str_eq(g_zr_name, disp)) {
            u32 i;
            for (i = 0; i < 30 && disp[i]; i++) g_zr_name[i] = disp[i];
            g_zr_name[i] = 0;
            g_zr_index = BOTTLE_CELL[bp];
        }
    } else if (nm && !str_eq(g_zr_name, nm)) {
        u32 i;
        for (i = 0; i < 30 && nm[i]; i++) g_zr_name[i] = nm[i];
        g_zr_name[i] = 0;
        g_zr_index = -1;
        for (int it = 0; it < 64; it++) {
            const char *n2 = item_name(it);
            if (n2 && str_eq(n2, g_zr_name)) { g_zr_index = it; break; }
        }
    }
    /* 2. show it on ZR, unless a slot item is borrowing the box right now */
    const char *show = disp ? disp : g_zr_name;
    char *dst = g_sb_force[10];
    if (g_zr_hold || str_eq(dst, show)) return;
    u32 i;
    for (i = 0; i < 30 && show[i]; i++) dst[i] = show[i];
    dst[i] = 0;
    dst[31] = 0;   /* apply again */
}
/* PlayerStatus frame our parts live in. Its u32 at +0x1A4 has bit 0x20000 set while the game is played and
 * clear while a menu covers it (pause ...), measured stable across pause / resume: then nothing of ours is touched
 * and our buttons do not act. */
static u64 g_ps_frame;
int hud_ingame(void) { return !vptr(g_ps_frame) || (*(volatile u32 *)(g_ps_frame + 0x1A4) & 0x20000u) != 0; }
/* hud_ingame() really reads the HUD (our parts are bound); 0: it always answers "in game" */
int hud_ingame_known(void) { return vptr(g_ps_frame) != 0; }
/* features/button_mapping keeps this: + was pressed in gameplay and gameplay has not run again yet */
u32 g_game_paused;
/* A menu covers the game (pause, map ...): what the mod injects or remaps must stay out of it. The HUD flag alone
 * is not steady through the whole pause menu, so the + tracking counts too. */
int hud_game_covered(void) { return g_game_paused || (hud_ingame_known() && !hud_ingame()); }
/* Corner minimap (MapMenu_00): the layout is captured at its part instantiation (hook id 4) and its frame is
 * layout + 0xB0 (the PlayerStatus layout uses the same field). While playing, the scale of every map node is zeroed;
 * the map screen (hud_ingame() == 0) keeps them. */
static u64 g_map_frame, g_mm_frame;
static u64 g_map_layout;
u64 g_minimap_dbg[3];   /* 0: the captured MapMenu frame, 1/2: two of its pane records */
/* The nodes that make up the corner map: the paper map, its caps, the player arrow and map icons, the frame and the
 * world-map thumbnail. Arrays, not pointer tables (the module has no relocations). */
#define MM_N 6
static const char g_mm_names[MM_N][20] = {"N_MiniMap_00", "N_MiniMapCap_00", "N_Common_00",
                                          "N_MapIcon_00", "P_MiniMapFrame_00", "N_WorldMap_00"};
static u64 g_mm_rec[MM_N];
static void hide_minimap(void) {
    if (g_map_layout) {
        u64 f = ptr(g_map_layout, 0xB0);
        if (f && vptr(f) && ptr(f, 0x80) == g_map_layout) g_map_frame = f;
    }
    g_minimap_dbg[0] = g_map_frame;
    if (!g_shbcfg.hide_minimap || !g_map_frame || !hud_ingame()) return;
    if (g_mm_frame != g_map_frame) {
        g_mm_frame = g_map_frame;
        for (u32 i = 0; i < MM_N; i++) g_mm_rec[i] = pane_record(g_map_frame, g_mm_names[i]);
        g_minimap_dbg[1] = g_mm_rec[0]; g_minimap_dbg[2] = g_mm_rec[1];
    }
    for (u32 i = 0; i < MM_N; i++) if (g_mm_rec[i]) {
        *(float *)(g_mm_rec[i] + 0x60) = 0.0f; *(float *)(g_mm_rec[i] + 0x64) = 0.0f;
        *(u32 *)g_mm_rec[i] |= 0x6F;
    }
}
/* Pause menu, Items tab: its caption panel (hooked at main+0x2CBD20) has byte +0x28 == 2 only while that tab is
 * open, and the highlighted cell's item index at +0x90 (our item numbering). The cell is owned when its
 * pattern frame ([[entry+0x48]+0x20]+0x18, entry = the panel's list +0x78 / count +0x70 whose +0xE8 is the
 * cell) is not 0: the game's own test before it shows the caption. */
extern u64 g_menu_panel, g_menu_cell;
static char g_map_buf[64];   /* Minus map: native total text + the area heart count */
u64 g_map_set_frame;         /* the frame of the native counter when its total text was set */
u64 g_map_icon_dbg[4];       /* diagnostic: MapMenu root, the added heart pane, the skull part, icon x */
u64 player_get(void);
int hud_menu_items(void) {   /* (the panel is built at boot: before a game is loaded it says nothing) */
    return g_menu_panel && player_get() && !hud_ingame() && *(volatile u8 *)(g_menu_panel + 0x28) == 2;
}
/* the highlighted cell's widget (main+0x43E500's: +0x30 Decide, +0x38 Failure, +0x48 State animations) */
static u64 menu_cell_widget(void) {
    if (!hud_menu_items()) return 0;
    u64 p = g_menu_panel;
    int n = *(volatile int *)(p + 0x70);
    u64 list = *(volatile u64 *)(p + 0x78);
    if (n <= 0 || n > 256 || !list) return 0;
    for (int i = 0; i < n; i++) {
        u64 e = *(volatile u64 *)(list + (u64)i * 8);
        if (e && *(volatile u64 *)(e + 0xE8) == g_menu_cell) return e;
    }
    return 0;
}
/* ---- GameData Bool flags (table 0: records of 0x18 bytes in GameDataList order, live value at +0x16) ---- */
static int gd_bool(u32 idx) {
    u64 mgr = *(u64 *)GAMEDATA_MGR;
    if (!vptr(mgr)) return -1;
    u32 cnt = *(volatile u32 *)(mgr + 8 + 8);
    u64 rec = *(volatile u64 *)(mgr + 8 + 0x10);
    if (!vptr(rec) || idx >= cnt) return -1;
    return *(volatile u8 *)(rec + (u64)idx * 0x18 + 0x16) != 0;
}
/* ---- GameData Int values (table 2: records of 0x28 bytes, live value at +0x1C, see the devtool's gdlive.py) ---- */
static int gd_int(u32 idx) {
    u64 mgr = *(u64 *)GAMEDATA_MGR;
    if (!vptr(mgr)) return -1;
    u32 cnt = *(volatile u32 *)(mgr + 8 + 0x60 * 2 + 8);
    u64 rec = *(volatile u64 *)(mgr + 8 + 0x60 * 2 + 0x10);
    if (!vptr(rec) || idx >= cnt) return -1;
    return *(volatile int *)(rec + (u64)idx * 0x28 + 0x1C);
}
/* GameData option: System > Options "Jump and B swapped" (OptionData struct
 * 0x834d9d9e, Bool member 0x3B0FC63F). The option row's type is the member hash;
 * read_bool resolves the live value. */
#define GD_OPTIONS_STRUCT 0x834d9d9eu
#define GD_JUMP_SWAP_FIELD 0x3B0FC63Fu
static u8 *gd_options_entry(void) {
    u64 g = *(u64 *)GAMEDATA_MGR;
    if (!vptr(g)) return 0;
    u32 *map = *(volatile u32 **)(g + 0xC20);
    u32 slots = *(volatile u32 *)(g + 0xC2C);
    u32 count = *(volatile u32 *)(g + 0xC10);
    u8 *entries = *(volatile u8 **)(g + 0xC18);
    if (!map || !slots || slots > 1048576u || !entries) return 0;
    u32 start = GD_OPTIONS_STRUCT % slots, at = start;
    do {
        if (map[at * 2] == GD_OPTIONS_STRUCT) {
            u32 index = map[at * 2 + 1];
            return index < count ? entries + (u64)index * 32 : 0;
        }
        if (!map[at * 2]) break;
        at = at + 1 == slots ? 0 : at + 1;
    } while (at != start);
    return 0;
}
static void refresh_button_swap(void) {
    u8 *entry = gd_options_entry();
    u64 g = *(u64 *)GAMEDATA_MGR;
    if (!entry || !vptr(g)) { g_btn_swap = 0; return; }
    /* The out parameters live in the module's BSS: a Rekarina guest function must not receive the address of a
     * C stack local (its guest and host stacks are different). */
    static struct { void *entry; u32 index; u32 active; } d;
    static u8 value;
    d.entry = entry;
    d.index = 0;
    d.active = *(volatile u32 *)(entry + 24) >= 2;
    value = 0;
    if (!read_bool((void *)g, &value, &d, GD_JUMP_SWAP_FIELD)) { g_btn_swap = 0; return; }
    g_btn_swap = value != 0;
}
void hud_refresh_button_swap(void) { refresh_button_swap(); }
static u32 mmh3x(const char *a, const char *b, const char *c) {   /* murmur3_32(a + b + c, seed 0): GameData names */
    u8 buf[96]; u32 n = 0;
    for (u32 i = 0; a[i] && n < 95; i++) buf[n++] = (u8)a[i];
    for (u32 i = 0; b[i] && n < 95; i++) buf[n++] = (u8)b[i];
    for (u32 i = 0; c[i] && n < 95; i++) buf[n++] = (u8)c[i];
    u32 h = 0, k;
    const u32 c1 = 0xcc9e2d51u, c2 = 0x1b873593u;
    u32 r = n & ~3u;
    for (u32 i = 0; i < r; i += 4) {
        k = buf[i] | (buf[i + 1] << 8) | (buf[i + 2] << 16) | ((u32)buf[i + 3] << 24);
        k *= c1; k = (k << 15) | (k >> 17); k *= c2;
        h ^= k; h = (h << 13) | (h >> 19); h = h * 5 + 0xe6546b64u;
    }
    k = 0;
    switch (n & 3) {
    case 3: k ^= (u32)buf[r + 2] << 16; /* fallthrough */
    case 2: k ^= (u32)buf[r + 1] << 8;  /* fallthrough */
    case 1: k ^= buf[r]; k *= c1; k = (k << 15) | (k >> 17); k *= c2; h ^= k;
    }
    h ^= n; h ^= h >> 16; h *= 0x85ebca6bu; h ^= h >> 13; h *= 0xc2b2ae35u; h ^= h >> 16;
    return h;
}
static u32 mmh3(const char *a, const char *b) { return mmh3x(a, b, ""); }
/* Items whose button shows a quantity: the stackable consumables. Unique items (boomerang, hookshot, ocarina...)
 * also have a PouchData StockNum (1), so an allowlist is what keeps the "1" off their buttons. */
static int item_has_count(int item) {
    switch (item) {
    case 0: case 1: case 2: case 4: case 9: case 13:                    /* stick, nut, bomb, seeds, bombchu, bean */
    case 30: case 46: case 47: case 48: return 1;                       /* bow and elemental arrows */
    default: return 0;
    }
}
/* The GameData Int index of a consumable item's quantity (PouchData.<item>.StockNum, looked up by hash).
 * The bow and the slingshot have no own counter: they use the arrows' and the seeds'. -1 = no quantity. */
static int item_stock_index(int item) {
    if (!item_has_count(item)) return -1;
    const char *nm;
    if (item == 4) nm = "Item_DekuSeed";            /* slingshot: the seed count */
    else if (item == 30 || item == 46 || item == 47 || item == 48)
        nm = "Item_Arrow_Normal";                   /* bow and elemental arrows: the shared arrow count */
    else nm = item_name(item);
    if (!nm || nm[0] != 'I') return -1;
    u32 i = gd_find(2, mmh3x("PouchData.", nm, ".StockNum"));
    return i == GD_NONE ? -1 : (int)i;
}
/* Link is a child (Sequence_IsChildhood): 1 / 0, -1 = GameData not there */
int hud_is_child(void) { return gd_bool(gd_find(0, 0xCEA220D3u)); }   /* Sequence_IsChildhood */
/* PlayerStatus.AssignableItem.Child / .Adult: the Enum record of the item box */
static u32 box_rec(int child) { return gd_find(6, child ? 0xA0029837u : 0xBC32CE24u); }
/* Which item box the game uses: the item set byte +0x5C0 of **(main+0x4C7E2A8), the one its R selector passes to
 * the equip call (main+0x33C9A04). The box getter main+0x17B6E0 names the GameData key from it: set & 1 indexes
 * {"Adult", "Child"} (main+0x3F13A20), i.e. 1 = PlayerStatus.AssignableItem.Child, 0 = .Adult. It can disagree with
 * Sequence_IsChildhood (found live: IsChildhood 1, set 0, the game reading and writing the Adult box), so the box is
 * always picked by the set. 1 / 0; Sequence_IsChildhood when the set object is not there; -1 = neither. */
int hud_box_child(void) {
    u64 sets = *(u64 *)ITEM_SET_SLOT;
    u64 so = vptr(sets) ? *(u64 *)sets : 0;
    if (vptr(so)) return *(volatile unsigned char *)(so + 0x5C0) & 1;
    return hud_is_child();
}
/* The game's item box (ZR) for Link's age, from GameData: the item index, -1 empty, -2 unknown (no GameData, or a
 * value no item maps to, like the 2nd..5th bottle). Value hash -> actor name hash via GD_ACTOR_BOX. */
static u32 box_value(void) {
    int ch = hud_box_child();
    u64 mgr = *(u64 *)GAMEDATA_MGR;
    if (ch < 0 || !vptr(mgr)) return 0;
    u32 cnt = *(volatile u32 *)(mgr + 8 + 0x60 * 6 + 8);
    u64 rec = *(volatile u64 *)(mgr + 8 + 0x60 * 6 + 0x10);
    u32 i = box_rec(ch);
    return rec && i < cnt ? *(volatile u32 *)(rec + (u64)i * 0x38 + 0x1C) : 0;
}
static u32 g_item_box[64];   /* cached box value of each item index + 1 bit: 0 = not looked up, 1 = none */
static u32 item_box_value(int it) {
    if (!g_item_box[it]) {
        const char *nm = item_name(it);
        u32 h = nm ? mmh3("", nm) : 0, lo = 0, hi = GD_ACTOR_BOX_N;
        while (lo < hi) { u32 mid = (lo + hi) / 2; if (GD_ACTOR_BOX[mid][0] < h) lo = mid + 1; else hi = mid; }
        g_item_box[it] = nm && lo < GD_ACTOR_BOX_N && GD_ACTOR_BOX[lo][0] == h ? GD_ACTOR_BOX[lo][1] : 1;
    }
    return g_item_box[it];
}
static int box_item(void) {
    u32 v = box_value();
    if (!v) return -2;
    if (v == GD_EMPTY_HASH) return -1;
    for (int it = 0; it < 64; it++) if (item_box_value(it) == v) return it;
    return -2;
}
/* features/button_mapping: the game's item box straight from GameData (also under the pause menu, where the HUD
 * icon is not read): item index, -1 empty, -2 unknown */
int hud_box_item(void) { return box_item(); }
/* ... and set it there (-1 empty): the menu shows and keeps what GameData has. 0 if the item has no box value. */
void hud_box_empty(int child);
int hud_box_set(int item) {
    if (item < 0) { int ch = hud_box_child(); if (ch < 0) return 0; hud_box_empty(ch); return 1; }
    if (item >= 64) return 0;
    u32 v = item_box_value(item);
    if (v <= 1) return 0;
    int ch = hud_box_child();
    u64 mgr = *(u64 *)GAMEDATA_MGR;
    if (ch < 0 || !vptr(mgr)) return 0;
    u32 cnt = *(volatile u32 *)(mgr + 8 + 0x60 * 6 + 8);
    u64 rec = *(volatile u64 *)(mgr + 8 + 0x60 * 6 + 0x10);
    u32 i = box_rec(ch);
    if (!rec || i >= cnt) return 0;
    *(volatile u32 *)(rec + (u64)i * 0x38 + 0x1C) = v;
    return 1;
}
/* Empty the game's item box (ZR): PlayerStatus.AssignableItem.Child / .Adult (Enum table k = 6, 0x38-byte records,
 * live value at +0x1C = the hash of the value name) = "Empty". The HUD box follows. */
void hud_box_empty(int child) {
    u64 mgr = *(u64 *)GAMEDATA_MGR;
    if (!vptr(mgr)) return;
    u32 cnt = *(volatile u32 *)(mgr + 8 + 0x60 * 6 + 8);
    u64 rec = *(volatile u64 *)(mgr + 8 + 0x60 * 6 + 0x10);
    u32 i = box_rec(child);
    if (rec && i < cnt) *(volatile u32 *)(rec + (u64)i * 0x38 + 0x1C) = GD_EMPTY_HASH;
}
/* the player has the item (IsGet.<item's actor name>): 1 / 0; 1 also when the item has no IsGet flag */
static u32 g_own_idx[64];   /* cached Bool index + 1; 0xFFFFFFFF = no flag */
int hud_item_owned(int item) {
    if (item < 0 || item >= 64) return 0;
    if ((item >= 14 && item <= 17) || item == 50) return 1;   /* bottles: five cells, contents are not IsGet flags */
    if (!g_own_idx[item]) {
        const char *nm = item_name(item);
        if (!nm) return 1;
        if (!vptr(*(u64 *)GAMEDATA_MGR)) return 0;   /* no GameData yet: not owned, nothing cached */
        u32 i = gd_find(0, mmh3("IsGet.", nm));
        g_own_idx[item] = i == GD_NONE ? 0xFFFFFFFFu : i + 1;
    }
    if (g_own_idx[item] == 0xFFFFFFFFu) return 1;
    int v = gd_bool(g_own_idx[item] - 1);
    if (v > 0) return 1;
    if (v < 0) return 0;   /* no GameData: not owned */
    /* IsGet off is not proof: items that never played their "you got" scene (nuts from the grass, a slingshot from
     * a shop or a save editor) keep it off. A quantity in the pouch (nuts, seeds, arrows ...) is. */
    int st = item_stock_index(item);
    return st >= 0 && gd_int((u32)st) > 0;
}
int hud_menu_item(void) {
    u64 e = menu_cell_widget();
    if (!e) return -1;
    int idx = *(volatile int *)(g_menu_panel + 0x90);
    if (idx < 0 || idx >= 64) return -1;
    u64 a = *(volatile u64 *)(e + 0x48);
    a = a ? *(volatile u64 *)(a + 0x20) : 0;
    return a && *(volatile float *)(a + 0x18) != 0.0f ? idx : -1;
}
/* The pouch holds the item (PouchData.<actor>.StockNum > 0): how the pause menu knows Link has a sword or a shield.
 * 1 / 0; -1 when the item has no StockNum or GameData is not there. */
int hud_pouch_has(const char *actor) {
    u32 i = gd_find(2, mmh3x("PouchData.", actor, ".StockNum"));
    if (i == GD_NONE) return -1;
    int v = gd_int(i);
    return v < 0 ? -1 : v > 0;
}
/* the player owns an equipment item (IsGet.<actor name>): 1 / 0; unknown flag = not owned */
int hud_owns_name(const char *actor) {
    return gd_bool(gd_find(0, mmh3("IsGet.", actor))) > 0;
}
/* The cell's Decide animation: what the game plays when A puts an item in its box. main+0x2D0FE0 plays an
 * animation from its start (no sound); main+0x3772100 is what A does on a cell: the cell's button object
 * (widget +0xE8) plays Decide through its own animation list, at speed 1, with the menu's sound. A playing animation is linked in its
 * layout's list (idle, its list node points to itself). */
extern void ui_btn_decide(u64 button);
extern u64 ui_btn_anim(u64 button, u32 index);   /* main+0x3772000: the button's animation (4 = Decide), 0 if none */
void hud_menu_decide(void) {   /* the game's own Decide play for the cell (what A does: its sound comes with it) */
    u64 e = menu_cell_widget(), btn = e ? ptr(e, 0xE8) : 0;   /* the cell's button object (= g_menu_cell) */
    if (btn && ui_btn_anim(btn, 4)) ui_btn_decide(btn);
}
int hud_menu_decided(void) {
    u64 e = menu_cell_widget(), a = e ? ptr(e, 0x30) : 0;
    return a && ptr(a, 0) != a;
}
/* the game put an item in its box from the menu (A): show it on ZR now (the HUD box catches up on close) */
void hud_menu_zr(int item) {   /* -1: empty */
    if (g_zr_hold) return;
    char *dst = g_sb_force[10];
    const char *nm = item >= 0 ? item_name(item) : 0;
    const char *bn = bottle_icon(item);   /* a bottle cell: its content, not the current bottle */
    if (bn) nm = bn;
    if (!nm) { dst[0] = 1; dst[1] = 0; return; }
    u32 i;
    for (i = 0; i < 30 && nm[i]; i++) dst[i] = nm[i];
    dst[i] = 0;
    dst[31] = 0;
}
/* Y shows the equipped sword: GameData PlayerStatus.EquipWeapon (String64 table k = 16, record 23, 0x108 bytes
 * each, the live string at +0x74 / pointed to by +0x68). Its value is the icon name itself (Weapon_Sword_Kokiri,
 * _Master, _Giant, _Biggoron, _BrokenGiant); empty = no sword, the Y icon is hidden. */
/* An equipped weapon (GameData String64 PlayerStatus.EquipWeapon = its icon name) as the fixed texture of an item
 * part: the Y button shows the sword. */
static void equip_mirror(u32 idx, u32 part) {
    u64 mgr = *(u64 *)GAMEDATA_MGR;
    if (!vptr(mgr)) return;
    u32 cnt = *(u32 *)(mgr + 8 + 0x60 * 16 + 8);
    u64 rec = *(u64 *)(mgr + 8 + 0x60 * 16 + 0x10);
    if (!vptr(rec) || cnt <= idx) return;
    u64 r = rec + (u64)idx * 0x108;
    const char *s = *(const char **)(r + 0x68);
    if (!vptr((u64)s)) s = (const char *)(r + 0x74);
    char *dst = g_sb_force[part];
    if (!s[0] || (s[0] != 'W' && s[0] != 'A')) { dst[0] = 1; return; }   /* none: hidden (a later one sets it again) */
    if (str_eq(dst, s)) return;
    u32 i;
    for (i = 0; i < 30 && s[i]; i++) dst[i] = s[i];
    dst[i] = 0;
    dst[31] = 0;   /* apply again */
}
/* The pause menu's Items page (PaItemMenu_00) carries the same PART_N parts (build_menu): g_mb, with their own
 * icon controllers (hud_menu_ctrl_init). Placement there: g_mb_lay (see g_sb_lay). */
static SlotBtn g_mb[PART_N];
static u64 g_mb_ctrl[PART_N];
static u64 g_mb_frame;
u64 g_mb_dbg[PART_N];
/* Layout options (app): simple = only the assignable buttons (no A / X / Y and the sword); L as item selector = L
 * can't hold an item and its button is hidden in both layouts. */
static int bm_part_hidden(u32 k) {
    if (part_phys(k) >= PB_L3) return 1;   /* remap: the role is on no button (or on a stick: no HUD art) */
    if (g_shbcfg.bm_simple && (k == SB_X || k == SB_Y || k == SB_A || k == 9 || k >= SB_SHIELD)) return 1;
    if (remap_l_selector() && (k == SB_L || k == 7)) return 1;   /* L opens the item selector: no L button */
    return 0;
}
/* Our parts are clones of the stock item box and each gets that box's UI control. The stock box's control has its
 * "focusable" flag (+0x40 bit 0x10) off; ours kept it, so the menu cursor moved onto our invisible parts (Items page,
 * right of the hammer) and the HUD layout held the focus on one of them (shops, dialogues, game over, Farore's Wind:
 * the stick went through invisible options first). The control is in its frame's control list ([part + 0x28] + 0x38
 * is the list's head object, links at +8), vtable main+0x4C216E8, its part at +0x20; byte +0x38 is its state
 * (3 = focused) and virtual +0xF8 sets it. Measured live (Fritchii). */
extern char UI_CTRL_VT[];
static u64 part_ctrl(u64 part) {
    u64 fr = vptr(part) ? ptr(part, 0x28) : 0;
    u64 head = vptr(fr) ? ptr(fr, 0x38) : 0;
    if (!vptr(head)) return 0;
    u64 first = head + 8, n = first;
    for (u32 i = 0; i < 1024; i++) {
        n = ptr(n, 8);
        if (!vptr(n) || n == first) return 0;
        u64 o = n - 8;
        if (ptr(o, 0) == (u64)UI_CTRL_VT && ptr(o, 0x20) == part) return o;
    }
    return 0;
}
static void ctrl_unfocusable(u64 c) {
    if (!vptr(c) || ptr(c, 0) != (u64)UI_CTRL_VT) return;
    volatile u32 *fl = (volatile u32 *)(c + 0x40);
    if (*fl & 0x10u) *fl &= ~0x10u;
    if (*(volatile u8 *)(c + 0x38) == 3) {   /* it has the focus: let it go (the game's own state change) */
        u64 vt = ptr(c, 0);
        ((void (*)(u64, u32))(void *)ptr(vt, 0xF8))(c, 0);
    }
}
static void slots_draw(SlotBtn *set, const u64 *ctrls, u64 frame, const float *lay, int main_set) {
    u32 nparts = main_set ? (u32)PART_TOT : PART_BASE;   /* only the parts that were built (no shield in the menu) */
    for (u32 k = 0; k < nparts; k++) {
        SlotBtn *b = &set[k];
        if (!b->part) {
            u64 p = (u64)find_part_safe((void *)frame, part_name(k));
            if (!p) continue;
            u64 none = (u64)find_anim((void *)p, "__no_such_anim__");
            u32 mask = 0;
            for (u32 i = 0; i < SA_N; i++) {
                u64 a = (u64)find_anim((void *)p, slot_anim_name(i));
                b->anim[i] = (a && a != none) ? a : 0;
                if (b->anim[i]) mask |= 1u << i;
            }
            {   /* the nested icon part: its icon pane and its shadow pane (where our button is drawn) */
                u64 rif = (u64)find_part_safe((void *)p, "Pa_ReplaceIcon_00");
                b->rep = rif ? ptr(rif, 0x6A8) : 0;
                b->ic = rif ? part_child(rif, "P_ReplaceIcon_00") : 0;
                b->sh = rif ? part_child(rif, "P_ReplaceIconSh_00") : 0;
                b->icon_set = 0;
                b->forced[0] = 0;
                b->ctrl = ctrls[k];   /* built during the HUD / menu init (hud_slots_init, hud_menu_ctrl_init) */
            }
            b->rec = pane_record(frame, part_name(k));
            for (u32 i = 0; i < 9; i++) b->hide[i] = part_child(p, hide_name(i));
            b->num_rec = part_child(p, "T_ItemNum_00");   /* the box's own count text */
            b->num_val = -2;
            b->num_item = -2;
            b->uctl = part_ctrl(p);
            b->part = p;
        }
        ctrl_unfocusable(b->uctl);
        /* an item part with no item (and no fixed texture, like the sword) is hidden */
        u32 empty = (!part_is_button(k) && g_sb_item[k] < 0 && g_sb_force[k][0] <= 1) || bm_part_hidden(k);
        float sc = empty || !BTNMAP ? 0.0f : g_sb_sc[k] * lay[1];
        float px, py;
        sb_xy(lay, k, g_shbcfg.bm_simple, &px, &py);
        if (b->rec) place(b->rec, main_set ? px - ACTIONS_X : px, py, sc, sc, 0.0f);
        /* ItemPattern = icon framing. Frame 0 shows the full picture: buttons, and items drawn big and even
         * like the item buttons (item_pattern() keeps the stock box's per-item framing for a smaller style) */
        if (b->anim[0]) set_frame((void *)b->anim[0], g_sb_item[k] < 0 || g_sb_big_items ? 0.0f : item_pattern(g_sb_item[k]));
        if (b->sh) {   /* the button behind the icon: full colour, opaque, centred on the icon (no shadow offset) */
            tint_rec(b->sh, 1.0f, 1.0f, 1.0f, 1.0f);
            if (b->ic) {
                *(float *)(b->sh + 0x6C) = *(float *)(b->ic + 0x6C);
                *(float *)(b->sh + 0x70) = *(float *)(b->ic + 0x70);
            }
            *(u32 *)b->sh |= 0x6F;
        }
        /* item quantity (consumables only): the box's own count text, e.g. 30 bombs / 20 arrows.
         * The Int index is resolved only when the button's item changes; the value is a plain read.
         * ZR does not go through g_sb_item: it mirrors the game's own box (hud_zr_item). */
        int count_item = g_sb_item[k];
        if (count_item < 0 && k == 10) count_item = hud_zr_item();
        if (b->num_item != count_item) {
            b->num_item = count_item;
            b->num_idx = item_stock_index(count_item);
            b->num_val = -2;
        }
        int cnt = b->num_idx >= 0 ? gd_int((u32)b->num_idx) : -1;
        if (main_set) g_sb_stock[k] = cnt;
        for (u32 i = 0; i < 9; i++)
            if (b->hide[i] && (i < 8 || cnt < 0)) {   /* the count stays on the icon parts, only with a quantity */
                *(float *)(b->hide[i] + 0x60) = 0.0f; *(float *)(b->hide[i] + 0x64) = 0.0f;
                *(u32 *)b->hide[i] |= 0x6F;
            }
        if (b->hide[8] && cnt >= 0) {   /* the count pane: visible whenever the item has a quantity */
            *(float *)(b->hide[8] + 0x60) = 1.0f; *(float *)(b->hide[8] + 0x64) = 1.0f;
            *(u32 *)b->hide[8] |= 0x6F;
        }
        if (b->num_rec && cnt >= 0) {
            if (cnt != b->num_val) {
                b->num_val = cnt;
                u32 n = put_int(b->num_txt, cnt);
                g_text_ref.s = b->num_txt; g_text_ref.len = n; g_text_ref.z = 0; g_text_ref.m1 = -1;
                set_text((void *)b->num_rec, &g_text_ref, 0);
            }
        } else {
            b->num_val = cnt;
        }
        for (u32 i = 0; i < SA_N; i++)
            if (g_sb_animf[i] >= 0.0f && b->anim[i]) set_frame((void *)b->anim[i], g_sb_animf[i]);
        /* fixed texture (buttons, sword, ZR mirror), only when it changes; OotBtn<style>_..: the pad style */
        if (b->ctrl && !g_sb_busy && g_sb_force[k][0] > 1) {
            char nm[32];
            u32 i;
            const char *src = g_sb_force[k];
            if (part_is_button(k)) {
                /* The artwork follows the physical button, including custom role assignments. */
                u32 pp = part_phys(k);
                int blank = k == SB_X || (k == SB_A && (!main_set || g_sb_force[SB_A][8] == 'B'));
                src = blank || pp >= PB_L3 ? "OotBtnS_Blank" : PB_TEX[pp];
                if (g_shbcfg.bm_layout == 3 && !g_shbcfg.bm_simple && g_shbcfg.button_style == 0) {
                    if (pp == SB_A) src = blank ? "OotBtnG_A0" : "OotBtnG_A";
                    if (pp == SB_B) src = blank ? "OotBtnG_B0" : "OotBtnG_B";
                    if (pp == SB_X) src = blank ? "OotBtnG_X0" : "OotBtnG_X";
                    if (pp == SB_Y) src = blank ? "OotBtnG_Y0" : "OotBtnG_Y";
                }
            }
            for (i = 0; i < 31 && src[i]; i++) nm[i] = src[i];
            nm[i] = 0;
            if (nm[0] == 'O' && i > 7 && nm[6] == 'S' && nm[7] == '_')
                nm[6] = g_shbcfg.button_style == 1 ? 'X' : g_shbcfg.button_style == 2 ? 'P' : 'S';
            if (!str_eq(b->forced, nm)) {
                for (u32 j = 0; j <= i; j++) b->forced[j] = nm[j];
                set_icon(b, nm);
            }
        }
        /* icon of the button's item: also refresh when the item's actor name changes (bottles keep the same
         * index while their content changes, e.g. empty -> red potion) */
        const char *inm = g_sb_item[k] >= 0 ? item_name(g_sb_item[k]) : 0;
        /* bottles: the item index gives the current bottle (e.g. RedPotion); read the bottle slot for its own content */
        if (k != 10) {
            const char *bn = bottle_icon(g_sb_item[k]);
            if (bn) inm = bn;
        }
        u32 nh = inm ? mmh3("", inm) : 0;
        if (b->ctrl && !g_sb_busy &&
            (!b->icon_set || b->shown_item != g_sb_item[k] || b->shown_name != nh)) {
            b->icon_set = 1;
            b->shown_item = g_sb_item[k];
            b->shown_name = nh;
            if (inm) set_icon_btn(b, inm, g_sb_btn[k][0] ? g_sb_btn[k] : 0);
        }
    }
}
/* Pause menu, Items tab: the cluster drawn on the menu page itself. Neither the HUD layer nor its frame update
 * run under the menu, so this is driven by the input hook (features/button_mapping), every frame. */
/* HUD language of the mod: the installer writes the language the game is played in into the config block (byte 75),
 * and the few labels the mod draws itself follow it. The game's own prompts already come in the game's language.
 * 0 English, 1 Spanish, 2 Portuguese, 3 French, 4 German, 5 Italian. */
enum { HL_EN, HL_ES, HL_PT, HL_FR, HL_DE, HL_IT, HL_N };
/* arrays, not pointer tables: our read-only data is not relocated (see the build's jump-table note) */
static const char g_hl_jump[HL_N][12] = {"Jump", "Saltar", "Pular", "Sauter", "Springen", "Saltare"};
static const char g_hl_attack[HL_N][12] = {"Attack", "Atacar", "Atacar", "Attaquer", "Angreifen", "Attaccare"};
static const char g_hl_roll[HL_N][12] = {"Roll", "Rodar", "Rolar", "Rouler", "Rollen", "Rotolare"};
static const char g_hl_assign[HL_N][12] = {"Assign:", "Asignar:", "Atribuir:", "Assigner:", "Zuweisen:", "Assegna:"};
static const char *hl(const char (*t)[12]) {
    u32 l = g_shbcfg.lang;
    return t[l < HL_N ? l : HL_EN];
}
/* Assign legend on the Items tab, bottom right above the game's own guide: which menu button assigns to which
 * slot. g_leg_pos = x of the first text, y, x step, text scale (layout units of PlusMenu_00, y up); g_leg_bar = the
 * dark bar behind it, like the game's own guide bar below. */
static char g_leg_txt[LEG_N][16] = {"", "\xee\x81\x82", "\xe2\x80\xba", "\xee\x81\x81", "\xee\x81\x84", "\xe2\x80\xba", "\xee\x81\x84", "\xee\x81\x85", "\xe2\x80\xba", "\xee\x81\x85", "\xee\x81\x80", "\xe2\x80\xba", "\xee\x81\x87"};   /* Assign: X > B  L > L  R > R  A > ZR */
float g_leg_pos[4] = {690.0f, -905.0f, 252.0f, 1.0f};   /* right above the game's guide bar */
float g_leg_geo[3] = {215.0f, 62.0f, 2.0f};   /* title -> first pair, icon -> arrow -> icon spacing, icon scale */
float g_leg_bar[8] = {1250.0f, -905.0f, 0.66f, 0.27f, 0.85f, 0.07f, 0.06f, 0.05f};   /* backing plate: centre x, y,
                                                                               * x / y scale, alpha, rgb */
static u64 g_leg_bar_rec;
static u64 g_plus_frame, g_leg_rec[LEG_N], g_leg_set;
static u32 g_leg_shown;
u64 g_leg_dbg[2];
static void hud_menu_legend(u64 page) {
    u64 pf = page ? ptr(page, 0x30) : 0;   /* the page frame +0x30: the PlusMenu_00 frame */
    if (pf != g_plus_frame) { g_plus_frame = pf; g_leg_set = 0; g_leg_bar_rec = 0; for (u32 t = 0; t < LEG_N; t++) g_leg_rec[t] = 0; }
    g_leg_dbg[0] = pf;
    if (!pf) return;
    {   /* the legend title, in the mod's language */
        const char *s = hl(g_hl_assign);
        u32 i = 0;
        for (; i < 15 && s[i]; i++) g_leg_txt[0][i] = s[i];
        g_leg_txt[0][i] = 0;
    }
    /* pairs: menu button > the slot's own button, for items 1 / 2 / 3, and A > the game's item box. The glyphs are
     * the game's button font (remap, Jump/B swap: the slot's button may be any of the eight). */
    u32 pair_on[4], n_off = 0;
    {
        static const u8 GLYPH[PB_N] = {4, 2, 5, 3, 0, 7, 1, 6, 0, 0};   /* PB_ -> 0xE040 + n (A B X Y L R ZL ZR) */
        static const u8 SLOT_ROLE[4] = {ROLE_ITEM1, ROLE_ITEM2, ROLE_ITEM3, ROLE_ITEM_MAIN};
        u32 mb[3];
        remap_menu_buttons(mb);
        u32 changed = 0;
        for (u32 pr = 0; pr < 4; pr++) {
            u32 src = pr < 3 ? mb[pr] : PB_A, dst = remap_role_phys(SLOT_ROLE[pr]);
            pair_on[pr] = src < PB_N && dst < PB_L3 &&
                          !(pr == 1 && (remap_l_selector() || (g_shbcfg.manual_shield && !remap_on())));   /* L slot */
            if (!pair_on[pr]) { n_off++; continue; }
            char *ts = g_leg_txt[1 + pr * 3], *td = g_leg_txt[3 + pr * 3];
            u8 gs = (u8)(0x80 + GLYPH[src]), gd = (u8)(0x80 + GLYPH[dst]);
            if ((u8)ts[2] != gs || (u8)td[2] != gd) changed = 1;
            ts[0] = td[0] = (char)0xEE; ts[1] = td[1] = (char)0x81;
            ts[2] = (char)gs; td[2] = (char)gd; ts[3] = td[3] = 0;
        }
        if (changed) g_leg_set = 0;   /* set the texts again */
    }
    static const float WHITE[3] = {1.0f, 1.0f, 1.0f}, GOLD[3] = {0.95f, 0.82f, 0.45f};
    if (!g_leg_bar_rec) g_leg_bar_rec = pane_record(pf, g_legbar_name);
    if (g_leg_bar_rec) {   /* no backing plate: the legend reads on the menu's own background */
        *(float *)(g_leg_bar_rec + 0x60) = 0.0f;
        *(float *)(g_leg_bar_rec + 0x64) = 0.0f;
        *(float *)(g_leg_bar_rec + 0x74) = 0.0f;
        *(u32 *)g_leg_bar_rec |= 0x6F;
    }
    for (u32 t = 0; t < LEG_N; t++) {
        if (!g_leg_rec[t]) g_leg_rec[t] = pane_record(pf, g_lnames[t]);
        u64 rec = g_leg_rec[t];
        if (!rec) continue;
        if (g_leg_set != pf) {
            u32 n = 0;
            while (g_leg_txt[t][n]) n++;
            g_text_ref.s = g_leg_txt[t]; g_text_ref.len = n; g_text_ref.z = 0; g_text_ref.m1 = -1;
            set_text((void *)rec, &g_text_ref, 0);
            g_leg_dbg[1]++;
        }
        const float *c = t ? WHITE : GOLD;
        tint_rec(rec, c[0], c[1], c[2], 1.0f);
        float x = g_leg_pos[0], sc = g_leg_pos[3];
        u32 on = 1;
        if (t) {
            u32 pr = (t - 1) / 3, k = (t - 1) % 3;
            if (!pair_on[pr]) on = 0;
            else { u32 at = pr; for (u32 q = 0; q < at; q++) if (!pair_on[q]) pr--; }   /* close the gaps */
            x += g_leg_geo[0] + g_leg_pos[2] * (float)pr + g_leg_geo[1] * (float)k;
            if (k != 1) sc *= g_leg_geo[2];
        }
        place(rec, x, g_leg_pos[1], on ? sc : 0.0f, on ? sc : 0.0f, 0.0f);
    }
    g_leg_set = pf;
    g_leg_shown = 1;
}
/* the menu shows another tab (or closes): hide the legend once */
void hud_menu_off(void) {
    if (!g_leg_shown || !g_plus_frame) return;
    g_leg_shown = 0;
    for (u32 t = 0; t < LEG_N; t++) if (g_leg_rec[t]) place(g_leg_rec[t], 0.0f, 0.0f, 0.0f, 0.0f, 0.0f);
    if (g_leg_bar_rec) place(g_leg_bar_rec, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f);
}
void hud_menu_tick(void) {
    if (!BTNMAP || g_sb_busy || !hud_menu_items()) return;
    u64 mf = ptr(g_menu_panel, 0x20);
    if (!mf) return;
    if (mf != g_mb_frame) { g_mb_frame = mf; for (u32 k = 0; k < PART_N; k++) g_mb[k].part = 0; }
    hud_menu_legend(mf);
    slots_draw(g_mb, g_mb_ctrl, mf, g_mb_lay, 0);
}
static void slots_update(u64 frame) {
    if (g_sb_busy) return;
    if (!g_ps_frame && g_sb[0].part) g_ps_frame = ptr(g_sb[0].part, 0x28);
    if (!hud_ingame()) return;
    if (BTNMAP) {
        zr_mirror();
        equip_mirror(GD_S64_EQUIP_WEAPON, 9);                              /* Y: the sword */
        if (hud_shield_parts()) equip_mirror(GD_S64_EQUIP_SHIELD, 13);     /* the shield button: the shield */
    }   /* (the ZR icon is frozen while a slot borrows the box) */
    if (!g_stock_box_rec) g_stock_box_rec = pane_record(frame, "Pa_SelectItem_00");
    if (!g_stock_dots_rec) g_stock_dots_rec = pane_record(frame, "N_Shortcut_00");
    for (u32 i = 0; i < 2 && g_sb_hide_stock && BTNMAP; i++) {
        u64 r = i ? g_stock_dots_rec : g_stock_box_rec;
        if (!r) continue;
        *(float *)(r + 0x60) = 0.0f; *(float *)(r + 0x64) = 0.0f;
        *(u32 *)r |= 0x6F;
    }
    slots_draw(g_sb, g_sb_ctrl, frame, g_sb_lay, 1);
}
/* The game's control guide (UICtrlGuideScreen, the "A Hablar" prompt next to Link) writes A's context action
 * ("Hablar", "Abrir", "Revisar" ...) into its text pane T_GuideA_00 (captured in hud_on_settext). While the prompt
 * is up its N_InOut_00 / N_A_00 alpha (pane record +0x78) is 1; it fades to 0 when there is nothing to do. */
u64 g_aguide_frame;
char g_aguide_text[48];
/* Diagnostic: the last 32 game texts set through the set-text-by-name call (pane name + first bytes), used to find
 * the names a layout writes (e.g. the map's). Read by tools/live_check.py. */
u32 g_stxt_n = 0x10000000u;
struct { char nm[24]; char s[24]; u32 len; } g_stxt[32];
static void stxt_log(const char *nm, const char *s, u32 len) {
    u32 e = (g_stxt_n - 0x10000000u) % 32, j;
    g_stxt_n++;
    for (j = 0; j < 23 && nm && nm[j]; j++) g_stxt[e].nm[j] = nm[j];
    g_stxt[e].nm[j] = 0;
    for (j = 0; j < 23 && s && j < len && s[j]; j++) g_stxt[e].s[j] = s[j];
    g_stxt[e].s[j] = 0;
    g_stxt[e].len = len;
}
char g_sb_a_label[16] = "";   /* context action of A (empty: the button shows its letter) */
extern int player_state_is(const char *name);
extern int player_state_starts(const char *prefix);
static int a_guide_up(void) {
    static u64 fr, r_io, r_a;
    if (!g_aguide_frame || !g_aguide_text[0]) return 0;
    if (fr != g_aguide_frame) {
        fr = g_aguide_frame;
        r_io = pane_record(fr, "N_InOut_00");
        r_a = pane_record(fr, "N_A_00");
    }
    return r_io && r_a && *(float *)(r_io + 0x78) > 0.5f && *(float *)(r_a + 0x78) > 0.5f;
}
static void a_label_update(void) {
    const char *txt = 0;
    if (a_guide_up()) txt = g_aguide_text;
    else if (player_state_starts("LockOn") && !player_state_is("LockOnWait")) txt = hl(g_hl_attack);   /* ZL + moving: jump attack */
    else if (player_state_is("Move")) txt = hl(g_hl_roll);   /* running: A rolls */
    u32 n = 0;
    if (txt) {
        while (txt[n] && n < 15) n++;
        while (n && n < 15 && txt[n] && ((u8)txt[n] & 0xC0) == 0x80) n--;   /* cut on a UTF-8 character */
    }
    int same = 1;
    for (u32 i = 0; i < n; i++) if (g_sb_a_label[i] != txt[i]) same = 0;
    if (same && !g_sb_a_label[n]) return;
    for (u32 i = 0; i < n; i++) g_sb_a_label[i] = txt[i];
    g_sb_a_label[n] = 0;
    /* A's button: the plain base under a label, the lettered one without */
    const char *bg = n ? "OotBtnS_Blank" : "OotBtnS_A";
    u32 i;
    for (i = 0; bg[i]; i++) g_sb_force[SB_A][i] = bg[i];
    g_sb_force[SB_A][i] = 0;
    g_sb_force[SB_A][31] = 0;
}
static void slots_labels(u64 frame) {
    if (!BTNMAP) return;
    static const float LABEL[3] = {0.92f, 0.92f, 0.92f};
    if (!hud_ingame()) return;
    a_label_update();
    float xx, xy, ax, ay;
    sb_xy(g_sb_lay, SB_X, 0, &xx, &xy);   /* the jump part: wherever jump is (remap, Jump/B swap) */
    sb_xy(g_sb_lay, SB_A, 0, &ax, &ay);
    g_fld_sc[4] = g_fld_sc[5] = 0.80f * g_sb_lay[1];   /* the labels follow the buttons' size */
    if (g_shbcfg.bm_layout == 3 && g_shbcfg.button_style == 0)
        g_fld_sc[4] *= 0.68f;  /* the jump label is wider than the curved X cap */
    u32 lbl = !g_shbcfg.bm_simple;   /* simple layout: no X / A buttons, so no labels */
    put_text(frame, 4, hl(g_hl_jump), xx, xy, lbl && part_phys(SB_X) < PB_L3, LABEL);
    put_text(frame, 5, g_sb_a_label, ax, ay, lbl && g_sb_a_label[0] != 0 && part_phys(SB_A) < PB_L3, LABEL);
}
/* ---- the context prompt next to Link (layout DoCommand_00, the "(A) Hablar" one) ----
 * It has one block per face button, N_A_00 / N_B_00 / N_X_00 / N_Y_00, each drawn as a diamond: P_Base?_00 the
 * diamond backing, the other three letters greyed (alpha 0.13), and the key bubble with T_Key?_00 (the button's
 * glyph) and T_Guide?_00 (the action). The game drives the blocks by VIRTUAL button; with the remap the role
 * behind a block can be on another physical button, so:
 *   - the backing and the greyed letters are hidden: only the key and the action show;
 *   - the key shows the glyph of the physical button that holds the block's role (action on X shows X).
 * The glyph is the game's button font (U+E040 A, B, X, Y, L, R, ZL, ZR), so the pad style (PS shapes) follows. */
static const char DC_HIDE[16][12] = {"P_BaseA_00", "P_B_00", "P_X_00", "P_Y_00", "P_BaseB_00", "P_A_00", "P_X_02",
                                     "P_Y_02", "P_BaseX_00", "P_A_02", "P_B_03", "P_Y_04", "P_BaseY_00", "P_A_01",
                                     "P_B_02", "P_X_03"};
static const char DC_KEY[4][10] = {"T_KeyA_00", "T_KeyB_00", "T_KeyX_00", "T_KeyY_00"};
static u64 g_dc_frame, g_dc_hide[16], g_dc_key[4];
static char g_dc_glyph[4][4];   /* glyph text each block's key gets ("" = the game's own) */
static u32 g_dc_set[4];         /* that glyph was set (cleared when the frame or the glyph changes) */
u64 g_dc_dbg[4];                /* diagnostic: frame, panes found, key sets, settext rewrites */
/* the block's virtual button -> the role behind it -> its physical button -> the glyph (E040 + n), -1: leave */
static int dc_glyph(u32 block) {
    static const u8 GLYPH[PB_N] = {4, 2, 5, 3, 0, 7, 1, 6, 0, 0};   /* PB_ -> 0xE040 + n (A B X Y L R ZL ZR) */
    u32 sw = g_btn_swap, role;
    switch (block) {
    case 0: role = ROLE_ACTION; break;
    case 1: role = sw ? ROLE_JUMP : ROLE_ITEM1; break;
    case 2: role = sw ? ROLE_ITEM1 : ROLE_JUMP; break;
    default: role = ROLE_SWORD; break;
    }
    u32 p = remap_role_phys(role);
    return p < PB_L3 ? (int)GLYPH[p] : -1;
}
/* The glyph each block's key shows: the role's physical button while the remap is live (gameplay); the block's
 * own button otherwise (dialogues, shops, menus keep the real buttons: features/button_mapping g_remap_live).
 * g_dc_over: the key shows (or showed) another glyph, so the own one is written back once when it changes. */
extern u32 g_remap_live;
static u32 g_dc_over[4];
static void dc_glyphs(void) {
    for (u32 b = 0; b < 4; b++) {
        int g = g_remap_live ? dc_glyph(b) : -1;
        if (g < 0) g = (int)b;   /* the block's own button (glyph E040 + A0 B1 X2 Y3) */
        char *t = g_dc_glyph[b];
        if (t[0] && (u8)t[2] == (u8)(0x80 + g)) continue;
        t[0] = (char)0xEE; t[1] = (char)0x81; t[2] = (char)(0x80 + g); t[3] = 0;
        if ((u32)g != b) g_dc_over[b] = 1;
        g_dc_set[b] = g_dc_over[b] ? 0 : 1;   /* never overridden: nothing to write */
        if ((u32)g == b) g_dc_over[b] = 0;
    }
}
static void dc_update(void) {
    u64 fr = g_aguide_frame;
    if (!vptr(fr) || !vptr(ptr(fr, 0x80))) return;
    if (fr != g_dc_frame) {   /* a new layout: find its panes again */
        g_dc_frame = fr;
        for (u32 i = 0; i < 16; i++) g_dc_hide[i] = pane_record(fr, DC_HIDE[i]);
        for (u32 b = 0; b < 4; b++) { g_dc_key[b] = pane_record(fr, DC_KEY[b]); g_dc_set[b] = 0; }
        g_dc_dbg[0] = fr;
    }
    u32 found = 0;
    for (u32 i = 0; i < 16; i++) {
        u64 r = g_dc_hide[i];
        if (!r) continue;
        found++;
        *(float *)(r + 0x60) = 0.0f; *(float *)(r + 0x64) = 0.0f;
        *(u32 *)r |= 0x6F;
    }
    g_dc_dbg[1] = found;
    dc_glyphs();
    /* the key glyph: set when it changes, and again each time the prompt comes up (the game may reset it) */
    static u32 was_up;
    u32 up = a_guide_up();
    if (up && !was_up) for (u32 b = 0; b < 4; b++) if (g_dc_over[b]) g_dc_set[b] = 0;
    was_up = up;
    for (u32 b = 0; b < 4; b++) {
        if (g_dc_set[b] || !g_dc_glyph[b][0] || !g_dc_key[b]) continue;
        g_text_ref.s = g_dc_glyph[b]; g_text_ref.len = 3; g_text_ref.z = 0; g_text_ref.m1 = -1;
        set_text((void *)g_dc_key[b], &g_text_ref, 0);
        g_dc_set[b] = 1;
        g_dc_dbg[2]++;
    }
}
/* the game writing a block's key glyph by name: our glyph instead (0: the game's own) */
static const char *dc_key_override(const char *nm) {
    for (u32 b = 0; b < 4; b++)
        if (str_eq(nm, DC_KEY[b])) return g_dc_glyph[b][0] && (u8)g_dc_glyph[b][2] != (u8)(0x80 + b) ? g_dc_glyph[b] : 0;
    return 0;
}
static void slots_reset(void) {
    g_ps_frame = 0; for (u32 k = 0; k < PART_N; k++) g_sb[k].part = 0; g_stock_box_rec = 0; g_stock_dots_rec = 0; }

u32 hud_frame_count(void) { return g_tick; }
static void hud_frame(void) {
    shb_clock_tick();                                /* real frame duration: the features count frames, not time */
    g_hud_dbg[1] = MARK | ++g_tick;
    refresh_button_swap();
    if (g_shbcfg.hud_collectibles) kset_refresh();   /* every frame, loads too: progress.sav is in the save buffer only briefly */
    if (!g_mp || (int)(g_tick - g_ready_at) < 0) return;
    if (!vptr(g_frame_node)) g_frame_node = 0;
    if (!g_frame_node) {
        u64 owner = ptr(g_mp, 0x18);
        if (!owner) return;
        u64 parent = ptr(owner, 0x28);
        if (!parent) return;
        g_frame_node = parent;
    }
    txt_update(g_frame_node);
    anchor_update();
    hide_minimap();
    if (g_test_mode) {
        u32 p = (g_tick / 120) % 13;     /* demo sweep 0..100 until the game publishes */
        g_cap = 100.0f;
        (void)p; g_value = 60.0f;
        g_show = 1.0f;
        g_tired = (g_tick / 480) & 1;   /* demo: alternate green / yellow */
    }
    ring_update();
    bar_update(g_frame_node);
    slots_update(g_frame_node);
    slots_labels(g_frame_node);
    dc_update();
    hud_group_scale(g_frame_node);
}
/* -------- observation of the stock item box (to learn icon frames) -------- */
#define FL_N 24
typedef struct { u32 tag; float frame; } FrameLog;
FrameLog g_fl[FL_N] = {{0x7FFFFFFFu, 0.0f}};
u32 g_fl_n = 0x10000000u;
static u64 g_stock_pat, g_stock_num;
u64 g_stock_dbg[4] = {MARK, MARK, MARK, MARK};
static float g_last_tagged[3] = {-1e9f, -1e9f, -1e9f};

static void observe_stock_box(u64 parent) {
    if (g_stock_pat) return;
    u64 box = (u64)find_part_safe((void *)parent, "Pa_SelectItem_00");
    g_stock_dbg[0] = MARK | box;
    if (!box) return;
    g_stock_pat = (u64)find_anim((void *)box, "ItemPattern");
    g_stock_num = (u64)find_anim((void *)box, "StockNum");
    g_stock_dbg[1] = MARK | g_stock_pat;
    g_stock_dbg[2] = MARK | g_stock_num;
}

/* last frame the game put on the stock icon/count animations (read by button_mapping) */
float g_last_pat_frame = -1.0f, g_last_num_frame = -1.0f;
u32 g_pat_changes;

static void on_set_frame(u64 anim, float frame) {
    u32 tag = 0;
    if (anim && anim == g_stock_pat) { tag = 1; g_last_pat_frame = frame; g_pat_changes++; }
    else if (anim && anim == g_stock_num) { tag = 2; g_last_num_frame = frame; }
    if (!tag || g_last_tagged[tag] == frame) return;
    g_last_tagged[tag] = frame;
    FrameLog *e = &g_fl[(g_fl_n - 0x10000000u) % FL_N];
    g_fl_n++;
    e->tag = tag;
    e->frame = frame;
}

u32 g_inst_log[32] = {0x7FFFFFFFu};   /* pane indices passed to part instantiation */
u32 g_inst_n = 0x10000000u;
u64 g_inst_child[32] = {1};
#define INST_LOG g_inst_log

void player_track(u64 actor);
void player_guard_event(u32 id);
__attribute__((weak)) void stamina_speed_hook(void *regs);   /* features/stamina (absent in builds without it) */
__attribute__((weak)) void songbook_on_ocarina(u32 active, void *ctx);   /* features/songbook */
__attribute__((weak)) void songbook_on_select(void *ctx, u32 sel) { (void)ctx; (void)sel; }   /* overridden by the feature */
__attribute__((weak)) void songbook_on_frame(void);                      /* features/songbook */
__attribute__((weak)) void manual_shield_on_frame(void *scheduler);      /* features/manual_shield */
__attribute__((weak)) void songbook_on_ui(void *ctx);                    /* features/songbook */
/* The game's control guide (UICtrlGuideScreen) writes the A action ("Hablar", "Abrir", "Rodar"...) into its text
 * pane T_GuideA_00 through the set-text-by-name call (frame in x22, pane name in x21). */
static void hud_on_settext(u64 pane, const u64 *ref, u64 lr, const u64 *regs) {
    extern char __base[];
    if (!ref || !vptr((u64)ref) || !regs || !vptr((u64)regs) || lr >= (u64)__base) return;   /* our own texts */
    const char *s = (const char *)ref[0];
    u32 len = (u32)ref[1];
    if (!s || !vptr((u64)s)) return;
    /* Minus map: append the area heart-piece count to the native Skulltula counter's total text. */
    const char *nm = (const char *)regs[0xF0 / 8];
    if (nm && vptr((u64)nm)) stxt_log(nm, s, len);
    if (g_shbcfg.hud_map_hearts && nm && vptr((u64)nm) && str_eq(nm, "T_NumTotal_00") && len < 40) {
        g_map_set_frame = regs[0xF8 / 8];
        u32 sd = 0, st = 0, hd = 0, ht = 0;
        if (collect_counts(&sd, &st, &hd, &ht) && ht) {
            u32 m = 0;
            for (u32 i = 0; i < len && i < 40; i++) g_map_buf[m++] = s[i];
            static const char E[] = "  ";
            for (u32 i = 0; E[i] && m < 47; i++) g_map_buf[m++] = E[i];
            m += put_int(g_map_buf + m, (int)hd);
            g_map_buf[m++] = '/';
            m += put_int(g_map_buf + m, (int)ht);
            g_map_buf[m] = 0;
            ((u64 *)ref)[0] = (u64)g_map_buf;
            ((u64 *)ref)[1] = m;
            s = g_map_buf;
            len = m;
            /* the heart-piece icon added to the counter part next to its total text */
            u64 root = g_map_set_frame;
            u64 hrec = vptr(root) ? pane_record(root, "P_ShbPic_398") : 0;
            g_map_icon_dbg[0] = root; g_map_icon_dbg[1] = hrec; g_map_icon_dbg[2] = 0;
            if (hrec) {
                g_map_icon_dbg[3] = 220;
                place(hrec, 220.0f, -5.0f, 0.66f, 0.66f, 0.0f);
                tint_rec(hrec, 1.0f, 1.0f, 1.0f, 1.0f);
            }
        }
    }
    if (nm && vptr((u64)nm) && str_eq(nm, "T_GuideA_00")) {
        g_aguide_frame = regs[0xF8 / 8];
        u32 i;
        for (i = 0; i < len && i < 47 && s[i]; i++) g_aguide_text[i] = s[i];
        g_aguide_text[i] = 0;
    }
    if (nm && vptr((u64)nm) && nm[0] == 'T' && nm[2] == 'K') {   /* T_Key?_00: the prompt's key glyph */
        const char *g = dc_key_override(nm);
        if (g) { ((u64 *)ref)[0] = (u64)g; ((u64 *)ref)[1] = 3; g_dc_dbg[3]++; }
    }
}
/* Our parts on the Items page get their icon controllers at the end of the page build (main+0xF78BA8, hook 15:
 * x19 = the page panel, its +0x20 the page frame; x20 = the build heap), right after the game's own cells got
 * theirs from that heap (main+0x43E500). Not later: the build heap is spent by then (the HUD's too), the default
 * heap (0) crashes and a heap of ours won't do (it holds textures). Not earlier: during the cells' inits our
 * parts' pane lists are not linked yet, and icon_widget_init walks one (it loops on null), so the same walk is
 * checked here first. */
extern u64 ui_find_sub(const char *name, u64 part, u64 *out);
extern u64 ui_owner_layout(u64 pane);
static int icon_list_ok(u64 part) {
    static u64 sub;
    sub = 0;
    if (!ui_find_sub_safe("Pa_ReplaceIcon_00", part, &sub) || !sub) return 0;
    u64 lay = ui_owner_layout(sub + 0x40);
    if (!lay) return 0;
    layout_repair(lay);
    layout_repair(ptr(sub, 0x80));
    u64 n = ptr(lay, 0x130);
    for (u32 i = 0; i < 64; i++) {
        if (!n) return 0;
        if (n == lay + 0x128) return 0;   /* end: not found (the init would give up anyway) */
        u64 nm = ptr(n, 0x148);
        if (nm && str_eq((const char *)nm, "Pa_ReplaceIcon_00")) return 1;
        n = ptr(n, 8);
    }
    return 0;
}
static u64 g_mb_ctrl_frame;
u32 g_mb_ctrl_dbg[3];   /* controllers built, failed, parts whose list was not ready */
static void hud_menu_ctrl_init(u64 fr, u64 heap) {
    if (!BTNMAP || !fr || !heap || fr == g_mb_ctrl_frame) return;
    if (!find_part_safe((void *)fr, part_name(0))) return;   /* not the page that carries our parts (build_menu) */
    g_mb_ctrl_frame = fr;
    for (u32 k = 0; k < PART_N; k++) {
        g_mb_ctrl[k] = 0;
        u64 p = (u64)find_part_safe((void *)fr, part_name(k));
        if (!p) continue;
        if (!icon_list_ok(p)) { g_mb_ctrl_dbg[2]++; continue; }
        for (u32 i = 0; i < 12; i++) g_mb_widget[k][i] = 0;
        g_mb_widget[k][4] = p;
        g_ctrl_captured = 0; g_ctrl_capture = 1; g_sb_busy = 1;
        icon_widget_init(g_mb_widget[k], (void *)heap);
        g_sb_busy = 0; g_ctrl_capture = 0;
        g_mb_ctrl[k] = g_ctrl_captured;
        g_mb_ctrl_dbg[g_mb_ctrl[k] ? 0 : 1]++;
    }
    for (u32 k = 0; k < PART_N; k++) g_mb[k].part = 0;   /* rebind with the controllers */
}
/* pause menu: the caption panel of the highlighted item (its name string at +0x68, written by the call) */
u64 g_menu_panel, g_menu_cell;
u32 g_menu_sel_n;
extern void button_mapping_on_open(const char *path, int mode) __attribute__((weak));
void hud_on_code(u32 id, void *a0, void *a1, void *a2, void *regs) {
    (void)a1; (void)a2;
    if (id >= 22) return;   /* diagnostic sites (hook_code_frame4/speed4): only counted in g_code_entry */
    if (id == 5) { player_track((u64)a0); return; }
    if (id >= 6 && id <= 9) { player_guard_event(id); return; }
    if (id == 10) { if (stamina_speed_hook) stamina_speed_hook(regs); return; }   /* ground movement: sprint speed */
    if (id == 13) { if (songbook_on_ocarina) songbook_on_ocarina(1, a0); return; }   /* ocarina enter */
    if (id == 14) { if (songbook_on_ocarina) songbook_on_ocarina(0, a0); return; }   /* ocarina leave */
    if (id == 16) { if (songbook_on_ui) songbook_on_ui(a0); return; }                /* ocarina UI update */
    if (id == 11) {
        hud_icon_ctrl_init((u64)a0);
        return;
    }                  /* icon controller init */
    if (id == 12) { hud_slots_init((u64)a0, (u64)a1); g_ready_at = g_tick + 2; return; }  /* PlayerStatus HUD init: build icon controllers */
    if (id == 18) {   /* nn::fs::OpenFile(handle, path, mode) */
        const char *path = (const char *)a1;
        if (button_mapping_on_open && path) button_mapping_on_open(path, (int)(u64)a2);
        return;
    }
    if (id == 15) { hud_menu_ctrl_init(ptr(*(u64 *)((u8 *)regs + 0xA0), 0x20), *(u64 *)((u8 *)regs + 0xA8)); return; }
    if (id == 21) { g_menu_panel = (u64)a0; g_menu_cell = (u64)a1; g_menu_sel_n++; return; }
    if (id == 20) { hud_on_settext((u64)a0, (const u64 *)a1, *(u64 *)((u8 *)regs + 0x08), (const u64 *)regs); return; }
    if (id == 4) {   /* part instance creation: log the pane index being instantiated */
        u64 layout = (u64)a1, pane = (u64)a2;
        if (!vptr(layout)) return;
        u64 arr = ptr(layout, 0x100);
        const u8 *lr = layout ? (const u8 *)ptr(layout, 0xF8) : 0;
        const char *asset = lr ? (const char *)q(lr, 0x18) : 0;
        if (asset && str_eq(asset, "MapMenu_00")) g_map_layout = layout;   /* the corner minimap layout */
        if (!asset || !str_eq(asset, "PlayerStatus_00")) return;
        {   /* child frame object the caller prepared: x0 -> [x0+8] -> [0]; null = creation failed */
            u64 o = (u64)a0, c = 0;
            if (o && vptr(o) && vptr(ptr(o, 8))) c = ptr(ptr(o, 8), 0);
            g_inst_child[(g_inst_n - 0x10000000u) % 32] = c;
        }
        INST_LOG[(g_inst_n - 0x10000000u) % 32] = arr ? (u32)((pane - arr) / 0xB0) : 0xFFFFFFFFu;
        g_inst_n++;
        return;
    }
    if (id == 3) {
        on_set_frame((u64)a0, *(float *)((u8 *)regs + 0xB0));   /* saved d0 low word = s0 */
        return;
    }
    if (id == 1) {
        g_mp = (u64)a0;
        g_frame_node = 0;
        g_ready_at = g_tick + 2;
        for (u32 i = 0; i < SEG_N; i++) { g_seg[i].bar = 0; g_seg[i].init = 0; }
        plates_reset();
        slots_reset();
        g_hud_dbg[0] = MARK | g_mp;
    } else if (id == 0) {
        if (a0 && *(u64 *)a0 == (u64)MAIN_SCHED_VT) {
            hud_frame();
            if (manual_shield_on_frame) manual_shield_on_frame(a0);
            if (songbook_on_frame) songbook_on_frame();
        }
    }
}












