#!/bin/sh
#
# Install the `estamora` binary from a published release.
#
#   curl -fsSL https://raw.githubusercontent.com/Estamora-Soroban-Layers/estamora-conformance-runner/v0.1.3/scripts/install-binary.sh | sh
#
#   ./install-binary.sh --version v0.1.3
#   ./install-binary.sh --dir "$HOME/.local/bin"
#   ./install-binary.sh --target aarch64-apple-darwin
#
# This is the install path for somebody who wants to measure a contract rather than to
# read this repository. `install.sh` builds from the checkout you are standing in and is
# the right one for a contributor; neither is a wrapper around the other, because the
# question "which revision am I running" has a different answer in each case:
#
#   * `install.sh` installs the working tree, so the revision is whatever you have.
#   * this script installs a published archive, so the revision is a tag, and the tag is
#     recorded in the archive's own VERSION file.
#
# The download is verified against the release's SHA256SUMS before anything is written.
# An installer that fetches a binary over the network and runs it without checking it is
# trusting the network, a DNS answer and a TLS certificate for the integrity of the tool
# whose job is to be trustworthy about somebody else's contract. So a missing checksum
# file, a missing entry, or a mismatch all stop the install rather than continuing with
# a warning.

# POSIX `sh`, deliberately, and not `bash`.
#
# The documented way to run this is `curl -fsSL <url> | sh`, and on Debian, Ubuntu and the
# GitHub runners `/bin/sh` is dash. `pipefail` is not in POSIX: `set -o pipefail` is an
# immediate `set: Illegal option -o pipefail`, the script exits before installing anything,
# and because the failure is on stderr inside a pipeline the reader sees a bare error with
# no install. That is exactly the shape of the defect this repository keeps finding in
# other people's installers, so it is not going to live in this one.
#
# Nothing here needs `pipefail`: every pipeline that could fail is either guarded with
# `|| die` or produces an empty result that the check after it rejects. `test-install-binary.sh`
# runs this script through `sh` as a case, so the guarantee is tested rather than promised.
set -eu

REPOSITORY="${ESTAMORA_REPOSITORY:-Estamora-Soroban-Layers/estamora-conformance-runner}"
VERSION="latest"
TARGET=""
DIR="${ESTAMORA_INSTALL_DIR:-}"

say() { printf '%s\n' "$*" >&2; }
die() { say "install: $*"; exit 1; }
# Printed from the script's own header when it is being read as a file. Piped to `sh`, `$0`
# is the interpreter's name rather than a path to anything, so there is nothing to read and
# a short usage is printed instead of an empty one.
usage() {
    if [ -f "$0" ]; then
        sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'
    else
        printf '%s\n' "usage: install-binary.sh [--version V] [--target T] [--dir D] [--repository O/R]"
        printf '%s\n' "       detects the platform, verifies the release checksum, installs to ~/.cargo/bin or ~/.local/bin"
    fi
}

while [ $# -gt 0 ]; do
    case "$1" in
        --version) VERSION="${2:-}"; shift 2 ;;
        --target) TARGET="${2:-}"; shift 2 ;;
        --dir) DIR="${2:-}"; shift 2 ;;
        --repository) REPOSITORY="${2:-}"; shift 2 ;;
        --help | -h) usage; exit 0 ;;
        *) die "unknown argument '$1' (try --help)" ;;
    esac
done

# The binary is native code, so the platform has to be identified rather than guessed.
# `uname -m` reports the architecture the shell is running as, which is why an arm64
# macOS running an x86_64 shell under Rosetta resolves to the Intel build — that build
# runs, and it is also the truthful answer to what the installer was asked for.
if [ -z "$TARGET" ]; then
    case "$(uname -s)" in
        Linux) os="unknown-linux-gnu" ;;
        Darwin) os="apple-darwin" ;;
        *) die "unsupported operating system '$(uname -s)'; use --target, or build from a checkout with scripts/install.sh" ;;
    esac
    case "$(uname -m)" in
        x86_64 | amd64) arch="x86_64" ;;
        arm64 | aarch64) arch="aarch64" ;;
        *) die "unsupported architecture '$(uname -m)'; use --target, or build from a checkout with scripts/install.sh" ;;
    esac
    TARGET="${arch}-${os}"
fi

if [ -z "$DIR" ]; then
    # `~/.cargo/bin` first when it exists, because that is already on the PATH of
    # anybody who has installed Rust, and a second location on the same machine would
    # mean two `estamora` binaries and an ambiguous answer to which one ran.
    if [ -d "$HOME/.cargo/bin" ]; then
        DIR="$HOME/.cargo/bin"
    else
        DIR="$HOME/.local/bin"
    fi
fi
mkdir -p "$DIR"

command -v curl >/dev/null 2>&1 || die "curl is not on PATH"
command -v shasum >/dev/null 2>&1 || command -v sha256sum >/dev/null 2>&1 \
    || die "neither shasum nor sha256sum is on PATH; the archive cannot be verified"

if [ "$VERSION" = "latest" ]; then
    base="https://github.com/${REPOSITORY}/releases/latest/download"
else
    case "$VERSION" in v*) ;; *) VERSION="v${VERSION}" ;; esac
    base="https://github.com/${REPOSITORY}/releases/download/${VERSION}"
fi

# An air-gapped or mirrored distribution points at its own copy of the release
# artefacts. The archives are still verified against the release's checksums, so a
# mirror cannot install something the release did not publish; the override decides
# where the bytes come from, not whether they are trusted. It is also how
# `scripts/test-install-binary.sh` exercises this script without a network.
[ -n "${ESTAMORA_RELEASE_BASE:-}" ] && base="${ESTAMORA_RELEASE_BASE}"

archive="estamora-${TARGET}.tar.gz"
work="$(mktemp -d)"
# The archive is downloaded into a directory that is removed whatever happens, so a
# failed verification does not leave a half-downloaded binary where a later run, or a
# person, might find it.
# `0` rather than `EXIT`: the latter is a bash spelling, and this script runs under whatever
# `sh` the reader has.
trap 'rm -rf "$work"' 0 1 2 3 15

say "install: fetching $archive"
curl -fsSL -o "$work/$archive" "$base/$archive" \
    || die "no archive at $base/$archive; check --version, and see https://github.com/${REPOSITORY}/releases"
curl -fsSL -o "$work/SHA256SUMS" "$base/SHA256SUMS" \
    || die "the release has no SHA256SUMS; refusing to install an unverifiable binary"

# The second field is the file name, optionally prefixed with `*` for binary mode and
# optionally a directory path: `shasum -b` writes `hash *name`, `sha256sum` writes
# `hash  name`, and a mirror that sums a directory writes `hash  ./sub/name`. All three
# describe the same archive, so all three are accepted and the name is compared as a
# basename. What is not accepted is an entry for a *different* file.
expected="$(awk -v name="$archive" '
    {
        file = $2
        sub(/^\*/, "", file)
        count = split(file, parts, "/")
        if (parts[count] == name) { print $1; exit }
    }
' "$work/SHA256SUMS")"
[ -n "$expected" ] || die "SHA256SUMS has no entry for $archive"

if command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$work/$archive" | awk '{ print $1 }')"
else
    actual="$(sha256sum "$work/$archive" | awk '{ print $1 }')"
fi

[ "$expected" = "$actual" ] \
    || die "$archive does not match its published checksum (expected $expected, got $actual)"

say "install: checksum verified"
tar -C "$work" -xzf "$work/$archive"

# `tar` on a hostile archive could write outside the extraction directory, so what is
# installed is the one binary extracted from a verified archive rather than whatever a
# recursive copy would have found.
binary="$(find "$work" -type f \( -name 'estamora' -o -name 'estamora.exe' \) | head -n 1)"
[ -n "$binary" ] || die "the archive holds no estamora binary"
install -m 0755 "$binary" "$DIR/estamora" 2>/dev/null \
    || { cp "$binary" "$DIR/estamora" && chmod 0755 "$DIR/estamora"; }

say "install: $("$DIR/estamora" --version 2>/dev/null || echo 'estamora') installed to $DIR/estamora"
case ":${PATH}:" in
    *":$DIR:"*) ;;
    *) say "install: add \"$DIR\" to PATH to run it as 'estamora'" ;;
esac
