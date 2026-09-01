//! Oracle parity tests for argv handling.
//!
//! Each test asserts that GNU `emacs` and our `neomacs` produce
//! comparable observable startup state when invoked with the same argv.
//! Tests gate on `NEOVM_FORCE_ORACLE_PATH` (the same env var
//! `neovm-oracle-tests` uses); when unset, every test exits early so
//! CI without GNU Emacs available still passes.
//!
//! Background: see `drafts/argv-parity-audit.md` for the ground-truth
//! `standard_args[]` table from `emacs.c:2646-2766` and the per-flag
//! gap analysis these tests gate against.
//!
//! ## Scope today
//!
//! These tests cover both early C-side exits (`--help`, `--version`,
//! `--chdir` failure) and batch startup paths where Lisp observes the
//! sorted/forwarded `command-line-args`. When a parity gap is known, the
//! specific test remains ignored with that gap stated on the test itself.

mod common;

use common::{
    ProbeResult, oracle_enabled, run_neomacs, run_neomacs_with_stdin, run_oracle_emacs,
    run_oracle_emacs_with_stdin,
};

fn assert_status_eq(neomacs: &ProbeResult, emacs: &ProbeResult, label: &str) {
    assert_eq!(
        neomacs.status, emacs.status,
        "{label}: exit status differs.\nneomacs: {:?}\nemacs: {:?}",
        neomacs, emacs,
    );
}

fn assert_stdout_parity(neomacs: &ProbeResult, emacs: &ProbeResult, label: &str) {
    assert_eq!(
        neomacs.stdout.trim(),
        emacs.stdout.trim(),
        "{label}: stdout differs.\nneomacs stdout: {:?}\nneomacs stderr: {:?}\nemacs stdout: {:?}\nemacs stderr: {:?}",
        neomacs.stdout,
        neomacs.stderr,
        emacs.stdout,
        emacs.stderr,
    );
}

// ---------- enabled today ----------

#[test]
fn batch_read_from_minibuffer_honors_read_and_default_with_stdin() {
    let argv = [
        "--quick",
        "--batch",
        "--eval",
        "(prin1 (list (read-from-minibuffer \"\" nil nil t) \
                      (read-from-minibuffer \"\" nil nil t nil \"42\") \
                      (read-from-minibuffer \"\" nil nil nil)))",
    ];
    let stdin = "(a b)\n\nplain\n";
    let n = run_neomacs_with_stdin(&argv, stdin);
    assert_eq!(n.status, 0, "batch minibuffer read failed: {n:?}");
    assert_eq!(n.stdout.trim(), "((a b) 42 \"plain\")", "{n:?}");

    if oracle_enabled() {
        let e = run_oracle_emacs_with_stdin(&argv, stdin);
        assert_status_eq(&n, &e, "batch read-from-minibuffer exit");
        assert_stdout_parity(&n, &e, "batch read-from-minibuffer result");
    }
}

#[test]
fn version_flag_exits_zero_and_prints_something() {
    skip_unless_oracle!();
    // GNU emacs.c:1508 / 2222 — `--version` prints version info and
    // exits 0. We also handle this via `classify_early_cli_action`
    // which short-circuits before `parse_startup_options`.
    let n = run_neomacs(&["--version"]);
    let e = run_oracle_emacs(&["--version"]);
    assert_status_eq(&n, &e, "--version exit");
    assert!(
        !n.stdout.is_empty(),
        "neomacs --version should print to stdout: {n:?}"
    );
    assert!(
        !e.stdout.is_empty(),
        "emacs --version should print to stdout: {e:?}"
    );
}

#[test]
fn help_flag_exits_zero_and_prints_something() {
    skip_unless_oracle!();
    // GNU emacs.c:1720 — `--help` prints usage and exits 0. We do the
    // same via `classify_early_cli_action`. The exact text differs (we
    // ship our own usage table) but both must exit 0 with non-empty
    // output.
    let n = run_neomacs(&["--help"]);
    let e = run_oracle_emacs(&["--help"]);
    assert_status_eq(&n, &e, "--help exit");
    assert!(!n.stdout.is_empty(), "neomacs --help should print");
    assert!(!e.stdout.is_empty(), "emacs --help should print");
}

#[test]
fn chdir_to_nonexistent_path_fails_with_nonzero_exit() {
    skip_unless_oracle!();
    // GNU emacs.c:1551 — chdir failure prints "X: Can't chdir to Y: Z"
    // to stderr and exits 1. Phase 3a mirrors this exit path. We don't
    // diff the exact stderr text (GNU prefixes with argv[0], which
    // differs by binary name) — only the non-zero exit and the
    // characteristic "chdir to" prefix.
    let n = run_neomacs(&["--chdir", "/this/path/cannot/possibly/exist", "--batch"]);
    let e = run_oracle_emacs(&["--chdir", "/this/path/cannot/possibly/exist", "--batch"]);
    assert_status_eq(&n, &e, "--chdir failure exit");
    assert_ne!(n.status, 0, "neomacs should exit non-zero on chdir failure");
    assert!(
        n.stderr.contains("chdir to"),
        "neomacs stderr should mention chdir failure: {:?}",
        n.stderr
    );
    assert!(
        e.stderr.contains("chdir to"),
        "emacs stderr should mention chdir failure: {:?}",
        e.stderr
    );
}

#[test]
fn batch_eval_prints_result() {
    skip_unless_oracle!();
    let argv = ["--batch", "--eval", "(princ (+ 1 2))"];
    let n = run_neomacs(&argv);
    let e = run_oracle_emacs(&argv);
    assert_stdout_parity(&n, &e, "batch eval parity");
}

#[test]
fn chdir_changes_default_directory() {
    skip_unless_oracle!();
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
    let dir_str = dir.to_string_lossy().into_owned();
    let argv: Vec<&str> = vec![
        "--batch",
        "--chdir",
        &dir_str,
        "--eval",
        "(princ default-directory)",
    ];
    let n = run_neomacs(&argv);
    let e = run_oracle_emacs(&argv);
    assert_stdout_parity(&n, &e, "chdir parity");
}

#[test]
fn quick_passes_through_to_lisp() {
    skip_unless_oracle!();
    // -Q must remain in command-line-args after the C-side peek so the
    // Lisp side can also act on it (Phase 3d).
    let argv = [
        "-Q",
        "--batch",
        "--eval",
        "(princ (member \"-Q\" command-line-args))",
    ];
    let n = run_neomacs(&argv);
    let e = run_oracle_emacs(&argv);
    assert_stdout_parity(&n, &e, "-Q peek parity");
}

#[test]
fn no_site_lisp_drops_site_lisp_from_load_path() {
    skip_unless_oracle!();
    let argv = [
        "--no-site-lisp",
        "--batch",
        "--eval",
        "(princ (catch 'found (dolist (p load-path nil) (when (and (stringp p) (string-match-p \"site-lisp\" p)) (throw 'found t)))))",
    ];
    let n = run_neomacs(&argv);
    let e = run_oracle_emacs(&argv);
    assert_stdout_parity(&n, &e, "--no-site-lisp parity");
}

#[test]
fn batch_implies_noninteractive() {
    skip_unless_oracle!();
    let argv = ["--batch", "--eval", "(princ (if noninteractive 't 'nil))"];
    let n = run_neomacs(&argv);
    let e = run_oracle_emacs(&argv);
    assert_stdout_parity(&n, &e, "--batch noninteractive parity");
}

#[test]
fn sort_args_orders_options_canonically() {
    skip_unless_oracle!();
    // The sort_args (Phase 2) parity check: any permutation of the
    // same flag set must produce the same canonical command-line-args
    // when walked by lisp/startup.el.
    // Skip argv[0]: Cargo invokes Neomacs by full test binary path while
    // the oracle is usually invoked as just `emacs`.
    let probe = "(princ (mapconcat 'identity (cdr command-line-args) \"|\"))";
    let argv_a = ["--batch", "-Q", "--eval", probe];
    let argv_b = ["-Q", "--batch", "--eval", probe];

    let na = run_neomacs(&argv_a);
    let nb = run_neomacs(&argv_b);
    let ea = run_oracle_emacs(&argv_a);
    let eb = run_oracle_emacs(&argv_b);

    assert_stdout_parity(&na, &ea, "sort_args parity (variant a)");
    assert_stdout_parity(&nb, &eb, "sort_args parity (variant b)");
    assert_eq!(
        na.stdout.trim(),
        nb.stdout.trim(),
        "neomacs sort_args should canonicalize ordering across permutations"
    );
}

#[test]
fn double_dash_terminator_passes_through() {
    skip_unless_oracle!();
    let argv = [
        "--batch",
        "--eval",
        "(princ (member \"literal-arg\" command-line-args))",
        "--",
        "literal-arg",
    ];
    let n = run_neomacs(&argv);
    let e = run_oracle_emacs(&argv);
    assert_stdout_parity(&n, &e, "-- terminator parity");
}

/// A `--batch` session must never run `emacs-startup-hook`.
///
/// `lisp/startup.el:784-818` (GNU `:774-808`) is one `unwind-protect` whose body is
/// `(command-line)` and whose cleanup ends in
/// `(unless inhibit-startup-hooks (run-hooks 'emacs-startup-hook
/// 'term-setup-hook))`.  `command-line` finishes processing `--load`/`--eval`
/// in `command-line-1` and then hits `:1757` (GNU `:1739`):
///
/// ```elisp
///   ;; If -batch, terminate after processing the command options.
///   (if noninteractive (kill-emacs t))
/// ```
///
/// GNU's `Fkill_emacs` is `attributes: noreturn` (src/emacs.c:2974) and ends
/// in `exit (exit_code)` (:3088), so the cleanup never runs and neither hook
/// fires in a batch session.  A port whose `kill-emacs` unwinds the specpdl instead runs
/// both -- after the last `--eval` and after `kill-emacs-hook` -- which is
/// what `parity_tests::affe::affe_backend_package_batch` was failing on
/// (ledger 203).
#[test]
fn batch_never_runs_emacs_startup_hook() {
    skip_unless_oracle!();
    let probe = "(progn \
                   (add-hook 'emacs-startup-hook \
                             (lambda () (princ \"STARTUP-HOOK-RAN\"))) \
                   (add-hook 'term-setup-hook \
                             (lambda () (princ \"TERM-SETUP-HOOK-RAN\"))) \
                   (princ \"EVAL-DONE\"))";
    let argv = ["--batch", "--quick", "--eval", probe];
    let n = run_neomacs(&argv);
    let e = run_oracle_emacs(&argv);
    assert_stdout_parity(&n, &e, "batch emacs-startup-hook parity");
    assert_eq!(
        e.stdout.trim(),
        "EVAL-DONE",
        "GNU's batch kill-emacs is noreturn, so neither hook runs"
    );
    assert_status_eq(&n, &e, "batch emacs-startup-hook exit");
}

/// The same contract in the shape the package corpus actually hit.
///
/// `affe-backend.el` ends with `(add-hook 'emacs-startup-hook
/// #'affe-backend--setup)`, and `affe-backend--setup`'s first form is
/// `(set-process-coding-system server-process ...)`.  `server-process` is
/// `server.el`'s `defvar`, `nil` in a session that never started a server, so
/// running that hook at all signals `Wrong type argument: processp, nil` after
/// every probe in the batch has already passed -- and takes the exit status to
/// 255 with it.
#[test]
fn batch_startup_hook_signal_cannot_reach_a_completed_session() {
    skip_unless_oracle!();
    let probe = "(progn \
                   (require 'server) \
                   (add-hook 'emacs-startup-hook \
                             (lambda () \
                               (set-process-coding-system server-process \
                                                          'utf-8 'utf-8))) \
                   (princ \"ALL-PROBES-PASSED\"))";
    let argv = ["--batch", "--quick", "--eval", probe];
    let n = run_neomacs(&argv);
    let e = run_oracle_emacs(&argv);
    assert_stdout_parity(&n, &e, "affe teardown shape stdout");
    assert_eq!(e.status, 0, "GNU completes this batch session cleanly");
    assert_status_eq(&n, &e, "affe teardown shape exit");
    assert!(
        !n.stderr.contains("processp"),
        "neomacs must not signal out of a hook GNU never runs: {:?}",
        n.stderr
    );
}
