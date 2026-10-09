//! Compare the real NeoVM helper with GNU's actual help-echo event dispatch.
//! GNU keyboard.c:2221-2228 inspects character zero only when nonempty;
//! keyboard.c:3346-3358 dispatches a help-echo event through show_help_echo.
//! The fixture is refreshed exclusively from GNU stdout with UPDATE_EXPECT=1.
use super::*;

const GNU_HELP_ECHO_FORM: &str = r#"(progn
  (unless (string= emacs-version "31.1")
    (error "help-echo oracle requires GNU Emacs 31.1, got %s" emacs-version))
  (defvar neovm--gde-help-calls 0)
  (defvar neovm--gde-help-shown nil)
  (prin1
    (let ((old-substitute (symbol-function 'substitute-command-keys))
          (old-fixup (symbol-function 'mouse-fixup-help-message)) out)
      (unwind-protect
          (progn
            (fset 'mouse-fixup-help-message (lambda (help) help))
            (fset 'substitute-command-keys
              (lambda (help)
                (setq neovm--gde-help-calls (1+ neovm--gde-help-calls))
                (concat "sub:" help)))
            (dolist (default '(nil t))
              (let ((default-text-properties
                      (and default '(help-echo-inhibit-substitution t))))
                (dolist (spec '(("" nil nil) ("x" nil nil) ("x" 0 t)
                                ("x" 0 nil) ("xy" 0 t) ("xy" 1 t)
                                ("жz" 0 t) ("жz" 1 t)))
                  (let ((help (copy-sequence (car spec)))
                        (show-help-function
                          (lambda (help) (setq neovm--gde-help-shown help))))
                    (setq neovm--gde-help-calls 0 neovm--gde-help-shown nil)
                    (when (cadr spec)
                      (put-text-property (cadr spec) (1+ (cadr spec))
                        'help-echo-inhibit-substitution (nth 2 spec) help))
                    (let ((unread-command-events
                            (list (list 'help-echo nil help nil nil nil) ?z)))
                      (unless (= (read-event) ?z)
                        (error "help-echo event did not reach its following character")))
                    (push (list default spec neovm--gde-help-calls
                                (and neovm--gde-help-shown
                                  (substring-no-properties neovm--gde-help-shown))) out)))))
            (nreverse out))
        (fset 'substitute-command-keys old-substitute)
        (fset 'mouse-fixup-help-message old-fixup))))
  (terpri))"#;

fn direct_help_echo_rows() -> String {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str(
        r#"(progn
          (defvar neovm--gde-help-calls 0)
          (fset 'substitute-command-keys
            (lambda (help)
              (setq neovm--gde-help-calls (1+ neovm--gde-help-calls))
              (concat "sub:" help))))"#,
    )
    .unwrap();
    let roots = eval.save_specpdl_roots();
    let mut rows = Vec::new();
    let specs = [
        ("", None, false),
        ("x", None, false),
        ("x", Some(0), true),
        ("x", Some(0), false),
        ("xy", Some(0), true),
        ("xy", Some(1), true),
        ("жz", Some(0), true),
        ("жz", Some(1), true),
    ];
    for default in [false, true] {
        eval.eval_str(if default {
            "(setq default-text-properties '(help-echo-inhibit-substitution t))"
        } else {
            "(setq default-text-properties nil)"
        })
        .unwrap();
        for (text, position, inhibit) in specs {
            eval.eval_str("(setq neovm--gde-help-calls 0)").unwrap();
            let spec = Value::list(vec![
                Value::string(text),
                position.map_or(Value::NIL, Value::fixnum),
                Value::bool_val(inhibit),
            ]);
            eval.push_specpdl_root(spec);
            let help = Value::string(text);
            eval.push_specpdl_root(help);
            if let Some(position) = position {
                crate::emacs_core::textprop::builtin_put_text_property_5(
                    &mut eval,
                    Value::fixnum(position),
                    Value::fixnum(position + 1),
                    Value::symbol("help-echo-inhibit-substitution"),
                    Value::bool_val(inhibit),
                    help,
                )
                .unwrap();
            }
            // Invoke the production Rust helper, bypassing NeoVM's unrelated
            // unread-command-events dispatch divergence.
            let shown = eval.substitute_help_echo_command_keys(help).unwrap();
            eval.push_specpdl_root(shown);
            let calls = eval.eval_str("neovm--gde-help-calls").unwrap();
            let shown_text = Value::string(shown.as_str_owned().expect("string help result"));
            let row = Value::list(vec![Value::bool_val(default), spec, calls, shown_text]);
            eval.push_specpdl_root(row);
            rows.push(row);
        }
    }
    let output = crate::emacs_core::print_value(&Value::list(rows));
    eval.restore_specpdl_roots(roots);
    output
}

#[test]
fn gnu_help_echo_substitution_direct_helper_matches_actual_event_path() {
    let Some(emacs) = std::env::var_os("NEOVM_FORCE_ORACLE_PATH") else {
        tracing::debug!("skipping help-echo GNU oracle: set NEOVM_FORCE_ORACLE_PATH");
        return;
    };
    let output = std::process::Command::new(emacs)
        .args(["-Q", "--batch", "--eval", GNU_HELP_ECHO_FORM])
        .output()
        .expect("run explicitly configured GNU Emacs oracle");
    assert!(
        output.status.success(),
        "GNU help-echo oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual_gnu = String::from_utf8(output.stdout.clone()).expect("GNU oracle UTF-8 stdout");
    let expected = if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/keyboard/tests/gnu_help_echo_substitution.expected");
        // Never write a NeoVM result to the oracle fixture.
        std::fs::write(&fixture, &output.stdout).expect("refresh fixture exclusively from GNU");
        std::fs::read_to_string(fixture).expect("read freshly written GNU fixture")
    } else {
        include_str!("gnu_help_echo_substitution.expected").to_owned()
    };
    assert!(
        !expected.trim().is_empty(),
        "refresh the blank fixture from GNU Emacs 31.1 with UPDATE_EXPECT=1"
    );
    assert_eq!(
        actual_gnu.trim_end(),
        expected.trim_end(),
        "GNU oracle fixture"
    );
    assert_eq!(
        direct_help_echo_rows(),
        expected.trim_end(),
        "NeoVM direct helper"
    );
}
