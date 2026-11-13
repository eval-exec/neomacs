//! The raw-mmap pdump readers: heap strings, copied-on-mutation strings,
//! immovable/rodata string headers, string text properties, and vector,
//! record, lambda, cons, float, marker and overlay objects all decode out of
//! the retained file image with their identity and GC tracing intact.
//! (These are the mmap-image reader tests of eval's pdump_test.rs; the
//! section-coverage test stays in eval pending the section-loader surface
//! ruling, and the non-mapped tests stay with it.)
//
// API-drift shims (post-split re-home): the pre-split tests reached
// `Context`, `dump_to_file`, `load_from_dump` and the mmap/object-extra
// decoders through the pdump test module. The evaluator is now built
// through the core test facade, `load_from_dump` lives in core's load_boot,
// the image/object-extra decoders ride their feature-gated test-support
// facades, and symbol reads/writes go through the `obarray()` /
// `obarray_mut()` accessors.
use crate::emacs_core::load_boot::load_from_dump;
use crate::emacs_core::pdump::dump_to_file;
use crate::emacs_core::pdump::mmap_image_test_support as mmap_image;
use crate::emacs_core::pdump::object_extra_test_support as object_extra;
use neovm_value::heap_types::{LispMarker, LispString, OverlayDataInit};
use neovm_value::value::{
    StringTextPropertyRun, Value, get_string_text_properties_for_value, list_to_vec,
    set_string_text_properties_for_value,
};

#[test]
fn file_pdump_loads_heap_string_bytes_from_mmap_image() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.obarray_mut().set_symbol_value(
        "test-pdump-mapped-string",
        Value::string("mapped-pdump-string"),
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("mapped-string.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let image = mmap_image::load_image(&dump_path).expect("load raw mmap image");
    let heap_section = image
        .section(mmap_image::DumpSectionKind::HeapImage)
        .expect("heap image section");
    assert!(
        heap_section
            .windows(b"mapped-pdump-string".len())
            .any(|window| window == b"mapped-pdump-string"),
        "heap string bytes should live in the mmap heap section"
    );
    assert!(
        heap_section
            .windows(b"mapped-pdump-string\0".len())
            .any(|window| window == b"mapped-pdump-string\0"),
        "mapped string bytes must include GNU's trailing NUL after SBYTES"
    );

    let loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-string")
        .expect("restored string symbol");
    let string = value.as_lisp_string().expect("restored string");

    assert_eq!(string.as_bytes(), b"mapped-pdump-string");
    assert!(string.has_trailing_nul());
    assert!(
        loaded.pdump_image_contains_ptr(value.as_string_ptr().unwrap().cast::<u8>()),
        "loaded string object must be a tagged pointer into the retained mmap image"
    );
    assert!(
        loaded.pdump_image_contains_ptr(string.as_bytes().as_ptr()),
        "loaded string bytes must be borrowed from the retained mmap image"
    );
}

#[test]
fn mapped_pdump_string_bytes_copy_only_on_mutation() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.obarray_mut().set_symbol_value(
        "test-pdump-copy-on-mutation",
        Value::string("mapped-before-mutation"),
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("copy-on-mutation.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = loaded
        .obarray()
        .symbol_value_copied("test-pdump-copy-on-mutation")
        .expect("restored string symbol");
    let string = value.as_lisp_string().expect("restored string");
    assert!(
        loaded.pdump_image_contains_ptr(string.as_bytes().as_ptr()),
        "before mutation, string bytes should be borrowed from the mmap image"
    );

    let _ = value.with_lisp_string_mut(|string| {
        string.mutate_bytes(|bytes| bytes.extend_from_slice(b"!"));
    });
    let string = value.as_lisp_string().expect("mutated string");
    assert_eq!(string.as_bytes(), b"mapped-before-mutation!");
    assert!(
        !loaded.pdump_image_contains_ptr(string.as_bytes().as_ptr()),
        "after mutation, string bytes should copy out of the mmap image"
    );
    assert!(string.has_trailing_nul());
}

#[test]
fn file_pdump_preserves_immovable_string_size_byte() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let mut string = LispString::from_unibyte(vec![0xC0, 0x87]);
    string.pin_immovable();
    eval.obarray_mut()
        .set_symbol_value("test-pdump-immovable-string", Value::heap_string(string));

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("immovable-string.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = loaded
        .obarray()
        .symbol_value_copied("test-pdump-immovable-string")
        .expect("restored string symbol");
    let string = value.as_lisp_string().expect("restored string");
    assert_eq!(string.as_bytes(), &[0xC0, 0x87]);
    assert_eq!(string.size_byte(), -3);
    assert!(string.is_immovable());
}

#[test]
fn file_pdump_preserves_rodata_string_with_static_relocation() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let string = LispString::from_rodata_unibyte(b"rodata\0");
    eval.obarray_mut()
        .set_symbol_value("test-pdump-rodata-string", Value::heap_string(string));

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("rodata-string.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = loaded
        .obarray()
        .symbol_value_copied("test-pdump-rodata-string")
        .expect("restored string symbol");
    let string = value.as_lisp_string().expect("restored string");
    assert_eq!(string.as_bytes(), b"rodata");
    assert_eq!(string.size_byte(), -2);
    assert!(string.is_rodata());
    assert!(string.has_trailing_nul());
    assert!(
        !loaded.pdump_image_contains_ptr(string.as_bytes().as_ptr()),
        "rodata string bytes should point at registered static storage, not copied heap image bytes"
    );
}

#[test]
fn file_pdump_loads_string_text_props_from_mmap_object() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let value = Value::string("mapped-props");
    set_string_text_properties_for_value(
        value,
        vec![StringTextPropertyRun {
            start: 0,
            end: 6,
            plist: Value::list(vec![
                Value::symbol("face"),
                Value::string("mapped-string-prop-value"),
            ]),
        }],
    );
    assert!(
        get_string_text_properties_for_value(value).is_some(),
        "test setup must attach string text properties before dumping"
    );
    eval.obarray_mut()
        .set_symbol_value("test-pdump-mapped-string-props", value);

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("mapped-string-props.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let image = mmap_image::load_image(&dump_path).expect("load raw mmap image");
    let object_extra_payload = image
        .section(mmap_image::DumpSectionKind::ObjectExtra)
        .expect("object-extra section");
    let object_extra = object_extra::load_object_extra(object_extra_payload).expect("object extra");
    assert!(
        object_extra.iter().any(|object| matches!(
            object,
            object_extra::ObjectExtra::String { text_props, .. } if !text_props.is_empty()
        )),
        "dump should contain string text properties"
    );

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-string-props")
        .expect("restored string symbol");

    assert!(
        loaded.pdump_image_contains_ptr(value.as_string_ptr().unwrap().cast::<u8>()),
        "loaded string object must be a tagged pointer into the retained mmap image"
    );
    assert!(
        get_string_text_properties_for_value(value).is_some(),
        "string text properties must be restored before GC"
    );

    loaded.gc_collect_exact();
    let value_after_gc = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-string-props")
        .expect("restored string symbol after GC");
    let runs = get_string_text_properties_for_value(value_after_gc).expect("text props after GC");
    let plist = list_to_vec(&runs[0].plist).expect("plist values");
    assert_eq!(
        plist[1]
            .as_lisp_string()
            .expect("text prop string value")
            .as_bytes(),
        b"mapped-string-prop-value"
    );
}

#[test]
fn file_pdump_loads_vector_slots_from_mmap_image() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.obarray_mut().set_symbol_value(
        "test-pdump-mapped-vector",
        Value::vector(vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::string("mapped-vector-child"),
        ]),
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("mapped-vector.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-vector")
        .expect("restored vector symbol");
    let slots = value.as_vector_data().expect("restored vector");

    assert_eq!(slots.as_slice()[0], Value::fixnum(1));
    assert_eq!(slots.as_slice()[1], Value::fixnum(2));
    assert_eq!(
        slots.as_slice()[2]
            .as_lisp_string()
            .expect("vector child string")
            .as_bytes(),
        b"mapped-vector-child"
    );
    assert!(
        loaded.pdump_image_contains_ptr(value.as_veclike_ptr().unwrap().cast::<u8>()),
        "loaded vector object must be a tagged pointer into the retained mmap image"
    );
    assert!(
        loaded.pdump_image_contains_ptr(slots.as_slice().as_ptr().cast::<u8>()),
        "loaded vector slots must be borrowed from the retained mmap image"
    );

    loaded.gc_collect_exact();
    let value_after_gc = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-vector")
        .expect("restored vector symbol after GC");
    assert_eq!(
        value_after_gc.as_vector_data().unwrap().as_slice()[2]
            .as_lisp_string()
            .expect("vector child string after GC")
            .as_bytes(),
        b"mapped-vector-child",
        "mapped vector GC marking must trace children from the mmap object"
    );
}

#[test]
fn file_pdump_loads_record_object_from_mmap_image() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.obarray_mut().set_symbol_value(
        "test-pdump-mapped-record",
        Value::make_record(vec![
            Value::symbol("record-type"),
            Value::string("mapped-record-child"),
        ]),
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("mapped-record.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-record")
        .expect("restored record symbol");
    let slots = value.as_record_data().expect("restored record");

    assert!(
        loaded.pdump_image_contains_ptr(value.as_veclike_ptr().unwrap().cast::<u8>()),
        "loaded record object must be a tagged pointer into the retained mmap image"
    );
    assert!(
        loaded.pdump_image_contains_ptr(slots.as_slice().as_ptr().cast::<u8>()),
        "loaded record slots must be borrowed from the retained mmap image"
    );
    assert_eq!(
        slots.as_slice()[1]
            .as_lisp_string()
            .expect("record child string")
            .as_bytes(),
        b"mapped-record-child"
    );

    loaded.gc_collect_exact();
    let value_after_gc = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-record")
        .expect("restored record symbol after GC");
    assert_eq!(
        value_after_gc.as_record_data().unwrap().as_slice()[1]
            .as_lisp_string()
            .expect("record child string after GC")
            .as_bytes(),
        b"mapped-record-child",
        "mapped record GC marking must trace children from the mmap object"
    );
}

#[test]
fn file_pdump_loads_lambda_object_from_mmap_image() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let mut slots = vec![Value::NIL; neovm_value::tagged::header::CLOSURE_MIN_SLOTS];
    slots[1] = Value::string("mapped-lambda-child");
    eval.obarray_mut().set_symbol_value(
        "test-pdump-mapped-lambda",
        Value::make_lambda_with_slots(slots),
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("mapped-lambda.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-lambda")
        .expect("restored lambda symbol");
    let slots = value.closure_slots().expect("restored lambda slots");

    assert!(
        loaded.pdump_image_contains_ptr(value.as_veclike_ptr().unwrap().cast::<u8>()),
        "loaded lambda object must be a tagged pointer into the retained mmap image"
    );
    assert!(
        loaded.pdump_image_contains_ptr(slots.as_slice().as_ptr().cast::<u8>()),
        "loaded lambda slots must be borrowed from the retained mmap image"
    );
    assert_eq!(
        slots.as_slice()[1]
            .as_lisp_string()
            .expect("lambda child string")
            .as_bytes(),
        b"mapped-lambda-child"
    );

    loaded.gc_collect_exact();
    let value_after_gc = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-lambda")
        .expect("restored lambda symbol after GC");
    assert_eq!(
        value_after_gc.closure_slots().unwrap().as_slice()[1]
            .as_lisp_string()
            .expect("lambda child string after GC")
            .as_bytes(),
        b"mapped-lambda-child",
        "mapped lambda GC marking must trace children from the mmap object"
    );
}

#[test]
fn file_pdump_loads_cons_cells_from_mmap_image() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.obarray_mut().set_symbol_value(
        "test-pdump-mapped-cons",
        Value::cons(
            Value::string("mapped-cons-car"),
            Value::vector(vec![Value::fixnum(9)]),
        ),
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("mapped-cons.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-cons")
        .expect("restored cons symbol");

    assert!(value.is_cons());
    assert!(
        loaded.pdump_image_contains_ptr(value.xcons_ptr().cast::<u8>()),
        "loaded cons cell must be a tagged pointer into the retained mmap image"
    );
    assert_eq!(
        value
            .cons_car()
            .as_lisp_string()
            .expect("cons car string")
            .as_bytes(),
        b"mapped-cons-car"
    );
    assert_eq!(
        value.cons_cdr().as_vector_data().unwrap().as_slice(),
        &[Value::fixnum(9)]
    );

    loaded.gc_collect_exact();
    let value_after_gc = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-cons")
        .expect("restored cons symbol after GC");
    assert_eq!(
        value_after_gc
            .cons_car()
            .as_lisp_string()
            .expect("cons car string after GC")
            .as_bytes(),
        b"mapped-cons-car",
        "mapped cons GC marking must trace children from the mmap cell"
    );
}

#[test]
fn file_pdump_loads_float_objects_from_mmap_image() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.obarray_mut().set_symbol_value(
        "test-pdump-mapped-float",
        Value::make_float(std::f64::consts::PI),
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("mapped-float.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-float")
        .expect("restored float symbol");

    assert!(value.is_float());
    assert_eq!(value.xfloat(), std::f64::consts::PI);
    assert!(
        loaded.pdump_image_contains_ptr(value.as_float_ptr().unwrap().cast::<u8>()),
        "loaded float object must be a tagged pointer into the retained mmap image"
    );

    loaded.gc_collect_exact();
    let value_after_gc = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-float")
        .expect("restored float symbol after GC");
    assert_eq!(value_after_gc.xfloat(), std::f64::consts::PI);
}

#[test]
fn file_pdump_loads_marker_object_from_mmap_image() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.obarray_mut().set_symbol_value(
        "test-pdump-mapped-marker",
        Value::make_marker(LispMarker::new(
            neovm_value::heap_types::MarkerBuffer::DETACHED,
            neovm_edit::buffer::CharPos0::new(7),
            neovm_edit::buffer::EmacsBytePos::new(7),
            true,
            true,
        )),
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("mapped-marker.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-marker")
        .expect("restored marker symbol");
    let marker = value.as_marker_data().expect("restored marker");

    assert!(
        loaded.pdump_image_contains_ptr(value.as_veclike_ptr().unwrap().cast::<u8>()),
        "loaded marker object must be a tagged pointer into the retained mmap image"
    );
    assert!(marker.insertion_type());
    assert_eq!(marker.bytepos.get(), 7);
    assert_eq!(marker.charpos.get(), 7);

    loaded.gc_collect_exact();
    let value_after_gc = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-marker")
        .expect("restored marker symbol after GC");
    // Marker identity is the object pointer: GC keeps the very same
    // MarkerObj the load decoded from the mmap image.
    assert_eq!(
        value_after_gc.as_veclike_ptr(),
        value.as_veclike_ptr(),
        "the restored marker keeps its identity across GC"
    );
}

#[test]
fn file_pdump_loads_overlay_object_from_mmap_image() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.obarray_mut().set_symbol_value(
        "test-pdump-mapped-overlay",
        Value::make_overlay(OverlayDataInit {
            serial: neovm_value::heap_types::OverlaySerialSource::Mint,
            plist: Value::list(vec![
                Value::symbol("face"),
                Value::string("mapped-overlay-child"),
            ]),
            buffer: None,
            start: 2,
            end: 9,
            front_advance: true,
            rear_advance: false,
        }),
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("mapped-overlay.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-overlay")
        .expect("restored overlay symbol");
    let overlay = value.as_overlay_data().expect("restored overlay");
    let plist = list_to_vec(&overlay.plist).expect("overlay plist");

    assert!(
        loaded.pdump_image_contains_ptr(value.as_veclike_ptr().unwrap().cast::<u8>()),
        "loaded overlay object must be a tagged pointer into the retained mmap image"
    );
    assert_eq!(overlay.start, 2);
    assert_eq!(overlay.end, 9);
    assert!(overlay.front_advance);
    assert!(!overlay.rear_advance);
    assert_eq!(
        plist[1]
            .as_lisp_string()
            .expect("overlay child string")
            .as_bytes(),
        b"mapped-overlay-child"
    );

    loaded.gc_collect_exact();
    let value_after_gc = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mapped-overlay")
        .expect("restored overlay symbol after GC");
    let overlay_after_gc = value_after_gc.as_overlay_data().expect("overlay after GC");
    let plist_after_gc = list_to_vec(&overlay_after_gc.plist).expect("overlay plist after GC");
    assert_eq!(
        plist_after_gc[1]
            .as_lisp_string()
            .expect("overlay child string after GC")
            .as_bytes(),
        b"mapped-overlay-child",
        "mapped overlay GC marking must trace plist children from the mmap object"
    );
}
