//! Observable TTY face realization boundaries from GNU Emacs 31.1.
//!
//! Refresh expectations only from GNU with NEOVM_ORACLE_MODE=refresh
//! UPDATE_EXPECT=1, running the test driver in the required PID sandbox.
//! An initial --batch frame is deliberately distinguished from an actual TTY:
//! xfaces.c:6201-6209 realizes dummy faces for the former.  frame.c:1822-1823
//! skips eager basic-face initialization when creating a TTY in batch mode.

#![cfg(unix)]

use crate::common;

#[path = "../src/tty_test_terminal.rs"]
mod tty_test_terminal;

use common::return_if_neovm_enable_oracle_proptest_not_set;
use tty_test_terminal::TestTerminal;

const TRACE_SUPPORT: &str = r##"
  (require 'term/tty-colors)
  (defvar fc-calls nil)
  (defvar fc-events nil)
  (defun fc-desc (old color &optional frame)
    (push (if (eq frame (selected-frame)) color (list 'wrong-frame color)) fc-calls)
    (if (string-prefix-p "#" color)
        (list color 1 65535 0 0)
      (funcall old color frame)))
  (defun fc-mark (stage)
    (push (list stage (length fc-calls) (nreverse fc-calls)) fc-events)
    (setq fc-calls nil))
  (setq redisplay-skip-initial-frame nil inhibit-redisplay nil)
"##;

const EXACT_BASIC_FACE_SETUP: &str = r##"
  (setq inhibit-message t)
  (dolist (face
    '(default mode-line-active mode-line-inactive tool-bar fringe
      header-line-active header-line-inactive scroll-bar border cursor mouse menu
      vertical-border window-divider window-divider-first-pixel window-divider-last-pixel
      internal-border child-frame-border tab-bar tab-line-active tab-line-inactive margin))
    (set-face-attribute face nil :foreground "red" :background "black" :underline nil))
"##;

const EXACT_TTY_BUFFER_SETUP: &str = r##"
  (setq fc-frame
    (make-terminal-frame
      (list (cons 'tty (getenv "FACECACHE_TEST_TTY")) '(tty-type . "xterm-256color"))))
  (fc-mark 'frame-creation)
  (select-frame fc-frame)
  (setq tty-defined-color-alist
    '(("red" 1 65535 0 0) ("black" 0 0 0 0)))
  (switch-to-buffer (get-buffer-create "fc-callback-boundaries"))
  (setq mode-line-format nil header-line-format nil)
  (erase-buffer)
  (insert "a")
  (redisplay t)
  (fc-mark 'basic-exact-palette)
"##;

/// GNU initial batch frames have no terminal color realization, even when
/// redisplay is explicitly enabled and a non-basic face reaches a glyph.
#[test]
fn tty_face_lifecycle_initial_batch_frame_uses_dummy_faces() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = format!(
        r##"(progn {TRACE_SUPPORT}
  (advice-add 'tty-color-desc :around #'fc-desc)
  (make-face 'fc-unused)
  (set-face-attribute 'fc-unused nil :foreground "#123456" :background "#654321")
  (fc-mark 'face-definition)
  (redisplay t)
  (fc-mark 'first-redisplay)
  (redisplay t)
  (fc-mark 'warm-redisplay)
  (set-face-attribute 'fc-unused nil :foreground "#234567")
  (fc-mark 'face-change)
  (redisplay t)
  (fc-mark 'after-face-change)
  (switch-to-buffer (get-buffer-create "fc-initial"))
  (setq mode-line-format nil)
  (erase-buffer)
  (insert (propertize "a" 'face 'fc-unused))
  (redisplay t)
  (fc-mark 'nonbasic-glyph)
  (nreverse fc-events))"##
    );
    common::assert_oracle_parity_with_env_expect(
        &form,
        &[("NEOVM_FACE_REALIZATION_LIFECYCLE", "1")],
        expect_test::expect![[
            r#""OK ((face-definition 0 nil) (first-redisplay 0 nil) (warm-redisplay 0 nil) (face-change 0 nil) (after-face-change 0 nil) (nonbasic-glyph 0 nil))""#
        ]],
    );
}

/// Basic faces use the exact order in xfaces.c:5952-5983.  A non-basic
/// definition does not realize that face until a displayed glyph uses it.
/// Ordinary palette assignment does not invalidate GNU's realized faces;
/// clear-face-cache does (xfaces.c:794-802; tty-colors.el:789-819).
#[test]
fn tty_face_lifecycle_creation_redisplay_demand_and_invalidation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let terminal = TestTerminal::new();
    let form = format!(
        r##"(progn {TRACE_SUPPORT}
  (setq fc-basic-faces
    '(default mode-line-active mode-line-inactive tool-bar fringe
      header-line-active header-line-inactive scroll-bar border cursor mouse menu
      vertical-border window-divider window-divider-first-pixel window-divider-last-pixel
      internal-border child-frame-border tab-bar tab-line-active tab-line-inactive margin))
  (let ((i 0))
    (dolist (face fc-basic-faces)
      (set-face-attribute face nil :foreground (format "#%06x" (+ 1 i))
        :background (format "#%06x" (+ 257 i)) :underline nil)
      (setq i (1+ i))))
  (make-face 'fc-parent)
  (set-face-attribute 'fc-parent nil :foreground "#123456" :background "#654321")
  (make-face 'fc-child)
  (set-face-attribute 'fc-child nil :inherit 'fc-parent :underline "#345678")
  (advice-add 'tty-color-desc :around #'fc-desc)
  (setq fc-frame
    (make-terminal-frame
      (list (cons 'tty (getenv "FACECACHE_TEST_TTY")) '(tty-type . "xterm-256color"))))
  (fc-mark 'frame-creation)
  (select-frame fc-frame)
  (switch-to-buffer (get-buffer-create "fc-actual-tty"))
  (setq mode-line-format nil)
  (erase-buffer)
  (insert "a")
  (redisplay t)
  (fc-mark 'first-redisplay)
  (redisplay t)
  (fc-mark 'warm-redisplay)
  (set-face-attribute 'fc-parent nil :foreground "#234567")
  (fc-mark 'unused-face-change)
  (redisplay t)
  (fc-mark 'redisplay-after-face-change)
  (put-text-property 1 2 'face 'fc-child)
  (redisplay t)
  (fc-mark 'inherited-nonbasic-glyph)
  (redisplay t)
  (fc-mark 'warm-nonbasic-glyph)
  (setq tty-defined-color-alist
    (cons '("#234567" 2 0 65535 0) tty-defined-color-alist))
  (redisplay t)
  (fc-mark 'palette-assignment)
  (clear-face-cache)
  (redisplay t)
  (fc-mark 'explicit-cache-clear)
  (modify-frame-parameters nil '((foreground-color . "#abcdef")))
  (fc-mark 'foreground-parameter-change)
  (redisplay t)
  (fc-mark 'redisplay-after-parameter-change)
  (advice-remove 'tty-color-desc #'fc-desc)
  (fset 'tty-color-desc
    (lambda (color &optional frame)
      (push (if (eq frame (selected-frame)) (concat "redefined:" color)
              (list 'wrong-frame color)) fc-calls)
      (list color 3 0 0 65535)))
  (redisplay t)
  (fc-mark 'function-redefinition)
  (clear-face-cache)
  (redisplay t)
  (fc-mark 'redefinition-after-cache-clear)
  (nreverse fc-events))"##
    );
    common::assert_oracle_parity_with_env_expect(
        &form,
        &[
            ("FACECACHE_TEST_TTY", &terminal.name),
            ("NEOVM_FACE_REALIZATION_LIFECYCLE", "1"),
        ],
        expect_test::expect![[
            r##""OK ((frame-creation 0 nil) (first-redisplay 44 (\"#000001\" \"#000101\" \"#000002\" \"#000102\" \"#000003\" \"#000103\" \"#000004\" \"#000104\" \"#000005\" \"#000105\" \"#000006\" \"#000106\" \"#000007\" \"#000107\" \"#000008\" \"#000108\" \"#000009\" \"#000109\" \"#00000a\" \"#00010a\" \"#00000b\" \"#00010b\" \"#00000c\" \"#00010c\" \"#00000d\" \"#00010d\" \"#00000e\" \"#00010e\" \"#00000f\" \"#00010f\" \"#000010\" \"#000110\" \"#000011\" \"#000111\" \"#000012\" \"#000112\" \"#000013\" \"#000113\" \"#000014\" \"#000114\" \"#000015\" \"#000115\" \"#000016\" \"#000116\")) (warm-redisplay 0 nil) (unused-face-change 0 nil) (redisplay-after-face-change 44 (\"#000001\" \"#000101\" \"#000002\" \"#000102\" \"#000003\" \"#000103\" \"#000004\" \"#000104\" \"#000005\" \"#000105\" \"#000006\" \"#000106\" \"#000007\" \"#000107\" \"#000008\" \"#000108\" \"#000009\" \"#000109\" \"#00000a\" \"#00010a\" \"#00000b\" \"#00010b\" \"#00000c\" \"#00010c\" \"#00000d\" \"#00010d\" \"#00000e\" \"#00010e\" \"#00000f\" \"#00010f\" \"#000010\" \"#000110\" \"#000011\" \"#000111\" \"#000012\" \"#000112\" \"#000013\" \"#000113\" \"#000014\" \"#000114\" \"#000015\" \"#000115\" \"#000016\" \"#000116\")) (inherited-nonbasic-glyph 3 (\"#345678\" \"#234567\" \"#654321\")) (warm-nonbasic-glyph 0 nil) (palette-assignment 0 nil) (explicit-cache-clear 46 (\"#000001\" \"#000101\" \"#000002\" \"#000102\" \"#000003\" \"#000103\" \"#000004\" \"#000104\" \"#000005\" \"#000105\" \"#000006\" \"#000106\" \"#000007\" \"#000107\" \"#000008\" \"#000108\" \"#000009\" \"#000109\" \"#00000a\" \"#00010a\" \"#00000b\" \"#00010b\" \"#00000c\" \"#00010c\" \"#00000d\" \"#00010d\" \"#00000e\" \"#00010e\" \"#00000f\" \"#00010f\" \"#000010\" \"#000110\" \"#000011\" \"#000111\" \"#000012\" \"#000112\" \"#000013\" \"#000113\" \"#000014\" \"#000114\" \"#000015\" \"#000115\" \"#000016\" \"#000116\" \"#345678\" \"#654321\")) (foreground-parameter-change 44 (\"#abcdef\" \"#000101\" \"#000002\" \"#000102\" \"#000003\" \"#000103\" \"#000004\" \"#000104\" \"#000005\" \"#000105\" \"#000006\" \"#000106\" \"#000007\" \"#000107\" \"#000008\" \"#000108\" \"#000009\" \"#000109\" \"#00000a\" \"#00010a\" \"#00000b\" \"#00010b\" \"#00000c\" \"#00010c\" \"#00000d\" \"#00010d\" \"#00000e\" \"#00010e\" \"#00000f\" \"#00010f\" \"#000010\" \"#000110\" \"#000011\" \"#000111\" \"#000012\" \"#000112\" \"#000013\" \"#000113\" \"#000014\" \"#000114\" \"#000015\" \"#000115\" \"#000016\" \"#000116\")) (redisplay-after-parameter-change 46 (\"#abcdef\" \"#000101\" \"#000002\" \"#000102\" \"#000003\" \"#000103\" \"#000004\" \"#000104\" \"#000005\" \"#000105\" \"#000006\" \"#000106\" \"#000007\" \"#000107\" \"#000008\" \"#000108\" \"#000009\" \"#000109\" \"#00000a\" \"#00010a\" \"#00000b\" \"#00010b\" \"#00000c\" \"#00010c\" \"#00000d\" \"#00010d\" \"#00000e\" \"#00010e\" \"#00000f\" \"#00010f\" \"#000010\" \"#000110\" \"#000011\" \"#000111\" \"#000012\" \"#000112\" \"#000013\" \"#000113\" \"#000014\" \"#000114\" \"#000015\" \"#000115\" \"#000016\" \"#000116\" \"#345678\" \"#654321\")) (function-redefinition 0 nil) (redefinition-after-cache-clear 46 (\"redefined:#abcdef\" \"redefined:#000101\" \"redefined:#000002\" \"redefined:#000102\" \"redefined:#000003\" \"redefined:#000103\" \"redefined:#000004\" \"redefined:#000104\" \"redefined:#000005\" \"redefined:#000105\" \"redefined:#000006\" \"redefined:#000106\" \"redefined:#000007\" \"redefined:#000107\" \"redefined:#000008\" \"redefined:#000108\" \"redefined:#000009\" \"redefined:#000109\" \"redefined:#00000a\" \"redefined:#00010a\" \"redefined:#00000b\" \"redefined:#00010b\" \"redefined:#00000c\" \"redefined:#00010c\" \"redefined:#00000d\" \"redefined:#00010d\" \"redefined:#00000e\" \"redefined:#00010e\" \"redefined:#00000f\" \"redefined:#00010f\" \"redefined:#000010\" \"redefined:#000110\" \"redefined:#000011\" \"redefined:#000111\" \"redefined:#000012\" \"redefined:#000112\" \"redefined:#000013\" \"redefined:#000113\" \"redefined:#000014\" \"redefined:#000114\" \"redefined:#000015\" \"redefined:#000115\" \"redefined:#000016\" \"redefined:#000116\" \"redefined:#345678\" \"redefined:#654321\")))""##
        ]],
    );
    terminal.finish();
}

/// Repeated colors still cross the Lisp callback boundary for each realized
/// basic face.  GNU does not memoize tty-color-desc once per distinct string.
#[test]
fn tty_face_lifecycle_repeated_colors_preserve_callback_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let terminal = TestTerminal::new();
    let form = format!(
        r##"(progn {TRACE_SUPPORT}
  (set-face-attribute 'mode-line nil :foreground "#112233" :background "#332211")
  (advice-add 'tty-color-desc :around #'fc-desc)
  (setq fc-frame
    (make-terminal-frame
      (list (cons 'tty (getenv "FACECACHE_TEST_TTY")) '(tty-type . "xterm-256color"))))
  (fc-mark 'frame-creation)
  (select-frame fc-frame)
  (switch-to-buffer (get-buffer-create "fc-repeated-tty"))
  (setq mode-line-format nil)
  (erase-buffer)
  (insert "a")
  (redisplay t)
  (fc-mark 'first-redisplay)
  (redisplay t)
  (fc-mark 'warm-redisplay)
  (nreverse fc-events))"##
    );
    common::assert_oracle_parity_with_env_expect(
        &form,
        &[
            ("FACECACHE_TEST_TTY", &terminal.name),
            ("NEOVM_FACE_REALIZATION_LIFECYCLE", "1"),
        ],
        expect_test::expect![[
            r##""OK ((frame-creation 0 nil) (first-redisplay 42 (\"unspecified-fg\" \"unspecified-bg\" \"#112233\" \"#332211\" \"#112233\" \"#332211\" \"unspecified-bg\" \"unspecified-fg\" \"gray\" \"#112233\" \"#332211\" \"#112233\" \"#332211\" \"unspecified-fg\" \"unspecified-bg\" \"unspecified-fg\" \"unspecified-bg\" \"unspecified-fg\" \"unspecified-fg\" \"unspecified-bg\" \"unspecified-fg\" \"unspecified-bg\" \"#112233\" \"#332211\" \"gray60\" \"unspecified-bg\" \"gray80\" \"unspecified-bg\" \"gray40\" \"unspecified-bg\" \"unspecified-fg\" \"unspecified-bg\" \"unspecified-fg\" \"unspecified-bg\" \"unspecified-fg\" \"grey\" \"unspecified-fg\" \"grey\" \"unspecified-fg\" \"grey\" \"unspecified-fg\" \"unspecified-bg\")) (warm-redisplay 0 nil))""##
        ]],
    );
    terminal.finish();
}

/// A failed realization preserves the unknown specification for retry after
/// invalidation. Anonymous and overlay faces cache complete attributes, so a
/// changed weight realizes the same foreground string again (xfaces.c:5084-5091).
#[test]
fn tty_face_lifecycle_failed_color_retry_and_complete_anonymous_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let terminal = TestTerminal::new();
    let form = format!(
        r##"(progn {TRACE_SUPPORT}
  (setq inhibit-message t)
  (dolist (face
    '(default mode-line-active mode-line-inactive tool-bar fringe
      header-line-active header-line-inactive scroll-bar border cursor mouse menu
      vertical-border window-divider window-divider-first-pixel window-divider-last-pixel
      internal-border child-frame-border tab-bar tab-line-active tab-line-inactive margin))
    (set-face-attribute face nil :foreground "red" :background "black" :underline nil))
  (set-face-attribute 'default nil :weight 'normal)
  (make-face 'fc-retry)
  (set-face-attribute 'fc-retry nil :foreground "facecache-retry-color")
  (defvar fc-retry-ready nil)
  (defun fc-retry-desc (old color &optional frame)
    (push (if (eq frame (selected-frame)) color (list 'wrong-frame color)) fc-calls)
    (cond
      ((equal color "facecache-retry-color")
       (and fc-retry-ready (list color 1 65535 0 0)))
      ((equal color "facecache-plist-color") (list color 1 65535 0 0))
      (t (funcall old color frame))))
  (advice-add 'tty-color-desc :around #'fc-retry-desc)
  (setq fc-frame
    (make-terminal-frame
      (list (cons 'tty (getenv "FACECACHE_TEST_TTY")) '(tty-type . "xterm-256color"))))
  (fc-mark 'frame-creation)
  (select-frame fc-frame)
  (setq tty-defined-color-alist
    '(("red" 1 65535 0 0) ("black" 0 0 0 0)))
  (switch-to-buffer (get-buffer-create "fc-retry-tty"))
  (setq mode-line-format nil)
  (erase-buffer)
  (insert "λ")
  (redisplay t)
  (fc-mark 'basic-exact-palette)
  (put-text-property 1 2 'face 'fc-retry)
  (redisplay t)
  (fc-mark 'unknown-first-glyph)
  (redisplay t)
  (fc-mark 'unknown-warm)
  (setq fc-retry-ready t)
  (redisplay t)
  (fc-mark 'resolver-now-succeeds-without-invalidation)
  (clear-face-cache)
  (redisplay t)
  (fc-mark 'retry-after-invalidation)
  (put-text-property 1 2 'face '(:foreground "facecache-plist-color"))
  (redisplay t)
  (fc-mark 'anonymous-plist)
  (setq fc-overlay (make-overlay 1 2))
  (overlay-put fc-overlay 'face '(:foreground "facecache-plist-color" :weight bold))
  (redisplay t)
  (fc-mark 'same-color-different-overlay-attributes)
  (redisplay t)
  (fc-mark 'overlay-warm)
  (nreverse fc-events))"##
    );
    common::assert_oracle_parity_with_env_expect(
        &form,
        &[
            ("FACECACHE_TEST_TTY", &terminal.name),
            ("NEOVM_FACE_REALIZATION_LIFECYCLE", "1"),
        ],
        expect_test::expect![[
            r#""OK ((frame-creation 0 nil) (basic-exact-palette 0 nil) (unknown-first-glyph 1 (\"facecache-retry-color\")) (unknown-warm 0 nil) (resolver-now-succeeds-without-invalidation 0 nil) (retry-after-invalidation 1 (\"facecache-retry-color\")) (anonymous-plist 1 (\"facecache-plist-color\")) (same-color-different-overlay-attributes 1 (\"facecache-plist-color\")) (overlay-warm 0 nil))""#
        ]],
    );
    terminal.finish();
}

/// GNU binds inhibit-redisplay around default-face realization because Lisp
/// color callbacks can themselves redisplay (xfaces.c:6104-6112).
#[test]
fn tty_face_lifecycle_default_color_callback_inhibits_recursive_redisplay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let terminal = TestTerminal::new();
    let form = format!(
        r##"(progn {TRACE_SUPPORT} {EXACT_BASIC_FACE_SETUP} {EXACT_TTY_BUFFER_SETUP}
  (defvar fc-redisplay-hooks 0)
  (defvar fc-in-recursive-probe nil)
  (setq pre-redisplay-function
    (lambda (&rest _) (setq fc-redisplay-hooks (1+ fc-redisplay-hooks))))
  (defun fc-recursive-desc (old color &optional frame)
    (let ((before fc-redisplay-hooks))
      (unless fc-in-recursive-probe
        (let ((fc-in-recursive-probe t)) (redisplay t)))
      (push (list color (not (null inhibit-redisplay))
                  (- fc-redisplay-hooks before)
                  (eq frame (selected-frame))) fc-calls))
    (list color 1 65535 0 0))
  (advice-add 'tty-color-desc :around #'fc-recursive-desc)
  (modify-frame-parameters nil '((foreground-color . "facecache-recursive-default")))
  (fc-mark 'immediate-default-realization)
  (redisplay t)
  (fc-mark 'redisplay-after-default-change)
  (list (nreverse fc-events) fc-redisplay-hooks))"##
    );
    common::assert_oracle_parity_with_env_expect(
        &form,
        &[
            ("FACECACHE_TEST_TTY", &terminal.name),
            ("NEOVM_FACE_REALIZATION_LIFECYCLE", "1"),
        ],
        expect_test::expect![[
            r#""OK (((frame-creation 0 nil) (basic-exact-palette 0 nil) (immediate-default-realization 1 ((\"facecache-recursive-default\" t 0 t))) (redisplay-after-default-change 1 ((\"facecache-recursive-default\" t 0 t)))) 1)""#
        ]],
    );
    terminal.finish();
}

/// Callback mutation refreshes its face source within the same display attempt;
/// frame-local face_change is consumed by init_iterator
/// (xfaces.c:3098-3105, xdisp.c:3265-3280).
#[test]
fn tty_face_lifecycle_callback_mutation_refreshes_displayed_face_source() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let terminal = TestTerminal::new();
    let form = format!(
        r##"(progn {TRACE_SUPPORT} {EXACT_BASIC_FACE_SETUP} {EXACT_TTY_BUFFER_SETUP}
  (make-face 'fc-mutated)
  (set-face-attribute 'fc-mutated nil :foreground "facecache-mutation-old")
  (defvar fc-mutated-once nil)
  (defun fc-mutating-desc (old color &optional frame)
    (push (if (eq frame (selected-frame)) color (list 'wrong-frame color)) fc-calls)
    (when (and (equal color "facecache-mutation-old") (not fc-mutated-once))
      (setq fc-mutated-once t)
      (set-face-attribute 'fc-mutated frame :foreground "facecache-mutation-new"))
    (list color 1 65535 0 0))
  (advice-add 'tty-color-desc :around #'fc-mutating-desc)
  (put-text-property 1 2 'face 'fc-mutated)
  (redisplay t)
  (fc-mark 'mutating-glyph-first-display)
  (redisplay t)
  (fc-mark 'display-after-callback-mutation)
  (redisplay t)
  (fc-mark 'updated-face-warm)
  (list (nreverse fc-events)
        (face-attribute 'fc-mutated :foreground fc-frame)))"##
    );
    common::assert_oracle_parity_with_env_expect(
        &form,
        &[
            ("FACECACHE_TEST_TTY", &terminal.name),
            ("NEOVM_FACE_REALIZATION_LIFECYCLE", "1"),
        ],
        expect_test::expect![[
            r#""OK (((frame-creation 0 nil) (basic-exact-palette 0 nil) (mutating-glyph-first-display 2 (\"facecache-mutation-old\" \"facecache-mutation-new\")) (display-after-callback-mutation 0 nil) (updated-face-warm 0 nil)) \"facecache-mutation-new\")""#
        ]],
    );
    terminal.finish();
}

/// Status-line string properties demand non-basic faces. A symbolic underline
/// foreground-color reuses the foreground; the same spelling as a string
/// requires color lookup (xfaces.c:6770-6778).
#[test]
fn tty_face_lifecycle_status_line_faces_and_underline_foreground_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let terminal = TestTerminal::new();
    let form = format!(
        r##"(progn {TRACE_SUPPORT} {EXACT_BASIC_FACE_SETUP} {EXACT_TTY_BUFFER_SETUP}
  (make-face 'fc-mode)
  (set-face-attribute 'fc-mode nil :foreground "facecache-mode-color")
  (make-face 'fc-header)
  (set-face-attribute 'fc-header nil :foreground "facecache-header-color")
  (make-face 'fc-underlined)
  (set-face-attribute 'fc-underlined nil :foreground "red"
    :underline '(:color foreground-color :style line))
  (defun fc-ui-desc (old color &optional frame)
    (push (if (eq frame (selected-frame)) color (list 'wrong-frame color)) fc-calls)
    (list color 1 65535 0 0))
  (advice-add 'tty-color-desc :around #'fc-ui-desc)
  (setq mode-line-format (list (propertize "MODE" 'face 'fc-mode))
        header-line-format (list (propertize "HEAD" 'face 'fc-header)))
  (redisplay t)
  (fc-mark 'status-line-text-properties)
  (redisplay t)
  (fc-mark 'status-lines-warm)
  (setq mode-line-format nil header-line-format nil)
  (put-text-property 1 2 'face 'fc-underlined)
  (redisplay t)
  (fc-mark 'named-underline-foreground-symbol)
  (put-text-property 1 2 'face
    '(:foreground "red" :underline (:color foreground-color :style wave)))
  (redisplay t)
  (fc-mark 'anonymous-underline-foreground-symbol)
  (put-text-property 1 2 'face
    '(:foreground "red" :underline (:color "foreground-color" :style line)))
  (redisplay t)
  (fc-mark 'underline-foreground-spelling-as-string)
  (nreverse fc-events))"##
    );
    common::assert_oracle_parity_with_env_expect(
        &form,
        &[
            ("FACECACHE_TEST_TTY", &terminal.name),
            ("NEOVM_FACE_REALIZATION_LIFECYCLE", "1"),
        ],
        expect_test::expect![[
            r#""OK ((frame-creation 0 nil) (basic-exact-palette 0 nil) (status-line-text-properties 2 (\"facecache-mode-color\" \"facecache-header-color\")) (status-lines-warm 0 nil) (named-underline-foreground-symbol 0 nil) (anonymous-underline-foreground-symbol 0 nil) (underline-foreground-spelling-as-string 1 (\"foreground-color\")))""#
        ]],
    );
    terminal.finish();
}

/// tty_lookup_color directly calls Lisp, so color callback signals escape
/// foreground/background parameter changes (xfaces.c:1093,3888-3910).
#[test]
fn tty_face_lifecycle_frame_parameter_color_callback_errors_propagate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let terminal = TestTerminal::new();
    let form = format!(
        r##"(progn {TRACE_SUPPORT} {EXACT_BASIC_FACE_SETUP} {EXACT_TTY_BUFFER_SETUP}
  (defun fc-error-desc (old color &optional frame)
    (push (if (eq frame (selected-frame)) color (list 'wrong-frame color)) fc-calls)
    (if (string-prefix-p "facecache-error-" color)
        (signal 'error (list 'facecache-color-callback color))
      (funcall old color frame)))
  (advice-add 'tty-color-desc :around #'fc-error-desc)
  (setq fc-foreground-result
    (condition-case err
        (progn
          (modify-frame-parameters nil '((foreground-color . "facecache-error-fg")))
          'returned)
      (error err)))
  (fc-mark 'foreground-parameter-error)
  (modify-frame-parameters nil '((foreground-color . "red")))
  (fc-mark 'restore-foreground)
  (setq fc-background-result
    (condition-case err
        (progn
          (modify-frame-parameters nil '((background-color . "facecache-error-bg")))
          'returned)
      (error err)))
  (fc-mark 'background-parameter-error)
  (list (nreverse fc-events) fc-foreground-result fc-background-result
        (frame-parameter fc-frame 'foreground-color)
        (frame-parameter fc-frame 'background-color)))"##
    );
    common::assert_oracle_parity_with_env_expect(
        &form,
        &[
            ("FACECACHE_TEST_TTY", &terminal.name),
            ("NEOVM_FACE_REALIZATION_LIFECYCLE", "1"),
        ],
        expect_test::expect![[
            r#""OK (((frame-creation 0 nil) (basic-exact-palette 0 nil) (foreground-parameter-error 1 (\"facecache-error-fg\")) (restore-foreground 0 nil) (background-parameter-error 1 (\"facecache-error-bg\"))) (error facecache-color-callback \"facecache-error-fg\") (error facecache-color-callback \"facecache-error-bg\") \"red\" \"facecache-error-bg\")""#
        ]],
    );
    terminal.finish();
}

/// GNU maps every underline :color entry in order, including entries later
/// overwritten or reset to foreground-color (xfaces.c:6758-6796). The complete
/// ordered plist remains part of the realized-face cache key.
#[test]
fn tty_face_lifecycle_ordered_underline_operations_precede_foreground_background() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let terminal = TestTerminal::new();
    let form = format!(
        r##"(progn {TRACE_SUPPORT} {EXACT_BASIC_FACE_SETUP} {EXACT_TTY_BUFFER_SETUP}
  (make-face 'fc-ordered-underline)
  (set-face-attribute 'fc-ordered-underline nil
    :foreground "#334455" :background "#556677"
    :underline '(:color "#111111" :color foreground-color :color "#222222"))
  (advice-add 'tty-color-desc :around #'fc-desc)
  (put-text-property 1 2 'face 'fc-ordered-underline)
  (redisplay t)
  (fc-mark 'named-ordered-underline)
  (redisplay t)
  (fc-mark 'named-ordered-underline-warm)
  (put-text-property 1 2 'face
    '(:foreground "#334455" :background "#556677" :weight bold
      :underline (:color "#111111" :color foreground-color :color "#222222")))
  (redisplay t)
  (fc-mark 'anonymous-ordered-underline)
  (redisplay t)
  (fc-mark 'anonymous-ordered-underline-warm)
  (put-text-property 1 2 'face
    '(:foreground "#334455" :background "#556677" :weight bold
      :underline (:color "#222222" :color foreground-color
                  :color "#111111" :color "#222222")))
  (redisplay t)
  (fc-mark 'different-operations-same-final-color)
  (redisplay t)
  (fc-mark 'different-operations-warm)
  (nreverse fc-events))"##
    );
    common::assert_oracle_parity_with_env_expect(
        &form,
        &[
            ("FACECACHE_TEST_TTY", &terminal.name),
            ("NEOVM_FACE_REALIZATION_LIFECYCLE", "1"),
        ],
        expect_test::expect![[
            r##""OK ((frame-creation 0 nil) (basic-exact-palette 0 nil) (named-ordered-underline 4 (\"#111111\" \"#222222\" \"#334455\" \"#556677\")) (named-ordered-underline-warm 0 nil) (anonymous-ordered-underline 4 (\"#111111\" \"#222222\" \"#334455\" \"#556677\")) (anonymous-ordered-underline-warm 0 nil) (different-operations-same-final-color 5 (\"#222222\" \"#111111\" \"#222222\" \"#334455\" \"#556677\")) (different-operations-warm 0 nil))""##
        ]],
    );
    terminal.finish();
}

/// GNU realizes tab-line-active/inactive as basics, not their canonical
/// tab-line parent (xfaces.c:5980-5981). Window selection picks the active or
/// inactive role (dispextern.h:1613-1632, xdisp.c:28100-28107), including the
/// base attributes underneath a non-basic string face property.
#[test]
fn tty_face_lifecycle_tab_line_basic_roles_and_nonbasic_format_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let terminal = TestTerminal::new();
    let form = format!(
        r##"(progn {TRACE_SUPPORT} {EXACT_BASIC_FACE_SETUP} {EXACT_TTY_BUFFER_SETUP}
  (setq mode-line-in-non-selected-windows t)
  (set-face-attribute 'tab-line nil :foreground "facecache-tab-canonical" :weight 'normal)
  (set-face-attribute 'tab-line-active nil :foreground "facecache-tab-active" :weight 'normal)
  (set-face-attribute 'tab-line-inactive nil :foreground "facecache-tab-inactive" :weight 'bold)
  (make-face 'fc-tab-property)
  (set-face-attribute 'fc-tab-property nil :foreground "facecache-tab-property")
  (defun fc-tab-desc (old color &optional frame)
    (push (if (eq frame (selected-frame)) color (list 'wrong-frame color)) fc-calls)
    (list color 1 65535 0 0))
  (advice-add 'tty-color-desc :around #'fc-tab-desc)
  (redisplay t)
  (fc-mark 'customized-basic-tab-roles)
  (setq tab-line-format (list "BASE " (propertize "COLOR" 'face 'fc-tab-property)))
  (redisplay t)
  (fc-mark 'active-tab-line-property-demand)
  (redisplay t)
  (fc-mark 'active-tab-line-warm)
  (setq fc-other-window (split-window-below))
  (redisplay t)
  (fc-mark 'inactive-tab-line-distinct-base-weight)
  (redisplay t)
  (fc-mark 'two-tab-lines-warm)
  (select-window fc-other-window)
  (redisplay t)
  (fc-mark 'switch-selected-window-reuses-both-roles)
  (setq mode-line-in-non-selected-windows nil
        tab-line-format
          (list "BASE " (propertize "OVERRIDE" 'face
                          '(:foreground "facecache-tab-option"))))
  (redisplay t)
  (fc-mark 'nil-option-gives-both-windows-active-base)
  (redisplay t)
  (fc-mark 'nil-option-warm)
  (nreverse fc-events))"##
    );
    common::assert_oracle_parity_with_env_expect(
        &form,
        &[
            ("FACECACHE_TEST_TTY", &terminal.name),
            ("NEOVM_FACE_REALIZATION_LIFECYCLE", "1"),
        ],
        expect_test::expect![[
            r#""OK ((frame-creation 0 nil) (basic-exact-palette 0 nil) (customized-basic-tab-roles 2 (\"facecache-tab-active\" \"facecache-tab-inactive\")) (active-tab-line-property-demand 1 (\"facecache-tab-property\")) (active-tab-line-warm 0 nil) (inactive-tab-line-distinct-base-weight 1 (\"facecache-tab-property\")) (two-tab-lines-warm 0 nil) (switch-selected-window-reuses-both-roles 0 nil) (nil-option-gives-both-windows-active-base 1 (\"facecache-tab-option\")) (nil-option-warm 0 nil))""#
        ]],
    );
    terminal.finish();
}

/// Explicit frame-local changes invalidate only that frame's realized faces
/// (xfaces.c:3102-3105,3770-3780,3936-3945). Two root frames share one TTY, so
/// selecting B hides A and isolates which frame redisplay can realize.
#[test]
fn tty_face_lifecycle_frame_local_changes_preserve_other_frame_cache() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let terminal = TestTerminal::new();
    let form = format!(
        r##"(progn {TRACE_SUPPORT} {EXACT_BASIC_FACE_SETUP}
  (make-face 'fc-per-frame)
  (set-face-attribute 'fc-per-frame nil :foreground "facecache-frame-shared")
  (make-face 'fc-custom-no-inherit)
  (set-face-attribute 'fc-custom-no-inherit nil :foreground "facecache-no-inherit-old")
  (put 'fc-custom-no-inherit 'face-no-inherit t)
  (setq fc-frame-a
    (make-terminal-frame
      (list (cons 'tty (getenv "FACECACHE_TEST_TTY")) '(tty-type . "xterm-256color"))))
  (setq tty-defined-color-alist
    '(("red" 1 65535 0 0) ("black" 0 0 0 0)))
  (select-frame fc-frame-a)
  (switch-to-buffer (get-buffer-create "fc-frame-a-buffer"))
  (setq mode-line-format nil header-line-format nil tab-line-format nil)
  (erase-buffer)
  (insert (propertize "a" 'face 'fc-per-frame))
  (defvar fc-frame-b nil)
  (defun fc-tagged-desc (old color &optional frame)
    (push (list (cond ((eq frame fc-frame-a) 'a)
                      ((eq frame fc-frame-b) 'b)
                      (t 'other)) color) fc-calls)
    (list color 1 65535 0 0))
  (advice-add 'tty-color-desc :around #'fc-tagged-desc)
  (redisplay t)
  (fc-mark 'frame-a-first-display)
  (redisplay t)
  (fc-mark 'frame-a-warm)
  (setq fc-frame-b
    (make-terminal-frame (list (cons 'terminal (frame-terminal fc-frame-a)))))
  (fc-mark 'frame-b-creation)
  (select-frame fc-frame-b)
  (switch-to-buffer (get-buffer-create "fc-frame-b-buffer"))
  (setq mode-line-format nil header-line-format nil tab-line-format nil)
  (erase-buffer)
  (insert (propertize "b" 'face 'fc-per-frame))
  (redisplay t)
  (fc-mark 'frame-b-first-display)
  (redisplay t)
  (fc-mark 'frame-b-warm)
  (set-face-attribute 'fc-per-frame fc-frame-a :foreground "facecache-frame-a-updated")
  (fc-mark 'frame-a-local-face-change)
  (redisplay t)
  (fc-mark 'unchanged-frame-b-after-a-local-face-change)
  (select-frame fc-frame-a)
  (redisplay t)
  (fc-mark 'frame-a-after-local-face-change)
  (redisplay t)
  (fc-mark 'frame-a-warm-after-local-face-change)
  (select-frame fc-frame-b)
  (redisplay t)
  (fc-mark 'frame-b-still-warm-after-a-realization)
  (modify-frame-parameters fc-frame-a '((foreground-color . "facecache-frame-a-default")))
  (fc-mark 'frame-a-foreground-parameter-eager)
  (redisplay t)
  (fc-mark 'unchanged-frame-b-after-a-foreground-parameter)
  (select-frame fc-frame-a)
  (redisplay t)
  (fc-mark 'frame-a-after-foreground-parameter)
  (redisplay t)
  (fc-mark 'frame-a-warm-after-parameter-change)
  (set-face-attribute 'fc-per-frame fc-frame-a :foreground "facecache-frame-a-updated")
  (redisplay t)
  (fc-mark 'equal-frame-local-setter)
  (set-face-attribute 'fc-per-frame t :foreground "facecache-defaults-only")
  (redisplay t)
  (fc-mark 'defaults-only-setter-frame-a)
  (select-frame fc-frame-b)
  (redisplay t)
  (fc-mark 'defaults-only-setter-frame-b)
  (set-face-attribute 'cursor fc-frame-a :foreground "facecache-frame-a-cursor")
  (redisplay t)
  (fc-mark 'cursor-face-no-inherit-keeps-frame-b)
  (select-frame fc-frame-a)
  (redisplay t)
  (fc-mark 'cursor-face-no-inherit-keeps-frame-a)
  (clear-face-cache)
  (redisplay t)
  (fc-mark 'explicit-clear-realizes-updated-cursor)
  (select-frame fc-frame-b)
  (redisplay t)
  (fc-mark 'explicit-clear-also-invalidated-frame-b)
  (select-frame fc-frame-a)
  (put-text-property 1 2 'face 'fc-custom-no-inherit)
  (redisplay t)
  (fc-mark 'custom-face-no-inherit-first-demand)
  (set-face-attribute 'fc-custom-no-inherit fc-frame-a :foreground "facecache-no-inherit-new")
  (goto-char (point-max))
  (insert " ")
  (redisplay t)
  (fc-mark 'custom-face-no-inherit-new-source-without-basic-purge)
  (set-face-attribute 'fc-custom-no-inherit fc-frame-a :foreground "facecache-no-inherit-old")
  (insert " ")
  (redisplay t)
  (fc-mark 'custom-face-no-inherit-old-realization-retained)
  (nreverse fc-events))"##
    );
    common::assert_oracle_parity_with_env_expect(
        &form,
        &[
            ("FACECACHE_TEST_TTY", &terminal.name),
            ("NEOVM_FACE_REALIZATION_LIFECYCLE", "1"),
        ],
        expect_test::expect![[
            r#""OK ((frame-a-first-display 1 ((a \"facecache-frame-shared\"))) (frame-a-warm 0 nil) (frame-b-creation 0 nil) (frame-b-first-display 1 ((b \"facecache-frame-shared\"))) (frame-b-warm 0 nil) (frame-a-local-face-change 0 nil) (unchanged-frame-b-after-a-local-face-change 0 nil) (frame-a-after-local-face-change 1 ((a \"facecache-frame-a-updated\"))) (frame-a-warm-after-local-face-change 0 nil) (frame-b-still-warm-after-a-realization 0 nil) (frame-a-foreground-parameter-eager 1 ((a \"facecache-frame-a-default\"))) (unchanged-frame-b-after-a-foreground-parameter 0 nil) (frame-a-after-foreground-parameter 2 ((a \"facecache-frame-a-default\") (a \"facecache-frame-a-updated\"))) (frame-a-warm-after-parameter-change 0 nil) (equal-frame-local-setter 0 nil) (defaults-only-setter-frame-a 0 nil) (defaults-only-setter-frame-b 0 nil) (cursor-face-no-inherit-keeps-frame-b 0 nil) (cursor-face-no-inherit-keeps-frame-a 0 nil) (explicit-clear-realizes-updated-cursor 3 ((a \"facecache-frame-a-default\") (a \"facecache-frame-a-cursor\") (a \"facecache-frame-a-updated\"))) (explicit-clear-also-invalidated-frame-b 1 ((b \"facecache-frame-shared\"))) (custom-face-no-inherit-first-demand 1 ((a \"facecache-no-inherit-old\"))) (custom-face-no-inherit-new-source-without-basic-purge 1 ((a \"facecache-no-inherit-new\"))) (custom-face-no-inherit-old-realization-retained 0 nil))""#
        ]],
    );
    terminal.finish();
}

/// GNU xfaces.c:4351-4370 force-realizes an already-cached default under
/// inhibit-redisplay, preserving unrelated realized entries. Global color
/// strings then invoke the frame parameter handlers before leaving that binding
/// (4396-4410), and their frame-local invalidation reaches the next redisplay.
#[test]
fn tty_face_lifecycle_global_default_merge_forces_cached_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let terminal = TestTerminal::new();
    let form = format!(
        r##"(progn {TRACE_SUPPORT} {EXACT_BASIC_FACE_SETUP}
  (defun fc-desc (old color &optional frame)
    (push (list color inhibit-redisplay (eq frame fc-frame)) fc-calls)
    (if (string-prefix-p "#" color)
        (list color 1 65535 0 0)
      (funcall old color frame)))
  (advice-add 'tty-color-desc :around #'fc-desc)
  (setq fc-frame
    (make-terminal-frame
      (list (cons 'tty (getenv "FACECACHE_TEST_TTY")) '(tty-type . "xterm-256color"))))
  (fc-mark 'frame-creation)
  (internal-merge-in-global-face 'default fc-frame)
  (fc-mark 'uncached-default-merge)
  (select-frame fc-frame)
  (setq tty-defined-color-alist
    '(("red" 1 65535 0 0) ("black" 0 0 0 0)))
  (switch-to-buffer (get-buffer-create "fc-default-merge"))
  (setq mode-line-format nil header-line-format nil)
  (erase-buffer)
  (insert "a")
  (redisplay t)
  (fc-mark 'first-redisplay)
  (make-face 'fc-merge-cached)
  (set-face-attribute 'fc-merge-cached fc-frame :foreground "#778899" :background "black")
  (put-text-property (point-min) (point-max) 'face 'fc-merge-cached)
  (redisplay t)
  (fc-mark 'nonbasic-first)
  (set-face-attribute 'default fc-frame :foreground "#112233" :background "#445566")
  (redisplay t)
  (fc-mark 'local-default-setup)
  (redisplay t)
  (fc-mark 'warm-setup)
  (set-face-attribute 'default t :foreground 'unspecified :background 'unspecified)
  (fc-mark 'global-colors-unspecified)
  (internal-merge-in-global-face 'default fc-frame)
  (fc-mark 'cached-default-equal-merge)
  (goto-char (point-max))
  (insert (propertize " " 'face 'fc-merge-cached))
  (redisplay t)
  (fc-mark 'cached-default-redisplay)
  (internal-merge-in-global-face 'default fc-frame)
  (fc-mark 'cached-default-equal-repeat)
  (insert (propertize " " 'face 'fc-merge-cached))
  (redisplay t)
  (fc-mark 'cached-default-repeat-redisplay)
  (set-face-attribute 'default t :foreground "#223344" :background "#556677")
  (fc-mark 'global-new-colors)
  (internal-merge-in-global-face 'default fc-frame)
  (fc-mark 'cached-default-color-merge)
  (redisplay t)
  (fc-mark 'after-color-merge-redisplay)
  (redisplay t)
  (fc-mark 'after-color-merge-warm)
  (nreverse fc-events))"##
    );
    common::assert_oracle_parity_with_env_expect(
        &form,
        &[
            ("FACECACHE_TEST_TTY", &terminal.name),
            ("NEOVM_FACE_REALIZATION_LIFECYCLE", "1"),
        ],
        expect_test::expect![[
            r##""OK ((frame-creation 0 nil) (uncached-default-merge 0 nil) (first-redisplay 0 nil) (nonbasic-first 1 ((\"#778899\" nil t))) (local-default-setup 8 ((\"#112233\" t t) (\"#445566\" nil t) (\"#445566\" nil t) (\"#112233\" t t) (\"#445566\" t t) (\"#112233\" t t) (\"#445566\" t t) (\"#778899\" nil t))) (warm-setup 0 nil) (global-colors-unspecified 0 nil) (cached-default-equal-merge 2 ((\"#112233\" t t) (\"#445566\" t t))) (cached-default-redisplay 0 nil) (cached-default-equal-repeat 2 ((\"#112233\" t t) (\"#445566\" t t))) (cached-default-repeat-redisplay 0 nil) (global-new-colors 0 nil) (cached-default-color-merge 8 ((\"#223344\" t t) (\"#556677\" t t) (\"#223344\" t t) (\"#556677\" t t) (\"#556677\" t t) (\"#556677\" t t) (\"#223344\" t t) (\"#556677\" t t))) (after-color-merge-redisplay 3 ((\"#223344\" t t) (\"#556677\" t t) (\"#778899\" nil t))) (after-color-merge-warm 0 nil))""##
        ]],
    );
    terminal.finish();
}
