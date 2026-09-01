//! Complex combo batch 9 — extend encode-coding-region no-op root, process exit
//! code root, plus more encoding/process edges: coding-system-for-write
//! propagation, process exit codes (0/42/255), process-status, encode/decode
//! region vs string consistency, coding-system-change-eol, process-id/tty.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx9_encode_region_utf16_vs_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((0 65 0 66) (0 65 0 66))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "AB"))
  (list (append (encode-coding-string s 'utf-16be) nil)
        (with-temp-buffer
          (insert s)
          (encode-coding-region (point-min) (point-max) 'utf-16be)
          (append (buffer-string) nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx9_encode_region_latin1_vs_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((99 97 102 233) (99 97 102 4194281))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café"))
  (list (append (encode-coding-string s 'latin-1) nil)
        (with-temp-buffer
          (insert s)
          (encode-coding-region (point-min) (point-max) 'latin-1)
          (append (buffer-string) nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx9_process_exit_code_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"finished\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (ev)
  (let ((p (make-process :name "neo-cx9-e0" :command '("true")
                         :sentinel (lambda (proc event) (setq ev event)))))
    (accept-process-output p 2))
  ev)
"##,
        expect,
    );
}

#[test]
fn div_cx9_process_exit_code_42() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"exited abnormally with code 42\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (ev)
  (let ((p (make-process :name "neo-cx9-e42" :command '("sh" "-c" "exit 42")
                         :sentinel (lambda (proc event) (setq ev event)))))
    (accept-process-output p 2))
  ev)
"##,
        expect,
    );
}

#[test]
fn div_cx9_process_exit_code_255() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"exited abnormally with code 255\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (ev)
  (let ((p (make-process :name "neo-cx9-e255" :command '("sh" "-c" "exit 255")
                         :sentinel (lambda (proc event) (setq ev event)))))
    (accept-process-output p 2))
  ev)
"##,
        expect,
    );
}

#[test]
fn div_cx9_process_status_after_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (exit 7 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx9-ps" :command '("sh" "-c" "exit 7"))))
  (accept-process-output p 2)
  (list (process-status p)
        (process-exit-status p)
        (process-live-p p)))
"##,
        expect,
    );
}

#[test]
fn div_cx9_coding_system_for_write_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function buffer-file-coding-system)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx9-csw-")))
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region "café\n" nil f nil 0))
  (prog1 (with-temp-buffer
           (let ((coding-system-for-read 'undecided))
             (insert-file-contents f))
           (list (buffer-string) (buffer-file-coding-system)))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx9_decode_region_vs_string_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"é€\" \"é€\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((bytes (unibyte-string 195 169 226 130 172)))
  (list (decode-coding-string bytes 'utf-8)
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert bytes)
          (decode-coding-region (point-min) (point-max) 'utf-8)
          (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx9_coding_system_change_eol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (utf-8-dos utf-8-mac 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (coding-system-change-eol-conversion 'utf-8 'dos)
      (coding-system-change-eol-conversion 'utf-8-unix 'mac)
      (coding-system-eol-type 'utf-8-dos))
"##,
        expect,
    );
}

#[test]
fn div_cx9_process_pid_and_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx9-pid" :command '("true"))))
  (prog1 (list (integerp (process-id p))
               (eq (process-type p) 'real)
               (processp p))
    (accept-process-output p 1)
    (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx9_set_process_coding_then_output_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café世界\" 11 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((p (make-process :name "neo-cx9-spc" :command '("printf" "%s" "café世界")
                         :buffer (current-buffer))))
    ;; The default sentinel can race with the first `accept-process-output` in
    ;; GNU and nondeterministically append the process-finished message here.
    ;; This case is about explicit process decoding, so suppress sentinel text.
    (set-process-sentinel p #'ignore)
    (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
    (accept-process-output p 1)
    (list (buffer-string) (string-bytes (buffer-string)) (length (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx9_encode_string_replacement_char_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (7 (97 32 98 32 99 32 100))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s "a€b中c😀d")
       (enc (encode-coding-string s 'iso-8859-1)))
  (list (length enc) (append enc nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx9_write_region_mustbenew() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:correctly-blocked :created t \"new\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx9-mbn-"))
      (f2 (make-temp-file "neo-cx9-mbn2-")))
  (ignore-errors (delete-file f2))
  (prog1 (list (condition-case e
                   (progn (write-region "data" nil f nil nil nil 'excl) :unexpected)
                 (file-already-exists :correctly-blocked))
               (condition-case e
                   (progn (write-region "new" nil f2 nil nil nil 'excl) :created)
                 (file-already-exists :unexpected))
               (file-exists-p f2)
               (with-temp-buffer
                 (insert-file-contents f2)
                 (buffer-string)))
    (ignore-errors (delete-file f))
    (ignore-errors (delete-file f2))))
"##,
        expect,
    );
}

#[test]
fn div_cx9_buffer_file_coding_system_write_read() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café世界\\n\" 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx8-bfc-")))
  (with-temp-buffer
    (insert "café世界\n")
    (set-buffer-file-coding-system 'utf-8-unix)
    (write-region (buffer-string) nil f nil 0))
  (prog1 (with-temp-buffer
           (let ((coding-system-for-read 'undecided))
             (insert-file-contents f))
           (list (buffer-string) (string-bytes (buffer-string))))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx9_accept_process_output_timeout_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx9-to" :command '("echo" "data")))
      (got nil))
  (set-process-filter p (lambda (proc msg) (setq got msg)))
  (accept-process-output p 0 nil t)
  (accept-process-output p 1)
  (if got t nil))
"##,
        expect,
    );
}

#[test]
fn div_cx9_process_mark_relocation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 12 \"beforemark\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "before")
  (let ((p (make-process :name "neo-cx9-pm" :command '("echo" "mark")
                         :buffer (current-buffer))))
    ;; Silence the default sentinel (its "Process ... finished" buffer message
    ;; is incidental noise) and drain to completion before reading, so the
    ;; process-mark position and buffer text are deterministic on both engines.
    (set-process-sentinel p #'ignore)
    (let ((m (process-mark p)))
      (while (process-live-p p) (accept-process-output p 1))
      (while (accept-process-output p 0))
      (list (markerp m) (marker-position m) (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx9_coding_system_category_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (coding-category-utf-8 coding-category-utf-8-sig coding-category-charset coding-category-emacs-mule)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (coding-system-category 'utf-8)
      (coding-system-category 'utf-8-with-signature)
      (coding-system-category 'latin-1)
      (coding-system-category 'emacs-mule))
"##,
        expect,
    );
}

#[test]
fn div_cx9_unicode_charset_vs_eightbit_in_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((4194248) eight-bit)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((decoded (decode-coding-string (unibyte-string 200) 'utf-8)))
  (list (append decoded nil)
        (char-charset (aref decoded 0))))
"##,
        expect,
    );
}

#[test]
fn div_cx9_multi_process_output_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"first\" \"second\" \"third\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (results)
  (dolist (cmd '(("echo" "first") ("echo" "second") ("echo" "third")))
    (let ((buf (generate-new-buffer " *neo-cx9-mp*")))
      (make-process :name (format "neo-cx9-mp%d" (length results))
                    :command cmd :buffer buf :connection-type 'pipe
                    :sentinel (lambda (proc ev)
                                (when (eq (process-status proc) 'exit)
                                  (push (with-current-buffer (process-buffer proc)
                                          (string-trim (buffer-string)))
                                        results)
                                  (kill-buffer (process-buffer proc))))))
    (sit-for 0.01))
  (sit-for 0.1)
  (nreverse results))
"##,
        expect,
    );
}

#[test]
fn div_cx9_encode_decode_region_identity_utf8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"café世界😀\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界😀")
  (let ((orig (buffer-string)))
    (encode-coding-region (point-min) (point-max) 'utf-8)
    (decode-coding-region (point-min) (point-max) 'utf-8)
    (list (equal orig (buffer-string)) (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx9_process_command_modify_after_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"echo\" \"original\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx9-cm" :command '("echo" "original"))))
  (accept-process-output p 1)
  (let ((cmd (process-command p)))
    (delete-process p)
    cmd))
"##,
        expect,
    );
}
