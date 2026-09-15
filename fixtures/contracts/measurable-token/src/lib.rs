//! A SEP-0041 token that exists to be measured as a **compiled artifact**.
//!
//! The other fixture contracts under `fixtures/contracts/` are registered from their Rust
//! types, which is what lets the test suite run with no build step. That route cannot
//! exercise the runner's `.wasm` target at all: there is no artifact to read an interface
//! out of, so interface inspection from a real `contractspecv0` section, artifact
//! deployment, and the digest a report records for a deployed artifact all go untested.
//!
//! This contract is therefore built to WebAssembly by `scripts/build-fixture-wasm.sh` and
//! the artifact is checked in beside it, so a test can measure an artifact without a
//! build step and without a network.
//!
//! # Why it is outside the workspace
//!
//! `soroban-sdk`'s `testutils` feature is enabled once in the runner's workspace and
//! inherited by every member, which is right for the runner and fatal for a contract:
//! `testutils` does not compile for WebAssembly. A contract in that workspace could never
//! be built, which is the whole reason this crate declares its own `[workspace]`.
//!
//! # What it deliberately does not do
//!
//! It has **no constructor**, and its balances start empty. A vector cannot seed a
//! deployed artifact — that is the documented limit of the `.wasm` target — so a
//! constructor that configured anything would only be reachable by a caller this runner
//! does not have. Everything a vector can actually ask of it is answered from an empty
//! token: `decimals`, `name` and `symbol` are constants, and every balance read is zero.
//!
//! # Why the signatures are exactly SEP-0041's
//!
//! This artifact exists to be measured against the `sep-41` profile, so it has to publish
//! the interface that profile declares — and the interface a contract publishes is the
//! one written into its `contractspecv0` section, not the one its source appears to
//! have.
//!
//! Two details of that encoding are easy to get wrong, and both were:
//!
//! * `approve`, `transfer`, `transfer_from`, `burn` and `burn_from` are declared by
//!   SEP-0041 as returning **nothing**. A signature returning `Result<(), Error>` — the
//!   first spelling this fixture used — publishes `result<void,error>`, which is a
//!   different interface and one a caller written against SEP-0041 cannot decode. A
//!   refusal therefore has to be a trap carrying the error, which is what the host
//!   records and what this contract now does.
//! * `name` and `symbol` return `String`. Importing it under an alias, as
//!   `String as SorobanString`, makes the SDK's spec generator fail to recognise it and
//!   emit a user-defined type named after the alias instead — so the artifact published
//!   `sorobanstring` and every token built that way would have been measured as
//!   publishing a type of its own invention. `String` is imported under its own name for
//!   that reason, and the interface test in this crate is what keeps it that way.
#![no_std]
// The signatures below take `Env` and `Address` by value because the Soroban contract ABI
// requires it: an exported function's parameters *are* the wire format of the call, and
// the host constructs them from the arguments it received. Clippy's advice to borrow them
// would produce a function the host cannot call, so the lint is disabled here with that
// reason rather than worked around with a signature that would change the interface.
#![allow(
    clippy::needless_pass_by_value,
    clippy::must_use_candidate,
    reason = "contract function signatures are the Soroban ABI, not a choice"
)]

use soroban_sdk::{
    Address, Env, MuxedAddress, String, contract, contracterror, contractimpl, contracttype,
    panic_with_error,
};

/// The precision this token is configured with.
///
/// A positive number, because SEP-0041 defines it as the number of decimal places and a
/// token that reported zero would be indistinguishable from one whose metadata read
/// failed.
const DECIMALS: u32 = 7;

/// What went wrong, in the terms SEP-0041 defines.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// The transferor does not hold enough.
    InsufficientBalance = 1,
    /// The allowance is smaller than the amount drawn against it.
    InsufficientAllowance = 2,
    /// An amount was not positive.
    InvalidAmount = 3,
}

/// Where a balance or an allowance lives.
#[contracttype]
pub enum DataKey {
    /// One holder's balance.
    Balance(Address),
    /// One spender's allowance from one holder, with the ledger it is live until.
    Allowance(Address, Address),
}

#[contract]
pub struct MeasurableToken;

#[contractimpl]
impl MeasurableToken {
    /// The remaining allowance, or zero when there is none or it has expired.
    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        Self::live_allowance(&env, &from, &spender).0
    }

    /// Sets `spender`'s allowance to `amount`, live until `live_until_ledger`.
    ///
    /// # Panics
    ///
    /// Panics with [`Error::InvalidAmount`] for a negative amount. A zero amount is not
    /// an error: revoking an allowance is a legitimate thing to ask for.
    pub fn approve(
        env: Env,
        from: Address,
        spender: Address,
        amount: i128,
        live_until_ledger: u32,
    ) {
        from.require_auth();
        if amount < 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        env.storage().temporary().set(
            &DataKey::Allowance(from, spender),
            &(amount, live_until_ledger),
        );
    }

    /// The balance held by `id`, which is zero for an address with no balance entry.
    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(id))
            .unwrap_or(0)
    }

    /// Moves `amount` from `from` to `to`.
    ///
    /// # Panics
    ///
    /// Panics with [`Error::InsufficientBalance`] when `from` does not hold `amount`, and
    /// with [`Error::InvalidAmount`] for a negative one.
    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        Self::demanding(&env, Self::move_value(&env, &from, &to.address(), amount));
    }

    /// Moves `amount` from `from` to `to`, drawing on `spender`'s allowance.
    ///
    /// # Panics
    ///
    /// Panics with [`Error::InsufficientAllowance`] when the live allowance is smaller
    /// than `amount`, and with [`Error::InsufficientBalance`] when the holder does not
    /// have it.
    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();
        let (allowed, live_until) = Self::live_allowance(&env, &from, &spender);
        if allowed < amount {
            panic_with_error!(&env, Error::InsufficientAllowance);
        }
        Self::demanding(&env, Self::move_value(&env, &from, &to, amount));
        env.storage().temporary().set(
            &DataKey::Allowance(from, spender),
            &(allowed - amount, live_until),
        );
    }

    /// Destroys `amount` of `from`'s balance.
    ///
    /// # Panics
    ///
    /// Panics with [`Error::InsufficientBalance`] when the holder does not hold `amount`.
    pub fn burn(env: Env, from: Address, amount: i128) {
        from.require_auth();
        let held = Self::balance(env.clone(), from.clone());
        if held < amount {
            panic_with_error!(&env, Error::InsufficientBalance);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from), &(held - amount));
    }

    /// Destroys `amount` of `from`'s balance, drawing on `spender`'s allowance.
    ///
    /// # Panics
    ///
    /// Panics with [`Error::InsufficientAllowance`] when the live allowance is smaller
    /// than `amount`, and with [`Error::InsufficientBalance`] when the holder does not
    /// have it.
    pub fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
        spender.require_auth();
        let (allowed, live_until) = Self::live_allowance(&env, &from, &spender);
        if allowed < amount {
            panic_with_error!(&env, Error::InsufficientAllowance);
        }
        let held = Self::balance(env.clone(), from.clone());
        if held < amount {
            panic_with_error!(&env, Error::InsufficientBalance);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &(held - amount));
        env.storage().temporary().set(
            &DataKey::Allowance(from, spender),
            &(allowed - amount, live_until),
        );
    }

    /// The precision this token reports.
    pub fn decimals(_env: Env) -> u32 {
        DECIMALS
    }

    /// This token's name.
    pub fn name(env: Env) -> String {
        String::from_str(&env, "Measurable Token")
    }

    /// This token's symbol.
    pub fn symbol(env: Env) -> String {
        String::from_str(&env, "MST")
    }

    /// The allowance that is still live at the current ledger.
    ///
    /// An expired allowance reads as zero rather than as the amount it was granted,
    /// because a spender that could still draw on an expired allowance would be spending
    /// against a permission that had been withdrawn.
    fn live_allowance(env: &Env, from: &Address, spender: &Address) -> (i128, u32) {
        let granted: Option<(i128, u32)> = env
            .storage()
            .temporary()
            .get(&DataKey::Allowance(from.clone(), spender.clone()));
        match granted {
            Some((amount, live_until)) if live_until >= env.ledger().sequence() => {
                (amount, live_until)
            },
            Some((_, live_until)) => (0, live_until),
            None => (0, 0),
        }
    }

    /// A refusal, as SEP-0041 requires one to be made.
    ///
    /// The standard declares these methods as returning nothing, so a failure cannot be
    /// reported in a return value: it has to be a trap carrying the error, which is what
    /// the host records and what a caller observes. `move_value` returns its outcome
    /// because a private helper may; the exported functions cannot.
    fn demanding<T>(env: &Env, outcome: Result<T, Error>) -> T {
        match outcome {
            Ok(value) => value,
            Err(error) => panic_with_error!(env, error),
        }
    }

    /// Moves value, refusing an amount the transferor does not hold.
    fn move_value(env: &Env, from: &Address, to: &Address, amount: i128) -> Result<(), Error> {
        if amount < 0 {
            return Err(Error::InvalidAmount);
        }
        let sender = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(from.clone()))
            .unwrap_or(0);
        if sender < amount {
            return Err(Error::InsufficientBalance);
        }
        let receiver: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &(sender - amount));
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to.clone()), &(receiver + amount));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};

    #[test]
    fn an_address_with_no_balance_entry_reads_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = env.register(MeasurableToken, ());
        let client = MeasurableTokenClient::new(&env, &contract);
        let holder = Address::generate(&env);

        assert_eq!(client.balance(&holder), 0);
    }

    #[test]
    fn the_metadata_is_stable() {
        let env = Env::default();
        let contract = env.register(MeasurableToken, ());
        let client = MeasurableTokenClient::new(&env, &contract);

        assert_eq!(client.decimals(), DECIMALS);
        assert_eq!(client.name(), String::from_str(&env, "Measurable Token"));
        assert_eq!(client.symbol(), String::from_str(&env, "MST"));
    }

    #[test]
    fn a_refusal_is_a_trap_carrying_the_error_not_a_returned_value() {
        // The change this asserts is invisible in the Rust signature and visible in the
        // published one: `Result<(), Error>` would publish `result<void,error>`, which is
        // not the interface SEP-0041 declares. The error still has to reach the caller,
        // so the code is what identifies the refusal.
        let env = Env::default();
        env.mock_all_auths();
        let contract = env.register(MeasurableToken, ());
        let client = MeasurableTokenClient::new(&env, &contract);
        let holder = Address::generate(&env);

        // The refusal arrives as a host error carrying this contract's code, which is the
        // route a caller has once the declared interface stops reporting one in a value.
        assert_eq!(
            client.try_burn(&holder, &1),
            Err(Ok(Error::InsufficientBalance.into())),
            "a refusal must carry the error SEP-0041 defines for it"
        );
        assert_eq!(
            client.try_approve(&holder, &holder, &-1, &10),
            Err(Ok(Error::InvalidAmount.into()))
        );
    }

    #[test]
    fn an_expired_allowance_reads_zero() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_sequence_number(1_000);
        let contract = env.register(MeasurableToken, ());
        let client = MeasurableTokenClient::new(&env, &contract);
        let holder = Address::generate(&env);
        let spender = Address::generate(&env);

        client.approve(&holder, &spender, &500, &1_050);
        assert_eq!(client.allowance(&holder, &spender), 500);

        env.ledger().set_sequence_number(1_051);
        assert_eq!(
            client.allowance(&holder, &spender),
            0,
            "an allowance past its last live ledger must read as zero"
        );
    }
}
