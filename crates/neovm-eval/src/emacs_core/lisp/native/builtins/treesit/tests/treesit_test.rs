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
