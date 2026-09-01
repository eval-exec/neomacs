use std::time::Duration;

use crate::{ALL_THE_ICONS_IVY_RICH_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod annotations;
mod files;
mod icons;
mod processes;
mod surface;

const ALL_THE_ICONS_IVY_RICH_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn all_the_icons_ivy_rich_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(
        ALL_THE_ICONS_IVY_RICH_MELPA_PIN,
        "all-the-icons-ivy-rich.el",
    )
    .expect("prepare pinned all-the-icons-ivy-rich and its dependencies below ./tmp")
    .with_timeout(ALL_THE_ICONS_IVY_RICH_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed all-the-icons-ivy-rich parity test")
        .into()
}

/// Multi-probe batch for `assert_all_the_icons_ivy_rich_parity` cases (2a).
pub(crate) fn assert_all_the_icons_ivy_rich_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        all_the_icons_ivy_rich_oracle(),
        &name,
        "all_the_icons_ivy_rich_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn all_the_icons_ivy_rich_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        annotations::annotations_public_surface_batch_cases(),
        files::files_public_surface_batch_cases(),
        icons::icons_public_surface_batch_cases(),
        processes::processes_public_surface_batch_cases(),
        surface::surface_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_all_the_icons_ivy_rich_batch(&cases);
}

// END generated package batch tests
