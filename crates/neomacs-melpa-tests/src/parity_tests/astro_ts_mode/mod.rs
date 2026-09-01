use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;
use std::time::Duration;

use crate::{
    ASTRO_TS_MODE_MELPA_PIN, CachedMelpaOracle, EmacsRuntime, elisp_string,
    prepare_cached_tree_sitter_grammar, prepare_cached_tree_sitter_grammar_from_subdirectory,
};
use expect_test::expect;

mod activation;
mod editing;
mod prefix;
mod registry;
mod workflows;

const ASTRO_TS_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const ASTRO_GRAMMAR_REPOSITORY: &str = "https://github.com/virchau13/tree-sitter-astro";
const ASTRO_GRAMMAR_REVISION: &str = "213f6e6973d9b456c6e50e86f19f66877e7ef0ee";
const CSS_GRAMMAR_REPOSITORY: &str = "https://github.com/tree-sitter/tree-sitter-css";
const CSS_GRAMMAR_REVISION: &str = "dda5cfc5722c429eaba1c910ca32c2c0c5bb1a3f";
const TYPESCRIPT_GRAMMAR_REPOSITORY: &str = "https://github.com/tree-sitter/tree-sitter-typescript";
const TYPESCRIPT_GRAMMAR_REVISION: &str = "75b3874edb2dc714fb1fd77a32013d0f8699989f";

fn astro_ts_mode_oracle(source_file: &str) -> CachedMelpaOracle {
    let gnu_emacs = EmacsRuntime::gnu_emacs();
    let astro_grammar_dir = prepare_cached_tree_sitter_grammar(
        &gnu_emacs,
        "astro",
        ASTRO_GRAMMAR_REPOSITORY,
        ASTRO_GRAMMAR_REVISION,
    )
    .expect("prepare pinned Astro grammar below ./tmp");
    let css_grammar_dir = prepare_cached_tree_sitter_grammar(
        &gnu_emacs,
        "css",
        CSS_GRAMMAR_REPOSITORY,
        CSS_GRAMMAR_REVISION,
    )
    .expect("prepare pinned CSS grammar below ./tmp");
    let tsx_grammar_dir = prepare_cached_tree_sitter_grammar_from_subdirectory(
        &gnu_emacs,
        "tsx",
        TYPESCRIPT_GRAMMAR_REPOSITORY,
        TYPESCRIPT_GRAMMAR_REVISION,
        "tsx/src",
    )
    .expect("prepare pinned TSX grammar below ./tmp");
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
