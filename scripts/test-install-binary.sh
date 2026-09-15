#!/usr/bin/env bash
#
# Test `scripts/install-binary.sh` against a release that is built here rather than
# fetched, because the part worth testing is the decision the installer makes, not the
# network it makes it over.
#
#   ./scripts/test-install-binary.sh
#
# Four cases, and the three that must fail are the reason this exists:
#
#   1. A verified archive installs, and the installed binary runs.
#   2. An archive that does not match its published checksum is refused.
#   3. A release with no SHA256SUMS at all is refused.
#   4. A SHA256SUMS that omits this archive is refused.
#
# An installer that downloads a binary and runs it is the one place in this project where
# the network is trusted for the integrity of the tool, so "it refused the tampered
# archive" is a property that has to be observed rather than intended. The `file://`
# scheme is used for the fixtures, so the whole suite runs offline.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALLER="$ROOT/scripts/install-binary.sh"

say() { printf '%s\n' "$*" >&2; }
die() { say "test-install-binary: $*"; exit 1; }

[ -x "$INSTALLER" ] || die "$INSTALLER is not executable"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT INT TERM

# --- a release built here -------------------------------------------------------------

# The archive layout is the one the release workflow produces: a single top-level
# directory holding the binary, its licence and a VERSION file.
make_release() {
    local dir="$1" target="$2" payload="${3:-ok}"
    local name="estamora-${target}"
    local stage="$work/stage-$payload-$target"

    rm -rf "$stage" "${dir:?}"
    mkdir -p "$stage/$name" "$dir"
    # A stand-in for the real binary: it has to be executable, answer `--version`, and
    # print something that identifies which release it came from.
    printf '#!/bin/sh\nprintf "estamora 0.1.0 (%s)\\n"\n' "$payload" > "$stage/$name/estamora"
    chmod 0755 "$stage/$name/estamora"
    printf 'estamora 0.1.0 (%s)\n' "$payload" > "$stage/$name/VERSION"

    tar -C "$stage" -czf "$dir/$name.tar.gz" "$name"

    # Written from inside the release directory, so the entries name the archives the way
    # the release workflow writes them: `hash *name.tar.gz`, with no directory prefix.
    case "$payload" in
        # The checksums are written from the archive that was just built, so case 1 is
        # the honest release and case 4 removes the entry for it.
        missing-entry) (cd "$dir" && shasum -a 256 -b "$name.tar.gz") | sed 's/ .*/  other.tar.gz/' > "$dir/SHA256SUMS" ;;
        # Case 3 omits the file entirely: the release claims to be installable and
        # publishes nothing to verify against.
        no-sums) : ;;
        # Case 2 publishes a checksum for a different set of bytes.
        tampered) printf '%064d  %s\n' 0 "$name.tar.gz" > "$dir/SHA256SUMS" ;;
        # Case 5 is the same archive, summed the way a mirror that walks a directory
        # would: a path in the second field rather than a bare name.
        prefixed) (cd "$dir" && shasum -a 256 "./$name.tar.gz") > "$dir/SHA256SUMS" ;;
        *) (cd "$dir" && shasum -a 256 -b "$name.tar.gz") > "$dir/SHA256SUMS" ;;
    esac
}

# The installer detects the platform from `uname`, so the fixtures are named for the
# machine running this test rather than for a target the test cannot execute.
case "$(uname -s)" in
    Linux) os="unknown-linux-gnu" ;;
    Darwin) os="apple-darwin" ;;
    *) die "this test builds a binary for the host; unsupported host $(uname -s)" ;;
esac
case "$(uname -m)" in
    x86_64 | amd64) arch="x86_64" ;;
    arm64 | aarch64) arch="aarch64" ;;
    *) die "unsupported host architecture $(uname -m)" ;;
esac
host_target="${arch}-${os}"

failures=0
check() {
    local what="$1" expected="$2" actual="$3"
    if [ "$expected" = "$actual" ]; then
        say "  ok   $what"
    else
        say "  FAIL $what (expected $expected, got $actual)"
        failures=$((failures + 1))
    fi
}

# --- 1. a verified archive installs ----------------------------------------------------

say "1. a verified archive installs"
make_release "$work/good" "$host_target" ok
mkdir -p "$work/good-bin"

status=0
out="$(ESTAMORA_RELEASE_BASE="file://$work/good" "$INSTALLER" --dir "$work/good-bin" 2>&1)" || status=$?

check "exits zero" "0" "$status"
check "installed a binary" "yes" "$([ -x "$work/good-bin/estamora" ] && echo yes || echo no)"
check "the binary runs" "estamora 0.1.0 (ok)" "$("$work/good-bin/estamora" --version)"
check "verified the checksum first" "yes" \
    "$(printf '%s' "$out" | grep -q 'checksum verified' && echo yes || echo no)"

# --- 2. a tampered archive is refused --------------------------------------------------

say "2. an archive that does not match its checksum is refused"
make_release "$work/tampered" "$host_target" tampered
mkdir -p "$work/tampered-bin"

status=0
out="$(ESTAMORA_RELEASE_BASE="file://$work/tampered" "$INSTALLER" --dir "$work/tampered-bin" 2>&1)" || status=$?

check "exits non-zero" "1" "$status"
check "wrote nothing" "no" "$([ -e "$work/tampered-bin/estamora" ] && echo yes || echo no)"
check "says why" "yes" \
    "$(printf '%s' "$out" | grep -q 'does not match its published checksum' && echo yes || echo no)"

# --- 3. a release with no checksums is refused -----------------------------------------

say "3. a release with no SHA256SUMS is refused"
make_release "$work/nosums" "$host_target" no-sums
mkdir -p "$work/nosums-bin"

status=0
out="$(ESTAMORA_RELEASE_BASE="file://$work/nosums" "$INSTALLER" --dir "$work/nosums-bin" 2>&1)" || status=$?

check "exits non-zero" "1" "$status"
check "wrote nothing" "no" "$([ -e "$work/nosums-bin/estamora" ] && echo yes || echo no)"
check "says why" "yes" \
    "$(printf '%s' "$out" | grep -q 'no SHA256SUMS' && echo yes || echo no)"

# --- 4. checksums that omit this archive are refused -----------------------------------

say "4. checksums that omit this archive are refused"
make_release "$work/noentry" "$host_target" missing-entry
mkdir -p "$work/noentry-bin"

status=0
out="$(ESTAMORA_RELEASE_BASE="file://$work/noentry" "$INSTALLER" --dir "$work/noentry-bin" 2>&1)" || status=$?

check "exits non-zero" "1" "$status"
check "wrote nothing" "no" "$([ -e "$work/noentry-bin/estamora" ] && echo yes || echo no)"
check "says why" "yes" \
    "$(printf '%s' "$out" | grep -q 'no entry for' && echo yes || echo no)"

# --- 5. a checksum written with a path prefix still verifies ---------------------------

say "5. checksums that name the archive by a path still verify"
make_release "$work/prefixed" "$host_target" prefixed
mkdir -p "$work/prefixed-bin"

status=0
out="$(ESTAMORA_RELEASE_BASE="file://$work/prefixed" "$INSTALLER" --dir "$work/prefixed-bin" 2>&1)" || status=$?

check "exits zero" "0" "$status"
check "installed a binary" "yes" "$([ -x "$work/prefixed-bin/estamora" ] && echo yes || echo no)"

# --- 6. the documented quickstart works through `sh` ---------------------------------

# The README says `curl -fsSL <url> | sh`, and on Debian, Ubuntu and the GitHub runners
# `/bin/sh` is dash. The installer used `set -o pipefail`, which is not POSIX: dash stopped
# on its first line, installed nothing and said so only on stderr inside a pipeline. Nobody
# running the documented command would have seen the error, and nothing here would have
# caught it, because every other case invokes the script with its own `bash` shebang.
#
# So the quickstart is a case. If `sh` is a shell that rejects the script, this fails.
say "6. the documented quickstart works through sh"
make_release "$work/posix" "$host_target" ok
mkdir -p "$work/posix-bin"

status=0
out="$(ESTAMORA_RELEASE_BASE="file://$work/posix" sh "$INSTALLER" --dir "$work/posix-bin" 2>&1)" || status=$?

check "exits zero under sh" "0" "$status"
check "installed a binary" "yes" "$([ -x "$work/posix-bin/estamora" ] && echo yes || echo no)"
check "no shell rejected the script" "no" \
    "$(printf '%s' "$out" | grep -qi 'illegal option\|not found' && echo yes || echo no)"

# The same script must also parse as bash, because `./install-binary.sh` and
# `bash install-binary.sh` are both things a reader will type.
if command -v bash >/dev/null 2>&1; then
    bash -n "$INSTALLER" || { say "  FAIL the installer is not valid bash"; failures=$((failures + 1)); }
    say "  ok   the installer is also valid bash"
fi

say ""
if [ "$failures" -ne 0 ]; then
    die "$failures check(s) failed"
fi
say "test-install-binary: all checks passed against $host_target"
