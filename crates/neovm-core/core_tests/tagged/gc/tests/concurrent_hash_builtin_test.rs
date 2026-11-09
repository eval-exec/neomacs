//! Re-homed Tier-H cold-mutation shell.
//! The value body keeps every heap assertion; this shell supplies the four
//! evaluator/text builtins the body drives while the fixture TestHeap is
//! installed.
use neovm_value::tagged::value::TaggedValue;
use neovm_value::test_support::authority::run_concurrent_hash_builtin_test_for_test;

#[test]
fn tier_h_builtin_cold_mutation_paths_keep_deleted_and_inserted_children_live() {
    let mut remhash = |key: TaggedValue, table: TaggedValue, symbols: bool| {
        crate::emacs_core::builtins::builtin_remhash_values(key, table, symbols)
            .map_err(|error| format!("{error:?}"))
    };
    let mut puthash = |args: Vec<TaggedValue>, symbols: bool| {
        neovm_eval::test_support::authority::builtin_puthash_with_symbols_for_test(args, symbols)
            .map_err(|error| format!("{error:?}"))
    };
    let mut clrhash = |args: Vec<TaggedValue>| {
        crate::emacs_core::builtins::builtin_clrhash(args).map_err(|error| format!("{error:?}"))
    };
    let mut json_parse = |args: Vec<TaggedValue>| {
        neovm_text::test_support::authority::builtin_json_parse_string_for_test(args)
            .map_err(|error| format!("{error:?}"))
    };
    run_concurrent_hash_builtin_test_for_test(
        &mut remhash,
        &mut puthash,
        &mut clrhash,
        &mut json_parse,
    );
}
