//! Owned key representation only; all GNU property resolution stays in the bridge.
use super::{LayoutBufferView, LayoutVar, Value};

/// One lookup owns these copied Value handles for its synchronous layout view.
/// The view retains the existing referent roots; this introduces no new root or
/// lifetime boundary. Independent attempts/mutators own distinct containers,
/// with no shared Lisp cache, Context pointer or cross-attempt publication.
#[derive(Clone, Debug)]
pub(super) struct PropertyKeyOrder {
    canonical: Value,
    aliases: Vec<Value>,
}

impl PropertyKeyOrder {
    #[inline]
    pub(super) fn capture<B: LayoutBufferView + ?Sized>(buffer: &B, property: Value) -> Self {
        if !enabled() {
            // Keep the literal legacy allocation and alias-growth sequence.
            // Extract the canonical once, so queries need no storage tag.
            let mut aliases = capture_heap_order(buffer, property);
            let canonical = aliases.remove(0);
            return Self { canonical, aliases };
        }
        #[cfg(test)]
        super::property_keys_test_support::note_inline_construction();
        let mut lookup_order = Vec::new();
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
                    if alias.bits() != property.bits()
                        && !lookup_order
                            .iter()
                            .any(|existing: &Value| existing.bits() == alias.bits())
                    {
                        if lookup_order.is_empty() {
                            #[cfg(test)]
                            super::property_keys_test_support::note_heap_materialization();
                            // Preserve the original first-distinct-alias upgrade
                            // and subsequent capacity growth before extracting
                            // the canonical from the completed ordered vector.
                            lookup_order = vec![property];
                            #[cfg(test)]
                            super::property_keys_test_support::note_alias_upgrade();
                        }
                        lookup_order.push(alias);
                    }
                    aliases = aliases.cons_cdr();
                }
                break;
            }
        }
        if !lookup_order.is_empty() {
            lookup_order.remove(0);
        }
        Self {
            canonical: property,
            aliases: lookup_order,
        }
    }

    /// The constructor always owns a canonical key, including nil. Borrowed
    /// aliases retain their first-match order and never contain that key.
    #[inline]
    pub(super) fn canonical_and_aliases(&self) -> (Value, &[Value]) {
        (self.canonical, &self.aliases)
    }

    /// Reconstruct the same ordered keys for extent watches and endpoint
    /// signatures. No Value escapes the owning synchronous layout view.
    #[inline]
    pub(super) fn ordered(&self) -> impl Iterator<Item = Value> + '_ {
        std::iter::once(self.canonical).chain(self.aliases.iter().copied())
    }
}

/// Literal legacy construction and alias control flow. This remains the OFF
/// branch; the caller extracts its canonical once after capture completes.
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

/// Absence alone selects ON; explicit invalid/empty/nonUnicode input remains OFF.
#[inline]
fn parse(value: Option<&std::ffi::OsStr>) -> bool {
    if value.is_none() {
        return true;
    }
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
