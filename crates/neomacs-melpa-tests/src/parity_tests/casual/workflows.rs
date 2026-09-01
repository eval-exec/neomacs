use expect_test::expect;

use super::ParityBatchCase;

fn editkit_menu_copies_and_reorders_balanced_expressions() -> ParityBatchCase {
    ParityBatchCase::value(
        "editkit_menu_copies_and_reorders_balanced_expressions",
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(alpha 1) (beta 2) (gamma 3)")
  (let ((kill-ring nil)
        (kill-ring-yank-pointer nil)
        (copy-command
         (neomacs-casual-test-command 'casual-editkit-main-tmenu "c"))
        (transpose-command
         (neomacs-casual-test-command 'casual-editkit-main-tmenu "t")))
    (goto-char (point-min))
    (funcall copy-command)
    (let ((copied (current-kill 0 t)))
      (goto-char (point-min))
      (forward-sexp 1)
      (funcall transpose-command 1)
      (list :menu-commands (list copy-command transpose-command)
            :copied copied
            :edited (buffer-string)
            :next-sexp (thing-at-point 'sexp t)))))
"##,
        expect![[
            r##"OK (:menu-commands (casual-editkit-copy-sexp transpose-sexps) :copied "(alpha 1)" :edited "(beta 2) (alpha 1) (gamma 3)" :next-sexp "(alpha 1)")"##
        ]],
    )
}

fn elisp_menu_navigates_forms_and_evaluates_the_work_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "elisp_menu_navigates_forms_and_evaluates_the_work_buffer",
        r##"
(unwind-protect
    (with-temp-buffer
      (emacs-lisp-mode)
      (insert "(setq neomacs-casual-test-total 21)\n"
              "(setq neomacs-casual-test-total (* neomacs-casual-test-total 2))\n"
              "(list :total neomacs-casual-test-total)\n")
      (let ((next-command
             (neomacs-casual-test-command 'casual-elisp-tmenu "C-<right>"))
            (eval-command
             (neomacs-casual-test-command 'casual-elisp-tmenu "L")))
        (goto-char (point-min))
        (funcall next-command)
        (let ((second (thing-at-point 'sexp t)))
          (funcall next-command)
          (let ((third (thing-at-point 'sexp t)))
            (deactivate-mark)
            (funcall eval-command)
            (list :menu-commands (list next-command eval-command)
                  :second second
                  :third third
                  :total neomacs-casual-test-total)))))
  (makunbound 'neomacs-casual-test-total))
"##,
        expect![[
            r##"OK (:menu-commands (casual-elisp-next-sexp elisp-eval-region-or-buffer) :second "(setq neomacs-casual-test-total (* neomacs-casual-test-total 2))" :third "(list :total neomacs-casual-test-total)" :total 42)"##
        ]],
    )
}

fn csv_menu_sorts_inventory_numerically_and_transposes_the_table() -> ParityBatchCase {
    ParityBatchCase::value(
        "csv_menu_sorts_inventory_numerically_and_transposes_the_table",
        r##"
(with-temp-buffer
  (insert "name,qty,price\npear,2,3.50\napple,10,1.25\nplum,4,2.00\n")
  (csv-mode)
  (let ((sort-command
         (neomacs-casual-test-command 'casual-csv-tmenu "N"))
        (transpose-command
         (neomacs-casual-test-command 'casual-csv-tmenu "t")))
    (goto-char (point-min))
    (forward-line 1)
    (funcall sort-command 2 (point) (point-max))
    (let ((sorted (buffer-string)))
      (funcall transpose-command (point-min) (point-max))
      (let ((transposed (buffer-string)))
        (funcall transpose-command (point-min) (point-max))
        (list :menu-commands (list sort-command transpose-command)
              :sorted sorted
              :transposed transposed
              :round-trip (buffer-string))))))
"##,
        expect![[
            r##"OK (:menu-commands (csv-sort-numeric-fields csv-transpose) :sorted "name,qty,price\npear,2,3.50\nplum,4,2.00\napple,10,1.25\n" :transposed "name,pear,plum,apple\nqty,2,4,10\nprice,3.50,2.00,1.25\n" :round-trip "name,qty,price\npear,2,3.50\nplum,4,2.00\napple,10,1.25\n")"##
        ]],
    )
}

fn dired_menu_marks_files_and_copies_the_selected_names() -> ParityBatchCase {
    ParityBatchCase::value(
        "dired_menu_marks_files_and_copies_the_selected_names",
        r##"
(let* ((root (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (dir (expand-file-name "casual-dired" root))
       (alpha (expand-file-name "alpha.txt" dir))
       (beta (expand-file-name "beta notes.txt" dir))
       (kill-ring nil)
       (kill-ring-yank-pointer nil)
       (mark-command
        (neomacs-casual-test-command 'casual-dired-tmenu "m"))
       (unmark-command
        (neomacs-casual-test-command 'casual-dired-tmenu "u"))
       (copy-command
        (neomacs-casual-test-command 'casual-dired-tmenu "w"))
       dired-buffer)
  (make-directory dir t)
  (neomacs-casual-test-write-file alpha "first record\n")
  (neomacs-casual-test-write-file beta "second record\n")
  (unwind-protect
      (progn
        (setq dired-buffer (dired-noselect dir))
        (with-current-buffer dired-buffer
          (dired-goto-file alpha)
          (funcall mark-command 1)
          (dired-goto-file beta)
          (funcall mark-command 1)
          (funcall copy-command)
          (let ((copied (current-kill 0 t))
                (selected (mapcar #'file-name-nondirectory
                                  (dired-get-marked-files))))
            (dired-goto-file alpha)
            (funcall unmark-command 1)
            (list :menu-commands
                  (list mark-command copy-command unmark-command)
                  :copied copied
                  :selected selected
                  :after-unmark
                  (mapcar #'file-name-nondirectory
                          (dired-get-marked-files))))))
    (when (buffer-live-p dired-buffer)
      (kill-buffer dired-buffer))))
"##,
        expect![[
            r##"OK (:menu-commands (dired-mark dired-copy-filename-as-kill dired-unmark) :copied "alpha.txt \"beta notes.txt\"" :selected ("alpha.txt" "beta notes.txt") :after-unmark ("beta notes.txt"))"##
        ]],
    )
}

fn ibuffer_menu_marks_and_alphabetizes_a_working_set() -> ParityBatchCase {
    ParityBatchCase::value(
        "ibuffer_menu_marks_and_alphabetizes_a_working_set",
        r##"
(let ((fixture-names '("*casual-zeta*" "*casual-alpha*" "*casual-beta*"))
      (ibuffer-name "*Casual Ibuffer*")
      fixture-buffers ibuffer-buffer)
  (unwind-protect
      (progn
        (setq fixture-buffers
              (mapcar (lambda (name)
                        (let ((buffer (get-buffer-create name)))
                          (with-current-buffer buffer
                            (erase-buffer)
                            (insert (format "work item %s\n" name)))
                          buffer))
                      fixture-names))
        (setq ibuffer-buffer (get-buffer-create ibuffer-name))
        (with-current-buffer ibuffer-buffer
          (ibuffer-mode)
          (ibuffer-update nil t)
          (let ((mark-command
                 (neomacs-casual-test-command 'casual-ibuffer-tmenu "m"))
                (unmark-command
                 (neomacs-casual-test-command 'casual-ibuffer-tmenu "u"))
                (sort-command
                 (neomacs-casual-test-command
                  'casual-ibuffer-sortby-tmenu "a")))
            (ibuffer-jump-to-buffer "*casual-zeta*")
            (funcall mark-command nil nil 1)
            (ibuffer-jump-to-buffer "*casual-beta*")
            (funcall mark-command nil nil 1)
            (let ((marked (neomacs-casual-test-ibuffer-targets)))
              (funcall sort-command)
              (let ((sorted (neomacs-casual-test-ibuffer-targets)))
                (ibuffer-jump-to-buffer "*casual-beta*")
                (funcall unmark-command nil nil 1)
                (list :menu-commands
                      (list mark-command sort-command unmark-command)
                      :marked marked
                      :sorted sorted
                      :after-unmark
                      (neomacs-casual-test-ibuffer-targets)))))))
    (dolist (buffer (cons ibuffer-buffer fixture-buffers))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))
"##,
        expect![[
            r##"OK (:menu-commands (ibuffer-mark-forward ibuffer-do-sort-by-alphabetic ibuffer-unmark-forward) :marked (("*casual-zeta*" marked) ("*casual-alpha*" empty) ("*casual-beta*" marked)) :sorted (("*casual-alpha*" empty) ("*casual-beta*" marked) ("*casual-zeta*" marked)) :after-unmark (("*casual-alpha*" empty) ("*casual-beta*" empty) ("*casual-zeta*" marked)))"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        editkit_menu_copies_and_reorders_balanced_expressions(),
        elisp_menu_navigates_forms_and_evaluates_the_work_buffer(),
        csv_menu_sorts_inventory_numerically_and_transposes_the_table(),
        dired_menu_marks_files_and_copies_the_selected_names(),
        ibuffer_menu_marks_and_alphabetizes_a_working_set(),
    ]
}
