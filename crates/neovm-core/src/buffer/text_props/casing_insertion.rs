//! GNU interval-offset inheritance for casing growth, without Lisp callbacks.
use super::{CharLen, CharPos0, CharRange, TextPropertyTable, Value};
use crate::emacs_core::error::{Flow, LispCondition, signal};
use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::textprop::{
    DirectCharProperties, StickinessProperty, TextPropertyControlVariable,
    resolve_effective_char_property,
};

/// Scratch roots belong to this Lisp mutator and must be restored on its
/// thread, after the prepared table is published or abandoned.
#[derive(Debug)]
#[must_use = "retain casing roots until the prepared table is published or abandoned"]
pub(crate) struct CasingPropertyRoots {
    saved: usize,
    mutator: std::marker::PhantomData<*const ()>,
}

impl CasingPropertyRoots {
    pub(crate) fn new() -> Self {
        Self {
            saved: crate::emacs_core::eval::save_scratch_gc_roots(),
            mutator: std::marker::PhantomData,
        }
    }

    pub(crate) fn root(&self, value: Value) {
        crate::emacs_core::eval::push_scratch_gc_root(value);
    }
}

impl Drop for CasingPropertyRoots {
    fn drop(&mut self) {
        crate::emacs_core::eval::restore_scratch_gc_roots(self.saved);
    }
}

/// GNU EQ follows symbols-with-pos-enabled (lisp.h:1316-1324).
#[derive(Clone, Copy, Debug)]
enum CasingPropertyEquality {
    Exact,
    PositionedSymbolsTransparent,
}

impl CasingPropertyEquality {
    fn same(self, left: Value, right: Value) -> bool {
        crate::emacs_core::value::eq_value_swp(
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
pub(crate) struct CasingPropertyControls {
    equality: CasingPropertyEquality,
    nonsticky: Value,
    aliases: Value,
    defaults: Value,
    mutator: std::marker::PhantomData<*const ()>,
}

impl CasingPropertyControls {
    pub(crate) fn for_context(ctx: &crate::emacs_core::eval::Context) -> Self {
        let read = |variable: TextPropertyControlVariable| {
            ctx.eval_symbol_by_id(variable.symbol_id()).ok()
        };
        Self {
            equality: if ctx.symbols_with_pos_enabled {
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
pub(crate) enum CasingPropertyMode<'a> {
    Unneeded,
    Inherit(CasingPropertyContext<'a>),
}

impl<'a> CasingPropertyMode<'a> {
    pub(crate) fn from_controls(
        obarray: &'a Obarray,
        controls: Option<CasingPropertyControls>,
    ) -> Self {
        match controls {
            None => Self::Unneeded,
            Some(controls) => Self::Inherit(CasingPropertyContext { obarray, controls }),
        }
    }
}

/// Immutable, call-local category and dynamic-property resolution.
pub(crate) struct CasingPropertyContext<'a> {
    obarray: &'a Obarray,
    controls: CasingPropertyControls,
}

impl std::fmt::Debug for CasingPropertyContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CasingPropertyContext")
            .field("controls", &self.controls)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InsertionSite {
    Interior(super::IntervalId),
    Boundary,
}

#[derive(Debug)]
enum InsertionInheritance {
    InteriorStretch(super::IntervalId),
    Boundary(Vec<(Value, Value)>),
}

#[derive(Clone, Copy, Debug)]
enum BoundaryInheritanceTarget {
    Predecessor,
    Successor,
    Inserted,
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
    let pair = crate::emacs_core::builtins::builtin_assq_values(
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
    crate::emacs_core::builtins::builtin_memq_values(
        name,
        set,
        matches!(
            equality,
            CasingPropertyEquality::PositionedSymbolsTransparent
        ),
    )
    .map(|tail| !tail.is_nil())
}

#[cold]
#[inline(never)]
fn invalid_interval_plan() -> Flow {
    signal(
        LispCondition::Error,
        vec![Value::string("Invalid casing interval plan")],
    )
}

impl CasingPropertyContext<'_> {
    fn property(&self, plist: &[(Value, Value)], name: Value) -> Option<Value> {
        property(plist, name, self.controls.equality)
    }

    fn same_properties(&self, left: &[(Value, Value)], right: &[(Value, Value)]) -> bool {
        left.len() == right.len()
            && left.iter().all(|&(name, value)| {
                self.property(right, name)
                    .is_some_and(|other| self.controls.equality.same(value, other))
            })
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
                    self.controls.equality.bare(category).as_symbol_id()?,
                    prop.as_symbol_id()?,
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
        let mut safe_tail = crate::emacs_core::plist::SafeTailGuard::new(cursor);
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
        Ok(crate::emacs_core::plist::plist_get_swp(
            self.controls.defaults,
            &name,
            matches!(
                self.controls.equality,
                CasingPropertyEquality::PositionedSymbolsTransparent
            ),
        )
        .unwrap_or(Value::NIL))
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
        if let InsertionSite::Interior(interval) = site {
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
                return Ok(InsertionInheritance::InteriorStretch(interval));
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
                .as_symbol_id()
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

impl TextPropertyTable {
    /// No syntax reader runs between the detached casing interval offsets
    /// (GNU intervals.c:1358-1365) and publication. Invalidate these positional
    /// caches once instead of shifting every cached range for every expansion.
    pub(in crate::buffer) fn invalidate_casify_syntax_caches(&mut self) {
        let ranges = self
            .syntax_prop_ranges
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *ranges = (0, Vec::new());
        self.syntax_prop_any
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// Casing calls offset_intervals at the expanded source character, with
    /// positive net growth (insdel.c:1800; intervals.c:1358-1365). Coordinates
    /// refer to physical text, independent of the buffer's narrowed bounds.
    pub(crate) fn adjust_for_casify_insertion(
        &mut self,
        pos: CharPos0,
        len: CharLen,
        old_object_len: CharLen,
        context: &CasingPropertyContext<'_>,
        roots: &CasingPropertyRoots,
    ) -> Result<(), Flow> {
        if len.is_empty() || self.is_empty() {
            return Ok(());
        }
        if pos > CharPos0::ZERO.add_len(old_object_len) {
            return Err(invalid_interval_plan());
        }
        self.intervals
            .ensure_cover(CharPos0::ZERO.add_len(old_object_len));
        let located = self.intervals.find_id(pos);
        let site = located.map_or(InsertionSite::Boundary, |(start, interval)| {
            if pos > start {
                InsertionSite::Interior(interval)
            } else {
                InsertionSite::Boundary
            }
        });
        let left = if pos == CharPos0::ZERO {
            Vec::new()
        } else {
            self.plist_at(pos.saturating_sub_len(CharLen::new(1)))
                .unwrap_or_default()
        };
        let right = self.plist_at(pos).unwrap_or_default();
        let inherited = match context.inherited(&left, &right, site, roots)? {
            InsertionInheritance::InteriorStretch(interval) => {
                // GNU intervals.c:971-978 stretches the original interval;
                // neither its plist identity nor its partition changes.
                self.mutation_tick += 1;
                if let Ok(mut guard) = self.syntax_prop_ranges.lock()
                    && guard.0 == self.syntax_prop_tick + 1
                {
                    for (start, end) in guard.1.iter_mut() {
                        if *start >= pos {
                            *start = start.add_len(len);
                            *end = end.add_len(len);
                        } else if *end > pos {
                            *end = end.add_len(len);
                        }
                    }
                }
                self.intervals.add_length_to_ancestors(
                    Some(interval),
                    isize::try_from(len.get()).map_err(|_| invalid_interval_plan())?,
                );
                return Ok(());
            }
            InsertionInheritance::Boundary(properties) => properties,
        };
        let target = if pos > CharPos0::ZERO && context.same_properties(&left, &inherited) {
            BoundaryInheritanceTarget::Predecessor
        } else if located.is_some() && context.same_properties(&right, &inherited) {
            BoundaryInheritanceTarget::Successor
        } else {
            BoundaryInheritanceTarget::Inserted
        };
        // A forced boundary must split even a property-free interval. The
        // ordinary insert helper may otherwise stretch such an interval.
        if let Some(interval) = self.intervals.split_at(pos) {
            roots.root(self.intervals.nodes[interval.0].plist);
        }
        self.adjust_for_insert_raw(pos, len);
        if !inherited.is_empty() {
            self.set_properties_for_object_char_len(
                CharRange::from_start_len(pos, len),
                old_object_len.add_len(len),
                inherited,
            );
        }
        let (_, inserted) = self
            .intervals
            .find_id(pos)
            .ok_or_else(invalid_interval_plan)?;
        roots.root(self.intervals.nodes[inserted.0].plist);
        match target {
            BoundaryInheritanceTarget::Predecessor => {
                // GNU extends the predecessor when its properties match. Do
                // not remove an already-existing equal boundary on the right.
                self.intervals
                    .merge_interval_left(inserted)
                    .ok_or_else(invalid_interval_plan)?;
            }
            BoundaryInheritanceTarget::Successor => {
                // GNU intervals.c:949-958,1380-1417 absorbs into the successor,
                // retaining its plist, including its cons identity.
                let (_, successor) = self
                    .intervals
                    .find_id(pos.add_len(len))
                    .ok_or_else(invalid_interval_plan)?;
                let plist = self.intervals.nodes[successor.0].plist;
                self.intervals.set_node_plist(inserted, plist);
                self.intervals.merge_into_predecessor(inserted, successor);
            }
            BoundaryInheritanceTarget::Inserted => {}
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/casing_insertion.rs"]
mod tests;
