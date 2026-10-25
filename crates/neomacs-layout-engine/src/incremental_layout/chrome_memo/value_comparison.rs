//! Call-local keys for the two plain memo comparison domains.
//!
//! Keys borrow the current formatted occurrence or property-bearing string's
//! lifetime. The caller keeps that owner and its sources scratch-rooted and
//! performs no Lisp call or collection while using the keys, as at the existing
//! fingerprint/source classification sites. A scope is a lifetime anchor, not
//! a GC root.
//! Nothing from this module belongs in the retained row fingerprint.
//!
//! Consumer integration:
//! - Each `PlainEncoder::string_props` call creates a local `for_string` scope
//!   from its existing borrowed string. Its temporary property entries become
//!   `(CapturedMemoKey, Value)`; deduplicate via `equivalent`, sort and encode
//!   via `original`, and preserve the first entry. Capture `display` and the
//!   three name-only symbols through this same scope at their old sites. The
//!   encoder needs no lifetime parameter, scope field, or comparison policy.
//! - Both source-projection paths create a `for_formatted` scope and capture
//!   the root once and each encountered source once. Keep first-seen
//!   `(key, source_id)` entries; alias/root checks use `equivalent`, and fresh
//!   bindings publish `original()`, never a key.
//! - Preserve operand order, comparison order, admission/encoding order, and
//!   the existing source/root publication site. Do not retain a key or its
//!   scope in `ChromeRowFingerprint`, a replay, or transport state.
//!
//! Source contracts: `tagged/value.rs` immediate extractors and tag-only string
//! predicate; `emacs_core/runtime/value/mod.rs` `eq_value`, `as_lisp_string`,
//! and the plain string arm of `equal_value_inner`; `heap_types.rs` `schars`
//! and `as_bytes`. No new equality rule is supplied for any Other/mixed pair.

#![deny(clippy::wildcard_enum_match_arm)]

use std::marker::PhantomData;

use neovm_core::buffer::CharLen;
use neovm_core::emacs_core::intern::SymId;
use neovm_core::emacs_core::value::{Value, eq_value};
use neovm_core::emacs_core::xdisp::ModeLineDisplayOutput;
use neovm_core::heap_types::LispString;

/// One existing owner bounds every key captured for its local memo walk.
/// No comparison results or string bodies are retained by this scope.
#[derive(Debug)]
pub(super) struct MemoKeyScope<'owner> {
    _owner: PhantomData<(&'owner (), std::rc::Rc<()>)>,
}

static_assertions::assert_not_impl_any!(MemoKeyScope<'static>: Send, Sync);
const _: () = assert!(std::mem::size_of::<MemoKeyScope<'static>>() == 0);

impl<'owner> MemoKeyScope<'owner> {
    pub(super) fn for_formatted(formatted: &'owner ModeLineDisplayOutput) -> Self {
        Self::anchored_to(formatted)
    }

    pub(super) fn for_string(string: &'owner LispString) -> Self {
        Self::anchored_to(string)
    }

    /// Fixtures can anchor keys to their existing rooted Value places without
    /// adding an unrooted, owner-free constructor to the production API.
    #[cfg(test)]
    pub(super) fn for_values(values: &'owner [Value]) -> Self {
        Self::anchored_to(values)
    }

    fn anchored_to<Owner: ?Sized>(_owner: &'owner Owner) -> Self {
        Self {
            _owner: PhantomData,
        }
    }

    /// Classify through immediate extractors and a tag-only string predicate.
    /// In particular, do not inspect an Other heap object's header here.
    #[inline]
    pub(super) fn capture(&self, original: Value) -> CapturedMemoKey<'owner> {
        let kind = if let Some(number) = original.as_fixnum() {
            CapturedMemoKind::Primitive {
                original,
                key: PrimitiveMemoKey::Fixnum(number),
            }
        } else if let Some(symbol) = original.as_bare_symbol_id() {
            CapturedMemoKind::Primitive {
                original,
                key: PrimitiveMemoKey::BareSymbol(symbol),
            }
        } else if original.is_string() {
            CapturedMemoKind::String(StringOwner { original })
        } else {
            CapturedMemoKind::Other(original)
        };
        CapturedMemoKey {
            kind,
            _owner: PhantomData,
        }
    }
}

/// Pure immediate data; differing variants have differing primitive meanings.
#[derive(Debug, PartialEq, Eq)]
enum PrimitiveMemoKey {
    Fixnum(i64),
    BareSymbol(SymId),
}

/// The existing plain string projection. Slice equality includes byte length;
/// storage mode and properties intentionally do not participate.
#[derive(Debug, PartialEq, Eq)]
struct PlainStringKey<'read> {
    characters: CharLen,
    bytes: &'read [u8],
}

/// The private constructor is reached only after the tag-only string test.
/// Keeping its original Value is deliberately lazy: capture reads no payload.
#[derive(Debug)]
struct StringOwner {
    original: Value,
}

impl StringOwner {
    #[inline]
    fn plain(&self) -> Option<PlainStringKey<'_>> {
        let string = self.original.as_lisp_string()?;
        Some(PlainStringKey {
            characters: CharLen::new(string.schars()),
            bytes: string.as_bytes(),
        })
    }
}

#[derive(Debug)]
enum CapturedMemoKind {
    Primitive {
        original: Value,
        key: PrimitiveMemoKey,
    },
    String(StringOwner),
    Other(Value),
}

/// A lazy category plus the exact original representative.
///
/// There is deliberately no derived equality on this outer type: cross-domain
/// and Other comparisons belong to the existing core Value predicate. Value's
/// native mutator confinement also prevents sending or sharing captured keys.
#[derive(Debug)]
pub(super) struct CapturedMemoKey<'owner> {
    kind: CapturedMemoKind,
    _owner: PhantomData<&'owner ()>,
}

static_assertions::assert_not_impl_any!(CapturedMemoKey<'static>: Send, Sync);
const _: () = assert!(
    std::mem::size_of::<CapturedMemoKey<'static>>() == std::mem::size_of::<CapturedMemoKind>()
);

impl CapturedMemoKey<'_> {
    /// Publish/use the original first binding; a key never substitutes a new
    /// string owner or canonicalizes two structurally equal source Values.
    #[inline]
    pub(super) fn original(&self) -> Value {
        match &self.kind {
            CapturedMemoKind::Primitive { original, .. } => *original,
            CapturedMemoKind::String(owner) => owner.original,
            CapturedMemoKind::Other(original) => *original,
        }
    }

    /// One semantic dispatch shared by property names and source aliases.
    /// String views live only inside this call; the identity boundary and all
    /// fallback pairs preserve the core comparison's collection-read behavior.
    #[inline]
    pub(super) fn equivalent(&self, other: &Self) -> bool {
        match (&self.kind, &other.kind) {
            (
                CapturedMemoKind::Primitive { key: left, .. },
                CapturedMemoKind::Primitive { key: right, .. },
            ) => left == right,
            (CapturedMemoKind::String(left), CapturedMemoKind::String(right)) => {
                if eq_value(&left.original, &right.original) {
                    return true;
                }
                // Evaluate both owner reads, matching the core string pair.
                // Failure remains false rather than adding an assertion.
                match (left.plain(), right.plain()) {
                    (Some(left), Some(right)) => left == right,
                    (None, Some(_)) | (Some(_), None) | (None, None) => false,
                }
            }
            (CapturedMemoKind::Primitive { .. }, CapturedMemoKind::String(_))
            | (CapturedMemoKind::Primitive { .. }, CapturedMemoKind::Other(_))
            | (CapturedMemoKind::String(_), CapturedMemoKind::Primitive { .. })
            | (CapturedMemoKind::String(_), CapturedMemoKind::Other(_))
            | (CapturedMemoKind::Other(_), CapturedMemoKind::Primitive { .. })
            | (CapturedMemoKind::Other(_), CapturedMemoKind::String(_))
            | (CapturedMemoKind::Other(_), CapturedMemoKind::Other(_)) => {
                self.original() == other.original()
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/chrome_value_comparison_test.rs"]
mod tests;
