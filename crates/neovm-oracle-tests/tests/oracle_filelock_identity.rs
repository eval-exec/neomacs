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

fn compare_filename_identity(name: &str, form: &str, recorded_gnu: Option<&str>) {
    return_if_neovm_enable_oracle_proptest_not_set!();
    if !oracle_mode_runs_live_gnu() {
        if let Some(recorded) = recorded_gnu {
            let neomacs = run_neovm_eval(form).expect("Neomacs filename form should run");
            assert_eq!(neomacs, recorded, "recorded GNU filename contract in {name}");
        } else {
            // New dependency probes have no guessed expectation. Only an
            // attested verify/live comparison establishes their contract.
            assert_oracle_parity(form);
        }
        return;
    }
    let oracle = run_oracle_eval(form).expect("attested GNU filename form should run");
    if let Some(recorded) = recorded_gnu {
        assert_eq!(oracle, recorded, "attested GNU contract changed in {name}");
    }
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
        Some(r#"OK (handler-token (t t t t) 88 yes)"#),
    );
}

#[test]
fn lock_file_plain_argument_identity() {
    compare_filename_identity(
        "lock_file_plain_argument_identity",
        include_str!("fixtures/filelock_identity/lock-file-plain.el"),
        Some(r#"OK (handler-token (t t t t) 88 yes)"#),
    );
}

#[test]
fn lock_file_propertized_argument_identity_under_gc() {
    compare_filename_identity(
        "lock_file_propertized_argument_identity_under_gc",
        include_str!("fixtures/filelock_identity/lock-file-propertized-gc.el"),
        Some(r#"OK (handler-token (t t t t) 88 yes)"#),
    );
}

#[test]
fn unlock_file_plain_argument_identity() {
    compare_filename_identity(
        "unlock_file_plain_argument_identity",
        include_str!("fixtures/filelock_identity/unlock-file-plain.el"),
        Some(r#"OK (nil (t t t t) 88 yes)"#),
    );
}

#[test]
fn unlock_file_propertized_argument_identity_under_gc() {
    compare_filename_identity(
        "unlock_file_propertized_argument_identity_under_gc",
        include_str!("fixtures/filelock_identity/unlock-file-propertized-gc.el"),
        Some(r#"OK (nil (t t t t) 88 yes)"#),
    );
}

#[test]
fn file_locked_p_plain_argument_identity() {
    compare_filename_identity(
        "file_locked_p_plain_argument_identity",
        include_str!("fixtures/filelock_identity/file-locked-p-plain.el"),
        Some(r#"OK (handler-token (t t t t) 88 yes)"#),
    );
}

#[test]
fn file_locked_p_propertized_argument_identity_under_gc() {
    compare_filename_identity(
        "file_locked_p_propertized_argument_identity_under_gc",
        include_str!("fixtures/filelock_identity/file-locked-p-propertized-gc.el"),
        Some(r#"OK (handler-token (t t t t) 88 yes)"#),
    );
}

#[test]
fn first_modification_uses_buffer_file_truename_object() {
    compare_filename_identity(
        "first_modification_uses_buffer_file_truename_object",
        include_str!("fixtures/filelock_identity/first-modification.el"),
        Some(r#"OK (((lock-file t t)) 88 yes t)"#),
    );
}

#[test]
fn unlock_buffer_uses_buffer_file_truename_object() {
    compare_filename_identity(
        "unlock_buffer_uses_buffer_file_truename_object",
        include_str!("fixtures/filelock_identity/unlock-buffer.el"),
        Some(r#"OK (((unlock-file t t)) 88 yes t)"#),
    );
}

#[test]
fn kill_buffer_unlock_uses_target_buffer_file_truename_object() {
    compare_filename_identity(
        "kill_buffer_unlock_uses_target_buffer_file_truename_object",
        include_str!("fixtures/filelock_identity/kill-buffer-unlock.el"),
        Some(r#"OK (((unlock-file t t)) 88 yes nil)"#),
    );
}

#[test]
fn set_buffer_modified_p_uses_buffer_file_truename_object() {
    compare_filename_identity(
        "set_buffer_modified_p_uses_buffer_file_truename_object",
        include_str!("fixtures/filelock_identity/set-buffer-modified-p.el"),
        Some(r#"OK (((lock-file t t)) 88 yes t)"#),
    );
}

#[test]
fn expansion_absolute_original_value() {
    compare_filename_identity(
        "expansion_absolute_original_value",
        include_str!("fixtures/filelock_identity/expansion/absolute-original-value.el"),
        None,
    );
}

#[test]
fn expansion_default_handler_original_args_mutation_gc() {
    compare_filename_identity(
        "expansion_default_handler_original_args_mutation_gc",
        include_str!("fixtures/filelock_identity/expansion/default-handler-original-args-mutation-gc.el"),
        None,
    );
}

#[test]
fn expansion_encode_file_after_make_utf16_nul() {
    compare_filename_identity(
        "expansion_encode_file_after_make_utf16_nul",
        include_str!("fixtures/filelock_identity/expansion/encode-file-after-make-utf16-nul.el"),
        None,
    );
}

#[test]
fn expansion_encode_file_returned_expanded_alias_nul() {
    compare_filename_identity(
        "expansion_encode_file_returned_expanded_alias_nul",
        include_str!("fixtures/filelock_identity/expansion/encode-file-returned-expanded-alias-nul.el"),
        None,
    );
}

#[test]
fn expansion_handler_return_value_identity_control() {
    compare_filename_identity(
        "expansion_handler_return_value_identity_control",
        include_str!("fixtures/filelock_identity/expansion/handler-return-value-identity-control.el"),
        None,
    );
}

#[test]
fn expansion_make_lock_absolute_expanded_alias_return_nil() {
    compare_filename_identity(
        "expansion_make_lock_absolute_expanded_alias_return_nil",
        include_str!("fixtures/filelock_identity/expansion/make-lock-absolute-expanded-alias-return-nil.el"),
        None,
    );
}

#[test]
fn expansion_make_lock_canonical_handler_returned_owner() {
    compare_filename_identity(
        "expansion_make_lock_canonical_handler_returned_owner",
        include_str!("fixtures/filelock_identity/expansion/make-lock-canonical-handler-returned-owner.el"),
        None,
    );
}

#[test]
fn expansion_multibyte_coercion_fresh_value() {
    compare_filename_identity(
        "expansion_multibyte_coercion_fresh_value",
        include_str!("fixtures/filelock_identity/expansion/multibyte-coercion-fresh-value.el"),
        None,
    );
}

#[test]
fn expansion_name_handler_original_args_mutation_gc() {
    compare_filename_identity(
        "expansion_name_handler_original_args_mutation_gc",
        include_str!("fixtures/filelock_identity/expansion/name-handler-original-args-mutation-gc.el"),
        None,
    );
}

#[test]
fn expansion_normalized_result_handler_separate_behavior() {
    compare_filename_identity(
        "expansion_normalized_result_handler_separate_behavior",
        include_str!("fixtures/filelock_identity/expansion/normalized-result-handler-separate-behavior.el"),
        None,
    );
}

#[test]
fn expansion_recursive_default_live_name_and_expanded_value() {
    compare_filename_identity(
        "expansion_recursive_default_live_name_and_expanded_value",
        include_str!("fixtures/filelock_identity/expansion/recursive-default-live-name-and-expanded-value.el"),
        None,
    );
}

#[test]
fn expansion_superroot_equal_bytes_fresh_value() {
    compare_filename_identity(
        "expansion_superroot_equal_bytes_fresh_value",
        include_str!("fixtures/filelock_identity/expansion/superroot-equal-bytes-fresh-value.el"),
        None,
    );
}

#[test]
fn write_region_explicit_lockname() {
    compare_filename_identity(
        "write_region_explicit_lockname",
        include_str!("fixtures/filelock_identity/write-region-explicit-lockname.el"),
        None,
    );
}

#[test]
fn write_region_default_lockname() {
    compare_filename_identity(
        "write_region_default_lockname",
        include_str!("fixtures/filelock_identity/write-region-default-lockname.el"),
        None,
    );
}

#[test]
fn encode_file_norecord_control() {
    compare_filename_identity(
        "encode_file_norecord_control",
        include_str!("fixtures/filelock_identity/encode-file-norecord-control.el"),
        None,
    );
}
