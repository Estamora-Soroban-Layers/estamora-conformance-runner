//! Reading contract state in the vector's own vocabulary.
//!
//! The evaluator reaches state only through [`estamora_core::world::World`], and this
//! is the implementation that reaches a real contract. Two obligations follow from
//! that trait and both are load-bearing here.
//!
//! # Answers are given in the vector's vocabulary
//!
//! A vector names fixture actors — `alice`, `bob` — and a ledger holds fifty-six
//! characters of base-32. This module owns the mapping in both directions, so a
//! requirement that fails names `alice` rather than an address no reader can check by
//! eye. A returned address that is not one of the fixture's actors is reported as
//! itself, because pretending it was an actor would be a misattribution.
//!
//! # A read that cannot be performed is an execution failure
//!
//! Not a failed requirement. A contract that cannot answer a read the profile
//! declares has not been *measured* on that requirement, and reporting it as a
//! violation would blame the contract for the runner's inability to see. Every
//! conversion failure and every host failure is therefore returned as an error, and
//! the assertion layer turns that into an undecidable vector rather than a failing
//! one.
//!
//! # The resource sets a fixture declares are the only ones readable
//!
//! An invariant that ranges over `balances` can be evaluated because the vector
//! declares which accounts exist; there is no way to enumerate a contract's storage,
//! and guessing at storage keys the specification never named would produce a result
//! nobody could interpret. `allowances` is readable from the allowances the fixture
//! granted. Any other resource set is refused by name.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use estamora_core::value::Value;
use estamora_core::world::World;
use estamora_core::{Error, ErrorClass, Result};
use soroban_sdk::xdr::{Limits, ScVal, WriteXdr as _};
use soroban_sdk::{
    Address, Env, FromVal, IntoVal, Map, String as SorobanString, Symbol, TryFromVal, Val, Vec,
};

/// The resource set name a vector uses for balances.
pub const BALANCES: &str = "balances";

/// The resource set name a vector uses for allowances.
pub const ALLOWANCES: &str = "allowances";

/// A fixture actor and the address it was deployed as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    /// The name the vector uses.
    pub name: String,
    /// The address it was deployed as.
    pub address: Address,
}

/// A contract, and the vocabulary its reads are answered in.
pub struct ContractWorld<'a> {
    env: &'a Env,
    contract: Address,
    actors: std::vec::Vec<Actor>,
    allowances: std::vec::Vec<AllowanceGrant>,
    ledger_sequence: u32,
}

/// An allowance a fixture granted, and when it lapses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowanceGrant {
    /// The holder who granted it.
    pub from: String,
    /// The account that may spend it.
    pub spender: String,
    /// The ledger at which it expires, when the fixture declared one.
    pub live_until_ledger: Option<u32>,
}

impl core::fmt::Debug for ContractWorld<'_> {
    /// Prints the vocabulary and the ledger point, never the host.
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ContractWorld")
            .field(
                "actors",
                &self
                    .actors
                    .iter()
                    .map(|actor| actor.name.as_str())
                    .collect::<std::vec::Vec<&str>>(),
            )
            .field("ledger_sequence", &self.ledger_sequence)
            .finish_non_exhaustive()
    }
}

impl<'a> ContractWorld<'a> {
    /// A world over `contract`, answering in the vocabulary of `actors`.
    #[must_use]
    pub fn new(
        env: &'a Env,
        contract: Address,
        actors: std::vec::Vec<Actor>,
        allowances: std::vec::Vec<AllowanceGrant>,
        ledger_sequence: u32,
    ) -> Self {
        Self {
            env,
            contract,
            actors,
            allowances,
            ledger_sequence,
        }
    }

    /// The address a fixture actor was deployed as.
    #[must_use]
    pub fn address_of(&self, name: &str) -> Option<&Address> {
        self.actors
            .iter()
            .find(|actor| actor.name == name)
            .map(|actor| &actor.address)
    }

    /// The actor name an address belongs to, where it belongs to one.
    #[must_use]
    pub fn name_of(&self, address: &Address) -> Option<&str> {
        self.actors
            .iter()
            .find(|actor| &actor.address == address)
            .map(|actor| actor.name.as_str())
    }

    /// Resolves a fixture actor's name to the address it was deployed as.
    ///
    /// Deliberately the *only* accepted spelling. A vector that names an arbitrary
    /// `StrKey` would have to be parsed by the host, whose parser panics on input it
    /// does not recognise — and a library that can be made to panic by a document is
    /// not one a runner can hand untrusted profiles to. The vector format declares its
    /// actors, so anything else is a document defect, and it is reported as one.
    ///
    /// # Errors
    ///
    /// Returns an execution failure when the name is not one of the fixture's actors.
    fn resolve(&self, value: &Value) -> Result<Address> {
        match value {
            Value::Address(name) | Value::Text(name) => self.address_of(name).cloned().ok_or_else(|| {
                Error::new(
                    ErrorClass::ExecutionError,
                    format!(
                        "`{name}` is not one of the fixture's actors; a vector names the actors it                          declares, and nothing else, so that every address in a report is one a                          reader can check by eye"
                    ),
                )
            }),
            other => Err(Error::new(
                ErrorClass::ExecutionError,
                format!(
                    "an address was required and {} was supplied",
                    other.render()
                ),
            )),
        }
    }

    /// Converts a vector value into a host value.
    ///
    /// # Errors
    ///
    /// Returns an execution failure for a value with no host representation, or for
    /// an address that cannot be resolved.
    pub fn to_host(&self, value: &Value) -> Result<Val> {
        let env = self.env;
        Ok(match value {
            Value::Integer(number) => number.into_val(env),
            Value::Bool(flag) => flag.into_val(env),
            Value::Text(text) => SorobanString::from_str(env, text).into_val(env),
            Value::Address(_) => self.resolve(value)?.into_val(env),
            Value::Sequence(items) => {
                let mut sequence = Vec::new(env);
                for item in items {
                    sequence.push_back(self.to_host(item)?);
                }
                sequence.into_val(env)
            },
            Value::Absent => {
                return Err(Error::new(
                    ErrorClass::ExecutionError,
                    "an absent value cannot be passed to a contract",
                ));
            },
            Value::Record(_) => {
                return Err(Error::new(
                    ErrorClass::ExecutionError,
                    "a named-field value cannot be passed to a contract as an argument; a \
                     profile names arguments positionally",
                ));
            },
        })
    }

    /// Converts a host value into a vector value.
    ///
    /// # Errors
    ///
    /// Returns an execution failure when the value is of a type this runner cannot
    /// carry. Refusing is deliberate: a value silently rendered as text would make
    /// every comparison against it incomparable, and the failure would be reported
    /// against the contract rather than against the conversion.
    pub fn from_host(&self, val: &Val) -> Result<Value> {
        let env = self.env;

        // A void return is the dominant case for every mutating method in SEP-41, and it
        // has to be recognised before the numeric attempts: left to them it fails every
        // conversion and the call is recorded as one whose result the runner could not
        // read, which makes every void-returning method in a profile undecidable rather
        // than measured. It is reported as an absence, which is what it is.
        if matches!(ScVal::from_val(env, val), ScVal::Void) {
            return Ok(Value::Absent);
        }

        if let Ok(number) = i128::try_from_val(env, val) {
            return Ok(Value::Integer(number));
        }
        if let Ok(flag) = bool::try_from_val(env, val) {
            return Ok(Value::Bool(flag));
        }
        if let Ok(address) = Address::try_from_val(env, val) {
            return Ok(match self.name_of(&address) {
                Some(name) => Value::Address(name.to_owned()),
                // Not a fixture actor: reported as itself rather than renamed,
                // because calling it an actor would attribute a movement to the wrong
                // party.
                None => Value::Address(address.to_string().to_string()),
            });
        }
        if let Ok(number) = u32::try_from_val(env, val) {
            return Ok(Value::Integer(i128::from(number)));
        }
        if let Ok(number) = u64::try_from_val(env, val) {
            return Ok(Value::Integer(i128::from(number)));
        }
        if let Ok(symbol) = Symbol::try_from_val(env, val) {
            return Ok(Value::Text(symbol.to_string()));
        }
        if let Ok(text) = SorobanString::try_from_val(env, val) {
            return Ok(Value::Text(text.to_string()));
        }
        if let Ok(items) = Vec::<Val>::try_from_val(env, val) {
            let mut sequence =
                std::vec::Vec::with_capacity(usize::try_from(items.len()).unwrap_or(0));
            for item in items.iter() {
                sequence.push(self.from_host(&item)?);
            }
            return Ok(Value::Sequence(sequence));
        }
        if let Ok(entries) = Map::<Val, Val>::try_from_val(env, val) {
            let mut fields = BTreeMap::new();
            for (key, value) in entries.iter() {
                let name = match self.from_host(&key)? {
                    Value::Text(text) => text,
                    other => other.render(),
                };
                fields.insert(name, self.from_host(&value)?);
            }
            return Ok(Value::Record(fields));
        }

        Err(Error::new(
            ErrorClass::ExecutionError,
            format!(
                "the contract returned a value this runner cannot carry: {}",
                describe(env, *val)
            ),
        ))
    }

    /// Calls a read-only method and returns its value.
    fn call(&self, method: &str, args: Vec<Val>) -> Result<Value> {
        let symbol = Symbol::new(self.env, method);
        let returned = self
            .env
            .try_invoke_contract::<Val, soroban_sdk::InvokeError>(&self.contract, &symbol, args)
            .map_err(|problem| {
                Error::new(
                    ErrorClass::ExecutionError,
                    format!("the read `{method}` could not be performed: {problem:?}"),
                )
                .with_context("method", method)
            })?;
        let returned = returned.map_err(|problem| {
            Error::new(
                ErrorClass::ExecutionError,
                format!(
                    "the read `{method}` returned a value the runner could not read: {problem:?}"
                ),
            )
            .with_context("method", method)
        })?;
        self.from_host(&returned)
    }
}

impl World for ContractWorld<'_> {
    fn read(&self, method: &str, args: &[Value]) -> Result<Value> {
        let mut host_args = Vec::new(self.env);
        for argument in args {
            host_args.push_back(self.to_host(argument)?);
        }
        self.call(method, host_args)
    }

    fn resource_members(&self, resource: &str) -> Result<std::vec::Vec<Value>> {
        match resource {
            BALANCES => {
                let mut members = std::vec::Vec::new();
                for actor in &self.actors {
                    let value = self.read("balance", &[Value::Address(actor.name.clone())])?;
                    members.push(value);
                }
                Ok(members)
            },
            ALLOWANCES => {
                let mut members = std::vec::Vec::new();
                for allowance in &self.allowances {
                    let value = self.read(
                        "allowance",
                        &[
                            Value::Address(allowance.from.clone()),
                            Value::Address(allowance.spender.clone()),
                        ],
                    )?;
                    members.push(value);
                }
                Ok(members)
            },
            other => Err(Error::new(
                ErrorClass::ExecutionError,
                format!(
                    "the resource set `{other}` cannot be enumerated: Estamora reads only the \
                     sets a vector declares, because a contract's storage cannot be walked and \
                     inventing keys the profile never named would produce a result nobody can \
                     interpret"
                ),
            )
            .with_context("resource", other)),
        }
    }

    fn ledger_sequence(&self) -> u32 {
        self.ledger_sequence
    }

    fn allowance_expiry(&self, from: &str, spender: &str) -> Option<u32> {
        self.allowances
            .iter()
            .find(|allowance| allowance.from == from && allowance.spender == spender)
            .and_then(|allowance| allowance.live_until_ledger)
    }
}

/// A short rendering of a host value, for a diagnostic.
///
/// A conversion that fails is reported with what the value *was*, so that a reader can
/// tell an unimplemented type from a contract returning nonsense. The XDR form is
/// used because it is the only total rendering the SDK offers.
fn describe(env: &Env, val: Val) -> String {
    let xdr: ScVal = ScVal::from_val(env, &val);
    let Ok(encoded) = xdr.to_xdr(Limits::none()) else {
        return "<unrenderable>".to_owned();
    };
    if encoded.is_empty() {
        return "<unrenderable>".to_owned();
    }
    let mut rendered = String::with_capacity(encoded.len() * 2);
    for byte in encoded.iter().take(32) {
        let _ignored = write!(rendered, "{byte:02x}");
    }
    if encoded.len() > 32 {
        rendered.push('…');
    }
    rendered
}

/// A world over a contract, used by tests that need one without an assertion.
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{Actor, AllowanceGrant, BALANCES, ContractWorld};
    use crate::host::LocalHost;
    use estamora_core::ErrorClass;
    use estamora_core::value::Value;
    use estamora_core::world::World as _;
    use estamora_fixture_token::ConformingToken;
    use soroban_sdk::Address;
    use soroban_sdk::testutils::Address as _;

    /// A host with a conforming token deployed and two named actors.
    fn world() -> (LocalHost, ContractWorld<'static>) {
        // The host owns the environment and the world borrows it; both are leaked so
        // that this helper can return a value rather than a closure. Test-only, and
        // the alternative is threading lifetimes through every test for no benefit.
        let host = Box::leak(Box::new(LocalHost::at_default_point()));
        let contract = host.register(ConformingToken);
        let alice = Address::generate(host.env());
        let bob = Address::generate(host.env());
        let world = ContractWorld::new(
            host.env(),
            contract,
            vec![
                Actor {
                    name: "alice".to_owned(),
                    address: alice,
                },
                Actor {
                    name: "bob".to_owned(),
                    address: bob,
                },
            ],
            vec![AllowanceGrant {
                from: "alice".to_owned(),
                spender: "bob".to_owned(),
                live_until_ledger: Some(5_000),
            }],
            1_000,
        );
        (host.clone(), world)
    }

    #[test]
    fn an_address_is_answered_by_the_name_the_vector_used() {
        let (_host, world) = world();
        let alice = world.address_of("alice").unwrap().clone();
        assert_eq!(world.name_of(&alice), Some("alice"));
        // And a resolved name is the same address, so a read agrees with itself.
        assert_eq!(
            world
                .read("balance", &[Value::Address("alice".to_owned())])
                .unwrap(),
            Value::Integer(0)
        );
    }

    #[test]
    fn a_resource_set_ranges_over_exactly_the_actors_the_fixture_declared() {
        let (_host, world) = world();
        let members = world.resource_members(BALANCES).unwrap();
        assert_eq!(
            members.len(),
            2,
            "one member per declared actor, and no others"
        );
    }

    #[test]
    fn a_resource_set_that_cannot_be_enumerated_is_refused_by_name() {
        // The honest boundary: a contract's storage cannot be walked, so an invariant
        // over a set nobody declared is an error rather than a guess.
        let (_host, world) = world();
        let problem = world.resource_members("storage").unwrap_err();
        assert_eq!(problem.class(), ErrorClass::ExecutionError);
        assert_eq!(problem.context_value("resource"), Some("storage"));
    }

    #[test]
    fn the_ledger_point_is_the_one_the_fixture_declared() {
        let (_host, world) = world();
        assert_eq!(world.ledger_sequence(), 1_000);
    }

    #[test]
    fn allowance_expiry_is_answered_from_the_fixture_that_granted_it() {
        let (_host, world) = world();
        assert_eq!(world.allowance_expiry("alice", "bob"), Some(5_000));
        assert_eq!(world.allowance_expiry("bob", "alice"), None);
    }

    #[test]
    fn a_value_with_no_host_representation_is_refused_rather_than_approximated() {
        let (_host, world) = world();
        assert!(world.to_host(&Value::Absent).is_err());
        assert!(
            world
                .to_host(&Value::Record(std::collections::BTreeMap::new()))
                .is_err()
        );
    }

    #[test]
    fn a_name_that_is_neither_an_actor_nor_an_address_is_an_execution_failure() {
        let (_host, world) = world();
        let problem = world
            .read("balance", &[Value::Address("not-an-address".to_owned())])
            .unwrap_err();
        assert_eq!(problem.class(), ErrorClass::ExecutionError);
    }

    #[test]
    fn a_read_of_a_method_the_contract_does_not_have_is_not_a_failed_requirement() {
        // It is the runner's inability to measure, so it must not reach a report as a
        // violation.
        let (_host, world) = world();
        let problem = world.read("nonexistent", &[]).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::ExecutionError);
    }
}
