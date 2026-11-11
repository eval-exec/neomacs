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

  (dolist (manual '("emacs" "elisp" "eintr" "cl" "org" "modus-themes"))
    (let ((file (expand-file-name (concat manual ".info") directory)))
      (unless (file-readable-p file)
        (error "Artifact manual missing or unreadable: %s" file))
      (neomacs-artifact-info-owned-file file directory))
    (neomacs-artifact-info-node manual "Top" directory))
  (neomacs-artifact-info-node "elisp" "Hooks" directory)
  (princ "NEOMACS_INFO_MANUALS_OK\n"))

;;; verify-info-manuals.el ends here
