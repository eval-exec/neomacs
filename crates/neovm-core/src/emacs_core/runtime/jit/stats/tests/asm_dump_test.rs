use super::asm_dump::force_asm_dump_for_test;
use super::perf_map::label_parts;
use super::{ObserveOverride, force_observe_for_test};
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::{LambdaParams, Value};

#[test]
fn jit_asm_dump_label_parts() {
    assert_eq!(label_parts("lisp:foo#12:mir"), Some(("foo", "12")));
    assert_eq!(label_parts("lisp:a#b#7:osr@3"), Some(("a#b", "7")));
    assert_eq!(label_parts("__neovm_mir_leaf"), None);
}

/// Under the knob, every JIT leaf's final machine code is appended: header
/// (label, id, tier, address, size), Cranelift's register-allocated
/// disassembly, and the finalized bytes.
#[test]
fn jit_asm_dump_appends_every_compiled_leaf() {
    // Exact native outcomes: immune to a NEOVM_JIT_FORCE_DEOPT=1 suite run.
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("jit.asm");
    force_asm_dump_for_test(Some(path.clone()));
    force_observe_for_test(ObserveOverride {
        naming: true,
        ..Default::default()
    });
    // (lambda (x) (+ x 1)) through the cache, so it is labelled with its id.
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return];
    f.constants = vec![Value::make_int(1)].into();
    f.max_stack = 16;
    let got = crate::emacs_core::jit::try_run_compiled(
        std::ptr::null_mut(),
        &f,
        Value::NIL,
        &[Value::make_int(41)],
    )
    .expect("runs");
    assert_eq!(got, Some(Value::make_int(42).bits()));
    let id = f.jit_runtime().compiled_id().expect("compiled");
    // And a baseline leaf built outside the cache (no label).
    let leaf = crate::emacs_core::jit::compile::lower_nullary_leaf(
        &[Op::Constant(0), Op::Return],
        &[Value::make_int(7)],
    )
    .expect("compiles");

    let text = std::fs::read_to_string(&path).expect("dump written");
    let headers: Vec<&str> = text.lines().filter(|l| l.starts_with(";; ==== ")).collect();
    assert_eq!(headers.len(), 2, "{text}");
    let first = headers[0];
    assert!(first.starts_with(";; ==== lisp:anon#"), "{first}");
    assert!(first.contains(&format!(" id={id} ")), "{first}");
    assert!(
        first.contains(" tier=mir ") || first.contains(" tier=baseline "),
        "{first}"
    );
    assert!(first.contains(" addr=0x"), "{first}");
    let second = headers[1];
    assert!(
        second.starts_with(";; ==== __neovm_jit_leaf name=- id=- tier=baseline "),
        "{second}"
    );
    assert!(
        second.contains(&format!(" addr={:#x} ", leaf.entry as usize)),
        "{second}"
    );
    // Register-allocated x86-64 (or aarch64) disassembly, then the bytes.
    assert!(
        text.contains("%rbp") || text.contains("%rsp") || text.contains("ret"),
        "{text}"
    );
    assert_eq!(text.matches(";; bytes:\n").count(), 2, "{text}");

    // Knob off: nothing more is written.
    force_asm_dump_for_test(None);
    crate::emacs_core::jit::compile::lower_nullary_leaf(
        &[Op::Constant(0), Op::Return],
        &[Value::make_int(8)],
    )
    .expect("compiles");
    assert_eq!(std::fs::read_to_string(&path).expect("dump"), text);
}
