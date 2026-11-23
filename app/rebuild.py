#!/usr/bin/env python3
"""Rebuild or re-apply the mod on a recompiled game or a Rekarina install, using this app's own code.

The build itself is not something the Rust window reimplements: `recomp.build` drives the Longshot/Hookshot builder
and `rekarina.build` drives Rekarina's own lift and compiler, with everything they need (toolchains, PGO dirs, env).
This entry point exists so another program — `shb rebuild` — can ask for exactly the same work and read its progress.

    py -3.12 app/rebuild.py --game "F:/Switch2/Longshot/rc" --profile recommended
    py -3.12 app/rebuild.py --game "F:/Switch2/rekarina" --profile souls --rebuild
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="Rebuild the mod into a recompiled game or a Rekarina install")
    ap.add_argument("--game", required=True, help="the game folder (recompiled game, Rekarina, or its parent)")
    ap.add_argument("--profile", default="recommended")
    ap.add_argument("--no-rebuild", action="store_true",
                    help="only patch the settings (the code stays as it was built)")
    ap.add_argument("--builder", default=None, help="the builder executable, when it is not the default one")
    a = ap.parse_args(argv)

    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import installer as I
    import presets
    import recomp
    import rekarina

    settings = I.Settings(**presets.settings_kwargs(a.profile))
    game = Path(a.game).expanduser()
    rebuild = not a.no_rebuild

    def progress(done, total=None):
        if total:
            print(f"  {done * 100 // max(1, total)}%", flush=True)

    def log(line):
        print(line, flush=True)

    try:
        if recomp.find_game(game) is not None:
            result = recomp.install(game, settings, rebuild=rebuild, builder_path=a.builder, progress=progress, log=log)
        elif rekarina.find_game(game) is not None:
            result = rekarina.install(game, settings, rebuild=rebuild, progress=progress, log=log)
        else:
            print("error: not a recompiled game or a Rekarina install:", game)
            return 2
    except I.InstallError as exc:
        print("error:", exc.key, exc.arg)
        return 1
    print("done:", result.target, "rebuilt" if getattr(result, "rebuilt", False) else "settings only")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
