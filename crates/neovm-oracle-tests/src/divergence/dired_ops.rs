//! Dired subsystem coverage (low-coverage area: only 2 prior files).
//!
//! Data-level dired operations on a temp directory: open + buffer name,
//! get-filename at point, mark/unmark, mark-files-regexp, toggle-marks,
//! map-over-marks collect, goto-file, rename/copy file, listing line count,
//! dired-buffers-for-dir. Avoids interactive do-* (which block on EOF).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

macro_rules! diredt {
    ($name:ident, $body:expr) => {
        #[test]
        fn $name() {
            return_if_neovm_enable_oracle_proptest_not_set!();
            crate::common::assert_oracle_parity($body);
        }
    };
}

diredt!(
    div_dired_open_bufname,
    r##"
(progn (require 'dired)
  (let ((dir (make-temp-file "neo-dired-o-" t)))
    (unwind-protect
        (let ((b (dired dir)))
          (prog1 (list (bufferp b) (string-prefix-p "neo-dired-o-" (buffer-name b)))
            (kill-buffer b)))
      (ignore-errors (delete-directory dir t)))))"##
);

diredt!(
    div_dired_get_filename_at_point,
    r##"
(progn (require 'dired)
  (let ((dir (make-temp-file "neo-dired-g-" t)))
    (dolist (n '("a.txt" "b.txt" "c.log")) (write-region "" nil (expand-file-name n dir) nil 0))
    (unwind-protect
        (let ((b (dired dir)))
          (with-current-buffer b (dired-next-line 2) (file-name-nondirectory (dired-get-filename)))
          (prog1 nil (kill-buffer b)))
      (ignore-errors (delete-directory dir t)))))"##
);

diredt!(
    div_dired_mark_get_marked_files,
    r##"
(progn (require 'dired)
  (let ((dir (make-temp-file "neo-dired-m-" t)))
    (dolist (n '("a.txt" "b.txt" "c.log")) (write-region "" nil (expand-file-name n dir) nil 0))
    (unwind-protect
        (let ((b (dired dir)))
          (with-current-buffer b (dired-mark 2) (length (dired-get-marked-files)))
          (prog1 nil (kill-buffer b)))
      (ignore-errors (delete-directory dir t)))))"##
);

diredt!(
    div_dired_mark_files_regexp,
    r##"
(progn (require 'dired)
  (let ((dir (make-temp-file "neo-dired-mr-" t)))
    (dolist (n '("a.txt" "b.txt" "c.log")) (write-region "" nil (expand-file-name n dir) nil 0))
    (unwind-protect
        (let ((b (dired dir)))
          (with-current-buffer b (dired-mark-files-regexp "\\.txt$" ?*) (length (dired-get-marked-files)))
          (prog1 nil (kill-buffer b)))
      (ignore-errors (delete-directory dir t)))))"##
);

diredt!(
    div_dired_toggle_marks,
    r##"
(progn (require 'dired)
  (let ((dir (make-temp-file "neo-dired-tm-" t)))
    (dolist (n '("a.txt" "b.txt" "c.log")) (write-region "" nil (expand-file-name n dir) nil 0))
    (unwind-protect
        (let ((b (dired dir)))
          (with-current-buffer b (dired-mark 1) (dired-toggle-marks) (length (dired-get-marked-files)))
          (prog1 nil (kill-buffer b)))
      (ignore-errors (delete-directory dir t)))))"##
);

diredt!(
    div_dired_map_over_marks,
    r##"
(progn (require 'dired)
  (let ((dir (make-temp-file "neo-dired-mom-" t)))
    (dolist (n '("a.txt" "b.txt" "c.log")) (write-region "" nil (expand-file-name n dir) nil 0))
    (unwind-protect
        (let ((b (dired dir)))
          (with-current-buffer b
            (dired-mark-files-regexp "." ?*)
            (let (acc)
              (dired-map-over-marks (lambda () (push (file-name-nondirectory (dired-get-filename)) acc)))
              (sort acc 'string<))))
      (ignore-errors (delete-directory dir t)))))"##
);

diredt!(
    div_dired_goto_file,
    r##"
(progn (require 'dired)
  (let ((dir (make-temp-file "neo-dired-gf-" t)))
    (dolist (n '("a.txt" "b.txt" "c.log")) (write-region "" nil (expand-file-name n dir) nil 0))
    (unwind-protect
        (let ((b (dired dir)))
          (with-current-buffer b
            (dired-goto-file (expand-file-name "c.log" dir))
            (file-name-nondirectory (dired-get-filename))))
      (ignore-errors (delete-directory dir t)))))"##
);

diredt!(
    div_dired_unmark_all,
    r##"
(progn (require 'dired)
  (let ((dir (make-temp-file "neo-dired-ua-" t)))
    (dolist (n '("a.txt" "b.txt" "c.log")) (write-region "" nil (expand-file-name n dir) nil 0))
    (unwind-protect
        (let ((b (dired dir)))
          (with-current-buffer b (dired-mark 3) (dired-unmark-all-marks nil) (length (dired-get-marked-files))))
      (ignore-errors (delete-directory dir t)))))"##
);

diredt!(
    div_dired_rename_file,
    r##"
(progn (require 'dired)
  (let ((dir (make-temp-file "neo-dired-rn-" t)))
    (dolist (n '("a.txt" "b.txt" "c.log")) (write-region "" nil (expand-file-name n dir) nil 0))
    (unwind-protect
        (progn (dired-rename-file (expand-file-name "a.txt" dir) (expand-file-name "a_r.txt" dir) nil)
               (sort (directory-files dir nil "^[^.]") 'string<))
      (ignore-errors (delete-directory dir t)))))"##
);

diredt!(
    div_dired_copy_file,
    r##"
(progn (require 'dired)
  (let ((dir (make-temp-file "neo-dired-cp-" t)))
    (dolist (n '("a.txt" "b.txt" "c.log")) (write-region "" nil (expand-file-name n dir) nil 0))
    (unwind-protect
        (progn (dired-copy-file (expand-file-name "a.txt" dir) (expand-file-name "a_copy.txt" dir) nil)
               (sort (directory-files dir nil "^[^.]") 'string<))
      (ignore-errors (delete-directory dir t)))))"##
);

diredt!(
    div_dired_listing_line_count,
    r##"
(progn (require 'dired)
  (let ((dir (make-temp-file "neo-dired-lc-" t)))
    (dolist (n '("a.txt" "b.txt" "c.log")) (write-region "" nil (expand-file-name n dir) nil 0))
    (unwind-protect
        (let ((b (dired dir)))
          (with-current-buffer b (count-lines (point-min) (point-max))))
      (ignore-errors (delete-directory dir t)))))"##
);

diredt!(
    div_dired_buffers_for_dir,
    r##"
(progn (require 'dired)
  (let ((dir (make-temp-file "neo-dired-bf-" t)))
    (unwind-protect
        (let ((b (dired dir)))
          (prog1 (length (dired-buffers-for-dir dir)) (kill-buffer b)))
      (ignore-errors (delete-directory dir t)))))"##
);

diredt!(
    div_dired_subdir_alist,
    r##"
(progn (require 'dired)
  (let ((dir (make-temp-file "neo-dired-sa-" t)))
    (dolist (n '("a.txt")) (write-region "" nil (expand-file-name n dir) nil 0))
    (unwind-protect
        (let ((b (dired dir)))
          (with-current-buffer b (length dired-subdir-alist)))
      (ignore-errors (delete-directory dir t)))))"##
);
