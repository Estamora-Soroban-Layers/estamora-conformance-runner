//! Evaluating the vector the specification repository actually publishes.
//!
//! The unit tests in each dimension module pin the rule each one applies. This file
//! pins something different and harder: that the seven dimensions, driven by the
//! *real* SEP-41 profile and a *real* vector, reach the verdicts a reader of those
//! documents would expect. A fixture written to satisfy an implementation proves
//! that the implementation agrees with itself; only the normative documents can
//! disagree with it.
//!
//! It needs a checkout of `estamora-conformance-spec`, so it is ignored by default
//! and runs from CI, where both repositories are present.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use estamora_assertions::dimensions::interface::{ObservedMethod, ObservedParameter};
use estamora_assertions::{
    CallResult, InterfaceInspection, ObservedAuthorization, ObservedCall, ObservedEvent,
    RunObservation, Value, World, evaluate_vector,
};
use estamora_core::{AssertionStatus, Error, ErrorClass, Result, VectorStatus};
use estamora_profile::ProfileBundle;
use estamora_vectors::VectorDocument;

/// The checkout of the specification repository, when there is one.
fn spec_root() -> Option<PathBuf> {
    let root = std::env::var_os("ESTAMORA_SPEC_REPO")?;
    let root = PathBuf::from(root);
    root.is_dir().then_some(root)
}

/// A world whose reads are answered from a table.
///
/// Addresses are answered in the vector's own vocabulary — fixture actor names —
/// because that is what the `World` contract requires and what makes a failing
/// requirement name `alice` rather than fifty-six characters of base-32.
struct Scripted {
    balances: BTreeMap<String, i128>,
    allowances: BTreeMap<(String, String), i128>,
    supply: i128,
    ledger: u32,
}

impl Scripted {
    /// The world the SEP-41 transfer vector's fixture describes, in its opening
    /// state.
    fn opening() -> Self {
        Self {
            balances: BTreeMap::from([("alice".to_owned(), 1_000), ("bob".to_owned(), 500)]),
            allowances: BTreeMap::new(),
            supply: 1_500,
            ledger: 1_000,
        }
    }

    /// The same world after a conforming transfer of 250 from alice to bob.
    fn after_conforming_transfer() -> Self {
        let mut world = Self::opening();
        world.balances.insert("alice".to_owned(), 750);
        world.balances.insert("bob".to_owned(), 750);
        world
    }
}

impl World for Scripted {
    fn read(&self, method: &str, args: &[Value]) -> Result<Value> {
        match (method, args) {
            ("balance", [Value::Address(actor)]) => Ok(Value::Integer(
                self.balances.get(actor).copied().unwrap_or(0),
            )),
            ("allowance", [Value::Address(from), Value::Address(spender)]) => Ok(Value::Integer(
                self.allowances
                    .get(&(from.clone(), spender.clone()))
                    .copied()
                    .unwrap_or(0),
            )),
            ("total_supply", []) => Ok(Value::Integer(self.supply)),
            _ => Err(Error::new(
                ErrorClass::ExecutionError,
                format!("the scripted world has no read for `{method}`"),
            )),
        }
    }

    fn resource_members(&self, resource: &str) -> Result<Vec<Value>> {
        match resource {
            "balances" => Ok(self
                .balances
                .values()
                .map(|balance| Value::Integer(*balance))
                .collect()),
            "allowances" => Ok(self
                .allowances
                .values()
                .map(|allowance| Value::Integer(*allowance))
                .collect()),
            other => Err(Error::new(
                ErrorClass::ExecutionError,
                format!("the scripted world has no resource set `{other}`"),
            )),
        }
    }

    fn ledger_sequence(&self) -> u32 {
        self.ledger
    }

    fn allowance_expiry(&self, _from: &str, _spender: &str) -> Option<u32> {
        None
    }
}

/// An interface declaring every method the profile does, canonically typed.
fn conforming_interface(bundle: &ProfileBundle) -> InterfaceInspection {
    let methods = bundle
        .methods()
        .methods
        .iter()
        .map(|method| ObservedMethod {
            name: method.name.clone(),
            parameters: method
                .args
                .iter()
                .map(|argument| ObservedParameter {
                    name: Some(argument.name.clone()),
                    type_name: estamora_assertions::canonical_type(&argument.type_expr),
                })
                .collect(),
            returns: Some(estamora_assertions::canonical_type(
                &method.returns.type_expr,
            )),
            readonly: method.mutability == estamora_profile::types::Mutability::Readonly,
        })
        .collect();
    InterfaceInspection::inspected(methods, "the profile's own method manifest")
}

/// A call that returned, with the authorization and event a conforming transfer
/// produces.
fn conforming_call() -> ObservedCall {
    let mut call = ObservedCall::returned("transfer", None, true);
    call.authorizations = vec![ObservedAuthorization {
        actor: "alice".to_owned(),
        covers: vec!["from".to_owned()],
    }];
    call.events = vec![ObservedEvent {
        name: Some("transfer".to_owned()),
        topics: vec![
            Value::Text("transfer".to_owned()),
            Value::Address("alice".to_owned()),
            Value::Address("bob".to_owned()),
        ],
        data: Value::Sequence(vec![Value::Integer(250)]),
    }];
    call
}

/// The vector and profile the specification repository publishes.
fn sep_41() -> Option<(ProfileBundle, VectorDocument)> {
    let root = spec_root()?;
    let bundle = ProfileBundle::load(root.join("profiles/sep-41/1.0")).unwrap();
    let text = std::fs::read_to_string(
        root.join("profiles/sep-41/1.0/vectors/transfer/transfer-moves-exact-amount.yaml"),
    )
    .unwrap();
    let vector: VectorDocument = serde_yaml_ng::from_str(&text).unwrap();
    Some((bundle, vector))
}

#[test]
#[ignore = "needs a checkout of estamora-conformance-spec; set ESTAMORA_SPEC_REPO and pass --ignored"]
fn a_conforming_transfer_satisfies_every_dimension() {
    let Some((bundle, vector)) = sep_41() else {
        return;
    };
    let before = Scripted::opening();
    let after = Scripted::after_conforming_transfer();

    let outcome = evaluate_vector(&RunObservation {
        profile: &bundle,
        vector: &vector,
        before: &before,
        after: &after,
        call: conforming_call(),
        interface: conforming_interface(&bundle),
    })
    .unwrap();

    assert_eq!(
        outcome.status,
        VectorStatus::Passed,
        "a conforming transfer must satisfy the vector; failures: {:#?}",
        outcome.failures()
    );

    // Each dimension a success vector exercises has to have actually reported
    // something. A dimension that silently reported nothing would make a passing
    // vector look identical to a vector that was never evaluated.
    for dimension in estamora_vectors::AssertionCategory::ALL {
        let (passed, total) = outcome.tally(dimension);
        assert_eq!(
            passed,
            total,
            "every {} check must have held",
            dimension.as_str()
        );
        if dimension == estamora_vectors::AssertionCategory::Failure {
            // A vector that expects success makes no failure claim, and reporting
            // one would be a claim the scenario never tested.
            assert_eq!(total, 0, "a success vector must make no failure check");
            continue;
        }
        assert!(
            total > 0,
            "the {} dimension reported no checks at all",
            dimension.as_str()
        );
    }
}

#[test]
#[ignore = "needs a checkout of estamora-conformance-spec; set ESTAMORA_SPEC_REPO and pass --ignored"]
fn a_transfer_that_moves_the_wrong_amount_fails_the_state_dimension() {
    let Some((bundle, vector)) = sep_41() else {
        return;
    };
    let before = Scripted::opening();
    // Credit a third account. The pairwise balances a naive check reads are still
    // plausible, and the conservation invariant over the whole set is what notices.
    let mut after = Scripted::after_conforming_transfer();
    after.balances.insert("mallory".to_owned(), 250);

    let outcome = evaluate_vector(&RunObservation {
        profile: &bundle,
        vector: &vector,
        before: &before,
        after: &after,
        call: conforming_call(),
        interface: conforming_interface(&bundle),
    })
    .unwrap();

    assert_eq!(outcome.status, VectorStatus::Failed);
    let failed: Vec<&str> = outcome
        .failures()
        .iter()
        .map(|assertion| assertion.id.as_str())
        .collect();
    assert!(
        failed
            .iter()
            .any(|id| id.starts_with("invariant/conservation")),
        "the whole-set conservation invariant must be the check that notices an \
         unannounced third account; failures: {failed:?}"
    );
}

#[test]
#[ignore = "needs a checkout of estamora-conformance-spec; set ESTAMORA_SPEC_REPO and pass --ignored"]
fn a_transfer_that_emits_nothing_fails_the_event_dimension() {
    let Some((bundle, vector)) = sep_41() else {
        return;
    };
    let before = Scripted::opening();
    let after = Scripted::after_conforming_transfer();
    let mut call = conforming_call();
    call.events.clear();

    let outcome = evaluate_vector(&RunObservation {
        profile: &bundle,
        vector: &vector,
        before: &before,
        after: &after,
        call,
        interface: conforming_interface(&bundle),
    })
    .unwrap();

    assert_eq!(outcome.status, VectorStatus::Failed);
    assert!(
        outcome
            .failures()
            .iter()
            .any(|assertion| assertion.id.starts_with("event/cardinality")),
        "a missing event must be reported as a cardinality failure"
    );
}

#[test]
#[ignore = "needs a checkout of estamora-conformance-spec; set ESTAMORA_SPEC_REPO and pass --ignored"]
fn a_signature_the_contract_never_demanded_fails_the_authorization_dimension() {
    let Some((bundle, vector)) = sep_41() else {
        return;
    };
    let before = Scripted::opening();
    let after = Scripted::after_conforming_transfer();
    let mut call = conforming_call();
    // The contract moved alice's value without binding the signature to the `from`
    // argument. The naive test — "an unauthorized transfer fails" — passes; the
    // coverage requirement is what catches it.
    call.authorizations = vec![ObservedAuthorization {
        actor: "alice".to_owned(),
        covers: Vec::new(),
    }];

    let outcome = evaluate_vector(&RunObservation {
        profile: &bundle,
        vector: &vector,
        before: &before,
        after: &after,
        call,
        interface: conforming_interface(&bundle),
    })
    .unwrap();

    assert_eq!(outcome.status, VectorStatus::Failed);
    assert!(
        outcome
            .failures()
            .iter()
            .any(|assertion| assertion.id.starts_with("authorization/coverage")),
        "the contract's argument coverage is what must be measured"
    );
}

#[test]
#[ignore = "needs a checkout of estamora-conformance-spec; set ESTAMORA_SPEC_REPO and pass --ignored"]
fn a_refusal_without_interface_inspection_establishes_nothing() {
    let Some((bundle, _)) = sep_41() else {
        return;
    };
    let root = spec_root().unwrap();
    let text = std::fs::read_to_string(
        root.join("vectors/common/authorization/transfer-without-signature-fails.yaml"),
    )
    .unwrap();
    let negative: VectorDocument = serde_yaml_ng::from_str(&text).unwrap();

    let before = Scripted::opening();
    let after = Scripted::opening();

    // The contract aborted. That is indistinguishable from a call to a method that
    // does not exist, and an interface that could not be read must therefore make
    // the requirement unestablished rather than satisfied.
    let unverified = evaluate_vector(&RunObservation {
        profile: &bundle,
        vector: &negative,
        before: &before,
        after: &after,
        call: ObservedCall {
            method: "transfer".to_owned(),
            outcome: CallResult::Refused { code: None },
            returned: None,
            events: Vec::new(),
            authorizations: Vec::new(),
            interface_verified: false,
        },
        interface: InterfaceInspection::Unavailable {
            detail: "no interface artifact was available".to_owned(),
        },
    })
    .unwrap();

    // Undecidable, not failed: the contract is not at fault when the runner could
    // not see it, and `error` contributes nothing to a verdict.
    assert_eq!(
        unverified.status,
        VectorStatus::Error,
        "an uninspectable interface must make the vector undecidable rather than failed"
    );
    assert!(
        unverified
            .diagnostics
            .iter()
            .any(|finding| finding.code == "interface-unavailable"),
        "the reason must be recorded: {:#?}",
        unverified.diagnostics
    );

    // With an interface that lacks the method, the same abort is still not a
    // contract decision: the interface dimension reports the missing method and the
    // failure dimension refuses to count the abort as a refusal.
    let mut incomplete = conforming_interface(&bundle);
    if let InterfaceInspection::Inspected { methods, .. } = &mut incomplete {
        methods.retain(|method| method.name != "transfer");
    }
    let outcome = evaluate_vector(&RunObservation {
        profile: &bundle,
        vector: &negative,
        before: &before,
        after: &after,
        call: ObservedCall {
            method: "transfer".to_owned(),
            outcome: CallResult::Refused { code: None },
            returned: None,
            events: Vec::new(),
            authorizations: Vec::new(),
            interface_verified: true,
        },
        interface: incomplete,
    })
    .unwrap();

    assert_eq!(
        outcome.status,
        VectorStatus::Failed,
        "a missing method must not be read as a correct refusal"
    );
    assert!(
        outcome
            .assertions
            .iter()
            .any(|assertion| assertion.id.starts_with("interface/method/")
                && assertion.status == AssertionStatus::Failed),
        "the missing method must itself be reported"
    );
    assert!(
        outcome
            .failures()
            .iter()
            .any(|assertion| assertion.id.starts_with("failure/outcome")),
        "the refusal must be reported as unestablished"
    );
}

#[test]
#[ignore = "needs a checkout of estamora-conformance-spec; set ESTAMORA_SPEC_REPO and pass --ignored"]
fn an_unexpected_success_on_a_negative_vector_is_a_failure_and_not_an_error() {
    let Some((bundle, _)) = sep_41() else {
        return;
    };
    let root = spec_root().unwrap();
    let text = std::fs::read_to_string(
        root.join("vectors/common/authorization/transfer-without-signature-fails.yaml"),
    )
    .unwrap();
    let negative: VectorDocument = serde_yaml_ng::from_str(&text).unwrap();

    let before = Scripted::opening();
    let after = Scripted::after_conforming_transfer();

    let outcome = evaluate_vector(&RunObservation {
        profile: &bundle,
        vector: &negative,
        before: &before,
        after: &after,
        call: conforming_call(),
        interface: conforming_interface(&bundle),
    })
    .unwrap();

    assert_eq!(
        outcome.status,
        VectorStatus::Failed,
        "a contract that accepted an unauthorized transfer is non-conformant, not \
         undecidable"
    );
    assert!(
        outcome
            .failures()
            .iter()
            .any(|assertion| assertion.id.starts_with("failure/outcome")),
        "the unexpected success must be reported by the failure dimension"
    );
}
