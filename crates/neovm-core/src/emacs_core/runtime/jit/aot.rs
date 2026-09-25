//! Ahead-of-time (AOT) object emission for compiled leaves (Phase R1c).
//!
//! AOT is a **fourth code producer**, never a deopt target: it emits the *same*
//! CLIF the JIT does (via the R1b `compile::build_mir_leaf_fn::<M: Module>` seam)
//! but routes it through Cranelift's [`ObjectModule`] instead of `JITModule`, so
//! the result is a relocatable `.o` rather than executable memory. The `.o` is
//! later linked into a `.so` (`cc -shared`) and `dlopen`'d, its entry inserted
//! into the per-thread `COMPILED` cache as a pre-warmed [`CompiledLeaf`]
//! (R1c-4..6). Tier-0 `bytecode::Vm` stays the sole oracle + `DeoptAt` landing
//! pad; the GC is concurrent non-moving, so AOT's only GC duty is liveness +
//! SATB-correct root publication (R1c-8) — no fixup/stackmaps.
//!
//! ## The three JIT seams AOT replaces (cf. `build_mir_leaf_fn`'s doc)
//!   * `builder.symbol(...)`    — JIT bakes shim host addresses; AOT leaves the
//!     `neovm_jit_*` shims as **undefined `Linkage::Import`s** (declared by
//!     `declare_rt_refs`), resolved by the dynamic loader against the host
//!     process at `dlopen` (host links `-rdynamic`; R1c-5).
//!   * `finalize_definitions()` — replaced by `ObjectModule::finish().emit()`.
//!   * `get_finalized_function` — replaced by a `dlsym` of the exported entry.
//!
//! Only built with the `jit` feature (links Cranelift).

use cranelift_codegen::settings::{self, Configurable};
use cranelift_module::{Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};

use super::backend::BackendError;
use super::compile::{CompileError, DeoptCells};
use super::mir;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::value::Value;

/// Wrap a stringly backend error as a `CompileError` (module-init flavor — the
/// ObjectModule setup + emit are the "module" steps here).
fn module_init_err(msg: String) -> CompileError {
    CompileError::Backend(BackendError::ModuleInit(msg))
}

// ---------------------------------------------------------------------------
// R1c-2: content hash + ABI tag + entry symbol.
//
// A loaded `.so` is only safe to run if (a) it was emitted from the SAME source
// (content hash) and (b) the host's ABI matches what the code assumes (ABI tag).
// The loader (R1c-5) refuses a hash/tag mismatch and falls back to the JIT.
// ---------------------------------------------------------------------------

/// ABI compatibility tag for AOT artifacts — a `u32` fingerprint of every
/// structural assumption the emitted code + loader share. Any change here MUST
/// bump `ABI_TAG_VERSION` so a stale `.so` (with a different tag) is refused.
///
/// Encodes the `STATUS_*` return codes; the entry ABI shape (the unified
/// 4-param entry `fn(vmctx, args, out, sidecar) -> i64`); the reloc-base,
/// `DeoptCells`, and `LeafSidecar` layouts; and the `neovm_jit_*` shim name set
/// (the imports the loader binds). It is a `u32` FNV-1a hash
/// ([`compute_abi_tag`]), embedded in the entry symbol as `{ABI_TAG:08x}`; the
/// DISTINCT per-leaf CONTENT hash ([`leaf_content_hash`]) is the `u128`. No
/// epoch is ever encoded (epochs are re-derived from the live obarray at load —
/// see the spec's cross-session invariants).
pub(crate) const ABI_TAG: u32 = compute_abi_tag();

/// Bump on ANY change to the entry ABI, `STATUS_*` codes, `DeoptCells`/reloc-base
/// layout, or the shim set — it salts [`ABI_TAG`] so old artifacts stop matching.
/// v2: #32-audit fix — the entry-ABI-shape code in [`compute_abi_tag`] now
/// reflects the real 4-param ABI (was a stale 3-param code) and salts the
/// `LeafSidecar` size; the tag value changes accordingly (harmless — no on-disk
/// `.so` artifacts predate this).
/// v3: R2 increment A (CBSym-in-AOT) — AOT baseline leaves now import the two
/// `neovm_jit_cbsym_*` shims (auto-salted via the grown `MIR_SHIM_NAMES`) and bake
/// the Tier-A `CBSYM_A_*` `which` discriminants as iconsts, so the discriminant
/// set is salted explicitly (see [`compute_abi_tag`]) — a renumber/swap re-tags.
/// v4: R2 increment B2 (Op::Call spec-in-AOT) — AOT baseline leaves now bake
/// `Op::Call` subr/bytecode speculation sites (per-site `SpecSlot`/expected loaded
/// from the sidecar, armed by the loader against the live obarray), import the
/// three round-1 spec shims (auto-salted via `MIR_SHIM_NAMES`), and carry a
/// descriptor spec-section. The [`SPEC_ENCODING_VERSION`], `SpecSlot` size, and
/// `SpecCalleeKind::DISC_COUNT` are salted (see [`compute_abi_tag`]).
/// v5: `Op::Aset` now calls `neovm_jit_named_builtin` with variant 3 (the
/// root-free fast path, which bounces a redefined `aset` with
/// STATUS_NEED_GENERIC to a rooted variant-2 fallback). A host that predates
/// variant 3 would run such a call as `CallBuiltinSym`.
/// v6: `Op::Aref` calls `neovm_jit_aref` and `Op::Aset` calls `neovm_jit_aset`
/// (value-returning shims; a `VALUE_SHIM_*` tag-`0b001` word is a sentinel), and
/// `neovm_jit_named_builtin` no longer has variant 3.
/// v7: `Op::Memq` and `Op::Assq` call `neovm_jit_memq` / `neovm_jit_assq`.
/// v8: `Op::VarRef` reads a plain symbol's value cell inline, through the
/// `Context`/`Obarray`/`LispSymbol` layout offsets salted below.
/// v9: `Op::Setcar` and `Op::Setcdr` call `neovm_jit_setcar` /
/// `neovm_jit_setcdr`.
const ABI_TAG_VERSION: u32 = 9;

/// Format version of the AOT descriptor spec-section + the runtime spec ABI
/// (`SpecSlot`/`spec_expected` sidecar bases, the loader re-classify+arm protocol).
/// Bump on ANY change to the spec encoding or the arm/disarm contract; salted into
/// [`ABI_TAG`] so a stale `.so` with a different spec ABI is refused.
const SPEC_ENCODING_VERSION: u32 = 1;

// The FULL set of `neovm_jit_*` runtime shims an AOT `.so` may import (resolved
// against the host at `dlopen`). This is the complete `declare_rt_refs` set —
// the host exports ALL of them (`#[unsafe(no_mangle)] pub` + the per-shim
// `--export-dynamic-symbol` in build.rs) and ALL are salted into `ABI_TAG`
// (audit #15), so a shim-ABI change invalidates artifacts and a call-bearing
// leaf that references any of them resolves at load.
//
// SINGLE SOURCE OF TRUTH (R2-C2): the list itself lives in `shim_names.rs` and
// is `include!`-ed here AND by both `build.rs` files, so the emit/salt set, the
// neovm-core lib export set, and the neomacs-bin export set can never drift.
// `MIR_SHIM_NAMES` is an alias for the included `NEOVM_JIT_SHIM_NAMES`. Still
// MUST match the shim DEFINITIONS in `compile.rs` + the `JIT_SHIM_TABLE` array.
include!("shim_names.rs");

/// The exported `neovm_jit_*` shim name set (alias of the single-source
/// `NEOVM_JIT_SHIM_NAMES` from `shim_names.rs`).
pub(crate) const MIR_SHIM_NAMES: &[&str] = NEOVM_JIT_SHIM_NAMES;

/// Compute [`ABI_TAG`] at compile time from the structural invariants. A `const`
/// FNV-1a over the salient constants + the shim names, so any drift in the ABI
/// the code assumes changes the tag (and old `.so`s no longer match).
const fn compute_abi_tag() -> u32 {
    // FNV-1a (32-bit), const-evaluable.
    let mut h: u32 = 0x811c_9dc5;
    macro_rules! mix_u64 {
        ($v:expr) => {{
            let v: u64 = $v;
            let mut i = 0;
            while i < 8 {
                let byte = ((v >> (i * 8)) & 0xff) as u32;
                h ^= byte;
                h = h.wrapping_mul(0x0100_0193);
                i += 1;
            }
        }};
    }
    mix_u64!(ABI_TAG_VERSION as u64);
    // The inline `Op::VarRef` read bakes these layout facts into the code.
    mix_u64!(core::mem::offset_of!(crate::emacs_core::eval::Context, obarray) as u64);
    mix_u64!(crate::emacs_core::symbol::OBARRAY_JIT_SPINE_OFFSET as u64);
    mix_u64!(crate::emacs_core::symbol::OBARRAY_JIT_LEN_OFFSET as u64);
    mix_u64!(crate::emacs_core::symbol::LISP_SYMBOL_SIZE as u64);
    mix_u64!(crate::emacs_core::symbol::LISP_SYMBOL_FLAGS_OFFSET as u64);
    mix_u64!(crate::emacs_core::symbol::LISP_SYMBOL_VAL_OFFSET as u64);
    // STATUS_* codes (the loader + code agree on these). STATUS_NEED_GENERIC
    // never crosses the leaf entry ABI (it is consumed inside a leaf's OWN
    // generated code by the fast-shim -> generic-fallback branch — this now
    // includes AOT baseline leaves, which as of increment A DO contain CBSym spec
    // sites; the round-1/`Op::Call` subr spec sites remain JIT-only), but it is
    // part of the status-code SPACE — salt it so any renumbering re-tags.
    mix_u64!(super::compile::STATUS_OK as u64);
    mix_u64!(super::compile::STATUS_DEOPT as u64);
    mix_u64!(super::compile::STATUS_SIGNAL as u64);
    mix_u64!(super::compile::STATUS_DEOPT_AT as u64);
    mix_u64!(super::compile::STATUS_NEED_GENERIC as u64);
    // Entry ABI shape: the unified 4-param entry ABI
    //   extern "C" fn(*mut u8, *const i64, *mut i64, *const LeafSidecar) -> i64
    // Encoded as <param_count><return_count> so a future arity/return change
    // re-tags artifacts (#32-audit minor: the old code claimed 3 params).
    mix_u64!(0x0004_0001);
    // LeafSidecar layout (the per-thread base block read through the 4th param):
    // a layout change (field add/reorder/resize) MUST re-tag stale `.so`s.
    mix_u64!(core::mem::size_of::<super::compile::LeafSidecar>() as u64);
    // DeoptCells layout: 3 i64 cells (pc, depth, handlers).
    mix_u64!(core::mem::size_of::<DeoptCells>() as u64);
    // Shim name set (count + each byte) — a shim-ABI change re-tags artifacts.
    mix_u64!(MIR_SHIM_NAMES.len() as u64);
    let mut si = 0;
    while si < MIR_SHIM_NAMES.len() {
        let name = MIR_SHIM_NAMES[si].as_bytes();
        let mut bi = 0;
        while bi < name.len() {
            h ^= name[bi] as u32;
            h = h.wrapping_mul(0x0100_0193);
            bi += 1;
        }
        si += 1;
    }
    // R2 increment A: the Tier-A `CBSYM_A_*` discriminants are baked into AOT code
    // as an iconst `which` and switched on by `neovm_jit_cbsym_read` at load. A
    // renumber (or a swap, e.g. `CBSYM_A_POINT`<->`CBSYM_A_POINT_MIN`) would make a
    // stale `.so` read the WRONG Tier-A op, so salt every discriminant VALUE in
    // definition order — catches a renumber, a swap, AND a count change. (The
    // shim-name loop above already salts the two new shim NAMES.)
    mix_u64!(super::compile::CBSYM_A_POINT as u64);
    mix_u64!(super::compile::CBSYM_A_POINT_MIN as u64);
    mix_u64!(super::compile::CBSYM_A_POINT_MAX as u64);
    mix_u64!(super::compile::CBSYM_A_BOLP as u64);
    mix_u64!(super::compile::CBSYM_A_EOLP as u64);
    mix_u64!(super::compile::CBSYM_A_BOBP as u64);
    mix_u64!(super::compile::CBSYM_A_EOBP as u64);
    mix_u64!(super::compile::CBSYM_A_FOLLOWING_CHAR as u64);
    mix_u64!(super::compile::CBSYM_A_PRECEDING_CHAR as u64);
    mix_u64!(super::compile::CBSYM_A_CHAR_AFTER as u64);
    mix_u64!(super::compile::CBSYM_A_CURRENT_BUFFER as u64);
    mix_u64!(super::compile::CBSYM_A_MATCH_BEGINNING as u64);
    mix_u64!(super::compile::CBSYM_A_MATCH_END as u64);
    // R2 increment B2 (Op::Call spec-in-AOT): the descriptor spec-section format +
    // the runtime spec ABI. A `SpecSlot` layout change, a `SpecCalleeKind`
    // discriminant renumber/count change, or a spec-encoding bump MUST re-tag stale
    // `.so`s (their baked slot arithmetic / kind discriminants would mismatch).
    mix_u64!(SPEC_ENCODING_VERSION as u64);
    mix_u64!(core::mem::size_of::<super::compile::SpecSlot>() as u64);
    mix_u64!(super::compile::SpecCalleeKind::DISC_COUNT as u64);
    h
}

/// The exported entry symbol for an AOT leaf with the given content hash:
/// `__neovm_aot_{hash:032x}_{ABI_TAG:08x}`. The tag is in the symbol so a
/// mismatched-ABI `.so` cannot even be `dlsym`'d under the current tag (a second,
/// cheap interlock on top of the descriptor check).
pub(crate) fn aot_entry_symbol(content_hash: u128) -> String {
    format!("__neovm_aot_{content_hash:032x}_{ABI_TAG:08x}")
}

// ---------------------------------------------------------------------------
// R1c-3: per-Value rebuild recipe (canonical, pointer-free).
//
// Heap-object reloc constants cannot bake a pointer into a cross-session `.so`,
// so each is serialized as a recipe (fixnum→bits, string→utf8, symbol→name,
// cons→recursive) and rebuilt against the LIVE obarray/heap at load. The SAME
// canonical encoding feeds the content hash (R1c-2), so two bodies with
// identical structure + constants hash identically regardless of heap layout.
// ---------------------------------------------------------------------------

/// Recipe type tags (1 byte) for [`write_value_recipe`] / [`rebuild_value`].
const RECIPE_FIXNUM: u8 = 1;
const RECIPE_STRING: u8 = 2;
const RECIPE_SYMBOL: u8 = 3;
const RECIPE_CONS: u8 = 4;
const RECIPE_NIL: u8 = 5;
const RECIPE_T: u8 = 6;

/// A Value whose type the AOT recipe codec does not (yet) support. The emitter
/// bails to the JIT rather than emit an artifact it cannot rebuild — keeping AOT
/// strictly additive (R1c-6: miss/error → JIT).
#[derive(Debug)]
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) struct UnsupportedRecipe(pub Value);

/// Serialize one `Value` into `out` as a canonical, pointer-free recipe.
///
/// Supports the const subset AOT can rebuild: fixnum, string, symbol, cons
/// (recursive), and the nil/t immediates. Anything else (float, vector, hash
/// table, ...) returns [`UnsupportedRecipe`] so the caller bails to the JIT.
pub(crate) fn write_value_recipe(out: &mut Vec<u8>, v: Value) -> Result<(), UnsupportedRecipe> {
    if v == Value::NIL {
        out.push(RECIPE_NIL);
        return Ok(());
    }
    if v == Value::T {
        out.push(RECIPE_T);
        return Ok(());
    }
    if let Some(n) = v.as_fixnum() {
        out.push(RECIPE_FIXNUM);
        out.extend_from_slice(&n.to_le_bytes());
        return Ok(());
    }
    if let Some(s) = v.as_lisp_string() {
        let bytes = s.as_bytes();
        out.push(RECIPE_STRING);
        // Multibyte flag (#32-audit minor): two strings with identical bytes but
        // differing multibyte-ness are DISTINCT (`LispString` PartialEq compares
        // the flag), and `from_emacs_bytes` re-derives the flag from byte content
        // alone — so an all-ASCII unibyte string would round-trip as multibyte.
        // Record the flag so the recipe hashes/verifies/rebuilds it faithfully.
        out.push(u8::from(s.is_multibyte()));
        out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        out.extend_from_slice(bytes);
        return Ok(());
    }
    if let Some(id) = v.as_symbol_id() {
        // A symbol is identified across sessions BY NAME, so reloc-by-name is
        // sound ONLY for the CANONICAL interned symbol of that name. An
        // UNINTERNED / gensym (`make-symbol`, cl-macro/pcase expansion output)
        // has a non-unique name, so a different session would resolve a
        // DIFFERENT symbol → wrong result. Reject it so the whole leaf falls to
        // the JIT (which bakes the in-session SymId, correct same-session).
        // (Audit #16 gensym hole.)
        if !crate::emacs_core::intern::is_canonical_id(id) {
            return Err(UnsupportedRecipe(v));
        }
        // Encode the name BYTE-faithfully (audit #B): elisp symbol names can be
        // non-UTF-8 (overlong C0/C1 raw bytes from `.elc`). `resolve_sym` would
        // `panic!` on a non-UTF-8 name — fatal on the runtime cache path. Use the
        // raw `LispString` bytes so the codec never panics AND round-trips any
        // name; the rebuild side re-interns from the same bytes.
        let name = crate::emacs_core::intern::resolve_sym_lisp_string(id);
        let nb = name.as_bytes();
        out.push(RECIPE_SYMBOL);
        // Multibyte flag (#32-audit minor): include the name string's
        // multibyte-ness so two symbols whose names share bytes but differ in
        // the flag hash/verify distinctly (reloc identity itself comes from
        // func.constants — this only sharpens hash/verify discrimination).
        out.push(u8::from(name.is_multibyte()));
        out.extend_from_slice(&(nb.len() as u64).to_le_bytes());
        out.extend_from_slice(nb);
        return Ok(());
    }
    if v.is_cons() {
        out.push(RECIPE_CONS);
        write_value_recipe(out, v.cons_car())?;
        write_value_recipe(out, v.cons_cdr())?;
        return Ok(());
    }
    Err(UnsupportedRecipe(v))
}

/// Max cons nesting a reloc recipe may rebuild — bounds the recursion so a
/// crafted/corrupt recipe (a deep RECIPE_CONS chain) cannot overflow the stack.
/// Real loadup const lists nest far shallower; a deeper recipe falls to JIT.
const MAX_RECIPE_CONS_DEPTH: usize = 256;

/// Rebuild a `Value` from a recipe slice, allocating fresh heap objects against
/// the LIVE thread-local heap + obarray (a string/cons born here is allocated
/// black by the GC's alloc path; rooting is the caller's duty — R1c-8). Returns
/// the value + the number of bytes consumed, or `None` on a malformed/truncated/
/// over-deep recipe.
///
/// Hardening (audit #4-9/#12): the recipe is dlsym'd out of a `.so` in the
/// NEOVM_AOT_DIR (a trust boundary). Although that dir already grants RCE (dlopen
/// runs the `.so`), this parser must FAIL CLOSED — every length/index is
/// bounds-checked and cons recursion is depth-bounded, so a corrupt artifact
/// returns `None` (→ the loader falls through to the JIT, honoring the additive
/// contract) instead of panicking / over-reading / overflowing the stack.
pub(crate) fn rebuild_value(bytes: &[u8], depth: usize) -> Option<(Value, usize)> {
    if depth > MAX_RECIPE_CONS_DEPTH {
        return None;
    }
    let tag = *bytes.first()?;
    match tag {
        RECIPE_NIL => Some((Value::NIL, 1)),
        RECIPE_T => Some((Value::T, 1)),
        RECIPE_FIXNUM => {
            let n = i64::from_le_bytes(bytes.get(1..9)?.try_into().ok()?);
            Some((Value::make_int(n), 9))
        }
        RECIPE_STRING => {
            // Layout (post-#32-audit): tag, multibyte flag (1B), len (8B), bytes.
            let multibyte = *bytes.get(1)? != 0;
            let len = u64::from_le_bytes(bytes.get(2..10)?.try_into().ok()?) as usize;
            // Byte-faithful (audit #B): elisp strings may be raw unibyte / non-
            // UTF-8, so reconstruct from the exact bytes, not a UTF-8 `&str`.
            let raw = bytes.get(10..10usize.checked_add(len)?)?.to_vec();
            // Honor the recorded flag (#32-audit minor): an all-ASCII unibyte
            // string must stay unibyte, not be promoted by `from_emacs_bytes`.
            let ls = if multibyte {
                crate::heap_types::LispString::from_emacs_bytes(raw)
            } else {
                crate::heap_types::LispString::from_unibyte(raw)
            };
            Some((Value::heap_string(ls), 10 + len))
        }
        RECIPE_SYMBOL => {
            // Layout (post-#32-audit): tag, name multibyte flag (1B), len (8B), bytes.
            // The flag discriminates the hash/verify; the canonical interned symbol
            // is identified by its NAME BYTES (intern_lisp_string), so the rebuilt
            // name string's flag tracks the recorded one for faithful re-intern.
            let multibyte = *bytes.get(1)? != 0;
            let len = u64::from_le_bytes(bytes.get(2..10)?.try_into().ok()?) as usize;
            // Byte-faithful symbol name (audit #B): re-intern from the raw bytes
            // so a non-UTF-8 symbol name round-trips to the same canonical symbol.
            let raw = bytes.get(10..10usize.checked_add(len)?)?.to_vec();
            let ls = if multibyte {
                crate::heap_types::LispString::from_emacs_bytes(raw)
            } else {
                crate::heap_types::LispString::from_unibyte(raw)
            };
            let id = crate::emacs_core::intern::intern_lisp_string(&ls);
            Some((Value::symbol(id), 10 + len))
        }
        RECIPE_CONS => {
            let (car, n1) = rebuild_value(bytes.get(1..)?, depth + 1)?;
            let (cdr, n2) = rebuild_value(bytes.get(1 + n1..)?, depth + 1)?;
            Some((Value::cons(car, cdr), 1 + n1 + n2))
        }
        _ => None, // unknown tag → fail closed.
    }
}

/// Content hash of a leaf's SOURCE (`ops` + canonical `constants` + `arity`),
/// salted by [`ABI_TAG`]. Gensym-stable: bytecode `Op`s carry only indices /
/// immediates (no pointers), and constants are canonicalized by VALUE (fixnum
/// bits, string bytes, symbol names, recursive conses) not by heap address — so
/// the same source hashes identically across sessions. The lambda-list `arity`
/// is folded in (the spec's arity-drift requirement). Returns `None` if any
/// constant is outside the recipe-supported subset (caller bails to JIT).
///
/// Truncated to `u128` (the entry-symbol width). Body identity rests SOLELY on
/// this hash (cryptographically collision-resistant for honest inputs) plus the
/// trusted `NEOVM_AOT_DIR` (any actor who can plant a `.so` there already has
/// in-process RCE via dlopen, so a hash collision is not an added attack
/// surface). NOTE (audit #11): there is NO load-time re-verification that the
/// rebuilt const vector equals the call-site constants — the hash is the whole
/// proof. Adding that recheck is a documented defense-in-depth follow-up.
/// Is `op`'s `Debug` form a CANONICAL (session-independent) identity for the
/// content hash? `true` → its Debug encodes only pool indices / immediates and
/// is safe to hash. `false` → its Debug embeds session-specific data (a raw
/// `SymId` or a heap `Value`) and the leaf must bail to the JIT.
///
/// This is deliberately an EXHAUSTIVE `match` with NO wildcard arm (#32-audit
/// minor): when a new `Op` variant is added, this fails to compile until the
/// author classifies it. The compiler thus ENFORCES the documented duty — a
/// future SymId/Value-bearing op cannot slip into the AOT hash unnoticed.
fn op_debug_is_canonical(op: &Op) -> bool {
    match op {
        // The sole session-specific variant today: its Debug embeds a raw,
        // intern-order-dependent `SymId` (audit #17). Bail.
        Op::CallBuiltinSym(..) => false,

        // Everything else carries only pool indices (u16) / jump targets (u32)
        // / immediates (u8) / no payload — all session-independent. Listed
        // explicitly (no `_`) so a new variant forces a decision here.
        Op::Constant(..)
        | Op::TrapOutOfRangeConstant(..)
        | Op::Nil
        | Op::True
        | Op::Pop
        | Op::Dup
        | Op::StackRef(..)
        | Op::StackSet(..)
        | Op::DiscardN(..)
        | Op::VarRef(..)
        | Op::VarSet(..)
        | Op::VarBind(..)
        | Op::Unbind(..)
        | Op::Call(..)
        | Op::Apply(..)
        | Op::Goto(..)
        | Op::GotoIfNil(..)
        | Op::GotoIfNotNil(..)
        | Op::GotoIfNilElsePop(..)
        | Op::GotoIfNotNilElsePop(..)
        | Op::Switch
        | Op::Return
        | Op::Add
        | Op::Sub
        | Op::Mul
        | Op::Div
        | Op::Rem
        | Op::Add1
        | Op::Sub1
        | Op::Negate
        | Op::Eqlsign
        | Op::Gtr
        | Op::Lss
        | Op::Leq
        | Op::Geq
        | Op::Max
        | Op::Min
        | Op::Car
        | Op::Cdr
        | Op::Cons
        | Op::List(..)
        | Op::Length
        | Op::Nth
        | Op::Nthcdr
        | Op::Setcar
        | Op::Setcdr
        | Op::CarSafe
        | Op::CdrSafe
        | Op::Elt
        | Op::Nconc
        | Op::Nreverse
        | Op::Member
        | Op::Memq
        | Op::Assq
        | Op::Symbolp
        | Op::Consp
        | Op::Stringp
        | Op::Listp
        | Op::Integerp
        | Op::Numberp
        | Op::Null
        | Op::Not
        | Op::Eq
        | Op::Equal
        | Op::Concat(..)
        | Op::Substring
        | Op::StringEqual
        | Op::StringLessp
        | Op::Aref
        | Op::Aset
        | Op::SymbolValue
        | Op::SymbolFunction
        | Op::Set
        | Op::Fset
        | Op::Get
        | Op::Put
        | Op::PushConditionCase(..)
        | Op::PushConditionCaseRaw(..)
        | Op::PushCatch(..)
        | Op::PopHandler
        | Op::UnwindProtectPop
        | Op::Throw
        | Op::SaveCurrentBuffer
        | Op::SaveExcursion
        | Op::SaveRestriction
        | Op::SaveWindowExcursion
        | Op::MakeClosure(..)
        | Op::CallBuiltin(..) => true,
    }
}

pub(crate) fn leaf_content_hash(ops: &[Op], constants: &[Value], arity: usize) -> Option<u128> {
    use sha2::{Digest, Sha256};
    // Probe seam (task #11): count hash ATTEMPTS so a test can assert the
    // manifest pre-filter skipped a candidate without paying this function.
    #[cfg(test)]
    test_support::note_hash_call();
    let mut h = Sha256::new();
    h.update(ABI_TAG.to_le_bytes());
    h.update((arity as u64).to_le_bytes());
    h.update((ops.len() as u64).to_le_bytes());
    // Ops: their Debug form is deterministic for this enum (Constant(idx)/Add/
    // Goto(t)/...) and pointer-free, so it canonically identifies the body —
    // EXCEPT an op whose Debug embeds SESSION-SPECIFIC data (a raw `SymId` or a
    // `Value` pointer). `op_debug_is_canonical` is an EXHAUSTIVE match (#32-audit
    // minor): a future SymId/Value-bearing `Op` variant fails to compile here
    // until it is explicitly classified as canonical-or-bail, so we can never
    // silently key the AOT cache on a session-specific op.
    // One reusable buffer for the per-op Debug rendering (Gap 4b: a fresh
    // `format!` String per op dominated the allocator profile of the startup
    // prepopulate pass, which hashes every loadup body). Identical hash bytes.
    let mut s = String::new();
    for op in ops {
        match op {
            // R2-E (must-nail #2, the cache-KEY half): CallBuiltinSym's Debug embeds
            // a SESSION-SPECIFIC raw SymId. Hash it BY the callee's canonical NAME
            // (byte-faithful) + nargs instead — session-stable. (Cross-session
            // CORRECTNESS — that the loaded leaf calls the right builtin — is the
            // reloc-by-name half, handled separately in the lowering/recipe.) A
            // gensym/uninterned callee → bail (non-unique name, #16 hole).
            Op::CallBuiltinSym(sym, n) => {
                if !crate::emacs_core::intern::is_canonical_id(*sym) {
                    return None;
                }
                let name = crate::emacs_core::intern::resolve_sym_lisp_string(*sym);
                let nb = name.as_bytes();
                h.update(b"CBS:");
                h.update([*n]);
                h.update((nb.len() as u64).to_le_bytes());
                h.update(nb);
            }
            _ => {
                if !op_debug_is_canonical(op) {
                    return None;
                }
                use std::fmt::Write as _;
                s.clear();
                let _ = write!(s, "{op:?}"); // fmt to a String is infallible.
                h.update((s.len() as u64).to_le_bytes());
                h.update(s.as_bytes());
            }
        }
    }
    // Constants: canonical recipe bytes (by value, not address).
    h.update((constants.len() as u64).to_le_bytes());
    let mut recipe = Vec::new();
    for &c in constants {
        recipe.clear();
        write_value_recipe(&mut recipe, c).ok()?;
        h.update((recipe.len() as u64).to_le_bytes());
        h.update(&recipe);
    }
    let digest = h.finalize();
    Some(u128::from_le_bytes(digest[..16].try_into().unwrap()))
}

// ---------------------------------------------------------------------------
// R1c-3 (cont.): the leaf DESCRIPTOR — a versioned, exported data blob carrying
// the lambda-list + frame metadata + reloc rebuild recipe. Emitted into the `.o`
// alongside the entry; dlsym'd + parsed by the loader (R1c-5).
// ---------------------------------------------------------------------------

/// Magic + version header on every descriptor blob, so the loader rejects a
/// truncated/foreign blob and a format change can be detected. The ABI_TAG also
/// rides along (a second interlock besides the entry-symbol tag).
const DESC_MAGIC: u32 = 0x4e41_4f54; // "NAOT"
/// v2 (R2 increment B2): the fixed header gains a `spec_count:u32` (right before
/// `recipe_len`) and the blob gains a spec-section (`spec_count` × [`AotSpecSite`],
/// [`SPEC_SITE_BYTES`] each) appended AFTER the recipe. A v1 loader/`.so` mismatch
/// is refused by the version check (and the ABI_TAG changed anyway).
const DESC_VERSION: u32 = 2;

/// One `Op::Call` speculation site baked into an AOT baseline leaf, recorded so the
/// LOADER can RE-CLASSIFY it against the live obarray and arm/disarm the runtime
/// `SpecSlot`. NO baked address/epoch (those are session-specific — the loader
/// derives `expected` from the live cell + `epoch` from `ob.function_epoch()`):
/// only the callee's reloc index (to recover its SymId at load), the baked
/// `kind_disc` (to require an exact live re-classification match), and the site's
/// `nargs` (so the loader re-runs `subr_spec_kind` with the same arity).
///
/// Emitted in SLOT ORDER: the loader's array position IS the codegen slot index
/// (`spec_slot_base[idx]` / `spec_expected_base[idx]`). `repr(C)` + fixed 8-byte
/// on-disk layout ([`SPEC_SITE_BYTES`]); the in-memory struct is decode-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AotSpecSite {
    /// The baked [`SpecCalleeKind::to_spec_disc`] value (0..DISC_COUNT). The loader
    /// arms ONLY when the live re-classification yields this same discriminant.
    pub kind_disc: u8,
    /// Reserved (0): a per-kind sub-discriminant for future variants (e.g. a
    /// `PredKind` split). Currently unused; salted into the ABI via the format.
    pub which: u8,
    /// The site's `Op::Call(n)` argument count, replayed into the loader's
    /// `subr_spec_kind(live_cell, sym, nargs)` re-classification.
    pub nargs: u16,
    /// Index into the leaf's reloc vector where the callee symbol lives; the loader
    /// reads `reloc_data[callee_reloc_idx].as_symbol_id()` to recover the SymId.
    pub callee_reloc_idx: u32,
}

/// On-disk byte width of one [`AotSpecSite`] (kind_disc:1 + which:1 + nargs:2 +
/// callee_reloc_idx:4). Fixed so the loader can bound + slice the spec-section.
const SPEC_SITE_BYTES: usize = 1 + 1 + 2 + 4;

/// Max `Op::Call` spec sites a single leaf may carry — bounds a crafted/corrupt
/// `spec_count` before it drives a huge allocation/over-read. Real leaves have a
/// handful; the operand stack is a u16 so no honest body approaches this.
const MAX_SPEC_SITES: u32 = 64 * 1024;

/// The decoded descriptor: leaf metadata + the reloc rebuild recipe bytes.
pub(crate) struct AotDescriptor {
    pub meta: super::compile::AotLeafMeta,
    /// Concatenated per-slot recipes (R1c-3); rebuilt into the reloc Vec at load.
    pub reloc_recipe: Vec<u8>,
    /// Number of reloc slots (recipes) in `reloc_recipe`.
    pub reloc_count: u32,
    /// R2 increment B2: the `Op::Call` spec sites, in slot order (empty for a MIR
    /// leaf or a baseline leaf with no armed spec site).
    pub spec_sites: Vec<AotSpecSite>,
}

/// Serialize an [`AotDescriptor`] to bytes (little-endian, fixed header + recipe
/// tail + spec-section tail). Layout: magic, version, ABI_TAG, the meta fields,
/// reloc_count, `spec_count` (v2), recipe_len, the concatenated recipe bytes, then
/// the spec-section (`spec_count` × [`SPEC_SITE_BYTES`], in slot order). `spec_count`
/// sits RIGHT BEFORE recipe_len so the fixed header stays contiguous.
pub(crate) fn encode_descriptor(
    meta: &super::compile::AotLeafMeta,
    reloc_recipe: &[u8],
    reloc_count: u32,
    spec_sites: &[AotSpecSite],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&DESC_MAGIC.to_le_bytes());
    out.extend_from_slice(&DESC_VERSION.to_le_bytes());
    out.extend_from_slice(&ABI_TAG.to_le_bytes());
    out.extend_from_slice(&(meta.arity as u64).to_le_bytes());
    out.extend_from_slice(&(meta.required as u64).to_le_bytes());
    out.push(u8::from(meta.has_rest));
    out.push(u8::from(meta.has_binds));
    out.push(u8::from(meta.has_handlers));
    out.push(u8::from(meta.has_side_effects));
    out.push(u8::from(meta.has_precise_deopt));
    out.extend_from_slice(&(meta.max_depth as u64).to_le_bytes());
    out.extend_from_slice(&reloc_count.to_le_bytes());
    // v2: spec_count right before recipe_len (keeps the fixed header contiguous).
    out.extend_from_slice(&(spec_sites.len() as u32).to_le_bytes());
    out.extend_from_slice(&(reloc_recipe.len() as u64).to_le_bytes());
    out.extend_from_slice(reloc_recipe);
    // Spec-section, AFTER the recipe, in slot order (position == codegen slot idx).
    for s in spec_sites {
        out.push(s.kind_disc);
        out.push(s.which);
        out.extend_from_slice(&s.nargs.to_le_bytes());
        out.extend_from_slice(&s.callee_reloc_idx.to_le_bytes());
    }
    out
}

/// Parse a descriptor blob. Returns `None` on a bad magic/version/ABI_TAG or a
/// truncated blob — the loader then refuses the artifact and falls back to JIT.
pub(crate) fn decode_descriptor(bytes: &[u8]) -> Option<AotDescriptor> {
    fn rd_u32(b: &[u8], at: &mut usize) -> Option<u32> {
        let v = b.get(*at..*at + 4)?;
        *at += 4;
        Some(u32::from_le_bytes(v.try_into().ok()?))
    }
    fn rd_u64(b: &[u8], at: &mut usize) -> Option<u64> {
        let v = b.get(*at..*at + 8)?;
        *at += 8;
        Some(u64::from_le_bytes(v.try_into().ok()?))
    }
    fn rd_u8(b: &[u8], at: &mut usize) -> Option<u8> {
        let v = *b.get(*at)?;
        *at += 1;
        Some(v)
    }
    let mut at = 0usize;
    if rd_u32(bytes, &mut at)? != DESC_MAGIC {
        return None;
    }
    if rd_u32(bytes, &mut at)? != DESC_VERSION {
        return None;
    }
    if rd_u32(bytes, &mut at)? != ABI_TAG {
        return None; // foreign / stale ABI — refuse.
    }
    let arity = rd_u64(bytes, &mut at)? as usize;
    let required = rd_u64(bytes, &mut at)? as usize;
    let has_rest = rd_u8(bytes, &mut at)? != 0;
    let has_binds = rd_u8(bytes, &mut at)? != 0;
    let has_handlers = rd_u8(bytes, &mut at)? != 0;
    let has_side_effects = rd_u8(bytes, &mut at)? != 0;
    let has_precise_deopt = rd_u8(bytes, &mut at)? != 0;
    let max_depth = rd_u64(bytes, &mut at)? as usize;
    let reloc_count = rd_u32(bytes, &mut at)?;
    // v2: spec_count sits right before recipe_len.
    let spec_count = rd_u32(bytes, &mut at)?;
    if spec_count > MAX_SPEC_SITES {
        return None; // crafted/corrupt count — fail closed.
    }
    let recipe_len = rd_u64(bytes, &mut at)? as usize;
    // checked_add (audit minor): recipe_len is untrusted; match the file's
    // all-checked-slicing invariant so a crafted blob fails closed (None), never
    // debug-panics on overflow, even for a future direct caller of this fn.
    let recipe_end = at.checked_add(recipe_len)?;
    let reloc_recipe = bytes.get(at..recipe_end)?.to_vec();
    // Spec-section AFTER the recipe: spec_count fixed-width records.
    let mut at = recipe_end;
    let mut spec_sites = Vec::with_capacity(spec_count as usize);
    for _ in 0..spec_count {
        let kind_disc = rd_u8(bytes, &mut at)?;
        let which = rd_u8(bytes, &mut at)?;
        let nargs = u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?);
        at += 2;
        let callee_reloc_idx = rd_u32(bytes, &mut at)?;
        spec_sites.push(AotSpecSite {
            kind_disc,
            which,
            nargs,
            callee_reloc_idx,
        });
    }
    Some(AotDescriptor {
        meta: super::compile::AotLeafMeta {
            arity,
            required,
            has_rest,
            has_binds,
            has_handlers,
            has_side_effects,
            max_depth,
            has_precise_deopt,
        },
        reloc_recipe,
        reloc_count,
        spec_sites,
    })
}

/// The exported descriptor symbol for an AOT leaf: `__neovm_aotd_{hash}_{tag}`.
pub(crate) fn aot_descriptor_symbol(content_hash: u128) -> String {
    format!("__neovm_aotd_{content_hash:032x}_{ABI_TAG:08x}")
}

/// Max reloc slots a single leaf may carry — bounds a crafted/corrupt
/// reloc_count before it drives a huge allocation. Real leaves have a handful.
const MAX_RELOC_COUNT: u32 = 64 * 1024;

/// Rebuild the reloc-const Vec from a descriptor's recipe — each per-slot recipe
/// decoded against the LIVE heap/obarray (byte-faithful; `None` on malformed/
/// over-long). Returns `None` on a malformed/over-long recipe.
///
/// NOT used by the in-session load path: audit #A made `load_leaf_from_unit`
/// source `reloc_data` from the LIVE function's own constants (eq-identical to
/// interp/JIT), and use the recipe only to VERIFY the `.so` matches. This
/// rebuild-from-recipe is retained for a future TRUE-cross-session path (a load
/// with no live source function), where fresh objects are the only option.
#[allow(dead_code)]
pub(crate) fn rebuild_reloc_consts(desc: &AotDescriptor) -> Option<Box<[Value]>> {
    if desc.reloc_count > MAX_RELOC_COUNT {
        return None;
    }
    let mut out = Vec::with_capacity(desc.reloc_count as usize);
    let mut at = 0usize;
    for _ in 0..desc.reloc_count {
        let (v, n) = rebuild_value(desc.reloc_recipe.get(at..)?, 0)?;
        out.push(v);
        at = at.checked_add(n)?;
    }
    // Every recipe byte must be consumed — a trailing tail is a malformed blob.
    if at != desc.reloc_recipe.len() {
        return None;
    }
    Some(out.into_boxed_slice())
}

/// Build a leaf's `reloc_data` from the DESCRIPTOR's recipe — replacing the
/// load-time live-reloc re-collection (`live_reloc_for_emit_tier`, which ran
/// `build_mir` per load: the measured 13.5µs/leaf floor of the AOT prewarm).
///
/// Semantics (audit #A preserved):
/// - symbols re-intern BY NAME → the eq-canonical interned object (identical
///   to the function's own constant);
/// - nil/t/fixnums are exact immediates;
/// - heap values (strings, cons trees) are rebuilt from the recipe, then
///   eq-UPGRADED to the function's own constant when an `equal` match exists
///   in `constants` — restoring shared identity with interp/JIT.
///
/// VERIFICATION (replaces the old live-recipe byte-compare): every resolved
/// non-immediate value MUST be eq/`equal`-matched by the live constant pool.
/// A stale or foreign `.so` whose recipe references anything outside this
/// function's pool is rejected (`None` → the caller stays on the JIT). The
/// recipe order IS the emitted reloc-index order, so the produced vector is
/// definitionally index-aligned with the generated code's loads.
fn resolve_reloc_from_descriptor(
    desc: &AotDescriptor,
    constants: &[Value],
) -> Option<Box<[Value]>> {
    if desc.reloc_count > MAX_RELOC_COUNT {
        return None;
    }
    let mut out = Vec::with_capacity(desc.reloc_count as usize);
    let mut at = 0usize;
    for _ in 0..desc.reloc_count {
        let (v, n) = rebuild_value(desc.reloc_recipe.get(at..)?, 0)?;
        at = at.checked_add(n)?;
        let resolved = if v == Value::NIL || v == Value::T || v.is_fixnum() {
            v
        } else if v.as_symbol_id().is_some() {
            // Interning by name already made it the canonical object — the
            // same symbol every tier resolves. No pool-membership demand: a
            // reloc symbol can be DERIVED from a pool constant rather than
            // equal to it (e.g. the bare symbol stripped from a
            // symbol-with-position VarRef operand).
            v
        } else {
            // Heap value: eq-UPGRADE to the pool's own object when a deep-
            // equal match exists (`Value ==` is deep equal — the documented
            // footgun is exactly the tool here; the pool object also carries
            // any text properties the recipe's byte encoding could not).
            // Absent from the pool (a derived value), keep the fresh rebuild —
            // the pre-audit-#A behavior, correct if not identity-shared.
            // Authenticity rests on the content hash that NAMED this entry:
            // exact by construction on the prewarm path (manifest hash checked
            // against the immutable marked object) and a 128-bit body hash on
            // the general path.
            constants.iter().copied().find(|&c| c == v).unwrap_or(v)
        };
        out.push(resolved);
    }
    if at != desc.reloc_recipe.len() {
        return None;
    }
    Some(out.into_boxed_slice())
}

// ---------------------------------------------------------------------------
// R1c-2 + R1c-5 (emit side): the full pure-leaf → object pipeline.
// ---------------------------------------------------------------------------

/// Whether a MIR leaf is AOT-runnable, and why not.
///
/// The R1c call-bearing increment unblocks Call/Apply (+ escaping cons): the
/// `neovm_jit_*` shims a runtime-call body imports are now host-EXPORTED
/// (`#[unsafe(no_mangle)] pub` + the test/prod binary linked `-rdynamic`), so a
/// call/cons `.so` binds them at `dlopen`. The precise-deopt-across-call path is
/// already in the lowering (the sidecar `materialize_deopt_refs`), so a
/// call-bearing leaf that deopts mid-body resumes correctly. Combined with the
/// sidecar's reloc/symbol support, the AOT subset now matches the MIR pure tier.
///
/// Still EXCLUDED:
///   * `CallBuiltin`/`CallBuiltinSym` — embed a SESSION-SPECIFIC raw `SymId`
///     (audit #17), so they cannot be content-hashed canonically; also outside
///     the MIR pure tier (they bail in `build_mir`/lowering anyway).
///   * non-recipe-able constants (float/vector/uninterned-symbol) — caught
///     upstream: `write_value_recipe` bails → `leaf_content_hash` is `None`.
///
/// A rejected body stays JIT-only (strictly additive).
/// Whether the MIR lowering would route any instruction through the
/// baseline-emitter adapter (`lower_mir_inst_via_baseline`): an `eq`, a
/// shim predicate, or an `Opaque` other than `Call`/`Apply`.
fn uses_mir_adapter(m: &mir::MirFunction) -> bool {
    use mir::{MirOp, PredKind};
    m.blocks
        .iter()
        .flat_map(|b| b.insts.iter())
        .any(|i| match &i.op {
            MirOp::Eq(..)
            | MirOp::Pred(PredKind::Symbolp | PredKind::Integerp | PredKind::Numberp, _) => true,
            MirOp::Opaque { op, .. } => !matches!(op, Op::Call(_) | Op::Apply(_)),
            _ => false,
        })
}

fn mir_is_aot_runnable(m: &mir::MirFunction) -> bool {
    use mir::MirOp;
    // Reject the sym-bearing opaque ops (audit #17): their Debug-keyed hash
    // embeds a session-specific SymId, so the AOT cache key would be
    // non-canonical. Call/Apply are now ALLOWED (their shims are host-exported).
    let has_unsupported_opaque = m.blocks.iter().any(|b| {
        b.insts.iter().any(|i| {
            matches!(
                &i.op,
                MirOp::Opaque {
                    op: Op::CallBuiltin(..) | Op::CallBuiltinSym(..),
                    ..
                }
            )
        })
    });
    !has_unsupported_opaque
    // Call/Apply (precise deopt + call/apply/gc shims) and escaping cons (cons
    // shim) are now AOT-runnable — the shims are host-exported and the sidecar
    // carries the per-thread reloc + deopt bases.
}

/// Collect the reloc constants of a MIR leaf (the DISTINCT heap-object consts, in
/// first-seen order — same dedup as the lowering). Returns the ordered Values;
/// the recipe is emitted in this order and rebuilt into the same order at load.
fn collect_reloc_consts(m: &mir::MirFunction) -> Vec<Value> {
    use mir::MirOp;
    let mut seen = std::collections::HashMap::new();
    let mut out = Vec::new();
    for blk in &m.blocks {
        for inst in &blk.insts {
            if let MirOp::Const(v) = &inst.op {
                let bits = v.bits();
                // AOT reloc set: heap objects AND non-nil/t symbols (audit #16).
                // MUST match the lowering's `needs_reloc` decision (aot=true).
                if super::compile::const_relocs_for_aot(*v) && !seen.contains_key(&bits) {
                    seen.insert(bits, out.len());
                    out.push(*v);
                }
            }
        }
    }
    out
}

/// R2-E (baseline-tier AOT): collect the per-leaf reloc set for the BASELINE
/// lowering, walking the raw `ops` (the baseline tier has no MIR). Covers BOTH:
///   * const-pool relocs (`Op::Constant` → `constants[idx]`, same predicate as the
///     MIR path's `const_relocs_for_aot`: heap objects + non-nil/t symbols), AND
///   * the named-builtin callee SYMBOLS that the baseline bakes as a session SymId
///     (`Op::CallBuiltinSym(sym, _)` → the symbol by id; `Op::CallBuiltin(idx, _)`
///     → `constants[idx]`'s symbol) — the must-nail #2 op-operand reloc. Each is a
///     symbol `Value`, which the recipe codec encodes BY NAME (#16) and the
///     lowering reloads via `reloc_index` (compile.rs named-builtin site).
///
/// Returns `None` if ANY symbol is uninterned/gensym (`!is_canonical_id`) — a
/// non-unique name would resolve to a DIFFERENT symbol cross-session (#16 hole),
/// so the whole leaf bails to the JIT. The returned Vec is in first-seen order
/// (== the recipe order); the index maps each Value's tagged bits → slot.
fn collect_baseline_aot_relocs(
    ops: &[Op],
    constants: &[Value],
) -> Option<(Vec<Value>, std::collections::HashMap<usize, u32>)> {
    let mut seen: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
    let mut out: Vec<Value> = Vec::new();
    let add = |v: Value,
               out: &mut Vec<Value>,
               seen: &mut std::collections::HashMap<usize, u32>|
     -> Option<()> {
        if !super::compile::const_relocs_for_aot(v) {
            return Some(()); // immediate (fixnum/nil/t/char) — baked, not reloc'd.
        }
        // Gensym guard (#16): reloc-by-name is sound only for the CANONICAL interned
        // symbol of a name. An uninterned/gensym symbol → bail the whole leaf.
        if let Some(id) = v.as_symbol_id()
            && !crate::emacs_core::intern::is_canonical_id(id)
        {
            return None;
        }
        let bits = v.bits();
        if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(bits) {
            e.insert(out.len() as u32);
            out.push(v);
        }
        Some(())
    };
    for op in ops {
        match op {
            Op::Constant(idx) => {
                if let Some(v) = constants.get(*idx as usize) {
                    add(*v, &mut out, &mut seen)?;
                }
            }
            Op::CallBuiltinSym(sym, _) => {
                add(Value::symbol(*sym), &mut out, &mut seen)?;
            }
            Op::CallBuiltin(name_idx, _) => {
                if let Some(v) = constants.get(*name_idx as usize) {
                    add(*v, &mut out, &mut seen)?;
                }
            }
            // Dynamic variable access — the symbol operand is session-specific
            // (audit CRITICAL #2). Collect the NORMALIZED plain symbol so the
            // reloc key matches `materialize_op_sym_id`'s `Value::symbol(sym).bits()`
            // (a symbol-with-pos const collapses to its underlying symbol — the var
            // SymId is all the shim needs). Without this the var-symbol wasn't in the
            // reloc vector at all → the lowering had nothing to load → baked the id.
            Op::VarRef(idx) | Op::VarSet(idx) | Op::VarBind(idx) => {
                if let Some(v) = constants.get(*idx as usize) {
                    let sym = v
                        .as_symbol_id()
                        .or_else(|| v.as_symbol_with_pos_sym().and_then(|s| s.as_symbol_id()));
                    if let Some(id) = sym {
                        add(Value::symbol(id), &mut out, &mut seen)?;
                    }
                    // Non-symbol operand (malformed) → leave to the lowering's
                    // const_sym_id, which errors the leaf (caller stays JIT).
                }
            }
            _ => {}
        }
    }
    Some((out, seen))
}

/// Compile one bytecode leaf to a relocatable `.o` for AOT (R1c + sidecar).
///
/// Computes the content hash + entry/descriptor symbols, builds the MIR, checks
/// it is AOT-runnable, collects the reloc consts into a rebuild recipe, emits the
/// entry + descriptor object (with the recipe + frame metadata), and returns
/// `(object_bytes, content_hash)`. Returns `Ok(None)` (NOT an error) when the
/// body is outside the supported subset — the caller stays JIT-only.
///
/// Covers the full MIR pure-tier subset: reloc-bearing (heap consts + interned
/// symbols, via the sidecar + recipe) AND call-bearing (Call/Apply/escaping cons,
/// via the host-exported shims + the sidecar precise-deopt path). Excludes only
/// CallBuiltin*/non-recipe-able consts (rejected upstream).
/// Everything needed to define one AOT leaf into a module: the lowered MIR, its
/// content hash, the entry/descriptor symbol names, and the descriptor bytes.
struct PreparedLeaf {
    m: mir::MirFunction,
    content_hash: u128,
    entry_name: String,
    desc_name: String,
    desc_bytes: Vec<u8>,
}

/// Prepare a leaf for AOT emit (no codegen): content hash, MIR, AOT-runnable
/// check, reloc recipe, frame metadata, descriptor bytes. `Ok(None)` if the body
/// is outside the AOT subset (caller stays JIT-only / skips the candidate).
/// Shared by [`compile_leaf_to_object`] (single-leaf) and [`build_preload_object`]
/// (multi-leaf R2-B4), so both compute the SAME hash/recipe/metadata.
fn prepare_leaf_emit(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
) -> Result<Option<PreparedLeaf>, CompileError> {
    let Some(content_hash) = leaf_content_hash(ops, constants, arity) else {
        return Ok(None); // a constant outside the recipe subset.
    };
    let m = match mir::build_mir(ops, constants, arity) {
        Ok(m) => m,
        Err(_) => return Ok(None), // not MIR-lowerable → JIT-only.
    };
    if !mir_is_aot_runnable(&m) {
        return Ok(None);
    }
    // The MIR tier's baseline-emitter adapter has no AOT parity coverage:
    // keep the AOT MIR population at shim-free bodies plus `Call`/`Apply`.
    if uses_mir_adapter(&m) {
        return Ok(None);
    }
    // MIR loops now poll in the JIT. Keep AOT loops on the existing baseline
    // path until the MIR poll/precise-deopt combination is qualified through
    // a serialized sidecar too.
    let plan = super::compile::lowering::plan_mir_leaf(&m);
    if plan.has_backedge {
        return Ok(None);
    }
    // Reloc consts → rebuild recipe (R1c-3), in the SAME order the lowering
    // assigns reloc indices. Bail if any const is outside the recipe subset.
    let reloc_consts = collect_reloc_consts(&m);
    let mut recipe = Vec::new();
    for &c in &reloc_consts {
        if write_value_recipe(&mut recipe, c).is_err() {
            return Ok(None);
        }
    }
    // Frame metadata from the MIR EXACTLY as lower_mir_pure does (so the loader
    // sizes the per-thread deopt buffers + side-effect flag identically): the
    // same `MirLeafPlan`. A shim-bearing body is ALL-PRECISE deopt +
    // side-effecting → sized deopt_spill.
    let meta = super::compile::AotLeafMeta {
        arity: m.arity,
        required: m.arity,
        has_rest: false,
        has_binds: false,
        has_handlers: false,
        has_side_effects: plan.precise,
        max_depth: if plan.precise { plan.max_depth } else { 0 },
        has_precise_deopt: plan.precise,
    };
    // The MIR tier never bakes `Op::Call` subr/bytecode spec sites (that pass runs
    // only under `Some(obarray)` at the baseline tier — increment B2), so it always
    // emits an empty spec-section.
    let desc_bytes = encode_descriptor(&meta, &recipe, reloc_consts.len() as u32, &[]);
    Ok(Some(PreparedLeaf {
        m,
        content_hash,
        entry_name: aot_entry_symbol(content_hash),
        desc_name: aot_descriptor_symbol(content_hash),
        desc_bytes,
    }))
}

pub(crate) fn compile_leaf_to_object(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    // R2 increment B2: the LIVE obarray. `Some` with an `Op::Call` spec site forces
    // the BASELINE tier (only it bakes the spec fast paths); `None` (tests/testkits)
    // keeps the MIR-first routing unchanged.
    obarray: Option<&crate::emacs_core::symbol::Obarray>,
) -> Result<Option<(Vec<u8>, u128)>, CompileError> {
    // TIER-PIVOT (increment B2): a body with ≥1 `Op::Call` subr/bytecode spec site
    // MUST emit via the BASELINE tier — the MIR tier never bakes those spec fast
    // paths, and the LOAD path (`live_reloc_for_emit_tier`) mirrors this so both
    // agree on the reloc collector (else the leaf silently never serves). Only when
    // NOT spec-forced do we try the (faster) MIR tier first.
    let spec_forced =
        obarray.is_some_and(|ob| super::compile::has_op_call_spec_sites(ops, constants, arity, ob));
    if !spec_forced {
        // MIR tier first (the existing pure/reloc/call-bearing subset).
        if let Some(p) = prepare_leaf_emit(ops, constants, arity)? {
            // build_object_for_leaf_inner runs the #15 shim-import audit on the bytes.
            let obj = build_object_for_leaf_inner(
                &p.m,
                &p.entry_name,
                Some((&p.desc_name, &p.desc_bytes)),
            )?;
            return Ok(Some((obj, p.content_hash)));
        }
    }
    // R2-E (b/d): the BASELINE tier for a body the MIR tier rejects (Switch/Throw/
    // handlers, or CallBuiltin(Sym)/VarRef Opaque-lowering errors) OR a spec-forced
    // body — BUT only for the conservatively-allowlisted op set whose AOT-deopt
    // soundness we've validated (Q2). Outside the allowlist → stay JIT-only.
    if !baseline_is_aot_runnable(ops) {
        return Ok(None);
    }
    build_baseline_object_for_leaf(ops, constants, arity, obarray)
}

/// R2-E (Q2): the CONSERVATIVE allowlist of ops the baseline-tier AOT path may
/// emit. Admits ONLY op-classes whose AOT-via-baseline soundness (esp. the new
/// sidecar deopt-resume + the named-builtin op-SymId reloc) is covered by a test.
/// Everything not listed → the leaf stays JIT-only. INCREMENTAL (Q1): this E1b set
/// adds the Category-2 ops (CallBuiltin(Sym)/VarRef/VarSet/VarBind/Unbind + the
/// pure/arith/list/string baseline ops); the Category-1 control ops
/// (Switch/Throw/condition-case/catch handlers) are DEFERRED to a follow-up and
/// remain rejected here. NB: any rejected op anywhere in the body bails the whole
/// leaf (an allowlist, not a per-op filter).
fn baseline_is_aot_runnable(ops: &[Op]) -> bool {
    ops.iter().all(|op| {
        matches!(
            op,
            // Stack / constants / control (no handlers/switch/throw yet).
            Op::Constant(_)
                | Op::Nil
                | Op::True
                | Op::Pop
                | Op::Dup
                | Op::StackRef(_)
                | Op::StackSet(_)
                | Op::DiscardN(_)
                | Op::Goto(_)
                | Op::GotoIfNil(_)
                | Op::GotoIfNotNil(_)
                | Op::GotoIfNilElsePop(_)
                | Op::GotoIfNotNilElsePop(_)
                | Op::Return
                // Arithmetic / comparison (the fixnum-guard deopt surface).
                | Op::Add
                | Op::Sub
                | Op::Mul
                | Op::Div
                | Op::Rem
                | Op::Add1
                | Op::Sub1
                | Op::Negate
                | Op::Eqlsign
                | Op::Gtr
                | Op::Lss
                | Op::Leq
                | Op::Geq
                | Op::Max
                | Op::Min
                // List / type-predicate / string / vector ops.
                | Op::Car
                | Op::Cdr
                | Op::Cons
                | Op::List(_)
                | Op::Length
                | Op::Nth
                | Op::Nthcdr
                | Op::Setcar
                | Op::Setcdr
                | Op::CarSafe
                | Op::CdrSafe
                | Op::Elt
                | Op::Memq
                | Op::Member
                | Op::Assq
                | Op::Symbolp
                | Op::Consp
                | Op::Stringp
                | Op::Listp
                | Op::Integerp
                | Op::Numberp
                | Op::Null
                | Op::Not
                | Op::Eq
                | Op::Equal
                | Op::Concat(_)
                | Op::Substring
                | Op::StringEqual
                | Op::StringLessp
                | Op::Aref
                | Op::Aset
                // Variables + the named-builtin escape hatch (the Category-2 unlock;
                // CallBuiltinSym's op-SymId is reloc'd by name — must-nail #2).
                | Op::VarRef(_)
                | Op::VarSet(_)
                | Op::VarBind(_)
                | Op::Unbind(_)
                | Op::Call(_)
                | Op::Apply(_)
                | Op::CallBuiltin(..)
                | Op::CallBuiltinSym(..)
        )
    })
}

/// Stats from a multi-leaf preload build: how many candidates collapsed to how
/// many unique objects (logged so dedup is never a silent drop).
#[derive(Debug, Clone, Copy, Default)]
pub struct PreloadBuildStats {
    /// Candidates offered to the builder.
    pub candidates: usize,
    /// Candidates that prepared successfully (in the AOT subset).
    pub prepared: usize,
    /// DISTINCT content hashes actually emitted into the object.
    pub unique_emitted: usize,
    /// Candidates dropped because they were outside the AOT subset.
    pub skipped_unsupported: usize,
    /// Candidates collapsed onto an already-emitted identical body (dedup).
    pub deduped: usize,
}

/// R2-B4: build ONE relocatable object containing ALL the given leaves (the
/// dump-time `libneomacs-preload.so` payload, pre-link).
///
/// DEDUP BY CONTENT-HASH (team-lead): distinct loadup functions can share an
/// IDENTICAL body (trivial accessors / macro-generated defuns) → identical
/// content hash → identical entry+descriptor symbol names. Emitting both into one
/// `ObjectModule` would be a DUPLICATE-SYMBOL collision. So each unique hash is
/// emitted ONCE; every function with that body binds the same entry at load (the
/// native code is identical, so it serves all of them). Returns the object bytes
/// + [`PreloadBuildStats`] (no silent drops — candidates/unique/deduped/skipped).
pub fn build_preload_object(
    leaves: &[LoadupLeaf],
    // R2 increment B2: the LIVE (dump-time) obarray, so spec-bearing loadup bodies
    // bake their `Op::Call` spec fast paths + descriptor entries. `None` → CBSym-only.
    obarray: Option<&crate::emacs_core::symbol::Obarray>,
) -> Result<(Vec<u8>, PreloadBuildStats), CompileError> {
    let mut module = make_aot_object_module()?;
    let mut seen: std::collections::HashSet<u128> = std::collections::HashSet::new();
    let mut stats = PreloadBuildStats {
        candidates: leaves.len(),
        ..Default::default()
    };
    for leaf in leaves {
        // TIER-PIVOT (increment B2): a spec-bearing body MUST go BASELINE (only it
        // bakes the `Op::Call` spec fast paths; the load path mirrors this). Try the
        // MIR tier first ONLY when not spec-forced (same routing as
        // compile_leaf_to_object).
        let spec_forced = obarray.is_some_and(|ob| {
            super::compile::has_op_call_spec_sites(leaf.ops, leaf.constants, leaf.arity, ob)
        });
        if !spec_forced {
            // MIR tier first (the existing pure/reloc/call-bearing subset).
            if let Some(p) = prepare_leaf_emit(leaf.ops, leaf.constants, leaf.arity)? {
                stats.prepared += 1;
                if !seen.insert(p.content_hash) {
                    // Identical body already emitted — its entry/descriptor symbols
                    // are shared; re-defining them would collide. Skip (same code).
                    stats.deduped += 1;
                    continue;
                }
                define_leaf_into_module(
                    &mut module,
                    &p.m,
                    &p.entry_name,
                    Some((&p.desc_name, &p.desc_bytes)),
                )?;
                stats.unique_emitted += 1;
                continue;
            }
        }
        // R2-E: BASELINE tier for a MIR-rejected (or spec-forced) body in the
        // conservative allowlist (mirrors compile_leaf_to_object's routing, so the
        // producer's emitted set == the is_d0_aot_candidate set — no gap).
        if !baseline_is_aot_runnable(leaf.ops) {
            stats.skipped_unsupported += 1;
            continue;
        }
        let Some(content_hash) = leaf_content_hash(leaf.ops, leaf.constants, leaf.arity) else {
            stats.skipped_unsupported += 1;
            continue;
        };
        stats.prepared += 1;
        if !seen.insert(content_hash) {
            stats.deduped += 1;
            continue;
        }
        let entry_name = aot_entry_symbol(content_hash);
        let desc_name = aot_descriptor_symbol(content_hash);
        if define_baseline_leaf_into_module(
            &mut module,
            leaf.ops,
            leaf.constants,
            leaf.arity,
            &entry_name,
            &desc_name,
            obarray,
        )?
        .is_none()
        {
            // A non-recipe-able const/symbol slipped past the allowlist → skip.
            seen.remove(&content_hash);
            stats.prepared -= 1;
            stats.skipped_unsupported += 1;
            continue;
        }
        stats.unique_emitted += 1;
    }
    let obj = module
        .finish()
        .emit()
        .map_err(|e| module_init_err(e.to_string()))?;
    assert_aot_imports_exported(&obj)?;
    Ok((obj, stats))
}

/// File name of the dump-time AOT preload shared object (beside the pdump).
pub const PRELOAD_SO_NAME: &str = "libneomacs-preload.so";
/// File name of the preload manifest (fingerprint interlock + per-name pre-keys).
pub const PRELOAD_MANIFEST_NAME: &str = "libneomacs-preload.manifest";
/// Manifest format version (bump on any manifest schema change).
///
/// v2 (task #11): the per-unique-hash `hash …` diagnostic lines were replaced by
/// per-NAME `leaf …` pre-key lines (see [`manifest_leaf_line`]) so the startup
/// prepopulate pass can resolve membership by symbol name WITHOUT paying the
/// SHA-256 content hash for every loadup candidate. A version-1 manifest fails
/// the interlock and is treated as ABSENT-preload (skip→JIT) — the simpler sound
/// compat arm: manifests are co-produced with the `.so` on every fresh-build and
/// pinned to the pdump by the fingerprint interlock, so a live v1 manifest is
/// stale by construction.
const PRELOAD_MANIFEST_VERSION: u32 = 2;

/// One parsed v2 manifest pre-key (task #11): the cheap per-NAME discriminators
/// the prepopulate pass consults BEFORE paying the SHA-256 content hash.
/// `member` distinguishes the emitted preload set (`m` lines) from dump-time
/// hashable NON-members (`x` lines — the skip class: a verified `x` key means
/// the dlsym membership probe would miss, so the hash can be skipped outright).
/// `hash` is the dump-time content hash (diagnostic: the runtime always
/// recomputes the LIVE body's hash before consulting the dlsym gate, which stays
/// the membership ground truth; the recorded hash only feeds drift tracing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ManifestPreKey {
    pub(crate) member: bool,
    pub(crate) ops_len: usize,
    pub(crate) arity: usize,
    pub(crate) hash: u128,
}

/// Symbol-name → pre-key map parsed from a v2 preload manifest.
pub(crate) type PreKeyMap = std::collections::HashMap<Box<str>, ManifestPreKey>;

/// Escape a symbol name into a single whitespace-free manifest token. Names are
/// emitted RAW unless empty, `%`-leading, or containing whitespace/control
/// characters (which would corrupt the line-based format) — those are emitted as
/// `%` + lowercase hex of the UTF-8 bytes. Unambiguous: a raw emission never
/// starts with `%`, so the loader unescapes exactly the `%`-leading tokens.
fn manifest_escape_name(name: &str) -> std::borrow::Cow<'_, str> {
    let needs_escape = name.is_empty()
        || name.starts_with('%')
        || name.chars().any(|c| c.is_whitespace() || c.is_control());
    if !needs_escape {
        return std::borrow::Cow::Borrowed(name);
    }
    use std::fmt::Write as _;
    let mut out = String::with_capacity(1 + name.len() * 2);
    out.push('%');
    for b in name.bytes() {
        let _ = write!(out, "{b:02x}");
    }
    std::borrow::Cow::Owned(out)
}

/// Inverse of [`manifest_escape_name`]. `None` on a malformed escape (the caller
/// treats the whole pre-key section as malformed — fail-closed to the no-filter
/// path).
fn manifest_unescape_name(tok: &str) -> Option<String> {
    let Some(hex) = tok.strip_prefix('%') else {
        return Some(tok.to_string());
    };
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        bytes.push(u8::from_str_radix(hex.get(i..i + 2)?, 16).ok()?);
    }
    String::from_utf8(bytes).ok()
}

/// Render ONE v2 pre-key line: `leaf <m|x> <ops_len> <arity> <hash> <name>\n`.
/// The single source for the line format — the parser ([`parse_preload_manifest`])
/// and the producer ([`build_and_link_preload`]) both go through it / its tests.
fn manifest_leaf_line(
    member: bool,
    ops_len: usize,
    arity: usize,
    hash: u128,
    name: &str,
) -> String {
    format!(
        "leaf {} {ops_len} {arity} {hash:032x} {}\n",
        if member { 'm' } else { 'x' },
        manifest_escape_name(name),
    )
}

/// A parsed preload manifest: the interlock header fields plus (v2) the pre-key
/// map. `prekeys` is `None` when the pre-key section is absent/malformed
/// (truncated file, corrupt line, duplicate name, `leaves` count mismatch) —
/// the FAIL-CLOSED signal: the prepopulate pass then hashes every candidate
/// exactly as before (the pre-filter is strictly an optimization; a manifest
/// that fails only its pre-key section still serves the `.so` via the header
/// interlock).
struct ParsedPreloadManifest {
    version: Option<u32>,
    abi_tag: Option<u32>,
    fingerprint: Option<String>,
    prekeys: Option<PreKeyMap>,
}

/// Parse a preload manifest's text. Unknown line tags are ignored (forward
/// compatibility; also tolerates v1 `hash …` diagnostic lines). See
/// [`ParsedPreloadManifest`] for the fail-closed `prekeys` contract.
fn parse_preload_manifest(text: &str) -> ParsedPreloadManifest {
    let mut version: Option<u32> = None;
    let mut abi_tag: Option<u32> = None;
    let mut fingerprint: Option<String> = None;
    let mut declared_leaves: Option<usize> = None;
    let mut map: PreKeyMap = PreKeyMap::new();
    let mut malformed = false;
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let Some(tag) = it.next() else { continue };
        match tag {
            "version" => version = it.next().and_then(|v| v.parse().ok()),
            "abi_tag" => abi_tag = it.next().and_then(|v| u32::from_str_radix(v, 16).ok()),
            "fingerprint" => fingerprint = it.next().map(str::to_string),
            "leaves" => declared_leaves = it.next().and_then(|v| v.parse().ok()),
            "leaf" => {
                let parsed = (|| {
                    let member = match it.next()? {
                        "m" => true,
                        "x" => false,
                        _ => return None,
                    };
                    let ops_len: usize = it.next()?.parse().ok()?;
                    let arity: usize = it.next()?.parse().ok()?;
                    let hash = u128::from_str_radix(it.next()?, 16).ok()?;
                    let name = manifest_unescape_name(it.next()?)?;
                    if it.next().is_some() {
                        return None; // trailing junk → malformed line.
                    }
                    Some((
                        name,
                        ManifestPreKey {
                            member,
                            ops_len,
                            arity,
                            hash,
                        },
                    ))
                })();
                match parsed {
                    // A duplicate name is a producer impossibility (obarray names
                    // are unique) → treat as corruption.
                    Some((name, key)) => {
                        if map.insert(name.into_boxed_str(), key).is_some() {
                            malformed = true;
                        }
                    }
                    None => malformed = true,
                }
            }
            _ => {} // unknown/diagnostic lines: ignore.
        }
    }
    // The declared count must cover the parsed map exactly — catches truncation
    // between the header and the tail of the pre-key list.
    let prekeys = (!malformed && declared_leaves == Some(map.len())).then_some(map);
    ParsedPreloadManifest {
        version,
        abi_tag,
        fingerprint,
        prekeys,
    }
}

/// The STALE INTERLOCK on a parsed manifest: `version`, `abi_tag`, and
/// `fingerprint` must all match the RUNNING build, else the whole preload is
/// skipped (→ JIT) — a foreign / stale / ABI-incompatible preload never
/// mis-serves native code. Mismatches are logged at debug.
fn manifest_interlock_ok(parsed: &ParsedPreloadManifest) -> bool {
    if parsed.version != Some(PRELOAD_MANIFEST_VERSION) {
        tracing::debug!(
            "aot-preload: manifest version mismatch ({:?}); skip→JIT",
            parsed.version
        );
        return false;
    }
    if parsed.abi_tag != Some(ABI_TAG) {
        tracing::debug!(
            "aot-preload: manifest abi_tag mismatch ({:?}); skip→JIT",
            parsed.abi_tag
        );
        return false;
    }
    // The interlock: the manifest fingerprint must equal the RUNNING pdump's.
    let running = crate::emacs_core::pdump::fingerprint_hex();
    if parsed.fingerprint.as_deref() != Some(running) {
        tracing::debug!(
            "aot-preload: manifest fingerprint mismatch (manifest={:?} running={running}); skip→JIT",
            parsed.fingerprint
        );
        return false;
    }
    true
}

/// R2-B6: enumerate `ctx`'s loadup AOT candidates, build ONE preload object,
/// link it to `out_dir/libneomacs-preload.so`, and write
/// `out_dir/libneomacs-preload.manifest`. The manifest carries the running
/// pdump's `fingerprint_hex` (the STALE INTERLOCK — the loader refuses a `.so`
/// whose manifest fingerprint ≠ the running pdump, so a foreign/stale preload is
/// a clean skip→JIT, never a crash), the ABI_TAG, the manifest version, and (v2,
/// task #11) one PRE-KEY line per hashable required-only loadup fn: NAME +
/// ops-count + arity + content hash, classed `m` (emitted preload member) or `x`
/// (hashable non-member). The `x` class is what the startup prepopulate pass
/// skips WITHOUT hashing (previously it paid a SHA-256 for all ~2195 candidates
/// to discover the ~1489 non-members); `m` lines double as the name-attributed
/// replacement for v1's anonymous per-hash diagnostic listing. Dedup'd bodies
/// (distinct names, identical body) each get their own `m` line sharing the
/// hash. `ctx` is the final pdump loaded in-process (the loadup closure).
/// Returns the build stats.
pub fn build_and_link_preload(
    ctx: &crate::emacs_core::eval::Context,
    out_dir: &std::path::Path,
) -> Result<PreloadBuildStats, CompileError> {
    // Enumerate WITHOUT the D0 filter so the manifest can carry a pre-key for
    // EVERY hashable candidate (member and non-member), then partition by the
    // same `is_d0_aot_candidate` gate the v1 path applied inside enumerate —
    // same emitted set, one full-compile probe per fn instead of v1's two.
    let all = enumerate_loadup_leaves(ctx, /*d0_filter=*/ false);
    let mut members: Vec<LoadupLeaf> = Vec::new();
    let mut prekey_lines = String::new();
    let mut prekey_count = 0usize;
    for leaf in all {
        // Unhashable body (non-canonical op / non-recipe-able const): no
        // pre-key. The runtime then takes the exact pre-v2 path for that fn
        // (a failed hash attempt → skip), preserving behavior + counts.
        let Some(hash) = leaf_content_hash(leaf.ops, leaf.constants, leaf.arity) else {
            continue;
        };
        let member = is_d0_aot_candidate(leaf.ops, leaf.constants, leaf.arity, Some(&ctx.obarray));
        prekey_lines.push_str(&manifest_leaf_line(
            member,
            leaf.ops.len(),
            leaf.arity,
            hash,
            &leaf.name,
        ));
        prekey_count += 1;
        if member {
            members.push(leaf);
        }
    }
    // B2: emit WITH the dump-time obarray so spec-bearing loadup bodies bake their
    // `Op::Call` spec fast paths + descriptor entries (armed at load = native FAST
    // from call 1). The runtime prepopulate re-classifies against the same (same
    // pdump) obarray, so the classification matches by construction.
    let (obj, stats) = build_preload_object(&members, Some(&ctx.obarray))?;
    let so_path = out_dir.join(PRELOAD_SO_NAME);
    link_object_to_so(&obj, &so_path)?;

    let mut manifest = String::new();
    manifest.push_str(&format!("version {PRELOAD_MANIFEST_VERSION}\n"));
    manifest.push_str(&format!("abi_tag {ABI_TAG:08x}\n"));
    manifest.push_str(&format!(
        "fingerprint {}\n",
        crate::emacs_core::pdump::fingerprint_hex()
    ));
    manifest.push_str(&format!("leaves {prekey_count}\n"));
    manifest.push_str(&prekey_lines);
    let manifest_path = out_dir.join(PRELOAD_MANIFEST_NAME);
    std::fs::write(&manifest_path, manifest)
        .map_err(|e| module_init_err(format!("write preload manifest: {e}")))?;
    Ok(stats)
}

/// Env var that ENABLES the dump-time preload producer (set by
/// `cargo xtask fresh-build --aot-preload` on the `--temacs=pdump` invocation).
pub const PRELOAD_ENABLE_ENV: &str = "NEOVM_AOT_PRELOAD";
/// Env var that puts the producer in DRY-RUN mode: enumerate + log candidates +
/// dedup stats, but do NOT link/write the `.so`/manifest.
pub const PRELOAD_DRY_RUN_ENV: &str = "NEOVM_AOT_PRELOAD_DRY_RUN";

/// R2-B1 (resolution B): the dump-time preload hook, called from
/// `builtin_dump_emacs_portable` right after the FINAL pdump is written, with
/// `ctx` = the live loadup closure (the #A eq-identity source) and `dump_dir` =
/// the directory the pdump landed in. Builds `libneomacs-preload.so` + manifest
/// beside the pdump so the runtime serves native from call 1.
///
/// Runs ONLY when [`PRELOAD_ENABLE_ENV`] is set (so ordinary
/// `dump-emacs-portable` calls — every test's, every plain dump — pay nothing).
/// In [`PRELOAD_DRY_RUN_ENV`] mode it only enumerates + logs (no link/write).
///
/// Because this runs IN the neomacs dump process, it owns the patched pdump
/// fingerprint slot + the live obarray, so the emitted `.so`'s content-hashes and
/// the manifest fingerprint match the runtime BY CONSTRUCTION. Failures are
/// LOGGED and swallowed (never abort the dump): a missing preload is an additive
/// miss → the runtime just JITs, honoring the off-by-default contract.
pub fn run_dump_time_preload(ctx: &crate::emacs_core::eval::Context, dump_dir: &std::path::Path) {
    if std::env::var_os(PRELOAD_ENABLE_ENV).is_none() {
        return;
    }
    let dry_run = std::env::var_os(PRELOAD_DRY_RUN_ENV).is_some();

    if dry_run {
        let leaves = enumerate_loadup_leaves(ctx, /*d0_filter=*/ true);
        match build_preload_object(&leaves, Some(&ctx.obarray)) {
            Ok((_, stats)) => {
                tracing::info!(
                    "aot-preload DRY-RUN: candidates={} prepared={} unique_emitted={} \
                     deduped={} skipped_unsupported={}",
                    stats.candidates,
                    stats.prepared,
                    stats.unique_emitted,
                    stats.deduped,
                    stats.skipped_unsupported,
                );
                for leaf in leaves.iter().take(40) {
                    tracing::info!(
                        "aot-preload candidate: {} (arity={})",
                        leaf.name,
                        leaf.arity
                    );
                }
                if leaves.len() > 40 {
                    tracing::info!("aot-preload: ... and {} more candidates", leaves.len() - 40);
                }
            }
            Err(e) => tracing::warn!("aot-preload DRY-RUN build_preload_object failed: {e}"),
        }
        return;
    }

    match build_and_link_preload(ctx, dump_dir) {
        Ok(stats) => tracing::info!(
            "aot-preload: emitted {}/{} beside pdump in {} (candidates={} prepared={} \
             unique_emitted={} deduped={} skipped_unsupported={})",
            PRELOAD_SO_NAME,
            PRELOAD_MANIFEST_NAME,
            dump_dir.display(),
            stats.candidates,
            stats.prepared,
            stats.unique_emitted,
            stats.deduped,
            stats.skipped_unsupported,
        ),
        Err(e) => tracing::warn!(
            "aot-preload: build_and_link_preload failed ({e}); runtime will JIT (additive miss)"
        ),
    }
}

/// Audit #15 + #32-audit: assert every UNDEFINED `neovm_jit_*` import in an
/// emitted AOT object is in [`MIR_SHIM_NAMES`] — the SINGLE-SOURCE exported set
/// (`shim_names.rs`, salted into `ABI_TAG` + exported by both build.rs files). A
/// shim outside it would not re-tag stale `.so`s on an ABI change AND would not
/// resolve at dlopen (an unexported import). FAILS CLOSED: this is now a HARD
/// (non-debug) emit-time check returning `Err` — the #32 audit flagged that the
/// old debug-only assert wouldn't catch a release-build widening that emits an
/// unexported shim. So a future lowering that emits an unsalted/unexported import
/// errors the emit (→ JIT) instead of producing a `.so` that aborts at dlopen.
fn assert_aot_imports_exported(obj: &[u8]) -> Result<(), CompileError> {
    use object::{Object, ObjectSymbol};
    let file = object::File::parse(obj)
        .map_err(|e| module_init_err(format!("parse emitted AOT object: {e}")))?;
    for sym in file.symbols() {
        if sym.is_undefined()
            && let Ok(name) = sym.name()
            && name.starts_with("neovm_jit_")
            && !MIR_SHIM_NAMES.contains(&name)
        {
            return Err(module_init_err(format!(
                "AOT object imports shim {name:?} not in the exported MIR_SHIM_NAMES \
                 (shim_names.rs) — salt it (compute_abi_tag) + export it (both build.rs)"
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// R1c-5: link `.o` → `.so`, load via libloading.
// R1c-7: the content-hash-keyed unit store (from NEOVM_AOT_DIR).
// R1c-6: `try_load_leaf` — the cache's AOT consult.
// ---------------------------------------------------------------------------

/// Link a relocatable object's bytes into a shared object at `so_path`, via
/// `cc -shared`. The host process must export the `neovm_jit_*` shims (linked
/// `-rdynamic`) so the loader can bind the `.so`'s undefined imports at dlopen.
pub(crate) fn link_object_to_so(
    obj_bytes: &[u8],
    so_path: &std::path::Path,
) -> Result<(), CompileError> {
    use std::io::Write;
    // Write the `.o` beside the target `.so` (same dir; temp name).
    let o_path = so_path.with_extension("o");
    std::fs::File::create(&o_path)
        .and_then(|mut f| f.write_all(obj_bytes))
        .map_err(|e| module_init_err(format!("write .o: {e}")))?;
    let status = std::process::Command::new("cc")
        .arg("-shared")
        .arg("-o")
        .arg(so_path)
        .arg(&o_path)
        .status()
        .map_err(|e| module_init_err(format!("spawn cc: {e}")))?;
    // Best-effort cleanup of the intermediate object.
    let _ = std::fs::remove_file(&o_path);
    if !status.success() {
        return Err(module_init_err(format!("cc -shared failed: {status}")));
    }
    Ok(())
}

/// Test-only: compile + link a leaf and place its `.so` into `dir` under the
/// unit-index naming convention (`{hash:032x}_{tag:08x}.so`), so an INTEGRATION
/// test can exercise the real `NEOVM_AOT=force` + `NEOVM_AOT_DIR` production path
/// (the lib's `test_support` seam is unavailable to integration tests, and the
/// `-rdynamic` shim export only applies to test binaries — see build.rs).
/// Returns the content hash on success, `None` if the body is outside the AOT
/// subset. `#[doc(hidden)]`: composes the already-`pub`/`pub(crate)` emit fns;
/// not a stable API.
#[doc(hidden)]
pub fn testkit_emit_and_place_so(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    dir: &std::path::Path,
) -> Option<u128> {
    // These low-level testkits emit WITHOUT a live obarray (no `Op::Call` spec
    // baking); the spec-aware selftests call the emit fns with an obarray directly.
    let (obj, content_hash) = compile_leaf_to_object(ops, constants, arity, None).ok()??;
    let so_path = dir.join(format!("{content_hash:032x}_{ABI_TAG:08x}.so"));
    link_object_to_so(&obj, &so_path).ok()?;
    Some(content_hash)
}

/// R2-E test seam: emit a body via the BASELINE tier AOT path + place its `.so`
/// in `dir` (unit-index naming), so an integration test exercises the real
/// `NEOVM_AOT=force` + `NEOVM_AOT_DIR` production serve of a BASELINE leaf.
#[doc(hidden)]
#[cfg(target_os = "linux")]
pub fn testkit_emit_baseline_and_place_so(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    dir: &std::path::Path,
) -> Option<u128> {
    let (obj, content_hash) =
        build_baseline_object_for_leaf(ops, constants, arity, None).ok()??;
    let so_path = dir.join(format!("{content_hash:032x}_{ABI_TAG:08x}.so"));
    link_object_to_so(&obj, &so_path).ok()?;
    Some(content_hash)
}

// ---------------------------------------------------------------------------
// R2: dump-time loadup AOT producer (the v1 deliverable). The loaded final pdump
// IS the loadup closure, so the candidate set = its obarray's bytecode-bound,
// required-only, AOT-emittable functions (D0-only: no PGO/spec/inline).
// ---------------------------------------------------------------------------

/// One loadup function eligible for dump-time AOT: its name (for diagnostics +
/// the content-hash inputs are ops/constants/arity) and the bytecode source the
/// emitter needs. Borrowed from the obarray-held `ByteCodeFunction` (`'static`
/// via the heap), so this carries refs, not copies.
pub struct LoadupLeaf {
    pub name: String,
    pub ops: &'static [Op],
    pub constants: &'static [Value],
    /// Required-arg count = the AOT native arity (candidates are required-only).
    pub arity: usize,
}

/// R2-B2: enumerate the AOT-candidate leaves of a loaded image (`ctx` = the final
/// pdump loaded in-process). Walks the obarray for globally-interned symbols
/// whose function cell holds a bytecode object, keeps the REQUIRED-ONLY ones
/// (the MIR pure-tier arity shape: no `&optional`/`&rest`), and — when
/// `d0_filter` — keeps only those the AOT emitter actually accepts (R2-B3:
/// `compile_leaf_to_object` Some ⇒ MIR-lowerable, AOT-runnable, recipe-able).
///
/// The D0 gate compiles with the LIVE obarray (increment B2), so a spec-bearing
/// body is admitted via the same BASELINE tier the preload producer uses (matching
/// emitted sets). Returns the candidates in obarray order (deterministic per image).
pub fn enumerate_loadup_leaves(
    ctx: &crate::emacs_core::eval::Context,
    d0_filter: bool,
) -> Vec<LoadupLeaf> {
    let mut out = Vec::new();
    for name in ctx.obarray.all_symbols() {
        let id = crate::emacs_core::intern::intern(name);
        let Some(func_val) = ctx.obarray.symbol_function_id(id) else {
            continue;
        };
        if !func_val.is_bytecode() {
            continue;
        }
        let Some(bc) = func_val.get_bytecode_data() else {
            continue;
        };
        // Required-only: matches the MIR pure tier's native-arity seeding (the AOT
        // subset). &optional/&rest functions are not AOT candidates here.
        if !bc.params.optional.is_empty() || bc.params.rest.is_some() {
            continue;
        }
        let arity = bc.params.required.len();
        let ops = bc.executable_ops();
        if d0_filter && !is_d0_aot_candidate(ops, &bc.constants, arity, Some(&ctx.obarray)) {
            continue;
        }
        out.push(LoadupLeaf {
            name: name.to_string(),
            ops,
            constants: &bc.constants,
            arity,
        });
    }
    out
}

/// R2-B3: whether a required-only leaf is a dump-time D0 AOT candidate — i.e. the
/// AOT emitter accepts it (`compile_leaf_to_object` returns `Some`, meaning it is
/// MIR-lowerable, passes `mir_is_aot_runnable`, and every const is recipe-able).
/// This is the SAME gate the runtime load path uses, so a candidate emitted here
/// will load there.
pub fn is_d0_aot_candidate(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    // R2 increment B2: the dump-time obarray, so the candidate gate matches the
    // spec-forced tier the preload producer uses (no "candidate but not emitted" gap).
    obarray: Option<&crate::emacs_core::symbol::Obarray>,
) -> bool {
    matches!(
        compile_leaf_to_object(ops, constants, arity, obarray),
        Ok(Some(_))
    )
}

/// Reconstruct an aliased virtual cons through an emitted object's sidecar,
/// then resume after an observable call. This exercises the serialized
/// precise-frame metadata and the imported allocator together.
#[cfg(target_os = "linux")]
#[doc(hidden)]
pub fn testkit_mir_cons_reconstruction_selftest(dir: &std::path::Path) {
    testkit_mir_cons_reconstruction_case(dir, false);
    testkit_mir_cons_reconstruction_case(dir, true);
}

#[cfg(target_os = "linux")]
fn testkit_mir_cons_reconstruction_case(dir: &std::path::Path, singleton: bool) {
    use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::jit::compile::{DeoptResume, LoadedUnit, NativeRun};
    use crate::emacs_core::value::LambdaParams;

    let mut ev = Context::new();
    ev.eval_str("(setq aot-rebuild-count 0) (fset 'aot-rebuild-effect (lambda () (setq aot-rebuild-count (1+ aot-rebuild-count))))").unwrap();
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![
            crate::emacs_core::intern::SymId(1),
            crate::emacs_core::intern::SymId(2),
        ],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.max_stack = 16;
    f.constants = vec![Value::symbol("aot-rebuild-effect")].into();
    f.ops = vec![
        Op::Constant(0),
        Op::Call(0),
        Op::Pop,
        Op::StackRef(1),
        Op::Add1,
        Op::StackRef(1),
        Op::Cons,
        Op::Dup,
        Op::StackRef(2),
        Op::Add1,
        Op::Pop,
        Op::Car,
        Op::Pop,
        Op::Car,
        Op::Return,
    ];
    if singleton {
        f.ops.splice(5..7, [Op::List(1)]);
    }
    f.seal_hand_assembled_ops();
    let m = mir::build_mir(&f.ops, &f.constants, 2).unwrap();
    assert_eq!(
        super::compile::plan_mir_leaf(&m)
            .cons_repl
            .iter()
            .filter(|c| c.is_some())
            .count(),
        1
    );
    let (obj, hash) = compile_leaf_to_object(&f.ops, &f.constants, 2, None)
        .unwrap()
        .unwrap();
    let path = dir.join(if singleton {
        "singleton-reconstruction.so"
    } else {
        "cons-reconstruction.so"
    });
    link_object_to_so(&obj, &path).unwrap();
    // SAFETY: this is the object just emitted by our compiler; its runtime
    // imports are the same exported shims used by the JIT.
    let library = unsafe { libloading::Library::new(&path) }.unwrap();
    let unit = std::sync::Arc::new(LoadedUnit::new(library));
    let leaf = load_leaf_from_unit(&unit, hash, 2, &collect_reloc_consts(&m), None).unwrap();
    let float = ev.eval_str("1.5").unwrap();
    let result = leaf.call(
        &mut ev as *mut Context as *mut u8,
        &[Value::make_int(7), float],
    );
    let NativeRun::DeoptAt(resume) = result else {
        panic!("precise AOT deopt expected: {result:?}")
    };
    assert_eq!(resume.pc, if singleton { 8 } else { 9 });
    assert_eq!(resume.stack[2], resume.stack[3]);
    assert_eq!(resume.stack[2].cons_car(), Value::make_int(8));
    assert_eq!(
        resume.stack[2].cons_cdr(),
        if singleton { Value::NIL } else { float }
    );
    let DeoptResume {
        pc,
        stack,
        handlers,
        binds,
        spec_base,
        cond_base,
    } = *resume;
    let result = Vm::from_context(&mut ev)
        .run_resumed_frame(
            &f,
            Value::NIL,
            pc,
            &stack,
            handlers,
            &binds,
            spec_base,
            cond_base,
        )
        .unwrap();
    assert_eq!(result, Value::make_int(8));
    assert_eq!(
        ev.eval_str("aot-rebuild-count").unwrap(),
        Value::make_int(1)
    );
    assert_eq!(ev.jit_root_stack_top, 0);
}

/// Crate-internal self-test for CALL-BEARING AOT, invoked from the
/// `tests/aot_call_bearing.rs` integration test (which runs in a shim-exporting
/// `-rdynamic` binary; see build.rs). It needs crate-private types (obarray, Vm,
/// ByteCodeFunction internals), so the logic lives here and the integration test
/// is a thin env-setting wrapper. Returns `Err(reason)` on any check failure.
///
/// Proves, end-to-end through the PUBLIC `try_run_compiled` under
/// `NEOVM_AOT=force` + `NEOVM_AOT_DIR` = `dir`:
///  1. a call-bearing leaf serves FROM AOT and matches the expected result;
///  2. SIDE-EFFECT-EXACTLY-ONCE across a forced precise-deopt: the call's
///     observable side effect runs once (resume-at-pc, NOT rerun-from-start);
///  3. AOT == interp for both the result and the side-effect count;
///  4. (#A) a heap constant the leaf returns is EQ-identical to the source const.
#[doc(hidden)]
#[cfg(target_os = "linux")]
pub fn testkit_call_bearing_selftest(dir: &std::path::Path) -> Result<(), String> {
    use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    use crate::emacs_core::value::{LambdaParams, ValueKind};

    if !aot_enabled() {
        return Err("NEOVM_AOT not enabled (env must be set before any AOT call)".into());
    }

    let mk_fn = |ops: Vec<Op>, constants: Vec<Value>, required: usize| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: (0..required).map(|i| SymId(1 + i as u32)).collect(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops;
        f.constants = constants.into();
        f.max_stack = 32;
        f.seal_hand_assembled_ops();
        f
    };
    let sym = |name: &str| -> (Value, SymId) {
        let s = Value::symbol(name);
        let ValueKind::Symbol(id) = s.kind() else {
            unreachable!("symbol");
        };
        (s, id)
    };

    let mut ev = Context::new();

    // Callee `bump`: (setcar cell (1+ (car cell))) ; arg.  The car of `cell`
    // counts how many times bump actually ran — the observable side effect.
    let cell = Value::cons(Value::make_int(0), Value::NIL);
    let (bump_sym, bump_id) = sym("aot-cb-bump");
    let bump = mk_fn(
        vec![
            Op::StackRef(0), // arg
            Op::Constant(0), // cell
            Op::Dup,         // cell cell
            Op::Car,         // cell (car cell)
            Op::Add1,        // cell (1+ (car cell))
            Op::Setcar,      // -> result (car set); stack: arg result
            Op::DiscardN(1), // arg
            Op::Return,
        ],
        vec![cell],
        1,
    );
    ev.obarray
        .set_symbol_function_id(bump_id, Value::make_bytecode(bump));

    // Caller `F`: (1+ (bump x)).  bump returns x; the post-call `1+` deopts when
    // x is NOT a fixnum (resume mid-body AFTER the call ran).
    let f_ops = vec![
        Op::Constant(0), // bump
        Op::StackRef(1), // x
        Op::Call(1),     // (bump x)
        Op::Add1,        // (1+ ...)
        Op::Return,
    ];
    let f_consts = vec![bump_sym];
    let f = mk_fn(f_ops.clone(), f_consts.clone(), 1);

    // Emit + place F's AOT `.so` (call-bearing → precise deopt).
    let _hash = testkit_emit_and_place_so(&f_ops, &f_consts, 1, dir)
        .ok_or("F not AOT-runnable (expected call-bearing → Some)")?;
    let ctx = &mut ev as *mut Context;
    let count = |cell: Value| cell.cons_car().as_fixnum().unwrap_or(-1);

    // (1) NON-deopt path: F(5) = 1+(bump 5) = 6; bump ran once. Served from AOT.
    cell.set_car(Value::make_int(0));
    let got = super::cache::try_run_compiled(ctx, &f, Value::NIL, &[Value::make_int(5)])
        .map_err(|_| "F(5) unexpectedly signalled")?;
    if got != Some(Value::make_int(6).bits()) {
        return Err(format!("F(5) = 1+(bump 5) expected 6, got {got:?}"));
    }
    if count(cell) != 1 {
        return Err(format!("F(5): bump must run once, ran {}", count(cell)));
    }
    // Confirm it was served FROM AOT (not JIT) — the leaf is cached AOT-backed.
    if super::cache::cached_leaf_is_aot_for_func(&f) != Some(true) {
        return Err("F(5) was not served from AOT".into());
    }

    // (2) DEOPT-ACROSS-CALL: F('not-a-number). bump returns the symbol → `1+`
    // guard fails → precise deopt → interp resumes at the `1+` pc (NOT from
    // start) → 1+ of a non-number signals. CRITICAL: bump ran EXACTLY ONCE.
    let arg = Value::symbol("aot-cb-not-a-number");
    cell.set_car(Value::make_int(0));
    let aot_res = super::cache::try_run_compiled(ctx, &f, Value::NIL, &[arg]);
    let aot_count = count(cell);
    if aot_count != 1 {
        return Err(format!(
            "SIDE-EFFECT-EXACTLY-ONCE violated: bump ran {aot_count} times across the \
             deopt (must be 1 — resume-at-pc, not rerun-from-start)"
        ));
    }

    // Cross-check vs the interpreter: same input, fresh counter.
    cell.set_car(Value::make_int(0));
    let interp_res = {
        let mut vm = Vm::from_context(&mut ev);
        vm.execute(&f, vec![arg])
    };
    let interp_count = count(cell);
    if interp_count != 1 {
        return Err(format!(
            "interp: bump ran {interp_count} times (expected 1)"
        ));
    }
    if aot_res.is_err() != interp_res.is_err() {
        return Err(format!(
            "AOT deopt outcome != interp: aot_err={}, interp_err={}",
            aot_res.is_err(),
            interp_res.is_err()
        ));
    }

    // (3) #A eq-identity: a leaf that RETURNS a heap-string constant returns the
    // SAME object as the source const (eq), exactly as interp/JIT — proven via a
    // separate reloc-returning leaf served from AOT.
    let lit = Value::string("aot-cb-literal");
    let g_ops = vec![Op::Constant(0), Op::Return];
    let g_consts = vec![lit];
    let g = mk_fn(g_ops.clone(), g_consts.clone(), 1);
    testkit_emit_and_place_so(&g_ops, &g_consts, 1, dir)
        .ok_or("G (reloc-returning) not AOT-runnable")?;
    let g_got = super::cache::try_run_compiled(ctx, &g, Value::NIL, &[Value::make_int(0)])
        .map_err(|_| "G signalled")?;
    if g_got != Some(lit.bits()) {
        return Err(format!(
            "#A eq-identity: G must return the SOURCE const object (bits {:#x}), got {g_got:?}",
            lit.bits()
        ));
    }

    Ok(())
}

/// R2-E E1a self-test: a BASELINE-tier AOT leaf emits + serves AOT==interp,
/// INCLUDING a FORCED precise deopt (must-nail #1, the genuinely-new
/// baseline-deopt-resume-via-sidecar path). Body = `(* x x)` — at D0 the baseline
/// emits a fixnum-range guard on the multiply; an in-range x stays native, an
/// x whose square overflows fixnum→bignum DEOPTS (STATUS_DEOPT_AT) → the
/// interpreter resumes at pc and does the bignum multiply. We assert AOT == interp
/// for BOTH (no-deopt and forced-deopt) inputs, end-to-end through the public
/// `try_run_compiled` under `NEOVM_AOT=force` + `NEOVM_AOT_DIR`.
///
/// Emitted via the BASELINE tier (`build_baseline_object_for_leaf`) — NOT the MIR
/// tier — so it exercises `build_leaf_fn::<ObjectModule>(aot=true)` + the sidecar
/// deopt bases (the new path (a) wired). `(* x x)` is intentionally builtin-free
/// (no op-SymId reloc) so this isolates the deopt-RESUME from must-nail #2's reloc.
#[doc(hidden)]
#[cfg(target_os = "linux")]
pub fn testkit_baseline_aot_selftest(dir: &std::path::Path) -> Result<(), String> {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    use crate::emacs_core::value::LambdaParams;

    if !aot_enabled() {
        return Err("NEOVM_AOT not enabled (env must be set before any AOT call)".into());
    }

    // (lambda (x) (* x x)) — 1 required arg, lexical. StackRef(0) twice, Mul, Return.
    let ops = vec![Op::StackRef(0), Op::StackRef(0), Op::Mul, Op::Return];
    let constants: Vec<Value> = vec![];
    let arity = 1usize;

    // Emit through the BASELINE AOT path + place the `.so` in NEOVM_AOT_DIR.
    let content_hash = testkit_emit_baseline_and_place_so(&ops, &constants, arity, dir)
        .ok_or("baseline AOT emit/place failed (body not baseline-AOT-runnable?)")?;
    let _ = content_hash;

    let mk_fn = |required: usize| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: (0..required).map(|i| SymId(1 + i as u32)).collect(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.clone();
        f.constants = constants.clone().into();
        f.max_stack = 16;
        f.seal_hand_assembled_ops();
        f
    };

    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;

    // Helper: AOT result via the cache (forced), and interp result, for one input
    // Value. Returns the two Values; compare by `eql_value` (a bignum/float result
    // is a HEAP object — distinct allocations have distinct addresses, so a raw-bits
    // compare would falsely fail; eql compares by numeric VALUE).
    let aot_and_interp =
        |ev: &mut Context, ctx: *mut Context, x: Value| -> Result<(Value, Value), String> {
            let f = mk_fn(arity);
            let f_val = Value::make_bytecode(f.clone());
            // AOT serve (try_run_compiled consults AOT first under force).
            let aot = super::cache::try_run_compiled(ctx, &f, f_val, &[x])
                .map_err(|_| "aot run raised".to_string())?
                .ok_or("aot run returned None (not served)".to_string())?;
            // Confirm it was actually AOT-backed (not JIT'd).
            if super::cache::cached_leaf_is_aot_for_func(&f) != Some(true) {
                return Err(format!("body not served AOT-backed for x={x:?}"));
            }
            // Interp result (a fresh fn, never compiled — run via the VM directly).
            let interp = {
                use crate::emacs_core::bytecode::Vm;
                let g = mk_fn(arity);
                let mut vm = Vm::from_context(ev);
                vm.execute(&g, vec![x])
                    .map_err(|_| "interp run raised".to_string())?
            };
            Ok((Value::from_bits(aot), interp))
        };

    // (1) In-range: x=7 → 49, native (no deopt). AOT == interp (by value).
    let (a1, i1) = aot_and_interp(&mut ev, ctx, Value::make_int(7))?;
    if !crate::emacs_core::value::eql_value(&a1, &i1) {
        return Err(format!("no-deopt: AOT {a1:?} != interp {i1:?} (x=7)"));
    }
    if a1.as_fixnum() != Some(49) {
        return Err(format!("no-deopt: wrong result {a1:?} (expected 49)"));
    }

    // (2) FORCED OVERFLOW DEOPT: x large enough that x*x overflows fixnum → bignum.
    // The baseline's fixnum-RANGE guard on Mul (raw_fixnum_mul) fails → STATUS_DEOPT_AT
    // → the interp resumes and computes the bignum. AOT == interp proves the resume.
    let big = 3_037_000_500i64; // big*big ≈ 9.22e18 > MOST_POSITIVE_FIXNUM (~2.3e18)
    let (a2, i2) = aot_and_interp(&mut ev, ctx, Value::make_int(big))?;
    if !crate::emacs_core::value::eql_value(&a2, &i2) {
        return Err(format!(
            "overflow-deopt: AOT {a2:?} != interp {i2:?} (x={big})"
        ));
    }
    if a2.as_fixnum().is_some() {
        return Err(
            "overflow-deopt: result is a fixnum — no overflow happened, deopt not exercised".into(),
        );
    }

    // (3) FORCED TYPE-GUARD DEOPT (team-lead defense-in-depth): a NON-fixnum operand
    // (a float) makes the fixnum TYPE guard (guard_fixnum) on Mul fail — a DISTINCT
    // deopt SITE at a different pc/operand-stack-depth than the overflow range-check.
    // → STATUS_DEOPT_AT → interp resumes → float multiply → 6.25. AOT == interp proves
    // the framestate restore is correct at >1 site (where a baseline-meta off-by-one
    // would bite differently).
    let fx = Value::make_float(2.5);
    let (a3, i3) = aot_and_interp(&mut ev, ctx, fx)?;
    if !crate::emacs_core::value::eql_value(&a3, &i3) {
        return Err(format!(
            "type-guard-deopt: AOT {a3:?} != interp {i3:?} (x=2.5)"
        ));
    }
    // Sanity: a float result (2.5*2.5=6.25) — NOT a fixnum, so the type-guard deopt
    // genuinely fired (the native fixnum fast path cannot produce a float).
    if a3.as_fixnum().is_some() {
        return Err(
            "type-guard-deopt: result is a fixnum — the float type-guard deopt did not fire".into(),
        );
    }

    super::cache::clear();
    Ok(())
}

/// R2-E audit follow-up (test gap a): a baseline-AOT deopt at a DEEPER stack with
/// LIVE RAW (unboxed) slots — the path the `(* x x)` selftest never reaches
/// (its deopt is always at pc=2/depth=2 with no live raw slot below the guard).
///
/// Body `(lambda (a b) (* (+ a 1) (+ b 1)))`: the two inner `Add`s leave unboxed
/// fixnums on the operand stack, so at the outer `Mul`'s deopt the pre-op stack is
/// `[a, b, (a+1), (b+1)]` (DEPTH 4 > 2) with `(a+1)`/`(b+1)` as LIVE RAW slots.
/// Forcing the `Mul` to overflow exercises emit_pending_deopts'
/// raw-slot-retag (stack_raw[j] → re-tag to a tagged Value) + the spill of a
/// deeper framestate — the cold path the audit flagged as AOT-uncovered. AOT ==
/// interp proves the deeper restore is correct.
#[doc(hidden)]
#[cfg(target_os = "linux")]
pub fn testkit_baseline_deep_rawslot_deopt_selftest(dir: &std::path::Path) -> Result<(), String> {
    use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    use crate::emacs_core::value::LambdaParams;

    if !aot_enabled() {
        return Err("NEOVM_AOT not enabled".into());
    }

    // (lambda (a b) (* (+ a 1) (+ b 1))). Stack starts [a, b] (b on top).
    //   StackRef(1) → a ; Constant(0)=1 ; Add → (a+1)
    //   StackRef(1) → b ; Constant(0)=1 ; Add → (b+1)
    //   Mul → result.  At Mul the pre-stack is [a, b, (a+1), (b+1)] (depth 4).
    let ops = vec![
        Op::StackRef(1),
        Op::Constant(0),
        Op::Add,
        Op::StackRef(1),
        Op::Constant(0),
        Op::Add,
        Op::Mul,
        Op::Return,
    ];
    let constants: Vec<Value> = vec![Value::make_int(1)];
    let arity = 2usize;

    // Place ONLY the BASELINE `.so` (no MIR `.so`), so `try_run_compiled` serves the
    // BASELINE leaf even though this pure-arith body is also MIR-AOT-runnable — the
    // unit index has only the baseline object for this content hash (same approach
    // as `testkit_baseline_aot_selftest`). `try_run_compiled` drives the FULL path:
    // native call + finish_native_run + run_resumed_frame on STATUS_DEOPT_AT, so the
    // deopt RESUME is exercised (a direct `leaf.call` would only return the raw
    // deopt status, not the resumed result).
    testkit_emit_baseline_and_place_so(&ops, &constants, arity, dir)
        .ok_or("baseline AOT emit/place failed for the deep-raw-slot body")?;

    let mk_fn = || {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1), SymId(2)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.clone();
        f.constants = constants.clone().into();
        f.max_stack = 16;
        f.seal_hand_assembled_ops();
        f
    };

    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;

    // Serve via the cache (baseline-backed) + the interpreter; compare. Two regimes:
    //  (1) NO deopt: small a,b → native fixnum result.
    //  (2) FORCED OVERFLOW deopt at the deep Mul (depth 4, live raw slots).
    let run_baseline = |ev: &mut Context,
                        ctx: *mut Context,
                        a: Value,
                        b: Value|
     -> Result<(Value, Value), String> {
        let f = mk_fn();
        let f_val = Value::make_bytecode(f.clone());
        let bits = super::cache::try_run_compiled(ctx, &f, f_val, &[a, b])
            .map_err(|_| "baseline leaf call raised".to_string())?
            .ok_or("aot run returned None (not served)".to_string())?;
        if super::cache::cached_leaf_is_aot_for_func(&f) != Some(true) {
            return Err("deep body not served AOT-backed".to_string());
        }
        let interp = {
            let g = mk_fn();
            let mut vm = Vm::from_context(ev);
            vm.execute(&g, vec![a, b])
                .map_err(|_| "interp raised".to_string())?
        };
        Ok((Value::from_bits(bits), interp))
    };

    // (1) No-deopt: (* (+ 3 1) (+ 4 1)) = 4*5 = 20.
    let (n_aot, n_int) = run_baseline(&mut ev, ctx, Value::make_int(3), Value::make_int(4))?;
    if !crate::emacs_core::value::eql_value(&n_aot, &n_int) {
        return Err(format!("deep no-deopt: AOT {n_aot:?} != interp {n_int:?}"));
    }
    if n_aot.as_fixnum() != Some(20) {
        return Err(format!(
            "deep no-deopt: wrong result {n_aot:?} (expected 20)"
        ));
    }

    // (2) FORCED OVERFLOW at the DEEP Mul (depth 4, two live raw slots (a+1),(b+1)):
    // a=b=2_000_000_000 → (a+1)*(b+1) ≈ 4.0e18 > MOST_POSITIVE_FIXNUM (~2.3e18) →
    // the Mul fixnum-range guard fails → STATUS_DEOPT_AT → the interp resumes from
    // the DEEPER framestate (raw slots retagged) → bignum. AOT == interp proves the
    // deep raw-slot restore is correct.
    let big = 2_000_000_000i64;
    let (d_aot, d_int) = run_baseline(&mut ev, ctx, Value::make_int(big), Value::make_int(big))?;
    if !crate::emacs_core::value::eql_value(&d_aot, &d_int) {
        return Err(format!(
            "deep overflow-deopt: AOT {d_aot:?} != interp {d_int:?} (a=b={big})"
        ));
    }
    if d_aot.as_fixnum().is_some() {
        return Err(
            "deep overflow-deopt: result is a fixnum — no overflow, the deep deopt was not exercised".into(),
        );
    }

    super::cache::clear();
    Ok(())
}

/// R2-E E1b self-test (must-nail #2): a BASELINE-tier AOT leaf that calls a
/// builtin via `Op::CallBuiltinSym` serves AOT==interp, AND its callee SymId is
/// RELOC'd BY NAME (not the session-specific baked id). Body = `(length x)` via
/// `CallBuiltinSym(intern("length"), 1)` — the Category-2 unlock that needs both
/// the baseline-emit path (b) and the op-SymId reloc (c).
///
/// The cross-session CORRECTNESS proof has THREE legs, all checked here:
///   1. the content hash is BY NAME — `leaf_content_hash` hashes CallBuiltinSym by
///      the callee's name+nargs, so the same body hashes identically regardless of
///      the session's intern order (the cache KEY is session-stable);
///   2. the callee symbol is in the leaf's RELOC set + recipe BY NAME (not baked):
///      we GROW the intern table after emit (modeling the cross-session intern-order
///      drift the #16 hazard is about — a baked emit-time SymId would now be stale),
///      then assert the SERVED leaf's reloc_values() contain the callee symbol
///      resolved to "length" (a baked id would NOT appear in the reloc set);
///   3. the served leaf computes the RIGHT result (== interp) — a wrong/stale baked
///      SymId would call the wrong builtin → wrong result or crash.
///
/// (One process can't literally re-assign "length"'s id, but the decoy-growth +
/// reloc-set-membership is the SAME rigor the #16 symbol-const cross-session test
/// uses — it proves the id is reloc'd-by-name, not baked, which IS the divergence
/// protection.)
#[doc(hidden)]
#[cfg(target_os = "linux")]
pub fn testkit_callbuiltinsym_aot_selftest(dir: &std::path::Path) -> Result<(), String> {
    use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    use crate::emacs_core::value::LambdaParams;

    if !aot_enabled() {
        return Err("NEOVM_AOT not enabled".into());
    }
    let length_id = crate::emacs_core::intern::intern("length");
    // (lambda (x) (length x)) — push arg, CallBuiltinSym length/1, return.
    let ops = vec![
        Op::StackRef(0),
        Op::CallBuiltinSym(length_id, 1),
        Op::Return,
    ];
    let constants: Vec<Value> = vec![];
    let arity = 1usize;

    // Leg 2a: the descriptor recipe must encode the callee BY NAME (reloc-by-name),
    // not a baked SymId. Build the baseline object + decode its descriptor recipe.
    let (reloc_data, _reloc_index, recipe) = prepare_baseline_relocs(&ops, &constants)
        .ok_or("baseline relocs: length symbol not recipe-able?")?;
    if reloc_data.is_empty() {
        return Err("CallBuiltinSym callee was NOT collected into the reloc set (op-SymId not reloc'd by name)".into());
    }
    // The recipe should contain the bytes "length" (the RECIPE_SYMBOL name).
    if !recipe.windows(b"length".len()).any(|w| w == b"length") {
        return Err(
            "reloc recipe does not encode the callee name 'length' (reloc-by-name missing)".into(),
        );
    }

    // Emit via the baseline AOT path + place the `.so`.
    testkit_emit_baseline_and_place_so(&ops, &constants, arity, dir)
        .ok_or("baseline AOT emit/place failed for the CallBuiltinSym body")?;

    // Leg 2 (cross-session drift): GROW the intern table AFTER emit, so an emit-time
    // BAKED SymId would now be stale relative to a fresh rebuild (models a different
    // intern order at load). The reloc-by-name path is immune; a baked id would not
    // be. (Same rigor as `aot_symbol_const_relocs_by_name_not_baked_sym_id`.)
    for i in 0..64 {
        let _ = crate::emacs_core::intern::intern(&format!("aot-cbs-decoy-{i}"));
    }

    let mk_fn = || {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.clone();
        f.constants = constants.clone().into();
        f.max_stack = 16;
        f.seal_hand_assembled_ops();
        f
    };

    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    // arg = a 3-element list → (length arg) == 3.
    let arg = Value::list(vec![
        Value::make_int(10),
        Value::make_int(20),
        Value::make_int(30),
    ]);

    let f = mk_fn();
    let f_val = Value::make_bytecode(f.clone());
    let aot = super::cache::try_run_compiled(ctx, &f, f_val, &[arg])
        .map_err(|_| "aot run raised".to_string())?
        .ok_or("aot run returned None".to_string())?;
    if super::cache::cached_leaf_is_aot_for_func(&f) != Some(true) {
        return Err("CallBuiltinSym body not served AOT-backed".into());
    }
    // Leg 1+2b: the served result must be CORRECT (3). A wrong/stale baked SymId
    // would have called a different builtin → wrong result. Cross-check vs interp.
    let interp = {
        let g = mk_fn();
        let mut vm = Vm::from_context(&mut ev);
        vm.execute(&g, vec![arg])
            .map_err(|_| "interp raised".to_string())?
    };
    if !crate::emacs_core::value::eql_value(&Value::from_bits(aot), &interp) {
        return Err(format!(
            "CallBuiltinSym: AOT {:?} != interp {:?}",
            Value::from_bits(aot),
            interp
        ));
    }
    if Value::from_bits(aot).as_fixnum() != Some(3) {
        return Err(format!(
            "CallBuiltinSym: wrong (length ...) result {:?} (expected 3 — reloc'd to the WRONG builtin?)",
            Value::from_bits(aot)
        ));
    }

    // Leg 2 PROOF: load the leaf directly + assert the callee symbol is in its
    // RELOC SET, resolved BY NAME to "length" (the CURRENT canonical symbol — note
    // the intern table was grown above). A BAKED SymId would NOT appear in
    // reloc_values(); its presence proves the op-SymId is reloc'd-by-name.
    let content_hash = leaf_content_hash(&ops, &constants, arity).ok_or("content hash None")?;
    let unit = load_unit(content_hash).ok_or("unit not found for reloc-set proof")?;
    let leaf = load_leaf_from_unit(&unit, content_hash, arity, &constants, None)
        .ok_or("load_leaf_from_unit None (reloc count/recipe mismatch?)")?;
    let reloc_names: std::collections::HashSet<&str> = leaf
        .reloc_values()
        .iter()
        .filter_map(|v| v.as_symbol_id())
        .map(crate::emacs_core::intern::resolve_sym)
        .collect();
    if !reloc_names.contains("length") {
        return Err(format!(
            "callee 'length' NOT in the leaf's reloc set (op-SymId was BAKED, not reloc'd-by-name) — reloc names: {reloc_names:?}"
        ));
    }

    super::cache::clear();
    Ok(())
}

/// R2 increment A (CBSym-in-AOT) debug-build snapshot of the `(fast, generic)`
/// CallBuiltinSym intrinsic-shim counters (host-side statics touched by
/// `neovm_jit_cbsym_read` / `neovm_jit_cbsym_spec`). A served AOT `.so` binds those
/// shims against THESE host statics at `dlopen`, so the counter moving is proof the
/// shim both resolved and ran the op itself.
#[cfg(all(target_os = "linux", debug_assertions))]
fn cbsym_shim_counters() -> (u64, u64) {
    use std::sync::atomic::Ordering;
    (
        super::compile::CBSYM_SPEC_FAST_COUNT.load(Ordering::Relaxed),
        super::compile::CBSYM_SPEC_GENERIC_COUNT.load(Ordering::Relaxed),
    )
}

/// R2 increment A: serve an ALREADY-PLACED CBSym-bearing baseline `.so` through
/// `try_run_compiled` and assert three things: (1) it served AOT-backed (the cbsym
/// shim import RESOLVED at `dlopen` — an unexported shim would fail to bind and the
/// leaf would never serve); (2) the FAST intrinsic shim fired (debug-build counter
/// moved, no NEED_GENERIC bounce — i.e. NOT the slow `neovm_jit_named_builtin`
/// path); (3) the served result == the interpreter on the SAME context state.
///
/// The caller MUST have placed the `.so` (via `testkit_emit_baseline_and_place_so`)
/// BEFORE the first serve of ANY unit: the AOT unit index scans `NEOVM_AOT_DIR`
/// once (a process-wide `OnceLock` frozen on the first `load_unit`), so a `.so`
/// planted after that first serve would never be discovered.
#[cfg(target_os = "linux")]
fn cbsym_aot_serve_and_check(
    ev: &mut crate::emacs_core::eval::Context,
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    args: &[Value],
    label: &str,
) -> Result<(), String> {
    use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
    use crate::emacs_core::intern::SymId;
    use crate::emacs_core::value::LambdaParams;

    let mk = || {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: (1..=arity as u32).map(SymId).collect(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.to_vec();
        f.constants = constants.to_vec().into();
        f.max_stack = 16;
        f.seal_hand_assembled_ops();
        f
    };

    #[cfg(debug_assertions)]
    let (fast0, gen0) = cbsym_shim_counters();

    let f = mk();
    let f_val = Value::make_bytecode(f.clone());
    let ctx: *mut crate::emacs_core::eval::Context = &mut *ev;
    let aot = super::cache::try_run_compiled(ctx, &f, f_val, args)
        .map_err(|_| format!("{label}: aot run raised"))?
        .ok_or_else(|| format!("{label}: aot run returned None"))?;
    if super::cache::cached_leaf_is_aot_for_func(&f) != Some(true) {
        return Err(format!("{label}: body not served AOT-backed"));
    }

    // DLOPEN + FAST-SHIM proof (debug builds): the served leaf CALLED the cbsym shim
    // (so it resolved at dlopen) AND the shim ran the op itself (fast count moved,
    // no NEED_GENERIC bounce to the slow named_builtin path).
    #[cfg(debug_assertions)]
    {
        let (fast1, gen1) = cbsym_shim_counters();
        if fast1 <= fast0 {
            return Err(format!(
                "{label}: CBSym FAST shim did not fire (fast {fast0}->{fast1}) — the AOT leaf took the slow named_builtin path (or the shim failed to bind at dlopen)"
            ));
        }
        if gen1 != gen0 {
            return Err(format!(
                "{label}: unexpected NEED_GENERIC bounce (generic {gen0}->{gen1})"
            ));
        }
    }

    // Result == interp on the SAME context state (a wrong/stale reloc'd SymId or a
    // mis-baked Tier-A `which` would compute a different value).
    let interp = {
        let g = mk();
        let mut vm = Vm::from_context(&mut *ev);
        vm.execute(&g, args.to_vec())
            .map_err(|_| format!("{label}: interp raised"))?
    };
    if !crate::emacs_core::value::eql_value(&Value::from_bits(aot), &interp) {
        return Err(format!(
            "{label}: AOT {:?} != interp {:?}",
            Value::from_bits(aot),
            interp
        ));
    }
    Ok(())
}

/// R2 increment A (CBSym-in-AOT) self-test — THE DLOPEN + FAST-SHIM proof. A
/// BASELINE-tier AOT leaf whose body is a CallBuiltinSym intrinsic now emits the
/// FAST shim (its classification is name-canonical + obarray-free, so it runs at
/// the AOT emit's `obarray=None`), so a served `.so` must bind the shim against the
/// host at `dlopen` and RUN it. Covers BOTH shims:
///   * `(point)`    → Tier-A `neovm_jit_cbsym_read` (GC-free read);
///   * `(length x)` → Tier-B `neovm_jit_cbsym_spec` (dispatch-skip).
/// Each leg proves the shim resolved (served AOT-backed), fired the fast path (the
/// host counter moved, no generic bounce), and matched the interpreter. This is the
/// sidecar-free proof-of-concept that validates the shim-export + dlopen + reloc
/// machinery before increment B.
#[doc(hidden)]
#[cfg(target_os = "linux")]
pub fn testkit_cbsym_aot_fast_shim_selftest(dir: &std::path::Path) -> Result<(), String> {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::intern;

    if !aot_enabled() {
        return Err("NEOVM_AOT not enabled".into());
    }

    // A Context with a non-trivial buffer + a moved point, so `(point)` is a
    // deterministic non-trivial fixnum (and `(point)` is a pure read, so the AOT
    // run doesn't perturb the interp cross-check).
    let mut ev = Context::new();
    ev.eval_str("(insert \"abc\\ndef\")")
        .map_err(|_| "buffer setup (insert) failed".to_string())?;
    ev.eval_str("(goto-char 3)")
        .map_err(|_| "buffer setup (goto-char) failed".to_string())?;

    let point_ops = vec![Op::CallBuiltinSym(intern("point"), 0), Op::Return];
    let length_ops = vec![
        Op::StackRef(0),
        Op::CallBuiltinSym(intern("length"), 1),
        Op::Return,
    ];
    let arg = Value::list(vec![
        Value::make_int(1),
        Value::make_int(2),
        Value::make_int(3),
        Value::make_int(4),
    ]);

    // Emit + place BOTH `.so`s FIRST. The AOT unit index scans NEOVM_AOT_DIR ONCE
    // (a process-wide OnceLock frozen on the first `load_unit`), so every `.so`
    // must be on disk before the first serve below. CBSym bodies land in the
    // BASELINE tier (the MIR pure tier rejects CallBuiltinSym).
    testkit_emit_baseline_and_place_so(&point_ops, &[], 0, dir)
        .ok_or("Tier-A (point) baseline AOT emit/place failed")?;
    testkit_emit_baseline_and_place_so(&length_ops, &[], 1, dir)
        .ok_or("Tier-B (length) baseline AOT emit/place failed")?;

    // Tier-A: (point) → neovm_jit_cbsym_read (GC-free read).
    cbsym_aot_serve_and_check(
        &mut ev,
        &point_ops,
        &[],
        0,
        &[],
        "Tier-A (point) [cbsym_read]",
    )?;

    // Tier-B: (length x) → neovm_jit_cbsym_spec (dispatch-skip), x a 4-element list.
    cbsym_aot_serve_and_check(
        &mut ev,
        &length_ops,
        &[],
        1,
        &[arg],
        "Tier-B (length) [cbsym_spec]",
    )?;

    super::cache::clear();
    Ok(())
}

/// R2-E audit CRITICAL fix: prove the OTHER baseline op-SymId sites — a SYMBOL
/// `Op::Constant` and the dynamic-variable ops (`VarRef`/`VarSet`/`VarBind`) —
/// also reloc their session-specific SymId BY NAME, not bake it.
///
/// The audit found that the `aot` reloc-awareness reached only the CallBuiltinSym
/// callee site; symbol consts (gated on `is_heap_object()`, which excludes
/// symbols) and the var ops (`iconst(sym)`, not even collected) baked the SESSION
/// SymId under baseline-AOT → silent cross-session corruption (recipe check still
/// passes). This test exercises both kinds of body END-TO-END through the
/// baseline serve and checks relocation-set membership. The variable body also
/// executes with its relocation payload changed to a symbol in another chunk,
/// exercising the generated runtime address calculation.
///
/// CRUCIAL — both bodies carry a `CallBuiltinSym` so `mir_is_aot_runnable` is
/// FALSE → the emit AND load tier-selects BOTH pick the baseline tier (a
/// VarRef-only body would route MIR-then-error and never reach baseline, so it
/// could not exercise the fix). Each asserts: the served leaf's `reloc_values()`
/// contain the symbol/var resolved BY NAME (a baked id would be absent), and the
/// result matches the interpreter.
#[doc(hidden)]
#[cfg(target_os = "linux")]
pub fn testkit_baseline_op_symbol_reloc_selftest(dir: &std::path::Path) -> Result<(), String> {
    use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    use crate::emacs_core::value::LambdaParams;

    if !aot_enabled() {
        return Err("NEOVM_AOT not enabled".into());
    }

    // One test body: the bytecode, its inputs, and the symbol that must be reloc'd
    // BY NAME. Both bodies carry a CallBuiltinSym so `mir_is_aot_runnable` is FALSE
    // → the emit AND load tier-selects both pick the baseline tier.
    struct Body {
        label: &'static str,
        ops: Vec<Op>,
        constants: Vec<Value>,
        arity: usize,
        args: Vec<Value>,
        must_contain: &'static str,
    }

    // --- Body A: a SYMBOL Op::Constant (audit CRITICAL #1) ---
    // (lambda (x) (symbol-name 'aot-symconst)) — the quoted symbol const flows
    // through `lower_simple_op`'s Op::Constant; CallBuiltinSym forces the baseline
    // tier. symbol-name of the canonical symbol → its name string, == interp.
    let body_a = {
        let symconst = Value::symbol(crate::emacs_core::intern::intern("aot-symconst"));
        let symbol_name = crate::emacs_core::intern::intern("symbol-name");
        Body {
            label: "symbol-const",
            ops: vec![
                Op::Constant(0),
                Op::CallBuiltinSym(symbol_name, 1),
                Op::Return,
            ],
            constants: vec![symconst],
            arity: 1,
            args: vec![Value::NIL],
            must_contain: "aot-symconst",
        }
    };
    // --- Body B: VarBind + VarSet + VarRef on a dynamic variable (CRITICAL #2) ---
    // (lambda () (let ((aot-dynvar 1)) (setq aot-dynvar 42) (identity aot-dynvar)))
    // VarBind binds, VarSet assigns, VarRef reads the SAME var-symbol (constants[0]);
    // CallBuiltinSym(identity) forces the baseline tier + returns the read value 42.
    let body_b = {
        let dynvar = Value::symbol(crate::emacs_core::intern::intern("aot-dynvar"));
        let identity = crate::emacs_core::intern::intern("identity");
        Body {
            label: "varbind/set/ref",
            ops: vec![
                Op::Constant(1),                 // 1
                Op::VarBind(0),                  // bind aot-dynvar = 1
                Op::Constant(2),                 // 42
                Op::VarSet(0),                   // aot-dynvar = 42
                Op::VarRef(0),                   // read aot-dynvar -> 42
                Op::CallBuiltinSym(identity, 1), // (identity 42) -> 42 [forces baseline]
                Op::Unbind(1),                   // pop the dynamic binding
                Op::Return,
            ],
            constants: vec![dynvar, Value::make_int(1), Value::make_int(42)],
            arity: 0,
            args: vec![],
            must_contain: "aot-dynvar",
        }
    };
    let bodies = [body_a, body_b];

    // PASS 1 — emit + place ALL `.so`s BEFORE any serve. The unit index is a
    // process-wide `OnceLock` frozen on the FIRST `load_unit`, so a body whose
    // `.so` is placed after the first serve would be invisible (the #32-audit
    // "G-serves-from-JIT" gotcha). Emit-all-first guarantees both are indexed.
    for b in &bodies {
        // Must route through the BASELINE tier (not MIR) for the fix to be tested.
        if let Ok(m) = mir::build_mir(&b.ops, &b.constants, b.arity)
            && mir_is_aot_runnable(&m)
        {
            return Err(format!(
                "{}: body is MIR-AOT-runnable → would not exercise the BASELINE op-SymId site",
                b.label
            ));
        }
        // The session-specific symbol MUST be in the reloc recipe BY NAME (a baked
        // id would not be collected at all).
        let (reloc_data, _idx, recipe) =
            prepare_baseline_relocs(&b.ops, &b.constants).ok_or(format!(
                "{}: baseline relocs None (symbol not recipe-able?)",
                b.label
            ))?;
        if reloc_data.is_empty() {
            return Err(format!(
                "{}: NO reloc consts — the op-SymId was BAKED, not collected by name",
                b.label
            ));
        }
        if !recipe
            .windows(b.must_contain.len())
            .any(|w| w == b.must_contain.as_bytes())
        {
            return Err(format!(
                "{}: reloc recipe does not encode '{}' by name",
                b.label, b.must_contain
            ));
        }
        testkit_emit_baseline_and_place_so(&b.ops, &b.constants, b.arity, dir)
            .ok_or(format!("{}: baseline AOT emit/place failed", b.label))?;
    }

    // Grow the table between emission and loading. Existing symbol identities
    // stay stable; the explicit payload change below exercises a different id.
    for i in 0..64 {
        let _ = crate::emacs_core::intern::intern(&format!("aot-opsym-decoy-{i}"));
    }

    // PASS 2 — serve + verify each body (the index is now frozen with both `.so`s).
    for b in &bodies {
        let mk_fn = || {
            let mut f = ByteCodeFunction::new(LambdaParams {
                required: (0..b.arity).map(|i| SymId(1 + i as u32)).collect(),
                optional: Vec::new(),
                rest: None,
            });
            f.lexical = true;
            f.ops = b.ops.clone();
            f.constants = b.constants.clone().into();
            f.max_stack = 16;
            f.seal_hand_assembled_ops();
            f
        };

        let mut ev = Context::new();
        let ctx = &mut ev as *mut Context;
        let f = mk_fn();
        let f_val = Value::make_bytecode(f.clone());
        let aot = super::cache::try_run_compiled(ctx, &f, f_val, &b.args)
            .map_err(|_| format!("{}: aot run raised", b.label))?
            .ok_or(format!(
                "{}: aot run returned None (reloc/tier mismatch?)",
                b.label
            ))?;
        if super::cache::cached_leaf_is_aot_for_func(&f) != Some(true) {
            return Err(format!("{}: not served AOT-backed", b.label));
        }
        // Result must equal the interpreter (a wrong/stale baked id would diverge).
        let interp = {
            let g = mk_fn();
            let mut vm = Vm::from_context(&mut ev);
            vm.execute(&g, b.args.clone())
                .map_err(|_| format!("{}: interp raised", b.label))?
        };
        // `equal` (content), not `eql` (identity): a String result from
        // `symbol-name` is a distinct allocation per call, so an identity compare
        // would falsely fail even though the names match. `equal_value` is also
        // correct for fixnum/float/bignum results.
        if !crate::emacs_core::value::equal_value(&Value::from_bits(aot), &interp, 0) {
            return Err(format!(
                "{}: AOT {:?} != interp {:?}",
                b.label,
                Value::from_bits(aot),
                interp
            ));
        }

        // RELOC-SET PROOF: load the leaf + assert the symbol is in its reloc set,
        // resolved BY NAME to `must_contain` (the CURRENT canonical symbol — the
        // intern table was grown above). A BAKED id would NOT appear.
        let content_hash = leaf_content_hash(&b.ops, &b.constants, b.arity)
            .ok_or(format!("{}: hash None", b.label))?;
        let unit = load_unit(content_hash).ok_or(format!("{}: unit not found", b.label))?;
        let mut leaf = load_leaf_from_unit(&unit, content_hash, b.arity, &b.constants, None)
            .ok_or(format!(
                "{}: load_leaf_from_unit None (reloc/recipe mismatch?)",
                b.label
            ))?;
        let reloc_names: std::collections::HashSet<String> = leaf
            .reloc_values()
            .iter()
            .filter_map(|v| v.as_symbol_id())
            .map(|id| crate::emacs_core::intern::resolve_sym(id).to_string())
            .collect();
        if !reloc_names.contains(b.must_contain) {
            return Err(format!(
                "{}: '{}' NOT in the leaf's reloc set (op-SymId was BAKED) — reloc names: {reloc_names:?}",
                b.label, b.must_contain
            ));
        }
        if b.label == "varbind/set/ref" {
            // Changing the payload in its existing allocation tests runtime
            // address calculation directly. Growing the intern table alone
            // never changes a previously interned symbol's identity.
            let original = Value::symbol("aot-dynvar");
            let original_id = original.as_symbol_id().unwrap().0 as usize;
            let slots = crate::emacs_core::symbol::OBARRAY_CHUNK_SLOTS;
            for i in 0..=slots {
                crate::emacs_core::intern::intern(&format!("aot-varref-offset-decoy-{i}"));
            }
            let relocated = Value::symbol("aot-varref-offset-target");
            let relocated_id = relocated.as_symbol_id().unwrap().0 as usize;
            assert_ne!(original_id / slots, relocated_id / slots);
            // Keep the original cell bound: an incorrectly baked address must
            // return the wrong value, instead of falling through to the shim.
            ev.eval_str("(setq aot-dynvar 41 aot-varref-offset-target 17)")
                .map_err(|e| format!("relocated variable setup: {e:?}"))?;
            let slot = leaf
                .reloc_data
                .iter()
                .position(|v| *v == original)
                .ok_or("variable relocation missing")?;
            let base = leaf.reloc_data.as_ptr();
            leaf.reloc_data[slot] = relocated;
            assert_eq!(leaf.reloc_data.as_ptr(), base);
            match leaf.call(ctx as *mut u8, &[]) {
                super::compile::NativeRun::Ok(bits) => {
                    assert_eq!(Value::from_bits(bits), Value::make_int(42));
                }
                other => return Err(format!("relocated variable leaf: {other:?}")),
            }
            let restored = ev
                .eval_str("(list aot-dynvar aot-varref-offset-target)")
                .map_err(|e| format!("relocated variable restoration: {e:?}"))?;
            assert_eq!(crate::emacs_core::print::print_value(&restored), "(41 17)");
        }
        super::cache::clear();
    }

    Ok(())
}

/// R2 increment B2 CROSS-SESSION SOUNDNESS CORPUS — the `Op::Call` spec-in-AOT
/// selftest, invoked from `tests/aot_spec.rs` (a shim-exporting `-rdynamic`
/// integration binary, so the three round-1 spec shims resolve at `dlopen`).
///
/// A body `(callee arg…)` is EMITTED via the baseline AOT tier WITH an obarray in
/// which a USER symbol is aliased to a builtin subr (so `find_spec_sites` bakes the
/// `Op::Call` spec fast path + a descriptor entry), the `.so` is placed in
/// `NEOVM_AOT_DIR`, then it is LOADED against a FRESH obarray. Proves, end to end
/// through the PUBLIC `try_run_compiled` under `NEOVM_AOT=force`:
///  (a) ARMED cross-session: a pred + subr spec body loads ARMED against a fresh
///      obarray + a grown intern table, serves the CORRECT result, and takes the
///      FAST shim FROM CALL 1 (`SUBR_SPEC_FAST_COUNT` moves at heat=0) — dlopen e2e
///      for `neovm_jit_pred_spec` + `neovm_jit_call_subr_spec` (they resolve or the
///      leaves would not serve; the eq shim is export/import-audit-covered);
///  (b/c) THE CRUX: re-alias the callee to a DIFFERENT subr before load → the
///      baked `PredRecordp` site RE-CLASSIFIES to a mismatched kind → DISARMS
///      (never arms `is_record` against the wrong type): the result is the
///      re-aliased subr's (`stringp` → `t` on a string), NOT `is_record`'s (`nil`);
///  (d) the DISARMED slot NEVER re-arms: repeated calls keep `SUBR_SPEC_FAST_COUNT`
///      at 0 and move `SUBR_SPEC_GENERIC_COUNT` each time (the `SPEC_EPOCH_DISARMED`
///      short-circuit, distinct from a re-armable epoch-stale slot).
#[doc(hidden)]
// Consumes the debug-only SUBR_SPEC_* counters → must match their cfg, else
// `cargo build --release --features jit` fails to resolve them (E0432).
#[cfg(all(target_os = "linux", debug_assertions))]
pub fn testkit_spec_aot_selftest(dir: &std::path::Path) -> Result<(), String> {
    use super::compile::{SUBR_SPEC_FAST_COUNT, SUBR_SPEC_GENERIC_COUNT};
    use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::{SymId, intern};
    use crate::emacs_core::value::LambdaParams;
    use std::sync::atomic::Ordering;

    if !aot_enabled() {
        return Err("NEOVM_AOT not enabled (env must be set before any AOT call)".into());
    }

    // Alias a USER symbol `alias_name` to the builtin `builtin_name`'s function cell
    // in `ev`'s obarray (so we control the callee binding per "session").
    let alias = |ev: &mut Context, alias_name: &str, builtin_name: &str| -> Result<(), String> {
        let f = ev
            .obarray
            .symbol_function_id(intern(builtin_name))
            .ok_or_else(|| format!("builtin '{builtin_name}' unbound"))?;
        ev.obarray.set_symbol_function(alias_name, f);
        Ok(())
    };
    // Build `(alias arg…)`: Constant(alias) then `nargs` pushes then Call(nargs).
    let mk_fn = |alias_name: &str, arity: usize, pushes: &[Op]| -> ByteCodeFunction {
        let mut ops = vec![Op::Constant(0)];
        ops.extend_from_slice(pushes);
        ops.push(Op::Call(pushes.len() as u16));
        ops.push(Op::Return);
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: (0..arity).map(|i| SymId(1 + i as u32)).collect(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops;
        f.constants = vec![Value::symbol(intern(alias_name))].into();
        f.max_stack = 16;
        f.seal_hand_assembled_ops();
        f
    };

    // ---- (a) ARMED cross-session + fast-from-call-1, for all three spec shims. ----
    struct Armed {
        label: &'static str,
        alias: &'static str,
        builtin: &'static str,
        arity: usize,
        pushes: Vec<Op>,
        args: Vec<Value>,
    }
    // All callees are Rust builtins bound in a minimal `Context::new()` (so no
    // loadup is needed). This exercises three spec shims a builtin-only context
    // can reach — `neovm_jit_pred_spec` (PredRecordp), `neovm_jit_call_subr_spec`
    // (SubrGeneral), and `neovm_jit_arith_spec` (ArithIntrinsic, logand). The
    // fourth shim (`neovm_jit_eq_incl_props_spec`, for `equal-including-properties`,
    // which is lisp-defined so unbound here) is still exported + salted into
    // `ABI_TAG` + covered by the emit-time `assert_aot_imports_exported` import audit.
    let corpus = [
        // recordp (PredRecordp → neovm_jit_pred_spec): (p x), x=5 → nil.
        Armed {
            label: "pred/recordp",
            alias: "aot-spec-pred",
            builtin: "recordp",
            arity: 1,
            pushes: vec![Op::StackRef(1)],
            args: vec![Value::make_int(5)],
        },
        // consp (SubrGeneral → neovm_jit_call_subr_spec): (c x), x=5 → nil.
        Armed {
            label: "subr/consp",
            alias: "aot-spec-subr",
            builtin: "consp",
            arity: 1,
            pushes: vec![Op::StackRef(1)],
            args: vec![Value::make_int(5)],
        },
        // logand (ArithIntrinsic → neovm_jit_arith_spec): (a x y), 12&10 → 8.
        // Exercises an AOT-baked bit-op intrinsic: the loader re-classifies the
        // live `logand` cell, matches the baked disc (5), arms, and the fast shim
        // computes the native `&` from call 1 (AOT == interp == 8).
        Armed {
            label: "arith/logand",
            alias: "aot-spec-arith",
            builtin: "logand",
            arity: 2,
            pushes: vec![Op::StackRef(2), Op::StackRef(2)],
            args: vec![Value::make_int(12), Value::make_int(10)],
        },
        // ash (ArithIntrinsic, distinct disc 8): (a x y), 3<<4 → 48. Covers a
        // second bit-op disc through the loader re-classify+arm path and the
        // fixnum-shift fast path (which the interpreter lacks) end-to-end.
        Armed {
            label: "arith/ash",
            alias: "aot-spec-ash",
            builtin: "ash",
            arity: 2,
            pushes: vec![Op::StackRef(2), Op::StackRef(2)],
            args: vec![Value::make_int(3), Value::make_int(4)],
        },
    ];

    // PASS 1 — EMIT every `.so` BEFORE any load (the unit index is a OnceLock frozen
    // at the first `load_unit`, so all artifacts must be on disk first). Each body
    // is emitted with its callee aliased to its builtin so `find_spec_sites` bakes
    // the spec fast path + descriptor entry.
    for c in &corpus {
        let mut emit_ctx = Context::new();
        alias(&mut emit_ctx, c.alias, c.builtin)?;
        let f = mk_fn(c.alias, c.arity, &c.pushes);
        let (obj, content_hash) =
            build_baseline_object_for_leaf(&f.ops, &f.constants, c.arity, Some(&emit_ctx.obarray))
                .map_err(|e| format!("{}: emit err {e}", c.label))?
                .ok_or_else(|| format!("{}: emit produced no object (not spec-baked?)", c.label))?;
        let so_path = dir.join(format!("{content_hash:032x}_{ABI_TAG:08x}.so"));
        link_object_to_so(&obj, &so_path).map_err(|e| format!("{}: link err {e}", c.label))?;
    }
    // Emit the CRUX body — aliased to `recordp` at emit (bakes a PredRecordp site).
    {
        let mut emit_ctx = Context::new();
        alias(&mut emit_ctx, "aot-spec-crux", "recordp")?;
        let f = mk_fn("aot-spec-crux", 1, &[Op::StackRef(1)]);
        let (obj, content_hash) =
            build_baseline_object_for_leaf(&f.ops, &f.constants, 1, Some(&emit_ctx.obarray))
                .map_err(|e| format!("crux: emit err {e}"))?
                .ok_or("crux: emit produced no object")?;
        let so_path = dir.join(format!("{content_hash:032x}_{ABI_TAG:08x}.so"));
        link_object_to_so(&obj, &so_path).map_err(|e| format!("crux: link err {e}"))?;
    }

    // Cross-session drift: grow the intern table AFTER all emits so the load-session
    // obarray + SymId space differs from emit (a baked emit-time SymId would be stale).
    for i in 0..96 {
        let _ = intern(&format!("aot-spec-decoy-{i}"));
    }

    // PASS 2a — SERVE each armed body against a FRESH obarray (same binding), and
    // prove FAST-from-call-1 + AOT==interp.
    for c in &corpus {
        let mut load_ctx = Context::new();
        alias(&mut load_ctx, c.alias, c.builtin)?;
        let ctx = &mut load_ctx as *mut Context;
        let g = mk_fn(c.alias, c.arity, &c.pushes);
        let g_val = Value::make_bytecode(g.clone());

        let fast0 = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed);
        let aot = super::cache::try_run_compiled(ctx, &g, g_val, &c.args)
            .map_err(|_| format!("{}: aot run raised", c.label))?
            .ok_or_else(|| format!("{}: aot run None (spec .so did not serve?)", c.label))?;
        if super::cache::cached_leaf_is_aot_for_func(&g) != Some(true) {
            return Err(format!("{}: not served AOT-backed", c.label));
        }
        // FAST-FROM-CALL-1: the loader armed the slot, so the very first call takes
        // the armed fast shim (dlopen resolved it, else this shim call would abort).
        let fast1 = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed);
        if fast1 <= fast0 {
            return Err(format!(
                "{}: FAST shim did not fire from call 1 (armed spec not served); fast {fast0}->{fast1}",
                c.label
            ));
        }
        // Result equals the interpreter.
        let interp = {
            let h = mk_fn(c.alias, c.arity, &c.pushes);
            let mut vm = Vm::from_context(&mut load_ctx);
            vm.execute(&h, c.args.clone())
                .map_err(|_| format!("{}: interp raised", c.label))?
        };
        if !crate::emacs_core::value::equal_value(&Value::from_bits(aot), &interp, 0) {
            return Err(format!(
                "{}: AOT {:?} != interp {:?}",
                c.label,
                Value::from_bits(aot),
                interp
            ));
        }
        super::cache::clear();
    }

    // ---- PASS 2b (b/c/d) THE CRUX: load the recordp-baked body against an obarray
    // where the SAME alias is bound to `stringp` (a different subr). The baked
    // PredRecordp site must DISARM (not run `is_record`): on a STRING, the generic
    // `stringp` returns `t` where a wrongly-armed `is_record` would return `nil`.
    {
        let mut load_ctx = Context::new();
        alias(&mut load_ctx, "aot-spec-crux", "stringp")?;
        // Sanity: the load-session binding IS `stringp` now — resolve the actual
        // subr NAME its cell holds, so a mismatch is diagnosable.
        {
            let bind = load_ctx
                .obarray
                .symbol_function_id(intern("aot-spec-crux"))
                .ok_or("crux SETUP: aot-spec-crux unbound in load_ctx")?;
            let name = crate::emacs_core::eval::subr_entry_from_value(bind)
                .map(|(s, _)| crate::emacs_core::intern::resolve_sym(s).to_string())
                .unwrap_or_else(|| "<not-a-subr>".to_string());
            if name != "stringp" {
                return Err(format!(
                    "crux SETUP: after alias→stringp, aot-spec-crux resolves to '{name}' \
                     (expected stringp) — re-alias did not take"
                ));
            }
        }
        let ctx = &mut load_ctx as *mut Context;
        let g = mk_fn("aot-spec-crux", 1, &[Op::StackRef(1)]);
        let g_val = Value::make_bytecode(g.clone());
        let arg = Value::string("a-string");

        let fast0 = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed);
        let gen0 = SUBR_SPEC_GENERIC_COUNT.load(Ordering::Relaxed);
        let aot = super::cache::try_run_compiled(ctx, &g, g_val, &[arg])
            .map_err(|_| "crux: aot run raised".to_string())?
            .ok_or("crux: aot run None")?;
        if super::cache::cached_leaf_is_aot_for_func(&g) != Some(true) {
            return Err("crux: not served AOT-backed".into());
        }
        let aot_v = Value::from_bits(aot);
        // Ground truth for the CURRENT (stringp) binding: interp of the same body.
        let interp = {
            let h = mk_fn("aot-spec-crux", 1, &[Op::StackRef(1)]);
            let mut vm = Vm::from_context(&mut load_ctx);
            vm.execute(&h, vec![Value::string("a-string")])
                .map_err(|_| "crux: interp raised".to_string())?
        };
        // The RE-CLASSIFY-DISARM property: the AOT result equals the interpreter of
        // the LIVE (`stringp`) binding — `t` on a string — NOT the baked
        // `is_record`'s `nil`. A naive "builtin+arity" arm would run `is_record` and
        // return nil (≠ interp), so this catches an arm-the-wrong-op regression.
        if aot_v != interp || aot_v != Value::T {
            return Err(format!(
                "crux: DISARM FAILED — AOT {aot_v:?}, interp {interp:?} (stringp('a-string')); \
                 expected both t. A wrongly-armed is_record on a string returns nil."
            ));
        }
        // The DISARMED slot took the GENERIC path (not FAST) on this first call.
        let fast1 = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed);
        let gen1 = SUBR_SPEC_GENERIC_COUNT.load(Ordering::Relaxed);
        if fast1 != fast0 {
            return Err("crux: FAST shim fired on a DISARMED site (should be generic)".into());
        }
        if gen1 <= gen0 {
            return Err("crux: GENERIC counter did not move on the disarmed site".into());
        }
        // (d) The DISARMED slot NEVER re-arms: many more calls (each a cache HIT on
        // the same leaf) keep FAST at 0 and move GENERIC each time (the
        // SPEC_EPOCH_DISARMED short-circuit — a merely epoch-stale slot would
        // re-validate + re-arm and FAST would move).
        for _ in 0..5 {
            let _ = super::cache::try_run_compiled(
                ctx,
                &g,
                Value::make_bytecode(g.clone()),
                &[Value::string("s")],
            )
            .map_err(|_| "crux: repeated disarmed run raised".to_string())?
            .ok_or("crux: repeated disarmed run None")?;
        }
        let fast2 = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed);
        let gen2 = SUBR_SPEC_GENERIC_COUNT.load(Ordering::Relaxed);
        if fast2 != fast0 {
            return Err(format!(
                "crux: DISARMED slot RE-ARMED across repeated calls (fast {fast0}->{fast2})"
            ));
        }
        if gen2 < gen1 + 5 {
            return Err(format!(
                "crux: GENERIC did not move on every repeated disarmed call ({gen1}->{gen2})"
            ));
        }
        super::cache::clear();
    }

    Ok(())
}

/// R2 increment C (AOT PGO persistence) — STEP 1 GO/NO-GO round-trip proof.
///
/// Proves the win the PGO drain will bank SURVIVES the runtime-emit →
/// next-session-load path, using the EXACT producer the drain calls:
/// [`compile_leaf_to_object`] with the LIVE obarray (NOT
/// [`build_baseline_object_for_leaf`] directly — the unified tier-select computes
/// `spec_forced` and picks the tier the LOADER re-derives via
/// `live_reloc_for_emit_tier`, so emit + load agree on the reloc collector). A
/// pred-class body `(recordp x)` is emitted via that producer with an obarray
/// aliasing the callee to `recordp` (so the `Op::Call` pred spec fast path bakes),
/// placed in `NEOVM_AOT_DIR` under the unit-index naming BEFORE any load (the index
/// is a OnceLock frozen at the first `load_unit`), then LOADED against a FRESH
/// obarray + a grown intern table through the PUBLIC [`super::cache::try_run_compiled`]
/// under `NEOVM_AOT=force`. Asserts, from CALL 1: (i) served AOT-backed, (ii) the
/// pred FAST shim fires at heat 0 (`SUBR_SPEC_FAST_COUNT` moves), (iii) result ==
/// interp.
///
/// Invoked from `tests/aot_pgo.rs` (a shim-exporting `-rdynamic` integration binary,
/// so the `neovm_jit_pred_spec` import resolves at dlopen).
#[doc(hidden)]
#[cfg(all(target_os = "linux", debug_assertions))]
pub fn testkit_pgo_roundtrip_selftest(dir: &std::path::Path) -> Result<(), String> {
    use super::compile::SUBR_SPEC_FAST_COUNT;
    use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::{SymId, intern};
    use crate::emacs_core::value::LambdaParams;
    use std::sync::atomic::Ordering;

    if !aot_enabled() {
        return Err("NEOVM_AOT not enabled (env must be set before any AOT call)".into());
    }

    // Alias a USER symbol to a builtin's function cell (control the callee binding
    // per "session"), mirroring `testkit_spec_aot_selftest`.
    let alias = |ev: &mut Context, alias_name: &str, builtin_name: &str| -> Result<(), String> {
        let f = ev
            .obarray
            .symbol_function_id(intern(builtin_name))
            .ok_or_else(|| format!("builtin '{builtin_name}' unbound"))?;
        ev.obarray.set_symbol_function(alias_name, f);
        Ok(())
    };
    // Build `(alias arg…)`: Constant(alias) then `nargs` pushes then Call(nargs).
    let mk_fn = |alias_name: &str, arity: usize, pushes: &[Op]| -> ByteCodeFunction {
        let mut ops = vec![Op::Constant(0)];
        ops.extend_from_slice(pushes);
        ops.push(Op::Call(pushes.len() as u16));
        ops.push(Op::Return);
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: (0..arity).map(|i| SymId(1 + i as u32)).collect(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops;
        f.constants = vec![Value::symbol(intern(alias_name))].into();
        f.max_stack = 16;
        f.seal_hand_assembled_ops();
        f
    };

    // PASS 1 — EMIT the pred `.so` via the UNIFIED producer (the drain's exact call),
    // BEFORE any load. Aliased to `recordp` so `find_spec_sites` bakes the pred spec
    // fast path + descriptor entry.
    {
        let mut emit_ctx = Context::new();
        alias(&mut emit_ctx, "aot-pgo-pred", "recordp")?;
        let f = mk_fn("aot-pgo-pred", 1, &[Op::StackRef(1)]);
        let (obj, content_hash) =
            compile_leaf_to_object(&f.ops, &f.constants, 1, Some(&emit_ctx.obarray))
                .map_err(|e| format!("pgo-roundtrip: emit err {e}"))?
                .ok_or(
                    "pgo-roundtrip: compile_leaf_to_object produced no object \
                     (not AOT-runnable / not spec-baked?)",
                )?;
        let so_path = dir.join(format!("{content_hash:032x}_{ABI_TAG:08x}.so"));
        link_object_to_so(&obj, &so_path).map_err(|e| format!("pgo-roundtrip: link err {e}"))?;
    }

    // Cross-session drift: grow the intern table AFTER the emit so the load-session
    // obarray + SymId space differs from emit (a baked emit-time SymId would be stale).
    for i in 0..96 {
        let _ = intern(&format!("aot-pgo-decoy-{i}"));
    }

    // PASS 2 — SERVE against a FRESH obarray (same binding); prove FAST-from-call-1
    // + AOT==interp through the PUBLIC try_run_compiled.
    let mut load_ctx = Context::new();
    alias(&mut load_ctx, "aot-pgo-pred", "recordp")?;
    let ctx = &mut load_ctx as *mut Context;
    let g = mk_fn("aot-pgo-pred", 1, &[Op::StackRef(1)]);
    let g_val = Value::make_bytecode(g.clone());
    let args = vec![Value::make_int(5)];

    let fast0 = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed);
    let aot = super::cache::try_run_compiled(ctx, &g, g_val, &args)
        .map_err(|_| "pgo-roundtrip: aot run raised".to_string())?
        .ok_or("pgo-roundtrip: aot run None (round-trip .so did not serve?)")?;
    if super::cache::cached_leaf_is_aot_for_func(&g) != Some(true) {
        return Err("pgo-roundtrip: not served AOT-backed".into());
    }
    // FAST-FROM-CALL-1: the loader armed the slot, so the very first call takes the
    // armed pred fast shim (dlopen resolved it, else this shim call would abort).
    let fast1 = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed);
    if fast1 <= fast0 {
        return Err(format!(
            "pgo-roundtrip: pred FAST shim did not fire from call 1 \
             (armed spec not served); fast {fast0}->{fast1}"
        ));
    }
    let interp = {
        let h = mk_fn("aot-pgo-pred", 1, &[Op::StackRef(1)]);
        let mut vm = Vm::from_context(&mut load_ctx);
        vm.execute(&h, args.clone())
            .map_err(|_| "pgo-roundtrip: interp raised".to_string())?
    };
    if !crate::emacs_core::value::equal_value(&Value::from_bits(aot), &interp, 0) {
        return Err(format!(
            "pgo-roundtrip: AOT {:?} != interp {:?}",
            Value::from_bits(aot),
            interp
        ));
    }
    super::cache::clear();
    Ok(())
}

/// R2 increment C — the DRAIN round-trip self-test: stage a proven-hot JIT pred
/// leaf, run the REAL [`drain_aot_pgo`] (env-gated on `NEOVM_AOT_PGO` +
/// `NEOVM_AOT_DIR`), and prove the persisted `.so` (i) lands under the correct
/// unit-index name, (ii) is NOT re-emitted on a second drain (the `.exists()` skip),
/// and (iii) serves a FRESH-obarray session AOT-backed + pred-FAST-from-call-1 +
/// result == interp.
///
/// The hot leaf is staged via `cache::compile_and_cache_jit_leaf` (NOT
/// `try_run_compiled`) so the drain runs BEFORE the process's first `load_unit`
/// freezes the OnceLock unit index — then the fresh-session `try_run_compiled` in
/// PASS 2 is that first `load_unit`, freezing the index WITH the drained `.so`
/// present. Invoked from `tests/aot_pgo.rs` (shim-exporting `-rdynamic` binary).
#[doc(hidden)]
#[cfg(all(target_os = "linux", debug_assertions))]
pub fn testkit_pgo_drain_selftest(dir: &std::path::Path) -> Result<(), String> {
    use super::compile::SUBR_SPEC_FAST_COUNT;
    use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::{SymId, intern};
    use crate::emacs_core::value::LambdaParams;
    use std::sync::atomic::Ordering;

    if !aot_enabled() {
        return Err("NEOVM_AOT not enabled".into());
    }
    if !aot_pgo_enabled() {
        return Err("NEOVM_AOT_PGO not enabled".into());
    }

    let alias = |ev: &mut Context, alias_name: &str, builtin_name: &str| -> Result<(), String> {
        let f = ev
            .obarray
            .symbol_function_id(intern(builtin_name))
            .ok_or_else(|| format!("builtin '{builtin_name}' unbound"))?;
        ev.obarray.set_symbol_function(alias_name, f);
        Ok(())
    };
    let mk_fn = |callee: &str| -> ByteCodeFunction {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
        f.constants = vec![Value::symbol(intern(callee))].into();
        f.max_stack = 16;
        f.seal_hand_assembled_ops();
        f
    };

    // ---- PASS 1 — stage a hot JIT pred leaf in ctx1, then DRAIN. ----
    let mut c1 = Context::new();
    alias(&mut c1, "pgo-drain-callee", "recordp")?;
    // Bind the fn under an interned name so the drain's obarray WALK finds it, and
    // retrieve the heap `bc` the walk will use (same identity → same compiled_id).
    c1.obarray.set_symbol_function(
        "pgo-drain-fn",
        Value::make_bytecode(mk_fn("pgo-drain-callee")),
    );
    let bc = c1
        .obarray
        .symbol_function_id(intern("pgo-drain-fn"))
        .and_then(|v| v.get_bytecode_data())
        .ok_or("pgo-drain-fn not bound to bytecode")?;
    super::cache::compile_and_cache_jit_leaf(bc, Some(&c1.obarray))
        .ok_or("staging: hot leaf did not JIT-compile")?;
    if super::cache::cached_leaf_is_aot_for_func(bc) != Some(false) {
        return Err("staging: leaf must be JIT-backed (not AOT) before drain".into());
    }

    let n = drain_aot_pgo(&c1);
    if n != 1 {
        return Err(format!("drain emitted {n} .so(s), expected 1"));
    }
    // Correct unit-index name.
    let hash = leaf_content_hash(&bc.ops, &bc.constants, 1).ok_or("content hash None")?;
    let expected = dir.join(format!("{hash:032x}_{ABI_TAG:08x}.so"));
    if !expected.exists() {
        return Err(format!(
            "drained .so not at expected name {}",
            expected.display()
        ));
    }
    // Second drain is a NO-OP (the `.exists()` skip): no duplicate `cc` spawn.
    let n2 = drain_aot_pgo(&c1);
    if n2 != 0 {
        return Err(format!(
            "second drain emitted {n2}, expected 0 (.exists() skip)"
        ));
    }

    // ---- PASS 2 — a FRESH session serves the drained `.so` AOT + FAST-from-call-1. ----
    super::cache::clear();
    let mut c2 = Context::new();
    alias(&mut c2, "pgo-drain-callee", "recordp")?;
    let g = mk_fn("pgo-drain-callee");
    let g_val = Value::make_bytecode(g.clone());
    let ctx2 = &mut c2 as *mut Context;
    let args = vec![Value::make_int(5)];

    let fast0 = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed);
    let aot = super::cache::try_run_compiled(ctx2, &g, g_val, &args)
        .map_err(|_| "pgo-drain: aot run raised".to_string())?
        .ok_or("pgo-drain: aot run None (drained .so did not serve?)")?;
    if super::cache::cached_leaf_is_aot_for_func(&g) != Some(true) {
        return Err("pgo-drain: not served AOT-backed next session".into());
    }
    let fast1 = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed);
    if fast1 <= fast0 {
        return Err(format!(
            "pgo-drain: pred FAST shim did not fire from call 1; fast {fast0}->{fast1}"
        ));
    }
    let interp = {
        let h = mk_fn("pgo-drain-callee");
        let mut vm = Vm::from_context(&mut c2);
        vm.execute(&h, args.clone())
            .map_err(|_| "pgo-drain: interp raised".to_string())?
    };
    if !crate::emacs_core::value::equal_value(&Value::from_bits(aot), &interp, 0) {
        return Err(format!(
            "pgo-drain: AOT {:?} != interp {:?}",
            Value::from_bits(aot),
            interp
        ));
    }
    super::cache::clear();
    Ok(())
}

/// R2 increment C — DEFAULT-OFF proof: with `NEOVM_AOT_PGO` UNSET, [`drain_aot_pgo`]
/// writes NOTHING even though a hot JIT leaf is staged AND `NEOVM_AOT_DIR` is set —
/// no surprise cache files in the default config. Invoked from `tests/aot_pgo.rs`
/// in a process that deliberately does NOT set `NEOVM_AOT_PGO`.
#[doc(hidden)]
#[cfg(target_os = "linux")]
pub fn testkit_pgo_default_off_selftest(dir: &std::path::Path) -> Result<(), String> {
    use crate::emacs_core::eval::Context;

    if aot_pgo_enabled() {
        return Err("NEOVM_AOT_PGO must be UNSET for the default-off proof".into());
    }

    let mut c1 = Context::new();
    let f = ev_alias_and_build(&mut c1)?;
    super::cache::compile_and_cache_jit_leaf(f, Some(&c1.obarray))
        .ok_or("staging: hot leaf did not JIT-compile")?;
    // There IS a drainable hot leaf — so a no-op below is the GATE, not an empty set.
    if super::cache::jit_compiled_ids().is_empty() {
        return Err("staging: no hot leaf in the JIT set".into());
    }

    let n = drain_aot_pgo(&c1);
    if n != 0 {
        return Err(format!("default-off drain emitted {n} .so(s), expected 0"));
    }
    let so_files = std::fs::read_dir(dir)
        .map_err(|e| format!("read_dir: {e}"))?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "so"))
        .count();
    if so_files != 0 {
        return Err(format!(
            "default-off left {so_files} .so file(s) in the dir"
        ));
    }
    super::cache::clear();
    Ok(())
}

/// Shared setup for the default-off self-test: alias a callee to `recordp` and bind
/// an interned `(callee x)` pred fn, returning the heap `bc` the drain walk sees.
#[cfg(target_os = "linux")]
fn ev_alias_and_build(
    c: &mut crate::emacs_core::eval::Context,
) -> Result<&'static crate::emacs_core::bytecode::ByteCodeFunction, String> {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::intern::{SymId, intern};
    use crate::emacs_core::value::LambdaParams;
    let cell = c
        .obarray
        .symbol_function_id(intern("recordp"))
        .ok_or("builtin 'recordp' unbound")?;
    c.obarray.set_symbol_function("pgo-off-callee", cell);
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
    f.constants = vec![Value::symbol(intern("pgo-off-callee"))].into();
    f.max_stack = 16;
    c.obarray
        .set_symbol_function("pgo-off-fn", Value::make_bytecode(f));
    c.obarray
        .symbol_function_id(intern("pgo-off-fn"))
        .and_then(|v| v.get_bytecode_data())
        .ok_or_else(|| "pgo-off-fn not bound to bytecode".to_string())
}

/// One AOT compilation unit's on-disk location, keyed by content hash.
type UnitIndex = std::collections::HashMap<u128, std::path::PathBuf>;

/// Process-wide index of available AOT `.so`s by content hash, built once from
/// `NEOVM_AOT_DIR` (default: none → AOT disabled). Memoized loaded units are
/// thread-local (the cache + leaves are `!Send`); the INDEX is shareable (just
/// paths). Indexes only files whose name carries the CURRENT `ABI_TAG`.
fn unit_index() -> &'static UnitIndex {
    static INDEX: std::sync::OnceLock<UnitIndex> = std::sync::OnceLock::new();
    INDEX.get_or_init(|| {
        let mut idx = UnitIndex::new();
        let Some(dir) = std::env::var_os("NEOVM_AOT_DIR") else {
            return idx;
        };
        let tag_suffix = format!("_{ABI_TAG:08x}.so");
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let path = e.path();
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                // Expect `<hash:032x>_<tag:08x>.so` (the entry-symbol stem). Only
                // index files matching the current ABI_TAG.
                if !name.ends_with(&tag_suffix) {
                    continue;
                }
                let stem = &name[..name.len() - tag_suffix.len()];
                if stem.len() == 32
                    && let Ok(hash) = u128::from_str_radix(stem, 16)
                {
                    idx.insert(hash, path);
                }
            }
        }
        idx
    })
}

thread_local! {
    /// Thread-local memo of units already `dlopen`'d on THIS thread, keyed by
    /// content hash. The `Arc<LoadedUnit>` keeps the `.so` mapped; cloned into
    /// every `CompiledLeaf` it backs. `!Send` (so per-thread), matching the
    /// thread-local `COMPILED` cache.
    ///
    /// COUPLING (audit #2): this memo is APPEND-ONLY — never cleared, even when
    /// `cache::clear()` drops the COMPILED leaves (and their per-leaf `Arc`s) on a
    /// heap-identity change. Safety does NOT depend on that: each `CompiledLeaf`
    /// holds its OWN `Arc<LoadedUnit>` (`_backing`), so its `entry` is valid for
    /// as long as it is cached regardless of this memo. The memo is purely a
    /// per-thread dlopen-dedup. INVARIANT for any future pruner: this map must
    /// outlive every COMPILED leaf that points into the same `.so` — never prune
    /// a unit while a leaf backed by it is still cached (today: never prune at
    /// all). The cost of append-only is a bounded-per-distinct-hash leak of
    /// mapped `.so` images across image reloads.
    static LOADED_UNITS: std::cell::RefCell<
        std::collections::HashMap<u128, std::sync::Arc<super::compile::LoadedUnit>>,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// dlopen (memoized) the unit for `content_hash` from the unit index, returning
/// the shared `Arc<LoadedUnit>`. `None` if no `.so` is indexed for this hash.
fn load_unit(content_hash: u128) -> Option<std::sync::Arc<super::compile::LoadedUnit>> {
    if let Some(u) = LOADED_UNITS.with(|m| m.borrow().get(&content_hash).cloned()) {
        return Some(u);
    }
    // Test seam: a directly-injected unit (see `test_support::inject_unit`) takes
    // precedence over the env-driven index, so a test can pre-load a `.so` it
    // just built and exercise the cache path.
    #[cfg(test)]
    if let Some(u) = test_support::injected_unit(content_hash) {
        return Some(u);
    }
    let path = unit_index().get(&content_hash)?;
    // SAFETY: dlopen of a `.so` we emitted; its undefined imports are the
    // `neovm_jit_*` shims, bound against the -rdynamic host. The library is
    // never unloaded while any backed leaf is cached (held by the Arc).
    let lib = unsafe { libloading::Library::new(path) }.ok()?;
    let unit = std::sync::Arc::new(super::compile::LoadedUnit::new(lib));
    LOADED_UNITS.with(|m| {
        m.borrow_mut()
            .insert(content_hash, std::sync::Arc::clone(&unit));
    });
    Some(unit)
}

// ---------------------------------------------------------------------------
// R2-C1/C2: the dump-time PRELOAD — ONE `.so` (all loadup leaves) beside the
// running executable, validated by a manifest fingerprint interlock + loaded
// with RTLD_NOW (fail-closed). Distinct from the per-hash NEOVM_AOT_DIR index
// above (R1c): the preload is a single unit serving every loadup entry by dlsym.
// ---------------------------------------------------------------------------

/// Path of the preload `.so` beside `exe` (same dir as the pdump, by
/// construction — the dump-time producer wrote it next to `neomacs`).
pub fn preload_so_path_for_executable(exe: &std::path::Path) -> std::path::PathBuf {
    exe.parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(PRELOAD_SO_NAME)
}

/// Path of the preload manifest beside `exe`.
pub fn preload_manifest_path_for_executable(exe: &std::path::Path) -> std::path::PathBuf {
    exe.parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(PRELOAD_MANIFEST_NAME)
}

/// Parse + validate the preload manifest at `manifest_path` against THIS image.
/// Returns `true` only when the manifest header is well-formed AND its
/// `version`, `abi_tag`, and `fingerprint` all match the running build — the
/// STALE INTERLOCK: a foreign / stale / ABI-incompatible preload fails here so
/// the loader skips it (→ JIT), never mis-serving native code built for a
/// different image. Any parse/IO/mismatch → `false` (logged at debug). The v2
/// pre-key section is NOT part of the interlock (a manifest with a valid header
/// but a corrupt pre-key list still serves the `.so`; only the pre-FILTER is
/// disabled — see [`load_preload_prekeys`]).
fn preload_manifest_matches(manifest_path: &std::path::Path) -> bool {
    let text = match std::fs::read_to_string(manifest_path) {
        Ok(t) => t,
        Err(e) => {
            tracing::debug!("aot-preload: manifest read failed ({e}); skip→JIT");
            return false;
        }
    };
    manifest_interlock_ok(&parse_preload_manifest(&text))
}

/// Load the v2 pre-key map for the preload beside the running executable
/// (task #11). `None` when there is no manifest, the interlock fails, or the
/// pre-key section is absent/malformed — in every case the caller
/// ([`prepopulate_aot_from_preload`]) falls back to the exact pre-v2 per-fn
/// hash+dlsym path (FAIL-CLOSED: the pre-filter can only be an optimization,
/// never the difference between serving and not serving a leaf the `.so` has).
/// Re-reads + re-interlocks the manifest independently of [`load_preload`]'s
/// validation: prepopulate runs once per startup, and a manifest swapped in
/// between the two reads either carries the same fingerprint (same image →
/// consistent pre-keys) or fails the interlock here (→ no pre-filter).
fn load_preload_prekeys() -> Option<PreKeyMap> {
    // Test seam: directly-injected pre-keys let a unit test drive the pre-filter
    // without a real manifest beside the test binary.
    #[cfg(test)]
    if let Some(injected) = test_support::injected_prekeys() {
        return Some(injected);
    }
    let exe = std::env::current_exe().ok()?;
    let manifest_path = preload_manifest_path_for_executable(&exe);
    let text = std::fs::read_to_string(&manifest_path).ok()?;
    let parsed = parse_preload_manifest(&text);
    if !manifest_interlock_ok(&parsed) {
        return None;
    }
    if parsed.prekeys.is_none() {
        tracing::debug!(
            "aot-preload: manifest pre-key section absent/malformed; \
             prepopulate hashes every candidate (fail-closed)"
        );
    }
    parsed.prekeys
}

thread_local! {
    /// The preload unit (one `.so` serving all loadup entries), `dlopen`'d once
    /// per thread on first [`load_preload`]. `Some(None)` records a checked miss
    /// (no/invalid preload) so we do not re-probe the filesystem every call.
    static PRELOAD_UNIT: std::cell::RefCell<
        Option<Option<std::sync::Arc<super::compile::LoadedUnit>>>,
    > = const { std::cell::RefCell::new(None) };
}

/// R2-C2: load (once per thread) the validated preload `.so` beside the running
/// executable. Returns the shared unit, or `None` when there is no preload, the
/// manifest interlock fails, or `dlopen` fails — every case a clean skip→JIT
/// (strictly additive). `dlopen` uses **RTLD_NOW | RTLD_LOCAL** so all 35
/// `neovm_jit_*` imports resolve UP FRONT: an unresolvable import fails the
/// `dlopen` (→ `None` → JIT) instead of aborting the process on the first shim
/// call (the RTLD_LAZY abort the #32 audit flagged).
pub(crate) fn load_preload() -> Option<std::sync::Arc<super::compile::LoadedUnit>> {
    // Test seam: a directly-injected preload unit (+ a fingerprint that must
    // match the running image) takes precedence, so a test can drive the
    // prepopulate path — and the fingerprint-MISMATCH path — without a real `.so`
    // beside the test binary.
    #[cfg(test)]
    if let Some(injected) = test_support::injected_preload() {
        return injected;
    }
    if let Some(memo) = PRELOAD_UNIT.with(|m| m.borrow().clone()) {
        return memo;
    }
    let resolved = load_preload_uncached();
    PRELOAD_UNIT.with(|m| *m.borrow_mut() = Some(resolved.clone()));
    resolved
}

// Open the emitted preload library. On unix we want RTLD_NOW|RTLD_LOCAL
// (#32-audit fix): resolve all imports up front so an unresolvable shim
// fails the open → skip→JIT, not an abort on first call. Windows LoadLibrary
// binds imports eagerly at load time, matching that intent; the preload is a
// Linux `.so` artifact that never exists beside a Windows image (the
// `so_path.exists()` guard skips this path there), but it must still compile.
std::cfg_select! {
    unix => {
        unsafe fn dlopen_preload(
            so_path: &std::path::Path,
        ) -> Result<libloading::Library, libloading::Error> {
            unsafe {
                libloading::os::unix::Library::open(
                    Some(so_path),
                    libloading::os::unix::RTLD_NOW | libloading::os::unix::RTLD_LOCAL,
                )
            }
            .map(libloading::Library::from)
        }
    }
    _ => {
        unsafe fn dlopen_preload(
            so_path: &std::path::Path,
        ) -> Result<libloading::Library, libloading::Error> {
            unsafe { libloading::Library::new(so_path) }
        }
    }
}

/// The uncached resolve + validate + dlopen core of [`load_preload`].
fn load_preload_uncached() -> Option<std::sync::Arc<super::compile::LoadedUnit>> {
    let exe = std::env::current_exe().ok()?;
    let so_path = preload_so_path_for_executable(&exe);
    let manifest_path = preload_manifest_path_for_executable(&exe);
    if !so_path.exists() || !manifest_path.exists() {
        return None; // no preload built for this image — pure JIT.
    }
    if !preload_manifest_matches(&manifest_path) {
        return None; // stale/foreign/ABI-mismatch — skip→JIT (never mis-serve).
    }
    // dlopen with eager import resolution (see `dlopen_preload`): an
    // unresolvable shim fails the open → skip→JIT, not an abort on first call.
    // SAFETY: a `.so` we emitted; its undefined imports are the `neovm_jit_*`
    // shims, bound against the -rdynamic host. The Arc keeps it mapped for the
    // lifetime of every leaf it backs.
    let lib = unsafe { dlopen_preload(&so_path) }
        .map_err(|e| {
            tracing::warn!(
                "aot-preload: dlopen {} failed ({e}); skip→JIT",
                so_path.display()
            );
            e
        })
        .ok()?;
    let unit = std::sync::Arc::new(super::compile::LoadedUnit::new(lib));
    tracing::debug!("aot-preload: loaded {}", so_path.display());
    Some(unit)
}

/// Whether AOT loading is enabled this session. R1c proves the path in-test via
/// `NEOVM_AOT=force`; R2 wires the real dump-time pre-warm.
pub(crate) fn aot_enabled() -> bool {
    // Test seam: a thread-local override (see `test_support`) lets a unit test
    // exercise the cache AOT path without relying on a process-start env var
    // (the env reads are OnceLock-memoized, so they can't be set per-test).
    #[cfg(test)]
    if let Some(forced) = test_support::forced_enabled() {
        return forced;
    }
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("NEOVM_AOT").as_deref(),
            Ok("force") | Ok("1") | Ok("on")
        )
    })
}

/// R2 increment C: whether the AOT-PGO shutdown DRAIN is enabled this session
/// (`NEOVM_AOT_PGO`). Mirrors [`aot_enabled`]. OFF by default — the drain is a
/// no-op (no surprise cache files) unless explicitly opted in. Persisting the
/// drained `.so`s ALSO requires `NEOVM_AOT_DIR` (the drain's write target + the
/// only place the loader reads next session) and `NEOVM_AOT` set NEXT session to
/// serve them.
pub(crate) fn aot_pgo_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("NEOVM_AOT_PGO").as_deref(),
            Ok("force") | Ok("1") | Ok("on")
        )
    })
}

/// R1c-6: try to serve a leaf for the given bytecode source from AOT.
///
/// Computes the content hash, finds + dlopens the matching unit, dlsym's the
/// entry + descriptor, verifies the descriptor (magic/version/ABI_TAG), rebuilds
/// the live reloc consts, and constructs a pre-warmed [`CompiledLeaf`]. Returns
/// `None` on ANY miss/mismatch/error — the caller falls back to the JIT
/// (strictly additive). The returned reloc consts are rooted by the caller's
/// cache insertion (R1c-8: they live in `COMPILED`, walked by
/// `collect_jit_reloc_gc_roots`).
pub(crate) fn try_load_leaf(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    // Prewarm-stashed manifest hash (see Runtime::aot_manifest_hash): skips
    // the SHA-256 body hash for the marked loadup class. `None` → hash here.
    precomputed_hash: Option<u128>,
    // R2 increment B2: the LIVE obarray, for RE-CLASSIFYING the descriptor's
    // `Op::Call` spec sites at load. `None` (shim-free test bodies with a null
    // ctx) leaves every spec site disarmed (the leaf still serves, just generic).
    obarray: Option<&crate::emacs_core::symbol::Obarray>,
) -> Option<super::compile::CompiledLeaf> {
    if !aot_enabled() {
        return None;
    }
    let content_hash = match precomputed_hash {
        Some(h) => h,
        None => leaf_content_hash(ops, constants, arity)?,
    };
    let unit = load_unit(content_hash)?;
    load_leaf_from_unit(&unit, content_hash, arity, constants, obarray)
}

/// The dlsym + descriptor-decode + verify + construct core, factored out of
/// [`try_load_leaf`] so a test can drive it with a directly-loaded unit
/// (bypassing the env/index resolution). The leaf's `reloc_data` is built by
/// [`resolve_reloc_from_descriptor`] straight from the `.so`'s recipe —
/// symbols re-interned to their canonical objects and heap values eq-upgraded
/// to the function's own `constants` (audit #A shared identity) — which is
/// ALSO the verifier: any recipe value the live pool cannot account for
/// rejects the leaf (`None` → JIT). This replaced the load-time live-reloc
/// re-collection whose `build_mir` was the measured 13.5µs/leaf floor.
/// Returns `None` on any symbol miss / descriptor mismatch / arity mismatch /
/// pool mismatch — the caller falls back to JIT.
pub(crate) fn load_leaf_from_unit(
    unit: &std::sync::Arc<super::compile::LoadedUnit>,
    content_hash: u128,
    arity: usize,
    constants: &[Value],
    // R2 increment B2: the LIVE obarray for the loader's spec-site re-classify+arm
    // (threaded through to `from_aot`). `None` leaves every spec site disarmed.
    obarray: Option<&crate::emacs_core::symbol::Obarray>,
) -> Option<super::compile::CompiledLeaf> {
    let entry_name = aot_entry_symbol(content_hash);
    let desc_name = aot_descriptor_symbol(content_hash);

    // dlsym the entry + descriptor out of the SAME unit (so the entry points
    // into the library the Arc keeps mapped). Unified 4-param ABI (the 4th is the
    // *const LeafSidecar); the ptr is cast to *const u8 and called via
    // CompiledLeaf::invoke_native, which passes the leaf's own sidecar.
    type EntryFn =
        unsafe extern "C" fn(*mut u8, *const i64, *mut i64, *const core::ffi::c_void) -> i64;
    // Hardening (review): the descriptor lives in a `.so` from NEOVM_AOT_DIR — a
    // trust boundary. The data-object size is not available via dlsym, so we read
    // the FIXED header first, VALIDATE magic+version+ABI_TAG before trusting any
    // length field, then bound + checked-add the recipe length before the second
    // `from_raw_parts`. A foreign/corrupt blob whose first 12 bytes don't match
    // is rejected here (→ JIT fallback) without ever reading an attacker-chosen
    // length. (A blob that fakes a valid header but lies about recipe_len can
    // still over-read within the cap; that is acceptable for the in-process,
    // operator-controlled AOT dir — the cap bounds the damage and decode_descriptor
    // re-checks the recipe count.)
    // v2 (B2): the fixed header gained a `spec_count:u32` right before `recipe_len`,
    // and the blob gained a spec-section (`spec_count` × SPEC_SITE_BYTES) AFTER the
    // recipe — so HDR is 4 larger (=57) and `total` includes the spec-section.
    const HDR: usize = 4 + 4 + 4 + 8 + 8 + 5 + 8 + 4 + 4 + 8; // encode_descriptor v2 (=57)
    // A generous cap: a leaf's reloc recipe is tiny in practice. Rejects an absurd
    // length before it drives a huge over-read.
    const MAX_RECIPE_LEN: usize = 1 << 20; // 1 MiB
    // SAFETY: symbols we exported; the entry's ABI is the CompiledLeaf entry ABI.
    let (entry_ptr, desc_bytes): (*const u8, Vec<u8>) = unsafe {
        let lib = unit.library();
        let entry: libloading::Symbol<EntryFn> = lib.get(entry_name.as_bytes()).ok()?;
        let entry_ptr = *entry as *const u8;
        let desc_sym: libloading::Symbol<*const u8> = lib.get(desc_name.as_bytes()).ok()?;
        let desc_ptr = *desc_sym;
        // 1) Read + copy the fixed header.
        let hdr = std::slice::from_raw_parts(desc_ptr, HDR).to_vec();
        // 2) Validate magic/version/ABI_TAG BEFORE trusting recipe_len: reject a
        //    foreign blob without reading an attacker-chosen length.
        let magic = u32::from_le_bytes(hdr[0..4].try_into().ok()?);
        let version = u32::from_le_bytes(hdr[4..8].try_into().ok()?);
        let tag = u32::from_le_bytes(hdr[8..12].try_into().ok()?);
        if magic != DESC_MAGIC || version != DESC_VERSION || tag != ABI_TAG {
            return None;
        }
        // 3) spec_count + recipe_len: bound BOTH, then checked-add the total size
        //    (fixed header + recipe + spec-section). spec_count is at HDR-12..HDR-8.
        let spec_count = u32::from_le_bytes(hdr[HDR - 12..HDR - 8].try_into().ok()?);
        if spec_count > MAX_SPEC_SITES {
            return None;
        }
        let recipe_len = u64::from_le_bytes(hdr[HDR - 8..HDR].try_into().ok()?) as usize;
        if recipe_len > MAX_RECIPE_LEN {
            return None;
        }
        let spec_bytes = (spec_count as usize).checked_mul(SPEC_SITE_BYTES)?;
        let total = HDR.checked_add(recipe_len)?.checked_add(spec_bytes)?;
        let all = std::slice::from_raw_parts(desc_ptr, total).to_vec();
        (entry_ptr, all)
    };

    let desc = decode_descriptor(&desc_bytes)?;
    // Sanity: arity must match the call site's lambda list.
    if desc.meta.arity != arity {
        return None;
    }
    // Cap the untrusted max_depth (audit minor): from_aot sizes deopt_spill to
    // max_depth*8 bytes when has_precise_deopt, so a crafted/corrupt descriptor
    // with a giant max_depth would drive an OOM-abort instead of the documented
    // fail-closed None. The bytecode operand stack is a u16 (max_stack), so any
    // real leaf is far under this; a larger value is a corrupt/foreign artifact.
    const MAX_DEOPT_DEPTH: usize = u16::MAX as usize;
    if desc.meta.has_precise_deopt && desc.meta.max_depth > MAX_DEOPT_DEPTH {
        return None;
    }
    // Build reloc_data from the recipe, verifying against the live pool as we
    // go (see resolve_reloc_from_descriptor: symbols canonical by interning,
    // heap values eq-upgraded to the pool's own objects — audit #A preserved;
    // any value the pool cannot account for rejects the leaf → JIT).
    let reloc_data = resolve_reloc_from_descriptor(&desc, constants)?;
    // SAFETY: `entry_ptr` is the real native entry inside `unit`'s loaded `.so`;
    // `unit` is held by the returned leaf for its whole life (kept mapped).
    let leaf = unsafe {
        super::compile::CompiledLeaf::from_aot(
            entry_ptr,
            std::sync::Arc::clone(unit),
            desc.meta,
            reloc_data,
            // R2 increment B2: RE-CLASSIFY the descriptor's spec sites against the
            // LIVE obarray cell and arm/disarm each runtime SpecSlot. `None`
            // (test/testkit load without a live obarray) leaves every site disarmed.
            &desc.spec_sites,
            obarray,
        )
    };
    Some(leaf)
}

/// CHEAP preload-membership probe (Gap 4b fast-reject): does `unit` export the
/// entry symbol for `content_hash`? A `dlsym` miss is a bloom-filtered hash-table
/// probe (~µs) and is GROUND TRUTH for membership (the entry symbol embeds the
/// content hash + ABI_TAG), vs the per-leaf live-reloc collection
/// (`live_reloc_for_emit_tier` runs `build_mir`) it gates. Only symbol PRESENCE
/// is checked; the pointer is neither called nor retained.
fn unit_has_entry(unit: &super::compile::LoadedUnit, content_hash: u128) -> bool {
    let entry_name = aot_entry_symbol(content_hash);
    // SAFETY: a presence-only dlsym into a `.so` we emitted; the resolved address
    // is dropped immediately (the real load path re-resolves + type-checks it).
    unsafe {
        unit.library()
            .get::<*const u8>(entry_name.as_bytes())
            .is_ok()
    }
}

/// Stats from a prepopulate pass (logged so a degraded preload is visible).
#[derive(Debug, Clone, Copy, Default)]
pub struct PrepopulateStats {
    /// Required-only fns probed for membership: hash-probed (content hash
    /// succeeded → dlsym gate) or, under the v2 manifest pre-filter (task #11),
    /// name-probed non-members skipped WITHOUT hashing. Same count as the
    /// pre-filter-less pass for an unchanged image: every skip-counted fn is a
    /// dump-time-verified hashable non-member (it would have hashed OK and then
    /// dlsym-missed).
    pub candidates: usize,
    /// CompiledLeaves successfully loaded from the preload `.so`.
    pub loaded: usize,
    /// COLD slots actually filled in COMPILED (insert-if-absent — a slot already
    /// holding a hook-compiled JIT leaf is KEPT, so `inserted` ≤ `loaded`).
    pub inserted: usize,
    /// Probed fns not in the preload — counted at the manifest NAME pre-filter
    /// (task #11: a verified `x` pre-key, no hash paid) or at the CHEAP dlsym
    /// gate (Gap 4b, before any MIR build) — or that failed to load/verify
    /// (→ JIT).
    pub missed: usize,
}

std::thread_local! {
    /// compiled_id → preload-manifest content hash, filled at prewarm-marking
    /// time. Lets the cache-miss AOT consult dlsym the entry WITHOUT re-hashing
    /// the body (the SHA was part of the measured per-leaf load floor). A side
    /// table rather than a Runtime field: the 384-byte ByteCodeObj slot has no
    /// room (the const assert in tagged/gc.rs enforces it). Exact by
    /// construction: bytecode is immutable and marking verified the manifest
    /// prekey against the very object whose id keys this map.
    static PREWARM_HASHES: std::cell::RefCell<std::collections::HashMap<u64, u128>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The stashed manifest hash for a prewarm-marked function, by compiled id.
pub(crate) fn prewarm_hash_for(compiled_id: u64) -> Option<u128> {
    PREWARM_HASHES.with(|m| m.borrow().get(&compiled_id).copied())
}

/// LAZY prewarm (the production path): mark every loadup function the preload
/// manifest lists as a MEMBER (name + ops_len + arity prekey match) so
/// `dispatch` serves it via `Plan::Compiled` from call 1 — the leaf itself is
/// built on the first call by the cache-miss path's `try_load_leaf` AOT
/// consult (~13µs, paid only for functions actually called). The EAGER
/// `prepopulate_aot_from_preload` builds all ~1.2k leaves up front
/// (~16.5ms measured) and is kept for tests/benchmarks.
///
/// A marked function whose body hash no longer matches the preload (redefined
/// between dump and run beyond what the ops_len/arity prekey catches) falls
/// back to a one-time JIT compile at first call — the same path any hot
/// function takes.
pub fn mark_preload_members_prewarmed(ctx: &crate::emacs_core::eval::Context) -> (usize, usize) {
    if !aot_enabled() {
        return (0, 0);
    }
    let Some(prekeys) = load_preload_prekeys() else {
        return (0, 0);
    };
    let mut candidates = 0usize;
    let mut marked = 0usize;
    for (name_id, func_val) in ctx.obarray.interned_function_cells_with_names() {
        if !func_val.is_bytecode() {
            continue;
        }
        // Prekey miss / non-member skips BEFORE touching the function data:
        // once pdump stubs exist this obarray-wide sweep must not materialize
        // thousands of never-called functions at startup. (`candidates` now
        // counts prekey members rather than all required-only bytecode fns —
        // a diagnostic-only shift.)
        let Some(key) = prekeys.get(crate::emacs_core::intern::resolve_name(name_id)) else {
            continue;
        };
        if !key.member {
            continue;
        }
        let Some(bc) = func_val.get_bytecode_data() else {
            continue;
        };
        if !bc.params.optional.is_empty() || bc.params.rest.is_some() {
            continue;
        }
        candidates += 1;
        if key.ops_len == bc.executable_ops().len() && key.arity == bc.params.required.len() {
            bc.jit_runtime().mark_aot_prewarmed();
            PREWARM_HASHES.with(|m| {
                m.borrow_mut()
                    .insert(bc.jit_runtime().compiled_id_or_assign(), key.hash)
            });
            marked += 1;
        }
    }
    (candidates, marked)
}

/// R2-C3: PREPOPULATE the per-thread `COMPILED` cache from the preload `.so`, so
/// every AOT-eligible loadup function serves NATIVE FROM CALL 1 (no JIT warmup).
///
/// Walks `ctx`'s obarray for the SAME AOT-candidate set the dump-time producer
/// emitted (same enumerate + D0 filter + content hash), loads each leaf from the
/// single preload unit (eq-identical reloc consts from the LIVE function — #A),
/// and inserts it into `COMPILED` keyed by that function's `compiled_id`. The 13
/// dedup'd bodies (distinct fns, one shared `.so` entry) each get their OWN
/// `CompiledLeaf` (own `reloc_data` + `compiled_id`) pointing at the shared entry.
///
/// Task #11: candidates are pre-filtered by the v2 manifest PRE-KEYS (symbol
/// name → member?/ops-count/arity) so a dump-time-verified non-member skips its
/// SHA-256 content hash outright; members and anything absent/mismatched take
/// the exact hash + dlsym path (dlsym stays the membership ground truth).
///
/// CRITICAL ordering (R1a heap-identity guard): `cache::prepopulate_aot_leaves`
/// FIRST syncs `COMPILED_HEAP` to the current heap (clearing the then-empty
/// cache), THEN inserts — otherwise the first GC's `sync_cache_to_current_heap`
/// would see `None != current` and CLEAR every prepopulated leaf (working for
/// call 1, then silently gone). See that function.
///
/// Runs ONLY when [`aot_enabled`]; a missing/invalid preload is a clean no-op
/// (every function just JITs — strictly additive). Returns the stats.
pub fn prepopulate_aot_from_preload(ctx: &crate::emacs_core::eval::Context) -> PrepopulateStats {
    let mut stats = PrepopulateStats::default();
    if !aot_enabled() {
        return stats;
    }
    let Some(unit) = load_preload() else {
        // Loud on NEOVM_AOT=force (audit w0guiyma9 minor): a benchmark/gate must be
        // able to tell "no win because the preload didn't load (missing/stale/
        // fingerprint-mismatch)" apart from "no win because AOT doesn't help here".
        // Quiet otherwise (a missing preload is the expected default — pure JIT).
        if matches!(std::env::var("NEOVM_AOT").as_deref(), Ok("force")) {
            tracing::warn!(
                "aot-preload: NEOVM_AOT=force but NO usable preload loaded \
                 (missing/stale/fingerprint-mismatch) — all functions will JIT"
            );
        } else {
            tracing::debug!("aot-preload: no usable preload; all functions will JIT");
        }
        return stats;
    };

    // Collect (compiled_id, content_hash, arity, live_reloc) for every AOT
    // candidate. We re-walk the obarray here (rather than reuse the producer's
    // `LoadupLeaf`, which omits the runtime id) so we can key COMPILED by the
    // function's own `compiled_id_or_assign()`.
    struct Prep {
        compiled_id: u64,
        content_hash: u128,
        arity: usize,
        constants: Vec<Value>,
    }
    let mut preps: Vec<Prep> = Vec::new();
    // Task #11 manifest pre-filter: the v2 pre-key map (name → member?/ops_len/
    // arity/hash) lets a dump-time-verified NON-member skip its SHA-256 content
    // hash outright — after the Gap 4b dlsym fast-reject, that hash (run for all
    // ~2195 loadup candidates, only ~706 of them members) WAS the remaining
    // startup floor (~16.8ms). `None` (no/invalid/pre-v2 manifest) → the exact
    // pre-filter-less path below for every fn.
    let prekeys = load_preload_prekeys();
    // Gap 4b fast-reject: iterate the bound function cells STRAIGHT off the
    // obarray chunks (same interned_global + bound-ness filters as the previous
    // all_symbols→intern→symbol_function_id walk, minus its per-symbol
    // name→intern→slot round-trip — which profiling showed dominated this pass).
    // The NameId rides along unresolved; only fns that survive the bytecode +
    // required-only filters pay the name resolution (a registry read-lock).
    for (name_id, func_val) in ctx.obarray.interned_function_cells_with_names() {
        if !func_val.is_bytecode() {
            continue;
        }
        // Raw-header reject BEFORE materializing: a stub with optionals or
        // &rest can never join the required-only tier, so it need not be
        // built just to be skipped.
        if func_val.bytecode_params_required_only_probe() != Some(true) {
            continue;
        }
        let Some(bc) = func_val.get_bytecode_data() else {
            continue;
        };
        // Required-only (matches the producer's enumerate + the MIR pure tier).
        if !bc.params.optional.is_empty() || bc.params.rest.is_some() {
            continue;
        }
        let arity = bc.params.required.len();
        let ops = bc.executable_ops();
        // MANIFEST PRE-FILTER (task #11): skip WITHOUT hashing exactly when the
        // dump-time manifest carries a VERIFIED non-member pre-key for this
        // name — `x` class with matching ops-count + arity, i.e. the body still
        // has the shape the producer hashed, so the dlsym gate below would
        // miss. Everything else falls through to the exact pre-existing
        // hash+dlsym path (FAIL-CLOSED): an ABSENT name (unhashable at dump —
        // or a post-dump definition), an `m` key (member: the hash is needed to
        // name its entry symbol; dlsym stays the membership ground truth), and
        // any ops_len/arity MISMATCH (body redefined between pdump load and
        // prepopulate, e.g. by `after-pdump-load-hook`).
        if let Some(key) = prekeys
            .as_ref()
            .and_then(|map| map.get(crate::emacs_core::intern::resolve_name(name_id)))
            && !key.member
            && key.ops_len == ops.len()
            && key.arity == arity
        {
            stats.candidates += 1; // same outcome the hash+dlsym path counted.
            stats.missed += 1;
            continue;
        }
        let Some(content_hash) = leaf_content_hash(ops, &bc.constants, arity) else {
            continue;
        };
        stats.candidates += 1;
        // Same D0 gate the emitter used (so the candidate set matches the `.so`).
        // NOTE: we deliberately do NOT call `is_d0_aot_candidate` here — it would
        // run a FULL Cranelift compile + object emit per loadup fn (~hundreds of
        // them) at EVERY startup, defeating the whole point of a prewarmed preload.
        // The `unit_has_entry` dlsym probe IS the real gate: a non-candidate fn has
        // no entry in the preload `.so` (the producer skipped it), so its dlsym
        // misses → that fn just JITs.
        //
        // Gap 4b fast-reject (startup regression fix): the CHEAP membership gate
        // runs FIRST. ~2/3 of loadup fns (~1489 of ~2195 measured) are NOT in the
        // preload; paying `live_reloc_for_emit_tier` (which runs `build_mir`) per
        // miss regressed NEOVM_AOT=force startup by ~34ms. Only a membership HIT
        // pays the reloc collection below.
        if !unit_has_entry(&unit, content_hash) {
            stats.missed += 1; // cheap-gate miss (was: load_leaf_from_unit dlsym miss).
            continue;
        }
        preps.push(Prep {
            compiled_id: bc.jit_runtime().compiled_id_or_assign(),
            content_hash,
            arity,
            // The pool travels to the leaf build below (the descriptor recipe
            // resolves + verifies against it — no build_mir at load anymore).
            constants: bc.constants.as_slice().to_vec(),
        });
    }

    // Build the leaves (each from the shared preload unit) OUTSIDE the COMPILED
    // borrow, then hand them to the cache for the sync-first-insert-after step.
    let mut leaves: Vec<(u64, super::compile::CompiledLeaf)> = Vec::new();
    for p in &preps {
        // B2 fast-from-call-1: RE-CLASSIFY each leaf's spec sites against the LIVE
        // obarray (post-loadup) so a pred/subr-class body serves the armed FAST shim
        // from call 1 — no JIT warmup, no first-call re-arm.
        match load_leaf_from_unit(
            &unit,
            p.content_hash,
            p.arity,
            &p.constants,
            Some(&ctx.obarray),
        ) {
            Some(leaf) => leaves.push((p.compiled_id, leaf)),
            None => stats.missed += 1,
        }
    }
    stats.loaded = leaves.len();
    let inserted_ids = super::cache::prepopulate_aot_leaves(leaves);
    stats.inserted = inserted_ids.len();
    // Serve the prewarmed leaves FROM CALL 1: without this, `dispatch`'s heat
    // gate keeps ONE-SHOT startup elisp interpreted forever — the preload only
    // ever helped functions that independently became hot. Re-walk the bound
    // function cells and mark the runtimes whose leaves were just inserted
    // (a marked function with a filled cache slot runs native immediately; a
    // slot kept by an existing JIT leaf is already hot, so marking is a no-op).
    if !inserted_ids.is_empty() {
        let hash_by_id: std::collections::HashMap<u64, u128> = preps
            .iter()
            .map(|p| (p.compiled_id, p.content_hash))
            .collect();
        let inserted: std::collections::HashSet<u64> = inserted_ids.into_iter().collect();
        for (_name_id, func_val) in ctx.obarray.interned_function_cells_with_names() {
            // A pdump stub can own no compiled_id and was never hot: the
            // materialized-only peek keeps this sweep from forcing them.
            if let Some(bc) = func_val.bytecode_data_if_materialized()
                && let Some(id) = bc.jit_runtime().compiled_id()
                && inserted.contains(&id)
                && let Some(&hash) = hash_by_id.get(&id)
            {
                bc.jit_runtime().mark_aot_prewarmed();
                PREWARM_HASHES.with(|m| m.borrow_mut().insert(id, hash));
            }
        }
    }
    tracing::debug!(
        "aot-preload: prepopulated {} inserted / {} loaded / {} candidates ({} missed, {} slots already warm)",
        stats.inserted,
        stats.loaded,
        stats.candidates,
        stats.missed,
        stats.loaded - stats.inserted,
    );
    stats
}

// ---------------------------------------------------------------------------
// R2 increment C: PGO PERSISTENCE — drain proven-hot JIT leaves to NEOVM_AOT_DIR
// at shutdown so the NEXT session serves them native + speculative from call 1.
// EMIT-SIDE + shutdown-only: the load path (try_load_leaf → unit_index) already
// serves runtime-placed `.so`s (increments A + B2); C only WRITES them.
// ---------------------------------------------------------------------------

/// Hard cap on the number of `.so`s the shutdown drain emits in one session — the
/// shutdown-budget lever (hazard 4). Each emit spawns a `cc -shared` subprocess, so
/// an unbounded drain would regress shutdown latency. 128 covers the hottest tail a
/// session realistically JITs beyond the dump-time preload; the `.exists()` skip
/// means successive sessions CONVERGE on the full hot set (each drains a fresh
/// slice, hottest-first, of what is not yet persisted).
pub(crate) const PGO_DRAIN_CAP: usize = 128;

/// Monotonic counter for unique temp names in [`pgo_atomic_place`] (composed with
/// pid + nanos), so concurrent drainers writing the same dir never collide on the
/// intermediate artifact before the atomic rename.
static PGO_TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The FINAL on-disk path of a drained leaf `.so`, in the unit-index naming the
/// loader indexes: `{hash:032x}_{tag:08x}.so` (same convention as [`unit_index`]).
fn pgo_final_path(dir: &std::path::Path, content_hash: u128) -> std::path::PathBuf {
    dir.join(format!("{content_hash:032x}_{ABI_TAG:08x}.so"))
}

/// ATOMIC place: link `obj_bytes` into a UNIQUE temp `.so` in `dir` then `fs::rename`
/// it onto its final unit-index name. The rename is atomic on a single filesystem, so
/// a concurrent next-session loader (or a crash mid-drain) NEVER observes a torn /
/// partial `.so` under the indexed name — it sees either no file or the complete new
/// one. The temp `.so` (and `cc`'s own derived temp `.o`) carry pid + a monotonic
/// counter + nanos, so parallel drainers into a shared dir don't clobber each other's
/// intermediates; the `.tmp` extension also keeps them out of the loader's
/// `_<tag>.so` index until the rename lands.
fn pgo_atomic_place(
    obj_bytes: &[u8],
    dir: &std::path::Path,
    content_hash: u128,
) -> Result<std::path::PathBuf, CompileError> {
    use std::sync::atomic::Ordering;
    let final_path = pgo_final_path(dir, content_hash);
    let pid = std::process::id();
    let ctr = PGO_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_path = dir.join(format!(
        "{content_hash:032x}_{ABI_TAG:08x}.{pid}.{ctr}_{nanos}.tmp"
    ));
    link_object_to_so(obj_bytes, &tmp_path)?;
    std::fs::rename(&tmp_path, &final_path).map_err(|e| {
        // Best-effort cleanup so a failed drain leaves no dangling temp.
        let _ = std::fs::remove_file(&tmp_path);
        module_init_err(format!("pgo atomic rename → {}: {e}", final_path.display()))
    })?;
    Ok(final_path)
}

/// R2 increment C — the shutdown DRAIN entry point. A NO-OP unless BOTH
/// [`aot_pgo_enabled`] (`NEOVM_AOT_PGO`) AND `NEOVM_AOT_DIR` are set (so the default
/// config writes NOTHING — no surprise cache files). Persists this session's
/// proven-hot-but-not-yet-AOT JIT leaves so the NEXT session serves them native +
/// speculative from call 1. Returns the number of `.so`s emitted (0 when
/// disabled/no-op). MUST run on the eval thread (reads the thread-local `COMPILED`).
pub fn drain_aot_pgo(ctx: &crate::emacs_core::eval::Context) -> usize {
    if !aot_pgo_enabled() {
        return 0;
    }
    let Some(dir) = std::env::var_os("NEOVM_AOT_DIR") else {
        return 0;
    };
    drain_aot_pgo_to_dir(ctx, std::path::Path::new(&dir), PGO_DRAIN_CAP)
}

/// The testable core of [`drain_aot_pgo`] with an explicit target `dir` + `cap` (so
/// a test drives it without env / OnceLock races). Walks `ctx`'s obarray for the
/// SAME required-only interned-bytecode candidate set the dump-time producer + the
/// prepopulate pass use (mirror of [`prepopulate_aot_from_preload`]), INTERSECTS it
/// with the proven-hot JIT set ([`super::cache::jit_compiled_ids`] —
/// `Compiled && !is_aot_backed`), and emits at most `cap` `.so`s HOTTEST-FIRST via
/// the UNIFIED producer [`compile_leaf_to_object`] with the LIVE obarray — the SAME
/// call the loader's `live_reloc_for_emit_tier` mirrors, so a drained leaf loads +
/// arms IDENTICALLY next session (never `None` obarray → the spec-baking tier fires,
/// the whole point of C). FAIL-CLOSED: an already-present `.so` (`.exists()`), an
/// unhashable / non-canonical body, an `Ok(None)` (outside the AOT subset), or any
/// emit / place error is SKIPPED, never fatal. `cap` bounds `cc` spawns (the
/// shutdown-budget lever); the `.exists()` skip makes successive sessions converge.
pub(crate) fn drain_aot_pgo_to_dir(
    ctx: &crate::emacs_core::eval::Context,
    dir: &std::path::Path,
    cap: usize,
) -> usize {
    if cap == 0 {
        return 0;
    }
    let hot = super::cache::jit_compiled_ids();
    if hot.is_empty() {
        return 0;
    }
    // Collect the hot ∩ required-only-bytecode candidates with their heat, to emit
    // HOTTEST-FIRST under `cap`. Bodies are borrowed `'static` from the heap
    // (`get_bytecode_data`), so holding refs across the sort is sound.
    let mut cands: Vec<(
        u32,
        u128,
        &'static crate::emacs_core::bytecode::ByteCodeFunction,
        usize,
    )> = Vec::new();
    for (_name_id, func_val) in ctx.obarray.interned_function_cells_with_names() {
        if !func_val.is_bytecode() {
            continue;
        }
        // Never-materialized stubs were never hot: peek, don't force.
        let Some(bc) = func_val.bytecode_data_if_materialized() else {
            continue;
        };
        // Required-only (matches the producer's enumerate + the MIR pure tier).
        if !bc.params.optional.is_empty() || bc.params.rest.is_some() {
            continue;
        }
        // Hot ∩: only leaves this session PROVED hot AND the AOT tier did not already
        // serve. A peek (no id assignment for the never-compiled walked-past majority).
        let Some(id) = bc.jit_runtime().compiled_id() else {
            continue;
        };
        if !hot.contains(&id) {
            continue;
        }
        let arity = bc.params.required.len();
        let ops = bc.executable_ops();
        let Some(content_hash) = leaf_content_hash(ops, &bc.constants, arity) else {
            continue; // non-canonical / non-recipe-able → skip (fail-closed).
        };
        cands.push((bc.jit_runtime().heat(), content_hash, bc, arity));
    }
    // Hottest first (stable within equal heat = obarray order).
    cands.sort_by_key(|candidate| std::cmp::Reverse(candidate.0));

    let mut emitted = 0usize;
    for (_heat, content_hash, bc, arity) in cands {
        if emitted >= cap {
            break;
        }
        // Already persisted (this or a prior session) → skip WITHOUT a `cc` spawn.
        if pgo_final_path(dir, content_hash).exists() {
            continue;
        }
        let ops = bc.executable_ops();
        match compile_leaf_to_object(ops, &bc.constants, arity, Some(&ctx.obarray)) {
            Ok(Some((obj, h))) => {
                // content_hash == h by construction (both are leaf_content_hash of the
                // same body); place under the OBJECT's own hash so the filename the
                // loader indexes always matches the baked entry/descriptor symbols.
                debug_assert_eq!(h, content_hash, "drain: emit hash != pre-key hash");
                match pgo_atomic_place(&obj, dir, h) {
                    Ok(_) => emitted += 1,
                    Err(e) => tracing::debug!("aot-pgo: place failed for {h:032x}: {e}; skip"),
                }
            }
            // Ok(None) = outside the AOT subset; Err = emit failure. Fail-closed skip.
            Ok(None) => {}
            Err(e) => tracing::debug!("aot-pgo: emit err for {content_hash:032x}: {e}; skip"),
        }
    }
    if emitted > 0 {
        tracing::debug!(
            "aot-pgo: drained {emitted} hot leaf .so(s) to {}",
            dir.display()
        );
    }
    emitted
}

/// Build a relocatable object (`.o` bytes) for one pure MIR leaf `m`, exporting
/// its entry under `entry_name`.
///
/// Mirrors [`compile::lower_mir_pure`]'s analysis prologue (the same `has_call`
/// / `cons_repl` / `needs_rt` derivation, the same reloc-constant collection and
/// deopt-buffer sizing) and drives the SAME module-generic build seam
/// (`build_mir_leaf_fn`) with `M = ObjectModule` and `aot=true`. The result is
/// SEMANTICALLY equivalent to the JIT (same RESULTS), but the CLIF is NOT
/// byte-identical: under `aot=true` the session-specific bases (reloc + the
/// precise-deopt buffers) are LOADED from the per-thread `LeafSidecar` (4th entry
/// arg) instead of baked as `iconst`, and session-specific symbol constants join
/// the reloc set (audit #16). The other differences from the JIT path are the
/// three module seams: no `builder.symbol` (shims are `Linkage::Import`, host-
/// exported), `Linkage::Export` (vs `Local`) for the entry, and `finish().emit()`
/// (vs `finalize_definitions`/`get_finalized_function`).
pub fn build_object_for_leaf(
    m: &mir::MirFunction,
    entry_name: &str,
) -> Result<Vec<u8>, CompileError> {
    build_object_for_leaf_inner(m, entry_name, None)
}

/// As [`build_object_for_leaf`], but also emits an exported, read-only data
/// object `descriptor.0` holding `descriptor.1` bytes — the AOT descriptor the
/// loader dlsym's to recover the leaf's metadata + reloc rebuild recipe (R1c-3).
/// Create a fresh PIC `ObjectModule` for AOT emission (the `.o` must be
/// position-independent so the loader can relocate the linked `.so`). Shared by
/// the single-leaf [`build_object_for_leaf_inner`] and the multi-leaf
/// [`build_preload_object`] (R2-B4) — the latter defines N leaves into ONE module.
fn make_aot_object_module() -> Result<ObjectModule, CompileError> {
    let mut flag_builder = settings::builder();
    // Mirror cranelift-jit's flags, except is_pic=true (a JITModule needs
    // is_pic=false; a shared object needs true so the loader can relocate it).
    flag_builder
        .set("use_colocated_libcalls", "false")
        .map_err(|e| module_init_err(e.to_string()))?;
    flag_builder
        .set("is_pic", "true")
        .map_err(|e| module_init_err(e.to_string()))?;
    let isa_builder = cranelift_native::builder().map_err(|e| module_init_err(e.to_string()))?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|e| module_init_err(e.to_string()))?;
    let builder = ObjectBuilder::new(isa, "neovm_aot", default_libcall_names())
        .map_err(|e| module_init_err(e.to_string()))?;
    Ok(ObjectModule::new(builder))
}

/// Define ONE leaf (entry + optional descriptor) into an existing `module`
/// (`aot=true` lowering). Factored from [`build_object_for_leaf_inner`] so
/// multiple leaves can be defined into a single shared module (R2-B4) before one
/// `finish().emit()`. Does NOT finish the module. The `#[cfg(debug_assertions)]`
/// shim-import audit (#15) is on the FINISHED bytes, so it stays in the callers.
fn define_leaf_into_module(
    module: &mut ObjectModule,
    m: &mir::MirFunction,
    entry_name: &str,
    descriptor: Option<(&str, &[u8])>,
) -> Result<(), CompileError> {
    // ----- Analysis prologue: the ONE plan lower_mir_leaf_fn uses. -----------
    let plan = super::compile::lowering::plan_mir_leaf(m);

    // Precise-deopt spill buffer + cells (sized exactly as the JIT does). These
    // are this-session throwaway buffers in R1c-1 — their *addresses* get baked,
    // which is fine for the parse gate (load-time rebuild is R1c-3/5).
    let deopt_spill: Box<[core::cell::Cell<i64>]> = if plan.precise {
        (0..plan.max_depth)
            .map(|_| core::cell::Cell::new(0))
            .collect()
    } else {
        Box::from([])
    };
    let deopt_meta: Box<DeoptCells> = Box::new(DeoptCells {
        pc: core::cell::Cell::new(0),
        depth: core::cell::Cell::new(0),
        handlers: core::cell::Cell::new(0),
    });

    // R1a reloc-constant collection (dedup by tagged bits), identical to the JIT.
    // AOT reloc index, derived from the ONE collector (`collect_reloc_consts`)
    // so the index order is identical to the recipe order by construction (no
    // duplicated predicate/order to drift): reloc slot i ↔ recipe slot i ↔ the
    // rebuilt Vec at load. The lowering (build_mir_leaf_fn, aot=true) looks up
    // this index for each session-specific const (heap object / non-nil-t symbol).
    let reloc_vals = collect_reloc_consts(m);
    let reloc_index: std::collections::HashMap<usize, u32> = reloc_vals
        .iter()
        .enumerate()
        .map(|(i, v)| (v.bits(), i as u32))
        .collect();
    let reloc_data: Box<[Value]> = reloc_vals.into_boxed_slice();

    // ----- Drive the SHARED build seam with M = ObjectModule, aot=true. --------
    // SEMANTICALLY the JIT lowering (same results), but `aot=true` makes the
    // session-specific bases (reloc + deopt) load from the sidecar (4th arg) and
    // pulls symbol consts into the reloc set — so NOT byte-identical CLIF. The
    // entry is Linkage::Export under `entry_name` for the loader to dlsym; the
    // `neovm_jit_*` shims stay undefined Linkage::Import (host-exported) imports.
    super::compile::build_mir_leaf_fn(
        module,
        m,
        &deopt_spill,
        &deopt_meta,
        &reloc_data,
        &reloc_index,
        &plan,
        entry_name,
        Linkage::Export,
        /*aot=*/ true,
        /*entry_counter=*/ None, // AOT code never counts entries
    )?;

    // R1c-3: emit the descriptor as an exported, read-only data object so the
    // loader can dlsym it and recover the leaf metadata + reloc rebuild recipe.
    if let Some((desc_name, desc_bytes)) = descriptor {
        let data_id = module
            .declare_data(
                desc_name,
                Linkage::Export,
                /*writable=*/ false,
                /*tls=*/ false,
            )
            .map_err(|e| module_init_err(e.to_string()))?;
        let mut desc = cranelift_module::DataDescription::new();
        desc.define(desc_bytes.to_vec().into_boxed_slice());
        module
            .define_data(data_id, &desc)
            .map_err(|e| module_init_err(e.to_string()))?;
    }
    Ok(())
}

/// R2-E: prepare the baseline-tier AOT emit for a body the MIR tier rejects.
/// Returns the per-leaf reloc set (const-relocs plus named-builtin op-symbols,
/// #16/#17) together with the recipe bytes, or `None` if any const/symbol is
/// outside the recipe subset (gensym / non-recipe-able), in which case the
/// caller bails to the JIT. Shared by the single-leaf and (later) multi-leaf
/// baseline emit paths.
// Private handoff of the relocation values, lookup index, and encoded recipe.
#[allow(clippy::type_complexity)]
fn prepare_baseline_relocs(
    ops: &[Op],
    constants: &[Value],
) -> Option<(Box<[Value]>, std::collections::HashMap<usize, u32>, Vec<u8>)> {
    let (reloc_vals, reloc_index) = collect_baseline_aot_relocs(ops, constants)?;
    let mut recipe = Vec::new();
    for v in &reloc_vals {
        if write_value_recipe(&mut recipe, *v).is_err() {
            return None;
        }
    }
    Some((reloc_vals.into_boxed_slice(), reloc_index, recipe))
}

/// R2-E: define ONE baseline-tier AOT leaf into `module` — the analog of
/// [`define_leaf_into_module`] for bodies the MIR tier rejects. Drives
/// `build_baseline_leaf_object` (build_leaf_fn::<ObjectModule>(aot=true)) + emits
/// the descriptor data object. The descriptor's meta comes from the BASELINE
/// analysis (`cfg.max_depth`, has_handlers) — the baseline is all-precise deopt
/// (every guard is STATUS_DEOPT_AT), so `has_precise_deopt=true`,
/// `has_side_effects=false` (no rerun-from-start). Returns `Ok(None)` if the body
/// has a non-recipe-able const/symbol (caller stays JIT-only).
fn define_baseline_leaf_into_module(
    module: &mut ObjectModule,
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    entry_name: &str,
    desc_name: &str,
    // R2 increment B2: the LIVE obarray, so the baseline leaf bakes its `Op::Call`
    // subr/bytecode spec fast paths + descriptor entries. `None` → CBSym-only.
    obarray: Option<&crate::emacs_core::symbol::Obarray>,
) -> Result<Option<()>, CompileError> {
    let Some((reloc_data, reloc_index, recipe)) = prepare_baseline_relocs(ops, constants) else {
        return Ok(None);
    };
    let bmeta = super::compile::build_baseline_leaf_object(
        module,
        ops,
        constants,
        arity,
        &reloc_data,
        &reloc_index,
        entry_name,
        obarray,
    )?;
    let meta = super::compile::AotLeafMeta {
        arity: bmeta.arity,
        required: bmeta.arity,
        has_rest: false,
        has_binds: bmeta.has_binds,
        has_handlers: bmeta.has_handlers,
        has_side_effects: false,
        max_depth: bmeta.max_depth,
        has_precise_deopt: true,
    };
    // Bake the baseline leaf's `Op::Call` spec sites (in slot order) into the
    // descriptor so the loader can re-classify + arm each runtime SpecSlot.
    let desc_bytes = encode_descriptor(&meta, &recipe, reloc_data.len() as u32, &bmeta.spec_sites);
    let data_id = module
        .declare_data(
            desc_name,
            Linkage::Export,
            /*writable=*/ false,
            /*tls=*/ false,
        )
        .map_err(|e| module_init_err(e.to_string()))?;
    let mut desc = cranelift_module::DataDescription::new();
    desc.define(desc_bytes.into_boxed_slice());
    module
        .define_data(data_id, &desc)
        .map_err(|e| module_init_err(e.to_string()))?;
    Ok(Some(()))
}

/// R2-E: build a single-leaf relocatable object via the BASELINE tier (for a body
/// the MIR tier rejects). Returns `(object_bytes, content_hash)`, or `Ok(None)` if
/// the body is outside the recipe subset. Mirrors `build_object_for_leaf_inner`.
/// (Used by the E1a testkit now + the E1b real routing — `allow(dead_code)` until
/// the routing lands so a production build without the routing doesn't warn.)
#[allow(dead_code)]
pub(crate) fn build_baseline_object_for_leaf(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    obarray: Option<&crate::emacs_core::symbol::Obarray>,
) -> Result<Option<(Vec<u8>, u128)>, CompileError> {
    let Some(content_hash) = leaf_content_hash(ops, constants, arity) else {
        return Ok(None);
    };
    let entry_name = aot_entry_symbol(content_hash);
    let desc_name = aot_descriptor_symbol(content_hash);
    let mut module = make_aot_object_module()?;
    if define_baseline_leaf_into_module(
        &mut module,
        ops,
        constants,
        arity,
        &entry_name,
        &desc_name,
        obarray,
    )?
    .is_none()
    {
        return Ok(None);
    }
    let obj = module
        .finish()
        .emit()
        .map_err(|e| module_init_err(e.to_string()))?;
    assert_aot_imports_exported(&obj)?;
    Ok(Some((obj, content_hash)))
}

/// Build a single-leaf relocatable object: make a module, define the leaf, emit.
fn build_object_for_leaf_inner(
    m: &mir::MirFunction,
    entry_name: &str,
    descriptor: Option<(&str, &[u8])>,
) -> Result<Vec<u8>, CompileError> {
    let mut module = make_aot_object_module()?;
    define_leaf_into_module(&mut module, m, entry_name, descriptor)?;
    let obj = module
        .finish()
        .emit()
        .map_err(|e| module_init_err(e.to_string()))?;
    assert_aot_imports_exported(&obj)?;
    Ok(obj)
}

/// Test-only seams that let a unit test drive the cache AOT path (`aot_enabled`
/// and `load_unit`) without a process-start env var (the env reads are
/// OnceLock-memoized and so cannot be toggled per-test). Production builds never
/// compile this; `aot_enabled`/`load_unit` consult it only under `cfg(test)`.
#[cfg(test)]
pub(crate) mod test_support {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::sync::Arc;

    thread_local! {
        static FORCE_ENABLED: RefCell<Option<bool>> = const { RefCell::new(None) };
        static INJECTED: RefCell<HashMap<u128, Arc<super::super::compile::LoadedUnit>>> =
            RefCell::new(HashMap::new());
        /// Injected preload result for `load_preload`: `None` = not injected (fall
        /// through to the real resolver); `Some(inner)` = `load_preload` returns
        /// `inner` (so a test can model both a present preload AND a checked miss,
        /// e.g. a fingerprint-mismatch skip→JIT).
        static INJECTED_PRELOAD: RefCell<
            Option<Option<Arc<super::super::compile::LoadedUnit>>>,
        > = const { RefCell::new(None) };
        /// Injected v2 manifest pre-keys for `load_preload_prekeys` (task #11):
        /// `None` = not injected (fall through to the real resolver, which in a
        /// test binary finds no manifest → no pre-filter).
        static INJECTED_PREKEYS: RefCell<Option<super::PreKeyMap>> =
            const { RefCell::new(None) };
        /// `leaf_content_hash` call counter (task #11 probe seam): lets a test
        /// assert the manifest pre-filter skipped a candidate WITHOUT hashing.
        static HASH_CALLS: Cell<usize> = const { Cell::new(0) };
    }

    /// The forced `aot_enabled()` value, if a test set one.
    pub(crate) fn forced_enabled() -> Option<bool> {
        FORCE_ENABLED.with(|c| *c.borrow())
    }

    /// Force `aot_enabled()` to `v` for the rest of this thread (test only).
    pub(crate) fn set_forced_enabled(v: bool) {
        FORCE_ENABLED.with(|c| *c.borrow_mut() = Some(v));
    }

    /// Reset the test overrides (call at the end of a test to avoid bleed).
    pub(crate) fn reset() {
        FORCE_ENABLED.with(|c| *c.borrow_mut() = None);
        INJECTED.with(|m| m.borrow_mut().clear());
        INJECTED_PRELOAD.with(|c| *c.borrow_mut() = None);
        INJECTED_PREKEYS.with(|c| *c.borrow_mut() = None);
        HASH_CALLS.with(|c| c.set(0));
    }

    /// Inject a pre-loaded unit for `content_hash` so `load_unit` returns it.
    pub(crate) fn inject_unit(content_hash: u128, unit: Arc<super::super::compile::LoadedUnit>) {
        INJECTED.with(|m| {
            m.borrow_mut().insert(content_hash, unit);
        });
    }

    /// The injected unit for `content_hash`, if any.
    pub(crate) fn injected_unit(
        content_hash: u128,
    ) -> Option<Arc<super::super::compile::LoadedUnit>> {
        INJECTED.with(|m| m.borrow().get(&content_hash).cloned())
    }

    /// Inject the result `load_preload` should return (the present-preload path).
    /// A test builds a unit + injects it to drive prepopulate without a real `.so`
    /// beside the test binary.
    pub(crate) fn inject_preload(unit: Arc<super::super::compile::LoadedUnit>) {
        INJECTED_PRELOAD.with(|c| *c.borrow_mut() = Some(Some(unit)));
    }

    /// Inject a `load_preload` MISS (the stale-interlock / no-preload path) so a
    /// test can assert `prepopulate_aot_from_preload` cleanly does nothing → JIT.
    pub(crate) fn inject_preload_miss() {
        INJECTED_PRELOAD.with(|c| *c.borrow_mut() = Some(None));
    }

    /// The injected `load_preload` result, if a test set one. Outer `Some` means
    /// "injected" (use the inner value); `None` means fall through to the real
    /// resolver.
    pub(crate) fn injected_preload() -> Option<Option<Arc<super::super::compile::LoadedUnit>>> {
        INJECTED_PRELOAD.with(|c| c.borrow().clone())
    }

    /// Inject a v2 pre-key map so `load_preload_prekeys` returns it (task #11:
    /// drives the prepopulate manifest pre-filter without a real manifest).
    pub(crate) fn inject_prekeys(map: super::PreKeyMap) {
        INJECTED_PREKEYS.with(|c| *c.borrow_mut() = Some(map));
    }

    /// The injected pre-key map, if a test set one.
    pub(crate) fn injected_prekeys() -> Option<super::PreKeyMap> {
        INJECTED_PREKEYS.with(|c| c.borrow().clone())
    }

    /// Count one `leaf_content_hash` attempt (called from that fn under test).
    pub(crate) fn note_hash_call() {
        HASH_CALLS.with(|c| c.set(c.get() + 1));
    }

    /// `leaf_content_hash` attempts since the last [`reset_hash_calls`]/[`reset`].
    pub(crate) fn hash_calls() -> usize {
        HASH_CALLS.with(|c| c.get())
    }

    /// Zero the hash-attempt counter.
    pub(crate) fn reset_hash_calls() {
        HASH_CALLS.with(|c| c.set(0));
    }
}

#[cfg(test)]
#[path = "aot/tests/aot_test.rs"]
mod tests;
