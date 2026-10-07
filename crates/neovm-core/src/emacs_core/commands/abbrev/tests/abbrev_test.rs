use super::*;
use crate::heap_types::LispString;

// -----------------------------------------------------------------------
// AbbrevManager unit tests (legacy -- kept for pdump compatibility)
// -----------------------------------------------------------------------

fn abbrev_runtime(text: &LispString) -> String {
    super::abbrev_string_to_runtime(text)
}

#[test]
fn define_and_expand() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AbbrevManager::new();
    mgr.define_abbrev("global-abbrev-table", "btw", "by the way");

    let result = mgr.expand_abbrev("global-abbrev-table", "btw");
    assert_eq!(result, Some("by the way".to_string()));

    // Check count incremented
    let tbl = mgr.get_table("global-abbrev-table").unwrap();
    let key_btw = super::runtime_string_to_abbrev_string("btw");
    assert_eq!(tbl.abbrevs.get(&key_btw).unwrap().count, 1);

    // Expand again
    let result = mgr.expand_abbrev("global-abbrev-table", "btw");
    assert_eq!(result, Some("by the way".to_string()));

    let tbl = mgr.get_table("global-abbrev-table").unwrap();
    assert_eq!(tbl.abbrevs.get(&key_btw).unwrap().count, 2);
}

#[test]
fn expand_nonexistent() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AbbrevManager::new();
    let result = mgr.expand_abbrev("global-abbrev-table", "nope");
    assert!(result.is_none());
}

#[test]
fn case_insensitive_lookup() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AbbrevManager::new();
    mgr.define_abbrev("global-abbrev-table", "BTW", "by the way");

    // Stored as lowercase key "btw"
    let result = mgr.expand_abbrev("global-abbrev-table", "btw");
    assert_eq!(result, Some("by the way".to_string()));

    let result = mgr.expand_abbrev("global-abbrev-table", "BTW");
    // All-uppercase input -> all-uppercase expansion
    assert_eq!(result, Some("BY THE WAY".to_string()));
}

#[test]
fn case_capitalized() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AbbrevManager::new();
    mgr.define_abbrev("global-abbrev-table", "btw", "by the way");

    // Capitalized input -> capitalized expansion
    let result = mgr.expand_abbrev("global-abbrev-table", "Btw");
    assert_eq!(result, Some("By the way".to_string()));
}

#[test]
fn case_fixed() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AbbrevManager::new();
    mgr.define_abbrev("global-abbrev-table", "btw", "by the way");
    mgr.tables
        .get_mut(&abbrev_table_sym("global-abbrev-table"))
        .unwrap()
        .case_fixed = true;

    // With case_fixed, expansion is returned verbatim regardless of input case
    let result = mgr.expand_abbrev("global-abbrev-table", "BTW");
    assert_eq!(result, Some("by the way".to_string()));
}

#[test]
fn table_inheritance() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AbbrevManager::new();

    // Define in global
    mgr.define_abbrev("global-abbrev-table", "btw", "by the way");

    // Create a child table with parent
    let child = mgr.create_table("lisp-mode-abbrev-table");
    child.parent = Some(runtime_string_to_abbrev_string("global-abbrev-table"));

    // Define a local abbrev in child
    mgr.define_abbrev("lisp-mode-abbrev-table", "df", "defun");

    // Child table can find its own abbrevs
    let result = mgr.expand_abbrev("lisp-mode-abbrev-table", "df");
    assert_eq!(result, Some("defun".to_string()));

    // Child table inherits from parent
    let result = mgr.expand_abbrev("lisp-mode-abbrev-table", "btw");
    assert_eq!(result, Some("by the way".to_string()));
}

#[test]
fn fallback_to_global() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AbbrevManager::new();

    mgr.define_abbrev("global-abbrev-table", "teh", "the");
    mgr.create_table("text-mode-abbrev-table");
    // No parent set, but should still fall back to global
    let result = mgr.expand_abbrev("text-mode-abbrev-table", "teh");
    assert_eq!(result, Some("the".to_string()));
}

#[test]
fn list_abbrevs_sorted() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AbbrevManager::new();
    mgr.define_abbrev("global-abbrev-table", "zz", "sleep");
    mgr.define_abbrev("global-abbrev-table", "aa", "alpha");
    mgr.define_abbrev("global-abbrev-table", "mm", "middle");

    let list = mgr.list_abbrevs("global-abbrev-table");
    assert_eq!(list.len(), 3);
    assert_eq!(list[0], ("aa".to_string(), "alpha".to_string()));
    assert_eq!(list[1], ("mm".to_string(), "middle".to_string()));
    assert_eq!(list[2], ("zz".to_string(), "sleep".to_string()));
}

#[test]
fn list_abbrevs_nonexistent_table() {
    crate::test_utils::init_test_tracing();
    let mgr = AbbrevManager::new();
    let list = mgr.list_abbrevs("no-such-table");
    assert!(list.is_empty());
}

#[test]
fn clear_table() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AbbrevManager::new();
    mgr.define_abbrev("global-abbrev-table", "a", "alpha");
    mgr.define_abbrev("global-abbrev-table", "b", "beta");
    assert_eq!(mgr.list_abbrevs("global-abbrev-table").len(), 2);

    mgr.clear_table("global-abbrev-table");
    assert_eq!(mgr.list_abbrevs("global-abbrev-table").len(), 0);
}

#[test]
fn enable_disable() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AbbrevManager::new();
    assert!(!mgr.is_enabled());

    mgr.set_enabled(true);
    assert!(mgr.is_enabled());

    mgr.set_enabled(false);
    assert!(!mgr.is_enabled());
}

#[test]
fn define_abbrev_full_with_hook_and_system() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AbbrevManager::new();
    mgr.define_abbrev_full(
        "global-abbrev-table",
        "hw",
        "hello world",
        Some(LispString::from_utf8("my-hook")),
        true,
    );

    let tbl = mgr.get_table("global-abbrev-table").unwrap();
    let key_hw = super::runtime_string_to_abbrev_string("hw");
    let ab = tbl.abbrevs.get(&key_hw).unwrap();
    assert_eq!(abbrev_runtime(&ab.expansion), "hello world");
    assert_eq!(
        ab.hook.as_ref().map(abbrev_runtime).as_deref(),
        Some("my-hook")
    );
    assert!(ab.system);
    assert_eq!(ab.count, 0);
}

#[test]
fn all_table_names() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AbbrevManager::new();
    mgr.create_table("z-table");
    mgr.create_table("a-table");

    let names = mgr.all_table_names();
    // Should include global + the two we created, sorted
    assert!(names.contains(&runtime_string_to_abbrev_string("a-table")));
    assert!(names.contains(&runtime_string_to_abbrev_string("global-abbrev-table")));
    assert!(names.contains(&runtime_string_to_abbrev_string("z-table")));
    // Verify sorting
    for i in 1..names.len() {
        assert!(abbrev_runtime(&names[i - 1]) <= abbrev_runtime(&names[i]));
    }
}

// -----------------------------------------------------------------------
// apply_case unit tests
// -----------------------------------------------------------------------

#[test]
fn test_apply_case() {
    crate::test_utils::init_test_tracing();
    // Lowercase word -> as-is
    assert_eq!(apply_case("hello world", "hw", false), "hello world");

    // Capitalized word -> capitalize expansion
    assert_eq!(apply_case("hello world", "Hw", false), "Hello world");

    // All-uppercase word -> uppercase expansion
    assert_eq!(apply_case("hello world", "HW", false), "HELLO WORLD");

    // case_fixed -> always as-is
    assert_eq!(apply_case("hello world", "HW", true), "hello world");

    // Empty word/expansion
    assert_eq!(apply_case("", "HW", false), "");
    assert_eq!(apply_case("hello", "", false), "hello");
}

// -----------------------------------------------------------------------
// Obarray-based builtin tests
// -----------------------------------------------------------------------

#[test]
fn test_make_abbrev_table_and_predicate() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // make-abbrev-table creates an abbrev table
    let table = builtin_make_abbrev_table(&mut eval, vec![]).unwrap();
    assert!(table.is_obarray());

    // abbrev-table-p returns true for it
    let result = builtin_abbrev_table_p(&mut eval, vec![table]).unwrap();
    assert!(result.is_truthy());

    // Non-tables return nil
    let result = builtin_abbrev_table_p(&mut eval, vec![Value::NIL]).unwrap();
    assert!(result.is_nil());

    let result = builtin_abbrev_table_p(&mut eval, vec![Value::fixnum(42)]).unwrap();
    assert!(result.is_nil());

    // A plain vector is not an abbrev table
    let plain_vec = Value::vector(vec![Value::fixnum(0); 10]);
    let result = builtin_abbrev_table_p(&mut eval, vec![plain_vec]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn test_define_abbrev_and_lookup() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    let table = builtin_make_abbrev_table(&mut eval, vec![]).unwrap();

    // Define an abbreviation
    let defined = builtin_define_abbrev(
        &mut eval,
        vec![table, Value::string("btw"), Value::string("by the way")],
    )
    .unwrap();
    assert_eq!(defined.as_utf8_str(), Some("btw"));

    // Look up via abbrev-expansion
    let result = builtin_abbrev_expansion(&mut eval, vec![Value::string("btw"), table]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("by the way"));

    // Look up via abbrev-symbol
    let sym = builtin_abbrev_symbol(&mut eval, vec![Value::string("btw"), table]).unwrap();
    assert!(sym.is_truthy());
    assert_eq!(sym.as_symbol_name(), Some("btw"));

    // Non-existent
    let result = builtin_abbrev_expansion(&mut eval, vec![Value::string("xyz"), table]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn abbrev_system_setting_matches_gnu_force_domain() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        AbbrevSystemSetting::from_system_flag(Value::NIL),
        AbbrevSystemSetting::User
    );
    assert_eq!(
        AbbrevSystemSetting::from_system_flag(Value::T),
        AbbrevSystemSetting::System
    );
    assert_eq!(
        AbbrevSystemSetting::from_system_flag(Value::symbol("force")),
        AbbrevSystemSetting::Force
    );
    assert_eq!(
        AbbrevSystemSetting::from_system_flag(Value::keyword(":force")),
        AbbrevSystemSetting::System
    );
    assert_eq!(
        AbbrevSystemSetting::from_system_flag(Value::string("force")),
        AbbrevSystemSetting::System
    );
}

#[test]
fn define_abbrev_system_force_matches_gnu_overwrite_and_storage() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    let table = builtin_make_abbrev_table(&mut eval, vec![]).unwrap();

    builtin_define_abbrev(
        &mut eval,
        vec![table, Value::string("x"), Value::string("user")],
    )
    .unwrap();
    eval.obarray_mut()
        .set_symbol_value("abbrevs-changed", Value::NIL);

    builtin_define_abbrev(
        &mut eval,
        vec![
            table,
            Value::string("x"),
            Value::string("system"),
            Value::NIL,
            Value::keyword(":system"),
            Value::T,
        ],
    )
    .unwrap();

    let expansion = builtin_abbrev_expansion(&mut eval, vec![Value::string("x"), table]).unwrap();
    assert_eq!(expansion.as_utf8_str(), Some("user"));
    let sym = builtin_abbrev_symbol(&mut eval, vec![Value::string("x"), table]).unwrap();
    let sym_id = symbol_id(sym).expect("abbrev symbol");
    assert!(
        eval.obarray()
            .get_property_id(sym_id, intern(":system"))
            .unwrap_or(Value::NIL)
            .is_nil()
    );

    builtin_define_abbrev(
        &mut eval,
        vec![
            table,
            Value::string("x"),
            Value::string("forced"),
            Value::NIL,
            Value::keyword(":system"),
            Value::symbol("force"),
        ],
    )
    .unwrap();

    let expansion = builtin_abbrev_expansion(&mut eval, vec![Value::string("x"), table]).unwrap();
    assert_eq!(expansion.as_utf8_str(), Some("forced"));
    assert_eq!(
        eval.obarray()
            .get_property_id(sym_id, intern(":system"))
            .unwrap_or(Value::NIL),
        Value::T
    );
    assert!(
        eval.obarray()
            .symbol_value("abbrevs-changed")
            .copied()
            .unwrap_or(Value::NIL)
            .is_nil()
    );
}

#[test]
fn test_clear_abbrev_table() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    let table = builtin_make_abbrev_table(&mut eval, vec![]).unwrap();

    // Define some abbrevs
    builtin_define_abbrev(
        &mut eval,
        vec![table, Value::string("a"), Value::string("alpha")],
    )
    .unwrap();
    builtin_define_abbrev(
        &mut eval,
        vec![table, Value::string("b"), Value::string("beta")],
    )
    .unwrap();

    // Verify they exist
    let result = builtin_abbrev_expansion(&mut eval, vec![Value::string("a"), table]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("alpha"));

    // Clear
    builtin_clear_abbrev_table(&mut eval, vec![table]).unwrap();

    // Verify gone
    let result = builtin_abbrev_expansion(&mut eval, vec![Value::string("a"), table]).unwrap();
    assert!(result.is_nil());

    // Table is still an abbrev table
    let result = builtin_abbrev_table_p(&mut eval, vec![table]).unwrap();
    assert!(result.is_truthy());
}

#[test]
fn test_abbrev_get_put() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    let table = builtin_make_abbrev_table(&mut eval, vec![]).unwrap();

    // Define an abbreviation
    builtin_define_abbrev(
        &mut eval,
        vec![table, Value::string("hw"), Value::string("hello world")],
    )
    .unwrap();
    let _sym = builtin_abbrev_symbol(&mut eval, vec![Value::string("hw"), table]).unwrap();
}

#[test]
fn test_define_abbrev_table_and_lookup() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // define-abbrev-table creates a named table
    builtin_define_abbrev_table(&mut eval, vec![Value::symbol("test-table"), Value::NIL]).unwrap();

    // The symbol value should be an abbrev table
    let table = eval.obarray().symbol_value("test-table").cloned().unwrap();
    let result = builtin_abbrev_table_p(&mut eval, vec![table]).unwrap();
    assert!(result.is_truthy());
}

#[test]
fn test_insert_abbrev_table_description_writes_buffer_text() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    builtin_define_abbrev_table(&mut eval, vec![Value::symbol("test-table"), Value::NIL]).unwrap();

    let table = eval
        .obarray()
        .symbol_value("test-table")
        .cloned()
        .expect("test-table value");
    builtin_define_abbrev(
        &mut eval,
        vec![
            table,
            Value::string("btw"),
            Value::string("by the way"),
            Value::NIL,
            Value::fixnum(7),
        ],
    )
    .unwrap();

    builtin_insert_abbrev_table_description(&mut eval, vec![Value::symbol("test-table")]).unwrap();

    let rendered = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .buffer_string();
    assert_eq!(
        rendered,
        "(define-abbrev-table 'test-table\n  '(\n    (\"btw\" \"by the way\" 7)\n   ))\n\n"
    );
}

#[test]
fn test_abbrev_tables_do_not_share_symbol_cells() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    let table_a = builtin_make_abbrev_table(&mut eval, vec![]).unwrap();
    let table_b = builtin_make_abbrev_table(&mut eval, vec![]).unwrap();

    builtin_define_abbrev(
        &mut eval,
        vec![table_a, Value::string("dup"), Value::string("first table")],
    )
    .unwrap();
    builtin_define_abbrev(
        &mut eval,
        vec![table_b, Value::string("dup"), Value::string("second table")],
    )
    .unwrap();

    let a = builtin_abbrev_expansion(&mut eval, vec![Value::string("dup"), table_a]).unwrap();
    let b = builtin_abbrev_expansion(&mut eval, vec![Value::string("dup"), table_b]).unwrap();
    assert_eq!(a.as_utf8_str(), Some("first table"));
    assert_eq!(b.as_utf8_str(), Some("second table"));
}

#[test]
fn test_define_abbrev_preserves_raw_unibyte_expansion() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    let table = builtin_make_abbrev_table(&mut eval, vec![]).unwrap();
    let raw = Value::heap_string(LispString::from_unibyte(vec![0xFF]));

    builtin_define_abbrev(&mut eval, vec![table, Value::string("raw"), raw]).unwrap();

    let result = builtin_abbrev_expansion(&mut eval, vec![Value::string("raw"), table]).unwrap();
    let text = result.as_lisp_string().expect("abbrev expansion string");
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF]);
}

#[test]
fn test_abbrev_table_properties_are_table_local() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    let table_a = builtin_make_abbrev_table(
        &mut eval,
        vec![Value::list(vec![Value::keyword(":case-fixed"), Value::T])],
    )
    .unwrap();
    let table_b = builtin_make_abbrev_table(&mut eval, vec![]).unwrap();

    let a =
        builtin_abbrev_table_get(&mut eval, vec![table_a, Value::keyword(":case-fixed")]).unwrap();
    let b =
        builtin_abbrev_table_get(&mut eval, vec![table_b, Value::keyword(":case-fixed")]).unwrap();
    assert!(a.is_truthy());
    assert!(b.is_nil());
}

#[test]
fn test_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // expand-abbrev needs exactly 0 args
    let result = builtin_expand_abbrev(&mut eval, vec![Value::string("t")]);
    assert!(result.is_err());
    let result = builtin_expand_abbrev(&mut eval, vec![]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}
