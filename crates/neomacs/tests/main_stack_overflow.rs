//! A deep recursion in batch neomacs signals instead of crashing, in the
//! layout where it crashed (T11/S0.6): address-space randomization off, as
//! under gdb, rr, `setarch -R` or a shell that inherited
//! `ADDR_NO_RANDOMIZE`. There the kernel's reserve below the main stack is
//! 1 MiB short of the 128 MiB `RLIMIT_STACK` the editor raises to, and the
//! JIT's native-stack guard and stacker's probes, which trusted the limit,
//! ran the stack into the kernel's guard gap (SIGSEGV). These run the real
//! executable, so they need a release build with a matching pdump
//! (`NEOMACS_GUI_TEST_BINARY` overrides the path, like `batch_startup.rs`).

#![cfg(target_os = "linux")]

use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{Command, Output};

fn root() -> PathBuf {
    neomacs_infra::crate_root!().join("../..")
}

fn binary() -> PathBuf {
    let path = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("target/release/neomacs"));
    assert!(
        path.is_file(),
        "release-pdump-tests needs a release neomacs at {}; build it with \
         `cargo xtask fresh-build --release` or set NEOMACS_GUI_TEST_BINARY",
        path.display(),
    );
    path
}

/// Run `neomacs -Q --batch --eval FORM` unrandomized, from the usual 8 MiB
/// stack limit, or `None` where the personality cannot be set.
fn run_unrandomized(form: &str) -> Option<Output> {
    let mut cmd = Command::new(binary());
    cmd.current_dir(root())
        .env("RUST_LOG", "off")
        .args(["-Q", "--batch", "--eval", form]);
    // SAFETY: `personality`, `getrlimit` and `setrlimit` are
    // async-signal-safe.
    unsafe {
        cmd.pre_exec(|| {
            let current = libc::personality(0xffff_ffff);
            if current == -1
                || libc::personality((current | libc::ADDR_NO_RANDOMIZE) as libc::c_ulong) == -1
            {
                return Err(std::io::Error::last_os_error());
            }
            let mut rlim = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::getrlimit(libc::RLIMIT_STACK, &mut rlim) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            rlim.rlim_cur = (8 << 20).min(rlim.rlim_max);
            if libc::setrlimit(libc::RLIMIT_STACK, &rlim) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    match cmd.output() {
        Ok(output) => Some(output),
        Err(error) if error.raw_os_error() == Some(libc::EPERM) => {
            eprintln!("skipping: cannot disable address-space randomization: {error}");
            None
        }
        Err(error) => panic!("run {}: {error}", binary().display()),
    }
}

/// The lines `message` wrote, which batch mode sends to stderr.
fn messages(output: &Output) -> String {
    assert_eq!(
        output.status.signal(),
        None,
        "neomacs died of a signal: {output:?}"
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn line<'a>(text: &'a str, key: &str) -> &'a str {
    text.lines()
        .find_map(|l| l.strip_prefix(key))
        .unwrap_or_else(|| panic!("no `{key}` line in\n{text}"))
}

/// GNU 31.1's depth for the form below, measured live on the pinned
/// `a360712c9d27` (`overflow: (error "Bytecode stack overflow")`,
/// `reached: 74887`). GNU's bytecode stack is 4 MiB (`BC_STACK_SIZE`,
/// src/bytecode.c:337); each frame costs its 32-byte header plus 8 bytes per
/// declared stack word, and the startup's byte-compiled frames are charged
/// too.
const GNU_BYTECODE_STACK_DEPTH: u64 = 74_887;

/// The T11 form: a byte-compiled self-recursion under an unbounded
/// `max-lisp-eval-depth`. It must end where GNU ends, in every tier.
const TIER_FORM: &str = "(progn
   (defvar neovm--sg-reached 0)
   (defun neovm--sg-deep (n)
     (setq neovm--sg-reached n)
     (if (= n 0) 0 (1+ (neovm--sg-deep (1- n)))))
   (byte-compile 'neovm--sg-deep)
   (dotimes (_ 40) (neovm--sg-deep 100))
   (message \"overflow: %S\"
            (let ((max-lisp-eval-depth most-positive-fixnum))
              (condition-case err (neovm--sg-deep 100000000) (error err))))
   (message \"reached: %d\" (- 100000000 neovm--sg-reached))
   (message \"after: %S\" (neovm--sg-deep 500)))";

/// T11 with a dynamic binding in the body: the callee has binds, so the
/// native paths enter it through the framed `invoke_native` entry, not a raw
/// direct entry. GNU still reaches 74887 (the binding costs specpdl, not
/// bytecode stack); the unfixed native guard stopped the JIT tiers at
/// ~10.5k under a hard 8 MiB stack limit.
const FRAMED_TIER_FORM: &str = "(progn
   (defvar neovm--sg-reached 0)
   (defvar neovm--sg-dyn 0)
   (defun neovm--sg-deep (n)
     (setq neovm--sg-reached n)
     (let ((neovm--sg-dyn n))
       (if (= n 0) 0 (1+ (neovm--sg-deep (1- n))))))
   (byte-compile 'neovm--sg-deep)
   (dotimes (_ 40) (neovm--sg-deep 100))
   (message \"overflow: %S\"
            (let ((max-lisp-eval-depth most-positive-fixnum))
              (condition-case err (neovm--sg-deep 100000000) (error err))))
   (message \"reached: %d\" (- 100000000 neovm--sg-reached))
   (message \"after: %S\" (neovm--sg-deep 500)))";

/// A byte-compiled recursion that signals at depth 2500, with
/// `signal-hook-function` capturing a `backtrace-frames` digest at signal
/// time -- GNU calls the hook before unwinding, so the whole Lisp stack is
/// intact.  The digest over the `neovm--sg-deep` frames is (count, sum of
/// the level numbers, innermost level, outermost level): one frame per Lisp
/// level prints as `(2501 3126250 2500 0)`.
const FRAMES_SIGNAL_FORM: &str = "(progn
  (defvar neovm--sg-frames nil)
  (defvar neovm--sg-signal nil)
  (defun neovm--sg-deep (n limit)
    (if (>= n limit)
        (error \"sg-boom\")
      (1+ (neovm--sg-deep (1+ n) limit))))
  (byte-compile 'neovm--sg-deep)
  (dotimes (_ 40) (condition-case nil (neovm--sg-deep 0 10) (error nil)))
  (let ((signal-hook-function
         (lambda (sym data)
           (when (and (eq sym 'error) (null neovm--sg-signal))
             (setq neovm--sg-signal (list sym data))
             (let* ((fs (backtrace-frames))
                    (ns (delq nil
                              (mapcar (lambda (f)
                                        (when (eq (nth 1 f) 'neovm--sg-deep)
                                          (car (nth 2 f))))
                                      fs))))
               (setq neovm--sg-frames
                     (list (length ns)
                           (let ((s 0)) (dolist (v ns) (setq s (+ s v))) s)
                           (car ns) (car (last ns)))))))))
    (message \"caught: %S\"
             (let ((max-lisp-eval-depth most-positive-fixnum))
               (condition-case err (neovm--sg-deep 0 2500) (error err)))))
  (message \"signal: %S\" neovm--sg-signal)
  (message \"frames: %S\" neovm--sg-frames))";

/// The hard limit of [`FRAMES_SIGNAL_FORM`]: 1408 KiB caps `main`'s stack
/// raise so the native stack holds ~1300 levels of the form (the unfixed
/// guard signalled at 1314 there), which makes the signal at depth 2500 a
/// signal raised after at least one guard fallback.
const FRAMES_LIMIT: u64 = 1408 * 1024;

/// T11 pinned to GNU 31.1: the deep byte-compiled recursion signals GNU's
/// `Bytecode stack overflow` at GNU's exact depth, in the unrandomized main
/// stack layout where the pre-ruling-2 build died.
#[test]
#[cfg(feature = "release-pdump-tests")]
fn batch_deep_compiled_recursion_signals_without_randomization() {
    let expected = gnu_expected("tier-form", TIER_FORM);
    let Some(output) = run_unrandomized(TIER_FORM) else {
        return;
    };
    let text = messages(&output);
    assert_eq!(line(&text, "overflow: "), expected.0.as_str());
    assert_eq!(line(&text, "after: "), expected.1.as_str());
    let reached: u64 = line(&text, "reached: ").parse().expect("a count");
    assert_eq!(reached, expected.2, "{text}");
}

/// Run `neomacs -Q --batch --eval FORM` with `envs` set.
fn run_with_env(form: &str, envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(binary());
    cmd.current_dir(root())
        .env("RUST_LOG", "off")
        .args(["-Q", "--batch", "--eval", form]);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output()
        .unwrap_or_else(|error| panic!("run {}: {error}", binary().display()))
}

/// Run `neomacs -Q --batch --eval FORM` with `envs` set and a HARD `limit`
/// stack limit: `setrlimit` lowers both the soft and the hard limit, the
/// same state `ulimit -s N` leaves in a shell. `main`'s `RLIMIT_STACK`
/// raise is then capped at the hard limit, so the native stack cannot grow
/// past `limit` and only the bytecode-stack accounting can bound GNU's
/// recursion.
fn run_with_stack_limit(form: &str, envs: &[(&str, &str)], limit: u64) -> Option<Output> {
    let mut cmd = Command::new(binary());
    cmd.current_dir(root())
        .env("RUST_LOG", "off")
        .args(["-Q", "--batch", "--eval", form]);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    // SAFETY: `personality`, `getrlimit` and `setrlimit` are
    // async-signal-safe.
    unsafe {
        cmd.pre_exec(move || {
            let current = libc::personality(0xffff_ffff);
            if current == -1
                || libc::personality((current | libc::ADDR_NO_RANDOMIZE) as libc::c_ulong) == -1
            {
                return Err(std::io::Error::last_os_error());
            }
            let mut rlim = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::getrlimit(libc::RLIMIT_STACK, &mut rlim) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let want = limit.min(rlim.rlim_max);
            rlim.rlim_cur = want;
            rlim.rlim_max = want;
            if libc::setrlimit(libc::RLIMIT_STACK, &rlim) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    match cmd.output() {
        Ok(output) => Some(output),
        Err(error) if error.raw_os_error() == Some(libc::EPERM) => {
            eprintln!("skipping: cannot disable address-space randomization: {error}");
            None
        }
        Err(error) => panic!("run {}: {error}", binary().display()),
    }
}

/// [`run_with_stack_limit`] at the 8 MiB hard limit of an `ulimit -s 8192`
/// shell.
fn run_hard_limit(form: &str, envs: &[(&str, &str)]) -> Option<Output> {
    run_with_stack_limit(form, envs, 8 << 20)
}

/// Ruling 2's bar: the bytecode-stack overflow depth is IDENTICAL across the
/// execution tiers (interpreter, JIT with direct calls, the shim's fast path,
/// the forced slow-spec and forced-deopt harnesses) and equal to GNU 31.1's
/// live measurement of the same `--eval` form.
/// The five execution tiers the exactness matrix covers.
const TIERS: [(&str, &[(&str, &str)]); 5] = [
    ("jit-off", &[("NEOVM_JIT", "0")]),
    ("jit-on", &[]),
    ("direct-call-off", &[("NEOVM_JIT_DIRECT_CALL", "off")]),
    ("slow-spec", &[("NEOVM_JIT_FORCE_SLOW_SPEC", "1")]),
    ("deopt", &[("NEOVM_JIT_FORCE_DEOPT", "1")]),
];

#[test]
#[cfg(feature = "release-pdump-tests")]
fn batch_bytecode_stack_depth_is_tier_independent_and_gnu_exact() {
    let expected = gnu_expected("tier-form", TIER_FORM);
    for (tier, envs) in TIERS {
        let output = run_with_env(TIER_FORM, envs);
        let text = messages(&output);
        assert_eq!(
            line(&text, "overflow: "),
            expected.0.as_str(),
            "{tier}: {text}"
        );
        assert_eq!(
            line(&text, "after: "),
            expected.1.as_str(),
            "{tier}: {text}"
        );
        let reached: u64 = line(&text, "reached: ").parse().expect("a count");
        assert_eq!(
            reached, expected.2,
            "{tier}: the bytecode-stack depth must be GNU's on this form: {text}"
        );
    }
}

/// The hard-8-MiB limit (see `run_hard_limit`): with the soft and hard
/// `RLIMIT_STACK` both at 8 MiB the editor's raise is capped.  GNU's depth is
/// set by the 4 MiB bytecode stack (`BC_STACK_SIZE`, src/bytecode.c:337), not
/// by the OS stack -- GNU keeps its frames on `bc->stack` and its native
/// stack does not grow per Lisp level -- so every tier must still reach the
/// pinned reference's answers for this form.  Compiled leaves used to signal
/// from their native-stack guard at ~49k (`~22k` through the call shims); the
/// guard now answers `STATUS_STACK_FALLBACK` and the call is re-run on the
/// Tier-0 interpreter, whose stacker probes grow segments, keeping the
/// bytecode-stack accounting authoritative.
#[test]
#[cfg(feature = "release-pdump-tests")]
fn batch_bytecode_stack_depth_survives_a_hard_8mib_stack_limit() {
    let expected = gnu_expected("tier-form", TIER_FORM);
    for (tier, envs) in TIERS {
        let Some(output) = run_hard_limit(TIER_FORM, envs) else {
            return;
        };
        let text = messages(&output);
        assert_eq!(
            line(&text, "overflow: "),
            expected.0.as_str(),
            "{tier}: {text}"
        );
        assert_eq!(
            line(&text, "after: "),
            expected.1.as_str(),
            "{tier}: {text}"
        );
        let reached: u64 = line(&text, "reached: ").parse().expect("a count");
        assert_eq!(
            reached, expected.2,
            "{tier}: the bytecode-stack depth must not depend on the OS stack limit: {text}"
        );
    }
}

/// The framed-entry half of the hard-8-MiB guarantee: a callee with a dynamic
/// binding is entered through `invoke_native` (framed), not a raw direct
/// entry, so its entry guard's `STATUS_STACK_FALLBACK` must be mapped there
/// too.  Before the fallback the native guard signalled at ~10.5k; with only
/// the direct-entry mapping the framed entry would trip the side-effect
/// assert.  Every tier must reach the pinned reference's recorded answers for
/// the form (`framed-tier-form`, measured live on the pinned 31.1), as the
/// interpreter does.
#[test]
#[cfg(feature = "release-pdump-tests")]
fn batch_framed_bytecode_stack_depth_survives_a_hard_8mib_stack_limit() {
    let expected = gnu_expected("framed-tier-form", FRAMED_TIER_FORM);
    for (tier, envs) in TIERS {
        let Some(output) = run_hard_limit(FRAMED_TIER_FORM, envs) else {
            return;
        };
        let text = messages(&output);
        assert_eq!(
            line(&text, "overflow: "),
            expected.0.as_str(),
            "{tier}: {text}"
        );
        assert_eq!(
            line(&text, "after: "),
            expected.1.as_str(),
            "{tier}: {text}"
        );
        let reached: u64 = line(&text, "reached: ").parse().expect("a count");
        assert_eq!(
            reached, expected.2,
            "{tier}: the bytecode-stack depth must not depend on the OS stack limit: {text}"
        );
    }
}

/// The fallback-soundness bar: a signal raised after a guard fallback must
/// show each Lisp level exactly ONCE in `backtrace-frames`, as the pinned
/// GNU reference does.  At the [`FRAMES_LIMIT`] hard limit the native stack
/// holds ~1300 levels of this form, so the depth-2500 signal is raised after
/// at least one guard fallback (the unfixed guard signalled at 1314 instead
/// of reaching it).  The native caller records the callee's backtrace frame
/// once before the native attempt, and the interpreter rerun
/// (`interp_fallback_native` -> `execute_from_stack_args`) adds no frame and
/// charges the bytecode frame exactly once: `run_resolved_leaf_native`
/// releases its charge on the Fallback return, and the direct-site cold exit
/// releases `budget_restore` before the rerun.
#[test]
#[cfg(feature = "release-pdump-tests")]
fn batch_signal_after_a_guard_fallback_shows_each_lisp_frame_once() {
    let expected_caught = gnu_line("caught: ", FRAMES_SIGNAL_FORM);
    let expected_signal = gnu_line("signal: ", FRAMES_SIGNAL_FORM);
    let expected_frames = gnu_line("frames: ", FRAMES_SIGNAL_FORM);
    assert_eq!(
        expected_frames, "(2501 3126250 2500 0)",
        "the pinned GNU 31.1 frame digest moved"
    );
    for (tier, envs) in TIERS {
        let Some(output) = run_with_stack_limit(FRAMES_SIGNAL_FORM, envs, FRAMES_LIMIT) else {
            return;
        };
        let text = messages(&output);
        assert_eq!(
            line(&text, "caught: "),
            expected_caught.as_str(),
            "{tier}: {text}"
        );
        assert_eq!(
            line(&text, "signal: "),
            expected_signal.as_str(),
            "{tier}: {text}"
        );
        assert_eq!(
            line(&text, "frames: "),
            expected_frames.as_str(),
            "{tier}: a fallback must leave one backtrace frame per Lisp level: {text}"
        );
    }
}

/// The pinned GNU reference's executable: `GNU_EMACS_BINARY`, or `src/emacs`
/// under the `GNU_EMACS_SOURCE` checkout, or the CI oracle install.  A
/// missing reference is a hard error: this suite asserts parity with the
/// pinned GNU, so silently substituting a recorded number would hide a
/// moved or absent oracle.
fn oracle_binary() -> PathBuf {
    if let Some(binary) = std::env::var_os("GNU_EMACS_BINARY") {
        let binary = PathBuf::from(binary);
        if binary.is_file() {
            return binary;
        }
    }
    if let Some(source) = std::env::var_os("GNU_EMACS_SOURCE") {
        let binary = PathBuf::from(source).join("src").join("emacs");
        if binary.is_file() {
            return binary;
        }
    }
    let ci = PathBuf::from("/opt/gnu-emacs-oracle/bin/emacs");
    if ci.is_file() {
        return ci;
    }
    panic!(
        "the exactness oracle is required: set GNU_EMACS_SOURCE to the pinned \
         emacs-31.1 checkout, GNU_EMACS_BINARY to its executable, or provide \
         /opt/gnu-emacs-oracle/bin/emacs; refusing to assert the \
         bytecode-stack depth without the pinned reference"
    );
}

/// Check once that the resolved oracle is the pinned GNU Emacs 31.1.
fn oracle_version_checked(binary: &std::path::Path) {
    static CHECKED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    CHECKED.get_or_init(|| {
        let output = Command::new(binary)
            .args(["-Q", "--batch", "--eval", "(princ emacs-version)"])
            .output()
            .unwrap_or_else(|error| panic!("run {}: {error}", binary.display()));
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        assert!(
            text.starts_with("31.1"),
            "the exactness oracle must be GNU Emacs 31.1 (got {text:?})"
        );
    });
}

/// The recorded pinned-GNU answers for one form label.  Each value was
/// taken from a live GNU 31.1 run of that exact form; the tests assert the
/// live oracle still agrees with them, so a moved reference or a changed
/// form cannot silently shift the expectation.
fn recorded_answer(label: &str) -> (String, String, u64) {
    // Recorded from live pinned GNU 31.1 runs of each form.  The plain
    // recursion forms reach GNU's 74887; a re-entry form that fires its
    // mechanism at depth 70000 reaches about 4885 because the nested
    // recursion starts from that depth, so each form keeps its own number.
    let reached = match label {
        "tier-form" => GNU_BYTECODE_STACK_DEPTH,
        "framed-tier-form" => 74_887,
        "throw across bytecode frames" => 74_887,
        "subr reentry and GC" => 74_887,
        "per-thread" => 74_887,
        "funcall" => 4_885,
        "apply" => 4_885,
        "mapcar" => 4_884,
        "watcher" => 4_884,
        "handler" => 4_886,
        "unwind-protect" => 4_885,
        "save-window-excursion" => 4_885,
        _ => panic!("no recorded GNU answer for the form {label:?}"),
    };
    (
        "(error \"Bytecode stack overflow\")".to_owned(),
        "500".to_owned(),
        reached,
    )
}

/// The expected answers for `label`'s `form`: run the pinned GNU on the
/// identical form, require the live answers to equal the recorded ones, and
/// return them.  `NEOVM_DUMP_ORACLE` prints the live answers and returns
/// them without the comparison (used only to record a new form's values).
fn gnu_expected(label: &str, form: &str) -> (String, String, u64) {
    let binary = oracle_binary();
    oracle_version_checked(&binary);
    let output = Command::new(&binary)
        .args(["-Q", "--batch", "--eval", form])
        .output()
        .unwrap_or_else(|error| panic!("run {}: {error}", binary.display()));
    assert!(
        output.status.success(),
        "the GNU reference run failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stderr).into_owned();
    let live = (
        line(&text, "overflow: ").to_owned(),
        line(&text, "after: ").to_owned(),
        line(&text, "reached: ").parse::<u64>().expect("a count"),
    );
    let recorded = recorded_answer(label);
    assert_eq!(
        live.0, recorded.0,
        "{label}: the pinned GNU reference's error identity moved"
    );
    assert_eq!(
        live.1, recorded.1,
        "{label}: the pinned GNU reference's after value moved"
    );
    assert_eq!(
        live.2, recorded.2,
        "{label}: the pinned GNU reference's depth moved"
    );
    recorded
}

/// Run `form` on the pinned GNU reference (same executable resolution and
/// version check as [`gnu_expected`]) and return its `key` line.
fn gnu_line(key: &str, form: &str) -> String {
    let binary = oracle_binary();
    oracle_version_checked(&binary);
    let output = Command::new(&binary)
        .args(["-Q", "--batch", "--eval", form])
        .output()
        .unwrap_or_else(|error| panic!("run {}: {error}", binary.display()));
    assert!(
        output.status.success(),
        "the GNU reference run failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stderr).into_owned();
    line(&text, key).to_owned()
}

/// The tree walker recurses on the main stack until stacker's probe moves
/// it to a heap segment; past the main stack's end it must already be on
/// one, so the recursion ends at `max-lisp-eval-depth`.
#[test]
#[cfg(feature = "release-pdump-tests")]
fn batch_deep_interpreted_recursion_leaves_the_main_stack_in_time() {
    let Some(output) = run_unrandomized(
        "(progn
           (defun neovm--sg-walk (n) (if (= n 0) 0 (1+ (neovm--sg-walk (1- n)))))
           (message \"walk: %S\"
                    (let ((max-lisp-eval-depth 2000000))
                      (condition-case err (neovm--sg-walk 1500000) (error err)))))",
    ) else {
        return;
    };
    let text = messages(&output);
    assert!(
        line(&text, "walk: ").starts_with("(excessive-lisp-nesting "),
        "{text}"
    );
}

/// The Lisp-thread kind of the #92 coverage in the release binary: a
/// `make-thread` body's interpreted recursion (which the bytecode budget
/// does not bound) ends in the clean Lisp depth error through `thread-join`.
#[test]
#[cfg(feature = "release-pdump-tests")]
fn make_thread_interpreted_recursion_ends_in_a_depth_error() {
    let output = run_with_env(
        "(progn
           (defun neovm--sg-walk-thread (n)
             (if (= n 0) 0 (1+ (neovm--sg-walk-thread (1- n)))))
           (message \"thread: %S\"
                    (thread-join
                     (make-thread
                      (lambda ()
                        (let ((max-lisp-eval-depth 300000))
                          (condition-case err (neovm--sg-walk-thread 400000)
                            (error err))))))))",
        &[],
    );
    let text = messages(&output);
    assert!(
        line(&text, "thread: ").starts_with("(excessive-lisp-nesting "),
        "{text}"
    );
}

/// The dynamic (`-l` of a file without a lexical-binding cookie) form has no
/// GNU reference -- GNU 31.1 itself overflows its C stack on it -- so Neo is
/// pinned only to "clean Lisp error, no signal", with NO depth assertion.
#[test]
#[cfg(feature = "release-pdump-tests")]
fn dynamic_l_file_deep_recursion_ends_in_a_clean_lisp_error() {
    let dir = std::env::temp_dir().join(format!("neomacs-dynamic-l-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let probe = dir.join("probe.el");
    std::fs::write(
        &probe,
        "(progn
           (defvar neovm--sg-reached 0)
           (defun neovm--sg-deep (n)
             (setq neovm--sg-reached n)
             (if (= n 0) 0 (1+ (neovm--sg-deep (1- n)))))
           (byte-compile 'neovm--sg-deep)
           (dotimes (_ 40) (neovm--sg-deep 100))
           (message \"overflow: %S\"
                    (let ((max-lisp-eval-depth most-positive-fixnum))
                      (condition-case err (neovm--sg-deep 100000000) (error err))))
           (message \"after: %S\" (neovm--sg-deep 500)))",
    )
    .expect("write probe");
    let mut cmd = Command::new(binary());
    cmd.current_dir(root())
        .env("RUST_LOG", "off")
        .args(["-Q", "--batch", "-l"])
        .arg(&probe);
    let output = cmd.output().expect("run neomacs");
    let _ = std::fs::remove_dir_all(&dir);
    let text = messages(&output);
    assert_eq!(
        line(&text, "overflow: "),
        "(error \"Bytecode stack overflow\")",
        "{text}"
    );
    assert_eq!(line(&text, "after: "), "500", "{text}");
}

/// A boundary event followed by the T11 count.  If the event leaks any
/// bytecode-stack position (its frames' charges never restored), the count
/// stops below GNU's depth and the test fails; the exact GNU depth is the
/// assertion.
fn boundary_tier_form(event: &str) -> String {
    format!(
        "(progn
           (defvar neovm--sg-reached 0)
           (defun neovm--sg-deep (n)
             (setq neovm--sg-reached n)
             (if (= n 0) 0 (1+ (neovm--sg-deep (1- n)))))
           (byte-compile 'neovm--sg-deep)
           (dotimes (_ 40) (neovm--sg-deep 100))
           {event}
           (message \"overflow: %S\"
                    (let ((max-lisp-eval-depth most-positive-fixnum))
                      (condition-case err (neovm--sg-deep 100000000) (error err))))
           (message \"reached: %d\" (- 100000000 neovm--sg-reached))
           (message \"after: %S\" (neovm--sg-deep 500)))"
    )
}

fn assert_gnu_depth_after(event: &str, label: &str) {
    let form = boundary_tier_form(event);
    // The expectation comes from the pinned GNU reference running the SAME
    // form; the earlier boundary event may legitimately shift GNU's own
    // reached count, so a hard-coded constant must not decide this.
    let expected = gnu_expected(label, &form);
    let Some(output) = run_unrandomized(&form) else {
        return;
    };
    let text = messages(&output);
    assert_eq!(
        line(&text, "overflow: "),
        expected.0.as_str(),
        "{label}: {text}"
    );
    assert_eq!(
        line(&text, "after: "),
        expected.1.as_str(),
        "{label}: {text}"
    );
    let reached: u64 = line(&text, "reached: ").parse().expect("a count");
    assert_eq!(
        reached, expected.2,
        "{label}: the bytecode-stack position was not restored by the boundary: {text}"
    );
}

/// A re-entry form: the byte-compiled leaf makes a direct self call that
/// RETURNS, then runs one mechanism that re-enters Lisp/bytecode and makes
/// that nested recursion reach the bytecode-stack boundary.  After the
/// returned direct call a position-argument JIT leaf's Context mirror still
/// holds the callee's position, so every such mechanism must publish the
/// leaf's own position before it re-enters; otherwise the nested frame is
/// charged from the wrong base and its overflow depth differs from GNU's.
fn reentry_tier_form(event: &str) -> String {
    format!(
        "(progn
           (defvar neovm--sg-reached 0)
           (defvar neovm--sg-trigger 99930000)
           (defvar neovm--sg-event nil)
           (defvar neovm--sg-watched 0)
           (defun neovm--sg-deep (n)
             (setq neovm--sg-reached n)
             (if (= n 0) 0
               (progn
                 (when (> n 1000) (neovm--sg-deep 0))
                 (when (= n neovm--sg-trigger) (funcall neovm--sg-event))
                 (1+ (neovm--sg-deep (1- n))))))
           (byte-compile 'neovm--sg-deep)
           (setq neovm--sg-event
                 (byte-compile (lambda () (setq neovm--sg-event nil) {event})))
           (dotimes (_ 40) (neovm--sg-deep 100))
           (message \"overflow: %S\"
                    (let ((max-lisp-eval-depth most-positive-fixnum))
                      (condition-case err (neovm--sg-deep 100000000) (error err))))
           (message \"reached: %d\" (- 100000000 neovm--sg-reached))
           (message \"after: %S\" (neovm--sg-deep 500)))"
    )
}

/// Every tier must match GNU's live answers on the SAME re-entry form.
fn assert_reentry_matches_gnu(event: &str, label: &str) {
    let form = reentry_tier_form(event);
    let expected = gnu_expected(label, &form);
    for (tier, envs) in TIERS {
        let output = run_with_env(&form, envs);
        let text = messages(&output);
        assert_eq!(
            line(&text, "overflow: "),
            expected.0.as_str(),
            "{label}/{tier}: {text}"
        );
        assert_eq!(
            line(&text, "after: "),
            expected.1.as_str(),
            "{label}/{tier}: {text}"
        );
        let reached: u64 = line(&text, "reached: ").parse().expect("a count");
        assert_eq!(
            reached, expected.2,
            "{label}/{tier}: a re-entry boundary charged the nested frame from a stale \
             position instead of the leaf's own: {text}"
        );
    }
}

/// The re-entry boundary matrix: funcall, apply, mapcar, a watched setq, a
/// condition-case handler, an unwind-protect cleanup and
/// save-window-excursion, each fired from inside the directly-called leaf
/// after a returned direct call, on all five tiers, against live GNU.
#[test]
#[cfg(feature = "release-pdump-tests")]
fn batch_bytecode_stack_depth_matches_gnu_through_reentry_boundaries() {
    let events: [(&str, &str); 7] = [
        (
            "funcall",
            "(funcall (byte-compile (lambda (n) (neovm--sg-deep n))) 100000000)",
        ),
        (
            "apply",
            "(apply (byte-compile (lambda (n) (neovm--sg-deep n))) (list 100000000))",
        ),
        (
            "mapcar",
            "(mapcar (byte-compile (lambda (n) (neovm--sg-deep n))) (list 100000000))",
        ),
        (
            "watcher",
            "(progn
               (add-variable-watcher 'neovm--sg-watched
                 (byte-compile (lambda (&rest _) (neovm--sg-deep 100000000))))
               (setq neovm--sg-watched 1))",
        ),
        (
            "handler",
            "(condition-case nil (error \"boom\") (error (neovm--sg-deep 100000000)))",
        ),
        (
            "unwind-protect",
            "(unwind-protect nil (neovm--sg-deep 100000000))",
        ),
        (
            "save-window-excursion",
            "(save-window-excursion (neovm--sg-deep 100000000))",
        ),
    ];
    for (label, event) in events {
        assert_reentry_matches_gnu(event, label);
    }
}

/// A non-local exit (`throw`) unwinds through several bytecode frames; the
/// catch must restore the frame position, so the following recursion still
/// reaches GNU's exact depth.
#[test]
#[cfg(feature = "release-pdump-tests")]
fn batch_bytecode_stack_position_restores_across_a_throw() {
    let event = "(progn
       (defun neovm--bb-throw (n)
         (if (= n 0) (throw 'neovm--bb-out 7) (neovm--bb-throw (1- n))))
       (byte-compile 'neovm--bb-throw)
       (catch 'neovm--bb-out (neovm--bb-throw 300)))";
    assert_gnu_depth_after(event, "throw across bytecode frames");
}

/// A builtin (mapcar) re-enters bytecode through a subr, and a forced GC
/// runs a safepoint inside bytecode; both must leave the mirror exact.
#[test]
#[cfg(feature = "release-pdump-tests")]
fn batch_bytecode_stack_position_survives_subr_reentry_and_gc() {
    let event = "(progn
       (defun neovm--bb-cb (x) (if (= x 0) 0 (1+ (neovm--bb-cb (1- x)))))
       (byte-compile 'neovm--bb-cb)
       (mapcar #'neovm--bb-cb '(100 200 300))
       (let (garbage)
         (dotimes (_ 2000)
           (setq garbage (cons (make-list 8 0) garbage)))
         (setq garbage nil)
         (garbage-collect)))";
    assert_gnu_depth_after(event, "subr reentry and GC");
}

/// A Lisp thread runs its own deep bytecode recursion to the budget's error
/// and joins; the main thread's position must be untouched, and the thread's
/// own error must be GNU's.
#[test]
#[cfg(feature = "release-pdump-tests")]
fn batch_bytecode_stack_position_is_per_thread() {
    let event = "(progn
       (defun neovm--bb-tw (n) (if (= n 0) 0 (1+ (neovm--bb-tw (1- n)))))
       (byte-compile 'neovm--bb-tw)
       (message \"thread: %S\"
                (thread-join
                 (make-thread
                  (lambda ()
                    (let ((max-lisp-eval-depth most-positive-fixnum))
                      (condition-case err (neovm--bb-tw 100000000) (error err))))))))";
    let form = boundary_tier_form(event);
    let expected = gnu_expected("per-thread", &form);
    let Some(output) = run_unrandomized(&form) else {
        return;
    };
    let text = messages(&output);
    assert_eq!(
        line(&text, "thread: "),
        "(error \"Bytecode stack overflow\")",
        "the Lisp thread's own bytecode budget must signal GNU's error: {text}"
    );
    assert_eq!(line(&text, "overflow: "), expected.0.as_str(), "{text}");
    assert_eq!(line(&text, "after: "), expected.1.as_str(), "{text}");
    let reached: u64 = line(&text, "reached: ").parse().expect("a count");
    assert_eq!(
        reached, expected.2,
        "the thread switch corrupted the main thread's bytecode-stack position: {text}"
    );
}
