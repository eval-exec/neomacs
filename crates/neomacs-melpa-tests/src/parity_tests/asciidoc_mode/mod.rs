use std::time::Duration;

use crate::{
    ASCIIDOC_MODE_MELPA_PIN, CachedMelpaOracle, elisp_string, extra_tree_sitter_grammar_pin,
};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod activation;
mod completion;
mod diagnostics;
mod editing;
mod font_lock;
mod navigation;
mod surface;

const ASCIIDOC_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn asciidoc_mode_oracle(source_file: &str) -> CachedMelpaOracle {
    let block_grammar_dir = extra_tree_sitter_grammar_pin("asciidoc")
        .and_then(|pin| pin.prepare())
        .expect("read the pinned AsciiDoc block grammar from the prefetched cache");
    let inline_grammar_dir = extra_tree_sitter_grammar_pin("asciidoc-inline")
        .and_then(|pin| pin.prepare())
        .expect("read the pinned AsciiDoc inline grammar from the prefetched cache");
    let block_grammar_dir = elisp_string(&block_grammar_dir.to_string_lossy());
    let inline_grammar_dir = elisp_string(&inline_grammar_dir.to_string_lossy());

    CachedMelpaOracle::new(ASCIIDOC_MODE_MELPA_PIN, source_file)
        .expect("prepare pinned asciidoc-mode source below ./tmp")
        .with_prelude(format!(
            "(setq treesit-extra-load-path (list {block_grammar_dir} {inline_grammar_dir}))"
        ))
        .with_timeout(ASCIIDOC_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed asciidoc-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_asciidoc_mode_autoload_parity` cases (2a).
pub(crate) fn assert_asciidoc_mode_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        asciidoc_mode_oracle("asciidoc-mode-autoloads.el"),
        &name,
        "asciidoc_mode_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_asciidoc_mode_parity` cases (2a).
pub(crate) fn assert_asciidoc_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        asciidoc_mode_oracle("asciidoc-mode.el"),
        &name,
        "asciidoc_mode_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn asciidoc_mode_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [surface::surface_asciidoc_mode_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_asciidoc_mode_autoload_batch(&cases);
}

#[test]
fn asciidoc_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        activation::activation_public_surface_batch_cases(),
        completion::completion_public_surface_batch_cases(),
        diagnostics::diagnostics_public_surface_batch_cases(),
        editing::editing_public_surface_batch_cases(),
        font_lock::font_lock_public_surface_batch_cases(),
        navigation::navigation_public_surface_batch_cases(),
        surface::surface_asciidoc_mode_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_asciidoc_mode_batch(&cases);
}

// END generated package batch tests
