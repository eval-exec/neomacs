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
