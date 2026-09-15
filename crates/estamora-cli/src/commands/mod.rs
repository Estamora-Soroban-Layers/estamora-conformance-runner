//! The command surface.
//!
//! Six commands, and each one answers a different question:
//!
//! | Command | Question |
//! | ------- | -------- |
//! | [`run`] | Does this contract behave as this profile requires? |
//! | [`inspect`] | What does this contract expose? |
//! | [`profile`] | What does this profile require? |
//! | [`validate`] | Are these requirements usable at all? |
//! | [`report`] | Render a result someone can read. |
//! | [`certify`] | Commit to a result, and check a commitment. |
//!
//! The split matters because three of them are answerable without executing anything,
//! and a run that conflated them would have no way to say "the profile is wrong" as
//! distinct from "the contract is wrong". `validate` and `profile` never deploy
//! anything; `inspect` deploys and reads, and states that an interface is not evidence
//! of behaviour; only `run` reaches a verdict.
//!
//! Every command returns an [`ExitCode`] rather than exiting, so the whole surface is
//! testable as a library and there is exactly one place a process status is set.

pub mod certify;
pub mod inspect;
pub mod profile;
pub mod report;
pub mod run;
pub mod validate;

use std::io::Write;

use estamora_core::{Error, ErrorClass, ExitCode, Result};

/// Options every command shares.
#[derive(Debug, clap::Args)]
pub struct Common {
    /// A checkout of `estamora-conformance-spec`.
    ///
    /// Defaults to `ESTAMORA_SPEC_REPO`, then to the sibling directory, because the two
    /// repositories are developed beside each other and CI checks both out into one
    /// workspace.
    #[arg(long, global = true, value_name = "PATH")]
    pub spec: Option<std::path::PathBuf>,

    /// List every check performed rather than only the ones that did not hold.
    #[arg(long, global = true)]
    pub verbose: bool,
}

impl Common {
    /// The specification checkout to read requirements from.
    ///
    /// # Errors
    ///
    /// Returns a profile error when the named or defaulted directory is not a
    /// directory. Never falls back silently: measuring a different checkout from the
    /// one that was named would attribute a result to requirements that were not read.
    pub fn spec_root(&self) -> Result<std::path::PathBuf> {
        match &self.spec {
            Some(path) if path.is_dir() => Ok(path.clone()),
            Some(path) => Err(Error::new(
                ErrorClass::ProfileError,
                format!("--spec names {} and it is not a directory", path.display()),
            )
            .with_context("path", path.display().to_string())),
            None => crate::config::spec_root(),
        }
    }
}

/// A command to perform.
#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Measure a contract against a profile and produce a verdict.
    Run(run::RunArgs),
    /// Read a contract's interface without executing anything against it.
    Inspect(inspect::InspectArgs),
    /// Describe what a profile requires.
    Profile(profile::ProfileArgs),
    /// Validate a profile and its vector corpus without executing anything.
    Validate(validate::ValidateArgs),
    /// Render a stored conformance report.
    Report(report::ReportArgs),
    /// Issue or verify a certification receipt.
    Certify(certify::CertifyArgs),
}

/// Performs a command.
///
/// # Errors
///
/// Returns a failure when the command could not be performed. A run that *reached* a
/// non-conformant verdict is not an error: it is a result, and it is returned as
/// [`ExitCode::NonConformant`] so that CI can tell the two apart.
pub fn dispatch(command: Command, common: &Common, out: &mut dyn Write) -> Result<ExitCode> {
    match command {
        Command::Run(args) => run::execute(&args, common, out),
        Command::Inspect(args) => inspect::execute(&args, common, out),
        Command::Profile(args) => profile::execute(&args, common, out),
        Command::Validate(args) => validate::execute(&args, common, out),
        Command::Report(args) => report::execute(&args, common, out),
        Command::Certify(args) => certify::execute(&args, common, out),
    }
}

/// The format a document is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Format {
    /// The normative document: the report itself, as the published schema defines it.
    #[default]
    Json,
    /// A rendering for a reviewer reading a pull request.
    Markdown,
    /// A rendering for an existing CI gate.
    Junit,
    /// A rendering for a terminal.
    Text,
}

impl Format {
    /// Renders a report in this format.
    ///
    /// # Errors
    ///
    /// Returns a report error when the document cannot be produced.
    pub fn render(self, report: &estamora_report::model::Report, verbose: bool) -> Result<String> {
        match self {
            Self::Json => estamora_report::render_json(report, true),
            Self::Markdown => estamora_report::render_markdown(report),
            Self::Junit => estamora_report::render_junit(report),
            Self::Text => Ok(crate::output::report(report, verbose)),
        }
    }
}

/// Writes `contents` to `path`, or to `out` when no path was given.
///
/// The sink is passed in rather than taken from the process, which is what lets a test
/// drive a command and read exactly what a user would see. Nothing in this crate
/// writes to standard output on its own, because the workspace forbids it: a stray line
/// on standard output is how a machine-readable document stops being one.
///
/// # Errors
///
/// Returns a report error when the file or the sink cannot be written.
pub fn emit(out: &mut dyn Write, path: Option<&std::path::Path>, contents: &str) -> Result<()> {
    match path {
        Some(path) => crate::output::write(path, contents),
        None => out.write_all(contents.as_bytes()).map_err(|problem| {
            Error::new(
                ErrorClass::ReportError,
                format!("the output could not be written: {problem}"),
            )
        }),
    }
}
