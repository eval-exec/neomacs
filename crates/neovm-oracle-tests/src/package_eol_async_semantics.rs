//! GNU `package.el` / `lisp-mnt.el` contracts behind the two MELPA upstream
//! ERT scenarios `package-test-install-file-EOLs` and
//! `package-test-update-archives-async`.
//!
//! GNU decodes a DOS-EOL package file to LF before `package-buffer-info' runs,
//! so the parsed version never carries a carriage return; and its asynchronous
//! package-menu completion writes the read-only "*Packages*" buffer from a
//! timer callback.  These probes pin both layers against GNU directly.

use crate::common::assert_oracle_parity_expect;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_package_buffer_info_parses_crlf_version_without_carriage_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"1.3\" (nil 108) simple-single (1 3) \"Test package\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'lisp-mnt)
  (require 'package)
  (let ((file (make-temp-file "neo-crlf-pkg" nil ".el")))
    (unwind-protect
        (progn
          (with-temp-buffer
            (set-buffer-file-coding-system 'utf-8-dos)
            (insert ";;; simple-single.el --- Test package\n;; Version: 1.3\n;; Keywords: test\n\n;;; Code:\n(provide 'simple-single)\n")
            (write-region (point-min) (point-max) file))
          (with-temp-buffer
            (insert-file-contents file)
            (let ((decoded (list (string-match-p "\r" (buffer-string))
                                 (length (buffer-string))))
                  (version (lm-package-version)))
              (let ((desc (package-buffer-info)))
                (list version decoded
                      (package-desc-name desc)
                      (package-desc-version desc)
                      (package-desc-summary desc))))))
      (delete-file file))))
"##,
        expect,
    );
}

#[test]
fn oracle_package_menu_post_refresh_writes_read_only_buffer_from_timer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `package-menu-mode' makes the buffer read-only and installs
    // `package-menu--refresh' on `tabulated-list-revert-hook'.  The probe
    // removes that hook so the printed entry is fixed (the real refresh
    // recomputes it from the package database), then runs the refresh
    // completion from a timer, exactly as the async download path does.  GNU
    // writes the read-only buffer without signalling and the buffer keeps the
    // 83-byte entry.
    let expect = expect_test::expect![[r#""OK (t 83 :ok 83)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'package)
  (require 'tabulated-list)
  (let ((buf (get-buffer-create "*Packages*")))
    (unwind-protect
        (with-current-buffer buf
          (package-menu-mode)
          (remove-hook 'tabulated-list-revert-hook #'package-menu--refresh t)
          (setq tabulated-list-entries
                (list (list "simple-single"
                            (vector "simple-single" "1.3" "available" "gnu" "Test package"))))
          (tabulated-list-print)
          (let ((before (buffer-size))
                (result :pending))
            (run-at-time 0 nil
                         (lambda ()
                           (with-current-buffer buf
                             (setq result (condition-case err
                                              (progn (package-menu--post-refresh) :ok)
                                            (error err))))))
            (let ((n 0))
              (while (and (eq result :pending) (< n 100))
                (accept-process-output nil 0.02)
                (setq n (1+ n))))
            (list buffer-read-only before result (buffer-size))))
      (kill-buffer buf))))
"##,
        expect,
    );
}
