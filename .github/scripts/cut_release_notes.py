#!/usr/bin/env python3
"""Turn the changelog's Unreleased section into a released one, and hand its text
to the release.

Run by the build workflow at the moment it bumps the version, so every GitHub
release carries what actually changed in it instead of "see the assets below" —
the menu-bar version item opens that page, and a person deciding whether to
install an update can read what they are installing.

    python3 .github/scripts/cut_release_notes.py 0.1.45 > notes.md

Rewrites CHANGELOG.md in place (Unreleased becomes `## v0.1.45 — 2026-08-10`,
with a fresh empty Unreleased above it) and prints the section's body. An empty
Unreleased leaves the file untouched and prints nothing, so a release with no
notes falls back to the generic text rather than publishing a bare heading.
"""

import datetime
import pathlib
import re
import sys

CHANGELOG = pathlib.Path(__file__).resolve().parents[2] / "CHANGELOG.md"
HEADING = "## Unreleased"


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: cut_release_notes.py <version>", file=sys.stderr)
        return 2
    version = sys.argv[1]

    text = CHANGELOG.read_text()
    start = text.find(HEADING)
    if start == -1:
        print(f"{CHANGELOG.name} has no '{HEADING}' section", file=sys.stderr)
        return 1

    body_start = start + len(HEADING)
    # The section runs to the next second-level heading, or to the end of file.
    next_heading = re.search(r"^## ", text[body_start:], flags=re.MULTILINE)
    body_end = body_start + next_heading.start() if next_heading else len(text)
    body = text[body_start:body_end].strip()

    if not body:
        return 0

    today = datetime.date.today().isoformat()
    released = f"## v{version} — {today}\n\n{body}\n\n"
    CHANGELOG.write_text(text[:start] + f"{HEADING}\n\n" + released + text[body_end:].lstrip("\n"))
    print(body)
    return 0


if __name__ == "__main__":
    sys.exit(main())
