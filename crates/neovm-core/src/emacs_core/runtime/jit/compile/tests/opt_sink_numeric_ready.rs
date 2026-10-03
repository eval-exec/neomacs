//! Borrowed/ready paths retain GNU semantics before lowering-quality checks.
use super::*;
#[path = "opt_sink_numeric_fixture.rs"]
mod fixture;
use fixture::*;

#[test]
fn opt_sink_numeric_known_ready_and_borrowed_paths_keep_gnu_results_without_readiness_diamonds() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let roots = Roots::enter();
    // A real immutable Float constant forces GNU contagion. The first Add
    // resolves an unknown borrowed argument; Mul consumes its ready result.
    let f = program(
        vec![
            Op::StackRef(0),
            Op::Constant(0),
            Op::Add,
            Op::Dup,
            Op::Mul,
            Op::Return,
        ],
        vec![Value::make_float(1.0)],
        1,
    );
    let cases = [-0.0, 0.0, 2.0, f64::NAN, f64::INFINITY]
        .into_iter()
        .map(|x| vec![Value::make_float(x)])
        .chain([
            vec![Value::make_int(2)],
            vec![Value::make_int(Value::MOST_POSITIVE_FIXNUM)],
            vec![Value::symbol("not-a-number")],
            vec![Value::string("not-a-number")],
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
    assert_eq!(
        readiness_diamonds(&clif),
        0,
        "known Borrow0/source1 must select their proven arm before CFG construction: {clif}"
    );
}
