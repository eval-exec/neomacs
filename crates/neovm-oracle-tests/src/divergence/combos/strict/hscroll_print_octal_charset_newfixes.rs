//! Strict combo oracle probes, batch 110: new areas from the remote pull —
//! hscroll behavior, prin1 octal-escape of unibyte high bytes, charset
//! resolution, and file-name-directory on Windows drive-relative names.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_s4_prin1_octal_escape_unibyte_high_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (prin1-to-string (string 200 201 202))
      (prin1-to-string (string 128 255))
      (prin1-to-string (string 0 1 127))
      (length (prin1-to-string (string 200)))
      (prin1-to-string "\377")
      (prin1-to-string (string-make-unibyte (string 200))))
"####,
    );
}

#[test]
fn div_s4_hscroll_window_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((b (get-buffer-create " *probe-hscroll-state*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (with-current-buffer b
          (insert (make-string 500 ?x)))
        (set-window-hscroll nil 10)
        (let ((auto-hscroll-mode nil))
          (goto-char 100)
          (list (window-hscroll)
                (current-column)
                (window-start))))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"####,
    );
}

#[test]
fn div_s4_file_name_directory_drive_relative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (file-name-directory "/path/to/file.txt")
      (file-name-directory "relative.el")
      (file-name-directory "/")
      (file-name-directory "~/dir/file")
      (file-name-directory "file.txt")
      (file-name-nondirectory "/path/to/")
      (file-name-nondirectory "file.txt"))
"####,
    );
}

#[test]
fn div_s4_charset_resolution_at_runtime() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (charsetp 'ascii)
      (charsetp 'unicode)
      (charsetp 'japanese-jisx0208)
      (char-charset ?a)
      (char-charset ?あ)
      (char-charset ?é)
      (decode-char 'ascii 65)
      (encode-char ?あ 'japanese-jisx0208))
"####,
    );
}

#[test]
fn div_s4_auto_hscroll_truncated_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((b (get-buffer-create " *probe-auto-hscroll*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (with-current-buffer b
          (insert (make-string 300 ?x)))
        (let ((truncate-lines t)
              (auto-hscroll-mode 'current-shift))
          (goto-char 250)
          (list (window-hscroll)
                (current-column)
                (- (point) (window-start))))
        (let ((truncate-lines nil))
          (goto-char 250)
          (list (window-hscroll)
                (current-column))))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"####,
    );
}
