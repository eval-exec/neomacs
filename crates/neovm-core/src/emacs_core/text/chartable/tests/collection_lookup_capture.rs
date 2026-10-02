use super::*;
use crate::tagged::collection_reads::capture;
use crate::tagged::mutate::LispCollectionRevision;

#[test]
fn collection_lookup_capture_retains_ascii_child_and_parent_dependencies() {
    crate::test_utils::init_test_tracing();
    #[derive(Debug)]
    enum Dependency {
        Table,
        Child,
        Parent,
        NestedChild,
    }
    for changed in [
        Dependency::Table,
        Dependency::Child,
        Dependency::Parent,
        Dependency::NestedChild,
    ] {
        let child = Value::make_sub_char_table(3, 0, vec![Value::NIL; 128]);
        let parent = Value::make_char_table(Value::NIL, Value::fixnum(42), 0);
        let table = Value::make_char_table(Value::NIL, Value::NIL, 0);
        table.with_char_table_mut(|obj| {
            obj.ascii = child;
            obj.parent = parent;
        });
        let (result, reads) = capture(|| ct_lookup(&table, b'x' as i64));
        assert_eq!(result.unwrap(), Value::fixnum(42));
        let reads = reads.expect("read-only lookup is coherent");
        assert!(reads.unchanged());
        let unrelated = Value::cons(Value::NIL, Value::NIL);
        unrelated.set_car(Value::T);
        assert!(reads.unchanged());
        match changed {
            Dependency::Table => {
                table.with_char_table_mut(|obj| obj.defalt = Value::T);
            }
            Dependency::Child => {
                child.with_sub_char_table_mut(|obj| {
                    obj.contents.ensure_owned()[b'x' as usize] = Value::T
                });
            }
            Dependency::Parent => {
                parent.with_char_table_mut(|obj| obj.defalt = Value::T);
            }
            Dependency::NestedChild => {
                // A child read by a nested lookup still joins its outer capture.
                let (_, outer) = capture(|| {
                    let (_, inner) = capture(|| ct_lookup(&table, b'x' as i64));
                    assert!(inner.unwrap().unchanged_and_observe());
                });
                child.with_sub_char_table_mut(|obj| {
                    obj.contents.ensure_owned()[b'x' as usize] = Value::T
                });
                assert!(!outer.unwrap().unchanged());
            }
        }
        assert!(!reads.unchanged(), "dependency {changed:?}");
    }
}

#[test]
fn collection_lookup_capture_retains_rejected_veclike_headers() {
    crate::test_utils::init_test_tracing();
    let wrong = Value::vector(vec![Value::NIL]);
    let (_, reads) = capture(|| ct_lookup(&wrong, b'x' as i64));
    wrong.set_vector_slot(0, Value::T);
    assert!(!reads.unwrap().unchanged());

    let table = Value::make_char_table(Value::NIL, Value::NIL, 0);
    let wrong_parent = Value::vector(vec![Value::NIL]);
    table.with_char_table_mut(|obj| obj.parent = wrong_parent);
    let (result, reads) = capture(|| ct_lookup(&table, b'x' as i64));
    assert_eq!(result.unwrap(), Value::NIL);
    wrong_parent.set_vector_slot(0, Value::T);
    assert!(!reads.unwrap().unchanged());
}

#[test]
fn collection_lookup_capture_keeps_first_read_before_an_intervening_write() {
    crate::test_utils::init_test_tracing();
    let table = Value::make_char_table(Value::NIL, Value::fixnum(42), 0);
    let (_, reads) = capture(|| {
        assert_eq!(ct_lookup(&table, b'x' as i64).unwrap(), Value::fixnum(42));
        LispCollectionRevision::changed(table);
        assert_eq!(ct_lookup(&table, b'x' as i64).unwrap(), Value::fixnum(42));
    });
    assert!(reads.is_none());
}
