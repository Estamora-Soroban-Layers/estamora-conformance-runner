//! The six documents a bundle is composed of, together.
//!
//! A bundle is not one document. It is a profile entry point, a method manifest
//! and five further documents that reference each other by identifier, and the
//! references are what make the whole thing equal to more than the sum of its
//! parts: a behavioural rule names the failures it expects, which name the methods
//! they are reachable from, which name the authorization rules that govern them.
//!
//! Holding them together is therefore not a convenience. A document cannot be
//! validated on its own, because almost every defect worth catching is a broken
//! reference *between* two of them — and a runner that loaded one file at a time
//! would discover the break only when a vector reached the point of needing it,
//! which is after the run has already begun acting on the profile.

use crate::authorization::AuthorizationDocument;
use crate::behavior::BehaviorDocument;
use crate::events::EventsDocument;
use crate::failures::FailuresDocument;
use crate::invariants::InvariantsDocument;
use crate::references;
use crate::types::MethodsDocument;
use estamora_core::Diagnostics;

/// Every document a bundle declares, parsed.
#[derive(Debug, Clone)]
pub struct ProfileDocuments {
    /// The method requirements.
    pub methods: MethodsDocument,
    /// The authorization requirements.
    pub authorization: AuthorizationDocument,
    /// The event requirements.
    pub events: EventsDocument,
    /// The behavioural rules.
    pub behavior: BehaviorDocument,
    /// The invariants.
    pub invariants: InvariantsDocument,
    /// The failure requirements.
    pub failures: FailuresDocument,
}

impl ProfileDocuments {
    /// Every defect that would make this profile unexecutable.
    ///
    /// Returns findings rather than stopping at the first, so that one run tells
    /// a contributor everything that is wrong instead of one thing per attempt.
    /// A caller decides what to do with them: [`crate::ProfileBundle::load`]
    /// refuses the bundle when any finding is an error, and keeps the warnings.
    #[must_use]
    pub fn validate(&self) -> Diagnostics {
        references::validate(self)
    }

    /// The method requirement with the given id, if the profile declares one.
    #[must_use]
    pub fn method(&self, id: &str) -> Option<&crate::types::MethodDefinition> {
        self.methods.methods.iter().find(|method| method.id == id)
    }

    /// The invariant with the given id, if the profile declares one.
    #[must_use]
    pub fn invariant(&self, id: &str) -> Option<&crate::invariants::InvariantDefinition> {
        self.invariants
            .invariants
            .iter()
            .find(|invariant| invariant.id == id)
    }

    /// The failure requirement with the given id, if the profile declares one.
    #[must_use]
    pub fn failure(&self, id: &str) -> Option<&crate::failures::FailureDefinition> {
        self.failures
            .failures
            .iter()
            .find(|failure| failure.id == id)
    }

    /// The event requirement with the given id, if the profile declares one.
    #[must_use]
    pub fn event(&self, id: &str) -> Option<&crate::events::EventDefinition> {
        self.events.events.iter().find(|event| event.id == id)
    }

    /// The invariants whose scope covers `method` and `outcome`.
    ///
    /// This is the query the assertion layer makes, and it is answered here
    /// rather than there so that the wildcard rule is stated once. An invariant
    /// scoped to `*` covers every method the profile declares; scoping it to a
    /// literal `*` is not the same as leaving it unmapped, which the format does
    /// not permit.
    #[must_use]
    pub fn invariants_for(
        &self,
        method: &str,
        outcome: crate::invariants::InvariantOutcome,
    ) -> Vec<&crate::invariants::InvariantDefinition> {
        self.invariants
            .invariants
            .iter()
            .filter(|invariant| {
                invariant.scope.outcomes.contains(&outcome)
                    && invariant
                        .scope
                        .methods
                        .iter()
                        .any(|scoped| scoped == "*" || scoped == method)
            })
            .collect()
    }

    /// The behavioural rules that govern `method`.
    #[must_use]
    pub fn behaviors_for(&self, method: &str) -> Vec<&crate::behavior::BehaviorRule> {
        self.behavior
            .behaviors
            .iter()
            .filter(|rule| rule.method == method)
            .collect()
    }
}
