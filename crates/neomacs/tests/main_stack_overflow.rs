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

/// T11 pinned to GNU 31.1: the deep byte-compiled recursion signals GNU's
/// `Bytecode stack overflow` at GNU's exact depth, in the unrandomized main
/// stack layout where the pre-ruling-2 build died.
#[test]
#[cfg(feature = "release-pdump-tests")]
fn batch_deep_compiled_recursion_signals_without_randomization() {
    let Some(output) = run_unrandomized(TIER_FORM) else {
        return;
    };
    let text = messages(&output);
    assert_eq!(
        line(&text, "overflow: "),
        "(error \"Bytecode stack overflow\")"
    );
    assert_eq!(line(&text, "after: "), "500");
    let reached: u64 = line(&text, "reached: ").parse().expect("a count");
    assert_eq!(reached, GNU_BYTECODE_STACK_DEPTH, "{text}");
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

/// Ruling 2's bar: the bytecode-stack overflow depth is IDENTICAL across the
/// execution tiers (interpreter, JIT with direct calls, the shim's fast path,
/// the forced slow-spec and forced-deopt harnesses) and equal to GNU 31.1's
/// live measurement of the same `--eval` form.
#[test]
#[cfg(feature = "release-pdump-tests")]
fn batch_bytecode_stack_depth_is_tier_independent_and_gnu_exact() {
    let tiers: [(&str, &[(&str, &str)]); 5] = [
        ("jit-off", &[("NEOVM_JIT", "0")]),
        ("jit-on", &[]),
        ("direct-call-off", &[("NEOVM_JIT_DIRECT_CALL", "off")]),
        ("slow-spec", &[("NEOVM_JIT_FORCE_SLOW_SPEC", "1")]),
        ("deopt", &[("NEOVM_JIT_FORCE_DEOPT", "1")]),
    ];
    for (tier, envs) in tiers {
        let output = run_with_env(TIER_FORM, envs);
        let text = messages(&output);
        assert_eq!(
            line(&text, "overflow: "),
            "(error \"Bytecode stack overflow\")",
            "{tier}: {text}"
        );
        assert_eq!(line(&text, "after: "), "500", "{tier}: {text}");
        let reached: u64 = line(&text, "reached: ").parse().expect("a count");
        assert_eq!(
            reached, GNU_BYTECODE_STACK_DEPTH,
            "{tier}: the bytecode-stack depth must be GNU's: {text}"
        );
    }
    if let Some((gnu_reached, gnu_overflow)) = gnu_live_probe() {
        assert_eq!(
            gnu_reached, GNU_BYTECODE_STACK_DEPTH,
            "the pinned GNU reference's depth moved"
        );
        assert_eq!(
            gnu_overflow, "(error \"Bytecode stack overflow\")",
            "the pinned GNU reference's error identity moved"
        );
    } else {
        eprintln!(
            "the live GNU reference was not run (set GNU_EMACS_SOURCE to the pinned \
             emacs-31.1 checkout or GNU_EMACS_BINARY to its executable): the pinned \
             depth {GNU_BYTECODE_STACK_DEPTH} is asserted instead"
        );
    }
}

/// The pinned GNU 31.1 reference's answers for [`TIER_FORM`], when it is
/// available: `GNU_EMACS_SOURCE` names its checkout (`src/emacs`), or
/// `GNU_EMACS_BINARY` its executable.
fn gnu_live_probe() -> Option<(u64, String)> {
    fn gnu_binary() -> Option<PathBuf> {
        if let Some(binary) = std::env::var_os("GNU_EMACS_BINARY") {
            let binary = PathBuf::from(binary);
            return binary.is_file().then_some(binary);
        }
        let source = PathBuf::from(std::env::var_os("GNU_EMACS_SOURCE")?);
        let binary = source.join("src").join("emacs");
        binary.is_file().then_some(binary)
    }
    let binary = gnu_binary()?;
    let output = Command::new(&binary)
        .args(["-Q", "--batch", "--eval", TIER_FORM])
        .output()
        .ok()?;
    if !output.status.success() {
        eprintln!(
            "the GNU reference run failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        return None;
    }
    let text = String::from_utf8_lossy(&output.stderr).into_owned();
    let reached = line(&text, "reached: ").parse().ok()?;
    Some((reached, line(&text, "overflow: ").to_owned()))
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
