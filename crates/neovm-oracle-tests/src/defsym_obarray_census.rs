//! Full obarray census: every compiled GNU `DEFSYM` spelling must name a
//! symbol in the runtime image.
//!
//! The core census (`emacs_core::gnu_defsym_symbols_test`) runs on a fresh
//! `Context`; this row runs the shipped binary with its pdump, which is where
//! the dump-construction surface normalizer can drop a spelling after the
//! seeder put it in. It reported nine of them missing - `call`, `ch`,
//! `conses`, `current`, `d`, `entry`, `intervals`, `p`, `strings` - because
//! the normalizer only exempted the fixed-id `WellKnown` table.
//!
//! The names are read from the checked-in ledger at test-build time and always
//! queried as strings, never as quoted symbols: the reader interns a quoted
//! symbol before `intern-soft` runs and would hide the bug.

use crate::common::assert_oracle_parity_expect;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// The generator's ledger, embedded from the workspace.
const LEDGER: &str = include_str!("../../neovm-value/src/intern/gnu_defsyms.snapshot.tsv");

/// The profile this test binary's runtime is built for. Non-macOS,
/// non-Windows builds use the Linux object list, matching the generated
/// table's `CURRENT_PROFILE`.
fn current_profile() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// Every spelling the ledger says this profile interns at boot.
///
/// `nil` is excluded: `(intern-soft "nil")` returns the symbol `nil`, which is
/// indistinguishable from "not found", so the probe cannot observe it.
fn active_spellings() -> Vec<&'static str> {
    let profile = current_profile();
    let mut names = Vec::new();
    for line in LEDGER.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split('\t');
        let (Some(name), Some(gate)) = (fields.next(), fields.next()) else {
            continue;
        };
        if gate == "uninterned" || gate == "none" || name == "nil" {
            continue;
        }
        if gate.split('+').any(|part| part == profile) {
            names.push(name);
        }
    }
    names
}

fn elisp_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            other if other.is_ascii() => out.push(other),
            other => panic!("non-ASCII DEFSYM spelling {other:?} needs an Elisp escape"),
        }
    }
    out.push('"');
    out
}

fn census_form() -> String {
    let names = active_spellings();
    let list = names
        .iter()
        .map(|name| elisp_string(name))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "(let ((names '({list})))\n\
         \x20 (let ((missing (delq nil (mapcar (lambda (name) (unless (intern-soft name) name)) names))))\n\
         \x20   (list (length names) (length missing) missing)))"
    )
}

/// Every active ledger spelling names a symbol in the dumped image, and GNU
/// agrees the set has no gaps.
#[test]
fn oracle_prop_every_compiled_defsym_spelling_is_in_the_runtime_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The count is part of the pinned expectation; update it only together
    // with a regenerated ledger. It is host-keyed because the active set is
    // the build profile's `defsym_name[]`: 1811 Linux, 1703 macOS, 1855
    // Windows rows carry that host's gate in `gnu_defsyms.snapshot.tsv`.
    // Linux and macOS counts are observed against the attested oracle
    // (both editors answer zero missing); the Windows count is derived from
    // the ledger and has no observed run yet.
    #[cfg(windows)]
    let expect = expect_test::expect![[r#""OK (1855 0 nil)""#]];
    #[cfg(all(not(windows), target_os = "macos"))]
    let expect = expect_test::expect![[r#""OK (1703 0 nil)""#]];
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let expect = expect_test::expect![[r#""OK (1811 0 nil)""#]];
    assert_oracle_parity_expect(&census_form(), expect);
}
