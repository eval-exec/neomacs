//! Coding identity, compiled signature attributes, and stream continuation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// GNU changes the candidate id during priority detection before deciding
/// whether to replace the decoder (coding.c:6627, 6743).
#[test]
fn coding_priority_nil_bom_target_keeps_candidate_and_original_decoder() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(progn
 (defun neovm-priority-probe (door coding bytes)
  (condition-case err
   (let (output used)
    (if (eq door 'string)
     (setq output (decode-coding-string bytes coding) used last-coding-system-used)
     (with-temp-buffer
      (let ((destination (current-buffer)) (coding-system-for-read coding)
            (coding-system-for-write 'no-conversion))
       (with-temp-buffer (set-buffer-multibyte nil) (insert bytes)
        (call-process-region (point-min) (point-max) "cat" nil destination nil)))
      (setq output (buffer-string) used last-coding-system-used)))
    (list (string-to-list output) used))
   (error (list 'error err))))
 (mapcar
  (lambda (slot)
   (let* ((coding (intern (format "neovm-nil-priority-%s" slot)))
          (_ (define-coding-system coding "Retained nil target"
               :coding-type 'utf-8 :mnemonic ?U :charset-list '(unicode)
               :bom (cons 'utf-8-with-signature 'utf-8)))
          (pair (coding-system-get coding :bom)))
    (if (eq slot 'car) (setcar pair nil) (setcdr pair nil))
    (coding-system-put coding :bom nil)
    (set-coding-system-priority coding)
    (mapcar
     (lambda (door)
      (mapcar
       (lambda (requested)
        (mapcar (lambda (bytes) (neovm-priority-probe door requested bytes))
          (if (eq slot 'car)
           (list (unibyte-string 239 187 191 65) (unibyte-string 239 187 191 65 13 10))
           (list (unibyte-string 231 161 172) (unibyte-string 231 161 172 13 10)))))
       (list coding 'undecided 'undecided-unix)))
     '(string process))))
  '(car cdr)))"#,
        expect_test::expect![[
            r#""OK ((((((65) neovm-nil-priority-car) ((65 10) neovm-nil-priority-car-dos)) (((4194287 4194235 4194239 65) neovm-nil-priority-car) ((4194287 4194235 4194239 65 10) neovm-nil-priority-car-dos)) (((4194287 4194235 4194239 65) neovm-nil-priority-car) ((4194287 4194235 4194239 65 10) neovm-nil-priority-car-dos))) ((((65) neovm-nil-priority-car) ((65 10) neovm-nil-priority-car-dos)) (((4194287 4194235 4194239 65) neovm-nil-priority-car) ((4194287 4194235 4194239 65 10) neovm-nil-priority-car-dos)) (((4194287 4194235 4194239 65) neovm-nil-priority-car) ((4194287 4194235 4194239 65 10) neovm-nil-priority-car-dos)))) (((((30828) neovm-nil-priority-cdr) ((30828 10) neovm-nil-priority-cdr-dos)) (((4194279 4194209 4194220) neovm-nil-priority-cdr) ((4194279 4194209 4194220 10) neovm-nil-priority-cdr-dos)) (((4194279 4194209 4194220) neovm-nil-priority-cdr) ((4194279 4194209 4194220 10) neovm-nil-priority-cdr-dos))) ((((30828) neovm-nil-priority-cdr) ((30828 10) neovm-nil-priority-cdr-dos)) (((4194279 4194209 4194220) neovm-nil-priority-cdr) ((4194279 4194209 4194220 10) neovm-nil-priority-cdr-dos)) (((4194279 4194209 4194220) neovm-nil-priority-cdr) ((4194279 4194209 4194220 10) neovm-nil-priority-cdr-dos)))))""#
        ]],
    );
}

/// The priority target's original fixed EOL wins; a vector EOL remains free
/// to inherit the request and resolve after decoding (coding.c:6471).
#[test]
fn coding_priority_fixed_eol_is_distinct_from_observed_eol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(progn
 (mapcar
  (lambda (priority)
   (set-coding-system-priority priority)
   (mapcar
    (lambda (door)
     (mapcar
      (lambda (coding)
       (let ((bytes (unibyte-string 231 161 172 13 65)) output used)
        (if (eq door 'string)
         (setq output (decode-coding-string bytes coding) used last-coding-system-used)
         (with-temp-buffer
          (let ((destination (current-buffer)) (coding-system-for-read coding)
                (coding-system-for-write 'no-conversion))
           (with-temp-buffer (set-buffer-multibyte nil) (insert bytes)
            (call-process-region (point-min) (point-max) "cat" nil destination nil)))
          (setq output (buffer-string) used last-coding-system-used)))
        (list (string-to-list output) used)))
      '(undecided undecided-unix undecided-mac)))
    '(string process)))
  '(utf-8 utf-8-dos utf-8-mac utf-8-unix)))"#,
        expect_test::expect![[
            r#""OK (((((30828 10 65) utf-8-mac) ((30828 13 65) utf-8-unix) ((30828 10 65) utf-8-mac)) (((30828 10 65) utf-8-mac) ((30828 13 65) utf-8-unix) ((30828 10 65) utf-8-mac))) ((((30828 13 65) utf-8-dos) ((30828 13 65) utf-8-dos) ((30828 13 65) utf-8-dos)) (((30828 13 65) utf-8-dos) ((30828 13 65) utf-8-dos) ((30828 13 65) utf-8-dos))) ((((30828 10 65) utf-8-mac) ((30828 10 65) utf-8-mac) ((30828 10 65) utf-8-mac)) (((30828 10 65) utf-8-mac) ((30828 10 65) utf-8-mac) ((30828 10 65) utf-8-mac))) ((((30828 13 65) utf-8-unix) ((30828 13 65) utf-8-unix) ((30828 13 65) utf-8-unix)) (((30828 13 65) utf-8-unix) ((30828 13 65) utf-8-unix) ((30828 13 65) utf-8-unix))))""#
        ]],
    );
}

#[test]
fn coding_custom_utf8_signature_metadata_preserves_conversion_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(progn
 (defvar neovm-signature-hooks nil)
 (defun neovm-signature-post (length)
  (push (list 'post length (string-to-list (buffer-string))) neovm-signature-hooks)
  (goto-char (point-max)) (insert "!") (1+ length))
 (defun neovm-signature-pre (start end)
  (push (list 'pre (string-to-list (buffer-substring start end))) neovm-signature-hooks)
  (goto-char (point-max)) (insert "?") nil)
 (define-coding-system 'neovm-hooked-signature "UTF8 BOM with Lisp conversions"
  :coding-type 'utf-8 :mnemonic ?U :charset-list '(unicode) :bom t
  :pre-write-conversion 'neovm-signature-pre :post-read-conversion 'neovm-signature-post)
 (define-coding-system-alias 'neovm-hooked-signature-alias 'neovm-hooked-signature)
 (define-coding-system 'neovm-select-hooked-signature "Select hooked custom target"
  :coding-type 'utf-8 :mnemonic ?U :charset-list '(unicode)
  :bom '(neovm-hooked-signature . utf-8))
 (coding-system-put 'neovm-hooked-signature :bom nil)
 (mapcar
  (lambda (coding)
   (let ((neovm-signature-hooks nil))
    (let* ((decoded (decode-coding-string (unibyte-string 239 187 191 65 13 10) coding))
           (decode-used last-coding-system-used)
           (after-decode (nreverse neovm-signature-hooks))
           (neovm-signature-hooks nil)
           (encoded (encode-coding-string "A\n" coding))
           (encode-used last-coding-system-used))
     (list (string-to-list decoded) decode-used after-decode
           (string-to-list encoded) encode-used (nreverse neovm-signature-hooks)))))
  '(neovm-hooked-signature neovm-hooked-signature-alias neovm-select-hooked-signature)))"#,
        expect_test::expect![[
            r#""OK (((65 10 33) neovm-hooked-signature-dos ((post 2 (65 10))) (239 187 191 65 10 63) neovm-hooked-signature ((pre (65 10)))) ((65 10 33) neovm-hooked-signature-dos ((post 2 (65 10))) (239 187 191 65 10 63) neovm-hooked-signature-alias ((pre (65 10)))) ((65 10 33) neovm-hooked-signature-dos ((post 2 (65 10))) (239 187 191 65 10) neovm-select-hooked-signature nil))""#
        ]],
    );
}

const STREAM_FIXTURE: &str = r#"
 (define-coding-system 'neovm-stream-signature "Registered signature"
  :coding-type 'utf-8 :mnemonic ?U :charset-list '(unicode) :bom t)
 (define-coding-system 'neovm-stream-auto "Select registered signature"
  :coding-type 'utf-8 :mnemonic ?U :charset-list '(unicode)
  :bom '(neovm-stream-signature . utf-8))
 (defvar neovm-stream-hook-reads nil)
 (defun neovm-stream-post (length)
  (push (list length (string-to-list (buffer-substring (point) (+ (point) length))))
        neovm-stream-hook-reads)
  length)
 (define-coding-system 'neovm-stream-hooked "Signature with post-read hook"
  :coding-type 'utf-8 :mnemonic ?U :charset-list '(unicode) :bom t
  :post-read-conversion 'neovm-stream-post)
 (defvar neovm-stream-pieces nil)
 (defun neovm-stream-filter (_process chunk) (push chunk neovm-stream-pieces))
 (defun neovm-stream-probe (coding chunks filter reset)
  (let* ((buffer (generate-new-buffer " *signature-state*"))
         (neovm-stream-pieces nil) (neovm-stream-hook-reads nil)
         (process (make-process :name "signature-state" :buffer buffer
          :command '("cat") :connection-type 'pipe :noquery t
          :coding (cons coding 'no-conversion) :sentinel #'ignore
          :filter (and filter 'neovm-stream-filter)))
         observations)
   (unwind-protect
    (progn
     (dolist (chunk chunks)
      (when observations
       (cond
        ((eq reset 'coding) (set-process-coding-system process coding 'no-conversion))
        ((eq reset 'buffer) (set-process-buffer process (process-buffer process)))
        ((eq reset 'filter) (set-process-filter process (process-filter process)))))
      (process-send-string process chunk)
      (let ((deadline (+ (float-time) 5)) (read nil))
       (while (and (not read) (< (float-time) deadline))
        (setq read (accept-process-output process 0.01)))
       (unless read (error "Signature chunk read timeout")))
      (push (list (string-to-list
                    (if filter (apply #'concat (reverse neovm-stream-pieces))
                     (with-current-buffer buffer (buffer-string))))
                  (car (process-coding-system process))
                  (reverse neovm-stream-hook-reads)) observations))
     (process-send-eof process)
     (let ((deadline (+ (float-time) 5)))
      (while (and (process-live-p process) (< (float-time) deadline))
       (accept-process-output process 0.01))
      (when (process-live-p process) (error "Signature process timeout")))
     (accept-process-output process 0.01)
     (list (nreverse observations)
           (string-to-list
            (if filter (apply #'concat (nreverse neovm-stream-pieces))
             (with-current-buffer buffer (buffer-string))))
           (car (process-coding-system process))
           (nreverse neovm-stream-hook-reads)))
    (when (process-live-p process) (delete-process process))
    (kill-buffer buffer))))
"#;

#[test]
fn coding_utf8_signature_is_consumed_once_per_process_stream() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        &format!(
            r#"(progn {STREAM_FIXTURE}
 (mapcar
  (lambda (filter)
   (mapcar (lambda (coding)
             (neovm-stream-probe coding
              (list (unibyte-string 239 187 191 65 13 10)
                    (unibyte-string 239 187 191 66)) filter nil))
     '(utf-8-with-signature utf-8 neovm-stream-signature neovm-stream-auto
       neovm-stream-hooked)))
  '(nil t)))"#
        ),
        expect_test::expect![[
            r#""OK ((((((65 10) utf-8-with-signature-dos nil) ((65 10 65279 66) utf-8-with-signature-dos nil)) (65 10 65279 66) utf-8-with-signature-dos nil) ((((65279 65 10) utf-8-dos nil) ((65279 65 10 65279 66) utf-8-dos nil)) (65279 65 10 65279 66) utf-8-dos nil) ((((65 10) neovm-stream-signature-dos nil) ((65 10 65279 66) neovm-stream-signature-dos nil)) (65 10 65279 66) neovm-stream-signature-dos nil) ((((65 10) neovm-stream-signature-dos nil) ((65 10 65279 66) neovm-stream-signature-dos nil)) (65 10 65279 66) neovm-stream-signature-dos nil) ((((65 10) neovm-stream-hooked-dos ((2 (65 10)))) ((65 10 65279 66) neovm-stream-hooked-dos ((2 (65 10)) (2 (65279 66))))) (65 10 65279 66) neovm-stream-hooked-dos ((2 (65 10)) (2 (65279 66))))) (((((65 10) utf-8-with-signature-dos nil) ((65 10 65279 66) utf-8-with-signature-dos nil)) (65 10 65279 66) utf-8-with-signature-dos nil) ((((65279 65 10) utf-8-dos nil) ((65279 65 10 65279 66) utf-8-dos nil)) (65279 65 10 65279 66) utf-8-dos nil) ((((65 10) neovm-stream-signature-dos nil) ((65 10 65279 66) neovm-stream-signature-dos nil)) (65 10 65279 66) neovm-stream-signature-dos nil) ((((65 10) neovm-stream-signature-dos nil) ((65 10 65279 66) neovm-stream-signature-dos nil)) (65 10 65279 66) neovm-stream-signature-dos nil) ((((65 10) neovm-stream-hooked-dos ((2 (65 10)))) ((65 10 65279 66) neovm-stream-hooked-dos ((2 (65 10)) (2 (65279 66))))) (65 10 65279 66) neovm-stream-hooked-dos ((2 (65 10)) (2 (65279 66)) (0 nil)))))""#
        ]],
    );
}

#[test]
fn coding_utf8_signature_split_initial_and_explicit_reset_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        &format!(
            r#"(progn {STREAM_FIXTURE}
 (list
  (neovm-stream-probe 'utf-8-with-signature
   (list (unibyte-string 239) (unibyte-string 187)
         (unibyte-string 191 65) (unibyte-string 239 187 191 66)) nil nil)
  (neovm-stream-probe 'utf-8-auto
   (list (unibyte-string 239 187 191) (unibyte-string 65)) nil nil)
  (mapcar (lambda (reset)
            (neovm-stream-probe 'utf-8-with-signature
             (list (unibyte-string 239 187 191 65)
                   (unibyte-string 239 187 191 66)) t reset))
    '(coding buffer filter))))"#
        ),
        expect_test::expect![[
            r#""OK ((((nil utf-8-with-signature nil) (nil utf-8-with-signature nil) ((65) utf-8-with-signature nil) ((65 65279 66) utf-8-with-signature nil)) (65 65279 66) utf-8-with-signature nil) ((((65279) utf-8 nil) ((65279 65) utf-8 nil)) (65279 65) utf-8 nil) (((((65) utf-8-with-signature nil) ((65 66) utf-8-with-signature nil)) (65 66) utf-8-with-signature nil) ((((65) utf-8-with-signature nil) ((65 66) utf-8-with-signature nil)) (65 66) utf-8-with-signature nil) ((((65) utf-8-with-signature nil) ((65 66) utf-8-with-signature nil)) (65 66) utf-8-with-signature nil)))""#
        ]],
    );
}

/// GNU merges stderr into the same coding stream. Neomacs captures the two
/// descriptors separately, but their delivery must share that continuation.
#[test]
fn coding_utf8_signature_continues_across_captured_stdout_and_stderr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        &format!(
            r#"(progn {STREAM_FIXTURE}
 (mapcar
  (lambda (coding)
   (let ((neovm-stream-hook-reads nil))
    (with-temp-buffer
     (let ((coding-system-for-read coding) (coding-system-for-write 'no-conversion))
      (call-process "sh" nil (list (current-buffer) t) nil "-c"
       "printf '\\357\\273\\277A\\r\\n'; printf '\\357\\273\\277B' >&2"))
     (list (string-to-list (buffer-string)) last-coding-system-used
           (nreverse neovm-stream-hook-reads)))))
  '(utf-8-with-signature neovm-stream-signature)))"#
        ),
        expect_test::expect![[
            r#""OK (((65 10 65279 66) utf-8-with-signature-dos nil) ((65 10 65279 66) neovm-stream-signature-dos nil))""#
        ]],
    );
}

/// GNU restricts the category scan for UTF-8 preference; the configured
/// category binding still selects the actual coding (coding.c:6619-6657).
#[test]
fn coding_prefer_utf8_keeps_registered_category_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(progn
 (define-coding-system 'neovm-prefer-review "UTF8 category preference"
  :coding-type 'undecided :mnemonic ?U :charset-list '(emacs) :prefer-utf-8 t)
 (define-coding-system-alias 'neovm-prefer-review-alias 'neovm-prefer-review-dos)
 (coding-system-put 'neovm-prefer-review :prefer-utf-8 nil)
 (define-coding-system 'neovm-prefer-target "Configured signature category"
  :coding-type 'utf-8 :mnemonic ?U :charset-list '(unicode) :bom t)
 (mapcar
  (lambda (priority)
   (set-coding-system-priority priority)
   (mapcar
    (lambda (door)
     (mapcar
      (lambda (coding)
       (mapcar
        (lambda (bytes)
         (let (output used)
          (if (eq door 'string)
           (setq output (decode-coding-string bytes coding) used last-coding-system-used)
           (with-temp-buffer
            (let ((destination (current-buffer)) (coding-system-for-read coding)
                  (coding-system-for-write 'no-conversion))
             (with-temp-buffer (set-buffer-multibyte nil) (insert bytes)
              (call-process-region (point-min) (point-max) "cat" nil destination nil)))
            (setq output (buffer-string) used last-coding-system-used)))
          (list (string-to-list output) used)))
        (list (unibyte-string 239 187 191 65)
              (unibyte-string 226 130 172) (unibyte-string 239 187 191))))
      '(neovm-prefer-review neovm-prefer-review-alias)))
    '(string process)))
  '(utf-8-with-signature neovm-prefer-target utf-8)))"#,
        expect_test::expect![[
            r#""OK ((((((65) utf-8-with-signature) ((8364) utf-8) ((65279) utf-8)) (((65) utf-8-with-signature-dos) ((8364) utf-8-dos) ((65279) utf-8-dos))) ((((65) utf-8-with-signature) ((8364) utf-8) ((65279) utf-8)) (((65) utf-8-with-signature-dos) ((8364) utf-8-dos) ((65279) utf-8-dos)))) (((((65) neovm-prefer-target) ((8364) utf-8) ((65279) utf-8)) (((65) neovm-prefer-target-dos) ((8364) utf-8-dos) ((65279) utf-8-dos))) ((((65) neovm-prefer-target) ((8364) utf-8) ((65279) utf-8)) (((65) neovm-prefer-target-dos) ((8364) utf-8-dos) ((65279) utf-8-dos)))) (((((65279 65) utf-8) ((8364) utf-8) ((65279) utf-8)) (((65279 65) utf-8-dos) ((8364) utf-8-dos) ((65279) utf-8-dos))) ((((65279 65) utf-8) ((8364) utf-8) ((65279) utf-8)) (((65279 65) utf-8-dos) ((8364) utf-8-dos) ((65279) utf-8-dos)))))""#
        ]],
    );
}

/// BOM-auto selection uses GNU's UTF-8 octet grammar, not Unicode scalar
/// validity; an incomplete non-final block follows detect_coding's tail rule.
#[test]
fn coding_bom_auto_uses_gnu_utf8_grammar_and_partial_tail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        &format!(
            r#"(progn {STREAM_FIXTURE}
 (define-coding-system 'neovm-grammar-review "GNU UTF8 detector grammar"
  :coding-type 'utf-8 :mnemonic ?U :charset-list '(unicode)
  :bom '(utf-8-with-signature . iso-latin-1))
 (list
  (mapcar
   (lambda (door)
    (mapcar
     (lambda (bytes)
      (let (output used)
       (if (eq door 'string)
        (setq output (decode-coding-string bytes 'neovm-grammar-review)
              used last-coding-system-used)
        (with-temp-buffer
         (let ((destination (current-buffer)) (coding-system-for-read 'neovm-grammar-review)
               (coding-system-for-write 'no-conversion))
          (with-temp-buffer (set-buffer-multibyte nil) (insert bytes)
           (call-process-region (point-min) (point-max) "cat" nil destination nil)))
         (setq output (buffer-string) used last-coding-system-used)))
       (list (string-to-list output) used)))
     (list (unibyte-string 65) (unibyte-string 239 187 191)
           (unibyte-string 237 160 128) (unibyte-string 239 187 191 65)
           (unibyte-string 244 144 128 128) (unibyte-string 248 136 128 128 128)
           (unibyte-string 237 160) (unibyte-string 237 160 65))))
   '(string process))
  (mapcar
   (lambda (filter)
    (neovm-stream-probe 'neovm-grammar-review
     (list (unibyte-string 237 160) (unibyte-string 128 65)) filter nil))
   '(nil t))))"#
        ),
        expect_test::expect![[
            r#""OK (((((65) iso-latin-1) ((239 187 191) iso-latin-1) ((237 160 128) iso-latin-1) ((65) utf-8-with-signature) ((244 144 128 128) iso-latin-1) ((2097152) neovm-grammar-review) ((4194285 4194208) neovm-grammar-review) ((4194285 4194208 65) neovm-grammar-review)) (((65) iso-latin-1) ((239 187 191) iso-latin-1) ((237 160 128) iso-latin-1) ((65) utf-8-with-signature) ((244 144 128 128) iso-latin-1) ((2097152) neovm-grammar-review) ((4194285 4194208) neovm-grammar-review) ((4194285 4194208 65) neovm-grammar-review))) (((((237 160) iso-latin-1 nil) ((237 160 128 65) iso-latin-1 nil)) (237 160 128 65) iso-latin-1 nil) ((((237 160) iso-latin-1 nil) ((237 160 128 65) iso-latin-1 nil)) (237 160 128 65) iso-latin-1 nil)))""#
        ]],
    );
}

/// Process coding setup resets the decoder specification, but the process's
/// pending byte tail survives (process.c:1312, 1404, 6312).
#[test]
fn coding_process_setup_preserves_pending_multibyte_carryover() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(progn
 (defvar neovm-carry-pieces nil)
 (defun neovm-carry-filter (_p text) (push text neovm-carry-pieces))
 (defun neovm-carry-probe (filter reset)
  (let* ((buffer (generate-new-buffer " *carry-reset*")) (neovm-carry-pieces nil)
         (process (make-process :name "carry-reset" :buffer buffer :command '("cat")
           :coding '(utf-8 . no-conversion) :connection-type 'pipe :noquery t
           :filter (and filter 'neovm-carry-filter) :sentinel #'ignore))
         (deadline (+ (float-time) 5)) first)
   (unwind-protect
    (progn
     (process-send-string process (unibyte-string 65 231 161))
     (while (and (< (if filter (length (apply #'concat neovm-carry-pieces))
                      (with-current-buffer buffer (buffer-size))) 1)
                 (< (float-time) deadline)) (accept-process-output process 0.01))
     (setq first (string-to-list (if filter (apply #'concat (reverse neovm-carry-pieces))
                                  (with-current-buffer buffer (buffer-string)))))
     (unless first (error "Pending character first-read timeout"))
     (cond ((eq reset 'filter) (set-process-filter process (process-filter process)))
           ((eq reset 'buffer) (set-process-buffer process (process-buffer process)))
           ((eq reset 'coding) (set-process-coding-system process 'utf-8 'no-conversion)))
     (process-send-string process (unibyte-string 172))
     (process-send-eof process)
     (while (and (process-live-p process) (< (float-time) deadline))
      (accept-process-output process 0.01))
     (when (process-live-p process) (error "Pending character process timeout"))
     (accept-process-output process 0.01)
     (list reset first (string-to-list
       (if filter (apply #'concat (nreverse neovm-carry-pieces))
        (with-current-buffer buffer (buffer-string))))))
    (when (process-live-p process) (delete-process process)) (kill-buffer buffer))))
 (mapcar (lambda (filter)
           (mapcar (lambda (reset) (neovm-carry-probe filter reset))
             '(none filter buffer coding)))
   '(nil t)))"#,
        expect_test::expect![[
            r#""OK (((none (65) (65 30828)) (filter (65) (65 30828)) (buffer (65) (65 30828)) (coding (65) (65 30828))) ((none (65) (65 30828)) (filter (65) (65 30828)) (buffer (65) (65 30828)) (coding (65) (65 30828))))""#
        ]],
    );
}

/// GNU's retained raw-text decoder still honors DOS lookahead and consumes
/// only the prefix before a trailing CR (coding.c:5267).
#[test]
fn coding_identity_only_raw_dos_keeps_trailing_cr_for_next_read() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(progn
 (defvar neovm-retained-pieces nil)
 (defvar neovm-retained-observed nil)
 (defun neovm-retained-filter (_p text) (push text neovm-retained-pieces))
 (defun neovm-retained-post (size)
  (when (> size 0)
   (push (list size (string-to-list (buffer-substring (point) (+ (point) size))))
         neovm-retained-observed)) size)
 (defun neovm-retained-probe (hook filter)
  (let* ((coding (if hook 'neovm-retained-hook-fixed 'neovm-retained-fixed))
         (_ (define-coding-system coding "Retained raw DOS"
          :coding-type 'utf-8 :mnemonic ?U :charset-list '(unicode) :eol-type 'dos
          :bom (cons 'utf-8-with-signature 'utf-8)
          :post-read-conversion (and hook 'neovm-retained-post)))
         (_ (setcdr (coding-system-get coding :bom) nil))
         (_ (coding-system-put coding :bom nil))
         (_ (set-coding-system-priority coding))
         (buffer (generate-new-buffer " *retained-raw-dos*"))
         (neovm-retained-pieces nil) (neovm-retained-observed nil)
         (process (make-process :name "retained-raw-dos" :buffer buffer :command '("cat")
          :coding '(undecided . no-conversion) :connection-type 'pipe :noquery t
          :filter (and filter 'neovm-retained-filter) :sentinel #'ignore))
         (deadline (+ (float-time) 5)) first)
   (unwind-protect
    (progn
     (process-send-string process (unibyte-string 195 169 13))
     (while (and (< (if filter (length (apply #'concat neovm-retained-pieces))
                      (with-current-buffer buffer (buffer-size))) 2)
                 (< (float-time) deadline)) (accept-process-output process 0.01))
     (setq first (string-to-list (if filter (apply #'concat (reverse neovm-retained-pieces))
                                  (with-current-buffer buffer (buffer-string)))))
     (unless first (error "Retained raw DOS first-read timeout"))
     (process-send-string process (unibyte-string 10))
     (process-send-eof process)
     (while (and (process-live-p process) (< (float-time) deadline))
      (accept-process-output process 0.01))
     (when (process-live-p process) (error "Retained raw DOS process timeout"))
     (accept-process-output process 0.01)
     (list hook filter first
      (string-to-list (if filter (apply #'concat (nreverse neovm-retained-pieces))
                        (with-current-buffer buffer (buffer-string))))
      (car (process-coding-system process)) (nreverse neovm-retained-observed)))
    (when (process-live-p process) (delete-process process)) (kill-buffer buffer))))
 (mapcar (lambda (hook)
           (mapcar (lambda (filter) (neovm-retained-probe hook filter)) '(nil t)))
   '(nil t)))"#,
        expect_test::expect![[
            r#""OK (((nil nil (4194243 4194217) (4194243 4194217 10) neovm-retained-fixed nil) (nil t (4194243 4194217) (4194243 4194217 10) neovm-retained-fixed nil)) ((t nil (4194243 4194217) (4194243 4194217 10) neovm-retained-hook-fixed ((2 (4194243 4194217)) (1 (10)))) (t t (4194243 4194217) (4194243 4194217 10) neovm-retained-hook-fixed ((2 (4194243 4194217)) (1 (10))))))""#
        ]],
    );
}
