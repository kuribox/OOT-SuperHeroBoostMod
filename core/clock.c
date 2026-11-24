#include "clock.h"

unsigned int g_clock_dbg[5] = {0, 0, 0, 0, 0};   /* ok, last frame us, accumulated ms, counter frequency (kHz), fps */

static unsigned long long s_last;      /* CNTPCT_EL0 at the previous tick; 0 = no previous one */
static unsigned long long s_us_acc;    /* accumulated microseconds (kept whole to avoid drift) */
static unsigned int s_us;              /* duration of the last frame that looked like one */
static unsigned int s_hz;              /* counter frequency the hardware reports (0 = not read yet) */
static unsigned int s_ok;              /* the counter has produced at least one real frame */

static unsigned long long cntpct(void) {
    unsigned long long v;
    __asm__ volatile("mrs %0, cntpct_el0" : "=r"(v));
    return v;
}

static unsigned long long cntfrq(void) {
    unsigned long long v;
    __asm__ volatile("mrs %0, cntfrq_el0" : "=r"(v));
    return v;
}

/* The Switch's counter is 19.2 MHz, but what the hardware reports is what the emulators run the
 * guest at (Fritchii reports 31.25 MHz), and trusting the constant there makes every duration 63%
 * off. Read CNTFRQ_EL0 once; a value that is not a sane frequency means the register is not
 * implemented on this target, so keep the documented 19.2 MHz. */
static unsigned int freq_hz(void) {
    unsigned long long f;
    if (s_hz) return s_hz;
    f = cntfrq();
    s_hz = (f >= 1000000ull && f <= 1000000000ull) ? (unsigned int)f : SHB_CLOCK_HZ;
    return s_hz;
}

void shb_clock_tick(void) {
    unsigned long long now = cntpct();
    if (s_last != 0) {
        unsigned long long d = now - s_last;                    /* 64-bit wrap is harmless */
        unsigned int hz = freq_hz();
        unsigned long long us64 = (d * 1000000ull) / (unsigned long long)hz;
        unsigned int us = us64 > 0xFFFFFFFFull ? 0xFFFFFFFFu : (unsigned int)us64;
        /* A real frame lasts 1..100 ms. Anything else is a hitch or a counter that is not the
         * console's: keep the previous duration (or stay uncalibrated, i.e. 1/60 per frame). */
        if (us >= 1000u && us <= 100000u) {
            s_us = us;
            s_us_acc += us;
            s_ok = 1;
        }
    }
    s_last = now;
    g_clock_dbg[0] = s_ok;
    g_clock_dbg[1] = s_us;
    g_clock_dbg[2] = (unsigned int)(s_us_acc / 1000ull);
    g_clock_dbg[3] = freq_hz() / 1000u;
    g_clock_dbg[4] = shb_frames(60);      /* frames the game runs per second, as the clock sees it */
}

unsigned int shb_clock_ok(void) { return s_ok; }
unsigned int shb_clock_us(void) { return s_ok ? s_us : 0u; }
unsigned int shb_clock_ms(void) { return (unsigned int)(s_us_acc / 1000ull); }

float shb_dt_frame(void) {
    return s_ok ? (float)s_us * 1e-6f : SHB_DT_FALLBACK;
}

unsigned int shb_frames(unsigned int at60) {
    unsigned int n;
    if (!s_ok) return at60;                       /* no clock: exactly the old behaviour */
    /* a 60 Hz frame lasts 16666.7 us: the same duration at the frame rate the game is running at */
    n = (unsigned int)((float)at60 * 16666.666f / (float)s_us + 0.5f);
    return n ? n : 1u;
}
