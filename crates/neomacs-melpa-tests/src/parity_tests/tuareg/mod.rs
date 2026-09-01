//! Practical parity for Tuareg's public OCaml editing commands.
//!
//! These cases open real `.ml`/`.mli` files, indent match/let/function
//! arguments, fontify keywords, move by defun and phrase, comment a line,
//! jump from an ocamlc error, and recover after an unbraced eval.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, TUAREG_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'compile)
(require 'tuareg)
(set-window-configuration (current-window-configuration))

(defconst tg435-test-tree
  "8684dd638e374cf77576107bb47eea6de114aacd")
(defconst tg435-test-manifest
  '(("dot-emacs.el" . "eca97615a731930e3bad965a01c77092ee3e5fa1d00ac1986b5ee967e4deafd8")
    ("ocamldebug.el" . "a6058e93e3c205bb95950d91263513c3d51091274ee65d76d6bc58b115ea2331")
    ("tuareg-compat.el" . "2f44915f34c818e12152826c31b1b0213ca20884cd3e5259bd39cdd9e5005e41")
    ("tuareg-menhir.el" . "257695c448612fa5c4145615894ebc1abe6d409d33d6f80252cd1cb7dff29c95")
    ("tuareg-opam.el" . "d545c63402c45f28fba6a389c84aabc553e045bf6096cffc2280937335082392")
    ("tuareg-pkg.el" . "e80ede2a2a9d66960507a18a5c0b8d71e854737034c1629c63ea5816c528d606")
    ("tuareg.el" . "8593843f998e1ca29571c2e88a5cda22280f8812e6a47957c4fb31f0bb123a73")))

(defvar tg435-test-case-index 0)
(defvar tg435-test-root nil)
(defvar tg435-test-root-owned nil)

(defun tg435-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun tg435-test-source-state ()
  (let* ((located (locate-library "tuareg.el"))
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
                         (cons file (tg435-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/tuareg.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car tg435-test-manifest)))
      (error "Unexpected installed tuareg payload: %S" (or manifest files)))
    (dolist (entry tg435-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (tg435-test-sha file) expected))
          (error "Unexpected installed tuareg source: %S"
                 (cons entry manifest)))))
    (list :tree tg435-test-tree
          :manifest manifest
          :feature (featurep 'tuareg)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'tuareg package-alist)))))))

(defun tg435-test-condition (thunk)
  (condition-case condition
      (list :returned (funcall thunk))
    (error
     (list :error (car condition)
           :data (mapcar (lambda (item)
                           (if (stringp item)
                               (copy-sequence item)
                             (copy-tree item)))
                         (cdr condition))
           :message (copy-sequence (error-message-string condition))))))

(defun tg435-test-forbid-external (operation &rest arguments)
  (error "Unexpected tuareg external boundary: %S %S" operation arguments))

(defun tg435-test-visit (root name code &optional mode)
  (let ((file (expand-file-name name root)))
    (make-directory (file-name-directory file) t)
    (write-region code nil file nil 'silent)
    (find-file file)
    (funcall (or mode #'tuareg-mode))
    (when (fboundp 'font-lock-ensure)
      (font-lock-ensure))
    file))

(defun tg435-test-reindent ()
  (goto-char (point-min))
  (while (re-search-forward (rx bol (+ (in " \t"))) nil t)
    (let ((syntax (save-match-data (syntax-ppss))))
      (unless (or (nth 3 syntax) (nth 4 syntax))
        (replace-match ""))))
  (indent-region (point-min) (point-max))
  (buffer-substring-no-properties (point-min) (point-max)))

(defun tg435-test-face-at (pattern)
  (save-excursion
    (goto-char (point-min))
    (re-search-forward pattern)
    (goto-char (match-beginning 0))
    (list :at (match-string-no-properties 0)
          :face (face-at-point))))

(defun tg435-test-run (body)
  (let* ((index (cl-incf tg435-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "tuareg-%d" index)
                                       sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (source-before (tg435-test-source-state))
         (directory-before default-directory)
         (enable-local-before enable-local-variables)
         (debug-before debug-on-error)
         (print-circle-before print-circle)
         (align-before tuareg-indent-align-with-first-arg)
         (pipes-before tuareg-match-patterns-aligned)
         (indent-before tuareg-default-indent)
         (next-error-before (and (boundp 'next-error-last-buffer)
                                 next-error-last-buffer))
         (tg435-test-root root)
         (tg435-test-root-owned nil)
         result body-error source-after cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute tuareg sandbox root"))
              (when (file-exists-p root)
                (error "tuareg sandbox root exists: %S" root))
              (make-directory root)
              (setq tg435-test-root-owned t
                    enable-local-variables nil
                    debug-on-error nil
                    print-circle nil
                    default-directory root)
              (cl-letf (((symbol-function 'call-process)
                         (lambda (&rest args)
                           (apply #'tg435-test-forbid-external
                                  'call-process args)))
                        ((symbol-function 'call-process-region)
                         (lambda (&rest args)
                           (apply #'tg435-test-forbid-external
                                  'call-process-region args)))
                        ((symbol-function 'make-process)
                         (lambda (&rest args)
                           (apply #'tg435-test-forbid-external
                                  'make-process args)))
                        ((symbol-function 'start-process)
                         (lambda (&rest args)
                           (apply #'tg435-test-forbid-external
                                  'start-process args)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest args)
                           (apply #'tg435-test-forbid-external
                                  'url-retrieve args)))
                        ((symbol-function 'url-retrieve-synchronously)
                         (lambda (&rest args)
                           (apply #'tg435-test-forbid-external
                                  'url-retrieve-synchronously args))))
                (setq result (funcall body root)))
              (setq source-after (tg435-test-source-state))
              (unless (equal source-before source-after)
                (error "tuareg source changed")))
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
        (setq tuareg-indent-align-with-first-arg align-before
              tuareg-match-patterns-aligned pipes-before
              tuareg-default-indent indent-before
              enable-local-variables enable-local-before
              debug-on-error debug-before
              print-circle print-circle-before
              default-directory directory-before)
        (when (boundp 'next-error-last-buffer)
          (setq next-error-last-buffer next-error-before))
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
        (when tg435-test-root-owned
          (attempt 'root (lambda () (delete-directory root t))))))
    (when body-error
      (error "tuareg body failed: %S" body-error))
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
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "tuareg cleanup failed: %S" (list result cleanup))
        (list :source source-before
              :result result
              :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TUAREG_MELPA_PIN, "tuareg.el")
        .expect("prepare pinned tuareg source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn opens_ml_and_indents_let_match_and_function_args() -> ParityBatchCase {
    ParityBatchCase::value(
        "opens_ml_and_indents_let_match_and_function_args",
        r####"
(tg435-test-run
 (lambda (root)
   (tg435-test-visit
    root "main.ml"
    "let f x =
match x with
| A
| B ->
g
y
| C -> 1
")
   (let* ((opened
           (list :file (file-relative-name buffer-file-name root)
                 :mode major-mode
                 :derived (and (derived-mode-p 'prog-mode) t)
                 :comment-start (copy-sequence comment-start)
                 :tabs indent-tabs-mode
                 :first-column compilation-first-column
                 :auto (cdr (assoc "\\.ml[p]?\\'" auto-mode-alist))))
          (default-indent
           (progn
             (setq tuareg-indent-align-with-first-arg nil
                   tuareg-match-patterns-aligned nil)
             (tg435-test-reindent)))
          (aligned
           (progn
             (erase-buffer)
             (insert "let _ =
apply arg1
arg2
")
             (setq tuareg-indent-align-with-first-arg t)
             (tg435-test-reindent))))
     (list :opened opened
           :default-indent default-indent
           :aligned aligned))))
"####,
        expect![[
            r#"OK (:source (:tree "8684dd638e374cf77576107bb47eea6de114aacd" :manifest (("dot-emacs.el" . "eca97615a731930e3bad965a01c77092ee3e5fa1d00ac1986b5ee967e4deafd8") ("ocamldebug.el" . "a6058e93e3c205bb95950d91263513c3d51091274ee65d76d6bc58b115ea2331") ("tuareg-compat.el" . "2f44915f34c818e12152826c31b1b0213ca20884cd3e5259bd39cdd9e5005e41") ("tuareg-menhir.el" . "257695c448612fa5c4145615894ebc1abe6d409d33d6f80252cd1cb7dff29c95") ("tuareg-opam.el" . "d545c63402c45f28fba6a389c84aabc553e045bf6096cffc2280937335082392") ("tuareg-pkg.el" . "e80ede2a2a9d66960507a18a5c0b8d71e854737034c1629c63ea5816c528d606") ("tuareg.el" . "8593843f998e1ca29571c2e88a5cda22280f8812e6a47957c4fb31f0bb123a73")) :feature t :version "20260626.936") :result (:opened (:file "main.ml" :mode tuareg-mode :derived t :comment-start "(* " :tabs nil :first-column 0 :auto tuareg-mode) :default-indent "let f x =\n  match x with\n  | A\n    | B ->\n     g\n       y\n  | C -> 1\n" :aligned "let _ =\n  apply arg1\n        arg2\n") :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn fontifies_keywords_and_opens_interface_and_opam() -> ParityBatchCase {
    ParityBatchCase::value(
        "fontifies_keywords_and_opens_interface_and_opam",
        r####"
(tg435-test-run
 (lambda (root)
   (tg435-test-visit
    root "types.ml"
    "(* café 界 *)
type t = A | B of int
let f x =
  match x with
  | A -> \"ok\"
  | B n -> string_of_int n
")
   (let ((faces
          (list (tg435-test-face-at "type")
                (tg435-test-face-at "let")
                (tg435-test-face-at "match")
                (tg435-test-face-at "café")
                (tg435-test-face-at "\"ok\""))))
     (tg435-test-visit root "types.mli" "type t\nval f : t -> string\n"
                      #'tuareg-interface-mode)
     (let ((iface
            (list :file (file-relative-name buffer-file-name root)
                  :mode major-mode
                  :parent (and (derived-mode-p 'tuareg-mode) t)
                  :auto (cdr (assoc "\\.mli\\'" auto-mode-alist))))
           (opam
            (tg435-test-condition
             (lambda ()
               (tg435-test-visit
                root "pkg.opam"
                "opam-version: \"2.0\"\ndepends: [ \"ocaml\" ]\n"
                #'tuareg-opam-mode)
               major-mode))))
       (list :faces faces
             :iface iface
             :opam opam)))))
"####,
        expect![[
            r#"OK (:source (:tree "8684dd638e374cf77576107bb47eea6de114aacd" :manifest (("dot-emacs.el" . "eca97615a731930e3bad965a01c77092ee3e5fa1d00ac1986b5ee967e4deafd8") ("ocamldebug.el" . "a6058e93e3c205bb95950d91263513c3d51091274ee65d76d6bc58b115ea2331") ("tuareg-compat.el" . "2f44915f34c818e12152826c31b1b0213ca20884cd3e5259bd39cdd9e5005e41") ("tuareg-menhir.el" . "257695c448612fa5c4145615894ebc1abe6d409d33d6f80252cd1cb7dff29c95") ("tuareg-opam.el" . "d545c63402c45f28fba6a389c84aabc553e045bf6096cffc2280937335082392") ("tuareg-pkg.el" . "e80ede2a2a9d66960507a18a5c0b8d71e854737034c1629c63ea5816c528d606") ("tuareg.el" . "8593843f998e1ca29571c2e88a5cda22280f8812e6a47957c4fb31f0bb123a73")) :feature t :version "20260626.936") :result (:faces ((:at "type" :face tuareg-font-lock-governing-face) (:at "let" :face tuareg-font-lock-governing-face) (:at "match" :face font-lock-keyword-face) (:at "café" :face font-lock-comment-face) (:at "\"ok\"" :face font-lock-string-face)) :iface (:file "types.mli" :mode tuareg-interface-mode :parent t :auto tuareg-interface-mode) :opam (:error void-variable :data (tuareg-opam--flymake-proc-allowed-file-name-masks) :message "Symbol’s value as variable is void: tuareg-opam--flymake-proc-allowed-file-name-masks")) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn moves_by_defun_discovers_phrases_and_comments_a_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "moves_by_defun_discovers_phrases_and_comments_a_line",
        r####"
(tg435-test-run
 (lambda (root)
   (tg435-test-visit
    root "phrase.ml"
    "let a = 1 and b = 2 in a + b
let f x =
  x + 1
and g x =
  x * 2
type ta = A
        | B of tb
and tb = C
       | D of ta
;;
")
   (let* ((p-min (point-min))
          (end1 (progn (goto-char p-min) (end-of-defun) (point)))
          (end2 (progn (end-of-defun) (point)))
          (beg2 (progn (beginning-of-defun) (point)))
          (phrase1 (tuareg-discover-phrase p-min))
          (phrase2 (tuareg-discover-phrase end1))
          (imenu (tuareg-imenu-create-index))
          (commented
           (progn
             (goto-char p-min)
             (tuareg-comment-dwim)
             (buffer-substring-no-properties
              (line-beginning-position)
              (line-end-position)))))
     (list :end1 end1
           :end2 end2
           :beg2 beg2
           :phrase1 phrase1
           :phrase2 phrase2
           :imenu imenu
           :commented commented))))
"####,
        expect![[
            r#"OK (:source (:tree "8684dd638e374cf77576107bb47eea6de114aacd" :manifest (("dot-emacs.el" . "eca97615a731930e3bad965a01c77092ee3e5fa1d00ac1986b5ee967e4deafd8") ("ocamldebug.el" . "a6058e93e3c205bb95950d91263513c3d51091274ee65d76d6bc58b115ea2331") ("tuareg-compat.el" . "2f44915f34c818e12152826c31b1b0213ca20884cd3e5259bd39cdd9e5005e41") ("tuareg-menhir.el" . "257695c448612fa5c4145615894ebc1abe6d409d33d6f80252cd1cb7dff29c95") ("tuareg-opam.el" . "d545c63402c45f28fba6a389c84aabc553e045bf6096cffc2280937335082392") ("tuareg-pkg.el" . "e80ede2a2a9d66960507a18a5c0b8d71e854737034c1629c63ea5816c528d606") ("tuareg.el" . "8593843f998e1ca29571c2e88a5cda22280f8812e6a47957c4fb31f0bb123a73")) :feature t :version "20260626.936") :result (:end1 30 :end2 48 :beg2 30 :phrase1 (1 29 29) :phrase2 (30 65 65) :imenu (("Install Merlin or caml-mode" . 0)) :commented "(* let a = 1 and b = 2 in a + b *)") :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn matches_compiler_errors_and_recovers_from_unbraced_eval() -> ParityBatchCase {
    ParityBatchCase::value(
        "matches_compiler_errors_and_recovers_from_unbraced_eval",
        r####"
(tg435-test-run
 (lambda (root)
   (let* ((src (expand-file-name "hello.ml" root))
          (log (expand-file-name "ocaml.log" root)))
     (write-region "let x = 1\nlet y = x 2\n" nil src nil 'silent)
     (write-region
      (concat "File \"hello.ml\", line 2, characters 8-9:\n"
              "Error: This expression has type int\n"
              "       This is not a function; it cannot be applied.\n")
      nil log nil 'silent)
     (find-file log)
     (let* ((rule (assq 'ocaml compilation-error-regexp-alist-alist))
            (parsed
             (progn
               (goto-char (point-min))
               (and (re-search-forward (nth 1 rule) nil t)
                    (list :file (copy-sequence (match-string 3))
                          :line (copy-sequence (match-string 4))
                          :col0 (copy-sequence (match-string 6))
                          :col1 (copy-sequence (match-string 7))
                          :end-col (tuareg--end-column)
                          :warning (match-string 8)))))
            (unbraced
             (with-current-buffer (find-file-noselect src)
               (tuareg-mode)
               (goto-char (point-max))
               (insert "\n(")
               (tg435-test-condition #'tuareg-eval-phrase))))
       (list :ocaml-rule
             (and (memq 'ocaml compilation-error-regexp-alist) t)
             :parsed parsed
             :unbraced unbraced)))))
"####,
        expect![[
            r#"OK (:source (:tree "8684dd638e374cf77576107bb47eea6de114aacd" :manifest (("dot-emacs.el" . "eca97615a731930e3bad965a01c77092ee3e5fa1d00ac1986b5ee967e4deafd8") ("ocamldebug.el" . "a6058e93e3c205bb95950d91263513c3d51091274ee65d76d6bc58b115ea2331") ("tuareg-compat.el" . "2f44915f34c818e12152826c31b1b0213ca20884cd3e5259bd39cdd9e5005e41") ("tuareg-menhir.el" . "257695c448612fa5c4145615894ebc1abe6d409d33d6f80252cd1cb7dff29c95") ("tuareg-opam.el" . "d545c63402c45f28fba6a389c84aabc553e045bf6096cffc2280937335082392") ("tuareg-pkg.el" . "e80ede2a2a9d66960507a18a5c0b8d71e854737034c1629c63ea5816c528d606") ("tuareg.el" . "8593843f998e1ca29571c2e88a5cda22280f8812e6a47957c4fb31f0bb123a73")) :feature t :version "20260626.936") :result (:ocaml-rule t :parsed (:file "hello.ml" :line "2" :col0 "8" :col1 "9" :end-col 8 :warning nil) :unbraced (:error user-error :data ("Expression after the point is not well braced") :message "Expression after the point is not well braced")) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn tuareg_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        opens_ml_and_indents_let_match_and_function_args(),
        fontifies_keywords_and_opens_interface_and_opam(),
        moves_by_defun_discovers_phrases_and_comments_a_line(),
        matches_compiler_errors_and_recovers_from_unbraced_eval(),
    ];
    assert_oracle_batch_cases(oracle(), "tuareg-rank435", "tuareg_parity", &cases);
}
