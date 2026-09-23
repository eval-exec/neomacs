use super::*;

/// GNU's `defvar_lisp_nopro` sets `declared_special` even when the symbol
/// already has forwarded storage (`src/lread.c:5266-5277`).  Runtime-image
/// rehydration must therefore repair both halves of a generated DEFVAR
/// declaration, not merely recognize that its redirect is already live.
#[test]
fn adoption_reasserts_generated_specialness_on_an_existing_forwarder() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.set_variable("data-directory", Value::string("/tmp/neomacs-data/"));
    eval.obarray_mut().make_non_special("data-directory");

    assert!(!eval.obarray().is_special("data-directory"));

    let counts = adopt(eval.obarray_mut());
    let id = crate::emacs_core::intern::lookup_interned("data-directory")
        .expect("data-directory should be interned");

    assert!(
        eval.obarray().is_special("data-directory"),
        "GNU's DEFVAR_LISP declaration must remain authoritative after rehydration; \
         redirect={:?}, counts={counts:?}",
        eval.obarray().get_by_id(id).map(|symbol| symbol.redirect())
    );
    assert_eq!(
        eval.obarray().get_by_id(id).map(|symbol| symbol.redirect()),
        Some(SymbolRedirect::Forwarded),
        "late-bound GNU DEFVAR storage must also be adopted"
    );
}

/// Re-running the pass on a live `Context` must find every row already
/// settled.
///
/// This is the guard that keeps the pass at the right point in the
/// bootstrap: it first ran with the `register_bootstrap_vars` calls, where
/// six names -- `main-thread`, `system-name`, `user-login-name`,
/// `user-full-name`, `user-real-login-name`, `operating-system-release` --
/// are not bound yet, and a re-run forwarding anything is exactly that
/// symptom.  `alias` is asserted zero because GNU's C would have forwarded
/// the name before any `defvaralias` could reach it, so a row landing
/// there is a divergence and not a state to accept.  The other three arms
/// are counts that move as the port grows a variable, so they are
/// reported rather than pinned.
#[test]
fn adoption_is_idempotent_and_leaves_no_alias_rows() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let counts = adopt(eval.obarray_mut());
    assert_eq!(
        counts.forwarded, 0,
        "the bootstrap pass ran too early and missed rows: {counts:?}"
    );
    assert_eq!(
        counts.alias, 0,
        "a GNU built-in variable is an alias here: {counts:?}"
    );
    assert_eq!(
        counts.already_forwarded + counts.localized,
        counts.already_forwarded + counts.unforwarded_but_present(),
        "{counts:?}"
    );
}

/// The `Localized` rows carry the forwarder in the BLV, GNU's `make_blv`
/// (`src/data.c:2112-2140`), so `set_internal` still refuses their unbind
/// through `blv->fwd` (`src/data.c:1723-1727`).  Measured under GNU Emacs
/// 31.0.90 `-Q --batch`, `case-fold-search` is one of them.
#[test]
fn localized_rows_still_carry_a_forwarder() {
    crate::test_utils::init_test_tracing();
    let eval = crate::emacs_core::eval::Context::new();
    let ob = eval.obarray();
    let mut localized = 0usize;
    for var in gnu_table::GNU_OBJECT_VARIABLES {
        let Some(id) = crate::emacs_core::intern::lookup_interned(var.name) else {
            continue;
        };
        let Some(sym) = ob.get_by_id(id) else {
            continue;
        };
        if sym.redirect() != SymbolRedirect::Localized {
            continue;
        }
        localized += 1;
        assert!(
            ob.blv(id).is_some_and(|blv| blv.fwd.is_some()),
            "{} is localized without a forwarder",
            var.name
        );
    }
    assert!(localized > 0, "no localized GNU-declared variable found");
}
