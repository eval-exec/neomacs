use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_link_doi_info_man_export_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"<a href=\\\"https://doi.example/10.1000/foo bar\\\">A DOI</a>\" \"\\\\href{https://doi.example/10.1000/foo bar}{A DOI}\" \"[A DOI] (<https://doi.example/10.1000/foo bar>)\" \"@uref{https://doi.example/10.1000/foo bar, A DOI}\" \"https://doi.example/10.1000/foo bar\") (\"<a href=\\\"https://www.gnu.org/software/emacs/manual/html_mono/elisp.html#Non_002dASCII-in-Strings\\\">Strings</a>\" \"@ref{Non-ASCII in Strings,Strings,,elisp,}\" nil) (\"<a target=\\\"_blank\\\" href=\\\"http://man.he.net/?topic=printf(3)::format&section=all\\\">printf</a>\" \"\\\\href{http://man.he.net/?topic=printf(3)::format&section=all}{printf}\" \"@uref{http://man.he.net/?topic=printf(3)::format&section=all,printf}\" \"[printf] (<http://man.he.net/?topic=printf(3)::format&section=all>)\" \"[printf](http://man.he.net/?topic=printf(3)::format&section=all)\" \"http://man.he.net/?topic=printf(3)::format&section=all\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ol-doi)
  (require 'ol-info)
  (require 'ol-man)
  (let ((org-link-doi-server-url "https://doi.example/"))
    (list
     (mapcar (lambda (backend)
               (org-link-doi-export "10.1000/foo bar" "A DOI" backend
                                    '(:ascii-links-to-notes nil)))
             '(html latex ascii texinfo md))
     (mapcar (lambda (backend)
               (org-info-export "elisp#Non-ASCII in Strings" "Strings" backend))
             '(html texinfo ascii))
     (mapcar (lambda (backend)
               (org-man-export "printf(3)::format" "printf" backend))
             '(html latex texinfo ascii md org)))))"##,
        expect,
    );
}

#[test]
fn org_info_link_file_node_description_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"dir\" . \"Top\") (\"dir\" . \"Top\") (\"emacs\" . \"Top\") (\"elisp\" . \"Non-ASCII in Strings\") (\"org\" . \"Tables\") (\"info\" . \"Special Node\")) (\"Top\" \"Non_002dASCII-in-Strings\" \"g_t1_002e2-Weird_002fNode\" \"spaced\") (\"info \\\"(dir)\\\"\" \"info elisp\" \"info \\\"(elisp) Non-ASCII in Strings\\\"\" \"Desc\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ol-info)
  (list
   (mapcar #'org-info--link-file-node
           '(nil "" "emacs" "elisp#Non-ASCII in Strings"
                 "org:Tables" "info#:Special Node"))
   (mapcar #'org-info--expand-node-name
           '("Top" "Non-ASCII in Strings" "1.2 Weird/Node" "  spaced  "))
   (mapcar (lambda (pair)
             (org-info-description-as-command (car pair) (cdr pair)))
           '(("info:dir" . nil)
             ("info:elisp" . "")
             ("info:elisp#Non-ASCII in Strings" . nil)
             ("https://example.org" . "Desc")))))"##,
        expect,
    );
}

#[test]
fn org_info_man_store_link_context_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ol)
  (require 'ol-info)
  (require 'ol-man)
  (let ((org-store-link-plist nil))
    (with-temp-buffer
      (setq major-mode 'Info-mode
            Info-current-file "/usr/share/info/elisp.info"
            Info-current-node "Symbols")
      (let ((info-link (org-info-store-link)))
        (let ((info-plist org-store-link-plist))
          (setq org-store-link-plist nil)
          (with-temp-buffer
            (rename-buffer "*Man printf*")
            (setq major-mode 'Man-mode)
            (let ((man-link (org-man-store-link)))
              (list info-link
                    info-plist
                    man-link
                    org-store-link-plist
                     (org-man-get-page-name))))))))"##,
        expect,
    );
}

#[test]
fn org_link_abbrev_custom_follow_export_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable followed)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ol)
  (let* ((followed nil)
         (org-link-parameters
          (cons '("ticket" :follow (lambda (path arg)
                                     (push (list path arg) followed)))
                org-link-parameters)))
    (with-temp-buffer
      (org-mode)
      ;; Define abbreviations
      (setq org-link-abbrev-alist
            '(("gh" . "https://github.com/%s")
              ("rfc" . "https://www.rfc-editor.org/rfc/rfc%s.txt")
              ("doi" . "https://doi.org/%s")))
      (insert "GitHub: [[gh:eval-exec/neomacs][Neomacs]].\n")
      (insert "RFC: [[rfc:9110][HTTP]].\n")
      (insert "DOI: [[doi:10.1000/example][Paper]].\n")
      (insert "Ticket: [[ticket:ABC-123][Bug]].\n\n")
      ;; Parse links
      (let* ((tree (org-element-parse-buffer))
             (links
              (org-element-map tree 'link
                (lambda (lk)
                  (list (org-element-property :type lk)
                        (org-element-property :path lk)
                        (org-element-property :raw-link lk)
                        (and (org-element-contents-begin lk)
                             (substring-no-properties
                              (buffer-substring-no-properties
                               (org-element-contents-begin lk)
                               (org-element-contents-end lk))))))))
             ;; Expand abbreviations
             (expanded-gh (org-link-expand-abbrev "gh:user/repo"))
             (expanded-rfc (org-link-expand-abbrev "rfc:9110"))
             ;; Open ticket
             (_ (org-link-open-from-string "[[ticket:ABC-123]]"))
             ;; Export
             (html (org-export-as 'html nil nil t '(:with-toc nil)))
             (has-github (string-match-p "github.com/eval-exec/neomacs" html))
             (has-rfc (string-match-p "rfc-editor.org/rfc/rfc9110" html)))
        (list links
              expanded-gh
              expanded-rfc
              followed
              has-github
              has-rfc))))))"##,
        expect,
    );
}
