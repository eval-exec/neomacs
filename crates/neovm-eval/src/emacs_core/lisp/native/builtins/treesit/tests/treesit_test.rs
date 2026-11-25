use super::freshness_tests::eval_with_json_parser;
use super::*;
use neovm_value::error::FlowKind;

fn json_opening_bracket_node(eval: &mut super::super::eval::Context, parser: Value) -> Value {
    let root =
        builtin_treesit_parser_root_node(eval, vec![parser]).expect("root node for json parser");
    let array = builtin_treesit_node_child(eval, vec![root, Value::fixnum(0)]).expect("array node");
    builtin_treesit_node_child(eval, vec![array, Value::fixnum(0)]).expect("opening bracket node")
}

fn expect_signal(result: EvalResult, symbol: &str) -> Box<neovm_value::error::SignalData> {
    match result.expect_err("expected signal").into_kind() {
        FlowKind::Signal(sig) => {
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
    neovm_value::value::list_to_vec(&captures)
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
    let mut eval = super::super::eval::Context::new(crate::test_utils::unit_runtime());
    let language_sym = Value::symbol("json")
        .as_bare_symbol_id()
        .expect("json symbol");
    eval.treesit.cache_loaded_language(
        language_sym,
        runtime::LoadedLanguage::from_loaded_parts(
            Language::new(tree_sitter_json::LANGUAGE),
            None,
            None,
        ),
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
    let mut eval = super::super::eval::Context::new(crate::test_utils::unit_runtime());
    let language_sym = Value::symbol("json")
        .as_bare_symbol_id()
        .expect("json symbol");
    eval.treesit.cache_loaded_language(
        language_sym,
        runtime::LoadedLanguage::from_loaded_parts(
            Language::new(tree_sitter_json::LANGUAGE),
            None,
            None,
        ),
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
    let buffer_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    eval.edit
        .buffers
        .goto_buffer_emacs_byte_pos(buffer_id, neovm_edit::buffer::EmacsBytePos::new(1));
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
    let other_buffer = eval
        .edit
        .buffers
        .create_buffer(" *treesit-predicate-other*");
    eval.set_current_buffer_unrecorded(other_buffer)
        .expect("switch current buffer");
    let query = string_capture_query(Value::list(vec![
        Value::keyword(":pred"),
        Value::string("neomacs--treesit-parser-buffer-p"),
        Value::symbol("@item"),
    ]));

    assert_eq!(captured_texts(&mut eval, parser, query), vec![r#""first""#]);
    assert_eq!(eval.edit.buffers.current_buffer_id(), Some(other_buffer));
}

#[test]
fn treesit_query_predicate_restores_parser_buffer_after_callback_switches_away() {
    let (mut eval, parser) = eval_with_json_parser(r#"["first"]"#);
    let parser_buffer = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("parser buffer");
    eval.edit.buffers.create_buffer(" *treesit-predicate-away*");
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
    assert_eq!(eval.edit.buffers.current_buffer_id(), Some(parser_buffer));
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
        neovm_value::value::list_to_vec(&captures).expect("capture list")[0].cons_cdr();
    let saved_node = eval
        .eval_str("neomacs--treesit-saved-node")
        .expect("saved predicate node");
    assert_eq!(returned_node, saved_node);
}

fn reparse_predicate(eval: &mut super::super::eval::Context) -> Value {
    eval.eval_str(
        "(defalias 'neomacs--treesit-reparse-p \
           (lambda (node) \
             (insert \"x\") \
             (treesit-parser-root-node (treesit-node-parser node)) \
             nil))",
    )
    .expect("reparse predicate");
    Value::symbol("neomacs--treesit-reparse-p")
}

/// GNU `treesit_pred_with_guard` signals `treesit-buffer_changed` (data is
/// the predicate) when a function predicate reparses. Attested on Emacs 31.1
/// `a360712c9d` for `treesit-search-subtree`, `treesit-search-forward`, and
/// `treesit-induce-sparse-tree`.
#[test]
fn treesit_search_subtree_predicate_that_reparses_signals_buffer_changed() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    let pred = reparse_predicate(&mut eval);
    let sig = expect_signal(
        builtin_treesit_search_subtree(&mut eval, vec![root, pred]),
        "treesit-buffer_changed",
    );
    assert_eq!(sig.data, vec![pred]);
}

#[test]
fn treesit_search_forward_predicate_that_reparses_signals_buffer_changed() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    let child = builtin_treesit_node_child(&mut eval, vec![root, Value::fixnum(0)]).expect("child");
    let pred = reparse_predicate(&mut eval);
    let sig = expect_signal(
        builtin_treesit_search_forward(&mut eval, vec![child, pred]),
        "treesit-buffer_changed",
    );
    assert_eq!(sig.data, vec![pred]);
}

#[test]
fn treesit_induce_sparse_tree_predicate_that_reparses_signals_buffer_changed() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    let pred = reparse_predicate(&mut eval);
    let sig = expect_signal(
        builtin_treesit_induce_sparse_tree(&mut eval, vec![root, pred]),
        "treesit-buffer_changed",
    );
    assert_eq!(sig.data, vec![pred]);
}

fn dotted_reparse_predicate(eval: &mut super::super::eval::Context) -> Value {
    eval.eval_str(
        "(defalias 'neomacs--treesit-dotted-reparse-p \
           (lambda (node) \
             (insert \"x\") \
             (treesit-parser-root-node (treesit-node-parser node)) \
             nil))",
    )
    .expect("dotted reparse predicate");
    eval.eval_str("(cons \"document\" 'neomacs--treesit-dotted-reparse-p)")
        .expect("dotted predicate")
}

/// GNU `treesit_traverse_match_predicate` calls a dotted `(string . function)`
/// predicate with `calln`, not `treesit_pred_with_guard`. Attested on Emacs
/// 31.1 `a360712c9d`: `treesit-search-subtree` returns nil and does not signal
/// when that function inserts and reparses.
#[test]
fn treesit_search_subtree_dotted_predicate_that_reparses_does_not_signal() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    let pred = dotted_reparse_predicate(&mut eval);
    let found = builtin_treesit_search_subtree(&mut eval, vec![root, pred]).expect("no signal");
    assert!(found.is_nil());
    assert_eq!(
        eval.eval_str("(buffer-string)").expect("buffer text"),
        Value::string("{\"a\": 1}x")
    );
}

#[test]
fn treesit_search_subtree_string_predicate_returns_a_live_node() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    let found = builtin_treesit_search_subtree(&mut eval, vec![root, Value::string("number")])
        .expect("number node");
    assert_eq!(
        builtin_treesit_node_type(&mut eval, vec![found]).expect("node type"),
        Value::string("number")
    );
}

fn edit_only_nil_predicate(eval: &mut super::super::eval::Context) -> Value {
    eval.eval_str("(defalias 'neomacs--treesit-edit-only-p (lambda (_n) (insert \"x\") nil))")
        .expect("edit-only predicate");
    Value::symbol("neomacs--treesit-edit-only-p")
}

/// GNU `treesit_pred_with_guard` lets an edit through when it does not reparse
/// (`src/treesit.c:2232-2245`). Attested on Emacs 31.1 `a360712c9d`:
/// `(treesit-search-subtree root (lambda (_n) (insert "x") nil))` => nil.
#[test]
fn treesit_search_subtree_edit_only_predicate_returns_nil() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    let pred = edit_only_nil_predicate(&mut eval);
    let found = builtin_treesit_search_subtree(&mut eval, vec![root, pred]).expect("no signal");
    assert!(found.is_nil());
    assert_eq!(
        builtin_treesit_node_type(&mut eval, vec![root]).expect("root stays current"),
        Value::string("document")
    );
}

/// Attested: `(treesit-search-forward child (lambda (_n) (insert "x") nil))` => nil.
#[test]
fn treesit_search_forward_edit_only_predicate_returns_nil() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    let child = builtin_treesit_node_child(&mut eval, vec![root, Value::fixnum(0)]).expect("child");
    let pred = edit_only_nil_predicate(&mut eval);
    let found = builtin_treesit_search_forward(&mut eval, vec![child, pred]).expect("no signal");
    assert!(found.is_nil());
}

/// Attested: `(treesit-induce-sparse-tree root (lambda (_n) (insert "x") nil))` => nil.
#[test]
fn treesit_induce_sparse_tree_edit_only_predicate_returns_nil() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    let pred = edit_only_nil_predicate(&mut eval);
    let tree = builtin_treesit_induce_sparse_tree(&mut eval, vec![root, pred]).expect("no signal");
    assert!(tree.is_nil());
}

/// Attested: `(treesit-induce-sparse-tree root "pair" (lambda (n) (insert "x") (treesit-node-type n)))`
/// => `(nil ("pair"))`.
#[test]
fn treesit_induce_sparse_tree_edit_only_process_fn_returns_pair() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    eval.eval_str(
        "(defalias 'neomacs--treesit-edit-type \
           (lambda (n) (insert \"x\") (treesit-node-type n)))",
    )
    .expect("process fn");
    let tree = builtin_treesit_induce_sparse_tree(
        &mut eval,
        vec![
            root,
            Value::string("pair"),
            Value::symbol("neomacs--treesit-edit-type"),
        ],
    )
    .expect("no signal");
    assert!(tree.cons_car().is_nil());
    let pair = tree.cons_cdr().cons_car();
    assert_eq!(pair.cons_car(), Value::string("pair"));
    assert!(pair.cons_cdr().is_nil());
    assert!(tree.cons_cdr().cons_cdr().is_nil());
}

/// Attested: `(treesit-node-match-p root (lambda (_n) (insert "x") t))` => t.
#[test]
fn treesit_node_match_p_edit_only_predicate_returns_t() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    eval.eval_str("(defalias 'neomacs--treesit-edit-true-p (lambda (_n) (insert \"x\") t))")
        .expect("edit-only true predicate");
    let matched = builtin_treesit_node_match_p(
        &mut eval,
        vec![root, Value::symbol("neomacs--treesit-edit-true-p")],
    )
    .expect("no signal");
    assert_eq!(matched, Value::T);
}

/// GNU walks the live tree here and is UB after the dotted reparse. Neomacs
/// must stay sound: the second arm runs on a clone and matches `"document"`.
#[test]
fn treesit_node_match_p_dotted_reparse_inside_or_stays_sound() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    let pred = eval
        .eval_str(
            "(progn \
               (defalias 'neomacs--treesit-or-reparse-p \
                 (lambda (node) \
                   (insert \"x\") \
                   (treesit-parser-root-node (treesit-node-parser node)) \
                   nil)) \
               (list 'or (cons \"document\" 'neomacs--treesit-or-reparse-p) \"document\"))",
        )
        .expect("or predicate");
    let matched = builtin_treesit_node_match_p(&mut eval, vec![root, pred]).expect("no signal");
    assert_eq!(matched, Value::T);
}

/// Neomacs-only. GNU segfaults if a dotted predicate returns t after a reparse
/// (`ts_tree_delete` then `ts_language_symbol_name`). Neomacs path-remaps the
/// clone node onto the live tree. The returned node is the document root.
#[test]
fn treesit_search_subtree_dotted_reparse_returning_t_remaps_onto_live_tree() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    let pred = eval
        .eval_str(
            "(progn \
               (defalias 'neomacs--treesit-dotted-reparse-t \
                 (lambda (node) \
                   (insert \"x\") \
                   (treesit-parser-root-node (treesit-node-parser node)) \
                   t)) \
               (cons \"document\" 'neomacs--treesit-dotted-reparse-t))",
        )
        .expect("dotted predicate");
    let found = builtin_treesit_search_subtree(&mut eval, vec![root, pred]).expect("no signal");
    assert_eq!(
        builtin_treesit_node_type(&mut eval, vec![found]).expect("remapped node"),
        Value::string("document")
    );
}

fn order_log_entries(mut log: Value) -> Vec<(Value, Value)> {
    let mut entries = Vec::new();
    while log.is_cons() {
        let item = log.cons_car();
        entries.push((item.cons_car(), item.cons_cdr().cons_car()));
        log = log.cons_cdr();
    }
    entries
}

/// Attested on Emacs 31.1 `a360712c9d`: an edit outside a walk leaves the
/// node's positions stale and `outdated` nil. Row 7 of the e150 bounce.
/// `(2 8 nil 2 8 "pair")` after inserting three spaces at point-min.
#[test]
fn treesit_node_keeps_stale_positions_after_edit_outside_a_walk() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1, "b": 2}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    let pair =
        builtin_treesit_search_subtree(&mut eval, vec![root, Value::string("pair")]).expect("pair");
    assert_eq!(
        builtin_treesit_node_start(&mut eval, vec![pair]).expect("start"),
        Value::fixnum(2)
    );
    assert_eq!(
        builtin_treesit_node_end(&mut eval, vec![pair]).expect("end"),
        Value::fixnum(8)
    );
    eval.eval_str("(progn (goto-char (point-min)) (insert \"   \"))")
        .expect("insert before the pair");
    assert!(
        builtin_treesit_node_check(&mut eval, vec![pair, Value::symbol("outdated")])
            .expect("outdated")
            .is_nil()
    );
    assert_eq!(
        builtin_treesit_node_start(&mut eval, vec![pair]).expect("stale start"),
        Value::fixnum(2)
    );
    assert_eq!(
        builtin_treesit_node_end(&mut eval, vec![pair]).expect("stale end"),
        Value::fixnum(8)
    );
    assert_eq!(
        builtin_treesit_node_type(&mut eval, vec![pair]).expect("type"),
        Value::string("pair")
    );
}

/// Attested on Emacs 31.1 `a360712c9d`: a search result whose predicate only
/// edited is the pair at its pre-edit positions. Row 11 of the e150 bounce.
#[test]
fn treesit_search_subtree_edit_inside_walk_keeps_stale_pair_positions() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1, "b": 2}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    eval.eval_str(
        "(defalias 'neomacs--treesit-insert-pair-p \
           (lambda (n) (insert \"x\") (equal (treesit-node-type n) \"pair\")))",
    )
    .expect("edit predicate");
    let found = builtin_treesit_search_subtree(
        &mut eval,
        vec![root, Value::symbol("neomacs--treesit-insert-pair-p")],
    )
    .expect("no signal");
    assert_eq!(
        builtin_treesit_node_type(&mut eval, vec![found]).expect("type"),
        Value::string("pair")
    );
    assert_eq!(
        builtin_treesit_node_start(&mut eval, vec![found]).expect("stale start"),
        Value::fixnum(2)
    );
    assert_eq!(
        builtin_treesit_node_end(&mut eval, vec![found]).expect("stale end"),
        Value::fixnum(8)
    );
}

/// Attested on Emacs 31.1 `a360712c9d`: `treesit_build_sparse_tree` calls
/// `process_fn` before visiting children (`src/treesit.c:4941-4950`). Row 12.
#[test]
fn treesit_induce_sparse_tree_calls_process_fn_before_children() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1, "b": 2}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    eval.eval_str(
        "(progn \
           (setq neomacs--treesit-order nil) \
           (defalias 'neomacs--treesit-order-pred \
             (lambda (n) \
               (setq neomacs--treesit-order \
                     (cons (list 'pred (treesit-node-type n)) neomacs--treesit-order)) \
               (member (treesit-node-type n) '(\"object\" \"pair\")))) \
           (defalias 'neomacs--treesit-order-proc \
             (lambda (n) \
               (setq neomacs--treesit-order \
                     (cons (list 'proc (treesit-node-type n)) neomacs--treesit-order)) \
               (treesit-node-type n))))",
    )
    .expect("order callbacks");
    builtin_treesit_induce_sparse_tree(
        &mut eval,
        vec![
            root,
            Value::symbol("neomacs--treesit-order-pred"),
            Value::symbol("neomacs--treesit-order-proc"),
        ],
    )
    .expect("no signal");
    let entries = order_log_entries(
        eval.eval_str("(nreverse neomacs--treesit-order)")
            .expect("order log"),
    );
    let prefix: Vec<(Value, Value)> = entries.into_iter().take(6).collect();
    assert_eq!(
        prefix,
        vec![
            (Value::symbol("pred"), Value::string("document")),
            (Value::symbol("pred"), Value::string("object")),
            (Value::symbol("proc"), Value::string("object")),
            (Value::symbol("pred"), Value::string("{")),
            (Value::symbol("pred"), Value::string("pair")),
            (Value::symbol("proc"), Value::string("pair")),
        ]
    );
}

/// Attested on Emacs 31.1 `a360712c9d`: `treesit_build_sparse_tree` visits
/// children only when `limit > 0` (`src/treesit.c:4957`). A negative DEPTH
/// still matches ROOT and does not walk downward, so a document root and
/// `"pair"` return nil.
#[test]
fn treesit_induce_sparse_tree_negative_depth_returns_nil() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1, "b": 2}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    let tree = builtin_treesit_induce_sparse_tree(
        &mut eval,
        vec![root, Value::string("pair"), Value::NIL, Value::fixnum(-1)],
    )
    .expect("no signal");
    assert!(tree.is_nil(), "negative DEPTH must return nil, got {tree}");
}

/// Attested on Emacs 31.1 `a360712c9d`: `treesit-parser-set-included-ranges`
/// inside a search predicate leaves the matched node live and not outdated.
/// Row 13 of the e150 bounce. `("pair" nil)`.
#[test]
fn treesit_search_subtree_set_included_ranges_inside_predicate_stays_current() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1, "b": 2}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    eval.eval_str(
        "(defalias 'neomacs--treesit-ranges-p \
           (lambda (n) \
             (narrow-to-region 2 (point-max)) \
             (treesit-parser-set-included-ranges (treesit-node-parser n) nil) \
             (equal (treesit-node-type n) \"pair\")))",
    )
    .expect("ranges predicate");
    let found = builtin_treesit_search_subtree(
        &mut eval,
        vec![root, Value::symbol("neomacs--treesit-ranges-p")],
    )
    .expect("no signal");
    eval.eval_str("(widen)").expect("widen");
    assert_eq!(
        builtin_treesit_node_type(&mut eval, vec![found]).expect("type"),
        Value::string("pair")
    );
    assert!(
        builtin_treesit_node_check(&mut eval, vec![found, Value::symbol("outdated")])
            .expect("outdated")
            .is_nil()
    );
}

/// Attested on Emacs 31.1 `a360712c9d`: `("pair" nil)`.
///
/// Row 13b. The ranges slot changes, so `treesit-parser-set-included-ranges`
/// reaches `ts_tree_edit` while the walk clone is alive. Regression guard:
/// the closure is the single `edit_tree` and the plan-only sync.
#[test]
fn treesit_search_subtree_changed_included_ranges_inside_predicate_stays_current() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1, "b": 2}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    let ranges = eval
        .eval_str("(list (cons (point-min) (point-max)))")
        .expect("full-buffer ranges");
    builtin_treesit_parser_set_included_ranges(&mut eval, vec![parser, ranges])
        .expect("arm ranges");
    eval.eval_str(
        "(defalias 'neomacs--treesit-clear-ranges-p \
           (lambda (n) \
             (narrow-to-region 2 (point-max)) \
             (treesit-parser-set-included-ranges (treesit-node-parser n) nil) \
             (equal (treesit-node-type n) \"pair\")))",
    )
    .expect("ranges predicate");
    let found = builtin_treesit_search_subtree(
        &mut eval,
        vec![root, Value::symbol("neomacs--treesit-clear-ranges-p")],
    )
    .expect("no signal");
    eval.eval_str("(widen)").expect("widen");
    assert_eq!(
        builtin_treesit_node_type(&mut eval, vec![found]).expect("type"),
        Value::string("pair")
    );
    assert!(
        builtin_treesit_node_check(&mut eval, vec![found, Value::symbol("outdated")])
            .expect("outdated")
            .is_nil()
    );
}

/// Attested on Emacs 31.1 `a360712c9d`: `("pair" 10 16 nil)`.
///
/// Row 15. The predicate saves each pair, then inserts two spaces at
/// point-min, and the search returns nil. The saved node is the second
/// pair, published before that in-walk edit. Reading it after the walk
/// drops the clone. The unit runtime has no `when`, so the body is `if`.
#[test]
fn treesit_search_subtree_saved_pair_keeps_stale_positions_after_point_min_insert() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1, "b": 2}"#);
    let root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root");
    eval.eval_str(
        "(progn \
           (setq neomacs--treesit-saved nil) \
           (defalias 'neomacs--treesit-save-pair-p \
             (lambda (n) \
               (if (equal (treesit-node-type n) \"pair\") \
                   (progn \
                     (setq neomacs--treesit-saved n) \
                     (goto-char (point-min)) \
                     (insert \"  \"))) \
               nil)))",
    )
    .expect("save predicate");
    let found = builtin_treesit_search_subtree(
        &mut eval,
        vec![root, Value::symbol("neomacs--treesit-save-pair-p")],
    )
    .expect("no signal");
    assert!(found.is_nil());
    let saved = eval.eval_str("neomacs--treesit-saved").expect("saved pair");
    assert_eq!(
        builtin_treesit_node_type(&mut eval, vec![saved]).expect("type"),
        Value::string("pair")
    );
    assert_eq!(
        builtin_treesit_node_start(&mut eval, vec![saved]).expect("stale start"),
        Value::fixnum(10)
    );
    assert_eq!(
        builtin_treesit_node_end(&mut eval, vec![saved]).expect("stale end"),
        Value::fixnum(16)
    );
    assert!(
        builtin_treesit_node_check(&mut eval, vec![saved, Value::symbol("outdated")])
            .expect("outdated")
            .is_nil()
    );
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
        TreesitBuiltinPredicate::from_symbol_value(
            Value::symbol("named"),
            neovm_value::tagged::SymbolMode::Bare
        ),
        Some(TreesitBuiltinPredicate::Named)
    );
    assert_eq!(
        TreesitBuiltinPredicate::from_symbol_value(
            Value::symbol("anonymous"),
            neovm_value::tagged::SymbolMode::Bare
        ),
        Some(TreesitBuiltinPredicate::Anonymous)
    );
    assert_eq!(
        TreesitBuiltinPredicate::from_symbol_value(
            Value::symbol("missing"),
            neovm_value::tagged::SymbolMode::Bare
        ),
        None
    );
    assert_eq!(TreesitBuiltinPredicate::Anonymous.name(), "anonymous");

    assert_eq!(
        TreesitBooleanPredicate::from_symbol_value(
            Value::symbol("not"),
            neovm_value::tagged::SymbolMode::Bare
        ),
        Some(TreesitBooleanPredicate::Not)
    );
    assert_eq!(
        TreesitBooleanPredicate::from_symbol_value(
            Value::symbol("or"),
            neovm_value::tagged::SymbolMode::Bare
        ),
        Some(TreesitBooleanPredicate::Or)
    );
    assert_eq!(
        TreesitBooleanPredicate::from_symbol_value(
            Value::symbol("and"),
            neovm_value::tagged::SymbolMode::Bare
        ),
        Some(TreesitBooleanPredicate::And)
    );
    assert_eq!(
        TreesitBooleanPredicate::from_symbol_value(
            Value::symbol("named"),
            neovm_value::tagged::SymbolMode::Bare
        ),
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

/// GNU `Ftreesit_parser_set_included_ranges` returns before touching the
/// parser when `Fequal` says the new list matches the saved one
/// (`src/treesit.c:2737-2738`). A freshly built equal list is that case.
#[test]
fn treesit_parser_set_included_ranges_equal_list_does_not_reparse() {
    let (mut eval, parser) = eval_with_json_parser("{}");
    reset_treesit_reparse_count();
    builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("parse");
    let ranges = Value::list(vec![Value::cons(Value::fixnum(1), Value::fixnum(3))]);
    builtin_treesit_parser_set_included_ranges(&mut eval, vec![parser, ranges]).expect("set");
    builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("reparse after ranges");
    let after_change = treesit_reparse_count();
    let fresh = Value::list(vec![Value::cons(Value::fixnum(1), Value::fixnum(3))]);
    assert!(
        !neovm_value::value::eq_value(&ranges, &fresh),
        "the second list is a different object"
    );
    builtin_treesit_parser_set_included_ranges(&mut eval, vec![parser, fresh]).expect("equal set");
    builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root after equal set");
    assert_eq!(
        treesit_reparse_count(),
        after_change,
        "an equal range list must not mark the parser for reparse"
    );
}

fn long_included_range_list(count: i64) -> Value {
    Value::list(
        (1..=count)
            .map(|start| Value::cons(Value::fixnum(start), Value::fixnum(start + 1)))
            .collect(),
    )
}

/// GNU `Fequal` walks a list spine in a loop, so length does not count as
/// nesting. `Value ==` returns false once that spine passes depth 200.
/// Attested oracle form `(equal <300 fresh conses> <300 fresh conses>)` is
/// `t` on Emacs 31.1. A fresh list of 250 ranges is the same shape.
#[test]
fn treesit_parser_set_included_ranges_long_equal_list_does_not_reparse() {
    let text = format!("\"{}\"", "x".repeat(260));
    let (mut eval, parser) = eval_with_json_parser(&text);
    reset_treesit_reparse_count();
    builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("parse");
    let ranges = long_included_range_list(250);
    builtin_treesit_parser_set_included_ranges(&mut eval, vec![parser, ranges]).expect("set");
    builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("reparse after ranges");
    let after_change = treesit_reparse_count();
    let fresh = long_included_range_list(250);
    assert!(
        !neovm_value::value::eq_value(&ranges, &fresh),
        "the second list is a different object"
    );
    builtin_treesit_parser_set_included_ranges(&mut eval, vec![parser, fresh]).expect("equal set");
    builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root after equal set");
    assert_eq!(
        treesit_reparse_count(),
        after_change,
        "250 equal ranges must not mark the parser for reparse"
    );
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

/// GNU `CHECK_CONS` (`src/treesit.c:2734-2735`) runs before `Fequal` when
/// ranges is non-nil. A symbol signals `(wrong-type-argument consp X)`.
#[test]
fn treesit_parser_set_included_ranges_requires_proper_list() {
    let (mut eval, parser) = eval_with_json_parser("{}");
    let bad_ranges = Value::symbol("not-a-list");

    let sig = expect_signal(
        builtin_treesit_parser_set_included_ranges(&mut eval, vec![parser, bad_ranges]),
        "wrong-type-argument",
    );
    assert_eq!(sig.data, vec![Value::symbol("consp"), bad_ranges]);
}

/// A dotted list is a cons, so `CHECK_CONS` passes. `CHECK_LIST_END`
/// (`lisp.h`) then signals `listp` on the whole ranges object.
#[test]
fn treesit_parser_set_included_ranges_rejects_dotted_list() {
    let (mut eval, parser) = eval_with_json_parser("{}");
    let ranges = Value::cons(
        Value::cons(Value::fixnum(1), Value::fixnum(2)),
        Value::symbol("dot"),
    );

    let sig = expect_signal(
        builtin_treesit_parser_set_included_ranges(&mut eval, vec![parser, ranges]),
        "wrong-type-argument",
    );
    assert_eq!(sig.data, vec![Value::symbol("listp"), ranges]);
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
    let buffer_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    let marker = neovm_edit::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.edit.buffers,
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
    let buffer_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    let marker = neovm_edit::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.edit.buffers,
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
    let buffer_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    let marker = neovm_edit::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.edit.buffers,
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

#[test]
fn treesit_insert_node_rejects_foreign_and_clone_tree_nodes() {
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": 1}"#);
    // Force an initial parse so the parser holds a live tree.
    let _root = builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root node");
    let parser_id = runtime::parser_id(parser).expect("parser id");
    let epoch = eval
        .treesit
        .parser(parser_id)
        .expect("live parser")
        .reparse_epoch();

    // Build an unrelated temporary tree using the same grammar.
    let mut other_parser = tree_sitter::Parser::new();
    other_parser
        .set_language(&Language::new(tree_sitter_json::LANGUAGE))
        .expect("set json language");
    let foreign_tree = other_parser.parse("[1, 2]", None).expect("foreign tree");

    let foreign_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        eval.treesit
            .insert_node(parser_id, foreign_tree.root_node(), epoch)
    }));
    assert!(
        foreign_result.is_err(),
        "insert_node must reject a node belonging to a foreign tree"
    );

    let clone = eval
        .treesit
        .parser(parser_id)
        .expect("live parser")
        .walk_clone()
        .expect("walk clone");
    let clone_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        eval.treesit
            .insert_node(parser_id, clone.root_node(), epoch)
    }));
    assert!(
        clone_result.is_err(),
        "insert_node must reject a node belonging to a WalkClone rather than the parser's live tree"
    );

    let live_root_raw = eval
        .treesit
        .parser(parser_id)
        .expect("live parser")
        .root_raw()
        .expect("live root");
    // SAFETY: `live_root_raw` belongs to `parser_id`'s current live tree and
    // is not mutated before `insert_node` returns.
    let live_root = unsafe { tree_sitter::Node::from_raw(live_root_raw) };
    let inserted_id = eval.treesit.insert_node(parser_id, live_root, epoch);
    assert!(
        eval.treesit.node(inserted_id).is_some(),
        "insert_node must accept a node belonging to the parser's own live tree"
    );
}

#[test]
fn treesit_callbacks_root_values_across_gc_in_sparse_tree_predicates_and_notifiers() {
    // 1. `build_sparse_tree`: `payload` and accumulated `children` must stay
    //    rooted across recursive predicate / process_fn callbacks that run GC.
    let (mut eval, parser) = eval_with_json_parser(r#"{"a": [1, 2, 3], "b": [4, 5, 6]}"#);
    let root =
        builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root node for json");
    let pred = eval
        .eval_str(
            r#"(lambda (n)
                 (garbage-collect)
                 (let ((i 0))
                   (while (< i 200)
                     (cons i (concat "junk-" (number-to-string i)))
                     (setq i (1+ i))))
                 (or (equal (treesit-node-type n) "array")
                     (equal (treesit-node-type n) "number")))"#,
        )
        .expect("sparse tree predicate");
    let process_fn = eval
        .eval_str(
            r#"(lambda (n)
                 (garbage-collect)
                 (let ((i 0))
                   (while (< i 200)
                     (cons i (concat "proc-junk-" (number-to-string i)))
                     (setq i (1+ i))))
                 (cons (concat "tag-" (treesit-node-type n))
                       (treesit-node-start n)))"#,
        )
        .expect("sparse tree process_fn");

    let tree = builtin_treesit_induce_sparse_tree(&mut eval, vec![root, pred, process_fn])
        .expect("induce sparse tree across GC");
    let expected = eval
        .eval_str(
            r#"(quote (nil
                       (("tag-array" . 7)
                        (("tag-number" . 8))
                        (("tag-number" . 11))
                        (("tag-number" . 14)))
                       (("tag-array" . 23)
                        (("tag-number" . 24))
                        (("tag-number" . 27))
                        (("tag-number" . 30)))))"#,
        )
        .expect("expected sparse tree");
    assert!(
        neovm_value::value::equal_value(&tree, &expected, 0),
        "sparse tree corrupted across GC: got {tree:?}"
    );
    drop(eval);

    // 2. `predicate_matches_node`: composite `or`/`and` predicate looked up
    //    from `treesit-thing-settings` must stay rooted even if the first
    //    branch drops `treesit-thing-settings` and runs GC.
    let (mut eval, parser) = eval_with_json_parser(r#"[1, 2]"#);
    let root =
        builtin_treesit_parser_root_node(&mut eval, vec![parser]).expect("root node for json");
    eval.eval_str(
        r#"(set (make-local-variable 'treesit-thing-settings)
             (list
              (list 'json
                    (list 'gc-thing
                          (list 'or
                                (lambda (n)
                                  (if (equal (treesit-node-type n) "number")
                                      (progn
                                        (setq treesit-thing-settings nil)
                                        (garbage-collect)
                                        (let ((i 0))
                                          (while (< i 200)
                                            (cons i (concat "or-junk-" (number-to-string i)))
                                            (setq i (1+ i))))))
                                  nil)
                                (eval '(let ((target (concat "num" "ber")))
                                         (lambda (n)
                                           (equal (treesit-node-type n) target)))
                                      t))))))"#,
    )
    .expect("install thing settings");
    let found = builtin_treesit_search_subtree(&mut eval, vec![root, Value::symbol("gc-thing")])
        .expect("search subtree with gc-thing");
    assert!(
        !found.is_nil(),
        "expected second `or` branch to match number"
    );
    let found_type = builtin_treesit_node_type(&mut eval, vec![found]).expect("found node type");
    assert_eq!(
        found_type.as_utf8_str(),
        Some("number"),
        "expected number node from second `or` branch"
    );
    drop(eval);

    // 3. `builtin_treesit_parser_changed_regions`: `regions` and copied
    //    `notifiers` must stay rooted across notifier callbacks that mutate
    //    the parser's notifier list and run GC.
    let (mut eval, parser) = eval_with_json_parser(r#"[1]"#);
    eval.eval_str("(setq neomacs--gc-notifier-log nil)")
        .expect("init notifier log");
    eval.eval_str(
        r#"(defalias 'neomacs--gc-notifier-2
             (eval '(let ((marker (concat "seen-" "n2")))
                      (lambda (ranges _parser)
                        (setq neomacs--gc-notifier-log
                              (list marker (car (car ranges)) (cdr (car ranges))))))
                   t))"#,
    )
    .expect("notifier2");
    eval.eval_str(
        r#"(defalias 'neomacs--gc-notifier-1
             (lambda (_ranges parser)
               (treesit-parser-remove-notifier parser 'neomacs--gc-notifier-2)
               (garbage-collect)
               (let ((i 0))
                 (while (< i 200)
                   (cons i (concat "notif-junk-" (number-to-string i)))
                   (setq i (1+ i))))))"#,
    )
    .expect("notifier1");
    builtin_treesit_parser_add_notifier(
        &mut eval,
        vec![parser, Value::symbol("neomacs--gc-notifier-2")],
    )
    .expect("add notifier2");
    builtin_treesit_parser_add_notifier(
        &mut eval,
        vec![parser, Value::symbol("neomacs--gc-notifier-1")],
    )
    .expect("add notifier1");
    // Edit buffer so changed_regions is non-empty.
    eval.eval_str("(goto-char 3) (insert \", 2\")")
        .expect("edit json buffer");
    let regions =
        builtin_treesit_parser_changed_regions(&mut eval, vec![parser]).expect("changed regions");
    assert!(
        regions.is_cons() && regions.cons_car().is_cons(),
        "returned regions corrupted across GC: {regions:?}"
    );
    let log = eval
        .eval_str("neomacs--gc-notifier-log")
        .expect("read notifier log");
    let log_items = neovm_value::value::list_to_vec(&log).expect("notifier log list");
    assert_eq!(log_items.len(), 3);
    assert_eq!(log_items[0].as_utf8_str(), Some("seen-n2"));
    assert!(log_items[1].is_fixnum() && log_items[2].is_fixnum());
    drop(eval);

    // 4. Pure-Lisp `treesit-induce-sparse-tree` and `treesit-search-forward`
    //    with `and`/`not` composite predicates and forced GC inside callbacks.
    let (mut eval, parser) = eval_with_json_parser(r#"{"x": [10, 20]}"#);
    eval.set_variable("neomacs--gc-parser", parser);
    let lisp_sparse = eval
        .eval_str(
            r#"(treesit-induce-sparse-tree
                 (treesit-parser-root-node neomacs--gc-parser)
                 (lambda (n)
                   (garbage-collect)
                   (or (equal (treesit-node-type n) "array")
                       (equal (treesit-node-type n) "number")))
                 (lambda (n)
                   (garbage-collect)
                   (cons (concat "p-" (treesit-node-type n))
                         (treesit-node-start n))))"#,
        )
        .expect("pure-Lisp sparse tree across GC");
    let expected_lisp_sparse = eval
        .eval_str(r#"(quote (nil (("p-array" . 7) (("p-number" . 8)) (("p-number" . 12)))))"#)
        .expect("expected pure-Lisp sparse tree");
    assert!(
        neovm_value::value::equal_value(&lisp_sparse, &expected_lisp_sparse, 0),
        "pure-Lisp sparse tree corrupted across GC: got {lisp_sparse:?}"
    );

    let lisp_fwd = eval
        .eval_str(
            r#"(let* ((root (treesit-parser-root-node neomacs--gc-parser))
                      (first-num (treesit-search-subtree root "number")))
                 (set (make-local-variable 'treesit-thing-settings)
                   (list
                    (list 'json
                          (list 'and-thing
                                (list 'and
                                      (list 'not
                                            (lambda (n)
                                              (if (equal (treesit-node-type n) "number")
                                                  (progn
                                                    (setq treesit-thing-settings nil)
                                                    (garbage-collect)))
                                              nil))
                                      (eval '(let ((want (concat "num" "ber")))
                                               (lambda (n)
                                                 (equal (treesit-node-type n) want)))
                                            t))))))
                 (let ((next (treesit-search-forward first-num 'and-thing)))
                   (and next (list (treesit-node-type next) (treesit-node-start next)))))"#,
        )
        .expect("pure-Lisp search-forward with and/not across GC");
    let expected_fwd = eval
        .eval_str(r#"(quote ("number" 12))"#)
        .expect("expected search-forward result");
    assert!(
        neovm_value::value::equal_value(&lisp_fwd, &expected_fwd, 0),
        "search-forward and/not predicate corrupted across GC: got {lisp_fwd:?}"
    );
}

#[test]
fn treesit_language_remap_alist_is_buffer_local_and_validates_targets_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new(crate::test_utils::unit_runtime());
    let json_sym = Value::symbol("json")
        .as_bare_symbol_id()
        .expect("json symbol");
    eval.treesit.cache_loaded_language(
        json_sym,
        runtime::LoadedLanguage::from_loaded_parts(
            Language::new(tree_sitter_json::LANGUAGE),
            None,
            None,
        ),
    );

    // GNU `src/treesit.c:5442`: `treesit-language-remap-alist` is buffer-local.
    assert_eq!(
        eval.eval_str("(local-variable-if-set-p 'treesit-language-remap-alist)")
            .expect("local-variable-if-set-p"),
        Value::T
    );

    // GNU `src/treesit.c:913`: `treesit-language-available-p` does not consult
    // `treesit-language-remap-alist`.
    let remap_dotted = Value::list(vec![Value::cons(
        Value::symbol("alias-json"),
        Value::symbol("json"),
    )]);
    eval.obarray
        .set_symbol_value("treesit-language-remap-alist", remap_dotted);
    assert_eq!(
        builtin_treesit_language_available_p(&mut eval, vec![Value::symbol("alias-json")])
            .expect("available-p"),
        Value::NIL
    );

    // GNU `src/treesit.c:2387-2388` & `:2182-2189`: non-symbol remap targets.
    let remap_bad = Value::list(vec![Value::cons(
        Value::symbol("bad-lang"),
        Value::fixnum(123),
    )]);
    eval.obarray
        .set_symbol_value("treesit-language-remap-alist", remap_bad);
    let parser_err = expect_signal(
        builtin_treesit_parser_create(
            &mut eval,
            vec![Value::symbol("bad-lang"), Value::NIL, Value::T, Value::NIL],
        ),
        "wrong-type-argument",
    );
    assert_eq!(
        parser_err.data,
        vec![Value::symbol("symbolp"), Value::fixnum(123)]
    );
    let query_err = expect_signal(
        builtin_treesit_query_compile(
            &mut eval,
            vec![
                Value::symbol("bad-lang"),
                Value::string("(string) @s"),
                Value::T,
            ],
        ),
        "treesit-query-error",
    );
    assert_eq!(
        query_err.data,
        vec![Value::string("Invalid language symbol"), Value::fixnum(123)]
    );
}

#[test]
fn treesit_query_capture_uncompiled_uses_parser_loaded_language_after_remap_cleared() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new(crate::test_utils::unit_runtime());
    eval.edit
        .buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert(r#"{"k": 1}"#);
    let json_sym = Value::symbol("json")
        .as_bare_symbol_id()
        .expect("json symbol");
    eval.treesit.cache_loaded_language(
        json_sym,
        runtime::LoadedLanguage::from_loaded_parts(
            Language::new(tree_sitter_json::LANGUAGE),
            None,
            None,
        ),
    );

    // GNU `src/treesit.c:656-662, 2387-2428`: dotted-pair remap creates a
    // parser whose Lisp-visible language stays unremapped (`alias-json`) while
    // loading the remapped `TSLanguage` (`json`).
    let remap_dotted = Value::list(vec![Value::cons(
        Value::symbol("alias-json"),
        Value::symbol("json"),
    )]);
    eval.obarray
        .set_symbol_value("treesit-language-remap-alist", remap_dotted);
    let parser = builtin_treesit_parser_create(
        &mut eval,
        vec![
            Value::symbol("alias-json"),
            Value::NIL,
            Value::T,
            Value::NIL,
        ],
    )
    .expect("remapped parser creation");
    assert_eq!(
        builtin_treesit_parser_language(&mut eval, vec![parser]).expect("parser language"),
        Value::symbol("alias-json")
    );

    // GNU `src/treesit.c:4063-4064, 3965-3988`: uncompiled query capture must
    // compile against the parser's loaded `TSLanguage`, even after
    // `treesit-language-remap-alist` is cleared to `nil`.
    eval.obarray
        .set_symbol_value("treesit-language-remap-alist", Value::NIL);
    assert_eq!(
        captured_texts(&mut eval, parser, Value::string("(string) @s")),
        vec![r#""k""#.to_string()]
    );
}

#[test]
fn treesit_query_capture_compiled_matches_loaded_tslanguage_across_remap() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new(crate::test_utils::unit_runtime());
    eval.edit
        .buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert(r#"{"k": 1}"#);
    let json_sym = Value::symbol("json")
        .as_bare_symbol_id()
        .expect("json symbol");
    eval.treesit.cache_loaded_language(
        json_sym,
        runtime::LoadedLanguage::from_loaded_parts(
            Language::new(tree_sitter_json::LANGUAGE),
            None,
            None,
        ),
    );

    let remap_dotted = Value::list(vec![Value::cons(
        Value::symbol("alias-json"),
        Value::symbol("json"),
    )]);
    eval.obarray
        .set_symbol_value("treesit-language-remap-alist", remap_dotted);
    let parser = builtin_treesit_parser_create(
        &mut eval,
        vec![
            Value::symbol("alias-json"),
            Value::NIL,
            Value::T,
            Value::NIL,
        ],
    )
    .expect("remapped parser creation");

    // Compiled query compiled for `'json` shares the loaded `TSLanguage` with
    // the `'alias-json` parser and must succeed instead of failing on
    // unremapped Lisp symbol inequality.
    eval.obarray
        .set_symbol_value("treesit-language-remap-alist", Value::NIL);
    let compiled_json = builtin_treesit_query_compile(
        &mut eval,
        vec![
            Value::symbol("json"),
            Value::string("(string) @s"),
            Value::T,
        ],
    )
    .expect("compile json query");
    assert_eq!(
        captured_texts(&mut eval, parser, compiled_json),
        vec![r#""k""#.to_string()]
    );
}

#[test]
fn treesit_language_remap_alist_circular_signals_circular_list_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new(crate::test_utils::unit_runtime());
    let parser_res = eval
        .eval_str(
            r#"(let ((c (list (cons 'a 'b))))
                 (setcdr c c)
                 (let ((treesit-language-remap-alist c))
                   (condition-case e (treesit-parser-create 'z) (error (car e)))))"#,
        )
        .expect("circular remap parser-create");
    assert_eq!(parser_res, Value::symbol("circular-list"));

    let query_res = eval
        .eval_str(
            r#"(let ((c (list (cons 'a 'b))))
                 (setcdr c c)
                 (let ((treesit-language-remap-alist c))
                   (condition-case e (treesit-query-compile 'z "(string) @s" t) (error (car e)))))"#,
        )
        .expect("circular remap query-compile");
    assert_eq!(query_res, Value::symbol("circular-list"));
}

#[test]
fn treesit_language_remap_alist_improper_signals_whole_alist_datum_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new(crate::test_utils::unit_runtime());
    let parser_res = eval
        .eval_str(
            r#"(let ((treesit-language-remap-alist '((a . b) . 42)))
                 (condition-case e (treesit-parser-create 'c) (error (prin1-to-string e))))"#,
        )
        .expect("improper remap parser-create");
    assert_eq!(
        parser_res,
        Value::string("(wrong-type-argument listp ((a . b) . 42))")
    );

    let query_res = eval
        .eval_str(
            r#"(let ((treesit-language-remap-alist '((a . b) . 42)))
                 (condition-case e (treesit-query-compile 'c "(string) @s" t) (error (prin1-to-string e))))"#,
        )
        .expect("improper remap query-compile");
    assert_eq!(
        query_res,
        Value::string("(wrong-type-argument listp ((a . b) . 42))")
    );
}
