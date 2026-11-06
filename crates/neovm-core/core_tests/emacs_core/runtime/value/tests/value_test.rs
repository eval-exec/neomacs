use super::*;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::eval::Context;
use malachite::integer::Integer;

#[test]
fn equal_compares_char_tables_by_gnu_pseudovector_slots() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let result = eval
        .eval_str(
            r#"
(let* ((a (make-char-table 'neomacs-equal nil))
       (b (make-char-table 'neomacs-equal nil))
       (different (make-char-table 'neomacs-equal nil))
       (parent-a (make-char-table 'neomacs-equal-parent nil))
       (parent-b (make-char-table 'neomacs-equal-parent nil))
       (child-a (make-char-table 'neomacs-equal nil))
       (child-b (make-char-table 'neomacs-equal nil))
       (props-a (make-char-table 'neomacs-equal nil))
       (props-b (make-char-table 'neomacs-equal nil))
       (cycle-a (make-char-table 'neomacs-equal nil))
       (cycle-b (make-char-table 'neomacs-equal nil)))
  (set-char-table-range a #x1f600 'same)
  (set-char-table-range b #x1f600 'same)
  (set-char-table-range different #x1f600 'different)
  (set-char-table-range parent-a ?a 'parent)
  (set-char-table-range parent-b ?a 'parent)
  (set-char-table-parent child-a parent-a)
  (set-char-table-parent child-b parent-b)
  (set-char-table-range props-a ?x (propertize "v" 'face 'bold))
  (set-char-table-range props-b ?x (propertize "v" 'face 'italic))
  (set-char-table-range cycle-a nil cycle-a)
  (set-char-table-range cycle-b nil cycle-b)
  (list
   (equal (make-char-table 'neomacs-equal nil)
          (make-char-table 'neomacs-equal nil))
   (equal (make-char-table 'one nil)
          (make-char-table 'two nil))
   (equal a b)
   (equal a different)
   (equal child-a child-b)
   (equal props-a props-b)
   (equal-including-properties props-a props-b)
   (equal cycle-a cycle-b)))"#,
        )
        .expect("char-table equality probe should evaluate");

    assert_eq!(
        result,
        Value::list(vec![
            Value::T,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::T,
            Value::T,
            Value::NIL,
            Value::T,
        ])
    );
}

#[test]
fn equal_char_tables_share_hashes_and_equal_hash_table_keys() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let result = eval
        .eval_str(
            r#"
(let ((left (make-char-table 'neomacs-equal-hash nil))
      (right (make-char-table 'neomacs-equal-hash nil))
      (table (make-hash-table :test 'equal)))
  (set-char-table-range left #x1f600 'same)
  (set-char-table-range right #x1f600 'same)
  (puthash left 'found table)
  (list
   (equal left right)
   (= (sxhash-equal left) (sxhash-equal right))
   (gethash right table 'missing)))"#,
        )
        .expect("char-table hash contract probe should evaluate");

    assert_eq!(
        result,
        Value::list(vec![Value::T, Value::T, Value::symbol("found")])
    );
}

#[test]
fn equal_on_circular_structures_matches_gnu_cycle_semantics() {
    // GNU 31.0.90 `equal` (internal_equal_1 / internal_equal_cycle) detects cycles
    // and returns t/nil instead of signaling `circular-list`.  Regression for the
    // cons-loop tortoise-hare that used to error on circular structures; goes
    // through the lisp `equal` (try_equal_value_inner), not the bool equal_value.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    // (form, expected truthy)
    let cases: [(&str, bool); 5] = [
        // two separate self-circular lists, equal cars -> t
        (
            "(let ((a (list 'x)) (b (list 'x))) (setcdr a a) (setcdr b b) (equal a b))",
            true,
        ),
        // self-circular lists, differing cars -> nil
        (
            "(let ((a (list 'x)) (c (list 'y))) (setcdr a a) (setcdr c c) (equal a c))",
            false,
        ),
        // two separate self-circular vectors, equal -> t
        (
            "(let ((v1 (vector 'x nil)) (v2 (vector 'x nil))) (aset v1 1 v1) (aset v2 1 v2) (equal v1 v2))",
            true,
        ),
        // self-circular vectors, differing -> nil
        (
            "(let ((v1 (vector 'x nil)) (v2 (vector 'y nil))) (aset v1 1 v1) (aset v2 1 v2) (equal v1 v2))",
            false,
        ),
        // circular left vs finite right -> nil (right terminates first)
        (
            "(let ((a (list 'x))) (setcdr a a) (equal a (list 'x 'x 'x)))",
            false,
        ),
    ];
    for (src, truthy) in cases {
        let got = ev
            .eval_str(src)
            .ok()
            .unwrap_or_else(|| panic!("`equal` signaled instead of returning for: {src}"));
        let is_truthy = got.bits() != Value::NIL.bits();
        assert_eq!(is_truthy, truthy, "wrong `equal` result for: {src}");
    }
}

/// GNU `internal_equal_1` (src/fns.c:2984-2998 in emacs-31.1, :2987-3001 on
/// master) compares a `PVEC_CLOSURE` exactly like a vector: the size check
/// first (`ASIZE`, so a five-slot closure never equals a four-slot one), then
/// every slot element-wise -- arglist, bytecode string, constants vector,
/// max depth, doc, interactive spec, and any extras.  `sxhash_obj`
/// (:5525-5536) hashes the same slots through `sxhash_vector`, so two `equal`
/// closures land in the same `equal` hash-table bucket.
///
/// The Lisp-visible consequence lsp-mode depends on: it removes the request
/// cancel closure it put on the global `post-command-hook` by rebuilding an
/// `equal` closure and calling `remove-hook` -> `delete`.  When byte-code
/// objects compare only by identity, that hook is never removed, fires on
/// every later command, and keeps sending `$/cancelRequest` to a server that
/// has since been shut down ("Sending to process failed ... not running").
///
/// Expected values are GNU Emacs 32's for this exact probe.
#[test]
fn equal_compares_byte_code_functions_element_wise_like_gnu_closures() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let result = eval
        .eval_str(
            r#"
(let* ((a (make-byte-code 257 "\300\207" [42] 2))
       (b (make-byte-code 257 "\300\207" [42] 2))
       (c (make-byte-code 257 "\300\207" [43] 2))
       (d (make-byte-code 257 "\300\207" [42] 3))
       (e (make-byte-code 257 "\300\207" [42] 2 "doc"))
       (f (make-byte-code 257 "\300\207" [42] 2 "doc"))
       (g (make-byte-code 257 "\300\207" [42] 2 nil))
       (h (make-byte-code 257 "\300\207" [42] 2 nil (list 'interactive "p")))
       (i (make-byte-code 257 "\300\207" [42] 2 nil (list 'interactive "p")))
       (j (make-byte-code 257 "\301\207" [42] 2))
       (k (make-byte-code '(x) "\300\207" [42] 2))
       (proto (make-byte-code 257 "\300\207" [placeholder] 2))
       (k1 (make-closure proto 'w))
       (k2 (make-closure proto 'w))
       (k3 (make-closure proto 'z))
       (table (make-hash-table :test 'equal))
       (v (vector 257 "\300\207" [42] 2)))
  (puthash a 'found table)
  (list (equal a b)                       ; same slots, distinct objects
        (equal a c)                       ; constants differ
        (equal a d)                       ; max depth differs
        (equal a e)                       ; doc slot differs
        (equal e f)                       ; same doc
        (equal a g)                       ; GNU ASIZE: 4 slots vs 5 slots
        (equal h i)                       ; same interactive spec
        (equal a j)                       ; bytecode differs
        (equal a k)                       ; arglist differs
        (equal k1 k2)                     ; make-closure: same captures
        (equal k1 k3)                     ; make-closure: different capture
        (eq k1 k2)
        (equal a v)                       ; a plain vector is not a closure
        (equal (list a 1) (list b 1))
        (length (delete b (list a 1)))    ; what remove-hook does
        (eq (car (member b (list 1 a))) a)
        (= (sxhash-equal a) (sxhash-equal b))
        (= (sxhash-equal k1) (sxhash-equal k2))
        (gethash b table)
        (gethash k1 (let ((tb (make-hash-table :test 'equal)))
                      (puthash k2 'closure-found tb)
                      tb))))"#,
        )
        .expect("byte-code equality probe should evaluate");

    assert_eq!(
        list_to_vec(&result).expect("probe returns a list"),
        vec![
            Value::T,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::fixnum(1),
            Value::T,
            Value::T,
            Value::T,
            Value::symbol("found"),
            Value::symbol("closure-found"),
        ]
    );
}

/// Helper: set up a temporary heap for tests that use Value constructors.
/// With the tagged-pointer runtime the test fallback heap is auto-created,
/// so this wrapper is now a simple pass-through.
fn with_test_heap<R>(f: impl FnOnce() -> R) -> R {
    f()
}

/// Foundation smoke test for bignum support: a value bigger than the
/// 62-bit fixnum range must round-trip through `Value::make_integer`,
/// classify as `integerp` / `bignump` (and *not* `fixnump`), and print
/// back to its decimal text. Mirrors GNU `make_integer_mpz`
/// (`src/bignum.c:146`).
#[test]
fn bignum_constructor_and_predicates() {
    crate::test_utils::init_test_tracing();
    with_test_heap(|| {
        // Pick a value that can never fit in fixnum: 2^100.
        let mut huge = Integer::from(1);
        huge <<= 100;
        let big = Value::make_integer(huge.clone());
        assert!(big.is_bignum(), "expected bignum, got {:?}", big.kind());
        assert!(big.is_integer(), "bignum should satisfy integerp");
        assert!(big.is_number(), "bignum should satisfy numberp");
        assert!(!big.is_fixnum(), "bignum must not be a fixnum");
        assert_eq!(big.type_name(), "integer");

        let borrowed = big.as_bignum().expect("as_bignum");
        assert_eq!(*borrowed, huge);
        assert_eq!(
            crate::emacs_core::print::print_value(&big),
            "1267650600228229401496703205376"
        );

        // Values that fit must come back as fixnums, not bignums.
        let small = Value::make_integer(Integer::from(42));
        assert!(small.is_fixnum());
        assert!(!small.is_bignum());
        assert_eq!(small.as_fixnum(), Some(42));
    });
}

/// Build a byte-code object the way the VM and `make-closure` do, so the
/// probes below can reach states `make-byte-code` cannot: a captured
/// lexical environment (`Op::MakeClosure` stores `Some(lexenv)`, and that
/// environment is `nil` for a closure over no variables), a docstring in
/// either string representation, and a function with no retained GNU
/// bytecode string.
fn byte_code_probe(
    env: Option<Value>,
    docstring: Option<crate::heap_types::LispString>,
    gnu_bytes: Option<Vec<u8>>,
    ops: Vec<crate::tagged::bytecode::opcode::Op>,
    constants: Vec<Value>,
) -> Value {
    use crate::tagged::bytecode::chunk::ByteCodeFunction;
    // SAFETY: the probe deliberately builds states `make-byte-code` cannot
    // reach (sealed flag without the normalizer, custom lexenv/docstring), so
    // it uses the signed from_parts escape instead of widening private fields.
    Value::make_bytecode(unsafe {
        ByteCodeFunction::from_parts(
            crate::tagged::bytecode::chunk::fresh_bytecode_source_id(),
            ops,
            true,
            false,
            constants.into(),
            crate::tagged::bytecode::function_slots::StackDepth::try_from(1usize)
                .expect("StackDepth::for_test depth is a valid GNU stack depth"),
            LambdaParams::simple(vec![]).into(),
            Value::fixnum(257),
            true,
            env,
            None,
            gnu_bytes.map(crate::tagged::header::LispByteVec::owned),
            docstring,
            None,
            None,
            4,
            Vec::new(),
        )
    })
}

/// `(equal A B)`, whether `sxhash-equal` agrees, and whether an `equal`
/// hash table keyed by A finds B -- the three surfaces that must agree.
fn byte_code_probe_agreement(eval: &mut Context, a: Value, b: Value) -> (bool, bool, bool) {
    eval.obarray_mut().set_symbol_value("neomacs-probe-a", a);
    eval.obarray_mut().set_symbol_value("neomacs-probe-b", b);
    let result = eval
        .eval_str(
            r#"
(list (equal neomacs-probe-a neomacs-probe-b)
      (= (sxhash-equal neomacs-probe-a) (sxhash-equal neomacs-probe-b))
      (let ((table (make-hash-table :test 'equal)))
        (puthash neomacs-probe-a 'found table)
        (eq (gethash neomacs-probe-b table) 'found)))"#,
        )
        .expect("probe should evaluate");
    let values = list_to_vec(&result).expect("probe list");
    (
        !values[0].is_nil(),
        !values[1].is_nil(),
        !values[2].is_nil(),
    )
}

fn simple_ops() -> Vec<crate::tagged::bytecode::opcode::Op> {
    use crate::tagged::bytecode::opcode::Op;
    vec![Op::Constant(0), Op::Return]
}

/// A closure that captured an empty lexical environment (`env: Some(nil)`)
/// is observably different from a function with no environment (`aref`
/// slot 2 answers nil versus the constants vector), so `equal` says so --
/// and the `equal`-table key must say the same, or two unequal objects
/// share a bucket.
#[test]
fn equal_table_keys_keep_the_presence_of_a_captured_environment() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let consts = || vec![Value::fixnum(42)];
    let bytes = || Some(vec![0xC0, 0x87]);

    let absent = byte_code_probe(None, None, bytes(), simple_ops(), consts());
    let captured_nil = byte_code_probe(Some(Value::NIL), None, bytes(), simple_ops(), consts());
    let captured_nil_twin =
        byte_code_probe(Some(Value::NIL), None, bytes(), simple_ops(), consts());
    let captured_w = byte_code_probe(
        Some(Value::symbol("w")),
        None,
        bytes(),
        simple_ops(),
        consts(),
    );

    assert_eq!(
        byte_code_probe_agreement(&mut eval, absent, captured_nil),
        (false, false, false),
        "no environment vs a captured nil environment"
    );
    assert_eq!(
        byte_code_probe_agreement(&mut eval, captured_nil, captured_nil_twin),
        (true, true, true)
    );
    assert_eq!(
        byte_code_probe_agreement(&mut eval, captured_nil, captured_w),
        (false, false, false),
        "the captured environment participates in equal, sxhash and the key"
    );
}

/// Structural key evidence must be domain-tagged rather than represented by
/// Lisp-reachable sentinel contents. `ByteCodeStructuralPart::Absent` used to become
/// the same `HashKey::Text` as a present string containing `"#<absent>"`, so
/// the hash table treated two objects that `equal` rejected as one key.
#[test]
fn equal_table_keys_do_not_alias_absence_with_lisp_values() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let consts = || vec![Value::fixnum(42)];
    let bytes = || Some(vec![0xC0, 0x87]);

    let absent = byte_code_probe(None, None, bytes(), simple_ops(), consts());
    let sentinel_text = byte_code_probe(
        Some(Value::string("#<absent>")),
        None,
        bytes(),
        simple_ops(),
        consts(),
    );

    let (equal, _same_hash, table_found) =
        byte_code_probe_agreement(&mut eval, absent, sentinel_text);
    assert!(!equal, "an absent environment is not a present Lisp value");
    assert!(
        !table_found,
        "an equal table must not alias typed absence with Lisp string contents"
    );
}

/// GNU string equality is character count, byte count and contents
/// (src/fns.c `internal_equal_1`, Lisp_String); the unibyte/multibyte
/// representation flag is not part of it.  An ASCII docstring stored
/// unibyte equals the same text stored multibyte -- verified with GNU
/// Emacs: `(equal (make-byte-code 257 "\300\207" [42] 2 "doc")
/// (make-byte-code 257 "\300\207" [42] 2 (string-to-multibyte "doc")))` is t
/// -- while a raw-byte unibyte string and the multibyte string with the
/// same bytes differ in character count and are not equal.
#[test]
fn docstrings_compare_with_gnu_string_equality_not_representation() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    use crate::heap_types::LispString;
    let consts = || vec![Value::fixnum(42)];
    let bytes = || Some(vec![0xC0, 0x87]);

    let unibyte = byte_code_probe(
        None,
        Some(LispString::from_unibyte(b"doc".to_vec())),
        bytes(),
        simple_ops(),
        consts(),
    );
    let multibyte = byte_code_probe(
        None,
        Some(LispString::new("doc".to_owned(), true)),
        bytes(),
        simple_ops(),
        consts(),
    );
    assert_eq!(
        byte_code_probe_agreement(&mut eval, unibyte, multibyte),
        (true, true, true),
        "same characters, same bytes: equal in GNU"
    );

    let raw_bytes = byte_code_probe(
        None,
        Some(LispString::from_unibyte(vec![0xC3, 0xA9])),
        bytes(),
        simple_ops(),
        consts(),
    );
    let e_acute = byte_code_probe(
        None,
        Some(LispString::new("\u{e9}".to_owned(), true)),
        bytes(),
        simple_ops(),
        consts(),
    );
    // GNU `sxhash_string` hashes the bytes, so these two collide in the
    // hash -- legal for unequal objects -- while `equal` and the table key
    // both tell them apart by character count.
    assert_eq!(
        byte_code_probe_agreement(&mut eval, raw_bytes, e_acute),
        (false, true, false),
        "two raw bytes are two characters; one multibyte character is one"
    );
}

/// A function with no retained GNU bytecode string (the `byte-code` subr's
/// transient functions, pdump stubs) has only its decoded instructions to
/// compare: equal instructions and constants are equal, and different
/// instructions are not, even though GNU's slot 1 would be nil for both.
#[test]
fn functions_without_gnu_bytes_compare_their_decoded_instructions() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    use crate::tagged::bytecode::opcode::Op;
    let consts = || vec![Value::fixnum(42)];

    let a = byte_code_probe(None, None, None, simple_ops(), consts());
    let b = byte_code_probe(None, None, None, simple_ops(), consts());
    let other_ops = byte_code_probe(
        None,
        None,
        None,
        vec![Op::Constant(0), Op::Constant(0), Op::Return],
        consts(),
    );
    let with_bytes = byte_code_probe(None, None, Some(vec![0xC0, 0x87]), simple_ops(), consts());

    assert_eq!(
        byte_code_probe_agreement(&mut eval, a, b),
        (true, true, true)
    );
    assert_eq!(
        byte_code_probe_agreement(&mut eval, a, other_ops),
        (false, false, false)
    );
    assert_eq!(
        byte_code_probe_agreement(&mut eval, a, with_bytes),
        (false, false, false),
        "retained bytes versus none is a different object"
    );
}

/// GNU only promises that `equal` byte-code objects have equal hashes;
/// collisions between unequal objects are valid. Exercise that implication
/// for both constants and captured environments, and independently require
/// the equal-table key to distinguish the unequal pairs.
#[test]
fn sxhash_equal_obeys_the_contract_for_constants_and_captured_environments() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let bytes = || Some(vec![0xC0, 0x87]);

    let base = byte_code_probe(
        Some(Value::NIL),
        None,
        bytes(),
        simple_ops(),
        vec![Value::fixnum(1)],
    );
    let base_twin = byte_code_probe(
        Some(Value::NIL),
        None,
        bytes(),
        simple_ops(),
        vec![Value::fixnum(1)],
    );
    let other_constants = byte_code_probe(
        Some(Value::NIL),
        None,
        bytes(),
        simple_ops(),
        vec![Value::fixnum(2)],
    );
    let other_constants_twin = byte_code_probe(
        Some(Value::NIL),
        None,
        bytes(),
        simple_ops(),
        vec![Value::fixnum(2)],
    );
    let other_env = byte_code_probe(
        Some(Value::list(vec![Value::cons(
            Value::symbol("x"),
            Value::fixnum(1),
        )])),
        None,
        bytes(),
        simple_ops(),
        vec![Value::fixnum(1)],
    );
    let other_env_twin = byte_code_probe(
        Some(Value::list(vec![Value::cons(
            Value::symbol("x"),
            Value::fixnum(1),
        )])),
        None,
        bytes(),
        simple_ops(),
        vec![Value::fixnum(1)],
    );

    assert_eq!(
        byte_code_probe_agreement(&mut eval, base, base_twin),
        (true, true, true)
    );
    assert_eq!(
        byte_code_probe_agreement(&mut eval, other_constants, other_constants_twin),
        (true, true, true)
    );
    assert_eq!(
        byte_code_probe_agreement(&mut eval, other_env, other_env_twin),
        (true, true, true)
    );

    let (equal, _same_hash, table_found) =
        byte_code_probe_agreement(&mut eval, base, other_constants);
    assert!(!equal && !table_found, "constants participate in structure");

    let (equal, _same_hash, table_found) = byte_code_probe_agreement(&mut eval, base, other_env);
    assert!(
        !equal && !table_found,
        "the environment participates in structure"
    );
}

#[path = "hash_probe_test.rs"]
mod hash_probe;
#[path = "jump_table_lookup_test.rs"]
pub(super) mod jump_table_lookup;
