use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo78_agenda_holidays_and_diary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:include-diary-fbound t :include-holidays-fbound nil :diary-file-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :include-diary-fbound (boundp 'org-agenda-include-diary)
 :include-holidays-fbound (boundp 'calendar-holidays)
 :diary-file-fbound (boundp 'diary-file)
 ))"##,
        expect,
    );
}
#[test]
fn combo78_babel_ob_haskell_awk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ob-haskell ob-haskell :ob-awk ob-awk :ob-clojure ob-clojure :ob-groovy ob-groovy :ob-lua ob-lua)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (list
 :ob-haskell (condition-case nil (require 'ob-haskell) (error (featurep 'ob-haskell)))
 :ob-awk (condition-case nil (require 'ob-awk) (error (featurep 'ob-awk)))
 :ob-clojure (condition-case nil (require 'ob-clojure) (error (featurep 'ob-clojure)))
 :ob-groovy (condition-case nil (require 'ob-groovy) (error (featurep 'ob-groovy)))
 :ob-lua (condition-case nil (require 'ob-lua) (error (featurep 'ob-lua)))
 ))"##,
        expect,
    );
}
#[test]
fn combo78_element_interp_babel_call_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument characterp quote)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-element)
 (let* ((call (org-element-create 'babel-call '(:call "doubler(x=10)" :end-header '(:results . "raw"))))
        (s (substring-no-properties (org-element-interpret-data call)))
        (r '()))
  (push (list :call-str s) r)
  (push (list :is-babel-call (string-match-p "#\\+CALL" s)) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo78_org_column_view_compute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:columns-fbound t) (:compute-fbound t) (:get-format-fbound t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-colview)
 (insert "* A\n:PROPERTIES:\n:EFFORT:   1:00\n:END:\n* B\n:PROPERTIES:\n:EFFORT:   2:00\n:END:\n")
 (let ((r '())) (goto-char (point-min))
  (push (list :columns-fbound (fboundp 'org-columns)) r)
  (push (list :compute-fbound (fboundp 'org-columns-compute)) r)
  (push (list :get-format-fbound (fboundp 'org-columns-get-format)) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo78_org_reveal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:after-reveal-invis nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "* A\n** B\n*** C\nBody.\n")
 (let ((r '())) (goto-char (point-min))
  (org-overview)
  (search-forward "*** C") (beginning-of-line)
  (condition-case nil (org-reveal) (error nil))
  (push (list :after-reveal-invis (get-char-property (point) 'invisible)) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo78_agenda_log_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:log-mode-fbound t :clockreport-mode-fbound t :follow-mode-fbound t :sticky-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :log-mode-fbound (fboundp 'org-agenda-log-mode)
 :clockreport-mode-fbound (fboundp 'org-agenda-clockreport-mode)
 :follow-mode-fbound (fboundp 'org-agenda-follow-mode)
 :sticky-fbound (boundp 'org-agenda-sticky)
 ))"##,
        expect,
    );
}
#[test]
fn combo78_org_edit_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:edit-special-fbound t :ctrl-c-ctrl-c-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (list
 :edit-special-fbound (fboundp 'org-edit-special)
 :ctrl-c-ctrl-c-fbound (fboundp 'org-ctrl-c-ctrl-c)
 ))"##,
        expect,
    );
}
#[test]
fn combo78_org_babel_tangle_body_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer (org-mode) (require 'ob-tangle)
 (let ((tmpfile (make-temp-file "tangle-" nil ".el")))
  (insert (format "#+begin_src emacs-lisp :tangle %s\n(message \"hi\")\n#+end_src\n" tmpfile))
  (let ((r '())) (goto-char (point-min))
   (condition-case nil (progn (org-babel-tangle) (push (list :tangled t) r)
    (push (list :file-exists (file-exists-p tmpfile)) r)) (error (push (list :error t) r)))
   (condition-case nil (delete-file tmpfile) (error nil))
   (nreverse r))))"##,
    );
}
#[test]
fn combo78_agenda_prepare_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:prepare-buffers-fbound t :finalize-fbound t :new-builder-fbound nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :prepare-buffers-fbound (fboundp 'org-agenda-prepare-buffers)
 :finalize-fbound (fboundp 'org-agenda-finalize)
 :new-builder-fbound (fboundp 'org-agenda-new-builder)
 ))"##,
        expect,
    );
}
#[test]
fn combo78_org_src_edit_exit_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:edit-src-fbound t) (:lang-fbound t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-src)
 (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n")
 (let ((r '())) (goto-char (point-min))
  (push (list :edit-src-fbound (fboundp 'org-edit-src-code)) r)
  (push (list :lang-fbound (fboundp 'org-src-get-lang-mode)) r)
  (nreverse r)))"##,
        expect,
    );
}
