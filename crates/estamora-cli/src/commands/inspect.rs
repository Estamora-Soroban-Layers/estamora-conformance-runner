//! `estamora inspect` — read what a contract exposes.
//!
//! Inspection is a prerequisite for a conformance run rather than a substitute for one.
//! Soroban reports a deliberate `panic!()` and a call to a method that does not exist
//! through the same channel, so an abort is only readable as the contract's own refusal
//! once the method is known to exist — which is what inspection establishes. It
//! establishes nothing else, and this command says so on every invocation.
//!
//! It is useful on its own for the same reason: a contract that publishes an interface
//! it does not implement will pass inspection and fail every vector, and seeing what a
//! contract *claims* is how that difference is diagnosed.

use std::io::Write;

use estamora_core::{ExitCode, Result};

use crate::commands::{Common, Format, emit};
use crate::config::RunConfig;
use crate::engine::{Target, inspect};
use crate::output;

/// Read a contract's interface.
#[derive(Debug, clap::Args)]
pub struct InspectArgs {
    /// The contract to inspect: `fixture:<name>`, a path ending in `.wasm`, or a
    /// contract identifier together with `--network`.
    #[arg(long, value_name = "CONTRACT")]
    pub contract: String,

    /// The network the contract lives on. Required for a deployed contract.
    #[arg(long, value_name = "NETWORK")]
    pub network: Option<String>,

    /// The format the inspection is written in.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    pub format: Format,

    /// Where the inspection is written. Defaults to standard output.
    #[arg(long, value_name = "PATH")]
    pub out: Option<std::path::PathBuf>,
}

/// Performs the command.
///
/// # Errors
///
/// Returns a usage error for a malformed contract name, and a contract resolution
/// error when the artifact cannot be read or the interface cannot be established. An
/// inspection that did not happen is never reported as an empty interface: the difference
/// between "exposes nothing" and "could not be read" is the difference between a defect
/// and an absence of information.
pub fn execute(args: &InspectArgs, common: &Common, out: &mut dyn Write) -> Result<ExitCode> {
    let spec_root = common.spec_root()?;
    let target = Target::parse(&args.contract, args.network.as_deref())?;
    // A profile is not needed to inspect, so a placeholder path is used that is never
    // read: `inspect` deploys the target and nothing else. It is constructed rather than
    // guessed at so that the type cannot grow a dependency on a profile it does not have.
    let config = RunConfig::new(spec_root.join("profiles"), spec_root, target);
    let inspection = inspect(&config)?;

    let rendered = match args.format {
        Format::Json => serde_json::to_string_pretty(&serde_json::json!({
            "target": inspection.target,
            "network": inspection.network,
            "wasm_hash": inspection.wasm_hash,
            "seeding": inspection.seeding.description(),
            "source": inspection.interface.source,
            "methods": inspection.interface.methods.iter().map(|method| serde_json::json!({
                "name": method.name,
                "parameters": method.parameters.iter().map(|parameter| &parameter.type_name).collect::<Vec<&String>>(),
                "returns": method.returns,
                "readonly": method.readonly,
            })).collect::<Vec<_>>(),
        }))
        .map_err(|problem| {
            estamora_core::Error::new(
                estamora_core::ErrorClass::ReportError,
                format!("the inspection could not be serialized: {problem}"),
            )
        })?,
        _ => output::inspection(&inspection),
    };

    emit(out, args.out.as_deref(), &rendered)?;
    // An inspection that succeeded is not a conformance result, and the exit status says
    // so by being success only in the sense of "the interface was read".
    Ok(ExitCode::Success)
}
