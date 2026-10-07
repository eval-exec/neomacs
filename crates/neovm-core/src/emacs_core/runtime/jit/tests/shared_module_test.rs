//! The persistent per-thread JIT backend and its code arena (P2.4 B4):
//! leaves compile into one long-lived module per allocator, code lives in
//! arena pages that are sealed once and never freed, and the module's
//! bookkeeping stays bounded however many leaves a thread compiles.

use super::*;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::stats;
use crate::emacs_core::value::LambdaParams;

fn function(ops: Vec<Op>, constants: Vec<Value>, arity: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity).map(|i| SymId(i as u32 + 1)).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 16;
    f.seal_hand_assembled_ops();
    f
}

/// `(lambda (x) (+ x k))`: a tiny MIR leaf, distinct per `k`.
fn add_k(k: i64) -> ByteCodeFunction {
    function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(k)],
        1,
    )
}

/// `(lambda (n) (let ((i 0)) (while (< i n) (setq i (1+ i))) i))`: a loop
/// (the full allocator).
fn count_to() -> ByteCodeFunction {
    function(
        vec![
            Op::Constant(0),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Lss,
            Op::GotoIfNil(9),
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::Goto(1),
            Op::Return,
        ],
        vec![Value::make_int(0)],
        1,
    )
}

/// Compile `f` through the cache (the first-sight seam: no heat needed) and
/// return its cached leaf.
fn compile_cached(ctx: *mut Context, f: &ByteCodeFunction) -> &'static CompiledLeaf {
    let ptr = cache::resolve_compiled_leaf_ptr(ctx, f).expect("compiles");
    // SAFETY: a cached leaf stays allocated for the thread's life (retired
    // leaves are kept); tests never clear the cache under this reference.
    unsafe { &*ptr }
}

fn call1(leaf: &CompiledLeaf, x: i64) -> Option<i64> {
    leaf.call_for_test(&[Value::make_int(x)])
        .map(|bits| Value::from_bits(bits).as_fixnum().expect("fixnum result"))
}

fn is_shared(leaf: &CompiledLeaf) -> bool {
    matches!(leaf._backing, LeafBacking::Shared)
}

/// The mapping of `/proc/self/maps` containing `addr`: its permissions.
fn permissions_at(addr: usize) -> Option<String> {
    let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
    maps.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let (lo, hi) = fields.next()?.split_once('-')?;
        let lo = usize::from_str_radix(lo, 16).ok()?;
        let hi = usize::from_str_radix(hi, 16).ok()?;
        (lo..hi)
            .contains(&addr)
            .then(|| fields.next().map(str::to_string))
            .flatten()
    })
}

fn mapping_count() -> usize {
    std::fs::read_to_string("/proc/self/maps")
        .map(|m| m.lines().count())
        .unwrap_or(0)
}

fn page_of(addr: *const u8) -> usize {
    // SAFETY: sysconf has no preconditions.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
    addr as usize / page
}

fn setup(persistent: bool) -> Context {
    force_deopt_for_test(false);
    shared::force_persistent_module_for_test(persistent);
    Context::new()
}

/// By default a leaf compiles into the persistent module and keeps nothing
/// but its entry; the knob sends it back to a module of its own.
#[test]
fn jit_shared_module_is_the_default_and_the_knob_restores_per_leaf_modules() {
    let mut ev = setup(true);
    let ctx = &mut ev as *mut Context;
    let leaf = compile_cached(ctx, &add_k(1));
    assert!(is_shared(leaf), "{:?}", leaf._backing);
    assert_eq!(call1(leaf, 41), Some(42));
    let s = shared::code_memory_stats();
    assert_eq!(s.shared_leaves, 1, "{s:?}");
    assert_eq!(s.modules_created, 1, "{s:?}");
    assert_eq!(s.per_leaf_modules, 0, "{s:?}");

    shared::force_persistent_module_for_test(false);
    let legacy = compile_cached(ctx, &add_k(2));
    assert!(matches!(legacy._backing, LeafBacking::Jit(_)));
    assert_eq!(call1(legacy, 40), Some(42));
    assert_eq!(shared::code_memory_stats().per_leaf_modules, 1);
    assert_eq!(call1(leaf, 1), Some(2), "the shared leaf is unaffected");
}

/// The persistent module generates the same machine code as a module per
/// leaf (addresses masked): only where the code lives changes.
#[test]
fn jit_shared_module_is_code_identical_to_per_leaf_modules() {
    use super::compile_pipeline_tests::{compile_corpus_for_test, corpus_len_for_test};
    shared::force_persistent_module_for_test(false);
    let per_leaf = compile_corpus_for_test();
    shared::force_persistent_module_for_test(true);
    let persistent = compile_corpus_for_test();
    assert_eq!(per_leaf.len(), corpus_len_for_test());
    assert_eq!(per_leaf, persistent);
}

/// Sealed code is read+execute, a sealed page is never handed out again,
/// and consecutive leaves take consecutive pages of the arena.
#[test]
fn jit_shared_arena_seals_pages_and_reuses_the_region() {
    let mut ev = setup(true);
    let ctx = &mut ev as *mut Context;
    let a = compile_cached(ctx, &add_k(10));
    let b = compile_cached(ctx, &add_k(20));
    let c = compile_cached(ctx, &add_k(30));
    for leaf in [a, b, c] {
        assert!(is_shared(leaf));
        let perms = permissions_at(leaf.entry as usize).expect("entry is mapped");
        assert!(perms.starts_with("r-x"), "sealed read+execute: {perms}");
    }
    let (pa, pb, pc) = (page_of(a.entry), page_of(b.entry), page_of(c.entry));
    assert!(
        pa < pb && pb < pc,
        "fresh pages after each seal: {pa} {pb} {pc}"
    );
    assert!(
        pc - pa <= 4,
        "tiny leaves take adjacent pages of one region: {pa} {pb} {pc}"
    );
    let s = shared::code_memory_stats();
    assert_eq!(s.arena_regions, 1, "{s:?}");
    assert_eq!(s.arena_seals, 3, "one seal per synchronous leaf: {s:?}");
    assert!(s.arena_code_bytes > 0 && s.arena_code_bytes <= s.arena_page_bytes);
    assert_eq!(
        (call1(a, 1), call1(b, 1), call1(c, 1)),
        (Some(11), Some(21), Some(31))
    );
}

/// A region that fills up is followed by a fresh one; every leaf in every
/// region stays callable.
#[test]
fn jit_shared_arena_rolls_over_to_new_regions() {
    let mut ev = setup(true);
    let ctx = &mut ev as *mut Context;
    // SAFETY: sysconf has no preconditions.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
    shared::reset_backend_for_test(Some(3 * page));
    let leaves: Vec<(i64, &CompiledLeaf)> = (0..40)
        .map(|k| (k, compile_cached(ctx, &add_k(k))))
        .collect();
    let s = shared::code_memory_stats();
    assert!(s.arena_regions >= 10, "3-page regions: {s:?}");
    for (k, leaf) in leaves {
        assert_eq!(call1(leaf, 5), Some(5 + k));
    }
}

/// Many compiles on one thread: every leaf stays callable, the module is
/// replaced every `limit` leaves (its bookkeeping is freed; its code stays),
/// the live module never holds more than `limit` leaves, and the process
/// mapping count does not grow with the compile count (sealed pages merge
/// into one mapping instead of a mapping per leaf).
#[test]
fn jit_shared_many_compiles_keep_bookkeeping_and_mappings_bounded() {
    const N: i64 = 20_000;
    const LIMIT: u32 = 256;
    let mut ev = setup(true);
    let ctx = &mut ev as *mut Context;
    shared::force_module_leaf_limit_for_test(LIMIT);
    // Warm the arena and the module so the baseline counts them.
    let first = compile_cached(ctx, &add_k(-1));
    let maps_before = mapping_count();
    let mut sample: Vec<(i64, &CompiledLeaf)> = Vec::new();
    for k in 0..N {
        let leaf = compile_cached(ctx, &add_k(k));
        if k % 997 == 0 {
            sample.push((k, leaf));
        }
        assert!(
            shared::live_modules_for_test()
                .iter()
                .all(|&leaves| leaves <= LIMIT),
            "a live module holds at most {LIMIT} leaves"
        );
    }
    let maps_after = mapping_count();
    let s = shared::code_memory_stats();
    assert_eq!(s.shared_leaves, N as u64 + 1, "{s:?}");
    assert!(
        s.modules_retired >= (N as u64) / u64::from(LIMIT) - 1,
        "modules are replaced as they fill: {s:?}"
    );
    assert!(
        maps_after <= maps_before + 64,
        "{N} compiles grew the mapping count {maps_before} -> {maps_after}"
    );
    assert!(s.arena_regions <= 4, "{s:?}");
    // SAFETY: sysconf has no preconditions.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;
    assert!(
        s.arena_page_bytes <= (N as u64 + 1) * page,
        "code memory is one page per tiny leaf, as with a module per leaf: {s:?}"
    );
    assert_eq!(
        call1(first, 1),
        Some(0),
        "the first leaf outlived its module"
    );
    for (k, leaf) in sample {
        assert!(is_shared(leaf));
        assert_eq!(call1(leaf, 3), Some(3 + k));
    }
}

/// A leaf recompiled under the same per-function name (a re-tier, a
/// recompile after eviction) and an OSR variant of the same function all
/// coexist in one module: entries are declared under unique names.
#[test]
fn jit_shared_same_label_recompiles_and_osr_coexist() {
    let mut ev = setup(true);
    let ctx = &mut ev as *mut Context;
    stats::force_observe_for_test(stats::ObserveOverride {
        stats: false,
        naming: true,
        entry_count: false,
    });
    let f = count_to();
    let first = compile_cached(ctx, &f);
    assert_eq!(call1(first, 7), Some(7));
    let id = f.jit_runtime().compiled_id().expect("compiled");
    for round in 0..3 {
        cache::evict_compiled(id);
        let again = compile_cached(ctx, &f);
        assert!(is_shared(again));
        assert_eq!(call1(again, 5 + round), Some(5 + round));
    }
    // The OSR variant at the loop header (pc 1, operand depth 2).
    let osr = lower_leaf_full_osr(
        f.executable_ops(),
        &f.constants,
        1,
        None,
        Some(&ev.obarray),
        Some(1),
        0,
    )
    .expect("osr leaf");
    assert!(is_shared(&osr));
    // Seed the header's operand stack [n i] = [9 4]: the loop resumes at i=4.
    let seeded = [
        Value::make_int(9).bits() as i64,
        Value::make_int(4).bits() as i64,
    ];
    match osr.call_premarshaled(ctx as *mut u8, seeded.as_ptr()) {
        NativeRun::Ok(bits) => assert_eq!(Value::from_bits(bits), Value::make_int(9)),
        other => panic!("OSR run: {other:?}"),
    }
    assert_eq!(
        call1(first, 3),
        Some(3),
        "the retired first leaf still runs"
    );
    assert_eq!(shared::code_memory_stats().per_leaf_modules, 0);
}

/// Straight-line and looping bodies compile into the two allocators'
/// modules; both live side by side.
#[test]
fn jit_shared_both_allocators_have_their_module() {
    let mut ev = setup(true);
    let ctx = &mut ev as *mut Context;
    let straight = compile_cached(ctx, &add_k(3));
    let looping = compile_cached(ctx, &count_to());
    assert_eq!(straight.regalloc, lowering::RegallocChoice::Fast);
    assert_eq!(looping.regalloc, lowering::RegallocChoice::Full);
    assert_eq!(shared::live_modules_for_test().len(), 2);
    assert_eq!(call1(straight, 1), Some(4));
    assert_eq!(call1(looping, 12), Some(12));
}

/// A cache `clear` drops the leaves (their boxes) but not the backend:
/// recompiles go into the same module and arena.
#[test]
fn jit_shared_cache_clear_then_recompile() {
    let mut ev = setup(true);
    let ctx = &mut ev as *mut Context;
    let f = add_k(7);
    assert_eq!(call1(compile_cached(ctx, &f), 1), Some(8));
    cache::clear();
    let g = add_k(8);
    let leaf = compile_cached(ctx, &g);
    assert_eq!(call1(leaf, 1), Some(9));
    let s = shared::code_memory_stats();
    assert_eq!(s.modules_created, 1, "the module survives the clear: {s:?}");
    assert_eq!(s.shared_leaves, 2);
}

/// The every-guard-fails harness on shared-module leaves: each run deopts
/// and resumes exactly.
#[test]
fn jit_shared_force_deopt_harness() {
    let mut ev = setup(true);
    force_deopt_for_test(true);
    let ctx = &mut ev as *mut Context;
    let f = add_k(1);
    let f_val = Value::make_bytecode(f.clone());
    for i in 0..4 {
        let got = crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[Value::make_int(i)])
            .expect("no signal");
        assert!(
            got.is_none_or(|bits| bits == Value::make_int(i + 1).bits()),
            "a precise resume is exact; a rerun falls back to the caller"
        );
    }
    let id = f.jit_runtime().compiled_id().expect("compiled");
    // SAFETY: the cached leaf stays allocated (see `compile_cached`).
    let leaf = unsafe { &*cache::compiled_leaf_ptr_for_test(id).expect("cached") };
    assert!(is_shared(leaf));
    let obs = leaf.obs.snapshot();
    assert_eq!(obs.deopt_at + obs.deopt_rerun, 4, "{obs:?}");
    force_deopt_for_test(false);
}

/// A heap constant referenced only by shared-module code stays alive across
/// a collection: the leaf's reloc vector is still the root, as before.
#[test]
fn jit_shared_leaf_constants_survive_gc() {
    let mut ev = setup(true);
    let ctx = &mut ev as *mut Context;
    let text = "shared-module constant survives gc";
    // (lambda () "…"): the string lives in the constant vector of a Rust
    // struct the GC cannot see, and in the leaf's reloc vector.
    let f = function(
        vec![Op::Constant(0), Op::Return],
        vec![Value::string(text)],
        0,
    );
    let leaf = compile_cached(ctx, &f);
    assert!(is_shared(leaf));
    assert!(!leaf.reloc_values().is_empty(), "the string is reloc'd");
    for _ in 0..3 {
        ev.gc_collect_exact();
    }
    let bits = leaf.call_for_test(&[]).expect("runs");
    assert_eq!(
        Value::from_bits(bits).as_runtime_string_owned().as_deref(),
        Some(text)
    );
}
