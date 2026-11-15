use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;
use std::time::Duration;

use crate::{
    ASTRO_TS_MODE_MELPA_PIN, CachedMelpaOracle, elisp_string, extra_tree_sitter_grammar_pin,
};
use expect_test::expect;

mod activation;
mod editing;
mod prefix;
mod registry;
mod workflows;

const ASTRO_TS_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn astro_ts_mode_oracle(source_file: &str) -> CachedMelpaOracle {
    let astro_grammar_dir = extra_tree_sitter_grammar_pin("astro")
        .and_then(|pin| pin.prepare())
        .expect("read the pinned Astro grammar from the prefetched cache");
    let css_grammar_dir = extra_tree_sitter_grammar_pin("css")
        .and_then(|pin| pin.prepare())
        .expect("read the pinned CSS grammar from the prefetched cache");
    let tsx_grammar_dir = extra_tree_sitter_grammar_pin("tsx")
        .and_then(|pin| pin.prepare())
        .expect("read the pinned TSX grammar from the prefetched cache");
    let astro_grammar_dir = elisp_string(&astro_grammar_dir.to_string_lossy());
    let css_grammar_dir = elisp_string(&css_grammar_dir.to_string_lossy());
    let tsx_grammar_dir = elisp_string(&tsx_grammar_dir.to_string_lossy());

    CachedMelpaOracle::new(ASTRO_TS_MODE_MELPA_PIN, source_file)
        .expect("prepare pinned astro-ts-mode source below ./tmp")
        .with_prelude(format!(
            "(setq treesit-extra-load-path (list {astro_grammar_dir} {css_grammar_dir} {tsx_grammar_dir}))"
        ))
        .with_timeout(ASTRO_TS_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed astro-ts-mode parity test")
        .into()
}

fn astro_ts_mode_harness_contract_reports_all_three_pinned_grammars_and_package() -> ParityBatchCase
{
    ParityBatchCase::value(
        "astro_ts_mode_harness_contract_reports_all_three_pinned_grammars_and_package",
        r##"(list
          (featurep 'astro-ts-mode)
          (package-installed-p 'astro-ts-mode '(20260417 101))
          (mapcar #'treesit-language-available-p '(astro css tsx))
          (file-name-nondirectory (locate-library "astro-ts-mode"))
          (package-version-join
           (package-desc-version
            (cadr (assq 'astro-ts-mode package-alist)))))"##,
        expect![[r#"OK (t t (t t t) "astro-ts-mode.el" "20260417.101")"#]],
    )
}

/// Multi-probe batch for `assert_astro_ts_mode_autoload_signal_parity` cases (2a).
pub(crate) fn assert_astro_ts_mode_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        astro_ts_mode_oracle("astro-ts-mode-autoloads.el"),
        &name,
        "astro_ts_mode_autoload_signal_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_astro_ts_mode_parity` cases (2a).
pub(crate) fn assert_astro_ts_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        astro_ts_mode_oracle("astro-ts-mode.el"),
        &name,
        "astro_ts_mode_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn astro_ts_mode_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_astro_ts_mode_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_astro_ts_mode_autoload_batch(&cases);
}

#[test]
fn astro_ts_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        vec![astro_ts_mode_harness_contract_reports_all_three_pinned_grammars_and_package()],
        activation::activation_public_surface_batch_cases(),
        editing::editing_public_surface_batch_cases(),
        prefix::prefix_public_surface_batch_cases(),
        registry::registry_astro_ts_mode_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_astro_ts_mode_batch(&cases);
}

// END generated package batch tests
