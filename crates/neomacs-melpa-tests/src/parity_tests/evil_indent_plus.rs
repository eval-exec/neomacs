use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVIL_INDENT_PLUS_MELPA_PIN, EVIL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'evil-indent-plus)

(defmacro neomacs-evil-indent-plus-test-with-buffer (text needle &rest body)
  "Run BODY in a live Evil buffer containing TEXT at NEEDLE."
  `(let ((buffer (generate-new-buffer " *evil-indent-plus-workflow*"))
         (this-command nil)
         (last-command nil)
         (kill-ring nil)
         (kill-ring-yank-pointer nil))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (text-mode)
           (insert ,text)
           (goto-char (point-min))
           (when ,needle
             (search-forward ,needle)
             (goto-char (match-beginning 0)))
           (evil-local-mode 1)
           (evil-indent-plus-default-bindings)
           (evil-normal-state)
           ,@body)
       (when (buffer-live-p buffer)
         (with-current-buffer buffer
           (when (bound-and-true-p evil-local-mode)
             (evil-local-mode -1)))
         (kill-buffer buffer)))))

(defun neomacs-evil-indent-plus-test-state ()
  "Capture the visible editing state and latest linewise kill."
  (list
   :text (buffer-substring-no-properties (point-min) (point-max))
   :point (point)
   :line (line-number-at-pos)
   :column (current-column)
   :state evil-state
   :kill (car-safe kill-ring)
   :register-type (and (car-safe kill-ring)
                       (get-text-property 0 'yank-handler (car kill-ring)))))

(defun neomacs-evil-indent-plus-test-range (command)
  "Return COMMAND's raw and expanded range with the selected text."
  (save-excursion
    (let* ((range (funcall command nil nil nil nil))
           (expanded (evil-expand-range (copy-sequence range))))
      (list
       command
       :raw (list (evil-range-beginning range)
                  (evil-range-end range)
                  (evil-type range))
       :expanded (list (evil-range-beginning expanded)
                       (evil-range-end expanded)
                       (evil-type expanded))
       :text (buffer-substring-no-properties
              (evil-range-beginning expanded)
              (evil-range-end expanded))))))
"####;

fn six_text_objects_select_exact_nested_workflow_boundaries() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-indent-plus-test-with-buffer
 "pipeline:\n  build:\n    command: cargo build\n    env:\n      RUST_LOG: warn\n\n  test:\n    command: cargo nextest run\ndeploy:\n  command: ./release\n"
 "command: cargo build"
 (mapcar #'neomacs-evil-indent-plus-test-range
         '(evil-indent-plus-i-indent
           evil-indent-plus-a-indent
           evil-indent-plus-i-indent-up
           evil-indent-plus-a-indent-up
           evil-indent-plus-i-indent-up-down
           evil-indent-plus-a-indent-up-down)))
"####;
    let expected = expect![[
        r#"OK ((evil-indent-plus-i-indent :raw (20 74 line) :expanded (20 75 line) :text "    command: cargo build\n    env:\n      RUST_LOG: warn\n") (evil-indent-plus-a-indent :raw (20 75 line) :expanded (20 76 line) :text "    command: cargo build\n    env:\n      RUST_LOG: warn\n\n") (evil-indent-plus-i-indent-up :raw (11 74 line) :expanded (11 75 line) :text "  build:\n    command: cargo build\n    env:\n      RUST_LOG: warn\n") (evil-indent-plus-a-indent-up :raw (11 75 line) :expanded (11 76 line) :text "  build:\n    command: cargo build\n    env:\n      RUST_LOG: warn\n\n") (evil-indent-plus-i-indent-up-down :raw (11 83 line) :expanded (11 84 line) :text "  build:\n    command: cargo build\n    env:\n      RUST_LOG: warn\n\n  test:\n") (evil-indent-plus-a-indent-up-down :raw (11 83 line) :expanded (11 84 line) :text "  build:\n    command: cargo build\n    env:\n      RUST_LOG: warn\n\n  test:\n"))"#
    ]];
    ParityBatchCase::value(
        "six_text_objects_select_exact_nested_workflow_boundaries",
        elisp_form,
        expected,
    )
}

fn inner_indent_delete_removes_a_nested_body_and_undo_restores_it() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-indent-plus-test-with-buffer
 "def deploy():\n    if ready:\n        validate()\n        publish()\n\n    notify()\ncleanup()\n"
 "validate"
 (buffer-enable-undo)
 (setq buffer-undo-list nil
       kill-ring nil)
 (execute-kbd-macro (kbd "d i i"))
 (let ((deleted (neomacs-evil-indent-plus-test-state)))
   (execute-kbd-macro (kbd "u"))
   (list :deleted deleted
         :undone (neomacs-evil-indent-plus-test-state))))
"####;
    let expected = expect![[
        r#"OK (:deleted (:text "def deploy():\n    if ready:\n\n\n    notify()\ncleanup()\n" :point 29 :line 3 :column 0 :state normal :kill #("        validate()\n        publish()\n" 0 37 (yank-handler #1=(evil-yank-line-handler nil t))) :register-type #2=(evil-yank-line-handler nil t)) :undone (:text "def deploy():\n    if ready:\n        validate()\n        publish()\n\n    notify()\ncleanup()\n" :point 37 :line 3 :column 8 :state normal :kill #("        validate()\n        publish()\n" 0 37 (yank-handler #1#)) :register-type #2#))"#
    ]];
    ParityBatchCase::value(
        "inner_indent_delete_removes_a_nested_body_and_undo_restores_it",
        elisp_form,
        expected,
    )
}

fn parent_context_change_replaces_a_complete_configuration_branch() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-indent-plus-test-with-buffer
 "jobs:\n  build:\n    image: rust\n    steps:\n      - cargo build\n      - cargo nextest run\n  deploy:\n    image: alpine\n"
 "cargo build"
 (execute-kbd-macro
  (vconcat (kbd "c i I")
           "checks:\n      - cargo fmt --check\n      - cargo nextest run"
           [escape]))
 (neomacs-evil-indent-plus-test-state))
"####;
    let expected = expect![[
        r#"OK (:text "jobs:\n  build:\n    image: rust\n    checks:\n      - cargo fmt --check\n      - cargo nextest run\n\n  deploy:\n    image: alpine\n" :point 94 :line 6 :column 24 :state normal :kill #("    steps:\n      - cargo build\n      - cargo nextest run\n" 0 57 (yank-handler (evil-yank-line-handler nil t))) :register-type (evil-yank-line-handler nil t))"#
    ]];
    ParityBatchCase::value(
        "parent_context_change_replaces_a_complete_configuration_branch",
        elisp_form,
        expected,
    )
}

fn parent_object_nests_a_section_under_the_preceding_key_and_undoes_once() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-indent-plus-test-with-buffer
 "release:\n  stages:\n  prepare:\n    lint\n    compile\n  publish:\n    upload\narchive:\n  retain\n"
 "compile"
 (buffer-enable-undo)
 (setq buffer-undo-list nil
       evil-shift-width 2)
 (execute-kbd-macro (kbd "> i I"))
 (let ((shifted (neomacs-evil-indent-plus-test-state)))
   (execute-kbd-macro (kbd "u"))
   (list :shifted shifted
         :undone (neomacs-evil-indent-plus-test-state))))
"####;
    let expected = expect![[
        r#"OK (:shifted (:text "release:\n  stages:\n    prepare:\n      lint\n      compile\n  publish:\n    upload\narchive:\n  retain\n" :point 48 :line 5 :column 4 :state normal :kill nil :register-type nil) :undone (:text "release:\n  stages:\n  prepare:\n    lint\n    compile\n  publish:\n    upload\narchive:\n  retain\n" :point 44 :line 5 :column 4 :state normal :kill nil :register-type nil))"#
    ]];
    ParityBatchCase::value(
        "parent_object_nests_a_section_under_the_preceding_key_and_undoes_once",
        elisp_form,
        expected,
    )
}

fn outer_indent_delete_consumes_separator_whitespace_not_the_next_block() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-indent-plus-test-with-buffer
 "server:\n  host: api.internal\n  port: 443\n\n\nlogging:\n  level: info\n\nmetrics:\n  enabled: true\n"
 "host"
 (let ((kill-ring nil))
   (execute-kbd-macro (kbd "d a i"))
   (neomacs-evil-indent-plus-test-state)))
"####;
    let expected = expect![[
        r#"OK (:text "server:\n\nlogging:\n  level: info\n\nmetrics:\n  enabled: true\n" :point 9 :line 2 :column 0 :state normal :kill #("  host: api.internal\n  port: 443\n\n" 0 34 (yank-handler (evil-yank-line-handler nil t))) :register-type (evil-yank-line-handler nil t))"#
    ]];
    ParityBatchCase::value(
        "outer_indent_delete_consumes_separator_whitespace_not_the_next_block",
        elisp_form,
        expected,
    )
}

fn narrowed_tab_indented_block_stays_inside_accessible_boundaries() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-indent-plus-test-with-buffer
 "outside-before\nsection:\n\tchild:\n\t\tfirst\n\t\tsecond\n\n\tsibling\noutside-after\n"
 "first"
 (let* ((start (save-excursion
                 (goto-char (point-min))
                 (forward-line 1)
                 (point)))
        (end (save-excursion
               (goto-char (point-max))
               (forward-line -1)
               (point)))
        restricted)
   (save-restriction
     (narrow-to-region start end)
     (setq restricted
           (list :bounds (list (point-min) (point-max))
                 :tab-width tab-width
                 :indent (current-indentation)
                 :inner
                 (neomacs-evil-indent-plus-test-range
                  #'evil-indent-plus-i-indent)
                 :up-down
                 (neomacs-evil-indent-plus-test-range
                  #'evil-indent-plus-a-indent-up-down))))
   (list :restricted restricted
         :whole (buffer-substring-no-properties (point-min) (point-max)))))
"####;
    let expected = expect![[
        r#"OK (:restricted (:bounds (16 60) :tab-width 8 :indent 16 :inner (evil-indent-plus-i-indent :raw (33 49 line) :expanded (33 50 line) :text "\11\11first\n\11\11second\n") :up-down (evil-indent-plus-a-indent-up-down :raw (25 59 line) :expanded (25 60 line) :text "\11child:\n\11\11first\n\11\11second\n\n\11sibling\n")) :whole "outside-before\nsection:\n\11child:\n\11\11first\n\11\11second\n\n\11sibling\noutside-after\n")"#
    ]];
    ParityBatchCase::value(
        "narrowed_tab_indented_block_stays_inside_accessible_boundaries",
        elisp_form,
        expected,
    )
}

fn evil_indent_plus_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_INDENT_PLUS_MELPA_PIN, "evil-indent-plus.el")
        .expect("prepare pinned Evil Indent Plus source below ./tmp")
        .with_melpa_dependency(EVIL_MELPA_PIN)
        .expect("prepare pinned Evil dependency")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn evil_indent_plus_practical_workflows_batch() {
    let cases = vec![
        six_text_objects_select_exact_nested_workflow_boundaries(),
        inner_indent_delete_removes_a_nested_body_and_undo_restores_it(),
        parent_context_change_replaces_a_complete_configuration_branch(),
        parent_object_nests_a_section_under_the_preceding_key_and_undoes_once(),
        outer_indent_delete_consumes_separator_whitespace_not_the_next_block(),
        narrowed_tab_indented_block_stays_inside_accessible_boundaries(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("evil-indent-plus parity batch");
    assert_oracle_batch_cases(
        evil_indent_plus_oracle(),
        test_name,
        "evil-indent-plus parity",
        &cases,
    );
}
