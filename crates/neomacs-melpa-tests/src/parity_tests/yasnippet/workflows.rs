use expect_test::expect;

use super::ParityBatchCase;

fn authoring_a_command_replaces_fields_updates_mirrors_and_navigates_both_ways() -> ParityBatchCase
{
    ParityBatchCase::value(
        "authoring_a_command_replaces_fields_updates_mirrors_and_navigates_both_ways",
        r##"
(progn
  (yas-define-snippets
   'emacs-lisp-mode
   (list
    (list
     "ncmd"
     (concat
      "(defun ${1:project-open-file} (${2:path})\n"
      "  \"${1:$(capitalize (replace-regexp-in-string \"-\" \" \" yas-text))}.\"\n"
      "  (interactive \"FFile: \")\n"
      "  ${3:(when (file-exists-p $2)\n"
      "    (find-file $2))})\n"
      "$0")
     "Define an interactive file command")))
  (with-temp-buffer
    (save-window-excursion
      (switch-to-buffer (current-buffer))
      (emacs-lisp-mode)
      (yas-minor-mode 1)
      (cl-labels
          ((state
            ()
            (let ((active (yas-active-snippets 'all)))
              (list
               :buffer (buffer-substring-no-properties
                        (point-min) (point-max))
               :point (point)
               :active (length active)))))
        (insert "ncmd")
        (let ((expanded (yas-expand))
              stages)
          (push (state) stages)
          (execute-kbd-macro "workspace-open-file")
          (push (state) stages)
          (yas-next-field-or-maybe-expand)
          (execute-kbd-macro "file-name")
          (push (state) stages)
          (yas-next-field-or-maybe-expand)
          (push (state) stages)
          (yas-prev-field)
          (push (state) stages)
          (yas-next-field-or-maybe-expand)
          (yas-next-field-or-maybe-expand)
          (yas-exit-all-snippets)
          (list
           :expanded expanded
           :stages (nreverse stages)
           :final (state)))))))
"##,
        expect![[
            r#"OK (:expanded t :stages ((:buffer "(defun project-open-file (path)\n  \"Project Open File.\"\n  (interactive \"FFile: \")\n  (when (file-exists-p path)\n    (find-file path)))\n" :point 8 :active 1) (:buffer "(defun workspace-open-file (path)\n  \"Workspace Open File.\"\n  (interactive \"FFile: \")\n  (when (file-exists-p path)\n    (find-file path)))\n" :point 27 :active 1) (:buffer "(defun workspace-open-file (file-name)\n  \"Workspace Open File.\"\n  (interactive \"FFile: \")\n  (when (file-exists-p file-name)\n    (find-file file-name)))\n" :point 38 :active 1) (:buffer "(defun workspace-open-file (file-name)\n  \"Workspace Open File.\"\n  (interactive \"FFile: \")\n  (when (file-exists-p file-name)\n    (find-file file-name)))\n" :point 93 :active 1) (:buffer "(defun workspace-open-file (file-name)\n  \"Workspace Open File.\"\n  (interactive \"FFile: \")\n  (when (file-exists-p file-name)\n    (find-file file-name)))\n" :point 29 :active 1)) :final (:buffer "(defun workspace-open-file (file-name)\n  \"Workspace Open File.\"\n  (interactive \"FFile: \")\n  (when (file-exists-p file-name)\n    (find-file file-name)))\n" :point 153 :active 0))"#
        ]],
    )
}

fn expanding_a_guard_inside_a_function_body_keeps_parent_and_child_fields_coherent()
-> ParityBatchCase {
    ParityBatchCase::value(
        "expanding_a_guard_inside_a_function_body_keeps_parent_and_child_fields_coherent",
        r##"
(progn
  (yas-define-snippets
   'emacs-lisp-mode
   (list
    (list
     "nroute"
     (concat
      "(defun ${1:handle-request} (${2:request})\n"
      "  ${3:body})\n"
      "$0")
     "Define a request handler")
    (list
     "nguard"
     (concat
      "(if (${1:request-authorized-p} ${2:request})\n"
      "    ${3:(render-json $2)}\n"
      "  ${4:(render-error $2)})$0")
     "Authorize and render a request")))
  (with-temp-buffer
    (save-window-excursion
      (switch-to-buffer (current-buffer))
      (emacs-lisp-mode)
      (yas-minor-mode 1)
      (let ((yas-triggers-in-field t))
        (cl-labels
            ((state
              ()
              (list
               :buffer (buffer-substring-no-properties
                        (point-min) (point-max))
               :point (point)
               :active (length (yas-active-snippets 'all)))))
          (insert "nroute")
          (let ((outer-expanded (yas-expand)))
            (execute-kbd-macro "handle-api-request")
            (yas-next-field-or-maybe-expand)
            (execute-kbd-macro "request")
            (yas-next-field-or-maybe-expand)
            (let ((outer-ready (state)))
              (execute-kbd-macro "nguard")
              (let ((inner-expanded
                     (yas-next-field-or-maybe-expand))
                    stages)
                (push (state) stages)
                (execute-kbd-macro "request-authorized-p")
                (yas-next-field-or-maybe-expand)
                (execute-kbd-macro "context")
                (push (state) stages)
                (yas-next-field-or-maybe-expand)
                (push (state) stages)
                (yas-next-field-or-maybe-expand)
                (push (state) stages)
                (yas-exit-all-snippets)
                (list
                 :outer-expanded outer-expanded
                 :outer-ready outer-ready
                 :inner-expanded inner-expanded
                 :stages (nreverse stages)
                 :final (state))))))))))
"##,
        expect![[
            r#"OK (:outer-expanded t :outer-ready (:buffer "(defun handle-api-request (request)\n  body)\n" :point 39 :active 1) :inner-expanded nil :stages ((:buffer "(defun handle-api-request (request)\n  (if (request-authorized-p request)\n      (render-json request)\n    (render-error request)))\n" :point 44 :active 2) (:buffer "(defun handle-api-request (request)\n  (if (request-authorized-p context)\n      (render-json context)\n    (render-error context)))\n" :point 72 :active 2) (:buffer "(defun handle-api-request (request)\n  (if (request-authorized-p context)\n      (render-json context)\n    (render-error context)))\n" :point 80 :active 2) (:buffer "(defun handle-api-request (request)\n  (if (request-authorized-p context)\n      (render-json context)\n    (render-error context)))\n" :point 106 :active 2)) :final (:buffer "(defun handle-api-request (request)\n  (if (request-authorized-p context)\n      (render-json context)\n    (render-error context)))\n" :point 131 :active 0))"#
        ]],
    )
}

fn undo_and_redo_restore_an_edited_field_and_its_transformed_mirror() -> ParityBatchCase {
    ParityBatchCase::value(
        "undo_and_redo_restore_an_edited_field_and_its_transformed_mirror",
        r##"
(progn
  (yas-define-snippets
   'emacs-lisp-mode
   (list
    (list
     "ncache"
     (concat
      "(let* ((${1:cache-key} ${2:(compute-value)}))\n"
      "  (puthash \"${1:$(upcase (replace-regexp-in-string \"-\" \"_\" yas-text))}\" $2 cache))\n"
      "$0")
     "Cache a computed value")))
  (with-temp-buffer
    (save-window-excursion
      (switch-to-buffer (current-buffer))
      (emacs-lisp-mode)
      (yas-minor-mode 1)
      (cl-labels
          ((state
            ()
            (list
             :buffer (buffer-substring-no-properties
                      (point-min) (point-max))
             :point (point)
             :active (length (yas-active-snippets 'all)))))
        (insert "ncache")
        (let ((expanded (yas-expand)))
          (setq buffer-undo-list nil)
          (execute-kbd-macro "session-token")
          (undo-boundary)
          (let ((after-edit (state)))
            (execute-kbd-macro (kbd "C-/"))
            (let ((after-undo (state)))
              (undo-redo 1)
              (let ((after-redo (state)))
                (yas-next-field-or-maybe-expand)
                (execute-kbd-macro "(load-token)")
                (yas-next-field-or-maybe-expand)
                (yas-exit-all-snippets)
                (list
                 :expanded expanded
                 :after-edit after-edit
                 :after-undo after-undo
                 :after-redo after-redo
                 :final (state))))))))))
"##,
        expect![[
            r#"OK (:expanded t :after-edit (:buffer "(let* ((session-token (compute-value)))\n  (puthash \"SESSION_TOKEN\" (compute-value) cache))\n" :point 22 :active 1) :after-undo (:buffer "(let* ((cache-key (compute-value)))\n  (puthash \"CACHE_KEY\" (compute-value) cache))\n" :point 9 :active 1) :after-redo (:buffer "(let* ((session-token (compute-value)))\n  (puthash \"SESSION_TOKEN\" (compute-value) cache))\n" :point 66 :active 1) :final (:buffer "(let* ((session-token (load-token)))\n  (puthash \"SESSION_TOKEN\" (load-token) cache))\n" :point 86 :active 0))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        authoring_a_command_replaces_fields_updates_mirrors_and_navigates_both_ways(),
        expanding_a_guard_inside_a_function_body_keeps_parent_and_child_fields_coherent(),
        undo_and_redo_restore_an_edited_field_and_its_transformed_mirror(),
    ]
}
