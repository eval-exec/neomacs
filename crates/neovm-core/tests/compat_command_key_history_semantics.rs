mod common;

use common::{oracle_enabled, run_oracle_eval};
use neovm_core::emacs_core::{Context, format_eval_result};

fn run_neovm_eval_minimal(form: &str) -> Result<String, String> {
    let mut eval = Context::new();
    eval.set_lexical_binding(true);
    let result = eval.eval_str(form);
    Ok(format_eval_result(&result))
}

struct CommandKeyCase {
    name: &'static str,
    form: &'static str,
}

#[test]
fn compat_command_key_history_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping command-key history audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let cases = [
        CommandKeyCase {
            name: "set_this_command_keys_updates_translated_surface_only",
            form: r#"(progn
  (set--this-command-keys (concat "\M-x" "foo" "\r"))
  (list
   (this-command-keys)
   (append (this-command-keys-vector) nil)
   (append (this-single-command-keys) nil)
   (append (this-single-command-raw-keys) nil)))"#,
        },
        CommandKeyCase {
            name: "set_this_command_keys_does_not_rewrite_recent_keys",
            form: r#"(progn
  (let ((unread-command-events '(97)))
    (read-char))
  (let ((before (append (recent-keys) nil)))
    (set--this-command-keys "b")
    (list before
          (append (recent-keys) nil)
          (this-command-keys)
          (append (this-single-command-raw-keys) nil))))"#,
        },
    ];

    for case in cases {
        let gnu = run_oracle_eval(case.form).expect("GNU Emacs evaluation");
        let neovm = run_neovm_eval_minimal(case.form).expect("NeoVM evaluation");
        assert_eq!(
            neovm, gnu,
            "command-key history mismatch for {}:\nGNU: {}\nNeoVM: {}",
            case.name, gnu, neovm
        );
    }
}
