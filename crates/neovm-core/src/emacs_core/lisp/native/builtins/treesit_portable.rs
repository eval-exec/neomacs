//! Tree-sitter subrs for hosts without native grammar modules.

use crate::emacs_core::error::{EvalResult, LispCondition, signal};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

fn unavailable() -> crate::emacs_core::error::Flow {
    signal(
        LispCondition::Error,
        vec![Value::string(
            "Tree-sitter native grammars are unavailable on this host",
        )],
    )
}

pub(crate) fn builtin_treesit_available_p(_args: Vec<Value>) -> EvalResult {
    Ok(Value::NIL)
}

pub(crate) fn builtin_treesit_compiled_query_p(_args: Vec<Value>) -> EvalResult {
    Ok(Value::NIL)
}

pub(crate) fn builtin_treesit_library_abi_version(_args: Vec<Value>) -> EvalResult {
    Ok(Value::NIL)
}

pub(crate) fn builtin_treesit_node_p(_args: Vec<Value>) -> EvalResult {
    Ok(Value::NIL)
}

pub(crate) fn builtin_treesit_node_parser(_args: Vec<Value>) -> EvalResult {
    Ok(Value::NIL)
}

pub(crate) fn builtin_treesit_parser_p(_args: Vec<Value>) -> EvalResult {
    Ok(Value::NIL)
}

pub(crate) fn builtin_treesit_query_p(_args: Vec<Value>) -> EvalResult {
    Ok(Value::NIL)
}

pub(crate) fn unsupported_pure(_args: Vec<Value>) -> EvalResult {
    Err(unavailable())
}

pub(crate) fn unsupported_context(_ctx: &mut Context, _args: Vec<Value>) -> EvalResult {
    Err(unavailable())
}

macro_rules! pure_subrs {
    ($($name:ident),+ $(,)?) => { $(pub(crate) use unsupported_pure as $name;)+ };
}

macro_rules! context_subrs {
    ($($name:ident),+ $(,)?) => { $(pub(crate) use unsupported_context as $name;)+ };
}

pure_subrs!(
    builtin_treesit_pattern_expand,
    builtin_treesit_query_expand,
    builtin_treesit_query_language,
    builtin_treesit_query_source,
);

context_subrs!(
    builtin_treesit_induce_sparse_tree,
    builtin_treesit_language_abi_version,
    builtin_treesit_language_available_p,
    builtin_treesit_node_check,
    builtin_treesit_node_child,
    builtin_treesit_node_child_by_field_name,
    builtin_treesit_node_child_count,
    builtin_treesit_node_descendant_for_range,
    builtin_treesit_node_end,
    builtin_treesit_node_eq,
    builtin_treesit_node_field_name_for_child,
    builtin_treesit_node_first_child_for_pos,
    builtin_treesit_node_match_p,
    builtin_treesit_node_next_sibling,
    builtin_treesit_node_parent,
    builtin_treesit_node_prev_sibling,
    builtin_treesit_node_start,
    builtin_treesit_node_string,
    builtin_treesit_node_type,
    builtin_treesit_parser_add_notifier,
    builtin_treesit_parser_buffer,
    builtin_treesit_parser_create,
    builtin_treesit_parser_delete,
    builtin_treesit_parser_included_ranges,
    builtin_treesit_parser_language,
    builtin_treesit_parser_list,
    builtin_treesit_parser_notifiers,
    builtin_treesit_parser_remove_notifier,
    builtin_treesit_parser_root_node,
    builtin_treesit_parser_set_included_ranges,
    builtin_treesit_parser_tag,
    builtin_treesit_query_capture,
    builtin_treesit_query_compile,
    builtin_treesit_search_forward,
    builtin_treesit_search_subtree,
    builtin_treesit_subtree_stat,
    builtin_treesit_grammar_location,
    builtin_treesit_tracking_line_column_p,
    builtin_treesit_parser_tracking_line_column_p,
    builtin_treesit_query_eagerly_compiled_p,
    builtin_treesit_parser_embed_level,
    builtin_treesit_parser_set_embed_level,
    builtin_treesit_parse_string,
    builtin_treesit_parser_changed_regions,
    builtin_treesit_linecol_at,
    builtin_treesit_linecol_cache_set,
    builtin_treesit_linecol_cache,
);
