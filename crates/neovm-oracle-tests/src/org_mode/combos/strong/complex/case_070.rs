//! Strong combo-complex-70 oracle tests — final probes:
//! org-publish with multiple projects, babel with :var from
//! multiple named tables, org-agenda with org-agenda-get-
//! progress, org-babel with :session for sh, element with
//! org-element-interpret-data for individual link types,
//! org-export with inline-only transcoders, org-timer with
//! org-timer-set-timer, org-cycle with org-cycle-level,
//! and org-macro with org-macro-replace-all on region.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo70_publish_multi_project() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:project-keys (\"web\" \"pdf\" \"all\") :has-components nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-publish)
  (let ((sample
         '(("web" :base-directory "~/org/web" :publishing-directory "~/public/web"
            :publishing-function org-html-publish-to-html)
           ("pdf" :base-directory "~/org/pdf" :publishing-directory "~/public/pdf"
            :publishing-function org-latex-publish-to-pdf)
           ("all" :components ("web" "pdf")))))
    (list
     :project-keys (mapcar #'car sample)
     :has-components (assq :components (cddr (car (last sample))))))
  )"##,
        expect,
    );
}

#[test]
fn combo70_babel_var_multi_named_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((10 30))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+name: t1\n| 10 | 20 |\n\n")
    (insert "#+name: t2\n| 30 | 40 |\n\n")
    (insert "#+begin_src emacs-lisp :results value :var a=t1 :var b=t2\n")
    (insert "(list (car (car a)) (car (car b)))\n")
    (insert "#+end_src\n")
    (let ((r '()))
      (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo70_agenda_get_progress() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:get-progress-fbound t) (:after-stats \"* TODO Task [66%]\\n- [X] a\\n- [ ] b\\n- [X] c\\n\") (:checked 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-agenda)
  (insert "* TODO Task [%]\n- [X] a\n- [ ] b\n- [X] c\n")
  (let ((r '()))
    (push (list :get-progress-fbound (fboundp 'org-agenda-get-progress)) r)
    ;; update stats to get current progress
    (goto-char (point-min))
    (org-update-statistics-cookies t)
    (push (list :after-stats (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; count checked items
    (push (list :checked (length (org-element-map (org-element-parse-buffer) 'item
                                  (lambda (i) (when (equal "X" (org-element-property :checkbox i)) i))))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo70_babel_session_sh() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"ob-sh\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-sh)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src sh :results output :session sh-sess\necho \"init\"\n#+end_src\n\n")
    (insert "#+begin_src sh :results output :session sh-sess\necho \"again\"\n#+end_src\n")
    (let ((r '()))
      (goto-char (point-min))(search-forward "#+begin_src sh")
      (condition-case e
          (push (org-babel-execute-src-block) r)
        (error (push (list :sh1-error (car e)) r)))
      (search-forward "#+begin_src sh")
      (condition-case e
          (push (org-babel-execute-src-block) r)
        (error (push (list :sh2-error (car e)) r)))
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo70_element_interpret_individual_links() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 19 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; link with type+path+raw-link
   (substring-no-properties
    (org-element-interpret-data
     (org-element-create 'link
       '(:type "https" :path "example.com" :raw-link "https://example.com"))))
   ;; file link
   (substring-no-properties
    (org-element-interpret-data
     (org-element-create 'link
       '(:type "file" :path "notes.org" :raw-link "file:notes.org"))))
   ;; internal link
   (substring-no-properties
    (org-element-interpret-data
     (org-element-create 'link
       '(:type "custom-id" :path "target" :raw-link "#target"))))
   )))"##,
        expect,
    );
}

#[test]
fn combo70_timer_set_timer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:set-timer-fbound t :timer-start-fbound t :timer-stop-fbound t :timer-countdown-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-timer)
  (list
   :set-timer-fbound (fboundp 'org-timer-set-timer)
   :timer-start-fbound (fboundp 'org-timer-start)
   :timer-stop-fbound (fboundp 'org-timer-stop)
   :timer-countdown-fbound (boundp 'org-timer-default-timer)
   ))"##,
        expect,
    );
}

#[test]
fn combo70_org_cycle_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:cycle-level-fbound t) (:after-shifttab nil) (:after-show 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** A1\nBody.\n** A2\nBody.\n* B\nBody.\n")
  (let ((r '()))
    (goto-char (point-min))
    ;; org-cycle-level (shift-tab equivalent)
    (push (list :cycle-level-fbound (fboundp 'org-shifttab)) r)
    ;; org-shifttab
    (condition-case nil
        (progn (org-shifttab 1)
               (push (list :after-shifttab (get-char-property (point) 'invisible)) r))
      (error nil))
    ;; show all
    (org-show-all)
    (push (list :after-show (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo70_macro_replace_all_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:replace-error t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: greet Hello, $1\n")
  (insert "{{{greet(Alice)}}} and {{{greet(Bob)}}}.\n")
  (let ((r '()))
    (goto-char (point-min))
    ;; org-macro-replace-all on region
    (condition-case nil
        (progn (org-macro-replace-all org-macro-templates)
               (push (list :after-replace (buffer-string)) r))
      (error (push (list :replace-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo70_org_export_info_for_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 11 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (with-temp-buffer (org-mode)
    (insert "#+TITLE: Info Test\n#+LANGUAGE: en\n#+OPTIONS: num:t\n")
    (let ((info (org-export-get-environment)))
      (list
       :backend (plist-get info :back-end)
       :translate-alist-bound (when (plist-get info :translate-alist) t)
       :export-options (plist-get info :export-options)))))
  )"##,
        expect,
    );
}

#[test]
fn combo70_org_element_at_point_no_context() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:h1-at headline) (:sub-at headline) (:no-ctx-fbound t) (:eob-narrow-at headline) (:eob-at headline))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Head\n** Sub\n** Sub2\n")
  (let ((r '()))
    (goto-char (point-min))
    ;; at-point on each
    (push (list :h1-at (org-element-type (org-element-at-point))) r)
    (search-forward "** Sub") (beginning-of-line)
    (push (list :sub-at (org-element-type (org-element-at-point))) r)
    ;; org-element-at-point-no-context
    (push (list :no-ctx-fbound (fboundp 'org-element-at-point-no-context)) r)
    ;; after narrowing
    (goto-char (point-min))
    (org-narrow-to-subtree)
    (goto-char (point-max))
    (push (list :eob-narrow-at (org-element-type (org-element-at-point))) r)
    (widen)
    (goto-char (point-max))
    (push (list :eob-at (org-element-type (org-element-at-point))) r)
    (nreverse r)))"##,
        expect,
    );
}
