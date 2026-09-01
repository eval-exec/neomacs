use crate::display_row::transition::{DisplayRowOverflowTransitionPlan, VisualWrapBreak};
use crate::display_row::walk_state::{
    DisplayRowTextOverflowDecision, SpecialTextRowOverflowDecision, TextRowTransitionStatePolicy,
    WordWrapBreakCandidate,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplaySourceTextCharOverflowAction {
    Fits,
    Truncate {
        transition: DisplayRowOverflowTransitionPlan,
    },
    WordWrap {
        break_candidate: WordWrapBreakCandidate,
        transition: DisplayRowOverflowTransitionPlan,
    },
    CharacterWrap {
        transition: DisplayRowOverflowTransitionPlan,
    },
}

impl DisplaySourceTextCharOverflowAction {
    pub(crate) fn for_decision(decision: DisplayRowTextOverflowDecision) -> Self {
        match decision {
            DisplayRowTextOverflowDecision::Fits => Self::Fits,
            DisplayRowTextOverflowDecision::Truncate => Self::Truncate {
                transition: DisplayRowOverflowTransitionPlan::truncation(
                    TextRowTransitionStatePolicy::truncation(),
                ),
            },
            DisplayRowTextOverflowDecision::WordWrap { break_candidate } => Self::WordWrap {
                break_candidate,
                transition: DisplayRowOverflowTransitionPlan::visual_wrap(
                    VisualWrapBreak::AtWordBoundary,
                    TextRowTransitionStatePolicy::visual_wrap(),
                ),
            },
            DisplayRowTextOverflowDecision::CharacterWrap => Self::CharacterWrap {
                transition: DisplayRowOverflowTransitionPlan::visual_wrap(
                    VisualWrapBreak::MidElement,
                    TextRowTransitionStatePolicy::character_wrap(),
                ),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplaySourceSpecialCharOverflowAction {
    Fits,
    Truncate {
        transition: DisplayRowOverflowTransitionPlan,
    },
    Wrap {
        transition: DisplayRowOverflowTransitionPlan,
    },
}

impl DisplaySourceSpecialCharOverflowAction {
    pub(crate) fn for_decision(decision: SpecialTextRowOverflowDecision) -> Self {
        match decision {
            SpecialTextRowOverflowDecision::Fits => Self::Fits,
            SpecialTextRowOverflowDecision::Truncate => Self::Truncate {
                transition: DisplayRowOverflowTransitionPlan::truncation(
                    TextRowTransitionStatePolicy::special_truncation(),
                ),
            },
            SpecialTextRowOverflowDecision::Wrap => Self::Wrap {
                transition: DisplayRowOverflowTransitionPlan::visual_wrap(
                    VisualWrapBreak::MidElement,
                    TextRowTransitionStatePolicy::special_visual_wrap(),
                ),
            },
        }
    }
}
