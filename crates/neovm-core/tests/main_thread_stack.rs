//! The native-stack guard on the process's MAIN thread, where batch and
//! `-nw` neomacs evaluate (T11/S0.6): a byte-compiled recursion under an
//! unbounded `max-lisp-eval-depth` signals GNU's
//! `(error "Bytecode stack overflow")` instead of dying of SIGSEGV.
//!
//! The in-crate tests run on libtest's threads, whose stacks glibc knows
//! exactly. The main thread's stack is different: it grows on demand, and
//! glibc -- through stacker, which the guard and the stacker probes trust --
//! bounds it by `RLIMIT_STACK` or by the end of the mapping below it,
//! while the kernel stops it `stack_guard_gap` (1 MiB) above that mapping.
//! Without address-space randomization the mapping below (the dynamic
//! loader) sits exactly 128 MiB under the stack's top, so the editor's
//! 128 MiB limit made the stack end 1 MiB above where the guard believed.
//!
//! So this target has no libtest harness: the test runs children of this
//! binary whose MAIN thread does what `neomacs` does (raise the limit, build
//! the evaluator, recurse), in that layout -- once with a readable page
//! planted where the loader sits without randomization (deterministic on
//! any host), once under `ADDR_NO_RANDOMIZE` itself (skipped where the
//! personality cannot be set, e.g. under a container's seccomp policy).
//! The CLI answers the libtest subset nextest uses (`--list --format terse`,
//! `--exact NAME`).

const TEST: &str = "deep_compiled_recursion_signals_on_the_main_thread";

/// `neomacs`'s `RLIMIT_STACK` target (`increase_stack_limit`).
const TARGET_STACK: usize = 128 * 1024 * 1024;

fn main() {
    #[cfg(target_os = "linux")]
    if let Ok(scenario) = std::env::var(linux::SCENARIO_ENV) {
        std::process::exit(linux::child(&scenario));
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    if flag("--list") {
        if !flag("--ignored") {
            println!("{TEST}: test");
        }
        return;
    }
    let mut filters = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--test-threads" | "--format" | "--color" | "--skip" | "--logfile" | "-Z" => {
                rest.next();
            }
            a if a.starts_with('-') => {}
            a => filters.push(a),
        }
    }
    let selected = filters.is_empty()
        || filters.iter().any(|f| {
            if flag("--exact") {
                *f == TEST
            } else {
                TEST.contains(f)
            }
        });
    if !selected || flag("--ignored") {
        println!("running 0 tests");
        return;
    }
    #[cfg(target_os = "linux")]
    linux::parent();
    println!("test {TEST} ... ok");
}

#[cfg(target_os = "linux")]
mod linux {
    use super::TARGET_STACK;
    use neovm_core::emacs_core::format_eval_result_with_eval;
    use neovm_core::emacs_core::load::{
        apply_runtime_startup_state, create_bootstrap_evaluator_cached,
    };
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    use std::process::Command;

    pub const SCENARIO_ENV: &str = "NEOVM_MAIN_STACK_SCENARIO";

    /// A child's exit status for "this host cannot build the layout".
    const SKIP: i32 = 77;

    /// Each scenario in a child of this binary; see the crate docs.
    pub fn parent() {
        let exe = std::env::current_exe().expect("current_exe");
        let mut planted = Command::new(&exe);
        planted.env(SCENARIO_ENV, "planted");
        start_from_the_default_stack_limit(&mut planted);
        check("planted", planted);

        let mut unrandomized = Command::new(&exe);
        unrandomized.env(SCENARIO_ENV, "no-aslr");
        start_from_the_default_stack_limit(&mut unrandomized);
        // SAFETY: `personality` is async-signal-safe.
        unsafe {
            unrandomized.pre_exec(|| {
                let current = libc::personality(0xffff_ffff);
                if current == -1
                    || libc::personality((current | libc::ADDR_NO_RANDOMIZE) as libc::c_ulong) == -1
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        check("no-aslr", unrandomized);
    }

    /// Exec the child under the usual 8 MiB `RLIMIT_STACK`, which sizes the
    /// kernel's reserve below the stack and which `neomacs` then raises.
    fn start_from_the_default_stack_limit(cmd: &mut Command) {
        // SAFETY: `getrlimit`/`setrlimit` are async-signal-safe.
        unsafe {
            cmd.pre_exec(|| {
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
    }

    fn check(scenario: &str, mut cmd: Command) {
        let output = match cmd.output() {
            Ok(output) => output,
            Err(error) if scenario == "no-aslr" && error.raw_os_error() == Some(libc::EPERM) => {
                eprintln!("skipping {scenario}: cannot disable randomization: {error}");
                return;
            }
            Err(error) => panic!("{scenario}: spawn failed: {error}"),
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: String = {
            let lines: Vec<&str> = stderr.lines().collect();
            lines[lines.len().saturating_sub(20)..].join("\n")
        };
        if output.status.code() == Some(SKIP) {
            eprintln!("skipping {scenario}: {stdout}");
            return;
        }
        if let Some(signal) = output.status.signal() {
            panic!(
                "{scenario}: the main thread died of signal {signal} -- the stack \
                 ended before the guard's red zone.\nstdout:\n{stdout}\nstderr tail:\n{tail}"
            );
        }
        assert!(
            output.status.success(),
            "{scenario}: {:?}\nstdout:\n{stdout}\nstderr tail:\n{tail}",
            output.status
        );
        let line = |key: &str| {
            stdout
                .lines()
                .find_map(|l| l.strip_prefix(key))
                .unwrap_or_else(|| panic!("{scenario}: no `{key}` line in\n{stdout}"))
        };
        assert_eq!(
            line("overflow: "),
            "OK (error \"Bytecode stack overflow\")",
            "{scenario}: {stdout}"
        );
        assert_eq!(line("after: "), "OK 500", "{scenario}: {stdout}");
        line("stacker: ");
        let reached: u64 = line("reached: ").parse().expect("reached is a count");
        assert!(
            reached > 10_000,
            "{scenario}: the recursion ran natively deep: {stdout}"
        );
        eprintln!("{scenario}: {}", stdout.trim().replace('\n', "; "));
    }

    /// The `[stack]` mapping and the end of the highest mapping below it.
    fn stack_and_below() -> Option<((usize, usize), usize)> {
        let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
        let range = |line: &str| {
            let (lo, hi) = line.split_whitespace().next()?.split_once('-')?;
            Some((
                usize::from_str_radix(lo, 16).ok()?,
                usize::from_str_radix(hi, 16).ok()?,
            ))
        };
        let stack = maps
            .lines()
            .filter(|l| l.trim_end().ends_with("[stack]"))
            .find_map(range)?;
        let below = maps
            .lines()
            .filter_map(range)
            .filter(|&(_, hi)| hi <= stack.0)
            .map(|(_, hi)| hi)
            .max()?;
        Some((stack, below))
    }

    /// Map one readable page ending exactly `TARGET_STACK` below the stack's
    /// top: where the loader sits without randomization. Readable, because
    /// the kernel keeps its guard gap only above an accessible mapping.
    fn plant_mapping_at_the_stack_reserve() -> Result<(), String> {
        let ((_, top), _) = stack_and_below().ok_or("no [stack] in /proc/self/maps")?;
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
        let at = top - TARGET_STACK - page;
        // SAFETY: MAP_FIXED_NOREPLACE never replaces an existing mapping.
        let got = unsafe {
            libc::mmap(
                at as *mut libc::c_void,
                page,
                libc::PROT_READ,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_FIXED_NOREPLACE,
                -1,
                0,
            )
        };
        if got == libc::MAP_FAILED {
            let error = std::io::Error::last_os_error();
            // Something is mapped there already (the loader, unrandomized).
            return if error.raw_os_error() == Some(libc::EEXIST) {
                Ok(())
            } else {
                Err(format!("mmap at {at:#x}: {error}"))
            };
        }
        if got as usize != at {
            // SAFETY: the page just mapped, unused.
            unsafe { libc::munmap(got, page) };
            return Err("the kernel took MAP_FIXED_NOREPLACE as a hint".into());
        }
        Ok(())
    }

    /// Descend in frames of 4 KiB until stacker reports under 64 KiB left,
    /// writing every frame: the stacker probes of the Tier-0 interpreter and
    /// the tree walker switch segments at 128 KiB, so the stack stacker
    /// reports must be stack the kernel will grow into.
    #[inline(never)]
    fn descend_to_the_reported_end() -> usize {
        let mut frame = [0u8; 4096];
        std::hint::black_box(&mut frame);
        match stacker::remaining_stack() {
            Some(left) if left >= 64 * 1024 => 1 + descend_to_the_reported_end(),
            _ => 0,
        }
    }

    const PROBE: &[(&str, &str)] = &[
        ("", "(defvar neovm--sg-reached 0)"),
        (
            "",
            "(defun neovm--sg-deep (n)
               (setq neovm--sg-reached n)
               (if (= n 0) 0 (1+ (neovm--sg-deep (1- n)))))",
        ),
        ("", "(byte-compile 'neovm--sg-deep)"),
        ("", "(dotimes (_ 40) (neovm--sg-deep 100))"),
        (
            "overflow: ",
            "(let ((max-lisp-eval-depth most-positive-fixnum))
               (condition-case err (neovm--sg-deep 100000000) (error err)))",
        ),
        ("reached: ", "(- 100000000 neovm--sg-reached)"),
        ("after: ", "(neovm--sg-deep 500)"),
    ];

    /// Runs on this process's main thread: set up the layout, raise the
    /// limit as `neomacs` does, and run the batch probe
    /// (tmp/v10x/probes/j2pf/probe-t11.el) and the stacker descent.
    pub fn child(scenario: &str) -> i32 {
        if scenario == "planted"
            && let Err(why) = plant_mapping_at_the_stack_reserve()
        {
            println!("{why}");
            return SKIP;
        }
        let Some(((_, top), below)) = stack_and_below() else {
            println!("no [stack] in /proc/self/maps");
            return SKIP;
        };
        if top - below > TARGET_STACK {
            // Randomized, and nothing planted: the limit binds, the layout
            // under test is not there.
            println!(
                "the mapping below the stack is {} MiB down",
                (top - below) >> 20
            );
            return SKIP;
        }
        neovm_core::emacs_core::eval::raise_main_stack_rlimit(TARGET_STACK);

        let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap");
        apply_runtime_startup_state(&mut eval).expect("runtime startup");
        eval.set_lexical_binding(true);
        for (label, form) in PROBE {
            let result = eval.eval_str(form);
            let printed = format_eval_result_with_eval(&eval, &result);
            if label.is_empty() {
                assert!(printed.starts_with("OK"), "{form}: {printed}");
            } else if *label == "reached: " {
                println!("reached: {}", printed.trim_start_matches("OK "));
            } else {
                println!("{label}{printed}");
            }
        }
        let frames = descend_to_the_reported_end();
        println!("stacker: {frames} frames of 4 KiB down to its reported end");
        0
    }
}
