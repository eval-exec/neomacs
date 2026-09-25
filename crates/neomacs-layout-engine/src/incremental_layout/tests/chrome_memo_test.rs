//! P3.5 E2: what the chrome memo's fingerprint sees and what it refuses.
//! Strings are built by Lisp in a bare context, so two evaluations of the
//! same mode line produce distinct but equal objects, as they do in a real
//! redisplay.

use super::*;
use neovm_core::emacs_core::Context;

fn inputs() -> ChromeRowInputs {
    ChromeRowInputs {
        kind: WindowChromeKind::ModeLine,
        display_row_index: 39,
        bounds: Rect::new(0.0, 624.0, 800.0, 16.0),
        text_area_left_px: 0.0,
        selected: true,
        metrics: DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        tab_policy: DisplayTabPolicy::from_tab_width_and_stops(0.0, 8, &[]),
        base_face: ResolvedFace::default(),
        image_scale_environment: ImageScaleEnvironment::default(),
        glyphless_table_bits: None,
        automatic_composition: false,
        face_change_count: 0,
        char_table_revision: neovm_core::window::CharTableLayoutRevision::default(),
        media_generation: 0,
        symbol_values: Box::new([]),
    }
}

fn fingerprint(eval: &mut Context, form: &str) -> Option<ChromeRowFingerprint> {
    let value = eval.eval_str(form).expect("chrome string");
    ChromeRowFingerprint::capture(inputs(), &ModeLineDisplayOutput::from_root_string(value))
}

const PLAIN: &str = r#"(concat " " (propertize "buf.el" 'face '(:weight bold) 'mouse-face 'mode-line-highlight) "  L12")"#;

#[test]
fn two_evaluations_of_the_same_mode_line_fingerprint_equal() {
    let mut eval = Context::new();
    let first = fingerprint(&mut eval, PLAIN).expect("plain data");
    let second = fingerprint(&mut eval, PLAIN).expect("plain data");
    assert_eq!(first, second, "fresh conses with equal contents");
}

#[test]
fn text_faces_and_mouse_faces_are_part_of_the_fingerprint() {
    let mut eval = Context::new();
    let base = fingerprint(&mut eval, PLAIN).expect("plain data");
    for changed in [
        r#"(concat " " (propertize "buf.el" 'face '(:weight bold) 'mouse-face 'mode-line-highlight) "  L13")"#,
        r#"(concat " " (propertize "buf.el" 'face '(:weight normal) 'mouse-face 'mode-line-highlight) "  L12")"#,
        r#"(concat " " (propertize "buf.el" 'face '(:weight bold) 'mouse-face 'highlight) "  L12")"#,
        r#"(concat " " (propertize "buf.e" 'face '(:weight bold) 'mouse-face 'mode-line-highlight) "l  L12")"#,
    ] {
        let other = fingerprint(&mut eval, changed).expect("plain data");
        assert_ne!(base, other, "{changed}");
    }
}

#[test]
fn request_inputs_are_part_of_the_fingerprint() {
    let mut eval = Context::new();
    let value = eval.eval_str(PLAIN).expect("chrome string");
    let formatted = ModeLineDisplayOutput::from_root_string(value);
    let base = ChromeRowFingerprint::capture(inputs(), &formatted).expect("plain data");
    let mut inactive = inputs();
    inactive.selected = false;
    let mut faces_changed = inputs();
    faces_changed.face_change_count = 1;
    let mut narrower = inputs();
    narrower.bounds = Rect::new(0.0, 624.0, 400.0, 16.0);
    for other in [inactive, faces_changed, narrower] {
        assert_ne!(
            Some(base.clone()),
            ChromeRowFingerprint::capture(other, &formatted)
        );
    }
}

#[test]
fn the_order_of_a_property_list_does_not_matter() {
    // The mode-line formatter merges property lists in no fixed order.
    let mut eval = Context::new();
    let ab = fingerprint(
        &mut eval,
        r#"(let ((s (copy-sequence "x"))) (set-text-properties 0 1 '(face bold mouse-face highlight) s) s)"#,
    )
    .expect("plain data");
    let ba = fingerprint(
        &mut eval,
        r#"(let ((s (copy-sequence "x"))) (set-text-properties 0 1 '(mouse-face highlight face bold) s) s)"#,
    )
    .expect("plain data");
    assert_eq!(ab, ba);
}

#[test]
fn mouse_target_properties_are_fingerprinted_by_name_only() {
    // A hit publishes the fresh string's sources, so help-echo and keymaps
    // come from this evaluation and need not match.
    let mut eval = Context::new();
    let first = fingerprint(
        &mut eval,
        r#"(propertize "buf.el" 'help-echo "one" 'local-map (list 'keymap (cons 'mouse-1 'ignore)))"#,
    )
    .expect("plain data");
    let second = fingerprint(
        &mut eval,
        r#"(propertize "buf.el" 'help-echo (lambda (w o p) "two") 'local-map (list 'keymap))"#,
    )
    .expect("a closure in help-echo is not encoded");
    assert_eq!(first, second);
    let without = fingerprint(&mut eval, r#"(propertize "buf.el")"#).expect("plain");
    assert_ne!(first, without, "the property names still count");
}

#[test]
fn a_display_property_or_an_opaque_value_refuses_the_memo() {
    let mut eval = Context::new();
    for refused in [
        // A display spec can read variables and evaluate `(when ...)`.
        r#"(propertize "x" 'display '(space :align-to 10))"#,
        r#"(propertize "x" 'face (make-marker))"#,
        r#"(propertize "x" 'mouse-face (lambda () 'highlight))"#,
    ] {
        assert_eq!(fingerprint(&mut eval, refused), None, "{refused}");
    }
    assert!(
        fingerprint(&mut eval, r#"(propertize "x" 'display nil)"#).is_some(),
        "a nil display property displays the text"
    );
}

#[test]
fn a_circular_property_value_is_refused_not_walked_forever() {
    let mut eval = Context::new();
    assert_eq!(
        fingerprint(
            &mut eval,
            r#"(let ((l (list 1 2))) (setcdr (cdr l) l) (propertize "x" 'face l))"#,
        ),
        None
    );
}

#[test]
fn a_string_that_holds_itself_in_a_property_is_refused() {
    let mut eval = Context::new();
    assert_eq!(
        fingerprint(
            &mut eval,
            r#"(let ((s (copy-sequence "x"))) (put-text-property 0 1 'my-prop s s) s)"#,
        ),
        None
    );
}

#[test]
fn strings_inside_property_values_carry_their_own_properties() {
    let mut eval = Context::new();
    let bold = fingerprint(
        &mut eval,
        r#"(propertize "x" 'my-prop (propertize "y" 'face 'bold))"#,
    )
    .expect("plain data");
    let italic = fingerprint(
        &mut eval,
        r#"(propertize "x" 'my-prop (propertize "y" 'face 'italic))"#,
    )
    .expect("plain data");
    assert_ne!(bold, italic);
}

#[test]
fn a_chrome_value_that_is_not_a_string_is_not_memoized() {
    assert!(
        ChromeRowFingerprint::capture(
            inputs(),
            &ModeLineDisplayOutput::from_root_string(Value::fixnum(1))
        )
        .is_none()
    );
}

#[test]
fn symbol_values_encode_in_name_order() {
    let a = encode_symbol_values([("b", Value::fixnum(2)), ("a", Value::fixnum(1))]);
    let b = encode_symbol_values([("a", Value::fixnum(1)), ("b", Value::fixnum(2))]);
    assert_eq!(a, b);
    assert_ne!(a, encode_symbol_values([("a", Value::fixnum(1))]));
}

#[test]
fn the_fingerprint_record_keeps_one_row_per_kind() {
    reset_fingerprint_record();
    let mut eval = Context::new();
    let first = fingerprint(&mut eval, PLAIN).expect("plain");
    record_fingerprint(7, WindowChromeKind::ModeLine, Some(first.clone()));
    record_fingerprint(7, WindowChromeKind::ModeLine, Some(first.clone()));
    assert_eq!(recorded_fingerprints(7).map(|rows| rows.len()), Some(1));
    record_fingerprint(7, WindowChromeKind::ModeLine, None);
    assert!(
        recorded_fingerprints(7).is_none(),
        "an unmemoizable row forgets the old one"
    );
    reset_fingerprint_record();
}
