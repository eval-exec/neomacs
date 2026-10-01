//! Gruvbox's two real terminal palette branches and rendered editing surfaces.

mod prelude;

mod harness;
mod scenario;

use harness::*;
use scenario::*;

#[test]
fn gruvbox_theme_real_terminal_profiles_match_gnu() {
    let oracle = oracle();
    let default_org = catch_phase("default Org consumer profile", || {
        default_org_consumer(oracle.prepared_packages())
    })
    .and_then(|result| result);
    let truecolor = catch_phase("truecolor profile", || {
        truecolor(oracle.prepared_packages())
    })
    .and_then(|result| result);
    let color256 = catch_phase("256-color profile", || color256(oracle.prepared_packages()))
        .and_then(|result| result);
    let failures = [default_org.err(), truecolor.err(), color256.err()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert!(
        failures.is_empty(),
        "Gruvbox real terminal profiles failed:\n{}",
        failures.join("\n\n")
    );
}
