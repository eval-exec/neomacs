//! Buffer-related bootstrap variables.
use neovm_value::value::Value;

pub fn register_bootstrap_vars(obarray: &mut neovm_value::symbol::Obarray) {
    // buffer.c:5987 DEFVAR_LISP, `Vclone_indirect_buffer_hook = Qnil'.
    obarray.define_c_hook_variable("clone-indirect-buffer-hook");
    // buffer.c:5994 DEFVAR_LISP, `XSETFASTINT (Vlong_line_threshold, 50000)'.
    // A `DEFVAR_LISP' holding a fixnum, not a `DEFVAR_INT': `syms_of_buffer'
    // gives it a Lisp value cell so Lisp can set it to nil, which is what
    // turns the long-line optimizations off (`src/buffer.c:5994-6006' documents
    // nil as the disabling value).  The three companions this port already
    // declares -- `long-line-optimizations-region-size',
    // `long-line-optimizations-bol-search-limit', `large-hscroll-threshold' --
    // really are `DEFVAR_INT' and live in the `syms_of_buffer' block in
    // `eval.rs'.  Neomacs's redisplay does not consult the threshold yet; the
    // declaration is what GNU's `boundp', `special-variable-p' and
    // `documentation-property' all answer from.
    obarray.define_special_variable("long-line-threshold", Value::fixnum(50000));
    obarray.seed_value("kill-buffer-query-functions", Value::NIL);
    obarray.seed_value("kill-buffer-hook", Value::NIL);
    obarray.seed_value("buffer-list-update-hook", Value::NIL);
    obarray.seed_value("change-major-mode-hook", Value::NIL);
    obarray.seed_value("after-change-major-mode-hook", Value::NIL);
    obarray.seed_value("first-change-hook", Value::NIL);
    obarray.seed_value("before-change-functions", Value::NIL);
    obarray.seed_value("after-change-functions", Value::NIL);
    obarray.seed_value("combine-after-change-calls", Value::NIL);
    // GNU `src/buffer.c` defines this with DEFVAR_LISP and initializes it to nil.
    obarray.seed_value("inhibit-read-only", Value::NIL);
    for name in [
        "first-change-hook",
        "before-change-functions",
        "after-change-functions",
        "combine-after-change-calls",
        "inhibit-read-only",
        "inhibit-modification-hooks",
        // GNU declares these hook variables with DEFVAR_LISP (buffer.c) or
        // `defvar' (subr.el/simple.el), which makes them special.  Without the
        // special mark, a `let'-binding of the hook under lexical-binding (as
        // the oracle harness uses via `eval FORM t') creates an invisible
        // lexical binding, so `run-hooks' reads the global value and the hook
        // never fires.  Marking them special restores GNU's dynamic-binding
        // semantics for `let'-bound hooks.
        "kill-buffer-query-functions",
        "kill-buffer-hook",
        "buffer-list-update-hook",
        "change-major-mode-hook",
        "after-change-major-mode-hook",
    ] {
        obarray.make_special(name);
    }
    // GNU Emacs marks this property in `syms_of_buffer` so changing major
    // modes does not discard a buffer's cleanup hook.
    obarray
        .put_property("kill-buffer-hook", "permanent-local", Value::T)
        .expect("bootstrap symbol properties must be writable");
    // The other properties `syms_of_buffer` puts (src/buffer.c): the values that
    // the `vertical-scroll-bar` and `overwrite-mode` buffer variables can take and
    // the range of a `fraction`, which Lisp such as `cus-edit` and GNU's
    // data-tests read, and the `protected-field` error.
    for (symbol, property, value) in [
        (
            "vertical-scroll-bar",
            "choice",
            Value::list(vec![
                Value::NIL,
                Value::T,
                Value::symbol("left"),
                Value::symbol("right"),
            ]),
        ),
        (
            "fraction",
            "range",
            Value::cons(Value::make_float(0.0), Value::make_float(1.0)),
        ),
        (
            "overwrite-mode",
            "choice",
            Value::list(vec![
                Value::NIL,
                Value::symbol("overwrite-mode-textual"),
                Value::symbol("overwrite-mode-binary"),
            ]),
        ),
        (
            "protected-field",
            "error-conditions",
            Value::list(vec![
                Value::symbol("protected-field"),
                Value::symbol("error"),
            ]),
        ),
        (
            "protected-field",
            "error-message",
            Value::string("Attempt to modify a protected field"),
        ),
    ] {
        obarray
            .put_property(symbol, property, value)
            .expect("bootstrap symbol properties must be writable");
    }
    for name in [
        "inhibit-field-text-motion",
        "buffer-access-fontify-functions",
        "buffer-access-fontified-property",
    ] {
        obarray.seed_value(name, Value::NIL);
        obarray.make_special(name);
    }
    obarray.seed_value("buffer-file-coding-system", Value::NIL);
    obarray.seed_value("buffer-file-format", Value::NIL);
    obarray.seed_value("buffer-saved-size", Value::fixnum(0));
    obarray.seed_value(
        "buffer-auto-save-file-format",
        Value::list(vec![Value::symbol("t")]),
    );
    // `buffer-stale-function` is deliberately NOT seeded: GNU defines it in
    // Lisp -- `lisp/files.el:7087` spells it `(defvar buffer-stale-function
    // #'buffer-stale--default-function ...)` -- and `defvar` only assigns to a
    // *void* symbol, so a nil seed would win permanently and leave every
    // stale-buffer check without the default function GNU gives it.
    obarray.seed_value("buffer-undo-list", Value::NIL);
    obarray.seed_value("buffer-display-table", Value::NIL);
    obarray.seed_value("enable-multibyte-characters", Value::T);
    // `default-enable-multibyte-characters' was removed in Emacs 31 (only a
    // comment in mule-cmds.el mentions it); GNU leaves it unbound, so neomacs
    // must not seed it.
    obarray.seed_value("find-file-hook", Value::NIL);
    obarray.seed_value("find-file-not-found-functions", Value::NIL);
    obarray.seed_value("major-mode", Value::symbol("fundamental-mode"));
    obarray.seed_value("mode-name", Value::string("Fundamental"));
    obarray.seed_value("local-abbrev-table", Value::NIL);
    obarray.seed_value("fill-column", Value::fixnum(70));
    obarray.seed_value("left-margin", Value::fixnum(0));
    // tab-width is set by init_indent_vars() with special=true
    obarray.seed_value("ctl-arrow", Value::T);
    obarray.seed_value("truncate-lines", Value::NIL);
    obarray.seed_value("word-wrap", Value::NIL);
    // GNU `xdisp.c` defines these as nil, buffer-local display variables.
    obarray.seed_value("wrap-prefix", Value::NIL);
    obarray.seed_value("line-prefix", Value::NIL);
    obarray.seed_value("selective-display", Value::NIL);
    obarray.seed_value("selective-display-ellipses", Value::T);
    obarray.seed_value("indicate-empty-lines", Value::NIL);
    obarray.seed_value("indicate-buffer-boundaries", Value::NIL);
    obarray.seed_value("fringe-indicator-alist", Value::NIL);
    obarray.seed_value("fringe-cursor-alist", Value::NIL);
    obarray.seed_value("scroll-up-aggressively", Value::NIL);
    obarray.seed_value("scroll-down-aggressively", Value::NIL);
    obarray.seed_value("auto-fill-function", Value::NIL);
    obarray.seed_value("buffer-display-count", Value::fixnum(0));
    obarray.seed_value("buffer-display-time", Value::NIL);
}
