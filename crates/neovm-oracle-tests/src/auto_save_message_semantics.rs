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

#[cfg(test)]
use crate::common::assert_oracle_batch_exit_and_stderr_expect;
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

#[test]
fn do_auto_save_bell_error_leaves_the_saved_buffer_current() {
    // GNU `internal_condition_case` (`fileio.c:6478`) runs `auto_save_error`.
    // A signal from that handler (`ring_bell` at `terminal.c:73`, or
    // `display-warning`) leaves `Fdo_auto_save` before `:6479-6482`, so GNU
    // does not mark the buffer auto-saved, does not update `save_length`,
    // and does not `set_buffer_internal (old)`. The caller's `condition-case`
    // therefore sees the saved buffer still current. `ring_bell` clears
    // `ring-bell-function` with no specbind, so a signaling function stays
    // nil (`terminal.c:70-73`).
    let form = r#"(let* ((dir (make-temp-file "asprobe" t))
       (f (expand-file-name "f.txt" dir))
       (b (find-file-noselect f)))
  (with-current-buffer b
    (insert "x")
    (setq buffer-auto-save-file-name "/nonexistent-neomacs-probe-dir/x/#f.txt#"))
  (setq ring-bell-function (lambda () (error "bell-boom")))
  (with-current-buffer (get-buffer-create "other")
    (message "before=%s" (buffer-name))
    (message "caught=%S" (condition-case e (do-auto-save t) (error e)))
    (message "after=%s" (buffer-name))
    (message "rbf=%S" ring-bell-function)
    (message "saved-size=%S autosaved=%S" (buffer-local-value 'buffer-saved-size b)
             (with-current-buffer b (recent-auto-save-p))))
  (delete-directory dir t))"#;
    assert_oracle_batch_stderr_and_value_expect(
        form,
        expect_test::expect![[
            r#""before=other\ncaught=(error \"bell-boom\")\nafter=f.txt\nrbf=nil\nsaved-size=0 autosaved=nil\nError (auto-save): Auto-saving f.txt: Opening output file: No such file or directory, /nonexistent-neomacs-probe-dir/x/#f.txt#\nVALUE OK nil""#
        ]],
    );
}

/// GNU `Fkill_emacs` runs `kill-emacs-hook` before `shut_down_emacs`, which
/// permanently sets `Vrun_hooks = Qnil` before `Fdo_auto_save (Qt, Qnil)`
/// (`src/emacs.c:3010-3028,3107-3110,3157`). The final auto-save must include
/// edits made by the kill hook without calling `auto-save-hook` again.
///
/// This probe exits the editor deliberately, so it must bypass the usual
/// evaluation/result normalizer and compare the real filesystem artifacts.
#[test]
fn shutdown_auto_save_writes_without_running_auto_save_hook() {
    let form = r##"(progn
  (setq auto-save-list-file-name nil)
  (with-current-buffer (generate-new-buffer "shutdown-save")
    (insert "before\n")
    (setq buffer-auto-save-file-name
          (expand-file-name "#shutdown-save#"
                            (getenv "NEOVM_ORACLE_TEST_TMPDIR"))))
  (setq kill-emacs-hook
        (list (lambda ()
                (with-current-buffer "shutdown-save"
                  (goto-char (point-max))
                  (insert "from-kill-hook\n"))
                (write-region "kill\n" nil
                              (expand-file-name "kill-hook.marker"
                                                (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
                              t 'silent))))
  (setq auto-save-hook
        (list (lambda ()
                (write-region "auto-save\n" nil
                              (expand-file-name "auto-save-hook.marker"
                                                (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
                              t 'silent))))
  (kill-emacs 0))"##;
    assert_shutdown_probe_artifacts(
        form,
        &[
            ("#shutdown-save#", Some(b"before\nfrom-kill-hook\n")),
            ("kill-hook.marker", Some(b"kill\n")),
            ("auto-save-hook.marker", None),
        ],
    );
}

// Same file-artifact probe for exit-time and explicit auto-save. The probe's
// directory comes from the canonical child environment, rather than the host.
#[cfg(test)]
fn shutdown_auto_save_file_probe(explicit: bool) -> String {
    let save = if explicit { "(do-auto-save t)" } else { "" };
    format!(
        r##"(progn
  (setq auto-save-list-file-name nil)
  (setq hookfile (expand-file-name "hook.log" (getenv "NEOVM_ORACLE_TEST_TMPDIR")))
  (add-hook 'auto-save-hook (lambda () (write-region "HOOK\n" nil hookfile t)))
  (with-current-buffer (get-buffer-create "x")
    (setq buffer-file-name (expand-file-name "x.txt" (getenv "NEOVM_ORACLE_TEST_TMPDIR")))
    (setq buffer-auto-save-file-name
          (expand-file-name "#x.txt#" (getenv "NEOVM_ORACLE_TEST_TMPDIR")))
    (insert "data data data")
    (set-buffer-modified-p t))
  {save}
  (kill-emacs 0))"##,
    )
}

#[test]
fn kill_emacs_auto_save_has_no_hook_artifact() {
    assert_shutdown_probe_artifacts(
        &shutdown_auto_save_file_probe(false),
        &[("#x.txt#", Some(b"data data data")), ("hook.log", None)],
    );
}

#[test]
fn explicit_auto_save_runs_hook_once_before_kill_emacs() {
    assert_shutdown_probe_artifacts(
        &shutdown_auto_save_file_probe(true),
        &[
            ("#x.txt#", Some(b"data data data")),
            ("hook.log", Some(b"HOOK\n")),
        ],
    );
}

#[test]
fn dying_lisp_walker_returns_while_display_warning_runs() {
    // GNU 31.1 runs Lisp from auto_save_error -> display-warning while
    // Vrun_hooks is nil. run-hooks returns nil and does not run the hook
    // function. run-hook-with-args-until-failure returns t (eval.c:2966).
    // fileio.c:6370 is the other direct Vrun_hooks check in that save.
    let form = r#"(progn
  (setenv "ASPROBE" (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
  (setq logf (expand-file-name "log.txt" (getenv "ASPROBE")))
  (defun probe-log (s) (write-region (concat s "\n") nil logf t))
  (setq probe-hook (list (lambda () (probe-log "HOOK-RAN"))))
  (fset 'display-warning
        (lambda (&rest args)
          (probe-log (format "DISPLAY-WARNING %S" (car args)))
          (probe-log (format "run-hooks-> %S" (run-hooks 'probe-hook)))
          (probe-log (format "until-failure-> %S"
                             (run-hook-with-args-until-failure 'probe-hook)))))
  (with-current-buffer (get-buffer-create "x")
    (setq buffer-file-name (expand-file-name "x.txt" (getenv "ASPROBE")))
    (setq buffer-auto-save-file-name "/nonexistent-neomacs-probe-dir/q/#x.txt#")
    (insert "data") (set-buffer-modified-p t))
  (kill-emacs 0))"#;
    assert_shutdown_probe_artifacts(
        form,
        &[(
            "log.txt",
            Some(b"DISPLAY-WARNING auto-save\nrun-hooks-> nil\nuntil-failure-> t\n"),
        )],
    );
}

#[cfg(test)]
fn assert_shutdown_probe_artifacts(form: &str, expected: &[(&str, Option<&[u8]>)]) {
    assert_shutdown_probe_exit_artifacts(form, 0, expected);
}

#[cfg(test)]
fn assert_shutdown_probe_exit_artifacts(
    form: &str,
    expected_exit_code: i32,
    expected: &[(&str, Option<&[u8]>)],
) {
    use crate::common::oracle_sandbox::{OracleSandbox, project_root};
    use neomacs_melpa_test_support::output_with_timeout_pinned;
    use std::path::{Path, PathBuf};
    use std::process::Stdio;
    use std::time::Duration;

    let load_root = project_root().join("lisp");
    let observe = |engine: &str, binary: &Path| {
        let sandbox = OracleSandbox::new(form, &[], &load_root)
            .expect("create shutdown oracle sandbox")
            .with_case_working_directory_and_tmpdir();
        let mut command = sandbox.pinned_command(binary);
        sandbox.configure(&mut command);
        let directory = command
            .get_current_dir()
            .expect("shutdown case working directory")
            .to_path_buf();
        command
            .args(["--batch", "-Q", "--eval", form])
            .stdin(Stdio::null())
            .env("EMACSNATIVELOADPATH", "/dev/null");
        // Drain both pipes concurrently; kill and reap a timed-out editor.
        let output = output_with_timeout_pinned(&mut command, Duration::from_secs(30))
            .unwrap_or_else(|error| panic!("{engine}: shutdown child failed: {error:?}"));
        eprintln!(
            "SHUTDOWN_ORACLE engine={engine} form={form}\nstatus={}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert_eq!(
            output.status.code(),
            Some(expected_exit_code),
            "{engine}: kill-emacs must preserve the requested exit code\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let mut artifacts = Vec::with_capacity(expected.len());
        for (name, expected_bytes) in expected {
            let bytes = match std::fs::read(directory.join(name)) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => panic!("{engine}: cannot read {name}: {error}"),
            };
            eprintln!("SHUTDOWN_ORACLE engine={engine} artifact={name} bytes={bytes:?}");
            assert_eq!(
                bytes.as_deref(),
                *expected_bytes,
                "{engine}: wrong shutdown artifact {name}\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
            artifacts.push(bytes);
        }
        artifacts
    };

    // Always run the live, attested GNU reference first. Exit artifacts are
    // not representable by the usual evaluation/value snapshot.
    let gnu = crate::common::oracle_emacs_path();
    let neomacs = PathBuf::from(crate::common::neomacs_binary_path());
    let reference = observe("GNU", &gnu);
    let ours = observe("Neomacs", &neomacs);
    assert_eq!(
        ours, reference,
        "shutdown filesystem artifacts must match GNU"
    );
}

#[test]
fn kill_emacs_retries_after_an_exit_auto_save_signal() {
    // GNU `Fkill_emacs` (`emacs.c:3015-3028`) runs the hook, then
    // `shut_down_emacs` nils `Vrun_hooks`, sets `Vinhibit_redisplay`
    // (`:3107-3110`), and calls `Fdo_auto_save (Qt, Qnil)` (`:3157`).
    // A signal from `ring_bell` (`fileio.c:6239`) leaves before `exit`.
    // Those two assignments are not specbound, so the retry exits 3 and
    // does not run hook bodies (`eval.c:3006-3007`). Pinned GNU 31.1:
    // the marker prints once, `probe` and `until` are nil, `inhibit` is t.
    let form = r#"(let* ((dir (make-temp-file "asprobe" t))
       (f (expand-file-name "f.txt" dir))
       (b (find-file-noselect f))
       (n 0))
  (with-current-buffer b
    (insert "x")
    (setq buffer-auto-save-file-name "/nonexistent-neomacs-probe-dir/x/#f.txt#"))
  (setq ring-bell-function (lambda () (error "bell-boom")))
  (add-hook 'kill-emacs-hook (lambda () (setq n (1+ n)) (message "KILL-EMACS-HOOK-RAN")))
  (setq probe-hook (lambda () (message "PROBE-HOOK-RAN")))
  (message "first=%S" (condition-case e (kill-emacs 0) (error e)))
  (message "after=%s" (buffer-name))
  (message "rbf=%S" ring-bell-function)
  (message "kill-probe=%S" (run-hooks 'kill-emacs-hook))
  (message "hooks=%S" n)
  (message "probe=%S" (run-hooks 'probe-hook))
  (message "until=%S" (run-hook-with-args-until-success 'probe-hook))
  (message "inhibit=%S" inhibit-redisplay)
  (kill-emacs 3)
  (message "NOT-REACHED"))"#;
    assert_oracle_batch_exit_and_stderr_expect(
        form,
        expect_test::expect![[
            r#""EXIT 3\nKILL-EMACS-HOOK-RAN\nfirst=(error \"bell-boom\")\nafter=f.txt\nrbf=nil\nkill-probe=nil\nhooks=1\nprobe=nil\nuntil=nil\ninhibit=t\nError (auto-save): Auto-saving f.txt: Opening output file: No such file or directory, /nonexistent-neomacs-probe-dir/x/#f.txt#""#
        ]],
    );
}

#[test]
fn in_flight_hook_walk_resumes_after_caught_exit_auto_save_error() {
    // GNU tests Vrun_hooks once at walker entry (eval.c:3004-3006).
    // Catching ring_bell's exit-time signal returns into the entered walk:
    // its remaining callback runs, but a newly entered hook is suppressed.
    let form = r##"(progn
(setq auto-save-list-file-name nil kill-emacs-hook nil neo-count 0)
(with-current-buffer (get-buffer-create "in-flight-save")
  (insert "payload")
  (setq buffer-auto-save-file-name "/nonexistent-neomacs-probe-dir/inflight/#x#"))
(setq ring-bell-function (lambda () (error "exit-save-failed")))
(setq neo-probe-hook
      (list (lambda () (condition-case e (kill-emacs 0)
                         (error (message "CAUGHT=%S" e))))
            (lambda () (setq neo-count (1+ neo-count)) (message "SECOND-RAN"))))
(run-hooks 'neo-probe-hook)
(message "COUNT=%S DYING-UNTIL-FAILURE=%S" neo-count
         (run-hook-with-args-until-failure 'neo-probe-hook))
(with-current-buffer "in-flight-save" (setq buffer-auto-save-file-name nil))
(kill-emacs 0))"##;
    assert_oracle_batch_exit_and_stderr_expect(
        form,
        expect_test::expect![[
            r#""EXIT 0\nCAUGHT=(error \"exit-save-failed\")\nSECOND-RAN\nCOUNT=1 DYING-UNTIL-FAILURE=t""#
        ]],
    );
}

#[cfg(test)]
fn shutdown_native_auto_save_probe(change: &str) -> String {
    format!(
        r##"(progn
  (setq auto-save-list-file-name nil)
  (with-current-buffer (get-buffer-create "native-save")
    (insert "native payload\n")
    (setq buffer-auto-save-file-name
          (expand-file-name "#native-save#" (getenv "NEOVM_ORACLE_TEST_TMPDIR"))))
  {change}
  (kill-emacs 0))"##,
    )
}

#[test]
fn shutdown_auto_save_ignores_lisp_function_redefinition() {
    // `Fdo_auto_save (Qt, Qnil)` is a direct C call (emacs.c:3157).
    // An explicit Lisp call must still see the replacement function.
    let form = shutdown_native_auto_save_probe(
        r#"(fset 'do-auto-save
        (lambda (&rest _args)
          (write-region "override\n" nil
                        (expand-file-name "override.log"
                                          (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
                        t 'silent)))
  (do-auto-save t)"#,
    );
    assert_shutdown_probe_artifacts(
        &form,
        &[
            ("#native-save#", Some(b"native payload\n")),
            ("override.log", Some(b"override\n")),
        ],
    );
}

#[test]
fn shutdown_auto_save_ignores_unbound_lisp_function() {
    let form = shutdown_native_auto_save_probe(r#"(fmakunbound 'do-auto-save)"#);
    assert_shutdown_probe_artifacts(
        &form,
        &[
            ("#native-save#", Some(b"native payload\n")),
            ("override.log", None),
        ],
    );
}

#[test]
fn shutdown_auto_save_propagates_a_handler_throw() {
    // A throw from auto_save_error leaves Fdo_auto_save and shut_down_emacs;
    // it does not publish an exit request (fileio.c:6478, emacs.c:3157).
    let form = r#"(progn
  (setq auto-save-list-file-name nil neo-throw-kills 0)
  (setq kill-emacs-hook
        (list (lambda () (setq neo-throw-kills (1+ neo-throw-kills)))))
  (with-current-buffer (get-buffer-create "throw-save")
    (insert "payload")
    (setq buffer-auto-save-file-name "/nonexistent-neomacs-probe-dir/throw/x"))
  (setq ring-bell-function (lambda () (throw 'save-escape 'escaped)))
  (message "THROW=%S" (catch 'save-escape (kill-emacs 0) 'not-thrown))
  (message "HOOKS=%S DEAD=%S" neo-throw-kills
           (run-hook-with-args-until-failure 'kill-emacs-hook))
  (with-current-buffer "throw-save" (setq buffer-auto-save-file-name nil))
  (kill-emacs 3))"#;
    assert_oracle_batch_exit_and_stderr_expect(
        form,
        expect_test::expect![[r#""EXIT 3\nTHROW=escaped\nHOOKS=1 DEAD=t""#]],
    );
}

#[test]
fn shutdown_auto_save_preserves_a_nested_exit_code() {
    // Peer GNU probe: the inner exit wins, hooks stay dead, and the failing
    // buffer is retried, calling display-warning a second time.
    let form = r##"(progn
  (setenv "ASPROBE" (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
(setq logf (expand-file-name "log.txt" (getenv "ASPROBE")))
(defun probe-log (s) (write-region (concat s "\n") nil logf t))
(setq kh 0)
(add-hook 'kill-emacs-hook (lambda () (setq kh (1+ kh)) (probe-log (format "KILL-EMACS-HOOK %d" kh))))
(setq dw-calls 0)
(fset 'display-warning
      (lambda (&rest _args)
        (setq dw-calls (1+ dw-calls))
        (probe-log (format "DISPLAY-WARNING call %d" dw-calls))
        (when (= dw-calls 1) (kill-emacs 7))))
(with-current-buffer (get-buffer-create "x")
  (setq buffer-file-name (expand-file-name "x.txt" (getenv "ASPROBE")))
  (setq buffer-auto-save-file-name "/nonexistent-neomacs-probe-dir/q/#x.txt#")
  (insert "data") (set-buffer-modified-p t))
(kill-emacs 0)
(probe-log "AFTER (never)"))"##;
    assert_shutdown_probe_exit_artifacts(
        form,
        7,
        &[(
            "log.txt",
            Some(b"KILL-EMACS-HOOK 1\nDISPLAY-WARNING call 1\nDISPLAY-WARNING call 2\n"),
        )],
    );
}

#[test]
fn nested_kill_emacs_hook_reenters_with_the_inner_exit_code() {
    // GNU Fkill_emacs runs the hook list again on nested entry, with no
    // InProgress recursion guard (emacs.c:2954,3015-3028). The inner exit
    // never returns to the outer hook walk, so the second hook runs once.
    let form = r##"(progn
  (setenv "ASPROBE" (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
(setq logf (expand-file-name "log.txt" (getenv "ASPROBE")))
(defun probe-log (s) (write-region (concat s "\n") nil logf t))
(setq n 0)
(add-hook 'kill-emacs-hook (lambda () (setq n (1+ n)) (probe-log (format "HOOK %d" n)) (when (= n 1) (kill-emacs 5))))
(add-hook 'kill-emacs-hook (lambda () (probe-log "SECOND-HOOK")) t)
(kill-emacs 0)
(probe-log "AFTER (never)"))"##;
    assert_shutdown_probe_exit_artifacts(
        form,
        5,
        &[("log.txt", Some(b"HOOK 1\nHOOK 2\nSECOND-HOOK\n"))],
    );
}

// Canonical write-region dispatch from explicit and native-shutdown auto-save.
// GNU 31.1 fileio.c:6249-6273/6478-6482 and emacs.c:3107-3157;
// expectation evidence: pinned GNU auto-save-handler r1 and r3.
#[cfg(test)]
fn assert_auto_save_write_region_handler_oracle_pair(
    form: &str,
    expected: &[(&str, Option<&[u8]>)],
) {
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(
        oracle.is_attested(),
        "auto-save handler parity requires the pinned GNU attestation; operator opt-out is not evidence"
    );
    eprintln!("AUTO_SAVE_HANDLER_ORACLE reference={}", oracle.stamp());
    assert_shutdown_probe_artifacts(form, expected);
}

#[cfg(test)]
fn auto_save_write_region_handler_oracle_form(action: &str, visit: bool, tail: &str) -> String {
    let visited = if visit { "t" } else { "nil" };
    format!(
        r#"(progn
  (let ((subject-pid (emacs-pid)))
    (princ (format "AUTO_SAVE_HANDLER_SUBJECT_PID=%s\n" subject-pid))
    (if (<= subject-pid 1) (error "oracle subject must not be PID namespace init")))
  (setq auto-save-list-file-name nil create-lockfiles nil
        asr-oracle-count 0 asr-oracle-action '{action}
        asr-oracle-root (file-name-as-directory (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
        asr-oracle-target (expand-file-name "handled-save" asr-oracle-root)
        asr-oracle-trace (expand-file-name "handler.trace" asr-oracle-root))
  (make-directory (expand-file-name "sub" asr-oracle-root) t)
  (fset 'asr-oracle-note
    (lambda (value)
      (let ((write-region-annotate-functions nil)
            (write-region-post-annotation-function nil)
            (buffer-file-format nil) (buffer-auto-save-file-format nil))
        (write-region (concat (prin1-to-string value) "\n") nil
                      asr-oracle-trace t 'silent))))
  (fset 'asr-oracle-delegate
    (lambda (operation arguments)
      (let ((inhibit-file-name-handlers
             (cons 'asr-oracle-handler
                   (and (eq inhibit-file-name-operation operation)
                        inhibit-file-name-handlers)))
            (inhibit-file-name-operation operation))
        (apply operation arguments))))
  (fset 'asr-oracle-handler
    (lambda (operation &rest arguments)
      (if (eq operation 'write-region)
          (progn
            (setq asr-oracle-count (1+ asr-oracle-count))
            ;; Keep every canonical argument observable. Only the two
            ;; case-private absolute paths become equality predicates.
            (asr-oracle-note
             (list 'ARGS operation (length arguments)
                   (nth 0 arguments) (nth 1 arguments)
                   (equal (nth 2 arguments) asr-oracle-target)
                   (nth 3 arguments) (nth 4 arguments)
                   (equal (nth 5 arguments) asr-oracle-target)
                   (nth 6 arguments)))
            (cond ((eq asr-oracle-action 'throw)
                   (throw 'asr-oracle-escape 'handler-escaped))
                  ((eq asr-oracle-action 'mutation)
                   (goto-char (point-max)) (insert "handler-edit\n")
                   (asr-oracle-delegate operation arguments))
                  (t 'handled)))
        (asr-oracle-delegate operation arguments))))
  (setq file-name-handler-alist
        (cons (cons (concat "\\`" (regexp-quote asr-oracle-target) "\\'")
                    'asr-oracle-handler)
              file-name-handler-alist)
        auto-save-hook (list (lambda () (asr-oracle-note 'AUTO_SAVE_HOOK))))
  (set-buffer (get-buffer-create "asr-oracle-subject"))
  (setq asr-oracle-subject (current-buffer)
        default-directory asr-oracle-root
        buffer-file-name (expand-file-name "ordinary-file" asr-oracle-root)
        buffer-auto-save-file-name "sub/../handled-save"
        auto-save-visited-file-name {visited})
  (insert "seed\n") (set-buffer-modified-p t)
  {tail})"#
    )
}

#[test]
fn auto_save_write_region_handler_oracle_running_mutation_and_full_arguments() {
    let form = auto_save_write_region_handler_oracle_form(
        "mutation",
        false,
        r#"
  (asr-oracle-note (list 'RETURN (do-auto-save t t)))
  (asr-oracle-note (list 'STATE asr-oracle-count buffer-saved-size
                         (recent-auto-save-p) (buffer-modified-p)))
  (setq buffer-auto-save-file-name nil)
  (set-buffer-modified-p nil)
  (kill-emacs 0)"#,
    );
    assert_auto_save_write_region_handler_oracle_pair(&form, &[
        ("handler.trace", Some(b"AUTO_SAVE_HOOK\n(ARGS write-region 7 nil nil t nil lambda t nil)\n(RETURN nil)\n(STATE 1 18 t autosaved)\n")),
        ("handled-save", Some(b"seed\nhandler-edit\n")),
    ]);
}

#[test]
fn auto_save_write_region_handler_oracle_running_original_matched_throw() {
    let form = auto_save_write_region_handler_oracle_form(
        "throw",
        false,
        r#"
  (set-buffer (get-buffer-create "asr-oracle-controller"))
  (asr-oracle-note (list 'RETURN (catch 'asr-oracle-escape (do-auto-save t nil))))
  (asr-oracle-note (list 'STATE asr-oracle-count
                         (eq (current-buffer) asr-oracle-subject)
                         buffer-saved-size (recent-auto-save-p) (buffer-modified-p)))
  (setq buffer-auto-save-file-name nil)
  (set-buffer-modified-p nil)
  (kill-emacs 0)"#,
    );
    assert_auto_save_write_region_handler_oracle_pair(&form, &[
        ("handler.trace", Some(b"AUTO_SAVE_HOOK\n(ARGS write-region 7 nil nil t nil lambda t nil)\n(RETURN handler-escaped)\n(STATE 1 t 0 nil t)\n")),
        ("handled-save", None),
    ]);
}

#[test]
fn auto_save_write_region_handler_oracle_shutdown_full_arguments_without_hook() {
    let form = auto_save_write_region_handler_oracle_form(
        "return",
        true,
        r#"
  (setq buffer-auto-save-file-name asr-oracle-target)
  (set-buffer (get-buffer-create "asr-oracle-controller"))
  (kill-emacs 0)"#,
    );
    assert_auto_save_write_region_handler_oracle_pair(
        &form,
        &[
            (
                "handler.trace",
                Some(b"(ARGS write-region 7 nil nil t nil t t nil)\n"),
            ),
            ("handled-save", None),
        ],
    );
}

#[test]
fn auto_save_write_region_handler_oracle_shutdown_original_matched_throw() {
    let form = auto_save_write_region_handler_oracle_form(
        "throw",
        false,
        r#"
  (setq buffer-auto-save-file-name asr-oracle-target)
  (set-buffer (get-buffer-create "asr-oracle-controller"))
  (asr-oracle-note (list 'RETURN (catch 'asr-oracle-escape (kill-emacs 0))))
  (asr-oracle-note (list 'STATE asr-oracle-count
                         (eq (current-buffer) asr-oracle-subject)
                         buffer-saved-size (recent-auto-save-p) (buffer-modified-p)))
  (setq buffer-auto-save-file-name nil)
  (set-buffer-modified-p nil)
  (kill-emacs 0)"#,
    );
    assert_auto_save_write_region_handler_oracle_pair(&form, &[
        ("handler.trace", Some(b"(ARGS write-region 7 nil nil t nil lambda t nil)\n(RETURN handler-escaped)\n(STATE 1 t 0 nil t)\n")),
        ("handled-save", None),
    ]);
}
