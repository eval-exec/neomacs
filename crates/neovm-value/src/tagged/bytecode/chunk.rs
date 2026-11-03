//! Bytecode storage definitions and their storage-owned impls (T1).
//!
//! The storage vocabulary and every impl that depends only on it live here:
//! construction, clone, the pure projections and the assembly helpers. All
//! decoder/verifier/sealer/VM/JIT policy stays in `emacs_core::bytecode`; the
//! upper module reaches the policy methods through its extension trait and
//! everything else through these inherent impls (an upper crate may not add
//! an inherent impl on a type defined here after the crate move).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

use crate::heap_types::LispString;
use crate::tagged::value::ValueKind;

use super::decode_data::StackProof;
use super::function_slots::{FunctionParams, StackDepth};
use super::opcode::Op;
use super::{LambdaParams, Value};

#[cfg(feature = "jit")]
use super::Runtime;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
pub(crate) struct DecodedGnuCode {
    pub(crate) ops: Vec<Op>,
    pub(crate) byte_offset_map: Vec<GnuByteOffsetMapEntry>,
    /// Result of `verify_stack_effects` for these instructions, proven once
    /// at decode against the owning function's entry depth and `max_stack`.
    pub(crate) stack_verified: bool,
}

/// Lazily decoded IR for a GNU byte-code string.
///
/// Loading validates each byte-code string once before installing this cell,
/// so initialization here is infallible unless the decoder itself regresses.
/// Keeping the cell behind a box costs one pointer in every bytecode object
/// and allocates the synchronization state only for GNU-backed functions.
#[derive(Debug)]
#[repr(C)]
pub(crate) struct LazyGnuCode {
    // Keep this first, with DecodedGnuCode's original payload unchanged:
    // run_loop inlines its projections, and their layout affects regalloc.
    pub(crate) decoded: OnceLock<DecodedGnuCode>,
    /// Immutable proof shared by closure clones and concurrent readers.
    /// Initialized inside `decoded.get_or_init`, before publishing decoded;
    /// every reader acquires decoded before consulting this sibling cell.
    pub(crate) proof: OnceLock<StackProof>,
    /// Source-invariant iterative admission, excluding the per-instance env.
    /// Append only: decoded/proof offsets are used by inlined VM projections.
    pub(crate) iterative_admission: OnceLock<Option<IterativeArgRange>>,
}

impl LazyGnuCode {
    pub(crate) fn new() -> Self {
        Self {
            decoded: OnceLock::new(),
            proof: OnceLock::new(),
            iterative_admission: OnceLock::new(),
        }
    }

    /// Ready-state projection for the hot decoded path: `Some` once the
    /// payload is published. Inline on purpose: a decoded hit costs one
    /// atomic load and a reference, never an out-of-line ready/cold factory
    /// call. Only a `None` here may enter the upper cold decode boundary
    /// (`decode_lazy_gnu_code`), which is where the constant slice and the
    /// byte-offset map are built.
    #[inline]
    pub(crate) fn decoded_ready(&self) -> Option<&DecodedGnuCode> {
        self.decoded.get()
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct IterativeArgRange {
    pub(crate) required: u16,
    pub(crate) nonrest: u16,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct GnuByteOffsetMapEntry {
    pub byte_offset: usize,
    pub instruction_index: usize,
}

/// One piece of evidence used for structural byte-code equality and hashing.
///
/// This is deliberately named a "part", not a GNU closure "slot": Neomacs
/// keeps some execution state (notably the captured environment) beside the
/// GNU-compatible slot projection.  Consumers must compare that state too,
/// because it affects execution even when it has no independently observable
/// GNU slot.
#[derive(Clone, Copy, Debug)]
pub enum ByteCodeStructuralPart<'a> {
    /// GNU's observable pseudovector size.  Keeping shape in the sequence
    /// makes it impossible for a consumer to forget the `ASIZE` comparison.
    ObservableSlotCount(usize),
    /// A Lisp value compared, hashed and keyed as any other.
    Value(Value),
    /// The GNU bytecode string, byte-exact.
    Bytes(&'a [u8]),
    /// Decoded instructions of a function with no retained GNU bytes.
    Ops(&'a [Op]),
    /// The constants vector, element-wise like a Lisp vector.
    Values(&'a [Value]),
    /// A docstring, under GNU string equality: characters, bytes, contents.
    Text(&'a LispString),
    /// An optional structural part that is not present.
    Absent,
}

/// Allocation-free iterator over a byte-code function's structural evidence.
pub(crate) struct ByteCodeStructuralParts<'a> {
    pub(crate) fixed: std::array::IntoIter<ByteCodeStructuralPart<'a>, 8>,
    pub(crate) extras: std::slice::Iter<'a, Value>,
}

/// A compiled bytecode function.
#[derive(Debug)]
pub struct ByteCodeFunction {
    /// Runtime identity of the source code object, preserved by `make-closure`.
    pub(crate) source_id: u64,
    /// The bytecode instructions.
    pub ops: Vec<Op>,
    /// Whether `ops` is `seal_ops`-normalized (trailing `Return`, in-bounds
    /// branch targets, in-range `Constant` indices). Set exclusively by the
    /// decode installers; the unchecked-fetch driver refuses eager
    /// instruction vectors without it, so a hand-assembled chunk can never
    /// reach an unchecked read. Lazy GNU-backed IR needs no marker: the
    /// deferred decoder is its only writer.
    pub(crate) ops_sealed: bool,
    /// Whether `verify_stack_effects` proved these eager instructions'
    /// operand-stack behavior against `max_stack` and the entry depth. Not
    /// serialized: recomputed wherever eager instructions are installed. The
    /// lazy path stores its proof beside the decoded instructions instead.
    pub(crate) stack_verified: bool,
    /// Constant pool: values referenced by Constant/VarRef/VarSet/etc.
    /// `LispValueVec` so a pdump load can alias the pool directly in the
    /// mapped image instead of materializing an owned Vec per function.
    pub constants: crate::tagged::header::LispValueVec,
    /// Maximum stack depth needed (for pre-allocation).
    pub max_stack: StackDepth,
    /// Parameter specification.
    pub params: FunctionParams,
    /// Original GNU byte-code slot 0 value.
    ///
    /// Lexical byte-code uses an integer arg descriptor here, while old-style
    /// dynamic byte-code uses an arglist.  Bytecomp's inliner distinguishes
    /// those cases through `(aref fn 0)`, so this must round-trip exactly.
    pub arglist: Value,
    /// Whether the function was compiled with lexical binding enabled.
    pub lexical: bool,
    /// For closures: captured lexical environment as a cons alist.
    pub env: Option<Value>,
    /// GNU `.elc` bytecode stores branch targets as byte offsets.
    /// Decoded runtime uses instruction indices, so GNU-decoded functions
    /// retain a sorted byte-offset -> instruction-index table for `switch`.
    ///
    /// GNU does not allocate a hash table for this: it executes directly from
    /// the byte string.  Keep Neomacs' bridge representation compact and cheap
    /// to restore; `Bswitch` is the only runtime user.
    pub gnu_byte_offset_map: Option<Vec<GnuByteOffsetMapEntry>>,
    /// Original GNU-format bytecode bytes from the .elc file or `make-byte-code`
    /// call.  NeoVM normally executes from `ops` (decoded IR), but elisp code
    /// like `byte-compile-make-closure` does `(aref FUN 1)` to read the raw
    /// bytecode string and pass it to `make-byte-code` for closure prototype
    /// generation.  Without preserving the original bytes, those round-trips
    /// produce empty bytecode functions.
    pub gnu_bytecode_bytes: Option<crate::tagged::header::LispByteVec>,
    /// Optional docstring.
    pub docstring: Option<LispString>,
    /// Optional documentation form (e.g., oclosure type symbol in slot 4).
    pub doc_form: Option<Value>,
    /// Interactive spec from GNU closure slot 5 (CLOSURE_INTERACTIVE).
    /// Can be a string code, a form to evaluate, or a vector [spec, modes].
    pub interactive: Option<Value>,
    /// GNU closure pseudovector size for observable sequence operations.
    ///
    /// GNU `make-byte-code` allocates a vector with exactly the number of
    /// arguments supplied, then marks it as `PVEC_CLOSURE`.  Explicit nil slots
    /// therefore still count for `length`, `aref`, `append`, printing, etc.
    pub closure_slot_count: usize,
    /// GNU accepts `&rest ELEMENTS` after the interactive slot.  They have no
    /// execution significance, but remain observable through closure slots.
    pub extra_slots: Vec<Value>,
    /// Runtime tiering/profiling state (the JIT path): a handle SHARED with
    /// every `make-closure` instance of this source (same lifetime as
    /// `source_id`). NOT part of the dumped representation — pure runtime
    /// state, started cold each session. Present only under the `jit`
    /// feature. See `jit::Runtime`.
    ///
    /// `None` ONLY on a pdump stub (see [`Self::is_pdump_stub`]): stubs are
    /// baked byte-for-byte into the dump image, and `None` is the one
    /// process-independent bit pattern an `Arc`-backed handle has. Every
    /// executable function carries `Some`; read through
    /// [`Self::jit_runtime`], never the field.
    #[cfg(feature = "jit")]
    pub(crate) runtime: Option<Runtime>,
    /// Present for GNU-backed functions whose validated decoded IR has been
    /// released until first execution. One pointer (an `Arc`), so ordinary
    /// bytecode objects continue to fit the existing 384-byte arena slot —
    /// and SHARED by `clone` (i.e. by every `make-closure` instance of one
    /// prototype): the decoded IR depends only on the original bytes, the
    /// constant-pool LENGTH and the stack contract, all of which the clone
    /// copies unchanged, so re-decoding per instance was pure waste (1,366
    /// decodes = 8% of the type-sim window; GNU runs its byte string directly
    /// and never pays a decode at all).
    pub(crate) lazy_gnu_code: Option<std::sync::Arc<LazyGnuCode>>,
}

#[cfg(test)]
static BYTECODE_FUNCTION_CLONE_COUNT: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_bytecode_function_clone_count_for_test() {
    BYTECODE_FUNCTION_CLONE_COUNT.store(0, Ordering::Relaxed);
}

/// Test-only: count a whole-function copy made without `Clone` (the
/// in-place `make-closure` instance writer).
#[cfg(test)]
pub(crate) fn note_bytecode_function_clone_for_test() {
    BYTECODE_FUNCTION_CLONE_COUNT.fetch_add(1, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn bytecode_function_clone_count_for_test() -> usize {
    BYTECODE_FUNCTION_CLONE_COUNT.load(Ordering::Relaxed)
}

static NEXT_SOURCE_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn fresh_bytecode_source_id() -> u64 {
    NEXT_SOURCE_ID.fetch_add(1, AtomicOrdering::Relaxed)
}

impl IterativeArgRange {
    #[inline(always)]
    pub(crate) fn accepts(self, nargs: usize) -> bool {
        let required = usize::from(self.required);
        let nonrest = usize::from(self.nonrest);
        // Installation proved required <= nonrest <= u16::MAX.
        // Values below required wrap above this bounded interval's width.
        debug_assert!(required <= nonrest);
        nargs.wrapping_sub(required) <= nonrest - required
    }
}

impl DecodedGnuCode {
    /// The instruction a jump-table value outside the proof's admitted set
    /// names: a GNU byte offset resolved through the byte-offset map exactly
    /// as the checked driver resolves every hit, or (natively indexed code)
    /// the value itself. Only a table mutated after sealing gets here.
    #[cold]
    #[inline(never)]
    pub(crate) fn unadmitted_switch_target(&self, raw: i64) -> Option<usize> {
        let raw_addr = usize::try_from(raw).ok()?;
        let map = &self.byte_offset_map;
        if map.is_empty() {
            return Some(raw_addr);
        }
        map.binary_search_by_key(&raw_addr, |entry| entry.byte_offset)
            .ok()
            .map(|index| map[index].instruction_index)
    }
}

impl GnuByteOffsetMapEntry {
    pub const fn new(byte_offset: usize, instruction_index: usize) -> Self {
        Self {
            byte_offset,
            instruction_index,
        }
    }
}

impl<'a> Iterator for ByteCodeStructuralParts<'a> {
    type Item = ByteCodeStructuralPart<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.fixed.next().or_else(|| {
            self.extras
                .next()
                .copied()
                .map(ByteCodeStructuralPart::Value)
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.fixed.len() + self.extras.len();
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for ByteCodeStructuralParts<'_> {}
impl std::iter::FusedIterator for ByteCodeStructuralParts<'_> {}

fn arglist_value_from_params(params: &LambdaParams) -> Value {
    let mut elements = Vec::new();
    for sym in &params.required {
        elements.push(Value::from_sym_id(*sym));
    }
    if !params.optional.is_empty() {
        elements.push(Value::symbol("&optional"));
        for sym in &params.optional {
            elements.push(Value::from_sym_id(*sym));
        }
    }
    if let Some(rest) = params.rest {
        elements.push(Value::symbol("&rest"));
        elements.push(Value::from_sym_id(rest));
    }
    Value::list(elements)
}

// `TaggedHeap::alloc_bytecode_instance` (`make-closure`) copies the same
// fields in place; its exhaustive destructure keeps the two in step.
impl Clone for ByteCodeFunction {
    #[inline(never)]
    fn clone(&self) -> Self {
        #[cfg(test)]
        note_bytecode_function_clone_for_test();

        // A stub must be materialized before anything clones it (print and
        // make-closure both go through the chokepoint); a cloned stub would
        // carry `runtime: None` into an arena object where only debug
        // asserts guard the invariant.
        debug_assert!(
            !self.is_pdump_stub(),
            "cloning an unmaterialized pdump stub"
        );

        Self {
            source_id: self.source_id,
            ops: self.ops.clone(),
            ops_sealed: self.ops_sealed,
            stack_verified: self.stack_verified,
            constants: self.constants.clone(),
            max_stack: self.max_stack,
            params: self.params.clone(),
            arglist: self.arglist,
            lexical: self.lexical,
            env: self.env,
            gnu_byte_offset_map: self.gnu_byte_offset_map.clone(),
            gnu_bytecode_bytes: self.gnu_bytecode_bytes.clone(),
            docstring: self.docstring.clone(),
            doc_form: self.doc_form,
            interactive: self.interactive,
            closure_slot_count: self.closure_slot_count,
            extra_slots: self.extra_slots.clone(),
            // A clone SHARES the source's tiering state (heat, feedback,
            // compiled leaf, patched-prefix record): `make-closure` clones the
            // prototype per instantiation, and per-instance state meant closure
            // code never tiered. Overturns the earlier "starts cold" rule — see
            // `jit::Runtime`. The identity this follows is `source_id` above.
            #[cfg(feature = "jit")]
            runtime: self.runtime.clone(),
            // Shares the deferred-decode cell (see the field doc).
            lazy_gnu_code: self.lazy_gnu_code.clone(),
        }
    }
}

impl ByteCodeFunction {
    pub fn new(params: impl Into<FunctionParams>) -> Self {
        let params = params.into();
        let arglist = match &params {
            FunctionParams::Named(params) => arglist_value_from_params(params),
            FunctionParams::Stack(template) => Value::fixnum(template.raw()),
            FunctionParams::Dynamic(arglist) => arglist.value(),
        };
        Self {
            source_id: fresh_bytecode_source_id(),
            ops: Vec::new(),
            ops_sealed: false,
            stack_verified: false,
            constants: Vec::new().into(),
            max_stack: StackDepth::ZERO,
            params,
            arglist,
            lexical: false,
            env: None,
            gnu_byte_offset_map: None,
            gnu_bytecode_bytes: None,
            docstring: None,
            doc_form: None,
            interactive: None,
            closure_slot_count: 4,
            extra_slots: Vec::new(),
            #[cfg(feature = "jit")]
            runtime: Some(Runtime::new()),
            lazy_gnu_code: None,
        }
    }

    /// Whether `executable_ops` returns `seal_ops`-normalized instructions.
    ///
    /// This is the release-safety gate for the unchecked-fetch dispatch
    /// driver; see the field documentation on [`Self::ops_sealed`].
    #[inline(always)]
    pub(crate) fn executes_sealed_ops(&self) -> bool {
        self.lazy_gnu_code.is_some() || self.ops_sealed
    }

    /// A lazy pdump stub awaiting materialization from its mapped extras.
    ///
    /// The discriminator is the SEALED-EMPTY niche: `seal_ops` appends a
    /// trailing `Return` unconditionally (decode.rs), so every genuinely
    /// sealed function has at least one op; the lazy-GNU arms leave
    /// `ops_sealed` false. `ops_sealed && ops.is_empty()` is therefore
    /// unconstructible except by the dump's baked stub template
    /// (`baked_stub_template` in `pdump::mapped_heap`). Two plain field
    /// loads — cheap enough for the `get_bytecode_data` chokepoint, and safe
    /// under the GC's plain reads because the concurrent tracer never touches
    /// mapped bytecode data (its arm defers to the mutator-side legs).
    ///
    /// On a stub, `closure_slot_count` carries the extras region LENGTH (the
    /// materializer and the GC stub walker need it; a stub has no observable
    /// closure slots — every reader goes through the chokepoint, which
    /// materializes first).
    ///
    /// The stub is the form of a mapped `ByteCodeObj` carrying an extras
    /// region: no per-function allocations, no decoded state, and — since
    /// the dump-time bake — no process-specific bit pattern at all
    /// (`runtime` is `None`), so the dump writes those exact bytes into the
    /// image and the loader writes NOTHING into bytecode struct spans.
    /// `source_id` 0 is a debug second-witness (real ids start at 1).
    #[inline(always)]
    pub(crate) fn is_pdump_stub(&self) -> bool {
        self.ops_sealed && self.ops.is_empty()
    }

    /// Decoded instructions that are resident now, without materializing a
    /// cold function. Used by heap diagnostics and capacity accounting.
    #[inline]
    pub(crate) fn resident_ops(&self) -> &[Op] {
        match &self.lazy_gnu_code {
            Some(lazy) => lazy
                .decoded
                .get()
                .map(|decoded| decoded.ops.as_slice())
                .unwrap_or_default(),
            None => &self.ops,
        }
    }

    #[inline]
    pub(crate) fn resident_ops_capacity(&self) -> usize {
        match &self.lazy_gnu_code {
            Some(lazy) => lazy
                .decoded
                .get()
                .map(|decoded| decoded.ops.capacity())
                .unwrap_or(0),
            None => self.ops.capacity(),
        }
    }

    /// Resident GNU offset map without forcing deferred IR initialization.
    #[inline]
    pub(crate) fn resident_gnu_byte_offset_map(&self) -> Option<&[GnuByteOffsetMapEntry]> {
        match &self.lazy_gnu_code {
            Some(lazy) => lazy
                .decoded
                .get()
                .map(|decoded| decoded.byte_offset_map.as_slice())
                .filter(|map| !map.is_empty()),
            None => self.gnu_byte_offset_map.as_deref(),
        }
    }

    #[inline]
    pub(crate) fn resident_gnu_byte_offset_map_capacity(&self) -> usize {
        match &self.lazy_gnu_code {
            Some(lazy) => lazy
                .decoded
                .get()
                .map(|decoded| decoded.byte_offset_map.capacity())
                .unwrap_or(0),
            None => self.gnu_byte_offset_map.as_ref().map_or(0, Vec::capacity),
        }
    }

    /// The structural evidence for this function: GNU's observable shape and
    /// closure slots, in GNU's order, plus the environment this port keeps
    /// beside the constants vector.
    ///
    /// GNU's `internal_equal_1` compares a `PVEC_CLOSURE` element-wise
    /// (src/fns.c:2984-2998 in emacs-31.1) and `sxhash_obj` hashes it through
    /// `sxhash_vector` (:5525-5536); `equal` hash tables therefore key on the
    /// same elements.  Neomacs stores those elements in typed fields, so
    /// `equal`, its fallible twin, `sxhash-equal` and the `equal`-table key
    /// all read THIS sequence rather than each re-deriving the schema:
    ///
    /// | part | GNU representation  | Neomacs structural evidence             |
    /// |------|---------------------|------------------------------------------|
    /// | 0    | pseudovector ASIZE  | `ObservableSlotCount`                    |
    /// | 1    | arglist             | `Value`                                  |
    /// | 2    | bytecode string     | `Bytes`, or `Ops` when none is retained  |
    /// | 3    | constants vector    | `Values`                                 |
    /// | 4    | inside constants    | captured `env`: `Value`, else `Absent`   |
    /// | 5    | max stack depth     | `Value` (a fixnum)                       |
    /// | 6    | docstring / form    | `Text`, `Value`, or `Absent`             |
    /// | 7    | interactive spec    | `Value` or `Absent`                      |
    /// | 8..  | `&rest ELEMENTS`    | `Value` each                             |
    ///
    /// `Absent` is distinct from a present `nil`: `Op::MakeClosure` stores
    /// `Some(lexenv)`, and a closure over no variables has a present, `nil`
    /// environment that `(aref FN 2)` reports as `nil` rather than as the
    /// constants vector.  A function with no retained GNU bytes compares its
    /// decoded instructions instead, so two functions with different code are
    /// never equal even though GNU's slot 1 would be `nil` for both.
    pub(crate) fn structural_parts(&self) -> ByteCodeStructuralParts<'_> {
        use ByteCodeStructuralPart as Part;

        ByteCodeStructuralParts {
            fixed: [
                Part::ObservableSlotCount(self.observable_closure_slot_count()),
                Part::Value(self.arglist),
                match &self.gnu_bytecode_bytes {
                    Some(bytes) => Part::Bytes(bytes.as_slice()),
                    None => Part::Ops(&self.ops),
                },
                Part::Values(self.constants.as_slice()),
                self.env.map_or(Part::Absent, Part::Value),
                Part::Value(self.max_stack.value()),
                match (self.doc_form, &self.docstring) {
                    (Some(form), _) => Part::Value(form),
                    (None, Some(doc)) => Part::Text(doc),
                    (None, None) => Part::Absent,
                },
                self.interactive.map_or(Part::Absent, Part::Value),
            ]
            .into_iter(),
            extras: self.extra_slots.iter(),
        }
    }

    pub fn observable_closure_slot_count(&self) -> usize {
        let mut count = self.closure_slot_count.max(4);
        if self.docstring.is_some() || self.doc_form.is_some() {
            count = count.max(5);
        }
        if self.interactive.is_some() {
            count = count.max(6);
        }
        if !self.extra_slots.is_empty() {
            count = count.max(6 + self.extra_slots.len());
        }
        count
    }

    /// Add a constant to the pool and return its index.
    /// Deduplicates by value equality for symbols and integers.
    pub fn add_constant(&mut self, value: Value) -> u16 {
        // Check for existing constant (simple dedup for common types)
        for (i, existing) in self.constants.iter().enumerate() {
            match (value.kind(), existing.kind()) {
                (ValueKind::Fixnum(a), ValueKind::Fixnum(b)) if a == b => return i as u16,
                (ValueKind::Symbol(a), ValueKind::Symbol(b)) if a == b => return i as u16,
                (ValueKind::Symbol(a), ValueKind::Symbol(b)) if a == b => return i as u16,
                (ValueKind::Symbol(a), ValueKind::Symbol(b)) if a == b => return i as u16,
                (ValueKind::Nil, ValueKind::Nil) => return i as u16,
                (ValueKind::T, ValueKind::T) => return i as u16,
                (ValueKind::Symbol(a), ValueKind::Symbol(b)) if a == b => return i as u16,
                _ => {}
            }
        }
        let idx = self.constants.len() as u16;
        self.constants.ensure_owned().push(value);
        idx
    }

    /// Add a symbol name to the constant pool and return its index.
    pub fn add_symbol(&mut self, name: &str) -> u16 {
        self.add_constant(Value::symbol(name))
    }

    /// Emit an instruction.
    pub fn emit(&mut self, op: Op) {
        self.ops.push(op);
    }

    /// Current instruction count (used for jump target calculation).
    pub fn current_offset(&self) -> u32 {
        self.ops.len() as u32
    }

    /// Patch a jump target at the given instruction index.
    pub fn patch_jump(&mut self, instr_idx: u32, target: u32) {
        let idx = instr_idx as usize;
        match &mut self.ops[idx] {
            Op::Goto(addr)
            | Op::GotoIfNil(addr)
            | Op::GotoIfNotNil(addr)
            | Op::GotoIfNilElsePop(addr)
            | Op::GotoIfNotNilElsePop(addr)
            | Op::PushConditionCase(addr)
            | Op::PushConditionCaseRaw(addr)
            | Op::PushCatch(addr) => {
                *addr = target;
            }
            _ => panic!("patch_jump on non-jump instruction at {}", idx),
        }
    }
}
