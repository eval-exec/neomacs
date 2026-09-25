use super::*;
use crate::emacs_core::intern::resolve_sym;
use crate::emacs_core::value::HashTableTest;

#[test]
fn string_value_to_bytes_basic() {
    crate::test_utils::init_test_tracing();
    let bytes = string_value_to_bytes("ABC");
    assert_eq!(bytes, vec![65, 66, 67]);
}

#[test]
fn string_value_to_bytes_octal_escape() {
    crate::test_utils::init_test_tracing();
    // \300 = 0xC0 = char 192
    let s = "\u{00C0}"; // 192 as char
    let bytes = string_value_to_bytes(s);
    assert_eq!(bytes, vec![0xC0]);
}

#[test]
fn decode_simple_constant_return() {
    crate::test_utils::init_test_tracing();
    // bytecodes: constant(0) return
    // byte 192 = constant 0, byte 135 = return
    let bytecodes = vec![192, 135];
    let mut constants = vec![Value::fixnum(42)];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::Constant(0), Op::Return]);
}

#[test]
fn decode_car_cdr() {
    crate::test_utils::init_test_tracing();
    // car=64, cdr=65, return=135
    let bytecodes = vec![64, 65, 135];
    let mut constants = vec![];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::Car, Op::Cdr, Op::Return]);
}

#[test]
fn decode_arithmetic() {
    crate::test_utils::init_test_tracing();
    // add=92, sub=90, mul=95, return=135
    let bytecodes = vec![92, 90, 95, 135];
    let mut constants = vec![];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::Add, Op::Sub, Op::Mul, Op::Return]);
}

#[test]
fn decode_varref_immediate() {
    crate::test_utils::init_test_tracing();
    // varref 0 = byte 8, varref 5 = byte 13
    let bytecodes = vec![8, 13, 135];
    let mut constants = vec![
        Value::symbol("x"),
        Value::symbol("y"),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::symbol("z"),
    ];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::VarRef(0), Op::VarRef(5), Op::Return]);
}

#[test]
fn decode_goto_jump_patching() {
    crate::test_utils::init_test_tracing();
    // constant(0), goto-if-nil to byte 5, constant(1), return, constant(2), return
    // byte 0: 192 → constant(0) [1 byte]
    // byte 1: 131, 5, 0 → goto-if-nil to byte 5 [3 bytes]
    // byte 4: 193 → constant(1) [1 byte]
    // byte 5: 135 → return [1 byte]
    let bytecodes = vec![192, 131, 5, 0, 193, 135];
    let mut constants = vec![Value::NIL, Value::fixnum(1)];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    // Instructions: [0] constant(0), [1] goto-if-nil(4), [2] constant(1), [3] return
    // Wait, byte 5 maps to instruction index... let me trace:
    // byte 0 → instr 0: constant(0)
    // byte 1 → instr 1: goto-if-nil(target=byte 5)
    // byte 4 → instr 2: constant(1)
    // byte 5 → instr 3: return
    // So goto-if-nil should jump to instruction 3
    assert_eq!(
        ops,
        vec![
            Op::Constant(0),
            Op::GotoIfNil(3),
            Op::Constant(1),
            Op::Return,
        ]
    );
}

#[test]
fn decode_call_immediate() {
    crate::test_utils::init_test_tracing();
    // call 0 = byte 32, call 3 = byte 35
    let bytecodes = vec![32, 35, 135];
    let mut constants = vec![];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::Call(0), Op::Call(3), Op::Return]);
}

#[test]
fn decode_list_ops() {
    crate::test_utils::init_test_tracing();
    // list1=67, list2=68, cons=66
    let bytecodes = vec![67, 68, 66, 135];
    let mut constants = vec![];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::List(1), Op::List(2), Op::Cons, Op::Return]);
}

#[test]
fn decode_constant_range() {
    crate::test_utils::init_test_tracing();
    // byte 192 = constant(0), byte 255 = constant(63)
    let bytecodes = vec![192, 255, 135];
    let mut constants = (0..64).map(Value::fixnum).collect();
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::Constant(0), Op::Constant(63), Op::Return]);
}

#[test]
fn decode_rejects_unused_opcode_128() {
    crate::test_utils::init_test_tracing();
    let mut constants = vec![Value::fixnum(42)];
    let err = decode_gnu_bytecode(&[128, 0, 135], &mut constants).unwrap_err();
    assert!(matches!(err, DecodeError::UnknownOpcode(128, 0)));
}

#[test]
fn decode_unwind_protect_pop() {
    crate::test_utils::init_test_tracing();
    // unwind-protect = byte 142
    let bytecodes = vec![142, 135];
    let mut constants = vec![];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::UnwindProtectPop, Op::Return]);
}

#[test]
fn decode_save_excursion_and_restriction() {
    crate::test_utils::init_test_tracing();
    let mut constants = vec![];
    let save_excursion = decode_gnu_bytecode(&[138, 135], &mut constants).unwrap();
    assert_eq!(save_excursion, vec![Op::SaveExcursion, Op::Return]);

    let mut constants = vec![];
    let save_restriction = decode_gnu_bytecode(&[140, 135], &mut constants).unwrap();
    assert_eq!(save_restriction, vec![Op::SaveRestriction, Op::Return]);
}

#[test]
fn decode_discard_n() {
    crate::test_utils::init_test_tracing();
    // discardN = byte 182, operand = 3
    let bytecodes = vec![182, 3, 135];
    let mut constants = vec![];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::DiscardN(3), Op::Return]);
}

#[test]
fn decode_non_switch_does_not_retain_byte_offset_map() {
    crate::test_utils::init_test_tracing();
    let mut constants = vec![];
    let (ops, offset_map) =
        decode_gnu_bytecode_with_offset_map(&[182, 3, 135], &mut constants).unwrap();

    assert_eq!(ops, vec![Op::DiscardN(3), Op::Return]);
    assert!(
        offset_map.is_empty(),
        "only Bswitch needs byte-offset translation after decoding"
    );
}

#[test]
fn decode_switch_preserves_hash_table_byte_targets() {
    crate::test_utils::init_test_tracing();
    let table = Value::hash_table(HashTableTest::Eq);
    if !table.is_hash_table() {
        panic!("expected hash table constant");
    };
    let _ = table.with_hash_table_mut(|ht| {
        let key = Value::symbol("foo").to_hash_key(&ht.test);
        ht.insert(key, Value::symbol("foo"), Value::fixnum(8));
    });

    // byte 0: constant key
    // byte 1: constant switch-table
    // byte 2: switch
    // byte 3: goto byte 8
    // byte 6: constant default
    // byte 7: return
    // byte 8: constant target
    // byte 9: return
    let bytecodes = vec![193, 192, 183, 130, 8, 0, 194, 135, 195, 135];
    let mut constants = vec![
        table,
        Value::symbol("foo"),
        Value::fixnum(10),
        Value::fixnum(20),
    ];
    let (ops, offset_map) =
        decode_gnu_bytecode_with_offset_map(&bytecodes, &mut constants).unwrap();

    assert_eq!(
        ops,
        vec![
            Op::Constant(1),
            Op::Constant(0),
            Op::Switch,
            Op::Goto(6),
            Op::Constant(2),
            Op::Return,
            Op::Constant(3),
            Op::Return,
        ]
    );

    let raw_target = {
        table
            .as_hash_table()
            .unwrap()
            .data
            .values()
            .next()
            .copied()
            .expect("switch table target")
    };
    assert_eq!(raw_target, Value::fixnum(8));
    assert_eq!(
        offset_map
            .binary_search_by_key(&8, |entry| entry.byte_offset)
            .map(|index| offset_map[index].instruction_index),
        Ok(6)
    );
}

#[test]
fn decode_buffer_op_point() {
    crate::test_utils::init_test_tracing();
    // point = byte 96
    let bytecodes = vec![96, 135];
    let mut constants = vec![];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    // Upstream 762188a5d moved buffer-op dispatch inline: the decoder
    // emits Op::CallBuiltinSym(intern("point"), 0) and does NOT inject
    // the symbol into the constants pool. Verify the op shape.
    match &ops[0] {
        Op::CallBuiltinSym(sym, 0) => {
            assert_eq!(
                crate::emacs_core::intern::resolve_sym(*sym),
                "point",
                "buffer-op byte 96 should dispatch to `point`"
            );
        }
        other => panic!("expected CallBuiltinSym(point, 0), got {:?}", other),
    }
}

#[test]
fn decode_buffer_op_save_current_buffer() {
    crate::test_utils::init_test_tracing();
    let bytecodes = vec![114, 135];
    let mut constants = vec![];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::SaveCurrentBuffer, Op::Return]);
    assert!(constants.is_empty());
}

#[test]
fn parse_arglist_descriptor_no_rest() {
    crate::test_utils::init_test_tracing();
    // 2 mandatory, 3 total → 1 optional
    let params = parse_arglist_descriptor(2 | (3 << 8));
    assert_eq!(params.required.len(), 2);
    assert_eq!(params.optional.len(), 1);
    assert!(params.rest.is_none());
}

#[test]
fn parse_arglist_descriptor_with_rest() {
    crate::test_utils::init_test_tracing();
    // 1 mandatory + &rest, with 1 non-rest slot total.
    let params = parse_arglist_descriptor(1 | (1 << 8) | 128);
    assert_eq!(params.required.len(), 1);
    assert_eq!(params.optional.len(), 0);
    assert!(params.rest.is_some());
}

#[test]
fn parse_arglist_descriptor_with_optional_and_rest_slot() {
    crate::test_utils::init_test_tracing();
    // GNU lexical bytecode can carry both optional args and a hidden rest slot.
    let params = parse_arglist_descriptor(3 | (4 << 8) | 128);
    assert_eq!(params.required.len(), 3);
    assert_eq!(params.optional.len(), 1);
    assert!(params.rest.is_some());
}

#[test]
fn parse_arglist_descriptor_zero_args() {
    crate::test_utils::init_test_tracing();
    let params = parse_arglist_descriptor(0);
    assert_eq!(params.required.len(), 0);
    assert_eq!(params.optional.len(), 0);
    assert!(params.rest.is_none());
}

#[test]
fn parse_arglist_value_from_list() {
    crate::test_utils::init_test_tracing();

    let arglist = Value::list(vec![
        Value::symbol("x"),
        Value::symbol("&optional"),
        Value::symbol("y"),
        Value::symbol("&rest"),
        Value::symbol("z"),
    ]);
    let params = parse_arglist_value(&arglist);
    assert_eq!(params.required.len(), 1);
    assert_eq!(resolve_sym(params.required[0]), "x");
    assert_eq!(params.optional.len(), 1);
    assert_eq!(resolve_sym(params.optional[0]), "y");
    assert!(params.rest.is_some());
    assert_eq!(resolve_sym(params.rest.unwrap()), "z");
}

#[test]
fn parse_arglist_value_int() {
    crate::test_utils::init_test_tracing();
    let params = parse_arglist_value(&Value::fixnum(1 | (2 << 8)));
    assert_eq!(params.required.len(), 1);
    assert_eq!(params.optional.len(), 1);
    assert!(params.rest.is_none());
}

// ---------------------------------------------------------------------------
// validate_gnu_bytecode: the decoder's verdict without its instructions
// ---------------------------------------------------------------------------

/// The decoder's verdict on `bytes`: `Ok` or its first error.
fn decode_verdict(bytes: &[u8]) -> Result<(), DecodeError> {
    let mut constants = Vec::new();
    decode_gnu_bytecode_with_offset_map(bytes, &mut constants).map(|_| ())
}

fn assert_same_verdict(bytes: &[u8]) {
    assert_eq!(
        validate_gnu_bytecode(bytes),
        decode_verdict(bytes),
        "validator and decoder disagree on {bytes:?}"
    );
}

#[test]
fn validate_matches_decode_on_every_error_kind() {
    crate::test_utils::init_test_tracing();
    let cases: &[(&[u8], Result<(), DecodeError>)] = &[
        (&[], Ok(())),
        (&[192, 135], Ok(())),
        // Unused and obsolete opcodes.
        (&[192, 128], Err(DecodeError::UnknownOpcode(128, 1))),
        (&[51], Err(DecodeError::UnknownOpcode(51, 0))),
        (&[141], Err(DecodeError::ObsoleteOpcode(141, 0))),
        (&[192, 144, 135], Err(DecodeError::ObsoleteOpcode(144, 1))),
        // Operands past the end: fetch1 and fetch2.
        (&[6], Err(DecodeError::UnexpectedEnd(0))),
        (&[192, 7, 1], Err(DecodeError::UnexpectedEnd(1))),
        (&[130, 0], Err(DecodeError::UnexpectedEnd(0))),
        // A jump to an instruction start, and to the end of the stream.
        (&[192, 131, 5, 0, 193, 135], Ok(())),
        (&[130, 3, 0], Ok(())),
        // A jump into the middle of an instruction, and past the end.
        (
            &[192, 131, 3, 0, 193, 135],
            Err(DecodeError::InvalidJumpTarget {
                target_byte_offset: 3,
                source_byte_offset: 1,
            }),
        ),
        (
            &[130, 4, 0],
            Err(DecodeError::InvalidJumpTarget {
                target_byte_offset: 4,
                source_byte_offset: 0,
            }),
        ),
        // Two bad jumps: the first in instruction order is reported.
        (
            &[130, 9, 0, 130, 2, 0, 135],
            Err(DecodeError::InvalidJumpTarget {
                target_byte_offset: 9,
                source_byte_offset: 0,
            }),
        ),
        // A bad jump before an undecodable instruction: the walk fails first.
        (&[130, 9, 0, 128], Err(DecodeError::UnknownOpcode(128, 3))),
        // Handler pushes are jumps too.
        (
            &[49, 1, 0, 135],
            Err(DecodeError::InvalidJumpTarget {
                target_byte_offset: 1,
                source_byte_offset: 0,
            }),
        ),
        (&[50, 3, 0, 135], Ok(())),
    ];
    for (bytes, want) in cases {
        assert_eq!(&decode_verdict(bytes), want, "decoder on {bytes:?}");
        assert_eq!(
            &validate_gnu_bytecode(bytes),
            want,
            "validator on {bytes:?}"
        );
    }
}

/// xorshift64*: a fixed-seed generator, so a failure reproduces.
struct Prng(u64);

impl Prng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[test]
fn validate_matches_decode_on_random_byte_strings() {
    crate::test_utils::init_test_tracing();
    let mut rng = Prng(0x9E37_79B9_7F4A_7C15);
    let mut accepted = 0usize;
    for _ in 0..100_000 {
        let len = rng.below(40) as usize;
        let mut bytes = Vec::with_capacity(len);
        while bytes.len() < len {
            // Bias toward jumps with in-range targets, so jump checks run
            // (and sometimes pass) instead of every string dying early.
            match rng.below(4) {
                0 => {
                    bytes.push(130 + rng.below(5) as u8);
                    bytes.push(rng.below(len as u64 + 2) as u8);
                    bytes.push(0);
                }
                1 => bytes.push(192 + rng.below(64) as u8),
                _ => bytes.push(rng.below(256) as u8),
            }
        }
        assert_same_verdict(&bytes);
        accepted += usize::from(validate_gnu_bytecode(&bytes).is_ok());
    }
    assert!(accepted > 1_000, "the corpus must exercise acceptance too");
}

/// Every compiled function a bootstrapped runtime holds (the preloaded
/// `.elc` files, plus bytecomp, byte-opt and cl-lib), nested closure
/// prototypes included, and truncations and bit flips of each.
#[test]
fn validate_matches_decode_on_real_elc_functions() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let functions = eval
        .eval_str(
            "(progn (require 'bytecomp) (require 'byte-opt) (require 'cl-lib)
               (let (acc)
                 (mapatoms (lambda (s)
                             (when (and (fboundp s)
                                        (byte-code-function-p (symbol-function s)))
                               (push (symbol-function s) acc))))
                 acc))",
        )
        .expect("collect compiled functions");
    crate::emacs_core::eval::push_scratch_gc_root(functions);

    let mut bodies: Vec<Vec<u8>> = Vec::new();
    let mut pending: Vec<Value> = crate::emacs_core::value::list_to_vec(&functions).unwrap();
    let mut seen = std::collections::HashSet::new();
    while let Some(f) = pending.pop() {
        if !seen.insert(f.bits()) {
            continue;
        }
        let data = f.get_bytecode_data().expect("byte-code function");
        if let Some(bytes) = &data.gnu_bytecode_bytes {
            bodies.push(bytes.to_vec());
        }
        pending.extend(
            data.constants
                .iter()
                .copied()
                .filter(|c| c.get_bytecode_data().is_some()),
        );
    }
    assert!(bodies.len() > 2_000, "only {} bodies", bodies.len());

    let mut rng = Prng(0xD1B5_4A32_D192_ED03);
    for body in &bodies {
        assert_eq!(validate_gnu_bytecode(body), Ok(()), "{body:?}");
        assert_same_verdict(body);
        if body.is_empty() {
            continue;
        }
        for _ in 0..6 {
            let cut = rng.below(body.len() as u64) as usize;
            assert_same_verdict(&body[..cut]);
            let mut flipped = body.clone();
            let at = rng.below(body.len() as u64) as usize;
            flipped[at] ^= 1 << rng.below(8);
            assert_same_verdict(&flipped);
        }
    }
}

/// `make-byte-code` validates without decoding: constructing a function
/// builds no instructions, its first call decodes it exactly once, and a
/// malformed body is still refused at construction with the decoder's
/// error text.
#[test]
fn make_byte_code_decodes_lazily_at_first_execution() {
    crate::test_utils::init_test_tracing();
    if crate::emacs_core::bytecode::chunk::eager_gnu_bytecode() {
        return;
    }
    let mut eval = crate::emacs_core::eval::Context::new();
    let full = super::full_decode_count_for_test;
    let lazy = crate::emacs_core::bytecode::chunk::lazy_gnu_decode_count_for_test;

    let (full0, lazy0) = (full(), lazy());
    let made = eval
        .eval_str(
            "(setq mbc-made (list (make-byte-code 0 \"\\300\\207\" [mbc-ran] 1)
                                  (make-byte-code 0 \"\\300\\301\\\\\\207\" [3 4] 2)))",
        )
        .expect("make-byte-code");
    crate::emacs_core::eval::push_scratch_gc_root(made);
    assert_eq!(full() - full0, 0, "construction must not decode");
    assert_eq!(lazy() - lazy0, 0);
    for f in crate::emacs_core::value::list_to_vec(&made).unwrap() {
        let data = f.get_bytecode_data().unwrap();
        assert!(data.ops.is_empty());
        assert!(data.lazy_gnu_code.is_some());
    }

    assert_eq!(
        eval.eval_str("(funcall (car mbc-made))").unwrap(),
        Value::symbol("mbc-ran")
    );
    assert_eq!((full() - full0, lazy() - lazy0), (1, 1));
    assert_eq!(
        eval.eval_str("(funcall (car mbc-made))").unwrap(),
        Value::symbol("mbc-ran")
    );
    assert_eq!((full() - full0, lazy() - lazy0), (1, 1), "decoded once");
    assert_eq!(
        eval.eval_str("(funcall (car (cdr mbc-made)))").unwrap(),
        Value::fixnum(7)
    );
    assert_eq!((full() - full0, lazy() - lazy0), (2, 2));

    let refused = eval.eval_str(
        "(condition-case err (make-byte-code 0 \"\\202\\011\\000\\207\" [] 1)
           (error err))",
    );
    assert_eq!(
        crate::emacs_core::error::format_eval_result(&refused),
        "OK (error \"bytecode decode error: jump target byte offset 9 not found \
         (from instruction at byte 0)\")"
    );
    assert_eq!(full() - full0, 2, "refusal must not decode either");
}
