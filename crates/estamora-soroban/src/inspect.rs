//! Reading a contract's interface.
//!
//! Inspection runs before any behavioural vector, and it is a correctness
//! requirement rather than a nicety: the host reports a deliberate `panic!()` and a
//! call to a method that does not exist through the same channel, so an abort is only
//! readable as the contract's own refusal once the method is known to exist. A
//! contract with a missing `decimals` would otherwise look exactly like a contract
//! that correctly rejects an unauthorized call.
//!
//! # Where an interface comes from
//!
//! Soroban's SDK embeds a contract spec in the compiled WebAssembly as a
//! `contractspecv0` custom section, and [`ExposedInterface::from_wasm`] reads it. That
//! is the only source that describes a contract the runner has never seen, and it is
//! what makes inspecting a deployed artifact possible at all.
//!
//! An in-repository fixture declares its interface instead, because it is a Rust type
//! rather than a file: [`ExposedInterface::declared`] is how a fixture states what it
//! exposes. Both routes produce the same structure, so the assertion layer cannot tell
//! which was used — and a fixture therefore cannot exercise a code path a real
//! deployment does not take.
//!
//! # What inspection does not establish
//!
//! That the interface is *right*. A contract that publishes a spec section declaring
//! methods it does not implement will pass inspection and fail every vector. That is
//! the correct outcome, and it is why interface compatibility is reported as the
//! weakest of the seven claims rather than as evidence of anything.

use estamora_core::{Error, ErrorClass, Result};
use soroban_sdk::xdr::{Limited, Limits, ReadXdr as _, ScSpecEntry, ScSpecTypeDef, ScSymbol};
use std::io::Cursor;

/// The custom section the SDK embeds a contract's interface in.
pub const SPEC_SECTION: &str = "contractspecv0";

/// One parameter of an exposed method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExposedParameter {
    /// The parameter's name, when the artifact names it.
    pub name: Option<String>,
    /// Its type, in the canonical spelling the assertion layer compares against.
    pub type_name: String,
}

/// One method a contract exposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExposedMethod {
    /// The method's name.
    pub name: String,
    /// Its parameters, in order.
    pub parameters: Vec<ExposedParameter>,
    /// Its return type, canonically spelled.
    pub returns: Option<String>,
    /// Whether the contract declares it read-only.
    ///
    /// `false` when the artifact does not say. A read the runner performs through a
    /// method it believed was read-only but was not would still be an observation;
    /// what the flag changes is whether the declaration *claimed* it.
    pub readonly: bool,
}

/// A method a fixture declares it exposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredMethod {
    /// The method's name.
    pub name: &'static str,
    /// Parameter type names, in order.
    pub parameters: &'static [&'static str],
    /// The return type name, or `None` for a void return.
    pub returns: Option<&'static str>,
    /// Whether the fixture declares it read-only.
    pub readonly: bool,
}

/// What a contract exposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExposedInterface {
    /// The exposed methods.
    pub methods: Vec<ExposedMethod>,
    /// Where the interface came from, so that a report can say how the claim was
    /// established rather than only that it was.
    pub source: String,
}

impl ExposedInterface {
    /// An interface a fixture declares.
    #[must_use]
    pub fn declared(methods: &[DeclaredMethod], source: impl Into<String>) -> Self {
        Self {
            methods: methods
                .iter()
                .map(|method| ExposedMethod {
                    name: method.name.to_owned(),
                    parameters: method
                        .parameters
                        .iter()
                        .map(|type_name| ExposedParameter {
                            name: None,
                            type_name: (*type_name).to_owned(),
                        })
                        .collect(),
                    returns: method.returns.map(ToOwned::to_owned),
                    readonly: method.readonly,
                })
                .collect(),
            source: source.into(),
        }
    }

    /// Reads the interface a compiled contract publishes.
    ///
    /// # Errors
    ///
    /// Returns a contract-resolution error when the bytes are not WebAssembly, when
    /// there is no spec section, or when an entry in it cannot be decoded. Refusing
    /// rather than returning a partial interface is deliberate: a partial interface
    /// would report a method as missing when it merely was not read, and the
    /// assertion layer would then treat a working contract as non-conformant.
    pub fn from_wasm(wasm: &[u8], source: impl Into<String>) -> Result<Self> {
        let section = custom_section(wasm, SPEC_SECTION)?;
        let entries = decode_entries(section)?;

        let mut methods = Vec::new();
        for entry in entries {
            let ScSpecEntry::FunctionV0(function) = entry else {
                // Structs, unions, enums, error enums and events are declarations the
                // interface dimension does not compare: a profile states method
                // requirements, and an event requirement is checked behaviourally
                // against what the contract actually emitted rather than against what
                // it declared it would.
                continue;
            };
            let name = symbol_name(&function.name);
            let parameters = function
                .inputs
                .iter()
                .map(|input| ExposedParameter {
                    name: Some(input.name.to_string()),
                    type_name: canonical(&input.type_),
                })
                .collect();
            let returns = function
                .outputs
                .first()
                .map(canonical)
                .filter(|name| name != "void");
            methods.push(ExposedMethod {
                name,
                parameters,
                returns,
                // The spec section does not record mutability, so nothing is
                // claimed: a method's read-only nature is a property the runner
                // observes by calling it, not one the artifact asserts.
                readonly: false,
            });
        }

        Ok(Self {
            methods,
            source: source.into(),
        })
    }

    /// The exposed method with this name, if there is one.
    #[must_use]
    pub fn method(&self, name: &str) -> Option<&ExposedMethod> {
        self.methods.iter().find(|method| method.name == name)
    }

    /// Whether a method of this name is exposed.
    #[must_use]
    pub fn declares(&self, name: &str) -> bool {
        self.method(name).is_some()
    }
}

/// Finds a named custom section in a WebAssembly module.
///
/// # Errors
///
/// Returns a contract-resolution error when the module is malformed, when a length
/// runs past the end of the bytes, or when the section is absent. Malformed input is a
/// resolution failure rather than an environment one: the artifact is not one the
/// runner can read, whatever the network is doing.
fn custom_section<'a>(wasm: &'a [u8], name: &str) -> Result<&'a [u8]> {
    const HEADER: usize = 8;
    if wasm.len() < HEADER || &wasm[..4] != b"\0asm" {
        return Err(Error::new(
            ErrorClass::ContractResolutionError,
            "the artifact is not a WebAssembly module: it does not begin with the magic bytes",
        ));
    }

    let mut cursor = HEADER;
    while cursor < wasm.len() {
        let id = wasm[cursor];
        cursor += 1;
        let (size, next) = leb128(wasm, cursor)?;
        cursor = next;
        let size = usize::try_from(size).map_err(|_| {
            Error::new(
                ErrorClass::ContractResolutionError,
                "a WebAssembly section declares a size this runner cannot index",
            )
        })?;
        let end = cursor.checked_add(size).filter(|end| *end <= wasm.len());
        let Some(end) = end else {
            return Err(Error::new(
                ErrorClass::ContractResolutionError,
                "a WebAssembly section runs past the end of the module",
            ));
        };

        if id == 0 {
            // A custom section's payload begins with its own name, length-prefixed.
            let (name_length, payload) = leb128(wasm, cursor)?;
            let name_length = usize::try_from(name_length).map_err(|_| {
                Error::new(
                    ErrorClass::ContractResolutionError,
                    "a custom section declares a name length this runner cannot index",
                )
            })?;
            let name_end = payload
                .checked_add(name_length)
                .filter(|name_end| *name_end <= end);
            let Some(name_end) = name_end else {
                return Err(Error::new(
                    ErrorClass::ContractResolutionError,
                    "a custom section's name runs past the end of the section",
                ));
            };
            if &wasm[payload..name_end] == name.as_bytes() {
                return Ok(&wasm[name_end..end]);
            }
        }

        cursor = end;
    }

    Err(Error::new(
        ErrorClass::ContractResolutionError,
        format!(
            "the artifact publishes no `{name}` section, so its interface cannot be read; a \
             contract built without the contract spec embedded cannot be inspected"
        ),
    ))
}

/// Reads an unsigned LEB128 integer.
///
/// # Errors
///
/// Returns a contract-resolution error when the encoding runs past the end of the
/// bytes or does not terminate within the five bytes a 32-bit length occupies.
fn leb128(bytes: &[u8], mut cursor: usize) -> Result<(u32, usize)> {
    let mut value: u32 = 0;
    let mut shift = 0;
    loop {
        let Some(byte) = bytes.get(cursor) else {
            return Err(Error::new(
                ErrorClass::ContractResolutionError,
                "a WebAssembly length field is truncated",
            ));
        };
        cursor += 1;
        value |= u32::from(byte & 0x7f).wrapping_shl(shift);
        if byte & 0x80 == 0 {
            return Ok((value, cursor));
        }
        shift += 7;
        if shift >= 35 {
            return Err(Error::new(
                ErrorClass::ContractResolutionError,
                "a WebAssembly length field does not terminate",
            ));
        }
    }
}

/// Decodes every entry in a spec section.
///
/// # Errors
///
/// Returns a contract-resolution error if an entry cannot be decoded. See
/// [`ExposedInterface::from_wasm`] for why a partial read is not acceptable.
fn decode_entries(section: &[u8]) -> Result<Vec<ScSpecEntry>> {
    let mut cursor = Cursor::new(section);
    let mut entries = Vec::new();
    while usize::try_from(cursor.position()).unwrap_or(usize::MAX) < section.len() {
        let mut limited = Limited::new(&mut cursor, Limits::none());
        let entry = ScSpecEntry::read_xdr(&mut limited).map_err(|problem| {
            Error::new(
                ErrorClass::ContractResolutionError,
                format!("the contract spec section could not be decoded: {problem}"),
            )
        })?;
        entries.push(entry);
    }
    Ok(entries)
}

/// A symbol's text.
fn symbol_name(symbol: &ScSymbol) -> String {
    symbol.0.to_string()
}

/// A type definition in the canonical spelling the assertion layer compares against.
fn canonical(type_def: &ScSpecTypeDef) -> String {
    match type_def {
        ScSpecTypeDef::Val => "val".to_owned(),
        ScSpecTypeDef::Bool => "bool".to_owned(),
        ScSpecTypeDef::Void => "void".to_owned(),
        ScSpecTypeDef::Error => "error".to_owned(),
        ScSpecTypeDef::U32 => "u32".to_owned(),
        ScSpecTypeDef::I32 => "i32".to_owned(),
        ScSpecTypeDef::U64 => "u64".to_owned(),
        ScSpecTypeDef::I64 => "i64".to_owned(),
        ScSpecTypeDef::Timepoint => "timepoint".to_owned(),
        ScSpecTypeDef::Duration => "duration".to_owned(),
        ScSpecTypeDef::U128 => "u128".to_owned(),
        ScSpecTypeDef::I128 => "i128".to_owned(),
        ScSpecTypeDef::U256 => "u256".to_owned(),
        ScSpecTypeDef::I256 => "i256".to_owned(),
        ScSpecTypeDef::Bytes => "bytes".to_owned(),
        ScSpecTypeDef::String => "string".to_owned(),
        ScSpecTypeDef::Symbol => "symbol".to_owned(),
        ScSpecTypeDef::Address => "address".to_owned(),
        ScSpecTypeDef::MuxedAddress => "muxed_address".to_owned(),
        ScSpecTypeDef::Option(option) => format!("option<{}>", canonical(&option.value_type)),
        ScSpecTypeDef::Result(result) => format!(
            "result<{},{}>",
            canonical(&result.ok_type),
            canonical(&result.error_type)
        ),
        ScSpecTypeDef::Vec(vector) => format!("vec<{}>", canonical(&vector.element_type)),
        ScSpecTypeDef::Map(map) => format!(
            "map<{},{}>",
            canonical(&map.key_type),
            canonical(&map.value_type)
        ),
        ScSpecTypeDef::Tuple(tuple) => {
            let parts: Vec<String> = tuple.value_types.iter().map(canonical).collect();
            format!("tuple<{}>", parts.join(","))
        },
        ScSpecTypeDef::BytesN(fixed) => format!("bytes_n<{}>", fixed.n),
        ScSpecTypeDef::Udt(named) => named.name.to_string().to_lowercase(),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{DeclaredMethod, ExposedInterface, canonical, custom_section, leb128};
    use estamora_core::ErrorClass;
    use soroban_sdk::xdr::{ScSpecTypeBytesN, ScSpecTypeDef, ScSpecTypeOption, ScSpecTypeVec};

    #[test]
    fn a_declared_interface_carries_the_names_a_profile_compares() {
        let interface = ExposedInterface::declared(
            &[
                DeclaredMethod {
                    name: "balance",
                    parameters: &["address"],
                    returns: Some("i128"),
                    readonly: true,
                },
                DeclaredMethod {
                    name: "transfer",
                    parameters: &["address", "address", "i128"],
                    returns: None,
                    readonly: false,
                },
            ],
            "the fixture",
        );
        assert!(interface.declares("balance"));
        assert!(interface.declares("transfer"));
        assert!(!interface.declares("decimals"));
        assert_eq!(interface.method("transfer").unwrap().parameters.len(), 3);
    }

    #[test]
    fn composed_types_read_the_way_a_profile_writes_them() {
        assert_eq!(
            canonical(&ScSpecTypeDef::Option(Box::new(ScSpecTypeOption {
                value_type: Box::new(ScSpecTypeDef::U64),
            }))),
            "option<u64>"
        );
        assert_eq!(
            canonical(&ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
                element_type: Box::new(ScSpecTypeDef::Address),
            }))),
            "vec<address>"
        );
        assert_eq!(
            canonical(&ScSpecTypeDef::BytesN(ScSpecTypeBytesN { n: 32 })),
            "bytes_n<32>"
        );
        assert_eq!(canonical(&ScSpecTypeDef::I128), "i128");
    }

    #[test]
    fn an_artifact_that_is_not_wasm_is_named_as_a_resolution_failure() {
        let problem = ExposedInterface::from_wasm(b"not wasm at all", "test").unwrap_err();
        assert_eq!(problem.class(), ErrorClass::ContractResolutionError);
        assert!(
            problem.message().contains("magic bytes"),
            "{}",
            problem.message()
        );
    }

    #[test]
    fn a_module_without_a_spec_section_says_so_rather_than_reporting_a_wrong_interface() {
        // A minimal module: the header and nothing else.
        let wasm = b"\0asm\x01\0\0\0";
        let problem = custom_section(wasm, "contractspecv0").unwrap_err();
        assert!(
            problem.message().contains("contractspecv0"),
            "{}",
            problem.message()
        );
    }

    #[test]
    fn a_section_that_runs_past_the_module_is_refused() {
        // A custom section claiming 4096 bytes in an eight-byte module.
        let wasm = b"\0asm\x01\0\0\0\x00\x80\x20";
        let problem = custom_section(wasm, "contractspecv0").unwrap_err();
        assert!(
            problem.message().contains("past the end"),
            "{}",
            problem.message()
        );
    }

    #[test]
    fn an_leb128_that_does_not_terminate_is_refused_rather_than_looping() {
        let endless = [0x80u8; 8];
        assert!(leb128(&endless, 0).is_err());
        assert!(leb128(&[], 0).is_err());
        assert_eq!(leb128(&[0x7f], 0).unwrap(), (127, 1));
        assert_eq!(leb128(&[0x80, 0x01], 0).unwrap(), (128, 2));
    }
}
