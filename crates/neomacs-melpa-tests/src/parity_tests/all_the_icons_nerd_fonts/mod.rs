use std::time::Duration;

use crate::{ALL_THE_ICONS_NERD_FONTS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod families;
mod overrides;
mod prefer;
mod registry;

const ALL_THE_ICONS_NERD_FONTS_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn all_the_icons_nerd_fonts_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALL_THE_ICONS_NERD_FONTS_MELPA_PIN, source_file)
        .expect("prepare pinned all-the-icons-nerd-fonts source below ./tmp")
        .with_timeout(ALL_THE_ICONS_NERD_FONTS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed all-the-icons-nerd-fonts parity test")
        .into()
}

/// Multi-probe batch for `assert_all_the_icons_nerd_fonts_autoload_parity` cases (2a).
pub(crate) fn assert_all_the_icons_nerd_fonts_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        all_the_icons_nerd_fonts_oracle("all-the-icons-nerd-fonts-autoloads.el"),
        &name,
        "all_the_icons_nerd_fonts_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_all_the_icons_nerd_fonts_parity` cases (2a).
pub(crate) fn assert_all_the_icons_nerd_fonts_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        all_the_icons_nerd_fonts_oracle("all-the-icons-nerd-fonts.el"),
        &name,
        "all_the_icons_nerd_fonts_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn all_the_icons_nerd_fonts_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_all_the_icons_nerd_fonts_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_all_the_icons_nerd_fonts_autoload_batch(&cases);
}

#[test]
fn all_the_icons_nerd_fonts_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        families::families_public_surface_batch_cases(),
        overrides::overrides_public_surface_batch_cases(),
        prefer::prefer_public_surface_batch_cases(),
        registry::registry_all_the_icons_nerd_fonts_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_all_the_icons_nerd_fonts_batch(&cases);
}

// END generated package batch tests
