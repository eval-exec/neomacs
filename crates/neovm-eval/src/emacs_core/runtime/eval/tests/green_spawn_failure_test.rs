//! P11 SOAK finding: a Lisp thread whose OS thread cannot be created must
//! signal a Lisp error, not abort the editor.
//!
//! GNU Emacs 31.1 (`src/thread.c:933-938`) unlinks the never-started thread
//! from `all_threads` and calls `error ("Could not start a new thread")` when
//! `sys_thread_create` fails.
//!
//! Reproducer origin: memq's `1a6276ecc0` (v10x/memq-soak) drove the failure
//! through `RLIMIT_NPROC`. The soak finding itself was an address-space
//! failure (the project's 8 GiB `RLIMIT_AS` nextest wrapper), so this
//! adaptation bounds the address space instead: the child builds the
//! evaluator, then lowers `RLIMIT_AS` to its current `VmSize` plus a margin
//! below the configured test-thread stack (`RUST_MIN_STACK`,
//! `.cargo/config.toml`: 128 MiB), so the carrier's stack reservation is what
//! fails and everything before the spawn still fits.
//!
//! Oracle (sandboxed pinned GNU 31.1, `RLIMIT_NPROC=1`):
//!
//! ```text
//! (condition-case e (progn (make-thread (lambda () 1) "probe")
//!                          (princ "NO-ERROR\n"))
//!   (error (princ (format "CAUGHT %S\n" e))))
//! => CAUGHT (error "Could not start a new thread")
//! ```
//!
//! Linux-only (`VmSize` comes from `/proc/self/status`); the deterministic
//! undo-path test in `green_spawn_refusal_test.rs` runs on every host. The
//! collector-thread row of the original finding is not here: it belongs to
//! the GC owner's spawn-tolerance work.

use crate::emacs_core::eval::{Context, ThreadingMode};

const PROBE_ENV: &str = "NEOVM_GREEN_SPAWN_FAILURE_PROBE";
const MAKE_THREAD_TEST: &str = "emacs_core::eval::green::spawn_failure_tests::a_lisp_thread_that_cannot_start_signals_instead_of_aborting";

/// Address-space headroom kept above the built evaluator. Below the 128 MiB
/// `RUST_MIN_STACK` test-thread stack and far above the Lisp form's own
/// allocations, so only the carrier's stack reservation fails.
const ADDRESS_SPACE_MARGIN: u64 = 16 * 1024 * 1024;

/// The configured test-thread stack the margin must undercut.
const MIN_STACK_REQUIRED: u64 = 64 * 1024 * 1024;

/// Current address-space size in bytes (`VmSize` in `/proc/self/status`).
fn address_space_bytes() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").expect("read /proc/self/status");
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmSize:") {
            let kb: u64 = rest
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse()
                .expect("VmSize kB");
            return kb * 1024;
        }
    }
    panic!("/proc/self/status has no VmSize line");
}

fn configured_min_stack() -> u64 {
    std::env::var("RUST_MIN_STACK")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
}

/// Cap the address space just above the evaluator's current size: the
/// make-thread allocations still fit, the carrier's configured stack does
/// not, so `pthread_create` fails with EAGAIN inside the spawn path.
fn lower_address_space_below_thread_stack() {
    let min_stack = configured_min_stack();
    assert!(
        min_stack >= MIN_STACK_REQUIRED,
        "the probe needs the project's test-thread stack (RUST_MIN_STACK >= {MIN_STACK_REQUIRED}); \
         the cargo/nextest environment sets it (found {min_stack})"
    );
    let margin = ADDRESS_SPACE_MARGIN.min(min_stack / 2);
    let limit = address_space_bytes() + margin;
    let rlim = libc::rlimit {
        rlim_cur: limit,
        rlim_max: limit,
    };
    // SAFETY: setrlimit with a valid pointer to a per-process limit; lowering
    // it cannot affect other processes, and the child exits immediately.
    let rc = unsafe { libc::setrlimit(libc::RLIMIT_AS, &rlim) };
    assert_eq!(rc, 0, "setrlimit(RLIMIT_AS) failed");
}

fn child_probe() -> ! {
    // Build the evaluator BEFORE lowering the limit: image/startup work may
    // itself allocate large mappings.
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    lower_address_space_below_thread_stack();
    let source = r#"(condition-case probe-error
                        (progn (make-thread (lambda () 1) "spawn-failure-probe")
                               "NO-ERROR")
                      (error (format "CAUGHT %S" probe-error)))"#;
    match context.eval_str(source) {
        Ok(value) => {
            let text = crate::emacs_core::print::print_value(&value);
            println!("{text}");
            if text.contains("CAUGHT") {
                std::process::exit(0);
            }
            println!("NO-ERROR");
            std::process::exit(3);
        }
        Err(error) => {
            eprintln!("uncaught eval error: {error:?}");
            std::process::exit(4);
        }
    }
}

fn spawn_child(test_path: &str) -> std::process::Output {
    let exe = std::env::current_exe().expect("test binary path");
    std::process::Command::new(exe)
        .arg("--exact")
        .arg(test_path)
        .arg("--nocapture")
        .env(PROBE_ENV, "make-thread")
        .output()
        .expect("spawn probe child")
}

#[test]
fn a_lisp_thread_that_cannot_start_signals_instead_of_aborting() {
    if std::env::var_os(PROBE_ENV).is_some() {
        child_probe();
    }
    let child = spawn_child(MAKE_THREAD_TEST);
    let stdout = String::from_utf8_lossy(&child.stdout);
    let stderr = String::from_utf8_lossy(&child.stderr);
    let tail = stderr
        .lines()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        child.status.success()
            && stdout.contains("CAUGHT")
            && stdout.contains("Could not start a new thread"),
        "a failed thread start must signal the GNU error, not abort the editor.\n\
         GNU 31.1 anchor (src/thread.c:933-938): (error \"Could not start a new thread\")\n\
         child status: {:?}\nstdout: {}\nstderr tail: {}",
        child.status,
        stdout.trim(),
        tail
    );
}
