use super::*;
use crate::emacs_core::bytecode::chunk::{ByteCodeFunction, GnuByteOffsetMapEntry};
use crate::emacs_core::bytecode::function_slots::FunctionParamsExt as _;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::eval::subr_abi::SubrValueExt;
use neovm_edit::buffer::LispCharPos1;
fn test_ob() -> neovm_value::symbol::Obarray {
    neovm_value::symbol::Obarray::new()
}
use crate::emacs_core::format_eval_result;
use crate::emacs_core::pdump::types::{DumpByteCodeInstructions, DumpHeapObject, DumpSymId};
use neovm_edit::emacs_core::mode::{FontLockDefaults, FontLockKeyword, MajorMode};
use neovm_value::heap_types::{LispString, OverlayDataInit};
use neovm_value::intern::intern;
use neovm_value::value::{
    LambdaParams, StringTextPropertyRun, Value, get_string_text_properties_for_value, list_to_vec,
    set_string_text_properties_for_value,
};

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
    function.gnu_bytecode_bytes = Some(neovm_value::tagged::header::LispByteVec::owned(vec![
        0xC1, 0x87,
    ]));

    let mut eval = Context::new(crate::test_utils::unit_runtime());
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
fn restore_snapshot_rejects_non_keymap_selected_global_map() {
    crate::test_utils::init_test_tracing();
    let template = Context::new(crate::test_utils::unit_runtime());
    let mut snapshot = snapshot_evaluator(&template);
    snapshot.current_global_map = types::DumpValue::Int(42);

    let result = restore_snapshot(&snapshot, template.hooks_installed());
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
fn test_clone_active_evaluator_preserves_in_progress_require_and_load_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    eval.require_stack.push(intern("cl-macs"));
    eval.loads_in_progress
        .push(neovm_value::heap_types::LispString::from_utf8(
            "/tmp/neomacs-pdump-clone-in-progress.el",
        ));

    let cloned = clone_active_evaluator(&mut eval).expect("clone should succeed");

    assert_eq!(cloned.require_stack, vec![intern("cl-macs")]);
    assert_eq!(
        cloned.loads_in_progress,
        vec![neovm_value::heap_types::LispString::from_utf8(
            "/tmp/neomacs-pdump-clone-in-progress.el"
        )]
    );
}

#[test]
fn test_restore_active_runtime_after_clone_reinstalls_live_charset_registry() {
    crate::test_utils::init_test_tracing();
    neovm_text::emacs_core::charset::reset_charset_registry();

    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let mut args = vec![value::Value::NIL; 17];
    args[0] = value::Value::symbol("charset-pdump-clone-restore-test");
    args[1] = value::Value::fixnum(1);
    args[2] = value::Value::vector(vec![value::Value::fixnum(0), value::Value::fixnum(127)]);
    args[16] = value::Value::list(vec![
        value::Value::symbol("doc"),
        value::Value::string("live charset registry should survive clone handoff"),
    ]);
    neovm_text::emacs_core::charset::builtin_define_charset_internal(args).unwrap();

    let live_runtime = snapshot_active_runtime(&mut eval);
    let cloned = clone_active_evaluator(&mut eval).expect("first clone should succeed");
    restore_active_runtime(&mut eval, &live_runtime);
    drop(cloned);

    let cloned_again = clone_active_evaluator(&mut eval).expect("second clone should succeed");
    restore_active_runtime(&mut eval, &live_runtime);
    drop(cloned_again);

    let registry = neovm_text::emacs_core::charset::snapshot_charset_registry();
    let charset_name = neovm_value::intern::intern("charset-pdump-clone-restore-test");
    let doc_key = neovm_value::intern::intern("doc");
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
    let dim_sym = neovm_value::intern::intern(":dimension");
    assert_eq!(entry.plist[1].0, dim_sym);
    assert_eq!(entry.plist[1].1, value::Value::fixnum(1));
}

#[test]
fn test_dump_buffers_use_symbol_ids_for_buffer_local_bindings() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let current = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    eval.edit
        .buffers
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
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let current = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    eval.edit
        .buffers
        .get_mut(current)
        .expect("current buffer mut")
        .set_last_window_start(LispCharPos1::from_one_based_usize(7));

    let dump = crate::emacs_core::pdump::convert::dump_evaluator(&eval);
    let dumped = dump
        .buffers
        .buffers
        .iter()
        .find(|(id, _)| id.0 == current.0)
        .map(|(_, buffer)| buffer)
        .expect("dumped current buffer");
    assert_eq!(dumped.last_window_start, Some(7));

    let loaded =
        restore_snapshot(&dump, eval.hooks_installed()).expect("restore snapshot should succeed");
    let restored = loaded
        .edit
        .buffers
        .get(current)
        .expect("restored current buffer");
    assert_eq!(
        restored.last_window_start(),
        LispCharPos1::from_one_based_usize(7)
    );
}

#[test]
fn pdump_round_trip_preserves_auto_save_failure_time() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let current = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .get_mut(current)
        .expect("current buffer")
        .set_auto_save_failure_time(1_700_000_000);

    let dump = crate::emacs_core::pdump::convert::dump_evaluator(&eval);
    let dumped = dump
        .buffers
        .buffers
        .iter()
        .find(|(id, _)| id.0 == current.0)
        .map(|(_, buffer)| buffer)
        .expect("dumped current buffer");
    assert_eq!(dumped.auto_save_failure_time, 1_700_000_000);

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("auto-save-failure-time.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let loaded = load_from_dump(&dump_path).expect("load should succeed");
    assert_eq!(
        loaded
            .buffers
            .get(current)
            .expect("restored buffer")
            .auto_save_failure_time(),
        1_700_000_000
    );
}

#[test]
fn test_dump_modes_use_symbol_ids_for_font_lock_faces() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new(crate::test_utils::unit_runtime());
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
fn test_restore_snapshot_preserves_core_subr_callable_surface() {
    crate::test_utils::init_test_tracing();
    let template = Context::new(crate::test_utils::unit_runtime());
    let snapshot = snapshot_evaluator(&template);

    let mut restored = restore_snapshot(&snapshot, template.hooks_installed())
        .expect("restored snapshot should succeed");
    let forms = crate::emacs_core::value_reader::read_all(
        r#"(list (funcall 'cons 1 2)
                 (funcall 'list 1 2 3)
                 (funcall 'intern "compat-pdump-subr-probe")
                 (funcall 'format "%s-%s" "pdump" "ok"))"#,
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .expect("parse");
    let result = restored
        .eval_sub(forms[0])
        .expect("restored runtime subrs should be callable");
    assert_eq!(
        crate::emacs_core::print_value_with_buffers(&result, &restored.edit.buffers),
        "((1 . 2) (1 2 3) compat-pdump-subr-probe \"pdump-ok\")"
    );
}

#[test]
fn test_restore_snapshot_preserves_lone_uninterned_symbol_identity() {
    crate::test_utils::init_test_tracing();
    let mut template = Context::new(crate::test_utils::unit_runtime());
    let solo = neovm_value::intern::intern_uninterned("compat-pdump-solo-uninterned");
    template
        .obarray
        .set_symbol_value("compat-pdump-uninterned-holder", Value::from_sym_id(solo));
    let snapshot = snapshot_evaluator(&template);

    let restored = restore_snapshot(&snapshot, template.hooks_installed())
        .expect("restored snapshot should succeed");
    let held = restored
        .obarray
        .symbol_value_copied("compat-pdump-uninterned-holder")
        .expect("holder binding should exist");
    let held_id = held
        .as_bare_symbol_id()
        .expect("holder should contain a symbol");
    assert_eq!(
        neovm_value::intern::resolve_sym(held_id),
        "compat-pdump-solo-uninterned"
    );
    assert!(
        !neovm_value::intern::is_canonical_id(held_id),
        "round-tripped lone uninterned symbol should stay uninterned"
    );
}

#[test]
fn test_restore_snapshot_preserves_raw_unibyte_symbol_name_storage() {
    crate::test_utils::init_test_tracing();
    let mut template = Context::new(crate::test_utils::unit_runtime());
    let raw_name = neovm_value::heap_types::LispString::from_unibyte(vec![0xFF, b'a']);
    let uninterned = neovm_value::intern::intern_uninterned_lisp_string(&raw_name);
    let canonical = neovm_value::intern::intern_lisp_string(&raw_name);
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

    let restored = restore_snapshot(&snapshot, template.hooks_installed())
        .expect("restored snapshot should succeed");

    for (holder, should_be_canonical) in [
        ("compat-pdump-raw-uninterned-holder", false),
        ("compat-pdump-raw-canonical-holder", true),
    ] {
        let held = restored
            .obarray
            .symbol_value_copied(holder)
            .expect("holder binding should exist");
        let held_id = held
            .as_bare_symbol_id()
            .expect("holder should contain a symbol");
        let restored_name = neovm_value::intern::resolve_sym_lisp_string(held_id);
        assert_eq!(restored_name.as_bytes(), &[0xFF, b'a']);
        assert!(!restored_name.is_multibyte());
        assert_eq!(
            neovm_value::intern::is_canonical_id(held_id),
            should_be_canonical
        );
    }
}

#[test]
fn test_restore_snapshot_preserves_subr_name_identity_via_name_atoms() {
    crate::test_utils::init_test_tracing();
    let mut template = Context::new(crate::test_utils::unit_runtime());
    let subr = Value::subr(intern("car"));
    template
        .obarray
        .set_symbol_value("compat-pdump-subr-holder", subr);
    let snapshot = snapshot_evaluator(&template);

    let restored = restore_snapshot(&snapshot, template.hooks_installed())
        .expect("restored snapshot should succeed");
    let held = restored
        .obarray
        .symbol_value_copied("compat-pdump-subr-holder")
        .expect("holder binding should exist");

    assert!(held.is_subr(), "holder should round-trip a subr object");
    assert_eq!(held.as_subr_id(), Some(intern("car")));
}

#[test]
fn test_restore_snapshot_does_not_report_file_based_pdump_session() {
    crate::test_utils::init_test_tracing();
    let mut template = Context::new(crate::test_utils::unit_runtime());
    let setup = crate::emacs_core::value_reader::read_all(
        "(progn
           (setq compat-pdump-snapshot-hook-fired nil)
           (setq after-pdump-load-hook
                 (list (lambda () (setq compat-pdump-snapshot-hook-fired t)))))",
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .unwrap();
    template
        .eval_sub(setup[0])
        .expect("setup hook should evaluate");
    let snapshot = snapshot_evaluator(&template);

    let mut restored = restore_snapshot(&snapshot, template.hooks_installed())
        .expect("restored snapshot should succeed");
    assert_eq!(
        restored
            .obarray
            .symbol_value_copied("compat-pdump-snapshot-hook-fired"),
        Some(Value::NIL)
    );

    let forms = crate::emacs_core::value_reader::read_all(
        "(pdumper-stats)",
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .unwrap();
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

// The duplicate-slot check is inside `#[cfg(debug_assertions)]`
// (`runtime/pdump/convert.rs`), so a release build restores the snapshot
// successfully and this test cannot pass there.
#[cfg(debug_assertions)]
#[test]
fn test_restore_snapshot_rejects_duplicate_obarray_symbol_slots() {
    crate::test_utils::init_test_tracing();
    let template = Context::new(crate::test_utils::unit_runtime());
    let mut snapshot = snapshot_evaluator(&template);
    let duplicate = snapshot
        .obarray
        .symbols
        .first()
        .cloned()
        .expect("snapshot should contain at least one symbol");
    snapshot.obarray.symbols.push(duplicate);

    let result = restore_snapshot(&snapshot, template.hooks_installed());
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

// The rejection now comes from `Obarray::from_dump` in every profile; this
// snapshot-level test stays debug-gated like the duplicate-slot test above,
// and `decode_hardening_test` covers both restore loops in every profile.
#[cfg(debug_assertions)]
#[test]
fn test_restore_snapshot_rejects_global_member_without_symbol_entry() {
    crate::test_utils::init_test_tracing();
    let template = Context::new(crate::test_utils::unit_runtime());
    let dangling = neovm_value::intern::intern_uninterned("compat-pdump-missing-global");
    let mut snapshot = snapshot_evaluator(&template);
    snapshot.obarray.global_members.push(DumpSymId(dangling.0));

    let result = restore_snapshot(&snapshot, template.hooks_installed());
    match result {
        Err(DumpError::DeserializationError(message)) => {
            assert!(
                message.contains("global member references missing symbol slot"),
                "unexpected error: {message}"
            );
        }
        Ok(_) => panic!("expected deserialization error, got successful restore"),
        Err(err) => panic!("expected deserialization error, got {err}"),
    }
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
    let mut eval = Context::new(crate::test_utils::unit_runtime());
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

    let err = load_from_dump_with_proof(&bad, crate::test_utils::unit_runtime());
    assert!(err.is_err(), "corrupted object-starts must fail the load");

    // The interner must still be fully usable after the failed load.
    let fresh = intern("pdleak-post-failure-fresh-name");
    assert_eq!(
        neovm_value::intern::resolve_sym(fresh),
        "pdleak-post-failure-fresh-name"
    );

    // And a follow-up load of the GOOD file must succeed (the failed
    // load's thread-local remaps were cleared by RestoreCleanup).
    let mut restored = load_from_dump_with_proof(&good, crate::test_utils::unit_runtime())
        .expect("good load after failed load");
    assert_eq!(
        format_eval_result(&restored.eval_str("pdleak-canary-var")),
        "OK 7"
    );
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

#[cfg(test)]
#[path = "decode_hardening_test.rs"]
mod decode_hardening;

#[path = "bytecode_function_params.rs"]
mod bytecode_function_params;
