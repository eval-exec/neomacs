//! Sealed atomic-read view of GC-managed vector, record, and closure slots.
//!
//! Rust module privacy seals `LispValueSlice.0` and `LispValueSliceIter.inner`
//! inside this leaf module: no safe code anywhere outside this file can obtain
//! `&[AtomicTaggedValue]`, `&[TaggedValue]`, or `&TaggedValue` from a
//! `&LispValueSlice` or `LispValueSliceIter`.
//!
//! This module carries zero `use` declarations, zero macro invocations, and an
//! exact budget of two `unsafe` blocks — both in the constructors
//! `LispValueSlice::from_atomic_slice` and
//! `LispValueSlice::from_raw_atomic_parts` — audited by the
//! `slice_view_unsafe_budget_fence` test. Unsafe transmutes written in other
//! modules are governed by workspace unsafe review and Miri SB+TB.

/// Borrowed view of GC-managed vector, record, or closure slots backed by
/// interior-mutable [`super::header::AtomicTaggedValue`] cells (or immutable
/// mapped pdump words).
///
/// Phase 1b: exposes only atomic relaxed value loads (never `&[TaggedValue]`
/// or `&TaggedValue`), so token-free single-slot atomic stores
/// (`set_vector_slot`, `set_record_slot`, `set_closure_slot`) never alias an
/// immutable `&[TaggedValue]` reference under Stacked Borrows or Tree Borrows.
///
/// Promotion semantics: on an `Owned` backing, a live `&LispValueSlice`
/// observes subsequent single-slot atomic stores on the same mutator thread.
/// On a `Mapped` (pdump) backing, the first single-slot store copy-promotes
/// into a fresh `Owned` buffer without mutating the mapped image; any
/// previously borrowed `&LispValueSlice` remains sound and immutable as a
/// snapshot of the pre-promotion mapped slots, and callers needing post-write
/// values must re-borrow.
///
/// Layout note: this `#[repr(transparent)]` attribute exists so
/// `from_atomic_slice` and `from_raw_atomic_parts` are layout-defined; it is
/// not a safety boundary.
#[repr(transparent)]
pub struct LispValueSlice([super::header::AtomicTaggedValue]);

/// Iterator yielding [`super::value::TaggedValue`] elements by relaxed atomic
/// load from a [`LispValueSlice`].
#[derive(::core::clone::Clone, ::core::fmt::Debug)]
pub struct LispValueSliceIter<'a> {
    inner: ::core::slice::Iter<'a, super::header::AtomicTaggedValue>,
}

impl ::core::iter::Iterator for LispValueSliceIter<'_> {
    type Item = super::value::TaggedValue;

    #[inline(always)]
    fn next(&mut self) -> ::core::option::Option<Self::Item> {
        self.inner
            .next()
            .map(super::header::AtomicTaggedValue::load_relaxed)
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, ::core::option::Option<usize>) {
        self.inner.size_hint()
    }

    #[inline(always)]
    fn count(self) -> usize {
        self.inner.len()
    }

    #[inline(always)]
    fn nth(&mut self, n: usize) -> ::core::option::Option<Self::Item> {
        self.inner
            .nth(n)
            .map(super::header::AtomicTaggedValue::load_relaxed)
    }

    #[inline(always)]
    fn last(self) -> ::core::option::Option<Self::Item> {
        self.inner
            .last()
            .map(super::header::AtomicTaggedValue::load_relaxed)
    }
}

impl ::core::iter::DoubleEndedIterator for LispValueSliceIter<'_> {
    #[inline(always)]
    fn next_back(&mut self) -> ::core::option::Option<Self::Item> {
        self.inner
            .next_back()
            .map(super::header::AtomicTaggedValue::load_relaxed)
    }

    #[inline(always)]
    fn nth_back(&mut self, n: usize) -> ::core::option::Option<Self::Item> {
        self.inner
            .nth_back(n)
            .map(super::header::AtomicTaggedValue::load_relaxed)
    }
}

impl ::core::iter::ExactSizeIterator for LispValueSliceIter<'_> {
    #[inline(always)]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl ::core::iter::FusedIterator for LispValueSliceIter<'_> {}

impl LispValueSlice {
    #[inline(always)]
    pub(crate) fn from_atomic_slice(slice: &[super::header::AtomicTaggedValue]) -> &Self {
        // SAFETY: `LispValueSlice` is `#[repr(transparent)]` over
        // `[super::header::AtomicTaggedValue]`.
        unsafe { &*(slice as *const [super::header::AtomicTaggedValue] as *const Self) }
    }

    /// Construct a borrowed `&LispValueSlice` from a raw mapped slot pointer
    /// and element length.
    ///
    /// # Safety
    /// `ptr..ptr+len` must be valid for shared
    /// `super::header::AtomicTaggedValue` reads for lifetime `'a` (matching
    /// `LispValueVec::mapped`'s provenance contract).
    #[inline(always)]
    pub(crate) unsafe fn from_raw_atomic_parts<'a>(
        ptr: *const super::header::AtomicTaggedValue,
        len: usize,
    ) -> &'a Self {
        // SAFETY: caller guarantees `ptr..ptr+len` is valid for shared
        // `AtomicTaggedValue` reads for `'a`, and `LispValueSlice` is
        // `#[repr(transparent)]` over `[AtomicTaggedValue]`.
        unsafe { &*(::core::ptr::slice_from_raw_parts(ptr, len) as *const Self) }
    }

    #[inline(always)]
    pub(crate) fn empty() -> &'static Self {
        Self::from_atomic_slice(&[])
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Relaxed atomic load of slot `index`, or `None` if out of bounds.
    #[inline(always)]
    pub fn get(&self, index: usize) -> ::core::option::Option<super::value::TaggedValue> {
        self.0
            .get(index)
            .map(super::header::AtomicTaggedValue::load_relaxed)
    }

    /// Relaxed atomic load of slot `index`, panicking if out of bounds.
    #[inline(always)]
    pub fn at(&self, index: usize) -> super::value::TaggedValue {
        self.0[index].load_relaxed()
    }

    /// Relaxed atomic load of the first slot, if non-empty.
    #[inline(always)]
    pub fn first(&self) -> ::core::option::Option<super::value::TaggedValue> {
        self.0
            .first()
            .map(super::header::AtomicTaggedValue::load_relaxed)
    }

    /// Relaxed atomic load of the last slot, if non-empty.
    #[inline(always)]
    pub fn last(&self) -> ::core::option::Option<super::value::TaggedValue> {
        self.0
            .last()
            .map(super::header::AtomicTaggedValue::load_relaxed)
    }

    /// Split off the first slot by relaxed atomic load plus the remaining slice.
    #[inline(always)]
    pub fn split_first(&self) -> ::core::option::Option<(super::value::TaggedValue, &Self)> {
        let (first, rest) = self.0.split_first()?;
        ::core::option::Option::Some((first.load_relaxed(), Self::from_atomic_slice(rest)))
    }

    /// Split off the last slot by relaxed atomic load plus the preceding slice.
    #[inline(always)]
    pub fn split_last(&self) -> ::core::option::Option<(super::value::TaggedValue, &Self)> {
        let (last, rest) = self.0.split_last()?;
        ::core::option::Option::Some((last.load_relaxed(), Self::from_atomic_slice(rest)))
    }

    /// Borrow a sub-slice by range without exposing `&[TaggedValue]`.
    #[inline(always)]
    pub fn get_slice<I>(&self, index: I) -> ::core::option::Option<&Self>
    where
        I: ::core::slice::SliceIndex<
                [super::header::AtomicTaggedValue],
                Output = [super::header::AtomicTaggedValue],
            >,
    {
        self.0.get(index).map(Self::from_atomic_slice)
    }

    /// Iterate over all slots by relaxed atomic load.
    #[inline(always)]
    pub fn iter(&self) -> LispValueSliceIter<'_> {
        LispValueSliceIter {
            inner: self.0.iter(),
        }
    }

    /// Return `true` if any slot equals `value` (by structural `PartialEq` / `equal_value` equality).
    #[inline]
    pub fn contains(&self, value: &super::value::TaggedValue) -> bool {
        self.iter().any(|item| item == *value)
    }

    /// Materialize an owned `Vec<TaggedValue>` via relaxed atomic loads.
    #[inline]
    pub fn to_vec(&self) -> ::std::vec::Vec<super::value::TaggedValue> {
        let mut out = ::std::vec::Vec::with_capacity(self.0.len());
        out.extend(self.iter());
        out
    }

    // This unsized slice view cannot implement `Clone`; the historical method
    // intentionally materializes an owned vector.
    #[allow(clippy::should_implement_trait)]
    #[inline]
    pub fn clone(&self) -> ::std::vec::Vec<super::value::TaggedValue> {
        self.to_vec()
    }
}

impl ::core::ops::Index<::core::ops::Range<usize>> for LispValueSlice {
    type Output = Self;

    #[inline(always)]
    fn index(&self, index: ::core::ops::Range<usize>) -> &Self::Output {
        Self::from_atomic_slice(&self.0[index])
    }
}

impl ::core::ops::Index<::core::ops::RangeFrom<usize>> for LispValueSlice {
    type Output = Self;

    #[inline(always)]
    fn index(&self, index: ::core::ops::RangeFrom<usize>) -> &Self::Output {
        Self::from_atomic_slice(&self.0[index])
    }
}

impl ::core::ops::Index<::core::ops::RangeTo<usize>> for LispValueSlice {
    type Output = Self;

    #[inline(always)]
    fn index(&self, index: ::core::ops::RangeTo<usize>) -> &Self::Output {
        Self::from_atomic_slice(&self.0[index])
    }
}

impl ::core::ops::Index<::core::ops::RangeFull> for LispValueSlice {
    type Output = Self;

    #[inline(always)]
    fn index(&self, _index: ::core::ops::RangeFull) -> &Self::Output {
        self
    }
}

impl ::core::ops::Index<::core::ops::RangeInclusive<usize>> for LispValueSlice {
    type Output = Self;

    #[inline(always)]
    fn index(&self, index: ::core::ops::RangeInclusive<usize>) -> &Self::Output {
        Self::from_atomic_slice(&self.0[index])
    }
}

impl ::core::ops::Index<::core::ops::RangeToInclusive<usize>> for LispValueSlice {
    type Output = Self;

    #[inline(always)]
    fn index(&self, index: ::core::ops::RangeToInclusive<usize>) -> &Self::Output {
        Self::from_atomic_slice(&self.0[index])
    }
}

impl ::core::fmt::Debug for LispValueSlice {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl ::core::cmp::PartialEq for LispValueSlice {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl ::core::cmp::Eq for LispValueSlice {}

impl ::core::cmp::PartialEq<[super::value::TaggedValue]> for LispValueSlice {
    fn eq(&self, other: &[super::value::TaggedValue]) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter().copied())
    }
}

impl ::core::cmp::PartialEq<LispValueSlice> for [super::value::TaggedValue] {
    fn eq(&self, other: &LispValueSlice) -> bool {
        other == self
    }
}

impl ::core::cmp::PartialEq<&[super::value::TaggedValue]> for LispValueSlice {
    fn eq(&self, other: &&[super::value::TaggedValue]) -> bool {
        self == *other
    }
}

impl ::core::cmp::PartialEq<LispValueSlice> for &[super::value::TaggedValue] {
    fn eq(&self, other: &LispValueSlice) -> bool {
        other == *self
    }
}

impl<const N: usize> ::core::cmp::PartialEq<[super::value::TaggedValue; N]> for LispValueSlice {
    fn eq(&self, other: &[super::value::TaggedValue; N]) -> bool {
        self == other.as_slice()
    }
}

impl<const N: usize> ::core::cmp::PartialEq<&[super::value::TaggedValue; N]> for LispValueSlice {
    fn eq(&self, other: &&[super::value::TaggedValue; N]) -> bool {
        self == other.as_slice()
    }
}

impl<const N: usize> ::core::cmp::PartialEq<LispValueSlice> for [super::value::TaggedValue; N] {
    fn eq(&self, other: &LispValueSlice) -> bool {
        other == self.as_slice()
    }
}

impl<const N: usize> ::core::cmp::PartialEq<LispValueSlice> for &[super::value::TaggedValue; N] {
    fn eq(&self, other: &LispValueSlice) -> bool {
        other == self.as_slice()
    }
}

impl ::core::cmp::PartialEq<::std::vec::Vec<super::value::TaggedValue>> for LispValueSlice {
    fn eq(&self, other: &::std::vec::Vec<super::value::TaggedValue>) -> bool {
        self == other.as_slice()
    }
}

impl ::core::cmp::PartialEq<LispValueSlice> for ::std::vec::Vec<super::value::TaggedValue> {
    fn eq(&self, other: &LispValueSlice) -> bool {
        other == self.as_slice()
    }
}

impl<'a> ::core::iter::IntoIterator for &'a LispValueSlice {
    type Item = super::value::TaggedValue;
    type IntoIter = LispValueSliceIter<'a>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> ::core::iter::IntoIterator for &'a &'a LispValueSlice {
    type Item = super::value::TaggedValue;
    type IntoIter = LispValueSliceIter<'a>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
