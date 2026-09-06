use super::*;
use neovm_core::buffer::EmacsByteRange;

fn read(eval: &mut Context, src: &str) -> Value {
    eval.eval_str(&format!("'{src}")).expect("read")
}

#[test]
fn holds_takes_nil_and_t_literally_and_falls_back_to_structural() {
    let mut eval = Context::new();
    let call = read(&mut eval, "(some-call)");
    let structural = DisplayWhenConditions::structural();
    assert!(!structural.holds(Value::NIL));
    assert!(structural.holds(Value::symbol("t")));
    assert!(
        structural.holds(call),
        "no evaluation behind it: non-nil holds"
    );

    let mut results = FxHashMap::default();
    results.insert(call, false);
    let evaluated = DisplayWhenConditions::evaluated(results);
    assert!(!evaluated.holds(call));
    assert!(evaluated.holds(read(&mut eval, "(unseen-call)")));
}

/// The FORM of the first (or `index`th) `when` spec of a display value.
fn when_form(value: Value, index: usize) -> Value {
    let mut forms = Vec::new();
    DisplayPropertySpecs::of(value).for_each(|spec| {
        if let Some((form, _)) = display_spec_when_parts(spec) {
            forms.push(form);
        }
        std::ops::ControlFlow::Continue(())
    });
    forms[index]
}

/// The FORM of the `when` spec on a prefix string's first character.
fn prefix_when_form(string: Value) -> Value {
    let props = get_string_text_properties_table_for_value(string).expect("string props");
    when_form(
        props
            .get_property_at_char_pos(CharPos0::new(0), Value::symbol("display"))
            .expect("display on the prefix"),
        0,
    )
}

#[test]
fn window_span_forms_are_evaluated_from_text_props_overlays_and_prefix_vars() {
    let mut eval = Context::new();
    let buf_id = eval.buffers.current_buffer_id().expect("buffer");
    // A `display` spec list on the text, dashboard-style: the two clauses
    // disagree, and only evaluation can tell them apart.
    let text_display = read(
        &mut eval,
        "((when (= 1 1) space :align-to 10) (when (= 1 2) space :align-to 20))",
    );
    // A prefix string whose own `display` property carries a form that reads
    // the bindings GNU provides.
    let prefix = eval
        .eval_str("(propertize \" \" 'display '(when (and (stringp object) (= position 0) (= buffer-position 1)) space :align-to 5))")
        .expect("prefix");
    {
        let buffer = eval.buffers.get_mut(buf_id).expect("buffer");
        buffer.insert("hello world\n");
        let start = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(0));
        let mid = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(5));
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(start, mid),
            Value::symbol("display"),
            text_display,
        );
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(start, mid),
            Value::symbol("line-prefix"),
            prefix,
        );
    }
    let overlay_display = eval
        .eval_str("(let ((o (make-overlay 7 9))) (overlay-put o 'display '(when (eq 'a 'a) . \"OVERLAY\")) (overlay-get o 'display))")
        .expect("overlay");
    let local_prefix = eval
        .eval_str("(set (make-local-variable 'wrap-prefix) (propertize \" \" 'display '(when (car 1) space :align-to 9)))")
        .expect("local");

    let conditions = evaluate_window_display_when_forms(
        &mut eval,
        buf_id,
        None,
        CharPos0::new(0),
        CharPos0::new(12),
    );
    let holds = |form: Value| match conditions.verdict(form) {
        DisplayWhenVerdict::Holds => Some(true),
        DisplayWhenVerdict::Fails => Some(false),
        DisplayWhenVerdict::Unseen => None,
    };
    assert_eq!(holds(when_form(text_display, 0)), Some(true));
    assert_eq!(holds(when_form(text_display, 1)), Some(false));
    assert_eq!(
        holds(prefix_when_form(prefix)),
        Some(true),
        "object/position/buffer-position are bound for a prefix string"
    );
    assert_eq!(
        holds(when_form(overlay_display, 0)),
        Some(true),
        "overlay display specs are scanned"
    );
    assert_eq!(
        holds(prefix_when_form(local_prefix)),
        Some(false),
        "an erroring form is nil (dsafe_eval); buffer-local wrap-prefix is scanned"
    );
    assert!(conditions.holds(when_form(text_display, 0)));
    assert!(!conditions.holds(when_form(text_display, 1)));
}

#[test]
fn a_disable_eval_wrapper_makes_its_when_forms_fail_like_gnu() {
    // GNU: `if (!NILP (form) && !EQ (form, Qt) && !enable_eval_p) form = Qnil;`
    // (src/xdisp.c:6139-6140), so a true FORM under `(disable-eval …)` still
    // disables the spec.
    let mut eval = Context::new();
    let buf_id = eval.buffers.current_buffer_id().expect("buffer");
    let disabled = read(&mut eval, "(disable-eval (when (= 1 1) . \"HIDDEN\"))");
    {
        let buffer = eval.buffers.get_mut(buf_id).expect("buffer");
        buffer.insert("abc\n");
        let start = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(0));
        let end = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(2));
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(start, end),
            Value::symbol("display"),
            disabled,
        );
    }
    let conditions = evaluate_window_display_when_forms(
        &mut eval,
        buf_id,
        None,
        CharPos0::new(0),
        CharPos0::new(4),
    );
    let form = when_form(disabled, 0);
    assert_eq!(conditions.verdict(form), DisplayWhenVerdict::Fails);
    assert!(!conditions.holds(form));
}

#[test]
fn forms_are_evaluated_with_the_window_buffer_current() {
    // GNU selects the window's buffer before the iterator runs
    // (src/xdisp.c:20533-20535); a FORM reading a buffer-local sees it.
    let mut eval = Context::new();
    let original = eval.buffers.current_buffer_id().expect("buffer");
    let other = eval.buffers.create_buffer("other");
    let spec = read(
        &mut eval,
        "((when (string-equal (buffer-name) \"other\") space :align-to 3))",
    );
    {
        let buffer = eval.buffers.get_mut(other).expect("buffer");
        buffer.insert("xyz\n");
        let start = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(0));
        let end = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(1));
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(start, end),
            Value::symbol("display"),
            spec,
        );
    }
    let conditions = evaluate_window_display_when_forms(
        &mut eval,
        other,
        None,
        CharPos0::new(0),
        CharPos0::new(4),
    );
    assert_eq!(
        conditions.verdict(when_form(spec, 0)),
        DisplayWhenVerdict::Holds
    );
    assert_eq!(
        eval.buffers.current_buffer_id(),
        Some(original),
        "caller's buffer restored"
    );
}

#[test]
fn an_after_string_binds_buffer_position_to_the_overlay_end() {
    // GNU loads an after-string when the iterator reaches the overlay's end
    // (xdisp.c:7172-7173) and binds `buffer-position` to that position
    // (xdisp.c:5919-5923); a before-string is bound to the start.
    let mut eval = Context::new();
    let buf_id = eval.buffers.current_buffer_id().expect("buffer");
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("abcdefgh\n");
    let after = eval
        .eval_str(
            "(let ((o (make-overlay 2 6)))
               (overlay-put o 'after-string
                 (propertize \" \" 'display '(when (= buffer-position 6) space :align-to 20)))
               (overlay-put o 'before-string
                 (propertize \" \" 'display '(when (= buffer-position 2) space :align-to 3)))
               (list (overlay-get o 'after-string) (overlay-get o 'before-string)))",
        )
        .expect("overlay");
    let after_string = after.cons_car();
    let before_string = after.cons_cdr().cons_car();
    let conditions = evaluate_window_display_when_forms(
        &mut eval,
        buf_id,
        None,
        CharPos0::new(0),
        CharPos0::new(9),
    );
    assert_eq!(
        conditions.verdict(prefix_when_form(after_string)),
        DisplayWhenVerdict::Holds
    );
    assert_eq!(
        conditions.verdict(prefix_when_form(before_string)),
        DisplayWhenVerdict::Holds
    );
}

#[test]
fn an_overlay_scoped_to_another_window_is_not_scanned() {
    // GNU: "Skip this overlay if it doesn't apply to IT->w" (xdisp.c:7153-7156).
    let mut eval = Context::new();
    let buf_id = eval.buffers.current_buffer_id().expect("buffer");
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("abcdefgh\n");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("scoped-overlay", 640, 200, buf_id);
    assert!(eval.frame_manager_mut().select_frame(frame_id));
    let display = eval
        .eval_str(
            "(let ((o (make-overlay 2 6)))
               (overlay-put o 'window (selected-window))
               (overlay-put o 'display '(when (= 1 1) . \"OTHER\"))
               (overlay-get o 'display))",
        )
        .expect("overlay");
    // Scanning for a window that is not the overlay's leaves its form unseen…
    let conditions = evaluate_window_display_when_forms(
        &mut eval,
        buf_id,
        Some(u64::MAX),
        CharPos0::new(0),
        CharPos0::new(9),
    );
    assert_eq!(
        conditions.verdict(when_form(display, 0)),
        DisplayWhenVerdict::Unseen
    );
    // …while the overlay's own window sees it evaluated.
    let selected = eval
        .eval_str("(selected-window)")
        .expect("selected window")
        .as_window_id()
        .expect("window id");
    let conditions = evaluate_window_display_when_forms(
        &mut eval,
        buf_id,
        Some(selected),
        CharPos0::new(0),
        CharPos0::new(9),
    );
    assert_eq!(
        conditions.verdict(when_form(display, 0)),
        DisplayWhenVerdict::Holds
    );
}

#[test]
fn a_hidden_overlay_shows_its_after_string_at_the_start_the_walk_reaches() {
    // GNU: "If the text ``under'' the overlay is invisible, both before- and
    // after-strings from this overlay are visible; start and end position are
    // indistinguishable" (xdisp.c:7158-7175): the after-string loads at the
    // start, so `buffer-position` is the start there.
    let mut eval = Context::new();
    let buf_id = eval.buffers.current_buffer_id().expect("buffer");
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("abcdefgh\n");
    let after = eval
        .eval_str(
            "(let ((o (make-overlay 2 6)))
               (overlay-put o 'invisible t)
               (overlay-put o 'after-string
                 (propertize \" \" 'display '(when (= buffer-position 2) space :align-to 20)))
               (overlay-get o 'after-string))",
        )
        .expect("overlay");
    let conditions = evaluate_window_display_when_forms(
        &mut eval,
        buf_id,
        None,
        CharPos0::new(0),
        CharPos0::new(9),
    );
    assert_eq!(
        conditions.verdict(prefix_when_form(after)),
        DisplayWhenVerdict::Holds
    );
}

#[test]
fn an_after_string_ending_at_the_window_start_is_scanned_and_one_beyond_the_span_is_not() {
    // GNU loads the strings of overlays that start or end at the iterator
    // position (xdisp.c:7141-7150), so an overlay ending exactly at the window
    // start still shows its after-string on the first row; one ending past the
    // span is never reached by this walk.
    let mut eval = Context::new();
    let buf_id = eval.buffers.current_buffer_id().expect("buffer");
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("abcdefghij\n");
    let strings = eval
        .eval_str(
            "(let ((a (make-overlay 1 5)) (b (make-overlay 6 11)))
               (overlay-put a 'after-string
                 (propertize \" \" 'display '(when (= 1 2) space :align-to 20)))
               (overlay-put b 'after-string
                 (propertize \" \" 'display '(when (= 1 1) space :align-to 20)))
               (list (overlay-get a 'after-string) (overlay-get b 'after-string)))",
        )
        .expect("overlays");
    let at_start = strings.cons_car();
    let beyond = strings.cons_cdr().cons_car();
    // Window starts at char 4 (GNU position 5, where overlay `a` ends) and
    // spans four characters, ending at char 8; overlay `b` ends at char 10,
    // past the span (a boundary at the span end itself is reached, see the
    // next test).
    let conditions = evaluate_window_display_when_forms(
        &mut eval,
        buf_id,
        None,
        CharPos0::new(4),
        CharPos0::new(8),
    );
    assert_eq!(
        conditions.verdict(prefix_when_form(at_start)),
        DisplayWhenVerdict::Fails,
        "the after-string at the window start is evaluated (its form is nil)"
    );
    assert_eq!(
        conditions.verdict(prefix_when_form(beyond)),
        DisplayWhenVerdict::Unseen,
        "an after-string displayed past the span is not reached"
    );
}

#[test]
fn overlay_strings_anchored_at_point_max_are_evaluated_when_the_span_reaches_it() {
    // A completion popup's empty overlay at point-max carries its candidates
    // in `before-string`; the walk displays them through its end-of-buffer
    // anchor path (GNU `reseat` -> `handle_stop` at ZV, xdisp.c:8046-8052),
    // so their forms must be evaluated too.
    let mut eval = Context::new();
    let buf_id = eval.buffers.current_buffer_id().expect("buffer");
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("abc\n");
    let strings = eval
        .eval_str(
            "(let ((empty (make-overlay 5 5)) (ending (make-overlay 2 5)))
               (overlay-put empty 'before-string
                 (propertize \" \" 'display '(when (= 1 2) space :align-to 20)))
               (overlay-put ending 'after-string
                 (propertize \" \" 'display '(when (= buffer-position 5) space :align-to 30)))
               (list (overlay-get empty 'before-string) (overlay-get ending 'after-string)))",
        )
        .expect("overlays");
    let empty_before = strings.cons_car();
    let ending_after = strings.cons_cdr().cons_car();
    let point_max = eval
        .buffers
        .get(buf_id)
        .expect("buffer")
        .layout_point_max_char_pos();
    let conditions =
        evaluate_window_display_when_forms(&mut eval, buf_id, None, CharPos0::new(0), point_max);
    assert_eq!(
        conditions.verdict(prefix_when_form(empty_before)),
        DisplayWhenVerdict::Fails,
        "the empty overlay's before-string at point-max is evaluated"
    );
    assert_eq!(
        conditions.verdict(prefix_when_form(ending_after)),
        DisplayWhenVerdict::Holds,
        "the after-string of an overlay ending at point-max is bound there"
    );
}

#[test]
fn an_empty_overlay_supplies_no_display_property() {
    // GNU `get_char_property_and_overlay` skips an overlay whose end is at
    // or before the position (textprop.c:652-653): an empty overlay's
    // `display` never applies, so its form is not evaluated.
    let mut eval = Context::new();
    let buf_id = eval.buffers.current_buffer_id().expect("buffer");
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("abcdef\n");
    let display = eval
        .eval_str(
            "(let ((o (make-overlay 3 3)))
               (overlay-put o 'display '(when (= 1 1) . \"EMPTY\"))
               (overlay-get o 'display))",
        )
        .expect("overlay");
    let conditions = evaluate_window_display_when_forms(
        &mut eval,
        buf_id,
        None,
        CharPos0::new(0),
        CharPos0::new(7),
    );
    assert_eq!(
        conditions.verdict(when_form(display, 0)),
        DisplayWhenVerdict::Unseen
    );
}
