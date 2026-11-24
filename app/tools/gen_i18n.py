#!/usr/bin/env python3
"""Generate the Rust text table from the Python app's `app-legacy/i18n.py`.

The window must say the same things in the same six languages as the Python app, so the table is generated from
its `TEXTS` dict instead of being retyped: one edit in `i18n.py`, one run here.

    py -3.12 app/tools/gen_i18n.py          # rewrite crates/shb-gui/src/i18n_table.rs
    py -3.12 app/tools/gen_i18n.py --check  # fail when the file is stale (CI)
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

LANGS = ["en", "es", "pt", "fr", "de", "it"]
ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "app" / "crates" / "shb-gui" / "src" / "i18n_table.rs"


def rust_string(text: str) -> str:
    out = text.replace("\\", "\\\\").replace('"', '\\"').replace("\r", "")
    out = out.replace("\n", "\\n").replace("\t", "\\t")
    return f'"{out}"'


def build(app: Path) -> str:
    sys.path.insert(0, str(app))
    # Some pages keep their texts in their own module and merge them into `T` on import (`remap_ui`, the enemy
    # aggression options): the window needs those too, so they are imported before the table is read. tkinter,
    # customtkinter and zstandard are stubbed, since none of them is needed to read a dict (the CI runner has no
    # customtkinter, and without the stub `remap_ui` does not import and its texts would be missing from the table).
    for name in ("tkinter", "tkinter.ttk", "tkinter.font", "customtkinter", "zstandard"):
        if name not in sys.modules:
            try:
                __import__(name)
            except Exception:  # noqa: BLE001 - a missing optional module must not stop the generator
                import types

                stub = types.ModuleType(name)
                stub.__getattr__ = lambda attr: type(attr, (), {})  # type: ignore[assignment]
                sys.modules[name] = stub
    i18n = __import__("i18n")
    for module in ("texts_enemies", "remap_ui"):
        try:
            __import__(module)
        except Exception as exc:  # noqa: BLE001 - say which one, and keep the table usable
            print(f"warning: {module} did not import ({exc}); its texts are missing", file=sys.stderr)
    texts = i18n.T
    keys = sorted(texts["en"])
    lines = [
        "// Generated from app-legacy/i18n.py by app/tools/gen_i18n.py — do not edit by hand.",
        "//",
        "// One row per key: the six languages in the order LANG_CODES. `i18n::tr` binary-searches this table.",
        "",
        '#[allow(dead_code)] // the codes of the columns below, for callers that switch on a language',
        'pub const LANG_CODES: [&str; 6] = ["en", "es", "pt", "fr", "de", "it"];',
        "",
        "pub static TEXTS: &[(&str, [&str; 6])] = &[",
    ]
    for key in keys:
        values = [texts.get(lang, {}).get(key, texts["en"][key]) for lang in LANGS]
        lines.append(f'    ("{key}", [{", ".join(rust_string(v) for v in values)}]),')
    lines.append("];")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--app", type=Path, default=ROOT / "app-legacy")
    ap.add_argument("--check", action="store_true", help="only report whether the file is up to date")
    args = ap.parse_args()

    generated = build(args.app.resolve())
    if args.check:
        current = OUT.read_text(encoding="utf-8") if OUT.is_file() else ""
        if current != generated:
            print(f"{OUT} is stale: run py -3.12 app/tools/gen_i18n.py")
            return 1
        print(f"{OUT} is up to date ({generated.count(chr(10))} lines)")
        return 0
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(generated, encoding="utf-8")
    print(f"wrote {OUT} ({len(generated)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
