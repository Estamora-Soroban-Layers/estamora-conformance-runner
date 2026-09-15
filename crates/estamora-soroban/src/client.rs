//! Resolving a contract that is deployed to a network.
//!
//! # Why a network is read from rather than written to
//!
//! The obvious way to measure a deployed contract is to send it transactions and watch
//! what happens. Estamora deliberately does not do that, and the reason is the same one
//! the specification gives for fixed ledger points: a verdict that depends on a funded
//! account, on the state a shared ledger happens to be in, or on a node agreeing to
//! include a transaction is not a verdict anybody else can reproduce.
//!
//! So a network is used for *resolution* and never for *execution*. [`resolve`] reads the
//! contract's WebAssembly out of the ledger and hands back the same bytes a local
//! `.wasm` artifact would carry; `estamora` then measures them in the local host, with
//! the ledger point, the balances and the authorizations fixed by the vector it is
//! running. What the network contributes is the identity of the thing being measured —
//! the contract, the code it is running, the protocol version, the passphrase — and what
//! the local host contributes is determinism.
//!
//! That split is also what lets `inspect` work against a deployed contract: the interface
//! is a custom section of the artifact, so reading the artifact is reading the interface,
//! and nothing has to be deployed to find out what a contract exposes.
//!
//! # What is verified rather than trusted
//!
//! The ledger says which code a contract runs, and separately holds that code. Both
//! answers come from the same server, so agreeing with each other would prove nothing;
//! what makes the pair meaningful is that the code entry names its own hash. The bytes
//! fetched are hashed here and compared against the hash the contract instance declares,
//! so a node that returned the wrong code — by misconfiguration or on purpose — is
//! refused rather than measured. A conformance report about the wrong artifact is worse
//! than no report.

use std::time::Duration;

use base64::Engine as _;
use estamora_core::{Error, ErrorClass, Result};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use stellar_xdr::{
    ContractDataDurability, ContractExecutable, ContractId, Hash, LedgerEntry, LedgerEntryData,
    LedgerEntryExt, LedgerKey, LedgerKeyContractCode, LedgerKeyContractData, Limits, ReadXdr as _,
    ScAddress, ScVal, WriteXdr as _,
};

/// The largest contract artifact the runner will fetch.
///
/// A deployed contract is bounded by the network's own limits, so this is not a guess
/// about what is reasonable: it is the point past which the answer is not a contract and
/// the runner should stop reading rather than keep allocating.
pub const MAX_ARTIFACT_BYTES: usize = 4 * 1024 * 1024;

/// How long a single RPC call may take.
const RPC_TIMEOUT: Duration = Duration::from_secs(30);

/// The environment variable that overrides an endpoint.
///
/// A named network resolves to one operator's endpoint, and an operator's endpoint is a
/// convenience rather than a fact about the network. Anything that has to be exactly
/// reproducible — a CI job, an audit — should point at the endpoint it trusts, and this
/// is how it does that without the runner growing a configuration format.
pub const RPC_URL_VARIABLE: &str = "ESTAMORA_RPC_URL";

/// A network a contract can be resolved on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    /// The name the runner was asked for, as given.
    pub name: String,
    /// The endpoint that will be read.
    pub rpc_url: String,
}

/// The endpoints this build knows by name.
///
/// Deliberately two, and deliberately not a registry: a short list of networks a
/// conformance result is meaningful about. Anything else is reached by naming an
/// endpoint through [`RPC_URL_VARIABLE`], which makes the dependency on that particular
/// operator explicit in the command that produced a result.
const KNOWN_ENDPOINTS: [(&str, &str); 2] = [
    ("testnet", "https://soroban-testnet.stellar.org"),
    ("mainnet", "https://soroban-rpc.mainnet.stellar.gateway.fm"),
];

impl Network {
    /// The network a name refers to.
    ///
    /// # Errors
    ///
    /// Returns a usage error for a name this build does not know and has no endpoint
    /// override for. The failure names the networks that are known, because the
    /// difference between a mistyped network and an unlinked one is the difference
    /// between a fixable command and a bug report.
    pub fn lookup(name: &str, override_url: Option<&str>) -> Result<Self> {
        if let Some(url) = override_url.filter(|url| !url.is_empty()) {
            return Ok(Self {
                name: name.to_owned(),
                rpc_url: url.to_owned(),
            });
        }

        KNOWN_ENDPOINTS
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(known, url)| Self {
                name: (*known).to_owned(),
                rpc_url: (*url).to_owned(),
            })
            .ok_or_else(|| {
                let known: Vec<&str> = KNOWN_ENDPOINTS.iter().map(|(name, _)| *name).collect();
                Error::new(
                    ErrorClass::UsageError,
                    format!(
                        "`{name}` is not a network this build knows. Known networks: {}. \
                         Any other endpoint can be named through ${RPC_URL_VARIABLE}",
                        known.join(", ")
                    ),
                )
                .with_context("network", name)
            })
    }
}

/// A contract that was resolved on a network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedContract {
    /// The contract identifier, as given.
    pub contract_id: String,
    /// The network it was read from.
    pub network: String,
    /// The endpoint it was read from, so a result names where it came from.
    pub rpc_url: String,
    /// The network's passphrase, as the node reports it.
    pub passphrase: String,
    /// The protocol version the node reports, recorded because it is the version the
    /// contract's behaviour was written against.
    pub protocol_version: u32,
    /// The hash of the deployed code, as the contract instance declares it.
    pub wasm_hash: String,
    /// The deployed WebAssembly, verified to hash to [`Self::wasm_hash`].
    pub wasm: Vec<u8>,
    /// The contract's own instance ledger entry, exactly as the network stores it.
    ///
    /// Kept because it carries the contract's **instance storage** — the admin address,
    /// the decimals, the symbol: whatever a constructor wrote. Loading it into a local
    /// host is what lets a deployed contract be measured rather than redeployed, and it
    /// is why a constructor taking arguments is not the end of the road for the
    /// contracts that have one.
    pub instance: LedgerEntry,
}

/// Resolves a deployed contract to the artifact it is running.
///
/// # Errors
///
/// Every failure here is an environment or usage failure, never a contract failure: not
/// being able to read a contract says nothing about how it behaves, and reporting the
/// two as the same thing is the mistake this whole layer exists to avoid. A contract that
/// does not exist is distinguished from a network that could not be reached, because the
/// first is a fixable command and the second is somebody else's outage.
pub fn resolve(
    contract_id: &str,
    network_name: &str,
    override_url: Option<&str>,
) -> Result<ResolvedContract> {
    let contract = parse_contract_id(contract_id)?;
    let network = Network::lookup(network_name, override_url)?;

    // Every failure past this point is a failure to read one particular contract on one
    // particular network, whichever step produced it. Naming both once here rather than at
    // each `?` is what makes a resolution failure actionable from a CI log — and the one
    // that used to be missed was the commonest of all: a node that could not be reached,
    // which named its endpoint and nothing else.
    resolve_on(contract_id, &contract, &network).map_err(|problem| {
        problem
            .with_context("contract", contract_id)
            .with_context("network", network.name.clone())
    })
}

/// [`resolve`] with the identifier parsed and the network named.
fn resolve_on(
    contract_id: &str,
    contract: &ContractId,
    network: &Network,
) -> Result<ResolvedContract> {
    let client = Client::new(network)?;

    let (passphrase, protocol_version) = client.network_identity()?;

    let instance_key = LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::Contract(contract.clone()),
        key: ScVal::LedgerKeyContractInstance,
        durability: ContractDataDurability::Persistent,
    });

    let instance = client.ledger_entry(&instance_key)?.ok_or_else(|| {
        Error::new(
            ErrorClass::ContractResolutionError,
            format!(
                "no contract instance is stored at {contract_id} on `{}`. The identifier \
                     is well formed, so this is a contract that does not exist on that \
                     network rather than one that could not be read",
                network.name
            ),
        )
        .with_context("reason", "contract-not-found")
    })?;

    let declared_hash = executable_hash(&instance).ok_or_else(|| {
        Error::new(
            ErrorClass::ContractResolutionError,
            format!(
                "{contract_id} on `{}` is a Stellar Asset Contract, whose behaviour is \
                 implemented by the host rather than by a deployed artifact. There is no \
                 WebAssembly to measure, so no portability profile can be applied to it",
                network.name
            ),
        )
        .with_context("reason", "host-implemented-contract")
    })?;

    let code_key = LedgerKey::ContractCode(LedgerKeyContractCode {
        hash: declared_hash.clone(),
    });
    let code_entry = client.ledger_entry(&code_key)?.ok_or_else(|| {
        Error::new(
            ErrorClass::ContractResolutionError,
            format!(
                "{contract_id} on `{}` names code {} but no such code entry is stored. \
                     The ledger is inconsistent with itself, so nothing was measured",
                network.name,
                hex::encode(declared_hash.0)
            ),
        )
        .with_context("reason", "code-entry-missing")
    })?;

    let wasm = code_bytes(&code_entry).ok_or_else(|| {
        Error::new(
            ErrorClass::ContractResolutionError,
            format!("the code entry for {contract_id} is not a contract code entry"),
        )
    })?;

    if wasm.len() > MAX_ARTIFACT_BYTES {
        return Err(Error::new(
            ErrorClass::ContractResolutionError,
            format!(
                "the code behind {contract_id} is {} bytes, above the {MAX_ARTIFACT_BYTES}-byte \
                 limit this runner will fetch",
                wasm.len()
            ),
        ));
    }

    verify_code_hash(&declared_hash, &wasm).map_err(|problem| {
        problem
            .with_context("contract", contract_id)
            .with_context("network", network.name.clone())
    })?;

    Ok(ResolvedContract {
        contract_id: contract_id.to_owned(),
        network: network.name.clone(),
        rpc_url: network.rpc_url.clone(),
        passphrase,
        protocol_version,
        wasm_hash: hex::encode(declared_hash.0),
        wasm,
        instance,
    })
}

/// A contract identifier as a ledger address.
///
/// Decoded through `stellar-strkey` rather than through the SDK's `Address::from_str`,
/// which panics on anything it does not recognise. A value that arrives from a command
/// line is not a place to panic, and "this is not a contract identifier" is a usage
/// error a caller can act on.
fn parse_contract_id(text: &str) -> Result<ContractId> {
    match stellar_strkey::Strkey::from_string(text) {
        Ok(stellar_strkey::Strkey::Contract(contract)) => Ok(ContractId(Hash(contract.0))),
        // A well-formed key that is not a contract's. Distinguished from malformed input
        // because the two need different fixes, and an account genuinely cannot be
        // measured: it has no artifact.
        Ok(_) => Err(usage(text, "contract").with_context(
            "reason",
            "the identifier is a well-formed Stellar account key rather than a contract's, \
             and only a contract has an artifact to measure",
        )),
        Err(problem) => Err(usage(text, "contract").with_context("reason", problem.to_string())),
    }
}

/// A usage failure that names what was expected.
fn usage(text: &str, expected: &str) -> Error {
    Error::new(
        ErrorClass::UsageError,
        format!("`{text}` is not a {expected} identifier"),
    )
    .with_context("contract", text)
}

/// Rebuilds the ledger entry that `getLedgerEntries` returned in pieces.
///
/// A JSON-RPC node does not put a whole `LedgerEntry` in `entries[].xdr`. It returns the
/// entry *decomposed*: `xdr` carries the `LedgerEntryData`, `extXdr` carries the
/// `LedgerEntryExt`, and `lastModifiedLedgerSeq` is a JSON number beside them. Those
/// three fields together are everything a `LedgerEntry` is, so they are reassembled
/// here.
///
/// Reading `xdr` as a whole `LedgerEntry` instead — which is what this did before a live
/// node was ever asked — takes the entry's leading four bytes as a ledger sequence and
/// then fails on the next field's discriminant, reporting `undecodable-ledger-entry`
/// against a contract that is perfectly healthy.
fn assemble_entry(entry: &Value) -> Result<LedgerEntry> {
    let malformed = |field: &str| {
        Error::new(
            ErrorClass::NetworkError,
            format!("the node returned a ledger entry without {field}"),
        )
        .with_context("reason", "malformed-response")
    };

    let encoded = entry
        .get("xdr")
        .and_then(Value::as_str)
        .ok_or_else(|| malformed("the data XDR"))?;
    let data = LedgerEntryData::from_xdr_base64(encoded, Limits::none()).map_err(|problem| {
        Error::new(
            ErrorClass::NetworkError,
            format!("a ledger entry could not be decoded: {problem}"),
        )
        .with_context("reason", "undecodable-ledger-entry")
    })?;

    let last_modified_ledger_seq = entry
        .get("lastModifiedLedgerSeq")
        .and_then(Value::as_u64)
        .and_then(|sequence| u32::try_from(sequence).ok())
        .ok_or_else(|| malformed("the ledger sequence that last modified it"))?;

    // An entry with no extension is the overwhelmingly common case, and a node is free
    // to omit the field when it has nothing to say.
    let ext = match entry.get("extXdr").and_then(Value::as_str) {
        Some(encoded) => {
            LedgerEntryExt::from_xdr_base64(encoded, Limits::none()).map_err(|problem| {
                Error::new(
                    ErrorClass::NetworkError,
                    format!("a ledger entry extension could not be decoded: {problem}"),
                )
                .with_context("reason", "undecodable-ledger-entry")
            })?
        },
        None => LedgerEntryExt::V0,
    };

    Ok(LedgerEntry {
        last_modified_ledger_seq,
        data,
        ext,
    })
}

/// The hash a contract instance says it is running.
pub(crate) fn executable_hash(entry: &LedgerEntry) -> Option<Hash> {
    // Anything that is not a contract-data entry carrying a contract instance is not
    // what this key addresses, so it is treated as absent rather than guessed at.
    let LedgerEntryData::ContractData(data) = &entry.data else {
        return None;
    };
    match &data.val {
        ScVal::ContractInstance(instance) => match &instance.executable {
            // A Stellar Asset Contract is implemented by the host. There is no artifact,
            // so there is nothing to measure and this is not a failure of the contract.
            ContractExecutable::StellarAsset => None,
            ContractExecutable::Wasm(hash) => Some(hash.clone()),
        },
        _ => None,
    }
}

/// The bytes a contract code entry carries.
fn code_bytes(entry: &LedgerEntry) -> Option<Vec<u8>> {
    let LedgerEntryData::ContractCode(code) = &entry.data else {
        return None;
    };
    // `BytesM` is a length-bounded byte vector whose field is private, so the bytes are
    // taken through the `Deref` it implements rather than by reaching into it.
    Some(code.code.to_vec())
}

/// Refuses code that does not hash to what the contract instance declares.
///
/// This is the one property that makes reading a contract from a single server
/// meaningful: the two entries agree only if the artifact really is the one the contract
/// says it runs, and a node that answered with the wrong code is caught here.
fn verify_code_hash(declared: &Hash, wasm: &[u8]) -> Result<()> {
    let actual: [u8; 32] = Sha256::digest(wasm).into();
    if actual == declared.0 {
        return Ok(());
    }
    Err(Error::new(
        ErrorClass::NetworkError,
        format!(
            "the code read for this contract hashes to {} but the contract instance declares \
             {}. The artifact and the ledger disagree, so no verdict about this contract was \
             reached",
            hex::encode(actual),
            hex::encode(declared.0)
        ),
    )
    .with_context("reason", "artifact-hash-mismatch"))
}

/// A JSON-RPC client for the two calls resolution needs.
struct Client {
    http: reqwest::blocking::Client,
    url: String,
}

impl Client {
    fn new(network: &Network) -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .timeout(RPC_TIMEOUT)
            .build()
            .map_err(|problem| {
                Error::new(
                    ErrorClass::InternalError,
                    format!("the HTTP client could not be built: {problem}"),
                )
            })?;
        Ok(Self {
            http,
            url: network.rpc_url.clone(),
        })
    }

    /// The network's passphrase and protocol version.
    fn network_identity(&self) -> Result<(String, u32)> {
        let value = self.call("getNetwork", None)?;
        let result = value.get("result").ok_or_else(|| {
            Error::new(
                ErrorClass::NetworkError,
                "the node answered getNetwork without a result",
            )
            .with_context("reason", "malformed-response")
        })?;
        let passphrase = result
            .get("passphrase")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                Error::new(
                    ErrorClass::NetworkError,
                    "the node answered getNetwork without a passphrase",
                )
                .with_context("reason", "malformed-response")
            })?;
        let protocol = result
            .get("protocolVersion")
            .and_then(Value::as_u64)
            .and_then(|version| u32::try_from(version).ok())
            .ok_or_else(|| {
                Error::new(
                    ErrorClass::NetworkError,
                    "the node answered getNetwork without a protocol version",
                )
                .with_context("reason", "malformed-response")
            })?;
        Ok((passphrase.to_owned(), protocol))
    }

    /// One ledger entry, or `None` when the ledger has none at that key.
    ///
    /// The failures here name neither the contract nor the network: every caller is
    /// resolving one contract on one network and attaches both once, at the boundary.
    fn ledger_entry(&self, key: &LedgerKey) -> Result<Option<LedgerEntry>> {
        let encoded = key.to_xdr_base64(Limits::none()).map_err(|problem| {
            Error::new(
                ErrorClass::InternalError,
                format!("a ledger key could not be encoded: {problem}"),
            )
        })?;
        let value = self.call("getLedgerEntries", Some(json!({ "keys": [encoded] })))?;
        let Some(entry) = value
            .get("result")
            .and_then(|result| result.get("entries"))
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
        else {
            return Ok(None);
        };
        assemble_entry(entry).map(Some)
    }

    /// Posts one JSON-RPC request and returns the decoded body.
    fn call(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let mut body = json!({ "jsonrpc": "2.0", "id": 1, "method": method });
        if let Some(params) = params {
            body["params"] = params;
        }

        let response = self
            .http
            .post(&self.url)
            .json(&body)
            .send()
            .map_err(|problem| self.network_error(method, &problem.to_string()))?;

        let status = response.status();
        let text = response
            .text()
            .map_err(|problem| self.network_error(method, &problem.to_string()))?;

        if !status.is_success() {
            return Err(self.network_error(
                method,
                &format!("the node answered {status}: {}", truncate(&text, 200)),
            ));
        }

        let value: Value = serde_json::from_str(&text).map_err(|problem| {
            Error::new(
                ErrorClass::NetworkError,
                format!("the node's answer to {method} was not JSON: {problem}"),
            )
            .with_context("rpc_url", self.url.clone())
            .with_context("reason", "malformed-response")
        })?;

        // A JSON-RPC error is the node reporting that it could not answer the question.
        // It is never a statement about a contract, so it never carries contract blame.
        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("no message");
            let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
            return Err(Error::new(
                ErrorClass::NetworkError,
                format!("the node refused {method} with error {code}: {message}"),
            )
            .with_context("rpc_url", self.url.clone())
            .with_context("reason", "rpc-error"));
        }

        Ok(value)
    }

    /// The failure to raise when the node could not be reached at all.
    ///
    /// A `NETWORK_ERROR`, so that an outage cannot be mistaken for a contract defect, and
    /// it names the endpoint because which endpoint was unreachable is the first thing
    /// anybody asks.
    fn network_error(&self, method: &str, detail: &str) -> Error {
        Error::new(
            ErrorClass::NetworkError,
            format!("{method} could not be completed: {}", truncate(detail, 300)),
        )
        .with_context("rpc_url", self.url.clone())
        .with_context("reason", "network-unreachable")
    }
}

/// Shortens a body for a message, so a node's error page does not become the report.
fn truncate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// Base64-decodes a value, for callers that need the raw entry bytes.
///
/// # Errors
///
/// Returns a network error when the value is not base64, which means the answer was not
/// the ledger entry it was supposed to be.
pub fn decode_base64(value: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|problem| {
            Error::new(
                ErrorClass::NetworkError,
                format!("a ledger entry was not valid base64: {problem}"),
            )
            .with_context("reason", "malformed-response")
        })
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "tests may panic; a failing test is the signal"
    )]

    use super::{
        Network, RPC_URL_VARIABLE, assemble_entry, code_bytes, parse_contract_id, truncate,
        verify_code_hash,
    };
    use estamora_core::ErrorClass;
    use serde_json::{Value, json};
    use sha2::{Digest as _, Sha256};
    use stellar_xdr::{
        BytesM, ContractCodeEntry, ContractCodeEntryExt, Hash, LedgerEntryData, LedgerEntryExt,
        Limits, ReadXdr as _, WriteXdr as _,
    };

    /// One `getLedgerEntries` entry, shaped exactly as a node sends it.
    ///
    /// The three fields are deliberately separate, because that separation is the whole
    /// point: a node does not send a whole `LedgerEntry` in one string.
    fn entry_as_a_node_sends_it(data: &LedgerEntryData, ext: Option<&LedgerEntryExt>) -> Value {
        let mut entry = json!({
            "xdr": data.to_xdr_base64(Limits::none()).unwrap(),
            "lastModifiedLedgerSeq": 4_609_662_u32,
        });
        if let Some(ext) = ext {
            entry["extXdr"] = Value::String(ext.to_xdr_base64(Limits::none()).unwrap());
        }
        entry
    }

    /// Contract code as it appears inside a `ContractCode` ledger entry.
    fn code_entry(module: &[u8]) -> LedgerEntryData {
        let hash = Hash(Sha256::digest(module).into());
        LedgerEntryData::ContractCode(ContractCodeEntry {
            ext: ContractCodeEntryExt::V0,
            hash,
            code: BytesM::try_from(module.to_vec()).unwrap(),
        })
    }

    #[test]
    fn a_node_s_decomposed_entry_field_is_the_entry_data_not_a_whole_entry() {
        // The regression this fixes: `entries[].xdr` carries the `LedgerEntryData` on its
        // own. Decoding it as a whole `LedgerEntry` reads its leading four bytes as a
        // ledger sequence and then fails on the discriminant, which against a live node
        // reported `undecodable-ledger-entry` for a contract that was entirely healthy.
        let module = b"\0asm\x01\0\0\0";
        let entry = entry_as_a_node_sends_it(&code_entry(module), None);

        let assembled = assemble_entry(&entry).expect("a well-formed entry must assemble");
        assert_eq!(assembled.last_modified_ledger_seq, 4_609_662);
        assert_eq!(
            code_bytes(&assembled).as_deref(),
            Some(module.as_slice()),
            "the bytes must survive the round trip through the accessor the transport reads"
        );

        // And the old reading of the same field really does fail, so the test above is
        // asserting something rather than passing by luck.
        let as_whole_entry = stellar_xdr::LedgerEntry::from_xdr_base64(
            entry["xdr"].as_str().unwrap(),
            Limits::none(),
        );
        assert!(
            as_whole_entry.is_err(),
            "if a node ever did send a whole entry here, this test would need rewriting"
        );
    }

    #[test]
    fn an_absent_extension_assembles_as_the_empty_one() {
        let entry = entry_as_a_node_sends_it(&code_entry(b"\0asm"), None);
        let assembled = assemble_entry(&entry).unwrap();
        assert!(
            matches!(assembled.ext, LedgerEntryExt::V0),
            "an entry with nothing to say about itself is the V0 extension"
        );
    }

    #[test]
    fn an_entry_without_a_ledger_sequence_is_refused_rather_than_defaulted() {
        // A plausible-looking default here would silently misreport when an entry was
        // last touched, so a missing sequence is a malformed response.
        let mut entry = entry_as_a_node_sends_it(&code_entry(b"\0asm"), None);
        entry
            .as_object_mut()
            .unwrap()
            .remove("lastModifiedLedgerSeq");

        let problem = assemble_entry(&entry).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::NetworkError);
        assert_eq!(problem.context_value("reason"), Some("malformed-response"));
    }

    #[test]
    fn an_entry_without_data_is_refused() {
        let entry = json!({ "lastModifiedLedgerSeq": 4_609_662_u32 });
        let problem = assemble_entry(&entry).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::NetworkError);
        assert_eq!(problem.context_value("reason"), Some("malformed-response"));
    }

    #[test]
    fn undecodable_entry_data_is_named_as_such() {
        let entry = json!({
            "xdr": "bm90IHhkcg==",
            "lastModifiedLedgerSeq": 4_609_662_u32,
        });
        let problem = assemble_entry(&entry).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::NetworkError);
        assert_eq!(
            problem.context_value("reason"),
            Some("undecodable-ledger-entry")
        );
    }

    #[test]
    fn an_undecodable_extension_is_refused_rather_than_ignored() {
        // An extension the runner cannot read is a version it does not understand, and
        // guessing at it would mean measuring something other than what is deployed.
        let entry = json!({
            "xdr": code_entry(b"\0asm").to_xdr_base64(Limits::none()).unwrap(),
            "lastModifiedLedgerSeq": 4_609_662_u32,
            "extXdr": "bm90IHhkcg==",
        });
        let problem = assemble_entry(&entry).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::NetworkError);
        assert_eq!(
            problem.context_value("reason"),
            Some("undecodable-ledger-entry")
        );
    }

    #[test]
    fn a_known_network_resolves_to_an_endpoint() {
        let testnet = Network::lookup("testnet", None).unwrap();
        assert_eq!(testnet.name, "testnet");
        assert_eq!(testnet.rpc_url, "https://soroban-testnet.stellar.org");
        assert!(Network::lookup("mainnet", None).is_ok());
    }

    #[test]
    fn an_unknown_network_is_a_usage_error_that_names_the_known_ones() {
        let problem = Network::lookup("testnett", None).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::UsageError);
        assert!(
            problem.message().contains("testnet"),
            "{}",
            problem.message()
        );
        assert!(
            problem.message().contains(RPC_URL_VARIABLE),
            "an unknown network must say how to name an endpoint instead: {}",
            problem.message()
        );
    }

    #[test]
    fn an_override_names_the_endpoint_and_is_used_verbatim() {
        let network = Network::lookup("staging", Some("https://rpc.internal.example")).unwrap();
        assert_eq!(network.name, "staging");
        assert_eq!(network.rpc_url, "https://rpc.internal.example");
    }

    #[test]
    fn an_empty_override_is_treated_as_absent() {
        let network = Network::lookup("testnet", Some("")).unwrap();
        assert_eq!(network.rpc_url, "https://soroban-testnet.stellar.org");
    }

    #[test]
    fn an_account_identifier_is_refused_as_a_contract() {
        // Encoded with the same library that decodes it, so this is a genuinely valid
        // StrKey rather than a hand-written string that happens to start with `G`. The
        // distinction matters: a fabricated one tests the malformed-input path twice and
        // the account path never.
        let account =
            stellar_strkey::Strkey::PublicKeyEd25519(stellar_strkey::ed25519::PublicKey([7; 32]))
                .to_string();
        let problem = parse_contract_id(account.as_str()).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::UsageError);
        assert!(
            problem.message().contains("not a contract"),
            "an account identifier must be refused as a contract rather than as malformed: {}",
            problem.message()
        );
        assert_eq!(
            problem
                .context_value("reason")
                .map(|reason| reason.contains("account")),
            Some(true)
        );
    }

    #[test]
    fn a_malformed_identifier_is_refused_before_any_network_call() {
        let problem = parse_contract_id("Cnot-a-contract").unwrap_err();
        assert_eq!(problem.class(), ErrorClass::UsageError);
    }

    #[test]
    fn code_that_does_not_hash_to_what_the_contract_declares_is_refused() {
        let code = b"\0asm\x01\0\0\0";
        let declared = Hash([9; 32]);
        let problem = verify_code_hash(&declared, code).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::NetworkError);
        assert_eq!(
            problem.context_value("reason"),
            Some("artifact-hash-mismatch")
        );
    }

    #[test]
    fn code_that_hashes_to_what_the_contract_declares_is_accepted() {
        let code = b"\0asm\x01\0\0\0";
        let declared = Hash(Sha256::digest(code).into());
        assert!(verify_code_hash(&declared, code).is_ok());
    }

    #[test]
    fn a_long_body_is_shortened_on_a_character_boundary() {
        assert_eq!(truncate("short", 10), "short");
        let long = "é".repeat(10);
        let shortened = truncate(&long, 5);
        assert!(shortened.ends_with('…'));
        assert!(shortened.len() <= 5 + "…".len() + 1);
    }
}
