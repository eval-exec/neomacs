use libfuzzer_sys::{
    Corpus,
    arbitrary::{self, Arbitrary},
};
use neovm_core::fuzz_support::{
    RegexCase, RegexCheck, RegexDifferential, SearchTarget, check_regex_differential,
};

/// Compact libFuzzer wire format for a semantic regexp differential case.
///
/// Scalar fields come first so even short inputs reach the checker. Offsets
/// are deliberately bounded on the wire; `RegexCase` maps them into the
/// generated text and remains independent of any fuzzing framework.
#[derive(Arbitrary, Debug)]
pub struct ArbitraryRegexCase<'a> {
    case_fold: bool,
    /// Search a unibyte text instead of a multibyte one.  Adding this field
    /// changed the wire format: corpora saved before it replay as other cases.
    unibyte_target: bool,
    start: u16,
    point: u16,
    pattern: &'a str,
    text: &'a [u8],
}

pub fn check(case: ArbitraryRegexCase<'_>, differential: RegexDifferential) -> Corpus {
    let semantic_case = RegexCase::new(
        case.pattern,
        case.text,
        case.case_fold,
        usize::from(case.start),
        usize::from(case.point),
    )
    .with_target(if case.unibyte_target {
        SearchTarget::Unibyte
    } else {
        SearchTarget::Multibyte
    });
    match check_regex_differential(semantic_case, differential) {
        Ok(RegexCheck::Equivalent { .. }) => Corpus::Keep,
        Ok(RegexCheck::NotApplicable(_)) => Corpus::Reject,
        Err(divergence) => panic!("{divergence}; case={case:?}"),
    }
}
