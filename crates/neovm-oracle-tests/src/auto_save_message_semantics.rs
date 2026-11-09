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
    assert_oracle_batch_stderr_and_value_expect, assert_oracle_batch_stderr_parity_expect,
    assert_oracle_batch_stdout_has_no_bel,
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

/// A filename buffer whose text is under 3/4 of a `buffer-saved-size` above
/// 5000. Pinned GNU 31.1: `4*(Z-BEG) < 3*save_length` with size 100 and
/// `save_length` 8000.
fn shrunk_filename_buffer(name: &str, saved: i64) -> String {
    format!(
        r##"(let ((buf (generate-new-buffer "{name}")))
  (with-current-buffer buf
    (setq buffer-file-name (expand-file-name "{name}.txt" temporary-file-directory))
    (insert (make-string 100 ?x))
    (setq buffer-saved-size {saved})
    (setq buffer-auto-save-file-name
          (expand-file-name "#{name}#" temporary-file-directory)))
  buf)"##
    )
}

#[test]
fn do_auto_save_shrink_disables_and_sets_save_length_to_minus_one() {
    // Live GNU: one stderr line, `buffer-saved-size` -1, auto-save file not written.
    let form = format!(
        "(progn {} (do-auto-save) (with-current-buffer (get-buffer \"as-shrink\") buffer-saved-size))",
        shrunk_filename_buffer("as-shrink", 8000)
    );
    assert_oracle_batch_stderr_and_value_expect(
        &form,
        expect_test::expect![[
            r#""Buffer as-shrink has shrunk a lot; auto save disabled in that buffer until next real save\nVALUE OK -1""#
        ]],
    );
}

#[test]
fn do_auto_save_no_message_does_not_take_the_shrink_branch() {
    // The `&& NILP (no_message)` term skips the whole shrink arm
    // (`fileio.c:6463`). Live GNU then saves and sets `save_length` to
    // Z-BEG (100), not -1, and prints nothing.
    let form = format!(
        "(progn {} (do-auto-save t) (with-current-buffer (get-buffer \"as-nomsg\") buffer-saved-size))",
        shrunk_filename_buffer("as-nomsg", 8000)
    );
    assert_oracle_batch_stderr_and_value_expect(&form, expect_test::expect![[r#""VALUE OK 100""#]]);
}

#[test]
fn do_auto_save_include_big_deletions_does_not_disable() {
    let form = format!(
        "(progn (setq auto-save-include-big-deletions t) {} (do-auto-save) (with-current-buffer (get-buffer \"as-include\") buffer-saved-size))",
        shrunk_filename_buffer("as-include", 8000)
    );
    assert_oracle_batch_stderr_and_value_expect(
        &form,
        expect_test::expect![[r#""Auto-saving...\nAuto-saving...done\nVALUE OK 100""#]],
    );
}

#[test]
fn do_auto_save_save_length_at_most_5000_does_not_disable() {
    // `save_length` 5000 is not `> 5000` (`fileio.c:6458`). Live GNU saves.
    let form = format!(
        "(progn {} (do-auto-save) (with-current-buffer (get-buffer \"as-small\") buffer-saved-size))",
        shrunk_filename_buffer("as-small", 5000)
    );
    assert_oracle_batch_stderr_and_value_expect(
        &form,
        expect_test::expect![[r#""Auto-saving...\nAuto-saving...done\nVALUE OK 100""#]],
    );
}

#[test]
fn clear_buffer_auto_save_failure_returns_nil_and_leaves_save_length() {
    // Live GNU 31.1: the C field is not Lisp-visible. The call returns nil
    // and does not change `buffer-saved-size`, `buffer-modified-p`, or
    // `recent-auto-save-p`.
    let form = r##"(let ((buf (generate-new-buffer "as-clear")))
  (with-current-buffer buf
    (insert (make-string 100 ?x))
    (setq buffer-file-name (expand-file-name "as-clear.txt" temporary-file-directory))
    (setq buffer-saved-size 8000)
    (setq buffer-auto-save-file-name
          (expand-file-name "#as-clear#" temporary-file-directory))
    (list (clear-buffer-auto-save-failure)
          buffer-saved-size
          (buffer-modified-p)
          (recent-auto-save-p))))"##;
    assert_oracle_batch_stderr_and_value_expect(
        form,
        expect_test::expect![[r#""VALUE OK (nil 8000 t nil)""#]],
    );
}

#[test]
fn set_buffer_auto_saved_sets_save_length_to_z_beg() {
    // Live GNU 31.1 `Fset_buffer_auto_saved` (`fileio.c:6526-6528`):
    // return nil, `save_length` becomes Z-BEG (2 for "hi"), modified state
    // is `autosaved`, `recent-auto-save-p` is t. The following clear returns
    // nil and leaves `save_length` at 2.
    let form = r#"(let ((buf (generate-new-buffer "as-set")))
  (with-current-buffer buf
    (insert "hi")
    (setq buffer-saved-size 9)
    (list (set-buffer-auto-saved)
          buffer-saved-size
          (buffer-modified-p)
          (recent-auto-save-p)
          (clear-buffer-auto-save-failure)
          buffer-saved-size)))"#;
    assert_oracle_batch_stderr_and_value_expect(
        form,
        expect_test::expect![[r#""VALUE OK (nil 2 autosaved t nil 2)""#]],
    );
}

#[test]
fn set_buffer_auto_saved_counts_multibyte_characters() {
    // Live GNU 31.1: "éé" is two characters (`#xE9`), so `Z - BEG` is 2.
    // A byte count would store 4.
    let form = r#"(let ((buf (generate-new-buffer "as-set-mb")))
  (with-current-buffer buf
    (insert (string #xE9 #xE9))
    (setq buffer-saved-size 9)
    (list (set-buffer-auto-saved)
          buffer-saved-size
          (buffer-modified-p)
          (recent-auto-save-p)
          (clear-buffer-auto-save-failure)
          buffer-saved-size)))"#;
    assert_oracle_batch_stderr_and_value_expect(
        form,
        expect_test::expect![[r#""VALUE OK (nil 2 autosaved t nil 2)""#]],
    );
}

#[test]
fn do_auto_save_multibyte_shrink_uses_character_length() {
    // Live GNU 31.1: 4000 characters of `#xE9` against `save_length` 6000.
    // `4*4000 < 3*6000` (16000 < 18000) disables. The same text is 8000
    // bytes, and `4*8000 < 18000` is false, so a byte compare would save.
    let form = r##"(let ((buf (generate-new-buffer "as-shrink-mb")))
  (with-current-buffer buf
    (setq buffer-file-name (expand-file-name "as-shrink-mb.txt" temporary-file-directory))
    (insert (make-string 4000 #xE9))
    (setq buffer-saved-size 6000)
    (setq buffer-auto-save-file-name
          (expand-file-name "#as-shrink-mb#" temporary-file-directory)))
  (do-auto-save)
  (with-current-buffer buf buffer-saved-size))"##;
    assert_oracle_batch_stderr_and_value_expect(
        form,
        expect_test::expect![[
            r#""Buffer as-shrink-mb has shrunk a lot; auto save disabled in that buffer until next real save\nVALUE OK -1""#
        ]],
    );
}

#[test]
fn do_auto_save_buffer_local_include_big_deletions_does_not_disable() {
    // Live GNU 31.1 reads `auto-save-include-big-deletions` after
    // `set_buffer_internal` (`fileio.c:6453`). Local t with a global nil
    // saves; `save_length` becomes Z-BEG (100).
    let form = r##"(progn
  (setq auto-save-include-big-deletions nil)
  (let ((buf (generate-new-buffer "as-local-include")))
    (with-current-buffer buf
      (setq-local auto-save-include-big-deletions t)
      (setq buffer-file-name (expand-file-name "as-local-include.txt" temporary-file-directory))
      (insert (make-string 100 ?x))
      (setq buffer-saved-size 8000)
      (setq buffer-auto-save-file-name
            (expand-file-name "#as-local-include#" temporary-file-directory)))
    (do-auto-save)
    (with-current-buffer buf buffer-saved-size)))"##;
    assert_oracle_batch_stderr_and_value_expect(
        form,
        expect_test::expect![[r#""Auto-saving...\nAuto-saving...done\nVALUE OK 100""#]],
    );
}

#[test]
fn do_auto_save_shrink_leaves_that_buffer_current() {
    // GNU `fileio.c:6333` records `old` before the hook. `set_buffer_internal`
    // (`:6453`) stays in effect through the shrink arm, and that arm
    // `continue`s without restoring `old` (`:6464-6474`). There is no
    // `record_unwind_current_buffer`. Pinned GNU 31.1 returns `(t -1)` and
    // prints only the shrink line.
    let form = r##"(let ((other (generate-new-buffer "as-other"))
      (buf (generate-new-buffer "f.txt")))
  (with-current-buffer buf
    (setq buffer-file-name (expand-file-name "f.txt" temporary-file-directory))
    (insert (make-string 1000 ?x))
    (setq buffer-saved-size 6000)
    (setq buffer-auto-save-file-name
          (expand-file-name "#f.txt#" temporary-file-directory)))
  (set-buffer other)
  (do-auto-save)
  (list (equal (buffer-name) "f.txt")
        (with-current-buffer "f.txt" buffer-saved-size)))"##;
    assert_oracle_batch_stderr_and_value_expect(
        form,
        expect_test::expect![[
            r#""Buffer f.txt has shrunk a lot; auto save disabled in that buffer until next real save\nVALUE OK (t -1)""#
        ]],
    );
}

#[test]
fn do_auto_save_later_save_restores_the_entry_buffer() {
    // A later buffer that actually saves restores `old` (`fileio.c:6482`).
    // Shrink is created first, so both engines walk it before the save:
    // GNU appends to `Vbuffer_alist` and `set-buffer` does not reorder it,
    // and Neomacs `set-buffer` is unrecorded. Pinned GNU 31.1 returns
    // `(t -1 2)` after the shrink line and the two progress lines.
    let form = r##"(let ((old (generate-new-buffer "as-old"))
      (shrink (generate-new-buffer "as-shrink-first"))
      (save (generate-new-buffer "as-save-later")))
  (with-current-buffer shrink
    (insert (make-string 1000 ?x))
    (setq buffer-file-name (expand-file-name "as-shrink-first.txt" temporary-file-directory))
    (setq buffer-saved-size 6000)
    (setq buffer-auto-save-file-name
          (expand-file-name "#as-shrink-first#" temporary-file-directory)))
  (with-current-buffer save
    (insert "xy")
    (setq buffer-file-name (expand-file-name "as-save-later.txt" temporary-file-directory))
    (setq buffer-saved-size 2)
    (setq buffer-auto-save-file-name
          (expand-file-name "#as-save-later#" temporary-file-directory)))
  (set-buffer old)
  (do-auto-save)
  (list (equal (buffer-name) "as-old")
        (with-current-buffer "as-shrink-first" buffer-saved-size)
        (with-current-buffer "as-save-later" buffer-saved-size)))"##;
    assert_oracle_batch_stderr_and_value_expect(
        form,
        expect_test::expect![[
            r#""Buffer as-shrink-first has shrunk a lot; auto save disabled in that buffer until next real save\nAuto-saving...\nAuto-saving...done\nVALUE OK (t -1 2)""#
        ]],
    );
}
