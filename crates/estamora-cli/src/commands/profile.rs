//! `estamora profile` — say what a profile requires.
//!
//! Answerable without a contract and without a deployment, which is the point: "what does
//! this profile require" and "does this contract satisfy it" are different questions, and
//! a command surface that could only answer the second would make a profile impossible to
//! review on its own.
//!
//! The method surface is listed with each method's requirement status, because a profile
//! that marks a method optional and one that requires it are different standards, and the
//! difference decides whether a failure decides a run.

use std::io::Write;

use estamora_core::{ExitCode, Result};
use estamora_profile::ProfileBundle;

use crate::commands::{Common, Format, emit};
use crate::config::resolve_profile;

/// Describe a profile.
#[derive(Debug, clap::Args)]
pub struct ProfileArgs {
    /// The profile to describe: `id@version`, a path under the checkout's `profiles/`,
    /// or a directory holding a `profile.yaml`.
    #[arg(long, value_name = "PROFILE")]
    pub profile: String,

    /// The format the description is written in.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    pub format: Format,

    /// Where the description is written. Defaults to standard output.
    #[arg(long, value_name = "PATH")]
    pub out: Option<std::path::PathBuf>,
}

/// Performs the command.
///
/// # Errors
///
/// Returns a profile error when the bundle cannot be resolved or loaded. A profile that
/// is malformed is not described: a summary of a document that could not be parsed would
/// be a summary of something else.
pub fn execute(args: &ProfileArgs, common: &Common, out: &mut dyn Write) -> Result<ExitCode> {
    let spec_root = common.spec_root()?;
    let profile_root = resolve_profile(&spec_root, &args.profile)?;
    let profile = ProfileBundle::load(&profile_root)?;

    let rendered = match args.format {
        Format::Json => serde_json::to_string_pretty(&serde_json::json!({
            "reference": profile.reference().to_string(),
            "estamora_spec_version": profile.spec_version_text(),
            "specification": {
                "name": profile.document().profile.specification.name,
                "version": profile.document().profile.specification.version,
                "url": profile.document().profile.specification.url,
            },
            "status": status_name(profile.document().profile.status),
            "summary": profile.document().profile.summary,
            "methods": profile.documents().methods.methods.iter().map(|method| serde_json::json!({
                "id": method.id,
                "name": method.name,
                "requirement": requirement_name(method.requirement),
                "arguments": method.args.iter().map(|argument| &argument.name).collect::<Vec<&String>>(),
                "returns": estamora_assertions::canonical_type(&method.returns.type_expr),
            })).collect::<Vec<_>>(),
        }))
        .map_err(|problem| {
            estamora_core::Error::new(
                estamora_core::ErrorClass::ReportError,
                format!("the profile description could not be serialized: {problem}"),
            )
        })?,
        _ => describe(&profile),
    };

    emit(out, args.out.as_deref(), &rendered)?;
    Ok(ExitCode::Success)
}

/// The profile lifecycle stage, as its own vocabulary spells it.
///
/// Spelled here rather than derived through serde so that the terminal rendering and
/// the JSON rendering cannot disagree, and so that a new lifecycle stage is a compile
/// error rather than a silently different word in one of the two.
const fn status_name(status: estamora_profile::ProfileStatus) -> &'static str {
    match status {
        estamora_profile::ProfileStatus::Draft => "draft",
        estamora_profile::ProfileStatus::Experimental => "experimental",
        estamora_profile::ProfileStatus::Stable => "stable",
        estamora_profile::ProfileStatus::Deprecated => "deprecated",
    }
}

/// Whether a requirement must hold, as its own vocabulary spells it.
const fn requirement_name(status: estamora_profile::RequirementStatus) -> &'static str {
    match status {
        estamora_profile::RequirementStatus::Required => "required",
        estamora_profile::RequirementStatus::Optional => "optional",
        estamora_profile::RequirementStatus::Forbidden => "forbidden",
    }
}

/// The terminal rendering of a profile.
fn describe(profile: &ProfileBundle) -> String {
    use std::fmt::Write as _;

    let metadata = &profile.document().profile;
    let mut out = String::new();
    let _ = writeln!(out, "{}", "─".repeat(72));
    let _ = writeln!(
        out,
        "{} {} · {}",
        metadata.id, metadata.version, metadata.title
    );
    let _ = writeln!(
        out,
        "status {} · estamora-spec {}",
        status_name(metadata.status),
        profile.spec_version_text()
    );
    let _ = writeln!(
        out,
        "upstream {} {} ({})",
        metadata.specification.name, metadata.specification.version, metadata.specification.url
    );
    let _ = writeln!(out, "license {}", metadata.license);
    let _ = writeln!(out, "digest {}", profile.reference());
    let _ = writeln!(out, "{}", "─".repeat(72));
    let _ = writeln!(out, "{}", metadata.summary);
    let _ = writeln!(out, "{}", "─".repeat(72));
    let _ = writeln!(out, "method requirements");
    for method in &profile.documents().methods.methods {
        let parameters: Vec<&str> = method
            .args
            .iter()
            .map(|argument| argument.name.as_str())
            .collect();
        let _ = writeln!(
            out,
            "  {:<10} {:<12} {}({}) -> {}",
            requirement_name(method.requirement),
            method.id,
            method.name,
            parameters.join(", "),
            estamora_assertions::canonical_type(&method.returns.type_expr)
        );
    }
    let documents = profile.documents();
    let _ = writeln!(out, "{}", "─".repeat(72));
    let _ = writeln!(out, "requirements declared");
    let _ = writeln!(
        out,
        "  authorization rules {}",
        documents.authorization.authorization_rules.len()
    );
    let _ = writeln!(
        out,
        "  events              {}",
        documents.events.events.len()
    );
    let _ = writeln!(
        out,
        "  behaviours          {}",
        documents.behavior.behaviors.len()
    );
    let _ = writeln!(
        out,
        "  invariants          {}",
        documents.invariants.invariants.len()
    );
    let _ = writeln!(
        out,
        "  failures            {}",
        documents.failures.failures.len()
    );
    let _ = writeln!(
        out,
        "  vector directories  {}",
        profile.vector_directories().join(", ")
    );
    out
}
