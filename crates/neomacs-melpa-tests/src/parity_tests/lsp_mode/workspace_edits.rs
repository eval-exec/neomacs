use expect_test::expect;

use super::ParityBatchCase;

fn rename_response_applies_ordered_unicode_edits_as_one_undoable_transaction() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "fn greet(name) {\n  return \"Hello, 😀 \" + name;\n}\n")
  (setq buffer-undo-list nil)
  (lsp--set-position-encoding "utf-16")
  (let ((events nil)
        (original (buffer-string))
        (edits
         (list
          (lsp-make-text-edit
           :range (neomacs-lsp-test-range 0 0 0 0)
           :new-text "// generated\n")
          (lsp-make-text-edit
           :range (neomacs-lsp-test-range 0 0 0 0)
           :new-text "// reviewed\n")
          (lsp-make-text-edit
           :range (neomacs-lsp-test-range 0 3 0 8)
           :new-text "welcome")
          (lsp-make-text-edit
           :range (neomacs-lsp-test-range 1 10 1 20)
           :new-text "Welcome back, 🌍 "))))
    (add-hook 'lsp-before-apply-edits-hook
              (lambda () (push :before events)) nil t)
    (add-hook 'lsp-after-apply-edits-hook
              (lambda (operation)
                (push (list :after operation (buffer-string)) events))
              nil t)
    (lsp--apply-text-edits edits 'rename)
    (undo-boundary)
    (let ((after (buffer-string))
          (after-point (point)))
      (undo-only 1)
      (list
       :after after
       :after-point after-point
       :events (nreverse events)
       :undo-restored (equal original (buffer-string))
       :undo-buffer (buffer-string)
       :undo-point (point)))))
"##;
    let expected = expect![[
        r##"OK (:after "// generated\n// reviewed\nfn welcome(name) {\n  return \"Welcome back, 🌍 \" + name;\n}\n" :after-point 83 :events (:before (:after rename "fn greet(name) {\n  return \"Welcome back, 🌍 \" + name;\n}\n") (:after rename "fn welcome(name) {\n  return \"Welcome back, 🌍 \" + name;\n}\n") (:after rename "// reviewed\nfn welcome(name) {\n  return \"Welcome back, 🌍 \" + name;\n}\n") (:after rename "// generated\n// reviewed\nfn welcome(name) {\n  return \"Welcome back, 🌍 \" + name;\n}\n")) :undo-restored t :undo-buffer "fn greet(name) {\n  return \"Hello, 😀 \" + name;\n}\n" :undo-point 49)"##
    ]];
    ParityBatchCase::value(
        "rename_response_applies_ordered_unicode_edits_as_one_undoable_transaction",
        elisp_form,
        expected,
    )
}

pub(super) fn workspace_edits_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![rename_response_applies_ordered_unicode_edits_as_one_undoable_transaction()]
}
