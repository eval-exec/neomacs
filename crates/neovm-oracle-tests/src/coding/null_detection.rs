//! Runtime NUL-byte detection policy and definition attributes.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// GNU's C-level NUL flag follows dynamic bindings even when eval supplies an
/// opposing lexical binding of the same symbol.
#[test]
fn coding_null_byte_inhibition_ignores_lexical_eval_environment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(mapcar
             (lambda (dynamic)
               (let ((inhibit-null-byte-detection dynamic))
                 (eval
                  '(let ((bytes (unibyte-string 0 231 161 172 10)))
                     (list
                      (detect-coding-string bytes t)
                      (with-temp-buffer
                        (set-buffer-multibyte nil)
                        (insert bytes)
                        (detect-coding-region (point-min) (point-max) t))
                      (mapcar
                       (lambda (coding)
                         (let* ((decoded (decode-coding-string bytes coding))
                                (used last-coding-system-used))
                           (list (equal decoded "\0硬\n") used)))
                       '(undecided prefer-utf-8))))
                  (list (cons 'inhibit-null-byte-detection (not dynamic))))))
             '(nil t))"#,
        expect_test::expect![[
            r#""OK ((no-conversion no-conversion ((nil no-conversion) (nil no-conversion))) (utf-8-unix utf-8-unix ((t utf-8-unix) (t utf-8-unix))))""#
        ]],
    );
}

/// Synchronous subprocess detection honors the live NUL flag before selecting
/// the decoder, for both direct output and region input.
#[test]
fn coding_null_byte_inhibition_synchronous_process_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(mapcar
             (lambda (flag)
               (let ((inhibit-null-byte-detection flag)
                     (coding-system-for-read 'undecided)
                     (coding-system-for-write 'no-conversion))
                 (list
                  (with-temp-buffer
                    (let* ((status
                            (call-process "sh" nil t nil "-c"
                                          "printf '\\000\\347\\241\\254\\012'"))
                           (used last-coding-system-used))
                      (list status (equal (buffer-string) "\0硬\n") used)))
                  (with-temp-buffer
                    (let* ((destination (current-buffer))
                           (status
                            (with-temp-buffer
                              (set-buffer-multibyte nil)
                              (insert (unibyte-string 0 231 161 172 10))
                              (call-process-region
                               (point-min) (point-max) "cat" nil destination nil)))
                           (used last-coding-system-used))
                      (list status (equal (buffer-string) "\0硬\n") used))))))
             '(nil t))"#,
        expect_test::expect![[
            r#""OK (((0 nil no-conversion) (0 nil no-conversion)) ((0 t utf-8-unix) (0 t utf-8-unix)))""#
        ]],
    );
}

/// A dynamic NUL-inhibition binding affects both reporting and decoding, and
/// nested bindings restore the enclosing decision.
#[test]
fn coding_null_byte_inhibition_scopes_string_region_and_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((bytes (unibyte-string 0 231 161 172 10))
                  (wanted "\0硬\n")
                  (inhibit-null-byte-detection nil)
                  (probe
                   (lambda ()
                     (list (detect-coding-string bytes t)
                           (with-temp-buffer
                             (set-buffer-multibyte nil)
                             (insert bytes)
                             (detect-coding-region (point-min) (point-max) t))
                           (mapcar
                            (lambda (coding)
                              (let* ((decoded (decode-coding-string bytes coding))
                                     (used last-coding-system-used))
                                (list (equal decoded wanted) used)))
                            '(undecided prefer-utf-8))))))
             (list (funcall probe)
                   (let ((inhibit-null-byte-detection t))
                     (list (funcall probe)
                           (let ((inhibit-null-byte-detection nil)) (funcall probe))
                           (funcall probe)))
                   (funcall probe)))"#,
        expect_test::expect![[
            r#""OK ((no-conversion no-conversion ((nil no-conversion) (nil no-conversion))) ((utf-8-unix utf-8-unix ((t utf-8-unix) (t utf-8-unix))) (no-conversion no-conversion ((nil no-conversion) (nil no-conversion))) (utf-8-unix utf-8-unix ((t utf-8-unix) (t utf-8-unix)))) (no-conversion no-conversion ((nil no-conversion) (nil no-conversion))))""#
        ]],
    );
}

/// Definition-time t/nil/inherit attributes override the ambient NUL policy;
/// aliases and explicit EOL variants retain that definition's attributes.
#[test]
fn coding_null_byte_inhibition_definition_overrides_and_aliases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(progn
             (dolist (entry '((neovm-null-never . nil)
                              (neovm-null-always . t)
                              (neovm-null-follow . 0)))
               (define-coding-system
                 (car entry) "NUL detection attribute probe"
                 :coding-type 'undecided :mnemonic ?N :charset-list '(emacs)
                 :inhibit-null-byte-detection (cdr entry)))
             (define-coding-system-alias 'neovm-null-alias 'neovm-null-follow-dos)
             (define-coding-system-alias 'neovm-null-never-alias 'neovm-null-never)
             (define-coding-system-alias 'neovm-null-always-alias 'neovm-null-always)
             (define-coding-system-alias 'neovm-null-follow-alias 'neovm-null-follow)
             (let ((bytes (unibyte-string 231 161 172 0 13 10)))
               (mapcar
                (lambda (flag)
                  (let ((inhibit-null-byte-detection flag))
                    (mapcar
                     (lambda (coding)
                       (let* ((decoded (decode-coding-string bytes coding))
                              (used last-coding-system-used))
                         (list coding (equal decoded "硬\0\n") used)))
                     '(neovm-null-never neovm-null-always neovm-null-follow
                       neovm-null-follow-dos neovm-null-alias
                       neovm-null-never-alias-dos neovm-null-always-alias-dos
                       neovm-null-follow-alias-dos))))
                '(nil t))))"#,
        expect_test::expect![[
            r#""OK (((neovm-null-never nil no-conversion) (neovm-null-always t utf-8-dos) (neovm-null-follow nil no-conversion) (neovm-null-follow-dos nil no-conversion) (neovm-null-alias nil no-conversion) (neovm-null-never-alias-dos nil no-conversion) (neovm-null-always-alias-dos t utf-8-dos) (neovm-null-follow-alias-dos nil no-conversion)) ((neovm-null-never nil no-conversion) (neovm-null-always t utf-8-dos) (neovm-null-follow t utf-8-dos) (neovm-null-follow-dos t utf-8-dos) (neovm-null-alias t utf-8-dos) (neovm-null-never-alias-dos nil no-conversion) (neovm-null-always-alias-dos t utf-8-dos) (neovm-null-follow-alias-dos t utf-8-dos)))""#
        ]],
    );
}

/// Async buffers and filters detect under the binding active when output is
/// read, rather than the opposite binding used when the process was created.
#[test]
fn coding_null_byte_inhibition_asynchronous_process_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(mapcar
             (lambda (use-filter)
               (mapcar
                (lambda (flag)
                  (let* ((inhibit-null-byte-detection (not flag))
                         (buffer (generate-new-buffer " *null-policy-output*"))
                         (pieces nil)
                         (process
                          (make-process
                           :name "null-policy-output" :buffer buffer
                           :command '("cat") :connection-type 'pipe :noquery t
                           :coding '(undecided . no-conversion) :sentinel #'ignore
                           :filter (and use-filter
                                        (lambda (_ chunk) (push chunk pieces))))))
                    (unwind-protect
                        (let ((inhibit-null-byte-detection flag)
                              (deadline (+ (float-time) 5)))
                          (process-send-string process (unibyte-string 0 231 161 172 10))
                          (process-send-eof process)
                          (while (and (process-live-p process)
                                      (< (float-time) deadline))
                            (accept-process-output process 0.01))
                          (when (process-live-p process) (error "Process output timeout"))
                          (accept-process-output process 0.01)
                          (let ((output (if use-filter
                                            (apply #'concat (nreverse pieces))
                                          (with-current-buffer buffer (buffer-string)))))
                            (list (equal output "\0硬\n")
                                  (car (process-coding-system process)))))
                      (when (process-live-p process) (delete-process process))
                      (kill-buffer buffer))))
                '(nil t)))
             '(nil t))"#,
        expect_test::expect![[
            r#""OK (((nil no-conversion) (t utf-8-unix)) ((nil no-conversion) (t utf-8-unix)))""#
        ]],
    );
}

/// coding-system-put changes the exposed plist but does not replace GNU's
/// compiled, definition-time undecided detection attributes.
#[test]
fn coding_null_byte_inhibition_ignores_plist_only_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(progn
             (define-coding-system
               'neovm-null-defined-on "Always ignore NUL during detection"
               :coding-type 'undecided :mnemonic ?N :charset-list '(emacs)
               :inhibit-null-byte-detection t)
             (define-coding-system
               'neovm-null-defined-off "Always detect NUL"
               :coding-type 'undecided :mnemonic ?N :charset-list '(emacs)
               :inhibit-null-byte-detection nil)
             (coding-system-put 'neovm-null-defined-on :inhibit-null-byte-detection nil)
             (coding-system-put 'neovm-null-defined-off :inhibit-null-byte-detection t)
             (let ((bytes (unibyte-string 231 161 172 0 10)))
               (mapcar
                (lambda (flag)
                  (let ((inhibit-null-byte-detection flag))
                    (mapcar
                     (lambda (coding)
                       (let* ((decoded (decode-coding-string bytes coding))
                              (used last-coding-system-used))
                         (list (coding-system-get coding :inhibit-null-byte-detection)
                               (equal decoded "硬\0\n") used)))
                     '(neovm-null-defined-on neovm-null-defined-off))))
                '(nil t))))"#,
        expect_test::expect![[
            r#""OK (((nil t utf-8-unix) (t nil no-conversion)) ((nil t utf-8-unix) (t nil no-conversion)))""#
        ]],
    );
}

/// Registered undecided types use detection for sync output, async buffers, and
/// filters. The invalid byte must survive a NUL-selected no-conversion result;
/// aliases, EOL variants, definition attributes, and explicit decoders agree.
#[test]
fn coding_custom_undecided_process_output_preserves_null_selected_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(let ((rows (progn
  (dolist (entry '((neovm-process-null-follow . 0)
                   (neovm-process-null-on . t)
                   (neovm-process-null-off . nil)))
    (define-coding-system (car entry) "Custom process detection probe"
      :coding-type 'undecided :mnemonic ?N :charset-list '(emacs)
      :inhibit-null-byte-detection (cdr entry)))
  (define-coding-system-alias 'neovm-process-null-alias 'neovm-process-null-follow)
  (mapcar
   (lambda (sink)
    (mapcar
     (lambda (flag)
      (let ((inhibit-null-byte-detection flag))
       (mapcar
        (lambda (coding)
         (mapcar
          (lambda (bytes)
           (let ((buffer (generate-new-buffer " *custom-null-process*"))
                 (pieces nil) process output used)
            (unwind-protect
             (progn
              (if (eq sink 'sync)
               (with-current-buffer buffer
                (let ((coding-system-for-read coding)
                      (coding-system-for-write 'no-conversion))
                 (with-temp-buffer
                  (set-buffer-multibyte nil)
                  (insert bytes)
                  (call-process-region (point-min) (point-max) "cat" nil buffer nil)))
                (setq used last-coding-system-used))
               (setq process
                (make-process :name "custom-null-process" :buffer buffer
                 :command '("cat") :connection-type 'pipe :noquery t
                 :coding (cons coding 'no-conversion) :sentinel #'ignore
                 :filter (and (eq sink 'filter)
                           (lambda (_ chunk) (push chunk pieces)))))
               (let ((deadline (+ (float-time) 5)))
                (process-send-string process bytes)
                (process-send-eof process)
                (while (and (process-live-p process) (< (float-time) deadline))
                 (accept-process-output process 0.01))
                (when (process-live-p process) (error "Process output timeout"))
                (accept-process-output process 0.01))
               (setq used (car (process-coding-system process))))
              (setq output (if (eq sink 'filter)
                            (apply #'concat (nreverse pieces))
                            (with-current-buffer buffer (buffer-string))))
              (list (string-to-list output) used))
             (when (and process (process-live-p process)) (delete-process process))
             (kill-buffer buffer))))
          (list (unibyte-string 0 231 161 172 13 10)
                (unibyte-string 0 255 13 10))))
        '(neovm-process-null-follow neovm-process-null-alias-dos
          neovm-process-null-on-dos neovm-process-null-off
          undecided utf-8-dos no-conversion))))
     '(nil t)))
   '(sync buffer filter)))))
  (list (car rows) (equal (car rows) (cadr rows))
        (caddr rows)))"#,
        expect_test::expect![[
            r#""OK ((((((0 4194279 4194209 4194220 13 10) no-conversion) ((0 4194303 13 10) no-conversion)) (((0 4194279 4194209 4194220 13 10) no-conversion) ((0 4194303 13 10) no-conversion)) (((0 30828 10) utf-8-dos) ((0 255 10) iso-latin-1-dos)) (((0 4194279 4194209 4194220 13 10) no-conversion) ((0 4194303 13 10) no-conversion)) (((0 4194279 4194209 4194220 13 10) no-conversion) ((0 4194303 13 10) no-conversion)) (((0 30828 10) utf-8-dos) ((0 4194303 10) utf-8-dos)) (((0 4194279 4194209 4194220 13 10) no-conversion) ((0 4194303 13 10) no-conversion))) ((((0 30828 10) utf-8-dos) ((0 255 10) iso-latin-1-dos)) (((0 30828 10) utf-8-dos) ((0 255 10) iso-latin-1-dos)) (((0 30828 10) utf-8-dos) ((0 255 10) iso-latin-1-dos)) (((0 4194279 4194209 4194220 13 10) no-conversion) ((0 4194303 13 10) no-conversion)) (((0 30828 10) utf-8-dos) ((0 255 10) iso-latin-1-dos)) (((0 30828 10) utf-8-dos) ((0 4194303 10) utf-8-dos)) (((0 4194279 4194209 4194220 13 10) no-conversion) ((0 4194303 13 10) no-conversion)))) t (((((0 231 161 172 13 10) no-conversion) ((0 255 13 10) no-conversion)) (((0 231 161 172 13 10) no-conversion) ((0 255 13 10) no-conversion)) (((0 30828 10) utf-8-dos) ((0 255 10) iso-latin-1-dos)) (((0 231 161 172 13 10) no-conversion) ((0 255 13 10) no-conversion)) (((0 231 161 172 13 10) no-conversion) ((0 255 13 10) no-conversion)) (((0 30828 10) utf-8-dos) ((0 4194303 10) utf-8-dos)) (((0 231 161 172 13 10) no-conversion) ((0 255 13 10) no-conversion))) ((((0 30828 10) utf-8-dos) ((0 255 10) iso-latin-1-dos)) (((0 30828 10) utf-8-dos) ((0 255 10) iso-latin-1-dos)) (((0 30828 10) utf-8-dos) ((0 255 10) iso-latin-1-dos)) (((0 231 161 172 13 10) no-conversion) ((0 255 13 10) no-conversion)) (((0 30828 10) utf-8-dos) ((0 255 10) iso-latin-1-dos)) (((0 30828 10) utf-8-dos) ((0 4194303 10) utf-8-dos)) (((0 231 161 172 13 10) no-conversion) ((0 255 13 10) no-conversion)))))""#
        ]],
    );
}

/// BOM auto-detection uses registered UTF type and compiled BOM-pair metadata,
/// including aliases and EOL subsidiaries, across sync output and async sinks.
#[test]
fn coding_process_bom_detection_uses_registered_aliases_and_definitions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(let ((rows (progn
  (define-coding-system-alias 'neovm-bom-u8 'utf-8-auto)
  (define-coding-system-alias 'neovm-bom-u16 'utf-16)
  (define-coding-system 'neovm-custom-u8-auto "Custom UTF-8 BOM detection"
    :coding-type 'utf-8 :mnemonic ?U :charset-list '(unicode)
    :bom '(utf-8-with-signature . utf-8))
  (define-coding-system 'neovm-custom-u16-auto "Custom UTF-16 BOM detection"
    :coding-type 'utf-16 :mnemonic ?U :charset-list '(unicode)
    :bom '(utf-16le-with-signature . utf-16be-with-signature)
    :endian 'big)
  ;; These public plist edits do not change GNU's compiled BOM attribute.
  (coding-system-put 'neovm-custom-u8-auto :bom nil)
  (coding-system-put 'neovm-custom-u16-auto :bom nil)
  (mapcar
   (lambda (sink)
    (mapcar
     (lambda (flag)
      (let ((inhibit-null-byte-detection flag))
       (mapcar
        (lambda (entry)
         (let ((coding (car entry)))
         (mapcar
          (lambda (bytes)
           (let ((buffer (generate-new-buffer " *custom-null-process*"))
                 (pieces nil) process output used)
            (unwind-protect
             (progn
              (if (eq sink 'sync)
               (with-current-buffer buffer
                (let ((coding-system-for-read coding)
                      (coding-system-for-write 'no-conversion))
                 (with-temp-buffer
                  (set-buffer-multibyte nil)
                  (insert bytes)
                  (call-process-region (point-min) (point-max) "cat" nil buffer nil)))
                (setq used last-coding-system-used))
               (setq process
                (make-process :name "custom-null-process" :buffer buffer
                 :command '("cat") :connection-type 'pipe :noquery t
                 :coding (cons coding 'no-conversion) :sentinel #'ignore
                 :filter (and (eq sink 'filter)
                           (lambda (_ chunk) (push chunk pieces)))))
               (let ((deadline (+ (float-time) 5)))
                (process-send-string process bytes)
                (process-send-eof process)
                (while (and (process-live-p process) (< (float-time) deadline))
                 (accept-process-output process 0.01))
                (when (process-live-p process) (error "Process output timeout"))
                (accept-process-output process 0.01))
               (setq used (car (process-coding-system process))))
              (setq output (if (eq sink 'filter)
                            (apply #'concat (nreverse pieces))
                            (with-current-buffer buffer (buffer-string))))
              (list (string-to-list output) used))
             (when (and process (process-live-p process)) (delete-process process))
             (kill-buffer buffer))))
          (cdr entry))))
        (append
         (mapcar (lambda (coding)
                   (list coding (unibyte-string 239 187 191 65 13 10)
                                (unibyte-string 65 13 10)))
                 '(utf-8-auto neovm-bom-u8 neovm-bom-u8-dos
                   neovm-bom-u8-unix neovm-custom-u8-auto
                   neovm-custom-u8-auto-dos utf-8 utf-8-with-signature))
         (mapcar (lambda (coding)
                   (list coding (unibyte-string 255 254 65 0 13 0 10 0)
                                (unibyte-string 254 255 0 65 0 13 0 10)))
                 '(utf-16 neovm-bom-u16 neovm-bom-u16-dos
                   neovm-bom-u16-unix neovm-custom-u16-auto
                   neovm-custom-u16-auto-dos utf-16le-with-signature
                   utf-16le))
         (mapcar (lambda (coding)
                   (list coding (unibyte-string 0 65 0 13 0 10)
                                (unibyte-string 78 45 0 13 0 10)))
                 '(utf-16 neovm-bom-u16-dos neovm-bom-u16-unix))))))
     '(nil)))
   '(sync buffer filter)))))
  (list (car rows) (equal (car rows) (cadr rows))
        (equal (car rows) (caddr rows))))"#,
        expect_test::expect![[
            r#""OK ((((((65 10) utf-8-with-signature-dos) ((65 10) utf-8-dos)) (((65 10) utf-8-with-signature-dos) ((65 10) utf-8-dos)) (((65 10) utf-8-with-signature-dos) ((65 10) utf-8-dos)) (((65 13 10) utf-8-with-signature-unix) ((65 13 10) utf-8-unix)) (((65 10) utf-8-with-signature-dos) ((65 10) utf-8-dos)) (((65 10) utf-8-with-signature-dos) ((65 10) utf-8-dos)) (((65279 65 10) utf-8-dos) ((65 10) utf-8-dos)) (((65 10) utf-8-with-signature-dos) ((65 10) utf-8-with-signature-dos)) (((65 10) utf-16le-with-signature-dos) ((65 10) utf-16be-with-signature-dos)) (((65 10) utf-16le-with-signature-dos) ((65 10) utf-16be-with-signature-dos)) (((65 10) utf-16le-with-signature-dos) ((65 10) utf-16be-with-signature-dos)) (((65 13 10) utf-16le-with-signature-unix) ((65 13 10) utf-16be-with-signature-unix)) (((65 10) utf-16le-with-signature-dos) ((65 10) utf-16be-with-signature-dos)) (((65 10) utf-16le-with-signature-dos) ((65 10) utf-16be-with-signature-dos)) (((65 10) utf-16le-with-signature-dos) ((65534 16640 3328 2560) utf-16le-with-signature)) (((65279 65 10) utf-16le-dos) ((65534 16640 3328 2560) utf-16le)) (((65 10) utf-16-dos) ((20013 10) utf-16-dos)) (((65 10) neovm-bom-u16-dos) ((20013 10) neovm-bom-u16-dos)) (((65 13 10) neovm-bom-u16-unix) ((20013 13 10) neovm-bom-u16-unix)))) t t)""#
        ]],
    );
}

/// UTF-8 preference and fixed EOL are compiled definition attributes, including
/// aliases and custom names that carry no descriptive EOL suffix.
#[test]
fn coding_process_detection_preserves_registered_preference_and_eol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(let ((rows (progn
  (define-coding-system-alias 'neovm-prefer-alias 'prefer-utf-8)
  (define-coding-system 'neovm-prefer-defined "Compiled UTF-8 preference"
    :coding-type 'undecided :mnemonic ?U :charset-list '(emacs)
    :prefer-utf-8 t :inhibit-null-byte-detection 0)
  (coding-system-put 'neovm-prefer-defined :prefer-utf-8 nil)
  (dolist (entry '((neovm-fixed-lf . unix)
                   (neovm-fixed-crlf . dos)
                   (neovm-fixed-cr . mac)))
    (define-coding-system (car entry) "Registered EOL type"
      :coding-type 'undecided :mnemonic ?N :charset-list '(emacs)
      :eol-type (cdr entry) :inhibit-null-byte-detection 0))
  (define-coding-system-alias 'neovm-fixed-alias 'neovm-fixed-lf)
  (set-coding-system-priority 'iso-latin-1)
  (mapcar
   (lambda (sink)
    (mapcar
     (lambda (flag)
      (let ((inhibit-null-byte-detection flag))
       (mapcar
        (lambda (entry)
         (let ((coding (car entry)))
         (mapcar
          (lambda (bytes)
           (let ((buffer (generate-new-buffer " *custom-null-process*"))
                 (pieces nil) process output used)
            (unwind-protect
             (progn
              (if (eq sink 'sync)
               (with-current-buffer buffer
                (let ((coding-system-for-read coding)
                      (coding-system-for-write 'no-conversion))
                 (with-temp-buffer
                  (set-buffer-multibyte nil)
                  (insert bytes)
                  (call-process-region (point-min) (point-max) "cat" nil buffer nil)))
                (setq used last-coding-system-used))
               (setq process
                (make-process :name "custom-null-process" :buffer buffer
                 :command '("cat") :connection-type 'pipe :noquery t
                 :coding (cons coding 'no-conversion) :sentinel #'ignore
                 :filter (and (eq sink 'filter)
                           (lambda (_ chunk) (push chunk pieces)))))
               (let ((deadline (+ (float-time) 5)))
                (process-send-string process bytes)
                (process-send-eof process)
                (while (and (process-live-p process) (< (float-time) deadline))
                 (accept-process-output process 0.01))
                (when (process-live-p process) (error "Process output timeout"))
                (accept-process-output process 0.01))
               (setq used (car (process-coding-system process))))
              (setq output (if (eq sink 'filter)
                            (apply #'concat (nreverse pieces))
                            (with-current-buffer buffer (buffer-string))))
              (list (string-to-list output) used))
             (when (and process (process-live-p process)) (delete-process process))
             (kill-buffer buffer))))
          (cdr entry))))
        (mapcar (lambda (coding)
                  (list coding (unibyte-string 231 161 172 13 10)
                               (unibyte-string 65 13 10)))
                '(prefer-utf-8 neovm-prefer-alias neovm-prefer-alias-dos
                  neovm-prefer-defined neovm-prefer-defined-unix
                  undecided neovm-fixed-lf neovm-fixed-crlf
                  neovm-fixed-cr neovm-fixed-alias)))))
     '(nil)))
   '(sync buffer filter)))))
  (list (car rows) (equal (car rows) (cadr rows))
        (equal (car rows) (caddr rows))))"#,
        expect_test::expect![[
            r#""OK ((((((30828 10) utf-8-dos) ((65 10) prefer-utf-8-dos)) (((30828 10) utf-8-dos) ((65 10) prefer-utf-8-dos)) (((30828 10) utf-8-dos) ((65 10) neovm-prefer-alias-dos)) (((30828 10) utf-8-dos) ((65 10) neovm-prefer-defined-dos)) (((30828 13 10) utf-8-unix) ((65 13 10) neovm-prefer-defined-unix)) (((231 161 172 10) iso-latin-1-dos) ((65 10) undecided-dos)) (((231 161 172 13 10) iso-latin-1-unix) ((65 13 10) neovm-fixed-lf)) (((231 161 172 10) iso-latin-1-dos) ((65 10) neovm-fixed-crlf)) (((231 161 172 10 10) iso-latin-1-mac) ((65 10 10) neovm-fixed-cr)) (((231 161 172 13 10) iso-latin-1-unix) ((65 13 10) neovm-fixed-alias)))) t t)""#
        ]],
    );
}

/// A complete BOM followed by a split character selects the concrete decoder
/// before carryover is computed, including aliases with explicit EOL.
#[test]
fn coding_process_bom_alias_detection_preserves_partial_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(progn
 (define-coding-system-alias 'neovm-partial-u8 'utf-8-auto)
 (define-coding-system-alias 'neovm-partial-u16 'utf-16)
 (mapcar
  (lambda (entry)
   (let* ((buffer (generate-new-buffer " *bom-partial*"))
          (process (make-process :name "bom-partial" :buffer buffer
                    :command '("cat") :connection-type 'pipe :noquery t
                    :coding (cons (car entry) 'no-conversion) :sentinel #'ignore))
          (deadline (+ (float-time) 5)) first)
    (unwind-protect
     (progn
      (process-send-string process (cadr entry))
      (while (and (< (with-current-buffer buffer (buffer-size)) 1)
                  (< (float-time) deadline))
       (accept-process-output process 0.01))
      (setq first (car (process-coding-system process)))
      (process-send-string process (caddr entry))
      (process-send-eof process)
      (while (and (process-live-p process) (< (float-time) deadline))
       (accept-process-output process 0.01))
      (when (process-live-p process) (error "BOM partial process timeout"))
      (accept-process-output process 0.01)
      (list first (with-current-buffer buffer (string-to-list (buffer-string)))
            (car (process-coding-system process))))
     (when (process-live-p process) (delete-process process))
     (kill-buffer buffer))))
  (list (list 'neovm-partial-u8-dos (unibyte-string 239 187 191 65 231 161)
                                  (unibyte-string 172 13 10))
        (list 'neovm-partial-u16-dos (unibyte-string 255 254 65 0 45)
                                   (unibyte-string 78 13 0 10 0)))))"#,
        expect_test::expect![[
            r#""OK ((utf-8-with-signature-dos (65 30828 10) utf-8-with-signature-dos) (utf-16le-with-signature-dos (65 20013 10) utf-16le-with-signature-dos))""#
        ]],
    );
}

/// Priority selection and direct detection read the same compiled BOM/category
/// after plist edits.
#[test]
fn coding_priority_detection_preserves_compiled_bom_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(progn
 (define-coding-system 'neovm-priority-u8 "Compiled UTF-8 auto category"
   :coding-type 'utf-8 :mnemonic ?U :charset-list '(unicode)
   :bom '(utf-8-with-signature . utf-8))
 (define-coding-system 'neovm-priority-u16 "Compiled UTF-16 auto category"
   :coding-type 'utf-16 :mnemonic ?U :charset-list '(unicode)
   :bom '(utf-16le-with-signature . utf-16be-with-signature) :endian 'big)
 (let ((u8-bom (coding-system-get 'neovm-priority-u8 :bom)))
  (coding-system-put 'neovm-priority-u8 :bom nil)
  (coding-system-put 'neovm-priority-u16 :bom nil)
  (let ((probe
         (lambda (coding bytes)
          (set-coding-system-priority coding)
          (list (coding-system-get coding :category)
                (let* ((decoded (decode-coding-string bytes 'undecided))
                       (used last-coding-system-used))
                  (list (string-to-list decoded) used))
                (with-temp-buffer
                 (let ((output (current-buffer))
                       (coding-system-for-read 'undecided)
                       (coding-system-for-write 'no-conversion))
                  (with-temp-buffer
                   (set-buffer-multibyte nil) (insert bytes)
                   (call-process-region (point-min) (point-max) "cat" nil output nil)))
                 (list (string-to-list (buffer-string)) last-coding-system-used))))))
   (list (funcall probe 'neovm-priority-u8 (unibyte-string 239 187 191 65 13 10))
         (funcall probe 'neovm-priority-u16 (unibyte-string 255 254 65 0 13 0 10 0))))))"#,
        expect_test::expect![[
            r#""OK ((coding-category-utf-8-auto ((65 10) utf-8-with-signature-dos) ((65 10) utf-8-with-signature-dos)) (coding-category-utf-16-auto ((65 10) utf-16le-with-signature-dos) ((65 10) utf-16le-with-signature-dos)))""#
        ]],
    );
}

/// GNU retains custom decoding attributes when detection has no concrete
/// result; string identity fast paths and concrete rebases still skip hooks.
#[test]
fn coding_unchanged_detection_preserves_custom_hook_and_eol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(progn
 (defvar neovm-detection-hook-calls nil)
 (defun neovm-detection-hook (length)
  (push length neovm-detection-hook-calls)
  (goto-char (point-max)) (insert "!")
  (1+ length))
 (dolist (entry '((neovm-hook-undecided nil nil)
                  (neovm-hook-fixed-lf unix nil)
                  (neovm-hook-fixed-crlf dos nil)
                  (neovm-hook-fast nil t)
                  (neovm-hook-fast-lf unix t)))
  (define-coding-system (car entry) "Unchanged detection preserves hooks"
    :coding-type 'undecided :mnemonic ?N :charset-list '(emacs)
    :eol-type (cadr entry) :ascii-compatible-p (caddr entry)
    :post-read-conversion 'neovm-detection-hook
    :inhibit-null-byte-detection 0))
 (define-coding-system-alias 'neovm-hook-alias 'neovm-hook-undecided)
 (define-coding-system-alias 'neovm-hook-fixed-alias 'neovm-hook-fixed-lf)
 (mapcar
  (lambda (door)
   (mapcar
    (lambda (coding)
     (mapcar
      (lambda (bytes)
       (let ((neovm-detection-hook-calls nil) output used)
        (if (eq door 'string)
         (setq output (decode-coding-string bytes coding)
               used last-coding-system-used)
         (with-temp-buffer
          (if (eq door 'region)
           (progn (insert bytes)
            (decode-coding-region (point-min) (point-max) coding))
           (let ((destination (current-buffer))
                 (coding-system-for-read coding)
                 (coding-system-for-write 'no-conversion))
            (with-temp-buffer
             (set-buffer-multibyte nil) (insert bytes)
             (call-process-region (point-min) (point-max) "cat" nil destination nil))))
          (setq output (buffer-string) used last-coding-system-used)))
        (list (string-to-list output) used (nreverse neovm-detection-hook-calls))))
      (list (unibyte-string 65 66) (unibyte-string 65 13 10)
            (unibyte-string 231 161 172 13 10))))
    '(neovm-hook-undecided neovm-hook-alias-dos
      neovm-hook-fixed-lf neovm-hook-fixed-alias neovm-hook-fixed-crlf
      neovm-hook-fast neovm-hook-fast-lf)))
  '(string region process)))"#,
        expect_test::expect![[
            r#""OK (((((65 66 33) neovm-hook-undecided (2)) ((65 10 33) neovm-hook-undecided-dos (2)) ((30828 10) utf-8-dos nil)) (((65 66 33) neovm-hook-alias-dos (2)) ((65 10 33) neovm-hook-alias-dos (2)) ((30828 10) utf-8-dos nil)) (((65 66 33) neovm-hook-fixed-lf (2)) ((65 13 10 33) neovm-hook-fixed-lf (3)) ((30828 13 10) utf-8-unix nil)) (((65 66 33) neovm-hook-fixed-alias (2)) ((65 13 10 33) neovm-hook-fixed-alias (3)) ((30828 13 10) utf-8-unix nil)) (((65 66 33) neovm-hook-fixed-crlf (2)) ((65 10 33) neovm-hook-fixed-crlf (2)) ((30828 10) utf-8-dos nil)) (((65 66) neovm-hook-fast nil) ((65 10 33) neovm-hook-fast-dos (2)) ((30828 10) utf-8-dos nil)) (((65 66) neovm-hook-fast-lf nil) ((65 13 10) neovm-hook-fast-lf nil) ((30828 13 10) utf-8-unix nil))) ((((65 66 33) neovm-hook-undecided (2)) ((65 10 33) neovm-hook-undecided-dos (2)) ((30828 10) utf-8-dos nil)) (((65 66 33) neovm-hook-alias-dos (2)) ((65 10 33) neovm-hook-alias-dos (2)) ((30828 10) utf-8-dos nil)) (((65 66 33) neovm-hook-fixed-lf (2)) ((65 13 10 33) neovm-hook-fixed-lf (3)) ((30828 13 10) utf-8-unix nil)) (((65 66 33) neovm-hook-fixed-alias (2)) ((65 13 10 33) neovm-hook-fixed-alias (3)) ((30828 13 10) utf-8-unix nil)) (((65 66 33) neovm-hook-fixed-crlf (2)) ((65 10 33) neovm-hook-fixed-crlf (2)) ((30828 10) utf-8-dos nil)) (((65 66 33) neovm-hook-fast (2)) ((65 10 33) neovm-hook-fast-dos (2)) ((30828 10) utf-8-dos nil)) (((65 66 33) neovm-hook-fast-lf (2)) ((65 13 10 33) neovm-hook-fast-lf (3)) ((30828 13 10) utf-8-unix nil))) ((((65 66 33) neovm-hook-undecided (2)) ((65 10 33) neovm-hook-undecided-dos (2)) ((30828 10) utf-8-dos nil)) (((65 66 33) neovm-hook-alias-dos (2)) ((65 10 33) neovm-hook-alias-dos (2)) ((30828 10) utf-8-dos nil)) (((65 66 33) neovm-hook-fixed-lf (2)) ((65 13 10 33) neovm-hook-fixed-lf (3)) ((30828 13 10) utf-8-unix nil)) (((65 66 33) neovm-hook-fixed-alias (2)) ((65 13 10 33) neovm-hook-fixed-alias (3)) ((30828 13 10) utf-8-unix nil)) (((65 66 33) neovm-hook-fixed-crlf (2)) ((65 10 33) neovm-hook-fixed-crlf (2)) ((30828 10) utf-8-dos nil)) (((65 66 33) neovm-hook-fast (2)) ((65 10 33) neovm-hook-fast-dos (2)) ((30828 10) utf-8-dos nil)) (((65 66 33) neovm-hook-fast-lf (2)) ((65 13 10 33) neovm-hook-fast-lf (3)) ((30828 13 10) utf-8-unix nil))))""#
        ]],
    );
}

/// A BOM-selected alias keeps its reported identity while its registered codec
/// and fixed EOL attributes govern decoding and requested EOL inheritance.
#[test]
fn coding_bom_target_aliases_keep_codec_identity_and_fixed_eol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(progn
 (define-coding-system-alias 'neovm-target-u8-sig 'utf-8-with-signature)
 (define-coding-system-alias 'neovm-target-u8-plain 'utf-8)
 (define-coding-system-alias 'neovm-target-u8-fixed-sig 'utf-8-with-signature-unix)
 (define-coding-system-alias 'neovm-target-u8-fixed-plain 'utf-8-unix)
 (define-coding-system-alias 'neovm-target-u16-le 'utf-16le-with-signature)
 (define-coding-system-alias 'neovm-target-u16-be 'utf-16be-with-signature)
 (define-coding-system-alias 'neovm-target-raw 'no-conversion)
 (dolist (entry '((neovm-target-auto-u8 utf-8 neovm-target-u8-sig . neovm-target-u8-plain)
                  (neovm-target-auto-fixed utf-8 neovm-target-u8-fixed-sig . neovm-target-u8-fixed-plain)
                  (neovm-target-auto-u16 utf-16 neovm-target-u16-le . neovm-target-u16-be)
                  (neovm-target-auto-raw utf-8 neovm-target-raw . neovm-target-raw)))
  (define-coding-system (car entry) "BOM target alias attributes"
   :coding-type (cadr entry) :mnemonic ?U :charset-list '(unicode)
   :bom (cddr entry) :endian 'big))
 (defun neovm-target-probe (door coding bytes)
  (condition-case error-data
   (let (output used)
    (if (eq door 'string)
     (setq output (decode-coding-string bytes coding) used last-coding-system-used)
     (with-temp-buffer
      (let ((destination (current-buffer))
            (coding-system-for-read coding) (coding-system-for-write 'no-conversion))
       (with-temp-buffer
        (set-buffer-multibyte nil) (insert bytes)
        (call-process-region (point-min) (point-max) "cat" nil destination nil)))
      (setq output (buffer-string) used last-coding-system-used)))
    (list (string-to-list output) (multibyte-string-p output) used))
   (error (list 'error error-data))))
 (mapcar
  (lambda (door)
   (mapcar
    (lambda (entry)
     (mapcar (lambda (bytes) (neovm-target-probe door (car entry) bytes)) (cdr entry)))
    (append
     (mapcar (lambda (coding)
               (list coding (unibyte-string 239 187 191 65)
                            (unibyte-string 239 187 191 65 13 10)
                            (unibyte-string 65 13 10)))
             '(neovm-target-auto-u8 neovm-target-auto-u8-unix neovm-target-auto-u8-dos
               neovm-target-auto-fixed-dos neovm-target-auto-fixed-mac
               neovm-target-auto-raw neovm-target-auto-raw-dos neovm-target-auto-raw-mac
               neovm-target-u8-sig neovm-target-raw))
     (mapcar (lambda (coding)
               (list coding (unibyte-string 255 254 65 0)
                            (unibyte-string 255 254 65 0 13 0 10 0)
                            (unibyte-string 254 255 0 65 0 13 0 10)))
             '(neovm-target-auto-u16 neovm-target-auto-u16-unix neovm-target-auto-u16-dos
               neovm-target-u16-le)))))
  '(string process)))"#,
        expect_test::expect![[
            r#""OK (((((65) t neovm-target-u8-sig) ((65 10) t utf-8-with-signature-dos) ((65 10) t utf-8-dos)) (((65) t utf-8-with-signature-unix) ((65 13 10) t utf-8-with-signature-unix) ((65 13 10) t utf-8-unix)) (((65) t utf-8-with-signature-dos) ((65 10) t utf-8-with-signature-dos) ((65 10) t utf-8-dos)) (((65) t neovm-target-u8-fixed-sig) ((65 13 10) t neovm-target-u8-fixed-sig) ((65 13 10) t neovm-target-u8-fixed-plain)) (((65) t neovm-target-u8-fixed-sig) ((65 13 10) t neovm-target-u8-fixed-sig) ((65 13 10) t neovm-target-u8-fixed-plain)) (((239 187 191 65) nil neovm-target-raw) ((239 187 191 65 13 10) nil neovm-target-raw) ((65 13 10) nil neovm-target-raw)) (((239 187 191 65) nil neovm-target-raw) ((239 187 191 65 13 10) nil neovm-target-raw) ((65 13 10) nil neovm-target-raw)) (((239 187 191 65) nil neovm-target-raw) ((239 187 191 65 13 10) nil neovm-target-raw) ((65 13 10) nil neovm-target-raw)) (((65) t neovm-target-u8-sig) ((65 10) t utf-8-with-signature-dos) ((65 10) t utf-8-with-signature-dos)) (((239 187 191 65) nil neovm-target-raw) ((239 187 191 65 13 10) nil neovm-target-raw) ((65 13 10) t neovm-target-raw)) (((65) t neovm-target-u16-le) ((65 10) t utf-16le-with-signature-dos) ((65 10) t utf-16be-with-signature-dos)) (((65) t utf-16le-with-signature-unix) ((65 13 10) t utf-16le-with-signature-unix) ((65 13 10) t utf-16be-with-signature-unix)) (((65) t utf-16le-with-signature-dos) ((65 10) t utf-16le-with-signature-dos) ((65 10) t utf-16be-with-signature-dos)) (((65) t neovm-target-u16-le) ((65 10) t utf-16le-with-signature-dos) ((65534 16640 3328 2560) t neovm-target-u16-le))) ((((65) t neovm-target-u8-sig) ((65 10) t utf-8-with-signature-dos) ((65 10) t utf-8-dos)) (((65) t utf-8-with-signature-unix) ((65 13 10) t utf-8-with-signature-unix) ((65 13 10) t utf-8-unix)) (((65) t utf-8-with-signature-dos) ((65 10) t utf-8-with-signature-dos) ((65 10) t utf-8-dos)) (((65) t neovm-target-u8-fixed-sig) ((65 13 10) t neovm-target-u8-fixed-sig) ((65 13 10) t neovm-target-u8-fixed-plain)) (((65) t neovm-target-u8-fixed-sig) ((65 13 10) t neovm-target-u8-fixed-sig) ((65 13 10) t neovm-target-u8-fixed-plain)) (((4194287 4194235 4194239 65) t neovm-target-raw) ((4194287 4194235 4194239 65 13 10) t neovm-target-raw) ((65 13 10) t neovm-target-raw)) (((4194287 4194235 4194239 65) t neovm-target-raw) ((4194287 4194235 4194239 65 13 10) t neovm-target-raw) ((65 13 10) t neovm-target-raw)) (((4194287 4194235 4194239 65) t neovm-target-raw) ((4194287 4194235 4194239 65 13 10) t neovm-target-raw) ((65 13 10) t neovm-target-raw)) (((65) t neovm-target-u8-sig) ((65 10) t utf-8-with-signature-dos) ((65 10) t utf-8-with-signature-dos)) (((4194287 4194235 4194239 65) t neovm-target-raw) ((4194287 4194235 4194239 65 13 10) t neovm-target-raw) ((65 13 10) t neovm-target-raw)) (((65) t neovm-target-u16-le) ((65 10) t utf-16le-with-signature-dos) ((65 10) t utf-16be-with-signature-dos)) (((65) t utf-16le-with-signature-unix) ((65 13 10) t utf-16le-with-signature-unix) ((65 13 10) t utf-16be-with-signature-unix)) (((65) t utf-16le-with-signature-dos) ((65 10) t utf-16le-with-signature-dos) ((65 10) t utf-16be-with-signature-dos)) (((65) t neovm-target-u16-le) ((65 10) t utf-16le-with-signature-dos) ((65534 16640 3328 2560) t neovm-target-u16-le))))""#
        ]],
    );
}

#[test]
fn coding_utf8_bom_detection_requires_a_following_byte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(progn
 (define-coding-system 'neovm-three-auto "Exact BOM length"
   :coding-type 'utf-8 :mnemonic ?U :charset-list '(unicode)
   :bom '(utf-8-with-signature . utf-8))
 (define-coding-system-alias 'neovm-three-alias 'neovm-three-auto)
 (mapcar
  (lambda (door)
   (mapcar
    (lambda (coding)
     (mapcar
      (lambda (bytes)
       (let (output used)
        (cond
         ((eq door 'string) (setq output (decode-coding-string bytes coding)
                                 used last-coding-system-used))
         ((eq door 'region)
          (with-temp-buffer
           (set-buffer-multibyte nil) (insert bytes)
           (setq output (decode-coding-region (point-min) (point-max) coding t)
                 used last-coding-system-used)))
         ((eq door 'process)
          (with-temp-buffer
           (let ((destination (current-buffer))
                 (coding-system-for-read coding) (coding-system-for-write 'no-conversion))
            (with-temp-buffer
             (set-buffer-multibyte nil) (insert bytes)
             (call-process-region (point-min) (point-max) "cat" nil destination nil)))
           (setq output (buffer-string) used last-coding-system-used)))
         (t
          (let ((file (make-temp-file "neovm-bom-length-")))
           (unwind-protect
            (progn
             (let ((coding-system-for-write 'no-conversion))
              (write-region bytes nil file nil 'silent))
             (with-temp-buffer
              (let ((coding-system-for-read coding)) (insert-file-contents file))
              (setq output (buffer-string) used last-coding-system-used)))
            (delete-file file)))))
        (list (string-to-list output) used)))
      (list (unibyte-string 239 187 191) (unibyte-string 239 187 191 65))))
    '(utf-8-auto neovm-three-auto neovm-three-alias utf-8-with-signature)))
  '(string region process file)))"#,
        expect_test::expect![[
            r#""OK (((((65279) utf-8) ((65) utf-8-with-signature)) (((65279) utf-8) ((65) utf-8-with-signature)) (((65279) utf-8) ((65) utf-8-with-signature)) ((nil utf-8-with-signature) ((65) utf-8-with-signature))) ((((65279) utf-8) ((65) utf-8-with-signature)) (((65279) utf-8) ((65) utf-8-with-signature)) (((65279) utf-8) ((65) utf-8-with-signature)) ((nil utf-8-with-signature) ((65) utf-8-with-signature))) ((((65279) utf-8) ((65) utf-8-with-signature)) (((65279) utf-8) ((65) utf-8-with-signature)) (((65279) utf-8) ((65) utf-8-with-signature)) ((nil utf-8-with-signature) ((65) utf-8-with-signature))) ((((65279) utf-8) ((65) utf-8-with-signature)) (((65279) utf-8) ((65) utf-8-with-signature)) (((65279) utf-8) ((65) utf-8-with-signature)) ((nil utf-8-with-signature) ((65) utf-8-with-signature))))""#
        ]],
    );
}

#[test]
fn coding_bom_target_aliases_keep_partial_output_and_raw_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(progn
 (define-coding-system-alias 'neovm-partial-target-u8 'utf-8-with-signature)
 (define-coding-system-alias 'neovm-partial-target-le 'utf-16le-with-signature)
 (define-coding-system-alias 'neovm-partial-target-be 'utf-16be-with-signature)
 (define-coding-system-alias 'neovm-partial-target-raw 'no-conversion)
 (define-coding-system 'neovm-partial-auto-u8 "Selected alias continuation"
  :coding-type 'utf-8 :mnemonic ?U :charset-list '(unicode)
  :bom '(neovm-partial-target-u8 . utf-8))
 (define-coding-system 'neovm-partial-auto-u16 "UTF16 alias continuation"
  :coding-type 'utf-16 :mnemonic ?U :charset-list '(unicode)
  :bom '(neovm-partial-target-le . neovm-partial-target-be) :endian 'big)
 (define-coding-system 'neovm-partial-auto-raw "Raw alias consumes every byte"
  :coding-type 'utf-8 :mnemonic ?U :charset-list '(unicode)
  :bom '(neovm-partial-target-raw . neovm-partial-target-raw))
 (mapcar
  (lambda (filter)
   (mapcar
    (lambda (entry)
     (let* ((buffer (generate-new-buffer " *target-partial*")) (pieces nil)
            (process (make-process :name "target-partial" :buffer buffer
             :command '("cat") :connection-type 'pipe :noquery t
             :coding (cons (car entry) 'no-conversion) :sentinel #'ignore
             :filter (and filter (lambda (_ chunk) (push chunk pieces)))))
            (deadline (+ (float-time) 5)) first)
      (unwind-protect
       (progn
        (process-send-string process (cadr entry))
        (while (and (< (if filter (length (apply #'concat pieces))
                        (with-current-buffer buffer (buffer-size))) 1)
                    (< (float-time) deadline))
         (accept-process-output process 0.01))
        (setq first (car (process-coding-system process)))
        (process-send-string process (caddr entry))
        (process-send-eof process)
        (while (and (process-live-p process) (< (float-time) deadline))
         (accept-process-output process 0.01))
        (when (process-live-p process) (error "Target alias output timeout"))
        (accept-process-output process 0.01)
        (let ((output (if filter (apply #'concat (nreverse pieces))
                       (with-current-buffer buffer (buffer-string)))))
         (list first (string-to-list output) (multibyte-string-p output)
               (car (process-coding-system process)))))
       (when (process-live-p process) (delete-process process))
       (kill-buffer buffer))))
    (list (list 'neovm-partial-auto-u8 (unibyte-string 239 187 191 65 231 161)
                                     (unibyte-string 172 13 10))
          (list 'neovm-partial-auto-u16 (unibyte-string 255 254 65 0 45)
                                      (unibyte-string 78 13 0 10 0))
          (list 'neovm-partial-auto-raw-dos (unibyte-string 239 187 191 65 231)
                                          (unibyte-string 161 172 13 10)))))
  '(nil t)))"#,
        expect_test::expect![[
            r#""OK (((neovm-partial-target-u8 (65 30828 10) t utf-8-with-signature-dos) (neovm-partial-target-le (65 20013 10) t utf-16le-with-signature-dos) (neovm-partial-target-raw (4194287 4194235 4194239 65 4194279 4194209 4194220 13 10) t neovm-partial-target-raw)) ((neovm-partial-target-u8 (65 30828 10) t utf-8-with-signature-dos) (neovm-partial-target-le (65 20013 10) t utf-16le-with-signature-dos) (neovm-partial-target-raw (239 187 191 65 231 161 172 13 10) nil neovm-partial-target-raw)))""#
        ]],
    );
}
