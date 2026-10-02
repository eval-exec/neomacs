use crate::emacs_core::builtins::symbols::{symbol_id, symbol_id_checked};
use crate::emacs_core::builtins::types::record_type_of;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::{Value, eq_value_swp};
use crate::tagged::collection_reads::capture;
use crate::tagged::gc::with_tagged_heap;
use crate::tagged::mutate::LispCollectionRevision;

fn check_identity_dependency(source: Value, read: impl FnOnce()) {
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    let (_, reads) = capture(read);
    let reads = reads.expect("a read-only metadata projection retains its capture");
    unrelated.set_car(Value::T);
    assert!(
        reads.unchanged(),
        "unrelated writes preserve metadata reads"
    );
    LispCollectionRevision::changed(source);
    assert!(
        !reads.unchanged(),
        "the metadata projection observes its identity"
    );
}

#[test]
fn metadata_capture_typed_getters_observe_success_and_wrong_veclikes() {
    crate::test_utils::init_test_tracing();
    let vector = Value::vector(vec![Value::NIL]);
    let record = Value::make_record(vec![Value::T]);
    let table = Value::make_char_table(Value::NIL, Value::NIL, 0);
    let subtable = Value::make_sub_char_table(1, 0, vec![Value::NIL]);
    let positioned = with_tagged_heap(|heap| {
        heap.alloc_symbol_with_pos(Value::symbol("metadata-capture-symbol"), Value::fixnum(7))
    });
    let subr_id = intern("car");
    let subr = Value::subr(subr_id);
    check_identity_dependency(vector, || assert!(vector.as_vector_data().is_some()));
    check_identity_dependency(record, || assert!(record.as_record_data().is_some()));
    check_identity_dependency(table, || assert!(table.as_char_table_obj().is_some()));
    check_identity_dependency(subtable, || {
        assert!(subtable.as_sub_char_table_obj().is_some())
    });
    check_identity_dependency(positioned, || {
        assert!(positioned.as_symbol_with_pos_sym().is_some())
    });
    check_identity_dependency(subr, || {
        assert_eq!(subr.as_subr_id(), Some(subr_id));
        assert!(subr.subr_interactivity().is_some());
    });
    check_identity_dependency(vector, || {
        assert!(vector.as_record_data().is_none());
        assert!(vector.as_char_table_obj().is_none());
        assert!(vector.as_sub_char_table_obj().is_none());
        assert!(vector.as_symbol_with_pos_sym().is_none());
        assert!(vector.as_subr_id().is_none());
        assert!(vector.subr_interactivity().is_none());
    });
    check_identity_dependency(record, || assert!(record.as_vector_data().is_none()));
}

#[test]
fn metadata_capture_symbol_checks_keep_disabled_and_wrong_type_dependencies() {
    crate::test_utils::init_test_tracing();
    let symbol = Value::symbol("metadata-capture-checked");
    let positioned = with_tagged_heap(|heap| heap.alloc_symbol_with_pos(symbol, Value::fixnum(1)));
    let vector = Value::vector(vec![Value::NIL]);
    for enabled in [false, true] {
        for bare in [
            Value::NIL,
            Value::T,
            symbol,
            Value::keyword(":metadata-capture"),
        ] {
            assert_eq!(symbol_id_checked(&bare, enabled), bare.as_symbol_id());
        }
        assert_eq!(symbol_id_checked(&Value::UNBOUND, enabled), None);
        assert_eq!(symbol_id_checked(&Value::fixnum(4), enabled), None);
        check_identity_dependency(vector, || {
            assert_eq!(symbol_id_checked(&vector, enabled), None)
        });
        check_identity_dependency(positioned, || {
            assert_eq!(
                symbol_id_checked(&positioned, enabled),
                enabled.then(|| symbol.as_symbol_id().unwrap())
            );
        });
    }
    check_identity_dependency(positioned, || {
        assert_eq!(symbol_id(&positioned), symbol.as_symbol_id())
    });
    check_identity_dependency(vector, || assert_eq!(symbol_id(&vector), None));
    assert_eq!(symbol_id(&Value::UNBOUND), None);
}

#[test]
fn metadata_capture_eq_swp_observes_each_projected_identity() {
    crate::test_utils::init_test_tracing();
    let symbol = Value::symbol("metadata-capture-eq");
    let positioned = with_tagged_heap(|heap| heap.alloc_symbol_with_pos(symbol, Value::fixnum(2)));
    let vector = Value::vector(vec![Value::NIL]);
    check_identity_dependency(positioned, || {
        assert!(eq_value_swp(&positioned, &symbol, true))
    });
    check_identity_dependency(vector, || assert!(!eq_value_swp(&vector, &symbol, true)));
    let (_, reads) = capture(|| assert!(!eq_value_swp(&positioned, &symbol, false)));
    LispCollectionRevision::changed(positioned);
    assert!(
        reads.unwrap().unchanged(),
        "disabled SWP comparison does not project metadata"
    );
    let (_, reads) = capture(|| assert!(eq_value_swp(&positioned, &positioned, true)));
    LispCollectionRevision::changed(positioned);
    assert!(
        reads.unwrap().unchanged(),
        "bitwise equality keeps its existing projection-free path"
    );
}

#[test]
fn metadata_capture_record_type_keeps_outer_and_class_dependencies() {
    crate::test_utils::init_test_tracing();
    let class_name = Value::symbol("metadata-capture-class");
    let class = Value::make_record(vec![Value::NIL, class_name]);
    let object = Value::make_record(vec![class, Value::NIL]);
    let (_, reads) = capture(|| assert_eq!(record_type_of(object), Some(class_name)));
    class.set_record_slot(1, Value::T);
    assert!(
        !reads.unwrap().unchanged(),
        "the class record is a dependency"
    );
    let (_, reads) = capture(|| assert_eq!(record_type_of(object), Some(Value::T)));
    object.set_record_slot(1, Value::T);
    assert!(
        !reads.unwrap().unchanged(),
        "the outer object is a dependency"
    );
    let short_class = Value::make_record(vec![Value::NIL]);
    let short_object = Value::make_record(vec![short_class]);
    check_identity_dependency(short_class, || {
        assert_eq!(record_type_of(short_object), Some(short_class))
    });
    let vector = Value::vector(vec![Value::NIL]);
    check_identity_dependency(vector, || assert_eq!(record_type_of(vector), None));
}

#[test]
fn metadata_capture_repeated_getters_preserve_first_read_and_nested_reads() {
    crate::test_utils::init_test_tracing();
    let vector = Value::vector(vec![Value::NIL]);
    let (_, reads) = capture(|| {
        assert!(vector.as_record_data().is_none());
        let (_, inner) = capture(|| assert!(vector.as_vector_data().is_some()));
        vector.set_vector_slot(0, Value::T);
        assert!(!inner.unwrap().unchanged());
        assert!(vector.as_record_data().is_none());
    });
    assert!(
        reads.is_none(),
        "the later projection must not replace the first-read revision"
    );
    let (_, outer) = capture(|| {
        let (_, inner) = capture(|| assert!(vector.as_record_data().is_none()));
        assert!(inner.unwrap().unchanged());
    });
    vector.set_vector_slot(0, Value::NIL);
    assert!(
        !outer.unwrap().unchanged(),
        "nested wrong-type metadata reads propagate outward"
    );
}
