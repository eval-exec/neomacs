//! Lower ownership contracts retained for the full parallel collection era.
use super::symbol_authority::KeeperWorldDesignator;
use super::{HeapIdentity, RegistrationError};
use crate::tagged::value::TaggedValue;

/// The census either excludes every suspended legacy activation or cannot
/// authorize function-cell replacement during the era.
#[derive(Clone, Copy)]
pub(crate) enum FrozenFunctionRoots {
    Quiescent,
    MayBeActive,
}

/// Census of the original Context, whose execution is suspended for the era.
/// These words never cross an owner thread or confer running access.
#[derive(Debug)]
pub(crate) struct FrozenLegacyRoots {
    heap: HeapIdentity,
    words: Vec<TaggedValue>,
    functions_quiescent: bool,
}

impl FrozenLegacyRoots {
    /// # Safety
    /// The words must be the complete census of the suspended original Context
    /// of this heap, and remain valid for the era that retains that Context's
    /// actual keeper. Quiescent certifies that no suspended legacy Lisp/native
    /// activation or outstanding compiled job may read a replaced function.
    /// Only eval/world may mint this census, before its exclusive heap loan.
    pub(crate) unsafe fn new(
        heap: HeapIdentity,
        words: Vec<TaggedValue>,
        functions: FrozenFunctionRoots,
    ) -> Self {
        Self {
            heap,
            words,
            functions_quiescent: matches!(functions, FrozenFunctionRoots::Quiescent),
        }
    }

    pub(crate) fn permits_function_writes(&self) -> bool {
        self.functions_quiescent
    }
    pub(crate) fn heap_identity(&self) -> HeapIdentity {
        self.heap
    }
    /// Copies opaque words; decoding requires stopped authority for this heap.
    pub(crate) fn append_words(&self, out: &mut Vec<TaggedValue>) {
        out.extend_from_slice(&self.words);
    }
}

static_assertions::assert_not_impl_any!(FrozenLegacyRoots: Send, Sync, Clone, Copy);

/// The actual serial-execution authority, owned by value throughout an era.
///
/// # Safety
/// The implementor must be the retained World's unique serial-execution
/// authority, held on this thread for as long as the era owns it. Its heap and
/// original manager identities must agree, and a successful designator borrow
/// must keep those allocations live and excluded from serial execution.
/// Dropping or leaking the keeper must preserve its existing retention rules.
pub unsafe trait EraKeeper {
    fn keeper_designator(&self) -> Result<KeeperWorldDesignator<'_>, RegistrationError>;
    fn parallel_heap_identity(&self) -> HeapIdentity;
}
