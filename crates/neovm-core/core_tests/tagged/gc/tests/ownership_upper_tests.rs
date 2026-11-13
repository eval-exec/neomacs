//! Ownership shells for the rows that call an upper crate.
//! `clear-string` and the subr-metadata mint live here. Assertions stay in
//! the value bodies.
use neovm_eval::test_support::authority::leaked_subr_metadata_for_test;
use neovm_value::test_support::authority::run_ownership_upper_test_for_test;

fn run(name: &str) {
    run_ownership_upper_test_for_test(
        name,
        crate::emacs_core::buffer::builtin_clear_string,
        leaked_subr_metadata_for_test,
    );
}

macro_rules! shell {
    ($name:ident) => {
        #[test]
        fn $name() {
            run(stringify!($name));
        }
    };
}

shell!(concurrent_leaked_subr_dropped_from_defer_path);
shell!(concurrent_mapped_subr_still_deferred_and_side_table_marked);
shell!(concurrent_builtin_clear_string_retains_interval_child_for_snapshot);
shell!(owned_subr_storage_is_deallocated_by_its_exact_layout);
