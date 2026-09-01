//! Practical parity for JSON Navigator's public hierarchy workflows.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, JSON_NAVIGATOR_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'json-navigator)
(set-window-configuration (current-window-configuration))

(defconst json-navigator421-test-tree
  "35664192e30b1671dcf5cd6498fdb70ef325363f")
(defconst json-navigator421-test-manifest
  '(("json-navigator-pkg.el" . "66a415efe2ffa87a5dd579a8b8133f33005bd156d9ab7cc6fcf5fb75f0eeddc7")
    ("json-navigator.el" . "18efb2d5b858625275f7377209055e387809778b3cec1b3ac8ac2c01d5ef50eb")))

(defun json-navigator421-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun json-navigator421-test-source-state ()
  (let* ((main (symbol-file 'json-navigator-navigate-region 'defun))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<))))
    (unless (and main
                 (not (file-symlink-p main))
                 (equal files (mapcar #'car json-navigator421-test-manifest)))
      (error "Unexpected installed JSON Navigator payload: %S" files))
    (dolist (entry json-navigator421-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (json-navigator421-test-sha file) (cdr entry)))
          (error "Unexpected installed JSON Navigator source: %S" entry))))
    (list :tree json-navigator421-test-tree
          :manifest json-navigator421-test-manifest
          :feature (featurep 'json-navigator)
          :version "20241031.630")))

(defun json-navigator421-test-condition (condition)
  (list :type (car condition)
        :data (copy-tree (cdr condition))
        :message (error-message-string condition)))

(defun json-navigator421-test-window-state ()
  (mapcar
   (lambda (window)
     (list :window window
           :selected (eq window (selected-window))
           :buffer (window-buffer window)
           :point (window-point window)
           :start (window-start window)
           :hscroll (window-hscroll window)
           :dedicated (window-dedicated-p window)
           :edges (window-edges window)))
   (seq-mapcat (lambda (frame) (window-list frame 'nomini)) (frame-list))))

(defun json-navigator421-test-tree-state ()
  (let ((buffer (window-buffer (selected-window))))
    (with-current-buffer buffer
      (list :name (buffer-name)
            :mode major-mode
            :read-only buffer-read-only
            :modified (buffer-modified-p)
            :point (point)
            :text (buffer-substring-no-properties (point-min) (point-max))))))

(defun json-navigator421-test-source-buffer-state ()
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :mark (when (mark t) (mark t))
        :mark-active mark-active
        :modified (buffer-modified-p)))

(defun json-navigator421-test-park-buffer (name)
  (when-let ((buffer (get-buffer name)))
    (let ((parked (generate-new-buffer-name
                   (format " *json-navigator421-parked:%s*" name))))
      (with-current-buffer buffer (rename-buffer parked t))
      (cons buffer name))))

(defvar json-navigator421-test-case-index 0)
(defvar json-navigator421-test-root nil)
(defvar json-navigator421-test-root-owned nil)

(defun json-navigator421-test-run (body)
  (let* ((index (cl-incf json-navigator421-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "json-navigator-%d" index)
                                       sandbox))))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (buffer-before (current-buffer))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (window-state-before (json-navigator421-test-window-state))
         (source-before (json-navigator421-test-source-state))
         (json-navigator421-test-root root)
         (json-navigator421-test-root-owned nil)
         (json-navigator-display-length 3)
         (transient-mark-mode transient-mark-mode)
         parked result body-error cleanup-errors fixture-before fixture-after
         source-after)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute JSON Navigator sandbox root"))
              (when (file-exists-p root)
                (error "JSON Navigator sandbox root exists: %S" root))
              (when-let ((entry
                          (json-navigator421-test-park-buffer
                           "*hierarchy-tree*")))
                (push entry parked))
              (make-directory root)
              (setq json-navigator421-test-root-owned t
                    fixture-before (directory-files root nil nil t))
              (cl-letf (((symbol-function 'call-process)
                         (lambda (&rest args)
                           (error "Unexpected call-process: %S" args)))
                        ((symbol-function 'call-process-region)
                         (lambda (&rest args)
                           (error "Unexpected call-process-region: %S" args)))
                        ((symbol-function 'process-file)
                         (lambda (&rest args)
                           (error "Unexpected process-file: %S" args)))
                        ((symbol-function 'start-process)
                         (lambda (&rest args)
                           (error "Unexpected start-process: %S" args)))
                        ((symbol-function 'start-file-process)
                         (lambda (&rest args)
                           (error "Unexpected start-file-process: %S" args)))
                        ((symbol-function 'make-process)
                         (lambda (&rest args)
                           (error "Unexpected make-process: %S" args)))
                        ((symbol-function 'make-network-process)
                         (lambda (&rest args)
                           (error "Unexpected network process: %S" args)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest args)
                           (error "Unexpected URL retrieval: %S" args))))
                (setq result (funcall body))))
          (t (setq body-error (json-navigator421-test-condition condition))))
      (condition-case condition
          (setq fixture-after (and root (file-exists-p root)
                                   (directory-files root nil nil t)))
        (t (push (json-navigator421-test-condition condition) cleanup-errors)))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case condition (delete-process process)
            (t (push (json-navigator421-test-condition condition) cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (or (memq buffer buffers-before) (assq buffer parked))
          (condition-case condition
              (with-current-buffer buffer
                (let ((kill-buffer-hook nil)
                      (kill-buffer-query-functions nil))
                  (set-buffer-modified-p nil)
                  (kill-buffer buffer)))
            (t (push (json-navigator421-test-condition condition) cleanup-errors)))))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (condition-case condition (cancel-timer timer)
            (t (push (json-navigator421-test-condition condition) cleanup-errors)))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case condition (delete-frame frame t)
            (t (push (json-navigator421-test-condition condition) cleanup-errors)))))
      (condition-case condition (set-window-configuration window-before)
        (t (push (json-navigator421-test-condition condition) cleanup-errors)))
      (dolist (entry parked)
        (condition-case condition
            (progn
              (unless (buffer-live-p (car entry))
                (error "Parked hierarchy buffer died: %S" (cdr entry)))
              (with-current-buffer (car entry) (rename-buffer (cdr entry) t)))
          (t (push (json-navigator421-test-condition condition) cleanup-errors))))
      (when (buffer-live-p buffer-before) (set-buffer buffer-before))
      (when json-navigator421-test-root-owned
        (condition-case condition (delete-directory root t)
          (t (push (json-navigator421-test-condition condition) cleanup-errors))))
      (condition-case condition
          (setq source-after (json-navigator421-test-source-state))
        (t (push (json-navigator421-test-condition condition) cleanup-errors))))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :fixture-accounted (equal fixture-before fixture-after)
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-remove (lambda (buffer) (memq buffer buffers-before))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process) (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     (append timer-list timer-idle-list)))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :window-restored
                 (and (eq (selected-window) selected-window-before)
                      (equal (json-navigator421-test-window-state)
                             window-state-before))
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "JSON Navigator workflow failed: %S" (list result cleanup))
        (list :source source-before :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(JSON_NAVIGATOR_MELPA_PIN, "json-navigator.el")
        .expect("prepare exact JSON Navigator source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_whole_buffer_region_navigation_renders_summary() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_whole_buffer_region_navigation_renders_summary",
        r####"
(json-navigator421-test-run
 (lambda ()
   (with-temp-buffer
     (insert "{\"café\": [1, 2, 3, 4], \"nested\": {\"界\": false}, \"empty\": null}")
     (goto-char 9)
     (set-buffer-modified-p nil)
     (let ((source (current-buffer))
           (source-before (json-navigator421-test-source-buffer-state)))
       (call-interactively #'json-navigator-navigate-region)
       (list :source-before source-before
             :source-after
             (with-current-buffer source
               (json-navigator421-test-source-buffer-state))
             :tree (json-navigator421-test-tree-state))))))
"####,
        expect![[
            r#"OK (:source (:tree "35664192e30b1671dcf5cd6498fdb70ef325363f" :manifest (("json-navigator-pkg.el" . "66a415efe2ffa87a5dd579a8b8133f33005bd156d9ab7cc6fcf5fb75f0eeddc7") ("json-navigator.el" . "18efb2d5b858625275f7377209055e387809778b3cec1b3ac8ac2c01d5ef50eb")) :feature t :version "20241031.630") :result (:source-before (:text "{\"café\": [1, 2, 3, 4], \"nested\": {\"界\": false}, \"empty\": null}" :point 9 :mark nil :mark-active nil :modified nil) :source-after (:text "{\"café\": [1, 2, 3, 4], \"nested\": {\"界\": false}, \"empty\": null}" :point 9 :mark nil :mark-active nil :modified nil) :tree (:name "*hierarchy-tree*" :mode json-navigator-mode :read-only t :modified t :point 1 :text "[+] {\"café\": Array[4], \"nested\": {…}, \"empty\": :json-null}\n")) :cleanup (:source-unchanged t :fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_active_region_uses_only_the_selected_json() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_active_region_uses_only_the_selected_json",
        r####"
(json-navigator421-test-run
 (lambda ()
   (with-temp-buffer
     (insert "prefix {\"chosen\": [\"café\", \"界\"]} suffix")
     (goto-char 8)
     (push-mark 33 t t)
     (setq transient-mark-mode t mark-active t)
     (let ((source (current-buffer))
           (source-before (json-navigator421-test-source-buffer-state)))
       (call-interactively #'json-navigator-navigate-region)
       (list :source-before source-before
             :source-after
             (with-current-buffer source
               (json-navigator421-test-source-buffer-state))
             :tree (json-navigator421-test-tree-state))))))
"####,
        expect![[
            r#"OK (:source (:tree "35664192e30b1671dcf5cd6498fdb70ef325363f" :manifest (("json-navigator-pkg.el" . "66a415efe2ffa87a5dd579a8b8133f33005bd156d9ab7cc6fcf5fb75f0eeddc7") ("json-navigator.el" . "18efb2d5b858625275f7377209055e387809778b3cec1b3ac8ac2c01d5ef50eb")) :feature t :version "20241031.630") :result (:source-before (:text "prefix {\"chosen\": [\"café\", \"界\"]} suffix" :point 8 :mark 33 :mark-active t :modified t) :source-after (:text "prefix {\"chosen\": [\"café\", \"界\"]} suffix" :point 8 :mark 33 :mark-active t :modified t) :tree (:name "*hierarchy-tree*" :mode json-navigator-mode :read-only t :modified t :point 1 :text "[+] {\"chosen\": Array[2]}\n")) :cleanup (:source-unchanged t :fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_after_point_ignores_prefix_and_preserves_source() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_after_point_ignores_prefix_and_preserves_source",
        r####"
(json-navigator421-test-run
 (lambda ()
   (with-temp-buffer
     (insert "ignored café\n[true, false, null, {\"界\": 7}] trailing")
     (goto-char 14)
     (set-buffer-modified-p nil)
     (let ((source (current-buffer))
           (source-before (json-navigator421-test-source-buffer-state)))
       (call-interactively #'json-navigator-navigate-after-point)
       (list :source-before source-before
             :source-after
             (with-current-buffer source
               (json-navigator421-test-source-buffer-state))
             :tree (json-navigator421-test-tree-state))))))
"####,
        expect![[
            r#"OK (:source (:tree "35664192e30b1671dcf5cd6498fdb70ef325363f" :manifest (("json-navigator-pkg.el" . "66a415efe2ffa87a5dd579a8b8133f33005bd156d9ab7cc6fcf5fb75f0eeddc7") ("json-navigator.el" . "18efb2d5b858625275f7377209055e387809778b3cec1b3ac8ac2c01d5ef50eb")) :feature t :version "20241031.630") :result (:source-before (:text "ignored café\n[true, false, null, {\"界\": 7}] trailing" :point 14 :mark nil :mark-active nil :modified nil) :source-after (:text "ignored café\n[true, false, null, {\"界\": 7}] trailing" :point 14 :mark nil :mark-active nil :modified nil) :tree (:name "*hierarchy-tree*" :mode json-navigator-mode :read-only t :modified t :point 1 :text "[+] [t, :json-false, :json-null, …]\n")) :cleanup (:source-unchanged t :fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_widget_navigation_expands_and_collapses_real_nodes() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_widget_navigation_expands_and_collapses_real_nodes",
        r####"
(json-navigator421-test-run
 (lambda ()
   (with-temp-buffer
     (insert "{\"a\":[1,2],\"b\":{\"c\":3}}")
     (call-interactively #'json-navigator-navigate-region))
   (with-current-buffer (window-buffer (selected-window))
     (let (collapsed root-open pair-point pair-open array-point array-open)
       (setq collapsed (json-navigator421-test-tree-state))
       (goto-char (point-min))
       (let ((bindings
              (mapcar (lambda (key) (list key (key-binding (kbd key))))
                      '("TAB" "<backtab>" "RET"))))
       (call-interactively (key-binding (kbd "RET")))
       (setq root-open (json-navigator421-test-tree-state))
       (call-interactively (key-binding (kbd "TAB")))
       (setq pair-point (point))
       (call-interactively (key-binding (kbd "RET")))
       (setq pair-open (json-navigator421-test-tree-state))
       (call-interactively (key-binding (kbd "TAB")))
       (setq array-point (point))
       (call-interactively (key-binding (kbd "RET")))
       (setq array-open (json-navigator421-test-tree-state))
       (call-interactively (key-binding (kbd "<backtab>")))
       (let ((backward-point (point)))
       (goto-char (point-min))
       (call-interactively (key-binding (kbd "RET")))
       (list :bindings bindings
             :collapsed collapsed
             :root-open root-open
             :pair-point pair-point
             :pair-open pair-open
             :array-point array-point
             :array-open array-open
             :backward-point backward-point
             :root-collapsed (json-navigator421-test-tree-state))))))))
"####,
        expect![[
            r#"OK (:source (:tree "35664192e30b1671dcf5cd6498fdb70ef325363f" :manifest (("json-navigator-pkg.el" . "66a415efe2ffa87a5dd579a8b8133f33005bd156d9ab7cc6fcf5fb75f0eeddc7") ("json-navigator.el" . "18efb2d5b858625275f7377209055e387809778b3cec1b3ac8ac2c01d5ef50eb")) :feature t :version "20241031.630") :result (:bindings (("TAB" widget-forward) ("<backtab>" widget-backward) ("RET" widget-button-press)) :collapsed (:name "*hierarchy-tree*" :mode json-navigator-mode :read-only t :modified t :point 1 :text "[+] {\"a\": Array[2], \"b\": {…}}\n") :root-open (:name "*hierarchy-tree*" :mode json-navigator-mode :read-only t :modified t :point 1 :text "[-] {\"a\": Array[2], \"b\": {…}}\n |-[+] a\n `-[+] b\n") :pair-point 34 :pair-open (:name "*hierarchy-tree*" :mode json-navigator-mode :read-only t :modified t :point 34 :text "[-] {\"a\": Array[2], \"b\": {…}}\n |-[-] a\n |  `-[+] [1, 2]\n `-[+] b\n") :array-point 46 :array-open (:name "*hierarchy-tree*" :mode json-navigator-mode :read-only t :modified t :point 46 :text "[-] {\"a\": Array[2], \"b\": {…}}\n |-[-] a\n |  `-[-] [1, 2]\n |     |-[+] 1\n |     `-[+] 2\n `-[+] b\n") :backward-point 34 :root-collapsed (:name "*hierarchy-tree*" :mode json-navigator-mode :read-only t :modified t :point 1 :text "[+] {\"a\": Array[2], \"b\": {…}}\n")) :cleanup (:source-unchanged t :fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn invalid_json_is_atomic_then_public_recovery_succeeds() -> ParityBatchCase {
    ParityBatchCase::value(
        "invalid_json_is_atomic_then_public_recovery_succeeds",
        r####"
(json-navigator421-test-run
 (lambda ()
   (with-temp-buffer
     (insert "{\"broken\": [1, }")
     (goto-char (point-min))
     (set-buffer-modified-p nil)
     (let* ((source (current-buffer))
            (source-before (json-navigator421-test-source-buffer-state))
            (window-buffer-before (window-buffer (selected-window)))
            (failure
             (condition-case condition
                 (progn
                   (call-interactively #'json-navigator-navigate-region)
                   :no-error)
               (t (json-navigator421-test-condition condition))))
            (failure-state
             (list :source
                   (with-current-buffer source
                     (json-navigator421-test-source-buffer-state))
                   :window-unchanged
                   (eq (window-buffer (selected-window)) window-buffer-before)
                   :hierarchy-buffer
                   (and (get-buffer "*hierarchy-tree*") t))))
       (erase-buffer)
       (insert "{\"recovered\": [1, 2, 3, 4], \"ok\": true}")
       (goto-char (point-min))
       (set-buffer-modified-p nil)
       (let ((json-navigator-display-length 2))
         (call-interactively #'json-navigator-navigate-region))
       (list :source-before source-before
             :failure failure
             :failure-state failure-state
             :recovery-source
             (with-current-buffer source
               (json-navigator421-test-source-buffer-state))
             :recovery-tree (json-navigator421-test-tree-state))))))
"####,
        expect![[
            r#"OK (:source (:tree "35664192e30b1671dcf5cd6498fdb70ef325363f" :manifest (("json-navigator-pkg.el" . "66a415efe2ffa87a5dd579a8b8133f33005bd156d9ab7cc6fcf5fb75f0eeddc7") ("json-navigator.el" . "18efb2d5b858625275f7377209055e387809778b3cec1b3ac8ac2c01d5ef50eb")) :feature t :version "20241031.630") :result (:source-before (:text "{\"broken\": [1, }" :point 1 :mark nil :mark-active nil :modified nil) :failure (:type json-readtable-error :data (125) :message "JSON readtable error: 125") :failure-state (:source (:text "{\"broken\": [1, }" :point 1 :mark nil :mark-active nil :modified nil) :window-unchanged t :hierarchy-buffer nil) :recovery-source (:text "{\"recovered\": [1, 2, 3, 4], \"ok\": true}" :point 1 :mark nil :mark-active nil :modified nil) :recovery-tree (:name "*hierarchy-tree*" :mode json-navigator-mode :read-only t :modified t :point 1 :text "[+] {\"recovered\": Array[4], \"ok\": t}\n")) :cleanup (:source-unchanged t :fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn json_navigator_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_whole_buffer_region_navigation_renders_summary(),
        public_active_region_uses_only_the_selected_json(),
        public_after_point_ignores_prefix_and_preserves_source(),
        public_widget_navigation_expands_and_collapses_real_nodes(),
        invalid_json_is_atomic_then_public_recovery_succeeds(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "json-navigator-rank421",
        "json_navigator_parity",
        &cases,
    );
}
