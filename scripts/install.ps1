<#
.SYNOPSIS
    Install the `estamora` binary from this checkout.

.DESCRIPTION
    Source-first on purpose: there is no prebuilt binary to fetch and no network step
    that could install something other than the revision you are standing in. What
    gets installed is what you can read, and `estamora --version` identifies it.

    The workspace's Rust toolchain is pinned in `rust-toolchain.toml`, so `rustup`
    selects it here rather than whatever the machine happens to have. A conformance
    result is only meaningful if the tooling that produced it is identified, which is
    why the toolchain is part of the repository rather than a prerequisite in a README.

.PARAMETER Root
    Where to install to. Defaults to cargo's own bin directory.

.EXAMPLE
    ./scripts/install.ps1
#>
[CmdletBinding()]
param(
    [string]$Root
)

$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Crate = 'estamora-cli'
$Bin = 'estamora'

function Say([string]$Message) { Write-Host $Message }
function Die([string]$Message) { Write-Error "install: $Message"; exit 1 }

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Die 'cargo is not on PATH; install Rust from https://rustup.rs first'
}

Push-Location $RepoRoot
try {
    Say "install: building $Crate from $RepoRoot"

    # `--locked` is not a convenience. The lock file is committed because an unpinned
    # dependency tree would let a conformance verdict change without a commit, which is
    # the drift this project exists to make visible. An install that silently
    # re-resolved it would install a different tool from the one the repository
    # describes.
    $arguments = @('install', '--locked', '--path', "crates/$Crate")
    if ($Root) { $arguments += @('--root', $Root) }

    & cargo @arguments
    if ($LASTEXITCODE -ne 0) {
        Die "the build failed; run 'cargo build -p $Crate' to see the errors in full"
    }
}
finally {
    Pop-Location
}

if (-not (Get-Command $Bin -ErrorAction SilentlyContinue)) {
    Say "install: $Bin was built but is not on PATH."
    Say "install: add `$HOME\.cargo\bin to PATH, or pass -Root to choose where it goes."
    exit 0
}

Say "install: $(& $Bin --version) is ready"
Say "install: point it at a specification checkout with --spec or `$env:ESTAMORA_SPEC_REPO"
