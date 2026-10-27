//! One semantic walker, specialized by the private sealed output contract.
//! Target policies own field accounting and leaf output, never interpretation.
use super::mode_line_string::{ElementDepth, Risk};
use super::mode_line_target::{DisplayTarget, TargetPolicy};
use super::*;
use crate::emacs_core::Context;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Transfer {
    Child,
    Tail,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Grammar {
    Native,
    Stored,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SymbolRiskTiming {
    BeforeValue,
    BeforeChild,
}
#[derive(Clone, Copy)]
pub(super) enum TextOrigin {
    CurrentControls,
    ForcedLiteral,
}
#[derive(Clone, Copy)]
enum Node {
    Empty,
    Symbol,
    Text,
    Cons,
    Invalid,
}
#[derive(Clone, Copy)]
enum ConsForm {
    Eval,
    Propertize,
    Conditional,
    Numeric(NumericFieldValue),
    Sequence,
    Invalid,
}
#[derive(Clone, Copy)]
enum Branch {
    Value(Value),
    Absent,
    Malformed,
}
#[derive(Clone, Copy)]
enum BranchTruth {
    NonNil,
    Nil,
}
impl From<bool> for BranchTruth {
    fn from(truth: bool) -> Self {
        if truth { Self::NonNil } else { Self::Nil }
    }
}

/// The signed numeric field domain admitted by canonical Lisp FIXNUMP.
/// No constructor accepts an arbitrary Rust integer. Interpretation remains at
/// the target-specific before-tail/after-child boundary, preserving callbacks.
#[derive(Clone, Copy)]
pub(super) struct NumericFieldValue(i64);
impl NumericFieldValue {
    fn from_value(value: Value) -> Option<Self> {
        value.as_fixnum().map(Self)
    }
    pub(super) fn native_after_child(self) -> i64 {
        self.0
    }
    pub(super) fn stored_before_tail(self) -> i64 {
        self.0
    }
}

pub(super) enum NumericTransfer<P: WalkPolicy> {
    Tail(P::Controls),
    Child(P::Envelope),
}
pub(super) enum FrameAdmission {
    Admit,
    Stop,
}
pub(super) enum SequenceAction<C> {
    Render(C),
    Stop,
}

/// A leaf subtree-output transaction, NEVER an alternative walker.
/// `output` selects native scratch or stored direct output; shared `walk` owns
/// the only recursive call, then `finish` performs only the target's wrapping.
pub(super) trait ChildEnvelope<P: WalkPolicy> {
    fn controls(&self) -> P::Controls;
    fn output<'out>(
        &'out mut self,
        parent: &'out mut ModeLineRendered,
    ) -> &'out mut ModeLineRendered;
    fn finish(
        self,
        eval: &mut Context,
        pctx: &ModeLinePercentContext<P>,
        parent: &mut ModeLineRendered,
        used: P::Extent,
    ) -> Result<P::Extent, Flow>;
}

/// Private: implementations are only for the sealed TargetPolicy types.
/// DISPLAY has Controls=Risk, Extent=(), InputRoot=(): no stored properties,
/// budgets, ordinal state, ordinal capability argument or runtime target tag.
pub(super) trait WalkPolicy: TargetPolicy + Sized {
    type Controls: Copy;
    fn initial_controls(risk: Risk) -> Self::Controls;
    #[allow(clippy::too_many_arguments)]
    fn append_text(
        eval: &mut Context,
        pctx: &ModeLinePercentContext<Self>,
        result: &mut ModeLineRendered,
        value: &Value,
        controls: Self::Controls,
        origin: TextOrigin,
        depth: ElementDepth,
        selection: Option<&mut mode_line_flow_policy::FormatSelection>,
    ) -> Result<Self::Extent, Flow>;
    type Extent: Copy;
    type InputRoot;
    type Envelope: ChildEnvelope<Self>;
    const GRAMMAR: Grammar;
    const SYMBOL_TRANSFER: Transfer;
    const CONDITIONAL_TRANSFER: Transfer;
    const SYMBOL_RISK_TIMING: SymbolRiskTiming;

    fn admit_frame(depth: ElementDepth) -> FrameAdmission;
    fn root_input(eval: &mut Context, value: Value, controls: Self::Controls) -> Self::InputRoot;
    fn replace_input(eval: &mut Context, root: &Self::InputRoot, value: Value);
    fn begin_dispatch(
        self,
        eval: &mut Context,
        root: &Self::InputRoot,
        element: &mut Value,
        depth: &mut ElementDepth,
    ) -> Result<(), Flow>;
    fn child_depth(depth: ElementDepth) -> ElementDepth;
    fn risk(controls: Self::Controls) -> Risk;
    fn untrusted(controls: Self::Controls) -> Self::Controls;
    fn literal(controls: Self::Controls) -> Self::Controls;
    fn zero() -> Self::Extent;
    fn add(left: Self::Extent, right: Self::Extent) -> Result<Self::Extent, Flow>;
    fn sequence_child(
        controls: Self::Controls,
        used: Self::Extent,
        live_tail: Value,
    ) -> SequenceAction<Self::Controls>;
    fn propertize(eval: &mut Context, live_cdr: Value, controls: Self::Controls) -> Self::Envelope;
    fn numeric(
        limit: NumericFieldValue,
        controls: Self::Controls,
    ) -> Result<NumericTransfer<Self>, Flow>;
    fn finish(
        self,
        eval: &mut Context,
        pctx: &ModeLinePercentContext<Self>,
        result: &mut ModeLineRendered,
        controls: Self::Controls,
        used: Self::Extent,
    ) -> Result<Self::Extent, Flow>;
}

/// One RAII specpdl extent per real invocation. Tail dispatch stays in it.
/// ScratchRoots belongs to the enclosing formatter and is NOT unwound here.
#[must_use]
struct FrameRoots<'eval> {
    eval: &'eval mut Context,
    saved: crate::emacs_core::eval::SpecpdlRootScopeState,
}
impl<'eval> FrameRoots<'eval> {
    fn enter(eval: &'eval mut Context) -> Self {
        let saved = eval.save_specpdl_roots();
        Self { eval, saved }
    }
}
impl Drop for FrameRoots<'_> {
    fn drop(&mut self) {
        self.eval.restore_specpdl_roots(self.saved);
    }
}
static_assertions::assert_not_impl_any!(FrameRoots<'static>: Send, Sync);

#[inline(always)]
pub(super) fn with_root_scope<R>(
    eval: &mut Context,
    operation: impl FnOnce(&mut Context) -> R,
) -> R {
    let roots = FrameRoots::enter(eval);
    operation(roots.eval)
}

/// One decoded-kind/symbol classifier. Positioned symbols use canonical SYMBOLP.
#[inline(always)]
fn classify<P: WalkPolicy>(value: Value, symbol_mode: crate::tagged::SymbolMode) -> Node {
    match value.kind() {
        ValueKind::Nil => match P::GRAMMAR {
            Grammar::Native => Node::Empty,
            Grammar::Stored => {
                if value.is_symbol(symbol_mode) {
                    Node::Symbol
                } else {
                    Node::Empty
                }
            }
        },
        ValueKind::String => Node::Text,
        ValueKind::Fixnum(_) => match P::GRAMMAR {
            Grammar::Native => Node::Empty,
            Grammar::Stored => Node::Invalid,
        },
        ValueKind::Cons => Node::Cons,
        ValueKind::T | ValueKind::Symbol(_) => {
            if value.is_symbol(symbol_mode) {
                Node::Symbol
            } else {
                Node::Empty
            }
        }
        ValueKind::Float
        | ValueKind::Subr(_)
        | ValueKind::Veclike(_)
        | ValueKind::Unbound
        | ValueKind::Unknown => {
            // Positioned symbols are vectorlike and must use canonical SYMBOLP.
            if value.is_symbol(symbol_mode) {
                Node::Symbol
            } else {
                Node::Invalid
            }
        }
    }
}

#[inline(always)]
fn classify_cons<P: WalkPolicy>(car: Value, symbol_mode: crate::tagged::SymbolMode) -> ConsForm {
    if car.is_symbol_named(":eval", symbol_mode) {
        return ConsForm::Eval;
    }
    if car.is_symbol_named(":propertize", symbol_mode) {
        return ConsForm::Propertize;
    }
    if let Some(limit) = NumericFieldValue::from_value(car) {
        return ConsForm::Numeric(limit);
    }
    if car.is_symbol(symbol_mode) {
        match P::GRAMMAR {
            Grammar::Stored => return ConsForm::Conditional,
            Grammar::Native => {
                if !car.is_symbol_named("t", symbol_mode) {
                    return ConsForm::Conditional;
                }
            }
        }
    }
    match P::GRAMMAR {
        Grammar::Native => ConsForm::Sequence,
        Grammar::Stored => {
            if car.is_string() || car.is_cons() {
                ConsForm::Sequence
            } else {
                ConsForm::Invalid
            }
        }
    }
}

/// Read the live branch shape ONCE. Targets differ only on malformed results.
#[inline(always)]
fn select_branch(cdr: Value, truth: BranchTruth) -> Branch {
    if !cdr.is_cons() {
        return Branch::Malformed;
    }
    match truth {
        BranchTruth::NonNil => return Branch::Value(cdr.cons_car()),
        BranchTruth::Nil => {}
    }
    let rest = cdr.cons_cdr();
    if rest.is_nil() {
        Branch::Absent
    } else if rest.is_cons() {
        Branch::Value(rest.cons_car())
    } else {
        Branch::Malformed
    }
}

#[inline(always)]
fn symbol_risk<P: WalkPolicy>(
    eval: &Context,
    symbol: mode_line_symbols::ResolvedModeLineSymbol,
    controls: P::Controls,
) -> P::Controls {
    match P::SYMBOL_RISK_TIMING {
        SymbolRiskTiming::BeforeValue => {}
        SymbolRiskTiming::BeforeChild => match P::risk(controls) {
            Risk::Untrusted => return controls,
            Risk::Trusted => {}
        },
    }
    if symbol.is_risky(&eval.obarray) {
        controls
    } else {
        P::untrusted(controls)
    }
}

#[inline(always)]
fn child<P: WalkPolicy>(
    eval: &mut Context,
    pctx: &ModeLinePercentContext<P>,
    value: Value,
    parent: &mut ModeLineRendered,
    depth: ElementDepth,
    mut envelope: P::Envelope,
    selection: Option<&mut mode_line_flow_policy::FormatSelection>,
) -> Result<P::Extent, Flow> {
    let controls = envelope.controls();
    let used = walk(
        eval,
        pctx,
        &value,
        envelope.output(parent),
        depth,
        controls,
        selection,
    )?;
    envelope.finish(eval, pctx, parent, used)
}

/// Shared production semantic walker. Stored recursive %M children return here.
/// Native %M remains the native decoder's unknown/literal arm.
pub(super) fn walk<P: WalkPolicy>(
    eval: &mut Context,
    pctx: &ModeLinePercentContext<P>,
    format: &Value,
    result: &mut ModeLineRendered,
    mut depth: ElementDepth,
    mut controls: P::Controls,
    mut selection: Option<&mut mode_line_flow_policy::FormatSelection>,
) -> Result<P::Extent, Flow> {
    // DISPLAY's existing >20 guard stays before specpdl pushes.
    match P::admit_frame(depth) {
        FrameAdmission::Admit => {}
        FrameAdmission::Stop => return Ok(P::zero()),
    }
    let roots = FrameRoots::enter(eval);
    let eval = &mut *roots.eval;
    let mut element = *format;
    let element_root = P::root_input(eval, element, controls);
    result.root_for_walk();
    let mut used = P::zero();
    loop {
        pctx.target
            .begin_dispatch(eval, &element_root, &mut element, &mut depth)?;
        let symbol_mode = eval.symbol_mode();
        match classify::<P>(element, symbol_mode) {
            Node::Empty => break,
            Node::Text => {
                used = P::append_text(
                    eval,
                    pctx,
                    result,
                    &element,
                    controls,
                    TextOrigin::CurrentControls,
                    depth,
                    selection.as_deref_mut(),
                )?;
                break;
            }
            Node::Symbol => {
                let Some(name) = element.as_symbol_name(symbol_mode) else {
                    break;
                };
                let symbol = pctx.resolve_dynamic_symbol(name);
                match P::SYMBOL_RISK_TIMING {
                    SymbolRiskTiming::BeforeValue => {
                        controls = symbol_risk::<P>(eval, symbol, controls);
                    }
                    SymbolRiskTiming::BeforeChild => {}
                }
                let Some(value) = symbol.value(eval.variables(), &eval.buffers) else {
                    break;
                };
                match P::GRAMMAR {
                    Grammar::Stored => {
                        if value == element {
                            break;
                        }
                    }
                    Grammar::Native => {
                        if value.is_nil() {
                            break;
                        }
                    }
                }
                if value.is_string() {
                    match P::SYMBOL_TRANSFER {
                        Transfer::Child => {
                            // DISPLAY symbol text is literal WITHOUT another entry.
                            used = P::append_text(
                                eval,
                                pctx,
                                result,
                                &value,
                                controls,
                                TextOrigin::ForcedLiteral,
                                depth,
                                selection.as_deref_mut(),
                            )?;
                            break;
                        }
                        Transfer::Tail => controls = P::literal(controls),
                    }
                }
                match P::SYMBOL_RISK_TIMING {
                    SymbolRiskTiming::BeforeChild => {
                        controls = symbol_risk::<P>(eval, symbol, controls);
                    }
                    SymbolRiskTiming::BeforeValue => {}
                }
                match P::SYMBOL_TRANSFER {
                    Transfer::Tail => {
                        element = value;
                        P::replace_input(eval, &element_root, element);
                        continue;
                    }
                    Transfer::Child => {}
                }
                used = walk(
                    eval,
                    pctx,
                    &value,
                    result,
                    P::child_depth(depth),
                    controls,
                    selection.as_deref_mut(),
                )?;
                break;
            }
            Node::Cons => {
                let car = element.cons_car();
                let cdr = element.cons_cdr();
                eval.push_specpdl_root(car);
                eval.push_specpdl_root(cdr);
                match classify_cons::<P>(car, symbol_mode) {
                    ConsForm::Eval => {
                        if matches!(P::risk(controls), Risk::Untrusted) {
                            break;
                        }
                        if cdr.is_cons() {
                            let form = cdr.cons_car();
                            eval.push_specpdl_root(form);
                            if let Some(selection) = selection.as_deref_mut() {
                                selection.before_eval(eval);
                            }
                            // Sole walker Lisp entry. No policy executes Lisp.
                            let value = mode_line_flow_policy::eval_form(eval, &form)?;
                            // Stored preserves its explicit pre-child result root;
                            // DISPLAY's recursive frame roots it before callbacks.
                            match P::GRAMMAR {
                                Grammar::Stored => eval.push_specpdl_root(value),
                                Grammar::Native => {}
                            }
                            used = walk(
                                eval,
                                pctx,
                                &value,
                                result,
                                P::child_depth(depth),
                                controls,
                                selection.as_deref_mut(),
                            )?;
                        }
                        break;
                    }
                    ConsForm::Propertize => {
                        if matches!(P::risk(controls), Risk::Untrusted) {
                            break;
                        }
                        if cdr.is_cons() {
                            let value = cdr.cons_car();
                            let envelope = P::propertize(eval, cdr, controls);
                            used = child(
                                eval,
                                pctx,
                                value,
                                result,
                                P::child_depth(depth),
                                envelope,
                                selection.as_deref_mut(),
                            )?;
                        }
                        break;
                    }
                    ConsForm::Conditional => {
                        // Stored malformed conditionals reject before looking
                        // up CAR. Native preserves its original lookup timing.
                        match P::GRAMMAR {
                            Grammar::Stored => {
                                if !cdr.is_cons() {
                                    element = Value::string("*invalid*");
                                    P::replace_input(eval, &element_root, element);
                                    continue;
                                }
                            }
                            Grammar::Native => {}
                        }
                        let truth = car
                            .as_symbol_name(symbol_mode)
                            .and_then(|name| {
                                pctx.resolve_dynamic_symbol(name)
                                    .value(eval.variables(), &eval.buffers)
                            })
                            .is_some_and(|value| value.is_truthy());
                        match select_branch(cdr, truth.into()) {
                            Branch::Absent => break,
                            Branch::Malformed => match P::GRAMMAR {
                                Grammar::Native => break,
                                Grammar::Stored => {
                                    element = Value::string("*invalid*");
                                    P::replace_input(eval, &element_root, element);
                                    continue;
                                }
                            },
                            Branch::Value(value) => {
                                match P::CONDITIONAL_TRANSFER {
                                    Transfer::Tail => {
                                        element = value;
                                        P::replace_input(eval, &element_root, element);
                                        continue;
                                    }
                                    Transfer::Child => {}
                                }
                                used = walk(
                                    eval,
                                    pctx,
                                    &value,
                                    result,
                                    P::child_depth(depth),
                                    controls,
                                    selection.as_deref_mut(),
                                )?;
                                break;
                            }
                        }
                    }
                    ConsForm::Numeric(limit) => {
                        match P::numeric(limit, controls)? {
                            NumericTransfer::Tail(next) => {
                                controls = next;
                                element = cdr;
                                P::replace_input(eval, &element_root, element);
                                continue;
                            }
                            NumericTransfer::Child(envelope) => {
                                used = child(
                                    eval,
                                    pctx,
                                    cdr,
                                    result,
                                    P::child_depth(depth),
                                    envelope,
                                    selection.as_deref_mut(),
                                )?;
                            }
                        }
                        break;
                    }
                    ConsForm::Sequence => {
                        // One live-spine/cycle algorithm for both policies.
                        let mut tail = element;
                        let tail_root = eval.push_specpdl_root_slot(tail);
                        let checkpoint_root = eval.push_specpdl_root_slot(tail);
                        let mut cycle = ModeLineTailCycle::new(tail);
                        while tail.is_cons() {
                            let child_controls = match P::sequence_child(controls, used, tail) {
                                SequenceAction::Render(controls) => controls,
                                SequenceAction::Stop => break,
                            };
                            let value = tail.cons_car();
                            let child_used = walk(
                                eval,
                                pctx,
                                &value,
                                result,
                                P::child_depth(depth),
                                child_controls,
                                selection.as_deref_mut(),
                            )?;
                            used = P::add(used, child_used)?;
                            // NEVER advance with a pre-callback cached cdr.
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
                    ConsForm::Invalid => {
                        element = Value::string("*invalid*");
                        P::replace_input(eval, &element_root, element);
                    }
                }
            }
            Node::Invalid => match P::GRAMMAR {
                Grammar::Native => {
                    result.append_string_value_preserving_props(&element);
                    break;
                }
                Grammar::Stored => {
                    element = Value::string("*invalid*");
                    P::replace_input(eval, &element_root, element);
                }
            },
        }
    }
    pctx.target.finish(eval, pctx, result, controls, used)
}

pub(super) struct DisplayEnvelope {
    rendered: ModeLineRendered,
    controls: Risk,
    finish: DisplayFinish,
}
enum DisplayFinish {
    PropertizeLive(Value),
    Numeric(NumericFieldValue),
}
impl ChildEnvelope<DisplayTarget> for DisplayEnvelope {
    fn controls(&self) -> Risk {
        self.controls
    }
    fn output<'out>(
        &'out mut self,
        _parent: &'out mut ModeLineRendered,
    ) -> &'out mut ModeLineRendered {
        &mut self.rendered
    }
    fn finish(
        self,
        _eval: &mut Context,
        pctx: &ModeLinePercentContext<DisplayTarget>,
        parent: &mut ModeLineRendered,
        _used: (),
    ) -> Result<(), Flow> {
        let mut rendered = self.rendered;
        match self.finish {
            DisplayFinish::PropertizeLive(cdr) => {
                // IMPORTANT: read AFTER the child, retaining baseline mutation timing.
                rendered.apply_propertize_properties(cdr.cons_cdr(), &pctx.target);
                parent.append_rendered(&rendered);
            }
            DisplayFinish::Numeric(limit) => {
                // IMPORTANT: native limit interpretation occurs AFTER the child.
                let limit = limit.native_after_child();
                append_mode_line_numeric_segment(
                    parent,
                    &rendered,
                    if limit > 0 { limit } else { 0 },
                    if limit < 0 { -limit } else { 0 },
                );
            }
        }
        Ok(())
    }
}
impl WalkPolicy for DisplayTarget {
    type Controls = Risk;
    #[inline(always)]
    fn initial_controls(risk: Risk) -> Risk {
        risk
    }
    fn append_text(
        eval: &mut Context,
        pctx: &ModeLinePercentContext<Self>,
        result: &mut ModeLineRendered,
        value: &Value,
        _controls: Risk,
        origin: TextOrigin,
        _depth: ElementDepth,
        _selection: Option<&mut mode_line_flow_policy::FormatSelection>,
    ) -> Result<(), Flow> {
        let literal = match origin {
            TextOrigin::CurrentControls => ModeLineLiteral::Expand,
            TextOrigin::ForcedLiteral => ModeLineLiteral::Literal,
        };
        append_mode_line_string_in_eval_with_field_limit(
            eval,
            pctx,
            result,
            value,
            literal,
            NoModeLineNativeFieldLimit,
        );
        Ok(())
    }
    type Extent = ();
    type InputRoot = ();
    type Envelope = DisplayEnvelope;
    const GRAMMAR: Grammar = Grammar::Native;
    const SYMBOL_TRANSFER: Transfer = Transfer::Child;
    const CONDITIONAL_TRANSFER: Transfer = Transfer::Child;
    const SYMBOL_RISK_TIMING: SymbolRiskTiming = SymbolRiskTiming::BeforeChild;
    #[inline(always)]
    fn admit_frame(depth: ElementDepth) -> FrameAdmission {
        if depth.get() > 20 {
            FrameAdmission::Stop
        } else {
            FrameAdmission::Admit
        }
    }
    #[inline(always)]
    fn root_input(eval: &mut Context, value: Value, _controls: Risk) {
        eval.push_specpdl_root(value);
    }
    #[inline(always)]
    fn replace_input(_eval: &mut Context, _root: &(), _value: Value) {}
    #[inline(always)]
    fn begin_dispatch(
        self,
        _eval: &mut Context,
        _root: &(),
        _element: &mut Value,
        _depth: &mut ElementDepth,
    ) -> Result<(), Flow> {
        Ok(())
    }
    #[inline(always)]
    fn child_depth(depth: ElementDepth) -> ElementDepth {
        (depth.get() + 1).into()
    }
    #[inline(always)]
    fn risk(controls: Risk) -> Risk {
        controls
    }
    #[inline(always)]
    fn untrusted(_controls: Risk) -> Risk {
        Risk::Untrusted
    }
    #[inline(always)]
    fn literal(controls: Risk) -> Risk {
        controls
    }
    #[inline(always)]
    fn zero() {}
    #[inline(always)]
    fn add(_left: (), _right: ()) -> Result<(), Flow> {
        Ok(())
    }
    #[inline(always)]
    fn sequence_child(controls: Risk, _used: (), _tail: Value) -> SequenceAction<Risk> {
        SequenceAction::Render(controls)
    }
    fn propertize(_eval: &mut Context, live_cdr: Value, controls: Risk) -> DisplayEnvelope {
        DisplayEnvelope {
            rendered: ModeLineRendered::default(),
            controls,
            finish: DisplayFinish::PropertizeLive(live_cdr),
        }
    }
    fn numeric(limit: NumericFieldValue, controls: Risk) -> Result<NumericTransfer<Self>, Flow> {
        Ok(NumericTransfer::Child(DisplayEnvelope {
            rendered: ModeLineRendered::default(),
            controls,
            finish: DisplayFinish::Numeric(limit),
        }))
    }
    #[inline(always)]
    fn finish(
        self,
        _eval: &mut Context,
        _pctx: &ModeLinePercentContext<Self>,
        _result: &mut ModeLineRendered,
        _controls: Risk,
        _used: (),
    ) -> Result<(), Flow> {
        Ok(())
    }
}
