use super::*;
use neovm_core::emacs_core::eval::RedisplayHookPolicyGuard;
use neovm_core::window::{SplitDirection, SplitPlacement};

/// Exercise the active-mini resize after the engine has measured its rows.
/// Thirteen inner rows model the fourteen-row terminal from the Vertico
/// regression, with its external one-row menu already removed. No chrome
/// producer or package oracle is needed to observe the root's allocation.
fn assert_active_mini_respects_root_minimum(
    window_min_height: i64,
    split: Option<SplitDirection>,
    split_size: Option<i64>,
    expected_root_minimum: i64,
) {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let mut eval =
        create_bootstrap_evaluator_cached_with_features(&["x", "neomacs"]).expect("bootstrap");
    apply_runtime_startup_state(&mut eval).expect("runtime startup state");
    eval.store_variable_by_name("resize-mini-windows", Value::symbol("grow-only"));
    eval.store_variable_by_name("max-mini-window-height", Value::fixnum(20));
    eval.store_variable_by_name("window-min-height", Value::fixnum(window_min_height));

    let root_buffer = eval
        .buffer_manager()
        .current_buffer()
        .expect("root buffer")
        .id();
    let frame_id = eval.frame_manager_mut().create_frame(
        "short-active-mini-root-minimum",
        80,
        13,
        root_buffer,
    );
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.shrink_mini_window();
    }
    eval.eval_form(Value::list(vec![
        Value::symbol("select-frame"),
        Value::make_frame(frame_id.0),
    ]))
    .expect("select fixture frame");

    if let Some(direction) = split {
        let target = eval
            .frame_manager()
            .get(frame_id)
            .expect("frame")
            .selected_window;
        eval.frame_manager_mut()
            .split_window(
                frame_id,
                target,
                direction,
                root_buffer,
                split_size,
                SplitPlacement::AfterTarget,
            )
            .expect("split root");
    }
    let root = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .root_window()
        .id();
    // GNU window.el: window--min-size-1 sums top/bottom children and takes
    // the maximum for side-by-side children. Include the real Lisp policy,
    // so this test does not replace it with a hard-coded frame-wide clamp.
    let root_minimum = eval
        .eval_form(Value::list(vec![
            Value::symbol("window-min-size"),
            Value::make_window(root.0),
            Value::NIL,
            Value::NIL,
            Value::T,
        ]))
        .expect("GNU-shaped root minimum")
        .as_fixnum()
        .expect("integral root pixel minimum");
    assert_eq!(root_minimum, expected_root_minimum, "fixture root policy");

    let minibuffer = eval.buffer_manager_mut().create_buffer(" *Minibuf-1*");
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(minibuffer)
            .expect("minibuffer");
        buffer.insert("mini\nfacet01\nfacet02\nfacet03\nfacet04\nfacet05\nfacet06\nfacet07\nfacet08\nfacet09\nfacet10");
        let eob = buffer.point_max_emacs_byte_pos();
        buffer.goto_emacs_byte_pos(eob);
    }
    let mini = eval
        .activate_minibuffer_window_for_buffer(minibuffer, LispString::from_utf8(""), None)
        .expect("activate minibuffer")
        .expect("minibuffer window");
    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(&mut eval, frame_id);
    assert!(
        engine.last_frame_display_state.is_some(),
        "layout converges"
    );

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let root_height = frame.root_window().bounds().height;
    let mini_height = frame.find_window(mini).expect("mini").bounds().height;
    assert_eq!(
        root_height, root_minimum as f32,
        "mini growth must leave the root's achievable GNU minimum"
    );
    assert_eq!(mini_height, 13.0 - root_minimum as f32);
    assert_eq!(root_height + mini_height, 13.0, "conserve inner rows");
    for leaf in frame.root_leaf_ids() {
        assert!(
            frame.find_window(leaf).expect("root leaf").bounds().height >= window_min_height as f32,
            "root allocation must preserve each child's minimum, not just their sum"
        );
    }
}

#[test]
fn active_mini_growth_respects_default_root_minimum_on_short_tty() {
    assert_active_mini_respects_root_minimum(4, None, None, 4);
}

#[test]
fn active_mini_growth_respects_custom_root_minimum_on_short_tty() {
    assert_active_mini_respects_root_minimum(6, None, None, 6);
}

#[test]
fn active_mini_growth_respects_side_by_side_root_minimum() {
    assert_active_mini_respects_root_minimum(4, Some(SplitDirection::Horizontal), None, 4);
}

#[test]
fn active_mini_growth_respects_stacked_root_minimum() {
    assert_active_mini_respects_root_minimum(4, Some(SplitDirection::Vertical), None, 8);
}

#[test]
fn active_mini_growth_preserves_each_uneven_stacked_child_minimum() {
    // Start with 8+4 rows. Merely clamping the root to8 and proportionally
    // redistributing would leave the second child below its four-row minimum.
    assert_active_mini_respects_root_minimum(4, Some(SplitDirection::Vertical), Some(8), 8);
}
