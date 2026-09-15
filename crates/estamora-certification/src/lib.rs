//! Digests, receipts and receipt verification.
//!
//! A conformance result is only useful to someone who was not present at the run if
//! they can check that the report shown to them is the one the result is about. This
//! crate provides the smallest artefact that makes that checkable: a receipt naming
//! the contract, the network, the pinned profile and corpus, the verdict, and the
//! digest of the report — optionally signed.
//!
//! # Two questions, two answers
//!
//! [`verify`] answers *is this report the one the receipt is about* and *who asserted
//! it* separately, and the second is `Unattributed` unless the caller supplies a key
//! it already trusts. A signature checked against a key carried in the same document
//! establishes only that the document agrees with itself, and presenting that as
//! verification would be the most misleading artefact this project could publish.
//!
//! # What this crate does not do
//!
//! It does not publish anything on a ledger, and it does not implement an on-chain
//! certification contract. Estamora's specification layer defines the normative data
//! model; the runner produces and verifies a receipt. Making the open-source runner
//! depend on infrastructure the project does not own would also invite reading an
//! on-chain record as a stronger claim than this document supports.
//!
//! It also does not encrypt anything. Signatures record *who asserted a result*; they
//! say nothing about whether the result is correct, and a valid signature over a
//! wrong conclusion is still a wrong conclusion.
#![forbid(unsafe_code)]

pub mod digest;
pub mod receipt;
pub mod verification;

pub use digest::{Digest, HEX_LENGTH, PREFIX};
pub use receipt::{
    Receipt, ReceiptCorpus, ReceiptProfile, ReceiptResult, ReceiptTarget, receipt_digest,
    report_digest,
};
pub use verification::{
    ALGORITHM, Attribution, SignedReceipt, Verification, fingerprint, sign, signing_key_from_hex,
    verify, verifying_key_from_base64,
};

#[cfg(test)]
mod tests;
