//! `estamora run` — measure a contract against a profile.
//!
//! The only command that reaches a verdict, and the one whose exit status CI reads. It
//! validates before it executes, so an unusable requirement set is reported as a
//! profile failure rather than as anything about the contract.
//!
//! # The report is emitted even when the verdict is bad
//!
//! A non-conformant result is a result. The report is written and the process exits `1`
//! only after it has been, because a CI gate that failed without leaving the document
//! that says why is a gate that gets switched off.
//!
//! # `--no-fail` exists for the case where the verdict is the artefact
//!
//! A pipeline that publishes reports rather than gating on them needs the run to
//! succeed while recording a non-conformant verdict. It changes the exit status and
//! nothing else: the report still says `NON_CONFORMANT`, so a reader cannot be misled
//! into thinking the contract passed.

use std::io::Write;
use std::path::PathBuf;

use estamora_core::{Error, ErrorClass, ExitCode, Result};

use crate::commands::{Common, Format, emit};
use crate::config::{RunConfig, resolve_profile};
use crate::engine::{Target, run};
use crate::output;

/// Measure a contract against a profile.
#[derive(Debug, clap::Args)]
pub struct RunArgs {
    /// The profile to execute: `id@version`, a path under the checkout's `profiles/`,
    /// or a directory holding a `profile.yaml`.
    #[arg(long, value_name = "PROFILE")]
    pub profile: String,

    /// The contract to measure: `fixture:<name>`, a path ending in `.wasm`, or a
    /// contract identifier together with `--network`.
    #[arg(long, value_name = "CONTRACT")]
    pub contract: String,

    /// The network the contract lives on. Required for a deployed contract.
    #[arg(long, value_name = "NETWORK")]
    pub network: Option<String>,

    /// Select only the vectors carrying every one of these tags.
    #[arg(long = "tag", value_name = "TAG")]
    pub tags: Vec<String>,

    /// The format the report is written in.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    pub format: Format,

    /// Where the report is written. Defaults to standard output.
    #[arg(long, value_name = "PATH")]
    pub out: Option<PathBuf>,

    /// Additionally write the normative JSON report here.
    ///
    /// Separate from `--out` because a human-readable rendering on a terminal and the
    /// document another tool consumes are different artefacts, and a pipeline usually
    /// wants both from one run.
    #[arg(long = "report", value_name = "PATH")]
    pub report_path: Option<PathBuf>,

    /// Write a certification receipt here.
    #[arg(long, value_name = "PATH")]
    pub receipt: Option<PathBuf>,

    /// Sign the receipt with this 32-byte hex-encoded seed.
    ///
    /// Without it the receipt is issued unsigned: it still commits to the report it is
    /// about, but it identifies no issuer, and verification says so rather than
    /// presenting internal consistency as evidence of authorship.
    #[arg(long, value_name = "HEX")]
    pub signing_key: Option<String>,

    /// Exit `0` whatever the verdict, having still recorded it.
    #[arg(long)]
    pub no_fail: bool,
}

/// Performs the command.
///
/// # Errors
///
/// Returns a failure when the run could not be attempted: an unusable profile, an
/// unusable corpus, a contract that could not be resolved, or a world that could not be
/// prepared. A contract that violated a requirement is not a failure of the command; it
/// is returned as [`ExitCode::NonConformant`].
pub fn execute(args: &RunArgs, common: &Common, out: &mut dyn Write) -> Result<ExitCode> {
    let spec_root = common.spec_root()?;
    let profile_root = resolve_profile(&spec_root, &args.profile)?;
    let target = Target::parse(&args.contract, args.network.as_deref())?;

    let mut config = RunConfig::new(profile_root, spec_root, target).with_tags(args.tags.clone());
    if let Some(key) = &args.signing_key {
        config = config.with_signing_key(key.clone());
    }
    if let Some(path) = &args.receipt {
        config = config.with_receipt_path(path.clone());
    }

    let outcome = run(&config)?;

    // The normative document first, so that a failure to write a convenience rendering
    // cannot lose the artefact that carries the result.
    if let Some(path) = &args.report_path {
        emit(
            out,
            Some(path),
            &estamora_report::render_json(&outcome.report, true)?,
        )?;
    }
    let rendered = args.format.render(&outcome.report, common.verbose)?;
    emit(out, args.out.as_deref(), &rendered)?;

    if let Some(receipt) = &outcome.receipt {
        let path = args.receipt.clone().ok_or_else(|| {
            Error::new(
                ErrorClass::InternalError,
                "a receipt was produced without a path to write it to",
            )
        })?;
        let document = serde_json::to_string_pretty(receipt).map_err(|problem| {
            Error::new(
                ErrorClass::CertificationError,
                format!("the receipt could not be serialized: {problem}"),
            )
        })?;
        emit(out, Some(&path), &format!("{document}\n"))?;
    }

    if !outcome.findings.is_empty() && common.verbose {
        emit(out, None, &output::findings(&outcome.findings))?;
    }

    let status = outcome.report.status;
    let exit = if args.no_fail {
        ExitCode::Success
    } else {
        status.exit_code()
    };
    Ok(exit)
}
