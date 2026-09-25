//! The first leaf builtins (design `p1-2-builtin-intrinsics` §2.3, commit 2):
//! each is the SAME body its reference already runs, behind a shared borrow
//! of the evaluator (see `subr::leaf` for the contract and the two shapes).
//!
//! Review checklist, per leaf (§6.5):
//! * the signature is `&Context` -- checked by the compiler;
//! * every Lisp-calling branch of the reference is a `generic_when` shape,
//!   answered with `LeafExit::Generic` before any side effect;
//! * the GNU shape matches `bytecode.c`: `Bgethash` does not exist, so
//!   `gethash`, `plist-get` and `get-char-property` are Bcall leaves; `Bget`,
//!   `Blength`, `Bnth`, `Bnthcdr`, `Belt`, `Bmemq`, `Bassq`, `Bmember`,
//!   `Bequal`, `Bstring_eqlsign` and `Bstring_lessp` are inline opcodes;
//! * containment: `Catch` everywhere except `nth`, whose fast half is the
//!   audited walk in [`fast`].
//!
//! Deferred: `symbol-value` and `buffer-local-value` (see the TODO on
//! `LeafId`), which must share P1.4 Stage A's `read_var_cached`.

// Only compiled code calls leaves: without the JIT they are declarations.
#![cfg_attr(not(feature = "jit"), allow(dead_code))]
// TEMPORARY: the JIT's trampolines, the next commit, are the first users.
#![allow(dead_code)]

use super::from_value::StringDesignator;
use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::subr::leaf::{
    BounceShape, Containment, Effects, LeafEntry, LeafExit, LeafId, LeafResult, LeafShape, LeafSpec,
};

mod fast;

/// Reads the heap and may signal: the list, sequence and equality leaves.
const READS: Effects = Effects::READ_HEAP.with(Effects::MAY_SIGNAL);

// ---------------------------------------------------------------------------
// Bcall leaves: attached to their builtin's SubrSpec.
// ---------------------------------------------------------------------------

/// `(gethash KEY TABLE &optional DEFAULT)`: `builtin_gethash_3` minus its
/// user-defined-test branch, which runs the table's Lisp hash and equality
/// functions and so bounces.
pub(crate) static GETHASH: LeafSpec = LeafSpec::new(
    LeafId::Gethash,
    "gethash",
    LeafEntry::L3(gethash),
    LeafShape::Bcall,
    READS,
    &[BounceShape::UserHashTest],
    Containment::Catch,
);

fn gethash(ctx: &Context, key: Value, table: Value, default: Value) -> LeafResult {
    if hash_table_has_user_test(table) {
        return Err(LeafExit::Generic);
    }
    Ok(builtin_gethash_values(
        key,
        table,
        default,
        ctx.symbols_with_pos_enabled,
    )?)
}

/// `(plist-get PLIST PROP &optional PREDICATE)`: `builtin_plist_get_3`'s
/// `eq` walk; a PREDICATE is called through funcall, so it bounces.
pub(crate) static PLIST_GET: LeafSpec = LeafSpec::new(
    LeafId::PlistGet,
    "plist-get",
    LeafEntry::L3(plist_get),
    LeafShape::Bcall,
    Effects::READ_HEAP,
    &[BounceShape::PlistPredicate],
    Containment::Catch,
);

fn plist_get(ctx: &Context, plist: Value, prop: Value, predicate: Value) -> LeafResult {
    if !predicate.is_nil() {
        return Err(LeafExit::Generic);
    }
    Ok(
        crate::emacs_core::plist::plist_get_swp(plist, &prop, ctx.symbols_with_pos_enabled)
            .unwrap_or(Value::NIL),
    )
}

/// `(get-char-property POSITION PROP &optional OBJECT)`: the registered body
/// already reads only the obarray, the buffers and the frames.
pub(crate) static GET_CHAR_PROPERTY: LeafSpec = LeafSpec::new(
    LeafId::GetCharProperty,
    "get-char-property",
    LeafEntry::L3(get_char_property),
    LeafShape::Bcall,
    Effects::READ_HEAP
        .with(Effects::READ_BUFFER)
        .with(Effects::READ_BINDINGS)
        .with(Effects::MAY_SIGNAL),
    &[],
    Containment::Catch,
);

fn get_char_property(ctx: &Context, position: Value, prop: Value, object: Value) -> LeafResult {
    Ok(
        crate::emacs_core::textprop::builtin_get_char_property_with_frames(
            &ctx.obarray,
            &ctx.buffers,
            Some(&ctx.frames),
            &[position, prop, object],
        )?,
    )
}

// ---------------------------------------------------------------------------
// Opcode leaves: each answers as the interpreter's opcode arm.
// ---------------------------------------------------------------------------

/// `Bget` (`Op::Get`): reads `overriding-plist-environment`, then the plist.
pub(crate) static GET: LeafSpec = LeafSpec::new(
    LeafId::Get,
    "get",
    LeafEntry::L2(get),
    LeafShape::Opcode,
    READS.with(Effects::READ_BINDINGS),
    &[],
    Containment::Catch,
);

fn get(ctx: &Context, symbol: Value, prop: Value) -> LeafResult {
    Ok(symbol_property_get(ctx, symbol, prop)?
        .1
        .unwrap_or(Value::NIL))
}

/// `Blength` (`Op::Length`).
pub(crate) static LENGTH: LeafSpec = LeafSpec::new(
    LeafId::Length,
    "length",
    LeafEntry::L1(length),
    LeafShape::Opcode,
    READS,
    &[],
    Containment::Catch,
);

fn length(_: &Context, sequence: Value) -> LeafResult {
    Ok(builtin_length_value(sequence)?)
}

/// `Bnth` (`Op::Nth`): a count of 0..127 walks inline and a non-list tail
/// signals with that TAIL (`bytecode_nth_values`), unlike `Fnth`.
pub(crate) static NTH: LeafSpec = LeafSpec::new(
    LeafId::Nth,
    "nth",
    LeafEntry::L2(nth),
    LeafShape::Opcode,
    READS,
    &[],
    Containment::FastOutside(fast::nth_fast),
);

fn nth(_: &Context, n: Value, list: Value) -> LeafResult {
    Ok(bytecode_nth_values(n, list)?)
}

/// `Bnthcdr` (`Op::Nthcdr`).
pub(crate) static NTHCDR: LeafSpec = LeafSpec::new(
    LeafId::Nthcdr,
    "nthcdr",
    LeafEntry::L2(nthcdr),
    LeafShape::Opcode,
    READS,
    &[],
    Containment::Catch,
);

fn nthcdr(_: &Context, n: Value, list: Value) -> LeafResult {
    Ok(builtin_nthcdr_values(n, list)?)
}

/// `Belt` (`Op::Elt`).
pub(crate) static ELT: LeafSpec = LeafSpec::new(
    LeafId::Elt,
    "elt",
    LeafEntry::L2(elt),
    LeafShape::Opcode,
    READS,
    &[],
    Containment::Catch,
);

fn elt(_: &Context, sequence: Value, n: Value) -> LeafResult {
    Ok(builtin_elt_values(sequence, n)?)
}

/// `Bmemq` (`Op::Memq`). Compiled `memq` sites keep their value shim
/// (`neovm_jit_memq`), which already has this body's fast half outside
/// containment; the leaf exists for the harness and later reuse.
pub(crate) static MEMQ: LeafSpec = LeafSpec::new(
    LeafId::Memq,
    "memq",
    LeafEntry::L2(memq),
    LeafShape::Opcode,
    READS,
    &[],
    Containment::Catch,
);

fn memq(ctx: &Context, elt: Value, list: Value) -> LeafResult {
    Ok(builtin_memq_values(
        elt,
        list,
        ctx.symbols_with_pos_enabled,
    )?)
}

/// `Bassq` (`Op::Assq`); see [`MEMQ`] on its compiled sites.
pub(crate) static ASSQ: LeafSpec = LeafSpec::new(
    LeafId::Assq,
    "assq",
    LeafEntry::L2(assq),
    LeafShape::Opcode,
    READS,
    &[],
    Containment::Catch,
);

fn assq(ctx: &Context, key: Value, list: Value) -> LeafResult {
    Ok(builtin_assq_values(
        key,
        list,
        ctx.symbols_with_pos_enabled,
    )?)
}

/// `Bmember` (`Op::Member`).
pub(crate) static MEMBER: LeafSpec = LeafSpec::new(
    LeafId::Member,
    "member",
    LeafEntry::L2(member),
    LeafShape::Opcode,
    READS,
    &[],
    Containment::Catch,
);

fn member(ctx: &Context, elt: Value, list: Value) -> LeafResult {
    Ok(builtin_member_values(
        elt,
        list,
        ctx.symbols_with_pos_enabled,
    )?)
}

/// `Bequal` (`Op::Equal`).
pub(crate) static EQUAL: LeafSpec = LeafSpec::new(
    LeafId::Equal,
    "equal",
    LeafEntry::L2(equal),
    LeafShape::Opcode,
    READS,
    &[],
    Containment::Catch,
);

fn equal(ctx: &Context, a: Value, b: Value) -> LeafResult {
    Ok(Value::bool_val(
        crate::emacs_core::value::try_equal_value_swp(&a, &b, 0, ctx.symbols_with_pos_enabled)?,
    ))
}

/// `Bstring_eqlsign` (`Op::StringEqual`): strings, or symbols through their
/// names (`StringDesignator`).
pub(crate) static STRING_EQUAL: LeafSpec = LeafSpec::new(
    LeafId::StringEqual,
    "string-equal",
    LeafEntry::L2(string_equal),
    LeafShape::Opcode,
    READS,
    &[],
    Containment::Catch,
);

fn string_equal(ctx: &Context, a: Value, b: Value) -> LeafResult {
    let a = StringDesignator::designate(ctx, a)?;
    let b = StringDesignator::designate(ctx, b)?;
    Ok(string_equal_designators(a.text(), b.text())?)
}

/// `Bstring_lessp` (`Op::StringLessp`).
pub(crate) static STRING_LESSP: LeafSpec = LeafSpec::new(
    LeafId::StringLessp,
    "string-lessp",
    LeafEntry::L2(string_lessp),
    LeafShape::Opcode,
    READS,
    &[],
    Containment::Catch,
);

fn string_lessp(ctx: &Context, a: Value, b: Value) -> LeafResult {
    let a = StringDesignator::designate(ctx, a)?;
    let b = StringDesignator::designate(ctx, b)?;
    Ok(Value::bool_val(string_ordering(a.text(), b.text()).is_lt()))
}

#[cfg(test)]
#[path = "tests/equivalence.rs"]
mod equivalence_tests;
