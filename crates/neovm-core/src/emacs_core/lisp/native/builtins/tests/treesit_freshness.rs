use super::*;

pub(super) fn eval_with_json_parser(text: &str) -> (super::super::eval::Context, Value) {
    let mut eval = super::super::eval::Context::new();
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert(text);
    let language_sym = Value::symbol("json").as_symbol_id().expect("json symbol");
    eval.treesit.cache_loaded_language(
        language_sym,
        runtime::LoadedLanguage {
            language: Language::new(tree_sitter_json::LANGUAGE),
            filename: None,
            _library: None,
        },
    );
    let parser = builtin_treesit_parser_create(
        &mut eval,
        vec![Value::symbol("json"), Value::NIL, Value::T, Value::NIL],
    )
    .expect("json parser");
    (eval, parser)
}

#[test]
fn unchanged_root_lookups_do_not_reextract_buffer_source() {
    crate::test_utils::init_test_tracing();
    let (mut eval, parser) = eval_with_json_parser(r##"{"key": 1}"##);
    reset_treesit_buffer_source_extraction_count();

    builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("initial parse");
    assert_eq!(treesit_buffer_source_extraction_count(), 1);

    for _ in 0..16 {
        builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("cached root");
    }
    assert_eq!(
        treesit_buffer_source_extraction_count(),
        1,
        "a clean parser should take GNU's O(1) fast path"
    );

    crate::emacs_core::textprop::builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .expect("add a text property");
    builtin_treesit_parser_root_node(&mut eval, vec![parser])
        .expect("root after a property-only change");
    assert_eq!(
        treesit_buffer_source_extraction_count(),
        1,
        "text properties are not parser input"
    );
    let parser_id = runtime::parser_id(parser).expect("parser id");
    assert!(
        eval.treesit
            .parser(parser_id)
            .expect("parser entry")
            .last_source
            .as_ref()
            .expect("parsed source")
            .intervals()
            .is_empty(),
        "the cached parser input must not retain buffer text properties"
    );
}

#[test]
fn tracked_character_edit_reparses_once_and_updates_source() {
    crate::test_utils::init_test_tracing();
    let (mut eval, parser) = eval_with_json_parser(r##"{"key": 1}"##);
    reset_treesit_buffer_source_extraction_count();
    builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("initial parse");

    crate::emacs_core::buffer::builtin_goto_char(&mut eval, vec![Value::fixnum(10)])
        .expect("move before the closing brace");
    crate::emacs_core::buffer::builtin_insert(&mut eval, vec![Value::string("0")])
        .expect("insert one digit");

    let parser_id = runtime::parser_id(parser).expect("parser id");
    assert!(matches!(
        eval.treesit
            .parser(parser_id)
            .expect("parser entry")
            .freshness,
        ParserFreshness::ReparsePending(_)
    ));

    builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("incremental reparse");
    let entry = eval.treesit.parser(parser_id).expect("parser entry");
    assert_eq!(
        entry
            .last_source
            .as_ref()
            .expect("parsed source")
            .as_bytes(),
        br##"{"key": 10}"##
    );
    assert!(matches!(entry.freshness, ParserFreshness::Clean(_)));
    assert_eq!(
        treesit_buffer_source_extraction_count(),
        2,
        "one character edit should materialize one new parser input"
    );
}

#[test]
fn inhibited_modification_hooks_do_not_hide_character_edits_from_parser() {
    crate::test_utils::init_test_tracing();
    let (mut eval, parser) = eval_with_json_parser(r##"{"key": 1}"##);
    builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("initial parse");
    eval.obarray
        .set_symbol_value("inhibit-modification-hooks", Value::T);

    crate::emacs_core::buffer::builtin_goto_char(&mut eval, vec![Value::fixnum(10)])
        .expect("move before the closing brace");
    crate::emacs_core::buffer::builtin_insert(&mut eval, vec![Value::string("0")])
        .expect("insert with Lisp modification hooks inhibited");

    let parser_id = runtime::parser_id(parser).expect("parser id");
    assert!(matches!(
        eval.treesit
            .parser(parser_id)
            .expect("parser entry")
            .freshness,
        ParserFreshness::ReparsePending(_)
    ));
    builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("incremental reparse");
    assert_eq!(
        eval.treesit
            .parser(parser_id)
            .expect("parser entry")
            .last_source
            .as_ref()
            .expect("parsed source")
            .as_bytes(),
        br##"{"key": 10}"##
    );
}

#[test]
fn untracked_character_edit_forces_safe_full_reparse() {
    crate::test_utils::init_test_tracing();
    let (mut eval, parser) = eval_with_json_parser(r##"{"key": 1}"##);
    reset_treesit_buffer_source_extraction_count();
    builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("initial parse");

    // Direct Buffer mutation deliberately bypasses the semantic edit pipeline.
    // The O(1) revision comparison must detect it without trusting a stale tree.
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("0");

    let ranges = builtin_treesit_parser_changed_regions(&mut eval, vec![parser])
        .expect("changed regions after an untracked edit");
    let ranges = crate::emacs_core::value::list_to_vec(&ranges).expect("changed range list");
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].cons_car(), Value::fixnum(1));
    assert_eq!(ranges[0].cons_cdr(), Value::fixnum(12));

    let parser_id = runtime::parser_id(parser).expect("parser id");
    let entry = eval.treesit.parser(parser_id).expect("parser entry");
    assert_eq!(
        entry
            .last_source
            .as_ref()
            .expect("parsed source")
            .as_bytes(),
        br##"{"key": 1}0"##
    );
    assert!(matches!(entry.freshness, ParserFreshness::Clean(_)));
    assert_eq!(treesit_buffer_source_extraction_count(), 2);
}

/// A new restriction is a reason to reparse and to move the tree's window
/// (`treesit_sync_visible_region`, `src/treesit.c:1626-1740`), never a reason
/// to throw the tree away.
#[test]
fn a_new_restriction_moves_the_parser_window_and_forces_a_reparse() {
    crate::test_utils::init_test_tracing();
    let (mut eval, parser) = eval_with_json_parser(r##"x{"key": 1}y"##);
    builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("initial parse");
    let parser_id = runtime::parser_id(parser).expect("parser id");
    assert_eq!(
        eval.treesit
            .parser(parser_id)
            .expect("parser entry")
            .tree
            .as_ref()
            .expect("a parsed tree")
            .visible(),
        EmacsByteRange::from_usize(0, 12)
    );

    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .narrow_buffer_to_emacs_byte_range(buffer_id, EmacsByteRange::from_usize(1, 11))
        .expect("narrow to the JSON object");

    builtin_treesit_parser_root_node(&mut eval, vec![parser])
        .expect("reparse of the narrowed input");
    let entry = eval.treesit.parser(parser_id).expect("parser entry");
    assert_eq!(
        entry
            .last_source
            .as_ref()
            .expect("parsed source")
            .as_bytes(),
        br##"{"key": 1}"##
    );
    assert_eq!(
        entry.tree.as_ref().expect("a parsed tree").visible(),
        EmacsByteRange::from_usize(1, 11),
        "the tree's window follows the buffer restriction"
    );
}

#[test]
fn character_edit_in_narrowed_buffer_uses_visible_relative_coordinates() {
    crate::test_utils::init_test_tracing();
    let (mut eval, parser) = eval_with_json_parser(
        r##"hidden
x{"key": 1}y
hidden"##,
    );
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    // Restrict parser input to the JSON object, whose first byte is not the
    // first byte of the backing buffer.
    eval.buffers
        .narrow_buffer_to_emacs_byte_range(buffer_id, EmacsByteRange::from_usize(8, 18))
        .expect("narrow to the JSON object");
    builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("initial narrowed parse");

    crate::emacs_core::buffer::builtin_goto_char(&mut eval, vec![Value::fixnum(18)])
        .expect("move before the closing brace");
    crate::emacs_core::buffer::builtin_insert(&mut eval, vec![Value::string("0")])
        .expect("insert into narrowed parser input");

    let parser_id = runtime::parser_id(parser).expect("parser id");
    assert!(matches!(
        eval.treesit
            .parser(parser_id)
            .expect("parser entry")
            .freshness,
        ParserFreshness::ReparsePending(_)
    ));
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser])
        .expect("incrementally reparsed root");
    assert_eq!(
        builtin_treesit_node_string(&mut eval, vec![root]).expect("root s-expression"),
        Value::string("(document (object (pair key: (string (string_content)) value: (number))))")
    );
    assert_eq!(
        eval.treesit
            .parser(parser_id)
            .expect("parser entry")
            .last_source
            .as_ref()
            .expect("parsed source")
            .as_bytes(),
        br##"{"key": 10}"##
    );
}

/// GNU never throws a parse tree away because the buffer was narrowed.
///
/// `comment-region-internal` wraps its edits in `comment-with-narrowing`
/// (`lisp/newcomment.el:1094-1112`, used at `:1169`), so every commenting
/// command edits a buffer narrowed to the commented region and then widens
/// again.  GNU records that edit against the parser's own
/// `visible_beg`/`visible_end` window (`src/treesit.c:1503-1560`) and
/// reconciles that window with the buffer's restriction by editing the tree
/// (`treesit_sync_visible_region`, `src/treesit.c:1626-1740`).  The tree
/// survives, so `treesit_get_affected_ranges` (`src/treesit.c:1857-1880`)
/// takes its whole-buffer branch only for a parser that has never parsed.
#[test]
fn an_edit_inside_a_temporary_narrowing_keeps_the_tree_and_its_changed_region_small() {
    crate::test_utils::init_test_tracing();
    let (mut eval, parser) = eval_with_json_parser(r##"{"a": 1, "b": 2, "c": 3}"##);
    builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("initial whole-buffer parse");

    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    // Bytes 9..15 are `"b": 2`, the way `comment-with-narrowing` restricts the
    // buffer to exactly the region a commenting command is about to edit.
    eval.buffers
        .narrow_buffer_to_emacs_byte_range(buffer_id, EmacsByteRange::from_usize(9, 15))
        .expect("narrow to the middle pair");
    crate::emacs_core::buffer::builtin_goto_char(&mut eval, vec![Value::fixnum(16)])
        .expect("move to the end of the narrowed region");
    crate::emacs_core::buffer::builtin_insert(&mut eval, vec![Value::string("0")])
        .expect("insert inside the narrowing");
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .widen();

    let parser_id = runtime::parser_id(parser).expect("parser id");
    let ranges = builtin_treesit_parser_changed_regions(&mut eval, vec![parser])
        .expect("changed regions after a narrowed edit");
    let ranges = crate::emacs_core::value::list_to_vec(&ranges).expect("changed range list");

    let entry = eval.treesit.parser(parser_id).expect("parser entry");
    assert_eq!(
        entry
            .last_source
            .as_ref()
            .expect("parsed source")
            .as_bytes(),
        br##"{"a": 1, "b": 20, "c": 3}"##
    );
    assert_eq!(
        entry.tree.as_ref().expect("a surviving tree").visible(),
        EmacsByteRange::from_usize(0, 25),
        "the tree survived the narrowing and its window followed the widening"
    );
    assert!(
        !ranges
            .iter()
            .any(|range| range.cons_car() == Value::fixnum(1)
                && range.cons_cdr() == Value::fixnum(26)),
        "the whole buffer is GNU's affected range only for a parser that never \
         parsed (src/treesit.c:1861-1878), but a narrowed edit reported it"
    );
}
