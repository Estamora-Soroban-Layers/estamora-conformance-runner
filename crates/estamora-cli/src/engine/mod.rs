//! The pipeline.
//!
//! Everything between "here is a profile and a contract" and "here is a verdict" lives
//! here, and the order is not incidental:
//!
//! 1. **The profile is loaded and validated before anything is executed.** A bundle
//!    that is malformed, self-inconsistent or written against a format version this
//!    runner does not understand must stop the run at that point, because a verdict
//!    produced against requirements that were partly ignored is worse than no verdict.
//! 2. **The corpus is resolved from what the profile declares**, not from a directory
//!    listing, so a profile cannot be measured against a sibling's vectors.
//! 3. **Every selected vector is executed**, each in its own world, and each producing
//!    its own per-assertion result.
//! 4. **The results are reduced to one status by a rule that lives in `estamora-core`**,
//!    so the report and the process exit code cannot be derived from two different
//!    reductions of the same evidence.
//!
//! # A document defect does not hide the rest of the corpus
//!
//! If one vector disagrees with the profile it is declared against, that vector is
//! reported as undecided with the reason and the run continues: the other vectors still
//! measured something, and the run's verdict is undecided rather than clean. Every
//! *other* class of failure stops the run, because a contract that cannot be resolved
//! or a host that cannot run fails every vector identically and a report full of
//! repetitions of one environment failure would be less useful than the failure itself.
//!
//! # Nothing is silently skipped
//!
//! A vector that could not be executed is `error`; a vector whose world could not be
//! prepared is `skipped`. Both leave the requirement unexercised, and either makes the
//! run's verdict undecided rather than `CONFORMANT`. That is the whole reason two
//! statuses exist instead of one: a report that quietly omitted an unexercised
//! requirement would be a report claiming more coverage than it has.

pub mod digest;
pub mod observe;
pub mod record;
pub mod scenario;
pub mod target;
pub mod values;

use std::collections::BTreeSet;

use estamora_assertions::VectorOutcome;
use estamora_certification::{Receipt, SignedReceipt};
use estamora_core::{Diagnostic, ErrorClass, Result};
use estamora_profile::{ProfileBundle, RequirementStatus};
use estamora_report::model::vector_id_is_acceptable;
use estamora_report::model::{
    CorpusIdentity, ProfileIdentity, REPORT_SCHEMA, Report, Summary, Target as ReportTarget,
    report_vector, tally,
};
use estamora_soroban::{ExposedInterface, LocalHost};
use estamora_vectors::{Vector, VectorCorpus};

use crate::config::{RunConfig, runner_identity};

pub use target::{Seeding, Target};

/// What a run established.
#[derive(Debug)]
pub struct RunOutcome {
    /// The report, as the specification's schema defines it.
    pub report: Report,
    /// The corpus that was executed, in the order it was executed.
    pub outcomes: Vec<VectorOutcome>,
    /// Findings that did not decide the run: profile warnings and corpus warnings.
    pub findings: Vec<Diagnostic>,
    /// The receipt, when one was asked for.
    pub receipt: Option<SignedReceipt>,
}

/// What a profile and its corpus look like, without executing anything.
#[derive(Debug)]
pub struct Validated {
    /// The profile's reference, as `id@version`.
    pub reference: String,
    /// The specification format version the bundle declares.
    pub spec_version: String,
    /// The content digest of the bundle.
    pub profile_digest: String,
    /// The content digest of the corpus.
    pub corpus_digest: String,
    /// How many requirements of each kind the bundle declares.
    pub requirements: Vec<(&'static str, usize)>,
    /// The identifiers of the vectors the corpus resolves to.
    pub vectors: Vec<String>,
    /// Findings that did not stop the load.
    pub findings: Vec<Diagnostic>,
}

/// What a contract exposes, without executing anything against it.
#[derive(Debug)]
pub struct Inspection {
    /// The target, as it was named.
    pub target: String,
    /// The network.
    pub network: String,
    /// The digest of the artifact, where one was deployed.
    pub wasm_hash: Option<String>,
    /// How a vector's opening state can be established against it.
    pub seeding: Seeding,
    /// The interface the contract publishes.
    pub interface: ExposedInterface,
}

/// Validates the profile and resolves the corpus, without executing anything.
///
/// # Errors
///
/// Returns a profile or vector error for a bundle or corpus that cannot be used. This
/// is the check that runs before any measurement, so that an unusable requirement set
/// is never reported as anything about a contract.
pub fn validate(config: &RunConfig) -> Result<Validated> {
    let profile = ProfileBundle::load(&config.profile_root)?;
    let corpus = VectorCorpus::load(&profile, config.spec_root.join("vectors"))?;
    let mut findings = Vec::new();
    findings.extend(profile.diagnostics().iter().cloned());
    findings.extend(corpus.diagnostics().iter().cloned());

    let documents = profile.documents();
    let vectors: Vec<&Vector> = corpus.vectors().iter().collect();

    Ok(Validated {
        reference: profile.reference().to_string(),
        spec_version: profile.spec_version_text().to_owned(),
        profile_digest: digest::profile(&config.profile_root)?.to_string(),
        corpus_digest: digest::corpus(&vectors)?.to_string(),
        requirements: vec![
            ("methods", documents.methods.methods.len()),
            (
                "authorization_rules",
                documents.authorization.authorization_rules.len(),
            ),
            ("events", documents.events.events.len()),
            ("behaviors", documents.behavior.behaviors.len()),
            ("invariants", documents.invariants.invariants.len()),
            ("failures", documents.failures.failures.len()),
        ],
        vectors: vectors
            .iter()
            .map(|vector| vector.id().to_owned())
            .collect(),
        findings,
    })
}

/// Resolves `config`'s target and reads its interface, without executing anything.
///
/// # Errors
///
/// Returns a contract resolution error when the target cannot be deployed or its
/// interface cannot be read. This is an inspection, not a verdict: a contract that
/// exposes every declared method is still not evidence of behavioural conformance, and
/// [`inspect`] therefore never claims otherwise.
pub fn inspect(config: &RunConfig) -> Result<Inspection> {
    let host = LocalHost::at_default_point();
    let artifact = config.target.fetch()?;
    let deployment = target::deploy(&config.target, &host, artifact.as_ref())?;
    Ok(Inspection {
        target: config.target.describe(),
        network: deployment.network,
        wasm_hash: deployment.wasm_hash,
        seeding: deployment.seeding,
        interface: deployment.interface,
    })
}

/// Executes `config` and returns what it established.
///
/// # Errors
///
/// Returns a profile error for an unusable bundle, a vector error for an unusable
/// corpus, a contract resolution error when the target cannot be deployed, and an
/// execution error when a vector's declared world cannot be put into the contract. A
/// contract that violates a requirement is not an error: it is a report.
pub fn run(config: &RunConfig) -> Result<RunOutcome> {
    let profile = ProfileBundle::load(&config.profile_root)?;
    let corpus = VectorCorpus::load(&profile, config.spec_root.join("vectors"))?;

    let mut findings = Vec::new();
    findings.extend(profile.diagnostics().iter().cloned());
    findings.extend(corpus.diagnostics().iter().cloned());

    let selected: Vec<&Vector> = if config.tags.is_empty() {
        corpus.vectors().iter().collect()
    } else {
        corpus.matching(&config.tags)
    };
    if selected.is_empty() && !corpus.is_empty() {
        // A tag filter that selects nothing is a run that measured nothing, and saying
        // so is the difference between an undecided verdict and a clean one.
        findings.push(Diagnostic::warning(
            ErrorClass::VectorError,
            format!(
                "the corpus holds {} vector(s) and none of them carry every one of the tags {}; \
                 nothing was executed",
                corpus.len(),
                config.tags.join(", ")
            ),
        ));
    }

    let mut outcomes: Vec<VectorOutcome> = Vec::new();
    // Resolved once, before anything is measured. Every vector is measured in a fresh
    // host at its own ledger point, so the contract is re-registered for each one; a
    // remote contract fetched per vector would make the run's cost scale with the corpus
    // and would let a node that advanced mid-corpus change the artifact half way through
    // its own report.
    let artifact = config.target.fetch()?;
    // The deployment facts are a property of the target rather than of a vector, so the
    // first one that reports them is the one the report carries.
    let mut deployment_facts: Option<(String, Option<String>)> = None;
    for vector in &selected {
        match scenario::execute(
            &config.target,
            artifact.as_ref(),
            &profile,
            vector.document(),
        ) {
            Ok(executed) => {
                if deployment_facts.is_none() {
                    deployment_facts = Some((executed.network, executed.wasm_hash));
                }
                outcomes.push(executed.outcome);
            },
            // A vector that disagrees with the profile it is declared against is a
            // defect in the corpus. It is reported against that vector and the rest of
            // the corpus is still measured, because a document defect involving one
            // scenario is no reason to hide the results of every other.
            Err(problem) if problem.class() == ErrorClass::VectorError => {
                findings.push(
                    Diagnostic::error(problem.class(), problem.message())
                        .with_context("vector", vector.id().to_owned()),
                );
                outcomes.push(VectorOutcome::could_not_run(
                    vector.id().to_owned(),
                    vector.document().kind.as_str(),
                    "vector-unusable",
                    problem.to_string(),
                ));
            },
            // Anything else — an artifact that cannot be deployed, a host that cannot
            // run, a world that cannot be prepared — fails every vector identically, so
            // the failure itself is reported rather than repeated once per vector.
            Err(problem) => return Err(problem),
        }
    }

    let (network, wasm_hash) =
        deployment_facts.unwrap_or_else(|| (config.target.network().to_owned(), None));
    let report = assemble(config, &profile, &selected, &outcomes, &network, wasm_hash)?;

    let receipt = match (&config.signing_key, &config.receipt_path) {
        (Some(seed), _) => {
            let receipt = Receipt::issue(&report, config.generated_at.clone())?;
            let key = estamora_certification::signing_key_from_hex(seed)?;
            Some(estamora_certification::sign(&receipt, &key)?)
        },
        (None, Some(_)) => Some(SignedReceipt {
            // An unsigned receipt is a legitimate artefact: it commits to the report it
            // is about, which is the property that matters most. What it does not do is
            // identify an issuer, and the verification result says so rather than
            // presenting internal consistency as evidence of authorship.
            algorithm: estamora_certification::ALGORITHM.to_owned(),
            public_key: String::new(),
            signature: String::new(),
            receipt: Receipt::issue(&report, config.generated_at.clone())?,
        }),
        (None, None) => None,
    };

    Ok(RunOutcome {
        report,
        outcomes,
        findings,
        receipt,
    })
}

/// Assembles the report from the results.
///
/// # Errors
///
/// Returns a report error when a vector identifier is not one the published schema
/// accepts. That is checked rather than reworded: a report whose identifiers fail the
/// schema is one no independent tool can read, and silently rewriting a profile
/// author's identifier would make a failure harder to locate than a refusal is.
fn assemble(
    config: &RunConfig,
    profile: &ProfileBundle,
    selected: &[&Vector],
    outcomes: &[VectorOutcome],
    network: &str,
    wasm_hash: Option<String>,
) -> Result<Report> {
    let mut results = Vec::with_capacity(outcomes.len());
    for outcome in outcomes {
        if !vector_id_is_acceptable(&outcome.vector_id) {
            return Err(estamora_core::Error::new(
                ErrorClass::ReportError,
                format!(
                    "the vector identifier {:?} is not one the published report schema accepts, \
                     so the run cannot be reported without either altering it or emitting a \
                     document no independent tool can read",
                    outcome.vector_id
                ),
            )
            .with_context("vector", outcome.vector_id.clone()));
        }
        results.push(report_vector(outcome));
    }

    // A vector is required when the method it exercises is required by the profile. A
    // vector for an optional method that fails is recorded and does not decide the run,
    // which is the whole meaning of the profile marking the method optional.
    let required: BTreeSet<String> = selected
        .iter()
        .filter(|vector| {
            profile
                .documents()
                .method(&vector.document().method)
                .is_none_or(|method| method.requirement == RequirementStatus::Required)
        })
        .map(|vector| vector.id().to_owned())
        .collect();

    let summary = Summary::of(&results);
    let status = tally(&results, &required).status();

    Ok(Report {
        schema: Some(REPORT_SCHEMA.to_owned()),
        estamora_spec_version: profile.spec_version_text().to_owned(),
        runner: runner_identity(),
        generated_at: config.generated_at.clone(),
        target: ReportTarget {
            contract: config.target.describe(),
            network: network.to_owned(),
            wasm_hash,
            // Contract-supplied metadata is untrusted input and this runner reads none
            // of it, so the map is empty rather than a field that looks implemented.
            metadata: std::collections::BTreeMap::new(),
        },
        profile: ProfileIdentity {
            id: profile.document().profile.id.clone(),
            version: profile.document().profile.version.clone(),
            digest: digest::profile(&config.profile_root)?.to_string(),
        },
        vectors: CorpusIdentity {
            digest: digest::corpus(selected)?.to_string(),
            count: u32::try_from(results.len()).unwrap_or(u32::MAX),
        },
        configuration: config.as_recorded(),
        results,
        summary,
        status,
        exit_code: status.exit_code().as_i32(),
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::Target;

    #[test]
    fn the_pipeline_module_exposes_the_target_parse_rule_it_is_documented_with() {
        assert!(Target::parse("fixture:none", None).is_ok());
    }
}
