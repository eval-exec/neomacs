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
//! The variable leaves -- `symbol-value` (opcode) and `buffer-local-value`
//! (Bcall), `NEOVM_JIT_LEAF=vars` -- are their references' own bodies, which
//! read a buffer-local variable's loaded binding through the same BLV cache
//! P1.4 Stage A's tiers use; `buffer-local-value` also answers its current-
//! buffer, loaded-binding shape through `Context::read_var_cached` itself,
//! skipping the alias resolution and the buffer lookup (never a second copy
//! of the read tiers).

// Only compiled code calls leaves: without the JIT they are declarations.
#![cfg_attr(not(feature = "jit"), allow(dead_code))]

use super::from_value::StringDesignator;
use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::subr::leaf::{
    BounceShape, Containment, Effects, LeafEntry, LeafExit, LeafId, LeafResult, LeafShape, LeafSpec,
};

pub(crate) mod fast;

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

pub(crate) fn gethash(ctx: &Context, key: Value, table: Value, default: Value) -> LeafResult {
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

pub(crate) fn plist_get(ctx: &Context, plist: Value, prop: Value, predicate: Value) -> LeafResult {
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

pub(crate) fn get_char_property(
    ctx: &Context,
    position: Value,
    prop: Value,
    object: Value,
) -> LeafResult {
    Ok(
        crate::emacs_core::textprop::builtin_get_char_property_with_frames(
            &ctx.obarray,
            &ctx.buffers,
            Some(&ctx.frames),
            &[position, prop, object],
        )?,
    )
}

/// `(buffer-local-value VARIABLE BUFFER)`: the registered body
/// (`buffer_local_value_in`, a shared borrow of the evaluator), after P1.4
/// Stage A's cached read for the one shape that cache answers identically:
/// BUFFER is the current buffer and VARIABLE a buffer-local variable whose
/// binding is loaded for it (the builtin's `read_localized` early-out on
/// `where == buf`). Everything else, an unbound value included, takes the
/// body, which signals as the builtin does.
pub(crate) static BUFFER_LOCAL_VALUE: LeafSpec = LeafSpec::new(
    LeafId::BufferLocalValue,
    "buffer-local-value",
    LeafEntry::L2(buffer_local_value),
    LeafShape::Bcall,
    Effects::READ_HEAP
        .with(Effects::READ_BUFFER)
        .with(Effects::READ_BINDINGS)
        .with(Effects::MAY_SIGNAL),
    &[],
    Containment::Catch,
);

pub(crate) fn buffer_local_value(ctx: &Context, variable: Value, buffer: Value) -> LeafResult {
    if let Some(sym) = variable.as_symbol_id()
        && buffer.as_buffer_id().is_some()
        && buffer.as_buffer_id() == ctx.buffers.current_buffer_id()
        && ctx
            .obarray
            .get_by_id(sym)
            .is_some_and(|s| s.redirect() == crate::emacs_core::symbol::SymbolRedirect::Localized)
        && let Some(value) = ctx.read_var_cached(sym)
    {
        return Ok(value);
    }
    Ok(crate::emacs_core::buffer::buffer_local_value_in(
        ctx, variable, buffer,
    )?)
}

// ---------------------------------------------------------------------------
// The first leaf batch (P1.2 commit 12; `NEOVM_JIT_LEAF=batch` for compiled
// sites, `NEOVM_VM_LEAF` for the interpreter's): Bcall leaves of builtins
// the org, magit and elb-eieio call census reaches often. Each is the
// registered builtin's own body behind a shared borrow; none runs Lisp
// except `assoc` with a TESTFN, which bounces.
// ---------------------------------------------------------------------------

/// `(assoc KEY ALIST &optional TESTFN)`: `builtin_assoc_3`'s walk (a key
/// `eq` to the entry's, else `equal`); a TESTFN is called through funcall,
/// so it bounces.
pub(crate) static ASSOC: LeafSpec = LeafSpec::new(
    LeafId::Assoc,
    "assoc",
    LeafEntry::L3(assoc),
    LeafShape::Bcall,
    READS,
    &[BounceShape::AssocTestfn],
    Containment::Catch,
);

pub(crate) fn assoc(ctx: &Context, key: Value, list: Value, test_fn: Value) -> LeafResult {
    if !test_fn.is_nil() {
        return Err(LeafExit::Generic);
    }
    Ok(assoc_values(key, list, ctx.symbols_with_pos_enabled)?)
}

/// `(rassq KEY ALIST)`.
pub(crate) static RASSQ: LeafSpec = LeafSpec::new(
    LeafId::Rassq,
    "rassq",
    LeafEntry::L2(rassq),
    LeafShape::Bcall,
    READS,
    &[],
    Containment::Catch,
);

pub(crate) fn rassq(ctx: &Context, key: Value, alist: Value) -> LeafResult {
    Ok(crate::emacs_core::misc::builtin_rassq_values(
        key,
        alist,
        ctx.symbols_with_pos_enabled,
    )?)
}

/// `(delq ELT LIST)`: unlinks in place (heap writes through the barrier);
/// never bounces, so no bounce can follow a write.
pub(crate) static DELQ: LeafSpec = LeafSpec::new(
    LeafId::Delq,
    "delq",
    LeafEntry::L2(delq),
    LeafShape::Bcall,
    Effects::READ_HEAP
        .with(Effects::WRITE_HEAP)
        .with(Effects::MAY_SIGNAL),
    &[],
    Containment::Catch,
);

pub(crate) fn delq(ctx: &Context, elt: Value, list: Value) -> LeafResult {
    Ok(builtin_delq_values(
        elt,
        list,
        ctx.symbols_with_pos_enabled,
    )?)
}

/// `(copy-sequence ARG)`: allocates the copy (allocation never collects).
pub(crate) static COPY_SEQUENCE: LeafSpec = LeafSpec::new(
    LeafId::CopySequence,
    "copy-sequence",
    LeafEntry::L1(copy_sequence),
    LeafShape::Bcall,
    READS.with(Effects::ALLOCATES),
    &[],
    Containment::Catch,
);

pub(crate) fn copy_sequence(_: &Context, arg: Value) -> LeafResult {
    Ok(copy_sequence_value(arg)?)
}

/// `(symbol-name SYMBOL)`: the name string (materialized on first use).
pub(crate) static SYMBOL_NAME: LeafSpec = LeafSpec::new(
    LeafId::SymbolName,
    "symbol-name",
    LeafEntry::L1(symbol_name),
    LeafShape::Bcall,
    READS.with(Effects::ALLOCATES),
    &[],
    Containment::Catch,
);

pub(crate) fn symbol_name(ctx: &Context, symbol: Value) -> LeafResult {
    Ok(builtin_symbol_name_value(
        symbol,
        ctx.symbols_with_pos_enabled,
    )?)
}

/// `(boundp SYMBOL)`: `boundp_in`, the registered body.
pub(crate) static BOUNDP: LeafSpec = LeafSpec::new(
    LeafId::Boundp,
    "boundp",
    LeafEntry::L1(boundp),
    LeafShape::Bcall,
    READS
        .with(Effects::READ_BINDINGS)
        .with(Effects::READ_BUFFER),
    &[],
    Containment::Catch,
);

pub(crate) fn boundp(ctx: &Context, symbol: Value) -> LeafResult {
    let symbol = expect_symbol_id_checked(&symbol, ctx.symbols_with_pos_enabled)?;
    Ok(boundp_in(ctx, symbol)?)
}

/// `(keywordp OBJECT)`.
pub(crate) static KEYWORDP: LeafSpec = LeafSpec::new(
    LeafId::Keywordp,
    "keywordp",
    LeafEntry::L1(keywordp),
    LeafShape::Bcall,
    Effects::READ_HEAP,
    &[],
    Containment::Catch,
);

pub(crate) fn keywordp(ctx: &Context, object: Value) -> LeafResult {
    Ok(Value::bool_val(keywordp_swp(
        object,
        ctx.symbols_with_pos_enabled,
    )))
}

// ---------------------------------------------------------------------------
// Opcode leaves: each answers as the interpreter's opcode arm.
// ---------------------------------------------------------------------------

/// `Bsymbol_value` (`Op::SymbolValue`): `builtin_symbol_value_1`'s body --
/// the argument checked as a symbol (a symbol with position when
/// `symbols-with-pos-enabled`), its value through
/// `visible_runtime_variable_value_by_id`, `void-variable` with the
/// argument as given. That body already reads a buffer-local variable's
/// loaded binding through the BLV cache P1.4 Stage A's `read_var_cached`
/// consults, so the leaf does not call `read_var_cached` first: measured
/// on the p12x probes, the extra obarray probe cost 12-13 instructions a
/// call on plain and buffer-local variables alike and saved nothing.
pub(crate) static SYMBOL_VALUE: LeafSpec = LeafSpec::new(
    LeafId::SymbolValue,
    "symbol-value",
    LeafEntry::L1(symbol_value),
    LeafShape::Opcode,
    Effects::READ_HEAP
        .with(Effects::READ_BUFFER)
        .with(Effects::READ_BINDINGS)
        .with(Effects::MAY_SIGNAL),
    &[],
    Containment::Catch,
);

pub(crate) fn symbol_value(ctx: &Context, symbol_value: Value) -> LeafResult {
    let symbol = expect_symbol_id_checked(&symbol_value, ctx.symbols_with_pos_enabled)?;
    match ctx.visible_runtime_variable_value_by_id(symbol)? {
        Some(value) => Ok(value),
        None => Err(signal(LispCondition::VoidVariable, vec![symbol_value]).into()),
    }
}

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

pub(crate) fn get(ctx: &Context, symbol: Value, prop: Value) -> LeafResult {
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

pub(crate) fn length(_: &Context, sequence: Value) -> LeafResult {
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

pub(crate) fn nth(_: &Context, n: Value, list: Value) -> LeafResult {
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

pub(crate) fn nthcdr(_: &Context, n: Value, list: Value) -> LeafResult {
    Ok(builtin_nthcdr_values(n, list)?)
}

/// `Belt` (`Op::Elt`): a short list walk signals with the tail, like `Bnth`.
pub(crate) static ELT: LeafSpec = LeafSpec::new(
    LeafId::Elt,
    "elt",
    LeafEntry::L2(elt),
    LeafShape::Opcode,
    READS,
    &[],
    Containment::Catch,
);

pub(crate) fn elt(_: &Context, sequence: Value, n: Value) -> LeafResult {
    Ok(bytecode_elt_values(sequence, n)?)
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

pub(crate) fn memq(ctx: &Context, elt: Value, list: Value) -> LeafResult {
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

pub(crate) fn assq(ctx: &Context, key: Value, list: Value) -> LeafResult {
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

pub(crate) fn member(ctx: &Context, elt: Value, list: Value) -> LeafResult {
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

pub(crate) fn equal(ctx: &Context, a: Value, b: Value) -> LeafResult {
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

pub(crate) fn string_equal(ctx: &Context, a: Value, b: Value) -> LeafResult {
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

pub(crate) fn string_lessp(ctx: &Context, a: Value, b: Value) -> LeafResult {
    let a = StringDesignator::designate(ctx, a)?;
    let b = StringDesignator::designate(ctx, b)?;
    Ok(Value::bool_val(string_ordering(a.text(), b.text()).is_lt()))
}

#[cfg(test)]
#[path = "tests/equivalence_test.rs"]
mod equivalence_tests;
