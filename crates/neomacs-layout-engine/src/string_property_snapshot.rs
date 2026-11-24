//! Evaluator-local ownership for retained string-property read projections.
//!
//! Capture performs only native reads and root registration. It does not
//! observe dependency freshness; callers preserve their original projection
//! capture points and validate those observations independently.

use neovm_core::emacs_core::{
    Value,
    owned_roots::{OwnedRootRetainer, OwnedRoots},
    value::{borrow_string_text_properties_table_for_value, text_props::TextPropertyReadView},
};

/// Required ownership policy for a string cursor and all its nested frames.
#[derive(Clone, Debug)]
pub(crate) enum StringSourceRootScope {
    Rooted(OwnedRootRetainer),
    /// The entire use of the snapshot permits no Lisp callback, collection,
    /// or source mutation. This is an explicit caller contract, never inferred
    /// from an absent face realizer or an absent display host.
    CallbackFree,
}

/// The original evaluator retired before a source could be retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetiredStringSourceRoots;

impl std::fmt::Display for RetiredStringSourceRoots {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("string source root evaluator has retired")
    }
}

impl std::error::Error for RetiredStringSourceRoots {}

#[derive(Clone, Debug)]
enum StringPropertyRootOwnership {
    Rooted {
        _source: OwnedRoots,
        _plists: OwnedRoots,
    },
    CallbackFree,
}

/// The owner exists even when its string has no property table: display sites
/// retain the source object independently of whether properties are present.
/// A view cannot be extracted while discarding its associated root leases.
#[derive(Clone, Debug)]
pub(crate) struct StringPropertySnapshot {
    view: Option<TextPropertyReadView>,
    _roots: StringPropertyRootOwnership,
}

impl StringSourceRootScope {
    pub(crate) fn capture(
        &self,
        value: Value,
    ) -> Result<StringPropertySnapshot, RetiredStringSourceRoots> {
        match self {
            Self::Rooted(retainer) => {
                // Test the weak capability and root the original source BEFORE
                // dereferencing it. Retirement must not become "no properties".
                let source = retainer.retain([value]).ok_or(RetiredStringSourceRoots)?;
                // No callback, collection, or property write occurs while this
                // temporary table borrow is projected into native ranges.
                let view = borrow_string_text_properties_table_for_value(value)
                    .map(|table| table.read_view());
                // A later source-property replacement can detach these exact
                // original plist graphs. Keep them independently of the source.
                let plists = retainer
                    .retain(view.iter().flat_map(TextPropertyReadView::root_operands))
                    .ok_or(RetiredStringSourceRoots)?;
                Ok(StringPropertySnapshot {
                    view,
                    _roots: StringPropertyRootOwnership::Rooted {
                        _source: source,
                        _plists: plists,
                    },
                })
            }
            Self::CallbackFree => Ok(StringPropertySnapshot {
                view: borrow_string_text_properties_table_for_value(value)
                    .map(|table| table.read_view()),
                _roots: StringPropertyRootOwnership::CallbackFree,
            }),
        }
    }
}

impl StringPropertySnapshot {
    pub(crate) fn as_ref(&self) -> Option<&TextPropertyReadView> {
        self.view.as_ref()
    }

    pub(crate) fn is_none(&self) -> bool {
        self.view.is_none()
    }

    pub(crate) fn is_some(&self) -> bool {
        self.view.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neovm_core::{
        buffer::CharPos0,
        emacs_core::value::{StringTextPropertyRun, set_string_text_properties_for_value},
    };

    fn allocated() -> usize {
        unsafe { neovm_core::tagged::gc::with_tagged_heap(|heap| heap.allocated_count()) }
    }

    #[test]
    fn propertyless_source_survives_exact_gc_until_last_snapshot_drops() {
        let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
        context.gc_collect_exact();
        let baseline = allocated();
        let source = Value::string("snapshot without properties");
        let scope = StringSourceRootScope::Rooted(context.owned_root_retainer());
        let snapshot = scope.capture(source).unwrap();
        assert!(snapshot.is_none());
        let last = snapshot.clone();
        drop(snapshot);
        context.gc_collect_exact();
        assert_eq!(allocated(), baseline + 1);
        assert_eq!(source.as_utf8_str(), Some("snapshot without properties"));
        drop(last);
        context.gc_collect_exact();
        assert_eq!(allocated(), baseline);
        // Do not dereference source after its owning snapshot has retired.
    }

    #[test]
    fn detached_original_plist_operands_survive_replacement_and_exact_gc() {
        let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
        let key = Value::symbol("snapshot-root-property");
        context.gc_collect_exact();
        let baseline = allocated();
        let source = Value::string("source");
        let old_operand = Value::vector(vec![Value::string("detached original operand")]);
        set_string_text_properties_for_value(
            source,
            vec![StringTextPropertyRun {
                start: 0,
                end: 6,
                plist: Value::list(vec![key, old_operand]),
            }],
        );
        let scope = StringSourceRootScope::Rooted(context.owned_root_retainer());
        let snapshot = scope.capture(source).unwrap();
        assert!(snapshot.is_some());
        // Calibrate under an independent strong lease. If snapshot ownership
        // regresses, the count assertion below fails before any reclaimed
        // operand is dereferenced by the semantic checks.
        let safety_guard = context.retain_gc_roots(
            [source]
                .into_iter()
                .chain(snapshot.as_ref().unwrap().root_operands()),
        );
        set_string_text_properties_for_value(
            source,
            vec![StringTextPropertyRun {
                start: 0,
                end: 6,
                plist: Value::list(vec![key, Value::fixnum(2)]),
            }],
        );
        context.gc_collect_exact();
        let expected = allocated();
        drop(safety_guard);
        context.gc_collect_exact();
        assert_eq!(
            allocated(),
            expected,
            "snapshot must retain the independently calibrated detached graph"
        );
        let retained = snapshot
            .as_ref()
            .unwrap()
            .get_property_at_char_pos(CharPos0::ZERO, key)
            .unwrap();
        assert_eq!(retained.bits(), old_operand.bits());
        assert_eq!(
            retained.as_vector_data().unwrap().as_slice()[0].as_utf8_str(),
            Some("detached original operand")
        );
        assert_eq!(
            borrow_string_text_properties_table_for_value(source)
                .unwrap()
                .get_property_at_char_pos(CharPos0::ZERO, key),
            Some(Value::fixnum(2))
        );
        drop(snapshot);
        context.gc_collect_exact();
        assert_eq!(allocated(), baseline);
        // Liveness after release is checked by counts, never by reading freed
        // string, vector, or plist operands.
    }

    #[test]
    fn retired_evaluator_is_an_error_even_for_propertyless_input() {
        let context = neovm_core::emacs_core::eval_hook_installers::new_context();
        let scope = StringSourceRootScope::Rooted(context.owned_root_retainer());
        drop(context);
        assert!(matches!(
            scope.capture(Value::NIL),
            Err(RetiredStringSourceRoots)
        ));
    }
}
