//! C-level DEFVAR seeding moved below the display layer (stage-4 K2 display).
//!
//! The evaluator bootstrap used to call the display modules' `syms_of_*`
//! equivalents; these bodies only touch the Obarray and lower-crate
//! vocabulary, so they live with the bootstrap that runs them.

pub(crate) fn register_bootstrap_vars(obarray: &mut neovm_value::symbol::Obarray) {
    use neovm_value::value::Value;

    // window.c:9541 DEFVAR_LISP,
    // `window_dead_windows_table = CALLN (Fmake_hash_table, QCweakness, Qvalue)'.
    // The weakness is the point: `record_killed_window' puts every dead window
    // in here so `window-restore-killed-buffer-windows' can find it again, and
    // a strong table would keep every window ever killed alive.  An `eql' table
    // with `:weakness value' is what GNU builds; nil would not be a weaker
    // version of it, it would signal on the first `puthash'.
    obarray.define_special_variable(
        "window-dead-windows-table",
        Value::hash_table_with_options(
            neovm_value::value::HashTableTest::Eql,
            0,
            Some(neovm_value::value::HashTableWeakness::Value),
            1.5,
            0.8125,
        ),
    );
    // window.c:9483 — DEFVAR_LISP
    obarray.seed_value(
        "window-persistent-parameters",
        Value::list(vec![Value::cons(Value::symbol("clone-of"), Value::T)]),
    );
    obarray.seed_value("recenter-redisplay", Value::symbol("tty"));
    obarray.seed_value("window-restore-killed-buffer-windows", Value::NIL);
    obarray.seed_value("window-combination-resize", Value::NIL);
    obarray.seed_value("window-combination-limit", Value::symbol("window-size"));
    for name in [
        "window-persistent-parameters",
        "recenter-redisplay",
        "window-restore-killed-buffer-windows",
        "window-combination-resize",
        "window-combination-limit",
    ] {
        obarray.make_special(name);
    }
    // GNU window.c declares all of these through DEFVAR_LISP.  Register their
    // value and dynamic-binding semantics atomically so lexical package code
    // cannot observe a bound-but-non-special hook variable.
    for name in [
        "delete-frame-functions",
        "after-delete-frame-functions",
        "window-buffer-change-functions",
        "window-size-change-functions",
        "window-selection-change-functions",
        "window-state-change-functions",
        "window-state-change-hook",
    ] {
        obarray.define_special_variable(name, Value::NIL);
    }
    // GNU window.c:9383–9398 initializes this C-owned default before Lisp
    // runs. The GNU transaction uses Fdefault_value, so a missing binding
    // must be repaired at its owner, rather than hidden by the reader. Each
    // Context's exclusive bootstrap owns this cell; no shared Lisp cache.
    // Preserve the legacy unbound projection while the policy is disabled.
    if crate::emacs_core::eval::gnu_redisplay_hooks_enabled() {
        obarray.define_special_variable("window-configuration-change-hook", Value::NIL);
    }
    obarray.seed_value("window-sides-vertical", Value::NIL);
    // `window-sides-slots` and `fit-frame-to-buffer-sizes` are deliberately NOT
    // seeded.  GNU defines both in Lisp (`window.el`, `frame.el`) with a
    // four-element default -- `(nil nil nil nil)` -- and `defvar` only assigns
    // to a *void* symbol, so a nil seed here wins permanently and leaves the
    // list-shaped value GNU's own code indexes into.  Letting the `.el` define
    // them, as the port must, keeps the value's source identical to GNU's.
    obarray.seed_value("fit-window-to-buffer-horizontally", Value::NIL);
    obarray.seed_value("fit-frame-to-buffer", Value::NIL);
    obarray.seed_value(
        "fit-frame-to-buffer-margins",
        Value::list(vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
        ]),
    );
    obarray.seed_value("window-min-height", Value::fixnum(4));
    obarray.seed_value("window-min-width", Value::fixnum(10));
    obarray.seed_value("window-safe-min-height", Value::fixnum(1));
    obarray.seed_value("window-safe-min-width", Value::fixnum(2));
    obarray.seed_value("scroll-preserve-screen-position", Value::NIL);
    // window.c:9270 DEFVAR_LISP, init nil.
    obarray.define_special_variable("window-point-insertion-type", Value::NIL);
    // window.c:9247 DEFVAR_INT, init 2.
    obarray.define_int_variable("next-screen-context-lines", 2);
    obarray.seed_value("scroll-error-top-bottom", Value::NIL);
    obarray.seed_value(
        "temp-buffer-max-height",
        Value::make_float(1.0 / 3.0), // (/ (frame-height) 3) approximation
    );
    obarray.seed_value("temp-buffer-max-width", Value::NIL);
    // `even-window-sizes' is NOT a C variable in GNU -- it is defined purely by
    // `(defcustom even-window-sizes t)' in window.el. A Rust bootstrap value
    // here would win (defcustom never overwrites an already-bound variable) and
    // shadow the .el default, so we deliberately do not seed it: neomacs's
    // window.el provides the value, matching GNU.
}
