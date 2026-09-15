#!/usr/bin/env python3
"""Check that every relative link and image in a Markdown file resolves.

Relative links are the ones that rot, because nothing resolves them at commit time:
a document that moved leaves a link that looks fine and goes nowhere. Absolute URLs
are deliberately not fetched. Fetching them would make this job depend on the
availability of other people's websites, and a red build caused by somebody else's
outage teaches people to ignore the job.

Usage: python3 .github/check-links.py [root]

Exits 1 with a list of every broken target, rather than on the first one, so that a
rename can be fixed in one pass.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlparse

# `[text](target)` and `![alt](target)`. Good enough for the Markdown this repository
# writes, and deliberately not a parser: a link this misses is a link nobody wrote on
# purpose.
LINK = re.compile(r"!?\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")

FENCE = re.compile(r"^\s*(```|~~~)")


def strip_code(text: str) -> str:
    """Removes fenced code blocks, where a link-shaped string is sample output."""
    kept: list[str] = []
    inside = False
    for line in text.splitlines():
        if FENCE.match(line):
            inside = not inside
            continue
        if not inside:
            kept.append(line)
    return "\n".join(kept)


def targets(document: Path) -> list[str]:
    """The relative link targets in one document, with anchors removed."""
    found: list[str] = []
    for match in LINK.finditer(strip_code(document.read_text(encoding="utf-8"))):
        target = match.group(1)
        # A link to a heading within the same document (`#section`) has nothing on
        # disk to check, and a link with no scheme is relative.
        if target.startswith("#"):
            continue
        parsed = urlparse(target)
        if parsed.scheme or target.startswith("//"):
            continue
        path = unquote(parsed.path)
        if path:
            found.append(path)
    return found


def main() -> int:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    documents = sorted(
        path
        for path in root.rglob("*.md")
        # Vendored trees and build output are not this repository's documentation.
        if not any(part in {"target", ".git", "node_modules"} for part in path.parts)
    )

    broken: list[str] = []
    for document in documents:
        for target in targets(document):
            resolved = (document.parent / target).resolve()
            if not resolved.exists():
                broken.append(f"{document.relative_to(root)}: {target}")

    if broken:
        print("broken relative links:", file=sys.stderr)
        for entry in broken:
            print(f"  {entry}", file=sys.stderr)
        return 1

    print(f"checked {len(documents)} document(s); every relative link resolves")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
