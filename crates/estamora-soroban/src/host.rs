//! A deterministic local execution environment.
//!
//! Every fixture a profile declares names a ledger point — a sequence number and
//! a timestamp — because a vector whose outcome depends on the wall clock cannot
//! be reproduced, and a result nobody else can reproduce is not evidence. This
//! module is what turns a declared fixture into a host that honours it.

use estamora_core::{Error, ErrorClass, Result};
use sha2::{Digest as _, Sha256};
use soroban_sdk::testutils::{Ledger as _, Register};
use soroban_sdk::{Address, Env};
use stellar_xdr::{
    BytesM, ContractCodeEntry, ContractCodeEntryExt, ContractDataDurability, ContractDataEntry,
    ContractExecutable, ContractId, ExtensionPoint, Hash, LedgerEntry, LedgerEntryData,
    LedgerEntryExt, LedgerKey, LedgerKeyContractCode, LedgerKeyContractData, ScAddress,
    ScContractInstance, ScVal,
};

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
    /// The contract the host was built holding, when it was built from a compiled
    /// artifact rather than by registering one.
    held: Option<Address>,
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
        Self {
            env,
            point,
            held: None,
        }
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

    /// Builds a host in which `wasm` already exists as a contract.
    ///
    /// # Why a compiled artifact is not registered
    ///
    /// Registering an artifact runs its `__constructor`, and the environment has no
    /// arguments to give it. A deployed contract's constructor took the arguments only its
    /// deployer knew, so registration fails for exactly the contracts this runner exists to
    /// measure, and the SDK turns that host error into a panic.
    ///
    /// So the code and the instance are placed in the ledger and the host is built from
    /// that ledger instead. Nothing runs the constructor, and the contract exists as the
    /// network has it: `instance` is the deployed instance entry when the artifact came
    /// from a network, so its instance storage — the admin, the decimals, the symbol — is
    /// the contract's own configuration rather than something invented here. A local
    /// artifact has no such entry, and gets an empty one: it is then in its
    /// pre-constructor state, which is a fact about the run rather than a defect, and the
    /// vectors that need more are reported as not exercised.
    ///
    /// # Errors
    ///
    /// Returns a contract resolution error when the artifact does not hash to the code
    /// hash the instance entry declares, or when its bytes cannot be encoded as a ledger
    /// entry. Both mean the ledger would be inconsistent with itself, and a run measured
    /// against an inconsistent ledger says nothing about any contract.
    pub fn holding(
        point: LedgerPoint,
        wasm: &[u8],
        instance: Option<&LedgerEntry>,
        describe: &str,
    ) -> Result<Self> {
        let code_hash = Hash(Sha256::digest(wasm).into());

        // The two ledger entries must agree, or the host would load code under a hash the
        // instance does not name and report a missing contract for a healthy artifact.
        if let Some(declared) = instance.and_then(crate::client::executable_hash)
            && declared != code_hash
        {
            return Err(Error::new(
                ErrorClass::ContractResolutionError,
                format!(
                    "{describe} declares the code hash {} but the artifact hashes to 
                     {}. The ledger would be inconsistent with itself, so nothing was 
                     measured",
                    hex::encode(declared.0),
                    hex::encode(code_hash.0)
                ),
            )
            .with_context("reason", "artifact-hash-mismatch"));
        }

        let contract_id = instance
            .and_then(instance_contract)
            .unwrap_or_else(|| ContractId(code_hash.clone()));
        let address = ScAddress::Contract(contract_id.clone());

        let instance_entry = match instance {
            Some(entry) => entry.clone(),
            None => LedgerEntry {
                last_modified_ledger_seq: point.sequence,
                data: LedgerEntryData::ContractData(ContractDataEntry {
                    ext: ExtensionPoint::V0,
                    contract: address.clone(),
                    key: ScVal::LedgerKeyContractInstance,
                    durability: ContractDataDurability::Persistent,
                    val: ScVal::ContractInstance(ScContractInstance {
                        executable: ContractExecutable::Wasm(code_hash.clone()),
                        storage: None,
                    }),
                }),
                ext: LedgerEntryExt::V0,
            },
        };

        let code_entry = LedgerEntry {
            last_modified_ledger_seq: point.sequence,
            data: LedgerEntryData::ContractCode(ContractCodeEntry {
                ext: ContractCodeEntryExt::V0,
                hash: code_hash.clone(),
                code: BytesM::try_from(wasm.to_vec()).map_err(|problem| {
                    Error::new(
                        ErrorClass::ContractResolutionError,
                        format!("{describe} cannot be stored as a ledger entry: {problem}"),
                    )
                })?,
            }),
            ext: LedgerEntryExt::V0,
        };

        // The ledger the contract is measured at comes from a default host rather than
        // from constants written here, so the protocol version and the TTL bounds are the
        // ones the execution environment itself is built against. Guessing them would mean
        // a contract rejected for a protocol feature it does support, or an entry declared
        // expired the moment it was written.
        let mut snapshot = Env::default().to_ledger_snapshot();
        let sequence = point.sequence;
        snapshot.sequence_number = sequence;
        snapshot.timestamp = point.timestamp;
        let live_until = sequence.saturating_add(snapshot.max_entry_ttl);
        snapshot.ledger_entries = vec![
            (
                Box::new(LedgerKey::ContractCode(LedgerKeyContractCode {
                    hash: code_hash.clone(),
                })),
                (Box::new(code_entry), Some(live_until)),
            ),
            (
                Box::new(LedgerKey::ContractData(LedgerKeyContractData {
                    contract: address.clone(),
                    key: ScVal::LedgerKeyContractInstance,
                    durability: ContractDataDurability::Persistent,
                })),
                (Box::new(instance_entry), Some(live_until)),
            ),
        ];

        let env = Env::from_ledger_snapshot(snapshot);
        // The snapshot carries the ledger point, and setting it again keeps a contract
        // that reads `ledger()` from seeing a different answer than the snapshot's.
        env.ledger().set_sequence_number(sequence);
        env.ledger().set_timestamp(point.timestamp);

        // The handle the rest of the runner passes to `invoke_contract`, named from the
        // address the entries were stored under so that a contract with its own contract
        // id is reached at that id rather than at a generated one.
        let held = Address::from_str(&env, &stellar_strkey::Contract(contract_id.0.0).to_string());

        Ok(Self {
            env,
            point,
            held: Some(held),
        })
    }

    /// The contract this host was built holding, when it was built from an artifact.
    #[must_use]
    pub fn held(&self) -> Option<&Address> {
        self.held.as_ref()
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
    /// This is the path a `.wasm` file on disk takes. A contract read from a network does
    /// not take it: its instance entry is available, so [`Self::holding`] places it in the
    /// ledger instead and its constructor is never run.
    ///
    /// # Why this is not `register`
    ///
    /// Registering an artifact runs its `__constructor`, and the environment supplies no
    /// arguments — so an artifact whose constructor declares any fails inside the host, and
    /// the SDK turns that host error into a panic rather than returning it. A panic in the
    /// middle of a run is a defect whatever the cause: it is not a verdict, it carries no
    /// class for CI to act on, and it cannot be told apart from a bug in this runner.
    ///
    /// [`constructor_needing_arguments`] is checked first, because a constructor that
    /// declares arguments is the common case and the one worth naming precisely, including
    /// what it wanted. This catches the rest — a constructor that fails for some other
    /// reason, such as one that requires authorization nobody can grant on a fresh
    /// instance.
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

/// The contract an instance entry belongs to, when it addresses one.
fn instance_contract(entry: &LedgerEntry) -> Option<ContractId> {
    let LedgerEntryData::ContractData(data) = &entry.data else {
        return None;
    };
    let ScAddress::Contract(id) = &data.contract else {
        return None;
    };
    Some(id.clone())
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
    use super::{LedgerPoint, LocalHost, constructor_needing_arguments};
    use crate::inspect::{ExposedInterface, ExposedMethod, ExposedParameter};
    use estamora_core::ErrorClass;
    use sha2::{Digest as _, Sha256};
    use soroban_sdk::Address;
    use stellar_xdr::{
        ContractDataDurability, ContractDataEntry, ContractExecutable, ContractId, ExtensionPoint,
        Hash, LedgerEntry, LedgerEntryData, LedgerEntryExt, ScAddress, ScContractInstance, ScVal,
    };

    /// The code hash an artifact hashes to, as the ledger records it.
    fn code_hash(wasm: &[u8]) -> Hash {
        Hash(Sha256::digest(wasm).into())
    }

    /// A contract instance entry naming `hash`, as a node reports one.
    fn instance(contract: Hash, executable: Hash) -> LedgerEntry {
        LedgerEntry {
            last_modified_ledger_seq: 1,
            data: LedgerEntryData::ContractData(ContractDataEntry {
                ext: ExtensionPoint::V0,
                contract: ScAddress::Contract(ContractId(contract)),
                key: ScVal::LedgerKeyContractInstance,
                durability: ContractDataDurability::Persistent,
                val: ScVal::ContractInstance(ScContractInstance {
                    executable: ContractExecutable::Wasm(executable),
                    storage: None,
                }),
            }),
            ext: LedgerEntryExt::V0,
        }
    }

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

    #[test]
    fn a_contract_read_from_a_network_is_held_at_the_id_it_is_deployed_at() {
        // The property the whole remote path rests on: the contract is placed in the
        // ledger, under the identifier the network serves it at, without anything running
        // its constructor. If the host reported a generated address instead, every call
        // in the corpus would be made against a contract that does not exist — and the
        // diagnostics for that look exactly like a contract with no methods.
        let wasm = b"bytes standing in for a deployed artifact";
        let deployed_at = Hash([7; 32]);
        let host = LocalHost::holding(
            LedgerPoint::default(),
            wasm,
            Some(&instance(deployed_at.clone(), code_hash(wasm))),
            "a test contract",
        )
        .expect("a well-formed instance entry is enough to hold a contract");

        let as_the_network_names_it = Address::from_str(
            host.env(),
            &stellar_strkey::Contract(deployed_at.0).to_string(),
        );
        assert_eq!(
            host.held().cloned(),
            Some(as_the_network_names_it),
            "the contract must be reachable at the id the network reports, not a generated one"
        );
    }

    #[test]
    fn a_local_artifact_with_no_instance_entry_is_held_at_its_own_code_hash() {
        // A `.wasm` file carries no instance entry, so the address is derived from the
        // artifact. Deriving it rather than generating one keeps a run reproducible: two
        // runs of the same file hold the same contract at the same address.
        let wasm = b"bytes standing in for a local artifact";
        let first = LocalHost::holding(LedgerPoint::default(), wasm, None, "a test artifact")
            .expect("an artifact with no instance entry is held from its own bytes");
        let second = LocalHost::holding(LedgerPoint::default(), wasm, None, "a test artifact")
            .expect("an artifact with no instance entry is held from its own bytes");

        assert!(first.held().is_some());
        assert_eq!(first.held(), second.held());
    }

    #[test]
    fn an_instance_entry_that_names_a_different_artifact_is_refused() {
        // The instance and the code must agree. If they did not, the host would load code
        // under a hash the instance does not name, and a healthy artifact would be
        // reported as a missing contract — an environment fault blamed on the contract.
        let wasm = b"the artifact that was fetched";
        let problem = LocalHost::holding(
            LedgerPoint::default(),
            wasm,
            Some(&instance(Hash([7; 32]), Hash([9; 32]))),
            "a test contract",
        )
        .expect_err("an inconsistent ledger must be refused");

        assert_eq!(problem.class(), ErrorClass::ContractResolutionError);
        assert_eq!(problem.blame(), estamora_core::Blame::Environment);
        assert_eq!(
            problem.context_value("reason"),
            Some("artifact-hash-mismatch")
        );
    }

    #[test]
    fn a_held_contract_keeps_the_ledger_point_it_was_built_at() {
        // Vectors declare the ledger they run at, and a contract that reads the ledger
        // must see the vector's values rather than the host's defaults. Building the host
        // from a snapshot is the one path where that could silently be lost.
        let point = LedgerPoint::new(42_000, 1_700_000_000);
        let wasm = b"bytes standing in for an artifact";
        let host = LocalHost::holding(point, wasm, None, "a test artifact")
            .expect("an artifact is held at the point it is built at");

        assert_eq!(host.point(), point);
        assert_eq!(host.env().ledger().sequence(), point.sequence);
        assert_eq!(host.env().ledger().timestamp(), point.timestamp);
    }
}
