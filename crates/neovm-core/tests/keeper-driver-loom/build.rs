//! Copies the product keeper-driver core and ring control into OUT_DIR,
//! read-only, plus one-edge mutated copies for the negative rows (L6.7 K9).
//!
//! `parallel_entry/driver_core.rs` is the driver's whole protocol, generic
//! over `KeeperOps`/`Batch`; its only synchronization is `super::sync`, which
//! the model's `tests/sync.rs` supplies. `rendezvous/control.rs` is the ring
//! the model's era drives, re-anchored as `pentry-l6-loom` does.
//!
//! Include-hostile regions are stripped: the leading `//!` block and inner
//! `#![...]` attributes (E0753 in an `include!`), and control.rs's trailing
//! `cfg(test)` module. Every mutation names its anchor and must match it
//! exactly once, so a moved line fails the build instead of a silent no-op.
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const CORE: &str = "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/driver_core.rs";
const CONTROL: &str =
    "../../../neovm-value/src/tagged/gc/mutator_registration/rendezvous/control.rs";

fn main() {
    println!("cargo:rerun-if-changed={CORE}");
    println!("cargo:rerun-if-changed={CONTROL}");
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR set by cargo"));

    let core = strip_header(&fs::read_to_string(CORE).expect("read driver_core.rs"));
    for forbidden in [
        "crate::",
        "super::super",
        "std::sync",
        "std::thread",
        "std::time::Instant",
    ] {
        assert!(
            !core.contains(forbidden),
            "driver_core.rs names `{forbidden}`: the core must reach synchronization and \
             its clock only through super::sync"
        );
    }
    for expected in [
        "use super::sync::{self, mpsc};",
        "pub(crate) trait KeeperOps",
        "pub(crate) trait Batch<O: KeeperOps>",
        "pub(crate) struct Driver<'o, O: KeeperOps>",
    ] {
        assert!(core.contains(expected), "driver_core.rs lost `{expected}`");
    }
    write(&out, "driver_core.rs", &core);
    // N-K8: a pending stop poisons, as K8 r1 did.
    write(
        &out,
        "driver_core_pending_poisons.rs",
        &mutate(
            &core,
            "                Ok(Attempt::Pending(pending)) => {\n",
            "                Ok(Attempt::Pending(pending)) => {\n                    return Err(Breach::SelfWait);\n",
        ),
    );
    // N-L8: the poison fan-out tells nobody.
    let silent = mutate(
        &core,
        "            let _ = self.send(owner, GrantMsg::Poisoned);",
        "            let _ = owner;",
    );
    let silent = mutate(
        &silent,
        "            let _ = enrolled.grants.try_send(GrantMsg::Poisoned);",
        "            let _ = enrolled;",
    );
    write(
        &out,
        "driver_core_silent_poison.rs",
        &mutate(
            &silent,
            "            let _ = group.grants.try_send(GroupMsg::Poisoned);",
            "            let _ = group;",
        ),
    );

    // N-L7: the grant goes to another enrolled owner.
    write(
        &out,
        "driver_core_misrouted_grant.rs",
        &mutate(
            &core,
            "        if self.send(owner, GrantMsg::Grant { ticket, epoch }).is_err() {",
            "        let other = self.owners.keys().copied().find(|&other| other != owner);\n        \
             if self.send(other.unwrap_or(owner), GrantMsg::Grant { ticket, epoch }).is_err() {",
        ),
    );

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
    // N-L13: the collecting group goes back at the TAIL, behind the owners
    // queued during its interval.
    write(
        &out,
        "control_requeue_tail.rs",
        &mutate(
            &control,
            "        self.serial_ring.push_front(RingEntry {",
            "        self.serial_ring.push_back(RingEntry {",
        ),
    );
    // N-L1: the ring commits at the head, so it grants newest first.
    write(
        &out,
        "control_lifo.rs",
        &mutate(
            &control,
            "        self.serial_ring.push_back(RingEntry {",
            "        self.serial_ring.push_front(RingEntry {",
        ),
    );
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

/// Drop the leading `//!` block and inner attributes.
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
