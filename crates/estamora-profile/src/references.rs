//! Cross-reference validation.
//!
//! Every defect worth catching in a profile is a reference that does not resolve.
//! A behavioural rule names a failure that no longer exists; an authorization rule
//! covers an argument the method does not take; an event correlates into an
//! invariant that was renamed. Each of these is a requirement that was written,
//! reviewed, and then silently stopped applying — which is the worst outcome for a
//! normative document, because the profile still *looks* complete and the runner
//! still reports a clean result.
//!
//! # Why the runner re-checks what the specification already checks
//!
//! The specification repository has its own validator, and it is authoritative: a
//! defect here is a defect there. But the runner consumes a bundle by path, and a
//! bundle reached by path may not be the one that was validated — it may be a
//! working copy, a hand-edited fixture, or a profile produced by a future tool.
//! Re-checking is defence in depth against executing an unvalidated document, and
//! the cost is bounded by the size of a profile.
//!
//! # What this does not decide
//!
//! Only *resolvability*. Whether a requirement is correct, complete, or faithful
//! to the upstream standard is a review question for the specification repository.
//! A profile that says something wrong but refers consistently to itself passes
//! here, exactly as it should.

use std::collections::BTreeSet;

use estamora_core::{Diagnostic, Diagnostics, ErrorClass};

use crate::authorization::{AuthorizationActor, AuthorizationOutcome, CoverageMode};
use crate::behavior::BehaviorKind;
use crate::documents::ProfileDocuments;
use crate::failures::{ErrorCodePolicy, FailureSignal};
use crate::invariants::InvariantKind;
use crate::types::RequirementStatus;

/// The highest topic index the format permits.
///
/// Bounded by Soroban's limit on contract event topics. Restated here so that a
/// bundle arriving by path is checked against it, not only against the
/// specification repository's copy of the rule.
const MAX_TOPIC_INDEX: u32 = 7;

/// Reports every unresolvable reference in the six documents.
#[must_use]
pub fn validate(documents: &ProfileDocuments) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();

    let method_ids = declared(
        documents
            .methods
            .methods
            .iter()
            .map(|method| method.id.as_str()),
        "the method document",
        &mut diagnostics,
    );
    let rule_ids = declared(
        documents
            .authorization
            .authorization_rules
            .iter()
            .map(|rule| rule.id.as_str()),
        "the authorization document",
        &mut diagnostics,
    );
    let event_ids = declared(
        documents
            .events
            .events
            .iter()
            .map(|event| event.id.as_str()),
        "the event document",
        &mut diagnostics,
    );
    let behavior_ids = declared(
        documents
            .behavior
            .behaviors
            .iter()
            .map(|rule| rule.id.as_str()),
        "the behaviour document",
        &mut diagnostics,
    );
    let invariant_ids = declared(
        documents
            .invariants
            .invariants
            .iter()
            .map(|invariant| invariant.id.as_str()),
        "the invariant document",
        &mut diagnostics,
    );
    let failure_ids = declared(
        documents
            .failures
            .failures
            .iter()
            .map(|failure| failure.id.as_str()),
        "the failure document",
        &mut diagnostics,
    );

    check_methods(
        documents,
        &rule_ids,
        &event_ids,
        &failure_ids,
        &behavior_ids,
        &mut diagnostics,
    );
    check_authorization(documents, &method_ids, &failure_ids, &mut diagnostics);
    check_events(documents, &event_ids, &invariant_ids, &mut diagnostics);
    check_behaviors(
        documents,
        &method_ids,
        &failure_ids,
        &event_ids,
        &invariant_ids,
        &mut diagnostics,
    );
    check_invariants(documents, &method_ids, &mut diagnostics);
    check_failures(documents, &method_ids, &mut diagnostics);

    diagnostics
}

/// Records an error-severity finding.
fn fail(diagnostics: &mut Diagnostics, message: String, context: [(&str, &str); 2]) {
    diagnostics.push(
        Diagnostic::error(ErrorClass::ProfileError, message)
            .with_context(context[0].0, context[0].1)
            .with_context(context[1].0, context[1].1),
    );
}

/// Records a warning-severity finding.
fn warn(diagnostics: &mut Diagnostics, message: String) {
    diagnostics.push(Diagnostic::warning(ErrorClass::ProfileError, message));
}

/// Collects a document's identifiers, reporting any it declares twice.
///
/// Duplicates are fatal rather than resolved by first-wins, because every
/// reference to the identifier is then ambiguous: a rule that expected the first
/// definition and got the second is a requirement that quietly changed meaning.
fn declared<'a>(
    ids: impl Iterator<Item = &'a str>,
    document: &str,
    diagnostics: &mut Diagnostics,
) -> BTreeSet<&'a str> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            fail(
                diagnostics,
                format!(
                    "{document} declares the id {id:?} more than once; every reference to it is ambiguous"
                ),
                [("document", document), ("id", id)],
            );
        }
    }
    seen
}

/// Reports a reference to an identifier no document declares.
fn require_known(
    diagnostics: &mut Diagnostics,
    known: &BTreeSet<&str>,
    id: &str,
    what: &str,
    owner: &str,
) {
    if !known.contains(id) {
        fail(
            diagnostics,
            format!("{owner} refers to {what} {id:?}, which the profile does not declare"),
            [("owner", owner), ("reference", id)],
        );
    }
}

/// Checks the method document's own consistency and its outgoing references.
fn check_methods(
    documents: &ProfileDocuments,
    rule_ids: &BTreeSet<&str>,
    event_ids: &BTreeSet<&str>,
    failure_ids: &BTreeSet<&str>,
    behavior_ids: &BTreeSet<&str>,
    diagnostics: &mut Diagnostics,
) {
    for method in &documents.methods.methods {
        let owner = format!("the method {:?}", method.id);

        // Two arguments with one name make every argument name ambiguous, which
        // both the interface check and a vector's input map depend on resolving.
        let mut argument_names = BTreeSet::new();
        for argument in &method.args {
            if !argument_names.insert(argument.name.as_str()) {
                fail(
                    diagnostics,
                    format!(
                        "{owner} declares the argument {:?} more than once",
                        argument.name
                    ),
                    [
                        ("method", method.id.as_str()),
                        ("argument", argument.name.as_str()),
                    ],
                );
            }
        }

        for rule in &method.authorization {
            require_known(diagnostics, rule_ids, rule, "authorization rule", &owner);
        }
        for event in &method.events {
            require_known(diagnostics, event_ids, event, "event requirement", &owner);
        }
        for failure in &method.failures {
            require_known(
                diagnostics,
                failure_ids,
                failure,
                "failure requirement",
                &owner,
            );
        }
        for behavior in &method.behaviors {
            require_known(
                diagnostics,
                behavior_ids,
                behavior,
                "behavioural rule",
                &owner,
            );
        }

        // A required method that no behavioural rule exercises is a requirement
        // the profile cannot test. Recorded rather than ignored, because it is
        // exactly the shape of a profile that appears to cover more of an
        // interface than it does.
        if method.requirement == RequirementStatus::Required
            && method.behaviors.is_empty()
            && documents.behaviors_for(&method.id).is_empty()
        {
            warn(
                diagnostics,
                format!(
                    "{owner} is required but no behavioural rule governs it, so its behaviour is \
                     never exercised"
                ),
            );
        }
    }
}

/// Checks the authorization document.
fn check_authorization(
    documents: &ProfileDocuments,
    method_ids: &BTreeSet<&str>,
    failure_ids: &BTreeSet<&str>,
    diagnostics: &mut Diagnostics,
) {
    for rule in &documents.authorization.authorization_rules {
        let owner = format!("the authorization rule {:?}", rule.id);

        for method in &rule.methods {
            require_known(diagnostics, method_ids, method, "method", &owner);
        }

        // The covered set is what the requirement actually constrains, so an
        // `exact` rule with nothing to cover would require the contract to demand
        // no authorization at all while appearing to require some.
        if rule.coverage.mode == CoverageMode::Exact && rule.coverage.arguments.is_empty() {
            fail(
                diagnostics,
                format!(
                    "{owner} requires exact argument coverage but lists no arguments, which would \
                     require the contract to demand no authorization"
                ),
                [("rule", rule.id.as_str()), ("mode", "exact")],
            );
        }

        // An argument-named principal that none of the governed methods takes is a
        // requirement that can never be observed.
        if let AuthorizationActor::Argument { argument, .. } = &rule.actor {
            let declared = documents.methods.methods.iter().any(|method| {
                rule.methods.contains(&method.id)
                    && method.args.iter().any(|arg| &arg.name == argument)
            });
            if !declared {
                fail(
                    diagnostics,
                    format!(
                        "{owner} requires {argument:?} to authorize, but no method it governs \
                         declares an argument with that name"
                    ),
                    [("rule", rule.id.as_str()), ("argument", argument.as_str())],
                );
            }
        }

        for (path, outcome) in [
            ("unauthorized", &rule.unauthorized),
            ("wrong_actor", &rule.wrong_actor),
        ] {
            if let AuthorizationOutcome::Fail { failure } = outcome {
                require_known(
                    diagnostics,
                    failure_ids,
                    failure,
                    "failure requirement",
                    &format!("{owner}'s {path} path"),
                );
            }
        }
    }
}

/// Checks the event document.
fn check_events(
    documents: &ProfileDocuments,
    event_ids: &BTreeSet<&str>,
    invariant_ids: &BTreeSet<&str>,
    diagnostics: &mut Diagnostics,
) {
    for event in &documents.events.events {
        let owner = format!("the event requirement {:?}", event.id);

        // Two topics claiming one position make the event's shape ambiguous, and
        // the shape is what a consumer matches on.
        let mut indexes = BTreeSet::new();
        for topic in &event.topics {
            if !indexes.insert(topic.index) {
                fail(
                    diagnostics,
                    format!(
                        "{owner} declares topic index {} more than once",
                        topic.index
                    ),
                    [("event", event.id.as_str()), ("index", "duplicate")],
                );
            }
            if topic.index > MAX_TOPIC_INDEX {
                fail(
                    diagnostics,
                    format!(
                        "{owner} declares topic index {}, above the format's limit of \
                         {MAX_TOPIC_INDEX}",
                        topic.index
                    ),
                    [("event", event.id.as_str()), ("index", "out of range")],
                );
            }
        }

        // A minimum above its maximum can never be satisfied, so the requirement
        // would fail every contract, including a conforming one.
        if event.cardinality.min > event.cardinality.max {
            fail(
                diagnostics,
                format!(
                    "{owner} requires at least {} emissions but permits at most {}, which no \
                     contract can satisfy",
                    event.cardinality.min, event.cardinality.max
                ),
                [("event", event.id.as_str()), ("field", "cardinality")],
            );
        }

        if event.requirement == RequirementStatus::Forbidden && event.cardinality.min > 0 {
            fail(
                diagnostics,
                format!(
                    "{owner} is forbidden and requires at least {} emissions, which is a \
                     contradiction",
                    event.cardinality.min
                ),
                [("event", event.id.as_str()), ("field", "requirement")],
            );
        }

        for ordering in &event.ordering {
            require_known(
                diagnostics,
                event_ids,
                &ordering.before,
                "event requirement",
                &owner,
            );
            if ordering.before == event.id {
                fail(
                    diagnostics,
                    format!("{owner} orders itself before itself"),
                    [("event", event.id.as_str()), ("field", "ordering")],
                );
            }
        }

        for correlation in &event.correlations {
            require_known(diagnostics, invariant_ids, correlation, "invariant", &owner);
        }

        // The first topic is what names an event, so a required event with no
        // topics cannot be matched against anything: the requirement would be
        // present in the document and unenforceable in practice.
        if event.requirement == RequirementStatus::Required && event.topics.is_empty() {
            warn(
                diagnostics,
                format!(
                    "{owner} is required but declares no topics, so it cannot be matched by name"
                ),
            );
        }
    }
}

/// Checks the behaviour document.
fn check_behaviors(
    documents: &ProfileDocuments,
    method_ids: &BTreeSet<&str>,
    failure_ids: &BTreeSet<&str>,
    event_ids: &BTreeSet<&str>,
    invariant_ids: &BTreeSet<&str>,
    diagnostics: &mut Diagnostics,
) {
    for rule in &documents.behavior.behaviors {
        let owner = format!("the behavioural rule {:?}", rule.id);

        require_known(diagnostics, method_ids, &rule.method, "method", &owner);

        for failure in &rule.expect_failures {
            require_known(
                diagnostics,
                failure_ids,
                failure,
                "failure requirement",
                &owner,
            );
        }
        for invariant in &rule.invariants {
            require_known(diagnostics, invariant_ids, invariant, "invariant", &owner);
        }
        for event in rule.expect_events.iter().chain(rule.forbid_events.iter()) {
            require_known(diagnostics, event_ids, event, "event requirement", &owner);
        }

        // An event cannot be both required and forbidden by one rule. Allowing
        // both would let a rule that contradicts itself read as a strict rule
        // rather than as a defect.
        for event in &rule.expect_events {
            if rule.forbid_events.contains(event) {
                fail(
                    diagnostics,
                    format!("{owner} requires the event {event:?} and forbids it in the same rule"),
                    [("behavior", rule.id.as_str()), ("event", event.as_str())],
                );
            }
        }

        // A failure rule that names no failure is satisfied by any error at all,
        // including an unrelated one, so it would never catch the defect it exists
        // for. A success rule that expects a failure describes a path the runner
        // cannot construct.
        match rule.kind {
            BehaviorKind::Failure if rule.expect_failures.is_empty() => fail(
                diagnostics,
                format!(
                    "{owner} describes a path that must fail but names no failure, so any failure \
                     including an unrelated one would satisfy it"
                ),
                [("behavior", rule.id.as_str()), ("kind", "failure")],
            ),
            BehaviorKind::Success if !rule.expect_failures.is_empty() => fail(
                diagnostics,
                format!("{owner} describes a path that must succeed but expects a failure"),
                [("behavior", rule.id.as_str()), ("kind", "success")],
            ),
            BehaviorKind::Success | BehaviorKind::Failure => {},
        }
    }
}

/// Checks the invariant document.
fn check_invariants(
    documents: &ProfileDocuments,
    method_ids: &BTreeSet<&str>,
    diagnostics: &mut Diagnostics,
) {
    for invariant in &documents.invariants.invariants {
        let owner = format!("the invariant {:?}", invariant.id);
        let named = [
            ("invariant", invariant.id.as_str()),
            ("kind", invariant.kind.as_str()),
        ];

        for method in &invariant.scope.methods {
            if method == "*" {
                continue;
            }
            require_known(diagnostics, method_ids, method, "method", &owner);
        }

        // The relationship between a check family and the data it needs is
        // restated rather than trusted, because a mismatch is not a stylistic
        // problem: an aggregate with no resource set has nothing to aggregate, and
        // an invariant the runner cannot evaluate is one it would report as
        // satisfied by accident.
        let ranges = invariant.kind.ranges_over_resource();
        match (&invariant.resource, ranges) {
            (None, true) => fail(
                diagnostics,
                format!(
                    "{owner} is a {} invariant but names no resource set to range over",
                    invariant.kind.as_str()
                ),
                named,
            ),
            (Some(_), false) => fail(
                diagnostics,
                format!(
                    "{owner} names a resource set but a {} invariant does not range over one",
                    invariant.kind.as_str()
                ),
                named,
            ),
            (None | Some(_), _) => {},
        }

        match (&invariant.direction, invariant.kind) {
            (None, InvariantKind::Monotonic) => fail(
                diagnostics,
                format!("{owner} is monotonic but states no direction"),
                named,
            ),
            (Some(_), kind) if kind != InvariantKind::Monotonic => fail(
                diagnostics,
                format!(
                    "{owner} states a direction but is a {} invariant, which is not a directional \
                     claim",
                    kind.as_str()
                ),
                named,
            ),
            (None | Some(_), _) => {},
        }

        let needs_predicate = matches!(
            invariant.kind,
            InvariantKind::Predicate | InvariantKind::Bounds
        );
        match (&invariant.predicate, needs_predicate) {
            (None, true) => fail(
                diagnostics,
                format!(
                    "{owner} is a {} invariant but supplies no predicate",
                    invariant.kind.as_str()
                ),
                named,
            ),
            (Some(_), false) => fail(
                diagnostics,
                format!(
                    "{owner} supplies a predicate but is a {} invariant, which is evaluated by its \
                     own rule",
                    invariant.kind.as_str()
                ),
                named,
            ),
            (None | Some(_), _) => {},
        }
    }
}

/// Checks the failure document.
fn check_failures(
    documents: &ProfileDocuments,
    method_ids: &BTreeSet<&str>,
    diagnostics: &mut Diagnostics,
) {
    for failure in &documents.failures.failures {
        let owner = format!("the failure requirement {:?}", failure.id);

        for method in &failure.methods {
            require_known(diagnostics, method_ids, method, "method", &owner);
        }

        // The policy and the list have to agree, or the list is dead text in one
        // direction and a silently relaxed requirement in the other.
        match failure.error_codes.policy {
            ErrorCodePolicy::SemanticOnly if !failure.error_codes.allowed.is_empty() => fail(
                diagnostics,
                format!(
                    "{owner} uses the semantic_only policy but lists {} tolerated payload(s), which \
                     would never be consulted",
                    failure.error_codes.allowed.len()
                ),
                [("failure", failure.id.as_str()), ("field", "error_codes")],
            ),
            ErrorCodePolicy::ExactRequired if failure.error_codes.allowed.len() != 1 => fail(
                diagnostics,
                format!(
                    "{owner} requires an exact payload but lists {} of them; exactly one is needed \
                     to name it",
                    failure.error_codes.allowed.len()
                ),
                [("failure", failure.id.as_str()), ("field", "error_codes")],
            ),
            ErrorCodePolicy::SemanticOnly
            | ErrorCodePolicy::Tolerated
            | ErrorCodePolicy::ExactRequired => {},
        }

        // An authorization rejection is produced by the contract's own check, so a
        // host-level error would mean the failure happened for a different reason
        // than the profile claims. Restating the rule here is what makes it hold
        // for a bundle that did not come through the specification's validator.
        if failure.category.is_authorization()
            && failure.expected.signal == FailureSignal::HostError
        {
            fail(
                diagnostics,
                format!(
                    "{owner} classifies an authorization failure as host_error, but an \
                     authorization rejection must come from the contract's own check"
                ),
                [
                    ("failure", failure.id.as_str()),
                    ("category", "authorization"),
                ],
            );
        }
    }
}
