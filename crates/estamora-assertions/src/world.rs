//! The world a requirement is evaluated against.
//!
//! The trait is defined in `estamora-core`, because it is the boundary between the
//! evaluator and the execution layer. See [`estamora_core::world`] for what an
//! implementation owes its callers, and in particular why answers are given in the
//! vector's own vocabulary rather than in an address encoding.

pub use estamora_core::world::World;
