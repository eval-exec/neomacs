//! Actual immutable Float constants prove contagion without seed speculation.
use super::*;
#[path = "opt_sink_numeric_fixture.rs"]
mod fixture;
use fixture::*;

#[test]
fn opt_sink_numeric_static_float_contagion_preserves_identity_and_integer_seed_paths() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let roots = Roots::enter();
    let f = program(
        vec![
            Op::StackRef(0),
            Op::Constant(0),
            Op::Mul,
            Op::Dup,
            Op::Mul,
            Op::Return,
        ],
        vec![Value::make_float(-0.0)],
        1,
    );
    let cases = [0.0, -0.0, 3.0, f64::NAN, f64::INFINITY]
        .into_iter()
        .map(|x| vec![Value::make_float(x)])
        .chain([
            vec![Value::make_int(3)],
            vec![Value::make_int(Value::MOST_POSITIVE_FIXNUM)],
            vec![Value::symbol("not-a-number")],
        ])
        .collect::<Vec<_>>();
    roots.add(&f.constants);
    // Train only the successful numeric path. Other feedback is deliberately
    // sticky; an error before either snapshot would leave the first Mul opaque
    // and fail to exercise the actual Const-origin proof at all.
    for args in &cases {
        roots.add(args);
        if !args[0].is_fixnum() && !args[0].is_float() {
            continue;
        }
        let result = tier0(&mut ctx, &f, args);
        if let Ok(value) = result {
            roots.add(&[value]);
        }
    }
    let (base, _) = compile(&ctx, &f, false, &roots);
    let (selected, clif) = compile(&ctx, &f, true, &roots);
    // Both leaves use actual successful-path observations. Every error/extreme
    // case is still independently checked in both native arms before quality,
    // including after the runtime feedback has legitimately widened to Other.
    check_semantics(&mut ctx, &f, &base, &cases, &roots);
    check_semantics(&mut ctx, &f, &selected, &cases, &roots);
    let args = [Value::make_int(3)];
    let a = native_result(&mut ctx, &f, &selected, &args).unwrap();
    roots.add(&[a]);
    let b = native_result(&mut ctx, &f, &selected, &args).unwrap();
    roots.add(&[b]);
    assert_ne!(
        a.bits(),
        b.bits(),
        "fresh results keep distinct GNU identities"
    );
    assert_ne!(
        a.bits(),
        f.constants[0].bits(),
        "constant Float keeps its own identity"
    );
    assert_eq!(
        crate::emacs_core::jit::compile::opt_census::snapshot(&selected.obs)
            .opt_sink
            .unwrap()
            .numeric_sources,
        2,
        "both real Mul sources must be selected before constant contagion is tested"
    );
    assert!(
        !has_integer_multiply_arm(&clif),
        "real nonprefix Float constant and its successful result exclude both-fix Mul arms: {clif}"
    );
}
