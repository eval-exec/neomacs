//! Frame and startup bootstrap variables.
use crate::emacs_core::symbol::LispVariableLocality;
use crate::emacs_core::value::Value;

pub fn register_bootstrap_vars(obarray: &mut crate::emacs_core::symbol::Obarray) {
    // The seven `syms_of_frame' DEFVAR_LISPs entry 173's sweep found this port
    // short of.  `frame.o' is in GNU's unconditional `base_obj'
    // (`src/Makefile.in:450'), so no build gate is in play for any of them --
    // including the two spelled `x-', which are `syms_of_frame' names rather
    // than `syms_of_xfns' ones.
    //
    // frame.c:7748 DEFVAR_LISP, `expose_hidden_buffer = Qnil', followed by
    // `Fmake_variable_buffer_local (Qexpose_hidden_buffer)' at `frame.c:7760'
    // -- the only one of the 49 that becomes buffer-local, so the declaration
    // is incomplete without it.
    obarray.define_lisp_variable(
        "expose-hidden-buffer",
        Value::NIL,
        LispVariableLocality::BufferLocalIfSet,
    );
    // frame.c:7762 DEFVAR_LISP.  A deny-list of parameters `frameset' must not
    // restore, assembled by three `Fcons' calls over an `#ifdef'-selected tail
    // (`frame.c:7764-7772'): `list4 (Qname, Qparent_id, Qwindow_id,
    // Qouter_window_id)' under `HAVE_X_WINDOWS' and `list3' without the last
    // element otherwise.  This port takes the four-element tail, the same
    // branch its `syms_of_xterm'/`syms_of_xfns' counterparts already follow --
    // and the choice is inert either way, because a deny-list naming a
    // parameter no frame here carries excludes nothing.
    obarray.define_special_variable(
        "frame-internal-parameters",
        Value::list(vec![
            Value::symbol("undeleted"),
            Value::symbol("cloned-from"),
            Value::symbol("frame-id"),
            Value::symbol("name"),
            Value::symbol("parent-id"),
            Value::symbol("window-id"),
            Value::symbol("outer-window-id"),
        ]),
    );
    // frame.c:7693 DEFVAR_LISP, `frame_size_history = Qnil'.  nil is not
    // "unused": `frame_size_history_extra' only records when the value is a
    // cons whose car is a positive fixnum, so nil is GNU's "off".
    obarray.define_special_variable("frame-size-history", Value::NIL);
    // frame.c:7502 DEFVAR_LISP, `Vmove_frame_functions = Qnil'.  GNU reports
    // `(x-dnd-after-move-frame)' even in `--batch' because
    // `lisp/x-dnd.el:625' runs `add-hook' at load time, not because the C
    // initializer says so.
    obarray.define_c_hook_variable("move-frame-functions");
    // frame.c:7893 DEFVAR_LISP, `Vtoolkit_theme = Qnil'.
    obarray.define_special_variable("toolkit-theme", Value::NIL);
    // frame.c:7395 / 7407 DEFVAR_LISP: `Vx_resource_name = Qnil' and
    // `Vx_resource_class = build_string (EMACS_CLASS)', where `EMACS_CLASS' is
    // `"Emacs"' (`src/frame.h:1840').  The class is an X-resource lookup key,
    // not a product string, so it is GNU's literal rather than this port's
    // display branding; `x-resource-name' stays nil until a display connection
    // sets it, which is also what GNU answers in `--batch'.
    obarray.define_special_variable("x-resource-name", Value::NIL);
    obarray.define_special_variable("x-resource-class", Value::string("Emacs"));
    obarray.set_symbol_value("default-frame-alist", Value::NIL);
    // GNU frame.c exposes this as a built-in variable. GUI builds default to a
    // concrete side instead of leaving scroll-bar.el to trip over an unbound var.
    // frame.c:7451 DEFVAR_LISP; GUI toolkit builds (GTK/NS/W32) init Qright.
    obarray.define_special_variable("default-frame-scroll-bars", Value::symbol("right"));
    obarray.set_symbol_value("initial-frame-alist", Value::NIL);
    // dispnew.c:7508 DEFVAR_LISP, zero-init nil; startup assigns the real one.
    obarray.define_special_variable("initial-window-system", Value::NIL);
    // `x-display-name' is NOT declared here, and the comment that used to sit
    // on this line is why: it read "GNU graphical builds load
    // term/common-win.el during loadup, which binds this public display
    // variable even for batch sessions", and then declared the variable in
    // Rust instead of loading the file.  GNU has no C `DEFVAR' for the name --
    // `grep -rn 'x-display-name' src/' finds nothing, and `symbol-file' in a
    // GNU image answers `term/common-win.elc' -- so a Rust declaration was a
    // reimplementation of Lisp we already ship, which is the one thing this
    // port does not do.  It also produced a state GNU never has: bound with no
    // documentation, because a declaration carries no docstring.
    // `lisp/loadup.el' now preloads `term/common-win' where GNU preloads it,
    // and the `defvar' at `lisp/term/common-win.el:145' both binds the name and
    // documents it.  DIVERGENCES.md 179.
    // GNU `DEFVAR_KBOARD` both installs the forwarded value and declares the
    // symbol special.  Neomacs models the selected-frame value separately,
    // but Lisp bindings must retain the same dynamic-scope contract.
    obarray.define_special_variable("window-system", Value::NIL);
    obarray.set_symbol_value("handle-args-function", Value::symbol("command-line-1"));
    obarray.set_symbol_value("handle-args-function-alist", Value::NIL);
    // resize-mini-windows is registered by xdisp::register_bootstrap_vars with
    // GNU's real pre-loadup init (nil); lisp/loadup.el:142 assigns `grow-only'
    // right after window.el is loaded, exactly like GNU.
    // GNU `syms_of_xdisp` (xdisp.c:38639-38647) assigns BOTH frame-title-format
    // and icon-title-format the same structured default: `(multiple-frames "%b"
    // ("" "%b - GNU Emacs at " system-name))`, where the inner tail's last
    // element is the `system-name` symbol (resolved at title-render time).
    //
    // Neomacs is NOT GNU Emacs: this is a DELIBERATE product-branding
    // divergence. The title bar must advertise "NEO Emacs", never "GNU Emacs".
    // We keep the structure (the `multiple-frames` form plus the trailing
    // `system-name` symbol) byte-for-byte identical to GNU and change only the
    // product name inside the literal. The oracle parity probe still locks that
    // structure: the shared normalizer canonicalizes the product name to
    // `[EMACS-PRODUCT]` on both engines, so the intentional brand difference is
    // ignored while every other part stays a parity assertion (see
    // crates/neovm-oracle-tests/src/divergence/combos/strict/modeline_lnum_fringe_windowtree.rs
    // and the EMACS-PRODUCT rule in crates/neovm-oracle-tests/src/common.rs).
    let icon_title_name_format = Value::list(vec![
        Value::string(""),
        Value::string("%b - NEO Emacs at "),
        Value::symbol("system-name"),
    ]);
    let title_format = Value::list(vec![
        Value::symbol("multiple-frames"),
        Value::string("%b"),
        icon_title_name_format,
    ]);
    // xdisp.c:38621 / 38629 DEFVAR_LISP -- special like every C DEFVAR.
    obarray.define_special_variable("frame-title-format", title_format);
    obarray.define_special_variable("icon-title-format", title_format);
    obarray.set_symbol_value("focus-follows-mouse", Value::NIL);
    // frame.c:7636 DEFVAR_LISP. GNU's GUI default when the tool bar is drawn
    // by Emacs itself (not an external GTK/NS toolkit bar) is
    // (tab-bar-lines tool-bar-lines): adding/removing those bars does not
    // implicitly resize the frame. Neomacs draws its own bars, so it takes
    // the same branch.
    obarray.define_special_variable(
        "frame-inhibit-implied-resize",
        Value::list(vec![
            Value::symbol("tab-bar-lines"),
            Value::symbol("tool-bar-lines"),
        ]),
    );
    // frame.c:7475 DEFVAR_LISP, zero-init nil; assigned at terminal init.
    obarray.define_special_variable("terminal-frame", Value::NIL);
    obarray.set_symbol_value("frameset-filter-alist", Value::NIL);
    obarray.set_symbol_value("frameset-session-filter-alist", Value::NIL);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emacs_core::eval::Context;

    /// `x-display-name` is a Lisp `defvar`, so nothing may bind it before Lisp
    /// runs.
    ///
    /// This replaces `graphical_backend_display_name_is_bound_in_batch_like_gnu`,
    /// whose name asserted a fact GNU does not have.  GNU binds the name from
    /// `lisp/term/common-win.el:145`, a `defvar` with a docstring, which its
    /// `loadup.el` preloads from every window-system branch; there is no
    /// `DEFVAR` for it anywhere in `src/`, so in GNU the moment this test
    /// models -- after the C declarations and before any `.el` -- has
    /// `x-display-name` unbound.  Declaring it here bound it with NO
    /// documentation, a combination GNU produces for this name never, and it
    /// hid the real defect: `loadup.el` was not preloading `term/common-win`
    /// at all.
    ///
    /// The dumped image's side of the same statement is
    /// `window_system_preload_test::term_common_win_is_preloaded_because_this_build_has_a_window_system`,
    /// which asserts the name is bound AND documented once loadup has run.
    ///
    /// DIVERGENCES.md 179.
    #[test]
    fn display_name_is_not_declared_before_lisp_because_gnu_has_no_c_defvar() {
        crate::test_utils::init_test_tracing();
        let eval = Context::new();

        assert_eq!(eval.obarray().symbol_value("x-display-name").copied(), None);
        assert!(!eval.obarray().is_special("x-display-name"));
    }

    /// The neighbour that IS a C `DEFVAR`, kept as the contrast.
    ///
    /// `x-resource-name` is `frame.c:7395` `DEFVAR_LISP`, under
    /// `HAVE_WINDOW_SYSTEM`, and this build has a window system -- so unlike
    /// `x-display-name` it is correctly declared before Lisp.  Having the two
    /// side by side is what makes the rule readable: the question is not
    /// whether the name starts with `x-`, it is which of GNU's two sources
    /// defines it.
    #[test]
    fn resource_name_is_declared_before_lisp_because_gnu_defvars_it_in_c() {
        crate::test_utils::init_test_tracing();
        let eval = Context::new();

        assert_eq!(
            eval.obarray().symbol_value("x-resource-name").copied(),
            Some(Value::NIL)
        );
        assert!(eval.obarray().is_special("x-resource-name"));
    }
}
