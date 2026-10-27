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

    let expect = expect_test::expect![[
        r#""OK (\"info:elisp.info#Symbols\" (:type \"info\" :file \"/usr/share/info/elisp.info\" :node \"Symbols\" :link \"info:elisp.info#Symbols\" :description \"elisp.info#Symbols\") (:type \"man\" :link \"man:printf\" :description \"Manpage for printf\") (:type \"man\" :link \"man:printf\" :description \"Manpage for printf\") \"printf\")""#
    ]];
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
                     (org-man-get-page-name)))))))))"##,
        expect,
    );
}
