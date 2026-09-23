use super::{CoverageRequirement, GnuFontPolicy, coalesce_ranges};
use neovm_core::emacs_core::fontset::StoredFontSpec;
use neovm_core::emacs_core::intern::intern;

#[test]
fn registry_constraints_are_platform_neutral_and_typed() {
    let spec = StoredFontSpec {
        family: None,
        registry: Some(intern("gb2312.1980-0")),
        lang: None,
        weight: None,
        slant: None,
        width: None,
        repertory: None,
    };

    let constraints = GnuFontPolicy::constraints_for_spec(&spec, '好');

    assert_eq!(constraints.representative_char(), '专');
    assert_eq!(constraints.languages()[0].as_str(), "zh-cn");
    assert_eq!(
        constraints.coverage(),
        &CoverageRequirement::Ranges(vec![(0x4e13, 0x4e13), ('好' as u32, '好' as u32),])
    );
}

#[test]
fn generic_unicode_registry_does_not_invent_a_charset_filter() {
    let spec = StoredFontSpec {
        family: None,
        registry: Some(intern("iso10646-1")),
        lang: None,
        weight: None,
        slant: None,
        width: None,
        repertory: None,
    };

    let constraints = GnuFontPolicy::constraints_for_spec(&spec, 'λ');

    assert_eq!(constraints.coverage(), &CoverageRequirement::Any);
}

#[test]
fn coverage_normalization_preserves_reversed_range_endpoints() {
    assert_eq!(
        coalesce_ranges(vec![(0x9fff, 0x4e00), (0x3400, 0x4dbf)]),
        vec![(0x3400, 0x4dbf), (0x4e00, 0x9fff)]
    );
}
