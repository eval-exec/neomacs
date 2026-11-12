//! Registered read-coding selection and setup policy.
//!
//! GNU detects before conversion, preserving the difference between replacing
//! a decoder and changing only its reported category identity. EOL is resolved
//! after the lower codec has produced characters.

use super::{CodingSystemManager, EolType};
use crate::encoding::{
    CodingReadSelection, RuntimeCodingSystem, apply_explicit_eol_suffix, coding_name_eol,
};
use neovm_value::intern::{intern, resolve_sym};

pub(crate) fn select(
    coding_systems: &CodingSystemManager,
    coding_system: &str,
    bytes: &[u8],
    block: super::SourceBlock,
    mode: neovm_value::tagged::SymbolMode,
    global: bool,
) -> Option<CodingReadSelection> {
    let eol = runtime_coding_eol(coding_systems, coding_system);
    match super::undecided_null_byte_inhibition(coding_systems, coding_system, global) {
        Some(inhibit_null) => {
            let detected = detect_undecided_coding(
                coding_systems,
                bytes,
                super::undecided_prefers_utf8(coding_systems, coding_system),
                inhibit_null,
                block,
                mode,
            );
            use super::DetectedBase;
            match detected {
                DetectedBase::Unchanged => None,
                DetectedBase::IdentityOnly(name) => {
                    Some(CodingReadSelection::IdentityOnly(resolve_sym(name)))
                }
                DetectedBase::Rebase(name) => {
                    Some(CodingReadSelection::Rebase(resolve_sym(intern(
                        &inherit_detection_eol(coding_systems, resolve_sym(name), eol),
                    ))))
                }
            }
        }
        // The two arms that are keyed on a byte-order mark rather than on the
        // category priority list (:6702-6742).
        None => {
            let found = detect_bom_auto_coding(coding_systems, coding_system, bytes, block, mode)?;
            Some(CodingReadSelection::Rebase(resolve_sym(intern(
                &inherit_detection_eol(coding_systems, found, eol),
            ))))
        }
    }
}

pub(crate) fn runtime_coding_eol(systems: &super::CodingSystemManager, name: &str) -> EolType {
    RuntimeCodingSystem::from_symbol(intern(name))
        .metadata(systems)
        .map_or_else(|| coding_name_eol(name), |metadata| metadata.eol_type)
}

/// The registered codec name, independent of the alias a conversion reports.
pub(crate) fn runtime_codec_name(
    systems: &super::CodingSystemManager,
    name: &str,
) -> Option<String> {
    RuntimeCodingSystem::from_symbol(intern(name))
        .metadata(systems)
        .map(|metadata| {
            let canonical = resolve_sym(metadata.info.name);
            if metadata.eol_type == metadata.info.eol_type {
                canonical.to_owned()
            } else {
                apply_explicit_eol_suffix(canonical, metadata.eol_type)
            }
        })
}

/// GNU adjusts the selected target only while its EOL remains undecided.
/// A fixed target keeps both its EOL and its alias (coding.c:6471-6495).
fn inherit_detection_eol(
    systems: &super::CodingSystemManager,
    selected: &str,
    requested: EolType,
) -> String {
    if requested == EolType::Undecided
        || runtime_coding_eol(systems, selected) != EolType::Undecided
    {
        return selected.to_owned();
    }
    systems
        .canonical_name_for_detected_eol(selected, requested.suffix())
        .unwrap_or_else(|| selected.to_owned())
}

/// Resolve `undecided` / `prefer-utf-8` to a concrete coding system by
/// inspecting the byte pattern, mirroring GNU's `detect_coding`: walk the
/// configured detection categories in priority order and return the first
/// defined coding system whose category accepts the bytes.
fn detect_undecided_coding(
    coding_systems: &super::CodingSystemManager,
    bytes: &[u8],
    prefer_utf_8: bool,
    inhibit_null: bool,
    block: super::SourceBlock,
    mode: neovm_value::tagged::SymbolMode,
) -> super::DetectedBase {
    super::detect_highest_coding_system_for_unibyte_bytes(
        coding_systems,
        bytes,
        block,
        mode,
        inhibit_null,
        prefer_utf_8,
    )
}

/// The BOM-detecting arms of GNU's `detect_coding` (src/coding.c:6702-6742).
///
/// Two coding systems are "auto" in a way `undecided` is not: their `:bom`
/// property is a CONS of the two concrete systems the byte-order mark chooses
/// between, and their detection category is `coding_category_utf_8_auto` /
/// `coding_category_utf_16_auto` rather than `coding_category_undecided`.
/// `detect_coding` gives each its own arm, and each arm ends in
/// `setup_coding_system (found, coding)` -- so the coding system a `utf-16`
/// decode actually RUNS with, and reports, is one of the two concrete ones.
/// Measured under GNU 31.0.90:
///
/// ```text
/// (decode-coding-string "\xfe\xff\0a\0\r\0\n\0b" 'utf-16) => last-coding-system-used  utf-16be-with-signature-dos
/// (decode-coding-string "\xff\xfe" ... 'utf-16)           => last-coding-system-used  utf-16le-with-signature-dos
/// (decode-coding-string "\0a\0\r\0\n\0b" 'utf-16)         => last-coding-system-used  utf-16-dos
/// ```
///
/// The third row is the one that says what "found" means: with no BOM,
/// `detect_coding_utf_16` sets neither `CATEGORY_MASK_UTF_16_LE` nor `_BE` --
/// those two flags come ONLY from the two BOM branches (src/coding.c:1513-1526)
/// -- so `found` stays nil and the coding system keeps its own name and its own
/// default (big-endian) byte order.  The `_NOSIG` categories that the pattern
/// scan below those branches can set are a different pair of flags and the auto
/// arm does not look at them.
///
/// Returns `None` when GNU's `found` would be nil, i.e. when nothing re-bases.
fn detect_bom_auto_coding(
    coding_systems: &super::CodingSystemManager,
    coding_system: &str,
    bytes: &[u8],
    block: super::SourceBlock,
    mode: neovm_value::tagged::SymbolMode,
) -> Option<&'static str> {
    use super::BomAutoCoding;
    let found = match super::bom_auto_coding(coding_systems, coding_system)? {
        // src/coding.c:6724-6742.  `coding->head_ascii = 0` and then
        // `detect_coding_utf_16`, which REFUSES an odd-length last block
        // outright (src/coding.c:1501-1507) before it looks at a signature --
        // measured, `(decode-coding-string "\xfe\xff\0" 'utf-16)` answers
        // `utf-16` and keeps U+FEFF as a character.
        BomAutoCoding::Utf16 { .. }
            if bytes.len() % 2 == 1 && matches!(block, super::SourceBlock::Last) =>
        {
            None
        }
        BomAutoCoding::Utf16 { little, big } => match bytes {
            [0xFF, 0xFE, ..] => Some(little),
            [0xFE, 0xFF, ..] => Some(big),
            _ => None,
        },
        // src/coding.c:6702-6722.  All-ASCII takes `XCDR` without even running
        // the detector; otherwise `detect_coding_utf_8` decides, and a BOM
        // takes `XCAR`.  Either way `found` is non-nil whenever the text is
        // valid UTF-8, so `utf-8-auto` re-bases to a system that is not `auto`.
        BomAutoCoding::Utf8 {
            signature,
            without_signature,
        } => {
            let mut detection = super::DetectInfo::default();
            if bytes.iter().all(|byte| byte.is_ascii()) {
                Some(without_signature)
            } else if !super::detect_utf_8(bytes, 0, false, block, &mut detection) {
                None
            } else if detection.found & super::cat_mask(super::CodingCat::Utf8Sig) != 0 {
                // GNU's strict `src + 3 < src_end` requires a following byte.
                // Exact three-byte input selects the other target, retaining U+FEFF.
                Some(signature)
            } else {
                Some(without_signature)
            }
        }
    }?;
    if found.is_nil() {
        return None;
    }
    found.as_symbol_id(mode).map(resolve_sym)
}

/// Registered setup raises detection for undecided or BOM-auto definitions.
pub(crate) fn requires_detection(mgr: &CodingSystemManager, name: &str) -> bool {
    super::undecided_null_byte_inhibition(mgr, name, false).is_some()
        || super::bom_auto_coding(mgr, name).is_some()
}

/// GNU `CODING_ID_NAME (coding->id)` after a decode: the coding system the
/// decode reports having used, read out of the resolution that ran it.
///
/// This is the name behind `last-coding-system-used` (src/coding.c:9497 for a
/// region, :9644 for a string) and
/// behind `(process-coding-system P)` (src/process.c:6421) alike, because in
/// GNU they are one field of one object.  When `adjust_coding_eol_type` fired,
/// it is the SUBSIDIARY that call rewrote the id to -- and the subsidiary comes
/// out of the coding system's own eol vector, which holds canonical names, so
/// an alias reports its base's subsidiary (measured under GNU 31.0.90:
/// `coding-system-for-read` `latin-1` over a CR-only child answers
/// `iso-latin-1-mac`).  When it did not fire -- a concrete eol type, no
/// terminator in the text, or `inhibit-eol-conversion` -- the name is the one
/// the coding system was set up with, verbatim.
///
/// Taking the [`DecodeEolResolution`](super::DecodeEolResolution)
/// rather than re-scanning the text is what keeps the reported name and the
/// conversion the same decision: GNU makes them one call, and every caller here
/// has to make them one value.
pub(crate) fn adjusted_coding_name(
    coding_systems: &super::CodingSystemManager,
    coding_system: &str,
    eol: super::DecodeEolResolution,
) -> String {
    match eol.adjusted() {
        Some(resolved) => coding_systems
            .canonical_name_for_detected_eol(coding_system, resolved.suffix())
            .unwrap_or_else(|| coding_system.to_owned()),
        None => coding_system.to_owned(),
    }
}
