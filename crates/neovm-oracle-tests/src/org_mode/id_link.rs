use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_id_create_save_reload_find_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t \"org\" 2 t \"B\" \"* A\\n:PROPERTIES:\\n:ID: a-id\\n:END:\\nBody\\n* B\\n:PROPERTIES:\\n:ID:       <generated-id>\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-id)
  (let* ((file (make-temp-file
                "org-id-round" nil ".org"
                "* A\n:PROPERTIES:\n:ID: a-id\n:END:\nBody\n* B\n"))
         (org-id-locations-file (make-temp-file "org-id-loc"))
         (org-id-track-globally t)
         (org-id-method 'org))
    (unwind-protect
        (progn
          (org-id-update-id-locations (list file) t)
          (with-current-buffer (find-file-noselect file)
            (org-mode)
            (goto-char (point-min))
            (search-forward "* B")
            (beginning-of-line)
            (let ((new (org-id-get-create)))
              (org-id-locations-save)
              (clrhash org-id-locations)
              (org-id-locations-load)
              (list (not (null (string-match-p "\\`[[:alnum:]]+\\'" new)))
                    (file-name-extension (org-id-find-id-file "a-id"))
                    (hash-table-count org-id-locations)
                    (markerp (org-id-find new t))
                    (with-current-buffer (marker-buffer (org-id-find "a-id" t))
                      (org-get-heading t t t t))
                    (replace-regexp-in-string
                     (regexp-quote new)
                     "<generated-id>"
                     (buffer-substring-no-properties
                      (point-min) (point-max)))))))
      (when (get-file-buffer file)
        (kill-buffer (get-file-buffer file)))
      (delete-file file)
      (when (file-exists-p org-id-locations-file)
        (delete-file org-id-locations-file)))))"##,
        expect,
    );
}

#[test]
fn org_fuzzy_link_search_and_open_heading_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"fuzzy\" \"*Target\" \"Target\" \"Target\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ol)
  (with-temp-buffer
    (org-mode)
    (insert "* Target\nBody line\n* Other\n[[*Target][go]]\n")
    (goto-char (point-min))
    (search-forward "Other")
    (search-forward "[[")
    (let ((link (org-element-context)))
      (list (org-element-property :type link)
            (org-element-property :path link)
            (save-excursion
              (org-link-search "*Target")
              (org-get-heading t t t t))
            (save-excursion
              (org-open-at-point)
              (org-get-heading t t t t))))))"##,
        expect,
    );
}

#[test]
fn org_id_relative_locations_reload_and_find_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"\\n((\\\"a.org\\\" \\\"rel-a\\\") (\\\"sub/b.org\\\" \\\"rel-b\\\"))\\n\" 2 \"a.org\" \"sub/b.org\" \"A\" \"B\")""#
    ]];
    crate::common::assert_oracle_parity_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-id)
  (let* ((root (make-temp-file "org-id-relative" t))
         (sub (expand-file-name "sub" root))
         (file-a (expand-file-name "a.org" root))
         (file-b (expand-file-name "b.org" sub))
         (org-id-locations-file (expand-file-name "ids.el" root))
         (org-id-locations-file-relative t)
         (org-id-track-globally t))
    (unwind-protect
        (progn
          (make-directory sub)
          (with-temp-file file-a
            (insert "* A\n:PROPERTIES:\n:ID: rel-a\n:END:\n"))
          (with-temp-file file-b
            (insert "* B\n:PROPERTIES:\n:ID: rel-b\n:END:\n"))
          (org-id-update-id-locations (list file-a file-b) t)
          (org-id-locations-save)
          (let ((raw (with-temp-buffer
                       (insert-file-contents org-id-locations-file)
                       (buffer-string))))
            (setq org-id-locations nil)
            (org-id-locations-load)
            (let ((marker-a (org-id-find "rel-a" t))
                  (marker-b (org-id-find "rel-b" t)))
              (list raw
                    (hash-table-count org-id-locations)
                    (file-name-nondirectory (org-id-find-id-file "rel-a"))
                    (file-relative-name (org-id-find-id-file "rel-b") root)
                    (and marker-a
                         (with-current-buffer (marker-buffer marker-a)
                           (org-get-heading t t t t)))
                    (and marker-b
                         (with-current-buffer (marker-buffer marker-b)
                           (org-get-heading t t t t)))))))
      (when (get-file-buffer file-a) (kill-buffer (get-file-buffer file-a)))
      (when (get-file-buffer file-b) (kill-buffer (get-file-buffer file-b)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_id_store_parent_context_and_open_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"id:parent-id::*Child\" (:link \"id:parent-id::*Child\" :description \"Child\" :type \"id\") \"Child\" nil \"* Parent\\n:PROPERTIES:\\n:ID: parent-id\\n:END:\\n** Child\\nBody\\n** Sibling\\n\")""#
    ]];
    crate::common::assert_oracle_parity_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-id)
  (require 'ol)
  (let* ((file (make-temp-file "org-id-parent" nil ".org"
                               "* Parent\n:PROPERTIES:\n:ID: parent-id\n:END:\n** Child\nBody\n** Sibling\n"))
         (org-id-locations-file (make-temp-file "org-id-parent-loc"))
         (org-id-track-globally t)
         (org-id-link-to-org-use-id 'use-existing)
         (org-id-link-consider-parent-id t)
         (org-id-link-use-context t)
         (org-link-context-for-files t)
         (org-store-link-plist nil))
    (unwind-protect
        (with-current-buffer (find-file-noselect file)
          (org-mode)
          (org-id-update-id-locations (list file) t)
          (goto-char (point-min))
          (search-forward "** Child")
          (beginning-of-line)
          (let* ((stored (org-id-store-link))
                 (plist org-store-link-plist))
            (goto-char (point-min))
            (search-forward "** Sibling")
            (beginning-of-line)
            (org-id-open (substring stored 3) nil)
            (list stored
                  plist
                  (org-get-heading t t t t)
                  (org-entry-get nil "ID")
                  (buffer-substring-no-properties
                   (point-min) (point-max)))))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-file file)
      (when (file-exists-p org-id-locations-file)
        (delete-file org-id-locations-file)))))"##,
        expect,
    );
}

#[test]
fn org_store_link_custom_id_and_id_policy_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 36 47)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-id)
  (require 'ol)
  (let* ((file (make-temp-file "org-store-custom" nil ".org"
                               "#+TITLE: Store\n* Target\n:PROPERTIES:\n:CUSTOM_ID: custom-target\n:ID: explicit-id\n:END:\nBody\n"))
         (org-id-locations-file (make-temp-file "org-store-custom-loc"))
         (org-id-track-globally t)
         (org-id-link-to-org-use-id 'create-if-interactive-and-no-custom-id)
         (org-link-context-for-files t)
         (org-id-link-use-context t)
         (org-stored-links nil)
         (org-store-link-plist nil))
    (unwind-protect
        (with-current-buffer (find-file-noselect file)
          (org-mode)
          (org-id-update-id-locations (list file) t)
          (goto-char (point-min))
          (search-forward "Target")
          (beginning-of-line)
          (let ((noninteractive (org-store-link nil nil))
                (plist-after-noninteractive org-store-link-plist))
            (setq org-store-link-plist nil
                  org-stored-links nil)
            (let ((interactive (org-store-link nil t)))
              (list noninteractive
                    plist-after-noninteractive
                    interactive
                    org-stored-links
                    org-store-link-plist
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-file file)
      (when (file-exists-p org-id-locations-file)
        (delete-file org-id-locations-file)))))"##,
        expect,
    );
}

#[test]
fn org_id_encoding_paste_tracker_lookup_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 58 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-id)
  (let* ((root (make-temp-file "org-id-low" t))
         (file (expand-file-name "ids.org" root))
         (missing (expand-file-name "missing.org" root))
         (org-id-locations-file (expand-file-name "ids-locations.el" root))
         (org-id-locations-file-relative t)
         (org-id-track-globally t)
         (org-id-locations (make-hash-table :test 'equal)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* Alpha\n:PROPERTIES:\n:ID: alpha-id\n:END:\n")
            (insert "** Beta\n:PROPERTIES:\n:ID: beta-id\n:END:\n"))
          (org-id-paste-tracker
           "  :ID: pasted-one\nText\n:ID: pasted-two\n"
           file)
          (org-id-update-id-locations (list file) t)
          (org-id-add-location "manual-id" file)
          (let* ((alist (sort (org-id-hash-to-alist org-id-locations)
                              (lambda (a b) (string< (car a) (car b)))))
                 (roundtrip (org-id-alist-to-hash alist)))
            (org-id-locations-save)
            (let ((raw (with-temp-buffer
                         (insert-file-contents org-id-locations-file)
                         (buffer-string)))
                  (alpha-cons (org-id-find-id-in-file "alpha-id" file nil))
                  (beta-marker (org-id-find-id-in-file "beta-id" file t))
                  (missing-file (org-id-find-id-in-file "alpha-id" missing nil))
                  (missing-id (org-id-find-id-in-file "no-such-id" file nil)))
              (list (mapcar (lambda (n)
                              (list n
                                    (org-id-int-to-b36 n 4)
                                    (org-id-b36-to-int
                                     (org-id-int-to-b36 n 4))))
                            '(0 1 35 36 1295 46655))
                    (org-id-decode "Pre:000100020003")
                    (mapcar #'car alist)
                    (hash-table-count roundtrip)
                    (gethash "pasted-one" roundtrip)
                    (file-relative-name (gethash "manual-id" roundtrip) root)
                    (replace-regexp-in-string
                     (regexp-quote root)
                     "<root>"
                     raw)
                    (and alpha-cons
                         (list (file-relative-name (car alpha-cons) root)
                               (cdr alpha-cons)))
                    (and beta-marker
                         (with-current-buffer (marker-buffer beta-marker)
                           (list (file-relative-name (buffer-file-name) root)
                                 (marker-position beta-marker)
                                 (org-get-heading t t t t))))
                    missing-file
                    missing-id)))))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_id_open_option_and_colon_id_fallback_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"id:<generated>\" (:link \"id:<generated>\" :description \"Child Beta\" :type \"id\") (\"a.org\" \"Child Beta\" 7) (\"a.org\" \"Child Beta\" \"<<beta-target>>\\n\") (\"b.org\" \"Literal Colon ID\" 1) nil (\"a.org\" \"b.org\"))""#
    ]];
    crate::common::assert_oracle_parity_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-id)
  (require 'ol)
  (let* ((root (make-temp-file "org-id-open" t))
         (file-a (expand-file-name "a.org" root))
         (file-b (expand-file-name "b.org" root))
         (org-id-locations-file (expand-file-name "ids.el" root))
         (org-id-track-globally t)
         (org-id-link-use-context t)
         (org-link-context-for-files t)
         (org-link-frame-setup '((file . find-file))))
    (unwind-protect
        (progn
          (with-temp-file file-a
            (insert "* Parent\n:PROPERTIES:\n:ID: parent\n:END:\n")
            (insert "** Child Alpha\nBody target alpha\n")
            (insert "** Child Beta\n<<beta-target>>\nBody beta\n"))
          (with-temp-file file-b
            (insert "* Literal Colon ID\n:PROPERTIES:\n:ID: legacy::id\n:END:\n")
            (insert "Literal body\n"))
          (org-id-update-id-locations (list file-a file-b) t)
          (with-current-buffer (find-file-noselect file-a)
            (org-mode)
            (goto-char (point-min))
            (search-forward "Child Beta")
            (beginning-of-line)
            (let* ((stored (org-id-store-link))
                   (stored-plist org-store-link-plist))
              (with-current-buffer (find-file-noselect file-b)
                (org-mode)
                (goto-char (point-min))
                (let (opened-option opened-target fallback-opened fallback-error)
                  (org-id-open "parent::*Child Beta" nil)
                  (setq opened-option
                        (list (file-name-nondirectory (buffer-file-name))
                              (org-get-heading t t t t)
                              (line-number-at-pos)))
                  (org-id-open "parent::beta-target" nil)
                  (setq opened-target
                        (list (file-name-nondirectory (buffer-file-name))
                              (org-get-heading t t t t)
                              (thing-at-point 'line t)))
                  (condition-case err
                      (progn
                        (org-id-open "legacy::id" nil)
                        (setq fallback-opened
                              (list (file-name-nondirectory (buffer-file-name))
                                    (org-get-heading t t t t)
                                    (line-number-at-pos))))
                    (error (setq fallback-error
                                 (cons (car err) (cdr err)))))
                  (list (replace-regexp-in-string
                         "id:[0-9a-f-]+\\'" "id:<generated>" stored)
                        (plist-put
                         (copy-sequence stored-plist)
                         :link
                         (replace-regexp-in-string
                          "id:[0-9a-f-]+\\'" "id:<generated>"
                          (plist-get stored-plist :link)))
                        opened-option
                        opened-target
                        fallback-opened
                        fallback-error
                        (mapcar (lambda (path)
                                  (file-relative-name path root))
                                (sort (mapcar
                                       #'car
                                       (org-id-hash-to-alist
                                        org-id-locations))
                                      #'string<))))))))
      (when (get-file-buffer file-a) (kill-buffer (get-file-buffer file-a)))
      (when (get-file-buffer file-b) (kill-buffer (get-file-buffer file-b)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_id_get_force_copy_marker_override_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"parent-id\" \"parent-id\" nil \"parent-id\" t t t 0 \"Child\" \"Child\" t \"override.org\" (\"ids.org\" \"override.org\") \"* Parent\\n:PROPERTIES:\\n:ID: parent-id\\n:END:\\n** Child\\n:PROPERTIES:\\n:ID:       <forced-id>\\n:END:\\nBody\\n* Numeric\\n:PROPERTIES:\\n:ID: 123\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-id)
  (let* ((root (make-temp-file "org-id-get" t))
         (file (expand-file-name "ids.org" root))
         (override (expand-file-name "override.org" root))
         (org-id-locations-file (expand-file-name "ids.el" root))
         (org-id-track-globally t)
         (org-id-method 'org)
         (org-id-prefix "Org")
         (org-id-locations (make-hash-table :test 'equal)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* Parent\n:PROPERTIES:\n:ID: parent-id\n:END:\n")
            (insert "** Child\nBody\n")
            (insert "* Numeric\n:PROPERTIES:\n:ID: 123\n:END:\n"))
          (with-current-buffer (find-file-noselect file)
            (org-mode)
            (org-id-update-id-locations (list file) t)
            (goto-char (point-min))
            (search-forward "Parent")
            (beginning-of-line)
            (let* ((parent-marker (copy-marker (point)))
                   (parent-element (org-element-at-point))
                   (parent-by-marker (org-id-get parent-marker))
                   (parent-by-element (org-id-get parent-element))
                   child-created forced copied lookup-symbol lookup-number
                   override-id override-location)
              (search-forward "Child")
              (beginning-of-line)
              (let ((own-before (org-id-get))
                    (inherit-before (org-id-get nil nil nil t)))
                (setq child-created (org-id-get nil 'create "child"))
                (org-id-copy)
                (setq copied (current-kill 0 t))
                (setq forced (org-id-get-create 'force))
                (setq lookup-symbol (org-id-find 'parent-id t))
                (setq lookup-number (org-id-find 123 t))
                (with-temp-buffer
                  (org-mode)
                  (insert "* Temp\n")
                  (goto-char (point-min))
                  (let ((org-id-overriding-file-name override))
                    (setq override-id (org-id-get nil 'create "tmp")))
                  (setq override-location
                        (file-relative-name
                         (gethash override-id org-id-locations)
                         root)))
                (move-marker parent-marker nil)
                (list parent-by-marker
                      parent-by-element
                      own-before
                      inherit-before
                      (string-prefix-p "child:" child-created)
                      (equal copied child-created)
                      (not (equal forced child-created))
                      (string-match-p "\\`Org:" forced)
                      (and lookup-symbol
                           (with-current-buffer (marker-buffer lookup-symbol)
                             (org-get-heading t t t t)))
                      (and lookup-number
                           (with-current-buffer (marker-buffer lookup-number)
                             (org-get-heading t t t t)))
                      (string-prefix-p "tmp:" override-id)
                      override-location
                      (sort (mapcar
                             (lambda (path)
                               (file-relative-name path root))
                             (mapcar #'car
                                     (org-id-hash-to-alist org-id-locations)))
                            #'string<)
                      (replace-regexp-in-string
                       (regexp-quote forced)
                       "<forced-id>"
                       (replace-regexp-in-string
                        (regexp-quote child-created)
                        "<child-id>"
                        (buffer-substring-no-properties
                         (point-min) (point-max)))))))))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_id_link_move_reload_visibility_navigation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"id:deep-id\" (:link \"id:deep-id\" :description \"Deep Target\" :type \"id\") 3 ((\"id\" \"deep-id\" \"id:deep-id\" \"[[id:deep-id][Deep ID]] \" 19 nil) (\"custom-id\" \"deep-custom\" \"#deep-custom\" \"[[#deep-custom][Deep Custom]] \" 19 nil) (\"fuzzy\" \"*Deep Target\" \"*Deep Target\" \"[[*Deep Target][Deep Fuzzy]] \" 19 nil) (\"fuzzy\" \"radio-deep\" \"radio-deep\" \"[[radio-deep][Radio]]\" 19 nil)) \"[[id:deep-id][Deep ID]] [[#deep-custom][Deep Custom]] [[*Deep Target][Deep Fuzzy]] [[radio-deep][Radio]].\" (#(\"Deep Target\" 0 11 (face org-level-3)) 10 nil) (#(\"Deep Target\" 0 11 (face org-level-3)) 10 \"deep-custom\") (#(\"Deep Target\" 0 11 (face org-level-3)) 3 10 nil) (#(\"Deep Target\" 0 11 (face org-level-3)) 3 10) ((\"https\" \"//example.test\" \"https://example.test\" \"web\") (\"id\" \"deep-id\" \"id:deep-id\" \"Deep ID\") (\"custom-id\" \"deep-custom\" \"#deep-custom\" \"Deep Custom\") (\"fuzzy\" \"*Deep Target\" \"*Deep Target\" \"Deep Fuzzy\") (\"fuzzy\" \"radio-deep\" \"radio-deep\" \"Radio\")) ((19 \"[[id:deep-id][Deep ID]] [[#deep-custom][Deep Custom]] [[*Deep Target][Deep Fuzzy]] [[radio-deep][Radio]].\") (19 \"[[id:deep-id][Deep ID]] [[#deep-custom][Deep Custom]] [[*Deep Target][Deep Fuzzy]] [[radio-deep][Radio]].\") (19 \"[[id:deep-id][Deep ID]] [[#deep-custom][Deep Custom]] [[*Deep Target][Deep Fuzzy]] [[radio-deep][Radio]].\") (19 \"[[id:deep-id][Deep ID]] [[#deep-custom][Deep Custom]] [[*Deep Target][Deep Fuzzy]] [[radio-deep][Radio]].\") (19 \"[[id:deep-id][Deep ID]] [[#deep-custom][Deep Custom]] [[*Deep Target][Deep Fuzzy]] [[radio-deep][Radio]].\")) \"#+TITLE: ID Link Nav\\n* Project\\n:PROPERTIES:\\n:ID: project-id\\n:END:\\n** Alpha\\n:PROPERTIES:\\n:ID: alpha-id\\n:END:\\n*** Deep Target\\n:PROPERTIES:\\n:ID: deep-id\\n:CUSTOM_ID: deep-custom\\n:END:\\nDeep body with <<radio-deep>> and [[https://example.test][web]].\\n** Beta\\nBeta body.\\n* Links\\n[[id:deep-id][Deep ID]] [[#deep-custom][Deep Custom]] [[*Deep Target][Deep Fuzzy]] [[radio-deep][Radio]].\\n\")""##
    ]];
    crate::common::assert_oracle_parity_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-id)
  (require 'org-cycle)
  (require 'org-fold)
  (require 'ol)
  (let* ((root (make-temp-file "org-id-link-nav" t))
         (file (expand-file-name "links.org" root))
         (org-id-locations-file (expand-file-name "ids.el" root))
         (org-id-track-globally t)
         (org-id-link-to-org-use-id 'use-existing)
         (org-id-link-use-context t)
         (org-link-descriptive t)
         (org-link-context-for-files t)
         (org-stored-links nil)
         (org-store-link-plist nil))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "#+TITLE: ID Link Nav\n")
            (insert "* Project\n:PROPERTIES:\n:ID: project-id\n:END:\n")
            (insert "** Alpha\n:PROPERTIES:\n:ID: alpha-id\n:END:\n")
            (insert "*** Deep Target\n:PROPERTIES:\n:ID: deep-id\n:CUSTOM_ID: deep-custom\n:END:\n")
            (insert "Deep body with <<radio-deep>> and [[https://example.test][web]].\n")
            (insert "** Beta\nBeta body.\n")
            (insert "* Links\n")
            (insert "[[id:deep-id][Deep ID]] [[#deep-custom][Deep Custom]] [[*Deep Target][Deep Fuzzy]] [[radio-deep][Radio]].\n"))
          (org-id-update-id-locations (list file) t)
          (org-id-locations-save)
          (clrhash org-id-locations)
          (org-id-locations-load)
          (with-current-buffer (find-file-noselect file)
            (org-mode)
            (let (stored stored-plist before-nav after-toggle opened-id
                  opened-custom moved-open search-open link-summary final-nav)
              (goto-char (point-min))
              (search-forward "Deep Target")
              (beginning-of-line)
              (setq stored (org-id-store-link)
                    stored-plist (copy-sequence org-store-link-plist))
              (org-fold-hide-sublevels 1)
              (goto-char (point-min))
              (search-forward "* Links")
              (beginning-of-line)
              (setq before-nav
                    (let (rows)
                      (dotimes (_ 4)
                        (org-next-link)
                        (let ((link (org-element-context)))
                          (push (list
                                 (org-element-property :type link)
                                 (org-element-property :path link)
                                 (org-element-property :raw-link link)
                                 (buffer-substring-no-properties
                                  (org-element-begin link)
                                  (org-element-end link))
                                 (line-number-at-pos)
                                 (not (null (org-invisible-p (point)))))
                                rows)))
                      (nreverse rows)))
              (goto-char (point-min))
              (search-forward "Deep ID")
              (org-toggle-link-display)
              (font-lock-ensure (point-min) (point-max))
              (setq after-toggle
                    (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position)))
              (org-toggle-link-display)
              (goto-char (point-min))
              (search-forward "Deep ID")
              (org-open-at-point)
              (setq opened-id
                    (list (org-get-heading t t t t)
                          (line-number-at-pos)
                          (not (null
                                (org-invisible-p
                                 (line-beginning-position))))))
              (goto-char (point-min))
              (search-forward "Deep Custom")
              (org-open-at-point)
              (setq opened-custom
                    (list (org-get-heading t t t t)
                          (line-number-at-pos)
                          (org-entry-get nil "CUSTOM_ID")))
              (goto-char (point-min))
              (search-forward "Deep Target")
              (beginning-of-line)
              (org-cut-subtree)
              (goto-char (point-min))
              (search-forward "Beta")
              (beginning-of-line)
              (org-paste-subtree 3)
              (org-fold-hide-sublevels 1)
              (org-id-open "deep-id" nil)
              (setq moved-open
                    (list (org-get-heading t t t t)
                          (org-outline-level)
                          (line-number-at-pos)
                          (not (null
                                (org-invisible-p
                                 (line-beginning-position))))))
              (goto-char (point-min))
              (search-forward "Links")
              (org-link-search "*Deep Target" nil t)
              (setq search-open
                    (list (org-get-heading t t t t)
                          (org-outline-level)
                          (line-number-at-pos)))
              (setq link-summary
                    (org-element-map (org-element-parse-buffer) 'link
                      (lambda (link)
                        (list (org-element-property :type link)
                              (org-element-property :path link)
                              (org-element-property :raw-link link)
                              (and (org-element-contents-begin link)
                                   (buffer-substring-no-properties
                                    (org-element-contents-begin link)
                                    (org-element-contents-end link)))))))
              (goto-char (point-min))
              (search-forward "* Links")
              (beginning-of-line)
              (setq final-nav
                    (let (rows)
                      (dotimes (_ 5)
                        (org-next-link)
                        (push (list (line-number-at-pos)
                                    (buffer-substring-no-properties
                                     (line-beginning-position)
                                     (line-end-position)))
                              rows))
                      (nreverse rows)))
              (list stored
                    stored-plist
                    (hash-table-count org-id-locations)
                    before-nav
                    after-toggle
                    opened-id
                    opened-custom
                    moved-open
                    search-open
                    link-summary
                    final-nav
                     (replace-regexp-in-string
                      (regexp-quote root)
                      "<root>"
                      (buffer-substring-no-properties
                       (point-min) (point-max)))))))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_id_create_find_open_cross_file_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 59 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-id)
  (let* ((root (make-temp-file "org-id-cross-deep" t))
         (file-a (expand-file-name "a.org" root))
         (file-b (expand-file-name "b.org" root))
         (org-id-locations-file (expand-file-name ".ids" root))
         (org-id-track-globally t))
    (unwind-protect
        (progn
          (with-temp-file file-a
            (insert "* Alpha\n")
            (insert ":PROPERTIES:\n:ID: alpha-fixed-id\n:END:\n")
            (insert "Alpha body.\n")
            (insert "** Child\n")
            (insert ":PROPERTIES:\n:ID: child-fixed-id\n:END:\n")
            (insert "Child body.\n"))
          (with-temp-file file-b
            (insert "* Beta\n")
            (insert "Link to [[id:alpha-fixed-id][Alpha]].\n")
            (insert "Link to [[id:child-fixed-id][Child]].\n"))
          (org-id-update-id-locations (list file-a file-b) t)
          (let ((alpha-marker (org-id-find "alpha-fixed-id" t))
                (child-marker (org-id-find "child-fixed-id" t))
                (alpha-file (org-id-find-id-file "alpha-fixed-id"))
                (child-file (org-id-find-id-file "child-fixed-id"))
                (loc-count (hash-table-count org-id-locations)))
            (with-current-buffer (find-file-noselect file-b)
              (org-mode)
              (goto-char (point-min))
              (search-forward "Alpha")
              (beginning-of-line)
              (let ((link-ctx (org-element-context)))
                (list (list 'alpha-marker
                            (markerp alpha-marker)
                            (and alpha-marker (marker-position alpha-marker))
                            (and alpha-marker
                                 (replace-regexp-in-string
                                  (regexp-quote root) "<root>"
                                  (buffer-file-name
                                   (marker-buffer alpha-marker)))))
                      (list 'child-marker
                            (markerp child-marker)
                            (and child-marker (marker-position child-marker)))
                      (list 'alpha-file
                            (and alpha-file
                                 (replace-regexp-in-string
                                  (regexp-quote root) "<root>" alpha-file)))
                      (list 'child-file
                            (and child-file
                                 (replace-regexp-in-string
                                  (regexp-quote root) "<root>" child-file)))
                      loc-count
                      (list 'link-ctx
                            (org-element-property :type link-ctx)
                            (org-element-property :path link-ctx))
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))
      (delete-directory root t))))"##,
        expect,
    );
}
