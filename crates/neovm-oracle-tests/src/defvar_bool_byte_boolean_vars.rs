//! Oracle guards for GNU's `DEFVAR_BOOL' declarations and the list the byte
//! optimizer reads them from.
//!
//! `defvar_bool' does two things (`src/lread.c:5253-5262'): it installs a
//! forwarder whose store is `*XBOOLVAR (valcontents) = !NILP (newval)'
//! (`src/data.c:1485-1487'), and it conses the symbol onto
//! `Vbyte_boolean_vars'.  The second is what makes the first visible to
//! compiled code: `byte-optimize-lapcode' refuses to fold a `varset X;
//! varref X' pair into the stored value when X is on that list, substituting
//! `t' instead (`lisp/emacs-lisp/byte-opt.el:2285-2300').
//!
//! The list is not the whole set.  `syms_of_lread' declares it and immediately
//! writes `Vbyte_boolean_vars = Qnil' (`src/lread.c:5772-5774'), discarding
//! every cons made by the thirteen `syms_of_*' functions `main' ran before it
//! (`src/emacs.c:1976-2306').  So a `DEFVAR_BOOL' variable from `fns.c'
//! coerces at run time and is folded anyway when compiled -- a difference
//! these tests pin on purpose, because "more correct than GNU" is still a
//! divergence.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Membership and shape of the list itself.
#[test]
fn oracle_byte_boolean_vars_matches_gnus_declaration_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list (length byte-boolean-vars)
      (car byte-boolean-vars)
      (nth 116 byte-boolean-vars)
      ;; declared after `syms_of_lread' cleared the list
      (and (memq 'visible-bell byte-boolean-vars) t)
      (and (memq 'inhibit-message byte-boolean-vars) t)
      (and (memq 'indent-tabs-mode byte-boolean-vars) t)
      (and (memq 'print-quoted byte-boolean-vars) t)
      ;; declared before it, so the cons was thrown away again
      (and (memq 'use-short-answers byte-boolean-vars) t)
      (and (memq 'garbage-collection-messages byte-boolean-vars) t)
      (and (memq 'symbols-with-pos-enabled byte-boolean-vars) t)
      (and (memq 'load-in-progress byte-boolean-vars) t))"#;
    let expect = expect_test::expect![
        r#""OK (117 font-use-system-font load-dangerous-libraries t t t t nil nil nil nil)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The byte-compiled case, which is the one that regresses silently: with the
/// list empty the optimizer folds `varset'/`varref' for every Boolean variable
/// and the compiled function returns the raw value, while the variable itself
/// holds `t'.
#[test]
fn oracle_byte_compiled_varset_varref_folds_only_off_the_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 ;; On the list: the optimizer substitutes t rather than the stored value.
 (funcall (byte-compile (lambda () (setq visible-bell 4) visible-bell)))
 (funcall (byte-compile (lambda () (setq inhibit-message 4) inhibit-message)))
 ;; Off the list: GNU folds the pair and hands back the raw 4 ...
 (funcall (byte-compile (lambda () (setq use-short-answers 4) use-short-answers)))
 (funcall (byte-compile (lambda () (setq garbage-collection-messages 7)
                          garbage-collection-messages)))
 ;; ... even though the slot coerced, which a later read shows.
 use-short-answers
 garbage-collection-messages)"#;
    let expect = expect_test::expect![r#""OK (t t 4 7 t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The coercion itself, through every assignment spelling: `setq' returns what
/// it was handed and the next read is canonical.  `let' and `set-default' go
/// through `store_symval_forwarding' too (`src/eval.c:3594-3622',
/// `src/data.c:2077'), and a buffer-local binding keeps the forwarder because
/// `make_blv' copies it (`src/data.c:2112-2140').
#[test]
fn oracle_defvar_bool_coerces_through_every_assignment_spelling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list (list (setq visible-bell 5) visible-bell)
      (list (setq create-lockfiles nil) create-lockfiles)
      (list (setq print-quoted (list 1)) print-quoted)
      (let ((inverse-video 3)) inverse-video)
      (progn (set-default 'inverse-video 9) (default-value 'inverse-video))
      (with-temp-buffer (setq-local indent-tabs-mode 4) indent-tabs-mode)
      (progn (setq print-escape-newlines nil)
             (let ((b (generate-new-buffer "fwd135")))
               (with-current-buffer b (setq-local print-escape-newlines 3))
               (kill-buffer b)
               (setq print-escape-newlines 7)
               print-escape-newlines)))"#;
    let expect = expect_test::expect![r#""OK ((5 t) (nil nil) ((1) t) t t t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// Six of the `DEFVAR_BOOL' variables are additionally made buffer-local --
/// five by `Fmake_variable_buffer_local' calls in the `syms_of_*' that
/// declares them (`src/xdisp.c:38735,38997,39015', `src/syntax.c',
/// `src/casefiddle.c'), and `indent-tabs-mode' by `lisp/bindings.el:1048'.
/// The declaration has to reach the BLV, not just the symbol: `make_blv'
/// copies the forwarder across (`src/data.c:2112-2140'), which is why
/// assigning 4 to a buffer-local `indent-tabs-mode' still reads back `t'.
#[test]
fn oracle_defvar_bool_variables_that_gnu_also_makes_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (mapcar (lambda (s) (set s (default-value s)) (and (local-variable-p s) s))
          '(case-symbols-as-words comment-end-can-be-escaped
            display-fill-column-indicator display-line-numbers-widen
            indent-tabs-mode make-window-start-visible
            ;; control: a DEFVAR_BOOL nothing localizes
            visible-bell)))"#;
    let expect = expect_test::expect![
        r#""OK (case-symbols-as-words comment-end-can-be-escaped display-fill-column-indicator display-line-numbers-widen indent-tabs-mode make-window-start-visible nil)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// Every `DEFVAR_BOOL' variable GNU binds on GNU/Linux is bound here, is
/// special, and reads back `t' -- never the 5 that was stored.
///
/// `debug-on-next-call' is the one name left out: setting it non-nil is what
/// arms the debugger, and entering the debugger clears it again, so it cannot
/// be probed by assignment.
///
/// Re-measured 2026-08-20 (ledger 168 item 4) and unchanged.
/// `(list (default-value 'debug-on-next-call)
///        (progn (set-default 'debug-on-next-call 5)
///               (default-value 'debug-on-next-call))
///        (progn (setq debug-on-next-call t) debug-on-next-call))`
/// is `(nil nil t)` under GNU and `(nil t t)` here -- and GNU prints four
/// debugger backtraces while answering, which is the mechanism showing itself:
/// the `set-default` arms the flag, the very next `funcall` reaches
/// `if (debug_on_next_call) do_debug_on_call (Qlambda, count)`
/// (`src/eval.c:3189`, and `2601` in `eval_sub`, and `src/bytecode.c:798`),
/// and `do_debug_on_call` clears it on its first line before calling the
/// debugger (`src/eval.c:336-340`); `call_debugger` clears it again
/// (`src/eval.c:298`).  The third element is `t` in both because `progn` and
/// `setq` are special forms, so no funcall intervenes before the read.
///
/// **Implemented 2026-08-21 (ledger 172).**  The three dispatch checks and
/// `do_debug_on_call` now exist (`emacs_core::debug_on_call`), so this port
/// answers `(nil nil t)` too and its own value is just as unstable under an
/// assignment probe as GNU's.  The exclusion below therefore STAYS: the point
/// was never that Neomacs lacked the mechanism, it is that a sweep which
/// assigns to every `DEFVAR_BOOL` and reads it back cannot include a variable
/// whose assignment arms a debugger that clears it before the read.  The
/// handshake itself is pinned in `debug_on_next_call.rs`; what belongs here is
/// only the reason for the hole in the list.  The variable's declaration and
/// its Boolean coercion were correct all along and are untouched.
#[test]
fn oracle_every_defvar_bool_variable_is_bound_and_canonical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
  (let ((names '(after-delete-frame-select-mru-frame
                 attempt-orderly-shutdown-on-fatal-signal attempt-stack-overflow-recovery
                 auto-raise-tab-bar-buttons auto-raise-tool-bar-buttons
                 auto-save-no-message auto-window-vscroll
                 backtrace-on-error-noninteractive backtrace-on-redisplay-error
                 bidi-inhibit-bpa cannot-suspend case-symbols-as-words
                 coding-system-require-warning comment-end-can-be-escaped
                 completion-ignore-case composition-break-at-point create-lockfiles
                 cross-disabled-images current-time-list cursor-in-echo-area
                 debug-on-quit debugger-may-continue debugger-stack-frame-as-list
                 delete-auto-save-files delete-by-moving-to-trash delete-exited-processes
                 disable-ascii-optimization disable-inhibit-text-conversion
                 display-fill-column-indicator display-hourglass
                 display-line-numbers-widen display-raw-bytes-as-hex
                 documentation-dynamic-reload echo-keystrokes-help
                 enable-recursive-minibuffers face-filters-always-match
                 fast-but-imprecise-scrolling fast-read-process-output
                 font-use-system-font force-load-messages frame-resize-pixelwise
                 garbage-collection-messages highlight-nonselected-windows
                 history-delete-duplicates indent-tabs-mode inherit-process-coding-system
                 inhibit--record-char inhibit-bidi-mirroring
                 inhibit-compacting-font-caches inhibit-eol-conversion
                 inhibit-eval-during-redisplay inhibit-free-realized-faces
                 inhibit-interaction inhibit-iso-escape-detection
                 inhibit-load-charset-map inhibit-menubar-update inhibit-message
                 inhibit-modification-hooks inhibit-mouse-event-check
                 inhibit-null-byte-detection inhibit-x-resources
                 input-pending-p-filter-events internal--text-quoting-flag inverse-video
                 kill-buffer-delete-auto-save-files kill-emacs-on-sigint
                 load-dangerous-libraries load-force-doc-strings load-in-progress
                 load-no-native load-prefer-newer make-window-start-visible
                 menu-prompting message-truncate-lines minibuffer-allow-text-properties
                 minibuffer-auto-raise mode-line-in-non-selected-windows
                 mouse-fine-grained-tracking mouse-prefer-closest-glyph
                 multibyte-syntax-as-symbol multiple-frames
                 multiple-terminals-merge-keyboards mwheel-coalesce-scroll-events
                 no-redraw-on-reenter nobreak-char-ascii-display noninteractive
                 open-paren-in-column-0-is-defun-start parse-sexp-ignore-comments
                 parse-sexp-lookup-properties print-escape-control-characters
                 print-escape-multibyte print-escape-newlines print-escape-nonascii
                 print-integers-as-characters print-quoted print-symbols-bare
                 process-prioritize-lower-fds query-all-font-backends
                 read-buffer-completion-ignore-case read-minibuffer-restore-windows
                 record-all-keys redisplay--inhibit-bidi
                 redisplay-adhoc-scroll-in-resize-mini-windows
                 redisplay-skip-fontification-on-input redisplay-skip-initial-frame
                 scroll-bar-adjust-thumb-portion scroll-minibuffer-conservatively
                 symbols-with-pos-enabled system-uses-terminfo
                 tab-bar--dragging-in-progress tab-bar-truncate
                 tooltip-reuse-hidden-frame translate-upper-case-key-bindings
                 tty-cursor-movement-use-TAB-BS tty-menu-calls-mouse-position-function
                 undo-inhibit-record-point unibyte-display-via-language-environment
                 use-default-font-for-symbols use-dialog-box use-file-dialog
                 use-short-answers use-system-tooltips visible-bell visible-cursor
                 window-auto-redraw-on-parameter-change window-resize-pixelwise
                 word-wrap-by-category words-include-escapes write-region-inhibit-fsync
                 x-dnd-disable-motif-drag x-dnd-disable-motif-protocol
                 x-dnd-fix-motif-leave x-dnd-preserve-selection-data
                 x-dnd-use-unsupported-drop x-fast-protocol-requests
                 x-frame-normalize-before-maximize x-gtk-file-dialog-help-text
                 x-gtk-show-hidden-files x-gtk-use-native-input x-gtk-use-old-file-dialog
                 x-gtk-use-window-move x-input-grab-touch-events
                 x-mouse-click-focus-ignore-position x-stretch-cursor
                 x-underline-at-descent-line x-use-underline-position-properties
                 xft-ignore-color-fonts
                 ))
        (unbound '()) (nonspecial '()) (noncanonical '()))
    ;; A fundamental-mode buffer: `lisp-mode-variables' gives *scratch* its own
    ;; buffer-local `parse-sexp-*' bindings, and `set_default_internal' skips
    ;; `store_symval_forwarding' while a local binding is loaded
    ;; (`src/data.c:2077-2113').
    (with-temp-buffer
      (dolist (s names)
        (cond ((not (boundp s)) (push s unbound))
              ((not (special-variable-p s)) (push s nonspecial))
              (t (let ((old (default-value s)))
                   (set-default s 5)
                   (unless (eq (default-value s) t) (push s noncanonical))
                   (set-default s old))))))
    (list (length names) (nreverse unbound) (nreverse nonspecial)
          (nreverse noncanonical)))"#;
    let expect = expect_test::expect![r#""OK (147 nil nil nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
