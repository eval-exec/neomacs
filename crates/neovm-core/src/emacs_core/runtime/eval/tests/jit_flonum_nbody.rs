#![cfg(feature = "jit")]
//! Unboxed float slots on the body they were built for: GNU 31.1's own
//! byte-compile of elisp-benchmarks' `elb-applyforces` (nbody), decoded
//! through the same `decode_gnu_bytecode` a loaded `.elc` takes.
//!
//! One straight-line block with 26 `Float`-feedback arithmetic sites, 12
//! `aref`, 6 `aset` and one `call 1` (`sqrt`). GNU allocates one float per
//! result: 26 + `sqrt`'s = 27 per call, which is what `NEOVM_JIT_FLONUM=off`
//! still does. `local` keeps results unboxed between float ops (15 per call:
//! dx, dy, dz and the sum at the `sqrt` call, mag and the three products at
//! the first `aref`, and the 6 stored values, plus `sqrt`'s); `resident`
//! keeps them across the call and the `aref`s too (8: the `sqrt` argument,
//! the 6 stored values and `sqrt`'s result).
//!
//! The JIT copy of the bodies must stay bit-identical to an interpreter copy
//! for 1,000 steps.

use crate::emacs_core::bytecode::decode::{decode_gnu_bytecode, parse_arglist_descriptor};
use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::compile::{FlonumMode, force_flonum_mode_for_test, lowering};
use crate::emacs_core::value::{Value, ValueKind};
use crate::tagged::gc::MemoryUseCountSlot;

/// `(aref (symbol-function 'elb-applyforces) 0..3)` from GNU 31.1:
/// arglist 771 (three required), 147 bytes, max depth 18.
fn applyforces() -> ByteCodeFunction {
    let bytes: [u8; 147] = [
        2, 192, 72, 2, 192, 72, 90, 3, 193, 72, 3, 193, 72, 90, 4, 194, 72, 4, 194, 72, 90, 195, 3,
        137, 95, 3, 137, 95, 92, 2, 137, 95, 92, 33, 4, 1, 137, 95, 2, 95, 165, 4, 1, 95, 4, 2, 95,
        4, 3, 95, 6, 10, 196, 6, 12, 196, 72, 5, 6, 13, 197, 72, 95, 90, 73, 136, 6, 10, 198, 6,
        12, 198, 72, 4, 6, 13, 197, 72, 95, 90, 73, 136, 6, 10, 199, 6, 12, 199, 72, 3, 6, 13, 197,
        72, 95, 90, 73, 136, 6, 9, 196, 6, 11, 196, 72, 5, 6, 14, 197, 72, 95, 92, 73, 136, 6, 9,
        198, 6, 11, 198, 72, 4, 6, 14, 197, 72, 95, 92, 73, 136, 6, 9, 199, 6, 11, 199, 72, 3, 6,
        14, 197, 72, 95, 92, 73, 200, 135,
    ];
    let mut constants = vec![
        Value::make_int(0),
        Value::make_int(1),
        Value::make_int(2),
        Value::symbol("sqrt"),
        Value::make_int(3),
        Value::make_int(6),
        Value::make_int(4),
        Value::make_int(5),
        Value::NIL,
    ];
    let ops = decode_gnu_bytecode(&bytes, &mut constants).expect("GNU bytecode decodes");
    let mut f = ByteCodeFunction::new(parse_arglist_descriptor(771));
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 18;
    f
}

/// nbody's five bodies, `[x y z vx vy vz mass]` (`make-elb-body`).
fn system() -> Vec<Value> {
    let days = 365.24_f64;
    let solar = 4.0 * std::f64::consts::PI * std::f64::consts::PI;
    let body = |x: f64, y: f64, z: f64, vx: f64, vy: f64, vz: f64, m: f64| {
        Value::vector(
            [x, y, z, vx * days, vy * days, vz * days, m * solar]
                .into_iter()
                .map(Value::make_float)
                .collect(),
        )
    };
    vec![
        Value::vector(
            [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, solar]
                .into_iter()
                .map(Value::make_float)
                .collect(),
        ),
        body(
            4.841_431_442_464_721,
            -1.160_320_044_027_428_4,
            -1.036_220_444_711_231_1e-1,
            1.660_076_642_744_037e-3,
            7.699_011_184_197_404e-3,
            -6.904_600_169_720_63e-5,
            9.547_919_384_243_266e-4,
        ),
        body(
            8.343_366_718_244_58,
            4.124_798_564_124_305,
            -4.035_234_171_143_214e-1,
            -2.767_425_107_268_624e-3,
            4.998_528_012_349_172e-3,
            2.304_172_975_737_639_3e-5,
            2.858_859_806_661_308e-4,
        ),
        body(
            1.289_436_956_213_913_1e1,
            -1.511_115_140_169_863_1e1,
            -2.233_075_788_926_557_3e-1,
            2.964_601_375_647_616e-3,
            2.378_471_739_594_809_5e-3,
            -2.965_895_685_402_375_6e-5,
            4.366_244_043_351_563e-5,
        ),
        body(
            1.537_969_711_485_091_6e1,
            -2.591_931_460_998_796_4e1,
            1.792_587_729_503_711_8e-1,
            2.680_677_724_903_893_2e-3,
            1.628_241_700_382_423e-3,
            -9.515_922_545_197_159e-5,
            5.151_389_020_466_114_5e-5,
        ),
    ]
}

fn slot(body: Value, i: usize) -> f64 {
    let v = body.as_vector_data().expect("a body vector")[i];
    assert!(matches!(v.kind(), ValueKind::Float), "slot {i}: {v:?}");
    v.xfloat()
}

/// `elb-advance`'s position update, `(cl-incf (x b) (* dt (vx b)))` for
/// x, y and z, done identically for both copies.
fn advance_positions(system: &[Value], dt: f64) {
    for &b in system {
        for axis in 0..3 {
            let moved = slot(b, axis) + dt * slot(b, axis + 3);
            assert!(b.set_vector_slot(axis, Value::make_float(moved)));
        }
    }
}

fn floats_consed(ev: &Context) -> u64 {
    ev.tagged_heap.memory_use_counts_snapshot()[MemoryUseCountSlot::Floats.index()]
}

/// Run 1,000 nbody steps with a JIT and an interpreter copy of the bodies
/// under `mode`; return the float allocations of one compiled call and
/// the census of the compile.
fn run_nbody(mode: FlonumMode, probe_name: &str) -> (u64, lowering::FlonumCensus) {
    force_flonum_mode_for_test(Some(mode));
    let mut ev = Context::new();
    let jit_fn = Value::make_bytecode(applyforces());
    let interp_fn = applyforces();
    // Bind the JIT copy to a symbol: that roots it for the whole test.
    let crate::emacs_core::value::ValueKind::Symbol(id) = Value::symbol(probe_name).kind() else {
        panic!("symbol")
    };
    ev.obarray.set_symbol_function_id(id, jit_fn);
    let jit_system = system();
    let interp_system = system();
    let dt = Value::make_float(0.01);
    // A Rust `Vec<Value>` is invisible to the collector, and 10,000 calls
    // collect: keep the bodies and `dt` in a global variable's vector.
    let roots: Vec<Value> = jit_system
        .iter()
        .chain(&interp_system)
        .copied()
        .chain([dt])
        .collect();
    ev.obarray
        .set_symbol_value(&format!("{probe_name}-roots"), Value::vector(roots));
    let bc = jit_fn.get_bytecode_data().expect("bytecode");
    let mut per_call = None;
    let mut census = None;
    for step in 0..1_000 {
        if step == 3 {
            // Warmed on the interpreter: every site has recorded Float.
            bc.jit_runtime().set_hot_for_test();
        }
        for i in 0..jit_system.len() {
            for j in i + 1..jit_system.len() {
                let before = floats_consed(&ev);
                ev.funcall_general_untraced(jit_fn, vec![jit_system[i], jit_system[j], dt])
                    .expect("JIT copy");
                let consed = floats_consed(&ev) - before;
                if step == 3 && census.is_none() {
                    census = Some(lowering::flonum_census());
                } else if step > 3 {
                    assert_eq!(*per_call.get_or_insert(consed), consed, "steady per call");
                }
                Vm::from_context(&mut ev)
                    .execute(&interp_fn, vec![interp_system[i], interp_system[j], dt])
                    .expect("interpreter copy");
            }
        }
        advance_positions(&jit_system, 0.01);
        advance_positions(&interp_system, 0.01);
    }
    force_flonum_mode_for_test(None);
    assert!(
        bc.jit_runtime().compiled_id().is_some(),
        "elb-applyforces must have been compiled, or this test is vacuous"
    );
    assert!(
        interp_fn.jit_runtime().compiled_id().is_none(),
        "the reference copy must stay interpreted"
    );
    for (b, (&jit, &interp)) in jit_system.iter().zip(&interp_system).enumerate() {
        for i in 0..7 {
            assert_eq!(
                slot(jit, i).to_bits(),
                slot(interp, i).to_bits(),
                "body {b} slot {i}: JIT {} vs interpreter {}",
                slot(jit, i),
                slot(interp, i)
            );
        }
    }
    (
        per_call.expect("compiled calls ran"),
        census.expect("the tier-up compile ran"),
    )
}

#[test]
fn nbody_applyforces_unboxed_locally_allocates_15_floats_per_call() {
    let (per_call, census) = run_nbody(FlonumMode::OpLocal, "flonum-nbody-local");
    assert_eq!(
        census.results, 26,
        "every Float site leaves its result unboxed"
    );
    assert_eq!(census.escape_boxes, 14);
    assert_eq!(per_call, 15, "GNU allocates 27");
}

#[test]
fn nbody_applyforces_boxed_at_every_site_allocates_27_floats_per_call() {
    let (per_call, census) = run_nbody(FlonumMode::Off, "flonum-nbody-off");
    assert_eq!(census, lowering::FlonumCensus::default());
    assert_eq!(per_call, 27, "one per result plus sqrt's, as GNU");
}
