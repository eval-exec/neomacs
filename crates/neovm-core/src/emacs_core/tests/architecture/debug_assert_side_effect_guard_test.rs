//! Source guard: no state-changing call inside a debug assertion.
//!
//! `debug_assert!`, `debug_assert_eq!` and `debug_assert_ne!` evaluate their
//! arguments only when `debug_assertions` is on. A mutation written inside one
//! (`debug_assert!(set.remove(&key))`) therefore runs in debug and test builds
//! and silently disappears from release builds. The suites that exercise it
//! pass while the shipped binary keeps stale state. The green scheduler's wait
//! index had three such removals. In release builds they left retired waits
//! indexed, and the deadlock verdict raised a false "All threads are blocked".
//!
//! The scan parses every source file of the crate and walks the token trees of
//! each debug-assertion macro, so comments and string literals never match. A
//! method call named in [`STATE_CHANGING_METHODS`] is a violation. The fix is
//! always the same: perform the call in a `let`, then assert on the result.

use std::path::{Path, PathBuf};

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use syn::visit::{self, Visit};

/// Container, cell and atomic methods that change the receiver's state.
const STATE_CHANGING_METHODS: &[&str] = &[
    "append",
    "clear",
    "compare_exchange",
    "compare_exchange_weak",
    "drain",
    "entry",
    "extend",
    "fetch_add",
    "fetch_and",
    "fetch_or",
    "fetch_sub",
    "get_or_insert",
    "get_or_insert_with",
    "insert",
    "or_default",
    "or_insert",
    "or_insert_with",
    "pop",
    "pop_back",
    "pop_front",
    "push",
    "push_back",
    "push_front",
    "remove",
    "replace",
    "retain",
    "set",
    "store",
    "swap",
    "swap_remove",
    "take",
    "truncate",
];

const DEBUG_ASSERTIONS: &[&str] = &["debug_assert", "debug_assert_eq", "debug_assert_ne"];

fn source_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("read source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            source_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

/// `.name(` sequences in `tokens`, recursively through every group.
fn state_changing_calls(tokens: TokenStream, found: &mut Vec<(String, usize)>) {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    for (index, tree) in trees.iter().enumerate() {
        match tree {
            TokenTree::Group(group) => state_changing_calls(group.stream(), found),
            TokenTree::Punct(punct) if punct.as_char() == '.' => {
                let (Some(TokenTree::Ident(name)), Some(TokenTree::Group(arguments))) =
                    (trees.get(index + 1), trees.get(index + 2))
                else {
                    continue;
                };
                let name = name.to_string();
                if arguments.delimiter() == Delimiter::Parenthesis
                    && STATE_CHANGING_METHODS.contains(&name.as_str())
                {
                    found.push((name, punct.span().start().line));
                }
            }
            _ => {}
        }
    }
}

struct DebugAssertionScan<'a> {
    relative: &'a str,
    violations: Vec<String>,
}

impl<'ast> Visit<'ast> for DebugAssertionScan<'_> {
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let is_debug_assertion =
            mac.path.segments.last().is_some_and(|segment| {
                DEBUG_ASSERTIONS.contains(&segment.ident.to_string().as_str())
            });
        if is_debug_assertion {
            let mut found = Vec::new();
            state_changing_calls(mac.tokens.clone(), &mut found);
            for (name, line) in found {
                self.violations.push(format!(
                    "{}:{line}: `.{name}(..)` inside a debug assertion is skipped in release builds",
                    self.relative
                ));
            }
        }
        visit::visit_macro(self, mac);
    }
}

#[test]
fn debug_assertions_never_change_state() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    source_files(&crate_root.join("src"), &mut files);
    files.sort();
    assert!(
        files.len() > 500,
        "the scan found only {} source files; check the crate root",
        files.len()
    );
    let mut violations = Vec::new();
    for file in &files {
        let relative = file
            .strip_prefix(crate_root)
            .expect("source file is inside the crate")
            .to_string_lossy()
            .replace('\\', "/");
        let source = std::fs::read_to_string(file)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.display()));
        let parsed =
            syn::parse_file(&source).unwrap_or_else(|error| panic!("parse {relative}: {error}"));
        let mut scan = DebugAssertionScan {
            relative: &relative,
            violations: Vec::new(),
        };
        scan.visit_file(&parsed);
        violations.extend(scan.violations);
    }
    assert!(
        violations.is_empty(),
        "move each call out of the assertion (`let r = x.remove(..); debug_assert!(r)`):\n{}",
        violations.join("\n")
    );
}

#[test]
fn the_scan_flags_a_mutation_inside_a_debug_assertion() {
    let source = r#"
        fn f(set: &mut std::collections::BTreeSet<u8>, v: &mut Vec<u8>) {
            debug_assert!(set.remove(&1), "message");
            debug_assert_eq!(v.pop(), Some(2));
            let removed = set.remove(&3);
            debug_assert!(removed);
            debug_assert!(set.contains(&4));
        }
    "#;
    let parsed = syn::parse_file(source).expect("fixture parses");
    let mut scan = DebugAssertionScan {
        relative: "fixture.rs",
        violations: Vec::new(),
    };
    scan.visit_file(&parsed);
    assert_eq!(scan.violations.len(), 2, "{:#?}", scan.violations);
    assert!(scan.violations[0].contains("`.remove(..)`"));
    assert!(scan.violations[1].contains("`.pop(..)`"));
}
