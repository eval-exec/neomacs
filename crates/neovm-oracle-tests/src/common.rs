//! Shared oracle helpers for Elisp unit tests.
//!
//! These helpers are intentionally test-only. The default snapshot mode only
//! requires a Neomacs release binary at `target/release/neomacs` (or
//! `NEOVM_BINARY_PATH`). Live oracle modes also require GNU Emacs on PATH (or
//! via `NEOVM_FORCE_ORACLE_PATH`).

use colored::Colorize;
use neomacs_parity_reference::ReferenceUse;
use neomacs_test_env::PinnedCommand;
use neomacs_test_oracle::CapturedEvaluation;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[path = "oracle_sandbox.rs"]
pub(crate) mod oracle_sandbox;

use oracle_sandbox::{OracleSandbox, ResultNormalization};

#[cfg(unix)]
fn apply_virtual_memory_limit(cmd: &mut PinnedCommand, mem_limit: u64) {
    if !neomacs_infra::ADDRESS_SPACE_LIMIT_SUPPORTED {
        return;
    }
    cmd.limit_address_space(mem_limit);
}

#[cfg(not(unix))]
fn apply_virtual_memory_limit(_cmd: &mut PinnedCommand, _mem_limit: u64) {}

/// Maximum virtual address space (in bytes) for each spawned oracle Emacs
/// process.  This prevents runaway evaluations from consuming unbounded
/// memory and triggering the system OOM killer.
/// Overridable via `NEOVM_ORACLE_MEM_LIMIT_MB` (default: 500 MB).
fn oracle_mem_limit_bytes() -> u64 {
    let mb: u64 = std::env::var("NEOVM_ORACLE_MEM_LIMIT_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);
    mb * 1024 * 1024
}

/// Optional virtual address space cap for spawned release Neomacs binary
/// checks. Unlike the GNU oracle process, release Neomacs can legitimately
/// map several gigabytes while running exhaustive recursive parity cases, and
/// some nextest child processes cannot raise a lower inherited hard limit.
///
/// Set `NEOVM_NEOMACS_BINARY_MEM_LIMIT_MB` to enable an extra cap.
fn neomacs_binary_mem_limit_bytes() -> Option<u64> {
    let mb: u64 = std::env::var("NEOVM_NEOMACS_BINARY_MEM_LIMIT_MB")
        .ok()
        .and_then(|v| v.parse().ok())?;
    Some(mb * 1024 * 1024)
}

pub(crate) const ORACLE_PROP_CASES: u32 = 10;

/// Whether a test that embeds a GNU expectation may run.
///
/// Snapshot mode runs Neomacs only and needs no GNU.  Every other mode
/// resolves the attested oracle here; a missing or mismatched oracle PANICS
/// with the env var and the one-recipe build script.  There is deliberately no
/// state in which this returns false.
pub(crate) fn oracle_prop_enabled() -> bool {
    OracleMode::from_env() == OracleMode::Snapshot || oracle_emacs_available()
}

/// Whether a GNU-comparison test may run.  Live modes resolve-or-panic for the
/// same reason; the false branch is unreachable.
pub(crate) fn live_oracle_enabled() -> bool {
    OracleMode::from_env() != OracleMode::Snapshot && oracle_emacs_available()
}

fn oracle_timing_enabled() -> bool {
    std::env::var_os("NEOVM_ORACLE_TIMING").is_some()
}

/// Execution strategy for oracle tests that embed GNU Emacs expectations in
/// the Rust test source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OracleMode {
    /// Fast path: run only Neomacs and compare its result with the checked-in
    /// inline GNU expectation.
    Snapshot,
    /// Consistency check: run GNU Emacs, compare it with the inline
    /// expectation, then require Neomacs to match the same live GNU result.
    Verify,
    /// Maintenance path: run GNU Emacs and compare it with the inline
    /// expectation; with `UPDATE_EXPECT=1`, `expect-test` rewrites the source.
    Refresh,
    /// Legacy parity path: run GNU Emacs and Neomacs directly, ignoring the
    /// inline expectation.
    Live,
}

impl OracleMode {
    fn from_env() -> Self {
        match std::env::var("NEOVM_ORACLE_MODE")
            .unwrap_or_else(|_| "snapshot".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "snapshot" | "snap" | "expected" => Self::Snapshot,
            "verify" => Self::Verify,
            "refresh" | "bless" | "update" => Self::Refresh,
            "live" => Self::Live,
            other => panic!(
                "unknown NEOVM_ORACLE_MODE={other:?}; expected snapshot, verify, refresh, or live"
            ),
        }
    }
}

// Historical call-site marker.  `oracle_prop_enabled` is always true now:
// snapshot mode needs no GNU, and every live mode resolves the attested oracle
// or panics.  The guard cannot skip a test.
macro_rules! return_if_neovm_enable_oracle_proptest_not_set {
    () => {
        if !$crate::common::oracle_prop_enabled() {
            tracing::info!(
                "skipping {}:{}: set NEOVM_FORCE_ORACLE_PATH=/path/to/emacs",
                module_path!(),
                line!()
            );
            return;
        }
    };
    ($ret:expr) => {
        if !$crate::common::oracle_prop_enabled() {
            tracing::info!(
                "skipping {}:{}: set NEOVM_FORCE_ORACLE_PATH=/path/to/emacs",
                module_path!(),
                line!()
            );
            return $ret;
        }
    };
}

pub(crate) use return_if_neovm_enable_oracle_proptest_not_set;

/// The attested GNU this process may run.
///
/// Resolution and identity attestation are the workspace's one resolver,
/// `neomacs_test_oracle::gnu`: `NEOVM_FORCE_ORACLE_PATH` when set, otherwise
/// the pinned reference checked on the `emacs` found on `PATH`.  A missing or
/// mismatched oracle PANICS with the env var and the one-recipe build script;
/// a live oracle run must never skip.  Snapshot mode never reaches this
/// function, which is what lets the snapshot suite run without GNU at all.
fn attested_oracle_emacs() -> &'static ReferenceUse {
    neomacs_test_oracle::gnu::gnu_oracle().reference()
}

/// The attested executable, for callers that only need the path.
pub(crate) fn oracle_emacs_path() -> PathBuf {
    neomacs_test_oracle::gnu::gnu_oracle().executable_path()
}

/// Whether a GNU oracle may be run.
///
/// Resolution either returns an attested oracle or fails loudly; there is no
/// "present but unusable" state left to turn into a skip.
fn oracle_emacs_available() -> bool {
    let _ = attested_oracle_emacs();
    true
}

fn neomacs_binary_path() -> String {
    std::env::var("NEOVM_BINARY_PATH").unwrap_or_else(|_| {
        oracle_sandbox::project_root()
            .join("target/release/neomacs")
            .to_string_lossy()
            .into_owned()
    })
}

// ---------------------------------------------------------------------------
// Frozen wall clock (libfaketime) for date/time-sensitive oracle tests
// ---------------------------------------------------------------------------
//
// A handful of oracle forms embed the *run-time* wall clock in their output:
// org-agenda's "today" line and week header, `%t`/`%U` capture timestamps,
// clock/export/archive stamps, and so on. The GNU recording process and the
// Neomacs replay process sample the clock seconds apart -- and, across a
// midnight boundary, on different days -- so that output is non-deterministic.
//
// Rather than scrub every timestamp format out of the serialized result with
// bespoke regexes (an open-ended, error-prone game of whack-a-mole -- a greedy
// pattern can silently mask a *real* divergence), the tests that need it opt
// into a frozen clock: both subprocesses run under libfaketime pinned to one
// fixed instant, so their raw output already agrees with no normalization.
// libfaketime interposes glibc's `clock_gettime`/`gettimeofday`, which is the
// one layer both engines share (GNU's C `current_timespec` and Neomacs's Rust
// `std::time` both bottom out there), so it catches even `format-time-string`,
// which reads the C clock directly and would slip past a Lisp-level redefinition
// of `current-time`.

/// A fixed wall-clock instant shared by every frozen-clock oracle test: Monday
/// 2026-06-15 12:00:00. Deliberately an unremarkable weekday far from any real
/// suite run date, so a checked-in expectation never depends on when the suite
/// runs. The leading `@` pins the clock (it does not advance from this instant).
const ORACLE_FROZEN_TIME: &str = "@2026-06-15 12:00:00";

/// Library file names libfaketime installs under a prefix's `lib/`.
///
/// The interposer is a shared object on Linux and a dylib on Darwin; a harness
/// that only knows the Linux name cannot freeze the clock on a Mac even when
/// the library is installed there.
const LIBFAKETIME_NAMES: &[&str] = if cfg!(target_os = "macos") {
    &[
        "lib/libfaketime.1.dylib",
        "lib/faketime/libfaketime.1.dylib",
        "lib/libfaketime.dylib",
    ]
} else {
    &[
        "lib/libfaketime.so.1",
        "lib/faketime/libfaketime.so.1",
        "lib64/libfaketime.so.1",
        "lib64/faketime/libfaketime.so.1",
    ]
};

/// Resolve the libfaketime shared library. Prefer the explicit
/// `NEOVM_LIBFAKETIME_SO` override (set by the flake devShell for
/// reproducibility); otherwise locate it relative to the `faketime` binary on
/// `PATH`. Frozen-clock tests cannot be deterministic without it, so a missing
/// library is a hard, clearly-explained error rather than a silently flaky
/// pass.
fn libfaketime_so_path() -> String {
    if let Ok(explicit) = std::env::var("NEOVM_LIBFAKETIME_SO")
        && !explicit.is_empty()
    {
        return explicit;
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let bin = dir.join("faketime");
            if !bin.is_file() {
                continue;
            }
            // Canonicalize resolves the nix-profile symlink to the real
            // <prefix>/bin/faketime, whose sibling <prefix>/lib holds the
            // library.
            let Ok(real) = std::fs::canonicalize(&bin) else {
                continue;
            };
            let Some(prefix) = real.parent().and_then(|p| p.parent()) else {
                continue;
            };
            for rel in LIBFAKETIME_NAMES {
                let library = prefix.join(rel);
                if library.is_file() {
                    return library.to_string_lossy().into_owned();
                }
            }
        }
    }
    panic!(
        "frozen-time oracle tests require libfaketime, but {} could not be located. \
         Set NEOVM_LIBFAKETIME_SO to its path, or run inside `nix develop` (the devShell provides it).",
        LIBFAKETIME_NAMES[0]
    );
}

/// The env vars that pin a spawned engine's wall clock to [`ORACLE_FROZEN_TIME`].
/// `FAKETIME_DONT_FAKE_MONOTONIC=1` freezes only `CLOCK_REALTIME` (the wall
/// clock that leaks into timestamps) and leaves `CLOCK_MONOTONIC` real, so the
/// subprocess's own `select`/timeout logic is unaffected.
fn frozen_time_env() -> Vec<(String, String)> {
    let library = libfaketime_so_path();
    // Linux interposes through the dynamic loader's LD_PRELOAD; Darwin
    // ignores that variable and uses dyld's flat-namespace insertion.
    let mut env = if cfg!(target_os = "macos") {
        vec![
            ("DYLD_INSERT_LIBRARIES".to_string(), library),
            ("DYLD_FORCE_FLAT_NAMESPACE".to_string(), "1".to_string()),
        ]
    } else {
        vec![("LD_PRELOAD".to_string(), library)]
    };
    env.push(("FAKETIME".to_string(), ORACLE_FROZEN_TIME.to_string()));
    env.push(("FAKETIME_DONT_FAKE_MONOTONIC".to_string(), "1".to_string()));
    env
}

fn project_lisp_dir() -> PathBuf {
    oracle_sandbox::project_root().join("lisp")
}

fn oracle_sandbox(form: &str, load_files: &[&str], load_root: &Path) -> OracleSandbox {
    OracleSandbox::new(form, load_files, load_root).expect("oracle sandbox should be created")
}

fn ensure_nonempty_form(form: &str) -> Result<(), String> {
    if form.trim().is_empty() {
        Err("no form parsed".to_string())
    } else {
        Ok(())
    }
}

const ORACLE_OUTCOME_MARKER: &str = "NEOVM-ORACLE-OUTCOME:";

const EVAL_PROGRAM_WITH_NORMALIZER: &str = r#"(condition-case err
    (progn
      (defun neovm--oracle-emit-outcome (kind value)
        (princ "\n" 'external-debugging-output)
        (princ "NEOVM-ORACLE-OUTCOME:" 'external-debugging-output)
        (princ kind 'external-debugging-output)
        (let ((print-escape-newlines t))
          (prin1 value 'external-debugging-output))
        (terpri 'external-debugging-output))
      (defun neovm--oracle-normalize-1 (v seen)
        (cond
         ;; Opaque handles print with implementation-specific identities:
         ;; GNU uses addresses for threads/mutexes/condition variables, while
         ;; Neomacs uses simulated ids.  Normalize to stable semantic tokens
         ;; before generic cons/vector traversal can copy Neomacs handles.
         ;; Thread liveness is intentionally not part of the opaque thread
         ;; token: GNU `make-thread' returns before the worker necessarily
         ;; exits, so `(thread-live-p v)' is scheduler-sensitive for short
         ;; thread functions.  Tests that need liveness should call
         ;; `thread-live-p' explicitly.
         ((and (fboundp 'threadp) (threadp v))
          (list :thread
                (and (fboundp 'thread-name) (thread-name v))))
         ((and (fboundp 'mutexp) (mutexp v))
          (list :mutex
                (and (fboundp 'mutex-name) (mutex-name v))))
         ((and (fboundp 'condition-variable-p) (condition-variable-p v))
          (list :condition-variable
                (and (fboundp 'condition-name) (condition-name v))
                (and (fboundp 'condition-mutex)
                     (let ((m (condition-mutex v)))
                       (and (fboundp 'mutexp)
                            (mutexp m)
                            (list :mutex
                                  (and (fboundp 'mutex-name)
                                       (mutex-name m))))))))
         ;; A raw frame object is an identity, not a value: GNU prints a real
         ;; pointer (`#<frame F1 0x555555b467d8>') and Neomacs a synthetic id
         ;; (`#<frame F1 0x100000000>').  The string branch below squashes
         ;; addresses inside printed messages, but a frame embedded in a
         ;; RESULT list never passes through a string, so normalize it to a
         ;; stable semantic token.  Frame type and name remain observable.
         ((and (fboundp 'framep) (framep v))
          (list :frame
                (and (fboundp 'frame-parameter)
                     (frame-parameter v 'name))))
         ((and (functionp v) (eq (type-of v) 'interpreted-function))
          (let ((args (aref v 0))
                (body (aref v 1))
                (env (aref v 2)))
            (if (null env)
                (cons 'lambda
                      (cons (neovm--oracle-normalize-1 args seen)
                            (neovm--oracle-normalize-1 body seen)))
              (cons 'closure
                    (cons (neovm--oracle-normalize-1 env seen)
                          (cons (neovm--oracle-normalize-1 args seen)
                                (neovm--oracle-normalize-1 body seen)))))))
         ((consp v)
          (or (gethash v seen)
              (let ((out (cons nil nil)))
                (puthash v out seen)
                (setcar out (neovm--oracle-normalize-1 (car v) seen))
                (setcdr out (neovm--oracle-normalize-1 (cdr v) seen))
                out)))
         ((vectorp v)
          (or (gethash v seen)
              (let* ((len (length v))
                     (out (make-vector len nil)))
                (puthash v out seen)
                (dotimes (i len)
                  (aset out i (neovm--oracle-normalize-1 (aref v i) seen)))
                out)))
         ;; Large fixnums in error data are implementation artefacts:
         ;; Neomacs uses a hardcoded sentinel for unfilled concat slots in
         ;; mapconcat, while GNU reuses uninitialised stack memory.  Both are
         ;; non-deterministic across builds, so squash them to 0 for parity.
         ((fixnump v) (if (> (abs v) 1000000000000) 0 v))
         ;; Frame/icon title product branding is a DELIBERATE Neomacs divergence:
         ;; GNU titles read "%b - GNU Emacs at HOST" while Neomacs -- which must
         ;; never advertise "GNU Emacs" -- reads "%b - NEO Emacs at HOST" (see
         ;; frame_vars.rs).  Canonicalize the product name on BOTH engines so the
         ;; frame-title-format STRUCTURE stays a real parity lock while this one
         ;; intentional brand difference is ignored.  The per-run tempdir
         ;; (`temporary-file-directory' / an OracleSandbox case directory) is
         ;; shared within a run but differs across
         ;; record/replay, so it is squashed to a stable token.
         ;;
         ;; Wall-clock date/time is NOT scrubbed here.  Date-sensitive tests run
         ;; under a frozen clock via `assert_oracle_parity_frozen_time_*', so both
         ;; engines emit the identical instant and their raw output already agrees
         ;; -- the non-determinism is fixed at the source instead of matching every
         ;; timestamp format with a regex.
         ;;
         ;; The configured load tree (`NEOVM_ORACLE_LOAD_ROOT') leaks into
         ;; engine output through `load'
         ;; error data, `load-file-name', `locate-library', and load-path
         ;; echoes.  That prefix depends on where the checkout lives (main
         ;; repo vs per-agent worktree), so stored expectations embedding it
         ;; go stale the moment the suite runs from a different directory.
         ;; Squash it to a stable token on BOTH engines, like the tempdirs
         ;; above; the path REMAINDER (the file actually reported) stays a
         ;; real parity lock.  The checkout root itself
         ;; (`NEOVM_ORACLE_PROJECT_ROOT') leaks too -- e.g. `default-directory'
         ;; under nextest is
         ;; <checkout>/crates/neovm-oracle-tests/ -- so squash any remaining
         ;; occurrences of it to a second, coarser token AFTER the
         ;; more-specific lisp/ squash, keeping lisp paths on the finer
         ;; token. Exact-domain replacement avoids masking unrelated paths.
         ;;
         ;; Printed object identities leak a raw address into error messages
         ;; and `prin1' output: GNU prints a real pointer
         ;; (`#<frame F1 0x555555b46708>') while Neomacs prints a synthetic id
         ;; (`#<frame F1 0x100000000>').  Neither is part of the observable
         ;; contract, and the GNU side is not even stable across runs, so the
         ;; address is squashed on BOTH engines while the object TYPE and NAME
         ;; stay a real parity lock.  Scoped to the `#<...>' form so ordinary
         ;; hex content (`%x' output, docstrings quoting code points) is
         ;; untouched.
         ((stringp v)
          (let ((out (copy-sequence v)))
            ;; jit-lock writes `fontified' as lazy redisplay bookkeeping.
            ;; It can differ with byte-compiled artifact state even when the
            ;; returned Org value is semantically identical.  Keep stripping
            ;; opt-in: exact font-lock probes must continue to observe it.
            (when (equal (getenv "NEOVM_ORACLE_RESULT_NORMALIZATION")
                         "ignore-volatile-fontification")
              (remove-text-properties 0 (length out) '(fontified nil) out))
            (replace-regexp-in-string
             "\\(#<[^>]*\\) 0x[0-9a-f]+"
             "\\1 0xADDR"
             (replace-regexp-in-string
              "%b - \\(?:GNU\\|NEO\\) Emacs at "
              "%b - [EMACS-PRODUCT] at "
              (neovm--oracle-squash-roots out)))))
         (t v)))
      (defun neovm--oracle-squash-roots (s)
        (let ((load-root (getenv "NEOVM_ORACLE_LOAD_ROOT"))
              (project-root (getenv "NEOVM_ORACLE_PROJECT_ROOT"))
              (scratch-root (getenv "NEOVM_ORACLE_SCRATCH_ROOT"))
              (session-root (getenv "NEOVM_ORACLE_SESSION_TMPDIR"))
              (home-root (getenv "NEOVM_ORACLE_HOME"))
              (neovm--oracle-case-root (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
              (pairs nil))
          (let ((proj-abs (and project-root
                               (> (length project-root) 0)
                               (directory-file-name project-root)))
                (scratch-abs (and scratch-root
                                  (> (length scratch-root) 0)
                                  (directory-file-name scratch-root)))
                (session-abs (and session-root
                                  (> (length session-root) 0)
                                  (directory-file-name session-root)))
                (home-abs (and home-root
                               (> (length home-root) 0)
                               (directory-file-name home-root)))
                (neovm--oracle-case-abs
                 (and neovm--oracle-case-root
                      (> (length neovm--oracle-case-root) 0)
                      (directory-file-name neovm--oracle-case-root)))
                (load-abs (and load-root
                               (> (length load-root) 0)
                               (directory-file-name load-root))))
              ;; Coarser project-root token LAST so the finer lisp/ token
              ;; wins on lisp paths; each root in both its absolute and its
              ;; abbreviated (~/...) spelling, since `default-directory'
              ;; and friends leak the abbreviated form.
              (when (and proj-abs (> (length proj-abs) 1))
                (push (cons (abbreviate-file-name proj-abs)
                            "[ORACLE-PROJECT-ROOT]")
                      pairs)
                (push (cons proj-abs "[ORACLE-PROJECT-ROOT]") pairs))
              ;; Scratch is inside the project checkout, so normalize it before
              ;; the coarser project root. Preserve the established snapshot
              ;; token used for temporary-file-directory.
              (when (and scratch-abs (> (length scratch-abs) 1))
                (push (cons (abbreviate-file-name scratch-abs)
                            "[SESSION-TMPDIR]")
                      pairs)
                (push (cons scratch-abs "[SESSION-TMPDIR]") pairs))
              ;; The child inherits (or explicitly overrides) TMPDIR. Replace
              ;; that exact directory instead of matching arbitrary /tmp paths.
              (when (and session-abs (> (length session-abs) 1))
                (push (cons (abbreviate-file-name session-abs)
                            "[SESSION-TMPDIR]")
                      pairs)
                (push (cons session-abs "[SESSION-TMPDIR]") pairs))
              ;; A shared case directory is nested inside scratch. Normalize
              ;; it first so snapshots preserve the more-specific token.
              (when (and neovm--oracle-case-abs
                         (> (length neovm--oracle-case-abs) 1))
                (push (cons (abbreviate-file-name neovm--oracle-case-abs)
                            "[ORACLE-TMPDIR]")
                      pairs)
                (push (cons neovm--oracle-case-abs "[ORACLE-TMPDIR]") pairs))
              (when (and load-abs (> (length load-abs) 1))
                (push (cons (abbreviate-file-name load-abs)
                            "[ORACLE-LOAD-ROOT]")
                      pairs)
                (push (cons load-abs "[ORACLE-LOAD-ROOT]") pairs))
              ;; The engine's OWN lisp tree. GNU now runs on its installed
              ;; share/emacs/<version>/lisp (the sibling of data-directory)
              ;; rather than the project tree, and runtime-loaded files
              ;; report that path (locate-library, autoload filenames, lazy
              ;; doc references).  Squash it to the same load-root token as
              ;; the project tree: WHICH tree is environment, while the path
              ;; REMAINDER (emacs-lisp/cl-lib.el, ...) stays the parity lock.
              (let ((own-lisp
                     (and (boundp 'data-directory)
                          (stringp data-directory)
                          (let ((dir (expand-file-name "../lisp" data-directory)))
                            (and (file-directory-p dir) dir)))))
                (when own-lisp
                  (push (cons (abbreviate-file-name own-lisp)
                              "[ORACLE-LOAD-ROOT]")
                        pairs)
                  (push (cons own-lisp "[ORACLE-LOAD-ROOT]") pairs)))
              ;; HOME is nested beneath the random per-case root. Normalize
              ;; only its absolute spelling so tilde syntax remains observable.
              (when (and home-abs (> (length home-abs) 1))
                (push (cons home-abs "[ORACLE-HOME]") pairs))
              (dolist (p pairs s)
                (when (> (length (car p)) 1)
                  (setq s (replace-regexp-in-string
                           (regexp-quote (car p)) (cdr p) s t t)))))))
      (defun neovm--oracle-coalesce-string-properties (s)
        ;; Some probes care about per-character properties but not the
        ;; implementation-specific interval boundaries used to store them.
        ;; Keep this normalization opt-in so unrelated snapshots retain their
        ;; exact printed representation.
        (let* ((out (copy-sequence s))
               (pos 0)
               (len (length out)))
          (while (< pos len)
            (let* ((props (text-properties-at pos out))
                   (end (next-property-change pos out len)))
              (while (and (< end len)
                          (equal props (text-properties-at end out)))
                (setq end (next-property-change end out len)))
              (set-text-properties pos end props out)
              (setq pos end)))
          out))
      (defun neovm--oracle-normalize (v)
        (neovm--oracle-normalize-1 v (make-hash-table :test 'eq)))
    (let* ((coding-system-for-read 'utf-8-unix)
           (coding-system-for-write 'utf-8-unix)
           (_ (set-language-environment "UTF-8"))
           (_ (setq system-time-locale "C"))
           (_ (let ((stable-system-name
                     (getenv "NEOVM_ORACLE_SYSTEM_NAME")))
                (when (and stable-system-name
                           (> (length stable-system-name) 0))
                  (setq system-name stable-system-name))))
           (load-root (getenv "NEOVM_ORACLE_LOAD_ROOT"))
           (load-files (split-string (or (getenv "NEOVM_ORACLE_LOAD_FILES") "") "\n" t))
           (form-file (getenv "NEOVM_ORACLE_FORM_FILE"))
           (result
            (let ((source-buf (generate-new-buffer " *neovm-oracle-form*")))
              (unwind-protect
                  (progn
                    ;; The project lisp/ tree is Neomacs's own runtime; for
                    ;; GNU it is a foreign tree whose .elc are Neo's compiler
                    ;; output (and may not even be generated).  GNU keeps its
                    ;; installed load-path; probes that need a project file
                    ;; pass it in load-files, which loads absolute paths.
                    (when (and load-root
                               (or (equal (getenv "NEOVM_ORACLE_ENGINE") "neomacs")
                                   (equal (getenv "NEOVM_ORACLE_LOAD_ROOT_OWNER")
                                          "probe")))
                      (let ((extra-load-path nil))
                        (dolist (sub '("" "emacs-lisp" "progmodes" "language"
                                       "international" "textmodes" "vc" "leim"
                                       "org"))
                          (let ((dir (if (equal sub "")
                                         load-root
                                       (expand-file-name sub load-root))))
                            (when (file-directory-p dir)
                              (push dir extra-load-path))))
                        (setq load-path (append (nreverse extra-load-path) load-path))))
                    (dolist (file load-files)
                      (load file nil t nil t))
                    (with-current-buffer source-buf
                      (insert-file-contents form-file)
                      (goto-char (point-min)))
                    (let ((last nil))
                      (condition-case nil
                          (while t
                            (setq last (eval (read source-buf) t)))
                        (end-of-file last))))
                (when (buffer-live-p source-buf)
                  (kill-buffer source-buf))))))
      (neovm--oracle-emit-outcome "OK " (neovm--oracle-normalize result))))
  (error
   (neovm--oracle-emit-outcome
    "ERR "
    (neovm--oracle-normalize (cons (car err) (cdr err))))))"#;

const EVAL_PROGRAM_RAW: &str = r#"(condition-case err
    (progn
      (defun neovm--oracle-emit-outcome (kind value)
        (princ "\n" 'external-debugging-output)
        (princ "NEOVM-ORACLE-OUTCOME:" 'external-debugging-output)
        (princ kind 'external-debugging-output)
        (let ((print-escape-newlines t))
          (prin1 value 'external-debugging-output))
        (terpri 'external-debugging-output))
      (let* ((coding-system-for-read 'utf-8-unix)
             (coding-system-for-write 'utf-8-unix)
             (_ (set-language-environment "UTF-8"))
             (_ (setq system-time-locale "C"))
             (_ (let ((stable-system-name
                       (getenv "NEOVM_ORACLE_SYSTEM_NAME")))
                  (when (and stable-system-name
                             (> (length stable-system-name) 0))
                    (setq system-name stable-system-name))))
             (load-root (getenv "NEOVM_ORACLE_LOAD_ROOT"))
             (load-files (split-string (or (getenv "NEOVM_ORACLE_LOAD_FILES") "") "\n" t))
             (form-file (getenv "NEOVM_ORACLE_FORM_FILE"))
             (result
              (let ((source-buf (generate-new-buffer " *neovm-oracle-form*")))
                (unwind-protect
                    (progn
                      ;; Same contract as EVAL_PROGRAM_WITH_NORMALIZER: only
                      ;; the engine that owns the project tree prepends it.
                      (when (and load-root
                                 (or (equal (getenv "NEOVM_ORACLE_ENGINE") "neomacs")
                                     (equal (getenv "NEOVM_ORACLE_LOAD_ROOT_OWNER")
                                            "probe")))
                        (let ((extra-load-path nil))
                          (dolist (sub '("" "emacs-lisp" "progmodes" "language"
                                         "international" "textmodes" "vc" "leim"
                                         "org"))
                            (let ((dir (if (equal sub "")
                                           load-root
                                         (expand-file-name sub load-root))))
                              (when (file-directory-p dir)
                                (push dir extra-load-path))))
                          (setq load-path (append (nreverse extra-load-path) load-path))))
                      (dolist (file load-files)
                        (load file nil t nil t))
                      (with-current-buffer source-buf
                        (insert-file-contents form-file)
                        (goto-char (point-min)))
                      (let ((last nil))
                        (condition-case nil
                            (while t
                              (setq last (eval (read source-buf) t)))
                          (end-of-file last))))
                  (when (buffer-live-p source-buf)
                    (kill-buffer source-buf))))))
        (neovm--oracle-emit-outcome "OK " result)))
  (error
   (neovm--oracle-emit-outcome "ERR " err)))"#;

const NATIVE_COMP_SUPPRESSION_PRELUDE: &str = "(setq native-comp-jit-compilation nil inhibit-automatic-native-compilation t native-comp-enable-subr-trampolines nil)";

// `accept-process-output' is edge-triggered: GNU can return after reading any
// bytes, and observing a terminal process status does not imply that the final
// pipe bytes or default sentinel have been delivered yet.  Process probes use
// this helper when their assertion is about the settled process buffer rather
// than an intermediate event-loop state.
const ORACLE_TEST_SUPPORT_PRELUDE: &str = r#"(progn
  (defun neovm--oracle-settle-process (process &optional attempts)
    (let ((remaining (or attempts 20)))
      (while (and (> remaining 0) (process-live-p process))
        (setq remaining (1- remaining))
        (accept-process-output process 0.05))
      ;; GNU process.c drains an already-terminal process and runs its sentinel
      ;; on this subsequent observation.  This call is deliberately outside
      ;; the live-status loop.
      (accept-process-output process 0)
      (when (process-live-p process)
        (error "Oracle process did not settle after %d attempts" (or attempts 20)))
      (process-status process))))"#;

#[derive(Clone, Copy)]
enum EvalProgram {
    Normalized,
    Raw,
}

impl EvalProgram {
    fn source(self) -> &'static str {
        match self {
            Self::Normalized => EVAL_PROGRAM_WITH_NORMALIZER,
            Self::Raw => EVAL_PROGRAM_RAW,
        }
    }

    fn configure_command(self, command: &mut PinnedCommand) {
        command.args([
            "--batch",
            "-Q",
            "--eval",
            NATIVE_COMP_SUPPRESSION_PRELUDE,
            "--eval",
            ORACLE_TEST_SUPPORT_PRELUDE,
            "--eval",
            self.source(),
        ]);
    }
}

/// Which editor the shared eval program is running inside.
///
/// The load-path setup is engine-specific on purpose.  The project's `lisp/`
/// tree IS Neomacs's runtime, so Neomacs must be able to `(require ...)` the
/// tree under test even when its dump was built in a different checkout (a
/// nextest archive remaps the workspace).  For GNU that tree is FOREIGN:
/// loading it runs Neomacs's byte-compiled `.elc` inside the oracle and makes
/// "GNU" answers depend on Neo's compiler output -- and on generated files the
/// checkout may not even contain.  GNU therefore keeps its installed
/// load-path unless a probe explicitly loads a project file through the
/// `load-files` API, which passes absolute paths to `load`.
#[derive(Clone, Copy)]
enum OracleEngine {
    Gnu,
    Neomacs,
}

impl OracleEngine {
    /// Environment variable carrying [`OracleEngine`] into the eval program.
    const ENV: &'static str = "NEOVM_ORACLE_ENGINE";

    fn as_env_value(self) -> &'static str {
        match self {
            Self::Gnu => "gnu",
            Self::Neomacs => "neomacs",
        }
    }
}

// ---------------------------------------------------------------------------
// Oracle (GNU Emacs) subprocess evaluation
// ---------------------------------------------------------------------------

fn run_oracle_eval_with_sandbox(
    sandbox: &OracleSandbox,
    eval_program: EvalProgram,
) -> Result<CapturedEvaluation, String> {
    // Ledger 214: every GNU invocation in this crate goes through the attested
    // reference, so there is no path that runs an unchecked oracle.
    let oracle = attested_oracle_emacs();
    let oracle_bin = oracle.executable().to_path_buf();

    let mem_limit = oracle_mem_limit_bytes();
    let mut cmd = sandbox.pinned_command(&oracle_bin);
    sandbox.configure(&mut cmd);
    cmd.env(OracleEngine::ENV, OracleEngine::Gnu.as_env_value());
    cmd.env("EMACSNATIVELOADPATH", "/dev/null");
    eval_program.configure_command(&mut cmd);

    apply_virtual_memory_limit(&mut cmd, mem_limit);

    let output = cmd
        .output()
        .map_err(|e| format!("failed to run oracle Emacs: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "oracle Emacs failed: status={}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ));
    }

    CapturedEvaluation::from_marked_streams(
        &String::from_utf8_lossy(&output.stdout),
        &String::from_utf8_lossy(&output.stderr),
        ORACLE_OUTCOME_MARKER,
    )
    .map_err(|error| {
        format!(
            "oracle Emacs emitted an invalid marked outcome: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        )
    })
}

fn run_oracle_eval_inner(form: &str, load_files: &[&str]) -> Result<String, String> {
    let sandbox = OracleSandbox::new(form, load_files, &project_lisp_dir())?;
    run_oracle_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
        .map(|evaluation| evaluation.legacy_transcript())
}

fn run_oracle_eval_inner_raw(form: &str, load_files: &[&str]) -> Result<String, String> {
    let sandbox = OracleSandbox::new(form, load_files, &project_lisp_dir())?;
    run_oracle_eval_with_sandbox(&sandbox, EvalProgram::Raw)
        .map(|evaluation| evaluation.legacy_transcript())
}

pub(crate) fn run_oracle_eval(form: &str) -> Result<String, String> {
    match OracleMode::from_env() {
        OracleMode::Snapshot => run_neomacs_binary_eval_inner(form, &[]),
        OracleMode::Verify | OracleMode::Refresh | OracleMode::Live => {
            run_oracle_eval_inner(form, &[])
        }
    }
}

pub(crate) fn run_oracle_eval_with_load(form: &str, load_files: &[&str]) -> Result<String, String> {
    match OracleMode::from_env() {
        OracleMode::Snapshot => run_neomacs_binary_eval_inner(form, load_files),
        OracleMode::Verify | OracleMode::Refresh | OracleMode::Live => {
            run_oracle_eval_inner(form, load_files)
        }
    }
}

pub(crate) fn run_oracle_eval_with_load_raw(
    form: &str,
    load_files: &[&str],
) -> Result<String, String> {
    match OracleMode::from_env() {
        OracleMode::Snapshot => run_neomacs_binary_eval_inner_raw(form, load_files),
        OracleMode::Verify | OracleMode::Refresh | OracleMode::Live => {
            run_oracle_eval_inner_raw(form, load_files)
        }
    }
}

/// Like `run_oracle_eval_with_load`, but loads files from an external
/// `load_root` (e.g. a third-party package checkout) instead of the project's
/// own `lisp/` tree. Used by the package-corpus oracle tests
/// (e.g. `emacsorphanage_*`) to exercise real-world Elisp against both GNU
/// Emacs and Neomacs from the same checkout.
pub(crate) fn run_oracle_eval_with_load_root(
    form: &str,
    load_files: &[&str],
    load_root: &Path,
) -> Result<String, String> {
    let sandbox = OracleSandbox::new(form, load_files, load_root)?.with_probe_load_root();
    match OracleMode::from_env() {
        OracleMode::Snapshot => {
            run_neomacs_binary_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
                .map(|evaluation| evaluation.legacy_transcript())
        }
        OracleMode::Verify | OracleMode::Refresh | OracleMode::Live => {
            run_oracle_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
                .map(|evaluation| evaluation.legacy_transcript())
        }
    }
}

// ---------------------------------------------------------------------------
// Neomacs binary subprocess evaluation
// ---------------------------------------------------------------------------

fn run_neomacs_binary_eval_with_sandbox(
    sandbox: &OracleSandbox,
    eval_program: EvalProgram,
) -> Result<CapturedEvaluation, String> {
    let neomacs_bin = neomacs_binary_path();

    let mut cmd = sandbox.pinned_command(&neomacs_bin);
    sandbox.configure(&mut cmd);
    cmd.env(OracleEngine::ENV, OracleEngine::Neomacs.as_env_value());
    eval_program.configure_command(&mut cmd);

    if let Some(mem_limit) = neomacs_binary_mem_limit_bytes() {
        apply_virtual_memory_limit(&mut cmd, mem_limit);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("failed to run Neomacs binary: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "Neomacs binary failed: status={}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ));
    }

    CapturedEvaluation::from_marked_streams(
        &String::from_utf8_lossy(&output.stdout),
        &String::from_utf8_lossy(&output.stderr),
        ORACLE_OUTCOME_MARKER,
    )
    .map_err(|error| {
        format!(
            "Neomacs emitted an invalid marked outcome: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        )
    })
}

fn run_neomacs_binary_eval_inner(form: &str, load_files: &[&str]) -> Result<String, String> {
    let sandbox = OracleSandbox::new(form, load_files, &project_lisp_dir())?;
    run_neomacs_binary_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
        .map(|evaluation| evaluation.legacy_transcript())
}

fn run_neomacs_binary_eval_inner_raw(form: &str, load_files: &[&str]) -> Result<String, String> {
    let sandbox = OracleSandbox::new(form, load_files, &project_lisp_dir())?;
    run_neomacs_binary_eval_with_sandbox(&sandbox, EvalProgram::Raw)
        .map(|evaluation| evaluation.legacy_transcript())
}

pub(crate) fn run_neovm_eval(form: &str) -> Result<String, String> {
    run_neomacs_binary_eval_inner(form, &[])
}

pub(crate) fn run_neovm_eval_with_load(form: &str, load_files: &[&str]) -> Result<String, String> {
    run_neomacs_binary_eval_inner(form, load_files)
}

pub(crate) fn run_neovm_eval_with_load_raw(
    form: &str,
    load_files: &[&str],
) -> Result<String, String> {
    run_neomacs_binary_eval_inner_raw(form, load_files)
}

// ---------------------------------------------------------------------------
// Internal parity helper
// ---------------------------------------------------------------------------

fn assert_neovm_oracle_parity(neovm: &CapturedEvaluation, oracle: &CapturedEvaluation, form: &str) {
    if neovm == oracle {
        return;
    }
    let neovm_transcript = neovm.legacy_transcript();
    let oracle_transcript = oracle.legacy_transcript();
    let neo_label = "NEO Emacs:".red().bold().to_string();
    let gnu_label = "GNU Emacs:".green().bold().to_string();
    panic!(
        "oracle parity mismatch for form: {form}\n  {neo_label}  {neovm_transcript}\n  {gnu_label}  {oracle_transcript}\n  NEO debug: {neovm:?}\n  GNU debug: {oracle:?}",
    );
}

// Store inline oracle values in a Rust-debug representation. This keeps exact
// newlines, tabs, quotes, and trailing spaces testable without putting literal
// trailing whitespace or conflict-marker-looking lines in source files.
fn inline_expect_payload(evaluation: &CapturedEvaluation) -> String {
    let value = evaluation.legacy_transcript();
    let source_safe = value.replace('\0', "\\0").replace('\r', "\\r");
    format!("{source_safe:?}")
}

// ---------------------------------------------------------------------------
// Public parity assertions
// ---------------------------------------------------------------------------

pub(crate) fn assert_oracle_parity(form: &str) {
    let t0 = std::time::Instant::now();
    let log_timing = oracle_timing_enabled();

    ensure_nonempty_form(form).expect("form should not be empty");
    let sandbox = oracle_sandbox(form, &[], &project_lisp_dir());

    if log_timing {
        eprintln!("oracle-timing: neomacs-binary-start");
    }
    let neomacs_t0 = std::time::Instant::now();
    let neovm = run_neomacs_binary_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
        .expect("neomacs binary eval should run");
    if OracleMode::from_env() == OracleMode::Snapshot {
        if log_timing {
            eprintln!(
                "oracle-timing: neomacs-binary-done {:.3?}",
                neomacs_t0.elapsed()
            );
        }
        return;
    }
    if log_timing {
        eprintln!(
            "oracle-timing: neomacs-binary-done {:.3?}",
            neomacs_t0.elapsed()
        );
        eprintln!("oracle-timing: oracle-start");
    }
    let oracle_t0 = std::time::Instant::now();
    let oracle = run_oracle_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
        .expect("oracle eval should run");
    if log_timing {
        eprintln!("oracle-timing: oracle-done {:.3?}", oracle_t0.elapsed());
    }
    eprintln!("total: {:.3?}", t0.elapsed());
    assert_neovm_oracle_parity(&neovm, &oracle, form);
}

fn assert_oracle_parity_expect_with_sandbox(
    form: &str,
    expected: expect_test::Expect,
    sandbox: &OracleSandbox,
    eval_program: EvalProgram,
) {
    ensure_nonempty_form(form).expect("form should not be empty");

    match OracleMode::from_env() {
        OracleMode::Snapshot => {
            let neovm = run_neomacs_binary_eval_with_sandbox(sandbox, eval_program)
                .expect("neomacs binary eval should run");
            expected.assert_eq(&inline_expect_payload(&neovm));
        }
        OracleMode::Verify => {
            let oracle = run_oracle_eval_with_sandbox(sandbox, eval_program)
                .expect("oracle eval should run");
            let neovm = run_neomacs_binary_eval_with_sandbox(sandbox, eval_program)
                .expect("neomacs binary eval should run");
            expected.assert_eq(&inline_expect_payload(&oracle));
            assert_neovm_oracle_parity(&neovm, &oracle, form);
        }
        OracleMode::Refresh => {
            let oracle = run_oracle_eval_with_sandbox(sandbox, eval_program)
                .expect("oracle eval should run");
            expected.assert_eq(&inline_expect_payload(&oracle));
        }
        OracleMode::Live => {
            let oracle = run_oracle_eval_with_sandbox(sandbox, eval_program)
                .expect("oracle eval should run");
            let neovm = run_neomacs_binary_eval_with_sandbox(sandbox, eval_program)
                .expect("neomacs binary eval should run");
            assert_neovm_oracle_parity(&neovm, &oracle, form);
        }
    }
}

pub(crate) fn assert_oracle_parity_expect(form: &str, expected: expect_test::Expect) {
    let sandbox = oracle_sandbox(form, &[], &project_lisp_dir());
    assert_oracle_parity_expect_with_sandbox(form, expected, &sandbox, EvalProgram::Normalized);
}

/// Pin a KNOWN divergence as an expected failure: `expected` is GNU's answer
/// (written by `NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1`, never by hand), and
/// the test passes while Neomacs still answers something else.
///
/// Once Neomacs agrees with GNU the pin fails on purpose, naming the fix: the
/// commit that fixes the divergence flips the test to
/// [`assert_oracle_parity_expect`] with the same expectation.
pub(crate) fn assert_oracle_divergence_expect(form: &str, expected: expect_test::Expect) {
    ensure_nonempty_form(form).expect("form should not be empty");
    let sandbox = oracle_sandbox(form, &[], &project_lisp_dir());
    let eval_program = EvalProgram::Normalized;
    let assert_still_diverges = |neovm: &CapturedEvaluation, gnu_payload: &str| {
        let neovm_payload = inline_expect_payload(neovm);
        assert_ne!(
            neovm_payload, gnu_payload,
            "the pinned divergence is FIXED: Neomacs now answers like GNU for {form}; \
             flip this pin to assert_oracle_parity_expect"
        );
        tracing::info!(neovm = %neovm_payload, gnu = %gnu_payload, "pinned divergence holds");
    };
    match OracleMode::from_env() {
        OracleMode::Snapshot => {
            let neovm = run_neomacs_binary_eval_with_sandbox(&sandbox, eval_program)
                .expect("neomacs binary eval should run");
            assert_still_diverges(&neovm, expected.data().trim());
        }
        OracleMode::Verify | OracleMode::Live => {
            let oracle = run_oracle_eval_with_sandbox(&sandbox, eval_program)
                .expect("oracle eval should run");
            let neovm = run_neomacs_binary_eval_with_sandbox(&sandbox, eval_program)
                .expect("neomacs binary eval should run");
            let gnu_payload = inline_expect_payload(&oracle);
            if OracleMode::from_env() == OracleMode::Verify {
                expected.assert_eq(&gnu_payload);
            }
            assert_still_diverges(&neovm, &gnu_payload);
        }
        OracleMode::Refresh => {
            let oracle = run_oracle_eval_with_sandbox(&sandbox, eval_program)
                .expect("oracle eval should run");
            expected.assert_eq(&inline_expect_payload(&oracle));
        }
    }
}

/// Pin both sides of a deliberate deviation. GNU's expectation is refreshed
/// only from the pinned oracle; Neomacs must keep its documented exact answer
/// under EXTRA_ENV, so an unrelated regression cannot satisfy a divergence pin.
pub(crate) fn assert_oracle_deviation_with_env_expect(
    form: &str,
    extra_env: &[(&str, &str)],
    gnu_expected: expect_test::Expect,
    neomacs_expected: &str,
) {
    ensure_nonempty_form(form).expect("form should not be empty");
    let sandbox = oracle_sandbox(form, &[], &project_lisp_dir());
    let mode = OracleMode::from_env();
    let oracle = match mode {
        OracleMode::Snapshot => None,
        _ => Some(
            run_oracle_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
                .expect("oracle eval should run"),
        ),
    };
    if let (OracleMode::Verify | OracleMode::Refresh, Some(oracle)) = (mode, &oracle) {
        gnu_expected.assert_eq(&inline_expect_payload(oracle));
    }
    if mode == OracleMode::Refresh {
        return;
    }
    let neomacs = run_neomacs_binary_eval_with_sandbox(
        &sandbox.with_extra_env(extra_env),
        EvalProgram::Normalized,
    )
    .expect("neomacs binary eval should run");
    let neomacs = inline_expect_payload(&neomacs);
    assert_eq!(
        neomacs, neomacs_expected,
        "documented Neomacs behaviour changed"
    );
    let gnu = oracle
        .as_ref()
        .map(inline_expect_payload)
        .unwrap_or_else(|| gnu_expected.data().trim().to_owned());
    assert_ne!(
        neomacs, gnu,
        "the pinned deviation now agrees with GNU; replace it with an oracle parity test"
    );
}

/// Assert an Org-style workflow result while ignoring only jit-lock's
/// volatile `fontified' string property.  Other text properties remain exact.
pub(crate) fn assert_oracle_parity_ignoring_volatile_fontification_expect(
    form: &str,
    expected: expect_test::Expect,
) {
    let sandbox = oracle_sandbox(form, &[], &project_lisp_dir())
        .with_result_normalization(ResultNormalization::IgnoreVolatileFontification);
    assert_oracle_parity_expect_with_sandbox(form, expected, &sandbox, EvalProgram::Normalized);
}

pub(crate) fn assert_oracle_parity_with_shared_tempdir_expect(
    form: &str,
    expected: expect_test::Expect,
) {
    let sandbox = oracle_sandbox(form, &[], &project_lisp_dir()).expose_case_root_as_test_tmpdir();
    assert_oracle_parity_expect_with_sandbox(form, expected, &sandbox, EvalProgram::Normalized);
}

pub(crate) fn assert_oracle_parity_with_case_workdir_expect(
    form: &str,
    expected: expect_test::Expect,
) {
    let sandbox =
        oracle_sandbox(form, &[], &project_lisp_dir()).with_case_working_directory_and_tmpdir();
    assert_oracle_parity_expect_with_sandbox(form, expected, &sandbox, EvalProgram::Normalized);
}

/// Like [`assert_oracle_parity_expect`], with the Neomacs side run once per
/// entry of `neomacs_envs` against the one GNU expectation: a knob matrix (a
/// JIT threshold, a fast tier switched off) must not move any answer.  GNU
/// runs once; the environments are Neomacs knobs it does not read.
pub(crate) fn assert_oracle_parity_under_envs_expect(
    form: &str,
    neomacs_envs: &[&[(&str, &str)]],
    expected: expect_test::Expect,
) {
    ensure_nonempty_form(form).expect("form should not be empty");
    let eval_program = EvalProgram::Normalized;
    let sandbox_with = |extra_env: &[(&str, &str)]| {
        oracle_sandbox(form, &[], &project_lisp_dir()).with_extra_env(extra_env)
    };
    let mode = OracleMode::from_env();
    let oracle = match mode {
        OracleMode::Snapshot => None,
        OracleMode::Verify | OracleMode::Refresh | OracleMode::Live => Some(
            run_oracle_eval_with_sandbox(&sandbox_with(&[]), eval_program)
                .expect("oracle eval should run"),
        ),
    };
    if let (OracleMode::Verify | OracleMode::Refresh, Some(oracle)) = (mode, &oracle) {
        expected.assert_eq(&inline_expect_payload(oracle));
    }
    if mode == OracleMode::Refresh {
        return;
    }
    for extra_env in neomacs_envs {
        let neovm = run_neomacs_binary_eval_with_sandbox(&sandbox_with(extra_env), eval_program)
            .unwrap_or_else(|e| panic!("neomacs binary eval should run under {extra_env:?}: {e}"));
        match &oracle {
            None => expected.assert_eq(&inline_expect_payload(&neovm)),
            Some(oracle) => assert_neovm_oracle_parity(&neovm, oracle, form),
        }
    }
}

/// Whether the AOT battery runs: `NEOVM_ORACLE_AOT=1`, set when the Neomacs
/// binary under test (`NEOVM_BINARY_PATH`) was built with its dump-time
/// preload (`cargo xtask fresh-build --aot-preload`). Without a preload there
/// is nothing AOT could serve, so the battery would pass vacuously; it stays
/// off unless asked for, and once asked for it refuses to be vacuous.
pub(crate) fn aot_battery_enabled() -> bool {
    matches!(
        std::env::var("NEOVM_ORACLE_AOT").as_deref(),
        Ok("1" | "on" | "yes")
    )
}

/// The knob matrix the AOT battery runs Neomacs under (P4.2 A5/A7): AOT
/// alone, immediate tier-up, forced deopt, and the low-threshold cross
/// product of AOT re-tier and the T2 spine. These arms pin both switches
/// so an outer test environment cannot silently remove a matrix case.
/// The T2 window is shorter than the battery's warm-up; synchronous
/// compilation keeps the served tier independent of worker scheduling.
const AOT_BATTERY_ENVS: &[&[(&str, &str)]] = &[
    &[
        ("NEOVM_AOT", "1"),
        ("NEOVM_AOT_RETIER", "off"),
        ("NEOVM_JIT_TIER2", "off"),
    ],
    &[
        ("NEOVM_AOT", "1"),
        ("NEOVM_AOT_RETIER", "off"),
        ("NEOVM_JIT_TIER2", "off"),
        ("NEOVM_JIT_THRESHOLD", "1"),
    ],
    &[
        ("NEOVM_AOT", "1"),
        ("NEOVM_AOT_RETIER", "off"),
        ("NEOVM_JIT_TIER2", "off"),
        ("NEOVM_JIT_FORCE_DEOPT", "1"),
    ],
    &[
        ("NEOVM_AOT", "1"),
        ("NEOVM_AOT_RETIER", "off"),
        ("NEOVM_JIT_TIER2", "on"),
        ("NEOVM_JIT_THRESHOLD", "1"),
        ("NEOVM_JIT_T2_WINDOW", "16"),
        ("NEOVM_JIT_T2_STABLE", "1"),
        ("NEOVM_JIT_T2_BUDGET", "0"),
        ("NEOVM_JIT_BG", "sync"),
    ],
    &[
        ("NEOVM_AOT", "1"),
        ("NEOVM_AOT_RETIER", "on"),
        ("NEOVM_JIT_TIER2", "off"),
        ("NEOVM_JIT_THRESHOLD", "1"),
        ("NEOVM_JIT_BG", "sync"),
    ],
    &[
        ("NEOVM_AOT", "1"),
        ("NEOVM_AOT_RETIER", "on"),
        ("NEOVM_JIT_TIER2", "on"),
        ("NEOVM_JIT_THRESHOLD", "1"),
        ("NEOVM_JIT_T2_WINDOW", "16"),
        ("NEOVM_JIT_T2_STABLE", "1"),
        ("NEOVM_JIT_T2_BUDGET", "0"),
        ("NEOVM_JIT_BG", "sync"),
    ],
];

/// `aot_loads=N` from the last `[neovm-jit-final]` line of a JIT stats file.
fn final_aot_loads(stats: &str) -> Option<u64> {
    stats
        .lines()
        .rev()
        .find(|line| line.starts_with("[neovm-jit-final]"))?
        .split_whitespace()
        .find_map(|field| field.strip_prefix("aot_loads="))?
        .parse()
        .ok()
}

/// [`assert_oracle_parity_under_envs_expect`] over [`AOT_BATTERY_ENVS`], which
/// also requires that AOT really served: each Neomacs run reports its JIT
/// statistics (`NEOVM_JIT_STATS_FILE`) and must show `aot_loads > 0`. The AOT
/// batteries once passed with no AOT leaf ever served (the first-call consult
/// never reached the preload, P4.2 B2); this makes that a failure.
pub(crate) fn assert_oracle_parity_under_aot_expect(form: &str, expected: expect_test::Expect) {
    ensure_nonempty_form(form).expect("form should not be empty");
    oracle_sandbox::validate_form_source(form).expect("oracle form source should read completely");
    let eval_program = EvalProgram::Normalized;
    let mode = OracleMode::from_env();
    // Refreshing the GNU expectation needs no Neomacs binary.
    if mode != OracleMode::Refresh {
        if !aot_battery_enabled() {
            tracing::info!(
                "skipping AOT battery form: set NEOVM_ORACLE_AOT=1 (needs a preload build)"
            );
            return;
        }
        let binary = PathBuf::from(neomacs_binary_path());
        let preload = binary.with_file_name("libneomacs-preload.so");
        assert!(
            preload.exists(),
            "NEOVM_ORACLE_AOT=1 but {} has no preload beside it ({}): build it with \
             `cargo xtask fresh-build --aot-preload`",
            binary.display(),
            preload.display()
        );
    }
    let oracle = match mode {
        OracleMode::Snapshot => None,
        OracleMode::Verify | OracleMode::Refresh | OracleMode::Live => Some(
            run_oracle_eval_with_sandbox(
                &oracle_sandbox(form, &[], &project_lisp_dir()),
                eval_program,
            )
            .expect("oracle eval should run"),
        ),
    };
    if let (OracleMode::Verify | OracleMode::Refresh, Some(oracle)) = (mode, &oracle) {
        expected.assert_eq(&inline_expect_payload(oracle));
    }
    if mode == OracleMode::Refresh {
        return;
    }
    for knobs in AOT_BATTERY_ENVS {
        let stats_dir = tempfile::tempdir().expect("stats tempdir");
        let stats_path = stats_dir.path().join("jit-stats.txt");
        let stats_file = stats_path.to_string_lossy().into_owned();
        let mut env: Vec<(&str, &str)> = knobs.to_vec();
        env.push(("NEOVM_JIT_STATS_FILE", stats_file.as_str()));
        let sandbox = oracle_sandbox(form, &[], &project_lisp_dir()).with_extra_env(&env);
        let neovm = run_neomacs_binary_eval_with_sandbox(&sandbox, eval_program)
            .unwrap_or_else(|e| panic!("neomacs binary eval should run under {knobs:?}: {e}"));
        match &oracle {
            None => expected.assert_eq(&inline_expect_payload(&neovm)),
            Some(oracle) => assert_neovm_oracle_parity(&neovm, oracle, form),
        }
        let stats = std::fs::read_to_string(&stats_path).unwrap_or_default();
        let aot_loads = final_aot_loads(&stats);
        assert!(
            aot_loads.is_some_and(|n| n > 0),
            "AOT served nothing under {knobs:?} (aot_loads={aot_loads:?}): the battery \
             would be vacuous. JIT stats:\n{stats}"
        );
    }
}

pub(crate) fn assert_oracle_parity_with_env_expect(
    form: &str,
    extra_env: &[(&str, &str)],
    expected: expect_test::Expect,
) {
    let sandbox = oracle_sandbox(form, &[], &project_lisp_dir()).with_extra_env(extra_env);
    assert_oracle_parity_expect_with_sandbox(form, expected, &sandbox, EvalProgram::Normalized);
}

/// Parity of a form together with what the engine wrote to a terminal while
/// evaluating it.  Each engine run gets its own terminal from `open`, which
/// also names the env the form finds it under; `observe` reduces that run's
/// byte stream to the fact under test, which follows the Lisp result in the
/// expectation.  The byte stream is a redisplay observation like any other
/// (GNU's term.c decides it), so it is refreshed from GNU and never written
/// by hand.
pub(crate) fn assert_oracle_parity_with_terminal_output_expect<T>(
    form: &str,
    open: impl Fn() -> (T, Vec<(String, String)>),
    observe: impl Fn(T) -> String,
    expected: expect_test::Expect,
) {
    ensure_nonempty_form(form).expect("form should not be empty");
    let eval_program = EvalProgram::Normalized;
    let run = |engine: &dyn Fn(&OracleSandbox) -> Result<CapturedEvaluation, String>| {
        let (terminal, env) = open();
        let env: Vec<(&str, &str)> = env
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let sandbox = oracle_sandbox(form, &[], &project_lisp_dir()).with_extra_env(&env);
        let evaluation = engine(&sandbox).expect("engine eval should run");
        format!(
            "{} {}",
            inline_expect_payload(&evaluation),
            observe(terminal)
        )
    };
    let gnu = |sandbox: &OracleSandbox| run_oracle_eval_with_sandbox(sandbox, eval_program);
    let neo = |sandbox: &OracleSandbox| run_neomacs_binary_eval_with_sandbox(sandbox, eval_program);
    let assert_parity = |neovm: &str, oracle: &str| {
        assert_eq!(
            neovm, oracle,
            "oracle parity mismatch (result and terminal output) for form: {form}"
        );
    };
    match OracleMode::from_env() {
        OracleMode::Snapshot => expected.assert_eq(&run(&neo)),
        OracleMode::Verify => {
            let oracle = run(&gnu);
            let neovm = run(&neo);
            expected.assert_eq(&oracle);
            assert_parity(&neovm, &oracle);
        }
        OracleMode::Refresh => expected.assert_eq(&run(&gnu)),
        OracleMode::Live => assert_parity(&run(&neo), &run(&gnu)),
    }
}

/// [`assert_oracle_divergence_expect`] for an observation that includes the
/// terminal output, as in [`assert_oracle_parity_with_terminal_output_expect`]:
/// `expected` is GNU's answer (refreshed, never written by hand) and the pin
/// passes while Neomacs still answers something else.  Once Neomacs agrees,
/// the pin fails on purpose; the commit that closes the divergence flips it
/// to the parity assertion with the same expectation.
pub(crate) fn assert_oracle_divergence_with_terminal_output_expect<T>(
    form: &str,
    open: impl Fn() -> (T, Vec<(String, String)>),
    observe: impl Fn(T) -> String,
    expected: expect_test::Expect,
) {
    ensure_nonempty_form(form).expect("form should not be empty");
    let eval_program = EvalProgram::Normalized;
    let run = |engine: &dyn Fn(&OracleSandbox) -> Result<CapturedEvaluation, String>| {
        let (terminal, env) = open();
        let env: Vec<(&str, &str)> = env
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let sandbox = oracle_sandbox(form, &[], &project_lisp_dir()).with_extra_env(&env);
        let evaluation = engine(&sandbox).expect("engine eval should run");
        format!(
            "{} {}",
            inline_expect_payload(&evaluation),
            observe(terminal)
        )
    };
    let gnu = |sandbox: &OracleSandbox| run_oracle_eval_with_sandbox(sandbox, eval_program);
    let neo = |sandbox: &OracleSandbox| run_neomacs_binary_eval_with_sandbox(sandbox, eval_program);
    let assert_still_diverges = |neovm: &str, gnu: &str| {
        assert_ne!(
            neovm, gnu,
            "the pinned divergence is FIXED: Neomacs now answers like GNU for {form}; \
             flip this pin to assert_oracle_parity_with_terminal_output_expect"
        );
        tracing::info!(neovm = %neovm, gnu = %gnu, "pinned divergence holds");
    };
    match OracleMode::from_env() {
        OracleMode::Snapshot => assert_still_diverges(&run(&neo), expected.data().trim()),
        OracleMode::Verify | OracleMode::Live => {
            let oracle = run(&gnu);
            let neovm = run(&neo);
            if OracleMode::from_env() == OracleMode::Verify {
                expected.assert_eq(&oracle);
            }
            assert_still_diverges(&neovm, &oracle);
        }
        OracleMode::Refresh => expected.assert_eq(&run(&gnu)),
    }
}

/// Like [`assert_oracle_parity_expect`], but runs both engines under a frozen
/// wall clock ([`ORACLE_FROZEN_TIME`]) via libfaketime. Use this for forms
/// whose output embeds the run-time date/time (org-agenda "today", `%t`/`%U`
/// captures, clock/export/archive stamps) so the checked-in expectation is
/// stable regardless of when -- or across what midnight boundary -- the suite
/// runs. Because both processes see the identical instant, no date/time output
/// normalization is needed. A shared per-case tempdir is provided so file paths
/// stay normalizable alongside the frozen clock.
pub(crate) fn assert_oracle_parity_frozen_time_expect(form: &str, expected: expect_test::Expect) {
    assert_oracle_parity_frozen_time_with_load_expect(form, &[], expected);
}

fn frozen_time_oracle_sandbox(
    form: &str,
    load_files: &[&str],
    result_normalization: ResultNormalization,
) -> OracleSandbox {
    let env_owned = frozen_time_env();
    let extra_env: Vec<(&str, &str)> = env_owned
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    oracle_sandbox(form, load_files, &project_lisp_dir())
        .expose_case_root_as_test_tmpdir()
        .with_extra_env(&extra_env)
        .with_result_normalization(result_normalization)
}

/// Frozen-clock counterpart of
/// [`assert_oracle_parity_ignoring_volatile_fontification_expect`].
pub(crate) fn assert_oracle_parity_frozen_time_ignoring_volatile_fontification_expect(
    form: &str,
    expected: expect_test::Expect,
) {
    let sandbox =
        frozen_time_oracle_sandbox(form, &[], ResultNormalization::IgnoreVolatileFontification);
    assert_oracle_parity_expect_with_sandbox(form, expected, &sandbox, EvalProgram::Normalized);
}

/// [`assert_oracle_parity_frozen_time_expect`] with extra `load_files` loaded
/// into both engines first -- the frozen-clock counterpart of
/// [`assert_oracle_parity_with_load_expect`].
pub(crate) fn assert_oracle_parity_frozen_time_with_load_expect(
    form: &str,
    load_files: &[&str],
    expected: expect_test::Expect,
) {
    let sandbox = frozen_time_oracle_sandbox(form, load_files, ResultNormalization::Exact);
    assert_oracle_parity_expect_with_sandbox(form, expected, &sandbox, EvalProgram::Normalized);
}

pub(crate) fn assert_oracle_parity_with_load_expect(
    form: &str,
    load_files: &[&str],
    expected: expect_test::Expect,
) {
    let sandbox = oracle_sandbox(form, load_files, &project_lisp_dir());
    assert_oracle_parity_expect_with_sandbox(form, expected, &sandbox, EvalProgram::Normalized);
}

pub(crate) fn assert_oracle_parity_with_load_raw_expect(
    form: &str,
    load_files: &[&str],
    expected: expect_test::Expect,
) {
    let sandbox = oracle_sandbox(form, load_files, &project_lisp_dir());
    assert_oracle_parity_expect_with_sandbox(form, expected, &sandbox, EvalProgram::Raw);
}

pub(crate) fn assert_oracle_parity_with_shared_tempdir(form: &str) {
    ensure_nonempty_form(form).expect("form should not be empty");
    let sandbox = oracle_sandbox(form, &[], &project_lisp_dir()).expose_case_root_as_test_tmpdir();
    let neovm = run_neomacs_binary_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
        .expect("neomacs binary eval should run");
    if OracleMode::from_env() == OracleMode::Snapshot {
        return;
    }
    let oracle = run_oracle_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
        .expect("oracle eval should run");
    assert_neovm_oracle_parity(&neovm, &oracle, form);
}

pub(crate) fn assert_oracle_parity_with_env(form: &str, extra_env: &[(&str, &str)]) {
    ensure_nonempty_form(form).expect("form should not be empty");
    let sandbox = oracle_sandbox(form, &[], &project_lisp_dir()).with_extra_env(extra_env);
    let neovm = run_neomacs_binary_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
        .expect("neomacs binary eval should run");
    if OracleMode::from_env() == OracleMode::Snapshot {
        return;
    }
    let oracle = run_oracle_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
        .expect("oracle eval should run");
    assert_neovm_oracle_parity(&neovm, &oracle, form);
}

pub(crate) fn assert_oracle_parity_with_load(form: &str, load_files: &[&str]) {
    let sandbox = oracle_sandbox(form, load_files, &project_lisp_dir());
    let neovm = run_neomacs_binary_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
        .expect("neomacs binary eval should run");
    if OracleMode::from_env() == OracleMode::Snapshot {
        return;
    }
    let oracle = run_oracle_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
        .expect("oracle eval should run");
    assert_neovm_oracle_parity(&neovm, &oracle, form);
}

pub(crate) fn assert_oracle_parity_with_load_raw(form: &str, load_files: &[&str]) {
    let sandbox = oracle_sandbox(form, load_files, &project_lisp_dir());
    let neovm = run_neomacs_binary_eval_with_sandbox(&sandbox, EvalProgram::Raw)
        .expect("neomacs binary eval should run");
    if OracleMode::from_env() == OracleMode::Snapshot {
        return;
    }
    let oracle =
        run_oracle_eval_with_sandbox(&sandbox, EvalProgram::Raw).expect("oracle eval should run");
    assert_neovm_oracle_parity(&neovm, &oracle, form);
}

/// Snapshot/parity assertion that loads third-party files from an external
/// `load_root` (a package checkout) rather than the project `lisp/` tree.
/// In Snapshot mode only Neomacs runs against the inline expectation; in
/// Verify/Refresh/Live the GNU oracle is driven from the same checkout.
pub(crate) fn assert_oracle_parity_with_load_root_expect(
    form: &str,
    load_files: &[&str],
    load_root: &Path,
    expected: expect_test::Expect,
) {
    let sandbox = oracle_sandbox(form, load_files, load_root).with_probe_load_root();
    assert_oracle_parity_expect_with_sandbox(form, expected, &sandbox, EvalProgram::Normalized);
}

/// Non-snapshot variant of `assert_oracle_parity_with_load_root_expect` for
/// cases where no inline GNU expectation is kept (pure live parity).
pub(crate) fn assert_oracle_parity_with_load_root(
    form: &str,
    load_files: &[&str],
    load_root: &Path,
) {
    let sandbox = oracle_sandbox(form, load_files, load_root).with_probe_load_root();
    let neovm = run_neomacs_binary_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
        .expect("neomacs binary eval should run");
    if OracleMode::from_env() == OracleMode::Snapshot {
        return;
    }
    let oracle = run_oracle_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
        .expect("oracle eval should run");
    assert_neovm_oracle_parity(&neovm, &oracle, form);
}

pub(crate) fn try_eval_oracle_and_neovm(form: &str) -> Result<(String, String), String> {
    ensure_nonempty_form(form)?;
    let sandbox = OracleSandbox::new(form, &[], &project_lisp_dir())
        .map_err(|error| format!("failed to create oracle sandbox: {error}"))?;
    if OracleMode::from_env() == OracleMode::Snapshot {
        let neovm = run_neomacs_binary_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
            .map_err(|error| format!("neomacs binary eval failed: {error}"))?;
        let transcript = neovm.legacy_transcript();
        return Ok((transcript.clone(), transcript));
    }
    let oracle = run_oracle_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
        .map_err(|error| format!("oracle eval failed: {error}"))?;
    let neovm = run_neomacs_binary_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
        .map_err(|error| format!("neomacs binary eval failed: {error}"))?;
    Ok((oracle.legacy_transcript(), neovm.legacy_transcript()))
}

pub(crate) fn eval_oracle_and_neovm(form: &str) -> (String, String) {
    try_eval_oracle_and_neovm(form).expect("oracle and neomacs evals should run")
}

pub(crate) fn eval_oracle_and_neovm_expect(
    form: &str,
    expected: expect_test::Expect,
) -> (String, String) {
    ensure_nonempty_form(form).expect("form should not be empty");
    let sandbox = oracle_sandbox(form, &[], &project_lisp_dir());

    match OracleMode::from_env() {
        OracleMode::Snapshot => {
            let neovm = run_neomacs_binary_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
                .expect("neomacs binary eval should run");
            expected.assert_eq(&inline_expect_payload(&neovm));
            let transcript = neovm.legacy_transcript();
            (transcript.clone(), transcript)
        }
        OracleMode::Verify => {
            let oracle = run_oracle_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
                .expect("oracle eval should run");
            let neovm = run_neomacs_binary_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
                .expect("neomacs binary eval should run");
            expected.assert_eq(&inline_expect_payload(&oracle));
            assert_neovm_oracle_parity(&neovm, &oracle, form);
            (oracle.legacy_transcript(), neovm.legacy_transcript())
        }
        OracleMode::Refresh => {
            let oracle = run_oracle_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
                .expect("oracle eval should run");
            expected.assert_eq(&inline_expect_payload(&oracle));
            let transcript = oracle.legacy_transcript();
            (transcript.clone(), transcript)
        }
        OracleMode::Live => {
            let oracle = run_oracle_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
                .expect("oracle eval should run");
            let neovm = run_neomacs_binary_eval_with_sandbox(&sandbox, EvalProgram::Normalized)
                .expect("neomacs binary eval should run");
            (oracle.legacy_transcript(), neovm.legacy_transcript())
        }
    }
}

pub(crate) fn assert_ok_eq(expected_payload: &str, oracle: &str, neovm: &str) {
    let expected = format!("OK {expected_payload}");
    assert_eq!(oracle, expected, "GNU Emacs should match expected payload");
    assert_eq!(neovm, expected, "Neomacs should match expected payload");
}

pub(crate) fn assert_err_kind(oracle: &str, neovm: &str, err_kind: &str) {
    assert!(
        oracle.starts_with("ERR "),
        "oracle should return an error: {oracle}"
    );
    assert!(
        neovm.starts_with("ERR "),
        "neovm should return an error: {neovm}"
    );

    let oracle_payload = oracle
        .strip_prefix("ERR ")
        .expect("oracle payload should have ERR prefix")
        .trim();
    let neovm_payload = neovm
        .strip_prefix("ERR ")
        .expect("neovm payload should have ERR prefix")
        .trim();

    assert!(
        !oracle_payload.is_empty(),
        "oracle error should include a message"
    );
    assert!(
        !neovm_payload.is_empty(),
        "neovm error should include a message"
    );
    assert!(
        oracle_payload.contains(err_kind),
        "oracle error kind should contain '{err_kind}': {oracle_payload}"
    );
    assert!(
        neovm_payload.contains(err_kind),
        "neovm error kind should contain '{err_kind}': {neovm_payload}"
    );
}
