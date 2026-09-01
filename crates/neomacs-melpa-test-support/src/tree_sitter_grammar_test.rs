use super::{EmacsRuntime, prepare_cached_tree_sitter_grammar};

#[test]
fn cached_tree_sitter_grammar_rejects_an_unpinned_revision_before_io() {
    let error = prepare_cached_tree_sitter_grammar(
        &EmacsRuntime::new("unused", "missing-emacs"),
        "rust",
        "https://github.com/tree-sitter/tree-sitter-rust",
        "main",
    )
    .expect_err("a branch name must not identify a benchmark grammar");

    assert!(error.contains("full revision"));
    assert!(error.contains("main"));
}
