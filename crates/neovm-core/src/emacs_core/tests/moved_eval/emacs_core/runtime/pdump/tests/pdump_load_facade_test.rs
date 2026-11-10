//! Pdump tests whose subject is the core load facade.
//!
//! `load_from_dump` and the bootstrap / runtime-image constructors live in
//! core's `load_boot`. Eval cannot name that crate. Test names and
//! assertions are unchanged. The bare harness is core's `new_context`:
//! eval's unit-runtime module stays `cfg(test)`, so a dependency build
//! cannot see it. Symbol cells go through `obarray` / `obarray_mut`
//! because the obarray field is `pub(crate)`. Buffer state is the public
//! `edit.buffers` field. The auto-save failure-time round trip also asserts
//! the dumped image field, through the existing `dump_evaluator` surface.

use neovm_edit::buffer::LispCharPos1;
use neovm_value::heap_types::LispMarker;
use neovm_value::intern::intern;
use neovm_value::value::Value;

fn test_ob() -> neovm_value::symbol::Obarray {
    neovm_value::symbol::Obarray::new()
}

#[test]
fn test_pdump_round_trip_basic() {
    crate::test_utils::init_test_tracing();
    // Create a minimal evaluator
    let mut eval = crate::test_utils::new_context();

    // Set a symbol value to verify round-trip
    eval.obarray_mut()
        .set_symbol_value("test-pdump-var", Value::fixnum(42));

    // Dump to temp file
    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("test.pdump");
    crate::emacs_core::pdump::dump_to_file(&eval, &dump_path).expect("dump should succeed");

    // Load from dump
    let loaded =
        crate::emacs_core::load_boot::load_from_dump(&dump_path).expect("load should succeed");

    // Verify the symbol value survived
    assert_eq!(
        loaded.obarray().symbol_value_copied("test-pdump-var"),
        Some(Value::fixnum(42))
    );
}

#[test]
fn pdump_round_trip_rebuilds_native_boolean_forwarders() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    assert_eq!(
        crate::emacs_core::print::format_eval_result(
            &eval.eval_str("(progn (setq inhibit-message :dumped) inhibit-message)")
        ),
        "OK t"
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("native-boolean.pdump");
    crate::emacs_core::pdump::dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let mut loaded =
        crate::emacs_core::load_boot::load_from_dump(&dump_path).expect("load should succeed");

    let result = loaded.eval_str(
        "(list inhibit-message
               (set 'inhibit-message nil)
               inhibit-message
               (set 'inhibit-message :after-load)
               inhibit-message)",
    );
    assert_eq!(
        crate::emacs_core::print::format_eval_result(&result),
        "OK (t nil nil :after-load t)"
    );
}

#[test]
fn pdump_round_trip_preserves_selected_global_map_separately_from_lisp_variable() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.eval_str(
        "(progn
           (setq pdump-selected-global-map (make-keymap))
           (use-global-map pdump-selected-global-map)
           (setq global-map (make-keymap)))",
    )
    .expect("global keymap setup should succeed");

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("selected-global-map.pdump");
    crate::emacs_core::pdump::dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let mut loaded =
        crate::emacs_core::load_boot::load_from_dump(&dump_path).expect("load should succeed");

    let restored = loaded.eval_str(
        "(list
           (eq (current-global-map) pdump-selected-global-map)
           (eq (current-global-map) global-map)
           (keymapp (current-global-map)))",
    );
    assert_eq!(
        crate::emacs_core::print::format_eval_result(&restored),
        "OK (t nil t)"
    );
}

#[test]
fn pdump_round_trip_preserves_buffer_text_backend_kind() {
    crate::test_utils::init_test_tracing();
    for backend_kind in neovm_edit::buffer::BufferTextBackendKind::non_gap_implemented_variants() {
        let backend = backend_kind.symbol_name();
        let mut eval = crate::test_utils::new_context();
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
            crate::emacs_core::print::format_eval_result(&setup),
            format!(r#"OK ({backend} "éabc")"#)
        );

        let dir = tempfile::tempdir().unwrap();
        let dump_path = dir.path().join(format!("{backend}-text-backend.pdump"));
        crate::emacs_core::pdump::dump_to_file(&eval, &dump_path).expect("dump should succeed");

        let mut loaded =
            crate::emacs_core::load_boot::load_from_dump(&dump_path).expect("load should succeed");
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
            crate::emacs_core::print::format_eval_result(&restored),
            format!(r#"OK ({backend} ({backend} "éabc") ({backend} "z"))"#)
        );
    }
}

/// The load image must restore marker identity, not just marker-shaped
/// values: positions, insertion types and the buffer mark all round-trip,
/// and the restored objects are spliced back into the buffer's chain, so
/// they keep tracking edits.
#[test]
fn file_pdump_round_trips_marker_positions_insertion_types_and_mark() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let buffer_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("scratch buffer");
    eval.edit
        .buffers
        .insert_into_buffer(buffer_id, "abcdefghij")
        .expect("insert text");

    // One Before marker at Lisp position 3, one After marker at position 5.
    let before_marker = neovm_edit::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.edit.buffers,
        buffer_id,
        LispCharPos1::new(3),
        false,
    );
    let after_marker = neovm_edit::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.edit.buffers,
        buffer_id,
        LispCharPos1::new(5),
        true,
    );
    eval.obarray_mut()
        .set_symbol_value("test-pdump-marker-before", before_marker);
    eval.obarray_mut()
        .set_symbol_value("test-pdump-marker-after", after_marker);

    // Mark at Emacs byte 4 (0-based char 4).
    eval.edit
        .buffers
        .set_buffer_mark_emacs_byte_pos(buffer_id, neovm_edit::buffer::EmacsBytePos::new(4))
        .expect("set mark");
    let mark = neovm_edit::emacs_core::marker::builtin_mark_marker(&mut eval.edit.buffers, vec![])
        .expect("mark-marker");
    eval.obarray_mut().set_symbol_value("test-pdump-mark", mark);

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("marker-roundtrip.pdump");
    crate::emacs_core::pdump::dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let mut loaded =
        crate::emacs_core::load_boot::load_from_dump(&dump_path).expect("load should succeed");

    let before_marker = loaded
        .obarray()
        .symbol_value_copied("test-pdump-marker-before")
        .expect("restored before marker");
    let after_marker = loaded
        .obarray()
        .symbol_value_copied("test-pdump-marker-after")
        .expect("restored after marker");
    let mark = loaded
        .obarray()
        .symbol_value_copied("test-pdump-mark")
        .expect("restored mark marker");

    for (marker, charpos, insertion_type) in [
        (before_marker, 2usize, false),
        (after_marker, 4usize, true),
        (mark, 4usize, false),
    ] {
        let data = marker.as_marker_data().expect("marker payload");
        assert_eq!(data.buffer_id(), Some(buffer_id), "buffer round-trip");
        assert_eq!(data.charpos.get(), charpos, "char position round-trip");
        assert_eq!(
            data.insertion_type(),
            insertion_type,
            "insertion type round-trip"
        );
        assert!(data.chained(), "the marker is back on the buffer chain");
    }

    // The buffer's mark reference names the same object Lisp holds.
    let mark_ref = loaded
        .edit
        .buffers
        .get(buffer_id)
        .expect("restored buffer")
        .mark_marker();
    assert_eq!(
        mark_ref.as_ptr(),
        mark.as_veclike_ptr().expect("mark veclike pointer")
            as *mut neovm_value::tagged::header::MarkerObj,
        "the restored buffer mark is the dumped marker object"
    );

    // The restored chain is live: inserting two chars at the start moves the
    // markers by two, each through its own chain entry.
    loaded
        .edit
        .buffers
        .goto_buffer_emacs_byte_pos(buffer_id, neovm_edit::buffer::EmacsBytePos::new(0))
        .expect("goto start");
    loaded
        .edit
        .buffers
        .insert_into_buffer(buffer_id, "XY")
        .expect("insert at start");
    assert_eq!(
        before_marker.as_marker_data().unwrap().charpos.get(),
        4,
        "Before marker follows the inserted text"
    );
    assert_eq!(
        after_marker.as_marker_data().unwrap().charpos.get(),
        6,
        "After marker follows the inserted text"
    );
    assert_eq!(
        neovm_edit::emacs_core::marker::marker_position_as_int_with_buffers(
            &loaded.edit.buffers,
            &mark
        )
        .expect("mark position after edit"),
        7,
        "mark moved with its marker object"
    );
}

/// A marker that lives in the mapped image and that Lisp later points into
/// a buffer (as `view-lossage` does with `help-window-point-marker`) must
/// stay in the buffer's marker chain across collections. Its header mark bit
/// keeps the value the loader wrote -- the image's mark is the side table --
/// so a chain walk that read the header spliced it out at the first GC, and
/// from then on it no longer moved with insertions (GNU: it does). Checked
/// over three cycles, both parities.
#[test]
fn a_mapped_marker_set_into_a_buffer_keeps_tracking_edits_across_gcs() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.obarray_mut().set_symbol_value(
        "test-pdump-chained-marker",
        Value::make_marker(LispMarker::new(
            neovm_value::heap_types::MarkerBuffer::DETACHED,
            neovm_edit::buffer::CharPos0::new(1),
            neovm_edit::buffer::EmacsBytePos::new(1),
            false,
            true,
        )),
    );
    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("chained-marker.pdump");
    crate::emacs_core::pdump::dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let mut loaded =
        crate::emacs_core::load_boot::load_from_dump(&dump_path).expect("load should succeed");
    let value = loaded
        .obarray()
        .symbol_value_copied("test-pdump-chained-marker")
        .expect("restored marker symbol");
    assert!(
        loaded.pdump_image_contains_ptr(value.as_veclike_ptr().unwrap().cast::<u8>()),
        "the marker must live in the mapped image for this test to mean anything"
    );
    loaded
        .eval_str(
            "(progn (set-buffer (get-buffer-create \" p3-markers\"))
                    (insert \"0123456789\")
                    (set-marker test-pdump-chained-marker 6))",
        )
        .expect("point the image marker into a buffer");
    for round in 0..3i64 {
        loaded.gc_collect_exact();
        let position = loaded
            .eval_str(
                "(progn (goto-char 1) (insert \"B\") (marker-position test-pdump-chained-marker))",
            )
            .expect("insert before the marker");
        assert_eq!(
            position.as_fixnum(),
            Some(7 + round),
            "after GC {} the image marker must have moved with the insertion",
            round + 1
        );
    }
}

#[test]
fn pdump_dumps_default_value_for_active_dynamic_plain_binding() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();

    let sym = intern("pdump-dynamic-plain-var");
    eval.obarray_mut()
        .set_symbol_value_id(sym, Value::symbol("default-value"));
    eval.try_specbind(sym, Value::symbol("dynamic-value"))
        .expect("dynamic binding");

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("dynamic-plain.pdump");
    crate::emacs_core::pdump::dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let loaded =
        crate::emacs_core::load_boot::load_from_dump(&dump_path).expect("load should succeed");
    assert_eq!(
        loaded.obarray().symbol_value_id_copied(sym),
        Some(Value::symbol("default-value")),
        "pdump must serialize the top-level value, not the active dynamic binding"
    );
}

/// Handing the thread back to the live evaluator after a clone rebuilds the
/// terminal thread-locals in the live heap. The clone's pdump restore had
/// replaced them with a terminal whose handle lives in the CLONE's heap, so
/// the live evaluator's `terminal-list` pointed into freed memory once the
/// clone was dropped. The bootstrap cache's failed-reload fallback hands the
/// thread back the same way.
#[test]
fn restoring_the_live_runtime_after_a_clone_rebuilds_the_terminal_in_the_live_heap() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let live_runtime = crate::emacs_core::pdump::snapshot_active_runtime(&mut eval);
    let cloned =
        crate::emacs_core::pdump::clone_active_evaluator(&mut eval).expect("clone should succeed");
    crate::emacs_core::pdump::restore_active_runtime(&mut eval, &live_runtime);
    let handle = crate::emacs_core::terminal::pure::terminal_handle_value();
    assert!(
        neovm_value::tagged::gc::with_tagged_heap(|heap| heap.owns_heap_value_for_test(handle)),
        "the terminal handle must live in the reinstalled heap"
    );
    drop(cloned);
    let name = eval
        .eval_str("(progn (garbage-collect) (terminal-name (car (terminal-list))))")
        .expect("terminal-list answers after the clone is gone");
    assert_eq!(
        name.as_utf8_str().map(str::to_owned),
        Some("initial_terminal".to_owned())
    );
}

#[test]
fn test_file_load_records_pdumper_stats_without_running_after_pdump_load_hook() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let setup = crate::emacs_core::value_reader::read_all(
        "(progn
           (setq compat-pdump-hook-fired nil)
           (setq after-pdump-load-hook
                 (list (lambda () (setq compat-pdump-hook-fired t)))))",
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .unwrap();
    eval.eval_sub(setup[0]).expect("setup hook should evaluate");

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("stats-and-hook.pdump");
    crate::emacs_core::pdump::dump_to_file(&eval, &dump_path).expect("dump should succeed");
    drop(eval);

    let mut loaded =
        crate::emacs_core::load_boot::load_from_dump(&dump_path).expect("load should succeed");
    assert_eq!(
        loaded
            .obarray()
            .symbol_value_copied("compat-pdump-hook-fired"),
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
    let eval = crate::test_utils::new_context();
    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("fingerprint-mismatch.pdump");
    crate::emacs_core::pdump::dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let mut bytes = std::fs::read(&dump_path).expect("read dump bytes");
    let fingerprint_start = 16 + 4 + 4 + 4 + 4;
    bytes[fingerprint_start] ^= 0x01;
    std::fs::write(&dump_path, bytes).expect("rewrite dump bytes");

    match crate::emacs_core::load_boot::load_from_dump(&dump_path) {
        Err(crate::emacs_core::pdump::DumpError::FingerprintMismatch { expected, found }) => {
            assert_eq!(expected, crate::emacs_core::pdump::fingerprint_hex());
            assert_ne!(expected, found);
        }
        Ok(_) => panic!("expected fingerprint mismatch, but load succeeded"),
        Err(other) => panic!("expected fingerprint mismatch, got {other}"),
    }
}

#[test]
fn test_pdump_bad_magic() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.pdump");
    std::fs::write(&path, b"BADMAGIC").unwrap();
    assert!(matches!(
        crate::emacs_core::load_boot::load_from_dump(&path),
        Err(crate::emacs_core::pdump::DumpError::BadMagic)
    ));
}

#[test]
fn test_pdump_round_trip_bootstrap() {
    crate::test_utils::init_test_tracing();
    // Bootstrap, dump, load, and verify eval works on loaded state
    let eval = crate::emacs_core::load_boot::create_bootstrap_evaluator()
        .expect("bootstrap should succeed");

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("bootstrap.pdump");

    let dump_start = std::time::Instant::now();
    crate::emacs_core::pdump::dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let dump_time = dump_start.elapsed();
    let file_size = std::fs::metadata(&dump_path).unwrap().len();
    eprintln!(
        "pdump: dump took {dump_time:.2?}, file size: {file_size} bytes ({:.1} MB)",
        file_size as f64 / 1048576.0
    );

    // Drop original evaluator before loading to test standalone load
    drop(eval);

    let load_start = std::time::Instant::now();
    let mut loaded =
        crate::emacs_core::load_boot::load_from_dump(&dump_path).expect("load should succeed");
    let load_time = load_start.elapsed();
    eprintln!("pdump: load took {load_time:.2?}");

    // Verify the loaded evaluator can evaluate Elisp
    let forms = crate::emacs_core::value_reader::read_all(
        "(+ 1 2)",
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .unwrap();
    let result = loaded.eval_sub(forms[0]).expect("eval should succeed");
    assert_eq!(result, Value::fixnum(3));

    // Verify features survived (bootstrap sets many features)
    // Note: subr.el does NOT call (provide 'subr); use 'backquote instead
    let forms = crate::emacs_core::value_reader::read_all(
        "(featurep 'backquote)",
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .unwrap();
    let result = loaded.eval_sub(forms[0]).expect("featurep should succeed");
    assert_eq!(result, Value::T, "featurep 'backquote should be t");

    // Verify a bootstrapped function works
    let forms = crate::emacs_core::value_reader::read_all(
        "(length '(a b c))",
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .unwrap();
    let result = loaded.eval_sub(forms[0]).expect("eval should succeed");
    assert_eq!(result, Value::fixnum(3));

    // Verify string operations (tests heap String objects)
    let forms = crate::emacs_core::value_reader::read_all(
        "(concat \"hello\" \" \" \"world\")",
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .unwrap();
    let result = loaded.eval_sub(forms[0]).expect("eval should succeed");
    assert_eq!(
        crate::emacs_core::print::print_value(&result),
        "\"hello world\""
    );

    // Verify hash table access (tests hash table round-trip)
    let forms = crate::emacs_core::value_reader::read_all(
        "(let ((h (make-hash-table :test 'equal))) (puthash \"key\" 42 h) (gethash \"key\" h))",
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .unwrap();
    let result = loaded.eval_sub(forms[0]).expect("eval should succeed");
    assert_eq!(result, Value::fixnum(42));

    // Verify defun works (tests lambda/macro round-trip)
    let forms = crate::emacs_core::value_reader::read_all(
        "(progn (defun pdump-test-fn (x) (* x x)) (pdump-test-fn 7))",
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .unwrap();
    let result = loaded.eval_sub(forms[0]).expect("eval should succeed");
    assert_eq!(result, Value::fixnum(49));
}

#[test]
fn test_pdump_round_trip_preserves_runtime_derived_mode_syntax() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::load_boot::create_bootstrap_evaluator()
        .expect("bootstrap should succeed");
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
    let probe = crate::emacs_core::value_reader::read_all(
        probe_src,
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .unwrap();
    let full_result = eval
        .eval_sub(probe[0])
        .expect("full bootstrap probe should run");
    assert_eq!(
        crate::emacs_core::print::print_value_with_buffers(&full_result, &eval.edit.buffers),
        "(t t t t t t t t 62 60 95 39)"
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("derived-mode-syntax.pdump");
    crate::emacs_core::pdump::dump_to_file(&eval, &dump_path).expect("dump should succeed");
    drop(eval);

    let mut loaded =
        crate::emacs_core::load_boot::load_from_dump(&dump_path).expect("load should succeed");
    crate::emacs_core::load::apply_runtime_startup_state(&mut loaded)
        .expect("runtime startup after load should succeed");

    let probe = crate::emacs_core::value_reader::read_all(
        probe_src,
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .unwrap();
    let loaded_result = loaded
        .eval_sub(probe[0])
        .expect("loaded bootstrap probe should run");
    assert_eq!(
        crate::emacs_core::print::print_value_with_buffers(&loaded_result, &loaded.edit.buffers),
        "(t t t t t t t t 62 60 95 39)"
    );
}

#[test]
fn test_pdump_round_trip_preserves_pre_runtime_standard_syntax_identity() {
    crate::test_utils::init_test_tracing();
    let eval = crate::emacs_core::load_boot::create_bootstrap_evaluator()
        .expect("bootstrap should succeed");

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("bootstrap-pre-runtime-syntax.pdump");
    crate::emacs_core::pdump::dump_to_file(&eval, &dump_path).expect("dump should succeed");
    drop(eval);

    let mut loaded =
        crate::emacs_core::load_boot::load_from_dump(&dump_path).expect("load should succeed");
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
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .unwrap();
    let result = loaded
        .eval_sub(probe[0])
        .expect("loaded pre-runtime probe should run");
    assert_eq!(
        crate::emacs_core::print::print_value_with_buffers(&result, &loaded.edit.buffers),
        "(t t t t 62 60 95 39)"
    );
}

#[test]
fn test_pdump_round_trip_preserves_default_fontset_han_order() {
    crate::test_utils::init_test_tracing();
    let mut eval =
        crate::emacs_core::load_boot::create_bootstrap_evaluator_with_features(&["neomacs"])
            .expect("bootstrap should succeed");
    let setup = crate::emacs_core::value_reader::read_all(
        r#"(new-fontset
            "fontset-default"
            '((han
               (nil . "GB2312.1980-0")
               (nil . "JISX0208*")
               (nil . "gb18030"))))"#,
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .unwrap();
    eval.eval_sub(setup[0])
        .expect("han-only fontset should install before dump");

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("bootstrap-charsets.pdump");
    crate::emacs_core::pdump::dump_to_file(&eval, &dump_path).expect("dump should succeed");
    drop(eval);

    let mut loaded =
        crate::emacs_core::load_boot::load_from_dump(&dump_path).expect("load should succeed");
    let probe = crate::emacs_core::value_reader::read_all(
        r#"(list
            (fontset-font t ?好 t)
            (fontset-font t (string-to-char "好") t))"#,
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .unwrap();
    let result = loaded
        .eval_sub(probe[0])
        .expect("pdump fontset probe should run");
    let rendered =
        crate::emacs_core::print::print_value_with_buffers(&result, &loaded.edit.buffers);

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
    let template = crate::emacs_core::load_boot::create_bootstrap_evaluator_cached()
        .expect("bootstrap template should succeed");
    let snapshot = crate::emacs_core::pdump::snapshot_evaluator(&template);

    let mut first =
        crate::emacs_core::pdump::restore_snapshot(&snapshot, template.hooks_installed())
            .expect("first clone should succeed");
    let setup = crate::emacs_core::value_reader::read_all(
        "(progn
           (setq compat-pdump-clone-smoke 'first)
           compat-pdump-clone-smoke)",
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .unwrap();
    let first_result = first
        .eval_sub(setup[0])
        .expect("first clone evaluation should succeed");
    assert_eq!(
        crate::emacs_core::print::print_value_with_buffers(&first_result, &first.edit.buffers),
        "first"
    );

    let mut second =
        crate::emacs_core::pdump::restore_snapshot(&snapshot, template.hooks_installed())
            .expect("second clone should succeed");
    let probe = crate::emacs_core::value_reader::read_all(
        "(boundp 'compat-pdump-clone-smoke)",
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &test_ob(),
        ),
    )
    .unwrap();
    let second_result = second
        .eval_sub(probe[0])
        .expect("second clone evaluation should succeed");
    assert_eq!(
        crate::emacs_core::print::print_value_with_buffers(&second_result, &second.edit.buffers),
        "nil"
    );
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

    crate::emacs_core::load_boot::create_bootstrap_evaluator_cached_at_path(&[], &bootstrap_path)
        .unwrap_or_else(|err| {
            panic!(
                "create bootstrap pdump fixture {}: {err}",
                bootstrap_path.display()
            )
        });

    let eval = crate::emacs_core::load_boot::create_runtime_startup_evaluator_at_path(
        &[],
        &bootstrap_path,
    )
    .unwrap_or_else(|err| {
        panic!(
            "create final pdump fixture from bootstrap {}: {err}",
            bootstrap_path.display()
        )
    });
    crate::emacs_core::pdump::dump_to_file(&eval, &final_path)
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
    let final_size = std::fs::metadata(final_path)
        .expect("stat final pdump")
        .len();
    let bootstrap_size = std::fs::metadata(bootstrap_path)
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
        crate::emacs_core::load_boot::load_from_dump(final_path)
            .expect("raw final load should succeed")
    });
    summarize_timings("raw final load_from_dump", &final_raw_load);

    let finalized_runtime_load = measure_timings(iterations, || {
        crate::emacs_core::load_boot::load_runtime_image_with_features(
            crate::emacs_core::load::RuntimeImageRole::Final,
            &[],
            Some(final_path),
        )
        .expect("final runtime image load should succeed")
    });
    summarize_timings("final load+finalize", &finalized_runtime_load);

    let loaded_final = crate::emacs_core::load_boot::load_from_dump(final_path)
        .expect("prepare final eval for dump bench");
    let dump_dir = tempfile::tempdir().expect("dump tempdir");
    let mut dump_sizes = Vec::with_capacity(iterations);
    let dump_samples = measure_timings(iterations, || {
        let output = dump_dir
            .path()
            .join(format!("bench-{}.pdump", dump_sizes.len()));
        crate::emacs_core::pdump::dump_to_file(&loaded_final, &output)
            .expect("dump should succeed");
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
    let bootstrap_size = std::fs::metadata(bootstrap_path)
        .expect("stat bootstrap pdump")
        .len();
    eprintln!(
        "pdump bench: bootstrap image size={} bytes ({:.1} MiB)",
        bootstrap_size,
        bootstrap_size as f64 / 1048576.0
    );

    let bootstrap_raw_load = measure_timings(5, || {
        crate::emacs_core::load_boot::load_from_dump(bootstrap_path)
            .expect("raw bootstrap load should succeed")
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
    let mut eval = crate::test_utils::new_context();
    assert_eq!(
        crate::emacs_core::print::format_eval_result(&eval.eval_str(
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
    crate::emacs_core::pdump::dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let mut loaded =
        crate::emacs_core::load_boot::load_from_dump(&dump_path).expect("load should succeed");

    assert_eq!(
        crate::emacs_core::print::format_eval_result(&loaded.eval_str(
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
fn pdump_round_trip_keeps_the_well_known_symbol_table_identity() {
    use neovm_value::intern::{
        FIRST_WELL_KNOWN_SYM_ID, WELL_KNOWN_NAMES, WellKnown, intern, is_canonical_id,
        lookup_interned, resolve_sym,
    };
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    // A value written through a fixed id before the dump must come back at
    // the same identity, not re-resolved by name.
    let probe = WellKnown::ModeLine;
    eval.assign_by_id(probe.id(), Value::fixnum(7));
    assert_eq!(
        eval.obarray().symbol_value_id_copied(probe.id()),
        Some(Value::fixnum(7))
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("well-known-table.pdump");
    crate::emacs_core::pdump::dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let loaded =
        crate::emacs_core::load_boot::load_from_dump(&dump_path).expect("load should succeed");

    assert_eq!(WELL_KNOWN_NAMES.len(), WellKnown::COUNT);
    for (index, variant) in WellKnown::VARIANTS.iter().copied().enumerate() {
        let id = variant.id();
        assert_eq!(id.to_raw(), FIRST_WELL_KNOWN_SYM_ID + index as u32);
        assert_eq!(variant.name(), WELL_KNOWN_NAMES[index]);
        // The constructor seed prefix restored by position: name, canonical
        // mapping and the fixed id all agree after load.
        assert_eq!(resolve_sym(id), variant.name(), "{variant:?}");
        assert!(is_canonical_id(id), "{variant:?}");
        assert!(
            loaded.obarray().is_global_member(id),
            "{variant:?} keeps GNU's initial-obarray membership across the dump"
        );
        assert_eq!(lookup_interned(variant.name()), Some(id), "{variant:?}");
        assert_eq!(intern(variant.name()), id, "{variant:?}");
        assert_eq!(
            Value::symbol(variant).bits(),
            Value::symbol(variant.name()).bits(),
            "{variant:?}"
        );
    }
    assert_eq!(
        loaded.obarray().symbol_value_id_copied(probe.id()),
        Some(Value::fixnum(7)),
        "the dumped value followed the fixed identity"
    );
    assert_eq!(
        loaded
            .obarray()
            .symbol_value_id_copied(intern(probe.name())),
        Some(Value::fixnum(7))
    );
}

#[test]
fn pdump_round_trip_preserves_auto_save_failure_time() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let current = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    eval.edit
        .buffers
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
    crate::emacs_core::pdump::dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let loaded =
        crate::emacs_core::load_boot::load_from_dump(&dump_path).expect("load should succeed");
    assert_eq!(
        loaded
            .edit
            .buffers
            .get(current)
            .expect("restored buffer")
            .auto_save_failure_time(),
        1_700_000_000
    );
}
