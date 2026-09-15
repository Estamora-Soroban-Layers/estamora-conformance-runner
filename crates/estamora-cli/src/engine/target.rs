//! Resolving and deploying the contract under test.
//!
//! A target is named on the command line and is one of three things: one of the
//! in-repository fixture contracts, a compiled WebAssembly artifact on disk, or a
//! contract on a network. This module turns a name into a deployed contract and an
//! interface to compare against, or refuses with a reason.
//!
//! # The network path is a boundary, not a stub
//!
//! Reading a contract's state and sending a transaction to a network needs a Soroban
//! RPC transport. This build does not link one, and the alternative to saying so
//! would be to accept a contract identifier, do nothing with it, and report a
//! verdict — which is the one outcome this project must never produce, because a
//! fabricated `CONFORMANT` is indistinguishable from a real one to everyone who did
//! not run it.
//!
//! So [`Target::Remote`] resolves to a [`ErrorClass::ContractResolutionError`]
//! naming the identifier, the network, and precisely what is missing. The failure is
//! classified as an *environment* failure and exits `4`, never `1`: CI retries it or
//! reports it as an infrastructure problem, and never as a contract that failed its
//! profile. That is the honest rendering of "the runner cannot measure this", and it
//! is the same treatment an unreachable endpoint gets on a build that *does* have a
//! transport — an outcome the pipeline is therefore already exercised against.
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
use estamora_soroban::{DeclaredMethod, ExposedInterface, LocalHost};
use soroban_sdk::Address;

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

/// The network a locally deployed contract is measured on.
pub const LOCAL_NETWORK: &str = "local";

/// A deployed contract, and what is known about it.
#[derive(Debug)]
pub struct Deployment {
    /// Where the contract was deployed.
    pub contract: Address,
    /// The network, as a report records it.
    pub network: String,
    /// The digest of the artifact deployed, where there was one.
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

/// Deploys `target` into `host` and reads its interface.
///
/// # Errors
///
/// Returns a contract resolution error when an artifact cannot be read, is larger
/// than [`MAX_ARTIFACT_BYTES`], publishes no contract spec section, or is a remote
/// target, which this build cannot reach. Every one of those is an environment
/// failure rather than anything about the contract's behaviour.
pub fn deploy(target: &Target, host: &LocalHost) -> Result<Deployment> {
    match target {
        Target::Fixture { defect } => {
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
            Ok(Deployment {
                contract: estamora_fixture_token::deploy(host.env(), *defect),
                network: LOCAL_NETWORK.to_owned(),
                // A fixture is registered from a Rust type, so there is no artifact
                // to hash and no claim to make about one.
                wasm_hash: None,
                interface: ExposedInterface::declared(
                    &methods,
                    format!("the fixture `{}`", defect.as_str()),
                ),
                seeding: Seeding::FixtureEntryPoints,
            })
        },
        Target::Wasm { path } => {
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
            let interface =
                ExposedInterface::from_wasm(&bytes, format!("the artifact {}", path.display()))?;
            let wasm_hash = estamora_certification::Digest::of_bytes(&bytes).to_string();

            Ok(Deployment {
                contract: host.register(bytes.as_slice()),
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
            })
        },
        Target::Remote {
            contract_id,
            network,
        } => Err(Error::new(
            ErrorClass::ContractResolutionError,
            format!(
                "contract {contract_id} on `{network}` cannot be resolved by this build: \
                 reading a deployed contract's interface and state needs a Soroban RPC \
                 transport, and none is linked into this runner. No verdict about the \
                 contract was reached, and this failure is classified as an environment \
                 failure rather than as a conformance result"
            ),
        )
        .with_context("contract", contract_id.clone())
        .with_context("network", network.clone())
        .with_context("reason", "network-transport-unavailable")),
    }
}

/// Whether `text` has the shape of a contract identifier.
///
/// A shape check rather than a checksum verification, deliberately and with a
/// consequence that matters: this can accept a mistyped identifier, and the transport
/// that would reject it is the one this build does not link. Checking the shape is
/// still worth doing, because the alternative is passing a path or a sentence to the
/// network layer and reporting whatever it says about it as a resolution outcome.
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
    use super::{Seeding, Target, looks_like_a_contract_id};
    use estamora_core::ErrorClass;
    use estamora_fixture_token::Defect;
    use estamora_soroban::LocalHost;

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
    fn a_remote_target_is_refused_as_an_environment_failure_with_the_reason_named() {
        // The decisive property: an unresolvable network must never be read as a
        // conformance result, and the refusal has to say why rather than merely fail.
        let id = format!("C{}", "A".repeat(55));
        let target = Target::parse(&id, Some("testnet")).unwrap();
        let host = LocalHost::at_default_point();
        let problem = super::deploy(&target, &host).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::ContractResolutionError);
        assert_eq!(
            problem.blame(),
            estamora_core::Blame::Environment,
            "an unreachable network must not blame the contract"
        );
        assert_eq!(
            problem.context_value("reason"),
            Some("network-transport-unavailable")
        );
    }

    #[test]
    fn a_fixture_deploys_with_the_interface_its_own_declaration_states() {
        let host = LocalHost::at_default_point();
        let deployment = super::deploy(
            &Target::Fixture {
                defect: Defect::MissingDecimals,
            },
            &host,
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
        let host = LocalHost::at_default_point();
        for defect in Defect::ALL {
            let deployment = super::deploy(&Target::Fixture { defect }, &host).unwrap();
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
