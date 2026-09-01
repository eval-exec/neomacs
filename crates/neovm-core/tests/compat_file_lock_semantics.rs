mod common;

use std::fs;

use common::{elisp_string, oracle_enabled, run_neovm_eval, run_oracle_eval};

#[test]
fn compat_file_lock_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping file lock semantics audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let temp = tempfile::NamedTempFile::new().expect("temp file");
    fs::write(temp.path(), b"x").expect("seed file");
    let file = elisp_string(temp.path());

    let form = format!(
        r#"(let ((f {file}))
  (unwind-protect
      (with-temp-buffer
        (setq buffer-file-name f
              buffer-file-truename f)
        (list
         (file-locked-p f)
         (progn
           (set-buffer-modified-p t)
           (file-locked-p f))
         (progn
           (set-buffer-modified-p nil)
           (file-locked-p f))))
    (ignore-errors
      (let ((lock (make-lock-file-name f)))
        (when lock
          (delete-file lock))))))"#
    );

    let gnu = run_oracle_eval(&form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(&form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "file lock semantics mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}
