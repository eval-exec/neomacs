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

use crate::common::{
    assert_oracle_batch_stderr_parity_expect, assert_oracle_batch_stdout_has_no_bel,
};

fn saved_buffer(name: &str, file: &str) -> String {
    format!(
        r#"(let ((buf (generate-new-buffer "{name}")))
  (with-current-buffer buf
    (insert "hello\n")
    (setq buffer-auto-save-file-name
          (expand-file-name "{file}" temporary-file-directory)))
  buf)"#
    )
}

#[test]
fn do_auto_save_prints_progress_and_done_on_stderr() {
    let form = format!(
        "(progn {} (do-auto-save))",
        saved_buffer("as-probe", "#as-probe#")
    );
    assert_oracle_batch_stderr_parity_expect(
        &form,
        expect_test::expect![[r#""Auto-saving...\nAuto-saving...done""#]],
    );
}

#[test]
fn do_auto_save_no_message_argument_prints_nothing() {
    let form = format!(
        "(progn {} (do-auto-save t))",
        saved_buffer("as-quiet", "#as-quiet#")
    );
    assert_oracle_batch_stderr_parity_expect(&form, expect_test::expect![[r#""""#]]);
}

#[test]
fn auto_save_no_message_variable_does_not_suppress_a_direct_call() {
    let form = format!(
        "(progn (setq auto-save-no-message t) {} (do-auto-save))",
        saved_buffer("as-var", "#as-var#")
    );
    assert_oracle_batch_stderr_parity_expect(
        &form,
        expect_test::expect![[r#""Auto-saving...\nAuto-saving...done""#]],
    );
}

#[test]
fn do_auto_save_error_suppresses_the_done_line() {
    let form = r#"(let ((dir (expand-file-name "not-a-file" temporary-file-directory))
      (buf (generate-new-buffer "as-bad")))
  (make-directory dir t)
  (with-current-buffer buf
    (insert "hello\n")
    (setq buffer-auto-save-file-name dir))
  (do-auto-save))"#;
    assert_oracle_batch_stderr_parity_expect(
        form,
        expect_test::expect![[
            r#""Auto-saving...\nError (auto-save): Auto-saving as-bad: Opening output file: Is a directory, [ORACLE-TMP]/not-a-file""#
        ]],
    );
}

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
