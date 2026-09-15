#!/usr/bin/env python3
"""Fail when the crates.io publish sequence in the release workflow cannot be run.

    cargo metadata --no-deps --format-version 1 | python3 scripts/check-publish-plan.py

    cargo metadata --no-deps --format-version 1 | python3 scripts/check-publish-plan.py --print-sequence

`--workflow <path>` reads a plan from somewhere other than the real workflow, which is how
`scripts/test-check-publish-plan.sh` shows that each failure below is actually reported.

`cargo publish` is deliberately a human command (see `.github/workflows/release.yml`), so
the sequence it is run in exists only as prose in a comment. Prose is not checked by
anything, and the two ways it can be wrong are both silent until the day somebody runs it:

  * A crate that the sequence omits stays unpublished, and every crate that depends on it
    then fails to package with "no matching package named" -- which reads like a transient
    ordering problem rather than a missing line.

  * A crate the sequence lists is not packageable at all. Packaging rewrites each path
    dependency into a registry requirement of the same version, so a crate with a *normal*
    dependency on a workspace crate that is `publish = false` can never be packaged: the
    requirement it would carry names a version that no registry will ever hold. Listing it
    produces a step that waits for something that is never going to arrive.

The second case is in this repository: `estamora-cli` links `estamora-fixture-token`
because `--contract fixture:<name>` is how the runner demonstrates itself without a
deployed contract, and that fixture is `publish = false`. So the command cannot come from
crates.io, and the release plan has to say so rather than name it.

Reading the plan back out of the workflow rather than restating it here is the point: one
place is edited, and this checks it against what Cargo actually resolves. An exclusion is
checked too, so if the reason for one stops being true -- the fixture becomes publishable,
or the dependency becomes a dev-dependency -- the exclusion is reported as stale instead
of quietly keeping a crate undeliverable.

Markers in `.github/workflows/release.yml`, parsed from the comment block that documents
the sequence:

    #   cargo publish -p <crate>        the ordered sequence
    # EXCLUDED <crate>: <reason>         a publishable crate kept out of it, with a reason

Exit codes: 0 the plan matches the workspace, 1 it does not, 2 the checker was misused or
cargo produced nothing.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

WORKFLOW = Path(".github/workflows/release.yml")

# Anchored at the start of a comment so the instructions in the prose above the sequence
# do not read as entries in it.
PUBLISH_LINE = re.compile(r"^\s*#\s*cargo publish -p (?P<crate>[A-Za-z0-9_-]+)\s*$")
EXCLUDED_LINE = re.compile(r"^\s*#\s*EXCLUDED (?P<crate>[A-Za-z0-9_-]+):\s*(?P<reason>\S.*)$")


def load(path: Path) -> tuple[list[str], dict[str, str]]:
    """Return the sequence in order, and the exclusions with their reasons."""
    sequence: list[str] = []
    excluded: dict[str, str] = {}
    for number, line in enumerate(path.read_text().splitlines(), start=1):
        if match := PUBLISH_LINE.match(line):
            sequence.append(match.group("crate"))
        elif match := EXCLUDED_LINE.match(line):
            crate = match.group("crate")
            if crate in excluded:
                raise ValueError(f"{path}:{number}: {crate} is excluded twice")
            excluded[crate] = match.group("reason")
    return sequence, excluded


def normal_workspace_dependencies(metadata: dict) -> dict[str, set[str]]:
    """The edges packaging has to resolve: normal dependencies on workspace crates.

    Dev-dependencies are excluded because Cargo drops them when it packages a crate, and
    build dependencies because they are not part of what a consumer links. A dependency
    with `kind == "dev"` on a crate that is not published is therefore not a reason a
    crate cannot be published, and treating it as one would produce a false failure.
    """
    in_workspace = {package["name"] for package in metadata.get("packages", [])}
    edges: dict[str, set[str]] = {}
    for package in metadata.get("packages", []):
        edges[package["name"]] = {
            dependency["name"]
            for dependency in package.get("dependencies", [])
            if dependency["name"] in in_workspace and dependency.get("kind") is None
        }
    return edges


def closure(crate: str, edges: dict[str, set[str]]) -> set[str]:
    """Every workspace crate reachable from `crate` along normal dependencies."""
    seen: set[str] = set()
    stack = list(edges.get(crate, ()))
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        seen.add(current)
        stack.extend(edges.get(current, ()))
    return seen


def main() -> int:
    print_sequence = False
    workflow = WORKFLOW
    arguments = sys.argv[1:]
    while arguments:
        argument = arguments.pop(0)
        if argument == "--print-sequence":
            print_sequence = True
        elif argument == "--workflow" and arguments:
            workflow = Path(arguments.pop(0))
        else:
            print(f"check-publish-plan: unknown argument {argument!r}", file=sys.stderr)
            return 2

    try:
        metadata = json.load(sys.stdin)
    except json.JSONDecodeError as error:
        print(f"check-publish-plan: could not read cargo metadata ({error})", file=sys.stderr)
        return 2

    packages = {package["name"]: package for package in metadata.get("packages", [])}
    if not packages:
        print("check-publish-plan: cargo metadata listed no packages", file=sys.stderr)
        return 2

    edges = normal_workspace_dependencies(metadata)
    # `publish = false` is reported as an empty list; a crate that names registries is
    # reported with them. Neither is republished from here, so the test is the same one
    # `check-crate-metadata.py` makes.
    publishable = {name for name, package in packages.items() if package.get("publish") != []}

    if print_sequence:
        # The release workflow asks for the sequence rather than repeating it, so the loop
        # that packages the crates and the list a human reads cannot disagree.
        try:
            sequence, _ = load(workflow)
        except (OSError, ValueError) as error:
            print(f"check-publish-plan: {error}", file=sys.stderr)
            return 2
        print("\n".join(sequence))
        return 0

    try:
        sequence, excluded = load(workflow)
    except (OSError, ValueError) as error:
        print(f"check-publish-plan: {error}", file=sys.stderr)
        return 2
    if not sequence:
        print(f"check-publish-plan: {workflow} documents no publish sequence", file=sys.stderr)
        return 2

    problems: list[str] = []

    repeated = [name for name in sequence if sequence.count(name) > 1]
    if repeated:
        problems.append(f"{sorted(set(repeated))} appear(s) more than once in the sequence")

    unknown = [name for name in sequence if name not in packages]
    if unknown:
        problems.append(f"{unknown} are named in the sequence but are not workspace crates")

    # A crate is deliverable from a registry only if every crate it depends on normally is
    # itself deliverable, so the deliverable set is the one whose whole closure is
    # publishable. Everything else has to be excluded, and the sequence has to be exactly
    # the deliverable set: one crate more and a maintainer runs a command that cannot
    # finish, one crate fewer and a dependency never resolves.
    deliverable = {
        name for name in publishable if closure(name, edges) <= publishable
    }
    listed = set(sequence)

    for name in sorted(deliverable - listed):
        problems.append(
            f"{name} is deliverable but is not in the sequence, so nothing publishes it "
            f"and the crates that depend on it can never resolve"
        )
    for name in sorted(listed - deliverable):
        unpublishable = sorted(
            dependency
            for dependency in closure(name, edges)
            if packages[dependency].get("publish") == []
        )
        problems.append(
            f"{name} is in the sequence but cannot be packaged: it depends on "
            f"{unpublishable}, which is `publish = false`, so the registry requirement the "
            f"packaged manifest would carry can never resolve. Exclude it, and say why"
        )

    for name in sorted(listed - publishable):
        problems.append(
            f"{name} is in the sequence but is `publish = false`; publishing it is not a "
            f"thing this workspace does"
        )

    for name in sorted(publishable - listed - set(excluded)):
        problems.append(
            f"{name} is publishable and is neither in the sequence nor excluded; a reader "
            f"cannot tell whether that was a decision"
        )
    for name in sorted(set(excluded) - publishable):
        problems.append(
            f"{name} is excluded but is not publishable in the first place, so the "
            f"exclusion says nothing"
        )

    # The exclusion has to be justified by the structure, not asserted. If the fixture is
    # ever made publishable, or the dependency becomes a dev-dependency, the crate becomes
    # deliverable and this reports the exclusion as stale rather than leaving the command
    # permanently unobtainable.
    for name, reason in sorted(excluded.items()):
        if name not in packages:
            problems.append(f"{name} is excluded but is not a workspace crate")
            continue
        blockers = sorted(
            dependency
            for dependency in edges.get(name, ())
            if packages[dependency].get("publish") == []
        )
        if not blockers:
            problems.append(
                f"{name} is excluded because {reason!r}, but it has no `publish = false` "
                f"dependency, so nothing stops it being published"
            )
        else:
            print(f"  excluded {name:26s} blocked by {', '.join(blockers)}")

    # Ordering. Cargo will not package a crate until every crate it depends on normally is
    # on the registry with a matching version, so each dependency has to be published
    # first. `--print-sequence` hands this order to the packaging loop.
    position = {name: index for index, name in enumerate(sequence)}
    for name in sequence:
        for dependency in sorted(edges.get(name, ())):
            if dependency in position and position[dependency] > position[name]:
                problems.append(
                    f"{name} is published before {dependency}, which it depends on; cargo "
                    f"cannot package a crate whose dependency is not on the registry yet"
                )

    if problems:
        print("check-publish-plan: the release plan does not match the workspace", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        print(
            f"check-publish-plan: edit the publish block in {workflow}, which is the one "
            f"place the plan is written",
            file=sys.stderr,
        )
        return 1

    print(f"check-publish-plan: {len(sequence)} crate(s) in publish order")
    for index, name in enumerate(sequence, start=1):
        print(f"  {index}. {name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
