//! TMP-only, unregistered/uncompiled/unexecuted. Original guard frames and
//! semantic observations precede Float-dispatch duplication assertions.
use super::*;
#[path = "opt_sink_numeric_fixture.rs"]
mod fixture;
use fixture::*;

// Count emitted pre-codegen operations, not executed instructions or ASM.
fn float_ops(clif: &str, opcode: &str) -> usize {
    clif.lines()
        .filter(|line| {
            let Some((_, rhs)) = line.split(';').next().unwrap().split_once(" = ") else {
                return false;
            };
            rhs.split_whitespace()
                .next()
                .is_some_and(|token| token.split('.').next() == Some(opcode))
        })
        .count()
}

#[test]
fn opt_sink_numeric_resolved_float_keeps_original_guards_and_frames_without_duplicate_dispatch() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let roots = Roots::enter();
    let mut quality = Vec::new();
    for op in [Op::Add, Op::Sub, Op::Mul, Op::Div] {
        // pc2 resolves a fresh Borrow of the actual static Float and unknown
        // arg0. pc5 resolves another unknown Borrow after the pc2 result has
        // two aliases in the original frame. Both precise exits remain.
        let f = program(
            vec![
                Op::StackRef(1),
                Op::Constant(0),
                op.clone(),
                Op::Dup,
                Op::StackRef(2),
                Op::Add,
                Op::Return,
            ],
            vec![Value::make_float(2.0)],
            2,
        );
        roots.add(&f.constants);
        let cases = [-0.0, 0.0, 3.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY]
            .into_iter()
            .map(|x| vec![Value::make_float(x), Value::make_float(-0.0)])
            .chain([
                vec![Value::make_int(3), Value::make_int(1)],
                vec![
                    Value::make_int(Value::MOST_POSITIVE_FIXNUM),
                    Value::make_int(0),
                ],
                vec![Value::make_int(9_007_199_254_740_993), Value::make_int(1)],
            ])
            .collect::<Vec<_>>();
        for args in &cases {
            roots.add(args);
            if let Ok(answer) = tier0(&mut ctx, &f, args) {
                roots.add(&[answer]);
            }
        }
        // Train successful numeric cases only, and compile both actual arms
        // before wrong types widen sticky feedback. GNU-derived edge families
        // are the existing signed-zero/NaN/inf/2^53 mixed arithmetic groups.
        let (baseline, _) = compile(&ctx, &f, false, &roots);
        let (selected, clif) = compile(&ctx, &f, true, &roots);
        let (_, _, sites, _) = lowering::LAST_IR_STATS.with(|c| c.get());
        check_semantics(&mut ctx, &f, &baseline, &cases, &roots);
        check_semantics(&mut ctx, &f, &selected, &cases, &roots);
        let valid = [Value::make_float(3.0), Value::make_int(1)];
        roots.add(&valid);
        let first = native_result(&mut ctx, &f, &selected, &valid).unwrap();
        roots.add(&[first]);
        let second = native_result(&mut ctx, &f, &selected, &valid).unwrap();
        roots.add(&[second]);
        assert_ne!(first.bits(), second.bits(), "fresh dynamic result identity");
        assert_ne!(first.bits(), valid[0].bits());
        assert_ne!(first.bits(), f.constants[0].bits());

        let bad_first = [Value::symbol("not-a-number"), Value::make_int(1)];
        roots.add(&bad_first);
        let expected = tier0(&mut ctx, &f, &bad_first);
        for leaf in [&baseline, &selected] {
            let NativeRun::DeoptAt(exit) = leaf.call_consts(
                &mut ctx as *mut Context as *mut u8,
                f.constants.as_ptr(),
                &bad_first,
            ) else {
                panic!("original pc2 Borrow type guard must remain");
            };
            roots.add(&exit.stack);
            assert_eq!(exit.pc, 2);
            assert_eq!(
                exit.stack,
                [bad_first[0], bad_first[1], bad_first[0], f.constants[0]]
            );
            assert_eq!(observe(&resume(&mut ctx, &f, &exit)), observe(&expected));
            assert_eq!(ctx.jit_root_stack_top, 0);
        }

        let bad_late = [Value::make_float(3.0), Value::string("not-a-number")];
        roots.add(&bad_late);
        let expected = tier0(&mut ctx, &f, &bad_late);
        let prefix = program(
            vec![Op::StackRef(0), Op::Constant(0), op.clone(), Op::Return],
            vec![f.constants[0]],
            1,
        );
        let product = tier0(&mut ctx, &prefix, &bad_late[..1]).unwrap();
        roots.add(&[product]);
        for leaf in [&baseline, &selected] {
            let NativeRun::DeoptAt(exit) = leaf.call_consts(
                &mut ctx as *mut Context as *mut u8,
                f.constants.as_ptr(),
                &bad_late,
            ) else {
                panic!("original pc5 unknown-Borrow guard must remain");
            };
            roots.add(&exit.stack);
            assert_eq!(exit.pc, 5);
            assert_eq!(exit.stack.len(), 5);
            assert_eq!(exit.stack[0].bits(), bad_late[0].bits());
            assert_eq!(exit.stack[1].bits(), bad_late[1].bits());
            assert_eq!(exit.stack[4].bits(), bad_late[1].bits());
            assert_eq!(
                exit.stack[2].bits(),
                exit.stack[3].bits(),
                "one cold alias box"
            );
            assert_eq!(exit.stack[2].xfloat().to_bits(), product.xfloat().to_bits());
            assert_ne!(exit.stack[2].bits(), bad_late[0].bits());
            assert_ne!(exit.stack[2].bits(), f.constants[0].bits());
            assert_eq!(observe(&resume(&mut ctx, &f, &exit)), observe(&expected));
            assert_eq!(ctx.jit_root_stack_top, 0);
        }
        quality.push((
            op,
            crate::emacs_core::jit::compile::opt_census::snapshot(&selected.obs)
                .opt_sink
                .unwrap()
                .numeric_sources,
            sites,
            clif,
        ));
    }

    // Feedback-only Float is no source-kind proof. The first source's two
    // borrowed Args and its ready result must retain exact checked FIX paths.
    let mixed = program(
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Add,
            Op::Dup,
            Op::Mul,
            Op::Return,
        ],
        Vec::new(),
        2,
    );
    let training = [Value::make_float(2.0), Value::make_float(3.0)];
    roots.add(&training);
    roots.add(&[tier0(&mut ctx, &mixed, &training).unwrap()]);
    let (baseline, _) = compile(&ctx, &mixed, false, &roots);
    let (selected, clif) = compile(&ctx, &mixed, true, &roots);
    let (_, _, sites, _) = lowering::LAST_IR_STATS.with(|c| c.get());
    force_deopt_for_test(true);
    let (forced, _) = compile(&ctx, &mixed, true, &roots);
    force_deopt_for_test(false);
    let cases = vec![
        vec![Value::make_int(2), Value::make_int(3)],
        vec![
            Value::make_int(Value::MOST_POSITIVE_FIXNUM),
            Value::make_int(0),
        ],
    ];
    check_semantics(&mut ctx, &mixed, &baseline, &cases, &roots);
    check_semantics(&mut ctx, &mixed, &selected, &cases, &roots);
    let NativeRun::DeoptAt(exit) = forced.call_consts(
        &mut ctx as *mut Context as *mut u8,
        mixed.constants.as_ptr(),
        &cases[0],
    ) else {
        panic!("force hook still applies to actual checked both-fix Add");
    };
    roots.add(&exit.stack);
    assert_eq!(exit.pc, 2);
    assert_eq!(
        exit.stack,
        [cases[0][0], cases[0][1], cases[0][0], cases[0][1]]
    );
    assert_eq!(
        observe(&resume(&mut ctx, &mixed, &exit)),
        observe(&tier0(&mut ctx, &mixed, &cases[0]))
    );
    let NativeRun::DeoptAt(exit) = selected.call_consts(
        &mut ctx as *mut Context as *mut u8,
        mixed.constants.as_ptr(),
        &cases[1],
    ) else {
        panic!("unknown-kind both-fix Mul overflow must retain exact exit");
    };
    roots.add(&exit.stack);
    assert_eq!(exit.pc, 4);
    assert_eq!(
        exit.stack,
        [cases[1][0], cases[1][1], cases[1][0], cases[1][0]]
    );
    assert_eq!(
        observe(&resume(&mut ctx, &mixed, &exit)),
        observe(&tier0(&mut ctx, &mixed, &cases[1]))
    );
    assert_eq!(ctx.jit_root_stack_top, 0);

    // All native/reference/replay checks above precede compiler quality.
    assert_eq!(
        crate::emacs_core::jit::compile::opt_census::snapshot(&selected.obs)
            .opt_sink
            .unwrap()
            .numeric_sources,
        2
    );
    assert_eq!(sites, 2, "both unknown-kind precise sites retained");
    assert!(
        has_integer_multiply_arm(&clif),
        "feedback does not erase checked FIX arm"
    );
    for (op, sources, sites, clif) in quality {
        assert_eq!(sources, 2);
        assert_eq!(
            sites, 2,
            "both grounded-Float sites still require Borrow guards"
        );
        let opcode = match &op {
            Op::Add => "fadd",
            Op::Sub => "fsub",
            Op::Mul => "fmul",
            Op::Div => "fdiv",
            _ => unreachable!(),
        };
        assert_eq!(
            float_ops(&clif, opcode),
            if op == Op::Add { 2 } else { 1 },
            "resolved Float arms must not duplicate the same operation: {clif}"
        );
        assert_eq!(
            float_ops(&clif, "fadd"),
            if op == Op::Add { 2 } else { 1 },
            "late unknown Borrow guard remains, scalar Add needs no ff/mix duplicate: {clif}"
        );
    }
}
