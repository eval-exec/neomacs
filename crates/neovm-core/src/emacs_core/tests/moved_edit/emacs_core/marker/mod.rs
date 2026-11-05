pub use neovm_edit::emacs_core::marker::*;
#[path = "../editing/marker/tests/marker_test.rs"]
mod tests;

#[cfg(test)]
pub fn builtin_marker_position(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<neovm_value::value::Value>,
) -> neovm_value::error::EvalResult {
    neovm_value::error::expect_args("marker-position", &args, 1)?;
    let arg = |i: usize| {
        args.get(i)
            .copied()
            .unwrap_or(neovm_value::value::Value::NIL)
    };
    neovm_edit::emacs_core::marker::builtin_marker_position_1(&eval.edit.buffers, arg(0))
}
