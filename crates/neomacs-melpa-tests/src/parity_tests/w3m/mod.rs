//! Practical parity workflows for emacs-w3m rank 382.
//!
//! MELPA 20260811.923 is pinned to upstream commit
//! `bb01ba0329ee5b02c2ff260d8881bbc6f389d80a`.  The halfdump frames below
//! were recorded from w3m 0.5.5 at executable digest
//! `dfc1477374da8ba0ffd5f3ddad59d2d8fe04907f6af1f6114826d36022826237`.
//! The replay accepts only those exact HTML bytes and argument vector.
//!
//! The recovery case deliberately preserves one stable Neo core red.  W3M
//! passes DELETE=t to `call-process-region`; GNU deletes the region before it
//! resolves a missing executable, while Neo resolves the executable first and
//! therefore retains the input.  Normal and fresh-process runs reproduce this
//! exact failure-state difference; the subsequent public recovery is equal.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, W3M_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'w3m)
(require 'w3m-bookmark)

;; Establish package keymaps, hooks, and mode symbols before shared baselines.
(with-temp-buffer (w3m-mode))

(defvar w3m382-test-owned-roots nil)
(defvar w3m382-test-owned-boundaries nil)

(defun w3m382-test-write-renderer (root)
  (let ((tool (expand-file-name "w3m-fixture" root)))
    (write-region
     (concat
      "#!/bin/sh\n"
      "args=\n"
      "for arg in \"$@\"; do args=\"${args}<${arg}>\"; done\n"
      "$W3M382_CAT >\"$W3M382_INPUT\"\n"
      "printf 'argv%s\\n' \"$args\" >>\"$W3M382_LOG\"\n"
      "if [ \"$args\" != \"$W3M382_EXPECTED_ARGS\" ]; then\n"
      "  printf 'UNRECORDED\\n' >>\"$W3M382_LOG\"\n"
      "  exit 86\n"
      "fi\n"
      "if \"$W3M382_CMP\" -s \"$W3M382_INPUT\" \"$W3M382_EXPECTED_FILE\"; then\n"
      "  if [ -n \"${W3M382_SECOND_FILE+x}\" ]; then "
      "printf 'page1\\n' >>\"$W3M382_LOG\"; fi\n"
      "  printf '%s' \"$W3M382_OUTPUT\"\n"
      "elif [ -n \"${W3M382_SECOND_FILE+x}\" ] && "
      "\"$W3M382_CMP\" -s \"$W3M382_INPUT\" \"$W3M382_SECOND_FILE\"; then\n"
      "  printf 'page2\\n' >>\"$W3M382_LOG\"\n"
      "  printf '%s' \"$W3M382_SECOND_OUTPUT\"\n"
      "else\n"
      "  printf 'UNRECORDED\\n' >>\"$W3M382_LOG\"\n"
      "  exit 86\n"
      "fi\n")
     nil tool nil 'silent)
    (set-file-modes tool #o700)
    tool))

(defconst w3m382-test-rich-html
  "<html><head><meta charset=\"UTF-8\"><title>Café Index</title></head><body><h1>Hello 界</h1><p>Alpha <a href=\"https://example.test/next\">Next page</a></p><form action=\"https://example.test/search\"><input name=\"q\" value=\"café\"><input type=\"submit\" value=\"Go\"></form></body></html>")

(defconst w3m382-test-rich-halfdump
  "<title_alt title=\"Café Index\"><b>Hello 界</b>\n\nAlpha <a hseq=\"1\" href=\"https://example.test/next\">Next page</a>\n\n<form_int method=\"get\" action=\"https://example.test/search\" fid=\"0\"><pre_int>[<input_alt hseq=\"2\" fid=\"0\" type=\"text\" name=\"q\" value=\"café\" maxlength=\"20\"><u>café                </u></input_alt>]</pre_int><pre_int><input_alt hseq=\"3\" fid=\"0\" type=\"submit\" name=\"\" value=\"Go\" maxlength=\"20\">[Go]</input_alt></pre_int>\n<internal>\n<title_alt title=\"Café Index\">\n</internal>\n")

(defconst w3m382-test-rich-args
  "<-halfdump><-o><ext_halfdump=1><-o><strict_iso2022=0><-o><fix_width_conv=1><-o><use_jisx0201=0><-o><ucs_conv=1><-I><UTF-8><-O><UTF-8><-T><text/html><-t><8><-cols><79>")

(defconst w3m382-test-second-html
  "<html><head><meta charset=\"UTF-8\"><title>Second 世界</title></head><body><h1>Second 世界</h1><p>Back-ready.</p></body></html>")

(defconst w3m382-test-second-halfdump
  "<title_alt title=\"Second 世界\"><b>Second 世界</b>\n\nBack-ready.\n\n<internal>\n<title_alt title=\"Second 世界\">\n</internal>\n")

(defun w3m382-test-call-with-renderer
    (second-input second-output expected-boundary callback)
  (let* ((root (make-temp-file "w3m382-render-" t))
         (log (expand-file-name "boundary.log" root))
         (input (expand-file-name "actual-input.html" root))
         (expected-file (expand-file-name "expected-input.html" root))
         (second-file (and second-input
                           (expand-file-name "second-input.html" root)))
         (tool (w3m382-test-write-renderer root))
         (cat (executable-find "cat"))
         (cmp (executable-find "cmp"))
         (w3m-command tool)
         (w3m-halfdump-command tool)
         (w3m-type 'w3m)
         (w3m-version "w3m/0.5.5")
         (w3m-compile-options nil)
         (w3m-display-ins-del nil)
         (w3m-force-redisplay nil)
         (w3m-use-form t)
         (w3m-use-tab nil)
         (w3m-use-tab-line nil)
         (w3m-use-cookies nil)
         (w3m-use-toolbar nil)
         (w3m-use-favicon nil)
         (w3m-arrived-db nil)
         (w3m-input-url-history '("w3m382-render-baseline"))
         (w3m-arrived-file (expand-file-name "arrived" root))
         (w3m-bookmark-file (expand-file-name "bookmark.html" root))
         (w3m-profile-directory (file-name-as-directory root))
         (w3m-default-directory (file-name-as-directory root))
         (process-environment
          (append
           (list "LC_ALL=C.UTF-8"
                 "PATH=/nonexistent"
                 (concat "W3M382_CAT=" cat)
                 (concat "W3M382_CMP=" cmp)
                 (concat "W3M382_LOG=" log)
                 (concat "W3M382_INPUT=" input)
                 (concat "W3M382_EXPECTED_FILE=" expected-file)
                 (concat "W3M382_EXPECTED_ARGS=" w3m382-test-rich-args)
                 (concat "W3M382_OUTPUT=" w3m382-test-rich-halfdump))
           (when second-input
             (list (concat "W3M382_SECOND_FILE=" second-file)
                   (concat "W3M382_SECOND_OUTPUT=" second-output))))))
    (dolist (boundary (list cat cmp))
      (unless (and boundary (file-name-absolute-p boundary)
                   (file-regular-p boundary) (file-executable-p boundary))
        (error "w3m fixture executable is unavailable: %S" boundary)))
    (let ((coding-system-for-write 'utf-8-unix))
      (write-region w3m382-test-rich-html nil expected-file nil 'silent)
      (when second-file
        (write-region second-input nil second-file nil 'silent)))
    (unless (and (file-regular-p expected-file)
                 (not (file-symlink-p expected-file))
                 (or (null second-file)
                     (and (file-regular-p second-file)
                          (not (file-symlink-p second-file)))))
      (error "w3m fixture input materialization is unsafe"))
    (push root w3m382-test-owned-roots)
    (push (list log expected-boundary)
          w3m382-test-owned-boundaries)
    (funcall callback root log)))

(defun w3m382-test-render-rich-page ()
  (w3m382-test-call-with-renderer
   nil nil (concat "argv" w3m382-test-rich-args "\n")
   (lambda (root log)
     (let ((buffer (generate-new-buffer " *w3m382-render*")))
       (switch-to-buffer buffer)
       (insert w3m382-test-rich-html)
       (w3m-region (point-min) (point-max)
                   "https://example.test/index" 'utf-8)
       (list buffer log root)))))

(defun w3m382-test-anchor-state ()
  (let* ((position (point))
         (start (or (previous-single-property-change
                     (1+ position) 'w3m-anchor-sequence nil (point-min))
                    position))
         (end (or (next-single-property-change
                   position 'w3m-anchor-sequence nil (point-max))
                  position)))
    (list :point position
          :run (and (< start end)
                    (list start end
                          (buffer-substring-no-properties start end)))
          :href (get-text-property position 'w3m-href-anchor)
          :sequence (get-text-property position 'w3m-anchor-sequence)
          :form-field (get-text-property position 'w3m-form-field-id)
          :action (and (get-text-property position 'w3m-action) t))))

(defun w3m382-test-history-state (first-url second-url)
  (list :page (cond ((equal w3m-current-url first-url) 'first)
                    ((equal w3m-current-url second-url) 'second)
                    (t 'unexpected))
        :title w3m-current-title
        :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :previous (and (w3m-history-previous-link-available-p) t)
        :next (and (w3m-history-next-link-available-p) t)))

(defun w3m382-test-render-two-page-history ()
  (let* ((first-url (concat "data:text/html;base64,"
                            (base64-encode-string
                             (encode-coding-string
                              w3m382-test-rich-html 'utf-8-unix)
                             t)))
         (second-url (concat "data:text/html;base64,"
                             (base64-encode-string
                              (encode-coding-string
                               w3m382-test-second-html 'utf-8-unix)
                              t))))
    (w3m382-test-call-with-renderer
     w3m382-test-second-html
     w3m382-test-second-halfdump
     (concat "argv" w3m382-test-rich-args "\npage1\n"
             "argv" w3m382-test-rich-args "\npage2\n"
             "argv" w3m382-test-rich-args "\npage1\n"
             "argv" w3m382-test-rich-args "\npage2\n")
     (lambda (_root log)
       (let ((buffer (generate-new-buffer " *w3m382-history*"))
             first second back forward)
         (switch-to-buffer buffer)
         (w3m-mode)
         (w3m-goto-url first-url)
         (setq first (w3m382-test-history-state first-url second-url))
         (w3m-goto-url second-url)
         (setq second (w3m382-test-history-state first-url second-url))
         (call-interactively #'w3m-view-previous-page)
         (setq back (w3m382-test-history-state first-url second-url))
         (call-interactively #'w3m-view-next-page)
         (setq forward (w3m382-test-history-state first-url second-url))
         (let ((boundary (with-temp-buffer
                           (insert-file-contents-literally log)
                           (buffer-string))))
           (list :first first :second second :back back :forward forward
                 :boundary-sha256 (secure-hash 'sha256 boundary))))))))

(defmacro w3m382-test-call-with-inputs (command inputs)
  (let ((input-list (make-symbol "inputs"))
        (ledger (make-symbol "ledger"))
        (result (make-symbol "result")))
    `(let* ((,input-list ,inputs)
            (executing-kbd-macro t)
            (unread-command-events
             (apply #'append
                    (mapcar
                     (lambda (input-spec)
                       (append (when (car input-spec)
                                 (listify-key-sequence (kbd "C-a C-k")))
                               (string-to-list (cdr input-spec))
                               (listify-key-sequence (kbd "RET"))))
                     ,input-list)))
            ,ledger ,result)
       (let ((minibuffer-setup-hook
              (cons (lambda ()
                      (push (list
                             :prompt (minibuffer-prompt)
                             :initial (minibuffer-contents-no-properties))
                            ,ledger))
                    minibuffer-setup-hook)))
         (setq ,result (funcall ,command)))
       (unless (and (= (length ,ledger) (length ,input-list))
                    (null unread-command-events))
         (error "w3m input command mismatch: prompts=%S inputs=%S events=%S"
                ,ledger ,input-list unread-command-events))
       (list :result ,result :minibuffers (nreverse ,ledger)))))

(defun w3m382-test-property-runs ()
  (let ((position (point-min)) runs)
    (while (< position (point-max))
      (let* ((next (or (next-property-change position nil (point-max))
                       (point-max)))
             (href (get-text-property position 'w3m-href-anchor))
             (name (get-text-property position 'w3m-name-anchor))
             (sequence (get-text-property position 'w3m-anchor-sequence))
             (face (get-text-property position 'face))
             (action (get-text-property position 'w3m-action)))
        (when (or href name sequence face action)
          (push (list position next
                      :text (buffer-substring-no-properties position next)
                      :href href :name name :sequence sequence
                      :face face :action (and action t))
                runs))
        (setq position next)))
    (nreverse runs)))

(defun w3m382-test-buffer-state ()
  (list :mode major-mode
        :title w3m-current-title
        :url w3m-current-url
        :base w3m-current-base-url
        :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :read-only buffer-read-only
        :properties (w3m382-test-property-runs)))

(defun w3m382-test-run (body)
  (let ((buffers-before (buffer-list))
        (frames-before (frame-list))
        (processes-before (process-list))
        (timers-before (append timer-list timer-idle-list))
        (buffer-before (current-buffer))
        (windows-before (current-window-configuration))
        (w3m382-test-owned-roots nil)
        (w3m382-test-owned-boundaries nil)
        result body-error cleanup-errors)
    (unwind-protect
        (condition-case error
            (setq result (funcall body))
          (error (setq body-error error)))
      (condition-case error
          (progn
            (when (buffer-live-p buffer-before) (set-buffer buffer-before))
            (set-window-configuration windows-before))
        (error (push (list :restore-windows error) cleanup-errors)))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case error
              (delete-frame frame t)
            (error (push (list :delete-frame error) cleanup-errors)))))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case error
              (progn
                (set-process-query-on-exit-flag process nil)
                (delete-process process))
            (error (push (list :delete-process (process-name process) error)
                         cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case error
              (progn
                (with-current-buffer buffer (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error (push (list :kill-buffer (buffer-name buffer) error)
                         cleanup-errors)))))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (condition-case error
              (cancel-timer timer)
            (error (push (list :cancel-timer error) cleanup-errors)))))
      (dolist (boundary w3m382-test-owned-boundaries)
        (condition-case error
            (let ((file (nth 0 boundary))
                  (expected (nth 1 boundary)))
              (unless (and (file-regular-p file)
                           (not (file-symlink-p file)))
                (error "missing or unsafe w3m boundary log: %S" file))
              (with-temp-buffer
                (insert-file-contents-literally file)
                (unless (equal (buffer-string) expected)
                  (error "unexpected w3m boundary log: %S"
                         (buffer-string)))))
          (error (push (list :boundary error) cleanup-errors))))
      (dolist (root w3m382-test-owned-roots)
        (condition-case error
            (when (file-exists-p root) (delete-directory root t))
          (error (push (list :delete-root root error) cleanup-errors))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (push (list :remaining-buffer (buffer-name buffer)) cleanup-errors)))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (push (list :remaining-frame (frame-parameter frame 'name))
                cleanup-errors)))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (push (list :remaining-process (process-name process))
                cleanup-errors)))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (push (list :remaining-timer t) cleanup-errors)))
      (dolist (root w3m382-test-owned-roots)
        (when (file-exists-p root)
          (push (list :remaining-root root) cleanup-errors))))
    (cond
     ((and body-error cleanup-errors)
      (error "w3m body failed %S; cleanup failed %S"
             body-error (nreverse cleanup-errors)))
     (body-error (signal (car body-error) (cdr body-error)))
     (cleanup-errors (error "w3m cleanup failed: %S"
                            (nreverse cleanup-errors)))
     (t result))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(W3M_MELPA_PIN, "w3m.el")
        .expect("prepare exact shallow w3m source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_region_renders_unicode_links_and_forms_through_w3m() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_region_renders_unicode_links_and_forms_through_w3m",
        r####"(w3m382-test-run
 (lambda ()
   (let* ((rendered (w3m382-test-render-rich-page))
          (buffer (nth 0 rendered))
          (log (nth 1 rendered))
          state boundary)
     (with-current-buffer buffer
       (setq state (w3m382-test-buffer-state)))
     (setq boundary
           (with-temp-buffer
             (insert-file-contents log)
             (buffer-string)))
     (list :page state :boundary boundary))))"####,
        expect![[
            r#"OK (:page (:mode fundamental-mode :title "Café Index" :url "https://example.test/index" :base "https://example.test/index" :text "Hello 界\n\nAlpha Next page\n\n[café                ][Go]\n" :point 54 :read-only nil :properties ((1 8 :text "Hello 界" :href nil :name nil :sequence nil :face (w3m-bold) :action nil) (16 25 :text "Next page" :href "https://example.test/next" :name nil :sequence 1 :face (w3m-anchor) :action nil) (28 48 :text "café                " :href nil :name nil :sequence 2 :face (w3m-form w3m-underline) :action t) (49 53 :text "[Go]" :href nil :name nil :sequence 3 :face nil :action t))) :boundary "argv<-halfdump><-o><ext_halfdump=1><-o><strict_iso2022=0><-o><fix_width_conv=1><-o><use_jisx0201=0><-o><ucs_conv=1><-I><UTF-8><-O><UTF-8><-T><text/html><-t><8><-cols><79>\n")"#
        ]],
    )
}

fn public_anchor_navigation_and_form_edit_preserve_semantic_properties() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_anchor_navigation_and_form_edit_preserve_semantic_properties",
        r####"(w3m382-test-run
 (lambda ()
   (let* ((rendered (w3m382-test-render-rich-page))
          (buffer (nth 0 rendered))
          prompt initial link form-before form-after submit back page
          next-link next-form next-submit previous-form)
     (switch-to-buffer buffer)
     (goto-char (point-min))
     (setq next-link (call-interactively #'w3m-next-anchor)
           link (w3m382-test-anchor-state)
           next-form (call-interactively #'w3m-next-anchor)
           form-before (w3m382-test-anchor-state))
     (let ((executing-kbd-macro t)
           (unread-command-events
            (append (listify-key-sequence (kbd "C-a C-k"))
                    (string-to-list "café 界")
                    (listify-key-sequence (kbd "RET")))))
       (minibuffer-with-setup-hook
           (lambda ()
             (setq prompt (minibuffer-prompt)
                   initial (minibuffer-contents-no-properties)))
         (call-interactively #'w3m-view-this-url))
       (unless (null unread-command-events)
         (error "w3m form edit left unread events: %S"
                unread-command-events)))
     (setq form-after (w3m382-test-anchor-state)
           next-submit (call-interactively #'w3m-next-anchor)
           submit (w3m382-test-anchor-state)
           previous-form (call-interactively #'w3m-previous-anchor)
           back (w3m382-test-anchor-state)
           page (w3m382-test-buffer-state))
     (list :moves (list next-link next-form next-submit previous-form)
           :link link
           :form-before form-before
           :minibuffer (list :prompt prompt :initial initial)
           :form-after form-after
           :submit submit
           :back back
           :page page))))"####,
        expect![[
            r#"OK (:moves (t t t t) :link (:point 16 :run (16 25 "Next page") :href "https://example.test/next" :sequence 1 :form-field nil :action nil) :form-before (:point 28 :run (28 48 "café                ") :href nil :sequence 2 :form-field "fid=0/type=text/name=q/id=1" :action t) :minibuffer (:prompt "TEXT: " :initial "café") :form-after (:point 28 :run (28 47 "café 界             ") :href nil :sequence 2 :form-field "fid=0/type=text/name=q/id=1" :action t) :submit (:point 48 :run (48 52 "[Go]") :href nil :sequence 3 :form-field "fid=0/type=submit/name=/id=2" :action t) :back (:point 28 :run (28 47 "café 界             ") :href nil :sequence 2 :form-field "fid=0/type=text/name=q/id=1" :action t) :page (:mode fundamental-mode :title "Café Index" :url "https://example.test/index" :base "https://example.test/index" :text "Hello 界\n\nAlpha Next page\n\n[café 界             ][Go]\n" :point 28 :read-only nil :properties ((1 8 :text "Hello 界" :href nil :name nil :sequence nil :face (w3m-bold) :action nil) (16 25 :text "Next page" :href "https://example.test/next" :name nil :sequence 1 :face (w3m-anchor) :action nil) (28 47 :text "café 界             " :href nil :name nil :sequence 2 :face (w3m-form w3m-underline) :action t) (48 52 :text "[Go]" :href nil :name nil :sequence 3 :face nil :action t))))"#
        ]],
    )
}

fn public_bookmark_commands_persist_page_and_link_then_open_owned_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_bookmark_commands_persist_page_and_link_then_open_owned_file",
        r####"(w3m382-test-run
 (lambda ()
   (let* ((rendered (w3m382-test-render-rich-page))
          (page-buffer (nth 0 rendered))
          (root (nth 2 rendered))
          (bookmark (expand-file-name "owned-bookmarks.html" root))
          (w3m-bookmark-file bookmark)
          (w3m-bookmark-file-coding-system 'utf-8-unix)
          (w3m-bookmark-default-section nil)
          (w3m-bookmark-section-history nil)
          (w3m-bookmark-title-history nil)
          (w3m-input-url-history nil)
          (w3m-edit-function #'find-file)
          (make-backup-files nil)
          current link file-before-edit edit-state)
     (switch-to-buffer page-buffer)
     (setq current
           (w3m382-test-call-with-inputs
            #'w3m-bookmark-add-current-url
            '((nil . "Research") (t . "Café Index 世界"))))
     (goto-char (point-min))
     (call-interactively #'w3m-next-anchor)
     (setq link
           (w3m382-test-call-with-inputs
            #'w3m-bookmark-add-this-url
            '((nil . "Research") (t . "Next page"))))
     (setq file-before-edit
           (with-temp-buffer
             (insert-file-contents-literally bookmark)
             (buffer-string)))
     (call-interactively #'w3m-bookmark-edit)
     (setq edit-state
           (list :selected (eq (current-buffer)
                               (window-buffer (selected-window)))
                 :file (file-relative-name buffer-file-name root)
                 :mode major-mode
                 :coding buffer-file-coding-system
                 :modified (buffer-modified-p)
                 :text (buffer-substring-no-properties
                        (point-min) (point-max))))
     (list :current current
           :link link
           :section-history w3m-bookmark-section-history
           :title-history w3m-bookmark-title-history
           :url-history w3m-input-url-history
           :file-before-edit file-before-edit
           :edit edit-state))))"####,
        expect![[
            r#"OK (:current (:result "Added" :minibuffers ((:prompt "Section: " :initial "") (:prompt "Title: " :initial "Café Index"))) :link (:result "Added" :minibuffers ((:prompt "Section: " :initial "") (:prompt "Title: " :initial "Next page"))) :section-history ("Research") :title-history ("Next page" "Café Index 世界") :url-history ("Next page" "https://example.test/next" "Café Index 世界" "https://example.test/index") :file-before-edit "<html><head><title>Bookmarks</title></head>\n<body>\n<h1>Bookmarks</h1>\n<h2>Research</h2>\n<ul>\n<li><a href=\"https://example.test/index\">Caf\303\251 Index \344\270\226\347\225\214</a>\n<li><a href=\"https://example.test/next\">Next page</a>\n<!--End of section (do not delete this comment)-->\n</ul>\n</body>\n</html>\n" :edit (:selected t :file "owned-bookmarks.html" :mode mhtml-mode :coding utf-8-unix :modified nil :text "<html><head><title>Bookmarks</title></head>\n<body>\n<h1>Bookmarks</h1>\n<h2>Research</h2>\n<ul>\n<li><a href=\"https://example.test/index\">Café Index 世界</a>\n<li><a href=\"https://example.test/next\">Next page</a>\n<!--End of section (do not delete this comment)-->\n</ul>\n</body>\n</html>\n"))"#
        ]],
    )
}

fn public_history_commands_move_back_and_forward_between_rendered_pages() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_history_commands_move_back_and_forward_between_rendered_pages",
        r####"(w3m382-test-run
 (lambda ()
   (w3m382-test-render-two-page-history)))"####,
        expect![[
            r#"OK (:first (:page first :title "Café Index" :text "Hello 界\n\nAlpha Next page\n\n[café                ][Go]\n\n" :point 1 :previous nil :next nil) :second (:page second :title "Second 世界" :text "Second 世界\n\nBack-ready.\n\n" :point 1 :previous t :next nil) :back (:page first :title "Café Index" :text "Hello 界\n\nAlpha Next page\n\n[café                ][Go]\n\n" :point 1 :previous nil :next t) :forward (:page second :title "Second 世界" :text "Second 世界\n\nBack-ready.\n\n" :point 1 :previous t :next nil) :boundary-sha256 "21db07b453df65d5c4f8be48b5883d6397d81a1c98ab5faa76f0232c4edf480c")"#
        ]],
    )
}

fn public_region_failure_is_reported_and_same_buffer_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_region_failure_is_reported_and_same_buffer_recovers",
        r####"(w3m382-test-run
 (lambda ()
   (w3m382-test-call-with-renderer
    nil nil (concat "argv" w3m382-test-rich-args "\n")
    (lambda (_root log)
      (let ((buffer (generate-new-buffer " *w3m382-recovery*"))
            failure preserved recovered boundary)
        (switch-to-buffer buffer)
        (insert w3m382-test-rich-html)
        (let ((w3m-command "/nonexistent/w3m382-missing")
              (w3m-halfdump-command "/nonexistent/w3m382-missing"))
          (condition-case error
              (progn
                (w3m-region (point-min) (point-max)
                            "https://example.test/failure" 'utf-8)
                (setq failure 'unexpected-success))
            (error
             (setq failure
                   (list :error error
                         :message (error-message-string error))))))
        (setq preserved
              (list :text (buffer-substring-no-properties
                           (point-min) (point-max))
                    :mode major-mode
                    :url w3m-current-url
                    :title w3m-current-title))
        (unless (equal (buffer-substring-no-properties
                        (point-min) (point-max))
                       w3m382-test-rich-html)
          (let ((inhibit-read-only t))
            (erase-buffer)
            (insert w3m382-test-rich-html)))
        (w3m-region (point-min) (point-max)
                    "https://example.test/recovered" 'utf-8)
        (setq recovered (w3m382-test-buffer-state)
              boundary (with-temp-buffer
                         (insert-file-contents-literally log)
                         (buffer-string)))
        (list :failure failure
              :preserved preserved
              :same-buffer (eq buffer (current-buffer))
              :recovered recovered
              :boundary-sha256 (secure-hash 'sha256 boundary)))))))"####,
        expect![[
            r#"OK (:failure (:error (file-missing "Searching for program" "No such file or directory" "/nonexistent/w3m382-missing") :message "Searching for program: No such file or directory, /nonexistent/w3m382-missing") :preserved (:text "" :mode fundamental-mode :url "https://example.test/failure" :title nil) :same-buffer t :recovered (:mode fundamental-mode :title "Café Index" :url "https://example.test/recovered" :base "https://example.test/recovered" :text "Hello 界\n\nAlpha Next page\n\n[café                ][Go]\n" :point 54 :read-only nil :properties ((1 8 :text "Hello 界" :href nil :name nil :sequence nil :face (w3m-bold) :action nil) (16 25 :text "Next page" :href "https://example.test/next" :name nil :sequence 1 :face (w3m-anchor) :action nil) (28 48 :text "café                " :href nil :name nil :sequence 2 :face (w3m-form w3m-underline) :action t) (49 53 :text "[Go]" :href nil :name nil :sequence 3 :face nil :action t))) :boundary-sha256 "f27677be60093fcbd1affe23603acd4c61721ca068c3c08607cd10db67df7342")"#
        ]],
    )
}

#[test]
fn w3m_package_batch() {
    let cases = vec![
        public_region_renders_unicode_links_and_forms_through_w3m(),
        public_anchor_navigation_and_form_edit_preserve_semantic_properties(),
        public_bookmark_commands_persist_page_and_link_then_open_owned_file(),
        public_history_commands_move_back_and_forward_between_rendered_pages(),
        public_region_failure_is_reported_and_same_buffer_recovers(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed w3m parity test");
    assert_oracle_batch_cases(oracle(), test_name, "w3m_parity", &cases);
}
