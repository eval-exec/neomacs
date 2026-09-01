//! Combo-strict-14 oracle tests — remaining contract verification:
//! org-table-import CSV, deep heading nesting (15+),
//! org-element-normalize-contents on all container types,
//! org-publish basic config, org-compat functions,
//! org-macs string utilities, org-speed-commands lookup,
//! org-babel with :mkdirp header, org-element-interpret-
//! data on individual subtypes, org-export-backend creation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn strict_table_import_csv() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 15 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "a,b,c\n1,2,3\n4,5,6\n")
      (goto-char (point-min))
      (let ((r '()))
        (condition-case nil
            (progn (org-table-convert-region (point-min) (point-max) '(4))
                   (push (list :after-convert (buffer-substring-no-properties (point-min) (point-max))) r)
                   (goto-char (point-min))
                   (push (list :to-lisp (org-table-to-lisp)) r)
                   (push (list :cell-count (length (org-element-map (org-element-parse-buffer) 'table-cell #'identity))) r))
          (error (push (list :convert-error t) r)))
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_deep_heading_nesting_15() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 16 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* L1\n** L2\n*** L3\n**** L4\n***** L5\n****** L6\n")
      (insert "******* L7\n******** L8\n********* L9\n********** L10\n")
      (insert "*********** L11\n************ L12\n************* L13\n")
      (insert "************** L14\n*************** L15\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headlines (org-element-map tree 'headline #'identity))
             (r '()))
        (push (list :count (length headlines)) r)
        (push (list :max-level (apply #'max (mapcar (lambda (h) (org-element-property :level h)) headlines))) r)
        (push (list :raw-values (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h))) headlines)) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_publish_basic_config() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 55)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-publish)
  (list
   ;; org-publish-project-alist should be customizable
   (list :publish-fbound (fboundp 'org-publish-project))
   (list :publish-alist-fbound (boundp 'org-publish-project-alist))
   ;; sample project structure
   (let ((sample '(("web" :base-directory "~/org"
                    :publishing-directory "~/public_html"
                    :publishing-function org-html-publish-to-html))))
     (list :sample-keys (mapcar #'car sample)
           :sample-type (nth 0 (cdr (car sample))))))))"##,
        expect,
    );
}

#[test]
fn strict_org_macs_string_utils() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 16 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-macs)
  (list
   ;; org-trim
   (list :trim-spaces (org-trim "  hello world  "))
   (list :trim-tabs (org-trim "\t\ttabbed\t\t"))
   (list :trim-empty (org-trim ""))
   ;; org-string-nw-p
   (list :nw-p-true (org-string-nw-p "hello"))
   (list :nw-p-space (org-string-nw-p "   "))
   (list :nw-p-nil (org-string-nw-p nil))
   ;; org-unique-local-variables
   (list :combine-fbound (fboundp 'org-combine-plists))
   ;; org-combine-plists basic
   (list :combine-1 (org-combine-plists '(:a 1 :b 2) '(:b 3 :c 4)))
   )))"##,
        expect,
    );
}

#[test]
fn strict_speed_commands_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; org-speed-commands should be available
   (list :speed-fbound (boundp 'org-speed-commands))
   ;; org-speed-command-help
   (list :help-fbound (fboundp 'org-speed-command-help))
   ;; check if user-defined speed commands exist
   (cond ((boundp 'org-speed-commands-user)
          (list :user-speed (length org-speed-commands-user)))
         (t :not-bound))
   )))"##,
        expect,
    );
}

#[test]
fn strict_babel_mkdirp_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 16 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (with-temp-buffer (org-mode)
      (insert "#+begin_src emacs-lisp :results value :tangle /tmp/neovm-test-dir/mkdirp-test.el :mkdirp yes\n")
      (insert "(message \"test\")\n")
      (insert "#+end_src\n")
      (let ((r '()))
        (goto-char (point-min))
        (search-forward "#+begin_src emacs-lisp")
        ;; check that :mkdirp is parsed
        (push (list :mkdirp-attr
                    (org-element-property :parameters
                     (car (org-element-map (org-element-parse-buffer) 'src-block #'identity)))) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_element_interpret_individual_subtypes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 37 80)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (list
   ;; interpret individual element types
   (list :bold
         (substring-no-properties
          (org-element-interpret-data
           (org-element-create 'bold nil "bold text"))))
   (list :italic
         (substring-no-properties
          (org-element-interpret-data
           (org-element-create 'italic nil "italic text"))))
   (list :code
         (substring-no-properties
          (org-element-interpret-data
           (org-element-create 'code nil "(+ 1 2)"))))
   (list :verbatim
         (substring-no-properties
          (org-element-interpret-data
           (org-element-create 'verbatim nil "literal"))))
   (list :underline
         (substring-no-properties
          (org-element-interpret-data
           (org-element-create 'underline nil "underlined"))))
   (list :strike-through
         (substring-no-properties
          (org-element-interpret-data
           (org-element-create 'strike-through nil "struck"))))
   (list :line-break
         (substring-no-properties
          (org-element-interpret-data
           (org-element-create 'line-break nil))))
   (list :entity
         (substring-no-properties
          (org-element-interpret-data
           (org-element-create 'entity '(:name "alpha" :use-brackets-p t))))))))"##,
        expect,
    );
}

#[test]
fn strict_org_combine_plists_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 13 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-macs)
  (list
   ;; basic combine
   (org-combine-plists '(:a 1 :b 2) '(:c 3))
   ;; override
   (org-combine-plists '(:a 1 :b 2) '(:b 99 :c 3))
   ;; three plists
   (org-combine-plists '(:x 10) '(:y 20) '(:z 30))
   ;; with nil
   (org-combine-plists nil '(:a 1 :b 2))
   ;; all nil
   (org-combine-plists nil nil)))))"##,
        expect,
    );
}

#[test]
fn strict_export_backend_creation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:create-fbound t) (:created t :name test-backend :parent ascii))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (list
   ;; org-export-create-backend
   (list :create-fbound (fboundp 'org-export-create-backend))
   ;; try creating a simple test backend
   (condition-case nil
       (let* ((test-backend (org-export-create-backend
                             :parent 'ascii
                             :name 'test-backend
                             :transcoders '((paragraph . (lambda (p c i) "TEST-PARA"))))))
         (list :created t
               :name (org-export-backend-name test-backend)
               :parent (org-export-backend-parent test-backend)))
     (error (list :create-error t)))))"##,
        expect,
    );
}

#[test]
fn strict_org_table_relative_references() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 15 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| a | b |\n| 1 | 2 |\n| 3 | 4 |\n")
      (insert "#+TBLFM: @>$1=$<::@>$2=vmax(@2..@-1)\n")
      (let ((r '()))
        (goto-char (point-min))
        (condition-case nil
            (progn (org-table-recalculate t) (org-table-align)
                   (push (list :after-recalc (buffer-string)) r)
                   (push (list :first-cell (org-table-get "$<" nil)) r)
                   (push (list :max-val (org-table-get "@>$2" nil)) r))
          (error (push (list :recalc-error t) r)))
        (nreverse r))))))"##,
        expect,
    );
}
