//! Frozen outputs refreshed solely from GNU Emacs 31.1 with UPDATE_EXPECT=1.
use super::*;
use std::path::PathBuf;

fn gnu_expect(name: &str, form: &str, fixture: &str) -> String {
    if std::env::var("UPDATE_EXPECT").as_deref() != Ok("1") {
        return fixture.trim_end().to_owned();
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    std::fs::create_dir_all(root.join("../../tmp")).expect("GNU oracle temporary directory");
    let script = root
        .join("../../tmp")
        .join(format!("escapes-{name}-{}.el", std::process::id()));
    std::fs::write(
        &script,
        format!("(unless (equal emacs-version \"31.1\") (error \"Expected GNU Emacs 31.1\"))\n(let ((print-length nil) (print-level nil)) (prin1 {form}))\n"),
    )
    .unwrap();
    let emacs = crate::test_utils::gnu_oracle().executable_path();
    let sandbox = std::env::var_os("NEOVM_LISP_SANDBOX").unwrap_or_else(|| {
        "/home/exec/Projects/github.com/eval-exec/neomacs-main/tmp/v10x/sandbox-run.sh".into()
    });
    let result = std::process::Command::new(sandbox)
        .arg(emacs)
        .args(["-Q", "--batch", "-l"])
        .arg(script)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "GNU error: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    let path = root
        .join("src/emacs_core/runtime/value_reader/tests")
        .join(format!("escapes-{name}.expect"));
    let saved = format!("{}\n", output.trim_end());
    if std::fs::read_to_string(&path).ok().as_deref() != Some(saved.as_str()) {
        std::fs::write(path, saved).unwrap();
    }
    output.trim_end().to_owned()
}

pub(super) fn check(name: &str, form: &str, fixture: &str) {
    crate::test_utils::init_test_tracing();
    let expected = gnu_expect(name, form, fixture);
    let actual = crate::test_utils::runtime_startup_eval_one(form);
    assert_eq!(actual, format!("OK {expected}"));
}

#[test]
fn rea03_string_escapes_share_character_decoder() {
    check(
        "string",
        r#"(mapcar (lambda (source) (condition-case e
        (let ((s (car (read-from-string source))))
          (list (append s nil) (multibyte-string-p s))) (error e)))
      '("\"\\C-\\x41\"" "\"\\M-\\101\"" "\"\\M-\\x41\""
        "\"\\S-\\x61\"" "\"\\^\\x41\"" "\"\\C-\\101\""
        "\"\\M-\\200\"" "\"\\x41;b\"" "\"\\x8000041\""
        "\"\\x2000041\"" "\"\\x8000000\"" "\"\\x400000\""
        "\"\\U00110000\"" "\"\\U00200000\"" "\"\\S\""
        "\"\\M\"" "\"\\C\"" "\"\\H\"" "\"\\A\""
        "\"\\N\"" "\"\\x\"" "\"\\u12\"" "\"\\xffffffff\""
        "\"\\x123456789\"" "\"\\s-\"" "\"\\M-\\s-a\""
        "\"\\u00e9\\200\"" "\"\\200\\u00e9\""))"#,
        include_str!("escapes-string.expect"),
    );
}

#[test]
fn rea04_character_backslash_newline_is_error() {
    check(
        "newline",
        r#"(mapcar (lambda (source) (condition-case e (read-from-string source) (error e)))
        (list (concat "?\\" (string 10)) (concat "?\\" (string 10) "x")))"#,
        include_str!("escapes-newline.expect"),
    );
}

#[test]
fn rea05_named_characters_use_gnu_names() {
    check(
        "names",
        r#"(mapcar (lambda (name)
       (list (condition-case e (read-from-string (concat "?\\N{" name "}")) (error e))
             (condition-case e (append (car (read-from-string (concat "\"\\N{" name "}\""))) nil) (error e))))
       '("CJK IDEOGRAPH-4E00" "cjk ideograph-4e00" "CJK UNIFIED IDEOGRAPH-4E00"
         "TANGUT IDEOGRAPH-17000" "tangut ideograph-17000" "CJK IDEOGRAPH-3400"
         "CJK IDEOGRAPH-30000" "CJK IDEOGRAPH-4DFF" "CJK IDEOGRAPH-04E00"
         "VARIATION SELECTOR-17" "variation selector-256" "VARIATION SELECTOR-257"
         "BACKSPACE" "BELL" "BELL (BEL)" "GREEK SMALL LETTER LAMBDA"
         "TANGUT COMPONENT-001" "KHITAN SMALL SCRIPT CHARACTER-18B00"
         "CJK COMPATIBILITY IDEOGRAPH-F900"))"#,
        include_str!("escapes-names.expect"),
    );
}

#[test]
fn rea06_unterminated_names_signal_end_of_file() {
    check(
        "eof",
        r#"(mapcar (lambda (source) (condition-case e (read-from-string source) (error e)))
       '("?\\N{LATIN SMALL LETTER A" "\"\\N{" "\"\\N{LATIN SMALL LETTER A"))"#,
        include_str!("escapes-eof.expect"),
    );
}

#[test]
fn shared_decoder_preserves_character_and_string_domains() {
    assert_eq!(read1(r"?\200").as_fixnum(), Some(128));
    assert_eq!(read1(r"?\M-\101").as_fixnum(), Some(0x8000041));
    assert_eq!(read1(r#""\x41;b""#).as_str_owned().as_deref(), Some("A;b"));
    let ob = crate::emacs_core::symbol::Obarray::new();
    let error = read_one(
        "?\\\n",
        0,
        crate::emacs_core::eval::VariableView::canonical(
            crate::emacs_core::symbol::CanonicalAccess::TEST,
            &ob,
        ),
    )
    .unwrap_err();
    assert_eq!(error.kind, ReadErrorKind::Error);
    let error = read_one(
        r"?\N{LATIN SMALL LETTER A",
        0,
        crate::emacs_core::eval::VariableView::canonical(
            crate::emacs_core::symbol::CanonicalAccess::TEST,
            &ob,
        ),
    )
    .unwrap_err();
    assert_eq!(error.kind, ReadErrorKind::EndOfFile);
    for source in [r"?\Mz", r"?\Nq"] {
        let error = read_one(
            source,
            0,
            crate::emacs_core::eval::VariableView::canonical(
                crate::emacs_core::symbol::CanonicalAccess::TEST,
                &ob,
            ),
        )
        .unwrap_err();
        assert_eq!(
            error.position,
            source.len(),
            "malformed prefix consumes its next character"
        );
    }
}

#[test]
fn modifier_chains_are_iterative_and_control_is_not_idempotent() {
    assert_eq!(read1(r"?\C-\C-a").as_fixnum(), Some(0x4000001));
    assert_eq!(read1(r"?\M-\200").as_fixnum(), Some(0x8000080));
    let source = format!("?{}a", r"\M-".repeat(100_000));
    assert_eq!(read1(&source).as_fixnum(), Some(0x8000061));
    let source = format!("\"{}a\"", r"\M-".repeat(100_000));
    let value = read1(&source);
    assert_eq!(
        value.as_lisp_string().expect("string literal").as_bytes(),
        &[225]
    );
}

#[test]
fn self_contained_escapes_match_gnu_for_every_ascii_character() {
    check(
        "ascii-self-contained",
        r#"(list
  (let (out)
    (dotimes (c 128)
      (push (list c
                  (condition-case e
                      (let ((s (car (read-from-string (string ?\" ?\\ c ?\")))))
                        (list (append s nil) (multibyte-string-p s)))
                    (error (car e)))
                  (condition-case e (car (read-from-string (string ?? ?\\ c)))
                    (error (car e))))
            out))
    (nreverse out))
  (mapcar (lambda (source)
            (condition-case e
                (let ((v (car (read-from-string source))))
                  (if (stringp v) (list (append v nil) (multibyte-string-p v)) v))
              (error (car e))))
          '("?\\M-\\\\" "?\\C-\\(" "?\\M-\\d" "?\\^\\e" "?\\S-\\\"" "?\\M-\\a"
            "\"\\C-\\\\\"" "\"\\M-\\\"\"")))"#,
        include_str!("escapes-ascii-self-contained.expect"),
    );
}
