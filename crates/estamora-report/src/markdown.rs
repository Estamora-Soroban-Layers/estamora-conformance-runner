//! Rendering a report for a person.
//!
//! A Markdown report is what a reviewer reads, what a pull request comment quotes,
//! and what a project links to as evidence. It is not what another tool consumes —
//! the JSON document is — so this rendering is allowed to choose what is worth
//! showing, and it is required to say what the result does **not** establish.
//!
//! # Contract-supplied content is untrusted here
//!
//! A contract's metadata is attacker-controlled: it arrives from a deployment the
//! runner does not control. In JSON that is harmless, because a consumer parses
//! fields. In Markdown it is not, because a value containing a newline, a backtick
//! or a table pipe can restructure the document around it — a report that says a
//! contract conforms, with text the contract itself supplied inside the sentence
//! that says so. Every interpolated value therefore passes through [`sanitise`],
//! and the sanitising is tested rather than assumed.

use estamora_core::Result;

use crate::model::{Report, VectorResult};
use crate::render::push;

/// Renders a report as Markdown.
///
/// # Errors
///
/// Returns a report error if the document cannot be written. Rendering is in memory,
/// so the only way it fails is a defect in the renderer, which is reported rather
/// than panicked on.
pub fn render(report: &Report) -> Result<String> {
    let mut out = String::new();
    out.push_str("# Estamora conformance report\n\n");
    out.push_str(&verdict(report));
    out.push('\n');

    out.push_str("## What was measured\n\n");
    out.push_str("| | |\n| --- | --- |\n");
    push(
        &mut out,
        format_args!("| Contract | `{}` |\n", sanitise(&report.target.contract)),
    );
    push(
        &mut out,
        format_args!("| Network | {} |\n", sanitise(&report.target.network)),
    );
    push(
        &mut out,
        format_args!(
            "| WASM hash | {} |\n",
            match &report.target.wasm_hash {
                Some(hash) => format!("`{}`", sanitise(hash)),
                None => "not resolved".to_owned(),
            }
        ),
    );
    push(
        &mut out,
        format_args!(
            "| Profile | `{}@{}` |\n",
            sanitise(&report.profile.id),
            sanitise(&report.profile.version)
        ),
    );
    push(
        &mut out,
        format_args!("| Vectors executed | {} |\n", report.vectors.count),
    );
    push(
        &mut out,
        format_args!(
            "| Runner | {} {} |\n",
            sanitise(&report.runner.name),
            sanitise(&report.runner.version)
        ),
    );
    push(
        &mut out,
        format_args!("| Generated | {} |\n", sanitise(&report.generated_at)),
    );
    out.push('\n');

    out.push_str("## Checks by dimension\n\n");
    out.push_str("| Dimension | Passed | Failed | Total | Warnings |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for (category, tally) in report.summary.all() {
        push(
            &mut out,
            format_args!(
                "| {} | {} | {} | {} | {} |\n",
                category.as_str(),
                tally.passed,
                tally.failed,
                tally.total,
                tally.warnings
            ),
        );
    }
    out.push('\n');

    let failed: Vec<&VectorResult> = report
        .results
        .iter()
        .filter(|result| result.status != "passed")
        .collect();

    if failed.is_empty() {
        out.push_str("## Results\n\nEvery vector the profile required passed.\n\n");
    } else {
        out.push_str("## Results\n\n");
        push(
            &mut out,
            format_args!(
                "{} of {} vector results are not a pass.\n\n",
                failed.len(),
                report.results.len()
            ),
        );
        for result in failed {
            out.push_str(&one_vector(result));
        }
    }

    out.push_str(&not_a_security_guarantee());
    Ok(out)
}

/// The headline verdict.
fn verdict(report: &Report) -> String {
    let lead = match report.status {
        estamora_core::ConformanceStatus::Conformant => {
            "The contract satisfied every requirement the profile marks as required."
        },
        estamora_core::ConformanceStatus::PartiallyConformant => {
            "The contract satisfied some required vectors and violated others."
        },
        estamora_core::ConformanceStatus::NonConformant => {
            "The contract violated required behavioural requirements."
        },
        estamora_core::ConformanceStatus::Inconclusive => {
            "The suite could not reach a decision: some required vectors could not be \
             executed, and the unexecuted ones may contain a failure."
        },
        estamora_core::ConformanceStatus::ExecutionError => {
            "The environment failed, so no verdict about the contract was reached. The \
             contract is not implicated."
        },
        estamora_core::ConformanceStatus::ProfileError => {
            "The requirements were unusable, so they were never applied to the contract."
        },
    };
    format!(
        "**{}** — {lead}\n\nProcess exit code `{}`.\n",
        report.status.as_str(),
        report.exit_code
    )
}

/// One vector's failing checks.
fn one_vector(result: &VectorResult) -> String {
    let mut out = String::new();
    push(
        &mut out,
        format_args!(
            "### `{}` — {}\n\n",
            sanitise(&result.vector_id),
            sanitise(&result.status)
        ),
    );
    push(
        &mut out,
        format_args!("Category: {}\n\n", sanitise(&result.category)),
    );

    let failures: Vec<&crate::model::ReportedAssertion> = result
        .assertions
        .iter()
        .filter(|assertion| assertion.status == "failed")
        .collect();

    if failures.is_empty() {
        out.push_str("No individual check failed; the vector could not be decided.\n\n");
    } else {
        out.push_str("| Check | Dimension | Expected | Observed |\n");
        out.push_str("| --- | --- | --- | --- |\n");
        for assertion in failures {
            push(
                &mut out,
                format_args!(
                    "| `{}` | {} | {} | {} |\n",
                    sanitise(&assertion.id),
                    sanitise(&assertion.category),
                    sanitise(&assertion.expected),
                    sanitise(&assertion.observed)
                ),
            );
        }
        out.push('\n');
    }

    if !result.diagnostics.is_empty() {
        out.push_str("Findings:\n\n");
        for finding in &result.diagnostics {
            push(
                &mut out,
                format_args!(
                    "- `{}`: {}\n",
                    sanitise(&finding.code),
                    sanitise(&finding.message)
                ),
            );
        }
        out.push('\n');
    }

    out
}

/// The limitation a reader must not have to infer.
fn not_a_security_guarantee() -> String {
    [
        "## What this report does not establish",
        "",
        "Estamora verifies defined behavioural compatibility with a profile. It is not a security audit,",
        "and it does not certify that a contract is safe.",
        "",
        "- Passing conformance vectors does not prove the absence of vulnerabilities.",
        "- Conformance is scoped to the requirements the profile states; a property no",
        "  profile requires is not measured, whatever its importance.",
        "- A profile is a document with an author. Its provenance and version matter, and",
        "  a profile that is not trusted must not be treated as a standard.",
        "- Estamora does not replace formal verification, an independent audit,",
        "  penetration testing, economic analysis or vulnerability research.",
        "",
        "Estamora answers one question: whether a contract's actual behaviour conforms to a",
        "defined profile. Nothing in this document should be read as a stronger claim.",
        "",
    ]
    .join("\n")
}

/// Makes a value safe to place inside a Markdown table cell or a line of prose.
///
/// Everything that could restructure the document is replaced rather than escaped
/// into a different character: a value that is quoted through a transformation still
/// reads as if the document were speaking, and the whole point is that it must not.
#[must_use]
pub fn sanitise(value: &str) -> String {
    const LIMIT: usize = 200;
    let mut cleaned = String::with_capacity(value.len());
    let mut previous_was_space = false;
    for character in value.chars() {
        let replacement = match character {
            // A newline would start a new block, letting a value author headings,
            // tables or claims in the report's own voice.
            '\n' | '\r' | '\u{2028}' | '\u{2029}' => Some(' '),
            // A pipe would add a column; a backtick would open code.
            // A pipe would add a table column; a backtick would open code. The
            // rest are Markdown's own structural characters, neutered rather than
            // removed so that the value still reads as what it says.
            '|' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '\\' => Some('\''),
            control if control.is_control() => Some(' '),
            other => Some(other),
        };
        let Some(character) = replacement else {
            continue;
        };
        if character == ' ' {
            if previous_was_space {
                continue;
            }
            previous_was_space = true;
        } else {
            previous_was_space = false;
        }
        cleaned.push(character);
    }
    cleaned.trim().chars().take(LIMIT).collect()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::sanitise;

    #[test]
    fn a_newline_cannot_start_a_new_markdown_block() {
        let hostile = "tokens\n\n## CONFORMANT\n\nEverything passed.";
        let safe = sanitise(hostile);
        assert!(!safe.contains('\n'), "{safe:?}");
        assert!(!safe.contains('#'), "{safe:?}");
    }

    #[test]
    fn a_pipe_cannot_add_a_table_column() {
        let hostile = "value | injected | column";
        let safe = sanitise(hostile);
        assert!(!safe.contains('|'), "{safe:?}");
    }

    #[test]
    fn a_backtick_cannot_open_a_code_span() {
        assert!(!sanitise("`code`").contains('`'));
    }

    #[test]
    fn sanitising_is_bounded_and_idempotent() {
        let long = "a".repeat(500);
        let once = sanitise(&long);
        assert_eq!(once.chars().count(), 200);
        assert_eq!(sanitise(&once), once);
    }

    #[test]
    fn ordinary_text_survives_unchanged() {
        assert_eq!(sanitise("a normal observation"), "a normal observation");
        assert_eq!(sanitise("sha256:abc"), "sha256:abc");
    }
}
