//! GNU frame opacity decoding. Lisp parameters retain their original values.
use crate::emacs_core::error::{Flow, signal};
use crate::emacs_core::{Value, ValueKind};

/// Decode one GNU opacity component: integers are percentages, floats fractions.
pub fn component(value: Value, nil: f32) -> Result<f32, Flow> {
    if value.is_nil() {
        return Ok(nil);
    }
    let (number, maximum) = match value.kind() {
        ValueKind::Fixnum(n) => (n as f64, 100.0),
        ValueKind::Float => (value.as_float().unwrap(), 1.0),
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("numberp"), value],
            ));
        }
    };
    if !(0.0..=maximum).contains(&number) {
        return Err(signal(
            "args-out-of-range",
            vec![
                if maximum == 100.0 {
                    Value::fixnum(0)
                } else {
                    Value::make_float(0.0)
                },
                if maximum == 100.0 {
                    Value::fixnum(100)
                } else {
                    Value::make_float(1.0)
                },
            ],
        ));
    }
    Ok((number / maximum) as f32)
}

/// Active/inactive pair, including GNU's nil (leave opacity unchanged) sentinel.
pub fn pair(mut value: Value) -> Result<[f32; 2], Flow> {
    let mut result = [-1.0; 2];
    for item in &mut result {
        let current = if value.is_cons() {
            let car = value.cons_car();
            value = value.cons_cdr();
            car
        } else {
            value
        };
        *item = component(current, -1.0)?;
    }
    Ok(result)
}

/// Backend lower limit. Unlike `alpha`, `alpha-background` has no lower limit.
pub fn lower_limit(value: Value) -> f32 {
    match value.kind() {
        ValueKind::Fixnum(n) => n as f32 / 100.0,
        ValueKind::Float => value.as_float().unwrap() as f32,
        _ => 1.0,
    }
}

#[cfg(test)]
#[path = "frame_alpha/tests/frame_alpha_test.rs"]
mod tests;
