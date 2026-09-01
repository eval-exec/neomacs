use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_goto_local_search_keymap_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((3 \"** TODO Deep Target :tag:\\n\") (5 \"* Beta Deep Target\\n\") nil ((\"q\" org-goto-quit) (\"n\" outline-next-visible-heading) (\"p\" outline-previous-visible-heading) (\"f\" outline-forward-same-level) (\"b\" outline-backward-same-level) (\"u\" outline-up-heading) (\"/\" org-occur) (\"\\r\" org-goto-ret)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-goto)
  (with-temp-buffer
    (let ((org-goto-auto-isearch nil))
      (org-mode)
      (insert "* Alpha\n")
      (insert "Body mentions Deep Target but is not a heading.\n")
      (insert "** TODO Deep Target :tag:\n")
      (insert "*** Leaf 42\n")
      (insert "* Beta Deep Target\n")
      (org-goto--set-map)
      (goto-char (point-min))
      (let* ((forward-body-ignored
              (save-excursion
                (let ((isearch-forward t))
                  (org-goto--local-search-headings "Deep Target" nil t))
                (list (line-number-at-pos)
                      (thing-at-point 'line t))))
             (backward-heading
              (save-excursion
                (goto-char (point-max))
                (let ((isearch-forward nil))
                  (org-goto--local-search-headings "Deep Target" nil t))
                (list (line-number-at-pos)
                      (thing-at-point 'line t))))
             (missing
              (save-excursion
                (let ((isearch-forward t))
                  (org-goto--local-search-headings "missing" nil t))))
             (bindings
              (mapcar (lambda (key)
                        (list key (lookup-key org-goto-map key)))
                      (list "q" "n" "p" "f" "b" "u" "/" "\C-m"))))
        (list forward-body-ignored
              backward-heading
              missing
              bindings
              (keymapp org-goto-map))))))"##,
        expect,
    );
}

#[test]
fn org_goto_location_indirect_return_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (invalid-function (symbol-function 'pop-to-buffer))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-goto)
  (with-temp-buffer
    (let ((org-goto-auto-isearch nil)
          (org-startup-folded 'showeverything))
      (org-mode)
      (insert "* Project\n")
      (insert "** Area\n")
      (insert "*** Target\nBody\n")
      (insert "* Other\n")
      (goto-char (point-min))
      (search-forward "Area")
      (beginning-of-line)
      (let ((origin (point))
            (org-goto-start-pos (point))
            selected)
        (org-goto--set-map)
        (cl-letf (((symbol-function 'recursive-edit)
                   (lambda ()
                     (goto-char (point-min))
                     (search-forward "*** Target")
                     (beginning-of-line)
                     (setq selected (list (buffer-name)
                                          (point)
                                          (thing-at-point 'line t)))
                     (org-goto-ret))))
                  ((symbol-function 'pop-to-buffer)
                   (lambda (buffer-or-name &optional _action _norecord)
                     (switch-to-buffer buffer-or-name)))
                  ((symbol-function 'org-fit-window-to-buffer)
                   (lambda (&rest _) nil)))
          (let ((result (org-goto-location nil "Help %s")))
            (list (list (- (car result) (point-min)) (cdr result))
                  selected
                  (= (point) origin)
                  (get-buffer "*org-goto*")
                  (get-buffer "*Org Help*")
                  (thing-at-point 'line t)))))))"##,
        expect,
    );
}

#[test]
fn org_goto_outline_path_completion_command_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-goto)
  (with-temp-buffer
    (let ((org-goto-interface 'outline-path-completion)
          (org-goto-max-level 4)
          (org-mark-ring nil))
      (org-mode)
      (insert "* Project\n")
      (insert "** Area\n")
      (insert "*** Target\nBody\n")
      (insert "**** Too deep\n")
      (insert "* Other\n")
      (goto-char (point-min))
      (search-forward "Project")
      (let ((origin (line-number-at-pos))
            (target (save-excursion
                      (search-forward "*** Target")
                      (beginning-of-line)
                      (point)))
            captured-targets)
        (cl-letf (((symbol-function 'org-refile-get-location)
                   (lambda (&rest _)
                     (setq captured-targets org-refile-targets)
                     (list "Project/Area/Target" nil nil target)))
                  ((symbol-function 'org-refile-check-position)
                   (lambda (location)
                     (list 'checked (car location) (nth 3 location)))))
          (org-goto)
          (let ((after-path (list (line-number-at-pos)
                                  (thing-at-point 'line t)
                                  captured-targets
                                  (mapcar #'marker-position org-mark-ring))))
            (goto-char (point-min))
            (search-forward "Other")
            (cl-letf (((symbol-function 'org-goto-location)
                       (lambda (&rest _)
                         (cons target 'return))))
              (let ((before-alt (line-number-at-pos)))
                (org-goto t)
                (list origin
                      after-path
                      before-alt
                      (line-number-at-pos)
                      (thing-at-point 'line t)
                      (mapcar #'marker-position org-mark-ring))))))))"##,
        expect,
    );
}

#[test]
fn org_goto_exit_commands_and_tag_search_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((org-goto-local-auto-isearch nil nil org-goto-ret) (nil digit-argument org-goto-quit outline-next-visible-heading org-goto-ret) ((1 \"* Match in title\") (4 \"** Nested Match\") (4 \"** Nested Match\")) ((4 \"** Nested Match\")) (nil (org-goto-left left 4) nil (org-goto-right right 4) nil (org-goto-quit quit nil)) ((user-error \"Not on a heading\") (user-error \"Not on a heading\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-goto)
  (with-temp-buffer
    (org-mode)
    (insert "* Match in title\n")
    (insert "Body Match should be ignored.\n")
    (insert "* Tag only :Match:\n")
    (insert "** Nested Match\n")
    (insert "*** Leaf\n")
    (insert "* Final\n")
    (let (map-auto map-manual forward backward exits errors)
      (let ((org-goto-auto-isearch t))
        (org-goto--set-map)
        (setq map-auto
              (list (lookup-key org-goto-map [t])
                    (lookup-key org-goto-map "1")
                    (lookup-key org-goto-map "q")
                    (lookup-key org-goto-map "\C-m"))))
      (let ((org-goto-auto-isearch nil))
        (org-goto--set-map)
        (setq map-manual
              (list (lookup-key org-goto-map [t])
                    (lookup-key org-goto-map "1")
                    (lookup-key org-goto-map "q")
                    (lookup-key org-goto-map "n")
                    (lookup-key org-goto-map "\C-m"))))
      (goto-char (point-min))
      (let ((isearch-forward t))
        (org-goto--local-search-headings "Match" nil t)
        (push (list (line-number-at-pos)
                    (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position)))
              forward)
        (org-goto--local-search-headings "Match" nil t)
        (push (list (line-number-at-pos)
                    (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position)))
              forward)
        (org-goto--local-search-headings "Match" nil t)
        (push (list (line-number-at-pos)
                    (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position)))
              forward))
      (goto-char (point-max))
      (let ((isearch-forward nil))
        (org-goto--local-search-headings "Match" nil t)
        (push (list (line-number-at-pos)
                    (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position)))
              backward))
      (dolist (cmd '(org-goto-left org-goto-right org-goto-quit))
        (goto-char (point-min))
        (search-forward "Nested Match")
        (beginning-of-line)
        (push
         (catch 'exit
           (funcall cmd)
           'no-throw)
         exits)
        (push (list cmd
                    org-goto-exit-command
                    (and org-goto-selected-point
                         (line-number-at-pos org-goto-selected-point)))
              exits))
      (dolist (cmd '(org-goto-left org-goto-right))
        (goto-char (point-min))
        (search-forward "Body Match")
        (push
         (condition-case err
             (progn (funcall cmd) 'no-error)
           (error (cons (car err) (cdr err))))
         errors))
      (list map-auto
            map-manual
            (nreverse forward)
            (nreverse backward)
            (nreverse exits)
            (nreverse errors)))))"##,
        expect,
    );
}

#[test]
fn org_heading_navigation_level_position_tracking_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 58 32)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* A\nbody A\n** B\nbody B\n*** C\nbody C\n**** D\nbody D\n")
    (insert "*** E\nbody E\n** F\nbody F\n*** G\nbody G\n* H\nbody H\n** I\nbody I\n")
    (let ((track (lambda (fn &optional arg)
                   (save-excursion
                     (if arg (funcall fn arg) (funcall fn))
                     (list (line-number-at-pos)
                           (buffer-substring-no-properties
                            (line-beginning-position)
                            (line-end-position)))))))
      ;; Start at beginning
      (goto-char (point-min))
      (list
       ;; org-next-visible-heading forward
       (funcall track #'org-next-visible-heading 1)
       (funcall track #'org-next-visible-heading 1)
       (funcall track #'org-next-visible-heading 1)
       ;; org-previous-visible-heading backward
       (funcall track #'org-previous-visible-heading 1)
       (funcall track #'org-previous-visible-heading 1)
       ;; org-forward-heading-same-level
       (goto-char (point-min))
       (funcall track #'org-forward-heading-same-level 1)
       (funcall track #'org-forward-heading-same-level 1)
       ;; org-backward-heading-same-level
       (funcall track #'org-backward-heading-same-level 1)
       ;; org-end-of-subtree
       (goto-char (point-min))
       (search-forward "** B")
       (beginning-of-line)
       (let ((end-pos (progn (org-end-of-subtree) (point))))
         (list (line-number-at-pos end-pos)
               (buffer-substring-no-properties
                (line-beginning-position)
                (line-end-position))))
       ;; org-up-heading
       (goto-char (point-min))
       (search-forward "**** D")
       (beginning-of-line)
       (funcall track #'org-up-heading 1)
       (funcall track #'org-up-heading 1)
       ;; org-next-visible-heading with negative arg
       (goto-char (point-min))
       (search-forward "* H")
       (beginning-of-line)
       (funcall track #'org-next-visible-heading -1)
       ;; Outline level at each heading
       (goto-char (point-min))
       (let (levels)
         (while (re-search-forward "^\\(\\*+\\) " nil t)
           (push (list (match-string 1)
                       (org-outline-level)
                       (line-number-at-pos))
                 levels))
         (nreverse levels)))))))"##,
        expect,
    );
}
