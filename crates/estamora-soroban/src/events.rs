//! Event capture.
//!
//! Events are part of behavioural compatibility: an indexer builds its tables
//! from them, so a transfer that moves the right balances while emitting nothing
//! has broken every consumer downstream. Capturing them correctly is therefore a
//! conformance concern rather than a debugging convenience.
//!
//! # What is captured, and what is not
//!
//! Only events emitted by calls that completed are captured. Soroban rolls back
//! a failed call's effects, including its events, and the SDK's event accessor
//! reflects that by omitting events from failed calls. This matters for the
//! requirement that a refused call must emit nothing: if a refused call's events
//! were captured, a contract that emits before checking its balance would look
//! conformant, and it is precisely the contract the requirement exists to catch.
//!
//! The raw XDR form is converted here so that no crate above this one has to know
//! what XDR is. A conversion that cannot be performed is reported as an execution
//! failure rather than skipped: an event the runner could not read is an event it
//! cannot assert about, and quietly dropping it would let a contract satisfy an
//! event requirement by emitting something unrepresentable.

use estamora_core::Result;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::xdr::{ContractEventBody, ScAddress, ScVal};
use soroban_sdk::{Address, Env, FromVal, TryFromVal, Val, Vec};

use crate::errors::observation_error;

/// One event, as emitted by one contract during one call.
///
/// Not `PartialEq`: the SDK's `Val` is an opaque handle whose equality is not
/// meaningful across hosts, so comparing two captures directly would be a
/// comparison that appears to work and does not. A profile compares an event by
/// reading its topics and payload through the value expressions it declares, and
/// the assertion layer is where that happens.
#[derive(Debug, Clone)]
pub struct CapturedEvent {
    /// The contract that emitted it.
    pub contract: Address,
    /// The event's topics, in order. The first topic is conventionally the event
    /// name in SEP-41.
    pub topics: Vec<Val>,
    /// The event's payload.
    pub data: Val,
}

impl CapturedEvent {
    /// Captures every event `env` has observed so far.
    ///
    /// The result is a host-independent [`std::vec::Vec`]: the crate's callers
    /// are not Soroban value types, and building one forced them through the
    /// host's conversion machinery for no benefit.
    ///
    /// # Errors
    ///
    /// Returns an execution failure if an event's contract identity, topics or
    /// payload cannot be converted into the runner's representation. Refusing the
    /// whole capture rather than skipping the event is deliberate: a partial
    /// capture would report an event requirement as satisfied on incomplete
    /// evidence.
    pub fn capture(env: &Env) -> Result<std::vec::Vec<Self>> {
        let observed = env.events().all();
        let mut captured = std::vec::Vec::with_capacity(observed.events().len());

        for event in observed.events() {
            let contract_id = event
                .contract_id
                .clone()
                .ok_or_else(|| observation_error("an event had no contract identity"))?;
            let contract =
                Address::try_from_val(env, &ScVal::Address(ScAddress::Contract(contract_id)))
                    .map_err(|_| {
                        observation_error("an event's contract identity was not an address")
                    })?;

            let ContractEventBody::V0(body) = &event.body;

            let mut topics = Vec::new(env);
            for topic in &body.topics {
                let value = Val::try_from_val(env, topic)
                    .map_err(|_| observation_error("an event topic could not be read"))?;
                topics.push_back(value);
            }

            let data = Val::try_from_val(env, &body.data)
                .map_err(|_| observation_error("an event payload could not be read"))?;

            captured.push(Self {
                contract,
                topics,
                data,
            });
        }

        Ok(captured)
    }

    /// The number of topics in this event.
    #[must_use]
    pub fn topic_count(&self) -> u32 {
        self.topics.len()
    }

    /// The event's first topic, read as a symbol name when it is one.
    ///
    /// SEP-41 names every event in its first topic, so this is how a profile's
    /// `name` field is matched against an observed event. It returns `None`
    /// rather than an error when the first topic is not a symbol: an event whose
    /// name is not a symbol simply does not match a name-based requirement, and
    /// its topics remain available to a profile that matches positionally.
    #[must_use]
    pub fn name(&self, env: &Env) -> Option<soroban_sdk::Symbol> {
        let first = self.topics.first()?;
        Some(soroban_sdk::Symbol::from_val(env, &first))
    }
}
