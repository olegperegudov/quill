#!/usr/bin/env python3
"""Guard for the updater manifest: a broken latest.json strands users silently.

The in-app updater is only as good as this file — a missing platform key leaves
that platform on its installed version forever, with no error anyone can see.
Run at publish time (build.yml) and again before the stable channel is moved
onto an older release (release-control.yml).

    python3 verify_manifest.py latest.json v0.1.18
"""

import json
import sys

WANT = {"windows-x86_64", "darwin-aarch64", "darwin-x86_64"}


def main(path: str, tag: str) -> int:
    manifest = json.load(open(path))
    platforms = manifest.get("platforms", {})

    missing = WANT - set(platforms)
    if missing:
        print(f"::error::{path} missing platform keys: {sorted(missing)}")
        return 1

    if f"v{manifest.get('version')}" != tag:
        print(f"::error::{path} version {manifest.get('version')!r} != tag {tag!r}")
        return 1

    # macOS ships per-arch, never lipo'd: a universal bundle does not anchor
    # TCC grants, so permissions silently stop sticking after an update.
    for key in ("darwin-aarch64", "darwin-x86_64"):
        if "universal" in platforms[key].get("url", "").lower():
            print(f"::error::{key} points at a universal bundle; macOS must ship per-arch")
            return 1

    print(f"{path} OK: v{manifest['version']}, platforms {sorted(platforms)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1], sys.argv[2]))
