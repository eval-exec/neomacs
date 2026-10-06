use crate::test_utils::runtime_startup_eval_one;

#[test]
fn gdn_word_motion_honors_dynamic_script_table_in_both_directions() {
    let result = runtime_startup_eval_one(
        r#"(let ((char-script-table (make-char-table 'char-script-table 'latin)))
       (set-char-table-range char-script-table ?b 'greek)
       (with-temp-buffer (insert "ab") (goto-char 1)
         (let ((forward (progn (forward-word) (point))))
           (goto-char 3) (backward-word) (list forward (point)))))"#,
    );
    assert_eq!(result, "OK (2 2)");
}

#[test]
fn gdn_word_motion_nil_category_sets_use_script_boundary() {
    assert_eq!(
        crate::test_utils::runtime_startup_eval_one(
            r#"(with-temp-buffer (insert "ab") (let ((table (make-char-table 'category-table nil)) (word-separating-categories '((nil . nil)))) (set-category-table table) (goto-char 1) (forward-word) (point)))"#
        ),
        "OK ".to_owned() + include_str!("gdn-word-nil-category.expect").trim()
    );
}

// GNU category.c:384 compares script entries with EQ, before category sets.
const SCRIPT_ENTRY_IDENTITY_FORM: &str = r#"(mapcar (lambda (make-entry)
      (mapcar (lambda (shared)
        (let* ((left (funcall make-entry))
               (right (if shared left (funcall make-entry)))
               (char-script-table (make-char-table 'char-script-table nil)))
          (set-char-table-range char-script-table ?a left)
          (set-char-table-range char-script-table ?b right)
          (with-temp-buffer
            (insert "ab")
            (set-category-table (make-char-table 'category-table nil))
            (goto-char 1)
            (let ((forward (progn (forward-word) (point))))
              (goto-char (point-max))
              (backward-word)
              (list (equal left right) (eq left right) forward (point))))))
        '(nil t)))
      (list (lambda () (vector 1))
            (lambda () (copy-sequence "script"))
            (lambda () (make-symbol "script"))))"#;

fn gnu_word_motion_fixture(name: &str, form: &str, cached: &str) -> String {
    if std::env::var("UPDATE_EXPECT").as_deref() != Ok("1") {
        return cached.trim().to_owned();
    }
    let emacs = std::env::var_os("NEOVM_FORCE_ORACLE_PATH").unwrap_or_else(|| {
        std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"))
            .join(".local/bin/emacs")
            .into_os_string()
    });
    // Campaign refreshes supply the mandatory PID/memory sandbox. A whole
    // nextest driver may also already run inside that sandbox.
    let mut command = if let Some(sandbox) = std::env::var_os("NEOVM_LISP_SANDBOX") {
        let mut command = std::process::Command::new(sandbox);
        command.arg(emacs);
        command
    } else {
        std::process::Command::new(emacs)
    };
    let output = command
        .args(["-Q", "--batch", "--eval"])
        .arg(format!("(prin1 {form})"))
        .output()
        .expect("GNU word motion fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = String::from_utf8(output.stdout).expect("GNU numeric fixture is UTF-8");
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/emacs_core/text/syntax/tests")
            .join(format!("{name}.expect")),
        format!("{}\n", expected.trim()),
    )
    .expect("GNU fixture output");
    expected.trim().to_owned()
}

#[test]
fn gdn_word_motion_compares_script_entries_by_identity() {
    assert_eq!(
        runtime_startup_eval_one(SCRIPT_ENTRY_IDENTITY_FORM),
        format!(
            "OK {}",
            gnu_word_motion_fixture(
                "gdn-word-script-identity",
                SCRIPT_ENTRY_IDENTITY_FORM,
                include_str!("gdn-word-script-identity.expect")
            )
        )
    );
}

// GNU syntax.c:1477,1505,1527,1556 uses FETCH_CHAR_AS_MULTIBYTE in both directions.
const EMACS_CHARACTER_CODES_FORM: &str = r#"(mapcar (lambda (shape)
      (mapcar (lambda (properties)
        (mapcar (lambda (mode)
          (let ((char-script-table (make-char-table 'char-script-table 'latin))
                (word-combining-categories '((?r . nil)))
                (word-separating-categories '((?r . nil)))
                (code (nth 2 shape)))
            (set-char-table-range char-script-table code
              (if (eq mode 'separate) 'latin 'greek))
            (set-char-table-range char-script-table #x80 'latin)
            (set-char-table-range char-script-table #xfffd 'latin)
            (with-temp-buffer
              (set-buffer-multibyte (not (car shape)))
              (insert (nth 1 shape))
              (setq-local parse-sexp-lookup-properties t)
              (let ((syntax (make-syntax-table))
                    (categories (make-char-table 'category-table nil)))
                (modify-syntax-entry code
                  (if (memq properties '(cons table)) " " "w") syntax)
                (modify-syntax-entry #x80 " " syntax)
                (modify-syntax-entry #xfffd " " syntax)
                (modify-syntax-entry ?a "w" syntax)
                (set-syntax-table syntax)
                (unless (eq mode 'script)
                  (set-char-table-range categories code (make-category-set "r"))
                  (set-char-table-range categories ?a (make-category-set "")))
                (set-category-table categories))
              (cond
                ((eq properties 'face)
                 (put-text-property 1 (point-max) 'face 'bold))
                ((eq properties 'cons)
                 (put-text-property 1 2 'syntax-table (string-to-syntax "w")))
                ((eq properties 'table)
                 (let ((override (make-syntax-table)))
                   (modify-syntax-entry code "w" override)
                   (modify-syntax-entry #x80 " " override)
                   (modify-syntax-entry #xfffd " " override)
                   (put-text-property 1 2 'syntax-table override))))
              (goto-char 1)
              (let ((forward (progn (forward-word) (point))))
                (goto-char (point-max))
                (backward-word)
                (list forward (point))))))
          '(script combine separate))) '(none face cons table)))
      (list (list t (unibyte-string #x80 ?a) #x3fff80)
            (list nil (string-to-multibyte (unibyte-string #x80 ?a)) #x3fff80)
            (list nil (string #x110000 ?a) #x110000)))"#;

#[test]
fn gdn_word_motion_preserves_raw_and_non_unicode_character_codes() {
    assert_eq!(
        runtime_startup_eval_one(EMACS_CHARACTER_CODES_FORM),
        format!(
            "OK {}",
            gnu_word_motion_fixture(
                "gdn-word-character-codes",
                EMACS_CHARACTER_CODES_FORM,
                include_str!("gdn-word-character-codes.expect")
            )
        )
    );
}
