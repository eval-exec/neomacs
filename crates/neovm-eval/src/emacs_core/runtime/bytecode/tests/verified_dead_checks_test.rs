//! Execute verified stack-check candidates only after an independent, checked
//! reference has visited their operands. A verifier/model disagreement panics
//! before the VM runs; debug assertions in the optimized arms guard the actual
//! unsafe accesses too. No arbitrary decoder-fuzz program is executed.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::bytecode::decode::{
    decode_gnu_bytecode, parse_arglist_descriptor, verify_stack_effects,
};
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug)]
enum Datum {
    Nil,
    True,
    Int(i64),
    Pair(Rc<RefCell<(Datum, Datum)>>),
}

impl Datum {
    #[doc(hidden)]
    pub fn pair(car: Self, cdr: Self) -> Self {
        Self::Pair(Rc::new(RefCell::new((car, cdr))))
    }

    #[doc(hidden)]
    pub fn value(&self) -> Value {
        match self {
            Self::Nil => Value::NIL,
            Self::True => Value::T,
            Self::Int(n) => Value::fixnum(*n),
            Self::Pair(pair) => {
                let pair = pair.borrow();
                Value::cons(pair.0.value(), pair.1.value())
            }
        }
    }

    #[doc(hidden)]
    pub fn printed(&self) -> String {
        match self {
            Self::Nil => "nil".to_owned(),
            Self::True => "t".to_owned(),
            Self::Int(n) => n.to_string(),
            Self::Pair(pair) => {
                let pair = pair.borrow();
                format!("({} . {})", pair.0.printed(), pair.1.printed())
            }
        }
    }

    #[doc(hidden)]
    pub fn integer(&self) -> i64 {
        match self {
            Self::Int(n) => *n,
            _ => panic!("bounded arithmetic corpus requires integer operands"),
        }
    }
}

fn pop(stack: &mut Vec<Datum>, pc: usize) -> Datum {
    stack
        .pop()
        .unwrap_or_else(|| panic!("independent reference underflow at pc {pc}"))
}

/// This follows operand accesses and checked Vec operations, rather than
/// consulting the production verifier's min/net table or its proof flag.
fn reference(
    ops: &[Op],
    constants: &[Datum],
    args: &[Datum],
    max_stack: usize,
) -> Result<String, String> {
    let mut stack = args.to_vec();
    let mut pc = 0usize;
    for _ in 0..256 {
        assert!(
            stack.len() <= max_stack,
            "reference max_stack before pc {pc}"
        );
        let here = pc;
        let op = ops.get(pc).expect("reference pc is in the sealed stream");
        pc += 1;
        match op {
            Op::Constant(index) => {
                stack.push(constants.get(*index as usize).expect("constant").clone())
            }
            Op::Nil => stack.push(Datum::Nil),
            Op::True => stack.push(Datum::True),
            Op::Pop => {
                pop(&mut stack, here);
            }
            Op::Dup => stack.push(stack.last().expect("reference dup operand").clone()),
            Op::StackRef(offset) => {
                let index = stack
                    .len()
                    .checked_sub(1 + *offset as usize)
                    .expect("reference stack-ref range");
                stack.push(
                    stack
                        .get(index)
                        .expect("reference stack-ref operand")
                        .clone(),
                );
            }
            Op::StackSet(offset) => {
                let value = pop(&mut stack, here);
                if *offset != 0 {
                    let index = stack
                        .len()
                        .checked_sub(*offset as usize)
                        .expect("reference stack-set range");
                    *stack
                        .get_mut(index)
                        .expect("reference stack-set destination") = value;
                }
            }
            Op::Add1 => {
                let a = pop(&mut stack, here).integer();
                stack.push(Datum::Int(a.checked_add(1).expect("bounded integer")));
            }
            Op::Add | Op::Sub | Op::Mul | Op::Lss => {
                let b = pop(&mut stack, here).integer();
                let a = pop(&mut stack, here).integer();
                stack.push(match op {
                    Op::Add => Datum::Int(a.checked_add(b).expect("bounded integer")),
                    Op::Sub => Datum::Int(a.checked_sub(b).expect("bounded integer")),
                    Op::Mul => Datum::Int(a.checked_mul(b).expect("bounded integer")),
                    Op::Lss if a < b => Datum::True,
                    Op::Lss => Datum::Nil,
                    _ => unreachable!(),
                });
            }
            Op::Car | Op::Cdr => {
                let value = pop(&mut stack, here);
                let answer = match &value {
                    Datum::Pair(pair) if matches!(op, Op::Car) => pair.borrow().0.clone(),
                    Datum::Pair(pair) => pair.borrow().1.clone(),
                    Datum::Nil => Datum::Nil,
                    _ => return Err(format!("wrong-type-argument listp {}", value.printed())),
                };
                stack.push(answer);
            }
            Op::Setcar => {
                let value = pop(&mut stack, here);
                let cell = pop(&mut stack, here);
                match cell {
                    Datum::Pair(pair) => pair.borrow_mut().0 = value.clone(),
                    other => return Err(format!("wrong-type-argument consp {}", other.printed())),
                }
                stack.push(value);
            }
            Op::Goto(target) => pc = *target as usize,
            Op::GotoIfNil(target) => {
                if matches!(pop(&mut stack, here), Datum::Nil) {
                    pc = *target as usize;
                }
            }
            // Return is intentionally allowed at depth zero: the current
            // verifier establishes no minimum for terminal instructions.
            Op::Return => return Ok(stack.pop().unwrap_or(Datum::Nil).printed()),
            _ => panic!("unsupported bounded reference opcode {op:?}"),
        }
        assert!(
            stack.len() <= max_stack,
            "reference max_stack after pc {here}"
        );
    }
    panic!("bounded reference exceeded its step budget");
}

fn run_case(name: &str, ops: Vec<Op>, constants: Vec<Datum>, args: Vec<Datum>, max_stack: usize) {
    let mut ctx = Context::new_vm_runtime_harness(crate::test_utils::unit_runtime());
    let values: Vec<Value> = constants.iter().map(Datum::value).collect();
    let arguments: Vec<Value> = args.iter().map(Datum::value).collect();
    let roots = ctx.save_vm_roots();
    for value in values.iter().chain(&arguments).copied() {
        ctx.push_vm_frame_root(value);
    }
    let nargs = args.len();
    let mut code = ByteCodeFunction::new(parse_arglist_descriptor((nargs | (nargs << 8)) as i64));
    code.ops = ops;
    code.constants = values.into();
    code.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(max_stack);
    code.seal_hand_assembled_ops_for_test();
    assert!(
        code.executes_verified_ops(),
        "{name}: must exercise VERIFIED=true: {:?}",
        code.executable_ops()
    );
    // Finish the independent walk BEFORE entering any unchecked VM path.
    let expected = reference(code.executable_ops(), &constants, &args, max_stack);
    let mut vm = Vm::from_context(&mut ctx);
    #[cfg(feature = "jit")]
    vm.force_interpreter_only_for_test();
    let actual = match vm.execute(&code, arguments) {
        Ok(value) => Ok(print_value(&value)),
        Err(flow) => match flow.into_kind() {
            neovm_value::error::FlowKind::Signal(signal) => Err(format!(
                "{} {}",
                signal.symbol_name(),
                signal
                    .data
                    .iter()
                    .map(print_value)
                    .collect::<Vec<_>>()
                    .join(" "),
            )),
            other => panic!("{name}: unexpected flow {other:?}"),
        },
    };
    assert_eq!(actual, expected, "{name}: {:?}", code.executable_ops());
    assert!(
        vm.ctx.bc_buf.is_empty(),
        "{name}: operand frontier restored"
    );
    assert!(
        vm.ctx.bc_frames.is_empty(),
        "{name}: root frontier restored"
    );
    vm.ctx.restore_vm_roots(roots);
}

#[test]
fn verified_dead_checks_cover_minimum_operands_and_fused_sequences() {
    for op in [Op::Car, Op::Cdr] {
        run_case(
            "pair unary",
            vec![op.clone(), Op::Return],
            vec![],
            vec![Datum::pair(Datum::Int(4), Datum::Int(5))],
            1,
        );
        run_case(
            "nil unary",
            vec![op, Op::Return],
            vec![],
            vec![Datum::Nil],
            1,
        );
    }
    run_case(
        "add1",
        vec![Op::Add1, Op::Return],
        vec![],
        vec![Datum::Int(4)],
        1,
    );
    for op in [Op::Add, Op::Lss] {
        run_case(
            "binary",
            vec![op, Op::Return],
            vec![],
            vec![Datum::Int(4), Datum::Int(5)],
            2,
        );
    }
    run_case(
        "setcar",
        vec![Op::Setcar, Op::Return],
        vec![],
        vec![Datum::pair(Datum::Int(4), Datum::Int(5)), Datum::Int(7)],
        2,
    );
    run_case(
        "dup",
        vec![Op::Dup, Op::Add, Op::Return],
        vec![],
        vec![Datum::Int(4)],
        2,
    );
    run_case(
        "pop",
        // Return reads the absolute buffer. Keep a result owned by this frame
        // instead of observing the parked original argument below frame_base.
        vec![Op::Pop, Op::Nil, Op::Return],
        vec![],
        vec![Datum::Int(4)],
        1,
    );
    for offset in [0, 1, 3] {
        run_case(
            "stack-set",
            if offset == 0 {
                vec![Op::StackSet(0), Op::Nil, Op::Return]
            } else {
                // Read back the destination, not an unaffected younger slot.
                vec![Op::StackSet(offset), Op::StackRef(offset - 1), Op::Return]
            },
            vec![],
            (0..=offset).map(|n| Datum::Int(i64::from(n))).collect(),
            usize::from(offset) + 1,
        );
    }
    run_case(
        "constant/stack-ref/return",
        vec![Op::Constant(0), Op::StackRef(0), Op::Return],
        vec![Datum::Int(9)],
        vec![],
        2,
    );
    for (a, b) in [(4, 5), (5, 4)] {
        run_case(
            "dup/stack-ref/lss/branch",
            vec![
                Op::Dup,
                Op::StackRef(2),
                Op::Lss,
                Op::GotoIfNil(5),
                Op::Add1,
                Op::Return,
            ],
            vec![],
            vec![Datum::Int(b), Datum::Int(a)],
            4,
        );
    }
}

#[test]
fn verified_dead_checks_reject_one_short_without_executing_invalid_code() {
    let cases = [
        (Op::Car, 1),
        (Op::Cdr, 1),
        (Op::Add1, 1),
        (Op::Add, 2),
        (Op::Lss, 2),
        (Op::Setcar, 2),
        (Op::Dup, 1),
        (Op::Pop, 1),
        (Op::StackSet(0), 1),
        (Op::StackSet(1), 2),
        (Op::StackSet(3), 4),
        (Op::StackRef(0), 1),
        (Op::StackRef(3), 4),
    ];
    for (op, required) in cases {
        assert!(
            !verify_stack_effects(&[op.clone(), Op::Return], required - 1, 8),
            "one-short {op:?}"
        );
    }
    assert!(
        !verify_stack_effects(&[Op::Constant(0), Op::StackRef(1), Op::Return], 0, 3),
        "fused constant cannot manufacture an older frame operand"
    );
    assert!(verify_stack_effects(&[Op::Return], 0, 1));
    run_case("empty terminal return", vec![Op::Return], vec![], vec![], 1);
}

#[test]
fn verified_dead_checks_execute_existing_decoder_fixture_bytes() {
    // Existing decode_test.rs fixtures: constant-return, car/cdr and the
    // add/sub/mul byte sequence. Supply their required entry operands rather
    // than treating successful decoding as proof of safe execution.
    let cases: &[(&str, &[u8], Vec<Datum>, Vec<Datum>, usize)] = &[
        (
            "decode constant",
            &[192, 135],
            vec![Datum::Int(42)],
            vec![],
            1,
        ),
        (
            "decode car/cdr",
            &[64, 65, 135],
            vec![],
            vec![Datum::Nil],
            1,
        ),
        (
            "decode arithmetic",
            &[92, 90, 95, 135],
            vec![],
            vec![Datum::Int(3), Datum::Int(4), Datum::Int(5), Datum::Int(6)],
            4,
        ),
    ];
    for (name, bytes, constants, args, limit) in cases {
        let mut values: Vec<Value> = constants.iter().map(Datum::value).collect();
        let ops = decode_gnu_bytecode(bytes, &mut values).expect("existing decoder fixture");
        run_case(name, ops, constants.clone(), args.clone(), *limit);
    }
}

#[test]
fn verified_dead_checks_keep_gnu_primitive_type_errors_on_the_verified_path() {
    // Same bounded pure error inputs as primitive_error_backtrace/{car,cdr,
    // setcar}.expect. The full oracle family separately checks GNU debugger
    // frames; here the real primitive runs with an asserted stack proof.
    for op in [Op::Car, Op::Cdr] {
        run_case(
            "GNU unary type error",
            vec![op, Op::Return],
            vec![],
            vec![Datum::Int(3)],
            1,
        );
    }
    run_case(
        "GNU setcar type error",
        vec![Op::Setcar, Op::Return],
        vec![],
        vec![Datum::Int(3), Datum::Int(7)],
        2,
    );
}

#[test]
fn verified_dead_checks_execute_a_bounded_fixed_seed_stack_corpus() {
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    for case in 0..256 {
        let mut ops = Vec::new();
        let mut integer = Vec::new();
        for _ in 0..32 {
            seed ^= seed >> 12;
            seed ^= seed << 25;
            seed ^= seed >> 27;
            let choice = seed.wrapping_mul(0x2545_f491_4f6c_dd1d) % 8;
            let len = integer.len();
            if len == 0 || (len < 8 && choice == 0) {
                ops.push(Op::Constant((seed % 4) as u16));
                integer.push(true);
            } else if choice == 1 && len < 8 {
                ops.push(Op::Dup);
                integer.push(*integer.last().unwrap());
            } else if choice == 2 && len < 8 {
                let offset = (seed as usize % len) as u16;
                ops.push(Op::StackRef(offset));
                integer.push(integer[len - 1 - usize::from(offset)]);
            } else if choice == 3 && integer[len - 1] {
                ops.push(Op::Add1);
            } else if choice == 4 && len >= 2 && integer[len - 1] && integer[len - 2] {
                ops.push(Op::Add);
                integer.pop();
            } else if choice == 5 && len >= 2 && integer[len - 1] && integer[len - 2] {
                ops.push(Op::Lss);
                integer.pop();
                *integer.last_mut().unwrap() = false;
            } else if choice == 6 {
                let offset = (seed as usize % len) as u16;
                ops.push(Op::StackSet(offset));
                let top = integer.pop().unwrap();
                if offset != 0 {
                    integer[len - 1 - usize::from(offset)] = top;
                }
            } else {
                ops.push(Op::Pop);
                integer.pop();
            }
        }
        ops.push(Op::Return);
        run_case(
            &format!("seeded program {case}"),
            ops,
            vec![Datum::Int(-3), Datum::Int(0), Datum::Int(1), Datum::Int(7)],
            vec![],
            8,
        );
    }
}
