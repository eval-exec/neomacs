//! Stored-target leaf accounting and output policy for the common walker.
//! No symbol lookup, :eval, conditional selection, sequence loop or walker here.
use super::super::mode_line_target::StoredTarget;
use super::super::mode_line_walk::NumericFieldValue;
use super::super::mode_line_walk::{
    ChildEnvelope, Grammar, NumericTransfer, SymbolRiskTiming, Transfer, WalkPolicy,
};
use super::super::mode_line_walk::{FrameAdmission, SequenceAction};
use super::*;

pub(in super::super) struct StoredEnvelope {
    controls: StringEmissionControls,
}
impl<'scope> ChildEnvelope<StoredTarget<'scope>> for StoredEnvelope {
    fn controls(&self) -> StringEmissionControls {
        self.controls
    }
    fn output<'out>(
        &'out mut self,
        parent: &'out mut ModeLineRendered,
    ) -> &'out mut ModeLineRendered {
        parent
    }
    #[inline(always)]
    fn finish(
        self,
        _eval: &mut Context,
        _pctx: &ModeLinePercentContext<StoredTarget<'scope>>,
        _parent: &mut ModeLineRendered,
        used: FieldBudget,
    ) -> Result<FieldBudget, Flow> {
        Ok(used)
    }
}

impl<'scope> WalkPolicy for StoredTarget<'scope> {
    type Controls = StringEmissionControls;
    #[inline(always)]
    fn initial_controls(risk: Risk) -> StringEmissionControls {
        StringEmissionControls::unrestricted(risk)
    }
    fn append_text(
        eval: &mut Context,
        pctx: &ModeLinePercentContext<Self>,
        result: &mut ModeLineRendered,
        value: &Value,
        mut controls: StringEmissionControls,
        origin: super::super::mode_line_walk::TextOrigin,
        depth: ElementDepth,
        selection: Option<&mut mode_line_flow_policy::FormatSelection>,
    ) -> Result<FieldBudget, Flow> {
        match origin {
            super::super::mode_line_walk::TextOrigin::CurrentControls => {}
            super::super::mode_line_walk::TextOrigin::ForcedLiteral => {
                controls.literal = ModeLineLiteral::Literal;
            }
        }
        append_mode_line_string_in_eval_with_controls(
            eval, pctx, result, value, controls, depth, selection,
        )
    }
    type Extent = FieldBudget;
    type InputRoot = crate::emacs_core::eval::SpecpdlRootSlot;
    type Envelope = StoredEnvelope;
    const GRAMMAR: Grammar = Grammar::Stored;
    const SYMBOL_TRANSFER: Transfer = Transfer::Tail;
    const CONDITIONAL_TRANSFER: Transfer = Transfer::Tail;
    const SYMBOL_RISK_TIMING: SymbolRiskTiming = SymbolRiskTiming::BeforeValue;

    #[inline(always)]
    fn admit_frame(_depth: ElementDepth) -> FrameAdmission {
        FrameAdmission::Admit
    }
    fn root_input(
        eval: &mut Context,
        value: Value,
        controls: StringEmissionControls,
    ) -> Self::InputRoot {
        let root = eval.push_specpdl_root_slot(value);
        eval.push_specpdl_root(controls.properties.0);
        root
    }
    #[inline(always)]
    fn replace_input(eval: &mut Context, root: &Self::InputRoot, value: Value) {
        eval.set_specpdl_root_slot(root, value);
    }
    fn begin_dispatch(
        self,
        eval: &mut Context,
        root: &Self::InputRoot,
        element: &mut Value,
        depth: &mut ElementDepth,
    ) -> Result<(), Flow> {
        // Exactly one stored logical entry, even on a tail transition.
        if depth.0 > 100 {
            *element = Value::string("*too-deep*");
            eval.set_specpdl_root_slot(root, *element);
        }
        self.enter_element()?;
        *depth = depth.next()?;
        Ok(())
    }
    #[inline(always)]
    fn child_depth(depth: ElementDepth) -> ElementDepth {
        depth
    }
    #[inline(always)]
    fn risk(controls: StringEmissionControls) -> Risk {
        controls.risk
    }
    #[inline(always)]
    fn untrusted(controls: StringEmissionControls) -> StringEmissionControls {
        StringEmissionControls {
            risk: Risk::Untrusted,
            ..controls
        }
    }
    #[inline(always)]
    fn literal(controls: StringEmissionControls) -> StringEmissionControls {
        StringEmissionControls {
            literal: ModeLineLiteral::Literal,
            ..controls
        }
    }
    #[inline(always)]
    fn zero() -> FieldBudget {
        FieldBudget::ZERO
    }
    #[inline(always)]
    fn add(left: FieldBudget, right: FieldBudget) -> Result<FieldBudget, Flow> {
        left.checked_add(right)
    }
    fn sequence_child(
        controls: StringEmissionControls,
        used: FieldBudget,
        live_tail: Value,
    ) -> SequenceAction<StringEmissionControls> {
        let mut child = controls.remaining(used);
        if matches!(
            child.limits.precision.availability(),
            PrecisionAvailability::Exhausted
        ) {
            return SequenceAction::Stop;
        }
        // This pre-child read chooses padding only; common walker rereads the
        // live cdr AFTER the callback to advance the sequence.
        if live_tail.cons_cdr().is_cons() {
            child.limits.minimum = FieldBudget::ZERO;
        }
        SequenceAction::Render(child)
    }
    fn propertize(
        eval: &mut Context,
        live_cdr: Value,
        controls: StringEmissionControls,
    ) -> StoredEnvelope {
        // Stored props are captured/rooted BEFORE the child; native envelope
        // deliberately captures only live_cdr and reads props AFTER the child.
        let properties = live_cdr.cons_cdr();
        eval.push_specpdl_root(properties);
        StoredEnvelope {
            controls: StringEmissionControls {
                properties: InheritedProperties(properties),
                ..controls
            },
        }
    }
    fn numeric(
        limit: NumericFieldValue,
        controls: StringEmissionControls,
    ) -> Result<NumericTransfer<Self>, Flow> {
        // Strict overflow is signalled BEFORE the logical tail's callbacks.
        Ok(NumericTransfer::Tail(StringEmissionControls {
            limits: controls
                .limits
                .numeric(NumericField::try_from(limit.stored_before_tail())?),
            ..controls
        }))
    }
    fn finish(
        self,
        _eval: &mut Context,
        _pctx: &ModeLinePercentContext<Self>,
        result: &mut ModeLineRendered,
        controls: StringEmissionControls,
        used: FieldBudget,
    ) -> Result<FieldBudget, Flow> {
        let padding = controls.limits.minimum.saturating_sub(used);
        if padding.get() != 0 {
            let start = CharPos0::new(result.char_len());
            result.pad_plain_spaces(padding.get());
            result.numeric_padding.mark(start.get()..result.char_len());
            if matches!(self, StoredTarget::String(_)) {
                result.apply_element_attribution(
                    CharRange::from_start_len(start, CharLen::new(padding.get())),
                    ElementAttribution::SyntheticNumericPadding,
                );
            }
        }
        used.checked_add(padding)
    }
}
