#!/usr/bin/env python3
"""Generate the Rust text table from the Python app's `app/i18n.py`.

The window must say the same things in the same six languages as the Python app, so the table is generated from
its `TEXTS` dict instead of being retyped: one edit in `i18n.py`, one run here.

    py -3.12 app-rs/tools/gen_i18n.py          # rewrite crates/shb-gui/src/i18n_table.rs
    py -3.12 app-rs/tools/gen_i18n.py --check  # fail when the file is stale (CI)
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

LANGS = ["en", "es", "pt", "fr", "de", "it"]
ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "app-rs" / "crates" / "shb-gui" / "src" / "i18n_table.rs"


def rust_string(text: str) -> str:
    out = text.replace("\\", "\\\\").replace('"', '\\"').replace("\r", "")
    out = out.replace("\n", "\\n").replace("\t", "\\t")
    return f'"{out}"'


def build(app: Path) -> str:
    sys.path.insert(0, str(app))
    i18n = __import__("i18n")
    texts = i18n.T
    keys = sorted(texts["en"])
    lines = [
        "// Generated from app/i18n.py by app-rs/tools/gen_i18n.py — do not edit by hand.",
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
    ap.add_argument("--app", type=Path, default=ROOT / "app")
    ap.add_argument("--check", action="store_true", help="only report whether the file is up to date")
    args = ap.parse_args()

    generated = build(args.app.resolve())
    if args.check:
        current = OUT.read_text(encoding="utf-8") if OUT.is_file() else ""
        if current != generated:
            print(f"{OUT} is stale: run py -3.12 app-rs/tools/gen_i18n.py")
            return 1
        print(f"{OUT} is up to date ({generated.count(chr(10))} lines)")
        return 0
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(generated, encoding="utf-8")
    print(f"wrote {OUT} ({len(generated)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
