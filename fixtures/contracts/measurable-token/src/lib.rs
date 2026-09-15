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
    Address, Env, MuxedAddress, String as SorobanString, contract, contracterror, contractimpl,
    contracttype,
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
    /// # Errors
    ///
    /// Returns [`Error::InvalidAmount`] for a negative amount. A zero amount is not an
    /// error: revoking an allowance is a legitimate thing to ask for.
    pub fn approve(
        env: Env,
        from: Address,
        spender: Address,
        amount: i128,
        live_until_ledger: u32,
    ) -> Result<(), Error> {
        from.require_auth();
        if amount < 0 {
            return Err(Error::InvalidAmount);
        }
        env.storage().temporary().set(
            &DataKey::Allowance(from, spender),
            &(amount, live_until_ledger),
        );
        Ok(())
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
    /// # Errors
    ///
    /// Returns [`Error::InsufficientBalance`] when `from` does not hold `amount`, and
    /// [`Error::InvalidAmount`] for a negative one.
    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) -> Result<(), Error> {
        from.require_auth();
        Self::move_value(&env, &from, &to.address(), amount)
    }

    /// Moves `amount` from `from` to `to`, drawing on `spender`'s allowance.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InsufficientAllowance`] when the live allowance is smaller than
    /// `amount`, and [`Error::InsufficientBalance`] when the holder does not have it.
    pub fn transfer_from(
        env: Env,
        spender: Address,
        from: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), Error> {
        spender.require_auth();
        let (allowed, live_until) = Self::live_allowance(&env, &from, &spender);
        if allowed < amount {
            return Err(Error::InsufficientAllowance);
        }
        Self::move_value(&env, &from, &to, amount)?;
        env.storage().temporary().set(
            &DataKey::Allowance(from, spender),
            &(allowed - amount, live_until),
        );
        Ok(())
    }

    /// Destroys `amount` of `from`'s balance.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InsufficientBalance`] when the holder does not hold `amount`.
    pub fn burn(env: Env, from: Address, amount: i128) -> Result<(), Error> {
        from.require_auth();
        let held = Self::balance(env.clone(), from.clone());
        if held < amount {
            return Err(Error::InsufficientBalance);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from), &(held - amount));
        Ok(())
    }

    /// Destroys `amount` of `from`'s balance, drawing on `spender`'s allowance.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InsufficientAllowance`] when the live allowance is smaller than
    /// `amount`, and [`Error::InsufficientBalance`] when the holder does not have it.
    pub fn burn_from(env: Env, spender: Address, from: Address, amount: i128) -> Result<(), Error> {
        spender.require_auth();
        let (allowed, live_until) = Self::live_allowance(&env, &from, &spender);
        if allowed < amount {
            return Err(Error::InsufficientAllowance);
        }
        let held = Self::balance(env.clone(), from.clone());
        if held < amount {
            return Err(Error::InsufficientBalance);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &(held - amount));
        env.storage().temporary().set(
            &DataKey::Allowance(from, spender),
            &(allowed - amount, live_until),
        );
        Ok(())
    }

    /// The precision this token reports.
    pub fn decimals(_env: Env) -> u32 {
        DECIMALS
    }

    /// This token's name.
    pub fn name(env: Env) -> SorobanString {
        SorobanString::from_str(&env, "Measurable Token")
    }

    /// This token's symbol.
    pub fn symbol(env: Env) -> SorobanString {
        SorobanString::from_str(&env, "MST")
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
        assert_eq!(
            client.name(),
            SorobanString::from_str(&env, "Measurable Token")
        );
        assert_eq!(client.symbol(), SorobanString::from_str(&env, "MST"));
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
