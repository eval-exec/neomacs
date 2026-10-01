use std::time::Duration;

use super::{BEACON_MELPA_PIN, CachedMelpaOracle};

use super::scenario::{PackageTuiScenario, PairTimeout, ReadinessCheckpoint, TerminalProfile};

mod harness;
mod prelude;
mod scenario;

use harness::*;
use prelude::*;
use scenario::*;

#[test]
fn beacon_real_truecolor_automated_display_and_timer_lifecycle_match_gnu() {
    let oracle = CachedMelpaOracle::new(BEACON_MELPA_PIN, "beacon.el")
        .expect("prepare exact shallow Beacon source below ./tmp")
        .with_prelude(BEACON_TUI_PRELUDE);
    let mut pair = PackageTuiScenario::new("beacon-real-display", oracle.prepared_packages())
        .terminal_profile(TerminalProfile::TrueColor)
        .spawn_when_ready(
            ReadinessCheckpoint::new(
                "initial scratch buffer",
                PairTimeout::per_editor(Duration::from_secs(20), Duration::from_secs(30)),
            ),
            |grid| grid.iter().any(|row| row.contains("*scratch*")),
        )
        .expect("spawn ready real truecolor Beacon PTY pair");
    pair.resize_both(24, 80);
    pair.send_both(b"\x1b[O");
    pair.settle_both(Duration::from_millis(500));

    let mut mismatches = Vec::new();
    let body = catch_phase("Beacon TUI body", || run_body(&mut pair, &mut mismatches))
        .and_then(|result| result);
    let cleanup = catch_phase("Beacon TUI cleanup", || {
        run_cleanup(&mut pair, &mut mismatches)
    })
    .and_then(|result| result);
    let mut errors = Vec::new();
    if let Err(error) = body {
        errors.push(error);
    }
    if let Err(error) = cleanup {
        errors.push(error);
    }
    errors.extend(mismatches);
    assert!(errors.is_empty(), "{}", errors.join("\n\n"));
}
