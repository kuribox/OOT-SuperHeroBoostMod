"""Per-user paths used by the app. Works on Windows, macOS and Linux.

Settings are portable: the file next to the program is tried first, then the
per-user configuration folder (which is writable even when the app lives in a
read-only place such as /Applications or an AppImage mount).
"""
import os
import sys
from pathlib import Path

APP_NAME = "SHB-OcarinaTweaks"
LEGACY_APP_NAME = "SuperHeroBoostConfig"   # up to 0.8.x: its settings are still read


def program_dir() -> Path:
    """Folder of the executable (frozen) or of the Python sources."""
    if getattr(sys, "frozen", False):
        return Path(sys.executable).resolve().parent
    return Path(__file__).resolve().parent


def user_config_dir() -> Path:
    if sys.platform == "win32":
        base = Path(os.environ.get("APPDATA") or Path.home() / "AppData" / "Roaming")
    elif sys.platform == "darwin":
        base = Path.home() / "Library" / "Application Support"
    else:
        base = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config")
    return base / APP_NAME


def settings_paths() -> tuple[Path, Path]:
    """Portable first, per-user second."""
    return program_dir() / f"{APP_NAME}.json", user_config_dir() / "settings.json"


def legacy_settings_paths() -> tuple[Path, Path]:
    """Where the app saved its settings before it was renamed (read only, so an update keeps them)."""
    return (program_dir() / f"{LEGACY_APP_NAME}.json",
            user_config_dir().with_name(LEGACY_APP_NAME) / "settings.json")

