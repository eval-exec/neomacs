//! `impl Value` surface for byte-code objects.
//!
//! These accessors live with the byte-code representation they project: the
//! materialization chokepoint (`get_bytecode_data`), the caller-proof
//! projections the VM and JIT use, and the constructor entry points. Callers
//! keep using `Value` methods; only the defining module moved, so the value
//! layer no longer names the byte-code module, its function type, or the
//! pdump materializer.

use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::bytecode::function_slots::FunctionParamsExt as _;
use neovm_value::heap_types::LispString;
use neovm_value::tagged::gc::with_tagged_heap;
use neovm_value::tagged::header::{ByteCodeObj, ByteCodeSlotObject, VecLikeType};
use neovm_value::tagged::mutate;
use neovm_value::tagged::value::{TAG_MASK, TaggedValue};
use neovm_value::value::{BytecodeInteractiveProbe, FunctionSourceIdentity, Value};

/// Byte-code object surface on the value layer's `TaggedValue`.
///
/// The byte-code representation, its function type and the pdump
/// materializer live above the value layer, so these accessors and
/// constructors stay upper as an extension trait on the moved type.
pub trait TaggedValueBytecodeExt {
    fn make_bytecode(bc: super::ByteCodeFunction) -> Self;

    fn make_bytecode_instance(
        proto: &super::ByteCodeFunction,
        constants: neovm_value::tagged::header::LispValueVec,
        code: Value,
    ) -> Self;

    fn get_bytecode_data(self) -> Option<&'static super::ByteCodeFunction>;

    #[cfg(feature = "jit")]
    fn get_bytecode_data_unobserved(self) -> Option<&'static super::ByteCodeFunction>;

    fn is_bytecode_object_of(self, f: &super::ByteCodeFunction) -> bool;

    fn bytecode_data_typechecked_by_caller(self) -> &'static super::ByteCodeFunction;

    #[cfg(feature = "jit")]
    unsafe fn bytecode_data_materialized_by_caller(self) -> &'static super::ByteCodeFunction;

    fn bytecode_data_if_materialized(self) -> Option<&'static super::ByteCodeFunction>;

    fn bytecode_params_required_only_probe(self) -> Option<bool>;

    fn bytecode_required_only_arity_probe(self) -> Option<usize>;

    fn bytecode_interactive_probe(self) -> Option<BytecodeInteractiveProbe>;
}

impl TaggedValueBytecodeExt for TaggedValue {
    /// Allocate a bytecode function.
    fn make_bytecode(bc: super::ByteCodeFunction) -> Self {
        // Test builds hand-assemble instruction vectors all over the suite;
        // route them through the real sealing normalizer so they may enter
        // the unchecked-fetch driver. Production sealing happens exclusively
        // in the decode installers — this branch must never widen to release
        // builds, or the `ops_sealed` marker would stop proving anything.
        #[cfg(any(test, feature = "test-support"))]
        let bc = {
            let mut bc = bc;
            bc.seal_hand_assembled_ops();
            bc
        };
        with_tagged_heap(|h| h.alloc_bytecode(bc))
    }

    /// Allocate a `make-closure` instance of `proto` with the constant pool
    /// `constants` and the prototype's code-string object `code`, built in
    /// place in its arena slot (see `TaggedHeap::alloc_bytecode_instance`).
    fn make_bytecode_instance(
        proto: &super::ByteCodeFunction,
        constants: neovm_value::tagged::header::LispValueVec,
        code: Value,
    ) -> Self {
        with_tagged_heap(|h| h.alloc_bytecode_instance(proto, constants, code))
    }

    /// Borrow the ByteCodeFunction from a ByteCode value.
    ///
    /// THE materialization seam: a lazy pdump stub is built from its mapped
    /// extras here, once, on the mutator thread, before any caller sees the
    /// data. Everything that reads bytecode data routes through this (the
    /// architecture test pins it), so a stub can never leak empty vectors.
    fn get_bytecode_data(self) -> Option<&'static super::ByteCodeFunction> {
        #[cfg(test)]
        neovm_value::value::note_bytecode_data_access();
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
    fn get_bytecode_data_unobserved(self) -> Option<&'static super::ByteCodeFunction> {
        debug_assert!(!neovm_value::tagged::collection_reads::is_active());
        #[cfg(test)]
        neovm_value::value::note_bytecode_data_access();
        if !self.is_veclike() {
            return None;
        }
        let header = (self.bits() & !TAG_MASK) as *const neovm_value::tagged::header::VecLikeHeader;
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
    fn is_bytecode_object_of(self, f: &super::ByteCodeFunction) -> bool {
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
    fn bytecode_data_typechecked_by_caller(self) -> &'static super::ByteCodeFunction {
        debug_assert_eq!(self.veclike_type(), Some(VecLikeType::ByteCode));
        #[cfg(test)]
        neovm_value::value::note_bytecode_data_access();
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
    unsafe fn bytecode_data_materialized_by_caller(self) -> &'static super::ByteCodeFunction {
        debug_assert_eq!(self.veclike_type(), Some(VecLikeType::ByteCode));
        #[cfg(test)]
        neovm_value::value::note_bytecode_data_access();
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
    fn bytecode_data_if_materialized(self) -> Option<&'static super::ByteCodeFunction> {
        // ONE observed projection for the header and the data, the same shape
        // as `get_bytecode_data`: `veclike_type()` followed by
        // `as_veclike_ptr()` records the same collection-read dependency
        // twice, and this accessor is the JIT call-spec fast path's charge
        // projection (dispatch.rs reads the callee's `max_stack` there).
        let header = self.as_veclike_ptr()?;
        // SAFETY: `as_veclike_ptr` proved a live veclike header; `type_tag`
        // is at the same address and is readable for every veclike object.
        if unsafe { (*header).type_tag } != VecLikeType::ByteCode {
            return None;
        }
        let ptr = header as *const ByteCodeObj;
        // SAFETY: the type tag proved a byte-code object; arena/mapped
        // byte-code objects are immovable and live for this read.
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
    fn bytecode_params_required_only_probe(self) -> Option<bool> {
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
    fn bytecode_required_only_arity_probe(self) -> Option<usize> {
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

    fn bytecode_interactive_probe(self) -> Option<BytecodeInteractiveProbe> {
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
        Some(BytecodeInteractiveProbe::new(
            data.observable_closure_slot_count(),
            data.interactive,
            data.doc_form,
        ))
    }
}

/// Lisp-facing byte-code identity and slot surface on the value layer's
/// `TaggedValue`.
///
/// These operations were inherent methods in the moving value module. They
/// stay `pub(crate)`: the public byte-code surface remains
/// [`TaggedValueBytecodeExt`], while the printer, profiler and `aref` slot
/// machinery reach these crate-local projections through this trait.
pub(crate) trait TaggedValueBytecodePolicyExt {
    fn function_source_identity(self) -> Option<FunctionSourceIdentity>;

    fn function_equal(self, other: Value) -> bool;

    fn bytecode_slot_object(self, slot: ByteCodeSlotObject) -> Option<Value>;

    fn bytecode_slot_object_if_created(self, slot: ByteCodeSlotObject) -> Option<Value>;
}

impl TaggedValueBytecodePolicyExt for TaggedValue {
    fn function_source_identity(self) -> Option<FunctionSourceIdentity> {
        // GNU `function-equal` (Ffunction_equal, profiler.c) only treats two
        // distinct objects as equal when both are COMPILEDP and share their
        // bytecode object. Interpreted-function closures (the Emacs 30+ `#[args
        // body env]` records) fall through to the EQ-only case there, so they
        // must NOT advertise a shared source identity here -- two instances of
        // the same lambda expression with different captured environments are
        // not `function-equal`.
        self.get_bytecode_data()
            .map(|function| FunctionSourceIdentity::ByteCode(function.source_id()))
    }

    /// GNU `function-equal`: two compiled closures are equal when they share
    /// their bytecode object; every other function compares by identity (EQ).
    fn function_equal(self, other: Value) -> bool {
        if self.bits() == other.bits() {
            return true;
        }
        matches!(
            (self.function_source_identity(), other.function_source_identity()),
            (Some(left), Some(right)) if left == right
        )
    }

    /// The Lisp object `aref` returns for byte-code closure slot `slot`: one
    /// object for the function's life, as GNU's pseudovector slot is
    /// (`(eq (aref f 2) (aref f 2))` is t). Created on the first read and
    /// kept in the object (`ByteCodeObj::slot_objects`); never read by code,
    /// so mutating it does not change what the function computes (P3.2 L4b).
    ///
    /// `Code` is `nil` for a function with no GNU byte string. `Constants`
    /// is the captured environment alist for a NeoVM-compiled closure, which
    /// is already a Lisp object. `None` when `self` is not byte code.
    #[inline]
    fn bytecode_slot_object(
        self,
        slot: neovm_value::tagged::header::ByteCodeSlotObject,
    ) -> Option<Value> {
        if self.veclike_type()? != VecLikeType::ByteCode {
            return None;
        }
        let ptr = self.as_veclike_ptr().unwrap() as *const ByteCodeObj;
        // SAFETY: a live byte-code object (the type test above).
        let cached = unsafe { (*ptr).slot_objects.get(slot) };
        if !cached.is_nil() {
            return Some(cached);
        }
        Some(self.create_bytecode_slot_object(slot))
    }

    /// [`Self::bytecode_slot_object`] without creating anything: the object
    /// Lisp may already hold, or `None` when none was created yet (so no
    /// Lisp reference to it can exist) or `self` is not byte code. For the
    /// printer, which must show `print-circle` the shared object when there
    /// is one, but need not keep a fresh one alive.
    fn bytecode_slot_object_if_created(
        self,
        slot: neovm_value::tagged::header::ByteCodeSlotObject,
    ) -> Option<Value> {
        if self.veclike_type()? != VecLikeType::ByteCode {
            return None;
        }
        let ptr = self.as_veclike_ptr().unwrap() as *const ByteCodeObj;
        // SAFETY: a live byte-code object (the type test above).
        let cached = unsafe { (*ptr).slot_objects.get(slot) };
        (!cached.is_nil()).then_some(cached)
    }
}

/// Private slot-object construction, kept out of the crate-visible policy
/// trait to preserve the original private visibility.
trait TaggedValueBytecodeSlotCreationExt {
    fn create_bytecode_slot_object(self, slot: ByteCodeSlotObject) -> Value;
}

impl TaggedValueBytecodeSlotCreationExt for TaggedValue {
    #[cold]
    #[inline(never)]
    fn create_bytecode_slot_object(
        self,
        slot: neovm_value::tagged::header::ByteCodeSlotObject,
    ) -> Value {
        use neovm_value::tagged::header::ByteCodeSlotObject;
        let bc = self
            .get_bytecode_data()
            .expect("caller proved a byte-code value");
        let object = match slot {
            ByteCodeSlotObject::Code => match &bc.gnu_bytecode_bytes {
                // GNU byte-code strings are unibyte: each byte one character.
                Some(bytes) => {
                    Value::heap_string(LispString::from_unibyte(bytes.as_slice().to_vec()))
                }
                None => return Value::NIL,
            },
            ByteCodeSlotObject::Constants => match bc.env {
                Some(env) => return env,
                None => Value::vector(bc.constants.as_slice().to_vec()),
            },
        };
        mutate::install_bytecode_slot_object(self, slot, object);
        object
    }
}
