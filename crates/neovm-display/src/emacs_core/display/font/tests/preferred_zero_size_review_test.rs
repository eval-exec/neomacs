use super::*;
use neovm_eval::emacs_core::eval::{
    FontListError, FontListFilter, FontOpeningSize, FontSpecListQuery, ListedFontEntity,
};

type Requests = Rc<RefCell<Vec<(FontSpecListQuery, Option<FontSpecListQuery>)>>>;
struct SizeHost(Requests);
impl DisplayHost for SizeHost {
    fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn list_fonts_for_spec(
        &mut self,
        primary: FontSpecListQuery,
        prefer: Option<FontSpecListQuery>,
        _: Option<usize>,
        _: Option<&mut FontListFilter<'_>>,
    ) -> Result<Vec<ListedFontEntity>, FontListError> {
        self.0.borrow_mut().push((primary, prefer));
        Ok(Vec::new())
    }
}

fn context() -> (Context, Requests) {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    ensure_selected_gui_frame(&mut eval);
    let requests = Rc::new(RefCell::new(Vec::new()));
    eval.set_display_host(Box::new(SizeHost(Rc::clone(&requests))));
    (eval, requests)
}

#[test]
fn preferred_zero_size_preserves_integer_and_float_presence_at_the_host_boundary() {
    let mut sizes = Vec::new();
    for case in 0..3 {
        let (mut eval, requests) = context();
        let value = match case {
            0 => Value::fixnum(0),
            1 => Value::make_float(0.0),
            _ => Value::make_float(-0.0),
        };
        let primary = font_spec(Vec::new()).unwrap();
        let preferred = font_spec(vec![Value::keyword("size"), value]).unwrap();
        list_fonts(&mut eval, vec![primary, Value::NIL, Value::NIL, preferred]).unwrap();
        let requests = requests.borrow();
        assert_eq!(requests[0].0.size, None);
        sizes.push(requests[0].1.as_ref().unwrap().size);
    }
    assert_eq!(sizes, vec![Some(FontOpeningSize::SmallestUsable); 3]);
}

#[test]
fn primary_zero_size_is_distinct_from_an_absent_size_in_the_listing_query() {
    let (mut eval, requests) = context();
    let primary = font_spec(vec![Value::keyword("size"), Value::fixnum(0)]).unwrap();
    list_fonts(&mut eval, vec![primary]).unwrap();
    assert_eq!(
        requests.borrow()[0].0.size,
        Some(FontOpeningSize::SmallestUsable)
    );
}

#[test]
fn float_listing_size_with_explicit_zero_dpi_preserves_computed_zero_pixels() {
    let mut captured = Vec::new();
    for case in 0..4 {
        let (mut eval, requests) = context();
        let size = if case == 3 {
            Value::fixnum(50)
        } else {
            Value::make_float(50.0)
        };
        let dpi_key = if case == 2 {
            Value::symbol("dpi")
        } else {
            Value::keyword("dpi")
        };
        let mut args = vec![Value::keyword("size"), size];
        if case != 1 {
            args.extend([dpi_key, Value::fixnum(0)]);
        }
        let primary = font_spec(args).unwrap();
        list_fonts(&mut eval, vec![primary]).unwrap();
        let request = &requests.borrow()[0].0;
        captured.push((request.size, request.dpi));
    }
    assert_eq!(
        captured[0],
        (Some(FontOpeningSize::SmallestUsable), Some(0))
    );
    assert!(matches!(
        captured[1],
        (Some(FontOpeningSize::Points { dpi: None, .. }), None)
    ));
    assert!(matches!(
        captured[2],
        (Some(FontOpeningSize::Points { dpi: None, .. }), None)
    ));
    assert!(matches!(
        captured[3],
        (Some(FontOpeningSize::Pixels(_)), Some(0))
    ));
}
