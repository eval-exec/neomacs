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

    let conditions =
        evaluate_window_display_when_forms(&mut eval, buf_id, CharPos0::new(0), CharPos0::new(12));
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
    let conditions =
        evaluate_window_display_when_forms(&mut eval, buf_id, CharPos0::new(0), CharPos0::new(4));
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
    let conditions =
        evaluate_window_display_when_forms(&mut eval, other, CharPos0::new(0), CharPos0::new(4));
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
