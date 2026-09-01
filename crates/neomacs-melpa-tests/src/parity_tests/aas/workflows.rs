use expect_test::expect;

use super::ParityBatchCase;

fn aas_expands_configured_snippets_while_the_user_types_prose() -> ParityBatchCase {
    ParityBatchCase::value(
        "aas_expands_configured_snippets_while_the_user_types_prose",
        r##"(aas-test-with-live-buffer
 (text-mode)
 (aas-set-snippets 'aas-workflow-mode
   "inf" "∞"
   ";a" "α"
   "#+lh" "#+latex_header: "
   "1/2" "½")
 (aas-mode +1)
 (aas-activate-keymap 'aas-workflow-mode)
 (execute-kbd-macro
  (kbd "t h e SPC s e t SPC i s SPC i n f SPC a n d SPC ; a SPC = SPC 1 / 2 RET # + l h a m s m a t h"))
 (list
  (buffer-string)
  (point)
  (line-number-at-pos)
  aas-mode
  aas-active-keymaps
  (and (memq #'aas-post-self-insert-hook post-self-insert-hook) t)
  (local-variable-p 'post-self-insert-hook)
  (hash-table-count aas-keymaps)
  (keymapp (gethash 'aas-workflow-mode aas-keymaps))))"##,
        expect![[
            r#"OK ("the set is ∞ and α = ½\n#+latex_header: amsmath" 47 2 t (aas-workflow-mode) t t 1 t)"#
        ]],
    )
}

fn aas_conditions_gate_expansion_and_a_nil_condition_clears_the_previous_one() -> ParityBatchCase {
    ParityBatchCase::value(
        "aas_conditions_gate_expansion_and_a_nil_condition_clears_the_previous_one",
        r##"(aas-test-with-live-buffer
 (text-mode)
 (aas-set-snippets 'aas-condition-mode
   :cond #'bolp
   "#+lh" "#+latex_header: "
   :cond nil
   "inf" "∞")
 (aas-mode +1)
 (aas-activate-keymap 'aas-condition-mode)
 (execute-kbd-macro (kbd "# + l h x RET p a d SPC # + l h SPC i n f"))
 (list
  (buffer-string)
  (point)
  (line-number-at-pos)))"##,
        expect![[r##"OK ("#+latex_header: x\npad #+lh ∞" 29 2)"##]],
    )
}

fn aas_function_expansions_see_the_transient_variables_and_run_both_hooks() -> ParityBatchCase {
    ParityBatchCase::value(
        "aas_function_expansions_see_the_transient_variables_and_run_both_hooks",
        r##"(aas-test-with-live-buffer
 (text-mode)
 (let (events)
   (aas-set-snippets 'aas-function-mode
     :cond (lambda () (push (list 'cond (point) (bolp)) events) 'checked)
     "sig"
     (lambda ()
       (interactive)
       (push (list 'expand
                   aas-transient-snippet-key
                   aas-transient-snippet-condition-result
                   (functionp aas-transient-snippet-expansion))
             events)
       (insert "-- Signed, " user-full-name)))
   (setq-local aas-pre-snippet-expand-hook
               (list (lambda () (push (list 'pre aas-transient-snippet-key (point)) events))))
   (setq-local aas-post-snippet-expand-hook
               (list (lambda () (push (list 'post aas-transient-snippet-key (point)) events))))
   (aas-mode +1)
   (aas-activate-keymap 'aas-function-mode)
   (execute-kbd-macro (kbd "b y e RET s i g"))
   (list
    (buffer-string)
    (point)
    (nreverse events)
    aas-transient-snippet-key
    aas-transient-snippet-expansion
    aas-transient-snippet-condition-result)))"##,
        expect![[
            r#"OK ("bye\n-- Signed, " 16 ((cond 5 t) (pre "sig" 5) (expand "sig" checked t) (post "sig" 16)) nil nil nil)"#
        ]],
    )
}

fn aas_walks_multi_key_prefixes_and_leaves_dead_ends_and_split_keys_alone() -> ParityBatchCase {
    ParityBatchCase::value(
        "aas_walks_multi_key_prefixes_and_leaves_dead_ends_and_split_keys_alone",
        r##"(aas-test-with-live-buffer
 (text-mode)
 (aas-set-snippets 'aas-tree-mode
   ";a" "α"
   ";;a" "\\alpha"
   ";;b" "\\beta")
 (aas-mode +1)
 (aas-activate-keymap 'aas-tree-mode)
 (execute-kbd-macro (kbd "; a SPC ; ; a SPC ; ; b SPC ; z"))
 (let ((typed (buffer-string)))
   (erase-buffer)
   (insert ";")
   (goto-char (point-min))
   (execute-kbd-macro (kbd ";"))
   (let ((split (buffer-string)))
     (erase-buffer)
     (insert ";a")
     (goto-char (point-min))
     (execute-kbd-macro (kbd ";"))
     (list typed
           split
           (buffer-string)
           (point)
           aas-global-condition-hook))))"##,
        expect![[r#"OK ("α \\alpha \\beta ;z" ";;" ";;a" 2 (aas--key-is-fully-typed?))"#]],
    )
}

fn aas_activation_lifecycle_decides_which_snippets_are_live_in_the_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "aas_activation_lifecycle_decides_which_snippets_are_live_in_the_buffer",
        r##"(aas-test-with-live-buffer
 (aas-set-snippets 'text-mode "tm" "TEXT ")
 (aas-set-snippets 'prog-mode "pm" "PROG ")
 (aas-set-snippets 'aas-extra-mode "xm" "EXTRA ")
 (text-mode)
 (aas-activate-for-major-mode)
 (execute-kbd-macro (kbd "t m p m x m"))
 (let ((only-major (buffer-string)))
   (erase-buffer)
   (aas-activate-keymap 'aas-extra-mode)
   (execute-kbd-macro (kbd "t m x m"))
   (let ((both (buffer-string))
         (keymaps (copy-sequence aas-active-keymaps)))
     (erase-buffer)
     (aas-deactivate-keymap 'text-mode)
     (execute-kbd-macro (kbd "t m x m"))
     (let ((deactivated (buffer-string)))
       (erase-buffer)
       (aas-set-snippets 'aas-extra-mode "xm" nil)
       (execute-kbd-macro (kbd "x m"))
       (let ((disabled (buffer-string)))
         (erase-buffer)
         (aas-set-snippets 'aas-extra-mode "xm" "EXTRA ")
         (aas-mode -1)
         (execute-kbd-macro (kbd "x m"))
         (list only-major
               both
               keymaps
               deactivated
               disabled
               (buffer-string)
               aas-active-keymaps
               aas-mode
               (aas--modes-to-activate 'text-mode)
               (and (memq #'aas-post-self-insert-hook post-self-insert-hook) t)))))))"##,
        expect![[
            r#"OK ("TEXT pmxm" "TEXT EXTRA " (aas-extra-mode text-mode) "tmEXTRA " "xm" "xm" (aas-extra-mode) nil (text-mode) nil)"#
        ]],
    )
}

fn aas_rejects_invalid_snippet_definitions_with_exact_errors() -> ParityBatchCase {
    ParityBatchCase::value(
        "aas_rejects_invalid_snippet_definitions_with_exact_errors",
        r##"(let (observed)
  (dolist (probe
           (list
            (cons 'number-expansion
                  (lambda () (aas-set-snippets 'aas-invalid-mode "k" 42)))
            (cons 'symbol-condition
                  (lambda ()
                    (aas-set-snippets 'aas-invalid-mode :cond 'not-a-function "k" "v")))
            (cons 'unknown-keyword
                  (lambda () (aas-set-snippets 'aas-invalid-mode :nope 1)))
            (cons 'prefix-clash
                  (lambda ()
                    (aas-set-snippets 'aas-clash-mode ";a" "α")
                    (aas-set-snippets 'aas-clash-mode ";ab" "β")))
            (cons 'unknown-keymap
                  (lambda () (aas-activate-keymap 'aas-never-defined-mode)))))
    (push (cons (car probe)
                (condition-case error
                    (list 'value (functionp (funcall (cdr probe))))
                  (error (list 'signal (car error) (cdr error)))))
          observed))
  (list (nreverse observed)
        (gethash 'aas-invalid-mode aas-keymaps)
        (and (gethash 'aas-clash-mode aas-keymaps) t)
        (gethash 'aas-never-defined-mode aas-keymaps)))"##,
        expect![[
            r#"OK (((number-expansion signal error ("Expansion must be either a string, function, tempel/yas form, or nil")) (symbol-condition signal error ("Condition must be either nil or a function")) (unknown-keyword signal error ("Unknown keyword: :nope")) (prefix-clash signal error ("Key sequence ; a b starts with non-prefix key ; a")) (unknown-keymap value nil)) nil t nil)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aas_expands_configured_snippets_while_the_user_types_prose(),
        aas_conditions_gate_expansion_and_a_nil_condition_clears_the_previous_one(),
        aas_function_expansions_see_the_transient_variables_and_run_both_hooks(),
        aas_walks_multi_key_prefixes_and_leaves_dead_ends_and_split_keys_alone(),
        aas_activation_lifecycle_decides_which_snippets_are_live_in_the_buffer(),
        aas_rejects_invalid_snippet_definitions_with_exact_errors(),
    ]
}
