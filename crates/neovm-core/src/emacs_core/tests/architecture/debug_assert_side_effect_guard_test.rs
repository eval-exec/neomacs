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
//! method call named in [`STATE_CHANGING_METHODS`], or starting with one of the
//! crate's mutator prefixes in [`STATE_CHANGING_PREFIXES`] (`note_`, `set_`,
//! `enqueue_`, ...), is a violation. The fix is always the same: perform the
//! call in a `let`, then assert on the result.

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use syn::visit::{self, Visit};

use super::architecture_production_source_roots::production_source_files;

/// Container, cell, lock, channel, iterator and atomic methods that change the
/// receiver's (or a shared) state.
const STATE_CHANGING_METHODS: &[&str] = &[
    "advance",
    "append",
    "borrow_mut",
    "clear",
    "compare_exchange",
    "compare_exchange_weak",
    "dedup",
    "drain",
    "entry",
    "extend",
    "fetch_add",
    "fetch_and",
    "fetch_max",
    "fetch_min",
    "fetch_nand",
    "fetch_or",
    "fetch_sub",
    "fetch_update",
    "fetch_xor",
    "fill",
    "get_or_insert",
    "get_or_insert_with",
    "insert",
    "invalidate",
    "lock",
    "next",
    "or_default",
    "or_insert",
    "or_insert_with",
    "pop",
    "push",
    "recv",
    "remove",
    "replace",
    "resize",
    "retain",
    "reverse",
    "send",
    "set",
    "sort",
    "sort_unstable",
    "split_off",
    "store",
    "swap",
    "take",
    "truncate",
    "try_lock",
    "try_recv",
];

/// The crate names its mutators by convention (`push_back`, `pop_ready`,
/// `remove_key`, `set_wait`, `store_plist_head`, `note_root_overwrite`,
/// `record_heap_write`, `enqueue_ready`, ...). Getters use other prefixes:
/// `mark_phase()` and `alloc_regions_open()` are pure queries and stay allowed.
const STATE_CHANGING_PREFIXES: &[&str] = &[
    "acquire_",
    "advance_",
    "enqueue_",
    "insert_",
    "install_",
    "note_",
    "notify_",
    "pop_",
    "push_",
    "record_",
    "register_",
    "release_",
    "remove_",
    "reset_",
    "retire_",
    "set_",
    "store_",
    "swap_",
    "take_",
    "unregister_",
    "wake_",
];

/// Prefix matches that are queries, not mutations.
const QUERY_EXCEPTIONS: &[&str] = &["take_while"];

fn is_state_changing(name: &str) -> bool {
    STATE_CHANGING_METHODS.contains(&name)
        || (STATE_CHANGING_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
            && !QUERY_EXCEPTIONS.contains(&name))
}

const DEBUG_ASSERTIONS: &[&str] = &["debug_assert", "debug_assert_eq", "debug_assert_ne"];

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
                if arguments.delimiter() == Delimiter::Parenthesis && is_state_changing(&name) {
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
    let files = production_source_files();
    assert!(
        files.len() > 500,
        "the scan found only {} source files; check the production source roots",
        files.len()
    );
    let mut violations = Vec::new();
    for file in &files {
        let relative = file.workspace_relative();
        let source = std::fs::read_to_string(&file.path)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.path.display()));
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
            debug_assert!(state.note_root_overwrite(5));
            debug_assert!(
                v.iter()
                    .copied()
                    .collect::<Vec<_>>()
                    .is_empty()
                    || {
                        v.retain(|x| *x > 0);
                        true
                    }
            );
            debug_assert!(state.mark_phase().is_idle());
            debug_assert!(v.iter().take_while(|x| **x > 0).count() < 9);
        }
    "#;
    let parsed = syn::parse_file(source).expect("fixture parses");
    let mut scan = DebugAssertionScan {
        relative: "fixture.rs",
        violations: Vec::new(),
    };
    scan.visit_file(&parsed);
    assert_eq!(scan.violations.len(), 4, "{:#?}", scan.violations);
    assert!(scan.violations[0].contains("`.remove(..)`"));
    assert!(scan.violations[1].contains("`.pop(..)`"));
    assert!(scan.violations[2].contains("`.note_root_overwrite(..)`"));
    assert!(scan.violations[3].contains("`.retain(..)`"));
}
