"""Update check and download for SHB-OcarinaTweaks.

The release assets are the only source: the app asks GitHub for the latest non-draft,
non-prerelease release, compares its tag with the running version, and can download the
asset for the current platform. When the app is a frozen Windows build and its folder is
writable, a small detached PowerShell script waits for the process to exit, replaces the
executable and starts the new one. In every other case the file is downloaded to a
per-user folder and shown to the user.
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.request
from dataclasses import dataclass
from pathlib import Path

REPO = "kuribox/OOT-SuperHeroBoostMod"
API_LATEST = f"https://api.github.com/repos/{REPO}/releases/latest"
RELEASES_PAGE = f"https://github.com/{REPO}/releases"
UA = "SHB-OcarinaTweaks"


class UpdaterError(Exception):
    pass


@dataclass
class Update:
    version: str
    tag: str
    page_url: str
    body: str = ""
    asset_url: str | None = None
    asset_name: str | None = None
    asset_size: int = 0
    digest: str | None = None


def _numbers(text: str) -> tuple[int, ...]:
    m = re.match(r"^[vV]?(\d+(?:\.\d+)*)", text.strip())
    if not m:
        return ()
    return tuple(int(x) for x in m.group(1).split("."))


def is_newer(tag: str, current: str) -> bool:
    a, b = _numbers(tag), _numbers(current)
    if not a:
        return False
    n = max(len(a), len(b))
    return a + (0,) * (n - len(a)) > b + (0,) * (n - len(b))


def _platform_assets() -> tuple[str, ...]:
    """This app's own asset names first: since 0.10.0 the plain names are the Rust app's, and updating the Python app
    into a Rust binary would replace it with something it cannot run from inside itself."""
    if sys.platform == "win32":
        return ("SHB-OcarinaTweaks-legacy.exe", "SHB-OcarinaTweaks.exe", "SuperHeroBoostConfig.exe")
    if sys.platform == "darwin":
        return ("SHB-OcarinaTweaks-legacy-macos.dmg", "SHB-OcarinaTweaks-legacy-macos.zip",
                "SHB-OcarinaTweaks-macos.dmg", "SHB-OcarinaTweaks.app.zip", "SHB-OcarinaTweaks-macos.zip",
                "SHB-OcarinaTweaks.dmg", "SHB-OcarinaTweaks",
                "SuperHeroBoostConfig-macos.dmg", "SuperHeroBoostConfig.app.zip",    # names up to 0.8.x
                "SuperHeroBoostConfig-macos.zip", "SuperHeroBoostConfig.dmg", "SuperHeroBoostConfig")
    return ("SHB-OcarinaTweaks-legacy-linux", "SHB-OcarinaTweaks-linux", "SHB-OcarinaTweaks",
            "SuperHeroBoostConfig-linux", "SuperHeroBoostConfig")


def _pick_asset(assets: list) -> dict | None:
    by_name = {a.get("name"): a for a in assets if isinstance(a, dict)}
    for name in _platform_assets():
        if name in by_name:
            return by_name[name]
    return None


def check(current: str, timeout: int = 10) -> Update | None:
    """Return the newest release if it is newer than `current`; None if up to date."""
    req = urllib.request.Request(
        API_LATEST,
        headers={"User-Agent": f"{UA}/{current}", "Accept": "application/vnd.github+json"},
    )
    with urllib.request.urlopen(req, timeout=timeout) as r:
        data = json.load(r)
    if not isinstance(data, dict):
        raise UpdaterError("bad response")
    if data.get("draft") or data.get("prerelease"):
        return None
    tag = str(data.get("tag_name") or "")
    if not is_newer(tag, current):
        return None
    asset = _pick_asset(data.get("assets") or [])
    return Update(
        version=tag.lstrip("vV"),
        tag=tag,
        page_url=str(data.get("html_url") or RELEASES_PAGE),
        body=str(data.get("body") or ""),
        asset_url=(asset or {}).get("browser_download_url"),
        asset_name=(asset or {}).get("name"),
        asset_size=int((asset or {}).get("size") or 0),
        digest=(asset or {}).get("digest"),
    )


def _writable(folder: Path) -> bool:
    try:
        probe = folder / f".shb-update-{os.getpid()}.tmp"
        probe.write_bytes(b"")
        probe.unlink()
        return True
    except OSError:
        return False


def _updates_dir() -> Path:
    if sys.platform == "win32":
        base = Path(os.environ.get("LOCALAPPDATA") or Path.home() / "AppData" / "Local")
    elif sys.platform == "darwin":
        base = Path.home() / "Library" / "Application Support"
    else:
        base = Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache")
    return base / "SHB-OcarinaTweaks" / "updates"


def update_path(version: str) -> tuple[Path, bool]:
    """Where to save the new executable, and whether it can replace the running one."""
    name = f"SHB-OcarinaTweaks-{version}"
    if getattr(sys, "frozen", False) and sys.platform == "win32":
        app_dir = Path(sys.executable).resolve().parent
        if _writable(app_dir):
            return app_dir / f"{name}.exe", True
    return _updates_dir() / f"{name}.exe", False


def download(update: Update, dest: Path, progress=None, timeout: int = 60) -> Path:
    if not update.asset_url:
        raise UpdaterError("no asset for this platform")
    dest = Path(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)
    part = dest.with_suffix(dest.suffix + ".part")
    req = urllib.request.Request(update.asset_url, headers={"User-Agent": UA})
    digest = hashlib.sha256()
    done = 0
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r, open(part, "wb") as f:
            total = int(r.headers.get("Content-Length") or update.asset_size or 0)
            while True:
                chunk = r.read(65536)
                if not chunk:
                    break
                f.write(chunk)
                digest.update(chunk)
                done += len(chunk)
                if progress:
                    progress(done, total)
        if update.digest:
            algo, _, want = update.digest.partition(":")
            if algo.lower() == "sha256" and digest.hexdigest().lower() != want.strip().lower():
                raise UpdaterError("checksum")
        part.replace(dest)
    except Exception:
        try:
            part.unlink()
        except OSError:
            pass
        raise
    return dest


def apply_and_restart(downloaded: Path) -> bool:
    """Replace the running Windows executable and start it again. False if unsupported."""
    if not (getattr(sys, "frozen", False) and sys.platform == "win32"):
        return False
    current = Path(sys.executable).resolve()
    downloaded = Path(downloaded).resolve()
    try:
        work = Path(tempfile.mkdtemp(prefix="shb-update-"))
    except OSError:
        return False
    script = work / "apply.ps1"

    def q(path: Path) -> str:
        return str(path).replace("'", "''")

    body = f"""$ErrorActionPreference = 'SilentlyContinue'
Wait-Process -Id {os.getpid()} -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 700
$ok = $false
for ($i = 0; $i -lt 40; $i++) {{
    Copy-Item -LiteralPath '{q(downloaded)}' -Destination '{q(current)}' -Force
    if ($?) {{ $ok = $true; break }}
    Start-Sleep -Milliseconds 500
}}
if ($ok) {{
    Remove-Item -LiteralPath '{q(downloaded)}' -Force
    Start-Process -FilePath '{q(current)}'
}} else {{
    Start-Process -FilePath '{q(downloaded)}'
}}
Remove-Item -LiteralPath $PSCommandPath -Force
"""
    try:
        script.write_text(body, encoding="utf-8-sig")
    except OSError:
        return False
    try:
        subprocess.Popen(
            ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass",
             "-WindowStyle", "Hidden", "-File", str(script)],
            creationflags=0x00000008 | 0x00000200,   # DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
            close_fds=True,
        )
    except OSError:
        return False
    return True


def reveal(path: Path) -> None:
    path = Path(path)
    try:
        if sys.platform == "win32":
            subprocess.Popen(["explorer", "/select,", str(path)])
        elif sys.platform == "darwin":
            subprocess.Popen(["open", "-R", str(path)])
        else:
            subprocess.Popen(["xdg-open", str(path.parent)])
    except OSError:
        pass


def open_releases() -> None:
    import webbrowser
    webbrowser.open(RELEASES_PAGE)
