use expect_test::expect;

use super::ParityBatchCase;

/// The package's headline workflow: a user edits a real Emacs Lisp file whose
/// major mode supplies `elisp-completion-at-point', enables ac-capf with the
/// documented trigger key, types a symbol prefix, picks the second candidate
/// and completes.  Only `fboundp' symbols may appear because point sits in a
/// function-call position; the shorter `neomacs-ac-capf-fixture-annals'
/// variable would sort first if the predicate were dropped.
fn emacs_lisp_mode_capf_completes_a_typed_symbol_through_the_trigger_key() -> ParityBatchCase {
    ParityBatchCase::value(
        "emacs_lisp_mode_capf_completes_a_typed_symbol_through_the_trigger_key",
        r##"(progn
 (defun neomacs-ac-capf-fixture-annotate (entry) entry)
 (defun neomacs-ac-capf-fixture-annotation (entry) entry)
 (defun neomacs-ac-capf-fixture-anniversary (entry) entry)
 (defvar neomacs-ac-capf-fixture-annals nil)
 (ac-capf-test-with-live-buffer
  (ac-capf-test-visit
   "notes/session.el"
   ";;; session.el --- session notes  -*- lexical-binding: t; -*-\n(defun session-run (entry)\n  (neomacs-ac-capf-fixture-")
  (ac-set-trigger-key "TAB")
  (ac-capf-setup)
  (ac-capf-setup)
  (execute-kbd-macro (kbd "a n n TAB"))
  (let ((offered (list (ac-capf-test-session) (ac-capf-test-menu))))
    (execute-kbd-macro (kbd "M-n"))
    (let ((moved (ac-capf-test-session)))
      (execute-kbd-macro (kbd "RET"))
      (let ((completed (ac-capf-test-buffer-state))
            (session (ac-capf-test-session)))
        (let ((make-backup-files nil))
          (save-buffer))
        (list :offered offered
              :moved moved
              :after completed
              :session session
              :file (ac-capf-test-read "notes/session.el")))))))"##,
        expect![[
            r#"OK (:offered ((:prefix "neomacs-ac-capf-fixture-ann" :prefix-start 92 :common "neomacs-ac-capf-fixture-ann" :menu-live t :selected "neomacs-ac-capf-fixture-annotate" :completing t) (("neomacs-ac-capf-fixture-annotate" "s" nil nil (symbol "s")) ("neomacs-ac-capf-fixture-annotation" "s" nil nil (symbol "s")) ("neomacs-ac-capf-fixture-anniversary" "s" nil nil (symbol "s")))) :moved (:prefix "neomacs-ac-capf-fixture-ann" :prefix-start 92 :common "neomacs-ac-capf-fixture-ann" :menu-live t :selected "neomacs-ac-capf-fixture-annotation" :completing t) :after (:text ";;; session.el --- session notes  -*- lexical-binding: t; -*-\n(defun session-run (entry)\n  (neomacs-ac-capf-fixture-annotation" :point 126 :mode emacs-lisp-mode :auto-complete t :sources (ac-source-capf) :capfs (elisp-completion-at-point t)) :session (:prefix nil :prefix-start nil :common nil :menu-live nil :selected nil :completing nil) :file ";;; session.el --- session notes  -*- lexical-binding: t; -*-\n(defun session-run (entry)\n  (neomacs-ac-capf-fixture-annotation\n")"#
        ]],
    )
}

fn a_custom_capf_loses_its_annotation_document_and_exit_metadata_through_ac_capf() -> ParityBatchCase
{
    ParityBatchCase::value(
        "a_custom_capf_loses_its_annotation_document_and_exit_metadata_through_ac_capf",
        r##"(progn
 (defvar ac-capf-test-exits nil)
 (defvar ac-capf-test-annotations nil)
 (defvar ac-capf-test-docs nil)
 (defvar ac-source-project-glossary
   '((candidates . (lambda () (list "narrator" "naïve")))
     (symbol . "g")))
 (defun ac-capf-test-glossary-capf ()
   (let ((bounds (bounds-of-thing-at-point 'symbol)))
     (when bounds
       (list (car bounds)
             (cdr bounds)
             (list "naïveté" "naïve" (propertize "narrative" 'face 'bold) "naïve")
             :annotation-function
             (lambda (candidate)
               (push candidate ac-capf-test-annotations)
               (format " <%s>" candidate))
             :company-doc-buffer
             (lambda (candidate)
               (push candidate ac-capf-test-docs)
               (get-buffer-create "*ac-capf-glossary-doc*"))
             :exit-function
             (lambda (candidate status)
               (push (list candidate status) ac-capf-test-exits))))))
 (ac-capf-test-with-live-buffer
  (ac-capf-test-scratch #'text-mode "The reviewer called it na")
  (setq-local ac-sources (list 'ac-source-project-glossary))
  (add-hook 'completion-at-point-functions #'ac-capf-test-glossary-capf nil t)
  (ac-capf-setup)
  (auto-complete)
  (let ((offered (list (ac-capf-test-session) (ac-capf-test-menu))))
    (ac-complete)
    (let ((after (ac-capf-test-buffer-state))
          (ac-exits ac-capf-test-exits)
          (ac-annotations ac-capf-test-annotations)
          (ac-docs ac-capf-test-docs)
          (ac-doc-buffer (get-buffer "*ac-capf-glossary-doc*")))
      (goto-char (point-max))
      (insert " and nar")
      (let ((standard (completion-at-point)))
        (list :offered offered
              :after after
              :ac-exits ac-exits
              :ac-annotations ac-annotations
              :ac-docs ac-docs
              :ac-doc-buffer ac-doc-buffer
              :standard standard
              :standard-text (buffer-substring-no-properties (point-min) (point-max))
              :standard-point (- (point) (point-min))
              :standard-exits ac-capf-test-exits))))))"##,
        expect![[
            r#"OK (:offered ((:prefix "na" :prefix-start 23 :common "na" :menu-live t :selected "naïve" :completing t) (("naïve" "s" nil nil (symbol "s")) ("naïveté" "s" nil nil (symbol "s")) ("narrator" "g" nil nil (symbol "g")) ("narrative" "s" nil nil (symbol "s")))) :after (:text "The reviewer called it naïve" :point 28 :mode text-mode :auto-complete t :sources (ac-source-capf ac-source-project-glossary) :capfs (ac-capf-test-glossary-capf t ispell-completion-at-point)) :ac-exits nil :ac-annotations nil :ac-docs nil :ac-doc-buffer nil :standard t :standard-text "The reviewer called it naïve and narrative" :standard-point 42 :standard-exits ((#("narrative" 0 9 (face bold)) finished)))"#
        ]],
    )
}

fn a_capf_whose_collection_is_a_function_drives_the_auto_complete_menu() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_capf_whose_collection_is_a_function_drives_the_auto_complete_menu",
        r##"(progn
 (defvar ac-capf-test-table-calls nil)
 (defun ac-capf-test-branch-table (string predicate action)
   (push (list string action) ac-capf-test-table-calls)
   (complete-with-action action
                         '("checkout-branch" "check-status" "cherry-pick" "checkout")
                         string
                         predicate))
 (defun ac-capf-test-branch-capf ()
   (let ((bounds (bounds-of-thing-at-point 'symbol)))
     (when bounds
       (list (car bounds)
             (cdr bounds)
             #'ac-capf-test-branch-table
             :predicate (lambda (candidate) (not (equal candidate "checkout")))))))
 (ac-capf-test-with-live-buffer
  (ac-capf-test-scratch #'text-mode "git che")
  (add-hook 'completion-at-point-functions #'ac-capf-test-branch-capf nil t)
  (ac-capf-setup)
  (auto-complete)
  (let ((offered (list (ac-capf-test-session) (ac-capf-test-menu))))
    (ac-next)
    (let ((moved (ac-capf-test-session)))
      (ac-complete)
      (list :offered offered
            :moved moved
            :calls (nreverse ac-capf-test-table-calls)
            :after (ac-capf-test-buffer-state)
            :session (ac-capf-test-session))))))"##,
        expect![[
            r#"OK (:offered ((:prefix "che" :prefix-start 4 :common "che" :menu-live t :selected "cherry-pick" :completing t) (("cherry-pick" "s" nil nil (symbol "s")) ("check-status" "s" nil nil (symbol "s")) ("checkout-branch" "s" nil nil (symbol "s")))) :moved (:prefix "che" :prefix-start 4 :common "che" :menu-live t :selected "check-status" :completing t) :calls (("che" metadata) ("che" metadata) ("che" (boundaries . "")) ("che" t)) :after (:text "git check-status" :point 16 :mode text-mode :auto-complete t :sources (ac-source-capf) :capfs (ac-capf-test-branch-capf t ispell-completion-at-point)) :session (:prefix nil :prefix-start nil :common nil :menu-live nil :selected nil :completing nil))"#
        ]],
    )
}

fn capfs_that_offer_nothing_leave_the_buffer_and_the_session_untouched() -> ParityBatchCase {
    ParityBatchCase::value(
        "capfs_that_offer_nothing_leave_the_buffer_and_the_session_untouched",
        r##"(progn
 (defvar ac-capf-test-consulted nil)
 (defun ac-capf-test-silent-capf ()
   (push 'silent ac-capf-test-consulted)
   nil)
 (defun ac-capf-test-broken-capf ()
   (push 'broken ac-capf-test-consulted)
   'not-a-completion-table)
 (defun ac-capf-test-empty-capf ()
   (push 'empty ac-capf-test-consulted)
   (let ((bounds (bounds-of-thing-at-point 'symbol)))
     (when bounds
       (list (car bounds) (cdr bounds) nil))))
 (ac-capf-test-with-live-buffer
  (ac-capf-test-scratch #'text-mode "nothing matches zzz")
  (add-hook 'completion-at-point-functions #'ac-capf-test-empty-capf nil t)
  (add-hook 'completion-at-point-functions #'ac-capf-test-silent-capf nil t)
  (add-hook 'completion-at-point-functions #'ac-capf-test-broken-capf nil t)
  (ac-capf-setup)
  (let ((first (auto-complete)))
    (let ((after-first (list (ac-capf-test-session)
                             (ac-capf-test-menu)
                             (ac-capf-test-buffer-state)
                             (nreverse ac-capf-test-consulted)))
          (misbehaving completion--capf-misbehave-funs))
      (setq ac-capf-test-consulted nil)
      (let ((second (auto-complete)))
        (list :first first
              :after-first after-first
              :misbehaving misbehaving
              :second second
              :consulted-again (nreverse ac-capf-test-consulted)
              :menu (ac-capf-test-menu)
              :after (ac-capf-test-buffer-state)))))))"##,
        expect![[
            r#"OK (:first t :after-first ((:prefix nil :prefix-start nil :common nil :menu-live nil :selected nil :completing nil) nil (:text "nothing matches zzz" :point 19 :mode text-mode :auto-complete t :sources #1=(ac-source-capf) :capfs #2=(ac-capf-test-broken-capf ac-capf-test-silent-capf ac-capf-test-empty-capf t ispell-completion-at-point)) (broken)) :misbehaving (ac-capf-test-broken-capf) :second t :consulted-again (silent empty) :menu nil :after (:text "nothing matches zzz" :point 19 :mode text-mode :auto-complete t :sources #1# :capfs #2#))"#
        ]],
    )
}

fn ac_capf_skips_the_tags_capf_and_falls_through_a_non_exclusive_one() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_capf_skips_the_tags_capf_and_falls_through_a_non_exclusive_one",
        r##"(progn
 (defvar ac-capf-test-consulted nil)
 (defun ac-capf-test-keyword-capf ()
   (push 'keyword ac-capf-test-consulted)
   (let ((bounds (bounds-of-thing-at-point 'symbol)))
     (when bounds
       (list (car bounds) (cdr bounds) '("TODO" "FIXME" "NOTE") :exclusive 'no))))
 (defun ac-capf-test-glossary-capf ()
   (push 'glossary ac-capf-test-consulted)
   (let ((bounds (bounds-of-thing-at-point 'symbol)))
     (when bounds
       (list (car bounds) (cdr bounds) '("dispatcher" "display" "dispatch")))))
 (ac-capf-test-write "src/dispatch.el"
                     "(defun dispatch-legacy () 1)\n(defun dispatch-table () 2)\n")
 (ac-capf-test-write "src/TAGS"
                     (concat "\f\ndispatch.el,52\n"
                             "(defun dispatch-legacy \177dispatch-legacy\0011,0\n"
                             "(defun dispatch-table \177dispatch-table\0012,29\n"))
 (setq tags-file-name (ac-capf-test-path "src/TAGS")
       tags-table-list nil
       tags-revert-without-query t)
 (let ((default-capfs (default-value 'completion-at-point-functions)))
   (unwind-protect
       (progn
         (setq-default completion-at-point-functions
                       (list 'tags-completion-at-point-function
                             'ac-capf-test-glossary-capf))
         (ac-capf-test-with-live-buffer
          (ac-capf-test-scratch #'text-mode "the disp")
          (add-hook 'completion-at-point-functions #'ac-capf-test-keyword-capf nil t)
          (ac-capf-setup)
          (auto-complete)
          (let ((offered (list (ac-capf-test-session) (ac-capf-test-menu))))
            (ac-complete)
            (list :offered offered
                  :consulted (nreverse ac-capf-test-consulted)
                  :after (ac-capf-test-buffer-state)
                  :default-capfs (default-value 'completion-at-point-functions)
                  :tags-capf
                  (let ((response (tags-completion-at-point-function)))
                    (list (- (nth 0 response) (point-min))
                          (- (nth 1 response) (point-min))
                          (sort (all-completions "disp" (nth 2 response)) #'string<)
                          (nthcdr 3 response)))))))
     (setq-default completion-at-point-functions default-capfs))))"##,
        expect![[
            r#"OK (:offered ((:prefix "disp" :prefix-start 4 :common "disp" :menu-live t :selected "display" :completing t) (("display" "s" nil nil (symbol "s")) ("dispatch" "s" nil nil (symbol "s")) ("dispatcher" "s" nil nil (symbol "s")))) :consulted (keyword glossary) :after (:text "the display" :point 11 :mode text-mode :auto-complete t :sources (ac-source-capf) :capfs (ac-capf-test-keyword-capf t ispell-completion-at-point)) :default-capfs (tags-completion-at-point-function ac-capf-test-glossary-capf) :tags-capf (4 11 ("dispatch-legacy" "dispatch-table") (:exclusive no)))"#
        ]],
    )
    .fresh_process()
}

fn a_file_name_capf_makes_auto_complete_fail_on_the_unbound_arg_variable() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_file_name_capf_makes_auto_complete_fail_on_the_unbound_arg_variable",
        r##"(progn
 (defvar ac-capf-test-requested nil)
 (defun ac-capf-test-include-capf ()
   (let ((bounds (bounds-of-thing-at-point 'symbol)))
     (when bounds
       (push (buffer-substring-no-properties (car bounds) (cdr bounds))
             ac-capf-test-requested)
       (list (car bounds) (cdr bounds) #'completion-file-name-table))))
 (dolist (name '("lib/finder.el" "lib/files.el" "lib/fringe.el" "lib/cursor.el"))
   (ac-capf-test-write name (format ";;; %s\n" name)))
 (ac-capf-test-with-live-buffer
  (ac-capf-test-scratch #'emacs-lisp-mode "(load-file \"lib/fi")
  (setq default-directory (file-name-as-directory (ac-capf-test-path "")))
  (add-hook 'completion-at-point-functions #'ac-capf-test-include-capf nil t)
  (ac-capf-setup)
  (let ((outcome (condition-case failure (auto-complete) (error failure))))
    (list :outcome outcome
          :requested (nreverse ac-capf-test-requested)
          :file-name-table (sort (all-completions "lib/fi" #'completion-file-name-table)
                                 #'string<)
          :session (ac-capf-test-session)
          :after (ac-capf-test-buffer-state)))))"##,
        expect![[
            r#"OK (:outcome (void-variable arg) :requested ("lib/fi") :file-name-table ("files.el" "finder.el") :session (:prefix "lib/fi" :prefix-start 12 :common nil :menu-live nil :selected nil :completing nil) :after (:text "(load-file \"lib/fi\n" :point 18 :mode emacs-lisp-mode :auto-complete t :sources (ac-source-capf) :capfs (ac-capf-test-include-capf elisp-completion-at-point t)))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        emacs_lisp_mode_capf_completes_a_typed_symbol_through_the_trigger_key(),
        a_custom_capf_loses_its_annotation_document_and_exit_metadata_through_ac_capf(),
        a_capf_whose_collection_is_a_function_drives_the_auto_complete_menu(),
        capfs_that_offer_nothing_leave_the_buffer_and_the_session_untouched(),
        ac_capf_skips_the_tags_capf_and_falls_through_a_non_exclusive_one(),
        a_file_name_capf_makes_auto_complete_fail_on_the_unbound_arg_variable(),
    ]
}
