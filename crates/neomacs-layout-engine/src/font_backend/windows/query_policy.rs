//! Coverage and explicit-language admission for DirectWrite catalog entries.
//!
//! Kept independent of COM so the same native predicate is testable on every
//! host. Native language hints may include a registry-derived language; only
//! the raw explicit :lang receives GNU Windows' codepage restriction.

use super::FontCandidateQuery;

pub(super) fn query_is_satisfied_by(
    query: &FontCandidateQuery,
    supports: impl FnMut(u32) -> bool,
    os2_table: impl FnOnce() -> Option<Vec<u8>>,
) -> bool {
    if !query.coverage_is_satisfied_by(supports) {
        return false;
    }
    // GNU w32font.c font_matches_spec tests only these exact explicit
    // language symbols against FONTSIGNATURE.fsCsb[0]. Registry-derived
    // Fontconfig hints do not represent an explicit :lang restriction.
    let required_codepages = match query.explicit_language.as_deref() {
        None => return true,
        Some("ja") => 1 << 17,
        Some("ko") => (1 << 19) | (1 << 21),
        Some("zh") => (1 << 18) | (1 << 20),
        Some(_) => return false,
    };
    let Some(table) = os2_table() else {
        return false;
    };
    // ulCodePageRange1 was introduced in OS/2 version 1. OpenType table
    // integers are big-endian, independently of the native platform.
    let Some(version) = table.get(..2).and_then(|bytes| bytes.try_into().ok()) else {
        return false;
    };
    if u16::from_be_bytes(version) == 0 {
        return false;
    }
    table
        .get(78..82)
        .and_then(|bytes| bytes.try_into().ok())
        .is_some_and(|bytes| u32::from_be_bytes(bytes) & required_codepages != 0)
}

#[cfg(test)]
#[path = "tests/query_policy_test.rs"]
mod tests;
