//! Real source/frame checks precede unused precise-exit quality assertions.
use super::*;
#[path = "opt_sink_numeric_fixture.rs"]
mod fixture;
use fixture::*;

#[test]
fn opt_sink_numeric_infallible_ready_float_omits_only_unused_precise_exit() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let roots = Roots::enter();
    // pc2 resolves original borrowed inputs. pc4 consumes that ready Float
    // twice. pc7 still resolves a NEW unknown borrowed input after two aliases
    // of the fresh pc4 identity have become live in its original GNU frame.
    let mut qualities = Vec::new();
    for op in [Op::Add, Op::Sub, Op::Mul, Op::Div] {
        let f = program(
            vec![
                Op::StackRef(1),
                Op::Constant(0),
                Op::Mul,
                Op::Dup,
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
        let cases = [-0.0, 0.0, 3.0, f64::NAN, f64::INFINITY]
            .into_iter()
            .map(|x| vec![Value::make_float(x), Value::make_float(-0.0)])
            .chain([
                vec![Value::make_int(3), Value::make_int(1)],
                vec![
                    Value::make_int(Value::MOST_POSITIVE_FIXNUM),
                    Value::make_int(0),
                ],
            ])
            .collect::<Vec<_>>();
        for args in &cases {
            roots.add(args);
            if let Ok(value) = tier0(&mut ctx, &f, args) {
                roots.add(&[value]);
            }
        }
        let (base, _) = compile(&ctx, &f, false, &roots);
        let (selected, clif) = compile(&ctx, &f, true, &roots);
        let (_, _, sites, _) = lowering::LAST_IR_STATS.with(|c| c.get());
        check_semantics(&mut ctx, &f, &base, &cases, &roots);
        check_semantics(&mut ctx, &f, &selected, &cases, &roots);
        let bad = [Value::make_float(3.0), Value::string("not-a-number")];
        roots.add(&bad);
        let expected = tier0(&mut ctx, &f, &bad);
        assert_eq!(
            observe(&native_result(&mut ctx, &f, &base, &bad)),
            observe(&expected)
        );
        let bad_first = [Value::symbol("not-a-number"), Value::make_float(1.0)];
        roots.add(&bad_first);
        let first_error = tier0(&mut ctx, &f, &bad_first);
        for leaf in [&base, &selected] {
            // guard_float branches directly and intentionally does not use the
            // force-deopt hook. Exercise its actual wrong-type branch instead.
            let NativeRun::DeoptAt(exit) = leaf.call_consts(
                &mut ctx as *mut Context as *mut u8,
                f.constants.as_ptr(),
                &bad_first,
            ) else {
                panic!("actual Symbol input must fail original Borrow at pc2");
            };
            roots.add(&exit.stack);
            assert_eq!(exit.pc, 2);
            assert_eq!(
                exit.stack,
                [bad_first[0], bad_first[1], bad_first[0], f.constants[0]]
            );
            assert_eq!(observe(&resume(&mut ctx, &f, &exit)), observe(&first_error));
            assert_eq!(ctx.jit_root_stack_top, 0);
        }
        let NativeRun::DeoptAt(exit) = selected.call_consts(
            &mut ctx as *mut Context as *mut u8,
            f.constants.as_ptr(),
            &bad,
        ) else {
            panic!("pc7 Borrow guard must retain exact original cold frame");
        };
        roots.add(&exit.stack);
        assert_eq!(exit.pc, 7);
        assert_eq!(exit.stack.len(), 5);
        assert_eq!(exit.stack[0].bits(), bad[0].bits());
        assert_eq!(exit.stack[1].bits(), bad[1].bits());
        assert_eq!(exit.stack[4].bits(), bad[1].bits());
        assert_eq!(exit.stack[2].bits(), exit.stack[3].bits());
        let oracle = program(
            vec![
                Op::StackRef(0),
                Op::Constant(0),
                Op::Mul,
                Op::Dup,
                op.clone(),
                Op::Return,
            ],
            vec![f.constants[0]],
            1,
        );
        let product = tier0(&mut ctx, &oracle, &bad[..1]).unwrap();
        roots.add(&[product]);
        assert_eq!(exit.stack[2].xfloat().to_bits(), product.xfloat().to_bits());
        assert_ne!(exit.stack[2].bits(), bad[0].bits());
        assert_ne!(exit.stack[2].bits(), f.constants[0].bits());
        assert_eq!(observe(&resume(&mut ctx, &f, &exit)), observe(&expected));
        assert_eq!(ctx.jit_root_stack_top, 0);

        qualities.push((
            crate::emacs_core::jit::compile::opt_census::snapshot(&selected.obs)
                .opt_sink
                .unwrap()
                .numeric_sources,
            sites,
            clif,
        ));
    }

    // Feedback-only Float cannot omit the checked both-fix arm. Both results
    // below are Ready, but no actual source/constant proves Float contagion.
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
    let (mixed_base, _) = compile(&ctx, &mixed, false, &roots);
    let (mixed_selected, _) = compile(&ctx, &mixed, true, &roots);
    let (_, _, mixed_sites, _) = lowering::LAST_IR_STATS.with(|c| c.get());
    // The retained checked both-fix Add uses emit_guard, unlike guard_float.
    // Compile its forced control before error/overflow feedback can widen.
    force_deopt_for_test(true);
    let (mixed_forced, _) = compile(&ctx, &mixed, true, &roots);
    force_deopt_for_test(false);
    let integer_cases = vec![
        vec![Value::make_int(2), Value::make_int(3)],
        vec![
            Value::make_int(Value::MOST_POSITIVE_FIXNUM),
            Value::make_int(0),
        ],
    ];
    check_semantics(&mut ctx, &mixed, &mixed_base, &integer_cases, &roots);
    check_semantics(&mut ctx, &mixed, &mixed_selected, &integer_cases, &roots);
    let forced_args = &integer_cases[0];
    let NativeRun::DeoptAt(exit) = mixed_forced.call_consts(
        &mut ctx as *mut Context as *mut u8,
        mixed.constants.as_ptr(),
        forced_args,
    ) else {
        panic!("forced actual checked Add must retain pc2 exit");
    };
    roots.add(&exit.stack);
    assert_eq!(exit.pc, 2);
    assert_eq!(
        exit.stack,
        [
            forced_args[0],
            forced_args[1],
            forced_args[0],
            forced_args[1]
        ]
    );
    assert_eq!(
        observe(&resume(&mut ctx, &mixed, &exit)),
        observe(&tier0(&mut ctx, &mixed, forced_args))
    );
    assert_eq!(ctx.jit_root_stack_top, 0);
    let overflow = &integer_cases[1];
    let NativeRun::DeoptAt(exit) = mixed_selected.call_consts(
        &mut ctx as *mut Context as *mut u8,
        mixed.constants.as_ptr(),
        overflow,
    ) else {
        panic!("Ready mixed-kind fixnum overflow must retain pc4 exit");
    };
    roots.add(&exit.stack);
    assert_eq!(exit.pc, 4);
    assert_eq!(
        exit.stack,
        [overflow[0], overflow[1], overflow[0], overflow[0]]
    );
    assert_eq!(
        observe(&resume(&mut ctx, &mixed, &exit)),
        observe(&tier0(&mut ctx, &mixed, overflow))
    );
    assert_eq!(ctx.jit_root_stack_top, 0);

    // Quality assertions LAST for ALL four operations: current queues 3 sites, including
    // pc4 which no emitted branch can enter. Proposal queues just pc2/pc7.
    for (sources, sites, clif) in qualities {
        assert_eq!(sources, 3);
        assert_eq!(
            sites, 2,
            "only the never-targeted pc4 exit is omitted: {clif}"
        );
    }
    assert_eq!(
        crate::emacs_core::jit::compile::opt_census::snapshot(&mixed_selected.obs)
            .opt_sink
            .unwrap()
            .numeric_sources,
        2
    );
    assert_eq!(mixed_sites, 2, "both-fix type/overflow exits retained");
}
