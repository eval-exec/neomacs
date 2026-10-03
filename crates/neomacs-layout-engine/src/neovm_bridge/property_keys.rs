//! Owned key representation only; all GNU property resolution stays in the bridge.
use super::{LayoutBufferView, LayoutVar, Value};

/// One lookup owns these copied Value handles for its synchronous layout view.
/// The view retains the existing referent roots; this introduces no new root or
/// lifetime boundary. Independent attempts/mutators own distinct containers,
/// with no shared Lisp cache, Context pointer or cross-attempt publication.
#[derive(Clone, Debug)]
pub(super) enum PropertyKeyOrder {
    Inline([Value; 1]),
    Heap(Vec<Value>),
}

impl PropertyKeyOrder {
    #[inline]
    pub(super) fn capture<B: LayoutBufferView + ?Sized>(buffer: &B, property: Value) -> Self {
        if !enabled() {
            return Self::Heap(capture_heap_order(buffer, property));
        }
        #[cfg(test)]
        super::property_keys_test_support::note_inline_construction();
        let mut lookup_order = Self::Inline([property]);
        if let Some(mut alist) = buffer.layout_buffer_local_value(LayoutVar::CharPropertyAliasAlist)
        {
            while alist.is_cons() {
                let entry = alist.cons_car();
                alist = alist.cons_cdr();
                if !entry.is_cons() || entry.cons_car().bits() != property.bits() {
                    continue;
                }
                let mut aliases = entry.cons_cdr();
                while aliases.is_cons() {
                    let alias = aliases.cons_car();
                    if !lookup_order
                        .as_slice()
                        .iter()
                        .any(|existing| existing.bits() == alias.bits())
                    {
                        lookup_order.push_alias(alias);
                    }
                    aliases = aliases.cons_cdr();
                }
                break;
            }
        }
        lookup_order
    }

    /// Borrow the same canonical-first keys; no Value escapes the owning view.
    #[inline]
    pub(super) fn as_slice(&self) -> &[Value] {
        match self {
            Self::Inline(keys) => keys,
            Self::Heap(keys) => keys,
        }
    }

    /// A distinct alias upgrades with the original vec![canonical] then push
    /// sequence, preserving content, alias order and heap capacity growth.
    #[inline]
    fn push_alias(&mut self, alias: Value) {
        match self {
            Self::Heap(keys) => keys.push(alias),
            Self::Inline([canonical]) => {
                #[cfg(test)]
                super::property_keys_test_support::note_heap_materialization();
                let mut keys = vec![*canonical];
                keys.push(alias);
                #[cfg(test)]
                super::property_keys_test_support::note_alias_upgrade();
                *self = Self::Heap(keys);
            }
        }
    }
}

/// Literal legacy construction and alias control flow. This remains the OFF
/// branch; only the caller wraps the resulting owned vector in Heap.
#[inline]
fn capture_heap_order<B: LayoutBufferView + ?Sized>(buffer: &B, property: Value) -> Vec<Value> {
    #[cfg(test)]
    super::property_keys_test_support::note_heap_materialization();
    let mut lookup_order = vec![property];
    if let Some(mut alist) = buffer.layout_buffer_local_value(LayoutVar::CharPropertyAliasAlist) {
        while alist.is_cons() {
            let entry = alist.cons_car();
            alist = alist.cons_cdr();
            if !entry.is_cons() || entry.cons_car().bits() != property.bits() {
                continue;
            }
            let mut aliases = entry.cons_cdr();
            while aliases.is_cons() {
                let alias = aliases.cons_car();
                if !lookup_order
                    .iter()
                    .any(|existing| existing.bits() == alias.bits())
                {
                    lookup_order.push(alias);
                }
                aliases = aliases.cons_cdr();
            }
            break;
        }
    }
    lookup_order
}

/// Pure numeric parser; explicit invalid/empty/nonUnicode input remains OFF.
#[inline]
fn parse(value: Option<&std::ffi::OsStr>) -> bool {
    value
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "on" | "true" | "yes"
            )
        })
}

/// OnceLock publishes one initialized numeric process policy. Independent
/// mutators read no Lisp state. Test-only overrides/counts are numeric only.
#[inline]
fn enabled() -> bool {
    #[cfg(test)]
    if let Some(value) = super::property_keys_test_support::forced() {
        return value;
    }
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED
        .get_or_init(|| parse(std::env::var_os("NEOMACS_LAYOUT_PROPERTY_KEYS_INLINE").as_deref()))
}

#[cfg(test)]
#[path = "tests/property_keys_policy_test.rs"]
mod tests;
