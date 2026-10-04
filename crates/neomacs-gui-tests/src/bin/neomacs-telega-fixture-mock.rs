//! Offline `telega-server` stand-in used by the account-free Telega GUI
//! fixture.  Telega launches this binary through its real
//! `telega-server-command` customization; the binary speaks the real
//! stdin/stdout protocol and serves the synthetic scenario below
//! `NEOMACS_TELEGA_FIXTURE_ROOT`.
//!
//! It never opens a network connection, never reads `~/.telega` or any other
//! personal path, and rejects unmodeled requests explicitly.

use std::process::ExitCode;

use neomacs_gui_tests::telega_fixture::mock::{
    ControlReader, FIXTURE_ROOT_ENV, FIXTURE_SCENARIO_ENV, FixtureLog, FixturePaths, FixtureServer,
    Invocation, parse_invocation, run_server,
};
use neomacs_gui_tests::telega_fixture::scenario::{AvatarAvailability, FixtureScenario};

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match parse_invocation(&args) {
        // Telega's `telega-server -h` version probe: exit 0 before touching
        // the environment so a missing fixture root is not misreported.
        Invocation::Probe(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Invocation::Error(message) => {
            eprintln!("telega-fixture-mock: {message}");
            ExitCode::from(2)
        }
        Invocation::Serve => match serve() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("telega-fixture-mock: {error}");
                ExitCode::FAILURE
            }
        },
    }
}

fn serve() -> Result<(), String> {
    let root = std::env::var(FIXTURE_ROOT_ENV)
        .map_err(|_| format!("{FIXTURE_ROOT_ENV} is not set; refusing to serve"))?;
    let scenario_name = std::env::var(FIXTURE_SCENARIO_ENV).unwrap_or_else(|_| "ready".to_string());
    let availability = AvatarAvailability::from_name(&scenario_name)
        .ok_or_else(|| format!("unknown fixture scenario `{scenario_name}`"))?;

    let paths = FixturePaths::new(root);
    paths.create().map_err(|error| error.to_string())?;
    let log = FixtureLog::open(&paths.log).map_err(|error| error.to_string())?;
    let scenario = FixtureScenario::new(&paths.photos, availability);
    let server = FixtureServer::new(scenario, log).map_err(|error| error.to_string())?;
    let control = ControlReader::new(paths.control.clone());
    run_server(server, std::io::stdout().lock(), control).map_err(|error| error.to_string())
}
