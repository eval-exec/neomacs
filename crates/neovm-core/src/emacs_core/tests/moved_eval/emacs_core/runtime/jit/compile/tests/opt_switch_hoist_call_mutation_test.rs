//! Switch-hoist call mutation against the GNU fixture.
//!
//! The plan and the lower stay in eval. This test owns the startup context,
//! the GNU expectation, and the assert messages.

use neovm_eval::test_support::authority::{
    TestOptAdmit, TestOptMode, force_deopt_for_test, force_inline_switch_for_test,
    force_opt_passes_spec_for_test, force_opt_token_for_test, force_switch_hoist_for_test,
};

const MUTATION_SETUP: &str = r#"(progn
 (require 'cl-lib)
 (defvar switch-hoist-ticks 0)
 (defvar switch-hoist-table nil)
 (defun switch-hoist-mutate ()
   (setq switch-hoist-ticks (1+ switch-hoist-ticks))
   (when (= switch-hoist-ticks 3)
     (puthash 'a (gethash 'b switch-hoist-table) switch-hoist-table)))
 (setq switch-hoist-function
   (byte-compile
    (lambda (x n)
      (let ((acc 0))
        (while (> n 0)
          (switch-hoist-mutate)
          (setq acc (+ acc (pcase x ('a 1) ('b 10) ('c 20) ('d 30) ('e 40) ('f 50) (_ 0))))
          (setq n (1- n)))
        acc))))
 (setq switch-hoist-ticks 0
       switch-hoist-table
       (cl-find-if #'hash-table-p (append (aref switch-hoist-function 2) nil))))"#;

/// Threading: scalar compiler overrides on this test's thread. No Lisp state.
#[must_use = "the compiler overrides end when this guard is dropped"]
struct SwitchSettings;

impl SwitchSettings {
    fn enter() -> Self {
        force_opt_token_for_test(
            Some(TestOptMode::Opt),
            Some(TestOptAdmit::parts(true, true, true, true, true, true)),
        );
        force_opt_passes_spec_for_test(Some("reps,bool"));
        force_deopt_for_test(false);
        force_inline_switch_for_test(Some(true));
        force_switch_hoist_for_test(Some(true));
        Self
    }
}

impl Drop for SwitchSettings {
    fn drop(&mut self) {
        force_inline_switch_for_test(None);
        force_switch_hoist_for_test(None);
        force_opt_token_for_test(None, None);
        force_opt_passes_spec_for_test(None);
        force_deopt_for_test(false);
    }
}

fn gnu_fixture(name: &str, form: &str) -> String {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join(format!(
        "../neovm-eval/src/emacs_core/runtime/jit/compile/tests/switch_hoist_{name}.expect"
    ));
    if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let script = root.join(format!("../../tmp/switch-hoist-{name}-oracle.el"));
        std::fs::write(
            &script,
            format!(";;; -*- lexical-binding: t; -*-\n(prin1 {form})\n"),
        )
        .unwrap();
        // The campaign sandbox and the editor are one typed command; a
        // misbehaving refresh is bounded by nextest's per-test watchdog.
        let output = crate::test_utils::gnu_refresh_command()
            .args(["-Q", "--batch", "-l"])
            .arg(script)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "GNU: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::write(&fixture, &output.stdout).unwrap();
    }
    std::fs::read_to_string(fixture)
        .expect("refresh GNU fixture with UPDATE_EXPECT=1")
        .trim_end()
        .to_owned()
}

fn gnu_mutation_expect() -> String {
    gnu_fixture(
        "call",
        &format!("(progn {MUTATION_SETUP} (funcall switch-hoist-function 'a 300))"),
    )
}

#[test]
fn opt_switch_hoist_call_mutation_matches_gnu() {
    let _settings = SwitchSettings::enter();
    let expected = gnu_mutation_expect();
    let mut ctx = crate::test_utils::runtime_startup_context();
    ctx.eval_str(&format!("(eval '{MUTATION_SETUP} t)"))
        .unwrap();
    let (nonempty, blocked, printed) =
        neovm_eval::test_support::authority::opt_switch_hoist_call_mutation_for_test(&mut ctx);
    assert!(nonempty);
    assert!(blocked, "the call must block hoisting");
    assert_eq!(printed, expected);
}
