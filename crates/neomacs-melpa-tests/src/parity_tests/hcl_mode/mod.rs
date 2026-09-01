//! Practical parity for hcl-mode's public Hashicorp editing commands.
//!
//! These cases open real `.hcl`/`.nomad` files, indent blocks/maps/arrays,
//! fontify assignments, booleans, interpolation and heredocs, move by
//! defun, and leave comment interiors unindented.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HCL_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'hcl-mode)
(set-window-configuration (current-window-configuration))

(defconst hm436-test-tree
  "12223205cfb8258ec1de9d61699ef2d1abd2e787")
(defconst hm436-test-manifest
  '(("hcl-mode-pkg.el" . "d3b73384b3501939f13742dff41f7a5fc0306b228f2012e32914cdc9f5f474e5")
    ("hcl-mode.el" . "04f07b902596b5ffd333435342c193977e5f8fa52688faab99c8665ed2a2e3b3")))

(defvar hm436-test-case-index 0)
(defvar hm436-test-root nil)
(defvar hm436-test-root-owned nil)

(defun hm436-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun hm436-test-source-state ()
  (let* ((located (locate-library "hcl-mode.el"))
         (main (and located (file-truename located)))
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
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (hm436-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/hcl-mode.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car hm436-test-manifest)))
      (error "Unexpected installed hcl-mode payload: %S" (or manifest files)))
    (dolist (entry hm436-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (hm436-test-sha file) expected))
          (error "Unexpected installed hcl-mode source: %S"
                 (cons entry manifest)))))
    (list :tree hm436-test-tree
          :manifest manifest
          :feature (featurep 'hcl-mode)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'hcl-mode package-alist)))))))

(defun hm436-test-forbid-external (operation &rest arguments)
  (error "Unexpected hcl-mode external boundary: %S %S" operation arguments))

(defun hm436-test-visit (root name code)
  (let ((file (expand-file-name name root)))
    (make-directory (file-name-directory file) t)
    (write-region code nil file nil 'silent)
    (find-file file)
    (when (fboundp 'font-lock-ensure)
      (font-lock-ensure)
      (syntax-propertize (point-max)))
    file))

(defun hm436-test-face-at (pattern)
  (save-excursion
    (goto-char (point-min))
    (re-search-forward pattern)
    (goto-char (match-beginning 0))
    (list :at (match-string-no-properties 0)
          :face (face-at-point))))

(defun hm436-test-indent-at (pattern)
  (goto-char (point-min))
  (re-search-forward pattern)
  (goto-char (match-beginning 0))
  (indent-for-tab-command)
  (list :col (current-indentation)
        :line (buffer-substring-no-properties
               (line-beginning-position)
               (line-end-position))))

(defun hm436-test-run (body)
  (let* ((index (cl-incf hm436-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "hcl-mode-%d" index)
                                       sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (source-before (hm436-test-source-state))
         (directory-before default-directory)
         (enable-local-before enable-local-variables)
         (debug-before debug-on-error)
         (print-circle-before print-circle)
         (indent-before hcl-indent-level)
         (hm436-test-root root)
         (hm436-test-root-owned nil)
         result body-error source-after cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute hcl-mode sandbox root"))
              (when (file-exists-p root)
                (error "hcl-mode sandbox root exists: %S" root))
              (make-directory root)
              (setq hm436-test-root-owned t
                    enable-local-variables nil
                    debug-on-error nil
                    print-circle nil
                    default-directory root)
              (cl-letf (((symbol-function 'call-process)
                         (lambda (&rest args)
                           (apply #'hm436-test-forbid-external
                                  'call-process args)))
                        ((symbol-function 'call-process-region)
                         (lambda (&rest args)
                           (apply #'hm436-test-forbid-external
                                  'call-process-region args)))
                        ((symbol-function 'make-process)
                         (lambda (&rest args)
                           (apply #'hm436-test-forbid-external
                                  'make-process args)))
                        ((symbol-function 'start-process)
                         (lambda (&rest args)
                           (apply #'hm436-test-forbid-external
                                  'start-process args)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest args)
                           (apply #'hm436-test-forbid-external
                                  'url-retrieve args)))
                        ((symbol-function 'url-retrieve-synchronously)
                         (lambda (&rest args)
                           (apply #'hm436-test-forbid-external
                                  'url-retrieve-synchronously args))))
                (setq result (funcall body root)))
              (setq source-after (hm436-test-source-state))
              (unless (equal source-before source-after)
                (error "hcl-mode source changed")))
          (error (setq body-error
                       (list (car condition)
                             (copy-tree (cdr condition))))))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error (push (list label (car condition)
                                  (copy-tree (cdr condition)))
                            cleanup-errors)))))
        (setq hcl-indent-level indent-before
              enable-local-variables enable-local-before
              debug-on-error debug-before
              print-circle print-circle-before
              default-directory directory-before)
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers-before)
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda ()
                       (when (buffer-live-p buffer)
                         (with-current-buffer buffer
                           (set-buffer-modified-p nil))
                         (kill-buffer buffer))))))
        (dolist (timer (append timer-list timer-idle-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window
                 (lambda () (set-window-configuration window-before)))
        (when (window-live-p selected-window-before)
          (attempt 'selected
                   (lambda () (select-window selected-window-before))))
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer
                   (lambda () (set-buffer buffer-before))))
        (when hm436-test-root-owned
          (attempt 'root (lambda () (delete-directory root t))))))
    (when body-error
      (error "hcl-mode body failed: %S" body-error))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :new-buffers (mapcar #'buffer-name
                                      (seq-remove
                                       (lambda (buffer)
                                         (memq buffer buffers-before))
                                       (buffer-list)))
                 :new-processes (length
                                 (seq-remove
                                  (lambda (process)
                                    (memq process processes-before))
                                  (process-list)))
                 :new-timers (length
                              (seq-remove
                               (lambda (timer)
                                 (memq timer timers-before))
                               (append timer-list timer-idle-list)))
                 :new-frames (length
                              (seq-remove
                               (lambda (frame)
                                 (memq frame frames-before))
                               (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :window-restored (eq (selected-window)
                                      selected-window-before)
                 :indent-restored (eq hcl-indent-level indent-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "hcl-mode cleanup failed: %S" (list result cleanup))
        (list :source source-before
              :result result
              :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HCL_MODE_MELPA_PIN, "hcl-mode.el")
        .expect("prepare pinned hcl-mode source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn opens_hcl_and_nomad_and_selects_the_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "opens_hcl_and_nomad_and_selects_the_mode",
        r####"
(hm436-test-run
 (lambda (root)
   (hm436-test-visit
    root "web.hcl"
    "variable \"ami\" {\n  description = \"café 界\"\n}\n")
   (let ((hcl
          (list :file (file-relative-name buffer-file-name root)
                :mode major-mode
                :derived (and (derived-mode-p 'prog-mode) t)
                :comment-start (copy-sequence comment-start)
                :indent indent-line-function
                :level hcl-indent-level
                :hcl-auto (cdr (assoc "\\.hcl\\'" auto-mode-alist))
                :nomad-auto (cdr (assoc "\\.nomad\\'" auto-mode-alist))
                :unicode (hm436-test-face-at "café"))))
     (hm436-test-visit root "job.nomad" "job \"web\" {\n  datacenters = [\"dc1\"]\n}\n")
     (list :hcl hcl
           :nomad (list :file (file-relative-name buffer-file-name root)
                        :mode major-mode)))))
"####,
        expect![[
            r##"OK (:source (:tree "12223205cfb8258ec1de9d61699ef2d1abd2e787" :manifest (("hcl-mode-pkg.el" . "d3b73384b3501939f13742dff41f7a5fc0306b228f2012e32914cdc9f5f474e5") ("hcl-mode.el" . "04f07b902596b5ffd333435342c193977e5f8fa52688faab99c8665ed2a2e3b3")) :feature t :version "20240220.1534") :result (:hcl (:file "web.hcl" :mode hcl-mode :derived t :comment-start "#" :indent hcl-indent-line :level 2 :hcl-auto hcl-mode :nomad-auto hcl-mode :unicode (:at "café" :face font-lock-string-face)) :nomad (:file "job.nomad" :mode hcl-mode)) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :indent-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn indents_blocks_maps_arrays_and_custom_level() -> ParityBatchCase {
    ParityBatchCase::value(
        "indents_blocks_maps_arrays_and_custom_level",
        r####"
(hm436-test-run
 (lambda (root)
   (hm436-test-visit
    root "stack.hcl"
    "provider \"aws\" {
foo = 10
}

map_var {
key = val
}

array_var [
\"foo\"
]

foo = \"val1\"

bar = \"val2\"
")
   (list :block (hm436-test-indent-at "^foo = 10")
         :map (hm436-test-indent-at "^key =")
         :array (hm436-test-indent-at "^\"foo\"")
         :closer (hm436-test-indent-at "^}")
         :top (hm436-test-indent-at "^bar =")
         :custom
         (progn
           (setq hcl-indent-level 4)
           (hm436-test-indent-at "foo = 10")))))
"####,
        expect![[
            r#"OK (:source (:tree "12223205cfb8258ec1de9d61699ef2d1abd2e787" :manifest (("hcl-mode-pkg.el" . "d3b73384b3501939f13742dff41f7a5fc0306b228f2012e32914cdc9f5f474e5") ("hcl-mode.el" . "04f07b902596b5ffd333435342c193977e5f8fa52688faab99c8665ed2a2e3b3")) :feature t :version "20240220.1534") :result (:block (:col 2 :line "  foo = 10") :map (:col 2 :line "  key = val") :array (:col 2 :line "  \"foo\"") :closer (:col 0 :line "}") :top (:col 0 :line "bar = \"val2\"") :custom (:col 4 :line "    foo = 10")) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :indent-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn fontifies_assignments_booleans_interpolation_and_heredoc() -> ParityBatchCase {
    ParityBatchCase::value(
        "fontifies_assignments_booleans_interpolation_and_heredoc",
        r####"
(hm436-test-run
 (lambda (root)
   (hm436-test-visit
    root "faces.hcl"
    "foo-bar = \"hello\"
enabled = true
map_var {
}
bar = \"${foo}\"
# café comment
/* block */
user_data = <<EOF
#!/usr/bin/env bash
echo hi
EOF
after_doc = 1
")
   (let ((closed
          (list :assign (hm436-test-face-at "foo-bar")
                :bool (hm436-test-face-at "true")
                :map (hm436-test-face-at "map_var")
                :interp
                (progn
                  (goto-char (point-min))
                  (re-search-forward "{foo}")
                  (goto-char (1+ (match-beginning 0)))
                  (list :at (string (char-after))
                        :face (face-at-point)))
                :hash-comment (hm436-test-face-at "café")
                :block-comment (hm436-test-face-at "block")
                :heredoc (hm436-test-face-at "bash")
                :after-heredoc (hm436-test-face-at "after_doc"))))
     (hm436-test-visit
      root "open-heredoc.hcl"
      "user_data = <<EOF
echo hi
broken = 1
")
     (let ((open-face (hm436-test-face-at "broken")))
       (goto-char (point-max))
       (insert "EOF\nrecovered = 1\n")
       (syntax-propertize (point-max))
       (when (fboundp 'font-lock-ensure)
         (font-lock-ensure))
       (list :closed closed
             :open open-face
             :recovered (hm436-test-face-at "recovered"))))))
"####,
        expect![[
            r#"OK (:source (:tree "12223205cfb8258ec1de9d61699ef2d1abd2e787" :manifest (("hcl-mode-pkg.el" . "d3b73384b3501939f13742dff41f7a5fc0306b228f2012e32914cdc9f5f474e5") ("hcl-mode.el" . "04f07b902596b5ffd333435342c193977e5f8fa52688faab99c8665ed2a2e3b3")) :feature t :version "20240220.1534") :result (:closed (:assign (:at "foo-bar" :face font-lock-variable-name-face) :bool (:at "true" :face font-lock-constant-face) :map (:at "map_var" :face font-lock-type-face) :interp (:at "f" :face font-lock-variable-name-face) :hash-comment (:at "café" :face font-lock-comment-face) :block-comment (:at "block" :face font-lock-comment-face) :heredoc (:at "bash" :face font-lock-string-face) :after-heredoc (:at "after_doc" :face font-lock-variable-name-face)) :open (:at "broken" :face font-lock-string-face) :recovered (:at "recovered" :face font-lock-variable-name-face)) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :indent-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn moves_by_defun_and_skips_indent_in_comments() -> ParityBatchCase {
    ParityBatchCase::value(
        "moves_by_defun_and_skips_indent_in_comments",
        r####"
(hm436-test-run
 (lambda (root)
   (hm436-test-visit
    root "motion.hcl"
    "variable \"ami\" {
    description = \"the AMI to use\"
}
# end1
resource \"aws_instance\" \"web\"{
    ami = \"${variable.ami}\"
    count = 2
}
")
   (let* ((beg
           (progn
             (goto-char (point-min))
             (re-search-forward "use")
             (hcl-beginning-of-defun)
             (list :ok (looking-at "^variable")
                   :line (buffer-substring-no-properties
                          (line-beginning-position)
                          (line-end-position)))))
          (end1
           (progn
             (re-search-forward "use")
             (hcl-end-of-defun)
             (list :ok (looking-at "^# end1")
                   :line (buffer-substring-no-properties
                          (line-beginning-position)
                          (line-end-position)))))
          (end2
           (progn
             (re-search-forward "^resource")
             (hcl-end-of-defun)
             (list :ok (eobp)
                   :point (point))))
          (from-resource
           (progn
             (goto-char (point-min))
             (re-search-forward "^resource")
             (hcl-beginning-of-defun)
             (list :ok (looking-at "^variable")
                   :line (buffer-substring-no-properties
                          (line-beginning-position)
                          (line-end-position))))))
     (hm436-test-visit
      root "comment.hcl"
      "    foo = 10
/*
  bar = 20
*/
")
     (let ((comment-indent
            (progn
              (goto-char (point-min))
              (re-search-forward "bar = 20")
              (goto-char (match-beginning 0))
              (let ((before (current-indentation)))
                (indent-for-tab-command)
                (list :before before
                      :after (current-indentation))))))
       (list :beg beg
             :end1 end1
             :end2 end2
             :from-resource from-resource
             :comment-indent comment-indent)))))
"####,
        expect![[
            r##"OK (:source (:tree "12223205cfb8258ec1de9d61699ef2d1abd2e787" :manifest (("hcl-mode-pkg.el" . "d3b73384b3501939f13742dff41f7a5fc0306b228f2012e32914cdc9f5f474e5") ("hcl-mode.el" . "04f07b902596b5ffd333435342c193977e5f8fa52688faab99c8665ed2a2e3b3")) :feature t :version "20240220.1534") :result (:beg (:ok t :line "variable \"ami\" {") :end1 (:ok t :line "# end1") :end2 (:ok t :point 137) :from-resource (:ok t :line "variable \"ami\" {") :comment-indent (:before 2 :after 2)) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :indent-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

#[test]
fn hcl_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        opens_hcl_and_nomad_and_selects_the_mode(),
        indents_blocks_maps_arrays_and_custom_level(),
        fontifies_assignments_booleans_interpolation_and_heredoc(),
        moves_by_defun_and_skips_indent_in_comments(),
    ];
    assert_oracle_batch_cases(oracle(), "hcl-mode-rank436", "hcl_mode_parity", &cases);
}
