use super::*;
use crate::neovm_bridge::LayoutBufferSnapshot;
use neovm_core::buffer::CharPos0;
use neovm_core::emacs_core::Context;
use neovm_core::face::{Color as NeoColor, Face as NeoFace, FaceTable};

#[cfg(test)]
#[path = "compiled_collection_mutation_test.rs"]
mod compiled_collection_mutation;

fn test_buffer_snapshot() -> LayoutBufferSnapshot {
    let mut context = Context::new();
    let buf_id = context
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = context
            .buffer_manager_mut()
            .get_mut(buf_id)
            .expect("current buffer");
        buffer.insert("abc");
        buffer.widen();
    }
    let buffer = context
        .buffer_manager()
        .get(buf_id)
        .expect("current buffer");
    LayoutBufferSnapshot::from_buffer(buffer)
}

fn test_face_resolver(table: &FaceTable) -> FaceResolver {
    FaceResolver::new(table, 0x00ffffff, 0x000000, 14.0, None)
}

fn face_id(face: RenderFaceRef) -> FaceId {
    match face {
        RenderFaceRef::FaceId(face_id) => face_id,
        RenderFaceRef::Inherit => panic!("expected concrete face id"),
    }
}

fn dashboard_like_face_table() -> FaceTable {
    let mut table = FaceTable::new();

    let mut blue_title = NeoFace::new("dashboard-title-blue");
    blue_title.foreground = Some(NeoColor::rgb(0x51, 0xaf, 0xef));
    table.define("dashboard-title-blue", blue_title);

    let mut purple_title = NeoFace::new("dashboard-title-purple");
    purple_title.foreground = Some(NeoColor::rgb(0xa9, 0xa1, 0xe1));
    table.define("dashboard-title-purple", purple_title);

    let mut hl_line = NeoFace::new("dashboard-hl-line");
    hl_line.background = Some(NeoColor::rgb(0x21, 0x24, 0x2b));
    hl_line.extend = Some(true);
    table.define("dashboard-hl-line", hl_line);

    table
}

#[test]
fn repeated_source_resolution_reuses_unchanged_base_face_storage() {
    use crate::buffer_source::text_source::BufferTextSourceCursor;

    let snapshot = test_buffer_snapshot();
    let table = dashboard_like_face_table();
    let face_resolver = test_face_resolver(&table);
    let mut base = face_resolver.default_face().clone();
    base.font_family = "source face family".into();
    let base = &base;
    let mut state = DisplaySourceResolveState::default();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let params = DisplaySourceResolveParams::new(
        DisplaySourceFaceBasis::new(
            &face_resolver,
            FaceId::new(0),
            base,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        ),
        None,
        ImageScaleEnvironment::default(),
    );
    let end = snapshot.layout_point_max_char_pos();
    let mut source = BufferTextSourceCursor::new(
        neovm_core::buffer::BufferId(1),
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::FaceId(FaceId::new(0)),
    );
    // A declined batched run is consumed character by character during wrap.
    source.set_char_granularity_end(Some(end));
    let mut storage = None;
    for _ in 0..end.get() {
        let resolved = resolve_next_display_source_item(
            &mut source,
            DisplaySourceFaceScope::FrameLocal,
            params,
            &mut state,
            &mut face_ids,
        );
        assert!(resolved.item().is_some());
        let retained = state.resolved_face(FaceId::new(0)).unwrap();
        assert_eq!(retained, base);
        let current = retained.font_family.as_ptr();
        if let Some(previous) = storage {
            assert_eq!(
                current, previous,
                "unchanged base face must not allocate a new family for each source item"
            );
        }
        storage = Some(current);
    }

    // Reusing the ID does not justify keeping stale attributes.
    let mut changed = base.clone();
    changed.font_family = "changed family".into();
    changed.font_size += 3.0;
    changed.fg ^= 0x00ff00;
    state.remember_face(FaceId::new(0), &changed);
    assert_eq!(state.resolved_face(FaceId::new(0)), Some(&changed));
}

#[test]
fn source_face_resolver_merges_overlay_face_over_current_base_face() {
    let table = dashboard_like_face_table();
    let face_resolver = test_face_resolver(&table);
    let base_face = face_resolver.default_face();
    let mut resolve_state = DisplaySourceResolveState::default();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut pending_faces = Vec::new();
    let params = DisplaySourceResolveParams::new(
        DisplaySourceFaceBasis::new(
            &face_resolver,
            FaceId::new(0),
            base_face,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        ),
        None,
        ImageScaleEnvironment::default(),
    );

    let highlighted_id = {
        let mut resolver = DisplaySourcePropertyResolver::frame_local(
            params,
            &mut resolve_state,
            &mut face_ids,
            &mut pending_faces,
        );
        let title = DisplayItemFaceResolver::resolve_face_ref(
            &mut resolver,
            RenderFaceRef::Inherit,
            Value::symbol("dashboard-title-blue"),
        );
        let highlighted = DisplayItemFaceResolver::resolve_face_ref(
            &mut resolver,
            title,
            Value::symbol("dashboard-hl-line"),
        );
        face_id(highlighted)
    };

    let highlighted = resolve_state
        .resolved_face(highlighted_id)
        .expect("highlighted face");
    assert_eq!(highlighted.fg, 0x0051afef);
    assert_eq!(highlighted.bg, 0x0021242b);
    assert!(highlighted.extend);
}

#[test]
fn source_face_cache_is_keyed_by_base_face_id() {
    let table = dashboard_like_face_table();
    let face_resolver = test_face_resolver(&table);
    let base_face = face_resolver.default_face();
    let mut resolve_state = DisplaySourceResolveState::default();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut pending_faces = Vec::new();
    let params = DisplaySourceResolveParams::new(
        DisplaySourceFaceBasis::new(
            &face_resolver,
            FaceId::new(0),
            base_face,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        ),
        None,
        ImageScaleEnvironment::default(),
    );

    let (blue_hl_id, purple_hl_id) = {
        let mut resolver = DisplaySourcePropertyResolver::frame_local(
            params,
            &mut resolve_state,
            &mut face_ids,
            &mut pending_faces,
        );
        let blue = DisplayItemFaceResolver::resolve_face_ref(
            &mut resolver,
            RenderFaceRef::Inherit,
            Value::symbol("dashboard-title-blue"),
        );
        let blue_hl = DisplayItemFaceResolver::resolve_face_ref(
            &mut resolver,
            blue,
            Value::symbol("dashboard-hl-line"),
        );
        let purple = DisplayItemFaceResolver::resolve_face_ref(
            &mut resolver,
            RenderFaceRef::Inherit,
            Value::symbol("dashboard-title-purple"),
        );
        let purple_hl = DisplayItemFaceResolver::resolve_face_ref(
            &mut resolver,
            purple,
            Value::symbol("dashboard-hl-line"),
        );
        (face_id(blue_hl), face_id(purple_hl))
    };

    assert_ne!(blue_hl_id, purple_hl_id);
    assert_eq!(
        resolve_state
            .resolved_face(blue_hl_id)
            .expect("blue highlight")
            .fg,
        0x0051afef
    );
    assert_eq!(
        resolve_state
            .resolved_face(purple_hl_id)
            .expect("purple highlight")
            .fg,
        0x00a9a1e1
    );
}

#[test]
fn buffer_source_face_resolver_uses_buffer_face_remapping() {
    let mut context = Context::new();
    let buf_id = context
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let remapping = Value::list(vec![Value::list(vec![
        Value::symbol("dashboard-hl-line"),
        Value::list(vec![
            Value::keyword("background"),
            Value::string("#282c34"),
            Value::keyword("extend"),
            Value::T,
        ]),
        Value::symbol("dashboard-hl-line"),
    ])]);
    {
        let buffer = context
            .buffer_manager_mut()
            .get_mut(buf_id)
            .expect("current buffer");
        buffer.insert("abc");
        buffer.widen();
        buffer.set_buffer_local("face-remapping-alist", remapping);
    }
    let table = dashboard_like_face_table();
    let face_resolver = test_face_resolver(&table);
    let buffer = context
        .buffer_manager()
        .get(buf_id)
        .expect("current buffer");
    let base_face = face_resolver.default_face();
    let mut resolve_state = DisplaySourceResolveState::default();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut pending_faces = Vec::new();
    let params = DisplaySourceResolveParams::new(
        DisplaySourceFaceBasis::new(
            &face_resolver,
            FaceId::new(0),
            base_face,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        ),
        None,
        ImageScaleEnvironment::default(),
    );

    let highlighted_id = {
        let mut resolver = DisplaySourcePropertyResolver::buffer_local(
            buffer,
            params,
            &mut resolve_state,
            &mut face_ids,
            &mut pending_faces,
        );
        let title = DisplayItemFaceResolver::resolve_face_ref(
            &mut resolver,
            RenderFaceRef::Inherit,
            Value::symbol("dashboard-title-blue"),
        );
        let highlighted = DisplayItemFaceResolver::resolve_face_ref(
            &mut resolver,
            title,
            Value::symbol("dashboard-hl-line"),
        );
        face_id(highlighted)
    };

    let highlighted = resolve_state
        .resolved_face(highlighted_id)
        .expect("highlighted face");
    assert_eq!(highlighted.fg, 0x0051afef);
    assert_eq!(highlighted.bg, 0x00282c34);
    assert!(highlighted.extend);
}

#[test]
fn named_face_background_equal_to_global_default_still_overrides_buffer_default() {
    let mut context = Context::new();
    let buf_id = context
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let remapping = Value::list(vec![Value::list(vec![
        Value::symbol("default"),
        Value::list(vec![Value::keyword("background"), Value::string("#21242b")]),
        Value::symbol("default"),
    ])]);
    {
        let buffer = context
            .buffer_manager_mut()
            .get_mut(buf_id)
            .expect("current buffer");
        buffer.insert("abc");
        buffer.widen();
        buffer.set_buffer_local("face-remapping-alist", remapping);
    }

    let mut table = FaceTable::new();
    let mut default = NeoFace::new("default");
    default.background = Some(NeoColor::rgb(0x28, 0x2c, 0x34));
    table.define("default", default);
    let mut selected_line = NeoFace::new("selected-line");
    selected_line.background = Some(NeoColor::rgb(0x28, 0x2c, 0x34));
    selected_line.extend = Some(true);
    table.define("selected-line", selected_line);

    let face_resolver = test_face_resolver(&table);
    let buffer = context
        .buffer_manager()
        .get(buf_id)
        .expect("current buffer");
    let buffer_default = face_resolver.resolve_buffer_default_face(buffer);
    assert_eq!(buffer_default.bg, 0x0021242b);

    let highlighted = face_resolver
        .resolve_buffer_face_value_over(buffer, &buffer_default, &Value::symbol("selected-line"))
        .expect("selected face should resolve");

    assert_eq!(highlighted.bg, 0x00282c34);
    assert!(highlighted.extend);
}

#[test]
fn display_string_base_face_reuses_active_face_before_prefix_policy() {
    let buffer = test_buffer_snapshot();
    let table = FaceTable::new();
    let resolver = test_face_resolver(&table);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(BasicFaceId::SENTINEL);

    let base_face = resolve_display_string_base_face(
        &buffer,
        &resolver,
        DisplayOrigin::LinePrefix {
            anchor_charpos: CharPos0::new(0),
        },
        BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::Default),
        Some(ActiveDisplayStringBaseFace::new(
            FaceId::new(500),
            resolver.default_face(),
        )),
        DisplayDefaultFaceInstallPolicy::InstallDefaultFace,
        &mut face_ids,
    );

    assert_eq!(base_face.face_id(), FaceId::new(500));
    assert!(base_face.pending_face().is_none());
    assert!(same_resolved_face(
        base_face.face(),
        resolver.default_face()
    ));
}

#[test]
fn display_string_unremapped_default_controls_pending_face() {
    let buffer = test_buffer_snapshot();
    let table = FaceTable::new();
    let resolver = test_face_resolver(&table);
    let mut install_face_ids = FrameFaceAttempt::for_test_with_next_id(BasicFaceId::SENTINEL);
    let mut reuse_face_ids = FrameFaceAttempt::for_test_with_next_id(BasicFaceId::SENTINEL);

    let installed = resolve_display_string_base_face(
        &buffer,
        &resolver,
        DisplayOrigin::LinePrefix {
            anchor_charpos: CharPos0::new(0),
        },
        BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::Default),
        None,
        DisplayDefaultFaceInstallPolicy::InstallDefaultFace,
        &mut install_face_ids,
    );
    let reused = resolve_display_string_base_face(
        &buffer,
        &resolver,
        DisplayOrigin::LinePrefix {
            anchor_charpos: CharPos0::new(0),
        },
        BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::Default),
        None,
        DisplayDefaultFaceInstallPolicy::ReuseInstalledDefaultFace,
        &mut reuse_face_ids,
    );

    assert_eq!(installed.face_id(), FaceId::from(BasicFaceId::Default));
    assert!(installed.pending_face().is_some());
    assert_eq!(reused.face_id(), FaceId::from(BasicFaceId::Default));
    assert!(reused.pending_face().is_none());
}

#[test]
fn display_string_base_face_allocates_pending_face_for_dynamic_source_face() {
    let buffer = test_buffer_snapshot();
    let table = FaceTable::new();
    let resolver = test_face_resolver(&table);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(500);

    let base_face = resolve_display_string_base_face(
        &buffer,
        &resolver,
        DisplayOrigin::ModeLine { selected: true },
        BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::ModeLineActive),
        None,
        DisplayDefaultFaceInstallPolicy::ReuseInstalledDefaultFace,
        &mut face_ids,
    );

    assert_eq!(base_face.face_id(), FaceId::new(500));
    let pending_face = base_face.pending_face().expect("pending face");
    assert_eq!(pending_face.face_id(), FaceId::new(500));
    assert!(same_resolved_face(
        pending_face.resolved(),
        base_face.face()
    ));
    assert_eq!(face_ids.next_face_id_for_test(), 501);
}

#[test]
fn resolve_display_replacement_returns_direct_xwidget_media() {
    let table = FaceTable::new();
    let resolver = test_face_resolver(&table);
    let xwidget = DisplayXwidgetItem {
        xwidget_id: neomacs_display_protocol::XwidgetId::new(42),
        webview_id: neomacs_display_protocol::WebViewId::new(420),
        width: 120.0,
        height: 36.0,
    };
    let media = DisplayMediaReplacement::xwidget(xwidget);

    let resolved = resolve_display_replacement(
        Value::NIL,
        &DisplayMediaReplacementProperty::Xwidget(media),
        None,
        resolver.default_face(),
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        ImageScaleEnvironment::default(),
        None,
    );

    assert_eq!(resolved, Some(ResolvedDisplayReplacement::Media(media)));
}

#[test]
fn resolve_display_replacement_uses_media_placeholder_without_host() {
    let table = FaceTable::new();
    let resolver = test_face_resolver(&table);

    let resolved = resolve_display_replacement(
        Value::NIL,
        &DisplayMediaReplacementProperty::Image,
        None,
        resolver.default_face(),
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        ImageScaleEnvironment::default(),
        None,
    );

    assert_eq!(
        resolved,
        Some(ResolvedDisplayReplacement::Placeholder("[img]"))
    );
}

#[test]
fn display_media_face_metrics_prefer_active_face_extents() {
    let table = FaceTable::new();
    let resolver = test_face_resolver(&table);
    let mut active_face = resolver.default_face().clone();
    active_face.set_measured_char_width_px(11.0);
    active_face.font_line_height = 24.0;
    active_face.font_ascent = 20.0;
    let fallback = DisplayRowFallbackMetrics::from_default_face_extents(8.0, 18.0, 14.0);

    let metrics = display_media_face_metrics(&active_face, fallback);

    assert_eq!(metrics.row_height(), 24.0);
    assert_eq!(metrics.ascent(), 20.0);
    assert_eq!(metrics.char_width(), 11.0);
}

#[test]
fn single_source_face_resolution_reuses_its_realization() {
    let table = dashboard_like_face_table();
    let face_resolver = test_face_resolver(&table);
    let mut state = DisplaySourceResolveState::default();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut pending = Vec::new();
    let params = DisplaySourceResolveParams::new(
        DisplaySourceFaceBasis::new(
            &face_resolver,
            FaceId::new(0),
            face_resolver.default_face(),
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        ),
        None,
        ImageScaleEnvironment::default(),
    );
    let source = OrderedFaceSources::from_text_and_overlays(
        Some(Value::symbol("dashboard-title-blue")),
        Vec::new(),
    );
    let mut resolver =
        DisplaySourcePropertyResolver::frame_local(params, &mut state, &mut face_ids, &mut pending);
    let first = resolver.resolve_face_sources(RenderFaceRef::Inherit, &source);
    for _ in 0..512 {
        assert_eq!(
            resolver.resolve_face_sources(RenderFaceRef::Inherit, &source),
            first
        );
    }
    assert_eq!(
        pending.len(),
        1,
        "repeated source lookups must not rebuild and republish the same realized face"
    );
    assert_eq!(state.resolved_face(face_id(first)).unwrap().fg, 0x0051afef);
}

#[test]
fn source_face_observations_borrow_the_existing_realization() {
    let table = dashboard_like_face_table();
    let face_resolver = test_face_resolver(&table);
    let base = face_resolver.default_face();
    let mut state = DisplaySourceResolveState::default();
    state.remember_face(FaceId::new(3), base);
    for reference in [
        RenderFaceRef::Inherit,
        RenderFaceRef::FaceId(FaceId::new(3)),
        RenderFaceRef::FaceId(FaceId::new(99)),
    ] {
        let observed = state.resolved_face_for(reference, base);
        let borrowed: &ResolvedFace = std::borrow::Borrow::borrow(&observed);
        let expected = if reference == RenderFaceRef::FaceId(FaceId::new(3)) {
            state.resolved_face(FaceId::new(3)).unwrap()
        } else {
            base
        };
        assert!(
            std::ptr::eq(borrowed, expected),
            "read-only face observation cloned the realized face"
        );
    }
}

#[test]
fn consecutive_face_lookups_reuse_hashing_and_replay_mutable_dependencies() {
    let _context = Context::new();
    use neovm_core::tagged::collection_reads::capture;
    let table = FaceTable::new();
    let resolver = test_face_resolver(&table);
    let mut state = DisplaySourceResolveState::default();
    let base = FaceId::new(0);
    let result = FaceId::new(20);
    let value = Value::list(vec![Value::symbol(":height"), Value::fixnum(120)]);
    state.cache_face(base, value, result, resolver.default_face());
    for _ in 0..512 {
        assert_eq!(
            state.cached_face(base, &value),
            Some(RenderFaceRef::FaceId(result))
        );
    }
    assert_eq!(
        state.structural_face_lookups, 1,
        "one structural lookup per unchanged face run"
    );
    let (_, reads) = capture(|| state.cached_face(base, &value));
    let reads = reads.expect("lookup dependencies");
    value.cons_cdr().set_car(Value::fixnum(180));
    assert!(
        !reads.unchanged(),
        "fast hits must expose nested dependencies"
    );
    let lookups = state.structural_face_lookups;
    state.cached_face(base, &value);
    assert_eq!(
        state.structural_face_lookups,
        lookups + 1,
        "mutation must retry structural lookup"
    );
}

#[test]
fn consecutive_face_lookup_preserves_structural_equality_and_base_identity() {
    let _context = Context::new();
    let table = FaceTable::new();
    let resolver = test_face_resolver(&table);
    let mut state = DisplaySourceResolveState::default();
    let base = FaceId::new(0);
    let result = FaceId::new(20);
    let value = Value::list(vec![Value::symbol(":height"), Value::fixnum(120)]);
    let equal = Value::list(vec![Value::symbol(":height"), Value::fixnum(120)]);
    state.cache_face(base, value, result, resolver.default_face());
    assert_eq!(
        state.cached_face(base, &value),
        Some(RenderFaceRef::FaceId(result))
    );
    assert_eq!(
        state.cached_face(base, &equal),
        Some(RenderFaceRef::FaceId(result))
    );
    assert_eq!(state.cached_face(FaceId::new(1), &value), None);
    state.cache_face(base, value, FaceId::new(21), resolver.default_face());
    assert_eq!(
        state.cached_face(base, &value),
        Some(RenderFaceRef::FaceId(FaceId::new(21)))
    );
}
