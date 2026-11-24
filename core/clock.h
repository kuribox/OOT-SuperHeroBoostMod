#ifndef SHB_CLOCK_H
#define SHB_CLOCK_H

/* Real frame time, from the console's own counter (CNTPCT_EL0).
 *
 * The mod counts frames: `hud_frame_count()` advances once per rendered frame, and the features
 * used to assume every frame is 1/60 s. The game does not always run at 60 (the intro lifts its
 * cap on some builds; Longshot with vsync off presented ~200 fps in a crash report), so a duration
 * in frames is only a duration in time when the frame rate is 60.
 *
 * CNTPCT_EL0 is the clock the game itself uses: `sdk` reads it 45 times (the
 * `nn::os::GetSystemTick` path), so every target that runs the game at all translates this
 * instruction — including the static recompilations, which lift `sdk` too. We do not call any
 * import for it: it is one instruction, nothing to hook. The rate comes from CNTFRQ_EL0 when the
 * hardware reports a sane one (19.2 MHz on the console, 31.25 MHz in Fritchii: the constant alone
 * puts every duration 63% off there), with 19.2 MHz as the fallback.
 *
 * Everything is defensive: if the counter is missing, frozen or implausible, `shb_clock_ok()` is 0
 * and every helper falls back to the old 1/60-per-frame behaviour, so a target without the counter
 * behaves exactly like before. */

#define SHB_CLOCK_HZ 19200000u          /* CNTPCT_EL0: nn::os::GetSystemTickFrequency() on Switch */
#define SHB_DT_FALLBACK (1.0f / 60.0f)  /* what a frame lasted before this existed */

/* Call once per rendered frame (core/hud.c, hud_frame), next to the frame counter. */
void shb_clock_tick(void);

/* 1 once the counter has produced at least one plausible frame duration. */
unsigned int shb_clock_ok(void);

/* Duration of the last rendered frame, microseconds (0 before the first tick). */
unsigned int shb_clock_us(void);

/* Duration of the last rendered frame in seconds: the real one, or 1/60 when there is no clock. */
float shb_dt_frame(void);

/* Accumulated real time since the module started, milliseconds (diagnostics). */
unsigned int shb_clock_ms(void);

/* Convert a duration written in 60 Hz frames into the frames the game is actually running at:
 * `shb_frames(18)` is 0.3 s whatever the frame rate. At 60 fps it returns exactly `at60`. */
unsigned int shb_frames(unsigned int at60);

/* Diagnostics (read live): { ok, last frame us, accumulated ms, counter frequency (kHz), fps }. */
extern unsigned int g_clock_dbg[5];

#endif
