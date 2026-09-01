//! GNU declares three names with a `DEFVAR_*` and then explicitly takes the
//! `declared_special` flag back off again.
//!
//! `DEFVAR_LISP` implies `declared_special` everywhere else -- GNU's
//! `defvar_lisp_nopro` sets it unconditionally (`src/lread.c:5274`) -- which is
//! why `defvar_object::adopt` can set it from a table scraped out of the
//! `DEFVAR` heads.  These are the whole of the exception in GNU's `src/`, and
//! GNU spells the un-declaration TWO different ways:
//!
//! * `features` -- `Fmake_var_non_special (Qfeatures);` (`src/fns.c:6823`,
//!   declared at `src/fns.c:6817`), under the comment "Let people use
//!   lexically scoped vars named `features'."
//! * `top-level` -- `XSYMBOL (Qtop_level)->u.s.declared_special = false;`
//!   (`src/keyboard.c:13955`, declared at `src/keyboard.c:13951`).
//! * `values` -- `XBARE_SYMBOL (intern ("values"))->u.s.declared_special =
//!   false;` (`src/lread.c:5596`, declared at `src/lread.c:5592`).
//!
//! The other two `declared_special = false` sites in GNU are not exceptions of
//! this shape and must not be read as ones: `src/alloc.c:3672` initialises a
//! freshly allocated symbol (`p->u.s.`, a C pointer, not a named symbol) and
//! `src/eval.c:1071` is the BODY of `internal-make-var-non-special` itself,
//! clearing the flag on whatever symbol it was handed at runtime.  Neither
//! names a symbol, which is exactly what distinguishes them.
//!
//! The Lisp-visible consequence is scoping: under `lexical-binding`, a `let`
//! of a special name rebinds the global dynamically, and a `let` of a
//! non-special name makes an ordinary lexical variable that the global never
//! sees.  For `features` that difference is load-bearing -- `featurep` reads
//! the global `Vfeatures` (`src/fns.c:3731`), so if `features` were special,
//! `(let ((features '(foo))) (featurep 'foo))` would answer `t`.

mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

#[test]
fn compat_defvar_non_special_exceptions_match_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping DEFVAR non-special audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    // The three exceptions, then four controls that GNU declares with the same
    // `DEFVAR_LISP` and does NOT un-declare.  Without the controls this pin
    // would pass just as well against a port that made every name lexical.
    let form = r#"(mapcar
 (lambda (sym)
   (let ((marker (list 'marker sym)))
     (list sym
           (special-variable-p sym)
           ;; Does a `let' of this name under lexical-binding reach the
           ;; global?  t = dynamic (special), nil = lexical.
           (funcall
            (eval
             `(lambda ()
                (let ((,sym ',marker))
                  (equal (symbol-value ',sym) ',marker)))
             t)))))
 ;; `default-directory' is deliberately NOT a control: GNU type-checks it on
 ;; store (`stringp'), so binding it to a marker signals rather than reporting.
 '(features top-level values
   load-path case-fold-search debug-on-error print-length))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "DEFVAR non-special exception mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}

/// The consequence the bootstrap-runtime-state audit actually tripped over:
/// `featurep` must read the global `features`, and a lexical `let` of the name
/// `features` must not be able to forge an answer out of it.
#[test]
fn compat_lexical_features_binding_does_not_reach_featurep() {
    if !oracle_enabled() {
        eprintln!(
            "skipping lexical `features' audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(list
 (let ((features '(l176-not-really-provided)))
   (featurep 'l176-not-really-provided))
 (featurep 'l176-not-really-provided)
 ;; And the same shape the audit uses: names this build genuinely does not
 ;; provide, asked while `features' is lexically bound to exactly them.
 (let ((features '(cl-lib pcase gv)))
   (list (featurep 'cl-lib) (featurep 'pcase) (featurep 'gv))))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "lexical `features' binding mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}
