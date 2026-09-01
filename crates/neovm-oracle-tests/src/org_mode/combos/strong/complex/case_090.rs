use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo90_combined_agenda_map_babel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp)
 (let ((org-confirm-babel-evaluate nil)) (insert "* TODO A :work:\n* TODO B :home:\n* DONE C :work:\n")
  (insert "#+begin_src emacs-lisp :results value :var todos=(org-map-entries (lambda () (org-get-heading t t t t)) \"TODO=\\\"TODO\\\"\")\n(length todos)\n#+end_src\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+begin_src")
   (push (org-babel-execute-src-block) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo90_combined_tag_inline_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* *bold* /italic/ +strike+ _under_ =code= ~verb~ [[link][desc]] :tag1:tag2:\n")
 (let ((r '())) (goto-char (point-min)) (let* ((t (org-element-parse-buffer))
  (h (car (org-element-map t 'headline #'identity)))) (when h
  (push (list :tags (org-element-property :tags h)) r)
  (push (list :raw (substring-no-properties (org-element-property :raw-value h))) r)
  (push (list :inline-count (length (org-element-map h t #'identity))) r))) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo90_combined_fixwidth_verse_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "# Comment line one\n# Comment line two\n:  fixed:width\n:  continued\n")
 (let ((r '())) (let* ((t (org-element-parse-buffer))
  (comments (length (org-element-map t 'comment #'identity)))
  (fw (length (org-element-map t 'fixed-width #'identity))))
  (push (list :comment-lines comments) r) (push (list :fw-lines fw) r)) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo90_combined_hlist_plain_child() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:buffer \"* Parent\\n- [X] task1\\n- [ ] task2\\n  - [X] sub a\\n  - [ ] sub b\\n\") (:items 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* Parent\n- [X] task1\n- [ ] task2\n  - [X] sub a\n  - [ ] sub b\n")
 (let ((r '())) (goto-char (point-min)) (org-update-statistics-cookies t)
  (push (list :buffer (buffer-string)) r) (push (list :items (length (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo90_combined_clock_effort_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:a-effort 90.0) (:b-effort 45.0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-clock) (require 'org-duration)
 (let ((org-clock-persist nil)) (insert "* A\n:PROPERTIES:\n:EFFORT:   1:30\n:END:\n* B\n:PROPERTIES:\n:EFFORT:   0:45\n:END:\n")
  (goto-char (point-min)) (org-clock-in nil) (org-clock-out nil nil)
  (search-forward "* B") (beginning-of-line) (org-clock-in nil) (org-clock-out nil nil)
  (let ((r '())) (goto-char (point-min))
   (push (list :a-effort (org-duration-to-minutes (or (org-entry-get nil "EFFORT") "0:00"))) r)
   (search-forward "* B") (beginning-of-line)
   (push (list :b-effort (org-duration-to-minutes (or (org-entry-get nil "EFFORT") "0:00"))) r)
   (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo90_combined_adopt_interpret_parse_verify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-element)
 (insert "* P\n* Q\n") (let ((r '())) (let* ((t (org-element-parse-buffer))
  (P (car (org-element-map t 'headline (lambda (h) (when (equal "P" (org-element-property :raw-value h)) h)))))
  (new (org-element-create 'headline '(:level 2 :raw-value "NewKid")))
  (r2 '())) (org-element-adopt-element P new)
  (push (list :p-kids (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
   (org-element-map P 'headline #'identity))) r)
  (let ((i (substring-no-properties (org-element-interpret-data t))))
   (push (list :has-NewKid (string-match-p "NewKid" i)) r))
  (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo90_combined_prop_drawer_override_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:l2-X \"100\") (:l2-Y \"20\") (:l3-X-inherit \"10 100\") (:l3-Y-inherit \"20\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "#+PROPERTY: X 1\n#+PROPERTY: Y 2\n* L1\n:PROPERTIES:\n:X: 10\n:END:\n** L2\n:PROPERTIES:\n:X+: 100\n:Y: 20\n:END:\n*** L3\n")
 (let ((r '())) (goto-char (point-min))
  (search-forward "** L2") (beginning-of-line)
  (push (list :l2-X (org-entry-get nil "X")) r) (push (list :l2-Y (org-entry-get nil "Y")) r)
  (search-forward "*** L3") (beginning-of-line)
  (push (list :l3-X-inherit (org-entry-get nil "X" t)) r) (push (list :l3-Y-inherit (org-entry-get nil "Y" t)) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo90_combined_sparse_export_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox-ascii)
 (let ((org-export-show-temporary-export-buffer nil)) (insert "* Z\n* A\n* M\n")
  (goto-char (point-min)) (org-sort-entries nil ?a)
  (let ((r '())) (let ((out (org-export-as 'ascii nil nil t)))
   (push (list :export-ok (> (length out) 0)) r)) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo90_combined_babel_session_3block_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"ob-emacs-lisp backend does not support sessions\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp)
 (let ((org-confirm-babel-evaluate nil))
  (insert "#+begin_src emacs-lisp :results value :session s90\n(setq s90-x 7)\n#+end_src\n\n")
  (insert "#+begin_src emacs-lisp :results value :session s90\n(setq s90-y (* s90-x 2))\n#+end_src\n\n")
  (insert "#+begin_src emacs-lisp :results value :session s90\n(+ s90-x s90-y)\n#+end_src\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+begin_src") (org-babel-execute-src-block)
   (search-forward "#+begin_src") (org-babel-execute-src-block)
   (search-forward "#+begin_src") (push (org-babel-execute-src-block) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo90_combined_todo_depend_checkbox_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:child-todo #(\"DONE\" 0 4 (org-todo-head \"TODO\"))) (:parent-todo #(\"DONE\" 0 4 (org-todo-head \"TODO\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (let ((org-enforce-todo-dependencies t)) (insert "* TODO Parent\n** TODO Child\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "** TODO Child") (beginning-of-line)
   (condition-case nil (progn (org-todo "DONE") (push (list :child-todo (org-get-todo-state)) r))
    (error (push (list :child-blocked t) r)))
   (goto-char (point-min)) (org-todo "DONE")
   (push (list :parent-todo (org-get-todo-state)) r) (nreverse r))))"##,
        expect,
    );
}
