use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_cycle_visibility_state_transitions_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 38 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\nBody A.\n\n")
    (insert "** A1\nBody A1.\n\n")
    (insert "*** A1a\nBody A1a.\n\n")
    (insert "*** A1b\nBody A1b.\n\n")
    (insert "** A2\nBody A2.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle
                                (line-number-at-pos)
                                (invisible-p (point))
                                (org-outline-level))
                          (list needle 'not-found nil nil))))
                  '("A" "A1" "A1a" "A1b" "A2")))))
      ;; Cycle at A: overview->children->subtree->overview
      (goto-char (point-min))
      (search-forward "A")
      (beginning-of-line)
      (let ((v0 (funcall vis)))
        (org-cycle nil)  ;; overview: only top-level visible
        (let ((v1 (funcall vis)))
          (org-cycle nil)  ;; children: show A1, A2
          (let ((v2 (funcall vis)))
            (org-cycle nil)  ;; subtree: show all
            (let ((v3 (funcall vis)))
              (org-cycle nil)  ;; back to overview
              (let ((v4 (funcall vis)))
                (list v0 v1 v2 v3 v4
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_then_edit_preserves_visibility_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 39 51)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* P\n")
    (insert "** P1\nBody P1.\n\n")
    (insert "** P2\nBody P2.\n\n")
    (insert "** P3\nBody P3.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("P" "P1" "P2" "P3")))))
      ;; Cycle P to show children
      (goto-char (point-min))
      (search-forward "P")
      (beginning-of-line)
      (org-cycle nil)  ;; overview
      (org-cycle nil)  ;; children visible
      (let ((after-cycle (funcall vis)))
        ;; Edit: insert P4 under P
        (goto-char (point-max))
        (insert "** P4\nBody P4.\n")
        (let ((after-edit (funcall vis)))
          ;; Re-cycle P
          (goto-char (point-min))
          (search-forward "P\n")
          (beginning-of-line)
          (org-cycle nil)  ;; overview
          (org-cycle nil)  ;; children visible
          (let ((after-re-cycle (funcall vis)))
            (list after-cycle after-edit after-re-cycle
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_global_cycle_with_hidden_edits_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 42 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Project\n")
    (insert "** DONE Task-A\nBody A.\n\n")
    (insert "** TODO Task-B\nBody B.\n\n")
    (insert "** WAIT Task-C\nBody C.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle
                                (line-number-at-pos)
                                (invisible-p (point))
                                (org-outline-level))
                          (list needle 'not-found nil nil))))
                  '("Project" "Task-A" "Task-B" "Task-C")))))
      ;; Global cycle: overview
      (org-global-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Global cycle: children
        (org-global-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Global cycle: all
          (org-global-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Edit: insert Task-D
            (goto-char (point-max))
            (insert "** NEXT Task-D\nBody D.\n")
            ;; Re-global-cycle: overview
            (org-global-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Re-global-cycle: children
              (org-global-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_property_drawer_visibility_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 46 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:Owner: Alice\n:END:\n")
    (insert "Body alpha.\n\n")
    (insert "** DONE Beta\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:Owner: Bob\n:END:\n")
    (insert "Body beta.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("Alpha" "Effort" "Owner" "Body alpha" "Beta")))))
      ;; Cycle Alpha: overview
      (goto-char (point-min))
      (search-forward "Alpha")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle Alpha: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Cycle Alpha: subtree
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Edit: set property
            (goto-char (point-min))
            (search-forward "Alpha")
            (beginning-of-line)
            (org-set-property "Status" "active")
            ;; Re-cycle
            (goto-char (point-min))
            (search-forward "Alpha")
            (beginning-of-line)
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              (list v1 v2 v3 v4
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_clock_logbook_visibility_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 44 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Task\n")
    (insert ":LOGBOOK:\nCLOCK: [2026-05-28 Wed 09:00]--[2026-05-28 Wed 11:00] =>  2:00\n:END:\n")
    (insert ":PROPERTIES:\n:Effort: 3h\n:END:\n")
    (insert "Body.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("Task" "LOGBOOK" "CLOCK" "Effort" "Body")))))
      ;; Cycle: overview
      (goto-char (point-min))
      (search-forward "Task")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Cycle: subtree
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Edit: add clock
            (goto-char (point-min))
            (search-forward "Task")
            (end-of-line)
            (insert "\nCLOCK: [2026-05-28 Wed 14:00]--[2026-05-28 Wed 15:00] =>  1:00\n")
            ;; Re-cycle
            (goto-char (point-min))
            (search-forward "Task")
            (beginning-of-line)
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              (list v1 v2 v3 v4
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_tag_toggle_visibility_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 42 51)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha :work:\nBody alpha.\n\n")
    (insert "** DONE Beta :home:\nBody beta.\n\n")
    (insert "** TODO Gamma :work:urgent:\nBody gamma.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle
                                (invisible-p (point))
                                (org-get-tags nil t))
                          (list needle 'not-found nil))))
                  '("Alpha" "Beta" "Gamma")))))
      ;; Cycle Alpha: overview
      (goto-char (point-min))
      (search-forward "Alpha")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Toggle tag on Alpha
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-toggle-tag "review" 'on)
          ;; Re-cycle
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            (list v1 v2 v3
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_font_lock_after_cycle_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 50 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (let ((org-fontify-whole-heading-line t)
          (org-fontify-done-headline t))
      (org-mode)
      (insert "* TODO Alpha\nBody alpha.\n\n")
      (insert "** DONE Beta\nBody beta.\n\n")
      (insert "** TODO Gamma\nBody gamma.\n\n")
      (font-lock-ensure (point-min) (point-max))
      (let ((snap (lambda ()
                    (mapcar
                     (lambda (needle)
                       (save-excursion
                         (goto-char (point-min))
                         (if (search-forward needle nil t)
                             (list needle
                                   (invisible-p (point))
                                   (get-text-property (line-beginning-position) 'face))
                             (list needle 'not-found nil))))
                     '("Alpha" "Beta" "Gamma")))))
        ;; Initial
        (let ((v0 (funcall snap)))
          ;; Cycle Alpha: overview
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-cycle nil)
          (font-lock-ensure (point-min) (point-max))
          (let ((v1 (funcall snap)))
            ;; Cycle: children
            (org-cycle nil)
            (font-lock-ensure (point-min) (point-max))
            (let ((v2 (funcall snap)))
              ;; Edit: change Beta to TODO
              (goto-char (point-min))
              (search-forward "DONE Beta")
              (replace-match "TODO Beta")
              (font-lock-ensure (point-min) (point-max))
              ;; Re-cycle
              (goto-char (point-min))
              (search-forward "Alpha")
              (beginning-of-line)
              (org-cycle nil)
              (font-lock-ensure (point-min) (point-max))
              (let ((v3 (funcall snap)))
                (list v0 v1 v2 v3
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_multi_level_nested_visibility_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 56 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* L1\n")
    (insert "** L2\n")
    (insert "*** L3\n")
    (insert "**** L4\n")
    (insert "***** L5\nBody L5.\n\n")
    (insert "**** L4b\nBody L4b.\n\n")
    (insert "*** L3b\nBody L3b.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle
                                (line-number-at-pos)
                                (invisible-p (point))
                                (org-outline-level))
                          (list needle 'not-found nil nil))))
                  '("L1" "L2" "L3" "L4" "L5" "L4b" "L3b")))))
      ;; Cycle L1: overview
      (goto-char (point-min))
      (search-forward "L1")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle L1: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Cycle L1: subtree (all visible)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle L2 locally
            (goto-char (point-min))
            (search-forward "L2")
            (beginning-of-line)
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Edit: insert L4c under L3
              (goto-char (point-min))
              (search-forward "L3b")
              (end-of-line)
              (insert "\n**** L4c\nBody L4c.\n")
              ;; Re-cycle L1
              (goto-char (point-min))
              (search-forward "L1")
              (beginning-of-line)
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_after_hide_all_show_all_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-fold-hide-all)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\nBody A.\n\n")
    (insert "** A1\nBody A1.\n\n")
    (insert "** A2\nBody A2.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2")))))
      ;; Hide all
      (org-fold-hide-all)
      (let ((v1 (funcall vis)))
        ;; Show all
        (org-fold-show-all)
        (let ((v2 (funcall vis)))
          ;; Cycle A: overview
          (goto-char (point-min))
          (search-forward "A\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle A: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Hide all again
              (org-fold-hide-all)
              (let ((v5 (funcall vis)))
                ;; Show all again
                (org-fold-show-all)
                (let ((v6 (funcall vis)))
                  (list v1 v2 v3 v4 v5 v6
                        (buffer-substring-no-properties
                         (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_then_refile_preserves_state_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 64 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (require 'org-refile)
  (let* ((root (make-temp-file "org-cycle-refile-" t))
         (file-a (expand-file-name "a.org" root))
         (file-b (expand-file-name "b.org" root))
         (org-refile-targets `((,file-b :maxlevel . 2))))
    (unwind-protect
        (progn
          (with-temp-file file-a
            (insert "* Source\n")
            (insert "** Item-1\nBody 1.\n\n")
            (insert "** Item-2\nBody 2.\n\n"))
          (with-temp-file file-b
            (insert "* Target\n"))
          (let* ((buf-a (find-file-noselect file-a))
                 (vis (lambda ()
                        (with-current-buffer buf-a
                          (mapcar
                           (lambda (needle)
                             (save-excursion
                               (goto-char (point-min))
                               (if (search-forward needle nil t)
                                   (list needle (invisible-p (point)))
                                   (list needle 'not-found))))
                           '("Source" "Item-1" "Item-2"))))))
            ;; Cycle Source: overview
            (with-current-buffer buf-a
              (org-mode)
              (goto-char (point-min))
              (search-forward "Source")
              (beginning-of-line)
              (org-cycle nil))
            (let ((v1 (funcall vis)))
              ;; Cycle Source: children
              (with-current-buffer buf-a
                (goto-char (point-min))
                (search-forward "Source")
                (beginning-of-line)
                (org-cycle nil))
              (let ((v2 (funcall vis)))
                ;; Refile Item-1
                (with-current-buffer buf-a
                  (goto-char (point-min))
                  (search-forward "Item-1")
                  (beginning-of-line)
                  (org-refile nil nil (list "Target" file-b nil nil)))
                (let ((v3 (funcall vis)))
                  ;; Re-cycle Source
                  (with-current-buffer buf-a
                    (goto-char (point-min))
                    (search-forward "Source")
                    (beginning-of-line)
                    (org-cycle nil))
                  (let ((v4 (funcall vis)))
                    (list v1 v2 v3 v4
                          (with-current-buffer buf-a
                            (buffer-substring-no-properties
                             (point-min) (point-max)))))))))))
      (dolist (f (list file-a file-b))
        (when (get-file-buffer f) (kill-buffer (get-file-buffer f)))
        (when (file-exists-p f) (delete-file f)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_cycle_then_edit_then_cycle_again_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 44 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* P\n")
    (insert "** P1\nBody P1.\n\n")
    (insert "** P2\nBody P2.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("P" "P1" "P2" "P3")))))
      ;; Cycle P: overview
      (goto-char (point-min))
      (search-forward "P\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle P: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Edit: insert P3
          (goto-char (point-max))
          (insert "** P3\nBody P3.\n")
          ;; Cycle P: overview
          (goto-char (point-min))
          (search-forward "P\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle P: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Cycle P: subtree
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_global_cycle_three_state_with_edits_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 42 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2" "B" "B1")))))
      ;; Global cycle: overview
      (org-global-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Global cycle: children
        (org-global-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Global cycle: all
          (org-global-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Edit: insert A3 under A
            (goto-char (point-min))
            (search-forward "A2")
            (end-of-line)
            (insert "\n** A3\nBody.\n")
            ;; Global cycle: overview
            (org-global-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Global cycle: children
              (org-global-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_subtree_then_hide_show_cycle_again() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 45 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* Top\n")
    (insert "** Mid\n")
    (insert "*** Leaf\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("Top" "Mid" "Leaf")))))
      ;; Cycle Top: overview
      (goto-char (point-min))
      (search-forward "Top")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle Top: subtree
        (org-cycle nil)
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Hide subtree
          (goto-char (point-min))
          (search-forward "Top")
          (beginning-of-line)
          (org-fold-hide-subtree)
          (let ((v3 (funcall vis)))
            ;; Show subtree
            (goto-char (point-min))
            (search-forward "Top")
            (beginning-of-line)
            (org-fold-show-subtree)
            (let ((v4 (funcall vis)))
              ;; Cycle again
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_todo_state_changes_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 45 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Project\n")
    (insert "** DONE Task-A\nBody A.\n\n")
    (insert "** TODO Task-B\nBody B.\n\n")
    (insert "** WAIT Task-C\nBody C.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle
                                (invisible-p (point))
                                (org-get-todo-state))
                          (list needle 'not-found nil))))
                  '("Project" "Task-A" "Task-B" "Task-C")))))
      ;; Cycle Project: overview
      (goto-char (point-min))
      (search-forward "Project")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Change Task-B to DONE
          (goto-char (point-min))
          (search-forward "TODO Task-B")
          (org-todo 'done)
          ;; Cycle Project: overview
          (goto-char (point-min))
          (search-forward "Project")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              (list v1 v2 v3 v4
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_priority_changes_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\nBody.\n\n")
    (insert "** TODO Beta\nBody.\n\n")
    (insert "** TODO Gamma\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle
                                (invisible-p (point))
                                (org-get-priority (point) 'force))
                          (list needle 'not-found nil))))
                  '("Alpha" "Beta" "Gamma")))))
      ;; Cycle Alpha: overview
      (goto-char (point-min))
      (search-forward "Alpha")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Set priority on Beta
          (goto-char (point-min))
          (search-forward "Beta")
          (beginning-of-line)
          (org-priority ?A)
          ;; Re-cycle
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            (list v1 v2 v3
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_tag_and_property_changes_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 49 51)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Root :root:\n")
    (insert ":PROPERTIES:\n:Owner: Alice\n:END:\n")
    (insert "** DONE Child-A :fe:\nBody A.\n\n")
    (insert "** TODO Child-B :be:\nBody B.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle
                                (invisible-p (point))
                                (org-get-tags nil t)
                                (org-entry-get nil "Owner" 'inherit))
                          (list needle 'not-found nil nil))))
                  '("Root" "Child-A" "Child-B")))))
      ;; Cycle Root: overview
      (goto-char (point-min))
      (search-forward "Root")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Toggle tag on Root
          (goto-char (point-min))
          (search-forward "Root")
          (beginning-of-line)
          (org-toggle-tag "review" 'on)
          ;; Set property on Child-B
          (goto-char (point-min))
          (search-forward "Child-B")
          (beginning-of-line)
          (org-set-property "Status" "blocked")
          ;; Re-cycle
          (goto-char (point-min))
          (search-forward "Root")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            (list v1 v2 v3
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_after_fold_hide_all_then_global_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-fold-hide-all)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "B" "B1")))))
      ;; Hide all
      (org-fold-hide-all)
      (let ((v1 (funcall vis)))
        ;; Global cycle: overview
        (org-global-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Global cycle: children
          (org-global-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Global cycle: all
            (org-global-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Hide all again
              (org-fold-hide-all)
              (let ((v5 (funcall vis)))
                ;; Global cycle again
                (org-global-cycle nil)
                (let ((v6 (funcall vis)))
                  (list v1 v2 v3 v4 v5 v6
                        (buffer-substring-no-properties
                         (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_insert_between_cycles_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 50 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* R\n")
    (insert "** R1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("R" "R1" "R2" "R3")))))
      ;; Cycle R: overview
      (goto-char (point-min))
      (search-forward "R\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle R: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Insert R2
          (goto-char (point-max))
          (insert "** R2\nBody.\n")
          ;; Cycle R: overview
          (goto-char (point-min))
          (search-forward "R\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle R: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Insert R3
              (goto-char (point-max))
              (insert "** R3\nBody.\n")
              ;; Cycle R: children
              (goto-char (point-min))
              (search-forward "R\n")
              (beginning-of-line)
              (org-cycle nil)
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_clock_in_out_between_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 45 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Task\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:END:\n")
    (insert "Body.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("Task" "LOGBOOK" "CLOCK" "Effort" "Body")))))
      ;; Cycle: overview
      (goto-char (point-min))
      (search-forward "Task")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle: subtree
        (org-cycle nil)
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Hide subtree
          (goto-char (point-min))
          (search-forward "Task")
          (beginning-of-line)
          (org-fold-hide-subtree)
          (let ((v3 (funcall vis)))
            ;; Show subtree
            (goto-char (point-min))
            (search-forward "Task")
            (beginning-of-line)
            (org-fold-show-subtree)
            (let ((v4 (funcall vis)))
              ;; Cycle again
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_drawer_toggle_between_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"Alpha\" 2) (\"Effort\" 2) (\"Body alpha\" 2) (\"Beta\" 2)) ((\"Alpha\" nil) (\"Effort\" nil) (\"Body alpha\" nil) (\"Beta\" 2)) ((\"Alpha\" nil) (\"Effort\" nil) (\"Body alpha\" nil) (\"Beta\" nil)) ((\"Alpha\" 2) (\"Effort\" 2) (\"Body alpha\" 2) (\"Beta\" 2)) ((\"Alpha\" nil) (\"Effort\" nil) (\"Body alpha\" nil) (\"Beta\" nil)) ((\"Alpha\" 2) (\"Effort\" 2) (\"Body alpha\" 2) (\"Beta\" 2)) \"* TODO Alpha\\n:PROPERTIES:\\n:Effort: 2h\\n:END:\\nBody alpha.\\n\\n** DONE Beta\\n:PROPERTIES:\\n:Effort: 1h\\n:END:\\nBody beta.\\n\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:END:\n")
    (insert "Body alpha.\n\n")
    (insert "** DONE Beta\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:END:\n")
    (insert "Body beta.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("Alpha" "Effort" "Body alpha" "Beta")))))
      ;; Cycle Alpha: overview
      (goto-char (point-min))
      (search-forward "Alpha")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle Alpha: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Cycle Alpha: subtree
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Hide Alpha subtree
            (goto-char (point-min))
            (search-forward "Alpha")
            (beginning-of-line)
            (org-fold-hide-subtree)
            (let ((v4 (funcall vis)))
              ;; Show Alpha subtree
              (goto-char (point-min))
              (search-forward "Alpha")
              (beginning-of-line)
              (org-fold-show-subtree)
              (let ((v5 (funcall vis)))
                ;; Cycle Alpha: overview
                (org-cycle nil)
                (let ((v6 (funcall vis)))
                  (list v1 v2 v3 v4 v5 v6
                        (buffer-substring-no-properties
                         (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_subtree_promote_demote() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 57 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle
                                (invisible-p (point))
                                (org-outline-level))
                          (list needle 'not-found nil))))
                  '("A" "A1" "A2")))))
      ;; Cycle A: overview
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle A: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Demote A1
          (goto-char (point-min))
          (search-forward "A1")
          (beginning-of-line)
          (org-demote-subtree)
          ;; Cycle A: overview
          (goto-char (point-min))
          (search-forward "A\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle A: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Promote A1 back
              (goto-char (point-min))
              (search-forward "A1")
              (beginning-of-line)
              (org-promote-subtree)
              ;; Cycle A: children
              (goto-char (point-min))
              (search-forward "A\n")
              (beginning-of-line)
              (org-cycle nil)
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_move_subtree_between_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 44 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (insert "** A3\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2" "A3")))))
      ;; Cycle A: overview
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle A: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Move A3 up
          (goto-char (point-min))
          (search-forward "A3")
          (beginning-of-line)
          (org-move-subtree-up)
          ;; Cycle A: overview
          (goto-char (point-min))
          (search-forward "A\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle A: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              (list v1 v2 v3 v4
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_narrow_widen_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 42 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\nBody A.\n\n")
    (insert "** A1\nBody A1.\n\n")
    (insert "** A2\nBody A2.\n\n")
    (insert "* B\nBody B.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2" "B")))))
      ;; Cycle A: overview
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle A: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Narrow to A
          (org-narrow-to-subtree)
          (let ((v3 (funcall vis)))
            ;; Widen
            (widen)
            (let ((v4 (funcall vis)))
              ;; Cycle A: overview
              (goto-char (point-min))
              (search-forward "A\n")
              (beginning-of-line)
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_copy_paste_subtree_between_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 50 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2" "B" "B1" "B2")))))
      ;; Cycle A: overview
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle A: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Copy A2 subtree
          (goto-char (point-min))
          (search-forward "A2")
          (beginning-of-line)
          (org-copy-subtree)
          ;; Paste under B
          (goto-char (point-min))
          (search-forward "B1")
          (end-of-line)
          (org-paste-subtree 2)
          ;; Cycle B: overview
          (goto-char (point-min))
          (search-forward "B\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle B: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              (list v1 v2 v3 v4
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_cut_paste_subtree_between_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 58 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (insert "** A3\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2" "A3" "B" "B1")))))
      ;; Cycle A: overview
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle A: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Cut A2 subtree
          (goto-char (point-min))
          (search-forward "A2")
          (beginning-of-line)
          (org-cut-subtree)
          ;; Paste under B
          (goto-char (point-min))
          (search-forward "B1")
          (end-of-line)
          (org-paste-subtree 2)
          ;; Cycle A: overview
          (goto-char (point-min))
          (search-forward "A\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle A: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Cycle B: children
              (goto-char (point-min))
              (search-forward "B\n")
              (beginning-of-line)
              (org-cycle nil)
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_sort_children_between_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 44 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* P\n")
    (insert "** Charlie\nBody.\n\n")
    (insert "** Alpha\nBody.\n\n")
    (insert "** Bravo\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("P" "Charlie" "Alpha" "Bravo")))))
      ;; Cycle P: overview
      (goto-char (point-min))
      (search-forward "P\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle P: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Sort children alphabetically
          (goto-char (point-min))
          (search-forward "P")
          (beginning-of-line)
          (org-sort-entries nil ?a)
          ;; Cycle P: overview
          (goto-char (point-min))
          (search-forward "P\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle P: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              (list v1 v2 v3 v4
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_todo_cycle_between_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 57 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* Project\n")
    (insert "** TODO Task-A\nBody.\n\n")
    (insert "** TODO Task-B\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle
                                (invisible-p (point))
                                (org-get-todo-state))
                          (list needle 'not-found nil))))
                  '("Project" "Task-A" "Task-B")))))
      ;; Cycle Project: overview
      (goto-char (point-min))
      (search-forward "Project")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle Project: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Cycle Task-A TODO state
          (goto-char (point-min))
          (search-forward "TODO Task-A")
          (beginning-of-line)
          (org-todo 'done)
          ;; Cycle Project: overview
          (goto-char (point-min))
          (search-forward "Project")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle Project: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Cycle Task-B TODO state to WAIT
              (goto-char (point-min))
              (search-forward "TODO Task-B")
              (beginning-of-line)
              (org-todo 'wait)
              ;; Cycle Project: children
              (goto-char (point-min))
              (search-forward "Project")
              (beginning-of-line)
              (org-cycle nil)
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_tag_toggle_between_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 57 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha :work:\nBody.\n\n")
    (insert "** DONE Beta :home:\nBody.\n\n")
    (insert "** TODO Gamma :work:\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle
                                (invisible-p (point))
                                (org-get-tags nil t))
                          (list needle 'not-found nil))))
                  '("Alpha" "Beta" "Gamma")))))
      ;; Cycle Alpha: overview
      (goto-char (point-min))
      (search-forward "Alpha")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle Alpha: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Toggle tag on Alpha
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-toggle-tag "review" 'on)
          ;; Cycle Alpha: overview
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle Alpha: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Toggle tag on Gamma
              (goto-char (point-min))
              (search-forward "Gamma")
              (beginning-of-line)
              (org-toggle-tag "urgent" 'on)
              ;; Cycle Alpha: children
              (goto-char (point-min))
              (search-forward "Alpha")
              (beginning-of-line)
              (org-cycle nil)
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_property_set_between_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 61 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:END:\n")
    (insert "Body alpha.\n\n")
    (insert "** DONE Beta\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:END:\n")
    (insert "Body beta.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle
                                (invisible-p (point))
                                (org-entry-get nil "Effort")
                                (org-entry-get nil "Status"))
                          (list needle 'not-found nil nil))))
                  '("Alpha" "Beta")))))
      ;; Cycle Alpha: overview
      (goto-char (point-min))
      (search-forward "Alpha")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle Alpha: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Set property on Alpha
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-set-property "Status" "active")
          ;; Cycle Alpha: overview
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle Alpha: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Set property on Beta
              (goto-char (point-min))
              (search-forward "Beta")
              (beginning-of-line)
              (org-set-property "Status" "done")
              ;; Cycle Alpha: children
              (goto-char (point-min))
              (search-forward "Alpha")
              (beginning-of-line)
              (org-cycle nil)
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_property_delete_between_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 60 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:Owner: Alice\n:END:\n")
    (insert "Body alpha.\n\n")
    (insert "** DONE Beta\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:Owner: Bob\n:END:\n")
    (insert "Body beta.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle
                                (invisible-p (point))
                                (org-entry-get nil "Owner"))
                          (list needle 'not-found nil))))
                  '("Alpha" "Beta")))))
      ;; Cycle Alpha: overview
      (goto-char (point-min))
      (search-forward "Alpha")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle Alpha: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Delete Owner from Alpha
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-delete-property "Owner")
          ;; Cycle Alpha: overview
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle Alpha: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Delete Owner from Beta
              (goto-char (point-min))
              (search-forward "Beta")
              (beginning-of-line)
              (org-delete-property "Owner")
              ;; Cycle Alpha: children
              (goto-char (point-min))
              (search-forward "Alpha")
              (beginning-of-line)
              (org-cycle nil)
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_schedule_between_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 42 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\nBody.\n\n")
    (insert "** TODO Beta\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("Alpha" "Beta" "SCHEDULED")))))
      ;; Cycle Alpha: overview
      (goto-char (point-min))
      (search-forward "Alpha")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle Alpha: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Schedule Alpha
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-schedule nil '(5 28 2026))
          ;; Cycle Alpha: overview
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle Alpha: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              (list v1 v2 v3 v4
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_deadline_between_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 42 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\nBody.\n\n")
    (insert "** TODO Beta\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("Alpha" "Beta" "DEADLINE")))))
      ;; Cycle Alpha: overview
      (goto-char (point-min))
      (search-forward "Alpha")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle Alpha: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Deadline Alpha
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-deadline nil '(6 1 2026))
          ;; Cycle Alpha: overview
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle Alpha: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              (list v1 v2 v3 v4
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_global_cycle_rapid_toggle_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 34 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\n")
    (insert "*** A1a\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A1a" "A2" "B" "B1")))))
      ;; Rapid toggle: overview -> children -> all -> overview -> children
      (org-global-cycle nil)
      (let ((v1 (funcall vis)))
        (org-global-cycle nil)
        (let ((v2 (funcall vis)))
          (org-global-cycle nil)
          (let ((v3 (funcall vis)))
            (org-global-cycle nil)
            (let ((v4 (funcall vis)))
              (org-global-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_local_cycle_rapid_toggle_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 34 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* P\n")
    (insert "** P1\nBody.\n\n")
    (insert "** P2\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("P" "P1" "P2")))))
      ;; Rapid local toggle: overview -> children -> subtree -> overview -> children
      (goto-char (point-min))
      (search-forward "P\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_mixed_local_global_toggle_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 44 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2" "B" "B1")))))
      ;; Global cycle: overview
      (org-global-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Local cycle A: children
        (goto-char (point-min))
        (search-forward "A\n")
        (beginning-of-line)
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Global cycle: children
          (org-global-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Local cycle B: subtree
            (goto-char (point-min))
            (search-forward "B\n")
            (beginning-of-line)
            (org-cycle nil)
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Global cycle: all
              (org-global-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_fold_subtree_hide_show_between() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2" "B" "B1")))))
      ;; Cycle A: overview
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle A: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Fold A subtree
          (goto-char (point-min))
          (search-forward "A")
          (beginning-of-line)
          (org-fold-subtree)
          (let ((v3 (funcall vis)))
            ;; Show A subtree
            (goto-char (point-min))
            (search-forward "A")
            (beginning-of-line)
            (org-fold-show-subtree)
            (let ((v4 (funcall vis)))
              ;; Cycle A: overview
              (goto-char (point-min))
              (search-forward "A\n")
              (beginning-of-line)
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_fold_hide_entry_between() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 43 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\nBody A.\n\n")
    (insert "** A1\nBody A1.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "Body A" "A1")))))
      ;; Cycle A: overview
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle A: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Hide entry
          (goto-char (point-min))
          (search-forward "A")
          (beginning-of-line)
          (org-fold-hide-entry)
          (let ((v3 (funcall vis)))
            ;; Show entry
            (goto-char (point-min))
            (search-forward "A")
            (beginning-of-line)
            (org-fold-show-entry)
            (let ((v4 (funcall vis)))
              ;; Cycle A: overview
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_fold_hide_leaves_between() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-fold-hide-leaves)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (insert "*** A2a\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2" "A2a")))))
      ;; Cycle A: subtree (all visible)
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (org-cycle nil)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Hide leaves
        (org-fold-hide-leaves)
        (let ((v2 (funcall vis)))
          ;; Show all
          (org-fold-show-all)
          (let ((v3 (funcall vis)))
            ;; Cycle A: overview
            (goto-char (point-min))
            (search-forward "A\n")
            (beginning-of-line)
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Cycle A: children
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_multiple_headings_different_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 42 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* L1-A\n")
    (insert "** L2-A\n")
    (insert "*** L3-A\nBody.\n\n")
    (insert "* L1-B\n")
    (insert "** L2-B\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle
                                (invisible-p (point))
                                (org-outline-level))
                          (list needle 'not-found nil))))
                  '("L1-A" "L2-A" "L3-A" "L1-B" "L2-B")))))
      ;; Global cycle: overview
      (org-global-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Global cycle: children
        (org-global-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Global cycle: all
          (org-global-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Local cycle L1-A: overview
            (goto-char (point-min))
            (search-forward "L1-A\n")
            (beginning-of-line)
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Local cycle L1-A: children
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_insert_subtree_between() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 52 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2" "A3")))))
      ;; Cycle A: overview
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle A: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Insert subtree A2
          (goto-char (point-min))
          (search-forward "A1")
          (end-of-line)
          (insert "\n** A2\nBody.\n")
          ;; Cycle A: overview
          (goto-char (point-min))
          (search-forward "A\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle A: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Insert subtree A3
              (goto-char (point-max))
              (insert "** A3\nBody.\n")
              ;; Cycle A: children
              (goto-char (point-min))
              (search-forward "A\n")
              (beginning-of-line)
              (org-cycle nil)
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_delete_subtree_between() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 44 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (insert "** A3\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2" "A3")))))
      ;; Cycle A: overview
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle A: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Delete A2 subtree
          (goto-char (point-min))
          (search-forward "A2")
          (beginning-of-line)
          (org-cut-subtree)
          ;; Cycle A: overview
          (goto-char (point-min))
          (search-forward "A\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle A: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              (list v1 v2 v3 v4
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_copy_subtree_and_cycle_destination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 46 51)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* Source\n")
    (insert "** Item\nBody.\n\n")
    (insert "* Dest\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("Source" "Item" "Dest" "Copy")))))
      ;; Cycle Source: children
      (goto-char (point-min))
      (search-forward "Source\n")
      (beginning-of-line)
      (org-cycle nil)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Copy Item
        (goto-char (point-min))
        (search-forward "Item")
        (beginning-of-line)
        (org-copy-subtree)
        ;; Paste under Dest as Copy
        (goto-char (point-min))
        (search-forward "Dest")
        (end-of-line)
        (insert "\n** Copy\nBody copy.\n")
        ;; Cycle Dest: overview
        (goto-char (point-min))
        (search-forward "Dest\n")
        (beginning-of-line)
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Cycle Dest: children
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            (list v1 v2 v3
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_global_cycle_and_local_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 44 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\n")
    (insert "*** B1a\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "B" "B1" "B1a")))))
      ;; Global cycle: overview
      (org-global-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Local cycle B: children
        (goto-char (point-min))
        (search-forward "B\n")
        (beginning-of-line)
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Local cycle B1: subtree
          (goto-char (point-min))
          (search-forward "B1\n")
          (beginning-of-line)
          (org-cycle nil)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Global cycle: children
            (org-global-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Global cycle: all
              (org-global-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_fold_show_children_between() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 48 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2")))))
      ;; Cycle A: overview
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Show children
        (goto-char (point-min))
        (search-forward "A")
        (beginning-of-line)
        (org-fold-show-children)
        (let ((v2 (funcall vis)))
          ;; Cycle A: overview
          (goto-char (point-min))
          (search-forward "A\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Show children again
            (goto-char (point-min))
            (search-forward "A")
            (beginning-of-line)
            (org-fold-show-children)
            (let ((v4 (funcall vis)))
              ;; Cycle A: subtree
              (org-cycle nil)
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_fold_show_branches_between() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-fold-hide-all)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\n")
    (insert "*** A1a\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A1a" "A2")))))
      ;; Hide all
      (org-fold-hide-all)
      (let ((v1 (funcall vis)))
        ;; Show branches
        (org-fold-show-branches)
        (let ((v2 (funcall vis)))
          ;; Cycle A: overview
          (goto-char (point-min))
          (search-forward "A\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle A: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Show branches again
              (org-fold-show-branches)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_multiple_cycles_different_roots() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 48 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* R1\n")
    (insert "** R1a\nBody.\n\n")
    (insert "* R2\n")
    (insert "** R2a\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("R1" "R1a" "R2" "R2a")))))
      ;; Cycle R1: overview
      (goto-char (point-min))
      (search-forward "R1\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle R2: overview
        (goto-char (point-min))
        (search-forward "R2\n")
        (beginning-of-line)
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Cycle R1: children
          (goto-char (point-min))
          (search-forward "R1\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle R2: children
            (goto-char (point-min))
            (search-forward "R2\n")
            (beginning-of-line)
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Global cycle
              (org-global-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_fold_show_all_from_hidden() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-fold-hide-all)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2")))))
      ;; Hide all
      (org-fold-hide-all)
      (let ((v1 (funcall vis)))
        ;; Cycle A: overview (from hidden)
        (goto-char (point-min))
        (search-forward "A\n")
        (beginning-of-line)
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Cycle A: children (from overview)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Hide all again
            (org-fold-hide-all)
            (let ((v4 (funcall vis)))
              ;; Cycle A: children (from hidden)
              (goto-char (point-min))
              (search-forward "A\n")
              (beginning-of-line)
              (org-cycle nil)
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_fold_hide_subtree_from_children_visible() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2")))))
      ;; Cycle A: children
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Fold subtree (hide all under A)
        (goto-char (point-min))
        (search-forward "A")
        (beginning-of-line)
        (org-fold-subtree)
        (let ((v2 (funcall vis)))
          ;; Show subtree
          (goto-char (point-min))
          (search-forward "A")
          (beginning-of-line)
          (org-fold-show-subtree)
          (let ((v3 (funcall vis)))
            ;; Cycle A: overview
            (goto-char (point-min))
            (search-forward "A\n")
            (beginning-of-line)
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Cycle A: children
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_global_cycle_overview_then_local_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 46 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\n")
    (insert "*** A1a\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A1a" "B" "B1")))))
      ;; Global cycle: overview
      (org-global-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Local cycle A: subtree
        (goto-char (point-min))
        (search-forward "A\n")
        (beginning-of-line)
        (org-cycle nil)
        (org-cycle nil)
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Global cycle: children
          (org-global-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Local cycle A1: subtree
            (goto-char (point-min))
            (search-forward "A1\n")
            (beginning-of-line)
            (org-cycle nil)
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Global cycle: all
              (org-global-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_insert_delete_insert_cycle_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 51 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* P\n")
    (insert "** P1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("P" "P1" "P2" "P3")))))
      ;; Cycle P: overview
      (goto-char (point-min))
      (search-forward "P\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Insert P2
        (goto-char (point-max))
        (insert "** P2\nBody.\n")
        ;; Cycle P: children
        (goto-char (point-min))
        (search-forward "P\n")
        (beginning-of-line)
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Delete P2
          (goto-char (point-min))
          (search-forward "P2")
          (beginning-of-line)
          (org-cut-subtree)
          ;; Insert P3
          (goto-char (point-max))
          (insert "** P3\nBody.\n")
          ;; Cycle P: overview
          (goto-char (point-min))
          (search-forward "P\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle P: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              (list v1 v2 v3 v4
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_fold_hide_drawer_between_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 44 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:Owner: Alice\n:END:\n")
    (insert "Body alpha.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("Alpha" "Effort" "Owner" "Body alpha")))))
      ;; Cycle Alpha: overview
      (goto-char (point-min))
      (search-forward "Alpha")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle Alpha: children (show properties)
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Cycle Alpha: subtree
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Hide drawer
            (goto-char (point-min))
            (search-forward "PROPERTIES")
            (beginning-of-line)
            (org-fold-hide-drawer-all)
            (let ((v4 (funcall vis)))
              ;; Cycle Alpha: overview
              (goto-char (point-min))
              (search-forward "Alpha")
              (beginning-of-line)
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_org_overview_content_all_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"A\" 2) (\"Body A\" 2) (\"A1\" 2) (\"B\" 2) (\"Body B\" 2)) ((\"A\" 2) (\"Body A\" 2) (\"A1\" 2) (\"B\" 2) (\"Body B\" 2)) ((\"A\" 2) (\"Body A\" 2) (\"A1\" 2) (\"B\" 2) (\"Body B\" 2)) ((\"A\" 2) (\"Body A\" 2) (\"A1\" 2) (\"B\" 2) (\"Body B\" 2)) ((\"A\" 2) (\"Body A\" 2) (\"A1\" 2) (\"B\" 2) (\"Body B\" 2)) \"* A\\nBody A.\\n\\n** A1\\nBody A1.\\n\\n* B\\nBody B.\\n\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\nBody A.\n\n")
    (insert "** A1\nBody A1.\n\n")
    (insert "* B\nBody B.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "Body A" "A1" "B" "Body B")))))
      ;; org-overview (same as global cycle overview)
      (goto-char (point-min))
      (org-overview)
      (let ((v1 (funcall vis)))
        ;; org-content (show headings)
        (org-content)
        (let ((v2 (funcall vis)))
          ;; org-all-vis (show all)
          (let ((org-show-all-visibility 'local))
            (org-overview)
            (let ((v3 (funcall vis)))
              ;; Cycle A: overview
              (goto-char (point-min))
              (search-forward "A\n")
              (beginning-of-line)
              (org-cycle nil)
              (let ((v4 (funcall vis)))
                ;; Cycle A: children
                (org-cycle nil)
                (let ((v5 (funcall vis)))
              (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_global_cycle_and_insert_at_each_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 43 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* P\n")
    (insert "** P1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("P" "P1" "P2" "P3" "P4")))))
      ;; Global cycle: overview
      (org-global-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Insert P2
        (goto-char (point-max))
        (insert "** P2\nBody.\n")
        ;; Global cycle: children
        (org-global-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Insert P3
          (goto-char (point-max))
          (insert "** P3\nBody.\n")
          ;; Global cycle: all
          (org-global-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Insert P4
            (goto-char (point-max))
            (insert "** P4\nBody.\n")
            ;; Global cycle: overview
            (org-global-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Global cycle: children
              (org-global-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_local_cycle_insert_local_cycle_again() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 46 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* P\n")
    (insert "** P1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("P" "P1" "P2" "P3")))))
      ;; Local cycle P: overview
      (goto-char (point-min))
      (search-forward "P\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Insert P2
        (goto-char (point-max))
        (insert "** P2\nBody.\n")
        ;; Local cycle P: children
        (goto-char (point-min))
        (search-forward "P\n")
        (beginning-of-line)
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Insert P3
          (goto-char (point-max))
          (insert "** P3\nBody.\n")
          ;; Local cycle P: overview
          (goto-char (point-min))
          (search-forward "P\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Local cycle P: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              (list v1 v2 v3 v4
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_fold_hide_all_global_cycle_hide_all_again() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-fold-hide-all)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "B" "B1")))))
      ;; Hide all
      (org-fold-hide-all)
      (let ((v1 (funcall vis)))
        ;; Global cycle: overview
        (org-global-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Hide all again
          (org-fold-hide-all)
          (let ((v3 (funcall vis)))
            ;; Global cycle: children
            (org-global-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Hide all again
              (org-fold-hide-all)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_show_all_then_global_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 36 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "B" "B1")))))
      ;; Show all
      (org-fold-show-all)
      (let ((v1 (funcall vis)))
        ;; Global cycle: overview
        (org-global-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Show all again
          (org-fold-show-all)
          (let ((v3 (funcall vis)))
            ;; Global cycle: children
            (org-global-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Show all again
              (org-fold-show-all)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_local_cycle_at_different_headings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 52 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "B" "B1")))))
      ;; Cycle A: overview
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle B: overview
        (goto-char (point-min))
        (search-forward "B\n")
        (beginning-of-line)
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Cycle A: children
          (goto-char (point-min))
          (search-forward "A\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle B: children
            (goto-char (point-min))
            (search-forward "B\n")
            (beginning-of-line)
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Cycle A: subtree
              (goto-char (point-min))
              (search-forward "A\n")
              (beginning-of-line)
              (org-cycle nil)
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_global_cycle_three_times_rapid() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"A\" 2) (\"A1\" 2) (\"B\" 2) (\"B1\" 2)) ((\"A\" nil) (\"A1\" 2) (\"B\" 2) (\"B1\" 2)) ((\"A\" nil) (\"A1\" nil) (\"B\" nil) (\"B1\" nil)) ((\"A\" 2) (\"A1\" 2) (\"B\" 2) (\"B1\" 2)) ((\"A\" nil) (\"A1\" 2) (\"B\" 2) (\"B1\" 2)) ((\"A\" nil) (\"A1\" nil) (\"B\" nil) (\"B1\" nil)) \"* A\\n** A1\\nBody.\\n\\n* B\\n** B1\\nBody.\\n\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "B" "B1")))))
      ;; Three rapid global cycles
      (org-global-cycle nil)
      (let ((v1 (funcall vis)))
        (org-global-cycle nil)
        (let ((v2 (funcall vis)))
          (org-global-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Three more rapid global cycles
            (org-global-cycle nil)
            (let ((v4 (funcall vis)))
              (org-global-cycle nil)
              (let ((v5 (funcall vis)))
                (org-global-cycle nil)
                (let ((v6 (funcall vis)))
                  (list v1 v2 v3 v4 v5 v6
                        (buffer-substring-no-properties
                         (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_outline_next_visible_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 45 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2" "B" "B1")))))
      ;; Cycle A: overview
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Navigate: next visible heading from A
        (org-next-visible-heading 1)
        (let ((pos-after-next (line-number-at-pos)))
          ;; Cycle A: children
          (goto-char (point-min))
          (search-forward "A\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v2 (funcall vis)))
            ;; Navigate: next visible heading from A
            (org-next-visible-heading 1)
            (let ((pos-after-next2 (line-number-at-pos)))
              ;; Cycle A: subtree
              (org-cycle nil)
              (org-cycle nil)
              (let ((v3 (funcall vis)))
                (list v1 v2 v3
                      pos-after-next pos-after-next2
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_outline_previous_visible_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 49 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2" "B" "B1")))))
      ;; Start at B1
      (goto-char (point-min))
      (search-forward "B1")
      (beginning-of-line)
      ;; Cycle B: overview
      (goto-char (point-min))
      (search-forward "B\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Navigate: previous visible heading from B1
        (search-forward "B1")
        (beginning-of-line)
        (org-previous-visible-heading 1)
        (let ((pos-after-prev (line-number-at-pos)))
          ;; Cycle B: children
          (goto-char (point-min))
          (search-forward "B\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v2 (funcall vis)))
            ;; Navigate: previous visible heading
            (search-forward "B1")
            (beginning-of-line)
            (org-previous-visible-heading 1)
            (let ((pos-after-prev2 (line-number-at-pos)))
              (list v1 v2
                    pos-after-prev pos-after-prev2
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_fold_hide_other_then_show_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-fold-hide-other)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\nBody A.\n\n")
    (insert "** A1\nBody A1.\n\n")
    (insert "* B\nBody B.\n\n")
    (insert "** B1\nBody B1.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "Body A" "A1" "B" "Body B" "B1")))))
      ;; Cycle A: children
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Fold hide other (hide everything except current subtree)
        (org-fold-hide-other)
        (let ((v2 (funcall vis)))
          ;; Show all
          (org-fold-show-all)
          (let ((v3 (funcall vis)))
            ;; Cycle B: children
            (goto-char (point-min))
            (search-forward "B\n")
            (beginning-of-line)
            (org-cycle nil)
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Fold hide other
              (org-fold-hide-other)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_fold_toggle_children_visibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-fold-hide-children)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* P\n")
    (insert "** P1\nBody.\n\n")
    (insert "** P2\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("P" "P1" "P2")))))
      ;; Show all
      (org-fold-show-all)
      (let ((v1 (funcall vis)))
        ;; Fold hide children
        (goto-char (point-min))
        (search-forward "P\n")
        (beginning-of-line)
        (org-fold-hide-children)
        (let ((v2 (funcall vis)))
          ;; Cycle P: overview
          (goto-char (point-min))
          (search-forward "P\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle P: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Cycle P: subtree
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_fold_show_hidden_subtree_from_overview() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 48 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\n")
    (insert "*** A1a\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A1a" "A2")))))
      ;; Cycle A: overview
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Show subtree of A1
        (goto-char (point-min))
        (search-forward "A1\n")
        (beginning-of-line)
        (org-fold-show-subtree)
        (let ((v2 (funcall vis)))
          ;; Cycle A: overview
          (goto-char (point-min))
          (search-forward "A\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle A: children
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Show subtree of A1
              (goto-char (point-min))
              (search-forward "A1\n")
              (beginning-of-line)
              (org-fold-show-subtree)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_fold_show_entry_from_hidden() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 43 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\nBody A.\n\n")
    (insert "** A1\nBody A1.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "Body A" "A1")))))
      ;; Cycle A: overview
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Hide entry
        (goto-char (point-min))
        (search-forward "A")
        (beginning-of-line)
        (org-fold-hide-entry)
        (let ((v2 (funcall vis)))
          ;; Cycle A: overview
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Show entry
            (goto-char (point-min))
            (search-forward "A")
            (beginning-of-line)
            (org-fold-show-entry)
            (let ((v4 (funcall vis)))
              ;; Cycle A: children
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_multiple_roots_and_global_local_interleave() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 45 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* R1\n")
    (insert "** R1a\nBody.\n\n")
    (insert "* R2\n")
    (insert "** R2a\nBody.\n\n")
    (insert "* R3\n")
    (insert "** R3a\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("R1" "R1a" "R2" "R2a" "R3" "R3a")))))
      ;; Global cycle: overview
      (org-global-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Local cycle R2: children
        (goto-char (point-min))
        (search-forward "R2\n")
        (beginning-of-line)
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Global cycle: children
          (org-global-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Local cycle R3: subtree
            (goto-char (point-min))
            (search-forward "R3\n")
            (beginning-of-line)
            (org-cycle nil)
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Global cycle: all
              (org-global-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_fold_hide_all_then_local_cycle_different_heads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-fold-hide-all)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "B" "B1")))))
      ;; Hide all
      (org-fold-hide-all)
      (let ((v1 (funcall vis)))
        ;; Local cycle A: overview
        (goto-char (point-min))
        (search-forward "A\n")
        (beginning-of-line)
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Local cycle B: overview
          (goto-char (point-min))
          (search-forward "B\n")
          (beginning-of-line)
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Local cycle A: children
            (goto-char (point-min))
            (search-forward "A\n")
            (beginning-of-line)
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Local cycle B: children
              (goto-char (point-min))
              (search-forward "B\n")
              (beginning-of-line)
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_global_cycle_four_times_rapid() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 30 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "B" "B1")))))
      ;; Four rapid global cycles
      (org-global-cycle nil)
      (let ((v1 (funcall vis)))
        (org-global-cycle nil)
        (let ((v2 (funcall vis)))
          (org-global-cycle nil)
          (let ((v3 (funcall vis)))
            (org-global-cycle nil)
            (let ((v4 (funcall vis)))
              (list v1 v2 v3 v4
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_global_cycle_with_three_roots() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 35 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (insert "* C\n")
    (insert "** C1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "B" "B1" "C" "C1")))))
      ;; Global cycle: overview
      (org-global-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Global cycle: children
        (org-global-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Global cycle: all
          (org-global-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Global cycle: overview again
            (org-global-cycle nil)
            (let ((v4 (funcall vis)))
              (list v1 v2 v3 v4
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_local_cycle_overview_children_subtree_overview() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 38 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2")))))
      ;; Cycle A: overview
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Cycle A: children
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Cycle A: subtree
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Cycle A: overview again
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              ;; Cycle A: children again
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_local_cycle_four_times_rapid() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 31 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1")))))
      ;; Four rapid local cycles
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              (list v1 v2 v3 v4
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_local_cycle_five_times_rapid() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 33 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1")))))
      ;; Five rapid local cycles
      (goto-char (point-min))
      (search-forward "A\n")
      (beginning-of-line)
      (org-cycle nil)
      (let ((v1 (funcall vis)))
        (org-cycle nil)
        (let ((v2 (funcall vis)))
          (org-cycle nil)
          (let ((v3 (funcall vis)))
            (org-cycle nil)
            (let ((v4 (funcall vis)))
              (org-cycle nil)
              (let ((v5 (funcall vis)))
                (list v1 v2 v3 v4 v5
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_cycle_with_global_cycle_and_three_roots_with_children() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 38 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\nBody.\n\n")
    (insert "** A2\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\nBody.\n\n")
    (insert "** B2\nBody.\n\n")
    (insert "* C\n")
    (insert "** C1\nBody.\n\n")
    (insert "** C2\nBody.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (if (search-forward needle nil t)
                          (list needle (invisible-p (point)))
                          (list needle 'not-found))))
                  '("A" "A1" "A2" "B" "B1" "B2" "C" "C1" "C2")))))
      ;; Global cycle: overview
      (org-global-cycle nil)
      (let ((v1 (funcall vis)))
        ;; Global cycle: children
        (org-global-cycle nil)
        (let ((v2 (funcall vis)))
          ;; Global cycle: all
          (org-global-cycle nil)
          (let ((v3 (funcall vis)))
            ;; Global cycle: overview again
            (org-global-cycle nil)
            (let ((v4 (funcall vis)))
              (list v1 v2 v3 v4
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}
