#!/usr/bin/env bash
#
# Show that `scripts/check-publish-plan.py` reports each way the release plan can be
# wrong, rather than only agreeing with the plan that happens to be committed.
#
# A checker that has never been seen to fail is a checker nobody can rely on, and this one
# guards a hand-written sequence in a comment: the plan it reads is prose, the correct
# version of it is not obvious, and the failure it was written for — a crate listed that
# cannot be published — would otherwise appear only on the day somebody ran the publish.
#
# Every case here is the real workflow with one line changed, so what is asserted is what
# the checker does to the file it actually reads.
#
#   ./scripts/test-check-publish-plan.sh
#
# It needs `cargo metadata` and therefore a Cargo workspace to read; it copies nothing but
# the workflow, and it writes only inside a temporary directory.

set -euo pipefail

cd "$(dirname "$0")/.."

WORKFLOW=.github/workflows/release.yml
CHECKER=scripts/check-publish-plan.py
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

checks=0
failures=0

# Run the checker against a mutated copy of the workflow and assert on its exit code and
# on one phrase of its complaint. The phrase matters as much as the exit code: a checker
# that fails for the wrong reason would pass this test and mislead the next person.
expect() {
  local name=$1 expected_code=$2 phrase=$3 file=$4
  local output code
  set +e
  output=$(cargo metadata --no-deps --format-version 1 2>/dev/null \
    | python3 "$CHECKER" --workflow "$file" 2>&1)
  code=$?
  set -e
  checks=$((checks + 1))

  if [ "$code" -ne "$expected_code" ]; then
    echo "FAIL  $name"
    echo "      expected exit $expected_code, got $code"
    echo "$output" | sed 's/^/      /'
    failures=$((failures + 1))
    return
  fi
  if [ -n "$phrase" ] && ! grep -qF -- "$phrase" <<<"$output"; then
    echo "FAIL  $name"
    echo "      exit code was right but the message did not say: $phrase"
    echo "$output" | sed 's/^/      /'
    failures=$((failures + 1))
    return
  fi
  echo "ok    $name"
}

# The plan as committed is the one shape that is accepted.
expect "the committed plan is accepted" 0 "7 crate(s) in publish order" "$WORKFLOW"

# A dependency published after the crate that needs it. Cargo cannot package a crate whose
# dependency is not on the registry, so this order cannot be run, and the ordering is the
# part of the plan a human is most likely to get wrong by hand.
python3 - "$WORKFLOW" "$work/out-of-order.yml" <<'PY'
import sys

source, target = sys.argv[1], sys.argv[2]
lines = open(source).read().splitlines(keepends=True)
core = "  #   cargo publish -p estamora-core\n"
profile = "  #   cargo publish -p estamora-profile\n"
first, second = lines.index(core), lines.index(profile)
lines[first], lines[second] = lines[second], lines[first]
open(target, "w").write("".join(lines))
PY
expect "a dependency published after its dependant is refused" 1 \
  "estamora-profile is published before estamora-core" "$work/out-of-order.yml"

# A crate that is publishable and is neither published nor excluded. This is the silent
# one: it leaves a crate undelivered and the next crate in the chain failing to package
# with "no matching package named", which reads like a transient ordering problem.
grep -v '^  #   cargo publish -p estamora-vectors$' "$WORKFLOW" > "$work/dropped.yml"
expect "a dropped crate is reported rather than silently skipped" 1 \
  "estamora-vectors is deliverable but is not in the sequence" "$work/dropped.yml"

# The defect this checker was written for, restored: `estamora-cli` links the
# `publish = false` fixture, so the registry requirement it would carry can never resolve.
cp "$WORKFLOW" "$work/listed.yml"
sed -i 's|^  # EXCLUDED estamora-cli:.*|  #   cargo publish -p estamora-cli|' "$work/listed.yml"
expect "a crate that cannot be packaged is refused" 1 \
  "estamora-cli is in the sequence but cannot be packaged" "$work/listed.yml"

# An exclusion that outlives its reason. Nothing in the workspace stops `estamora-cli`
# being published except that one dependency, so if that changes the exclusion has to be
# deleted — otherwise the command stays unobtainable for a reason that is no longer true.
cp "$WORKFLOW" "$work/stale.yml"
sed -i 's|^  # EXCLUDED estamora-cli:.*|  # EXCLUDED estamora-core: kept back for now|' "$work/stale.yml"
expect "an exclusion with no structural reason is refused" 1 \
  "estamora-core is excluded because 'kept back for now', but it has no" "$work/stale.yml"

# An exclusion for a crate that is not publishable in the first place says nothing, and
# hides nothing either — it is noise that makes the block look considered.
cp "$WORKFLOW" "$work/pointless.yml"
printf '  # EXCLUDED estamora-benches: it is a benchmark harness\n' >> "$work/pointless.yml"
expect "an exclusion of an unpublished crate is refused" 1 \
  "estamora-benches is excluded but is not publishable" "$work/pointless.yml"

# The sequence the packaging loop reads has to be the sequence a human reads.
sequence=$(cargo metadata --no-deps --format-version 1 2>/dev/null \
  | python3 "$CHECKER" --print-sequence)
checks=$((checks + 1))
if [ "$(grep -c '^  #   cargo publish -p ' "$WORKFLOW")" -ne "$(wc -l <<<"$sequence")" ]; then
  echo "FAIL  --print-sequence and the documented list are the same length"
  failures=$((failures + 1))
else
  echo "ok    --print-sequence and the documented list are the same length"
fi

# And a missing workflow is a misuse, not a pass. A checker that returns zero because it
# found nothing to read is worse than no checker.
expect "a workflow that is not there is a misuse, not a pass" 2 \
  "No such file or directory" "$work/absent.yml"

echo
echo "$((checks - failures))/$checks checks passed"
[ "$failures" -eq 0 ]
