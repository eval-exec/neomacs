use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_num_overlay_update_after_heading_edit_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"Alpha\" ((t \"\"))) (\"Beta\" ((t \"\"))) (\"COMMENT Skip\" ((t \"\"))) (\"Gamma :noexport:\" ((t \"\"))) (\"Delta\" ((t \"\"))) (\"Epsilon\" ((t \"\")))) ((\"Alpha\" ((t \"\"))) (\"Beta\" ((t \"\"))) (\"Inserted\" ((t \"\"))) (\"COMMENT Skip\" ((t \"\"))) (\"Gamma :noexport:\" ((t \"\"))) (\"Delta\" ((t \"\"))) (\"Epsilon\" ((t \"\")))) \"* Alpha\\n** Beta\\n*** Inserted\\n\\n*** COMMENT Skip\\n*** Gamma :noexport:\\n**** Delta\\n* Epsilon\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-num)
  (with-temp-buffer
    (org-mode)
    (let ((org-num-skip-tags '("noexport"))
          (org-num-max-level 4))
      (insert "* Alpha\n** Beta\n*** COMMENT Skip\n*** Gamma :noexport:\n**** Delta\n* Epsilon\n")
      (org-num-mode 1)
      (let ((snapshot
             (lambda ()
               (let (out)
                 (goto-char (point-min))
                 (while (re-search-forward "^\\*+ \\(.*\\)" nil t)
                   (let* ((pos (line-beginning-position))
                          (ovs (overlays-at pos))
                          (nums (delq nil
                                      (mapcar
                                       (lambda (ov)
                                         (when (overlay-get ov 'org-num)
                                           (list (overlay-get ov 'org-num)
                                                 (substring-no-properties
                                                  (or (overlay-get ov 'before-string)
                                                      "")))))
                                       ovs))))
                     (push (list (match-string-no-properties 1) nums) out)))
                 (nreverse out)))))
        (let ((before (funcall snapshot)))
          (goto-char (point-min))
          (search-forward "Beta")
          (end-of-line)
          (insert "\n*** Inserted\n")
          (org-num--verify (point-min) (point-max) 0)
          (list before
                (funcall snapshot)
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_indent_mode_prefix_after_deep_edit_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"* A\" nil nil) (\"body A\" nil nil) (\"** B\" nil nil) (\"body B\" nil nil) (\"*** C\" nil nil) (\"body C\" (6 org-indent) (6 org-indent)) (\"more C\" (6 org-indent) (6 org-indent)) (\"**** D\" nil nil) (\"body D\" nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-indent)
  (with-temp-buffer
    (let ((org-indent-indentation-per-level 2)
          (org-indent-mode-turns-on-hiding-stars t))
      (org-mode)
      (insert "* A\nbody A\n** B\nbody B\n*** C\nbody C\n**** D\nbody D\n")
      (org-indent-mode 1)
      (font-lock-ensure (point-min) (point-max))
      (goto-char (point-min))
      (search-forward "body C")
      (end-of-line)
      (insert "\nmore C")
      (font-lock-ensure (point-min) (point-max))
      (let (out)
        (goto-char (point-min))
        (while (not (eobp))
          (let* ((lp (get-text-property (line-beginning-position) 'line-prefix))
                 (wp (get-text-property (line-beginning-position) 'wrap-prefix)))
            (push (list (buffer-substring-no-properties
                         (line-beginning-position) (line-end-position))
                        (and (stringp lp)
                             (list (length lp)
                                   (get-text-property 0 'face lp)))
                        (and (stringp wp)
                             (list (length wp)
                                   (get-text-property 0 'face wp))))
                  out))
          (forward-line 1))
        (nreverse out)))))"##,
        expect,
    );
}

#[test]
fn org_font_lock_deep_headline_markup_faces_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"TODO\" org-meta-line t nil) (\"[#A]\" (org-priority org-level-1) t nil) (\"work\" (org-tag org-level-1) nil nil) (\"WAIT\" org-meta-line t nil) (\"link\" (org-link org-level-2) nil nil) (\"DONE\" org-meta-line t nil) (\"code\" (org-verbatim org-headline-done org-level-3) nil nil) (\"verbatim\" (org-code org-headline-done org-level-3) nil nil) (\"[#B]\" (org-priority org-level-4) t nil) (\"bold\" (bold org-level-4) nil nil) (\"italic\" (italic org-level-4) nil nil) (\"deep\" (org-tag org-level-4) nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "#+TODO: TODO WAIT | DONE\n")
    (insert "* TODO [#A] L1 :work:\n")
    (insert "** WAIT L2 with [[https://example.org][link]]\n")
    (insert "*** DONE L3 with =code= and ~verbatim~\n")
    (insert "**** TODO [#B] L4 with *bold* /italic/ :deep:work:\n")
    (font-lock-ensure (point-min) (point-max))
    (let (out)
      (dolist (needle '("TODO" "[#A]" "work" "WAIT" "link"
                        "DONE" "code" "verbatim" "[#B]" "bold"
                        "italic" "deep"))
        (goto-char (point-min))
        (search-forward needle)
        (push (list needle
                    (get-text-property (match-beginning 0) 'face)
                    (get-text-property (match-beginning 0) 'font-lock-fontified)
                    (get-text-property (match-beginning 0) 'invisible))
              out))
      (nreverse out))))"##,
        expect,
    );
}

#[test]
fn org_indent_inlinetask_list_property_refresh_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((before ((\"* Project\" (0 \"\" nil) (2 \"* \" org-indent)) (\"SCHEDULED: <2026-05-27 Wed>\" (0 \"\" nil) (2 \"* \" org-indent)) (\":PROPERTIES:\" (0 \"\" nil) (2 \"* \" org-indent)) (\":Owner: Ada\" (0 \"\" nil) (2 \"* \" org-indent)) (\":END:\" (0 \"\" nil) (2 \"* \" org-indent)) (\"Paragraph one\" (2 \"  \" org-indent) (2 \"  \" org-indent)) (\"- item alpha\" (2 \"  \" org-indent) (4 \"    \" org-indent)) (\"  continuation alpha\" (2 \"  \" org-indent) (4 \"    \" org-indent)) (\"** Area\" (2 \"**\" org-indent) (5 \"**** \" org-indent)) (\"Area body\" (5 \"     \" org-indent) (5 \"     \" org-indent)) (\"**** Inline task\" (6 \"******\" org-warning) (11 \"********** \" org-indent)) (\"Inline body\" (5 \"     \" org-indent) (5 \"     \" org-indent)) (\"**** END\" (6 \"******\" org-warning) (11 \"********** \" org-indent)))) (after-edit ((\"* Project\" (0 \"\" nil) (2 \"* \" org-indent)) (\"SCHEDULED: <2026-05-27 Wed>\" (0 \"\" nil) (2 \"* \" org-indent)) (\":PROPERTIES:\" (0 \"\" nil) (2 \"* \" org-indent)) (\":Owner: Ada\" (0 \"\" nil) (2 \"* \" org-indent)) (\":END:\" (0 \"\" nil) (2 \"* \" org-indent)) (\"Paragraph one\" (2 \"  \" org-indent) (2 \"  \" org-indent)) (\"- item alpha\" (2 \"  \" org-indent) (4 \"    \" org-indent)) (\"  new continuation\" (2 \"  \" org-indent) (4 \"    \" org-indent)) (\"  continuation alpha\" (2 \"  \" org-indent) (4 \"    \" org-indent)) (\"*** Inserted\" (4 \"****\" org-indent) (8 \"******* \" org-indent)) (\"Inserted body\" (8 \"        \" org-indent) (8 \"        \" org-indent)) (\"** Area\" (2 \"**\" org-indent) (5 \"**** \" org-indent)) (\"Area body\" (5 \"     \" org-indent) (5 \"     \" org-indent)) (\"**** Inline task\" (6 \"******\" org-warning) (11 \"********** \" org-indent)) (\"Inline body\" (5 \"     \" org-indent) (5 \"     \" org-indent)) (\"**** END\" (6 \"******\" org-warning) (11 \"********** \" org-indent)))) (0 0) \"* Project\\nSCHEDULED: <2026-05-27 Wed>\\n:PROPERTIES:\\n:Owner: Ada\\n:END:\\nParagraph one\\n- item alpha\\n  new continuation\\n  continuation alpha\\n*** Inserted\\nInserted body\\n** Area\\nArea body\\n**** Inline task\\nInline body\\n**** END\\n\" ((\"* Project\" nil nil) (\"SCHEDULED: <2026-05-27 Wed>\" nil nil) (\":PROPERTIES:\" nil nil) (\":Owner: Ada\" nil nil) (\":END:\" nil nil) (\"Paragraph one\" nil nil) (\"- item alpha\" nil nil) (\"  new continuation\" nil nil) (\"  continuation alpha\" nil nil) (\"*** Inserted\" nil nil) (\"Inserted body\" nil nil) (\"** Area\" nil nil) (\"Area body\" nil nil) (\"**** Inline task\" nil nil) (\"Inline body\" nil nil) (\"**** END\" nil nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-indent)
  (require 'org-inlinetask)
  (with-temp-buffer
    (let ((org-indent-indentation-per-level 3)
          (org-adapt-indentation 'headline-data)
          (org-indent-mode-turns-off-org-adapt-indentation nil)
          (org-indent-mode-turns-on-hiding-stars t)
          (org-inlinetask-min-level 4)
          (org-inlinetask-show-first-star t))
      (org-mode)
      (insert "* Project\n")
      (insert "SCHEDULED: <2026-05-27 Wed>\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:END:\n")
      (insert "Paragraph one\n")
      (insert "- item alpha\n")
      (insert "  continuation alpha\n")
      (insert "** Area\n")
      (insert "Area body\n")
      (insert "**** Inline task\n")
      (insert "Inline body\n")
      (insert "**** END\n")
      (org-indent-mode 1)
      (org-indent-indent-buffer)
      (let ((snapshot
             (lambda (label)
               (let (out)
                 (goto-char (point-min))
                 (while (not (eobp))
                   (let* ((pos (line-beginning-position))
                          (lp (get-text-property pos 'line-prefix))
                          (wp (get-text-property pos 'wrap-prefix)))
                     (push
                      (list (buffer-substring-no-properties
                             pos (line-end-position))
                            (and (stringp lp)
                                 (list (length lp)
                                       (substring-no-properties lp)
                                       (get-text-property 0 'face lp)))
                            (and (stringp wp)
                                 (list (length wp)
                                       (substring-no-properties wp)
                                       (get-text-property 0 'face wp))))
                      out))
                   (forward-line 1))
                 (list label (nreverse out))))))
        (let ((before (funcall snapshot 'before)))
          (goto-char (point-min))
          (search-forward "Area")
          (beginning-of-line)
          (insert "*** Inserted\nInserted body\n")
          (goto-char (point-min))
          (search-forward "item alpha")
          (end-of-line)
          (insert "\n  new continuation")
          (let* ((after-edit (funcall snapshot 'after-edit))
                 (copied (filter-buffer-substring
                          (point-min) (point-max) nil))
                 (copied-props
                  (list (text-property-any 0 (length copied)
                                           'line-prefix nil copied)
                        (text-property-any 0 (length copied)
                                           'wrap-prefix nil copied))))
            (org-indent-mode -1)
            (list before
                  after-edit
                  copied-props
                  (substring-no-properties copied)
                  (let (props)
                    (goto-char (point-min))
                    (while (not (eobp))
                      (push (list (buffer-substring-no-properties
                                   (line-beginning-position)
                                   (line-end-position))
                                  (get-text-property
                                   (line-beginning-position)
                                   'line-prefix)
                                  (get-text-property
                                   (line-beginning-position)
                                   'wrap-prefix))
                            props)
                      (forward-line 1))
                    (nreverse props)))))))))"##,
        expect,
    );
}

#[test]
fn org_preview_latex_inline_image_overlay_lifecycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable norm)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'cl-lib)
  (require 'org)
  (require 'ox)
  (with-temp-buffer
    (let* ((root (file-name-as-directory
                  (make-temp-file "org-oracle-images-" t)))
           (one (expand-file-name "one.png" root))
           (two (expand-file-name "two.jpg" root))
           (three (expand-file-name "three.gif" root))
           (flushes nil)
           (created nil)
           (org-image-actual-width '(300))
           (org-image-align 'right)
           (org-preview-latex-image-directory root)
           org-inline-image-overlays)
      (with-temp-file one (insert "not-real-png"))
      (with-temp-file two (insert "not-real-jpg"))
      (with-temp-file three (insert "not-real-gif"))
      (cl-letf (((symbol-function 'display-graphic-p) (lambda (&rest _) t))
                ((symbol-function 'clear-image-cache)
                 (lambda (&rest _) (push '(clear-image-cache) flushes)))
                ((symbol-function 'image-flush)
                 (lambda (image) (push (list 'image-flush image) flushes)))
                ((symbol-function 'org--create-inline-image)
                 (lambda (file width)
                   (let ((image (list 'fake-image
                                      (file-name-nondirectory file)
                                      width)))
                     (push image created)
                     image)))
                ((symbol-function 'org--latex-preview-region)
                 (lambda (beg end)
                   (save-excursion
                     (goto-char beg)
                     (while (re-search-forward
                             "\\$[^$\n]+\\$\\|\\\\([^)\n]+\\\\)"
                             end t)
                       (org--make-preview-overlay
                        (match-beginning 0) (match-end 0)
                        (expand-file-name
                         (format "latex-%d.svg" (match-beginning 0))
                         root)
                        "svg"))))))
        (org-mode)
        (insert "#+TITLE: Preview overlay combo\n")
        (insert "* Head\n")
        (insert "Text before $a+b$ then inline \\(c+d\\).\n")
        (insert "#+ATTR_ORG: :width 120 :align center\n")
        (insert "[[file:" one "]]\n")
        (insert "#+ATTR_HTML: :width 45% :align right\n")
        (insert "[[file:" two "][file:" two "]]\n")
        (insert "Plain linked image [[file:" three "][visible gif]].\n")
        (let ((norm
               (lambda (value)
                 (cond
                  ((stringp value)
                   (replace-regexp-in-string
                    (regexp-quote root) "<root>/" value t t))
                  ((consp value) (mapcar norm value))
                  (t value))))
              (latex-snapshot
               (lambda (label)
                 (let (out)
                   (dolist (ov (sort (cl-remove-if-not
                                      (lambda (ov)
                                        (eq (overlay-get ov 'org-overlay-type)
                                            'org-latex-overlay))
                                      (overlays-in (point-min) (point-max)))
                                     (lambda (a b)
                                       (< (overlay-start a)
                                          (overlay-start b)))))
                     (push
                      (list (overlay-start ov)
                            (overlay-end ov)
                            (buffer-substring-no-properties
                             (overlay-start ov) (overlay-end ov))
                            (overlay-get ov 'evaporate)
                            (and (overlay-get ov 'modification-hooks) t)
                            (funcall norm (overlay-get ov 'display)))
                      out))
                   (list label (nreverse out)))))
              (image-snapshot
               (lambda (label)
                 (let (out)
                   (dolist (ov (sort (copy-sequence org-inline-image-overlays)
                                     (lambda (a b)
                                       (< (overlay-start a)
                                          (overlay-start b)))))
                     (push
                      (list (overlay-start ov)
                            (overlay-end ov)
                            (buffer-substring-no-properties
                             (overlay-start ov) (overlay-end ov))
                            (overlay-get ov 'org-image-overlay)
                            (overlay-get ov 'face)
                            (keymapp (overlay-get ov 'keymap))
                            (overlay-get ov 'before-string)
                            (overlay-get ov 'display))
                      out))
                   (list label (nreverse out))))))
          (goto-char (point-min))
          (search-forward "$a+b$")
          (org-latex-preview)
          (let ((latex-one (funcall latex-snapshot 'latex-one)))
            (org-latex-preview)
            (let ((latex-toggled (funcall latex-snapshot 'latex-toggled)))
              (org-latex-preview '(16))
              (let ((latex-buffer (funcall latex-snapshot 'latex-buffer)))
                (org-clear-latex-preview
                 (point-min) (save-excursion
                               (goto-char (point-min))
                               (search-forward "\\(c+d\\)")
                               (match-beginning 0)))
                (let ((latex-partial-clear
                       (funcall latex-snapshot 'latex-partial-clear)))
                  (org-clear-latex-preview (point-min) (point-max))
                  (org-display-inline-images nil nil
                                             (point-min) (point-max))
                  (let ((images-unlinked
                         (funcall image-snapshot 'images-unlinked)))
                    (setq flushes nil created nil)
                    (org-display-inline-images t t
                                               (point-min) (point-max))
                    (let ((images-refresh
                           (funcall image-snapshot 'images-refresh))
                          (refresh-flushes
                           (funcall norm (nreverse flushes)))
                          (refresh-created
                           (funcall norm (nreverse created))))
                      (goto-char (point-min))
                      (search-forward "one.png")
                      (delete-char 1)
                      (let ((images-after-modification
                             (funcall image-snapshot
                                      'images-after-modification)))
                        (org-remove-inline-images
                         (save-excursion
                           (goto-char (point-min))
                           (search-forward "two.jpg")
                           (match-beginning 0))
                         (point-max))
                        (list latex-one
                              latex-toggled
                              latex-buffer
                              latex-partial-clear
                              (funcall latex-snapshot 'latex-cleared)
                              images-unlinked
                              images-refresh
                              refresh-flushes
                              refresh-created
                              images-after-modification
                              (funcall image-snapshot
                                       'images-after-region-remove)
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))))))))))))"##,
        expect,
    );
}
