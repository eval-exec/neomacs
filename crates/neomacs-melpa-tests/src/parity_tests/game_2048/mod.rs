use std::time::Duration;

use crate::{CachedMelpaOracle, GAME_2048_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod lifecycle;
mod moves;
mod state;

const GAME_2048_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn game_2048_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GAME_2048_MELPA_PIN, "2048-game.el")
        .expect("prepare pinned 2048-game source below ./tmp")
        .with_timeout(GAME_2048_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed 2048-game parity test")
        .into()
}

/// Multi-probe batch for `assert_game_2048_parity` cases (2a).
pub(crate) fn assert_game_2048_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(game_2048_oracle(), &name, "game_2048_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn game_2048_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        lifecycle::lifecycle_public_surface_batch_cases(),
        moves::moves_public_surface_batch_cases(),
        state::state_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_game_2048_batch(&cases);
}

// END generated package batch tests
