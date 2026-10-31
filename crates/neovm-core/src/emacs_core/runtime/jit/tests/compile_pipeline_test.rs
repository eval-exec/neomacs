//! The compile pipeline's fixed-cost machinery (P2.4 B0-B4): compile origins
//! and phase timers through the real cache seams.

use super::*;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::stats::{self, CompileOrigin, CompilePhase};
use crate::emacs_core::value::LambdaParams;

pub(crate) fn function(ops: Vec<Op>, constants: Vec<Value>, arity: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity).map(|i| SymId(i as u32 + 1)).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(16);
    f.seal_hand_assembled_ops();
    f
}

fn observe_stats() {
    stats::force_observe_for_test(stats::ObserveOverride {
        stats: true,
        naming: false,
        entry_count: false,
    });
}

/// `(lambda (x) (+ x 1))`.
fn add1() -> ByteCodeFunction {
    function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(1)],
        1,
    )
}

/// A dispatch-seam compile lands in the `dispatch` origin row, and its phase
/// split covers the backend (setup, codegen, finalize) and sums to the
/// stall aggregate.
#[test]
fn jit_pipeline_dispatch_compile_is_split_by_phase() {
    // This per-thread clock sums front and backend phases on the same thread.
    // Worker front/latency accounting is observed separately in bg tests.
    let _backend =
        crate::emacs_core::jit::bg::TestModeScope::enter(crate::emacs_core::jit::bg::BgMode::Sync);
    force_deopt_for_test(false);
    observe_stats();
    stats::reset_compile_stats();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    let f_val = Value::make_bytecode(f.clone());
    // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
    let got =
        unsafe { crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[Value::make_int(41)]) }
            .expect("no signal");
    assert_eq!(got, Some(Value::make_int(42).bits()));
    let s = stats::compile_stats_snapshot();
    assert_eq!(s.total_compiles, 1);
    let row = s.origins[CompileOrigin::Dispatch as usize];
    assert_eq!((row.count, row.ok, row.us), (1, 1, s.total_us), "{s:?}");
    for phase in [
        CompilePhase::Gate,
        CompilePhase::Lower,
        CompilePhase::Codegen,
        CompilePhase::Finalize,
    ] {
        assert!(s.phase_ns[phase as usize] > 0, "{phase:?} unclaimed: {s:?}");
    }
    let split_us = s.phase_ns.iter().sum::<u64>() / 1_000;
    assert!(
        split_us.abs_diff(s.total_us) <= 1,
        "split {split_us}us vs stall {}us",
        s.total_us
    );
    // The leaf remembers its own stall for the exit report's leaf rows.
    let id = f.jit_runtime().compiled_id().expect("compiled");
    let row = cache::leaf_report_rows()
        .0
        .into_iter()
        .find(|r| r.id == id)
        .expect("cached leaf row");
    assert_eq!(u64::from(row.obs.compile_us), s.total_us);
}

/// A spec site's first call into an uncompiled callee is a `first_sight`
/// compile; a compile outside the cache is `direct` and is not a stall.
#[test]
fn jit_pipeline_first_sight_and_direct_origins() {
    force_deopt_for_test(false);
    observe_stats();
    stats::reset_compile_stats();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    assert!(crate::emacs_core::jit::test_support::resolve_leaf(ctx, &f).is_some());
    let g = add1();
    compile_bytecode_function_with(&g, None).expect("compiles");
    let s = stats::compile_stats_snapshot();
    assert_eq!(s.origins[CompileOrigin::FirstSight as usize].count, 1);
    assert_eq!(
        s.origins[CompileOrigin::Direct as usize].count,
        0,
        "only the cache seams run a compile clock"
    );
    assert_eq!(s.total_compiles, 1);
}

/// Mask what legitimately differs between two compiles of one body: baked
/// addresses (hex literals of 6+ digits, and aarch64 `movz`/`movk` runs that
/// build a runtime address) and module-local external-name indices
/// (`userextname3`).
pub(crate) fn mask_code_text(text: &str) -> String {
    let text = &mask_aarch64_address_materialization(text);
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if text[i..].starts_with("0x") {
            let digits = text[i + 2..]
                .bytes()
                .take_while(u8::is_ascii_hexdigit)
                .count();
            if digits >= 6 {
                out.push_str("0xADDR");
                i += 2 + digits;
                continue;
            }
        }
        if text[i..].starts_with("userextname") {
            out.push_str("userextname");
            i += "userextname".len();
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            continue;
        }
        let ch = text[i..].chars().next().expect("in bounds");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// One aarch64 wide-immediate move: `movz xN, #imm[, LSL #s]` or
/// `movk xN, xN, #imm, LSL #s`, as Cranelift's disassembly prints them.
struct WideMove<'a> {
    keeps: bool,
    reg: &'a str,
    imm: u64,
    shift: u32,
}

fn parse_wide_move(line: &str) -> Option<WideMove<'_>> {
    let trimmed = line.trim_start();
    let (keeps, operands) = if let Some(rest) = trimmed.strip_prefix("movz ") {
        (false, rest)
    } else if let Some(rest) = trimmed.strip_prefix("movk ") {
        (true, rest)
    } else {
        return None;
    };
    let mut parts = operands.split(", ");
    let reg = parts.next()?;
    let mut imm = None;
    let mut shift = 0;
    for part in parts {
        if let Some(amount) = part.strip_prefix("LSL #") {
            shift = amount.trim().parse().ok()?;
        } else if let Some(value) = part.strip_prefix('#') {
            imm = Some(value.trim().parse().ok()?);
        }
    }
    Some(WideMove {
        keeps,
        reg,
        imm: imm?,
        shift,
    })
}

/// Marker for an aarch64 address materialization folded by
/// [`mask_aarch64_address_materialization`]. Its instruction count, and so the
/// leaf's size, depends on the address (a zero 16-bit half needs no `movk`).
const AARCH64_ADDRESS_RUN: &str = "movaddr";

/// Fold every aarch64 `movz` + `movk`... run that builds a runtime address
/// (see [`is_runtime_address`]) into one `movaddr xN, #ADDR` line. aarch64 has
/// no 64-bit immediate move, so a baked address arrives as 16-bit halves that
/// no hex mask sees, and an address with a zero half takes one move fewer:
/// two compiles of one body baking different boxes differ only there. A run
/// building any other constant is left alone, so a changed constant still
/// shows.
fn mask_aarch64_address_materialization(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let Some(first) = parse_wide_move(lines[i]).filter(|m| !m.keeps) else {
            out.push(lines[i].to_owned());
            i += 1;
            continue;
        };
        let mut value = first.imm << first.shift;
        let mut end = i + 1;
        while let Some(next) = lines.get(end).and_then(|line| parse_wide_move(line)) {
            if !next.keeps || next.reg != first.reg {
                break;
            }
            value = (value & !(0xffff << next.shift)) | (next.imm << next.shift);
            end += 1;
        }
        if is_runtime_address(value) {
            let indent = &lines[i][..lines[i].len() - lines[i].trim_start().len()];
            out.push(format!(
                "{indent}{AARCH64_ADDRESS_RUN} {}, #ADDR",
                first.reg
            ));
        } else {
            out.extend(lines[i..end].iter().map(|line| (*line).to_owned()));
        }
        i = end;
    }
    out.join("\n")
}

/// A `movz`/`movk` run that builds an address folds whole, whatever its
/// length; one building a constant, a lone small `movz`, and other registers'
/// moves stay visible.
#[test]
fn mask_code_text_hides_aarch64_materialized_addresses_only() {
    let base = "  movz x15, #39616\n  movk x15, x15, #53645, LSL #16\n  movk x15, x15, #8, LSL #32\n  ldr x0, [x15]";
    let moved = "  movz x15, #39616\n  movk x15, x15, #53834, LSL #16\n  movk x15, x15, #8, LSL #32\n  ldr x0, [x15]";
    // An address whose low half is zero starts at LSL #16: one move fewer.
    let zero_low = "  movz x15, #53834, LSL #16\n  movk x15, x15, #8, LSL #32\n  ldr x0, [x15]";
    assert_eq!(mask_code_text(base), mask_code_text(moved));
    assert_eq!(mask_code_text(base), mask_code_text(zero_low));
    assert_eq!(
        mask_code_text(base),
        "  movaddr x15, #ADDR\n  ldr x0, [x15]"
    );
    // 0xffff_ffff is a mask, not an address: the run stays as it is.
    let constant = "  movz x3, #65535\n  movk x3, x3, #65535, LSL #16";
    assert_eq!(mask_code_text(constant), constant);
    // A lone small `movz` and a changed small immediate are not hidden.
    assert_ne!(
        mask_code_text("  movz x1, #24"),
        mask_code_text("  movz x1, #32")
    );
    // A `movk` of another register ends the run.
    let split = "  movz x15, #39616\n  movk x16, x16, #53645, LSL #16";
    assert_eq!(mask_code_text(split), split);
}

/// The `size=` header of a captured leaf: exact, unless the masked code folded
/// an aarch64 address run, whose length depends on the address.
fn captured_size(size: &str, masked_code: &str) -> String {
    if masked_code.contains(AARCH64_ADDRESS_RUN) {
        "size=ADDRESS-DEPENDENT".to_owned()
    } else {
        size.to_owned()
    }
}

/// Hex immediates in `[ADDRESS_FLOOR, ADDRESS_CEILING)` are runtime
/// addresses (shim entries, statics, heap boxes, leaf buffers). The window
/// covers every platform and allocator: macOS arm64 maps the heap and the
/// binary from 2^32 up, Linux x86-64 sits near 2^46..2^47, Linux arm64 reaches
/// 2^48. Constants the lowering emits that are not addresses (tag and payload
/// masks, fixnum range bounds, sign-extended words) are outside it or are
/// single-bit / all-ones patterns, which [`is_runtime_address`] excludes.
// 64-bit Mach-O reserves the low 4 GiB as __PAGEZERO, so no macOS user mapping
// sits below 2^32 (lowest seen: statics at 0x1_08a7_4b70) unless the binary is
// linked with a custom -pagezero_size.
const ADDRESS_FLOOR: u64 = 1 << 32;
const ADDRESS_CEILING: u64 = 1 << 48;

fn is_runtime_address(value: u64) -> bool {
    (ADDRESS_FLOOR..ADDRESS_CEILING).contains(&value)
        && !value.is_power_of_two()
        && value & value.wrapping_add(1) != 0
}

/// Rewrite every runtime-address immediate in CLIF `text` to `0xADDR<n>`,
/// where `n` numbers the distinct addresses by first appearance. Two dumps of
/// one body then compare equal wherever their address operands have the same
/// shape (same address reused, same count of distinct addresses), whatever
/// the platform put the heap at; a changed constant, instruction or
/// address-sharing pattern still differs. Cranelift prints immediates of
/// 10000 or more in hex with `_` digit groups (`0x7fff_e833_27a0`).
pub(crate) fn mask_runtime_addresses(text: &str) -> String {
    let mut seen: Vec<u64> = Vec::new();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("0x") {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 2..];
        let len = tail
            .find(|c: char| !(c.is_ascii_hexdigit() || c == '_'))
            .unwrap_or(tail.len());
        let address = u64::from_str_radix(&tail[..len].replace('_', ""), 16)
            .ok()
            .filter(|&value| is_runtime_address(value));
        match address {
            Some(value) => {
                let index = seen.iter().position(|&v| v == value).unwrap_or_else(|| {
                    seen.push(value);
                    seen.len() - 1
                });
                out.push_str(&format!("0xADDR{index}"));
            }
            None => out.push_str(&rest[at..at + 2 + len]),
        }
        rest = &tail[len..];
    }
    out.push_str(rest);
    out
}

/// Addresses from any platform's heap or binary mapping mask to the same
/// first-appearance names; constants that are not addresses survive.
#[test]
fn mask_runtime_addresses_hides_addresses_of_every_platform() {
    let linux = "v1 = iconst.i64 0x7fff_e833_27a0\nv2 = iconst.i64 0x7fff_e833_27a8\nv3 = iconst.i64 0x7fff_e833_27a0";
    let macos = "v1 = iconst.i64 0x1_0123_4560\nv2 = iconst.i64 0x1_0123_4568\nv3 = iconst.i64 0x1_0123_4560";
    let arm_linux = "v1 = iconst.i64 0xffff_9abc_def0\nv2 = iconst.i64 0xffff_9abc_def8\nv3 = iconst.i64 0xffff_9abc_def0";
    let want = "v1 = iconst.i64 0xADDR0\nv2 = iconst.i64 0xADDR1\nv3 = iconst.i64 0xADDR0";
    for dump in [linux, macos, arm_linux] {
        assert_eq!(mask_runtime_addresses(dump), want);
    }
}

#[test]
fn mask_runtime_addresses_keeps_constants_and_still_tells_dumps_apart() {
    let constants = "iconst.i64 0x1fff_ffff_ffff_ffff\niconst.i64 0xffff_ffff\niconst.i64 0x1_0000_0000\niconst.i64 0x18\niconst.i64 4096";
    assert_eq!(mask_runtime_addresses(constants), constants);
    let base = "iconst.i64 0x7fff_e833_27a0\niconst.i64 0x7fff_e833_27a8";
    // The same two addresses, but one reused: a different shape.
    let reused = "iconst.i64 0x7fff_e833_27a0\niconst.i64 0x7fff_e833_27a0";
    assert_ne!(mask_runtime_addresses(base), mask_runtime_addresses(reused));
    // A changed non-address immediate or instruction is not hidden.
    let other = "iconst.i64 0x7fff_e833_27a0\niconst.i64 0x7fff_e833_27a8\niconst.i64 0x1fff_ffff_ffff_fffe";
    assert_ne!(
        mask_runtime_addresses(&format!("{base}\niconst.i64 0x1fff_ffff_ffff_ffff")),
        mask_runtime_addresses(other)
    );
    assert_ne!(
        mask_runtime_addresses("iadd v1, v2"),
        mask_runtime_addresses("isub v1, v2")
    );
}

/// The register-allocated code of every leaf `compile` builds on this
/// thread, one entry per leaf: `size=` plus the masked disassembly (the
/// `NEOVM_JIT_DUMP_ASM` capture; addresses and bytes are dropped).
pub(crate) fn captured_code(compile: impl FnOnce()) -> Vec<String> {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("leaves.asm");
    stats::asm_dump::force_asm_dump_for_test(Some(path.clone()));
    compile();
    stats::asm_dump::force_asm_dump_for_test(None);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    text.split(";; ==== ")
        .filter(|chunk| !chunk.trim().is_empty())
        .map(|chunk| {
            let (header, rest) = chunk.split_once('\n').expect("header line");
            let size = header
                .split_whitespace()
                .find(|field| field.starts_with("size="))
                .expect("size field");
            let code = rest.split(";; bytes:").next().expect("disassembly");
            let masked = mask_code_text(code);
            format!("{}\n{masked}", captured_size(size, &masked))
        })
        .collect()
}

/// A body mix that reaches both tiers and most shim families: a pure MIR
/// leaf, a baseline leaf with calls, a loop, a condition-case and float
/// arithmetic.
pub(crate) fn corpus() -> Vec<(Vec<Op>, Vec<Value>, usize)> {
    let sym = |name: &str| Value::symbol(name);
    vec![
        // (lambda (x) (+ x 1)): MIR.
        (
            vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
            vec![Value::make_int(1)],
            1,
        ),
        // (lambda (x) (car (cons x x))): cons + car.
        (
            vec![Op::StackRef(0), Op::Dup, Op::Cons, Op::Car, Op::Return],
            vec![],
            1,
        ),
        // (lambda (x) (list x (symbol-value 'foo))): a varref and a list.
        (
            vec![Op::StackRef(0), Op::VarRef(1), Op::List(2), Op::Return],
            vec![Value::NIL, sym("jit-pipeline-foo")],
            1,
        ),
        // (lambda (n) (let ((i 0)) (while (< i n) (setq i (1+ i))) i)): loop.
        (
            vec![
                Op::Constant(0),  // 0: i = 0         [n i]
                Op::StackRef(0),  // 1: i             [n i i]
                Op::StackRef(2),  // 2: n             [n i i n]
                Op::Lss,          // 3: (< i n)       [n i b]
                Op::GotoIfNil(9), // 4:               [n i]
                Op::StackRef(0),  // 5: i             [n i i]
                Op::Add1,         // 6:               [n i i+1]
                Op::StackSet(1),  // 7: i = i+1       [n i]
                Op::Goto(1),      // 8
                Op::Return,       // 9: i
            ],
            vec![Value::make_int(0)],
            1,
        ),
        // (lambda (x y) (* x y)) on floats would need feedback; plain `*`.
        (
            vec![Op::StackRef(1), Op::StackRef(1), Op::Mul, Op::Return],
            vec![],
            2,
        ),
        // (lambda (x) (eq x 'a)): eq + symbol constant.
        (
            vec![Op::StackRef(0), Op::Constant(0), Op::Eq, Op::Return],
            vec![sym("jit-pipeline-a")],
            1,
        ),
        // (lambda (f x) (+ 1 (funcall f x))): a generic call (baseline, call
        // shim) with enough arithmetic to pass the profitability gate.
        (
            vec![
                Op::Constant(0),
                Op::StackRef(2),
                Op::StackRef(2),
                Op::Call(1),
                Op::Add,
                Op::Return,
            ],
            vec![Value::make_int(1)],
            2,
        ),
    ]
}

/// Compile every corpus body through the cache (a fresh function object per
/// body, so each compiles) and return its captured code.
fn compile_corpus() -> Vec<String> {
    force_deopt_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    captured_code(|| {
        for (ops, constants, arity) in corpus() {
            let f = function(ops, constants, arity);
            assert!(
                crate::emacs_core::jit::test_support::resolve_leaf(ctx, &f).is_some(),
                "every corpus body compiles: {:?}",
                f.ops
            );
        }
    })
}

/// T-S3: the cached ISA carries exactly the flags a fresh build has, for
/// both allocators, and is shared rather than rebuilt.
#[test]
fn jit_pipeline_cached_isa_matches_a_fresh_build() {
    use lowering::{
        RegallocChoice, RegallocScope, build_jit_isa, force_isa_cache_for_test, jit_isa,
    };
    force_isa_cache_for_test(true);
    for choice in [RegallocChoice::Fast, RegallocChoice::Full] {
        let _scope = RegallocScope::enter(choice);
        let cached = jit_isa().expect("isa");
        let again = jit_isa().expect("isa");
        assert!(
            std::sync::Arc::ptr_eq(&cached, &again),
            "{choice:?}: one ISA per allocator"
        );
        let fresh = build_jit_isa(choice).expect("isa");
        let shared = |isa: &cranelift_codegen::isa::OwnedTargetIsa| {
            isa.flags()
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
        };
        let specific = |isa: &cranelift_codegen::isa::OwnedTargetIsa| {
            isa.isa_flags()
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(shared(&cached), shared(&fresh), "{choice:?}");
        assert_eq!(specific(&cached), specific(&fresh), "{choice:?}");
        assert_eq!(cached.triple(), fresh.triple());
        assert!(
            shared(&cached).contains(&format!(
                "regalloc_algorithm={}",
                choice.cranelift_setting()
            )),
            "{choice:?}: {:?}",
            shared(&cached)
        );
    }
    force_isa_cache_for_test(false);
    let _scope = RegallocScope::enter(RegallocChoice::Full);
    assert!(
        !std::sync::Arc::ptr_eq(&jit_isa().expect("isa"), &jit_isa().expect("isa")),
        "NEOVM_JIT_ISA_CACHE=off builds per compile"
    );
}

/// The ISA cache changes no generated code: the corpus compiles to the same
/// machine code (addresses masked) with the cache on and off.
#[test]
fn jit_pipeline_isa_cache_is_code_identical() {
    lowering::force_isa_cache_for_test(false);
    let fresh = compile_corpus();
    lowering::force_isa_cache_for_test(true);
    let cached = compile_corpus();
    assert_eq!(fresh.len(), corpus().len(), "{fresh:?}");
    assert_eq!(fresh, cached);
}

/// The CLIF text of every function `compile` lowers on this thread (the
/// `NEOVM_JIT_DUMP_CLIF` capture), one entry per function.
pub(crate) fn captured_clif(compile: impl FnOnce()) -> Vec<String> {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("leaves.clif");
    lowering::force_clif_dump_for_test(Some(path.to_string_lossy().into_owned()));
    compile();
    lowering::force_clif_dump_for_test(None);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    text.split("\n;; ")
        .filter(|chunk| !chunk.trim().is_empty())
        .map(str::to_string)
        .collect()
}

/// How many external functions a CLIF function declares (`fnN = ...`).
fn imported_functions(clif: &str) -> usize {
    clif.lines()
        .filter(|line| {
            let line = line.trim_start();
            line.starts_with("fn")
                && line
                    .split_once(" = ")
                    .is_some_and(|(name, _)| name[2..].bytes().all(|b| b.is_ascii_digit()))
        })
        .count()
}

/// Every [`Shim`] name is the exported symbol the JIT table registers (so
/// the JIT resolves it) and, except the JIT-only speculation shims, one an
/// AOT object may import.
#[test]
fn jit_pipeline_shim_table_names_every_registered_shim() {
    use strum::IntoEnumIterator;
    let registered: std::collections::HashSet<&str> =
        JIT_SHIM_TABLE.iter().map(|(name, _)| *name).collect();
    let mut symbols = std::collections::HashSet::new();
    for shim in Shim::iter() {
        assert!(
            registered.contains(shim.symbol()),
            "{shim:?} ({}) is not in JIT_SHIM_TABLE",
            shim.symbol()
        );
        assert!(symbols.insert(shim.symbol()), "{shim:?} named twice");
        let sig = shim.signature(
            cranelift_codegen::isa::CallConv::SystemV,
            cranelift_codegen::ir::types::I64,
        );
        assert!(sig.returns.len() <= 1, "{shim:?}");
    }
}

/// A body imports only the shims it calls: `(cons x x)` carries one
/// external function lazily and the whole base set eagerly — and the
/// machine code of the corpus is identical either way.
#[test]
fn jit_pipeline_lazy_shim_import_keeps_only_used_imports() {
    force_deopt_for_test(false);
    let cons_body = || {
        let mut ev = Context::new();
        let ctx = &mut ev as *mut Context;
        let f = function(
            vec![Op::StackRef(0), Op::Dup, Op::Cons, Op::Return],
            vec![],
            1,
        );
        assert!(crate::emacs_core::jit::test_support::resolve_leaf(ctx, &f).is_some());
    };
    shim_refs::force_lazy_shims_for_test(true);
    let lazy = captured_clif(cons_body);
    shim_refs::force_lazy_shims_for_test(false);
    let eager = captured_clif(cons_body);
    assert_eq!(lazy.len(), 1, "{lazy:?}");
    assert_eq!(eager.len(), 1, "{eager:?}");
    // The T2 prologue also calls TierRequest when its countdown expires.
    let used_imports = 1 + usize::from(jit_tier2().on);
    assert_eq!(imported_functions(&lazy[0]), used_imports, "{}", lazy[0]);
    assert!(
        imported_functions(&eager[0]) >= 41,
        "eager imports the whole base set: {}",
        eager[0]
    );

    shim_refs::force_lazy_shims_for_test(false);
    let eager_code = compile_corpus();
    shim_refs::force_lazy_shims_for_test(true);
    let lazy_code = compile_corpus();
    assert_eq!(eager_code.len(), corpus().len());
    assert_eq!(
        eager_code, lazy_code,
        "import order changes no machine code"
    );
}

/// The optional speculation groups stay refused when the body has no such
/// site, even from a module that declared them: `try_get` answers `None`.
#[test]
fn jit_pipeline_optional_shim_groups_follow_the_leaf_not_the_module() {
    use cranelift_codegen::ir::{Function, Signature, UserFuncName};
    use cranelift_codegen::isa::CallConv;
    let mut module = {
        let _scope = lowering::RegallocScope::enter(lowering::RegallocChoice::Fast);
        let mut builder = cranelift_jit::JITBuilder::with_isa(
            lowering::jit_isa().expect("isa"),
            cranelift_module::default_libcall_names(),
        );
        register_shims(&mut builder);
        cranelift_jit::JITModule::new(builder)
    };
    let every = ShimGroups {
        throw_flow: crate::emacs_core::jit::compile::shim_refs::ThrowShimSelection::Contextual,
        subr_spec: true,
        cbsym_spec: true,
        tier2_profile: true,
        direct_shapes: true,
        call_census: true,
        direct_framed: true,
        hof: true,
        collection_journal: true,
        collection_observation_gate: true,
    };
    let ids = ShimIds::declare(&mut module, CallConv::SystemV, types::I64, every).expect("ids");
    let again = ShimIds::declare(&mut module, CallConv::SystemV, types::I64, every).expect("ids");
    assert_eq!(
        ids.get(Shim::CallSpec),
        again.get(Shim::CallSpec),
        "declaring is idempotent per module"
    );
    shim_refs::force_lazy_shims_for_test(true);
    let mut func =
        Function::with_name_signature(UserFuncName::user(0, 0), Signature::new(CallConv::SystemV));
    let base_only = ShimGroups {
        throw_flow: crate::emacs_core::jit::compile::shim_refs::ThrowShimSelection::Unavailable,
        subr_spec: false,
        cbsym_spec: false,
        tier2_profile: false,
        direct_shapes: false,
        call_census: false,
        direct_framed: false,
        hof: false,
        collection_journal: false,
        collection_observation_gate: false,
    };
    let refs = RtRefs::new(ids, base_only, &mut func, CallConv::SystemV, types::I64);
    assert_eq!(
        func.dfg.ext_funcs.len(),
        0,
        "lazy: nothing imported up front"
    );
    assert!(refs.try_get(&mut func, Shim::CallSubrSpec).is_none());
    assert!(refs.try_get(&mut func, Shim::CbsymRead).is_none());
    assert!(refs.try_get(&mut func, Shim::TierRequest).is_none());
    assert!(refs.try_get(&mut func, Shim::DirectSlow).is_none());
    assert!(refs.try_get(&mut func, Shim::CallCensus).is_none());
    assert!(refs.try_get(&mut func, Shim::CallSpecCensus).is_none());
    assert!(refs.try_get(&mut func, Shim::DirectFramed).is_none());
    assert!(
        refs.try_get(&mut func, Shim::StringCollectionWrite)
            .is_none()
    );
    assert!(
        refs.try_get(&mut func, Shim::UnobservedCollectionOwner)
            .is_none()
    );
    let gate_signature = Shim::UnobservedCollectionOwner.signature(CallConv::SystemV, types::I64);
    assert_eq!(gate_signature.params.len(), 1);
    assert_eq!(gate_signature.params[0].value_type, types::I64);
    assert_eq!(gate_signature.returns.len(), 1);
    assert_eq!(gate_signature.returns[0].value_type, types::I8);
    let cons = refs.get(&mut func, Shim::Cons);
    assert_eq!(refs.get(&mut func, Shim::Cons), cons, "imported once");
    assert_eq!(func.dfg.ext_funcs.len(), 1);

    // Eager mode must also leave disabled shape/census imports absent.
    shim_refs::force_lazy_shims_for_test(false);
    let mut eager_func =
        Function::with_name_signature(UserFuncName::user(0, 0), Signature::new(CallConv::SystemV));
    let eager_refs = RtRefs::new(
        ids,
        base_only,
        &mut eager_func,
        CallConv::SystemV,
        types::I64,
    );
    assert!(
        eager_refs
            .try_get(&mut eager_func, Shim::DirectSlow)
            .is_none()
    );
    assert!(
        eager_refs
            .try_get(&mut eager_func, Shim::CallCensus)
            .is_none()
    );
    for shim in [
        Shim::DirectSlow,
        Shim::CallCensus,
        Shim::CallSpecCensus,
        Shim::DirectFramed,
        Shim::StringCollectionWrite,
        Shim::UnobservedCollectionOwner,
    ] {
        let id = ids.get(shim).expect("backend declares every group");
        assert!(
            eager_func
                .params
                .user_named_funcs()
                .values()
                .all(|name| name.index != id.as_u32()),
            "{shim:?}: disabled group imported eagerly"
        );
    }
    shim_refs::force_lazy_shims_for_test(true);
}

/// [`compile_corpus`] for the persistent-module tests.
pub(super) fn compile_corpus_for_test() -> Vec<String> {
    compile_corpus()
}

/// The corpus size.
pub(super) fn corpus_len_for_test() -> usize {
    corpus().len()
}

/// `(lambda (x &optional y) (+ x 1))`: a baseline leaf, kept off MIR by the
/// `&optional` pre-build gate.
fn optional_add1() -> ByteCodeFunction {
    let mut f = function(
        vec![Op::StackRef(1), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(1)],
        1,
    );
    let mut params = f.params.named().expect("named fixture parameters").clone();
    params.optional = vec![SymId(9)];
    f.params = params.into();
    f
}

/// `(lambda (x) (if x (throw 'jit-tag 1) 2))`: MIR's builder does not model
/// `throw`, the baseline does.
fn throw_if() -> ByteCodeFunction {
    function(
        vec![
            Op::StackRef(0),  // 0
            Op::GotoIfNil(5), // 1
            Op::Constant(0),  // 2: 'jit-tag
            Op::Constant(1),  // 3: 1
            Op::Throw,        // 4
            Op::Constant(2),  // 5: 2
            Op::Return,       // 6
        ],
        vec![
            Value::symbol("jit-tag"),
            Value::make_int(1),
            Value::make_int(2),
        ],
        1,
    )
}

/// Under a report knob every compile records why the MIR tier did or did not
/// take its body, on the leaf it produced (`mir=` on `[neovm-jit-final-leaf]`).
#[test]
fn jit_pipeline_leaf_records_its_mir_verdict() {
    let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
        crate::emacs_core::jit::compile::OptMode::Legacy,
    );
    force_deopt_for_test(false);
    observe_stats();
    let verdict = |f: &ByteCodeFunction| {
        let leaf = compile_bytecode_function_with(f, None).expect("compiles");
        (
            leaf.tier().name(),
            leaf.obs.mir_verdict.as_deref().map(str::to_string),
        )
    };
    assert_eq!(verdict(&add1()), ("mir", Some("taken".to_string())));
    assert_eq!(
        verdict(&optional_add1()),
        ("baseline", Some("gate_opt".to_string()))
    );
    let mut rest = optional_add1();
    let mut params = rest
        .params
        .named()
        .expect("named fixture parameters")
        .clone();
    params.rest = Some(SymId(10));
    rest.params = params.into();
    assert_eq!(
        verdict(&rest),
        ("baseline", Some("gate_rest+gate_opt".to_string())),
        "every pre-build gate that trips, in the funnel's order"
    );
    assert_eq!(
        verdict(&throw_if()),
        (
            "baseline",
            Some("build:UnsupportedOp(\"mir-unmodelled-control:Throw\")".to_string())
        )
    );
}

/// With every report knob off no verdict is recorded at all.
#[test]
fn jit_pipeline_no_mir_verdict_without_a_report_knob() {
    force_deopt_for_test(false);
    stats::force_observe_for_test(stats::ObserveOverride::default());
    for f in [add1(), optional_add1(), throw_if()] {
        let leaf = compile_bytecode_function_with(&f, None).expect("compiles");
        assert_eq!(leaf.obs.mir_verdict, None);
    }
}
