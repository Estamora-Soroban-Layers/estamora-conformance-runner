//! The values a requirement is evaluated against.
//!
//! The types are defined in `estamora-core`, because the observed vocabulary is the
//! boundary the execution layer and the evaluator must agree on. They are re-exported
//! here so that every path inside this crate keeps reading as though the evaluator
//! owned them, which it does conceptually.
//!
//! See [`estamora_core::value`] for why comparison is not equality.

pub use estamora_core::value::{Comparison, Value};
