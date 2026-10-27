//! Stored STRING/NOPROP/TITLE output and field accounting for the shared walk.
//!
//! Entry belongs to a logical `display_mode_element` dispatch, including a
//! tail transition. Emission never advances the shared ordinal authority.

use super::*;
#[path = "mode_line_walk_stored.rs"]
mod stored_walk_policy;
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

impl From<&StoredTarget<'_>> for TargetAccounting {
    fn from(target: &StoredTarget<'_>) -> Self {
        match target {
            StoredTarget::String(_) => Self::Characters,
            StoredTarget::NoProperties | StoredTarget::Title { .. } => Self::Columns,
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

pub(super) fn format_child(
    eval: &mut Context,
    pctx: &ModeLinePercentContext<StoredTarget<'_>>,
    format: &Value,
    result: &mut ModeLineRendered,
    depth: ElementDepth,
    controls: StringEmissionControls,
    selection: Option<&mut mode_line_flow_policy::FormatSelection>,
) -> Result<FieldBudget, Flow> {
    super::mode_line_walk::walk(eval, pctx, format, result, depth, controls, selection)
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
    pctx: &ModeLinePercentContext<StoredTarget<'_>>,
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
    if let StoredTarget::String(owner) = &pctx.target {
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

fn finish_native_piece(
    eval: &mut Context,
    pctx: &ModeLinePercentContext<StoredTarget<'_>>,
    result: &mut ModeLineRendered,
    rendered: ModeLineRendered,
    limits: FieldLimits,
    field_width: FieldBudget,
) -> Result<FieldBudget, Flow> {
    let accounting = TargetAccounting::from(&pctx.target);
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

/// Stored driver for the common scanner. Its source and inherited map have
/// specpdl roots in the enclosing walk; no source/string borrow crosses %M.
struct StoredAppendDriver<'eval, 'context, 'scope, 'selection> {
    eval: &'eval mut Context,
    pctx: &'context ModeLinePercentContext<StoredTarget<'scope>>,
    result: &'eval mut ModeLineRendered,
    prepared: Value,
    codes: Vec<u32>,
    controls: StringEmissionControls,
    depth: ElementDepth,
    selection: Option<&'selection mut mode_line_flow_policy::FormatSelection>,
    used: FieldBudget,
    inherited: InheritedProperties,
    inherited_root: crate::emacs_core::eval::SpecpdlRootSlot,
}

impl PercentWidth for FieldBudget {
    type Error = Flow;
    const ZERO: Self = Self::ZERO;
    fn decimal_digit(self, digit: DecimalDigit) -> Result<Self, Self::Error> {
        self.get()
            .checked_mul(10)
            .and_then(|width| width.checked_add(usize::from(digit.value())))
            .map(Self::new)
            .ok_or_else(|| signal(LispCondition::OverflowError, vec![]))
    }
}

impl PercentAppendDriver for StoredAppendDriver<'_, '_, '_, '_> {
    type Error = Flow;
    type Width = FieldBudget;
    const ZERO: ZeroTerminator = ZeroTerminator::Stop;
    const TRAILING: TrailingPercent = TrailingPercent::Stop;

    fn code_at(&self, position: CharPos0) -> Option<EmacsChar> {
        self.codes
            .get(position.get())
            .copied()
            .and_then(EmacsChar::from_code)
    }
    fn admit(&mut self) -> Result<AppendAdmission, Flow> {
        Ok(
            match self
                .controls
                .remaining(self.used)
                .limits
                .precision
                .availability()
            {
                PrecisionAvailability::Exhausted => AppendAdmission::Stop,
                PrecisionAvailability::Available => AppendAdmission::Continue,
            },
        )
    }
    fn literal(&mut self, range: CharRange) -> Result<(), Flow> {
        let remaining = self.controls.remaining(self.used);
        let range = direct_literal_range(self.eval, &self.codes, range, remaining.limits.precision);
        let used = TargetAccounting::from(&self.pctx.target)
            .extent(
                self.eval,
                &self.codes[range.start().get()..range.end().get()],
            )?
            .into_budget();
        emit_piece(
            self.pctx,
            self.result,
            EmissionPiece::SourceSlice {
                source: &self.prepared,
                range,
            },
        );
        self.used = self.used.checked_add(used)?;
        Ok(())
    }
    fn field(
        &mut self,
        range: CharRange,
        spec: Option<EmacsChar>,
        width: FieldBudget,
    ) -> Result<(), Flow> {
        let remaining = StringEmissionControls {
            properties: self.inherited,
            ..self.controls.remaining(self.used)
        };
        // ASCII codes have identical native/Emacs decoding. For a non-ASCII
        // unknown spec preserve the old fragment's Emacs-to-lossy conversion
        // (including raw-unibyte/byte8/22-bit characters), without rescanning
        // its percent stop or decimal width.
        let native_spec = match spec {
            Some(code) if code.code() < 128 => code.as_rust_char(),
            Some(_) => {
                let fragment = fragment_value(self.prepared, range);
                self.eval.push_specpdl_root(fragment);
                fragment.as_lisp_string().and_then(|string| {
                    crate::emacs_core::emacs_char::to_utf8_lossy(string.as_bytes())
                        .chars()
                        .last()
                })
            }
            None => None,
        };
        let mut rendered = ModeLineRendered::default();
        let action = {
            let command_loop_depth =
                ModeLineCommandDepth::from(self.eval.recursive_command_loop_depth());
            let vars = VariableView::new(&self.eval.obarray, &self.eval.owner);
            let mut access = mode_line_point::Access::Live(&mut self.eval.frames);
            // Old stored native fragments created fresh name/column memos.
            // Retain that ownership; a %M callback cannot leave stale fields.
            let mut cache = NativeSpecCache::default();
            let native_width = ModeLineNativeFieldWidth::try_from(CharLen::new(width.get()))
                .unwrap_or(ModeLineNativeFieldWidth::MAX);
            decode_one_native_spec::<StoredTarget<'_>, RecursiveGlobal>(
                NativeSpecInputs {
                    vars,
                    coding: CodingAccess::Registry(&self.eval.coding_systems),
                    dynamic: &[],
                    buffers: &self.eval.buffers,
                    processes: &self.eval.processes,
                    command_loop_depth,
                    pctx: self.pctx,
                    point_access: &mut access,
                    cache: &mut cache,
                },
                NativePercentField {
                    value: &self.prepared,
                    position: range.start(),
                    spec: native_spec,
                    width: native_width,
                },
                &mut rendered,
                &NativeFieldPadding {
                    accounting: TargetAccounting::from(&self.pctx.target),
                    limits: remaining.limits,
                },
            )
        };
        match action {
            NativeSpecAction::RecurseGlobal(()) => {
                let global = self
                    .pctx
                    .resolve_dynamic_symbol("global-mode-string")
                    .value(self.eval.variables(), &self.eval.buffers)
                    .unwrap_or(Value::NIL);
                self.eval.push_specpdl_root(global);
                let child_used = format_child(
                    self.eval,
                    self.pctx,
                    &global,
                    self.result,
                    self.depth,
                    StringEmissionControls {
                        limits: FieldLimits {
                            minimum: remaining.limits.cap_budget(width),
                            precision: remaining.limits.precision,
                        },
                        literal: ModeLineLiteral::Expand,
                        ..remaining
                    },
                    self.selection.as_deref_mut(),
                )?;
                self.used = self.used.checked_add(child_used)?;
                // Re-read only after callback/rooted child emission completes.
                self.codes = match self.prepared.as_lisp_string() {
                    Some(string) => mode_line_string_char_codes(string),
                    None => Vec::new(),
                };
            }
            NativeSpecAction::Emitted => {
                let fragment_used = finish_native_piece(
                    self.eval,
                    self.pctx,
                    self.result,
                    rendered,
                    remaining.limits,
                    width,
                )?;
                self.used = self.used.checked_add(fragment_used)?;
                if let StoredTarget::String(owner) = &self.pctx.target {
                    let position = i64::try_from(range.start().get())
                        .map_err(|_| signal(LispCondition::OverflowError, vec![]))?;
                    let properties = crate::emacs_core::textprop::builtin_text_properties_at_2(
                        self.eval,
                        Value::fixnum(position),
                        self.prepared,
                    )?;
                    self.eval.push_specpdl_root(properties);
                    self.inherited = InheritedProperties(
                        ElementAttribution::FormatElement(owner.current())
                            .apply_to_inherited(properties)?,
                    );
                    self.eval
                        .set_specpdl_root_slot(&self.inherited_root, self.inherited.0);
                }
            }
        }
        Ok(())
    }
}

pub(super) fn append_mode_line_string_in_eval_with_controls(
    eval: &mut Context,
    pctx: &ModeLinePercentContext<StoredTarget<'_>>,
    result: &mut ModeLineRendered,
    value: &Value,
    controls: StringEmissionControls,
    depth: ElementDepth,
    selection: Option<&mut mode_line_flow_policy::FormatSelection>,
) -> Result<FieldBudget, Flow> {
    let prepared = prepare_string(eval, *value, controls.properties, controls.risk)?;
    eval.push_specpdl_root(prepared);
    let Some(string) = prepared.as_lisp_string() else {
        return Ok(FieldBudget::ZERO);
    };
    let codes = mode_line_string_char_codes(string);
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
    let inherited_root = eval.push_specpdl_root_slot(controls.properties.0);
    let mut driver = StoredAppendDriver {
        eval,
        pctx,
        result,
        prepared,
        codes,
        controls,
        depth,
        selection,
        used: FieldBudget::ZERO,
        inherited: controls.properties,
        inherited_root,
    };
    mode_line_append::append_percent_stops(&mut driver)?;
    Ok(driver.used)
}
