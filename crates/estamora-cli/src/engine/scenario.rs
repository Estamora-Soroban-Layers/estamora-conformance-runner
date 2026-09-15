//! One vector, executed.
//!
//! This module is the whole of Estamora's answer for a single scenario: it builds the
//! world the vector declares, puts that world into the contract, records it, calls the
//! method under the authorization the vector names, and hands the observation to the
//! evaluator.
//!
//! # Every vector gets its own world
//!
//! A vector declares the state its operation starts from, and a run has many vectors.
//! So the contract is deployed afresh for each one, at the ledger point the vector
//! names. Sharing a host between vectors would let one vector's effects leak into the
//! next, and a result that depends on the order vectors happened to be read in is not
//! a result.
//!
//! # The authorization plan is a property of the scenario
//!
//! A positive vector runs with authorization granted, because the contract is entitled
//! to assume its caller is authorized. A negative one runs with it withheld, because
//! that is the only way to observe whether the check exists at all. And a wrong-actor
//! scenario authorizes *somebody* — just not the principal the profile requires —
//! which is why withholding everything could not express it: the call would be refused
//! before the wrong account could be observed.
//!
//! # Seeding that cannot happen is a skip, not a failure
//!
//! A compiled artifact is not assumed to publish setup entry points, so a vector whose
//! world declares an opening balance cannot be prepared against one. That vector is
//! reported as skipped and the run's verdict becomes undecided: the requirement was
//! not exercised, and a run that did not exercise a requirement must never report
//! `CONFORMANT`. Measuring it against a world it did not declare would be worse still.
//!
//! # A world that cannot be built is an environment failure
//!
//! If the contract refuses a setup call, nothing about its conformance follows — it
//! was never measured on any requirement. The run stops with an environment failure
//! rather than marking every vector in the corpus non-conformant, because the second
//! outcome would blame the contract for the runner's inability to prepare a scenario.

use std::collections::BTreeMap;
use std::vec::Vec;

use estamora_assertions::run::RunObservation;
use estamora_assertions::{OutcomeDiagnostic, VectorOutcome};
use estamora_core::value::Value;
use estamora_core::{Error, ErrorClass, Result};
use estamora_profile::{MethodDefinition, ProfileBundle};
use estamora_soroban::{
    AuthorizationMode, ContractWorld, LedgerPoint, LocalHost, apply_authorizations,
};
use estamora_vectors::{ActorKind, AuthorizationExpectation, VectorDocument};
use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{Address, Env, IntoVal as _, Symbol, Val};

use crate::engine::target::{self, RemoteArtifact, Target};
use crate::engine::{observe, record, values};

/// What executing one vector produced.
#[derive(Debug)]
pub struct Executed {
    /// What the vector concluded.
    pub outcome: VectorOutcome,
    /// The network the contract was measured on.
    pub network: String,
    /// The digest of the artifact, where one was deployed.
    pub wasm_hash: Option<String>,
}

/// Executes one vector against `target`.
///
/// `artifact` is what [`Target::fetch`] produced for `target`, and is required when the
/// target is remote. It is passed in rather than fetched here because this runs once per
/// vector: a corpus of twenty vectors must not make twenty round trips for one artifact,
/// nor let the network change which artifact is being measured part way through a report.
///
/// # Errors
///
/// Returns an error when the run could not be attempted at all: an artifact that could
/// not be deployed, a world that could not be put into the contract, a timestamp that
/// is not a date-time, or a vector that does not agree with the profile it is declared
/// against. A vector that was merely undecidable is a result rather than an error, and
/// is reported inside the returned outcome.
pub fn execute(
    target: &Target,
    artifact: Option<&RemoteArtifact>,
    profile: &ProfileBundle,
    vector: &VectorDocument,
) -> Result<Executed> {
    let point = LedgerPoint::new(
        vector.fixtures.ledger.sequence,
        vector.fixtures.ledger.timestamp_unix()?,
    );
    let host = LocalHost::new(point);
    let env = host.env();

    // Deployment happens before anything is measured, so a contract that cannot be
    // resolved or inspected stops the run instead of producing verdicts about an
    // artifact the runner never read.
    let deployment = target::deploy(target, &host, artifact)?;

    let mut diagnostics: Vec<OutcomeDiagnostic> = Vec::new();
    let mut addresses: BTreeMap<String, Address> = BTreeMap::new();
    for actor in &vector.fixtures.actors {
        addresses.insert(actor.name.clone(), Address::generate(env));
        if actor.kind == ActorKind::Contract {
            // A vector may name a contract actor and the format carries no deployment
            // recipe for one: no artifact, no constructor arguments. It is materialized
            // as a generated address and said so, because silently treating it as an
            // account would misdescribe the scenario the vector describes.
            diagnostics.push(OutcomeDiagnostic::new(
                "fixture.contract-actor-materialized",
                format!(
                    "the fixture actor `{}` is declared as a contract and a vector carries no \
                     artifact to deploy for one, so it was materialized as a generated address",
                    actor.name
                ),
            ));
        }
    }

    let actor_names: Vec<String> = vector
        .fixtures
        .actors
        .iter()
        .map(|actor| actor.name.clone())
        .collect();
    let world = ContractWorld::new(
        env,
        deployment.contract.clone(),
        record::actors(&actor_names, &addresses),
        record::allowances(vector),
        vector.fixtures.ledger.sequence,
    );

    let method = profile.documents().method(&vector.method).ok_or_else(|| {
        Error::new(
            ErrorClass::VectorError,
            format!(
                "the vector `{}` exercises the method `{}`, which the profile does not declare",
                vector.id, vector.method
            ),
        )
    })?;

    let (argument_names, argument_values) = arguments(method, vector)?;

    // Seeding, and the decision that follows from not being able to.
    if declares_opening_state(vector) && !deployment.seeding.is_available() {
        let reason = deployment
            .seeding
            .reason()
            .unwrap_or("there is no way to establish the vector's opening state");
        return Ok(Executed {
            outcome: VectorOutcome::skipped(
                vector.id.clone(),
                vector.kind.as_str(),
                "seeding-unavailable",
                format!(
                    "the vector declares an opening balance or allowance and {reason}; the \
                     requirement was not exercised and this vector contributes nothing to the \
                     verdict"
                ),
            )
            .with_diagnostics(diagnostics),
            network: deployment.network,
            wasm_hash: deployment.wasm_hash,
        });
    }
    if deployment.seeding.is_available() {
        seed(&deployment.contract, vector, &addresses, env)?;
    }

    // The world before the call, captured while it is still true.
    let before = record::capture(profile, vector, &world);

    let host_args = host_arguments(env, &world, method, &argument_values)?;
    apply_authorization(
        vector,
        &addresses,
        &deployment.contract,
        &method.name,
        &host_args,
        env,
    )?;

    let invocation = estamora_soroban::invoke(
        env,
        &deployment.contract,
        &method.name,
        host_args,
        // The interface was read during deployment, before this call, which is what
        // licenses reading a later abort as the contract's own refusal. See
        // `estamora_soroban::invoker` for why nothing weaker would do.
        true,
    )?;

    let call = observe::call(
        &invocation,
        env,
        &world,
        &method.name,
        &argument_names,
        &argument_values,
    );
    let interface = observe::inspection(&deployment.interface);

    let observation = RunObservation {
        profile,
        vector,
        before: &before,
        after: &world,
        call,
        interface,
    };
    // A cross-reference inside the evaluator that does not resolve is a defect in the
    // corpus rather than anything about the contract, so it is reported as an
    // undecided vector carrying the reason rather than as a failed one.
    let outcome = estamora_assertions::evaluate_vector(&observation)
        .unwrap_or_else(|problem| {
            VectorOutcome::could_not_run(
                vector.id.clone(),
                vector.kind.as_str(),
                "evaluation-unavailable",
                problem.to_string(),
            )
        })
        .with_diagnostics(diagnostics);

    Ok(Executed {
        outcome,
        network: deployment.network,
        wasm_hash: deployment.wasm_hash,
    })
}

/// Whether the vector's world declares state that has to be put into the contract.
#[must_use]
pub fn declares_opening_state(vector: &VectorDocument) -> bool {
    vector
        .fixtures
        .balances
        .values()
        .any(|amount| amount.get() != 0)
        || !vector.fixtures.allowances.is_empty()
}

/// The arguments the method is called with, in the order the profile declares them.
///
/// The profile decides the order and the types; the vector decides the values. A
/// vector that does not supply one of the declared arguments is refused rather than
/// called with a placeholder, because a call with a wrong argument measures something
/// other than what the vector describes.
///
/// # Errors
///
/// Returns a vector error when an argument is missing, when an input names an argument
/// the method does not declare, or when a value cannot be read as the declared type.
fn arguments(
    method: &MethodDefinition,
    vector: &VectorDocument,
) -> Result<(Vec<String>, Vec<Value>)> {
    let mut names = Vec::with_capacity(method.args.len());
    let mut arguments = Vec::with_capacity(method.args.len());
    for argument in &method.args {
        let expression = vector.inputs.get(&argument.name).ok_or_else(|| {
            Error::new(
                ErrorClass::VectorError,
                format!(
                    "the profile declares that `{}` takes an argument named `{}`, and the vector \
                     `{}` does not supply it",
                    method.name, argument.name, vector.id
                ),
            )
        })?;
        names.push(argument.name.clone());
        arguments.push(values::argument(
            expression,
            &argument.type_expr,
            &vector.inputs,
        )?);
    }
    for supplied in vector.inputs.keys() {
        if !method
            .args
            .iter()
            .any(|argument| &argument.name == supplied)
        {
            return Err(Error::new(
                ErrorClass::VectorError,
                format!(
                    "the vector `{}` supplies the input `{supplied}`, which `{}` does not declare \
                     as an argument",
                    vector.id, method.name
                ),
            ));
        }
    }
    Ok((names, arguments))
}

/// Converts the resolved arguments into host values.
///
/// # Errors
///
/// Returns an execution failure for a value with no host representation, or for an
/// address that is not one of the fixture's actors. Both are the runner's inability to
/// make the call, so neither may be reported as a contract finding.
fn host_arguments(
    env: &Env,
    world: &ContractWorld<'_>,
    method: &MethodDefinition,
    values: &[Value],
) -> Result<soroban_sdk::Vec<Val>> {
    let mut host_args = soroban_sdk::Vec::new(env);
    for (value, argument) in values.iter().zip(method.args.iter()) {
        host_args.push_back(values::host_argument(
            env,
            world,
            value,
            &argument.type_expr,
        )?);
    }
    Ok(host_args)
}

/// Puts the vector's declared opening state into the contract.
///
/// # Errors
///
/// Returns an execution failure when the contract refuses a setup call, or a vector
/// error when the fixture names an actor that does not exist.
fn seed(
    contract: &Address,
    vector: &VectorDocument,
    addresses: &BTreeMap<String, Address>,
    env: &Env,
) -> Result<()> {
    for (name, amount) in &vector.fixtures.balances {
        let Some(who) = addresses.get(name) else {
            return Err(Error::new(
                ErrorClass::VectorError,
                format!(
                    "the vector `{}` declares a balance for `{name}`, which is not one of its \
                     fixture actors",
                    vector.id
                ),
            ));
        };
        let args: soroban_sdk::Vec<Val> = (who.clone(), amount.get()).into_val(env);
        call_setup(env, contract, "fixture_mint", args, name)?;
    }

    for allowance in &vector.fixtures.allowances {
        let (Some(from), Some(spender)) = (
            addresses.get(&allowance.from),
            addresses.get(&allowance.spender),
        ) else {
            return Err(Error::new(
                ErrorClass::VectorError,
                format!(
                    "the vector `{}` grants an allowance between `{}` and `{}`, and at least one \
                     of them is not one of its fixture actors",
                    vector.id, allowance.from, allowance.spender
                ),
            ));
        };
        // The expiry is passed rather than assumed, because a vector about an expired
        // allowance declares a lapsed ledger and there is no other way to put a lapsed
        // grant into the contract: the interface will not create one. A fixture that
        // declares no expiry gets a far-future one, which is what "the implementation's
        // default" means for a token that has no default of its own.
        let live_until_ledger = allowance.live_until_ledger.unwrap_or(u32::MAX);
        let args: soroban_sdk::Vec<Val> = (
            from.clone(),
            spender.clone(),
            allowance.amount.get(),
            live_until_ledger,
        )
            .into_val(env);
        call_setup(env, contract, "fixture_approve", args, &allowance.from)?;
    }

    Ok(())
}

/// Makes one setup call, classifying a refusal as an environment failure.
fn call_setup(
    env: &Env,
    contract: &Address,
    method: &str,
    args: soroban_sdk::Vec<Val>,
    subject: &str,
) -> Result<()> {
    let symbol = Symbol::new(env, method);
    let _value = env
        .try_invoke_contract::<Val, soroban_sdk::InvokeError>(contract, &symbol, args)
        .map_err(|problem| {
            Error::new(
                ErrorClass::ExecutionError,
                format!(
                    "the contract could not be put into the state the vector declares: the setup \
                     entry point `{method}` failed with {problem:?}"
                ),
            )
            .with_context("setup", method.to_owned())
            .with_context("fixture", subject.to_owned())
        })?
        .map_err(|problem| {
            Error::new(
                ErrorClass::ExecutionError,
                format!(
                    "the contract could not be put into the state the vector declares: the setup \
                     entry point `{method}` returned a value this runner could not read: {problem:?}"
                ),
            )
            .with_context("setup", method.to_owned())
        })?;
    Ok(())
}

/// Applies the vector's authorization plan to the host.
///
/// # Errors
///
/// Returns a vector error when the plan names an actor that is not one of the
/// fixture's.
fn apply_authorization(
    vector: &VectorDocument,
    addresses: &BTreeMap<String, Address>,
    contract: &Address,
    method: &str,
    args: &soroban_sdk::Vec<Val>,
    env: &Env,
) -> Result<()> {
    match vector.authorization.expected {
        // Granted rather than hand-built, because a scenario that must succeed is
        // entitled to supply whatever the contract legitimately demands, including
        // authorizations for contracts it calls in turn. What the contract demanded is
        // still observed afterwards, so nothing about the check is hidden by this.
        AuthorizationExpectation::Accepted => {
            AuthorizationMode::Granted.apply(env);
            Ok(())
        },
        // A read, or an operation no reasonable interface asks anybody to sign for.
        AuthorizationExpectation::NotRequired => {
            AuthorizationMode::Denied.apply(env);
            Ok(())
        },
        AuthorizationExpectation::Rejected => {
            if vector.authorization.actors.is_empty() {
                // Nobody signed. This is the observation a "must require authorization"
                // vector needs.
                AuthorizationMode::Denied.apply(env);
                return Ok(());
            }

            // Somebody signed and the vector expects a refusal anyway: the wrong-actor
            // case. Authorizing exactly the named actors, and nobody else, is what makes
            // "refused because the signature belonged to the wrong principal"
            // distinguishable from "refused because there was no signature". Without
            // that distinction the vector would be satisfied by a contract that simply
            // refuses everything.
            let mut authorizers: Vec<Address> = Vec::new();
            for name in &vector.authorization.actors {
                let Some(address) = addresses.get(name) else {
                    return Err(Error::new(
                        ErrorClass::VectorError,
                        format!(
                            "the vector `{}` names `{name}` as signing the call, and it is not one \
                             of the fixture's actors",
                            vector.id
                        ),
                    ));
                };
                authorizers.push(address.clone());
            }

            let invocation = MockAuthInvoke {
                contract,
                fn_name: method,
                args: args.clone(),
                sub_invokes: &[],
            };
            let auths: Vec<MockAuth<'_>> = authorizers
                .iter()
                .map(|address| MockAuth {
                    address,
                    invoke: &invocation,
                })
                .collect();
            apply_authorizations(env, &auths);
            Ok(())
        },
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use std::collections::BTreeMap;

    use estamora_vectors::{
        AuthorizationExpectation, AuthorizationPlan, EventExpectations, ExpectedOutcome,
        ExpectedResult, FixtureActor, Fixtures, LedgerFixture, VectorDocument, VectorKind,
    };

    use super::declares_opening_state;

    /// A vector whose only interesting property is its fixture's opening state.
    fn vector(balances: &[(&str, &str)], allowances: bool) -> VectorDocument {
        VectorDocument {
            schema: None,
            id: "test-vector".to_owned(),
            profile: "test".to_owned(),
            profile_version: "1.0".to_owned(),
            title: "t".to_owned(),
            description: "d".to_owned(),
            kind: VectorKind::Positive,
            method: "decimals".to_owned(),
            tags: Vec::new(),
            fixtures: Fixtures {
                actors: vec![FixtureActor {
                    name: "alice".to_owned(),
                    kind: estamora_vectors::ActorKind::Account,
                    reference: None,
                    description: None,
                }],
                balances: balances
                    .iter()
                    .map(|(name, amount)| {
                        (
                            (*name).to_owned(),
                            estamora_vectors::IntegerString::new(amount.parse().unwrap()),
                        )
                    })
                    .collect(),
                allowances: if allowances {
                    vec![estamora_vectors::FixtureAllowance {
                        from: "alice".to_owned(),
                        spender: "bob".to_owned(),
                        amount: estamora_vectors::IntegerString::new(600),
                        live_until_ledger: Some(5_000),
                    }]
                } else {
                    Vec::new()
                },
                total_supply: None,
                ledger: LedgerFixture {
                    sequence: 1_000,
                    timestamp: "2026-01-15T12:00:00Z".to_owned(),
                },
                authorization: BTreeMap::new(),
            },
            inputs: BTreeMap::new(),
            authorization: AuthorizationPlan {
                actors: Vec::new(),
                expected: AuthorizationExpectation::NotRequired,
                substituted_for: Vec::new(),
            },
            expected: ExpectedOutcome {
                outcome: ExpectedResult::Success,
                failure: None,
                failure_category: None,
                returns: None,
                state_assertions: Vec::new(),
                events: EventExpectations {
                    required: Vec::new(),
                    forbidden: Vec::new(),
                },
                invariants: Vec::new(),
            },
            assertions: Vec::new(),
            rationale: "r".to_owned(),
            references: Vec::new(),
        }
    }

    #[test]
    fn a_world_that_declares_nothing_is_prepared_without_seeding() {
        // The distinction that decides whether a compiled artifact can be measured at
        // all: a vector declaring no opening state needs nothing put into the contract,
        // so it is executable even where seeding is not available.
        assert!(!declares_opening_state(&vector(&[], false)));
        assert!(!declares_opening_state(&vector(&[("alice", "0")], false)));
    }

    #[test]
    fn a_world_that_declares_an_opening_balance_needs_seeding() {
        assert!(declares_opening_state(&vector(&[("alice", "1")], false)));
    }

    #[test]
    fn an_opening_allowance_alone_is_enough_to_need_seeding() {
        assert!(declares_opening_state(&vector(&[], true)));
    }
}
