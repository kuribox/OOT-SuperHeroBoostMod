"""Modern widget kit for the companion app (CustomTkinter).

The app's logic and layout stay in :mod:`main`; this module supplies the visual layer: a dark forest palette,
rounded cards and buttons, styled selects, switches and entries, the scrolling page, and the help window.

The wrappers keep the tkinter call shapes the rest of the app already uses (``bg``, ``fg``, ``highlightbackground``,
``.config``, ``.current``, ``.bind("<<ComboboxSelected>>")``) so a widget can be swapped without rewriting its
call site. Text labels remain tkinter labels: their wrapping is the most predictable part of tkinter, and colour and
font are enough there.
"""
from __future__ import annotations

import tkinter as tk

import customtkinter as ctk

# --------------------------------------------------------------------------- palette
BG = "#0A1410"
SIDEBAR = "#0C1A13"
PANEL = "#13261C"
PANEL2 = "#0C1A14"
CARD = "#162B20"
BORDER = "#2A4836"
ACCENT = "#E7BA4B"
ACCENT2 = "#F2D27A"
ACCENT_DARK = "#7A5C1F"
TEAL = "#63C79A"
TEXT = "#F5EFDD"
DIM = "#A8B8AA"
DISABLED = "#6B7C70"
RED = "#E06A5A"
AMBER = "#F2B45E"
HELP_BG = "#2B2412"
ROW_HOVER = "#24402E"

FAM = "Segoe UI"
DISPLAY = "Palatino Linotype"
HELP_TITLE = "Ayuda"
HELP_HANDLER = None


def set_help_handler(handler) -> None:
    """Register the inline help strip: HelpIcon sends the text there instead of opening a window."""
    global HELP_HANDLER
    HELP_HANDLER = handler


def set_fonts(fam: str, display: str) -> None:
    global FAM, DISPLAY
    FAM, DISPLAY = fam, display


def setup() -> None:
    ctk.set_appearance_mode("dark")
    try:
        ctk.set_default_color_theme("dark-blue")
    except Exception:
        pass
    # The app still calls .config on some widgets (tkinter's spelling). CustomTkinter implements .configure only;
    # alias it so the visual layer stays a drop-in.
    for cls in (ctk.CTk, ctk.CTkFrame, ctk.CTkLabel, ctk.CTkButton, ctk.CTkEntry, ctk.CTkCheckBox,
                ctk.CTkRadioButton, ctk.CTkOptionMenu, ctk.CTkProgressBar, ctk.CTkTextbox,
                ctk.CTkScrollableFrame, ctk.CTkToplevel):
        try:
            cls.config = cls.configure
        except Exception:
            pass


def _parent_bg(parent) -> str:
    for key in ("bg", "fg_color"):
        try:
            value = parent.cget(key)
            if isinstance(value, str) and value:
                return value
        except Exception:
            pass
    return PANEL


def _strip(kw: dict) -> dict:
    for k in ("bd", "relief", "insertbackground", "activebackground", "activeforeground", "selectcolor",
              "highlightcolor", "highlightthickness", "highlightbackground", "padx", "pady", "takefocus",
              "overrelief", "offrelief", "troughcolor", "indicatoron", "indicator_color", "border_spacing"):
        kw.pop(k, None)
    return kw


# --------------------------------------------------------------------------- frames and labels
class Frame(ctk.CTkFrame):
    """``tk.Frame``-shaped CTk frame: ``bg`` colours the surface, a 1 px border is optional."""

    def __init__(self, parent, bg=None, highlightbackground=None, highlightthickness=None, radius=0, **kw):
        fg = bg or kw.pop("fg_color", PANEL)
        width = kw.pop("width", None)
        height = kw.pop("height", 0)
        bw = int(highlightthickness if highlightthickness is not None else 0)
        _strip(kw)
        super().__init__(parent, fg_color=fg, border_width=bw, border_color=highlightbackground or BORDER,
                         corner_radius=radius, **({"width": width} if width else {}),
                         height=height, **kw)


class Card(ctk.CTkFrame):
    """A rounded card with a quiet gold line across its top."""

    def __init__(self, parent, **kw):
        super().__init__(parent, fg_color=PANEL, border_width=1, border_color=BORDER, corner_radius=16,
                         height=kw.pop("height", 0), **{k: v for k, v in kw.items() if k == "width"})
        self.accent = ctk.CTkFrame(self, height=3, fg_color=ACCENT, corner_radius=2)
        self.accent.pack(fill="x", padx=16, pady=(12, 0))


class Label(tk.Label):
    """A tk label with the app's palette, so wrapping stays reliable."""

    def __init__(self, parent, text="", bg=None, fg=TEXT, font=None, size=10, bold=False, italic=False,
                 wrap=760, anchor="w", justify="left", **kw):
        bg = bg or _parent_bg(parent)
        fg = kw.pop("color", fg)
        wrap = kw.pop("wraplength", wrap)
        if font is None:
            style = "bold" if bold else ("italic" if italic else "normal")
            font = (FAM, size, style)
        super().__init__(parent, text=text, bg=bg, fg=fg, font=font, anchor=anchor, justify=justify,
                         wraplength=wrap, **kw)


class Chip(ctk.CTkLabel):
    """A small rounded status chip."""

    def __init__(self, parent, text="", bg=None, fg=None, **kw):
        super().__init__(parent, text=text, fg_color=bg or PANEL2, text_color=fg or ACCENT,
                         corner_radius=12, border_width=1, border_color=BORDER,
                         font=(FAM, 9, "bold"), height=26, padx=10, **kw)

    def configure(self, **kw):
        if "fg" in kw:
            kw["text_color"] = kw.pop("fg")
        if "bg" in kw:
            kw["fg_color"] = kw.pop("bg")
        return super().configure(**kw)

    config = configure


class ChoiceCard(ctk.CTkFrame):
    """A large selectable card with a left-aligned title and description."""

    def __init__(self, parent, title, desc="", command=None, desc_wrap=520, title_size=10,
                 help_text=None, **kw):
        super().__init__(parent, fg_color=PANEL2, border_width=1, border_color=BORDER, corner_radius=12,
                         height=kw.pop("height", 0), **{k: v for k, v in kw.items() if k == "width"})
        self._command = command
        self._enabled_cmd = command
        self._active = False
        self.head = tk.Frame(self, bg=PANEL2)
        self.head.pack(fill="x", padx=14, pady=(8, 0))
        self.title = tk.Label(self.head, text=title, bg=PANEL2, fg=TEXT, font=(FAM, title_size, "bold"),
                              anchor="w", justify="left")
        self.title.pack(side="left")
        self.help = None
        if help_text:
            self.help = HelpIcon(self.head, help_text, title)
            self.help.pack(side="right")
        self.desc = tk.Label(self, text=desc, bg=PANEL2, fg=DIM, font=(FAM, 8), anchor="w",
                             justify="left", wraplength=desc_wrap)
        self.desc.pack(fill="x", padx=14, pady=(1, 8))
        for w in (self, self.head, self.title, self.desc):
            try:
                w.configure(cursor="hand2")
            except Exception:
                pass
            w.bind("<Button-1>", self._click)

    def _click(self, _e=None):
        if self._command:
            self._command()

    def set_active(self, active: bool):
        self._active = active
        fg = ACCENT if active else PANEL2
        border = ACCENT if active else BORDER
        title = "#1A1408" if active else TEXT
        desc = "#3A2C08" if active else DIM
        try:
            self.configure(fg_color=fg, border_color=border)
            self.head.configure(bg=fg)
            self.title.configure(bg=fg, fg=title)
            self.desc.configure(bg=fg, fg=desc)
            if self.help is not None:
                dark = "#1A1408"
                self.help.set_theme(fg, dark if active else ACCENT,
                                    dark if active else ACCENT2)
        except Exception:
            pass

    def set_enabled(self, enabled: bool):
        self._command = self._enabled_cmd if enabled else None
        try:
            self.configure(cursor="hand2" if enabled else "arrow")
            if self._active:
                self.title.configure(fg="#1A1408")
                self.desc.configure(fg="#3A2C08")
                if self.help is not None:
                    dark = "#1A1408"
                    self.help.set_theme(ACCENT, dark if enabled else DISABLED,
                                        dark if enabled else DISABLED)
            else:
                self.title.configure(fg=TEXT if enabled else DISABLED)
                self.desc.configure(fg=DIM if enabled else DISABLED)
                if self.help is not None:
                    self.help.set_theme(PANEL2, ACCENT if enabled else DISABLED,
                                        ACCENT2 if enabled else DISABLED)
        except Exception:
            pass


# --------------------------------------------------------------------------- controls
class Button(ctk.CTkButton):
    def __init__(self, parent, text="", cmd=None, primary=False, ghost=False, danger=False, **kw):
        fg = ACCENT if primary else ("transparent" if ghost else PANEL2)
        hover = ACCENT2 if primary else (BORDER if not ghost else PANEL)
        fg_text = "#1A1408" if primary else (RED if danger else TEXT)
        _strip(kw)
        super().__init__(parent, text=text, command=cmd, fg_color=fg, hover_color=hover,
                         text_color=fg_text, text_color_disabled=DISABLED, font=(FAM, 10, "bold"),
                         corner_radius=10, height=34, border_width=0 if primary or ghost else 1,
                         border_color=BORDER, **kw)
        self._on_fg, self._on_hover, self._on_text = fg, hover, fg_text

    def configure(self, **kw):
        if "state" in kw:
            state = kw.pop("state")
            if state == "disabled":
                super().configure(state="disabled", fg_color=PANEL, hover_color=PANEL, text_color=DISABLED,
                                  border_color=BORDER)
            else:
                super().configure(state="normal", fg_color=self._on_fg, hover_color=self._on_hover,
                                  text_color=self._on_text)
        return super().configure(**kw)

    config = configure


class Entry(ctk.CTkEntry):
    def __init__(self, parent, textvariable=None, bg=None, fg=None, font=None, highlightbackground=None,
                 highlightthickness=None, **kw):
        _strip(kw)
        kw.setdefault("placeholder_text_color", DIM)
        super().__init__(parent, textvariable=textvariable, fg_color=bg or PANEL2, text_color=fg or TEXT,
                         font=font or (FAM, 10), border_width=1, border_color=highlightbackground or BORDER,
                         corner_radius=9, height=36, **kw)


class Check(ctk.CTkCheckBox):
    def __init__(self, parent, text="", var=None, bg=None, **kw):
        if var is None and "variable" in kw:
            var = kw.pop("variable")
        else:
            kw.pop("variable", None)
        font = kw.pop("font", (FAM, 10))
        kw.pop("fg", None)
        width = kw.pop("width", 1)
        _strip(kw)
        super().__init__(parent, text=text, variable=var, fg_color=ACCENT, hover_color=ACCENT2,
                         border_color=BORDER, checkmark_color="#1A1408", text_color=TEXT,
                         text_color_disabled=DISABLED, font=font, corner_radius=5, width=width,
                         checkbox_width=20, checkbox_height=20, **kw)
        try:   # the box sits a hair high next to Segoe UI's text; nudge it to the visual centre
            self._canvas.grid_configure(pady=(0, 2))
        except Exception:
            pass

    def configure(self, **kw):
        if "state" in kw:
            state = kw.pop("state")
            if state == "disabled":
                super().configure(state="disabled", fg_color=BORDER, hover_color=BORDER,
                                  border_color=BORDER, text_color=DISABLED)
            else:
                super().configure(state="normal", fg_color=ACCENT, hover_color=ACCENT2,
                                  border_color=BORDER, text_color=TEXT)
        return super().configure(**kw)

    config = configure


class Radio(ctk.CTkRadioButton):
    def __init__(self, parent, text="", var=None, value=0, cmd=None, bg=None, **kw):
        if var is None and "variable" in kw:
            var = kw.pop("variable")
        else:
            kw.pop("variable", None)
        font = kw.pop("font", (FAM, 10))
        kw.pop("fg", None)
        width = kw.pop("width", 1)
        _strip(kw)
        super().__init__(parent, text=text, variable=var, value=value, command=cmd, fg_color=ACCENT,
                         hover_color=ACCENT2, border_color=BORDER, text_color=TEXT, text_color_disabled=DISABLED,
                         font=font, width=width, radiobutton_width=18, radiobutton_height=18, corner_radius=9, **kw)
        try:   # same optical nudge as the checkbox: the ring should centre on the text, not on its line box
            self._canvas.grid_configure(pady=(0, 2))
        except Exception:
            pass

    def configure(self, **kw):
        if "state" in kw:
            state = kw.pop("state")
            if state == "disabled":
                super().configure(state="disabled", fg_color=BORDER, hover_color=BORDER,
                                  border_color=BORDER, text_color=DISABLED)
            else:
                super().configure(state="normal", fg_color=ACCENT, hover_color=ACCENT2,
                                  border_color=BORDER, text_color=TEXT)
        return super().configure(**kw)

    config = configure


class Select(ctk.CTkOptionMenu):
    """A drop-down with the small part of the ttk.Combobox API the app uses.

    The list is a small borderless popup of our own (the native menu draws a white edge that cannot be removed). It
    grabs the pointer while it is open: the wheel scrolls the list and never the page, a click outside only closes
    it (nothing behind receives it), and Up/Down/Enter/Escape work."""

    def __init__(self, parent, values, var, width=16, **kw):
        self._values = list(values or [""]) or [""]
        self._var = var
        self._cb = None
        self._pending = None
        if var.get() not in self._values:
            var.set(self._values[0])
        _strip(kw)
        super().__init__(parent, values=self._values, variable=var, width=max(150, int(width) * 9),
                         height=34, corner_radius=8, fg_color=PANEL2, button_color=PANEL2,
                         button_hover_color=BORDER, text_color=TEXT, text_color_disabled=DISABLED,
                         dropdown_fg_color=PANEL, dropdown_hover_color=ROW_HOVER, dropdown_text_color=TEXT,
                         font=(FAM, 10), dropdown_font=(FAM, 10), command=self._chosen, **kw)
        self._popup = None

    def _chosen(self, _value=None):
        if self._cb is not None:
            self._cb(None)

    def _dropdown_callback(self, value):
        # The variable is set inside the parent callback, and the app's trace may redraw the table before the
        # command runs. current() reports the clicked value meanwhile.
        self._pending = value
        try:
            super()._dropdown_callback(value)
        finally:
            self._pending = None

    def bind(self, seq, func, add=None):
        if seq == "<<ComboboxSelected>>":
            self._cb = func
            return None
        return super().bind(seq, func, add)

    def current(self, idx=None):
        if idx is None:
            value = self._pending if self._pending is not None else self._var.get()
            return self._values.index(value) if value in self._values else 0
        if 0 <= idx < len(self._values):
            self._var.set(self._values[idx])

    def get(self):
        return self._var.get()

    def set(self, value):
        if value in self._values:
            self._var.set(value)

    def configure(self, **kw):
        if "values" in kw:
            self._values = list(kw.pop("values") or [""]) or [""]
            if self._var.get() not in self._values:
                self._var.set(self._values[0])
            kw["values"] = self._values
        if "state" in kw:
            state = kw.pop("state")
            kw["state"] = "disabled" if state == "disabled" else "normal"
        return super().configure(**kw)

    config = configure


    # ---- the list ----
    def _clicked(self, _event=0):
        if self._state == "disabled":
            return
        if self._popup is not None:
            self._close_popup()
        else:
            self._open_popup()

    def _open_popup(self):
        self._close_popup()
        n = len(self._values)
        row_h = 28
        shown = min(n, 12)
        width = max(self.winfo_width(), 150)
        top = tk.Toplevel(self)
        top.withdraw()
        top.overrideredirect(True)
        top.configure(bg=BORDER)
        try:
            top.attributes("-topmost", True)
        except Exception:
            pass
        box = tk.Listbox(top, bg=PANEL, fg=TEXT, selectbackground=ROW_HOVER, selectforeground=ACCENT2,
                         activestyle="none", relief="flat", borderwidth=0, highlightthickness=0,
                         exportselection=False, font=(FAM, 10), height=shown, takefocus=0)
        for v in self._values:
            box.insert("end", "  " + v)
        sb = None
        if n > shown:
            sb = ctk.CTkScrollbar(top, width=12, fg_color=PANEL, button_color=BORDER,
                                  button_hover_color=ACCENT_DARK, command=box.yview)
            box.configure(yscrollcommand=sb.set)
            sb.pack(side="right", fill="y", padx=(0, 1), pady=1)
        box.pack(side="left", fill="both", expand=True, padx=1, pady=1)
        cur = self._values.index(self._var.get()) if self._var.get() in self._values else 0
        box.selection_set(cur)
        box.see(cur)
        top.update_idletasks()
        height = box.winfo_reqheight() + 2 + (0 if n > shown else 0)
        x, y = self.winfo_rootx(), self.winfo_rooty() + self.winfo_height() + 2
        if y + height > self.winfo_screenheight() - 40:                 # no room below: open upwards
            y = max(0, self.winfo_rooty() - height - 2)
        top.geometry("%dx%d+%d+%d" % (width, height, x, y))
        top.deiconify()
        top.lift()
        self._popup = top
        self._box = box

        def inside(e):
            return top.winfo_rootx() <= e.x_root < top.winfo_rootx() + top.winfo_width() and \
                top.winfo_rooty() <= e.y_root < top.winfo_rooty() + top.winfo_height()

        def motion(e):
            i = box.nearest(e.y)
            box.selection_clear(0, "end")
            box.selection_set(i)

        def click(e):
            if not inside(e):
                self._close_popup()
            elif e.widget is box:
                self._pick(box.nearest(e.y))
            return "break"

        def wheel(e):
            box.yview_scroll(-1 if e.delta > 0 else 1, "units")
            if sb is not None:
                sb.set(*box.yview())
            return "break"

        def key(e):
            sel = (box.curselection() or (cur,))[0]
            if e.keysym in ("Up", "Down"):
                sel = max(0, min(n - 1, sel + (-1 if e.keysym == "Up" else 1)))
                box.selection_clear(0, "end")
                box.selection_set(sel)
                box.see(sel)
            elif e.keysym in ("Return", "KP_Enter"):
                self._pick(sel)
            elif e.keysym == "Escape":
                self._close_popup()
            return "break"

        for w in (top, box):
            w.bind("<Button-1>", click)
            w.bind("<MouseWheel>", wheel)
            w.bind("<Key>", key)
        box.bind("<Motion>", motion)
        try:
            top.focus_force()
            box.focus_set()
            top.grab_set()
        except Exception:
            pass
        top.bind("<FocusOut>", lambda e: self.after(80, self._focus_lost), add="+")

    def _focus_lost(self):
        if self._popup is not None and self._popup.focus_displayof() is None:
            self._close_popup()

    def _pick(self, idx):
        value = self._values[idx] if 0 <= idx < len(self._values) else None
        self._close_popup()
        if value is not None:
            self._dropdown_callback(value)

    def _close_popup(self):
        top, self._popup = self._popup, None
        if top is not None:
            try:
                top.grab_release()
            except Exception:
                pass
            try:
                top.destroy()
            except Exception:
                pass



class Progress(ctk.CTkProgressBar):
    """A progress bar that accepts both 0..1 and 0..100 values."""

    def __init__(self, parent, **kw):
        kw.pop("style", None)
        kw.pop("maximum", None)
        value = kw.pop("value", 0)
        super().__init__(parent, height=12, corner_radius=6, fg_color=PANEL2, progress_color=ACCENT,
                         border_width=0, **kw)
        self.set(value)

    def set(self, value):
        if value is None:
            return
        try:
            value = float(value)
        except (TypeError, ValueError):
            return
        if value > 1.0:
            value /= 100.0
        super().set(max(0.0, min(1.0, value)))

    def configure(self, **kw):
        if "value" in kw:
            self.set(kw.pop("value"))
        kw.pop("maximum", None)
        kw.pop("style", None)
        return super().configure(**kw)

    config = configure

    def __setitem__(self, key, value):
        if key == "value":
            self.set(value)
        else:
            super().__setitem__(key, value)


class Slider(ctk.CTkFrame):
    """A percent slider with its value beside it: an IntVar moved in fixed steps between lo and hi."""

    def __init__(self, parent, var, lo, hi, step, width=260, bg=None, **kw):
        _strip(kw)
        super().__init__(parent, fg_color=bg or _parent_bg(parent), corner_radius=0, **kw)
        self.var, self.lo, self.step = var, lo, step
        self.value = tk.Label(self, text="", font=(FAM, 10, "bold"), fg=ACCENT, bg=bg or _parent_bg(parent),
                              width=5, anchor="w")
        self.slider = ctk.CTkSlider(self, from_=lo, to=hi, number_of_steps=max(1, (hi - lo) // step), width=width,
                                    fg_color=PANEL2, progress_color=ACCENT, button_color=ACCENT,
                                    button_hover_color=ACCENT2, command=self._moved)
        self.slider.set(var.get())
        self.slider.pack(side="left")
        self.value.pack(side="left", padx=(10, 0))
        self._show()
        var.trace_add("write", lambda *a: self._sync())

    def _moved(self, v):
        v = int(round((float(v) - self.lo) / self.step)) * self.step + self.lo
        if v != self.var.get():
            self.var.set(v)
        self._show()

    def _sync(self):
        try:
            v = int(self.var.get())
        except (tk.TclError, ValueError):
            return
        if int(round(self.slider.get())) != v:
            self.slider.set(v)
        self._show()

    def _show(self):
        try:
            self.value.configure(text="%d%%" % int(self.var.get()))
        except (tk.TclError, ValueError):
            pass

    def configure(self, **kw):
        if "state" in kw:
            self.slider.configure(state=kw.pop("state"))
        return super().configure(**kw) if kw else None

    config = configure


class Textbox(ctk.CTkTextbox):
    def __init__(self, parent, bg=None, fg=None, font=None, wrap="none", height=120, state="normal", **kw):
        for k in ("highlightthickness", "highlightbackground", "highlightcolor", "insertbackground", "relief"):
            kw.pop(k, None)
        if isinstance(height, int) and height < 40:   # call sites still say "6 lines"
            height *= 18
        super().__init__(parent, fg_color=bg or PANEL2, text_color=fg or DIM, font=font or (FAM, 8),
                         wrap=wrap, height=height, border_width=1, border_color=BORDER, corner_radius=8, **kw)
        if state != "normal":
            self.configure(state=state)


class Toplevel(ctk.CTkToplevel):
    def __init__(self, parent, bg=None, **kw):
        super().__init__(parent, fg_color=bg or BG, **kw)

    def configure(self, **kw):
        if "bg" in kw:
            kw["fg_color"] = kw.pop("bg")
        return super().configure(**kw)

    config = configure


# --------------------------------------------------------------------------- scrolling page
class ScrollArea:
    """The app's scrolling content, on CustomTkinter's own scrollable frame."""

    def __init__(self, parent, bg=BG):
        self.frame = Frame(parent, bg=bg)
        self.inner = ctk.CTkScrollableFrame(self.frame, fg_color=bg, corner_radius=0,
                                            scrollbar_fg_color=bg, scrollbar_button_color=BORDER,
                                            scrollbar_button_hover_color=ACCENT_DARK)
        self.inner.pack(fill="both", expand=True)
        self.canvas = getattr(self.inner, "_parent_canvas", None)

    def refresh(self):
        try:
            self.inner.update_idletasks()
        except Exception:
            pass

    def destroy(self):
        try:
            self.frame.destroy()
        except Exception:
            pass


# --------------------------------------------------------------------------- help
def _help_text(text) -> str:
    return text() if callable(text) else str(text or "")


def _show_help(title, text) -> None:
    if HELP_HANDLER is not None and text:
        HELP_HANDLER(_help_text(title) or HELP_TITLE, _help_text(text))


class HelpBanner(ctk.CTkFrame):
    """A slim inline strip that shows an explanation above the page. No popup, no window."""

    def __init__(self, parent, **kw):
        super().__init__(parent, fg_color=HELP_BG, border_width=2, border_color=ACCENT_DARK,
                         corner_radius=10, height=0, **kw)
        self.accent = ctk.CTkFrame(self, width=4, height=1, fg_color=ACCENT, corner_radius=2)
        self.accent.pack(side="left", fill="y", padx=(10, 0), pady=10)
        body = tk.Frame(self, bg=HELP_BG)
        body.pack(side="left", fill="x", expand=True, padx=10, pady=8)
        self.title = tk.Label(body, text="", bg=HELP_BG, fg=ACCENT2, font=(DISPLAY, 12, "bold"), anchor="w")
        self.title.pack(anchor="w")
        self.text = tk.Label(body, text="", bg=HELP_BG, fg=TEXT, font=(FAM, 9), anchor="w",
                             justify="left", wraplength=880)
        self.text.pack(anchor="w", pady=(2, 0))
        close = tk.Label(self, text="×", bg=HELP_BG, fg=DIM, font=(FAM, 13, "bold"), cursor="hand2")
        close.pack(side="right", anchor="n", padx=10, pady=6)
        close.bind("<Button-1>", lambda e: self.hide())
        close.bind("<Enter>", lambda e: close.configure(fg=TEXT))
        close.bind("<Leave>", lambda e: close.configure(fg=DIM))

    def show(self, title, text):
        self.title.configure(text=title or HELP_TITLE)
        self.text.configure(text=text)
        self.text.configure(wraplength=max(420, self.winfo_toplevel().winfo_width() - 420))

    def hide(self):
        self.pack_forget()


class HelpIcon(tk.Canvas):
    """A quiet gold "?" that opens the inline help strip."""

    def __init__(self, parent, text, title=None, bg=None, **kw):
        self._text = text
        self._title = title
        self._bg = bg or _parent_bg(parent)
        self._fg = ACCENT
        self._hover = ACCENT2
        super().__init__(parent, width=16, height=20, bg=self._bg,
                         highlightthickness=0, cursor="hand2", **kw)
        self._draw()
        self.bind("<Button-1>", self._open)
        self.bind("<Enter>", lambda e: self._paint(self._hover))
        self.bind("<Leave>", lambda e: self._paint(self._fg))

    def _paint(self, color):
        self.delete("all")
        self.create_text(8, 11, text="?", fill=color, font=(FAM, 10, "bold"))

    def _draw(self):
        self._paint(self._fg)

    def set_theme(self, bg, fg=ACCENT, hover=ACCENT2):
        self._bg = bg
        self._fg = fg
        self._hover = hover
        self.configure(bg=bg)
        self._draw()

    def _open(self, _e=None):
        if HELP_HANDLER is None:
            return
        text = _help_text(self._text)
        if not text:
            return
        title = _help_text(self._title) if self._title is not None else HELP_TITLE
        HELP_HANDLER(title, text)
