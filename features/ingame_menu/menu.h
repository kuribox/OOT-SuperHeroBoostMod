#ifndef SHB_IM_MENU_H
#define SHB_IM_MENU_H

#include "../../core/dispatch.h"

/* The in-game settings menu (features/ingame_menu). menu.c owns the state, the input and the painting; ingame_menu.c
 * owns the graphics side and only asks for a picture. */

/* Largest picture the menu can paint (pixels). The image in the graphics pool has exactly this size. */
#define IM_MAXW 1408
#define IM_MAXH 736

/* 1 while the menu is open. */
int im_menu_open(void);

/* Paints the menu into `px` (rows `stride_px` pixels apart, R11G11B10F) when it changed since the last paint or the
 * scale `f` changed, and returns the picture's size in *pw / *ph (always, painted or not). f is the font scale. */
void im_menu_paint(u32 *px, int stride_px, int f, int *pw, int *ph);

#endif
