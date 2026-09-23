use super::*;

fn classify_number(number: &Retained<NSNumber>) -> FocusProbe {
    classify_focus_probe(
        Retained::as_ptr(number).cast::<AnyObject>().cast_mut(),
        ptr::null_mut(),
    )
}

#[test]
fn only_core_foundation_booleans_are_focus_answers() {
    assert_eq!(
        classify_number(&NSNumber::new_bool(true)),
        FocusProbe::Focused
    );
    assert_eq!(
        classify_number(&NSNumber::new_bool(false)),
        FocusProbe::Unfocused
    );
    assert_eq!(
        classify_number(&NSNumber::new_i32(1)),
        FocusProbe::NotABoolean,
        "numeric one is truthy but is not a JavaScript Boolean"
    );
    assert_eq!(
        classify_number(&NSNumber::new_i32(0)),
        FocusProbe::NotABoolean,
        "numeric zero is falsey but is not a JavaScript Boolean"
    );
}

#[test]
fn objective_c_callback_panics_are_contained() {
    let escaped = std::panic::catch_unwind(|| {
        run_objc_callback(|| panic!("synthetic callback panic"));
    });
    assert!(escaped.is_ok(), "a panic must not unwind into Objective-C");
}
