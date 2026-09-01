use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_custom_link_follow_export_store_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"abc%20def\" (4))) \"<p>\\n[html:abc:Desc]</p>\\n\" \"[ascii:abc:]\\n\" (:type \"probe\" :link \"probe:stored\" :description \"Stored Probe\") nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ol)
  (let (follow-calls)
    (org-link-set-parameters
     "probe"
     :follow (lambda (path arg)
               (push (list path arg) follow-calls))
     :export (lambda (path desc backend _info)
               (format "[%s:%s:%s]" backend path (or desc "")))
     :store (lambda ()
              (org-link-store-props
               :type "probe"
               :link "probe:stored"
               :description "Stored Probe")
              t))
    (with-temp-buffer
      (org-mode)
      (insert "[[probe:abc%20def][Desc]]\n")
      (goto-char (point-min))
      (let ((link (org-element-context)))
        (org-link-open link '(4))
        (let ((html (org-export-string-as
                     "[[probe:abc][Desc]]" 'html t))
              (ascii (org-export-string-as
                      "[[probe:abc]]" 'ascii t))
              (org-stored-links nil)
              (org-store-link-plist nil))
          (org-store-link nil nil)
          (list (nreverse follow-calls)
                html
                ascii
                org-store-link-plist
                org-stored-links))))))"##,
        expect,
    );
}

#[test]
fn org_custom_link_activation_completion_export_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    crate::common::assert_oracle_parity(
        r##"(progn
  (require 'org)
  (require 'ol)
  (let ((follow-calls nil)
        (store-calls nil)
        (complete-calls nil)
        (activate-calls nil)
        (org-stored-links nil)
        (org-store-link-plist nil))
    (org-link-set-parameters
     "combo"
     :follow (lambda (path arg)
               (push (list path arg) follow-calls))
     :export (lambda (path desc backend info)
               (format "{%s|%s|%s|toc=%S}"
                       backend path (or desc "")
                       (plist-get info :with-toc)))
     :store (lambda ()
              (push 'store store-calls)
              (org-link-store-props
               :type "combo"
               :link "combo:stored/value"
               :description "Stored combo")
              t)
     :complete (lambda (&optional arg)
                 (push arg complete-calls)
                 "combo:completed/path")
     :face (lambda (path) (if (string-match-p "warn" path)
                              'org-warning
                            'org-link))
     :display "COMBO"
     :activate-func (lambda (start end path bracketp)
                      (push (list (- start (point-min))
                                  (- end (point-min))
                                  path bracketp)
                            activate-calls)
                      (put-text-property start end 'combo-path path)))
    (with-temp-buffer
      (org-mode)
      (insert "#+OPTIONS: toc:nil\n")
      (insert "[[combo:ok/path][Okay]] [[combo:warn/path]]\n")
      (font-lock-ensure (point-min) (point-max))
      (let ((props
             (mapcar
              (lambda (needle)
                (save-excursion
                  (goto-char (point-min))
                  (search-forward needle)
                  (list needle
                        (get-text-property (match-beginning 0) 'face)
                        (get-text-property (match-beginning 0) 'display)
                        (get-text-property (match-beginning 0) 'combo-path)
                        (get-text-property (match-beginning 0)
                                           'mouse-face)
                        (keymapp (get-text-property
                                  (match-beginning 0) 'keymap)))))
              '("Okay" "combo:warn"))))
        (goto-char (point-min))
        (search-forward "Okay")
        (org-open-at-point '(4))
        (let ((completion (org-link-complete-file '(4)))
              (html (org-export-string-as
                     "[[combo:ok/path][Okay]]" 'html t
                     '(:with-toc nil)))
              (ascii (org-export-string-as
                      "[[combo:warn/path]]" 'ascii t
                      '(:with-toc nil)))
              (org-out (org-export-string-as
                        "[[combo:ok/path][Okay]]" 'org t
                        '(:with-toc nil))))
          (org-store-link nil nil)
          (list props
                (nreverse activate-calls)
                (nreverse follow-calls)
                completion
                (nreverse complete-calls)
                html
                ascii
                org-out
                (nreverse store-calls)
                org-store-link-plist
                org-stored-links
                   (buffer-substring-no-properties
                    (point-min) (point-max))))))))"##,
    );
}

#[test]
fn org_link_search_fuzzy_target_radio_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""No match - create this as a new heading? (yes or no) OK (((\"custom-id\" \"alpha-id\" \"#alpha-id\" nil \"Custom ID\") (\"fuzzy\" \"*Heading Beta\" \"*Heading Beta\" nil \"Star Link\") (\"fuzzy\" \"radio\" \"radio\" nil \"Radio Link\") (\"https\" \"//example.org\" \"https://example.org\" nil nil)) (\"custom-id\" \"alpha-id\" \"#alpha-id\" nil \"Custom ID\") (\"fuzzy\" \"*Heading Beta\" \"*Heading Beta\" nil \"Star Link\") (\"fuzzy\" \"radio\" \"radio\" nil \"Radio Link\") (\"https\" \"//example.org\" \"https://example.org\" nil nil) (found found (err end-of-file \"Error reading from stdin\")) \"* Heading Alpha\\n:PROPERTIES:\\n:CUSTOM_ID: alpha-id\\n:END:\\nAlpha body.\\n\\n* Heading Beta\\nBeta body with <<<radio>>> target.\\n\\n* Heading Gamma :tag:\\nGamma body.\\n\\nLink to [[#alpha-id][Custom ID]].\\nLink to [[*Heading Beta][Star Link]].\\nLink to [[radio][Radio Link]].\\nPlain https://example.org link.\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* Heading Alpha\n")
    (insert ":PROPERTIES:\n:CUSTOM_ID: alpha-id\n:END:\n")
    (insert "Alpha body.\n\n")
    (insert "* Heading Beta\n")
    (insert "Beta body with <<<radio>>> target.\n\n")
    (insert "* Heading Gamma :tag:\n")
    (insert "Gamma body.\n\n")
    (insert "Link to [[#alpha-id][Custom ID]].\n")
    (insert "Link to [[*Heading Beta][Star Link]].\n")
    (insert "Link to [[radio][Radio Link]].\n")
    (insert "Plain https://example.org link.\n")
    ;; Parse all links
    (let* ((tree (org-element-parse-buffer))
           (links
            (org-element-map tree 'link
              (lambda (lk)
                (list (org-element-property :type lk)
                      (org-element-property :path lk)
                      (org-element-property :raw-link lk)
                      (org-element-property :search-option lk)
                      (and (org-element-contents-begin lk)
                           (buffer-substring-no-properties
                            (org-element-contents-begin lk)
                            (org-element-contents-end lk)))))))
           ;; Check link descriptions
           (custom-link (nth 0 links))
           (star-link (nth 1 links))
           (radio-link (nth 2 links))
           (plain-link (nth 3 links))
           ;; Try org-link-search
           (search-results
            (list
             (condition-case err
                 (progn (org-link-search "#alpha-id") 'found)
               (error (cons 'err err)))
             (condition-case err
                 (progn (org-link-search "*Heading Beta") 'found)
               (error (cons 'err err)))
             (condition-case err
                 (progn (org-link-search "radio") 'found)
               (error (cons 'err err))))))
      (list links
            custom-link
            star-link
            radio-link
            plain-link
            search-results
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_id_cross_file_folded_context_store_open_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((store-from-hidden nil) (store-from-visible nil) (link-type nil nil) (store-from-b \"[[file:<root>/b.org::*Refs][Refs]]\") (find-task-ccc t 162 \"<root>/a.org\") (find-proj t 1 \"<root>/a.org\")) \"\")""#
    ]];
    crate::common::assert_oracle_parity_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-id)
  (require 'ol)
  (let* ((root (make-temp-file "org-id-cross" t))
         (file-a (expand-file-name "a.org" root))
         (file-b (expand-file-name "b.org" root))
         (org-id-locations-file (expand-file-name ".org-id-locations" root))
         (org-id-track-globally t)
         (org-stored-links nil))
    (unwind-protect
        (progn
          (with-temp-file file-a
            (insert "* Project :work:\n")
            (insert ":PROPERTIES:\n:ID: proj-aaa-111\n:END:\n")
            (insert "Project body.\n")
            (insert "** TODO Task one\n")
            (insert ":PROPERTIES:\n:ID: task-bbb-222\n:END:\n")
            (insert "Task one body.\n")
            (insert "*** Detail\n")
            (insert "Detail body.\n")
            (insert "** DONE Task two\n")
            (insert ":PROPERTIES:\n:ID: task-ccc-333\n:END:\n")
            (insert "Task two body.\n"))
          (with-temp-file file-b
            (insert "* Refs\n")
            (insert "Link to [[id:task-bbb-222][Task one]].\n")
            (insert "Link to [[id:proj-aaa-111][Project]].\n"))
          (org-id-update-id-locations (list file-a file-b) t)
          (let ((results nil))
            (with-current-buffer (find-file-noselect file-a)
              (org-mode)
              (org-fold-hide-sublevels 1)
              (goto-char (point-min))
              (search-forward "Task one")
              (beginning-of-line)
              (let ((org-link-descriptive t)
                    (link (org-store-link nil)))
                (push (list 'store-from-hidden
                            (and link
                                 (string-match-p "id:task-bbb-222" link)))
                      results))
              (org-fold-show-all)
              (goto-char (point-min))
              (search-forward "Task two")
              (beginning-of-line)
              (let ((link (org-store-link nil)))
                (push (list 'store-from-visible
                            (and link
                                 (string-match-p "id:task-ccc-333" link)))
                      results))
              (kill-buffer))
            (with-current-buffer (find-file-noselect file-b)
              (org-mode)
              (goto-char (point-min))
              (search-forward "Task one")
              (beginning-of-line)
              (let ((link-info (org-element-context)))
                (push (list 'link-type
                            (org-element-property :type link-info)
                            (org-element-property :path link-info))
                      results))
              (let ((stored (org-store-link nil)))
                (push (list 'store-from-b stored) results))
              (kill-buffer))
            (let ((marker (org-id-find "task-ccc-333" t)))
              (push (list 'find-task-ccc
                          (markerp marker)
                          (and marker (marker-position marker))
                          (and marker
                               (buffer-file-name
                                (marker-buffer marker))))
                    results))
            (let ((marker (org-id-find "proj-aaa-111" t)))
              (push (list 'find-proj
                          (markerp marker)
                          (and marker (marker-position marker))
                          (and marker
                               (buffer-file-name
                                (marker-buffer marker))))
                    results))
            (list (mapcar
                   (lambda (entry)
                     (if (and (listp entry) (eq (car entry) 'find-task-ccc))
                         (list (nth 0 entry)
                               (nth 1 entry)
                               (nth 2 entry)
                               (and (nth 3 entry)
                                    (replace-regexp-in-string
                                     (regexp-quote root) "<root>"
                                     (nth 3 entry))))
                       (if (and (listp entry) (eq (car entry) 'find-proj))
                           (list (nth 0 entry)
                                 (nth 1 entry)
                                 (nth 2 entry)
                                 (and (nth 3 entry)
                                      (replace-regexp-in-string
                                       (regexp-quote root) "<root>"
                                       (nth 3 entry))))
                         (if (and (listp entry) (eq (car entry) 'store-from-b))
                             (list (car entry)
                                   (and (nth 1 entry)
                                        (replace-regexp-in-string
                                         (regexp-quote root) "<root>"
                                         (nth 1 entry))))
                           entry))))
                   (nreverse results))
                  (replace-regexp-in-string
                   (regexp-quote root) "<root>"
                   (mapconcat #'identity
                              (mapcar (lambda (s)
                                        (replace-regexp-in-string
                                         (regexp-quote root) "<root>" s))
                                      org-stored-links)
                              "\n")))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_link_escape_decode_make_string_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"file:has space/ümlaut?#x\" \"file:has space/ümlaut?#x\" \"a%20b%2F%C3%A7\" \"a b/ç\" \"[[https://example.org/a b][Example]]\" \"[[https://example.org/a b]]\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ol)
  (let* ((raw "file:has space/ümlaut?#x")
         (escaped (org-link-escape raw))
         (unescaped (org-link-unescape escaped))
         (encoded (org-link-encode "a b/ç" '(?\s ?/ ?ç)))
         (decoded (org-link-decode encoded)))
    (list escaped
          unescaped
          encoded
          decoded
          (org-link-make-string "https://example.org/a b" "Example")
          (org-link-make-string "https://example.org/a b"))))"##,
        expect,
    );
}

#[test]
fn org_link_store_props_mail_date_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-time-stamp-format)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ol)
  (let ((org-link-from-user-regexp "me@example\\.org")
        org-store-link-plist)
    (org-link-store-props
     :type "mail"
     :from "Me <me@example.org>"
     :to "Ada <ada@example.org>"
     :date "Wed, 27 May 2026 09:30:00 +0000"
     :subject "Hello")
    (org-link-add-props :link "mailto:ada@example.org" :description "Hello")
    (list (plist-get org-store-link-plist :fromname)
          (plist-get org-store-link-plist :fromaddress)
          (plist-get org-store-link-plist :toname)
          (plist-get org-store-link-plist :toaddress)
          (plist-get org-store-link-plist :fromto)
          (plist-get org-store-link-plist :date-timestamp)
          (plist-get org-store-link-plist :date-timestamp-inactive)
          (plist-get org-store-link-plist :link)
          (plist-get org-store-link-plist :description))))"##,
        expect,
    );
}

#[test]
fn org_link_navigation_toggle_context_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ol)
  (with-temp-buffer
    (org-mode)
    (insert "* Links\n")
    (insert "[[https://example.org/a][Alpha]] plain https://example.org/b\n")
    (insert "[[file:/tmp/demo.txt::12][File line]] and [[#target][Target]]\n")
    (insert "* Target\n:PROPERTIES:\n:CUSTOM_ID: target\n:END:\n")
    (font-lock-ensure (point-min) (point-max))
    (let ((snap
           (lambda (label)
             (let ((context (org-element-context)))
               (list label
                     (point)
                     (org-element-type context)
                     (org-element-property :type context)
                     (org-element-property :path context)
                     (org-element-property :raw-link context)
                     org-link-descriptive
                     (get-text-property (point) 'invisible)
                     (get-text-property (point) 'display))))))
      (goto-char (point-min))
      (org-next-link)
      (let ((first (funcall snap 'first)))
        (org-next-link)
        (let ((second (funcall snap 'second)))
          (org-next-link)
          (let ((third (funcall snap 'third)))
            (org-previous-link)
            (let ((back (funcall snap 'back)))
              (org-toggle-link-display)
              (font-lock-ensure (point-min) (point-max))
              (let ((after-toggle (funcall snap 'toggle)))
                (list first
                      second
                      third
                      back
                      after-toggle
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_link_id_custom_fuzzy_radio_runtime_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"[[id:alpha-id][Alpha Target]]\" (:link \"id:alpha-id\" :description \"Alpha Target\" :type \"id\") ((link-0 206 link \"id\" \"alpha-id\" \"id:alpha-id\" org-link nil org-link) (link-1 232 link \"custom-id\" \"custom-beta\" \"#custom-beta\" org-link nil org-link) (link-2 262 link \"fuzzy\" \"*Plain Target\" \"*Plain Target\" org-link nil org-link) (link-3 293 link \"fuzzy\" \"radio target\" \"radio target\" org-link nil org-link) (raw-0 206 link \"id\" \"alpha-id\" \"id:alpha-id\" nil nil org-link) (raw-1 232 link \"custom-id\" \"custom-beta\" \"#custom-beta\" nil nil org-link) (raw-2 262 link \"fuzzy\" \"*Plain Target\" \"*Plain Target\" nil nil org-link) (raw-3 293 link \"fuzzy\" \"radio target\" \"radio target\" nil nil org-link)) ((\"Alpha ID\" #(\"Alpha Target\" 0 12 (face org-level-1)) 21 \"* Alpha Target\") (\"Beta Custom\" #(\"Beta Target\" 0 11 (face org-level-1)) 103 \"* Beta Target\") (\"Plain Fuzzy\" #(\"Plain Target\" 0 12 (face org-level-1)) 171 \"* Plain Target\") (\"Radio Target\" #(\"Alpha Target\" 0 12 (face org-level-1)) 99 \"Alpha body with <<radio target>>.\")) ((\"id\" \"alpha-id\" \"id:alpha-id\" \"Alpha ID\") (\"custom-id\" \"custom-beta\" \"#custom-beta\" \"Beta Custom\") (\"fuzzy\" \"*Plain Target\" \"*Plain Target\" \"Plain Fuzzy\") (\"fuzzy\" \"radio target\" \"radio target\" \"Radio Target\")) \"#+TITLE: Mixed Links\\n* Alpha Target\\n:PROPERTIES:\\n:ID: alpha-id\\n:END:\\nAlpha body with <<radio target>>.\\n* Beta Target\\n:PROPERTIES:\\n:CUSTOM_ID: custom-beta\\n:END:\\nBeta body.\\n* Plain Target\\nPlain body.\\n* Links\\n[[id:alpha-id][Alpha ID]] [[#custom-beta][Beta Custom]] [[*Plain Target][Plain Fuzzy]] [[radio target][Radio Target]]\\n\\n\" \"1 Alpha Target\\n==============\\n\\n  Alpha body with .\\n\\n\\n2 Beta Target\\n=============\\n\\n  Beta body.\\n\\n\\n3 Plain Target\\n==============\\n\\n  Plain body.\\n\\n\\n4 Links\\n=======\\n\\n  [Alpha ID] [Beta Custom] [Plain Fuzzy] [Radio Target]\\n\\n\\n[Alpha ID] See section 1\\n\\n[Beta Custom] See section 2\\n\\n[Plain Fuzzy] See section 3\\n\\n[Radio Target] See figure (1)\\n\")""##
    ]];
    crate::common::assert_oracle_parity_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-id)
  (require 'ol)
  (require 'ox-ascii)
  (let* ((root (make-temp-file "org-link-mixed" t))
         (file (expand-file-name "mixed.org" root))
         (org-id-locations-file (expand-file-name "ids.el" root))
         (org-id-track-globally t)
         (org-id-link-to-org-use-id 'use-existing)
         (org-link-context-for-files t)
         (org-link-descriptive t)
         (org-stored-links nil)
         (org-store-link-plist nil))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "#+TITLE: Mixed Links\n")
            (insert "* Alpha Target\n")
            (insert ":PROPERTIES:\n:ID: alpha-id\n:END:\n")
            (insert "Alpha body with <<radio target>>.\n")
            (insert "* Beta Target\n")
            (insert ":PROPERTIES:\n:CUSTOM_ID: custom-beta\n:END:\n")
            (insert "Beta body.\n")
            (insert "* Plain Target\nPlain body.\n")
            (insert "* Links\n"))
          (org-id-update-id-locations (list file) t)
          (with-current-buffer (find-file-noselect file)
            (org-mode)
            (goto-char (point-min))
            (search-forward "Alpha Target")
            (beginning-of-line)
            (let ((stored-alpha (org-store-link nil nil))
                  (stored-plist org-store-link-plist))
              (goto-char (point-min))
              (search-forward "* Links")
              (end-of-line)
              (insert "\n")
              (insert "[[id:alpha-id][Alpha ID]] ")
              (insert "[[#custom-beta][Beta Custom]] ")
              (insert "[[*Plain Target][Plain Fuzzy]] ")
              (insert "[[radio target][Radio Target]]\n")
              (font-lock-ensure (point-min) (point-max))
              (let ((snap
                     (lambda (label)
                       (let ((ctx (org-element-context)))
                         (list label
                               (- (point) (point-min))
                               (org-element-type ctx)
                               (org-element-property :type ctx)
                               (org-element-property :path ctx)
                               (org-element-property :raw-link ctx)
                               (get-text-property (point) 'invisible)
                               (get-text-property (point) 'display)
                               (get-text-property (point) 'face)))))
                    link-snaps open-snaps)
                (goto-char (point-min))
                (dotimes (i 4)
                  (org-next-link)
                  (push (funcall snap (intern (format "link-%d" i)))
                        link-snaps))
                (org-toggle-link-display)
                (font-lock-ensure (point-min) (point-max))
                (goto-char (point-min))
                (dotimes (i 4)
                  (org-next-link)
                  (push (funcall snap (intern (format "raw-%d" i)))
                        link-snaps))
                (dolist (needle '("Alpha ID" "Beta Custom" "Plain Fuzzy"
                                  "Radio Target"))
                  (goto-char (point-min))
                  (search-forward needle)
                  (push (save-excursion
                          (org-open-at-point)
                          (list needle
                                (org-get-heading t t t t)
                                (- (point) (point-min))
                                (buffer-substring-no-properties
                                 (line-beginning-position)
                                 (line-end-position))))
                        open-snaps))
                (let* ((tree (org-element-parse-buffer))
                       (links
                        (org-element-map tree 'link
                          (lambda (link)
                            (list (org-element-property :type link)
                                  (org-element-property :path link)
                                  (org-element-property :raw-link link)
                                  (and (org-element-contents-begin link)
                                       (buffer-substring-no-properties
                                        (org-element-contents-begin link)
                                        (org-element-contents-end link)))))))
                       (ascii
                        (org-export-string-as
                         (buffer-substring-no-properties
                          (point-min) (point-max))
                         'ascii t '(:with-toc nil))))
                  (list stored-alpha
                        stored-plist
                        (nreverse link-snaps)
                        (nreverse open-snaps)
                        links
                        (replace-regexp-in-string
                         (regexp-quote root)
                         "<root>"
                         (buffer-substring-no-properties
                          (point-min) (point-max)))
                        (replace-regexp-in-string
                         (regexp-quote root)
                         "<root>"
                         ascii)))))))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_link_abbrev_expand_open_from_string_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ol)
  (let ((org-link-abbrev-alist-local
         '(("bug" . "https://bugs.example/%s")
           ("hex" . "https://hex.example/%h")))
        (org-link-abbrev-alist
         '(("doc" . "https://docs.example/%s")))
        calls)
    (org-link-set-parameters
     "probe-open"
     :follow (lambda (path arg) (push (list path arg) calls)))
    (with-temp-buffer
      (org-mode)
      (insert "* H\n")
      (insert "See <<radio target>> and [[probe-open:value%20x][open me]].\n")
      (insert "* Destination\n:PROPERTIES:\n:CUSTOM_ID: dest\n:END:\n")
      (goto-char (point-min))
      (search-forward "probe-open")
      (org-link-open-from-string "[[probe-open:from-string][Open]]" '(4))
      (let ((custom (save-excursion
                      (org-link-open-from-string "[[#dest]]")
                      (org-get-heading t t t t)))
            (radio (save-excursion
                     (org-link-open-from-string "[[radio target]]")
                     (buffer-substring-no-properties
                      (point) (line-end-position)))))
        (list (mapcar #'org-link-expand-abbrev
                      '("bug:123" "doc:topic" "hex:a b/c" "plain:x"))
              (nreverse calls)
              custom
              radio
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_link_store_region_file_context_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"[[file:<file>::*Beta][Beta]]\" \"[[file:<file>::*Beta][Beta]]\" nil nil \"* Alpha\\nBody one\\n** Beta\\nBody two\\n\")""#
    ]];
    crate::common::assert_oracle_parity_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'ol)
  (let ((file (make-temp-file "org-link-store" nil ".org")))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* Alpha\nBody one\n** Beta\nBody two\n"))
          (with-current-buffer (find-file-noselect file)
            (org-mode)
            (setq org-stored-links nil
                  org-store-link-plist nil)
            (goto-char (point-min))
            (search-forward "Beta")
            (let ((at-heading (org-store-link nil nil)))
              (push-mark (line-beginning-position) t t)
              (end-of-line)
              (let ((from-region (org-store-link nil nil))
                    (plist org-store-link-plist)
                    (stored org-stored-links))
                (list (replace-regexp-in-string
                       (regexp-quote file) "<file>" at-heading)
                      (replace-regexp-in-string
                       (regexp-quote file) "<file>" from-region)
                      (plist-get plist :description)
                      (mapcar (lambda (entry)
                                (list (replace-regexp-in-string
                                       (regexp-quote file) "<file>"
                                       (car entry))
                                      (cdr entry)))
                              stored)
                      (buffer-substring-no-properties
                       (point-min) (point-max)))))))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (when (file-exists-p file) (delete-file file)))))"##,
        expect,
    );
}

#[test]
fn org_link_precise_target_region_named_heading_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ol)
  (with-temp-buffer
    (org-mode)
    (insert "#+NAME: tbl\n")
    (insert "| a | b |\n| 1 | 2 |\n")
    (insert "* TODO [#A] Target :tag:\n")
    (insert ":PROPERTIES:\n:CUSTOM_ID: custom-target\n:END:\n")
    (insert "Body region words here.\n")
    (let ((offset (lambda (row)
                    (and row
                         (list (nth 0 row)
                               (nth 1 row)
                               (- (nth 2 row) (point-min)))))))
      (goto-char (point-min))
      (search-forward "| a |")
      (let ((named (funcall offset (org-link-precise-link-target))))
        (search-forward "Target")
        (let ((heading (funcall offset (org-link-precise-link-target))))
          (search-forward "region words")
          (push-mark (match-beginning 0) t t)
          (goto-char (match-end 0))
          (let ((region (funcall offset (org-link-precise-link-target))))
            (list named
                  heading
                  region
                  (org-link-heading-search-string)
                  (org-link-heading-search-string
                   "TODO [#B] Other [33%] :x:y:")
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_insert_link_stored_region_completion_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (search-failed \"Stored Desc\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ol)
  (let* ((root (make-temp-file "org-insert-link" t))
         (target (expand-file-name "target.org" root))
         (other (expand-file-name "sub/other.org" root))
         (answers '("Stored Desc" "custom:" "Edited Desc"))
         (completions nil)
         (org-link-keep-stored-after-insertion nil)
         (org-link-file-path-type 'relative)
         (org-stored-links
          `(("https://example.org/stored" "Stored Desc")
            (,(concat "file:" target "::*Target Heading") "Target Heading")))
         (org-link-make-description-function
          (lambda (link desc)
            (format "AUTO:%s:%s" link (or desc "")))))
    (unwind-protect
        (progn
          (make-directory (file-name-directory other) t)
          (with-temp-file target
            (insert "* Target Heading\nBody\n"))
          (with-temp-file other (insert "other\n"))
          (org-link-set-parameters
           "custom"
           :complete (lambda (&optional arg)
                       (push arg completions)
                       "custom:path/value")
           :insert-description
           (lambda (link desc)
             (format "DESC:%s:%s" link (or desc ""))))
          (with-temp-buffer
            (setq default-directory root
                  buffer-file-name (expand-file-name "source.org" root))
            (org-mode)
            (insert "* Source\n")
            (insert "Replace this region\n")
            (insert "Edit [[https://old.example/path][Old]].\n")
            (cl-letf (((symbol-function 'org-completing-read)
                       (lambda (prompt collection &rest _)
                         (push (list prompt
                                     (sort
                                      (mapcar (lambda (entry)
                                                (if (consp entry)
                                                    (car entry)
                                                  entry))
                                              collection)
                                      #'string<))
                               completions)
                         (pop answers)))
                      ((symbol-function 'read-string)
                       (lambda (prompt &optional initial &rest _)
                         (push (list prompt initial) completions)
                         (or (pop answers) initial ""))))
              ;; Empty explicit description lets stored-link description win.
              (goto-char (point-min))
              (search-forward "Source")
              (end-of-line)
              (insert "\n")
              (org-insert-link nil "https://example.org/stored" "")
              ;; Region text becomes default description for a file link.
              (goto-char (point-min))
              (search-forward "Replace this region")
              (push-mark (match-beginning 0) t t)
              (goto-char (match-end 0))
              (activate-mark)
              (org-insert-link nil (concat "file:" other) nil)
              ;; Complete custom link type and use :insert-description.
              (goto-char (point-max))
              (insert "\n")
              (org-insert-link nil nil nil)
              ;; Edit an existing bracket link in place.
              (goto-char (point-min))
              (search-forward "Old")
              (org-insert-link nil "https://new.example/path" nil)
              ;; Insert remaining stored links with and without retention.
              (goto-char (point-max))
              (insert "\n")
              (org-insert-all-links '(4) "- " "\n")
              (org-insert-last-stored-link 1)
              (font-lock-ensure (point-min) (point-max))
              (let* ((tree (org-element-parse-buffer))
                     (links
                      (org-element-map tree 'link
                        (lambda (link)
                          (list (org-element-property :type link)
                                (org-element-property :path link)
                                (org-element-property :raw-link link)
                                (and (org-element-contents-begin link)
                                     (buffer-substring-no-properties
                                      (org-element-contents-begin link)
                                      (org-element-contents-end link)))))))
                     (props
                      (mapcar
                       (lambda (needle)
                         (save-excursion
                           (goto-char (point-min))
                           (search-forward needle)
                           (list needle
                                 (get-text-property (match-beginning 0)
                                                    'face)
                                 (get-text-property (match-beginning 0)
                                                    'invisible)
                                 (get-text-property (match-beginning 0)
                                                    'font-lock-fontified))))
                       '("Stored Desc" "Replace this region" "custom:path"
                         "Edited Desc"))))
                (list links
                      props
                      org-stored-links
                      (nreverse completions)
                      (buffer-substring-no-properties
                       (point-min) (point-max)))))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_link_search_targets_names_coderef_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"No match for coderef: call\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ol)
  (with-temp-buffer
    (org-mode)
    (insert "* Alpha\n")
    (insert "<<radio target>> text.\n")
    (insert "#+NAME: named-table\n")
    (insert "| a | b |\n")
    (insert "* Code\n")
    (insert "#+begin_src emacs-lisp -n -r\n")
    (insert "(message \"hi\") ;; (call)\n")
    (insert "#+end_src\n")
    (insert "* Multi   Word\nbody\n")
    (let ((probe
           (lambda (search)
             (goto-char (point-min))
             (let ((kind (org-link-search search nil t)))
               (list search
                     kind
                     (buffer-substring-no-properties
                      (line-beginning-position)
                      (line-end-position))
                     (- (point) (point-min)))))))
      (list (funcall probe "radio target")
            (funcall probe "named-table")
            (funcall probe "(call)")
            (funcall probe "*Multi Word")
            (funcall probe "body")
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_link_open_file_search_targets_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"#alpha-id\" \"<file>\" \"* Alpha\" 0) (\"*Beta\" \"<file>\" \"* Beta\" 60) (\"radio-file-target\" \"<file>\" \"<<radio-file-target>>\" 67) (\"named-block\" \"<file>\" \"#+NAME: named-block\" 89))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ol)
  (let ((file (make-temp-file "org-link-open-file" nil ".org")))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* Alpha\n")
            (insert ":PROPERTIES:\n:CUSTOM_ID: alpha-id\n:END:\n")
            (insert "Alpha body.\n")
            (insert "* Beta\n")
            (insert "<<radio-file-target>>\n")
            (insert "#+NAME: named-block\n")
            (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n"))
          (let ((link-to (lambda (search)
                           (concat "file:" file "::" search))))
            (let (out)
              (dolist (search '("#alpha-id" "*Beta" "radio-file-target"
                                "named-block"))
                (org-link-open-from-string
                 (org-link-make-string (funcall link-to search)) '(16))
                (push (list search
                            "<file>"
                            (buffer-substring-no-properties
                             (line-beginning-position)
                             (line-end-position))
                            (- (point) (point-min)))
                      out))
              (nreverse out))))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (when (file-exists-p file) (delete-file file)))))"##,
        expect,
    );
}

#[test]
fn org_export_resolve_links_reference_matrix_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (org-link-broken \"tbl\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (with-temp-buffer
    (org-mode)
    (insert "#+NAME: tbl\n")
    (insert "#+CAPTION: Table caption\n")
    (insert "| a | b |\n| 1 | 2 |\n\n")
    (insert "* Target heading\n")
    (insert ":PROPERTIES:\n:CUSTOM_ID: custom-target\n:END:\n")
    (insert "<<radio target>> body.\n")
    (insert "#+begin_src emacs-lisp -n -r\n")
    (insert "(message \"hi\") ;; (call)\n")
    (insert "#+end_src\n")
    (insert "Links: [[tbl]] [[*Target heading]] [[#custom-target]] ")
    (insert "[[radio target]] [[(call)]].\n")
    (let* ((tree (org-element-parse-buffer))
           (info (org-export-get-environment 'html nil nil))
           (links (org-element-map tree 'link #'identity))
           (table (org-element-map tree 'table #'identity nil t))
           (src (org-element-map tree 'src-block #'identity nil t))
           (headline (org-element-map tree 'headline #'identity nil t))
           (resolve
            (lambda (link)
              (let ((raw (org-element-property :raw-link link)))
                (list raw
                      (org-element-type
                       (org-export-resolve-link link info))
                      (org-export-get-reference
                       (org-export-resolve-link link info) info))))))
      (list (mapcar resolve links)
            (org-export-get-caption table)
            (org-export-get-reference table info)
            (org-export-get-reference src info)
            (org-export-get-reference headline info)
            (org-export-get-ordinal table info)
            (org-export-resolve-coderef "call" info)
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_insert_link_region_file_stored_edit_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"* Source\\nSelected Words[[descprobe:path value][auto:descprobe:path value:none]]\\n* Target\\nBody\\n\" ((\"https://example.org/one\" \"One\") (\"https://example.org/two\" \"Two\")) \"* Source\\nSelected Words[[descprobe:path value][auto:descprobe:path value:none]]\\n* Target\\nBody\\n[[file:docs/read me.txt::7][External doc]]\\n[[*Target][Same file target]]\\n[[descprobe:auto-only][auto:descprobe:auto-only:none]]\\n\" \"* Source\\nSelected Words[[https://edited.example/a b][Edited Desc]]\\n* Target\\nBody\\n[[file:docs/read me.txt::7][External doc]]\\n[[*Target][Same file target]]\\n[[descprobe:auto-only][auto:descprobe:auto-only:none]]\\n\" \"* Source\\nSelected Words[[https://edited.example/a b][Edited Desc]]\\n* Target\\nBody\\n[[file:docs/read me.txt::7][External doc]]\\n[[*Target][Same file target]]\\n[[descprobe:auto-only][auto:descprobe:auto-only:none]]\\n\\n- [[https://example.org/one][One]]\\n- [[https://example.org/two][Two]]\\n\" nil ((\"https://example.org/keep\" \"Keep\") (\"https://example.org/also\" \"Also\")) (\"Shown\" \"x:y\" \"plain\") (\"<a>\" \"<b>\" \"<c>\" \"<d>\") \"* Source\\nSelected Words[[https://edited.example/a b][Edited Desc]]\\n* Target\\nBody\\n[[file:docs/read me.txt::7][External doc]]\\n[[*Target][Same file target]]\\n[[descprobe:auto-only][auto:descprobe:auto-only:none]]\\n\\n- [[https://example.org/one][One]]\\n- [[https://example.org/two][Two]]\\n+ [[https://example.org/keep][Keep]]\\n+ [[https://example.org/also][Also]]\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ol)
  (let* ((root (make-temp-file "org-link-insert" t))
         (org-file (expand-file-name "notes.org" root))
         (other-file (expand-file-name "docs/read me.txt" root)))
    (unwind-protect
        (progn
          (make-directory (file-name-directory other-file) t)
          (with-temp-file other-file
            (insert "external\n"))
          (with-temp-file org-file
            (insert "* Source\n")
            (insert "Selected Words\n")
            (insert "* Target\nBody\n"))
          (org-link-set-parameters
           "descprobe"
           :insert-description
           (lambda (link desc)
             (format "auto:%s:%s" link (or desc "none"))))
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (let ((default-directory root)
                  (org-link-file-path-type 'relative)
                  (org-link-keep-stored-after-insertion nil)
                  (org-stored-links
                   '(("https://example.org/one" "One")
                     ("https://example.org/two" "Two"))))
              (goto-char (point-min))
              (search-forward "Selected")
              (push-mark (match-beginning 0) t t)
              (search-forward "Words")
              (org-insert-link nil "descprobe:path value" nil)
              (let ((after-region
                     (buffer-substring-no-properties
                      (point-min) (point-max)))
                    (stored-after-region org-stored-links))
                (goto-char (point-min))
                (search-forward "Body")
                (end-of-line)
                (insert "\n")
                (org-insert-link nil (concat "file:" other-file "::7")
                                 "External doc")
                (insert "\n")
                (org-insert-link nil (concat "file:" org-file "::*Target")
                                 "Same file target")
                (insert "\n")
                (org-insert-link nil "descprobe:auto-only" nil)
                (let ((after-file-auto
                       (buffer-substring-no-properties
                        (point-min) (point-max))))
                  (goto-char (point-min))
                  (search-forward "Selected Words")
                  (cl-letf (((symbol-function 'read-string)
                             (lambda (prompt &optional initial &rest _)
                               (list prompt initial)
                               "https://edited.example/a b")))
                    (org-insert-link nil nil "Edited Desc"))
                  (let ((after-edit
                         (buffer-substring-no-properties
                          (point-min) (point-max))))
                    (goto-char (point-max))
                    (insert "\n")
                    (org-insert-all-links nil "- " "\n")
                    (let ((after-insert-all
                           (buffer-substring-no-properties
                            (point-min) (point-max)))
                          (stored-after-delete org-stored-links))
                      (setq org-stored-links
                            '(("https://example.org/keep" "Keep")
                              ("https://example.org/also" "Also")))
                      (org-insert-all-links '(4) "+ " "\n")
                      (list after-region
                            stored-after-region
                            after-file-auto
                            after-edit
                            after-insert-all
                            stored-after-delete
                            org-stored-links
                            (mapcar #'org-link-display-format
                                    '("[[x:y][Shown]]" "[[x:y]]" "plain"))
                            (mapcar #'org-link-add-angle-brackets
                                    '("a" "<b" "c>" "<d>"))
                            (replace-regexp-in-string
                             (regexp-quote root)
                             "<root>"
                             (buffer-substring-no-properties
                              (point-min) (point-max)))))))))))
      (when (get-file-buffer org-file) (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_link_open_store_move_visibility_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"[[id:alpha-id][Alpha]]\" (:link \"id:alpha-id\" :description \"Alpha\" :type \"id\") ((\"Alpha ID\" \"main.org\" #(\"Alpha\" 0 5 (face org-level-1)) 2 nil \"* TODO Alpha :work:\") (\"Beta Custom\" \"main.org\" #(\"Beta\" 0 4 (face org-level-1)) 13 nil \"* TODO Beta\") (\"Alpha Grand\" \"main.org\" #(\"Alpha grand\" 0 11 (face org-level-3)) 10 t \"*** TODO Alpha grand\") (\"Radio\" \"main.org\" #(\"Alpha\" 0 5 (face org-level-1)) 7 t \"Alpha body with <<radio-alpha>>.\") (\"Stored Alpha\" \"main.org\" #(\"Alpha\" 0 5 (face org-level-1)) 2 nil \"* TODO Alpha :work:\") (\"External Heading\" \"other.org\" \"External\" 1 nil \"* External\")) (#(\"Beta child\" 0 10 (face org-level-2)) 19 2) (#(\"Beta child\" 0 10 (face org-level-2)) 11 5 dedicated #(\"Beta child\" 0 10 (face org-level-2))) ((\"Alpha\" 2 nil org-level-1) (\"Alpha child\" 8 t org-level-2) (\"Alpha grand\" 10 nil org-level-3) (\"Alpha L4\" 13 t org-level-4) (\"Beta child\" 11 nil org-level-2) (\"beta child body\" 12 nil nil) (\"Beta\" 11 nil org-level-2) (\"Links\" 21 t org-level-1)) ((\"id\" \"alpha-id\" \"id:alpha-id\" \"Stored Alpha\") (\"file\" \"other.org\" \"file:other.org::*External\" \"External Heading\") (\"id\" \"alpha-id\" \"id:alpha-id\" \"Alpha ID\") (\"custom-id\" \"beta-custom\" \"#beta-custom\" \"Beta Custom\") (\"fuzzy\" \"*Alpha grand\" \"*Alpha grand\" \"Alpha Grand\") (\"fuzzy\" \"radio-alpha\" \"radio-alpha\" \"Radio\")) \"#+TITLE: Link Move\\n* TODO Alpha :work:\\n:PROPERTIES:\\n:ID: alpha-id\\n:CUSTOM_ID: alpha-custom\\n:END:\\nAlpha body with <<radio-alpha>>.\\n** WAIT Alpha child\\nchild body\\n*** TODO Alpha grand\\n***** TODO Beta child\\nbeta child body\\n**** TODO Alpha L4\\nlevel four body\\n* TODO Beta\\n:PROPERTIES:\\n:ID: beta-id\\n:CUSTOM_ID: beta-custom\\n:END:\\nBeta body.\\n* Links\\n[[id:alpha-id][Stored Alpha]] [[file:other.org::*External][External Heading]]\\n[[id:alpha-id][Alpha ID]] [[#beta-custom][Beta Custom]]\\n[[*Alpha grand][Alpha Grand]] [[radio-alpha][Radio]]\\n\")""##
    ]];
    crate::common::assert_oracle_parity_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-id)
  (require 'org-cycle)
  (require 'org-fold)
  (require 'ol)
  (let* ((root (make-temp-file "org-link-open-store" t))
         (main (expand-file-name "main.org" root))
         (other (expand-file-name "other.org" root))
         (org-id-locations-file (expand-file-name "ids.el" root))
         (org-id-track-globally t)
         (org-link-file-path-type 'relative)
         (org-link-context-for-files t)
         (org-id-link-to-org-use-id 'use-existing)
         (org-link-descriptive t)
         (org-stored-links nil)
         (org-store-link-plist nil))
    (unwind-protect
        (progn
          (with-temp-file main
            (insert "#+TITLE: Link Move\n")
            (insert "* TODO Alpha :work:\n")
            (insert ":PROPERTIES:\n:ID: alpha-id\n:CUSTOM_ID: alpha-custom\n:END:\n")
            (insert "Alpha body with <<radio-alpha>>.\n")
            (insert "** WAIT Alpha child\nchild body\n")
            (insert "*** TODO Alpha grand\n")
            (insert "**** TODO Alpha L4\nlevel four body\n")
            (insert "* TODO Beta\n")
            (insert ":PROPERTIES:\n:ID: beta-id\n:CUSTOM_ID: beta-custom\n:END:\n")
            (insert "Beta body.\n")
            (insert "** TODO Beta child\nbeta child body\n")
            (insert "* Links\n")
            (insert "[[id:alpha-id][Alpha ID]] [[#beta-custom][Beta Custom]]\n")
            (insert "[[*Alpha grand][Alpha Grand]] [[radio-alpha][Radio]]\n"))
          (with-temp-file other
            (insert "* External\n")
            (insert ":PROPERTIES:\n:CUSTOM_ID: external-custom\n:END:\n")
            (insert "External body.\n"))
          (org-id-update-id-locations (list main other) t)
          (with-current-buffer (find-file-noselect main)
            (org-mode)
            (let ((default-directory root)
                  opened before-move after-move inserted stored alpha-plist)
              (org-fold-hide-sublevels 2)
              (goto-char (point-min))
              (search-forward "Alpha")
              (beginning-of-line)
              (setq stored (org-store-link nil nil)
                    alpha-plist org-store-link-plist)
              (goto-char (point-min))
              (search-forward "* Links")
              (end-of-line)
              (insert "\n")
              (org-insert-link nil (plist-get alpha-plist :link)
                               "Stored Alpha")
              (insert " ")
              (org-insert-link nil (concat "file:" other "::*External")
                               "External Heading")
              (font-lock-ensure (point-min) (point-max))
              (dolist (needle '("Alpha ID" "Beta Custom" "Alpha Grand"
                                "Radio" "Stored Alpha" "External Heading"))
                (goto-char (point-min))
                (search-forward needle)
                (push
                 (save-excursion
                   (org-open-at-point)
                   (list needle
                         (file-name-nondirectory (or (buffer-file-name) ""))
                         (org-get-heading t t t t)
                         (line-number-at-pos)
                         (not (null
                               (org-invisible-p
                                (line-beginning-position))))
                         (buffer-substring-no-properties
                          (line-beginning-position)
                          (line-end-position))))
                 opened))
              (setq before-move
                    (save-excursion
                      (goto-char (point-min))
                      (search-forward "Beta child")
                      (list (org-get-heading t t t t)
                            (line-number-at-pos)
                            (org-outline-level))))
              (goto-char (point-min))
              (search-forward "Beta child")
              (beginning-of-line)
              (org-cut-subtree)
              (goto-char (point-min))
              (search-forward "Alpha L4")
              (beginning-of-line)
              (org-paste-subtree 5)
              (setq after-move
                    (save-excursion
                      (goto-char (point-min))
                      (search-forward "Beta child")
                      (list (org-get-heading t t t t)
                            (line-number-at-pos)
                            (org-outline-level)
                            (org-link-search "*Beta child" nil t)
                            (org-get-heading t t t t))))
              (org-fold-hide-sublevels 2)
              (goto-char (point-min))
              (search-forward "beta child body")
              (org-fold-show-context 'isearch)
              (setq inserted
                    (mapcar
                     (lambda (needle)
                       (save-excursion
                         (goto-char (point-min))
                         (search-forward needle)
                         (list needle
                               (line-number-at-pos)
                               (not (null (org-invisible-p (point))))
                               (get-text-property
                                (line-beginning-position) 'face))))
                     '("Alpha" "Alpha child" "Alpha grand" "Alpha L4"
                       "Beta child" "beta child body" "Beta" "Links")))
              (let* ((tree (org-element-parse-buffer))
                     (links
                      (org-element-map tree 'link
                        (lambda (link)
                          (list (org-element-property :type link)
                                (replace-regexp-in-string
                                 (regexp-quote root)
                                 "<root>"
                                 (org-element-property :path link))
                                (org-element-property :raw-link link)
                                (and (org-element-contents-begin link)
                                     (buffer-substring-no-properties
                                      (org-element-contents-begin link)
                                      (org-element-contents-end link))))))))
                (list (replace-regexp-in-string
                       (regexp-quote root) "<root>" stored)
                      (plist-put
                       (copy-sequence alpha-plist)
                       :link
                       (replace-regexp-in-string
                        (regexp-quote root) "<root>"
                        (plist-get alpha-plist :link)))
                      (nreverse opened)
                      before-move
                      after-move
                      inserted
                      links
                      (replace-regexp-in-string
                       (regexp-quote root)
                       "<root>"
                       (buffer-substring-no-properties
                        (point-min) (point-max))))))))
      (when (get-file-buffer main) (kill-buffer (get-file-buffer main)))
      (when (get-file-buffer other) (kill-buffer (get-file-buffer other)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_link_attachment_custom_export_visibility_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable norm)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-attach)
  (require 'ol)
  (require 'ox-ascii)
  (require 'ox-html)
  (let* ((root (file-name-as-directory
                (make-temp-file "org-link-attach-custom" t)))
         (org-file (expand-file-name "notes.org" root))
         (org-attach-id-dir root)
         (org-attach-store-link-p 'attached)
         (org-attach-use-inheritance nil)
         (org-link-descriptive t)
         (org-stored-links nil)
         (org-store-link-plist nil)
         (follow-calls nil)
         (store-calls nil))
    (unwind-protect
        (progn
          (org-link-set-parameters
           "audit"
           :follow (lambda (path arg)
                     (push (list path arg (buffer-name)) follow-calls))
           :store (lambda ()
                    (push (list (buffer-name) (point)) store-calls)
                    (org-link-store-props
                     :type "audit"
                     :link "audit:stored/path"
                     :description "Stored Audit")
                    t)
           :export (lambda (path desc backend _info)
                     (format "<%s:%s:%s>" backend path (or desc "")))
           :face (lambda (path)
                   (if (string-match-p "warn" path)
                       'org-warning
                     'org-link))
           :display "AUDIT")
          (with-temp-file org-file
            (insert "#+TITLE: Attach Custom\n")
            (insert "* Asset Node\n")
            (insert ":PROPERTIES:\n:ID: asset-id\n:END:\n")
            (insert "Body with [[audit:warn/path][Audit Warn]].\n")
            (insert "[[attachment:asset one.txt][Asset One]] ")
            (insert "[[attachment:asset-two.dat]].\n")
            (insert "* Other\nOther body\n"))
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (setq default-directory root)
            (goto-char (point-min))
            (search-forward "Asset Node")
            (beginning-of-line)
            (let* ((attach-dir (org-attach-dir-get-create))
                   (asset-one (expand-file-name "asset one.txt" attach-dir))
                   (asset-two (expand-file-name "asset-two.dat" attach-dir)))
              (with-temp-file asset-one (insert "asset one contents\n"))
              (with-temp-file asset-two (insert "asset two contents\n"))
              (font-lock-ensure (point-min) (point-max))
              (let ((norm
                     (lambda (value)
                       (cond
                        ((stringp value)
                         (replace-regexp-in-string
                          (regexp-quote root) "<root>/" value t t))
                        ((consp value) (mapcar norm value))
                        (t value))))
                    (props
                     (lambda (needle)
                       (save-excursion
                         (goto-char (point-min))
                         (search-forward needle)
                         (list needle
                               (get-text-property
                                (match-beginning 0) 'face)
                               (get-text-property
                                (match-beginning 0) 'display)
                               (get-text-property
                                (match-beginning 0) 'invisible)
                               (get-text-property
                                (match-beginning 0) 'mouse-face)
                               (keymapp
                                (get-text-property
                                 (match-beginning 0) 'keymap))))))
                    (link-summary
                     (lambda ()
                       (org-element-map (org-element-parse-buffer) 'link
                         (lambda (link)
                           (list (org-element-property :type link)
                                 (org-element-property :path link)
                                 (org-element-property :raw-link link)
                                 (and (org-element-contents-begin link)
                                      (buffer-substring-no-properties
                                       (org-element-contents-begin link)
                                       (org-element-contents-end link)))))))))
                (let ((before-props
                       (mapcar props
                               '("Audit Warn" "Asset One"
                                 "attachment:asset-two.dat")))
                      (expanded
                       (mapcar #'org-attach-expand
                               '("asset one.txt" "asset-two.dat"
                                 "missing.bin"))))
                  (goto-char (point-min))
                  (search-forward "Audit Warn")
                  (org-open-at-point '(4))
                  (goto-char (point-min))
                  (search-forward "Asset One")
                  (let ((asset-open
                         (cl-letf (((symbol-function 'org-link-open-as-file)
                                    (lambda (path arg)
                                      (list 'opened
                                            (funcall norm path)
                                            arg))))
                           (org-open-at-point '(16)))))
                    (goto-char (point-min))
                    (search-forward "Asset Node")
                    (beginning-of-line)
                    (let ((stored-audit
                           (progn
                             (setq org-store-link-functions nil)
                             (org-store-link nil nil)))
                          (stored-audit-plist org-store-link-plist))
                      (setq org-store-link-plist nil)
                      (let ((export-source
                             (buffer-substring-no-properties
                              (point-min) (point-max))))
                        (list before-props
                              (funcall norm expanded)
                              asset-open
                              (nreverse follow-calls)
                              stored-audit
                              stored-audit-plist
                              (nreverse store-calls)
                              (funcall link-summary)
                              (funcall norm
                                       (org-export-string-as
                                        export-source 'ascii t
                                        '(:with-toc nil)))
                              (funcall norm
                                       (org-export-string-as
                                        export-source 'html t
                                        '(:with-toc nil)))
                              (funcall norm
                                       (buffer-substring-no-properties
                                        (point-min) (point-max))))))))))))
      (when (get-file-buffer org-file)
        (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_link_abbrev_safety_legacy_follow_matrix_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ol)
  (let ((warnings nil)
        (follow-calls nil)
        (org-link-abbrev-alist-local
         '(("local" . "https://local.example/%s")
           ("hexlocal" . "https://hex.local/%h")
           ("unsafe" . "https://unsafe.example/%(oracle-link-unsafe)")
           ("safe" . "https://safe.example/%(oracle-link-safe)")))
        (org-link-abbrev-alist
         '(("global" . "https://global.example/%s")
           ("symbol" . oracle-link-symbol)
           ("plain" . "https://plain.example/"))))
    (defun oracle-link-safe (tag)
      (format "safe:%s" (or tag "")))
    (put 'oracle-link-safe 'org-link-abbrev-safe t)
    (defun oracle-link-unsafe (tag)
      (format "unsafe:%s" (or tag "")))
    (defun oracle-link-symbol (tag)
      (format "symbol:%s" (or tag "")))
    (org-link-set-parameters
     "legacy-follow"
     :follow (lambda (path)
               (push (list path 'one-arg) follow-calls)))
    (org-link-set-parameters
     "modern-follow"
     :follow (lambda (path arg)
               (push (list path arg 'two-arg) follow-calls)))
    (with-temp-buffer
      (org-mode)
      (insert "* Links\n")
      (insert "[[legacy-follow:old/path][Legacy]]\n")
      (insert "[[modern-follow:new/path][Modern]]\n")
      (font-lock-ensure (point-min) (point-max))
      (cl-letf (((symbol-function 'org-display-warning)
                 (lambda (message)
                   (push message warnings))))
        (let* ((expanded
                (mapcar #'org-link-expand-abbrev
                        '("local:a b"
                          "hexlocal:a b/c"
                          "global:g h"
                          "symbol:tag value"
                          "plain:tail"
                          "safe:abc"
                          "unsafe:secret"
                          "unsafe:again"
                          "missing:value"
                          "local")))
               (after-abbrevs
                (list org-link-abbrev-alist-local
                      org-link-abbrev-alist
                      warnings))
               (props
                (mapcar
                 (lambda (needle)
                   (save-excursion
                     (goto-char (point-min))
                     (search-forward needle)
                     (list needle
                           (get-text-property (match-beginning 0) 'face)
                           (get-text-property (match-beginning 0) 'mouse-face)
                           (keymapp
                            (get-text-property
                             (match-beginning 0) 'keymap)))))
                 '("Legacy" "Modern"))))
          (org-link-open-from-string
           "[[legacy-follow:from-string][Legacy]]" '(4))
          (org-link-open-from-string
           "[[modern-follow:from-string][Modern]]" '(16))
          (let* ((tree (org-element-parse-buffer))
                 (links
                  (org-element-map tree 'link
                    (lambda (link)
                      (list (org-element-property :type link)
                            (org-element-property :path link)
                            (org-element-property :raw-link link)
                            (and (org-element-contents-begin link)
                                 (buffer-substring-no-properties
                                  (org-element-contents-begin link)
                                  (org-element-contents-end link))))))))
            (list expanded
                  after-abbrevs
                  props
                  links
                  (nreverse follow-calls)
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_link_stored_insert_edit_search_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"org-link\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-link)
  (with-temp-buffer
    (org-mode)
    (insert "* Section One\nBody one.\n\n")
    (insert "* Section Two\nBody two.\n\n")
    (insert "* Section Three\nBody three.\n\n")
    ;; Store link
    (goto-char (point-min))
    (search-forward "Section One")
    (beginning-of-line)
    (let ((stored (org-store-link nil)))
      ;; Insert stored link at end
      (goto-char (point-max))
      (insert "\nSee " stored "\n")
      ;; Parse links
      (let* ((tree (org-element-parse-buffer))
             (links
              (org-element-map tree 'link
                (lambda (lk)
                  (list (org-element-property :type lk)
                        (org-element-property :path lk)
                        (org-element-property :raw-link lk)
                        (org-element-property :search-option lk))))))
        ;; Search for "Section Two"
        (goto-char (point-min))
        (org-link-search "Section Two")
        (let ((after-search-pos (point))
              (after-search-line (line-number-at-pos)))
          ;; Edit: insert a new heading
          (goto-char (point-max))
          (insert "\n* Section Four\nBody four.\n")
          ;; Re-parse
          (let* ((tree2 (org-element-parse-buffer))
                 (links2
                  (org-element-map tree2 'link
                    (lambda (lk)
                      (list (org-element-property :type lk)
                            (org-element-property :path lk)
                            (org-element-property :raw-link lk)
                            (org-element-property :search-option lk))))))
            (list stored
                  links
                  after-search-pos
                  after-search-line
                  links2
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_link_id_radio_custom_export_parse_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (error \"‘org-id-get’ expects a file-visiting buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-id)
  (require 'ox)
  (with-temp-buffer
    (org-mode)
    (insert "* Target heading\n")
    (insert ":PROPERTIES:\n:CUSTOM_ID: my-target\n:END:\n")
    (insert "Target body.\n\n")
    (insert "* References\n")
    (insert "See [[#my-target][custom link]].\n")
    (insert "See [[id:" (org-id-get-create (point-min)) "][id link]].\n")
    ;; Parse links
    (let* ((tree (org-element-parse-buffer))
           (links
            (org-element-map tree 'link
              (lambda (lk)
                (list (org-element-property :type lk)
                      (org-element-property :path lk)
                      (org-element-property :raw-link lk)
                      (org-element-property :search-option lk)))))
           ;; Export HTML
           (html (org-export-as 'html nil nil t '(:with-toc nil)))
           (html-has-custom (string-match-p "my-target" html))
           (html-has-id (string-match-p "id-" html))
           ;; Edit: change CUSTOM_ID
           (_ (progn
                (goto-char (point-min))
                (search-forward "CUSTOM_ID: my-target")
                (replace-match "CUSTOM_ID: renamed-target")))
           ;; Re-parse
           (tree2 (org-element-parse-buffer))
           (links2
            (org-element-map tree2 'link
              (lambda (lk)
                (list (org-element-property :type lk)
                      (org-element-property :path lk)
                      (org-element-property :raw-link lk)))))
           ;; Re-export
           (html2 (org-export-as 'html nil nil t '(:with-toc nil)))
           (html2-has-renamed (string-match-p "renamed-target" html2)))
      (list links
            html-has-custom
            html-has-id
            links2
            html2-has-renamed
            (buffer-substring-no-properties
             (point-min) (point-max)))))))"##,
        expect,
    );
}
