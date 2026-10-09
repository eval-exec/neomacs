//! Coding detection follows the active language environment and coding priority.
use neovm_core::emacs_core::load::create_bootstrap_evaluator_cached;

#[test]
fn chinese_gb_detects_gbk_extension_bytes_in_strings_and_regions() {
    let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap evaluator");
    let result = eval
        .eval_str(
            r#"(progn
                 (set-language-environment 'Chinese-GB)
                 (let ((bytes (unibyte-string #x86 #xaa #xe0 #xc2 #x0a)))
                   (list (seq-take (detect-coding-string bytes) 3)
                         (detect-coding-string bytes t)
                         (with-temp-buffer
                           (set-buffer-multibyte nil) (insert bytes)
                           (detect-coding-region (point-min) (point-max) t))
                         (string-to-list (decode-coding-string bytes 'gbk)))))"#,
        )
        .unwrap();
    // Independently measured with GNU Emacs 31.1, including decoded characters.
    assert_eq!(
        format!("{result}"),
        "((chinese-gbk-unix japanese-shift-jis-unix raw-text-unix) chinese-gbk-unix chinese-gbk-unix (21872 21990 10))"
    );
}

#[test]
fn preferred_windows_1252_detects_c1_bytes_including_unmapped_codes() {
    let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap evaluator");
    let result = eval
        .eval_str(
            r#"(progn
                 (prefer-coding-system 'windows-1252)
                 (mapcar (lambda (code)
                           (detect-coding-string (unibyte-string code #x20 #x31 #x30 #x0a)))
                         '(#x80 #x81 #x8d #x9f)))"#,
        )
        .unwrap();
    assert_eq!(
        format!("{result}"),
        "((windows-1252-unix raw-text-unix) (windows-1252-unix raw-text-unix) (windows-1252-unix raw-text-unix) (windows-1252-unix emacs-mule-unix raw-text-unix))"
    );
}

#[test]
fn gbk_detection_uses_code_space_and_rejects_invalid_or_incomplete_units() {
    let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap evaluator");
    let result = eval
        .eval_str(
            r#"(progn
                 (set-language-environment 'Chinese-GB)
                 (mapcar (lambda (codes)
                           (and (memq 'chinese-gbk
                                      (detect-coding-string (apply #'unibyte-string codes))) t))
                         '((#x9f #x40) (#x81 #x80) (#x81 #x40) (#x86 #xb4)
                           (#x88 #xd2) (#xa0 #x40) (#xaa #x40) (#xfe #x40)
                           (#xe9 #x46) (#x81 #x7f) (#x81) (#x81 #x3f)
                           (#x81 #xff) (#xff #x40) (#x86 #xaa #x81))))"#,
        )
        .unwrap();
    // GNU accepts 0x817f structurally even though it has no GBK Unicode map.
    assert_eq!(
        format!("{result}"),
        "(t t t t t t t t t t nil nil nil nil nil)"
    );
}

#[test]
fn runtime_charsets_preserve_gnu_overlapping_candidate_consumption() {
    let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap evaluator");
    let result = eval
        .eval_str(
            r#"(progn
                 (define-charset 'detect-a "probe" :dimension 2
                                 :code-space [#x40 #x4f #x80 #x80] :code-offset #x2000)
                 (define-charset 'detect-b "probe" :dimension 2
                                 :code-space [#x60 #x6f #x80 #x80] :code-offset #x3000)
                 (define-coding-system 'detect-custom "probe" :coding-type 'charset
                                       :mnemonic ?x :charset-list '(ascii detect-a detect-b))
                 (prefer-coding-system 'detect-custom)
                 (mapcar (lambda (codes)
                           (and (memq 'detect-custom
                                      (detect-coding-string (apply #'unibyte-string codes))) t))
                         '((#x80 #x40) (#x80 #x60) (#x80 #x60 #x60)
                           (#x80 #x20) (#x80 #x20 #x20) (#x80))))"#,
        )
        .unwrap();
    // Measured GNU behavior: candidates share the cursor and do not backtrack.
    assert_eq!(format!("{result}"), "(t nil t nil t nil)");
}

#[test]
fn explicit_ascii_compatibility_skips_only_the_initial_ascii_prefix() {
    let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap evaluator");
    let result = eval
        .eval_str(
            r#"(progn
                 (define-charset 'detect-explicit "probe" :dimension 1
                                 :code-space [128 128] :code-offset #x2000)
                 (define-coding-system 'detect-explicit-coding "probe" :coding-type 'charset
                                       :mnemonic ?x :charset-list '(detect-explicit)
                                       :ascii-compatible-p t)
                 (prefer-coding-system 'detect-explicit-coding)
                 (list (detect-coding-string (unibyte-string 65 128))
                       (detect-coding-string (unibyte-string 128 65))))"#,
        )
        .unwrap();
    assert_eq!(
        format!("{result}"),
        "((detect-explicit-coding raw-text) (raw-text))"
    );
}

#[test]
fn multibyte_detection_keeps_raw_byte_units_separate_from_unicode_characters() {
    let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap evaluator");
    let result = eval
        .eval_str(
            r#"(progn
                 (set-language-environment 'Chinese-GB)
                 (let ((text (concat "漢" (string-to-multibyte
                                          (unibyte-string #x86 #xaa #xe0 #xc2 #x0a)))))
                   (list (detect-coding-string text t)
                         (with-temp-buffer
                           (insert text)
                           (detect-coding-region (point-min) (point-max) t)))))"#,
        )
        .unwrap();
    assert_eq!(format!("{result}"), "(chinese-gbk-unix chinese-gbk-unix)");
}
