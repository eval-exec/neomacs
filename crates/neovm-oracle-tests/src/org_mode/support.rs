use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_pcomplete_case_command_at_point_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"TODO\" \"todo\" \"DONE\" \"done\" \"WAIT\" \"wait\") (\"file-option\" . \"STARTUP\") \"file-option/startup\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-pcomplete)
  (with-temp-buffer
    (org-mode)
    (insert "#+STARTUP: fold\n")
    (insert "#+PROPERTY: Effort_ALL 0:15 0:30\n")
    (insert "* TODO Heading\n")
    (insert ":PROPERTIES:\n:Effort: 0:15\n:END:\n")
    (goto-char (point-min))
    (search-forward "STARTUP")
    (list (org-pcomplete-case-double '("todo" "done" "Wait"))
          (org-thing-at-point)
          (org-command-at-point))))"##,
        expect,
    );
}

#[test]
fn org_ctags_lookup_replace_tag_table_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"XbrXcXdXbrX\" (\"topic.org\" 1 1) (\"Alpha\" \"Beta\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-ctags)
  (let* ((root (make-temp-file "org-ctags" t))
         (topic (expand-file-name "topic.org" root))
         (tags (expand-file-name "TAGS" root))
         (tags-file-name tags))
    (unwind-protect
        (progn
          (with-temp-file topic
            (insert "* Alpha\nBody\n* Beta\nBody\n"))
          (with-temp-file tags
            (insert "\f\n" topic ",20\n"
                    "Alpha\177Alpha\0011,1\n"
                    "Beta\177Beta\0013,14\n"))
          (let ((found (org-ctags-get-filename-for-tag "Alpha")))
            (list (org-ctags-string-search-and-replace
                   "a" "X" "abracadabra")
                  (list (file-name-nondirectory (nth 0 found))
                        (nth 1 found)
                        (nth 2 found))
                  (sort (org-ctags-all-tags-in-current-tags-table)
                        #'string<))))
      (delete-directory root t))))"#,
        expect,
    );
}

#[test]
fn org_ctags_point_append_narrow_decline_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-ctags-new-topic-template)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-ctags)
  (with-temp-buffer
    (org-mode)
    (insert "* Source\n")
    (insert "Plain AlphaBeta text before [[WikiTopic]] and Mixed_Word_99.\n")
    (let ((probe
           (lambda (needle offset)
             (goto-char (point-min))
             (search-forward needle)
             (forward-char offset)
             (org-ctags-find-tag-at-point))))
          (org-ctags-new-topic-template "* <<%t>>\nBody for %t.\n\n"))
      (let ((point-tags (list (funcall probe "AlphaBeta" -3)
                              (funcall probe "WikiTopic" -5)
                              (funcall probe "Mixed_Word_99" -4))))
        (goto-char (point-max))
        (let ((appended (org-ctags-append-topic "fresh topic" t))
              (narrowed (buffer-narrowed-p))
              (narrow-text (buffer-substring-no-properties
                            (point-min) (point-max)))
              (narrow-point (list (line-number-at-pos)
                                  (- (point) (point-min)))))
          (widen)
          (let ((declined
                 (cl-letf (((symbol-function 'y-or-n-p)
                            (lambda (&rest _) nil)))
                   (org-ctags-ask-append-topic "declined topic")))
                (full-text (buffer-substring-no-properties
                            (point-min) (point-max))))
            (list point-tags
                  appended
                  narrowed
                  narrow-point
                  narrow-text
                  declined
                  (string-match-p "declined topic" full-text)
                  (org-ctags-fail-silently "anything")
                  full-text))))))"#,
        expect,
    );
}

#[test]
fn org_ctags_enable_create_visit_interactive_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil (\"org-ctags-find-tag\" \"org-ctags-visit-buffer-or-file\" \"org-ctags-append-topic\" \"org-ctags-fail-silently\") org-ctags-find-tag-at-point (\"Alpha\" \"Beta Tag\" \"Fresh Topic\") (\"main.org\" 3 33) nil (\"Alpha\") \"ctags --langdef=orgmode --langmap=orgmode:.org --regex-orgmode=/\\\\<\\\\<\\\\(\\\\[\\\\^\\\\<\\\\>\\\\]\\\\+\\\\)\\\\>\\\\>/\\\\\\\\1/d\\\\,definition/ -f <root>/TAGS -e -R <root>/*\" 54 \"Existing.org\" (\"Created.org\" \"* <<Created>>\\nCreated body for Created.\\n\\n\") nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-ctags)
  (let* ((root (make-temp-file "org-ctags-flow" t))
         (main (expand-file-name "main.org" root))
         (existing (expand-file-name "Existing.org" root))
         (tags (expand-file-name "TAGS" root))
         (default-directory root)
         (tags-file-name tags)
         (org-ctags-new-topic-template
          "* <<%t>>\nCreated body for %t.\n\n")
         (org-ctags-open-link-functions
          '(org-ctags-find-tag
            org-ctags-visit-buffer-or-file
            org-ctags-append-topic
            org-ctags-fail-silently))
         (org-open-link-functions nil)
         (org-ctags-find-tag-history nil)
         (commands nil)
         (xref-calls nil))
    (unwind-protect
        (progn
          (with-temp-file main
            (insert "Alpha\n* Main\nSee [[Alpha]] and [[Fresh Topic]].\n"))
          (with-temp-file existing
            (insert "* Existing root\n"))
          (with-temp-file tags
            (insert "\f\n" main ",40\n"
                    "Alpha\177Alpha\0011,1\n"
                    "Beta Tag\177Beta Tag\0012,12\n"))
          (cl-letf (((symbol-function 'shell-command)
                     (lambda (cmd)
                       (push cmd commands)
                       (with-temp-file tags
                         (insert "\f\n" main ",80\n"
                                 "Alpha\177Alpha\0011,1\n"
                                 "Beta Tag\177Beta Tag\0012,12\n"
                                 "Fresh Topic\177Fresh Topic\0013,33\n"))
                       0))
                    ((symbol-function 'xref-find-definitions)
                     (lambda (tag)
                       (push tag xref-calls)
                       (when (string= tag "missing")
                         (error "missing"))
                       t))
                    ((symbol-function 'completing-read)
                     (let ((answers '("Alpha" "New Topic")))
                       (lambda (&rest _)
                         (pop answers)))))
            (with-current-buffer (find-file-noselect main)
              (org-mode)
              (let ((before-hooks org-open-link-functions))
                (org-ctags-enable)
                (let ((enabled-hooks org-open-link-functions)
                      (find-default
                       (get 'org-mode 'find-tag-default-function)))
                  (org-ctags-create-tags root)
                  (visit-tags-table tags t)
                  (org--ctags-load-tag-list)
                  (org-ctags-find-tag-interactive)
                  (org-ctags-find-tag-interactive)
                  (let ((interactive-text
                         (buffer-substring-no-properties
                          (point-min) (point-max)))
                        (history org-ctags-find-tag-history)
                        (tag-list org-ctags-tag-list)
                        (found (org-ctags-get-filename-for-tag
                                "Fresh Topic")))
                    (let ((opened-buffer
                           (progn
                             (org-ctags-visit-buffer-or-file "Existing")
                             (buffer-name)))
                          (created-buffer
                           (progn
                             (org-ctags-visit-buffer-or-file
                              "Created" t)
                             (list (buffer-name)
                                   (buffer-substring-no-properties
                                    (point-min) (point-max))))))
                      (with-current-buffer (find-file-noselect main)
                        (org-ctags-unload-function)
                        (list before-hooks
                              (mapcar #'symbol-name enabled-hooks)
                              find-default
                              (sort tag-list #'string<)
                              (list (file-name-nondirectory (nth 0 found))
                                    (nth 1 found)
                                    (nth 2 found))
                              (reverse xref-calls)
                              history
                              (replace-regexp-in-string
                               (regexp-quote root)
                               "<root>"
                               (car commands))
                              (string-match-p "New Topic"
                                              interactive-text)
                              opened-buffer
                              created-buffer
                              (get 'org-mode
                                   'find-tag-default-function)
                              (mapcar #'symbol-name
                                      org-open-link-functions))))))))))
      (dolist (file (list main existing
                          (expand-file-name "Created.org" root)))
        (when (get-file-buffer file) (kill-buffer (get-file-buffer file))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_crypt_detect_encrypted_entry_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((51 109) nil \"-----BEGIN PGP MESSAGE-----\\nabc\\n-----END PGP MESSAGE-----\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-crypt)
  (with-temp-buffer
    (org-mode)
    (insert "* Secret :crypt:\n")
    (insert ":PROPERTIES:\n:CRYPTKEY: nil\n:END:\n")
    (insert "-----BEGIN PGP MESSAGE-----\nabc\n-----END PGP MESSAGE-----\n")
    (insert "* Plain\n")
    (goto-char (point-min))
    (search-forward "Secret")
    (beginning-of-line)
    (let ((encrypted (org-at-encrypted-entry-p))
          (key (let ((org-crypt-key nil))
                 (org-crypt-key-for-heading))))
      (list (and encrypted
                 (list (- (car encrypted) (point-min))
                       (- (cdr encrypted) (point-min))))
            key
            (and encrypted
                 (org-crypt--encrypted-text
                  (car encrypted)
                  (cdr encrypted)))))))"#,
        expect,
    );
}

#[test]
fn org_macs_plist_string_visibility_time_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((b 2 5) (c) (a 1 3 4)) (a c e) (:a 1 :b override :drop 9 :c nil :d 4) (:a 1 :b override :d 4) ((:alpha 1) (:beta two) (:gamma nil)) ((\"[inside]\" \"inside\" \"[inside]\" \"[inside]\") (\"\\\"quoted\\\"\" \"\\\"quoted\\\"\" \"quoted\" \"\\\"quoted\\\"\") (\"short\" \"short\" \"short\" \"short\") (\"long words break here\" \"long words break here\" \"long words break here\" \"long...\")) \"a   bb  c\" \"a\\n b\\nc\" ((\"one two\" \"three\" \"four five\") (\"one two three\" \"four five\")) \"alpha\\n  beta\\ngamma\\n\" \"NR/N//TM\" \"alpha   |%a-beta|nil|alpha\" (italic highlight \"help\" italic) (\"aaBBcc\" nil \"aaBBcc\" nil) (t t nil 43 51) ((active (0 45 13 27 5 2026 nil -1 nil) (0 45 13 27 5 2026 nil -1 nil)) (range (0 45 13 27 5 2026 nil -1 nil) (0 45 13 27 5 2026 nil -1 nil))) (t t nil t nil 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (let ((prop-string (copy-sequence "aaBBcc"))
        (now (float-time (encode-time 0 0 12 27 5 2026))))
    (add-text-properties 2 4 '(face bold invisible org-fold-outline)
                         prop-string)
    (with-temp-buffer
      (org-mode)
      (insert "* Alpha\nVisible line\n** Hidden\nSecret line\n* Beta\n")
      (goto-char (point-min))
      (search-forward "Alpha")
      (beginning-of-line)
      (org-fold-hide-subtree)
      (let* ((secret-pos (save-excursion
                           (goto-char (point-min))
                           (search-forward "Secret")
                           (point)))
             (visible-pos (save-excursion
                            (goto-char (point-min))
                            (search-forward "Beta")
                            (point)))
             (plist-a '(:a 1 :b 2 :drop 9))
             (plist-b '(:b override :c nil :d 4))
             (combined (org-combine-plists plist-a plist-b))
             (deleted (org-plist-delete-all combined '(:drop :c)))
             (added (org-add-props (copy-sequence "PROP")
                        '(face italic)
                      'mouse-face 'highlight
                      'help-echo "help"))
             (restricted (org-no-properties (copy-sequence prop-string) t))
             (plain (org-no-properties (copy-sequence prop-string)))
             (template
              (org-fill-template
               "%noweb-ref/%noweb/%missing/%tangle-mode"
               '(("noweb" . "N")
                 ("noweb-ref" . "NR")
                 ("tangle-mode" . "TM")
                 ("missing" . nil))))
             (escapes
              (org-replace-escapes
               "%-8a|%b|%c|%a"
               '(("%a" . "alpha")
                 ("%b" . "%a-beta")
                 ("%c" . nil)))))
        (let ((org-matcher-time-now now))
          (list (org-uniquify-alist
                 '((a 1) (b 2) (a 3 4) (c) (b 5)))
                (org-delete-all '(b d) '(a b c b d e))
                combined
                deleted
                (org-make-parameter-alist
                 '(:alpha 1 :beta two :gamma nil))
                (mapcar (lambda (s)
                          (list s
                                (org-unbracket-string "[" "]" s)
                                (org-strip-quotes s)
                                (org-shorten-string s 10)))
                        '("[inside]" "\"quoted\""
                          "short" "long words break here"))
                (org-remove-tabs "a\tbb\tc" 4)
                (org-remove-blank-lines "a\n\n  \n b\n\nc")
                (list (org-wrap "one two three four five" 9)
                      (org-wrap "one two three four five" nil 2))
                (org-remove-indentation
                 "    alpha\n      beta\n    gamma\n")
                template
                escapes
                (list (get-text-property 0 'face added)
                      (get-text-property 0 'mouse-face added)
                      (get-text-property 0 'help-echo added)
                      (org-find-text-property-in-string 'face added))
                (list restricted
                      (text-properties-at 2 restricted)
                      plain
                      (text-properties-at 2 plain))
                (list (not (null (org-invisible-p secret-pos)))
                      (not (null (org-invisible-p secret-pos t)))
                      (org-invisible-p visible-pos)
                      (save-excursion
                        (goto-char secret-pos)
                        (org-find-visible))
                      (save-excursion
                        (goto-char visible-pos)
                        (org-find-invisible)))
                (mapcar (lambda (pair)
                          (list (car pair)
                                (org-parse-time-string (cdr pair))
                                (org-parse-time-string (cdr pair) t)))
                        '((active . "<2026-05-27 Wed 13:45>")
                          (range . "<2026-05-27 Wed 13:45-15:00>")))
                (list (org-time< "<2026-05-27 Wed>" "<2026-05-28 Thu>")
                      (org-time= "<2026-05-27 Wed>" "<2026-05-27 Wed>")
                      (org-time<> "<2026-05-27 Wed>" "<2026-05-28 Thu>")
                      (org-time> 10 5)
                      (org-time<= nil 5)
                      (org-2ft "not a time"))))))))"##,
        expect,
    );
}

#[test]
fn org_mks_nested_special_navigation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"b\" \"Beta direct\" beta) (\"ab\" \"Alpha two\" alpha-two) (error \"no more keys\") \"!\" (\"aa\" \"Alpha one\" alpha-one :payload 1) (\"b\" \"Beta direct\" beta) (user-error \"Abort\") (14) (\"*Org Select*\" \"*Org Select*\" \"*Org Select*\" \"*Org Select*\" \"*Org Select*\" \"*Org Select*\" \"*Org Select*\") 10 (\"\" \"\" \"\" \"\" \"\" \"Invalid key: `\t'\" \"\" \"\" \"Invalid key: `z'\" \"\" \"\" \"\") nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((table '(("a" "Alpha prefix")
                 ("aa" "Alpha one" alpha-one :payload 1)
                 ("ab" "Alpha two" alpha-two)
                 ("b" "Beta direct" beta)
                 ("c" "Nested prefix")
                 ("cx" "Nested x" nested-x)
                 ("c " "Nested space" nested-space)))
        (specials '(("?" "Help")
                    ("!" "Bang")))
        (keys nil)
        (scrolls nil)
        (buffers nil)
        (fits nil)
        (messages nil))
    (cl-labels
        ((run (input)
           (setq keys input)
           (condition-case err
               (cl-letf (((symbol-function 'read-char-exclusive)
                          (lambda (&rest _)
                            (if keys
                                (pop keys)
                              (error "no more keys"))))
                         ((symbol-function 'org-scroll)
                          (lambda (key &optional _)
                            (push key scrolls)))
                         ((symbol-function 'switch-to-buffer-other-window)
                          (lambda (buffer)
                            (push buffer buffers)
                            (get-buffer-create buffer)))
                         ((symbol-function 'org-fit-window-to-buffer)
                          (lambda (&rest args)
                            (push args fits)))
                         ((symbol-function 'message)
                          (lambda (fmt &rest args)
                            (push (apply #'format fmt args) messages))))
                 (org-mks table "Title" "Prompt: " specials))
             (error (cons (car err) (cdr err))))))
      (let ((direct (run '(?b)))
            (nested (run '(?a ?b)))
            (space-nested (run '(?c ?\s)))
            (special (run '(?!)))
            (invalid-then-ok (run '(?z ?a ?a)))
            (nav-then-ok (run '(14 ?b)))
            (abort (run '(?\C-g)))
            (empty-leftover nil))
        (setq empty-leftover keys)
        (list direct
              nested
              space-nested
              special
              invalid-then-ok
              nav-then-ok
              abort
              (nreverse scrolls)
              (nreverse buffers)
              (length fits)
              (nreverse messages)
              empty-leftover
              (get-buffer "*Org Select*"))))))"##,
        expect,
    );
}
