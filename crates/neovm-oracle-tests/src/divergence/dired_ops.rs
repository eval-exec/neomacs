//! Dired subsystem coverage (low-coverage area: only 2 prior files).
//!
//! Data-level dired operations on a temp directory: open + buffer name,
//! get-filename at point, mark/unmark, mark-files-regexp, toggle-marks,
//! map-over-marks collect, goto-file, rename/copy file, listing line count,
//! dired-buffers-for-dir. Avoids interactive do-* (which block on EOF).
//! Also the `dired-sort-set-mode-line' label rows (#833), as inline GNU
//! expectations so they hold in snapshot mode too.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[cfg(all(test, unix))]
mod count_nil;

#[cfg(all(test, unix))]
mod count_zero_open;

#[cfg(all(test, unix))]
mod directory_error_data;

#[cfg(all(test, unix))]
mod streaming_quit;

#[cfg(all(test, unix))]
mod attribute_failures;

#[cfg(all(test, unix))]
mod readdir_order;

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

// `dired-sort-set-mode-line' picks its label with `string-match-p' on two regexps
// built from `dired-ls-sorting-switches'.  These two rows carry an inline GNU
// expectation (not `diredt!`) so snapshot mode also pins the labels: issue #833
// reported the raw switches for "-l -a" as a Neomacs bug, and GNU 31.1 prints the
// same.  The short-option branch of `dired-sort-by-name-regexp' is two adjacent
// one-or-more classes (`-[^- tSXU]+[^- tSXU]+`, upstream f800f2300bf, Bug#80200),
// so one dash needs at least two letters after it.  Emacs 30.2 had one class and
// labels "-l -a" as "Dired by name"; the pin is 31.1.

#[test]
fn div_dired_sort_mode_line_labels() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"-ahl -v --group-directories-first\" \"Dired -ahl -v --group-directories-first\") (\"-ahlv --group-directories-first\" \"Dired by name\") (\"-l -a\" \"Dired -l -a\") (\"-la\" \"Dired by name\") (\"-al\" \"Dired by name\") (\"-l\" \"Dired -l\") (\"-l -a -h\" \"Dired -l -a -h\") (\"-lS\" \"Dired -lS\") (\"-l -S\" \"Dired -l -S\") (\"-lt\" \"Dired by date\") (\"-ltr\" \"Dired by date\") (\"-t\" \"Dired by date\") (\"-rt\" \"Dired by date\") (\"--sort=name -l\" \"Dired --sort=name -l\") (\"-ahl \\\"x y\\\"\" \"Dired by name\") (\"-l \\\"a b\\\" -h\" \"Dired -l \\\"a b\\\" -h\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'dired)
  (let (out)
    (dolist (switches '("-ahl -v --group-directories-first"
                        "-ahlv --group-directories-first"
                        "-l -a"
                        "-la"
                        "-al"
                        "-l"
                        "-l -a -h"
                        "-lS"
                        "-l -S"
                        "-lt"
                        "-ltr"
                        "-t"
                        "-rt"
                        "--sort=name -l"
                        "-ahl \"x y\""
                        "-l \"a b\" -h"))
      (with-temp-buffer
        (dired-mode)
        (setq-local dired-switches-in-mode-line nil
                    dired-actual-switches switches)
        (dired-sort-set-mode-line)
        (push (list switches mode-name) out)))
    (nreverse out)))"#,
        expect,
    );
}

#[test]
fn div_dired_sort_by_name_regexp_two_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((t t nil nil nil nil nil) (t t t nil nil) (t t nil nil nil) (t t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'dired)
  (let ((case-fold-search nil))
    (list
     (mapcar (lambda (s) (and (string-match-p dired-sort-by-name-regexp s) t))
             '("-ahlv --group-directories-first" "-la"
               "-ahl -v --group-directories-first" "-l -a" "-l" "-lS" "-l -S"))
     (mapcar (lambda (s) (and (string-match-p dired-sort-by-date-regexp s) t))
             '("-lt" "-t" "-rt" "-l -a" "-la"))
     (mapcar (lambda (s) (and (string-match-p "-[^- tSXU]+[^- tSXU]+" s) t))
             '("-la" "-ahl -v" "-l -a" "-l" "-a"))
     (mapcar (lambda (s) (and (string-match-p "-[^- tSXU]+" s) t))
             '("-la" "-ahl -v" "-l -a" "-l" "-a")))))"#,
        expect,
    );
}
