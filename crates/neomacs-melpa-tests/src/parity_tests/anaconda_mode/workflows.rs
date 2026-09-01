use expect_test::expect;

use super::ParityBatchCase;

fn starting_the_server_builds_the_documented_command_line_and_binds_its_port() -> ParityBatchCase {
    ParityBatchCase::value(
        "starting_the_server_builds_the_documented_command_line_and_binds_its_port",
        r##"
        ;; A user opens a Python file and anaconda-mode brings its server up.
        ;; The package must run the packaged `anaconda-mode.py' from the
        ;; interpreter it was told about, pass the version-stamped server
        ;; directory, the localhost address and the (empty) virtualenv root,
        ;; work from the installation directory it creates, read the port out
        ;; of the server's announcement, and call back once it is bound.
        ;; Starting again while that server is healthy must reuse it.
        (let ((buffer nil))
          (unwind-protect
              (let ((port (ana-test-setup)))
                (setq buffer (ana-test-visit))
                (list
                 :installation-directory-missing-before
                 (file-directory-p anaconda-mode-installation-directory)
                 :started (ana-test-start-server)
                 :port-is-the-one-announced (= port (anaconda-mode-port))
                 :host (ana-test-copy (anaconda-mode-host))
                 :localhost-address (ana-test-copy anaconda-mode-localhost-address)
                 :server-version anaconda-mode-server-version
                 :server-directory (anaconda-mode-server-directory)
                 :installation-directory-created
                 (file-directory-p anaconda-mode-installation-directory)
                 :launches (ana-test-argv)
                 :command-args (mapcar #'ana-test-normalize (anaconda-mode-server-command-args))
                 :process-name (process-name anaconda-mode-process)
                 :process-status (process-status anaconda-mode-process)
                 :query-on-exit (process-query-on-exit-flag anaconda-mode-process)
                 :process-properties
                 (list :interpreter (equal (process-get anaconda-mode-process 'interpreter)
                                           python-shell-interpreter)
                       :virtualenv (process-get anaconda-mode-process 'virtualenv)
                       :remote-p (process-get anaconda-mode-process 'remote-p))
                 :announcement (with-current-buffer anaconda-mode-process-buffer
                                 (replace-regexp-in-string
                                  "port [0-9]+" "port PORT" (buffer-string)))
                 :needs-restart (anaconda-mode-need-restart)
                 :second-start (ana-test-start-server)
                 :launches-after-second-start (length (ana-test-argv))
                 :stopped (progn (anaconda-mode-stop)
                                 (list :process anaconda-mode-process
                                       :running (anaconda-mode-running-p)))))
            (when (buffer-live-p buffer) (kill-buffer buffer))
            (ana-test-teardown)))
    "##,
        expect![[
            r#"OK (:installation-directory-missing-before nil :started (:callback t :running t :bound t) :port-is-the-one-announced t :host "127.0.0.1" :localhost-address "127.0.0.1" :server-version "0.1.17" :server-directory "[ORACLE-SANDBOX]/anaconda-install/0.1.17" :installation-directory-created t :launches (("[PACKAGE]/anaconda-mode.py" "[ORACLE-SANDBOX]/anaconda-install/0.1.17" "127.0.0.1" "" "cwd [ORACLE-SANDBOX]/anaconda-install")) :command-args ("[PACKAGE]/anaconda-mode.py" "[ORACLE-SANDBOX]/anaconda-install/0.1.17" "127.0.0.1" "") :process-name "anaconda-mode" :process-status run :query-on-exit nil :process-properties (:interpreter t :virtualenv nil :remote-p nil) :announcement "anaconda_mode port PORT\n" :needs-restart nil :second-start (:callback t :running t :bound t) :launches-after-second-start 1 :stopped (:process nil :running nil))"#
        ]],
    )
}

fn moving_the_server_to_another_address_and_virtualenv_restarts_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "moving_the_server_to_another_address_and_virtualenv_restarts_it",
        r##"
        ;; A user works on a box where 127.0.0.1 is taken, sets
        ;; `anaconda-mode-localhost-address' to another loopback address, then
        ;; activates a virtualenv part way through the session.  The address
        ;; has to reach both the server's bind argument and the URL the
        ;; package posts to; the new virtualenv root has to make the package
        ;; notice its server is stale, kill it, and launch a fresh one with the
        ;; root appended to the command line.
        (let ((buffer nil))
          (unwind-protect
              (progn
                (ana-test-setup "127.0.0.2")
                (setq buffer (ana-test-visit))
                (ana-test-start-server)
                (let ((first-process anaconda-mode-process))
                  (list
                   :address (ana-test-copy anaconda-mode-localhost-address)
                   :host (ana-test-copy (anaconda-mode-host))
                   :bound-locally (butlast (append (process-contact ana-test-server :local) nil))
                   :first-jump (progn (ana-test-goto 37 12)
                                      (ana-test-invoke
                                       "M-." (lambda () (= (line-number-at-pos) 29)))
                                      (ana-test-here))
                   :activating-a-virtualenv
                   (progn (setq python-shell-virtualenv-root (ana-test-path "venv"))
                          (list :needs-restart (anaconda-mode-need-restart)
                                :command-args (mapcar #'ana-test-normalize
                                                      (anaconda-mode-server-command-args))))
                   :second-jump (progn (ana-test-goto 36 8)
                                       (ana-test-invoke
                                        "M-." (lambda () (= (line-number-at-pos) 4)))
                                       (ana-test-here))
                   :restarted (list :same-process (eq first-process anaconda-mode-process)
                                    :old-process-live (process-live-p first-process)
                                    :virtualenv (ana-test-copy
                                                 (process-get anaconda-mode-process 'virtualenv)))
                   :launches (ana-test-argv)
                   :requests (ana-test-request-methods))))
            (when (buffer-live-p buffer) (kill-buffer buffer))
            (ana-test-teardown)))
    "##,
        expect![[
            r#"OK (:address "127.0.0.2" :host "127.0.0.2" :bound-locally (127 0 0 2) :first-jump (:line 29 :column 4 :text "def total_price(widgets):") :activating-a-virtualenv (:needs-restart t :command-args ("[PACKAGE]/anaconda-mode.py" "[ORACLE-SANDBOX]/anaconda-install/0.1.17" "127.0.0.2" "[ORACLE-SANDBOX]/venv")) :second-jump (:line 4 :column 6 :text "class Widget:") :restarted (:same-process nil :old-process-live nil :virtualenv "[ORACLE-SANDBOX]/venv") :launches (("[PACKAGE]/anaconda-mode.py" "[ORACLE-SANDBOX]/anaconda-install/0.1.17" "127.0.0.2" "" "cwd [ORACLE-SANDBOX]/anaconda-install") ("[PACKAGE]/anaconda-mode.py" "[ORACLE-SANDBOX]/anaconda-install/0.1.17" "127.0.0.2" "[ORACLE-SANDBOX]/venv" "cwd [ORACLE-SANDBOX]/anaconda-install")) :requests (("infer" 37 12) ("infer" 36 8)))"#
        ]],
    )
    .fresh_process()
}

fn completing_an_attribute_posts_the_whole_buffer_and_inserts_the_candidate() -> ParityBatchCase {
    ParityBatchCase::value(
        "completing_an_attribute_posts_the_whole_buffer_and_inserts_the_candidate",
        r##"
        ;; The user types `first.dup' and presses C-M-i.  The package must POST
        ;; the entire buffer with the one-based line and zero-based column of
        ;; point and the file's path, then complete the symbol at point in
        ;; place.  A unique candidate is inserted; two candidates that share
        ;; the typed prefix are offered in *Completions*, annotated with the
        ;; type the server reported.  The exact JSON body is pinned because it
        ;; is what actually crosses the wire.
        (ana-test-with-project
         (list
          :unique
          (progn (ana-test-goto 38 15)
                 (ana-test-invoke "C-M-i"
                                  (lambda () (equal (thing-at-point 'symbol t) "duplicate")))
                 (list :here (ana-test-here)
                       :modified (buffer-modified-p)
                       :completions-offered (and (get-buffer "*Completions*") t)))
          :ambiguous
          (progn (ana-test-goto 36 15)
                 (ana-test-invoke "C-M-i" (lambda () (get-buffer "*Completions*")))
                 (list :here (ana-test-here)
                       :completions
                       (with-current-buffer "*Completions*"
                         (buffer-substring-no-properties (point-min) (point-max)))))
          :discarded-after-the-user-types-on
          (progn
            (setq ana-test-behavior 'defer)
            (ana-test-goto 36 15)
            (ana-test-invoke "C-M-i" (lambda () (= 3 (length (ana-test-server-requests)))))
            (kill-buffer "*Completions*")
            (goto-char (point-min))
            (setq ana-test-behavior 'ok)
            (list :released (ana-test-release)
                  :waited (ana-test-wait (lambda () nil) 1)
                  :point (point)
                  :completions-offered (and (get-buffer "*Completions*") t)
                  :line-36 (save-excursion (ana-test-goto 36 0) (ana-test-here))))
          :first-request-body (car (ana-test-server-bodies))
          :requests (ana-test-server-requests)
          :buffer (buffer-substring-no-properties (point-min) (point-max))))
    "##,
        expect![[
            r#"OK (:unique (:here (:line 38 :column 21 :text "print(first.duplicate)") :modified t :completions-offered nil) :ambiguous (:here (:line 36 :column 15 :text "print(first.dis)") :completions "Type M-x minibuffer-choose-completion on a completion to select it.\nType M-x minibuffer-next-completion or M-x minibuffer-previous-completion to move point between completions.\n\n2 possible completions:\ndiscounted <function>\ndisplay_name <function>") :discarded-after-the-user-types-on (:released 1 :waited nil :point 1 :completions-offered nil :line-36 (:line 36 :column 0 :text "print(first.dis)")) :first-request-body "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"complete\",\"params\":{\"source\":\"\\\"\\\"\\\"Warehouse inventory helpers for the anaconda-mode parity fixture.\\\"\\\"\\\"\\n\\n\\nclass Widget:\\n    \\\"\\\"\\\"A catalogue item with a name and a price.\\\"\\\"\\\"\\n\\n    def __init__(self, name, price):\\n        self.name = name\\n        self.price = price\\n\\n    def discounted(self, percent):\\n        \\\"\\\"\\\"Return the price with PERCENT taken off.\\\"\\\"\\\"\\n        return self.price * (100 - percent) / 100\\n\\n    def display_name(self):\\n        \\\"\\\"\\\"Return the name shown to a customer.\\\"\\\"\\\"\\n        return self.name.upper()\\n\\n    def duplicate(self):\\n        \\\"\\\"\\\"Return an independent copy of this widget.\\\"\\\"\\\"\\n        return Widget(self.name, self.price)\\n\\n\\ndef build_catalogue(names, price):\\n    \\\"\\\"\\\"Create one Widget per entry of NAMES, each costing PRICE.\\\"\\\"\\\"\\n    return [Widget(name, price) for name in names]\\n\\n\\ndef total_price(widgets):\\n    \\\"\\\"\\\"Sum the price of every widget in WIDGETS.\\\"\\\"\\\"\\n    return sum(widget.price for widget in widgets)\\n\\n\\ncatalogue = build_catalogue([\\\"bolt\\\", \\\"nut\\\", \\\"washer\\\"], 12)\\nfirst = catalogue[0]\\nprint(first.dis)\\nprint(total_price(catalogue))\\nprint(first.dup)\\nsample = Widget(\\\"bolt\\\", 12)\\n\",\"line\":38,\"column\":15,\"path\":\"[ORACLE-SANDBOX]/project/inventory.py\"}}" :requests ((:body nil :request-line "POST / HTTP/1.1" :jsonrpc "2.0" :id 1 :method "complete" :line 38 :column 15 :path "[ORACLE-SANDBOX]/project/inventory.py" :source (:length 1073)) (:body nil :request-line "POST / HTTP/1.1" :jsonrpc "2.0" :id 1 :method "complete" :line 36 :column 15 :path "[ORACLE-SANDBOX]/project/inventory.py" :source (:length 1079)) (:body nil :request-line "POST / HTTP/1.1" :jsonrpc "2.0" :id 1 :method "complete" :line 36 :column 15 :path "[ORACLE-SANDBOX]/project/inventory.py" :source (:length 1079))) :buffer "\"\"\"Warehouse inventory helpers for the anaconda-mode parity fixture.\"\"\"\n\n\nclass Widget:\n    \"\"\"A catalogue item with a name and a price.\"\"\"\n\n    def __init__(self, name, price):\n        self.name = name\n        self.price = price\n\n    def discounted(self, percent):\n        \"\"\"Return the price with PERCENT taken off.\"\"\"\n        return self.price * (100 - percent) / 100\n\n    def display_name(self):\n        \"\"\"Return the name shown to a customer.\"\"\"\n        return self.name.upper()\n\n    def duplicate(self):\n        \"\"\"Return an independent copy of this widget.\"\"\"\n        return Widget(self.name, self.price)\n\n\ndef build_catalogue(names, price):\n    \"\"\"Create one Widget per entry of NAMES, each costing PRICE.\"\"\"\n    return [Widget(name, price) for name in names]\n\n\ndef total_price(widgets):\n    \"\"\"Sum the price of every widget in WIDGETS.\"\"\"\n    return sum(widget.price for widget in widgets)\n\n\ncatalogue = build_catalogue([\"bolt\", \"nut\", \"washer\"], 12)\nfirst = catalogue[0]\nprint(first.dis)\nprint(total_price(catalogue))\nprint(first.duplicate)\nsample = Widget(\"bolt\", 12)\n")"#
        ]],
    )
}

fn navigating_to_a_definition_an_assignment_and_every_reference() -> ParityBatchCase {
    ParityBatchCase::value(
        "navigating_to_a_definition_an_assignment_and_every_reference",
        r##"
        ;; With point on `first', M-. infers its type and jumps to the class,
        ;; M-, comes back to exactly where the jump started, and M-= goes to
        ;; the assignment instead.  M-r asks for references and renders all
        ;; three call sites in the xref buffer.  Every jump has to land on the
        ;; precise line and column the server reported.
        (ana-test-with-project
         (list
          :start (ana-test-goto 36 8)
          :symbol (thing-at-point 'symbol t)
          :definition (progn (ana-test-invoke "M-." (lambda () (= (line-number-at-pos) 4)))
                             (list :here (ana-test-here)
                                   :buffer (ana-test-copy (buffer-name))
                                   :window-buffer (ana-test-copy
                                                   (buffer-name (window-buffer (selected-window))))))
          :back (progn (ana-test-invoke "M-," (lambda () (= (line-number-at-pos) 36)))
                       (ana-test-here))
          :assignment (progn (ana-test-goto 36 8)
                             (ana-test-invoke "M-=" (lambda () (= (line-number-at-pos) 35)))
                             (ana-test-here))
          :references (progn (ana-test-goto 36 8)
                             (ana-test-invoke "M-r" (lambda () (get-buffer "*xref*")))
                             (with-current-buffer "*xref*"
                               (list :mode major-mode
                                     :window (and (get-buffer-window "*xref*") t)
                                     :text (ana-test-unsandbox
                                            (buffer-substring-no-properties
                                             (point-min) (point-max))))))
          :requests (ana-test-request-methods)))
    "##,
        expect![[
            r#"OK (:start (36 8) :symbol "first" :definition (:here (:line 4 :column 6 :text "class Widget:") :buffer "inventory.py" :window-buffer "inventory.py") :back (:line 36 :column 8 :text "print(first.dis)") :assignment (:line 35 :column 0 :text "first = catalogue[0]") :references (:mode xref--xref-buffer-mode :window t :text "[SANDBOX]/project/inventory.py\n35:first = catalogue[0]\n36:print(first.dis)\n38:print(first.dup)\n") :requests (("infer" 36 8) ("goto" 36 8) ("get_references" 36 8)))"#
        ]],
    )
}

fn the_synchronous_xref_backend_answers_or_reports_that_it_timed_out() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_synchronous_xref_backend_answers_or_reports_that_it_timed_out",
        r##"
        ;; anaconda-mode also registers an xref backend, and that path is
        ;; synchronous: `xref-find-definitions' blocks, pumping the process
        ;; loop, until the server answers or `anaconda-mode-sync-request-timeout'
        ;; passes.  Both outcomes are user visible - a jump, or an error naming
        ;; the RPC that never came back - and neither may hang.
        (ana-test-with-project
         (list
          :backend (list :hook xref-backend-functions
                         :selected (run-hook-with-args-until-success 'xref-backend-functions)
                         :timeout anaconda-mode-sync-request-timeout)
          :answered (progn (ana-test-goto 37 12)
                           (xref-find-definitions "total_price")
                           (ana-test-here))
          :timed-out (progn (setq ana-test-behavior 'defer)
                            (ana-test-goto 36 8)
                            (condition-case error
                                (progn (xref-find-definitions "first") :no-signal)
                              (error (list :signal (car error) :data (cdr error)))))
          :point-after (ana-test-here)
          :requests (ana-test-request-methods)))
    "##,
        expect![[
            r#"OK (:backend (:hook (anaconda-mode-xref-backend t) :selected anaconda :timeout 2) :answered (:line 29 :column 4 :text "def total_price(widgets):") :timed-out (:signal error :data ("infer request timed out")) :point-after (:line 36 :column 8 :text "print(first.dis)") :requests (("infer" 37 12) ("infer" 36 8)))"#
        ]],
    )
}

fn reading_documentation_renders_the_anaconda_buffer_or_says_there_is_none() -> ParityBatchCase {
    ParityBatchCase::value(
        "reading_documentation_renders_the_anaconda_buffer_or_says_there_is_none",
        r##"
        ;; M-? on `Widget' pops up *Anaconda* with the module name in bold, the
        ;; signature and docstring below it, right trimmed, in a read-only view
        ;; buffer scrolled to the top.  Asking on a closing paren, where the
        ;; server has nothing to say, must leave no buffer behind and tell the
        ;; user so in the echo area.
        (ana-test-with-project
         (list
          :documented
          (progn (ana-test-goto 39 11)
                 (ana-test-invoke "M-?" (lambda () (get-buffer "*Anaconda*")))
                 (with-current-buffer "*Anaconda*"
                   (list :mode major-mode
                         :view-mode (and (bound-and-true-p view-mode) t)
                         :read-only buffer-read-only
                         :point (point)
                         :window (and (get-buffer-window "*Anaconda*") t)
                         :text (buffer-substring-no-properties (point-min) (point-max))
                         :faces (ana-test-faces (point-min) (point-max)))))
          :selected-after (ana-test-copy (buffer-name (window-buffer (selected-window))))
          :undocumented
          (progn (select-window (get-buffer-window buffer))
                 (kill-buffer "*Anaconda*")
                 (ana-test-goto 37 29)
                 (ana-test-invoke "M-?" (lambda () (= 2 (length (ana-test-server-requests)))))
                 (ana-test-wait (lambda () nil) 0.5)
                 (list :buffer (and (get-buffer "*Anaconda*") t)
                       :messages (ana-test-messages "^No documentation available$")))
          :requests (ana-test-request-methods)))
    "##,
        expect![[
            r#"OK (:documented (:mode fundamental-mode :view-mode t :read-only t :point 1 :window t :text "inventory\nWidget(name, price)\n\nA catalogue item with a name and a price.\n\n" :faces ((bold "inventory") (nil "\nWidget(name, price)\n\nA catalogue item with a name and a price.\n\n"))) :selected-after "*Anaconda*" :undocumented (:buffer nil :messages ("No documentation available")) :requests (("show_doc" 39 11) ("show_doc" 37 29)))"#
        ]],
    )
    .fresh_process()
}

fn eldoc_highlights_the_argument_under_point_and_trims_to_one_line_on_request() -> ParityBatchCase {
    ParityBatchCase::value(
        "eldoc_highlights_the_argument_under_point_and_trims_to_one_line_on_request",
        r##"
        ;; With `anaconda-eldoc-mode' on, asking for documentation inside
        ;; `Widget("bolt", 12)' must show the signature with the name in
        ;; `font-lock-function-name-face' and the argument point is actually
        ;; sitting in - the second one - in
        ;; `eldoc-highlight-function-argument'.  Moving to the first argument
        ;; must move the highlight.  With `anaconda-mode-eldoc-as-single-line'
        ;; the long builtin signature is cut to the frame width instead of
        ;; wrapping.
        (ana-test-with-project
         (progn
           (anaconda-eldoc-mode 1)
           (list
            :registered (list :buffer-local eldoc-documentation-functions
                              :global (default-value 'eldoc-documentation-functions)
                              :eldoc-mode (and (bound-and-true-p eldoc-mode) t))
            :second-argument
            (progn (ana-test-goto 39 24)
                   (eldoc t)
                   (ana-test-wait (lambda () (get-buffer "*eldoc*")))
                   (ana-test-wait (lambda () nil) 0.3)
                   (with-current-buffer "*eldoc*"
                     (list :text (buffer-substring-no-properties (point-min) (point-max))
                           :faces (ana-test-faces (point-min) (point-max)))))
            :first-argument
            (progn (kill-buffer "*eldoc*")
                   (ana-test-goto 39 17)
                   (eldoc t)
                   (ana-test-wait (lambda () (get-buffer "*eldoc*")))
                   (ana-test-wait (lambda () nil) 0.3)
                   (with-current-buffer "*eldoc*"
                     (list :text (buffer-substring-no-properties (point-min) (point-max))
                           :faces (ana-test-faces (point-min) (point-max)))))
            :echo-area-message eldoc-last-message
            :single-line
            (let ((anaconda-mode-eldoc-as-single-line t)
                  (captured 'pending))
              (ana-test-goto 36 15)
              (anaconda-mode-eldoc-function (lambda (value &rest _) (setq captured value)))
              (ana-test-wait (lambda () (not (eq captured 'pending))))
              (list :frame-width (frame-width)
                    :length (length captured)
                    :text (substring-no-properties captured)))
            :multi-line
            (let ((captured 'pending))
              (ana-test-goto 36 15)
              (anaconda-mode-eldoc-function (lambda (value &rest _) (setq captured value)))
              (ana-test-wait (lambda () (not (eq captured 'pending))))
              (list :length (length captured)
                    :text (substring-no-properties captured)))
            :requests (ana-test-request-methods))))
    "##,
        expect![[
            r#"OK (:registered (:buffer-local (anaconda-mode-eldoc-function python-eldoc-function t) :global (eldoc-show-help-at-pt) :eldoc-mode t) :second-argument (:text "Widget(name, price)" :faces ((font-lock-function-name-face "Widget") (nil "(name, ") (eldoc-highlight-function-argument "price") (nil ")"))) :first-argument (:text "Widget(name, price)" :faces ((font-lock-function-name-face "Widget") (nil "(") (eldoc-highlight-function-argument "name") (nil ", price)"))) :echo-area-message nil :single-line (:frame-width 80 :length 80 :text "print(*values: object, sep: Optional[str]=..., end: Optional[str]=..., file: Opt") :multi-line (:length 127 :text "print(*values: object, sep: Optional[str]=..., end: Optional[str]=..., file: Optional[SupportsWrite[str]]=..., flush: bool=...)") :requests (("eldoc" 39 24) ("eldoc" 39 17) ("eldoc" 36 15) ("eldoc" 36 15)))"#
        ]],
    )
    .fresh_process()
}

fn a_malformed_body_a_server_error_and_a_closed_listener_each_reach_the_user() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_malformed_body_a_server_error_and_a_closed_listener_each_reach_the_user",
        r##"
        ;; Three ways the transport can go wrong once the server is up.  A body
        ;; that is not JSON has to be kept verbatim in *anaconda-response*,
        ;; prefixed with the url status and the point the reader stopped at, so
        ;; the user can look at it, and the echo area has to say so.  A
        ;; JSON-RPC error object has to be reported with its message, its data
        ;; and a pointer at the process buffer.  A listener that has gone away
        ;; has to surface the connection failure rather than silently doing
        ;; nothing.  Point must never move for any of them.
        (ana-test-with-project
         (list
          :malformed
          (progn (setq ana-test-behavior 'malformed)
                 (ana-test-goto 36 8)
                 (ana-test-invoke "M-." (lambda () (get-buffer anaconda-mode-response-buffer)))
                 (list :here (ana-test-here)
                       :response (with-current-buffer anaconda-mode-response-buffer
                                   (list :point (point)
                                         :text (buffer-substring-no-properties
                                                (point-min) (point-max))))))
          :server-error
          (progn (setq ana-test-behavior 'rpc-error)
                 (kill-buffer anaconda-mode-response-buffer)
                 (ana-test-goto 36 8)
                 (ana-test-invoke "M-." (lambda () (= 2 (length (ana-test-server-requests)))))
                 (ana-test-wait (lambda () nil) 0.5)
                 (list :here (ana-test-here)
                       :response-buffer (and (get-buffer anaconda-mode-response-buffer) t)))
          :listener-closed
          (progn (setq ana-test-behavior 'ok)
                 (ana-test-server-stop)
                 (when (get-buffer anaconda-mode-response-buffer)
                   (kill-buffer anaconda-mode-response-buffer))
                 (ana-test-goto 36 8)
                 (ana-test-invoke "M-." (lambda () (get-buffer anaconda-mode-response-buffer)) 10)
                 (ana-test-wait (lambda () nil) 1)
                 (list :here (ana-test-here)
                       :server-still-running (and (anaconda-mode-running-p) t)
                       :requests (length (ana-test-server-requests))
                       :response (with-current-buffer anaconda-mode-response-buffer
                                   (replace-regexp-in-string
                                    "[0-9][0-9][0-9][0-9][0-9]+" "PORT"
                                    (buffer-substring-no-properties (point-min) (point-max))))))
          :messages (ana-test-messages
                     "^\\(Cannot read anaconda-mode server response\\|Server error.*\\)$")
          :requests (ana-test-request-methods)))
    "##,
        expect![[
            r##"OK (:malformed (:here (:line 36 :column 8 :text "print(first.dis)") :response (:point 1 :text "# status: nil\n# point: 109\nHTTP/1.1 200 OK\nServer: BaseHTTP/0.6 Python/3.13.12\nDate: Mon, 28 Jul 2026 00:00:00 GMT\nContent-Length: 41\n\n<html>anaconda is not running here</html>")) :server-error (:here (:line 36 :column 8 :text "print(first.dis)") :response-buffer nil) :listener-closed (:here (:line 36 :column 8 :text "print(first.dis)") :server-still-running t :requests 2 :response "# status: (:error (error connection-failed deleted\n :host 127.0.0.1 :service PORT) :error (error connection-failed failed with code 111\n :host 127.0.0.1 :service PORT))\n# point: 1\n") :messages ("Cannot read anaconda-mode server response" "Server error: AttributeError(\"'NoneType' object has no attribute 'start_pos'\") - see *anaconda-mode* for more information.") :requests (("infer" 36 8) ("infer" 36 8)))"##
        ]],
    )
    .fresh_process()
}

fn a_python_that_cannot_start_the_server_leaves_its_traceback_and_answers_nothing()
-> ParityBatchCase {
    ParityBatchCase::value(
        "a_python_that_cannot_start_the_server_leaves_its_traceback_and_answers_nothing",
        r##"
        ;; The common first-run failure: the interpreter is there but cannot
        ;; bring the server up, because jedi could not be installed.  The
        ;; package must leave pip's output and the traceback in *anaconda-mode*
        ;; with the ANSI colours turned into faces by its own filter, never
        ;; bind a port, never post anything, and leave point where it was.  A
        ;; missing interpreter must fail the same way rather than signalling.
        ;; And with `anaconda-mode-disable-rpc' set to `always' the package
        ;; must not even launch a server.
        (let ((buffer nil))
          (unwind-protect
              (progn
                (ana-test-setup)
                (setq buffer (ana-test-visit))
                (list
                 :rpc-disabled
                 (let ((anaconda-mode-disable-rpc 'always))
                   (ana-test-goto 36 8)
                   (ana-test-invoke "M-." (lambda () nil) 1)
                   (list :here (ana-test-here)
                         :process anaconda-mode-process
                         :launches (length (ana-test-argv))
                         :requests (length (ana-test-server-requests))))
                 :without-jedi
                 (progn
                   (setq pythonic-interpreter
                         (ana-test-install-interpreter
                          "python-without-jedi" ana-test-failing-interpreter-script)
                         python-shell-interpreter pythonic-interpreter)
                   (ana-test-goto 36 8)
                   (ana-test-invoke "M-." (lambda () (not (anaconda-mode-running-p))))
                   (list :here (ana-test-here)
                         :bound (anaconda-mode-bound-p)
                         :port (anaconda-mode-port)
                         :status (process-status anaconda-mode-process)
                         :exit (process-exit-status anaconda-mode-process)
                         :requests (length (ana-test-server-requests))
                         :process-buffer
                         (with-current-buffer anaconda-mode-process-buffer
                           (list :text (buffer-substring-no-properties (point-min) (point-max))
                                 :faces (ana-test-faces (point-min) (point-max)
                                                        nil 'font-lock-face)))))
                 :missing-interpreter
                 (progn
                   (anaconda-mode-stop)
                   (setq pythonic-interpreter (ana-test-path "bin/absent-python")
                         python-shell-interpreter pythonic-interpreter)
                   (ana-test-goto 36 8)
                   (condition-case error
                       (progn
                         (ana-test-invoke "M-." (lambda () (not (anaconda-mode-running-p))) 10)
                         (list :here (ana-test-here)
                               :bound (anaconda-mode-bound-p)
                               :status (process-status anaconda-mode-process)
                               :exit (process-exit-status anaconda-mode-process)))
                     (error (list :signal (car error) :data (cdr error)))))
                 :launches (ana-test-argv)))
            (when (buffer-live-p buffer) (kill-buffer buffer))
            (ana-test-teardown)))
    "##,
        expect![[
            r#"OK (:rpc-disabled (:here (:line 36 :column 8 :text "print(first.dis)") :process nil :launches 0 :requests 0) :without-jedi (:here (:line 36 :column 8 :text "print(first.dis)") :bound nil :port nil :status exit :exit 1 :requests 0 :process-buffer (:text "Collecting jedi==0.19.2\nERROR: No matching distribution found for jedi==0.19.2\nTraceback (most recent call last):\n  File \"anaconda-mode.py\", line 113, in <module>\n    import jedi\nModuleNotFoundError: No module named 'jedi'\n" :faces ((nil "Collecting jedi==0.19.2\n") ((:foreground "red3") "ERROR: No matching distribution found for jedi==0.19.2") (nil "\nTraceback (most recent call last):\n  File \"anaconda-mode.py\", line 113, in <module>\n    import jedi\nModuleNotFoundError: No module named 'jedi'\n")))) :missing-interpreter (:here (:line 36 :column 8 :text "print(first.dis)") :bound nil :status exit :exit 127) :launches (("[PACKAGE]/anaconda-mode.py" "[ORACLE-SANDBOX]/anaconda-install/0.1.17" "127.0.0.1" "" "cwd [ORACLE-SANDBOX]/anaconda-install")))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        starting_the_server_builds_the_documented_command_line_and_binds_its_port(),
        moving_the_server_to_another_address_and_virtualenv_restarts_it(),
        completing_an_attribute_posts_the_whole_buffer_and_inserts_the_candidate(),
        navigating_to_a_definition_an_assignment_and_every_reference(),
        the_synchronous_xref_backend_answers_or_reports_that_it_timed_out(),
        reading_documentation_renders_the_anaconda_buffer_or_says_there_is_none(),
        eldoc_highlights_the_argument_under_point_and_trims_to_one_line_on_request(),
        a_malformed_body_a_server_error_and_a_closed_listener_each_reach_the_user(),
        a_python_that_cannot_start_the_server_leaves_its_traceback_and_answers_nothing(),
    ]
}
