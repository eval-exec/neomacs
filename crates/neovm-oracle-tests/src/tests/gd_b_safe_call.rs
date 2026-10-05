//! GNU safe_funcall pins through print_error_message's safe substitution
//! callback (print.c:1112, eval.c:3230-3241). Expectations are refreshed
//! exclusively from GNU with NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1.

const ENVS: &[&[(&str, &str)]] = &[
    &[("NEOVM_JIT", "0")],
    &[],
    &[("NEOVM_JIT_THRESHOLD", "1"), ("NEOVM_JIT_BG", "sync")],
];

#[cfg(test)]
#[path = "gd_b_safe_call/qt_barrier.rs"]
mod qt_barrier;

#[cfg(test)]
#[path = "gd_b_safe_call/debugger.rs"]
mod debugger;

#[cfg(test)]
#[path = "gd_b_safe_call/conditions.rs"]
mod conditions;

#[cfg(test)]
#[path = "gd_b_safe_call/binding_scope.rs"]
mod binding_scope;
