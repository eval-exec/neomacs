//! Complex combo batch 106 — eshell / shell-command / term / vterm / tramp
//! / process-file / remote-file access metadata.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx106_eshell_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eshell)
      (list (fboundp 'eshell)
            (boundp 'eshell-directory-name)
            (boundp 'eshell-buffer-name)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx106_eshell_basic_command_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello\\n\" \"hello\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((output (shell-command-to-string "echo hello")))
      (list output
            (string-trim output)
            (string= (string-trim output) "hello")))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx106_shell_command_with_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"3\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((input-file (make-temp-file "neo-cx106-in"))
       (output (progn
                 (with-temp-buffer
                   (insert "alpha\nbeta\ngamma\n")
                   (write-region (point-min) (point-max) input-file nil 'silent))
                 (shell-command-to-string (format "wc -l < %s" input-file)))))
  (delete-file input-file)
  (string-trim output))
"##,
        expect,
    );
}

#[test]
fn div_cx106_call_process_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"hello\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx106-cp*")))
  (with-current-buffer buf
    (erase-buffer))
  (let ((status (call-process "echo" nil buf nil "hello")))
    (prog1 (list status
                 (with-current-buffer buf (buffer-string)))
      (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx106_call_process_region_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"alpha\\nbeta\\ngamma\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx106-cpr*")))
  (with-current-buffer buf
    (erase-buffer)
    (insert "alpha beta gamma")
    (let ((status (call-process-region (point-min) (point-max)
                                       "tr" t t nil " " "\n")))
      (list status (buffer-string))))
  (prog1 (with-current-buffer buf (buffer-string))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx106_process_file_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'process-file)
      (fboundp 'process-file-side-effects)
      (fboundp 'make-process)
      (fboundp 'make-pipe-process)
      (fboundp 'serial-process-configure))
"##,
        expect,
    );
}

#[test]
fn div_cx106_tramp_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'tramp)
      (list (fboundp 'tramp-file-name-handler)
            (boundp 'tramp-methods)
            (boundp 'tramp-default-method)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx106_remote_file_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"/method:host:\" \"/ssh:localhost:\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (file-remote-p "/local/path")
          (file-remote-p "/method:host:/remote/path")
          (file-remote-p "/ssh:localhost:"))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx106_make_network_process_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored file-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((p (make-network-process :name "neo-cx106-net"
                                    :host "127.0.0.1"
                                    :service 80
                                    :family 'ipv4)))
      (prog1 (list (processp p)
                   (process-status p)
                   (process-name p))
        (delete-process p)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx106_term_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'term)
      (list (fboundp 'term)
            (fboundp 'ansi-term)
            (fboundp 'make-term)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx106_vterm_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'vterm)
          (fboundp 'vterm)
          (fboundp 'vterm-other-window))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx106_shell_environment_persistence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"value-1\" \"value-2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((original (getenv "NEO_CX106"))
       (env1 (let ((process-environment
                    (cons "NEO_CX106=value-1" process-environment)))
               (shell-command-to-string "echo $NEO_CX106")))
       (env2 (let ((process-environment
                    (cons "NEO_CX106=value-2" process-environment)))
               (shell-command-to-string "echo $NEO_CX106"))))
  (list original
        (string-trim env1)
        (string-trim env2)))
"##,
        expect,
    );
}

#[test]
fn div_cx106_subprocess_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX106=v" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX106"))))
      (exit-code (let ((p (make-process :name "neo-cx106-mega-ec"
                                          :command '("sh" "-c" "exit 5"))))
                   (accept-process-output p 2)
                   (process-exit-status p))))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (let ((buf (get-buffer-create " *neo-cx106-mega*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (insert "Subprocess mega test buffer content")
      (put-text-property 1 10 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)))
    (let ((p (make-process :name "neo-cx106-mega-p"
                           :command '("sh" "-c" "printf 'SUB'")
                           :buffer buf)))
      (accept-process-output p 1)
      (sit-for 0.05))
    (let ((content (with-current-buffer buf (buffer-string))))
      (with-current-buffer buf
        (widen)
        (let ((state (list timer-fired env-val exit-code
                           content (length content)
                           (length (overlays-in 1 20))
                           (text-properties-at 1))))
          (undo)
          (kill-buffer buf)
          (list state (buffer-string))))))
"##,
        expect,
    );
}
