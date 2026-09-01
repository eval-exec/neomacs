use expect_test::expect;

use super::ParityBatchCase;

fn action_index_resolves_key_function_name_and_position() -> ParityBatchCase {
    ParityBatchCase::value(
        "action_index_resolves_key_function_name_and_position",
        r####"
(let ((actions
       '(1
         ("o" identity "open")
         ("j" ignore "jump")
         ("x" numberp "extra"))))
  (list :by-key (counsel-projectile--action-index "j" actions)
        :by-fun (counsel-projectile--action-index #'numberp actions)
        :by-name (counsel-projectile--action-index "open" actions)
        :by-pos (counsel-projectile--action-index 2 actions)
        :missing
        (condition-case err
            (progn (counsel-projectile--action-index "nope" actions) :ok)
          (error (error-message-string err)))))
"####,
        expect![[
            r#"OK (:by-key 2 :by-fun 3 :by-name 1 :by-pos 2 :missing "Action not found: nope")"#
        ]],
    )
}

fn modify_action_add_remove_setname_setkey_and_default() -> ParityBatchCase {
    ParityBatchCase::value(
        "modify_action_add_remove_setname_setkey_and_default",
        r####"
(setq neomacs-counsel-projectile-test-actions
      '(1
        ("o" identity "open")
        ("j" ignore "jump")
        ("x" numberp "extra")))
(counsel-projectile-modify-action
 'neomacs-counsel-projectile-test-actions
 '((remove "j")
   (add ("z" zerop "zero") "x")
   (default "x")
   (setname "o" "open-file")
   (setkey "x" "e")))
(list :default-index (car neomacs-counsel-projectile-test-actions)
      :keys (mapcar #'car (cdr neomacs-counsel-projectile-test-actions))
      :names (mapcar #'cl-caddr (cdr neomacs-counsel-projectile-test-actions))
      :actions (mapcar (lambda (a)
                         (list (car a) (cadr a) (cl-caddr a)))
                       (cdr neomacs-counsel-projectile-test-actions)))
"####,
        expect![[
            r#"OK (:default-index 3 :keys ("o" "z" "e") :names ("open-file" "zero" "extra") :actions (("o" identity "open-file") ("z" zerop "zero") ("e" numberp "extra")))"#
        ]],
    )
}

fn find_file_matcher_basename_filters_paths() -> ParityBatchCase {
    ParityBatchCase::value(
        "find_file_matcher_basename_filters_paths",
        r####"
(let ((candidates
       '("src/alpha.el" "src/beta.el" "test/alpha_test.el" "readme.md"))
      (ivy-text "alpha")
      (ivy-use-ignore nil)
      (counsel-find-file-ignore-regexp nil))
  (list :alpha
        (counsel-projectile-find-file-matcher-basename "alpha" candidates)
        :el
        (let ((ivy-text "el"))
          (counsel-projectile-find-file-matcher-basename "\\.el\\'" candidates))
        :none
        (let ((ivy-text "zzz"))
          (counsel-projectile-find-file-matcher-basename "zzz" candidates))))
"####,
        expect![[
            r#"OK (:alpha ("src/alpha.el" "test/alpha_test.el") :el ("src/alpha.el" "src/beta.el" "test/alpha_test.el") :none nil)"#
        ]],
    )
}

fn project_buffers_list_respects_remove_current_setting() -> ParityBatchCase {
    ParityBatchCase::value(
        "project_buffers_list_respects_remove_current_setting",
        r####"
(let* ((a (get-buffer-create "cp-a"))
       (b (get-buffer-create "cp-b")))
  (unwind-protect
      (cl-letf (((symbol-function 'projectile-project-buffer-names)
                 (lambda () (list "cp-a" "cp-b")))
                ((symbol-function 'ivy--buffer-list)
                 (lambda (_str _pred matcher)
                   (funcall matcher
                            (mapcar (lambda (n) (list n))
                                    (list "cp-a" "cp-b" "other"))))))
        (with-current-buffer a
          (let ((counsel-projectile-remove-current-buffer t))
            (list :with-remove
                  (counsel-projectile--project-buffers)
                  :without-remove
                  (let ((counsel-projectile-remove-current-buffer nil))
                    (sort (copy-sequence
                           (counsel-projectile--project-buffers))
                          #'string-lessp))))))
    (let ((kill-buffer-hook nil)
          (kill-buffer-query-functions nil))
      (when (buffer-live-p a) (kill-buffer a))
      (when (buffer-live-p b) (kill-buffer b)))))
"####,
        expect!["OK (:with-remove nil :without-remove nil)"],
    )
}

fn find_file_transformer_marks_non_visited_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "find_file_transformer_marks_non_visited_files",
        r####"
(cl-letf (((symbol-function 'projectile-expand-root)
           (lambda (str) (concat "/proj/" str)))
          ((symbol-function 'get-file-buffer)
           (lambda (path)
             (and (string-suffix-p "real.el" path)
                  (get-buffer-create " *visited*")))))
  (let ((normal (counsel-projectile-find-file-transformer "src/real.el"))
        (virtual (counsel-projectile-find-file-transformer "src/virtual.el")))
    (list :normal-text (substring-no-properties normal)
          :normal-face (get-text-property 0 'face normal)
          :virtual-text (substring-no-properties virtual)
          :virtual-face (get-text-property 0 'face virtual))))
"####,
        expect![[
            r#"OK (:normal-text "src/real.el" :normal-face nil :virtual-text "src/virtual.el" :virtual-face ivy-virtual)"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        action_index_resolves_key_function_name_and_position(),
        modify_action_add_remove_setname_setkey_and_default(),
        find_file_matcher_basename_filters_paths(),
        project_buffers_list_respects_remove_current_setting(),
        find_file_transformer_marks_non_visited_files(),
    ]
}
