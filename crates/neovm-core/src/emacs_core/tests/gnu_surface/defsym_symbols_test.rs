//! GNU interns every compiled `DEFSYM` spelling into the initial obarray
//! before Lisp runs.
//!
//! In GNU 31.1 `DEFSYM` is an empty placeholder for make-docfile
//! (`src/lisp.h:2395`); the spellings live in the generated `defsym_name[]`
//! array, and `init_obarray_once` (`src/lread.c:5193-5203`) interns each one
//! with no function cell, no value and an empty plist. The port used to
//! intern only the spellings its own registration path happened to name, so a
//! `DEFSYM`-only symbol was missing: the pinned oracle completes 74 names for
//! `"buffer-"`, the port 73 (`buffer-or-string-p`, `src/data.c:4084`).
//!
//! The generated table is the compiled DEFSYM set for this platform (see
//! `cargo xtask gnu-defsyms`); this file is the in-suite census.

#[path = "../../../../../neovm-value/src/intern/gnu_defsyms.generated.rs"]
mod gnu_defsyms_generated;

use neovm_value::value::{Value, list_to_vec};

/// Every spelling this platform's GNU build compiles must name a symbol.
#[test]
fn every_compiled_gnu_defsym_spelling_is_a_boot_symbol() {
    let eval = crate::test_utils::new_context();
    let mut missing: Vec<&str> = Vec::new();
    let mut checked = 0usize;
    for (name, gate) in gnu_defsyms_generated::GNU_DEFSYMS {
        if !gate.is_active() {
            continue;
        }
        checked += 1;
        if eval.obarray().intern_soft(name).is_none() {
            missing.push(name);
        }
    }
    assert!(
        checked > 1000,
        "the generated table should hold over a thousand spellings for this \
         platform, got {checked}"
    );
    assert!(
        missing.is_empty(),
        "{} of {checked} compiled GNU DEFSYM spellings are not in the obarray \
         (run `cargo xtask gnu-defsyms` if the table is stale): {:?}",
        missing.len(),
        &missing[..missing.len().min(20)]
    );
}

/// Every literal `intern_c_string`/`intern` startup spelling this platform's
/// GNU sources compile must name a symbol, with no value and no function.
///
/// These are GNU's own `syms_of_*` interning calls (`Fprovide`, cursor and
/// DND tables, feature names, ...); no `DEFSYM` row covers them, so the
/// generated C-intern ledger is a second boot table.
#[test]
fn every_active_c_intern_spelling_is_a_boot_member() {
    let eval = crate::test_utils::new_context();
    let mut checked = 0usize;
    let mut missing: Vec<&str> = Vec::new();
    for (name, gate) in gnu_defsyms_generated::GNU_C_INTERNS {
        if !gate.is_active() {
            continue;
        }
        checked += 1;
        if eval.obarray().intern_soft(name).is_none() {
            missing.push(name);
        }
    }
    assert!(
        checked > 50,
        "the generated C-intern table should hold over fifty active spellings, got {checked}"
    );
    assert!(
        missing.is_empty(),
        "{} of {checked} active C-intern spellings are not in the obarray \
         (run `cargo xtask gnu-defsyms` if the table is stale): {:?}",
        missing.len(),
        &missing[..missing.len().min(20)]
    );
    // The other profiles' literals stay out, matching GNU's defsym_name[] /
    // compiled object split (`kqueue.c` is macOS, `w32notify.c` Windows).
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        assert!(eval.obarray().intern_soft("kqueue").is_none());
        assert!(eval.obarray().intern_soft("w32notify").is_none());
    }
}

/// A DEFSYM gives the symbol no function and no value, matching GNU.
#[test]
fn defsym_only_spellings_are_unbound_and_have_no_function() {
    let mut eval = crate::test_utils::new_context();
    // Pinned GNU 31.1 oracle: all three names answer bound=nil, fbound=nil,
    // plist=nil.
    let result = eval
        .eval_str(
            r#"(let ((names '(buffer-or-string-p AEAD-ciphers above-suspended access)))
                 (mapcar (lambda (symbol)
                           (list (null (boundp symbol))
                                 (null (fboundp symbol))
                                 (null (symbol-plist symbol))
                                 (eq (intern-soft (symbol-name symbol)) symbol)))
                         names))"#,
        )
        .expect("DEFSYM-only attribute oracle");
    assert_eq!(
        list_to_vec(&result),
        Some(vec![
            Value::list(vec![
                Value::T,
                Value::T,
                Value::T,
                Value::T
            ]);
            4
        ])
    );
}

/// GNU interns these spellings and then removes them again, or never interns
/// them at all, so Lisp must not see them at startup: the five startup
/// `Funintern`s (lread.c:5872-5873, editfns.c:4866, eval.c:4597, print.c:3063)
/// and `Qunbound`'s deliberate uninterned state in `define_symbol`. The set is
/// generated from the ledger (`GNU_UNINTERNED_SPELLINGS`) so a startup census
/// and this test consume one list.
#[test]
fn explicitly_uninterned_gnu_defsym_spellings_are_absent() {
    let eval = crate::test_utils::new_context();
    assert_eq!(
        gnu_defsyms_generated::GNU_UNINTERNED_SPELLINGS.len(),
        6,
        "the ledger's uninterned set changed; update this expectation with its GNU source"
    );
    let present: Vec<&str> = gnu_defsyms_generated::GNU_UNINTERNED_SPELLINGS
        .iter()
        .copied()
        .filter(|name| eval.obarray().intern_soft(name).is_some())
        .collect();
    assert!(
        present.is_empty(),
        "GNU uninterns these before the startup obarray is complete: {present:?}"
    );
}

/// GNU defines `file-locked` in `lisp/userlock.el` (`define-error` at
/// userlock.el:40), which loadup does not preload, so the name is not an
/// obarray symbol at startup; the port used to register it eagerly with the
/// file-error family. Pinned GNU 31.1: `(intern-soft "file-locked")` nil
/// before `(load "userlock")` and `(file-locked file-error error)` after.
#[test]
fn file_locked_is_not_an_obarray_symbol_at_boot() {
    let eval = crate::test_utils::new_context();
    assert!(
        eval.obarray().intern_soft("file-locked").is_none(),
        "file-locked must be created lazily by userlock.el, not at boot"
    );
}
