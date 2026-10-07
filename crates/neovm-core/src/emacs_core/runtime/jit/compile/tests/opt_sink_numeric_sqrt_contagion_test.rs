//! Actual builtin sqrt proof feeds Float contagion; exceptional calls replay.
use super::*;
#[path = "opt_sink_numeric_fixture.rs"]
mod fixture;
use fixture::*;

#[test]
fn opt_sink_numeric_sqrt_contagion_keeps_replay_and_uses_proven_float_paths() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let _ = ctx.debug_on_next_call_is_armed();
    let roots = Roots::enter();
    let f = program(
        vec![
            Op::Constant(0),
            Op::StackRef(1),
            Op::Call(1),
            Op::Dup,
            Op::Constant(1),
            Op::Mul,
            Op::Add,
            Op::Return,
        ],
        vec![Value::symbol("sqrt"), Value::make_int(2)],
        1,
    );
    // Same signed-zero, nonfinite, wrong-type and positive argument groups as
    // the committed GNU19 sqrt oracle; whole-program answers come from Tier0.
    let cases = [
        4.0,
        1.0e-300,
        1.0e300,
        0.0,
        -0.0,
        -4.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ]
    .into_iter()
    .map(|x| vec![Value::make_float(x)])
    .chain([
        vec![Value::make_int(4)],
        vec![Value::make_int(0)],
        vec![Value::symbol("not-a-number")],
    ])
    .collect::<Vec<_>>();
    roots.add(&f.constants);
    for args in &cases {
        roots.add(args);
        let result = tier0(&mut ctx, &f, args);
        if let Ok(value) = result {
            roots.add(&[value]);
        }
    }
    let (base, _) = compile(&ctx, &f, false, &roots);
    check_semantics(&mut ctx, &f, &base, &cases, &roots);
    let (selected, clif) = compile(&ctx, &f, true, &roots);
    check_semantics(&mut ctx, &f, &selected, &cases, &roots);
    assert!(
        has_sqrt(&clif),
        "actual guarded SourceSqrt reached native lowering"
    );
    assert!(
        !has_integer_multiply_arm(&clif),
        "successful SourceSqrt is Float proof for its downstream Mul/Add, independent of feedback: {clif}"
    );
}
