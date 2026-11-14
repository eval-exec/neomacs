//! Filename handlers must receive the actual argument or buffer slot object.
//!
//! Source contract: GNU a360712 filelock.c:Flock_file/Funlock_file/
//! Ffile_locked_p pass their Lisp_Object through, while buffer.c and
//! insdel.c supply the buffer's file_truename slot. Recorded expectations
//! must come from printed, attested GNU results; none are guessed here.
//!
//! The initial GC forms retain independent bindings and establish identity/
//! mutation rather than exclusive rooting. The post-ENCODE fixtures additionally
//! remove explicit buffer holders and observe weak keys across real native
//! callbacks; their outputs must be interpreted with the weak negative control.
//! No particular hidden-root mechanism is inferred from a passing observation.

use crate::common::{
    assert_oracle_parity, oracle_mode_runs_live_gnu,
    return_if_neovm_enable_oracle_proptest_not_set, run_neovm_eval, run_oracle_eval,
};

fn compare_filename_identity(name: &str, form: &str, recorded_gnu: Option<&str>) {
    return_if_neovm_enable_oracle_proptest_not_set!();
    if !oracle_mode_runs_live_gnu() {
        if let Some(recorded) = recorded_gnu {
            let neomacs = run_neovm_eval(form).expect("Neomacs filename form should run");
            assert_eq!(
                neomacs, recorded,
                "recorded GNU filename contract in {name}"
            );
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
    assert_eq!(
        neomacs, oracle,
        "filename identity mismatch in {name}\n{form}"
    );
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
        Some(r#"OK ((t t t nil nil) "/threadsig-expand/leaX" yes)"#),
    );
}

#[test]
fn expansion_default_handler_original_args_mutation_gc() {
    compare_filename_identity(
        "expansion_default_handler_original_args_mutation_gc",
        include_str!(
            "fixtures/filelock_identity/expansion/default-handler-original-args-mutation-gc.el"
        ),
        Some(r#"OK ((expand-file-name t t t) t "leaX" "/threadsig-expand-default:dirY" yes)"#),
    );
}

#[test]
fn expansion_encode_file_after_make_utf16_nul() {
    compare_filename_identity(
        "expansion_encode_file_after_make_utf16_nul",
        include_str!("fixtures/filelock_identity/expansion/encode-file-after-make-utf16-nul.el"),
        Some(r#"OK (t t (caught wrong-type-argument filenamep))"#),
    );
}

#[test]
fn expansion_encode_file_returned_expanded_alias_nul() {
    compare_filename_identity(
        "expansion_encode_file_returned_expanded_alias_nul",
        include_str!(
            "fixtures/filelock_identity/expansion/encode-file-returned-expanded-alias-nul.el"
        ),
        Some(r#"OK (t t yes (caught wrong-type-argument filenamep))"#),
    );
}

#[test]
fn expansion_handler_return_value_identity_control() {
    compare_filename_identity(
        "expansion_handler_return_value_identity_control",
        include_str!(
            "fixtures/filelock_identity/expansion/handler-return-value-identity-control.el"
        ),
        Some(r#"OK (t t)"#),
    );
}

#[test]
fn expansion_make_lock_absolute_expanded_alias_return_nil() {
    compare_filename_identity(
        "expansion_make_lock_absolute_expanded_alias_return_nil",
        include_str!(
            "fixtures/filelock_identity/expansion/make-lock-absolute-expanded-alias-return-nil.el"
        ),
        Some(r#"OK (nil (t t) "/threadsig-lock-owner-leaX" yes)"#),
    );
}

#[test]
fn expansion_make_lock_canonical_handler_returned_owner() {
    compare_filename_identity(
        "expansion_make_lock_canonical_handler_returned_owner",
        include_str!(
            "fixtures/filelock_identity/expansion/make-lock-canonical-handler-returned-owner.el"
        ),
        Some(r#"OK (nil ((expand expand-file-name t t) (make t nil t)) yes)"#),
    );
}

#[test]
fn expansion_multibyte_coercion_fresh_value() {
    compare_filename_identity(
        "expansion_multibyte_coercion_fresh_value",
        include_str!("fixtures/filelock_identity/expansion/multibyte-coercion-fresh-value.el"),
        Some(
            r#"OK (t nil nil nil (47 116 104 114 101 97 100 115 105 103 45 101 120 112 97 110 100 45 206 187) (47 116 104 114 101 97 100 115 105 103 45 101 120 112 97 110 100 45 206 187) nil)"#,
        ),
    );
}

#[test]
fn expansion_name_handler_original_args_mutation_gc() {
    compare_filename_identity(
        "expansion_name_handler_original_args_mutation_gc",
        include_str!(
            "fixtures/filelock_identity/expansion/name-handler-original-args-mutation-gc.el"
        ),
        Some(
            r#"OK ((expand-file-name t t t) t "/threadsig-expand-name:leaX" "/threadsig-directoryY" yes)"#,
        ),
    );
}

#[test]
fn expansion_normalized_result_handler_separate_behavior() {
    compare_filename_identity(
        "expansion_normalized_result_handler_separate_behavior",
        include_str!(
            "fixtures/filelock_identity/expansion/normalized-result-handler-separate-behavior.el"
        ),
        Some(
            r#"OK ((expand-file-name nil t "/threadsig-normalized:leaf") t "/threadsig-handled-result" t)"#,
        ),
    );
}

#[test]
fn expansion_recursive_default_live_name_and_expanded_value() {
    compare_filename_identity(
        "expansion_recursive_default_live_name_and_expanded_value",
        include_str!(
            "fixtures/filelock_identity/expansion/recursive-default-live-name-and-expanded-value.el"
        ),
        Some(
            r#"OK (((inner expand-file-name t t) (outer expand-file-name t t "leaX" t)) t "leaX" yes)"#,
        ),
    );
}

#[test]
fn expansion_superroot_equal_bytes_fresh_value() {
    compare_filename_identity(
        "expansion_superroot_equal_bytes_fresh_value",
        include_str!("fixtures/filelock_identity/expansion/superroot-equal-bytes-fresh-value.el"),
        Some(r#"OK (nil t "/.." nil t)"#),
    );
}

#[test]
fn write_region_explicit_lockname() {
    compare_filename_identity(
        "write_region_explicit_lockname",
        include_str!("fixtures/filelock_identity/write-region-explicit-lockname.el"),
        Some(r#"OK (((lock-file t nil) (unlock-file t nil)) 88 "x")"#),
    );
}

#[test]
fn write_region_default_lockname() {
    compare_filename_identity(
        "write_region_default_lockname",
        include_str!("fixtures/filelock_identity/write-region-default-lockname.el"),
        Some(r#"OK ((t t t) (t t t))"#),
    );
}

#[test]
fn encode_file_norecord_control() {
    compare_filename_identity(
        "encode_file_norecord_control",
        include_str!("fixtures/filelock_identity/encode-file-norecord-control.el"),
        Some(r#"OK threadsig-sentinel"#),
    );
}

#[test]
fn post_encode_supersession_alias_mutation_gc() {
    compare_filename_identity(
        "post_encode_supersession_alias_mutation_gc",
        include_str!("fixtures/filelock_identity/post_encode/supersession-alias-mutation-gc.el"),
        Some(r#"OK (((threat t t)) t nil t)"#),
    );
}

#[test]
fn post_encode_lock_clash_alias_mutation_gc() {
    compare_filename_identity(
        "post_encode_lock_clash_alias_mutation_gc",
        include_str!("fixtures/filelock_identity/post_encode/clash-alias-mutation-gc.el"),
        Some(r#"OK (((ask-lock t t)) t t t)"#),
    );
}

#[test]
fn whole_operation_original_value_survives_slot_clear_gc() {
    compare_filename_identity(
        "whole_operation_original_value_survives_slot_clear_gc",
        include_str!("fixtures/filelock_identity/post_encode/weak-original-slot-clear-gc.el"),
        Some(r#"OK (t (verify-visited-file-modtime 1 t) "x" t)"#),
    );
}

#[test]
fn whole_operation_encoded_value_survives_holder_clear_kill_gc() {
    compare_filename_identity(
        "whole_operation_encoded_value_survives_holder_clear_kill_gc",
        include_str!("fixtures/filelock_identity/post_encode/weak-encoded-holder-clear-kill-gc.el"),
        Some(r#"OK ((verify-visited-file-modtime 1 1 nil) nil t)"#),
    );
}

#[test]
fn weak_unowned_string_collection_control() {
    compare_filename_identity(
        "weak_unowned_string_collection_control",
        include_str!("fixtures/filelock_identity/post_encode/weak-unowned-string-control.el"),
        Some(r#"OK 0"#),
    );
}

#[test]
fn post_encode_lock_clash_alias_nul_mutation_gc() {
    compare_filename_identity(
        "post_encode_lock_clash_alias_nul_mutation_gc",
        include_str!("fixtures/filelock_identity/post_encode/clash-alias-nul-mutation-gc.el"),
        Some(r#"OK (((ask-lock t t)) t t t)"#),
    );
}

#[test]
fn whole_operation_multibyte_utf8_alias_value_survives_holder_clear_kill_gc() {
    compare_filename_identity(
        "whole_operation_multibyte_utf8_alias_value_survives_holder_clear_kill_gc",
        include_str!(
            "fixtures/filelock_identity/post_encode/weak-multibyte-utf8-alias-holder-clear-kill-gc.el"
        ),
        Some(r#"OK ((t t) (verify-visited-file-modtime 1 1 nil) t t nil t)"#),
    );
}

#[test]
fn write_region_filename_handler_observes_visit_mutation() {
    compare_filename_identity(
        "write_region_filename_handler_observes_visit_mutation",
        include_str!(
            "fixtures/filelock_identity/write-region-filename-handler-after-visit-mutation.el"
        ),
        Some(r#"OK (((visit-expand t) (write-region t t t t)) threadsig-write-handler-marker t)"#),
    );
}
