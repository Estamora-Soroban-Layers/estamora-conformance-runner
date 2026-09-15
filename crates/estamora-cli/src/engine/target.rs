//! Resolving and deploying the contract under test.
//!
//! A target is named on the command line and is one of three things: one of the
//! in-repository fixture contracts, a compiled WebAssembly artifact on disk, or a
//! contract on a network. This module turns a name into a deployed contract and an
//! interface to compare against, or refuses with a reason.
//!
//! # A remote contract is read, then measured locally
//!
//! [`Target::Remote`] is resolved over Soroban RPC to the WebAssembly the contract is
//! running, and that artifact is then deployed into the local host and measured exactly
//! as a `.wasm` file on disk is. Nothing about the verdict depends on the network beyond
//! which bytes were fetched: no funded account is needed, no transaction is submitted,
//! and a shared ledger cannot change the answer half way through a corpus.
//!
//! That means a network problem is never a conformance result. An endpoint that cannot
//! be reached, a contract that does not exist, and a code hash that disagrees with the
//! artifact are all [`ErrorClass::ContractResolutionError`] or [`ErrorClass::NetworkError`]
//! blamed on the environment, and the run exits `4` rather than `1`. A CI job retries
//! them or files them as infrastructure, and never as a contract that failed its profile.
//!
//! ## Why the artifact is fetched once per run
//!
//! Every vector declares its own ledger point, so every vector is measured in a fresh
//! host and the contract is re-registered for each one. Re-fetching it over RPC each
//! time would mean a corpus of twenty vectors made twenty round trips for a single
//! artifact — and would let a node that advanced mid-corpus change the artifact half way
//! through its own report. [`Target::fetch`] is therefore called once by each entry
//! point and the result passed down, so one run measures one artifact.
//!
//! # Seeding, and why some vectors are refused rather than failed
//!
//! A vector declares the state its operation starts from: Alice holds 1000. For that
//! to be true of the contract under test, something has to put 1000 there. The
//! standard exposes no way to mint, so a fixture declares setup entry points under a
//! `fixture_` prefix, and a compiled artifact is assumed to have no such thing. A
//! vector whose world cannot be established is reported as skipped rather than as
//! failed: the requirement was not exercised, so the contract was not measured on it,
//! and a run that could not exercise a requirement must not report `CONFORMANT`.

use std::path::{Path, PathBuf};

use estamora_core::{Error, ErrorClass, Result};
use estamora_fixture_token::Defect;
use estamora_soroban::{DeclaredMethod, ExposedInterface, LedgerPoint, LocalHost};
use soroban_sdk::Address;
use stellar_xdr::LedgerEntry;

/// The largest artifact the runner will read into memory.
///
/// A Soroban contract is tens of kilobytes. The bound exists so that a path pointing
/// at something enormous fails with a message instead of by exhausting the machine.
pub const MAX_ARTIFACT_BYTES: u64 = 32 * 1024 * 1024;

/// The prefix that names one of the in-repository fixtures.
pub const FIXTURE_PREFIX: &str = "fixture:";

/// A contract to measure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// One of the in-repository contracts, each wrong in at most one way.
    Fixture {
        /// Which contract.
        defect: Defect,
    },
    /// A compiled contract, deployed into the local host from its bytes.
    Wasm {
        /// Where the artifact is.
        path: PathBuf,
    },
    /// A contract on a network.
    Remote {
        /// The contract's identifier.
        contract_id: String,
        /// The network it is on.
        network: String,
    },
}

/// A contract artifact fetched from a network, with the provenance a report needs.
///
/// The provenance is kept rather than reduced to a hash because the point of a
/// conformance result is that somebody else can reproduce it, and reproducing a remote
/// measurement means knowing which endpoint was read and which protocol version the
/// node was serving at the time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteArtifact {
    /// The contract identifier, as it was given.
    pub contract_id: String,
    /// The network it was read from.
    pub network: String,
    /// The endpoint that served it.
    pub rpc_url: String,
    /// The network's passphrase, as the node reports it.
    pub passphrase: String,
    /// The protocol version the node reports.
    pub protocol_version: u32,
    /// The hash the contract instance declares, verified against `wasm`.
    pub wasm_hash: String,
    /// The deployed WebAssembly.
    pub wasm: Vec<u8>,
    /// The contract's own instance ledger entry, exactly as the network stores it.
    ///
    /// This is what makes a deployed contract measurable. Its `__constructor` took
    /// arguments only its deployer knew, so the contract cannot be re-deployed locally
    /// — but it does not need to be: the instance entry carries the instance storage the
    /// constructor wrote, so the contract can be placed in the local ledger in exactly
    /// the state the network has it in and measured from there.
    pub instance: LedgerEntry,
}

impl Target {
    /// Parses a `--contract` value.
    ///
    /// `network` is the `--network` value, required only for a remote contract.
    ///
    /// # Errors
    ///
    /// Returns a usage error for `fixture:` naming an unknown fixture, for a remote
    /// contract with no network, and for an empty identifier. Returns a contract
    /// resolution error for a `.wasm` path that does not exist.
    pub fn parse(text: &str, network: Option<&str>) -> Result<Self> {
        if let Some(name) = text.strip_prefix(FIXTURE_PREFIX) {
            let defect = Defect::parse(name).ok_or_else(|| {
                let known: Vec<&str> = Defect::ALL.iter().map(|defect| defect.as_str()).collect();
                Error::new(
                    ErrorClass::UsageError,
                    format!(
                        "there is no fixture named {name:?}; the fixtures are {}",
                        known.join(", ")
                    ),
                )
                .with_context("fixture", name.to_owned())
            })?;
            return Ok(Self::Fixture { defect });
        }

        if text.is_empty() {
            return Err(Error::new(
                ErrorClass::UsageError,
                "a contract must be named: a `fixture:<name>`, a path ending in `.wasm`, or a \
                 contract identifier together with `--network`",
            ));
        }

        if Path::new(text)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            == Some("wasm")
        {
            let path = PathBuf::from(text);
            if !path.is_file() {
                return Err(Error::new(
                    ErrorClass::ContractResolutionError,
                    format!(
                        "{} is not a file, so there is no artifact to deploy",
                        path.display()
                    ),
                )
                .with_context("contract", path.display().to_string()));
            }
            return Ok(Self::Wasm { path });
        }

        let Some(network) = network else {
            return Err(Error::new(
                ErrorClass::UsageError,
                format!(
                    "{text:?} is not a `fixture:<name>` or a `.wasm` path, so it is read as a \
                     deployed contract and needs `--network` to say where it lives"
                ),
            ));
        };
        if !looks_like_a_contract_id(text) {
            return Err(Error::new(
                ErrorClass::UsageError,
                format!(
                    "{text:?} is not a contract identifier: a contract identifier is 56 \
                     characters beginning with `C`"
                ),
            )
            .with_context("network", network.to_owned()));
        }
        Ok(Self::Remote {
            contract_id: text.to_owned(),
            network: network.to_owned(),
        })
    }

    /// Fetches what the target needs from outside the repository, once per run.
    ///
    /// [`Target::Fixture`] needs nothing and returns `None`; it is registered from a
    /// Rust type. A remote contract is read over RPC here and nowhere else, so that a
    /// run fetches one artifact rather than one per vector.
    ///
    /// # Errors
    ///
    /// Returns a contract resolution error or a network error when the contract cannot
    /// be read, and a usage error when the `--network` value does not name a network.
    /// None of those is a statement about the contract's behaviour.
    pub fn fetch(&self) -> Result<Option<RemoteArtifact>> {
        self.fetch_via(endpoint_override().as_deref())
    }

    /// [`Self::fetch`], against an explicitly named endpoint.
    ///
    /// The endpoint is a parameter rather than read from the environment here so that a
    /// test can point one network name at a closed port and assert the whole remote path
    /// without depending on a shared ledger being up — and without mutating a
    /// process-wide variable that other tests in the same binary would see.
    ///
    /// # Errors
    ///
    /// As [`Self::fetch`].
    pub fn fetch_via(&self, override_url: Option<&str>) -> Result<Option<RemoteArtifact>> {
        let Self::Remote {
            contract_id,
            network,
        } = self
        else {
            return Ok(None);
        };
        let resolved = estamora_soroban::resolve(contract_id, network, override_url)?;
        Ok(Some(RemoteArtifact {
            contract_id: resolved.contract_id,
            network: resolved.network,
            rpc_url: resolved.rpc_url,
            passphrase: resolved.passphrase,
            protocol_version: resolved.protocol_version,
            wasm_hash: resolved.wasm_hash,
            wasm: resolved.wasm,
            instance: resolved.instance,
        }))
    }

    /// The network the target lives on, as a report records it.
    #[must_use]
    pub fn network(&self) -> &str {
        match self {
            Self::Fixture { .. } | Self::Wasm { .. } => LOCAL_NETWORK,
            Self::Remote { network, .. } => network,
        }
    }

    /// The target as a report names it.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Fixture { defect } => format!("{FIXTURE_PREFIX}{}", defect.as_str()),
            Self::Wasm { path } => path.display().to_string(),
            Self::Remote { contract_id, .. } => contract_id.clone(),
        }
    }

    /// How the target's opening state is established, for the report.
    #[must_use]
    pub const fn seeding_description(&self) -> &'static str {
        match self {
            Self::Fixture { .. } => Seeding::FIXTURE_DESCRIPTION,
            Self::Wasm { .. } | Self::Remote { .. } => Seeding::NO_SETUP_DESCRIPTION,
        }
    }
}

/// The endpoint named by the environment, if any.
///
/// This is how a network operator that is not one of the two this build knows by name is
/// reached, and it is why an unfamiliar endpoint is never guessed at. A blank value is
/// treated as absent rather than as an endpoint of `""`, because an empty variable is how
/// a shell renders an unset one and silently reading that as a URL would be a confusing
/// way to fail.
#[must_use]
fn endpoint_override() -> Option<String> {
    std::env::var(estamora_soroban::RPC_URL_VARIABLE)
        .ok()
        .filter(|url| !url.trim().is_empty())
}

/// The network a locally deployed contract is measured on.
pub const LOCAL_NETWORK: &str = "local";

/// A deployed contract, and what is known about it.
#[derive(Debug)]
pub struct Deployment {
    /// Where the contract was deployed.
    pub contract: Address,
    /// The network, as a report records it.
    pub network: String,
    /// The artifact deployed, where there was one, as its bare 64-character code hash.
    ///
    /// Not a `sha256:`-prefixed digest: this is the hash a network states for a
    /// deployment and the form the report schema requires, and it has to be the same
    /// string whichever route produced it.
    ///
    /// `None` for an in-repository fixture, which is registered from a Rust type
    /// rather than from a file. Reported as `null` rather than omitted so that the
    /// absence is visible to a reader.
    pub wasm_hash: Option<String>,
    /// The interface the contract publishes.
    pub interface: ExposedInterface,
    /// How the vector's opening state can be established.
    pub seeding: Seeding,
}

/// How a vector's declared opening state can be put into the contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seeding {
    /// The contract publishes the fixture setup entry points, so opening balances and
    /// allowances can be established.
    FixtureEntryPoints,
    /// Nothing can establish opening state, with the reason a report gives.
    Unavailable {
        /// Why the state cannot be established.
        reason: String,
    },
}

impl Seeding {
    /// The report's description of fixture seeding.
    pub const FIXTURE_DESCRIPTION: &'static str = "fixture_entry_points";

    /// The report's description of no seeding.
    pub const NO_SETUP_DESCRIPTION: &'static str = "none";

    /// Whether opening state can be established.
    #[must_use]
    pub const fn is_available(&self) -> bool {
        matches!(self, Self::FixtureEntryPoints)
    }

    /// The description a report records.
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::FixtureEntryPoints => Self::FIXTURE_DESCRIPTION,
            Self::Unavailable { .. } => Self::NO_SETUP_DESCRIPTION,
        }
    }

    /// Why seeding is unavailable, where it is.
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::FixtureEntryPoints => None,
            Self::Unavailable { reason } => Some(reason),
        }
    }
}

/// Deploys `target` at `point` and reads its interface.
///
/// `artifact` is what [`Target::fetch`] returned for this target, and is required for a
/// remote target: the fetch is deliberately not repeated here, because this function is
/// called once per vector and a run must measure one artifact rather than whichever one
/// the network was serving at each moment.
///
/// # Why this builds the host rather than being given one
///
/// A local target is deployed *into* a host, but a remote one *is* the host: the contract
/// is placed in a ledger built from its deployed instance entry, and that ledger is what
/// the call is made against. So the two cannot share one caller-supplied host, and the
/// host comes back out with the deployment for the caller to measure in. Nothing is
/// registered in the remote case, which is what makes a contract whose constructor takes
/// arguments measurable instead of merely refused.
///
/// # Errors
///
/// Returns a contract resolution error when an artifact cannot be read, is larger than
/// [`MAX_ARTIFACT_BYTES`], publishes no contract spec section, or cannot be placed in the
/// ledger with the instance entry the network reports; an internal error when a remote
/// target is deployed without the artifact its fetch should have produced. Every one of
/// those is an environment failure rather than anything about the contract's behaviour.
pub fn deploy(
    target: &Target,
    point: LedgerPoint,
    artifact: Option<&RemoteArtifact>,
) -> Result<(LocalHost, Deployment)> {
    match target {
        Target::Fixture { defect } => {
            let host = LocalHost::new(point);
            let declared = estamora_fixture_token::interface::declared(*defect);
            let methods: Vec<DeclaredMethod> = declared
                .iter()
                .map(|method| DeclaredMethod {
                    name: method.name,
                    parameters: method.parameters,
                    returns: method.returns,
                    readonly: method.readonly,
                })
                .collect();
            let contract = estamora_fixture_token::deploy(host.env(), *defect);
            Ok((
                host,
                Deployment {
                    contract,
                    network: LOCAL_NETWORK.to_owned(),
                    // A fixture is registered from a Rust type, so there is no artifact
                    // to hash and no claim to make about one.
                    wasm_hash: None,
                    interface: ExposedInterface::declared(
                        &methods,
                        format!("the fixture `{}`", defect.as_str()),
                    ),
                    seeding: Seeding::FixtureEntryPoints,
                },
            ))
        },
        Target::Wasm { path } => {
            let host = LocalHost::new(point);
            let metadata = std::fs::metadata(path).map_err(|problem| {
                Error::new(
                    ErrorClass::ContractResolutionError,
                    format!("{} could not be read: {problem}", path.display()),
                )
                .with_context("contract", path.display().to_string())
            })?;
            if metadata.len() > MAX_ARTIFACT_BYTES {
                return Err(Error::new(
                    ErrorClass::ContractResolutionError,
                    format!(
                        "{} is {} bytes, above the {MAX_ARTIFACT_BYTES}-byte limit",
                        path.display(),
                        metadata.len()
                    ),
                ));
            }
            let bytes = std::fs::read(path).map_err(|problem| {
                Error::new(
                    ErrorClass::ContractResolutionError,
                    format!("{} could not be read: {problem}", path.display()),
                )
                .with_context("contract", path.display().to_string())
            })?;

            // The interface is read before the deployment, so a contract that
            // publishes no spec section is refused rather than deployed and then
            // asked what it exposes.
            let describe = format!("the artifact {}", path.display());
            let interface = ExposedInterface::from_wasm(&bytes, describe.clone())?;
            // The bare code hash, without a `sha256:` prefix. That is the form the report
            // schema requires of this field and the form a network reports, and the two
            // routes have to agree: the same contract read from disk and read over RPC
            // must hash to the same string, or a report's identity depends on where the
            // bytes were found. The prefixed form is what a *digest* is elsewhere in a
            // report — the profile and corpus digests — which is why this one converts
            // rather than using the `Digest` display.
            let wasm_hash = estamora_certification::Digest::of_bytes(&bytes)
                .hex()
                .to_owned();

            let contract = host.register_artifact(&bytes, &describe)?;
            Ok((
                host,
                Deployment {
                    contract,
                    network: LOCAL_NETWORK.to_owned(),
                    wasm_hash: Some(wasm_hash),
                    interface,
                    seeding: Seeding::Unavailable {
                        reason: "a compiled artifact is not assumed to publish Estamora's fixture \
                                 setup entry points, so a vector whose world declares no opening \
                                 balance or allowance can be prepared and one that declares either \
                                 cannot"
                            .to_owned(),
                    },
                },
            ))
        },
        Target::Remote {
            contract_id,
            network,
        } => {
            // A remote target that arrives here without its artifact is a defect in
            // this runner rather than anything the caller did, and saying so is the
            // only safe answer: the alternative is to fetch it here, which would
            // silently reintroduce one round trip per vector.
            let Some(artifact) = artifact else {
                return Err(Error::new(
                    ErrorClass::InternalError,
                    format!(
                        "contract {contract_id} on `{network}` reached deployment without \
                         having been fetched; the runner must resolve a remote contract \
                         once before it is measured"
                    ),
                )
                .with_context("contract", contract_id.clone())
                .with_context("network", network.clone())
                .with_context("reason", "remote-artifact-not-fetched"));
            };

            // The interface is read before the deployment, for the same reason a
            // local artifact's is: a contract that publishes no spec section is
            // refused rather than deployed and then asked what it exposes.
            let interface = ExposedInterface::from_wasm(
                &artifact.wasm,
                format!(
                    "contract {contract_id} on `{}` as read from {}",
                    artifact.network, artifact.rpc_url
                ),
            )
            .map_err(|problem| {
                // A reader is given the identifier and the network, because the two
                // things a caller can act on are which contract was meant and which
                // endpoint answered — and neither is recoverable from the bytes.
                problem
                    .with_context("contract", contract_id.clone())
                    .with_context("network", artifact.network.clone())
                    .with_context("rpc_url", artifact.rpc_url.clone())
            })?;

            let describe = format!(
                "contract {contract_id} on `{}` as read from {}",
                artifact.network, artifact.rpc_url
            );

            // The contract is placed in the ledger as the network has it, from the
            // instance entry that was read alongside its code, rather than being
            // registered. Registering would run `__constructor`, which a deployed
            // contract's deployer already ran with arguments this runner does not have
            // and must not invent — and the instance entry it wrote is right here, so
            // there is nothing to reproduce. The contract's own configuration — the
            // admin, the decimals, the symbol — is therefore the contract's, not a
            // guess, and the measurement starts from the state the contract is actually
            // running in.
            let host =
                LocalHost::holding(point, &artifact.wasm, Some(&artifact.instance), &describe)
                    .map_err(|problem| {
                        problem
                            .with_context("contract", contract_id.clone())
                            .with_context("network", artifact.network.clone())
                            .with_context("rpc_url", artifact.rpc_url.clone())
                    })?;
            let Some(contract) = host.held().cloned() else {
                return Err(Error::new(
                    ErrorClass::InternalError,
                    format!(
                        "{describe} was placed in the ledger but the host reports holding \
                         nothing, so there is no contract to call"
                    ),
                )
                .with_context("contract", contract_id.clone())
                .with_context("reason", "host-holds-no-contract"));
            };

            Ok((
                host,
                Deployment {
                    contract,
                    network: artifact.network.clone(),
                    wasm_hash: Some(artifact.wasm_hash.clone()),
                    interface,
                    // The opening state a vector declares cannot be established against a
                    // deployed contract: it publishes its own interface, not Estamora's
                    // fixture setup entry points, and writing to somebody else's ledger is
                    // not something a measurement may do.
                    seeding: Seeding::Unavailable {
                        reason: "a deployed contract publishes its own interface rather than \
                                 Estamora's fixture setup entry points, and a measurement does \
                                 not write to the ledger it reads from, so a vector whose world \
                                 declares no opening balance or allowance can be prepared and \
                                 one that declares either cannot"
                            .to_owned(),
                    },
                },
            ))
        },
    }
}

/// Whether `text` has the shape of a contract identifier.
///
/// A shape check rather than a checksum verification, deliberately: this can accept a
/// mistyped identifier and leave it to the transport to reject, which is what happens,
/// and it does so as a usage error naming the identifier. Checking the shape is still
/// worth doing, because the alternative is passing a path or a sentence to the network
/// layer and reporting whatever it says about it as a resolution outcome.
#[must_use]
pub fn looks_like_a_contract_id(text: &str) -> bool {
    const LENGTH: usize = 56;
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    text.len() == LENGTH
        && text.starts_with('C')
        && text.bytes().all(|byte| ALPHABET.contains(&byte))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{RemoteArtifact, Seeding, Target, looks_like_a_contract_id};
    use estamora_core::ErrorClass;
    use estamora_fixture_token::Defect;
    use estamora_soroban::LedgerPoint;
    use stellar_xdr::{
        ContractDataDurability, ContractDataEntry, ContractExecutable, ContractId, ExtensionPoint,
        Hash, LedgerEntry, LedgerEntryData, LedgerEntryExt, ScAddress, ScContractInstance, ScVal,
    };

    /// An artifact standing in for one fetched from a network.
    ///
    /// Deliberately the same bytes as a real contract would be: the deployment path
    /// reads a spec section out of them, so a fixture that merely had the right shape
    /// would not exercise it.
    fn fetched(wasm: &[u8]) -> RemoteArtifact {
        RemoteArtifact {
            contract_id: format!("C{}", "A".repeat(55)),
            network: "testnet".to_owned(),
            rpc_url: "https://soroban-testnet.stellar.org".to_owned(),
            passphrase: "Test SDF Network ; September 2015".to_owned(),
            protocol_version: 28,
            wasm_hash: "0".repeat(64),
            wasm: wasm.to_vec(),
            instance: instance_entry(),
        }
    }

    /// A contract instance entry, in the shape a node reports one.
    ///
    /// Deliberately a real ledger entry rather than a placeholder: the deployment path
    /// stores it in a ledger, so an entry that could not be stored would say nothing
    /// about the path that stores it. It is not the entry of any particular contract —
    /// the tests that use it stop before anything is executed — so it need only be well
    /// formed.
    fn instance_entry() -> LedgerEntry {
        LedgerEntry {
            last_modified_ledger_seq: 1,
            data: LedgerEntryData::ContractData(ContractDataEntry {
                ext: ExtensionPoint::V0,
                contract: ScAddress::Contract(ContractId(Hash([0; 32]))),
                key: ScVal::LedgerKeyContractInstance,
                durability: ContractDataDurability::Persistent,
                val: ScVal::ContractInstance(ScContractInstance {
                    executable: ContractExecutable::Wasm(Hash([0; 32])),
                    storage: None,
                }),
            }),
            ext: LedgerEntryExt::V0,
        }
    }

    #[test]
    fn a_fixture_is_named_and_an_unknown_one_is_refused_with_the_list_of_real_ones() {
        assert_eq!(
            Target::parse("fixture:none", None).unwrap(),
            Target::Fixture {
                defect: Defect::None
            }
        );
        let problem = Target::parse("fixture:not-a-fixture", None).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::UsageError);
        assert!(
            problem.message().contains("skips-authorization"),
            "the refusal must list the fixtures that do exist: {}",
            problem.message()
        );
    }

    #[test]
    fn a_contract_identifier_without_a_network_is_a_usage_error_rather_than_a_target() {
        // Reading it as a target and failing later would report a command-line mistake
        // as a resolution outcome, which CI would then treat as infrastructure.
        let id = format!("C{}", "A".repeat(55));
        let problem = Target::parse(&id, None).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::UsageError);
        assert!(
            problem.message().contains("--network"),
            "{}",
            problem.message()
        );
    }

    #[test]
    fn something_that_is_neither_a_fixture_nor_a_path_is_refused_when_it_is_not_an_identifier() {
        let problem = Target::parse("not a contract", Some("testnet")).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::UsageError);
        assert!(
            problem.message().contains("56 characters"),
            "{}",
            problem.message()
        );
    }

    #[test]
    fn a_missing_artifact_is_a_resolution_failure_and_not_a_usage_error() {
        // The command line was well formed; the thing it named was not there. CI has
        // to be able to tell those apart.
        let problem = Target::parse("/nowhere/contract.wasm", None).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::ContractResolutionError);
    }

    #[test]
    fn a_local_target_fetches_nothing() {
        // A fixture is registered from a Rust type and an artifact is already on disk,
        // so neither may cause a network read.
        assert!(
            Target::Fixture {
                defect: Defect::None
            }
            .fetch()
            .unwrap()
            .is_none()
        );
        assert!(
            Target::Wasm {
                path: "/nowhere/contract.wasm".into()
            }
            .fetch()
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn a_remote_target_that_cannot_be_reached_is_an_environment_failure_with_the_endpoint_named() {
        // The whole remote path, asserted deterministically: a network name this build
        // knows is pointed at a closed port, so the transport is genuinely exercised —
        // parsed, fetched, failed — without depending on a shared ledger being up. What
        // matters is the classification: somebody else's outage must never be reported as
        // a contract that failed its profile.
        //
        // The identifier is a correctly check-summed StrKey rather than merely a
        // well-shaped string, because the shape check is not what this test is about: a
        // fabricated one would be refused as a usage error before the transport was
        // reached and would assert nothing.
        let id = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM";
        let target = Target::parse(id, Some("testnet")).unwrap();
        let problem = target
            .fetch_via(Some("http://127.0.0.1:1"))
            .expect_err("nothing is listening on port 1");

        assert_eq!(problem.class(), ErrorClass::NetworkError);
        assert_eq!(problem.blame(), estamora_core::Blame::Environment);
        assert_eq!(problem.context_value("reason"), Some("network-unreachable"));
        assert_eq!(problem.context_value("rpc_url"), Some("http://127.0.0.1:1"));
        assert_eq!(problem.context_value("contract"), Some(id));
        assert_eq!(
            crate::exit_code(problem.class()),
            estamora_core::ExitCode::EnvironmentFailed,
            "CI must be able to tell an outage from a non-conformant contract"
        );
    }

    #[test]
    fn a_remote_target_without_its_artifact_is_an_internal_error_not_a_verdict() {
        // Deployment is called once per vector, so a remote target reaching it unfetched
        // is a defect in the runner. Fetching one here instead would hide that defect
        // behind a round trip per vector, and would let a node that advanced mid-corpus
        // change the artifact part way through its own report.
        let id = format!("C{}", "A".repeat(55));
        let target = Target::parse(&id, Some("testnet")).unwrap();
        let problem = super::deploy(&target, LedgerPoint::default(), None).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::InternalError);
        assert_eq!(
            problem.blame(),
            estamora_core::Blame::Runner,
            "the runner failed to fetch, so the runner is what went wrong"
        );
        assert_eq!(
            problem.context_value("reason"),
            Some("remote-artifact-not-fetched")
        );
    }

    #[test]
    fn a_remote_target_is_measured_from_the_artifact_it_was_fetched_with() {
        // The decisive property: the bytes the fetch produced are the bytes that get
        // deployed, not merely bytes that are carried alongside. Bytes that are not
        // WebAssembly must therefore be refused as a resolution failure, which they can
        // only be if the artifact actually reached the interface reader.
        let id = format!("C{}", "A".repeat(55));
        let target = Target::parse(&id, Some("testnet")).unwrap();
        let artifact = fetched(b"not wasm");

        let problem = super::deploy(&target, LedgerPoint::default(), Some(&artifact)).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::ContractResolutionError);
        assert_eq!(problem.blame(), estamora_core::Blame::Environment);
        assert_eq!(problem.context_value("contract"), Some(id.as_str()));
        assert_eq!(problem.context_value("network"), Some("testnet"));
        assert_eq!(
            problem.context_value("rpc_url"),
            Some("https://soroban-testnet.stellar.org"),
            "a remote failure must say which endpoint answered"
        );
    }

    #[test]
    fn a_fixture_deploys_with_the_interface_its_own_declaration_states() {
        let (_, deployment) = super::deploy(
            &Target::Fixture {
                defect: Defect::MissingDecimals,
            },
            LedgerPoint::default(),
            None,
        )
        .unwrap();
        assert_eq!(
            deployment.interface.source,
            "the fixture `missing-decimals`"
        );
        assert!(!deployment.interface.declares("decimals"));
        assert!(deployment.interface.declares("transfer"));
        // The setup entry points are exposed and therefore admitted, which is what
        // makes the declaration a description of the contract rather than of a claim
        // about it.
        assert!(deployment.interface.declares("fixture_mint"));
        assert_eq!(deployment.seeding, Seeding::FixtureEntryPoints);
        assert_eq!(deployment.network, "local");
        assert!(deployment.wasm_hash.is_none());
    }

    #[test]
    fn a_fixture_that_admits_its_defect_is_the_only_one_that_does() {
        for defect in Defect::ALL {
            let (_, deployment) =
                super::deploy(&Target::Fixture { defect }, LedgerPoint::default(), None).unwrap();
            assert_eq!(
                deployment.interface.declares("decimals"),
                defect.exposes_decimals(),
                "{defect:?} describes its own interface wrongly"
            );
        }
    }

    #[test]
    fn a_contract_identifier_shape_is_checked_without_claiming_a_checksum() {
        assert!(looks_like_a_contract_id(&format!("C{}", "A".repeat(55))));
        assert!(!looks_like_a_contract_id(&format!("G{}", "A".repeat(55))));
        assert!(!looks_like_a_contract_id("Cshort"));
        // Lowercase and `0`/`1` are outside the base-32 alphabet a StrKey uses.
        assert!(!looks_like_a_contract_id(&format!("C{}", "a".repeat(55))));
        assert!(!looks_like_a_contract_id(&format!("C{}0", "A".repeat(54))));
    }
}
