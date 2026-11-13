//! GNU interval-offset inheritance for casing growth, without Lisp callbacks.
#[cfg(test)]
use super::TextPropertyTable;
use super::Value;
pub(crate) use super::{
    CasingInsertionPolicy, CasingPropertyRoots, InsertionInheritance, InsertionSite,
};
#[cfg(test)]
use crate::buffer::position::{CharLen, CharPos0, CharRange};
use crate::emacs_core::textprop::{
    DirectCharProperties, StickinessProperty, TextPropertyControlVariable,
    resolve_effective_char_property,
};
use neovm_value::error::{Flow, LispCondition, signal};
use neovm_value::symbol::Obarray;

#[cfg_attr(feature = "test-support", visibility::make(pub))]
/// GNU EQ follows symbols-with-pos-enabled (lisp.h:1316-1324).
#[derive(Clone, Copy, Debug)]
enum CasingPropertyEquality {
    Exact,
    PositionedSymbolsTransparent,
}

impl CasingPropertyEquality {
    fn same(self, left: Value, right: Value) -> bool {
        neovm_value::value::eq_value_swp(
            &left,
            &right,
            matches!(self, Self::PositionedSymbolsTransparent),
        )
    }

    fn bare(self, value: Value) -> Value {
        match self {
            Self::Exact => value,
            Self::PositionedSymbolsTransparent => value.as_symbol_with_pos_sym().unwrap_or(value),
        }
    }
}

/// Resolved after modification hooks, before the casing storage transaction.
/// Heap values remain rooted in the calling context; this snapshot cannot move
/// to another mutator. There are no evaluator callbacks during interval edits.
#[derive(Clone, Copy, Debug)]
#[doc(hidden)]
pub struct CasingPropertyControls {
    pub(crate) equality: CasingPropertyEquality,
    pub(crate) nonsticky: Value,
    pub(crate) aliases: Value,
    pub(crate) defaults: Value,
    pub(crate) mutator: std::marker::PhantomData<*const ()>,
}

impl CasingPropertyControls {
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied nonsticky alist (VIS read arm, lead #1052).  The fields are
    /// `pub(crate)` in every arm; these getters are the only cross-crate
    /// read routes.
    pub(crate) fn nonsticky_for_test(&self) -> Value {
        self.nonsticky
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied char-property alias alist (VIS read arm, lead #1052).
    pub(crate) fn aliases_for_test(&self) -> Value {
        self.aliases
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied default text properties (VIS read arm, lead #1052).
    pub(crate) fn defaults_for_test(&self) -> Value {
        self.defaults
    }

    #[doc(hidden)]
    pub fn for_host<H: crate::emacs_core::edit_host::EditHost>(host: &H) -> Self {
        // GNU reads these three through the swapped C globals (the current
        // buffer's binding plus a live `let`), never through the default:
        // font-lock and enriched-mode setq-local two of them. The typed
        // closure keeps the closed variable set, and the read is
        // GNU `find_symbol_value` (`forwarded_variable_value`), not the
        // buffer-blind `default_value_only`.
        let read = |variable: TextPropertyControlVariable| {
            host.forwarded_variable_value(variable.symbol_id())
        };
        Self {
            equality: if matches!(
                host.symbol_mode(),
                neovm_value::tagged::SymbolMode::Positioned
            ) {
                CasingPropertyEquality::PositionedSymbolsTransparent
            } else {
                CasingPropertyEquality::Exact
            },
            nonsticky: read(TextPropertyControlVariable::TextPropertyDefaultNonsticky)
                .unwrap_or_else(crate::emacs_core::textprop::default_text_property_nonsticky_alist),
            aliases: read(TextPropertyControlVariable::CharPropertyAliasAlist)
                .unwrap_or(Value::NIL),
            defaults: read(TextPropertyControlVariable::DefaultTextProperties)
                .unwrap_or(Value::NIL),
            mutator: std::marker::PhantomData,
        }
    }
}

/// No interval policy is needed when there is no growth or no interval tree.
#[derive(Debug)]
#[doc(hidden)]
pub enum CasingPropertyMode<'a> {
    Unneeded,
    Inherit(CasingPropertyContext<'a>),
}

impl<'a> CasingPropertyMode<'a> {
    #[doc(hidden)]
    pub fn from_controls(obarray: &'a Obarray, controls: Option<CasingPropertyControls>) -> Self {
        match controls {
            None => Self::Unneeded,
            Some(controls) => Self::Inherit(CasingPropertyContext { obarray, controls }),
        }
    }
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
/// Immutable, call-local category and dynamic-property resolution.
pub(crate) struct CasingPropertyContext<'a> {
    pub(crate) obarray: &'a Obarray,
    pub(crate) controls: CasingPropertyControls,
}

impl std::fmt::Debug for CasingPropertyContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CasingPropertyContext")
            .field("controls", &self.controls)
            .finish_non_exhaustive()
    }
}

fn property(
    plist: &[(Value, Value)],
    name: Value,
    equality: CasingPropertyEquality,
) -> Option<Value> {
    plist
        .iter()
        .find_map(|&(key, value)| equality.same(key, name).then_some(value))
}

fn assq(list: Value, name: Value, equality: CasingPropertyEquality) -> Result<Option<Value>, Flow> {
    let pair = neovm_value::list_scan::builtin_assq_values(
        name,
        list,
        matches!(
            equality,
            CasingPropertyEquality::PositionedSymbolsTransparent
        ),
    )?;
    Ok(pair.is_cons().then(|| pair.cons_cdr()))
}

fn member(name: Value, set: Value, equality: CasingPropertyEquality) -> Result<bool, Flow> {
    if !set.is_cons() {
        return Ok(!set.is_nil());
    }
    neovm_value::list_scan::builtin_memq_values(
        name,
        set,
        matches!(
            equality,
            CasingPropertyEquality::PositionedSymbolsTransparent
        ),
    )
    .map(|tail| !tail.is_nil())
}

impl CasingPropertyContext<'_> {
    fn property(&self, plist: &[(Value, Value)], name: Value) -> Option<Value> {
        property(plist, name, self.controls.equality)
    }

    fn assq(&self, list: Value, name: Value) -> Result<Option<Value>, Flow> {
        assq(list, name, self.controls.equality)
    }

    fn member(&self, name: Value, set: Value) -> Result<bool, Flow> {
        member(name, set, self.controls.equality)
    }

    fn effective(&self, plist: &[(Value, Value)], name: Value) -> Result<Value, Flow> {
        let direct = DirectCharProperties::from_getter(|key| self.property(plist, key), name);
        let direct_or_category = resolve_effective_char_property(
            direct,
            |category, prop| {
                self.obarray.get_property_id(
                    self.controls.equality.bare(category).as_bare_symbol_id()?,
                    prop.as_bare_symbol_id()?,
                )
            },
            name,
            std::iter::empty(),
            |_| None,
            None,
        );
        if let Some(value) = direct_or_category {
            return Ok(value);
        }
        let aliases = self
            .assq(self.controls.aliases, name)?
            .unwrap_or(Value::NIL);
        let mut cursor = aliases;
        let mut safe_tail = neovm_value::plist::SafeTailGuard::new(cursor);
        while cursor.is_cons() {
            if let Some(value) = self.property(plist, cursor.cons_car())
                && !value.is_nil()
            {
                return Ok(value);
            }
            cursor = cursor.cons_cdr();
            if safe_tail.found_cycle_after_advance(cursor) {
                return Err(signal(LispCondition::CircularList, vec![aliases]));
            }
        }
        // GNU plist_get uses FOR_EACH_TAIL_SAFE (fns.c:2634-2646), so the
        // existing pure getter also handles malformed/circular defaults.
        Ok(neovm_value::plist::plist_get_swp(
            self.controls.defaults,
            &name,
            matches!(
                self.controls.equality,
                CasingPropertyEquality::PositionedSymbolsTransparent
            ),
        )
        .unwrap_or(Value::NIL))
    }
}

impl CasingInsertionPolicy for CasingPropertyContext<'_> {
    fn same_properties(&self, left: &[(Value, Value)], right: &[(Value, Value)]) -> bool {
        left.len() == right.len()
            && left.iter().all(|&(name, value)| {
                self.property(right, name)
                    .is_some_and(|other| self.controls.equality.same(value, other))
            })
    }

    fn inherited(
        &self,
        left: &[(Value, Value)],
        right: &[(Value, Value)],
        site: InsertionSite,
        roots: &CasingPropertyRoots,
    ) -> Result<InsertionInheritance, Flow> {
        let front_key = StickinessProperty::FrontSticky.value();
        let rear_key = StickinessProperty::RearNonsticky.value();
        let left_front = self.effective(left, front_key)?;
        let left_rear = self.effective(left, rear_key)?;
        let right_front = self.effective(right, front_key)?;
        let right_rear = self.effective(right, rear_key)?;
        // GNU intervals.c:837-897: a uniform interval stretches unless a
        // real property forces splitting. Explicit all-front-sticky wins over
        // defaults, while explicit all-rear-nonsticky forces a split first.
        if let InsertionSite::Interior = site {
            let split = if !left_rear.is_cons() && !left_rear.is_nil() {
                true
            } else if !left_front.is_cons() && !left_front.is_nil() {
                false
            } else {
                let mut split = false;
                for &(name, _) in left {
                    if !self.member(name, left_front)?
                        && (self.member(name, left_rear)?
                            || self.assq(self.controls.nonsticky, name)?.is_some())
                    {
                        split = true;
                        break;
                    }
                }
                split
            };
            if !split {
                return Ok(InsertionInheritance::InteriorStretch);
            }
        }
        // GNU intervals.c:1024-1150: merge each sticky property, preferring
        // the left except when a nil value loses to a non-nil right value.
        let mut merged = Vec::new();
        let mut fronts = Vec::new();
        let mut rears = Vec::new();
        for &(name, right_value) in right {
            if self.controls.equality.same(name, front_key)
                || self.controls.equality.same(name, rear_key)
            {
                continue;
            }
            let default = self.assq(self.controls.nonsticky, name)?;
            let left_value = self.property(left, name);
            let mut use_left = left_value.is_some()
                && !(self.member(name, left_rear)? || default.is_some_and(|v| !v.is_nil()));
            let mut use_right =
                self.member(name, right_front)? || default.is_some_and(|v| v.is_nil());
            if use_left && use_right {
                if left_value.is_some_and(|v| v.is_nil()) {
                    use_left = false;
                } else if right_value.is_nil() {
                    use_right = false;
                }
            }
            if let Some(value) = left_value.filter(|_| use_left) {
                merged.push((name, value));
                if self.member(name, left_front)? {
                    fronts.push(name);
                }
                if self.member(name, left_rear)? {
                    rears.push(name);
                }
            } else if use_right {
                merged.push((name, right_value));
                if self.member(name, right_front)? {
                    fronts.push(name);
                }
                if self.member(name, right_rear)? {
                    rears.push(name);
                }
            }
        }
        for &(name, value) in left {
            if self.controls.equality.same(name, front_key)
                || self.controls.equality.same(name, rear_key)
                || self.property(right, name).is_some()
            {
                continue;
            }
            let default = self.assq(self.controls.nonsticky, name)?;
            if !(self.member(name, left_rear)? || default.is_some_and(|v| !v.is_nil())) {
                merged.push((name, value));
                if self.member(name, left_front)? {
                    fronts.push(name);
                }
            } else if self.member(name, right_front)? || default.is_some_and(|v| v.is_nil()) {
                fronts.push(name);
                if self.member(name, right_rear)? {
                    rears.push(name);
                }
            }
        }
        if !rears.is_empty() {
            let value = Value::list(rears);
            roots.root(value);
            merged.insert(0, (rear_key, value));
        }
        let category = self.effective(&merged, Value::symbol("category"))?;
        let category_all_front = !category.is_nil()
            && self
                .controls
                .equality
                .bare(category)
                .as_bare_symbol_id()
                .and_then(|category| {
                    self.obarray
                        .get_property_id(category, StickinessProperty::FrontSticky.symbol_id())
                })
                .is_some_and(|value| self.controls.equality.same(value, Value::T));
        if !fronts.is_empty() && !category_all_front {
            let value = Value::list(fronts);
            roots.root(value);
            merged.insert(0, (front_key, value));
        }
        Ok(InsertionInheritance::Boundary(merged))
    }
}

#[cfg(test)]
#[path = "tests/casing_insertion.rs"]
mod tests;
