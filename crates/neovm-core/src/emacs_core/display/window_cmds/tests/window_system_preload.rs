//! Standing check: which of GNU's window-system preloads does this build take?
//!
//! GNU's `lisp/loadup.el` splits the graphical preloads in two, and the split
//! is not cosmetic -- the two halves answer two different questions about the
//! build:
//!
//! 1. **"Is there a window system at all?"**  `lisp/loadup.el:291-299`
//!    (`(if (fboundp 'x-create-frame) (progn (load "fringe") (load
//!    "emacs-lisp/regexp-opt") (load "image") (load
//!    "international/fontset") (load "dnd") (load "tool-bar")))`).
//!    `x-create-frame` is `DEFUN`ed in `src/xfns.c`, `src/pgtkfns.c`,
//!    `src/nsfns.m`, `src/haikufns.c`, `src/androidfns.c` and `src/w32fns.c`,
//!    i.e. once per window system, so `fboundp` here is GNU's Lisp-level
//!    spelling of `HAVE_WINDOW_SYSTEM`.
//! 2. **"WHICH window system?"**  `lisp/loadup.el:304-362`, six sibling
//!    branches on `(featurep 'x)`, `'haiku`, `'android`, `'w32`, `'ns` and
//!    `'pgtk`.  Each provides its own `term/FOO-win.el`, and **every one of
//!    the six also loads `term/common-win.el`** -- the file whose name says
//!    what it is.  The only branch that skips it is `ms-dos`, and it says why
//!    in a comment: "Don't load term/common-win: it isn't appropriate for the
//!    `pc' ``window system'', which generally behaves like a terminal"
//!    (`lisp/loadup.el:341-343`).
//!
//! This port answers **yes** to (1): `x-create-frame` is registered, and
//! `loadup.el` takes that branch exactly as GNU does.  It answers **none** to
//! (2): there is no `neo` branch, because `term/neo-win.el` is a GUI-runtime
//! concern (it opens with `(unless (featurep 'neomacs) (error ...))`, and
//! `neomacs` is not provided in a dumped batch image).
//!
//! Measured over the 1972 names of ledger 178's deleted seed tables, against
//! GNU Emacs 31.0.90 `-Q --batch` built three ways -- `--with-x-toolkit=gtk3`,
//! `--without-x --without-all`, and this port:
//!
//! | `documentation-property` answers | GNU+X | GNU tty-only | neomacs | n |
//! | --- | --- | --- | --- | --- |
//! | | doc | doc | doc | 1856 |
//! | question (1)'s files | doc | **nil** | **doc** | **77** |
//! | question (2)'s files | doc | nil | nil | **28** |
//! | names GNU 31 no longer documents | nil | nil | nil | 11 |
//!
//! The 77 row is the load-bearing one: on every variable that separates a
//! tty-only GNU from a graphical one, **this port answers with the graphical
//! build.**  It is not a tty build, so "the tty build skips `common-win` too"
//! is not available as a defence for skipping it here.
//!
//! Ledger 179.

use crate::test_utils::runtime_startup_eval_one;

/// `term/common-win.el` is preloaded, because this build has a window system.
///
/// GNU loads it from all six window-system branches of `lisp/loadup.el`
/// (`:308`, `:313`, `:320`, `:326`, `:349`, `:361`), so a graphical GNU build
/// carries it in the dump whatever the window system is, and even in `--batch`
/// where `window-system` is nil.  This port is graphical by GNU's own
/// predicate -- `(fboundp 'x-create-frame)` is `t`, and `loadup.el` already
/// takes GNU's `HAVE_WINDOW_SYSTEM` branch on it -- so it must carry the file
/// too.
///
/// The four public variables asserted below are the ones `term/common-win.el`
/// defines with a docstring; GNU's own `symbol-file` attributes all four to
/// `term/common-win.elc`.  They are named rather than scanned out of the file
/// on purpose: this is GNU's published surface, not a table this port
/// maintains.  The anchor that cannot go green by attrition is
/// `(featurep 'term/common-win)` -- `lisp/term/common-win.el:416` ends with
/// `(provide 'term/common-win)`, so an emptied or unloaded file reports `nil`
/// here rather than an empty list of names to check.
///
/// The two functions are the reason this is a behaviour fix and not a
/// documentation one.  `x-setup-function-keys` is what installs
/// `x-alternatives-map` into `local-function-key-map`, and
/// `lisp/faces.el:2238` calls it unguarded from `x-create-frame-with-faces`;
/// `x-handle-args` is the command-line handler every `term/FOO-win.el`
/// delegates to.  Before ledger 179 neither was in the dumped image.
///
/// Ledger 179.  RED before the fix: `OK (nil ((x-alternatives-map nil nil)
/// (x-colors nil nil) (x-display-name t nil) (emacs-save-session-functions
/// nil nil)) nil nil t)` -- note `x-display-name`, bound with no
/// documentation, which is what a Rust stand-in for a Lisp `defvar` looks
/// like from Lisp.
#[test]
fn term_common_win_is_preloaded_because_this_build_has_a_window_system() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        "(list
           ;; `lisp/term/common-win.el:416'.  Not a name list: an emptied or
           ;; unloaded file answers nil here instead of vacuously passing.
           (featurep 'term/common-win)
           ;; GNU's four documented variables from that file.
           (mapcar (lambda (s)
                     (list s
                           (and (boundp s) t)
                           (and (stringp (documentation-property
                                          s 'variable-documentation t))
                                t)))
                   '(x-alternatives-map x-colors x-display-name
                     emacs-save-session-functions))
           ;; The two functions the dump was missing.
           (and (fboundp 'x-setup-function-keys) t)
           (and (fboundp 'x-handle-args) t)
           ;; ... and the predicate that says this build is entitled to all of
           ;; it: GNU's Lisp spelling of HAVE_WINDOW_SYSTEM.
           (and (fboundp 'x-create-frame) t))",
    );
    assert_eq!(
        result,
        "OK (t ((x-alternatives-map t t) (x-colors t t) (x-display-name t t) \
         (emacs-save-session-functions t t)) t t t)"
    );
}

/// The GUI terminal layer does not load `easy-mmode`, because no GNU
/// `term/*-win.el` does.
///
/// `grep -c define-minor-mode` over GNU's own window-system files answers
/// **0** for every one of the eight -- `x-win.el`, `pgtk-win.el`, `ns-win.el`,
/// `haiku-win.el`, `android-win.el`, `w32-win.el`, `pc-win.el` and
/// `common-win.el`.  The only two `define-minor-mode`s anywhere under
/// `lisp/term/` are `tvi970.el:102` and `vt100.el:41`, and both are TTY files
/// loaded at runtime and never dumped.  So a window-system file that pulls
/// `easy-mmode` in is not a GNU shape, and `lisp/term/neo-win.el` was pulling
/// it in for exactly two `:global t` modes and nothing else -- the file's only
/// reference to the library is the `require` itself.
///
/// `define-minor-mode` is a macro, and a `:global t` mode without `:keymap`
/// expands to `defcustom` / `defun` / `add-minor-mode`, none of which lives in
/// `easy-mmode`.  Measured both ways on the same fixture, in GNU 31.0.90 and
/// in this port: with `(eval-when-compile (require 'easy-mmode))` the compiled
/// file loads AND the mode toggles with `(featurep 'easy-mmode)` still nil.
///
/// Source-loading the file is unaffected, which is why the `require` is kept
/// rather than deleted: the interpreter evaluates an `eval-when-compile` body
/// at load time, so `(load "term/neo-win.el")` still has the macro.
///
/// The two anchors are what stop this from going green by attrition:
/// `(featurep 'term/neo-win)` is `lisp/term/neo-win.el:713`, so an emptied or
/// unloadable file reports nil rather than an absent library; and
/// `neomacs-scroll-indicator-mode` must be `fboundp`, so deleting the modes is
/// not a way to pass.
///
/// Ledger 194.  RED before: `OK (t t t)`.
#[test]
fn the_gui_terminal_layer_does_not_load_easy_mmode() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        "(progn
           ;; `crates/neomacs/src/main.rs:2802-2822', which is how a GUI session
           ;; reaches this file: provide the backend feature, then the two
           ;; libraries in order.
           (provide 'neomacs)
           (load \"term/common-win\" nil t)
           (load \"term/neo-win\" nil t)
           (list (featurep 'term/neo-win)
                 (featurep 'easy-mmode)
                 (fboundp 'neomacs-scroll-indicator-mode)))",
    );
    assert_eq!(result, "OK (t nil t)");
}

/// The other half of the same statement: a window system this build does NOT
/// have contributes nothing to the dumped image.
///
/// This is the guard against "fixing" ledger 179's remaining divergences the
/// wrong way.  Twenty-three of the twenty-eight names ledger 178 handed over
/// come from `lisp/x-dnd.el` (18) and `lisp/term/x-win.el` (5), which GNU
/// preloads only behind `(featurep 'x)` (`lisp/loadup.el:304-309`), and `x` is
/// provided by `syms_of_xfns` (`src/xfns.c:10498`), compiled only under
/// `HAVE_X_WINDOWS`.  Preloading them here to make those rows match GNU's
/// X build would be an invention of the same kind ledger 178 deleted -- a
/// variable that exists with no window system behind it.
///
/// Measured rather than argued, three ways:
///
/// * GNU 31.0.90 built `--without-x --without-all` answers `nil` for all 28.
/// * GNU 31.0.90 built `--with-x-toolkit=gtk3` answers `nil`, in `--batch`,
///   for every Lisp variable of the five window systems it was NOT built
///   with -- `haiku-dnd-selection-value`, `haiku-normal-selection-encoders`,
///   `w32-standard-fontset-spec`, `w32-initialized`, `w32-non-USB-fonts`,
///   `android-primary-selection`, `android-preedit-overlay`,
///   `ns-working-overlay`, `ns-pop-up-frames`: 9 of 9 unbound and
///   undocumented.  GNU's own dump has this state; it is not a defect.
/// * `src/doc.c:585-594` says so in a comment -- "The (f)boundp checks below
///   ensure we don't report docs for eg w32-specific items on X" -- and
///   enforces it at `:606-613`, where the `Fput` of a
///   `variable-documentation` is gated on `Fboundp`.
///
/// The `x-dnd-` half is a `mapatoms` prefix scan over the whole obarray rather
/// than a list of the 18 names: ledger 173's law is that a predicate over rows
/// that exist cannot see a row that was never written, and `x-dnd.el` can grow
/// a nineteenth.  A prefix scan over 17k symbols has no empty state and counts
/// names nobody thought to list.
///
/// **It found ten on its first run, and they are the C half of exactly this
/// window system.**  `x-dnd-disable-motif-drag`, `x-dnd-disable-motif-protocol`,
/// `x-dnd-fix-motif-leave`, `x-dnd-movement-function`,
/// `x-dnd-native-test-function`, `x-dnd-preserve-selection-data`,
/// `x-dnd-targets-list`, `x-dnd-unsupported-drop-function`,
/// `x-dnd-use-unsupported-drop` and `x-dnd-wheel-function` are `DEFVAR`s in
/// `src/xterm.c` (`:32870`-`:32960`), so GNU binds them only under
/// `HAVE_X_WINDOWS` -- the tty-only build answers `nil` for all ten -- and this
/// port declares them on purpose, from the `syms_of_xterm` sweep at
/// `crates/neovm-core/src/emacs_core/runtime/eval/mod.rs:5680-5699` and
/// `defvar_bool.rs:269`.  They are pinned here BY NAME, not folded into the
/// count, so that the count that must stay zero (the `x-dnd.el` one) stays
/// readable and an eleventh cannot arrive unnoticed.  The tension they record
/// -- this build has GNU's X drag-and-drop callback variables and not the
/// `x-dnd.el` that assigns them -- is ledger 179's, deliberately not resolved
/// there.
///
/// Ledger 179.  GREEN before and after the fix; it is here so that the fix for
/// the other five cannot be extended to these by accident.
#[test]
fn x_only_lisp_variables_are_absent_because_this_build_does_not_provide_x() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        "(let ((xterm-c-dnd-vars
                ;; src/xterm.c:32870-32960, DEFVAR'd under HAVE_X_WINDOWS and
                ;; declared here by eval.rs:5680-5699 / defvar_bool.rs:269.
                '(x-dnd-disable-motif-drag x-dnd-disable-motif-protocol
                  x-dnd-fix-motif-leave x-dnd-movement-function
                  x-dnd-native-test-function x-dnd-preserve-selection-data
                  x-dnd-targets-list x-dnd-unsupported-drop-function
                  x-dnd-use-unsupported-drop x-dnd-wheel-function)))
           (list
             ;; src/xfns.c:10498 `Fprovide (Qx, Qnil)', under HAVE_X_WINDOWS.
             (featurep 'x)
             ;; lisp/x-dnd.el:1743 and lisp/term/x-win.el:1656-1657.
             (featurep 'x-dnd)
             (featurep 'x-win)
             (featurep 'term/x-win)
             ;; Whole-obarray: no `x-dnd-' name from `x-dnd.el' -- that is, none
             ;; beyond the C ones above -- may be bound or documented.
             (let ((n 0))
               (mapatoms
                (lambda (s)
                  (if (and (string-prefix-p \"x-dnd-\" (symbol-name s))
                           (not (memq s xterm-c-dnd-vars)))
                      (if (or (boundp s)
                              (documentation-property
                               s 'variable-documentation t))
                          (setq n (1+ n))))))
               n)
             ;; The C ones, pinned by name: exactly these, no more, no fewer.
             (let (found)
               (mapatoms
                (lambda (s)
                  (if (and (string-prefix-p \"x-dnd-\" (symbol-name s))
                           (boundp s))
                      (push (symbol-name s) found))))
               (sort found #'string<))
             ;; The five `term/x-win.el' names, spelled out because they share no
             ;; prefix with each other.
             (let ((n 0))
               (dolist (s '(icon-map-list x-gtk-stock-map x-initialized
                            x-preedit-overlay
                            x-display-cursor-at-start-of-preedit-string))
                 (if (or (boundp s)
                         (documentation-property s 'variable-documentation t))
                     (setq n (1+ n))))
               n)))",
    );
    assert_eq!(
        result,
        "OK (nil nil nil nil 0 \
         (\"x-dnd-disable-motif-drag\" \"x-dnd-disable-motif-protocol\" \
         \"x-dnd-fix-motif-leave\" \"x-dnd-movement-function\" \
         \"x-dnd-native-test-function\" \"x-dnd-preserve-selection-data\" \
         \"x-dnd-targets-list\" \"x-dnd-unsupported-drop-function\" \
         \"x-dnd-use-unsupported-drop\" \"x-dnd-wheel-function\") 0)"
    );
}

/// The rule ledger 179 fixed for one name, stated for all of them: **this
/// port's `term/FOO-win.el` must not re-`defvar` a name the C layer already
/// declares.**
///
/// GNU spells the rule twice in the same file, and both times it uses the
/// *value-less* `defvar`, which is the compiler-silencing form and installs
/// nothing:
///
/// * `lisp/term/x-win.el:1223` -- `(defvar x-display-name)`, for the name
///   `term/common-win.el:145` really defines.
/// * `lisp/term/x-win.el:1634` -- `(defvar x-input-coding-function)`, for the
///   name `src/xterm.c:32993` really defines with a `DEFVAR_LISP`.
///
/// The second form is not optional decoration.
/// `internal--define-uninitialized-variable` installs the docstring
/// unconditionally when one is given (`src/eval.c:909-912`), so a
/// `defvar` WITH a docstring over a C name silently replaces GNU's C text with
/// the port's own -- and it resets the C initializer to the Lisp form's value
/// as well.  Ledger 179 found and deleted one instance of that
/// (`lisp/term/neo-win.el:70`, `x-display-name`) and pinned the four
/// `term/common-win.el` names by name.
///
/// A per-name pin could not have found the next one, which is 173's law again:
/// a predicate over rows somebody listed cannot see the row nobody listed.  So
/// this check has no name list at all.  It snapshots every
/// `variable-documentation` in the dumped image, loads the GUI terminal layer
/// exactly as `crates/neomacs/src/main.rs:2802-2822` does at GUI startup, and
/// reports every name whose documentation CHANGED.  Adding documentation for a
/// new name is what a window-system file is *for*; changing the documentation
/// of a name that already had some is the defect.  An emptied `term/neo-win.el`
/// makes `(featurep 'term/neo-win)` nil and fails the first element rather than
/// vacuously passing an empty change list.
///
/// Ledger 189.  RED before the fix, with the second instance of ledger 179's
/// defect 296 lines further down the same file:
///
/// ```text
/// left:  "OK (t (\"x-input-coding-function\"))"
/// right: "OK (t nil)"
/// ```
///
/// `x-input-coding-function` is `src/xterm.c:32993` `DEFVAR_LISP` -- declared
/// here at `crates/neovm-core/src/emacs_core/runtime/eval/mod.rs:5755` -- and
/// `lisp/term/neo-win.el` re-declared it with a docstring of its own
/// ("Function used to determine the coding system for input method text.")
/// over GNU's ("Function used to determine the coding system used by input
/// methods.").
#[test]
fn the_gui_terminal_layer_adds_documentation_and_never_rewrites_it() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        "(let ((before (make-hash-table :test 'eq))
               (changed nil))
           ;; Ledger 182: a doc sweep is a WRITE unless this is off first --
           ;; a failed (FILE . POS) read re-`load's the file and re-runs its
           ;; defvars (src/doc.c:311-317, :720).
           (set 'documentation-dynamic-reload nil)
           (mapatoms
            (lambda (s)
              (let ((d (documentation-property s 'variable-documentation t)))
                (if (stringp d) (puthash s d before)))))
           ;; What `load_neomacs_gui_term_layer' does at GUI startup.
           (provide 'neomacs)
           (load \"term/common-win\")
           (load \"term/neo-win\")
           (maphash
            (lambda (s d)
              (let ((now (documentation-property s 'variable-documentation t)))
                (unless (equal now d) (push (symbol-name s) changed))))
            before)
           (list (and (featurep 'term/neo-win) t)
                 (sort changed #'string<)))",
    );
    assert_eq!(result, "OK (t nil)");
}

/// The state of GNU's SECOND loadup question in this build, pinned as a
/// divergence rather than as parity -- and the measurement that priced the
/// seventh branch.
///
/// GNU has six window-system branches (`lisp/loadup.el:304`, `:311`, `:316`,
/// `:323`, `:347`, `:357`), each firing on `(featurep FOO)` for its own
/// window system.  The feature comes from C, from inside the same `#ifdef`
/// that compiles the backend at all: `src/emacs.c:2373-2375` is
/// `#ifdef HAVE_X_WINDOWS { syms_of_xterm (); syms_of_xfns (); ... }` and
/// `Fprovide (Qx, Qnil)` sits inside `syms_of_xfns` (`src/xfns.c:10498`).
/// So in GNU **one** switch decides three things at once -- the C variable
/// surface, the feature, and therefore which `loadup.el` branch runs -- and
/// they cannot disagree.
///
/// In this port they are three independent answers.  The C surface is on
/// (ledger 173's `syms_of_xterm` sweep), the feature is provided at GUI
/// startup instead of at dump time (`crates/neomacs/src/main.rs:2811`), and there
/// is no branch -- one existed and was deleted on 2026-05-25 by commit
/// `4163618ca` "Keep GUI terminal layer out of dump", which had it in GNU's
/// exact shape:
///
/// ```elisp
/// (if (featurep 'neomacs)
///     (progn
///       (load "term/common-win")
///       (load "term/neo-win")))
/// ```
///
/// **Ledger 189 rebuilt that branch, measured it, and declined it.**  The
/// prototype loads cleanly -- `term/neo-win.el`'s `(unless (featurep 'neomacs)
/// (error ...))` guard is GNU's own idiom, shared by all six of its
/// `term/FOO-win.el` files, and it passes once the feature is provided before
/// loadup as `syms_of_pgtkterm` provides `pgtk` (`src/pgtkterm.c:7502`).  What
/// it costs, measured in the resulting dump:
///
/// | | GNU+X | this build | prototype |
/// | --- | --- | --- | --- |
/// | `(featurep 'easy-mmode)` | nil | nil | **t** |
/// | `window-system` in `--batch` | nil | nil | **neo** |
/// | `initial-window-system` in `--batch` | nil | nil | **neo** |
/// | `display-format-alist` | the `x` entry | **nil** | `((".*" . neo))` |
/// | `special-event-map` `[drag-n-drop]` | `x-dnd-handle-drag-n-drop-event` | **nil** | `neomacs-drag-n-drop` |
/// | `special-event-map` `[preedit-text]` | `x-preedit-text` | **nil** | `x-preedit-text` |
/// | symbols in the image | 18619 | 17366 | 17610 |
///
/// The last three rows are the gain; the first three are the price, and the
/// first two are why this is a decline rather than a landing.  No GNU
/// `term/FOO-win.el` contains a `define-minor-mode` -- the only two `term/`
/// files that do are `tvi970.el` and `vt100.el`, which are TTY files loaded at
/// runtime and never dumped -- so no GNU dump has `easy-mmode`.  This port's
/// `term/neo-win.el` has two, at `:514` and `:538`, and they sit inside a
/// 305-line block of renderer knobs (`:420`-`:724` of 728) that GNU's
/// window-system files have no counterpart for.  And `window-system` non-nil
/// in a batch image is the recurring "invented Rust default" of this campaign
/// wearing a new hat: `crates/neovm-core/src/emacs_core/lisp/load/mod.rs:4627-4637` derives it
/// from `(featurep 'neomacs)`, so providing the feature at dump time makes a
/// batch session claim a GUI.
///
/// What the price actually costs a caller was measured rather than assumed.
/// Over eight window-system predicates -- `window-system`,
/// `display-graphic-p`, `display-multi-frame-p`, `display-popup-menus-p`,
/// `framep-on-display`, `(and window-system t)`, the frame's `window-system`
/// parameter and `noninteractive` -- GNU+X, GNU `--without-x` and this build
/// all answer `(nil nil nil nil t nil nil t)` in `--batch`.  The prototype
/// answers `(neo nil nil nil t t nil t)`: **one** row moves, and it is the one
/// every `(if window-system ...)` in Elisp reads.
///
/// And the reason this is pinned here rather than left to a suite: the
/// prototype passes the ORACLE suite.  `38815 tests run: 38812 passed, 3
/// failed`, and the three are the pre-existing upstream window-scroll trio.
/// Thirty-eight thousand agreement rows do not see any of the four costs
/// above, which is the brief's false-green law in its structural form -- so
/// the decline rests on direct measurement against GNU, and this test is what
/// carries it.
///
/// Ledger 189.  This test pins a state that is NOT GNU parity on two rows, and
/// says so: `display-format-alist` and the `[drag-n-drop]` binding are things
/// GNU's dump has and this one does not.  It exists so the decline cannot
/// drift -- flipping `(featurep 'neomacs)` at dump time turns it red, and
/// whoever does that has to read this entry first.  RED under the prototype,
/// which is the negative control for every row above:
///
/// ```text
/// left:  "OK (t t t neo neo ((\".*\" . neo)) neomacs-drag-n-drop t t)"
/// right: "OK (nil nil nil nil nil nil nil t t)"
/// ```
#[test]
fn this_build_answers_gnus_second_loadup_question_with_no_branch_at_all() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        "(list
           ;; What the seventh branch would need, and what it would flip.
           (featurep 'neomacs)
           (featurep 'term/neo-win)
           ;; GNU has no `easy-mmode' in any dump; `term/neo-win.el' would
           ;; bring one.
           (featurep 'easy-mmode)
           ;; GNU's own batch answer even in an X build, measured.
           window-system
           initial-window-system
           ;; ... and the two rows where this build is NOT at parity, kept
           ;; visible rather than left out.  GNU+X answers
           ;; ((\"\\\\`.*:[0-9]+\\\\(\\\\.[0-9]+\\\\)?\\\\'\" . x) and
           ;; x-dnd-handle-drag-n-drop-event here; `lisp/term/x-win.el:1347'
           ;; and `:1375' put them in the dump.
           display-format-alist
           (lookup-key special-event-map [drag-n-drop])
           ;; The anchor: question ONE is answered yes, which is what makes
           ;; question two's absence a real question rather than a tty build.
           (and (fboundp 'x-create-frame) t)
           (featurep 'term/common-win))",
    );
    assert_eq!(result, "OK (nil nil nil nil nil nil nil t t)");
}

/// The other half of ledger 179's handed-over question, sized: this build
/// carries **all 31** of the `x-*` C variables GNU declares only for X, and
/// GNU's own non-X GUI backend declares **none** of them.
///
/// 179 pinned ten `x-dnd-*` names and handed the tension on -- "this build has
/// GNU's X drag-and-drop callback variables and not the `lisp/x-dnd.el` that
/// assigns them ... belongs to whoever owns the C-surface-vs-window-system
/// question".  Ledger 189 owns it, and the first thing to say is that ten was
/// a third of the number.
///
/// GNU answers the question in source, without needing a build.  Take the
/// `DEFVAR_*` names of every window-system backend GNU ships --
/// `xterm.c`/`xfns.c`, `pgtkterm.c`/`pgtkfns.c`, `haikuterm.c`/`haikufns.c`,
/// `androidterm.c`/`androidfns.c`, `w32term.c`/`w32fns.c`,
/// `nsterm.m`/`nsfns.m` -- and split X's 62 `x-`-prefixed ones by whether any
/// *other* backend also declares them.  It comes out 31/31:
///
/// * **31 shared** -- the keysym five (`x-ctrl-keysym` ... `x-super-keysym`),
///   `x-toolkit-scroll-bars`, `x-underline-at-descent-line`,
///   `x-use-underline-position-properties`, `x-cursor-fore-pixel`,
///   `x-max-tooltip-size`, the pointer shapes, the window-edge cursors.
///   These are what "having a window system" means, and `src/pgtkterm.c:7449`
///   -`7494` is GNU declaring exactly that subset for a GUI backend that is
///   not X.  This build binds **29** of the 31 (`x-mode-pointer-shape` and
///   `x-nontext-pointer-shape` are the two it leaves out).
/// * **31 X-only** -- the ten `x-dnd-*`, the Motif and XDND protocol knobs,
///   `x-keysym-table`, `x-input-coding-system`/`-function`, `x-quit-keysym`,
///   `x-detect-server-trust`, `x-fast-protocol-requests`,
///   `x-allow-focus-stealing`, `x-lax-frame-positioning`,
///   `x-auto-preserve-selections`, `x-scroll-event-delta-factor`, the four
///   `x-gtk-*` ones in `xterm.c`, and the rest.  GNU compiles these only
///   inside `#ifdef HAVE_X_WINDOWS` (`src/emacs.c:2373-2375`), which is the
///   same `#ifdef` that runs `Fprovide (Qx, Qnil)` (`src/xfns.c:10498`).
///   **This build binds 31 of 31.**
///
/// GNU says the same thing a second time in preloaded Lisp, which is the
/// sharper citation because it is a table rather than an inference.
/// `lisp/cus-start.el` decides whether a missing C variable is an error by
/// asking a per-name `native-p` question, and it separates the two cases
/// explicitly: `scroll-bar-adjust-thumb-portion`,
/// `x-scroll-event-delta-factor`, `x-dnd-disable-motif-drag` and
/// `x-auto-preserve-selections` are gated on `(featurep 'x)`, while a generic
/// `x-` name is gated on `(fboundp 'x-create-frame)`.  That is GNU's own
/// spelling of question two versus question one, applied to the C surface.
///
/// **Not changed here, and the reason is a pin rather than an opinion.**
/// `crates/neovm-oracle-tests/src/defvar_bool_byte_boolean_vars.rs`'s
/// `oracle_every_defvar_bool_variable_is_bound_and_canonical` asserts that all
/// 147 of GNU's `DEFVAR_BOOL` names -- five `x-dnd-*` among them -- are bound
/// AND canonical here, and it agrees with GNU because the reference binary is
/// built with X.  Deleting the X-only declarations turns that agreement pin
/// red and replaces it with a hand-maintained exception list, which is the
/// shape ledger 176 caught being one GNU spelling away from silently wrong.
/// So the C surface is a *policy* this project has pinned against GNU+X, not
/// a leftover: changing it is a decision about what the oracle reference
/// means, and it belongs in its own entry with its own gates.
///
/// Ledger 189.  GREEN before and after; it exists so the 31 cannot drift in
/// either direction unnoticed, and so the next author finds GNU's own rule for
/// telling the two halves apart instead of re-deriving it.
#[test]
fn the_c_surface_carries_all_31_of_gnus_x_only_variables_and_29_of_the_31_shared_ones() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        "(let ((x-only
                ;; DEFVAR'd in src/xterm.c or src/xfns.c and in NO other GNU
                ;; window-system backend.
                '(x-allow-focus-stealing x-auto-preserve-selections
                  x-color-cache-bucket-size x-detect-server-trust
                  x-dnd-disable-motif-drag x-dnd-disable-motif-protocol
                  x-dnd-fix-motif-leave x-dnd-movement-function
                  x-dnd-native-test-function x-dnd-preserve-selection-data
                  x-dnd-targets-list x-dnd-unsupported-drop-function
                  x-dnd-use-unsupported-drop x-dnd-wheel-function
                  x-fast-protocol-requests x-fast-selection-list
                  x-frame-normalize-before-maximize x-gtk-resize-child-frames
                  x-gtk-use-native-input x-gtk-use-window-move
                  x-input-coding-function x-input-coding-system
                  x-input-grab-touch-events x-keysym-table
                  x-lax-frame-positioning x-mouse-click-focus-ignore-position
                  x-mouse-click-focus-ignore-time x-quit-keysym
                  x-scroll-event-delta-factor
                  x-set-frame-visibility-more-laxly
                  x-use-fast-mouse-position))
               (shared
                ;; DEFVAR'd in src/xterm.c or src/xfns.c AND in at least one
                ;; non-X backend -- src/pgtkterm.c:7449-7494 is the clearest.
                '(x-alt-keysym x-ctrl-keysym x-cursor-fore-pixel
                  x-gtk-file-dialog-help-text x-gtk-show-hidden-files
                  x-gtk-use-old-file-dialog x-hourglass-pointer-shape
                  x-hyper-keysym x-max-tooltip-size x-meta-keysym
                  x-mode-pointer-shape x-nontext-pointer-shape
                  x-no-window-manager x-pixel-size-width-font-regexp
                  x-pointer-shape x-sensitive-text-pointer-shape
                  x-super-keysym x-toolkit-scroll-bars
                  x-underline-at-descent-line
                  x-use-underline-position-properties x-wait-for-event-timeout
                  x-window-bottom-edge-cursor
                  x-window-bottom-left-corner-cursor
                  x-window-bottom-right-corner-cursor
                  x-window-horizontal-drag-cursor x-window-left-edge-cursor
                  x-window-right-edge-cursor x-window-top-edge-cursor
                  x-window-top-left-corner-cursor
                  x-window-top-right-corner-cursor
                  x-window-vertical-drag-cursor)))
           (list (length x-only)
                 (length (delq nil (mapcar (lambda (s) (and (boundp s) s))
                                           x-only)))
                 (length shared)
                 (length (delq nil (mapcar (lambda (s) (and (boundp s) s))
                                           shared)))
                 ;; ... and the whole `x-' surface, so that a name outside
                 ;; both lists cannot arrive unseen.  GNU built --without-x
                 ;; binds 15 (all of them from preloaded Lisp: the
                 ;; `x-font-regexp' family, `x-select-enable-*',
                 ;; `x-popup-menu-function'); GNU built --with-x
                 ;; --with-x-toolkit=no binds 193; this build binds the number
                 ;; below.
                 (let ((n 0))
                   (mapatoms
                    (lambda (s)
                      (if (and (string-prefix-p \"x-\" (symbol-name s))
                               (boundp s))
                          (setq n (1+ n)))))
                   n)
                 ;; The feature that would make all 62 legitimate, and does
                 ;; not exist here.
                 (featurep 'x)))",
    );
    assert_eq!(result, "OK (31 31 31 29 93 nil)");
}
