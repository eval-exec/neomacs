//! Batch stderr of `do-auto-save`, pinned to GNU Emacs 31.1 `src/fileio.c`.
//!
//! `Fdo_auto_save` (`fileio.c:6315`) prints `Auto-saving...` before the first
//! save and `Auto-saving...done` afterwards when `NO-MESSAGE` is nil and
//! `auto_save_error_occurred` stayed clear (`fileio.c:6476`, `6496-6509`).
//! A non-nil `NO-MESSAGE` argument prints neither line. The command-loop
//! variable `auto-save-no-message` (`keyboard.c:13850`) is not read here; the
//! command loop passes it as `NO-MESSAGE` at `keyboard.c:2902` and `:2970`.
//! A failed save calls `auto_save_error` (`fileio.c:6233`), which sets the
//! flag, and the `...done` line is then absent. Batch `message1` writes these
//! lines to stderr (`xdisp.c:12591`).

use crate::common::assert_oracle_batch_stdout_has_no_bel;

#[test]
fn do_auto_save_error_writes_no_bel_on_batch_stdout() {
    // `(ding)` would put BEL on batch stdout. `ring_bell` does not.
    let form = r#"(let ((dir (expand-file-name "not-a-file" temporary-file-directory))
      (buf (generate-new-buffer "as-bad")))
  (make-directory dir t)
  (with-current-buffer buf
    (insert "hello\n")
    (setq buffer-auto-save-file-name dir))
  (do-auto-save))"#;
    assert_oracle_batch_stdout_has_no_bel(form);
}
