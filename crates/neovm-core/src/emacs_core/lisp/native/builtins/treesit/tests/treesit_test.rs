use super::freshness_tests::eval_with_json_parser;
use super::*;
use crate::emacs_core::error::Flow;

fn json_opening_bracket_node(eval: &mut super::super::eval::Context, parser: Value) -> Value {
    let root =
        builtin_treesit_parser_root_node(eval, vec![parser]).expect("root node for json parser");
    let array = builtin_treesit_node_child(eval, vec![root, Value::fixnum(0)]).expect("array node");
    builtin_treesit_node_child(eval, vec![array, Value::fixnum(0)]).expect("opening bracket node")
}

fn expect_signal(result: EvalResult, symbol: &str) -> Box<crate::emacs_core::error::SignalData> {
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
            let source =
                query_predicate_parser_source(eval, node.parser_id).expect("parser source");
            let raw = unsafe { tree_sitter::Node::from_raw(node.raw) };
            let text = source
                .slice(raw.start_byte(), raw.end_byte())
                .expect("captured node text");
            String::from_utf8_lossy(text.as_bytes()).into_owned()
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
    let array =
        builtin_treesit_node_child(&mut eval, vec![root, Value::fixnum(0)]).expect("array node");

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
        builtin_treesit_parser_included_ranges(&mut eval, vec![parser]).expect("included ranges"),
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
        builtin_treesit_node_descendant_for_range(&mut eval, vec![root, marker, Value::fixnum(2)]),
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
