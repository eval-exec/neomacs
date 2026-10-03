//! Compiler-local native numeric carrier for verified selected sink recipes.
//! This child emits compiler-local CLIF temporaries. It owns no Lisp identity,
//! object cache, IR projection, phi, root, snapshot or runtime/TLS state. The
//! selected sink adapter and Func verifier must establish tuple provenance and
//! publish any returned cache update by semantic SSA identity.

use super::*;
use cranelift_codegen::ir::immediates::Ieee64;

/// Independently derived emission facts, never runtime/ABI fields. Threading:
/// copied scalar metadata owned by one compiler invocation and exact recipe
/// version. Unknown tuple/phi facts retain the general lowering path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CarrierFacts {
    pub(super) ready: NumericReady,
    pub(super) kind: NumericKind,
    pub(super) cache: NumericCache,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum NumericReady {
    #[default]
    Unknown,
    Borrowed,
    Ready,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum NumericKind {
    #[default]
    FloatOrFixnum,
    Float,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum NumericCache {
    #[default]
    Unknown,
    /// Fresh source: NIL for Float, exact word for Fix. Never merely NIL.
    Fresh,
    Boxed,
}

impl NumericKind {
    fn flonum(self) -> FlonumKind {
        match self {
            Self::Float => FlonumKind::Float,
            Self::FloatOrFixnum => FlonumKind::FloatOrFixnum,
        }
    }
}

/// The four independently transported fields of one verified numeric recipe.
/// Threading: these are SSA names owned by one compiler invocation, never
/// shared mutable state. At execution, only `real_box` is a possible Lisp root.
/// `word` must never enter a root window, tagged spill, call or return.
///
/// `ready == 0`: payload is positive zero and ignored; word and real_box hold
/// the exact borrowed Lisp value, which need not yet be a number. No producer
/// has dereferenced it. `ready == 1`: a Float payload is its exact F64 view; a FIX payload may be
/// dummy and numeric F64 observations derive it from the exact word. Word is either
/// UNBOXED_FLOAT_TAG_WORD or an exact tagged fixnum. A float's real_box is NIL
/// until materialized, or its exact original/one materialized object. A fixnum
/// has its exact tagged word in real_box. Ready is normalized I8 zero/one.
#[derive(Clone, Copy, Debug)]
pub(super) struct NumericCarrier {
    pub(super) payload: ClifValue,
    pub(super) word: ClifValue,
    pub(super) ready: ClifValue,
    pub(super) real_box: ClifValue,
    pub(super) facts: CarrierFacts,
}

/// A materialization result and its explicit SSA cache update. Threading: both
/// are compiler-local values. The caller must publish `carrier` to every alias
/// of this same semantic identity on the materialization's successor path.
/// Reusing an older tuple would allocate a second box; equal F64 payloads or
/// equal CLIF payload names are never a cache key.
#[derive(Clone, Copy, Debug)]
pub(super) struct MaterializedNumeric {
    pub(super) tagged: ClifValue,
    pub(super) carrier: NumericCarrier,
}

/// Physical shape only. Semantic ready/word/cache provenance is independently
/// verified by the owning IR; testing machine types cannot establish it.
fn check_carrier(fb: &FunctionBuilder, carrier: NumericCarrier) -> Result<(), CompileError> {
    let dfg = &fb.func.dfg;
    if dfg.value_type(carrier.payload) != types::F64
        || dfg.value_type(carrier.word) != types::I64
        || dfg.value_type(carrier.ready) != types::I8
        || dfg.value_type(carrier.real_box) != types::I64
    {
        return Err(CompileError::UnsupportedOp(
            "opt-sink:numeric-carrier-types",
        ));
    }
    Ok(())
}

/// Borrow without a tag test, pointer load, allocation or numeric coercion.
/// A zero-trip loop can therefore return this exact object, including an
/// initial heap Float or a nonnumeric value which was never operated on.
pub(super) fn borrow_number(
    fb: &mut FunctionBuilder,
    exact_tagged: ClifValue,
) -> Result<NumericCarrier, CompileError> {
    if fb.func.dfg.value_type(exact_tagged) != types::I64 {
        return Err(CompileError::UnsupportedOp("opt-sink:borrow-number-type"));
    }
    Ok(NumericCarrier {
        payload: fb.ins().f64const(Ieee64::with_bits(0.0_f64.to_bits())),
        word: exact_tagged,
        ready: fb.ins().iconst(types::I8, 0),
        real_box: exact_tagged,
        facts: CarrierFacts {
            ready: NumericReady::Borrowed,
            cache: NumericCache::Boxed,
            ..CarrierFacts::default()
        },
    })
}

/// A fresh Float-producing operation defines this tuple on EACH execution,
/// resetting its cache to NIL. The caller owns its distinct GNU allocation
/// identity; this constructor does not reuse any earlier equal payload.
pub(super) fn fresh_float(
    fb: &mut FunctionBuilder,
    payload: ClifValue,
) -> Result<NumericCarrier, CompileError> {
    if fb.func.dfg.value_type(payload) != types::F64 {
        return Err(CompileError::UnsupportedOp("opt-sink:fresh-float-type"));
    }
    Ok(NumericCarrier {
        payload,
        word: fb.ins().iconst(types::I64, UNBOXED_FLOAT_TAG_WORD),
        ready: fb.ins().iconst(types::I8, 1),
        real_box: fb.ins().iconst(types::I64, Value::NIL.bits() as i64),
        facts: CarrierFacts {
            ready: NumericReady::Ready,
            kind: NumericKind::Float,
            cache: NumericCache::Fresh,
        },
    })
}

/// Resolve one operand at the original numeric operation, not at BorrowNumber
/// or a loop preheader. The passed exit must already recover the complete
/// original pre-operation frame, including this tuple's original identity.
///
/// Ready operands use their normalized word/payload without touching a heap
/// object. Borrowed operands use the exact real_box and the unchanged shared
/// tag guards and float load. Marker, bignum and wrong type go to original
/// Tier-0 replay; this child never performs a coercion or raises a signal.
/// No call, Poll, GC safepoint or externally visible effect occurs here.
pub(super) fn resolve_operand(
    fb: &mut FunctionBuilder,
    original_deopt: Block,
    carrier: NumericCarrier,
) -> Result<NumericCarrier, CompileError> {
    check_carrier(fb, carrier)?;
    match carrier.facts.ready {
        NumericReady::Ready => return Ok(carrier),
        NumericReady::Borrowed => return resolve_borrowed(fb, original_deopt, carrier),
        NumericReady::Unknown => {}
    }
    let word_out = fb.declare_var(types::I64);
    let payload_out = fb.declare_var(types::F64);
    let ready_block = fb.create_block();
    let borrowed_block = fb.create_block();
    let merge = fb.create_block();
    fb.ins()
        .brif(carrier.ready, ready_block, &[], borrowed_block, &[]);

    fb.switch_to_block(ready_block);
    fb.seal_block(ready_block);
    fb.def_var(word_out, carrier.word);
    fb.def_var(payload_out, carrier.payload);
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(borrowed_block);
    fb.seal_block(borrowed_block);
    let borrowed = resolve_borrowed(fb, original_deopt, carrier)?;
    fb.def_var(word_out, borrowed.word);
    fb.def_var(payload_out, borrowed.payload);
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(merge);
    fb.seal_block(merge);
    Ok(NumericCarrier {
        payload: fb.use_var(payload_out),
        word: fb.use_var(word_out),
        ready: fb.ins().iconst(types::I8, 1),
        // Keep the original borrowed object, or an already cached box. The
        // caller may publish this resolved view without losing identity.
        real_box: carrier.real_box,
        facts: CarrierFacts {
            ready: NumericReady::Ready,
            ..carrier.facts
        },
    })
}

/// Execute the original borrowed guards even for a known immutable Float
/// constant. The compiler fact removes only a readiness diamond; it never
/// permits an eager seed load or a feedback-based guard omission.
fn resolve_borrowed(
    fb: &mut FunctionBuilder,
    original_deopt: Block,
    carrier: NumericCarrier,
) -> Result<NumericCarrier, CompileError> {
    let payload = lowering::stack_as_f64_or_promote(
        fb,
        original_deopt,
        &[carrier.real_box],
        &[SlotRep::Tagged],
        0,
    );
    let is_fix = lowering::fixnum_tag_test(fb, carrier.real_box);
    let sentinel = fb.ins().iconst(types::I64, UNBOXED_FLOAT_TAG_WORD);
    let word = fb.ins().select(is_fix, carrier.real_box, sentinel);
    Ok(NumericCarrier {
        payload,
        word,
        ready: fb.ins().iconst(types::I8, 1),
        real_box: carrier.real_box,
        facts: CarrierFacts {
            ready: NumericReady::Ready,
            kind: carrier.facts.kind,
            cache: NumericCache::Boxed,
        },
    })
}

/// The F64 operand view of an ALREADY resolved carrier. Ready FIX sources may
/// carry a dummy payload: derive their exact signed integer from the tagged
/// word and promote it, never use that dummy as a number. A Float marker uses
/// its actual payload. This emits no heap load or allocation; the caller must
/// first resolve BorrowNumber under the exact original operation exit.
pub(super) fn ready_as_f64(
    fb: &mut FunctionBuilder,
    carrier: NumericCarrier,
) -> Result<ClifValue, CompileError> {
    check_carrier(fb, carrier)?;
    if carrier.facts.kind == NumericKind::Float {
        return Ok(carrier.payload);
    }
    let is_fix = lowering::fixnum_tag_test(fb, carrier.word);
    let raw = lowering::sshr_imm_p(fb, carrier.word, FIXNUM_SHIFT as i64);
    let promoted = fb.ins().fcvt_from_sint(types::F64, raw);
    Ok(fb.ins().select(is_fix, promoted, carrier.payload))
}

/// The four supported binary source operations use the unchanged P0.9
/// dispatcher, including its full fix/fix arm. Threading: operand vectors and
/// tuple fields are compiler-local; existing compiler-only census is reused.
///
/// BOTH integer inputs produce an integer result, including truncated `/`.
/// Integer zero division, fixnum overflow and min/-1 replay the original
/// operation. A float operand uses the existing double promotion/IEEE path,
/// including negative zero and invalid-operation NaN selection. No reassoc,
/// contraction, constant folding or payload-keyed boxing is introduced.
pub(super) fn emit_binary(
    fb: &mut FunctionBuilder,
    op: &Op,
    original_deopt: Block,
    left: NumericCarrier,
    right: NumericCarrier,
) -> Result<NumericCarrier, CompileError> {
    // The shared dispatcher's catch-all is integer division. Admit only its
    // exact supported family; Rem, unary arithmetic and generic arities must
    // remain their original source operations.
    if !matches!(op, Op::Add | Op::Sub | Op::Mul | Op::Div) {
        return Err(CompileError::UnsupportedOp("opt-sink:numeric-source-op"));
    }
    check_carrier(fb, left)?;
    check_carrier(fb, right)?;
    // Its mixed arm resolves right before left. A failed guard raises nothing
    // and replays the exact original full frame, so GNU argument/error order
    // remains Tier-0's. There is no effect between these resolutions.
    let right = resolve_operand(fb, original_deopt, right)?;
    let left = resolve_operand(fb, original_deopt, left)?;
    let left_payload = ready_as_f64(fb, left)?;
    let right_payload = ready_as_f64(fb, right)?;
    // The precise exit and original right-before-left guards already exist.
    // Both tuples are now normalized; actual Float contagion means the old
    // ff/mix arms compute the same shared operation on these prepared views.
    // Kind still comes only from independently checked exact recipe versions,
    // never feedback/declarations or an inferred unknown/CachePhi seed.
    if left.facts.kind == NumericKind::Float || right.facts.kind == NumericKind::Float {
        let payload = lowering::emit_ready_float_result(fb, op, left_payload, right_payload);
        return fresh_float(fb, payload);
    }
    let mut stack = vec![left.word, right.word];
    let mut reps = vec![
        SlotRep::Flonum {
            f64: left_payload,
            kind: left.facts.kind.flonum(),
        },
        SlotRep::Flonum {
            f64: right_payload,
            kind: right.facts.kind.flonum(),
        },
    ];
    lowering::lower_float_site_arith(fb, op, original_deopt, &mut stack, &mut reps);
    let [word] = stack.as_slice() else {
        return Err(CompileError::UnsupportedOp("opt-sink:numeric-result-stack"));
    };
    let [SlotRep::Flonum { f64: payload, kind }] = reps.as_slice() else {
        return Err(CompileError::UnsupportedOp("opt-sink:numeric-result-rep"));
    };
    let word = *word;
    let payload = *payload;
    let kind = match kind {
        FlonumKind::Float => NumericKind::Float,
        FlonumKind::FloatOrFixnum => NumericKind::FloatOrFixnum,
    };
    let real_box = if kind == NumericKind::Float {
        fb.ins().iconst(types::I64, Value::NIL.bits() as i64)
    } else {
        let is_float = lowering::icmp_imm_p(fb, IntCC::Equal, word, UNBOXED_FLOAT_TAG_WORD);
        let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
        fb.ins().select(is_float, nil, word)
    };
    Ok(NumericCarrier {
        payload,
        word,
        ready: fb.ins().iconst(types::I8, 1),
        // New result: no input Float object is ever reused. A fixnum's exact
        // word is already its Lisp value; Float gets a fresh empty box cache.
        real_box,
        facts: CarrierFacts {
            ready: NumericReady::Ready,
            kind,
            cache: NumericCache::Fresh,
        },
    })
}

/// True only when the independently validated CURRENT tuple facts establish
/// that neither input can require a borrowed tag guard and Float contagion
/// excludes the shared dispatcher's checked both-fix arm. Declaration types,
/// feedback, cached boxes and unknown/CachePhi readiness do not prove this.
pub(super) fn binary_has_no_error_path(left: NumericCarrier, right: NumericCarrier) -> bool {
    left.facts.ready == NumericReady::Ready
        && right.facts.ready == NumericReady::Ready
        && (left.facts.kind == NumericKind::Float || right.facts.kind == NumericKind::Float)
}

/// Emit the infallible successful numeric arm only after exact-version proof.
/// The same shared Float arithmetic retains signed-zero and division NaN
/// policy. This accepts no dummy deopt block and creates no cold snapshot.
/// Source guards on Borrowed/Unknown and all checked both-fix operations still
/// use emit_binary with their complete original precise exit.
pub(super) fn emit_binary_ready_float(
    fb: &mut FunctionBuilder,
    op: &Op,
    left: NumericCarrier,
    right: NumericCarrier,
) -> Result<NumericCarrier, CompileError> {
    if !matches!(op, Op::Add | Op::Sub | Op::Mul | Op::Div)
        || !binary_has_no_error_path(left, right)
    {
        return Err(CompileError::UnsupportedOp(
            "opt-sink:not-proven-ready-float",
        ));
    }
    check_carrier(fb, left)?;
    check_carrier(fb, right)?;
    let left_payload = ready_as_f64(fb, left)?;
    let right_payload = ready_as_f64(fb, right)?;
    let payload = lowering::emit_ready_float_result(fb, op, left_payload, right_payload);
    fresh_float(fb, payload)
}

/// Return the current tuple's actual Lisp value, with a successor cache view.
/// Borrowed values return the exact object without a numeric guard. Ready
/// values reuse their real box, or allocate exactly once through the existing
/// no-GC Float boxer. Fixnum results never allocate. `cold` changes only block
/// placement/boxing choice using the shared boxer's existing cold policy.
///
/// The root adapter MUST publish the returned cache by semantic identity on
/// this path before any later alias materialization. Rooting/snapshot code
/// must retain these exact tuple fields at its own point, never consult a
/// mutable "latest tuple" after a later producer or escape has executed.
pub(super) fn materialize(
    fb: &mut FunctionBuilder,
    refs: &RtRefs,
    inline_rt: Option<&RtCtx>,
    carrier: NumericCarrier,
    cold: bool,
) -> Result<MaterializedNumeric, CompileError> {
    check_carrier(fb, carrier)?;
    if carrier.facts.cache == NumericCache::Boxed || carrier.facts.ready == NumericReady::Borrowed {
        return Ok(materialized(carrier.real_box, carrier));
    }
    if carrier.facts.ready == NumericReady::Ready && carrier.facts.cache == NumericCache::Fresh {
        // Fresh mixed sources may already be fixnums. The unchanged word-aware
        // boxer returns their exact word and boxes only the Float arm.
        let tagged = lowering::box_flonum_value(
            fb,
            refs,
            inline_rt,
            carrier.word,
            carrier.payload,
            carrier.facts.kind.flonum(),
            cold,
        );
        return Ok(materialized(tagged, carrier));
    }
    let tagged_out = fb.declare_var(types::I64);
    let borrowed_block = fb.create_block();
    let ready_block = fb.create_block();
    let cached_block = fb.create_block();
    let box_block = fb.create_block();
    let merge = fb.create_block();
    if cold {
        for block in [borrowed_block, ready_block, cached_block, box_block, merge] {
            fb.set_cold_block(block);
        }
    }
    fb.ins()
        .brif(carrier.ready, ready_block, &[], borrowed_block, &[]);

    fb.switch_to_block(borrowed_block);
    fb.seal_block(borrowed_block);
    fb.def_var(tagged_out, carrier.real_box);
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(ready_block);
    fb.seal_block(ready_block);
    let has_box = lowering::icmp_imm_p(
        fb,
        IntCC::NotEqual,
        carrier.real_box,
        Value::NIL.bits() as i64,
    );
    fb.ins().brif(has_box, cached_block, &[], box_block, &[]);

    fb.switch_to_block(cached_block);
    fb.seal_block(cached_block);
    fb.def_var(tagged_out, carrier.real_box);
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(box_block);
    fb.seal_block(box_block);
    let tagged = lowering::box_flonum_value(
        fb,
        refs,
        inline_rt,
        carrier.word,
        carrier.payload,
        carrier.facts.kind.flonum(),
        cold,
    );
    fb.def_var(tagged_out, tagged);
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(merge);
    fb.seal_block(merge);
    let tagged = fb.use_var(tagged_out);
    Ok(materialized(tagged, carrier))
}

fn materialized(tagged: ClifValue, carrier: NumericCarrier) -> MaterializedNumeric {
    MaterializedNumeric {
        tagged,
        carrier: NumericCarrier {
            real_box: tagged,
            facts: CarrierFacts {
                cache: NumericCache::Boxed,
                ..carrier.facts
            },
            ..carrier
        },
    }
}
