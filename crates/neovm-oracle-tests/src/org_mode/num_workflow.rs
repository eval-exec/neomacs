use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_num_skip_property_comment_tags_update_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"* Alpha\\n** Beta\\n*** Leaf\\n** COMMENT Hidden\\n*** Hidden child\\n** Tagged :skip:\\n*** Tagged child\\n** Property\\n:PROPERTIES:\\n:UNNUMBERED: t\\n:END:\\n*** Property child\\n* Omega\\n\" 1 nil \"[1] \" bold) (\"** Beta\\n*** Leaf\\n** COMMENT Hidden\\n*** Hidden child\\n** Tagged :skip:\\n*** Tagged child\\n** Property\\n:PROPERTIES:\\n:UNNUMBERED: t\\n:END:\\n*** Property child\\n* Omega\\n\" 2 nil \"[1.1] \" bold) (\"*** Leaf\\n** COMMENT Hidden\\n*** Hidden child\\n** Tagged :skip:\\n*** Tagged child\\n** Property\\n:PROPERTIES:\\n:UNNUMBERED: t\\n:END:\\n*** Property child\\n* Omega\\n\" 3 nil \"[1.1.1] \" bold) (\"** COMMENT Hidden\\n*** Hidden child\\n** Tagged :skip:\\n*** Tagged child\\n** Property\\n:PROPERTIES:\\n:UNNUMBERED: t\\n:END:\\n*** Property child\\n* Omega\\n\" 2 t \"\" nil) (\"*** Hidden child\\n** Tagged :skip:\\n*** Tagged child\\n** Property\\n:PROPERTIES:\\n:UNNUMBERED: t\\n:END:\\n*** Property child\\n* Omega\\n\" 3 nil \"\" nil) (\"** Tagged :skip:\\n*** Tagged child\\n** Property\\n:PROPERTIES:\\n:UNNUMBERED: t\\n:END:\\n*** Property child\\n* Omega\\n\" 2 t \"\" nil) (\"*** Tagged child\\n** Property\\n:PROPERTIES:\\n:UNNUMBERED: t\\n:END:\\n*** Property child\\n* Omega\\n\" 3 nil \"\" nil) (\"** Property\\n:PROPERTIES:\\n:UNNUMBERED: t\\n:END:\\n*** Property child\\n* Omega\\n\" 2 t \"\" nil) (\"*** Property child\\n* Omega\\n\" 3 nil \"\" nil) (\"* Omega\\n\" 1 nil \"[2] \" bold)) ((\"* Alpha\\n** Beta\\n*** Inserted\\n*** Leaf\\n** COMMENT Hidden\\n*** Hidden child\\n** Tagged\" 1 nil \"[1] \" bold) (\"** Beta\\n*** Inserted\\n*** Leaf\\n** COMMENT Hidden\\n*** Hidden child\\n** Tagged\" 2 nil \"[1.1] \" bold) (\"*** Inserted\\n*** Leaf\\n** COMMENT Hidden\\n*** Hidden child\\n** Tagged\" 3 nil \"[1.1.1] \" bold) (\"*** Leaf\\n** COMMENT Hidden\\n*** Hidden child\\n** Tagged\" 3 nil \"[1.1.2] \" bold) (\"** COMMENT Hidden\\n*** Hidden child\\n** Tagged\" 2 t \"\" nil) (\"*** Hidden child\\n** Tagged\" 3 nil \"\" nil) (\"** Tagged\" 2 nil \"[1.2] \" bold) (\"\\n\" 3 nil \"[1.2.1] \" bold) (\"\\n*** Tagged child\\n\" 2 t \"\" nil) (\"\\n*** Tagged child\\n** Property\\n:PROPERTIES:\\n:UNNUMBERED: t\\n:END:\\n\" 3 nil \"\" nil) (\"\\n*** Tagged child\\n** Property\\n:PROPERTIES:\\n:UNNUMBERED: t\\n:END:\\n*** Property child\\n\" 1 nil \"[2] \" bold)) nil \"* Alpha\\n** Beta\\n*** Inserted\\n*** Leaf\\n** COMMENT Hidden\\n*** Hidden child\\n** Tagged\\n*** Tagged child\\n** Property\\n:PROPERTIES:\\n:UNNUMBERED: t\\n:END:\\n*** Property child\\n* Omega\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-num)
  (with-temp-buffer
    (let ((org-num-max-level 3)
          (org-num-skip-commented t)
          (org-num-skip-tags '("noexport" "skip"))
          (org-num-skip-unnumbered t)
          (org-num-format-function
           (lambda (numbers)
             (propertize (format "[%s] " (mapconcat #'number-to-string numbers "."))
                         'face 'bold))))
      (org-mode)
      (insert "* Alpha\n")
      (insert "** Beta\n")
      (insert "*** Leaf\n")
      (insert "** COMMENT Hidden\n")
      (insert "*** Hidden child\n")
      (insert "** Tagged :skip:\n")
      (insert "*** Tagged child\n")
      (insert "** Property\n:PROPERTIES:\n:UNNUMBERED: t\n:END:\n")
      (insert "*** Property child\n")
      (insert "* Omega\n")
      (org-num-mode 1)
      (let ((snapshot
             (lambda ()
               (let (out)
                 (dolist (ov (sort (copy-sequence org-num--overlays)
                                   (lambda (a b)
                                     (< (overlay-start a) (overlay-start b)))))
                   (push (list (buffer-substring-no-properties
                                (overlay-start ov)
                                (line-end-position))
                               (overlay-get ov 'level)
                               (overlay-get ov 'skip)
                               (substring-no-properties
                                (or (overlay-get ov 'after-string) ""))
                               (get-text-property
                                0 'face
                                (or (overlay-get ov 'after-string) "")))
                         out))
                 (nreverse out)))))
        (let ((before (funcall snapshot)))
          (goto-char (point-min))
          (search-forward "Beta")
          (end-of-line)
          (insert "\n*** Inserted")
          (org-num--verify (line-beginning-position) (point) 0)
          (goto-char (point-min))
          (search-forward "Tagged :skip:")
          (org-toggle-tag "skip" 'off)
          (org-num--verify (line-beginning-position) (line-end-position) 0)
          (list before
                (funcall snapshot)
                org-num--invalid-flag
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_num_odd_levels_and_footnotes_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"* Top\\n*** Odd child\\n***** Odd grandchild\\n* Footnotes\\n[fn:1] Footnote text.\\n* Tail\\n\" 1 nil \"1 \") (\"*** Odd child\\n***** Odd grandchild\\n* Footnotes\\n[fn:1] Footnote text.\\n* Tail\\n\" 2 nil \"1.1 \") (\"***** Odd grandchild\\n* Footnotes\\n[fn:1] Footnote text.\\n* Tail\\n\" 3 nil \"1.1.1 \") (\"* Footnotes\\n[fn:1] Footnote text.\\n* Tail\\n\" 1 t \"\") (\"* Tail\\n\" 1 nil \"2 \"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-num)
  (with-temp-buffer
    (let ((org-odd-levels-only t)
          (org-num-max-level 3)
          (org-num-skip-footnotes t)
          (org-num-format-function #'org-num-default-format))
      (org-mode)
      (insert "* Top\n")
      (insert "*** Odd child\n")
      (insert "***** Odd grandchild\n")
      (insert "* Footnotes\n")
      (insert "[fn:1] Footnote text.\n")
      (insert "* Tail\n")
      (org-num-mode 1)
      (let (out)
        (dolist (ov (sort (copy-sequence org-num--overlays)
                          (lambda (a b)
                            (< (overlay-start a) (overlay-start b)))))
          (push (list (buffer-substring-no-properties
                       (overlay-start ov)
                       (line-end-position))
                      (overlay-get ov 'level)
                      (overlay-get ov 'skip)
                      (substring-no-properties
                       (or (overlay-get ov 'after-string) "")))
                out))
        (nreverse out)))))"##,
        expect,
    );
}

#[test]
fn org_num_mode_toggle_clear_reenable_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((1 3 \"1|\") (5 8 \"1-1|\") (10 12 \"2|\")) (nil nil) ((\"* A\\n** B\\n* C\\n** D\\n\" \"1|\") (\"** B\\n* C\\n** D\\n\" \"1-1|\") (\"* C\\n** D\\n\" \"2|\") (\"** D\\n\" \"2-1|\")) \"* A\\n** B\\n* C\\n** D\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-num)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n** B\n* C\n")
    (let ((org-num-format-function
           (lambda (numbers) (format "%s|" (mapconcat #'number-to-string numbers "-")))))
      (org-num-mode 1)
      (let ((first (mapcar (lambda (ov)
                             (list (overlay-start ov)
                                   (overlay-end ov)
                                   (substring-no-properties
                                    (or (overlay-get ov 'after-string) ""))))
                           org-num--overlays)))
        (org-num-mode -1)
        (let ((after-disable (list org-num--overlays
                                   (overlays-in (point-min) (point-max)))))
          (goto-char (point-max))
          (insert "** D\n")
          (org-num-mode 1)
          (list first
                after-disable
                (mapcar (lambda (ov)
                          (list (buffer-substring-no-properties
                                 (overlay-start ov)
                                 (line-end-position))
                                (substring-no-properties
                                 (or (overlay-get ov 'after-string) ""))))
                        org-num--overlays)
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_num_narrow_mutate_promote_refresh_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable states)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-num)
  (with-temp-buffer
    (let ((org-num-max-level 4)
          (org-num-skip-tags '("skip"))
          (org-num-skip-commented t)
          (org-num-skip-unnumbered t)
          (org-num-format-function
           (lambda (numbers)
             (propertize
              (format "<%s> " (mapconcat #'number-to-string numbers "-"))
              'face 'org-warning))))
      (org-mode)
      (insert "* Root\n")
      (insert "** A\n")
      (insert "*** A1\n")
      (insert "*** A2 :skip:\n")
      (insert "**** A2 child\n")
      (insert "** B\n")
      (insert ":PROPERTIES:\n:UNNUMBERED: t\n:END:\n")
      (insert "*** B child\n")
      (insert "* Tail\n")
      (org-num-mode 1)
      (let ((snapshot
             (lambda (label)
               (list label
                     org-num--invalid-flag
                     org-num--missing-overlay
                     (mapcar
                      (lambda (ov)
                        (list (buffer-substring-no-properties
                               (overlay-start ov)
                               (line-end-position))
                              (overlay-get ov 'level)
                              (overlay-get ov 'skip)
                              (substring-no-properties
                               (or (overlay-get ov 'after-string) ""))
                              (get-text-property
                               0 'face
                               (or (overlay-get ov 'after-string) ""))))
                      (sort (copy-sequence org-num--overlays)
                            (lambda (a b)
                              (< (overlay-start a) (overlay-start b)))))))))
            states)
        (push (funcall snapshot 'initial) states)
        (goto-char (point-min))
        (search-forward "** A")
        (beginning-of-line)
        (org-narrow-to-subtree)
        (goto-char (point-max))
        (insert "*** A3\n**** A3 child\n")
        (org-num--verify (point-min) (point-max) 0)
        (push (funcall snapshot 'after-narrow-insert) states)
        (goto-char (point-min))
        (search-forward "A2 :skip:")
        (org-toggle-tag "skip" 'off)
        (org-num--verify (line-beginning-position) (line-end-position) 0)
        (push (funcall snapshot 'after-unskip) states)
        (goto-char (point-min))
        (search-forward "A3 child")
        (beginning-of-line)
        (org-promote-subtree)
        (org-num--verify (line-beginning-position) (line-end-position) 0)
        (push (funcall snapshot 'after-promote) states)
        (goto-char (point-min))
        (search-forward "A1")
        (beginning-of-line)
        (kill-whole-line)
        (org-num--verify (point-min) (point-max) 0)
        (push (funcall snapshot 'after-delete) states)
        (widen)
        (org-num--update)
        (push (funcall snapshot 'after-widen-update) states)
        (list (nreverse states)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_num_face_skip_cleanup_archive_footnotes_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable states)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-num)
  (with-temp-buffer
    (let ((org-num-max-level nil)
          (org-num-face 'org-warning)
          (org-num-skip-commented t)
          (org-num-skip-footnotes t)
          (org-num-skip-tags '("ARCHIVE"))
          (org-num-skip-unnumbered t)
          (org-num-format-function
           (lambda (numbers)
             (if (= 1 (length numbers))
                 (format "{%s} " (mapconcat #'number-to-string numbers "."))
               (propertize
                (format "{%s} " (mapconcat #'number-to-string numbers "."))
                'face 'org-done)))))
      (org-mode)
      (insert "* Alpha\n")
      (insert "** Beta\n")
      (insert "*** Gamma\n")
      (insert "** COMMENT Hidden\n")
      (insert "*** Hidden child\n")
      (insert "** Archived :ARCHIVE:\n")
      (insert "*** Archived child\n")
      (insert "** Property\n")
      (insert ":PROPERTIES:\n:UNNUMBERED: t\n:END:\n")
      (insert "*** Property child\n")
      (insert "* Footnotes\n")
      (insert "[fn:1] note body\n")
      (insert "* Tail\n")
      (let ((snapshot
             (lambda (label)
               (list label
                     org-num--invalid-flag
                     org-num--missing-overlay
                     (mapcar
                      (lambda (ov)
                        (let ((text (or (overlay-get ov 'after-string) "")))
                          (list (buffer-substring-no-properties
                                 (overlay-start ov)
                                 (line-end-position))
                                (overlay-get ov 'level)
                                (overlay-get ov 'skip)
                                (substring-no-properties text)
                                (get-text-property 0 'face text)
                                (overlay-get ov 'numbering-face)
                                (overlay-buffer ov))))
                      (sort (copy-sequence org-num--overlays)
                            (lambda (a b)
                              (< (overlay-start a) (overlay-start b)))))))))
            states)
        (org-num-mode 1)
        (push (funcall snapshot 'initial) states)
        (goto-char (point-min))
        (search-forward "Archived")
        (org-toggle-tag "ARCHIVE" 'off)
        (org-num--verify (line-beginning-position) (line-end-position) 0)
        (push (funcall snapshot 'after-unarchive) states)
        (goto-char (point-min))
        (search-forward "Property")
        (beginning-of-line)
        (org-entry-delete nil "UNNUMBERED")
        (org-num--verify (line-beginning-position) (line-end-position) 0)
        (push (funcall snapshot 'after-property-delete) states)
        (goto-char (point-min))
        (search-forward "Tail")
        (beginning-of-line)
        (org-demote-subtree)
        (org-num--verify (line-beginning-position) (line-end-position) 0)
        (push (funcall snapshot 'after-tail-demote) states)
        (org-num-mode -1)
        (list (nreverse states)
              org-num--overlays
              (mapcar (lambda (ov)
                        (list (overlay-start ov)
                              (overlay-end ov)
                              (overlay-get ov 'org-num)))
                      (overlays-in (point-min) (point-max)))
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_num_folded_subtree_cut_paste_renumber_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable states)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (require 'org-num)
  (with-temp-buffer
    (let ((org-num-max-level 5)
          (org-num-skip-commented t)
          (org-num-skip-tags '("skip"))
          (org-num-skip-unnumbered t)
          (org-num-format-function
           (lambda (numbers)
             (format "[%s] " (mapconcat #'number-to-string numbers ".")))))
      (org-mode)
      (insert "* Alpha\n")
      (insert "** A1\n")
      (insert "*** A1a\n")
      (insert "**** A1a-i\n")
      (insert "** A2 :skip:\n")
      (insert "*** A2a\n")
      (insert "* Beta\n")
      (insert "** B1\n")
      (insert "*** B1a\n")
      (insert "* Gamma\n")
      (insert "** G1\n")
      (let ((snapshot
             (lambda (label)
               (list label
                     org-num--invalid-flag
                     org-num--missing-overlay
                     (mapcar
                      (lambda (needle)
                        (save-excursion
                          (goto-char (point-min))
                          (search-forward needle)
                          (beginning-of-line)
                          (let ((ov (cl-find-if
                                     (lambda (overlay)
                                       (= (overlay-start overlay) (point)))
                                     org-num--overlays)))
                            (list needle
                                  (line-number-at-pos)
                                  (org-outline-level)
                                  (invisible-p (line-end-position))
                                  (and ov (overlay-get ov 'level))
                                  (and ov (overlay-get ov 'skip))
                                  (and ov
                                       (substring-no-properties
                                        (or (overlay-get ov 'after-string)
                                            "")))))))
                      '("Alpha" "A1" "A1a" "A1a-i" "A2" "A2a"
                        "Beta" "B1" "B1a" "Gamma" "G1"))
                     (mapcar
                      (lambda (ov)
                        (list (buffer-substring-no-properties
                               (overlay-start ov) (line-end-position))
                              (overlay-get ov 'level)
                              (overlay-get ov 'skip)
                              (substring-no-properties
                               (or (overlay-get ov 'after-string) ""))))
                      (sort (copy-sequence org-num--overlays)
                            (lambda (a b)
                              (< (overlay-start a) (overlay-start b)))))
                     (buffer-substring-no-properties
                      (point-min) (point-max))))))
            states)
        (org-num-mode 1)
        (push (funcall snapshot 'initial) states)
        (goto-char (point-min))
        (search-forward "A1")
        (beginning-of-line)
        (org-fold-hide-subtree)
        (push (funcall snapshot 'fold-a1) states)
        (org-cut-subtree)
        (org-num--verify (point-min) (point-max) 0)
        (push (funcall snapshot 'after-cut-a1) states)
        (goto-char (point-min))
        (search-forward "Gamma")
        (beginning-of-line)
        (org-paste-subtree 2)
        (org-num--verify (point-min) (point-max) 0)
        (push (funcall snapshot 'after-paste-under-gamma) states)
        (goto-char (point-min))
        (search-forward "A2")
        (org-toggle-tag "skip" 'off)
        (org-num--verify (line-beginning-position) (line-end-position) 0)
        (push (funcall snapshot 'after-unskip-a2) states)
        (goto-char (point-min))
        (search-forward "B1a")
        (beginning-of-line)
        (org-promote-subtree)
        (org-num--verify (line-beginning-position) (line-end-position) 0)
        (push (funcall snapshot 'after-promote-b1a) states)
        (org-fold-show-all)
        (org-num--update)
        (push (funcall snapshot 'final-show-all) states)
        (list (nreverse states)
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}
