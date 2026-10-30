//! `impl Value` surface for byte-code objects.
//!
//! These accessors live with the byte-code representation they project: the
//! materialization chokepoint (`get_bytecode_data`), the caller-proof
//! projections the VM and JIT use, and the constructor entry points. Callers
//! keep using `Value` methods; only the defining module moved, so the value
//! layer no longer names the byte-code module, its function type, or the
//! pdump materializer.

use crate::emacs_core::value::{BytecodeInteractiveProbe, Value};
use crate::tagged::gc::with_tagged_heap;
use crate::tagged::header::{ByteCodeObj, VecLikeType};
use crate::tagged::value::{TAG_MASK, TaggedValue};

impl TaggedValue {
    /// Allocate a bytecode function.
    pub fn make_bytecode(bc: super::ByteCodeFunction) -> Self {
        super::semantics::install();
        // Test builds hand-assemble instruction vectors all over the suite;
        // route them through the real sealing normalizer so they may enter
        // the unchecked-fetch driver. Production sealing happens exclusively
        // in the decode installers — this branch must never widen to release
        // builds, or the `ops_sealed` marker would stop proving anything.
        #[cfg(test)]
        let bc = {
            let mut bc = bc;
            bc.seal_hand_assembled_ops_for_test();
            bc
        };
        with_tagged_heap(|h| h.alloc_bytecode(bc))
    }

    /// Allocate a `make-closure` instance of `proto` with the constant pool
    /// `constants` and the prototype's code-string object `code`, built in
    /// place in its arena slot (see `TaggedHeap::alloc_bytecode_instance`).
    pub(crate) fn make_bytecode_instance(
        proto: &super::ByteCodeFunction,
        constants: crate::tagged::header::LispValueVec,
        code: Value,
    ) -> Self {
        super::semantics::install();
        with_tagged_heap(|h| h.alloc_bytecode_instance(proto, constants, code))
    }

    /// Borrow the ByteCodeFunction from a ByteCode value.
    ///
    /// THE materialization seam: a lazy pdump stub is built from its mapped
    /// extras here, once, on the mutator thread, before any caller sees the
    /// data. Everything that reads bytecode data routes through this (the
    /// architecture test pins it), so a stub can never leak empty vectors.
    pub fn get_bytecode_data(self) -> Option<&'static super::ByteCodeFunction> {
        #[cfg(test)]
        crate::emacs_core::value::note_bytecode_data_access();
        // Observe before the header and data projections, which share this
        // identity and have no intervening Lisp call or write.
        let header = self.as_veclike_ptr()?;
        if unsafe { (*header).type_tag } == VecLikeType::ByteCode {
            let ptr = header as *const ByteCodeObj;
            let data = unsafe { &(*ptr).data };
            if data.is_pdump_stub() {
                crate::emacs_core::pdump::materialize_and_publish_stub(self);
            }
            Some(unsafe { &(*ptr).data })
        } else {
            None
        }
    }

    /// Project bytecode data when the caller has established that this
    /// mutator has no active capture at this read. Private synchronous map
    /// traversal can retain that policy across a callback only because every
    /// nested capture finishes before traversal resumes. Prologue hooks and
    /// callback Lisp reads must continue using ordinary observed accessors.
    ///
    /// The caller re-reads and materializes data after its GC/debugger
    /// prologue; this caches no pointer or Lisp state. Other mutators capture
    /// their own reads and do not require this mutator to observe dependencies.
    #[cfg(feature = "jit")]
    #[inline(always)]
    pub(crate) fn get_bytecode_data_unobserved(self) -> Option<&'static super::ByteCodeFunction> {
        debug_assert!(!crate::tagged::collection_reads::is_active());
        #[cfg(test)]
        crate::emacs_core::value::note_bytecode_data_access();
        if !self.is_veclike() {
            return None;
        }
        let header = (self.bits() & !TAG_MASK) as *const crate::tagged::header::VecLikeHeader;
        if unsafe { (*header).type_tag } != VecLikeType::ByteCode {
            return None;
        }
        let ptr = header as *const ByteCodeObj;
        let data = unsafe { &(*ptr).data };
        if data.is_pdump_stub() {
            crate::emacs_core::pdump::materialize_and_publish_stub(self);
        }
        Some(unsafe { &(*ptr).data })
    }

    /// Whether `self` is the byte-code object whose data is `f`: an address
    /// comparison that, unlike [`Self::get_bytecode_data`], never materializes
    /// a lazy pdump stub (a compile asking "is this function cell me?" must
    /// not build every stub it merely looks at).
    pub(crate) fn is_bytecode_object_of(self, f: &super::ByteCodeFunction) -> bool {
        if self.veclike_type() != Some(VecLikeType::ByteCode) {
            return false;
        }
        let ptr = self.as_veclike_ptr().unwrap() as *const ByteCodeObj;
        // SAFETY: `ptr` addresses a live byte-code object (the type test
        // above); `addr_of!` forms the field address without reading it.
        std::ptr::eq(unsafe { std::ptr::addr_of!((*ptr).data) }, f)
    }

    /// [`Self::get_bytecode_data`] for callers that ALREADY proved the
    /// veclike type — the VM's resolved-callee token proves it at mint, and
    /// re-checking on every `code()` projection measured +7.3 Ir/call on the
    /// tier-0 differential. Lives here, at the chokepoint file, because this
    /// is exactly where the lazy pdump stub check (two loads + a predictable
    /// branch to a cold materializer) lands: the caller's type proof does not
    /// exempt it from materialization, only from re-classification.
    ///
    /// SAFETY contract (checked in debug): `self` must be a ByteCode value.
    #[inline(always)]
    pub(crate) fn bytecode_data_typechecked_by_caller(self) -> &'static super::ByteCodeFunction {
        debug_assert_eq!(self.veclike_type(), Some(VecLikeType::ByteCode));
        #[cfg(test)]
        crate::emacs_core::value::note_bytecode_data_access();
        let ptr = (self.bits() & !TAG_MASK) as *const ByteCodeObj;
        // SAFETY: the caller's type proof (debug-asserted above) establishes
        // a live ByteCodeObj; bytecode arena/mapped objects are immovable.
        let data = unsafe { &(*ptr).data };
        if data.is_pdump_stub() {
            crate::emacs_core::pdump::materialize_and_publish_stub(self);
            return unsafe { &(*ptr).data };
        }
        data
    }

    /// [`Self::bytecode_data_typechecked_by_caller`] for a caller that has
    /// ALSO proved the object materialized: the JIT's armed spec site, whose
    /// slot caches a leaf compiled from this very object's instruction
    /// stream (`get_bytecode_data` materialized it before the leaf could be
    /// resolved, and a stub is materialized in place, once, for good). Skips
    /// the type check and the stub probe both — six instructions on a call
    /// path that runs five million times in a package load.
    ///
    /// # Safety
    /// `self` must be a ByteCode value whose data was materialized by an
    /// earlier `get_bytecode_data` on the same object (debug-asserted).
    #[cfg(feature = "jit")]
    #[inline(always)]
    pub(crate) unsafe fn bytecode_data_materialized_by_caller(
        self,
    ) -> &'static super::ByteCodeFunction {
        debug_assert_eq!(self.veclike_type(), Some(VecLikeType::ByteCode));
        #[cfg(test)]
        crate::emacs_core::value::note_bytecode_data_access();
        let ptr = (self.bits() & !TAG_MASK) as *const ByteCodeObj;
        // SAFETY: the caller's type and materialization proofs (debug-asserted)
        // establish a live, filled ByteCodeObj; bytecode arena/mapped objects
        // are immovable.
        let data = unsafe { &(*ptr).data };
        debug_assert!(
            !data.is_pdump_stub(),
            "caller promised a materialized function"
        );
        data
    }

    /// [`Self::get_bytecode_data`] that promises NOT to materialize a lazy
    /// pdump stub (once stubs exist): the peek for scanners that only care
    /// about already-live functions — AOT post-insert marking, PGO drains.
    /// Today (pre-stub) it is the same borrow.
    pub(crate) fn bytecode_data_if_materialized(self) -> Option<&'static super::ByteCodeFunction> {
        if self.veclike_type()? != VecLikeType::ByteCode {
            return None;
        }
        let ptr = self.as_veclike_ptr().unwrap() as *const ByteCodeObj;
        let data = unsafe { &(*ptr).data };
        (!data.is_pdump_stub()).then_some(data)
    }

    /// The command-classification facts of a bytecode value, WITHOUT forcing
    /// full materialization (once stubs exist, this reads the raw mapped
    /// extras header): closure slot count plus the interactive/doc-form
    /// slots. Serving `commandp`/`interactive-form`/`command-modes` through
    /// this probe keeps the first obarray-wide M-x sweep from materializing
    /// every dumped function in one burst.
    /// Raw-header required-only check for a still-stub function; `None` when
    /// the value is not bytecode, `Some(materialized-or-raw answer)` else —
    /// without materializing a stub.
    pub(crate) fn bytecode_params_required_only_probe(self) -> Option<bool> {
        if self.veclike_type()? != VecLikeType::ByteCode {
            return None;
        }
        let ptr = self.as_veclike_ptr().unwrap() as *const ByteCodeObj;
        let data = unsafe { &(*ptr).data };
        if data.is_pdump_stub() {
            return Some(unsafe {
                crate::emacs_core::pdump::stub_params_required_only(ptr, data.closure_slot_count)
            });
        }
        Some(data.params.fixed_arity().is_some())
    }

    /// The required-parameter count of a byte-code function that takes only
    /// required parameters, `None` for any other function or value — read
    /// from a lazy pdump stub's raw header without materializing it.
    pub(crate) fn bytecode_required_only_arity_probe(self) -> Option<usize> {
        if self.veclike_type()? != VecLikeType::ByteCode {
            return None;
        }
        let ptr = self.as_veclike_ptr().unwrap() as *const ByteCodeObj;
        let data = unsafe { &(*ptr).data };
        if data.is_pdump_stub() {
            return unsafe {
                crate::emacs_core::pdump::stub_required_only_arity(ptr, data.closure_slot_count)
            };
        }
        data.params.fixed_arity()
    }

    pub(crate) fn bytecode_interactive_probe(self) -> Option<BytecodeInteractiveProbe> {
        if self.veclike_type()? != VecLikeType::ByteCode {
            return None;
        }
        let ptr = self.as_veclike_ptr().unwrap() as *const ByteCodeObj;
        let data = unsafe { &(*ptr).data };
        if data.is_pdump_stub() {
            // Read the raw mapped extras header — the whole point of this
            // probe is that an obarray-wide commandp/interactive sweep must
            // not materialize 6,779 functions in one burst.
            return Some(unsafe {
                crate::emacs_core::pdump::stub_interactive_probe(ptr, data.closure_slot_count)
            });
        }
        Some(BytecodeInteractiveProbe {
            slot_count: data.observable_closure_slot_count(),
            interactive: data.interactive,
            doc_form: data.doc_form,
        })
    }
}
