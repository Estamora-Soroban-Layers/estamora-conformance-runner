//! A deterministic local execution environment.
//!
//! Every fixture a profile declares names a ledger point — a sequence number and
//! a timestamp — because a vector whose outcome depends on the wall clock cannot
//! be reproduced, and a result nobody else can reproduce is not evidence. This
//! module is what turns a declared fixture into a host that honours it.

use soroban_sdk::testutils::{Ledger as _, Register};
use soroban_sdk::{Address, Env};

/// The ledger sequence a run uses when a fixture does not name one.
///
/// Any fixed value would do; this one is chosen to be large enough that TTL
/// arithmetic in a fixture does not short-circuit, and the choice is recorded so
/// that two runs of the same fixture agree.
pub const DEFAULT_LEDGER_SEQUENCE: u32 = 1_000;

/// The ledger timestamp a run uses when a fixture does not name one:
/// `2026-01-15T12:00:00Z`, matching the instant the specification's own example
/// vectors declare.
pub const DEFAULT_LEDGER_TIMESTAMP: u64 = 1_768_478_400;

/// A point in ledger time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedgerPoint {
    /// The ledger sequence number the fixture runs at.
    pub sequence: u32,
    /// The ledger close time, in seconds since the Unix epoch.
    pub timestamp: u64,
}

impl Default for LedgerPoint {
    fn default() -> Self {
        Self {
            sequence: DEFAULT_LEDGER_SEQUENCE,
            timestamp: DEFAULT_LEDGER_TIMESTAMP,
        }
    }
}

impl LedgerPoint {
    /// A point at `sequence` and `timestamp`.
    #[must_use]
    pub const fn new(sequence: u32, timestamp: u64) -> Self {
        Self {
            sequence,
            timestamp,
        }
    }
}

/// A local execution environment, pinned to one ledger point.
#[derive(Clone)]
pub struct LocalHost {
    env: Env,
    point: LedgerPoint,
}

impl core::fmt::Debug for LocalHost {
    /// Prints the ledger point and the environment's identity, never the host's
    /// internal state.
    ///
    /// `Env` has no usable `Debug`, and deriving one from it would put a
    /// handle's address into every diagnostic — a value that differs between two
    /// runs of the same fixture, which is the opposite of what this runner
    /// produces anywhere else.
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("LocalHost")
            .field("sequence", &self.point.sequence)
            .field("timestamp", &self.point.timestamp)
            .finish_non_exhaustive()
    }
}

impl LocalHost {
    /// Builds a host pinned to `point`.
    ///
    /// The ledger is set before any contract is registered, so a contract whose
    /// constructor reads the ledger sees the fixture's values rather than the
    /// host defaults.
    #[must_use]
    pub fn new(point: LedgerPoint) -> Self {
        let env = Env::default();
        env.ledger().set_sequence_number(point.sequence);
        env.ledger().set_timestamp(point.timestamp);
        Self { env, point }
    }

    /// Builds a host at the default ledger point.
    #[must_use]
    pub fn at_default_point() -> Self {
        Self::new(LedgerPoint::default())
    }

    /// The underlying environment.
    #[must_use]
    pub fn env(&self) -> &Env {
        &self.env
    }

    /// The ledger point this host is pinned to.
    #[must_use]
    pub const fn point(&self) -> LedgerPoint {
        self.point
    }

    /// Registers a contract and returns its address.
    ///
    /// The parameter is generic over the SDK's `Register` trait, which is
    /// implemented both by a contract type defined in a fixture and by the bytes
    /// of a compiled contract. That is deliberate: the same code path deploys an
    /// in-repository fixture and a user's WebAssembly, so a fixture cannot
    /// exercise a route the real deployment does not take.
    pub fn register(&self, contract: impl Register) -> Address {
        contract.register(&self.env, None, ())
    }
}
