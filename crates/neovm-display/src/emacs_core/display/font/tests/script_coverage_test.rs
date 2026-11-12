//! Fresh-query transport, not a model of GNU's sticky live-frame entity cache.
use super::*;
use neovm_eval::emacs_core::eval::{FontScriptCoverage as Coverage, FontSpecListQuery};
type Requests = Rc<RefCell<Vec<(FontSpecListQuery, Option<FontSpecListQuery>)>>>;
struct RecordingHost(Requests);
impl DisplayHost for RecordingHost {
    fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn list_fonts_for_spec(
        &mut self,
        query: FontSpecListQuery,
        prefer: Option<FontSpecListQuery>,
        _: Option<usize>,
        _: Option<&mut neovm_eval::emacs_core::eval::FontListFilter<'_>>,
    ) -> Result<
        Vec<neovm_eval::emacs_core::eval::ListedFontEntity>,
        neovm_eval::emacs_core::eval::FontListError,
    > {
        self.0.borrow_mut().push((query, prefer));
        Ok(Vec::new())
    }
}
fn context() -> (Context, Requests) {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    ensure_selected_gui_frame(&mut eval);
    let requests = Rc::new(RefCell::new(Vec::new()));
    eval.set_display_host(Box::new(RecordingHost(Rc::clone(&requests))));
    (eval, requests)
}
fn spec(script: Option<Value>) -> Value {
    let mut fields = vec![Value::keyword("family"), Value::string("Hack")];
    if let Some(script) = script {
        fields.extend([Value::keyword("script"), script]);
    }
    font_spec(fields).unwrap()
}
fn table(eval: &mut Context, script: Value, representatives: Value) {
    eval.set_variable(
        "script-representative-chars",
        Value::list(vec![Value::cons(script, representatives)]),
    );
}
fn capture(eval: &mut Context, requests: &Requests, spec: Value) -> Option<Coverage> {
    list_fonts(eval, vec![spec]).unwrap();
    requests.borrow().last().unwrap().0.script_coverage.clone()
}

#[test]
fn script_capture_preserves_list_and_vector_quantifiers_and_character_domains() {
    for case in 0..7 {
        let (mut eval, requests) = context();
        let script = Value::symbol("fresh-script");
        let (representatives, expected) = match case {
            0 => (
                Value::list(vec![Value::fixnum(65), Value::fixnum(0x4e00)]),
                Some(Coverage::All(vec![65, 0x4e00])),
            ),
            1 => (
                Value::vector(vec![Value::fixnum(65), Value::fixnum(0x4e00)]),
                Some(Coverage::Any(vec![65, 0x4e00])),
            ),
            2 => (Value::vector(Vec::new()), Some(Coverage::Any(Vec::new()))),
            3 => (Value::NIL, None),
            4 => (
                Value::list(vec![
                    Value::fixnum(0x400000),
                    Value::fixnum(65),
                    Value::symbol("ignored"),
                    Value::fixnum(0x3fffff),
                ]),
                Some(Coverage::All(vec![65, 0x3fffff])),
            ),
            5 => (
                Value::vector(vec![
                    Value::fixnum(-1),
                    Value::fixnum(u32::MAX as i64),
                    Value::fixnum(u32::MAX as i64 + 1),
                    Value::symbol("ignored"),
                    Value::fixnum(0x110000),
                    Value::fixnum(65),
                ]),
                Some(Coverage::Any(vec![u32::MAX, 0x110000, 65])),
            ),
            _ => (
                Value::cons(Value::fixnum(65), Value::symbol("improper-tail")),
                Some(Coverage::All(vec![65])),
            ),
        };
        table(&mut eval, script, representatives);
        assert_eq!(
            capture(&mut eval, &requests, spec(Some(script))),
            expected,
            "case {case}"
        );
    }
}

#[test]
fn script_capture_uses_association_identity_and_the_first_entry() {
    for same_identity in [false, true] {
        let (mut eval, requests) = context();
        let uninterned = Value::from_sym_id(neovm_value::intern::intern_uninterned("han"));
        let key = if same_identity {
            uninterned
        } else {
            Value::symbol("han")
        };
        table(&mut eval, key, Value::vector(vec![Value::fixnum(0x4e00)]));
        assert_eq!(
            capture(&mut eval, &requests, spec(Some(uninterned))),
            same_identity.then(|| Coverage::Any(vec![0x4e00]))
        );
    }
    let (mut eval, requests) = context();
    let script = Value::symbol("first-entry");
    eval.set_variable(
        "script-representative-chars",
        Value::list(vec![
            Value::cons(script, Value::NIL),
            Value::cons(script, Value::vector(vec![Value::fixnum(0x4e00)])),
        ]),
    );
    assert_eq!(capture(&mut eval, &requests, spec(Some(script))), None);
}

#[test]
fn script_capture_distinguishes_an_absent_property_from_explicit_nil() {
    for case in 0..3 {
        let (mut eval, requests) = context();
        let representatives = if case == 2 {
            Value::list(vec![Value::fixnum(0x4e00)])
        } else {
            Value::vector(vec![Value::fixnum(0x4e00)])
        };
        table(&mut eval, Value::NIL, representatives);
        let script = if case == 0 { None } else { Some(Value::NIL) };
        assert_eq!(
            capture(&mut eval, &requests, spec(script)),
            if case == 1 {
                Some(Coverage::Any(vec![0x4e00]))
            } else {
                None
            }
        );
    }
}

#[test]
fn script_capture_owns_codepoints_and_does_not_filter_by_preference() {
    let (mut eval, requests) = context();
    let script = Value::symbol("owned-vector");
    let representatives = Value::vector(vec![Value::fixnum(65)]);
    table(&mut eval, script, representatives);
    let primary = spec(Some(script));
    list_fonts(
        &mut eval,
        vec![primary, Value::NIL, Value::fixnum(1), primary],
    )
    .unwrap();
    assert_eq!(
        requests.borrow()[0].0.script_coverage,
        Some(Coverage::Any(vec![65]))
    );
    assert_eq!(
        requests.borrow()[0].1.as_ref().unwrap().script_coverage,
        None
    );
    assert!(representatives.replace_vector_data(vec![Value::fixnum(0x4e00)]));
    assert_eq!(
        requests.borrow()[0].0.script_coverage,
        Some(Coverage::Any(vec![65])),
        "host request owns its snapshot; this does not assert GNU cache invalidation"
    );
}

#[test]
fn script_capture_preserves_early_num_and_tty_validation_without_host_requests() {
    let (mut eval, requests) = context();
    let gui = eval.frame_manager().selected_frame().unwrap().id;
    let buffer = eval.buffer_manager_mut().create_buffer("*script-tty*");
    let tty = eval
        .frame_manager_mut()
        .create_frame_on_terminal("script-tty", 1, 80, 25, buffer);
    eval.frame_manager_mut()
        .get_mut(tty)
        .unwrap()
        .set_window_system(None);
    eval.set_variable("script-representative-chars", Value::fixnum(42));
    let primary = spec(Some(Value::symbol("malformed-table")));
    assert!(
        list_fonts(
            &mut eval,
            vec![
                primary,
                Value::make_frame(gui.0),
                Value::fixnum(0),
                Value::T
            ]
        )
        .unwrap()
        .is_nil()
    );
    assert!(
        list_fonts(&mut eval, vec![primary, Value::make_frame(tty.0)])
            .unwrap()
            .is_nil()
    );
    assert!(requests.borrow().is_empty());
}
