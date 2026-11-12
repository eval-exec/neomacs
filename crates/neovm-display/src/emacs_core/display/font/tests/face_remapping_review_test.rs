use super::*;
use neovm_eval::emacs_core::eval::{
    FontListError, FontListFilter, FontSpecListQuery, ListedFontEntity, ResolvedFrameFont,
};

struct FaceRequestHost(Rc<RefCell<Vec<FrameFontSize>>>);

impl DisplayHost for FaceRequestHost {
    fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resolve_frame_font(
        &mut self,
        _: FrameId,
        request: FrameFontRequest,
    ) -> Result<Option<ResolvedFrameFont>, String> {
        self.0.borrow_mut().push(request.size());
        Ok(None)
    }

    fn list_fonts_for_spec(
        &mut self,
        _: FontSpecListQuery,
        _: Option<FontSpecListQuery>,
        _: Option<usize>,
        _: Option<&mut FontListFilter<'_>>,
    ) -> Result<Vec<ListedFontEntity>, FontListError> {
        Ok(Vec::new())
    }
}

fn context() -> (Context, FrameId, FrameId, Rc<RefCell<Vec<FrameFontSize>>>) {
    let (mut eval, gui, tty, _) = mixed_font_listing_context();
    eval.frame_manager_mut().select_frame(gui);
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("height"),
            Value::fixnum(130),
            Value::make_frame(gui.0),
        ],
    )
    .unwrap();
    let requests = Rc::new(RefCell::new(Vec::new()));
    eval.set_display_host(Box::new(FaceRequestHost(Rc::clone(&requests))));
    (eval, gui, tty, requests)
}

fn remap(eval: &mut Context, enabled: bool) {
    let value = if enabled {
        Value::list(vec![Value::list(vec![
            Value::symbol("default"),
            Value::list(vec![Value::keyword("height"), Value::make_float(2.0)]),
            Value::symbol("default"),
        ])])
    } else {
        Value::NIL
    };
    eval.buffer_manager_mut()
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("face-remapping-alist", value);
}

fn query(eval: &mut Context, face: Value, frame: Value) {
    x_list_fonts(
        eval,
        vec![Value::string("Hack-13"), face, frame, Value::fixnum(1)],
    )
    .unwrap();
}

#[test]
fn face_remapping_changes_x_list_fonts_metrics_and_restores_current_buffer_state() {
    let (mut eval, _, _, requests) = context();
    query(&mut eval, Value::symbol("default"), Value::NIL);
    remap(&mut eval, true);
    query(&mut eval, Value::symbol("default"), Value::NIL);
    remap(&mut eval, false);
    query(&mut eval, Value::symbol("default"), Value::NIL);
    assert_eq!(
        *requests.borrow(),
        vec![
            FrameFontSize::points(13.0).unwrap(),
            FrameFontSize::points(26.0).unwrap(),
            FrameFontSize::points(13.0).unwrap()
        ]
    );
}

#[test]
fn face_remapping_preserves_invalid_face_fallback_and_nil_or_tty_face_clearing() {
    let (mut eval, _, tty, requests) = context();
    remap(&mut eval, true);
    query(&mut eval, Value::vector(Vec::new()), Value::NIL);
    query(&mut eval, Value::NIL, Value::NIL);
    query(
        &mut eval,
        Value::symbol("default"),
        Value::make_frame(tty.0),
    );
    assert_eq!(
        *requests.borrow(),
        vec![FrameFontSize::points(13.0).unwrap()]
    );
}
