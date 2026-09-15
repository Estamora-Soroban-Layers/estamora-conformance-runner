#!/usr/bin/env python3
"""Fail when a publishable crate is missing metadata a consumer needs.

    cargo metadata --no-deps --format-version 1 | python3 scripts/check-crate-metadata.py estamora-core ...

A crate's page on a registry is generated from its manifest, and the fields that make that
page useful are easy to lose: they are declared once in `[workspace.package]` and inherited
by each crate, so a crate that forgets `description.workspace = true` has no description
while the workspace still does. Checking the workspace manifest would pass. Checking the
resolved metadata does not.

That distinction is the reason this reads `cargo metadata` output rather than grepping the
manifests, which is what the release script used to do.

Exit codes: 0 every crate carries the metadata, 1 something is missing, 2 the checker was
misused or cargo produced nothing.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

# `readme` is checked as a path rather than a string: cargo will resolve it relative to the
# workspace root, and a path that does not exist is worse than no path at all, because the
# registry accepts the package and the published page renders without the document.
REQUIRED_STRINGS = ("description", "license", "repository", "homepage")
REQUIRED_LISTS = ("keywords", "categories")


def main() -> int:
    try:
        metadata = json.load(sys.stdin)
    except json.JSONDecodeError as error:
        print(f"check-crate-metadata: could not read cargo metadata ({error})", file=sys.stderr)
        return 2

    packages = {package["name"]: package for package in metadata.get("packages", [])}
    if not packages:
        print("check-crate-metadata: cargo metadata listed no packages", file=sys.stderr)
        return 2

    # Named packages are checked as named. With no arguments every *publishable* package is
    # checked, which is derived from the metadata rather than from a list repeated in a
    # workflow: Cargo reports `publish = false` as an empty list, and a crate added later is
    # then covered without anybody remembering to add it here. A list maintained by hand is
    # the failure mode this check exists to catch, one level up.
    if len(sys.argv) > 1:
        wanted = set(sys.argv[1:])
    else:
        wanted = {
            name
            for name, package in packages.items()
            if package.get("publish") != []
        }
        if not wanted:
            print("check-crate-metadata: no publishable package was listed", file=sys.stderr)
            return 2
        print(f"check-crate-metadata: no packages named; checking all {len(wanted)} publishable")
    problems: list[str] = []
    for name in sorted(wanted):
        package = packages.get(name)
        if package is None:
            problems.append(f"{name}: no such package in the workspace")
            continue

        for field in REQUIRED_STRINGS:
            value = package.get(field)
            if not isinstance(value, str) or not value.strip():
                problems.append(f'{name}: declares no `{field}`, so its registry page has none')

        for field in REQUIRED_LISTS:
            value = package.get(field)
            if not isinstance(value, list) or not value:
                problems.append(f'{name}: declares no `{field}`, so it is unclassifiable')

        readme = package.get("readme")
        if not isinstance(readme, str) or not readme.strip():
            problems.append(
                f"{name}: declares no `readme`, so its registry page renders a bare description"
            )
        else:
            # Resolved against the package, not against the working directory. Cargo
            # rewrites an inherited value into a path relative to the crate that inherited
            # it, so a workspace-level `readme = "README.md"` reads as `../../README.md`
            # here -- correct, and only meaningful next to the manifest it belongs to.
            manifest = package.get("manifest_path")
            base = Path(manifest).parent if isinstance(manifest, str) else Path.cwd()
            if not (base / readme).is_file():
                problems.append(
                    f"{name}: `readme` names {readme!r}, which is not a file beside {base}"
                )

        print(f"  ok   {name}")

    if problems:
        print("check-crate-metadata: incomplete manifest metadata", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        print(
            "check-crate-metadata: every field is declared once in [workspace.package] and "
            "inherited with `<field>.workspace = true`",
            file=sys.stderr,
        )
        return 1

    print(f"check-crate-metadata: {len(wanted)} package(s) carry the metadata a consumer needs")
    return 0


if __name__ == "__main__":
    sys.exit(main())
