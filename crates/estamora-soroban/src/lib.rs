//! Soroban execution.
//!
//! The only crate in the workspace that knows what a ledger is. Everything above
//! it consumes an [`Invocation`]: what a call returned, whether it was accepted,
//! what it emitted, and what authorization it required.
//!
//! # The distinction this crate exists to keep
//!
//! A contract call can end in four ways, and three of them are routinely
//! conflated:
//!
//! | Ending | What it means | Who is at fault |
//! | ------ | ------------- | --------------- |
//! | returns a value | The call was accepted | Nobody |
//! | refuses the call | The contract rejected it deliberately | Nobody — this may be required |
//! | cannot be evaluated | The environment failed | The environment |
//! | method does not exist | The interface is not what was claimed | The contract |
//!
//! The last two are the reason the interface layer exists and runs *before* the
//! behavioural vectors. Soroban reports a missing method and a deliberate
//! `panic!()` through the same channel, so a call that failed is not by itself
//! evidence of a refusal; only a contract whose interface has been checked can
//! have its failures read as refusals. See [`invoker`] for how the ambiguity is
//! handled and why neither branch is guessed at.

pub mod auth;
pub mod errors;
pub mod events;
pub mod host;
pub mod inspect;
pub mod invoker;
pub mod world;

pub use auth::{AuthorizationMode, AuthorizationRecord, apply_authorizations};
pub use events::CapturedEvent;
pub use host::{DEFAULT_LEDGER_SEQUENCE, DEFAULT_LEDGER_TIMESTAMP, LedgerPoint, LocalHost};
pub use inspect::{
    DeclaredMethod, ExposedInterface, ExposedMethod, ExposedParameter, SPEC_SECTION,
};
pub use invoker::{CallOutcome, Invocation, invoke};
pub use world::{Actor, AllowanceGrant, ContractWorld};

#[cfg(test)]
mod tests;
