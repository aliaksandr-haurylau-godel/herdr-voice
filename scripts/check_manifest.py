#!/usr/bin/env python3
"""Check herdr-plugin.toml against what the binary actually offers.

The manifest is the only contract between herdr and this plugin, and a typo in
it surfaces as a plugin that installs and then does nothing. This runs in CI so
the mismatch is caught before a release.
"""

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PLATFORMS = {"macos", "linux", "windows"}

def fail(message: str) -> None:
    print(f"manifest: {message}", file=sys.stderr)
    sys.exit(1)

def main() -> None:
    manifest = tomllib.loads((ROOT / "herdr-plugin.toml").read_text())

    for key in ("id", "name", "version", "description"):
        if not manifest.get(key):
            fail(f"missing required key: {key}")

    declared = set(manifest.get("platforms", []))
    if not declared <= PLATFORMS:
        fail(f"unknown platform in the manifest: {declared - PLATFORMS}")

    cargo = (ROOT / "Cargo.toml").read_text()
    crate_version = re.search(r'^version = "([^"]+)"', cargo, re.M).group(1)
    if manifest["version"] != crate_version:
        fail(f"version {manifest['version']} does not match the crate's {crate_version}")

    entries = []
    for section in ("build", "startup", "actions", "panes"):
        for entry in manifest.get(section, []):
            entries.append((section, entry))
            for platform in entry.get("platforms", []):
                if platform not in PLATFORMS:
                    fail(f"{section}: unknown platform {platform}")
            if not entry.get("command"):
                fail(f"{section}: an entry has no command")

    # Every subcommand the manifest calls must be one the binary knows. The list
    # is read from the source rather than from a second copy kept in step by hand.
    source = (ROOT / "src" / "main.rs").read_text()
    known = set(re.findall(r'Some\("([a-z-]+)"\)', source))
    for section, entry in entries:
        command = entry["command"]
        if not any("herdr-voice" in part for part in command):
            continue
        index = max(i for i, part in enumerate(command) if "herdr-voice" in part)
        subcommand = command[index + 1] if len(command) > index + 1 else None
        if subcommand and subcommand not in known:
            fail(
                f"{section}: the manifest calls '{subcommand}', "
                f"which src/main.rs does not accept (known: {sorted(known)})"
            )

    print(f"manifest: {len(entries)} entries, all commands known")

if __name__ == "__main__":
    main()
