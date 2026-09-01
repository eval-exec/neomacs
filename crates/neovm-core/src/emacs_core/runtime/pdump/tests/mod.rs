use super::*;
use crate::buffer::LispCharPos1;
use crate::emacs_core::bytecode::chunk::{ByteCodeFunction, GnuByteOffsetMapEntry};
use crate::emacs_core::bytecode::opcode::Op;
fn test_ob() -> crate::emacs_core::symbol::Obarray {
    crate::emacs_core::symbol::Obarray::new()
}
use crate::emacs_core::format_eval_result;
use crate::emacs_core::intern::intern;
use crate::emacs_core::mode::{FontLockDefaults, FontLockKeyword, MajorMode};
use crate::emacs_core::pdump::types::{DumpByteCodeInstructions, DumpHeapObject, DumpSymId};
use crate::emacs_core::value::{
    LambdaParams, StringTextPropertyRun, Value, get_string_text_properties_for_value, list_to_vec,
    set_string_text_properties_for_value,
};
use crate::heap_types::{LispMarker, LispString, OverlayData};

#[test]
fn test_pdump_round_trip_basic() {
    crate::test_utils::init_test_tracing();
    // Create a minimal evaluator
    let mut eval = Context::new();

    // Set a symbol value to verify round-trip
    eval.obarray
        .set_symbol_value("test-pdump-var", Value::fixnum(42));

    // Dump to temp file
    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("test.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    // Load from dump
    let loaded = load_from_dump(&dump_path).expect("load should succeed");

    // Verify the symbol value survived
    assert_eq!(
        loaded.obarray.symbol_value("test-pdump-var"),
        Some(&Value::fixnum(42))
    );
}

#[test]
fn file_pdump_object_descriptors_stay_sparse() {
    let objects = vec![
        DumpHeapObject::Cons {
            car: types::DumpValue::True,
            cdr: types::DumpValue::Nil,
        },
        DumpHeapObject::Free,
    ];
    let object_extra = object_extra::build_object_extra(&objects, &[]).expect("build object extra");
    let heap = types::DumpTaggedHeap {
        objects,
        mapped_cons: vec![Some(types::DumpConsSpan { offset: 0 }), None],
        mapped_floats: vec![None, None],
        mapped_strings: vec![None, None],
        mapped_veclikes: vec![None, None],
        mapped_slots: vec![None, None],
    };
    let spans = object_starts::LoadedSpans::from_heap(&heap);

    let descriptors = object_extra::load_file_object_descriptors(&object_extra, &spans, None)
        .expect("load sparse file descriptors");

    assert_eq!(descriptors.len(), 2);
    assert_eq!(descriptors.descriptor_count(), 1);
    assert!(descriptors.get(0).is_none());
    assert!(matches!(descriptors.get(1), Some(DumpHeapObject::Free)));
}

#[test]
fn dumped_gnu_bytecode_does_not_duplicate_derived_instructions() {
    let mut function = ByteCodeFunction::new(LambdaParams::simple(Vec::new()));
    function.ops = vec![Op::Nil, Op::Return];
    function.gnu_byte_offset_map = Some(vec![
        GnuByteOffsetMapEntry::new(0, 0),
        GnuByteOffsetMapEntry::new(1, 1),
    ]);
    function.gnu_bytecode_bytes = Some(crate::tagged::header::LispByteVec::owned(vec![0xC1, 0x87]));

    let mut eval = Context::new();
    eval.obarray
        .set_symbol_value("pdump-bytecode-source", Value::make_bytecode(function));
    let snapshot = snapshot_evaluator(&eval);
    let dumped = snapshot
        .tagged_heap
        .objects
        .iter()
        .find_map(|object| match object {
            DumpHeapObject::ByteCode(function) => Some(function),
            _ => None,
        })
        .expect("dumped bytecode descriptor");

    assert!(
        matches!(
            &dumped.instructions,
            DumpByteCodeInstructions::Gnu(crate::emacs_core::pdump::types::DumpByteData::Owned(bytes)) if bytes == &[0xC1, 0x87]
        ),
        "GNU bytecode bytes must be the sole instruction source; decoded instructions and byte offsets are derived load-time state"
    );
}

#[test]
fn pdump_round_trip_rebuilds_native_boolean_forwarders() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    assert_eq!(
        format_eval_result(
            &eval.eval_str("(progn (setq inhibit-message :dumped) inhibit-message)")
        ),
        "OK t"
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("native-boolean.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");

    let result = loaded.eval_str(
        "(list inhibit-message
               (set 'inhibit-message nil)
               inhibit-message
               (set 'inhibit-message :after-load)
               inhibit-message)",
    );
    assert_eq!(format_eval_result(&result), "OK (t nil nil :after-load t)");
}

#[test]
fn pdump_round_trip_preserves_selected_global_map_separately_from_lisp_variable() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str(
        "(progn
           (setq pdump-selected-global-map (make-keymap))
           (use-global-map pdump-selected-global-map)
           (setq global-map (make-keymap)))",
    )
    .expect("global keymap setup should succeed");

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("selected-global-map.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");

    let restored = loaded.eval_str(
        "(list
           (eq (current-global-map) pdump-selected-global-map)
           (eq (current-global-map) global-map)
           (keymapp (current-global-map)))",
    );
    assert_eq!(format_eval_result(&restored), "OK (t nil t)");
}

#[test]
fn restore_snapshot_rejects_non_keymap_selected_global_map() {
    crate::test_utils::init_test_tracing();
    let mut snapshot = snapshot_evaluator(&Context::new());
    snapshot.current_global_map = types::DumpValue::Int(42);

    let result = restore_snapshot(&snapshot);
    match result {
        Err(DumpError::DeserializationError(message)) => {
            assert!(
                message.contains("selected global map"),
                "unexpected validation error: {message}"
            );
        }
        Err(other) => panic!("unexpected restoration error: {other}"),
        Ok(_) => panic!("non-keymap selected global map unexpectedly restored"),
    }
}

#[test]
fn test_pdump_bytecode_round_trips_image_resident() {
    crate::test_utils::init_test_tracing();
    // task 03/3a, amended by the mapped-bytecode arc: the `#[...]` reader
    // literal (`closure_from_reader_literal_slots` -> `Value::make_bytecode`)
    // allocates from the BYTECODE ARENA PAGES pre-dump, while the pdump
    // restore installs the function INTO ITS IMAGE-RESERVED SPAN (the
    // `ByteCodeObj` reservation written by the dump; see
    // `reserve_typed_object_with_extras`). A restored function must be
    // image-resident, still callable, and survive a full GC in place.
    let mut eval = Context::new();
    // Classic constant-returning GNU bytecode: constants[0]=42, Breturn.
    eval.eval_str("(defvar bcarena-pdump-fn #[0 \"\\300\\207\" [42] 1])")
        .expect("defvar should evaluate");
    let pre = *eval
        .obarray
        .symbol_value("bcarena-pdump-fn")
        .expect("pre-dump value");
    assert_eq!(
        pre.veclike_type(),
        Some(crate::tagged::header::VecLikeType::ByteCode)
    );
    assert!(
        eval.tagged_heap
            .bytecode_arena_owns_for_test(pre.as_veclike_ptr().unwrap() as *const u8),
        "reader-constructed bytecode must live on the arena pages",
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("bcarena.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let mut restored = load_from_dump(&dump_path).expect("load should succeed");

    let post = *restored
        .obarray
        .symbol_value("bcarena-pdump-fn")
        .expect("restored value");
    assert_eq!(
        post.veclike_type(),
        Some(crate::tagged::header::VecLikeType::ByteCode)
    );
    assert!(
        restored.tagged_heap.mapped_image_owns_for_test(post),
        "pdump-restored bytecode must live in its image-reserved span",
    );
    assert!(
        !restored
            .tagged_heap
            .bytecode_arena_owns_for_test(post.as_veclike_ptr().unwrap() as *const u8),
        "image-resident bytecode must not also claim an arena page",
    );
    assert!(
        post.get_bytecode_data()
            .expect("restored bytecode data")
            .resident_ops()
            .is_empty(),
        "pdump-restored GNU bytecode should stay cold until execution",
    );

    // The restored function executes (payload round-tripped intact), and
    // still does after a full GC in the restored evaluator.
    let out = restored
        .eval_str("(funcall bcarena-pdump-fn)")
        .expect("restored bytecode runs");
    assert_eq!(out, Value::fixnum(42));
    assert!(
        !post
            .get_bytecode_data()
            .expect("restored bytecode data")
            .resident_ops()
            .is_empty(),
        "executing restored GNU bytecode should initialize its decoded IR",
    );
    restored.gc_collect();
    let out2 = restored
        .eval_str("(funcall bcarena-pdump-fn)")
        .expect("bytecode runs after GC");
    assert_eq!(out2, Value::fixnum(42));
    let after_gc = *restored
        .obarray
        .symbol_value("bcarena-pdump-fn")
        .expect("value after GC");
    assert!(
        restored.tagged_heap.mapped_image_owns_for_test(after_gc),
        "image-resident bytecode survives GC in place",
    );
}

#[test]
fn pdump_round_trip_preserves_buffer_text_backend_kind() {
    crate::test_utils::init_test_tracing();
    for backend_kind in crate::buffer::BufferTextBackendKind::non_gap_implemented_variants() {
        let backend = backend_kind.symbol_name();
        let mut eval = Context::new();
        let buffer_name = format!("{backend}-dump");
        let setup = eval.eval_str(&format!(
            r#"(progn
                 (neomacs-set-default-buffer-text-backend '{backend})
                 (save-current-buffer
                   (set-buffer (get-buffer-create "{buffer_name}"))
                   (insert "éabc")
                   (list (neomacs-buffer-text-backend) (buffer-string))))"#
        ));
        assert_eq!(
            format_eval_result(&setup),
            format!(r#"OK ({backend} "éabc")"#)
        );

        let dir = tempfile::tempdir().unwrap();
        let dump_path = dir.path().join(format!("{backend}-text-backend.pdump"));
        dump_to_file(&eval, &dump_path).expect("dump should succeed");

        let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
        let restored = loaded.eval_str(&format!(
            r#"(list
                 (neomacs-default-buffer-text-backend)
                 (save-current-buffer
                   (set-buffer (get-buffer "{buffer_name}"))
                   (list (neomacs-buffer-text-backend) (buffer-string)))
                 (save-current-buffer
                   (set-buffer (get-buffer-create "{backend}-after-load"))
                   (insert "z")
                   (list (neomacs-buffer-text-backend) (buffer-string))))"#
        ));
        assert_eq!(
            format_eval_result(&restored),
            format!(r#"OK ({backend} ({backend} "éabc") ({backend} "z"))"#)
        );
    }
}

#[test]
fn file_pdump_stores_symbol_table_in_raw_mmap_section() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.obarray
        .set_symbol_value("pdump-symbol-section-probe", Value::fixnum(71));

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("symbol-table-section.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let image = super::mmap_image::load_image(&dump_path).expect("load raw mmap image");
    assert!(
        image
            .section(super::mmap_image::DumpSectionKind::SymbolTable)
            .is_some(),
        "file pdumps must carry the symbol interner in a raw mmap section"
    );
    assert!(
        image
            .section(super::mmap_image::DumpSectionKind::ObjectStarts)
            .is_some(),
        "file pdumps must carry mapped heap object starts in a raw mmap section"
    );
    let object_extra = image
        .section(super::mmap_image::DumpSectionKind::ObjectExtra)
        .expect("object-extra section");
    let object_extra = super::object_extra::load_object_extra(object_extra).expect("object extra");
    assert!(
        !object_extra.is_empty(),
        "file pdumps must carry compact non-HeapImage object metadata"
    );
    let obarray = image
        .section(super::mmap_image::DumpSectionKind::Obarray)
        .expect("obarray section");
    let obarray = super::obarray_image::load_obarray_section(obarray).expect("obarray");
    assert!(
        !obarray.symbols.is_empty(),
        "file pdumps must carry obarray symbol state in a raw mmap section"
    );
    let charset_payload = image
        .section(super::mmap_image::DumpSectionKind::CharsetRegistry)
        .expect("charset-registry section");
    let _charset =
        super::charset_image::load_charset_section(charset_payload).expect("charset registry");
    assert!(
        !charset_payload.is_empty(),
        "file pdumps must carry charset registry state in a raw mmap section"
    );
    let coding_system_payload = image
        .section(super::mmap_image::DumpSectionKind::CodingSystems)
        .expect("coding-systems section");
    let _coding_systems =
        super::coding_system_image::load_coding_system_section(coding_system_payload)
            .expect("coding systems");
    assert!(
        !coding_system_payload.is_empty(),
        "file pdumps must carry coding system state in a raw mmap section"
    );
    let face_payload = image
        .section(super::mmap_image::DumpSectionKind::FaceTable)
        .expect("face-table section");
    let _faces = super::face_image::load_face_table_section(face_payload).expect("face table");
    assert!(
        !face_payload.is_empty(),
        "file pdumps must carry Lisp face state in a raw mmap section"
    );
    let buffer_payload = image
        .section(super::mmap_image::DumpSectionKind::Buffers)
        .expect("buffers section");
    let _buffers =
        super::buffer_image::load_buffer_manager_section(buffer_payload).expect("buffers");
    assert!(
        !buffer_payload.is_empty(),
        "file pdumps must carry buffer state in a raw mmap section"
    );
    let roots_payload = image
        .section(super::mmap_image::DumpSectionKind::Roots)
        .expect("roots section");
    let _roots = super::roots_image::load_roots_section(roots_payload).expect("roots");
    assert!(
        !roots_payload.is_empty(),
        "file pdumps must carry top-level Lisp roots in a raw mmap section"
    );
    let autoloads_payload = image
        .section(super::mmap_image::DumpSectionKind::Autoloads)
        .expect("autoloads section");
    let _autoloads =
        super::autoloads_image::load_autoloads_section(autoloads_payload).expect("autoloads");
    assert!(
        !autoloads_payload.is_empty(),
        "file pdumps must carry autoload manager state in a raw mmap section"
    );
    let runtime_managers_payload = image
        .section(super::mmap_image::DumpSectionKind::RuntimeManagers)
        .expect("runtime-managers section");
    let _runtime_managers =
        super::runtime_managers_image::load_runtime_managers_section(runtime_managers_payload)
            .expect("runtime managers");
    assert!(
        image
            .section(super::mmap_image::DumpSectionKind::RuntimeState)
            .is_none(),
        "file pdumps must not carry a monolithic bincode RuntimeState section"
    );

    let loaded = load_from_dump(&dump_path).expect("load should succeed");
    assert_eq!(
        loaded.obarray.symbol_value("pdump-symbol-section-probe"),
        Some(&Value::fixnum(71))
    );
}

#[test]
fn file_pdump_loads_heap_string_bytes_from_mmap_image() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.obarray.set_symbol_value(
        "test-pdump-mapped-string",
        Value::string("mapped-pdump-string"),
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("mapped-string.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let image = super::mmap_image::load_image(&dump_path).expect("load raw mmap image");
    let heap_section = image
        .section(super::mmap_image::DumpSectionKind::HeapImage)
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
    let value = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-string")
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
    let mut eval = Context::new();
    eval.obarray.set_symbol_value(
        "test-pdump-copy-on-mutation",
        Value::string("mapped-before-mutation"),
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("copy-on-mutation.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = *loaded
        .obarray
        .symbol_value("test-pdump-copy-on-mutation")
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
    let mut eval = Context::new();
    let mut string = LispString::from_unibyte(vec![0xC0, 0x87]);
    string.pin_immovable();
    eval.obarray
        .set_symbol_value("test-pdump-immovable-string", Value::heap_string(string));

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("immovable-string.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = *loaded
        .obarray
        .symbol_value("test-pdump-immovable-string")
        .expect("restored string symbol");
    let string = value.as_lisp_string().expect("restored string");
    assert_eq!(string.as_bytes(), &[0xC0, 0x87]);
    assert_eq!(string.size_byte(), -3);
    assert!(string.is_immovable());
}

#[test]
fn file_pdump_preserves_rodata_string_with_static_relocation() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let string = LispString::from_rodata_unibyte(b"rodata\0");
    eval.obarray
        .set_symbol_value("test-pdump-rodata-string", Value::heap_string(string));

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("rodata-string.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = *loaded
        .obarray
        .symbol_value("test-pdump-rodata-string")
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
    let mut eval = Context::new();
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
    eval.obarray
        .set_symbol_value("test-pdump-mapped-string-props", value);

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("mapped-string-props.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let image = super::mmap_image::load_image(&dump_path).expect("load raw mmap image");
    let object_extra_payload = image
        .section(super::mmap_image::DumpSectionKind::ObjectExtra)
        .expect("object-extra section");
    let object_extra =
        super::object_extra::load_object_extra(object_extra_payload).expect("object extra");
    assert!(
        object_extra.iter().any(|object| matches!(
            object,
            super::object_extra::ObjectExtra::String { text_props, .. } if !text_props.is_empty()
        )),
        "dump should contain string text properties"
    );

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-string-props")
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
    let value_after_gc = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-string-props")
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
    let mut eval = Context::new();
    eval.obarray.set_symbol_value(
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
    let value = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-vector")
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
    let value_after_gc = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-vector")
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
    let mut eval = Context::new();
    eval.obarray.set_symbol_value(
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
    let value = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-record")
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
    let value_after_gc = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-record")
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
    let mut eval = Context::new();
    let mut slots = vec![Value::NIL; crate::tagged::header::CLOSURE_MIN_SLOTS];
    slots[1] = Value::string("mapped-lambda-child");
    eval.obarray.set_symbol_value(
        "test-pdump-mapped-lambda",
        Value::make_lambda_with_slots(slots),
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("mapped-lambda.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-lambda")
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
    let value_after_gc = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-lambda")
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
    let mut eval = Context::new();
    eval.obarray.set_symbol_value(
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
    let value = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-cons")
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
    let value_after_gc = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-cons")
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
    let mut eval = Context::new();
    eval.obarray.set_symbol_value(
        "test-pdump-mapped-float",
        Value::make_float(std::f64::consts::PI),
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("mapped-float.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-float")
        .expect("restored float symbol");

    assert!(value.is_float());
    assert_eq!(value.xfloat(), std::f64::consts::PI);
    assert!(
        loaded.pdump_image_contains_ptr(value.as_float_ptr().unwrap().cast::<u8>()),
        "loaded float object must be a tagged pointer into the retained mmap image"
    );

    loaded.gc_collect_exact();
    let value_after_gc = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-float")
        .expect("restored float symbol after GC");
    assert_eq!(value_after_gc.xfloat(), std::f64::consts::PI);
}

#[test]
fn file_pdump_loads_marker_object_from_mmap_image() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.obarray.set_symbol_value(
        "test-pdump-mapped-marker",
        Value::make_marker(LispMarker {
            buffer: None,
            insertion_type: true,
            marker_id: Some(42),
            bytepos: 7,
            charpos: 7,
            last_position_valid: true,
            next_marker: std::ptr::null_mut(),
        }),
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("mapped-marker.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-marker")
        .expect("restored marker symbol");
    let marker = value.as_marker_data().expect("restored marker");

    assert!(
        loaded.pdump_image_contains_ptr(value.as_veclike_ptr().unwrap().cast::<u8>()),
        "loaded marker object must be a tagged pointer into the retained mmap image"
    );
    assert!(marker.insertion_type);
    assert_eq!(marker.marker_id, Some(42));
    assert_eq!(marker.bytepos, 7);
    assert_eq!(marker.charpos, 7);

    loaded.gc_collect_exact();
    let value_after_gc = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-marker")
        .expect("restored marker symbol after GC");
    assert_eq!(
        value_after_gc
            .as_marker_data()
            .expect("marker after GC")
            .marker_id,
        Some(42)
    );
}

#[test]
fn file_pdump_loads_overlay_object_from_mmap_image() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.obarray.set_symbol_value(
        "test-pdump-mapped-overlay",
        Value::make_overlay(OverlayData {
            serial: 0,
            plist: Value::list(vec![
                Value::symbol("face"),
                Value::string("mapped-overlay-child"),
            ]),
            buffer: None,
            start: 2,
            end: 9,
            position_handle: None,
            front_advance: true,
            rear_advance: false,
        }),
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("mapped-overlay.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let value = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-overlay")
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
    let value_after_gc = *loaded
        .obarray
        .symbol_value("test-pdump-mapped-overlay")
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

#[test]
fn pdump_dumps_default_value_for_active_dynamic_plain_binding() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let sym = intern("pdump-dynamic-plain-var");
    eval.obarray
        .set_symbol_value_id(sym, Value::symbol("default-value"));
    eval.try_specbind(sym, Value::symbol("dynamic-value"))
        .expect("dynamic binding");

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("dynamic-plain.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let loaded = load_from_dump(&dump_path).expect("load should succeed");
    assert_eq!(
        loaded.obarray.symbol_value_id(sym),
        Some(&Value::symbol("default-value")),
        "pdump must serialize the top-level value, not the active dynamic binding"
    );
}

#[test]
fn test_clone_active_evaluator_preserves_in_progress_require_and_load_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.require_stack.push(intern("cl-macs"));
    eval.loads_in_progress
        .push(crate::heap_types::LispString::from_utf8(
            "/tmp/neomacs-pdump-clone-in-progress.el",
        ));

    let cloned = clone_active_evaluator(&mut eval).expect("clone should succeed");

    assert_eq!(cloned.require_stack, vec![intern("cl-macs")]);
    assert_eq!(
        cloned.loads_in_progress,
        vec![crate::heap_types::LispString::from_utf8(
            "/tmp/neomacs-pdump-clone-in-progress.el"
        )]
    );
}

#[test]
fn test_restore_active_runtime_after_clone_reinstalls_live_charset_registry() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::charset::reset_charset_registry();

    let mut eval = Context::new();
    let mut args = vec![value::Value::NIL; 17];
    args[0] = value::Value::symbol("charset-pdump-clone-restore-test");
    args[1] = value::Value::fixnum(1);
    args[2] = value::Value::vector(vec![value::Value::fixnum(0), value::Value::fixnum(127)]);
    args[16] = value::Value::list(vec![
        value::Value::symbol("doc"),
        value::Value::string("live charset registry should survive clone handoff"),
    ]);
    crate::emacs_core::charset::builtin_define_charset_internal(args).unwrap();

    let live_runtime = snapshot_active_runtime(&mut eval);
    let cloned = clone_active_evaluator(&mut eval).expect("first clone should succeed");
    restore_active_runtime(&mut eval, &live_runtime);
    drop(cloned);

    let cloned_again = clone_active_evaluator(&mut eval).expect("second clone should succeed");
    restore_active_runtime(&mut eval, &live_runtime);
    drop(cloned_again);

    let registry = crate::emacs_core::charset::snapshot_charset_registry();
    let charset_name = crate::emacs_core::intern::intern("charset-pdump-clone-restore-test");
    let doc_key = crate::emacs_core::intern::intern("doc");
    let entry = registry
        .charsets
        .iter()
        .find(|info| info.name == charset_name)
        .expect("restored charset entry");
    assert_eq!(entry.plist.len(), 2);
    assert_eq!(entry.plist[0].0, doc_key);
    assert_eq!(
        entry.plist[0].1,
        value::Value::string("live charset registry should survive clone handoff")
    );
    let dim_sym = crate::emacs_core::intern::intern(":dimension");
    assert_eq!(entry.plist[1].0, dim_sym);
    assert_eq!(entry.plist[1].1, value::Value::fixnum(1));
}

#[test]
fn test_dump_buffers_use_symbol_ids_for_buffer_local_bindings() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let current = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .get_mut(current)
        .expect("current buffer mut")
        .set_buffer_local("fill-column", Value::fixnum(80));

    let dump = crate::emacs_core::pdump::convert::dump_evaluator(&eval);
    let dumped = dump
        .buffers
        .buffers
        .iter()
        .find(|(id, _)| id.0 == current.0)
        .map(|(_, buffer)| buffer)
        .expect("dumped current buffer");

    assert!(
        dumped
            .properties_syms
            .iter()
            .any(|(sym_id, _)| sym_id.0 == intern("fill-column").0)
    );
    assert!(
        dumped
            .local_binding_syms
            .iter()
            .any(|sym_id| sym_id.0 == intern("fill-column").0)
    );
    assert!(
        dumped.properties.is_empty(),
        "fresh dumps should not flatten buffer-local names to strings"
    );
    assert!(
        dumped.local_binding_names.is_empty(),
        "fresh dumps should not record buffer-local ordering via legacy string names"
    );
}

#[test]
fn pdump_round_trip_preserves_last_window_start_lisp_position() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let current = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .get_mut(current)
        .expect("current buffer mut")
        .last_window_start = LispCharPos1::from_one_based_usize(7);

    let dump = crate::emacs_core::pdump::convert::dump_evaluator(&eval);
    let dumped = dump
        .buffers
        .buffers
        .iter()
        .find(|(id, _)| id.0 == current.0)
        .map(|(_, buffer)| buffer)
        .expect("dumped current buffer");
    assert_eq!(dumped.last_window_start, Some(7));

    let loaded = restore_snapshot(&dump).expect("restore snapshot should succeed");
    let restored = loaded
        .buffers
        .get(current)
        .expect("restored current buffer");
    assert_eq!(
        restored.last_window_start,
        LispCharPos1::from_one_based_usize(7)
    );
}

#[test]
fn test_dump_modes_use_symbol_ids_for_font_lock_faces() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.modes.register_major_mode(
        "compat-font-lock-mode",
        MajorMode {
            pretty_name: LispString::from_utf8("Compat Font Lock"),
            parent: None,
            mode_hook: Value::symbol("compat-font-lock-mode-hook"),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: Some(FontLockDefaults {
                keywords: vec![FontLockKeyword {
                    pattern: LispString::from_utf8("\\_<compat\\_>"),
                    face: intern("font-lock-keyword-face"),
                    group: 0,
                    override_: false,
                    laxmatch: false,
                }],
                case_fold: false,
                syntax_table: None,
            }),
            body: None,
        },
    );

    let dump = crate::emacs_core::pdump::convert::dump_evaluator(&eval);
    let dumped = dump
        .modes
        .major_modes
        .iter()
        .find(|(sym_id, _)| sym_id.0 == intern("compat-font-lock-mode").0)
        .map(|(_, mode)| mode)
        .expect("dumped compat-font-lock-mode");
    let keyword = dumped
        .font_lock
        .as_ref()
        .and_then(|font_lock| font_lock.keywords.first())
        .expect("dumped font-lock keyword");

    assert_eq!(
        keyword.face_sym,
        Some(DumpSymId(intern("font-lock-keyword-face").0))
    );
    assert!(
        keyword.face.is_none(),
        "fresh dumps should not flatten font-lock faces to strings"
    );
}

#[test]
fn test_file_load_records_pdumper_stats_without_running_after_pdump_load_hook() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let setup = crate::emacs_core::value_reader::read_all(
        "(progn
           (setq compat-pdump-hook-fired nil)
           (setq after-pdump-load-hook
                 (list (lambda () (setq compat-pdump-hook-fired t)))))",
        &test_ob(),
    )
    .unwrap();
    eval.eval_sub(setup[0]).expect("setup hook should evaluate");

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("stats-and-hook.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");
    drop(eval);

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    assert_eq!(
        loaded.obarray.symbol_value("compat-pdump-hook-fired"),
        Some(&Value::NIL)
    );

    let forms = crate::emacs_core::value_reader::read_all("(pdumper-stats)", &test_ob()).unwrap();
    let stats = loaded
        .eval_sub(forms[0])
        .expect("pdumper-stats should evaluate");
    assert!(stats.is_cons(), "pdumper-stats should return an alist");

    let dumped_with = stats.cons_car();
    assert_eq!(dumped_with.cons_car(), Value::symbol("dumped-with-pdumper"));
    assert_eq!(dumped_with.cons_cdr(), Value::T);

    let load_time = stats.cons_cdr().cons_car();
    assert_eq!(load_time.cons_car(), Value::symbol("load-time"));
    assert!(load_time.cons_cdr().is_float());

    let dump_file = stats.cons_cdr().cons_cdr().cons_car();
    assert_eq!(dump_file.cons_car(), Value::symbol("dump-file-name"));
    let expected = dump_path
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        dump_file.cons_cdr().as_str_owned().as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn test_pdump_rejects_fingerprint_mismatch() {
    crate::test_utils::init_test_tracing();
    let eval = Context::new();
    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("fingerprint-mismatch.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let mut bytes = std::fs::read(&dump_path).expect("read dump bytes");
    let fingerprint_start = 16 + 4 + 4 + 4 + 4;
    bytes[fingerprint_start] ^= 0x01;
    std::fs::write(&dump_path, bytes).expect("rewrite dump bytes");

    match load_from_dump(&dump_path) {
        Err(DumpError::FingerprintMismatch { expected, found }) => {
            assert_eq!(expected, fingerprint_hex());
            assert_ne!(expected, found);
        }
        Ok(_) => panic!("expected fingerprint mismatch, but load succeeded"),
        Err(other) => panic!("expected fingerprint mismatch, got {other}"),
    }
}

#[test]
fn baked_stub_template_is_deterministic_and_reads_back_as_stub() {
    use crate::emacs_core::bytecode::chunk::ByteCodeFunction;
    use crate::emacs_core::pdump::mapped_heap::baked_stub_template;

    // Byte determinism is what the layout witness and per-object template
    // comparison ride on: an uninitialized byte reaching the template would
    // make images silently never validate (bootstrap regenerating forever).
    let a = baked_stub_template(0x1234);
    let b = baked_stub_template(0x1234);
    assert_eq!(
        a, b,
        "two bakes of the same template must be byte-identical"
    );
    assert_ne!(
        baked_stub_template(1),
        baked_stub_template(2),
        "closure_slot_count must be part of the baked bytes"
    );

    // The bytes must reconstruct a live stub in this binary's layout —
    // the exact trust the loader places in a mapped struct span.
    let mut slot = std::mem::MaybeUninit::<ByteCodeFunction>::zeroed();
    unsafe {
        std::ptr::copy_nonoverlapping(a.as_ptr(), slot.as_mut_ptr().cast::<u8>(), a.len());
        let stub = slot.assume_init_ref();
        assert!(stub.is_pdump_stub());
        assert_eq!(stub.closure_slot_count, 0x1234);
        assert!(stub.ops.is_empty());
        // No drop: every field owns nothing (MaybeUninit never drops).
    }
}

#[test]
fn test_pdump_rejects_stub_layout_witness_mismatch() {
    crate::test_utils::init_test_tracing();
    let eval = Context::new();
    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("witness-mismatch.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    // The witness is the LAST u64 of the header; corrupting it models an
    // image dumped by a binary whose ByteCodeFunction layout differs. The
    // loader must reject cleanly BEFORE trusting any baked struct bytes.
    let mut bytes = std::fs::read(&dump_path).expect("read dump bytes");
    let witness_start = crate::emacs_core::pdump::mmap_image::header_size_for_test() - 8;
    bytes[witness_start] ^= 0x01;
    std::fs::write(&dump_path, bytes).expect("rewrite dump bytes");

    match load_from_dump(&dump_path) {
        Err(DumpError::ImageFormatError(message)) => {
            assert!(
                message.contains("stub layout witness"),
                "unexpected rejection message: {message}"
            );
        }
        Ok(_) => panic!("expected witness mismatch rejection, but load succeeded"),
        Err(other) => panic!("expected witness mismatch rejection, got {other}"),
    }
}

#[test]
fn test_pdump_bad_magic() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.pdump");
    std::fs::write(&path, b"BADMAGIC").unwrap();
    assert!(matches!(load_from_dump(&path), Err(DumpError::BadMagic)));
}

#[test]
fn test_pdump_round_trip_bootstrap() {
    crate::test_utils::init_test_tracing();
    // Bootstrap, dump, load, and verify eval works on loaded state
    let eval =
        crate::emacs_core::load::create_bootstrap_evaluator().expect("bootstrap should succeed");

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("bootstrap.pdump");

    let dump_start = std::time::Instant::now();
    dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let dump_time = dump_start.elapsed();
    let file_size = std::fs::metadata(&dump_path).unwrap().len();
    eprintln!(
        "pdump: dump took {dump_time:.2?}, file size: {file_size} bytes ({:.1} MB)",
        file_size as f64 / 1048576.0
    );

    // Drop original evaluator before loading to test standalone load
    drop(eval);

    let load_start = std::time::Instant::now();
    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let load_time = load_start.elapsed();
    eprintln!("pdump: load took {load_time:.2?}");

    // Verify the loaded evaluator can evaluate Elisp
    let forms = crate::emacs_core::value_reader::read_all("(+ 1 2)", &test_ob()).unwrap();
    let result = loaded.eval_sub(forms[0]).expect("eval should succeed");
    assert_eq!(result, Value::fixnum(3));

    // Verify features survived (bootstrap sets many features)
    // Note: subr.el does NOT call (provide 'subr); use 'backquote instead
    let forms =
        crate::emacs_core::value_reader::read_all("(featurep 'backquote)", &test_ob()).unwrap();
    let result = loaded.eval_sub(forms[0]).expect("featurep should succeed");
    assert_eq!(result, Value::T, "featurep 'backquote should be t");

    // Verify a bootstrapped function works
    let forms = crate::emacs_core::value_reader::read_all("(length '(a b c))", &test_ob()).unwrap();
    let result = loaded.eval_sub(forms[0]).expect("eval should succeed");
    assert_eq!(result, Value::fixnum(3));

    // Verify string operations (tests heap String objects)
    let forms =
        crate::emacs_core::value_reader::read_all("(concat \"hello\" \" \" \"world\")", &test_ob())
            .unwrap();
    let result = loaded.eval_sub(forms[0]).expect("eval should succeed");
    assert_eq!(crate::emacs_core::print_value(&result), "\"hello world\"");

    // Verify hash table access (tests hash table round-trip)
    let forms = crate::emacs_core::value_reader::read_all(
        "(let ((h (make-hash-table :test 'equal))) (puthash \"key\" 42 h) (gethash \"key\" h))",
        &test_ob(),
    )
    .unwrap();
    let result = loaded.eval_sub(forms[0]).expect("eval should succeed");
    assert_eq!(result, Value::fixnum(42));

    // Verify defun works (tests lambda/macro round-trip)
    let forms = crate::emacs_core::value_reader::read_all(
        "(progn (defun pdump-test-fn (x) (* x x)) (pdump-test-fn 7))",
        &test_ob(),
    )
    .unwrap();
    let result = loaded.eval_sub(forms[0]).expect("eval should succeed");
    assert_eq!(result, Value::fixnum(49));
}

#[test]
fn test_pdump_round_trip_preserves_runtime_derived_mode_syntax() {
    crate::test_utils::init_test_tracing();
    let mut eval =
        crate::emacs_core::load::create_bootstrap_evaluator().expect("bootstrap should succeed");
    crate::emacs_core::load::apply_runtime_startup_state(&mut eval)
        .expect("runtime startup should succeed");

    let probe_src = r#"(list
             (boundp 'lisp-data-mode-syntax-table)
             (boundp 'emacs-lisp-mode-syntax-table)
             (boundp 'lisp-interaction-mode-syntax-table)
             (functionp (symbol-function 'lisp-interaction-mode))
             (eq (char-table-parent emacs-lisp-mode-syntax-table)
                 lisp-data-mode-syntax-table)
             (eq (char-table-parent lisp-interaction-mode-syntax-table)
                 emacs-lisp-mode-syntax-table)
             (eq (string-to-syntax "w")
                 (char-table-range (standard-syntax-table) ?a))
             (eq (string-to-syntax ".")
                 (char-table-range (standard-syntax-table) ?.))
             (char-syntax ?\n)
             (char-syntax ?\;)
             (char-syntax ?{)
             (char-syntax ?'))"#;
    let probe = crate::emacs_core::value_reader::read_all(probe_src, &test_ob()).unwrap();
    let full_result = eval
        .eval_sub(probe[0])
        .expect("full bootstrap probe should run");
    assert_eq!(
        crate::emacs_core::print_value_with_buffers(&full_result, &eval.buffers),
        "(t t t t t t t t 62 60 95 39)"
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("derived-mode-syntax.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");
    drop(eval);

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    crate::emacs_core::load::apply_runtime_startup_state(&mut loaded)
        .expect("runtime startup after load should succeed");

    let probe = crate::emacs_core::value_reader::read_all(probe_src, &test_ob()).unwrap();
    let loaded_result = loaded
        .eval_sub(probe[0])
        .expect("loaded bootstrap probe should run");
    assert_eq!(
        crate::emacs_core::print_value_with_buffers(&loaded_result, &loaded.buffers),
        "(t t t t t t t t 62 60 95 39)"
    );
}

#[test]
fn test_pdump_round_trip_preserves_pre_runtime_standard_syntax_identity() {
    crate::test_utils::init_test_tracing();
    let eval =
        crate::emacs_core::load::create_bootstrap_evaluator().expect("bootstrap should succeed");

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("bootstrap-pre-runtime-syntax.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");
    drop(eval);

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    crate::emacs_core::load::apply_runtime_startup_state(&mut loaded)
        .expect("runtime startup after load should succeed");

    let probe = crate::emacs_core::value_reader::read_all(
        r#"(list
             (eq (char-table-parent emacs-lisp-mode-syntax-table)
                 lisp-data-mode-syntax-table)
             (eq (char-table-parent lisp-interaction-mode-syntax-table)
                 emacs-lisp-mode-syntax-table)
             (eq (string-to-syntax "w")
                 (char-table-range (standard-syntax-table) ?a))
             (eq (string-to-syntax ".")
                 (char-table-range (standard-syntax-table) ?.))
             (char-syntax ?\n)
             (char-syntax ?\;)
             (char-syntax ?{)
             (char-syntax ?'))"#,
        &test_ob(),
    )
    .unwrap();
    let result = loaded
        .eval_sub(probe[0])
        .expect("loaded pre-runtime probe should run");
    assert_eq!(
        crate::emacs_core::print_value_with_buffers(&result, &loaded.buffers),
        "(t t t t 62 60 95 39)"
    );
}

#[test]
fn test_pdump_round_trip_preserves_default_fontset_han_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::load::create_bootstrap_evaluator_with_features(&["neomacs"])
        .expect("bootstrap should succeed");
    let setup = crate::emacs_core::value_reader::read_all(
        r#"(new-fontset
            "fontset-default"
            '((han
               (nil . "GB2312.1980-0")
               (nil . "JISX0208*")
               (nil . "gb18030"))))"#,
        &test_ob(),
    )
    .unwrap();
    eval.eval_sub(setup[0])
        .expect("han-only fontset should install before dump");

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("bootstrap-charsets.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");
    drop(eval);

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let probe = crate::emacs_core::value_reader::read_all(
        r#"(list
            (fontset-font t ?好 t)
            (fontset-font t (string-to-char "好") t))"#,
        &test_ob(),
    )
    .unwrap();
    let result = loaded
        .eval_sub(probe[0])
        .expect("pdump fontset probe should run");
    let rendered = crate::emacs_core::print_value_with_buffers(&result, &loaded.buffers);

    assert!(
        rendered.starts_with(
            "(((nil . \"gb2312.1980-0\") \
              (nil . \"jisx0208*\") \
              (nil . \"gb18030\")) \
             ((nil . \"gb2312.1980-0\") \
              (nil . \"jisx0208*\") \
              (nil . \"gb18030\")))"
        ),
        "unexpected pdump fontset order: {rendered}"
    );
}

#[test]
fn test_restore_snapshot_isolated_between_clones() {
    crate::test_utils::init_test_tracing();
    let template = crate::emacs_core::load::create_bootstrap_evaluator_cached()
        .expect("bootstrap template should succeed");
    let snapshot = snapshot_evaluator(&template);

    let mut first = restore_snapshot(&snapshot).expect("first clone should succeed");
    let setup = crate::emacs_core::value_reader::read_all(
        "(progn
           (setq compat-pdump-clone-smoke 'first)
           compat-pdump-clone-smoke)",
        &test_ob(),
    )
    .unwrap();
    let first_result = first
        .eval_sub(setup[0])
        .expect("first clone evaluation should succeed");
    assert_eq!(
        crate::emacs_core::print_value_with_buffers(&first_result, &first.buffers),
        "first"
    );

    let mut second = restore_snapshot(&snapshot).expect("second clone should succeed");
    let probe =
        crate::emacs_core::value_reader::read_all("(boundp 'compat-pdump-clone-smoke)", &test_ob())
            .unwrap();
    let second_result = second
        .eval_sub(probe[0])
        .expect("second clone evaluation should succeed");
    assert_eq!(
        crate::emacs_core::print_value_with_buffers(&second_result, &second.buffers),
        "nil"
    );
}

#[test]
fn test_restore_snapshot_preserves_core_subr_callable_surface() {
    crate::test_utils::init_test_tracing();
    let template = Context::new();
    let snapshot = snapshot_evaluator(&template);

    let mut restored = restore_snapshot(&snapshot).expect("restored snapshot should succeed");
    let forms = crate::emacs_core::value_reader::read_all(
        r#"(list (funcall 'cons 1 2)
                 (funcall 'list 1 2 3)
                 (funcall 'intern "compat-pdump-subr-probe")
                 (funcall 'format "%s-%s" "pdump" "ok"))"#,
        &test_ob(),
    )
    .expect("parse");
    let result = restored
        .eval_sub(forms[0])
        .expect("restored runtime subrs should be callable");
    assert_eq!(
        crate::emacs_core::print_value_with_buffers(&result, &restored.buffers),
        "((1 . 2) (1 2 3) compat-pdump-subr-probe \"pdump-ok\")"
    );
}

#[test]
fn test_restore_snapshot_preserves_lone_uninterned_symbol_identity() {
    crate::test_utils::init_test_tracing();
    let mut template = Context::new();
    let solo = crate::emacs_core::intern::intern_uninterned("compat-pdump-solo-uninterned");
    template
        .obarray
        .set_symbol_value("compat-pdump-uninterned-holder", Value::from_sym_id(solo));
    let snapshot = snapshot_evaluator(&template);

    let restored = restore_snapshot(&snapshot).expect("restored snapshot should succeed");
    let held = *restored
        .obarray
        .symbol_value("compat-pdump-uninterned-holder")
        .expect("holder binding should exist");
    let held_id = held.as_symbol_id().expect("holder should contain a symbol");
    assert_eq!(
        crate::emacs_core::intern::resolve_sym(held_id),
        "compat-pdump-solo-uninterned"
    );
    assert!(
        !crate::emacs_core::intern::is_canonical_id(held_id),
        "round-tripped lone uninterned symbol should stay uninterned"
    );
}

#[test]
fn test_restore_snapshot_preserves_raw_unibyte_symbol_name_storage() {
    crate::test_utils::init_test_tracing();
    let mut template = Context::new();
    let raw_name = crate::heap_types::LispString::from_unibyte(vec![0xFF, b'a']);
    let uninterned = crate::emacs_core::intern::intern_uninterned_lisp_string(&raw_name);
    let canonical = crate::emacs_core::intern::intern_lisp_string(&raw_name);
    template.obarray.set_symbol_value(
        "compat-pdump-raw-uninterned-holder",
        Value::from_sym_id(uninterned),
    );
    template.obarray.set_symbol_value(
        "compat-pdump-raw-canonical-holder",
        Value::from_sym_id(canonical),
    );
    template.obarray.ensure_interned_global_id(canonical);
    let snapshot = snapshot_evaluator(&template);

    let restored = restore_snapshot(&snapshot).expect("restored snapshot should succeed");

    for (holder, should_be_canonical) in [
        ("compat-pdump-raw-uninterned-holder", false),
        ("compat-pdump-raw-canonical-holder", true),
    ] {
        let held = *restored
            .obarray
            .symbol_value(holder)
            .expect("holder binding should exist");
        let held_id = held.as_symbol_id().expect("holder should contain a symbol");
        let restored_name = crate::emacs_core::intern::resolve_sym_lisp_string(held_id);
        assert_eq!(restored_name.as_bytes(), &[0xFF, b'a']);
        assert!(!restored_name.is_multibyte());
        assert_eq!(
            crate::emacs_core::intern::is_canonical_id(held_id),
            should_be_canonical
        );
    }
}

#[test]
fn test_restore_snapshot_preserves_subr_name_identity_via_name_atoms() {
    crate::test_utils::init_test_tracing();
    let mut template = Context::new();
    let subr = Value::subr(intern("car"));
    template
        .obarray
        .set_symbol_value("compat-pdump-subr-holder", subr);
    let snapshot = snapshot_evaluator(&template);

    let restored = restore_snapshot(&snapshot).expect("restored snapshot should succeed");
    let held = *restored
        .obarray
        .symbol_value("compat-pdump-subr-holder")
        .expect("holder binding should exist");

    assert!(held.is_subr(), "holder should round-trip a subr object");
    assert_eq!(held.as_subr_id(), Some(intern("car")));
}

#[test]
fn test_restore_snapshot_does_not_report_file_based_pdump_session() {
    crate::test_utils::init_test_tracing();
    let mut template = Context::new();
    let setup = crate::emacs_core::value_reader::read_all(
        "(progn
           (setq compat-pdump-snapshot-hook-fired nil)
           (setq after-pdump-load-hook
                 (list (lambda () (setq compat-pdump-snapshot-hook-fired t)))))",
        &test_ob(),
    )
    .unwrap();
    template
        .eval_sub(setup[0])
        .expect("setup hook should evaluate");
    let snapshot = snapshot_evaluator(&template);

    let mut restored = restore_snapshot(&snapshot).expect("restored snapshot should succeed");
    assert_eq!(
        restored
            .obarray
            .symbol_value("compat-pdump-snapshot-hook-fired"),
        Some(&Value::NIL)
    );

    let forms = crate::emacs_core::value_reader::read_all("(pdumper-stats)", &test_ob()).unwrap();
    let stats = restored
        .eval_sub(forms[0])
        .expect("pdumper-stats should evaluate");
    assert!(stats.is_nil());
}

#[test]
fn test_pdump_rejects_corrupt_runtime_managers_section() {
    crate::test_utils::init_test_tracing();
    let result =
        super::runtime_managers_image::load_runtime_managers_section(b"not a runtime section");
    assert!(matches!(result, Err(DumpError::ImageFormatError(_))));
}

#[test]
fn test_restore_snapshot_rejects_duplicate_obarray_symbol_slots() {
    crate::test_utils::init_test_tracing();
    let mut snapshot = snapshot_evaluator(&Context::new());
    let duplicate = snapshot
        .obarray
        .symbols
        .first()
        .cloned()
        .expect("snapshot should contain at least one symbol");
    snapshot.obarray.symbols.push(duplicate);

    let result = restore_snapshot(&snapshot);
    match result {
        Err(DumpError::DeserializationError(message)) => {
            assert!(
                message.contains("duplicate symbol slot"),
                "unexpected error: {message}"
            );
        }
        Ok(_) => panic!("expected deserialization error, got successful restore"),
        Err(err) => panic!("expected deserialization error, got {err}"),
    }
}

#[test]
fn test_restore_snapshot_rejects_global_member_without_symbol_entry() {
    crate::test_utils::init_test_tracing();
    let template = Context::new();
    let dangling = crate::emacs_core::intern::intern_uninterned("compat-pdump-missing-global");
    let mut snapshot = snapshot_evaluator(&template);
    snapshot.obarray.global_members.push(DumpSymId(dangling.0));

    let result = restore_snapshot(&snapshot);
    match result {
        Err(DumpError::DeserializationError(message)) => {
            assert!(
                message.contains("global_members entry references missing symbol slot"),
                "unexpected error: {message}"
            );
        }
        Ok(_) => panic!("expected deserialization error, got successful restore"),
        Err(err) => panic!("expected deserialization error, got {err}"),
    }
}

fn summarize_timings(label: &str, samples: &[std::time::Duration]) {
    let mut millis: Vec<f64> = samples.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    millis.sort_by(|a, b| a.partial_cmp(b).expect("timing values should compare"));
    let count = millis.len();
    let mean = millis.iter().sum::<f64>() / count as f64;
    let min = millis[0];
    let max = millis[count - 1];
    let median = millis[count / 2];
    eprintln!(
        "pdump bench: {label}: mean={mean:.1}ms median={median:.1}ms min={min:.1}ms max={max:.1}ms n={count}"
    );
}

fn measure_timings<T>(iterations: usize, mut op: impl FnMut() -> T) -> Vec<std::time::Duration> {
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = std::time::Instant::now();
        let _ = op();
        samples.push(start.elapsed());
    }
    samples
}

struct CurrentProcessPdumpFixture {
    _dir: tempfile::TempDir,
    final_path: std::path::PathBuf,
    bootstrap_path: std::path::PathBuf,
}

fn current_process_pdump_fixture() -> CurrentProcessPdumpFixture {
    let dir = tempfile::tempdir().expect("pdump fixture tempdir");
    let bootstrap_path = dir.path().join("bootstrap-neomacs.pdump");
    let final_path = dir.path().join("neomacs.pdump");

    crate::emacs_core::load::create_bootstrap_evaluator_cached_at_path(&[], &bootstrap_path)
        .unwrap_or_else(|err| {
            panic!(
                "create bootstrap pdump fixture {}: {err}",
                bootstrap_path.display()
            )
        });

    let eval =
        crate::emacs_core::load::create_runtime_startup_evaluator_at_path(&[], &bootstrap_path)
            .unwrap_or_else(|err| {
                panic!(
                    "create final pdump fixture from bootstrap {}: {err}",
                    bootstrap_path.display()
                )
            });
    dump_to_file(&eval, &final_path)
        .unwrap_or_else(|err| panic!("write final pdump fixture {}: {err}", final_path.display()));

    CurrentProcessPdumpFixture {
        _dir: dir,
        final_path,
        bootstrap_path,
    }
}

#[test]
fn test_measure_current_process_final_pdump_performance() {
    crate::test_utils::init_test_tracing();
    let fixture = current_process_pdump_fixture();
    let final_path = &fixture.final_path;
    let bootstrap_path = &fixture.bootstrap_path;
    let final_size = std::fs::metadata(&final_path)
        .expect("stat final pdump")
        .len();
    let bootstrap_size = std::fs::metadata(&bootstrap_path)
        .expect("stat bootstrap pdump")
        .len();
    eprintln!(
        "pdump bench: final image size={} bytes ({:.1} MiB)",
        final_size,
        final_size as f64 / 1048576.0
    );
    eprintln!(
        "pdump bench: bootstrap image size={} bytes ({:.1} MiB)",
        bootstrap_size,
        bootstrap_size as f64 / 1048576.0
    );

    let iterations = 5;
    let final_raw_load = measure_timings(iterations, || {
        load_from_dump(&final_path).expect("raw final load should succeed")
    });
    summarize_timings("raw final load_from_dump", &final_raw_load);

    let finalized_runtime_load = measure_timings(iterations, || {
        crate::emacs_core::load::load_runtime_image_with_features(
            crate::emacs_core::load::RuntimeImageRole::Final,
            &[],
            Some(&final_path),
        )
        .expect("final runtime image load should succeed")
    });
    summarize_timings("final load+finalize", &finalized_runtime_load);

    let loaded_final = load_from_dump(&final_path).expect("prepare final eval for dump bench");
    let dump_dir = tempfile::tempdir().expect("dump tempdir");
    let mut dump_sizes = Vec::with_capacity(iterations);
    let dump_samples = measure_timings(iterations, || {
        let output = dump_dir
            .path()
            .join(format!("bench-{}.pdump", dump_sizes.len()));
        dump_to_file(&loaded_final, &output).expect("dump should succeed");
        dump_sizes.push(std::fs::metadata(&output).expect("stat dumped image").len());
    });
    summarize_timings("dump_to_file from loaded final image", &dump_samples);
    if let Some(last_size) = dump_sizes.last() {
        eprintln!(
            "pdump bench: dumped bench image size={} bytes ({:.1} MiB)",
            last_size,
            *last_size as f64 / 1048576.0
        );
    }
}

#[test]
fn test_measure_current_process_bootstrap_pdump_raw_load() {
    crate::test_utils::init_test_tracing();
    let fixture = current_process_pdump_fixture();
    let bootstrap_path = &fixture.bootstrap_path;
    let bootstrap_size = std::fs::metadata(&bootstrap_path)
        .expect("stat bootstrap pdump")
        .len();
    eprintln!(
        "pdump bench: bootstrap image size={} bytes ({:.1} MiB)",
        bootstrap_size,
        bootstrap_size as f64 / 1048576.0
    );

    let bootstrap_raw_load = measure_timings(5, || {
        load_from_dump(&bootstrap_path).expect("raw bootstrap load should succeed")
    });
    summarize_timings("raw bootstrap load_from_dump", &bootstrap_raw_load);
}

/// An indirect buffer borrows three things from its base: its text, its undo
/// state, and -- since GNU `record_first_change` follows `b->base_buffer`
/// (`src/undo.c:213-214`) -- the visited-file modtime it records in a
/// `(t . TIME)` entry.  The image format stores each buffer's OWN modtime, so
/// the third link has to be rebuilt on load exactly like the first two.
///
/// The base's modtime is changed AFTER the load on purpose: a restored link
/// that had been rebuilt as a copy would still answer with the dumped value.
#[test]
fn pdump_round_trip_relinks_an_indirect_buffers_visited_file_modtime() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(let ((base (get-buffer-create "base-145")))
                 (set-buffer base)
                 (setq buffer-file-name "/nonesuch/base-145.txt")
                 (set-visited-file-modtime '(25000 1000 0 0))
                 (make-indirect-buffer base "indirect-145")
                 (visited-file-modtime))"#
        )),
        "OK (25000 1000 0 0)"
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("indirect-modtime.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");

    assert_eq!(
        format_eval_result(&loaded.eval_str(
            r#"(progn
                 (set-buffer "base-145")
                 (set-visited-file-modtime '(30000 2000 0 0))
                 (set-buffer-modified-p nil)
                 (setq buffer-undo-list nil)
                 (set-buffer "indirect-145")
                 (insert "X")
                 (list :recorded (cdr (assq t buffer-undo-list))
                       :own-modtime (visited-file-modtime)))"#
        )),
        "OK (:recorded (30000 2000 0 0) :own-modtime 0)",
        "the restored indirect buffer must record its base's CURRENT modtime"
    );
}

#[test]
fn test_failed_load_after_symbol_table_leaves_interner_usable() {
    crate::test_utils::init_test_tracing();
    // A load that fails AFTER the symbol-table section has populated the
    // global interner must (a) return Err instead of panicking, (b) leave
    // the interner usable — cross-process, the mapping is deliberately
    // LEAKED so the `borrowed_alias` name keys keep valid bytes (see
    // `load_from_dump`'s error arm) — and (c) leave the thread-local load
    // remaps cleared so a follow-up load of a good file succeeds.
    let mut eval = Context::new();
    eval.eval_str("(defvar pdleak-canary-var 7)")
        .expect("defvar should evaluate");
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good.pdump");
    dump_to_file(&eval, &good).expect("dump should succeed");

    // Corrupt the object-starts payload ON DISK (that section parses right
    // after the symbol table, so the failure lands exactly in the danger
    // zone). Rebuilding via write_image is no longer possible for this: its
    // bake sweep validates relocation shapes at write time, and re-writing
    // already-baked heap words would double-bake them.
    let bad = dir.path().join("bad.pdump");
    std::fs::copy(&good, &bad).expect("copy should succeed");
    mmap_image::corrupt_section_on_disk_for_test(&bad, mmap_image::DumpSectionKind::ObjectStarts)
        .expect("corruption helper should find the section");

    let err = load_from_dump(&bad);
    assert!(err.is_err(), "corrupted object-starts must fail the load");

    // The interner must still be fully usable after the failed load.
    let fresh = intern("pdleak-post-failure-fresh-name");
    assert_eq!(
        crate::emacs_core::intern::resolve_sym(fresh),
        "pdleak-post-failure-fresh-name"
    );

    // And a follow-up load of the GOOD file must succeed (the failed
    // load's thread-local remaps were cleared by RestoreCleanup).
    let mut restored = load_from_dump(&good).expect("good load after failed load");
    assert_eq!(
        format_eval_result(&restored.eval_str("pdleak-canary-var")),
        "OK 7"
    );
}

#[test]
fn lazy_stub_survives_gc_and_materializes_on_first_call() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    // Hand-assembled GNU function with every extras facet: params (required
    // + optional + rest), docstring, interactive spec, and a constants pool
    // holding a heap cons only the stub's mapped words keep alive.
    // Body: constant0; return  ->  returns the cons.
    let secret = Value::cons(Value::fixnum(77), Value::symbol("pdump-lazy-tail"));
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::intern("pdump-lazy-a")],
        optional: vec![crate::emacs_core::intern::intern("pdump-lazy-b")],
        rest: Some(crate::emacs_core::intern::intern("pdump-lazy-c")),
    });
    function.ops = vec![Op::Constant(0), Op::Return];
    function.constants = vec![secret].into();
    function.max_stack = 8;
    function.lexical = true;
    function.docstring = Some(crate::heap_types::LispString::from_unibyte(
        b"Lazy stub docstring.".to_vec(),
    ));
    function.interactive = Some(Value::string("p"));
    function.gnu_byte_offset_map = Some(vec![
        GnuByteOffsetMapEntry::new(0, 0),
        GnuByteOffsetMapEntry::new(1, 1),
    ]);
    // Bconstant0 (0xC0), Breturn (0x87).
    function.gnu_bytecode_bytes = Some(crate::tagged::header::LispByteVec::owned(vec![0xC0, 0x87]));
    let func_val = Value::make_bytecode(function);
    eval.obarray.set_symbol_value("pdump-lazy-probe", func_val);

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("lazy.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let func = *loaded
        .obarray
        .symbol_value("pdump-lazy-probe")
        .expect("value cell should be restored");
    assert!(func.is_bytecode(), "restored value must be bytecode");
    let was_stub = func.bytecode_data_if_materialized().is_none();

    // The interactive probe must answer WITHOUT materializing.
    let probe = func
        .bytecode_interactive_probe()
        .expect("bytecode probe should answer");
    assert!(
        probe.slot_count >= 6,
        "interactive fn must classify as command (got {})",
        probe.slot_count
    );
    if was_stub {
        assert!(
            func.bytecode_data_if_materialized().is_none(),
            "the interactive probe must not materialize a stub"
        );
    }

    // A full GC cycle with the stub un-materialized: its constants live only
    // in the mapped image words — the stub tracing legs must keep the
    // secret cons alive.
    loaded
        .eval_str("(garbage-collect)")
        .expect("gc with live stubs should succeed");

    // First call materializes and runs; the cons must have survived (UAF
    // check), and the params must have round-tripped as runtime symbols.
    let result = loaded
        .eval_str("(funcall (symbol-value 'pdump-lazy-probe) 5)")
        .expect("first call of a lazy stub should materialize and run");
    assert_eq!(format!("{result}"), "(77 . pdump-lazy-tail)");
    if was_stub {
        let data = func
            .bytecode_data_if_materialized()
            .expect("the call must have materialized the stub");
        assert_eq!(
            crate::emacs_core::intern::resolve_sym(data.params.required[0]),
            "pdump-lazy-a",
            "param symbols must resolve after the fallback-path id rewrite"
        );
        assert_eq!(
            data.params.rest.map(crate::emacs_core::intern::resolve_sym),
            Some("pdump-lazy-c"),
        );
    }
}
#[test]
fn baked_stub_template_is_stack_state_independent() {
    // Regression guard for the bake's core determinism contract: the
    // template must not contain a single byte sourced from uninitialized
    // stack memory. The original whole-value field writes copied None
    // payload bytes from stack temporaries, which differ BY CALL SITE —
    // the dump-time and load-time witness hashes disagreed inside one
    // process. Building the template under deliberately different stack
    // garbage from distinct non-inlined call sites reproduces that class.
    use crate::emacs_core::pdump::mapped_heap::baked_stub_template;
    #[inline(never)]
    fn dirty_stack(seed: u8) -> u64 {
        let junk = [seed ^ 0xA5; 512];
        std::hint::black_box(&junk);
        junk.iter().map(|&b| u64::from(b)).sum()
    }
    #[inline(never)]
    fn site_a() -> Box<[u8]> {
        baked_stub_template(0x77)
    }
    #[inline(never)]
    fn site_b() -> Box<[u8]> {
        let x = dirty_stack(0x3C);
        let t = baked_stub_template(0x77);
        assert!(x > 0);
        t
    }
    let a = site_a();
    std::hint::black_box(dirty_stack(0x99));
    let b = site_b();
    let diffs: Vec<usize> = a
        .iter()
        .zip(b.iter())
        .enumerate()
        .filter(|(_, (x, y))| x != y)
        .map(|(i, _)| i)
        .collect();
    assert!(
        diffs.is_empty(),
        "template bytes differ across stack states at offsets {diffs:?} — \
         an uninitialized byte is reaching the baked image"
    );
}
