#![cfg(unix)]
//! Spacemacs boot parity: both editors mount the sealed Spacemacs fixture
//! through the HOME overlay and must reach the Spacemacs home buffer with
//! matching geometry.
//!
//! The fixture is the Spacemacs analog of the Doom one: a GNU-bootstrapped,
//! sealed checkout that sessions mount by making `<state>/.emacs.d` an
//! overlay (symlinks into the tree, `elpa` shared read-only, `.cache`
//! seeded per session) with the deterministic `.spacemacs` dotfile that
//! suppresses the first-run wizard.  Materialization is explicit
//! (`cargo run -p xtask -- infra materialize spacemacs`); the test skips
//! where the fixture is absent.
//!
//! The learned Doom lesson applies here too: settle idle before the first
//! keypress, so deferred startup paths (which race input in any config
//! framework) take the same branch in both editors.

use crate::support;
use neomacs_infra::config_env::ConfigEnvironment as _;
use neomacs_tui_tests::{TuiLaunch, TuiSession, TuiTempDirectory};
use std::time::Duration;
use support::*;

fn spacemacs_launch(
    spacemacs: &neomacs_infra::SpacemacsEnvironment,
    state: &TuiTempDirectory,
    program: &std::ffi::OsStr,
) -> TuiLaunch {
    let mut launch = TuiLaunch::new(program).arg("-nw");
    for (name, value) in spacemacs.session_env(state) {
        launch = launch.env(name, value);
    }
    launch
}

#[test]
fn spacemacs_boot_reaches_home_buffer_in_both_editors() {
    let Some(spacemacs) = neomacs_infra::SpacemacsEnvironment::open() else {
        eprintln!(
            "skipping: no sealed Spacemacs fixture; run \
             `cargo run -p xtask -- infra materialize spacemacs` to build one"
        );
        return;
    };

    let gnu_state = TuiTempDirectory::new("spac-gnu-");
    let neo_state = TuiTempDirectory::new("spac-neo-");
    for state in [&gnu_state, &neo_state] {
        spacemacs
            .prepare_session_state(state)
            .expect("seed Spacemacs session state");
    }

    let mut gnu = TuiSession::spawn_launch(
        spacemacs_launch(&spacemacs, &gnu_state, std::ffi::OsStr::new("emacs")),
        "GNU",
    );
    gnu.read(Duration::from_secs(5));
    let mut neo = TuiSession::spawn_launch(
        spacemacs_launch(
            &spacemacs,
            &neo_state,
            neomacs_tui_tests::neomacs_binary().as_os_str(),
        ),
        "NEO",
    );

    let home_visible = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("SPC") && row.contains("Find File"))
            || grid.iter().any(|row| row.contains("spacemacs"))
    };
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(90), home_visible);

    // Settle past deferred startup before comparing or touching input.
    read_both(&mut gnu, &mut neo, Duration::from_secs(4));
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        assert!(
            home_visible(&session.text_grid()),
            "{label} should reach the Spacemacs home buffer"
        );
    }
    assert_pair_exact_display(
        "spacemacs_boot_reaches_home_buffer_in_both_editors",
        &gnu,
        &neo,
    );
}
