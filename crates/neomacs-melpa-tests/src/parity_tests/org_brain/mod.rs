//! Practical parity for Org Brain's user-visible knowledge-map workflows.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ORG_BRAIN_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'org)
(require 'org-id)
(require 'org-brain)

(let* ((source (symbol-file 'org-brain-visualize 'defun))
       (directory (and source (file-name-directory source)))
       (payload
        (and directory
             (sort (seq-filter
                    (lambda (name)
                      (and (string-suffix-p ".el" name)
                           (not (string-suffix-p "-autoloads.el" name))
                           (not (string-suffix-p "-pkg.el" name))))
                    (directory-files directory nil nil t))
                   #'string<))))
  (unless (and source
               (equal (file-name-nondirectory source) "org-brain.el")
               (equal payload '("org-brain.el"))
               (file-regular-p source)
               (not (file-symlink-p source)))
    (error "Unexpected installed Org Brain source: %S" source))
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally source)
    (unless (equal (secure-hash 'sha256 (current-buffer))
                   "caf148bd26c9529833252209c6c52655ada7a56d9f20612cdb03a3dfa39a0f42")
      (error "Unexpected installed Org Brain source digest"))))

(defun org-brain403-test-file-sha256 (file root)
  (with-temp-buffer
    (insert-file-contents file)
    (let ((text (buffer-string)))
      (when root
        (setq text
              (replace-regexp-in-string
               (regexp-quote (directory-file-name root)) "[ROOT]" text t t)))
      (secure-hash 'sha256 text))))

(defun org-brain403-test-normalize (value root)
  (cond
   ((stringp value)
    (if root
        (replace-regexp-in-string
         (regexp-quote (directory-file-name root)) "[ROOT]" value t t)
      (copy-sequence value)))
   ((markerp value)
    (list :position (marker-position value)
          :buffer (and (marker-buffer value) (buffer-name (marker-buffer value)))))
   ((consp value)
    (cons (org-brain403-test-normalize (car value) root)
          (org-brain403-test-normalize (cdr value) root)))
   ((vectorp value)
    (apply #'vector
           (mapcar (lambda (item) (org-brain403-test-normalize item root)) value)))
   ((hash-table-p value)
    (let (entries)
      (maphash (lambda (key item)
                 (push (cons (org-brain403-test-normalize key root)
                             (org-brain403-test-normalize item root))
                       entries))
               value)
      (sort entries (lambda (left right)
                      (string< (format "%S" (car left))
                               (format "%S" (car right)))))))
   (t value)))

(defun org-brain403-test-condition (condition root)
  (list :error (car condition)
        :data (org-brain403-test-normalize (copy-tree (cdr condition)) root)))

(defun org-brain403-test-write-file (root relative content)
  (let* ((expanded-root (file-name-as-directory (expand-file-name root)))
         (file (expand-file-name relative expanded-root)))
    (unless (and (not (equal file (directory-file-name expanded-root)))
                 (string-prefix-p expanded-root file))
      (error "Refusing Org Brain fixture outside root: %s" file))
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix))
      (with-temp-file file (insert content)))
    file))

(defun org-brain403-test-manifest (root)
  (let (entries)
    (dolist (file (directory-files-recursively root "." nil nil t))
      (when (file-regular-p file)
        (push (cons (file-relative-name file root)
                    (org-brain403-test-file-sha256 file root))
              entries)))
    (sort entries (lambda (left right) (string< (car left) (car right))))))

(defun org-brain403-test-read-file (file)
  (with-temp-buffer
    (insert-file-contents file)
    (buffer-string)))

(defun org-brain403-test-buttons ()
  (let ((position (point-min)) buttons button)
    (while (setq button (next-button position t))
      (push (list :start (button-start button)
                  :end (button-end button)
                  :label (substring-no-properties (button-label button))
                  :id (button-get button 'id)
                  :category (button-get button 'brain-category)
                  :help (button-get button 'help-echo))
            buttons)
      (setq position (button-end button)))
    (nreverse buttons)))

(defun org-brain403-test-selected-locus ()
  (let* ((window (selected-window))
         (buffer (window-buffer window)))
    (with-current-buffer buffer
      (list :buffer (buffer-name buffer)
            :file (and buffer-file-name
                       (file-relative-name buffer-file-name org-brain-path))
            :point (window-point window)
            :line (line-number-at-pos (window-point window))
            :column (save-excursion
                      (goto-char (window-point window))
                      (current-column))))))

(defun org-brain403-test-window-state ()
  (mapcar (lambda (window)
            (list (buffer-name (window-buffer window))
                  (window-point window) (window-start window)
                  (window-dedicated-p window)))
          (seq-mapcat (lambda (frame) (window-list frame 'nomini)) (frame-list))))

(defun org-brain403-test-park-buffer (name)
  (when-let ((buffer (get-buffer name)))
    (let ((parked (generate-new-buffer-name (concat " *parked " name "*"))))
      (with-current-buffer buffer (rename-buffer parked t))
      (cons buffer name))))

(defun org-brain403-test-forbid-external (kind &rest arguments)
  (error "Unexpected external Org Brain boundary: %S" (cons kind arguments)))

(defun org-brain403-test-new-mark-ring ()
  (let (ring)
    (dotimes (_ org-mark-ring-length)
      (push (make-marker) ring))
    (setq ring (nreverse ring))
    (when ring
      (setcdr (last ring) ring))
    ring))

(defun org-brain403-test-run (files body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name "org-brain/" sandbox))))
         (brain (and root (file-name-as-directory (expand-file-name "brain/" root))))
         (window-before (current-window-configuration))
         (window-state-before (org-brain403-test-window-state))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (org-directory brain)
         (org-brain-path brain)
         (org-brain-data-file (and brain (expand-file-name ".org-brain-data.el" brain)))
         (org-id-locations-file (and root (expand-file-name "org-id-locations" root)))
         (org-id-locations (make-hash-table :test 'equal))
         (org-id-files nil)
         (org-id--locations-checksum nil)
         (org-id-overriding-file-name nil)
         (org-id-extra-files nil)
         (org-mark-ring (org-brain403-test-new-mark-ring))
         (org-mark-ring-last-goto nil)
         (org-file-apps '((auto-mode . emacs)))
         (org-link-frame-setup '((file . find-file)))
         (org-agenda-files nil)
         (org-brain-headline-cache (make-hash-table :test 'equal))
         (org-brain-pins nil)
         (org-brain-selected nil)
         (org-brain--vis-entry nil)
         (org-brain--vis-entry-keywords nil)
         (org-brain--vis-history nil)
         (org-brain--vis-entry-text-marker 0)
         (org-brain--visualize-follow nil)
         (org-brain-wander-timer nil)
         (org-brain-visualizing-mind-map nil)
         (org-brain-mind-map-parent-level
          (default-value 'org-brain-mind-map-parent-level))
         (org-brain-mind-map-child-level
          (default-value 'org-brain-mind-map-child-level))
         (org-brain-show-icons nil)
         (org-brain-show-history t)
         (org-brain-show-resources t)
         (org-brain-show-text t)
         (org-brain-open-same-window t)
         (org-brain-new-entry-hook nil)
         (org-brain-visualize-mode-hook nil)
         (org-brain-visualize-text-hook nil)
         (org-brain-after-visualize-hook nil)
         (org-brain-visualize-follow-hook nil)
         (org-brain-after-resource-button-functions nil)
         (org-brain-vis-title-prepend-functions nil)
         (org-brain-vis-title-append-functions nil)
         (org-brain-vis-current-title-prepend-functions nil)
         (org-brain-vis-current-title-append-functions nil)
         (org-brain-visualize-sort-function 'org-brain-title<)
         (org-brain-scan-directories-recursively t)
         (org-brain-include-file-entries t)
         (org-brain-file-entries-use-title t)
         (org-brain-scan-for-header-entries t)
         (org-brain-visualize-default-choices 'all)
         (auto-save-default nil)
         (create-lockfiles nil)
         (make-backup-files nil)
         (message-log-max nil)
         (print-circle nil)
         (root-owned nil)
         (parked nil)
         fixture-before fixture-after result body-error cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root brain (file-name-absolute-p root))
                (error "Missing absolute Org Brain sandbox root"))
              (when (file-exists-p root)
                (error "Org Brain sandbox root already exists: %s" root))
              (when-let ((entry (org-brain403-test-park-buffer "*org-brain*")))
                (push entry parked))
              (setq root-owned t)
              (make-directory brain t)
              (dolist (entry files)
                (org-brain403-test-write-file brain (car entry) (cdr entry)))
              (setq fixture-before (org-brain403-test-manifest root))
              (setq result
                    (cl-letf (((symbol-function 'call-process)
                               (lambda (&rest args)
                                 (apply #'org-brain403-test-forbid-external
                                        'call-process args)))
                              ((symbol-function 'call-process-region)
                               (lambda (&rest args)
                                 (apply #'org-brain403-test-forbid-external
                                        'call-process-region args)))
                              ((symbol-function 'process-file)
                               (lambda (&rest args)
                                 (apply #'org-brain403-test-forbid-external
                                        'process-file args)))
                              ((symbol-function 'start-process)
                               (lambda (&rest args)
                                 (apply #'org-brain403-test-forbid-external
                                        'start-process args)))
                              ((symbol-function 'start-file-process)
                               (lambda (&rest args)
                                 (apply #'org-brain403-test-forbid-external
                                        'start-file-process args)))
                              ((symbol-function 'make-process)
                               (lambda (&rest args)
                                 (apply #'org-brain403-test-forbid-external
                                        'make-process args)))
                              ((symbol-function 'make-network-process)
                               (lambda (&rest args)
                                 (apply #'org-brain403-test-forbid-external
                                        'make-network-process args)))
                              ((symbol-function 'open-network-stream)
                               (lambda (&rest args)
                                 (apply #'org-brain403-test-forbid-external
                                        'open-network-stream args)))
                              ((symbol-function 'url-retrieve)
                               (lambda (&rest args)
                                 (apply #'org-brain403-test-forbid-external
                                        'url-retrieve args)))
                              ((symbol-function 'url-retrieve-synchronously)
                               (lambda (&rest args)
                                 (apply #'org-brain403-test-forbid-external
                                        'url-retrieve-synchronously args))))
                      (funcall body brain)))
              (setq fixture-after (org-brain403-test-manifest root)))
          (error (setq body-error (org-brain403-test-condition condition root))))
      (when (timerp org-brain-wander-timer)
        (condition-case condition (cancel-timer org-brain-wander-timer)
          (error (push (org-brain403-test-condition condition root) cleanup-errors))))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case condition (delete-process process)
            (error (push (org-brain403-test-condition condition root) cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case condition
              (progn
                (with-current-buffer buffer (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error (push (org-brain403-test-condition condition root) cleanup-errors)))))
      (dolist (timer (copy-sequence timer-list))
        (unless (memq timer timers-before)
          (condition-case condition (cancel-timer timer)
            (error (push (org-brain403-test-condition condition root) cleanup-errors)))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case condition (delete-frame frame t)
            (error (push (org-brain403-test-condition condition root) cleanup-errors)))))
      (condition-case condition (set-window-configuration window-before)
        (error (push (org-brain403-test-condition condition root) cleanup-errors)))
      (dolist (entry parked)
        (condition-case condition
            (if (buffer-live-p (car entry))
                (with-current-buffer (car entry) (rename-buffer (cdr entry) t))
              (error "Parked Org Brain buffer died: %S" entry))
          (error (push (org-brain403-test-condition condition root) cleanup-errors))))
      (when (buffer-live-p buffer-before) (set-buffer buffer-before))
      (when root-owned
        (condition-case condition (delete-directory root t)
          (error (push (org-brain403-test-condition condition root) cleanup-errors)))))
    (let ((cleanup
           (list :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter
                          (lambda (buffer)
                            (and (buffer-live-p buffer)
                                 (not (memq buffer buffers-before))))
                          (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process)
                                       (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     timer-list))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :window-restored
                 (equal window-state-before (org-brain403-test-window-state))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Org Brain workflow failed: %S" (list result cleanup))
        (org-brain403-test-normalize
         (list :result result
               :fixture-before fixture-before
               :fixture-after fixture-after
               :cleanup cleanup)
         root)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ORG_BRAIN_MELPA_PIN, "org-brain.el")
        .expect("prepare exact shallow Org Brain source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_visualization_buttons_history_and_goto_real_entries() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_visualization_buttons_history_and_goto_real_entries",
        r####"
(org-brain403-test-run
 '(("root.org" . "#+TITLE: Root Café\n#+BRAIN_CHILDREN: child\n#+BRAIN_FRIENDS: friend\n\nRoot body 界.\n\n* TODO Alpha Node :ship:\n:PROPERTIES:\n:ID: h-alpha\n:END:\nAlpha body.\n")
   ("child.org" . "#+TITLE: Child β\n#+BRAIN_PARENTS: root\n\nChild body.\n")
   ("friend.org" . "#+TITLE: Friend γ\n#+BRAIN_FRIENDS: root\n\nFriend body.\n"))
 (lambda (_brain)
   (org-brain-update-id-locations)
   (org-brain-visualize "root")
   (let ((initial
          (with-current-buffer "*org-brain*"
            (list :mode major-mode
                  :entry org-brain--vis-entry
                  :history (copy-tree org-brain--vis-history)
                  :text (buffer-substring-no-properties (point-min) (point-max))
                  :buttons (org-brain403-test-buttons)
                  :keys (list (lookup-key org-brain-visualize-mode-map "o")
                              (lookup-key org-brain-visualize-mode-map "b")))))
         child back headline)
     (with-current-buffer "*org-brain*"
       (goto-char (point-min))
       (while (and (forward-button 1)
                   (not (equal (button-get (button-at (point)) 'id) "child"))))
       (push-button)
       (setq child
             (list :entry org-brain--vis-entry
                   :history (copy-tree org-brain--vis-history)
                   :text (buffer-substring-no-properties (point-min) (point-max))
                   :buttons (org-brain403-test-buttons)))
       (org-brain-visualize-back)
       (setq back (list :entry org-brain--vis-entry
                        :history (copy-tree org-brain--vis-history)
                        :point (point))))
     (org-brain-goto (org-brain-entry-from-id "h-alpha"))
     (setq headline
           (list :locus (org-brain403-test-selected-locus)
                 :heading (substring-no-properties (org-get-heading t t t t))
                 :entry (org-brain-entry-at-pt)))
     (list :files (org-brain-files t)
           :headlines (org-brain-headline-entries)
           :initial initial :child child :back back :headline headline))))
"####,
        expect![[
            r#"OK (:result (:files ("child" "friend" "root") :headlines (("root" "Alpha Node" "h-alpha")) :initial (:mode org-brain-visualize-mode :entry "root" :history ("root") :text "PINNED:\nHISTORY:  Root Café\n\n\nRoot Café <-> Friend γ\n\nAlpha Node  Child β  \n\n--- Entry -------------------------------------\n\nRoot body 界.\n" :buttons ((:start 19 :end 28 :label "Root Café" :id "root" :category history :help nil) (:start 45 :end 53 :label "Friend γ" :id "friend" :category friend :help nil) (:start 55 :end 65 :label "Alpha Node" :id "h-alpha" :category child :help nil) (:start 67 :end 74 :label "Child β" :id "child" :category child :help nil)) :keys (org-brain-goto-current org-brain-visualize-back)) :child (:entry "child" :history ("child" "root") :text "PINNED:\nHISTORY:  Root Café  Child β\n\n\n   Root Café-+-Alpha Node\n       |\n       V\n    Child β\n\n--- Entry -------------------------------------\n\nChild body." :buttons ((:start 19 :end 28 :label "Root Café" :id "root" :category history :help nil) (:start 30 :end 37 :label "Child β" :id "child" :category history :help nil) (:start 43 :end 52 :label "Root Café" :id "root" :category parent :help nil) (:start 55 :end 65 :label "Alpha Node" :id "h-alpha" :category sibling :help nil))) :back (:entry "root" :history ("root") :point 31) :headline (:locus (:buffer "root.org" :file "root.org" :point 83 :line 7 :column 0) :heading "Alpha Node" :entry ("root" "Alpha Node" "h-alpha"))) :fixture-before (("brain/child.org" . "7e7df1cb2c3430ae112fb90f6162d86e17b83099d54dbfeb8475d3ae9ff91319") ("brain/friend.org" . "a33d213f96d623b3ef075cc8e6993f3b436172ffad476d4bfebf6fbf3cd3eae9") ("brain/root.org" . "980055eeb5be6c82f10d75ec67fc43b885217c58cf71b40f6e50b18e684c7495")) :fixture-after (("brain/child.org" . "7e7df1cb2c3430ae112fb90f6162d86e17b83099d54dbfeb8475d3ae9ff91319") ("brain/friend.org" . "a33d213f96d623b3ef075cc8e6993f3b436172ffad476d4bfebf6fbf3cd3eae9") ("brain/root.org" . "980055eeb5be6c82f10d75ec67fc43b885217c58cf71b40f6e50b18e684c7495") ("org-id-locations" . "4a5c33729d9767c4a69a00f55129d666f5d92493a38b267dc308273efe8d0920")) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_relationship_rename_failure_and_recovery_preserve_graph() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_relationship_rename_failure_and_recovery_preserve_graph",
        r####"
(org-brain403-test-run
 '(("root.org" . "#+TITLE: Root Café\n\nRoot body.\n")
   ("child.org" . "#+TITLE: Child β\n\nChild body.\n")
   ("friend.org" . "#+TITLE: Friend γ\n\nFriend body.\n")
   ("existing.org" . "#+TITLE: Existing\n\nCollision guard.\n"))
 (lambda (brain)
   (org-brain-update-id-locations)
   (org-brain-add-entry "Created δ")
   (let* ((self-before (org-brain403-test-manifest brain))
          (self-error
           (condition-case condition
               (progn (org-brain-add-child "root" '("root")) nil)
             (error (org-brain403-test-condition condition brain))))
          (self-after (org-brain403-test-manifest brain)))
     (org-brain-add-child "root" '("child" "Created δ") t)
     (org-brain-add-friendship "root" '("friend") t)
     (org-brain-pin "root" 1)
     (org-brain-select "child" 1)
     (let* ((collision-before (org-brain403-test-manifest brain))
            (collision-error
             (condition-case condition
                 (progn (org-brain-rename-file "child" "existing") nil)
               (error (org-brain403-test-condition condition brain))))
            (collision-after (org-brain403-test-manifest brain)))
       (org-brain-rename-file "child" "nested/renamed child 界")
       (org-brain-set-title "nested/renamed child 界" "Renamed Child 界")
       (org-brain-add-nickname "nested/renamed child 界" "β-child")
       (list :self-error self-error
             :self-atomic (equal self-before self-after)
             :collision-error collision-error
             :collision-atomic (equal collision-before collision-after)
             :files (org-brain-files t)
             :root-children (org-brain-children "root")
             :created-text (org-brain403-test-read-file
                            (expand-file-name "Created δ.org" brain))
             :renamed-parents (org-brain-parents "nested/renamed child 界")
             :root-friends (org-brain-friends "root")
             :friend-friends (org-brain-friends "friend")
             :pins (copy-tree org-brain-pins)
             :selected (copy-tree org-brain-selected)
             :root-text (org-brain403-test-read-file
                         (expand-file-name "root.org" brain))
             :renamed-text (org-brain403-test-read-file
                            (expand-file-name "nested/renamed child 界.org" brain))
             :data-text (org-brain403-test-read-file org-brain-data-file))))))
"####,
        expect![[
            r##"OK (:result (:self-error (:error error :data ("An entry can’t be a parent/child to itself")) :self-atomic t :collision-error (:error error :data ("There’s already a file [ROOT]/existing.org")) :collision-atomic t :files ("nested/renamed child 界" "Created δ" "existing" "friend" "root") :root-children ("Created δ" "nested/renamed child 界") :created-text "#+BRAIN_PARENTS: root\n\n" :renamed-parents ("root") :root-friends ("friend") :friend-friends ("root") :pins ("root") :selected ("nested/renamed child 界" "nested/renamed child 界") :root-text "#+BRAIN_FRIENDS: friend\n\n#+BRAIN_CHILDREN: Created%20δ nested/renamed%20child%20界\n\n#+TITLE: Root Café\n\nRoot body.\n" :renamed-text "#+NICKNAMES: β-child\n#+BRAIN_PARENTS: root\n\n#+TITLE: Renamed Child 界\n\nChild body.\n" :data-text "(setq org-brain-pins\n      '(\n\11\"root\"\n\11))\n") :fixture-before (("brain/child.org" . "8abe74c10b6abe5a2efa81a523eaee19aa283f9a26a02693dc5452592268cb26") ("brain/existing.org" . "19cffa18420f15e20de8b16c5ae9d057b30dda77ccdd160f422e8a43569444b8") ("brain/friend.org" . "4164e5e5e444fc73e134c42a19ed34c4596929a3ef3bec40561524d85f23f5ac") ("brain/root.org" . "99e9e39f6c477126f8c182421cc97c985cdf546f87ebc101d1d6acd1e2574b6e")) :fixture-after (("brain/.org-brain-data.el" . "e86c6c806e40c4d15d9208c0147e4aefc3668f5e386816c2e8f83a2e3dd64dd8") ("brain/Created δ.org" . "3bcc9f92c6b63866cfa9308404a61ae08f626d159c49e87d1e33c91b190e7512") ("brain/existing.org" . "19cffa18420f15e20de8b16c5ae9d057b30dda77ccdd160f422e8a43569444b8") ("brain/friend.org" . "8f91df97644b27b46e85a88a68b2b897c16c23d84047da7a1b1c15f76a7d0715") ("brain/nested/renamed child 界.org" . "419d6970fb036b183c49973b8673d77ffd85ffdbba4262d9c8749898b5ed73af") ("brain/root.org" . "5bd7b0517d4e7976e0e9212c1f487e4f11148dc53a8f0e4a9afacc8285c13da3") ("org-id-locations" . "3d5138989761a3818a7eb6470ecf12d19ed9640741f3f6f6274ef917181fc137")) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn public_resource_command_opens_exact_owned_file_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_resource_command_opens_exact_owned_file_line",
        r####"
(org-brain403-test-run
 '(("root.org" . "#+TITLE: Root Café\n\nRoot body.\n")
   ("reference.txt" . "first line\nsecond café 界\nthird line\n"))
 (lambda (brain)
   (let ((reference (expand-file-name "reference.txt" brain)))
     (org-brain-add-file-line-as-resource reference "2" "root")
     (let ((resources (copy-tree (org-brain-resources "root")))
           (root-text (org-brain403-test-read-file
                       (expand-file-name "root.org" brain))))
       (org-brain-open-resource "root")
       (list :resources resources
             :root-text root-text
             :destination (org-brain403-test-selected-locus)
             :destination-line
             (buffer-substring-no-properties (line-beginning-position)
                                             (line-end-position)))))))
"####,
        expect![[
            r##"OK (:result (:resources (("file:[ROOT]/brain/reference.txt::2")) :root-text "#+TITLE: Root Café\n:RESOURCES:\n- [[file:[ROOT]/brain/reference.txt::2]]\n:END:\n\n\nRoot body.\n" :destination (:buffer "reference.txt" :file "reference.txt" :point 12 :line 2 :column 0) :destination-line "second café 界") :fixture-before (("brain/reference.txt" . "68634ffb58daa1edfc2ee4891f5609c4cbc6c55de5098d262eed93122f1d8c5c") ("brain/root.org" . "99e9e39f6c477126f8c182421cc97c985cdf546f87ebc101d1d6acd1e2574b6e")) :fixture-after (("brain/reference.txt" . "68634ffb58daa1edfc2ee4891f5609c4cbc6c55de5098d262eed93122f1d8c5c") ("brain/root.org" . "f16d8cbe5875454e34ba19f818a25c11fe39f73e35ffc32eff6a23ce14e7b029")) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn public_switch_brain_loads_and_restores_independent_pins() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_switch_brain_loads_and_restores_independent_pins",
        r####"
(org-brain403-test-run
 '(("root.org" . "#+TITLE: Primary Root\n\nPrimary.\n"))
 (lambda (brain)
   (let* ((root (file-name-directory (directory-file-name brain)))
          (alternate (file-name-as-directory (expand-file-name "alternate" root))))
     (org-brain-pin "root" 1)
     (org-brain403-test-write-file alternate "alt.org"
                                   "#+TITLE: Alternate 界\n\nAlternate body.\n")
     (org-brain403-test-write-file
      alternate ".org-brain-data.el"
      "(setq org-brain-pins\n      '(\n        \"alt\"\n        ))\n")
     (org-brain-switch-brain alternate)
     (let ((alternate-state
            (list :path org-brain-path
                  :data org-brain-data-file
                  :files (org-brain-files t)
                  :pins (copy-tree org-brain-pins)
                  :history (copy-tree org-brain--vis-history))))
       (org-brain-visualize "alt")
       (let ((alternate-visual
              (with-current-buffer "*org-brain*"
                (list :entry org-brain--vis-entry
                      :text (buffer-substring-no-properties
                             (point-min) (point-max))))))
         (org-brain-switch-brain brain)
         (list :alternate alternate-state
               :visual alternate-visual
               :primary (list :path org-brain-path
                              :files (org-brain-files t)
                              :pins (copy-tree org-brain-pins)
                              :history (copy-tree org-brain--vis-history))))))))
"####,
        expect![[
            r#"OK (:result (:alternate (:path "[ROOT]/alternate/" :data "[ROOT]/alternate/.org-brain-data.el" :files ("alt") :pins ("alt") :history nil) :visual (:entry "alt" :text "PINNED:  Alternate 界\nHISTORY:  Alternate 界\n\n\nAlternate 界\n\n--- Entry -------------------------------------\n\nAlternate body.") :primary (:path "[ROOT]/brain/" :files ("root") :pins ("root") :history nil)) :fixture-before (("brain/root.org" . "b1fbafaec4943a7db5424adcad71f0b0f36a9d23a0f9ef78e1cf343892c78f7c")) :fixture-after (("alternate/.org-brain-data.el" . "ff0fadf3f99bc5ee8a4758463705038b8adae60ac83c3d6367b253412d2c2709") ("alternate/alt.org" . "2f7bb17a0d9d558d2b8b710ab0797e96b76060d3375480dbaad41f3e4784c890") ("brain/.org-brain-data.el" . "e86c6c806e40c4d15d9208c0147e4aefc3668f5e386816c2e8f83a2e3dd64dd8") ("brain/root.org" . "b1fbafaec4943a7db5424adcad71f0b0f36a9d23a0f9ef78e1cf343892c78f7c")) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn org_brain_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_visualization_buttons_history_and_goto_real_entries(),
        public_relationship_rename_failure_and_recovery_preserve_graph(),
        public_resource_command_opens_exact_owned_file_line(),
        public_switch_brain_loads_and_restores_independent_pins(),
    ];
    assert_oracle_batch_cases(oracle(), "org-brain-rank403", "org-brain", &cases);
}
