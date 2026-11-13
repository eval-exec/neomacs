//! Upper native entry ABI and cold canonical construction policy.
use neovm_value::intern::{SymId, symbol_name_id};
use neovm_value::tagged::header::{SubrMetadata, SubrStorageLayout, VecLikeHeader};
use neovm_value::tagged::value::{CanonicalSubrFactory, TaggedValue};
/// Heap-allocated built-in function (like GNU's PVEC_SUBR).
/// Contains a GNU-shaped fixed-arity or variadic entry point together with
/// arity metadata stored on the SubrObj itself.
pub type SubrFnMany = fn(
    &mut crate::emacs_core::eval::Context,
    Vec<neovm_value::tagged::value::TaggedValue>,
) -> neovm_value::error::EvalResult;
/// Variadic native subroutine that does not need evaluator state.
///
/// Keeping this shape distinct avoids erasing the implementation's actual
/// dependency behind an adapter closure that accepts and discards `Context`.
pub type SubrFnManyNoContext =
    fn(Vec<neovm_value::tagged::value::TaggedValue>) -> neovm_value::error::EvalResult;
pub type SubrFnManySlice = fn(
    &mut crate::emacs_core::eval::Context,
    &[neovm_value::tagged::value::TaggedValue],
) -> neovm_value::error::EvalResult;
pub type SubrFn0 = fn(&mut crate::emacs_core::eval::Context) -> neovm_value::error::EvalResult;
pub type SubrFn1 = fn(
    &mut crate::emacs_core::eval::Context,
    neovm_value::tagged::value::TaggedValue,
) -> neovm_value::error::EvalResult;
pub type SubrFn2 = fn(
    &mut crate::emacs_core::eval::Context,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
) -> neovm_value::error::EvalResult;
pub type SubrFn3 = fn(
    &mut crate::emacs_core::eval::Context,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
) -> neovm_value::error::EvalResult;
pub type SubrFn4 = fn(
    &mut crate::emacs_core::eval::Context,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
) -> neovm_value::error::EvalResult;
pub type SubrFn5 = fn(
    &mut crate::emacs_core::eval::Context,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
) -> neovm_value::error::EvalResult;
pub type SubrFn6 = fn(
    &mut crate::emacs_core::eval::Context,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
) -> neovm_value::error::EvalResult;
pub type SubrFn7 = fn(
    &mut crate::emacs_core::eval::Context,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
) -> neovm_value::error::EvalResult;
pub type SubrFn8 = fn(
    &mut crate::emacs_core::eval::Context,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
    neovm_value::tagged::value::TaggedValue,
) -> neovm_value::error::EvalResult;

#[derive(Clone, Copy)]
pub enum SubrFn {
    Many(SubrFnMany),
    ManyNoContext(SubrFnManyNoContext),
    ManySlice(SubrFnManySlice),
    A0(SubrFn0),
    A1(SubrFn1),
    A2(SubrFn2),
    A3(SubrFn3),
    A4(SubrFn4),
    A5(SubrFn5),
    A6(SubrFn6),
    A7(SubrFn7),
    A8(SubrFn8),
}

pub(crate) type SubrObj = neovm_value::tagged::header::SubrObj<SubrFn>;

pub(crate) struct EvalCanonicalSubrFactory;
// SAFETY: the only production factory constructs the asserted SubrFn instance,
// copies the authoritative catalog registration (or exact pre-install fallback),
// and leaks that allocation. Refresh uses the existing selected lock protocol.
unsafe impl CanonicalSubrFactory for EvalCanonicalSubrFactory {
    fn allocate(sym_id: SymId) -> TaggedValue {
        allocate_static_subr_object(sym_id)
    }
}

/// Upper-local extension trait remains coherent when TaggedValue moves lower.
pub trait SubrValueExt: Sized {
    fn subr_from_sym_id(sym_id: SymId) -> Self;
    fn subr(sym_id: SymId) -> Self;
}
impl SubrValueExt for TaggedValue {
    #[inline]
    fn subr_from_sym_id(sym_id: SymId) -> Self {
        Self::subr_from_sym_id::<EvalCanonicalSubrFactory>(sym_id)
    }
    #[inline]
    fn subr(sym_id: SymId) -> Self {
        <Self as SubrValueExt>::subr_from_sym_id(sym_id)
    }
}
pub(crate) fn mint_static_subr_objects(ids: impl IntoIterator<Item = SymId>) {
    neovm_value::tagged::value::mint_static_subr_objects::<EvalCanonicalSubrFactory>(ids);
}
fn allocate_static_subr_object(sym_id: SymId) -> TaggedValue {
    let name_id = symbol_name_id(sym_id);
    let entry = crate::emacs_core::eval::lookup_global_subr_entry(sym_id);
    let (function, min_args, max_args, dispatch_kind, interactivity) = if let Some(entry) = entry {
        (
            entry.function,
            entry.min_args,
            entry.max_args,
            entry.dispatch_kind,
            neovm_value::tagged::header::SubrInteractivity::from(entry.interactive_spec.is_some()),
        )
    } else {
        (
            None,
            0,
            None,
            neovm_value::tagged::header::SubrDispatchKind::Builtin,
            neovm_value::tagged::header::SubrInteractivity::NonInteractive,
        )
    };

    let metadata = SubrMetadata::new(
        sym_id,
        name_id,
        min_args,
        max_args,
        dispatch_kind,
        interactivity,
    );
    // SAFETY: this sole factory obtains the actual catalog-backed registration
    // or its exact pre-install fallback; layout assertions below bind the entry
    // specialization. Initialization is exclusive, refresh retains its lock.
    let ptr = allocate_initialized_subr(metadata, function);
    // SAFETY: the initialized canonical allocation is permanent and retains
    // the unique upper entry ABI; its real metadata/header start at offset zero.
    unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
}

// Compile-time assertions run on BOTH supported hosts, not a Linux-only cfg.
const _: () = {
    use std::mem::{align_of, needs_drop, offset_of, size_of};
    assert!(size_of::<Option<SubrFn>>() == size_of::<[usize; 2]>());
    assert!(align_of::<Option<SubrFn>>() == align_of::<[usize; 2]>());
    assert!(size_of::<SubrObj>() == size_of::<SubrStorageLayout>());
    assert!(align_of::<SubrObj>() == align_of::<SubrStorageLayout>());
    assert!(SubrStorageLayout::METADATA_OFFSET == 0);
    assert!(SubrObj::METADATA_OFFSET == 0);
    assert!(SubrObj::FUNCTION_OFFSET == SubrStorageLayout::ENTRY_OFFSET);
    assert!(!needs_drop::<SubrObj>());
};

/// Single allocation leaf for the real ABI, including cfg(test) collector probes.
fn allocate_initialized_subr(metadata: SubrMetadata, function: Option<SubrFn>) -> *mut SubrObj {
    // SAFETY: all callers are this upper policy module; metadata comes from
    // canonical construction or the explicit cfg(test) collector fixture.
    // SubrFn is Copy/static, the assertions bind every layout, and publication
    // follows full initialization. No entry refresh can overlap allocation.
    Box::leak(Box::new(unsafe {
        SubrObj::new_unchecked(metadata, function)
    }))
}

#[cfg(test)]
#[repr(C)]
struct PreviousSubrLayout {
    header: VecLikeHeader,
    sym_id: SymId,
    name: neovm_value::intern::NameId,
    min_args: u16,
    max_args: Option<u16>,
    dispatch_kind: neovm_value::tagged::header::SubrDispatchKind,
    interactivity: neovm_value::tagged::header::SubrInteractivity,
    function: Option<SubrFn>,
}
// The pre-cut field layout is independent of the new nested prefix; compile
// these exact assertions on Linux and macOS, without reading enum padding.
#[cfg(test)]
const _: () = {
    use std::mem::{align_of, offset_of, size_of};
    assert!(size_of::<PreviousSubrLayout>() == size_of::<SubrObj>());
    assert!(align_of::<PreviousSubrLayout>() == align_of::<SubrObj>());
    assert!(offset_of!(PreviousSubrLayout, function) == SubrObj::FUNCTION_OFFSET);
    let old = [
        offset_of!(PreviousSubrLayout, header),
        offset_of!(PreviousSubrLayout, sym_id),
        offset_of!(PreviousSubrLayout, name),
        offset_of!(PreviousSubrLayout, min_args),
        offset_of!(PreviousSubrLayout, max_args),
        offset_of!(PreviousSubrLayout, dispatch_kind),
        offset_of!(PreviousSubrLayout, interactivity),
    ];
    let mut i = 0;
    while i < old.len() {
        assert!(old[i] == SubrMetadata::FIELD_OFFSETS[i]);
        i += 1;
    }
};

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn leaked_subr_metadata_for_test() -> (TaggedValue, *mut SubrMetadata) {
    use neovm_value::intern::NameId;
    use neovm_value::tagged::header::{SubrDispatchKind, SubrInteractivity};
    let metadata = SubrMetadata::new(
        unsafe { SymId::from_raw(1) },
        NameId::for_test(1),
        1,
        Some(2),
        SubrDispatchKind::Builtin,
        SubrInteractivity::NonInteractive,
    );
    let ptr = allocate_initialized_subr(metadata, None);
    // SAFETY: the test owns the fully initialized actual specialization;
    // the real first metadata/header subobject has the allocation's address.
    (
        unsafe { TaggedValue::from_veclike_ptr(ptr.cast()) },
        ptr.cast(),
    )
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn builtin_hof_entry_for_test(
    operation: neovm_value::tagged::gc::HofOperation,
) -> super::SubrEntry {
    use neovm_value::tagged::gc::HofOperation;
    let body: SubrFn2 = match operation {
        HofOperation::Mapc => crate::emacs_core::builtins::builtin_mapc_2,
        HofOperation::Mapcar => crate::emacs_core::builtins::builtin_mapcar_2,
    };
    super::SubrEntry {
        function: Some(SubrFn::A2(body)),
        min_args: 2,
        max_args: Some(2),
        dispatch_kind: neovm_value::tagged::header::SubrDispatchKind::Builtin,
        interactive_spec: None,
    }
}

#[cfg(test)]
#[path = "subr_abi/tests/subr_abi_test.rs"]
mod tests;

/// Authoritative builtin registration entry.  The Lisp-visible static
/// [`SubrObj`] mirrors its directly observable GNU object metadata; the table
/// retains Rust-only dispatch data and the complete interactive spec.
///
/// P1-SUBR (peer 15:2xZ; peer 16:xxZ mandatory change; peer P1-HOOKS-2 gate
/// fix): the item is `pub(crate)` with a test-support `visibility::make(pub)`
/// (the WorldSlot pattern), so the eval authority fence remains its only
/// public path; every field stays `pub(crate)` and no field is widened. The
/// visibility gate matches the fence module's `cfg(any(test, feature))`, so
/// eval's own cfg(test) build does not re-export a private item (E0365).
#[derive(Clone, Copy)]
#[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
pub(crate) struct SubrEntry {
    pub(crate) function: Option<SubrFn>,
    pub(crate) min_args: u16,
    pub(crate) max_args: Option<u16>,
    pub(crate) dispatch_kind: neovm_value::tagged::header::SubrDispatchKind,
    pub(crate) interactive_spec:
        Option<crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec>,
}
