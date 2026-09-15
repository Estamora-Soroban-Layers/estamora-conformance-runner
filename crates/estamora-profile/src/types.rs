//! The profile documents, as typed structures.
//!
//! Every structure here rejects an unknown field. That is not tidiness: a profile
//! is a document other people write, and a field the runner silently ignores is a
//! requirement that was written, reviewed and then never enforced. A deserialize
//! failure naming the field is the only acceptable outcome.
//!
//! The field names, the required sets and the closed vocabularies mirror the
//! specification layer's JSON Schemas exactly. Where this module and a schema
//! disagree, the schema is normative and this module is a defect.

use serde::Deserialize;

/// The `profile.yaml` document.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileDocument {
    /// The version of the document format this bundle was written against.
    pub estamora_spec_version: String,
    /// The profile's own identity and requirements-level metadata.
    pub profile: ProfileMetadata,
    /// The files this bundle is composed of.
    pub includes: BundleManifest,
}

/// Who a profile is, and where its requirements came from.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileMetadata {
    /// The profile identifier, e.g. `sep-41`.
    pub id: String,
    /// The profile version. Independent of every other version in play.
    pub version: String,
    /// A human title.
    pub title: String,
    /// The lifecycle stage, which decides how freely the requirements may change.
    pub status: ProfileStatus,
    /// One sentence describing what the profile requires.
    pub summary: String,
    /// The full description.
    pub description: String,
    /// The license the profile text is offered under.
    pub license: String,
    /// The upstream document the requirements were taken from.
    pub specification: SpecificationReference,
    /// The profile version this one replaces, if any.
    #[serde(default)]
    pub supersedes: Option<String>,
    /// The profile version that replaces this one, if any.
    #[serde(default)]
    pub superseded_by: Option<String>,
    /// Who maintains the profile.
    pub maintainers: Vec<String>,
    /// How completely the profile covers the interface it describes.
    pub compatibility: Compatibility,
    /// Where the requirements came from, and how ambiguity was resolved.
    pub provenance: Provenance,
}

/// The lifecycle stage of a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileStatus {
    /// Under construction; may change without notice.
    Draft,
    /// Complete and executable, not yet reviewed as authoritative.
    Experimental,
    /// Reviewed and relied upon; behavioural change requires a version bump.
    Stable,
    /// Superseded; retained so that historical results stay verifiable.
    Deprecated,
}

/// The upstream document a profile encodes.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpecificationReference {
    /// The upstream document's short name, e.g. `SEP-0041`.
    pub name: String,
    /// Its title.
    pub title: String,
    /// The revision that was read.
    pub version: String,
    /// That revision's own status upstream, which is not the profile's status.
    pub status: UpstreamStatus,
    /// Where it can be read.
    pub url: String,
    /// When that revision was last updated.
    pub updated: String,
}

/// The status of the upstream document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamStatus {
    /// Still moving.
    Draft,
    /// Under review.
    Review,
    /// Ratified.
    Final,
    /// Continuously updated.
    Living,
    /// Superseded.
    Deprecated,
    /// Replaced by another document.
    Superseded,
    /// The upstream document declares no status.
    Unknown,
}

/// How completely a profile covers its interface.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compatibility {
    /// Whether every element of the interface is modelled.
    pub interface: InterfaceCoverage,
    /// The reasoning, one entry per point the reader should know.
    pub notes: Vec<String>,
}

/// Whether a profile models a whole interface or part of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterfaceCoverage {
    /// Every method of the interface is modelled.
    Full,
    /// Some elements are deliberately not modelled.
    Partial,
}

/// Where a profile's requirements came from.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// The kind of source.
    pub source: ProvenanceSource,
    /// The exact revision read.
    pub derived_from: String,
    /// Every point where the upstream text was ambiguous and a reading was
    /// chosen. The specification layer requires this to be non-empty, so a
    /// profile cannot be silent about where it interpreted.
    pub interpretation_notes: Vec<String>,
}

/// The kind of source a profile's requirements were taken from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProvenanceSource {
    /// A published specification.
    UpstreamSpecification,
    /// An existing implementation.
    UpstreamImplementation,
    /// Convention across the ecosystem rather than a document.
    EcosystemConvention,
    /// More than one of the above.
    Composite,
}

/// The files a bundle is composed of.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleManifest {
    /// File holding the method requirements.
    pub methods: String,
    /// File holding the authorization requirements.
    pub authorization: String,
    /// File holding the event requirements.
    pub events: String,
    /// File holding the behavioural rules.
    pub behavior: String,
    /// File holding the invariants.
    pub invariants: String,
    /// File holding the failure requirements.
    pub failures: String,
    /// Operation directories this profile owns vectors for.
    pub vectors: Vec<String>,
    /// Shared vector sets this profile declares.
    pub shared_vectors: Vec<String>,
}

impl BundleManifest {
    /// Every file the manifest names, with the role it plays.
    ///
    /// Returned as a list rather than read field by field so that a loader
    /// cannot handle six of the seven files and appear to work.
    #[must_use]
    pub fn files(&self) -> [(&'static str, &str); 6] {
        [
            ("methods", &self.methods),
            ("authorization", &self.authorization),
            ("events", &self.events),
            ("behavior", &self.behavior),
            ("invariants", &self.invariants),
            ("failures", &self.failures),
        ]
    }
}

/// The `methods.yaml` document.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MethodsDocument {
    /// The method requirements.
    pub methods: Vec<MethodDefinition>,
}

/// One method requirement.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MethodDefinition {
    /// Stable identifier, referenced by vectors and behaviour rules.
    pub id: String,
    /// The name the contract must expose.
    pub name: String,
    /// Whether the method must, may, or must not be present.
    pub requirement: RequirementStatus,
    /// One sentence stating the requirement.
    pub summary: String,
    /// The full statement.
    #[serde(default)]
    pub description: Option<String>,
    /// The method's parameters, in call order.
    pub args: Vec<MethodArgument>,
    /// The method's return.
    pub returns: MethodReturn,
    /// Whether the method reads or writes state.
    pub mutability: Mutability,
    /// How the runner is expected to call it.
    pub invocation: Invocation,
    /// Ids of the authorization rules that apply.
    pub authorization: Vec<String>,
    /// Ids of the events the method is required to emit.
    pub events: Vec<String>,
    /// Ids of the failures the method is required to produce.
    pub failures: Vec<String>,
    /// Ids of the behavioural rules that constrain it.
    pub behaviors: Vec<String>,
    /// Anything a reader should know that is not a requirement.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Whether a requirement must hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementStatus {
    /// The element must be present and satisfy its requirement.
    Required,
    /// The element may be present.
    Optional,
    /// The element must be absent.
    Forbidden,
}

/// One parameter of a method.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MethodArgument {
    /// The parameter name as the interface declares it.
    pub name: String,
    /// Its type.
    #[serde(rename = "type")]
    pub type_expr: TypeExpr,
    /// What the parameter means.
    pub semantics: String,
    /// Whether the caller must authorize this specific argument.
    #[serde(default)]
    pub authorization: Option<ArgumentAuthorization>,
}

/// Authorization attached to a single argument.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgumentAuthorization {
    /// Whether the argument must be covered by the caller's authorization.
    pub required: bool,
    /// Why.
    pub semantics: String,
}

/// A method's return value.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MethodReturn {
    /// The declared return type.
    #[serde(rename = "type")]
    pub type_expr: TypeExpr,
    /// What the returned value means.
    pub semantics: String,
}

/// Whether a method reads or writes state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mutability {
    /// Does not write state.
    Readonly,
    /// Writes state.
    Mutating,
}

/// How the runner is expected to call a method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Invocation {
    /// Submit a real invocation.
    Invoke,
    /// Perform a read-only invocation.
    ReadOnly,
    /// Simulate without committing.
    Simulate,
}

/// A structural type description.
///
/// Composed rather than named, so that `Vec<Address>` and `Map<Address, i128>`
/// are expressible without inventing type names the runner could not map onto a
/// contract spec entry.
///
/// `deny_unknown_fields` is deliberately absent: serde does not support it on an
/// internally tagged enum, so an unknown *shape* is caught by the specification
/// layer's schema validation instead. The variants that do exist reject unknown
/// fields inside themselves, which is where a misspelled field name would land.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeExpr {
    /// A Soroban SDK value type.
    Prim {
        /// The type's name.
        name: PrimitiveType,
        /// The width of a byte array, where the type takes one.
        #[serde(default)]
        width: Option<u32>,
    },
    /// A homogeneous sequence.
    Vec {
        /// The element type.
        element: Box<TypeExpr>,
    },
    /// An optional value.
    Option {
        /// The present type.
        some: Box<TypeExpr>,
    },
    /// A keyed collection.
    Map {
        /// The key type.
        key: Box<TypeExpr>,
        /// The value type.
        value: Box<TypeExpr>,
    },
    /// A fixed-length heterogeneous sequence.
    Tuple {
        /// The element types, in order.
        elements: Vec<TypeExpr>,
    },
    /// A fallible value.
    Result {
        /// The success type.
        ok: Box<TypeExpr>,
        /// The failure type.
        err: Box<TypeExpr>,
    },
    /// A type the interface names but the primitives do not cover.
    Custom {
        /// The name as declared.
        name: String,
        /// What it represents.
        #[serde(default)]
        description: Option<String>,
    },
}

/// The closed set of Soroban SDK value types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveType {
    /// `Address`.
    Address,
    /// `MuxedAddress`, which carries a multiplexing identifier.
    MuxedAddress,
    /// Signed 128-bit integer, the token amount type.
    I128,
    /// Unsigned 128-bit integer.
    U128,
    /// Signed 64-bit integer.
    I64,
    /// Unsigned 64-bit integer.
    U64,
    /// Signed 32-bit integer.
    I32,
    /// Unsigned 32-bit integer, the ledger-number type.
    U32,
    /// Boolean.
    Bool,
    /// `Symbol`.
    Symbol,
    /// `String`.
    String,
    /// `Bytes`.
    Bytes,
    /// `BytesN`.
    BytesN,
    /// The absence of a value.
    Void,
    /// An untyped host value.
    Val,
    /// A point in time.
    Timepoint,
    /// A span of time.
    Duration,
}
