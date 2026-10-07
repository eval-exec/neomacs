//! `make-byte-code` reifies `(make-hash-table-from-literal '(hash-table ...))`
//! forms in its constant vector (how `.elc` files carry hash-table
//! literals) and leaves every other constant alone.
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

#[test]
fn make_byte_code_converts_only_hash_table_literal_forms() {
    let mut ctx = Context::new();
    let f = ctx
        .eval_str(
            "(let ((circular (list 'mbl-head 1)))
               (setcdr (cdr circular) circular)
               (make-byte-code 0 \"\\300\\207\"
                 (vector
                  '(make-hash-table-from-literal '(hash-table test eq data (mbl-k 1)))
                  (list (make-symbol \"make-hash-table-from-literal\")
                        ''(hash-table data (mbl-k 2)))
                  '(mbl-other '(hash-table data (mbl-k 3)))
                  '(make-hash-table-from-literal)
                  '(make-hash-table-from-literal . improper)
                  '(1 . 2)
                  circular)
                 1))",
        )
        .expect("make-byte-code");
    crate::emacs_core::eval::push_scratch_gc_root(f);
    let constants = f.get_bytecode_data().unwrap().constants.to_vec();
    // The interned head, and an uninterned symbol of the same name (the
    // conversion has always matched the name), become tables.
    for (index, want) in [(0, 1), (1, 2)] {
        let table = constants[index];
        assert!(table.is_hash_table(), "constant {index}: {table:?}");
        let table = table.as_hash_table().unwrap();
        let key = Value::symbol("mbl-k").to_hash_key(&table.test);
        let got = table.data.get(&key).copied();
        assert_eq!(got, Some(Value::fixnum(want)), "constant {index}");
    }
    // Everything else stays the very same object.
    for (index, constant) in constants.iter().enumerate().skip(2) {
        assert!(constant.is_cons(), "constant {index}: {constant:?}");
    }
    assert_eq!(constants[2].cons_car(), Value::symbol("mbl-other"));
    assert_eq!(constants[6].cons_car(), Value::symbol("mbl-head"));
}
