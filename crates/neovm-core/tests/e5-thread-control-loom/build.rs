//! Copies the product E5.3/E5.4 thread-control files into OUT_DIR, read-only,
//! plus one-edge mutated copies for the negative rows (E5.4b).
//!
//! Product files copied verbatim (with include-hostile headers/imports
//! re-anchored):
//! - `green/scheduler/wait_core.rs`: `OwnerWake`, `SignalHint`,
//!   `ParallelParticipant`, `Participant` (only sync is `super::sync`).
//! - `green/condition_wait.rs`: `ValidatedWait`, `ConditionWait`,
//!   `ConditionSeat`, `WaitOwner`, `wait_with_seat`.
//! - `parallel_entry/driver_core.rs`: keeper driver protocol (`Driver`,
//!   `KeeperOps`, `Batch`, `KeeperMsg::BatonRequest`, `await_return`).
//! - `rendezvous/control.rs`: FIFO serial ring (`enqueue_serial`,
//!   `enqueue_baton_group`, `grant_head_in_stop`).
//!
//! Every mutation names an anchor and asserts it matches exactly once so a
//! moved product line fails the build loudly instead of silently turning a
//! negative row into a no-op.
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const WAIT_CORE: &str =
    "../../../neovm-eval/src/emacs_core/runtime/eval/green/scheduler/wait_core.rs";
const CONDITION_WAIT: &str =
    "../../../neovm-eval/src/emacs_core/runtime/eval/green/condition_wait.rs";
const DRIVER_CORE: &str =
    "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/driver_core.rs";
const CONTROL: &str =
    "../../../neovm-value/src/tagged/gc/mutator_registration/rendezvous/control.rs";

fn main() {
    println!("cargo:rerun-if-changed={WAIT_CORE}");
    println!("cargo:rerun-if-changed={CONDITION_WAIT}");
    println!("cargo:rerun-if-changed={DRIVER_CORE}");
    println!("cargo:rerun-if-changed={CONTROL}");
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR set by cargo"));

    // 1. green/scheduler/wait_core.rs
    let wait_core = strip_header(&fs::read_to_string(WAIT_CORE).expect("read wait_core.rs"));
    for forbidden in ["crate::", "super::super", "std::sync", "std::thread"] {
        assert!(
            !wait_core.contains(forbidden),
            "wait_core.rs names `{forbidden}`: wait_core must reach synchronization only through super::sync"
        );
    }
    for expected in [
        "use super::sync::{Arc, Condvar, Mutex, PoisonError, atomic, mpsc};",
        "pub(crate) struct OwnerWake",
        "pub(crate) struct SignalHint",
        "pub(crate) struct ParallelParticipant",
        "pub(crate) enum Participant<W>",
    ] {
        assert!(
            wait_core.contains(expected),
            "wait_core.rs lost `{expected}`"
        );
    }
    write(&out, "wait_core.rs", &wait_core);
    // Negative (a): OwnerWake::wake notifies without recording `*woken = true`,
    // breaking wake-before-park durability.
    write(
        &out,
        "wait_core_drop_wake_flag.rs",
        &mutate(
            &wait_core,
            "        *woken = true;\n        cvar.notify_one();\n",
            "        let _ = &mut *woken;\n        cvar.notify_one();\n",
        ),
    );

    // 2. green/condition_wait.rs
    let condition_wait_raw =
        strip_header(&fs::read_to_string(CONDITION_WAIT).expect("read condition_wait.rs"));
    for expected in [
        "pub(crate) struct ValidatedWait",
        "pub(crate) enum ConditionWait",
        "pub(crate) trait ConditionSeat",
        "pub(crate) fn wait_with_seat<S: ConditionSeat>",
    ] {
        assert!(
            condition_wait_raw.contains(expected),
            "condition_wait.rs lost `{expected}`"
        );
    }
    let condition_wait = condition_wait_raw
        .replace(
            "use crate::emacs_core::eval::Context;",
            "use super::cw_env::Context;",
        )
        .replace("use crate::emacs_core::threads::{", "use super::cw_env::{")
        .replace(
            "use neovm_value::error::{EvalResult, Flow, signal};",
            "use super::cw_env::{EvalResult, Flow, signal};",
        )
        .replace(
            "use neovm_value::heap_types::threading::RecursionDepth;",
            "use super::cw_env::RecursionDepth;",
        )
        .replace(
            "use neovm_value::value::Value;",
            "use super::cw_env::Value;",
        )
        .replace("std::sync::Arc::clone", "crate::sync::Arc::clone")
        .replace(
            "static_assertions::assert_not_impl_any!(ConditionWait: Send, Sync, Clone, Copy);",
            "// static_assertions checked in neovm-eval",
        );
    for forbidden in ["crate::emacs_core", "neovm_value::", "static_assertions::"] {
        assert!(
            !condition_wait.contains(forbidden),
            "condition_wait.rs still names `{forbidden}` after re-anchoring"
        );
    }
    write(&out, "condition_wait.rs", &condition_wait);
    // Negative (b)-1: wait_with_seat delivers pending thread-signal BEFORE
    // reacquiring the mutex at saved recursive depth (breaks GNU thread.c:470-475).
    write(
        &out,
        "condition_wait_signal_before_relock.rs",
        &mutate(
            &condition_wait,
            "    owner.start_reacquiring()?;\n",
            "    owner.seat.deliver_signal(owner.context, owner.key)?;\n    owner.start_reacquiring()?;\n",
        ),
    );
    // Negative (b)-2: wait_with_seat omits wake_mutex_waiters after unlocking
    // the associated mutex, leaving a mutex contender parked forever.
    write(
        &out,
        "condition_wait_drop_wake_mutex.rs",
        &mutate(
            &condition_wait,
            "    owner\n        .seat\n        .wake_mutex_waiters(owner.context, MutexId::new(waiting.mutex.native_key()))?;\n",
            "    let _ = &waiting;\n",
        ),
    );

    // 3. parallel_entry/driver_core.rs
    let driver_core = strip_header(&fs::read_to_string(DRIVER_CORE).expect("read driver_core.rs"));
    for forbidden in [
        "crate::",
        "super::super",
        "std::sync",
        "std::thread",
        "std::time::Instant",
    ] {
        assert!(
            !driver_core.contains(forbidden),
            "driver_core.rs names `{forbidden}`: must reach synchronization only through super::sync"
        );
    }
    for expected in [
        "use super::sync::{self, mpsc};",
        "pub(crate) trait KeeperOps",
        "pub(crate) trait Batch<O: KeeperOps>",
        "pub(crate) struct Driver<'o, O: KeeperOps>",
    ] {
        assert!(
            driver_core.contains(expected),
            "driver_core.rs lost `{expected}`"
        );
    }
    write(&out, "driver_core.rs", &driver_core);

    // 4. rendezvous/control.rs
    let control = strip_trailing_test_mod(&strip_header(
        &fs::read_to_string(CONTROL).expect("read control.rs"),
    ))
    .replace("pub(in crate::tagged::gc)", "pub(crate)")
    .replace("use super::super::MutatorKey;", "use super::MutatorKey;");
    for forbidden in ["std::sync", "crate::", "super::super"] {
        assert!(
            !control.contains(forbidden),
            "control.rs names `{forbidden}`: re-anchor the ring seam"
        );
    }
    write(&out, "control.rs", &control);
}

fn write(out: &Path, name: &str, text: &str) {
    fs::write(out.join(name), text).expect("write a model copy");
}

fn mutate(text: &str, anchor: &str, replacement: &str) -> String {
    assert_eq!(
        text.matches(anchor).count(),
        1,
        "mutation anchor `{anchor}` must match exactly once"
    );
    text.replacen(anchor, replacement, 1)
}

fn strip_header(original: &str) -> String {
    let mut stripped = String::with_capacity(original.len());
    let mut past_header = false;
    for line in original.lines() {
        if !past_header && (line.starts_with("//!") || line.starts_with("#![")) {
            continue;
        }
        past_header = true;
        stripped.push_str(line);
        stripped.push('\n');
    }
    stripped
}

fn strip_trailing_test_mod(text: &str) -> String {
    const DECLARATION: &str = "#[cfg(test)]\n#[path = \"control_tests.rs\"]\nmod tests;";
    let trimmed = text.trim_end_matches('\n');
    let cut = trimmed
        .strip_suffix(DECLARATION)
        .unwrap_or_else(|| panic!("control.rs must end with its cfg(test) tests module"));
    format!("{cut}\n")
}
