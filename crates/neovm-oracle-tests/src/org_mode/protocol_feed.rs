use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_protocol_parse_store_open_source_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range \"two\" 5 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-protocol)
  (let* ((root (make-temp-file "org-protocol" t))
         (work (expand-file-name "site" root))
         (article (expand-file-name "posts/article.org" work))
         (index (expand-file-name "index.org" work))
         (org-stored-links nil)
         (kill-ring nil)
         (org-protocol-project-alist
          `(("site"
             :base-url "https://example.org/"
             :working-directory ,(file-name-as-directory work)
             :online-suffix ".html"
             :working-suffix ".org"
             :rewrites (("https://example\\.org/?$" . "index.org"))))))
    (unwind-protect
        (progn
          (make-directory (file-name-directory article) t)
          (with-temp-file article (insert "* Article\n"))
          (with-temp-file index (insert "* Index\n"))
          (let* ((query "url=https%3A%2F%2Fexample.org%2Fposts%2Farticle.html&title=Hello+World&body=A%2FB")
                 (plist (org-protocol-parse-parameters query t))
                 (old (org-protocol-parse-parameters
                       "https:%2F%2Fexample.org%2Fold/Old%20Title/body"
                       nil '(:url :title :body)))
                 (split (org-protocol-split-data "a%2Fb/c+d" t))
                 (flat (org-protocol-flatten-greedy
                        '("/tmp/org-protocol:/greedy:/one" ("two" (3 . 4)))
                        t "<cwd>/"))
                 (store (org-protocol-store-link plist))
                 (opened (org-protocol-open-source
                          '(:url "https://example.org/posts/article.html?utm=1")))
                 (rewritten (org-protocol-open-source
                             '(:url "https://example.org/"))))
            (list plist
                  old
                  split
                  flat
                  store
                  org-stored-links
                  kill-ring
                  (file-relative-name opened root)
                  (file-relative-name rewritten root))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_protocol_custom_handler_dispatch_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-protocol)
  (let ((calls nil)
        (killed nil)
        (messages nil)
        (org-protocol-protocol-alist
         `(("normal"
            :protocol "normal"
            :function ,(lambda (plist)
                         (push (list 'normal plist) calls)
                         (plist-get plist :file)))
           ("drop"
            :protocol "drop"
            :function ,(lambda (plist)
                         (push (list 'drop plist) calls)
                         nil)
            :kill-client t)
           ("greedy"
            :protocol "greedy"
            :function ,(lambda (files)
                         (push (list 'greedy
                                     (org-protocol-flatten-greedy
                                      files t "<cwd>/"))
                               calls))
            :greedy t))))
    (cl-letf (((symbol-function 'server-edit)
               (lambda (&rest _) (push 'server-edit killed)))
              ((symbol-function 'message)
               (lambda (fmt &rest args)
                 (push (apply #'format fmt args) messages))))
      (let* ((normal
              (org-protocol-check-filename-for-protocol
               "org-protocol://normal?url=https%3A%2F%2Fexample.org%2Fa%3Fb%3D1&title=A+B&file=/tmp/from-protocol.org"
               nil nil))
             (drop
              (org-protocol-check-filename-for-protocol
               "org-protocol://drop?url=https%3A%2F%2Fexample.org%2Fdrop&title=Drop"
               nil nil))
             (greedy
              (org-protocol-check-filename-for-protocol
               "/work/org-protocol://greedy:/first"
               '(("/work/org-protocol://greedy:/first" . 1)
                 ("/work/second" . 2)
                 ("/work/third" . 3))
               nil))
             (unknown
              (org-protocol-check-filename-for-protocol
               "org-protocol://unknown?x=1" nil nil)))
        (list normal
              drop
              greedy
              unknown
              (nreverse calls)
              (nreverse killed)
              (nreverse messages)
              (org-protocol-parse-parameters
               "url=https%3A%2F%2Fexample.org%2Fone&title=One+Two&body=A%2FB"
               t)
              (org-protocol-assign-parameters
               '("https://example.org/old" "Old Title" "body" "extra" "value")
               '(:url :title :body))))))"##,
        expect,
    );
}

#[test]
fn org_protocol_rewrite_greedy_sanitize_matrix_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument symbolp (closure (t) (s) (upcase (org-link-decode s))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-protocol)
  (let* ((root (make-temp-file "org-protocol-matrix" t))
         (work (expand-file-name "work" root))
         (posts (expand-file-name "posts" work))
         (calls nil)
         (messages nil)
         (killed nil)
         (org-protocol-reverse-list-of-files nil)
         (org-protocol-project-alist
          `(("site"
             :base-url "https://example.org/"
             :working-directory ,(file-name-as-directory work)
             :online-suffix ".html"
             :working-suffix ".org"
             :rewrites (("https://example\\.org/redirect/\\(.*\\)" . "posts/rewritten.org")
                        ("https://example\\.org/?$" . "index.org")))))
         (org-protocol-protocol-alist
          `(("capture"
             :protocol "cap-old"
             :function ,(lambda (data)
                          (push (list 'cap-old data) calls)
                          (concat "old:" data)))
            ("new"
             :protocol "new"
             :function ,(lambda (plist)
                          (push (list 'new plist) calls)
                          (plist-get plist :file)))
            ("greedy"
             :protocol "grab"
             :greedy t
             :kill-client t
             :function ,(lambda (files)
                          (push (list 'greedy
                                      (org-protocol-flatten-greedy
                                       files t "<cwd>/"))
                                calls))))))
    (unwind-protect
        (progn
          (make-directory posts t)
          (with-temp-file (expand-file-name "index.org" work)
            (insert "* Index\n"))
          (with-temp-file (expand-file-name "posts/one.org" work)
            (insert "* One\n"))
          (with-temp-file (expand-file-name "posts/rewritten.org" work)
            (insert "* Rewritten\n"))
          (cl-letf (((symbol-function 'message)
                     (lambda (fmt &rest args)
                       (push (apply #'format fmt args) messages)))
                    ((symbol-function 'server-edit)
                     (lambda (&rest _) (push 'server-edit killed))))
            (let* ((sanitized
                    (mapcar #'org-protocol-sanitize-uri
                            '("https:/example.org/a"
                              "file:/tmp/a"
                              "mailto:ada@example.org"
                              "org-protocol://capture?x=1")))
                   (split-custom
                    (org-protocol-split-data
                     "one%2Ftwo--three+four??five"
                     (lambda (s) (upcase (org-link-decode s)))
                     "--\\|\\?\\?"))
                   (assign-short
                    (org-protocol-assign-parameters
                     '("u" "t" "body" "extra" "value")
                     '(:url :title :body)))
                   (open-one
                    (org-protocol-open-source
                     '(:url "https:/example.org/posts/one.html?utm=1")))
                   (open-index
                    (org-protocol-open-source
                     '(:url "https://example.org/")))
                   (open-rewrite
                    (org-protocol-open-source
                     '(:url "https://example.org/redirect/deep.html")))
                   (open-missing
                    (org-protocol-open-source
                     '(:url "https://example.org/missing.html")))
                   (new-dispatch
                    (org-protocol-check-filename-for-protocol
                     "org-protocol://new?file=/tmp/from-new.org&url=https%3A%2F%2Fexample.org%2Fa&title=A+B"
                     nil nil))
                   (old-dispatch
                    (org-protocol-check-filename-for-protocol
                     "org-protocol://cap-old://https%3A%2F%2Fexample.org%2Fold/Old+Title"
                     nil nil))
                   (greedy
                    (org-protocol-check-filename-for-protocol
                     "/cwd/org-protocol://grab:/first"
                     '(("/cwd/org-protocol://grab:/first" . 1)
                       ("/cwd/second" . 2)
                       (("/cwd/third" . 3)))
                     nil))
                   (unknown
                    (org-protocol-check-filename-for-protocol
                     "/cwd/org-protocol://unknown?x=1" nil nil)))
              (list sanitized
                    split-custom
                    assign-short
                    (mapcar (lambda (file)
                              (and file
                                   (file-relative-name file root)))
                            (list open-one open-index open-rewrite
                                  open-missing))
                    new-dispatch
                    old-dispatch
                    greedy
                    unknown
                    (nreverse calls)
                    (nreverse killed)
                    (nreverse messages)))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_protocol_capture_template_finalize_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable capture-state)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-protocol)
  (require 'org-capture)
  (let* ((file (make-temp-file "org-protocol-capture" nil ".org"
                               "* Inbox\n* Archive\n"))
         (org-stored-links nil)
         (org-protocol-default-template-key "p")
         (org-capture-templates
          `(("p" "Protocol" entry
             (file+headline ,file "Inbox")
             "* TODO %:description\nURL: %:link\nType: %:type\nAnnotation: %a\nBody:\n%i\nQuery: %(prin1-to-string (plist-get org-store-link-plist :query))\n"
             :empty-lines 0)
            ("q" "Quoted" entry
             (file+headline ,file "Archive")
             "* QUOTE %:description\n%a\n%i\n"
             :empty-lines 0))))
         capture-state)
    (unwind-protect
        (progn
          (cl-letf (((symbol-function 'raise-frame) (lambda (&rest _) nil)))
            (org-protocol-capture
             '(:template "p"
               :url "https://example.org/path?q=1#frag"
               :title "Example Title"
               :body "Line one\nLine two"))
            (setq capture-state
                  (list (buffer-name)
                        org-stored-links
                        org-store-link-plist
                        (buffer-substring-no-properties
                         (point-min) (point-max))))
            (org-capture-finalize)
            (org-protocol-capture
             '(:template "q"
               :url "mailto:ada@example.org"
               :title "Mail Title"
               :body "> quoted\n> body"))
            (let ((second-state
                   (list (buffer-name)
                         org-stored-links
                         org-store-link-plist
                         (buffer-substring-no-properties
                          (point-min) (point-max)))))
              (org-capture-finalize)
              (with-temp-buffer
                (insert-file-contents file)
                (list capture-state
                      second-state
                      org-stored-links
                      (replace-regexp-in-string
                       (regexp-quote file)
                       "<file>"
                       (buffer-string)))))))
      (dolist (buf '("CAPTURE-org-protocol-capture"
                     "CAPTURE-org-protocol-capture.org"))
        (when (get-buffer buf) (kill-buffer buf)))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_feed_rss_parse_entry_insert_filter_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((:guid \"item-1\" :item-full-text \"<guid>item-1</guid><title>First Entry</title><link>https://example.org/1</link><description>Body one</description><pubDate>Wed, 27 May 2026 10:00:00 GMT</pubDate>\") (:guid \"item-2\" :item-full-text \"<guid>item-2</guid><title>Second Entry</title><link>https://example.org/2</link><description>Body two</description><pubDate>Wed, 27 May 2026 11:00:00 GMT</pubDate>\")) error \"* Incoming\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-feed)
  (let* ((root (make-temp-file "org-feed-rss" t))
         (file (expand-file-name "inbox.org" root))
         (rss-buffer (generate-new-buffer " *rss-test*"))
         (org-feed-alist
          `(("TestRSS"
             :url "https://example.org/rss.xml"
             :file ,file
             :headline "Incoming")))
         (calls nil))
    (unwind-protect
        (progn
          (with-current-buffer rss-buffer
            (insert "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>Feed</title>")
            (insert "<item><guid>item-1</guid><title>First Entry</title><link>https://example.org/1</link><description>Body one</description><pubDate>Wed, 27 May 2026 10:00:00 GMT</pubDate></item>")
            (insert "<item><guid>item-2</guid><title>Second Entry</title><link>https://example.org/2</link><description>Body two</description><pubDate>Wed, 27 May 2026 11:00:00 GMT</pubDate></item>")
            (insert "</channel></rss>"))
          ;; Parse RSS entries
          (let ((entries (org-feed-parse-rss-feed rss-buffer)))
            ;; Create inbox file
            (with-temp-file file
              (insert "* Incoming\n"))
            ;; Update feed
            (let ((updated (condition-case nil
                               (org-feed-update "TestRSS")
                             (error 'error))))
              ;; Check inbox content
              (let ((inbox-content
                     (with-current-buffer (find-file-noselect file)
                       (prog1 (buffer-substring-no-properties
                               (point-min) (point-max))
                         (kill-buffer)))))
                (list entries
                      updated
                      (replace-regexp-in-string
                       (regexp-quote root) "<root>" inbox-content))))))
      (kill-buffer rss-buffer)
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_feed_parse_format_status_add_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 48 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-feed)
  (let* ((root (make-temp-file "org-feed" t))
         (file (expand-file-name "feeds.org" root))
         (rss (get-buffer-create " *rss-feed*")))
    (unwind-protect
        (progn
          (with-current-buffer rss
            (erase-buffer)
            (insert "<?xml version=\"1.0\"?><rss><channel>\n")
            (insert "<item><guid isPermaLink=\"false\">g-1</guid><title>One &amp; Two</title><link>https://example.org/1</link><description>Line 1\nLine 2</description><pubDate>2026-05-27</pubDate></item>\n")
            (insert "<item><guid>https://example.org/2</guid><title>Second</title><link>https://example.org/2</link><description>Desc</description></item>\n")
            (insert "</channel></rss>"))
          (with-temp-file file (insert "* Inbox\nExisting\n"))
          (let* ((raw (org-feed-parse-rss-feed rss))
                 (parsed (mapcar #'org-feed-parse-rss-entry raw))
                 (formatted
                  (mapcar (lambda (entry)
                            (org-feed-format-entry
                             entry
                             "\n* TODO %h\n  %u\n  %description\n  %a"
                             nil))
                          parsed))
                 (pos (org-feed-goto-inbox-internal file "Inbox"))
                 (status '(("old" t "abc"))))
            (org-feed-add-items pos formatted)
            (org-feed-write-status
             pos "FEEDSTATUS"
             (append status
                     (mapcar (lambda (entry)
                               (list (plist-get entry :guid)
                                     t
                                     (sha1 (plist-get entry :item-full-text))))
                             parsed)))
            (list (mapcar (lambda (entry)
                            (list (plist-get entry :guid)
                                  (plist-get entry :title)
                                  (plist-get entry :guid-permalink)
                                  (plist-get entry :link)))
                          parsed)
                  (org-feed-read-previous-status pos "FEEDSTATUS")
                  (with-current-buffer (find-file-noselect file)
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))
      (when (get-buffer rss) (kill-buffer rss))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_feed_update_with_custom_retriever_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (1 0 ((\"https://example.org/new\" t \"1ca541f2707e1ccf8b8b3bf2050db208f9d9fed7\")) \"* Inbox\\n:MOCKSTATUS:\\n((\\\"https://example.org/new\\\" t\\n  \\\"1ca541f2707e1ccf8b8b3bf2050db208f9d9fed7\\\"))\\n:END:\\n\\n** New Item\\n  [2026-06-15 Mon 12:00]\\n  New desc\\n  [[https://example.org/new]]\\n\\n\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org)
  (require 'org-feed)
  (let* ((root (make-temp-file "org-feed-update" t))
         (file (expand-file-name "feeds.org" root))
         (org-feed-save-after-adding nil)
         (org-feed-retrieve-method
          (lambda (_url)
            (let ((buf (get-buffer-create " *mock-feed*")))
              (with-current-buffer buf
                (erase-buffer)
                (insert "<?xml version=\"1.0\"?><rss><channel>")
                (insert "<item><guid>https://example.org/new</guid><title>New Item</title><link>https://example.org/new</link><description>New desc</description></item>")
                (insert "</channel></rss>"))
              buf)))
         (feed (list "Mock" "mock://feed" file "Inbox"
                     :drawer "MOCKSTATUS"
                     :filter (lambda (entry)
                               (and (string-match-p "New" (plist-get entry :title))
                                    entry)))))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* Inbox\n")
            (insert ":MOCKSTATUS:\n((\"https://example.org/old\" t \"oldsha\"))\n:END:\n"))
          (let ((first (org-feed-update feed))
                (second (org-feed-update feed)))
            (with-current-buffer (find-file-noselect file)
              (list first
                    second
                    (org-feed-read-previous-status (point-min) "MOCKSTATUS")
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))
      (when (get-buffer " *mock-feed*") (kill-buffer " *mock-feed*"))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_feed_handlers_changed_update_all_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (1 (\"1 new entry from 2 feeds (unavailable feeds: 1)\") (\"mock://bad\" \"mock://good\" \"mock://good\") ((new 1 ((\"new-guid\" \"New keep\" nil))) (changed 1 ((\"new-guid\" \"New keep\" nil))) (after \"feeds.org\" 1) (new 10 ((\"new-guid\" \"New keep\" nil))) (changed 10 ((\"new-guid\" \"New keep\" nil))) (after \"feeds.org\" 10)) (#(\"Position saved to mark ring, go back with ‘C-c &’.\" 43 48 (font-lock-face help-key-binding face help-key-binding)) \"Added 1 new item from feed Handlers to file feeds.org, heading Inbox\" #(\"Position saved to mark ring, go back with ‘C-c &’.\" 43 48 (font-lock-face help-key-binding face help-key-binding)) \"Added 1 new item from feed Handlers to file feeds.org, heading Inbox\" \"1 new entry from 2 feeds (unavailable feeds: 1)\") ((\"stable-guid\" t \"f0ff76e2a819eb9e0b3c11b820b37137bc347c2b\") (\"new-guid\" t \"770c17ce8c4cef5da3323b903a535dcfc74a5c9c\")) \"** HANDLED CHANGED\\n  :CUSTOMSTATUS:\\n((\\\"stable-guid\\\" t \\\"f0ff76e2a819eb9e0b3c11b820b37137bc347c2b\\\")\\n (\\\"new-guid\\\" t \\\"770c17ce8c4cef5da3323b903a535dcfc74a5c9c\\\"))\\n  :END:\\n*** New keep\\n** HANDLED NEW\\n*** New keep\\nhttps://example.org/new\\n** HANDLED CHANGED\\n  :CUSTOMSTATUS:\\n((\\\"stable-guid\\\" t \\\"f0ff76e2a819eb9e0b3c11b820b37137bc347c2b\\\")\\n (\\\"new-guid\\\" t \\\"770c17ce8c4cef5da3323b903a535dcfc74a5c9c\\\"))\\n  :END:\\n*** New keep\\n** HANDLED NEW\\n*** New keep\\nhttps://example.org/new\\n* Inbox\\n:CUSTOMSTATUS:\\n((\\\"stable-guid\\\" t \\\"432b9506974150c0f3e087ef8d633bbed7bd7148\\\"))\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-feed)
  (let* ((root (make-temp-file "org-feed-handlers" t))
         (file (expand-file-name "feeds.org" root))
         (calls nil)
         (retrieved nil)
         (messages nil)
         (org-feed-save-after-adding nil)
         (org-feed-after-adding-hook
          (list (lambda ()
                  (push (list 'after
                              (file-relative-name (buffer-file-name) root)
                              (line-number-at-pos))
                        calls))))
         (feed-xml
          (lambda (title body)
            (concat "<?xml version=\"1.0\"?><rss><channel>"
                    "<item><guid>stable-guid</guid><title>" title
                    "</title><link>https://example.org/stable</link>"
                    "<description>" body "</description></item>"
                    "<item><guid>new-guid</guid><title>New keep</title>"
                    "<link>https://example.org/new</link>"
                    "<description>fresh</description></item>"
                    "</channel></rss>")))
         (retriever
          (lambda (url)
            (push url retrieved)
            (when (string-match-p "bad" url)
              (error "mock unavailable"))
            (let ((buf (get-buffer-create (format " *feed-%s*" url))))
              (with-current-buffer buf
                (erase-buffer)
                (insert (funcall feed-xml "Changed keep" "changed body")))
              buf)))
         (new-handler
          (lambda (entries)
            (push (list 'new
                        (line-number-at-pos)
                        (mapcar (lambda (entry)
                                  (list (plist-get entry :guid)
                                        (plist-get entry :title)
                                        (plist-get entry :handled)))
                                entries))
                  calls)
            (insert "** HANDLED NEW\n")
            (dolist (entry entries)
              (insert "*** " (plist-get entry :title) "\n"
                      (plist-get entry :link) "\n"))))
         (changed-handler
          (lambda (entries)
            (push (list 'changed
                        (line-number-at-pos)
                        (mapcar (lambda (entry)
                                  (list (plist-get entry :guid)
                                        (plist-get entry :title)
                                        (plist-get entry :handled)))
                                entries))
                  calls)
            (insert "** HANDLED CHANGED\n")
            (dolist (entry entries)
              (insert "*** " (plist-get entry :title) "\n"))))
         feed good bad)
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* Inbox\n")
            (insert ":CUSTOMSTATUS:\n")
            (insert "((\"stable-guid\" t \"")
            (insert (sha1 "<item><guid>stable-guid</guid><title>Old keep</title><link>https://example.org/stable</link><description>old body</description></item>"))
            (insert "\"))\n")
            (insert ":END:\n"))
          (setq feed (list "Handlers" "mock://good" file "Inbox"
                           :drawer "CUSTOMSTATUS"
                           :new-handler new-handler
                           :changed-handler changed-handler
                           :filter (lambda (entry)
                                     (and (string-match-p
                                           "keep\\|New"
                                           (plist-get entry :title))
                                          entry))))
          (setq good (append feed (list :parse-feed 'org-feed-parse-rss-feed
                                        :parse-entry 'org-feed-parse-rss-entry)))
          (setq bad (list "Bad" "mock://bad" file "Inbox"))
          (let* ((org-feed-retrieve-method retriever)
                 (org-feed-alist (list good bad))
                 (update-one (cl-letf (((symbol-function 'message)
                                        (lambda (fmt &rest args)
                                          (push (apply #'format fmt args)
                                                messages))))
                               (org-feed-update "Handlers")))
                 (update-all (cl-letf (((symbol-function 'message)
                                        (lambda (fmt &rest args)
                                          (push (apply #'format fmt args)
                                                messages))))
                               (org-feed-update-all))))
            (with-current-buffer (find-file-noselect file)
              (list update-one
                    update-all
                    (sort retrieved #'string<)
                    (nreverse calls)
                    (nreverse messages)
                    (org-feed-read-previous-status (point-min)
                                                   "CUSTOMSTATUS")
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))
      (dolist (buf '(" *feed-mock://good*" " *feed-mock://bad*"))
        (when (get-buffer buf) (kill-buffer buf)))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_feed_atom_formatter_filter_status_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"tag:example,2026:1\" \"Keep One\" \"https://example.org/one\" nil nil) (\"tag:example,2026:2\" \"Drop Two\" \"https://example.org/two\" \"Unknown ‘nil’ content.\" nil)) nil nil ((\"tag:example,2026:1\" t \"055edee1ed338146d1032d3b4887710f533a2041\") (\"tag:example,2026:2\" t \"49206ff62f2d2becfe639a46417c31834e7a3f33\")) \"* Inbox\\n\\n  :ATOMSTATUS:\\n((\\\"tag:example,2026:1\\\" t \\\"055edee1ed338146d1032d3b4887710f533a2041\\\")\\n (\\\"tag:example,2026:2\\\" t \\\"49206ff62f2d2becfe639a46417c31834e7a3f33\\\"))\\n  :END:\\n** Keep One\\n   [2026-06-15 Mon]\\n   DESC:\\n   [[https://example.org/one]]\\n\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org)
  (require 'org-feed)
  (let* ((root (make-temp-file "org-feed-atom" t))
         (file (expand-file-name "atom.org" root))
         (feed-buf (get-buffer-create " *atom-feed*"))
         (org-feed-save-after-adding nil)
         (before nil)
         (after nil))
    (unwind-protect
        (progn
          (with-current-buffer feed-buf
            (erase-buffer)
            (insert "<?xml version=\"1.0\"?><feed xmlns=\"http://www.w3.org/2005/Atom\">")
            (insert "<entry><id>tag:example,2026:1</id><title>Keep One</title>")
            (insert "<link href=\"https://example.org/one\"/>")
            (insert "<updated>2026-05-27T09:30:00Z</updated>")
            (insert "<summary>Atom summary &amp; details</summary></entry>")
            (insert "<entry><id>tag:example,2026:2</id><title>Drop Two</title>")
            (insert "<link href=\"https://example.org/two\"/>")
            (insert "<content>Drop content</content></entry>")
            (insert "</feed>"))
          (with-temp-file file (insert "* Inbox\n"))
          (let* ((raw (org-feed-parse-atom-feed feed-buf))
                 (parsed (mapcar #'org-feed-parse-atom-entry raw))
                 (kept
                  (delq nil
                        (mapcar (lambda (entry)
                                  (and (string-match-p
                                        "Keep" (plist-get entry :title))
                                       entry))
                                parsed)))
                 (formatted
                  (mapcar
                   (lambda (entry)
                     (let ((copy (copy-sequence entry)))
                       (plist-put copy :description
                                  (concat "DESC:"
                                          (plist-get copy :description)))
                       (org-feed-format-entry
                        copy
                        "\n** %h\n   %u\n   %description\n   %a"
                        nil)))
                   kept))
                 (pos (org-feed-goto-inbox-internal file "Inbox")))
            (add-hook 'org-feed-before-adding-hook
                      (lambda () (push (line-number-at-pos) before)))
            (add-hook 'org-feed-after-adding-hook
                      (lambda () (push (line-number-at-pos) after)))
            (org-feed-add-items pos formatted)
            (org-feed-write-status
             pos "ATOMSTATUS"
             (mapcar (lambda (entry)
                       (list (plist-get entry :guid)
                             t
                             (sha1 (plist-get entry :item-full-text))))
                     parsed))
            (list (mapcar (lambda (entry)
                            (list (plist-get entry :guid)
                                  (plist-get entry :title)
                                  (plist-get entry :link)
                                  (plist-get entry :description)
                                  (plist-get entry :date)))
                          parsed)
                  (nreverse before)
                  (nreverse after)
                  (org-feed-read-previous-status pos "ATOMSTATUS")
                  (with-current-buffer (find-file-noselect file)
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))
      (when (get-buffer feed-buf) (kill-buffer feed-buf))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_feed_rss_incremental_status_element_visibility_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (search-failed \"stable changed\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-feed)
  (require 'org-element)
  (require 'org-fold)
  (let* ((root (make-temp-file "org-feed-incremental" t))
         (file (expand-file-name "feeds.org" root))
         (phase 0)
         (events nil)
         (org-feed-save-after-adding nil)
         (org-feed-before-adding-hook
          (list (lambda ()
                  (push (list 'before
                              (line-number-at-pos)
                              (buffer-substring-no-properties
                               (line-beginning-position)
                               (line-end-position)))
                        events))))
         (org-feed-after-adding-hook
          (list (lambda ()
                  (push (list 'after
                              (line-number-at-pos)
                              (count-matches "^\\*+ "
                                             (point-min) (point-max)))
                        events))))
         (feed-xml
          (lambda ()
            (concat "<?xml version=\"1.0\"?><rss><channel>"
                    "<item><guid>stable</guid><title>Stable Keep</title>"
                    "<link>https://example.org/stable</link>"
                    "<description>"
                    (if (= phase 0) "stable old" "stable changed")
                    "</description><pubDate>2026-05-27</pubDate></item>"
                    "<item><guid>drop</guid><title>Drop Me</title>"
                    "<link>https://example.org/drop</link>"
                    "<description>drop body</description></item>"
                    (if (= phase 0)
                        ""
                      "<item><guid>fresh</guid><title>Fresh Keep</title><link>https://example.org/fresh</link><description>fresh body</description><pubDate>2026-05-28</pubDate></item>")
                    "</channel></rss>")))
         (retriever
          (lambda (_url)
            (let ((buf (get-buffer-create " *incremental-feed*")))
              (with-current-buffer buf
                (erase-buffer)
                (insert (funcall feed-xml)))
              buf)))
         (feed (list "Incremental" "mock://incremental" file "Inbox"
                     :drawer "RSSSTATUS"
                     :filter (lambda (entry)
                               (and (string-match-p
                                     "Keep"
                                     (plist-get entry :title))
                                    entry))
                     :template
                     "\n** TODO %h\n:PROPERTIES:\n:GUID: %guid\n:END:\n%u\n%description\n%a\n")))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "#+TITLE: Feeds\n")
            (insert "* Inbox\n")
            (insert "Intro line\n")
            (insert ":RSSSTATUS:\n")
            (insert "nil\n")
            (insert ":END:\n")
            (insert "* Archive\nold\n"))
          (let ((org-feed-retrieve-method retriever))
            (let ((first (org-feed-update feed)))
              (setq phase 1)
              (let ((second (org-feed-update feed)))
                (with-current-buffer (find-file-noselect file)
                  (org-mode)
                  (goto-char (point-min))
                  (org-fold-hide-drawer-all)
                  (let ((hidden
                         (mapcar
                          (lambda (needle)
                            (save-excursion
                              (goto-char (point-min))
                              (search-forward needle)
                              (list needle
                                    (line-number-at-pos)
                                    (invisible-p (point)))))
                          '("RSSSTATUS" "stable changed" "Fresh Keep"
                            "Archive")))
                        (ast
                         (org-element-map
                             (org-element-parse-buffer)
                             '(headline drawer property-drawer node-property link)
                           (lambda (el)
                             (list (org-element-type el)
                                   (org-element-property :raw-value el)
                                   (org-element-property :key el)
                                   (org-element-property :value el)
                                   (org-element-property :begin el)
                                   (org-element-property :end el)))))
                        (status
                         (org-feed-read-previous-status
                          (point-min) "RSSSTATUS")))
                    (org-fold-show-all)
                    (list first
                          second
                          (nreverse events)
                          status
                          hidden
                          ast
                          (count-matches "^\\*+ "
                                         (point-min) (point-max))
                          (buffer-substring-no-properties
                           (point-min) (point-max)))))))))
      (when (get-buffer " *incremental-feed*")
        (kill-buffer " *incremental-feed*"))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_feed_raw_inbox_headers_error_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"feeds.org\" \"Incoming\" 6) (\" *raw-feed*\" 1 \"<?xml version=\\\"1.0\\\"?><rss><channel><item><guid>raw-1</g\" nil 2) (\" *raw-feed*\" 1 t 2) (error \"mock missing feed\") (error \"No such feed in ‘org-feed-alist\") 2 (((\"raw-1\" t \"ff0cfc66f33d573c9755c0a31137d02487b4ffbd\") (\"raw-2\" t \"a4c1b482714c008187093767e9057736f899cdc1\")) ((headline \"Existing\" nil nil 20 38) (headline \"Incoming\" nil nil 38 365) (drawer nil nil nil 51 186) (headline \"Raw Two\" nil nil 186 276) (node-property nil \"GUID\" \"raw-2\" 215 228) (link nil nil nil 264 273) (headline \"Raw One\" nil nil 276 365) (node-property nil \"GUID\" \"raw-1\" 305 318) (link nil nil nil 354 363)) \"#+TITLE: Raw Feeds\\n* Existing\\nBody\\n\\n\\n* Incoming\\n\\n\\n  :RAWSTATUS:\\n((\\\"raw-1\\\" t \\\"ff0cfc66f33d573c9755c0a31137d02487b4ffbd\\\")\\n (\\\"raw-2\\\" t \\\"a4c1b482714c008187093767e9057736f899cdc1\\\"))\\n  :END:\\n** TODO Raw Two\\n:PROPERTIES:\\n:GUID: raw-2\\n:END:\\n[2026-06-15 Mon]\\nRaw body two\\n[[raw-2]]\\n\\n\\n** TODO Raw One\\n:PROPERTIES:\\n:GUID: raw-1\\n:END:\\n[2026-06-15 Mon]\\nRaw body one\\n[[raw-1]]\\n\\n\") ((retrieve \"mock://raw\") (retrieve \"mock://raw\") (retrieve \"mock://missing\") (retrieve \"mock://raw\")) (\"Clipboard pasted as level 2 subtree\" \"Clipboard pasted as level 2 subtree\" #(\"Position saved to mark ring, go back with ‘C-c &’.\" 43 48 (font-lock-face help-key-binding face help-key-binding)) \"Added 2 new items from feed Raw to file feeds.org, heading Incoming\"))""##
    ]];
    crate::common::assert_oracle_parity_frozen_time_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-feed)
  (let* ((root (make-temp-file "org-feed-raw" t))
         (file (expand-file-name "feeds.org" root))
         (calls nil)
         (messages nil)
         (raw-buffer (get-buffer-create " *raw-feed*"))
         (org-feed-save-after-adding nil)
         (org-feed-retrieve-method
          (lambda (url)
            (push (list 'retrieve url) calls)
            (when (string-match-p "missing" url)
              (error "mock missing feed"))
            (with-current-buffer raw-buffer
              (erase-buffer)
              (insert "HTTP/1.1 200 OK\nContent-Type: application/rss+xml\n\n")
              (insert "<?xml version=\"1.0\"?><rss><channel>")
              (insert "<item><guid>raw-1</guid><title>Raw One</title>")
              (insert "<link>https://example.org/raw-1</link>")
              (insert "<description>Raw body one</description></item>")
              (insert "<item><guid>raw-2</guid><title>Raw Two</title>")
              (insert "<link>https://example.org/raw-2</link>")
              (insert "<description>Raw body two</description></item>")
              (insert "</channel></rss>"))
            (org-feed-skip-http-headers raw-buffer)))
         (good (list "Raw" "mock://raw" file "Incoming"
                     :drawer "RAWSTATUS"
                     :template "\n** TODO %h\n:PROPERTIES:\n:GUID: %guid\n:END:\n%u\n%description\n%a\n"))
         (bad (list "Missing" "mock://missing" file "Incoming"))
         (org-feed-alist (list good bad)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "#+TITLE: Raw Feeds\n")
            (insert "* Existing\nBody\n"))
          (cl-letf (((symbol-function 'message)
                     (lambda (fmt &rest args)
                       (push (apply #'format fmt args) messages))))
            (let* ((inbox-created
                    (progn
                      (org-feed-goto-inbox "Raw")
                      (list (file-relative-name (buffer-file-name) root)
                            (org-get-heading t t t t)
                            (line-number-at-pos))))
                   (raw-update (org-feed-update good 'retrieve-only))
                   (raw-summary
                    (with-current-buffer raw-update
                      (list (buffer-name)
                            (point-min)
                            (buffer-substring-no-properties
                             (point-min)
                             (min (point-max) (+ (point-min) 55)))
                            (not (null
                                  (string-match-p
                                   "HTTP/1.1"
                                   (buffer-substring-no-properties
                                    (point-min) (point-max)))))
                            (length (org-feed-parse-rss-feed raw-update)))))
                   (shown
                    (progn
                      (org-feed-show-raw-feed "Raw")
                      (list (buffer-name)
                            (point)
                            (looking-at "<\\?xml")
                            (length (org-feed-parse-rss-feed
                                     (current-buffer))))))
                   (missing-error
                    (condition-case err
                        (progn (org-feed-show-raw-feed "Missing") nil)
                      (error (cons (car err) (cdr err)))))
                   (missing-name-error
                    (condition-case err
                        (progn (org-feed-goto-inbox "Nope") nil)
                      (error (cons (car err) (cdr err)))))
                   (update-count (org-feed-update "Raw"))
                   (after-update
                    (with-current-buffer (find-file-noselect file)
                      (let ((pos (progn
                                   (goto-char (point-min))
                                   (search-forward "Incoming")
                                   (beginning-of-line)
                                   (point))))
                        (list (org-feed-read-previous-status
                               pos "RAWSTATUS")
                              (org-element-map
                                  (org-element-parse-buffer)
                                  '(headline drawer node-property link)
                                (lambda (el)
                                  (list (org-element-type el)
                                        (org-element-property :raw-value el)
                                        (org-element-property :key el)
                                        (org-element-property :value el)
                                        (org-element-property :begin el)
                                        (org-element-property :end el))))
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))))
              (list inbox-created
                    raw-summary
                    shown
                    missing-error
                    missing-name-error
                    update-count
                    after-update
                    (nreverse calls)
                    (nreverse messages)))))
      (when (get-buffer raw-buffer) (kill-buffer raw-buffer))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_feed_atom_parse_entry_insert_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((nil \"tag:one\" nil) (nil \"tag:two\" nil)) error \"* Incoming\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-feed)
  (let* ((root (make-temp-file "org-feed-atom" t))
         (file (expand-file-name "inbox.org" root))
         (atom-buffer (generate-new-buffer " *atom-test*"))
         (org-feed-alist
          `(("TestAtom"
             :url "https://example.org/atom.xml"
             :file ,file
             :headline "Incoming"))))
    (unwind-protect
        (progn
          (with-current-buffer atom-buffer
            (insert "<?xml version=\"1.0\"?><feed xmlns=\"http://www.w3.org/2005/Atom\"><title>Atom</title>")
            (insert "<entry><title>Entry One</title><id>tag:one</id><updated>2026-05-27T10:00:00Z</updated><link href=\"https://example.org/1\"/><content type=\"text\">Body one</content></entry>")
            (insert "<entry><title>Entry Two</title><id>tag:two</id><updated>2026-05-27T11:00:00Z</updated><link href=\"https://example.org/2\"/><content type=\"text\">Body two</content></entry>")
            (insert "</feed>"))
          (let ((entries (org-feed-parse-atom-feed atom-buffer)))
            (with-temp-file file
              (insert "* Incoming\n"))
            (let ((updated (condition-case nil
                               (org-feed-update "TestAtom")
                             (error 'error))))
              (let ((inbox-content
                     (with-current-buffer (find-file-noselect file)
                       (prog1 (buffer-substring-no-properties
                               (point-min) (point-max))
                         (kill-buffer)))))
                (list (mapcar (lambda (e)
                                (list (plist-get e :title)
                                      (plist-get e :guid)
                                      (plist-get e :link)))
                              entries)
                      updated
                      (replace-regexp-in-string
                       (regexp-quote root) "<root>"
                       inbox-content))))))
      (kill-buffer atom-buffer)
      (delete-directory root t))))"##,
        expect,
    );
}
