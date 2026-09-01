use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_org_export_native_filter_options_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-org)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Native Export\n")
    (insert "#+AUTHOR: Ada\n")
    (insert "#+OPTIONS: toc:nil num:nil tags:t todo:t pri:t\n")
    (insert "* TODO [#A] Alpha :work:urgent:\n")
    (insert "SCHEDULED: <2026-05-27 Wed 09:00> DEADLINE: <2026-05-28 Thu>\n")
    (insert ":PROPERTIES:\n:Effort: 1:00\n:END:\n")
    (insert "Paragraph with [fn:one], [[https://example.org][Example]], and =code=.\n")
    (insert "#+ATTR_HTML: :class ignored-in-org-export\n")
    (insert "| / | < | > |\n")
    (insert "|---+---+---|\n")
    (insert "| A | B | C |\n")
    (insert ":LOGBOOK:\n")
    (insert "CLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 10:00] =>  1:00\n")
    (insert ":END:\n")
    (insert "[fn:one] Footnote text.\n")
    (let* ((org-org-with-special-rows nil)
           (exported
            (org-export-as
             'org nil nil t
             '(:with-todo-keywords nil
               :with-tags nil
               :with-priority nil
               :with-drawers ("PROPERTIES" "LOGBOOK")
               :time-stamp-file nil)))
           (tree (with-temp-buffer
                   (org-mode)
                   (insert exported)
                   (org-element-parse-buffer))))
      (list exported
            (org-element-map tree 'headline
              (lambda (h)
                (list (org-element-property :todo-keyword h)
                      (org-element-property :priority h)
                      (org-element-property :raw-value h)
                      (org-element-property :tags h))))
            (org-element-map tree 'node-property
              (lambda (p)
                (cons (org-element-property :key p)
                      (org-element-property :value p))))
            (org-element-map tree 'footnote-definition
              (lambda (f) (org-element-property :label f))))))"##,
        expect,
    );
}

#[test]
fn org_org_export_subtree_body_visible_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (require 'ox-org)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Whole Title\n")
    (insert "* Alpha\n")
    (insert "Visible alpha.\n")
    (insert "** Hidden child\n")
    (insert "Hidden body.\n")
    (insert "* Beta\n")
    (insert ":PROPERTIES:\n:EXPORT_TITLE: Beta Title\n:EXPORT_OPTIONS: toc:nil tags:nil todo:nil\n:END:\n")
    (insert "Beta visible [fn:b].\n")
    (insert "** Beta child :tag:\n")
    (insert "Child body.\n")
    (insert "[fn:b] Beta footnote.\n")
    (goto-char (point-min))
    (search-forward "Hidden child")
    (beginning-of-line)
    (org-fold-hide-subtree)
    (let ((visible-export
           (org-export-as 'org nil t t '(:time-stamp-file nil))))
      (goto-char (point-min))
      (search-forward "* Beta")
      (beginning-of-line)
      (let* ((subtree-export
              (org-export-as 'org t nil t '(:time-stamp-file nil)))
             (parsed (with-temp-buffer
                       (org-mode)
                       (insert subtree-export)
                       (org-element-parse-buffer))))
        (list visible-export
              subtree-export
              (mapcar (lambda (needle)
                        (not (null (string-match-p needle visible-export))))
                      '("Visible alpha" "Hidden child" "Hidden body" "Beta child"))
              (org-element-map parsed 'headline
                (lambda (h)
                  (list (org-element-property :level h)
                        (org-element-property :raw-value h)
                        (org-element-property :tags h))))
              (org-element-map parsed 'footnote-reference
                (lambda (f)
                  (org-element-property :label f)))))))"##,
        expect,
    );
}

#[test]
fn org_org_export_include_macro_custom_link_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"#+macro: word /$1/\\n** Included\\nIncluded text with /inside/.\\n* Local\\nMacro /local/ and [[https://tracker.example/42][org:bug]].\\n\" ((2 \"Included\") (1 \"Local\")) ((\"https\" \"//tracker.example/42\" \"org:bug\")))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox-org)
  (let* ((root (make-temp-file "ox-org" t))
         (inc (expand-file-name "snippet.org" root))
         (old-issue (assoc "issue" org-link-parameters)))
    (unwind-protect
        (progn
          (with-temp-file inc
            (insert "* Included\n")
            (insert "Included text with {{{word(inside)}}}.\n"))
          (org-link-set-parameters
           "issue"
           :export (lambda (path desc backend _info)
                     (format "[[https://tracker.example/%s][%s:%s]]"
                             path backend (or desc path))))
          (with-temp-buffer
            (org-mode)
            (insert "#+TITLE: Include Macro Link\n")
            (insert "#+MACRO: word /$1/\n")
            (insert "#+INCLUDE: \"" inc "\" :minlevel 2\n")
            (insert "* Local\n")
            (insert "Macro {{{word(local)}}} and [[issue:42][bug]].\n")
            (let ((exported
                    (org-export-as
                     'org nil nil t
                     '(:with-toc nil :time-stamp-file nil))))
              (with-temp-buffer
                (org-mode)
                (insert exported)
                (let ((parsed (org-element-parse-buffer)))
                  (list exported
                    (org-element-map parsed 'headline
                      (lambda (h)
                        (list (org-element-property :level h)
                              (org-element-property :raw-value h))))
                    (org-element-map parsed 'link
                      (lambda (l)
                        (list (org-element-property :type l)
                              (org-element-property :path l)
                              (and (org-element-property :contents-begin l)
                                   (buffer-substring-no-properties
                                    (org-element-property :contents-begin l)
                                    (org-element-property :contents-end l))))))))))))
      (if old-issue
          (setcdr old-issue (cdr old-issue))
        (setq org-link-parameters
              (assq-delete-all "issue" org-link-parameters)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_export_org_roundtrip_headline_link_property_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((1 \"TODO\" \"Alpha\" (\"work\")) (2 \"DONE\" \"Beta\" nil) (2 nil \"WAIT Gamma\" nil)) ((\"https\" \"//example.org\")) nil \"#+todo: TODO WAIT | DONE\\n* TODO Alpha                                                           :work:\\nAlpha body with *bold* and /italic/.\\n** DONE Beta\\nBeta body with [[https://example.org][link]].\\n** WAIT Gamma\\nGamma body.\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox-org)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Roundtrip\n")
    (insert "#+AUTHOR: Tester\n")
    (insert "#+TODO: TODO WAIT | DONE\n\n")
    (insert "* TODO Alpha :work:\n")
    (insert "SCHEDULED: <2026-05-27 Wed>\n")
    (insert ":PROPERTIES:\n:Effort: 2:00\n:Owner: Ada\n:END:\n")
    (insert "Alpha body with *bold* and /italic/.\n\n")
    (insert "** DONE Beta\n")
    (insert "CLOSED: [2026-05-26 Mon]\n")
    (insert "Beta body with [[https://example.org][link]].\n\n")
    (insert "** WAIT Gamma\n")
    (insert "Gamma body.\n")
    (let* ((org-export-with-toc nil)
           (exported (org-export-as 'org nil nil t nil)))
      ;; Re-parse exported
      (with-temp-buffer
        (org-mode)
        (insert exported)
        (let* ((tree (org-element-parse-buffer))
               (headlines
                (org-element-map tree 'headline
                  (lambda (h)
                    (list (org-element-property :level h)
                          (org-element-property :todo-keyword h)
                          (org-element-property :raw-value h)
                          (org-element-property :tags h)))))
               (links
                (org-element-map tree 'link
                  (lambda (l)
                    (list (org-element-property :type l)
                          (org-element-property :path l)))))
               (planning
                (org-element-map tree 'planning
                  (lambda (p)
                    (list (and (org-element-property :scheduled p)
                               (org-element-property :raw-value
                                (org-element-property :scheduled p)))
                          (and (org-element-property :closed p)
                               (org-element-property :raw-value
                                (org-element-property :closed p))))))))
          (list headlines
                links
                planning
                exported))))))"##,
        expect,
    );
}
