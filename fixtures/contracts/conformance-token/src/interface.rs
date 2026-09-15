//! What each fixture publishes, declared by the fixture itself.
//!
//! # Why a fixture declares its interface instead of publishing a spec section
//!
//! A deployed contract is inspected by reading the `contractspecv0` custom section out
//! of its compiled WebAssembly. A fixture is a Rust type registered directly into the
//! host, so there is no artifact to read — but the same is true of an in-repository
//! example contract used as a fixture, and a runner that could only inspect deployed
//! artifacts could not exercise its own interface dimension at all.
//!
//! So a fixture states what it exposes, in the same shape the runner's interface
//! inspection produces from a real artifact, and the assertion layer cannot tell which
//! route was used. The declaration is kept honest by construction rather than by
//! convention: [`declared`] returns a slice that differs for
//! [`Defect::MissingDecimals`], so the one fixture that genuinely omits a method is the
//! one fixture whose declaration omits it.
//!
//! # The one place a declaration is not the artifact's shape
//!
//! The mutating methods below declare no return, which is what SEP-0041 declares for
//! them. The fixture's Rust functions spell their refusals as `Result<(), Error>`,
//! because a fixture is called from Rust tests that want the error back — and a signature
//! returning `Result` would publish `result<void,error>` if this crate were ever compiled
//! to an artifact. The runtime behaviour is the same either way: the generated wrapper
//! traps on `Err`, so the host records the same contract error a `panic_with_error!`
//! produces, and the failure dimension cannot tell the two spellings apart.
//!
//! The distinction matters because the two routes are not interchangeable. This
//! declaration is the standard's surface, which is what the interface dimension should
//! compare a fixture against; `fixtures/contracts/measurable-token` is a real artifact
//! and therefore has to match the standard in the bytes, which is why it spells its
//! refusals as traps and imports `String` under its own name.
//!
//! # Why the setup entry points are listed
//!
//! `fixture_mint` and `fixture_approve` really are exposed by every fixture, so they are
//! listed. Omitting them would make the declared interface a description of the standard
//! the fixture claims to implement rather than of what it publishes, and the two are
//! exactly what a conformance run has to keep apart. A profile does not declare them, so
//! the interface dimension does not compare them; a reader of a report that lists the
//! exposed surface still sees them, which is the honest rendering.
//!
//! # Why the types are spelled the way the SDK spells them
//!
//! `transfer`'s destination is a `muxed_address` because SEP-0041 declares it as
//! `MuxedAddress`, and the type is not cosmetic: a contract that accepted only a plain
//! `Address` could not receive a multiplexed destination, and the interface dimension
//! reports that as the narrowing it is.

use crate::Defect;

/// One method a fixture publishes, in plain types.
///
/// Deliberately not the runner's `DeclaredMethod`: a fixture must not depend on the
/// crate that inspects it, or the inspection route a fixture exercises would be one a
/// deployed contract does not take. The runner converts this into its own shape, which
/// is a conversion with nothing to get wrong.
#[derive(Debug, Clone, Copy)]
pub struct Exposed {
    /// The method's name.
    pub name: &'static str,
    /// Its parameter types, in order, in the spelling the runner compares.
    pub parameters: &'static [&'static str],
    /// Its return type, or `None` for a void return.
    pub returns: Option<&'static str>,
    /// Whether the method is read-only.
    pub readonly: bool,
}

const fn exposing(
    name: &'static str,
    parameters: &'static [&'static str],
    returns: Option<&'static str>,
    readonly: bool,
) -> Exposed {
    Exposed {
        name,
        parameters,
        returns,
        readonly,
    }
}

/// The SEP-0041 method surface, in the order the standard declares it.
///
/// One list rather than three near-copies, because the three declarations below differ
/// only in whether `decimals` is present and whether the fixture publishes the two extra
/// entry points that exist so a test can observe something rather than as part of any
/// interface.
const SEP_41: &[Exposed] = &[
    exposing(
        "transfer",
        &["address", "muxed_address", "i128"],
        None,
        false,
    ),
    exposing(
        "approve",
        &["address", "address", "i128", "u32"],
        None,
        false,
    ),
    exposing("allowance", &["address", "address"], Some("i128"), true),
    exposing("balance", &["address"], Some("i128"), true),
    exposing(
        "transfer_from",
        &["address", "address", "address", "i128"],
        None,
        false,
    ),
    exposing("burn", &["address", "i128"], None, false),
    exposing("burn_from", &["address", "address", "i128"], None, false),
    exposing("decimals", &[], Some("u32"), true),
    exposing("name", &[], Some("string"), true),
    exposing("symbol", &[], Some("string"), true),
];

/// The entry points a fixture exposes so that a vector's opening state can be
/// established, which the standard itself provides no way to do.
const SETUP: &[Exposed] = &[
    exposing("fixture_mint", &["address", "i128"], None, false),
    exposing(
        "fixture_approve",
        &["address", "address", "i128", "u32"],
        None,
        false,
    ),
];

/// The entry points that exist only so that a test can observe something, and that are
/// published by the conforming fixture alone.
const OBSERVATION: &[Exposed] = &[
    exposing("total_supply", &[], Some("i128"), true),
    exposing("always_refuses", &[], Some("i128"), false),
    exposing("emits_then_refuses", &["address"], None, false),
    exposing(
        "mutates_then_refuses",
        &["address", "i128", "bool"],
        None,
        false,
    ),
];

/// The methods the fixture that behaves correctly publishes.
const CONFORMING: &[Exposed] = &[
    exposing(
        "transfer",
        &["address", "muxed_address", "i128"],
        None,
        false,
    ),
    exposing(
        "approve",
        &["address", "address", "i128", "u32"],
        None,
        false,
    ),
    exposing("allowance", &["address", "address"], Some("i128"), true),
    exposing("balance", &["address"], Some("i128"), true),
    exposing(
        "transfer_from",
        &["address", "address", "address", "i128"],
        None,
        false,
    ),
    exposing("burn", &["address", "i128"], None, false),
    exposing("burn_from", &["address", "address", "i128"], None, false),
    exposing("decimals", &[], Some("u32"), true),
    exposing("name", &[], Some("string"), true),
    exposing("symbol", &[], Some("string"), true),
    exposing("total_supply", &[], Some("i128"), true),
    exposing("always_refuses", &[], Some("i128"), false),
    exposing("emits_then_refuses", &["address"], None, false),
    exposing(
        "mutates_then_refuses",
        &["address", "i128", "bool"],
        None,
        false,
    ),
    exposing("fixture_mint", &["address", "i128"], None, false),
    exposing(
        "fixture_approve",
        &["address", "address", "i128", "u32"],
        None,
        false,
    ),
];

/// What a defective fixture publishes, `decimals` included.
const DEFECTIVE: &[Exposed] = &[
    exposing(
        "transfer",
        &["address", "muxed_address", "i128"],
        None,
        false,
    ),
    exposing(
        "approve",
        &["address", "address", "i128", "u32"],
        None,
        false,
    ),
    exposing("allowance", &["address", "address"], Some("i128"), true),
    exposing("balance", &["address"], Some("i128"), true),
    exposing(
        "transfer_from",
        &["address", "address", "address", "i128"],
        None,
        false,
    ),
    exposing("burn", &["address", "i128"], None, false),
    exposing("burn_from", &["address", "address", "i128"], None, false),
    exposing("decimals", &[], Some("u32"), true),
    exposing("name", &[], Some("string"), true),
    exposing("symbol", &[], Some("string"), true),
    exposing("fixture_mint", &["address", "i128"], None, false),
    exposing(
        "fixture_approve",
        &["address", "address", "i128", "u32"],
        None,
        false,
    ),
];

/// What the fixture that omits `decimals` publishes.
const DEFECTIVE_WITHOUT_DECIMALS: &[Exposed] = &[
    exposing(
        "transfer",
        &["address", "muxed_address", "i128"],
        None,
        false,
    ),
    exposing(
        "approve",
        &["address", "address", "i128", "u32"],
        None,
        false,
    ),
    exposing("allowance", &["address", "address"], Some("i128"), true),
    exposing("balance", &["address"], Some("i128"), true),
    exposing(
        "transfer_from",
        &["address", "address", "address", "i128"],
        None,
        false,
    ),
    exposing("burn", &["address", "i128"], None, false),
    exposing("burn_from", &["address", "address", "i128"], None, false),
    exposing("name", &[], Some("string"), true),
    exposing("symbol", &[], Some("string"), true),
    exposing("fixture_mint", &["address", "i128"], None, false),
    exposing(
        "fixture_approve",
        &["address", "address", "i128", "u32"],
        None,
        false,
    ),
];

/// The methods the fixture for `defect` publishes.
///
/// Returned whole rather than composed at the call site, so that a fixture cannot
/// describe itself as something it is not.
#[must_use]
pub const fn declared(defect: Defect) -> &'static [Exposed] {
    match defect {
        Defect::None => CONFORMING,
        Defect::MissingDecimals => DEFECTIVE_WITHOUT_DECIMALS,
        _ => DEFECTIVE,
    }
}

/// The standard's own methods, without the entry points that exist for the runner.
///
/// Exposed separately because a test that needs to name a method of the interface under
/// test should not have to filter the setup and observation entry points out of a
/// fixture's full surface.
#[must_use]
pub fn token_methods(defect: Defect) -> Vec<&'static Exposed> {
    let _ = declared(defect);
    SEP_41
        .iter()
        .filter(|method| defect.exposes_decimals() || method.name != "decimals")
        .collect()
}

/// Whether a name is one of the entry points that are not part of the standard.
#[must_use]
pub fn is_auxiliary(name: &str) -> bool {
    SETUP
        .iter()
        .chain(OBSERVATION.iter())
        .any(|entry| entry.name == name)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{SEP_41, declared, is_auxiliary, token_methods};
    use crate::Defect;

    #[test]
    fn only_the_fixture_that_omits_decimals_declares_the_absence() {
        assert!(declared(Defect::None).iter().any(|m| m.name == "decimals"));
        assert!(
            declared(Defect::MissingDecimals)
                .iter()
                .all(|m| m.name != "decimals")
        );
        for defect in Defect::ALL {
            if defect != Defect::MissingDecimals {
                assert!(
                    declared(defect).iter().any(|m| m.name == "decimals"),
                    "{defect:?} publishes decimals and must say so"
                );
            }
        }
    }

    #[test]
    fn every_fixture_declares_the_whole_standard_so_its_defect_is_its_only_difference() {
        // A fixture missing a method the profile requires would be wrong in more ways
        // than the one its name advertises, and every interface failure would be
        // reported against all of them.
        for defect in Defect::ALL {
            let names: Vec<&str> = declared(defect).iter().map(|m| m.name).collect();
            for method in SEP_41 {
                if method.name == "decimals" && !defect.exposes_decimals() {
                    continue;
                }
                assert!(
                    names.contains(&method.name),
                    "{defect:?} does not publish {}",
                    method.name
                );
            }
        }
    }

    #[test]
    fn a_declared_parameter_matches_the_signature_the_contract_implements() {
        // The declaration is the runner's only view of a fixture's interface, so a
        // parameter count that disagreed with the contract would make the interface
        // dimension pass a contract it should fail.
        let transfer = declared(Defect::None)
            .iter()
            .find(|method| method.name == "transfer")
            .expect("the conforming fixture publishes transfer");
        assert_eq!(
            transfer.parameters,
            ["address", "muxed_address", "i128"],
            "SEP-0041 declares transfer's destination as a muxed address"
        );
        let approve = declared(Defect::None)
            .iter()
            .find(|method| method.name == "approve")
            .expect("the conforming fixture publishes approve");
        assert_eq!(approve.parameters.len(), 4);
    }

    #[test]
    fn the_standard_s_methods_are_reported_without_the_auxiliary_entry_points() {
        for defect in Defect::ALL {
            let methods = token_methods(defect);
            assert!(
                methods.iter().all(|method| !is_auxiliary(method.name)),
                "{defect:?} reported an auxiliary entry point as a token method"
            );
            assert_eq!(
                methods.len(),
                if defect.exposes_decimals() { 10 } else { 9 },
                "{defect:?} reports the wrong number of standard methods"
            );
        }
    }

    #[test]
    fn a_fixture_that_behaves_correctly_admits_the_entry_points_it_was_given_to_be_testable() {
        assert!(
            declared(Defect::None)
                .iter()
                .any(|method| method.name == "always_refuses")
        );
        assert!(
            declared(Defect::DoubleEmits)
                .iter()
                .all(|method| method.name != "always_refuses")
        );
    }
}
