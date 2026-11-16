use crate::tree_sitter_grammar::{
    GrammarCachePaths, GrammarSourceSpec, cached_tree_sitter_grammar_in,
};
use crate::{EmacsRuntime, prefetch_cached_tree_sitter_grammar};
use std::fs;

fn spec<'a>(
    language: &'a str,
    revision: &'a str,
    source_directory: Option<&'a str>,
    source_sha256: &'a str,
) -> GrammarSourceSpec<'a> {
    GrammarSourceSpec {
        language,
        repository: "https://github.com/tree-sitter/tree-sitter-fixture",
        revision,
        source_directory,
        source_sha256,
    }
}

/// A cache entry that satisfies every check of the cache-only reader: the
/// ready marker this pin expects, a library file, and a `parser.c` hashing
/// to the pin.
fn prime_cache_entry(root: &std::path::Path, source: GrammarSourceSpec<'_>, parser_c: &str) {
    let paths = GrammarCachePaths::below(root, source);
    let parser = source.parser_source(&paths.checkout);
    fs::create_dir_all(parser.parent().unwrap()).unwrap();
    fs::write(&parser, parser_c).unwrap();
    fs::create_dir_all(&paths.grammar_directory).unwrap();
    fs::write(
        paths.grammar_directory.join("libtree-sitter-fixture.so"),
        b"not a real library",
    )
    .unwrap();
    let marker = format!(
        "{}\t{}\t{}\t{}\n",
        source.language,
        source.repository,
        source.revision,
        source.source_directory.unwrap_or("")
    );
    fs::write(&paths.ready_marker, marker).unwrap();
}

#[test]
fn prefetch_rejects_an_unpinned_revision_before_io() {
    let error = prefetch_cached_tree_sitter_grammar(
        &EmacsRuntime::new("unused", "missing-emacs"),
        "rust",
        "https://github.com/tree-sitter/tree-sitter-rust",
        "main",
        "0000000000000000000000000000000000000000000000000000000000000000",
    )
    .expect_err("a branch name must not identify a benchmark grammar");

    assert!(error.contains("full revision"));
    assert!(error.contains("main"));
}

#[test]
fn prefetch_rejects_a_non_hexadecimal_hash_pin() {
    let error = prefetch_cached_tree_sitter_grammar(
        &EmacsRuntime::new("unused", "missing-emacs"),
        "rust",
        "https://github.com/tree-sitter/tree-sitter-rust",
        "c447dcce961ac438aaeaf117347749fe7d1e8365",
        "not-a-sha256",
    )
    .expect_err("the content half of a pin must be a SHA-256");

    assert!(error.contains("SHA-256"));
}

#[test]
fn cached_read_fails_loudly_when_the_cache_is_cold() {
    let root = tempfile::tempdir().unwrap();
    let error = cached_tree_sitter_grammar_in(
        root.path(),
        "go",
        "12fe553fdaaa7449f764bc876fd777704d4fb752",
        None,
        "7aa4657c76dd66538228ddb319447225f9dddf54573e521743b4203f1d909051",
    )
    .expect_err("an empty cache must not silently skip the fixture");

    assert!(error.contains("cargo xtask treesit-grammars"));
    assert!(error.contains("go"));
    assert!(error.contains("12fe553fdaaa7449f764bc876fd777704d4fb752"));
}

#[test]
fn cached_read_returns_the_verified_entry() {
    let root = tempfile::tempdir().unwrap();
    let parser_c = "int main(void) { return 0; }\n";
    let sha = neomacs_infra::inventory::sha256_hex(parser_c.as_bytes());
    let source = spec(
        "fixture",
        "12fe553fdaaa7449f764bc876fd777704d4fb752",
        None,
        &sha,
    );
    prime_cache_entry(root.path(), source, parser_c);

    let directory = cached_tree_sitter_grammar_in(
        root.path(),
        source.language,
        source.revision,
        source.source_directory,
        source.source_sha256,
    )
    .expect("a primed, hash-matching entry satisfies the pin");
    assert!(directory.ends_with(".emacs.d/tree-sitter"));
}

#[test]
fn cached_read_rejects_tampered_parser_bytes() {
    let root = tempfile::tempdir().unwrap();
    let pinned = "int main(void) { return 0; }\n";
    let sha = neomacs_infra::inventory::sha256_hex(pinned.as_bytes());
    let source = spec(
        "fixture",
        "12fe553fdaaa7449f764bc876fd777704d4fb752",
        None,
        &sha,
    );
    prime_cache_entry(root.path(), source, "int main(void) { return 1; }\n");

    let error = cached_tree_sitter_grammar_in(
        root.path(),
        source.language,
        source.revision,
        source.source_directory,
        source.source_sha256,
    )
    .expect_err("bytes that no longer hash to the pin are not the fixture");

    assert!(error.contains("SHA-256"));
    assert!(error.contains("pinned bytes"));
}

#[test]
fn cached_read_rejects_a_marker_from_another_revision() {
    let root = tempfile::tempdir().unwrap();
    let parser_c = "int main(void) { return 0; }\n";
    let sha = neomacs_infra::inventory::sha256_hex(parser_c.as_bytes());
    let source = spec(
        "fixture",
        "12fe553fdaaa7449f764bc876fd777704d4fb752",
        None,
        &sha,
    );
    // Prime the entry for a different revision than the one being read.
    let other = spec(
        "fixture",
        "3aa2995549d5d8b26928e8d3fa2770fd4327414e",
        None,
        &sha,
    );
    prime_cache_entry(root.path(), other, parser_c);

    let error = cached_tree_sitter_grammar_in(
        root.path(),
        source.language,
        source.revision,
        source.source_directory,
        source.source_sha256,
    )
    .expect_err("a marker naming another revision does not satisfy this pin");

    assert!(error.contains("cargo xtask treesit-grammars"));
}
