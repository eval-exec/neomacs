//! Combo-strict-16 oracle tests — final edge probes:
//! org-entity-get for all entity types, org-macro-initialize-
//! templates, org-cycle-internal-local, org-return-indent,
//! org-timer-change-times-in-region, org-refile-cache-clear,
//! org-babel-check-confirm-evaluate, org-export-data low-level,
//! org-element-normalize-contents on verse/example/fixed-width.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn strict_entity_get_all_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 24 41)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-entities)
  (list
   ;; Greek
   (list :alpha (nth 6 (org-entity-get "alpha")))
   (list :Alpha (nth 6 (org-entity-get "Alpha")))
   (list :beta (nth 6 (org-entity-get "beta")))
   ;; Math
   (list :sum (nth 6 (org-entity-get "sum")))
   (list :int (nth 6 (org-entity-get "int")))
   (list :pi (nth 6 (org-entity-get "pi")))
   (list :pm (nth 6 (org-entity-get "pm")))
   ;; Arrows
   (list :rarr (nth 6 (org-entity-get "rarr")))
   (list :larr (nth 6 (org-entity-get "larr")))
   (list :harr (nth 6 (org-entity-get "harr")))
   ;; Special
   (list :hellip (nth 6 (org-entity-get "hellip")))
   (list :frac12 (nth 6 (org-entity-get "frac12")))
   ;; Non-existent returns nil
   (list :bogus (org-entity-get "bogus"))
   ;; Entity count
   (list :total (length org-entities)))))"##,
        expect,
    );
}

#[test]
fn strict_macro_initialize_templates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 41)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+MACRO: hello Hello, world!\n")
      (goto-char (point-min))
      ;; org-macro-initialize-templates
      (condition-case nil
          (let ((templates (when (fboundp 'org-macro-initialize-templates)
                             (org-macro-initialize-templates))))
            (list :init-fbound (fboundp 'org-macro-initialize-templates)))
        (error (list :init-error t)))))))"##,
        expect,
    );
}

#[test]
fn strict_cycle_internal_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 16 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* A\nBody A.\n** A1\nBody A1.\n")
      (goto-char (point-min))
      (let ((r '()))
        ;; org-cycle-internal-local
        (push (list :cycle-local-fbound (fboundp 'org-cycle-internal-local)) r)
        ;; fold
        (condition-case nil (org-cycle-internal-local) (error nil))
        (push (list :after-fold-invis (get-char-property (point) 'invisible)) r)
        ;; show all
        (org-show-all)
        (push (list :after-show-headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_return_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 17 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-adapt-indentation t))
    (with-temp-buffer (org-mode)
      (insert "* A\n** B\nBody B.\n")
      (goto-char (point-min))
      (let ((r '()))
        ;; org-return-indent
        (push (list :return-indent-fbound (fboundp 'org-return)) r)
        ;; go to B body and press return
        (search-forward "Body B.")
        (condition-case nil
            (progn (org-return)
                   (push (list :after-return (buffer-string)) r))
          (error (push (list :return-error t) r)))
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_timer_change_times_in_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:change-times-fbound t :timer-start-fbound t :timer-secs-fbound nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-timer)
  (list
   :change-times-fbound (fboundp 'org-timer-change-times-in-region)
   :timer-start-fbound (fboundp 'org-timer-start)
   :timer-secs-fbound (fboundp 'org-timer-secs)
   ))"##,
        expect,
    );
}

#[test]
fn strict_refile_cache_clear() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 9 29)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-refile)
  (list
   :refile-fbound (fboundp 'org-refile)
   :cache-clear-fbound (fboundp 'org-refile-cache-clear)
   ;; try clearing cache
   (condition-case nil
       (progn (org-refile-cache-clear) :cleared)
     (error :clear-error)))))"##,
        expect,
    );
}

#[test]
fn strict_babel_confirm_evaluate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 17 29)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (list
   ;; org-babel-check-confirm-evaluate
   (list :confirm-fbound (fboundp 'org-babel-check-confirm-evaluate))
   ;; org-confirm-babel-evaluate
   (list :confirm-var-bound (boundp 'org-confirm-babel-evaluate))
   ;; try checking
   (condition-case nil
       (when (fboundp 'org-babel-check-confirm-evaluate)
         (with-temp-buffer (org-mode)
           (insert "#+begin_src emacs-lisp\n1\n#+end_src\n")
           (goto-char (point-min))
           (let ((info (org-babel-get-src-block-info)))
             (list :info-p (and info (listp info))))))
     (error :check-error)))))"##,
        expect,
    );
}

#[test]
fn strict_export_data_low_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 19 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text *bold*.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-export-get-environment))
             (para (car (org-element-map tree 'paragraph #'identity)))
             (r '()))
        (push (list :org-export-data-fbound (fboundp 'org-export-data)) r)
        ;; export the paragraph data directly
        (condition-case nil
            (let ((out (when (fboundp 'org-export-data)
                         (org-export-data para info))))
              (push (list :export-data-string (and out (stringp out))) r))
          (error (push (list :export-data-error t) r)))
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_normalize_contents_all_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 19 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Fixed-width
   (org-element-normalize-contents
    '(fixed-width nil "  : line1\n  : line2"))
   ;; Verse block indentation
   (org-element-normalize-contents
    '(verse-block nil "  verse line1\n   verse indented\n\n  verse line2"))
   ;; Example block
   (org-element-normalize-contents
    '(example-block nil "  example1\n    example2\n  example3"))
   ;; Center block
   (org-element-normalize-contents
    '(center-block nil "  centered\n  centered2"))
   ;; Quote block
   (org-element-normalize-contents
    '(quote-block nil "  quoted\n    more quoted\n  back"))
   )))"##,
        expect,
    );
}

#[test]
fn strict_org_prettify_entity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:pretty-fbound t) (:sub-fbound t) (:use-sub-fbound t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; org-pretty-entities
   (list :pretty-fbound (boundp 'org-pretty-entities))
   ;; org-pretty-entities-include-sub-superscripts
   (list :sub-fbound (boundp 'org-pretty-entities-include-sub-superscripts))
   ;; org-use-sub-superscripts
   (list :use-sub-fbound (boundp 'org-use-sub-superscripts))
   ))"##,
        expect,
    );
}
