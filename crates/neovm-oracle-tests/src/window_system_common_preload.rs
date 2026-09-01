//! Oracle guard for `term/common-win.el`, the window-system preload that is
//! not about a window system.
//!
//! GNU's `lisp/loadup.el` asks two separate questions about a build:
//!
//! * **"Is there a window system at all?"** -- `(fboundp 'x-create-frame)` at
//!   `lisp/loadup.el:291`, which gates `fringe`, `image`,
//!   `international/fontset`, `dnd` and `tool-bar`.  `x-create-frame` is
//!   `DEFUN`ed once per window system (`src/xfns.c`, `src/pgtkfns.c`,
//!   `src/nsfns.m`, `src/haikufns.c`, `src/androidfns.c`, `src/w32fns.c`), so
//!   this is the Lisp spelling of `HAVE_WINDOW_SYSTEM`.
//! * **"Which window system?"** -- six sibling branches at
//!   `lisp/loadup.el:304-362` on `(featurep 'x)`, `'haiku`, `'android`,
//!   `'w32`, `'ns` and `'pgtk`.  Each loads its own `term/FOO-win.el`, and
//!   **all six also load `term/common-win.el`** (`:308`, `:313`, `:320`,
//!   `:326`, `:349`, `:361`).  Only the `ms-dos` branch skips it, with a
//!   comment saying why (`lisp/loadup.el:341`).
//!
//! Ledger 179 found this port answering **yes** to the first and **none** to
//! the second: `x-create-frame` is registered and `loadup.el` takes GNU's
//! `HAVE_WINDOW_SYSTEM` branch, but there was no window-system branch, so the
//! one file common to all six was missing from the dump.  A GNU built
//! `--without-x --without-all` was measured to settle it -- it answers `nil`
//! to `(fboundp 'x-create-frame)` and leaves all of `common-win` unbound,
//! which is a different build from this one, not a licence for this one.
//!
//! The names below are the whole documented surface of `term/common-win.el`
//! and the two functions the dumped image was missing.  Nothing here is
//! X-specific: `x-alternatives-map` and `x-setup-function-keys` are the GUI
//! function-key translations `lisp/faces.el:2238` calls unguarded from
//! `x-create-frame-with-faces`, and `x-handle-args` is the command-line
//! handler every `term/FOO-win.el` delegates to.
//!
//! The names that ARE X-specific -- the 18 of `lisp/x-dnd.el` and the 5 of
//! `lisp/term/x-win.el` -- are deliberately **not** pinned here, because this
//! build does not provide `x` and GNU's tty-only build answers `nil` for all
//! of them.  Their guard is a `mapatoms` on the neomacs side
//! (`crates/neovm-core/src/emacs_core/window_system_preload_test.rs`), since an
//! agreement pin against an X-enabled GNU would demand the opposite.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// `term/common-win.el` is in the dumped image, with GNU's own docstrings.
///
/// `x-display-name` is the row that shows what the defect looked like from
/// Lisp.  Before ledger 179 this port answered `(x-display-name t nil)` --
/// bound, undocumented -- because `crates/neovm-core/src/emacs_core/frame_vars.rs`
/// declared the name in Rust under a comment that said GNU loads it from
/// `term/common-win.el` during loadup.  GNU has no C `DEFVAR` for it anywhere
/// in `src/`; `symbol-file` in a GNU image answers `term/common-win.elc`.  A
/// declaration carries no docstring, so the stand-in produced a
/// bound-yet-undocumented state GNU never has for this name, and it hid the
/// missing preload behind a `boundp` that looked right.
///
/// The other three were `(NAME nil nil)`.
#[test]
fn oracle_term_common_win_is_preloaded_with_gnu_docstrings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (mapcar (lambda (s)
           (list s (boundp s)
                 (let ((doc (documentation-property s 'variable-documentation)))
                   (and (stringp doc) (car (split-string doc "\n"))))))
         '(x-alternatives-map x-colors x-display-name
           emacs-save-session-functions))
 (fboundp 'x-setup-function-keys)
 (fboundp 'x-handle-args)
 (featurep 'term/common-win))"#;
    let expect = expect_test::expect![[
        r#""OK (((x-alternatives-map t \"Keymap of possible alternative meanings for some keys.\") (x-colors t \"List of basic colors available on color displays.\") (x-display-name t \"The name of the window display on which Emacs was started.\") (emacs-save-session-functions t \"Special hook run when a save-session event occurs.\")) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
