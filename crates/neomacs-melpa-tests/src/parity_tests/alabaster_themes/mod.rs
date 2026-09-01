use std::time::Duration;

use crate::{ALABASTER_THEMES_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod faces;
mod lifecycle;
mod palettes;
mod registry;
mod rendering;
mod workflows;

const ALABASTER_THEMES_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const TRUE_COLOR_PRELUDE: &str = r##"
(require 'cl-lib)
(fset 'display-color-cells
      (lambda (&optional _display) 16777216))
"##;

fn alabaster_themes_oracle(source_file: &str, prelude: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALABASTER_THEMES_MELPA_PIN, source_file)
        .expect("prepare pinned alabaster-themes source below ./tmp")
        .with_prelude(format!("{TRUE_COLOR_PRELUDE}\n{prelude}"))
        .with_timeout(ALABASTER_THEMES_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed alabaster-themes parity test")
        .into()
}

/// Multi-probe batch for `assert_alabaster_themes_autoload_parity` cases (2a).
pub(crate) fn assert_alabaster_themes_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        alabaster_themes_oracle("alabaster-themes-autoloads.el", ""),
        &name,
        "alabaster_themes_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_alabaster_themes_parity` cases (2a).
pub(crate) fn assert_alabaster_themes_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        alabaster_themes_oracle("alabaster-themes.el", ""),
        &name,
        "alabaster_themes_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn alabaster_themes_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_alabaster_themes_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_alabaster_themes_autoload_batch(&cases);
}

#[test]
fn alabaster_themes_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        faces::faces_public_surface_batch_cases(),
        lifecycle::lifecycle_public_surface_batch_cases(),
        palettes::palettes_public_surface_batch_cases(),
        registry::registry_alabaster_themes_batch_cases(),
        rendering::rendering_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_alabaster_themes_batch(&cases);
}

// END generated package batch tests
