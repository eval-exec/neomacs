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

/// Ruling 2's bar: the bytecode-stack overflow depth is IDENTICAL across the
/// execution tiers (interpreter, JIT with direct calls, shim fast path, the
/// forced slow-spec and forced-deopt harnesses) and equal to GNU 31.1's.
const TEST_TIERS: &str = "bytecode_stack_overflow_depth_is_tier_independent";

/// Coverage the 4 MiB bytecode-stack budget does NOT bound: interpreted
/// recursion with a raised `max-lisp-eval-depth` must end in a clean Lisp
/// depth error on the process's main thread, never in a native signal.
const TEST_INTERPRETED: &str = "interpreted_recursion_ends_in_a_depth_error_on_the_main_thread";

/// `neomacs`'s `RLIMIT_STACK` target (`increase_stack_limit`).
const TARGET_STACK: usize = 128 * 1024 * 1024;

/// The exec-time `RLIMIT_STACK` the child inherits (8 MiB): the kernel
/// reserves only this much below the stack, so the loader can sit just
/// below it and the editor's raise clamps there (the cramped layout).
const CRAMPED_RESERVE: usize = 8 * 1024 * 1024;

fn main() {
    #[cfg(target_os = "linux")]
    if let Ok(scenario) = std::env::var(linux::SCENARIO_ENV) {
        std::process::exit(linux::child(&scenario));
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    if flag("--list") {
        if !flag("--ignored") {
            for test in [TEST, TEST_TIERS, TEST_INTERPRETED] {
                println!("{test}: test");
            }
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
    let selected = |test: &str| {
        !flag("--ignored")
            && (filters.is_empty()
                || filters.iter().any(|f| {
                    if flag("--exact") {
                        *f == test
                    } else {
                        test.contains(f)
                    }
                }))
    };
    let mut ran = false;
    #[cfg(target_os = "linux")]
    {
        if selected(TEST) {
            linux::parent();
            ran = true;
        }
        if selected(TEST_TIERS) {
            linux::parent_tiers();
            ran = true;
        }
        if selected(TEST_INTERPRETED) {
            linux::parent_interpreted();
            ran = true;
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = &selected;
    if !ran {
        println!("running 0 tests");
        return;
    }
    for test in [TEST, TEST_TIERS, TEST_INTERPRETED] {
        if selected(test) {
            println!("test {test} ... ok");
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{CRAMPED_RESERVE, TARGET_STACK};
    use neovm_core::emacs_core::format_eval_result_with_eval;
    use neovm_core::emacs_core::load::apply_runtime_startup_state;
    use neovm_core::emacs_core::load_boot::create_bootstrap_evaluator_cached;
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

        // Ruling-1 cramped layout: the kernel's exec reserve is the 8 MiB
        // RLIMIT, so the loader sits just below it and the editor's raise
        // clamps there instead of reaching TARGET_STACK.  Debug frames
        // consume more than the sampled probes' red zone between samples;
        // the child must still end in a clean Lisp error, never a signal.
        // `cramped-jit-off` repeats it with the JIT kill switch on.
        let mut cramped = Command::new(&exe);
        cramped.env(SCENARIO_ENV, "cramped");
        start_from_the_default_stack_limit(&mut cramped);
        check("cramped", cramped);

        let mut cramped_jit_off = Command::new(&exe);
        cramped_jit_off.env(SCENARIO_ENV, "cramped-jit-off");
        cramped_jit_off.env("NEOVM_JIT", "0");
        start_from_the_default_stack_limit(&mut cramped_jit_off);
        check("cramped-jit-off", cramped_jit_off);

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

    /// Ruling 2's bar: every execution tier ends the byte-compiled recursion
    /// at GNU's bytecode-stack depth, with GNU's error. Each tier runs in a
    /// fresh child because the knobs are read once per process.
    pub fn parent_tiers() {
        let exe = std::env::current_exe().expect("current_exe");
        // The forced-deopt tier is NOT run here: ending the recursion at the
        // bytecode budget's depth makes every level deopt into a nested
        // interpreter entry, and in a DEBUG build those frames need more live
        // native stack than the test memory wrapper's 8 GiB address-space
        // limit (the release CLI test covers the deopt tier; release frames
        // are an order of magnitude smaller).
        let tiers: [(&str, &[(&str, &str)]); 4] = [
            ("tier-jit-off", &[("NEOVM_JIT", "0")]),
            ("tier-jit-on", &[]),
            ("tier-direct-call-off", &[("NEOVM_JIT_DIRECT_CALL", "off")]),
            ("tier-slow-spec", &[("NEOVM_JIT_FORCE_SLOW_SPEC", "1")]),
        ];
        // The exact GNU depth is pinned by the CLI test
        // (`crates/neomacs/tests/main_stack_overflow.rs`), whose bootstrap is
        // GNU's `--eval` environment. This in-process evaluator keeps a
        // different set of byte-compiled startup frames live, so the BAR here
        // is that every tier ends at the SAME depth -- and that the depth is
        // set by the bytecode budget, not by the native stack.
        let mut depths: Vec<(&str, u64)> = Vec::new();
        for (scenario, envs) in tiers {
            let mut cmd = Command::new(&exe);
            cmd.env(SCENARIO_ENV, scenario);
            for (key, value) in envs {
                cmd.env(key, value);
            }
            start_from_the_default_stack_limit(&mut cmd);
            if let Some(reached) = check_tier(scenario, cmd) {
                depths.push((scenario, reached));
            }
        }
        let Some(&(first_scenario, first)) = depths.first() else {
            return;
        };
        for &(scenario, reached) in &depths[1..] {
            assert_eq!(
                reached, first,
                "{scenario} reached {reached} but {first_scenario} reached {first}: \
                 the bytecode-stack depth must be tier-independent"
            );
        }
        eprintln!(
            "all {} tiers reached the bytecode-stack depth {first}",
            depths.len()
        );
    }

    /// The coverage the 4 MiB budget does NOT bound: interpreted recursion
    /// with a raised `max-lisp-eval-depth` on the main thread ends in GNU's
    /// Lisp depth error, never a native signal.
    pub fn parent_interpreted() {
        let exe = std::env::current_exe().expect("current_exe");
        let mut cmd = Command::new(&exe);
        cmd.env(SCENARIO_ENV, "interpreted");
        start_from_the_default_stack_limit(&mut cmd);
        check_interpreted(cmd);
    }

    fn check_tier(scenario: &str, cmd: Command) -> Option<u64> {
        let (stdout, _stderr) = run_child(scenario, cmd)?;
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
        let reached: u64 = line("reached: ").parse().expect("reached is a count");
        assert!(
            reached > 10_000,
            "{scenario}: the bytecode stack, not the native stack, ended the \
             recursion: {stdout}"
        );
        eprintln!(
            "{scenario}: reached {reached}; {}",
            stdout.trim().replace('\n', "; ")
        );
        Some(reached)
    }

    fn check_interpreted(cmd: Command) {
        let scenario = "interpreted";
        let Some((stdout, _stderr)) = run_child(scenario, cmd) else {
            return;
        };
        let line = |key: &str| {
            stdout
                .lines()
                .find_map(|l| l.strip_prefix(key))
                .unwrap_or_else(|| panic!("{scenario}: no `{key}` line in\n{stdout}"))
        };
        assert!(
            line("overflow: ").starts_with("OK (excessive-lisp-nesting "),
            "{scenario}: expected a clean Lisp depth error: {stdout}"
        );
        assert_eq!(line("after: "), "OK 500", "{scenario}: {stdout}");
        eprintln!("{scenario}: {}", stdout.trim().replace('\n', "; "));
    }

    /// Run one child; SKIP exits are reported as test skips (`None`), signals
    /// as failures (the regression this target exists to catch).
    fn run_child(scenario: &str, mut cmd: Command) -> Option<(String, String)> {
        let output = match cmd.output() {
            Ok(output) => output,
            Err(error) => panic!("{scenario}: spawn failed: {error}"),
        };
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let tail: String = {
            let lines: Vec<&str> = stderr.lines().collect();
            lines[lines.len().saturating_sub(20)..].join("\n")
        };
        if output.status.code() == Some(SKIP) {
            eprintln!("skipping {scenario}: {stdout}");
            return None;
        }
        if let Some(signal) = output.status.signal() {
            panic!(
                "{scenario}: the main thread died of signal {signal} -- the stack \
                 ended before the Lisp error.\nstdout:\n{stdout}\nstderr tail:\n{tail}"
            );
        }
        assert!(
            output.status.success(),
            "{scenario}: {:?}\nstdout:\n{stdout}\nstderr tail:\n{tail}",
            output.status
        );
        Some((stdout, stderr))
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
        let overflow = line("overflow: ");
        if scenario.starts_with("cramped") {
            // The cramped layout's probe is finite: the depth limit must be
            // reached on grown segments, not the thread stack's end.
            assert!(
                overflow.starts_with("OK (error \"Lisp nesting exceeds"),
                "{scenario}: expected a clean depth error: {stdout}"
            );
        } else {
            assert_eq!(
                overflow, "OK (error \"Bytecode stack overflow\")",
                "{scenario}: {stdout}"
            );
        }
        assert_eq!(line("after: "), "OK 500", "{scenario}: {stdout}");
        if !scenario.starts_with("cramped") {
            line("stacker: ");
        }
        let reached: u64 = line("reached: ").parse().expect("reached is a count");
        // The exact GNU depth is pinned by the CLI test (the `--eval`
        // environment GNU was measured in); this harness's in-process
        // bootstrap keeps a different set of startup frames charged.
        assert!(
            reached > 10_000,
            "{scenario}: the recursion was ended by the bytecode budget, not \
             the native stack: {stdout}"
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

    /// Map one readable page ending exactly `distance` below the stack's
    /// top: where the loader sits without randomization (TARGET_STACK) or
    /// where the 8 MiB exec reserve puts it (CRAMPED_RESERVE). Readable,
    /// because the kernel keeps its guard gap only above an accessible
    /// mapping.
    fn plant_mapping_below_the_stack(distance: usize) -> Result<(), String> {
        let ((_, top), _) = stack_and_below().ok_or("no [stack] in /proc/self/maps")?;
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
        let at = top - distance - page;
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

    /// The cramped scenarios' probe: a byte-compiled recursion under a
    /// finite `max-lisp-eval-depth`, so the growth itself is what is under
    /// test (the deep probe's unbounded depth would keep growing segments
    /// forever with the JIT off).  Expected: the clean depth error, after
    /// the stack has been grown far past the cramped main stack.
    const CRAMPED_PROBE: &[(&str, &str)] = &[
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
            "(let ((max-lisp-eval-depth 40000))
               (condition-case err (neovm--sg-deep 40000) (error err)))",
        ),
        ("reached: ", "(- 40000 neovm--sg-reached)"),
        ("after: ", "(neovm--sg-deep 500)"),
    ];

    /// The #92 coverage: interpreted (NOT byte-compiled) recursion, which the
    /// 4 MiB bytecode-stack budget does not bound. A raised but finite
    /// `max-lisp-eval-depth` must end it in the clean Lisp depth error on this
    /// thread, in debug and release alike.
    const INTERPRETED_PROBE: &[(&str, &str)] = &[
        ("", "(defvar neovm--sg-reached 0)"),
        (
            "",
            "(defun neovm--sg-walk (n)
               (setq neovm--sg-reached n)
               (if (= n 0) 0 (1+ (neovm--sg-walk (1- n)))))",
        ),
        ("", "(dotimes (_ 40) (neovm--sg-walk 100))"),
        (
            "overflow: ",
            "(let ((max-lisp-eval-depth 300000))
               (condition-case err (neovm--sg-walk 400000) (error err)))",
        ),
        ("after: ", "(neovm--sg-walk 500)"),
    ];

    /// Runs on this process's main thread: set up the layout, raise the
    /// limit as `neomacs` does, and run the batch probe
    /// (tmp/v10x/probes/j2pf/probe-t11.el) and the stacker descent.
    pub fn child(scenario: &str) -> i32 {
        let planted = match scenario {
            "planted" => Some(TARGET_STACK),
            // The cramped layout: force the mapping below the stack to sit
            // at the exec-time 8 MiB reserve, so the editor's raise clamps.
            "cramped" | "cramped-jit-off" => Some(CRAMPED_RESERVE),
            _ => None,
        };
        if let Some(distance) = planted
            && let Err(why) = plant_mapping_below_the_stack(distance)
        {
            println!("{why}");
            return SKIP;
        }
        let lisp_limit_scenario = scenario.starts_with("tier-") || scenario == "interpreted";
        if lisp_limit_scenario {
            // These pin Lisp-visible limits (GNU's bytecode-stack budget and
            // the depth limit), which are independent of the native layout;
            // the same `RLIMIT_STACK` policy as `neomacs` still applies.
            neovm_core::emacs_core::eval::raise_main_stack_rlimit(TARGET_STACK);
        } else {
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
        }

        let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap");
        apply_runtime_startup_state(&mut eval).expect("runtime startup");
        eval.set_lexical_binding(true);
        let probe: &[(&str, &str)] = if scenario.starts_with("cramped") {
            CRAMPED_PROBE
        } else if scenario == "interpreted" {
            INTERPRETED_PROBE
        } else {
            PROBE
        };
        for (label, form) in probe {
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
        // The descent proves stacker's reported end is real stack for the
        // TARGET_STACK layout.  The cramped layout deliberately reports an
        // end past where the clamped stack actually stops, so descending to
        // it would fault in the harness, not in the product; skip it there.
        if !scenario.starts_with("cramped")
            && !scenario.starts_with("tier-")
            && scenario != "interpreted"
        {
            let frames = descend_to_the_reported_end();
            println!("stacker: {frames} frames of 4 KiB down to its reported end");
        }
        0
    }
}
