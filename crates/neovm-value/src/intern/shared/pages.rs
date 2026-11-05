//! Immutable, shareable pages with separate symbol and name coordinates.
use super::super::{NameId, SymId};
use std::marker::PhantomData;
use std::sync::Arc;

const PAGE_ROWS: usize = 256;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CacheProbe<T> {
    Hit(T),
    Miss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SnapshotError {
    SpanOverflow,
    OutsideSpan,
    NameOutsideSpan,
    DuplicateTemplate,
    DuplicateHeapName,
    NonMonotonicEpoch,
}

/// Sealed by crate visibility; SymId and NameId never share an index type.
pub(crate) trait RowKey: Copy {
    fn row_word(self) -> u32;
}
impl RowKey for SymId {
    fn row_word(self) -> u32 {
        self.0
    }
}
impl RowKey for NameId {
    fn row_word(self) -> u32 {
        self.0
    }
}

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TableSpan<I> {
    slots: u32,
    key: PhantomData<fn(I) -> I>,
}
impl<I> Copy for TableSpan<I> {}
impl<I> Clone for TableSpan<I> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<I> TableSpan<I> {
    pub(crate) fn try_from_len(slots: usize) -> Result<Self, SnapshotError> {
        Ok(Self {
            slots: slots.try_into().map_err(|_| SnapshotError::SpanOverflow)?,
            key: PhantomData,
        })
    }
    pub(crate) fn len(self) -> usize {
        self.slots as usize
    }
    pub(super) fn extending(self, rows: usize) -> Result<Self, SnapshotError> {
        Self::try_from_len(
            self.len()
                .checked_add(rows)
                .ok_or(SnapshotError::SpanOverflow)?,
        )
    }
}
impl<I: RowKey> TableSpan<I> {
    pub(crate) fn contains(self, id: I) -> bool {
        id.row_word() < self.slots
    }
}

#[derive(Clone, Copy)]
struct PageIndex(usize);
#[derive(Clone, Copy)]
struct RowOffset(usize);
struct PageAddress {
    page: PageIndex,
    row: RowOffset,
}
impl PageAddress {
    fn of<I: RowKey>(id: I) -> Self {
        let index = id.row_word() as usize;
        Self {
            page: PageIndex(index / PAGE_ROWS),
            row: RowOffset(index % PAGE_ROWS),
        }
    }
}

/// Pages and their directory are immutable. A new version shares complete
/// unchanged pages and replaces only its changed page/directory ownership.
pub(crate) struct PagedRows<T, I> {
    span: TableSpan<I>,
    pages: Arc<[Arc<[T]>]>,
}
impl<T, I> Clone for PagedRows<T, I> {
    fn clone(&self) -> Self {
        Self {
            span: self.span,
            pages: Arc::clone(&self.pages),
        }
    }
}
impl<T, I> PagedRows<T, I> {
    pub(crate) fn span(&self) -> TableSpan<I> {
        self.span
    }
}
impl<T: Clone, I: RowKey> PagedRows<T, I> {
    pub(crate) fn from_rows(rows: Vec<T>) -> Result<Self, SnapshotError> {
        let span = TableSpan::try_from_len(rows.len())?;
        let mut pages = Vec::with_capacity(rows.len().div_ceil(PAGE_ROWS));
        let mut rows = rows.into_iter().peekable();
        while rows.peek().is_some() {
            let page: Vec<T> = rows.by_ref().take(PAGE_ROWS).collect();
            pages.push(Arc::from(page));
        }
        Ok(Self {
            span,
            pages: Arc::from(pages),
        })
    }
    pub(crate) fn probe(&self, id: I) -> CacheProbe<&T> {
        if !self.span.contains(id) {
            return CacheProbe::Miss;
        }
        let PageAddress { page, row } = PageAddress::of(id);
        CacheProbe::Hit(&self.pages[page.0][row.0])
    }
    pub(crate) fn replacing(&self, id: I, value: T) -> Result<Self, SnapshotError> {
        if !self.span.contains(id) {
            return Err(SnapshotError::OutsideSpan);
        }
        let PageAddress { page, row } = PageAddress::of(id);
        let mut pages = self.pages.to_vec();
        let mut rows = pages[page.0].to_vec();
        rows[row.0] = value;
        pages[page.0] = Arc::from(rows);
        Ok(Self {
            span: self.span,
            pages: Arc::from(pages),
        })
    }
    pub(crate) fn appending(&self, value: T) -> Result<Self, SnapshotError> {
        self.extending(std::iter::once(value))
    }
    /// One COW directory per batch, one copy of the existing partial tail.
    /// Complete prefix pages are retained, never scanned or row-cloned.
    /// Directory metadata costs O(page count) per nonempty batch; row work is
    /// O(changed rows + at most one partial page), not O(total symbol span).
    pub(crate) fn extending(
        &self,
        rows: impl IntoIterator<Item = T>,
    ) -> Result<Self, SnapshotError> {
        let rows: Vec<T> = rows.into_iter().collect();
        if rows.is_empty() {
            return Ok(self.clone());
        }
        let span = self.span.extending(rows.len())?;
        let mut pages = self.pages.to_vec();
        let mut rows = rows.into_iter().peekable();
        if self.span.len() % PAGE_ROWS != 0 {
            // The private constructors guarantee a tail for a partial span.
            if let Some(tail) = pages.last_mut() {
                let mut copied = tail.to_vec();
                let remaining = PAGE_ROWS - copied.len();
                copied.extend(rows.by_ref().take(remaining));
                *tail = Arc::from(copied);
            }
        }
        while rows.peek().is_some() {
            let page: Vec<T> = rows.by_ref().take(PAGE_ROWS).collect();
            pages.push(Arc::from(page));
        }
        Ok(Self {
            span,
            pages: Arc::from(pages),
        })
    }
}

/// Zero deliberately remains a miss for canonical nil, as in the old cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct CanonicalSlot(u32);
impl CanonicalSlot {
    pub(crate) const fn from_symbol(id: SymId) -> Self {
        Self(id.0)
    }
    pub(crate) const fn probe(self) -> CacheProbe<SymId> {
        match self.0 {
            0 => CacheProbe::Miss,
            id => CacheProbe::Hit(SymId(id)),
        }
    }
}

static_assertions::assert_eq_size!(TableSpan<SymId>, u32);
static_assertions::assert_eq_size!(TableSpan<NameId>, u32);
static_assertions::assert_eq_size!(CanonicalSlot, u32);

#[cfg(test)]
#[path = "tests/pages.rs"]
mod tests;
