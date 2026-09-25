use super::*;
use object::{Object, ObjectSymbol};

/// R1c-1 gate: a pure leaf's object bytes parse via `object::File`, the entry
/// symbol is exported (defined + global), and the `neovm_jit_*` shims appear
/// as UNDEFINED imports (resolved by the loader, not baked).
#[test]
fn object_emits_with_exported_entry_and_imported_shims() {
    // A 0-arg pure body that conses two fixnums and RETURNS the cons. A
    // returned cons escapes → escape analysis keeps a real heap allocation →
    // needs_rt → the cons shim (+ gc_save/push/restore) declared as imports.
    // Fixnum constants need no reloc vector, keeping this body minimal.
    let ops = [Op::Constant(0), Op::Constant(1), Op::Cons, Op::Return];
    let constants = [Value::make_int(1), Value::make_int(2)];
    let m = mir::build_mir(&ops, &constants, None, 0).expect("build_mir for cons body");

    let entry = "__neovm_aot_test_cons";
    let bytes = build_object_for_leaf(&m, entry).expect("emit object");
    assert!(!bytes.is_empty(), "object bytes must be non-empty");

    let file = object::File::parse(&*bytes).expect("parse object bytes");

    // Entry symbol: defined (not undefined) and global.
    let entry_sym = file
        .symbols()
        .find(|s| s.name() == Ok(entry))
        .unwrap_or_else(|| panic!("entry symbol {entry} not found"));
    assert!(
        entry_sym.is_definition(),
        "entry {entry} must be a definition (exported)"
    );
    assert!(entry_sym.is_global(), "entry {entry} must be global");

    // The cons shim must appear as an UNDEFINED import (the loader resolves it
    // against the host; AOT never bakes the shim address).
    let cons_shim = file
        .symbols()
        .find(|s| s.name() == Ok("neovm_jit_cons"))
        .expect("neovm_jit_cons import symbol present");
    assert!(
        cons_shim.is_undefined(),
        "shim neovm_jit_cons must be an undefined import"
    );
}

/// R2 increment A (CBSym-in-AOT) — the object-level before/after proof. The
/// baseline AOT emit now classifies CallBuiltinSym intrinsics at `obarray=None`
/// (their classification is name-canonical + obarray-free), so the emitted
/// object imports the Tier-A read shim (`neovm_jit_cbsym_read`) / Tier-B
/// dispatch-skip shim (`neovm_jit_cbsym_spec`). BEFORE this increment the same
/// body imported ONLY the general `neovm_jit_named_builtin` — so the PRESENCE of
/// the fast-shim import is the proof classification engaged. (The general shim
/// is STILL imported for the per-site NEED_GENERIC fallback, so this asserts the
/// fast shim's presence, not the general shim's absence.) The emit's own
/// `assert_aot_imports_exported` also proves the two shims are in the exported +
/// salted `MIR_SHIM_NAMES` set (else the emit would error).
#[test]
fn baseline_cbsym_object_imports_the_fast_intrinsic_shims() {
    use crate::emacs_core::intern::intern;
    // Populate the thread-local static subr table (`lookup_global_subr_entry`,
    // which `cbsym_spec_kind` consults) — populated from `SubrSpec`s during
    // Context setup.
    let _ev = crate::emacs_core::eval::Context::new();

    let imports = |ops: &[Op], constants: &[Value], arity: usize| -> Vec<String> {
        let (obj, _hash) = build_baseline_object_for_leaf(ops, constants, arity, None)
            .expect("baseline emit ok")
            .expect("baseline emit produced an object");
        let file = object::File::parse(&*obj).expect("parse object");
        file.symbols()
            .filter(|s| s.is_undefined())
            .filter_map(|s| s.name().ok().map(str::to_owned))
            .collect()
    };

    // Tier-A: (point) → neovm_jit_cbsym_read.
    let point_ops = [Op::CallBuiltinSym(intern("point"), 0), Op::Return];
    let a = imports(&point_ops, &[], 0);
    assert!(
        a.iter().any(|n| n == "neovm_jit_cbsym_read"),
        "Tier-A (point) AOT object must import neovm_jit_cbsym_read (classification did not engage at obarray=None) — imports: {a:?}"
    );

    // Tier-B: (length x) → neovm_jit_cbsym_spec.
    let length_ops = [
        Op::StackRef(0),
        Op::CallBuiltinSym(intern("length"), 1),
        Op::Return,
    ];
    let b = imports(&length_ops, &[], 1);
    assert!(
        b.iter().any(|n| n == "neovm_jit_cbsym_spec"),
        "Tier-B (length) AOT object must import neovm_jit_cbsym_spec (classification did not engage at obarray=None) — imports: {b:?}"
    );
}

/// SCRATCH validation (not a final gate): prove the FULL pure-subset path —
/// emit `.o` → `cc -shared` → `dlopen` → `dlsym` → call — produces native
/// code byte-identical to the JIT (`lower_mir_pure`). Uses a PURE arithmetic
/// leaf with NO heap constants and NO calls, so the lowering bakes ZERO
/// session-specific addresses (no reloc_base, no precise-deopt buffers) and
/// the `.o` is directly runnable across the emit→load boundary. This is the
/// foundation R1c-5 builds on; reloc/precise-deopt rebuild come later.
#[cfg(target_os = "linux")]
#[test]
fn aot_pure_arith_leaf_matches_jit_via_dlopen() {
    use crate::emacs_core::eval::Context;
    use std::io::Write;

    // 1-arg pure body: (+ arg 5). No heap consts, no calls, no deopt buffers.
    let ops = [Op::Constant(0), Op::Add, Op::Return];
    let constants = [Value::make_int(5)];
    let m = mir::build_mir(&ops, &constants, None, 1).expect("build_mir add body");

    // Reference: the JIT leaf for the same MIR.
    let jit_leaf = super::super::compile::lower_mir_pure(&m).expect("JIT lowers");

    // Emit the AOT object for the same MIR.
    let entry = "__neovm_aot_test_add5";
    let obj = build_object_for_leaf(&m, entry).expect("emit object");

    // Write the `.o`, link to a `.so` with `cc -shared`.
    let dir = tempfile::tempdir().expect("tempdir");
    let o_path = dir.path().join("leaf.o");
    let so_path = dir.path().join("libleaf.so");
    std::fs::File::create(&o_path)
        .and_then(|mut f| f.write_all(&obj))
        .expect("write .o");
    let status = std::process::Command::new("cc")
        .arg("-shared")
        .arg("-o")
        .arg(&so_path)
        .arg(&o_path)
        .status()
        .expect("spawn cc");
    assert!(status.success(), "cc -shared failed");

    // dlopen + dlsym the entry (the unified 4-param CompiledLeaf entry ABI:
    // the 4th arg is the *const LeafSidecar — null here since this is a PURE
    // leaf that bakes its bases and never reads the sidecar).
    type EntryFn =
        unsafe extern "C" fn(*mut u8, *const i64, *mut i64, *const core::ffi::c_void) -> i64;
    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen .so");
    let aot_entry: libloading::Symbol<EntryFn> =
        unsafe { lib.get(entry.as_bytes()) }.expect("dlsym entry");

    // Call AOT and JIT for several args; results must be bit-identical.
    let mut eval = Context::new_minimal_vm_harness();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    for a in [0i64, 1, 7, -3, 1000] {
        let arg = Value::make_int(a);
        // JIT result.
        let jit = match jit_leaf.call(ctx_ptr, &[arg]) {
            crate::emacs_core::jit::compile::NativeRun::Ok(bits) => bits as i64,
            other => panic!("JIT did not return Ok: {other:?}"),
        };
        // AOT result via the raw entry ABI (one arg word, out slot, null
        // sidecar — pure leaf ignores it).
        let args = [arg.bits() as i64];
        let mut out: i64 = 0;
        let status = unsafe {
            (aot_entry)(
                ctx_ptr,
                args.as_ptr(),
                &mut out as *mut i64,
                core::ptr::null(),
            )
        };
        assert_eq!(
            status,
            super::super::compile::STATUS_OK,
            "AOT status not OK"
        );
        assert_eq!(out, jit, "AOT result != JIT result for arg {a}");
    }
}

/// R1c-3 gate: a recipe round-trips a const value with a string + symbol +
/// (nested) cons — emit recipe → rebuild fresh against the live heap/obarray
/// → leaves match by VALUE (not pointer). Needs a VM harness for allocation.
#[test]
fn recipe_round_trips_string_symbol_cons() {
    // The harness installs the thread-local heap so Value::string/cons can
    // allocate (same pattern as the compile.rs MIR const tests).
    let mut _eval = crate::emacs_core::eval::Context::new_minimal_vm_harness();

    // (cons "hello" (cons 'my-sym 42)) — exercises every supported recipe arm.
    let inner = Value::cons(
        Value::symbol(crate::emacs_core::intern::intern("my-sym")),
        Value::make_int(42),
    );
    let original = Value::cons(Value::string("hello"), inner);

    let mut recipe = Vec::new();
    write_value_recipe(&mut recipe, original).expect("string+symbol+cons supported");
    let (rebuilt, consumed) = rebuild_value(&recipe, 0).expect("valid recipe rebuilds");
    assert_eq!(consumed, recipe.len(), "recipe fully consumed");

    // Structurally equal by value (fresh allocations, so NOT `eq`).
    assert!(rebuilt.is_cons(), "top is a cons");
    assert_eq!(
        rebuilt.cons_car().as_lisp_string().unwrap().as_bytes(),
        b"hello"
    );
    assert_eq!(
        crate::emacs_core::intern::resolve_sym(
            rebuilt.cons_cdr().cons_car().as_symbol_id().unwrap()
        ),
        "my-sym"
    );
    assert_eq!(rebuilt.cons_cdr().cons_cdr().as_fixnum(), Some(42));

    // A float is outside the supported subset → recipe bails (caller → JIT).
    let mut tmp = Vec::new();
    assert!(
        write_value_recipe(&mut tmp, Value::make_float(1.5)).is_err(),
        "float must be unsupported (bail to JIT)"
    );
}

/// R1c-2 gate: the content hash is STABLE for identical source and
/// DISCRIMINATES different bodies / arities / constants; the entry symbol
/// round-trips the hash + ABI_TAG.
#[test]
fn content_hash_stable_and_discriminating() {
    let ops_a = [Op::Constant(0), Op::Add, Op::Return];
    let consts_a = [Value::make_int(5)];
    let h1 = leaf_content_hash(&ops_a, &consts_a, 1).expect("hashable");
    let h2 = leaf_content_hash(&ops_a, &consts_a, 1).expect("hashable");
    assert_eq!(h1, h2, "same source → same hash");

    // Different constant.
    let consts_b = [Value::make_int(6)];
    assert_ne!(
        h1,
        leaf_content_hash(&ops_a, &consts_b, 1).expect("hashable"),
        "different constant → different hash"
    );
    // Different arity (lambda-list drift).
    assert_ne!(
        h1,
        leaf_content_hash(&ops_a, &consts_a, 2).expect("hashable"),
        "different arity → different hash"
    );
    // Different ops.
    let ops_c = [Op::Constant(0), Op::Sub, Op::Return];
    assert_ne!(
        h1,
        leaf_content_hash(&ops_c, &consts_a, 1).expect("hashable"),
        "different ops → different hash"
    );

    // Entry symbol round-trips hash + tag.
    let sym = aot_entry_symbol(h1);
    assert!(sym.starts_with("__neovm_aot_"));
    assert!(sym.ends_with(&format!("{ABI_TAG:08x}")));
    assert!(sym.contains(&format!("{h1:032x}")));
}

/// The LIVE reloc set a test must pass to `load_leaf_from_unit` (audit #A):
/// the function's own constant objects, re-collected in emit-time order.
#[cfg(target_os = "linux")]
fn live_reloc_for(ops: &[Op], constants: &[Value], arity: usize) -> Vec<Value> {
    let m = mir::build_mir(ops, constants, None, arity).expect("mir for live reloc");
    collect_reloc_consts(&m)
}

/// Shared R1c-5/R1c-9 harness: for one pure body, build → link → load the
/// AOT leaf and assert interp == JIT == AOT (bit-for-bit) over `args`.
#[cfg(target_os = "linux")]
fn assert_aot_matches_interp_and_jit(ops: &[Op], constants: &[Value], nargs: usize, args: &[i64]) {
    use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::value::LambdaParams;

    // Emit → link → dlopen → load via the production helpers.
    let (obj, content_hash) = compile_leaf_to_object(ops, constants, nargs, None)
        .expect("compile ok")
        .expect("pure subset → Some");
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir
        .path()
        .join(format!("{content_hash:032x}_{ABI_TAG:08x}.so"));
    link_object_to_so(&obj, &so_path).expect("link .so");
    // SAFETY: dlopen a `.so` we just emitted; pure leaf has no shim imports.
    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    let unit = std::sync::Arc::new(super::super::compile::LoadedUnit::new(lib));
    let aot_leaf = load_leaf_from_unit(
        &unit,
        content_hash,
        nargs,
        &live_reloc_for(ops, constants, nargs),
        None,
    )
    .expect("load leaf from unit");

    // Reference: JIT leaf for the same MIR.
    let m = mir::build_mir(ops, constants, None, nargs).expect("mir");
    let jit_leaf = super::super::compile::lower_mir_pure(&m).expect("jit lowers");

    // Reference: the interpreter (the oracle).
    let mut eval = Context::new_minimal_vm_harness();
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..nargs)
            .map(|i| crate::emacs_core::intern::SymId(1 + i as u32))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops.to_vec();
    f.constants = constants.to_vec().into();
    f.max_stack = 16;

    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    // For a 1-arg body sweep `args`; otherwise call once with the first
    // `nargs` entries (the corpus bodies below are 0- or 1-arg).
    let calls: Vec<Vec<Value>> = if nargs == 1 {
        args.iter().map(|&a| vec![Value::make_int(a)]).collect()
    } else {
        vec![
            args.iter()
                .take(nargs)
                .map(|&a| Value::make_int(a))
                .collect(),
        ]
    };
    for call in calls {
        let interp = {
            let mut vm = Vm::from_context(&mut eval);
            vm.execute(&f, call.clone()).expect("interp").bits()
        };
        let aot = match aot_leaf.call(ctx_ptr, &call) {
            crate::emacs_core::jit::compile::NativeRun::Ok(b) => b,
            other => panic!("AOT not Ok: {other:?}"),
        };
        let jit = match jit_leaf.call(ctx_ptr, &call) {
            crate::emacs_core::jit::compile::NativeRun::Ok(b) => b,
            other => panic!("JIT not Ok: {other:?}"),
        };
        assert_eq!(aot, interp, "AOT != interp for {call:?}");
        assert_eq!(aot, jit, "AOT != JIT for {call:?}");
    }
}

/// R1c-5 gate: the FULL PRODUCTION path for a pure leaf —
/// `compile_leaf_to_object` → `link_object_to_so` → dlopen →
/// `load_leaf_from_unit` → `CompiledLeaf::call` — is byte-identical to BOTH
/// the interpreter and the JIT, incl across several args.
#[cfg(target_os = "linux")]
/// The AOT MIR population is exactly what it was before the MIR tier's
/// baseline-emitter adapter: a body that would lower any op through the
/// adapter (here `length`) is not emitted as an AOT MIR leaf — the adapter
/// has no AOT parity coverage, and a MIR loop has no back-edge poll.
#[test]
fn aot_mir_emit_refuses_bodies_that_would_use_the_adapter() {
    let pure = [Op::StackRef(0), Op::Add1, Op::Return];
    let m = mir::build_mir(&pure, &[], None, 1).expect("builds");
    assert!(!uses_mir_adapter(&m));
    assert!(
        prepare_leaf_emit(&pure, &[], 1)
            .expect("prepares")
            .is_some(),
        "a shim-free body is still an AOT MIR leaf"
    );
    let with_length = [Op::StackRef(0), Op::Length, Op::Return];
    let m = mir::build_mir(&with_length, &[], None, 1).expect("builds");
    assert!(uses_mir_adapter(&m));
    assert!(
        prepare_leaf_emit(&with_length, &[], 1)
            .expect("prepares")
            .is_none(),
        "a body with an adapter op stays out of the AOT MIR tier"
    );
    let with_call = [Op::Constant(0), Op::Call(0), Op::Return];
    let m =
        mir::build_mir(&with_call, &[Value::symbol("jit-aot-callee")], None, 0).expect("builds");
    assert!(
        !uses_mir_adapter(&m),
        "Call/Apply keep their pre-adapter AOT admission"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn aot_pure_leaf_matches_jit_and_interp() {
    // 1-arg pure body: (* (+ arg 5) 2) — fixnum arith, no consts/calls.
    let ops = [
        Op::Constant(0),
        Op::Add,
        Op::Constant(1),
        Op::Mul,
        Op::Return,
    ];
    let constants = [Value::make_int(5), Value::make_int(2)];
    assert_aot_matches_interp_and_jit(&ops, &constants, 1, &[0, 1, 7, -3, 1000, -1000]);
}

/// R1c-9 harness: a CORPUS of pure bodies — each emitted → linked → loaded →
/// compared interp == JIT == AOT bit-for-bit. Covers arithmetic, comparison
/// (branchy), unary, and a 0-arg constant-folding body, exercising the AOT
/// path across the pure subset (the in-test analogue of the suite-wide
/// `NEOVM_AOT=force` byte-identity gate, which needs R2's pre-built `.so`s).
#[cfg(target_os = "linux")]
#[test]
fn aot_roundtrip_matches_interp_and_jit_corpus() {
    let probe = [0i64, 1, 2, 7, -3, 42, 1000, -1000];

    // 0-arg: (+ 2 3) — constant fold, no args.
    assert_aot_matches_interp_and_jit(
        &[Op::Constant(0), Op::Constant(1), Op::Add, Op::Return],
        &[Value::make_int(2), Value::make_int(3)],
        0,
        &[],
    );
    // 1-arg: (- arg 1)
    assert_aot_matches_interp_and_jit(
        &[Op::Constant(0), Op::Sub, Op::Return],
        &[Value::make_int(1)],
        1,
        &probe,
    );
    // 1-arg: (1+ arg)
    assert_aot_matches_interp_and_jit(&[Op::Add1, Op::Return], &[], 1, &probe);
    // 1-arg: (* arg arg) — needs the arg twice (StackRef duplicates it).
    assert_aot_matches_interp_and_jit(&[Op::StackRef(0), Op::Mul, Op::Return], &[], 1, &probe);
    // 1-arg branchy: (if (< arg 0) ...) via Lss + GotoIfNil — comparison +
    // control flow, the deopt-free pure path.
    assert_aot_matches_interp_and_jit(
        &[Op::Constant(0), Op::Lss, Op::Return],
        &[Value::make_int(0)],
        1,
        &probe,
    );
}

/// AUDIT #16 gate (the CRITICAL one): a SYMBOL constant must be reloc'd by
/// NAME, never baked as a session-specific SymId. A baked SymId is only valid
/// in the emitting session; a cross-session load (the R2 dump-then-run case)
/// would return the wrong symbol or an out-of-range SymId. This test proves
/// the symbol const is in the reloc set (NOT baked) and that the rebuilt
/// symbol is the right one by NAME — even after the intern table has grown
/// (decoy interns) so an emit-time SymId would no longer be valid.
#[cfg(target_os = "linux")]
#[test]
fn aot_symbol_const_relocs_by_name_not_baked_sym_id() {
    let mut eval = crate::emacs_core::eval::Context::new_minimal_vm_harness();

    // 1-arg body: (lambda (x) (if (consp x) 'yes 'no)) — two symbol consts.
    // Bytecode: StackRef(0); Consp; GotoIfNil(->op5); Constant(0); Return;
    //           Constant(1); Return.
    let ops = [
        Op::StackRef(0),
        Op::Consp,
        Op::GotoIfNil(5),
        Op::Constant(0),
        Op::Return,
        Op::Constant(1),
        Op::Return,
    ];
    let sym_yes = Value::symbol(crate::emacs_core::intern::intern("aot-yes"));
    let sym_no = Value::symbol(crate::emacs_core::intern::intern("aot-no"));
    let constants = [sym_yes, sym_no];
    let arity = 1usize;

    let Some((obj, content_hash)) =
        compile_leaf_to_object(&ops, &constants, arity, None).expect("compile ok")
    else {
        panic!("symbol-bearing shim-free leaf must be AOT-runnable");
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir
        .path()
        .join(format!("{content_hash:032x}_{ABI_TAG:08x}.so"));
    link_object_to_so(&obj, &so_path).expect("link");

    // Grow the intern table BEFORE loading, so an emit-time-baked SymId for
    // 'aot-yes/'aot-no would now be stale relative to a fresh rebuild. (In one
    // process the ids don't actually move, but this models the cross-session
    // drift; the real proof is that the symbols are in the reloc set below.)
    for i in 0..50 {
        let _ = crate::emacs_core::intern::intern(&format!("aot-decoy-{i}"));
    }

    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    let unit = std::sync::Arc::new(super::super::compile::LoadedUnit::new(lib));
    let aot_leaf = load_leaf_from_unit(
        &unit,
        content_hash,
        arity,
        &live_reloc_for(&ops, &constants, arity),
        None,
    )
    .expect("load");

    // PROOF OF #16 FIX: both symbols are in the reloc set (the func's own
    // canonical symbol objects), NOT baked. A baked symbol would NOT appear
    // in reloc_values().
    let relocs = aot_leaf.reloc_values();
    assert_eq!(relocs.len(), 2, "two symbol reloc consts (not baked)");
    let names: std::collections::HashSet<&str> = relocs
        .iter()
        .map(|v| {
            crate::emacs_core::intern::resolve_sym(
                v.as_symbol_id().expect("reloc const is a symbol"),
            )
        })
        .collect();
    assert!(
        names.contains("aot-yes") && names.contains("aot-no"),
        "names: {names:?}"
    );

    // And the leaf returns the RIGHT symbol per branch, by IDENTITY (eq):
    // the rebuilt symbol must be the live obarray's 'aot-yes/'aot-no.
    let ctx_ptr = &mut eval as *mut crate::emacs_core::eval::Context as *mut u8;
    let call = |arg: Value| match aot_leaf.call(ctx_ptr, &[arg]) {
        crate::emacs_core::jit::compile::NativeRun::Ok(b) => Value::from_bits(b),
        other => panic!("not Ok: {other:?}"),
    };
    // (consp '(1)) → 'yes ; (consp 5) → 'no.
    let cons_arg = Value::cons(Value::make_int(1), Value::NIL);
    assert_eq!(
        call(cons_arg).as_symbol_id(),
        Some(crate::emacs_core::intern::intern("aot-yes")),
        "consp arg → 'aot-yes (by current-session SymId)"
    );
    assert_eq!(
        call(Value::make_int(5)).as_symbol_id(),
        Some(crate::emacs_core::intern::intern("aot-no")),
        "non-consp arg → 'aot-no"
    );
}

/// AUDIT #16 gensym hole (team-lead must-add): reloc-by-NAME is sound ONLY
/// for the CANONICAL interned symbol of a name. An UNINTERNED / gensym const
/// (make-symbol; cl-macro/pcase expansions embed these in quoted forms) has a
/// non-unique name, so rebuilding it by name in a different session would
/// yield the WRONG symbol. The emitter must REFUSE such a leaf (→ JIT, which
/// bakes the in-session SymId — correct same-session). This test: a leaf with
/// a gensym const is NOT AOT-emitted (compile_leaf_to_object → None).
#[test]
fn aot_gensym_symbol_const_is_rejected_stays_jit() {
    let _eval = crate::emacs_core::eval::Context::new_minimal_vm_harness();

    // An uninterned (gensym) symbol — name not registered as canonical.
    let gensym = Value::symbol(crate::emacs_core::intern::intern_uninterned("g$decoy"));
    assert!(
        !crate::emacs_core::intern::is_canonical_id(gensym.as_symbol_id().unwrap()),
        "precondition: the gensym is non-canonical"
    );
    // write_value_recipe must REFUSE the gensym (the load-bearing guard).
    let mut buf = Vec::new();
    assert!(
        write_value_recipe(&mut buf, gensym).is_err(),
        "gensym recipe must be refused"
    );

    // And the whole emit pipeline must bail to None (JIT) for a leaf that
    // returns the gensym const.
    let ops = [Op::Constant(0), Op::Return];
    let constants = [gensym];
    assert!(
        compile_leaf_to_object(&ops, &constants, 1, None)
            .expect("compile ok")
            .is_none(),
        "a gensym-const leaf must NOT be AOT-emitted (stays JIT)"
    );

    // Sanity: the CANONICAL symbol of the same shape IS accepted (so the
    // rejection is specific to uninterned, not symbols in general).
    let interned = Value::symbol(crate::emacs_core::intern::intern("g-interned-ok"));
    let mut buf2 = Vec::new();
    assert!(
        write_value_recipe(&mut buf2, interned).is_ok(),
        "a canonical interned symbol is still accepted"
    );
}

/// R1c-sidecar gate: a RELOC-bearing leaf (returns a heap-string constant)
/// loads its reloc base from the per-thread sidecar and rebuilds the const at
/// load. The AOT leaf returns a FRESH string (different pointer from the JIT
/// leaf's original const), so the result is compared by CONTENT, not bits.
/// This exercises the genuinely new sidecar path: `reloc_base` ← sidecar load,
/// recipe rebuild, and the rebuilt const being GC-rooted (it lives in the
/// leaf's reloc_data, walked by collect_jit_reloc_gc_roots).
#[cfg(target_os = "linux")]
#[test]
fn aot_reloc_bearing_leaf_rebuilds_string_const() {
    // Heap allocation needs a live heap — set up the harness FIRST so the
    // string const + JIT leaf are built against it.
    let mut eval = crate::emacs_core::eval::Context::new_minimal_vm_harness();

    // 1-arg body: (lambda (x) "hello") — returns a heap-string CONSTANT.
    let ops = [Op::Constant(0), Op::Return];
    let constants = [Value::string("hello")];
    let arity = 1usize;

    // The leaf is reloc-bearing (heap const) but shim-free → AOT-runnable.
    let (obj, content_hash) = compile_leaf_to_object(&ops, &constants, arity, None)
        .expect("compile ok")
        .expect("reloc-bearing shim-free leaf → Some");
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir
        .path()
        .join(format!("{content_hash:032x}_{ABI_TAG:08x}.so"));
    link_object_to_so(&obj, &so_path).expect("link .so");
    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    let unit = std::sync::Arc::new(super::super::compile::LoadedUnit::new(lib));
    let aot_leaf = load_leaf_from_unit(
        &unit,
        content_hash,
        arity,
        &live_reloc_for(&ops, &constants, arity),
        None,
    )
    .expect("load reloc leaf");

    // The reloc Vec holds the FUNCTION'S OWN "hello" object (audit #A:
    // eq-identical to the source constant, not a fresh copy).
    let relocs = aot_leaf.reloc_values();
    assert_eq!(relocs.len(), 1, "one reloc const");
    assert_eq!(relocs[0].as_lisp_string().unwrap().as_bytes(), b"hello");
    assert_eq!(
        relocs[0].bits(),
        constants[0].bits(),
        "reloc const IS the function's own object (eq-identical), not a copy"
    );

    // Calling the AOT leaf returns that exact "hello" object.
    let ctx_ptr = &mut eval as *mut crate::emacs_core::eval::Context as *mut u8;
    let bits = match aot_leaf.call(ctx_ptr, &[Value::make_int(0)]) {
        crate::emacs_core::jit::compile::NativeRun::Ok(b) => b,
        other => panic!("AOT reloc leaf not Ok: {other:?}"),
    };
    let result = Value::from_bits(bits);
    // Audit #A: the AOT result is EQ-IDENTICAL to the source constant (same
    // object) — exactly what interp/JIT return, so `eq` against the literal
    // would agree. (Before #A this was a fresh recipe-rebuilt copy → eq nil.)
    assert_eq!(
        result.bits(),
        constants[0].bits(),
        "AOT result is the function's own constant object (eq-identical)"
    );
}

/// R1c-sidecar: a TWO-const reloc leaf with control flow — proves the recipe
/// rebuild ORDER matches the lowering's reloc-index order (slot 0 ↔ "first",
/// slot 1 ↔ "second"). A wrong order would swap the branches' results.
#[cfg(target_os = "linux")]
#[test]
fn aot_two_reloc_consts_rebuild_in_index_order() {
    let mut eval = crate::emacs_core::eval::Context::new_minimal_vm_harness();

    // (lambda (x) (if x "first" "second")) — both arms return a heap string.
    // Bytecode: StackRef(0); GotoIfNil(->op3); Constant(0); Return;
    //           Constant(1); Return.  (op indices: 0..5)
    let ops = [
        Op::StackRef(0),
        Op::GotoIfNil(4),
        Op::Constant(0),
        Op::Return,
        Op::Constant(1),
        Op::Return,
    ];
    let constants = [Value::string("first"), Value::string("second")];
    let arity = 1usize;

    let Some((obj, content_hash)) =
        compile_leaf_to_object(&ops, &constants, arity, None).expect("compile ok")
    else {
        // If this body isn't MIR-lowerable/AOT-runnable, skip (don't fail) —
        // the single-const test already covers the reloc mechanism.
        return;
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir
        .path()
        .join(format!("{content_hash:032x}_{ABI_TAG:08x}.so"));
    link_object_to_so(&obj, &so_path).expect("link");
    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    let unit = std::sync::Arc::new(super::super::compile::LoadedUnit::new(lib));
    let aot_leaf = load_leaf_from_unit(
        &unit,
        content_hash,
        arity,
        &live_reloc_for(&ops, &constants, arity),
        None,
    )
    .expect("load");

    // reloc_values must be ["first","second"] in that order.
    let relocs = aot_leaf.reloc_values();
    assert_eq!(relocs.len(), 2, "two reloc consts");
    assert_eq!(relocs[0].as_lisp_string().unwrap().as_bytes(), b"first");
    assert_eq!(relocs[1].as_lisp_string().unwrap().as_bytes(), b"second");

    // And each branch returns the right rebuilt string.
    let ctx_ptr = &mut eval as *mut crate::emacs_core::eval::Context as *mut u8;
    let call = |arg: Value| match aot_leaf.call(ctx_ptr, &[arg]) {
        crate::emacs_core::jit::compile::NativeRun::Ok(b) => Value::from_bits(b),
        other => panic!("not Ok: {other:?}"),
    };
    assert_eq!(
        call(Value::T).as_lisp_string().unwrap().as_bytes(),
        b"first",
        "x=t → first arm"
    );
    assert_eq!(
        call(Value::NIL).as_lisp_string().unwrap().as_bytes(),
        b"second",
        "x=nil → second arm"
    );
}

/// R1c-8 gate: a reloc-bearing AOT leaf served THROUGH THE CACHE has its
/// rebuilt reloc const collected as a GC root (so it survives collection — it
/// is the leaf's only reference to that fresh string). Proves the AOT leaf is
/// covered by the existing R1a COMPILED-walking root scan with NO new root set.
#[cfg(target_os = "linux")]
#[test]
fn aot_reloc_const_is_gc_rooted_via_compiled_walk() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::value::LambdaParams;

    let mut _eval = crate::emacs_core::eval::Context::new_minimal_vm_harness();

    // Prime the cache's heap-identity guard with THIS heap BEFORE caching, so
    // the later root walk's `sync_cache_to_current_heap` does not see a
    // None→Some transition and clear the (just-cached) leaf. In production the
    // guard is primed by the first GC long before any compile; the test must
    // do it explicitly because it caches before any GC.
    {
        let mut prime: Vec<Value> = Vec::new();
        super::super::cache::collect_jit_reloc_gc_roots(&mut prime);
    }

    // (lambda (x) "needle") — reloc-bearing, shim-free.
    let ops = vec![Op::Constant(0), Op::Return];
    let constants = vec![Value::string("needle-aot-root")];
    let arity = 1usize;

    let (obj, content_hash) = compile_leaf_to_object(&ops, &constants, arity, None)
        .expect("compile ok")
        .expect("reloc shim-free leaf");
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir.path().join("leaf.so");
    link_object_to_so(&obj, &so_path).expect("link");
    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    let unit = std::sync::Arc::new(super::super::compile::LoadedUnit::new(lib));

    // Serve it through the cache (so it lands in COMPILED, where the root walk
    // looks).
    test_support::set_forced_enabled(true);
    test_support::inject_unit(content_hash, unit);

    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops.clone();
    f.constants = constants.clone().into();
    f.max_stack = 16;

    // Drive try_run_compiled so the AOT leaf is cached.
    let id = f.jit_runtime().compiled_id_or_assign();
    super::super::stats::reset_compile_stats();
    let _ = super::super::cache::try_run_compiled(
        std::ptr::null_mut(),
        &f,
        Value::NIL,
        &[Value::make_int(0)],
    )
    .unwrap();
    // Sanity: it must have been served FROM AOT (else the test proves nothing).
    assert_eq!(
        super::super::cache::cached_leaf_is_aot_for_test(id),
        Some(true),
        "leaf must be AOT-backed for this rooting test to be meaningful"
    );
    // The compile-stall metering must see it as an AOT serve, NOT a compile.
    let stats = super::super::stats::compile_stats_snapshot();
    assert_eq!(stats.aot_loads, 1);
    assert_eq!(stats.total_compiles, 0);

    // The root walk must include the leaf's rebuilt const (by content).
    let mut roots: Vec<Value> = Vec::new();
    super::super::cache::collect_jit_reloc_gc_roots(&mut roots);
    let found = roots.iter().any(|v| {
        v.as_lisp_string()
            .is_some_and(|s| s.as_bytes() == b"needle-aot-root")
    });
    assert!(
        found,
        "the AOT leaf's rebuilt reloc const must be a GC root (COMPILED walk)"
    );

    super::super::cache::clear();
    test_support::reset();
}

/// R1c-6 gate: a content/ABI MISMATCH (wrong arity, foreign hash) makes
/// `load_leaf_from_unit` return None (→ caller falls back to JIT, additive).
#[cfg(target_os = "linux")]
#[test]
fn aot_load_miss_falls_through() {
    let ops = [Op::Constant(0), Op::Add, Op::Return];
    let constants = [Value::make_int(5)];
    let arity = 1usize;
    let (obj, content_hash) = compile_leaf_to_object(&ops, &constants, arity, None)
        .expect("compile ok")
        .expect("pure subset");
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir.path().join("leaf.so");
    link_object_to_so(&obj, &so_path).expect("link");
    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    let unit = std::sync::Arc::new(super::super::compile::LoadedUnit::new(lib));

    let live = live_reloc_for(&ops, &constants, arity); // empty (fixnum body)
    // Right hash, WRONG arity → the descriptor arity check bails.
    assert!(
        load_leaf_from_unit(&unit, content_hash, /*arity=*/ 2, &live, None).is_none(),
        "arity mismatch must miss"
    );
    // A foreign content hash → the entry/descriptor symbols don't exist →
    // dlsym miss → None.
    assert!(
        load_leaf_from_unit(&unit, content_hash ^ 0xdead_beef, arity, &live, None).is_none(),
        "foreign hash must miss"
    );
}

/// R1c-6 gate: with AOT enabled and the unit pre-loaded, `try_run_compiled`
/// serves the leaf FROM AOT (the cached entry is AOT-backed, NOT JIT) and the
/// result matches the interpreter — the pre-warmed cache hit.
#[cfg(target_os = "linux")]
#[test]
fn aot_hit_serves_without_jitting_through_cache() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::value::LambdaParams;

    // 1-arg pure body (+ arg 5) — the AOT pure subset.
    let ops = vec![Op::Constant(0), Op::Add, Op::Return];
    let constants = vec![Value::make_int(5)];
    let arity = 1usize;

    // Build + link the `.so`, dlopen it, inject the unit by content hash.
    let (obj, content_hash) = compile_leaf_to_object(&ops, &constants, arity, None)
        .expect("compile ok")
        .expect("pure subset");
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir.path().join("leaf.so");
    link_object_to_so(&obj, &so_path).expect("link");
    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    let unit = std::sync::Arc::new(super::super::compile::LoadedUnit::new(lib));

    // Drive the cache path with the test seams (force-enable + inject unit).
    // Reset at the end so the override doesn't bleed into other tests.
    test_support::set_forced_enabled(true);
    test_support::inject_unit(content_hash, unit);

    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops.clone();
    f.constants = constants.clone().into();
    f.max_stack = 16;

    let id = f.jit_runtime().compiled_id_or_assign();
    let got = super::super::cache::try_run_compiled(
        std::ptr::null_mut(),
        &f,
        Value::NIL,
        &[Value::make_int(37)],
    )
    .unwrap();
    // (+ 37 5) = 42.
    assert_eq!(got, Some(Value::make_int(42).bits()), "AOT result");
    // The cached leaf must be AOT-backed — served from the `.so`, not JIT'd.
    assert_eq!(
        super::super::cache::cached_leaf_is_aot_for_test(id),
        Some(true),
        "cached leaf must be AOT-backed (served without JITing)"
    );

    super::super::cache::clear();
    test_support::reset();
}

/// R2-B2 gate: `enumerate_loadup_leaves` walks a (tiny) obarray and returns
/// the bytecode-bound, required-only, D0-AOT-candidate fns — and EXCLUDES a
/// non-candidate (here: an `&optional` fn, rejected by the required-only
/// filter). Models the dump-time enumeration over the loaded loadup closure.
#[test]
fn r2_enumerate_loadup_leaves_finds_d0_candidates() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::intern::SymId;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();

    // Candidate A: (lambda (a) (+ a 5)) — pure arith, required-only → D0 AOT.
    let mut a = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    a.lexical = true;
    a.ops = vec![Op::Constant(0), Op::Add, Op::Return];
    a.constants = vec![Value::make_int(5)].into();
    a.max_stack = 16;
    let a_id = crate::emacs_core::intern::intern("r2-cand-add5");
    ev.obarray
        .set_symbol_function_id(a_id, Value::make_bytecode(a));

    // Non-candidate B: an &OPTIONAL fn — rejected by the required-only filter
    // (matches the MIR pure-tier arity shape).
    let mut b = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: vec![SymId(2)],
        rest: None,
    });
    b.lexical = true;
    b.ops = vec![Op::StackRef(0), Op::Return];
    b.max_stack = 16;
    let b_id = crate::emacs_core::intern::intern("r2-noncand-optional");
    ev.obarray
        .set_symbol_function_id(b_id, Value::make_bytecode(b));

    let leaves = enumerate_loadup_leaves(&ev, /*d0_filter=*/ true);
    let names: std::collections::HashSet<&str> = leaves.iter().map(|l| l.name.as_str()).collect();
    assert!(
        names.contains("r2-cand-add5"),
        "the pure arith required-only defun is a D0 candidate; got {names:?}"
    );
    assert!(
        !names.contains("r2-noncand-optional"),
        "the &optional defun must NOT be a candidate (required-only filter)"
    );
    // The candidate's recorded arity/ops match the source.
    let cand = leaves.iter().find(|l| l.name == "r2-cand-add5").unwrap();
    assert_eq!(cand.arity, 1);
    assert_eq!(cand.ops, &[Op::Constant(0), Op::Add, Op::Return]);
}

/// R2-B4 gate: a multi-leaf preload object DEDUPS by content-hash — two
/// loadup fns with IDENTICAL bodies collapse to ONE emitted entry (not a
/// duplicate-symbol collision), a distinct body emits its own, and a
/// non-AOT-subset body is skipped. The bytes parse + carry exactly the unique
/// entries. (Team-lead: dedup, log the collapse, no silent drops.)
#[test]
fn r2_build_preload_object_dedups_identical_bodies() {
    use object::{Object, ObjectSymbol};

    let _ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();

    // Two IDENTICAL bodies (1+ arg) under different names → same content hash.
    let add5 = (
        vec![Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(5)],
        1usize,
    );
    // A DISTINCT body (- arg 1).
    let sub1 = (
        vec![Op::Constant(0), Op::Sub, Op::Return],
        vec![Value::make_int(1)],
        1usize,
    );
    let leaf = |name: &str, t: &(Vec<Op>, Vec<Value>, usize)| LoadupLeaf {
        name: name.to_string(),
        // Leak to get 'static refs for the test (LoadupLeaf borrows 'static
        // from the obarray heap in production; fine to leak in a unit test).
        ops: Box::leak(t.0.clone().into_boxed_slice()),
        constants: Box::leak(t.1.clone().into_boxed_slice()),
        arity: t.2,
    };
    let leaves = vec![
        leaf("dup-a", &add5),
        leaf("dup-b", &add5), // identical body → dedup
        leaf("distinct", &sub1),
    ];

    let (obj, stats) = build_preload_object(&leaves, None).expect("build preload object");
    assert_eq!(stats.candidates, 3);
    assert_eq!(stats.prepared, 3, "all three are AOT-runnable");
    assert_eq!(stats.unique_emitted, 2, "two DISTINCT bodies emitted");
    assert_eq!(stats.deduped, 1, "the identical-body pair collapsed once");
    assert_eq!(stats.skipped_unsupported, 0);

    // Parse: exactly 2 exported entry symbols (`__neovm_aot_{hash}_{tag}`),
    // NOT the descriptors (`__neovm_aotd_...`) — no duplicate-symbol collision.
    let file = object::File::parse(&*obj).expect("parse preload object");
    let entries = file
        .symbols()
        .filter(|s| {
            s.is_definition()
                && s.name()
                    .map(|n| n.starts_with("__neovm_aot_") && !n.starts_with("__neovm_aotd_"))
                    .unwrap_or(false)
        })
        .count();
    assert_eq!(
        entries, 2,
        "two unique entry symbols, no duplicate-symbol collision"
    );
}

/// R2-C3 GATE (native-from-call-1 AND survive-a-GC). Prepopulate the COMPILED
/// cache from a preload `.so` and assert the loadup candidate is AOT-backed at
/// HEAT=0 (no warmup), then FORCE A GC and assert the leaf SURVIVES. The
/// survive-a-GC half is the regression guard for the R1a heap-identity bug:
/// `prepopulate_aot_leaves` must sync `COMPILED_HEAP` to current BEFORE
/// inserting, else the first GC's `sync_cache_to_current_heap` (COMPILED_HEAP
/// still None) would CLEAR every prepopulated leaf — native for call 1, then
/// silently gone. We deliberately do NOT pre-prime the heap guard here.
#[test]
fn r2_prepopulate_native_from_call_1_and_survives_gc() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::intern::SymId;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();

    // One pure-arith required-only candidate (lambda (a) (+ a 5)) → D0 AOT.
    let ops = vec![Op::Constant(0), Op::Add, Op::Return];
    let constants = vec![Value::make_int(5)];
    let _arity = 1usize;
    let mut a = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    a.lexical = true;
    a.ops = ops.clone();
    a.constants = constants.clone().into();
    a.max_stack = 16;
    let sym = crate::emacs_core::intern::intern("r2-prepop-add5");
    ev.obarray
        .set_symbol_function_id(sym, Value::make_bytecode(a));

    // Build the ONE preload `.so` (the producer's multi-leaf object), dlopen,
    // and inject it as THE preload (so load_preload returns it).
    let leaves = enumerate_loadup_leaves(&ev, /*d0_filter=*/ true);
    let (obj, _stats) = build_preload_object(&leaves, None).expect("build preload object");
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir.path().join(PRELOAD_SO_NAME);
    link_object_to_so(&obj, &so_path).expect("link");
    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    let unit = std::sync::Arc::new(super::super::compile::LoadedUnit::new(lib));

    test_support::set_forced_enabled(true);
    test_support::inject_preload(unit);

    // The candidate's compiled_id (so we can probe the cache without a call).
    let id = ev
        .obarray
        .symbol_function_id(sym)
        .and_then(|v| v.get_bytecode_data())
        .map(|bc| bc.jit_runtime().compiled_id_or_assign())
        .expect("candidate fn id");

    // PREPOPULATE. NOTE: no pre-priming of the heap guard — prepopulate must
    // establish COMPILED_HEAP itself for the survive-a-GC half to pass.
    let stats = prepopulate_aot_from_preload(&ev);
    assert!(
        stats.candidates >= 1,
        "at least one candidate; got {stats:?}"
    );
    assert_eq!(
        stats.loaded, stats.candidates,
        "all candidates loaded from .so"
    );
    assert_eq!(
        stats.inserted, stats.loaded,
        "empty cache → every loaded leaf fills a cold slot; got {stats:?}"
    );
    assert_eq!(stats.missed, 0, "no preload misses; got {stats:?}");

    // NATIVE FROM CALL 1: compiled at heat=0 (no warmup) AND AOT-backed.
    assert!(
        super::super::cache::is_compiled_for_test(id),
        "loadup leaf must be compiled at heat=0 (native from call 1)"
    );
    assert_eq!(
        super::super::cache::cached_leaf_is_aot_for_test(id),
        Some(true),
        "the prepopulated leaf must be AOT-backed (from the preload .so)"
    );

    // FORCE A GC — exercises collect_jit_reloc_gc_roots → sync_cache_to_current_heap.
    ev.gc_collect_exact();

    // SURVIVE-A-GC: the leaf must still be cached + AOT-backed (the sync did
    // NOT clear it, because prepopulate established COMPILED_HEAP first).
    assert!(
        super::super::cache::is_compiled_for_test(id),
        "prepopulated leaf must SURVIVE a GC (heap-identity sync must not clear it)"
    );
    assert_eq!(
        super::super::cache::cached_leaf_is_aot_for_test(id),
        Some(true),
        "prepopulated leaf must still be AOT-backed after a GC"
    );

    super::super::cache::clear();
    test_support::reset();
}

/// R2-C2 GATE (stale interlock): a `load_preload` MISS (e.g. a manifest
/// fingerprint mismatch) makes `prepopulate_aot_from_preload` a clean no-op —
/// nothing is inserted, the function will JIT normally. The happy-path seam
/// can't exercise the interlock, and it is load-bearing for v1 safety (a
/// stale/foreign preload must never mis-serve), so test the miss explicitly.
#[test]
fn r2_prepopulate_skips_on_preload_miss() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::intern::SymId;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let mut a = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    a.lexical = true;
    a.ops = vec![Op::Constant(0), Op::Add, Op::Return];
    a.constants = vec![Value::make_int(5)].into();
    a.max_stack = 16;
    let sym = crate::emacs_core::intern::intern("r2-prepop-miss-add5");
    ev.obarray
        .set_symbol_function_id(sym, Value::make_bytecode(a));
    let id = ev
        .obarray
        .symbol_function_id(sym)
        .and_then(|v| v.get_bytecode_data())
        .map(|bc| bc.jit_runtime().compiled_id_or_assign())
        .expect("candidate fn id");

    // AOT enabled, but the preload resolves to a MISS (the stale-interlock /
    // no-preload path) → prepopulate must do nothing.
    test_support::set_forced_enabled(true);
    test_support::inject_preload_miss();

    let stats = prepopulate_aot_from_preload(&ev);
    assert_eq!(
        stats.candidates, 0,
        "no candidates collected on a preload miss"
    );
    assert_eq!(stats.inserted, 0, "nothing inserted on a preload miss");
    assert!(
        !super::super::cache::is_compiled_for_test(id),
        "a preload miss must leave the fn uncompiled (it will JIT normally)"
    );

    super::super::cache::clear();
    test_support::reset();
}

/// R2-C audit (w0guiyma9) GATE: prepopulate must NEVER overwrite a pre-existing
/// COMPILED entry. The `after-pdump-load-hook` runs arbitrary elisp right
/// before prepopulate; if it dispatches a loadup fn the JIT compiles it into
/// the slot prepopulate would fill. That JIT leaf may be spec-slot-referenced
/// (Rc::as_ptr) and/or INLINE_DEPS-registered, so overwriting it with the AOT
/// leaf → use-after-free + a later-redefine `evict_inline_dependents`
/// spec-slot-safety panic. INSERT-IF-ABSENT keeps the warm slot → root guard.
///
/// This uses a PURE leaf (so its `.so` dlopens in the lib unit-test binary,
/// which does NOT export the `neovm_jit_*` shims a call-bearing `.so` would
/// need — that's why call-bearing AOT lives in `tests/aot_call_bearing.rs`).
/// The KEEP assertion is the necessary+sufficient guard: insert-if-absent
/// never overwrites ANY slot, so the inline-deps/UAF consequences (which only
/// fire on an overwrite) cannot arise. The eviction machinery itself is covered
/// by `compile::tests::precise_eviction_only_evicts_inlined_dependents`; and an
/// AOT leaf (inline_epoch=None) is never in any INLINE_DEPS set, so eviction
/// never targets one (see `prepopulate_aot_leaves`' REDEFINITION note).
#[test]
fn r2_prepopulate_never_overwrites_existing_jit_leaf() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::intern::SymId;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let mut a = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    a.lexical = true;
    a.ops = vec![Op::Constant(0), Op::Add, Op::Return];
    a.constants = vec![Value::make_int(5)].into();
    a.max_stack = 16;
    let sym = crate::emacs_core::intern::intern("r2-prepop-nooverwrite-add5");
    ev.obarray
        .set_symbol_function_id(sym, Value::make_bytecode(a));
    let id = ev
        .obarray
        .symbol_function_id(sym)
        .and_then(|v| v.get_bytecode_data())
        .map(|bc| bc.jit_runtime().compiled_id_or_assign())
        .expect("candidate fn id");

    // (1) Simulate the hook: with AOT OFF, JIT-compile the fn into COMPILED via
    // the real cache path (or_insert_with → compile_bytecode_function_with).
    test_support::set_forced_enabled(false);
    let f = ev
        .obarray
        .symbol_function_id(sym)
        .and_then(|v| v.get_bytecode_data())
        .expect("bc");
    let got = super::super::cache::try_run_compiled(
        std::ptr::null_mut(),
        f,
        Value::NIL,
        &[Value::make_int(37)],
    )
    .unwrap();
    assert_eq!(got, Some(Value::make_int(42).bits()), "JIT result (+ 37 5)");
    assert_eq!(
        super::super::cache::cached_leaf_is_aot_for_test(id),
        Some(false),
        "precondition: a JIT (non-AOT) leaf is cached for this fn"
    );

    // (2) Now enable AOT + inject a real preload that DOES contain this fn,
    // then prepopulate. The slot is already warm (JIT) → must be KEPT.
    let leaves = enumerate_loadup_leaves(&ev, /*d0_filter=*/ true);
    let (obj, _) = build_preload_object(&leaves, None).expect("build preload object");
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir.path().join(PRELOAD_SO_NAME);
    link_object_to_so(&obj, &so_path).expect("link");
    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    let unit = std::sync::Arc::new(super::super::compile::LoadedUnit::new(lib));
    test_support::set_forced_enabled(true);
    test_support::inject_preload(unit);

    let stats = prepopulate_aot_from_preload(&ev);
    // The fn WAS loadable from the preload (loaded>=1) but its slot was already
    // occupied by the JIT leaf, so inserted=0 for it (insert-if-absent).
    assert!(
        stats.loaded >= 1,
        "the fn is loadable from the preload; got {stats:?}"
    );
    assert_eq!(
        stats.inserted, 0,
        "the already-warm JIT slot must NOT be filled (insert-if-absent); got {stats:?}"
    );

    // (3) THE GUARD: the slot still holds the ORIGINAL JIT leaf, not the AOT one.
    assert_eq!(
        super::super::cache::cached_leaf_is_aot_for_test(id),
        Some(false),
        "prepopulate must NOT overwrite the pre-existing JIT leaf (UAF + inline-dep panic risk)"
    );

    // (4) Sanity: a GC + a redefinition must not panic (the JIT leaf is intact).
    ev.gc_collect_exact();
    ev.obarray.set_symbol_function_id(
        sym,
        Value::make_bytecode({
            let mut b = ByteCodeFunction::new(LambdaParams {
                required: vec![SymId(1)],
                optional: Vec::new(),
                rest: None,
            });
            b.lexical = true;
            b.ops = vec![Op::Constant(0), Op::Sub, Op::Return];
            b.constants = vec![Value::make_int(1)].into();
            b.max_stack = 16;
            b
        }),
    );

    super::super::cache::clear();
    test_support::reset();
}

/// Task #11 format gate: the v2 pre-key line renderer and the manifest
/// parser round-trip (incl. names that need escaping), and every malformed
/// shape FAILS CLOSED to `prekeys: None` (pre-filter disabled) rather than
/// yielding a partial map that could wrongly skip a member.
#[test]
fn manifest_v2_prekeys_round_trip_and_fail_closed_parsing() {
    let entries: [(&str, ManifestPreKey); 4] = [
        (
            "plain-name",
            ManifestPreKey {
                member: true,
                ops_len: 3,
                arity: 1,
                hash: 0xdead_beef,
            },
        ),
        (
            "with space", // whitespace → hex-escaped token
            ManifestPreKey {
                member: false,
                ops_len: 7,
                arity: 2,
                hash: 1,
            },
        ),
        (
            "%leading", // leading '%' → hex-escaped (escape marker collision)
            ManifestPreKey {
                member: false,
                ops_len: 1,
                arity: 0,
                hash: 2,
            },
        ),
        (
            "", // empty name → escapes to the bare "%" token
            ManifestPreKey {
                member: true,
                ops_len: 2,
                arity: 3,
                hash: u128::MAX,
            },
        ),
    ];
    let mut text = String::from("version 2\nabi_tag 00000000\nfingerprint f00\nleaves 4\n");
    for (name, key) in &entries {
        text.push_str(&manifest_leaf_line(
            key.member,
            key.ops_len,
            key.arity,
            key.hash,
            name,
        ));
    }
    let parsed = parse_preload_manifest(&text);
    assert_eq!(parsed.version, Some(2));
    assert_eq!(parsed.abi_tag, Some(0));
    assert_eq!(parsed.fingerprint.as_deref(), Some("f00"));
    let map = parsed.prekeys.expect("well-formed pre-key section parses");
    assert_eq!(map.len(), 4);
    for (name, key) in &entries {
        assert_eq!(map.get(*name), Some(key), "round-trip for {name:?}");
    }

    // v1-shaped manifest: header parses (interlock rejects it upstream by
    // version), and the `hash` diagnostic lines never form a pre-key map
    // (declared `leaves` ≠ zero parsed `leaf` lines).
    let v1 = "version 1\nabi_tag 00000000\nfingerprint f00\nleaves 2\n\
              hash 000000000000000000000000000000ff\n\
              hash 0000000000000000000000000000ff00\n";
    let parsed_v1 = parse_preload_manifest(v1);
    assert_eq!(parsed_v1.version, Some(1));
    assert!(parsed_v1.prekeys.is_none(), "v1 text yields no pre-keys");

    // FAIL-CLOSED shapes: each corrupt variant discards the WHOLE map.
    let hdr = "version 2\nabi_tag 00000000\nfingerprint f00\n";
    for (label, body) in [
        ("bad class", "leaves 1\nleaf z 3 1 00 name\n"),
        ("trailing junk", "leaves 1\nleaf m 3 1 00 name extra\n"),
        ("truncated fields", "leaves 1\nleaf m 3\n"),
        ("count mismatch", "leaves 2\nleaf m 3 1 00 name\n"),
        ("missing leaves line", "leaf m 3 1 00 name\n"),
        (
            "duplicate name",
            "leaves 2\nleaf m 3 1 00 dup\nleaf x 4 2 01 dup\n",
        ),
        ("odd escape hex", "leaves 1\nleaf m 3 1 00 %abc\n"),
        ("non-hex hash", "leaves 1\nleaf m 3 1 zz name\n"),
    ] {
        let parsed = parse_preload_manifest(&format!("{hdr}{body}"));
        assert!(
            parsed.prekeys.is_none(),
            "malformed pre-key section ({label}) must fail closed"
        );
    }
}

/// Task #11 producer gate: `build_and_link_preload` writes a v2 manifest
/// with one pre-key line per HASHABLE required-only loadup fn — `m` for the
/// emitted member, `x` for a hashable non-member (here: a `Throw` body, MIR
/// unmodelled-control + outside the baseline allowlist) — and none for an
/// `&optional` fn (not required-only). The written text parses back into
/// the exact pre-keys the runtime pre-filter consumes.
#[test]
fn build_and_link_preload_writes_v2_prekey_manifest() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::intern::SymId;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let mk = |required: Vec<SymId>, optional: Vec<SymId>, ops: Vec<Op>, consts: Vec<Value>| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required,
            optional,
            rest: None,
        });
        f.lexical = true;
        f.ops = ops;
        f.constants = consts.into();
        f.max_stack = 16;
        f.seal_hand_assembled_ops();
        f
    };
    let member_ops = vec![Op::Constant(0), Op::Add, Op::Return];
    let member_consts = vec![Value::make_int(5)];
    let throw_ops = vec![Op::Constant(0), Op::Constant(1), Op::Throw, Op::Return];
    // The trailing Return is unreachable Throw padding, but it keeps the
    // vector seal-shaped so the pipeline hashes exactly these ops.
    let throw_consts = vec![Value::symbol("prod-pf-tag"), Value::make_int(1)];
    for (name, f) in [
        (
            "prod-pf-member-add5",
            mk(
                vec![SymId(1)],
                vec![],
                member_ops.clone(),
                member_consts.clone(),
            ),
        ),
        (
            "prod-pf-nonmember-throw",
            mk(
                vec![SymId(1)],
                vec![],
                throw_ops.clone(),
                throw_consts.clone(),
            ),
        ),
        (
            "prod-pf-optional",
            mk(
                vec![],
                vec![SymId(1)],
                member_ops.clone(),
                member_consts.clone(),
            ),
        ),
    ] {
        let sym = crate::emacs_core::intern::intern(name);
        ev.obarray
            .set_symbol_function_id(sym, Value::make_bytecode(f));
    }

    let dir = tempfile::tempdir().expect("tempdir");
    build_and_link_preload(&ev, dir.path()).expect("build_and_link_preload");
    let text =
        std::fs::read_to_string(dir.path().join(PRELOAD_MANIFEST_NAME)).expect("manifest written");
    let parsed = parse_preload_manifest(&text);
    assert_eq!(parsed.version, Some(PRELOAD_MANIFEST_VERSION));
    assert_eq!(parsed.abi_tag, Some(ABI_TAG));
    assert_eq!(
        parsed.fingerprint.as_deref(),
        Some(crate::emacs_core::pdump::fingerprint_hex()),
        "manifest carries the running fingerprint (stale interlock)"
    );
    let map = parsed
        .prekeys
        .expect("v2 manifest carries a well-formed pre-key map");

    let member_hash = leaf_content_hash(&member_ops, &member_consts, 1).expect("hashable");
    assert_eq!(
        map.get("prod-pf-member-add5"),
        Some(&ManifestPreKey {
            member: true,
            ops_len: 3,
            arity: 1,
            hash: member_hash
        }),
        "the D0 candidate gets an `m` pre-key"
    );
    let throw_hash = leaf_content_hash(&throw_ops, &throw_consts, 1).expect("hashable");
    assert_eq!(
        map.get("prod-pf-nonmember-throw"),
        Some(&ManifestPreKey {
            member: false,
            ops_len: 4,
            arity: 1,
            hash: throw_hash
        }),
        "the hashable non-candidate gets an `x` pre-key (the skip class)"
    );
    assert!(
        !map.contains_key("prod-pf-optional"),
        "&optional fns are not required-only → no pre-key"
    );
}

/// Task #11 THE GATE: with a v2 pre-key map injected, prepopulate skips a
/// verified non-member WITHOUT calling `leaf_content_hash` (asserted via
/// the hash-call probe seam, as a control-vs-filtered DELTA so unrelated
/// harness fns can't skew it), while the member still loads from the `.so`
/// and every stat (candidates/loaded/missed) matches the pre-filter-less
/// pass — the "counts stay 706 / 2195" property in miniature.
#[test]
fn prepopulate_manifest_prefilter_skips_nonmember_without_hashing() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::intern::SymId;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let mk = |ops: Vec<Op>, consts: Vec<Value>| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops;
        f.constants = consts.into();
        f.max_stack = 16;
        f.seal_hand_assembled_ops();
        f
    };
    let member_ops = vec![Op::Constant(0), Op::Add, Op::Return];
    let member_consts = vec![Value::make_int(5)];
    let nonmember_ops = vec![Op::Constant(0), Op::Sub, Op::Return];
    let nonmember_consts = vec![Value::make_int(1)];
    let m_sym = crate::emacs_core::intern::intern("pf-member-add5");
    let x_sym = crate::emacs_core::intern::intern("pf-nonmember-sub1");
    ev.obarray.set_symbol_function_id(
        m_sym,
        Value::make_bytecode(mk(member_ops.clone(), member_consts.clone())),
    );
    ev.obarray.set_symbol_function_id(
        x_sym,
        Value::make_bytecode(mk(nonmember_ops.clone(), nonmember_consts.clone())),
    );
    let id_of = |ev: &crate::emacs_core::eval::Context, sym| {
        ev.obarray
            .symbol_function_id(sym)
            .and_then(|v| v.get_bytecode_data())
            .map(|bc| bc.jit_runtime().compiled_id_or_assign())
            .expect("fn id")
    };
    let m_id = id_of(&ev, m_sym);
    let x_id = id_of(&ev, x_sym);

    // Preload `.so` containing ONLY the member leaf (the nonmember is a
    // genuine dlsym miss).
    let member_leaf = LoadupLeaf {
        name: "pf-member-add5".to_string(),
        ops: Box::leak(member_ops.clone().into_boxed_slice()),
        constants: Box::leak(member_consts.clone().into_boxed_slice()),
        arity: 1,
    };
    let (obj, _) =
        build_preload_object(std::slice::from_ref(&member_leaf), None).expect("build preload");
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir.path().join(PRELOAD_SO_NAME);
    link_object_to_so(&obj, &so_path).expect("link");
    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    let unit = std::sync::Arc::new(super::super::compile::LoadedUnit::new(lib));
    test_support::set_forced_enabled(true);
    test_support::inject_preload(unit);

    let m_hash = leaf_content_hash(&member_ops, &member_consts, 1).expect("hashable");
    let x_hash = leaf_content_hash(&nonmember_ops, &nonmember_consts, 1).expect("hashable");

    // CONTROL: no pre-keys → the exact pre-filter-less path (both fns hash).
    test_support::reset_hash_calls();
    let control = prepopulate_aot_from_preload(&ev);
    let control_hashes = test_support::hash_calls();
    assert!(
        control.loaded >= 1,
        "member loads in control; got {control:?}"
    );
    assert!(
        control.missed >= 1,
        "nonmember misses in control; got {control:?}"
    );
    super::super::cache::clear();

    // FILTERED: inject the v2 pre-keys — `m` for the member, verified `x`
    // for the nonmember.
    let mut map = PreKeyMap::new();
    map.insert(
        "pf-member-add5".into(),
        ManifestPreKey {
            member: true,
            ops_len: 3,
            arity: 1,
            hash: m_hash,
        },
    );
    map.insert(
        "pf-nonmember-sub1".into(),
        ManifestPreKey {
            member: false,
            ops_len: 3,
            arity: 1,
            hash: x_hash,
        },
    );
    test_support::inject_prekeys(map);
    test_support::reset_hash_calls();
    let filtered = prepopulate_aot_from_preload(&ev);
    let filtered_hashes = test_support::hash_calls();

    // THE WIN: exactly the x-keyed nonmember skipped its hash; nothing else
    // changed. (Delta-based so unrelated harness fns can't skew it.)
    assert_eq!(
        filtered_hashes,
        control_hashes - 1,
        "the verified non-member must be skipped WITHOUT a leaf_content_hash call"
    );
    // Count parity with the pre-filter-less pass.
    assert_eq!(
        filtered.candidates, control.candidates,
        "candidates preserved"
    );
    assert_eq!(filtered.missed, control.missed, "missed preserved");
    assert_eq!(filtered.loaded, control.loaded, "loaded preserved");
    // Membership outcome: member native + AOT-backed, nonmember untouched.
    assert!(
        super::super::cache::is_compiled_for_test(m_id),
        "member serves native from the preload under the pre-filter"
    );
    assert_eq!(
        super::super::cache::cached_leaf_is_aot_for_test(m_id),
        Some(true)
    );
    assert!(
        !super::super::cache::is_compiled_for_test(x_id),
        "nonmember stays uncompiled (it will JIT normally)"
    );

    super::super::cache::clear();
    test_support::reset();
}

/// Task #11 FAIL-CLOSED gate: a STALE pre-key (here an `x` key whose
/// ops-count no longer matches the live body — e.g. the fn was redefined
/// between dump and prepopulate) must NOT be trusted: the pass falls
/// through to the exact hash+dlsym path, which still loads the member.
#[test]
fn prepopulate_prekey_mismatch_fails_closed_to_hash_path() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::intern::SymId;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ops = vec![Op::Constant(0), Op::Add, Op::Return];
    let consts = vec![Value::make_int(5)];
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops.clone();
    f.constants = consts.clone().into();
    f.max_stack = 16;
    let sym = crate::emacs_core::intern::intern("pf-fc-add5");
    ev.obarray
        .set_symbol_function_id(sym, Value::make_bytecode(f));
    let id = ev
        .obarray
        .symbol_function_id(sym)
        .and_then(|v| v.get_bytecode_data())
        .map(|bc| bc.jit_runtime().compiled_id_or_assign())
        .expect("fn id");

    let leaf = LoadupLeaf {
        name: "pf-fc-add5".to_string(),
        ops: Box::leak(ops.clone().into_boxed_slice()),
        constants: Box::leak(consts.clone().into_boxed_slice()),
        arity: 1,
    };
    let (obj, _) = build_preload_object(std::slice::from_ref(&leaf), None).expect("build preload");
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir.path().join(PRELOAD_SO_NAME);
    link_object_to_so(&obj, &so_path).expect("link");
    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    let unit = std::sync::Arc::new(super::super::compile::LoadedUnit::new(lib));
    test_support::set_forced_enabled(true);
    test_support::inject_preload(unit);

    // Stale `x` key: wrong ops_len (99 ≠ 3). Discriminator mismatch → the
    // pre-filter must fall through, NOT skip.
    let mut map = PreKeyMap::new();
    map.insert(
        "pf-fc-add5".into(),
        ManifestPreKey {
            member: false,
            ops_len: 99,
            arity: 1,
            hash: 0,
        },
    );
    test_support::inject_prekeys(map);
    test_support::reset_hash_calls();
    let stats = prepopulate_aot_from_preload(&ev);
    assert!(
        test_support::hash_calls() >= 1,
        "mismatched pre-key must fall through to the hash path"
    );
    assert!(stats.loaded >= 1, "member still loads; got {stats:?}");
    assert!(
        super::super::cache::is_compiled_for_test(id),
        "a stale pre-key must not suppress a member load (fail-closed)"
    );
    assert_eq!(
        super::super::cache::cached_leaf_is_aot_for_test(id),
        Some(true)
    );

    super::super::cache::clear();
    test_support::reset();
}

// === R2 increment C: AOT PGO persistence ===

/// Build a `(callee x)` pred body (Constant(callee), StackRef(1), Call(1), Return).
fn pgo_pred_body(callee: &str) -> crate::emacs_core::bytecode::ByteCodeFunction {
    use crate::emacs_core::intern::{SymId, intern};
    use crate::emacs_core::value::LambdaParams;
    let mut f = crate::emacs_core::bytecode::ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
    f.constants = vec![Value::symbol(intern(callee))].into();
    f.max_stack = 16;
    f
}

/// Alias a user symbol to a builtin's function cell (control the callee binding).
fn pgo_alias(c: &mut crate::emacs_core::eval::Context, alias: &str, builtin: &str) {
    use crate::emacs_core::intern::intern;
    let cell = c
        .obarray
        .symbol_function_id(intern(builtin))
        .expect("builtin bound");
    c.obarray.set_symbol_function(alias, cell);
}

fn count_so(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "so"))
        .count()
}

/// Hazard-1 equivalence: the drain's producer (`compile_leaf_to_object` with the
/// LIVE obarray) is BYTE-IDENTICAL across two independent sessions (fresh
/// obarrays, drifted SymId spaces, same callee binding) — so a runtime-emitted
/// `.so` and a dump-time `.so` of the same body are the same artifact (same
/// content hash, same object). Also proves `aot_pgo_enabled()` defaults OFF.
#[test]
fn pgo_runtime_emit_is_byte_identical_across_sessions_and_default_off() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::intern;

    // Default gate: OFF unless NEOVM_AOT_PGO is set (this process never sets it).
    assert!(!aot_pgo_enabled(), "NEOVM_AOT_PGO must default OFF");

    let mut c1 = Context::new();
    pgo_alias(&mut c1, "pgo-eq-callee", "recordp");
    for i in 0..40 {
        let _ = intern(&format!("pgo-eq-drift1-{i}"));
    }
    let mut c2 = Context::new();
    for i in 0..123 {
        let _ = intern(&format!("pgo-eq-drift2-{i}"));
    }
    pgo_alias(&mut c2, "pgo-eq-callee", "recordp");

    let f = pgo_pred_body("pgo-eq-callee");
    let (o1, h1) = compile_leaf_to_object(&f.ops, &f.constants, 1, Some(&c1.obarray))
        .expect("emit1 ok")
        .expect("emit1 some");
    let (o2, h2) = compile_leaf_to_object(&f.ops, &f.constants, 1, Some(&c2.obarray))
        .expect("emit2 ok")
        .expect("emit2 some");
    assert_eq!(h1, h2, "content hash must be session-independent");
    assert_eq!(
        o1, o2,
        "runtime-emit must be BYTE-IDENTICAL across sessions (dump-time == runtime)"
    );
}

/// The drain CAP bounds the number of `.so`s emitted per call, and the
/// `.exists()` skip makes a re-drain a no-op (no duplicate `cc` spawn), while
/// successive drains CONVERGE on the full hot set.
#[test]
fn pgo_drain_cap_bounds_count_and_exists_skips() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::intern;

    let dir = tempfile::tempdir().expect("tempdir");
    super::super::cache::clear();
    // A bare `(callee x)` is call-dominated → decline the JIT profit gate so it
    // enters the JIT set as the drain's spec-bearing source.
    super::super::compile::force_profit_gate_for_test(false);
    let mut c = Context::new();
    pgo_alias(&mut c, "pgo-cap-a", "recordp");
    pgo_alias(&mut c, "pgo-cap-b", "consp");
    // Bind + stage TWO distinct hot pred bodies (different callees → different hashes).
    for (fname, callee) in [("pgo-cap-fa", "pgo-cap-a"), ("pgo-cap-fb", "pgo-cap-b")] {
        c.obarray
            .set_symbol_function(fname, Value::make_bytecode(pgo_pred_body(callee)));
        let bc = c
            .obarray
            .symbol_function_id(intern(fname))
            .and_then(|v| v.get_bytecode_data())
            .expect("bound bytecode");
        super::super::cache::compile_and_cache_jit_leaf(bc, Some(&c.obarray))
            .expect("jit-compile hot leaf");
    }
    assert_eq!(
        super::super::cache::jit_compiled_ids().len(),
        2,
        "both leaves hot in the JIT set"
    );

    // CAP=1 → exactly ONE .so emitted (the shutdown-budget lever).
    assert_eq!(
        drain_aot_pgo_to_dir(&c, dir.path(), 1),
        1,
        "cap=1 bounds emit"
    );
    assert_eq!(count_so(dir.path()), 1);

    // CAP high → the remaining leaf emits; the first is `.exists()`-skipped (no dup).
    assert_eq!(
        drain_aot_pgo_to_dir(&c, dir.path(), 128),
        1,
        "second drain emits only the not-yet-persisted leaf"
    );
    assert_eq!(
        count_so(dir.path()),
        2,
        "two distinct bodies → two .so total"
    );

    // A THIRD drain is a full no-op (both `.exists()`).
    assert_eq!(
        drain_aot_pgo_to_dir(&c, dir.path(), 128),
        0,
        ".exists() skip → re-drain no-op"
    );
    assert_eq!(count_so(dir.path()), 2);
    super::super::cache::clear();
}

/// `jit_compiled_ids` reports the proven-hot JIT set; a NON-compilable body is
/// excluded (never a drain candidate) — and the env-gated `drain_aot_pgo`
/// wrapper is a no-op by default even with a hot leaf present.
#[test]
fn pgo_jit_set_and_wrapper_default_off() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::intern;

    let dir = tempfile::tempdir().expect("tempdir");
    super::super::cache::clear();
    super::super::compile::force_profit_gate_for_test(false);
    let mut c = Context::new();
    pgo_alias(&mut c, "pgo-set-callee", "recordp");
    c.obarray.set_symbol_function(
        "pgo-set-fn",
        Value::make_bytecode(pgo_pred_body("pgo-set-callee")),
    );
    let bc = c
        .obarray
        .symbol_function_id(intern("pgo-set-fn"))
        .and_then(|v| v.get_bytecode_data())
        .expect("bound bytecode");
    let id = super::super::cache::compile_and_cache_jit_leaf(bc, Some(&c.obarray))
        .expect("jit-compile hot leaf");
    assert!(
        super::super::cache::jit_compiled_ids().contains(&id),
        "the staged JIT leaf is in the hot set"
    );

    // Default-off wrapper: NEOVM_AOT_PGO unset → drain_aot_pgo is a no-op even
    // though a hot leaf exists; the testable core WOULD have drained it.
    assert_eq!(
        drain_aot_pgo(&c),
        0,
        "wrapper no-op when NEOVM_AOT_PGO unset"
    );
    assert_eq!(
        count_so(dir.path()),
        0,
        "no surprise cache files by default"
    );
    assert_eq!(
        drain_aot_pgo_to_dir(&c, dir.path(), 128),
        1,
        "the core drains the same hot leaf when explicitly targeted"
    );
    super::super::cache::clear();
}

/// An AOT leaf carries no GNU byte-offset map, so a `switch` jump table's
/// byte offsets could not be resolved on the AOT paths: both entry points
/// turn a Switch body away (JIT-only) before any analysis reads the table.
#[test]
fn aot_switch_bodies_stay_jit_only() {
    use crate::emacs_core::value::HashTableTest;
    let table = Value::hash_table(HashTableTest::Eq);
    let _ = table.with_hash_table_mut(|ht| {
        let key = Value::symbol("jit-aot-sw").to_hash_key(&ht.test);
        // Byte offset 8: past the end if read as an instruction index.
        ht.insert(key, Value::symbol("jit-aot-sw"), Value::fixnum(8));
    });
    let ops = vec![
        Op::StackRef(0),
        Op::Constant(0),
        Op::Switch,
        Op::Constant(1),
        Op::Return,
        Op::Constant(2),
        Op::Return,
    ];
    let constants = vec![table, Value::make_int(10), Value::make_int(20)];
    assert!(aot_body_has_switch(&ops));
    assert!(matches!(
        compile_leaf_to_object(&ops, &constants, 1, None),
        Ok(None)
    ));
    let leaf = LoadupLeaf {
        name: "jit-aot-switch".to_string(),
        ops: Box::leak(ops.into_boxed_slice()),
        constants: Box::leak(constants.into_boxed_slice()),
        arity: 1,
    };
    let (_, stats) =
        build_preload_object(std::slice::from_ref(&leaf), None).expect("build preload object");
    assert_eq!(
        (stats.candidates, stats.prepared, stats.skipped_unsupported),
        (1, 0, 1)
    );
}
