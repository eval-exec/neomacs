//! Re-homed frame-resize host tests (P1 B1): bodies are the display originals;
//! only the constructor line changed.
use super::*;

#[test]
fn overlapping_native_observations_keep_the_latest_requested_grid_for_font_changes() {
    let mut eval = crate::test_utils::new_context();
    let buffer = eval.edit.buffers.create_buffer("*overlapping-resize*");
    let fid = eval
        .frame_manager_mut()
        .create_frame("overlapping", 800, 600, buffer);
    let frame = eval.frame_manager_mut().get_mut(fid).unwrap();
    frame.set_window_system(Some(Value::symbol("neo")));
    frame.char_width = 8.0;
    frame.char_height = 16.0;
    for (key, value) in [
        ("vertical-scroll-bars", Value::NIL),
        ("left-fringe", Value::fixnum(0)),
        ("right-fringe", Value::fixnum(0)),
    ] {
        frame.set_parameter(Value::symbol(key), value);
    }
    frame.resize_pixelwise(800, 600);
    let host = RecordingDisplayHost::with_resolved_frame_font(remapped_mono_font_metrics());
    let requests = host.resized.clone();
    eval.set_display_host(Box::new(host));
    eval.eval_str("(set-frame-width nil 91)").unwrap();
    let a = requests.borrow().last().unwrap().clone();
    eval.eval_str("(set-frame-width nil 101)").unwrap();
    let b = requests.borrow().last().unwrap().clone();
    let other = eval
        .frame_manager_mut()
        .create_frame("other", 300, 200, buffer);
    let tx = crate::host_input::HostInputPublisher::install(&mut eval);

    for event in [
        neovm_eval::emacs_core::eval::keyboard::InputEvent::Focus {
            focused: true,
            emacs_frame_id: fid.0,
        },
        neovm_eval::emacs_core::eval::keyboard::InputEvent::Resize {
            width: a.width,
            height: a.height,
            scale_factor: 2.0,
            emacs_frame_id: fid.0,
        },
        neovm_eval::emacs_core::eval::keyboard::InputEvent::Resize {
            width: 320,
            height: 240,
            scale_factor: 1.0,
            emacs_frame_id: other.0,
        },
        neovm_eval::emacs_core::eval::keyboard::InputEvent::key_press(
            neovm_eval::emacs_core::eval::keyboard::KeyEvent::char('r'),
        ),
    ] {
        tx.send(event).unwrap();
    }
    assert_eq!(
        eval.eval_str("(frame-native-width)").unwrap().as_int(),
        Some(a.width as i64)
    );
    assert_eq!(
        (
            eval.frame_manager().get(other).unwrap().width,
            eval.frame_manager().get(other).unwrap().height
        ),
        (320, 240)
    );
    assert_eq!(eval.read_char().unwrap(), Value::fixnum('r' as i64));
    eval.eval_str("(modify-frame-parameters nil '((left . 10)))")
        .unwrap();
    eval.eval_str(r#"(internal-set-lisp-face-attribute 'default :font "Remapped Mono-24" nil)"#)
        .unwrap();
    let c = requests.borrow().last().unwrap().clone();
    assert_eq!(
        c.width,
        101 * 16,
        "the newest requested grid must survive an older allocation and a geometry query"
    );
    for observed in [&b, &b, &c] {
        eval.apply_resize_input_event(observed.width, observed.height, 1.0, fid.0, false)
            .expect("resize redisplay");
        assert_eq!(
            eval.eval_str("(frame-native-width)").unwrap().as_int(),
            Some(observed.width as i64)
        );
    }
    assert!(
        eval.frame_manager()
            .get(fid)
            .unwrap()
            .pending_gui_resize
            .is_none(),
        "attaining the target releases pending intent"
    );
    eval.apply_resize_input_event(84 * 16, c.height, 1.0, fid.0, false)
        .expect("resize redisplay");
    assert_eq!(
        eval.eval_str("(frame-text-cols)").unwrap().as_int(),
        Some(84),
        "a later unsolicited allocation remains authoritative"
    );
}

#[test]
fn inhibited_font_change_preserves_an_unsent_explicit_resize() {
    let mut eval = crate::test_utils::new_context();
    let buffer = eval.edit.buffers.create_buffer("*font-resize*");
    let fid = eval
        .frame_manager_mut()
        .create_frame("F1", 800, 600, buffer);
    let frame = eval.frame_manager_mut().get_mut(fid).unwrap();
    frame.set_window_system(Some(Value::symbol("neo")));
    frame.char_width = 8.0;
    frame.char_height = 16.0;
    let host = RecordingDisplayHost::with_resolved_frame_font(remapped_mono_font_metrics());
    let requests = host.resized.clone();
    eval.set_display_host(Box::new(host));
    eval.eval_str("(internal-set-lisp-face-attribute 'default :font \"Remapped Mono-24\" nil)")
        .unwrap();
    assert_eq!(requests.borrow().len(), 1);
    eval.eval_str("(modify-frame-parameters nil '((width . 80) (height . 25)))")
        .unwrap();

    let mut host = RecordingDisplayHost::with_resolved_frame_font(resolved_frame_font(
        "Small Mono",
        "SmallMono-Regular",
        128,
        FontPxProbeResult {
            pixel_size: 17,
            height: 18,
            ascent: 14,
            descent: 4,
            max_width: 9,
            space_width: 9,
            average_width: 9,
        },
    ));
    host.resized = requests.clone();
    eval.set_display_host(Box::new(host));
    eval.eval_str("(setq frame-inhibit-implied-resize t)")
        .unwrap();
    eval.eval_str("(internal-set-lisp-face-attribute 'default :font \"Small Mono-13\" nil)")
        .unwrap();
    assert_eq!(
        requests.borrow().len(),
        1,
        "inhibition must not request another native resize"
    );

    let tx = crate::host_input::HostInputPublisher::install(&mut eval);
    // 80 columns at 9px plus 16px of fringes and the GNU 16px scroll-bar
    // default: the explicit native resize is column-aligned, so it must reach
    // the host unchanged.
    tx.send(neovm_eval::emacs_core::eval::keyboard::InputEvent::Resize {
        width: 752,
        height: 450,
        scale_factor: 1.0,
        emacs_frame_id: fid.0,
    })
    .unwrap();
    eval.eval_str("(frame-native-width)").unwrap();
    let requests = requests.borrow();
    assert_eq!(
        requests.len(),
        2,
        "the explicit request must still reach the host"
    );
    assert_eq!((requests[1].width, requests[1].height), (752, 450));
}
