//! Pins for the GNU `DEFVAR_*` specialness sweep (DIVERGENCES.md entry 64).
//!
//! GNU declares each of these variables in C with `DEFVAR_LISP` /
//! `DEFVAR_BOOL` / `DEFVAR_INT` / `DEFVAR_KBOARD`, which both binds the
//! symbol and marks it special (dynamically scoped). A variable that is
//! bound but NOT special fails silently: a `let` of it inside a
//! lexical-binding file creates a lexical binding that callees reading the
//! dynamic value never see.
//!
//! Each table entry is (NAME, PREDICATE-ON-`v`) where `v` is bound to
//! `(default-value 'NAME)`; the predicate pins GNU's C-level initial value
//! (the value before loadup/startup Lisp runs, which is what a bare
//! `Context` models).

use crate::emacs_core::eval::Context;
use crate::emacs_core::format_eval_result;

fn assert_gnu_special(eval: &mut Context, name: &str, default_pred: &str) {
    let form = format!(
        "(list (special-variable-p '{name}) (let ((v (default-value '{name}))) {default_pred}))"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str(&form)),
        "OK (t t)",
        "variable `{name}' must be special with its GNU DEFVAR initial value \
         (predicate on v=(default-value '{name}): {default_pred})"
    );
}

fn assert_batch(batch: &[(&str, &str)]) {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    for (name, pred) in batch {
        assert_gnu_special(&mut eval, name, pred);
    }
}

/// keyboard.c / macros.c cluster.
#[test]
fn keyboard_defvars_are_special_with_gnu_defaults() {
    assert_batch(&[
        // keyboard.c:13841 DEFVAR_INT, init 300.
        ("auto-save-interval", "(eq v 300)"),
        // keyboard.c:13850 DEFVAR_LISP, XSETFASTINT 30.
        ("auto-save-timeout", "(eq v 30)"),
        // keyboard.c:14459 DEFVAR_LISP, init nil.
        ("current-key-remap-sequence", "(null v)"),
        // keyboard.c:14358 DEFVAR_LISP, init Qsigusr2.
        ("debug-on-event", "(eq v 'sigusr2)"),
        // macros.c:427 DEFVAR_KBOARD, kboard slots start nil.
        ("defining-kbd-macro", "(null v)"),
        // keyboard.c:14422 DEFVAR_LISP, init nil.
        ("display-monitors-changed-functions", "(null v)"),
        // keyboard.c:13886 DEFVAR_INT, init 3.
        ("double-click-fuzz", "(eq v 3)"),
        // keyboard.c:13876 DEFVAR_LISP, make_fixnum 500.
        ("double-click-time", "(eq v 500)"),
        // keyboard.c:14319 DEFVAR_LISP, init nil.
        ("enable-disabled-menus-and-buttons", "(null v)"),
        // keyboard.c:13993 DEFVAR_INT, init 0.
        ("extra-keyboard-modifiers", "(eq v 0)"),
        // keyboard.c:14202 DEFVAR_LISP, Fmake_sparse_keymap.
        ("function-key-map", "(keymapp v)"),
        // keyboard.c:14210 DEFVAR_LISP, Fmake_sparse_keymap.
        ("key-translation-map", "(keymapp v)"),
        // keyboard.c:13913 DEFVAR_LISP, init nil.
        ("last-event-device", "(null v)"),
        // keyboard.c:13908 DEFVAR_LISP, init nil.
        ("last-event-frame", "(null v)"),
        // macros.c:442 DEFVAR_KBOARD, kboard slots start nil.
        ("last-kbd-macro", "(null v)"),
        // keyboard.c:13744 DEFVAR_LISP, zero-init nil.
        ("last-nonmenu-event", "(null v)"),
        // keyboard.c:13803 DEFVAR_KBOARD, kboard slots start nil.
        ("last-repeatable-command", "(null v)"),
        // keyboard.c:13777 DEFVAR_LISP, XSETINT 033.
        ("meta-prefix-char", "(eq v 27)"),
        // keyboard.c:14287 DEFVAR_LISP, make_fixnum 2.
        ("minibuffer-message-timeout", "(eq v 2)"),
        // keyboard.c:13897 DEFVAR_INT, init 0.
        ("num-input-keys", "(eq v 0)"),
        // keyboard.c:13903 DEFVAR_INT, init 0.
        ("num-nonmacro-input-events", "(eq v 0)"),
        // keyboard.c:13869 DEFVAR_LISP, make_float 2.0.
        ("polling-period", "(equal v 2.0)"),
        // keyboard.c:14446 DEFVAR_LISP, init nil.
        ("post-select-region-hook", "(null v)"),
        // keyboard.c:14340 DEFVAR_LISP, init nil.
        ("saved-region-selection", "(null v)"),
        // keyboard.c:14130 DEFVAR_LISP, list1 (Qkeymap).
        ("special-event-map", "(keymapp v)"),
        // keyboard.c:13834 DEFVAR_LISP, init nil.
        ("this-original-command", "(null v)"),
        // keyboard.c:14228 DEFVAR_LISP, init nil.
        ("timer-idle-list", "(null v)"),
        // keyboard.c:14224 DEFVAR_LISP, init nil.
        ("timer-list", "(null v)"),
    ]);
}

/// xdisp.c display cluster.
#[test]
fn xdisp_defvars_are_special_with_gnu_defaults() {
    assert_batch(&[
        ("auto-raise-tab-bar-buttons", "(eq v t)"),
        ("auto-raise-tool-bar-buttons", "(eq v t)"),
        ("auto-resize-tab-bars", "(eq v t)"),
        ("auto-resize-tool-bars", "(eq v t)"),
        // make_fixnum (DEFAULT_HOURGLASS_DELAY) = 1 (xdisp.c:1115).
        ("hourglass-delay", "(eq v 1)"),
        ("hscroll-margin", "(eq v 5)"),
        ("hscroll-step", "(eq v 0)"),
        ("inhibit-try-cursor-movement", "(null v)"),
        ("line-number-display-limit-width", "(eq v 200)"),
        // make_float (0.25), not a fixnum (xdisp.c:38541).
        ("maximum-scroll-margin", "(equal v 0.25)"),
        ("max-mini-window-height", "(equal v 0.25)"),
        ("messages-buffer-name", "(equal v \"*Messages*\")"),
        ("message-truncate-lines", "(null v)"),
        ("nobreak-char-display", "(eq v t)"),
        ("overline-margin", "(eq v 2)"),
        ("pre-redisplay-function", "(eq v 'ignore)"),
        // Initialized to t until loadup loads charprop.el (xdisp.c:39235).
        ("redisplay--inhibit-bidi", "(eq v t)"),
        // GNU inits nil pre-loadup; loadup.el:142 assigns `grow-only'.
        ("resize-mini-windows", "(null v)"),
        ("scroll-conservatively", "(eq v 0)"),
        ("scroll-margin", "(eq v 0)"),
        ("scroll-step", "(eq v 0)"),
        ("tab-bar-border", "(eq v 'internal-border-width)"),
        ("tab-bar-button-margin", "(eq v 1)"),
        ("tab-bar-button-relief", "(eq v 1)"),
        ("tab-bar-truncate", "(null v)"),
        ("tool-bar-border", "(eq v 'internal-border-width)"),
        ("tool-bar-button-margin", "(eq v 4)"),
        ("tool-bar-button-relief", "(eq v 1)"),
        // DEFAULT_TOOL_BAR_LABEL_SIZE = 14 (dispextern.h:3494).
        ("tool-bar-max-label-size", "(eq v 14)"),
        ("truncate-partial-width-windows", "(eq v 50)"),
        ("underline-minimum-offset", "(eq v 1)"),
        ("void-text-area-pointer", "(eq v 'arrow)"),
    ]);
}

/// frame.c / window.c / dispnew.c / terminal.c / term.c / minibuf.c /
/// textconv.c / emacs.c cluster.
#[test]
fn frame_window_terminal_defvars_are_special_with_gnu_defaults() {
    assert_batch(&[
        // frame.c:7796 DEFVAR_BOOL, init true.
        ("after-delete-frame-select-mru-frame", "(eq v t)"),
        // window.c:9243 DEFVAR_BOOL, init true.
        ("auto-window-vscroll", "(eq v t)"),
        // dispnew.c:7488 DEFVAR_INT, and the only one in the tree with no
        // initializer: the C global starts at 0 and only `init_baud_rate' from
        // `init_tty' (term.c:4755) or a window system's `baud_rate = 19200'
        // (xterm.c:32279) ever writes it.  A `Context' has no terminal, which
        // is the `--batch' case GNU reports 0 for.
        ("baud-rate", "(eq v 0)"),
        // frame.c:7451 DEFVAR_LISP; toolkit GUI builds init Qright.
        ("default-frame-scroll-bars", "(eq v 'right)"),
        // terminal.c:700 DEFVAR_LISP, init nil.
        ("delete-terminal-functions", "(null v)"),
        // frame.c:7636 DEFVAR_LISP; own-drawn tool bar branch.
        (
            "frame-inhibit-implied-resize",
            "(equal v '(tab-bar-lines tool-bar-lines))",
        ),
        // xdisp.c:38621/38629 DEFVAR_LISP; structure pinned, branding aside.
        ("frame-title-format", "(eq (car v) 'multiple-frames)"),
        ("icon-title-format", "(eq (car v) 'multiple-frames)"),
        // frame.c:7733 DEFVAR_LISP, init Qiconify_top_level.
        ("iconify-child-frame", "(eq v 'iconify-top-level)"),
        // emacs.c:3597 DEFVAR_BOOL, init 0.
        ("inhibit-x-resources", "(null v)"),
        // dispnew.c:7508 DEFVAR_LISP, zero-init nil pre-startup.
        ("initial-window-system", "(null v)"),
        // minibuf.c:2538 DEFVAR_LISP, init Qt.
        ("minibuffer-follows-selected-frame", "(eq v t)"),
        // window.c:9247 DEFVAR_INT, init 2.
        ("next-screen-context-lines", "(eq v 2)"),
        // textconv.c:2621 DEFVAR_LISP, init Qlambda.
        ("overriding-text-conversion-style", "(eq v 'lambda)"),
        // minibuf.c:2533 / 2528 DEFVAR_LISP, init nil.
        ("read-buffer-function", "(null v)"),
        ("read-expression-history", "(null v)"),
        // term.c:5240 / 5233 DEFVAR_LISP, init nil.
        ("resume-tty-functions", "(null v)"),
        ("suspend-tty-functions", "(null v)"),
        // term.c:5224 DEFVAR_BOOL, init 1 on TERMINFO systems.
        ("system-uses-terminfo", "(eq v t)"),
        // frame.c:7475 DEFVAR_LISP, zero-init nil pre-terminal-init.
        ("terminal-frame", "(null v)"),
        // window.c:9270 DEFVAR_LISP, init nil.
        ("window-point-insertion-type", "(null v)"),
    ]);
}

/// xfaces.c / image.c cluster.
#[test]
fn face_image_defvars_are_special_with_gnu_defaults() {
    assert_batch(&[
        // xfaces.c:7624 DEFVAR_LISP, build_string ("gray3").
        ("face-default-stipple", "(equal v \"gray3\")"),
        // xfaces.c:7612 DEFVAR_BOOL, zero-init false.
        ("face-filters-always-match", "(null v)"),
        // xfaces.c:7751 DEFVAR_LISP, init Qt.
        ("face-font-lax-matched-attributes", "(eq v t)"),
        // xfaces.c:7728 DEFVAR_LISP, init nil.
        ("face-font-rescale-alist", "(null v)"),
        // xfaces.c:7646 DEFVAR_LISP, nil on non-Xft builds.
        ("face-ignored-fonts", "(null v)"),
        // xfaces.c:7737 DEFVAR_INT, init 30000.
        ("face-near-same-color-threshold", "(eq v 30000)"),
        // xfaces.c:7662 DEFVAR_LISP, init nil.
        ("face-remapping-alist", "(null v)"),
        // xfaces.c:7631 DEFVAR_LISP, init nil.
        ("tty-defined-color-alist", "(null v)"),
        // image.c:13269 DEFVAR_LISP, make_fixnum (300).
        ("image-cache-eviction-delay", "(eq v 300)"),
        // image.c:13028 DEFVAR_LISP; filled with the supported decoders.
        ("image-types", "(listp v)"),
        // image.c:13034 DEFVAR_LISP, make_float (MAX_IMAGE_SIZE) = 10.0.
        ("max-image-size", "(equal v 10.0)"),
    ]);
}

/// fileio.c / coding.c / charset.c / character.c / chartab.c / ccl.c /
/// casefiddle.c / composite.c / timefns.c / lread.c / fns.c / thread.c
/// cluster.
#[test]
fn file_coding_charset_defvars_are_special_with_gnu_defaults() {
    assert_batch(&[
        // fileio.c:6869 DEFVAR_LISP, init nil.
        ("after-insert-file-functions", "(null v)"),
        // composite.c:2231 DEFVAR_LISP, init Qt.
        ("auto-composition-mode", "(eq v t)"),
        // character.c:1104 DEFVAR_LISP, char-table with SPC and NL = t.
        (
            "auto-fill-chars",
            "(and (char-table-p v) (aref v ?\\s) (aref v ?\\n))",
        ),
        // fileio.c:6944 DEFVAR_LISP, init nil.
        ("auto-save-include-big-deletions", "(null v)"),
        // casefiddle.c:738 DEFVAR_BOOL, init 0, buffer-local on set.
        ("case-symbols-as-words", "(null v)"),
        // chartab.c:1375 DEFVAR_LISP, init nil.
        ("char-code-property-alist", "(null v)"),
        // charset.c:2426 / 2438 / 2430 inits.
        ("charset-map-path", "(null v)"),
        ("current-iso639-language", "(null v)"),
        ("inhibit-load-charset-map", "(null v)"),
        // timefns.c:2112 DEFVAR_BOOL, CURRENT_TIME_LIST = true.
        ("current-time-list", "(eq v t)"),
        // coding.c:12139 DEFVAR_LISP, init nil.
        ("default-process-coding-system", "(null v)"),
        // lread.c:5670 DEFVAR_BOOL, zero-init false.
        ("load-in-progress", "(null v)"),
        // fileio.c:6856 DEFVAR_LISP, init nil.
        ("set-auto-coding-function", "(null v)"),
        // ccl.c:2382 DEFVAR_LISP, init nil.
        ("translation-hash-table-vector", "(null v)"),
        // character.c:1156 DEFVAR_LISP, init nil (characters.el fills it).
        ("unicode-category-table", "(null v)"),
        // fileio.c:6879 / 6916 / 6905 DEFVAR_LISP, init nil.
        ("write-region-annotate-functions", "(null v)"),
        ("write-region-annotations-so-far", "(null v)"),
        ("write-region-post-annotation-function", "(null v)"),
        // fns.c:6867 DEFVAR_LISP, "(yes or no) ".
        ("yes-or-no-prompt", "(equal v \"(yes or no) \")"),
        // thread.c:1307 DEFVAR_LISP, the main thread object.
        ("main-thread", "t"),
    ]);
}

/// A `let' of a special variable must be dynamic: a callee defined OUTSIDE
/// the `let' (so no lexical capture is possible) observes the binding.
#[test]
fn keyboard_defvar_let_is_visible_to_callees() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    assert_eq!(
        format_eval_result(&eval.eval_str(
            "(progn
               (fset 'neovm-test--read-meta-prefix (lambda () meta-prefix-char))
               (list (let ((meta-prefix-char 42))
                       (neovm-test--read-meta-prefix))
                     (neovm-test--read-meta-prefix)))"
        )),
        "OK (42 27)"
    );
}
