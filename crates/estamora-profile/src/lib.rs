//! Loading profile bundles.
//!
//! A profile is the normative half of Estamora and lives in the specification
//! repository. This crate is how the runner reads one: it finds the bundle's
//! entry point, resolves the documents the bundle declares, refuses a format
//! version it cannot execute, parses all six documents into typed structures,
//! checks every cross-reference between them, and hands back something the
//! assertion layer can evaluate against.
//!
//! # What this crate refuses to do
//!
//! It does not decide whether a profile is *good*. Whether a requirement is
//! correct, complete or faithful to the upstream standard is a review question
//! for the specification repository, and a runner that second-guessed it would be
//! running a different profile from the one that was reviewed.
//!
//! What it does decide is whether a profile is *executable*: whether it is
//! well-formed, whether its documents exist inside the bundle and parse, whether
//! its format version is one this runner understands, whether every document
//! refers only to things the profile declares, and whether its identity matches
//! the place it is stored. An executable-looking profile that fails those checks
//! must stop the run rather than produce a verdict, because a verdict produced
//! against requirements that were partly ignored is worse than no verdict at all.
#![forbid(unsafe_code)]

pub mod authorization;
pub mod behavior;
pub mod documents;
pub mod events;
pub mod failures;
pub mod invariants;
pub mod loader;
pub mod references;
pub mod spec;
pub mod types;

pub use authorization::{
    AuthorizationActor, AuthorizationCoverage, AuthorizationDocument, AuthorizationOutcome,
    AuthorizationRule, CoverageMode,
};
pub use behavior::{BehaviorDocument, BehaviorKind, BehaviorRule};
pub use documents::ProfileDocuments;
pub use events::{
    EventBinding, EventCardinality, EventData, EventDataField, EventDataFormat, EventDataResource,
    EventDefinition, EventOccurrence, EventOrdering, EventTopic, EventsDocument,
};
pub use failures::{
    ErrorCodePolicy, ErrorCodeRequirement, EventsEmitted, FailureCategory, FailureDefinition,
    FailureExpectation, FailureOutcome, FailureSignal, FailuresDocument, StateEffect,
};
pub use invariants::{
    InvariantDefinition, InvariantKind, InvariantOutcome, InvariantScope, InvariantSeverity,
    InvariantsDocument, MonotonicDirection,
};
pub use loader::{BUNDLE_ENTRY_POINT, MAX_DOCUMENT_BYTES, ProfileBundle, ProfileReference};
pub use spec::{SUPPORTED_SPEC_VERSION, SpecVersion};
pub use types::{
    ArgumentAuthorization, BundleManifest, Compatibility, InterfaceCoverage, Invocation,
    MethodArgument, MethodDefinition, MethodReturn, MethodsDocument, Mutability, PrimitiveType,
    ProfileDocument, ProfileMetadata, ProfileStatus, Provenance, ProvenanceSource,
    RequirementStatus, SpecificationReference, TypeExpr, UpstreamStatus,
};

#[cfg(test)]
mod tests;
