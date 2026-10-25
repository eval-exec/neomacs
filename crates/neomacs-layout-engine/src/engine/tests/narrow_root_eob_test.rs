use super::*;
use neovm_core::emacs_core::eval::RedisplayHookPolicyGuard;

#[test]
fn narrowing_with_active_mini_recenters_the_root_to_its_visible_eob_row() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let mut eval = Context::new();
    eval.store_variable_by_name("resize-mini-windows", Value::symbol("grow-only"));
    eval.store_variable_by_name("max-mini-window-height", Value::fixnum(11));
    let root_buffer = eval
        .buffer_manager()
        .current_buffer()
        .expect("root buffer")
        .id();
    // Same four logical scratch lines as the existing visible-EOB positive
    // control and the saved GNU narrow-terminal scenario. At sixty columns,
    // these occupy six display rows, including the final empty EOB row.
    let text = ";; This buffer is for text that is not saved, and for Lisp evaluation.\n\
;; To create a file, visit it with \u{2018}C-x C-f\u{2019} and enter text in its buffer.\n\n";
    let point = {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(root_buffer)
            .expect("root buffer");
        buffer.set_buffer_local("mode-line-format", Value::string("ROOT MODE"));
        buffer.insert(text);
        let point = LispCharPos1::from_one_based_usize(buffer.point_max_char_pos().get() + 1);
        buffer.goto_emacs_byte_pos(buffer.point_max_emacs_byte_pos());
        point
    };
    let continuation = " in its buffer.";
    let continuation_byte = text.find(continuation).expect("GNU continuation tail");
    let expected_start =
        LispCharPos1::from_one_based_usize(text[..continuation_byte].chars().count() + 1);

    // External menu chrome is already removed: the saved physical resize
    // 100x24 -> 60x18 is an inner-frame resize 100x23 -> 60x17. Keep the
    // eleven-row active mini, so the final root has six rows including its
    // mode line and exactly five rows for text.
    let frame_id = eval.frame_manager_mut().create_frame(
        "narrow-root-trailing-newline-eob",
        100,
        23,
        root_buffer,
    );
    let root = {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.shrink_mini_window();
        let resize = frame
            .plan_mini_window_resize(11.0, 11.0)
            .expect("mini growth");
        frame.apply_mini_window_resize(resize);
        let root = frame.root_window().id();
        let neovm_core::window::Window::Leaf {
            point: window_point,
            window_start,
            ..
        } = frame.find_window_mut(root).expect("root")
        else {
            panic!("root must be a leaf");
        };
        *window_point = point;
        *window_start = LispCharPos1::ONE;
        root
    };
    eval.eval_form(Value::list(vec![
        Value::symbol("select-frame"),
        Value::make_frame(frame_id.0),
    ]))
    .expect("select fixture frame");

    let mini_buffer = eval.buffer_manager_mut().create_buffer(" *Minibuf-1*");
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(mini_buffer)
            .expect("mini buffer");
        buffer.insert("mini\nfacet01\nfacet02\nfacet03\nfacet04\nfacet05\nfacet06\nfacet07\nfacet08\nfacet09\nfacet10");
        buffer.goto_emacs_byte_pos(buffer.point_max_emacs_byte_pos());
    }
    let mini = eval
        .activate_minibuffer_window_for_buffer(mini_buffer, LispString::from_utf8(""), None)
        .expect("activate mini")
        .expect("mini window");
    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(&mut eval, frame_id);
    {
        let frame = eval.frame_manager().get(frame_id).expect("frame");
        assert_eq!((frame.width, frame.height), (100, 23));
        assert_eq!(frame.root_window().bounds().height, 12.0);
        assert_eq!(frame.find_window(mini).expect("mini").bounds().height, 11.0);
        assert_eq!(frame.selected_window, mini);
        assert_eq!(frame.root_window().window_start(), Some(LispCharPos1::ONE));
        let snapshot = frame
            .redisplay_snapshot(root)
            .expect("initial root snapshot");
        let eob = snapshot
            .point_for_buffer_pos(point)
            .expect("initial visible EOB");
        let body_top = snapshot.regions.text_body.y - snapshot.regions.outer.y;
        assert!(
            eob.y as f32 >= body_top
                && (eob.y + eob.height) as f32 <= body_top + snapshot.regions.text_body.height,
            "the positive-control initial viewport must contain EOB"
        );
    }
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .resize_pixelwise(60, 17);
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("resized layout");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == root.0 as i64)
        .expect("root matrix");
    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(root)
        .expect("resized root snapshot");
    assert_eq!((frame.width, frame.height), (60, 17));
    assert_eq!(frame.root_window().bounds().height, 6.0);
    assert_eq!(frame.find_window(mini).expect("mini").bounds().height, 11.0);
    // The matrix's text_pixel_bounds reserves the horizontal text area over
    // the whole window height. The canonical body partition/clip excludes
    // chrome vertically and is the actual five-row viewport being tested.
    assert_eq!(
        snapshot.mode_line_height, 1,
        "visible one-row root mode line"
    );
    assert_eq!(
        snapshot.regions.text_body.height, 5.0,
        "five-row root text box"
    );
    assert_eq!(
        entry.text_clip_bounds.expect("canonical body clip").height,
        5.0
    );
    assert_eq!(
        frame.selected_window, mini,
        "physical cursor belongs to mini"
    );
    assert_eq!(
        eval.buffer_manager()
            .current_buffer()
            .expect("current buffer")
            .id(),
        mini_buffer,
        "root placement must use its own buffer while mini is current"
    );
    let root_window = frame.find_window(root).expect("root");
    let neovm_core::window::Window::Leaf {
        point: root_point,
        window_start,
        ..
    } = root_window
    else {
        panic!("root must remain a leaf");
    };
    assert_eq!(*root_point, point, "resize must not move the root's point");
    assert_eq!(
        *window_start, expected_start,
        "GNU recenters the trailing-newline EOB row, placing its last wrapped comment tail first"
    );
    let first_row = enabled_window_row_texts(entry)
        .into_iter()
        .next()
        .expect("first root row");
    assert_eq!(first_row.trim_end(), continuation);
    let eob = snapshot.point_for_buffer_pos(point).expect("root EOB row");
    let body_top = snapshot.regions.text_body.y - snapshot.regions.outer.y;
    assert!(
        eob.y as f32 >= body_top
            && (eob.y + eob.height) as f32 <= body_top + snapshot.regions.text_body.height,
        "root EOB must own an admitted on-screen insertion row despite mini owning the frame cursor"
    );
}
