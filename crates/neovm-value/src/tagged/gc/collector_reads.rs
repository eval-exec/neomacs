//! Collector-only pointer projections. Marking, ownership checks and barrier
//! metadata reads do not read Lisp program data for dependency tracking.
//!
//! These accessors never publish through heap TLS, so a protected collector
//! loan cannot be re-entered from a root-derived observation-window read.
//! Public Lisp projections keep their observation behavior in value.rs.

use super::{ConsCell, StringObj, TaggedValue, VecLikeHeader, VecLikeType};
use crate::tagged::value::TAG_MASK;

impl TaggedValue {
    /// The collector has already discriminated a cons tag. Extracting the
    /// masked address is total; dereferencing still requires a live cons.
    #[inline(always)]
    pub(super) fn gc_cons_ptr(self) -> *const ConsCell {
        (self.bits() & !TAG_MASK) as *const ConsCell
    }

    #[inline(always)]
    pub(super) fn gc_string_ptr(self) -> Option<*const StringObj> {
        if self.is_string() {
            Some((self.bits() & !TAG_MASK) as *const StringObj)
        } else {
            None
        }
    }

    #[inline(always)]
    pub(super) fn gc_veclike_ptr(self) -> Option<*const VecLikeHeader> {
        if self.is_veclike() {
            Some((self.bits() & !TAG_MASK) as *const VecLikeHeader)
        } else {
            None
        }
    }

    #[inline(always)]
    pub(super) fn gc_heap_ptr(self) -> Option<*const u8> {
        if self.is_heap_object() {
            Some((self.bits() & !TAG_MASK) as *const u8)
        } else {
            None
        }
    }

    /// `kind()` observes vectorlike headers. Other tags never project an
    /// observed pointer, so keep their existing decoding and sentinel rules.
    #[inline(always)]
    pub(super) fn gc_kind(self) -> crate::tagged::value::ValueKind {
        if self.is_veclike() {
            let ptr = (self.bits() & !TAG_MASK) as *const VecLikeHeader;
            // SAFETY: Value retains an initialized object, as for kind().
            crate::tagged::value::ValueKind::Veclike(unsafe { (*ptr).type_tag })
        } else {
            self.kind()
        }
    }

    /// Collector/admission classification of a retained object's header.
    #[inline(always)]
    pub(super) fn gc_veclike_type(self) -> Option<VecLikeType> {
        self.gc_veclike_ptr().map(|ptr| {
            // SAFETY: the same retained, initialized Value contract as the
            // public projection, without publishing a Lisp read dependency.
            unsafe { (*ptr).type_tag }
        })
    }
}
