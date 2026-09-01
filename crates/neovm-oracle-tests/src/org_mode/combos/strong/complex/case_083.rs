use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo83_babel_noweb_prefix_suffix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (100)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp)
 (let ((org-confirm-babel-evaluate nil))
  (insert "#+name: shared\n#+begin_src emacs-lisp\n(setq v 42)\n#+end_src\n\n")
  (insert "#+begin_src emacs-lisp :results value :noweb yes\n<<shared>>\n(+ v 58)\n#+end_src\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+name: shared") (search-forward "#+begin_src emacs-lisp")
   (org-babel-execute-src-block) (search-forward "#+begin_src emacs-lisp")
   (push (org-babel-execute-src-block) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo83_export_inline_task() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox-ascii)
 (let ((org-export-show-temporary-export-buffer nil) (org-inlinetask-min-level 15))
  (insert "*************** TODO Inline Task\nInline body.\n*************** END\n") (let ((r '()))
   (let ((out (org-export-as 'ascii nil nil t))) (push (list :ok (> (length out) 0)) r)) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo83_org_babel_read_header_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:read-fbound t :combine-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list
 :read-fbound (fboundp 'org-babel-parse-header-arguments) :combine-fbound (fboundp 'org-babel-merge-params)))"##,
        expect,
    );
}
#[test]
fn combo83_agenda_sticky_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:sticky-fbound nil :bulk-fbound t :set-effort-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :sticky-fbound (fboundp 'org-agenda-toggle-sticky-mode) :bulk-fbound (fboundp 'org-agenda-bulk-action)
 :set-effort-fbound (fboundp 'org-agenda-set-effort)))"##,
        expect,
    );
}
#[test]
fn combo83_element_cache_before_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:heads1 3) (:heads2 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-element)
 (insert "* A\n** B\n** C\n") (let ((r '())) (org-element-cache-reset)
  (push (list :heads1 (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  (goto-char (point-max)) (insert "\n** D\n") (org-element-cache-reset)
  (push (list :heads2 (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo83_org_yank_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:after-yank (\"A\" \"B\" \"C** B\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n** B\nBody.\n* C\n") (let ((r '())) (goto-char (point-min))
  (search-forward "** B") (beginning-of-line) (org-copy-subtree)
  (search-forward "* C") (end-of-line) (org-yank)
  (push (list :after-yank (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
    (org-element-map (org-element-parse-buffer) 'headline #'identity))) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo83_org_table_formula_string_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "| name  | len |\n|-------+-----|\n| Alice |     |\n| Bob   |     |\n")
 (insert "#+TBLFM: $2='(length $1)\n") (let ((r '())) (goto-char (point-min))
  (condition-case e (progn (org-table-recalculate t) (org-table-align) (push (list :ok t) r)) (error (push (list :err (car e)) r)))
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo83_org_struct_template_try() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:err (:buffer \"<e\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "<e") (let ((r '())) (condition-case nil (org-try-structure-completion)
  (error (push :err r))) (push (list :buffer (buffer-string)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo83_org_export_block_fixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "#+BEGIN_EXPORT latex\n\\textbf{bold}\n#+END_EXPORT\n")
 (let ((r '())) (let* ((t (org-element-parse-buffer)) (eb (car (org-element-map t 'export-block #'identity))))
  (when eb (push (list :type (org-element-property :type eb)) r) (push (list :value (org-element-property :value eb)) r)))
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo83_agenda_date_prompt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:date-fbound t :get-date-fbound t :time-span-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :date-fbound (fboundp 'org-agenda-date-prompt) :get-date-fbound (fboundp 'org-read-date)
 :time-span-fbound (boundp 'org-agenda-current-span)))"##,
        expect,
    );
}
