use crate::buffer::BufferTextBackendKind;

use super::super::types::DumpHeapRef;
use super::*;

#[test]
fn buffer_text_backend_kind_tags_match_runtime_backend_kind() {
    for kind in BufferTextBackendKind::implemented_variants() {
        let dump_kind = DumpBufferTextBackendKind::from(kind);
        assert_eq!(BufferTextBackendKind::from(dump_kind), kind);
        assert_eq!(u8::from(dump_kind), u8::from(kind));
    }
}

#[test]
fn buffer_section_round_trips_manager_state() {
    let buffer = DumpBuffer {
        id: DumpBufferId(1),
        name_lisp: Some(DumpLispString {
            data: b"*scratch*".to_vec(),
            size: 9,
            size_byte: 9,
        }),
        name: Some("*scratch*".into()),
        last_name_lisp: None,
        last_name: Some("*old-scratch*".into()),
        base_buffer: Some(DumpBufferId(2)),
        text: DumpBufferText {
            backend_kind: DumpBufferTextBackendKind::GapBuffer,
            text: b"hello buffer".to_vec(),
        },
        pt: 1,
        pt_char: Some(1),
        mark: Some(2),
        mark_char: Some(2),
        begv: 0,
        begv_char: Some(0),
        zv: 12,
        zv_char: Some(12),
        modified: true,
        modified_tick: 3,
        chars_modified_tick: 4,
        save_modified_tick: Some(5),
        autosave_modified_tick: Some(6),
        modtime_sec: Some(1234567890),
        modtime_nsec: Some(500000000),
        modtime_size: Some(1024),
        last_window_start: Some(7),
        read_only: false,
        multibyte: true,
        file_name_lisp: None,
        file_name: Some("/tmp/file".into()),
        auto_save_file_name_lisp: None,
        auto_save_file_name: Some("#file#".into()),
        markers: vec![DumpMarker {
            buffer: Some(DumpBufferId(1)),
            insertion_type: true,
            marker_id: Some(8),
            bytepos: 9,
            charpos: 10,
            last_position_valid: true,
        }],
        state_pt_marker: Some(11),
        state_begv_marker: Some(12),
        state_zv_marker: Some(13),
        properties_syms: vec![(
            DumpSymId(14),
            DumpRuntimeBindingValue::Bound(DumpValue::Int(15)),
        )],
        properties: vec![("prop".into(), DumpRuntimeBindingValue::Void)],
        local_binding_syms: vec![DumpSymId(16)],
        local_binding_names: vec!["legacy-local".into()],
        local_map: DumpValue::Symbol(DumpSymId(17)),
        text_props: DumpTextPropertyTable {
            intervals: vec![DumpPropertyInterval {
                start: 0,
                end: 5,
                properties: vec![(DumpValue::Symbol(DumpSymId(18)), DumpValue::True)],
            }],
        },
        overlays: DumpOverlayList {
            overlays: vec![DumpOverlay {
                serial: 27,
                plist: DumpValue::Nil,
                buffer: Some(DumpBufferId(1)),
                start: 0,
                end: 1,
                front_advance: true,
                rear_advance: false,
            }],
        },
        undo_list: Some(DumpUndoList {
            records: vec![
                DumpUndoRecord::Insert { pos: 1, len: 2 },
                DumpUndoRecord::Delete {
                    pos: 3,
                    text: "gone".into(),
                },
                DumpUndoRecord::PropertyChange {
                    pos: 4,
                    len: 5,
                    old_props: vec![("face".into(), DumpValue::Symbol(DumpSymId(19)))],
                },
                DumpUndoRecord::CursorMove { pos: 6 },
                DumpUndoRecord::FirstChange {
                    visited_file_modtime: 7,
                },
                DumpUndoRecord::Boundary,
            ],
            limit: 20,
            enabled: true,
        }),
        slots: vec![DumpValue::Vector(DumpHeapRef { index: 21 })],
        local_flags: 22,
        local_var_alist: DumpValue::Cons(DumpHeapRef { index: 23 }),
    };
    let manager = DumpBufferManager {
        buffers: vec![(DumpBufferId(1), buffer)],
        buffer_order: vec![DumpBufferId(1)],
        current: Some(DumpBufferId(1)),
        next_id: 24,
        next_marker_id: 25,
        buffer_defaults: vec![DumpValue::Int(26)],
        default_text_backend_kind: DumpBufferTextBackendKind::PieceTree,
    };

    let bytes = buffer_manager_section_bytes(&manager).expect("encode buffers");
    let decoded = load_buffer_manager_section(&bytes).expect("decode buffers");

    assert_eq!(format!("{decoded:?}"), format!("{manager:?}"));
}

#[test]
fn buffer_section_rejects_bad_magic() {
    let mut bytes = buffer_manager_section_bytes(&empty_buffer_manager()).expect("encode buffers");
    bytes[0] ^= 1;
    let err = load_buffer_manager_section(&bytes).expect_err("bad magic should fail");
    assert!(matches!(err, DumpError::ImageFormatError(_)));
}
