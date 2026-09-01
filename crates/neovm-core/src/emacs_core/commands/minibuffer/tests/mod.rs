use super::*;
use crate::buffer::BufferId;
use crate::emacs_core::intern::intern;
use crate::heap_types::LispString;

fn ls(text: &str) -> LispString {
    LispString::from_utf8(text)
}

// -- completion--flex-cost-gotoh (GNU 31.0.90 parity) ---------------------

/// Flatten `(COST . MATCHES)` (or nil) into a `Vec<i64>` for comparison.
fn gotoh_flatten(v: Value) -> Option<Vec<i64>> {
    if v.is_nil() {
        return None;
    }
    let mut out = vec![v.cons_car().as_fixnum().expect("cost fixnum")];
    let mut rest = v.cons_cdr();
    while rest.is_cons() {
        out.push(rest.cons_car().as_fixnum().expect("match fixnum"));
        rest = rest.cons_cdr();
    }
    Some(out)
}

fn gotoh(eval: &mut crate::emacs_core::eval::Context, pat: &str, str: &str) -> Option<Vec<i64>> {
    let r = builtin_flex_cost_gotoh(eval, vec![Value::string(pat), Value::string(str)])
        .expect("flex-cost-gotoh");
    gotoh_flatten(r)
}

#[test]
fn flex_cost_gotoh_matches_gnu_31() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // Reference values captured from GNU Emacs 31.0.90 batch.
    assert_eq!(gotoh(&mut eval, "foo", "foobar"), Some(vec![0, 0, 1, 2]));
    assert_eq!(gotoh(&mut eval, "foo", "barfoo"), Some(vec![5, 3, 4, 5]));
    assert_eq!(gotoh(&mut eval, "abc", "axbxc"), Some(vec![20, 0, 2, 4]));
    assert_eq!(
        gotoh(&mut eval, "find-f", "find-file"),
        Some(vec![0, 0, 1, 2, 3, 4, 5])
    );
    assert_eq!(gotoh(&mut eval, "aa", "aXaXa"), Some(vec![10, 0, 2]));
    // No match / degenerate cases.
    assert_eq!(gotoh(&mut eval, "xyz", "find-file"), None);
    assert_eq!(gotoh(&mut eval, "", "abc"), None);
    assert_eq!(gotoh(&mut eval, "abc", ""), None);
}

#[test]
fn flex_cost_gotoh_honors_completion_ignore_case() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // Case-sensitive: "FOO" is not a subsequence of "foobar" -> nil.
    assert_eq!(gotoh(&mut eval, "FOO", "foobar"), None);
    // With completion-ignore-case, it matches like the lowercase form.
    eval.assign("completion-ignore-case", Value::T);
    assert_eq!(gotoh(&mut eval, "FOO", "foobar"), Some(vec![0, 0, 1, 2]));
}

// -- Completion matching --------------------------------------------------

#[test]
fn completion_prefix_models_empty_input_as_match_all() {
    let empty = LispString::from_utf8("");
    let nonempty = LispString::from_utf8("M-x");

    assert_eq!(
        CompletionPrefix::from_lisp_string(&empty),
        CompletionPrefix::Empty
    );
    assert_eq!(
        CompletionPrefix::from_lisp_string(&nonempty),
        CompletionPrefix::Characters(vec!['M' as u32, '-' as u32, 'x' as u32])
    );
}

#[test]
fn normalize_symbol_reader_default_uses_list_head_and_symbol_name() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        normalize_symbol_reader_default(Value::list(vec![
            Value::symbol("forward-char"),
            Value::symbol("backward-char"),
        ])),
        Value::string("forward-char")
    );
    assert_eq!(
        normalize_symbol_reader_default(Value::symbol("fill-column")),
        Value::string("fill-column")
    );
}

#[test]
fn normalize_buffer_reader_default_uses_list_head_and_live_buffer_name() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buf_id = eval.buffers.create_buffer(" minibuffer-default ");

    assert_eq!(
        normalize_buffer_reader_default(
            &eval.buffers,
            Value::list(vec![Value::make_buffer(buf_id), Value::string("fallback")]),
        ),
        Value::string(" minibuffer-default ")
    );
    assert_eq!(
        normalize_buffer_reader_default(&eval.buffers, Value::make_buffer(buf_id)),
        Value::string(" minibuffer-default ")
    );
}

#[test]
fn read_buffer_completing_args_use_live_buffer_names_and_normalized_default() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buf_id = eval.buffers.create_buffer(" target-buffer ");
    let args = read_buffer_completing_args(
        &eval.obarray,
        &eval.buffers,
        &[
            Value::string("Buffer: "),
            Value::list(vec![Value::make_buffer(buf_id), Value::string("fallback")]),
            Value::T,
            Value::symbol("predicate"),
        ],
    );
    assert_eq!(args[0], Value::string("Buffer (default  target-buffer ): "));
    assert_eq!(args[2], Value::symbol("predicate"));
    assert_eq!(args[3], Value::T);
    assert_eq!(args[6], Value::string(" target-buffer "));
    let names = super::value_to_string_list(&args[1]);
    assert!(names.contains(&" target-buffer ".to_string()));
}

#[test]
fn read_buffer_completing_args_formats_default_prompt_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.obarray
        .set_symbol_value("minibuffer-default-prompt-format", Value::string(" [%s]"));

    let with_default = read_buffer_completing_args(
        &eval.obarray,
        &eval.buffers,
        &[
            Value::string("Switch to buffer: "),
            Value::string("*Messages*"),
        ],
    );
    assert_eq!(
        with_default[0],
        Value::string("Switch to buffer [*Messages*]: ")
    );

    let without_default = read_buffer_completing_args(
        &eval.obarray,
        &eval.buffers,
        &[Value::string("Switch to buffer: "), Value::NIL],
    );
    assert_eq!(without_default[0], Value::string("Switch to buffer: "));

    let empty_default = read_buffer_completing_args(
        &eval.obarray,
        &eval.buffers,
        &[Value::string("Switch to buffer: "), Value::string("")],
    );
    assert_eq!(empty_default[0], Value::string("Switch to buffer: "));
}

#[test]
fn finish_read_command_with_minibuffer_normalizes_default_and_interns_result() {
    crate::test_utils::init_test_tracing();
    let result = finish_read_command_with_minibuffer(
        &[
            Value::string("Command: "),
            Value::list(vec![
                Value::symbol("forward-char"),
                Value::symbol("backward-char"),
            ]),
        ],
        |minibuffer_args| {
            assert_eq!(
                minibuffer_args,
                &[
                    Value::string("Command: "),
                    Value::NIL,
                    Value::NIL,
                    Value::NIL,
                    Value::NIL,
                    Value::string("forward-char"),
                ]
            );
            Ok(Value::string("next-line"))
        },
    )
    .unwrap();
    assert_eq!(result, Value::symbol("next-line"));
}

#[test]
fn finish_read_variable_with_minibuffer_normalizes_default_and_interns_result() {
    crate::test_utils::init_test_tracing();
    let result = finish_read_variable_with_minibuffer(
        &[Value::string("Variable: "), Value::symbol("fill-column")],
        |minibuffer_args| {
            assert_eq!(
                minibuffer_args,
                &[
                    Value::string("Variable: "),
                    Value::NIL,
                    Value::NIL,
                    Value::NIL,
                    Value::NIL,
                    Value::string("fill-column"),
                ]
            );
            Ok(Value::string("tab-width"))
        },
    )
    .unwrap();
    assert_eq!(result, Value::symbol("tab-width"));
}

#[test]
fn prefix_match_basic() {
    crate::test_utils::init_test_tracing();
    let candidates = vec![ls("apple"), ls("application"), ls("banana"), ls("apply")];
    let result = prefix_match(&ls("app"), &candidates);
    assert_eq!(result.len(), 3);
    assert!(result.contains(&ls("apple")));
    assert!(result.contains(&ls("application")));
    assert!(result.contains(&ls("apply")));
}

#[test]
fn prefix_match_case_insensitive() {
    crate::test_utils::init_test_tracing();
    let candidates = vec![ls("Apple"), ls("APPLY"), ls("banana")];
    let result = prefix_match(&ls("app"), &candidates);
    assert_eq!(result.len(), 2);
}

#[test]
fn prefix_match_empty_input() {
    crate::test_utils::init_test_tracing();
    let candidates = vec![ls("a"), ls("b"), ls("c")];
    let result = prefix_match(&ls(""), &candidates);
    assert_eq!(result.len(), 3);
}

#[test]
fn prefix_match_no_matches() {
    crate::test_utils::init_test_tracing();
    let candidates = vec![ls("apple"), ls("banana")];
    let result = prefix_match(&ls("zz"), &candidates);
    assert!(result.is_empty());
}

#[test]
fn substring_match_basic() {
    crate::test_utils::init_test_tracing();
    let candidates = vec![ls("foobar"), ls("bazfoo"), ls("hello"), ls("food")];
    let result = substring_match(&ls("foo"), &candidates);
    assert_eq!(result.len(), 3);
    assert!(result.contains(&ls("foobar")));
    assert!(result.contains(&ls("bazfoo")));
    assert!(result.contains(&ls("food")));
}

#[test]
fn flex_match_basic() {
    crate::test_utils::init_test_tracing();
    let candidates = vec![
        ls("find-file"),
        ls("flycheck"),
        ls("first-foo"),
        ls("hello"),
    ];
    // "ff" should match strings where 'f' appears twice in order.
    let result = flex_match(&ls("ff"), &candidates);
    assert!(result.contains(&ls("find-file")));
    assert!(result.contains(&ls("first-foo")));
    assert!(!result.contains(&ls("hello")));
}

#[test]
fn flex_match_all_chars_in_order() {
    crate::test_utils::init_test_tracing();
    let candidates = vec![ls("abcdef"), ls("axbycz"), ls("zzz")];
    let result = flex_match(&ls("abc"), &candidates);
    assert_eq!(result.len(), 2);
    assert!(result.contains(&ls("abcdef")));
    assert!(result.contains(&ls("axbycz")));
}

#[test]
fn flex_match_case_insensitive() {
    crate::test_utils::init_test_tracing();
    let candidates = vec![ls("FindFile")];
    let result = flex_match(&ls("ff"), &candidates);
    assert_eq!(result.len(), 1);
}

#[test]
fn basic_match_case_sensitive() {
    crate::test_utils::init_test_tracing();
    let candidates = vec![ls("Apple"), ls("apple"), ls("application")];
    let result = basic_match(&ls("app"), &candidates);
    assert_eq!(result.len(), 2);
    assert!(result.contains(&ls("apple")));
    assert!(result.contains(&ls("application")));
    assert!(!result.contains(&ls("Apple")));
}

// -- Common prefix --------------------------------------------------------

#[test]
fn common_prefix_empty() {
    crate::test_utils::init_test_tracing();
    assert!(compute_common_prefix(&[]).is_none());
}

#[test]
fn common_prefix_single() {
    crate::test_utils::init_test_tracing();
    let strings = vec![ls("hello")];
    assert_eq!(compute_common_prefix(&strings), Some(ls("hello")));
}

#[test]
fn common_prefix_multiple() {
    crate::test_utils::init_test_tracing();
    let strings = vec![ls("application"), ls("apple"), ls("apply")];
    assert_eq!(compute_common_prefix(&strings), Some(ls("appl")));
}

#[test]
fn common_prefix_no_overlap() {
    crate::test_utils::init_test_tracing();
    let strings = vec![ls("abc"), ls("xyz")];
    assert_eq!(compute_common_prefix(&strings), Some(ls("")));
}

#[test]
fn common_prefix_identical() {
    crate::test_utils::init_test_tracing();
    let strings = vec![ls("test"), ls("test")];
    assert_eq!(compute_common_prefix(&strings), Some(ls("test")));
}

// -- History navigation ---------------------------------------------------

#[test]
fn history_navigation() {
    crate::test_utils::init_test_tracing();
    let mut mgr = MinibufferManager::new();
    mgr.add_to_history(intern("test-history"), "first", 100);
    mgr.add_to_history(intern("test-history"), "second", 100);
    mgr.add_to_history(intern("test-history"), "third", 100);

    // Enter minibuffer with history.
    mgr.read_from_minibuffer(BufferId(1), "prompt: ", None, Some(intern("test-history")))
        .unwrap();

    // Go back in history: should get "third" (most recent).
    let prev = mgr.history_previous();
    assert_eq!(prev, Some(ls("third")));

    // Go back again: "second".
    let prev = mgr.history_previous();
    assert_eq!(prev, Some(ls("second")));

    // Go forward: back to "third".
    let next = mgr.history_next();
    assert_eq!(next, Some(ls("third")));

    // Go forward again: back to original input (empty string).
    let next = mgr.history_next();
    assert_eq!(next, Some(ls("")));

    // Go forward past the start: None.
    let next = mgr.history_next();
    assert_eq!(next, None);

    // Clean up.
    mgr.exit_minibuffer();
}

#[test]
fn history_dedup() {
    crate::test_utils::init_test_tracing();
    let mut mgr = MinibufferManager::new();
    mgr.add_to_history(intern("h"), "same", 100);
    mgr.add_to_history(intern("h"), "same", 100);
    mgr.add_to_history(intern("h"), "same", 100);
    assert_eq!(mgr.history.get(intern("h")).len(), 1);

    mgr.add_to_history(intern("h"), "different", 100);
    assert_eq!(mgr.history.get(intern("h")).len(), 2);
    assert_eq!(
        crate::emacs_core::emacs_char::to_utf8_lossy(mgr.history.get(intern("h"))[0].as_bytes())
            .as_str(),
        "different"
    );
    assert_eq!(
        crate::emacs_core::emacs_char::to_utf8_lossy(mgr.history.get(intern("h"))[1].as_bytes())
            .as_str(),
        "same"
    );
}

// -- Recursive minibuffer depth -------------------------------------------

#[test]
fn recursive_depth() {
    crate::test_utils::init_test_tracing();
    let mut mgr = MinibufferManager::new();
    assert_eq!(mgr.depth(), 0);
    assert!(!mgr.is_active());

    mgr.read_from_minibuffer(BufferId(1), "1: ", None, None)
        .unwrap();
    assert_eq!(mgr.depth(), 1);
    assert!(mgr.is_active());

    mgr.read_from_minibuffer(BufferId(2), "2: ", None, None)
        .unwrap();
    assert_eq!(mgr.depth(), 2);

    mgr.exit_minibuffer();
    assert_eq!(mgr.depth(), 1);

    mgr.exit_minibuffer();
    assert_eq!(mgr.depth(), 0);
    assert!(!mgr.is_active());
}

#[test]
fn recursive_depth_limit() {
    crate::test_utils::init_test_tracing();
    let mut mgr = MinibufferManager::new();
    mgr.max_depth = 2;

    mgr.read_from_minibuffer(BufferId(1), "1: ", None, None)
        .unwrap();
    mgr.read_from_minibuffer(BufferId(2), "2: ", None, None)
        .unwrap();
    let result = mgr.read_from_minibuffer(BufferId(3), "3: ", None, None);
    assert!(result.is_err());
}

#[test]
fn exit_recursive_edit_rejects_top_level_command_loop_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.command_loop.recursive_depth = 1;

    let result = builtin_exit_recursive_edit(&mut eval, vec![]);
    assert!(matches!(
        result,
        Err(crate::emacs_core::error::Flow::Signal(sig))
            if sig.symbol_name() == "user-error"
    ));
}

#[test]
fn abort_recursive_edit_rejects_top_level_command_loop_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.command_loop.recursive_depth = 1;

    let result = builtin_abort_recursive_edit(&mut eval, vec![]);
    assert!(matches!(
        result,
        Err(crate::emacs_core::error::Flow::Signal(sig))
            if sig.symbol_name() == "user-error"
    ));
}

#[test]
fn recursive_disabled() {
    crate::test_utils::init_test_tracing();
    let mut mgr = MinibufferManager::new();

    mgr.read_from_minibuffer(BufferId(1), "1: ", None, None)
        .unwrap();
    assert_eq!(
        mgr.prepare_entry(RecursiveMinibufferPolicy::Reject)
            .expect_err("nested entry should be rejected"),
        MinibufferEntryRejection::RecursiveDisabled
    );
}

// -- Minibuffer enter/exit lifecycle --------------------------------------

#[test]
fn enter_exit_lifecycle() {
    crate::test_utils::init_test_tracing();
    let mut mgr = MinibufferManager::new();

    {
        let state = mgr
            .read_from_minibuffer(BufferId(1), "Enter: ", Some("init"), None)
            .unwrap();
        assert_eq!(
            state.prompt,
            crate::heap_types::LispString::from_utf8("Enter: ")
        );
        assert_eq!(
            crate::emacs_core::emacs_char::to_utf8_lossy(state.content.as_bytes()),
            "init"
        );
        assert!(state.active);
        assert_eq!(state.depth, 1);
    }

    // Modify content
    {
        let state = mgr.current_mut().unwrap();
        state.content = super::super::builtins::plain_str_to_lisp_string("modified", true);
    }

    let result = mgr.exit_minibuffer();
    assert_eq!(result, Some(ls("modified")));
    assert_eq!(mgr.depth(), 0);
}

#[test]
fn exit_with_default() {
    crate::test_utils::init_test_tracing();
    let mut mgr = MinibufferManager::new();
    {
        let state = mgr
            .read_from_minibuffer(BufferId(1), "Enter: ", None, None)
            .unwrap();
        state.default_value = Some(crate::heap_types::LispString::from_utf8("fallback"));
        // Content is empty, so default should be used.
    }
    let result = mgr.exit_minibuffer();
    assert_eq!(result, Some(ls("fallback")));
}

#[test]
fn abort_minibuffer_clears_state() {
    crate::test_utils::init_test_tracing();
    let mut mgr = MinibufferManager::new();
    mgr.read_from_minibuffer(BufferId(1), "Enter: ", None, None)
        .unwrap();
    assert_eq!(mgr.depth(), 1);
    mgr.abort_minibuffer();
    assert_eq!(mgr.depth(), 0);
    assert!(!mgr.is_active());
}

#[test]
fn exit_empty_stack() {
    crate::test_utils::init_test_tracing();
    let mut mgr = MinibufferManager::new();
    assert_eq!(mgr.exit_minibuffer(), None);
}

// -- MinibufferManager completion -----------------------------------------

#[test]
fn try_complete_with_table() {
    crate::test_utils::init_test_tracing();
    let mut mgr = MinibufferManager::new();
    {
        let state = mgr
            .read_from_minibuffer(BufferId(1), "M-x ", Some("find"), None)
            .unwrap();
        state.completion_table = Some(CompletionTable::List(vec![
            ls("find-file"),
            ls("find-file-other-window"),
            ls("find-tag"),
            ls("forward-char"),
        ]));
    }
    let state = mgr.current().unwrap();
    let result = mgr.try_complete(state);
    assert_eq!(result.matches.len(), 3); // find-file, find-file-other-window, find-tag
    assert_eq!(result.common_prefix, Some(ls("find-")));
    mgr.exit_minibuffer();
}

#[test]
fn test_completion_exact_match() {
    crate::test_utils::init_test_tracing();
    let mgr = MinibufferManager::new();
    let table = CompletionTable::List(vec![ls("apple"), ls("banana"), ls("cherry")]);
    assert!(mgr.test_completion(&ls("apple"), &table));
    assert!(mgr.test_completion(&ls("banana"), &table));
    assert!(!mgr.test_completion(&ls("app"), &table));
    assert!(!mgr.test_completion(&ls("APPLE"), &table));
}

#[test]
fn try_completion_string_result() {
    crate::test_utils::init_test_tracing();
    let mgr = MinibufferManager::new();
    let table = CompletionTable::List(vec![ls("application"), ls("apple"), ls("apply")]);
    let result = mgr.try_completion_string(&ls("app"), &table);
    assert_eq!(result, Some(ls("appl")));
}

#[test]
fn all_completions_empty() {
    crate::test_utils::init_test_tracing();
    let mgr = MinibufferManager::new();
    let table = CompletionTable::List(vec![ls("foo"), ls("bar")]);
    let result = mgr.all_completions(&ls("zzz"), &table);
    assert!(result.is_empty());
}

// -- Completion with different styles -------------------------------------

#[test]
fn completion_style_substring() {
    crate::test_utils::init_test_tracing();
    let mut mgr = MinibufferManager::new();
    mgr.set_completion_style(CompletionStyle::Substring);
    let table = CompletionTable::List(vec![ls("find-file"), ls("describe-file"), ls("file-name")]);
    let result = mgr.all_completions(&ls("file"), &table);
    assert_eq!(result.len(), 3); // All contain "file"
}

#[test]
fn completion_style_flex() {
    crate::test_utils::init_test_tracing();
    let mut mgr = MinibufferManager::new();
    mgr.set_completion_style(CompletionStyle::Flex);
    let table = CompletionTable::List(vec![ls("find-file"), ls("forward-char"), ls("flycheck")]);
    // "ff" should flex-match "find-file" and "flycheck" (f...f? no, flycheck has no second f)
    // Actually: "find-file" has f...f, "flycheck" has f but only one f total.
    let result = mgr.all_completions(&ls("ff"), &table);
    assert!(result.contains(&ls("find-file")));
    // "flycheck" has only one 'f', so "ff" won't match it.
    assert!(!result.contains(&ls("flycheck")));
}

#[test]
fn completion_style_basic_case_sensitive() {
    crate::test_utils::init_test_tracing();
    let mut mgr = MinibufferManager::new();
    mgr.set_completion_style(CompletionStyle::Basic);
    let table = CompletionTable::List(vec![ls("Apple"), ls("apple"), ls("application")]);
    let result = mgr.all_completions(&ls("app"), &table);
    assert_eq!(result.len(), 2);
    assert!(result.contains(&ls("apple")));
    assert!(result.contains(&ls("application")));
}

// -- Alist completion table -----------------------------------------------

#[test]
fn alist_completion() {
    crate::test_utils::init_test_tracing();
    let mgr = MinibufferManager::new();
    let table = CompletionTable::Alist(vec![
        (ls("alpha"), Value::fixnum(1)),
        (ls("beta"), Value::fixnum(2)),
        (ls("alphabetical"), Value::fixnum(3)),
    ]);
    let result = mgr.all_completions(&ls("alph"), &table);
    assert_eq!(result.len(), 2);
}

#[test]
fn builtin_try_completion_unique_exact() {
    crate::test_utils::init_test_tracing();
    // Exact unique match should return t.
    let mut eval = crate::emacs_core::eval::Context::new();
    let coll = Value::list(vec![Value::string("unique"), Value::string("other")]);
    let result = builtin_try_completion(&mut eval, vec![Value::string("unique"), coll]).unwrap();
    assert!(result.is_t());
}

#[test]
fn builtin_try_completion_common_prefix() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let coll = Value::list(vec![Value::string("application"), Value::string("apple")]);
    let result = builtin_try_completion(&mut eval, vec![Value::string("app"), coll]).unwrap();
    assert!(result.as_utf8_str().unwrap() == "appl");
}

#[test]
fn builtin_try_completion_ignore_case_matches_gnu_bestmatch_case() {
    crate::test_utils::init_test_tracing();
    // Faithful to GNU `Ftry_completion` (src/minibuf.c): with
    // completion-ignore-case the returned value carries the case pattern of
    // the candidate GNU selects as `bestmatch', not simply the first match.
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.assign("completion-ignore-case", Value::T);

    // (try-completion "A" '("abc" "ABC" "abd")) => "AB"  (GNU oracle).
    let coll = Value::list(vec![
        Value::string("abc"),
        Value::string("ABC"),
        Value::string("abd"),
    ]);
    let result = builtin_try_completion(&mut eval, vec![Value::string("A"), coll]).unwrap();
    assert_eq!(result.as_utf8_str().unwrap(), "AB");

    // coll = '("Alpha" "ALPHA" "alpha" "Beta").
    let coll = || {
        Value::list(vec![
            Value::string("Alpha"),
            Value::string("ALPHA"),
            Value::string("alpha"),
            Value::string("Beta"),
        ])
    };
    // (try-completion "a" coll) => "alpha"  (exact-case match preferred).
    let r = builtin_try_completion(&mut eval, vec![Value::string("a"), coll()]).unwrap();
    assert_eq!(r.as_utf8_str().unwrap(), "alpha");
    // (try-completion "A" coll) => "Alpha".
    let r = builtin_try_completion(&mut eval, vec![Value::string("A"), coll()]).unwrap();
    assert_eq!(r.as_utf8_str().unwrap(), "Alpha");

    // (try-completion "FOO" '("foobar" "FOOBAR")) => "FOOBAR".
    let coll = Value::list(vec![Value::string("foobar"), Value::string("FOOBAR")]);
    let r = builtin_try_completion(&mut eval, vec![Value::string("FOO"), coll]).unwrap();
    assert_eq!(r.as_utf8_str().unwrap(), "FOOBAR");
}

#[test]
fn builtin_try_completion_ignore_case_all_completions_lengths() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.assign("completion-ignore-case", Value::T);
    let coll = || {
        Value::list(vec![
            Value::string("Alpha"),
            Value::string("ALPHA"),
            Value::string("alpha"),
            Value::string("Beta"),
        ])
    };
    let a = builtin_all_completions(&mut eval, vec![Value::string("a"), coll()]).unwrap();
    let upper_a = builtin_all_completions(&mut eval, vec![Value::string("A"), coll()]).unwrap();
    assert_eq!(crate::emacs_core::value::list_to_vec(&a).unwrap().len(), 3);
    assert_eq!(
        crate::emacs_core::value::list_to_vec(&upper_a)
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn builtin_try_completion_no_match() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let coll = Value::list(vec![Value::string("foo"), Value::string("bar")]);
    let result = builtin_try_completion(&mut eval, vec![Value::string("zzz"), coll]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn builtin_try_completion_handles_raw_unibyte_candidates_without_panicking() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let coll = Value::list(vec![raw]);
    let result = builtin_try_completion(&mut eval, vec![Value::string(""), coll]);
    assert!(result.is_ok(), "try-completion should return a Lisp result");
}

#[test]
fn builtin_try_completion_accepts_gnu_obarray_objects() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let obarray = Value::obarray(3);
    let buckets = vec![
        Value::NIL,
        Value::list(vec![Value::symbol("neo-obarray-beta")]),
        Value::list(vec![Value::symbol("neo-obarray-alpha")]),
    ];
    assert!(crate::emacs_core::builtins::symbols::replace_obarray_buckets(obarray, buckets));

    let result = builtin_try_completion(&mut eval, vec![Value::string("neo-obarray-"), obarray])
        .expect("try-completion should accept GNU obarray objects");
    assert_eq!(result.as_utf8_str().unwrap(), "neo-obarray-");
}

#[test]
fn builtin_try_completion_rejects_more_than_three_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let coll = Value::list(vec![Value::string("a")]);
    let result = builtin_try_completion(
        &mut eval,
        vec![Value::string(""), coll, Value::NIL, Value::NIL],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn builtin_all_completions_returns_list() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let coll = Value::list(vec![
        Value::string("apple"),
        Value::string("application"),
        Value::string("banana"),
    ]);
    let result = builtin_all_completions(&mut eval, vec![Value::string("app"), coll]).unwrap();
    let items = super::super::value::list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 2);
}

#[test]
fn builtin_all_completions_rejects_more_than_three_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let coll = Value::list(vec![Value::string("a")]);
    let result = builtin_all_completions(
        &mut eval,
        vec![Value::string(""), coll, Value::NIL, Value::NIL],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn builtin_test_completion_match() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let coll = Value::list(vec![Value::string("alpha"), Value::string("beta")]);
    let result = builtin_test_completion(&mut eval, vec![Value::string("alpha"), coll]).unwrap();
    assert!(result.is_t());
}

#[test]
fn builtin_test_completion_no_match() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let coll = Value::list(vec![Value::string("alpha"), Value::string("beta")]);
    let result = builtin_test_completion(&mut eval, vec![Value::string("alp"), coll]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn builtin_test_completion_rejects_more_than_three_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let coll = Value::list(vec![Value::string("a")]);
    let result = builtin_test_completion(
        &mut eval,
        vec![Value::string(""), coll, Value::NIL, Value::NIL],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn builtin_try_completion_returns_raw_unibyte_common_prefix() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let input = Value::heap_string(LispString::from_unibyte(vec![0xFF]));
    let coll = Value::list(vec![
        Value::heap_string(LispString::from_unibyte(vec![0xFF, b'A'])),
        Value::heap_string(LispString::from_unibyte(vec![0xFF, b'B'])),
    ]);
    let result = builtin_try_completion(&mut eval, vec![input, coll]).unwrap();
    let string = result
        .as_lisp_string()
        .expect("raw completion result string");
    assert_eq!(
        crate::emacs_core::builtins::lisp_string_char_codes(string),
        vec![0xFF]
    );
}

#[test]
fn builtin_all_completions_preserves_raw_unibyte_candidates() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let input = Value::heap_string(LispString::from_unibyte(vec![0xFF]));
    let coll = Value::list(vec![
        Value::heap_string(LispString::from_unibyte(vec![0xFF, b'A'])),
        Value::heap_string(LispString::from_unibyte(vec![0xFF, b'B'])),
        Value::heap_string(LispString::from_unibyte(vec![0xFE, b'C'])),
    ]);
    let result = builtin_all_completions(&mut eval, vec![input, coll]).unwrap();
    let items = crate::emacs_core::value::list_to_vec(&result).expect("completion list");
    assert_eq!(items.len(), 2);
    assert_eq!(
        crate::emacs_core::builtins::lisp_string_char_codes(
            items[0].as_lisp_string().expect("first raw completion")
        ),
        vec![0xFF, b'A' as u32]
    );
    assert_eq!(
        crate::emacs_core::builtins::lisp_string_char_codes(
            items[1].as_lisp_string().expect("second raw completion")
        ),
        vec![0xFF, b'B' as u32]
    );
}

#[test]
fn builtin_all_completions_honors_raw_unibyte_completion_regexps() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.obarray.set_symbol_value(
        "completion-regexp-list",
        Value::list(vec![Value::heap_string(LispString::from_unibyte(vec![
            0xFF,
        ]))]),
    );
    let coll = Value::list(vec![
        Value::heap_string(LispString::from_unibyte(vec![0xFF])),
        Value::heap_string(LispString::from_unibyte(vec![0xFE])),
    ]);
    let result = builtin_all_completions(&mut eval, vec![Value::string(""), coll]).unwrap();
    let items = crate::emacs_core::value::list_to_vec(&result).expect("filtered completion list");
    assert_eq!(items.len(), 1);
    assert_eq!(
        crate::emacs_core::builtins::lisp_string_char_codes(
            items[0]
                .as_lisp_string()
                .expect("raw regexp-matched completion")
        ),
        vec![0xFF]
    );
}

#[test]
fn builtin_all_completions_completion_regexps_fold_when_completion_ignore_case() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.obarray
        .set_symbol_value("completion-ignore-case", Value::T);
    eval.obarray.set_symbol_value(
        "completion-regexp-list",
        Value::list(vec![Value::string("con")]),
    );
    let coll = Value::list(vec![
        Value::string("CONCAP"),
        Value::string("config"),
        Value::string("custom"),
    ]);

    let result = builtin_all_completions(&mut eval, vec![Value::string(""), coll]).unwrap();
    let items = crate::emacs_core::value::list_to_vec(&result).expect("filtered completion list");
    let names: Vec<&str> = items.iter().map(|v| v.as_utf8_str().unwrap()).collect();
    assert_eq!(names, vec!["CONCAP", "config"]);
}

#[test]
fn builtin_all_completions_signals_invalid_completion_regexp() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.obarray.set_symbol_value(
        "completion-regexp-list",
        Value::list(vec![Value::string("[")]),
    );
    let coll = Value::list(vec![Value::string("alpha")]);

    let err = builtin_all_completions(&mut eval, vec![Value::string(""), coll])
        .expect_err("invalid completion regexp should signal");
    let crate::emacs_core::error::Flow::Signal(signal) = err else {
        panic!("expected invalid-regexp signal, got {err:?}");
    };
    assert_eq!(
        crate::emacs_core::intern::resolve_sym(signal.symbol),
        "invalid-regexp"
    );
}

#[test]
fn builtin_minibuffer_depth_returns_zero() {
    crate::test_utils::init_test_tracing();
    let result = builtin_minibuffer_depth(vec![]).unwrap();
    assert!(result.is_fixnum());
}

#[test]
fn builtin_minibufferp_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_minibufferp(vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn eval_minibuffer_runtime_state_tracks_active_prompt_and_contents() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let minibuf_id = eval.buffers.create_buffer(" *Minibuf-1*");
    crate::emacs_core::minibuffer::install_minibuffer_buffer_text(
        &mut eval.buffers,
        minibuf_id,
        &crate::heap_types::LispString::from_utf8("Prompt: "),
        Some(&crate::heap_types::LispString::from_utf8("value")),
        crate::emacs_core::minibuffer::default_minibuffer_prompt_properties(),
    );
    eval.buffers.set_current(minibuf_id);
    eval.minibuffers
        .read_from_minibuffer(minibuf_id, "Prompt: ", Some("value"), None)
        .expect("enter minibuffer");

    assert_eq!(
        builtin_minibuffer_prompt_ctx(&mut eval, vec![]).unwrap(),
        Value::heap_string(crate::heap_types::LispString::from_utf8("Prompt: "))
    );
    assert_eq!(
        builtin_minibuffer_contents_ctx(&mut eval, vec![])
            .unwrap()
            .as_utf8_str(),
        Some("value")
    );
    assert_eq!(
        builtin_minibuffer_contents_no_properties_ctx(&mut eval, vec![])
            .unwrap()
            .as_utf8_str(),
        Some("value")
    );
    assert_eq!(
        builtin_minibuffer_depth_ctx(&mut eval, vec![]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_minibufferp_ctx(&mut eval, vec![]).unwrap(),
        Value::T
    );
    assert_eq!(
        builtin_minibufferp_ctx(&mut eval, vec![Value::NIL, Value::T]).unwrap(),
        Value::T
    );
    // GNU's `abort-minibuffers` does not throw `exit` itself (that is
    // `abort-recursive-edit`, meaning a plain `quit`).  It delegates to the
    // Lisp `minibuffer-quit-recursive-edit`, which is undefined in this bare
    // harness -- so the void-function signal names the delegation target.
    assert!(matches!(
        builtin_abort_minibuffers_ctx(&mut eval, vec![]),
        Err(flow) if format!("{flow:?}").contains("minibuffer-quit-recursive-edit")
    ));
}

#[test]
fn eval_minibuffer_runtime_state_preserves_raw_unibyte_prompt_and_contents() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let minibuf_id = eval.buffers.create_buffer(" *Minibuf-raw*");
    let raw_prompt = crate::heap_types::LispString::from_unibyte(vec![0xFF, b':', b' ']);
    crate::emacs_core::minibuffer::install_minibuffer_buffer_text(
        &mut eval.buffers,
        minibuf_id,
        &raw_prompt,
        Some(&crate::heap_types::LispString::from_utf8("value")),
        crate::emacs_core::minibuffer::default_minibuffer_prompt_properties(),
    );
    eval.buffers.set_current(minibuf_id);
    eval.minibuffers
        .read_from_minibuffer_lisp(
            minibuf_id,
            &raw_prompt,
            Some(&crate::heap_types::LispString::from_utf8("value")),
            None,
        )
        .expect("enter raw minibuffer");

    let prompt = builtin_minibuffer_prompt_ctx(&mut eval, vec![]).unwrap();
    assert_eq!(prompt.as_lisp_string().expect("prompt string"), &raw_prompt);

    let contents = builtin_minibuffer_contents_ctx(&mut eval, vec![]).unwrap();
    assert_eq!(contents.as_utf8_str(), Some("value"));
}

#[test]
fn install_minibuffer_buffer_text_applies_gnu_prompt_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let minibuf_id = eval.buffers.create_buffer(" *Minibuf-props*");
    let prompt_end = crate::emacs_core::minibuffer::install_minibuffer_buffer_text(
        &mut eval.buffers,
        minibuf_id,
        &crate::heap_types::LispString::from_utf8("Prompt: "),
        Some(&crate::heap_types::LispString::from_utf8("value")),
        crate::emacs_core::minibuffer::default_minibuffer_prompt_properties(),
    );
    let buf = eval.buffers.get(minibuf_id).expect("minibuffer buffer");

    assert_eq!(
        prompt_end,
        crate::buffer::EmacsBytePos::new("Prompt: ".len())
    );
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(0),
            Value::symbol("field")
        ),
        Some(Value::T)
    );
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(0),
            Value::symbol("front-sticky")
        ),
        Some(Value::T)
    );
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(0),
            Value::symbol("rear-nonsticky")
        ),
        Some(Value::T)
    );
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(0),
            Value::symbol("read-only")
        ),
        Some(Value::T)
    );
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(0),
            Value::symbol("face")
        ),
        None
    );
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(prompt_end, Value::symbol("read-only")),
        None
    );
}

#[test]
fn builtin_minibuffer_prompt_end_falls_back_to_point_min_without_prompt_field() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let minibuf_id = eval.buffers.create_buffer(" *Minibuf-1*");
    {
        let prompt_end = crate::emacs_core::minibuffer::install_minibuffer_buffer_text(
            &mut eval.buffers,
            minibuf_id,
            &crate::heap_types::LispString::from_utf8("Prompt: "),
            Some(&crate::heap_types::LispString::from_utf8("vm-mini")),
            crate::emacs_core::minibuffer::default_minibuffer_prompt_properties(),
        );
        let buf = eval.buffers.get_mut(minibuf_id).expect("minibuffer buffer");
        let _ = buf.text_props_remove_property_in_emacs_byte_range(
            crate::buffer::EmacsByteRange::new(crate::buffer::EmacsBytePos::new(0), prompt_end),
            Value::symbol("field"),
        );
    }
    eval.buffers.set_current(minibuf_id);
    eval.minibuffers
        .read_from_minibuffer(minibuf_id, "Prompt: ", Some("vm-mini"), None)
        .expect("enter minibuffer");

    assert_eq!(
        builtin_minibuffer_prompt_end_ctx(&mut eval, vec![]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_minibuffer_contents_ctx(&mut eval, vec![])
            .unwrap()
            .as_utf8_str(),
        Some("Prompt: vm-mini")
    );
}

#[test]
fn install_minibuffer_buffer_text_reuses_existing_buffer_via_buffer_edit_pipeline() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let minibuf_id = eval.buffers.create_buffer(" *Minibuf-reinstall*");

    let first_prompt_end = crate::emacs_core::minibuffer::install_minibuffer_buffer_text(
        &mut eval.buffers,
        minibuf_id,
        &crate::heap_types::LispString::from_utf8("Prompt: "),
        Some(&crate::heap_types::LispString::from_utf8("stale")),
        crate::emacs_core::minibuffer::default_minibuffer_prompt_properties(),
    );
    assert_eq!(
        first_prompt_end,
        crate::buffer::EmacsBytePos::new("Prompt: ".len())
    );
    assert_eq!(
        eval.buffers
            .get(minibuf_id)
            .expect("minibuffer buffer")
            .point_emacs_byte_pos()
            .get(),
        "Prompt: stale".len()
    );

    let second_prompt_end = crate::emacs_core::minibuffer::install_minibuffer_buffer_text(
        &mut eval.buffers,
        minibuf_id,
        &crate::heap_types::LispString::from_utf8("Switch to buffer: "),
        Some(&crate::heap_types::LispString::from_utf8("*Messages*")),
        crate::emacs_core::minibuffer::default_minibuffer_prompt_properties(),
    );

    assert_eq!(
        second_prompt_end,
        crate::buffer::EmacsBytePos::new("Switch to buffer: ".len())
    );
    let buf = eval.buffers.get(minibuf_id).expect("minibuffer buffer");
    assert_eq!(buf.buffer_string(), "Switch to buffer: *Messages*");
    assert_eq!(
        buf.point_emacs_byte_pos().get(),
        "Switch to buffer: *Messages*".len()
    );
}

#[test]
fn builtin_minibufferp_accepts_string_and_second_arg() {
    crate::test_utils::init_test_tracing();
    let result = builtin_minibufferp(vec![Value::string("x"), Value::NIL]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn builtin_minibufferp_rejects_non_buffer_like_values() {
    crate::test_utils::init_test_tracing();
    let result = builtin_minibufferp(vec![Value::fixnum(1)]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));
}

#[test]
fn builtin_minibufferp_rejects_more_than_two_args() {
    crate::test_utils::init_test_tracing();
    let result = builtin_minibufferp(vec![Value::NIL, Value::NIL, Value::NIL]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn builtin_recursive_edit_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = builtin_recursive_edit(&mut eval, vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn builtin_recursive_edit_rejects_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = builtin_recursive_edit(&mut eval, vec![Value::NIL]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn builtin_top_level_throws_top_level_tag() {
    crate::test_utils::init_test_tracing();
    let result = builtin_top_level(vec![]);
    // top-level now throws 'top-level to exit all recursive edits
    // (mirrors GNU Emacs keyboard.c:1187 Ftop_level).
    assert!(matches!(
        result,
        Err(Flow::Throw(ref thrown))
            if thrown.tag.is_symbol_named("top-level") && thrown.value.is_nil()
    ));
}

#[test]
fn builtin_top_level_rejects_args() {
    crate::test_utils::init_test_tracing();
    let result = builtin_top_level(vec![Value::NIL]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn builtin_exit_recursive_edit_signals_user_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = builtin_exit_recursive_edit(&mut eval, vec![]);
    // Not in a recursive edit → user-error
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "user-error"
    ));
}

#[test]
fn builtin_exit_recursive_edit_rejects_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = builtin_exit_recursive_edit(&mut eval, vec![Value::NIL]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn builtin_minibuffer_contents_returns_current_buffer_text() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    eval.buffers
        .current_buffer_mut()
        .expect("scratch buffer")
        .insert("probe");
    let result = builtin_minibuffer_contents_ctx(&mut eval, vec![]).unwrap();
    assert!(result.as_utf8_str().unwrap() == "probe");
}

#[test]
fn builtin_minibuffer_contents_respects_narrowing_in_current_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("scratch buffer");
        buf.insert("012345");
        buf.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(1, 4));
    }

    let contents = builtin_minibuffer_contents_ctx(&mut eval, vec![]).unwrap();
    assert_eq!(contents.as_utf8_str(), Some("123"));
    let plain = builtin_minibuffer_contents_no_properties_ctx(&mut eval, vec![]).unwrap();
    assert_eq!(plain.as_utf8_str(), Some("123"));
}

#[test]
fn builtin_minibuffer_contents_no_properties_drops_properties_from_current_buffer_text() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let buffer_id = eval.buffers.current_buffer_id().expect("scratch buffer");
    eval.buffers
        .current_buffer_mut()
        .expect("scratch buffer")
        .insert("probe");
    eval.buffers
        .put_buffer_text_property_in_emacs_byte_range(
            buffer_id,
            crate::buffer::EmacsByteRange::from_usize(0, 5),
            Value::symbol("face"),
            Value::symbol("bold"),
        )
        .expect("put buffer text property");

    let rich = builtin_minibuffer_contents_ctx(&mut eval, vec![]).unwrap();
    assert!(
        crate::emacs_core::value::get_string_text_properties_table_for_value(rich).is_some(),
        "minibuffer-contents should retain the buffer's properties"
    );
    let result = builtin_minibuffer_contents_no_properties_ctx(&mut eval, vec![]).unwrap();
    assert!(result.as_utf8_str().unwrap() == "probe");
    assert!(
        crate::emacs_core::value::get_string_text_properties_table_for_value(result).is_none(),
        "minibuffer-contents-no-properties should return an interval-free string"
    );
}

#[test]
fn builtin_minibuffer_contents_no_properties_rejects_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = builtin_minibuffer_contents_no_properties_ctx(&mut eval, vec![Value::NIL]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn builtin_exit_minibuffer_throws_exit_tag() {
    crate::test_utils::init_test_tracing();
    let result = builtin_exit_minibuffer(vec![]);
    assert!(matches!(
        result,
        Err(Flow::Throw(ref thrown))
            if thrown.tag.is_symbol_named("exit") && thrown.value.is_nil()
    ));
}

#[test]
fn builtin_abort_minibuffers_signals_not_in_minibuffer_error() {
    crate::test_utils::init_test_tracing();
    let result = builtin_abort_minibuffers(vec![]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "error"
                && matches!(sig.data.as_slice(), [val] if val.as_utf8_str().map(|s| s == "Not in a minibuffer").unwrap_or(false))
    ));
}

#[test]
fn builtin_abort_minibuffers_rejects_args() {
    crate::test_utils::init_test_tracing();
    let result = builtin_abort_minibuffers(vec![Value::NIL]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn builtin_abort_recursive_edit_signals_user_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = builtin_abort_recursive_edit(&mut eval, vec![]);
    // Not in a recursive edit → user-error
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "user-error"
    ));
}

#[test]
fn builtin_abort_recursive_edit_rejects_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = builtin_abort_recursive_edit(&mut eval, vec![Value::NIL]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn builtin_read_file_name_signals_end_of_file() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = builtin_read_file_name(
        &mut eval,
        vec![
            Value::string("File: "),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::string("/tmp/test.txt"),
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "end-of-file"
                && matches!(sig.data.as_slice(), [val] if val.as_utf8_str().map(|s| s == "Error reading from stdin").unwrap_or(false))
    ));
}

#[test]
fn builtin_read_file_name_validates_dir_default_and_initial() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let bad_dir =
        builtin_read_file_name(&mut eval, vec![Value::string("File: "), Value::fixnum(1)]);
    assert!(matches!(
        bad_dir,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));

    let bad_default = builtin_read_file_name(
        &mut eval,
        vec![Value::string("File: "), Value::NIL, Value::fixnum(1)],
    );
    assert!(matches!(
        bad_default,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));

    let bad_initial = builtin_read_file_name(
        &mut eval,
        vec![
            Value::string("File: "),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(1),
        ],
    );
    assert!(matches!(
        bad_initial,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));
}

#[test]
fn builtin_read_file_name_rejects_more_than_six_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = builtin_read_file_name(
        &mut eval,
        vec![
            Value::string("File: "),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn builtin_read_buffer_signals_end_of_file() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = builtin_read_buffer(
        &mut eval,
        vec![Value::string("Buffer: "), Value::string("*scratch*")],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file"
    ));
}

#[test]
fn builtin_read_directory_name_rejects_more_than_five_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = builtin_read_directory_name(
        &mut eval,
        vec![
            Value::string("Directory: "),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn builtin_read_directory_name_validates_dir_default_and_initial() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let bad_dir = builtin_read_directory_name(
        &mut eval,
        vec![Value::string("Directory: "), Value::fixnum(1)],
    );
    assert!(matches!(
        bad_dir,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));

    let bad_default = builtin_read_directory_name(
        &mut eval,
        vec![Value::string("Directory: "), Value::NIL, Value::fixnum(1)],
    );
    assert!(matches!(
        bad_default,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));

    let bad_initial = builtin_read_directory_name(
        &mut eval,
        vec![
            Value::string("Directory: "),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(1),
        ],
    );
    assert!(matches!(
        bad_initial,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));
}

#[test]
fn builtin_read_buffer_rejects_more_than_four_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = builtin_read_buffer(
        &mut eval,
        vec![
            Value::string("Buffer: "),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn builtin_read_command_rejects_more_than_two_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = builtin_read_command(
        &mut eval,
        vec![Value::string("Command: "), Value::NIL, Value::NIL],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn builtin_read_variable_rejects_more_than_two_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = builtin_read_variable(
        &mut eval,
        vec![Value::string("Variable: "), Value::NIL, Value::NIL],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

// -- value_to_string_list -------------------------------------------------

#[test]
fn value_to_string_list_from_list() {
    crate::test_utils::init_test_tracing();
    let list = Value::list(vec![
        Value::string("foo"),
        Value::string("bar"),
        Value::string("baz"),
    ]);
    let result = value_to_string_list(&list);
    assert_eq!(result, vec!["foo", "bar", "baz"]);
}

#[test]
fn value_to_string_list_from_alist() {
    crate::test_utils::init_test_tracing();
    let alist = Value::list(vec![
        Value::cons(Value::string("key1"), Value::fixnum(1)),
        Value::cons(Value::string("key2"), Value::fixnum(2)),
    ]);
    let result = value_to_string_list(&alist);
    assert_eq!(result, vec!["key1", "key2"]);
}

#[test]
fn value_to_string_list_from_nil() {
    crate::test_utils::init_test_tracing();
    let result = value_to_string_list(&Value::NIL);
    assert!(result.is_empty());
}

#[test]
fn value_to_string_list_from_vector() {
    crate::test_utils::init_test_tracing();
    let vec = Value::vector(vec![Value::string("a"), Value::string("b")]);
    let result = value_to_string_list(&vec);
    assert_eq!(result, vec!["a", "b"]);
}

/// GNU `Fall_completions` converts symbol candidates with `Fsymbol_name`
/// (`src/minibuf.c`), so completion observes the exact mutable Lisp string
/// stored in the symbol rather than the immutable atom used for identity.
#[test]
fn symbol_completion_observes_the_lisp_visible_mutated_name() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = eval
        .eval_str(
            r##"(let* ((symbol (make-symbol "foo"))
         (name (symbol-name symbol))
         (global-name (copy-sequence "global-completion-name-probe"))
         (global-symbol (intern global-name)))
    (aset name 0 ?b)
    (aset global-name 0 ?X)
    (format "%S"
            (list (all-completions "b" (list symbol))
                  (try-completion "b" (list symbol))
                  (test-completion "boo" (list symbol))
                  (all-completions "f" (list symbol))
                  (all-completions "Xlobal-completion-name-probe"
                                   obarray))))"##,
        )
        .expect("mutable symbol-name completion program");

    assert_eq!(
        result.as_utf8_str(),
        Some("((\"boo\") \"boo\" t nil (\"Xlobal-completion-name-probe\"))")
    );
}

/// GNU `Fall_completions` conses the exact `Fsymbol_name` object into its
/// result.  Bootstrap/Rust-created symbols start from immutable name atoms in
/// Neomacs, but their first Lisp-visible completion must materialize and retain
/// one object rather than cloning the atom for every TAB.
#[test]
fn atom_backed_completion_returns_the_symbols_materialized_name_object() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.obarray
        .intern("atom-backed-completion-name-identity-probe");

    let result = eval
        .eval_str(
            r##"(let ((matches
                    (all-completions
                     "atom-backed-completion-name-identity-probe" obarray)))
                (eq (car matches)
                    (symbol-name 'atom-backed-completion-name-identity-probe)))"##,
        )
        .expect("atom-backed completion identity probe");

    assert_eq!(result, Value::T);
}

/// GNU `Fall_completions` (`src/minibuf.c`) advances one `obarray_iter_t`
/// entry at a time and constructs only the surviving completion result.  The
/// Neomacs ownership boundary needs one rooted candidate collection because a
/// Lisp predicate may run GC, but it must not first materialize parallel
/// full-obarray symbol and name collections.
#[test]
fn global_obarray_completion_has_no_pre_candidate_staging_collections() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    crate::emacs_core::builtins::symbols::reset_global_obarray_symbol_value_materializations();
    eval.eval_str(r##"(all-completions "global-obarray-stage-probe" obarray)"##)
        .expect("global obarray completion");

    assert_eq!(
        crate::emacs_core::builtins::symbols::global_obarray_symbol_value_materializations(),
        0,
        "global obarray completion must stream cached ids and visible names directly into the rooted candidate set"
    );
}

#[test]
fn completion_candidate_keeps_the_global_obarray_scan_compact() {
    assert!(
        std::mem::size_of::<CompletionCandidate>() <= 5 * std::mem::size_of::<usize>(),
        "completion candidates should carry compact ownership handles, not inline LispString payloads; got {} bytes",
        std::mem::size_of::<CompletionCandidate>()
    );
}

/// GNU keeps these hot completion controls in predeclared
/// `completion_ignore_case` / `Vcompletion_regexp_list` C state. Reading them
/// for every TAB must use their cached symbol identities rather than returning
/// to the global string interner.
#[test]
fn completion_state_reads_use_predeclared_symbols() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.obarray
        .set_symbol_value("completion-ignore-case", Value::T);
    eval.obarray.set_symbol_value(
        "completion-regexp-list",
        Value::list(vec![Value::string("neo")]),
    );

    assert!(completion_ignore_case(&eval.obarray));
    assert_eq!(
        completion_regexp_lisp_list_from_obarray(&eval.obarray),
        vec![LispString::from_unibyte(b"neo".to_vec())]
    );

    crate::emacs_core::intern::reset_intern_calls();
    assert!(completion_ignore_case(&eval.obarray));
    assert_eq!(
        completion_regexp_lisp_list_from_obarray(&eval.obarray),
        vec![LispString::from_unibyte(b"neo".to_vec())]
    );
    assert_eq!(
        crate::emacs_core::intern::intern_calls(),
        0,
        "completion state reads must use GNU-shaped predeclared identities"
    );
}

/// GNU's empty-prefix M-x scan walks predeclared symbol identities and function
/// cells; after initialization it does not return to the string interner for
/// each candidate.  Keep that invariant at the whole `all-completions` entry
/// point so a name-based helper cannot hide below `commandp` again.
#[test]
fn repeated_mx_completion_does_not_intern_during_candidate_scan() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let obarray = *eval
        .obarray
        .symbol_value("obarray")
        .expect("global obarray proxy");
    let args = vec![
        Value::string(""),
        obarray,
        Value::from_sym_id(crate::emacs_core::interactive::CommandpSymbol::id()),
    ];

    let _ = builtin_all_completions(&mut eval, args.clone()).expect("warm M-x completion");
    crate::emacs_core::intern::reset_intern_calls();
    let _ = builtin_all_completions(&mut eval, args).expect("steady-state M-x completion");

    assert_eq!(
        crate::emacs_core::intern::intern_calls(),
        0,
        "steady-state M-x candidate scanning must use symbol identities; interned {:?}",
        crate::emacs_core::intern::intern_call_names()
    );
}

/// GNU `Ftry_completion` / `Fall_completions` compare the predicate with the
/// predeclared `Qcommandp` and call `Fcommandp` directly.  This is observable,
/// not merely an optimization: rebinding the symbol's function cell does not
/// replace the primitive predicate used for an obarray completion.  In
/// contrast, `Ftest_completion` has no such special case and must keep ordinary
/// Lisp function dispatch.  GNU 31 returns `(nil nil t)` for this probe.
#[test]
fn obarray_completion_uses_gnu_commandp_primitive_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = eval
        .eval_str(
            r##"(let ((old (symbol-function 'commandp))
         (table (obarray-make)))
    (intern "not-a-command" table)
    (unwind-protect
        (progn
          (fset 'commandp (lambda (_) t))
          (list (try-completion "" table 'commandp)
                (all-completions "" table 'commandp)
                (test-completion "not-a-command" table 'commandp)))
      (fset 'commandp old)))"##,
        )
        .expect("commandp completion dispatch");

    assert_eq!(
        result,
        Value::list(vec![Value::NIL, Value::NIL, Value::T]),
        "only GNU's scanning completion primitives bypass commandp's rebound function cell"
    );
}

/// Task #26: GNU filters completion candidates against
/// `completion-regexp-list` through `fast_string_match_internal`
/// (`src/minibuf.c:1592` `match_regexps`, `Ftry_completion`), which arms
/// `re_match_object` = the candidate string and
/// `RE_SETUP_SYNTAX_TABLE_FOR_OBJECT` (`src/syntax.c:277`):
/// `SETUP_BUFFER_SYNTAX_TABLE` makes `\sw` classify by the CURRENT BUFFER's
/// syntax table, buffer-local table included. Measured GNU 31 (buffer-local
/// copy of the standard table, `?z` made whitespace): all-completions →
/// ("abc"), try-completion → "abc", test-completion "zzz" → nil.
#[test]
fn completion_regexp_list_reads_current_buffer_syntax_table() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.eval_str("(set-syntax-table (copy-syntax-table (standard-syntax-table)))")
        .expect("set-syntax-table");
    eval.eval_str("(modify-syntax-entry ?z \" \")")
        .expect("modify-syntax-entry");
    eval.obarray.set_symbol_value(
        "completion-regexp-list",
        Value::list(vec![Value::string("\\`\\sw+\\'")]),
    );

    let coll = Value::list(vec![Value::string("abc"), Value::string("zzz")]);
    let all = builtin_all_completions(&mut eval, vec![Value::string(""), coll]).unwrap();
    let names: Vec<String> = crate::emacs_core::value::list_to_vec(&all)
        .expect("completion list")
        .iter()
        .map(|v| v.as_utf8_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        vec!["abc"],
        "all-completions: \\sw must classify by the buffer's syntax table"
    );

    let coll = Value::list(vec![Value::string("abc"), Value::string("zzz")]);
    let tried = builtin_try_completion(&mut eval, vec![Value::string(""), coll]).unwrap();
    assert_eq!(
        tried.as_utf8_str(),
        Some("abc"),
        "try-completion must filter zzz through the buffer's syntax table"
    );

    let coll = Value::list(vec![Value::string("abc"), Value::string("zzz")]);
    let tested = builtin_test_completion(&mut eval, vec![Value::string("zzz"), coll]).unwrap();
    assert!(
        tested.is_nil(),
        "test-completion: zzz is not \\sw+ under the buffer table"
    );
}

/// Task #26: the candidate string's own `syntax-table` text properties apply
/// under `parse-sexp-lookup-properties`, exactly as for the Lisp-visible
/// `string-match` — GNU's `re_match_object` is the candidate string itself.
/// Measured GNU 31: gate off → 2 survivors, gate on → 1 ("abc"; class 6 =
/// expression prefix is not `\sw`).
#[test]
fn completion_regexp_list_reads_candidate_syntax_table_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let out = eval
        .eval_str(
            r#"(let ((cand (copy-sequence "zzz"))
      (completion-regexp-list '("\\`\\sw+\\'")))
  (put-text-property 0 3 'syntax-table '(6) cand)
  (format "%S"
          (list (length (all-completions "" (list cand "abc")))
                (length (let ((parse-sexp-lookup-properties t))
                          (all-completions "" (list cand "abc")))))))"#,
        )
        .expect("propertized completion program");
    assert_eq!(
        out.as_utf8_str(),
        Some("(2 1)"),
        "candidate syntax-table properties must be honored only under parse-sexp-lookup-properties"
    );
}
