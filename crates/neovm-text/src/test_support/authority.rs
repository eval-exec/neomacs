//! Test-only authority fence for core-owned tests of the moved regex probes.
//!
//! Writes and resets stay in this module: the originals are plain
//! `pub(crate)` with no visibility widening, and this fence is their only
//! cross-crate route.  Reads use the TEST-SUPPORT-VIS rows instead.

/// P1-CLASSES-ALL-CRATES FENCE: run `f` with the syntax-class fusion
/// peephole disabled and restore the previous policy afterwards.  This is a
/// scoped thread-local write, so the fence is the only cross-crate route;
/// the original `with_syntax_fusion_disabled` stays `pub(crate)`.
pub fn with_syntax_fusion_disabled_for_test<R>(f: impl FnOnce() -> R) -> R {
    crate::emacs_core::regex_emacs::with_syntax_fusion_disabled(f)
}

/// P1-CLASSES-ALL-CRATES FENCE: zero the debug-only publish/read counters
/// used by the lazy-match-data probe.  A counter reset is a write, so the
/// fence is the only cross-crate route; the original `reset` stays
/// `pub(crate)`.  The counters exist only in debug builds, where the probe
/// runs; optimized builds have nothing to reset.
pub fn reset_regex_match_stats_for_test() {
    #[cfg(debug_assertions)]
    crate::emacs_core::regex::match_stats::reset();
}

/// FENCE (N2 TLS reset): clear this thread's regex pattern caches.
/// The original stays pub(crate) with no visibility widening; this fence is
/// its only cross-crate route (LEAD 18:1xZ).
pub fn clear_regex_caches() {
    crate::emacs_core::regex::clear_regex_caches();
}

/// FENCE (N2 TLS reset): reset this thread's existence-DFA filter counters.
/// The original stays pub(crate) with no visibility widening; this fence is
/// its only cross-crate route (LEAD 18:1xZ).
pub fn reset_dfa_stats() {
    crate::emacs_core::regex_emacs::dfa::reset_dfa_stats();
}

/// FENCE (N2 TLS reset): reset this thread's syntax parse-cache stats.
/// The original stays pub(crate) with no visibility widening; this fence is
/// its only cross-crate route (LEAD 18:1xZ).
pub fn reset_parse_cache_stats() {
    crate::emacs_core::syntax::parse_cache::reset_parse_cache_stats();
}

/// FENCE (N2 TLS reset): zero the per-thread syntax-table decode counter.
/// The original stays pub(crate) with no visibility widening; this fence is
/// its only cross-crate route (LEAD 18:1xZ).
pub fn reset_syntax_table_decodes_for_test() {
    crate::emacs_core::syntax::reset_syntax_table_decodes_for_test();
}

/// FENCE (R4-F2, peer P5): arm this process's existence-DFA filter counters.
/// The runtime stats enable stays `cfg!(test)`; without this explicit arming
/// a test-support dependency build records nothing (own-build discriminator).
/// Inert unless called.  Arming persists for the rest of the process (there
/// is no disarm); the moved DFA tests rely on nextest running every test in
/// its own process, so a later test in the same process would still observe
/// the counters armed.
pub fn enable_dfa_stats_for_test() {
    crate::emacs_core::regex_emacs::dfa::enable_dfa_stats_for_test();
}

/// FENCE (global charset-state write): reorder the process charset registry,
/// as `set-charset-priority` does.  The fontset ordering tests in display need
/// this write to prove the charset-preference substrate hook republishes the
/// rendering order.  Lisp is unreachable there: the subr is registered by the
/// upper core layer, which display's unit runtime replaces with the signed
/// absent-layer stand-in, so the global write routes through this fence.  The
/// original builtin stays `pub(crate)`; its pre-existing test-support widening
/// is the signed TEST-SUPPORT-VIS-TEXT row used by the moved core tests.
pub fn set_charset_priority_for_test(
    args: Vec<neovm_value::value::Value>,
) -> neovm_value::error::EvalResult {
    crate::emacs_core::charset::builtin_set_charset_priority(args)
}

/// FENCE (global charset-state write): define or re-point a charset alias, as
/// `define-charset-alias` does.  Same reachability argument as
/// [`set_charset_priority_for_test`]: the alias mutation is global charset
/// state and the original builtin stays `pub(crate)`.
pub fn define_charset_alias_for_test(
    args: Vec<neovm_value::value::Value>,
) -> neovm_value::error::EvalResult {
    crate::emacs_core::charset::builtin_define_charset_alias(args)
}
// ---------------------------------------------------------------------------
// peer #816 VIS write-arm (neovm-text): construction fences and one setter
// per widened field.  Consumer tests stay read-only over the widened fields;
// every write goes through one of these functions, which carry an
// AUTHORITY-ITEMS row and are the only cross-crate test route for their item.
// ---------------------------------------------------------------------------

use neovm_edit::buffer::EmacsBytePos;
use neovm_value::emacs_char::EmacsChar;
use neovm_value::intern::SymId;
use neovm_value::value::Value;

use crate::emacs_core::charset::{
    CharsetInfo, CharsetMapCacheKey, CharsetMapData, CharsetMethod, CharsetPlist,
};
use crate::emacs_core::coding::{CODING_CAT_MAX, CharsetDetectionPlan, CodingDetectionBindings};
use crate::emacs_core::regex_emacs::dfa::{
    ClassContext, DfaCounters, DfaGaveUp, DfaLease, DfaSlot, ExistenceDfa,
};
use crate::emacs_core::regex_emacs::{
    BoundarySyntaxCharacter, BoundarySyntaxRequest, CompiledPattern, MatchSyntax, SyntaxAssertion,
};
use crate::emacs_core::syntax::SyntaxTable;
use crate::emacs_core::syntax::parse_cache::RunKey;

/// FENCE (peer #816 write-arm): `BoundarySyntaxCharacter` construction.
/// Every field explicit; the only cross-crate test construction path.
pub fn boundary_syntax_character(
    character: EmacsChar,
    byte_position: EmacsBytePos,
) -> BoundarySyntaxCharacter {
    BoundarySyntaxCharacter {
        character,
        byte_position,
    }
}

/// FENCE (peer #816 write-arm): `BoundarySyntaxRequest` construction.
/// Every field explicit; the only cross-crate test construction path.
pub fn boundary_syntax_request(
    kind: SyntaxAssertion,
    current_byte: EmacsBytePos,
    previous: Option<BoundarySyntaxCharacter>,
    current: Option<BoundarySyntaxCharacter>,
) -> BoundarySyntaxRequest {
    BoundarySyntaxRequest {
        kind,
        current_byte,
        previous,
        current,
    }
}

/// FENCE (peer #816 write-arm): `SyntaxTable` construction.
pub fn syntax_table(chartable: Value) -> SyntaxTable {
    SyntaxTable { chartable }
}

/// FENCE (peer #816 write-arm): `CodingDetectionBindings` construction.
/// Every field explicit; the only cross-crate test construction path.
pub fn coding_detection_bindings(
    priorities: Vec<usize>,
    systems: [Option<SymId>; CODING_CAT_MAX],
    charset: Option<CharsetDetectionPlan>,
) -> CodingDetectionBindings {
    CodingDetectionBindings {
        priorities,
        systems,
        charset,
    }
}

/// FENCE (peer #816 write-arm): `Place` construction.  Every field explicit;
/// the only cross-crate test construction path.
pub fn place<'a>(
    text: &'a [u8],
    d: usize,
    stop: usize,
    point: usize,
    target_multibyte: bool,
    syntax: MatchSyntax<'a>,
) -> crate::emacs_core::regex_emacs::dfa::Place<'a> {
    crate::emacs_core::regex_emacs::dfa::Place {
        text,
        d,
        stop,
        point,
        target_multibyte,
        syntax,
    }
}

/// FENCE (peer #816 write-arm): `CharsetMapData` construction.  Every field
/// explicit; the only cross-crate test construction path.
pub fn charset_map_data(
    code_to_char: rustc_hash::FxHashMap<i64, i64>,
    char_to_code: rustc_hash::FxHashMap<i64, i64>,
) -> CharsetMapData {
    CharsetMapData {
        code_to_char,
        char_to_code,
    }
}

/// FENCE (peer #816 write-arm): `CharsetMapCacheKey` construction.  Every
/// field explicit; the only cross-crate test construction path.
pub fn charset_map_cache_key(
    map_name: String,
    code_space: [i64; 8],
    min_code: i64,
) -> CharsetMapCacheKey {
    CharsetMapCacheKey {
        map_name,
        code_space,
        min_code,
    }
}

/// FENCE (peer #816 write-arm): set `CharsetInfo::method` in place.
pub fn set_charset_info_method(info: &mut CharsetInfo, value: CharsetMethod) {
    info.method = value;
}

/// FENCE (peer #816 write-arm): set `CharsetInfo::unified_p` in place.
pub fn set_charset_info_unified_p(info: &mut CharsetInfo, value: bool) {
    info.unified_p = value;
}

/// FENCE (peer #816 write-arm): set `CharsetInfo::unify_map` in place.
pub fn set_charset_info_unify_map(info: &mut CharsetInfo, value: Value) {
    info.unify_map = value;
}

/// FENCE (peer #816 write-arm): write one `ExistenceDfa::trans` entry.
pub fn set_existence_dfa_trans_at(dfa: &mut ExistenceDfa, at: usize, value: u32) {
    dfa.trans[at] = value;
}

/// FENCE (VIS read-arm series, lead #1052): set `ExistenceDfa::memory` in
/// place; the field stays a read surface for the consumer.
pub fn set_existence_dfa_memory(dfa: &mut ExistenceDfa, value: usize) {
    dfa.memory = value;
}

/// FENCE (VIS read-arm series, lead #1052): set `ExistenceDfa::plain` in
/// place; the field stays a read surface for the consumer.
pub fn set_existence_dfa_plain(dfa: &mut ExistenceDfa, value: std::ops::Range<usize>) {
    dfa.plain = value;
}

/// FENCE (VIS read-arm series, lead #1052): set `ClassContext::tick` in
/// place; the field stays a read surface for the consumer.
pub fn set_class_context_tick(context: &mut ClassContext, value: u64) {
    context.tick = value;
}

/// FENCE (peer #816 write-arm): set `ExistenceDfa::gave_up` in place.
pub fn set_existence_dfa_gave_up(dfa: &mut ExistenceDfa, value: Option<DfaGaveUp>) {
    dfa.gave_up = value;
}

/// FENCE (peer #816 write-arm): write one `CharClasses::byte_class` entry
/// through the owning `ExistenceDfa` (its `classes` field stays read-only at
/// the consumer).
pub fn set_existence_dfa_byte_class(dfa: &mut ExistenceDfa, byte: usize, value: u8) {
    dfa.classes.byte_class[byte] = value;
}

/// FENCE (peer #816 write-arm): write one `CharClasses::byte_facts` entry
/// through the owning `ExistenceDfa`.
pub fn set_existence_dfa_byte_facts(dfa: &mut ExistenceDfa, byte: usize, value: u8) {
    dfa.classes.byte_facts[byte] = value;
}

/// FENCE (peer #816 write-arm): set `SearchSyntax::read_limit` in place
/// through the owning `ExistenceDfa`.
pub fn set_existence_dfa_search_read_limit(dfa: &mut ExistenceDfa, value: usize) {
    dfa.search.read_limit = value;
}

/// FENCE (peer #816 write-arm): set `SearchSyntax::positional` in place
/// through the owning `ExistenceDfa`.
pub fn set_existence_dfa_search_positional(dfa: &mut ExistenceDfa, value: bool) {
    dfa.search.positional = value;
}

/// FENCE (peer #816 write-arm): sync the DFA's `CharClasses` (an
/// `&mut self` method on the widened `classes` field).
pub fn sync_existence_dfa_classes(dfa: &mut ExistenceDfa, context: ClassContext) {
    dfa.classes.sync(context);
}

/// FENCE (peer #816 write-arm): read the live lease's
/// `LiveDfa::overflow_free_span`; the consumer never borrows `slot`.
pub fn live_dfa_overflow_free_span_for_test(lease: &DfaLease<'_>) -> Option<usize> {
    match &*lease.slot {
        DfaSlot::Live(live) => Some(live.overflow_free_span),
        _ => None,
    }
}

/// FENCE (peer #816 write-arm): set the live lease's
/// `LiveDfa::overflow_free_span`; the consumer never borrows `slot`.
pub fn set_live_dfa_overflow_free_span_for_test(lease: &mut DfaLease<'_>, value: usize) {
    match &mut *lease.slot {
        DfaSlot::Live(live) => live.overflow_free_span = value,
        _ => panic!("compiled DFA slot is not live"),
    }
}

/// FENCE (peer #816 write-arm): read the live lease's `LiveDfa::dfa`
/// counters; the consumer never borrows `slot`.
pub fn live_dfa_counters_for_test(lease: &DfaLease<'_>) -> Option<DfaCounters> {
    match &*lease.slot {
        DfaSlot::Live(live) => Some(live.dfa.counters),
        _ => None,
    }
}

/// FENCE (peer #816 write-arm): set `RunKey::from_char` in place.
pub fn set_run_key_from_char_for_test(key: &mut RunKey, value: usize) {
    key.from_char = value;
}

/// FENCE (peer #816 write-arm): write one `CompiledPattern::buffer` byte.
pub fn set_compiled_pattern_buffer_byte_for_test(
    pattern: &mut CompiledPattern,
    at: usize,
    value: u8,
) {
    pattern.buffer[at] = value;
}

/// FENCE (peer #816 write-arm): mutate `CodingDetectionBindings::priorities`
/// through a scoped borrow; the field stays read-only at the consumer.
pub fn with_coding_detection_priorities_mut_for_test<R>(
    bindings: &mut CodingDetectionBindings,
    f: impl FnOnce(&mut Vec<usize>) -> R,
) -> R {
    bindings.with_priorities_mut_for_test(f)
}

/// FENCE (peer #816 write-arm): run the anchored existence search on the
/// compiled pattern's live DFA slot; the consumer never borrows
/// `CompiledPattern::dfa` or `DfaCell::slot`.
pub fn compiled_pattern_anchored_exists_for_test(
    pattern: &CompiledPattern,
    text: &[u8],
    p: usize,
    stop: usize,
    point: usize,
    syntax: &dyn crate::emacs_core::regex_emacs::SyntaxLookup,
) -> crate::emacs_core::regex_emacs::dfa::Exists {
    let mut slot = pattern.dfa.slot_mut();
    match &mut *slot {
        DfaSlot::Live(live) => live
            .dfa
            .anchored_exists(pattern, text, p, stop, point, syntax),
        _ => panic!("compiled DFA slot is not live"),
    }
}

/// FENCE (peer #816 write-arm): write one `SyntaxPropByteRun::ascii` cell;
/// the array is interior-mutable and stays a read surface for consumers.
pub fn set_syntax_prop_byte_run_ascii_for_test(
    run: &crate::emacs_core::syntax::SyntaxPropByteRun<'_>,
    byte: usize,
    entry: Option<crate::emacs_core::syntax::SyntaxEntry>,
) {
    run.ascii[byte].set(entry);
}

/// FENCE (peer #816 write-arm): write one `SyntaxPropRange::ascii` cell;
/// the array is interior-mutable and stays a read surface for consumers.
pub fn set_syntax_prop_range_ascii_for_test(
    range: &crate::emacs_core::syntax::SyntaxPropRange<'_>,
    byte: usize,
    entry: Option<crate::emacs_core::syntax::SyntaxEntry>,
) {
    range.ascii[byte].set(entry);
}

/// FENCE (VIS read-arm series, lead #1052): set one `CharsetInfo` field in
/// place.  The field stays pub(crate) for this crate; the moved charset
/// fixtures write only through these fences.
pub fn set_charset_info_code_space(info: &mut CharsetInfo, value: [i64; 8]) {
    info.code_space = value;
}
pub fn set_charset_info_id(info: &mut CharsetInfo, value: i64) {
    info.id = value;
}
pub fn set_charset_info_min_code(info: &mut CharsetInfo, value: i64) {
    info.min_code = value;
}
pub fn set_charset_info_max_code(info: &mut CharsetInfo, value: i64) {
    info.max_code = value;
}
pub fn set_charset_info_emacs_mule_id(info: &mut CharsetInfo, value: Option<i64>) {
    info.emacs_mule_id = value;
}
pub fn set_charset_info_name(info: &mut CharsetInfo, value: SymId) {
    info.name = value;
}
pub fn set_charset_info_supplementary_p(info: &mut CharsetInfo, value: bool) {
    info.supplementary_p = value;
}
pub fn set_charset_info_plist(info: &mut CharsetInfo, value: CharsetPlist) {
    info.plist = value;
}
