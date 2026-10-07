//! Full precise frames precede cold-code specialization assertions.
use super::*;
#[path = "opt_sink_numeric_fixture.rs"]
mod fixture;
use fixture::*;

#[test]
fn opt_sink_numeric_fresh_cold_materialization_keeps_exact_point_aliases_without_generic_cache_diamonds()
 {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let roots = Roots::enter();
    let f = program(
        vec![
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Mul,
            Op::Dup,
            Op::StackRef(2),
            Op::Add,
            Op::Return,
        ],
        Vec::new(),
        3,
    );
    let cases = vec![
        vec![
            Value::make_float(2.0),
            Value::make_float(3.0),
            Value::make_float(4.0),
        ],
        vec![Value::make_int(2), Value::make_int(3), Value::make_int(4)],
        vec![
            Value::make_float(-0.0),
            Value::make_float(3.0),
            Value::make_float(-0.0),
        ],
    ];
    for args in &cases {
        roots.add(args);
        let result = tier0(&mut ctx, &f, args).unwrap();
        roots.add(&[result]);
    }
    let (base, _) = compile(&ctx, &f, false, &roots);
    check_semantics(&mut ctx, &f, &base, &cases, &roots);
    let (selected, clif) = compile(&ctx, &f, true, &roots);
    check_semantics(&mut ctx, &f, &selected, &cases, &roots);
    let bad = [
        Value::make_float(2.0),
        Value::make_float(3.0),
        Value::string("not-a-number"),
    ];
    roots.add(&bad);
    let expected = tier0(&mut ctx, &f, &bad);
    let NativeRun::DeoptAt(exit) = selected.call_consts(
        &mut ctx as *mut Context as *mut u8,
        f.constants.as_ptr(),
        &bad,
    ) else {
        panic!("wrong right operand must leave at its exact original Add");
    };
    assert_eq!(exit.pc, 5);
    assert_eq!(exit.stack.len(), 6);
    for slot in 0..3 {
        assert_eq!(exit.stack[slot].bits(), bad[slot].bits());
    }
    assert_eq!(
        exit.stack[3].bits(),
        exit.stack[4].bits(),
        "two frame aliases reconstruct one object"
    );
    assert_ne!(exit.stack[3].bits(), bad[0].bits());
    assert_ne!(exit.stack[3].bits(), bad[1].bits());
    assert_eq!(exit.stack[5].bits(), bad[2].bits());
    roots.add(&exit.stack);
    let prefix = program(
        vec![Op::StackRef(1), Op::StackRef(1), Op::Mul, Op::Return],
        Vec::new(),
        2,
    );
    let product = tier0(&mut ctx, &prefix, &bad[..2]).unwrap();
    roots.add(&[product]);
    assert_eq!(exit.stack[3].xfloat().to_bits(), product.xfloat().to_bits());
    let resumed = resume(&mut ctx, &f, &exit);
    assert_eq!(observe(&resumed), observe(&expected));
    assert_eq!(ctx.jit_root_stack_top, 0);
    // This source has exactly two virtual slots at its one post-Mul numeric
    // frame. Capturing that same point twice leaves duplicate unused NIL
    // placeholders in CLIF; no frame slot or identity is removed by reuse.
    assert!(
        unused_snapshot_word_nil_constants(&clif) <= 2,
        "one exact-point frame capture must not emit a discarded duplicate snapshot: {clif}"
    );
    assert_eq!(
        generic_fresh_cache_diamonds(&clif),
        0,
        "fresh ready source recipes need their shared boxer, not borrowed/cache dispatch: {clif}"
    );
}
