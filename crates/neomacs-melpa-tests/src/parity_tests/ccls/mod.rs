//! Practical parity for ccls's public LSP extension workflows.
//!
//! A closed LSP/process boundary supplies recorded ccls responses while the
//! package's commands, notification handlers, overlays, code lenses, tree UI,
//! file buffers, and cleanup remain real.

use std::time::Duration;

use expect_test::expect;

use crate::{CCLS_MELPA_PIN, CachedMelpaOracle, DASH_MELPA_PIN, LSP_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'ccls)
(get-buffer-create " *code-conversion-work*")

(defconst ccls400-test-source-manifest
  '(("ccls-call-hierarchy.el" . "ccb825f1eb76fac90a9dcaa0ec135660bd3f5a0cae424acca3d9709226eb7e33")
    ("ccls-code-lens.el" . "02c781347a5007ad4f543b7e856f3329dcab2baaefd50134d20d073b944d877b")
    ("ccls-common.el" . "6c390edd5c872122f94d3fc34578fc92259fd0f7d9e2ce4af591dc5bb6e20aab")
    ("ccls-inheritance-hierarchy.el" . "1bdf820a38f8456ff7270cc10ad26538aa8541e6f667ee50734f6570077e3a56")
    ("ccls-member-hierarchy.el" . "95206a81aea3e267cd84c87bc9fba3aac0c8b7dc6380d7f2dc8592696d6b2c1b")
    ("ccls-semantic-highlight.el" . "bbe7a0202dd0f6ef06dd909575659fbfd5dcf49111720e02403cf78a9c4460de")
    ("ccls-tree.el" . "e4bcfd5513d871c4c720b745a85e49736642476f2223790b8f8d046ad475fd71")
    ("ccls.el" . "175f146b94afcdfce966aa247304b77bdba41b581ef3dfe934b11055663eac29")))

(defun ccls400-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(let* ((main (symbol-file 'ccls-info 'defun))
       (directory (and main (file-name-directory main)))
       (payload
        (and directory
             (sort (seq-filter
                    (lambda (name)
                      (and (string-prefix-p "ccls" name)
                           (string-suffix-p ".el" name)
                           (not (string-suffix-p "-autoloads.el" name))
                           (not (string-suffix-p "-pkg.el" name))))
                    (directory-files directory nil nil t))
                   #'string<))))
  (unless (and (file-regular-p main)
               (equal payload (mapcar #'car ccls400-test-source-manifest))
               (cl-every
                (lambda (entry)
                  (let ((file (expand-file-name (car entry) directory)))
                    (and (file-regular-p file)
                         (not (file-symlink-p file))
                         (equal (ccls400-test-file-sha256 file) (cdr entry)))))
                ccls400-test-source-manifest))
    (error "Unexpected installed ccls payload: %S" (list main payload))))

(defvar ccls400-test-root nil)

(defun ccls400-test-normalize (value root)
  (cond
   ((stringp value)
    (if root
        (replace-regexp-in-string
         (regexp-quote (directory-file-name root)) "[ROOT]" value t t)
      (copy-sequence value)))
   ((consp value)
    (cons (ccls400-test-normalize (car value) root)
          (ccls400-test-normalize (cdr value) root)))
   ((vectorp value)
    (apply #'vector (mapcar (lambda (item)
                              (ccls400-test-normalize item root))
                            value)))
   (t value)))

(defun ccls400-test-condition (condition root)
  (list :error (car condition)
        :data (ccls400-test-normalize (copy-tree (cdr condition)) root)))

(defun ccls400-test-write-file (root relative content)
  (let ((file (expand-file-name relative root)))
    (unless (file-in-directory-p file root)
      (error "Refusing ccls fixture outside root: %s" file))
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix))
      (with-temp-file file (insert content)))
    file))

(defun ccls400-test-manifest (root)
  (let (entries)
    (dolist (file (directory-files-recursively root "." nil nil t))
      (when (file-regular-p file)
        (push (cons (file-relative-name file root)
                    (ccls400-test-file-sha256 file))
              entries)))
    (sort entries (lambda (left right) (string< (car left) (car right))))))

(defun ccls400-test-window-state ()
  (mapcar (lambda (window)
            (list (buffer-name (window-buffer window))
                  (window-point window)
                  (window-start window)
                  (window-dedicated-p window)))
          (seq-mapcat (lambda (frame) (window-list frame 'nomini))
                      (frame-list))))

(defun ccls400-test-park-buffer (name)
  (when-let ((buffer (get-buffer name)))
    (let ((parked (generate-new-buffer-name (concat " *parked " name "*"))))
      (with-current-buffer buffer (rename-buffer parked t))
      (cons buffer name))))

(defun ccls400-test-overlay-state (property)
  (mapcar
   (lambda (overlay)
     (list :start (overlay-start overlay)
           :end (overlay-end overlay)
           :text (buffer-substring-no-properties
                  (overlay-start overlay) (overlay-end overlay))
           :face (overlay-get overlay 'face)
           :owned (and (overlay-get overlay property) t)))
   (sort (seq-filter (lambda (overlay) (overlay-get overlay property))
                     (copy-sequence (overlays-in (point-min) (point-max))))
         (lambda (left right) (< (overlay-start left) (overlay-start right))))))

(defun ccls400-test-lens-state ()
  (mapcar
   (lambda (overlay)
     (let* ((display (or (overlay-get overlay 'display)
                         (overlay-get overlay 'after-string)))
            (map (and (stringp display) (get-text-property 0 'local-map display))))
       (list :start (overlay-start overlay)
             :end (overlay-end overlay)
             :display (and display (substring-no-properties display))
             :face (and (stringp display) (get-text-property 0 'face display))
             :mouse-face (and (stringp display)
                              (get-text-property 0 'mouse-face display))
             :mouse-command (and (keymapp map)
                                 (commandp (lookup-key map [mouse-1]))))))
   (sort (seq-filter (lambda (overlay) (overlay-get overlay 'ccls-code-lens))
                     (copy-sequence (overlays-in (point-min) (point-max))))
         (lambda (left right) (< (overlay-start left) (overlay-start right))))))

(defun ccls400-test-forbid-external (kind &rest arguments)
  (error "Unexpected external ccls boundary: %S" (cons kind arguments)))

(defun ccls400-test-run (files body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name "ccls/" sandbox))))
         (window-before (current-window-configuration))
         (window-state-before (ccls400-test-window-state))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (ccls-executable "ccls")
         (ccls-args nil)
         (ccls-initialization-options nil)
         (ccls-root-files '(".ccls-root"))
         (ccls-sem-highlight-method nil)
         (ccls-enable-skipped-ranges t)
         (ccls-code-lens-position 'end)
         (ccls-tree-initial-levels 2)
         (ccls-member-hierarchy-qualified nil)
         (message-log-max nil)
         (print-circle nil)
         (root-owned nil)
         (parked nil)
         fixture-before fixture-after result body-error cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute ccls sandbox root"))
              (when (file-exists-p root)
                (error "ccls sandbox root already exists: %s" root))
              (dolist (name '("*ccls400 preprocess*"
                              "*ccls-tree member hierarchy*"
                              "*Help*"))
                (when-let ((entry (ccls400-test-park-buffer name)))
                  (push entry parked)))
              (make-directory root t)
              (setq root-owned t)
              (dolist (entry files)
                (ccls400-test-write-file root (car entry) (cdr entry)))
              (setq fixture-before (ccls400-test-manifest root))
              (let ((ccls400-test-root root))
                (setq result
                      (cl-letf (((symbol-function 'call-process)
                                 (lambda (&rest arguments)
                                   (apply #'ccls400-test-forbid-external
                                          'call-process arguments)))
                                ((symbol-function 'call-process-region)
                                 (lambda (&rest arguments)
                                   (apply #'ccls400-test-forbid-external
                                          'call-process-region arguments)))
                                ((symbol-function 'process-file)
                                 (lambda (&rest arguments)
                                   (apply #'ccls400-test-forbid-external
                                          'process-file arguments)))
                                ((symbol-function 'start-process)
                                 (lambda (&rest arguments)
                                   (apply #'ccls400-test-forbid-external
                                          'start-process arguments)))
                                ((symbol-function 'start-file-process)
                                 (lambda (&rest arguments)
                                   (apply #'ccls400-test-forbid-external
                                          'start-file-process arguments)))
                                ((symbol-function 'make-process)
                                 (lambda (&rest arguments)
                                   (apply #'ccls400-test-forbid-external
                                          'make-process arguments)))
                                ((symbol-function 'make-network-process)
                                 (lambda (&rest arguments)
                                   (apply #'ccls400-test-forbid-external
                                          'make-network-process arguments)))
                                ((symbol-function 'open-network-stream)
                                 (lambda (&rest arguments)
                                   (apply #'ccls400-test-forbid-external
                                          'open-network-stream arguments)))
                                ((symbol-function 'url-retrieve-synchronously)
                                 (lambda (&rest arguments)
                                   (apply #'ccls400-test-forbid-external
                                          'url-retrieve-synchronously arguments))))
                        (funcall body root))))
              (setq fixture-after (ccls400-test-manifest root))
              (unless (equal fixture-before fixture-after)
                (error "ccls fixture changed: %S -> %S"
                       fixture-before fixture-after)))
          (error (setq body-error (ccls400-test-condition condition root))))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case condition (delete-process process)
            (error (push (ccls400-test-condition condition root) cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case condition
              (progn
                (with-current-buffer buffer (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error (push (ccls400-test-condition condition root) cleanup-errors)))))
      (dolist (timer (copy-sequence timer-list))
        (unless (memq timer timers-before)
          (condition-case condition (cancel-timer timer)
            (error (push (ccls400-test-condition condition root) cleanup-errors)))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case condition (delete-frame frame t)
            (error (push (ccls400-test-condition condition root) cleanup-errors)))))
      (condition-case condition (set-window-configuration window-before)
        (error (push (ccls400-test-condition condition root) cleanup-errors)))
      (dolist (entry parked)
        (condition-case condition
            (if (buffer-live-p (car entry))
                (with-current-buffer (car entry) (rename-buffer (cdr entry) t))
              (error "Parked ccls buffer died: %S" entry))
          (error (push (ccls400-test-condition condition root) cleanup-errors))))
      (when (buffer-live-p buffer-before) (set-buffer buffer-before))
      (when root-owned
        (condition-case condition (delete-directory root t)
          (error (push (ccls400-test-condition condition root) cleanup-errors)))))
    (let ((cleanup
           (list :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter (lambda (buffer)
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
                 :fixture-restored (equal fixture-before fixture-after)
                 :window-restored
                 (equal window-state-before (ccls400-test-window-state))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "ccls workflow failed: %S" (list result cleanup))
        (ccls400-test-normalize
         (list :source (copy-tree ccls400-test-source-manifest)
               :result result
               :cleanup cleanup)
         root)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CCLS_MELPA_PIN, "ccls.el")
        .expect("prepare exact shallow ccls source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare exact shallow Dash dependency below ./tmp")
        .with_melpa_dependency(LSP_MODE_MELPA_PIN)
        .expect("prepare exact shallow LSP Mode dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_extension_commands_preserve_exact_lsp_methods_and_root() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_extension_commands_preserve_exact_lsp_methods_and_root",
        r####"
(ccls400-test-run
 '(("project/.ccls-root" . "owned\n")
   ("project/src/main.cpp" . "int main() { return 0; }\n"))
 (lambda (root)
   (let* ((file (expand-file-name "project/src/main.cpp" root))
          (buffer (find-file-noselect file))
          calls info file-info reload navigate suggested client-state)
     (with-current-buffer buffer
       (c++-mode)
       (setq default-directory (file-name-directory file))
       (cl-letf (((symbol-function 'lsp-request)
                  (lambda (method params)
                    (push (list :request method
                                (if (hash-table-p params)
                                    (list :empty-hash (= 0 (hash-table-count params)))
                                  (copy-tree params)))
                          calls)
                    (if (equal method "$ccls/info")
                        '(:version "ccls 0.2026" :index "ready")
                      (lsp-ht ("path" "src/main.cpp")
                              ("args" ["clang++" "-std=c++20"])))))
                 ((symbol-function 'lsp--text-document-position-params)
                  (lambda () '(:textDocument (:uri "file://owned/main.cpp")
                               :position (:line 0 :character 4))))
                 ((symbol-function 'lsp-notify)
                  (lambda (method params)
                    (push (list :notify method (copy-tree params)) calls)
                    :notified))
                 ((symbol-function 'lsp-find-custom)
                  (lambda (method params)
                    (push (list :find method (copy-tree params)) calls)
                    :navigated)))
         (setq info (ccls-info))
         (let ((raw (ccls-file-info '(:include "all"))))
           (setq file-info
                 (list :path (gethash "path" raw)
                       :args (gethash "args" raw))))
         (setq
               reload (call-interactively #'ccls-reload)
               navigate (ccls-navigate "R")
               suggested (lsp--suggest-project-root))
         (let ((client (gethash 'ccls lsp-clients)))
           (setq client-state
                 (list :server-id (lsp--client-server-id client)
                       :multi-root (and (lsp--client-multi-root client) t)
                       :activation
                       (and (funcall (lsp--client-activation-fn client)
                                     file 'c++-mode)
                            t)
                       :initialization
                       (funcall (lsp--client-initialization-options client))
                       :library-folders
                       (funcall (lsp--client-library-folders-fn client))))))
     (list :info info
           :file-info file-info
           :reload reload
           :navigate navigate
           :suggested suggested
           :client client-state
           :calls (nreverse calls))))))
"####,
        expect![[
            r#"OK (:source (("ccls-call-hierarchy.el" . "ccb825f1eb76fac90a9dcaa0ec135660bd3f5a0cae424acca3d9709226eb7e33") ("ccls-code-lens.el" . "02c781347a5007ad4f543b7e856f3329dcab2baaefd50134d20d073b944d877b") ("ccls-common.el" . "6c390edd5c872122f94d3fc34578fc92259fd0f7d9e2ce4af591dc5bb6e20aab") ("ccls-inheritance-hierarchy.el" . "1bdf820a38f8456ff7270cc10ad26538aa8541e6f667ee50734f6570077e3a56") ("ccls-member-hierarchy.el" . "95206a81aea3e267cd84c87bc9fba3aac0c8b7dc6380d7f2dc8592696d6b2c1b") ("ccls-semantic-highlight.el" . "bbe7a0202dd0f6ef06dd909575659fbfd5dcf49111720e02403cf78a9c4460de") ("ccls-tree.el" . "e4bcfd5513d871c4c720b745a85e49736642476f2223790b8f8d046ad475fd71") ("ccls.el" . "175f146b94afcdfce966aa247304b77bdba41b581ef3dfe934b11055663eac29")) :result (:info (:version "ccls 0.2026" :index "ready") :file-info (:path "src/main.cpp" :args ["clang++" "-std=c++20"]) :reload :notified :navigate :navigated :suggested "[ROOT]/project/" :client (:server-id ccls :multi-root nil :activation t :initialization nil :library-folders nil) :calls ((:request "$ccls/info" (:empty-hash t)) (:request "$ccls/fileInfo" (:textDocument (:uri "file://owned/main.cpp") :position (:line 0 :character 4) :include "all")) (:notify "$ccls/reload" (:whitelist [] :blacklist [])) (:find "$ccls/navigate" (:direction "R")))) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_preprocess_builds_read_only_cxx_buffer_from_exact_process_call() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_preprocess_builds_read_only_cxx_buffer_from_exact_process_call",
        r####"
(ccls400-test-run
 '(("src/space 界.cpp" . "int café = 1;\n"))
 (lambda (root)
   (let* ((file (expand-file-name "src/space 界.cpp" root))
          (source (find-file-noselect file))
          output
          calls)
     (with-current-buffer source
       (c++-mode)
       (setq default-directory (file-name-directory file))
       (setq output (get-buffer-create "*ccls400 preprocess*"))
       (cl-letf (((symbol-function 'lsp--cur-workspace-check) (lambda () t))
                 ((symbol-function 'lsp-request)
                  (lambda (method params)
                    (push (list :request method
                                (if (hash-table-p params)
                                    (list :empty-hash (= 0 (hash-table-count params)))
                                  (copy-tree params)))
                          calls)
                    (lsp-ht
                     ("path" "src/space 界.cpp")
                     ("args" ["clang++" "-I" "include dir" "-o" "ignored.o"
                              "-DNAME=界" "src/space 界.cpp"]))))
                 ((symbol-function 'lsp--text-document-position-params)
                  (lambda () '(:textDocument (:uri "file://owned/space.cpp")
                               :position (:line 0 :character 4))))
                 ((symbol-function 'process-file)
                  (lambda (program infile destination display &rest args)
                    (unless (and (equal program "clang++")
                                 (null infile) (eq destination t) (null display))
                      (error "Unexpected preprocess call: %S"
                             (list program infile destination display args)))
                    (unless (and (equal args
                                        '("-E" "-I" "include dir" "-DNAME=界"
                                          "src/space 界.cpp"))
                                 (equal default-directory
                                        (file-name-directory file)))
                      (error "Unexpected preprocess arguments: %S"
                             (list args default-directory)))
                    (push (list :process program (copy-tree args)
                                :directory default-directory)
                          calls)
                    (insert "\n#line 1 \"space 界.cpp\"\nint café = 1;\n")
                    0)))
         (ccls-preprocess-file output)))
     (with-current-buffer output
       (list :calls (nreverse calls)
             :text (buffer-substring-no-properties (point-min) (point-max))
             :mode major-mode
             :read-only buffer-read-only
             :modified (buffer-modified-p)
             :selected (eq (window-buffer (selected-window)) output))))))
"####,
        expect![[
            r#"OK (:source (("ccls-call-hierarchy.el" . "ccb825f1eb76fac90a9dcaa0ec135660bd3f5a0cae424acca3d9709226eb7e33") ("ccls-code-lens.el" . "02c781347a5007ad4f543b7e856f3329dcab2baaefd50134d20d073b944d877b") ("ccls-common.el" . "6c390edd5c872122f94d3fc34578fc92259fd0f7d9e2ce4af591dc5bb6e20aab") ("ccls-inheritance-hierarchy.el" . "1bdf820a38f8456ff7270cc10ad26538aa8541e6f667ee50734f6570077e3a56") ("ccls-member-hierarchy.el" . "95206a81aea3e267cd84c87bc9fba3aac0c8b7dc6380d7f2dc8592696d6b2c1b") ("ccls-semantic-highlight.el" . "bbe7a0202dd0f6ef06dd909575659fbfd5dcf49111720e02403cf78a9c4460de") ("ccls-tree.el" . "e4bcfd5513d871c4c720b745a85e49736642476f2223790b8f8d046ad475fd71") ("ccls.el" . "175f146b94afcdfce966aa247304b77bdba41b581ef3dfe934b11055663eac29")) :result (:calls ((:request "$ccls/fileInfo" (:textDocument (:uri "file://owned/space.cpp") :position (:line 0 :character 4))) (:process "clang++" ("-E" "-I" "include dir" "-DNAME=界" "src/space 界.cpp") :directory "[ROOT]/src/")) :text "// Generated by: clang++ -I \"include dir\" -DNAME=界 \"src/space 界.cpp\"\n#line 1 \"space 界.cpp\"\nint café = 1;\n" :mode c++-mode :read-only t :modified nil :selected t) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn registered_notifications_replace_semantic_and_skipped_range_overlays() -> ParityBatchCase {
    ParityBatchCase::value(
        "registered_notifications_replace_semantic_and_skipped_range_overlays",
        r####"
(ccls400-test-run
 '(("src/highlight.cpp" . "int global界 = 1;\n#if 0\nint hidden;\n#endif\n"))
 (lambda (root)
   (let* ((file (expand-file-name "src/highlight.cpp" root))
          (uri (lsp--path-to-uri file))
          (buffer (find-file-noselect file))
          (client (gethash 'ccls lsp-clients))
          (handlers (and client (lsp--client-notification-handlers client)))
          (semantic (and handlers
                         (gethash "$ccls/publishSemanticHighlight" handlers)))
          (skipped (and handlers
                        (gethash "$ccls/publishSkippedRanges" handlers))))
     (unless (and (functionp semantic) (functionp skipped))
       (error "Missing registered ccls notification handlers"))
     (with-current-buffer buffer
       (c++-mode)
       (let ((ccls-sem-highlight-method 'overlay)
             (ccls-sem-face-function
              (lambda (_symbol) '(font-lock-variable-name-face bold))))
         (funcall semantic nil
                  (lsp-ht
                   ("uri" uri)
                   ("symbols"
                    (vector
                     (lsp-ht ("id" 7) ("parentKind" 1) ("kind" 13)
                             ("storage" 2)
                             ("ranges" (vector (lsp-ht ("L" 4) ("R" 11)))))))))
         (funcall skipped nil
                  (lsp-ht
                   ("uri" uri)
                   ("skippedRanges"
                    (vector
                     (lsp-ht
                      ("start" (lsp-ht ("line" 1) ("character" 0)))
                      ("end" (lsp-ht ("line" 3) ("character" 6))))))))
         (let ((semantic-before (ccls400-test-overlay-state 'ccls-sem-highlight))
               (skipped-before (ccls400-test-overlay-state 'ccls-inactive))
               (semantic-objects (copy-sequence ccls--sem-overlays))
               (skipped-objects (copy-sequence ccls--skipped-ranges-overlays)))
           (funcall semantic nil (lsp-ht ("uri" uri) ("symbols" [])))
           (funcall skipped nil (lsp-ht ("uri" uri) ("skippedRanges" [])))
           (list :handlers
                 (list (eq semantic #'ccls--publish-semantic-highlight)
                       (eq skipped #'ccls--publish-skipped-ranges))
                 :semantic semantic-before
                 :skipped skipped-before
                 :old-deleted
                 (and (cl-every (lambda (overlay) (null (overlay-buffer overlay)))
                                semantic-objects)
                      (cl-every (lambda (overlay) (null (overlay-buffer overlay)))
                                skipped-objects))
                 :remaining
                 (list (ccls400-test-overlay-state 'ccls-sem-highlight)
                       (ccls400-test-overlay-state 'ccls-inactive)))))))))
"####,
        expect![[
            r##"OK (:source (("ccls-call-hierarchy.el" . "ccb825f1eb76fac90a9dcaa0ec135660bd3f5a0cae424acca3d9709226eb7e33") ("ccls-code-lens.el" . "02c781347a5007ad4f543b7e856f3329dcab2baaefd50134d20d073b944d877b") ("ccls-common.el" . "6c390edd5c872122f94d3fc34578fc92259fd0f7d9e2ce4af591dc5bb6e20aab") ("ccls-inheritance-hierarchy.el" . "1bdf820a38f8456ff7270cc10ad26538aa8541e6f667ee50734f6570077e3a56") ("ccls-member-hierarchy.el" . "95206a81aea3e267cd84c87bc9fba3aac0c8b7dc6380d7f2dc8592696d6b2c1b") ("ccls-semantic-highlight.el" . "bbe7a0202dd0f6ef06dd909575659fbfd5dcf49111720e02403cf78a9c4460de") ("ccls-tree.el" . "e4bcfd5513d871c4c720b745a85e49736642476f2223790b8f8d046ad475fd71") ("ccls.el" . "175f146b94afcdfce966aa247304b77bdba41b581ef3dfe934b11055663eac29")) :result (:handlers (t t) :semantic ((:start 5 :end 12 :text "global界" :face (font-lock-variable-name-face bold) :owned t)) :skipped ((:start 18 :end 42 :text "#if 0\nint hidden;\n#endif" :face ccls-skipped-range-face :owned t)) :old-deleted t :remaining (nil nil)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn public_code_lens_request_renders_commands_and_clear_removes_them() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_code_lens_request_renders_commands_and_clear_removes_them",
        r####"
(ccls400-test-run
 '(("src/lens.cpp" . "int alpha();\nint beta();\n"))
 (lambda (root)
   (let* ((file (expand-file-name "src/lens.cpp" root))
          (buffer (find-file-noselect file))
          request callback-state)
     (with-current-buffer buffer
       (c++-mode)
       (cl-letf (((symbol-function 'lsp--cur-workspace-check) (lambda () t))
                 ((symbol-function 'lsp--send-request-async)
                  (lambda (body callback &rest arguments)
                    (unless (and (equal (plist-get body :jsonrpc) "2.0")
                                 (equal (plist-get body :method)
                                        "textDocument/codeLens")
                                 (equal (plist-get
                                         (plist-get (plist-get body :params)
                                                    :textDocument)
                                         :uri)
                                        (concat lsp--uri-file-prefix file))
                                 (null arguments))
                      (error "Unexpected ccls code-lens request: %S"
                             (list body arguments)))
                    (setq request (list body (copy-tree arguments)))
                    (funcall
                     callback
                     (vector
                      (lsp-ht
                       ("range"
                        (lsp-ht
                         ("start" (lsp-ht ("line" 0) ("character" 4)))
                         ("end" (lsp-ht ("line" 0) ("character" 9)))))
                       ("command"
                        (lsp-ht ("title" "3 refs") ("command" "ccls.xref")
                                ("arguments" ["alpha界"]))))
                      (lsp-ht
                       ("range"
                        (lsp-ht
                         ("start" (lsp-ht ("line" 1) ("character" 4)))
                         ("end" (lsp-ht ("line" 1) ("character" 8)))))
                       ("command"
                        (lsp-ht ("title" "1 caller") ("command" "ccls.call")
                                ("arguments" ["beta"]))))))
                    :queued)))
         (setq callback-state (ccls-request-code-lens)))
       (let ((before (ccls400-test-lens-state)))
         (call-interactively #'ccls-clear-code-lens)
         (list :return callback-state
               :request request
               :before before
               :after (ccls400-test-lens-state)))))))
"####,
        expect![[
            r#"OK (:source (("ccls-call-hierarchy.el" . "ccb825f1eb76fac90a9dcaa0ec135660bd3f5a0cae424acca3d9709226eb7e33") ("ccls-code-lens.el" . "02c781347a5007ad4f543b7e856f3329dcab2baaefd50134d20d073b944d877b") ("ccls-common.el" . "6c390edd5c872122f94d3fc34578fc92259fd0f7d9e2ce4af591dc5bb6e20aab") ("ccls-inheritance-hierarchy.el" . "1bdf820a38f8456ff7270cc10ad26538aa8541e6f667ee50734f6570077e3a56") ("ccls-member-hierarchy.el" . "95206a81aea3e267cd84c87bc9fba3aac0c8b7dc6380d7f2dc8592696d6b2c1b") ("ccls-semantic-highlight.el" . "bbe7a0202dd0f6ef06dd909575659fbfd5dcf49111720e02403cf78a9c4460de") ("ccls-tree.el" . "e4bcfd5513d871c4c720b745a85e49736642476f2223790b8f8d046ad475fd71") ("ccls.el" . "175f146b94afcdfce966aa247304b77bdba41b581ef3dfe934b11055663eac29")) :result (:return :queued :request ((:jsonrpc "2.0" :method "textDocument/codeLens" :params (:textDocument (:uri "file://[ROOT]/src/lens.cpp"))) nil) :before ((:start 13 :end 14 :display " 3 refs\n" :face ccls-code-lens-face :mouse-face ccls-code-lens-mouse-face :mouse-command t) (:start 25 :end 26 :display " 1 caller\n" :face ccls-code-lens-face :mouse-face ccls-code-lens-mouse-face :mouse-command t)) :after nil) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_member_hierarchy_reports_empty_failure_then_recovers_and_quits() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_member_hierarchy_reports_empty_failure_then_recovers_and_quits",
        r####"
(ccls400-test-run
 '(("src/widget.cpp" . "struct Widget {\n  int count界;\n  void render();\n};\n"))
 (lambda (root)
   (let* ((file (expand-file-name "src/widget.cpp" root))
          (source (find-file-noselect file))
          (uri (lsp--path-to-uri file))
          (response
           (lsp-ht
            ("id" 1) ("name" "Widget") ("fieldName" "") ("numChildren" 2)
            ("location"
             (lsp-ht
              ("uri" uri)
              ("range"
               (lsp-ht
                ("start" (lsp-ht ("line" 0) ("character" 7)))
                ("end" (lsp-ht ("line" 0) ("character" 13)))))))
            ("children"
             (vector
              (lsp-ht
               ("id" 2) ("name" "int Widget::count界")
               ("fieldName" "count界") ("numChildren" 0)
               ("location"
                (lsp-ht
                 ("uri" uri)
                 ("range"
                  (lsp-ht
                   ("start" (lsp-ht ("line" 1) ("character" 6)))
                   ("end" (lsp-ht ("line" 1) ("character" 12)))))))
               ("children" []))
              (lsp-ht
               ("id" 3) ("name" "void Widget::render")
               ("fieldName" "render") ("numChildren" 0)
               ("location"
                (lsp-ht
                 ("uri" uri)
                 ("range"
                  (lsp-ht
                   ("start" (lsp-ht ("line" 2) ("character" 7)))
                   ("end" (lsp-ht ("line" 2) ("character" 13)))))))
               ("children" []))))))
          (responses (list nil response))
          requests failure tree-state after-quit)
     (switch-to-buffer source)
     (c++-mode)
     (goto-char 8)
     (cl-letf (((symbol-function 'lsp-request)
                (lambda (method params)
                  (push (list method (copy-tree params)) requests)
                  (prog1 (pop responses)
                    (unless responses (setq responses nil)))))
               ((symbol-function 'lsp--cur-position)
                (lambda () (list :line 0 :character 7))))
       (setq failure
             (condition-case condition
                 (list :return (call-interactively #'ccls-member-hierarchy))
               (error (ccls400-test-condition condition root))))
       (call-interactively #'ccls-member-hierarchy))
     (with-current-buffer "*ccls-tree member hierarchy*"
       (setq ccls-tree-calling nil)
       (let ((initial (buffer-substring-no-properties (point-min) (point-max)))
             (header (and (stringp header-line-format)
                          (substring-no-properties header-line-format)))
             (mode-line (and (stringp mode-line-format)
                             (substring-no-properties mode-line-format))))
         (call-interactively #'ccls-tree-next-line)
         (let* ((node (ccls-tree--node-at-point))
                (location (and node (ccls-tree-node-location node))))
           (setq tree-state
                 (list :initial initial
                       :header header
                       :mode-line mode-line
                       :point (point)
                       :depth (ccls-tree--depth-at-point)
                       :field (and node
                                   (ccls-member-hierarchy-node-field-name
                                    (ccls-tree-node-data node)))
                       :location
                       (and location
                            (list (file-relative-name (car location) root)
                                  (lsp:position-line (cdr location))
                                  (lsp:position-character (cdr location)))))))
         (call-interactively #'ccls-tree-quit)))
     (setq after-quit
           (list :selected-file
                 (and (buffer-file-name (window-buffer (selected-window)))
                      (file-relative-name
                       (buffer-file-name (window-buffer (selected-window))) root))
                 :selected-point (window-point (selected-window))))
     (list :failure failure
           :requests (nreverse requests)
           :tree tree-state
           :after-quit after-quit
           :responses-exhausted (null responses)))))
"####,
        expect![[
            r#"OK (:source (("ccls-call-hierarchy.el" . "ccb825f1eb76fac90a9dcaa0ec135660bd3f5a0cae424acca3d9709226eb7e33") ("ccls-code-lens.el" . "02c781347a5007ad4f543b7e856f3329dcab2baaefd50134d20d073b944d877b") ("ccls-common.el" . "6c390edd5c872122f94d3fc34578fc92259fd0f7d9e2ce4af591dc5bb6e20aab") ("ccls-inheritance-hierarchy.el" . "1bdf820a38f8456ff7270cc10ad26538aa8541e6f667ee50734f6570077e3a56") ("ccls-member-hierarchy.el" . "95206a81aea3e267cd84c87bc9fba3aac0c8b7dc6380d7f2dc8592696d6b2c1b") ("ccls-semantic-highlight.el" . "bbe7a0202dd0f6ef06dd909575659fbfd5dcf49111720e02403cf78a9c4460de") ("ccls-tree.el" . "e4bcfd5513d871c4c720b745a85e49736642476f2223790b8f8d046ad475fd71") ("ccls.el" . "175f146b94afcdfce966aa247304b77bdba41b581ef3dfe934b11055663eac29")) :result (:failure (:error user-error :data ("Couldn’t open tree from point")) :requests (("$ccls/member" (:textDocument (:uri "file://[ROOT]/src/widget.cpp") :position (:line 0 :character 7) :levels 1 :qualified :json-false :hierarchy t)) ("$ccls/member" (:textDocument (:uri "file://[ROOT]/src/widget.cpp") :position (:line 0 :character 7) :levels 1 :qualified :json-false :hierarchy t))) :tree (:initial "Members of\nWidget\n├╸count界\n└╸render\n" :header nil :mode-line "Member hierarchy" :point 19 :depth 1 :field "count界" :location ("src/widget.cpp" 1 6)) :after-quit (:selected-file "src/widget.cpp" :selected-point 8) :responses-exhausted t) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn ccls_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_extension_commands_preserve_exact_lsp_methods_and_root(),
        public_preprocess_builds_read_only_cxx_buffer_from_exact_process_call(),
        registered_notifications_replace_semantic_and_skipped_range_overlays(),
        public_code_lens_request_renders_commands_and_clear_removes_them(),
        public_member_hierarchy_reports_empty_failure_then_recovers_and_quits(),
    ];
    assert_oracle_batch_cases(oracle(), "ccls-rank400", "ccls", &cases);
}
