#!/usr/bin/env python3
"""Archive obsolete per-user ZORU Miner launchers without modifying wallet data."""
import argparse
import configparser
import os
from pathlib import Path
import re
import shlex
import uuid


def obsolete_desktop(path: Path) -> bool:
    if not path.is_file() or path.is_symlink():
        return False
    try:
        parser = configparser.ConfigParser(interpolation=None)
        parser.optionxform = str
        with path.open(encoding="utf-8") as handle:
            parser.read_file(handle)
        entry = parser["Desktop Entry"]
        name = entry.get("Name", "").strip()
        command = shlex.split(entry.get("Exec", ""))
        binary = Path(command[0]).name if command else ""
        return bool(
            re.fullmatch(r"ZORU Miner(?: v[0-9][^/]*)?", name)
            and (binary == "zoru-miner" or re.fullmatch(r"ZORU Miner v[0-9].*", name))
        )
    except (OSError, UnicodeError, ValueError, KeyError, configparser.Error):
        return False


def cleanup(applications: Path, archive: Path, apply: bool) -> list[Path]:
    if not applications.exists():
        return []
    matches = sorted(p for p in applications.glob("*.desktop") if obsolete_desktop(p))
    if apply and matches:
        archive.mkdir(parents=True, exist_ok=True, mode=0o700)
        os.chmod(archive, 0o700)
        for path in matches:
            target = archive / (path.name + "." + uuid.uuid4().hex + ".disabled")
            path.rename(target)
            print(f"Archived obsolete shortcut: {path.name}")
    elif matches:
        for path in matches:
            print(f"Obsolete shortcut: {path.name}")
        print("Run zoru-clean-launchers --apply to archive these launchers.")
    return matches


def main() -> None:
    parser = argparse.ArgumentParser(description="Archive obsolete local ZORU Miner shortcuts")
    parser.add_argument("--apply", action="store_true", help="Archive matching shortcuts")
    args = parser.parse_args()
    local = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share")
    cleanup(local / "applications", local / "zoru-miner/retired-launchers", args.apply)


if __name__ == "__main__":
    main()
