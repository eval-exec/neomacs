use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use crate::heap_types::LispString;

fn compressed_table(context: &mut Context) -> Value {
    let purpose = context
        .tagged_heap
        .alloc_symbol_with_pos(Value::symbol("char-code-property-table"), Value::fixnum(0));
    let table =
        crate::emacs_core::chartable::make_char_table_with_extra_slots(purpose, Value::NIL, 5);
    let compressed = Value::heap_string(LispString::from_unibyte(vec![2, 2, 0xC2, 0x83, 1]));
    let mut depth2_contents = vec![Value::NIL; 32];
    depth2_contents[2] = compressed;
    let depth2 = Value::make_sub_char_table(2, 0, depth2_contents);
    let mut depth1_contents = vec![Value::NIL; 16];
    depth1_contents[0] = depth2;
    let depth1 = Value::make_sub_char_table(1, 0, depth1_contents);
    table
        .with_char_table_mut(|object| object.set_contents(0, depth1))
        .unwrap();
    table
}

#[test]
fn positioned_elt_public_and_bytecode_access_use_context_policy() {
    let mut context = Context::new();
    for enabled in [false, true] {
        context.symbols_with_pos_enabled = enabled;
        // Fresh tables avoid GNU's in-place decompression crossing modes.
        let table = compressed_table(&mut context);
        let public = builtin_elt_in_state(&mut context, vec![table, Value::fixnum(256)]).unwrap();
        let table = compressed_table(&mut context);
        let bytecode = builtin_elt_2(&mut context, table, Value::fixnum(256)).unwrap();
        if enabled {
            assert_eq!(public, Value::fixnum(2));
            assert_eq!(bytecode, Value::fixnum(2));
        } else {
            assert!(public.is_string());
            assert!(bytecode.is_string());
        }
    }
}

#[test]
fn elt_sequence_validation_precedes_index_validation() {
    let mut context = Context::new();
    let answer = context
        .eval_str(
            r#"
        (list (condition-case err (elt 42 'bad) (error err))
              (condition-case err (elt [] 'bad) (error err))
              (condition-case err (elt (make-char-table 'test) -1) (error err)))
    "#,
        )
        .unwrap();
    assert_eq!(
        print_value(&answer),
        "((wrong-type-argument sequencep 42) (wrong-type-argument fixnump bad) (wrong-type-argument characterp -1))"
    );
}
