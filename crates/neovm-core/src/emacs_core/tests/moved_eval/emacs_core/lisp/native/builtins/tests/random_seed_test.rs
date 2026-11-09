//! The `random' generator is seeded from the OS in every process.
//!
//! GNU `main` calls `init_random ()` on every startup, dumped image or not
//! (src/emacs.c:2020), and `init_random` seeds libc's `random ()` from
//! `getrandom`, falling back to the PID and the time only when that fails
//! (src/sysdep.c:2225-2249).  A generator nobody seeds answers POSIX's
//! `srandom (1)` sequence in every process, so `server-generate-key'
//! (lisp/server.el:606-617) handed every `server-use-tcp' server the same
//! key, and any local process could compute it.
//!
//! What is mostly under test is a fact about fresh processes, so those tests
//! re-run themselves as a child probe: the probe's draws are the first ones
//! the child's generator makes.

use crate::emacs_core::{Context, format_eval_result};
use std::process::Command;

const PROBE_ENV: &str = "NEOVM_RANDOM_SEED_PROBE";
const PROBE_MARKER: &str = "RANDOM-SEED-PROBE ";

/// The generator is process-wide: where tests share a process (plain
/// `cargo test`), the sequences drawn in this one must not interleave.
static IN_PROCESS_DRAWS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The draws `server-generate-key' makes: 64 times `(+ 33 (random 94))'.
const SERVER_KEY_DRAWS: &str = "(let ((n 0) (key nil))
                                  (while (< n 64)
                                    (setq key (cons (+ 33 (random 94)) key)
                                          n (1+ n)))
                                  (concat (nreverse key)))";

fn eval_one(ev: &mut Context, src: &str) -> String {
    format_eval_result(&ev.eval_str(src))
}

fn is_probe() -> bool {
    std::env::var_os(PROBE_ENV).is_some()
}

/// Run test NAME of this module alone, in a fresh process, as a probe, and
/// return the line its probe printed.
fn probe_in_fresh_process(name: &str) -> String {
    let module = module_path!();
    let module = module.split_once("::").map_or(module, |(_, rest)| rest);
    let exe = std::env::current_exe().expect("test binary path");
    let output = Command::new(exe)
        .arg("--exact")
        .arg(format!("{module}::{name}"))
        .arg("--nocapture")
        .env(PROBE_ENV, "1")
        .output()
        .expect("spawn random probe child");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "probe child failed: {:?}\nstdout: {stdout}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(PROBE_MARKER))
        .unwrap_or_else(|| panic!("probe child printed no probe line:\n{stdout}"))
        .to_string()
}

#[test]
fn fresh_processes_do_not_draw_the_same_server_key() {
    crate::test_utils::init_test_tracing();
    const NAME: &str = "fresh_processes_do_not_draw_the_same_server_key";
    if is_probe() {
        let mut ev = crate::test_utils::new_context();
        println!("{PROBE_MARKER}{}", eval_one(&mut ev, SERVER_KEY_DRAWS));
        return;
    }
    let first = probe_in_fresh_process(NAME);
    let second = probe_in_fresh_process(NAME);
    assert!(first.starts_with("OK \""), "probe result: {first}");
    assert!(second.starts_with("OK \""), "probe result: {second}");
    assert_ne!(
        first, second,
        "two fresh processes drew the same `server-generate-key' sequence: the \
         generator was never seeded (GNU init_random, src/emacs.c:2020, \
         src/sysdep.c:2225-2249)"
    );
}

/// `(random STRING)' is a deterministic seed (GNU `Frandom`, src/fns.c:90-91;
/// `seed_random`, src/sysdep.c:2213-2222).  It must stay one when it is the
/// first use of the generator in a process and after `(random t)' reseeded
/// it: seeding a fresh process from the OS must never replace a seed Lisp
/// chose.
#[test]
fn string_seed_repeats_its_sequence_in_any_process() {
    crate::test_utils::init_test_tracing();
    const NAME: &str = "string_seed_repeats_its_sequence_in_any_process";
    let seeded = format!("(progn (random \"neovm-random-seed-probe\") {SERVER_KEY_DRAWS})");
    let mut ev = crate::test_utils::new_context();
    if is_probe() {
        println!("{PROBE_MARKER}{}", eval_one(&mut ev, &seeded));
        return;
    }
    let first_use = probe_in_fresh_process(NAME);
    assert!(first_use.starts_with("OK \""), "probe result: {first_use}");
    let _serial = IN_PROCESS_DRAWS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let here = eval_one(&mut ev, &seeded);
    assert_eq!(eval_one(&mut ev, "(integerp (random t))"), "OK t");
    eval_one(&mut ev, SERVER_KEY_DRAWS);
    let after_reseed = eval_one(&mut ev, &seeded);
    assert_eq!(
        first_use, here,
        "a string seed must not depend on the process"
    );
    assert_eq!(
        here, after_reseed,
        "a string seed must not depend on an earlier `(random t)'"
    );
}

/// `(random t)' seeds from the OS (GNU `Frandom`, src/fns.c:88-89), so the
/// draws after it never replay a string seed's sequence; and two strings
/// that fold to different seeds draw different sequences (`seed_random`
/// XORs the bytes into one `unsigned int`, src/sysdep.c:2213-2222).
#[test]
fn entropy_or_another_string_replaces_a_string_seed() {
    crate::test_utils::init_test_tracing();
    let _serial = IN_PROCESS_DRAWS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut ev = crate::test_utils::new_context();
    let mut draws_after = |prefix: &str| {
        let result = eval_one(&mut ev, &format!("(progn {prefix} {SERVER_KEY_DRAWS})"));
        assert!(
            result.starts_with("OK \""),
            "draws after {prefix}: {result}"
        );
        result
    };
    let seeded = draws_after(r#"(random "neovm-random-seed-probe")"#);
    let other_seed = draws_after(r#"(random "neovm-random-seed-probe-2")"#);
    assert_ne!(
        seeded, other_seed,
        "two different string seeds drew the same sequence"
    );
    // Without a reseed, the state after the string seed and the draw of
    // `(random t)' is the same both times, and so are the draws after it.
    let reseeded = draws_after(r#"(random "neovm-random-seed-probe") (random t)"#);
    let reseeded_again = draws_after(r#"(random "neovm-random-seed-probe") (random t)"#);
    assert_ne!(
        reseeded, reseeded_again,
        "the draws after `(random t)' replayed the string seed's sequence: \
         `(random t)' did not reseed the generator from the OS"
    );
}
