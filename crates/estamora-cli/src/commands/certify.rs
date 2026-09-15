//! `estamora certify` — commit to a result, and check a commitment.
//!
//! A receipt is the smallest artefact that lets somebody who was not present at a run
//! check that the report shown to them is the one the result is about. Two questions are
//! answered separately, and that separation is the whole design:
//!
//! 1. **Is this report the one the receipt is about?** Answered by recomputing the
//!    report's digest. It needs no key, and it is the property that matters most: it is
//!    what stops a report being swapped for a better-looking one.
//! 2. **Who asserted it?** Answered by a signature, and only by a signature checked
//!    against a key the verifier already trusts. A signature checked against a key
//!    carried in the same document establishes that the document agrees with itself and
//!    nothing else — anyone can generate a key pair and sign anything with it.
//!
//! So verification reports an [`Attribution`](estamora_certification::Attribution), and
//! the unattributed case is a value a caller has to look at rather than an error they can
//! ignore. Presenting a self-consistent receipt as a verified one would be the most
//! misleading artefact this project could publish.
//!
//! # What a receipt is not
//!
//! It is not a statement that a contract is correct or safe. It is a statement that a
//! named runner reached a named verdict under a named profile against a named contract,
//! and it is verifiable to exactly that extent. Nothing is published on a ledger, and no
//! on-chain certification contract is implemented: the specification layer defines the
//! normative data model, and making the open-source runner depend on infrastructure the
//! project does not own would invite reading an on-chain record as a stronger claim than
//! the document supports.

use std::io::Write;

use estamora_core::{Error, ErrorClass, ExitCode, Result};

use crate::commands::{Common, emit};

/// Issue or verify a receipt.
#[derive(Debug, clap::Args)]
pub struct CertifyArgs {
    /// What to do.
    #[command(subcommand)]
    pub action: CertifyAction,
}

/// The two things `certify` can do.
#[derive(Debug, clap::Subcommand)]
pub enum CertifyAction {
    /// Issue a receipt committing to a report.
    Issue(IssueArgs),
    /// Check a receipt against the report it claims to be about.
    Verify(VerifyArgs),
}

/// Issue a receipt.
#[derive(Debug, clap::Args)]
pub struct IssueArgs {
    /// The JSON report to commit to.
    #[arg(long, value_name = "PATH")]
    pub report: std::path::PathBuf,

    /// Where the receipt is written.
    #[arg(long, value_name = "PATH")]
    pub out: Option<std::path::PathBuf>,

    /// Sign the receipt with this 32-byte hex-encoded seed.
    #[arg(long, value_name = "HEX")]
    pub signing_key: Option<String>,

    /// The instant the receipt records, as an RFC 3339 date-time. Defaults to now.
    #[arg(long, value_name = "RFC3339")]
    pub issued_at: Option<String>,
}

/// Verify a receipt.
#[derive(Debug, clap::Args)]
pub struct VerifyArgs {
    /// The receipt to check.
    #[arg(long, value_name = "PATH")]
    pub receipt: std::path::PathBuf,

    /// The report the receipt is claimed to be about.
    #[arg(long, value_name = "PATH")]
    pub report: std::path::PathBuf,

    /// The issuer's public key, base64-encoded, which the caller already trusts.
    ///
    /// Supplying it is what turns "the report matches" into "and this key asserted it".
    /// Without it verification still checks the report, and reports that nobody is
    /// identified.
    #[arg(long, value_name = "BASE64")]
    pub public_key: Option<String>,
}

/// Performs the command.
///
/// # Errors
///
/// Returns a certification error when a document cannot be read, a key or signature is
/// malformed, or a signature does not check out under the trusted key.
pub fn execute(args: &CertifyArgs, common: &Common, out: &mut dyn Write) -> Result<ExitCode> {
    let _ = common;
    match &args.action {
        CertifyAction::Issue(issue) => issue_receipt(issue, out),
        CertifyAction::Verify(verify) => verify_receipt(verify, out),
    }
}

/// Issues a receipt for a stored report.
fn issue_receipt(args: &IssueArgs, out: &mut dyn Write) -> Result<ExitCode> {
    let report = read_report(&args.report)?;
    let issued_at = args.issued_at.clone().unwrap_or_else(crate::config::now);
    let receipt = estamora_certification::Receipt::issue(&report, issued_at)?;

    let document = match &args.signing_key {
        Some(seed) => {
            let key = estamora_certification::signing_key_from_hex(seed)?;
            serde_json::to_string_pretty(&estamora_certification::sign(&receipt, &key)?)
        },
        // Unsigned, and honest about it: the receipt still commits to the report, and
        // verification will say that no issuer was identified rather than implying one.
        None => serde_json::to_string_pretty(&estamora_certification::SignedReceipt {
            algorithm: estamora_certification::ALGORITHM.to_owned(),
            public_key: String::new(),
            signature: String::new(),
            receipt,
        }),
    }
    .map_err(|problem| {
        Error::new(
            ErrorClass::CertificationError,
            format!("the receipt could not be serialized: {problem}"),
        )
    })?;

    emit(out, args.out.as_deref(), &format!("{document}\n"))?;
    Ok(ExitCode::Success)
}

/// Verifies a receipt against a report.
fn verify_receipt(args: &VerifyArgs, out: &mut dyn Write) -> Result<ExitCode> {
    use std::fmt::Write as _;

    let report = read_report(&args.report)?;
    let text = std::fs::read_to_string(&args.receipt).map_err(|problem| {
        Error::new(
            ErrorClass::CertificationError,
            format!("{} could not be read: {problem}", args.receipt.display()),
        )
    })?;
    let signed: estamora_certification::SignedReceipt =
        serde_json::from_str(&text).map_err(|problem| {
            Error::new(
                ErrorClass::CertificationError,
                format!(
                    "{} is not a signed receipt: {problem}",
                    args.receipt.display()
                ),
            )
        })?;

    let trusted = match &args.public_key {
        Some(encoded) => Some(estamora_certification::verifying_key_from_base64(encoded)?),
        None => None,
    };
    let verification = estamora_certification::verify(&signed, &report, trusted.as_ref())?;

    let mut rendered = String::new();
    let _ = writeln!(rendered, "report digest {}", verification.report_digest);
    match &verification.attribution {
        estamora_certification::Attribution::Attributed { key } => {
            let _ = writeln!(rendered, "attributed to key {key}");
        },
        estamora_certification::Attribution::Unattributed { reason } => {
            let _ = writeln!(rendered, "attributed to nobody: {reason}");
        },
    }
    let _ = writeln!(
        rendered,
        "verdict {} (exit {}) under {}@{} for {} on {}",
        signed.receipt.result.status,
        signed.receipt.result.exit_code,
        signed.receipt.profile.id,
        signed.receipt.profile.version,
        signed.receipt.target.contract,
        signed.receipt.target.network
    );
    let _ = writeln!(
        rendered,
        "\nA receipt attests that a named runner reached a named verdict under a named \
         profile. It is not a statement that the contract is correct or safe, and a valid \
         signature over a wrong conclusion is still a wrong conclusion."
    );
    emit(out, None, &rendered)?;
    Ok(ExitCode::Success)
}

/// Reads and parses a report.
fn read_report(path: &std::path::Path) -> Result<estamora_report::model::Report> {
    let text = std::fs::read_to_string(path).map_err(|problem| {
        Error::new(
            ErrorClass::ReportError,
            format!("{} could not be read: {problem}", path.display()),
        )
    })?;
    estamora_report::parse(&text)
}
