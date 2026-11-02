//! ByteCode chunk — compiled function representation.

use std::sync::OnceLock;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use super::function_slots::{FunctionParams, StackDepth};
use super::opcode::Op;
#[cfg(test)]
use crate::emacs_core::value::LambdaParams;
use crate::emacs_core::value::{Value, ValueKind};

pub use crate::tagged::bytecode::chunk::{
    ByteCodeFunction, ByteCodeStructuralPart, GnuByteOffsetMapEntry,
};
pub(crate) use crate::tagged::bytecode::chunk::{
    DecodedGnuCode, IterativeArgRange, LazyGnuCode, fresh_bytecode_source_id,
};
#[cfg(test)]
pub(crate) use crate::tagged::bytecode::chunk::{
    bytecode_function_clone_count_for_test, reset_bytecode_function_clone_count_for_test,
};

static EAGER_GNU_BYTECODE: OnceLock<bool> = OnceLock::new();

pub(crate) fn eager_gnu_bytecode() -> bool {
    *EAGER_GNU_BYTECODE.get_or_init(|| {
        std::env::var_os("NEOMACS_EAGER_GNU_BYTECODE")
            .is_some_and(|value| !value.is_empty() && value != "0")
    })
}

#[cfg(test)]
static LAZY_GNU_DECODE_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Test-only: how many deferred GNU decodes ran on any thread so far.
#[cfg(test)]
pub(crate) fn lazy_gnu_decode_count_for_test() -> usize {
    LAZY_GNU_DECODE_COUNT.load(Ordering::Relaxed)
}

/// The upper cold decode boundary: it names the decoder/verifier and is the
/// only place the lazy algorithm runs. The hot path calls it only on a
/// `decoded_ready()` miss; the storage layer owns the cells and the inline
/// ready projection. A free function (not an inherent impl) so the upper
/// crate adds no impl to the lowered type.
#[cold]
#[inline(never)]
fn decode_lazy_gnu_code<'a, 'l>(
    lazy: &'l LazyGnuCode,
    raw_bytes: &[u8],
    published_constants_len: usize,
    verifier_inputs: impl FnOnce() -> (Option<usize>, &'a [Value]),
    max_stack: usize,
) -> &'l DecodedGnuCode {
    lazy.decoded.get_or_init(|| {
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
        // The published pool is read (never lent mutably) only to admit
        // `Switch`: its jump tables are hash-table constants, and the
        // byte-offset map (retained exactly when the stream has a switch)
        // turns their byte offsets into instruction indices.
        // Reuse the existing function-reference closure capture to read
        // the pool on this cold boundary. Passing a slice from inlined
        // executable_ops would add a live pointer in run_loop.
        let (entry_depth, published_constants) = verifier_inputs();
        let proof = entry_depth.and_then(|depth| {
            super::decode::verify_stack_effects_with_tables(
                &ops,
                depth,
                max_stack,
                Some(super::decode::SwitchTables {
                    constants: published_constants,
                    offset_map: (!byte_offset_map.is_empty()).then_some(&byte_offset_map[..]),
                }),
            )
        });
        let stack_verified = proof.is_some();
        let _ = lazy.proof.set(proof.unwrap_or_default());
        DecodedGnuCode {
            ops,
            byte_offset_map,
            stack_verified,
        }
    })
}

/// Baked pdump stubs rely on `Option<Runtime>` taking the null-pointer niche
/// through the inner `Arc` (None = zero word, `unwrap_unchecked` = no-op). If
/// a compiler ever declines the niche, fail the build rather than regress.
#[cfg(feature = "jit")]
const _: () = assert!(
    std::mem::size_of::<Option<crate::emacs_core::jit::Runtime>>()
        == std::mem::size_of::<crate::emacs_core::jit::Runtime>()
);

/// Upper policy methods on the lowered bytecode storage (T1 storage cut).
///
/// The storage definitions live in the value layer; decoding, verification,
/// sealing, GNU-policy restoration and the JIT handle accessors stay upper.
/// These are ordinary statically dispatched methods: no vtable, no boxing and
/// no duplicate struct. Callers import the trait (`as _` is enough).
pub(crate) trait ByteCodeFunctionExt {
    /// The tiering/profiling handle (JIT builds only).
    #[cfg(feature = "jit")]
    fn jit_runtime(&self) -> &crate::emacs_core::jit::Runtime;

    /// The constant base a compiled leaf reads through (JIT builds only).
    #[cfg(feature = "jit")]
    fn jit_constant_base(&self) -> *const crate::emacs_core::value::Value;

    /// Release validated GNU decoded IR until first execution.
    fn defer_gnu_decode(&mut self);

    /// Operand-stack entry depth the verifier must assume.
    fn verifier_entry_depth(&self) -> Option<usize>;

    /// Hot iterative-admission probe: the pure cached projection on a hit;
    /// a miss calls the cold [`Self::initialize_lazy_iterative_admission`].
    /// UPPER because the miss crosses into decoding policy; a trait method
    /// (not an inherent impl on the lowered type) so the upper crate adds no
    /// impl to the type it will no longer own. The `#[inline(always)]` lives
    /// on the impl (required trait methods take no inline attribute).
    fn lazy_iterative_admission(&self, nargs: usize) -> Option<bool>;

    /// Initialize the immutable iterative-admission proof on a lazy miss.
    ///
    /// UPPER policy: it reads the eager/lazy shape policy and, transitively,
    /// [`Self::executes_verified_ops`] (which can decode and verify). It
    /// mutates the shared admission cell and is deliberately cold; it is not
    /// a pure projection and never lives on the lowered data.
    fn initialize_lazy_iterative_admission(&self, nargs: usize) -> bool;

    /// Whether the executable instructions carry a standing stack proof.
    fn executes_verified_ops(&self) -> bool;

    /// Human-readable disassembly (UPPER: reads decoded policy output).
    /// Its consumer is the unit-test/diagnostic surface, so the crate-private
    /// trait would otherwise read as dead code outside tests.
    #[allow(dead_code)]
    fn disassemble(&self) -> String;

    /// Resolve a switch hit through the standing proof.
    fn verified_switch_pc(&self, raw: i64) -> Option<Option<usize>>;

    /// Recompute the eager-instruction stack proof.
    fn refresh_stack_verification(&mut self);

    /// Seal hand-assembled instructions through the decoding normalizer.
    fn seal_hand_assembled_ops(&mut self);

    /// Unit-test alias for [`Self::seal_hand_assembled_ops`].
    #[cfg(test)]
    fn seal_hand_assembled_ops_for_test(&mut self);

    /// Restore GNU bytecode from its canonical byte stream.
    fn restore_gnu_decode_policy(&mut self) -> Result<(), super::decode::DecodeError>;

    /// Executable instructions, decoding a cold GNU byte string on first use.
    fn executable_ops(&self) -> &[Op];

    /// GNU byte offsets corresponding to [`Self::executable_ops`].
    fn executable_gnu_byte_offset_map(&self) -> Option<&[GnuByteOffsetMapEntry]>;
}

impl ByteCodeFunctionExt for ByteCodeFunction {
    /// The tiering/profiling handle. Every function that can be observed
    /// outside the pdump loader carries `Some` (the chokepoints materialize
    /// stubs before handing data out), so the unwrap compiles to nothing —
    /// `Option<Runtime>` has the null-pointer niche (compile-asserted below).
    #[cfg(feature = "jit")]
    #[inline(always)]
    fn jit_runtime(&self) -> &crate::emacs_core::jit::Runtime {
        debug_assert!(
            self.runtime.is_some(),
            "jit runtime read on an unmaterialized pdump stub"
        );
        // SAFETY: `runtime` is `None` only on a pdump stub, and stubs are
        // unreachable here (chokepoints materialize first; see field doc).
        unsafe { self.runtime.as_ref().unwrap_unchecked() }
    }
    /// The constant base a compiled leaf of this function reads its
    /// `make-closure`-patched constants through (the leaf entry's `aux`
    /// word): the first word of the constant pool, owned or mapped. The one
    /// place the JIT takes it from (`jit_layout::bytecode_constants_offsets`
    /// is the same word, read from the object).
    #[cfg(feature = "jit")]
    #[inline(always)]
    fn jit_constant_base(&self) -> *const crate::emacs_core::value::Value {
        self.constants.as_ptr()
    }
    /// Release already-validated GNU decoded IR until it is first needed.
    /// The original byte string remains the single source of truth.
    fn defer_gnu_decode(&mut self) {
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
    /// Operand-stack entry depth the verifier must assume, mirroring
    /// `run_frame`'s argument staging exactly: params-on-stack conventions
    /// seed `nonrest` slots plus one `&rest` list slot; pure dynamic-binding
    /// functions enter with an empty operand stack.
    fn verifier_entry_depth(&self) -> Option<usize> {
        let params_on_stack = match &self.params {
            FunctionParams::Stack(_) => true,
            FunctionParams::Dynamic(_) => false,
            FunctionParams::Named(_) => {
                self.lexical
                    || self.env.is_some()
                    || matches!(self.arglist.kind(), ValueKind::Fixnum(_))
            }
        };
        if params_on_stack {
            self.params.stack_shape()?.entry_depth().ok()
        } else {
            Some(0)
        }
    }
    /// Admission for GNU-backed code, whose parameter/stack contract is
    /// immutable and shared by all clones of this decode cell. Eager code
    /// retains the VM's original classification path.
    ///
    /// Captured environments are per-instance, so they must never enter the
    /// shared proof. A clone with an environment cannot poison its env-less
    /// prototype's admission, or consume admission from that prototype.
    #[inline(always)]
    fn lazy_iterative_admission(&self, nargs: usize) -> Option<bool> {
        let lazy = self.lazy_gnu_code.as_ref()?;
        if self.env.is_some() {
            return Some(false);
        }
        if let Some(range) = lazy.iterative_admission.get() {
            return Some(range.is_some_and(|range| range.accepts(nargs)));
        }
        Some(self.initialize_lazy_iterative_admission(nargs))
    }

    /// Initialize the immutable admission proof without imposing a save frame
    /// on the cached probe. Only the lazy, env-less miss above calls this.
    #[cold]
    #[inline(never)]
    fn initialize_lazy_iterative_admission(&self, nargs: usize) -> bool {
        let lazy = self
            .lazy_gnu_code
            .as_ref()
            .expect("lazy admission initialization needs GNU metadata");
        debug_assert!(self.env.is_none());
        let Some(shape) = self.params.stack_shape() else {
            return false;
        };
        let required = shape.required();
        let nonrest = shape.nonrest();
        // Match the original short circuit: a wrong-arity call must neither
        // decode code for verification nor publish a permanent refusal for
        // a later valid call with a different argument count.
        if nargs < required || nargs > nonrest {
            return false;
        }
        let params_on_stack = match &self.params {
            FunctionParams::Stack(_) => true,
            FunctionParams::Dynamic(_) => false,
            FunctionParams::Named(_) => {
                self.lexical || matches!(self.arglist.kind(), ValueKind::Fixnum(_))
            }
        };
        let range = if !shape.rest().is_present()
            && (nonrest == 0 || params_on_stack)
            && nonrest <= self.max_stack.get()
            && self.max_stack <= StackDepth::ITERATIVE_FRAME_LIMIT
            && self.executes_sealed_ops()
            && self.executes_verified_ops()
        {
            Some(IterativeArgRange {
                // Both counts fit because nonrest <= max_stack <= u16::MAX,
                // and this call already proved required <= nargs <= nonrest.
                required: required as u16,
                nonrest: nonrest as u16,
            })
        } else {
            None
        };
        // Clones share the same immutable source contract. Concurrent
        // classifiers therefore publish the same range/refusal; a losing
        // set need not retry or wait before using its own identical answer.
        let _ = lazy.iterative_admission.set(range);
        range.is_some()
    }

    /// Whether `executable_ops` returns instructions with a standing
    /// `verify_stack_effects` proof. Resolves the lazy decode if needed —
    /// callers are classification-time, never the dispatch hot path.
    fn executes_verified_ops(&self) -> bool {
        match &self.lazy_gnu_code {
            Some(lazy) => {
                if let Some(decoded) = lazy.decoded_ready() {
                    return decoded.stack_verified;
                }
                let raw_bytes = self
                    .gnu_bytecode_bytes
                    .as_deref()
                    .expect("lazy GNU bytecode lost its original bytes");
                decode_lazy_gnu_code(
                    lazy,
                    raw_bytes,
                    self.constants.len(),
                    || (self.verifier_entry_depth(), self.constants.as_slice()),
                    self.max_stack.get(),
                )
                .stack_verified
            }
            None => self.stack_verified,
        }
    }
    /// Resolve a switch hit through the standing proof, if this function has
    /// one. All its reachable switches share the post-pop depth recorded at
    /// verification, so this needs no live depth from the dispatch arm.
    /// A mutated table may name any instruction proven at that fixed depth.
    /// Admitted values use the pre-resolved table; only new values search
    /// the byte-offset map on the cold path.
    ///
    /// The outer `None` leaves unverified/eager functions with the checked
    /// resolver's existing behavior. `Some(None)` rejects an unsafe target.
    #[inline(never)]
    fn verified_switch_pc(&self, raw: i64) -> Option<Option<usize>> {
        let lazy = self.lazy_gnu_code.as_ref()?;
        let decoded = lazy.decoded_ready()?;
        if !decoded.stack_verified {
            return None;
        }
        Some(lazy.proof.get().and_then(|proof| {
            let target = match proof.switch_target(raw) {
                Some(target) => target,
                None => decoded.unadmitted_switch_target(raw)?,
            };
            let depth = proof.switch_depth?;
            proof.proves_depth(target, depth).then_some(target)
        }))
    }
    /// Recompute the operand-stack proof for eager instructions. Call at
    /// the last point before publication, after every shape field
    /// (params/lexical/arglist/env/max_stack) has its final value.
    fn refresh_stack_verification(&mut self) {
        self.stack_verified = self.verifier_entry_depth().is_some_and(|depth| {
            super::decode::verify_stack_effects(&self.ops, depth, self.max_stack.get())
        });
    }
    /// Run hand-assembled instructions through the real sealing normalizer
    /// so synthesized chunks may enter the unchecked-fetch driver. For the
    /// lib-internal AOT testkit and the unit-test harness only — production
    /// bytecode is sealed exclusively by the decode installers. Call after
    /// `ops` and `constants` are both in place; no-op for GNU-backed or
    /// already-sealed functions.
    fn seal_hand_assembled_ops(&mut self) {
        if self.executes_sealed_ops() || self.gnu_bytecode_bytes.is_some() {
            return;
        }
        self.ops = super::decode::seal_ops(std::mem::take(&mut self.ops), self.constants.len());
        self.ops_sealed = true;
        self.stack_verified = self.verifier_entry_depth().is_some_and(|depth| {
            super::decode::verify_stack_effects(&self.ops, depth, self.max_stack.get())
        });
    }
    /// Unit-test alias for [`Self::seal_hand_assembled_ops`].
    #[cfg(test)]
    fn seal_hand_assembled_ops_for_test(&mut self) {
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
    fn restore_gnu_decode_policy(&mut self) -> Result<(), super::decode::DecodeError> {
        let raw_bytes = self
            .gnu_bytecode_bytes
            .as_deref()
            .expect("restored GNU bytecode requires original bytes");
        self.lazy_gnu_code = None;
        if eager_gnu_bytecode() {
            let (ops, byte_offset_map) = super::decode::decode_gnu_bytecode_with_offset_map(
                raw_bytes,
                &mut self.constants.ensure_owned(),
            )?;
            self.ops = ops;
            self.ops_sealed = true;
            self.stack_verified = self.verifier_entry_depth().is_some_and(|depth| {
                super::decode::verify_stack_effects(&self.ops, depth, self.max_stack.get())
            });
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
    fn executable_ops(&self) -> &[Op] {
        match &self.lazy_gnu_code {
            Some(lazy) => {
                let decoded = match lazy.decoded_ready() {
                    Some(decoded) => decoded,
                    None => {
                        let raw_bytes = self
                            .gnu_bytecode_bytes
                            .as_deref()
                            .expect("lazy GNU bytecode lost its original bytes");
                        decode_lazy_gnu_code(
                            lazy,
                            raw_bytes,
                            self.constants.len(),
                            || (self.verifier_entry_depth(), self.constants.as_slice()),
                            self.max_stack.get(),
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
    fn executable_gnu_byte_offset_map(&self) -> Option<&[GnuByteOffsetMapEntry]> {
        match &self.lazy_gnu_code {
            Some(lazy) => {
                let decoded = match lazy.decoded_ready() {
                    Some(decoded) => decoded,
                    None => {
                        let raw_bytes = self
                            .gnu_bytecode_bytes
                            .as_deref()
                            .expect("lazy GNU bytecode lost its original bytes");
                        decode_lazy_gnu_code(
                            lazy,
                            raw_bytes,
                            self.constants.len(),
                            || (self.verifier_entry_depth(), self.constants.as_slice()),
                            self.max_stack.get(),
                        )
                    }
                };
                let map = &decoded.byte_offset_map;
                (!map.is_empty()).then_some(map)
            }
            None => self.gnu_byte_offset_map.as_deref(),
        }
    }
    /// Disassemble to a human-readable string.
    fn disassemble(&self) -> String {
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
#[path = "tests/chunk_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/iterative_admission_test.rs"]
mod iterative_admission_test;
