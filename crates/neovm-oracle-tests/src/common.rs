//! Shared oracle helpers for Elisp unit tests.
//!
//! These helpers are intentionally test-only. The default snapshot mode only
//! requires a Neomacs release binary at `target/release/neomacs` (or
//! `NEOVM_BINARY_PATH`). Live oracle modes also require GNU Emacs on PATH (or
//! via `NEOVM_FORCE_ORACLE_PATH`).

use colored::Colorize;
use neomacs_parity_reference::{AttestationError, ReferenceUse};
use neomacs_test_oracle::CapturedEvaluation;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

#[path = "oracle_sandbox.rs"]
pub(crate) mod oracle_sandbox;

use oracle_sandbox::{OracleSandbox, ResultNormalization};

#[cfg(unix)]
fn apply_virtual_memory_limit(cmd: &mut Command, mem_limit: u64) {
    unsafe {
        cmd.pre_exec(move || {
            let rlim = libc::rlimit {
                rlim_cur: mem_limit as libc::rlim_t,
                rlim_max: mem_limit as libc::rlim_t,
            };
            if libc::setrlimit(libc::RLIMIT_AS, &rlim) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn apply_virtual_memory_limit(_cmd: &mut Command, _mem_limit: u64) {}

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

pub(crate) fn oracle_prop_enabled() -> bool {
    OracleMode::from_env() == OracleMode::Snapshot || oracle_emacs_available()
}

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

/// Where the GNU oracle was asked for, and whether someone asked by name.
///
/// The distinction matters at exactly one point: an oracle that cannot be
/// resolved.  If GNU is simply absent from `PATH`, skipping is right --- that is
/// what lets the snapshot suite run on a machine without the mirror.  If an
/// operator NAMED a binary in `NEOVM_FORCE_ORACLE_PATH`, silently skipping
/// every live test is a green run that measured nothing, and ledger 210's
/// lesson is to check the count rather than the colour.  Found while
/// sensitivity-checking this file's own guard: a relative path in that variable
/// resolves against nextest's working directory, not the shell's, so a typo
/// produced 38826 skips and an `ok`.
struct OracleRequest {
    path: String,
    named_explicitly: bool,
}

fn oracle_emacs_request() -> OracleRequest {
    match std::env::var("NEOVM_FORCE_ORACLE_PATH") {
        Ok(path) => OracleRequest {
            path,
            named_explicitly: true,
        },
        Err(_) => OracleRequest {
            path: "emacs".to_string(),
            named_explicitly: false,
        },
    }
}

fn oracle_emacs_path() -> String {
    oracle_emacs_request().path
}

/// The attested GNU this process may run, resolved once.
///
/// # Why this replaced a presence check (ledger 214)
///
/// This used to be `oracle_emacs_available`, which spawned `emacs --version`
/// and looked only at the exit status --- it never even read the output.  That
/// answers "can an emacs be run", not "is it OUR emacs", and the difference is
/// not academic in every mode:
///
/// * `Snapshot` never runs GNU at all, so a changed reference cannot move the
///   score.  Nothing below is reached, and the cost stays zero.
/// * `Verify` and `Live` run GNU and compare, so a changed reference changes
///   the verdict.  `Verify` would at least go red --- but red as if the PORT
///   regressed, which is the wrong diagnosis and the expensive one to chase.
/// * `Refresh` with `UPDATE_EXPECT=1` runs GNU and **rewrites the inline
///   expectations from its answers**.  A changed reference there silently
///   re-baselines the whole suite against a binary nobody chose, and NOTHING
///   detects it.  That is the highest-stakes case in this project.
///
/// Attesting costs one 48-byte read of the dump header, which is cheaper than
/// the process spawn it replaces (measured: ~35 ms for `emacs --version`), so
/// the live modes got faster and gained a guarantee.  `Exhaustive` is
/// deliberately NOT used here: nextest runs a process per test, and hashing
/// 18.7 MB in each of a live run's tens of thousands of processes would cost
/// tens of minutes to re-check a file that has not changed since the process
/// before it looked.
fn attested_oracle_emacs() -> Result<&'static ReferenceUse, &'static AttestationError> {
    static ATTESTED: OnceLock<Result<ReferenceUse, AttestationError>> = OnceLock::new();
    ATTESTED
        .get_or_init(|| {
            neomacs_parity_reference::attest(
                Path::new(&oracle_emacs_path()),
                neomacs_parity_reference::AttestationDepth::Fingerprint,
            )
        })
        .as_ref()
}

/// Whether a GNU oracle may be run, and refuse loudly if one is present but
/// wrong.
///
/// A missing GNU is still a SKIP --- that is what lets the snapshot suite run
/// on a machine without the mirror --- but a GNU that is present and is not the
/// pin is a panic, because silently scoring against it is the failure this
/// check exists to prevent.
fn oracle_emacs_available() -> bool {
    match attested_oracle_emacs() {
        Ok(reference) => {
            tracing::debug!(reference = %reference.stamp(), "oracle reference attested");
            true
        }
        // The variant, not the message.  "There is no GNU here" and "the GNU
        // here is the wrong one" have opposite consequences --- a skip and a
        // panic --- so the thing that tells them apart must not be a string
        // anyone could reword.
        Err(error @ AttestationError::ExecutableUnresolved { .. }) => {
            if oracle_emacs_request().named_explicitly {
                panic!(
                    "NEOVM_FORCE_ORACLE_PATH names a GNU oracle that cannot be resolved, so \
                     every live oracle test would SKIP and the run would be green having \
                     measured nothing.  Note that a relative path here resolves against the \
                     test process's working directory, not the shell's.\n{error}"
                );
            }
            tracing::info!("no GNU oracle available: {error}");
            false
        }
        Err(error) => panic!(
            "the GNU oracle is present but is NOT the pinned reference, so its answers \
             are not comparable with this suite's expectations -- and in \
             NEOVM_ORACLE_MODE=refresh they would be WRITTEN INTO them.\n{error}"
        ),
    }
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

/// Resolve `libfaketime.so.1`. Prefer the explicit `NEOVM_LIBFAKETIME_SO`
/// override (set by the flake devShell for reproducibility); otherwise locate
/// it relative to the `faketime` binary on `PATH`. Frozen-clock tests cannot be
/// deterministic without it, so a missing library is a hard, clearly-explained
/// error rather than a silently flaky pass.
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
            // <prefix>/bin/faketime, whose sibling <prefix>/lib holds the .so.
            let Ok(real) = std::fs::canonicalize(&bin) else {
                continue;
            };
            let Some(prefix) = real.parent().and_then(|p| p.parent()) else {
                continue;
            };
            for rel in [
                "lib/libfaketime.so.1",
                "lib/faketime/libfaketime.so.1",
                "lib64/libfaketime.so.1",
                "lib64/faketime/libfaketime.so.1",
            ] {
                let so = prefix.join(rel);
                if so.is_file() {
                    return so.to_string_lossy().into_owned();
                }
            }
        }
    }
    panic!(
        "frozen-time oracle tests require libfaketime, but libfaketime.so.1 could not be located. \
         Set NEOVM_LIBFAKETIME_SO to its path, or run inside `nix develop` (the devShell provides it)."
    );
}

/// The env vars that pin a spawned engine's wall clock to [`ORACLE_FROZEN_TIME`].
/// `FAKETIME_DONT_FAKE_MONOTONIC=1` freezes only `CLOCK_REALTIME` (the wall
/// clock that leaks into timestamps) and leaves `CLOCK_MONOTONIC` real, so the
/// subprocess's own `select`/timeout logic is unaffected.
fn frozen_time_env() -> Vec<(String, String)> {
    vec![
        ("LD_PRELOAD".to_string(), libfaketime_so_path()),
        ("FAKETIME".to_string(), ORACLE_FROZEN_TIME.to_string()),
        ("FAKETIME_DONT_FAKE_MONOTONIC".to_string(), "1".to_string()),
    ]
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
                    (when load-root
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
                      (when load-root
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

    fn configure_command(self, command: &mut Command) {
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

// ---------------------------------------------------------------------------
// Oracle (GNU Emacs) subprocess evaluation
// ---------------------------------------------------------------------------

fn run_oracle_eval_with_sandbox(
    sandbox: &OracleSandbox,
    eval_program: EvalProgram,
) -> Result<CapturedEvaluation, String> {
    // Ledger 214: every GNU invocation in this crate goes through the attested
    // reference, so there is no path that runs an unchecked oracle.
    let oracle = attested_oracle_emacs().map_err(|error| error.to_string())?;
    let oracle_bin = oracle.executable().to_path_buf();

    let mem_limit = oracle_mem_limit_bytes();
    let mut cmd = Command::new(&oracle_bin);
    sandbox.configure(&mut cmd);
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
    let sandbox = OracleSandbox::new(form, load_files, load_root)?;
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

    let mut cmd = Command::new(&neomacs_bin);
    sandbox.configure(&mut cmd);
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

pub(crate) fn assert_oracle_parity_with_env_expect(
    form: &str,
    extra_env: &[(&str, &str)],
    expected: expect_test::Expect,
) {
    let sandbox = oracle_sandbox(form, &[], &project_lisp_dir()).with_extra_env(extra_env);
    assert_oracle_parity_expect_with_sandbox(form, expected, &sandbox, EvalProgram::Normalized);
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
    let sandbox = oracle_sandbox(form, load_files, load_root);
    assert_oracle_parity_expect_with_sandbox(form, expected, &sandbox, EvalProgram::Normalized);
}

/// Non-snapshot variant of `assert_oracle_parity_with_load_root_expect` for
/// cases where no inline GNU expectation is kept (pure live parity).
pub(crate) fn assert_oracle_parity_with_load_root(
    form: &str,
    load_files: &[&str],
    load_root: &Path,
) {
    let sandbox = oracle_sandbox(form, load_files, load_root);
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
