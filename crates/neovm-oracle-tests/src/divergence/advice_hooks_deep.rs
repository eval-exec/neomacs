//! Divergence tests: advice, hooks, before/after/around deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_advice_add_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (43 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-advice-fn-xxx () 42)
  (advice-add 'test-advice-fn-xxx :around
    (lambda (fn &rest args) (1+ (apply fn args))))
  (list (test-advice-fn-xxx)
        (progn
          (advice-remove 'test-advice-fn-xxx
            (lambda (fn &rest args) (1+ (apply fn args))))
          (test-advice-fn-xxx)))) "#,
        expect,
    );
}

#[test]
fn divergence_advice_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'advice-add)
  (fboundp 'advice-remove)
  (fboundp 'advice-mapc)
  (member :before '(before after around override))
  (member :after '(before after around override))) "#,
        expect,
    );
}

#[test]
fn divergence_hooks_add_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (((closure (t) nil 'hook-called)) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-hook-var-xxx nil)
  (add-hook 'test-hook-var-xxx (lambda () 'hook-called))
  (list test-hook-var-xxx
        (progn
          (remove-hook 'test-hook-var-xxx (lambda () 'hook-called))
          test-hook-var-xxx))) "#,
        expect,
    );
}

#[test]
fn divergence_hook_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil (append-fn-xxx t))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-hook-depth-xxx nil)
  (add-hook 'test-hook-depth-xxx 'append-fn-xxx nil t)
  (list (listp test-hook-depth-xxx)
        (boundp 'test-hook-depth-xxx)
        (remove-hook 'test-hook-depth-xxx 'append-fn-xxx)
        test-hook-depth-xxx)) "#,
        expect,
    );
}

#[test]
fn divergence_run_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'run-hooks)
  (fboundp 'run-hook-with-args)
  (fboundp 'run-hook-with-args-until-success)
  (fboundp 'run-hook-with-args-until-failure))"#,
        expect,
    );
}

#[test]
fn divergence_add_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'add-function)
  (fboundp 'remove-function)
  (fboundp 'function-put)
  (fboundp 'function-get))"#,
        expect,
    );
}

#[test]
fn divergence_narrowed_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'change-major-mode-hook)
  (boundp 'after-change-major-mode-hook)
  (listp change-major-mode-hook)
  (listp after-change-major-mode-hook))"#,
        expect,
    );
}

#[test]
fn divergence_find_file_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t (find-file-hook find-file-hooks find-tag-hook first-change-hook focus-in-hook focus-out-hook font-lock-mode-hook font-lock-mode-off-hook font-lock-mode-on-hook global-auto-composition-mode-hook global-auto-composition-mode-off-hook global-auto-composition-mode-on-hook global-eldoc-mode-hook global-eldoc-mode-off-hook global-eldoc-mode-on-hook global-font-lock-mode-hook global-font-lock-mode-off-hook global-font-lock-mode-on-hook global-prettify-symbols-mode-hook global-prettify-symbols-mode-off-hook global-prettify-symbols-mode-on-hook global-visual-line-mode-hook global-visual-line-mode-off-hook global-visual-line-mode-on-hook grep-setup-hook hack-local-variables-hook hook hook--depth-alist hookvar horizontal-scroll-bar-mode-hook horizontal-scroll-bar-mode-off-hook horizontal-scroll-bar-mode-on-hook indent-tabs-mode-hook indent-tabs-mode-off-hook indent-tabs-mode-on-hook inhibit-modification-hooks inhibit-point-motion-hooks inhibit-startup-hooks input-method-activate-hook input-method-after-insert-chunk-hook input-method-deactivate-hook insert-behind-hooks insert-in-front-hooks isearch-fold-quotes-mode-hook isearch-fold-quotes-mode-off-hook isearch-fold-quotes-mode-on-hook isearch-mode-end-hook isearch-mode-end-hook-quit isearch-mode-hook isearch-post-command-hook isearch-pre-command-hook isearch-update-post-hook jit-lock-debug-mode-hook jit-lock-debug-mode-off-hook jit-lock-debug-mode-on-hook jka-cmpr-hook kbd-macro-termination-hook kill-buffer-hook kill-emacs-hook lazy-count-update-hook line-number-mode-hook line-number-mode-off-hook line-number-mode-on-hook lisp-data-mode-hook lisp-indent-hook lisp-interaction-mode-hook lisp-mode-hook local-write-file-hooks lock-file-mode-hook lock-file-mode-off-hook lock-file-mode-on-hook long-line-optimizations-in-command-hooks lost-selection-mode-hook lost-selection-mode-off-hook lost-selection-mode-on-hook mail-citation-hook mail-mode-hook mail-send-hook mail-setup-hook menu-bar-mode-hook menu-bar-mode-off-hook menu-bar-mode-on-hook menu-bar-update-hook message-send-hook messages-buffer-mode-hook mh-before-send-letter-hook minibuffer-exit-hook minibuffer-inactive-mode-hook minibuffer-mode-hook minibuffer-nonselected-mode-hook minibuffer-nonselected-mode-off-hook minibuffer-nonselected-mode-on-hook minibuffer-regexp-mode-hook minibuffer-regexp-mode-off-hook minibuffer-regexp-mode-on-hook minibuffer-setup-hook minibuffer-with-setup-hook mode-line-invisible-mode-hook mode-line-invisible-mode-off-hook mode-line-invisible-mode-on-hook modification-hooks modifier-bar-mode-hook modifier-bar-mode-off-hook modifier-bar-mode-on-hook mouse-leave-buffer-hook mouse-shift-adjust-mode-hook mouse-shift-adjust-mode-off-hook mouse-shift-adjust-mode-on-hook mouse-wheel-mode-hook mouse-wheel-mode-off-hook mouse-wheel-mode-on-hook next-error-follow-minor-mode-hook next-error-follow-minor-mode-off-hook next-error-follow-minor-mode-on-hook next-error-follow-mode-post-command-hook next-error-hook normal-erase-is-backspace-mode-hook normal-erase-is-backspace-mode-off-hook normal-erase-is-backspace-mode-on-hook occur-edit-mode-hook occur-hook occur-mode-find-occurrence-hook occur-mode-hook overwrite-mode-hook overwrite-mode-off-hook overwrite-mode-on-hook paragraph-indent-minor-mode-hook paragraph-indent-minor-mode-off-hook paragraph-indent-minor-mode-on-hook paragraph-indent-text-mode-hook permanent-local-hook post-command-hook post-gc-hook post-select-region-hook post-self-insert-hook post-text-conversion-hook pre-command-hook prefix-command-preserve-state-hook prettify-symbols--post-command-hook prettify-symbols-mode-hook prettify-symbols-mode-off-hook prettify-symbols-mode-on-hook process-menu-mode-hook prog-mode-hook quit-window-hook read-extended-command-mode-hook read-extended-command-mode-off-hook read-extended-command-mode-on-hook read-only-mode-hook read-only-mode-off-hook read-only-mode-on-hook remove-hook replace-update-post-hook revert-buffer-internal-hook rfn-eshadow-setup-minibuffer-hook rfn-eshadow-update-overlay-hook rmail-mode-hook rmail-show-message-hook run-hook-query-error-with-timeout run-hook-with-args run-hook-with-args-until-failure run-hook-with-args-until-success run-hook-wrapped run-hooks run-mode-hooks run-window-configuration-change-hook scroll-bar-mode-hook scroll-bar-mode-off-hook scroll-bar-mode-on-hook set-language-environment-hook show-paren-local-mode-hook show-paren-local-mode-off-hook show-paren-local-mode-on-hook show-paren-mode-hook show-paren-mode-off-hook show-paren-mode-on-hook signal-hook-function size-indication-mode-hook size-indication-mode-off-hook size-indication-mode-on-hook special-mode-hook subr--with-wrapper-hook-no-warnings suspend-hook suspend-resume-hook tab-bar-history-mode-hook tab-bar-history-mode-off-hook tab-bar-history-mode-on-hook tab-bar-mode-hook tab-bar-mode-off-hook tab-bar-mode-on-hook tab-switcher-mode-hook tabulated-list-mode-hook tabulated-list-revert-hook temp-buffer-resize-mode-hook temp-buffer-resize-mode-off-hook temp-buffer-resize-mode-on-hook temp-buffer-setup-hook temp-buffer-show-hook temp-buffer-window-setup-hook temp-buffer-window-show-hook term-setup-hook text-mode-hook text-mode-hook-identify tool-bar-mode-hook tool-bar-mode-off-hook tool-bar-mode-on-hook tooltip-mode-hook tooltip-mode-off-hook tooltip-mode-on-hook tramp-archive-unload-hook transient-mark-mode-hook transient-mark-mode-off-hook transient-mark-mode-on-hook tty-setup-hook undelete-frame-mode-hook undelete-frame-mode-off-hook undelete-frame-mode-on-hook use-hard-newlines-hook use-hard-newlines-off-hook use-hard-newlines-on-hook vc-auto-revert-mode-hook vc-auto-revert-mode-off-hook vc-auto-revert-mode-on-hook vc-before-checkin-hook vc-checkin-hook vc-checkout-hook vc-default-find-file-hook vc-find-file-hook vc-hooks vc-kill-buffer-hook vc-mode-line-hook visible-mode-hook visible-mode-off-hook visible-mode-on-hook visual-line-mode-hook visual-line-mode-off-hook visual-line-mode-on-hook window-configuration-change-hook window-divider-mode-hook window-divider-mode-off-hook window-divider-mode-on-hook window-setup-hook window-state-change-hook with-wrapper-hook write-contents-hooks write-file-hooks x-pre-popup-menu-hook))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'find-file-hook)
  (listp find-file-hook)
  (member 'find-file-hook (apropos-internal "hook"))) "#,
        expect,
    );
}

#[test]
fn divergence_post_command_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'post-command-hook)
  (boundp 'pre-command-hook)
  (listp post-command-hook)
  (listp pre-command-hook))"#,
        expect,
    );
}

#[test]
fn divergence_idle_timer_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'run-with-idle-timer)
  (fboundp 'run-at-time)
  (fboundp 'cancel-timer)
  (fboundp 'timerp)
  (fboundp 'current-idle-time))"#,
        expect,
    );
}
