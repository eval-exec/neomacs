//! GNU's character-category word-wrap rules, independent of row production.
//!
//! The policy is plain frozen data: category ranges projected from the
//! buffer's category table. Projection itself binds the text layer's category
//! machinery, so the upper display layer performs it through
//! `EditHost::word_wrap_policy`; the value that crosses down is owned here,
//! together with the mutator-local dependency cache it was projected from.

use neovm_value::tagged::collection_reads::CollectionReads;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WordWrapMode {
    #[default]
    Whitespace,
    Category,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WordWrapDirection {
    #[default]
    Forward,
    Reversed,
}

/// Only the three categories consumed by GNU's char_can_wrap_* predicates.
/// This projection owns no Lisp objects and can cross a worker boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WordWrapCategories(u8);

impl WordWrapCategories {
    fn from_bits(bits: u128) -> Self {
        Self(
            u8::from(bits & (1 << b'|') != 0)
                | (u8::from(bits & (1 << b'>') != 0) << 1)
                | (u8::from(bits & (1 << b'<') != 0) << 2),
        )
    }
}

/// Owned, immutable wrapping semantics; range ordering stays private.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WordWrapPolicy(WordWrapPolicyKind);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum WordWrapPolicyKind {
    #[default]
    Whitespace,
    Category(Arc<[WordWrapCategoryRange]>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WordWrapCategoryRange {
    start: u32,
    end: u32,
    categories: WordWrapCategories,
}

/// Mutator-local dependency metadata, kept outside the worker-safe policy.
/// Identities are never dereferenced. Reclamation invalidates them before
/// address reuse, and the certificate checks both mutation and World ownership.
#[derive(Debug)]
pub struct CachedWordWrapPolicy {
    source: CategoryProjectionSource,
    dependencies: CollectionReads,
    policy: WordWrapPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CategoryProjectionSource {
    table: usize,
    reclamation: u64,
}

static_assertions::assert_not_impl_any!(CachedWordWrapPolicy: Send, Sync);

impl WordWrapPolicy {
    /// A policy that breaks on whitespace only (GNU's `word-wrap` nil arm).
    pub fn whitespace() -> Self {
        Self(WordWrapPolicyKind::Whitespace)
    }

    /// Freeze a category projection into adjacent equal-category ranges.
    /// No category-table or bool-vector Lisp object is retained.
    pub fn category_from_runs(runs: impl IntoIterator<Item = (u32, u32, u128)>) -> Self {
        let mut ranges: Vec<WordWrapCategoryRange> = Vec::new();
        for (start, end, bits) in runs {
            let categories = WordWrapCategories::from_bits(bits);
            if categories == WordWrapCategories::default() {
                continue;
            }
            if let Some(previous) = ranges.last_mut()
                && previous.end.checked_add(1) == Some(start)
                && previous.categories == categories
            {
                previous.end = end;
            } else {
                ranges.push(WordWrapCategoryRange {
                    start,
                    end,
                    categories,
                });
            }
        }
        Self(WordWrapPolicyKind::Category(ranges.into()))
    }

    pub fn opportunity(&self, code: u32, direction: WordWrapDirection) -> WordWrapOpportunity {
        match &self.0 {
            WordWrapPolicyKind::Whitespace => {
                WordWrapMode::Whitespace.opportunity(code, WordWrapCategories::default(), direction)
            }
            WordWrapPolicyKind::Category(ranges) => {
                let index = ranges.partition_point(|range| range.end < code);
                let categories = ranges
                    .get(index)
                    .filter(|range| range.start <= code)
                    .map_or(WordWrapCategories::default(), |range| range.categories);
                WordWrapMode::Category.opportunity(code, categories, direction)
            }
        }
    }
}

static_assertions::assert_impl_all!(WordWrapPolicy: Send, Sync);

impl CachedWordWrapPolicy {
    /// Record one projection's identity and collection dependencies.
    pub fn new(
        table: usize,
        reclamation: u64,
        dependencies: CollectionReads,
        policy: WordWrapPolicy,
    ) -> Self {
        Self {
            source: CategoryProjectionSource { table, reclamation },
            dependencies,
            policy,
        }
    }

    /// Whether this cache entry belongs to the same category-table epoch.
    pub fn matches(&self, table: usize, reclamation: u64) -> bool {
        self.source.table == table && self.source.reclamation == reclamation
    }

    /// Observe and report whether the collected dependencies are unchanged.
    pub fn dependencies_unchanged_and_observe(&self) -> bool {
        self.dependencies.unchanged_and_observe()
    }

    pub fn policy(&self) -> WordWrapPolicy {
        self.policy.clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WordWrapOpportunity {
    before: bool,
    after: bool,
}

impl WordWrapOpportunity {
    pub const fn before(self) -> bool {
        self.before
    }

    pub const fn after(self) -> bool {
        self.after
    }
}

impl WordWrapMode {
    /// GNU xdisp.c:577–617. A reversed glyph row swaps the kinsoku ends.
    pub fn opportunity(
        self,
        code: u32,
        categories: WordWrapCategories,
        direction: WordWrapDirection,
    ) -> WordWrapOpportunity {
        // GNU it_char_has_category treats a zero character as absent.
        let categories = if code == 0 {
            WordWrapCategories::default()
        } else {
            categories
        };
        let whitespace = matches!(code, 0x20 | 0x09);
        let (not_at_start, not_at_end) = match direction {
            WordWrapDirection::Forward => (2, 4),
            WordWrapDirection::Reversed => (4, 2),
        };
        WordWrapOpportunity {
            before: !whitespace && (self == Self::Whitespace || categories.0 & not_at_start == 0),
            after: whitespace
                || (self == Self::Category
                    && categories.0 & 1 != 0
                    && categories.0 & not_at_end == 0),
        }
    }
}
