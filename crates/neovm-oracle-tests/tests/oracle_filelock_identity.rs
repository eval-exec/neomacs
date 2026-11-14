//! Filename handlers must receive the actual argument or buffer slot object.
//!
//! Source contract: GNU a360712 filelock.c:Flock_file/Funlock_file/
//! Ffile_locked_p pass their Lisp_Object through, while buffer.c and
//! insdel.c supply the buffer's file_truename slot. Recorded expectations
//! must come from printed, attested GNU results; none are guessed here.
//!
//! The GC forms deliberately retain an independent binding to the original
//! string. They cover identity, mutation and property survival, not exclusive
//! ownership of a GC root. Native fallback, error routing, canonical expansion
//! and write-region lockname selection are separate coverage obligations.

use crate::common::{
    assert_oracle_parity, oracle_mode_runs_live_gnu, return_if_neovm_enable_oracle_proptest_not_set,
    run_neovm_eval, run_oracle_eval,
};

fn compare_filename_identity(name: &str, form: &str) {
    return_if_neovm_enable_oracle_proptest_not_set!();
    if !oracle_mode_runs_live_gnu() {
        // The existing parity helper runs Neomacs only in snapshot mode.
        // This is not evidence of GNU compatibility; verify/live is required.
        assert_oracle_parity(form);
        return;
    }
    let oracle = run_oracle_eval(form).expect("attested GNU filename form should run");
    let neomacs = run_neovm_eval(form).expect("Neomacs filename form should run");
    eprintln!("{name} GNU -> {oracle}");
    eprintln!("{name} Neomacs -> {neomacs}");
    assert_eq!(neomacs, oracle, "filename identity mismatch in {name}\n{form}");
}

#[test]
fn direct_handler_identity_control() {
    compare_filename_identity(
        "direct_handler_identity_control",
        include_str!("fixtures/filelock_identity/direct-handler-control.el"),
    );
}

#[test]
fn lock_file_plain_argument_identity() {
    compare_filename_identity(
        "lock_file_plain_argument_identity",
        include_str!("fixtures/filelock_identity/lock-file-plain.el"),
    );
}

#[test]
fn lock_file_propertized_argument_identity_under_gc() {
    compare_filename_identity(
        "lock_file_propertized_argument_identity_under_gc",
        include_str!("fixtures/filelock_identity/lock-file-propertized-gc.el"),
    );
}

#[test]
fn unlock_file_plain_argument_identity() {
    compare_filename_identity(
        "unlock_file_plain_argument_identity",
        include_str!("fixtures/filelock_identity/unlock-file-plain.el"),
    );
}

#[test]
fn unlock_file_propertized_argument_identity_under_gc() {
    compare_filename_identity(
        "unlock_file_propertized_argument_identity_under_gc",
        include_str!("fixtures/filelock_identity/unlock-file-propertized-gc.el"),
    );
}

#[test]
fn file_locked_p_plain_argument_identity() {
    compare_filename_identity(
        "file_locked_p_plain_argument_identity",
        include_str!("fixtures/filelock_identity/file-locked-p-plain.el"),
    );
}

#[test]
fn file_locked_p_propertized_argument_identity_under_gc() {
    compare_filename_identity(
        "file_locked_p_propertized_argument_identity_under_gc",
        include_str!("fixtures/filelock_identity/file-locked-p-propertized-gc.el"),
    );
}

#[test]
fn first_modification_uses_buffer_file_truename_object() {
    compare_filename_identity(
        "first_modification_uses_buffer_file_truename_object",
        include_str!("fixtures/filelock_identity/first-modification.el"),
    );
}

#[test]
fn unlock_buffer_uses_buffer_file_truename_object() {
    compare_filename_identity(
        "unlock_buffer_uses_buffer_file_truename_object",
        include_str!("fixtures/filelock_identity/unlock-buffer.el"),
    );
}

#[test]
fn kill_buffer_unlock_uses_target_buffer_file_truename_object() {
    compare_filename_identity(
        "kill_buffer_unlock_uses_target_buffer_file_truename_object",
        include_str!("fixtures/filelock_identity/kill-buffer-unlock.el"),
    );
}

#[test]
fn set_buffer_modified_p_uses_buffer_file_truename_object() {
    compare_filename_identity(
        "set_buffer_modified_p_uses_buffer_file_truename_object",
        include_str!("fixtures/filelock_identity/set-buffer-modified-p.el"),
    );
}
