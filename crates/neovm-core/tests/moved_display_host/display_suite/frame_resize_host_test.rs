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

struct ChromeResizeFixture {
    eval: Context,
    frame_id: crate::window::FrameId,
    requests: std::rc::Rc<std::cell::RefCell<Vec<GuiFrameHostRequest>>>,
}

impl ChromeResizeFixture {
    fn new() -> Self {
        crate::test_utils::init_test_tracing();
        let mut eval = crate::test_utils::new_context();
        let buffer = eval.edit.buffers.create_buffer("*chrome-resize*");
        let frame_id = eval
            .frame_manager_mut()
            .create_frame("chrome-resize", 800, 610, buffer);
        eval.frame_manager_mut().select_frame(frame_id);
        let frame = eval.frame_manager_mut().get_mut(frame_id).unwrap();
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.displays_chrome = true;
        frame.char_width = 8.0;
        frame.char_height = 16.0;
        frame.font_pixel_size = 14.0;
        for (key, value) in [
            ("vertical-scroll-bars", Value::NIL),
            ("horizontal-scroll-bars", Value::NIL),
            ("left-fringe", Value::fixnum(0)),
            ("right-fringe", Value::fixnum(0)),
            ("internal-border-width", Value::fixnum(0)),
            ("menu-bar-lines", Value::fixnum(1)),
            ("tool-bar-lines", Value::fixnum(1)),
            ("tab-bar-lines", Value::fixnum(0)),
            ("min-width", Value::fixnum(10)),
            ("min-height", Value::fixnum(5)),
        ] {
            frame.set_parameter(Value::symbol(key), value);
        }
        frame.sync_bar_heights_from_parameters();
        // 560 text pixels, a 16px menu and the established 34px toolbar model.
        frame.resize_pixelwise(800, 610);
        let host = RecordingDisplayHost::with_resolved_frame_font(resolved_frame_font(
            "Chrome Mono",
            "ChromeMono-Regular",
            140,
            FontPxProbeResult {
                pixel_size: 14,
                height: 16,
                ascent: 12,
                descent: 4,
                max_width: 8,
                space_width: 8,
                average_width: 8,
            },
        ));
        let requests = host.resized.clone();
        eval.set_display_host(Box::new(host));
        eval.eval_str("(setq frame-inhibit-implied-resize '(tab-bar-lines))")
            .unwrap();
        Self {
            eval,
            frame_id,
            requests,
        }
    }

    fn acknowledge(&mut self, request: &GuiFrameHostRequest) {
        assert_eq!(request.frame_id, self.frame_id);
        self.eval
            .apply_resize_input_event(request.width, request.height, 1.0, self.frame_id.0, false)
            .unwrap();
    }

    fn assert_geometry(&mut self, columns: i64, rows: i64, native_height: i64) {
        let actual = self
            .eval
            .eval_str(
                "(list (frame-width) (frame-height) (frame-native-height) (frame-char-height))",
            )
            .unwrap();
        assert_eq!(
            list_to_vec(&actual).unwrap(),
            vec![
                Value::fixnum(columns),
                Value::fixnum(rows),
                Value::fixnum(native_height),
                Value::fixnum(16),
            ]
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn toolbar_implied_resize_preserves_text_geometry_with_canonical_policy() {
    let mut fixture = ChromeResizeFixture::new();
    fixture.assert_geometry(100, 35, 560);
    fixture
        .eval
        .eval_str("(modify-frame-parameters nil '((tool-bar-lines . 0)))")
        .unwrap();
    let off = fixture.requests.borrow().last().unwrap().clone();
    assert_eq!((off.width, off.height), (800, 576));
    fixture.acknowledge(&off);
    fixture.assert_geometry(100, 35, 560);

    fixture
        .eval
        .eval_str("(modify-frame-parameters nil '((tool-bar-lines . 1)))")
        .unwrap();
    assert_eq!(fixture.requests.borrow().len(), 2);
    let on = fixture.requests.borrow().last().unwrap().clone();
    assert_eq!((on.width, on.height), (800, 610));
    fixture.acknowledge(&on);
    fixture.assert_geometry(100, 35, 560);
}

#[test]
fn toolbar_implied_resize_honors_true_parameter_list_and_force_inhibition() {
    for policy in ["t", "'(tool-bar-lines)", "'force"] {
        let mut fixture = ChromeResizeFixture::new();
        fixture
            .eval
            .eval_str(&format!("(setq frame-inhibit-implied-resize {policy})"))
            .unwrap();
        fixture
            .eval
            .eval_str("(modify-frame-parameters nil '((tool-bar-lines . 0)))")
            .unwrap();
        assert!(fixture.requests.borrow().is_empty(), "policy {policy}");
        assert_eq!(
            fixture
                .eval
                .frame_manager()
                .get(fixture.frame_id)
                .unwrap()
                .height,
            610
        );
        fixture.assert_geometry(100, 37, 594);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn toolbar_creation_geometry_ignores_ordinary_inhibition_but_honors_force() {
    for (policy, expected_request) in [("t", true), ("'(tool-bar-lines)", true), ("'force", false)]
    {
        let mut fixture = ChromeResizeFixture::new();
        fixture
            .eval
            .eval_str("(frame-after-make-frame nil nil)")
            .unwrap();
        fixture
            .eval
            .eval_str(&format!("(setq frame-inhibit-implied-resize {policy})"))
            .unwrap();
        fixture
            .eval
            .eval_str("(modify-frame-parameters nil '((tool-bar-lines . 0)))")
            .unwrap();
        if expected_request {
            assert_eq!(fixture.requests.borrow().len(), 1);
            let request = fixture.requests.borrow()[0].clone();
            assert_eq!((request.width, request.height), (800, 576));
            fixture.acknowledge(&request);
            fixture.assert_geometry(100, 35, 560);
        } else {
            assert!(fixture.requests.borrow().is_empty());
            fixture.assert_geometry(100, 37, 594);
        }
    }
}

#[test]
fn toolbar_noop_is_silent_and_same_alist_explicit_size_wins_once() {
    for parameters in [
        "'((tool-bar-lines . 0) (width . 90) (height . 40))",
        "'((height . 40) (width . 90) (tool-bar-lines . 0))",
    ] {
        let mut fixture = ChromeResizeFixture::new();
        fixture
            .eval
            .eval_str("(modify-frame-parameters nil '((tool-bar-lines . 1)))")
            .unwrap();
        assert!(fixture.requests.borrow().is_empty());
        fixture
            .eval
            .eval_str(&format!("(modify-frame-parameters nil {parameters})"))
            .unwrap();
        assert_eq!(fixture.requests.borrow().len(), 1);
        let request = fixture.requests.borrow()[0].clone();
        assert_eq!((request.width, request.height), (720, 656));
        fixture.acknowledge(&request);
        fixture.assert_geometry(90, 40, 640);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn toolbar_change_preserves_requested_explicit_grid_across_stale_native_ack() {
    let mut fixture = ChromeResizeFixture::new();
    fixture.eval.eval_str("(set-frame-size nil 90 40)").unwrap();
    let old = fixture.requests.borrow()[0].clone();
    assert_eq!((old.width, old.height), (720, 690));
    fixture
        .eval
        .eval_str("(modify-frame-parameters nil '((tool-bar-lines . 0)))")
        .unwrap();
    assert_eq!(fixture.requests.borrow().len(), 2);
    let latest = fixture.requests.borrow()[1].clone();
    assert_eq!((latest.width, latest.height), (720, 656));
    fixture.acknowledge(&old);
    fixture.acknowledge(&latest);
    fixture.assert_geometry(90, 40, 640);
}

#[cfg(target_os = "linux")]
#[test]
fn toolbar_change_preserves_unsent_explicit_size_after_same_metric_font_change() {
    let mut fixture = ChromeResizeFixture::new();
    // Opening a different font with the same cell metrics defers subsequent
    // explicit parameters, without itself requiring a native size change.
    fixture
        .eval
        .eval_str(r#"(internal-set-lisp-face-attribute 'default :font "Chrome Mono-14" nil)"#)
        .unwrap();
    fixture
        .eval
        .eval_str("(modify-frame-parameters nil '((width . 90) (height . 40)))")
        .unwrap();
    assert!(fixture.requests.borrow().is_empty());
    fixture
        .eval
        .eval_str("(modify-frame-parameters nil '((tool-bar-lines . 0)))")
        .unwrap();
    assert_eq!(fixture.requests.borrow().len(), 1);
    let request = fixture.requests.borrow()[0].clone();
    assert_eq!((request.width, request.height), (720, 656));
    fixture.acknowledge(&request);
    fixture.assert_geometry(90, 40, 640);
}

#[cfg(target_os = "linux")]
#[test]
fn menu_and_internal_border_changes_share_implied_resize_and_settled_geometry() {
    let mut fixture = ChromeResizeFixture::new();
    fixture
        .eval
        .eval_str(
            "(modify-frame-parameters nil '((menu-bar-lines . 0) (internal-border-width . 3)))",
        )
        .unwrap();
    assert_eq!(fixture.requests.borrow().len(), 1);
    let request = fixture.requests.borrow()[0].clone();
    // The native API includes the six border pixels, excludes the toolbar.
    assert_eq!((request.width, request.height), (806, 600));
    fixture.acknowledge(&request);
    fixture.assert_geometry(100, 35, 566);
}

#[cfg(target_os = "linux")]
#[test]
fn menu_and_border_parameter_lists_inhibit_only_the_changed_reason() {
    for (policy, parameters, expected_request, geometry) in [
        (
            "'(menu-bar-lines)",
            "'((menu-bar-lines . 0))",
            None,
            (100, 36, 576),
        ),
        (
            "'(internal-border-width)",
            "'((internal-border-width . 3))",
            None,
            (99, 34, 560),
        ),
        (
            "'(tool-bar-lines)",
            "'((menu-bar-lines . 0))",
            Some((800, 594)),
            (100, 35, 560),
        ),
        (
            "'(tool-bar-lines)",
            "'((internal-border-width . 3))",
            Some((806, 616)),
            (100, 35, 566),
        ),
    ] {
        let mut fixture = ChromeResizeFixture::new();
        fixture
            .eval
            .eval_str(&format!("(setq frame-inhibit-implied-resize {policy})"))
            .unwrap();
        fixture
            .eval
            .eval_str(&format!("(modify-frame-parameters nil {parameters})"))
            .unwrap();
        if let Some((width, height)) = expected_request {
            assert_eq!(fixture.requests.borrow().len(), 1);
            let request = fixture.requests.borrow()[0].clone();
            assert_eq!((request.width, request.height), (width, height));
            fixture.acknowledge(&request);
        } else {
            assert!(fixture.requests.borrow().is_empty());
        }
        fixture.assert_geometry(geometry.0, geometry.1, geometry.2);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn chrome_resize_minima_override_inhibition_only_on_the_required_axis() {
    for policy in ["t", "'force"] {
        let mut fixture = ChromeResizeFixture::new();
        fixture
            .eval
            .eval_str(&format!("(setq frame-inhibit-implied-resize {policy})"))
            .unwrap();
        fixture
            .eval
            .eval_str("(modify-frame-parameters nil '((min-height . 40)))")
            .unwrap();
        assert!(fixture.requests.borrow().is_empty());
        fixture
            .eval
            .eval_str("(modify-frame-parameters nil '((tool-bar-lines . 0)))")
            .unwrap();
        assert_eq!(fixture.requests.borrow().len(), 1);
        let request = fixture.requests.borrow()[0].clone();
        assert_eq!((request.width, request.height), (800, 656));
        fixture.acknowledge(&request);
        fixture.assert_geometry(100, 40, 640);
    }

    let mut fixture = ChromeResizeFixture::new();
    fixture
        .eval
        .eval_str("(setq frame-inhibit-implied-resize 'force)")
        .unwrap();
    fixture
        .eval
        .eval_str("(modify-frame-parameters nil '((min-width . 110)))")
        .unwrap();
    assert!(fixture.requests.borrow().is_empty());
    fixture
        .eval
        .eval_str("(modify-frame-parameters nil '((internal-border-width . 3)))")
        .unwrap();
    assert_eq!(fixture.requests.borrow().len(), 1);
    let request = fixture.requests.borrow()[0].clone();
    assert_eq!((request.width, request.height), (886, 610));
    fixture.acknowledge(&request);
    fixture.assert_geometry(110, 34, 560);
}
