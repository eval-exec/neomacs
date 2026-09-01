use std::time::Duration;

use expect_test::expect;

use crate::{
    COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, JS2_MODE_MELPA_PIN, SIMPLE_HTTPD_MELPA_PIN,
    SKEWER_MODE_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const SKEWER_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const SKEWER_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'skewer-mode)
(require 'skewer-css)
(require 'skewer-html)
(require 'skewer-repl)
(require 'skewer-bower)

(setq httpd-log-buffer nil
      httpd-server-name "skewer parity")

(defun skewer-test-reset ()
  (setq skewer-clients nil
        skewer-queue nil
        skewer-callbacks (cache-table-create skewer-timeout :test 'equal)
        skewer-eval-print-map (cache-table-create skewer-timeout :test 'equal)
        skewer-hosted-scripts (cache-table-create skewer-timeout)
        skewer--last-timestamp 0
        skewer-response-hook nil
        skewer-js-hook nil)
  (dolist (name '("*skewer-clients*" "*skewer-error*" "*skewer-repl*"))
    (when-let ((buffer (get-buffer name)))
      (when-let ((process (get-buffer-process buffer)))
        (set-process-query-on-exit-flag process nil)
        (set-process-sentinel process #'ignore)
        (delete-process process))
      (kill-buffer buffer))))

(defun skewer-test-cancel-js2-timer ()
  (when (and (boundp 'js2-mode-parse-timer)
             (timerp js2-mode-parse-timer))
    (cancel-timer js2-mode-parse-timer)
    (setq js2-mode-parse-timer nil)))

(defun skewer-test-activate-js2 ()
  (let ((js2-idle-timer-delay 3600))
    (js2-mode))
  (skewer-test-cancel-js2-timer)
  (js2-reparse 'force)
  (skewer-test-cancel-js2-timer))

(defun skewer-test-parse-wire (wire)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert wire)
    (let* ((headers (httpd-parse))
           (date-header (assoc "Date" headers))
           (date (cadr date-header)))
      (list
       :headers (delq date-header headers)
       :date-valid
       (and date
            (equal date (httpd-date-string (date-to-time date))))
       :body (buffer-substring-no-properties (point) (point-max))))))

(defun skewer-test-capture (request responder)
  (let ((properties (list (cons :request-active request)))
        chunks
        deleted)
    (cl-letf (((symbol-function 'process-get)
               (lambda (_process property)
                 (alist-get property properties)))
              ((symbol-function 'process-put)
               (lambda (_process property value)
                 (setf (alist-get property properties) value)))
              ((symbol-function 'process-send-string)
               (lambda (_process string)
                 (push string chunks)))
              ((symbol-function 'process-send-region)
               (lambda (_process start end)
                 (push (buffer-substring-no-properties start end) chunks)))
              ((symbol-function 'process-contact)
               (lambda (_process &optional _key _no-block)
                 '("127.0.0.1" 4242)))
              ((symbol-function 'delete-process)
               (lambda (_process) (setq deleted t))))
      (funcall responder 'skewer-test-client))
    (append
     (skewer-test-parse-wire (apply #'concat (nreverse chunks)))
     (list
      :active-after (alist-get :request-active properties)
      :closed deleted))))

(skewer-test-reset)
"####;

fn skewer_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SKEWER_MODE_MELPA_PIN, "skewer-mode.el")
        .expect("prepare pinned skewer-mode source below ./tmp")
        .with_melpa_dependency(JS2_MODE_MELPA_PIN)
        .expect("prepare pinned js2-mode dependency below ./tmp")
        .with_melpa_dependency(SIMPLE_HTTPD_MELPA_PIN)
        .expect("prepare pinned simple-httpd dependency below ./tmp")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare pinned Compat dependency below ./tmp")
        .with_prelude(SKEWER_MODE_TEST_PRELUDE)
        .with_timeout(SKEWER_MODE_TEST_TIMEOUT)
}

fn browser_post_completes_a_queued_evaluation_and_receives_the_next_long_poll_payload()
-> ParityBatchCase {
    let elisp_form = r####"
(progn
  (skewer-test-reset)
  (let (callback-values hook-values evaluation response callback-retained)
    (cl-letf (((symbol-function 'random) (lambda (&optional _limit) 2748))
              ((symbol-function 'float-time) (lambda (&optional _time) 1000.25)))
      (setq skewer-response-hook
            (list (lambda (result) (push result hook-values))))
      (setq evaluation
            (skewer-eval
             "cart.total + cart.tax"
             (lambda (result) (push result callback-values))
             :verbose t
             :strict t
             :extra '((context . "checkout"))))
      (setq response
            (skewer-test-capture
             `(("POST" "/skewer/post" "HTTP/1.1")
               ("User-Agent" "ParityBrowser/1.0")
               ("Content-Type" "application/json")
               ("Content"
                ,(json-encode
                  '((id . "abc")
                    (status . "success")
                    (value . "49.95")
                    (time . 0.125)))))
             (lambda (client)
               (httpd/skewer/post client nil nil
                                  `(("Content"
                                     ,(json-encode
                                       '((id . "abc")
                                         (status . "success")
                                         (value . "49.95")
                                         (time . 0.125))))
                                    ("User-Agent" "ParityBrowser/1.0"))))))
      (setq callback-retained
            (functionp (cache-table-get "abc" skewer-callbacks))))
    (list
     :evaluation evaluation
     :callbacks (nreverse callback-values)
     :hooks (nreverse hook-values)
     :response
     (list
      :status (car (plist-get response :headers))
      :type (cadr (assoc "Content-Type" (plist-get response :headers)))
      :cache (cadr (assoc "Cache-Control" (plist-get response :headers)))
      :cors (cadr (assoc "Access-Control-Allow-Origin"
                         (plist-get response :headers)))
      :json (json-read-from-string (plist-get response :body))
      :active-after (plist-get response :active-after))
     :queue skewer-queue
     :clients skewer-clients
     :last-timestamp skewer--last-timestamp
     :callback-retained callback-retained)))
"####;
    let expect = expect![[
        r####"OK (:evaluation ((type . "eval") (eval . "cart.total + cart.tax") (id . "abc") (verbose . t) (strict . t) (context . "checkout")) :callbacks (#1=((id . "abc") (status . "success") (value . "49.95") (time . 0.125))) :hooks (#1#) :response (:status ("HTTP/1.1" "200" "OK") :type "text/plain; charset=utf-8" :cache "no-cache" :cors "*" :json ((type . "eval") (eval . "cart.total + cart.tax") (id . "abc") (verbose . t) (strict . t) (context . "checkout")) :active-after nil) :queue nil :clients nil :last-timestamp 1000.25 :callback-retained t)"####
    ]];
    ParityBatchCase::value(
        "browser_post_completes_a_queued_evaluation_and_receives_the_next_long_poll_payload",
        elisp_form,
        expect,
    )
}

fn javascript_editor_commands_select_real_ast_expressions_and_preserve_strict_context()
-> ParityBatchCase {
    let elisp_form = r####"
(progn
  (skewer-test-reset)
  (with-temp-buffer
    (insert
     "\"use strict\";\n"
     "const cart = { subtotal: 45, tax: 4.95 };\n"
     "function checkout(order) {\n"
     "  const total = order.subtotal + order.tax;\n"
     "  return { id: \"REL-417\", total };\n"
     "}\n"
     "checkout(cart);\n")
    (skewer-test-activate-js2)
    (skewer-mode 1)
    (set-buffer-modified-p nil)
    (let (evaluations flashes last-expression defun-expression)
      (goto-char (point-max))
      (setq last-expression (skewer-get-last-expression))
      (goto-char (point-min))
      (search-forward "const total")
      (setq defun-expression (skewer-get-defun))
      (cl-letf (((symbol-function 'skewer-eval)
                 (lambda (string &optional callback &rest arguments)
                   (push
                    (list string
                          (and (symbolp callback) callback)
                          arguments)
                    evaluations)
                   '((id . "editor-request"))))
                ((symbol-function 'skewer-flash-region)
                 (lambda (start end &optional timeout)
                   (push
                    (list start end timeout
                          (buffer-substring-no-properties start end))
                    flashes))))
        (goto-char (point-max))
        (skewer-eval-last-expression)
        (goto-char (point-min))
        (search-forward "const total")
        (skewer-eval-defun))
      (list
       :strict (skewer-mode-strict-p)
       :last-expression last-expression
       :defun-expression defun-expression
       :evaluations (nreverse evaluations)
       :flashes (nreverse flashes)
       :mode
       (list
        skewer-mode
        (lookup-key skewer-mode-map (kbd "C-x C-e"))
        (lookup-key skewer-mode-map (kbd "C-M-x"))
        (lookup-key skewer-mode-map (kbd "C-c C-k")))
       :point (point)
       :modified (buffer-modified-p)))))
"####;
    let expect = expect![[
        r####"OK (:strict t :last-expression ("checkout(cart);" 165 180) :defun-expression ("function checkout(order) {\n  const total = order.subtotal + order.tax;\n  return { id: \"REL-417\", total };\n}" 57 164) :evaluations (("checkout(cart);" skewer-post-minibuffer nil) ("function checkout(order) {\n  const total = order.subtotal + order.tax;\n  return { id: \"REL-417\", total };\n}" skewer-post-minibuffer nil)) :flashes ((165 180 nil "checkout(cart);") (57 164 nil "function checkout(order) {\n  const total = order.subtotal + order.tax;\n  return { id: \"REL-417\", total };\n}")) :mode (t skewer-eval-last-expression skewer-eval-defun skewer-load-buffer) :point 97 :modified nil)"####
    ]];
    ParityBatchCase::value(
        "javascript_editor_commands_select_real_ast_expressions_and_preserve_strict_context",
        elisp_form,
        expect,
    )
}

fn css_live_editing_sends_the_current_declaration_rule_stylesheet_and_clear_command()
-> ParityBatchCase {
    let elisp_form = r####"
(progn
  (skewer-test-reset)
  (with-temp-buffer
    (insert
     ":root { --release-accent: #4b7bec; }\n\n"
     ".checkout-card,\n"
     ".checkout-summary {\n"
     "  display: grid;\n"
     "  gap: 1.25rem;\n"
     "  color: rgb(20, 30, 40);\n"
     "}\n")
    (css-mode)
    (skewer-css-mode 1)
    (set-buffer-modified-p nil)
    (goto-char (point-min))
    (search-forward "gap: 1.25rem")
    (let ((selectors (skewer-css-selectors))
          (declaration (skewer-css-declaration))
          requests
          flashes)
      (cl-letf (((symbol-function 'skewer-eval)
                 (lambda (string &optional callback &rest arguments)
                   (push (list string callback arguments) requests)
                   '((id . "css-request"))))
                ((symbol-function 'skewer-flash-region)
                 (lambda (start end &optional timeout)
                   (push
                    (list start end timeout
                          (buffer-substring-no-properties start end))
                    flashes))))
        (skewer-css-eval-current-declaration)
        (skewer-css-eval-current-rule)
        (skewer-css-eval-buffer)
        (skewer-css-clear-all))
      (list
       :selection (list selectors declaration)
       :requests (nreverse requests)
       :flashes (nreverse flashes)
       :mode
       (list
        skewer-css-mode
        (lookup-key skewer-css-mode-map (kbd "C-x C-e"))
        (lookup-key skewer-css-mode-map (kbd "C-M-x"))
        (lookup-key skewer-css-mode-map (kbd "C-c C-k"))
        (lookup-key skewer-css-mode-map (kbd "C-c C-c")))
       :content (buffer-string)
       :modified (buffer-modified-p)))))
"####;
    let expect = expect![[
        r####"OK (:selection (".checkout-card, .checkout-summary" ("gap" "1.25rem;")) :requests ((".checkout-card, .checkout-summary { gap: 1.25rem; }" nil (:type "css")) (".checkout-card, .checkout-summary { display: grid; gap: 1.25rem; color: rgb(20, 30, 40); }" nil (:type "css")) (":root { --release-accent: #4b7bec; }\n\n.checkout-card,\n.checkout-summary {\n  display: grid;\n  gap: 1.25rem;\n  color: rgb(20, 30, 40);\n}\n" nil (:type "css")) (nil nil (:type "cssClearAll"))) :flashes ((94 107 nil "gap: 1.25rem;") (39 135 nil ".checkout-card,\n.checkout-summary {\n  display: grid;\n  gap: 1.25rem;\n  color: rgb(20, 30, 40);\n}")) :mode (t skewer-css-eval-current-declaration skewer-css-eval-current-rule skewer-css-eval-buffer skewer-css-clear-all) :content ":root { --release-accent: #4b7bec; }\n\n.checkout-card,\n.checkout-summary {\n  display: grid;\n  gap: 1.25rem;\n  color: rgb(20, 30, 40);\n}\n" :modified nil)"####
    ]];
    ParityBatchCase::value(
        "css_live_editing_sends_the_current_declaration_rule_stylesheet_and_clear_command",
        elisp_form,
        expect,
    )
}

fn html_live_editing_computes_a_unique_selector_fetches_markup_and_sends_tag_updates()
-> ParityBatchCase {
    let elisp_form = r####"
(progn
  (skewer-test-reset)
  (with-temp-buffer
    (insert
     "<!doctype html>\n"
     "<html><body>\n"
     "  <main id=\"release-dashboard\">\n"
     "    <section class=\"card\"><h2>Draft</h2></section>\n"
     "    <section class=\"card\">\n"
     "      <h2>Release 42</h2>\n"
     "      <p>Status: <strong>Ready</strong></p>\n"
     "    </section>\n"
     "  </main>\n"
     "</body></html>\n")
    (html-mode)
    (skewer-html-mode 1)
    (set-buffer-modified-p nil)
    (goto-char (point-min))
    (search-forward "Ready")
    (let ((ancestry (skewer-html-compute-tag-ancestry))
          (selector (skewer-html-compute-selector))
          requests
          flashes
          fetched)
      (cl-letf (((symbol-function 'skewer-eval)
                 (lambda (string &optional callback &rest arguments)
                   (push (list string callback arguments) requests)
                   '((id . "html-request"))))
                ((symbol-function 'skewer-eval-synchronously)
                 (lambda (string &rest arguments)
                   (list
                    '(status . "success")
                    (cons
                     'value
                     (format
                      "<strong data-selector=\"%s\">Ready</strong>"
                      string))
                    (cons 'arguments arguments))))
                ((symbol-function 'skewer-flash-region)
                 (lambda (start end &optional timeout)
                   (push
                    (list start end timeout
                          (buffer-substring-no-properties start end))
                    flashes))))
        (setq fetched (skewer-html-fetch-selector selector))
        (skewer-html-eval-tag)
        (skewer-html-eval
         "<li data-release=\"42\">Ready</li>"
         '(("body" 0) ("main" 0) ("ul" 1))
         t))
      (list
       :ancestry ancestry
       :selector selector
       :fetched fetched
       :requests (nreverse requests)
       :flashes (nreverse flashes)
       :mode
       (list skewer-html-mode
             (lookup-key skewer-html-mode-map (kbd "C-M-x")))
       :point (point)
       :content (buffer-substring-no-properties (point-min) (point-max))
       :modified (buffer-modified-p)))))
"####;
    let expect = expect![[
        r####"OK (:ancestry (("body" 1) ("main" 1) ("section" 2) ("p" 1) ("strong" 1)) :selector "body:nth-of-type(1) > main:nth-of-type(1) > section:nth-of-type(2) > p:nth-of-type(1) > strong:nth-of-type(1)" :fetched "<strong data-selector=\"body:nth-of-type(1) > main:nth-of-type(1) > section:nth-of-type(2) > p:nth-of-type(1) > strong:nth-of-type(1)\">Ready</strong>" :requests (("<strong>Ready</strong>" nil (:type "html" :extra ((ancestry . [("body" 1) ("main" 1) ("section" 2) ("p" 1) ("strong" 1)]) (append)))) ("<li data-release=\"42\">Ready</li>" nil (:type "html" :extra ((ancestry . [("body" 0) ("main" 0) ("ul" 1)]) (append . t))))) :flashes ((205 183 nil "<strong>Ready</strong>")) :mode (t skewer-html-eval-tag) :point 196 :content "<!doctype html>\n<html><body>\n  <main id=\"release-dashboard\">\n    <section class=\"card\"><h2>Draft</h2></section>\n    <section class=\"card\">\n      <h2>Release 42</h2>\n      <p>Status: <strong>Ready</strong></p>\n    </section>\n  </main>\n</body></html>\n" :modified nil)"####
    ]];
    ParityBatchCase::value(
        "html_live_editing_computes_a_unique_selector_fetches_markup_and_sends_tag_updates",
        elisp_form,
        expect,
    )
}

fn loading_a_named_buffer_hosts_an_immutable_script_and_serves_the_injected_runtime()
-> ParityBatchCase {
    let elisp_form = r####"
(progn
  (skewer-test-reset)
  (with-temp-buffer
    (rename-buffer "release plan.js" t)
    (insert
     "const release = { id: \"REL-417\", state: \"ready\" };\n"
     "window.releasePlan = release;\n")
    (set-buffer-modified-p nil)
    (let (evaluation callback-called script-response runtime-response)
      (cl-letf (((symbol-function 'random)
                 (lambda (&optional _limit) 417))
                ((symbol-function 'skewer-eval)
                 (lambda (string &optional callback &rest arguments)
                   (setq evaluation
                         (list string (functionp callback) arguments))
                   (when callback
                     (funcall callback '((status . "success"))))
                   (setq callback-called t)
                   '((id . "load-request")))))
        (skewer-load-buffer))
      (setq skewer-js-hook
            (list
             (lambda ()
               (goto-char (point-max))
               (insert "\nwindow.parityInjected = true;\n"))))
      (setq script-response
            (skewer-test-capture
             '(("GET" "/skewer/script/417/release%20plan.js" "HTTP/1.1")
               ("Host" "localhost"))
             (lambda (client)
               (httpd/skewer/script
                client "/skewer/script/417/release%20plan.js" nil nil))))
      (setq runtime-response
            (skewer-test-capture
             '(("GET" "/skewer" "HTTP/1.1") ("Host" "localhost"))
             (lambda (client)
               (httpd/skewer client "/skewer" nil nil))))
      (let ((runtime (plist-get runtime-response :body)))
        (list
         :evaluation evaluation
         :callback-called callback-called
         :cached (cache-table-get 417 skewer-hosted-scripts)
         :script
         (list
          :status (car (plist-get script-response :headers))
          :type (cadr (assoc "Content-Type"
                             (plist-get script-response :headers)))
          :body (plist-get script-response :body))
         :runtime
         (list
          :status (car (plist-get runtime-response :headers))
          :type (cadr (assoc "Content-Type"
                             (plist-get runtime-response :headers)))
          :bytes (string-bytes runtime)
          :sha256 (secure-hash 'sha256 runtime)
          :injected (string-suffix-p
                     "\nwindow.parityInjected = true;\n" runtime))
         :modified (buffer-modified-p))))))
"####;
    let expect = expect![[
        r####"OK (:evaluation ("/skewer/script/417/release%20plan.js" t (:type "script")) :callback-called t :cached "const release = { id: \"REL-417\", state: \"ready\" };\nwindow.releasePlan = release;\n" :script (:status ("HTTP/1.1" "200" "OK") :type "text/javascript; charset=UTF-8" :body "const release = { id: \"REL-417\", state: \"ready\" };\nwindow.releasePlan = release;\n") :runtime (:status ("HTTP/1.1" "200" "OK") :type "text/javascript; charset=UTF-8" :bytes 12797 :sha256 "45e982140056abf420b90197865c8f4009dc8b541b9b34c0788f2a2ccf618325" :injected t) :modified nil)"####
    ]];
    ParityBatchCase::value(
        "loading_a_named_buffer_hosts_an_immutable_script_and_serves_the_injected_runtime",
        elisp_form,
        expect,
    )
}

fn repl_session_sends_strict_input_records_history_and_formats_results_and_browser_logs()
-> ParityBatchCase {
    let elisp_form = r####"
(progn
  (skewer-test-reset)
  (let ((buffer (get-buffer-create "*skewer-repl*"))
        requests)
    (unwind-protect
        (with-current-buffer buffer
          (skewer-repl-mode)
          (setq skewer-repl-strict-p t)
          (cl-letf (((symbol-function 'skewer-eval)
                     (lambda (string &optional callback &rest arguments)
                       (push
                        (list string
                              (and (symbolp callback) callback)
                              arguments)
                        requests)
                       '((id . "repl-request")))))
            (goto-char (point-max))
            (insert "cart.total + cart.tax")
            (comint-send-input))
          (skewer-post-repl '((value . "49.95")))
          (skewer-post-log
           '((type . "error")
             (value . "payment gateway unavailable")
             (filename . "https://localhost/app/checkout.js")
             (line . 42)
             (column . 17)))
          (goto-char (point-min))
          (search-forward "payment gateway unavailable")
          (let ((log-face
                 (get-text-property
                  (- (point) (length "payment gateway unavailable"))
                  'font-lock-face)))
            (goto-char (point-min))
            (search-forward "cart.total + cart.tax")
            (let ((input-start (- (point) (length "cart.total + cart.tax"))))
              (goto-char (point-max))
              (re-search-backward (regexp-quote skewer-repl-prompt))
              (list
               :buffer (buffer-substring-no-properties (point-min) (point-max))
               :requests (nreverse requests)
               :history (ring-elements comint-input-ring)
               :input-properties
               (list
                (get-text-property input-start 'field)
                (get-text-property input-start 'font-lock-face))
               :prompt-properties
               (list
                (get-text-property (point) 'field)
                (get-text-property (point) 'font-lock-face))
               :log-face log-face
               :process
               (let ((process (skewer-repl-process)))
                 (list
                  (process-name process)
                  (and (process-live-p process) t)
                  (marker-position (process-mark process))))
               :compilation-mode compilation-shell-minor-mode
               :compilation-regexp compilation-error-regexp-alist
               :completion-expressions
               (mapcar
                #'skewer-repl--get-completion-expression
                '("window.checkout.total" "document" "cart.items.length"))))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (when-let ((process (get-buffer-process buffer)))
            (set-process-query-on-exit-flag process nil)
            (set-process-sentinel process #'ignore)
            (delete-process process)))
        (kill-buffer buffer)))))
"####;
    let expect = expect![[
        r####"OK (:buffer "*** Welcome to Skewer ***\njs> cart.total + cart.tax\n49.95\npayment gateway unavailable\n    at https://localhost/app/checkout.js:42:17\njs> " :requests (("cart.total + cart.tax" skewer-post-repl (:verbose t :strict t))) :history ("cart.total + cart.tax") :input-properties (nil comint-highlight-input) :prompt-properties (output (comint-highlight-prompt)) :log-face skewer-error-face :process ("skewer-repl" t 138) :compilation-mode t :compilation-regexp (("^[ ]*at https?://[^/]+/\\(?:[^/]+/\\)\\{1\\}\\([^:?#]+\\)\\(?:[?#][^:]*\\)?:\\([[:digit:]]+\\)\\(?::\\([[:digit:]]+\\)\\)?$" 1 2 3 2)) :completion-expressions ("window.checkout" nil "cart.items"))"####
    ]];
    ParityBatchCase::value(
        "repl_session_sends_strict_input_records_history_and_formats_results_and_browser_logs",
        elisp_form,
        expect,
    )
}

fn evaluation_results_render_fast_and_slow_messages_and_actionable_error_details() -> ParityBatchCase
{
    let elisp_form = r####"
(progn
  (skewer-test-reset)
  (let (messages)
    (cl-letf (((symbol-function 'message)
               (lambda (format-string &rest arguments)
                 (let ((rendered
                        (apply #'format-message format-string arguments)))
                   (push rendered messages)
                   rendered))))
      (skewer-post-minibuffer
       '((status . "success") (value . "49.95") (time . 0.125)))
      (skewer-post-minibuffer
       '((status . "success") (value . "deployed") (time . 1.234))))
    (skewer-post-minibuffer
     '((status . "error")
       (strict . t)
       (error
        (name . "TypeError")
        (message . "Cannot read properties of undefined")
        (stack . "checkout@https://localhost/app/checkout.js:42:17\nsubmit@https://localhost/app/ui.js:9:3")
        (eval . "cart.payment.charge()"))))
    (with-current-buffer "*skewer-error*"
      (let ((name-face (get-text-property (point-min) 'font-lock-face)))
        (list
         :messages (nreverse messages)
         :success-p
         (mapcar
          #'skewer-success-p
          '(((status . "success"))
            ((status . "error"))
            ((value . "missing-status"))))
         :error-buffer
         (list
          :mode major-mode
          :read-only buffer-read-only
          :truncate truncate-lines
          :point (point)
          :name-face name-face
          :content (buffer-substring-no-properties (point-min) (point-max))))))))
"####;
    let expect = expect![[
        r####"OK (:messages ("49.95" "deployed (1.234 seconds)") :success-p (t nil nil) :error-buffer (:mode skewer-error-mode :read-only t :truncate t :point 1 :name-face skewer-error-face :content "TypeError: Cannot read properties of undefined\n\ncheckout@https://localhost/app/checkout.js:42:17\nsubmit@https://localhost/app/ui.js:9:3\n\nExpression: (strict)\n\ncart.payment.charge()"))"####
    ]];
    ParityBatchCase::value(
        "evaluation_results_render_fast_and_slow_messages_and_actionable_error_details",
        elisp_form,
        expect,
    )
}

fn one_evaluation_broadcasts_identical_json_to_all_waiting_browser_clients() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (skewer-test-reset)
  (let ((properties (make-hash-table :test #'equal))
        (wires (make-hash-table :test #'eq))
        evaluation
        tabulated)
    (dolist (process '(browser-a browser-b))
      (puthash
       (cons process :request-active)
       '(("GET" "/skewer/get" "HTTP/1.1") ("Host" "localhost"))
       properties))
    (setq skewer-clients
          (list
           (make-skewer-client :proc 'browser-a :agent "Chrome/126 Linux")
           (make-skewer-client :proc 'browser-b :agent "Firefox/128 macOS")))
    (cl-letf (((symbol-function 'process-contact)
               (lambda (process &optional _key _no-block)
                 (if (eq process 'browser-a)
                     '("10.0.0.10" 5010)
                   '("10.0.0.11" 5011))))
              ((symbol-function 'process-get)
               (lambda (process property)
                 (gethash (cons process property) properties)))
              ((symbol-function 'process-put)
               (lambda (process property value)
                 (puthash (cons process property) value properties)))
              ((symbol-function 'process-send-string)
               (lambda (process string)
                 (puthash
                  process
                  (append (gethash process wires) (list string))
                  wires)))
              ((symbol-function 'process-send-region)
               (lambda (process start end)
                 (puthash
                  process
                  (append
                   (gethash process wires)
                   (list (buffer-substring-no-properties start end)))
                  wires)))
              ((symbol-function 'delete-process) (lambda (_process) nil))
              ((symbol-function 'random) (lambda (&optional _limit) 42))
              ((symbol-function 'float-time) (lambda (&optional _time) 500.0)))
      (setq tabulated (skewer-clients-tabulate))
      (setq evaluation
            (skewer-eval
             "window.releaseDashboard.refresh()"
             nil
             :type "eval"
             :extra '((reason . "deployment-complete")))))
    (list
     :tabulated
     (mapcar
      (lambda (entry) (append (cadr entry) nil))
      tabulated)
     :evaluation evaluation
     :responses
     (mapcar
      (lambda (process)
        (let ((response
               (skewer-test-parse-wire
                (apply #'concat (gethash process wires)))))
          (list
           process
           :status (car (plist-get response :headers))
           :type (cadr (assoc "Content-Type" (plist-get response :headers)))
           :cache (cadr (assoc "Cache-Control" (plist-get response :headers)))
           :cors (cadr (assoc "Access-Control-Allow-Origin"
                              (plist-get response :headers)))
           :json (json-read-from-string (plist-get response :body))
           :active-after
           (gethash (cons process :request-active) properties))))
      '(browser-a browser-b))
     :queue skewer-queue
     :clients skewer-clients
     :last-timestamp skewer--last-timestamp)))
"####;
    let expect = expect![[
        r####"OK (:tabulated (("10.0.0.10" "5010" "Chrome/126 Linux") ("10.0.0.11" "5011" "Firefox/128 macOS")) :evaluation ((type . "eval") (eval . "window.releaseDashboard.refresh()") (id . "2a") (verbose) (strict) (reason . "deployment-complete")) :responses ((browser-a :status ("HTTP/1.1" "200" "OK") :type "text/plain; charset=utf-8" :cache "no-cache" :cors "*" :json ((type . "eval") (eval . "window.releaseDashboard.refresh()") (id . "2a") (verbose) (strict) (reason . "deployment-complete")) :active-after nil) (browser-b :status ("HTTP/1.1" "200" "OK") :type "text/plain; charset=utf-8" :cache "no-cache" :cors "*" :json ((type . "eval") (eval . "window.releaseDashboard.refresh()") (id . "2a") (verbose) (strict) (reason . "deployment-complete")) :active-after nil)) :queue nil :clients nil :last-timestamp 500.0)"####
    ]];
    ParityBatchCase::value(
        "one_evaluation_broadcasts_identical_json_to_all_waiting_browser_clients",
        elisp_form,
        expect,
    )
}

#[test]
fn skewer_mode_package_batch() {
    let cases = vec![
        browser_post_completes_a_queued_evaluation_and_receives_the_next_long_poll_payload(),
        javascript_editor_commands_select_real_ast_expressions_and_preserve_strict_context(),
        css_live_editing_sends_the_current_declaration_rule_stylesheet_and_clear_command(),
        html_live_editing_computes_a_unique_selector_fetches_markup_and_sends_tag_updates(),
        loading_a_named_buffer_hosts_an_immutable_script_and_serves_the_injected_runtime(),
        repl_session_sends_strict_input_records_history_and_formats_results_and_browser_logs(),
        evaluation_results_render_fast_and_slow_messages_and_actionable_error_details(),
        one_evaluation_broadcasts_identical_json_to_all_waiting_browser_clients(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed skewer-mode parity test");
    assert_oracle_batch_cases(
        skewer_mode_oracle(),
        test_name,
        "skewer_mode_parity",
        &cases,
    );
}
