;;; verify-info-manuals.el --- Check an artifact's Info manuals -*- lexical-binding: t; -*-

;; Exercise the normal search path: setting Info-directory-list here would
;; conceal a broken runtime installation.  GNU 31.1 info.el's info-initialize,
;; Info-default-dirs and Info-find-file define this lookup contract.
(require 'info)
(info-initialize)

(defun neomacs-artifact-info-owned-file (file directory)
  "Require FILE to resolve directly inside canonical DIRECTORY."
  (unless (and (stringp file)
               (equal (file-name-directory (file-truename file)) directory))
    (error "Info file escaped artifact directory %S: %S" directory file)))

(defun neomacs-artifact-info-node (manual node directory)
  "Open MANUAL's NODE normally and require artifact-owned backing files."
  (Info-find-node manual node)
  (unless (equal Info-current-node node)
    (error "Wrong Info node for %s: expected %S, got %S"
           manual node Info-current-node))
  (neomacs-artifact-info-owned-file Info-current-file directory)
  (when Info-current-subfile
    (neomacs-artifact-info-owned-file
     (expand-file-name Info-current-subfile directory) directory))
  (princ (format "Info (%s)%s: %s\n" manual node Info-current-file)))

(defun neomacs-artifact-info-manuals (directory)
  "Read DIRECTORY's generated inventory, or use the GNU comparison subset."
  (let ((manifest (expand-file-name ".neomacs-info-manifest" directory)))
    (if (file-exists-p manifest)
        (progn
          (neomacs-artifact-info-owned-file manifest directory)
          (let ((entries (with-temp-buffer
                           (insert-file-contents manifest)
                           (split-string (buffer-string) "\n"))))
            ;; A final newline terminates the last entry; interior blank
            ;; lines are invalid rather than silently skipped.
            (when (equal (car (last entries)) "")
              (setq entries (butlast entries)))
            (unless entries
              (error "Artifact Info manifest is empty: %s" manifest))
            (dolist (entry entries)
              (unless (string-match-p
                       "\\`[A-Za-z0-9][A-Za-z0-9._+-]*\\.info\\'" entry)
                (error "Invalid manual basename in artifact manifest: %S" entry)))
            (mapcar #'file-name-sans-extension entries)))
      ;; The pinned GNU source tree has no Neomacs-generated manifest.
      '("emacs" "elisp" "eintr" "cl" "org" "modus-themes"))))

(defun neomacs-artifact-info-check-manifest-parser ()
  "Check manifest results and diagnostics in a private disposable directory."
  (let* ((directory (file-name-as-directory
                     (file-truename (make-temp-file "neomacs-info-manifest-" t))))
         (manifest (expand-file-name ".neomacs-info-manifest" directory)))
    (unwind-protect
        (progn
          (dolist (contents '("emacs.info\ncustom-name_1+2.info\n"
                              "emacs.info\ncustom-name_1+2.info"))
            (with-temp-file manifest (insert contents))
            (unless (equal (neomacs-artifact-info-manuals directory)
                           '("emacs" "custom-name_1+2"))
              (error "Manifest parser rejected valid entries: %S" contents)))
          (dolist (case '(("" . "Artifact Info manifest is empty")
                          ("emacs.info\n\nelisp.info\n" . "Invalid manual basename")
                          ("../outside.info\n" . "Invalid manual basename")
                          ("/absolute.info\n" . "Invalid manual basename")
                          ("-option.info\n" . "Invalid manual basename")
                          ("bad name.info\n" . "Invalid manual basename")
                          ("manual.txt\n" . "Invalid manual basename")))
            (with-temp-file manifest (insert (car case)))
            (let ((diagnostic
                   (condition-case err
                       (progn (neomacs-artifact-info-manuals directory) nil)
                     (error (error-message-string err)))))
              (unless (and diagnostic
                           (string-match-p (regexp-quote (cdr case)) diagnostic))
                (error "Manifest parser check for %S expected %S, got %S"
                       (car case) (cdr case) diagnostic))))
          (delete-file manifest)
          (unless (equal (neomacs-artifact-info-manuals directory)
                         '("emacs" "elisp" "eintr" "cl" "org" "modus-themes"))
            (error "Missing manifest did not select the GNU comparison subset"))
          (princ "Info manifest parser checks passed\n"))
      (delete-directory directory t))))

(neomacs-artifact-info-check-manifest-parser)

(let* ((expected (getenv "NEOMACS_EXPECTED_INFO_DIRECTORY"))
       (directory (and expected
                       (file-name-as-directory (file-truename expected)))))
  (unless (and directory (file-directory-p directory))
    (error "Artifact Info directory missing: %S (Info-directory-list=%S)"
           expected Info-directory-list))
  (unless (and (car Info-directory-list)
               (equal (file-name-as-directory
                       (file-truename (car Info-directory-list)))
                      directory))
    (error "Artifact Info directory is not first in natural search path: %S; expected %S"
           Info-directory-list directory))

  ;; GNU's (dir)Top is virtual and has Info-current-file="dir". Verify its
  ;; actual input files, then open the artifact's physical dir node as well.
  (Info-find-node "dir" "Top")
  (unless (and (equal Info-current-file "dir")
               (equal Info-current-node "Top"))
    (error "Cannot open the natural Info directory node"))
  (let ((found nil)
        (dir-file (file-truename (expand-file-name "dir" directory))))
    (dolist (entry Info-dir-file-attributes)
      (when (equal (file-truename (car entry)) dir-file)
        (setq found t)))
    (unless found
      (error "Info directory node did not read artifact's dir file: %S"
             Info-dir-file-attributes)))
  (neomacs-artifact-info-node (expand-file-name "dir" directory) "Top" directory)

  (let ((manuals (neomacs-artifact-info-manuals directory)))
    (dolist (manual manuals)
      (let ((file (expand-file-name (concat manual ".info") directory)))
        (unless (file-readable-p file)
          (error "Artifact manual missing or unreadable: %s" file))
        (neomacs-artifact-info-owned-file file directory))
      (neomacs-artifact-info-node manual "Top" directory))
    (princ (format "Info Top nodes verified: %d\n" (length manuals))))
  (neomacs-artifact-info-node "elisp" "Hooks" directory)
  (princ "NEOMACS_INFO_MANUALS_OK\n"))

;;; verify-info-manuals.el ends here
