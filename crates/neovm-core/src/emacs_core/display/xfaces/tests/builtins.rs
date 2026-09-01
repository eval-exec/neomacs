//! Tests for the xfaces.c builtin surface (moved from font_test.rs).

use super::*;
use crate::buffer::{Buffer, CharPos0, CharRange};
use crate::emacs_core::display_host::FontResolveRequest;
use crate::emacs_core::eval::{
    Context, DisplayHost, FontPxProbeResult, GuiFrameHostRequest, ResolvedFontMatch,
    ResolvedFrameFont,
};
use crate::emacs_core::font::*;
use crate::emacs_core::value::{ValueKind, VecLikeType};
use crate::face::{Color, FaceAttrValue, FaceHeight};
use crate::window::{FRAME_ID_BASE, FrameId, FrameParam};

macro_rules! call_font_builtin {
    ($builtin:ident, $args:expr) => {{
        let mut eval = Context::new();
        let args = $args;
        $builtin(&mut eval, args)
    }};
}

fn ensure_selected_gui_frame(eval: &mut Context) -> FrameId {
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(eval);
    let frame = eval
        .frame_manager_mut()
        .get_mut(frame_id)
        .expect("selected frame");
    frame.set_window_system(Some(Value::symbol("neo")));
    frame_id
}

fn resolved_frame_font(
    family: &str,
    postscript_name: &str,
    height_tenths: i32,
    metrics: FontPxProbeResult,
) -> ResolvedFrameFont {
    ResolvedFrameFont {
        font: crate::emacs_core::eval::test_resolved_opened_font(
            family,
            None,
            None,
            FontWeight::NORMAL,
            FontSlant::Normal,
            FontWidth::Normal,
            Some(postscript_name),
            metrics,
            None,
        ),
        height_tenths,
    }
}

fn buffer_char_pos_to_byte(buffer: &Buffer, char_pos: usize) -> usize {
    buffer
        .char_pos_to_emacs_byte_pos_clamped(CharPos0::new(char_pos))
        .get()
}

struct LiveFrameFontDisplayHost {
    realized: Option<ResolvedFrameFont>,
}

impl DisplayHost for LiveFrameFontDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resolve_frame_font(
        &mut self,
        _frame_id: crate::window::FrameId,
        _request: crate::emacs_core::display_host::FrameFontRequest,
    ) -> Result<Option<ResolvedFrameFont>, String> {
        Ok(self.realized.clone())
    }

    fn resolve_font_for_char(
        &mut self,
        request: FontResolveRequest,
    ) -> Result<Option<ResolvedFontMatch>, String> {
        Ok(self.realized.clone().map(|resolved| ResolvedFontMatch {
            font: resolved.font,
            glyph_code: Some(request.character.code()),
        }))
    }
}

fn install_font_at_display_host(eval: &mut Context, family: &str) {
    eval.set_display_host(Box::new(LiveFrameFontDisplayHost {
        realized: Some(resolved_frame_font(
            family,
            "TestFont-Regular",
            100,
            FontPxProbeResult {
                pixel_size: 14,
                height: 17,
                ascent: 13,
                descent: 4,
                max_width: 9,
                space_width: 8,
                average_width: 8,
            },
        )),
    }));
}

#[test]
fn clear_font_cache_resets_face_caches() {
    crate::test_utils::init_test_tracing();
    let face_name = "__neovm_clear_font_cache_unit_test";
    let _ = call_font_builtin!(
        builtin_internal_make_lisp_face,
        vec![Value::symbol(face_name)]
    )
    .unwrap();
    let _ = call_font_builtin!(
        builtin_internal_set_lisp_face_attribute,
        vec![
            Value::symbol(face_name),
            Value::keyword(":foreground"),
            Value::string("white"),
        ]
    )
    .unwrap();

    CREATED_LISP_FACES.with(|slot| {
        assert!(
            slot.borrow()
                .contains(&crate::emacs_core::intern::intern(face_name,))
        );
    });
    FACE_ATTR_STATE.with(|slot| {
        assert!(!slot.borrow().selected_overrides.is_empty());
    });

    let result = clear_font_cache(vec![]).unwrap();
    assert!(result.is_nil());

    CREATED_LISP_FACES.with(|slot| assert!(slot.borrow().is_empty()));
    CREATED_FACE_IDS.with(|slot| assert!(slot.borrow().is_empty()));
    NEXT_CREATED_FACE_ID.with(|slot| {
        assert_eq!(*slot.borrow(), FIRST_DYNAMIC_FACE_ID);
    });
    FACE_ATTR_STATE.with(|slot| {
        let state = slot.borrow();
        assert!(state.selected_overrides.is_empty());
        assert!(state.defaults_overrides.is_empty());
        assert!(state.selected_created.is_empty());
    });
}

#[test]
fn set_lisp_face_attribute_bumps_face_change_count() {
    crate::test_utils::init_test_tracing();
    clear_font_cache_state();
    let mut eval = Context::new();
    let face_name = "__neovm_incr_layout_probe_face";
    builtin_internal_make_lisp_face(&mut eval, vec![Value::symbol(face_name)]).unwrap();

    // Changing a face attribute must advance the global face-change counter:
    // incremental redisplay's retained key compares `face_change_count` and
    // drops matrices when set-face-attribute / theme load mutates appearance
    // in place (spec §4.2 escalation — the hash cannot backstop face drift).
    let before = eval.face_change_count;
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol(face_name),
            Value::keyword(":foreground"),
            Value::string("green"),
        ],
    )
    .unwrap();
    let after = eval.face_change_count;
    assert!(
        after > before,
        "set-face-attribute must bump face_change_count (before={before} after={after})"
    );
}

#[test]
fn frame_face_attribute_setter_defers_runtime_realization_until_redisplay() {
    crate::test_utils::init_test_tracing();
    clear_font_cache_state();
    let mut eval = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let frame = Value::make_frame(frame_id.0);
    let face_name = "__neovm_deferred_face_realization";

    builtin_internal_make_lisp_face(&mut eval, vec![Value::symbol(face_name), frame]).unwrap();
    let before = eval.face_change_count;

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol(face_name),
            Value::keyword(":foreground"),
            Value::string("#51afef"),
            frame,
        ],
    )
    .unwrap();

    let vector = lookup_frame_lisp_face_vector(&eval, frame_id, face_name)
        .expect("frame-local Lisp face vector");
    assert_eq!(
        lisp_face_vector_attr(vector, LFaceAttr::Foreground),
        Some(Value::string("#51afef")),
        "the GNU-shaped frame-local Lisp face vector is authoritative",
    );
    assert!(
        eval.face_change_count > before,
        "the setter must dirty redisplay"
    );
    assert_eq!(
        eval.face_table().resolve(face_name).foreground,
        None,
        "the setter must not eagerly mutate redisplay's derived face table",
    );
}

#[test]
fn created_face_runtime_state_uses_symbol_identity() {
    crate::test_utils::init_test_tracing();
    clear_font_cache_state();
    let face_name = "__neovm_symbol_runtime_face";
    let face_symbol = crate::emacs_core::intern::intern(face_name);
    let mut eval = Context::new();

    builtin_internal_make_lisp_face(&mut eval, vec![Value::symbol(face_name)]).unwrap();
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol(face_name),
            Value::keyword(":foreground"),
            Value::string("green"),
        ],
    )
    .unwrap();

    CREATED_LISP_FACES.with(|slot| {
        assert!(slot.borrow().contains(&face_symbol));
    });
    CREATED_FACE_IDS.with(|slot| {
        assert!(slot.borrow().contains_key(&face_symbol));
    });
    FACE_ATTR_STATE.with(|slot| {
        let state = slot.borrow();
        assert_eq!(
            state
                .selected_overrides
                .get(&face_symbol)
                .and_then(|attrs| attrs.get(&crate::face::LFaceAttr::Foreground))
                .copied(),
            Some(Value::string("green"))
        );
    });
}

#[test]
fn font_at_eval_returns_font_object_for_multibyte_buffer_face() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    ensure_selected_gui_frame(&mut eval);
    install_font_at_display_host(&mut eval, "Serif");

    let face = Value::symbol("font-at-buffer-face");
    builtin_internal_make_lisp_face(&mut eval, vec![face]).unwrap();
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![face, Value::keyword("family"), Value::string("Serif")],
    )
    .unwrap();

    let buffer = eval
        .buffers
        .current_buffer_mut()
        .expect("current buffer for font-at buffer test");
    buffer.insert("a好b");
    let start = buffer_char_pos_to_byte(buffer, 1);
    let end = buffer_char_pos_to_byte(buffer, 2);
    buffer.text_props_put_property_in_emacs_byte_range(
        crate::buffer::EmacsByteRange::from_usize(start, end),
        Value::symbol("face"),
        face,
    );

    let font = font_at(&mut eval, vec![Value::fixnum(2)]).unwrap();
    assert!(
        fontp(vec![font, Value::symbol("font-object")])
            .unwrap()
            .is_truthy()
    );
    assert_eq!(
        font_get(vec![font, Value::keyword("family")])
            .unwrap()
            .as_symbol_name(),
        Some("Serif")
    );
}

#[test]
fn font_at_eval_returns_font_object_for_multibyte_string_face() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    ensure_selected_gui_frame(&mut eval);
    install_font_at_display_host(&mut eval, "Serif");

    let face = Value::symbol("font-at-string-face");
    builtin_internal_make_lisp_face(&mut eval, vec![face]).unwrap();
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![face, Value::keyword("family"), Value::string("Serif")],
    )
    .unwrap();

    let string = Value::string("a好b");
    if !string.is_string() {
        panic!("expected string value");
    };
    let mut table = crate::buffer::TextPropertyTable::new();
    let start = 1;
    let end = 2;
    table.put_property_in_char_range(
        CharRange::from_usize(start, end),
        Value::symbol("face"),
        face,
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(string, table);

    let font = font_at(&mut eval, vec![Value::fixnum(1), Value::NIL, string]).unwrap();
    assert!(
        fontp(vec![font, Value::symbol("font-object")])
            .unwrap()
            .is_truthy()
    );
    assert_eq!(
        font_get(vec![font, Value::keyword("family")])
            .unwrap()
            .as_symbol_name(),
        Some("Serif")
    );
}

#[test]
fn font_at_eval_preserves_raw_unibyte_string_face() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    ensure_selected_gui_frame(&mut eval);
    install_font_at_display_host(&mut eval, "Serif");

    let face = Value::symbol("font-at-raw-string-face");
    builtin_internal_make_lisp_face(&mut eval, vec![face]).unwrap();
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![face, Value::keyword("family"), Value::string("Serif")],
    )
    .unwrap();

    let string = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let char_len = string.as_lisp_string().expect("font test string").schars();
    let mut table = crate::buffer::TextPropertyTable::new();
    table.put_property_in_char_range(
        CharRange::from_usize(0, char_len),
        Value::symbol("face"),
        face,
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(string, table);

    let font = font_at(&mut eval, vec![Value::fixnum(0), Value::NIL, string]).unwrap();
    assert!(
        fontp(vec![font, Value::symbol("font-object")])
            .unwrap()
            .is_truthy()
    );
    assert_eq!(
        font_get(vec![font, Value::keyword("family")])
            .unwrap()
            .as_symbol_name(),
        Some("Serif")
    );
}

#[test]
fn internal_lisp_face_p_symbol_returns_face_vector() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_internal_lisp_face_p(&mut eval, vec![Value::symbol("default")]).unwrap();
    let values = match result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => result.as_vector_data().unwrap().clone(),
        _ => panic!("expected vector"),
    };
    assert_eq!(values.len(), LISP_FACE_VECTOR_LEN);
    assert_eq!(values[0].as_symbol_name(), Some("face"));
}

#[test]
fn internal_lisp_face_p_resolves_face_alias_chains() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let target = Value::symbol("__neovm_face_alias_target");
    let intermediate = Value::symbol("__neovm_face_alias_intermediate");
    let alias = Value::symbol("__neovm_face_alias");

    builtin_internal_make_lisp_face(&mut eval, vec![target]).unwrap();
    eval.obarray_mut()
        .put_property("__neovm_face_alias_intermediate", "face-alias", target)
        .unwrap();
    eval.obarray_mut()
        .put_property("__neovm_face_alias", "face-alias", intermediate)
        .unwrap();

    let target_vector = builtin_internal_lisp_face_p(&mut eval, vec![target]).unwrap();
    let alias_vector = builtin_internal_lisp_face_p(&mut eval, vec![alias]).unwrap();
    assert!(
        crate::emacs_core::value::eq_value(&alias_vector, &target_vector),
        "a face alias chain must return the canonical target's live face vector",
    );
}

#[test]
fn internal_lisp_face_p_rejects_circular_face_aliases() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let first = Value::symbol("__neovm_circular_face_alias_a");
    let second = Value::symbol("__neovm_circular_face_alias_b");

    eval.obarray_mut()
        .put_property("__neovm_circular_face_alias_a", "face-alias", second)
        .unwrap();
    eval.obarray_mut()
        .put_property("__neovm_circular_face_alias_b", "face-alias", first)
        .unwrap();

    match builtin_internal_lisp_face_p(&mut eval, vec![first]) {
        Err(Flow::Signal(signal)) => {
            assert_eq!(signal.symbol_name(), "circular-list");
            assert_eq!(signal.data, vec![first]);
        }
        other => panic!("expected circular-list for a face alias cycle, got {other:?}"),
    }
}

#[test]
fn internal_lisp_face_p_non_symbol() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_internal_lisp_face_p(&mut eval, vec![Value::fixnum(42)]).unwrap();
    assert!(result.is_nil());
}

/// Regression guard for the `internal-lisp-face-p` hotspot fix.
///
/// The predicate used to re-seed `face--new-frame-defaults` on every call --
/// rebuilding a cons + a fresh lface vector for *every* known face only to
/// discard them -- and routed known faces through the create-on-miss `ensure`
/// path. That made it the top self-time hotspot (~300us/call, ~150x a raw
/// gethash). GNU's `Finternal_lisp_face_p` is a single allocation-free,
/// symbol-keyed hash read. Lock in that contract: a known face returns the live
/// table vector *by identity* (proving no per-call allocation/copy), and neither
/// a hit nor an unknown miss ever mutates the table.
#[test]
fn internal_lisp_face_p_is_a_pure_read_of_the_live_table() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let table = eval
        .obarray()
        .symbol_value("face--new-frame-defaults")
        .copied()
        .expect("face--new-frame-defaults is bound at startup");
    let count_before = table.as_hash_table().expect("hash table").data.len();

    // Known face: the returned vector must be eq-identical to the entry already
    // stored in the table -- i.e. read straight from the canonical store, not a
    // freshly allocated or seeded copy.
    let hit = builtin_internal_lisp_face_p(&mut eval, vec![Value::symbol("default")]).unwrap();
    assert!(hit.is_vector());
    let live = crate::emacs_core::xfaces::lookup_face_new_frame_defaults_vector(
        &eval,
        Value::symbol("default"),
    )
    .expect("default face present in table");
    assert!(
        crate::emacs_core::value::eq_value(&hit, &live),
        "predicate must return the live table vector by identity, not a copy",
    );

    // Unknown face: nil, and no entry gets created for it.
    let miss = builtin_internal_lisp_face_p(
        &mut eval,
        vec![Value::symbol("__neovm_pure_read_unknown_face")],
    )
    .unwrap();
    assert!(miss.is_nil());

    let count_after = table.as_hash_table().expect("hash table").data.len();
    assert_eq!(
        count_before, count_after,
        "internal-lisp-face-p must not mutate face--new-frame-defaults",
    );

    // Red-capable guard for the allocation storm itself: the old form re-seeded
    // the table on every call, allocating a cons + a fresh lface vector for each
    // of ~190 faces (~40 KB of tagged-heap garbage per call). The GNU-shaped read
    // path allocates nothing. `total_allocated_bytes` is monotonic across GC, so
    // the delta over a batch is a stable signal. Old: ~500 * 40 KB ~= 20 MB; new:
    // ~0. The 100 KB bound sits ~200x below the old cost and far above any
    // incidental allocation, so it separates the two without flaking.
    let bytes_before = crate::tagged::gc::with_tagged_heap(|heap| heap.total_allocated_bytes());
    let default_sym = Value::symbol("default");
    for _ in 0..500 {
        let v = builtin_internal_lisp_face_p(&mut eval, vec![default_sym]).unwrap();
        assert!(v.is_vector());
    }
    let bytes_after = crate::tagged::gc::with_tagged_heap(|heap| heap.total_allocated_bytes());
    let grew = bytes_after.saturating_sub(bytes_before);
    assert!(
        grew < 100_000,
        "internal-lisp-face-p must be an allocation-free read; 500 calls grew the \
         tagged heap by {grew} bytes (a re-seed/create-on-miss regression)",
    );
}

#[test]
fn internal_lisp_face_p_nil_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_internal_lisp_face_p(&mut eval, vec![Value::NIL]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn internal_lisp_face_p_rejects_non_nil_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result =
        builtin_internal_lisp_face_p(&mut eval, vec![Value::symbol("default"), Value::fixnum(1)]);
    assert!(result.is_err());
}

#[test]
fn internal_lisp_face_p_with_frame_designator_returns_resolved_vector() {
    crate::test_utils::init_test_tracing();
    clear_font_cache_state();
    let mut eval = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);

    let result = builtin_internal_lisp_face_p(
        &mut eval,
        vec![Value::symbol("default"), Value::make_frame(frame_id.0)],
    )
    .unwrap();
    let values = match result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => result.as_vector_data().unwrap().clone(),
        _ => panic!("expected vector"),
    };
    assert_eq!(values[0].as_symbol_name(), Some("face"));
    assert_eq!(values[1].as_utf8_str(), Some("default"));
    assert_eq!(values[2].as_utf8_str(), Some("default"));
    assert_eq!(values[3].as_symbol_name(), Some("normal"));
    assert_eq!(values[4].as_int(), Some(1));
    assert_eq!(values[5].as_symbol_name(), Some("normal"));
    assert_eq!(values[8].as_symbol_name(), Some("nil"));
    assert!(values[9].as_utf8_str().is_some());
    assert!(values[10].as_utf8_str().is_some());
}

/// Regression guard for the fast-path/known-set desync the footgun review found:
/// `internal-lisp-face-p`'s fast path reads the `face--new-frame-defaults` table,
/// but `clear_created_lisp_face` (source unload) only clears the created-face set.
/// Without also removing the table entry the predicate keeps reporting a stale
/// face (a table hit short-circuits the known-set gate). Assert removal keeps the
/// two stores in agreement.
#[test]
fn internal_lisp_face_p_returns_nil_after_face_table_entry_removed() {
    crate::test_utils::init_test_tracing();
    let face_name = "__neovm_face_unload_desync_test";
    let mut eval = Context::new();

    // Create it: predicate sees it (table entry + created-set both present).
    builtin_internal_make_lisp_face(&mut eval, vec![Value::symbol(face_name)]).unwrap();
    assert!(
        builtin_internal_lisp_face_p(&mut eval, vec![Value::symbol(face_name)])
            .unwrap()
            .is_vector(),
        "freshly made face must be visible to internal-lisp-face-p"
    );

    // Unload it exactly as the source-unload path now does: drop from the
    // created-set AND the canonical table.
    clear_created_lisp_face(face_name);
    crate::emacs_core::xfaces::remove_face_new_frame_defaults_entry(&eval, face_name);

    // The fast path must now agree that the face is gone (would return the stale
    // vector if the table entry had been left behind).
    assert!(
        builtin_internal_lisp_face_p(&mut eval, vec![Value::symbol(face_name)])
            .unwrap()
            .is_nil(),
        "internal-lisp-face-p must return nil after the face is unloaded"
    );
}

#[test]
fn internal_make_lisp_face_creates_symbol_visible_to_internal_lisp_face_p() {
    crate::test_utils::init_test_tracing();
    let face_name = "__neovm_make_face_unit_test";
    let mut eval = Context::new();
    let made = builtin_internal_make_lisp_face(&mut eval, vec![Value::symbol(face_name)]).unwrap();
    assert!(made.is_vector());
    let exists = builtin_internal_lisp_face_p(&mut eval, vec![Value::symbol(face_name)]).unwrap();
    assert!(exists.is_vector());
    assert_eq!(
        eval.obarray().get_property(face_name, "face"),
        face_id_for_name(face_name).map(Value::fixnum),
    );
}

#[test]
fn internal_lisp_face_p_without_frame_returns_global_lface_attributes() {
    crate::test_utils::init_test_tracing();
    clear_font_cache_state();

    let face_name = "__neovm_global_lface_attrs_unit_test";
    let mut eval = Context::new();
    builtin_internal_make_lisp_face(&mut eval, vec![Value::symbol(face_name)]).unwrap();
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol(face_name),
            Value::keyword(":weight"),
            Value::symbol("bold"),
            Value::fixnum(0),
        ],
    )
    .unwrap();
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol(face_name),
            Value::keyword(":slant"),
            Value::symbol("italic"),
            Value::fixnum(0),
        ],
    )
    .unwrap();

    let result = builtin_internal_lisp_face_p(&mut eval, vec![Value::symbol(face_name)]).unwrap();
    let values = match result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => result.as_vector_data().unwrap().clone(),
        _ => panic!("expected vector"),
    };
    assert_eq!(values[5].as_symbol_name(), Some("bold"));
    assert_eq!(values[6].as_symbol_name(), Some("italic"));
}

#[test]
fn internal_set_lisp_face_attribute_accepts_gnu_font_style_aliases() {
    crate::test_utils::init_test_tracing();
    clear_font_cache_state();

    let face_name = "__neovm_lface_style_aliases_unit_test";
    let mut eval = Context::new();
    builtin_internal_make_lisp_face(&mut eval, vec![Value::symbol(face_name)]).unwrap();

    for (attr, value) in [
        (":weight", "ultra-heavy"),
        (":slant", "ro"),
        (":width", "wide"),
    ] {
        builtin_internal_set_lisp_face_attribute(
            &mut eval,
            vec![
                Value::symbol(face_name),
                Value::keyword(attr),
                Value::symbol(value),
                Value::fixnum(0),
            ],
        )
        .unwrap();
    }

    let result = builtin_internal_lisp_face_p(&mut eval, vec![Value::symbol(face_name)]).unwrap();
    let values = result.as_vector_data().expect("lface vector").clone();
    assert_eq!(values[3].as_symbol_name(), Some("wide"));
    assert_eq!(values[5].as_symbol_name(), Some("ultra-heavy"));
    assert_eq!(values[6].as_symbol_name(), Some("ro"));
}

#[test]
fn internal_set_lisp_face_attribute_rejects_numeric_weight_like_gnu() {
    crate::test_utils::init_test_tracing();
    clear_font_cache_state();

    let face_name = "__neovm_lface_numeric_weight_unit_test";
    let mut eval = Context::new();
    builtin_internal_make_lisp_face(&mut eval, vec![Value::symbol(face_name)]).unwrap();

    let result = builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol(face_name),
            Value::keyword(":weight"),
            Value::fixnum(700),
            Value::fixnum(0),
        ],
    );
    assert!(result.is_err());
}

#[test]
fn internal_set_lisp_face_attribute_mutates_live_global_lface_vector() {
    crate::test_utils::init_test_tracing();
    clear_font_cache_state();

    let face_name = "__neovm_live_global_lface_vector_unit_test";
    let mut eval = Context::new();
    builtin_internal_make_lisp_face(&mut eval, vec![Value::symbol(face_name)]).unwrap();
    let vector = builtin_internal_lisp_face_p(&mut eval, vec![Value::symbol(face_name)]).unwrap();

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol(face_name),
            Value::keyword(":weight"),
            Value::symbol("unspecified"),
            Value::fixnum(0),
        ],
    )
    .unwrap();

    let values = vector.as_vector_data().unwrap().clone();
    assert_eq!(values[5].as_symbol_name(), Some(":ignore-defface"));
}

#[test]
fn internal_copy_lisp_face_keeps_global_and_frame_domains_separate() {
    crate::test_utils::init_test_tracing();
    clear_font_cache_state();

    let mut eval = Context::new();
    let frame =
        Value::make_frame(crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0);
    let face_name = "__neovm_copy_face_domains_unit_test";

    builtin_internal_copy_lisp_face(
        &mut eval,
        vec![
            Value::symbol("bold"),
            Value::symbol(face_name),
            frame,
            Value::NIL,
        ],
    )
    .unwrap();

    let frame_vector =
        builtin_internal_lisp_face_p(&mut eval, vec![Value::symbol(face_name), frame]).unwrap();
    let global_vector =
        builtin_internal_lisp_face_p(&mut eval, vec![Value::symbol(face_name)]).unwrap();
    assert_eq!(
        frame_vector.as_vector_data().unwrap()[5].as_symbol_name(),
        Some("bold")
    );
    assert_eq!(
        global_vector.as_vector_data().unwrap()[5].as_symbol_name(),
        Some("unspecified")
    );
}

#[test]
fn internal_make_lisp_face_publishes_known_face_id_property() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let made = builtin_internal_make_lisp_face(&mut eval, vec![Value::symbol("default")]).unwrap();
    assert!(made.is_vector());
    assert_eq!(
        eval.obarray().get_property("default", "face"),
        Some(Value::fixnum(0))
    );
}

#[test]
fn internal_make_lisp_face_sets_gnu_face_id_symbol_property() {
    crate::test_utils::init_test_tracing();
    clear_font_cache_state();

    let mut eval = Context::new();
    let face_name = "__neovm_make_face_id_property_unit_test";
    builtin_internal_make_lisp_face(&mut eval, vec![Value::symbol(face_name)]).unwrap();

    let face_id = eval
        .obarray()
        .get_property(face_name, "face")
        .and_then(|value| value.as_int())
        .expect("new Lisp face should publish its GNU face id");
    assert_eq!(Some(face_id), face_id_for_name(face_name));

    builtin_internal_make_lisp_face(&mut eval, vec![Value::symbol(face_name)]).unwrap();
    let repeated_face_id = eval
        .obarray()
        .get_property(face_name, "face")
        .and_then(|value| value.as_int())
        .expect("existing Lisp face should keep its GNU face id");
    assert_eq!(repeated_face_id, face_id);
}

#[test]
fn internal_make_lisp_face_rejects_non_symbol_and_non_nil_frame() {
    crate::test_utils::init_test_tracing();
    assert!(
        call_font_builtin!(builtin_internal_make_lisp_face, vec![Value::string("foo")]).is_err()
    );
    assert!(
        call_font_builtin!(
            builtin_internal_make_lisp_face,
            vec![Value::symbol("foo"), Value::fixnum(1)]
        )
        .is_err()
    );
}

#[test]
fn internal_copy_lisp_face_returns_to_when_frame_t() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_internal_copy_lisp_face(
        &mut eval,
        vec![
            Value::symbol("bold"),
            Value::symbol("my-face"),
            Value::T,
            Value::NIL,
        ],
    )
    .unwrap();
    assert_eq!(result.as_symbol_name(), Some("my-face"));
}

#[test]
fn internal_copy_lisp_face_sets_gnu_face_id_symbol_property() {
    crate::test_utils::init_test_tracing();
    clear_font_cache_state();

    let mut eval = Context::new();
    let face_name = "__neovm_copy_face_id_property_unit_test";
    builtin_internal_copy_lisp_face(
        &mut eval,
        vec![
            Value::symbol("bold"),
            Value::symbol(face_name),
            Value::T,
            Value::NIL,
        ],
    )
    .unwrap();

    let face_id = eval
        .obarray()
        .get_property(face_name, "face")
        .and_then(|value| value.as_int())
        .expect("copied Lisp face should publish its GNU face id");
    assert_eq!(Some(face_id), face_id_for_name(face_name));
}

#[test]
fn internal_copy_lisp_face_copies_frame_spec_without_runtime_realization() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let frame = Value::make_frame(frame_id.0);
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("bold"),
            Value::keyword("family"),
            Value::string("Serif"),
            frame,
        ],
    )
    .unwrap();

    let copied = builtin_internal_copy_lisp_face(
        &mut eval,
        vec![
            Value::symbol("bold"),
            Value::symbol("copied-face"),
            frame,
            Value::NIL,
        ],
    )
    .unwrap();
    assert_eq!(copied.as_symbol_name(), Some("copied-face"));
    let vector = lookup_frame_lisp_face_vector(&eval, frame_id, "copied-face")
        .expect("copied frame-local Lisp face vector");
    assert_eq!(
        lisp_face_vector_attr(vector, LFaceAttr::Family),
        Some(Value::string("Serif")),
    );
    assert_eq!(
        eval.face_table().resolve("copied-face").family,
        None,
        "copying a Lisp face must not eagerly mutate redisplay's derived table",
    );
    assert_eq!(
        eval.face_table().resolve("copied-face").weight,
        None,
        "even a standard source face must stay unrealized until redisplay",
    );
}

#[test]
fn internal_copy_lisp_face_rejects_non_t_frame_designator() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(
        builtin_internal_copy_lisp_face,
        vec![
            Value::symbol("default"),
            Value::symbol("my-face"),
            Value::NIL,
            Value::NIL,
        ]
    );
    assert!(result.is_err());
}

#[test]
fn internal_copy_lisp_face_validates_new_frame_when_frame_designator_used() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let frame = Value::make_frame(frame_id.0);
    let err_t = builtin_internal_copy_lisp_face(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::symbol("my-face"),
            frame,
            Value::T,
        ],
    );
    assert!(err_t.is_err());

    let err_small_int = builtin_internal_copy_lisp_face(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::symbol("my-face"),
            frame,
            Value::fixnum(1),
        ],
    );
    assert!(err_small_int.is_err());

    let ok = builtin_internal_copy_lisp_face(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::symbol("my-face"),
            frame,
            frame,
        ],
    )
    .unwrap();
    assert_eq!(ok.as_symbol_name(), Some("my-face"));
}

#[test]
fn internal_copy_lisp_face_uses_symbol_checks_before_frame_checks() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(
        builtin_internal_copy_lisp_face,
        vec![
            Value::fixnum(1),
            Value::symbol("my-face"),
            Value::NIL,
            Value::NIL,
        ]
    );
    assert!(result.is_err());
}

#[test]
fn internal_set_lisp_face_attribute_returns_value() {
    crate::test_utils::init_test_tracing();
    let face_name = "__neovm_set_attr_unit_test";
    let mut eval = Context::new();
    let result = builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol(face_name),
            Value::keyword("foreground"),
            Value::string("white"),
        ],
    )
    .unwrap();
    assert_eq!(result.as_symbol_name(), Some(face_name));
}

#[test]
fn internal_get_lisp_face_attribute_default_foreground() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_internal_get_lisp_face_attribute(
        &mut eval,
        vec![Value::symbol("default"), Value::keyword(":foreground")],
    )
    .unwrap();
    assert_eq!(result.as_utf8_str(), Some("unspecified-fg"));
}

#[test]
fn internal_get_lisp_face_attribute_mode_line_returns_unspecified() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_internal_get_lisp_face_attribute(
        &mut eval,
        vec![Value::symbol("mode-line"), Value::keyword(":foreground")],
    )
    .unwrap();
    // GNU `internal-get-lisp-face-attribute` returns the LISP face's slot (never
    // the realized face), so mode-line's unset `:foreground` reports as
    // `unspecified` here rather than the color it happens to realize to on a
    // color-capable display (fixed in a604c3a19; matches this test's own name).
    assert_eq!(result.as_symbol_name(), Some("unspecified"));
}

#[test]
fn internal_get_lisp_face_attribute_defaults_frame_returns_unspecified() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_internal_get_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword(":foreground"),
            Value::T,
        ],
    )
    .unwrap();
    assert_eq!(result.as_symbol_name(), Some("unspecified"));
}

#[test]
fn internal_get_lisp_face_attribute_invalid_face_errors() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(
        builtin_internal_get_lisp_face_attribute,
        vec![Value::symbol("unknown-face"), Value::keyword(":foreground"),]
    );
    assert!(result.is_err());
}

#[test]
fn internal_get_lisp_face_attribute_invalid_plist_face_spreads_signal_data() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let face = Value::list(vec![
        Value::keyword(":background"),
        Value::string("yellow"),
        Value::keyword(":background"),
        Value::string("red"),
    ]);

    let result = builtin_internal_get_lisp_face_attribute(
        &mut eval,
        vec![face, Value::keyword(":background")],
    );
    let Err(Flow::Signal(signal)) = result else {
        panic!("expected invalid face signal");
    };

    assert_eq!(signal.symbol_name(), "error");
    assert_eq!(signal.data[0].as_utf8_str(), Some("Invalid face"));
    assert_eq!(signal.data[1], Value::keyword(":background"));
    assert_eq!(signal.data[2].as_utf8_str(), Some("yellow"));
    assert_eq!(signal.data[3], Value::keyword(":background"));
    assert_eq!(signal.data[4].as_utf8_str(), Some("red"));
}

#[test]
fn internal_get_lisp_face_attribute_invalid_attr_errors() {
    crate::test_utils::init_test_tracing();
    let wrong_type = call_font_builtin!(
        builtin_internal_get_lisp_face_attribute,
        vec![Value::symbol("default"), Value::fixnum(1)]
    );
    assert!(wrong_type.is_err());

    let invalid_name = call_font_builtin!(
        builtin_internal_get_lisp_face_attribute,
        vec![Value::symbol("default"), Value::symbol("bogus"),]
    );
    assert!(invalid_name.is_err());
}

#[test]
fn internal_set_lisp_face_attribute_font_object_derives_font_related_attrs() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let mut font_face = RuntimeFace::new("default");
    font_face.family = Some(Value::string("Hack"));
    font_face.weight = Some(FontWeight::NORMAL);
    font_face.slant = Some(FontSlant::Normal);
    font_face.width = Some(FontWidth::Normal);
    font_face.height = Some(FaceHeight::Absolute(102));
    let font_object = build_font_object(&font_face);
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            font_object,
        ],
    )
    .unwrap();

    assert_eq!(
        builtin_internal_get_lisp_face_attribute(
            &mut eval,
            vec![Value::symbol("default"), Value::keyword(":family"),]
        )
        .unwrap()
        .as_utf8_str(),
        Some("Hack")
    );
    assert_eq!(
        builtin_internal_get_lisp_face_attribute(
            &mut eval,
            vec![Value::symbol("default"), Value::keyword(":weight"),]
        )
        .unwrap()
        .as_symbol_name(),
        Some("regular")
    );
    assert_eq!(
        builtin_internal_get_lisp_face_attribute(
            &mut eval,
            vec![Value::symbol("default"), Value::keyword(":height"),]
        )
        .unwrap()
        .as_int(),
        Some(102)
    );
}

#[test]
fn internal_set_lisp_face_attribute_default_font_spec_float_size_derives_absolute_height() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let font_spec = font_spec(vec![
        Value::keyword("family"),
        Value::string("Monospace"),
        Value::keyword("size"),
        Value::make_float(13.0),
        Value::keyword("weight"),
        Value::symbol("semi-light"),
    ])
    .expect("create font spec");

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![Value::symbol("default"), Value::keyword("font"), font_spec],
    )
    .expect("font-spec float :size should not become a relative default face height");

    assert_eq!(
        builtin_internal_get_lisp_face_attribute(
            &mut eval,
            vec![Value::symbol("default"), Value::keyword(":family"),]
        )
        .expect("default face family")
        .as_utf8_str(),
        Some("Monospace")
    );
    assert_eq!(
        builtin_internal_get_lisp_face_attribute(
            &mut eval,
            vec![Value::symbol("default"), Value::keyword(":height"),]
        )
        .expect("default face height")
        .as_int(),
        Some(130)
    );
    assert_eq!(
        builtin_internal_get_lisp_face_attribute(
            &mut eval,
            vec![Value::symbol("default"), Value::keyword(":weight"),]
        )
        .expect("default face weight")
        .as_symbol_name(),
        Some("semi-light")
    );
}

#[test]
fn internal_set_lisp_face_attribute_eval_uses_live_frame_font_parameter_for_default_face() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let font_name = Value::string("-*-Hack-regular-normal-*-*-102-*-*-*-m-0-iso10646-1");
    let mut font_face = RuntimeFace::new("default");
    font_face.family = Some(Value::string("Hack"));
    font_face.weight = Some(FontWeight::NORMAL);
    font_face.slant = Some(FontSlant::Normal);
    font_face.width = Some(FontWidth::Normal);
    font_face.height = Some(FaceHeight::Absolute(102));
    let font_object = build_font_object(&font_face);

    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.window_system = Some(Value::symbol("neo"));
        frame.set_parameter(Value::symbol("font"), font_name);
        frame.set_parameter(Value::symbol("font-parameter"), font_object);
    }

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            font_name,
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("set live default face font");

    let public_font = builtin_internal_get_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword(":font"),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("default face font");
    let internal_font = eval
        .frames
        .get(frame_id)
        .and_then(|frame| frame.parameter("font-parameter"))
        .expect("internal opened font");
    assert_eq!(public_font, internal_font);
    assert_eq!(
        font_get(vec![internal_font, Value::keyword(":family")])
            .expect("default face font family")
            .as_symbol_name(),
        Some("Hack")
    );
    assert_eq!(
        font_get(vec![internal_font, Value::keyword(":size"),])
            .expect("default face font size")
            .as_int(),
        Some(102)
    );
    assert_eq!(
        font_get(vec![internal_font, Value::keyword(":height")])
            .expect("internal opened font height")
            .as_int(),
        Some(102)
    );
    assert!(
        fontp(vec![public_font, Value::symbol("font-object")])
            .expect("default face font type")
            .is_truthy()
    );
    assert_eq!(
        builtin_internal_get_lisp_face_attribute(
            &mut eval,
            vec![
                Value::symbol("default"),
                Value::keyword(":family"),
                Value::make_frame(frame_id.0),
            ],
        )
        .expect("default face family")
        .as_utf8_str(),
        Some("Hack")
    );
    assert_eq!(
        builtin_internal_get_lisp_face_attribute(
            &mut eval,
            vec![
                Value::symbol("default"),
                Value::keyword(":height"),
                Value::make_frame(frame_id.0),
            ],
        )
        .expect("default face height")
        .as_int(),
        Some(102)
    );
}

#[test]
fn internal_set_lisp_face_attribute_eval_realizes_string_font_requests_for_live_default_face() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.window_system = Some(Value::symbol("neo"));
    }
    eval.set_display_host(Box::new(LiveFrameFontDisplayHost {
        realized: Some(resolved_frame_font(
            "Noto Sans Mono",
            "NotoSansMono-Regular",
            160,
            FontPxProbeResult {
                pixel_size: 22,
                height: 31,
                ascent: 23,
                descent: 8,
                max_width: 13,
                space_width: 13,
                average_width: 13,
            },
        )),
    }));

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            Value::string("Noto Sans Mono-16"),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("set live default face font from string");

    let frame = eval
        .frame_manager()
        .get(frame_id)
        .expect("selected frame after font change");
    assert_eq!(
        frame
            .parameter("font")
            .and_then(|value| value.as_utf8_str()),
        Some("Noto Sans Mono-16")
    );
    let font_parameter = frame
        .parameter("font-parameter")
        .expect("font-parameter should be set");
    assert!(
        fontp(vec![font_parameter, Value::symbol("font-object")])
            .expect("font-object check")
            .is_truthy()
    );
    assert_eq!(frame.char_width, 13.0);
    assert_eq!(frame.char_height, 31.0);
    assert_eq!(frame.font_pixel_size, 22.0);

    let default_font = builtin_internal_get_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword(":font"),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("default face font");
    let opened_font = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.parameter("font-parameter"))
        .expect("font-parameter should retain the realized font object");
    assert!(
        fontp(vec![default_font, Value::symbol("font-object")])
            .expect("live default face :font type check")
            .is_truthy(),
        "GNU exposes a realized font object from a live graphical face"
    );
    assert_eq!(default_font, opened_font);
    assert_eq!(
        font_get(vec![opened_font, Value::keyword(":family")])
            .expect("default font family")
            .as_symbol_name(),
        Some("Noto Sans Mono")
    );
    assert_eq!(
        builtin_internal_get_lisp_face_attribute(
            &mut eval,
            vec![
                Value::symbol("default"),
                Value::keyword(":height"),
                Value::make_frame(frame_id.0),
            ],
        )
        .expect("default face height")
        .as_int(),
        Some(160)
    );
}

#[test]
fn unresolved_live_default_font_request_preserves_opened_font_and_geometry() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = ensure_selected_gui_frame(&mut eval);
    let mut old_face = RuntimeFace::new("default");
    old_face.family = Some(Value::string("Old Mono"));
    old_face.height = Some(FaceHeight::Absolute(110));
    let old_font = build_font_object(&old_face);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.set_known_parameter(FrameParam::Font, Value::string("Old Mono-11"));
        frame.set_parameter(Value::symbol("font-parameter"), old_font);
        frame.font_pixel_size = 15.0;
        frame.char_width = 8.0;
        frame.char_height = 19.0;
    }
    eval.set_display_host(Box::new(LiveFrameFontDisplayHost { realized: None }));

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            Value::string("Missing Font-17"),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("an unresolved public selector remains representable");

    let frame = eval.frame_manager().get(frame_id).expect("selected frame");
    assert_eq!(frame.parameter("font-parameter"), Some(old_font));
    assert_eq!(frame.font_pixel_size, 15.0);
    assert_eq!(frame.char_width, 8.0);
    assert_eq!(frame.char_height, 19.0);
}

#[test]
fn opened_live_default_font_request_applies_its_stored_geometry() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = ensure_selected_gui_frame(&mut eval);
    let mut face = RuntimeFace::new("default");
    face.family = Some(Value::string("Exact Mono"));
    face.height = Some(FaceHeight::Absolute(170));
    let resolved = resolved_frame_font(
        "Exact Mono",
        "ExactMono-Regular",
        170,
        FontPxProbeResult {
            pixel_size: 23,
            height: 31,
            ascent: 24,
            descent: 7,
            max_width: 14,
            space_width: 12,
            average_width: 13,
        },
    );
    let opened = opened_font_from_resolved_match(
        &face,
        &ResolvedFontMatch {
            font: resolved.font,
            glyph_code: None,
        },
    );

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            opened,
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("set an already-opened frame font");

    let frame = eval.frame_manager().get(frame_id).expect("selected frame");
    assert_eq!(frame.parameter("font-parameter"), Some(opened));
    assert_eq!(frame.font_pixel_size, 23.0);
    assert_eq!(frame.char_width, 13.0);
    assert_eq!(frame.char_height, 31.0);
}

#[test]
fn internal_set_lisp_face_attribute_eval_uses_resolved_point_height_when_font_request_has_no_size()
{
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.window_system = Some(Value::symbol("neo"));
    }
    eval.set_display_host(Box::new(LiveFrameFontDisplayHost {
        realized: Some(resolved_frame_font(
            "Noto Sans Mono",
            "NotoSansMono-Regular",
            102,
            FontPxProbeResult {
                pixel_size: 22,
                height: 31,
                ascent: 23,
                descent: 8,
                max_width: 13,
                space_width: 13,
                average_width: 13,
            },
        )),
    }));

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            Value::string("Noto Sans Mono"),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("set live default face font without size");

    let default_font = builtin_internal_get_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword(":font"),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("default face font");
    let opened_font = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.parameter("font-parameter"))
        .expect("font-parameter should retain the realized font object");
    assert_eq!(default_font, opened_font);
    assert_eq!(
        font_get(vec![opened_font, Value::keyword(":size")])
            .expect("default font size")
            .as_int(),
        Some(22)
    );
    assert_eq!(
        builtin_internal_get_lisp_face_attribute(
            &mut eval,
            vec![
                Value::symbol("default"),
                Value::keyword(":height"),
                Value::make_frame(frame_id.0),
            ],
        )
        .expect("default face height")
        .as_int(),
        Some(102)
    );
}

#[test]
fn face_font_eval_returns_font_name_on_live_gui_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let frame = eval
        .frame_manager_mut()
        .get_mut(frame_id)
        .expect("selected frame");
    frame.window_system = Some(Value::symbol("neo"));
    frame.font_pixel_size = 16.0;
    frame.char_width = 8.0;
    frame.char_height = 16.0;

    let result = builtin_face_font(&mut eval, vec![Value::symbol("default")]).unwrap();
    assert!(result.is_string());
    assert!(result.as_utf8_str().is_some_and(|name| !name.is_empty()));
}

#[test]
fn internal_lisp_face_attribute_values_discrete_boolean_attrs() {
    crate::test_utils::init_test_tracing();
    let result =
        builtin_internal_lisp_face_attribute_values(vec![Value::keyword(":underline")]).unwrap();
    let vals = list_to_vec(&result).expect("list");
    assert_eq!(vals, vec![Value::T, Value::NIL]);
}

#[test]
fn internal_lisp_face_attribute_values_non_discrete_attr_is_nil() {
    crate::test_utils::init_test_tracing();
    let result =
        builtin_internal_lisp_face_attribute_values(vec![Value::keyword(":weight")]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn internal_lisp_face_attribute_values_rejects_non_symbol() {
    crate::test_utils::init_test_tracing();
    let result = builtin_internal_lisp_face_attribute_values(vec![Value::fixnum(1)]);
    assert!(result.is_err());
}

#[test]
fn set_face_attr_alias_domain_matches_gnu_legacy_keywords() {
    assert_eq!(
        SetFaceAttrAlias::from_keyword(":bold"),
        Some(SetFaceAttrAlias::Bold)
    );
    assert_eq!(
        SetFaceAttrAlias::from_keyword(":italic"),
        Some(SetFaceAttrAlias::Italic)
    );
    assert_eq!(SetFaceAttrAlias::Bold.keyword(), ":bold");
    assert_eq!(SetFaceAttrAlias::Italic.keyword(), ":italic");
    assert_eq!(SetFaceAttrAlias::from_keyword(":Bold"), None);
    assert_eq!(SetFaceAttrAlias::from_keyword(":ITALIC"), None);
}

#[test]
fn internal_lisp_face_empty_p_selected_frame_default_is_not_empty() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(
        builtin_internal_lisp_face_empty_p,
        vec![Value::symbol("default")]
    )
    .unwrap();
    assert!(result.is_nil());
}

#[test]
fn internal_lisp_face_empty_p_accepts_string_face_name() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(
        builtin_internal_lisp_face_empty_p,
        vec![Value::string("default")]
    )
    .unwrap();
    assert!(result.is_nil());
}

#[test]
fn internal_lisp_face_empty_p_defaults_frame_is_empty() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(
        builtin_internal_lisp_face_empty_p,
        vec![Value::symbol("default"), Value::T]
    )
    .unwrap();
    assert!(result.is_truthy());
}

#[test]
fn internal_lisp_face_empty_p_rejects_non_nil_non_t_frame_designator() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(
        builtin_internal_lisp_face_empty_p,
        vec![Value::symbol("default"), Value::fixnum(1)]
    );
    assert!(result.is_err());
    let frame_result = call_font_builtin!(
        builtin_internal_lisp_face_empty_p,
        vec![Value::symbol("default"), Value::make_frame(1)]
    );
    assert!(frame_result.is_err());
}

#[test]
fn internal_lisp_face_comparators_accept_frame_handles() {
    crate::test_utils::init_test_tracing();
    let frame = Value::make_frame(FRAME_ID_BASE);
    let empty_result = call_font_builtin!(
        builtin_internal_lisp_face_empty_p,
        vec![Value::symbol("default"), frame]
    )
    .unwrap();
    assert!(empty_result.is_nil());

    let equal_result = call_font_builtin!(
        builtin_internal_lisp_face_equal_p,
        vec![Value::symbol("default"), Value::symbol("mode-line"), frame,]
    )
    .unwrap();
    assert!(equal_result.is_nil());
}

#[test]
fn internal_lisp_face_equal_p_selected_frame_distinguishes_faces() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(
        builtin_internal_lisp_face_equal_p,
        vec![Value::symbol("default"), Value::symbol("mode-line")]
    )
    .unwrap();
    assert!(result.is_nil());
}

#[test]
fn internal_lisp_face_equal_p_defaults_frame_treats_faces_as_equal() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(
        builtin_internal_lisp_face_equal_p,
        vec![
            Value::symbol("default"),
            Value::symbol("mode-line"),
            Value::T,
        ]
    )
    .unwrap();
    assert!(result.is_truthy());
}

#[test]
fn internal_lisp_face_equal_p_accepts_string_face_names() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(
        builtin_internal_lisp_face_equal_p,
        vec![Value::string("default"), Value::string("default")]
    )
    .unwrap();
    assert!(result.is_truthy());
}

#[test]
fn internal_merge_in_global_face_rejects_non_frame_designator() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(
        builtin_internal_merge_in_global_face,
        vec![Value::symbol("default"), Value::NIL]
    );
    assert!(result.is_err());
    let frame_handle_result = call_font_builtin!(
        builtin_internal_merge_in_global_face,
        vec![Value::symbol("default"), Value::make_frame(1)]
    );
    assert!(frame_handle_result.is_err());
}

#[test]
fn internal_merge_in_global_face_copies_defaults_into_selected_face() {
    crate::test_utils::init_test_tracing();
    let face_name = "__neovm_merge_face_unit_test";
    let mut eval = Context::new();
    let frame =
        Value::make_frame(crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0);
    let _ = builtin_internal_make_lisp_face(&mut eval, vec![Value::symbol(face_name)]).unwrap();
    let _ = builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol(face_name),
            Value::keyword("foreground"),
            Value::string("white"),
            Value::T,
        ],
    )
    .unwrap();
    let merged =
        builtin_internal_merge_in_global_face(&mut eval, vec![Value::symbol(face_name), frame])
            .unwrap();
    assert!(merged.is_nil());
    let got = builtin_internal_get_lisp_face_attribute(
        &mut eval,
        vec![Value::symbol(face_name), Value::keyword(":foreground")],
    )
    .unwrap();
    assert_eq!(got.as_utf8_str(), Some("white"));
}

#[test]
fn internal_lisp_face_helpers_accept_frame_handles() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let frame =
        Value::make_frame(crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0);

    let descriptor =
        builtin_internal_lisp_face_p(&mut eval, vec![Value::symbol("default"), frame]).unwrap();
    assert!(descriptor.is_vector());

    let face_name = "__neovm_face_frame_handle_unit_test";
    let made =
        builtin_internal_make_lisp_face(&mut eval, vec![Value::symbol(face_name), frame]).unwrap();
    assert!(made.is_vector());

    let copied = builtin_internal_copy_lisp_face(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::symbol(face_name),
            frame,
            Value::NIL,
        ],
    )
    .unwrap();
    assert_eq!(copied.as_symbol_name(), Some(face_name));

    let set = builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("foreground"),
            Value::string("red"),
            frame,
        ],
    )
    .unwrap();
    assert_eq!(set, Value::symbol("default"));

    let got = builtin_internal_get_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword(":foreground"),
            frame,
        ],
    )
    .unwrap();
    assert_eq!(got.as_utf8_str(), Some("red"));
}

#[test]
fn face_attribute_relative_p_height_non_fixnum_is_relative() {
    crate::test_utils::init_test_tracing();
    let result =
        builtin_face_attribute_relative_p(vec![Value::keyword("height"), Value::NIL]).unwrap();
    assert!(result.is_truthy());
}

#[test]
fn face_attribute_relative_p_height_fixnum_is_not_relative() {
    crate::test_utils::init_test_tracing();
    let result =
        builtin_face_attribute_relative_p(vec![Value::keyword("height"), Value::fixnum(1)])
            .unwrap();
    assert!(result.is_nil());
}

#[test]
fn face_attribute_relative_p_non_height_attribute_is_nil() {
    crate::test_utils::init_test_tracing();
    let result =
        builtin_face_attribute_relative_p(vec![Value::keyword("weight"), Value::symbol("foo")])
            .unwrap();
    assert!(result.is_nil());
}

#[test]
fn face_attribute_relative_p_unspecified_is_relative() {
    crate::test_utils::init_test_tracing();
    let result = builtin_face_attribute_relative_p(vec![
        Value::keyword("weight"),
        Value::symbol("unspecified"),
    ])
    .unwrap();
    assert!(result.is_truthy());
}

#[test]
fn merge_face_attribute_non_unspecified() {
    crate::test_utils::init_test_tracing();
    let result = builtin_merge_face_attribute(vec![
        Value::keyword("foreground"),
        Value::string("red"),
        Value::string("blue"),
    ])
    .unwrap();
    assert_eq!(result.as_utf8_str(), Some("red"));
}

#[test]
fn merge_face_attribute_unspecified() {
    crate::test_utils::init_test_tracing();
    let result = builtin_merge_face_attribute(vec![
        Value::keyword("foreground"),
        Value::symbol("unspecified"),
        Value::string("blue"),
    ])
    .unwrap();
    assert_eq!(result.as_utf8_str(), Some("blue"));
}

#[test]
fn merge_face_attribute_height_relative_over_absolute() {
    crate::test_utils::init_test_tracing();
    let result = builtin_merge_face_attribute(vec![
        Value::keyword("height"),
        Value::make_float(1.5),
        Value::fixnum(120),
    ])
    .unwrap();
    assert_eq!(result, Value::fixnum(180));
}

#[test]
fn merge_face_attribute_height_relative_over_relative() {
    crate::test_utils::init_test_tracing();
    let result = builtin_merge_face_attribute(vec![
        Value::keyword("height"),
        Value::make_float(1.5),
        Value::make_float(1.2),
    ])
    .unwrap();
    match result.kind() {
        ValueKind::Float => {
            let value = result.as_float().unwrap();
            assert!((value - 1.8).abs() < 1e-9);
        }
        other => panic!("expected float result, got {other:?}"),
    }
}

#[test]
fn internal_set_face_height_accepts_function_height_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("neo-height-face"),
            Value::keyword(":height"),
            Value::symbol("identity"),
            Value::NIL,
        ],
    )
    .expect("GNU accepts function-valued non-default face height");

    let stored = builtin_internal_get_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("neo-height-face"),
            Value::keyword(":height"),
            Value::NIL,
        ],
    )
    .expect("read stored height");
    assert_eq!(stored, Value::symbol("identity"));
}

#[test]
fn merge_face_attribute_height_calls_function_height_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let result = builtin_merge_face_attribute_with_eval(
        &mut eval,
        vec![
            Value::keyword("height"),
            Value::symbol("1-"),
            Value::fixnum(120),
        ],
    )
    .expect("merge function height");
    assert_eq!(result, Value::fixnum(119));
}

#[test]
fn face_list_orders_default_last_and_includes_dynamic_faces() {
    crate::test_utils::init_test_tracing();
    clear_font_cache_state();
    let mut eval = Context::new();
    builtin_internal_make_lisp_face(&mut eval, vec![Value::symbol("__neovm_face_list_dynamic")])
        .expect("create dynamic face");

    let result = builtin_face_list(vec![]).unwrap();
    let faces = list_to_vec(&result).unwrap();
    let names: Vec<&str> = faces.iter().filter_map(|v| v.as_symbol_name()).collect();
    assert!(names.contains(&"default"));
    assert!(names.contains(&"bold"));
    assert!(names.contains(&"cursor"));
    assert!(names.contains(&"mode-line"));
    assert!(names.contains(&"tool-bar"));
    assert!(names.contains(&"tab-bar"));
    assert!(names.contains(&"tab-line"));
    assert!(names.contains(&"__neovm_face_list_dynamic"));
    assert_eq!(names.last().copied(), Some("default"));
}

#[test]
fn color_defined_p_known_and_unknown() {
    crate::test_utils::init_test_tracing();
    let result = builtin_color_defined_p(vec![Value::string("red")]).unwrap();
    assert!(result.is_truthy());

    let missing = builtin_color_defined_p(vec![Value::string("anything")]).unwrap();
    assert!(missing.is_nil());

    let invalid_hex = builtin_color_defined_p(vec![Value::string("#ggg")]).unwrap();
    assert!(invalid_hex.is_nil());

    let non_string = builtin_color_defined_p(vec![Value::fixnum(1)]).unwrap();
    assert!(non_string.is_nil());
}

#[test]
fn color_queries_validate_optional_device_arg() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_color_defined_p(vec![Value::string("red"), Value::fixnum(1)]).is_err());
    assert!(builtin_color_values(vec![Value::string("red"), Value::fixnum(1)]).is_err());
    assert!(builtin_defined_colors(vec![Value::fixnum(1)]).is_err());
    assert!(builtin_color_defined_p(vec![Value::string("red"), Value::make_frame(1)]).is_err());
    assert!(builtin_color_values(vec![Value::string("red"), Value::make_frame(1)]).is_err());
    assert!(builtin_defined_colors(vec![Value::make_frame(1)]).is_err());
    assert!(
        builtin_color_defined_p(vec![Value::string("red"), Value::make_frame(FRAME_ID_BASE)])
            .is_ok()
    );
    assert!(
        builtin_color_values(vec![Value::string("red"), Value::make_frame(FRAME_ID_BASE)]).is_ok()
    );
    assert!(builtin_defined_colors(vec![Value::make_frame(FRAME_ID_BASE)]).is_ok());
}

#[test]
fn color_values_named_black() {
    crate::test_utils::init_test_tracing();
    let result = builtin_color_values(vec![Value::string("black")]).unwrap();
    let rgb = list_to_vec(&result).unwrap();
    assert_eq!(rgb.len(), 3);
    assert_eq!(rgb[0].as_int(), Some(0));
    assert_eq!(rgb[1].as_int(), Some(0));
    assert_eq!(rgb[2].as_int(), Some(0));
}

#[test]
fn color_values_named_white() {
    crate::test_utils::init_test_tracing();
    let result = builtin_color_values(vec![Value::string("white")]).unwrap();
    let rgb = list_to_vec(&result).unwrap();
    assert_eq!(rgb[0].as_int(), Some(65535));
    assert_eq!(rgb[1].as_int(), Some(65535));
    assert_eq!(rgb[2].as_int(), Some(65535));
}

#[test]
fn color_values_hex_rrggbb() {
    crate::test_utils::init_test_tracing();
    // Hex colors are approximated to terminal palette colors in batch mode.
    let result = builtin_color_values(vec![Value::string("#FF0000")]).unwrap();
    let rgb = list_to_vec(&result).unwrap();
    assert_eq!(rgb[0].as_int(), Some(65535));
    assert_eq!(rgb[1].as_int(), Some(0));
    assert_eq!(rgb[2].as_int(), Some(0));
}

#[test]
fn color_values_hex_short() {
    crate::test_utils::init_test_tracing();
    // #F00 resolves and approximates to red.
    let result = builtin_color_values(vec![Value::string("#F00")]).unwrap();
    let rgb = list_to_vec(&result).unwrap();
    assert_eq!(rgb[0].as_int(), Some(65535));
    assert_eq!(rgb[1].as_int(), Some(0));
    assert_eq!(rgb[2].as_int(), Some(0));
}

#[test]
fn color_values_hex_12digit() {
    crate::test_utils::init_test_tracing();
    // 12-digit hex resolves and approximates to red.
    let result = builtin_color_values(vec![Value::string("#FFFF00000000")]).unwrap();
    let rgb = list_to_vec(&result).unwrap();
    assert_eq!(rgb[0].as_int(), Some(65535));
    assert_eq!(rgb[1].as_int(), Some(0));
    assert_eq!(rgb[2].as_int(), Some(0));
}

#[test]
fn color_values_unknown_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_color_values(vec![Value::string("nonexistent-color")]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn color_values_wrong_type_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_color_values(vec![Value::fixnum(42)]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn defined_colors_returns_list() {
    crate::test_utils::init_test_tracing();
    let result = builtin_defined_colors(vec![]).unwrap();
    assert!(result.is_list());
    assert!(!result.is_nil());
    let colors = list_to_vec(&result).expect("defined-colors list");
    assert_eq!(colors.len(), 8);
    assert_eq!(colors[0].as_utf8_str(), Some("black"));
    assert_eq!(colors[7].as_utf8_str(), Some("white"));
}

#[test]
fn face_id_rejects_non_symbol_faces() {
    crate::test_utils::init_test_tracing();
    let result = builtin_face_id(vec![Value::symbol("default")]).unwrap();
    assert_eq!(result.as_int(), Some(0));
}

#[test]
fn face_id_known_faces_use_oracle_ids() {
    crate::test_utils::init_test_tracing();
    let faces = [
        ("default", 0),
        ("bold", 1),
        ("italic", 2),
        ("bold-italic", 3),
        ("mode-line", 25),
        ("mode-line-active", 26),
        ("mode-line-inactive", 27),
        ("header-line", 31),
        ("header-line-active", 33),
        ("header-line-inactive", 34),
        ("margin", 42),
        ("fringe", 43),
        ("scroll-bar", 44),
        ("cursor", 46),
        ("tool-bar", 48),
        ("tab-line", 50),
        ("tab-line-active", 51),
        ("tab-line-inactive", 52),
        ("menu", 53),
        ("tooltip", 185),
    ];
    for (face, id) in faces {
        let value = builtin_face_id(vec![Value::symbol(face)]).unwrap();
        assert_eq!(value.as_int(), Some(id), "face-id mismatch for {face}");
    }
    assert_eq!(FIRST_DYNAMIC_FACE_ID, 186);
}

#[test]
fn gnu_bootstrap_lisp_face_ids_round_trip_names() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        GNU_BOOTSTRAP_LISP_FACES.len(),
        FIRST_DYNAMIC_FACE_ID as usize
    );
    for (expected_id, face) in GNU_BOOTSTRAP_LISP_FACES.iter().copied().enumerate() {
        assert_eq!(face.id(), expected_id as i64);
        assert_eq!(known_face_id(face.name()), Some(expected_id as i64));
    }
    assert_eq!(known_face_id("isearch-group-1"), Some(114));
    assert_eq!(known_face_id("isearch-group-2"), Some(115));
    assert_eq!(known_face_id("unknown-face"), None);
}

#[test]
fn face_id_accepts_optional_frame_argument() {
    crate::test_utils::init_test_tracing();
    let result = builtin_face_id(vec![Value::symbol("default"), Value::NIL]).unwrap();
    assert_eq!(result.as_int(), Some(0));
}

#[test]
fn face_id_assigns_dynamic_id_for_created_faces() {
    crate::test_utils::init_test_tracing();
    let face_name = "__neovm_face_id_dynamic_unit_test";
    let _ = call_font_builtin!(
        builtin_internal_make_lisp_face,
        vec![Value::symbol(face_name)]
    )
    .unwrap();
    let first = builtin_face_id(vec![Value::symbol(face_name)]).unwrap();
    let second = builtin_face_id(vec![Value::symbol(face_name)]).unwrap();
    assert_eq!(first, second);
}

#[test]
fn face_id_rejects_invalid_face() {
    crate::test_utils::init_test_tracing();
    let result = builtin_face_id(vec![Value::fixnum(1)]);
    assert!(result.is_err());
}

#[test]
fn internal_get_lisp_face_attribute_eval_reads_live_face_table() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // internal-get-lisp-face-attribute reads the live LISP face table (not the
    // realized runtime face, dropped in a604c3a19), so a background written
    // through the Lisp face path must be reflected immediately on read-back.
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("mode-line"),
            Value::keyword(":background"),
            Value::string("grey75"),
            Value::T,
        ],
    )
    .expect("set live mode-line background");

    let value = builtin_internal_get_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("mode-line"),
            Value::keyword(":background"),
            Value::T,
        ],
    )
    .expect("live face attribute");

    assert_eq!(value, Value::string("grey75"));
}

#[test]
fn internal_merge_in_global_face_updates_frame_spec_without_runtime_realization() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let face = Value::symbol("__neovm_internal_merge_global_face_eval");
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0 as i64;

    builtin_internal_make_lisp_face(&mut eval, vec![face])
        .expect("create dynamic face in live face table");
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            face,
            Value::keyword(":background"),
            Value::string("grey85"),
            Value::T,
        ],
    )
    .expect("set defaults background");
    builtin_internal_merge_in_global_face(&mut eval, vec![face, Value::fixnum(frame_id)])
        .expect("merge defaults into selected live face");

    assert_eq!(
        eval.face_table()
            .resolve("__neovm_internal_merge_global_face_eval")
            .background,
        None,
        "merging Lisp defaults must not eagerly mutate redisplay's derived table",
    );

    let value = builtin_internal_get_lisp_face_attribute(
        &mut eval,
        vec![face, Value::keyword(":background"), Value::NIL],
    )
    .expect("read merged live background");

    assert_eq!(value, Value::string("grey85"));
}

#[test]
fn runtime_face_sync_uses_frame_lisp_face_vector_as_source_of_truth() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let vector = ensure_frame_lisp_face_vector(
        &mut eval,
        frame_id,
        "default",
        FrameFaceInitial::SelectedBase,
    )
    .expect("default frame face vector");

    set_lisp_face_vector_attr(
        vector,
        crate::face::LFaceAttr::Foreground,
        Value::string("#bbc2cf"),
    );
    set_lisp_face_vector_attr(
        vector,
        crate::face::LFaceAttr::Background,
        Value::string("#282c34"),
    );
    eval.set_face_attribute(
        "default",
        crate::face::LFaceAttr::Foreground,
        FaceAttrValue::Color(Color::rgb(255, 255, 255)),
    );
    eval.set_face_attribute(
        "default",
        crate::face::LFaceAttr::Background,
        FaceAttrValue::Color(Color::rgb(255, 255, 255)),
    );

    sync_runtime_face_table_from_frame_lisp_faces(&mut eval, frame_id);

    let face = eval.face_table().resolve("default");
    assert_eq!(face.foreground, Color::from_hex("#bbc2cf"));
    assert_eq!(face.background, Color::from_hex("#282c34"));
}

#[test]
fn runtime_face_sync_realizes_default_colors_from_frame_parameters() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let vector = ensure_frame_lisp_face_vector(
        &mut eval,
        frame_id,
        "default",
        FrameFaceInitial::SelectedBase,
    )
    .expect("default frame face vector");
    set_lisp_face_vector_attr(
        vector,
        crate::face::LFaceAttr::Foreground,
        Value::string("unspecified-fg"),
    );
    set_lisp_face_vector_attr(
        vector,
        crate::face::LFaceAttr::Background,
        Value::string("unspecified-bg"),
    );
    let frame = eval
        .frame_manager_mut()
        .get_mut(frame_id)
        .expect("selected frame");
    frame.set_known_parameter(FrameParam::ForegroundColor, Value::string("#51afef"));
    frame.set_known_parameter(FrameParam::BackgroundColor, Value::string("#1d2026"));

    sync_runtime_face_table_from_frame_lisp_faces(&mut eval, frame_id);

    let face = eval.face_table().resolve("default");
    assert_eq!(face.foreground, Color::from_hex("#51afef"));
    assert_eq!(face.background, Color::from_hex("#1d2026"));
}

#[test]
fn redisplay_face_preparation_reuses_generation_until_a_lisp_face_changes() {
    crate::test_utils::init_test_tracing();
    clear_font_cache_state();
    let mut eval = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let frame = Value::make_frame(frame_id.0);

    assert!(
        eval.sync_runtime_faces_for_frame(frame_id),
        "the first redisplay preparation must materialize the frame's faces",
    );
    assert!(
        !eval.sync_runtime_faces_for_frame(frame_id),
        "an unchanged face generation must reuse the derived runtime table",
    );

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("mode-line"),
            Value::keyword(":foreground"),
            Value::string("#c678dd"),
            frame,
        ],
    )
    .unwrap();

    assert!(
        eval.sync_runtime_faces_for_frame(frame_id),
        "a Lisp face mutation must invalidate redisplay's derived table",
    );
    assert_eq!(
        eval.face_table().resolve("mode-line").foreground,
        Color::from_hex("#c678dd"),
    );
}

#[test]
fn default_face_font_attr_update_refreshes_live_frame_font_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let frame_id = ensure_selected_gui_frame(&mut eval);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.install_gnu_gui_default_parameters();
        frame.set_known_parameter(FrameParam::Font, Value::string("JetBrains Mono-10"));
        frame.char_width = 7.8;
        frame.char_height = 18.0;
        frame.font_pixel_size = 14.0;
    }
    eval.set_display_host(Box::new(LiveFrameFontDisplayHost {
        realized: Some(resolved_frame_font(
            "JetBrains Mono",
            "JetBrainsMono-Regular",
            90,
            FontPxProbeResult {
                pixel_size: 13,
                height: 17,
                ascent: 13,
                descent: 4,
                max_width: 7,
                space_width: 7,
                average_width: 7,
            },
        )),
    }));

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("family"),
            Value::string("JetBrains Mono"),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("set default family");
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("height"),
            Value::fixnum(90),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("set default height");

    let frame = eval.frame_manager().get(frame_id).expect("selected frame");
    assert!(
        frame
            .parameter("font-parameter")
            .is_some_and(|value| value.is_font_object()),
        "default face font attr changes should refresh internal font-parameter"
    );
    assert_eq!(frame.char_width, 7.0);
    assert_eq!(frame.char_height, 17.0);
    assert_eq!(frame.font_pixel_size, 13.0);
    assert_ne!(
        frame.known_parameter(FrameParam::Font),
        Some(Value::string("JetBrains Mono-10")),
        "frame font parameter must not remain at the stale pre-face-change font"
    );
}

#[test]
fn internal_get_lisp_face_attribute_eval_prefers_explicit_lisp_face_values() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let face = Value::symbol("__neovm_internal_get_lisp_face_attribute_eval_prefers_lisp");

    builtin_internal_make_lisp_face(&mut eval, vec![face])
        .expect("create dynamic face in live face table");
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            face,
            Value::keyword(":foreground"),
            Value::string("red"),
            Value::NIL,
        ],
    )
    .expect("set selected foreground");

    let value = builtin_internal_get_lisp_face_attribute(
        &mut eval,
        vec![face, Value::keyword(":foreground"), Value::NIL],
    )
    .expect("read selected foreground");

    assert_eq!(value, Value::string("red"));
}

#[test]
fn internal_face_x_get_resource_returns_nil_for_string_args() {
    crate::test_utils::init_test_tracing();
    let result = builtin_internal_face_x_get_resource(vec![
        Value::string("font"),
        Value::string("Font"),
        Value::NIL,
    ])
    .unwrap();
    assert!(result.is_nil());
}

#[test]
fn internal_face_x_get_resource_validates_string_args_and_arity() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_internal_face_x_get_resource(vec![]).is_err());
    assert!(builtin_internal_face_x_get_resource(vec![Value::NIL]).is_err());
    assert!(builtin_internal_face_x_get_resource(vec![Value::NIL, Value::string("Font")]).is_err());
    assert!(builtin_internal_face_x_get_resource(vec![Value::string("font"), Value::NIL]).is_err());
}

#[test]
fn internal_set_font_selection_order_accepts_valid_order() {
    crate::test_utils::init_test_tracing();
    let result = builtin_internal_set_font_selection_order(vec![Value::list(vec![
        Value::keyword(":width"),
        Value::keyword(":height"),
        Value::keyword(":weight"),
        Value::keyword(":slant"),
    ])])
    .unwrap();
    assert!(result.is_nil());
}

#[test]
fn internal_set_font_selection_order_rejects_invalid_order() {
    crate::test_utils::init_test_tracing();
    let result =
        builtin_internal_set_font_selection_order(vec![Value::list(vec![Value::symbol("x")])]);
    assert!(result.is_err());
}

#[test]
fn internal_set_alternative_font_family_alist_returns_converted_list() {
    crate::test_utils::init_test_tracing();
    let result = builtin_internal_set_alternative_font_family_alist(vec![Value::NIL]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn internal_set_alternative_font_family_alist_converts_strings_to_symbols() {
    crate::test_utils::init_test_tracing();
    let input = Value::list(vec![Value::list(vec![
        Value::string("Foo"),
        Value::string("Bar"),
    ])]);
    let result = builtin_internal_set_alternative_font_family_alist(vec![input]).unwrap();
    let outer = list_to_vec(&result).expect("outer list");
    let inner = list_to_vec(&outer[0]).expect("inner list");
    assert_eq!(inner[0].as_symbol_name(), Some("Foo"));
    assert_eq!(inner[1].as_symbol_name(), Some("Bar"));
}

#[test]
fn internal_set_alternative_font_family_alist_accepts_raw_unibyte_strings() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let expected = crate::emacs_core::builtins::lisp_string_to_runtime_string(raw);
    let input = Value::list(vec![Value::list(vec![raw])]);
    let result = builtin_internal_set_alternative_font_family_alist(vec![input]).unwrap();
    let outer = list_to_vec(&result).expect("outer list");
    let inner = list_to_vec(&outer[0]).expect("inner list");
    // Issue #131: the family is interned faithfully, so its symbol name keeps the
    // raw byte (0xFF) instead of the PUA-sentinel storage form.
    let crate::emacs_core::value::ValueKind::Symbol(sym_id) = inner[0].kind() else {
        panic!("expected interned symbol");
    };
    assert_eq!(
        crate::emacs_core::intern::resolve_sym_lisp_string(sym_id).as_bytes(),
        &[0xFF]
    );
    assert_eq!(alternative_font_families(&expected), vec![expected]);
}

#[test]
fn internal_set_alternative_font_family_alist_updates_family_lookup_order() {
    crate::test_utils::init_test_tracing();
    let input = Value::list(vec![Value::list(vec![
        Value::string("Noto Sans Mono"),
        Value::string("Noto Sans Mono CJK SC"),
        Value::string("Sarasa Gothic CL"),
    ])]);
    builtin_internal_set_alternative_font_family_alist(vec![input]).unwrap();

    assert_eq!(
        alternative_font_families("noto sans mono"),
        vec![
            "Noto Sans Mono".to_string(),
            "Noto Sans Mono CJK SC".to_string(),
            "Sarasa Gothic CL".to_string(),
        ]
    );
    assert_eq!(
        alternative_font_families("Noto Sans Mono"),
        vec![
            "Noto Sans Mono".to_string(),
            "Noto Sans Mono CJK SC".to_string(),
            "Sarasa Gothic CL".to_string(),
        ]
    );
}

#[test]
fn internal_set_alternative_font_registry_alist_returns_nil_or_value() {
    crate::test_utils::init_test_tracing();
    let result = builtin_internal_set_alternative_font_registry_alist(vec![Value::NIL]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn internal_set_alternative_font_registry_alist_downcases_values() {
    crate::test_utils::init_test_tracing();
    let input = Value::list(vec![Value::list(vec![
        Value::string("ISO10646-1"),
        Value::string("GB18030.2000-1"),
    ])]);
    let result = builtin_internal_set_alternative_font_registry_alist(vec![input]).unwrap();
    let outer = list_to_vec(&result).expect("outer list");
    let inner = list_to_vec(&outer[0]).expect("inner list");
    assert_eq!(
        inner[0].as_runtime_string_owned().as_deref(),
        Some("iso10646-1")
    );
    assert_eq!(
        inner[1].as_runtime_string_owned().as_deref(),
        Some("gb18030.2000-1")
    );
    assert_eq!(
        alternative_font_registries("ISO10646-1"),
        vec!["iso10646-1".to_string(), "gb18030.2000-1".to_string()]
    );
}

#[test]
fn internal_set_alternative_font_registry_alist_accepts_raw_unibyte_strings() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b'A',
    ]));
    let expected = crate::emacs_core::builtins::lisp_string_to_runtime_string(Value::heap_string(
        crate::heap_types::LispString::from_unibyte(vec![0xFF, b'a']),
    ));
    let input = Value::list(vec![Value::list(vec![raw])]);
    let result = builtin_internal_set_alternative_font_registry_alist(vec![input]).unwrap();
    let outer = list_to_vec(&result).expect("outer list");
    let inner = list_to_vec(&outer[0]).expect("inner list");
    // Raw unibyte [0xFF, 'A'] is downcased byte-faithfully to unibyte [0xFF, 'a'].
    assert_eq!(
        inner[0].as_lisp_string(),
        Some(&crate::heap_types::LispString::from_unibyte(vec![
            0xFF, b'a'
        ]))
    );
    assert_eq!(alternative_font_registries(&expected), vec![expected]);
}

#[test]
fn face_attribute_relative_p_wrong_arity() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_face_attribute_relative_p(vec![Value::NIL]).is_err());
}

#[test]
fn merge_face_attribute_wrong_arity() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_merge_face_attribute(vec![Value::NIL, Value::NIL]).is_err());
}

#[test]
fn color_values_case_insensitive() {
    crate::test_utils::init_test_tracing();
    let result = builtin_color_values(vec![Value::string("RED")]).unwrap();
    let rgb = list_to_vec(&result).unwrap();
    assert_eq!(rgb[0].as_int(), Some(65535));
    assert_eq!(rgb[1].as_int(), Some(0));
    assert_eq!(rgb[2].as_int(), Some(0));
}

#[test]
fn color_values_hex_lowercase() {
    crate::test_utils::init_test_tracing();
    let result = builtin_color_values(vec![Value::string("#ff8000")]).unwrap();
    let rgb = list_to_vec(&result).unwrap();
    // #ff8000 approximates to yellow in the terminal palette.
    assert_eq!(rgb[0].as_int(), Some(65535));
    assert_eq!(rgb[1].as_int(), Some(65535));
    assert_eq!(rgb[2].as_int(), Some(0));
}

#[test]
fn color_values_invalid_hex_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_color_values(vec![Value::string("#ggg")]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn color_values_from_color_spec_semantics() {
    crate::test_utils::init_test_tracing();
    let rgb_short =
        list_to_vec(&builtin_color_values_from_color_spec(vec![Value::string("#000")]).unwrap())
            .unwrap();
    assert_eq!(
        rgb_short,
        vec![Value::fixnum(0), Value::fixnum(0), Value::fixnum(0)]
    );

    let rgb_12 = list_to_vec(
        &builtin_color_values_from_color_spec(vec![Value::string("#111122223333")]).unwrap(),
    )
    .unwrap();
    assert_eq!(
        rgb_12,
        vec![
            Value::fixnum(4369),
            Value::fixnum(8738),
            Value::fixnum(13107)
        ]
    );

    assert!(
        builtin_color_values_from_color_spec(vec![Value::string("#abcd")])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_color_values_from_color_spec(vec![Value::string("bogus")])
            .unwrap()
            .is_nil()
    );

    let type_err = builtin_color_values_from_color_spec(vec![Value::fixnum(1)])
        .expect_err("color-values-from-color-spec should enforce stringp");
    match type_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn color_gray_and_supported_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    assert!(
        builtin_color_gray_p(&mut eval, vec![Value::string("#000000")])
            .unwrap()
            .is_truthy()
    );
    assert!(
        builtin_color_gray_p(&mut eval, vec![Value::string("#808080")])
            .unwrap()
            .is_truthy()
    );
    assert!(
        builtin_color_gray_p(&mut eval, vec![Value::string("#ff0000")])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_color_gray_p(&mut eval, vec![Value::string("#fff"), Value::NIL])
            .unwrap()
            .is_truthy()
    );

    let gray_color_type = builtin_color_gray_p(&mut eval, vec![Value::fixnum(1)])
        .expect_err("color-gray-p should enforce stringp");
    match gray_color_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let gray_frame_type =
        builtin_color_gray_p(&mut eval, vec![Value::string("#fff"), Value::fixnum(0)])
            .expect_err("color-gray-p should validate FRAME");
    match gray_frame_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("framep"), Value::fixnum(0)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    assert!(
        builtin_color_supported_p(vec![Value::string("#123456")])
            .unwrap()
            .is_truthy()
    );
    assert!(
        builtin_color_supported_p(vec![Value::string("#fff"), Value::NIL, Value::T])
            .unwrap()
            .is_truthy()
    );
    assert!(
        builtin_color_supported_p(vec![Value::string("bogus"), Value::NIL, Value::NIL])
            .unwrap()
            .is_nil()
    );

    let supported_type = builtin_color_supported_p(vec![Value::fixnum(1)])
        .expect_err("color-supported-p should enforce stringp");
    match supported_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let supported_frame_type =
        builtin_color_supported_p(vec![Value::string("#fff"), Value::fixnum(1)])
            .expect_err("color-supported-p should validate FRAME");
    match supported_frame_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("framep"), Value::fixnum(1)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn color_distance_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let black_white = builtin_color_distance(
        &mut eval,
        vec![Value::string("#000"), Value::string("#fff")],
    )
    .expect("color-distance should evaluate");
    match black_white.kind() {
        ValueKind::Fixnum(n) => assert!(n > 0),
        other => panic!("expected integer distance, got {other:?}"),
    }

    assert_eq!(
        builtin_color_distance(
            &mut eval,
            vec![Value::string("#000"), Value::string("#000")]
        )
        .unwrap(),
        Value::fixnum(0)
    );

    // Both colors collapse to black in tty-approx mode.
    assert_eq!(
        builtin_color_distance(
            &mut eval,
            vec![Value::string("#000"), Value::string("#111")]
        )
        .unwrap(),
        Value::fixnum(0)
    );
}

#[test]
fn xw_color_primitives_follow_live_gui_frame_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.window_system = Some(Value::symbol("neo"));
    }

    assert_eq!(
        builtin_xw_color_defined_p_ctx(
            &eval,
            vec![Value::string("#123456"), Value::make_frame(frame_id.0)],
        )
        .expect("xw-color-defined-p should evaluate"),
        Value::T
    );
    assert_eq!(
        builtin_xw_color_values_ctx(
            &eval,
            vec![Value::string("#123456"), Value::make_frame(frame_id.0)],
        )
        .expect("xw-color-values should evaluate"),
        Value::list(vec![
            Value::fixnum(0x12 * 257),
            Value::fixnum(0x34 * 257),
            Value::fixnum(0x56 * 257),
        ])
    );
    assert_eq!(
        crate::emacs_core::builtins::symbols::builtin_xw_display_color_p_ctx(
            &eval,
            vec![Value::make_frame(frame_id.0)],
        )
        .expect("xw-display-color-p should evaluate"),
        Value::T
    );
}

#[test]
fn xw_color_values_follow_gnu_x11_rgb_database() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("selected frame")
        .window_system = Some(Value::symbol("neo"));

    for (name, component) in [("gray", 190 * 257), ("DarkGray", 169 * 257)] {
        assert_eq!(
            builtin_xw_color_values_ctx(
                &eval,
                vec![Value::string(name), Value::make_frame(frame_id.0)],
            )
            .expect("xw-color-values should evaluate"),
            Value::list(vec![
                Value::fixnum(component),
                Value::fixnum(component),
                Value::fixnum(component),
            ]),
            "{name}",
        );
    }
}

#[test]
fn color_distance_errors_match_oracle_shape() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let invalid_left =
        builtin_color_distance(&mut eval, vec![Value::string("#00"), Value::string("#fff")])
            .unwrap_err();
    match invalid_left {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Invalid color"), Value::string("#00")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let invalid_type =
        builtin_color_distance(&mut eval, vec![Value::fixnum(1), Value::string("#fff")])
            .expect_err("color-distance should signal invalid color for non-string args");
    match invalid_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Invalid color"), Value::fixnum(1)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let frame_err = builtin_color_distance(
        &mut eval,
        vec![Value::string("#000"), Value::string("#fff"), Value::T],
    )
    .expect_err("color-distance should validate optional FRAME");
    match frame_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("frame-live-p"), Value::T]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn color_values_dark_gray_approximates_to_white() {
    crate::test_utils::init_test_tracing();
    let result = builtin_color_values(vec![Value::string("DarkGray")]).unwrap();
    let rgb = list_to_vec(&result).unwrap();
    assert_eq!(rgb[0].as_int(), Some(65535));
    assert_eq!(rgb[1].as_int(), Some(65535));
    assert_eq!(rgb[2].as_int(), Some(65535));
}

#[test]
fn color_distance_accepts_the_terminal_default_sentinels() {
    // GNU's `tty_defined_color' (src/xfaces.c:1143-1174) seeds its Emacs_Color
    // with pixel = FACE_TTY_DEFAULT_COLOR and RGB (0, 0, 0) (:1150-1153), runs
    // `tty_lookup_color', and THEN -- only if the lookup left the pixel
    // unresolved -- maps the two sentinel names `face-background' returns on a
    // TTY frame to the terminal's default fg/bg pixels (:1160-1167).  Setting
    // the pixel is what makes the lookup succeed (:1170-1171); the RGB triple
    // stays at its zero defaults, which is why GNU measures both sentinels as
    // black.  `color-values' and `color-defined-p' still answer nil for them in
    // both editors, because those are Lisp and consult `tty-color-alist'; the
    // sentinel branch exists only in the C defined_color_hook, so
    // `color-distance' is the function that sees it.
    //
    // Expectations measured under GNU Emacs 31.0.90, not derived
    // (tmp/coord-colordist-probe.el).
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    for sentinel in ["unspecified-fg", "unspecified-bg"] {
        assert_eq!(
            builtin_color_distance(
                &mut eval,
                vec![Value::string("black"), Value::string(sentinel)],
            )
            .unwrap_or_else(|e| panic!("{sentinel} must resolve, got {e:?}")),
            Value::fixnum(0),
            "{sentinel} resolves to the zero RGB defaults, so it measures as black"
        );
    }

    // Both sentinels are the same colour value, so the distance between them
    // is zero even though they name opposite ends of the frame.
    assert_eq!(
        builtin_color_distance(
            &mut eval,
            vec![
                Value::string("unspecified-fg"),
                Value::string("unspecified-bg"),
            ],
        )
        .expect("both sentinels resolve"),
        Value::fixnum(0)
    );

    // A genuinely unknown name still signals in GNU, so the sentinel branch
    // must not turn into lenient parsing.
    match builtin_color_distance(
        &mut eval,
        vec![Value::string("black"), Value::string("not-a-color")],
    )
    .expect_err("an unknown colour name still signals")
    {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Invalid color"), Value::string("not-a-color")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}
