//! The `estamora` binary.
//!
//! Thin on purpose. Everything that can change a verdict is in the library, so an
//! integration test drives the same code a user's command line drives rather than a
//! second copy of the pipeline. What is left here is argument parsing, the process's
//! output sinks, logging, and the single place a process status is set.
//!
//! # One exit status, derived
//!
//! The library returns an [`ExitCode`](estamora_core::ExitCode); this file turns it into
//! a number and nothing else. A failure is mapped through the same rule the run's status
//! is, so a network outage that stops a run and a network outage that is reported inside
//! a run exit identically — which is what makes `4` a reliable "the environment failed,
//! retry" signal for a pipeline.
//!
//! # Standard output is for the document
//!
//! Diagnostics go to standard error. A machine-readable report on standard output is how
//! a pipeline consumes a run, and a log line printed beside it is how that pipeline
//! breaks in a way nobody notices until the day it matters.

use std::io::Write as _;

use clap::Parser;

use estamora_cli::commands::{self, Command, Common};
use estamora_cli::errors;

/// The Estamora conformance runner.
#[derive(Debug, Parser)]
#[command(
    name = "estamora",
    version,
    about = "Measure whether a Soroban contract behaves as a conformance profile requires.",
    long_about = "Estamora measures a Soroban contract against a profile: a machine-readable \
statement of what it means for a contract to conform to an interface, a standard or a \
behavioural profile. A profile states requirements about the interface, authorization, \
events, behaviour, state, invariants and failure; Estamora executes the profile's vectors \
against the contract and reports every check it made.\n\n\
Conformance is not security. A conformant verdict says the contract behaved as a named \
profile requires over a named corpus, and nothing more."
)]
struct Cli {
    /// Options every command shares.
    #[command(flatten)]
    common: Common,

    /// What to do.
    #[command(subcommand)]
    command: Command,
}

fn main() {
    // Diagnostics are written to standard error and confined to one line per event, so
    // that they can be enabled in a pipeline that consumes standard output without
    // corrupting it.
    let _ignored = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .try_init();

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(problem) => {
            // `--help` and `--version` are not failures, and Clap knows which is which.
            let _ignored = problem.print();
            std::process::exit(if problem.use_stderr() {
                // `sysexits.h`'s usage status, so that a wrapper script can recognise a
                // command-line mistake without knowing this program.
                errors::exit_code(estamora_core::ErrorClass::UsageError).as_i32()
            } else {
                0
            });
        },
    };

    tracing::debug!(
        command = ?std::mem::discriminant(&cli.command),
        "dispatching"
    );

    let mut out = std::io::stdout().lock();
    let code = match commands::dispatch(cli.command, &cli.common, &mut out) {
        Ok(code) => code,
        Err(problem) => {
            let mut err = std::io::stderr().lock();
            let _ignored = writeln!(err, "{}: {}", problem.class(), problem);
            errors::exit_code_for(&problem)
        },
    };
    let _ignored = out.flush();
    std::process::exit(code.as_i32());
}
