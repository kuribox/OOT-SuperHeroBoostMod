#include "../../core/dispatch.h"
#include "menu.h"

/* In-game settings menu: the graphics side.
 *
 * The game fills its table of NVN entry points in one function (main+0x129E290): it receives the device and the
 * driver's lookup function, and calls lookup(device, "nvnXxx") once per entry. A hook on that function hands it a
 * lookup of our own (im_lookup) that asks the driver for the real function and, for the few entries the menu needs,
 * returns a wrapper of ours instead. Every other entry is passed through untouched.
 *
 *  - nvnWindowBuilderSetTextures: remembers the window's images, so the last command buffer of a frame (the one
 *    that binds a window image) can be told from the scene's.
 *  - nvnCommandBufferSetRenderTargets: in that last command buffer, the second render-target bind is the HDR
 *    buffer the final image is made from. When the menu is open its picture is copied onto that buffer there.
 *  - nvnQueuePresentTexture: marks the frame boundary.
 *
 * The picture is painted on the CPU (menu.c) into a linear image of the buffer's own format (R11G11B10F) that lives
 * in a memory pool over a static buffer of the module, and copied with the GPU's copy engine. Nothing is drawn while
 * the menu is closed: the wrappers then only forward.
 *
 * No pointer lives in data: the real functions are stored at run time as values, the wrappers' addresses are formed
 * pc-relative, and the NVN names are literals in code (one call each, no table). */

typedef void (*GenericFn)(void);
typedef GenericFn (*LookupFn)(u64 device, const char *name);
typedef void (*Fwd8Fn)(u64, u64, u64, u64, u64, u64, u64, u64);
typedef int (*TexIntFn)(const void *);

#define IM_MAGIC 0x494D4E55u   /* non-zero initialiser: keeps the diagnostics in .data, readable from the debug server */

/* ---- state ---------------------------------------------------------------------------------------------------- */

u64 g_im_dev = IM_MAGIC;            /* the device the table was loaded for */
u64 g_im_real_lookup = IM_MAGIC;
static u64 g_real_srt, g_real_present, g_real_wbst;   /* the driver's SetRenderTargets, Present, SetTextures */
static u64 g_tex_w, g_tex_h, g_tex_fmt;               /* nvnTextureGetWidth / Height / Format */
static u64 g_swap[8];                                 /* the window's images */
static u32 g_swap_n;
static u64 g_final_cmd, g_next_final_cmd;             /* the command buffer that binds a window image: last frame's / this one's */
static u32 g_final_n;                                 /* binds seen in it this frame */

/* diagnostics (read with the debug server): all counters start at IM_MAGIC */
u32 g_im_dbg[8] = {IM_MAGIC, IM_MAGIC, IM_MAGIC, IM_MAGIC, IM_MAGIC, IM_MAGIC, IM_MAGIC, IM_MAGIC};
enum { D_FRAMES, D_COPIES, D_INITFAIL, D_FMT, D_MISSING, D_BINDS, D_PW, D_PH };

/* ---- the overlay image ---------------------------------------------------------------------------------------- */

#define IM_STRIDE (IM_MAXW * 4)                 /* bytes per row: a multiple of 256, so any linear-stride alignment holds */
#define IM_POOL_BYTES (4u << 20)                /* 4 MiB >= IM_STRIDE * IM_MAXH */
typedef struct { int x, y, z, w, h, d; } CopyRegion;
typedef struct { unsigned char b[1024]; } __attribute__((aligned(64))) Opaque;

/* Pool storage must be 4 KiB aligned; a bigger alignment would leave a gap in front of .bss, outside the memory the
 * loader maps for it. */
static unsigned char g_mem[IM_POOL_BYTES] __attribute__((aligned(4096)));
static Opaque g_pool, g_pool_b, g_tex, g_tex_b;
static int g_img_state;                         /* 0 none, 1 ready, -1 failed */
static u64 g_api[24];
static u32 g_missing;

enum { A_PB_DEF, A_PB_DEV, A_PB_STO, A_PB_FLG, A_POOL_INIT, A_TB_DEF, A_TB_DEV, A_TB_TGT, A_TB_SZ, A_TB_FMT, A_TB_FLG,
       A_TB_STRIDE, A_TB_STO, A_TEX_INIT, A_COPY, A_BARRIER, A_N };

/* One call per name, with the literal inline: a table of name pointers would need relocations. */
#define RESOLVE(slot, name) do { g_api[slot] = (u64)((LookupFn)g_im_real_lookup)(g_im_dev, name); \
    if (!g_api[slot]) g_missing |= 1u << (slot); } while (0)

static int resolve_api(void) {
    g_missing = 0;
    RESOLVE(A_PB_DEF, "nvnMemoryPoolBuilderSetDefaults");
    RESOLVE(A_PB_DEV, "nvnMemoryPoolBuilderSetDevice");
    RESOLVE(A_PB_STO, "nvnMemoryPoolBuilderSetStorage");
    RESOLVE(A_PB_FLG, "nvnMemoryPoolBuilderSetFlags");
    RESOLVE(A_POOL_INIT, "nvnMemoryPoolInitialize");
    RESOLVE(A_TB_DEF, "nvnTextureBuilderSetDefaults");
    RESOLVE(A_TB_DEV, "nvnTextureBuilderSetDevice");
    RESOLVE(A_TB_TGT, "nvnTextureBuilderSetTarget");
    RESOLVE(A_TB_SZ, "nvnTextureBuilderSetSize2D");
    RESOLVE(A_TB_FMT, "nvnTextureBuilderSetFormat");
    RESOLVE(A_TB_FLG, "nvnTextureBuilderSetFlags");
    RESOLVE(A_TB_STRIDE, "nvnTextureBuilderSetStride");
    RESOLVE(A_TB_STO, "nvnTextureBuilderSetStorage");
    RESOLVE(A_TEX_INIT, "nvnTextureInitialize");
    RESOLVE(A_COPY, "nvnCommandBufferCopyTextureToTexture");
    RESOLVE(A_BARRIER, "nvnCommandBufferBarrier");
    g_im_dbg[D_MISSING] = IM_MAGIC + g_missing;
    return g_missing == 0;
}

typedef void (*V1)(void *);
typedef void (*V2p)(void *, const void *);
typedef void (*V2i)(void *, int);
typedef void (*V3i)(void *, int, int);
typedef void (*Vsto)(void *, void *, u64);
typedef void (*Vstr)(void *, long);
typedef void (*Vtbs)(void *, const void *, long);
typedef u32 (*Rinit)(void *, const void *);
typedef void (*Vcopy)(void *, const void *, const void *, const CopyRegion *, const void *, const void *,
                      const CopyRegion *, int);
typedef void (*Vbar)(void *, int);

/* The pool and the image, built once (on the render thread, at the first copy). The image has the format of the
 * buffer it lands on and the largest size the menu can need; a smaller picture is painted in its top-left corner. */
static int make_image(int fmt) {
    if (g_img_state) return g_img_state > 0;
    if (!resolve_api()) { g_img_state = -1; return 0; }
    ((V1)g_api[A_PB_DEF])(&g_pool_b);
    ((V2p)g_api[A_PB_DEV])(&g_pool_b, (const void *)g_im_dev);
    ((Vsto)g_api[A_PB_STO])(&g_pool_b, g_mem, IM_POOL_BYTES);
    ((V2i)g_api[A_PB_FLG])(&g_pool_b, 0x2 | 0x20);   /* CPU uncached + GPU cached */
    if (!(((Rinit)g_api[A_POOL_INIT])(&g_pool, &g_pool_b) & 0xFF)) { g_im_dbg[D_INITFAIL]++; g_img_state = -1; return 0; }
    ((V1)g_api[A_TB_DEF])(&g_tex_b);
    ((V2p)g_api[A_TB_DEV])(&g_tex_b, (const void *)g_im_dev);
    ((V2i)g_api[A_TB_TGT])(&g_tex_b, 1);             /* 2D */
    ((V3i)g_api[A_TB_SZ])(&g_tex_b, IM_MAXW, IM_MAXH);
    ((V2i)g_api[A_TB_FMT])(&g_tex_b, fmt);
    ((V2i)g_api[A_TB_FLG])(&g_tex_b, 0x10);          /* linear */
    ((Vstr)g_api[A_TB_STRIDE])(&g_tex_b, IM_STRIDE);
    ((Vtbs)g_api[A_TB_STO])(&g_tex_b, &g_pool, 0);
    if (!(((Rinit)g_api[A_TEX_INIT])(&g_tex, &g_tex_b) & 0xFF)) { g_im_dbg[D_INITFAIL]++; g_img_state = -1; return 0; }
    g_img_state = 1;
    return 1;
}

static u64 call_int(u64 fn, u64 tex) {
    if (!fn || !tex) return 0;
    return (u64)(unsigned int)((TexIntFn)fn)((const void *)tex);
}

/* Paints if the menu changed, then copies the picture onto `tex`, centred. */
static void overlay_copy(u64 cmd, u64 tex) {
    int fmt = (int)call_int(g_tex_fmt, tex);
    int w = (int)call_int(g_tex_w, tex), h = (int)call_int(g_tex_h, tex);
    g_im_dbg[D_FMT] = IM_MAGIC + (u32)fmt;
    if (fmt != 0x3F || w < 640 || h < 360) return;       /* only the R11G11B10F buffer is painted for */
    if (!make_image(fmt)) return;
    int pw = 0, ph = 0;
    im_menu_paint((u32 *)g_mem, IM_STRIDE / 4, (h + 240) / 480, &pw, &ph);
    if (pw <= 0 || ph <= 0 || pw > w || ph > h) return;
    g_im_dbg[D_PW] = IM_MAGIC + (u32)pw;
    g_im_dbg[D_PH] = IM_MAGIC + (u32)ph;
    CopyRegion src = {0, 0, 0, pw, ph, 1};
    CopyRegion dst = {(w - pw) / 2, (h - ph) / 2, 0, pw, ph, 1};
    ((Vbar)g_api[A_BARRIER])((void *)cmd, 0x2 | 0x10);
    ((Vcopy)g_api[A_COPY])((void *)cmd, &g_tex, 0, &src, (const void *)tex, 0, &dst, 0x2);
    ((Vbar)g_api[A_BARRIER])((void *)cmd, 0x2 | 0x10);
    g_im_dbg[D_COPIES]++;
}

/* ---- wrappers ------------------------------------------------------------------------------------------------- */

static void wrap_set_render_targets(u64 cmd, u64 count, u64 colors, u64 views, u64 depth, u64 dview, u64 a6, u64 a7) {
    if ((u32)count > 0 && colors) {
        u64 tex = ((const u64 *)colors)[0];
        int swap = 0;
        for (u32 i = 0; i < g_swap_n; i++) if (g_swap[i] == tex) swap = 1;
        if (swap) {
            g_next_final_cmd = cmd;   /* this command buffer binds the window's image: it is the frame's last one */
        } else if (cmd == g_final_cmd) {
            u32 n = g_final_n++;
            if (n == 1 && im_menu_open()) overlay_copy(cmd, tex);
        }
    }
    ((Fwd8Fn)g_real_srt)(cmd, count, colors, views, depth, dview, a6, a7);
}

static void wrap_window_textures(u64 builder, u64 count, u64 textures, u64 a3, u64 a4, u64 a5, u64 a6, u64 a7) {
    const u64 *t = (const u64 *)textures;
    u32 n = (u32)count > 8 ? 8 : (u32)count;
    if (t) for (u32 i = 0; i < n; i++) g_swap[i] = t[i];
    g_swap_n = n;
    ((Fwd8Fn)g_real_wbst)(builder, count, textures, a3, a4, a5, a6, a7);
}

static void wrap_present(u64 queue, u64 window, u64 index, u64 a3, u64 a4, u64 a5, u64 a6, u64 a7) {
    g_im_dbg[D_FRAMES]++;
    g_final_cmd = g_next_final_cmd;
    g_final_n = 0;
    ((Fwd8Fn)g_real_present)(queue, window, index, a3, a4, a5, a6, a7);
}

/* ---- the lookup we hand the game ------------------------------------------------------------------------------ */

static int name_eq(const char *a, const char *b) {
    while (*a && *a == *b) { a++; b++; }
    return *a == *b;
}

static GenericFn im_lookup(u64 device, const char *name) {
    GenericFn real = ((LookupFn)g_im_real_lookup)(device, name);
    if (!real || !name) return real;
    if (name_eq(name, "nvnCommandBufferSetRenderTargets")) { g_real_srt = (u64)real; return (GenericFn)&wrap_set_render_targets; }
    if (name_eq(name, "nvnWindowBuilderSetTextures")) { g_real_wbst = (u64)real; return (GenericFn)&wrap_window_textures; }
    if (name_eq(name, "nvnQueuePresentTexture")) { g_real_present = (u64)real; return (GenericFn)&wrap_present; }
    if (name_eq(name, "nvnTextureGetWidth")) g_tex_w = (u64)real;
    else if (name_eq(name, "nvnTextureGetHeight")) g_tex_h = (u64)real;
    else if (name_eq(name, "nvnTextureGetFormat")) g_tex_fmt = (u64)real;
    return real;
}

/* Hook id 24, the table-loading function's entry: x0 = device, x1 = the driver's lookup. regs: x0 at +0x10, x1 at +0x18. */
void ingame_menu_on_nvn_load(void *regs) {
    u64 *x = (u64 *)((unsigned char *)regs + 0x10);
    if (!x[1] || x[1] == (u64)&im_lookup) return;
    g_im_dev = x[0];
    g_im_real_lookup = x[1];
    x[1] = (u64)&im_lookup;
}
