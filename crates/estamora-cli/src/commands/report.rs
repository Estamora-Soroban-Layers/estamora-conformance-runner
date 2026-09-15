//! `estamora report` — render a stored result.
//!
//! A conformance report is written once and read many times: on the machine that
//! produced it, in a pull request, in a CI gate built on a different format, and months
//! later by someone checking what was claimed. So the normative document is JSON, and
//! every other rendering is derived from it rather than from the run — which is the only
//! way the renderings can be guaranteed to agree, since there is one source and the
//! renderers read it.
//!
//! # The exit status is the recorded verdict's
//!
//! A pipeline that renders a stored report inside a gate needs the gate to see the
//! verdict rather than the fact that rendering worked. Exiting `0` for a successful
//! rendering of a `NON_CONFORMANT` result would make this command useless as a check and
//! actively misleading as a gate.

use std::io::Write;

use estamora_core::{Error, ErrorClass, ExitCode, Result};

use crate::commands::{Common, Format, emit};

/// Render a stored report.
#[derive(Debug, clap::Args)]
pub struct ReportArgs {
    /// The JSON report to render.
    #[arg(long, value_name = "PATH")]
    pub report: std::path::PathBuf,

    /// The format to render it in.
    #[arg(long, value_enum, default_value_t = Format::Markdown)]
    pub format: Format,

    /// Where the rendering is written. Defaults to standard output.
    #[arg(long, value_name = "PATH")]
    pub out: Option<std::path::PathBuf>,

    /// Exit `0` whatever the recorded verdict.
    #[arg(long)]
    pub no_fail: bool,
}

/// Performs the command.
///
/// # Errors
///
/// Returns a report error when the document cannot be read or parsed. A document that
/// does not parse is not rendered: a rendering of something that was partly understood
/// would look exactly like a rendering of a result.
pub fn execute(args: &ReportArgs, common: &Common, out: &mut dyn Write) -> Result<ExitCode> {
    let text = std::fs::read_to_string(&args.report).map_err(|problem| {
        Error::new(
            ErrorClass::ReportError,
            format!("{} could not be read: {problem}", args.report.display()),
        )
    })?;
    let report = estamora_report::parse(&text)?;
    let rendered = args.format.render(&report, common.verbose)?;
    emit(out, args.out.as_deref(), &rendered)?;

    if args.no_fail {
        return Ok(ExitCode::Success);
    }
    Ok(report.status.exit_code())
}
