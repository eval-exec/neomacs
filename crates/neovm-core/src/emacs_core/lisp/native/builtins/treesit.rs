use super::*;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_fixnum};
use libloading::Library;
use std::path::{Path, PathBuf};
use strum::{EnumString, IntoStaticStr};
use tree_sitter::{
    LANGUAGE_VERSION, Language, MIN_COMPATIBLE_LANGUAGE_VERSION, Parser, Point, Range as TSRange,
};
use tree_sitter_language::LanguageFn;

use crate::buffer::{Buffer, BufferId, EmacsBytePos, EmacsByteRange, LispCharPos1};
use crate::emacs_core::buffer::expect_buffer_id;
use crate::emacs_core::emacs_char::byte_to_char_pos;
use crate::emacs_core::intern::{NIL_SYM_ID, SymId, intern, resolve_sym};
use crate::emacs_core::treesit::{
    self as runtime, NODE_SLOT_PARSER, PARSER_SLOT_EMBED_LEVEL, PARSER_SLOT_LANGUAGE,
    PARSER_SLOT_NOTIFIERS, PARSER_SLOT_TAG, ParserFreshness, ParserInputRevision,
    ParserReparseKind, ParserTagFilter, QUERY_SLOT_LANGUAGE, QUERY_SLOT_SOURCE,
};
use crate::heap_types::LispString;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum TreesitNodeProperty {
    Named,
    Missing,
    Extra,
    Outdated,
    HasError,
    Live,
}

impl TreesitNodeProperty {
    fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    #[cfg(test)]
    fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum TreesitBuiltinPredicate {
    Named,
    Anonymous,
}

impl TreesitBuiltinPredicate {
    fn from_symbol_value(value: Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    fn matches_node(self, node: tree_sitter::Node<'_>) -> bool {
        match self {
            Self::Named => node.is_named(),
            Self::Anonymous => !node.is_named(),
        }
    }

    #[cfg(test)]
    fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum TreesitBooleanPredicate {
    Not,
    Or,
    And,
}

impl TreesitBooleanPredicate {
    fn from_symbol_value(value: Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    #[cfg(test)]
    fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
enum TreesitPatternKeyword {
    #[strum(serialize = ":anchor")]
    Anchor,
    #[strum(serialize = ":?")]
    Question,
    #[strum(serialize = ":*")]
    Star,
    #[strum(serialize = ":+")]
    Plus,
    #[strum(serialize = ":equal")]
    Equal,
    #[strum(serialize = ":eq?")]
    EqQuestion,
    #[strum(serialize = ":match")]
    Match,
    #[strum(serialize = ":match?")]
    MatchQuestion,
    #[strum(serialize = ":pred")]
    Pred,
    #[strum(serialize = ":pred?")]
    PredQuestion,
}

impl TreesitPatternKeyword {
    fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    fn expansion(self) -> &'static str {
        match self {
            Self::Anchor => ".",
            Self::Question => "?",
            Self::Star => "*",
            Self::Plus => "+",
            Self::Equal | Self::EqQuestion => "#eq?",
            Self::Match | Self::MatchQuestion => "#match?",
            Self::Pred | Self::PredQuestion => "#pred?",
        }
    }

    #[cfg(test)]
    fn name(self) -> &'static str {
        self.into()
    }
}

fn default_dynamic_library_suffixes() -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    {
        &[".dll"]
    }
    #[cfg(target_os = "macos")]
    {
        &[".dylib"]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        &[".so"]
    }
}

fn posix_versioned_candidates(base: &str, suffix: &str) -> Vec<String> {
    #[cfg(unix)]
    {
        #[cfg(target_os = "macos")]
        {
            vec![format!("{base}{suffix}")]
        }
        #[cfg(not(target_os = "macos"))]
        {
            let mut out = vec![format!("{base}{suffix}")];
            out.push(format!("{base}{suffix}.0"));
            out.push(format!("{base}{suffix}.0.0"));
            for version in MIN_COMPATIBLE_LANGUAGE_VERSION..=LANGUAGE_VERSION {
                out.push(format!("{base}{suffix}.{version}.0"));
            }
            out
        }
    }
    #[cfg(not(unix))]
    {
        vec![format!("{base}{suffix}")]
    }
}

fn parse_symbol_arg(name: &str, value: &Value) -> Result<SymId, Flow> {
    if value.is_nil() {
        return Ok(NIL_SYM_ID);
    }
    value.as_symbol_id().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *value, Value::symbol(name)],
        )
    })
}

fn expect_symbol_or_nil(name: &str, value: Value) -> Result<Value, Flow> {
    if value.is_nil() || value.as_symbol_name().is_some() {
        Ok(value)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), value, Value::symbol(name)],
        ))
    }
}

fn parser_type_error(name: &str, value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![
            Value::symbol("treesit-parser-p"),
            value,
            Value::symbol(name),
        ],
    )
}

fn node_type_error(name: &str, value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("treesit-node-p"), value, Value::symbol(name)],
    )
}

fn query_type_error(name: &str, value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("treesit-query-p"), value, Value::symbol(name)],
    )
}

fn compiled_query_type_error(name: &str, value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![
            Value::symbol("treesit-compiled-query-p"),
            value,
            Value::symbol(name),
        ],
    )
}

fn parser_deleted_error(value: Value) -> Flow {
    signal(LispCondition::TreesitParserDeleted, vec![value])
}

#[cfg(test)]
thread_local! {
    static TREESIT_BUFFER_SOURCE_EXTRACTIONS: std::cell::Cell<usize> = const {
        std::cell::Cell::new(0)
    };
}

#[cfg(test)]
fn reset_treesit_buffer_source_extraction_count() {
    TREESIT_BUFFER_SOURCE_EXTRACTIONS.set(0);
}

#[cfg(test)]
fn treesit_buffer_source_extraction_count() -> usize {
    TREESIT_BUFFER_SOURCE_EXTRACTIONS.get()
}

fn treesit_buffer_source(buffer: &crate::buffer::Buffer) -> LispString {
    #[cfg(test)]
    TREESIT_BUFFER_SOURCE_EXTRACTIONS.with(|count| count.set(count.get() + 1));
    buffer.buffer_substring_lisp_string_no_properties_range(buffer.accessible_emacs_byte_range())
}

fn node_outdated_error(value: Value) -> Flow {
    signal(LispCondition::TreesitNodeOutdated, vec![value])
}

fn node_buffer_killed_error(value: Value) -> Flow {
    signal(LispCondition::TreesitNodeBufferKilled, vec![value])
}

fn treesit_parse_error(value: Value) -> Flow {
    signal(LispCondition::TreesitParseError, vec![value])
}

fn treesit_query_error(message: impl Into<String>) -> Flow {
    signal(
        LispCondition::TreesitQueryError,
        vec![Value::string(message.into())],
    )
}

fn treesit_query_error_from_query(err: tree_sitter::QueryError) -> Flow {
    signal(
        LispCondition::TreesitQueryError,
        vec![
            Value::string(err.message),
            Value::fixnum(err.offset as i64),
            Value::fixnum(err.row as i64),
            Value::fixnum(err.column as i64),
        ],
    )
}

fn list_value_to_strings(value: Option<Value>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    crate::emacs_core::value::list_to_vec(&value)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| item.as_str_owned())
        .collect()
}

fn list_assoc_symbol_key(value: Option<Value>, key: SymId) -> Option<Vec<Value>> {
    let list = crate::emacs_core::value::list_to_vec(&value?)?;
    for entry in list {
        let items = crate::emacs_core::value::list_to_vec(&entry)?;
        let Some(lang) = items.first().and_then(|v| v.as_symbol_id()) else {
            continue;
        };
        if lang == key {
            return Some(items);
        }
    }
    None
}

fn maybe_remap_language(eval: &super::eval::Context, language: SymId) -> SymId {
    let remapped =
        super::misc_eval::dynamic_or_global_symbol_value(eval, "treesit-language-remap-alist");
    let Some(items) = list_assoc_symbol_key(remapped, language) else {
        return language;
    };
    items
        .get(1)
        .and_then(|v| v.as_symbol_id())
        .unwrap_or(language)
}

fn language_requires_linecol_tracking(eval: &super::eval::Context, language: SymId) -> bool {
    let remapped = maybe_remap_language(eval, language);
    let Some(languages) = super::misc_eval::dynamic_or_global_symbol_value(
        eval,
        "treesit-languages-require-line-column-tracking",
    ) else {
        return false;
    };
    crate::emacs_core::value::list_to_vec(&languages)
        .unwrap_or_default()
        .into_iter()
        .any(|value| value.as_symbol_id().is_some_and(|name| name == remapped))
}

fn treesit_override_names(
    eval: &super::eval::Context,
    language: SymId,
) -> Option<(String, String)> {
    let overrides =
        super::misc_eval::dynamic_or_global_symbol_value(eval, "treesit-load-name-override-list");
    let items = list_assoc_symbol_key(overrides, language)?;
    let lib_name = items.get(1)?.as_str_owned()?;
    let c_symbol = items.get(2)?.as_str_owned()?;
    Some((lib_name, c_symbol))
}

fn treesit_user_emacs_dir(eval: &super::eval::Context) -> Option<String> {
    super::misc_eval::dynamic_or_global_symbol_value(eval, "user-emacs-directory")
        .and_then(|value| value.as_str_owned())
}

fn treesit_candidate_paths(eval: &super::eval::Context, language: SymId) -> Vec<String> {
    let remapped_language = maybe_remap_language(eval, language);
    let remapped_name = resolve_sym(remapped_language);
    let default_lib_base = format!("libtree-sitter-{remapped_name}");
    let default_c_symbol = format!("tree_sitter_{}", remapped_name.replace('-', "_"));
    let (lib_base_name, _c_symbol) = treesit_override_names(eval, remapped_language)
        .unwrap_or((default_lib_base, default_c_symbol));

    let mut candidates = Vec::new();

    for suffix in default_dynamic_library_suffixes() {
        candidates.extend(posix_versioned_candidates(&lib_base_name, suffix));
    }

    if let Some(user_emacs_dir) = treesit_user_emacs_dir(eval) {
        let base = Path::new(&user_emacs_dir)
            .join("tree-sitter")
            .join(&lib_base_name);
        let base = base.to_string_lossy().into_owned();
        for suffix in default_dynamic_library_suffixes() {
            candidates.extend(posix_versioned_candidates(&base, suffix));
        }
    }

    for dir in list_value_to_strings(super::misc_eval::dynamic_or_global_symbol_value(
        eval,
        "treesit-extra-load-path",
    )) {
        let base = Path::new(&dir).join(&lib_base_name);
        let base = base.to_string_lossy().into_owned();
        for suffix in default_dynamic_library_suffixes() {
            candidates.extend(posix_versioned_candidates(&base, suffix));
        }
    }

    candidates
}

fn load_language_from_path(path: &str, c_symbol: &str) -> Result<runtime::LoadedLanguage, String> {
    let library = unsafe { Library::new(path) }.map_err(|err| err.to_string())?;
    let symbol_name = format!("{c_symbol}\0");
    let lang_fn = unsafe {
        library
            .get::<unsafe extern "C" fn() -> *const ()>(symbol_name.as_bytes())
            .map_err(|err| err.to_string())?
    };
    let language = Language::new(unsafe { LanguageFn::from_raw(*lang_fn) });
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|err| format!("ABI mismatch: {err}"))?;
    let filename = PathBuf::from(path)
        .canonicalize()
        .ok()
        .map(|resolved| resolved.to_string_lossy().into_owned())
        .or_else(|| Some(path.to_owned()));
    Ok(runtime::LoadedLanguage {
        language,
        filename,
        _library: Some(library),
    })
}

fn load_language(
    eval: &mut super::eval::Context,
    language: SymId,
) -> Result<(Language, Option<String>), Value> {
    let remapped_language = maybe_remap_language(eval, language);
    if let Some((loaded, filename)) = eval.treesit.loaded_language(remapped_language) {
        return Ok((loaded, filename));
    }

    let remapped_name = resolve_sym(remapped_language);
    let default_lib_base = format!("libtree-sitter-{remapped_name}");
    let default_c_symbol = format!("tree_sitter_{}", remapped_name.replace('-', "_"));
    let (_lib_base_name, c_symbol) = treesit_override_names(eval, remapped_language)
        .unwrap_or((default_lib_base, default_c_symbol));

    let candidates = treesit_candidate_paths(eval, language);
    let mut errors = Vec::new();
    for candidate in candidates {
        match load_language_from_path(&candidate, &c_symbol) {
            Ok(loaded) => {
                let result = (loaded.language.clone(), loaded.filename.clone());
                eval.treesit
                    .cache_loaded_language(remapped_language, loaded);
                return Ok(result);
            }
            Err(err) => errors.push(err),
        }
    }

    Err(Value::list(
        std::iter::once(Value::symbol("not-found"))
            .chain(errors.into_iter().map(Value::string))
            .collect(),
    ))
}

fn resolve_buffer_ids(
    eval: &super::eval::Context,
    arg: Option<&Value>,
) -> Result<(BufferId, BufferId, Value), Flow> {
    let orig_id = match arg {
        None => eval
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?,
        Some(value) if value.is_nil() => eval
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?,
        Some(value) => expect_buffer_id(value)?,
    };
    let orig_buffer = eval
        .buffers
        .get(orig_id)
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
    let root_id = orig_buffer.base_buffer.unwrap_or(orig_id);
    Ok((orig_id, root_id, Value::make_buffer(orig_id)))
}

fn parser_record_slot(parser: Value, index: usize) -> Result<Value, Flow> {
    parser
        .as_record_data()
        .and_then(|items| items.get(index).copied())
        .ok_or_else(|| parser_type_error("treesit-parser-slot", parser))
}

fn query_record_slot(query: Value, index: usize) -> Result<Value, Flow> {
    query
        .as_record_data()
        .and_then(|items| items.get(index).copied())
        .ok_or_else(|| compiled_query_type_error("treesit-query-slot", query))
}

fn query_like_p(value: Value) -> bool {
    runtime::is_compiled_query(value) || value.is_cons() || value.is_string()
}

fn expect_parser_id(name: &str, value: Value) -> Result<u64, Flow> {
    runtime::parser_id(value).ok_or_else(|| parser_type_error(name, value))
}

fn expect_live_parser_id(
    eval: &super::eval::Context,
    name: &str,
    value: Value,
) -> Result<u64, Flow> {
    let id = expect_parser_id(name, value)?;
    let Some(entry) = eval.treesit.parser(id) else {
        return Err(parser_deleted_error(value));
    };
    if entry.deleted {
        return Err(parser_deleted_error(value));
    }
    Ok(id)
}

#[derive(Clone, Copy)]
struct NodeHandle {
    parser_id: u64,
    raw: tree_sitter::ffi::TSNode,
    generation: u64,
}

fn expect_node_handle(
    eval: &super::eval::Context,
    name: &str,
    value: Value,
) -> Result<NodeHandle, Flow> {
    let Some(id) = runtime::node_id(value) else {
        return Err(node_type_error(name, value));
    };
    let Some(entry) = eval.treesit.node(id) else {
        return Err(node_outdated_error(value));
    };
    Ok(NodeHandle {
        parser_id: entry.parser_id,
        raw: entry.raw,
        generation: entry.generation,
    })
}

fn ensure_current_node(
    eval: &super::eval::Context,
    name: &str,
    value: Value,
) -> Result<NodeHandle, Flow> {
    let handle = expect_node_handle(eval, name, value)?;
    let Some(parser) = eval.treesit.parser(handle.parser_id) else {
        return Err(node_outdated_error(value));
    };
    if parser.generation != handle.generation {
        return Err(node_outdated_error(value));
    }
    if eval.buffers.get(parser.orig_buffer_id).is_none() {
        return Err(node_buffer_killed_error(value));
    }
    Ok(handle)
}

fn parser_live_p(eval: &super::eval::Context, parser_id: u64) -> bool {
    let Some(parser) = eval.treesit.parser(parser_id) else {
        return false;
    };
    !parser.deleted && eval.buffers.get(parser.orig_buffer_id).is_some()
}

fn make_node_value_for_parser(
    eval: &mut super::eval::Context,
    parser_id: u64,
    node: tree_sitter::Node<'_>,
) -> Value {
    let (parser_value, generation) = {
        let parser = eval
            .treesit
            .parser(parser_id)
            .expect("parser should exist while creating node value");
        (parser.value, parser.generation)
    };
    let id = eval
        .treesit
        .insert_node(parser_id, node.into_raw(), generation);
    runtime::make_node_value(id, parser_value)
}

fn treesit_check_position(buf: &Buffer, value: Value) -> Result<i64, Flow> {
    let pos = expect_int(&value)?;
    let accessible_chars = buf.accessible_char_region();
    let min = accessible_chars.start_lisp().as_i64();
    let max = accessible_chars.end_lisp().as_i64();
    if pos < min || pos > max {
        return Err(signal(LispCondition::ArgsOutOfRange, vec![value]));
    }
    Ok(pos)
}

fn lisp_pos_to_relative_byte(buf: &Buffer, pos: i64) -> usize {
    buf.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(pos))
        .saturating_offset_from(buf.accessible_emacs_byte_region().start())
        .get()
}

fn treesit_position_to_relative_byte(buf: &Buffer, value: Value) -> Result<usize, Flow> {
    let pos = treesit_check_position(buf, value)?;
    Ok(lisp_pos_to_relative_byte(buf, pos))
}

fn treesit_invalid_range(ranges: Value) -> Flow {
    signal(
        "treesit-range-invalid",
        vec![
            Value::string("RANGE is either overlapping, out-of-order or out-of-range"),
            ranges,
        ],
    )
}

fn expect_treesit_range_fixnum(value: Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), value],
        )),
    }
}

fn validate_treesit_included_range(
    buf: &Buffer,
    range: Value,
    ranges: Value,
    last_point: &mut i64,
) -> Result<(i64, i64), Flow> {
    if !range.is_cons() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("consp"), range],
        ));
    }
    let beg = expect_treesit_range_fixnum(range.cons_car())?;
    let end = expect_treesit_range_fixnum(range.cons_cdr())?;
    let point_max = buf.accessible_char_region().end_lisp().as_i64();
    if *last_point <= beg && beg <= end && end <= point_max {
        *last_point = end;
        Ok((beg, end))
    } else {
        Err(treesit_invalid_range(ranges))
    }
}

fn byte_offset_to_lisp_pos(buf: &Buffer, source: &LispString, byte_offset: usize) -> Value {
    let char_offset = byte_to_char_pos(source.as_bytes(), byte_offset) as i64;
    Value::fixnum(buf.accessible_char_region().start_lisp().as_i64() + char_offset)
}

struct ParserReparseRequest {
    parser_value: Value,
    current_revision: ParserInputRevision,
    kind: ParserReparseKind,
    source: LispString,
    /// Buffer byte window the new tree will cover -- GNU's `visible_beg` /
    /// `visible_end` after `treesit_sync_visible_region`.
    visible: EmacsByteRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChangedRangeCollection {
    Ignore,
    Collect,
}

fn parser_reparse_request(
    eval: &mut super::eval::Context,
    parser_id: u64,
) -> Result<Option<ParserReparseRequest>, Flow> {
    let (parser_value, orig_buffer_id) = {
        let parser = eval
            .treesit
            .parser(parser_id)
            .ok_or_else(|| signal("error", vec![Value::string("Missing tree-sitter parser")]))?;
        if parser.deleted {
            return Err(parser_deleted_error(parser.value));
        }
        (parser.value, parser.orig_buffer_id)
    };

    let current_revision = {
        let buffer = eval.buffers.get(orig_buffer_id).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("Parser buffer has been killed")],
            )
        })?;
        ParserInputRevision::for_buffer(buffer)
    };

    // GNU catches up with the narrowing situation before it consults
    // `need_reparse`, because syncing can set it (`src/treesit.c:1908-1914`).
    {
        let buffer = eval.buffers.get(orig_buffer_id).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("Parser buffer has been killed")],
            )
        })?;
        eval.treesit.sync_visible_region(parser_id, buffer);
    }

    let reparse_kind = {
        let parser = eval
            .treesit
            .parser_mut(parser_id)
            .ok_or_else(|| signal("error", vec![Value::string("Missing tree-sitter parser")]))?;
        if parser.tree.is_none() {
            parser.freshness = ParserFreshness::Unparsed;
        }
        parser.freshness.reparse_kind(current_revision)
    };
    let Some(reparse_kind) = reparse_kind else {
        return Ok(None);
    };
    let (source, visible) = {
        let buffer = eval.buffers.get(orig_buffer_id).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("Parser buffer has been killed")],
            )
        })?;
        (
            treesit_buffer_source(buffer),
            buffer.accessible_emacs_byte_range(),
        )
    };
    Ok(Some(ParserReparseRequest {
        parser_value,
        current_revision,
        kind: reparse_kind,
        source,
        visible,
    }))
}

fn ensure_parser_reparsed(
    eval: &mut super::eval::Context,
    parser_id: u64,
    changed_range_collection: ChangedRangeCollection,
) -> Result<Option<Vec<runtime::SourceByteRange>>, Flow> {
    let Some(request) = parser_reparse_request(eval, parser_id)? else {
        return Ok(None);
    };

    let changed_ranges = {
        let parser = eval
            .treesit
            .parser_mut(parser_id)
            .ok_or_else(|| signal("error", vec![Value::string("Missing tree-sitter parser")]))?;
        // GNU always hands the previous tree back to tree-sitter
        // (`treesit_ensure_parsed`, `src/treesit.c:1925`); `Full` is reached
        // only for a parser that has no usable tree at all.
        let old_tree = match request.kind {
            ParserReparseKind::Incremental => parser.tree.as_ref().map(|tree| tree.tree().clone()),
            ParserReparseKind::Full => None,
        };
        // Issue #131: feed the parser the exact Emacs bytes so byte offsets
        // match the buffer and real PUA glyphs / eight-bit bytes survive.
        let tree = parser
            .parser
            .parse(request.source.as_bytes(), old_tree.as_ref())
            .ok_or_else(|| treesit_parse_error(request.parser_value))?;
        let changed_ranges = match changed_range_collection {
            ChangedRangeCollection::Ignore => Vec::new(),
            ChangedRangeCollection::Collect => {
                if let Some(old_tree) = old_tree.as_ref() {
                    old_tree
                        .changed_ranges(&tree)
                        .map(|range| {
                            runtime::SourceByteRange::new(range.start_byte, range.end_byte)
                        })
                        .collect::<Vec<_>>()
                } else if request.source.as_bytes().is_empty() {
                    Vec::new()
                } else {
                    vec![runtime::SourceByteRange::new(
                        0,
                        request.source.as_bytes().len(),
                    )]
                }
            }
        };
        parser.tree = Some(runtime::ParsedTree::new(tree, request.visible));
        parser.last_source = Some(request.source);
        parser.freshness = ParserFreshness::Clean(request.current_revision);
        parser.generation = parser.generation.saturating_add(1);
        if changed_range_collection == ChangedRangeCollection::Collect {
            parser.last_changed_ranges = changed_ranges.clone();
        }
        changed_ranges
    };
    eval.treesit.clear_nodes_for_parser(parser_id);
    Ok(Some(changed_ranges))
}

fn ensure_parser_parsed(eval: &mut super::eval::Context, parser_id: u64) -> Result<(), Flow> {
    let _ = ensure_parser_reparsed(eval, parser_id, ChangedRangeCollection::Ignore)?;
    Ok(())
}

fn ensure_query_compiled(eval: &mut super::eval::Context, query: Value) -> Result<(), Flow> {
    let id = runtime::query_id(query)
        .ok_or_else(|| compiled_query_type_error("treesit-query-compile", query))?;
    if eval
        .treesit
        .query(id)
        .and_then(|entry| entry.compiled.as_ref())
        .is_some()
    {
        return Ok(());
    }

    let language = query_record_slot(query, QUERY_SLOT_LANGUAGE)?;
    let language_sym = parse_symbol_arg("treesit-query-compile", &language)?;
    let source = query_record_slot(query, QUERY_SLOT_SOURCE)?;
    let source_string = expand_query_value("treesit-query-compile", source)?;

    let (lang, _) = load_language(eval, language_sym).map_err(|_| {
        treesit_query_error(format!(
            "Failed to load tree-sitter language `{}`",
            resolve_sym(language_sym)
        ))
    })?;
    let compiled =
        runtime::EmacsQuery::new(&lang, &source_string).map_err(treesit_query_error_from_query)?;
    let query_entry = eval
        .treesit
        .query_mut(id)
        .ok_or_else(|| compiled_query_type_error("treesit-query-compile", query))?;
    query_entry.compiled = Some(compiled);
    Ok(())
}

fn ensure_parser_parsed_with_changes(
    eval: &mut super::eval::Context,
    parser_id: u64,
) -> Result<Option<Vec<runtime::SourceByteRange>>, Flow> {
    ensure_parser_reparsed(eval, parser_id, ChangedRangeCollection::Collect)
}

fn expand_query_string(source: &str) -> String {
    let mut expanded = String::with_capacity(source.len() + 2);
    expanded.push('"');
    for ch in source.chars() {
        match ch {
            '\0' => expanded.push_str("\\0"),
            '\n' => expanded.push_str("\\n"),
            '\r' => expanded.push_str("\\r"),
            '\t' => expanded.push_str("\\t"),
            '"' | '\\' => {
                expanded.push('\\');
                expanded.push(ch);
            }
            _ => expanded.push(ch),
        }
    }
    expanded.push('"');
    expanded
}

fn pattern_keyword_expansion(name: &str) -> Option<&'static str> {
    TreesitPatternKeyword::from_symbol_name(name).map(TreesitPatternKeyword::expansion)
}

fn expand_pattern_value(pattern: Value) -> Result<String, Flow> {
    if let Some(name) = pattern.as_symbol_name()
        && let Some(expanded) = pattern_keyword_expansion(name)
    {
        return Ok(expanded.to_string());
    }

    if let Some(text) = pattern.as_str_owned() {
        return Ok(expand_query_string(&text));
    }

    if let Some(items) = pattern.as_vector_data() {
        let mut pieces = Vec::with_capacity(items.len());
        for item in items {
            pieces.push(expand_pattern_value(*item)?);
        }
        return Ok(format!("[{}]", pieces.join(" ")));
    }

    if let Some(items) = crate::emacs_core::value::list_to_vec(&pattern) {
        let mut pieces = Vec::with_capacity(items.len());
        for item in items {
            pieces.push(expand_pattern_value(item)?);
        }
        return Ok(format!("({})", pieces.join(" ")));
    }

    Ok(crate::emacs_core::print::print_value(&pattern))
}

fn expand_query_value(caller: &str, query: Value) -> Result<String, Flow> {
    if let Some(source) = query.as_str_owned() {
        return Ok(source);
    }
    if let Some(items) = crate::emacs_core::value::list_to_vec(&query) {
        let mut pieces = Vec::with_capacity(items.len());
        for item in items {
            pieces.push(expand_pattern_value(item)?);
        }
        return Ok(pieces.join(" "));
    }
    Err(query_type_error(caller, query))
}

fn byte_offset_to_linecol(
    source: &LispString,
    byte_offset: usize,
    hint: runtime::LineColCache,
) -> runtime::LineColCache {
    let bytes = source.as_bytes();
    let target = byte_offset.min(bytes.len());
    let (mut line, mut col, mut idx) =
        if hint.bytepos <= target && hint.bytepos <= bytes.len() && hint.line > 0 && hint.col > 0 {
            (hint.line, hint.col, hint.bytepos)
        } else {
            (1, 1, 0)
        };

    while idx < target {
        if bytes[idx] == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
        idx += 1;
    }

    runtime::LineColCache {
        line,
        col,
        bytepos: target,
    }
}

fn byte_offset_to_point(
    source: &LispString,
    byte_offset: usize,
    hint: runtime::LineColCache,
) -> Point {
    let linecol = byte_offset_to_linecol(source, byte_offset, hint);
    Point {
        row: linecol.line.saturating_sub(1) as usize,
        column: linecol.col.saturating_sub(1) as usize,
    }
}

fn query_range_bytes(
    buf: &Buffer,
    args: &[Value],
    beg_index: usize,
    end_index: usize,
) -> Result<Option<std::ops::Range<usize>>, Flow> {
    let Some(beg) = args.get(beg_index).copied() else {
        return Ok(None);
    };
    let Some(end) = args.get(end_index).copied() else {
        return Ok(None);
    };
    if beg.is_nil() || end.is_nil() {
        return Ok(None);
    }
    let start = treesit_position_to_relative_byte(buf, beg)?;
    let finish = treesit_position_to_relative_byte(buf, end)?;
    Ok(Some(start..finish))
}

fn changed_ranges_to_lisp(
    eval: &super::eval::Context,
    parser_id: u64,
    changed_ranges: &[runtime::SourceByteRange],
) -> Result<Value, Flow> {
    let parser = eval
        .treesit
        .parser(parser_id)
        .ok_or_else(|| signal("error", vec![Value::string("Missing tree-sitter parser")]))?;
    let buf = eval.buffers.get(parser.orig_buffer_id).ok_or_else(|| {
        signal(
            "error",
            vec![Value::string("Parser buffer has been killed")],
        )
    })?;
    let source = parser
        .last_source
        .as_ref()
        .ok_or_else(|| treesit_parse_error(parser.value))?;
    Ok(Value::list(
        changed_ranges
            .iter()
            .map(|range| {
                Value::cons(
                    byte_offset_to_lisp_pos(buf, source, range.start()),
                    byte_offset_to_lisp_pos(buf, source, range.end()),
                )
            })
            .collect(),
    ))
}

fn resolve_compiled_query_value(
    eval: &mut super::eval::Context,
    language_symbol: Value,
    query: Value,
    caller: &str,
) -> Result<Value, Flow> {
    let language_sym = parse_symbol_arg(caller, &language_symbol)?;
    let compiled_query = if runtime::is_compiled_query(query) {
        query
    } else {
        builtin_treesit_query_compile(eval, vec![language_symbol, query, Value::T])?
    };
    let query_language = query_record_slot(compiled_query, QUERY_SLOT_LANGUAGE)?;
    if query_language != language_symbol {
        return Err(treesit_query_error(format!(
            "Query language mismatch: expected `{}`",
            resolve_sym(language_sym)
        )));
    }
    ensure_query_compiled(eval, compiled_query)?;
    Ok(compiled_query)
}

#[derive(Clone, Copy)]
struct ResolvedNodeInput {
    parser_id: u64,
    parser_value: Value,
    language_symbol: Value,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    node_value: Value,
    node_raw: tree_sitter::ffi::TSNode,
}

fn resolve_node_input(
    eval: &mut super::eval::Context,
    value: Value,
    caller: &str,
) -> Result<ResolvedNodeInput, Flow> {
    if runtime::is_node(value) {
        let handle = ensure_current_node(eval, caller, value)?;
        let parser = eval
            .treesit
            .parser(handle.parser_id)
            .ok_or_else(|| node_outdated_error(value))?;
        return Ok(ResolvedNodeInput {
            parser_id: handle.parser_id,
            parser_value: parser.value,
            language_symbol: parser_record_slot(parser.value, PARSER_SLOT_LANGUAGE)?,
            node_value: value,
            node_raw: handle.raw,
        });
    }

    if runtime::is_parser(value) {
        let parser_id = expect_live_parser_id(eval, caller, value)?;
        ensure_parser_parsed(eval, parser_id)?;
        let root_raw = {
            let parser = eval
                .treesit
                .parser(parser_id)
                .ok_or_else(|| parser_deleted_error(value))?;
            parser
                .tree
                .as_ref()
                .map(|tree| tree.tree().root_node().into_raw())
                .ok_or_else(|| treesit_parse_error(value))?
        };
        let node_value = make_node_value_for_parser(eval, parser_id, unsafe {
            tree_sitter::Node::from_raw(root_raw)
        });
        return resolve_node_input(eval, node_value, caller);
    }

    if value.as_symbol_name().is_some() {
        let parser =
            builtin_treesit_parser_create(eval, vec![value, Value::NIL, Value::NIL, Value::NIL])?;
        return resolve_node_input(eval, parser, caller);
    }

    Err(node_type_error(caller, value))
}

fn lookup_thing_definition(
    eval: &super::eval::Context,
    language_symbol: Value,
    thing_symbol: Value,
) -> Option<Value> {
    let language_name = language_symbol.as_symbol_name()?;
    let thing_name = thing_symbol.as_symbol_name()?;
    let settings =
        super::misc_eval::dynamic_or_global_symbol_value(eval, "treesit-thing-settings")?;
    let find_entry = |mut alist: Value, key: &str| {
        while alist.is_cons() {
            let entry = alist.cons_car();
            if entry.is_cons() && entry.cons_car().as_symbol_name() == Some(key) {
                return Some(entry);
            }
            alist = alist.cons_cdr();
        }
        None
    };

    let language_entry = find_entry(settings, language_name)?;
    let definition_entry = find_entry(language_entry.cons_cdr(), thing_name)?;
    let definition = definition_entry.cons_cdr();
    if definition.is_cons() {
        Some(definition.cons_car())
    } else {
        None
    }
}

fn treesit_predicate_not_found(predicate: Value) -> Flow {
    signal(LispCondition::TreesitPredicateNotFound, vec![predicate])
}

fn treesit_invalid_predicate(message: impl Into<String>, predicate: Value) -> Flow {
    signal(
        "treesit-invalid-predicate",
        vec![Value::string(message.into()), predicate],
    )
}

fn predicate_function_p(eval: &super::eval::Context, predicate: Value) -> bool {
    if predicate.is_nil() {
        return false;
    }
    if predicate.as_symbol_name().is_some() {
        return eval
            .obarray()
            .symbol_function_of_value(&predicate)
            .is_some();
    }
    true
}

fn call_node_predicate(
    eval: &mut super::eval::Context,
    predicate: Value,
    parser_id: u64,
    node: tree_sitter::Node<'_>,
) -> Result<bool, Flow> {
    let node_value = make_node_value_for_parser(eval, parser_id, node);
    Ok(!eval.funcall_general(predicate, vec![node_value])?.is_nil())
}

fn predicate_matches_node(
    eval: &mut super::eval::Context,
    parser_id: u64,
    parser_value: Value,
    node: tree_sitter::Node<'_>,
    predicate: Value,
    named_only: bool,
    ignore_missing: bool,
) -> Result<bool, Flow> {
    if named_only && !node.is_named() {
        return Ok(false);
    }

    if let Some(pattern) = eval.lisp_string(predicate) {
        return crate::emacs_core::regex::predicate_match(pattern, node.kind())
            .map_err(|err| treesit_invalid_predicate(err.to_string(), predicate));
    }

    if let Some(builtin) = TreesitBuiltinPredicate::from_symbol_value(predicate) {
        return Ok(builtin.matches_node(node));
    }

    if let Some(definition) = lookup_thing_definition(
        eval,
        parser_record_slot(parser_value, PARSER_SLOT_LANGUAGE)?,
        predicate,
    ) {
        return predicate_matches_node(
            eval,
            parser_id,
            parser_value,
            node,
            definition,
            named_only,
            ignore_missing,
        );
    }

    if predicate.as_symbol_name().is_some() && !predicate_function_p(eval, predicate) {
        return if ignore_missing {
            Ok(false)
        } else {
            Err(treesit_predicate_not_found(predicate))
        };
    }

    if predicate_function_p(eval, predicate) && !predicate.is_cons() {
        return call_node_predicate(eval, predicate, parser_id, node);
    }

    if !predicate.is_cons() {
        return Err(treesit_invalid_predicate(
            "Unsupported tree-sitter predicate",
            predicate,
        ));
    }

    let head = predicate.cons_car();
    let tail = predicate.cons_cdr();
    if let Some(boolean_predicate) = TreesitBooleanPredicate::from_symbol_value(head) {
        return match boolean_predicate {
            TreesitBooleanPredicate::Not => {
                let args = crate::emacs_core::value::list_to_vec(&tail).ok_or_else(|| {
                    treesit_invalid_predicate("`not' expects one predicate", predicate)
                })?;
                if args.len() != 1 {
                    return Err(treesit_invalid_predicate(
                        "`not' expects one predicate",
                        predicate,
                    ));
                }
                Ok(!predicate_matches_node(
                    eval,
                    parser_id,
                    parser_value,
                    node,
                    args[0],
                    named_only,
                    ignore_missing,
                )?)
            }
            TreesitBooleanPredicate::Or | TreesitBooleanPredicate::And => {
                let args = crate::emacs_core::value::list_to_vec(&tail).ok_or_else(|| {
                    treesit_invalid_predicate(
                        "`or' or `and' must have a list of patterns as arguments ",
                        predicate,
                    )
                })?;
                if args.is_empty() {
                    return Err(treesit_invalid_predicate(
                        "`or' or `and' must have a list of patterns as arguments ",
                        predicate,
                    ));
                }
                match boolean_predicate {
                    TreesitBooleanPredicate::Or => {
                        for item in args {
                            if predicate_matches_node(
                                eval,
                                parser_id,
                                parser_value,
                                node,
                                item,
                                named_only,
                                ignore_missing,
                            )? {
                                return Ok(true);
                            }
                        }
                        Ok(false)
                    }
                    TreesitBooleanPredicate::And => {
                        for item in args {
                            if !predicate_matches_node(
                                eval,
                                parser_id,
                                parser_value,
                                node,
                                item,
                                named_only,
                                ignore_missing,
                            )? {
                                return Ok(false);
                            }
                        }
                        Ok(true)
                    }
                    TreesitBooleanPredicate::Not => unreachable!(),
                }
            }
        };
    }

    if let Some(pattern) = eval.lisp_string(head) {
        if !predicate_function_p(eval, tail) {
            return Err(treesit_invalid_predicate(
                "Dotted tree-sitter predicates expect a callable cdr",
                predicate,
            ));
        }
        if !crate::emacs_core::regex::predicate_match(pattern, node.kind())
            .map_err(|err| treesit_invalid_predicate(err.to_string(), predicate))?
        {
            return Ok(false);
        }
        return call_node_predicate(eval, tail, parser_id, node);
    }

    Err(treesit_invalid_predicate(
        "Malformed tree-sitter predicate",
        predicate,
    ))
}

fn first_child_for_search(
    node: tree_sitter::Node<'_>,
    forward: bool,
    named_only: bool,
) -> Option<tree_sitter::Node<'_>> {
    if named_only {
        let count = node.named_child_count();
        if count == 0 {
            None
        } else if forward {
            node.named_child(0)
        } else {
            node.named_child((count - 1) as u32)
        }
    } else {
        let count = node.child_count();
        if count == 0 {
            None
        } else if forward {
            node.child(0)
        } else {
            node.child((count - 1) as u32)
        }
    }
}

fn sibling_for_search(
    node: tree_sitter::Node<'_>,
    forward: bool,
    named_only: bool,
) -> Option<tree_sitter::Node<'_>> {
    match (forward, named_only) {
        (true, true) => node.next_named_sibling(),
        (true, false) => node.next_sibling(),
        (false, true) => node.prev_named_sibling(),
        (false, false) => node.prev_sibling(),
    }
}

fn descend_to_leaf(mut node: tree_sitter::Node<'_>, forward: bool) -> tree_sitter::Node<'_> {
    while let Some(child) = first_child_for_search(node, forward, false) {
        node = child;
    }
    node
}

#[allow(clippy::too_many_arguments)] // tree-sitter traversal state is explicit and allocation-free
fn search_subtree_impl<'tree>(
    eval: &mut super::eval::Context,
    parser_id: u64,
    parser_value: Value,
    node: tree_sitter::Node<'tree>,
    predicate: Value,
    forward: bool,
    named_only: bool,
    depth: i64,
    skip_root: bool,
) -> Result<Option<tree_sitter::Node<'tree>>, Flow> {
    if !skip_root
        && predicate_matches_node(
            eval,
            parser_id,
            parser_value,
            node,
            predicate,
            named_only,
            false,
        )?
    {
        return Ok(Some(node));
    }
    if depth == 0 {
        return Ok(None);
    }
    let Some(mut child) = first_child_for_search(node, forward, named_only) else {
        return Ok(None);
    };
    loop {
        if let Some(found) = search_subtree_impl(
            eval,
            parser_id,
            parser_value,
            child,
            predicate,
            forward,
            named_only,
            depth - 1,
            false,
        )? {
            return Ok(Some(found));
        }
        let Some(next) = sibling_for_search(child, forward, false) else {
            break;
        };
        child = next;
    }
    Ok(None)
}

fn search_forward_impl<'tree>(
    eval: &mut super::eval::Context,
    parser_id: u64,
    parser_value: Value,
    start: tree_sitter::Node<'tree>,
    predicate: Value,
    forward: bool,
    named_only: bool,
) -> Result<Option<tree_sitter::Node<'tree>>, Flow> {
    let mut current = start;
    loop {
        if let Some(sibling) = sibling_for_search(current, forward, named_only) {
            let candidate = descend_to_leaf(sibling, forward);
            if predicate_matches_node(
                eval,
                parser_id,
                parser_value,
                candidate,
                predicate,
                named_only,
                false,
            )? {
                return Ok(Some(candidate));
            }
            current = candidate;
        }
        if sibling_for_search(current, forward, named_only).is_some() {
            continue;
        }
        let Some(parent) = current.parent() else {
            return Ok(None);
        };
        current = parent;
        if predicate_matches_node(
            eval,
            parser_id,
            parser_value,
            current,
            predicate,
            named_only,
            false,
        )? {
            return Ok(Some(current));
        }
    }
}

fn subtree_stats(node: tree_sitter::Node<'_>) -> (i64, i64, i64) {
    let child_count = node.child_count() as i64;
    let mut max_depth = 1;
    let mut max_width = child_count;
    let mut count = 1;
    for idx in 0..node.child_count() {
        if let Some(child) = node.child(idx as u32) {
            let (child_depth, child_width, child_count) = subtree_stats(child);
            max_depth = max_depth.max(child_depth + 1);
            max_width = max_width.max(child_width);
            count += child_count;
        }
    }
    (max_depth, max_width, count)
}

fn build_sparse_tree(
    eval: &mut super::eval::Context,
    parser_id: u64,
    parser_value: Value,
    node: tree_sitter::Node<'_>,
    predicate: Value,
    process_fn: Value,
    depth: i64,
) -> Result<Option<Value>, Flow> {
    let matched =
        predicate_matches_node(eval, parser_id, parser_value, node, predicate, false, true)?;
    let mut children = Vec::new();
    if depth != 0 {
        for idx in 0..node.child_count() {
            if let Some(child) = node.child(idx as u32)
                && let Some(item) = build_sparse_tree(
                    eval,
                    parser_id,
                    parser_value,
                    child,
                    predicate,
                    process_fn,
                    depth.saturating_sub(1),
                )?
            {
                children.push(item);
            }
        }
    }
    if !matched && children.is_empty() {
        return Ok(None);
    }
    let payload = if matched {
        let node_value = make_node_value_for_parser(eval, parser_id, node);
        if process_fn.is_nil() {
            node_value
        } else {
            eval.funcall_general(process_fn, vec![node_value])?
        }
    } else {
        Value::NIL
    };
    Ok(Some(Value::cons(payload, Value::list(children))))
}

pub(crate) fn builtin_treesit_available_p(args: Vec<Value>) -> EvalResult {
    expect_args("treesit-available-p", &args, 0)?;
    Ok(Value::T)
}

pub(crate) fn builtin_treesit_compiled_query_p(args: Vec<Value>) -> EvalResult {
    expect_args("treesit-compiled-query-p", &args, 1)?;
    Ok(if runtime::is_compiled_query(args[0]) {
        Value::T
    } else {
        Value::NIL
    })
}

pub(crate) fn builtin_treesit_induce_sparse_tree(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-induce-sparse-tree", &args, 2, 4)?;
    let depth = match args.get(3).copied().unwrap_or(Value::NIL) {
        value if value.is_nil() => 1000,
        value => expect_fixnum(&value)?,
    };
    let handle = ensure_current_node(eval, "treesit-induce-sparse-tree", args[0])?;
    let parser_value = eval
        .treesit
        .parser(handle.parser_id)
        .ok_or_else(|| node_outdated_error(args[0]))?
        .value;
    let root = unsafe { tree_sitter::Node::from_raw(handle.raw) };
    match build_sparse_tree(
        eval,
        handle.parser_id,
        parser_value,
        root,
        args[1],
        args.get(2).copied().unwrap_or(Value::NIL),
        depth,
    ) {
        Ok(Some(tree)) => Ok(tree),
        Ok(None) => Ok(Value::NIL),
        Err(Flow::Signal(sig)) if sig.symbol == intern("treesit-predicate-not-found") => {
            Ok(Value::NIL)
        }
        Err(err) => Err(err),
    }
}

pub(crate) fn builtin_treesit_language_abi_version(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-language-abi-version", &args, 0, 1)?;
    let Some(language_arg) = args.first() else {
        return Ok(Value::NIL);
    };
    if language_arg.is_nil() {
        return Ok(Value::NIL);
    }
    let language = parse_symbol_arg("treesit-language-abi-version", language_arg)?;
    match load_language(eval, language) {
        Ok((loaded, _)) => Ok(Value::fixnum(loaded.abi_version() as i64)),
        Err(_) => Ok(Value::NIL),
    }
}

pub(crate) fn builtin_treesit_language_available_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-language-available-p", &args, 1, 2)?;
    let language = parse_symbol_arg("treesit-language-available-p", &args[0])?;
    let detail = args.get(1).is_some_and(|value| !value.is_nil());
    match load_language(eval, language) {
        Ok(_) if detail => Ok(Value::cons(Value::T, Value::NIL)),
        Ok(_) => Ok(Value::T),
        Err(data) if detail => Ok(Value::cons(Value::NIL, data)),
        Err(_) => Ok(Value::NIL),
    }
}

pub(crate) fn builtin_treesit_library_abi_version(args: Vec<Value>) -> EvalResult {
    expect_args_range("treesit-library-abi-version", &args, 0, 1)?;
    if args.first().is_some_and(|value| !value.is_nil()) {
        Ok(Value::fixnum(MIN_COMPATIBLE_LANGUAGE_VERSION as i64))
    } else {
        Ok(Value::fixnum(LANGUAGE_VERSION as i64))
    }
}

pub(crate) fn builtin_treesit_node_check(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-node-check", &args, 2)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let property_name = args[1].as_symbol_name().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![
                Value::symbol("symbolp"),
                args[1],
                Value::symbol("treesit-node-check"),
            ],
        )
    })?;
    let Some(property) = TreesitNodeProperty::from_symbol_name(property_name) else {
        return Err(signal(
            "error",
            vec![
                Value::string(
                    "Expecting `named', `missing', `extra', `outdated', `has-error', or `live'",
                ),
                args[1],
            ],
        ));
    };

    let handle = expect_node_handle(eval, "treesit-node-check", args[0])?;
    let parser = eval
        .treesit
        .parser(handle.parser_id)
        .ok_or_else(|| node_outdated_error(args[0]))?;

    if property == TreesitNodeProperty::Outdated {
        return Ok(if parser.generation == handle.generation {
            Value::NIL
        } else {
            Value::T
        });
    }

    let node = ensure_current_node(eval, "treesit-node-check", args[0])?;
    let ts_node = unsafe { tree_sitter::Node::from_raw(node.raw) };
    let result = match property {
        TreesitNodeProperty::Named => ts_node.is_named(),
        TreesitNodeProperty::Missing => ts_node.is_missing(),
        TreesitNodeProperty::Extra => ts_node.is_extra(),
        TreesitNodeProperty::HasError => ts_node.has_error(),
        TreesitNodeProperty::Live => parser_live_p(eval, handle.parser_id),
        TreesitNodeProperty::Outdated => unreachable!(),
    };
    Ok(if result { Value::T } else { Value::NIL })
}

pub(crate) fn builtin_treesit_node_child(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-node-child", &args, 2, 3)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let handle = ensure_current_node(eval, "treesit-node-child", args[0])?;
    let mut idx = expect_int(&args[1])?;
    let named = args.get(2).is_some_and(|value| !value.is_nil());
    let node = unsafe { tree_sitter::Node::from_raw(handle.raw) };
    let count = if named {
        node.named_child_count() as i64
    } else {
        node.child_count() as i64
    };
    if idx < 0 {
        idx += count;
    }
    if idx < 0 {
        return Ok(Value::NIL);
    }
    let idx =
        u32::try_from(idx).map_err(|_| signal(LispCondition::ArgsOutOfRange, vec![args[1]]))?;
    let child = if named {
        node.named_child(idx)
    } else {
        node.child(idx)
    };
    Ok(child.map_or(Value::NIL, |child| {
        make_node_value_for_parser(eval, handle.parser_id, child)
    }))
}

pub(crate) fn builtin_treesit_node_child_by_field_name(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-node-child-by-field-name", &args, 2)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let handle = ensure_current_node(eval, "treesit-node-child-by-field-name", args[0])?;
    let field_name = expect_string_lossy(&args[1])?;
    let node = unsafe { tree_sitter::Node::from_raw(handle.raw) };
    Ok(node
        .child_by_field_name(&field_name)
        .map_or(Value::NIL, |child| {
            make_node_value_for_parser(eval, handle.parser_id, child)
        }))
}

pub(crate) fn builtin_treesit_node_child_count(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-node-child-count", &args, 1, 2)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let handle = ensure_current_node(eval, "treesit-node-child-count", args[0])?;
    let named = args.get(1).is_some_and(|value| !value.is_nil());
    let node = unsafe { tree_sitter::Node::from_raw(handle.raw) };
    Ok(Value::fixnum(if named {
        node.named_child_count() as i64
    } else {
        node.child_count() as i64
    }))
}

pub(crate) fn builtin_treesit_node_descendant_for_range(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-node-descendant-for-range", &args, 3, 4)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let handle = ensure_current_node(eval, "treesit-node-descendant-for-range", args[0])?;
    let parser = eval
        .treesit
        .parser(handle.parser_id)
        .ok_or_else(|| node_outdated_error(args[0]))?;
    let buf = eval
        .buffers
        .get(parser.orig_buffer_id)
        .ok_or_else(|| node_buffer_killed_error(args[0]))?;
    let start_byte = treesit_position_to_relative_byte(buf, args[1])?;
    let end_byte = treesit_position_to_relative_byte(buf, args[2])?;
    let named = args.get(3).is_some_and(|value| !value.is_nil());
    let node = unsafe { tree_sitter::Node::from_raw(handle.raw) };
    let descendant = if named {
        node.named_descendant_for_byte_range(start_byte, end_byte)
    } else {
        node.descendant_for_byte_range(start_byte, end_byte)
    };
    Ok(descendant.map_or(Value::NIL, |child| {
        make_node_value_for_parser(eval, handle.parser_id, child)
    }))
}

pub(crate) fn builtin_treesit_node_end(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-node-end", &args, 1)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let handle = ensure_current_node(eval, "treesit-node-end", args[0])?;
    let parser = eval
        .treesit
        .parser(handle.parser_id)
        .ok_or_else(|| node_outdated_error(args[0]))?;
    let buf = eval
        .buffers
        .get(parser.orig_buffer_id)
        .ok_or_else(|| node_buffer_killed_error(args[0]))?;
    let source = parser
        .last_source
        .as_ref()
        .ok_or_else(|| node_outdated_error(args[0]))?;
    let node = unsafe { tree_sitter::Node::from_raw(handle.raw) };
    Ok(byte_offset_to_lisp_pos(buf, source, node.end_byte()))
}

pub(crate) fn builtin_treesit_node_eq(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-node-eq", &args, 2)?;
    if args[0].is_nil() || args[1].is_nil() {
        return Ok(Value::NIL);
    }
    let left = ensure_current_node(eval, "treesit-node-eq", args[0])?;
    let right = ensure_current_node(eval, "treesit-node-eq", args[1])?;
    let equal = left.parser_id == right.parser_id
        && left.generation == right.generation
        && unsafe {
            tree_sitter::Node::from_raw(left.raw) == tree_sitter::Node::from_raw(right.raw)
        };
    Ok(if equal { Value::T } else { Value::NIL })
}

pub(crate) fn builtin_treesit_node_field_name_for_child(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-node-field-name-for-child", &args, 2)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let handle = ensure_current_node(eval, "treesit-node-field-name-for-child", args[0])?;
    let mut idx = expect_int(&args[1])?;
    let node = unsafe { tree_sitter::Node::from_raw(handle.raw) };
    let count = node.child_count() as i64;
    if idx < 0 {
        idx += count;
    }
    if idx < 0 {
        return Ok(Value::NIL);
    }
    let idx =
        u32::try_from(idx).map_err(|_| signal(LispCondition::ArgsOutOfRange, vec![args[1]]))?;
    Ok(node
        .field_name_for_child(idx)
        .map_or(Value::NIL, Value::string))
}

pub(crate) fn builtin_treesit_node_first_child_for_pos(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-node-first-child-for-pos", &args, 2, 3)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let handle = ensure_current_node(eval, "treesit-node-first-child-for-pos", args[0])?;
    let parser = eval
        .treesit
        .parser(handle.parser_id)
        .ok_or_else(|| node_outdated_error(args[0]))?;
    let buf = eval
        .buffers
        .get(parser.orig_buffer_id)
        .ok_or_else(|| node_buffer_killed_error(args[0]))?;
    let byte = treesit_position_to_relative_byte(buf, args[1])?;
    let named = args.get(2).is_some_and(|value| !value.is_nil());
    let node = unsafe { tree_sitter::Node::from_raw(handle.raw) };
    let child = if named {
        node.first_named_child_for_byte(byte)
    } else {
        node.first_child_for_byte(byte)
    };
    Ok(child.map_or(Value::NIL, |child| {
        make_node_value_for_parser(eval, handle.parser_id, child)
    }))
}

pub(crate) fn builtin_treesit_node_match_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-node-match-p", &args, 2, 3)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let ignore_missing = args.get(2).is_some_and(|value| !value.is_nil());
    let handle = ensure_current_node(eval, "treesit-node-match-p", args[0])?;
    let parser_value = eval
        .treesit
        .parser(handle.parser_id)
        .ok_or_else(|| node_outdated_error(args[0]))?
        .value;
    let matched = predicate_matches_node(
        eval,
        handle.parser_id,
        parser_value,
        unsafe { tree_sitter::Node::from_raw(handle.raw) },
        args[1],
        false,
        ignore_missing,
    )?;
    Ok(if matched { Value::T } else { Value::NIL })
}

pub(crate) fn builtin_treesit_node_next_sibling(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-node-next-sibling", &args, 1, 2)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let handle = ensure_current_node(eval, "treesit-node-next-sibling", args[0])?;
    let named = args.get(1).is_some_and(|value| !value.is_nil());
    let node = unsafe { tree_sitter::Node::from_raw(handle.raw) };
    let sibling = if named {
        node.next_named_sibling()
    } else {
        node.next_sibling()
    };
    Ok(sibling.map_or(Value::NIL, |sibling| {
        make_node_value_for_parser(eval, handle.parser_id, sibling)
    }))
}

pub(crate) fn builtin_treesit_node_p(args: Vec<Value>) -> EvalResult {
    expect_args("treesit-node-p", &args, 1)?;
    Ok(if runtime::is_node(args[0]) {
        Value::T
    } else {
        Value::NIL
    })
}

pub(crate) fn builtin_treesit_node_parent(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-node-parent", &args, 1)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let handle = ensure_current_node(eval, "treesit-node-parent", args[0])?;
    let node = unsafe { tree_sitter::Node::from_raw(handle.raw) };
    Ok(node.parent().map_or(Value::NIL, |parent| {
        make_node_value_for_parser(eval, handle.parser_id, parent)
    }))
}

pub(crate) fn builtin_treesit_node_parser(args: Vec<Value>) -> EvalResult {
    expect_args("treesit-node-parser", &args, 1)?;
    let Some(node) = args[0].as_record_data() else {
        return Err(node_type_error("treesit-node-parser", args[0]));
    };
    Ok(node.get(NODE_SLOT_PARSER).copied().unwrap_or(Value::NIL))
}

pub(crate) fn builtin_treesit_node_prev_sibling(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-node-prev-sibling", &args, 1, 2)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let handle = ensure_current_node(eval, "treesit-node-prev-sibling", args[0])?;
    let named = args.get(1).is_some_and(|value| !value.is_nil());
    let node = unsafe { tree_sitter::Node::from_raw(handle.raw) };
    let sibling = if named {
        node.prev_named_sibling()
    } else {
        node.prev_sibling()
    };
    Ok(sibling.map_or(Value::NIL, |sibling| {
        make_node_value_for_parser(eval, handle.parser_id, sibling)
    }))
}

pub(crate) fn builtin_treesit_node_start(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-node-start", &args, 1)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let handle = ensure_current_node(eval, "treesit-node-start", args[0])?;
    let parser = eval
        .treesit
        .parser(handle.parser_id)
        .ok_or_else(|| node_outdated_error(args[0]))?;
    let buf = eval
        .buffers
        .get(parser.orig_buffer_id)
        .ok_or_else(|| node_buffer_killed_error(args[0]))?;
    let source = parser
        .last_source
        .as_ref()
        .ok_or_else(|| node_outdated_error(args[0]))?;
    let node = unsafe { tree_sitter::Node::from_raw(handle.raw) };
    Ok(byte_offset_to_lisp_pos(buf, source, node.start_byte()))
}

pub(crate) fn builtin_treesit_node_string(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-node-string", &args, 1)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let handle = ensure_current_node(eval, "treesit-node-string", args[0])?;
    let node = unsafe { tree_sitter::Node::from_raw(handle.raw) };
    Ok(Value::string(node.to_sexp()))
}

pub(crate) fn builtin_treesit_node_type(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-node-type", &args, 1)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let handle = ensure_current_node(eval, "treesit-node-type", args[0])?;
    let node = unsafe { tree_sitter::Node::from_raw(handle.raw) };
    Ok(Value::string(node.kind()))
}

pub(crate) fn builtin_treesit_parser_add_notifier(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-parser-add-notifier", &args, 2)?;
    let _ = expect_live_parser_id(eval, "treesit-parser-add-notifier", args[0])?;
    if args[1].as_symbol_name().is_none() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[1]],
        ));
    }
    let mut items =
        crate::emacs_core::value::list_to_vec(&parser_record_slot(args[0], PARSER_SLOT_NOTIFIERS)?)
            .unwrap_or_default();
    if !items.contains(&args[1]) {
        items.insert(0, args[1]);
        if !args[0].set_record_slot(PARSER_SLOT_NOTIFIERS, Value::list(items)) {
            return Err(signal(
                "error",
                vec![Value::string("Failed to update parser notifiers")],
            ));
        }
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_treesit_parser_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-parser-buffer", &args, 1)?;
    let parser_id = expect_live_parser_id(eval, "treesit-parser-buffer", args[0])?;
    let parser = eval
        .treesit
        .parser(parser_id)
        .ok_or_else(|| parser_deleted_error(args[0]))?;
    Ok(Value::make_buffer(parser.orig_buffer_id))
}

pub(crate) fn builtin_treesit_parser_create(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-parser-create", &args, 1, 4)?;
    let language = parse_symbol_arg("treesit-parser-create", &args[0])?;
    let tag = expect_symbol_or_nil(
        "treesit-parser-create",
        args.get(3).copied().unwrap_or(Value::NIL),
    )?;
    if tag.is_t() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::list(vec![Value::symbol("not"), Value::T]), Value::T],
        ));
    }

    let (orig_buffer_id, root_buffer_id, buffer_value) = resolve_buffer_ids(eval, args.get(1))?;
    if args.get(2).is_none_or(|value| value.is_nil())
        && let Some(existing) = eval
            .treesit
            .find_reusable_parser(orig_buffer_id, language, tag)
    {
        return Ok(existing);
    }

    let (loaded_language, _) = load_language(eval, language).map_err(|detail| {
        signal(
            "error",
            vec![
                Value::string(format!(
                    "Failed to load tree-sitter language `{}`",
                    resolve_sym(language)
                )),
                detail,
            ],
        )
    })?;
    let mut parser = Parser::new();
    parser
        .set_language(&loaded_language)
        .map_err(|err| signal("error", vec![Value::string(format!("ABI mismatch: {err}"))]))?;

    let tracking_linecol = eval.treesit.linecol_cache(orig_buffer_id).is_some()
        || language_requires_linecol_tracking(eval, language);
    if tracking_linecol {
        eval.treesit.enable_linecol_tracking(orig_buffer_id);
    }
    let placeholder = Value::NIL;
    let id = eval.treesit.insert_parser(
        placeholder,
        orig_buffer_id,
        root_buffer_id,
        language,
        tag,
        parser,
        tracking_linecol,
    );
    let value = runtime::make_parser_value(id, Value::symbol(language), buffer_value, tag);
    let entry = eval.treesit.parser_mut(id).ok_or_else(|| {
        signal(
            "error",
            vec![Value::string("Failed to register tree-sitter parser")],
        )
    })?;
    entry.value = value;
    Ok(value)
}

pub(crate) fn builtin_treesit_parser_delete(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-parser-delete", &args, 1)?;
    let parser_id = expect_live_parser_id(eval, "treesit-parser-delete", args[0])?;
    let (need_to_gc_buffer, buffer_id) = {
        let parser = eval
            .treesit
            .parser(parser_id)
            .ok_or_else(|| parser_deleted_error(args[0]))?;
        (parser.need_to_gc_buffer, parser.orig_buffer_id)
    };
    let _ = eval.treesit.mark_parser_deleted(parser_id);
    if need_to_gc_buffer {
        let _ = eval.buffers.kill_buffer(buffer_id);
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_treesit_parser_included_ranges(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-parser-included-ranges", &args, 1)?;
    let _ = expect_live_parser_id(eval, "treesit-parser-included-ranges", args[0])?;
    parser_record_slot(args[0], runtime::PARSER_SLOT_INCLUDED_RANGES)
}

pub(crate) fn builtin_treesit_parser_language(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-parser-language", &args, 1)?;
    let _ = expect_live_parser_id(eval, "treesit-parser-language", args[0])?;
    parser_record_slot(args[0], PARSER_SLOT_LANGUAGE)
}

pub(crate) fn builtin_treesit_parser_list(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-parser-list", &args, 0, 3)?;
    let (orig_buffer_id, root_buffer_id, _) = resolve_buffer_ids(eval, args.first())?;
    let language = match args.get(1).copied().unwrap_or(Value::NIL) {
        value if value.is_nil() => None,
        value => Some(parse_symbol_arg("treesit-parser-list", &value)?),
    };
    let tag = args.get(2).copied().unwrap_or(Value::NIL);
    let tag_filter = if tag.is_t() {
        ParserTagFilter::Any
    } else {
        expect_symbol_or_nil("treesit-parser-list", tag)?;
        ParserTagFilter::Exact(tag)
    };
    let items =
        eval.treesit
            .parser_values_for(root_buffer_id, orig_buffer_id, language, tag_filter);
    Ok(Value::list(items))
}

pub(crate) fn builtin_treesit_parser_notifiers(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-parser-notifiers", &args, 1)?;
    let _ = expect_live_parser_id(eval, "treesit-parser-notifiers", args[0])?;
    parser_record_slot(args[0], PARSER_SLOT_NOTIFIERS)
}

pub(crate) fn builtin_treesit_parser_p(args: Vec<Value>) -> EvalResult {
    expect_args("treesit-parser-p", &args, 1)?;
    Ok(if runtime::is_parser(args[0]) {
        Value::T
    } else {
        Value::NIL
    })
}

pub(crate) fn builtin_treesit_parser_remove_notifier(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-parser-remove-notifier", &args, 2)?;
    let _ = expect_live_parser_id(eval, "treesit-parser-remove-notifier", args[0])?;
    if args[1].as_symbol_name().is_none() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[1]],
        ));
    }
    let items =
        crate::emacs_core::value::list_to_vec(&parser_record_slot(args[0], PARSER_SLOT_NOTIFIERS)?)
            .unwrap_or_default()
            .into_iter()
            .filter(|item| *item != args[1])
            .collect::<Vec<_>>();
    if !args[0].set_record_slot(PARSER_SLOT_NOTIFIERS, Value::list(items)) {
        return Err(signal(
            "error",
            vec![Value::string("Failed to update parser notifiers")],
        ));
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_treesit_parser_root_node(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-parser-root-node", &args, 1)?;
    let parser_id = expect_live_parser_id(eval, "treesit-parser-root-node", args[0])?;
    ensure_parser_parsed(eval, parser_id)?;
    let root = {
        let parser = eval
            .treesit
            .parser(parser_id)
            .ok_or_else(|| parser_deleted_error(args[0]))?;
        parser
            .tree
            .as_ref()
            .map(|tree| tree.tree().root_node())
            .map(tree_sitter::Node::into_raw)
            .ok_or_else(|| treesit_parse_error(args[0]))?
    };
    Ok(make_node_value_for_parser(eval, parser_id, unsafe {
        tree_sitter::Node::from_raw(root)
    }))
}

pub(crate) fn builtin_treesit_parser_set_included_ranges(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-parser-set-included-ranges", &args, 2)?;
    let parser_id = expect_live_parser_id(eval, "treesit-parser-set-included-ranges", args[0])?;
    let current_ranges = parser_record_slot(args[0], runtime::PARSER_SLOT_INCLUDED_RANGES)?;
    if current_ranges == args[1] {
        return Ok(Value::NIL);
    }

    let source = {
        let parser = eval
            .treesit
            .parser(parser_id)
            .ok_or_else(|| parser_deleted_error(args[0]))?;
        let buffer = eval.buffers.get(parser.orig_buffer_id).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("Parser buffer has been killed")],
            )
        })?;
        treesit_buffer_source(buffer)
    };
    let buffer = {
        let parser = eval
            .treesit
            .parser(parser_id)
            .ok_or_else(|| parser_deleted_error(args[0]))?;
        eval.buffers.get(parser.orig_buffer_id).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("Parser buffer has been killed")],
            )
        })?
    };

    let ts_ranges = if args[1].is_nil() {
        Vec::new()
    } else {
        let mut last_point = buffer.accessible_char_region().start_lisp().as_i64();
        let range_values = crate::emacs_core::value::list_to_vec(&args[1]).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), args[1]],
            )
        })?;
        let mut hint = runtime::LineColCache {
            line: 1,
            col: 1,
            bytepos: 0,
        };
        let mut ranges = Vec::new();
        for value in range_values {
            let (start_pos, end_pos) =
                validate_treesit_included_range(buffer, value, args[1], &mut last_point)?;
            let start = lisp_pos_to_relative_byte(buffer, start_pos);
            let end = lisp_pos_to_relative_byte(buffer, end_pos);
            let start_point = byte_offset_to_point(&source, start, hint);
            let next_hint = byte_offset_to_linecol(&source, end, hint);
            let end_point = Point {
                row: next_hint.line.saturating_sub(1) as usize,
                column: next_hint.col.saturating_sub(1) as usize,
            };
            hint = next_hint;
            ranges.push(TSRange {
                start_byte: start,
                end_byte: end,
                start_point,
                end_point,
            });
        }
        ranges
    };

    // GNU catches up with the narrowing situation before it changes the ranges
    // (`Ftreesit_parser_set_included_ranges`, `src/treesit.c:2744-2746`).
    let current_revision = {
        let orig_buffer_id = eval
            .treesit
            .parser(parser_id)
            .ok_or_else(|| parser_deleted_error(args[0]))?
            .orig_buffer_id;
        let buffer = eval.buffers.get(orig_buffer_id).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("Parser buffer has been killed")],
            )
        })?;
        let revision = ParserInputRevision::for_buffer(buffer);
        eval.treesit.sync_visible_region(parser_id, buffer);
        revision
    };

    let parser = eval
        .treesit
        .parser_mut(parser_id)
        .ok_or_else(|| parser_deleted_error(args[0]))?;
    parser
        .parser
        .set_included_ranges(&ts_ranges)
        .map_err(|_err| {
            signal(
                "treesit-range-invalid",
                vec![
                    Value::string("Something went wrong when setting ranges"),
                    args[1],
                ],
            )
        })?;
    // GNU only flags a reparse here and keeps the tree (`src/treesit.c:2770`).
    // The next parse diffs the new ranges against it, which is what keeps the
    // affected ranges reported to `treesit--font-lock-mark-ranges-to-fontify`
    // down to the text the range change really touched.
    parser.freshness = if parser.tree.is_some() {
        ParserFreshness::ReparsePending(current_revision)
    } else {
        ParserFreshness::Unparsed
    };
    parser.last_changed_ranges.clear();
    if !args[0].set_record_slot(runtime::PARSER_SLOT_INCLUDED_RANGES, args[1]) {
        return Err(signal(
            "error",
            vec![Value::string("Failed to update parser included ranges")],
        ));
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_treesit_parser_tag(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-parser-tag", &args, 1)?;
    let _ = expect_live_parser_id(eval, "treesit-parser-tag", args[0])?;
    parser_record_slot(args[0], PARSER_SLOT_TAG)
}

pub(crate) fn builtin_treesit_pattern_expand(args: Vec<Value>) -> EvalResult {
    expect_args("treesit-pattern-expand", &args, 1)?;
    Ok(Value::string(expand_pattern_value(args[0])?))
}

#[derive(Clone, Copy)]
struct QueryCaptureNode {
    index: u32,
    node: tree_sitter::ffi::TSNode,
    value: Value,
}

struct QueryMatchNodes {
    pattern_index: usize,
    captures: Vec<QueryCaptureNode>,
}

fn query_predicate_capture(
    query_match: &QueryMatchNodes,
    capture_index: u32,
    capture_names: &[String],
) -> Result<QueryCaptureNode, Flow> {
    query_match
        .captures
        .iter()
        // GNU builds the match's capture list with `cons` before predicate
        // evaluation, so the last occurrence of a repeated capture wins.
        .rev()
        .find(|capture| capture.index == capture_index)
        .copied()
        .ok_or_else(|| {
            let name = capture_names
                .get(capture_index as usize)
                .map(String::as_str)
                .unwrap_or("unknown");
            signal(
                LispCondition::TreesitQueryError,
                vec![
                    Value::string("Cannot find captured node"),
                    Value::symbol(name),
                    Value::string(
                        "A predicate can only refer to captured nodes in the same pattern",
                    ),
                ],
            )
        })
}

fn query_predicate_capture_text(
    source: &LispString,
    query_match: &QueryMatchNodes,
    capture_index: u32,
    capture_names: &[String],
) -> Result<LispString, Flow> {
    let capture = query_predicate_capture(query_match, capture_index, capture_names)?;
    let node = unsafe { tree_sitter::Node::from_raw(capture.node) };
    source
        .slice(node.start_byte(), node.end_byte())
        .ok_or_else(|| treesit_query_error("Captured node falls outside the parser source"))
}

fn query_predicate_parser_source(
    eval: &super::eval::Context,
    parser_id: u64,
) -> Result<&LispString, Flow> {
    eval.treesit
        .parser(parser_id)
        .and_then(|parser| parser.last_source.as_ref())
        .ok_or_else(|| treesit_query_error("Missing tree-sitter parser source"))
}

fn query_predicate_arg_text(
    source: &LispString,
    query_match: &QueryMatchNodes,
    capture_names: &[String],
    arg: &runtime::QueryPredicateArg,
) -> Result<LispString, Flow> {
    match arg {
        runtime::QueryPredicateArg::Capture(index) => {
            query_predicate_capture_text(source, query_match, *index, capture_names)
        }
        runtime::QueryPredicateArg::String(string) => Ok(LispString::from_utf8(string)),
    }
}

fn query_predicate_equal(
    eval: &super::eval::Context,
    parser_id: u64,
    query_match: &QueryMatchNodes,
    capture_names: &[String],
    args: &[runtime::QueryPredicateArg],
) -> Result<bool, Flow> {
    if args.len() != 2 {
        return Err(signal(
            LispCondition::TreesitQueryError,
            vec![
                Value::string("Predicate `equal' requires two arguments but got"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let source = query_predicate_parser_source(eval, parser_id)?;
    let left = query_predicate_arg_text(source, query_match, capture_names, &args[0])?;
    let right = query_predicate_arg_text(source, query_match, capture_names, &args[1])?;
    Ok(left.schars() == right.schars()
        && left.sbytes() == right.sbytes()
        && left.as_bytes() == right.as_bytes())
}

fn query_predicate_match(
    eval: &mut super::eval::Context,
    parser_buffer_id: BufferId,
    parser_source_start: EmacsBytePos,
    query_match: &QueryMatchNodes,
    capture_names: &[String],
    args: &[runtime::QueryPredicateArg],
) -> Result<bool, Flow> {
    if args.len() != 2 {
        return Err(signal(
            LispCondition::TreesitQueryError,
            vec![
                Value::string("Predicate `match?' requires two arguments but got"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let (regexp, capture_index) = match (&args[0], &args[1]) {
        (
            runtime::QueryPredicateArg::String(regexp),
            runtime::QueryPredicateArg::Capture(index),
        )
        | (
            runtime::QueryPredicateArg::Capture(index),
            runtime::QueryPredicateArg::String(regexp),
        ) => (regexp, *index),
        _ => {
            return Err(signal(
                LispCondition::TreesitQueryError,
                vec![
                    Value::string(
                        "Predicate `match?' takes a regexp and a node capture (order doesn't matter), but got",
                    ),
                    Value::fixnum(args.len() as i64),
                ],
            ));
        }
    };
    let capture = query_predicate_capture(query_match, capture_index, capture_names)?;
    let node = unsafe { tree_sitter::Node::from_raw(capture.node) };
    let result = {
        let buffer = eval.buffers.get(parser_buffer_id).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("Parser buffer has been killed")],
            )
        })?;
        let start = parser_source_start
            .get()
            .checked_add(node.start_byte())
            .ok_or_else(|| treesit_query_error("Captured node byte range overflow"))?;
        let end = parser_source_start
            .get()
            .checked_add(node.end_byte())
            .ok_or_else(|| treesit_query_error("Captured node byte range overflow"))?;
        let range = EmacsByteRange::new(EmacsBytePos::new(start), EmacsBytePos::new(end));
        if range.end() > buffer.full_emacs_byte_range().end() {
            return Err(treesit_query_error(
                "Captured node falls outside the parser buffer",
            ));
        }
        crate::emacs_core::regex::treesit_predicate_match_lisp(
            buffer,
            &LispString::from_utf8(regexp),
            range,
        )
    };
    eval.maybe_quit()?;
    result.map_err(super::search::regex_error_signal)
}

fn query_predicate_pred(
    eval: &mut super::eval::Context,
    parser_id: u64,
    parser_buffer_id: BufferId,
    query_match: &QueryMatchNodes,
    capture_names: &[String],
    args: &[runtime::QueryPredicateArg],
) -> Result<bool, Flow> {
    if args.len() < 2 {
        return Err(signal(
            LispCondition::TreesitQueryError,
            vec![
                Value::string("Predicate `pred' requires at least two arguments, but only got"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let runtime::QueryPredicateArg::String(function_name) = &args[0] else {
        return Err(treesit_query_error(
            "Predicate `pred' requires a function name followed by node captures",
        ));
    };
    let function = Value::symbol(function_name);
    let mut nodes = Vec::with_capacity(args.len() - 1);
    for arg in &args[1..] {
        let runtime::QueryPredicateArg::Capture(capture_index) = arg else {
            return Err(treesit_query_error(
                "Predicate `pred' arguments after the function must be node captures",
            ));
        };
        let capture = query_predicate_capture(query_match, *capture_index, capture_names)?;
        nodes.push(capture.value);
    }

    let generation = eval
        .treesit
        .parser(parser_id)
        .ok_or_else(|| treesit_query_error("Missing tree-sitter parser"))?
        .generation;
    let saved_buffer = eval.buffers.current_buffer_id();
    if saved_buffer != Some(parser_buffer_id) {
        eval.set_current_buffer_unrecorded(parser_buffer_id)?;
    }
    let call_result = eval.funcall_general(function, nodes);
    if let Some(saved_buffer) = saved_buffer {
        eval.restore_current_buffer_if_live(saved_buffer);
    }
    let value = call_result?;
    if eval
        .treesit
        .parser(parser_id)
        .is_none_or(|parser| parser.generation != generation)
    {
        return Err(signal(LispCondition::TreesitQueryError, vec![function]));
    }
    Ok(!value.is_nil())
}

fn query_match_passes_predicates(
    eval: &mut super::eval::Context,
    parser_id: u64,
    parser_buffer_id: BufferId,
    parser_source_start: EmacsBytePos,
    query_match: &QueryMatchNodes,
    capture_names: &[String],
    predicates: &[runtime::QueryPredicate],
) -> Result<bool, Flow> {
    for predicate in predicates {
        let Some(runtime::QueryPredicateArg::String(operator)) = predicate.steps.first() else {
            return Err(treesit_query_error(
                "Tree-sitter predicate must start with a function name",
            ));
        };
        let args = &predicate.steps[1..];
        let passed = match operator.as_str() {
            "eq?" => query_predicate_equal(eval, parser_id, query_match, capture_names, args)?,
            "match?" => query_predicate_match(
                eval,
                parser_buffer_id,
                parser_source_start,
                query_match,
                capture_names,
                args,
            )?,
            "pred?" => query_predicate_pred(
                eval,
                parser_id,
                parser_buffer_id,
                query_match,
                capture_names,
                args,
            )?,
            _ => {
                return Err(signal(
                    LispCondition::TreesitQueryError,
                    vec![
                        Value::string("Invalid predicate"),
                        Value::string(operator),
                        Value::string(
                            "Currently Emacs only supports `equal', `match', and `pred' predicates",
                        ),
                    ],
                ));
            }
        };
        if !passed {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) fn builtin_treesit_query_capture(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-query-capture", &args, 2, 6)?;
    let input = resolve_node_input(eval, args[0], "treesit-query-capture")?;
    let compiled_query = resolve_compiled_query_value(
        eval,
        input.language_symbol,
        args[1],
        "treesit-query-capture",
    )?;
    let query_id = runtime::query_id(compiled_query)
        .ok_or_else(|| compiled_query_type_error("treesit-query-capture", compiled_query))?;
    let byte_range = {
        let parser = eval
            .treesit
            .parser(input.parser_id)
            .ok_or_else(|| parser_deleted_error(input.parser_value))?;
        let buf = eval.buffers.get(parser.orig_buffer_id).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("Parser buffer has been killed")],
            )
        })?;
        query_range_bytes(buf, &args, 2, 3)?
    };
    if byte_range
        .as_ref()
        .is_some_and(|range| range.start == 0 && range.end == 0)
    {
        return Ok(Value::NIL);
    }
    let node_only = args.get(4).is_some_and(|value| !value.is_nil());
    let grouped = args.get(5).is_some_and(|value| !value.is_nil());

    let (capture_names, predicates, raw_matches, parser_buffer_id, parser_source_start) = {
        let parser = eval
            .treesit
            .parser(input.parser_id)
            .ok_or_else(|| parser_deleted_error(input.parser_value))?;
        if parser.last_source.is_none() {
            return Err(treesit_parse_error(input.parser_value));
        }
        let query = eval
            .treesit
            .query(query_id)
            .and_then(|entry| entry.compiled.as_ref())
            .ok_or_else(|| compiled_query_type_error("treesit-query-capture", compiled_query))?;
        let capture_names = query.capture_names().to_vec();
        let predicates = query.predicates().to_vec();
        let raw_matches = query
            .matches(input.node_raw, byte_range)
            .map_err(treesit_query_error)?;
        (
            capture_names,
            predicates,
            raw_matches,
            parser.orig_buffer_id,
            eval.buffers
                .get(parser.orig_buffer_id)
                .expect("parser buffer checked above")
                .accessible_emacs_byte_range()
                .start(),
        )
    };

    let root_scope = eval.save_specpdl_roots();
    let result = (|| -> EvalResult {
        let mut matches = Vec::new();
        for raw_match in raw_matches {
            let mut captures = Vec::with_capacity(raw_match.captures.len());
            for raw_capture in raw_match.captures {
                let value = make_node_value_for_parser(eval, input.parser_id, unsafe {
                    tree_sitter::Node::from_raw(raw_capture.node)
                });
                eval.push_specpdl_root(value);
                captures.push(QueryCaptureNode {
                    index: raw_capture.index,
                    node: raw_capture.node,
                    value,
                });
            }
            let query_match = QueryMatchNodes {
                pattern_index: raw_match.pattern_index,
                captures,
            };
            let pattern_predicates = predicates
                .get(query_match.pattern_index)
                .map(Vec::as_slice)
                .unwrap_or_default();
            if query_match_passes_predicates(
                eval,
                input.parser_id,
                parser_buffer_id,
                parser_source_start,
                &query_match,
                &capture_names,
                pattern_predicates,
            )? {
                matches.push(query_match);
            }
        }

        let result = if grouped {
            Value::list(
                matches
                    .into_iter()
                    .map(|query_match| {
                        Value::list(
                            query_match
                                .captures
                                .into_iter()
                                .map(|capture| {
                                    if node_only {
                                        capture.value
                                    } else {
                                        Value::cons(
                                            Value::symbol(&capture_names[capture.index as usize]),
                                            capture.value,
                                        )
                                    }
                                })
                                .collect(),
                        )
                    })
                    .collect(),
            )
        } else {
            Value::list(
                matches
                    .into_iter()
                    .flat_map(|query_match| query_match.captures)
                    .map(|capture| {
                        if node_only {
                            capture.value
                        } else {
                            Value::cons(
                                Value::symbol(&capture_names[capture.index as usize]),
                                capture.value,
                            )
                        }
                    })
                    .collect(),
            )
        };
        Ok(result)
    })();
    eval.restore_specpdl_roots(root_scope);
    result
}

pub(crate) fn builtin_treesit_query_compile(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-query-compile", &args, 2, 3)?;
    let query = args[1];
    if !query_like_p(query) {
        return Err(query_type_error("treesit-query-compile", query));
    }

    let language = expect_symbol_or_nil("treesit-query-compile", args[0])?;
    let eager = args.get(2).is_some_and(|value| !value.is_nil());

    if runtime::is_compiled_query(query) {
        if eager {
            ensure_query_compiled(eval, query)?;
        }
        return Ok(query);
    }

    let language_sym = parse_symbol_arg("treesit-query-compile", &language)?;
    let id = eval.treesit.insert_query(language_sym);
    let value = runtime::make_query_value(id, language, query);
    if eager {
        ensure_query_compiled(eval, value)?;
    }
    Ok(value)
}

pub(crate) fn builtin_treesit_query_eagerly_compiled_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-query-eagerly-compiled-p", &args, 1)?;
    let id = runtime::query_id(args[0])
        .ok_or_else(|| compiled_query_type_error("treesit-query-eagerly-compiled-p", args[0]))?;
    let compiled = eval
        .treesit
        .query(id)
        .and_then(|entry| entry.compiled.as_ref())
        .is_some();
    Ok(if compiled { Value::T } else { Value::NIL })
}

pub(crate) fn builtin_treesit_query_expand(args: Vec<Value>) -> EvalResult {
    expect_args("treesit-query-expand", &args, 1)?;
    let source = match args[0] {
        value if runtime::is_compiled_query(value) => query_record_slot(value, QUERY_SLOT_SOURCE)?,
        value => value,
    };
    Ok(Value::string(expand_query_value(
        "treesit-query-expand",
        source,
    )?))
}

pub(crate) fn builtin_treesit_query_language(args: Vec<Value>) -> EvalResult {
    expect_args("treesit-query-language", &args, 1)?;
    let _ = runtime::query_id(args[0])
        .ok_or_else(|| compiled_query_type_error("treesit-query-language", args[0]))?;
    query_record_slot(args[0], QUERY_SLOT_LANGUAGE)
}

pub(crate) fn builtin_treesit_query_p(args: Vec<Value>) -> EvalResult {
    expect_args("treesit-query-p", &args, 1)?;
    Ok(if query_like_p(args[0]) {
        Value::T
    } else {
        Value::NIL
    })
}

pub(crate) fn builtin_treesit_query_source(args: Vec<Value>) -> EvalResult {
    expect_args("treesit-query-source", &args, 1)?;
    let _ = runtime::query_id(args[0])
        .ok_or_else(|| compiled_query_type_error("treesit-query-source", args[0]))?;
    query_record_slot(args[0], QUERY_SLOT_SOURCE)
}

pub(crate) fn builtin_treesit_search_forward(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-search-forward", &args, 2, 4)?;
    let handle = ensure_current_node(eval, "treesit-search-forward", args[0])?;
    let parser_value = eval
        .treesit
        .parser(handle.parser_id)
        .ok_or_else(|| node_outdated_error(args[0]))?
        .value;
    let forward = args.get(2).is_none_or(|value| value.is_nil());
    let named_only = args.get(3).is_none_or(|value| value.is_nil());
    match search_forward_impl(
        eval,
        handle.parser_id,
        parser_value,
        unsafe { tree_sitter::Node::from_raw(handle.raw) },
        args[1],
        forward,
        named_only,
    ) {
        Ok(Some(node)) => Ok(make_node_value_for_parser(eval, handle.parser_id, node)),
        Ok(None) => Ok(Value::NIL),
        Err(Flow::Signal(sig)) if sig.symbol == intern("treesit-predicate-not-found") => {
            Ok(Value::NIL)
        }
        Err(err) => Err(err),
    }
}

pub(crate) fn builtin_treesit_search_subtree(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-search-subtree", &args, 2, 5)?;
    let handle = ensure_current_node(eval, "treesit-search-subtree", args[0])?;
    let parser_value = eval
        .treesit
        .parser(handle.parser_id)
        .ok_or_else(|| node_outdated_error(args[0]))?
        .value;
    let forward = args.get(2).is_none_or(|value| value.is_nil());
    let named_only = args.get(3).is_none_or(|value| value.is_nil());
    let depth = match args.get(4).copied().unwrap_or(Value::NIL) {
        value if value.is_nil() => 1000,
        value => expect_fixnum(&value)?,
    };
    match search_subtree_impl(
        eval,
        handle.parser_id,
        parser_value,
        unsafe { tree_sitter::Node::from_raw(handle.raw) },
        args[1],
        forward,
        named_only,
        depth,
        false,
    ) {
        Ok(Some(node)) => Ok(make_node_value_for_parser(eval, handle.parser_id, node)),
        Ok(None) => Ok(Value::NIL),
        Err(Flow::Signal(sig)) if sig.symbol == intern("treesit-predicate-not-found") => {
            Ok(Value::NIL)
        }
        Err(err) => Err(err),
    }
}

pub(crate) fn builtin_treesit_subtree_stat(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-subtree-stat", &args, 1)?;
    let handle = ensure_current_node(eval, "treesit-subtree-stat", args[0])?;
    let (depth, width, count) = subtree_stats(unsafe { tree_sitter::Node::from_raw(handle.raw) });
    Ok(Value::list(vec![
        Value::fixnum(depth),
        Value::fixnum(width),
        Value::fixnum(count),
    ]))
}

pub(crate) fn builtin_treesit_grammar_location(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-grammar-location", &args, 1)?;
    let language = parse_symbol_arg("treesit-grammar-location", &args[0])?;
    match load_language(eval, language) {
        Ok((_, filename)) => Ok(filename.map_or(Value::NIL, Value::string)),
        Err(_) => Ok(Value::NIL),
    }
}

pub(crate) fn builtin_treesit_tracking_line_column_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("treesit-tracking-line-column-p", &args, 0, 1)?;
    let buffer_id = match args.first().copied().unwrap_or(Value::NIL) {
        value if value.is_nil() => eval
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?,
        value => expect_buffer_id(&value)?,
    };
    Ok(if eval.treesit.linecol_cache(buffer_id).is_some() {
        Value::T
    } else {
        Value::NIL
    })
}

pub(crate) fn builtin_treesit_parser_tracking_line_column_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-parser-tracking-line-column-p", &args, 1)?;
    let parser_id = expect_live_parser_id(eval, "treesit-parser-tracking-line-column-p", args[0])?;
    let tracking = eval
        .treesit
        .parser(parser_id)
        .ok_or_else(|| parser_deleted_error(args[0]))?
        .tracking_linecol;
    Ok(if tracking { Value::T } else { Value::NIL })
}

pub(crate) fn builtin_treesit_parser_embed_level(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-parser-embed-level", &args, 1)?;
    let _ = expect_live_parser_id(eval, "treesit-parser-embed-level", args[0])?;
    parser_record_slot(args[0], PARSER_SLOT_EMBED_LEVEL)
}

pub(crate) fn builtin_treesit_parser_set_embed_level(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-parser-set-embed-level", &args, 2)?;
    let _ = expect_live_parser_id(eval, "treesit-parser-set-embed-level", args[0])?;
    let level = if args[1].is_nil() {
        Value::NIL
    } else {
        let level = expect_wholenump(&args[1])?;
        Value::fixnum(level)
    };
    if !args[0].set_record_slot(PARSER_SLOT_EMBED_LEVEL, level) {
        return Err(signal(
            "error",
            vec![Value::string("Failed to update parser embed level")],
        ));
    }
    Ok(level)
}

pub(crate) fn builtin_treesit_parse_string(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-parse-string", &args, 2)?;
    let language = expect_symbol_or_nil("treesit-parse-string", args[1])?;
    if language.is_nil() {
        return Err(query_type_error("treesit-parse-string", language));
    }
    // Anchored on the heap rather than on the whole `Context`: the borrow
    // still blocks every safepoint (all of them are `&mut Context`) while
    // leaving the disjoint `eval.buffers` mutations below alone.  This is
    // DIVERGENCES.md 175 §5's one measured false positive of
    // `Context::lisp_string`, landed rather than reverted.
    let text = args[0].expect_lisp_string_in(&eval.tagged_heap)?;
    let name = format!(" *treesit-parse-string-{}*", eval.treesit.roots().len() + 1);
    let buffer_id = eval.buffers.create_buffer_with_hook_inhibition(&name, true);
    let saved_current = eval.buffers.current_buffer_id();
    let _ = eval.buffers.switch_current(buffer_id);
    let _ = eval.buffers.insert_lisp_string_into_buffer(buffer_id, text);
    let parser = builtin_treesit_parser_create(
        eval,
        vec![
            language,
            Value::make_buffer(buffer_id),
            Value::T,
            Value::NIL,
        ],
    )?;
    if let Some(parser_id) = runtime::parser_id(parser)
        && let Some(entry) = eval.treesit.parser_mut(parser_id)
    {
        entry.need_to_gc_buffer = true;
    }
    if let Some(saved) = saved_current {
        let _ = eval.buffers.switch_current(saved);
    }
    builtin_treesit_parser_root_node(eval, vec![parser])
}

pub(crate) fn builtin_treesit_parser_changed_regions(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit-parser-changed-regions", &args, 1)?;
    let parser_id = expect_live_parser_id(eval, "treesit-parser-changed-regions", args[0])?;
    let changed_ranges = ensure_parser_parsed_with_changes(eval, parser_id)?;
    let Some(changed_ranges) = changed_ranges else {
        return Ok(Value::NIL);
    };
    if changed_ranges.is_empty() {
        return Ok(Value::NIL);
    }
    let regions = changed_ranges_to_lisp(eval, parser_id, &changed_ranges)?;
    for notifier in
        crate::emacs_core::value::list_to_vec(&parser_record_slot(args[0], PARSER_SLOT_NOTIFIERS)?)
            .unwrap_or_default()
    {
        let _ = eval.funcall_general(notifier, vec![regions, args[0]])?;
    }
    Ok(regions)
}

pub(crate) fn builtin_treesit_linecol_at(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit--linecol-at", &args, 1)?;
    let pos = expect_number(&args[0])? as i64;
    let buffer_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let buffer = eval
        .buffers
        .get(buffer_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let source = treesit_buffer_source(buffer);
    let byte_offset = lisp_pos_to_relative_byte(buffer, pos);
    let hint = eval
        .treesit
        .linecol_cache(buffer_id)
        .unwrap_or(runtime::LineColCache {
            line: 1,
            col: 1,
            bytepos: 0,
        });
    let linecol = byte_offset_to_linecol(&source, byte_offset, hint);
    Ok(Value::cons(
        Value::fixnum(linecol.line),
        Value::fixnum(linecol.col),
    ))
}

pub(crate) fn builtin_treesit_linecol_cache_set(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit--linecol-cache-set", &args, 3)?;
    let buffer_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let line = expect_fixnum(&args[0])?;
    let col = expect_fixnum(&args[1])?;
    let bytepos = expect_fixnum(&args[2])?;
    eval.treesit.set_linecol_cache(
        buffer_id,
        runtime::LineColCache {
            line,
            col,
            bytepos: bytepos.max(0) as usize,
        },
    );
    Ok(Value::NIL)
}

pub(crate) fn builtin_treesit_linecol_cache(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("treesit--linecol-cache", &args, 0)?;
    let buffer_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let Some(cache) = eval.treesit.linecol_cache(buffer_id) else {
        return Ok(Value::NIL);
    };
    let buffer = eval
        .buffers
        .get(buffer_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let source = treesit_buffer_source(buffer);
    let pos = byte_offset_to_lisp_pos(buffer, &source, cache.bytepos);
    Ok(Value::list(vec![
        Value::keyword(":line"),
        Value::fixnum(cache.line),
        Value::keyword(":col"),
        Value::fixnum(cache.col),
        Value::keyword(":pos"),
        pos,
        Value::keyword(":bytepos"),
        Value::fixnum(cache.bytepos as i64),
    ]))
}

#[cfg(test)]
#[path = "tests/treesit_freshness.rs"]
mod freshness_tests;

#[cfg(test)]
mod tests {
    use super::freshness_tests::eval_with_json_parser;
    use super::*;
    use crate::emacs_core::error::Flow;

    fn json_opening_bracket_node(eval: &mut super::super::eval::Context, parser: Value) -> Value {
        let root = builtin_treesit_parser_root_node(eval, vec![parser])
            .expect("root node for json parser");
        let array =
            builtin_treesit_node_child(eval, vec![root, Value::fixnum(0)]).expect("array node");
        builtin_treesit_node_child(eval, vec![array, Value::fixnum(0)])
            .expect("opening bracket node")
    }

    fn expect_signal(
        result: EvalResult,
        symbol: &str,
    ) -> Box<crate::emacs_core::error::SignalData> {
        match result.expect_err("expected signal") {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), symbol);
                sig
            }
            other => panic!("expected {symbol} signal, got {other:?}"),
        }
    }

    fn captured_texts(
        eval: &mut super::super::eval::Context,
        parser: Value,
        query: Value,
    ) -> Vec<String> {
        let captures = builtin_treesit_query_capture(eval, vec![parser, query])
            .expect("tree-sitter query captures");
        crate::emacs_core::value::list_to_vec(&captures)
            .expect("capture list")
            .into_iter()
            .map(|capture| {
                let node_value = capture.cons_cdr();
                let node = ensure_current_node(eval, "test", node_value).expect("current node");
                let parser = eval.treesit.parser(node.parser_id).expect("node parser");
                let source = parser.last_source.as_ref().expect("parsed source");
                let raw = unsafe { tree_sitter::Node::from_raw(node.raw) };
                String::from_utf8_lossy(&source.as_bytes()[raw.start_byte()..raw.end_byte()])
                    .into_owned()
            })
            .collect()
    }

    fn string_capture_query(predicate: Value) -> Value {
        Value::list(vec![Value::list(vec![
            Value::list(vec![Value::symbol("string")]),
            Value::symbol("@item"),
            predicate,
        ])])
    }

    #[test]
    fn treesit_node_property_domain_matches_gnu_symbols() {
        assert_eq!(
            TreesitNodeProperty::from_symbol_name("named"),
            Some(TreesitNodeProperty::Named)
        );
        assert_eq!(
            TreesitNodeProperty::from_symbol_name("missing"),
            Some(TreesitNodeProperty::Missing)
        );
        assert_eq!(
            TreesitNodeProperty::from_symbol_name("extra"),
            Some(TreesitNodeProperty::Extra)
        );
        assert_eq!(
            TreesitNodeProperty::from_symbol_name("outdated"),
            Some(TreesitNodeProperty::Outdated)
        );
        assert_eq!(
            TreesitNodeProperty::from_symbol_name("has-error"),
            Some(TreesitNodeProperty::HasError)
        );
        assert_eq!(
            TreesitNodeProperty::from_symbol_name("live"),
            Some(TreesitNodeProperty::Live)
        );
        assert_eq!(TreesitNodeProperty::from_symbol_name("anonymous"), None);
        assert_eq!(TreesitNodeProperty::HasError.name(), "has-error");
    }

    #[test]
    fn treesit_query_compile_malformed_query_signals_query_error() {
        crate::test_utils::init_test_tracing();
        let mut eval = super::super::eval::Context::new();
        let language_sym = Value::symbol("json").as_symbol_id().expect("json symbol");
        eval.treesit.cache_loaded_language(
            language_sym,
            runtime::LoadedLanguage {
                language: Language::new(tree_sitter_json::LANGUAGE),
                filename: None,
                _library: None,
            },
        );

        let signal = expect_signal(
            builtin_treesit_query_compile(
                &mut eval,
                vec![Value::symbol("json"), Value::string(")"), Value::T],
            ),
            "treesit-query-error",
        );
        assert_eq!(signal.symbol_name(), "treesit-query-error");
    }

    #[test]
    fn treesit_query_compile_accepts_emacs_match_predicate_argument_order() {
        crate::test_utils::init_test_tracing();
        let mut eval = super::super::eval::Context::new();
        let language_sym = Value::symbol("json").as_symbol_id().expect("json symbol");
        eval.treesit.cache_loaded_language(
            language_sym,
            runtime::LoadedLanguage {
                language: Language::new(tree_sitter_json::LANGUAGE),
                filename: None,
                _library: None,
            },
        );
        let query = Value::list(vec![Value::list(vec![
            Value::list(vec![Value::symbol("string")]),
            Value::symbol("@doc"),
            Value::list(vec![
                Value::keyword(":match"),
                Value::string("\\`doc"),
                Value::symbol("@doc"),
            ]),
        ])]);

        let compiled =
            builtin_treesit_query_compile(&mut eval, vec![Value::symbol("json"), query, Value::T]);

        assert!(
            compiled.is_ok(),
            "GNU accepts regexp-first `:match` predicates: {compiled:?}"
        );
    }

    #[test]
    fn treesit_query_capture_filters_regex_first_match_with_emacs_regexp() {
        let (mut eval, parser) = eval_with_json_parser(r#"["doc", "other"]"#);
        let query = string_capture_query(Value::list(vec![
            Value::keyword(":match"),
            Value::string("\\`\\\"doc"),
            Value::symbol("@item"),
        ]));

        assert_eq!(captured_texts(&mut eval, parser, query), vec![r#""doc""#]);
    }

    #[test]
    fn treesit_query_match_is_case_sensitive_even_when_case_fold_search_is_true() {
        let (mut eval, parser) = eval_with_json_parser(r#"["DOC", "doc"]"#);
        eval.eval_str("(setq case-fold-search t)")
            .expect("enable case folding");
        let query = string_capture_query(Value::list(vec![
            Value::keyword(":match"),
            Value::string("doc"),
            Value::symbol("@item"),
        ]));

        assert_eq!(captured_texts(&mut eval, parser, query), vec![r#""doc""#]);
    }

    #[test]
    fn treesit_query_match_uses_parser_buffer_point_for_at_point_anchor() {
        let (mut eval, parser) = eval_with_json_parser(r#"["doc"]"#);
        let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
        eval.buffers
            .goto_buffer_emacs_byte_pos(buffer_id, crate::buffer::EmacsBytePos::new(1));
        let query = string_capture_query(Value::list(vec![
            Value::keyword(":match"),
            Value::string("\\="),
            Value::symbol("@item"),
        ]));

        assert_eq!(captured_texts(&mut eval, parser, query), vec![r#""doc""#]);
    }

    #[test]
    fn treesit_query_capture_filters_equal_with_either_argument_kind() {
        let (mut eval, parser) = eval_with_json_parser(r#"["keep", "drop"]"#);
        let query = string_capture_query(Value::list(vec![
            Value::keyword(":equal"),
            Value::string(r#""keep""#),
            Value::symbol("@item"),
        ]));

        assert_eq!(captured_texts(&mut eval, parser, query), vec![r#""keep""#]);
    }

    #[test]
    fn treesit_query_capture_calls_emacs_predicate_with_captured_nodes() {
        let (mut eval, parser) = eval_with_json_parser(r#"["first", "second"]"#);
        eval.eval_str(
            "(defalias 'neomacs--treesit-first-p (lambda (node) (= (treesit-node-start node) 2)))",
        )
        .expect("predicate function");
        let query = string_capture_query(Value::list(vec![
            Value::keyword(":pred"),
            Value::string("neomacs--treesit-first-p"),
            Value::symbol("@item"),
        ]));

        assert_eq!(captured_texts(&mut eval, parser, query), vec![r#""first""#]);
    }

    #[test]
    fn treesit_query_predicate_runs_in_parser_buffer_and_restores_current_buffer() {
        let (mut eval, parser) = eval_with_json_parser(r#"["first"]"#);
        eval.eval_str(
            "(defalias 'neomacs--treesit-parser-buffer-p
               (lambda (node)
                 (eq (current-buffer)
                     (treesit-parser-buffer (treesit-node-parser node)))))",
        )
        .expect("predicate function");
        let other_buffer = eval.buffers.create_buffer(" *treesit-predicate-other*");
        eval.set_current_buffer_unrecorded(other_buffer)
            .expect("switch current buffer");
        let query = string_capture_query(Value::list(vec![
            Value::keyword(":pred"),
            Value::string("neomacs--treesit-parser-buffer-p"),
            Value::symbol("@item"),
        ]));

        assert_eq!(captured_texts(&mut eval, parser, query), vec![r#""first""#]);
        assert_eq!(eval.buffers.current_buffer_id(), Some(other_buffer));
    }

    #[test]
    fn treesit_query_predicate_restores_parser_buffer_after_callback_switches_away() {
        let (mut eval, parser) = eval_with_json_parser(r#"["first"]"#);
        let parser_buffer = eval.buffers.current_buffer_id().expect("parser buffer");
        eval.buffers.create_buffer(" *treesit-predicate-away*");
        eval.eval_str(
            "(defalias 'neomacs--treesit-switch-buffer-p
               (lambda (node) (set-buffer \" *treesit-predicate-away*\") t))",
        )
        .expect("predicate function");
        let query = string_capture_query(Value::list(vec![
            Value::keyword(":pred"),
            Value::string("neomacs--treesit-switch-buffer-p"),
            Value::symbol("@item"),
        ]));

        assert_eq!(captured_texts(&mut eval, parser, query), vec![r#""first""#]);
        assert_eq!(eval.buffers.current_buffer_id(), Some(parser_buffer));
    }

    #[test]
    fn treesit_query_predicate_receives_the_returned_capture_node() {
        let (mut eval, parser) = eval_with_json_parser(r#"["first"]"#);
        eval.eval_str(
            "(defalias 'neomacs--treesit-save-node-p
               (lambda (node)
                 (setq neomacs--treesit-saved-node node)
                 (garbage-collect)
                 t))",
        )
        .expect("predicate function");
        let query = string_capture_query(Value::list(vec![
            Value::keyword(":pred"),
            Value::string("neomacs--treesit-save-node-p"),
            Value::symbol("@item"),
        ]));

        let captures =
            builtin_treesit_query_capture(&mut eval, vec![parser, query]).expect("query captures");
        let returned_node =
            crate::emacs_core::value::list_to_vec(&captures).expect("capture list")[0].cons_cdr();
        let saved_node = eval
            .eval_str("neomacs--treesit-saved-node")
            .expect("saved predicate node");
        assert_eq!(returned_node, saved_node);
    }

    #[test]
    fn treesit_query_predicate_rejects_parser_buffer_mutation() {
        let (mut eval, parser) = eval_with_json_parser(r#"["first"]"#);
        eval.eval_str("(defalias 'neomacs--treesit-mutating-p (lambda (node) (insert \"x\") t))")
            .expect("predicate function");
        let query = string_capture_query(Value::list(vec![
            Value::keyword(":pred"),
            Value::string("neomacs--treesit-mutating-p"),
            Value::symbol("@item"),
        ]));

        expect_signal(
            builtin_treesit_query_capture(&mut eval, vec![parser, query]),
            "treesit-query-error",
        );
    }

    #[test]
    fn treesit_query_capture_rejects_predicates_gnu_does_not_support() {
        let (mut eval, parser) = eval_with_json_parser(r#"["first"]"#);
        let query = Value::string(r#"(string) @item (#not-eq? @item "\"other\"")"#);

        expect_signal(
            builtin_treesit_query_capture(&mut eval, vec![parser, query]),
            "treesit-query-error",
        );
    }

    #[test]
    fn treesit_query_capture_empty_range_at_buffer_start_returns_no_matches() {
        let (mut eval, parser) = eval_with_json_parser(r#"["first"]"#);
        let query = Value::string("(string) @item");

        let captures = builtin_treesit_query_capture(
            &mut eval,
            vec![parser, query, Value::fixnum(1), Value::fixnum(1)],
        )
        .expect("empty range query");

        assert!(captures.is_nil());
    }

    #[test]
    fn treesit_predicate_domain_matches_gnu_symbols() {
        assert_eq!(
            TreesitBuiltinPredicate::from_symbol_value(Value::symbol("named")),
            Some(TreesitBuiltinPredicate::Named)
        );
        assert_eq!(
            TreesitBuiltinPredicate::from_symbol_value(Value::symbol("anonymous")),
            Some(TreesitBuiltinPredicate::Anonymous)
        );
        assert_eq!(
            TreesitBuiltinPredicate::from_symbol_value(Value::symbol("missing")),
            None
        );
        assert_eq!(TreesitBuiltinPredicate::Anonymous.name(), "anonymous");

        assert_eq!(
            TreesitBooleanPredicate::from_symbol_value(Value::symbol("not")),
            Some(TreesitBooleanPredicate::Not)
        );
        assert_eq!(
            TreesitBooleanPredicate::from_symbol_value(Value::symbol("or")),
            Some(TreesitBooleanPredicate::Or)
        );
        assert_eq!(
            TreesitBooleanPredicate::from_symbol_value(Value::symbol("and")),
            Some(TreesitBooleanPredicate::And)
        );
        assert_eq!(
            TreesitBooleanPredicate::from_symbol_value(Value::symbol("named")),
            None
        );
        assert_eq!(TreesitBooleanPredicate::And.name(), "and");
    }

    #[test]
    fn treesit_node_match_resolves_a_single_thing_definition_from_gnu_alist_shape() {
        let (mut eval, parser) = eval_with_json_parser(r#"[\"first\"]"#);
        eval.eval_str("(setq treesit-thing-settings '((json (sentence \"array\"))))")
            .expect("single tree-sitter thing definition");
        let root = builtin_treesit_parser_root_node(&mut eval, vec![parser])
            .expect("root node for json parser");
        let array = builtin_treesit_node_child(&mut eval, vec![root, Value::fixnum(0)])
            .expect("array node");

        let matched = builtin_treesit_node_match_p(
            &mut eval,
            vec![array, Value::symbol("sentence"), Value::NIL],
        )
        .expect("defined thing predicate");

        assert_eq!(matched, Value::T);
    }

    #[test]
    fn treesit_node_match_accepts_rx_bracket_character_class() {
        let (mut eval, parser) = eval_with_json_parser("[]");
        let opening_bracket = json_opening_bracket_node(&mut eval, parser);
        // GNU `rx` emits this valid Emacs regexp for `(rx (or "[" "("))`.
        // C mode uses the same bracket character-class form in its tree-sitter
        // thing settings (issue #176).
        let pattern = Value::string("[([]");

        assert_eq!(
            builtin_treesit_node_type(&mut eval, vec![opening_bracket])
                .expect("opening bracket node type"),
            Value::string("[")
        );
        assert_eq!(pattern.as_str_owned().as_deref(), Some("[([]"));
        assert_eq!(
            builtin_treesit_node_match_p(&mut eval, vec![opening_bracket, pattern])
                .expect("Emacs regexp should match anonymous node type"),
            Value::T
        );
    }

    #[test]
    fn treesit_node_match_dotted_predicate_accepts_rx_bracket_character_class() {
        let (mut eval, parser) = eval_with_json_parser("[]");
        let opening_bracket = json_opening_bracket_node(&mut eval, parser);
        let predicate = Value::cons(Value::string("[([]"), Value::symbol("identity"));

        assert_eq!(
            builtin_treesit_node_match_p(&mut eval, vec![opening_bracket, predicate])
                .expect("Emacs regexp should gate the callable predicate"),
            Value::T
        );
    }

    #[test]
    fn treesit_pattern_keyword_domain_matches_gnu_symbols() {
        assert_eq!(pattern_keyword_expansion(":anchor"), Some("."));
        assert_eq!(pattern_keyword_expansion(":?"), Some("?"));
        assert_eq!(pattern_keyword_expansion(":*"), Some("*"));
        assert_eq!(pattern_keyword_expansion(":+"), Some("+"));
        assert_eq!(pattern_keyword_expansion(":equal"), Some("#eq?"));
        assert_eq!(pattern_keyword_expansion(":eq?"), Some("#eq?"));
        assert_eq!(pattern_keyword_expansion(":match"), Some("#match?"));
        assert_eq!(pattern_keyword_expansion(":match?"), Some("#match?"));
        assert_eq!(pattern_keyword_expansion(":pred"), Some("#pred?"));
        assert_eq!(pattern_keyword_expansion(":pred?"), Some("#pred?"));
        assert_eq!(pattern_keyword_expansion(":capture"), None);

        assert_eq!(
            TreesitPatternKeyword::from_symbol_name(":match?"),
            Some(TreesitPatternKeyword::MatchQuestion)
        );
        assert_eq!(TreesitPatternKeyword::EqQuestion.name(), ":eq?");
    }

    #[test]
    fn treesit_parser_set_included_ranges_accepts_valid_fixnum_ranges() {
        let (mut eval, parser) = eval_with_json_parser("{}");
        let ranges = Value::list(vec![Value::cons(Value::fixnum(1), Value::fixnum(3))]);

        assert_eq!(
            builtin_treesit_parser_set_included_ranges(&mut eval, vec![parser, ranges])
                .expect("valid included ranges"),
            Value::NIL
        );
        assert_eq!(
            builtin_treesit_parser_included_ranges(&mut eval, vec![parser])
                .expect("included ranges"),
            ranges
        );
    }

    #[test]
    fn treesit_parser_set_included_ranges_requires_proper_list() {
        let (mut eval, parser) = eval_with_json_parser("{}");
        let bad_ranges = Value::symbol("not-a-list");

        let sig = expect_signal(
            builtin_treesit_parser_set_included_ranges(&mut eval, vec![parser, bad_ranges]),
            "wrong-type-argument",
        );
        assert_eq!(sig.data, vec![Value::symbol("listp"), bad_ranges]);
    }

    #[test]
    fn treesit_parser_set_included_ranges_requires_range_cons() {
        let (mut eval, parser) = eval_with_json_parser("{}");
        let bad_range = Value::fixnum(1);
        let ranges = Value::list(vec![bad_range]);

        let sig = expect_signal(
            builtin_treesit_parser_set_included_ranges(&mut eval, vec![parser, ranges]),
            "wrong-type-argument",
        );
        assert_eq!(sig.data, vec![Value::symbol("consp"), bad_range]);
    }

    #[test]
    fn treesit_parser_set_included_ranges_requires_fixnum_endpoints() {
        let (mut eval, parser) = eval_with_json_parser("{}");
        let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
        let marker = crate::emacs_core::marker::make_registered_buffer_marker(
            &mut eval.buffers,
            buffer_id,
            LispCharPos1::new(1),
            false,
        );
        let ranges = Value::list(vec![Value::cons(marker, Value::fixnum(3))]);

        let sig = expect_signal(
            builtin_treesit_parser_set_included_ranges(&mut eval, vec![parser, ranges]),
            "wrong-type-argument",
        );
        assert_eq!(sig.data, vec![Value::symbol("integerp"), marker]);
    }

    #[test]
    fn treesit_parser_set_included_ranges_rejects_overlapping_ranges() {
        let (mut eval, parser) = eval_with_json_parser("{}");
        let ranges = Value::list(vec![
            Value::cons(Value::fixnum(1), Value::fixnum(3)),
            Value::cons(Value::fixnum(2), Value::fixnum(3)),
        ]);

        let sig = expect_signal(
            builtin_treesit_parser_set_included_ranges(&mut eval, vec![parser, ranges]),
            "treesit-range-invalid",
        );
        assert_eq!(
            sig.data,
            vec![
                Value::string("RANGE is either overlapping, out-of-order or out-of-range"),
                ranges,
            ]
        );
    }

    #[test]
    fn treesit_parser_set_included_ranges_rejects_out_of_range_endpoint() {
        let (mut eval, parser) = eval_with_json_parser("{}");
        let ranges = Value::list(vec![Value::cons(Value::fixnum(1), Value::fixnum(4))]);

        let sig = expect_signal(
            builtin_treesit_parser_set_included_ranges(&mut eval, vec![parser, ranges]),
            "treesit-range-invalid",
        );
        assert_eq!(
            sig.data,
            vec![
                Value::string("RANGE is either overlapping, out-of-order or out-of-range"),
                ranges,
            ]
        );
    }

    #[test]
    fn treesit_node_position_apis_reject_markers_like_gnu() {
        let (mut eval, parser) = eval_with_json_parser("{}");
        let root = builtin_treesit_parser_root_node(&mut eval, vec![parser])
            .expect("root node for json parser");
        let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
        let marker = crate::emacs_core::marker::make_registered_buffer_marker(
            &mut eval.buffers,
            buffer_id,
            LispCharPos1::new(1),
            false,
        );

        let sig = expect_signal(
            builtin_treesit_node_first_child_for_pos(&mut eval, vec![root, marker]),
            "wrong-type-argument",
        );
        assert_eq!(sig.data, vec![Value::symbol("integerp"), marker]);

        let sig = expect_signal(
            builtin_treesit_node_descendant_for_range(
                &mut eval,
                vec![root, marker, Value::fixnum(2)],
            ),
            "wrong-type-argument",
        );
        assert_eq!(sig.data, vec![Value::symbol("integerp"), marker]);
    }

    #[test]
    fn treesit_linecol_at_rejects_markers_like_gnu() {
        let (mut eval, _parser) = eval_with_json_parser("{}");
        let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
        let marker = crate::emacs_core::marker::make_registered_buffer_marker(
            &mut eval.buffers,
            buffer_id,
            LispCharPos1::new(1),
            false,
        );

        let sig = expect_signal(
            builtin_treesit_linecol_at(&mut eval, vec![marker]),
            "wrong-type-argument",
        );
        assert_eq!(sig.data, vec![Value::symbol("numberp"), marker]);
    }
}
