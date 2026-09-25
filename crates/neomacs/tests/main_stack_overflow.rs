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
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn binary() -> PathBuf {
    std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("target/release/neomacs"))
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

/// tmp/v10x/probes/j2pf/probe-t11.el: GNU 31.1 prints
/// `overflow: (error "Bytecode stack overflow")`, `reached: 74884 levels`
/// and `after: 500`.
#[test]
#[ignore = "requires release executable with matching pdump"]
fn batch_deep_compiled_recursion_signals_without_randomization() {
    let Some(output) = run_unrandomized(
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
           (message \"reached: %d\" (- 100000000 neovm--sg-reached))
           (message \"after: %S\" (neovm--sg-deep 500)))",
    ) else {
        return;
    };
    let text = messages(&output);
    assert_eq!(
        line(&text, "overflow: "),
        "(error \"Bytecode stack overflow\")"
    );
    assert_eq!(line(&text, "after: "), "500");
    let reached: u64 = line(&text, "reached: ").parse().expect("a count");
    assert!(reached > 100_000, "ran on the 127 MiB main stack: {text}");
}

/// The tree walker recurses on the main stack until stacker's probe moves
/// it to a heap segment; past the main stack's end it must already be on
/// one, so the recursion ends at `max-lisp-eval-depth`.
#[test]
#[ignore = "requires release executable with matching pdump"]
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
