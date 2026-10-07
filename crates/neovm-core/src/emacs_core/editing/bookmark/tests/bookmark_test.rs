use super::*;
use crate::buffer::LispCharPos1;
use crate::heap_types::LispString;

// -----------------------------------------------------------------------
// BookmarkManager unit tests
// -----------------------------------------------------------------------

fn bm_str(text: &str) -> LispString {
    runtime_string_to_bookmark_string(text)
}

fn bm_runtime(text: Option<&LispString>) -> Option<String> {
    text.map(bookmark_string_to_runtime)
}

#[test]
fn set_get_delete() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BookmarkManager::new();

    let bm = Bookmark {
        name: bm_str("test"),
        filename: Some(bm_str("/tmp/test.txt")),
        position: LispCharPos1::new(42),
        front_context: Some(bm_str("after")),
        rear_context: Some(bm_str("before")),
        annotation: None,
        handler: None,
    };

    mgr.set(bm_str("test"), bm);
    assert!(mgr.get(&bm_str("test")).is_some());
    assert_eq!(
        mgr.get(&bm_str("test")).unwrap().position,
        LispCharPos1::new(42)
    );
    assert_eq!(
        bm_runtime(mgr.get(&bm_str("test")).unwrap().filename.as_ref()).as_deref(),
        Some("/tmp/test.txt"),
    );

    assert!(mgr.delete(&bm_str("test")));
    assert!(mgr.get(&bm_str("test")).is_none());
    assert!(!mgr.delete(&bm_str("test"))); // already gone
}

#[test]
fn rename() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BookmarkManager::new();

    let bm = Bookmark {
        name: bm_str("old"),
        filename: None,
        position: LispCharPos1::new(10),
        front_context: None,
        rear_context: None,
        annotation: None,
        handler: None,
    };
    mgr.set(bm_str("old"), bm);

    assert!(mgr.rename(&bm_str("old"), bm_str("new")));
    assert!(mgr.get(&bm_str("old")).is_none());
    assert!(mgr.get(&bm_str("new")).is_some());
    assert_eq!(
        mgr.get(&bm_str("new")).unwrap().position,
        LispCharPos1::new(10)
    );
}

#[test]
fn rename_nonexistent() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BookmarkManager::new();
    assert!(!mgr.rename(&bm_str("nope"), bm_str("whatever")));
}

#[test]
fn rename_collision() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BookmarkManager::new();
    let bm1 = Bookmark {
        name: bm_str("a"),
        filename: None,
        position: LispCharPos1::new(1),
        front_context: None,
        rear_context: None,
        annotation: None,
        handler: None,
    };
    let bm2 = Bookmark {
        name: bm_str("b"),
        filename: None,
        position: LispCharPos1::new(2),
        front_context: None,
        rear_context: None,
        annotation: None,
        handler: None,
    };
    mgr.set(bm_str("a"), bm1);
    mgr.set(bm_str("b"), bm2);

    // Cannot rename a -> b when b already exists
    assert!(!mgr.rename(&bm_str("a"), bm_str("b")));

    // Renaming to self is fine
    assert!(mgr.rename(&bm_str("a"), bm_str("a")));
}

#[test]
fn all_names_sorted() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BookmarkManager::new();

    for name in &["zebra", "alpha", "middle"] {
        let bm = Bookmark {
            name: bm_str(name),
            filename: None,
            position: LispCharPos1::new(1),
            front_context: None,
            rear_context: None,
            annotation: None,
            handler: None,
        };
        mgr.set(bm_str(name), bm);
    }

    let names = mgr.all_names();
    assert_eq!(
        names,
        vec![bm_str("alpha"), bm_str("middle"), bm_str("zebra")]
    );
}

#[test]
fn most_recent_tracking() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BookmarkManager::new();

    for name in &["first", "second", "third"] {
        let bm = Bookmark {
            name: bm_str(name),
            filename: None,
            position: LispCharPos1::new(1),
            front_context: None,
            rear_context: None,
            annotation: None,
            handler: None,
        };
        mgr.set(bm_str(name), bm);
    }

    // Most recent should be "third"
    assert_eq!(mgr.recent_names()[0], bm_str("third"));
    assert_eq!(mgr.recent_names()[1], bm_str("second"));
    assert_eq!(mgr.recent_names()[2], bm_str("first"));

    // Re-set "first" -> moves to front
    let bm = Bookmark {
        name: bm_str("first"),
        filename: None,
        position: LispCharPos1::new(99),
        front_context: None,
        rear_context: None,
        annotation: None,
        handler: None,
    };
    mgr.set(bm_str("first"), bm);
    assert_eq!(mgr.recent_names()[0], bm_str("first"));
}

#[test]
fn serialize_deserialize() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BookmarkManager::new();

    let bm1 = Bookmark {
        name: bm_str("alpha"),
        filename: Some(bm_str("/home/test/file.el")),
        position: LispCharPos1::new(100),
        front_context: Some(bm_str("(defun")),
        rear_context: Some(bm_str(";;")),
        annotation: Some(bm_str("Important function")),
        handler: None,
    };
    let bm2 = Bookmark {
        name: bm_str("beta"),
        filename: None,
        position: LispCharPos1::new(1),
        front_context: None,
        rear_context: None,
        annotation: None,
        handler: Some(bm_str("my-handler")),
    };
    mgr.set(bm_str("alpha"), bm1);
    mgr.set(bm_str("beta"), bm2);

    let data = mgr.save_to_string();
    assert!(!data.is_empty());

    // Load into a fresh manager
    let mut mgr2 = BookmarkManager::new();
    mgr2.load_from_string(&data);

    let names = mgr2.all_names();
    assert_eq!(names, vec![bm_str("alpha"), bm_str("beta")]);

    let a = mgr2.get(&bm_str("alpha")).unwrap();
    assert_eq!(a.position, LispCharPos1::new(100));
    assert_eq!(
        bm_runtime(a.filename.as_ref()).as_deref(),
        Some("/home/test/file.el")
    );
    assert_eq!(
        bm_runtime(a.front_context.as_ref()).as_deref(),
        Some("(defun")
    );
    assert_eq!(bm_runtime(a.rear_context.as_ref()).as_deref(), Some(";;"));
    assert_eq!(
        bm_runtime(a.annotation.as_ref()).as_deref(),
        Some("Important function")
    );
    assert!(a.handler.is_none());

    let b = mgr2.get(&bm_str("beta")).unwrap();
    assert_eq!(b.position, LispCharPos1::new(1));
    assert!(b.filename.is_none());
    assert_eq!(
        bm_runtime(b.handler.as_ref()).as_deref(),
        Some("my-handler")
    );
}

#[test]
fn load_empty_string() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BookmarkManager::new();
    let bm = Bookmark {
        name: bm_str("test"),
        filename: None,
        position: LispCharPos1::new(1),
        front_context: None,
        rear_context: None,
        annotation: None,
        handler: None,
    };
    mgr.set(bm_str("test"), bm);

    mgr.load_from_string("");
    assert!(mgr.all_names().is_empty());
}

#[test]
fn modified_flag() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BookmarkManager::new();
    assert!(!mgr.is_modified());

    let bm = Bookmark {
        name: bm_str("test"),
        filename: None,
        position: LispCharPos1::new(1),
        front_context: None,
        rear_context: None,
        annotation: None,
        handler: None,
    };
    mgr.set(bm_str("test"), bm);
    assert!(mgr.is_modified());

    mgr.mark_saved();
    assert!(!mgr.is_modified());

    mgr.delete(&bm_str("test"));
    assert!(mgr.is_modified());
}

// -----------------------------------------------------------------------
// Builtin-level tests
// -----------------------------------------------------------------------

fn set_current_buffer_file(eval: &mut super::super::eval::Context, path: &str) {
    if let Some(buffer) = eval.buffers.current_buffer_mut() {
        buffer.set_file_name_value(Value::string(path));
    }
}

#[test]
fn test_builtin_bookmark_set_and_jump() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    set_current_buffer_file(&mut eval, "/tmp/test.el");

    // bookmark-set
    let result = builtin_bookmark_set(&mut eval, vec![Value::string("my-bookmark")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());

    // bookmark-jump returns alist
    let result = builtin_bookmark_jump(&mut eval, vec![Value::string("my-bookmark")]);
    assert!(result.is_ok());
    let alist = result.unwrap();
    assert!(alist.is_list());

    // bookmark-jump on nonexistent -> error
    let result = builtin_bookmark_jump(&mut eval, vec![Value::string("nope")]);
    assert!(result.is_err());
}

#[test]
fn test_builtin_bookmark_jump_permissive_designators() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // nil designator is a dedicated error in GNU Emacs.
    let nil_result = builtin_bookmark_jump(&mut eval, vec![Value::NIL]);
    assert!(nil_result.is_err());

    // Non-string designators are tolerated and return nil.
    let int_result = builtin_bookmark_jump(&mut eval, vec![Value::fixnum(1)]);
    assert!(int_result.unwrap().is_nil());

    let list_result =
        builtin_bookmark_jump(&mut eval, vec![Value::list(vec![Value::symbol("foo")])]);
    assert!(list_result.unwrap().is_nil());

    // Optional second argument is accepted.
    let missing_with_flag =
        builtin_bookmark_jump(&mut eval, vec![Value::string("missing"), Value::T]);
    assert!(missing_with_flag.is_err());
}

#[test]
fn test_builtin_bookmark_delete() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    set_current_buffer_file(&mut eval, "/tmp/delete.el");

    // Set a bookmark
    builtin_bookmark_set(&mut eval, vec![Value::string("del-me")]).unwrap();

    // Delete it (returns nil) and verify side effect.
    let result = builtin_bookmark_delete(&mut eval, vec![Value::string("del-me")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
    assert!(eval.bookmarks.get(&bm_str("del-me")).is_none());

    // Delete again -> nil (not found).
    let result = builtin_bookmark_delete(&mut eval, vec![Value::string("del-me")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());

    // Non-string payloads are accepted and return nil.
    let result = builtin_bookmark_delete(&mut eval, vec![Value::fixnum(1)]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());

    // Optional second argument is accepted.
    let result = builtin_bookmark_delete(&mut eval, vec![Value::fixnum(1), Value::T]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn test_builtin_bookmark_delete_accepts_raw_unibyte_name() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    set_current_buffer_file(&mut eval, "/tmp/raw-bookmark.el");
    let raw_name = Value::heap_string(LispString::from_unibyte(vec![0xFF]));

    builtin_bookmark_set(&mut eval, vec![raw_name]).expect("set raw bookmark");
    assert_eq!(eval.bookmarks.all_names().len(), 1);

    builtin_bookmark_delete(&mut eval, vec![raw_name]).expect("delete raw bookmark");
    assert!(eval.bookmarks.all_names().is_empty());
}

#[test]
fn test_builtin_bookmark_rename() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    set_current_buffer_file(&mut eval, "/tmp/rename.el");

    builtin_bookmark_set(&mut eval, vec![Value::string("old-name")]).unwrap();

    // Rename
    let result = builtin_bookmark_rename(
        &mut eval,
        vec![Value::string("old-name"), Value::string("new-name")],
    );
    assert!(result.is_ok());

    // Old name gone, new name exists.
    assert!(eval.bookmarks.get(&bm_str("old-name")).is_none());
    assert!(eval.bookmarks.get(&bm_str("new-name")).is_some());
}

#[test]
fn test_builtin_bookmark_rename_permissive_designators() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    set_current_buffer_file(&mut eval, "/tmp/rename-permissive.el");
    builtin_bookmark_set(&mut eval, vec![Value::string("old-name")]).unwrap();

    // One-arg calls fall back to prompt behavior in batch mode and error.
    let one_arg = builtin_bookmark_rename(&mut eval, vec![Value::string("old-name")]);
    assert!(one_arg.is_err());

    // Non-cons old payloads signal wrong-type in this compatibility path.
    let ints = builtin_bookmark_rename(&mut eval, vec![Value::fixnum(1), Value::fixnum(2)]);
    assert!(ints.is_err());

    // Cons/list old payloads with non-string NEW are tolerated and return nil.
    let list_ok = builtin_bookmark_rename(
        &mut eval,
        vec![
            Value::list(vec![Value::symbol("a")]),
            Value::list(vec![Value::symbol("b")]),
        ],
    );
    assert!(list_ok.unwrap().is_nil());

    // Cons/list old payloads with string NEW error on invalid bookmark designator.
    let list_str = builtin_bookmark_rename(
        &mut eval,
        vec![
            Value::list(vec![Value::symbol("a")]),
            Value::string("new-name"),
        ],
    );
    assert!(list_str.is_err());

    // String path still renames when the source bookmark exists.
    let rename_ok = builtin_bookmark_rename(
        &mut eval,
        vec![Value::string("old-name"), Value::string("new-name")],
    );
    assert!(rename_ok.is_ok());
    assert!(eval.bookmarks.get(&bm_str("old-name")).is_none());
    assert!(eval.bookmarks.get(&bm_str("new-name")).is_some());
}

#[test]
fn test_builtin_bookmark_all_names() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    set_current_buffer_file(&mut eval, "/tmp/all-names.el");
    builtin_bookmark_set(&mut eval, vec![Value::string("z-bookmark")]).unwrap();
    builtin_bookmark_set(&mut eval, vec![Value::string("a-bookmark")]).unwrap();

    let result = builtin_bookmark_all_names(&mut eval, vec![]).unwrap();
    let names = super::super::value::list_to_vec(&result).unwrap();
    assert_eq!(names.len(), 2);
    assert_eq!(names[0].as_utf8_str(), Some("a-bookmark"));
    assert_eq!(names[1].as_utf8_str(), Some("z-bookmark"));
}

#[test]
fn test_builtin_bookmark_get_filename() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    set_current_buffer_file(&mut eval, "/tmp/file.el");
    builtin_bookmark_set(&mut eval, vec![Value::string("with-file")]).unwrap();

    let found = builtin_bookmark_get_filename(&mut eval, vec![Value::string("with-file")]).unwrap();
    assert_eq!(found.as_utf8_str(), Some("/tmp/file.el"));

    let missing = builtin_bookmark_get_filename(&mut eval, vec![Value::string("missing")]).unwrap();
    assert!(missing.is_nil());
}

#[test]
fn test_builtin_bookmark_get_position() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    set_current_buffer_file(&mut eval, "/tmp/position.el");
    builtin_bookmark_set(&mut eval, vec![Value::string("at-point")]).unwrap();

    let found = builtin_bookmark_get_position(&mut eval, vec![Value::string("at-point")]).unwrap();
    assert_eq!(found.as_int(), Some(1));

    let missing = builtin_bookmark_get_position(&mut eval, vec![Value::string("missing")]).unwrap();
    assert!(missing.is_nil());
}

#[test]
fn test_builtin_bookmark_set_stores_lisp_char_position() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    set_current_buffer_file(&mut eval, "/tmp/multibyte-position.el");
    {
        let buffer = eval.buffers.current_buffer_mut().expect("current buffer");
        buffer.insert("\u{20AC}x");
        buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new("\u{20AC}".len()));
        assert_eq!(buffer.point_lisp_char_pos().as_i64(), 2);
    }

    builtin_bookmark_set(&mut eval, vec![Value::string("multibyte")]).unwrap();

    let found = builtin_bookmark_get_position(&mut eval, vec![Value::string("multibyte")])
        .expect("bookmark-get-position");
    assert_eq!(found.as_int(), Some(2));
}

#[test]
fn test_builtin_bookmark_get_annotation() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    set_current_buffer_file(&mut eval, "/tmp/annotation.el");
    builtin_bookmark_set(&mut eval, vec![Value::string("with-note")]).unwrap();
    builtin_bookmark_set_annotation(
        &mut eval,
        vec![Value::string("with-note"), Value::string("note")],
    )
    .unwrap();

    let found =
        builtin_bookmark_get_annotation(&mut eval, vec![Value::string("with-note")]).unwrap();
    assert_eq!(found.as_utf8_str(), Some("note"));

    let missing =
        builtin_bookmark_get_annotation(&mut eval, vec![Value::string("missing")]).unwrap();
    assert!(missing.is_nil());
}

#[test]
fn bookmark_record_key_domain_matches_gnu_bookmark_props() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    for (key, name) in [
        (BookmarkRecordKey::Filename, "filename"),
        (BookmarkRecordKey::Position, "position"),
        (BookmarkRecordKey::Annotation, "annotation"),
    ] {
        let value = Value::symbol(name);
        assert_eq!(key.name(), name);
        assert_eq!(BookmarkRecordKey::from_lisp_value(&value), Some(key));
    }

    let record = Value::list(vec![
        Value::cons(Value::symbol("filename"), Value::string("/tmp/record.el")),
        Value::cons(Value::symbol("position"), Value::fixnum(42)),
        Value::cons(Value::symbol("annotation"), Value::string("note")),
    ]);
    let mut eval = Context::new();
    let filename = builtin_bookmark_get_filename(&mut eval, vec![record]).unwrap();
    let position = builtin_bookmark_get_position(&mut eval, vec![record]).unwrap();
    let annotation = builtin_bookmark_get_annotation(&mut eval, vec![record]).unwrap();

    assert_eq!(filename.as_utf8_str(), Some("/tmp/record.el"));
    assert_eq!(position.as_int(), Some(42));
    assert_eq!(annotation.as_utf8_str(), Some("note"));
}

#[test]
fn test_builtin_bookmark_set_annotation() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    set_current_buffer_file(&mut eval, "/tmp/set-annotation.el");
    builtin_bookmark_set(&mut eval, vec![Value::string("entry")]).unwrap();

    let set_result = builtin_bookmark_set_annotation(
        &mut eval,
        vec![Value::string("entry"), Value::string("note")],
    )
    .unwrap();
    assert_eq!(set_result.as_utf8_str(), Some("note"));

    let got = builtin_bookmark_get_annotation(&mut eval, vec![Value::string("entry")]).unwrap();
    assert_eq!(got.as_utf8_str(), Some("note"));

    let missing = builtin_bookmark_set_annotation(
        &mut eval,
        vec![Value::string("missing"), Value::string("note")],
    )
    .unwrap();
    assert!(missing.is_nil());
}

#[cfg(unix)]
#[test]
fn bookmark_save_load_preserves_raw_unibyte_file_path() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;
    use std::os::unix::ffi::OsStrExt;

    let mut eval = Context::new();
    // A bookmark file path with a raw 0xFF byte (not valid UTF-8). The path was
    // previously round-tripped through a storage String, so the byte became an
    // in-Unicode sentinel and the file was written to the wrong path.
    let raw_path_bytes = b"/tmp/neovm-bm-raw-\xFF.data".to_vec();
    let os_path = std::path::PathBuf::from(std::ffi::OsStr::from_bytes(&raw_path_bytes));
    let _ = std::fs::remove_file(&os_path);
    let path_val = Value::heap_string(crate::heap_types::LispString::from_unibyte(
        raw_path_bytes.clone(),
    ));

    set_current_buffer_file(&mut eval, "/raw-file.el");
    builtin_bookmark_set(&mut eval, vec![Value::string("raw-bm")]).unwrap();

    builtin_bookmark_save(&mut eval, vec![Value::NIL, path_val]).expect("save to raw path");
    assert!(
        os_path.exists(),
        "bookmark file was not written to the raw-byte path {os_path:?}"
    );

    eval.bookmarks = BookmarkManager::new();
    builtin_bookmark_load(&mut eval, vec![path_val]).expect("load from raw path");
    assert!(eval.bookmarks.get(&bm_str("raw-bm")).is_some());

    let _ = std::fs::remove_file(&os_path);
}

#[test]
fn test_builtin_bookmark_save_load() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    let save_file = "/tmp/neovm-bookmark-save-load.data";

    set_current_buffer_file(&mut eval, "/file1.el");
    builtin_bookmark_set(&mut eval, vec![Value::string("bm1")]).unwrap();
    set_current_buffer_file(&mut eval, "/file2.el");
    builtin_bookmark_set(&mut eval, vec![Value::string("bm2")]).unwrap();

    // Save to an explicit file path.
    let result = builtin_bookmark_save(
        &mut eval,
        vec![Value::NIL, Value::string(save_file.to_string())],
    );
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());

    // Clear and load
    eval.bookmarks = BookmarkManager::new();
    let result = builtin_bookmark_load(&mut eval, vec![Value::string(save_file.to_string())]);
    assert!(result.is_ok());
    let load_message = result.unwrap();
    assert_eq!(
        load_message.as_utf8_str(),
        Some("Loading bookmarks from /tmp/neovm-bookmark-save-load.data...done")
    );

    // Verify restored bookmark payloads.
    let bm1 = eval.bookmarks.get(&bm_str("bm1")).expect("bm1 restored");
    assert_eq!(
        bm_runtime(bm1.filename.as_ref()).as_deref(),
        Some("/file1.el")
    );

    let bm2 = eval.bookmarks.get(&bm_str("bm2")).expect("bm2 restored");
    assert_eq!(
        bm_runtime(bm2.filename.as_ref()).as_deref(),
        Some("/file2.el")
    );

    // NO-MSG suppresses the loading message.
    let result = builtin_bookmark_load(
        &mut eval,
        vec![Value::string(save_file.to_string()), Value::NIL, Value::T],
    );
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn test_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // bookmark-set needs between 1 and 2 args.
    let result = builtin_bookmark_set(&mut eval, vec![]);
    assert!(result.is_err());
    let result = builtin_bookmark_set(
        &mut eval,
        vec![Value::string("name"), Value::NIL, Value::NIL],
    );
    assert!(result.is_err());

    // bookmark-jump requires at least one argument.
    let result = builtin_bookmark_jump(&mut eval, vec![]);
    assert!(result.is_err());

    // bookmark-delete requires at least one argument.
    let result = builtin_bookmark_delete(&mut eval, vec![]);
    assert!(result.is_err());

    // bookmark-rename with one arg errors in batch mode.
    let result = builtin_bookmark_rename(&mut eval, vec![Value::string("x")]);
    assert!(result.is_err());
}
