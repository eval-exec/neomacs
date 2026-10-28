//! Dump ordering is independent of runtime map layout, including heap visits.
//!
//! One Context owns each test's live Values. Rebuilding manager maps reuses the
//! same keys and object graphs; it changes only insertion/capacity/RandomState.
//! Compare the real converter's section bytes AND heap descriptors, so sorting
//! an already encoded manager cannot hide nondeterministic first-visit IDs.

use super::{
    Context, autoloads_image, coding_system_image, convert, face_image, object_value_codec,
    runtime_managers_image, types,
};
use crate::emacs_core::autoload::{AutoloadEntry, AutoloadManager, AutoloadType};
use crate::emacs_core::coding::{CodingSystemInfo, EolType};
use crate::emacs_core::interactive::{InteractiveRegistry, InteractiveSpec};
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::Value;
use crate::face::{Face, FaceTable};
use crate::heap_types::LispString;
use std::collections::HashMap;
use std::hash::Hash;
use strum::IntoEnumIterator;

#[derive(Clone, Copy, strum::EnumIter)]
enum MapLayout {
    Forward,
    Reverse,
    RotatedWithSpareCapacity,
}

fn rebuild<K: Clone + Eq + Hash, V: Clone>(entries: &[(K, V)], layout: MapLayout) -> HashMap<K, V> {
    let mut entries = entries.to_vec();
    let capacity = match layout {
        MapLayout::Forward => entries.len(),
        MapLayout::Reverse => {
            entries.reverse();
            entries.len()
        }
        MapLayout::RotatedWithSpareCapacity => {
            let shift = entries.len() / 3;
            entries.rotate_left(shift);
            entries.len() * 4
        }
    };
    let mut map = HashMap::with_capacity(capacity);
    map.extend(entries);
    map
}

fn heap_bytes(state: &types::DumpContextState) -> Vec<u8> {
    let mut bytes = Vec::new();
    for object in &state.tagged_heap.objects {
        object_value_codec::write_heap_object(&mut bytes, object)
            .expect("encode real dumped object");
    }
    bytes
}

fn assert_same_snapshot(
    baseline: &mut Option<(Vec<u8>, Vec<u8>)>,
    state: &types::DumpContextState,
    section: Vec<u8>,
) {
    let heap = heap_bytes(state);
    if let Some((expected_section, expected_heap)) = baseline {
        assert_eq!(
            &heap, expected_heap,
            "map layout must not alter first-visit heap IDs or object contents"
        );
        assert_eq!(
            &section, expected_section,
            "manager serialization must not depend on map iteration order"
        );
    } else {
        *baseline = Some((section, heap));
    }
}

fn coding_info(name: crate::emacs_core::SymId, label: &str) -> CodingSystemInfo {
    CodingSystemInfo {
        name,
        coding_type: intern("raw-text"),
        mnemonic: 'D',
        eol_type: EolType::Unix,
        ascii_compatible_p: true,
        charset_list: vec![intern("latin-iso8859-1"), intern("ascii")],
        post_read_conversion: None,
        pre_write_conversion: None,
        default_char: None,
        for_unibyte: false,
        properties: HashMap::from([(
            intern("pdump-order-doc"),
            Value::list(vec![Value::string(label), Value::fixnum(7)]),
        )]),
        int_properties: HashMap::new(),
    }
}

#[test]
fn deterministic_order_coding_registry_preserves_priority_and_alias_sequences() {
    let mut eval = Context::new();
    let systems: Vec<_> = (0..16)
        .map(|index| {
            let name = intern(&format!("pdump-order-coding-{index}"));
            (
                name,
                coding_info(name, &format!("coding documentation {index}")),
            )
        })
        .collect();
    let aliases: Vec<_> = systems
        .iter()
        .enumerate()
        .map(|(index, (name, _))| (intern(&format!("pdump-order-alias-{index}")), *name))
        .collect();
    let alias_order: Vec<_> = systems
        .iter()
        .enumerate()
        .map(|(index, (name, _))| (*name, vec![aliases[index].0, *name]))
        .collect();
    let priority = vec![systems[9].0, systems[2].0, systems[8].0];
    let mut baseline = None;
    for layout in MapLayout::iter() {
        eval.coding_systems.systems = rebuild(&systems, layout);
        eval.coding_systems.aliases = rebuild(&aliases, layout);
        eval.coding_systems.alias_order = rebuild(&alias_order, layout);
        eval.coding_systems.priority = priority.clone();
        let state = convert::dump_evaluator(&eval);
        assert_eq!(
            state.coding_systems.priority_syms,
            priority
                .iter()
                .map(|id| types::DumpSymId(id.0))
                .collect::<Vec<_>>()
        );
        for (name, sequence) in &alias_order {
            let dumped = state
                .coding_systems
                .alias_order_syms
                .iter()
                .find(|(id, _)| id.0 == name.0)
                .expect("alias group retained");
            assert_eq!(
                dumped.1,
                sequence
                    .iter()
                    .map(|id| types::DumpSymId(id.0))
                    .collect::<Vec<_>>()
            );
        }
        assert_same_snapshot(
            &mut baseline,
            &state,
            coding_system_image::coding_system_section_bytes(&state.coding_systems)
                .expect("encode coding manager"),
        );
    }
}

#[test]
fn deterministic_order_coding_properties_are_ordered_before_heap_encoding() {
    let mut eval = Context::new();
    let name = intern("pdump-order-single-coding");
    let mut info = coding_info(name, "single coding");
    let properties: Vec<_> = (0..16)
        .map(|index| {
            (
                intern(&format!("pdump-order-property-{index}")),
                Value::vector(vec![Value::string(format!("property {index}"))]),
            )
        })
        .collect();
    let integer_properties: Vec<_> = (0..16)
        .map(|index| {
            (
                index,
                Value::list(vec![Value::string(format!("integer property {index}"))]),
            )
        })
        .collect();
    eval.coding_systems.systems.clear();
    eval.coding_systems.aliases.clear();
    eval.coding_systems.alias_order.clear();
    eval.coding_systems.priority = vec![name];
    let mut baseline = None;
    for layout in MapLayout::iter() {
        info.properties = rebuild(&properties, layout);
        info.int_properties = rebuild(&integer_properties, layout);
        eval.coding_systems.systems.insert(name, info.clone());
        let state = convert::dump_evaluator(&eval);
        assert_same_snapshot(
            &mut baseline,
            &state,
            coding_system_image::coding_system_section_bytes(&state.coding_systems)
                .expect("encode value-bearing coding properties"),
        );
    }
}

#[test]
fn deterministic_order_autoloads_preserve_callback_and_loaded_file_sequences() {
    let mut eval = Context::new();
    let entries: Vec<_> = (0..16)
        .map(|index| {
            (
                intern(&format!("pdump-order-autoload-{index}")),
                AutoloadEntry {
                    file: LispString::from_utf8(&format!("library-{index}")),
                    docstring: Some(LispString::from_utf8(&format!("autoload {index}"))),
                    interactive: index % 2 == 0,
                    autoload_type: AutoloadType::Function,
                },
            )
        })
        .collect();
    let callbacks: Vec<_> = (0..16)
        .map(|index| {
            (
                format!("after-library-{index}"),
                Value::vector(vec![Value::string(format!("callback {index}"))]),
            )
        })
        .collect();
    let mut baseline = None;
    for layout in MapLayout::iter() {
        let mut manager = AutoloadManager::new();
        for (name, entry) in rebuild(&entries, layout) {
            manager.register_symbol(name, entry);
        }
        for (file, callback) in rebuild(&callbacks, layout) {
            manager.add_after_load(&file, Value::fixnum(20));
            manager.add_after_load(&file, Value::fixnum(10));
            manager.add_after_load(&file, callback);
        }
        manager.mark_loaded("z-loaded-before-a");
        manager.mark_loaded("a-loaded-after-z");
        *eval.autoloads = manager;
        let state = convert::dump_evaluator(&eval);
        assert_eq!(
            state
                .autoloads
                .loaded_files
                .iter()
                .map(|file| file.data.as_slice())
                .collect::<Vec<_>>(),
            vec![
                b"z-loaded-before-a".as_slice(),
                b"a-loaded-after-z".as_slice()
            ]
        );
        for (_, callbacks) in &state.autoloads.after_load_lisp {
            assert!(matches!(
                callbacks.as_slice(),
                [
                    types::DumpValue::Int(20),
                    types::DumpValue::Int(10),
                    types::DumpValue::Vector(_)
                ]
            ));
        }
        assert_same_snapshot(
            &mut baseline,
            &state,
            autoloads_image::autoloads_section_bytes(&state.autoloads).expect("encode autoloads"),
        );
    }
}

#[test]
fn deterministic_order_faces_are_ordered_before_value_encoding() {
    let mut eval = Context::new();
    let faces: Vec<_> = (0..16)
        .map(|index| {
            let name = format!("pdump-order-face-{index}");
            intern(&name);
            (
                name,
                Face {
                    family: Some(Value::string(format!("family {index}"))),
                    doc: Some(Value::string(format!("face documentation {index}"))),
                    ..Face::default()
                },
            )
        })
        .collect();
    let mut baseline = None;
    for layout in MapLayout::iter() {
        *eval.face_table_mut() = FaceTable::from_dump(rebuild(&faces, layout));
        let state = convert::dump_evaluator(&eval);
        assert_same_snapshot(
            &mut baseline,
            &state,
            face_image::face_table_section_bytes(&state.face_table).expect("encode faces"),
        );
    }
}

#[test]
fn deterministic_order_runtime_interactive_specs_are_ordered_before_value_encoding() {
    let mut eval = Context::new();
    let specs: Vec<_> = (0..16)
        .map(|index| {
            (
                intern(&format!("pdump-order-command-{index}")),
                InteractiveSpec::from_value(Value::list(vec![Value::string(format!(
                    "interactive {index}"
                ))])),
            )
        })
        .collect();
    let mut baseline = None;
    for layout in MapLayout::iter() {
        *eval.interactive = InteractiveRegistry::from_dump(rebuild(&specs, layout));
        let state = convert::dump_evaluator(&eval);
        let managers = runtime_managers_image::RuntimeManagersState::from_context_state(&state);
        assert_same_snapshot(
            &mut baseline,
            &state,
            runtime_managers_image::runtime_managers_section_bytes(&managers)
                .expect("encode runtime managers"),
        );
    }
}

fn assert_gc_forwarding(eval: &Context) {
    use crate::emacs_core::forward::LispFwdType;
    assert_eq!(
        eval.obarray.forward_type(intern("gc-elapsed")),
        Some(LispFwdType::Obj)
    );
    assert_eq!(
        eval.obarray.forward_type(intern("gcs-done")),
        Some(LispFwdType::Int)
    );
}

fn assert_live_snapshot_gc_values(state: &types::DumpContextState, elapsed: f64, count: i64) {
    let elapsed_symbol = state
        .obarray
        .symbols
        .iter()
        .find(|(id, _)| id.0 == intern("gc-elapsed").0)
        .expect("gc-elapsed remains a residual forwarded symbol");
    let types::DumpSymbolVal::ObjForwarded(types::DumpValue::Float(reference)) =
        &elapsed_symbol.1.val
    else {
        panic!("gc-elapsed must retain its object-forwarded float arm");
    };
    assert!(matches!(
        state.tagged_heap.objects.get(reference.index as usize),
        Some(types::DumpHeapObject::Float(value)) if *value == elapsed
    ));
    let count_symbol = state
        .obarray
        .symbols
        .iter()
        .find(|(id, _)| id.0 == intern("gcs-done").0)
        .expect("gcs-done remains a residual forwarded symbol");
    assert!(matches!(count_symbol.1.val,
        types::DumpSymbolVal::IntForwarded(types::DumpValue::Int(value)) if value == count));
}

#[test]
fn deterministic_order_restart_gc_stats_exclude_history_without_mutating_live_state() {
    let mut eval = Context::new();
    // Ordinary names resembling the builtins are part of Lisp state, not
    // restart statistics. Allocate both historical floats before either dump.
    let historical_elapsed = [
        Value::make_float(0.017328728),
        Value::make_float(0.017198402),
    ];
    eval.set_variable("pdump-order-gc-elapsed", Value::make_float(7.25));
    eval.set_variable("pdump-order-gcs-done", Value::fixnum(93));
    // An ordinary reference to the SAME old float is semantic user data.
    // Keep it fixed across both runs; only the canonical GC root restarts.
    eval.set_variable("pdump-order-gc-history-reference", historical_elapsed[0]);
    let dir = tempfile::tempdir().expect("private dump directory");
    let first = dir.path().join("first.pdump");
    let second = dir.path().join("second.pdump");

    for (path, elapsed, count) in [
        (&first, historical_elapsed[0], 14),
        (&second, historical_elapsed[1], 29),
    ] {
        eval.set_variable("gc-elapsed", elapsed);
        eval.set_variable("gcs-done", Value::fixnum(count));
        assert_gc_forwarding(&eval);
        let live = convert::dump_evaluator(&eval);
        assert_live_snapshot_gc_values(&live, elapsed.as_float().expect("float fixture"), count);
        super::dump_to_file(&eval, path).expect("write restart image");
        // File-only normalization must never write through the live native
        // descriptor or silently redirect the original symbol to a plain slot.
        assert_gc_forwarding(&eval);
        assert_eq!(
            eval.obarray.symbol_value_copied("gc-elapsed"),
            Some(elapsed)
        );
        assert_eq!(
            eval.obarray.symbol_value_copied("gcs-done"),
            Some(Value::fixnum(count))
        );
        assert_live_snapshot_gc_values(
            &convert::dump_evaluator(&eval),
            elapsed.as_float().unwrap(),
            count,
        );
        assert_eq!(
            eval.obarray
                .symbol_value_copied("pdump-order-gc-history-reference")
                .and_then(|value| value.as_float()),
            Some(0.017328728),
            "restart normalization must not zero an ordinary reference in the live heap"
        );
    }
    assert_eq!(
        std::fs::read(&first).expect("first image"),
        std::fs::read(&second).expect("second image"),
        "complete restart image bytes, including heap/obarray, must not retain producer GC history"
    );

    let loaded = super::load_from_dump(&second).expect("load normalized restart image");
    assert_eq!(
        loaded
            .obarray
            .symbol_value_copied("pdump-order-gc-history-reference")
            .and_then(|value| value.as_float()),
        Some(0.017328728),
        "the image must preserve user references to the old elapsed object"
    );
    assert_gc_forwarding(&loaded);
    assert_eq!(
        loaded
            .obarray
            .symbol_value_copied("gc-elapsed")
            .and_then(|value| value.as_float()),
        Some(0.0)
    );
    assert_eq!(
        loaded
            .obarray
            .symbol_value_copied("gcs-done")
            .and_then(|value| value.as_fixnum()),
        Some(0)
    );
    assert_eq!(
        loaded
            .obarray
            .symbol_value_copied("pdump-order-gc-elapsed")
            .and_then(|value| value.as_float()),
        Some(7.25)
    );
    assert_eq!(
        loaded
            .obarray
            .symbol_value_copied("pdump-order-gcs-done")
            .and_then(|value| value.as_fixnum()),
        Some(93)
    );
}

#[test]
fn deterministic_order_overlay_serials_survive_dump_and_resume_the_counter() {
    use crate::heap_types::{OverlayDataInit, OverlaySerial, OverlaySerialSource};

    const DETACHED_SERIAL: u64 = 1_000_000_101;
    const ATTACHED_SERIAL: u64 = 1_000_000_202;
    let preserved = |raw: u64| {
        OverlaySerialSource::Preserved(
            OverlaySerial::from_u64(raw).expect("test serials are non-zero"),
        )
    };

    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("pdump-overlay-serial");
    let detached = Value::make_overlay(OverlayDataInit {
        serial: preserved(DETACHED_SERIAL),
        plist: Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
        buffer: None,
        start: 2,
        end: 9,
        front_advance: true,
        rear_advance: false,
    });
    let attached = Value::make_overlay(OverlayDataInit {
        serial: preserved(ATTACHED_SERIAL),
        plist: Value::NIL,
        buffer: Some(buffer),
        start: 1,
        end: 1,
        front_advance: false,
        rear_advance: true,
    });
    eval.buffers
        .get_mut(buffer)
        .unwrap()
        .overlays
        .insert_overlay(attached);
    eval.set_variable("pdump-overlay-detached", detached);
    eval.set_variable("pdump-overlay-attached", attached);

    // Redisplay snapshots are observers: their cloned objects keep the
    // creation serial of the overlay they observe.
    let observer = eval.buffers.get(buffer).unwrap().overlays.snapshot_clone();
    let observer_values = observer.dump_overlays();
    assert_eq!(
        observer_values[0].as_overlay_data().unwrap().serial.get(),
        ATTACHED_SERIAL
    );

    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.pdump");
    let second = dir.path().join("second.pdump");
    super::dump_to_file(&eval, &first).unwrap();
    super::dump_to_file(&eval, &second).unwrap();
    assert_eq!(
        std::fs::read(&first).unwrap(),
        std::fs::read(&second).unwrap(),
        "serials are part of the image and must dump deterministically"
    );

    let mut loaded = super::load_from_dump(&second).unwrap();
    let a = loaded
        .obarray
        .symbol_value_copied("pdump-overlay-detached")
        .unwrap();
    let b = loaded
        .obarray
        .symbol_value_copied("pdump-overlay-attached")
        .unwrap();
    assert!(!crate::emacs_core::value::eq_value(&a, &b));
    assert_eq!(a.as_overlay_data().unwrap().serial.get(), DETACHED_SERIAL);
    assert_eq!(b.as_overlay_data().unwrap().serial.get(), ATTACHED_SERIAL);
    let restored_buffer = loaded.buffers.get(buffer).unwrap();
    for value in restored_buffer.overlays.dump_overlays() {
        assert_eq!(
            value.as_overlay_data().unwrap().serial.get(),
            ATTACHED_SERIAL
        );
    }

    // The restored World resumes minting above every preserved serial, so a
    // post-load overlay still sorts after every overlay that came from the
    // image.
    let fresh = loaded
        .eval_str("(make-overlay (point-min) (point-min))")
        .expect("create a fresh overlay after image load");
    assert!(fresh.as_overlay_data().unwrap().serial.get() > ATTACHED_SERIAL);
}
