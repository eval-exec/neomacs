use super::*;
use neovm_core::emacs_core::intern::{
    intern_lisp_string, intern_uninterned, unintern_canonical_id,
};
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
fn named_face_producers_keep_the_canonical_symbol_and_miss_an_uninterned_object() {
    let _runtime = neovm_core::emacs_core::eval_hook_installers::new_context();
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
        "mode-line",
        "mode-line-active",
    ] {
        let named = Some(resolver.resolve_named_face(name));
        assert_eq!(
            resolver.resolve_named_face_producer(Value::symbol(name)),
            named.clone(),
            "{name}",
        );
        // GNU `Fgethash` / `lface_from_face_name_no_resolve` (xfaces.c:2060-2075)
        // is EQ on the symbol object. `default` pixels match an empty face of
        // that spelling, so equality there is not evidence of a hit or a miss.
        if name == "default" {
            continue;
        }
        let uninterned =
            resolver.resolve_named_face_producer(Value::from_sym_id(intern_uninterned(name)));
        if name == "cont9-layout-missing" {
            assert_eq!(uninterned, named, "{name}");
        } else {
            assert_ne!(uninterned, named, "{name}");
        }
    }
}

#[test]
fn named_face_producer_keeps_realized_basic_face_short_circuit() {
    let _runtime = neovm_core::emacs_core::eval_hook_installers::new_context();
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
    assert_eq!(
        resolver.resolve_named_face_producer(value),
        Some(cached.clone())
    );
    assert_eq!(
        resolver.resolve_named_face_producer(value),
        Some(resolver.resolve_named_face("mode-line-active"))
    );
    // The cache key is the canonical symbol object. An uninterned
    // `mode-line-active` is not that object, and its pixels differ from the
    // cached basic face (0x112233 / face id 417).
    let uninterned = Value::from_sym_id(intern_uninterned("mode-line-active"));
    assert_ne!(
        resolver.resolve_named_face_producer(uninterned),
        Some(cached)
    );
}

/// `tab-line-active`, `tab-line-inactive`, and `margin` are static DEFSYM
/// objects (`Qtab_line_active` / `Qtab_line_inactive`, src/xfaces.c:7541-7542
/// and lookup_basic_face 5198-5199; `Qmargin`, src/xdisp.c:38309, used at
/// xfaces.c:5214 and realized at 5982). `make-symbol` of the spelling misses.
/// After `unintern` and a fresh `intern`, the original object still hits the
/// realized role and the new object misses.
#[test]
fn tab_line_and_margin_roles_stay_on_the_boot_symbol_after_reintern() {
    let _runtime = neovm_core::emacs_core::eval_hook_installers::new_context();
    let table = FaceTable::new();
    let mut resolver = FaceResolver::new(&table, 0xffffff, 0, 14.0, None);
    let mut basic_faces = enum_map::EnumMap::default();
    let rows = [
        (
            "tab-line-active",
            BasicFace::TabLineActive,
            ResolvedFace {
                fg: 0x110021,
                font_size: 41.0,
                face_id: 501,
                ..ResolvedFace::default()
            },
        ),
        (
            "tab-line-inactive",
            BasicFace::TabLineInactive,
            ResolvedFace {
                fg: 0x110022,
                font_size: 42.0,
                face_id: 502,
                ..ResolvedFace::default()
            },
        ),
        (
            "margin",
            BasicFace::Margin,
            ResolvedFace {
                fg: 0x110023,
                font_size: 43.0,
                face_id: 503,
                ..ResolvedFace::default()
            },
        ),
    ];
    for (_, role, face) in &rows {
        basic_faces[*role] = Some(face.clone());
    }
    resolver.realized_basic_faces = Some(std::sync::Arc::new(basic_faces));
    for (name, _, face) in rows {
        let original = Value::symbol(name);
        let made = Value::from_sym_id(intern_uninterned(name));
        assert_ne!(
            resolver.resolve_named_face_producer(made),
            Some(face.clone()),
            "uninterned {name}",
        );
        let original_id = original.as_bare_symbol_id().expect("bare symbol");
        assert!(unintern_canonical_id(original_id), "unintern {name}");
        let reinterned = Value::symbol(name);
        assert_ne!(reinterned.bits(), original.bits(), "{name}");
        assert_eq!(
            resolver.resolve_named_face_producer(original),
            Some(face.clone()),
            "original {name}",
        );
        assert_ne!(
            resolver.resolve_named_face_producer(reinterned),
            Some(face),
            "reinterned {name}",
        );
    }
}

#[test]
fn window_named_face_producers_keep_canonical_symbols_on_every_remapping() {
    let runtime = neovm_core::emacs_core::eval_hook_installers::new_context();
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
        let label = match remapping.alist() {
            None => "empty",
            Some(alist) if alist.is_nil() => "nil",
            Some(_) => "nonempty",
        };
        // `mode-line` is the distinct-pixel basic row: gray on light gray,
        // unlike `default`, whose empty-face pixels coincide.
        for name in [
            "cont9-window-producer",
            "default",
            "cont9-window-missing",
            "mode-line",
            "mode-line-active",
        ] {
            let expected = Some(resolver.resolve_remapped_named_face(remapping, name));
            assert_eq!(
                resolver.resolve_remapped_named_face_producer(remapping, Value::symbol(name)),
                expected.clone(),
                "{label} {name}",
            );
            // `assq_no_quit` (xfaces.c:2141) is EQ. An uninterned object misses
            // under an empty list, a nil alist, and a nonempty alist alike.
            if name == "default" {
                continue;
            }
            let uninterned = resolver.resolve_remapped_named_face_producer(
                remapping,
                Value::from_sym_id(intern_uninterned(name)),
            );
            if name == "cont9-window-missing" {
                assert_eq!(uninterned, expected, "{label} {name}");
            } else {
                assert_ne!(uninterned, expected, "{label} {name}");
            }
        }
    }
}

#[test]
fn layout_named_face_producer_keeps_bare_utf8_admission_boundary() {
    let runtime = neovm_core::emacs_core::eval_hook_installers::new_context();
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
