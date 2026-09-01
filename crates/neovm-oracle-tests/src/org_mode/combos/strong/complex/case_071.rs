//! Strong combo-complex-71/72 — extreme probes: org-babel-load-
//! in-session, org-export-dispatch, org-agenda-to-appt,
//! org-element-cache-sync, org-property-values, org-indent-
//! refresh-maybe, org-timer-item-repeat, org-cycle-include-lists
//! integrate, org-babel with :results org-indent, and
//! org-footnote-all-labels cross-check.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo71_babel_load_in_session() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:load-in-session-fbound t :load-file-fbound t :lob-ingest-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (list
   :load-in-session-fbound (fboundp 'org-babel-load-in-session)
   :load-file-fbound (fboundp 'org-babel-load-file)
   :lob-ingest-fbound (fboundp 'org-babel-lob-ingest)
   ))"##,
        expect,
    );
}

#[test]
fn combo71_agenda_to_appt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:to-appt-fbound t :appt-time-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-agenda)
  (list
   :to-appt-fbound (fboundp 'org-agenda-to-appt)
   :appt-time-fbound (fboundp 'org-agenda-todayp)
   ))"##,
        expect,
    );
}

#[test]
fn combo71_element_cache_sync() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:sync-fbound t) (:cache-active-fbound t) (:headlines 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-element)
  (insert "* A\n** B\n** C\n")
  (let ((r '()))
    (push (list :sync-fbound (fboundp 'org-element--cache-sync)) r)
    (push (list :cache-active-fbound (boundp 'org-element-use-cache)) r)
    ;; parse, modify, sync, reparse
    (org-element-cache-reset)
    (goto-char (point-max))
    (insert "\n** D\n")
    (when (fboundp 'org-element--cache-sync)
      (condition-case nil (org-element--cache-sync (current-buffer)) (error nil)))
    (push (list :headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo71_property_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:values-fbound t) (:colors (\"red\" \"blue\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n:PROPERTIES:\n:COLOR: red\n:END:\n")
  (insert "* B\n:PROPERTIES:\n:COLOR: blue\n:END:\n")
  (insert "* C\n:PROPERTIES:\n:COLOR: red\n:END:\n")
  (let ((r '()))
    ;; org-property-values
    (push (list :values-fbound (fboundp 'org-property-values)) r)
    (condition-case nil
        (when (fboundp 'org-property-values)
          (let ((vals (org-property-values "COLOR")))
            (push (list :colors vals) r)))
      (error nil))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo71_indent_refresh() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:refresh-fbound t :indent-fbound t :add-prop-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-indent)
  (list
   :refresh-fbound (fboundp 'org-indent-refresh-maybe)
   :indent-fbound (fboundp 'org-indent-mode)
   :add-prop-fbound (fboundp 'org-indent-add-properties)
   ))"##,
        expect,
    );
}

#[test]
fn combo71_timer_item_repeat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:item-fbound t :pause-fbound t :change-times-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-timer)
  (list
   :item-fbound (fboundp 'org-timer-item)
   :pause-fbound (fboundp 'org-timer-pause-or-continue)
   :change-times-fbound (fboundp 'org-timer-change-times-in-region)
   ))"##,
        expect,
    );
}

#[test]
fn combo71_cycle_include_lists_integrate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:after-fold nil) (:vis-items 0) (:all-items 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (let ((org-cycle-include-plain-lists 'integrate))
    (insert "* H\n- item 1\n- item 2\n  - sub a\n  - sub b\n")
    (let ((r '()))
      (goto-char (point-min))
      ;; TAB should fold heading+list together
      (org-cycle)
      (push (list :after-fold (get-char-property (point) 'invisible)) r)
      ;; parse visible
      (push (list :vis-items (length (org-element-map (org-element-parse-buffer nil t) 'item #'identity))) r)
      (org-show-all)
      (push (list :all-items (length (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo71_babel_results_org_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (((\"a\" \"b\") (\"c\" \"d\")) (:table-count 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src emacs-lisp :results value\n'((\"a\" \"b\") (\"c\" \"d\"))\n#+end_src\n")
    (let ((r '()))
      (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      (push (list :table-count (length (org-element-map (org-element-parse-buffer) 'table #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo71_footnote_all_labels_crosscheck() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:ref-labels (\"1\" \"2\" \"3\")) (:def-labels (\"1\" \"2\" \"3\")) (:unique-ref-labels nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "A[fn:1] B[fn:2][fn:3] C[fn:1 again]\n")
  (insert "[fn:1] One.\n[fn:2] Two.\n[fn:3] Three.\n")
  (let ((r '()))
    ;; all footnote-reference labels
    (push (list :ref-labels (mapcar (lambda (fr) (org-element-property :label fr))
                                    (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
    ;; all footnote-definition labels
    (push (list :def-labels (mapcar (lambda (fd) (org-element-property :label fd))
                                    (org-element-map (org-element-parse-buffer) 'footnote-definition #'identity))) r)
    ;; unique labels in refs
    (push (list :unique-ref-labels
                (sort (delete-dups (copy-sequence (plist-get (car r) :ref-labels))) #'string-lessp)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo71_export_dispatch_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:dispatch-fbound t :backends (odt latex icalendar html ascii) :backend-count 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (list
   :dispatch-fbound (fboundp 'org-export-dispatch)
   :backends (mapcar #'org-export-backend-name org-export-registered-backends)
   :backend-count (length org-export-registered-backends)
   ))"##,
        expect,
    );
}
