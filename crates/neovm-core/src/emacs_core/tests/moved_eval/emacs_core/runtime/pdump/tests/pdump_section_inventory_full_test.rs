//! The file pdump's raw mmap sections cover every state slice the loader
//! needs: the symbol table, mapped-object starts and extra metadata, the
//! obarray, charset, coding systems, faces, buffers, roots, autoloads and
//! runtime managers, and no monolithic RuntimeState section.
//! (This is the section-coverage FULL test of eval's pdump_test.rs; its
//! white-box section walk now goes through the single copied inspection
//! reader raw_section_inventory_for_test, peer option (d).)
//
// API-drift shims (post-split re-home): the pre-split test reached
// `Context`, `dump_to_file`, `load_from_dump` and the eight section loaders
// through the pdump test module. The evaluator is now built through the core
// test facade, `load_from_dump` lives in core's load_boot, the image loader
// rides the mmap test-support facade, and the section walk goes through the
// one copied inventory reader (whose assertion map is in the delivery
// ready); no loader or decoded type crosses the crate boundary.
use crate::emacs_core::load_boot::load_from_dump;
use crate::emacs_core::pdump::dump_to_file;
use crate::emacs_core::pdump::mmap_image_test_support as mmap_image;
use neovm_value::value::Value;

#[test]
fn file_pdump_stores_symbol_table_in_raw_mmap_section() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.obarray_mut()
        .set_symbol_value("pdump-symbol-section-probe", Value::fixnum(71));

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("symbol-table-section.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let image = mmap_image::load_image(&dump_path).expect("load raw mmap image");
    let inventory = neovm_eval::test_support::raw_section_inventory_for_test(&image);

    assert!(
        inventory.symbol_table_present,
        "file pdumps must carry the symbol interner in a raw mmap section"
    );
    assert!(
        inventory.object_starts_present,
        "file pdumps must carry mapped heap object starts in a raw mmap section"
    );
    assert!(
        inventory.object_extra_objects > 0,
        "file pdumps must carry compact non-HeapImage object metadata"
    );
    assert!(
        inventory.obarray_symbols > 0,
        "file pdumps must carry obarray symbol state in a raw mmap section"
    );
    assert!(
        inventory.charset_payload_bytes > 0,
        "file pdumps must carry charset registry state in a raw mmap section"
    );
    assert!(
        inventory.coding_systems_payload_bytes > 0,
        "file pdumps must carry coding system state in a raw mmap section"
    );
    assert!(
        inventory.face_table_payload_bytes > 0,
        "file pdumps must carry Lisp face state in a raw mmap section"
    );
    assert!(
        inventory.buffers_payload_bytes > 0,
        "file pdumps must carry buffer state in a raw mmap section"
    );
    assert!(
        inventory.roots_payload_bytes > 0,
        "file pdumps must carry top-level Lisp roots in a raw mmap section"
    );
    assert!(
        inventory.autoloads_payload_bytes > 0,
        "file pdumps must carry autoload manager state in a raw mmap section"
    );
    assert!(
        !inventory.runtime_state_present,
        "file pdumps must not carry a monolithic bincode RuntimeState section"
    );

    let loaded = load_from_dump(&dump_path).expect("load should succeed");
    assert_eq!(
        loaded
            .obarray()
            .symbol_value_copied("pdump-symbol-section-probe"),
        Some(Value::fixnum(71))
    );
}
