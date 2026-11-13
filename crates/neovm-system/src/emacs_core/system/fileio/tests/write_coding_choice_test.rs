//! Write-coding selection checks private to the fileio implementation.

#[test]
fn decide_write_coding_is_gnu_choose_write_coding_system() {
    crate::test_utils::init_test_tracing();
    use super::{
        BufferCodingSlot, BufferDefaultEol, BufferMultibyte, CodingValue, ForceRawTextReason,
        WriteCodingChoice, WriteCodingInput, WriteCodingSelector, WriteCodingSource,
        WriteCodingWarning, decide_write_coding,
    };
    use neovm_text::emacs_core::coding::WriteCodingClass;

    let selector = neovm_value::intern::intern("select-safe-coding-system");
    let utf8 = neovm_value::intern::intern("utf-8");
    let utf8_dos = neovm_value::intern::intern("utf-8-dos");
    let no_conversion = neovm_value::intern::intern("no-conversion");
    let present = WriteCodingSelector::Present(selector);
    let absent = WriteCodingSelector::Absent;
    let silent = WriteCodingWarning::Silent;
    let confirm = WriteCodingWarning::Confirm;
    let raw = WriteCodingClass::RawText;
    let selectable = WriteCodingClass::Selectable;
    let multibyte = BufferMultibyte::Multibyte;
    let unibyte = BufferMultibyte::Unibyte;

    let bound = |warning| WriteCodingSource::Bound {
        coding: utf8,
        warning,
    };
    let buffer = |chosen, class| WriteCodingSource::Buffer { chosen, class };
    let input = |source, selector, multibyte, slot, buffer_default| WriteCodingInput {
        source,
        selector,
        multibyte,
        slot,
        buffer_default,
    };

    // The whole GNU `choose_write_coding_system` truth table
    // (src/fileio.c:5383-5450).  Every arm is one typed state; a selector arm
    // carries its symbol, and the forced arms carry the source of the force.
    let cases = [
        // A bound `coding-system-for-write` is used as-is unless
        // `coding-system-require-warning` asks for confirmation.
        (
            input(
                bound(silent),
                absent,
                multibyte,
                BufferCodingSlot::Inherited(CodingValue::Unset),
                CodingValue::Unset,
            ),
            WriteCodingChoice::BoundDirect(utf8),
        ),
        (
            input(
                bound(silent),
                present,
                multibyte,
                BufferCodingSlot::Inherited(CodingValue::Unset),
                CodingValue::Unset,
            ),
            WriteCodingChoice::BoundDirect(utf8),
        ),
        (
            input(
                bound(confirm),
                absent,
                multibyte,
                BufferCodingSlot::Inherited(CodingValue::Unset),
                CodingValue::Unset,
            ),
            WriteCodingChoice::BoundDirect(utf8),
        ),
        (
            input(
                bound(confirm),
                present,
                multibyte,
                BufferCodingSlot::Inherited(CodingValue::Unset),
                CodingValue::Unset,
            ),
            WriteCodingChoice::BoundConfirmed {
                coding: utf8,
                selector,
            },
        ),
        // (a) unibyte and no usable local coding forces raw text, with or
        // without a selector.
        (
            input(
                buffer(utf8, selectable),
                present,
                unibyte,
                BufferCodingSlot::Inherited(CodingValue::Coding(utf8)),
                CodingValue::Coding(utf8),
            ),
            WriteCodingChoice::ForcedRawText {
                reason: ForceRawTextReason::UnibyteBufferWithoutLocalCoding,
                coding: utf8,
                eol: BufferDefaultEol::Settled,
            },
        ),
        (
            input(
                buffer(utf8, selectable),
                absent,
                unibyte,
                BufferCodingSlot::Local(CodingValue::Unset),
                CodingValue::Unset,
            ),
            WriteCodingChoice::ForcedRawText {
                reason: ForceRawTextReason::UnibyteBufferWithoutLocalCoding,
                coding: utf8,
                eol: BufferDefaultEol::Settled,
            },
        ),
        // A local coding disables (a); a selectable coding then follows the
        // selector rule.
        (
            input(
                buffer(utf8, selectable),
                absent,
                unibyte,
                BufferCodingSlot::Local(CodingValue::Coding(utf8)),
                CodingValue::Coding(utf8_dos),
            ),
            WriteCodingChoice::Selected {
                coding: utf8,
                eol: BufferDefaultEol::InheritFrom(utf8_dos),
            },
        ),
        // (b) a raw-text-type coding forces raw text.
        (
            input(
                buffer(no_conversion, raw),
                present,
                multibyte,
                BufferCodingSlot::Local(CodingValue::Coding(no_conversion)),
                CodingValue::Coding(utf8_dos),
            ),
            WriteCodingChoice::ForcedRawText {
                reason: ForceRawTextReason::RawTextCodingType,
                coding: no_conversion,
                eol: BufferDefaultEol::InheritFrom(utf8_dos),
            },
        ),
        (
            input(
                buffer(no_conversion, raw),
                present,
                multibyte,
                BufferCodingSlot::Inherited(CodingValue::Coding(no_conversion)),
                CodingValue::Unset,
            ),
            WriteCodingChoice::ForcedRawText {
                reason: ForceRawTextReason::RawTextCodingType,
                coding: no_conversion,
                eol: BufferDefaultEol::Settled,
            },
        ),
        // Non-forced buffer selections: the selector decides when present,
        // otherwise the chosen coding is used directly.
        (
            input(
                buffer(utf8, selectable),
                present,
                multibyte,
                BufferCodingSlot::Local(CodingValue::Coding(utf8)),
                CodingValue::Coding(utf8_dos),
            ),
            WriteCodingChoice::SelectWithDefault {
                coding: utf8,
                selector,
                eol: BufferDefaultEol::InheritFrom(utf8_dos),
            },
        ),
        (
            input(
                buffer(utf8, selectable),
                absent,
                multibyte,
                BufferCodingSlot::Inherited(CodingValue::Coding(utf8)),
                CodingValue::Coding(utf8),
            ),
            WriteCodingChoice::Selected {
                coding: utf8,
                eol: BufferDefaultEol::Settled,
            },
        ),
        (
            input(
                buffer(utf8, selectable),
                present,
                multibyte,
                BufferCodingSlot::Inherited(CodingValue::Coding(utf8)),
                CodingValue::Coding(utf8),
            ),
            WriteCodingChoice::SelectWithDefault {
                coding: utf8,
                selector,
                eol: BufferDefaultEol::Settled,
            },
        ),
        (
            input(
                buffer(utf8, selectable),
                present,
                multibyte,
                BufferCodingSlot::Local(CodingValue::Unset),
                CodingValue::Unset,
            ),
            WriteCodingChoice::SelectWithDefault {
                coding: utf8,
                selector,
                eol: BufferDefaultEol::Settled,
            },
        ),
    ];
    for (case, expected) in cases {
        assert_eq!(decide_write_coding(case), expected);
    }
}
