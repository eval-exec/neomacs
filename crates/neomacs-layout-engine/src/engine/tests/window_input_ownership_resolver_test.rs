//! Register as a child of engine::tests to reuse its exclusive-Context fixtures.
use super::*;
use neovm_core::window::WindowLayoutQueryScope;
use std::cell::Cell;
use std::num::NonZeroUsize;
use std::rc::Rc;

struct ResolverCatalog;
impl ImageCatalog for ResolverCatalog {
    fn lookup(&self, _: ImageResolveRequest, _: ImageSizeLimit) -> ImageLookup {
        panic!("source adjustment must not perform an image lookup")
    }
}

struct ResolverHost {
    catalog: Rc<dyn ImageCatalog>,
    calls: Rc<Cell<usize>>,
}
impl neovm_core::emacs_core::DisplayConnection for ResolverHost {
    fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        panic!("source adjustment must not realize a GUI frame")
    }
    fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        panic!("source adjustment must not resize a GUI frame")
    }
}

impl neovm_core::emacs_core::DisplayServices for ResolverHost {
    fn image_catalog_shared(&self) -> Option<Rc<dyn ImageCatalog>> {
        self.calls.set(self.calls.get() + 1);
        Some(Rc::clone(&self.catalog))
    }
}

fn resolver_fixture() -> (Context, neovm_core::window::FrameId, WindowParams) {
    let (eval, frame, buffer, window) = incr_editing_frame(
        "λ0123456789 abcdefghijklmnopqrstuvwxyz\nsecond line\n",
        640,
        320,
    );
    let mut params = test_window_params();
    params.window_id = window.0 as i64;
    params.buffer_id = buffer.0;
    params.window_start = 9;
    params.buffer_begv = 3;
    params.buffer_size = 35;
    params.point = 28;
    params.previous_visible_end = Some(31);
    params.hscroll = 5;
    params.vscroll = 7;
    params.tab_stop_list = vec![3, 9, 17];
    params.wrap_prefix = "→ ".as_bytes().to_vec();
    params.line_prefix = b"| ".to_vec();
    let mut profile = neomacs_display_protocol::effect_config::EffectsConfig::default();
    profile.indent_guides.rainbow_colors = vec![(0.25, 0.5, 0.75, 1.0)];
    params.cursor_effects = Some(profile.into());
    (eval, frame, params)
}

fn paired_resolve(
    eval: &mut Context,
    params: &WindowParams,
    purpose: WindowLayoutWalkPurpose,
) -> (WindowParams, WindowDisplaySource) {
    let expected_catalog: Rc<dyn ImageCatalog> = Rc::new(ResolverCatalog);
    let calls = Rc::new(Cell::new(0));
    eval.set_display_host(Box::new(ResolverHost {
        catalog: Rc::clone(&expected_catalog),
        calls: Rc::clone(&calls),
    }));
    let before = format!("{params:?}");
    let legacy = resolve_window_display_source_params_legacy(eval, params, purpose);
    assert_eq!(calls.get(), 1, "legacy host query count");
    let mut actual = params.clone();
    let tab_storage = actual.tab_stop_list.as_ptr();
    let prefix_storage = actual.wrap_prefix.as_ptr();
    let profile = actual.cursor_effects.clone();
    let source = resolve_window_display_source_params(eval, &mut actual, purpose);
    assert_eq!(
        calls.get(),
        2,
        "one catalog query per resolution in the same order"
    );
    assert_eq!(source, legacy.source);
    assert_eq!(
        format!("{actual:?}"),
        format!("{:?}", legacy.params),
        "all debug-visible inputs must agree, not only source and range"
    );
    assert_eq!(
        format!("{params:?}"),
        before,
        "borrowed legacy source remains unchanged"
    );
    assert_eq!(actual.tab_stop_list.as_ptr(), tab_storage);
    assert_eq!(actual.wrap_prefix.as_ptr(), prefix_storage);
    for result in [&legacy.params, &actual] {
        let catalog = &result.space_image_catalog.as_ref().expect("host catalog").0;
        assert!(Rc::ptr_eq(catalog, &expected_catalog));
        match (result.cursor_effects.as_ref(), profile.as_ref()) {
            (Some(actual), Some(expected)) => {
                assert!(std::ptr::eq(actual.as_ref(), expected.as_ref()))
            }
            (None, None) => {}
            _ => panic!("source adjustment must preserve optional profile presence"),
        }
    }
    (actual, source)
}

#[test]
fn source_resolution_normal_and_mini_preparation_match_all_legacy_inputs() {
    let (mut eval, _, params) = resolver_fixture();
    for purpose in [
        WindowLayoutWalkPurpose::Redisplay,
        WindowLayoutWalkPurpose::Snapshot,
    ] {
        let (actual, source) = paired_resolve(&mut eval, &params, purpose);
        assert_eq!(source, WindowDisplaySource::LiveWindow);
        assert_eq!(actual.window_start, 9);
        assert_eq!(actual.vscroll, 7);
    }
    let (actual, source) =
        paired_resolve(&mut eval, &params, WindowLayoutWalkPurpose::MiniPreparation);
    assert_eq!(source, WindowDisplaySource::LiveWindow);
    assert_eq!(actual.window_start, params.buffer_begv);
    assert_eq!(actual.vscroll, 0);
    assert!(actual.force_start);
    assert!(actual.previous_visible_end.is_none());
    assert_eq!(
        actual.source_extent,
        crate::types::WindowSourceExtent::AccessibleEnd
    );
}

#[test]
fn source_resolution_keeps_absent_cursor_profile_absent() {
    let (mut eval, _, mut params) = resolver_fixture();
    params.cursor_effects = None;
    let (actual, source) = paired_resolve(&mut eval, &params, WindowLayoutWalkPurpose::Redisplay);
    assert_eq!(source, WindowDisplaySource::LiveWindow);
    assert!(actual.cursor_effects.is_none());
}

#[test]
fn source_resolution_every_query_scope_matches_legacy_with_independent_budgets() {
    let (mut eval, _, params) = resolver_fixture();
    let count = NonZeroUsize::new(4).expect("positive rows");
    let height = NonZeroUsize::new(53).expect("positive pixels");
    let scopes = [
        WindowLayoutQueryScope::Viewport,
        WindowLayoutQueryScope::Position {
            target: LispCharPos1::new(19),
        },
        WindowLayoutQueryScope::Rows {
            start: LispCharPos1::ONE,
            count,
        },
        WindowLayoutQueryScope::Pixels {
            start: LispCharPos1::new(99),
            height,
        },
        WindowLayoutQueryScope::TextExtent {
            buffer: BufferId(params.buffer_id),
            start: LispCharPos1::ONE,
            end: LispCharPos1::new(20),
            width: Some(37),
            height: Some(height),
        },
        WindowLayoutQueryScope::TextExtent {
            buffer: BufferId(params.buffer_id),
            start: LispCharPos1::new(99),
            end: LispCharPos1::new(99),
            width: None,
            height: None,
        },
    ];
    for scope in scopes {
        let (actual, source) = paired_resolve(
            &mut eval,
            &params,
            WindowLayoutWalkPurpose::SynchronousQuery(scope),
        );
        assert_eq!(source, WindowDisplaySource::LiveWindow);
        if let WindowLayoutQueryScope::TextExtent { width, height, .. } = scope {
            assert_eq!(
                actual.measurement_width,
                Some(width.unwrap_or(i32::MAX as usize))
            );
            assert_eq!(actual.measurement_pixels, height);
            assert_eq!(actual.hscroll, 0);
            assert!(actual.force_start);
        }
    }
}

#[test]
fn source_resolution_position_query_keeps_fontification_callback_extent() {
    let (mut eval, _, params) = resolver_fixture();
    let purpose = WindowLayoutWalkPurpose::SynchronousQuery(WindowLayoutQueryScope::Position {
        target: LispCharPos1::new(19),
    });
    eval.store_variable_by_name("fontification-functions", Value::T);
    let (callbacks, _) = paired_resolve(&mut eval, &params, purpose);
    assert!(callbacks.query_target.is_none());
    eval.buffer_manager_mut()
        .get_mut(BufferId(params.buffer_id))
        .expect("buffer")
        .set_buffer_local("fontification-functions", Value::NIL);
    let (local_nil, _) = paired_resolve(&mut eval, &params, purpose);
    assert_eq!(
        local_nil.query_target,
        Some(crate::types::LayoutCharPos0::new(18))
    );
}

#[test]
fn source_resolution_inactive_mini_own_buffer_and_echo_keep_cursor_source() {
    for role in [WindowCursorRole::Active, WindowCursorRole::Inactive] {
        let (mut eval, frame, mut params) = resolver_fixture();
        let mini = eval
            .frame_manager()
            .get(frame)
            .expect("frame")
            .minibuffer_window
            .expect("mini window");
        params.window_id = mini.0 as i64;
        params.kind = crate::types::WindowKind::Minibuffer;
        params.cursor_role = role;
        eval.set_current_message(None);
        let (_, source) = paired_resolve(&mut eval, &params, WindowLayoutWalkPurpose::Redisplay);
        assert_eq!(source, WindowDisplaySource::LiveWindow);
        eval.set_current_message(Some(LispString::from_utf8("λ echo\nmessage")));
        let (actual, source) =
            paired_resolve(&mut eval, &params, WindowLayoutWalkPurpose::Redisplay);
        assert_eq!(source, WindowDisplaySource::InactiveEchoArea);
        assert_eq!(
            actual.buffer_id,
            eval.echo_area_display_buffer().expect("echo buffer").0
        );
        assert_eq!(actual.window_start, 0);
        assert_eq!(actual.buffer_begv, 0);
        assert!(actual.previous_visible_end.is_none());
        assert_eq!(
            actual.point,
            if role.is_active() {
                actual.buffer_size
            } else {
                0
            }
        );
    }
}

#[test]
fn source_resolution_active_minibuffer_keeps_live_source_despite_message() {
    let (mut eval, _, mut params) = resolver_fixture();
    let buffer = eval
        .buffer_manager_mut()
        .create_buffer(" *ownership-active-mini*");
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("mini buffer")
        .insert("prompt: λ");
    let mini = eval
        .activate_minibuffer_window_for_buffer(buffer, LispString::from_utf8("prompt: "), None)
        .expect("activate mini")
        .expect("mini window");
    params.window_id = mini.0 as i64;
    params.buffer_id = buffer.0;
    params.kind = crate::types::WindowKind::Minibuffer;
    eval.set_current_message(Some(LispString::from_utf8("other message")));
    let (actual, source) = paired_resolve(&mut eval, &params, WindowLayoutWalkPurpose::Redisplay);
    assert_eq!(source, WindowDisplaySource::LiveWindow);
    assert_eq!(actual.buffer_id, buffer.0);
}
