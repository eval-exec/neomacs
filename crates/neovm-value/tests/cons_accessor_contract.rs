//! Safe cons accessors must refuse a value that is not a cons. A fixnum's
//! tagged word is not a cons pointer. `black_box` keeps the load alive so
//! an optimizing build cannot delete the unsound read.

use neovm_value::value::Value;

fn refuse_fixnum(read: fn(Value) -> Value) -> Result<(), Box<dyn std::any::Any + Send>> {
    std::panic::catch_unwind(|| {
        let value = std::hint::black_box(Value::fixnum(5));
        let _ = std::hint::black_box(read(value));
    })
}

#[test]
fn cons_car_refuses_a_fixnum() {
    assert!(
        refuse_fixnum(Value::cons_car).is_err(),
        "cons_car accepted a fixnum"
    );
}

#[test]
fn cons_cdr_refuses_a_fixnum() {
    assert!(
        refuse_fixnum(Value::cons_cdr).is_err(),
        "cons_cdr accepted a fixnum"
    );
}

#[test]
fn cons_car_unobserved_refuses_a_fixnum() {
    assert!(
        refuse_fixnum(Value::cons_car_unobserved).is_err(),
        "cons_car_unobserved accepted a fixnum"
    );
}

#[test]
fn cons_cdr_unobserved_refuses_a_fixnum() {
    assert!(
        refuse_fixnum(Value::cons_cdr_unobserved).is_err(),
        "cons_cdr_unobserved accepted a fixnum"
    );
}
