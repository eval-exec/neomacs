use expect_test::expect;

use super::ParityBatchCase;

fn modes_filters_edits_and_delay_follow_public_lifecycle() -> ParityBatchCase {
    let elisp_form = r##"(imp-test-run
 "impatient-mode-lifecycle"
 (lambda (_root)
   (let ((plain (imp-test-buffer " *imp-test-plain*"))
         (html (imp-test-buffer " *imp-test-html*"))
         (delayed (imp-test-buffer " *imp-test-delayed*"))
         plain-states html-states delayed-states)
     (with-current-buffer plain
       (fundamental-mode)
       (insert "release candidate")
       (push
        (list impatient-mode imp-last-state imp-user-filter
              (local-variable-p 'imp-user-filter)
              (and (memq #'imp--on-change after-change-functions) t)
              (and (memq #'imp--cleanup-timer kill-buffer-hook) t))
        plain-states)
       (impatient-mode 1)
       (push
        (list impatient-mode imp-last-state imp-user-filter
              (local-variable-p 'imp-user-filter)
              (and (memq #'imp--on-change after-change-functions) t)
              (and (memq #'imp--cleanup-timer kill-buffer-hook) t)
              (assq 'impatient-mode minor-mode-alist))
        plain-states)
       (goto-char (point-max))
       (imp-test-type ?!)
       (push (list imp-last-state (buffer-string)) plain-states)
       (imp-toggle-htmlize)
       (push
        (list imp-last-state imp-user-filter
              (local-variable-p 'imp-user-filter))
        plain-states)
       (imp-toggle-htmlize)
       (push
        (list imp-last-state imp-user-filter
              (local-variable-p 'imp-user-filter))
        plain-states)
       (imp-set-user-filter
        (lambda (buffer)
          (princ
           (format "<p>%d bytes</p>"
                   (with-current-buffer buffer (buffer-size))))))
       (push
        (list imp-last-state (functionp imp-user-filter)
              (local-variable-p 'imp-user-filter))
        plain-states)
       (imp-remove-user-filter)
       (push
        (list imp-last-state imp-user-filter
              (local-variable-p 'imp-user-filter))
        plain-states)
       (impatient-mode -1)
       (push
        (list impatient-mode
              (and (memq #'imp--on-change after-change-functions) t)
              (and (memq #'imp--cleanup-timer kill-buffer-hook) t)
              imp--idle-timer)
        plain-states))
     (with-current-buffer html
       (html-mode)
       (insert "<main>Deploy Ω</main>")
       (impatient-mode 1)
       (push
        (list impatient-mode imp-last-state imp-user-filter
              (local-variable-p 'imp-user-filter))
        html-states)
       (goto-char (point-max))
       (imp-test-type ?!)
       (push (list imp-last-state (buffer-string)) html-states)
       (imp-toggle-htmlize)
       (push
        (list imp-last-state imp-user-filter
              (local-variable-p 'imp-user-filter))
        html-states)
       (imp-remove-user-filter)
       (push
        (list imp-last-state imp-user-filter
              (local-variable-p 'imp-user-filter))
        html-states)
       (impatient-mode -1))
     (with-current-buffer delayed
       (fundamental-mode)
       (let ((impatient-mode-delay 2))
         (impatient-mode 1)
         (goto-char (point-max))
         (imp-test-type ?a)
         (let ((timer (cdr imp--idle-timer)))
           (push
            (list :after-first-edit
                  :state imp-last-state
                  :dirty imp--buffer-dirty-p
                  :stored-delay (car imp--idle-timer)
                  :timer-seconds
                  (time-convert (timer--time timer) 'integer)
                  :idle-kind (timer--idle-delay timer)
                  :repeat (timer--repeat-delay timer)
                  :queued (and (memq timer timer-idle-list) t))
            delayed-states)
           (timer-event-handler timer)
           (push
            (list :after-real-fire
                  :state imp-last-state
                  :dirty imp--buffer-dirty-p
                  :stored-delay (car imp--idle-timer)
                  :same-timer (eq timer (cdr imp--idle-timer))
                  :queued (and (memq timer timer-idle-list) t))
            delayed-states)
           (setq impatient-mode-delay 3)
           (imp-test-type ?b)
           (push
            (list :after-retime
                  :state imp-last-state
                  :dirty imp--buffer-dirty-p
                  :stored-delay (car imp--idle-timer)
                  :same-timer (eq timer (cdr imp--idle-timer))
                  :timer-seconds
                  (time-convert (timer--time timer) 'integer)
                  :idle-kind (timer--idle-delay timer)
                  :repeat (timer--repeat-delay timer))
            delayed-states)
           (setq impatient-mode-delay nil)
           (imp-test-type ?c)
           (push
            (list :after-immediate
                  :state imp-last-state
                  :dirty imp--buffer-dirty-p
                  :timer imp--idle-timer
                  :old-timer-queued (and (memq timer timer-idle-list) t)
                  :contents (buffer-string))
            delayed-states))
         (impatient-mode -1)))
     (list
      :plain (nreverse plain-states)
      :html (nreverse html-states)
      :delayed (nreverse delayed-states)))))"##;
    let expect = expect![[
        r#"OK (:result (:plain ((nil 0 imp-htmlize-filter nil nil nil) (t 1 imp-htmlize-filter nil t t (impatient-mode " imp")) (2 "release candidate!") (3 nil t) (4 imp-htmlize-filter t) (5 t t) (6 imp-htmlize-filter nil) (nil nil t nil)) :html ((t 2 nil t) (3 "<main>Deploy Ω</main>!") (4 imp-htmlize-filter t) (6 nil t)) :delayed ((:after-first-edit :state 1 :dirty :dirty :stored-delay 2 :timer-seconds 2 :idle-kind idle :repeat :repeat :queued t) (:after-real-fire :state 2 :dirty nil :stored-delay 2 :same-timer t :queued t) (:after-retime :state 2 :dirty :dirty :stored-delay 3 :same-timer t :timer-seconds 3 :idle-kind idle :repeat :repeat) (:after-immediate :state 3 :dirty nil :timer nil :old-timer-queued nil :contents "abc"))) :cleanup (:server nil :httpd-clients nil :network-processes nil :owned-buffers nil :owned-reference-live nil :published nil :root-exists nil))"#
    ]];
    ParityBatchCase::value(
        "modes_filters_edits_and_delay_follow_public_lifecycle",
        elisp_form,
        expect,
    )
}

fn browser_visit_listing_filters_and_static_assets_use_real_http() -> ParityBatchCase {
    let elisp_form = r##"(imp-test-run
 "impatient-mode-real-http"
 (lambda (_root)
   (let* ((html-name "Release plan & notes Ω.html")
          (code-name "Deploy audit λ.el")
          (html (imp-test-buffer html-name))
          (code (imp-test-buffer code-name))
          (html-source "<main><h1>Release Ω</h1><p>safe & ready</p></main>")
          (code-source
           ";; Verify deployment Ω\n(defun deployment-status (artifact)\n  \"Return the production STATUS for ARTIFACT.\"\n  (let ((status \"ready\"))\n    (message \"%s:%s\" artifact status)))\n\n(deployment-status \"api\")\n")
          listing raw code-html custom restored live loading jquery)
     (with-current-buffer html
       (html-mode)
       (insert html-source)
       (imp-visit-buffer nil)
       (imp-visit-buffer t))
     (with-current-buffer code
       (emacs-lisp-mode)
       (insert code-source)
       (impatient-mode 1))
     (setq
      listing
      (imp-test-await-response
       (imp-test-open-client "imp-test-listing" "/imp/"))
      raw
      (imp-test-await-response
       (imp-test-open-client
        "imp-test-raw"
        (format "/imp/buffer/%s?id=-1" (url-hexify-string html-name))))
      code-html
      (imp-test-await-response
       (imp-test-open-client
        "imp-test-htmlize"
        (format "/imp/buffer/%s?id=-1" (url-hexify-string code-name)))))
     (with-current-buffer code
       (imp-set-user-filter #'imp-test-word-count-filter))
     (setq custom
           (imp-test-await-response
            (imp-test-open-client
             "imp-test-custom"
             (format "/imp/buffer/%s?id=-1"
                     (url-hexify-string code-name)))))
     (with-current-buffer code
       (imp-remove-user-filter))
     (setq
      restored
      (imp-test-await-response
       (imp-test-open-client
        "imp-test-restored"
        (format "/imp/buffer/%s?id=-1" (url-hexify-string code-name))))
      live
      (imp-test-await-response
       (imp-test-open-client
        "imp-test-live"
        (format "/imp/live/%s/" (url-hexify-string html-name))))
      loading
      (imp-test-await-response
       (imp-test-open-client
        "imp-test-loading" "/imp/static/loading.html"))
      jquery
      (imp-test-await-response
       (imp-test-open-client
        "imp-test-jquery" "/imp/static/jquery.js")))
     (let* ((index-path (expand-file-name "index.html" imp-shim-root))
            (loading-path (expand-file-name "loading.html" imp-shim-root))
            (jquery-path (expand-file-name "jquery.js" imp-shim-root))
            (code-body (plist-get code-html :body))
            (restored-body (plist-get restored :body)))
       (list
        :source-directory
        (file-name-nondirectory
         (directory-file-name imp-shim-root))
        :listener
        (list :ephemeral (integerp (imp-test-port))
              :host httpd-host :family httpd-ip-family)
        :browser-events
        (mapcar
         (lambda (event)
           (list (imp-test-normalize-url (car event)) (cadr event)))
         (nreverse imp-test-browser-events))
        :listing
        (list (imp-test-response-summary listing)
              (plist-get listing :body))
        :raw
        (list (imp-test-response-summary raw)
              (plist-get raw :body))
        :htmlize
        (list
         (imp-test-response-summary code-html)
         :source-unchanged
         (with-current-buffer code (equal (buffer-string) code-source))
         :body code-body
         :document (and (string-prefix-p "<!DOCTYPE html" code-body) t)
         :comment (and (string-match-p "Verify deployment" code-body) t)
         :definition (and (string-match-p "deployment-status" code-body) t)
         :doc (and (string-match-p "production STATUS" code-body) t)
         :binding (and (string-match-p "font-lock-keyword-face" code-body) t)
         :literal (and (string-match-p "ready" code-body) t))
        :custom-filter
        (list (imp-test-response-summary custom)
              (plist-get custom :body)
              :source-unchanged
              (with-current-buffer code (equal (buffer-string) code-source)))
        :restored-filter
        (list (imp-test-response-summary restored)
              :same-html (equal code-body restored-body))
        :live-shim
        (list
         (imp-test-response-summary live)
         :etag-shape
         (and (string-match-p
               "^\"[0-9a-f]\\{16\\}\"$" (imp-test-header live "ETag"))
              t)
         :modified-exact
         (equal
          (imp-test-header live "Last-Modified")
          (httpd-date-string
           (file-attribute-modification-time
            (file-attributes index-path))))
         :transport-script
         (and (string-match-p "/imp/buffer/" (plist-get live :body)) t))
        :loading
        (list
         (imp-test-response-summary loading)
         :etag-shape
         (and (string-match-p
               "^\"[0-9a-f]\\{16\\}\"$" (imp-test-header loading "ETag"))
              t)
         :modified-exact
         (equal
          (imp-test-header loading "Last-Modified")
          (httpd-date-string
           (file-attribute-modification-time
            (file-attributes loading-path))))
         :body (plist-get loading :body))
        :jquery
        (list
         (imp-test-response-summary jquery)
         :etag-shape
         (and (string-match-p
               "^\"[0-9a-f]\\{16\\}\"$" (imp-test-header jquery "ETag"))
              t)
         :modified-exact
         (equal
          (imp-test-header jquery "Last-Modified")
          (httpd-date-string
           (file-attribute-modification-time
            (file-attributes jquery-path))))
         :banner
         (and (string-match-p "jQuery JavaScript Library v1.5.2"
                              (plist-get jquery :body))
              t))
        :states
        (list
         (with-current-buffer html
           (list impatient-mode imp-last-state imp-client-list))
         (with-current-buffer code
           (list impatient-mode imp-last-state imp-client-list))))))))"##;
    let expect = expect![[
        r#"OK (:result (:source-directory "impatient-mode-20260426.1323" :listener (:ephemeral t :host "127.0.0.1" :family ipv4) :browser-events (("http://localhost:PORT/imp/live/Release%20plan%20%26%20notes%20%CE%A9.html/" (nil)) ("http://localhost:PORT/imp/" (nil))) :listing ((:status "HTTP/1.1 200 OK" :type "text/html; charset=utf-8" :length 370 :connection "close" :server "impatient-mode parity" :cache nil :count nil :location nil :date-valid t :sha256 "1d91eb5babc860af0854ebcdb3e1c53619b6a2fa92d3d654e54a1722c235c95e") "<html><head>\n<title>impatient-mode buffer list</title>\n</head><body>\n<h1>Public Buffers</h1>\n<hr/><ul>\n<li><a href=\"live/Release%20plan%20%26%20notes%20%CE%A9.html/\">Release plan &amp; notes Ω.html</a></li>\n<li><a href=\"live/Deploy%20audit%20%CE%BB.el/\">Deploy audit λ.el</a></li>\n</ul>\n<hr/>Enable <code>impatient-mode</code> in buffers to publish them.</body></html>") :raw ((:status "HTTP/1.1 200 OK" :type "text/html; charset=utf-8" :length 51 :connection "close" :server "impatient-mode parity" :cache "no-cache" :count "2" :location nil :date-valid t :sha256 "94e6ac0af4d106334194fd548877557a0d45e7935ec164aa3f69be6dc099b72b") "<main><h1>Release Ω</h1><p>safe & ready</p></main>") :htmlize ((:status "HTTP/1.1 200 OK" :type "text/html; charset=utf-8" :length 1630 :connection "close" :server "impatient-mode parity" :cache "no-cache" :count "1" :location nil :date-valid t :sha256 "6748afb5ec788a0226b12d97e33494457d6b4a6d7c76a6f0a5a1d0c73cf3b2cd") :source-unchanged t :body "<!DOCTYPE html PUBLIC \"-//W3C//DTD HTML 4.01//EN\">\n<!-- Created by htmlize-1.59 in css mode. -->\n<html>\n  <head>\n    <title>Deploy audit &#955;.el</title>\n    <style type=\"text/css\">\n    <!--\n      body {\n        color: #000000;\n        background-color: #ffffff;\n      }\n      .comment {\n        /* font-lock-comment-face */\n        font-weight: bold;\n        font-style: italic;\n      }\n      .comment-delimiter {\n        /* font-lock-comment-delimiter-face */\n        font-weight: bold;\n        font-style: italic;\n      }\n      .doc {\n        /* font-lock-doc-face */\n        font-style: italic;\n      }\n      .function-name {\n        /* font-lock-function-name-face */\n        font-weight: bold;\n      }\n      .keyword {\n        /* font-lock-keyword-face */\n        font-weight: bold;\n      }\n      .string {\n        /* font-lock-string-face */\n        font-style: italic;\n      }\n\n      a {\n        color: inherit;\n        background-color: inherit;\n        font: inherit;\n        text-decoration: inherit;\n      }\n      a:hover {\n        text-decoration: underline;\n      }\n    -->\n    </style>\n  </head>\n  <body>\n    <pre>\n<span class=\"comment-delimiter\">;; </span><span class=\"comment\">Verify deployment &#937;\n</span>(<span class=\"keyword\">defun</span> <span class=\"function-name\">deployment-status</span> (artifact)\n  <span class=\"doc\">\"Return the production STATUS for ARTIFACT.\"</span>\n  (<span class=\"keyword\">let</span> ((status <span class=\"string\">\"ready\"</span>))\n    (message <span class=\"string\">\"%s:%s\"</span> artifact status)))\n\n(deployment-status <span class=\"string\">\"api\"</span>)\n</pre>\n  </body>\n</html>\n" :document t :comment t :definition t :doc t :binding t :literal t) :custom-filter ((:status "HTTP/1.1 200 OK" :type "text/html; charset=utf-8" :length 42 :connection "close" :server "impatient-mode parity" :cache "no-cache" :count "2" :location nil :date-valid t :sha256 "98221a1cc0f0d4290f4123be72939a005f51fae7b2c51e534c939ba7f2fc985c") "<output data-kind=\"word-count\">24</output>" :source-unchanged t) :restored-filter ((:status "HTTP/1.1 200 OK" :type "text/html; charset=utf-8" :length 1630 :connection "close" :server "impatient-mode parity" :cache "no-cache" :count "3" :location nil :date-valid t :sha256 "6748afb5ec788a0226b12d97e33494457d6b4a6d7c76a6f0a5a1d0c73cf3b2cd") :same-html t) :live-shim ((:status "HTTP/1.1 200 OK" :type "text/html; charset=utf-8" :length 2820 :connection "close" :server "impatient-mode parity" :cache nil :count nil :location nil :date-valid t :sha256 "172ae36d00a70f011d33c30d564243331ea6f70e341281658e00280b8ddd921e") :etag-shape t :modified-exact t :transport-script t) :loading ((:status "HTTP/1.1 200 OK" :type "text/html; charset=utf-8" :length 64 :connection "close" :server "impatient-mode parity" :cache nil :count nil :location nil :date-valid t :sha256 "3991058fa9955e8d092fd29048494d69ac3f77a3b2913a8f44397c2846528f95") :etag-shape t :modified-exact t :body "<!DOCTYPE html>\n<meta charset=\"utf-8\">\n<h1>Loading Content</h1>\n") :jquery ((:status "HTTP/1.1 200 OK" :type "text/javascript; charset=utf-8" :length 219227 :connection "close" :server "impatient-mode parity" :cache nil :count nil :location nil :date-valid t :sha256 "e2107c8ecdb479c36d822d82bda2a8caf4429ab2d2cf9f20d5c931f75275403c") :etag-shape t :modified-exact t :banner t) :states ((t 2 nil) (t 3 nil))) :cleanup (:server nil :httpd-clients nil :network-processes nil :owned-buffers nil :owned-reference-live nil :published nil :root-exists nil))"#
    ]];
    ParityBatchCase::value(
        "browser_visit_listing_filters_and_static_assets_use_real_http",
        elisp_form,
        expect,
    )
}

fn related_resources_and_long_polls_follow_real_project_edits() -> ParityBatchCase {
    let elisp_form = r##"(imp-test-run
 "impatient-mode-related-project"
 (lambda (root)
   (let* ((project (expand-file-name "release site Ω" root))
          (assets (expand-file-name "assets" project))
          (private (expand-file-name "private" project))
          (page-path (expand-file-name "dashboard Ω.html" project))
          (css-path (expand-file-name "site.css" assets))
          (note-path (expand-file-name "harmless.txt" private))
          (page-source
           "<!doctype html>\n<link rel=\"stylesheet\" href=\"assets/site.css\">\n<h1>Release dashboard Ω</h1>\n")
          (css-disk "body { color: navy; }\n")
          (css-memory "body { color: navy; }\n/* unsaved Ω */\n")
          (note-source "fixture-only deployment note\n")
          page css resource exposed related-poll own-poll)
     (make-directory assets t)
     (make-directory private t)
     (with-temp-buffer
       (insert page-source)
       (write-region (point-min) (point-max) page-path nil 'silent))
     (with-temp-buffer
       (insert css-disk)
       (write-region (point-min) (point-max) css-path nil 'silent))
     (with-temp-buffer
       (insert note-source)
       (write-region (point-min) (point-max) note-path nil 'silent))
     (setq page (imp-test-own-buffer (find-file-noselect page-path))
           css (imp-test-own-buffer (find-file-noselect css-path)))
     (with-current-buffer page
       (html-mode)
       (impatient-mode 1)
       (imp-visit-buffer nil))
     (with-current-buffer css
       (css-mode)
       (impatient-mode 1)
       (goto-char (point-max))
       (insert "/* unsaved Ω */\n"))
     (let* ((page-name (buffer-name page))
            (encoded (url-hexify-string page-name))
            (resource-path
             (format "/imp/live/%s/assets/site.css" encoded))
            (exposed-path
             (format "/imp/live/%s/private/harmless.txt" encoded)))
       (setq
        resource
        (imp-test-await-response
         (imp-test-open-client "imp-test-live-css" resource-path))
        exposed
        (imp-test-await-response
         (imp-test-open-client "imp-test-exposed-file" exposed-path)))
       (let ((pending
              (imp-test-open-client
               "imp-test-related-poll"
               (format "/imp/buffer/%s?id=2" encoded))))
         (imp-test-wait
          (lambda ()
            (with-current-buffer page
              (and (= (length imp-client-list) 1)
                   (= (with-current-buffer (plist-get pending :buffer)
                        (buffer-size))
                      0))))
          "related page poll registration with zero response bytes")
         (let ((before
                (with-current-buffer page
                  (list :state imp-last-state
                        :clients (length imp-client-list)
                        :related
                        (mapcar
                         (lambda (file) (file-relative-name file project))
                         imp-related-files))))
               (pending-bytes-before
                (with-current-buffer (plist-get pending :buffer)
                  (buffer-size))))
           (with-current-buffer css
             (goto-char (point-max))
             (imp-test-type ?!))
           (setq related-poll (imp-test-await-response pending))
           (setq related-poll
                 (list
                  :before before
                  :wire-before pending-bytes-before
                  :response (imp-test-response-summary related-poll)
                  :body (plist-get related-poll :body)
                  :after
                  (with-current-buffer page
                    (list :state imp-last-state
                          :clients imp-client-list))
                  :css-after
                  (with-current-buffer css
                    (list :state imp-last-state
                          :contents (buffer-string)))))))
       (let ((pending
              (imp-test-open-client
               "imp-test-own-poll"
               (format "/imp/buffer/%s?id=2" encoded))))
         (imp-test-wait
          (lambda ()
            (with-current-buffer page
              (and (= (length imp-client-list) 1)
                   (= (with-current-buffer (plist-get pending :buffer)
                        (buffer-size))
                      0))))
          "page's own pending poll registration")
         (with-current-buffer page
           (goto-char (point-max))
           (insert "<p>Ready &amp; verified</p>\n"))
         (let ((response (imp-test-await-response pending)))
           (setq own-poll
                 (list
                  :response (imp-test-response-summary response)
                  :body (plist-get response :body)
                  :after
                  (with-current-buffer page
                    (list :state imp-last-state
                          :clients imp-client-list))))))
       (list
        :browser
        (mapcar
         (lambda (event)
           (imp-test-normalize-url (car event)))
         (nreverse imp-test-browser-events))
        :live-css
        (list
         (imp-test-response-summary resource)
         :body (plist-get resource :body)
         :memory-wins (equal (plist-get resource :body) css-memory)
         :disk-unchanged
         (with-temp-buffer
           (insert-file-contents-literally css-path)
           (equal (buffer-string) css-disk)))
        :directory-exposure
        (list (imp-test-response-summary exposed)
              (plist-get exposed :body))
        :related-poll related-poll
        :own-poll own-poll
        :related
        (with-current-buffer page
          (mapcar
           (lambda (file) (file-relative-name file project))
           imp-related-files)))))))"##;
    let expect = expect![[
        r#"OK (:result (:browser ("http://localhost:PORT/imp/live/dashboard%20%CE%A9.html/") :live-css ((:status "HTTP/1.1 200 OK" :type "text/css; charset=utf-8" :length 39 :connection "close" :server "impatient-mode parity" :cache "no-cache" :count nil :location nil :date-valid t :sha256 "77e5c45b36e8cd3a3e295ed6b37611acab176f510dfdfea5e57976a896f363d5") :body "body { color: navy; }\n/* unsaved Ω */\n" :memory-wins t :disk-unchanged t) :directory-exposure ((:status "HTTP/1.1 200 OK" :type "text/plain; charset=utf-8" :length 29 :connection "close" :server "impatient-mode parity" :cache nil :count nil :location nil :date-valid t :sha256 "00caf37b0e9cb4cebef59b72f030223eb4c8a0413afbf22d71649c2fb6962dbf") "fixture-only deployment note\n") :related-poll (:before (:state 2 :clients 1 :related ("private/harmless.txt" "assets/site.css")) :wire-before 0 :response (:status "HTTP/1.1 200 OK" :type "text/html; charset=utf-8" :length 93 :connection "close" :server "impatient-mode parity" :cache "no-cache" :count "2" :location nil :date-valid t :sha256 "4233c599e08046ac342bd898a371fa8c7a7ff4c2f09b62e0281ae0c52285e518") :body "<!doctype html>\n<link rel=\"stylesheet\" href=\"assets/site.css\">\n<h1>Release dashboard Ω</h1>\n" :after (:state 2 :clients nil) :css-after (:state 3 :contents "body { color: navy; }\n/* unsaved Ω */\n!")) :own-poll (:response (:status "HTTP/1.1 200 OK" :type "text/html; charset=utf-8" :length 121 :connection "close" :server "impatient-mode parity" :cache "no-cache" :count "3" :location nil :date-valid t :sha256 "4040db9adbbcc8bec442f40bf6743d4c13b0c63a6ddd524328a954d03e29c74e") :body "<!doctype html>\n<link rel=\"stylesheet\" href=\"assets/site.css\">\n<h1>Release dashboard Ω</h1>\n<p>Ready &amp; verified</p>\n" :after (:state 3 :clients nil)) :related ("private/harmless.txt" "assets/site.css")) :cleanup (:server nil :httpd-clients nil :network-processes nil :owned-buffers nil :owned-reference-live nil :published nil :root-exists nil))"#
    ]];
    ParityBatchCase::value(
        "related_resources_and_long_polls_follow_real_project_edits",
        elisp_form,
        expect,
    )
}

fn failures_disconnects_and_filter_errors_leave_server_recoverable() -> ParityBatchCase {
    let elisp_form = r##"(imp-test-run
 "impatient-mode-failure-boundaries"
 (lambda (_root)
   (let* ((page-name "Incident response Ω.html")
          (private-name "Private draft & notes.html")
          (page (imp-test-buffer page-name))
          (private (imp-test-buffer private-name))
          browser-failure redirect private-response missing-response
          forbidden-response missing-static disconnected filter-failure recovery)
     (with-current-buffer page
       (html-mode)
       (insert "<main>Incident response Ω</main>"))
     (with-current-buffer private
       (html-mode)
       (insert "<p>must remain private</p>"))
     (with-current-buffer page
       (let ((browse-url-browser-function
              (lambda (url &rest _)
                (error "browser boundary blocked %s" url))))
         (setq browser-failure
               (condition-case error-data
                 (list :unexpected (imp-visit-buffer nil))
                 (error
                  (list
                   :signal (car error-data)
                   (mapcar
                    (lambda (datum)
                      (if (stringp datum)
                          (imp-test-normalize-url datum)
                        datum))
                    (cdr error-data)))))))
       (setq browser-failure
             (append
              browser-failure
              (list
               :mode impatient-mode
               :state imp-last-state
               :server (and (httpd-running-p) t))))
       (imp-visit-buffer t))
     (let ((encoded (url-hexify-string page-name)))
       (setq
        redirect
        (imp-test-await-response
         (imp-test-open-client "imp-test-redirect" "/imp"))
        private-response
        (imp-test-await-response
         (imp-test-open-client
          "imp-test-private"
          (format "/imp/live/%s/" (url-hexify-string private-name))))
        missing-response
        (imp-test-await-response
         (imp-test-open-client
          "imp-test-missing"
          "/imp/live/No%20such%20buffer/"))
        forbidden-response
        (imp-test-await-response
         (imp-test-open-client "imp-test-forbidden" "/imp/not-registered"))
        missing-static
        (imp-test-await-response
         (imp-test-open-client
          "imp-test-missing-static" "/imp/static/missing.js")))
       (let ((pending
              (imp-test-open-client
               "imp-test-disconnect"
               (format "/imp/buffer/%s?id=2" encoded))))
         (imp-test-wait
          (lambda ()
            (with-current-buffer page (= (length imp-client-list) 1)))
          "pending client before disconnect")
         (let ((server-peer
                (with-current-buffer page (car imp-client-list))))
           (delete-process (plist-get pending :process))
           (imp-test-wait
            (lambda () (not (process-live-p server-peer)))
            "server peer after browser disconnect")
           (with-current-buffer page
             (goto-char (point-max))
             (imp-test-type ?D))
           (setq disconnected
                 (list
                  :wire-bytes
                  (with-current-buffer (plist-get pending :buffer)
                    (buffer-size))
                  :server-peer-live (and (process-live-p server-peer) t)
                  :page
                  (with-current-buffer page
                    (list :state imp-last-state
                          :clients imp-client-list
                          :contents (buffer-string)))))))
       (with-current-buffer page
         (imp-set-user-filter
          (lambda (_buffer) (error "intentional filter failure"))))
       (let ((pending
              (imp-test-open-client
               "imp-test-filter-error"
               (format "/imp/buffer/%s?id=4" encoded))))
         (imp-test-wait
          (lambda ()
            (with-current-buffer page (= (length imp-client-list) 1)))
          "pending client before filter failure")
         (let ((server-peer
                (with-current-buffer page (car imp-client-list))))
           (with-current-buffer page
             (goto-char (point-max))
             (imp-test-type ?E))
           (setq filter-failure
                 (list
                  :wire-bytes
                  (with-current-buffer (plist-get pending :buffer)
                    (buffer-size))
                  :server-peer-live (and (process-live-p server-peer) t)
                  :page
                  (with-current-buffer page
                    (list :state imp-last-state
                          :clients imp-client-list
                          :contents (buffer-string)))))
           (when (process-live-p (plist-get pending :process))
             (delete-process (plist-get pending :process)))
           (imp-test-wait
            (lambda () (not (process-live-p server-peer)))
            "filter-failure peer disconnect")))
       (with-current-buffer page
         (imp-remove-user-filter))
       (setq recovery
             (imp-test-await-response
              (imp-test-open-client
               "imp-test-recovery"
               (format "/imp/buffer/%s?id=-1" encoded))))
       (list
        :browser-failure browser-failure
        :browser-recovery
        (mapcar
         (lambda (event) (imp-test-normalize-url (car event)))
         (nreverse imp-test-browser-events))
        :redirect
        (list (imp-test-response-summary redirect)
              (plist-get redirect :body))
        :private
        (list (imp-test-response-summary private-response)
              (plist-get private-response :body))
        :missing
        (list (imp-test-response-summary missing-response)
              (plist-get missing-response :body))
        :forbidden
        (list (imp-test-response-summary forbidden-response)
              (plist-get forbidden-response :body))
        :missing-static
        (list (imp-test-response-summary missing-static)
              (plist-get missing-static :body))
        :disconnect disconnected
        :filter-failure filter-failure
        :recovery
        (list (imp-test-response-summary recovery)
              (plist-get recovery :body)
              :page
              (with-current-buffer page
                (list :state imp-last-state
                      :clients imp-client-list
                      :filter imp-user-filter))))))))"##;
    let expect = expect![[
        r#"OK (:result (:browser-failure (:signal error ("browser boundary blocked http://localhost:PORT/imp/live/Incident%20response%20%CE%A9.html/") :mode t :state 2 :server t) :browser-recovery ("http://localhost:PORT/imp/") :redirect ((:status "HTTP/1.1 301 Moved Permanently" :type "text/plain; charset=utf-8" :length 0 :connection "close" :server "impatient-mode parity" :cache nil :count nil :location "/imp/" :date-valid t :sha256 "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855") "") :private ((:status "HTTP/1.1 403 Forbidden" :type "text/html; charset=utf-8" :length 225 :connection "close" :server "impatient-mode parity" :cache nil :count nil :location nil :date-valid t :sha256 "542475e5c428b4ed5c69594643bf54bb41faffe62faa27145e8c1e1375c4e1c0") "<!DOCTYPE html>\n<html><head><title>403 Forbidden</title></head><body>\n<h1>403 Forbidden</h1>\n<p>An error occurred.</p>\n<pre>error: Buffer Private draft &amp; notes.html is private or doesn&apos;t exist.\n</pre>\n</body></html>\n") :missing ((:status "HTTP/1.1 403 Forbidden" :type "text/html; charset=utf-8" :length 209 :connection "close" :server "impatient-mode parity" :cache nil :count nil :location nil :date-valid t :sha256 "8ea55913243edfc218e68719e67010c72ad6174fe056f9bc13361b32b680c92f") "<!DOCTYPE html>\n<html><head><title>403 Forbidden</title></head><body>\n<h1>403 Forbidden</h1>\n<p>An error occurred.</p>\n<pre>error: Buffer No such buffer is private or doesn&apos;t exist.\n</pre>\n</body></html>\n") :forbidden ((:status "HTTP/1.1 403 Forbidden" :type "text/html; charset=utf-8" :length 183 :connection "close" :server "impatient-mode parity" :cache nil :count nil :location nil :date-valid t :sha256 "9871a66147686face091e2adfc310d8e2523f062c3b3c7c8b2095255d5400c69") "<!DOCTYPE html>\n<html><head><title>403 Forbidden</title></head><body>\n<h1>403 Forbidden</h1>\n<p>An error occurred.</p>\n<pre>error: /imp/not-registered not found\n</pre>\n</body></html>\n") :missing-static ((:status "HTTP/1.1 404 Not Found" :type "text/html; charset=utf-8" :length 174 :connection "close" :server "impatient-mode parity" :cache nil :count nil :location nil :date-valid t :sha256 "447a591071f9c0f64f35d3ff008944ba758e59a9a5443b9d405645da092384c1") "<!DOCTYPE html>\n<html><head><title>404 Not Found</title></head><body>\n<h1>404 Not Found</h1>\n<p>The requested URL was not found on this server.</p>\n<pre></pre></body></html>\n") :disconnect (:wire-bytes 0 :server-peer-live nil :page (:state 3 :clients nil :contents "<main>Incident response Ω</main>D")) :filter-failure (:wire-bytes 0 :server-peer-live t :page (:state 5 :clients nil :contents "<main>Incident response Ω</main>DE")) :recovery ((:status "HTTP/1.1 200 OK" :type "text/html; charset=utf-8" :length 35 :connection "close" :server "impatient-mode parity" :cache "no-cache" :count "7" :location nil :date-valid t :sha256 "41718808918890d6056c8b9b96dcddcd5c994e0227c2f7f8578eb455234e67cd") "<main>Incident response Ω</main>DE" :page (:state 7 :clients nil :filter nil))) :cleanup (:server nil :httpd-clients nil :network-processes nil :owned-buffers nil :owned-reference-live nil :published nil :root-exists nil))"#
    ]];
    ParityBatchCase::value(
        "failures_disconnects_and_filter_errors_leave_server_recoverable",
        elisp_form,
        expect,
    )
}

pub(super) fn public_workflow_cases() -> Vec<ParityBatchCase> {
    vec![
        modes_filters_edits_and_delay_follow_public_lifecycle(),
        browser_visit_listing_filters_and_static_assets_use_real_http(),
        related_resources_and_long_polls_follow_real_project_edits(),
        failures_disconnects_and_filter_errors_leave_server_recoverable(),
    ]
}
