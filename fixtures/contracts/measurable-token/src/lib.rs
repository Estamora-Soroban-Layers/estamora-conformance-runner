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
//! # Why each storage key is built once
//!
//! An entry point that reads an entry and then writes the same entry builds that entry's key
//! once and reuses it. The duplicate construction this replaces is not itself a host call, so
//! it costs no resource fee — costing every entry point in the local host before and after
//! gives identical instruction counts — but it *is* code in the compiled contract, and the
//! size of the compiled contract is what a deployment writes to the ledger. Removing it leaves
//! the artifact 115 bytes smaller, 11,324 to 11,209.
//!
//! The two things are worth keeping apart: what a *call* pays for is the storage reads and
//! writes, which the operation's arithmetic fixes and no amount of tidying moves; what a
//! *deployment* pays for is the code that performs them, which tidying does move.
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
    /// An amount was negative, so applying it would move value in the wrong direction.
    ///
    /// Zero is not this error. Revoking an allowance, or burning nothing, is a legitimate
    /// thing to ask for, and SEP-0041 defines no refusal for it.
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
        Self::refuse_invalid_amount(&env, amount);
        env.storage().temporary().set(
            &DataKey::Allowance(from, spender),
            &(amount, live_until_ledger),
        );
    }

    /// The balance held by `id`, which is zero for an address with no balance entry.
    pub fn balance(env: Env, id: Address) -> i128 {
        Self::held(&env, &DataKey::Balance(id))
    }

    /// The balance stored under an already-built key.
    ///
    /// Split from [`MeasurableToken::balance`] so that an entry point which reads and then
    /// writes the same entry builds its key once. See the module documentation.
    fn held(env: &Env, key: &DataKey) -> i128 {
        env.storage().persistent().get(key).unwrap_or(0)
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
        // One key, built once: this call reads the entry and then rewrites it.
        let allowance_key = DataKey::Allowance(from.clone(), spender);
        let (allowed, live_until) = Self::live_allowance_at(&env, &allowance_key);
        if allowed < amount {
            panic_with_error!(&env, Error::InsufficientAllowance);
        }
        Self::demanding(&env, Self::move_value(&env, &from, &to, amount));
        env.storage()
            .temporary()
            .set(&allowance_key, &(allowed - amount, live_until));
    }

    /// Destroys `amount` of `from`'s balance.
    ///
    /// # Panics
    ///
    /// Panics with [`Error::InvalidAmount`] for a negative amount, and with
    /// [`Error::InsufficientBalance`] when the holder does not hold `amount`.
    pub fn burn(env: Env, from: Address, amount: i128) {
        from.require_auth();
        Self::refuse_invalid_amount(&env, amount);
        let balance_key = DataKey::Balance(from);
        let held = Self::held(&env, &balance_key);
        if held < amount {
            panic_with_error!(&env, Error::InsufficientBalance);
        }
        env.storage()
            .persistent()
            .set(&balance_key, &(held - amount));
    }

    /// Destroys `amount` of `from`'s balance, drawing on `spender`'s allowance.
    ///
    /// # Panics
    ///
    /// Panics with [`Error::InvalidAmount`] for a negative amount, with
    /// [`Error::InsufficientAllowance`] when the live allowance is smaller than `amount`,
    /// and with [`Error::InsufficientBalance`] when the holder does not have it.
    pub fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
        spender.require_auth();
        Self::refuse_invalid_amount(&env, amount);
        // Both entries are touched twice, so both keys are built once.
        let allowance_key = DataKey::Allowance(from.clone(), spender);
        let (allowed, live_until) = Self::live_allowance_at(&env, &allowance_key);
        if allowed < amount {
            panic_with_error!(&env, Error::InsufficientAllowance);
        }
        let balance_key = DataKey::Balance(from);
        let held = Self::held(&env, &balance_key);
        if held < amount {
            panic_with_error!(&env, Error::InsufficientBalance);
        }
        env.storage()
            .persistent()
            .set(&balance_key, &(held - amount));
        env.storage()
            .temporary()
            .set(&allowance_key, &(allowed - amount, live_until));
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
        Self::live_allowance_at(env, &DataKey::Allowance(from.clone(), spender.clone()))
    }

    /// The allowance stored under an already-built key, and the ledger it is live until.
    ///
    /// Split from [`MeasurableToken::live_allowance`] so that an entry point which reads and
    /// then rewrites the same grant builds its key once.
    fn live_allowance_at(env: &Env, key: &DataKey) -> (i128, u32) {
        let granted: Option<(i128, u32)> = env.storage().temporary().get(key);
        match granted {
            Some((amount, live_until)) if live_until >= env.ledger().sequence() => {
                (amount, live_until)
            },
            Some((_, live_until)) => (0, live_until),
            None => (0, 0),
        }
    }

    /// Whether `amount` is one this token will act on.
    ///
    /// The rule is stated **once**, here, and every entry point that takes an amount asks
    /// this rather than repeating the comparison. It used to be written out in three
    /// places and omitted from two of them: `approve` refused a negative amount inline,
    /// and `move_value` refused it for `transfer` and `transfer_from`, while `burn` and
    /// `burn_from` refused nothing at all. A negative amount does not merely travel the
    /// other way — it *creates* value, because `held - amount` is larger than `held` when
    /// `amount` is negative, and the same arithmetic on an allowance hands a spender a
    /// grant out of nothing. One shared predicate is what makes that omission
    /// unrepeatable: a method added later either asks this function or fails to compile.
    ///
    /// Zero is valid: revoking an allowance, or burning nothing, is a legitimate thing to
    /// ask for, and this token has no refusal for it.
    fn is_valid_amount(amount: i128) -> bool {
        amount >= 0
    }

    /// Refuses an amount this token will not act on.
    ///
    /// An entry point whose signature is SEP-0041's returns nothing, so a refusal cannot
    /// be reported in a return value: it has to be a trap carrying the error, which is
    /// what the host records and what a caller observes. `move_value` reports the same
    /// refusal through its `Result` because a private helper may; the exported functions
    /// cannot, which is why both routes ask [`MeasurableToken::is_valid_amount`] instead of
    /// each deciding for itself.
    fn refuse_invalid_amount(env: &Env, amount: i128) {
        if !Self::is_valid_amount(amount) {
            panic_with_error!(env, Error::InvalidAmount);
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
        if !Self::is_valid_amount(amount) {
            return Err(Error::InvalidAmount);
        }
        // Two entries, two keys, built once each: the reads and the writes below name the
        // same pair.
        let from_key = DataKey::Balance(from.clone());
        let to_key = DataKey::Balance(to.clone());
        let sender = Self::held(env, &from_key);
        if sender < amount {
            return Err(Error::InsufficientBalance);
        }
        let receiver = Self::held(env, &to_key);
        env.storage()
            .persistent()
            .set(&from_key, &(sender - amount));
        env.storage()
            .persistent()
            .set(&to_key, &(receiver + amount));
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

    /// A negative amount must be refused by every entry point that takes one, and the
    /// refusal must leave the ledger exactly as it found it.
    ///
    /// This is a regression test with a measured history, not a guess about what might
    /// break. `burn` and `burn_from` used to refuse nothing, and the arithmetic that
    /// followed did not move value the other way — it *created* it:
    ///
    /// ```text
    /// burn(holder, -1_000)                  balance 0 -> 1_000, and the call SUCCEEDED
    /// burn_from(spender, holder, -2_000)    balance 0 -> 2_000
    ///                                       allowance 0 -> 2_000
    /// ```
    ///
    /// Two things let that survive a suite that was otherwise green. The rule was
    /// duplicated rather than shared, so `approve` and `move_value` had it while the two
    /// `burn` entry points did not; and this module's only negative-amount assertion was
    /// aimed at `approve` — the one method that already had the guard — so the hole was
    /// never asked about. `burn_from` was worse than `burn`: its allowance check is
    /// `allowed < amount`, which a negative amount passes too, so a spender holding no
    /// approval at all could credit any address it named *and* be granted an allowance on
    /// it, in one call, from nothing.
    ///
    /// Every assertion below is made against empty state on purpose. This token has no
    /// mint, so an empty token is the only world a vector can reach — and it is also where
    /// the inversion was largest, zero to twenty thousand.
    #[test]
    fn a_negative_amount_is_refused_by_every_entry_point_and_changes_nothing() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_sequence_number(1_000);
        let contract = env.register(MeasurableToken, ());
        let client = MeasurableTokenClient::new(&env, &contract);
        let holder = Address::generate(&env);
        let spender = Address::generate(&env);
        let recipient = Address::generate(&env);
        let muxed = MuxedAddress::from(recipient.clone());

        assert_eq!(
            client.try_burn(&holder, &-1_000),
            Err(Ok(Error::InvalidAmount.into())),
            "burn must refuse a negative amount rather than credit the holder"
        );
        assert_eq!(
            client.try_burn_from(&spender, &holder, &-2_000),
            Err(Ok(Error::InvalidAmount.into())),
            "burn_from must refuse a negative amount rather than credit the holder"
        );
        assert_eq!(
            client.try_transfer(&holder, &muxed, &-1),
            Err(Ok(Error::InvalidAmount.into()))
        );
        assert_eq!(
            client.try_transfer_from(&spender, &holder, &recipient, &-1),
            Err(Ok(Error::InvalidAmount.into()))
        );
        assert_eq!(
            client.try_approve(&holder, &spender, &-1, &10),
            Err(Ok(Error::InvalidAmount.into()))
        );

        // Refusing is only half the claim. A trap that had already written state would
        // still be a way to mint, so the ledger has to be unchanged afterwards — and the
        // allowance assertion is the one that catches `burn_from`, where a *successful*
        // call was not even required to mint: the check that was meant to stop it passed
        // a negative amount straight through and then widened the grant.
        assert_eq!(
            client.balance(&holder),
            0,
            "a refused burn must not create a balance"
        );
        assert_eq!(client.balance(&recipient), 0);
        assert_eq!(
            client.allowance(&holder, &spender),
            0,
            "a refused burn_from must not grant the spender an allowance"
        );
    }

    /// Zero is a valid amount, and the boundary has to stay exactly where it is.
    ///
    /// [`MeasurableToken::is_valid_amount`] refuses a negative amount and accepts zero.
    /// Tightening it to "positive" would read as stricter and would silently break
    /// revocation — approving zero is how an allowance is withdrawn — while fixing nothing
    /// about the inversion the guard exists for, because zero cannot invert anything. This
    /// test is what makes that a decision rather than an accident.
    #[test]
    fn a_zero_amount_is_accepted_so_an_allowance_can_be_revoked() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_sequence_number(1_000);
        let contract = env.register(MeasurableToken, ());
        let client = MeasurableTokenClient::new(&env, &contract);
        let holder = Address::generate(&env);
        let spender = Address::generate(&env);

        client.approve(&holder, &spender, &500, &2_000);
        assert_eq!(client.allowance(&holder, &spender), 500);

        client.approve(&holder, &spender, &0, &2_000);
        assert_eq!(
            client.allowance(&holder, &spender),
            0,
            "approving zero must revoke rather than refuse"
        );

        // Burning nothing is likewise not an error, and must not fail for a holder whose
        // balance is already zero.
        client.burn(&holder, &0);
        assert_eq!(client.balance(&holder), 0);
    }
}
