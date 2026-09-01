//! Practical parity for the terminal historical gist.el package.
//!
//! The corpus uses MELPA's exact one-file source at
//! `b2712a61d04af98a05cc2556d85479803b6626be`. Network effects are replaced
//! only at the declared gh.el dependency boundary with completed response
//! objects; gist.el's public commands, callbacks, modes, caches, prompts,
//! windows, Dired integration, and edits remain real.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GIST_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'gh-url)
(require 'gh-auth)
(require 'gh-profile)
(require 'gist)

(defconst gist385-test-installed-sha256
  "93cd6acb755ebca46e112918029cf67b30539db97f5c3e9766cadcb7b6d8a5a2")
(defvar gist385-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
(defvar gist385-test-owned-roots nil)

(let ((source (symbol-file 'gist-list 'defun)))
  (unless (and source
               (equal (with-temp-buffer
                        (set-buffer-multibyte nil)
                        (insert-file-contents-literally source)
                        (secure-hash 'sha256 (current-buffer)))
                      gist385-test-installed-sha256)
               (equal (file-name-nondirectory source) "gist.el"))
    (error "Unexpected installed gist source: %S" source)))

(dolist (command '(gist-list gist-region gist-buffer gist-fetch gist-star
                   gist-unstar gist-fork dired-do-gist))
  (unless (commandp command)
    (error "Missing public gist command: %S" command)))

(defun gist385-test-response (data)
  (make-instance 'gh-url-response :data-received t :data data))

(defun gist385-test-file (name content)
  (make-instance 'gh-gist-gist-file
                 :filename name :size (length content)
                 :url (concat "https://raw.example/" name)
                 :content content))

(defun gist385-test-gist (id public description files)
  (make-instance 'gh-gist-gist
                 :id id :public public :description description
                 :date "2026-08-12T14:30:00Z"
                 :html-url (format "https://gist.example/%s" id)
                 :files files :comments 0 :history nil :forks nil))

(defun gist385-test-gist-state (gist)
  (list :id (oref gist :id)
        :public (oref gist :public)
        :description (copy-sequence (or (oref gist :description) ""))
        :html-url (copy-sequence (oref gist :html-url))
        :files
        (mapcar (lambda (file)
                  (list :filename (oref file :filename)
                        :content (copy-sequence (or (oref file :content) ""))))
                (oref gist :files))))

(defun gist385-test-stub-state (stub)
  (list :public (oref stub :public)
        :description (copy-sequence (oref stub :description))
        :files
        (mapcar (lambda (file)
                  (list :filename (oref file :filename)
                        :content (copy-sequence (or (oref file :content) ""))))
                (oref stub :files))))

(defun gist385-test-edit-state (gist)
  (let ((id (oref gist :id)))
    (unless (and (stringp id) (not (string-empty-p id)))
      (error "Gist edit omitted its target id: %S" id))
    (cons :id (cons (copy-sequence id) (gist385-test-stub-state gist)))))

(defun gist385-test-buffer-state (buffer)
  (with-current-buffer buffer
    (list :name (buffer-name)
          :mode major-mode
          :gist-mode (and (boundp 'gist-mode) gist-mode)
          :gist-id (and (boundp 'gist-id) gist-id)
          :gist-filename (and (boundp 'gist-filename) gist-filename)
          :modified (buffer-modified-p)
          :text (buffer-substring-no-properties (point-min) (point-max)))))

(defun gist385-test-row-id ()
  (goto-char (point-min))
  (while (and (not (eobp)) (null (tabulated-list-get-id)))
    (forward-line 1))
  (tabulated-list-get-id))

(defun gist385-test-run (body)
  (let* ((buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (buffer-before (current-buffer))
         (windows-before (current-window-configuration))
         (kill-ring (copy-tree kill-ring))
         (kill-ring-yank-pointer kill-ring)
         (gist-list-db (make-hash-table :test 'equal))
         (gist-list-db-by-user (make-hash-table :test 'equal))
         (gist-list-limits nil)
         (gist-user-history nil)
         (gh-profile-current-profile "github")
         (gh-profile-default-profile "github")
         (gh-profile-alist '(("github" :url "https://api.example")))
         (gh-api-v3-authenticator
          (lambda (&rest _)
            (make-instance 'gh-oauth-authenticator
                           :username "owner" :token "owned-token")))
         (message-log-max nil)
         (inhibit-message t)
         (gist385-test-owned-roots nil)
         result body-error cleanup-errors window-restored)
    (unwind-protect
        (condition-case error
            (cl-letf (((symbol-function 'url-retrieve)
                       (lambda (&rest args)
                         (error "Unexpected gist network request: %S" args)))
                      ((symbol-function 'url-retrieve-synchronously)
                       (lambda (&rest args)
                         (error "Unexpected synchronous gist network request: %S" args)))
                      ((symbol-function 'make-network-process)
                       (lambda (&rest args)
                         (error "Unexpected gist network process: %S" args)))
                      ((symbol-function 'open-network-stream)
                       (lambda (&rest args)
                         (error "Unexpected gist network stream: %S" args))))
              (setq result (funcall body)))
          (error (setq body-error error)))
      (condition-case error
          (progn
            (set-window-configuration windows-before)
            (when (buffer-live-p buffer-before) (set-buffer buffer-before))
            (setq window-restored t))
        (error (push (list :windows error) cleanup-errors)))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case error
              (progn (set-process-query-on-exit-flag process nil)
                     (delete-process process))
            (error (push (list :process error) cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case error
              (progn (with-current-buffer buffer (set-buffer-modified-p nil))
                     (kill-buffer buffer))
            (error (push (list :buffer error) cleanup-errors)))))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (condition-case error (cancel-timer timer)
            (error (push (list :timer error) cleanup-errors))))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case error (delete-frame frame t)
            (error (push (list :frame error) cleanup-errors)))))
      (dolist (root gist385-test-owned-roots)
        (condition-case error
            (when (file-exists-p root) (delete-directory root t))
          (error (push (list :root root error) cleanup-errors))))
    (let ((cleanup
           (list :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter (lambda (buffer)
                                       (not (memq buffer buffers-before)))
                                     (buffer-list)))
                 :new-processes
                 (mapcar #'process-name
                         (seq-filter (lambda (process)
                                       (not (memq process processes-before)))
                                     (process-list)))
                 :new-timers
                 (length
                  (seq-filter (lambda (timer) (not (memq timer timers-before)))
                              (append timer-list timer-idle-list)))
                 :new-frames
                 (length
                  (seq-filter (lambda (frame) (not (memq frame frames-before)))
                              (frame-list)))
                 :roots-exist
                 (seq-some #'file-exists-p gist385-test-owned-roots)
                 :window-restored window-restored
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "gist385 failure: %S" (list body-error cleanup-errors cleanup))
        (list :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GIST_MELPA_PIN, "gist.el")
        .expect("prepare exact shallow gist.el source graph below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn creation_commands_preserve_prompts_payload_callbacks_and_public_side_effects() -> ParityBatchCase
{
    let probe = r####"
(gist385-test-run
 (lambda ()
   (let ((gist-ask-for-description t)
         (gist-ask-for-filename t)
         (gist-view-gist t)
         (created 0) calls prompts browsed)
     (with-temp-buffer
       (rename-buffer "release helper.el" t)
       (emacs-lisp-mode)
       (insert "α start\n(message \"ship ✓\")\nω end\n")
       (goto-char (point-min))
       (forward-line 1)
       (set-mark (line-end-position))
       (activate-mark)
       (cl-letf (((symbol-function 'read-from-minibuffer)
                  (lambda (prompt &rest _)
                    (push prompt prompts) "Release λ #ship"))
                 ((symbol-function 'read-string)
                  (lambda (prompt &rest _)
                    (push prompt prompts) "deploy-界.el"))
                 ((symbol-function 'gh-gist-new)
                  (lambda (_api stub)
                    (push (gist385-test-stub-state stub) calls)
                    (setq created (1+ created))
                    (gist385-test-response
                     (gist385-test-gist
                      (format "new-%d" created) (oref stub :public)
                      (oref stub :description) (oref stub :files)))))
                 ((symbol-function 'browse-url)
                  (lambda (url &rest _) (push (copy-sequence url) browsed))))
         (call-interactively #'gist-region-or-buffer)
         (deactivate-mark)
         (call-interactively #'gist-buffer-private)))
     (list :calls (nreverse calls)
           :prompts (nreverse prompts)
           :browsed (nreverse browsed)
           :kill-ring (copy-sequence (car kill-ring))))))
"####;
    ParityBatchCase::value(
        "creation_commands_preserve_prompts_payload_callbacks_and_public_side_effects",
        probe,
        expect![[
            r#"OK (:result (:calls ((:public t :description "Release λ #ship" :files ((:filename "deploy-界.el" :content "(message \"ship ✓\")"))) (:public :json-false :description "Release λ #ship" :files ((:filename "deploy-界.el" :content "α start\n(message \"ship ✓\")\nω end\n")))) :prompts ("File name (release helper.el): " "Gist description: " "File name (release helper.el): " "Gist description: ") :browsed ("https://gist.example/new-1" "https://gist.example/new-2") :kill-ring "https://gist.example/new-2") :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :new-frames 0 :roots-exist nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_list_renders_rows_multi_file_tags_visibility_and_keymap() -> ParityBatchCase {
    let probe = r####"
(gist385-test-run
 (lambda ()
   (let* ((gist-list-format '((id "Id" 10 nil identity)
                              (visibility "Visibility" 10 nil
                               (lambda (public) (if public "public" "private")))
                              (files "Files" 22 nil (lambda (files)
                                                      (mapconcat #'identity files ",")))
                              (description "Description" 0 nil identity)))
          (one (gist385-test-gist "a1" t "Public #ship"
                                  (list (gist385-test-file "one.el" "1\n"))))
          (two (gist385-test-gist "b2" nil "Private #ship #secret"
                                  (list (gist385-test-file "two.el" "2\n")
                                        (gist385-test-file "notes.md" "n\n"))))
          (three (gist385-test-gist "c3" t "Docs #docs"
                                    (list (gist385-test-file "readme.md" "r\n"))))
          (all (list one two three))
          (responses (list all all all all))
          requests)
     (cl-letf (((symbol-function 'gh-gist-list)
                (lambda (_api username)
                  (unless responses (error "Unexpected gist list request"))
                  (push (copy-sequence username) requests)
                  (gist385-test-response (pop responses))))
               ((symbol-function 'read-from-minibuffer)
                (lambda (prompt &rest _)
                  (unless (equal prompt "GitHub user: ")
                    (error "Unexpected gist list prompt: %S" prompt))
                  "owner")))
       (gist-list nil nil)
       (with-current-buffer "*github:gists*"
         (let ((initial (list :mode major-mode
                              :ids (mapcar #'car tabulated-list-entries)
                              :text (buffer-substring-no-properties
                                     (point-min) (point-max))
                              :multi-tag
                              (save-excursion
                                (goto-char (point-min))
                                (forward-line 1)
                                (char-after))))
               (bindings (mapcar (lambda (key)
                                   (cons key (lookup-key gist-list-menu-mode-map key)))
                                 '("\r" "g" "e" "k" "+" "-" "y" "b" "*" "^" "f"))))
           (gist-list-push-visibility-limit t)
           (let ((private (list :mode-name mode-name
                                :ids (mapcar #'car tabulated-list-entries)
                                :text (buffer-substring-no-properties
                                       (point-min) (point-max)))))
             (gist-list-push-tag-limit "+ship -secret")
             (let ((private-ship
                    (list :mode-name mode-name
                          :ids (mapcar #'car tabulated-list-entries)
                          :text (buffer-substring-no-properties
                                 (point-min) (point-max)))))
               (gist-list-pop-limit nil)
               (list :initial initial :private private
                     :private-ship private-ship
                     :popped-ids (mapcar #'car tabulated-list-entries)
                     :requests (nreverse requests)
                     :responses-left (length responses)
                     :bindings bindings)))))))))
"####;
    ParityBatchCase::value(
        "public_list_renders_rows_multi_file_tags_visibility_and_keymap",
        probe,
        expect![[
            r#"OK (:result (:initial (:mode gist-list-mode :ids ("a1" "b2" "c3") :text "  a1         public     one.el                 Public #ship\n+ b2         private    two.el,notes.md        Private #ship #secret\n  c3         public     readme.md              Docs #docs\n" :multi-tag 43) :private (:mode-name "Gists[1/3]" :ids ("b2") :text "+ b2         private    two.el,notes.md        Private #ship #secret\n") :private-ship (:mode-name "Gists[0/3]" :ids nil :text "") :popped-ids ("b2") :requests ("owner" "owner" "owner" "owner") :responses-left 0 :bindings (("\15" . gist-fetch-current) ("g" . gist-list-reload) ("e" . gist-edit-current-description) ("k" . gist-kill-current) ("+" . gist-add-buffer) ("-" . gist-remove-file) ("y" . gist-print-current-url) ("b" . gist-browse-current-url) ("*" . gist-star) ("^" . gist-unstar) ("f" . gist-fork))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :new-frames 0 :roots-exist nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn fetch_save_rename_url_and_browse_follow_real_gist_mode_routes() -> ParityBatchCase {
    let probe = r####"
(gist385-test-run
 (lambda ()
   (let* ((file (gist385-test-file "demo.el" "(message \"before λ\")\n"))
          (gist (gist385-test-gist "edit-9" t "Editable" (list file)))
          edits browsed)
     (puthash "edit-9" gist gist-list-db)
     (puthash "owner" (list gist) gist-list-db-by-user)
     (cl-letf (((symbol-function 'gh-gist-edit)
               (lambda (_api update)
                  (push (gist385-test-edit-state update) edits)
                  (gist385-test-response
                   (gist385-test-gist "edit-9" t "Editable"
                                      (oref update :files)))))
               ((symbol-function 'read-from-minibuffer)
                (lambda (&rest _) "renamed-界.el"))
               ((symbol-function 'browse-url)
                (lambda (url &rest _) (push (copy-sequence url) browsed))))
       (gist-fetch "edit-9")
       (let* ((buffer (get-buffer "*gist-edit-9*/demo.el"))
              (fetched (gist385-test-buffer-state buffer)))
         (with-current-buffer buffer
           (goto-char (point-max))
           (insert ";; changed ✓\n")
           (call-interactively (or (command-remapping 'save-buffer)
                                   #'save-buffer))
           (call-interactively (or (command-remapping 'write-file)
                                   #'write-file))
           (gist-print-current-url)
           (gist-browse-current-url))
         (list :fetched fetched
               :after (gist385-test-buffer-state buffer)
               :edits (nreverse edits)
               :browsed (nreverse browsed)
               :kill-ring (copy-sequence (car kill-ring))
               :cache (gist385-test-gist-state (gethash "edit-9" gist-list-db))))))))
"####;
    ParityBatchCase::value(
        "fetch_save_rename_url_and_browse_follow_real_gist_mode_routes",
        probe,
        expect![[
            r#"OK (:result (:fetched (:name "*gist-edit-9*/demo.el" :mode emacs-lisp-mode :gist-mode t :gist-id "edit-9" :gist-filename "demo.el" :modified nil :text "(message \"before λ\")\n") :after (:name "*gist-edit-9*/renamed-界.el" :mode emacs-lisp-mode :gist-mode t :gist-id "edit-9" :gist-filename "renamed-界.el" :modified nil :text "(message \"before λ\")\n;; changed ✓\n") :edits ((:id "edit-9" :public t :description "Editable" :files ((:filename "demo.el" :content "(message \"before λ\")\n;; changed ✓\n"))) (:id "edit-9" :public t :description "Editable" :files ((:filename "demo.el" :content "") (:filename "renamed-界.el" :content "(message \"before λ\")\n;; changed ✓\n")))) :browsed ("https://gist.example/edit-9") :kill-ring "https://gist.example/edit-9" :cache (:id "edit-9" :public t :description "Editable" :html-url "https://gist.example/edit-9" :files ((:filename "demo.el" :content "") (:filename "renamed-界.el" :content "(message \"before λ\")\n;; changed ✓\n")))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :new-frames 0 :roots-exist nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_list_mutations_enforce_ownership_and_emit_exact_dependency_calls() -> ParityBatchCase {
    let probe = r####"
(gist385-test-run
 (lambda ()
   (let* ((base-file (gist385-test-file "base.txt" "base\n"))
          (gist (gist385-test-gist "mut-7" t "Before" (list base-file)))
          (foreign (gist385-test-gist "foreign" t "Other" (list base-file)))
          calls denied (list-calls 0))
     (puthash "mut-7" gist gist-list-db)
     (puthash "foreign" foreign gist-list-db)
     (puthash "owner" (list gist) gist-list-db-by-user)
     (cl-letf (((symbol-function 'gh-gist-edit)
                  (lambda (_api update)
                    (push (cons 'edit (gist385-test-edit-state update)) calls)
                    (gist385-test-response update)))
                 ((symbol-function 'gh-gist-set-star)
                  (lambda (_api id how)
                    (push (list 'star id how) calls)
                    (gist385-test-response 'empty)))
                 ((symbol-function 'gh-gist-fork)
                  (lambda (_api id)
                    (push (list 'fork id) calls)
                    (gist385-test-response gist)))
                 ((symbol-function 'gh-gist-delete)
                  (lambda (_api id)
                    (push (list 'delete id) calls)
                    (gist385-test-response 'empty)))
                 ((symbol-function 'gh-gist-list)
                  (lambda (_api username)
                    (unless (< list-calls 4)
                      (error "Unexpected mutation list request: %S" username))
                    (setq list-calls (1+ list-calls))
                    (push (list 'list username) calls)
                    (gist385-test-response
                     (cond ((equal username "owner") (list gist))
                           ((equal username "other") (list foreign))
                           (t (error "Unexpected mutation user: %S" username))))))
                 ((symbol-function 'read-from-minibuffer)
                  (lambda (&rest _) "After λ"))
                 ((symbol-function 'yes-or-no-p) (lambda (&rest _) t)))
       (gist-list-user "other" nil nil)
       (with-current-buffer "*github:other's-gists*"
         (goto-char (point-min))
         (unless (equal (gist385-test-row-id) "foreign")
           (error "Foreign gist was not selected"))
         (setq denied
               (condition-case error
                   (progn (gist-edit-current-description) nil)
                 (user-error (list :id (tabulated-list-get-id)
                                   :type (car error) :data (cdr error))))))
       (gist-list nil nil)
       (with-current-buffer "*github:gists*"
         (goto-char (point-min))
         (gist385-test-row-id)
         (gist-edit-current-description)
         (gist-star)
         (gist-unstar)
         (gist-fork)
         (gist-kill-current)))
     (unless (= list-calls 4)
       (error "Missing mutation list request: %S" list-calls))
     (list :calls (nreverse calls)
           :denied denied
           :cache-owner (mapcar (lambda (item) (oref item :id))
                                (gethash "owner" gist-list-db-by-user))))))
"####;
    ParityBatchCase::value(
        "public_list_mutations_enforce_ownership_and_emit_exact_dependency_calls",
        probe,
        expect![[
            r#"OK (:result (:calls ((list "other") (list "owner") (edit :id "mut-7" :public t :description "After λ" :files nil) (list "owner") (star "mut-7" t) (star "mut-7" nil) (fork "mut-7") (delete "mut-7") (list "owner")) :denied (:id "foreign" :type user-error :data ("Can’t edit a gist that doesn’t belong to you")) :cache-owner ("mut-7")) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :new-frames 0 :roots-exist nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn dired_posting_uses_marked_file_bytes_and_recovers_after_missing_input() -> ParityBatchCase {
    let probe = r####"
(gist385-test-run
 (lambda ()
   (let* ((root (expand-file-name "dired-case/" gist385-test-root))
          (one (expand-file-name "one.txt" root))
          (two (expand-file-name "two.el" root))
          calls missing)
     (when (file-exists-p root) (delete-directory root t))
     (make-directory root t)
     (push root gist385-test-owned-roots)
     (unwind-protect
         (progn
           (write-region "first ✓\n" nil one nil 'silent)
           (write-region "(message \"second\")\n" nil two nil 'silent)
           (cl-letf (((symbol-function 'gh-gist-new)
                      (lambda (_api stub)
                        (push (gist385-test-stub-state stub) calls)
                        (gist385-test-response
                         (gist385-test-gist "dired-1" (oref stub :public) ""
                                            (oref stub :files))))))
             (with-current-buffer (dired-noselect root)
               (dired-mark-files-regexp "one\\|two")
               (dired-do-gist t))
             (setq missing
                   (condition-case error
                       (progn (gist-files (list (expand-file-name "missing" root))) nil)
                     (file-missing (list :type (car error)
                                         :mentions-missing
                                         (and (string-match-p "missing"
                                                              (error-message-string error)) t)))))
             (gist-files (list two) nil)))
       nil)
     (list :calls (nreverse calls) :missing missing
           :binding (lookup-key dired-mode-map "@")))))
"####;
    ParityBatchCase::value(
        "dired_posting_uses_marked_file_bytes_and_recovers_after_missing_input",
        probe,
        expect![[
            r#"OK (:result (:calls ((:public :json-false :description "" :files ((:filename "two.el" :content "(message \"second\")\n") (:filename "one.txt" :content "first ✓\n"))) (:public t :description "" :files ((:filename "two.el" :content "(message \"second\")\n")))) :missing (:type file-missing :mentions-missing t) :binding dired-do-gist) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :new-frames 0 :roots-exist nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        creation_commands_preserve_prompts_payload_callbacks_and_public_side_effects(),
        public_list_renders_rows_multi_file_tags_visibility_and_keymap(),
        fetch_save_rename_url_and_browse_follow_real_gist_mode_routes(),
        public_list_mutations_enforce_ownership_and_emit_exact_dependency_calls(),
        dired_posting_uses_marked_file_bytes_and_recovers_after_missing_input(),
    ]
}

#[test]
fn gist_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "gist-package-batch",
        "gist.el",
        &workflow_batch_cases(),
    );
}
