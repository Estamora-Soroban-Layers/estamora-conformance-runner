# Fuzzing

Four targets, one per boundary where this runner accepts something it did not write.
Each is a document that arrives from somewhere else and is parsed before anything
evaluates it, and the property being tested is the same in all four: for *every*
input, the runner either produces a usable structure or a diagnostic — never a
panic, never unbounded work, and never a result that looks like a real one.

| Target | The untrusted input |
| --- | --- |
| `profile-fuzzer` | The six documents of a profile bundle, its manifest, and its identity. |
| `vector-fuzzer` | A vector: the measuring document, and the most deeply nested structure in the format. |
| `assertion-fuzzer` | The expression algebra — value expressions, predicates, literals, operators. |
| `report-fuzzer` | A stored report, which is both output and, for `estamora report`, input. |

## Running them

```console
$ rustup toolchain install nightly
$ cargo install cargo-fuzz
$ cd fuzz
$ cargo +nightly fuzz run profile-fuzzer
$ cargo +nightly fuzz list              # all four
```

CI runs each target for two minutes on a pull request and ten minutes nightly, and
uploads a crashing input as an artefact when one is found. The corpus is cached
between runs, so a seed found once is not rediscovered.

## Why they are in their own workspace

`fuzz/Cargo.toml` has an empty `[workspace]` table, which makes this directory its
own root. That is not tidiness: `cargo-fuzz` builds with libFuzzer and an address
sanitizer, which needs nightly, and making these targets members of the root
workspace would force the pinned stable toolchain that builds the runner *and its
contract fixtures* to be a nightly instead. A conformance result names the toolchain
that produced it; that toolchain should not be a moving one.

Directories rather than the conventional single `fuzz_targets/`, one per boundary,
because each has a different thing to keep out of the runner and a reader should be
able to see which is which. Cargo finds them through the explicit `[[bin]]` entries
in `fuzz/Cargo.toml`, so `cargo fuzz run <name>` is unaffected.

## What a seed is for

`fuzz/<target>/seeds/` is tracked, and a file belongs there when it is *interesting*
rather than merely crashing — an input that reaches a code path a random byte string
would not, such as a report with a maximal vector count or a profile whose manifest
names a path with a separator in it. A crashing input goes in a regression test, not
in a corpus.

## What fuzzing here does not prove

Nothing about a contract. These targets exercise Estamora's own parsers, which is a
statement about this tool's robustness and not about any contract's security. Reading
a clean fuzz run as evidence about a contract is exactly the mistake `docs/security.md`
exists to prevent.
