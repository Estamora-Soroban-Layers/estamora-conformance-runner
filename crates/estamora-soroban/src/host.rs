//! A deterministic local execution environment.
//!
//! Every fixture a profile declares names a ledger point — a sequence number and
//! a timestamp — because a vector whose outcome depends on the wall clock cannot
//! be reproduced, and a result nobody else can reproduce is not evidence. This
//! module is what turns a declared fixture into a host that honours it.

use estamora_core::{Error, ErrorClass, Result};
use soroban_sdk::testutils::{Ledger as _, Register};
use soroban_sdk::{Address, Env};

use crate::inspect;

/// The ledger sequence a run uses when a fixture does not name one.
///
/// Any fixed value would do; this one is chosen to be large enough that TTL
/// arithmetic in a fixture does not short-circuit, and the choice is recorded so
/// that two runs of the same fixture agree.
pub const DEFAULT_LEDGER_SEQUENCE: u32 = 1_000;

/// The ledger timestamp a run uses when a fixture does not name one:
/// `2026-01-15T12:00:00Z`, matching the instant the specification's own example
/// vectors declare.
pub const DEFAULT_LEDGER_TIMESTAMP: u64 = 1_768_478_400;

/// A point in ledger time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedgerPoint {
    /// The ledger sequence number the fixture runs at.
    pub sequence: u32,
    /// The ledger close time, in seconds since the Unix epoch.
    pub timestamp: u64,
}

impl Default for LedgerPoint {
    fn default() -> Self {
        Self {
            sequence: DEFAULT_LEDGER_SEQUENCE,
            timestamp: DEFAULT_LEDGER_TIMESTAMP,
        }
    }
}

impl LedgerPoint {
    /// A point at `sequence` and `timestamp`.
    #[must_use]
    pub const fn new(sequence: u32, timestamp: u64) -> Self {
        Self {
            sequence,
            timestamp,
        }
    }
}

/// A local execution environment, pinned to one ledger point.
#[derive(Clone)]
pub struct LocalHost {
    env: Env,
    point: LedgerPoint,
}

impl core::fmt::Debug for LocalHost {
    /// Prints the ledger point and the environment's identity, never the host's
    /// internal state.
    ///
    /// `Env` has no usable `Debug`, and deriving one from it would put a
    /// handle's address into every diagnostic — a value that differs between two
    /// runs of the same fixture, which is the opposite of what this runner
    /// produces anywhere else.
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("LocalHost")
            .field("sequence", &self.point.sequence)
            .field("timestamp", &self.point.timestamp)
            .finish_non_exhaustive()
    }
}

impl LocalHost {
    /// Builds a host pinned to `point`.
    ///
    /// The ledger is set before any contract is registered, so a contract whose
    /// constructor reads the ledger sees the fixture's values rather than the
    /// host defaults.
    #[must_use]
    pub fn new(point: LedgerPoint) -> Self {
        let env = Env::default();
        env.ledger().set_sequence_number(point.sequence);
        env.ledger().set_timestamp(point.timestamp);
        Self { env, point }
    }

    /// Builds a host at the default ledger point.
    #[must_use]
    pub fn at_default_point() -> Self {
        Self::new(LedgerPoint::default())
    }

    /// The underlying environment.
    #[must_use]
    pub fn env(&self) -> &Env {
        &self.env
    }

    /// The ledger point this host is pinned to.
    #[must_use]
    pub const fn point(&self) -> LedgerPoint {
        self.point
    }

    /// Registers a contract and returns its address.
    ///
    /// The parameter is generic over the SDK's `Register` trait, which is
    /// implemented both by a contract type defined in a fixture and by the bytes
    /// of a compiled contract. That is deliberate: the same code path deploys an
    /// in-repository fixture and a user's WebAssembly, so a fixture cannot
    /// exercise a route the real deployment does not take.
    pub fn register(&self, contract: impl Register) -> Address {
        contract.register(&self.env, None, ())
    }

    /// Registers a compiled artifact, reporting a constructor that cannot be run.
    ///
    /// # Why this is not `register`
    ///
    /// Registering an artifact runs its `__constructor`, and the environment supplies no
    /// arguments — so an artifact whose constructor declares any fails inside the host, and
    /// the SDK turns that host error into a panic rather than returning it. A panic in the
    /// middle of a run is a defect whatever the cause: it is not a verdict, it carries no
    /// class for CI to act on, and it cannot be told apart from a bug in this runner.
    ///
    /// [`constructor_needing_arguments`] is checked first, because that is the case a
    /// deployed contract actually hits and the one worth naming precisely. This catches the
    /// rest — a constructor that fails for some other reason, such as one that requires
    /// authorization nobody can grant on a fresh instance.
    ///
    /// # Errors
    ///
    /// Returns a contract resolution error when the artifact cannot be instantiated. That
    /// is an environment failure: nothing about how the contract behaves has been
    /// observed, so nothing about it may be concluded.
    pub fn register_artifact(&self, wasm: &[u8], describe: &str) -> Result<Address> {
        let interface = inspect::ExposedInterface::from_wasm(wasm, describe)?;
        if let Some(constructor) = constructor_needing_arguments(&interface) {
            return Err(Error::new(
                ErrorClass::ContractResolutionError,
                format!(
                    "{describe} declares a constructor taking {constructor}, and a constructor can \
                     only be run by whoever deployed the contract: only the deployer knew \
                     what to pass it. The runner will not invent arguments, because doing so \
                     would fabricate the very state the vectors are then measured against. \
                     No instance can be created locally, so nothing about this contract's \
                     behaviour was observed"
                ),
            )
            .with_context("reason", "constructor-needs-arguments")
            .with_context("constructor", "__constructor"));
        }

        // The artifact is user-supplied and the host executes it during registration, so
        // this is a boundary: a panic here must become a classified failure rather than
        // take the process down. `AssertUnwindSafe` is honest rather than convenient — the
        // host may be left inconsistent — because the run is abandoned either way, and the
        // alternative is a crash that reads as a defect in this runner.
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.register(wasm))) {
            Ok(address) => Ok(address),
            Err(_) => Err(Error::new(
                ErrorClass::ContractResolutionError,
                format!(
                    "{describe} could not be instantiated: the host failed while running its \
                     constructor. Nothing about this contract's behaviour was observed, so \
                     no verdict was reached"
                ),
            )
            .with_context("reason", "instantiation-failed")),
        }
    }
}

/// The name the SDK gives a contract's constructor in its spec section.
pub const CONSTRUCTOR: &str = "__constructor";

/// The parameter list of an artifact's constructor, when registering it would need one.
///
/// `None` when the artifact declares no constructor at all, or declares one that takes no
/// arguments — both of which the host can register without being told anything the runner
/// does not know.
#[must_use]
pub fn constructor_needing_arguments(interface: &inspect::ExposedInterface) -> Option<String> {
    let constructor = interface.method(CONSTRUCTOR)?;
    if constructor.parameters.is_empty() {
        return None;
    }
    Some(
        constructor
            .parameters
            .iter()
            .map(|parameter| match &parameter.name {
                Some(name) => {
                    let type_name = &parameter.type_name;
                    format!("{name}: {type_name}")
                },
                None => parameter.type_name.clone(),
            })
            .collect::<Vec<String>>()
            .join(", "),
    )
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{LocalHost, constructor_needing_arguments};
    use crate::inspect::{ExposedInterface, ExposedMethod, ExposedParameter};
    use estamora_core::ErrorClass;

    fn interface(methods: Vec<ExposedMethod>) -> ExposedInterface {
        ExposedInterface {
            methods,
            source: "a test".to_owned(),
        }
    }

    fn method(name: &str, parameters: &[(&str, &str)]) -> ExposedMethod {
        ExposedMethod {
            name: name.to_owned(),
            parameters: parameters
                .iter()
                .map(|(name, type_name)| ExposedParameter {
                    name: Some((*name).to_owned()),
                    type_name: (*type_name).to_owned(),
                })
                .collect(),
            returns: None,
            readonly: false,
        }
    }

    #[test]
    fn an_artifact_with_no_constructor_needs_nothing() {
        let declared = interface(vec![method("transfer", &[("from", "address")])]);
        assert_eq!(constructor_needing_arguments(&declared), None);
    }

    #[test]
    fn an_artifact_whose_constructor_takes_nothing_is_registrable() {
        let declared = interface(vec![method(super::CONSTRUCTOR, &[])]);
        assert_eq!(constructor_needing_arguments(&declared), None);
    }

    #[test]
    fn a_constructor_that_takes_arguments_is_reported_with_them() {
        // The refusal has to name what it cannot supply, because "this contract cannot be
        // measured" is not actionable and "this contract's constructor wants an admin
        // address" is.
        let declared = interface(vec![method(
            super::CONSTRUCTOR,
            &[("admin", "address"), ("decimal", "u32")],
        )]);
        assert_eq!(
            constructor_needing_arguments(&declared).as_deref(),
            Some("admin: address, decimal: u32")
        );
    }

    #[test]
    fn a_constructor_is_detected_however_many_methods_surround_it() {
        let declared = interface(vec![
            method("decimals", &[]),
            method(super::CONSTRUCTOR, &[("admin", "address")]),
            method("transfer", &[("from", "address")]),
        ]);
        assert!(
            constructor_needing_arguments(&declared).is_some(),
            "a constructor is not made registrable by having neighbours"
        );
    }

    #[test]
    fn an_artifact_with_a_constructor_that_needs_arguments_is_refused_rather_than_crashing() {
        // The defect this guards: registering such an artifact panics inside the host, so
        // a run died with a stack trace instead of a class CI could act on. Bytes that are
        // not WebAssembly cannot reach the check, so this asserts the check is reached by
        // the interface reader first and reported as a resolution failure.
        let host = LocalHost::at_default_point();
        let problem = host
            .register_artifact(b"not wasm", "a test artifact")
            .expect_err("bytes that are not WebAssembly cannot be instantiated");
        assert_eq!(problem.class(), ErrorClass::ContractResolutionError);
        assert_eq!(problem.blame(), estamora_core::Blame::Environment);
    }
}
