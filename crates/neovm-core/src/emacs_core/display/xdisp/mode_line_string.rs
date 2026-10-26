//! GNU's STRING/NOPROP element walk. DISPLAY keeps its native row walker.
//!
//! Entry belongs to a logical `display_mode_element` dispatch, including a
//! tail transition. Emission never advances the shared ordinal authority.

use super::*;
use crate::emacs_core::Context;

/// GNU's numeric field budget; its unit is selected by the output target.
#[derive(Clone, Copy, Debug)]
pub(super) struct FieldBudget(usize);

impl FieldBudget {
    const ZERO: Self = Self(0);

    fn new(value: usize) -> Self {
        Self(value)
    }
    fn get(self) -> usize {
        self.0
    }
    fn is_empty(self) -> bool {
        self.0 == 0
    }
    fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
    fn checked_add(self, other: Self) -> Result<Self, Flow> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or_else(|| signal(LispCondition::OverflowError, vec![]))
    }
}

enum AccountedExtent {
    Characters(CharLen),
    Columns(DisplayColumn),
}

impl AccountedExtent {
    fn into_budget(self) -> FieldBudget {
        match self {
            Self::Characters(chars) => FieldBudget::new(chars.get()),
            Self::Columns(columns) => FieldBudget::new(columns.get()),
        }
    }
}

#[derive(Clone, Copy)]
enum TargetAccounting {
    Characters,
    Columns,
}

impl From<&ModeLineTarget<'_>> for TargetAccounting {
    fn from(target: &ModeLineTarget<'_>) -> Self {
        match target {
            ModeLineTarget::String(_) => Self::Characters,
            ModeLineTarget::NoProperties
            | ModeLineTarget::Title { .. }
            | ModeLineTarget::Display { .. } => Self::Columns,
        }
    }
}

impl TargetAccounting {
    fn extent(self, eval: &Context, codes: &[u32]) -> Result<AccountedExtent, Flow> {
        match self {
            Self::Characters => Ok(AccountedExtent::Characters(CharLen::new(codes.len()))),
            Self::Columns => {
                let policy = crate::encoding::CharacterWidthPolicy::from_context(eval);
                let mut columns = DisplayColumn::ZERO;
                for &code in codes {
                    columns = DisplayColumn::new(
                        columns
                            .get()
                            .checked_add(policy.width(code))
                            .ok_or_else(|| signal(LispCondition::OverflowError, vec![]))?,
                    );
                }
                Ok(AccountedExtent::Columns(columns))
            }
        }
    }

    fn stored_range(
        self,
        eval: &Context,
        codes: &[u32],
        range: CharRange,
        precision: Precision,
    ) -> CharRange {
        match self {
            Self::Characters => {
                let kept = precision.kept(FieldBudget::new(range.len().get()));
                CharRange::from_start_len(range.start(), CharLen::new(kept.get()))
            }
            Self::Columns => {
                // GNU's NOPROP/TITLE store consumes a C-string prefix even
                // when a symbol made the source literal. STRING retains NUL.
                let mut end = range.start();
                while end < range.end() && codes[end.get()] != 0 {
                    end = end.add_len(CharLen::new(1));
                }
                direct_literal_range(eval, codes, CharRange::new(range.start(), end), precision)
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Risk {
    Trusted,
    Untrusted,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Precision {
    Unlimited,
    Limited(FieldBudget),
}

impl Precision {
    fn remaining(self, used: FieldBudget) -> Self {
        match self {
            Self::Unlimited => Self::Unlimited,
            Self::Limited(limit) => Self::Limited(limit.saturating_sub(used)),
        }
    }

    fn restrict(self, limit: FieldBudget) -> Self {
        match self {
            Self::Unlimited => Self::Limited(limit),
            Self::Limited(old) => Self::Limited(FieldBudget::new(old.get().min(limit.get()))),
        }
    }

    fn availability(self) -> PrecisionAvailability {
        match self {
            Self::Unlimited => PrecisionAvailability::Available,
            Self::Limited(limit) => {
                if limit.is_empty() {
                    PrecisionAvailability::Exhausted
                } else {
                    PrecisionAvailability::Available
                }
            }
        }
    }

    fn kept(self, produced: FieldBudget) -> FieldBudget {
        match self {
            Self::Unlimited => produced,
            Self::Limited(limit) => FieldBudget::new(produced.get().min(limit.get())),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum PrecisionAvailability {
    Available,
    Exhausted,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FieldLimits {
    minimum: FieldBudget,
    precision: Precision,
}

impl FieldLimits {
    fn cap_budget(self, field_width: FieldBudget) -> FieldBudget {
        if self.minimum.is_empty() {
            field_width
        } else {
            FieldBudget::new(field_width.get().min(self.minimum.get()))
        }
    }

    fn unrestricted() -> Self {
        Self {
            minimum: FieldBudget::ZERO,
            precision: Precision::Unlimited,
        }
    }

    fn remaining(self, used: FieldBudget) -> Self {
        Self {
            minimum: self.minimum.saturating_sub(used),
            precision: self.precision.remaining(used),
        }
    }

    fn numeric(self, limit: NumericField) -> Self {
        match limit {
            NumericField::Precision(limit) => Self {
                precision: self.precision.restrict(limit),
                ..self
            },
            NumericField::Minimum(minimum) => {
                let minimum = self.precision.kept(minimum);
                Self {
                    minimum: FieldBudget::new(self.minimum.get().max(minimum.get())),
                    ..self
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum NumericField {
    Minimum(FieldBudget),
    Precision(FieldBudget),
}

impl TryFrom<i64> for NumericField {
    type Error = Flow;
    fn try_from(value: i64) -> Result<Self, Flow> {
        if value < 0 {
            let magnitude = value
                .checked_neg()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| signal(LispCondition::OverflowError, vec![]))?;
            Ok(Self::Precision(FieldBudget::new(magnitude)))
        } else {
            usize::try_from(value)
                .map(|n| Self::Minimum(FieldBudget::new(n)))
                .map_err(|_| signal(LispCondition::OverflowError, vec![]))
        }
    }
}

impl ModeLineNativeFieldLimit for FieldLimits {
    fn cap(&self, field_width: ModeLineNativeFieldWidth) -> ModeLineNativeFieldWidth {
        let field_width = CharLen::try_from(field_width).unwrap_or(CharLen::new(usize::MAX));
        let capped = CharLen::new(self.cap_budget(FieldBudget::new(field_width.get())).get());
        ModeLineNativeFieldWidth::try_from(capped).unwrap_or(ModeLineNativeFieldWidth::MAX)
    }
}

struct NativeFieldPadding {
    accounting: TargetAccounting,
    limits: FieldLimits,
}

impl ModeLineNativeFieldLimit for NativeFieldPadding {
    fn cap(&self, field_width: ModeLineNativeFieldWidth) -> ModeLineNativeFieldWidth {
        self.limits.cap(field_width)
    }

    fn store_width(&self, field_width: ModeLineNativeFieldWidth) -> ModeLineNativeFieldWidth {
        match self.accounting {
            TargetAccounting::Characters => field_width,
            // Keep the decoded width available for %- selection, while the
            // column store clips and pads the produced text exactly once.
            TargetAccounting::Columns => ModeLineNativeFieldWidth::ZERO,
        }
    }
}

#[derive(Clone, Copy)]
struct InheritedProperties(Value);

#[derive(Clone, Copy)]
pub(super) struct StringEmissionControls {
    limits: FieldLimits,
    properties: InheritedProperties,
    literal: ModeLineLiteral,
    risk: Risk,
}

impl StringEmissionControls {
    fn unrestricted(risk: Risk) -> Self {
        Self {
            limits: FieldLimits::unrestricted(),
            properties: InheritedProperties(Value::NIL),
            literal: ModeLineLiteral::Expand,
            risk,
        }
    }

    fn remaining(self, used: FieldBudget) -> Self {
        Self {
            limits: self.limits.remaining(used),
            ..self
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ElementDepth(usize);

impl From<usize> for ElementDepth {
    fn from(depth: usize) -> Self {
        Self(depth)
    }
}

impl From<bool> for Risk {
    fn from(risky: bool) -> Self {
        if risky {
            Self::Untrusted
        } else {
            Self::Trusted
        }
    }
}

impl ElementDepth {
    pub(super) fn get(self) -> usize {
        self.0
    }

    fn next(self) -> Result<Self, Flow> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or_else(|| signal(LispCondition::OverflowError, vec![]))
    }
}

/// Specpdl roots unwind on every Result path without shortening the enclosing
/// accumulator's scratch-root lifetime.
#[must_use]
struct WalkRoots<'eval> {
    eval: &'eval mut Context,
    saved: crate::emacs_core::eval::SpecpdlRootScopeState,
}

static_assertions::assert_not_impl_any!(WalkRoots<'static>: Send, Sync);

impl<'eval> WalkRoots<'eval> {
    fn enter(eval: &'eval mut Context) -> Self {
        let saved = eval.save_specpdl_roots();
        Self { eval, saved }
    }
}

impl Drop for WalkRoots<'_> {
    fn drop(&mut self) {
        self.eval.restore_specpdl_roots(self.saved);
    }
}

pub(super) fn format(
    eval: &mut Context,
    pctx: &ModeLinePercentContext<'_>,
    format: &Value,
    result: &mut ModeLineRendered,
    depth: ElementDepth,
    risk: Risk,
    selection: Option<&mut mode_line_flow_policy::FormatSelection>,
) -> Result<(), Flow> {
    format_child(
        eval,
        pctx,
        format,
        result,
        depth,
        StringEmissionControls::unrestricted(risk),
        selection,
    )
    .map(|_| ())
}

pub(super) fn format_child(
    eval: &mut Context,
    pctx: &ModeLinePercentContext<'_>,
    format: &Value,
    result: &mut ModeLineRendered,
    mut depth: ElementDepth,
    mut controls: StringEmissionControls,
    mut selection: Option<&mut mode_line_flow_policy::FormatSelection>,
) -> Result<FieldBudget, Flow> {
    let roots = WalkRoots::enter(eval);
    let eval = &mut *roots.eval;
    let mut element = *format;
    let element_root = eval.push_specpdl_root_slot(element);
    eval.push_specpdl_root(controls.properties.0);
    result.root_for_walk();
    // Account at emission, before later Lisp callbacks can change width tables.
    let mut used = FieldBudget::ZERO;

    loop {
        if depth.0 > 100 {
            element = Value::string("*too-deep*");
            eval.set_specpdl_root_slot(&element_root, element);
        }
        pctx.target.enter_element()?;
        depth = depth.next()?;
        let symbol_mode = eval.symbol_mode();

        // Canonical SYMBOLP handles positioned symbols before the total
        // decoded-kind dispatch. Unknown vectorlike inputs remain invalid.
        if element.is_symbol(symbol_mode) {
            let Some(name) = element.as_symbol_name(symbol_mode) else {
                break;
            };
            let symbol = pctx.resolve_dynamic_symbol(name);
            if !symbol.is_risky(&eval.obarray) {
                controls.risk = Risk::Untrusted;
            }
            let Some(value) = symbol.value(eval.variables(), &eval.buffers) else {
                break;
            };
            if value == element {
                break;
            }
            if value.is_string() {
                controls.literal = ModeLineLiteral::Literal;
            }
            element = value;
            eval.set_specpdl_root_slot(&element_root, element);
            continue;
        }

        match element.kind() {
            ValueKind::String => {
                used = append_mode_line_string_in_eval_with_controls(
                    eval,
                    pctx,
                    result,
                    &element,
                    controls,
                    depth,
                    selection.as_deref_mut(),
                )?;
                break;
            }
            ValueKind::Nil | ValueKind::T | ValueKind::Symbol(_) => break,
            ValueKind::Cons => {
                let car = element.cons_car();
                let cdr = element.cons_cdr();
                eval.push_specpdl_root(car);
                eval.push_specpdl_root(cdr);

                if car.is_symbol_named(":eval", symbol_mode) {
                    match controls.risk {
                        Risk::Untrusted => break,
                        Risk::Trusted => {}
                    }
                    if cdr.is_cons() {
                        let form = cdr.cons_car();
                        eval.push_specpdl_root(form);
                        if let Some(selection) = selection.as_deref_mut() {
                            selection.before_eval(eval);
                        }
                        let value = mode_line_flow_policy::eval_form(eval, &form)?;
                        eval.push_specpdl_root(value);
                        used = format_child(
                            eval,
                            pctx,
                            &value,
                            result,
                            depth,
                            controls,
                            selection.as_deref_mut(),
                        )?;
                    }
                    break;
                }
                if car.is_symbol_named(":propertize", symbol_mode) {
                    match controls.risk {
                        Risk::Untrusted => break,
                        Risk::Trusted => {}
                    }
                    if cdr.is_cons() {
                        let value = cdr.cons_car();
                        let properties = cdr.cons_cdr();
                        eval.push_specpdl_root(properties);
                        used = format_child(
                            eval,
                            pctx,
                            &value,
                            result,
                            depth,
                            StringEmissionControls {
                                properties: InheritedProperties(properties),
                                ..controls
                            },
                            selection.as_deref_mut(),
                        )?;
                    }
                    break;
                }
                if car.is_symbol(symbol_mode) {
                    if !cdr.is_cons() {
                        element = Value::string("*invalid*");
                        eval.set_specpdl_root_slot(&element_root, element);
                        continue;
                    }
                    let truth = car
                        .as_symbol_name(symbol_mode)
                        .and_then(|name| {
                            pctx.resolve_dynamic_symbol(name)
                                .value(eval.variables(), &eval.buffers)
                        })
                        .is_some_and(|value| !value.is_nil());
                    if truth {
                        element = cdr.cons_car();
                    } else {
                        let rest = cdr.cons_cdr();
                        if rest.is_nil() {
                            break;
                        }
                        if !rest.is_cons() {
                            element = Value::string("*invalid*");
                        } else {
                            element = rest.cons_car();
                        }
                    }
                    eval.set_specpdl_root_slot(&element_root, element);
                    continue;
                }
                if let Some(limit) = car.as_fixnum() {
                    controls.limits = controls.limits.numeric(NumericField::try_from(limit)?);
                    element = cdr;
                    eval.set_specpdl_root_slot(&element_root, element);
                    continue;
                }
                if car.is_string() || car.is_cons() {
                    let mut tail = element;
                    let tail_root = eval.push_specpdl_root_slot(tail);
                    let checkpoint_root = eval.push_specpdl_root_slot(tail);
                    let mut cycle = ModeLineTailCycle::new(tail);
                    while tail.is_cons() {
                        let mut child = controls.remaining(used);
                        match child.limits.precision.availability() {
                            PrecisionAvailability::Exhausted => break,
                            PrecisionAvailability::Available => {}
                        }
                        if tail.cons_cdr().is_cons() {
                            child.limits.minimum = FieldBudget::ZERO;
                        }
                        let value = tail.cons_car();
                        let child_used = format_child(
                            eval,
                            pctx,
                            &value,
                            result,
                            depth,
                            child,
                            selection.as_deref_mut(),
                        )?;
                        used = used.checked_add(child_used)?;
                        tail = tail.cons_cdr();
                        eval.set_specpdl_root_slot(&tail_root, tail);
                        if cycle.step(tail, |checkpoint| {
                            eval.set_specpdl_root_slot(&checkpoint_root, checkpoint)
                        }) {
                            break;
                        }
                    }
                    break;
                }
                element = Value::string("*invalid*");
                eval.set_specpdl_root_slot(&element_root, element);
            }
            ValueKind::Fixnum(_)
            | ValueKind::Float
            | ValueKind::Subr(_)
            | ValueKind::Veclike(_)
            | ValueKind::Unbound
            | ValueKind::Unknown => {
                element = Value::string("*invalid*");
                eval.set_specpdl_root_slot(&element_root, element);
            }
        }
    }

    let padding = controls.limits.minimum.saturating_sub(used);
    if padding.get() != 0 {
        let start = CharPos0::new(result.char_len());
        result.pad_plain_spaces(padding.get());
        result.numeric_padding.mark(start.get()..result.char_len());
        if let ModeLineTarget::String(_) = &pctx.target {
            result.apply_element_attribution(
                CharRange::from_start_len(start, CharLen::new(padding.get())),
                ElementAttribution::SyntheticNumericPadding,
            );
        }
    }
    used.checked_add(padding)
}

/// Translate inherited properties at the actual string entry, never after a
/// subtree's generated ordinal properties have already been attached.
fn prepare_string(
    eval: &mut Context,
    value: Value,
    properties: InheritedProperties,
    risk: Risk,
) -> Result<Value, Flow> {
    let Some(string) = value.as_lisp_string() else {
        return Ok(value);
    };
    if string.schars() == 0 {
        return Ok(value);
    }
    if matches!(risk, Risk::Trusted) && properties.0.is_nil() {
        return Ok(value);
    }
    let existing =
        crate::emacs_core::textprop::builtin_text_properties_at_2(eval, Value::fixnum(0), value)?;
    eval.push_specpdl_root(existing);
    let equal = crate::emacs_core::builtins::builtin_equal_2(eval, properties.0, existing)?;
    if matches!(risk, Risk::Trusted) && !equal.is_nil() {
        return Ok(value);
    }
    let mut merged = properties.0;
    if matches!(risk, Risk::Trusted) && !existing.is_nil() {
        merged = crate::emacs_core::builtins::copy_sequence_value(existing)?;
        let merged_root = eval.push_specpdl_root_slot(merged);
        let mut tail = properties.0;
        while tail.is_cons() && tail.cons_cdr().is_cons() {
            let pair = tail.cons_cdr();
            merged = crate::emacs_core::builtins::builtin_plist_put(vec![
                merged,
                tail.cons_car(),
                pair.cons_car(),
            ])?;
            eval.set_specpdl_root_slot(&merged_root, merged);
            tail = pair.cons_cdr();
        }
    }
    eval.push_specpdl_root(merged);
    let copied = crate::emacs_core::builtins::copy_sequence_value(value)?;
    eval.push_specpdl_root(copied);
    let length =
        i64::try_from(string.schars()).map_err(|_| signal(LispCondition::OverflowError, vec![]))?;
    let arguments = [Value::fixnum(0), Value::fixnum(length), merged, copied];
    let property_result = crate::emacs_core::textprop::builtin_set_text_properties_in_buffers(
        &mut eval.buffers,
        arguments.to_vec(),
    );
    if property_result.is_err() {
        let form = Value::cons(
            Value::symbol("set-text-properties"),
            Value::list(arguments.to_vec()),
        );
        eval.push_specpdl_root(form);
        // GNU's safe_set_text_properties catches signals and continues with
        // the copied source. Reuse the formatter's one safe-error definition.
        mode_line_flow_policy::handle_error(eval, &form, property_result)?;
    }
    Ok(copied)
}

fn fragment_value(source: Value, range: CharRange) -> Value {
    let mut piece = ModeLineRendered::default();
    piece.append_string_char_slice_preserving_props(
        &source,
        range.start().get(),
        range.end().get(),
    );
    piece.into_value(ModeLineFaceSpec {
        no_props: false,
        face: None,
    })
}

enum EmissionPiece<'source> {
    SourceSlice {
        source: &'source Value,
        range: CharRange,
    },
    NativeRendered(&'source ModeLineRendered),
}

/// Store one fragment and attribute it using the live shared authority. A
/// source slice retains the original rooted string without a temporary copy.
fn emit_piece(
    pctx: &ModeLinePercentContext<'_>,
    result: &mut ModeLineRendered,
    piece: EmissionPiece<'_>,
) {
    let begin = CharPos0::new(result.char_len());
    match piece {
        EmissionPiece::SourceSlice { source, range } => result
            .append_string_char_slice_preserving_props(
                source,
                range.start().get(),
                range.end().get(),
            ),
        EmissionPiece::NativeRendered(rendered) => result.append_rendered(rendered),
    }
    let range = CharRange::new(begin, CharPos0::new(result.char_len()));
    // An earlier %M may have changed the authority of this same string.
    if let ModeLineTarget::String(owner) = &pctx.target {
        result.apply_element_attribution(range, ElementAttribution::FormatElement(owner.current()));
    }
}

/// GNU's non-literal string scanner admits a prefix by display columns, but
/// its caller accounts the characters that were actually stored. Symbol-valued
/// literal strings bypass this scan and keep the native store's character cap.
fn direct_literal_range(
    eval: &Context,
    codes: &[u32],
    range: CharRange,
    precision: Precision,
) -> CharRange {
    let limit = match precision {
        Precision::Unlimited => return range,
        // This is the scanner's explicit boundary from the walk's remaining
        // character budget to c_string_width's display-column precision.
        Precision::Limited(remaining) => DisplayColumn::new(remaining.get()),
    };
    let policy = crate::encoding::CharacterWidthPolicy::from_context(eval);
    let mut column = DisplayColumn::ZERO;
    let mut end = range.start();
    while end < range.end() {
        let width = policy.width(codes[end.get()]);
        if width > limit.get().saturating_sub(column.get()) {
            break;
        }
        column = DisplayColumn::new(column.get() + width);
        end = end.add_len(CharLen::new(1));
    }
    CharRange::new(range.start(), end)
}

fn emit_native_piece(
    eval: &mut Context,
    pctx: &ModeLinePercentContext<'_>,
    result: &mut ModeLineRendered,
    value: &Value,
    mode: ModeLineLiteral,
    limits: FieldLimits,
    field_width: FieldBudget,
) -> Result<FieldBudget, Flow> {
    match limits.precision.availability() {
        PrecisionAvailability::Exhausted => return Ok(FieldBudget::ZERO),
        PrecisionAvailability::Available => {}
    }
    let mut rendered = ModeLineRendered::default();
    let accounting = TargetAccounting::from(&pctx.target);
    super::append_mode_line_string_in_eval_with_field_limit(
        eval,
        pctx,
        &mut rendered,
        value,
        mode,
        NativeFieldPadding { accounting, limits },
    );
    let kept = accounting
        .stored_range(
            eval,
            &rendered.text,
            CharRange::from_start_len(CharPos0::ZERO, CharLen::new(rendered.char_len())),
            limits.precision,
        )
        .len();
    let mut rendered = if kept.get() < rendered.char_len() {
        rendered.slice_chars(kept.get())
    } else {
        rendered
    };
    let mut used = accounting.extent(eval, &rendered.text)?.into_budget();
    match accounting {
        TargetAccounting::Characters => {}
        TargetAccounting::Columns => {
            let padding = limits.cap_budget(field_width).saturating_sub(used);
            rendered.pad_plain_spaces(padding.get());
            used = used.checked_add(padding)?;
        }
    }
    emit_piece(pctx, result, EmissionPiece::NativeRendered(&rendered));
    Ok(used)
}

/// Tokenize only the boundary needed to intercept %M. Native spec decoding,
/// encoding, source properties and width handling remain in the shared helper.
pub(super) fn append_mode_line_string_in_eval_with_controls(
    eval: &mut Context,
    pctx: &ModeLinePercentContext<'_>,
    result: &mut ModeLineRendered,
    value: &Value,
    controls: StringEmissionControls,
    depth: ElementDepth,
    mut selection: Option<&mut mode_line_flow_policy::FormatSelection>,
) -> Result<FieldBudget, Flow> {
    let prepared = prepare_string(eval, *value, controls.properties, controls.risk)?;
    eval.push_specpdl_root(prepared);
    let Some(string) = prepared.as_lisp_string() else {
        return Ok(FieldBudget::ZERO);
    };
    let mut codes = mode_line_string_char_codes(string);
    if matches!(controls.literal, ModeLineLiteral::Literal) {
        let range = TargetAccounting::from(&pctx.target).stored_range(
            eval,
            &codes,
            CharRange::from_start_len(CharPos0::ZERO, CharLen::new(codes.len())),
            controls.limits.precision,
        );
        let used = TargetAccounting::from(&pctx.target)
            .extent(eval, &codes[range.start().get()..range.end().get()])?
            .into_budget();
        emit_piece(
            pctx,
            result,
            EmissionPiece::SourceSlice {
                source: &prepared,
                range,
            },
        );
        return Ok(used);
    }
    let mut used = FieldBudget::ZERO;
    let mut cursor = CharPos0::ZERO;
    let mut inherited = controls.properties;
    let inherited_root = eval.push_specpdl_root_slot(inherited.0);
    while cursor.get() < codes.len() {
        let remaining = StringEmissionControls {
            properties: inherited,
            ..controls.remaining(used)
        };
        match remaining.limits.precision.availability() {
            PrecisionAvailability::Exhausted => break,
            PrecisionAvailability::Available => {}
        }
        if codes[cursor.get()] == 0 {
            break;
        }
        let start = cursor;
        if codes[start.get()] != u32::from('%') {
            while cursor.get() < codes.len()
                && codes[cursor.get()] != u32::from('%')
                && codes[cursor.get()] != 0
            {
                cursor = cursor.add_len(CharLen::new(1));
            }
            let range = direct_literal_range(
                eval,
                &codes,
                CharRange::new(start, cursor),
                remaining.limits.precision,
            );
            let fragment_used = TargetAccounting::from(&pctx.target)
                .extent(eval, &codes[range.start().get()..range.end().get()])?
                .into_budget();
            emit_piece(
                pctx,
                result,
                EmissionPiece::SourceSlice {
                    source: &prepared,
                    // The direct scanner already applied the column cap;
                    // applying a character cap again loses zero-width text.
                    range,
                },
            );
            used = used.checked_add(fragment_used)?;
            continue;
        }
        cursor = cursor.add_len(CharLen::new(1));
        let mut width = FieldBudget::ZERO;
        while cursor.get() < codes.len()
            && (u32::from('0')..=u32::from('9')).contains(&codes[cursor.get()])
        {
            let next = width
                .get()
                .checked_mul(10)
                .and_then(|n| n.checked_add((codes[cursor.get()] - u32::from('0')) as usize))
                .ok_or_else(|| signal(LispCondition::OverflowError, vec![]))?;
            width = FieldBudget::new(next);
            cursor = cursor.add_len(CharLen::new(1));
        }
        let spec = codes.get(cursor.get()).copied();
        if spec.is_none() || spec == Some(0) {
            break;
        }
        if spec.is_some() {
            cursor = cursor.add_len(CharLen::new(1));
        }
        if spec == Some(u32::from('M')) {
            let global = pctx
                .resolve_dynamic_symbol("global-mode-string")
                .value(eval.variables(), &eval.buffers)
                .unwrap_or(Value::NIL);
            eval.push_specpdl_root(global);
            let child_used = format_child(
                eval,
                pctx,
                &global,
                result,
                depth,
                StringEmissionControls {
                    limits: FieldLimits {
                        minimum: remaining.limits.cap_budget(width),
                        precision: remaining.limits.precision,
                    },
                    literal: ModeLineLiteral::Expand,
                    ..remaining
                },
                selection.as_deref_mut(),
            )?;
            used = used.checked_add(child_used)?;
            // %M can invoke Lisp, which may mutate even the unconsumed tail
            // of this source. Re-read the rooted string before another stop.
            let Some(string) = prepared.as_lisp_string() else {
                return Ok(used);
            };
            codes = mode_line_string_char_codes(string);
        } else {
            let fragment = fragment_value(prepared, CharRange::new(start, cursor));
            eval.push_specpdl_root(fragment);
            let fragment_used = emit_native_piece(
                eval,
                pctx,
                result,
                &fragment,
                ModeLineLiteral::Expand,
                remaining.limits,
                width,
            )?;
            used = used.checked_add(fragment_used)?;
            // GNU's STRING percent arm updates its local PROPS variable.
            // A following %M inherits this live map, not the entry snapshot.
            if let ModeLineTarget::String(owner) = &pctx.target {
                let position = i64::try_from(start.get())
                    .map_err(|_| signal(LispCondition::OverflowError, vec![]))?;
                let properties = crate::emacs_core::textprop::builtin_text_properties_at_2(
                    eval,
                    Value::fixnum(position),
                    prepared,
                )?;
                eval.push_specpdl_root(properties);
                inherited = InheritedProperties(
                    ElementAttribution::FormatElement(owner.current())
                        .apply_to_inherited(properties)?,
                );
                eval.set_specpdl_root_slot(&inherited_root, inherited.0);
            }
        }
    }
    Ok(used)
}
