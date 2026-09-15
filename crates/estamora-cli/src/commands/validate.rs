//! `estamora validate` — check that a requirement set is usable.
//!
//! This is the command CI runs on a specification change, and the one a profile author
//! runs before proposing one. It loads the bundle, resolves the corpus the bundle
//! declares, checks every cross-reference between the two, and reports every defect it
//! found — not the first, because a contributor fixing one problem per attempt is a
//! contributor who gives up.
//!
//! It never deploys anything. A profile is a document, and whether it is well formed is
//! independent of whether any contract satisfies it.

use std::io::Write;

use estamora_core::{Error, ErrorClass, ExitCode, Result};

use crate::commands::{Common, Format, emit};
use crate::config::{RunConfig, resolve_profile};
use crate::engine::{Target, validate};
use crate::output;

/// Validate a profile and its corpus.
#[derive(Debug, clap::Args)]
pub struct ValidateArgs {
    /// The profile to validate: `id@version`, a path under the checkout's `profiles/`,
    /// or a directory holding a `profile.yaml`.
    #[arg(long, value_name = "PROFILE")]
    pub profile: String,

    /// The format the result is written in.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    pub format: Format,

    /// Where the result is written. Defaults to standard output.
    #[arg(long, value_name = "PATH")]
    pub out: Option<std::path::PathBuf>,
}

/// Performs the command.
///
/// # Errors
///
/// Returns a profile or vector error when the requirement set is unusable, which is
/// reported as [`ExitCode::SpecificationInvalid`] so that CI can tell a broken profile
/// from a broken contract.
pub fn execute(args: &ValidateArgs, common: &Common, out: &mut dyn Write) -> Result<ExitCode> {
    let spec_root = common.spec_root()?;
    let profile_root = resolve_profile(&spec_root, &args.profile)?;
    // The target is parsed but never deployed: `validate` answers a question about the
    // requirements, and naming a contract it would not measure would be a promise the
    // command does not keep.
    let config = RunConfig::new(
        profile_root,
        spec_root.clone(),
        Target::parse("fixture:none", None)?,
    );

    let validated = validate(&config)?;

    let rendered = match args.format {
        Format::Json => serde_json::to_string_pretty(&serde_json::json!({
            "reference": validated.reference,
            "estamora_spec_version": validated.spec_version,
            "profile_digest": validated.profile_digest,
            "corpus_digest": validated.corpus_digest,
            "requirements": validated
                .requirements
                .iter()
                .map(|(what, count)| serde_json::json!({ "kind": what, "count": count }))
                .collect::<Vec<_>>(),
            "vectors": validated.vectors,
            "findings": validated.findings.iter().map(|finding| serde_json::json!({
                "severity": finding.severity.as_str(),
                "class": finding.class.as_str(),
                "message": finding.message,
            })).collect::<Vec<_>>(),
        }))
        .map_err(|problem| {
            Error::new(
                ErrorClass::ReportError,
                format!("the validation result could not be serialized: {problem}"),
            )
        })?,
        _ => output::validated(&validated),
    };

    emit(out, args.out.as_deref(), &rendered)?;
    Ok(ExitCode::Success)
}
