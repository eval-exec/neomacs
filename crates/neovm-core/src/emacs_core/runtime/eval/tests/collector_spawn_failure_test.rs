//! A failed collector start must collect inline, without aborting the editor.
//! The P11 soak failed under an inherited 8 GiB RLIMIT_AS: pthread stacks
//! reserve virtual addresses. This Linux child records all limits and verifies
//! EAGAIN with the same default stack size before requesting its first GC.
//! GNU 31.1 performs garbage_collect synchronously (alloc.c:5779, :5994).
//! The independent make-thread/error row ships with cired2 SpawnRefusal.

use crate::emacs_core::eval::{Context, ThreadingMode};

const PROBE_ENV: &str = "NEOVM_COLLECTOR_SPAWN_FAILURE_PROBE";
const COLLECTOR_TEST: &str = "emacs_core::eval::green::collector_spawn_failure_tests::a_collector_thread_that_cannot_start_does_not_abort";

/// Constrain only this Linux child. Reserving the default 128 MiB stack must
/// exceed its remaining address space, while normal marking has 64 MiB of
/// headroom. NPROC is deliberately unchanged: P11 failed on stack VA, not on
/// the user's task count. The typed injection tests cover other platforms.
#[cfg(target_os = "linux")]
fn bound_collector_address_space() {
    eprintln!(
        "GCSPAWN limits before:\n{}",
        std::fs::read_to_string("/proc/self/limits").unwrap()
    );
    let statm = std::fs::read_to_string("/proc/self/statm").unwrap();
    let pages: u64 = statm.split_whitespace().next().unwrap().parse().unwrap();
    // SAFETY: sysconf reads the process page-size constant.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    assert!(page_size > 0);
    let bytes = pages * page_size as u64 + 64 * 1024 * 1024;
    let limit = libc::rlimit {
        rlim_cur: bytes,
        rlim_max: bytes,
    };
    // SAFETY: valid per-process limit pointer; this isolated child exits.
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit) }, 0);
    eprintln!(
        "GCSPAWN limits during:\n{}",
        std::fs::read_to_string("/proc/self/limits").unwrap()
    );
    let result = std::thread::Builder::new().spawn(|| ());
    assert!(
        result.is_err(),
        "the bounded child must reject a default-stack thread"
    );
    eprintln!(
        "GCSPAWN verified thread-start failure: {}",
        result.unwrap_err()
    );
}

fn child_probe() -> ! {
    // Finish startup before imposing a limit on the first collector request.
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    assert!(!crate::tagged::gc::collector_is_registered(
        context.tagged_heap.heap_identity()
    ));
    bound_collector_address_space();
    match context.eval_str("(progn (garbage-collect) \"GC-COMPLETE\")") {
        Ok(value) => {
            let text = crate::emacs_core::print::print_value(&value);
            println!("{text}");
            std::process::exit(if text.contains("GC-COMPLETE") { 0 } else { 3 });
        }
        Err(error) => {
            eprintln!("uncaught eval error: {error:?}");
            std::process::exit(4);
        }
    }
}

#[test]
fn a_collector_thread_that_cannot_start_does_not_abort() {
    if std::env::var_os(PROBE_ENV).is_some() {
        child_probe();
    }
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", COLLECTOR_TEST, "--nocapture"])
        .env(PROBE_ENV, "child")
        .env("RUST_MIN_STACK", "134217728")
        .output()
        .expect("spawn collector probe child");
    let stdout = String::from_utf8_lossy(&child.stdout);
    let stderr = String::from_utf8_lossy(&child.stderr);
    eprintln!("GCSPAWN child receipt:\n{stderr}");
    assert!(
        child.status.success() && stdout.contains("GC-COMPLETE"),
        "a refused collector start must complete collection inline.\nchild status: {:?}\nstdout: {stdout}\nstderr: {stderr}",
        child.status,
    );
}
