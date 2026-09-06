use super::*;
use crate::buffer::{
    BufferId, CharLen, CharPos0, CharRange, EmacsBytePos, EmacsByteRange, LispCharPos1,
};
use crate::emacs_core::error::{expect_args, expect_args_range, expect_fixnum, expect_min_args};
use crate::emacs_core::regex::{
    BufferRegexpMatchContext, BufferRegexpSyntaxProperties, MatchDataSource, MatchGroup,
    char_pos_to_byte, char_pos_to_byte_lisp_string,
};
use crate::emacs_core::value::ValueKind;
use strum::IntoStaticStr;

/// GNU search state whose C implementation is held in predeclared `V...`
/// variables rather than looked up by name for every operation.
///
/// The closed enum makes every supported identity explicit. Its exhaustive
/// cache match means a newly added search variable cannot silently fall back
/// to runtime string interning.
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum SearchStateVariable {
    CaseFoldSearch,
    InhibitChangingMatchData,
    CharScriptTable,
    WordCombiningCategories,
    WordSeparatingCategories,
    CaseSymbolsAsWords,
}

impl SearchStateVariable {
    #[inline(always)]
    fn symbol_id(self) -> SymId {
        use std::sync::OnceLock;

        static CASE_FOLD_SEARCH: OnceLock<SymId> = OnceLock::new();
        static INHIBIT_CHANGING_MATCH_DATA: OnceLock<SymId> = OnceLock::new();
        static CHAR_SCRIPT_TABLE: OnceLock<SymId> = OnceLock::new();
        static WORD_COMBINING_CATEGORIES: OnceLock<SymId> = OnceLock::new();
        static WORD_SEPARATING_CATEGORIES: OnceLock<SymId> = OnceLock::new();
        static CASE_SYMBOLS_AS_WORDS: OnceLock<SymId> = OnceLock::new();

        let name: &'static str = self.into();
        match self {
            Self::CaseFoldSearch => *CASE_FOLD_SEARCH.get_or_init(|| intern(name)),
            Self::InhibitChangingMatchData => {
                *INHIBIT_CHANGING_MATCH_DATA.get_or_init(|| intern(name))
            }
            Self::CharScriptTable => *CHAR_SCRIPT_TABLE.get_or_init(|| intern(name)),
            Self::WordCombiningCategories => {
                *WORD_COMBINING_CATEGORIES.get_or_init(|| intern(name))
            }
            Self::WordSeparatingCategories => {
                *WORD_SEPARATING_CATEGORIES.get_or_init(|| intern(name))
            }
            Self::CaseSymbolsAsWords => *CASE_SYMBOLS_AS_WORDS.get_or_init(|| intern(name)),
        }
    }
}

#[inline(always)]
fn dynamic_or_global_symbol_value(
    eval: &super::eval::Context,
    variable: SearchStateVariable,
) -> Option<Value> {
    // GNU reads these via `find_symbol_value`: specials never have a lexenv
    // cell, so skip that probe.
    match eval.find_symbol_value_by_id(variable.symbol_id()) {
        Ok(super::eval::SymbolValueLookup::Bound(value)) => Some(value),
        Ok(super::eval::SymbolValueLookup::Unbound) | Err(_) => None,
    }
}

/// Map a regex front-end error string to its Lisp signal.  Compile
/// errors are `invalid-regexp`; the matcher's fail-stack overflow is a
/// plain `error` in GNU (`search.c:matcher_overflow`: `error ("Stack
/// overflow in regexp matcher")`).
pub(crate) fn regex_error_signal(msg: String) -> crate::emacs_core::error::Flow {
    if msg == crate::emacs_core::regex_emacs::MATCHER_OVERFLOW_MESSAGE {
        signal("error", vec![Value::string(msg)])
    } else {
        signal(LispCondition::InvalidRegexp, vec![Value::string(msg)])
    }
}

// ===========================================================================
// Search / Regex builtins (evaluator-dependent)
// ===========================================================================

/// GNU `search.c:282, 376, 1168, 2053` — every search path reads
/// `Vinhibit_changing_match_data` at the top:
///
///     bool modify_match_data = NILP (Vinhibit_changing_match_data)
///                              && modify_data;
///
/// When the variable is non-nil, the match data must stay pinned to
/// its prior state across the search. Returns `true` when the
/// variable is currently set (i.e. do NOT modify match data).
/// Routes through `dynamic_or_global_symbol_value` so let-bindings
/// and per-buffer overrides are observed, matching the audit #3 fix.
fn read_inhibit_changing_match_data(eval: &super::eval::Context) -> bool {
    dynamic_or_global_symbol_value(eval, SearchStateVariable::InhibitChangingMatchData)
        .is_some_and(|v| !v.is_nil())
}

pub(crate) fn current_word_boundary_lookup(
    eval: &super::eval::Context,
) -> crate::emacs_core::regex_emacs::WordBoundaryLookup {
    crate::emacs_core::regex_emacs::WordBoundaryLookup::new(
        dynamic_or_global_symbol_value(eval, SearchStateVariable::CharScriptTable)
            .filter(|value| !value.is_nil()),
        dynamic_or_global_symbol_value(eval, SearchStateVariable::WordCombiningCategories)
            .unwrap_or(Value::NIL),
        dynamic_or_global_symbol_value(eval, SearchStateVariable::WordSeparatingCategories)
            .unwrap_or(Value::NIL),
    )
}

/// Snapshot the match-time state, borrowing only the obarray so the caller can
/// still take `&mut eval.buffers` for the search itself.
fn current_buffer_regexp_match_context<'a>(
    obarray: &'a crate::emacs_core::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    word_boundary: crate::emacs_core::regex_emacs::WordBoundaryLookup,
    syntax_properties: BufferRegexpSyntaxProperties,
) -> BufferRegexpMatchContext<'a> {
    BufferRegexpMatchContext::new(
        crate::emacs_core::syntax::SyntaxProperties::for_scan(
            syntax_properties.is_honor(),
            obarray,
            buffers,
        ),
        word_boundary,
    )
}

/// The syntax lookup a regexp over STRING runs under: GNU's
/// `RE_SETUP_SYNTAX_TABLE_FOR_OBJECT` for a string object (src/syntax.c:277),
/// with the current buffer's syntax table as the base and the STRING's own
/// intervals supplying each character's positional `syntax-table` property.
fn string_regexp_syntax_lookup<'a>(
    syntax_table: Option<&crate::emacs_core::syntax::SyntaxTable>,
    category_table: Option<Value>,
    word_boundary: crate::emacs_core::regex_emacs::WordBoundaryLookup,
    string: &'a crate::heap_types::LispString,
    syntax_properties: crate::emacs_core::syntax::SyntaxProperties<'a>,
) -> super::regex::StringSyntaxLookup<'a> {
    super::regex::StringSyntaxLookup::new(
        syntax_table.map(
            |syntax_table| crate::emacs_core::regex_emacs::BufferSyntaxLookup {
                syntax_table: *syntax_table,
                category_table,
                word_boundary,
            },
        ),
        string,
        syntax_properties,
    )
}

/// The syntax state GNU's internal `fast_string_match_internal`
/// (`src/search.c:485`) arms before matching: `re_match_object` = the searched
/// string plus `RE_SETUP_SYNTAX_TABLE_FOR_OBJECT` (`src/syntax.c:277`), whose
/// `SETUP_BUFFER_SYNTAX_TABLE` takes the base table -- and the category table
/// and the `parse-sexp-lookup-properties` gate -- from the CURRENT BUFFER,
/// exactly as the Lisp-visible `string-match` does. Its callers are the
/// internal matchers: `completion-regexp-list` filtering
/// (`src/minibuf.c:1592`, `src/dired.c:756`), the `directory-files` MATCH
/// argument (`src/dired.c:311`) and `Ffind_file_name_handler`
/// (`src/fileio.c:411`).
///
/// Owned and `Copy` so a caller snapshots it once and keeps matching between
/// `eval.apply` calls (the completion predicate loop) without holding any
/// borrow of the evaluator. The per-string property resolver is rebuilt from
/// `(obarray, buffers)` at each match, and only when the snapshot's gate was
/// set AND the string actually carries intervals -- a match over a
/// propertyless string runs the position-free path, as GNU's
/// `update_syntax_table` returns early when `interval_of` finds none.
#[derive(Clone, Copy)]
pub(crate) struct FastStringMatchSyntax {
    /// `SETUP_BUFFER_SYNTAX_TABLE`: `None` when there is no current buffer,
    /// which degrades to GNU's standard classification.
    base: Option<crate::emacs_core::regex_emacs::BufferSyntaxLookup>,
    /// The current buffer's `parse-sexp-lookup-properties` at snapshot time.
    honor_properties: bool,
}

impl FastStringMatchSyntax {
    /// Snapshot from the evaluator -- the only constructor, so an internal
    /// string matcher cannot exist without having consulted the current
    /// buffer's tables. Infallible: the sole failure inside
    /// `active_category_table_for_buffer` is the standard category table
    /// failing to bootstrap, and a `None` category table already degrades to
    /// the default classification at match time
    /// (`BufferSyntaxLookup::char_has_category`).
    pub(crate) fn for_current_buffer(eval: &super::eval::Context) -> Self {
        let current_buffer = eval.buffers.current_buffer();
        let base =
            current_buffer.map(
                |buffer| crate::emacs_core::regex_emacs::BufferSyntaxLookup {
                    syntax_table: crate::emacs_core::syntax::SyntaxTable::for_buffer(buffer),
                    category_table: crate::emacs_core::category::active_category_table_for_buffer(
                        current_buffer,
                    )
                    .ok(),
                    word_boundary: current_word_boundary_lookup(eval),
                },
            );
        let honor_properties =
            base.is_some() && crate::emacs_core::syntax::parse_sexp_lookup_properties_enabled(eval);
        Self {
            base,
            honor_properties,
        }
    }

    /// GNU `fast_string_match_internal`'s `re_search` over STRING (always
    /// non-POSIX: `search.c` compiles the pattern with `posix = 0`).
    #[allow(clippy::too_many_arguments)] // matching options stay explicit at the GNU-regexp boundary
    pub(crate) fn search(
        &self,
        obarray: &crate::emacs_core::symbol::Obarray,
        buffers: &crate::buffer::BufferManager,
        pattern: &crate::heap_types::LispString,
        string: &crate::heap_types::LispString,
        searched_string: super::regex::SearchedString,
        start: usize,
        case_fold: bool,
    ) -> Result<Option<super::regex::StringSearchSuccess>, String> {
        // Interval test ahead of the resolver snapshot: a string with no
        // intervals cannot honour anything (see
        // `current_string_match_syntax_properties`).
        let honor = self.honor_properties && !string.intervals().is_empty();
        let properties =
            crate::emacs_core::syntax::SyntaxProperties::for_scan(honor, obarray, buffers);
        let lookup = super::regex::StringSyntaxLookup::new(self.base, string, properties);
        super::regex::string_search_full_with_case_fold_source_lisp_pattern_posix_syntax(
            pattern,
            string,
            searched_string,
            start,
            case_fold,
            false,
            None,
            lookup.as_lookup(),
        )
    }
}

/// Snapshot the string-match property source, borrowing only the obarray.
///
/// The gate is the CURRENT BUFFER's `parse-sexp-lookup-properties`, whatever
/// buffer the string itself came from: GNU tests the C variable mirroring that
/// buffer-local binding at the `RE_SETUP_SYNTAX_TABLE_FOR_OBJECT` site. Unlike
/// a buffer search there is nothing to propertize first -- `syntax-propertize`
/// works on buffers -- so this is a plain read.
/// `searched` is the string-match STRING argument. A string with no intervals
/// has no property to read at any position -- GNU's `update_syntax_table`
/// returns early when `interval_of` finds no interval -- so it is tested first,
/// ahead of the flag: the test is a pointer load on the string, the flag is a
/// dynamic-variable lookup, and a `string-match` over a propertyless string
/// (very nearly all of them) must pay neither that lookup nor the resolver
/// snapshot behind it.
pub(crate) fn current_string_match_syntax_properties<'a>(
    eval: &super::eval::Context,
    obarray: &'a crate::emacs_core::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    searched: Option<&Value>,
) -> crate::emacs_core::syntax::SyntaxProperties<'a> {
    let honor = searched.is_some_and(|value| {
        crate::emacs_core::value::string_has_text_properties_for_value(*value)
    }) && crate::emacs_core::syntax::parse_sexp_lookup_properties_enabled(eval);
    crate::emacs_core::syntax::SyntaxProperties::for_scan(honor, obarray, buffers)
}

fn buffer_byte_to_lisp_char(buf: &crate::buffer::Buffer, byte_pos: EmacsBytePos) -> i64 {
    buf.emacs_byte_pos_to_lisp_char_pos(byte_pos).as_i64()
}

fn match_data_for_explicit_string_arg(md: &super::regex::MatchData) -> super::regex::MatchData {
    super::regex::MatchData::string(md.groups_snapshot(), None)
}

fn buffer_byte_to_char_pos(buf: &crate::buffer::Buffer, byte_pos: EmacsBytePos) -> CharPos0 {
    buf.emacs_byte_pos_to_char_pos_clamped(byte_pos)
}

fn commit_buffer_search_success(
    buffers: &mut crate::buffer::BufferManager,
    success: super::regex::BufferSearchSuccess,
    match_data: Option<&mut Option<super::regex::MatchData>>,
) -> Result<EmacsBytePos, Flow> {
    let (buffer_id, point, published_match_data) = success.into_parts();
    buffers
        .goto_buffer_emacs_byte_pos(buffer_id, point)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    if let Some(match_data) = match_data {
        *match_data = Some(published_match_data);
    }
    Ok(point)
}

pub(crate) fn builtin_search_forward(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("search-forward", &args, 1, 4)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_search_forward_4(eval, arg(0), arg(1), arg(2), arg(3))
}
/// `search-forward` as registered: fixed arity 4, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a4` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
pub(crate) fn builtin_search_forward_4(
    eval: &mut super::eval::Context,
    string: Value,
    bound: Value,
    noerror: Value,
    count: Value,
) -> EvalResult {
    let args: [Value; 4] = [string, bound, noerror, count];
    let case_fold = dynamic_or_global_symbol_value(eval, SearchStateVariable::CaseFoldSearch)
        .map(|v| !v.is_nil())
        .unwrap_or(true);
    let inhibit_changing = read_inhibit_changing_match_data(eval);
    let match_data = (!inhibit_changing).then_some(&mut eval.match_data);
    builtin_search_forward_with_state(case_fold, &mut eval.buffers, match_data, &args)
}

pub(crate) fn builtin_search_forward_with_state(
    case_fold: bool,
    buffers: &mut crate::buffer::BufferManager,
    mut match_data: Option<&mut Option<super::regex::MatchData>>,
    args: &[Value],
) -> EvalResult {
    expect_args_range("search-forward", args, 1, 4)?;
    let pattern = expect_lisp_string(&args[0])?;
    let (current_id, opts, start_pt, start_char) =
        current_search_context_in_manager(buffers, args, SearchKind::ForwardLiteral)?;
    if opts.steps == 0 {
        return Ok(Value::fixnum(start_char));
    }

    let mut last_pos = None;
    for _ in 0..opts.steps {
        let result = {
            let buf = buffers
                .get_mut(current_id)
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            match opts.direction {
                SearchDirection::Forward => super::regex::search_forward(
                    buf,
                    pattern,
                    opts.bound.map(|bound| bound.get()),
                    false,
                    case_fold,
                ),
                SearchDirection::Backward => super::regex::search_backward(
                    buf,
                    pattern,
                    opts.bound.map(|bound| bound.get()),
                    false,
                    case_fold,
                ),
            }
        };
        match result {
            Ok(Some(success)) => {
                last_pos = Some(commit_buffer_search_success(
                    buffers,
                    success,
                    match_data.as_deref_mut(),
                )?)
            }
            Ok(None) => {
                // regex::search_* with `noerror = false` never returns None.
                return Err(signal(LispCondition::SearchFailed, vec![args[0]]));
            }
            Err(_) => {
                return handle_search_failure_in_manager(
                    buffers,
                    current_id,
                    args[0],
                    opts,
                    start_pt,
                    SearchErrorKind::NotFound,
                );
            }
        }
    }

    let end = last_pos.expect("search loop should produce at least one match");
    buffer_byte_to_char_result_in_manager(buffers, current_id, end)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SearchDirection {
    Forward,
    Backward,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SearchNoErrorMode {
    Signal,
    KeepPoint,
    MoveToBound,
}

#[derive(Clone, Copy)]
enum SearchKind {
    ForwardLiteral,
    BackwardLiteral,
    ForwardRegexp,
    BackwardRegexp,
}

#[derive(Clone, Copy)]
enum SearchErrorKind {
    NotFound,
}

#[derive(Clone, Copy)]
struct SearchOptions {
    bound: Option<EmacsBytePos>,
    direction: SearchDirection,
    noerror_mode: SearchNoErrorMode,
    steps: usize,
}

#[derive(Clone, Copy)]
struct SearchBound {
    lisp_pos: LispCharPos1,
    byte_pos: EmacsBytePos,
}

fn search_count_arg(args: &[Value]) -> Result<i64, Flow> {
    match args.get(3) {
        None => Ok(1),
        Some(v) if v.is_nil() => Ok(1),
        Some(v) => match v.kind() {
            ValueKind::Fixnum(n) => Ok(n),
            _ => Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("fixnump"), *v],
            )),
        },
    }
}

fn search_bound_in_manager(
    buffers: &crate::buffer::BufferManager,
    buf: &crate::buffer::Buffer,
    value: &Value,
) -> Result<SearchBound, Flow> {
    let lisp_pos = LispCharPos1::new(super::super::buffer::expect_integer_or_marker_in_buffers(
        buffers, value,
    )?);
    Ok(SearchBound {
        lisp_pos,
        byte_pos: buf.lisp_pos_to_accessible_emacs_byte_pos(lisp_pos),
    })
}

fn parse_search_options_in_manager(
    buffers: &crate::buffer::BufferManager,
    buf: &crate::buffer::Buffer,
    args: &[Value],
    kind: SearchKind,
) -> Result<SearchOptions, Flow> {
    let count = search_count_arg(args)?;
    let noerror_mode = match args.get(2) {
        None => SearchNoErrorMode::Signal,
        Some(v) if v.is_nil() => SearchNoErrorMode::Signal,
        Some(v) if v.is_t() => SearchNoErrorMode::KeepPoint,
        Some(_) => SearchNoErrorMode::MoveToBound,
    };
    let bound = match args.get(1) {
        Some(v) if !v.is_nil() => Some(search_bound_in_manager(buffers, buf, v)?),
        _ => None,
    };

    let direction = match kind {
        SearchKind::ForwardLiteral | SearchKind::ForwardRegexp => {
            if count > 0 {
                SearchDirection::Forward
            } else {
                SearchDirection::Backward
            }
        }
        SearchKind::BackwardLiteral | SearchKind::BackwardRegexp => {
            if count < 0 {
                SearchDirection::Forward
            } else {
                SearchDirection::Backward
            }
        }
    };
    let steps = count.unsigned_abs() as usize;

    if let Some(limit) = bound.map(|bound| bound.lisp_pos.as_i64()) {
        let point_lisp = buffer_byte_to_lisp_char(buf, buf.point_emacs_byte_pos());
        match direction {
            SearchDirection::Forward if limit < point_lisp => {
                return Err(signal(
                    "error",
                    vec![Value::string("Invalid search bound (wrong side of point)")],
                ));
            }
            SearchDirection::Backward if limit > point_lisp => {
                return Err(signal(
                    "error",
                    vec![Value::string("Invalid search bound (wrong side of point)")],
                ));
            }
            _ => {}
        }
    }

    Ok(SearchOptions {
        bound: bound.map(|bound| bound.byte_pos),
        direction,
        noerror_mode,
        steps,
    })
}

fn current_search_context_in_manager(
    buffers: &crate::buffer::BufferManager,
    args: &[Value],
    kind: SearchKind,
) -> Result<(crate::buffer::BufferId, SearchOptions, EmacsBytePos, i64), Flow> {
    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let buf = buffers
        .get(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let opts = parse_search_options_in_manager(buffers, buf, args, kind)?;
    let start_pt = buf.point_emacs_byte_pos();
    let start_char = buffer_byte_to_lisp_char(buf, start_pt);
    Ok((current_id, opts, start_pt, start_char))
}

fn buffer_byte_to_char_result_in_manager(
    buffers: &crate::buffer::BufferManager,
    buffer_id: crate::buffer::BufferId,
    byte: EmacsBytePos,
) -> EvalResult {
    let buf = buffers
        .get(buffer_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::fixnum(buffer_byte_to_lisp_char(buf, byte)))
}

fn search_failure_position(buf: &crate::buffer::Buffer, opts: SearchOptions) -> EmacsBytePos {
    let accessible = buf.accessible_emacs_byte_region();
    match opts.bound {
        Some(limit) => accessible.clamp(limit),
        None => match opts.direction {
            SearchDirection::Forward => accessible.end(),
            SearchDirection::Backward => accessible.start(),
        },
    }
}

fn handle_search_failure_in_manager(
    buffers: &mut crate::buffer::BufferManager,
    buffer_id: crate::buffer::BufferId,
    pattern: Value,
    opts: SearchOptions,
    start_pt: EmacsBytePos,
    kind: SearchErrorKind,
) -> EvalResult {
    match kind {
        SearchErrorKind::NotFound => match opts.noerror_mode {
            SearchNoErrorMode::Signal => {
                let _ = buffers.goto_buffer_emacs_byte_pos(buffer_id, start_pt);
                Err(signal(LispCondition::SearchFailed, vec![pattern]))
            }
            SearchNoErrorMode::KeepPoint => {
                let _ = buffers.goto_buffer_emacs_byte_pos(buffer_id, start_pt);
                Ok(Value::NIL)
            }
            SearchNoErrorMode::MoveToBound => {
                let target = buffers
                    .get(buffer_id)
                    .map(|buf| search_failure_position(buf, opts))
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
                let _ = buffers.goto_buffer_emacs_byte_pos(buffer_id, target);
                Ok(Value::NIL)
            }
        },
    }
}

/// Like [`prepare_current_buffer_regexp_syntax`], but propertizing only up to
/// PATTERN is the Lisp value, not a borrow of its payload, and that is
/// load-bearing (DIVERGENCES.md 163): this runs `syntax-propertize-function`
/// — arbitrary Lisp, a GC safepoint — through `maybe_syntax_propertize_for_scan`.
/// Taking `&LispString` meant every caller held a borrow into the heap across
/// that Lisp call, sound only because the argument list roots the string;
/// taking the `Value` moves the borrow INSIDE, where the compiler can see it
/// ends before the evaluator is used mutably.
///
/// `propertize_target_char` (exclusive-ish; the last position the matcher can
/// examine, plus one). GNU's matcher propertizes LAZILY as it scans
/// (parse_sexp_propertize stops at charpos + 1); neomacs pre-propertizes
/// because its Rust matcher cannot run re-entrant Lisp, so the target must be
/// the SEARCH RANGE end — pre-propertizing to point-max made every bounded
/// syntax-dependent search (looking-back, font-lock anchors) re-propertize
/// the whole buffer tail after each edit flushed syntax-propertize--done:
/// O(buffer) per keystroke. `None` keeps the conservative whole-accessible
/// target (patterns whose scan range is genuinely unbounded).
fn prepare_current_buffer_regexp_syntax_to(
    eval: &mut super::eval::Context,
    pattern: Value,
    case_fold: bool,
    posix: bool,
    propertize_target_char: Option<i64>,
) -> Result<BufferRegexpSyntaxProperties, Flow> {
    prepare_current_buffer_regexp_syntax_to_reporting(
        eval,
        pattern,
        case_fold,
        posix,
        propertize_target_char,
    )
    .map(|(props, _)| props)
}

/// [`prepare_current_buffer_regexp_syntax_to`] that also reports whether the
/// pattern reads buffer syntax at all (the lazy-propertize drivers arm their
/// frontier only then).
fn prepare_current_buffer_regexp_syntax_to_reporting(
    eval: &mut super::eval::Context,
    pattern: Value,
    case_fold: bool,
    posix: bool,
    propertize_target_char: Option<i64>,
) -> Result<(BufferRegexpSyntaxProperties, bool), Flow> {
    prepare_current_buffer_regexp_syntax_to_reporting_compiled(
        eval,
        pattern,
        case_fold,
        posix,
        propertize_target_char,
    )
    .map(|(props, lazy_relevant, _)| (props, lazy_relevant))
}

/// `prepare_current_buffer_regexp_syntax_to_reporting` that also returns the
/// compiled pattern, for callers that match right after (one cache probe,
/// not two).
fn prepare_current_buffer_regexp_syntax_to_reporting_compiled(
    eval: &mut super::eval::Context,
    pattern: Value,
    case_fold: bool,
    posix: bool,
    propertize_target_char: Option<i64>,
) -> Result<
    (
        BufferRegexpSyntaxProperties,
        bool,
        std::rc::Rc<crate::emacs_core::regex_emacs::CompiledPattern>,
    ),
    Flow,
> {
    // The borrow of PATTERN's payload lives and dies inside this block, which
    // is why it may not be a parameter: `maybe_syntax_propertize_for_scan`
    // below runs `syntax-propertize-function`.
    let (dependency, compiled) = {
        let pattern = eval.expect_lisp_string(pattern)?;
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        super::regex::buffer_regexp_syntax_dependency_compiled(buf, pattern, case_fold, posix)
            .map_err(regex_error_signal)?
    };
    let syntax_properties = if crate::emacs_core::syntax::parse_sexp_lookup_properties_enabled(eval)
    {
        BufferRegexpSyntaxProperties::Honor
    } else {
        BufferRegexpSyntaxProperties::Ignore
    };

    let lazy_relevant = dependency.is_buffer_syntax_dependent() && syntax_properties.is_honor();
    if lazy_relevant {
        let accessible_target = eval
            .buffers
            .current_buffer()
            .map(|buf| buf.accessible_char_region().end().get().saturating_add(1))
            .unwrap_or(1);
        let target = match propertize_target_char {
            Some(explicit) => explicit.clamp(1, accessible_target as i64) as usize,
            None => accessible_target,
        };
        crate::emacs_core::syntax::maybe_syntax_propertize_for_scan(eval, target)?;
    }

    Ok((syntax_properties, lazy_relevant, compiled))
}

/// Lazy `syntax-propertize` driver for a point-anchored match (`looking-at`
/// and friends), the neomacs form of GNU's `parse_sexp_propertize`: GNU's
/// `looking_at_1` propertizes `min (zv, 1 + charpos)` at setup and then only
/// as far as the matcher's syntax-reading ops actually advance. Here the
/// first attempt propertizes exactly that one char; the armed
/// [`PropertizeFrontier`] records the first syntax read at or past
/// `syntax-propertize--done`, and the retry propertizes to it (widened
/// geometrically so a long `\s-*` run converges in O(log n) attempts rather
/// than one 500-char chunk per attempt). Before this, `looking-at`
/// propertized to the accessible end: after every edit flushed `--done`,
/// `lisp-indent-line`'s `(looking-at "\\s<\\s<\\s<")` re-propertized the
/// whole buffer tail — O(buffer) per line in comment-region/indent loops.
struct AnchoredPropertize {
    target_lisp: i64,
    point_lisp: i64,
    lookahead: i64,
    attempts: u32,
}

impl AnchoredPropertize {
    const MAX_ATTEMPTS: u32 = 16;

    fn new(buffers: &crate::buffer::BufferManager) -> Self {
        let point_lisp = buffers
            .current_buffer()
            .map(|buf| buf.point_char_pos().get() as i64 + 1)
            .unwrap_or(1);
        Self {
            target_lisp: point_lisp.saturating_add(1),
            point_lisp,
            lookahead: 0,
            attempts: 0,
        }
    }

    /// The frontier to arm for this attempt: the first byte at or past
    /// `syntax-propertize--done`, or `None` when the buffer is propertized
    /// through its accessible end (or propertizing is not in play).
    fn frontier_byte(
        eval: &super::eval::Context,
        lazy_relevant: bool,
    ) -> Option<crate::buffer::EmacsBytePos> {
        if !lazy_relevant {
            return None;
        }
        let done = eval
            .special_variable_value_by_id(crate::emacs_core::syntax::syntax_propertize_done_sym())?
            .as_fixnum()?;
        let buf = eval.buffers.current_buffer()?;
        let accessible_end_lisp = buf.accessible_char_region().end().get() as i64 + 1;
        if done >= accessible_end_lisp {
            return None;
        }
        let begin_lisp = buf.accessible_char_region().start().get() as i64 + 1;
        Some(buf.lisp_pos_to_emacs_byte_pos(crate::buffer::LispCharPos1::new(done.max(begin_lisp))))
    }

    /// The matcher read syntax at `crossed` (past the frontier): choose the
    /// next propertize target. Returns false when no further attempt can
    /// help (no progress possible, or the attempt budget is spent).
    fn advance(
        &mut self,
        buffers: &crate::buffer::BufferManager,
        crossed: crate::buffer::EmacsBytePos,
    ) -> bool {
        self.attempts += 1;
        if self.attempts >= Self::MAX_ATTEMPTS {
            return false;
        }
        let Some(buf) = buffers.current_buffer() else {
            return false;
        };
        let crossed_lisp = buf.emacs_byte_pos_to_char_pos_clamped(crossed).get() as i64 + 1;
        self.lookahead = self.lookahead.saturating_mul(4).max(512);
        let next = crossed_lisp
            .saturating_add(1)
            .max(self.point_lisp.saturating_add(self.lookahead));
        if next <= self.target_lisp {
            return false;
        }
        self.target_lisp = next;
        true
    }
}

/// How a buffer regexp search should obtain its syntax-table properties.
enum RegexpSearchPrep {
    /// `syntax-propertize` already ran far enough; search directly.
    Ready(BufferRegexpSyntaxProperties),
    /// Forward search with a finite per-attempt span: the caller runs the
    /// probe ladder in [`propertize_window_for_forward_regexp`] before the
    /// committed search. `region_end_char` is the search's reachable end
    /// (BOUND when given, else the accessible end), so the ladder's final
    /// rung propertizes exactly what the conservative path always did.
    Windowed {
        syntax_properties: BufferRegexpSyntaxProperties,
        start_char: usize,
        region_end_char: usize,
        margin_chars: usize,
        bound_byte: Option<usize>,
    },
}

fn prepare_buffer_regexp_search(
    eval: &mut super::eval::Context,
    args: &[Value],
    kind: SearchKind,
    case_fold: bool,
    posix: bool,
) -> Result<RegexpSearchPrep, Flow> {
    // GNU `search_command` (src/search.c) runs `CHECK_STRING (string)` before
    // it looks at COUNT, so a non-string pattern signals even when the search
    // would do nothing. Keep that order without carrying the borrow past the
    // `syntax-propertize` below: `let _ =` drops it on this line.
    let _ = eval.expect_lisp_string(args[0])?;
    let (_, opts, _, start_char) = current_search_context_in_manager(&eval.buffers, args, kind)?;
    if opts.steps == 0 {
        return Ok(RegexpSearchPrep::Ready(
            if crate::emacs_core::syntax::parse_sexp_lookup_properties_enabled(eval) {
                BufferRegexpSyntaxProperties::Honor
            } else {
                BufferRegexpSyntaxProperties::Ignore
            },
        ));
    }

    // The matcher's reachable range: a backward search only examines
    // positions at or before the starting point (matches end at or before
    // point), so propertizing through point suffices; a forward search is
    // capped by its BOUND argument when given. `opts.bound` has BOUND
    // already coerced from fixnum OR marker (GNU
    // `CHECK_FIXNUM_COERCE_MARKER`) — newcomment passes `copy-marker`
    // bounds, and reading only fixnums here made every such search
    // propertize to point-max instead of the region end.
    let target = match opts.direction {
        SearchDirection::Backward => Some(start_char.saturating_add(1)),
        SearchDirection::Forward => opts.bound.and_then(|bound| {
            eval.buffers.current_buffer().map(|buf| {
                // 0-based char of the bound, +1 to 1-based, +1 past it.
                buf.emacs_byte_pos_to_char_pos_clamped(bound).get() as i64 + 2
            })
        }),
    };

    // A forward search whose pattern has a finite per-attempt span doesn't
    // need its whole reachable range propertized up front: the probe ladder
    // covers exactly as much as the search examines. This applies bounded
    // searches too — newcomment-style loops pass the region end as BOUND,
    // and re-propertizing edit..BOUND per iteration is the same quadratic.
    if matches!(opts.direction, SearchDirection::Forward)
        && opts.steps == 1
        && crate::emacs_core::syntax::parse_sexp_lookup_properties_enabled(eval)
    {
        // Warm path: `syntax-propertize--done` already covers the whole
        // accessible region (fontified buffer, no edits since), so neither
        // the ladder's probe search nor any propertize call is needed.
        let done = eval
            .special_variable_value_by_id(crate::emacs_core::syntax::syntax_propertize_done_sym())
            .unwrap_or(Value::fixnum(-1));
        let covered = eval
            .buffers
            .current_buffer()
            .map(|buf| buf.accessible_char_region().end().get() as i64 + 1)
            .is_some_and(|accessible_target| {
                // `accessible_target` is the 1-based point-max (end is
                // 0-based); a BOUND caps the needed coverage at bound + 1,
                // exactly the old conservative propertize target.
                let full_target = target
                    .map(|t| t.clamp(1, accessible_target))
                    .unwrap_or(accessible_target);
                matches!(done.kind(), ValueKind::Fixnum(d) if d >= full_target)
            });
        if covered {
            return Ok(RegexpSearchPrep::Ready(BufferRegexpSyntaxProperties::Honor));
        }
        let windowed = {
            let pattern = eval.expect_lisp_string(args[0])?;
            let buf = eval
                .buffers
                .current_buffer()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            let (dependency, span) = super::regex::buffer_regexp_syntax_dependency_and_span(
                buf, pattern, case_fold, posix,
            )
            .map_err(regex_error_signal)?;
            (dependency.is_buffer_syntax_dependent())
                .then_some(span)
                .flatten()
                .map(|span| {
                    let accessible_end = buf.accessible_char_region().end().get();
                    // With a BOUND, the reachable end is the bound (target
                    // holds bound + 1), clamped like the conservative path.
                    let region_end_char = target
                        .map(|t| ((t - 1).max(1) as usize).min(accessible_end))
                        .unwrap_or(accessible_end);
                    (
                        start_char.max(1) as usize,
                        region_end_char,
                        span.saturating_add(2),
                    )
                })
        };
        if let Some((start_char, region_end_char, margin_chars)) = windowed {
            return Ok(RegexpSearchPrep::Windowed {
                syntax_properties: BufferRegexpSyntaxProperties::Honor,
                start_char,
                region_end_char,
                margin_chars,
                bound_byte: opts.bound.map(|bound| bound.get()),
            });
        }
    }

    prepare_current_buffer_regexp_syntax_to(eval, args[0], case_fold, posix, target)
        .map(RegexpSearchPrep::Ready)
}

/// Unpack a [`RegexpSearchPrep`], running the probe ladder for the
/// windowed case so the committed search reads only propertized text.
fn resolve_regexp_search_prep(
    eval: &mut super::eval::Context,
    args: &[Value],
    case_fold: bool,
    posix: bool,
    prep: RegexpSearchPrep,
) -> Result<BufferRegexpSyntaxProperties, Flow> {
    match prep {
        RegexpSearchPrep::Ready(syntax_properties) => Ok(syntax_properties),
        RegexpSearchPrep::Windowed {
            syntax_properties,
            start_char,
            region_end_char,
            margin_chars,
            bound_byte,
        } => {
            propertize_window_for_forward_regexp(
                eval,
                args,
                case_fold,
                posix,
                syntax_properties,
                start_char,
                region_end_char,
                margin_chars,
                bound_byte,
            )?;
            Ok(syntax_properties)
        }
    }
}

/// Propertize just enough of the buffer for an unbounded forward regexp
/// search whose pattern has a finite per-attempt span (`margin_chars` =
/// that span + 2). GNU's matcher extends `syntax-propertize--done` lazily
/// as it advances (`update_syntax_table_forward` →
/// `parse_sexp_propertize`, which stops at charpos + 1); our matcher
/// cannot run Lisp mid-match, and propertizing to point-max on every
/// search made each syntax-dependent search after a buffer edit re-run
/// `syntax-propertize` over the whole tail — quadratic under
/// search/replace loops.
///
/// Ladder: propertize a window, run a pure probe through the same
/// searcher core the committed search uses (no point/match-data commit),
/// and accept once the found match end clears the window frontier by
/// `margin_chars`. Every attempt at or before the found start examines at
/// most its start + span + 1 positions, all inside the window, so the
/// committed re-run reads only propertized text and returns the identical
/// result. Failures widen the window ×4 until it covers the region — at
/// which point behavior equals the old conservative full propertize.
fn propertize_window_for_forward_regexp(
    eval: &mut super::eval::Context,
    args: &[Value],
    case_fold: bool,
    posix: bool,
    syntax_properties: BufferRegexpSyntaxProperties,
    start_char: usize,
    region_end_char: usize,
    margin_chars: usize,
    bound_byte: Option<usize>,
) -> Result<(), Flow> {
    // Bounded searches come from region loops (newcomment et al.) whose
    // matches sit within a line or two; start them on a small window so a
    // per-edit retreat of `--done` re-propertizes only that much.
    let base = if bound_byte.is_some() {
        512usize
    } else {
        4096usize
    };
    let mut lookahead = base.max(margin_chars.saturating_mul(4));
    loop {
        let window_end = start_char.saturating_add(lookahead).min(region_end_char);
        crate::emacs_core::syntax::maybe_syntax_propertize_for_scan(
            eval,
            window_end.saturating_add(1),
        )?;
        if window_end >= region_end_char {
            return Ok(());
        }
        let match_context = current_buffer_regexp_match_context(
            &eval.obarray,
            &eval.buffers,
            current_word_boundary_lookup(eval),
            syntax_properties,
        );
        let probe = {
            let pattern = expect_lisp_string(&args[0])?;
            let Some(buf) = eval.buffers.current_buffer_mut() else {
                return Ok(());
            };
            super::regex::re_search_forward_lisp_with_posix(
                buf,
                pattern,
                bound_byte,
                false,
                case_fold,
                posix,
                match_context,
            )
        };
        match probe {
            Ok(Some(success)) => {
                let (_, end_byte, _) = success.into_parts();
                let end_char = eval
                    .buffers
                    .current_buffer()
                    .map(|buf| buf.emacs_byte_pos_to_char_pos_clamped(end_byte).get())
                    .unwrap_or(usize::MAX);
                if end_char.saturating_add(margin_chars) <= window_end {
                    return Ok(());
                }
            }
            // A real regexp error reproduces in the committed search; give
            // it full coverage so its semantics match the old path exactly.
            Err(msg) if msg != "Search failed" => {
                crate::emacs_core::syntax::maybe_syntax_propertize_for_scan(
                    eval,
                    region_end_char.saturating_add(1),
                )?;
                return Ok(());
            }
            Ok(None) | Err(_) => {}
        }
        lookahead = lookahead.saturating_mul(4);
    }
}

pub(crate) fn builtin_search_backward(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let case_fold = dynamic_or_global_symbol_value(eval, SearchStateVariable::CaseFoldSearch)
        .map(|v| !v.is_nil())
        .unwrap_or(true);
    let inhibit_changing = read_inhibit_changing_match_data(eval);
    let match_data = (!inhibit_changing).then_some(&mut eval.match_data);
    builtin_search_backward_with_state(case_fold, &mut eval.buffers, match_data, &args)
}

pub(crate) fn builtin_search_backward_with_state(
    case_fold: bool,
    buffers: &mut crate::buffer::BufferManager,
    mut match_data: Option<&mut Option<super::regex::MatchData>>,
    args: &[Value],
) -> EvalResult {
    expect_args_range("search-backward", args, 1, 4)?;
    let pattern = expect_lisp_string(&args[0])?;
    let (current_id, opts, start_pt, start_char) =
        current_search_context_in_manager(buffers, args, SearchKind::BackwardLiteral)?;
    if opts.steps == 0 {
        return Ok(Value::fixnum(start_char));
    }

    let mut last_pos = None;
    for _ in 0..opts.steps {
        let result = {
            let buf = buffers
                .get_mut(current_id)
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            match opts.direction {
                SearchDirection::Forward => super::regex::search_forward(
                    buf,
                    pattern,
                    opts.bound.map(|bound| bound.get()),
                    false,
                    case_fold,
                ),
                SearchDirection::Backward => super::regex::search_backward(
                    buf,
                    pattern,
                    opts.bound.map(|bound| bound.get()),
                    false,
                    case_fold,
                ),
            }
        };
        match result {
            Ok(Some(success)) => {
                last_pos = Some(commit_buffer_search_success(
                    buffers,
                    success,
                    match_data.as_deref_mut(),
                )?)
            }
            Ok(None) => {
                return Err(signal(LispCondition::SearchFailed, vec![args[0]]));
            }
            Err(_) => {
                return handle_search_failure_in_manager(
                    buffers,
                    current_id,
                    args[0],
                    opts,
                    start_pt,
                    SearchErrorKind::NotFound,
                );
            }
        }
    }

    let end = last_pos.expect("search loop should produce at least one match");
    buffer_byte_to_char_result_in_manager(buffers, current_id, end)
}

pub(crate) fn builtin_re_search_forward(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("re-search-forward", &args, 1, 4)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_re_search_forward_4(eval, arg(0), arg(1), arg(2), arg(3))
}
/// `re-search-forward` as registered: fixed arity 4, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a4` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
pub(crate) fn builtin_re_search_forward_4(
    eval: &mut super::eval::Context,
    regexp: Value,
    bound: Value,
    noerror: Value,
    count: Value,
) -> EvalResult {
    let args: [Value; 4] = [regexp, bound, noerror, count];
    expect_args_range("re-search-forward", &args, 1, 4)?;
    let case_fold = dynamic_or_global_symbol_value(eval, SearchStateVariable::CaseFoldSearch)
        .map(|v| !v.is_nil())
        .unwrap_or(true);
    let prep =
        prepare_buffer_regexp_search(eval, &args, SearchKind::ForwardRegexp, case_fold, false)?;
    let syntax_properties = resolve_regexp_search_prep(eval, &args, case_fold, false, prep)?;
    // Compile once here (GNU `search_command` -> `compile_pattern` once) so
    // the word-boundary tables are read only for syntax-dependent patterns
    // and the search below does not probe the pattern cache again.
    let compiled = {
        let pattern = eval.expect_lisp_string(args[0])?;
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        super::regex::buffer_regexp_syntax_dependency_compiled(buf, pattern, case_fold, false)
            .map_err(regex_error_signal)?
            .1
    };
    let word_boundary = if compiled.uses_syntax {
        current_word_boundary_lookup(eval)
    } else {
        crate::emacs_core::regex_emacs::WordBoundaryLookup::default()
    };
    let match_context = current_buffer_regexp_match_context(
        &eval.obarray,
        &eval.buffers,
        word_boundary,
        syntax_properties,
    );
    let inhibit_changing = read_inhibit_changing_match_data(eval);
    let match_data = (!inhibit_changing).then_some(&mut eval.match_data);
    let result = re_search_forward_with_state_posix_and_syntax_properties(
        case_fold,
        false,
        match_context,
        &mut eval.buffers,
        match_data,
        &args,
        Some(&compiled),
    );
    // Mirrors GNU `search.c:1247,1291`: poll quit after each search
    // call so a `C-g` that set `tls_quit_pending()` during the match
    // surfaces as a `quit` signal rather than being interpreted as
    // `search-failed`. The matcher itself returned None on the TLS
    // flag; here we promote it.
    eval.maybe_quit()?;
    result
}

/// Shared body for `re-search-forward` and `posix-search-forward`.
/// When `posix` is true, the matcher runs the GNU POSIX longest-match
/// algorithm (regex-emacs.c:4143-4344). See audit #2 in
/// `drafts/regex-search-audit.md`; before this fix the posix builtins
/// were silent aliases.
fn re_search_forward_with_state_posix_and_syntax_properties(
    case_fold: bool,
    posix: bool,
    match_context: BufferRegexpMatchContext<'_>,
    buffers: &mut crate::buffer::BufferManager,
    mut match_data: Option<&mut Option<super::regex::MatchData>>,
    args: &[Value],
    compiled: Option<&crate::emacs_core::regex_emacs::CompiledPattern>,
) -> EvalResult {
    let name = if posix {
        "posix-search-forward"
    } else {
        "re-search-forward"
    };
    expect_args_range(name, args, 1, 4)?;
    let pattern = expect_lisp_string(&args[0])?;
    let (current_id, opts, start_pt, start_char) =
        current_search_context_in_manager(buffers, args, SearchKind::ForwardRegexp)?;
    if opts.steps == 0 {
        return Ok(Value::fixnum(start_char));
    }

    let mut last_pos = None;
    for _ in 0..opts.steps {
        let result = {
            let buf = buffers
                .get_mut(current_id)
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            match opts.direction {
                SearchDirection::Forward => match compiled {
                    Some(compiled) => super::regex::re_search_forward_compiled(
                        buf,
                        compiled,
                        opts.bound.map(|bound| bound.get()),
                        false,
                        match_context,
                    ),
                    None => super::regex::re_search_forward_lisp_with_posix(
                        buf,
                        pattern,
                        opts.bound.map(|bound| bound.get()),
                        false,
                        case_fold,
                        posix,
                        match_context,
                    ),
                },
                SearchDirection::Backward => super::regex::re_search_backward_lisp_with_posix(
                    buf,
                    pattern,
                    opts.bound.map(|bound| bound.get()),
                    false,
                    case_fold,
                    posix,
                    match_context,
                ),
            }
        };

        match result {
            Ok(Some(success)) => {
                last_pos = Some(commit_buffer_search_success(
                    buffers,
                    success,
                    match_data.as_deref_mut(),
                )?)
            }
            Ok(None) => {
                return Err(signal(LispCondition::SearchFailed, vec![args[0]]));
            }
            Err(msg) if msg != "Search failed" => {
                let _ = buffers.goto_buffer_emacs_byte_pos(current_id, start_pt);
                return Err(regex_error_signal(msg));
            }
            Err(_) => {
                return handle_search_failure_in_manager(
                    buffers,
                    current_id,
                    args[0],
                    opts,
                    start_pt,
                    SearchErrorKind::NotFound,
                );
            }
        }
    }

    let end = last_pos.expect("search loop should produce at least one match");
    buffer_byte_to_char_result_in_manager(buffers, current_id, end)
}

pub(crate) fn builtin_re_search_backward(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("re-search-backward", &args, 1, 4)?;
    let case_fold = dynamic_or_global_symbol_value(eval, SearchStateVariable::CaseFoldSearch)
        .map(|v| !v.is_nil())
        .unwrap_or(true);
    let prep =
        prepare_buffer_regexp_search(eval, &args, SearchKind::BackwardRegexp, case_fold, false)?;
    let syntax_properties = resolve_regexp_search_prep(eval, &args, case_fold, false, prep)?;
    let match_context = current_buffer_regexp_match_context(
        &eval.obarray,
        &eval.buffers,
        current_word_boundary_lookup(eval),
        syntax_properties,
    );
    let inhibit_changing = read_inhibit_changing_match_data(eval);
    let match_data = (!inhibit_changing).then_some(&mut eval.match_data);
    let result = re_search_backward_with_state_posix_and_syntax_properties(
        case_fold,
        false,
        match_context,
        &mut eval.buffers,
        match_data,
        &args,
    );
    // See `builtin_re_search_forward`: promote a TLS-detected quit.
    eval.maybe_quit()?;
    result
}

/// Shared body for `re-search-backward` and `posix-search-backward`.
/// See [`re_search_forward_with_state_posix_and_syntax_properties`] for the POSIX longest-
/// match rationale (audit #2).
fn re_search_backward_with_state_posix_and_syntax_properties(
    case_fold: bool,
    posix: bool,
    match_context: BufferRegexpMatchContext<'_>,
    buffers: &mut crate::buffer::BufferManager,
    mut match_data: Option<&mut Option<super::regex::MatchData>>,
    args: &[Value],
) -> EvalResult {
    let name = if posix {
        "posix-search-backward"
    } else {
        "re-search-backward"
    };
    expect_args_range(name, args, 1, 4)?;
    let pattern = expect_lisp_string(&args[0])?;
    let (current_id, opts, start_pt, start_char) =
        current_search_context_in_manager(buffers, args, SearchKind::BackwardRegexp)?;
    if opts.steps == 0 {
        return Ok(Value::fixnum(start_char));
    }

    let mut last_pos = None;
    for _ in 0..opts.steps {
        let result = {
            let buf = buffers
                .get_mut(current_id)
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            match opts.direction {
                SearchDirection::Forward => super::regex::re_search_forward_lisp_with_posix(
                    buf,
                    pattern,
                    opts.bound.map(|bound| bound.get()),
                    false,
                    case_fold,
                    posix,
                    match_context,
                ),
                SearchDirection::Backward => super::regex::re_search_backward_lisp_with_posix(
                    buf,
                    pattern,
                    opts.bound.map(|bound| bound.get()),
                    false,
                    case_fold,
                    posix,
                    match_context,
                ),
            }
        };

        match result {
            Ok(Some(success)) => {
                last_pos = Some(commit_buffer_search_success(
                    buffers,
                    success,
                    match_data.as_deref_mut(),
                )?)
            }
            Ok(None) => {
                return Err(signal(LispCondition::SearchFailed, vec![args[0]]));
            }
            Err(msg) if msg != "Search failed" => {
                let _ = buffers.goto_buffer_emacs_byte_pos(current_id, start_pt);
                return Err(regex_error_signal(msg));
            }
            Err(_) => {
                return handle_search_failure_in_manager(
                    buffers,
                    current_id,
                    args[0],
                    opts,
                    start_pt,
                    SearchErrorKind::NotFound,
                );
            }
        }
    }

    let end = last_pos.expect("search loop should produce at least one match");
    buffer_byte_to_char_result_in_manager(buffers, current_id, end)
}

pub(crate) fn builtin_posix_search_forward(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("posix-search-forward", &args, 1, 4)?;
    let case_fold = dynamic_or_global_symbol_value(eval, SearchStateVariable::CaseFoldSearch)
        .map(|v| !v.is_nil())
        .unwrap_or(true);
    let prep =
        prepare_buffer_regexp_search(eval, &args, SearchKind::ForwardRegexp, case_fold, true)?;
    let syntax_properties = resolve_regexp_search_prep(eval, &args, case_fold, true, prep)?;
    let match_context = current_buffer_regexp_match_context(
        &eval.obarray,
        &eval.buffers,
        current_word_boundary_lookup(eval),
        syntax_properties,
    );
    let inhibit_changing = read_inhibit_changing_match_data(eval);
    let match_data = (!inhibit_changing).then_some(&mut eval.match_data);
    re_search_forward_with_state_posix_and_syntax_properties(
        case_fold,
        true,
        match_context,
        &mut eval.buffers,
        match_data,
        &args,
        None,
    )
}

pub(crate) fn builtin_posix_search_backward(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("posix-search-backward", &args, 1, 4)?;
    let case_fold = dynamic_or_global_symbol_value(eval, SearchStateVariable::CaseFoldSearch)
        .map(|v| !v.is_nil())
        .unwrap_or(true);
    let prep =
        prepare_buffer_regexp_search(eval, &args, SearchKind::BackwardRegexp, case_fold, true)?;
    let syntax_properties = resolve_regexp_search_prep(eval, &args, case_fold, true, prep)?;
    let match_context = current_buffer_regexp_match_context(
        &eval.obarray,
        &eval.buffers,
        current_word_boundary_lookup(eval),
        syntax_properties,
    );
    let inhibit_changing = read_inhibit_changing_match_data(eval);
    let match_data = (!inhibit_changing).then_some(&mut eval.match_data);
    re_search_backward_with_state_posix_and_syntax_properties(
        case_fold,
        true,
        match_context,
        &mut eval.buffers,
        match_data,
        &args,
    )
}

pub(crate) fn builtin_looking_at(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("looking-at", &args, 1, 2)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_looking_at_2(eval, arg(0), arg(1))
}
/// `looking-at` as registered: fixed arity 2, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a2` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
pub(crate) fn builtin_looking_at_2(
    eval: &mut super::eval::Context,
    regexp: Value,
    inhibit_modify: Value,
) -> EvalResult {
    let args: [Value; 2] = [regexp, inhibit_modify];
    expect_args_range("looking-at", &args, 1, 2)?;
    let case_fold = dynamic_or_global_symbol_value(eval, SearchStateVariable::CaseFoldSearch)
        .map(|v| !v.is_nil())
        .unwrap_or(true);
    let inhibit_changing = read_inhibit_changing_match_data(eval);
    let mut lazy = AnchoredPropertize::new(&eval.buffers);
    loop {
        let (syntax_properties, lazy_relevant, compiled) =
            prepare_current_buffer_regexp_syntax_to_reporting_compiled(
                eval,
                args[0],
                case_fold,
                false,
                Some(lazy.target_lisp),
            )?;
        let crossed = std::cell::Cell::new(None);
        let frontier = AnchoredPropertize::frontier_byte(eval, lazy_relevant).map(|byte| {
            super::regex::PropertizeFrontier {
                byte,
                crossed: &crossed,
            }
        });
        // The word-boundary tables (`char-script-table`,
        // `word-combining-categories`, `word-separating-categories`) are read
        // by the matcher only at syntax-dependent ops (GNU reads
        // `Vchar_script_table` inside `wordbound`); a pattern without any
        // needs no lookup -- three variable reads per call otherwise.
        let word_boundary = if compiled.uses_syntax {
            current_word_boundary_lookup(eval)
        } else {
            crate::emacs_core::regex_emacs::WordBoundaryLookup::default()
        };
        let mut match_context = current_buffer_regexp_match_context(
            &eval.obarray,
            &eval.buffers,
            word_boundary,
            syntax_properties,
        );
        if let Some(frontier) = frontier {
            match_context = match_context.with_frontier(frontier);
        }
        let match_data = (!inhibit_changing).then_some(&mut eval.match_data);
        let result = builtin_looking_at_with_state_and_syntax_properties(
            match_context,
            &eval.buffers,
            match_data,
            &args,
            &compiled,
        );
        if let Some(byte) = crossed.get()
            && lazy.advance(&eval.buffers, byte)
        {
            continue;
        }
        // Promote a TLS-detected quit to a `quit` signal (see
        // `builtin_re_search_forward`).
        eval.maybe_quit()?;
        return result;
    }
}

fn builtin_looking_at_with_state_and_syntax_properties(
    match_context: BufferRegexpMatchContext<'_>,
    buffers: &crate::buffer::BufferManager,
    match_data: Option<&mut Option<super::regex::MatchData>>,
    args: &[Value],
    compiled: &crate::emacs_core::regex_emacs::CompiledPattern,
) -> EvalResult {
    expect_args_range("looking-at", args, 1, 2)?;
    let inhibit_modify = args.get(1).is_some_and(|arg| !arg.is_nil());

    let buf = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let result = super::regex::looking_at_compiled(buf, compiled, match_context);

    match result {
        Ok(published_match_data) => {
            let matched = published_match_data.is_some();
            if !inhibit_modify
                && let (Some(match_data), Some(published_match_data)) =
                    (match_data, published_match_data)
            {
                *match_data = Some(published_match_data);
            }
            Ok(Value::bool_val(matched))
        }
        Err(msg) => Err(regex_error_signal(msg)),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_looking_at_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("looking-at-p", &args, 1)?;
    let case_fold = dynamic_or_global_symbol_value(eval, SearchStateVariable::CaseFoldSearch)
        .map(|v| !v.is_nil())
        .unwrap_or(true);
    let mut lazy = AnchoredPropertize::new(&eval.buffers);
    loop {
        let (syntax_properties, lazy_relevant) = prepare_current_buffer_regexp_syntax_to_reporting(
            eval,
            args[0],
            case_fold,
            false,
            Some(lazy.target_lisp),
        )?;
        let crossed = std::cell::Cell::new(None);
        let frontier = AnchoredPropertize::frontier_byte(eval, lazy_relevant).map(|byte| {
            super::regex::PropertizeFrontier {
                byte,
                crossed: &crossed,
            }
        });
        let mut match_context = current_buffer_regexp_match_context(
            &eval.obarray,
            &eval.buffers,
            current_word_boundary_lookup(eval),
            syntax_properties,
        );
        if let Some(frontier) = frontier {
            match_context = match_context.with_frontier(frontier);
        }
        let result = builtin_looking_at_p_with_state_and_syntax_properties(
            case_fold,
            match_context,
            &eval.buffers,
            &args,
        );
        if let Some(byte) = crossed.get()
            && lazy.advance(&eval.buffers, byte)
        {
            continue;
        }
        return result;
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_looking_at_p_with_state(
    case_fold: bool,
    buffers: &crate::buffer::BufferManager,
    args: &[Value],
) -> EvalResult {
    builtin_looking_at_p_with_state_and_syntax_properties(
        case_fold,
        BufferRegexpMatchContext::new(
            crate::emacs_core::syntax::SyntaxProperties::Ignore,
            crate::emacs_core::regex_emacs::WordBoundaryLookup::default(),
        ),
        buffers,
        args,
    )
}

fn builtin_looking_at_p_with_state_and_syntax_properties(
    case_fold: bool,
    match_context: BufferRegexpMatchContext<'_>,
    buffers: &crate::buffer::BufferManager,
    args: &[Value],
) -> EvalResult {
    expect_args("looking-at-p", args, 1)?;
    let pattern = expect_lisp_string(&args[0])?;

    let buf = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    match super::regex::looking_at_lisp_with_posix(buf, pattern, case_fold, false, match_context) {
        Ok(published_match_data) => Ok(Value::bool_val(published_match_data.is_some())),
        Err(msg) => Err(regex_error_signal(msg)),
    }
}

pub(crate) fn builtin_posix_looking_at(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("posix-looking-at", &args, 1, 2)?;
    let case_fold = dynamic_or_global_symbol_value(eval, SearchStateVariable::CaseFoldSearch)
        .map(|v| !v.is_nil())
        .unwrap_or(true);
    let inhibit_changing = read_inhibit_changing_match_data(eval);
    let mut lazy = AnchoredPropertize::new(&eval.buffers);
    loop {
        let (syntax_properties, lazy_relevant) = prepare_current_buffer_regexp_syntax_to_reporting(
            eval,
            args[0],
            case_fold,
            true,
            Some(lazy.target_lisp),
        )?;
        let crossed = std::cell::Cell::new(None);
        let frontier = AnchoredPropertize::frontier_byte(eval, lazy_relevant).map(|byte| {
            super::regex::PropertizeFrontier {
                byte,
                crossed: &crossed,
            }
        });
        let mut match_context = current_buffer_regexp_match_context(
            &eval.obarray,
            &eval.buffers,
            current_word_boundary_lookup(eval),
            syntax_properties,
        );
        if let Some(frontier) = frontier {
            match_context = match_context.with_frontier(frontier);
        }
        let match_data = (!inhibit_changing).then_some(&mut eval.match_data);
        let result = builtin_posix_looking_at_with_state_and_syntax_properties(
            case_fold,
            match_context,
            &eval.buffers,
            match_data,
            &args,
        );
        if let Some(byte) = crossed.get()
            && lazy.advance(&eval.buffers, byte)
        {
            continue;
        }
        return result;
    }
}

fn builtin_posix_looking_at_with_state_and_syntax_properties(
    case_fold: bool,
    match_context: BufferRegexpMatchContext<'_>,
    buffers: &crate::buffer::BufferManager,
    match_data: Option<&mut Option<super::regex::MatchData>>,
    args: &[Value],
) -> EvalResult {
    // GNU `src/search.c:Fposix_looking_at` calls `looking_at_1`
    // with `posix = 1`, which threads into `compile_pattern` and
    // ultimately into `re_match_2_internal` to enable POSIX
    // longest-match (regex-emacs.c:4143-4344). See audit #2 in
    // `drafts/regex-search-audit.md`; this wrapper used to be a
    // silent alias for `looking-at`.
    expect_args_range("posix-looking-at", args, 1, 2)?;
    let pattern = expect_lisp_string(&args[0])?;
    let inhibit_modify = args.get(1).is_some_and(|arg| !arg.is_nil());

    let buf = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let result =
        super::regex::looking_at_lisp_with_posix(buf, pattern, case_fold, true, match_context);

    match result {
        Ok(published_match_data) => {
            let matched = published_match_data.is_some();
            if !inhibit_modify
                && let (Some(match_data), Some(published_match_data)) =
                    (match_data, published_match_data)
            {
                *match_data = Some(published_match_data);
            }
            Ok(Value::bool_val(matched))
        }
        Err(msg) => Err(regex_error_signal(msg)),
    }
}

fn commit_string_search_success(
    result: Result<Option<super::regex::StringSearchSuccess>, String>,
    match_data: Option<&mut Option<super::regex::MatchData>>,
) -> EvalResult {
    match result {
        Ok(Some(success)) => {
            let (start, published_match_data) = success.into_parts();
            if let Some(match_data) = match_data {
                *match_data = Some(published_match_data);
            }
            Ok(Value::fixnum(start.get() as i64))
        }
        Ok(None) => Ok(Value::NIL),
        Err(msg) => Err(regex_error_signal(msg)),
    }
}

#[allow(clippy::too_many_arguments)] // match-time Lisp state stays explicit at this seam
pub(crate) fn builtin_string_match_with_state(
    case_fold: bool,
    case_translation_table: Option<Value>,
    syntax_table: Option<&crate::emacs_core::syntax::SyntaxTable>,
    category_table: Option<Value>,
    word_boundary: crate::emacs_core::regex_emacs::WordBoundaryLookup,
    syntax_properties: crate::emacs_core::syntax::SyntaxProperties<'_>,
    match_data: Option<&mut Option<super::regex::MatchData>>,
    args: &[Value],
) -> EvalResult {
    crate::emacs_core::perf_trace::time_op(
        crate::emacs_core::perf_trace::HotpathOp::StringMatch,
        || {
            expect_args_range("string-match", args, 2, 4)?;
            let inhibit_modify = args.get(3).is_some_and(|v| v.is_truthy());

            match (args[0].kind(), args[1].kind()) {
                (ValueKind::String, ValueKind::String) => {
                    let pattern = expect_lisp_string(&args[0])?;
                    let string = args[1].as_lisp_string().unwrap();
                    let start = crate::emacs_core::search::normalize_lisp_string_start_arg(
                        string,
                        args.get(2),
                    )?;
                    let string_syntax = string_regexp_syntax_lookup(
                        syntax_table,
                        category_table,
                        word_boundary,
                        string,
                        syntax_properties,
                    );
                    let syntax = string_syntax.as_lookup();
                    let result = super::regex::string_search_full_with_case_fold_source_lisp_pattern_posix_syntax(
                        pattern,
                        string,
                        super::regex::SearchedString::Heap(args[1]),
                        start,
                        case_fold,
                        false,
                        case_translation_table,
                        syntax,
                    );
                    let target = if inhibit_modify { None } else { match_data };
                    commit_string_search_success(result, target)
                }
                _ => {
                    let pattern = expect_string_lossy(&args[0])?;
                    let s = expect_string_lossy(&args[1])?;
                    let start = normalize_string_start_arg(&s, args.get(2))?;
                    let result = super::regex::string_search_full_with_case_fold_and_posix(
                        &pattern, &s, start, case_fold, false,
                    );
                    let target = if inhibit_modify { None } else { match_data };
                    commit_string_search_success(result, target)
                }
            }
        },
    )
}

/// Context-dependent `string-match`: updates match data on the evaluator.
pub(crate) fn builtin_string_match(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_string_match_slice(eval, &args)
}

pub(crate) fn builtin_string_match_slice(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    let case_fold = dynamic_or_global_symbol_value(eval, SearchStateVariable::CaseFoldSearch)
        .map(|v| !v.is_nil())
        .unwrap_or(true);
    // The canon TABLE only: regexp compile builds (and caches) the actual
    // CaseTranslation; constructing its 1KB memo per call was pure waste.
    let case_translation_table = if case_fold {
        Some(crate::emacs_core::casetab::current_case_canon_table(eval)?)
    } else {
        None
    };
    let current_buffer = eval.buffers.current_buffer();
    let syntax_table = current_buffer.map(crate::emacs_core::syntax::SyntaxTable::for_buffer);
    let category_table =
        Some(crate::emacs_core::category::active_category_table_for_buffer(current_buffer)?);
    let word_boundary = current_word_boundary_lookup(eval);
    let inhibit_changing = read_inhibit_changing_match_data(eval);
    let syntax_properties =
        current_string_match_syntax_properties(eval, &eval.obarray, &eval.buffers, args.get(1));
    let match_data = (!inhibit_changing).then_some(&mut eval.match_data);
    let result = builtin_string_match_with_state(
        case_fold,
        case_translation_table,
        syntax_table.as_ref(),
        category_table,
        word_boundary,
        syntax_properties,
        match_data,
        args,
    );
    // Promote a TLS-detected quit (see `builtin_re_search_forward`).
    eval.maybe_quit()?;
    result
}

#[allow(clippy::too_many_arguments)] // match-time Lisp state stays explicit at this seam
pub(crate) fn builtin_posix_string_match_with_state(
    case_fold: bool,
    case_translation_table: Option<Value>,
    syntax_table: Option<&crate::emacs_core::syntax::SyntaxTable>,
    category_table: Option<Value>,
    word_boundary: crate::emacs_core::regex_emacs::WordBoundaryLookup,
    syntax_properties: crate::emacs_core::syntax::SyntaxProperties<'_>,
    match_data: Option<&mut Option<super::regex::MatchData>>,
    args: &[Value],
) -> EvalResult {
    // GNU `src/search.c:Fposix_string_match` calls `string_match_1`
    // with `posix = 1`. Before this fix the neomacs builtin was a
    // silent alias for `string-match` (audit #2). We duplicate the
    // body of `builtin_string_match_with_state` and route through
    // the `*_posix` compile helpers so `CompiledPattern::posix` is
    // set for the matcher.
    crate::emacs_core::perf_trace::time_op(
        crate::emacs_core::perf_trace::HotpathOp::StringMatch,
        || {
            expect_args_range("posix-string-match", args, 2, 4)?;
            let inhibit_modify = args.get(3).is_some_and(|v| v.is_truthy());

            match (args[0].kind(), args[1].kind()) {
                (ValueKind::String, ValueKind::String) => {
                    let pattern = expect_lisp_string(&args[0])?;
                    let string = args[1].as_lisp_string().unwrap();
                    let start = crate::emacs_core::search::normalize_lisp_string_start_arg(
                        string,
                        args.get(2),
                    )?;
                    let string_syntax = string_regexp_syntax_lookup(
                        syntax_table,
                        category_table,
                        word_boundary,
                        string,
                        syntax_properties,
                    );
                    let syntax = string_syntax.as_lookup();
                    let result = super::regex::string_search_full_with_case_fold_source_lisp_pattern_posix_syntax(
                        pattern,
                        string,
                        super::regex::SearchedString::Heap(args[1]),
                        start,
                        case_fold,
                        true,
                        case_translation_table,
                        syntax,
                    );
                    let target = if inhibit_modify { None } else { match_data };
                    commit_string_search_success(result, target)
                }
                _ => {
                    let pattern = expect_string_lossy(&args[0])?;
                    let s = expect_string_lossy(&args[1])?;
                    let start = normalize_string_start_arg(&s, args.get(2))?;
                    let result = super::regex::string_search_full_with_case_fold_and_posix(
                        &pattern, &s, start, case_fold, true,
                    );
                    let target = if inhibit_modify { None } else { match_data };
                    commit_string_search_success(result, target)
                }
            }
        },
    )
}

pub(crate) fn builtin_string_match_p_with_case_fold(
    case_fold: bool,
    case_translation_table: Option<Value>,
    syntax_table: Option<&crate::emacs_core::syntax::SyntaxTable>,
    category_table: Option<Value>,
    word_boundary: crate::emacs_core::regex_emacs::WordBoundaryLookup,
    syntax_properties: crate::emacs_core::syntax::SyntaxProperties<'_>,
    args: &[Value],
) -> EvalResult {
    expect_args_range("string-match-p", args, 2, 3)?;
    match (args[0].kind(), args[1].kind()) {
        (ValueKind::String, ValueKind::String) => {
            let pattern = expect_lisp_string(&args[0])?;
            let string = args[1].as_lisp_string().unwrap();
            let start =
                crate::emacs_core::search::normalize_lisp_string_start_arg(string, args.get(2))?;
            let string_syntax = string_regexp_syntax_lookup(
                syntax_table,
                category_table,
                word_boundary,
                string,
                syntax_properties,
            );
            let syntax = string_syntax.as_lookup();
            commit_string_search_success(
                super::regex::string_search_full_with_case_fold_source_lisp_pattern_posix_syntax(
                    pattern,
                    string,
                    super::regex::SearchedString::Heap(args[1]),
                    start,
                    case_fold,
                    false,
                    case_translation_table,
                    syntax,
                ),
                None,
            )
        }
        _ => {
            let pattern = expect_string_lossy(&args[0])?;
            let s = expect_string_lossy(&args[1])?;
            let start = normalize_string_start_arg(&s, args.get(2))?;
            commit_string_search_success(
                super::regex::string_search_full_with_case_fold_and_posix(
                    &pattern, &s, start, case_fold, false,
                ),
                None,
            )
        }
    }
}

// There is no `builtin_string_match_p' subr entry point.  GNU DEFUNs
// `string-match' (src/search.c:442) and writes `string-match-p' as a
// `defsubst' over it (lisp/subr.el:5941), so every compiled caller inlines
// `(string-match REGEXP STRING START t)' and the name has no function of its
// own to reach (DIVERGENCES.md 152).  `builtin_string_match_p_with_case_fold'
// above stays: `Context::skip_debugger' calls it to match
// `debug-ignored-errors', which GNU also does from C -- `fast_string_match'
// at src/eval.c:2163, over src/search.c:485 -- and never through a Lisp name.

pub(crate) fn builtin_posix_string_match(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let case_fold = dynamic_or_global_symbol_value(eval, SearchStateVariable::CaseFoldSearch)
        .map(|v| !v.is_nil())
        .unwrap_or(true);
    // The canon TABLE only: regexp compile builds (and caches) the actual
    // CaseTranslation; constructing its 1KB memo per call was pure waste.
    let case_translation_table = if case_fold {
        Some(crate::emacs_core::casetab::current_case_canon_table(eval)?)
    } else {
        None
    };
    let current_buffer = eval.buffers.current_buffer();
    let syntax_table = current_buffer.map(crate::emacs_core::syntax::SyntaxTable::for_buffer);
    let category_table =
        Some(crate::emacs_core::category::active_category_table_for_buffer(current_buffer)?);
    let word_boundary = current_word_boundary_lookup(eval);
    let inhibit_changing = read_inhibit_changing_match_data(eval);
    let syntax_properties =
        current_string_match_syntax_properties(eval, &eval.obarray, &eval.buffers, args.get(1));
    let match_data = (!inhibit_changing).then_some(&mut eval.match_data);
    builtin_posix_string_match_with_state(
        case_fold,
        case_translation_table,
        syntax_table.as_ref(),
        category_table,
        word_boundary,
        syntax_properties,
        match_data,
        &args,
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_match_string(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("match-string", &args, 1, 2)?;
    let group_index = expect_int(&args[0])?;
    if group_index < 0 {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::fixnum(group_index), Value::fixnum(0)],
        ));
    }
    let group_index = group_index as usize;

    let md = match &eval.match_data {
        Some(md) => md,
        None => return Ok(Value::NIL),
    };

    let Some(group) = md.group(group_index) else {
        return Ok(Value::NIL);
    };
    let start = group.start();
    let end = group.end();

    let slice_lisp_string = |string: &crate::heap_types::LispString, use_char_positions: bool| {
        let (byte_start, byte_end) = if use_char_positions {
            (
                char_pos_to_byte_lisp_string(string, start),
                char_pos_to_byte_lisp_string(string, end),
            )
        } else {
            (start, end)
        };
        if byte_end <= string.byte_len() && byte_start <= byte_end {
            string.slice(byte_start, byte_end).map(Value::heap_string)
        } else {
            None
        }
    };

    // If an optional second arg is a string, use that first.
    if args.len() > 1 {
        let explicit_md = match_data_for_explicit_string_arg(md);
        let Some(group) = explicit_md.group(group_index) else {
            return Ok(Value::NIL);
        };
        let start = group.start();
        let end = group.end();
        if let Some(string) = eval.lisp_string(args[1]) {
            let (byte_start, byte_end) = (
                char_pos_to_byte_lisp_string(string, start),
                char_pos_to_byte_lisp_string(string, end),
            );
            if byte_end <= string.byte_len()
                && byte_start <= byte_end
                && let Some(slice) = string.slice(byte_start, byte_end).map(Value::heap_string)
            {
                return Ok(slice);
            }
            return Ok(Value::NIL);
        }

        if let Some(s) = args[1].as_utf8_str() {
            let (byte_start, byte_end) = (char_pos_to_byte(s, start), char_pos_to_byte(s, end));
            if byte_end <= s.len() && byte_start <= byte_end {
                return Ok(Value::string(&s[byte_start..byte_end]));
            }
            return Ok(Value::NIL);
        }
    }

    // Otherwise, if the match was against a string, use that string.
    if let Some(searched) = md.searched_string() {
        if let super::regex::SearchedString::Heap(val) = searched
            && let Some(string) = eval.lisp_string(*val)
        {
            if let Some(slice) = slice_lisp_string(string, true) {
                return Ok(slice);
            }
            return Ok(Value::NIL);
        }

        if let Some(string) = searched.as_lisp_string()
            && let Some(slice) = slice_lisp_string(string, true)
        {
            return Ok(slice);
        }
        return Ok(Value::NIL);
    }

    let buf = match eval.buffers.current_buffer() {
        Some(b) => b,
        None => return Ok(Value::NIL),
    };
    let start_byte = buf.lisp_pos_to_emacs_byte_pos(LispCharPos1::from_one_based_usize(start));
    let end_byte = buf.lisp_pos_to_emacs_byte_pos(LispCharPos1::from_one_based_usize(end));
    if end_byte.get() <= buf.total_emacs_byte_len().get() && start_byte <= end_byte {
        Ok(buf.buffer_substring_value_range(EmacsByteRange::new(start_byte, end_byte)))
    } else {
        Ok(Value::NIL)
    }
}

pub(crate) fn builtin_match_beginning_with_state(
    match_data: &Option<super::regex::MatchData>,
    args: &[Value],
) -> EvalResult {
    crate::emacs_core::perf_trace::time_op(
        crate::emacs_core::perf_trace::HotpathOp::MatchBeginning,
        || {
            expect_args("match-beginning", args, 1)?;
            // GNU `match_limit` (search.c) runs SUBEXP through `CHECK_FIXNUM`,
            // signalling `(wrong-type-argument fixnump …)` — not `integerp`.
            let group = expect_fixnum(&args[0])?;
            if group < 0 {
                return Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![Value::fixnum(group), Value::fixnum(0)],
                ));
            }
            let group = group as usize;

            let md = match match_data {
                Some(md) => md,
                None => return Ok(Value::NIL),
            };

            match md.group(group) {
                Some(group) => Ok(Value::fixnum(group.start() as i64)),
                None => Ok(Value::NIL),
            }
        },
    )
}

pub(crate) fn builtin_match_beginning(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("match-beginning", &args, 1)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_match_beginning_1(eval, arg(0))
}
/// `match-beginning` as registered: fixed arity 1, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a1` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
pub(crate) fn builtin_match_beginning_1(
    eval: &mut super::eval::Context,
    subexp: Value,
) -> EvalResult {
    let args: [Value; 1] = [subexp];
    builtin_match_beginning_with_state(&eval.match_data, &args)
}

pub(crate) fn builtin_match_end_with_state(
    match_data: &Option<super::regex::MatchData>,
    args: &[Value],
) -> EvalResult {
    crate::emacs_core::perf_trace::time_op(
        crate::emacs_core::perf_trace::HotpathOp::MatchEnd,
        || {
            expect_args("match-end", args, 1)?;
            // GNU `match_limit` (search.c) runs SUBEXP through `CHECK_FIXNUM`,
            // signalling `(wrong-type-argument fixnump …)` — not `integerp`.
            let group = expect_fixnum(&args[0])?;
            if group < 0 {
                return Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![Value::fixnum(group), Value::fixnum(0)],
                ));
            }
            let group = group as usize;

            let md = match match_data {
                Some(md) => md,
                None => return Ok(Value::NIL),
            };

            match md.group(group) {
                Some(group) => Ok(Value::fixnum(group.end() as i64)),
                None => Ok(Value::NIL),
            }
        },
    )
}

pub(crate) fn builtin_match_end(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("match-end", &args, 1)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_match_end_1(eval, arg(0))
}
/// `match-end` as registered: fixed arity 1, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a1` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
pub(crate) fn builtin_match_end_1(eval: &mut super::eval::Context, subexp: Value) -> EvalResult {
    let args: [Value; 1] = [subexp];
    builtin_match_end_with_state(&eval.match_data, &args)
}

/// How buffer provenance should materialize in a `(match-data)` result.
///
/// GNU retains the searched buffer object after it dies, but a marker cannot
/// attach to that dead buffer.  Keeping those states distinct prevents a
/// marker from carrying the contradictory combination of a dead buffer ID and
/// a live-looking position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatchDataMaterializationSource {
    String,
    LiveBuffer(BufferId),
    DeadBuffer(BufferId),
}

impl MatchDataMaterializationSource {
    fn classify(md: &super::regex::MatchData, buffers: &crate::buffer::BufferManager) -> Self {
        match md.source() {
            MatchDataSource::String => Self::String,
            MatchDataSource::Buffer(buffer_id) if buffers.get(buffer_id).is_some() => {
                Self::LiveBuffer(buffer_id)
            }
            MatchDataSource::Buffer(buffer_id) => Self::DeadBuffer(buffer_id),
        }
    }

    fn buffer_id(self) -> Option<BufferId> {
        match self {
            Self::String => None,
            Self::LiveBuffer(buffer_id) | Self::DeadBuffer(buffer_id) => Some(buffer_id),
        }
    }
}

pub(crate) fn builtin_match_data_with_state(
    buffers: &mut crate::buffer::BufferManager,
    match_data: &Option<super::regex::MatchData>,
    args: &[Value],
) -> EvalResult {
    #[cfg(debug_assertions)]
    super::regex::match_stats::count_full_export();
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("match-data"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let reuse = args.get(1).copied().unwrap_or(Value::NIL);
    if args.get(2).is_some_and(|arg| arg.is_truthy()) {
        reseat_match_data_markers(buffers, reuse, None);
    }

    let Some(md) = match_data else {
        return Ok(Value::NIL);
    };
    let integers = args.first().is_some_and(|arg| arg.is_truthy());
    let source = MatchDataMaterializationSource::classify(md, buffers);

    // Emacs trims trailing unmatched groups from match-data output.
    let mut trailing = md.group_count();
    while trailing > 0 && md.group(trailing - 1).is_none() {
        trailing -= 1;
    }

    let mut flat: Vec<Value> = Vec::with_capacity(trailing * 2);
    for group_index in 0..trailing {
        let grp = md.group(group_index);
        match grp {
            Some(group) => {
                let start = group.start() as i64;
                let end = group.end() as i64;
                match source {
                    MatchDataMaterializationSource::String
                    | MatchDataMaterializationSource::LiveBuffer(_)
                    | MatchDataMaterializationSource::DeadBuffer(_)
                        if integers =>
                    {
                        flat.push(Value::fixnum(start));
                        flat.push(Value::fixnum(end));
                    }
                    MatchDataMaterializationSource::String => {
                        flat.push(Value::fixnum(start));
                        flat.push(Value::fixnum(end));
                    }
                    MatchDataMaterializationSource::LiveBuffer(buffer_id) => {
                        flat.push(super::marker::make_registered_buffer_marker(
                            buffers,
                            buffer_id,
                            LispCharPos1::new(start),
                            false,
                        ));
                        flat.push(super::marker::make_registered_buffer_marker(
                            buffers,
                            buffer_id,
                            LispCharPos1::new(end),
                            false,
                        ));
                    }
                    MatchDataMaterializationSource::DeadBuffer(_) => {
                        // GNU Fmatch_data creates fresh markers and asks
                        // Fset_marker to attach them to last_thing_searched.
                        // A dead buffer makes Fset_marker leave them fully
                        // detached, with no saved last position.
                        flat.push(super::marker::make_marker_value(None, None, false));
                        flat.push(super::marker::make_marker_value(None, None, false));
                    }
                }
            }
            None => {
                flat.push(Value::NIL);
                flat.push(Value::NIL);
            }
        }
    }

    if integers && let Some(buffer_id) = source.buffer_id() {
        flat.push(Value::make_buffer(buffer_id));
    }
    Ok(store_match_data_in_reuse(reuse, &flat))
}

fn store_match_data_in_reuse(reuse: Value, data: &[Value]) -> Value {
    if !reuse.is_cons() {
        return Value::list_from_slice(data);
    }

    let mut index = 0usize;
    let mut tail = reuse;
    let mut prev = Value::NIL;
    while tail.is_cons() {
        tail.set_car(data.get(index).copied().unwrap_or(Value::NIL));
        prev = tail;
        tail = tail.cons_cdr();
        index += 1;
    }

    if index < data.len() {
        prev.set_cdr(Value::list_from_slice(&data[index..]));
    }

    reuse
}

/// A position accepted by GNU `set-match-data`.
///
/// Detached markers are not errors here: search.c explicitly coerces them to
/// integer zero.  Model that case instead of losing it in an
/// `(i64, Option<BufferId>)` pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatchDataInputPosition {
    Integer(i64),
    LiveMarker { position: i64, buffer_id: BufferId },
    DetachedMarker,
}

impl MatchDataInputPosition {
    fn position(self) -> i64 {
        match self {
            Self::Integer(position) | Self::LiveMarker { position, .. } => position,
            Self::DetachedMarker => 0,
        }
    }

    fn buffer_id(self) -> Option<BufferId> {
        match self {
            Self::LiveMarker { buffer_id, .. } => Some(buffer_id),
            Self::Integer(_) | Self::DetachedMarker => None,
        }
    }
}

fn expect_match_data_position_in_manager(
    buffers: &crate::buffer::BufferManager,
    value: &Value,
) -> Result<MatchDataInputPosition, Flow> {
    match value.kind() {
        ValueKind::Fixnum(position) => Ok(MatchDataInputPosition::Integer(position)),
        _ if super::marker::is_marker(value) => {
            let fields = super::marker::marker_logical_fields(value)
                .expect("marker predicate guarantees marker fields");
            match fields {
                (Some(buffer_id), Some(position), _) if buffers.get(buffer_id).is_some() => {
                    Ok(MatchDataInputPosition::LiveMarker {
                        position: position.as_i64(),
                        buffer_id,
                    })
                }
                _ => Ok(MatchDataInputPosition::DetachedMarker),
            }
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

pub(crate) fn builtin_match_data(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_match_data_with_state(&mut eval.buffers, &eval.match_data, &args)
}

/// A marker-backed snapshot produced by GNU-compatible `(match-data)`.
///
/// Keep this private newtype instead of passing a bare `Value` through native
/// unwind code: only a snapshot captured here may be restored with reseating.
/// Buffer positions are markers, so the saved ranges continue to track edits
/// made while the protected operation runs.
#[derive(Clone, Copy)]
struct SavedMatchData(Value);

impl SavedMatchData {
    fn capture(eval: &mut super::eval::Context) -> Result<Self, Flow> {
        builtin_match_data(eval, Vec::new()).map(Self)
    }

    fn root(self, eval: &mut super::eval::Context) {
        eval.push_specpdl_root(self.0);
    }

    fn restore(self, eval: &mut super::eval::Context) -> EvalResult {
        builtin_set_match_data(eval, vec![self.0, Value::T])
    }
}

/// Run native evaluator work with GNU `record_unwind_save_match_data`
/// semantics.
///
/// This is the Rust-side equivalent of GNU `search.c` saving `(match-data)` on
/// the unwind stack and restoring it with `(set-match-data SAVED t)`.  The
/// closure boundary makes restoration unavoidable on both `Ok` and `Flow`
/// exits, while the rooted [`SavedMatchData`] keeps its marker list alive
/// across arbitrary Lisp execution and GC.
pub(crate) fn with_preserved_match_data<T>(
    eval: &mut super::eval::Context,
    operation: impl FnOnce(&mut super::eval::Context) -> Result<T, Flow>,
) -> Result<T, Flow> {
    let roots = eval.save_specpdl_roots();
    let saved = match SavedMatchData::capture(eval) {
        Ok(saved) => saved,
        Err(flow) => {
            eval.restore_specpdl_roots(roots);
            return Err(flow);
        }
    };
    saved.root(eval);

    let operation_result = operation(eval);
    let restore_result = saved.restore(eval);
    eval.restore_specpdl_roots(roots);

    match operation_result {
        Err(flow) => Err(flow),
        Ok(value) => restore_result.map(|_| value),
    }
}

pub(crate) fn builtin_set_match_data_with_state(
    buffers: &mut crate::buffer::BufferManager,
    match_data: &mut Option<super::regex::MatchData>,
    args: &[Value],
) -> EvalResult {
    expect_min_args("set-match-data", args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("set-match-data"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    if args[0].is_nil() {
        *match_data = None;
        return Ok(Value::NIL);
    }

    let items = list_to_vec(&args[0]).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), args[0]],
        )
    })?;

    let explicit_buffer_id = if items.len() % 2 == 1 {
        items.last().and_then(|value| value.as_buffer_id())
    } else {
        None
    };
    let pair_len = items.len() - usize::from(explicit_buffer_id.is_some());

    let mut groups: Vec<Option<MatchGroup>> = Vec::with_capacity(pair_len / 2);
    let mut searched_buffer = explicit_buffer_id;
    let mut i = 0usize;
    while i + 1 < pair_len {
        let start_v = &items[i];
        let end_v = &items[i + 1];

        if start_v.is_nil() && end_v.is_nil() {
            groups.push(None);
            i += 2;
            continue;
        }

        let start = expect_match_data_position_in_manager(buffers, start_v)?;
        let end = expect_match_data_position_in_manager(buffers, end_v)?;
        if searched_buffer.is_none() {
            searched_buffer = start.buffer_id().or(end.buffer_id());
        }
        let start = start.position();
        let end = end.position();

        // Emacs treats negative marker positions as an end sentinel and
        // truncates remaining groups.
        if start < 0 || end < 0 {
            break;
        }

        groups.push(Some(MatchGroup::new(start as usize, end as usize)));
        i += 2;
    }

    if groups.is_empty() {
        *match_data = None;
    } else if let Some(searched_buffer) = searched_buffer {
        *match_data = Some(super::regex::MatchData::buffer_lisp_chars(
            groups,
            searched_buffer,
        ));
    } else {
        *match_data = Some(super::regex::MatchData::string(groups, None));
    }

    if args.get(1).is_some_and(|arg| arg.is_truthy()) {
        reseat_match_data_markers(buffers, args[0], Some(pair_len));
    }

    Ok(Value::NIL)
}

fn reseat_match_data_markers(
    buffers: &mut crate::buffer::BufferManager,
    list: Value,
    max_cells: Option<usize>,
) {
    let mut tail = list;
    let mut seen = 0usize;
    while tail.is_cons() && max_cells.is_none_or(|limit| seen < limit) {
        let item = tail.cons_car();
        if super::marker::is_marker(&item) {
            super::marker::detach_marker_in_buffers(buffers, &item);
            tail.set_car(Value::NIL);
        }
        tail = tail.cons_cdr();
        seen += 1;
    }
}

pub(crate) fn builtin_set_match_data(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_match_data_with_state(&mut eval.buffers, &mut eval.match_data, &args)
}

fn translate_match_data(match_data: &mut Option<super::regex::MatchData>, delta: i64) {
    if let Some(md) = match_data {
        md.translate_positions(delta);
    }
}

pub(crate) fn builtin_match_data_translate_with_state(
    _buffers: &crate::buffer::BufferManager,
    match_data: &mut Option<super::regex::MatchData>,
    args: &[Value],
) -> EvalResult {
    expect_args("match-data--translate", args, 1)?;
    let delta = expect_fixnum(&args[0])?;
    translate_match_data(match_data, delta);
    Ok(Value::NIL)
}

pub(crate) fn builtin_match_data_translate(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_match_data_translate_with_state(&eval.buffers, &mut eval.match_data, &args)
}

#[derive(Clone, Copy)]
struct BufferReplacementCoordinates {
    old_char_range: CharRange,
    replacement_char_len: CharLen,
}

impl BufferReplacementCoordinates {
    fn published_match_data_bounds(self) -> (usize, usize, usize) {
        let oldstart = self.old_char_range.start_lisp().to_one_based_usize();
        let oldend = self.old_char_range.end_lisp().to_one_based_usize();
        (
            oldstart,
            oldend,
            oldstart.saturating_add(self.replacement_char_len.get()),
        )
    }
}

fn update_match_data_after_buffer_replace(
    match_data: &mut Option<super::regex::MatchData>,
    replacement: BufferReplacementCoordinates,
) {
    let Some(md) = match_data else {
        return;
    };

    let (oldstart, oldend, newend) = replacement.published_match_data_bounds();
    let change = newend as i64 - oldend as i64;
    md.map_lisp_positions(|match_group| {
        let mut start = match_group.start();
        let mut end = match_group.end();

        if start <= oldstart {
            // Keep starts for enclosing groups, matching GNU's optimistic
            // `update_search_regs` heuristic.
        } else if start >= oldend {
            start = (start as i64 + change) as usize;
        } else {
            start = oldstart;
        }

        if end >= oldend {
            end = (end as i64 + change) as usize;
        } else if end > oldstart {
            end = oldstart;
        }
        MatchGroup::new(start, end)
    });
}

/// Variant that also carries the current value of
/// `case-symbols-as-words` into the case-preservation decision for
/// `replace-match` with FIXEDCASE=nil. Audit findings #14/#20 in
/// `drafts/regex-search-audit.md`.
pub(crate) fn builtin_replace_match_with_state_and_flags(
    obarray: &crate::emacs_core::symbol::Obarray,
    buffers: &mut crate::buffer::BufferManager,
    match_data: &mut Option<super::regex::MatchData>,
    args: &[Value],
    _case_symbols_as_words: bool,
) -> EvalResult {
    expect_min_args("replace-match", args, 1)?;
    if args.len() > 5 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("replace-match"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let newtext_lisp = expect_lisp_string(&args[0])?;
    let fixedcase = args.get(1).is_some_and(|arg| arg.is_truthy());
    let literal = args.get(2).is_some_and(|arg| arg.is_truthy());
    let raw_subexp = args.get(4).copied().unwrap_or(Value::NIL);
    let string_arg = if args.get(3).is_some_and(|arg| !arg.is_nil()) {
        Some(expect_lisp_string(&args[3])?)
    } else {
        None
    };
    let subexp = if args.get(4).is_some_and(|arg| !arg.is_nil()) {
        let n = expect_int(&args[4])?;
        if n < 0 {
            return if let Some(source) = string_arg.as_ref() {
                Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![
                        Value::fixnum(n),
                        Value::fixnum(0),
                        Value::fixnum(source.schars() as i64),
                    ],
                ))
            } else {
                Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![Value::fixnum(n)],
                ))
            };
        }
        n as usize
    } else {
        0usize
    };

    let md_snapshot = match_data.clone();
    let missing_subexp_error = super::regex::REPLACE_MATCH_SUBEXP_MISSING;
    let missing_subexp_signal = |subexp_value: Value| {
        signal(
            "error",
            vec![Value::string(missing_subexp_error), subexp_value],
        )
    };
    // C-level `error()' messages are requoted via `text-quoting-style' by
    // GNU's doprnt (e.g. "Invalid use of `\\' ..." -> curly quotes).
    let quoting_style = crate::emacs_core::coding::effective_text_quoting_style(obarray);
    let c_error = |msg: String| {
        signal(
            "error",
            vec![Value::string(
                crate::emacs_core::coding::requote_c_error_message(&msg, quoting_style),
            )],
        )
    };

    if let Some(source) = string_arg {
        if md_snapshot.is_none() {
            return Err(missing_subexp_signal(raw_subexp));
        }
        if let Some(md) = md_snapshot.as_ref()
            && subexp >= md.group_count()
        {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![
                    Value::fixnum(subexp as i64),
                    Value::fixnum(0),
                    Value::fixnum(md.group_count().saturating_sub(1) as i64),
                ],
            ));
        }
        let string_md_snapshot = md_snapshot.as_ref().map(match_data_for_explicit_string_arg);
        return match crate::emacs_core::search::replace_match_lisp_string_with_syntax(
            source,
            newtext_lisp,
            fixedcase,
            literal,
            subexp,
            &string_md_snapshot,
        ) {
            Ok(result) => Ok(Value::heap_string(result)),
            Err(msg) if msg == missing_subexp_error => Err(missing_subexp_signal(raw_subexp)),
            Err(msg) => Err(c_error(msg)),
        };
    }

    if let Some(md) = md_snapshot.as_ref()
        && subexp >= md.group_count()
    {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![
                Value::fixnum(subexp as i64),
                Value::fixnum(0),
                Value::fixnum(md.group_count().saturating_sub(1) as i64),
            ],
        ));
    }

    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let (old_byte_range, old_char_range, replacement, case_action) = {
        let buf = buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        // GNU checks the subexpression against the *accessible* portion of the
        // current buffer and reports both endpoints (search.c:2418-2427).
        if let Some(group) = md_snapshot.as_ref().and_then(|md| md.group(subexp)) {
            let begv = buf.point_min_lisp_char_pos().to_one_based_usize();
            let zv = buf.point_max_lisp_char_pos().to_one_based_usize();
            if group.start() < begv || group.end() > zv {
                return Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![
                        Value::fixnum(group.start() as i64),
                        Value::fixnum(group.end() as i64),
                    ],
                ));
            }
        }
        let replacement = crate::emacs_core::search::compute_buffer_replacement_lisp_string(
            buf,
            newtext_lisp,
            fixedcase,
            literal,
            subexp,
            &md_snapshot,
        )
        .map_err(|msg| {
            if msg == missing_subexp_error {
                missing_subexp_signal(raw_subexp)
            } else {
                c_error(msg)
            }
        })?;
        let case_action = if fixedcase {
            crate::emacs_core::casefiddle::ReplaceMatchCaseAction::NoChange
        } else {
            let matched_range = EmacsByteRange::new(
                EmacsBytePos::new(replacement.0),
                EmacsBytePos::new(replacement.1),
            );
            let matched = buf.buffer_substring_lisp_string_range(matched_range);
            crate::emacs_core::casefiddle::replace_match_case_action_lisp_default(&matched)
        };
        (
            EmacsByteRange::new(
                EmacsBytePos::new(replacement.0),
                EmacsBytePos::new(replacement.1),
            ),
            CharRange::new(
                buf.emacs_byte_pos_to_char_pos_clamped(EmacsBytePos::new(replacement.0)),
                buf.emacs_byte_pos_to_char_pos_clamped(EmacsBytePos::new(replacement.1)),
            ),
            replacement.2,
            case_action,
        )
    };
    let replacement_len = replacement.sbytes();
    let replacement_char_len = CharLen::new(replacement.schars());
    let old_range = buffers
        .edit_range_for_buffer_emacs_byte_range(current_id, old_byte_range)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    super::super::fns::replace_buffer_region_lisp_string_in_manager(
        buffers,
        current_id,
        old_range,
        &replacement,
    )?;
    let replacement_byte_range = EmacsByteRange::new(
        old_byte_range.start(),
        old_byte_range
            .start()
            .add_len(crate::buffer::EmacsByteLen::new(replacement_len)),
    );
    if case_action != crate::emacs_core::casefiddle::ReplaceMatchCaseAction::NoChange
        && old_byte_range.start() < replacement_byte_range.end()
        && let Some(buf) = buffers.get_mut(current_id)
    {
        let start_char = buffer_byte_to_char_pos(buf, replacement_byte_range.start());
        let cased_text = buf.buffer_substring_lisp_string_range(replacement_byte_range);
        let mut undo_list = buf.get_undo_list();
        if !crate::buffer::undo::undo_list_is_disabled(&undo_list) {
            let end_char = start_char.add_len(CharLen::new(cased_text.schars()));
            crate::buffer::undo::undo_list_record_delete(
                &mut undo_list,
                start_char,
                cased_text.clone(),
                end_char,
            );
            crate::buffer::undo::undo_list_record_insert(
                &mut undo_list,
                start_char,
                CharLen::new(cased_text.schars()),
            );
            buf.set_undo_list(undo_list);
        }
    }
    // GNU `src/search.c:Freplace_match` records the caller's old point while
    // editing, then "officially" moves point to NEWPOINT, the end of the
    // replacement text.  Lisp parsers such as `xml-parse-string` depend on
    // this to continue after an expanded entity rather than re-reading the
    // replacement from its beginning.
    let _ = buffers.goto_buffer_emacs_byte_pos(current_id, replacement_byte_range.end());
    update_match_data_after_buffer_replace(
        match_data,
        BufferReplacementCoordinates {
            old_char_range,
            replacement_char_len,
        },
    );
    Ok(Value::NIL)
}

pub(crate) fn builtin_replace_match(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    // GNU `src/search.c:2485-2505` consults `case-symbols-as-words`
    // when classifying the matched text for FIXEDCASE=nil. Read it
    // from the current dynamic environment once and thread it down.
    // See audit finding #20 in `drafts/regex-search-audit.md`.
    let case_symbols_as_words =
        dynamic_or_global_symbol_value(eval, SearchStateVariable::CaseSymbolsAsWords)
            .map(|v| !v.is_nil())
            .unwrap_or(false);

    // Determine whether this is a buffer replacement (4th arg nil/absent) so we
    // can fire modification hooks.  String replacements don't touch the buffer.
    let is_buffer_replace = args.len() < 4 || args[3].is_nil();

    if is_buffer_replace {
        // Try to compute the match region for before-change signalling.
        let subexp = if args.len() >= 5 && !args[4].is_nil() {
            match expect_int(&args[4]) {
                Ok(n) if n >= 0 => n as usize,
                _ => {
                    return builtin_replace_match_with_state_and_flags(
                        &eval.obarray,
                        &mut eval.buffers,
                        &mut eval.match_data,
                        &args,
                        case_symbols_as_words,
                    );
                }
            }
        } else {
            0usize
        };
        if let Some(ref md) = eval.match_data
            && !md.source().is_string()
            && md.group(subexp).is_some()
        {
            let newtext_lisp = eval.expect_lisp_string(args[0])?;
            let fixedcase = args.get(1).is_some_and(|arg| arg.is_truthy());
            let literal = args.get(2).is_some_and(|arg| arg.is_truthy());
            let raw_subexp = args.get(4).copied().unwrap_or(Value::NIL);
            let missing_subexp_error = super::regex::REPLACE_MATCH_SUBEXP_MISSING;
            // C-level `error()' messages are requoted via
            // `text-quoting-style' by GNU's doprnt.
            let quoting_style =
                crate::emacs_core::coding::effective_text_quoting_style(&eval.obarray);
            let change = {
                let buf = eval
                    .buffers
                    .current_buffer()
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
                let (oldstart, oldend, replacement) =
                    crate::emacs_core::search::compute_buffer_replacement_lisp_string(
                        buf,
                        newtext_lisp,
                        fixedcase,
                        literal,
                        subexp,
                        &eval.match_data,
                    )
                    .map_err(|msg| {
                        if msg == missing_subexp_error {
                            signal(
                                "error",
                                vec![Value::string(missing_subexp_error), raw_subexp],
                            )
                        } else {
                            signal(
                                "error",
                                vec![Value::string(
                                    crate::emacs_core::coding::requote_c_error_message(
                                        &msg,
                                        quoting_style,
                                    ),
                                )],
                            )
                        }
                    })?;
                let current_id = eval
                    .buffers
                    .current_buffer_id()
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

                super::editfns::text_change_for_lisp_string_replacement_in_manager(
                    &eval.buffers,
                    current_id,
                    EmacsByteRange::new(EmacsBytePos::new(oldstart), EmacsBytePos::new(oldend)),
                    &replacement,
                )?
            };
            super::editfns::signal_before_text_change(eval, change)?;
            let result = builtin_replace_match_with_state_and_flags(
                &eval.obarray,
                &mut eval.buffers,
                &mut eval.match_data,
                &args,
                case_symbols_as_words,
            )?;
            super::editfns::signal_after_text_change(eval, change)?;
            return Ok(result);
        }
    }

    // Fallback: string replacement or no match data — no buffer hooks needed.
    builtin_replace_match_with_state_and_flags(
        &eval.obarray,
        &mut eval.buffers,
        &mut eval.match_data,
        &args,
        case_symbols_as_words,
    )
}

#[cfg(test)]
#[path = "tests/search.rs"]
mod tests;
