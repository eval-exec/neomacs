use super::*;
use neovm_core::emacs_core::intern::{intern_lisp_string, intern_uninterned};
use strum::VariantArray;

#[test]
fn fixed_bridge_face_producers_match_original_complete_face_resolution() {
    let mut table = FaceTable::new();
    let mut parent = NeoFace::new("cont9-fixed-face-parent");
    parent.foreground = Some(NeoColor::rgb(12, 34, 56));
    parent.weight = Some(FontWeight::BOLD);
    table.define("cont9-fixed-face-parent", parent);
    for (index, fixed) in FixedFace::VARIANTS.iter().copied().enumerate() {
        let mut own = NeoFace::new(fixed.name());
        own.inherit = Some(Value::symbol("cont9-fixed-face-parent"));
        own.background = Some(NeoColor::rgb(index as u8, 21, 43));
        table.define(fixed.name(), own);
        assert_eq!(fixed.resolve(&table), table.resolve(fixed.name()));
    }
}

#[test]
fn named_face_producers_preserve_frame_realization_and_noncanonical_fallback() {
    let _runtime = Context::new();
    let mut table = FaceTable::new();
    for name in ["cont9-layout-producer", ":cont9-layout-keyword"] {
        let mut face = NeoFace::new(name);
        face.foreground = Some(NeoColor::rgb(11, 22, 33));
        face.background = Some(NeoColor::rgb(44, 55, 66));
        face.height = Some(FaceHeight::Absolute(180));
        face.overline = Some(true);
        table.define(name, face);
    }
    let resolver = FaceResolver::new(&table, 0xffffff, 0, 14.0, None);
    for name in [
        "cont9-layout-producer",
        ":cont9-layout-keyword",
        "cont9-layout-missing",
        "default",
    ] {
        for producer in [
            Value::symbol(name),
            Value::from_sym_id(intern_uninterned(name)),
        ] {
            assert_eq!(
                resolver.resolve_named_face_producer(producer),
                Some(resolver.resolve_named_face(name)),
            );
        }
    }
}

#[test]
fn named_face_producer_keeps_realized_basic_face_short_circuit() {
    let _runtime = Context::new();
    let table = FaceTable::new();
    let mut resolver = FaceResolver::new(&table, 0xffffff, 0, 14.0, None);
    let mut basic_faces = enum_map::EnumMap::default();
    let cached = ResolvedFace {
        fg: 0x112233,
        font_size: 37.0,
        face_id: 417,
        ..ResolvedFace::default()
    };
    basic_faces[BasicFace::ModeLineActive] = Some(cached.clone());
    resolver.realized_basic_faces = Some(std::sync::Arc::new(basic_faces));
    let value = Value::symbol("mode-line-active");
    assert_eq!(resolver.resolve_named_face_producer(value), Some(cached));
    assert_eq!(
        resolver.resolve_named_face_producer(value),
        Some(resolver.resolve_named_face("mode-line-active"))
    );
}

#[test]
fn window_named_face_producers_preserve_nil_and_nonempty_remapping_paths() {
    let runtime = Context::new();
    let mut table = FaceTable::new();
    let mut face = NeoFace::new("cont9-window-producer");
    face.foreground = Some(NeoColor::rgb(17, 34, 51));
    table.define("cont9-window-producer", face);
    let resolver = FaceResolver::new(&table, 0xffffff, 0, 14.0, None);
    let alist = Value::list(vec![Value::cons(
        Value::symbol("cont9-window-producer"),
        Value::list(vec![Value::keyword("weight"), Value::symbol("bold")]),
    )]);
    let _roots = runtime.retain_gc_roots([alist]);
    for remapping in [
        BufferFaceRemapping::empty(),
        BufferFaceRemapping::for_alist(Some(Value::NIL)),
        BufferFaceRemapping::for_alist(Some(alist)),
    ] {
        for name in ["cont9-window-producer", "default", "cont9-window-missing"] {
            for value in [
                Value::symbol(name),
                Value::from_sym_id(intern_uninterned(name)),
            ] {
                assert_eq!(
                    resolver.resolve_remapped_named_face_producer(remapping, value),
                    Some(resolver.resolve_remapped_named_face(remapping, name)),
                );
            }
        }
    }
}

#[test]
fn layout_named_face_producer_keeps_bare_utf8_admission_boundary() {
    let runtime = Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0xffffff, 0, 14.0, None);
    let text = Value::string("default");
    let _roots = runtime.retain_gc_roots([text]);
    let raw = Value::from_sym_id(intern_lisp_string(
        &neovm_core::heap_types::LispString::from_unibyte(vec![b'c', b'9', 0xff]),
    ));
    for value in [Value::fixnum(3), text, raw] {
        assert!(resolver.resolve_named_face_producer(value).is_none());
        assert!(
            resolver
                .resolve_remapped_named_face_producer(BufferFaceRemapping::empty(), value)
                .is_none()
        );
    }
}
