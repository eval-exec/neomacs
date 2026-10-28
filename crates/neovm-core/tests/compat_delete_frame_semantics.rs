use crate::common;

use common::run_oracle_eval;
use neovm_core::emacs_core::{Context, format_eval_result};

fn run_neovm_eval_minimal_with_frame(form: &str) -> Result<String, String> {
    let mut eval = Context::new();
    let buf = eval.buffer_manager_mut().create_buffer("*scratch*");
    eval.buffer_manager_mut().set_current(buf);
    eval.frame_manager_mut().create_frame("F1", 800, 600, buf);
    let result = eval.eval_str(form);
    Ok(format_eval_result(&result))
}

#[test]
fn compat_delete_frame_sole_frame_error_semantics_match_gnu_emacs() {
    let form = r#"(list
  (condition-case err
      (delete-frame nil)
    (error err))
  (condition-case err
      (delete-frame nil t)
    (error err)))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval_minimal_with_frame(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "delete-frame sole-frame semantics mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}
