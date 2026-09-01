//! ByteCode chunk — compiled function representation.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use super::opcode::Op;
use crate::emacs_core::value::{LambdaParams, Value, ValueKind};
use crate::heap_types::LispString;

static NEXT_SOURCE_ID: AtomicU64 = AtomicU64::new(1);
static EAGER_GNU_BYTECODE: OnceLock<bool> = OnceLock::new();

pub(crate) fn eager_gnu_bytecode() -> bool {
    *EAGER_GNU_BYTECODE.get_or_init(|| {
        std::env::var_os("NEOMACS_EAGER_GNU_BYTECODE")
            .is_some_and(|value| !value.is_empty() && value != "0")
    })
}

pub(crate) fn fresh_bytecode_source_id() -> u64 {
    NEXT_SOURCE_ID.fetch_add(1, AtomicOrdering::Relaxed)
}

#[derive(Debug)]
struct DecodedGnuCode {
    ops: Vec<Op>,
    byte_offset_map: Vec<GnuByteOffsetMapEntry>,
    /// Result of `verify_stack_effects` for these instructions, proven once
    /// at decode against the owning function's entry depth and `max_stack`.
    stack_verified: bool,
}

/// Lazily decoded IR for a GNU byte-code string.
///
/// Loading validates each byte-code string once before installing this cell,
/// so initialization here is infallible unless the decoder itself regresses.
/// Keeping the cell behind a box costs one pointer in every bytecode object
/// and allocates the synchronization state only for GNU-backed functions.
#[derive(Debug)]
pub(crate) struct LazyGnuCode {
    decoded: OnceLock<DecodedGnuCode>,
}

#[cfg(test)]
static LAZY_GNU_DECODE_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Test-only: how many deferred GNU decodes ran on any thread so far.
#[cfg(test)]
pub(crate) fn lazy_gnu_decode_count_for_test() -> usize {
    LAZY_GNU_DECODE_COUNT.load(Ordering::Relaxed)
}

impl LazyGnuCode {
    fn new() -> Self {
        Self {
            decoded: OnceLock::new(),
        }
    }

    #[cold]
    #[inline(never)]
    fn decode(
        &self,
        raw_bytes: &[u8],
        published_constants_len: usize,
        entry_depth: usize,
        max_stack: usize,
    ) -> &DecodedGnuCode {
        self.decoded.get_or_init(|| {
            #[cfg(test)]
            LAZY_GNU_DECODE_COUNT.fetch_add(1, Ordering::Relaxed);
            // The published pool length lets `seal_ops` prove `Constant`
            // indices without lending the immutable Lisp constants mutably
            // merely to decode IR.
            let (ops, byte_offset_map) = super::decode::decode_gnu_bytecode_for_published_pool(
                raw_bytes,
                published_constants_len,
            )
            .expect("validated GNU bytecode failed deferred decoding");
            let stack_verified = super::decode::verify_stack_effects(&ops, entry_depth, max_stack);
            DecodedGnuCode {
                ops,
                byte_offset_map,
                stack_verified,
            }
        })
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct GnuByteOffsetMapEntry {
    pub byte_offset: usize,
    pub instruction_index: usize,
}

impl GnuByteOffsetMapEntry {
    pub const fn new(byte_offset: usize, instruction_index: usize) -> Self {
        Self {
            byte_offset,
            instruction_index,
        }
    }
}

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
    /// lazy path stores its proof inside the decoded cell instead.
    pub(crate) stack_verified: bool,
    /// Constant pool: values referenced by Constant/VarRef/VarSet/etc.
    /// `LispValueVec` so a pdump load can alias the pool directly in the
    /// mapped image instead of materializing an owned Vec per function.
    pub constants: crate::tagged::header::LispValueVec,
    /// Maximum stack depth needed (for pre-allocation).
    pub max_stack: u16,
    /// Parameter specification.
    pub params: LambdaParams,
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
    /// `None` ONLY on a pdump stub (see [`Self::pdump_stub`]): stubs are
    /// baked byte-for-byte into the dump image, and `None` is the one
    /// process-independent bit pattern an `Arc`-backed handle has. Every
    /// executable function carries `Some`; read through
    /// [`Self::jit_runtime`], never the field.
    #[cfg(feature = "jit")]
    pub runtime: Option<crate::emacs_core::jit::Runtime>,
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

/// Baked pdump stubs rely on `Option<Runtime>` taking the null-pointer niche
/// through the inner `Arc` (None = zero word, `unwrap_unchecked` = no-op). If
/// a compiler ever declines the niche, fail the build rather than regress.
#[cfg(feature = "jit")]
const _: () = assert!(
    std::mem::size_of::<Option<crate::emacs_core::jit::Runtime>>()
        == std::mem::size_of::<crate::emacs_core::jit::Runtime>()
);

#[cfg(test)]
static BYTECODE_FUNCTION_CLONE_COUNT: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_bytecode_function_clone_count_for_test() {
    BYTECODE_FUNCTION_CLONE_COUNT.store(0, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn bytecode_function_clone_count_for_test() -> usize {
    BYTECODE_FUNCTION_CLONE_COUNT.load(Ordering::Relaxed)
}

impl Clone for ByteCodeFunction {
    fn clone(&self) -> Self {
        #[cfg(test)]
        BYTECODE_FUNCTION_CLONE_COUNT.fetch_add(1, Ordering::Relaxed);

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
    pub fn new(params: LambdaParams) -> Self {
        let arglist = arglist_value_from_params(&params);
        Self {
            source_id: fresh_bytecode_source_id(),
            ops: Vec::new(),
            ops_sealed: false,
            stack_verified: false,
            constants: Vec::new().into(),
            max_stack: 0,
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
            runtime: Some(crate::emacs_core::jit::Runtime::new()),
            lazy_gnu_code: None,
        }
    }

    /// The tiering/profiling handle. Every function that can be observed
    /// outside the pdump loader carries `Some` (the chokepoints materialize
    /// stubs before handing data out), so the unwrap compiles to nothing —
    /// `Option<Runtime>` has the null-pointer niche (compile-asserted below).
    #[cfg(feature = "jit")]
    #[inline(always)]
    pub fn jit_runtime(&self) -> &crate::emacs_core::jit::Runtime {
        debug_assert!(
            self.runtime.is_some(),
            "jit runtime read on an unmaterialized pdump stub"
        );
        // SAFETY: `runtime` is `None` only on a pdump stub, and stubs are
        // unreachable here (chokepoints materialize first; see field doc).
        unsafe { self.runtime.as_ref().unwrap_unchecked() }
    }

    /// Release already-validated GNU decoded IR until it is first needed.
    /// The original byte string remains the single source of truth.
    pub(crate) fn defer_gnu_decode(&mut self) {
        if eager_gnu_bytecode() {
            return;
        }
        assert!(
            self.gnu_bytecode_bytes.is_some(),
            "deferred GNU decode requires original bytecode bytes"
        );
        self.ops = Vec::new();
        self.ops_sealed = false;
        self.stack_verified = false;
        self.gnu_byte_offset_map = None;
        self.lazy_gnu_code = Some(std::sync::Arc::new(LazyGnuCode::new()));
    }

    /// Whether `executable_ops` returns `seal_ops`-normalized instructions.
    ///
    /// This is the release-safety gate for the unchecked-fetch dispatch
    /// driver; see the field documentation on [`Self::ops_sealed`].
    #[inline]
    pub(crate) fn executes_sealed_ops(&self) -> bool {
        self.lazy_gnu_code.is_some() || self.ops_sealed
    }

    /// A lazy pdump stub awaiting materialization from its mapped extras.
    ///
    /// The discriminator is the SEALED-EMPTY niche: `seal_ops` appends a
    /// trailing `Return` unconditionally (decode.rs), so every genuinely
    /// sealed function has at least one op; the lazy-GNU arms leave
    /// `ops_sealed` false. `ops_sealed && ops.is_empty()` is therefore
    /// unconstructible except through [`Self::pdump_stub`]. Two plain field
    /// loads — cheap enough for the `get_bytecode_data` chokepoint, and safe
    /// under the GC's plain reads because the concurrent tracer never touches
    /// mapped bytecode data (its arm defers to the mutator-side legs).
    ///
    /// On a stub, `closure_slot_count` carries the extras region LENGTH (the
    /// materializer and the GC stub walker need it; a stub has no observable
    /// closure slots — every reader goes through the chokepoint, which
    /// materializes first).
    #[inline]
    pub(crate) fn is_pdump_stub(&self) -> bool {
        self.ops_sealed && self.ops.is_empty()
    }

    /// The stub form of a mapped `ByteCodeObj` carrying an extras region:
    /// no per-function allocations, no decoded state, and — since the
    /// dump-time bake — no process-specific bit pattern at all (`runtime` is
    /// `None`), so the dump writes these exact bytes into the image and the
    /// loader writes NOTHING into bytecode struct spans. `source_id` 0 is a
    /// debug second-witness (real ids start at 1).
    pub(crate) fn pdump_stub(extras_len: usize) -> Self {
        Self {
            source_id: 0,
            ops: Vec::new(),
            ops_sealed: true,
            stack_verified: false,
            constants: Vec::new().into(),
            max_stack: 0,
            params: super::super::builtins::LambdaParams::simple(Vec::new()),
            arglist: crate::emacs_core::value::Value::NIL,
            lexical: false,
            env: None,
            gnu_byte_offset_map: None,
            gnu_bytecode_bytes: None,
            docstring: None,
            doc_form: None,
            interactive: None,
            closure_slot_count: extras_len,
            extra_slots: Vec::new(),
            #[cfg(feature = "jit")]
            runtime: None,
            lazy_gnu_code: None,
        }
    }

    /// Operand-stack entry depth the verifier must assume, mirroring
    /// `run_frame`'s argument staging exactly: params-on-stack conventions
    /// seed `nonrest` slots plus one `&rest` list slot; pure dynamic-binding
    /// functions enter with an empty operand stack.
    pub(crate) fn verifier_entry_depth(&self) -> usize {
        let nonrest = self.params.required.len() + self.params.optional.len();
        let params_on_stack = self.lexical
            || self.env.is_some()
            || matches!(self.arglist.kind(), ValueKind::Fixnum(_));
        if params_on_stack {
            nonrest + usize::from(self.params.rest.is_some())
        } else {
            0
        }
    }

    /// Whether `executable_ops` returns instructions with a standing
    /// `verify_stack_effects` proof. Resolves the lazy decode if needed —
    /// callers are classification-time, never the dispatch hot path.
    pub(crate) fn executes_verified_ops(&self) -> bool {
        match &self.lazy_gnu_code {
            Some(lazy) => {
                let raw_bytes = self
                    .gnu_bytecode_bytes
                    .as_deref()
                    .expect("lazy GNU bytecode lost its original bytes");
                lazy.decode(
                    raw_bytes,
                    self.constants.len(),
                    self.verifier_entry_depth(),
                    self.max_stack as usize,
                )
                .stack_verified
            }
            None => self.stack_verified,
        }
    }

    /// Recompute the operand-stack proof for eager instructions. Call at
    /// the last point before publication, after every shape field
    /// (params/lexical/arglist/env/max_stack) has its final value.
    pub(crate) fn refresh_stack_verification(&mut self) {
        self.stack_verified = super::decode::verify_stack_effects(
            &self.ops,
            self.verifier_entry_depth(),
            self.max_stack as usize,
        );
    }

    /// Run hand-assembled instructions through the real sealing normalizer
    /// so synthesized chunks may enter the unchecked-fetch driver. For the
    /// lib-internal AOT testkit and the unit-test harness only — production
    /// bytecode is sealed exclusively by the decode installers. Call after
    /// `ops` and `constants` are both in place; no-op for GNU-backed or
    /// already-sealed functions.
    pub(crate) fn seal_hand_assembled_ops(&mut self) {
        if self.executes_sealed_ops() || self.gnu_bytecode_bytes.is_some() {
            return;
        }
        self.ops = super::decode::seal_ops(std::mem::take(&mut self.ops), self.constants.len());
        self.ops_sealed = true;
        self.stack_verified = super::decode::verify_stack_effects(
            &self.ops,
            self.verifier_entry_depth(),
            self.max_stack as usize,
        );
    }

    /// Unit-test alias for [`Self::seal_hand_assembled_ops`].
    #[cfg(test)]
    pub(crate) fn seal_hand_assembled_ops_for_test(&mut self) {
        self.seal_hand_assembled_ops();
    }

    /// Restore GNU bytecode from its canonical byte stream using the active
    /// eager/lazy policy.
    ///
    /// File dumps intentionally omit decoded instructions because they are
    /// derived from `gnu_bytecode_bytes`. Unlike [`Self::defer_gnu_decode`],
    /// this entry point also materializes those instructions when eager mode
    /// is requested, so the serialized representation cannot create an
    /// invalid empty eager function.
    pub(crate) fn restore_gnu_decode_policy(&mut self) -> Result<(), super::decode::DecodeError> {
        let raw_bytes = self
            .gnu_bytecode_bytes
            .as_deref()
            .expect("restored GNU bytecode requires original bytes");
        self.lazy_gnu_code = None;
        if eager_gnu_bytecode() {
            let (ops, byte_offset_map) = super::decode::decode_gnu_bytecode_with_offset_map(
                raw_bytes,
                self.constants.ensure_owned(),
            )?;
            self.ops = ops;
            self.ops_sealed = true;
            self.stack_verified = super::decode::verify_stack_effects(
                &self.ops,
                self.verifier_entry_depth(),
                self.max_stack as usize,
            );
            self.gnu_byte_offset_map = Some(byte_offset_map);
        } else {
            self.ops.clear();
            self.ops_sealed = false;
            self.stack_verified = false;
            self.gnu_byte_offset_map = None;
            self.lazy_gnu_code = Some(std::sync::Arc::new(LazyGnuCode::new()));
        }
        Ok(())
    }

    /// Executable instructions, decoding a cold GNU byte string on first use.
    #[inline]
    pub fn executable_ops(&self) -> &[Op] {
        match &self.lazy_gnu_code {
            Some(lazy) => {
                let decoded = match lazy.decoded.get() {
                    Some(decoded) => decoded,
                    None => {
                        let raw_bytes = self
                            .gnu_bytecode_bytes
                            .as_deref()
                            .expect("lazy GNU bytecode lost its original bytes");
                        lazy.decode(
                            raw_bytes,
                            self.constants.len(),
                            self.verifier_entry_depth(),
                            self.max_stack as usize,
                        )
                    }
                };
                &decoded.ops
            }
            None => &self.ops,
        }
    }

    /// GNU byte offsets corresponding to [`Self::executable_ops`].
    #[inline]
    pub fn executable_gnu_byte_offset_map(&self) -> Option<&[GnuByteOffsetMapEntry]> {
        match &self.lazy_gnu_code {
            Some(lazy) => {
                let decoded = match lazy.decoded.get() {
                    Some(decoded) => decoded,
                    None => {
                        let raw_bytes = self
                            .gnu_bytecode_bytes
                            .as_deref()
                            .expect("lazy GNU bytecode lost its original bytes");
                        lazy.decode(
                            raw_bytes,
                            self.constants.len(),
                            self.verifier_entry_depth(),
                            self.max_stack as usize,
                        )
                    }
                };
                let map = &decoded.byte_offset_map;
                (!map.is_empty()).then_some(map)
            }
            None => self.gnu_byte_offset_map.as_deref(),
        }
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

    /// Disassemble to a human-readable string.
    pub fn disassemble(&self) -> String {
        let ops = self.executable_ops();
        let mut out = String::new();
        out.push_str(&format!(
            "bytecode function ({} ops, {} constants, stack {})\n",
            ops.len(),
            self.constants.len(),
            self.max_stack
        ));

        out.push_str("constants:\n");
        for (i, c) in self.constants.iter().enumerate() {
            out.push_str(&format!("  {}: {}\n", i, c));
        }

        out.push_str("code:\n");
        for (i, op) in ops.iter().enumerate() {
            out.push_str(&format!("  {:4}: {}\n", i, op.disasm(&self.constants)));
        }
        out
    }
}
#[cfg(test)]
#[path = "tests/chunk.rs"]
mod tests;
