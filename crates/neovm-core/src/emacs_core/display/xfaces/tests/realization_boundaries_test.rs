use super::*;

#[test]
fn default_slot_projection_observes_same_generation_winners_and_refuses_stale_cache() {
    let mut context = crate::emacs_core::Context::new();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut context);
    let attributes = Face::default();
    let generation = context.face_change_count;
    let cache = &mut context.frames.get_mut(frame).unwrap().face_realization;
    cache.prepare(generation, crate::face::FaceTable::new());
    cache.insert_basic(BasicFace::Default, attributes.clone(), attributes.clone());
    assert_eq!(
        context.cached_display_face_slot_for_frame(frame, &attributes),
        crate::face::realization::CachedFaceSlot::Default
    );
    context
        .frames
        .get_mut(frame)
        .unwrap()
        .face_realization
        .insert_basic(BasicFace::Margin, attributes.clone(), attributes.clone());
    assert_eq!(
        context.cached_display_face_slot_for_frame(frame, &attributes),
        crate::face::realization::CachedFaceSlot::NonDefault
    );
    assert_eq!(context.face_change_count, generation);
    context.face_change_count += 1;
    assert_eq!(
        context.cached_display_face_slot_for_frame(frame, &attributes),
        crate::face::realization::CachedFaceSlot::Unavailable
    );
}

#[test]
fn definition_root_lease_is_reused_until_source_retirement_and_keeps_old_sources_alive() {
    let mut context = crate::emacs_core::Context::new();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut context);
    let mut old = crate::face::FaceTable::new();
    let mut unused = Face::new("not-yet-displayed");
    unused.family = Some(Value::string("retained-unused-family"));
    unused.stipple = Some(Value::list(vec![Value::string("retained-unused-stipple")]));
    old.define("not-yet-displayed", unused);
    let initial = context.face_change_count;
    context
        .frames
        .get_mut(frame)
        .expect("selected frame")
        .face_realization
        .prepare(initial, old.clone());
    let retained = context
        .display_face_table_root_lease(frame, &old)
        .expect("installed current mutator");
    let warm = context
        .display_face_table_root_lease(frame, &old)
        .expect("warm current source");
    assert!(std::sync::Arc::ptr_eq(&retained, &warm));
    drop(warm);

    context
        .frames
        .get_mut(frame)
        .unwrap()
        .face_realization
        .preserve_across_change(initial, initial + 1);
    context.face_change_count = initial + 1;
    let retagged = context
        .display_face_table_root_lease(frame, &old)
        .expect("unaffected frame keeps its source lease");
    assert!(std::sync::Arc::ptr_eq(&retained, &retagged));
    drop(retagged);

    let cache = &mut context.frames.get_mut(frame).unwrap().face_realization;
    cache.definitions_changed(initial + 1, initial + 2, None);
    assert!(cache.root_lease().is_none());
    cache.replace_definitions(crate::face::FaceTable::new());
    context.face_change_count = initial + 2;
    let replacement = context
        .frames
        .get(frame)
        .unwrap()
        .face_realization
        .table(initial + 2)
        .unwrap()
        .clone();
    let replaced = context
        .display_face_table_root_lease(frame, &replacement)
        .expect("replacement snapshot gets its own lease");
    assert!(!std::sync::Arc::ptr_eq(&retained, &replaced));
    context
        .frames
        .get_mut(frame)
        .unwrap()
        .face_realization
        .invalidate();
    assert!(
        context
            .frames
            .get(frame)
            .unwrap()
            .face_realization
            .root_lease()
            .is_none()
    );

    // The retired layout snapshot is no longer owned by a runtime cache.
    // Its retained lease alone must keep even unused Lisp attributes alive.
    for _ in 0..3 {
        context.gc_collect_exact();
        let source = old.resolve("not-yet-displayed");
        assert_eq!(
            source.family_runtime_string_owned().as_deref(),
            Some("retained-unused-family")
        );
        assert_eq!(
            source.stipple.unwrap().cons_car().as_utf8_str(),
            Some("retained-unused-stipple")
        );
    }
    drop(retained);
}

#[test]
fn face_change_walks_only_frames_whose_realizations_stay_valid() {
    let mut context = crate::emacs_core::Context::new();
    let selected = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut context);
    let buffer = context.buffers.create_buffer("*face-change-other*");
    let other = context.frames.create_frame("other", 80, 25, buffer);

    assert!(!super::other_frames_keep_realizations(
        &context.frames,
        FaceChangeScope::AllFrames,
    ));
    assert!(!super::other_frames_keep_realizations(
        &context.frames,
        FaceChangeScope::Frames(&[selected, other]),
    ));
    assert!(super::other_frames_keep_realizations(
        &context.frames,
        FaceChangeScope::Frames(&[selected]),
    ));
    assert!(super::other_frames_keep_realizations(
        &context.frames,
        FaceChangeScope::Frames(&[]),
    ));
    assert!(super::other_frames_keep_realizations(
        &context.frames,
        FaceChangeScope::DefinitionsOnly(&[selected]),
    ));
}

#[test]
fn default_realization_binding_restores_after_lisp_error() {
    let mut context = super::super::super::eval::Context::new();
    context.set_variable("inhibit-redisplay", Value::NIL);
    let mut scope = RealizationBinding::enter(&mut context, "inhibit-redisplay", Value::T)
        .expect("ordinary special variable accepts its binding");
    assert_eq!(
        scope.context().variable_value_by_name("inhibit-redisplay"),
        Some(Value::T)
    );
    let failure: Result<(), Flow> = Err(crate::emacs_core::error::signal(
        "error",
        vec![Value::string("realization advice failure")],
    ));
    assert!(scope.finish(failure).is_err());
    assert_eq!(
        context.variable_value_by_name("inhibit-redisplay"),
        Some(Value::NIL)
    );
}

#[test]
fn default_realization_binding_drop_restores_native_state() {
    let mut context = super::super::super::eval::Context::new();
    context.set_variable("inhibit-redisplay", Value::fixnum(7));
    {
        let _scope = RealizationBinding::enter(&mut context, "inhibit-redisplay", Value::T)
            .expect("ordinary special variable accepts its binding");
    }
    assert_eq!(
        context.variable_value_by_name("inhibit-redisplay"),
        Some(Value::fixnum(7))
    );
}

#[test]
fn foreground_underline_symbol_does_not_become_a_color_name() {
    let attribute = Value::list(vec![
        Value::keyword("color"),
        Value::string("red"),
        Value::keyword("color"),
        Value::symbol("foreground-color"),
    ]);
    let converted = super::super::lisp_value_to_face_attr_resolved(
        LFaceAttr::Underline,
        attribute,
        FaceColorResolver::Standard,
    );
    let Some(crate::face::FaceAttrValue::SpecifiedUnderline { underline, color }) = converted
    else {
        panic!("a color plist remains an explicitly enabled underline");
    };
    assert_eq!(color, None);
    assert_eq!(underline.color, None);

    let attribute = Value::list(vec![
        Value::keyword("color"),
        Value::string("foreground-color"),
    ]);
    let converted = super::super::lisp_value_to_face_attr_resolved(
        LFaceAttr::Underline,
        attribute,
        FaceColorResolver::Standard,
    );
    let Some(crate::face::FaceAttrValue::SpecifiedUnderline { color, .. }) = converted else {
        panic!("a string color remains an explicitly enabled underline");
    };
    assert_eq!(
        color,
        Some(SpecifiedColor::Named("foreground-color".to_owned()))
    );
}

#[test]
fn initial_terminal_faces_hide_creation_colors_and_restore_owners() {
    let mut context = super::super::super::eval::Context::new();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut context);
    let foreground = Value::string("#123456");
    let background = Value::string("#654321");
    let definitions = {
        let owner = context.frames.get_mut(frame).expect("selected frame");
        owner.set_known_parameter(FrameParam::ForegroundColor, foreground);
        owner.set_known_parameter(FrameParam::BackgroundColor, background);
        owner.face_hash_table
    };
    {
        let initial = InitialTerminalFaces::new(&mut context, frame).expect("live frame");
        let owner = initial.context.frames.get(frame).expect("live frame");
        assert_eq!(
            owner.known_parameter(FrameParam::ForegroundColor),
            Some(Value::string("unspecified-fg"))
        );
        assert_eq!(
            owner.known_parameter(FrameParam::BackgroundColor),
            Some(Value::string("unspecified-bg"))
        );
        assert_ne!(owner.face_hash_table, definitions);
    }
    let owner = context.frames.get(frame).expect("live frame");
    assert_eq!(
        owner.known_parameter(FrameParam::ForegroundColor),
        Some(foreground)
    );
    assert_eq!(
        owner.known_parameter(FrameParam::BackgroundColor),
        Some(background)
    );
    assert_eq!(owner.face_hash_table, definitions);
}

#[test]
fn creation_color_replay_defers_the_other_color_and_initial_error_restores_parameters() {
    let mut context = crate::emacs_core::Context::new();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut context);
    let foreground = Value::string("#123456");
    let background = Value::string("#654321");
    let foreground_pair = Value::cons(FrameParam::ForegroundColor.symbol(), foreground);
    let background_pair = Value::cons(FrameParam::BackgroundColor.symbol(), background);
    let _roots = RealizationRoots::from_values([foreground_pair, background_pair]);
    {
        let owner = context.frames.get_mut(frame).unwrap();
        owner.set_known_parameter(FrameParam::ForegroundColor, foreground);
        owner.set_known_parameter(FrameParam::BackgroundColor, background);
    }
    let failed_initialization: Result<(), Flow> = (|| {
        let initial = InitialTerminalFaces::new(&mut context, frame).unwrap();
        assert_eq!(
            initial
                .context
                .frames
                .get(frame)
                .unwrap()
                .known_parameter(FrameParam::ForegroundColor),
            Some(Value::string("unspecified-fg"))
        );
        Err(crate::emacs_core::error::signal(
            "error",
            vec![Value::string("initial face callback failed")],
        ))
    })();
    assert!(failed_initialization.is_err());
    assert_eq!(
        context
            .frames
            .get(frame)
            .unwrap()
            .known_parameter(FrameParam::ForegroundColor),
        Some(foreground)
    );
    assert_eq!(
        context
            .frames
            .get(frame)
            .unwrap()
            .known_parameter(FrameParam::BackgroundColor),
        Some(background)
    );

    defer_creation_color_parameters(&mut context, frame, &[foreground_pair, background_pair]);
    assert_eq!(
        context
            .frames
            .get(frame)
            .unwrap()
            .known_parameter(FrameParam::ForegroundColor),
        Some(Value::string("unspecified-fg"))
    );
    assert_eq!(
        context
            .frames
            .get(frame)
            .unwrap()
            .known_parameter(FrameParam::BackgroundColor),
        Some(Value::string("unspecified-bg"))
    );
    super::super::super::frame::builtin_modify_frame_parameters(
        &mut context,
        vec![
            Value::make_frame(frame.0),
            Value::list(vec![background_pair]),
        ],
    )
    .expect("first reverse-order color handler");
    assert_eq!(
        context
            .frames
            .get(frame)
            .unwrap()
            .known_parameter(FrameParam::ForegroundColor),
        Some(Value::string("unspecified-fg"))
    );
    assert_eq!(
        context
            .frames
            .get(frame)
            .unwrap()
            .known_parameter(FrameParam::BackgroundColor),
        Some(background)
    );
    let vector = lookup_frame_lisp_face_vector(&context, frame, "default").unwrap();
    assert_eq!(
        lisp_face_vector_attr(vector, LFaceAttr::Foreground),
        Some(Value::string("unspecified-fg"))
    );
    super::super::super::frame::builtin_modify_frame_parameters(
        &mut context,
        vec![
            Value::make_frame(frame.0),
            Value::list(vec![foreground_pair]),
        ],
    )
    .expect("second reverse-order color handler");
    assert_eq!(
        context
            .frames
            .get(frame)
            .unwrap()
            .known_parameter(FrameParam::ForegroundColor),
        Some(foreground)
    );
    assert_eq!(
        context
            .frames
            .get(frame)
            .unwrap()
            .known_parameter(FrameParam::BackgroundColor),
        Some(background)
    );
}

#[test]
fn named_and_anonymous_underlines_retain_identical_ordered_operations() {
    let attribute = Value::list(vec![
        Value::keyword("color"),
        Value::string("#111111"),
        Value::keyword("color"),
        Value::symbol("foreground-color"),
        Value::keyword("color"),
        Value::string("#222222"),
    ]);
    let vector = super::super::make_lisp_face_vector();
    super::super::set_lisp_face_vector_attr(vector, LFaceAttr::Underline, attribute);
    let named = super::super::runtime_face_from_lisp_face_vector("ordered", vector);
    let anonymous = Face::from_plist("ordered", &[Value::keyword("underline"), attribute]);
    assert_eq!(named.color_specifications, anonymous.color_specifications);
    assert_eq!(named.underline, anonymous.underline);
    assert_eq!(
        named
            .color_specifications
            .underline_operations
            .as_deref()
            .map(|operations| operations.len()),
        Some(3)
    );
}

#[test]
fn face_no_inherit_property_preserves_all_realization_scopes() {
    let mut context = crate::emacs_core::eval::Context::new();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut context);
    let local = [frame];
    assert_eq!(
        named_face_change_scope(
            &context,
            "property-scope-face",
            FaceChangeScope::Frames(&local)
        ),
        FaceChangeScope::Frames(&local)
    );
    context
        .obarray_mut()
        .put_property("property-scope-face", "face-no-inherit", Value::T)
        .expect("test property is writable");
    assert_eq!(
        named_face_change_scope(
            &context,
            "property-scope-face",
            FaceChangeScope::Frames(&local)
        ),
        FaceChangeScope::DefinitionsOnly(&local)
    );
    assert_eq!(
        named_face_change_scope(&context, "property-scope-face", FaceChangeScope::AllFrames),
        FaceChangeScope::Frames(&[])
    );
    context
        .obarray_mut()
        .put_property("property-scope-face", "face-no-inherit", Value::NIL)
        .expect("test property is writable");
    assert_eq!(
        named_face_change_scope(&context, "property-scope-face", FaceChangeScope::AllFrames),
        FaceChangeScope::AllFrames
    );
}

#[test]
fn face_frame_class_uses_live_terminal_output_kind() {
    use crate::emacs_core::terminal::pure::{self, TerminalHost, TerminalRuntimeConfig};
    use crate::face::realization::{FrameFaceClass, frame_face_class};
    struct Host;
    impl TerminalHost for Host {
        fn suspend_tty(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn resume_tty(&mut self) -> Result<(), String> {
            Ok(())
        }
    }
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            pure::reset_terminal_host();
            pure::reset_terminal_runtime();
        }
    }
    let mut context = crate::emacs_core::Context::new();
    let _restore = Restore;
    pure::reset_terminal_runtime();
    let initial = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut context);
    let buffer = context.buffers.current_buffer_id().unwrap();
    let bare = context.frames.create_frame("bare-frame", 80, 25, buffer);
    assert!(!context.frames.get(bare).unwrap().initial);
    assert_eq!(
        frame_face_class(context.frames.get(initial).unwrap()),
        Some(FrameFaceClass::Initial)
    );
    assert_eq!(
        frame_face_class(context.frames.get(bare).unwrap()),
        Some(FrameFaceClass::Initial)
    );
    assert!(!context.frame_is_tty_face_target(bare));

    let config = || {
        TerminalRuntimeConfig::interactive(Some("xterm-256color".to_owned()), neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(256))
    };
    pure::configure_terminal_runtime(config());
    // Primary setup redescribes native terminal0 in place; neither the id nor
    // an old bootstrap frame.initial flag can override the installed kind.
    assert!(context.frame_is_tty_face_target(initial));
    assert!(context.frame_is_tty_face_target(bare));
    pure::set_terminal_host(Box::new(Host));
    pure::builtin_suspend_tty(&mut context, vec![Value::make_frame(bare.0)])
        .expect("suspend configured primary tty");
    assert!(context.frame_is_tty_face_target(bare));

    let secondary_id = 27;
    pure::ensure_terminal_runtime_owner(secondary_id, "secondary-tty", config());
    let secondary =
        context
            .frames
            .create_frame_on_terminal("secondary-frame", secondary_id, 80, 25, buffer);
    assert!(context.frame_is_tty_face_target(secondary));
    context
        .frames
        .get_mut(bare)
        .unwrap()
        .set_window_system(Some(Value::symbol("neo")));
    assert_eq!(
        frame_face_class(context.frames.get(bare).unwrap()),
        Some(FrameFaceClass::WindowSystem)
    );
    assert!(!context.frame_is_tty_face_target(bare));
    assert!(context.frame_is_tty_face_target(secondary));
}

#[test]
fn same_value_face_set_marks_no_definitions_dirty() {
    if !crate::face::realization::enabled() {
        return;
    }
    let mut context = crate::emacs_core::eval::Context::new();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut context);
    super::builtin_internal_set_lisp_face_attribute(
        &mut context,
        vec![
            Value::symbol("incremental-same-face"),
            Value::keyword("foreground"),
            Value::string("red"),
        ],
    )
    .expect("first set creates the face");
    prepare_runtime_faces_result(&mut context, frame).expect("first refresh publishes");
    super::builtin_internal_set_lisp_face_attribute(
        &mut context,
        vec![
            Value::symbol("incremental-same-face"),
            Value::keyword("foreground"),
            Value::string("red"),
        ],
    )
    .expect("same-value set is accepted");
    let cache = &context
        .frames
        .get(frame)
        .expect("selected frame")
        .face_realization;
    assert!(
        cache
            .reusable_definitions()
            .expect("a published table exists")
            .1
            .is_empty(),
        "GNU gates the change flag on Fequal (xfaces.c:3767-3773)"
    );
    assert!(!cache.definitions_all_dirty());
    let changed = prepare_runtime_faces_result(&mut context, frame).expect("refresh runs");
    assert!(!changed, "a clean generation cache-hits");
}

#[test]
fn changed_face_marks_only_its_name_and_refresh_matches_a_full_rebuild() {
    if !crate::face::realization::enabled() {
        return;
    }
    let mut context = crate::emacs_core::eval::Context::new();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut context);
    for (face, color) in [
        ("incremental-face-a", "red"),
        ("incremental-face-b", "blue"),
    ] {
        super::builtin_internal_set_lisp_face_attribute(
            &mut context,
            vec![
                Value::symbol(face),
                Value::keyword("foreground"),
                Value::string(color),
            ],
        )
        .expect("face definition lands");
    }
    prepare_runtime_faces_result(&mut context, frame).expect("first refresh publishes");
    super::builtin_internal_set_lisp_face_attribute(
        &mut context,
        vec![
            Value::symbol("incremental-face-b"),
            Value::keyword("foreground"),
            Value::string("green"),
        ],
    )
    .expect("genuine change is accepted");
    {
        let cache = &context
            .frames
            .get(frame)
            .expect("selected frame")
            .face_realization;
        let dirty = cache
            .reusable_definitions()
            .expect("previous table survives for reuse")
            .1;
        assert_eq!(dirty.iter().count(), 1);
        assert!(dirty.contains("incremental-face-b"));
        assert!(
            cache.table(context.face_change_count).is_none(),
            "a dirty definition cannot serve as current"
        );
    }
    prepare_runtime_faces_result(&mut context, frame).expect("incremental refresh runs");
    let incremental = context.face_table.clone();
    let full = runtime_face_table_from_frame_lisp_faces(&context, frame, false);
    for face in ["incremental-face-a", "incremental-face-b", "default"] {
        assert_eq!(
            incremental.get(face),
            full.get(face),
            "{face} must match a from-scratch derivation"
        );
    }
    let cache = &context
        .frames
        .get(frame)
        .expect("selected frame")
        .face_realization;
    assert!(
        cache
            .reusable_definitions()
            .expect("refreshed table published")
            .1
            .is_empty()
    );
    assert!(cache.table(context.face_change_count).is_some());
}

#[test]
fn unnamed_face_change_retires_every_reusable_definition() {
    if !crate::face::realization::enabled() {
        return;
    }
    let mut context = crate::emacs_core::eval::Context::new();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut context);
    super::builtin_internal_set_lisp_face_attribute(
        &mut context,
        vec![
            Value::symbol("incremental-unnamed-face"),
            Value::keyword("foreground"),
            Value::string("red"),
        ],
    )
    .expect("face definition lands");
    prepare_runtime_faces_result(&mut context, frame).expect("first refresh publishes");
    record_face_change(&mut context, FaceChangeScope::Frames(&[frame]));
    assert!(
        context
            .frames
            .get(frame)
            .expect("selected frame")
            .face_realization
            .definitions_all_dirty()
    );
    prepare_runtime_faces_result(&mut context, frame).expect("full refresh runs");
    let refreshed = context.face_table.clone();
    let full = runtime_face_table_from_frame_lisp_faces(&context, frame, false);
    assert_eq!(
        refreshed.get("incremental-unnamed-face"),
        full.get("incremental-unnamed-face")
    );
}

#[test]
fn realization_miss_roots_the_source_table_through_the_frame_lease() {
    if !crate::face::realization::enabled() {
        return;
    }
    use crate::emacs_core::terminal::pure::{self, TerminalHost, TerminalRuntimeConfig};
    struct Host;
    impl TerminalHost for Host {
        fn suspend_tty(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn resume_tty(&mut self) -> Result<(), String> {
            Ok(())
        }
    }
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            pure::reset_terminal_host();
            pure::reset_terminal_runtime();
        }
    }
    let mut context = crate::emacs_core::Context::new();
    let _restore = Restore;
    pure::reset_terminal_runtime();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut context);
    pure::configure_terminal_runtime(TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_owned()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));
    assert!(context.frame_is_tty_face_target(frame));
    let generation = context.face_change_count;
    let mut table = crate::face::FaceTable::new();
    let mut rooted = Face::new("lease-rooted-family");
    rooted.family = Some(Value::string("lease-rooted"));
    table.define("lease-rooted-family", rooted);
    context
        .frames
        .get_mut(frame)
        .expect("selected frame")
        .face_realization
        .prepare(generation, table);
    assert!(
        context
            .frames
            .get(frame)
            .unwrap()
            .face_realization
            .root_lease()
            .is_none(),
        "prepare retires the lease; the first miss must install it"
    );
    // Two distinct attribute sets force two lookup misses. Each used to
    // re-walk every definition and push it on the scratch root stack; both
    // now share the frame cache's retained batch lease instead.
    let first = context.realize_display_face_for_frame(frame, &Face::new("first-miss"));
    let installed = context
        .frames
        .get(frame)
        .unwrap()
        .face_realization
        .root_lease()
        .expect("realization miss installed the frame lease");
    let second = context.realize_display_face_for_frame(frame, &Face::new("second-miss"));
    let warm = context
        .frames
        .get(frame)
        .unwrap()
        .face_realization
        .root_lease()
        .expect("warm miss keeps the frame lease");
    assert!(std::sync::Arc::ptr_eq(&installed, &warm));
    // No color specifications: realization returns the merged source as-is.
    assert_eq!(first, Face::new("first-miss"));
    assert_eq!(second, Face::new("second-miss"));
}
