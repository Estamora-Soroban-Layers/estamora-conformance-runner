#!/usr/bin/env python3
"""Measure what the example token costs to call, and render the table that records it.

    ./scripts/measure-contract-costs.py measure            # against testnet, writes JSON
    ./scripts/measure-contract-costs.py render costs.json  # renders Markdown from a capture

# Why this measures the live contract rather than the runner's own execution

`benches/` measures what it costs to *run a conformance measurement*: hosts, deployments,
profile evaluation. This measures something else, and it is the thing a contract author asking
"what does my token cost?" means: the CPU instructions, ledger bytes and resource fee that
Soroban charges for calling each entrypoint of the deployed contract.

Those numbers cannot be produced by the local host the runner executes vectors in. They come
from `simulateTransaction` against a real network, which is the only thing that knows the
current ledger, the current contract code, and what the host actually charges.

# How a call is measured without holding an account

The transaction is built with `stellar contract invoke --build-only`, which needs a source
account and no signature, and then handed to `simulateTransaction` directly. Simulation does not
verify signatures -- it records the authorization requirements instead -- so a call can be
measured with nothing but a public key.

That matters for which calls appear below. This fixture has no constructor, no mint and no admin,
and its balances start empty *by design*: a vector cannot seed a deployed artifact, so a token
with reachable state would only be reachable by a caller the runner does not have. The operations
that can therefore be measured are the read paths and the two writes that are legitimate against
an empty token: revoking an allowance, and burning nothing.

# Why the refusals are listed with no numbers against them

A refused simulation answers with an error and **no** `transactionData`, and the resource
consumed is inside the transaction data. So a refusal has nothing to report, and the honest thing
is to record that rather than to invent a figure or to quietly leave the operations out. They
stay in the table because the contract's refusals are part of its surface: a reader comparing
this table with the profiles should be able to see which calls were refused and which were
simply not attempted.

# Why the table is rendered rather than written

Every number in the README's table comes from the JSON this script writes, and
`--render` is what produces the table. A hand-copied number is a number that stops matching the
capture it came from, and this project's whole claim is that it is careful about that.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import urllib.request

# The deployment the example documents. A measurement of a *different* deployment of the same
# code would produce the same numbers -- the code hash is recorded for exactly that reason --
# so this is the instance already named in the README rather than a second one to keep in step.
CONTRACT = "CDMCJRW5QBTOOOGYDPCJV6N4RKLX44V6XWKNN6ZFAKN6J2F5HQPSNAOV"

# A public key, used only as the transaction's source. See the module docstring: no secret is
# needed, because nothing is signed and nothing is submitted.
SOURCE = "GBITS7JPWINS2T22IWQ5BZK42GZXWOFS4CTFNXML33IX75TOLCDXDA5G"

RPC = "https://soroban-testnet.stellar.org"
NETWORK = "testnet"

# The RPC provider answers `403` to `Python-urllib/3.x`, so the agent is stated rather than left
# at the default. It is also what the operator of an endpoint wants: a request that says who made
# it can be answered, and an anonymous one can only be blocked.
AGENT = "estamora-contract-costs (+https://github.com/Estamora-Soroban-Layers/estamora-conformance-runner)"

HEADERS = {"content-type": "application/json", "user-agent": AGENT}

# Entrypoints, and the arguments that are valid against this contract as it actually is.
#
# `live_until_ledger` is filled in from the current ledger at run time, because the value is only
# meaningful relative to the ledger the simulation runs against.
OPERATIONS: list[tuple[str, str, list[str]]] = [
    ("decimals", "a constant, read", ["decimals"]),
    ("name", "a constant, read", ["name"]),
    ("symbol", "a constant, read", ["symbol"]),
    ("balance", "a balance that does not exist", ["balance", "--id", SOURCE]),
    ("allowance", "an allowance that does not exist", ["allowance", "--from", SOURCE, "--spender", SOURCE]),
    (
        "approve",
        "revoking an allowance with zero",
        ["approve", "--from", SOURCE, "--spender", SOURCE, "--amount", "0", "--live_until_ledger", "{ledger}"],
    ),
    ("burn", "burning nothing", ["burn", "--from", SOURCE, "--amount", "0"]),
    (
        "burn_from",
        "burning nothing against a zero allowance",
        ["burn_from", "--spender", SOURCE, "--from", SOURCE, "--amount", "0"],
    ),
    ("transfer", "refused: the holder has a zero balance", ["transfer", "--from", SOURCE, "--to", SOURCE, "--amount", "1"]),
    ("burn", "refused: a negative amount", ["burn", "--from", SOURCE, "--amount", "-1"]),
    ("approve", "refused: a negative amount", ["approve", "--from", SOURCE, "--spender", SOURCE, "--amount", "-1", "--live_until_ledger", "{ledger}"]),
]


def run(arguments: list[str], *, stdin: str | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(arguments, capture_output=True, text=True, input=stdin)


def latest_ledger() -> int:
    payload = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "getLatestLedger"}).encode()
    request = urllib.request.Request(RPC, data=payload, headers=HEADERS)
    with urllib.request.urlopen(request, timeout=60) as response:
        return int(json.loads(response.read())["result"]["sequence"])


def simulate(envelope: str) -> dict[str, object]:
    payload = json.dumps(
        {"jsonrpc": "2.0", "id": 1, "method": "simulateTransaction", "params": {"transaction": envelope}}
    ).encode()
    request = urllib.request.Request(RPC, data=payload, headers=HEADERS)
    with urllib.request.urlopen(request, timeout=60) as response:
        body = json.loads(response.read())

    if "error" in body:
        message = body["error"].get("message") if isinstance(body["error"], dict) else str(body["error"])
        return {"refused": True, "error": message or "the simulation was refused"}

    result = body["result"]

    # A refused call arrives as a result with an `error` and no resource data at all, because the
    # resource consumed is carried inside the transaction data that only a successful simulation
    # produces. Recorded as a refusal rather than reported as a cost of zero.
    if "transactionData" not in result:
        error = result.get("error", "the simulation was refused")
        return {"refused": True, "error": error if isinstance(error, str) else json.dumps(error)[:200]}

    transaction_data = result["transactionData"]

    decoded = run(
        ["stellar", "xdr", "decode", "--type", "SorobanTransactionData", "--input", "single-base64", "--output", "json", transaction_data]
    )
    if decoded.returncode != 0:
        raise SystemExit(f"could not decode the resource data: {decoded.stderr.strip()}")
    resources = json.loads(decoded.stdout)["resources"]

    return {
        "refused": False,
        "instructions": resources["instructions"],
        "disk_read_bytes": resources["disk_read_bytes"],
        "write_bytes": resources["write_bytes"],
        # In stroops. 1 stroop = 0.0000001 XLM.
        "resource_fee": int(result["minResourceFee"]),
    }


def measure() -> dict[str, object]:
    ledger = latest_ledger()
    rows: list[dict[str, object]] = []

    for name, note, arguments in OPERATIONS:
        resolved = [str(ledger + 1000) if a == "{ledger}" else a for a in arguments]

        built = run(
            [
                "stellar", "contract", "invoke", "--build-only",
                "--id", CONTRACT,
                "--source-account", SOURCE,
                "--network", NETWORK,
                "--", *resolved,
            ]
        )
        if built.returncode != 0:
            raise SystemExit(f"could not build a transaction for {name}: {built.stderr.strip()[:400]}")

        row = {"entrypoint": name, "call": note, **simulate(built.stdout.strip())}
        rows.append(row)
        status = "refused (no resource data)" if row["refused"] else f"{row['instructions']:,} instructions"
        print(f"  {name:<12} {note[:44]:<46} {status}", file=sys.stderr)

    return {
        "contract": CONTRACT,
        "source_account": SOURCE,
        "rpc": RPC,
        "network": NETWORK,
        "latest_ledger": ledger,
        "rows": rows,
    }


def render(capture: dict[str, object], *, markdown: bool) -> str:
    rows = capture["rows"]
    assert isinstance(rows, list)
    lines: list[str] = []

    if markdown:
        lines.append("| entrypoint | call | CPU instructions | ledger bytes | resource fee |")
        lines.append("| --- | --- | ---: | ---: | ---: |")

    for row in rows:
        entrypoint, call = row["entrypoint"], row["call"]
        if row["refused"]:
            instructions, ledger_bytes, fee = "-", "-", "-"
            call = f"refused: {call.split(': ', 1)[-1]}"
        else:
            instructions = f"{row['instructions']:,}"
            written, read = row["write_bytes"], row["disk_read_bytes"]
            ledger_bytes = f"{written:,} written, {read:,} read" if (written or read) else "0"
            fee = f"{row['resource_fee']:,} stroops"
        if markdown:
            lines.append(f"| `{entrypoint}` | {call} | {instructions} | {ledger_bytes} | {fee} |")
        else:
            lines.append(f"{entrypoint:<12} {call:<50} {instructions:>14} {ledger_bytes:>22} {fee:>18}")

    if markdown:
        lines.append("")
        lines.append(
            f"Measured against `{capture['contract'][:8]}…` on {capture['network']} at ledger "
            f"{capture['latest_ledger']}, by `scripts/measure-contract-costs.py`. Instructions and bytes are what "
            "Soroban charges for; the resource fee is their price in stroops, taken from the simulation's "
            "`minResourceFee` and excluding the inclusion fee and any refund of unused bytes."
        )
        lines.append("")
        lines.append(
            "A refused call is listed because its refusal is part of the contract's surface, and it has no "
            "numbers against it because a failed simulation returns no resource data: the resources consumed "
            "are carried inside the transaction data a successful simulation produces, and are absent here "
            "rather than zero."
        )
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    measure_parser = subparsers.add_parser("measure", help="measure the live contract and write the capture to stdout")
    measure_parser.add_argument("--json", action="store_true", help="write JSON (the default)")

    render_parser = subparsers.add_parser("render", help="render the table from a capture")
    render_parser.add_argument("capture", help="a file written by `measure`")
    render_parser.add_argument("--format", choices=["markdown", "text"], default="markdown")

    check_parser = subparsers.add_parser(
        "check", help="assert that a document contains exactly the table this capture renders"
    )
    check_parser.add_argument("capture", help="a file written by `measure`")
    check_parser.add_argument("document", help="the document that is supposed to contain it")

    arguments = parser.parse_args()

    if arguments.command == "measure":
        json.dump(measure(), sys.stdout, indent=2)
        sys.stdout.write("\n")
        return 0

    capture = json.loads(open(arguments.capture, encoding="utf-8").read())

    if arguments.command == "check":
        # The point of the check: a number in a document that is not the number in the capture
        # it claims to come from is worse than no number, because the reader cannot tell.
        table = render(capture, markdown=True).split("\n\n")[0]
        document = open(arguments.document, encoding="utf-8").read()
        if table not in document:
            print(
                f"::error::{arguments.document} does not contain the table that {arguments.capture} renders.\n"
                "Re-render it rather than editing the numbers by hand: "
                f"scripts/measure-contract-costs.py render {arguments.capture}",
                file=sys.stderr,
            )
            return 1
        print(f"{arguments.document} matches the capture ({len(capture['rows'])} rows)")
        return 0

    print(render(capture, markdown=arguments.format == "markdown"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
