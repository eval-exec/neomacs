//! Strong combo-complex-62 oracle tests — final deep probes:
//! org-agenda-get-scheduled, org-timer item list, org-element-
//! cache-reset multi-cycle, org-babel-tangle multi-file,
//! org-habit parse (if available), org-attach file ops,
//! org-mobile-agendas, org-babel with :session across blocks
//! of different languages, org-crypt entry encrypt/decrypt,
//! org-export with #+SETUPFILE, and org-goto UI structure.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo62_agenda_get_scheduled() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:agenda-error t) (:map-todos ((\"A\" nil))))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-agenda)
  (insert "* A\nSCHEDULED: <2024-01-15 Mon>\n* B\nDEADLINE: <2024-01-20 Sat>\n")
  (let ((r '()))
    ;; org-agenda-get-day-entries
    (condition-case nil
        (let* ((today (org-today))
               (entries
                (when (fboundp 'org-agenda-get-day-entries)
                  (org-agenda-get-day-entries
                   (buffer-file-name) today :scheduled))))
          (push (list :scheduled-fbound (fboundp 'org-agenda-get-day-entries)) r)
          (push (list :entry-count (length entries)) r))
      (error (push (list :agenda-error t) r)))
    ;; get all TODO entries as agenda-like list
    (push (list :map-todos (org-map-entries
                            (lambda () (list (org-get-heading t t t t)
                                             (org-get-todo-state)))
                            "SCHEDULED<>\"\"")) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo62_timer_item_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:timer-item-fbound t) (:timer-item-inserted t) (:buffer \"- 0:00:00 :: \") (:timer-seconds nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-timer)
  (let ((r '()))
    ;; org-timer-item: insert a timer item
    (push (list :timer-item-fbound (fboundp 'org-timer-item)) r)
    (condition-case nil
        (progn (org-timer-item)
               (push (list :timer-item-inserted t) r)
               (push (list :buffer (buffer-string)) r))
      (error (push (list :timer-item-error t) r)))
    ;; start, pause, stop cycle
    (condition-case nil (org-timer-start) (error nil))
    (condition-case nil (org-timer-pause-or-continue) (error nil))
    (condition-case nil
        (let ((secs (org-timer-stop)))
          (push (list :timer-seconds (numberp secs)) r))
      (error nil))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo62_element_cache_reset_multicycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:iter 0 :before 2 :after 3) (:iter 1 :before 3 :after 4) (:iter 2 :before 4 :after 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-element)
  (insert "* A\n** B\n")
  (let ((r '()))
    ;; parse, modify, cache-reset, reparse - do 3 times
    (dotimes (i 3)
      (let ((before (length (org-element-map (org-element-parse-buffer) 'headline #'identity))))
        (goto-char (point-max))
        (insert (format "\n** C%d\n" i))
        (org-element-cache-reset)
        (let ((after (length (org-element-map (org-element-parse-buffer) 'headline #'identity))))
          (push (list :iter i :before before :after after) r))))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo62_babel_tangle_multifile() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-tangle)
  (let ((file1 (make-temp-file "tangle-" nil ".el"))
        (file2 (make-temp-file "tangle-" nil ".sh")))
    (insert (format "#+begin_src emacs-lisp :tangle %s\n(message \"lisp\")\n#+end_src\n" file1))
    (insert (format "#+begin_src sh :tangle %s\necho shell\n#+end_src\n" file2))
    (let ((r '()))
      (goto-char (point-min))
      (condition-case e
          (progn (org-babel-tangle)
                 (push (list :tangled t) r)
                 (push (list :file1-exists (file-exists-p file1)) r)
                 (push (list :file2-exists (file-exists-p file2)) r))
        (error (push (list :tangle-error (car e)) r)))
      (condition-case nil (delete-file file1) (error nil))
      (condition-case nil (delete-file file2) (error nil))
      (nreverse r))))"##,
    );
}

#[test]
fn combo62_habit_parse_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:habit-fbound t) (:is-habit t) (:parse-todo-fbound t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-habit)
  (insert "* TODO Habit task\n")
  (insert "SCHEDULED: <2024-06-01 Sat .+1d>\n")
  (insert ":PROPERTIES:\n:STYLE:    habit\n:END:\n")
  (let ((r '()))
    (goto-char (point-min))
    ;; org-is-habit-p
    (push (list :habit-fbound (fboundp 'org-is-habit-p)) r)
    (condition-case nil
        (let ((is-habit (when (fboundp 'org-is-habit-p) (org-is-habit-p (point)))))
          (push (list :is-habit is-habit) r))
      (error (push (list :habit-error t) r)))
    ;; org-habit-parse-todo
    (push (list :parse-todo-fbound (fboundp 'org-habit-parse-todo)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo62_attach_file_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"‘org-id-get’ expects a file-visiting buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-attach)
  (insert "* Task\n")
  (let ((r '()))
    (goto-char (point-min))
    ;; org-attach-dir
    (let ((dir (org-attach-dir t)))
      (push (list :attach-dir-exists (and dir (file-directory-p dir))) r)
      ;; write a file into attach dir
      (when dir
        (let ((testfile (expand-file-name "test-attach.txt" dir)))
          (with-temp-file testfile (insert "attach content"))
          (push (list :file-created (file-exists-p testfile)) r)
          ;; org-attach-file-list
          (push (list :file-list (org-attach-file-list dir)) r)
          ;; org-attach-delete-one
          (condition-case nil
              (progn (org-attach-delete-one "test-attach.txt")
                     (push (list :file-deleted (not (file-exists-p testfile))) r))
            (error (push (list :delete-error t) r)))))
    (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo62_babel_session_cross_language() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"ob-sh\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (require 'ob-sh)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src emacs-lisp :results value\n'(42 43 44)\n#+end_src\n\n")
    (insert "#+begin_src sh :results output :var elisp-data=previous-elisp\necho \"data: $elisp-data\"\n#+end_src\n")
    (let ((r '()))
      (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      (search-forward "#+begin_src sh")
      (condition-case e
          (push (org-babel-execute-src-block) r)
        (error (push (list :sh-error (car e)) r)))
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo62_crypt_entry_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:encrypt-fbound t) (:decrypt-fbound t) (:encrypt-error t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-crypt)
  (insert "* Secret\nSecret content.\n")
  (let ((r '()))
    (goto-char (point-min))
    ;; org-crypt encrypt/decrypt
    (push (list :encrypt-fbound (fboundp 'org-encrypt-entry)) r)
    (push (list :decrypt-fbound (fboundp 'org-decrypt-entry)) r)
    ;; try encrypting (may fail without GPG)
    (condition-case nil
        (let* ((epg-context (ignore-errors (epg-make-context)))
               (before (buffer-string)))
          (progn (org-encrypt-entry) nil)
          (push (list :encrypted (not (string= before (buffer-string)))) r))
      (error (push (list :encrypt-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo62_export_setupfile() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:author-from-setup (#(\"File Author\" 0 11 (:parent (#(\"File Author\" 0 11 (:parent #5))))))) (:num-from-setup nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-ascii)
  (let ((sf (make-temp-file "org-setup-" nil ".org"))
        (org-export-show-temporary-export-buffer nil))
    (with-temp-file sf
      (insert "#+AUTHOR: File Author\n#+OPTIONS: num:nil\n"))
    (insert (format "#+SETUPFILE: %s\n" sf))
    (insert "* Test\nBody.\n")
    (let ((r '()))
      (condition-case e
          (let ((info (org-export-get-environment)))
            (push (list :author-from-setup (plist-get info :author)) r)
            (push (list :num-from-setup (plist-get info :with-numbers)) r))
        (error (push (list :setupfile-error (car e)) r)))
      (condition-case nil (delete-file sf) (error nil))
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo62_mobile_agendas() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:mobile-files-fbound t) (:mobile-directory-fbound t) (:push-fbound t) (:pull-fbound t) (:default-dir \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-mobile)
  (list
   ;; org-mobile-files
   (list :mobile-files-fbound (boundp 'org-mobile-files))
   (list :mobile-directory-fbound (boundp 'org-mobile-directory))
   ;; org-mobile-push/pull
   (list :push-fbound (fboundp 'org-mobile-push))
   (list :pull-fbound (fboundp 'org-mobile-pull))
   ;; default values
   (cond ((boundp 'org-mobile-directory) (list :default-dir org-mobile-directory))
         (t :not-bound))
   ))"##,
        expect,
    );
}
