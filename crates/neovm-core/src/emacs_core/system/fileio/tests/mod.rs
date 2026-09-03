use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::format_eval_result;
use crate::emacs_core::value::list_to_vec;
use crate::test_utils::runtime_startup_eval_all;
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};

#[cfg(target_os = "windows")]
mod windows_test;

fn bootstrap_eval(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

thread_local! {
    /// Keep ALL test contexts alive across a single #[test] so that
    /// heap-backed return values from earlier `call_fileio_builtin!`
    /// invocations remain valid when later assertions inspect them.
    /// Previously this stored only the *last* context, which freed
    /// earlier strings and produced use-after-free panics in tests
    /// that compared results across multiple builtin calls.
    static LAST_TEST_CTX: std::cell::RefCell<Vec<Context>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

macro_rules! call_fileio_builtin {
    ($builtin:ident, $args:expr) => {{
        let mut eval = Context::new();
        let result = $builtin(&mut eval, $args);
        LAST_TEST_CTX.with(|slot| slot.borrow_mut().push(eval));
        result
    }};
}

#[cfg(unix)]
fn assert_same_file_paths(path1: &str, path2: &str) {
    use std::os::unix::fs::MetadataExt;

    let meta1 = fs::metadata(path1).expect("metadata path1");
    let meta2 = fs::metadata(path2).expect("metadata path2");
    assert_eq!(meta1.dev(), meta2.dev());
    assert_eq!(meta1.ino(), meta2.ino());
}

#[cfg(not(unix))]
fn assert_same_file_paths(path1: &str, path2: &str) {
    assert_eq!(
        fs::read(path1).expect("read path1"),
        fs::read(path2).expect("read path2")
    );
}

#[cfg(unix)]
fn assert_same_file_path_bufs(path1: &std::path::Path, path2: &std::path::Path) {
    use std::os::unix::fs::MetadataExt;

    let meta1 = fs::metadata(path1).expect("metadata path1");
    let meta2 = fs::metadata(path2).expect("metadata path2");
    assert_eq!(meta1.dev(), meta2.dev());
    assert_eq!(meta1.ino(), meta2.ino());
}

fn assert_unibyte_string_bytes(value: Value, expected: &[u8]) {
    let string = value
        .as_lisp_string()
        .expect("expected string result for raw-byte assertion");
    assert!(!string.is_multibyte(), "expected unibyte string");
    assert_eq!(string.as_bytes(), expected);
}

#[cfg(unix)]
fn raw_temp_path(component: &[u8]) -> std::path::PathBuf {
    let mut bytes = std::env::temp_dir().as_os_str().as_bytes().to_vec();
    if bytes.last() != Some(&b'/') {
        bytes.push(b'/');
    }
    bytes.extend_from_slice(component);
    std::path::PathBuf::from(std::ffi::OsString::from_vec(bytes))
}

#[cfg(unix)]
fn raw_path_value(path: &std::path::Path) -> Value {
    Value::heap_string(crate::heap_types::LispString::from_unibyte(
        path.as_os_str().as_bytes().to_vec(),
    ))
}

#[test]
fn temporary_file_directory_for_eval_accepts_raw_unibyte_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(
        b"/tmp/neomacs-\xFF".to_vec(),
    ));
    eval.obarray
        .set_symbol_value("temporary-file-directory", raw);

    assert_eq!(
        temporary_file_directory_for_eval(&eval),
        Some(crate::heap_types::LispString::from_unibyte(
            b"/tmp/neomacs-\xFF".to_vec()
        ))
    );
}

#[test]
fn find_file_name_handler_matches_raw_unibyte_filename_bytes() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let handler = Value::symbol("vm-raw-file-handler");
    eval.obarray.set_symbol_value(
        "file-name-handler-alist",
        Value::list(vec![Value::cons(Value::string("\\`/fake:"), handler)]),
    );

    let raw_filename = crate::heap_types::LispString::from_unibyte(b"/fake:\xFF".to_vec());
    assert_eq!(
        find_file_name_handler_lisp_for_eval(&eval, &raw_filename, Value::symbol("file-exists-p")),
        handler
    );
}

#[test]
fn do_auto_save_names_a_fileless_buffer_under_a_raw_unibyte_prefix_directory() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(
        b"/tmp/neomacs-\xFF/".to_vec(),
    ));
    eval.obarray
        .set_symbol_value("auto-save-list-file-prefix", raw);

    let buffer_name = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .name_runtime_string_owned();
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert_lisp_string(&crate::heap_types::LispString::from_utf8("payload"));
    let safe_name = buffer_name.replace('/', "!");
    let mut expected = b"/tmp/neomacs-\xFF/#*".to_vec();
    expected.extend_from_slice(safe_name.as_bytes());
    expected.extend_from_slice(b"*#");

    builtin_do_auto_save(&mut eval, vec![]).expect("do-auto-save should name the buffer");
    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_unibyte_string_bytes(buf.auto_save_file_name_value(), &expected);
}

#[cfg(unix)]
#[test]
fn do_auto_save_preserves_a_raw_unibyte_visited_filename_in_the_auto_save_name() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(
        b"/tmp/neomacs-\xFF/demo-\xFE".to_vec(),
    ));
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.set_file_name_value(raw);
        buf.insert_lisp_string(&crate::heap_types::LispString::from_utf8("payload"));
    }

    builtin_do_auto_save(&mut eval, vec![])
        .expect("do-auto-save should preserve raw visited file names");

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_unibyte_string_bytes(
        buf.auto_save_file_name_value(),
        b"/tmp/neomacs-\xFF/#demo-\xFE#",
    );
}

// -----------------------------------------------------------------------
// Path operations
// -----------------------------------------------------------------------

#[test]
fn test_expand_file_name_absolute() {
    crate::test_utils::init_test_tracing();
    let result = expand_file_name("/usr/bin/ls", None);
    assert_eq!(result, "/usr/bin/ls");
}

#[cfg(windows)]
#[test]
fn test_expand_file_name_accepts_gnu_windows_absolute_paths() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        expand_file_name("emacs-lisp", Some(r"D:\a\neomacs\neomacs\lisp")),
        "D:/a/neomacs/neomacs/lisp/emacs-lisp"
    );
    assert_eq!(
        expand_file_name(r"D:\a\neomacs\neomacs\lisp", None),
        "D:/a/neomacs/neomacs/lisp"
    );
    assert!(file_name_absolute_p("D:/a/neomacs"));
    assert!(file_name_absolute_p(r"D:\a\neomacs"));
}

#[cfg(windows)]
#[test]
fn builtin_expand_file_name_treats_drive_paths_as_absolute_on_windows() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let value = builtin_expand_file_name(
        &mut eval,
        vec![
            Value::string("emacs-lisp"),
            Value::string(r"D:\a\neomacs\neomacs\lisp"),
        ],
    )
    .expect("expand-file-name should accept drive absolute default directory");
    let result = value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        .expect("string result");
    assert_eq!(result, "D:/a/neomacs/neomacs/lisp/emacs-lisp");
}

#[cfg(windows)]
#[test]
fn host_paths_are_exposed_to_lisp_with_gnu_windows_separators() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        host_path_to_lisp_file_name_string(std::path::Path::new(r"D:\a\neomacs\neomacs\lisp")),
        "D:/a/neomacs/neomacs/lisp"
    );
}

#[test]
fn test_expand_file_name_relative() {
    crate::test_utils::init_test_tracing();
    let result = expand_file_name("foo.txt", Some("/home/user"));
    assert_eq!(result, "/home/user/foo.txt");
}

#[test]
fn test_expand_file_name_tilde() {
    crate::test_utils::init_test_tracing();
    if std::env::var("HOME").is_ok() {
        let result = expand_file_name("~/test.txt", None);
        assert!(result.ends_with("/test.txt"));
        assert!(!result.starts_with("~"));
    }
}

#[cfg(unix)]
#[test]
fn test_expand_file_name_tilde_user() {
    crate::test_utils::init_test_tracing();
    assert_eq!(expand_file_name("~root/", Some("/tmp")), "/root/");
    assert_eq!(
        expand_file_name("~neomacs-definitely-missing-user/file", Some("/tmp")),
        "/tmp/~neomacs-definitely-missing-user/file"
    );
}

#[test]
fn test_expand_file_name_dotdot() {
    crate::test_utils::init_test_tracing();
    let result = expand_file_name("../bar.txt", Some("/home/user/dir"));
    assert_eq!(result, "/home/user/bar.txt");
}

#[test]
fn test_expand_file_name_dot() {
    crate::test_utils::init_test_tracing();
    let result = expand_file_name("./foo.txt", Some("/home/user"));
    assert_eq!(result, "/home/user/foo.txt");
}

#[test]
fn test_expand_file_name_preserves_directory_marker() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        expand_file_name("fixtures/", Some("/tmp")),
        "/tmp/fixtures/"
    );
    assert_eq!(expand_file_name("", Some("/tmp")), "/tmp");
}

#[test]
fn test_expand_file_name_preserves_gnu_superroot_spellings() {
    crate::test_utils::init_test_tracing();
    assert_eq!(expand_file_name("//server/share/../x", None), "//server/x");
    assert_eq!(expand_file_name("///server/share/../x", None), "/server/x");
    assert_eq!(expand_file_name("/../x", None), "/../x");
    assert_eq!(expand_file_name("/../../x", None), "/x");
}

#[test]
fn test_file_truename_missing_file_and_trailing_slash() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        file_truename("/tmp/neovm-file-truename-missing", None),
        "/tmp/neovm-file-truename-missing"
    );
    assert_eq!(file_truename("/tmp/../tmp/", None), "/tmp/");
}

#[test]
fn test_file_truename_resolves_relative_default_directory() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm-file-truename-rel");
    let _ = fs::create_dir_all(&dir);
    let file = dir.join("alpha.txt");
    fs::write(&file, b"alpha").unwrap();

    let resolved = file_truename("alpha.txt", Some(&dir.to_string_lossy()));
    assert_eq!(resolved, file.to_string_lossy());

    let _ = fs::remove_file(file);
    let _ = fs::remove_dir(dir);
}

#[cfg(unix)]
#[test]
fn builtin_file_truename_preserves_raw_unibyte_directory_bytes() {
    crate::test_utils::init_test_tracing();
    let dir = raw_temp_path(b"neovm-file-truename-\xFF");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("alpha.txt"), b"alpha").unwrap();

    let mut eval = Context::new();
    let mut default_dir_bytes = dir.as_os_str().as_bytes().to_vec();
    default_dir_bytes.push(b'/');
    eval.set_variable(
        "default-directory",
        Value::heap_string(crate::heap_types::LispString::from_unibyte(
            default_dir_bytes.clone(),
        )),
    );

    let value = builtin_file_truename(
        &mut eval,
        vec![Value::heap_string(
            crate::heap_types::LispString::from_unibyte(b"alpha.txt".to_vec()),
        )],
    )
    .expect("file-truename should preserve raw directory bytes");

    let mut expected = dir.as_os_str().as_bytes().to_vec();
    expected.extend_from_slice(b"/alpha.txt");
    assert_unibyte_string_bytes(value, &expected);

    let _ = fs::remove_file(dir.join("alpha.txt"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_file_name_directory() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        file_name_directory("/home/user/test.txt"),
        Some("/home/user/".to_string())
    );
    assert_eq!(file_name_directory("test.txt"), None);
    assert_eq!(
        file_name_directory("/home/user/dir/"),
        Some("/home/user/dir/".to_string())
    );
    #[cfg(windows)]
    {
        assert_eq!(
            file_name_directory(r"D:\a\neomacs\lisp\international\uni-titlecase.el"),
            Some(r"D:\a\neomacs\lisp\international\".to_string())
        );
    }
}

#[test]
fn test_file_name_nondirectory() {
    crate::test_utils::init_test_tracing();
    assert_eq!(file_name_nondirectory("/home/user/test.txt"), "test.txt");
    assert_eq!(file_name_nondirectory("test.txt"), "test.txt");
    assert_eq!(file_name_nondirectory("/home/user/"), "");
    #[cfg(windows)]
    {
        assert_eq!(
            file_name_nondirectory(r"D:\a\neomacs\lisp\international\uni-titlecase.el"),
            "uni-titlecase.el"
        );
        assert_eq!(file_name_nondirectory(r"D:\a\neomacs\"), "");
    }
}

#[test]
fn test_file_name_as_directory() {
    crate::test_utils::init_test_tracing();
    assert_eq!(file_name_as_directory("/tmp"), "/tmp/");
    assert_eq!(file_name_as_directory("/tmp/"), "/tmp/");
    assert_eq!(file_name_as_directory(""), "./");
    assert_eq!(file_name_as_directory("foo"), "foo/");
    assert_eq!(file_name_as_directory("foo/"), "foo/");
    assert_eq!(file_name_as_directory("~"), "~/");
    assert_eq!(file_name_as_directory("~/"), "~/");
}

#[test]
fn test_directory_file_name() {
    crate::test_utils::init_test_tracing();
    assert_eq!(directory_file_name("/tmp/"), "/tmp");
    assert_eq!(directory_file_name("/tmp"), "/tmp");
    assert_eq!(directory_file_name("/"), "/");
    assert_eq!(directory_file_name("//"), "//");
    assert_eq!(directory_file_name("///"), "/");
    assert_eq!(directory_file_name("foo/"), "foo");
    assert_eq!(directory_file_name("foo"), "foo");
    assert_eq!(directory_file_name("a//"), "a");
    assert_eq!(directory_file_name("~/"), "~");
    assert_eq!(directory_file_name("~"), "~");
    assert_eq!(directory_file_name(""), "");
}

#[test]
fn test_file_name_concat() {
    crate::test_utils::init_test_tracing();
    assert_eq!(file_name_concat(&["foo", "bar"]), "foo/bar");
    assert_eq!(file_name_concat(&["foo", "bar", "zot"]), "foo/bar/zot");
    assert_eq!(file_name_concat(&["foo/", "bar"]), "foo/bar");
    assert_eq!(file_name_concat(&["foo/", "bar/", "zot"]), "foo/bar/zot");
    assert_eq!(file_name_concat(&["foo", "/bar"]), "foo//bar");
    assert_eq!(file_name_concat(&["foo"]), "foo");
    assert_eq!(file_name_concat(&["foo/"]), "foo/");
    assert_eq!(file_name_concat(&["foo", "", "", ""]), "foo");
    assert_eq!(file_name_concat(&[""]), "");
    assert_eq!(file_name_concat(&["", "bar"]), "bar");
    assert_eq!(file_name_concat(&[]), "");
}

#[test]
fn test_file_name_absolute_p() {
    crate::test_utils::init_test_tracing();
    assert!(file_name_absolute_p("/tmp"));
    assert!(file_name_absolute_p("~/tmp"));
    assert!(file_name_absolute_p("~"));
    assert!(file_name_absolute_p("~root"));
    assert!(!file_name_absolute_p(
        "~neomacs-user-that-should-not-exist-2b6ad2e4"
    ));
    assert!(!file_name_absolute_p(
        "~neomacs-user-that-should-not-exist-2b6ad2e4/tmp"
    ));
    assert!(!file_name_absolute_p("tmp"));
    assert!(!file_name_absolute_p("./tmp"));
}

#[test]
fn test_directory_name_p() {
    crate::test_utils::init_test_tracing();
    assert!(directory_name_p("/tmp/"));
    assert!(directory_name_p("foo/"));
    assert!(!directory_name_p("/tmp"));
    assert!(!directory_name_p("foo"));
    assert!(!directory_name_p(""));
    #[cfg(windows)]
    {
        assert!(directory_name_p(r"D:\a\"));
        assert!(!directory_name_p(r"D:\a"));
    }
}

#[test]
fn test_substitute_in_file_name() {
    crate::test_utils::init_test_tracing();
    let home = std::env::var("HOME").unwrap_or_default();

    assert_eq!(substitute_in_file_name("$HOME/foo"), format!("{home}/foo"));
    assert_eq!(
        substitute_in_file_name("${HOME}/foo"),
        format!("{home}/foo")
    );
    assert_eq!(substitute_in_file_name("$UNDEF/foo"), "$UNDEF/foo");
    assert_eq!(substitute_in_file_name("$$HOME"), "$HOME");
    assert_eq!(substitute_in_file_name("${}"), "${}");
    assert_eq!(substitute_in_file_name("$"), "$");
    assert_eq!(substitute_in_file_name("${HOME"), "${HOME");
    assert_eq!(substitute_in_file_name("bar/~/foo"), "~/foo");
    assert_eq!(
        substitute_in_file_name("/usr/local/$HOME/foo"),
        format!("{home}/foo")
    );
    assert_eq!(substitute_in_file_name("a//b"), "/b");
    assert_eq!(substitute_in_file_name("a///b"), "/b");
}

// -----------------------------------------------------------------------
// File predicates
// -----------------------------------------------------------------------

#[test]
fn test_file_exists_p() {
    crate::test_utils::init_test_tracing();
    assert!(file_exists_p("/tmp"));
    assert!(!file_exists_p("/nonexistent_path_12345"));
}

#[test]
fn test_file_directory_p() {
    crate::test_utils::init_test_tracing();
    assert!(file_directory_p("/tmp"));
    assert!(!file_directory_p("/nonexistent_path_12345"));
}

#[cfg(windows)]
#[test]
fn file_readable_p_treats_existing_directories_as_readable_on_windows() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join(format!("neomacs-readable-dir-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    assert!(file_readable_p(&dir.to_string_lossy()));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_file_regular_p() {
    crate::test_utils::init_test_tracing();
    // /tmp is a directory, not a regular file
    assert!(!file_regular_p("/tmp"));
    assert!(!file_regular_p("/nonexistent_path_12345"));
}

#[test]
fn test_file_symlink_p() {
    crate::test_utils::init_test_tracing();
    // /tmp itself typically isn't a symlink
    assert!(!file_symlink_p("/nonexistent_path_12345"));
}

#[cfg(unix)]
#[test]
fn builtin_file_symlink_p_preserves_raw_unibyte_target_bytes() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join(format!("neovm-raw-symlink-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();

    let target_name = b"target-\xFF.txt";
    let target_path = base.join(std::ffi::OsStr::from_bytes(target_name));
    fs::write(&target_path, b"x").unwrap();

    let link_path = base.join("link.txt");
    std::os::unix::fs::symlink(std::ffi::OsStr::from_bytes(target_name), &link_path).unwrap();

    let value = call_fileio_builtin!(
        builtin_file_symlink_p,
        vec![Value::string(link_path.to_string_lossy().as_ref())]
    )
    .expect("file-symlink-p should preserve raw target bytes");
    assert_unibyte_string_bytes(value, target_name);

    let _ = fs::remove_file(&link_path);
    let _ = fs::remove_file(&target_path);
    let _ = fs::remove_dir_all(&base);
}

#[cfg(unix)]
#[test]
fn builtin_file_truename_chases_raw_unibyte_symlink_targets() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join(format!("neovm-raw-truename-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();

    let target_name = b"target-\xFF.txt";
    let target_path = base.join(std::ffi::OsStr::from_bytes(target_name));
    fs::write(&target_path, b"x").unwrap();
    let link_path = base.join("link.txt");
    std::os::unix::fs::symlink(std::ffi::OsStr::from_bytes(target_name), &link_path).unwrap();

    let mut eval = Context::new();
    eval.set_variable(
        "default-directory",
        Value::string(format!("{}/", base.to_string_lossy())),
    );

    let value = builtin_file_truename(&mut eval, vec![Value::string("link.txt")])
        .expect("file-truename should chase raw-byte symlink targets");

    let mut expected = base.as_os_str().as_bytes().to_vec();
    expected.push(b'/');
    expected.extend_from_slice(target_name);
    assert_unibyte_string_bytes(value, &expected);

    let _ = fs::remove_file(&link_path);
    let _ = fs::remove_file(&target_path);
    let _ = fs::remove_dir_all(&base);
}

// -----------------------------------------------------------------------
// File read/write
// -----------------------------------------------------------------------

#[test]
fn test_read_write_file() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_fileio_test");
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("test_rw.txt");
    let path_str = path.to_string_lossy().to_string();

    // Write
    write_string_to_file("hello, world\n", &path_str, false).unwrap();

    // Read back
    let contents = read_file_contents(&path_str).unwrap();
    assert_eq!(contents, "hello, world\n");

    // Append
    write_string_to_file("second line\n", &path_str, true).unwrap();
    let contents = read_file_contents(&path_str).unwrap();
    assert_eq!(contents, "hello, world\nsecond line\n");

    // Overwrite
    write_string_to_file("replaced\n", &path_str, false).unwrap();
    let contents = read_file_contents(&path_str).unwrap();
    assert_eq!(contents, "replaced\n");

    // Predicates on the file we just wrote
    assert!(file_exists_p(&path_str));
    assert!(file_regular_p(&path_str));
    assert!(file_readable_p(&path_str));
    assert!(!file_directory_p(&path_str));

    // Clean up
    delete_file(&path_str).unwrap();
    assert!(!file_exists_p(&path_str));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_file_error_symbol_mapping() {
    crate::test_utils::init_test_tracing();
    assert_eq!(file_error_symbol(ErrorKind::NotFound), "file-missing");
    assert_eq!(
        file_error_symbol(ErrorKind::AlreadyExists),
        "file-already-exists"
    );
    assert_eq!(
        file_error_symbol(ErrorKind::PermissionDenied),
        "permission-denied"
    );
    assert_eq!(file_error_symbol(ErrorKind::InvalidInput), "file-error");
}

#[test]
fn test_signal_file_io_error_uses_specific_condition() {
    crate::test_utils::init_test_tracing();
    let flow = signal_file_io_error(
        std::io::Error::from(ErrorKind::PermissionDenied),
        "Writing to /tmp/neovm-probe".to_string(),
    );
    match flow {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "permission-denied");
            assert_eq!(sig.data.len(), 1);
            let Some(message) = sig.data[0].as_utf8_str() else {
                panic!("expected string error payload");
            };
            assert!(message.contains("Writing to /tmp/neovm-probe"));
        }
        other => panic!("expected signal, got {:?}", other),
    }
}

#[test]
fn test_delete_file_compat_missing_is_noop() {
    crate::test_utils::init_test_tracing();
    let path = std::env::temp_dir().join("neovm_delete_missing_noop.tmp");
    let path_str = path.to_string_lossy().to_string();
    let _ = fs::remove_file(&path);
    assert!(delete_file_compat(&path_str).is_ok());
}

#[test]
fn test_builtin_delete_file_accepts_optional_trash_arg() {
    crate::test_utils::init_test_tracing();
    let path = std::env::temp_dir().join("neovm_delete_file_trash_arg.tmp");
    let path_str = path.to_string_lossy().to_string();
    let _ = fs::remove_file(&path);
    fs::write(&path, b"x").unwrap();

    let result = call_fileio_builtin!(
        builtin_delete_file,
        vec![Value::string(&path_str), Value::T]
    )
    .unwrap();
    assert_eq!(result, Value::NIL);
    assert!(!path.exists());

    let err = call_fileio_builtin!(
        builtin_delete_file,
        vec![Value::string(&path_str), Value::NIL, Value::NIL]
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("delete-file"), Value::fixnum(3)]
            );
        }
        other => panic!("expected signal, got {:?}", other),
    }
}

#[cfg(unix)]
#[test]
fn builtin_delete_file_internal_handles_raw_unibyte_paths() {
    crate::test_utils::init_test_tracing();
    let path = raw_temp_path(b"neovm-delete-file-\xFF");
    let _ = fs::remove_file(&path);
    fs::write(&path, b"x").unwrap();
    let value = Value::heap_string(crate::heap_types::LispString::from_unibyte(
        path.as_os_str().as_bytes().to_vec(),
    ));

    call_fileio_builtin!(builtin_delete_file_internal, vec![value])
        .expect("delete-file-internal should handle raw-byte paths");
    assert!(!path.exists());
}

#[test]
fn test_builtin_delete_directory_basic_and_recursive() {
    crate::test_utils::init_test_tracing();
    let root = std::env::temp_dir().join("neovm_delete_directory_test");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let root_str = root.to_string_lossy().to_string();

    // Non-recursive removal succeeds for empty directories.
    assert_eq!(
        call_fileio_builtin!(builtin_delete_directory, vec![Value::string(&root_str)]).unwrap(),
        Value::NIL
    );
    assert!(!root.exists());

    // Non-recursive removal fails for non-empty directories.
    fs::create_dir_all(&root).unwrap();
    let nested = root.join("child.txt");
    fs::write(&nested, b"x").unwrap();
    let err =
        call_fileio_builtin!(builtin_delete_directory, vec![Value::string(&root_str)]).unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "file-error");
        }
        other => panic!("expected signal, got {:?}", other),
    }

    // Recursive removal succeeds.
    assert_eq!(
        call_fileio_builtin!(
            builtin_delete_directory,
            vec![Value::string(&root_str), Value::T]
        )
        .unwrap(),
        Value::NIL
    );
    assert!(!root.exists());
}

#[test]
fn test_builtin_delete_directory_eval_resolves_default_directory() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join("neovm-delete-dir-eval");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    let mut eval = Context::new();
    eval.set_variable(
        "default-directory",
        Value::string(format!("{}/", base.to_string_lossy())),
    );

    let child = base.join("child");
    fs::create_dir_all(&child).unwrap();
    builtin_delete_directory(&mut eval, vec![Value::string("child")]).unwrap();
    assert!(!child.exists());

    let _ = fs::remove_dir_all(base);
}

#[cfg(unix)]
#[test]
fn builtin_delete_directory_internal_handles_raw_unibyte_paths() {
    crate::test_utils::init_test_tracing();
    let path = raw_temp_path(b"neovm-delete-dir-\xFF");
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    let value = Value::heap_string(crate::heap_types::LispString::from_unibyte(
        path.as_os_str().as_bytes().to_vec(),
    ));

    call_fileio_builtin!(builtin_delete_directory_internal, vec![value])
        .expect("delete-directory-internal should handle raw-byte paths");
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn builtin_make_directory_internal_handles_raw_unibyte_paths() {
    crate::test_utils::init_test_tracing();
    let path = raw_temp_path(b"neovm-mkdir-\xFF");
    let _ = fs::remove_dir_all(&path);
    let value = Value::heap_string(crate::heap_types::LispString::from_unibyte(
        path.as_os_str().as_bytes().to_vec(),
    ));

    call_fileio_builtin!(builtin_make_directory_internal, vec![value])
        .expect("make-directory-internal should handle raw-byte paths");
    assert!(path.exists());

    let _ = fs::remove_dir_all(&path);
}

#[cfg(unix)]
#[test]
fn test_builtin_make_symbolic_link_core_semantics() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join("neovm-symlink-test");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    let target = base.join("target.txt");
    let link = base.join("link.txt");
    fs::write(&target, b"x").unwrap();
    let target_str = target.to_string_lossy().to_string();
    let link_str = link.to_string_lossy().to_string();

    assert_eq!(
        call_fileio_builtin!(
            builtin_make_symbolic_link,
            vec![Value::string(&target_str), Value::string(&link_str)]
        )
        .unwrap(),
        Value::NIL
    );
    assert!(file_symlink_p(&link_str));

    let err = call_fileio_builtin!(
        builtin_make_symbolic_link,
        vec![Value::string(&target_str), Value::string(&link_str)]
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "file-already-exists"),
        other => panic!("expected signal, got {:?}", other),
    }

    assert_eq!(
        call_fileio_builtin!(
            builtin_make_symbolic_link,
            vec![
                Value::string(&target_str),
                Value::string(&link_str),
                Value::T,
            ]
        )
        .unwrap(),
        Value::NIL
    );

    delete_file(&link_str).unwrap();
    delete_file(&target_str).unwrap();
    let _ = fs::remove_dir_all(base);
}

#[cfg(unix)]
#[test]
fn test_builtin_make_symbolic_link_eval_uses_default_directory() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join("neovm-symlink-eval");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    let mut eval = Context::new();
    eval.set_variable(
        "default-directory",
        Value::string(format!("{}/", base.to_string_lossy())),
    );

    fs::write(base.join("target.txt"), b"x").unwrap();
    builtin_make_symbolic_link(
        &mut eval,
        vec![Value::string("target.txt"), Value::string("link.txt")],
    )
    .unwrap();
    assert!(file_symlink_p(&base.join("link.txt").to_string_lossy()));

    delete_file(&base.join("link.txt").to_string_lossy()).unwrap();
    delete_file(&base.join("target.txt").to_string_lossy()).unwrap();
    let _ = fs::remove_dir_all(base);
}

#[cfg(unix)]
#[test]
fn builtin_make_symbolic_link_keeps_relative_raw_target_bytes() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join("neovm-raw-relative-link");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();

    let target = base.join(std::ffi::OsStr::from_bytes(b"target-\xFF"));
    let link = base.join(std::ffi::OsStr::from_bytes(b"link-\xFE"));
    fs::write(&target, b"x").unwrap();

    let mut eval = Context::new();
    eval.set_variable(
        "default-directory",
        Value::string(format!("{}/", base.to_string_lossy())),
    );

    builtin_make_symbolic_link(
        &mut eval,
        vec![
            Value::heap_string(crate::heap_types::LispString::from_unibyte(
                b"target-\xFF".to_vec(),
            )),
            Value::heap_string(crate::heap_types::LispString::from_unibyte(
                b"link-\xFE".to_vec(),
            )),
        ],
    )
    .expect("make-symbolic-link should preserve target bytes");

    let read_target = fs::read_link(&link).expect("symlink target");
    assert_eq!(read_target.as_os_str().as_bytes(), b"target-\xFF");

    let _ = fs::remove_file(&link);
    let _ = fs::remove_file(&target);
    let _ = fs::remove_dir_all(base);
}

// -----------------------------------------------------------------------
// Directory operations
// -----------------------------------------------------------------------

#[test]
fn test_make_directory_and_directory_files() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join("neovm_dirtest");
    let _ = fs::remove_dir_all(&base);
    let base_str = base.to_string_lossy().to_string();

    // Create with parents
    let nested = base.join("a/b/c");
    let nested_str = nested.to_string_lossy().to_string();
    make_directory(&nested_str, true).unwrap();
    assert!(file_directory_p(&nested_str));

    // Create files in the base directory
    for name in &["foo.txt", "bar.txt", "baz.el"] {
        let p = base.join(name);
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(b"data").unwrap();
    }

    let base_ls = crate::heap_types::LispString::from_unibyte(base_str.as_bytes().to_vec());

    // List files
    let files = directory_files(&base_ls, false, None, false, None).unwrap();
    let names: Vec<&[u8]> = files.iter().map(|f| f.as_bytes()).collect();
    assert!(names.contains(&b".".as_slice()));
    assert!(names.contains(&b"..".as_slice()));
    assert!(names.contains(&b"foo.txt".as_slice()));
    assert!(names.contains(&b"bar.txt".as_slice()));
    assert!(names.contains(&b"baz.el".as_slice()));

    // List with filter
    let pattern = crate::heap_types::LispString::from_unibyte(b"\\.el$".to_vec());
    let filtered = directory_files(&base_ls, false, Some(&pattern), false, None).unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].as_bytes(), b"baz.el");

    // List with full paths
    let full = directory_files(&base_ls, true, None, false, None).unwrap();
    for entry in &full {
        assert!(entry.as_bytes().starts_with(base_str.as_bytes()));
    }

    // Clean up
    let _ = fs::remove_dir_all(&base);
}

#[cfg(unix)]
#[test]
fn directory_files_preserves_raw_unibyte_filename() {
    crate::test_utils::init_test_tracing();
    use std::os::unix::ffi::OsStrExt;

    let base = std::env::temp_dir().join(format!("neovm_dirtest_raw_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    // A file whose name carries a raw 0xFF byte (not valid UTF-8). GNU's
    // directory-files DECODE_FILEs each readdir name and keeps the byte; our
    // path_to_lisp_file_name keeps it as a raw unibyte byte. Either way it must
    // NOT be lossily mangled to U+FFFD.
    let raw_path = base.join(std::ffi::OsStr::from_bytes(b"raw-\xFF-file"));
    fs::File::create(&raw_path).unwrap();

    let base_ls = crate::heap_types::LispString::from_unibyte(base.as_os_str().as_bytes().to_vec());
    let files = directory_files(&base_ls, false, None, false, None).unwrap();

    assert!(
        files.iter().any(|f| f.as_bytes() == b"raw-\xFF-file"),
        "raw-byte filename not preserved: {:?}",
        files
            .iter()
            .map(|f| f.as_bytes().to_vec())
            .collect::<Vec<_>>()
    );
    assert!(
        !files
            .iter()
            .any(|f| f.as_bytes() == "raw-\u{FFFD}-file".as_bytes()),
        "raw-byte filename was lossily mangled to U+FFFD"
    );

    let _ = fs::remove_dir_all(&base);
}

// -----------------------------------------------------------------------
// File management: rename, copy
// -----------------------------------------------------------------------

#[test]
fn test_rename_and_copy_file() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_rename_copy_test");
    let _ = fs::create_dir_all(&dir);

    let src = dir.join("source.txt");
    let dst_rename = dir.join("renamed.txt");
    let dst_copy = dir.join("copied.txt");

    let src_str = src.to_string_lossy().to_string();
    let dst_rename_str = dst_rename.to_string_lossy().to_string();
    let dst_copy_str = dst_copy.to_string_lossy().to_string();

    // Create source
    write_string_to_file("original content", &src_str, false).unwrap();

    // Copy
    copy_file(&src_str, &dst_copy_str).unwrap();
    assert!(file_exists_p(&src_str));
    assert!(file_exists_p(&dst_copy_str));
    assert_eq!(
        read_file_contents(&dst_copy_str).unwrap(),
        "original content"
    );

    // Rename
    rename_file(&src_str, &dst_rename_str).unwrap();
    assert!(!file_exists_p(&src_str));
    assert!(file_exists_p(&dst_rename_str));
    assert_eq!(
        read_file_contents(&dst_rename_str).unwrap(),
        "original content"
    );

    // Clean up
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_builtin_rename_file_overwrite_semantics() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_builtin_rename_overwrite");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let src = dir.join("src.txt");
    let dst = dir.join("dst.txt");
    fs::write(&src, b"x").unwrap();
    fs::write(&dst, b"y").unwrap();
    let src_s = src.to_string_lossy().to_string();
    let dst_s = dst.to_string_lossy().to_string();

    let err = call_fileio_builtin!(
        builtin_rename_file,
        vec![Value::string(&src_s), Value::string(&dst_s)]
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "file-already-exists"),
        other => panic!("expected signal, got {:?}", other),
    }

    assert_eq!(
        call_fileio_builtin!(
            builtin_rename_file,
            vec![Value::string(&src_s), Value::string(&dst_s), Value::T]
        )
        .unwrap(),
        Value::NIL
    );
    assert!(!src.exists());
    assert!(dst.exists());

    let err = call_fileio_builtin!(
        builtin_rename_file,
        vec![
            Value::string("a"),
            Value::string("b"),
            Value::NIL,
            Value::NIL,
        ]
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments, got {:?}", other),
    }

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn rename_file_cross_device_regular_file_copies_then_deletes_source() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_rename_exdev_fallback");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let src = dir.join("src.txt");
    let dst = dir.join("dst.txt");
    fs::write(&src, b"payload").unwrap();

    let attempts = std::cell::Cell::new(0);
    rename_path_with_cross_device_fallback(&src, &dst, true, |from, to| {
        attempts.set(attempts.get() + 1);
        assert_eq!(from, src.as_path());
        assert_eq!(to, dst.as_path());
        Err(io::Error::from_raw_os_error(libc::EXDEV))
    })
    .unwrap();

    assert_eq!(attempts.get(), 1);
    assert!(!src.exists());
    assert_eq!(fs::read(&dst).unwrap(), b"payload");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_builtin_copy_file_optional_arg_semantics() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_builtin_copy_optional");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let src = dir.join("src.txt");
    let dst = dir.join("dst.txt");
    fs::write(&src, b"src").unwrap();
    fs::write(&dst, b"dst").unwrap();
    let src_s = src.to_string_lossy().to_string();
    let dst_s = dst.to_string_lossy().to_string();

    let err = call_fileio_builtin!(
        builtin_copy_file,
        vec![Value::string(&src_s), Value::string(&dst_s)]
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "file-already-exists"),
        other => panic!("expected signal, got {:?}", other),
    }

    assert_eq!(
        call_fileio_builtin!(
            builtin_copy_file,
            vec![Value::string(&src_s), Value::string(&dst_s), Value::T]
        )
        .unwrap(),
        Value::NIL
    );

    assert_eq!(
        call_fileio_builtin!(
            builtin_copy_file,
            vec![
                Value::string(&src_s),
                Value::string(&dst_s),
                Value::T,
                Value::T,
                Value::T,
                Value::T,
            ]
        )
        .unwrap(),
        Value::NIL
    );

    let err = call_fileio_builtin!(
        builtin_copy_file,
        vec![
            Value::string("a"),
            Value::string("b"),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ]
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments, got {:?}", other),
    }

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn builtin_copy_file_handles_raw_unibyte_paths() {
    crate::test_utils::init_test_tracing();
    let base = raw_temp_path(b"neovm-copy-\xFF");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();

    let src = base.join(std::ffi::OsStr::from_bytes(b"src-\xFE"));
    let dst = base.join(std::ffi::OsStr::from_bytes(b"dst-\xFD"));
    fs::write(&src, b"copy me").unwrap();

    builtin_copy_file(
        &mut Context::new(),
        vec![raw_path_value(&src), raw_path_value(&dst)],
    )
    .expect("copy-file should handle raw-byte paths");
    assert_eq!(fs::read(&dst).unwrap(), b"copy me");

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn builtin_copy_file_directory_target_uses_source_basename() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_copy_dir_target");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let src = dir.join("source.txt");
    let dst_dir = dir.join("dest");
    fs::create_dir_all(&dst_dir).unwrap();
    fs::write(&src, b"copied").unwrap();

    let dst_dir_arg = format!("{}/", dst_dir.to_string_lossy());
    builtin_copy_file(
        &mut Context::new(),
        vec![
            Value::string(src.to_string_lossy().as_ref()),
            Value::string(&dst_dir_arg),
        ],
    )
    .expect("copy-file should target basename within directory");

    assert_eq!(fs::read(dst_dir.join("source.txt")).unwrap(), b"copied");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_builtin_add_name_to_file_semantics() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_add_name_to_file_test");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let src = dir.join("source.txt");
    let dst = dir.join("alias.txt");
    fs::write(&src, b"x").unwrap();

    let src_str = src.to_string_lossy().to_string();
    let dst_str = dst.to_string_lossy().to_string();

    assert_eq!(
        call_fileio_builtin!(
            builtin_add_name_to_file,
            vec![Value::string(&src_str), Value::string(&dst_str)]
        )
        .unwrap(),
        Value::NIL
    );
    assert!(file_exists_p(&dst_str));
    assert_same_file_paths(&src_str, &dst_str);

    let err = call_fileio_builtin!(
        builtin_add_name_to_file,
        vec![Value::string(&src_str), Value::string(&dst_str)]
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "file-already-exists"),
        other => panic!("expected signal, got {:?}", other),
    }

    assert_eq!(
        call_fileio_builtin!(
            builtin_add_name_to_file,
            vec![Value::string(&src_str), Value::string(&dst_str), Value::T,]
        )
        .unwrap(),
        Value::NIL
    );
    assert_same_file_paths(&src_str, &dst_str);

    let missing = dir.join("missing.txt").to_string_lossy().to_string();
    let dst2 = dir.join("alias2.txt").to_string_lossy().to_string();
    let err = call_fileio_builtin!(
        builtin_add_name_to_file,
        vec![Value::string(&missing), Value::string(&dst2)]
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "file-missing"),
        other => panic!("expected signal, got {:?}", other),
    }

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn builtin_rename_file_handles_raw_unibyte_paths() {
    crate::test_utils::init_test_tracing();
    let base = raw_temp_path(b"neovm-rename-\xFF");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();

    let src = base.join(std::ffi::OsStr::from_bytes(b"src-\xFE"));
    let dst = base.join(std::ffi::OsStr::from_bytes(b"dst-\xFD"));
    fs::write(&src, b"rename me").unwrap();

    builtin_rename_file(
        &mut Context::new(),
        vec![raw_path_value(&src), raw_path_value(&dst)],
    )
    .expect("rename-file should handle raw-byte paths");
    assert!(!src.exists());
    assert_eq!(fs::read(&dst).unwrap(), b"rename me");

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn builtin_rename_file_directory_target_uses_source_basename() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_rename_dir_target");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let src = dir.join("source.txt");
    let dst_dir = dir.join("dest");
    fs::create_dir_all(&dst_dir).unwrap();
    fs::write(&src, b"renamed").unwrap();

    let dst_dir_arg = format!("{}/", dst_dir.to_string_lossy());
    builtin_rename_file(
        &mut Context::new(),
        vec![
            Value::string(src.to_string_lossy().as_ref()),
            Value::string(&dst_dir_arg),
        ],
    )
    .expect("rename-file should target basename within directory");

    assert!(!src.exists());
    assert_eq!(fs::read(dst_dir.join("source.txt")).unwrap(), b"renamed");

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn builtin_add_name_to_file_handles_raw_unibyte_paths() {
    crate::test_utils::init_test_tracing();
    let base = raw_temp_path(b"neovm-add-name-\xFF");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();

    let src = base.join(std::ffi::OsStr::from_bytes(b"src-\xFE"));
    let dst = base.join(std::ffi::OsStr::from_bytes(b"dst-\xFD"));
    fs::write(&src, b"link me").unwrap();

    builtin_add_name_to_file(
        &mut Context::new(),
        vec![raw_path_value(&src), raw_path_value(&dst)],
    )
    .expect("add-name-to-file should handle raw-byte paths");
    assert_same_file_path_bufs(&src, &dst);

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn builtin_add_name_to_file_directory_target_uses_source_basename() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_add_name_dir_target");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let src = dir.join("source.txt");
    let dst_dir = dir.join("dest");
    fs::create_dir_all(&dst_dir).unwrap();
    fs::write(&src, b"linked").unwrap();

    let dst_dir_arg = format!("{}/", dst_dir.to_string_lossy());
    builtin_add_name_to_file(
        &mut Context::new(),
        vec![
            Value::string(src.to_string_lossy().as_ref()),
            Value::string(&dst_dir_arg),
        ],
    )
    .expect("add-name-to-file should target basename within directory");

    assert_same_file_paths(
        src.to_string_lossy().as_ref(),
        dst_dir.join("source.txt").to_string_lossy().as_ref(),
    );

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn builtin_make_symbolic_link_directory_target_uses_target_basename() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_symlink_dir_target");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let target = dir.join("source.txt");
    let dst_dir = dir.join("links");
    fs::create_dir_all(&dst_dir).unwrap();
    fs::write(&target, b"x").unwrap();

    let dst_dir_arg = format!("{}/", dst_dir.to_string_lossy());
    builtin_make_symbolic_link(
        &mut Context::new(),
        vec![
            Value::string(target.to_string_lossy().as_ref()),
            Value::string(&dst_dir_arg),
        ],
    )
    .expect("make-symbolic-link should target basename within directory");

    let link = dst_dir.join("source.txt");
    assert!(link.exists());
    assert_eq!(
        fs::read_link(&link).unwrap().as_os_str().as_bytes(),
        target.as_os_str().as_bytes()
    );

    let _ = fs::remove_dir_all(&dir);
}

// -----------------------------------------------------------------------
// File attributes
// -----------------------------------------------------------------------

#[test]
fn test_file_attributes() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_attrs_test");
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("attrs.txt");
    let path_str = path.to_string_lossy().to_string();

    write_string_to_file("content", &path_str, false).unwrap();

    let attrs = file_attributes(&path_str).unwrap();
    assert_eq!(attrs.size, 7); // "content" is 7 bytes
    assert!(!attrs.is_dir);
    assert!(!attrs.is_symlink);
    assert!(attrs.modified.is_some());

    // Directory attributes
    let dir_str = dir.to_string_lossy().to_string();
    let dir_attrs = file_attributes(&dir_str).unwrap();
    assert!(dir_attrs.is_dir);

    // Non-existent file
    assert!(file_attributes("/nonexistent_path_12345").is_none());

    // Clean up
    let _ = fs::remove_dir_all(&dir);
}

// -----------------------------------------------------------------------
// Builtin wrappers (Value-level)
// -----------------------------------------------------------------------

#[test]
fn test_builtin_expand_file_name() {
    crate::test_utils::init_test_tracing();
    let result = call_fileio_builtin!(
        builtin_expand_file_name,
        vec![Value::string("/usr/local/bin/emacs")]
    );
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_utf8_str(), Some("/usr/local/bin/emacs"));

    // GNU treats an explicit non-string DEFAULT-DIRECTORY as root.
    let result = call_fileio_builtin!(
        builtin_expand_file_name,
        vec![Value::string("a"), Value::symbol("x")]
    );
    assert_eq!(result.unwrap().as_utf8_str(), Some("/a"));

    let result = call_fileio_builtin!(
        builtin_expand_file_name,
        vec![Value::string("a"), Value::NIL, Value::NIL]
    );
    assert!(result.is_err());
}

#[test]
fn test_builtin_expand_file_name_eval_uses_default_directory() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    // `default-directory` is a SYMBOL_FORWARDED BUFFER_OBJFWD slot
    // since Phase 10C; the per-buffer slot is the source of truth.
    // Using `eval.set_variable` routes through the FORWARDED path
    // (mirroring GNU's `set_internal` for SYMBOL_FORWARDED) so the
    // current buffer's slot is updated, which is what
    // `default_directory_in_state` reads.
    eval.set_variable("default-directory", Value::string("/tmp/neovm-expand/"));

    let with_implicit = builtin_expand_file_name(&mut eval, vec![Value::string("alpha.txt")]);
    assert_eq!(
        with_implicit.unwrap().as_utf8_str(),
        Some("/tmp/neovm-expand/alpha.txt")
    );

    let with_nil = builtin_expand_file_name(&mut eval, vec![Value::string("beta.txt"), Value::NIL]);
    assert_eq!(
        with_nil.unwrap().as_utf8_str(),
        Some("/tmp/neovm-expand/beta.txt")
    );
}

#[test]
fn builtin_expand_file_name_empty_name_and_empty_default_dir_returns_empty() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.set_variable("default-directory", Value::string("/base/dir/"));

    // GNU: `(expand-file-name "" "")` => "" — both "" share
    // `empty_unibyte_string`, so they are `eq`, the recursive expansion of
    // DEFAULT-DIRECTORY is skipped, and the empty NAME is returned unchanged.
    let both_empty =
        builtin_expand_file_name(&mut eval, vec![Value::string(""), Value::string("")]).unwrap();
    assert_eq!(both_empty.as_utf8_str(), Some(""));

    // GNU: `(expand-file-name "")` [nil/absent dir] => "/base/dir" — falls back
    // to `default-directory`.  The canonical empty-string identity must not
    // conflate an omitted/nil directory with an explicit empty string.
    let empty_name_nil_dir = builtin_expand_file_name(&mut eval, vec![Value::string("")]).unwrap();
    assert_eq!(empty_name_nil_dir.as_utf8_str(), Some("/base/dir"));

    let empty_name_explicit_nil =
        builtin_expand_file_name(&mut eval, vec![Value::string(""), Value::NIL]).unwrap();
    assert_eq!(empty_name_explicit_nil.as_utf8_str(), Some("/base/dir"));

    // GNU: `(expand-file-name "x" "")` => "/base/dir/x" — only NAME is empty, so
    // DEFAULT-DIRECTORY "" is still expanded against `default-directory`.
    let nonempty_name_empty_dir =
        builtin_expand_file_name(&mut eval, vec![Value::string("x"), Value::string("")]).unwrap();
    assert_eq!(nonempty_name_empty_dir.as_utf8_str(), Some("/base/dir/x"));

    // GNU: `(expand-file-name "" "/other/")` => "/other" — non-empty explicit dir.
    let empty_name_other_dir =
        builtin_expand_file_name(&mut eval, vec![Value::string(""), Value::string("/other/")])
            .unwrap();
    assert_eq!(empty_name_other_dir.as_utf8_str(), Some("/other"));
}

#[test]
fn builtin_expand_file_name_preserves_raw_unibyte_default_directory_bytes() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let raw_default = Value::heap_string(crate::heap_types::LispString::from_unibyte(
        b"/tmp/neovm-\xFF/".to_vec(),
    ));
    eval.set_variable("default-directory", raw_default);

    let value = builtin_expand_file_name(
        &mut eval,
        vec![Value::heap_string(
            crate::heap_types::LispString::from_unibyte(b"alpha.txt".to_vec()),
        )],
    )
    .expect("expand-file-name should keep raw default-directory bytes");

    assert_unibyte_string_bytes(value, b"/tmp/neovm-\xFF/alpha.txt");
}

#[test]
fn builtin_expand_file_name_promotes_ascii_unibyte_name_for_multibyte_default_directory() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.set_variable(
        "default-directory",
        Value::multibyte_string("/tmp/neovm-e/"),
    );

    let value = builtin_expand_file_name(
        &mut eval,
        vec![Value::heap_string(
            crate::heap_types::LispString::from_unibyte(b"alpha.txt".to_vec()),
        )],
    )
    .expect("expand-file-name should promote ascii unibyte names like GNU");

    let string = value.as_lisp_string().expect("string result");
    assert!(string.is_multibyte(), "expected multibyte string");
    assert_eq!(
        crate::emacs_core::emacs_char::to_utf8_lossy(string.as_bytes()),
        "/tmp/neovm-e/alpha.txt"
    );
}

#[test]
fn test_fileio_eval_prefers_current_buffer_local_default_directory() {
    crate::test_utils::init_test_tracing();
    let base =
        std::env::temp_dir().join(format!("neovm-fileio-buffer-local-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(base.join("subdir")).unwrap();
    fs::write(base.join("alpha.txt"), "alpha").unwrap();

    let mut eval = Context::new();
    eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
    let current = eval.buffers.current_buffer_id().expect("current buffer");
    let base_str = format!("{}/", base.to_string_lossy());
    eval.buffers
        .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
        .expect("buffer local default-directory should set");

    assert_eq!(
        builtin_expand_file_name(&mut eval, vec![Value::string("alpha.txt")])
            .unwrap()
            .as_utf8_str(),
        Some(base.join("alpha.txt").to_string_lossy().as_ref())
    );
    assert_eq!(
        builtin_file_truename(&mut eval, vec![Value::string("alpha.txt")])
            .unwrap()
            .as_utf8_str(),
        Some(base.join("alpha.txt").to_string_lossy().as_ref())
    );
    assert_eq!(
        builtin_file_exists_p(&mut eval, vec![Value::string("alpha.txt")]).unwrap(),
        Value::T
    );
    assert_eq!(
        builtin_file_directory_p(&mut eval, vec![Value::string("subdir")]).unwrap(),
        Value::T
    );
    assert_eq!(
        builtin_file_regular_p(&mut eval, vec![Value::string("alpha.txt")]).unwrap(),
        Value::T
    );

    let _ = fs::remove_dir_all(base);
}

#[test]
fn test_builtin_file_truename_counter_validation() {
    crate::test_utils::init_test_tracing();
    let value = call_fileio_builtin!(
        builtin_file_truename,
        vec![Value::string("/tmp"), Value::list(vec![])]
    )
    .unwrap();
    assert_eq!(value.as_utf8_str(), Some("/tmp"));

    let err = call_fileio_builtin!(
        builtin_file_truename,
        vec![Value::string("/tmp"), Value::fixnum(1)]
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("listp"), Value::fixnum(1)]);
        }
        other => panic!("expected signal, got {:?}", other),
    }

    let err = call_fileio_builtin!(
        builtin_file_truename,
        vec![
            Value::string("/tmp"),
            Value::list(vec![Value::symbol("visited")]),
        ]
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![
                    Value::symbol("number-or-marker-p"),
                    Value::symbol("visited")
                ]
            );
        }
        other => panic!("expected signal, got {:?}", other),
    }
}

#[cfg(windows)]
#[test]
fn windows_file_truename_accepts_backslash_drive_paths() {
    crate::test_utils::init_test_tracing();
    let value = call_fileio_builtin!(builtin_file_truename, vec![Value::string(r"C:\")]).unwrap();
    assert_eq!(value.as_utf8_str(), Some("C:/"));
}

#[cfg(windows)]
#[test]
fn windows_drive_root_is_file_truename_recursion_root() {
    crate::test_utils::init_test_tracing();
    let filename = LispString::from_utf8("D:/");
    let dirfile = lisp_directory_file_name(&filename);
    assert_eq!(dirfile.as_utf8_str(), Some("D:/"));

    let value = call_fileio_builtin!(builtin_file_truename, vec![Value::string("D:/")]).unwrap();
    assert_eq!(value.as_utf8_str(), Some("D:/"));
}

#[test]
fn test_builtin_file_truename_eval_uses_default_directory() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.set_variable(
        "default-directory",
        Value::string("/tmp/neovm-file-truename/"),
    );

    let value = builtin_file_truename(&mut eval, vec![Value::string("alpha.txt")]).unwrap();
    assert_eq!(
        value.as_utf8_str(),
        Some("/tmp/neovm-file-truename/alpha.txt")
    );
}

#[test]
fn test_builtin_make_temp_file_core_paths() {
    crate::test_utils::init_test_tracing();
    let file =
        call_fileio_builtin!(builtin_make_temp_file, vec![Value::string("neovm-mtf-")]).unwrap();
    let file_path = file.as_utf8_str().unwrap().to_string();
    assert!(file_exists_p(&file_path));
    delete_file(&file_path).unwrap();

    let dir = call_fileio_builtin!(
        builtin_make_temp_file,
        vec![Value::string("neovm-mtf-dir-"), Value::T]
    )
    .unwrap();
    let dir_path = dir.as_utf8_str().unwrap().to_string();
    assert!(file_directory_p(&dir_path));
    fs::remove_dir(&dir_path).unwrap();

    let with_text = call_fileio_builtin!(
        builtin_make_temp_file,
        vec![
            Value::string("neovm-mtf-text-"),
            Value::NIL,
            Value::string(".txt"),
            Value::string("abc"),
        ]
    )
    .unwrap();
    let text_path = with_text.as_utf8_str().unwrap().to_string();
    assert_eq!(read_file_contents(&text_path).unwrap(), "abc");
    delete_file(&text_path).unwrap();
}

#[cfg(unix)]
#[test]
fn test_builtin_make_temp_file_private_unix_mode_bits() {
    // GNU's `make-temp-file-internal` routes through gnulib `gen_tempname`,
    // which creates files via `open(..., O_CREAT|O_EXCL, S_IRUSR|S_IWUSR)`
    // (mode 0600) and directories via `mkdir(..., S_IRWXU)` (mode 0700),
    // regardless of the process umask.  The docstring promises "the
    // file/directory is created with access mode bits that limit access to
    // the current user."  Neomacs previously created temp files/dirs with the
    // umask-derived 0644/0755, leaking world-readable temp files.
    use std::os::unix::fs::MetadataExt;
    crate::test_utils::init_test_tracing();

    let file = call_fileio_builtin!(
        builtin_make_temp_file,
        vec![Value::string("neovm-mtf-mode-")]
    )
    .unwrap();
    let file_path = file.as_utf8_str().unwrap().to_string();
    let file_mode = fs::metadata(&file_path).unwrap().mode() & 0o7777;
    assert_eq!(
        file_mode, 0o600,
        "temp file should be created with private mode 0600, got {file_mode:o}"
    );
    delete_file(&file_path).unwrap();

    let dir = call_fileio_builtin!(
        builtin_make_temp_file,
        vec![Value::string("neovm-mtf-mode-dir-"), Value::T]
    )
    .unwrap();
    let dir_path = dir.as_utf8_str().unwrap().to_string();
    let dir_mode = fs::metadata(&dir_path).unwrap().mode() & 0o7777;
    assert_eq!(
        dir_mode, 0o700,
        "temp directory should be created with private mode 0700, got {dir_mode:o}"
    );
    fs::remove_dir(&dir_path).unwrap();
}

#[test]
fn test_builtin_make_temp_file_validation() {
    crate::test_utils::init_test_tracing();
    let err = call_fileio_builtin!(builtin_make_temp_file, vec![Value::fixnum(1)]).unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("sequencep"), Value::fixnum(1)]);
        }
        other => panic!("expected signal, got {:?}", other),
    }

    let err = call_fileio_builtin!(
        builtin_make_temp_file,
        vec![Value::string("neo"), Value::NIL, Value::fixnum(1)]
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::fixnum(1)]);
        }
        other => panic!("expected signal, got {:?}", other),
    }
}

#[test]
fn builtin_file_name_directory_preserves_raw_unibyte_bytes() {
    crate::test_utils::init_test_tracing();
    let value = call_fileio_builtin!(
        builtin_file_name_directory,
        vec![Value::heap_string(
            crate::heap_types::LispString::from_unibyte(b"/tmp/neovm-\xFF/alpha.txt".to_vec())
        )]
    )
    .expect("file-name-directory should keep raw bytes");

    assert_unibyte_string_bytes(value, b"/tmp/neovm-\xFF/");
}

#[test]
fn builtin_file_name_nondirectory_preserves_raw_unibyte_bytes() {
    crate::test_utils::init_test_tracing();
    let value = call_fileio_builtin!(
        builtin_file_name_nondirectory,
        vec![Value::heap_string(
            crate::heap_types::LispString::from_unibyte(b"/tmp/neovm-\xFF".to_vec())
        )]
    )
    .expect("file-name-nondirectory should keep raw bytes");

    assert_unibyte_string_bytes(value, b"neovm-\xFF");
}

#[test]
fn builtin_file_name_as_directory_preserves_raw_unibyte_bytes() {
    crate::test_utils::init_test_tracing();
    let value = call_fileio_builtin!(
        builtin_file_name_as_directory,
        vec![Value::heap_string(
            crate::heap_types::LispString::from_unibyte(b"neovm-\xFF".to_vec())
        )]
    )
    .expect("file-name-as-directory should keep raw bytes");

    assert_unibyte_string_bytes(value, b"neovm-\xFF/");
}

#[test]
fn builtin_directory_file_name_preserves_raw_unibyte_bytes() {
    crate::test_utils::init_test_tracing();
    let value = call_fileio_builtin!(
        builtin_directory_file_name,
        vec![Value::heap_string(
            crate::heap_types::LispString::from_unibyte(b"neovm-\xFF/".to_vec())
        )]
    )
    .expect("directory-file-name should keep raw bytes");

    assert_unibyte_string_bytes(value, b"neovm-\xFF");
}

#[test]
fn test_builtin_make_temp_file_eval_honors_temp_directory() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let dir = std::env::temp_dir().join("neovm-mtf-eval");
    let _ = fs::create_dir_all(&dir);
    eval.obarray.set_symbol_value(
        "temporary-file-directory",
        Value::string(format!("{}/", dir.to_string_lossy())),
    );

    let value = builtin_make_temp_file(&mut eval, vec![Value::string("eval-neo-")]).unwrap();
    let path = value.as_utf8_str().unwrap().to_string();
    assert!(path.starts_with(&dir.to_string_lossy().to_string()));
    assert!(file_exists_p(&path));
    delete_file(&path).unwrap();
    let _ = fs::remove_dir(&dir);
}

#[test]
fn test_builtin_make_nearby_temp_file_core_semantics() {
    crate::test_utils::init_test_tracing();
    let path = call_fileio_builtin!(
        builtin_make_nearby_temp_file,
        vec![Value::string("neovm-nearby-")]
    )
    .unwrap();
    let path_str = path.as_utf8_str().unwrap().to_string();
    assert!(file_exists_p(&path_str));
    delete_file(&path_str).unwrap();

    let dir = call_fileio_builtin!(
        builtin_make_nearby_temp_file,
        vec![Value::string("neovm-nearby-dir-"), Value::T]
    )
    .unwrap();
    let dir_str = dir.as_utf8_str().unwrap().to_string();
    assert!(file_directory_p(&dir_str));
    fs::remove_dir(&dir_str).unwrap();

    let base = std::env::temp_dir().join("neovm-nearby-parent");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    let prefix = base.join("child-").to_string_lossy().to_string();
    let nearby =
        call_fileio_builtin!(builtin_make_nearby_temp_file, vec![Value::string(&prefix)]).unwrap();
    let nearby_str = nearby.as_utf8_str().unwrap().to_string();
    assert_eq!(
        file_name_directory(&nearby_str),
        file_name_directory(&prefix),
    );
    assert!(file_exists_p(&nearby_str));
    delete_file(&nearby_str).unwrap();
    fs::remove_dir_all(&base).unwrap();
}

#[test]
fn test_builtin_make_nearby_temp_file_eval_relative_prefix_uses_temp_dir() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join("neovm-nearby-eval");
    let sub = base.join("sub");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&sub).unwrap();
    let mut eval = Context::new();
    eval.set_variable(
        "default-directory",
        Value::string(format!("{}/", base.to_string_lossy())),
    );

    let err =
        builtin_make_nearby_temp_file(&mut eval, vec![Value::string("sub/child-")]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "file-missing"),
        other => panic!("expected signal, got {:?}", other),
    }
    let _ = fs::remove_dir_all(base);
}

#[test]
fn test_builtin_file_predicates() {
    crate::test_utils::init_test_tracing();
    let result = call_fileio_builtin!(builtin_file_exists_p, vec![Value::string("/tmp")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_truthy());

    let result = call_fileio_builtin!(builtin_file_directory_p, vec![Value::string("/tmp")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_truthy());

    let result = call_fileio_builtin!(
        builtin_file_exists_p,
        vec![Value::string("/no_such_file_xyz")]
    );
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn test_builtin_access_file_semantics() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        call_fileio_builtin!(
            builtin_access_file,
            vec![Value::string("/tmp"), Value::string("read")]
        )
        .unwrap(),
        Value::NIL
    );

    let missing = call_fileio_builtin!(
        builtin_access_file,
        vec![
            Value::string("/definitely-not-here-neovm"),
            Value::string("read"),
        ]
    )
    .expect_err("missing file should signal");
    match missing {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "file-missing");
            assert_eq!(sig.data.first(), Some(&Value::string("read")));
            assert_eq!(
                sig.data.last(),
                Some(&Value::string("/definitely-not-here-neovm"))
            );
        }
        other => panic!("expected file-missing signal, got {:?}", other),
    }

    let file_type = call_fileio_builtin!(
        builtin_access_file,
        vec![Value::fixnum(1), Value::string("read")]
    )
    .expect_err("FILE should require string");
    match file_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::fixnum(1)]);
        }
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }

    let op_type = call_fileio_builtin!(
        builtin_access_file,
        vec![Value::string("/tmp"), Value::fixnum(1)]
    )
    .expect_err("OPERATION should require string");
    match op_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::fixnum(1)]);
        }
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }
}

#[cfg(unix)]
#[test]
fn builtin_access_file_preserves_raw_unibyte_filename_in_errors() {
    crate::test_utils::init_test_tracing();
    let raw_missing = raw_temp_path(b"neovm-access-missing-\xFF");
    let _ = fs::remove_file(&raw_missing);
    let raw_value = Value::heap_string(crate::heap_types::LispString::from_unibyte(
        raw_missing.as_os_str().as_bytes().to_vec(),
    ));

    let err = call_fileio_builtin!(builtin_access_file, vec![raw_value, Value::string("read")])
        .expect_err("missing raw-byte file should signal");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "file-missing");
            assert_eq!(sig.data.first(), Some(&Value::string("read")));
            let path = sig.data.last().expect("raw filename in signal data");
            let string = path.as_lisp_string().expect("raw filename string");
            assert!(!string.is_multibyte(), "expected unibyte filename");
            assert_eq!(string.as_bytes(), raw_missing.as_os_str().as_bytes());
        }
        other => panic!("expected file-missing signal, got {:?}", other),
    }
}

#[test]
fn test_builtin_file_modes_semantics() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        call_fileio_builtin!(
            builtin_file_modes,
            vec![Value::string("/tmp/neovm-file-modes-missing")]
        )
        .unwrap(),
        Value::NIL
    );

    let path = call_fileio_builtin!(
        builtin_make_temp_file,
        vec![Value::string("neovm-file-modes-")]
    )
    .unwrap();
    let path_str = path.as_utf8_str().unwrap().to_string();
    let mode = call_fileio_builtin!(builtin_file_modes, vec![Value::string(&path_str)]).unwrap();
    assert!(mode.is_fixnum());
    let with_flag =
        call_fileio_builtin!(builtin_file_modes, vec![Value::string(&path_str), Value::T]).unwrap();
    assert!(with_flag.is_fixnum());
    delete_file(&path_str).unwrap();
}

#[cfg(unix)]
#[test]
fn builtin_file_modes_treats_any_non_nil_flag_as_nofollow() {
    crate::test_utils::init_test_tracing();
    use std::os::unix::fs::PermissionsExt;

    let dir =
        std::env::temp_dir().join(format!("neovm-file-modes-nofollow-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir).unwrap();
    let target = dir.join("target");
    let link = dir.join("link");
    fs::write(&target, b"x").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let link_name = Value::string(link.to_string_lossy());
    let nofollow = call_fileio_builtin!(
        builtin_file_modes,
        vec![link_name, Value::symbol("nofollow")]
    )
    .unwrap();
    let arbitrary = call_fileio_builtin!(
        builtin_file_modes,
        vec![link_name, Value::symbol("anything-non-nil")]
    )
    .unwrap();
    let t_flag = call_fileio_builtin!(builtin_file_modes, vec![link_name, Value::T]).unwrap();
    let follow = call_fileio_builtin!(builtin_file_modes, vec![link_name, Value::NIL]).unwrap();

    assert_eq!(arbitrary, nofollow);
    assert_eq!(t_flag, nofollow);
    assert_eq!(follow.as_fixnum().unwrap() & 0o7777, 0o600);

    let _ = fs::remove_file(&link);
    let _ = fs::remove_file(&target);
    let _ = fs::remove_dir(&dir);
}

#[cfg(unix)]
#[test]
fn builtin_file_modes_handles_raw_unibyte_paths() {
    crate::test_utils::init_test_tracing();
    let path = raw_temp_path(b"neovm-file-modes-\xFF");
    let _ = fs::remove_file(&path);
    fs::write(&path, b"x").unwrap();
    let value = Value::heap_string(crate::heap_types::LispString::from_unibyte(
        path.as_os_str().as_bytes().to_vec(),
    ));

    let mode = call_fileio_builtin!(builtin_file_modes, vec![value]).unwrap();
    assert!(mode.is_fixnum());

    let _ = fs::remove_file(&path);
}

#[cfg(unix)]
#[test]
fn builtin_file_predicates_handle_raw_unibyte_paths() {
    crate::test_utils::init_test_tracing();
    let base = raw_temp_path(b"neovm-preds-\xFF");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();

    let file = base.join(std::ffi::OsStr::from_bytes(b"file-\xFF"));
    fs::write(&file, b"x").unwrap();
    let file_value = Value::heap_string(crate::heap_types::LispString::from_unibyte(
        file.as_os_str().as_bytes().to_vec(),
    ));
    let dir_value = Value::heap_string(crate::heap_types::LispString::from_unibyte(
        base.as_os_str().as_bytes().to_vec(),
    ));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&file).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&file, perms).unwrap();
    }

    assert_eq!(
        call_fileio_builtin!(builtin_file_exists_p, vec![file_value]).unwrap(),
        Value::T
    );
    assert_eq!(
        call_fileio_builtin!(builtin_file_readable_p, vec![file_value]).unwrap(),
        Value::T
    );
    assert_eq!(
        call_fileio_builtin!(builtin_file_writable_p, vec![file_value]).unwrap(),
        Value::T
    );
    assert_eq!(
        call_fileio_builtin!(builtin_file_regular_p, vec![file_value]).unwrap(),
        Value::T
    );
    assert_eq!(
        call_fileio_builtin!(builtin_file_executable_p, vec![file_value]).unwrap(),
        Value::T
    );
    assert_eq!(
        call_fileio_builtin!(builtin_file_directory_p, vec![dir_value]).unwrap(),
        Value::T
    );
    assert_eq!(
        call_fileio_builtin!(builtin_file_accessible_directory_p, vec![dir_value]).unwrap(),
        Value::T
    );

    let _ = fs::remove_file(&file);
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn test_builtin_file_modes_eval_respects_default_directory() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join("neovm-file-modes-eval");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    let file = base.join("alpha.txt");
    fs::write(&file, b"x").unwrap();

    let mut eval = Context::new();
    eval.set_variable(
        "default-directory",
        Value::string(format!("{}/", base.to_string_lossy())),
    );
    let mode = builtin_file_modes(&mut eval, vec![Value::string("alpha.txt")]).unwrap();
    assert!(mode.is_fixnum());

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn test_builtin_set_file_modes_semantics() {
    crate::test_utils::init_test_tracing();
    let path = call_fileio_builtin!(
        builtin_make_temp_file,
        vec![Value::string("neovm-set-file-modes-")]
    )
    .unwrap();
    let path_str = path.as_utf8_str().unwrap().to_string();

    assert_eq!(
        call_fileio_builtin!(
            builtin_set_file_modes,
            vec![Value::string(&path_str), Value::fixnum(0o600)]
        )
        .unwrap(),
        Value::NIL
    );
    assert_eq!(
        call_fileio_builtin!(
            builtin_set_file_modes,
            vec![Value::string(&path_str), Value::fixnum(0o640), Value::T]
        )
        .unwrap(),
        Value::NIL
    );
    assert_eq!(
        call_fileio_builtin!(builtin_file_modes, vec![Value::string(&path_str)])
            .unwrap()
            .as_int(),
        Some(0o640)
    );

    delete_file(&path_str).unwrap();
}

#[cfg(unix)]
#[test]
fn builtin_set_file_modes_handles_raw_unibyte_paths() {
    crate::test_utils::init_test_tracing();
    let path = raw_temp_path(b"neovm-set-file-modes-\xFF");
    let _ = fs::remove_file(&path);
    fs::write(&path, b"x").unwrap();
    let value = Value::heap_string(crate::heap_types::LispString::from_unibyte(
        path.as_os_str().as_bytes().to_vec(),
    ));

    call_fileio_builtin!(builtin_set_file_modes, vec![value, Value::fixnum(0o600)])
        .expect("set-file-modes should handle raw-byte paths");
    let mode = call_fileio_builtin!(builtin_file_modes, vec![value]).unwrap();
    assert_eq!(mode.as_int(), Some(0o600));

    let _ = fs::remove_file(&path);
}

#[cfg(unix)]
#[test]
fn builtin_set_file_modes_preserves_raw_unibyte_filename_in_errors() {
    crate::test_utils::init_test_tracing();
    let missing = raw_temp_path(b"neovm-set-file-modes-missing-\xFF");
    let _ = fs::remove_file(&missing);
    let value = Value::heap_string(crate::heap_types::LispString::from_unibyte(
        missing.as_os_str().as_bytes().to_vec(),
    ));

    let err = call_fileio_builtin!(builtin_set_file_modes, vec![value, Value::fixnum(0o600)])
        .expect_err("missing raw-byte file should signal");
    match err {
        Flow::Signal(sig) => {
            let path = sig.data.last().expect("raw filename in chmod signal");
            let string = path.as_lisp_string().expect("raw filename string");
            assert!(!string.is_multibyte(), "expected unibyte filename");
            assert_eq!(string.as_bytes(), missing.as_os_str().as_bytes());
        }
        other => panic!("expected signal, got {:?}", other),
    }
}

#[test]
fn test_builtin_set_file_modes_eval_respects_default_directory() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join("neovm-set-file-modes-eval");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    let file = base.join("alpha.txt");
    fs::write(&file, b"x").unwrap();

    let mut eval = Context::new();
    eval.set_variable(
        "default-directory",
        Value::string(format!("{}/", base.to_string_lossy())),
    );
    builtin_set_file_modes(
        &mut eval,
        vec![Value::string("alpha.txt"), Value::fixnum(0o600)],
    )
    .unwrap();
    assert_eq!(
        call_fileio_builtin!(
            builtin_file_modes,
            vec![Value::string(file.to_string_lossy().to_string())]
        )
        .unwrap()
        .as_int(),
        Some(0o600)
    );

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn test_builtin_directory_files_args() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_dirfiles_builtin");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let dir_str = dir.to_string_lossy().to_string();
    let file = dir.join("beta.el");
    fs::write(&file, "").unwrap();
    fs::write(dir.join("alpha.txt"), "").unwrap();
    fs::write(dir.join(".hidden"), "").unwrap();

    let result = call_fileio_builtin!(
        builtin_directory_files,
        vec![
            Value::string(&dir_str),
            Value::NIL,
            Value::string("\\.el$"),
            Value::NIL,
            Value::fixnum(1),
        ]
    )
    .unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].as_utf8_str(), Some("beta.el"));

    let unsorted = call_fileio_builtin!(
        builtin_directory_files,
        vec![Value::string(&dir_str), Value::NIL, Value::NIL, Value::T,]
    )
    .unwrap();
    let unsorted_items = list_to_vec(&unsorted).unwrap();

    let unsorted_limited = call_fileio_builtin!(
        builtin_directory_files,
        vec![
            Value::string(&dir_str),
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::fixnum(2),
        ]
    )
    .unwrap();
    let unsorted_limited_items = list_to_vec(&unsorted_limited).unwrap();
    let tail = &unsorted_items[unsorted_items.len() - 2..];
    assert_eq!(unsorted_limited_items.as_slice(), tail);

    let sorted_limited = call_fileio_builtin!(
        builtin_directory_files,
        vec![
            Value::string(&dir_str),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(2),
        ]
    )
    .unwrap();
    let mut sorted_from_unsorted = unsorted_limited_items.clone();
    sorted_from_unsorted.sort_by(|a, b| {
        let a = a.as_utf8_str().unwrap_or_default();
        let b = b.as_utf8_str().unwrap_or_default();
        a.cmp(b)
    });
    assert_eq!(list_to_vec(&sorted_limited).unwrap(), sorted_from_unsorted);

    let result = call_fileio_builtin!(
        builtin_directory_files,
        vec![
            Value::string(&dir_str),
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::fixnum(0),
        ]
    )
    .unwrap();
    assert!(list_to_vec(&result).unwrap().is_empty());

    let result = call_fileio_builtin!(
        builtin_directory_files,
        vec![
            Value::string(&dir_str),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(-1),
        ]
    );
    assert!(result.is_err());

    let result = call_fileio_builtin!(
        builtin_directory_files,
        vec![
            Value::string(&dir_str),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
        ]
    );
    assert!(result.is_err());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn directory_files_decodes_names_before_matching_and_returning_them() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join(format!("neovm_dirfiles_unicode_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("Übung – Lösung.zip"), "").unwrap();

    let mut eval = Context::new();
    eval.obarray.set_symbol_value(
        "default-file-name-coding-system",
        Value::symbol("utf-8-unix"),
    );
    let result = builtin_directory_files(
        &mut eval,
        vec![
            Value::string(dir.to_string_lossy().to_string()),
            Value::NIL,
            Value::string("Übung"),
        ],
    )
    .unwrap();

    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 1);
    let name = items[0].as_lisp_string().unwrap();
    assert!(name.is_multibyte());
    assert_eq!(name.as_utf8_str(), Some("Übung – Lösung.zip"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn directory_files_keeps_ascii_decoded_names_unibyte_like_gnu() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join(format!(
        "neovm_dirfiles_string_width_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("répertoire.bmk"), "").unwrap();

    let mut eval = Context::new();
    eval.obarray.set_symbol_value(
        "default-file-name-coding-system",
        Value::symbol("utf-8-unix"),
    );
    let result = builtin_directory_files(
        &mut eval,
        vec![Value::string(dir.to_string_lossy().to_string())],
    )
    .expect("directory-files should decode directory entries");

    let items = list_to_vec(&result).expect("directory-files must return a list");
    let entries: Vec<_> = items
        .iter()
        .map(|value| value.as_lisp_string().expect("directory entry string"))
        .collect();
    let dot = entries
        .iter()
        .find(|entry| entry.as_bytes() == b".")
        .expect("dot entry");
    let dotdot = entries
        .iter()
        .find(|entry| entry.as_bytes() == b"..")
        .expect("dot-dot entry");
    let accented = entries
        .iter()
        .find(|entry| entry.as_utf8_str() == Some("répertoire.bmk"))
        .expect("decoded accented entry");

    assert!(!dot.is_multibyte(), "GNU keeps decoded ASCII names unibyte");
    assert!(
        !dotdot.is_multibyte(),
        "GNU keeps decoded ASCII names unibyte"
    );
    assert!(
        accented.is_multibyte(),
        "a UTF-8 filename containing non-ASCII must be decoded to multibyte"
    );

    let _ = fs::remove_dir_all(&dir);
}

// GNU `directory_files_internal` (src/dired.c) applies COUNT to the *raw*
// readdir stream — it breaks after COUNT entries (`if (ind == last) break;`)
// and only THEN sorts.  So `(directory-files DIR nil nil nil COUNT)` is the
// SORT of the first COUNT readdir entries, NOT the lexicographic prefix of the
// fully sorted listing.  Readdir order is not portable, so assert the
// ALGORITHM directly: the sorted-COUNT result must equal sort(first COUNT of
// the raw readdir order), and must contain at most COUNT entries.
#[test]
fn test_directory_files_count_truncates_raw_stream_before_sort_like_gnu() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_dirfiles_count_algo");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for name in &["m.txt", "a.txt", "z.txt", "q.txt", "b.txt"] {
        fs::write(dir.join(name), "").unwrap();
    }
    let dir_ls =
        crate::heap_types::LispString::from_unibyte(dir.to_string_lossy().as_bytes().to_vec());

    const COUNT: usize = 3;

    // `nosort` keeps the traversal order WITHOUT sorting, exactly like GNU when
    // NOSORT is non-nil.  Both GNU and neomacs build the list with `cons`
    // (prepend), so the NOSORT listing is the REVERSE of the raw readdir order.
    // COUNT breaks after the first COUNT *readdir* entries, which are therefore
    // the LAST COUNT entries of the NOSORT listing; GNU then sorts them.
    let nosort = directory_files(&dir_ls, false, None, true, None).unwrap();
    let first_count_readdir = &nosort[nosort.len().saturating_sub(COUNT)..];
    let mut expected: Vec<Vec<u8>> = first_count_readdir
        .iter()
        .map(|s| s.as_bytes().to_vec())
        .collect();
    expected.sort();

    // The COUNT result must be sort(first-COUNT-of-raw), NOT the global sorted
    // prefix.
    let counted = directory_files(&dir_ls, false, None, false, Some(COUNT)).unwrap();
    let counted_bytes: Vec<Vec<u8>> = counted.iter().map(|s| s.as_bytes().to_vec()).collect();

    assert!(
        counted_bytes.len() <= COUNT,
        "COUNT must cap the entry count"
    );
    assert_eq!(
        counted_bytes, expected,
        "directory-files COUNT must truncate the raw readdir stream before sorting"
    );
    // Sanity: the result is itself sorted.
    let mut sorted_check = counted_bytes.clone();
    sorted_check.sort();
    assert_eq!(counted_bytes, sorted_check);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_builtin_directory_files_eval_respects_default_directory() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join("neovm_dirfiles_eval_builtin");
    let fixture = base.join("fixtures");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&fixture).unwrap();
    fs::write(fixture.join("alpha.txt"), "").unwrap();
    fs::write(fixture.join("beta.el"), "").unwrap();

    let mut eval = Context::new();
    let base_str = format!("{}/", base.to_string_lossy());
    eval.set_variable("default-directory", Value::string(&base_str));

    let result = builtin_directory_files(
        &mut eval,
        vec![
            Value::string("fixtures"),
            Value::NIL,
            Value::string("\\.el$"),
        ],
    )
    .unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].as_utf8_str(), Some("beta.el"));

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn directory_files_expands_tilde_from_lisp_process_environment_like_gnu() {
    crate::test_utils::init_test_tracing();
    let base = std::env::current_dir()
        .expect("current directory")
        .join("tmp")
        .join(format!("neovm-directory-files-home-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(base.join("Projects")).expect("create HOME fixture");
    let home = base.to_string_lossy().replace('\\', "\\\\");

    let results = bootstrap_eval(&format!(
        r#"(let ((previous-home (getenv "HOME"))
                 (abbreviated-home-dir nil))
             (unwind-protect
                 (progn
                   (setenv "HOME" "{home}")
                   (list
                    (directory-files "~/")
                    (mapcar #'car (directory-files-and-attributes "~/"))))
               (setenv "HOME" previous-home)))"#
    ));

    assert_eq!(
        results,
        vec![r#"OK (("." ".." "Projects") ("." ".." "Projects"))"#]
    );
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn test_builtin_directory_files_nonexistent_signals_file_missing() {
    crate::test_utils::init_test_tracing();
    let result = call_fileio_builtin!(
        builtin_directory_files,
        vec![Value::string("/nonexistent_dir_xyz_12345")]
    );
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "file-missing"),
        other => panic!("expected file-missing signal, got {:?}", other),
    }
}

#[test]
fn test_builtin_directory_files_invalid_regexp_signals_invalid_regexp() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_dirfiles_invalid_regexp");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let dir_str = dir.to_string_lossy().to_string();

    let result = call_fileio_builtin!(
        builtin_directory_files,
        vec![
            Value::string(&dir_str),
            Value::NIL,
            Value::string("[invalid"),
        ]
    );
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "invalid-regexp"),
        other => panic!("expected invalid-regexp signal, got {:?}", other),
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_builtin_file_ops_eval_respects_default_directory() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join("neovm_fileops_eval_builtin");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("alpha.txt"), "x").unwrap();

    let mut eval = Context::new();
    let base_str = format!("{}/", base.to_string_lossy());
    eval.set_variable("default-directory", Value::string(&base_str));

    builtin_copy_file(
        &mut eval,
        vec![Value::string("alpha.txt"), Value::string("beta.txt")],
    )
    .unwrap();
    assert!(base.join("beta.txt").exists());

    builtin_rename_file(
        &mut eval,
        vec![Value::string("beta.txt"), Value::string("gamma.txt")],
    )
    .unwrap();
    assert!(!base.join("beta.txt").exists());
    assert!(base.join("gamma.txt").exists());

    builtin_delete_file(&mut eval, vec![Value::string("gamma.txt")]).unwrap();
    assert!(!base.join("gamma.txt").exists());

    builtin_add_name_to_file(
        &mut eval,
        vec![Value::string("alpha.txt"), Value::string("delta.txt")],
    )
    .unwrap();
    assert!(base.join("delta.txt").exists());
    assert_same_file_paths(
        &base.join("alpha.txt").to_string_lossy(),
        &base.join("delta.txt").to_string_lossy(),
    );
    builtin_delete_file(&mut eval, vec![Value::string("delta.txt")]).unwrap();

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn test_builtin_rename_file_eval_overwrite_semantics() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join("neovm_rename_eval_overwrite");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("src.txt"), "x").unwrap();
    fs::write(base.join("dst.txt"), "y").unwrap();

    let mut eval = Context::new();
    let base_str = format!("{}/", base.to_string_lossy());
    eval.set_variable("default-directory", Value::string(&base_str));

    let err = builtin_rename_file(
        &mut eval,
        vec![Value::string("src.txt"), Value::string("dst.txt")],
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "file-already-exists"),
        other => panic!("expected signal, got {:?}", other),
    }

    assert_eq!(
        builtin_rename_file(
            &mut eval,
            vec![Value::string("src.txt"), Value::string("dst.txt"), Value::T],
        )
        .unwrap(),
        Value::NIL
    );
    assert!(!base.join("src.txt").exists());
    assert!(base.join("dst.txt").exists());

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn test_builtin_copy_file_eval_optional_arg_semantics() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join("neovm_copy_eval_optional");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("src.txt"), "src").unwrap();
    fs::write(base.join("dst.txt"), "dst").unwrap();

    let mut eval = Context::new();
    let base_str = format!("{}/", base.to_string_lossy());
    eval.set_variable("default-directory", Value::string(&base_str));

    let err = builtin_copy_file(
        &mut eval,
        vec![Value::string("src.txt"), Value::string("dst.txt")],
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "file-already-exists"),
        other => panic!("expected signal, got {:?}", other),
    }

    assert_eq!(
        builtin_copy_file(
            &mut eval,
            vec![Value::string("src.txt"), Value::string("dst.txt"), Value::T],
        )
        .unwrap(),
        Value::NIL
    );

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn test_builtin_file_name_ops() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_file_name_directory(&mut ev, vec![Value::string("/home/user/test.el")]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("/home/user/"));

    #[cfg(windows)]
    {
        let result = builtin_file_name_directory(
            &mut ev,
            vec![Value::string(
                r"D:\a\neomacs\neomacs\lisp\international\uni-titlecase.el",
            )],
        );
        assert_eq!(
            result.unwrap().as_utf8_str(),
            Some("D:/a/neomacs/neomacs/lisp/international/")
        );
    }

    let result = builtin_file_name_nondirectory(&mut ev, vec![Value::string("/home/user/test.el")]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("test.el"));

    #[cfg(windows)]
    {
        let result = builtin_file_name_nondirectory(
            &mut ev,
            vec![Value::string(
                r"D:\a\neomacs\neomacs\lisp\international\uni-titlecase.el",
            )],
        );
        assert_eq!(result.unwrap().as_utf8_str(), Some("uni-titlecase.el"));
    }

    let result = builtin_file_name_as_directory(&mut ev, vec![Value::string("/home/user")]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("/home/user/"));

    let result = builtin_directory_file_name(&mut ev, vec![Value::string("/home/user/")]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("/home/user"));

    let result = builtin_file_name_concat(vec![
        Value::string("foo"),
        Value::string(""),
        Value::NIL,
        Value::string("bar"),
    ]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("foo/bar"));
}

#[test]
fn test_builtin_file_name_ops_strict_types() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    assert!(builtin_file_name_directory(&mut ev, vec![Value::symbol("x")]).is_err());
    assert!(builtin_file_name_nondirectory(&mut ev, vec![Value::symbol("x")]).is_err());
    assert!(builtin_file_name_as_directory(&mut ev, vec![Value::symbol("x")]).is_err());
    assert!(builtin_directory_file_name(&mut ev, vec![Value::symbol("x")]).is_err());
}

#[test]
fn file_name_with_extension_bootstrap_matches_gnu_elisp() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (file-name-with-extension "foo" "el")
        (file-name-with-extension "foo.el" "txt")
        (file-name-with-extension "foo" ".el")
        (condition-case err (file-name-with-extension "foo" "") (error (car err)))
        (condition-case err (file-name-with-extension "/tmp/dir/" "el") (error (car err)))
        (condition-case err (file-name-with-extension 'x "el") (error (car err)))
        (condition-case err (file-name-with-extension "x" 'el) (error (car err)))
        "#,
    );
    assert_eq!(results[0], r#"OK "foo.el""#);
    assert_eq!(results[1], r#"OK "foo.txt""#);
    assert_eq!(results[2], r#"OK "foo.el""#);
    assert_eq!(results[3], "OK error");
    assert_eq!(results[4], "OK error");
    assert_eq!(results[5], "OK wrong-type-argument");
    assert_eq!(results[6], "OK wrong-type-argument");
}

#[test]
fn file_name_splitters_bootstrap_match_gnu_files_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (list (subrp (symbol-function 'file-name-extension))
              (subrp (symbol-function 'file-name-sans-extension))
              (subrp (symbol-function 'file-name-base))
              (subrp (symbol-function 'file-name-parent-directory))
              (subrp (symbol-function 'file-name-split)))
        (file-name-extension "/home/user/test.el")
        (file-name-extension "/home/user/test.el" t)
        (file-name-extension "no_ext" t)
        (file-name-sans-extension "/home/user/test.el")
        (file-name-base "/home/user/test.el")
        (file-name-parent-directory "/foo/bar")
        (file-name-parent-directory "/foo/")
        (file-name-parent-directory "/")
        (file-name-parent-directory "foo/bar")
        (file-name-parent-directory "foo")
        (file-name-parent-directory "//usr")
        (file-name-split "/foo/bar")
        (file-name-split "/")
        (file-name-split "foo/")
        (file-name-split "")
        "#,
    );
    assert_eq!(results[0], "OK (nil nil nil nil nil)");
    assert_eq!(results[1], r#"OK "el""#);
    assert_eq!(results[2], r#"OK ".el""#);
    assert_eq!(results[3], r#"OK """#);
    assert_eq!(results[4], r#"OK "/home/user/test""#);
    assert_eq!(results[5], r#"OK "test""#);
    assert_eq!(results[6], r#"OK "/foo/""#);
    assert_eq!(results[7], r#"OK "/""#);
    assert_eq!(results[8], "OK nil");
    assert_eq!(results[9], r#"OK "foo/""#);
    assert_eq!(results[10], r#"OK "./""#);
    assert_eq!(results[11], r#"OK "//""#);
    assert_eq!(results[12], r#"OK ("" "foo" "bar")"#);
    assert_eq!(results[13], r#"OK ("" "" "")"#);
    assert_eq!(results[14], r#"OK ("foo" "")"#);
    assert_eq!(results[15], "OK nil");
}

#[test]
fn file_name_splitters_bootstrap_error_shapes_match_gnu_files_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (condition-case err (file-name-extension 'x) (error (car err)))
        (condition-case err (file-name-extension "x" nil nil) (error (car err)))
        (condition-case err (file-name-sans-extension 'x) (error (car err)))
        (condition-case err (file-name-base 'x) (error (car err)))
        (condition-case err (file-name-parent-directory 'x) (error (car err)))
        (condition-case err (file-name-split 'x) (error (car err)))
        "#,
    );
    assert_eq!(results[0], "OK wrong-type-argument");
    assert_eq!(results[1], "OK wrong-number-of-arguments");
    assert_eq!(results[2], "OK wrong-type-argument");
    assert_eq!(results[3], "OK wrong-type-argument");
    assert_eq!(results[4], "OK wrong-type-argument");
    assert_eq!(results[5], "OK wrong-type-argument");
}

#[test]
fn file_name_sans_versions_bootstrap_matches_gnu_files_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (subrp (symbol-function 'file-name-sans-versions))
        (file-name-sans-versions "foo.~12~")
        (file-name-sans-versions "foo.~12~.~3~")
        (file-name-sans-versions "foo.~~")
        (file-name-sans-versions "foo.~12~" t)
        "#,
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], r#"OK "foo""#);
    assert_eq!(results[2], r#"OK "foo.~12~""#);
    assert_eq!(results[3], r#"OK "foo.~""#);
    assert_eq!(results[4], r#"OK "foo.~12~""#);
}

#[test]
fn file_name_sans_versions_bootstrap_error_shapes_match_gnu_files_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (condition-case err (file-name-sans-versions 'x) (error (car err)))
        (condition-case err (file-name-sans-versions "x" nil nil) (error (car err)))
        "#,
    );
    assert_eq!(results[0], "OK wrong-type-argument");
    assert_eq!(results[1], "OK wrong-number-of-arguments");
}

#[test]
fn file_name_misc_bootstrap_matches_gnu_files_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r##"
        (list (subrp (symbol-function 'convert-standard-filename))
              (subrp (symbol-function 'backup-file-name-p))
              (subrp (symbol-function 'auto-save-file-name-p))
              (subrp (symbol-function 'abbreviate-file-name)))
        (backup-file-name-p "foo.~12~")
        (backup-file-name-p "foo.txt")
        (auto-save-file-name-p "#foo#")
        (auto-save-file-name-p "foo.txt")
        (let* ((home (expand-file-name "~"))
               (under (concat home "/project")))
          (list (equal (abbreviate-file-name home) "~")
                (equal (abbreviate-file-name under) "~/project")
                (abbreviate-file-name "/tmp/x")))
        (convert-standard-filename "/tmp/x")
        (convert-standard-filename 'x)
        (convert-standard-filename 42)
        "##,
    );
    assert_eq!(results[0], "OK (nil nil nil nil)");
    assert_eq!(results[1], "OK 7");
    assert_eq!(results[2], "OK nil");
    assert_eq!(results[3], "OK 0");
    assert_eq!(results[4], "OK nil");
    assert_eq!(results[5], r#"OK (t t "/tmp/x")"#);
    assert_eq!(results[6], r#"OK "/tmp/x""#);
    assert_eq!(results[7], "OK x");
    assert_eq!(results[8], "OK 42");
}

#[test]
fn file_name_misc_bootstrap_error_shapes_match_gnu_files_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (condition-case err (backup-file-name-p 'x) (error (car err)))
        (condition-case err (auto-save-file-name-p 'x) (error (car err)))
        (condition-case err (abbreviate-file-name 'x) (error (car err)))
        (condition-case err (convert-standard-filename) (error (car err)))
        (condition-case err (convert-standard-filename nil nil) (error (car err)))
        "#,
    );
    assert_eq!(results[0], "OK wrong-type-argument");
    assert_eq!(results[1], "OK wrong-type-argument");
    assert_eq!(results[2], "OK wrong-type-argument");
    assert_eq!(results[3], "OK wrong-number-of-arguments");
    assert_eq!(results[4], "OK wrong-number-of-arguments");
}

#[test]
fn test_builtin_file_name_concat_strict_types() {
    crate::test_utils::init_test_tracing();
    let result = builtin_file_name_concat(vec![Value::NIL, Value::string("bar")]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("bar"));

    let result = builtin_file_name_concat(vec![Value::symbol("foo"), Value::string("bar")]);
    assert!(result.is_err());
}

#[test]
fn test_builtin_path_predicates() {
    crate::test_utils::init_test_tracing();
    let result = builtin_file_name_absolute_p(vec![Value::string("/tmp")]);
    assert_eq!(result.unwrap(), Value::T);

    let result = builtin_file_name_absolute_p(vec![Value::string("tmp")]);
    assert_eq!(result.unwrap(), Value::NIL);

    let result = builtin_file_name_absolute_p(vec![Value::string(
        "~neovm-user-that-should-not-exist-94d11b/tmp",
    )]);
    assert_eq!(result.unwrap(), Value::NIL);

    let result = builtin_directory_name_p(vec![Value::string("foo/")]);
    assert_eq!(result.unwrap(), Value::T);

    let result = builtin_directory_name_p(vec![Value::string("foo")]);
    assert_eq!(result.unwrap(), Value::NIL);

    let base = std::env::temp_dir().join("neovm_builtin_directory_empty_p");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    let file = base.join("entry");
    fs::write(&file, "x").unwrap();

    fs::remove_file(&file).unwrap();

    fs::remove_dir_all(&base).unwrap();
}

#[test]
fn test_builtin_path_predicates_strict_types() {
    crate::test_utils::init_test_tracing();
    let result = builtin_file_name_absolute_p(vec![Value::symbol("foo")]);
    assert!(result.is_err());

    let result = builtin_directory_name_p(vec![Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn test_builtin_file_predicates_strict_types() {
    crate::test_utils::init_test_tracing();
    assert!(call_fileio_builtin!(builtin_file_exists_p, vec![Value::NIL]).is_err());
    assert!(call_fileio_builtin!(builtin_file_readable_p, vec![Value::NIL]).is_err());
    assert!(call_fileio_builtin!(builtin_file_writable_p, vec![Value::NIL]).is_err());
    assert!(call_fileio_builtin!(builtin_file_directory_p, vec![Value::NIL]).is_err());
    assert!(call_fileio_builtin!(builtin_file_regular_p, vec![Value::NIL]).is_err());
    assert!(call_fileio_builtin!(builtin_file_symlink_p, vec![Value::NIL]).is_err());
    assert!(call_fileio_builtin!(builtin_file_name_case_insensitive_p, vec![Value::NIL]).is_err());
    assert!(
        call_fileio_builtin!(
            builtin_file_newer_than_file_p,
            vec![Value::NIL, Value::string("/tmp")]
        )
        .is_err()
    );
    assert!(
        call_fileio_builtin!(
            builtin_file_newer_than_file_p,
            vec![Value::string("/tmp"), Value::NIL]
        )
        .is_err()
    );
}

#[test]
fn test_eval_file_predicates_respect_default_directory() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_fileio_eval_default_dir");
    let subdir = dir.join("subdir");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&subdir).expect("create test subdir");

    let mut eval = Context::new();
    eval.set_variable("default-directory", Value::string(dir.to_string_lossy()));

    let is_dir = builtin_file_directory_p(&mut eval, vec![Value::string("subdir")])
        .expect("file-directory-p eval");
    assert!(is_dir.is_truthy());

    let exists = builtin_file_exists_p(&mut eval, vec![Value::string("subdir")])
        .expect("file-exists-p eval");
    assert!(exists.is_truthy());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_file_name_case_insensitive_eval_respects_default_directory() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_fileio_case_insensitive_eval");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create test dir");
    let file = dir.join("alpha.txt");
    fs::write(&file, b"x").expect("create test file");

    let absolute = call_fileio_builtin!(
        builtin_file_name_case_insensitive_p,
        vec![Value::string(file.to_string_lossy())]
    )
    .expect("absolute case-insensitive query");

    let mut eval = Context::new();
    eval.set_variable(
        "default-directory",
        Value::string(format!("{}/", dir.to_string_lossy())),
    );
    let relative =
        builtin_file_name_case_insensitive_p(&mut eval, vec![Value::string("alpha.txt")])
            .expect("relative case-insensitive query");
    assert_eq!(relative, absolute);

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn builtin_file_system_info_handles_raw_unibyte_paths() {
    crate::test_utils::init_test_tracing();
    let dir = raw_temp_path(b"neovm-file-system-info-\xFF");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create raw dir");

    let value = builtin_file_system_info(&mut Context::new(), vec![raw_path_value(&dir)])
        .expect("file-system-info should accept raw-byte paths");
    let parts = list_to_vec(&value).expect("file-system-info should return list");
    assert_eq!(parts.len(), 3);
    assert!(parts.iter().all(|value| value.is_fixnum()));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_file_system_info_eval_respects_default_directory() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm-file-system-info-eval");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create test dir");

    let mut absolute_eval = Context::new();
    let absolute = builtin_file_system_info(
        &mut absolute_eval,
        vec![Value::string(dir.to_string_lossy().as_ref())],
    )
    .expect("absolute file-system-info");

    let mut eval = Context::new();
    eval.set_variable(
        "default-directory",
        Value::string(format!("{}/", dir.to_string_lossy())),
    );
    let relative = builtin_file_system_info(&mut eval, vec![Value::string(".")])
        .expect("relative file-system-info");
    let absolute_parts = list_to_vec(&absolute).expect("absolute file-system-info list");
    let relative_parts = list_to_vec(&relative).expect("relative file-system-info list");
    assert_eq!(absolute_parts.len(), 3);
    assert_eq!(relative_parts.len(), 3);
    assert_eq!(absolute_parts[0], relative_parts[0]);
    assert!(absolute_parts.iter().all(|value| value.is_fixnum()));
    assert!(relative_parts.iter().all(|value| value.is_fixnum()));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_builtin_file_newer_than_file_p_semantics() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm-file-newer-than-file-p");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create test dir");

    let old = dir.join("old.txt");
    let new = dir.join("new.txt");
    let missing = dir.join("missing.txt");

    fs::write(&old, b"old").expect("write old file");
    std::thread::sleep(std::time::Duration::from_millis(1200));
    fs::write(&new, b"new").expect("write new file");

    assert_eq!(
        call_fileio_builtin!(
            builtin_file_newer_than_file_p,
            vec![
                Value::string(new.to_string_lossy()),
                Value::string(old.to_string_lossy()),
            ]
        )
        .expect("newer"),
        Value::T
    );
    assert_eq!(
        call_fileio_builtin!(
            builtin_file_newer_than_file_p,
            vec![
                Value::string(old.to_string_lossy()),
                Value::string(new.to_string_lossy()),
            ]
        )
        .expect("older"),
        Value::NIL
    );
    assert_eq!(
        call_fileio_builtin!(
            builtin_file_newer_than_file_p,
            vec![
                Value::string(missing.to_string_lossy()),
                Value::string(old.to_string_lossy()),
            ]
        )
        .expect("missing first"),
        Value::NIL
    );
    assert_eq!(
        call_fileio_builtin!(
            builtin_file_newer_than_file_p,
            vec![
                Value::string(old.to_string_lossy()),
                Value::string(missing.to_string_lossy()),
            ]
        )
        .expect("missing second"),
        Value::T
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_file_newer_than_file_p_eval_respects_default_directory() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm-file-newer-than-file-p-eval");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create test dir");

    let old = dir.join("old.txt");
    let new = dir.join("new.txt");
    fs::write(&old, b"old").expect("write old file");
    std::thread::sleep(std::time::Duration::from_millis(1200));
    fs::write(&new, b"new").expect("write new file");

    let mut eval = Context::new();
    eval.set_variable(
        "default-directory",
        Value::string(format!("{}/", dir.to_string_lossy())),
    );

    let result = builtin_file_newer_than_file_p(
        &mut eval,
        vec![Value::string("new.txt"), Value::string("old.txt")],
    )
    .expect("relative newer check");
    assert_eq!(result, Value::T);

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn builtin_file_newer_than_file_p_handles_raw_unibyte_paths() {
    crate::test_utils::init_test_tracing();
    let dir = raw_temp_path(b"neovm-file-newer-\xFF");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create raw test dir");

    let old = dir.join(std::ffi::OsStr::from_bytes(b"old-\xFE"));
    let new = dir.join(std::ffi::OsStr::from_bytes(b"new-\xFD"));
    fs::write(&old, b"old").expect("write old raw file");
    std::thread::sleep(std::time::Duration::from_millis(1200));
    fs::write(&new, b"new").expect("write new raw file");

    assert_eq!(
        builtin_file_newer_than_file_p(
            &mut Context::new(),
            vec![raw_path_value(&new), raw_path_value(&old)]
        )
        .expect("raw file-newer-than-file-p"),
        Value::T
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_builtin_set_file_times_semantics() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm-set-file-times");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create test dir");

    let older = dir.join("older.txt");
    let newer = dir.join("newer.txt");
    fs::write(&older, b"older").expect("write older");
    fs::write(&newer, b"newer").expect("write newer");

    assert_eq!(
        call_fileio_builtin!(
            builtin_set_file_times,
            vec![Value::string(older.to_string_lossy()), Value::fixnum(0),]
        )
        .expect("set-file-times"),
        Value::T
    );
    assert_eq!(
        call_fileio_builtin!(
            builtin_set_file_times,
            vec![Value::string(newer.to_string_lossy()), Value::NIL, Value::T,]
        )
        .expect("set-file-times with flag"),
        Value::T
    );
    assert_eq!(
        call_fileio_builtin!(
            builtin_file_newer_than_file_p,
            vec![
                Value::string(newer.to_string_lossy()),
                Value::string(older.to_string_lossy()),
            ]
        )
        .expect("newer-than"),
        Value::T
    );

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn builtin_set_file_times_handles_raw_unibyte_paths() {
    crate::test_utils::init_test_tracing();
    let dir = raw_temp_path(b"neovm-set-file-times-\xFF");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create raw set-file-times dir");

    let file = dir.join(std::ffi::OsStr::from_bytes(b"alpha-\xFE"));
    fs::write(&file, b"alpha").expect("write raw file");

    assert_eq!(
        builtin_set_file_times(
            &mut Context::new(),
            vec![raw_path_value(&file), Value::fixnum(0)],
        )
        .expect("raw set-file-times"),
        Value::T
    );

    let mtime = fs::metadata(&file)
        .expect("metadata")
        .modified()
        .expect("modified")
        .duration_since(UNIX_EPOCH)
        .expect("epoch")
        .as_secs();
    assert_eq!(mtime, 0);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_set_file_times_eval_respects_default_directory() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm-set-file-times-eval");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create test dir");
    let file = dir.join("alpha.txt");
    fs::write(&file, b"alpha").expect("write file");

    let mut eval = Context::new();
    eval.set_variable(
        "default-directory",
        Value::string(format!("{}/", dir.to_string_lossy())),
    );

    assert_eq!(
        builtin_set_file_times(
            &mut eval,
            vec![Value::string("alpha.txt"), Value::fixnum(0)],
        )
        .expect("eval set-file-times"),
        Value::T
    );
    let mtime = fs::metadata(&file)
        .expect("metadata")
        .modified()
        .expect("modified")
        .duration_since(UNIX_EPOCH)
        .expect("epoch")
        .as_secs();
    assert_eq!(mtime, 0);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_visited_file_modtime_state_builtins_use_current_buffer_file_name() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let current = eval.buffers.current_buffer_id().expect("current buffer");

    assert_eq!(
        builtin_verify_visited_file_modtime(&mut eval, vec![Value::make_buffer(current)])
            .expect("verify-visited-file-modtime"),
        Value::T
    );

    let missing = builtin_set_visited_file_modtime(&mut eval, vec![Value::NIL])
        .expect_err("missing visited file should signal");
    match missing {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::NIL]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    eval.buffers
        .set_buffer_file_name(current, Value::string("/tmp/neovm-visited-file.txt"))
        .expect("buffer file name should set");
    assert_eq!(
        builtin_set_visited_file_modtime(&mut eval, vec![Value::NIL])
            .expect("set-visited-file-modtime"),
        Value::NIL
    );
}

#[test]
fn test_default_file_modes_round_trip() {
    crate::test_utils::init_test_tracing();
    let original = builtin_default_file_modes(vec![])
        .expect("default-file-modes")
        .as_int()
        .expect("default-file-modes int");
    assert_eq!(
        builtin_set_default_file_modes(vec![Value::fixnum(0o700)]).expect("set-default-file-modes"),
        Value::NIL
    );
    assert_eq!(
        builtin_default_file_modes(vec![])
            .expect("default-file-modes after set")
            .as_int(),
        Some(0o700)
    );
    let _ = builtin_set_default_file_modes(vec![Value::fixnum(original)]);
}

#[test]
fn test_default_file_modes_argument_errors() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_set_default_file_modes(vec![]).is_err());
    assert!(builtin_default_file_modes(vec![Value::fixnum(1)]).is_err());
    assert!(builtin_set_default_file_modes(vec![Value::NIL]).is_err());
}

#[test]
fn test_builtin_substitute_in_file_name() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result =
        builtin_substitute_in_file_name(&mut ev, vec![Value::string("$HOME/foo")]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("$HOME/foo"));

    let values = bootstrap_eval(
        r#"
(let ((process-environment (copy-sequence process-environment)))
  (setenv "NEOMACS_SUBSTITUTE_UNIT_HOME" "/tmp/neomacs-home")
  (substitute-in-file-name "$NEOMACS_SUBSTITUTE_UNIT_HOME/foo"))
"#,
    );
    assert_eq!(
        values.last().map(String::as_str),
        Some(r#"OK "/tmp/neomacs-home/foo""#)
    );
}

#[test]
fn test_builtin_substitute_in_file_name_strict_type() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_substitute_in_file_name(&mut ev, vec![Value::symbol("foo")]);
    assert!(result.is_err());
}

#[test]
fn test_builtin_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    // expand-file-name needs at least 1 arg
    let result = call_fileio_builtin!(builtin_expand_file_name, vec![]);
    assert!(result.is_err());

    // file-exists-p needs exactly 1 arg
    let result = call_fileio_builtin!(builtin_file_exists_p, vec![]);
    assert!(result.is_err());
}

// -----------------------------------------------------------------------
// Context-dependent builtins
// -----------------------------------------------------------------------

#[test]
fn test_insert_file_contents_and_write_region() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = std::env::temp_dir().join("neovm_eval_fileio_test");
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("eval_test.txt");
    let path_str = path.to_string_lossy().to_string();

    // Write a test file to disk
    write_string_to_file("hello from file", &path_str, false).unwrap();

    let mut eval = Context::new();

    // insert-file-contents
    let result = builtin_insert_file_contents(&mut eval, vec![Value::string(&path_str)]);
    assert!(result.is_ok());

    // Check that the buffer now contains the text
    let buf = eval.buffers.current_buffer().unwrap();
    assert_eq!(buf.buffer_string(), "hello from file");

    // write-region: write entire buffer to a new file
    let out_path = dir.join("output.txt");
    let out_str = out_path.to_string_lossy().to_string();
    let result = builtin_write_region(
        &mut eval,
        vec![Value::NIL, Value::NIL, Value::string(&out_str)],
    );
    assert!(result.is_ok());

    let written = read_file_contents(&out_str).unwrap();
    assert_eq!(written, "hello from file");

    // Clean up
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn insert_file_contents_non_visit_change_hooks_match_gnu() {
    crate::test_utils::init_test_tracing();

    let dir = std::env::temp_dir().join("neovm_eval_insert_file_contents_change_hooks");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let path = dir.join("insert.txt");
    let path_str = path.to_string_lossy().to_string();
    fs::write(&path, "abc").unwrap();

    let mut eval = Context::new();
    eval.eval_str(
        r#"
(progn
  (erase-buffer)
  (insert "xx")
  (goto-char 2)
  (setq neomacs-ifc-events nil)
  (setq before-change-functions
        (list (lambda (beg end)
                (setq neomacs-ifc-events
                      (cons (list 'before beg end) neomacs-ifc-events)))))
  (setq after-change-functions
        (list (lambda (beg end old-len)
                (setq neomacs-ifc-events
                      (cons (list 'after beg end old-len) neomacs-ifc-events))))))
"#,
    )
    .expect("hook setup should evaluate");

    builtin_insert_file_contents(&mut eval, vec![Value::string(&path_str)])
        .expect("insert-file-contents should succeed");

    let rendered = format_eval_result(&eval.eval_str(
        r#"
(list (buffer-string) (point) (nreverse neomacs-ifc-events))
"#,
    ));
    assert_eq!(rendered, "OK (\"xabcx\" 2 ((before 2 2) (after 2 5 0)))");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_insert_file_contents_visit_sets_file_name_and_clears_modified() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = std::env::temp_dir().join("neovm_eval_insert_file_contents_visit");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let path = dir.join("visit.txt");
    let path_str = path.to_string_lossy().to_string();
    write_string_to_file("visited text", &path_str, false).unwrap();

    let mut eval = Context::new();
    let result = builtin_insert_file_contents(&mut eval, vec![Value::string(&path_str), Value::T])
        .expect("insert-file-contents with visit should succeed");
    let parts = list_to_vec(&result).expect("insert-file-contents should return list");
    assert_eq!(parts[0].as_utf8_str(), Some(path_str.as_str()));

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "visited text");
    assert_eq!(
        buf.file_name_runtime_string_owned().as_deref(),
        Some(path_str.as_str())
    );
    assert!(
        buf.buffer_local_value("buffer-file-truename")
            .unwrap_or(Value::NIL)
            .is_nil()
    );
    assert!(!buf.is_modified());
    assert!(buf.get_undo_list().is_nil());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn insert_file_contents_visit_does_not_signal_change_hooks_like_gnu() {
    crate::test_utils::init_test_tracing();

    let dir = std::env::temp_dir().join("neovm_eval_insert_file_contents_visit_hooks");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let path = dir.join("visit-hooks.txt");
    let path_str = path.to_string_lossy().to_string();
    fs::write(&path, "abc").unwrap();

    let mut eval = Context::new();
    eval.eval_str(
        r#"
(progn
  (setq neomacs-ifc-events nil)
  (setq before-change-functions
        (list (lambda (beg end)
                (setq neomacs-ifc-events
                      (cons (list 'before beg end) neomacs-ifc-events)))))
  (setq after-change-functions
        (list (lambda (beg end old-len)
                (setq neomacs-ifc-events
                      (cons (list 'after beg end old-len) neomacs-ifc-events))))))
"#,
    )
    .expect("hook setup should evaluate");

    builtin_insert_file_contents(&mut eval, vec![Value::string(&path_str), Value::T])
        .expect("visited insert-file-contents should succeed");

    let rendered = format_eval_result(&eval.eval_str(
        r#"
(list (buffer-string) (point) neomacs-ifc-events)
"#,
    ));
    assert_eq!(rendered, "OK (\"abc\" 1 nil)");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn insert_file_contents_replace_preserves_matching_ends_and_hooks_like_gnu() {
    crate::test_utils::init_test_tracing();

    let dir = std::env::temp_dir().join("neovm_eval_insert_file_contents_replace_hooks");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let path = dir.join("replace.txt");
    let path_str = path.to_string_lossy().to_string();
    fs::write(&path, "abXYZe").unwrap();

    let mut eval = Context::new();
    eval.eval_str(
        r#"
(progn
  (erase-buffer)
  (insert "abcde")
  (goto-char 4)
  (setq neomacs-ifc-events nil)
  (setq before-change-functions
        (list (lambda (beg end)
                (setq neomacs-ifc-events
                      (cons (list 'before beg end) neomacs-ifc-events)))))
  (setq after-change-functions
        (list (lambda (beg end old-len)
                (setq neomacs-ifc-events
                      (cons (list 'after beg end old-len) neomacs-ifc-events))))))
"#,
    )
    .expect("hook setup should evaluate");

    builtin_insert_file_contents(
        &mut eval,
        vec![
            Value::string(&path_str),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
        ],
    )
    .expect("replace insert-file-contents should succeed");

    let rendered = format_eval_result(&eval.eval_str(
        r#"
(list (buffer-string) (point) (nreverse neomacs-ifc-events))
"#,
    ));
    assert_eq!(
        rendered,
        "OK (\"abXYZe\" 4 ((before 3 5) (after 3 3 2) (before 3 3) (after 3 6 0)))"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn insert_file_contents_visit_replace_hides_file_name_from_change_hooks_like_gnu() {
    crate::test_utils::init_test_tracing();

    let dir = std::env::temp_dir().join("neovm_eval_insert_file_contents_visit_replace_name");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let path = dir.join("replace.txt");
    let path_str = path.to_string_lossy().to_string();
    fs::write(&path, "external contents").unwrap();

    let mut eval = Context::new();
    let current_id = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .set_buffer_file_name(current_id, Value::string(&path_str))
        .expect("set visited file name");
    eval.eval_str(
        r#"
(progn
  (erase-buffer)
  (insert "stale contents")
  (setq neomacs-ifc-file-names nil)
  (setq before-change-functions
        (list (lambda (_beg _end)
                (setq neomacs-ifc-file-names
                      (cons buffer-file-name neomacs-ifc-file-names))))))
"#,
    )
    .expect("hook setup should evaluate");

    builtin_insert_file_contents(
        &mut eval,
        vec![
            Value::string(&path_str),
            Value::T,
            Value::NIL,
            Value::NIL,
            Value::T,
        ],
    )
    .expect("visited replacement should succeed without a supersession check");

    let rendered = format_eval_result(&eval.eval_str(
        r#"
(list (buffer-string) buffer-file-name (nreverse neomacs-ifc-file-names))
"#,
    ));
    assert_eq!(
        rendered,
        format!("OK (\"external contents\" \"{path_str}\" (nil nil))")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn insert_file_contents_visit_missing_file_completes_visit_before_error() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = std::env::temp_dir().join("neovm_eval_insert_file_contents_missing_visit");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let path = dir.join("missing.txt");
    let path_str = path.to_string_lossy().to_string();

    let mut eval = Context::new();
    let current = eval.buffers.current_buffer_id().expect("current buffer");
    let _ = eval.buffers.set_buffer_modified_flag(current, true);

    let err = builtin_insert_file_contents(&mut eval, vec![Value::string(&path_str), Value::T])
        .expect_err("missing visited file should signal file-missing");
    match err {
        crate::emacs_core::error::Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "file-missing");
        }
        other => panic!("expected file-missing signal, got {other:?}"),
    }

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(
        buf.file_name_runtime_string_owned().as_deref(),
        Some(path_str.as_str())
    );
    assert!(
        buf.buffer_local_value("buffer-file-truename")
            .unwrap_or(Value::NIL)
            .is_nil()
    );
    assert!(!buf.is_modified());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn insert_file_contents_sets_last_coding_before_after_insert_hook() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = std::env::temp_dir().join("neovm_eval_insert_file_contents_coding_hook");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let path = dir.join("ascii.txt");
    let path_str = path.to_string_lossy().to_string();
    write_string_to_file("ascii text\n", &path_str, false).unwrap();

    let mut eval = Context::new();
    eval.eval_str(
        r#"(defalias 'after-insert-file-set-coding
             #'(lambda (_inserted _visit)
                 (setq neomacs-test-last-coding-in-hook last-coding-system-used)
                 nil))"#,
    )
    .expect("define hook");

    builtin_insert_file_contents(&mut eval, vec![Value::string(&path_str), Value::T])
        .expect("insert-file-contents with visit should succeed");

    assert_eq!(
        eval.visible_variable_value_or_nil("neomacs-test-last-coding-in-hook")
            .as_symbol_name(),
        Some("undecided-unix")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn decode_insert_file_contents_defaults_to_gnu_ascii_undecided_codings() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let without_eol = super::decode_insert_file_contents(&mut eval, b"alpha", true, None)
        .expect("decode ascii text without EOL evidence");
    assert_eq!(without_eol.text().as_utf8_str(), Some("alpha"));
    assert_eq!(without_eol.coding, "undecided");

    let unix =
        super::decode_insert_file_contents(&mut eval, b"alpha line\nbeta line\n", true, None)
            .expect("decode ascii unix text");
    assert_eq!(unix.text().as_utf8_str(), Some("alpha line\nbeta line\n"));
    assert_eq!(unix.coding, "undecided-unix");

    let dos =
        super::decode_insert_file_contents(&mut eval, b"alpha line\r\nbeta line\r\n", true, None)
            .expect("decode ascii dos text");
    assert_eq!(dos.text().as_utf8_str(), Some("alpha line\nbeta line\n"));
    assert_eq!(dos.coding, "undecided-dos");

    let mac = super::decode_insert_file_contents(&mut eval, b"alpha line\rbeta line\r", true, None)
        .expect("decode ascii mac text");
    assert_eq!(mac.text().as_utf8_str(), Some("alpha line\nbeta line\n"));
    assert_eq!(mac.coding, "undecided-mac");

    let stray_cr_in_dos =
        super::decode_insert_file_contents(&mut eval, b"alpha\rbeta\r\n", true, None)
            .expect("decode GNU-compatible mixed CR and CRLF text");
    assert_eq!(stray_cr_in_dos.coding, "undecided-dos");

    // No bounded evidence window: `decode_eol` (src/coding.c:6785-6806) ORs a
    // flag per line terminator over the WHOLE decoded text, so a fourth, bare
    // LF after three CR LFs makes the text MIXED and selects unix.  The three-
    // terminator limit belongs to `detect_eol` (src/coding.c:6373), which
    // serves `detect-coding-string` and never runs on this path.  Re-derived by
    // running the case under GNU Emacs 31.0.90: a file holding
    // `a\r\nb\r\nc\r\nd\n` reads back with every CR intact and
    // `last-coding-system-used' `undecided-unix'; the previous `undecided-dos'
    // pin recorded Neomacs's own answer.
    let a_fourth_bare_lf_makes_the_text_mixed =
        super::decode_insert_file_contents(&mut eval, b"a\r\nb\r\nc\r\nd\n", true, None)
            .expect("decode text whose line endings disagree");
    assert_eq!(
        a_fourth_bare_lf_makes_the_text_mixed.text().as_utf8_str(),
        Some("a\r\nb\r\nc\r\nd\n")
    );
    assert_eq!(
        a_fourth_bare_lf_makes_the_text_mixed.coding,
        "undecided-unix"
    );
}

#[test]
fn decode_insert_file_contents_preserves_lone_cr_in_lf_text() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let decoded = super::decode_insert_file_contents(&mut eval, b"alpha\rdata\nbeta\n", true, None)
        .expect("decode ascii unix text with embedded cr");
    assert_eq!(decoded.text().as_utf8_str(), Some("alpha\rdata\nbeta\n"));
    assert_eq!(decoded.coding, "undecided-unix");

    let decoded =
        super::decode_insert_file_contents(&mut eval, b"(setq probe \"a\rb\")\n", true, None)
            .expect("decode source-loaded unix text with embedded cr");
    assert_eq!(
        decoded.text().as_utf8_str(),
        Some("(setq probe \"a\rb\")\n")
    );
    assert_eq!(decoded.coding, "undecided-unix");
}

#[test]
fn insert_file_contents_preserves_lone_cr_in_lf_text() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("embedded-cr.el");
    fs::write(&path, b"(setq probe \"a\rb\")\n").expect("write fixture");
    let path_str = path.to_string_lossy().to_string();

    let mut eval = Context::new();
    builtin_insert_file_contents(&mut eval, vec![Value::string(&path_str)])
        .expect("insert-file-contents should preserve embedded cr");

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "(setq probe \"a\rb\")\n");
    assert_eq!(
        eval.visible_variable_value_or_nil("last-coding-system-used")
            .as_symbol_name(),
        Some("undecided-unix")
    );
}

#[test]
fn insert_file_contents_consumes_utf8_signature() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bom-source.el");
    fs::write(&path, b"\xEF\xBB\xBF;;; bom-source.el --- fixture\n")
        .expect("write utf-8 signature fixture");
    let path_str = path.to_string_lossy().to_string();

    let mut eval = Context::new();
    builtin_insert_file_contents(&mut eval, vec![Value::string(&path_str)])
        .expect("insert-file-contents should consume UTF-8 signature");

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), ";;; bom-source.el --- fixture\n");
    assert_eq!(
        eval.visible_variable_value_or_nil("last-coding-system-used")
            .as_symbol_name(),
        Some("utf-8-with-signature-unix")
    );
}

#[test]
fn decode_insert_file_contents_source_load_normalizes_detected_eols() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let decoded = super::decode_insert_file_contents(
        &mut eval,
        b"(message \"alpha\")\r\n(message \"beta\")\r\n",
        true,
        Some("utf-8-emacs"),
    )
    .expect("decode source-loaded dos-eol text with explicit utf-8-emacs coding");

    assert_eq!(
        decoded.text().as_utf8_str(),
        Some("(message \"alpha\")\n(message \"beta\")\n")
    );
    assert_eq!(decoded.coding, "utf-8-emacs-dos");

    let decoded = super::decode_insert_file_contents(
        &mut eval,
        b"(message \"alpha\")\r(message \"beta\")\r",
        true,
        None,
    )
    .expect("decode source-loaded mac-eol text");

    assert_eq!(
        decoded.text().as_utf8_str(),
        Some("(message \"alpha\")\n(message \"beta\")\n")
    );
    assert_eq!(decoded.coding, "undecided-mac");
}

#[test]
fn decode_insert_file_contents_accepts_chinese_big5_coding() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let decoded = super::decode_insert_file_contents(
        &mut eval,
        &[0xa4, 0x40, b'\r', b'\n'],
        true,
        Some("chinese-big5-unix"),
    )
    .expect("decode Big5 file bytes");

    assert_eq!(decoded.text().as_utf8_str(), Some("一\r\n"));
    assert_eq!(decoded.coding, "chinese-big5-unix");
}

#[test]
fn decode_insert_file_contents_accepts_chinese_gb2312_coding() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let decoded = super::decode_insert_file_contents(
        &mut eval,
        &[0xd2, 0xbb, b'\r', b'\n'],
        true,
        Some("cn-gb-2312-unix"),
    )
    .expect("decode GB2312 file bytes");

    assert_eq!(decoded.text().as_utf8_str(), Some("一\r\n"));
    // `coding` here is `last-coding-system-used', and GNU reports the ALIAS the
    // caller named when its end-of-line type is already concrete:
    // `Fdefine_coding_system_alias` puts the alias in the coding-system hash
    // table as a KEY of its own (src/coding.c), so `CODING_ID_NAME (coding.id)`
    // (src/coding.c:9497) is the alias, and `adjust_coding_eol_type` returns
    // "Already adjusted" without rewriting the id (src/coding.c:6477-6479).
    // Re-derived by running the case under GNU Emacs 31.0.90:
    // `last-coding-system-used' is `cn-gb-2312-unix' while
    // `buffer-file-coding-system' -- which Lisp canonicalises separately -- is
    // `chinese-iso-8bit-unix'.  The bare `cn-gb-2312' spelling, whose eol type
    // IS undecided, does report the canonical `chinese-iso-8bit-dos'.
    assert_eq!(decoded.coding, "cn-gb-2312-unix");
}

#[test]
fn insert_file_contents_preserves_decoded_charset_text_property() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gb2312.txt");
    fs::write(&path, [0xd2, 0xbb, b'\n']).expect("write GB2312 fixture");
    let path_str = path.to_string_lossy().to_string();

    let mut eval = Context::new();
    eval.set_variable("coding-system-for-read", Value::symbol("cn-gb-2312-unix"));
    builtin_insert_file_contents(&mut eval, vec![Value::string(&path_str)])
        .expect("insert-file-contents should decode GB2312");

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "一\n");
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new(0),
            Value::symbol("charset")
        )
        .and_then(|value| value.as_symbol_name()),
        Some("chinese-gb2312")
    );
    assert_eq!(
        buf.text_props_get_property_at_emacs_byte_pos(
            crate::buffer::EmacsBytePos::new("一".len()),
            Value::symbol("charset")
        )
        .and_then(|value| value.as_symbol_name()),
        Some("chinese-gb2312")
    );
}

#[test]
fn decode_insert_file_contents_adds_detected_eol_to_base_coding_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let decoded = super::decode_insert_file_contents(
        &mut eval,
        &[0xd2, 0xbb, b'\r', b'\n'],
        true,
        Some("cn-gb-2312"),
    )
    .expect("decode GB2312 file bytes with detected DOS EOL");

    assert_eq!(decoded.text().as_utf8_str(), Some("一\n"));
    assert_eq!(decoded.coding, "chinese-iso-8bit-dos");
}

#[test]
fn write_region_honors_dynamic_coding_system_for_write() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("dynamic-coding.eld");
    let path_lisp = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let results = bootstrap_eval(&format!(
        r#"(let ((coding-system-for-write 'emacs-internal))
             (with-temp-file "{path_lisp}"
               (insert "abc")))
           last-coding-system-used"#
    ));

    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK emacs-internal");
    assert_eq!(std::fs::read(&path).expect("read output"), b"abc");
}

#[test]
fn write_region_annotation_plan_intersperses_sorted_text_and_runs_post_cleanup() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("annotations.txt");
    let path_lisp = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let results = bootstrap_eval(&format!(
        r#"(let (first-saw second-saw post-ran)
             (with-temp-buffer
               (insert "éZ")
               (let ((write-region-annotate-functions
                      (list
                       (lambda (start end)
                         (setq first-saw
                               (list start end
                                     (copy-tree write-region-annotations-so-far)))
                         (list (cons start "<A>") (cons end "<END1>")))
                       (lambda (start end)
                         (setq second-saw
                               (list start end
                                     (copy-tree write-region-annotations-so-far)))
                         (list (cons 2 "<B>") (cons end "<END2>")))))
                     (write-region-post-annotation-function
                      (lambda () (setq post-ran t))))
                 (write-region (point-min) (point-max)
                               "{path_lisp}" nil 'silent)))
             (list first-saw second-saw post-ran))"#
    ));

    assert_eq!(
        results[0],
        "OK ((1 3 nil) (1 3 ((1 . \"<A>\") (3 . \"<END1>\"))) t)"
    );
    assert_eq!(
        std::fs::read(&path).expect("read annotated output"),
        "<A>é<B>Z<END2><END1>".as_bytes()
    );
}

#[test]
fn write_region_annotation_plan_preserves_order_within_equal_position_batches() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("equal-position-annotations.txt");
    let path_lisp = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let results = bootstrap_eval(&format!(
        r#"(with-temp-buffer
             (insert "x")
             (let ((write-region-annotate-functions
                    (list
                     (lambda (start _end)
                       (list (cons start "<A1>") (cons start "<A2>")))
                     (lambda (start _end)
                       (list (cons start "<B1>") (cons start "<B2>"))))))
               (write-region (point-min) (point-max)
                             "{path_lisp}" nil 'silent)))"#
    ));

    assert_eq!(results[0], "OK nil");
    assert_eq!(
        std::fs::read(&path).expect("read equal-position output"),
        b"<B1><B2><A1><A2>x"
    );
}

#[test]
fn write_region_literal_source_skips_annotations_but_runs_post_cleanup() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("literal.txt");
    let path_lisp = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let results = bootstrap_eval(&format!(
        r#"(let (annotation-ran post-ran)
             (with-temp-buffer
               (let ((write-region-annotate-functions
                      (list (lambda (&rest _)
                              (setq annotation-ran t)
                              (list (cons 1 "BAD")))))
                     (write-region-post-annotation-function
                      (lambda () (setq post-ran t))))
                 (write-region "raw" nil "{path_lisp}" nil 'silent)))
             (list annotation-ran post-ran))"#
    ));

    assert_eq!(results[0], "OK (nil t)");
    assert_eq!(std::fs::read(&path).expect("read literal output"), b"raw");
}

#[test]
fn write_region_annotation_buffer_switch_replaces_source_and_cleans_up_in_gnu_order() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("replacement.txt");
    let path_lisp = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let results = bootstrap_eval(&format!(
        r#"(let (replacement cleanup)
             (unwind-protect
                 (with-temp-buffer
                   (insert "old")
                   (let ((write-region-annotate-functions
                          (list (lambda (_start _end)
                                  (setq replacement (generate-new-buffer " *replacement*"))
                                  (set-buffer replacement)
                                  (insert "new")
                                  nil)))
                         (write-region-post-annotation-function
                          (lambda () (push (buffer-string) cleanup))))
                     (write-region (point-min) (point-max)
                                   "{path_lisp}" nil 'silent))
                   (list (nreverse cleanup) (buffer-string)))
               (when (buffer-live-p replacement)
                 (kill-buffer replacement))))"#
    ));

    assert_eq!(results[0], "OK ((\"new\" \"old\") \"old\")");
    assert_eq!(
        std::fs::read(&path).expect("read replacement output"),
        b"new"
    );
}

#[test]
fn write_region_honors_buffer_coding_cookie_over_default_dos_eol() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("generated-lisp.el");
    let path_lisp = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let results = bootstrap_eval(&format!(
        r#"(let ((saved-default (default-value 'buffer-file-coding-system)))
             (unwind-protect
                 (progn
                   (set-default 'buffer-file-coding-system 'undecided-dos)
                   (with-temp-buffer
                     (insert "alpha\n;; Local Variables:\n;; coding: utf-8-emacs-unix\n;; End:\n")
                     (write-region nil nil "{path_lisp}" nil 'silent)
                     last-coding-system-used))
               (set-default 'buffer-file-coding-system saved-default)))"#
    ));

    assert_eq!(results[0], "OK utf-8-emacs-unix");
    assert_eq!(
        std::fs::read(&path).expect("read generated Lisp output"),
        b"alpha\n;; Local Variables:\n;; coding: utf-8-emacs-unix\n;; End:\n"
    );
}

#[test]
fn write_region_uses_runtime_shift_jis_codec() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("shift-jis.txt");
    let path_lisp = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let results = bootstrap_eval(&format!(
        r#"(with-temp-buffer
             (insert "日本\n")
             (setq buffer-file-coding-system 'japanese-shift-jis-unix)
             (write-region nil nil "{path_lisp}" nil 'silent))"#
    ));

    assert_eq!(results[0], "OK nil");
    assert_eq!(
        std::fs::read(&path).expect("read Shift-JIS output"),
        [0x93, 0xfa, 0x96, 0x7b, b'\n']
    );
}

#[test]
fn write_region_uses_iso2022_file_stream_boundary() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("iso-2022-jp.txt");
    let path_lisp = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let results = bootstrap_eval(&format!(
        r#"(let ((text "日本語の設計"))
             (let ((coding-system-for-write 'iso-2022-jp))
               (with-temp-buffer
                 (insert text)
                 (write-region nil nil "{path_lisp}" nil 'silent)))
             (string-to-list (encode-coding-string text 'iso-2022-jp)))"#
    ));

    assert_eq!(
        results[0],
        "OK (27 36 66 70 124 75 92 56 108 36 78 64 95 55 87 27 40 66)"
    );
    assert_eq!(
        std::fs::read(&path).expect("read ISO-2022-JP output"),
        [27, 36, 66, 70, 124, 75, 92, 56, 108, 36, 78, 64, 95, 55, 87]
    );
}

#[test]
fn insert_file_contents_honors_dynamic_big5_coding_system_for_read() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("big5.txt");
    fs::write(&path, [0xa4, 0x40, b'\r', b'\n']).expect("write Big5 fixture");
    let path_lisp = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let results = bootstrap_eval(&format!(
        r#"(let ((coding-system-for-read
                  (coding-system-change-eol-conversion 'big5 'unix)))
             (with-temp-buffer
               (insert-file-contents "{path_lisp}")
               (list (special-variable-p 'coding-system-for-read)
                     (equal (string-to-list (buffer-string)) '(19968 13 10))
                     last-coding-system-used)))"#
    ));

    assert_eq!(results[0], "OK (t t chinese-big5-unix)");
}

#[test]
fn insert_file_contents_honors_dynamic_gb2312_coding_system_for_read() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gb2312.txt");
    fs::write(&path, [0xd2, 0xbb, b'\r', b'\n']).expect("write GB2312 fixture");
    let path_lisp = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let results = bootstrap_eval(&format!(
        r#"(let ((coding-system-for-read
                  (coding-system-change-eol-conversion 'cn-gb-2312 'unix)))
             (with-temp-buffer
               (insert-file-contents "{path_lisp}")
               (list (special-variable-p 'coding-system-for-read)
                     (equal (string-to-list (buffer-string)) '(19968 13 10))
                     last-coding-system-used)))"#
    ));

    assert_eq!(results[0], "OK (t t chinese-iso-8bit-unix)");
}

#[test]
fn insert_file_contents_consults_file_coding_system_alist_when_auto_coding_declines() {
    crate::test_utils::init_test_tracing();

    // Live GNU (emacs -Q --batch), pure-ASCII fixture:
    //   before modify-coding-system-alist -> undecided-unix
    //   after  modify-coding-system-alist -> utf-8-unix
    // scala-mode registers exactly such an entry from its autoloads
    // (scala-mode-autoloads.el:49), which is what promotes its .sbt and
    // .worksheet.sc buffers.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("promoted.sbt");
    fs::write(&path, b"ThisBuild / scalaVersion := \"2.13.16\"\n").expect("write ASCII fixture");
    let path_lisp = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let results = bootstrap_eval(&format!(
        r#"(list (with-temp-buffer
                   (insert-file-contents "{path_lisp}")
                   last-coding-system-used)
                 (let ((file-coding-system-alist
                        (cons (cons "\\.sbt\\'" (cons 'utf-8 'utf-8))
                              file-coding-system-alist)))
                   (with-temp-buffer
                     (insert-file-contents "{path_lisp}")
                     last-coding-system-used)))"#
    ));

    assert_eq!(results[0], "OK (undecided-unix utf-8-unix)");
}

#[test]
fn insert_file_contents_prefers_auto_coding_over_file_coding_system_alist() {
    crate::test_utils::init_test_tracing();

    // GNU asks `file-coding-system-alist' only when `set-auto-coding-function'
    // declined (src/fileio.c:4411-4420, :5057-5066), so a `coding:' cookie
    // outranks a matching alist entry.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("cookie.sbt");
    fs::write(
        &path,
        [
            b'%', b' ', b'-', b'*', b'-', b' ', b'c', b'o', b'd', b'i', b'n', b'g', b':', b' ',
            b'c', b'n', b'-', b'g', b'b', b'-', b'2', b'3', b'1', b'2', b' ', b'-', b'*', b'-',
            b'\n', 0xd2, 0xbb, b'\n',
        ],
    )
    .expect("write cookie fixture");
    let path_lisp = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let results = bootstrap_eval(&format!(
        r#"(let ((file-coding-system-alist
                  (cons (cons "\\.sbt\\'" (cons 'utf-8 'utf-8))
                        file-coding-system-alist)))
             (with-temp-buffer
               (insert-file-contents "{path_lisp}")
               last-coding-system-used))"#
    ));

    assert_eq!(results[0], "OK chinese-iso-8bit-unix");
}

#[test]
fn insert_file_contents_uses_set_auto_coding_function_for_coding_cookie() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("pinyin-cookie.map");
    fs::write(
        &path,
        [
            b'%', b' ', b'-', b'*', b'-', b' ', b'c', b'o', b'd', b'i', b'n', b'g', b':', b' ',
            b'c', b'n', b'-', b'g', b'b', b'-', b'2', b'3', b'1', b'2', b' ', b'-', b'*', b'-',
            b'\r', b'\n', 0xd2, 0xbb, b'\r', b'\n',
        ],
    )
    .expect("write GB2312 coding-cookie fixture");
    let path_lisp = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let results = bootstrap_eval(&format!(
        r#"(with-temp-buffer
             (insert-file-contents "{path_lisp}")
             (let ((codes (string-to-list (buffer-string))))
               (list last-coding-system-used
                     (not (null (memq 19968 codes)))
                     (null (memq 13 codes)))))"#
    ));

    assert_eq!(results[0], "OK (chinese-iso-8bit-dos t t)");
}

#[test]
fn insert_file_contents_empty_buffer_auto_coding_uses_current_buffer_like_gnu() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("empty-buffer-auto-coding.txt");
    fs::write(&path, b"alpha\n").expect("write fixture");
    let path_str = path.to_string_lossy().to_string();

    let mut eval = Context::new();
    eval.eval_str(
        r#"(progn
             (defalias 'neovm-test-set-auto-coding-function
               (lambda (_filename _size)
                 (setq neovm-test-auto-coding-buffer
                       (buffer-name (current-buffer)))
                 nil))
             (setq set-auto-coding-function
                   'neovm-test-set-auto-coding-function))"#,
    )
    .expect("install set-auto-coding-function probe");

    builtin_insert_file_contents(&mut eval, vec![Value::string(&path_str)])
        .expect("insert-file-contents should succeed");

    assert_eq!(
        eval.visible_variable_value_or_nil("neovm-test-auto-coding-buffer")
            .as_utf8_str(),
        Some("*scratch*")
    );
    assert!(
        eval.buffers
            .find_buffer_by_name(" *code-converting-work*")
            .is_none(),
        "GNU's empty-buffer insert-file-contents path does not create the work buffer"
    );
    assert_eq!(
        eval.buffers
            .current_buffer()
            .expect("current buffer")
            .buffer_string(),
        "alpha\n"
    );
}

#[test]
fn insert_file_contents_sets_last_coding_before_after_insert_file_set_coding() {
    crate::test_utils::init_test_tracing();

    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("neovm-test")
        .join(format!(
            "insert-file-coding-order-{}-{unique}",
            std::process::id()
        ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("dos.txt");
    fs::write(&path, b"alpha\r\nbeta\r\n").expect("write dos fixture");
    let path_str = path.to_string_lossy().to_string();

    let mut eval = Context::new();
    eval.eval_str(
        r#"(progn
             (setq neovm-seen-insert-file-coding nil)
             (defalias 'after-insert-file-set-coding
               (lambda (inserted visit)
                 (setq neovm-seen-insert-file-coding last-coding-system-used)
                 inserted)))"#,
    )
    .expect("install after-insert-file-set-coding probe");

    builtin_insert_file_contents(&mut eval, vec![Value::string(&path_str)])
        .expect("insert-file-contents should decode dos fixture");

    assert_eq!(
        format_eval_result(&eval.eval_str("neovm-seen-insert-file-coding")),
        "OK undecided-dos"
    );
    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "alpha\nbeta\n");
}

#[test]
fn decode_insert_file_contents_defaults_to_gnu_utf8_coding_for_non_ascii_text() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let decoded =
        super::decode_insert_file_contents(&mut eval, "alpha cafe\n".as_bytes(), true, None)
            .expect("decode utf-8 text");
    assert_eq!(decoded.text().as_utf8_str(), Some("alpha cafe\n"));
    assert_eq!(decoded.coding, "undecided-unix");

    let decoded =
        super::decode_insert_file_contents(&mut eval, "alpha caf\u{00E9}\n".as_bytes(), true, None)
            .expect("decode utf-8 accented text");
    assert_eq!(decoded.text().as_utf8_str(), Some("alpha caf\u{00E9}\n"));
    assert_eq!(decoded.coding, "utf-8-unix");
}

/// With no newline bytes, GNU's undecided decoder has no EOL evidence, so
/// `after-insert-file-set-coding` leaves the new buffer's default
/// `utf-8-unix` coding intact.  In particular, the EOL component is Unix (0),
/// not undecided/nil; this is the third `U` in the TTY mode-line `UUU`.
#[test]
fn find_file_ascii_without_newline_keeps_gnu_default_utf8_unix_eol() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("ascii-without-newline.txt");
    fs::write(&path, b"ab").expect("write no-newline ASCII fixture");
    let path_lisp = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let mut eval = crate::test_utils::runtime_startup_context();

    let result = eval
        .eval_str(&format!(
            r##"(progn
                   ;; `runtime_startup_context` ends at loadup.  Startup then
                   ;; derives UTF-8 from this test host's C.UTF-8 locale; mirror
                   ;; that GNU `-Q` state explicitly here.
                   (set-language-environment "UTF-8")
                   (let ((buffer (find-file-noselect "{path_lisp}")))
                     (with-current-buffer buffer
                       (list (default-value 'buffer-file-coding-system)
                             buffer-file-coding-system
                             (coding-system-eol-type
                              buffer-file-coding-system)))))"##
        ))
        .expect("visit no-newline ASCII file");

    assert_eq!(format!("{result}"), "(utf-8-unix utf-8-unix 0)");
}

/// An ASCII `.el` file with LF evidence is decoded as `utf-8-unix`, then GNU's
/// `after-insert-file-set-coding` preserves the file-alist preference by
/// publishing `prefer-utf-8-unix` on the visited buffer.  That coding system
/// deliberately declares the `-` mnemonic despite containing `utf-8` in its
/// name.
#[test]
fn find_file_ascii_elisp_with_newline_publishes_prefer_utf8_unix_coding() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("ascii-with-newline.el");
    fs::write(&path, b"(message \"ascii\")\n").expect("write ASCII fixture");
    let path_lisp = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let mut eval = crate::test_utils::runtime_startup_context();

    let result = eval
        .eval_str(&format!(
            r##"(progn
                   (set-language-environment "UTF-8")
                   (let ((buffer (find-file-noselect "{path_lisp}")))
                     (with-current-buffer buffer
                       (list last-coding-system-used
                             buffer-file-coding-system))))"##
        ))
        .expect("visit newline-terminated ASCII file");

    assert_eq!(format!("{result}"), "(prefer-utf-8-unix prefer-utf-8-unix)");
}

#[cfg(unix)]
#[test]
fn builtin_insert_file_contents_handles_raw_unibyte_filename() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = raw_temp_path(b"neovm-insert-file-\xFF");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let path = dir.join(std::ffi::OsStr::from_bytes(b"visit-\xFE"));
    fs::write(&path, b"raw file").unwrap();

    let mut eval = Context::new();
    let result = builtin_insert_file_contents(&mut eval, vec![raw_path_value(&path), Value::T])
        .expect("insert-file-contents should accept raw-byte filenames");
    let parts = list_to_vec(&result).expect("insert-file-contents should return list");
    assert_unibyte_string_bytes(parts[0], path.as_os_str().as_bytes());

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "raw file");
    assert_unibyte_string_bytes(buf.file_name_value(), path.as_os_str().as_bytes());
    assert!(!buf.is_modified());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_insert_file_contents_visit_rejects_partial_and_nonempty_visits() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = std::env::temp_dir().join("neovm_eval_insert_file_contents_visit_errors");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let path = dir.join("visit.txt");
    let path_str = path.to_string_lossy().to_string();
    write_string_to_file("visited text", &path_str, false).unwrap();

    let mut eval_partial = Context::new();
    let partial = builtin_insert_file_contents(
        &mut eval_partial,
        vec![Value::string(&path_str), Value::T, Value::fixnum(0)],
    )
    .expect_err("visit with BEG should reject");
    match partial {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Attempt to visit less than an entire file")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let mut eval_nonempty = Context::new();
    eval_nonempty
        .buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("x");
    let nonempty =
        builtin_insert_file_contents(&mut eval_nonempty, vec![Value::string(&path_str), Value::T])
            .expect_err("visit in non-empty buffer without replace should reject");
    match nonempty {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string(
                    "Cannot do file visiting in a non-empty buffer"
                )]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn insert_file_contents_visit_decodes_text_enriched_formats() {
    crate::test_utils::init_test_tracing();

    let dir = std::env::temp_dir().join("neovm_eval_insert_file_contents_text_enriched");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let path = dir.join("hello.enriched");
    fs::write(
        &path,
        concat!(
            "Content-Type: text/enriched\n",
            "\n",
            "<x-color><param>orange red</param>hello</x-color>\n",
        ),
    )
    .unwrap();

    let path_str = path.to_string_lossy().to_string();

    let mut eval = Context::new();
    eval.eval_str(
        r#"(progn
             (defalias 'format-decode
               (lambda (_format len _visit)
                 (delete-region (point-min) (point-max))
                 (insert "hello\n")
                 (setq buffer-file-format '(text/enriched))
                 6))
             (setq after-insert-file-functions
                   (list (lambda (len)
                           (setq enriched-mode t)
                           len))))"#,
    )
    .expect("stub format decode setup");

    builtin_insert_file_contents(&mut eval, vec![Value::string(&path_str), Value::T])
        .expect("insert-file-contents should decode text/enriched");

    assert_eq!(
        format_eval_result(&eval.eval_str("buffer-file-format")),
        "OK (text/enriched)"
    );
    assert_eq!(format_eval_result(&eval.eval_str("enriched-mode")), "OK t");
    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "hello\n");
    assert_eq!(
        buf.file_name_runtime_string_owned().as_deref(),
        Some(path_str.as_str())
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_insert_file_contents_beg_end_semantics() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = std::env::temp_dir().join("neovm_eval_insert_file_contents_beg_end");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("slice.txt");
    let path_str = path.to_string_lossy().to_string();
    write_string_to_file("abcdef", &path_str, false).unwrap();

    let mut eval_slice = Context::new();
    let inserted = builtin_insert_file_contents(
        &mut eval_slice,
        vec![
            Value::string(&path_str),
            Value::NIL,
            Value::fixnum(2),
            Value::fixnum(4),
        ],
    )
    .expect("insert-file-contents 2..4 should succeed");
    assert_eq!(
        list_to_vec(&inserted).unwrap()[1],
        Value::fixnum(2),
        "inserted char count should match slice length"
    );
    assert_eq!(
        eval_slice.buffers.current_buffer().unwrap().buffer_string(),
        "cd",
        "slice 2..4 should insert 'cd'"
    );

    let mut eval_empty = Context::new();
    let inserted_zero = builtin_insert_file_contents(
        &mut eval_empty,
        vec![
            Value::string(&path_str),
            Value::NIL,
            Value::fixnum(4),
            Value::fixnum(2),
        ],
    )
    .expect("insert-file-contents start>end should succeed with empty insertion");
    assert_eq!(list_to_vec(&inserted_zero).unwrap()[1], Value::fixnum(0));
    assert_eq!(
        eval_empty.buffers.current_buffer().unwrap().buffer_string(),
        ""
    );

    let mut eval_tail = Context::new();
    let inserted_tail = builtin_insert_file_contents(
        &mut eval_tail,
        vec![
            Value::string(&path_str),
            Value::NIL,
            Value::fixnum(2),
            Value::fixnum(99),
        ],
    )
    .expect("insert-file-contents end beyond file should clamp");
    assert_eq!(list_to_vec(&inserted_tail).unwrap()[1], Value::fixnum(4));
    assert_eq!(
        eval_tail.buffers.current_buffer().unwrap().buffer_string(),
        "cdef"
    );

    let mut eval_bad = Context::new();
    let bad_offset = builtin_insert_file_contents(
        &mut eval_bad,
        vec![
            Value::string(&path_str),
            Value::NIL,
            Value::fixnum(-1),
            Value::fixnum(2),
        ],
    )
    .expect_err("negative BEG should reject with file-offset predicate");
    match bad_offset {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("file-offset"), Value::fixnum(-1)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_insert_file_contents_and_write_region_arity_bounds() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = std::env::temp_dir().join("neovm_eval_fileio_arity_bounds");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let file_path = dir.join("arity.txt");
    let file_str = file_path.to_string_lossy().to_string();
    write_string_to_file("", &file_str, false).unwrap();

    let mut eval_insert_ok = Context::new();
    let insert_ok = builtin_insert_file_contents(
        &mut eval_insert_ok,
        vec![
            Value::string(&file_str),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect("5-arg insert-file-contents should succeed");
    assert_eq!(list_to_vec(&insert_ok).unwrap()[1], Value::fixnum(0));

    let mut eval_insert_bad = Context::new();
    let insert_bad = builtin_insert_file_contents(
        &mut eval_insert_bad,
        vec![
            Value::string(&file_str),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect_err("6-arg insert-file-contents should fail");
    match insert_bad {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("insert-file-contents"), Value::fixnum(6)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let out_path = dir.join("arity-out.txt");
    let out_str = out_path.to_string_lossy().to_string();

    let mut eval_write_ok = Context::new();
    eval_write_ok
        .buffers
        .current_buffer_mut()
        .unwrap()
        .insert("x");
    builtin_write_region(
        &mut eval_write_ok,
        vec![
            Value::NIL,
            Value::NIL,
            Value::string(&out_str),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect("7-arg write-region should succeed");

    let mut eval_write_bad = Context::new();
    eval_write_bad
        .buffers
        .current_buffer_mut()
        .unwrap()
        .insert("x");
    let write_bad = builtin_write_region(
        &mut eval_write_bad,
        vec![
            Value::NIL,
            Value::NIL,
            Value::string(&out_str),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect_err("8-arg write-region should fail");
    match write_bad {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("write-region"), Value::fixnum(8)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_find_file_noselect_arity_bounds() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = std::env::temp_dir().join("neovm_eval_find_file_noselect_arity");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let file_path = dir.join("arity.txt");
    let file_str = file_path.to_string_lossy().to_string();
    write_string_to_file("", &file_str, false).unwrap();

    let mut eval_ok = Context::new();
    let ok = builtin_find_file_noselect(
        &mut eval_ok,
        vec![Value::string(&file_str), Value::NIL, Value::NIL, Value::NIL],
    )
    .expect("4-arg find-file-noselect should succeed");
    assert!(ok.is_buffer());

    let mut eval_bad = Context::new();
    let bad = builtin_find_file_noselect(
        &mut eval_bad,
        vec![
            Value::string(&file_str),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect_err("5-arg find-file-noselect should fail");
    match bad {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("find-file-noselect"), Value::fixnum(5)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_eval_fileio_relative_paths_respect_default_directory() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = std::env::temp_dir().join("neovm_eval_fileio_relative");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let alpha_path = dir.join("alpha.txt");
    fs::write(&alpha_path, "alpha\n").unwrap();
    let alpha_str = alpha_path.to_string_lossy().to_string();
    let out_path = dir.join("out.txt");
    let out_str = out_path.to_string_lossy().to_string();
    let default_dir = format!("{}/", dir.to_string_lossy());

    let mut eval_insert = Context::new();
    eval_insert.set_variable("default-directory", Value::string(&default_dir));
    let inserted =
        builtin_insert_file_contents(&mut eval_insert, vec![Value::string("alpha.txt")]).unwrap();
    let inserted_parts = list_to_vec(&inserted).unwrap();
    assert_eq!(inserted_parts[0].as_utf8_str(), Some(alpha_str.as_str()));
    let ibuf = eval_insert.buffers.current_buffer().unwrap();
    assert_eq!(ibuf.buffer_string(), "alpha\n");

    let mut eval_write = Context::new();
    eval_write.set_variable("default-directory", Value::string(&default_dir));
    eval_write
        .buffers
        .current_buffer_mut()
        .unwrap()
        .insert("neo");
    builtin_write_region(
        &mut eval_write,
        vec![Value::NIL, Value::NIL, Value::string("out.txt")],
    )
    .unwrap();
    assert_eq!(read_file_contents(&out_str).unwrap(), "neo");

    let mut eval_find = Context::new();
    eval_find.set_variable("default-directory", Value::string(&default_dir));
    let found =
        builtin_find_file_noselect(&mut eval_find, vec![Value::string("alpha.txt")]).unwrap();
    if !found.is_buffer() {
        panic!("expected Buffer");
    };
    let buf_id = found.as_buffer_id().unwrap();
    let fbuf = eval_find.buffers.get(buf_id).unwrap();
    assert_eq!(fbuf.buffer_string(), "alpha\n");
    assert_eq!(
        fbuf.file_name_runtime_string_owned().as_deref(),
        Some(alpha_str.as_str())
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_write_region_bounds_and_order_semantics() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = std::env::temp_dir().join("neovm_eval_write_region_bounds");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let out_path = dir.join("out.txt");
    let out_str = out_path.to_string_lossy().to_string();

    let mut eval = Context::new();
    eval.buffers.current_buffer_mut().unwrap().insert("abc");
    let current = Value::make_buffer(eval.buffers.current_buffer().unwrap().id);

    builtin_write_region(
        &mut eval,
        vec![Value::fixnum(3), Value::fixnum(1), Value::string(&out_str)],
    )
    .expect("write-region should accept reversed in-range bounds");
    assert_eq!(read_file_contents(&out_str).unwrap(), "ab");

    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    let marker_start = crate::emacs_core::marker::make_marker_value(
        Some(buffer_id),
        Some(crate::buffer::LispCharPos1::new(2)),
        false,
    );
    let marker_end = crate::emacs_core::marker::make_marker_value(
        Some(buffer_id),
        Some(crate::buffer::LispCharPos1::new(4)),
        false,
    );
    builtin_write_region(
        &mut eval,
        vec![marker_start, marker_end, Value::string(&out_str)],
    )
    .expect("write-region should accept marker bounds");
    assert_eq!(read_file_contents(&out_str).unwrap(), "bc");

    for (start, end) in [(-1, 2), (1, -1), (1, 9)] {
        let err = builtin_write_region(
            &mut eval,
            vec![
                Value::fixnum(start),
                Value::fixnum(end),
                Value::string(&out_str),
            ],
        )
        .expect_err("out-of-range bounds should signal");
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "args-out-of-range");
                assert_eq!(
                    sig.data,
                    vec![current, Value::fixnum(start), Value::fixnum(end)]
                );
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_write_region_mustbenew_excl_semantics() {
    // GNU `Fwrite_region` MUSTBENEW handling (src/fileio.c):
    //   open_flags |= EQ (mustbenew, Qexcl) ? O_EXCL : ...
    // When MUSTBENEW is `excl` and the file already exists, the O_EXCL open
    // fails with EEXIST, which `get_file_errno_data` turns into
    //   (file-already-exists "File exists" FILENAME).
    // When the file does not exist, the write succeeds normally.
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = std::env::temp_dir().join("neovm_eval_write_region_excl");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    // --- excl on a NON-existent file: succeeds and writes content. ---
    let new_path = dir.join("brand-new.txt");
    let new_str = new_path.to_string_lossy().to_string();
    let mut eval = Context::new();
    eval.buffers.current_buffer_mut().unwrap().insert("hi");
    builtin_write_region(
        &mut eval,
        vec![
            Value::NIL,
            Value::NIL,
            Value::string(&new_str),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::symbol("excl"),
        ],
    )
    .expect("excl write-region to a new file should succeed");
    assert_eq!(read_file_contents(&new_str).unwrap(), "hi");

    // --- excl on an EXISTING file: signals file-already-exists. ---
    let err = builtin_write_region(
        &mut eval,
        vec![
            Value::NIL,
            Value::NIL,
            Value::string(&new_str),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::symbol("excl"),
        ],
    )
    .expect_err("excl write-region to an existing file should signal");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "file-already-exists");
            // GNU data: (file-already-exists "File exists" FILENAME)
            assert_eq!(
                sig.data,
                vec![Value::string("File exists"), Value::string(&new_str)],
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
    // The pre-existing content must be untouched by the failed excl write.
    assert_eq!(read_file_contents(&new_str).unwrap(), "hi");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_write_region_visit_sets_file_name_and_clears_modified() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = std::env::temp_dir().join("neovm_eval_write_region_visit");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let out_path = dir.join("visited.txt");
    let out_str = out_path.to_string_lossy().to_string();

    let mut eval = Context::new();
    eval.set_variable("noninteractive", Value::NIL);
    eval.buffers.current_buffer_mut().unwrap().insert("neo");
    assert!(eval.buffers.current_buffer().unwrap().is_modified());

    builtin_write_region(
        &mut eval,
        vec![
            Value::NIL,
            Value::NIL,
            Value::string(&out_str),
            Value::NIL,
            Value::T,
        ],
    )
    .expect("write-region with visit should succeed");

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(
        buf.file_name_runtime_string_owned().as_deref(),
        Some(out_str.as_str())
    );
    assert!(
        buf.buffer_local_value("buffer-file-truename")
            .unwrap_or(Value::NIL)
            .is_nil()
    );
    assert!(!buf.is_modified());
    assert_eq!(read_file_contents(&out_str).unwrap(), "neo");
    let expected_message = format!("Wrote {}", out_str);
    assert_eq!(
        eval.current_message_text().as_deref(),
        Some(expected_message.as_str())
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn write_region_reports_nonvisiting_interactive_operation() {
    crate::test_utils::init_test_tracing();

    let dir =
        std::env::temp_dir().join(format!("neovm_write_region_report_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    for (label, append, initial, expected_prefix) in [
        ("truncate", Value::NIL, "old", "Wrote "),
        ("append", Value::T, "old", "Added to "),
        ("seek", Value::fixnum(1), "old", "Updated "),
    ] {
        let out_path = dir.join(format!("{label}.txt"));
        let out_str = out_path.to_string_lossy().to_string();
        write_string_to_file(initial, &out_str, false).unwrap();

        let mut eval = Context::new();
        eval.set_variable("noninteractive", Value::NIL);
        eval.buffers.current_buffer_mut().unwrap().insert("neo");
        builtin_write_region(
            &mut eval,
            vec![
                Value::NIL,
                Value::NIL,
                Value::string(&out_str),
                append,
                Value::NIL,
            ],
        )
        .expect("non-visiting write-region should succeed");

        assert_eq!(
            eval.current_message_text().as_deref(),
            Some(format!("{expected_prefix}{out_str}").as_str()),
            "GNU reports the completed write operation even when VISIT is nil"
        );
        let buffer = eval.buffers.current_buffer().expect("current buffer");
        assert!(
            buffer.file_name_value().is_nil(),
            "VISIT=nil must not make the buffer visit the output"
        );
        assert!(
            buffer.is_modified(),
            "VISIT=nil must not mark the source buffer saved"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn write_region_unlocks_a_successfully_saved_file() {
    crate::test_utils::init_test_tracing();

    let dir =
        std::env::temp_dir().join(format!("neovm_write_region_unlock_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let out_path = dir.join("visited.txt");
    let lock_path = dir.join(".#visited.txt");
    let out_str = out_path.to_string_lossy().to_string();

    let mut eval = Context::new();
    eval.set_variable("create-lockfiles", Value::T);
    eval.buffers.current_buffer_mut().unwrap().insert("neo");
    crate::emacs_core::filelock::builtin_lock_file(&mut eval, vec![Value::string(&out_str)])
        .expect("first modification should lock the visited file");
    assert!(
        fs::symlink_metadata(&lock_path).is_ok(),
        "precondition: a dangling Emacs lock symlink exists"
    );

    builtin_write_region(
        &mut eval,
        vec![
            Value::NIL,
            Value::NIL,
            Value::string(&out_str),
            Value::NIL,
            Value::T,
        ],
    )
    .expect("write-region with visit should save the buffer");

    assert!(
        matches!(
            fs::symlink_metadata(&lock_path),
            Err(error) if error.kind() == io::ErrorKind::NotFound
        ),
        "GNU unlocks LOCKNAME after write-region closes the file"
    );
    assert!(!eval.buffers.current_buffer().unwrap().is_modified());

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn write_region_unlocks_explicit_lockname_after_open_error() {
    crate::test_utils::init_test_tracing();

    let dir = std::env::temp_dir().join(format!(
        "neovm_write_region_error_unlock_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let out_path = dir.join("missing-parent/out.txt");
    let lock_target = dir.join("logical-visit.txt");
    let lock_path = dir.join(".#logical-visit.txt");
    let out_str = out_path.to_string_lossy().to_string();
    let lock_target_str = lock_target.to_string_lossy().to_string();

    let mut eval = Context::new();
    eval.set_variable("create-lockfiles", Value::T);
    eval.buffers.current_buffer_mut().unwrap().insert("neo");
    crate::emacs_core::filelock::builtin_lock_file(
        &mut eval,
        vec![Value::string(&lock_target_str)],
    )
    .expect("pre-existing explicit lock");
    assert!(fs::symlink_metadata(&lock_path).is_ok());

    builtin_write_region(
        &mut eval,
        vec![
            Value::NIL,
            Value::NIL,
            Value::string(&out_str),
            Value::NIL,
            Value::NIL,
            Value::string(&lock_target_str),
        ],
    )
    .expect_err("opening an output below a missing directory must fail");

    assert!(
        matches!(
            fs::symlink_metadata(&lock_path),
            Err(error) if error.kind() == io::ErrorKind::NotFound
        ),
        "GNU unlocks an explicit LOCKNAME when write-region fails after locking"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_write_region_visit_updates_recorded_modtime_and_size() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = std::env::temp_dir().join("neovm_write_region_modtime");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let out_path = dir.join("visited.txt");
    fs::write(&out_path, b"a").unwrap();
    let out_str = out_path.to_string_lossy().to_string();

    let mut eval = Context::new();
    let current = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .set_buffer_file_name(current, Value::string(&out_str))
        .expect("set visited file");
    builtin_set_visited_file_modtime(&mut eval, vec![Value::NIL])
        .expect("record initial visited file modtime");

    eval.buffers.current_buffer_mut().unwrap().insert("a\nb\n");
    builtin_write_region(
        &mut eval,
        vec![
            Value::NIL,
            Value::NIL,
            Value::string(&out_str),
            Value::NIL,
            Value::T,
        ],
    )
    .expect("write-region with visit should succeed");

    assert_eq!(
        builtin_verify_visited_file_modtime(&mut eval, vec![])
            .expect("verify-visited-file-modtime after visiting write"),
        Value::T
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_write_region_string_start_numeric_append_and_visit_string_semantics() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = std::env::temp_dir().join("neovm_eval_write_region_string_append");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let out_path = dir.join("out.txt");
    let out_str = out_path.to_string_lossy().to_string();
    let visit_path = dir.join("visit.txt");
    let visit_str = visit_path.to_string_lossy().to_string();
    write_string_to_file("abcde", &out_str, false).unwrap();

    let mut eval = Context::new();
    eval.buffers
        .current_buffer_mut()
        .unwrap()
        .insert("buffer text");
    assert!(eval.buffers.current_buffer().unwrap().is_modified());

    builtin_write_region(
        &mut eval,
        vec![
            Value::string("XY"),
            Value::NIL,
            Value::string(&out_str),
            Value::fixnum(2),
            Value::string(&visit_str),
        ],
    )
    .expect("write-region string start with numeric append should succeed");

    assert_eq!(read_file_contents(&out_str).unwrap(), "abXYe");
    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(
        buf.file_name_runtime_string_owned().as_deref(),
        Some(visit_str.as_str())
    );
    assert!(!buf.is_modified());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_write_region_preserves_unibyte_raw_bytes() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = std::env::temp_dir().join("neovm_eval_write_region_raw_bytes");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let out_path = dir.join("buffer.bin");
    let out_str = out_path.to_string_lossy().to_string();
    let out2_path = dir.join("string.bin");
    let out2_str = out2_path.to_string_lossy().to_string();

    let mut eval = Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.set_multibyte_value(false);
        buf.insert_lisp_string(&crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    }

    builtin_write_region(
        &mut eval,
        vec![Value::NIL, Value::NIL, Value::string(&out_str)],
    )
    .expect("write-region should preserve raw buffer bytes");
    assert_eq!(fs::read(&out_path).unwrap(), vec![0xFF]);

    builtin_write_region(
        &mut eval,
        vec![
            Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
                0xFE, 0xFF,
            ])),
            Value::NIL,
            Value::string(&out2_str),
        ],
    )
    .expect("write-region string payload should preserve raw bytes");
    assert_eq!(fs::read(&out2_path).unwrap(), vec![0xFE, 0xFF]);

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn builtin_write_region_handles_raw_unibyte_filename_and_visit() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = raw_temp_path(b"neovm-write-region-\xFF");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let out_path = dir.join(std::ffi::OsStr::from_bytes(b"buffer-\xFE"));

    let mut eval = Context::new();
    eval.set_variable("make-backup-files", Value::NIL);
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.set_multibyte_value(false);
        buf.insert_lisp_string(&crate::heap_types::LispString::from_unibyte(vec![
            0xFF, b'A',
        ]));
    }

    builtin_write_region(
        &mut eval,
        vec![
            Value::NIL,
            Value::NIL,
            raw_path_value(&out_path),
            Value::NIL,
            Value::T,
        ],
    )
    .expect("write-region should accept raw-byte filenames");

    assert_eq!(fs::read(&out_path).unwrap(), vec![0xFF, b'A']);
    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_unibyte_string_bytes(buf.file_name_value(), out_path.as_os_str().as_bytes());
    assert!(!buf.is_modified());

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn builtin_write_region_does_not_create_backup_for_raw_unibyte_filename() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = raw_temp_path(b"neovm-write-region-no-backup-\xFF");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let out_path = dir.join(std::ffi::OsStr::from_bytes(b"buffer-\xFE"));
    fs::write(&out_path, b"old bytes").unwrap();
    let backup_path = dir.join(std::ffi::OsStr::from_bytes(b"buffer-\xFE~"));

    let mut eval = Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.set_multibyte_value(false);
        buf.insert_lisp_string(&crate::heap_types::LispString::from_unibyte(vec![
            0xFF, b'A',
        ]));
    }

    builtin_write_region(
        &mut eval,
        vec![
            Value::NIL,
            Value::NIL,
            raw_path_value(&out_path),
            Value::NIL,
            Value::T,
        ],
    )
    .expect("write-region should not run save-buffer backup logic");

    assert_eq!(fs::read(&out_path).unwrap(), vec![0xFF, b'A']);
    assert!(
        !backup_path.exists(),
        "GNU write-region does not create backup files; backup-buffer is part of save-buffer"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_find_file_noselect() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = std::env::temp_dir().join("neovm_findfile_test");
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("findme.txt");
    let path_str = path.to_string_lossy().to_string();

    write_string_to_file("file content here", &path_str, false).unwrap();

    let mut eval = Context::new();

    // find-file-noselect
    let result = builtin_find_file_noselect(&mut eval, vec![Value::string(&path_str)]);
    assert!(result.is_ok());
    let buf_val = result.unwrap();
    match buf_val.kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let buf = eval.buffers.get(buf_val.as_buffer_id().unwrap()).unwrap();
            assert_eq!(buf.buffer_string(), "file content here");
            assert!(buf.file_name_value().is_string());
            assert!(!buf.is_modified());
            assert!(buf.get_undo_list().is_nil());
        }
        _other => panic!("Expected Buffer, got {:?}", buf_val),
    }

    // Calling again with the same file should return the same buffer
    let result2 = builtin_find_file_noselect(&mut eval, vec![Value::string(&path_str)]);
    assert!(result2.is_ok());
    let buf_val2 = result2.unwrap();
    assert!(buf_val.is_buffer() && buf_val2.is_buffer());
    assert_eq!(buf_val, buf_val2);

    // Clean up
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn builtin_find_file_noselect_handles_raw_unibyte_filename() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = raw_temp_path(b"neovm-find-file-\xFF");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let path = dir.join(std::ffi::OsStr::from_bytes(b"find-\xFE"));
    fs::write(&path, b"file content here").unwrap();

    let mut eval = Context::new();
    let result = builtin_find_file_noselect(&mut eval, vec![raw_path_value(&path)])
        .expect("find-file-noselect should accept raw-byte filename");
    let buf_id = result.as_buffer_id().expect("buffer result");
    let buf = eval.buffers.get(buf_id).expect("buffer");
    assert_eq!(buf.buffer_string(), "file content here");
    assert_unibyte_string_bytes(buf.file_name_value(), path.as_os_str().as_bytes());

    let result2 = builtin_find_file_noselect(&mut eval, vec![raw_path_value(&path)])
        .expect("repeat raw find-file-noselect");
    assert_eq!(result, result2);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn bootstrap_find_file_noselect_applies_footer_local_variables() {
    crate::test_utils::init_test_tracing();
    use super::super::load::create_bootstrap_evaluator_cached;

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("locals.txt");
    fs::write(
        &path,
        "headline\n\n\
         ;; Local Variables:\n\
         ;; tab-width: 42\n\
         ;; End:\n",
    )
    .expect("write local-vars fixture");

    let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap evaluator");
    let path_str = path.to_string_lossy().to_string();
    let rendered = format_eval_result(&eval.eval_str(&format!(
        r#"(let ((buf (find-file-noselect {:?})))
                 (with-current-buffer buf
                   (list tab-width
                         (local-variable-p 'tab-width (current-buffer))
                         default-directory)))"#,
        path_str
    )));

    let expected_dir = format!("{}/", dir.path().to_string_lossy());
    assert_eq!(rendered, format!("OK (42 t {expected_dir:?})"));
}

#[test]
fn bootstrap_find_file_noselect_runs_find_file_hook() {
    crate::test_utils::init_test_tracing();
    use super::super::load::create_bootstrap_evaluator_cached;

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("hook.txt");
    fs::write(&path, "hook body\n").expect("write hook fixture");

    let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap evaluator");
    let path_str = path.to_string_lossy().to_string();
    let rendered = format_eval_result(&eval.eval_str(&format!(
        r#"(let ((find-file-hook (list (lambda () (setq-local neovm-find-file-hook-ran t)))))
                 (let ((buf (find-file-noselect {:?})))
                   (with-current-buffer buf
                     (list (bound-and-true-p neovm-find-file-hook-ran)
                           buffer-file-name))))"#,
        path_str
    )));

    assert_eq!(rendered, format!("OK (t {path_str:?})"));
}

#[test]
fn bootstrap_find_file_noselect_undo_preserves_visited_file_contents() {
    crate::test_utils::init_test_tracing();
    use super::super::load::create_bootstrap_evaluator_cached;

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("undo-visit.txt");
    fs::write(&path, "alpha line\n").expect("write undo fixture");

    let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap evaluator");
    let path_str = path.to_string_lossy().to_string();
    let rendered = format_eval_result(&eval.eval_str(&format!(
        r#"(let ((buf (find-file-noselect {:?})))
             (with-current-buffer buf
               (goto-char (point-max))
               (insert "omega line")
               (condition-case _err
                   (undo)
                 (error nil))
               ;; GNU's first-change entry is `(t . VISITED-FILE-MODTIME)`
               ;; (`src/undo.c:209-223`), so its datum varies per run.  Fold it
               ;; to whether it is the modtime `primitive-undo` will compare it
               ;; against (`lisp/simple.el:3669-3688`); GNU prints `(t . t)`.
               (list (buffer-string)
                     pending-undo-list
                     (mapcar (lambda (entry)
                               (if (and (consp entry) (eq (car entry) t))
                                   (cons t (equal (cdr entry)
                                                  (visited-file-modtime)))
                                 entry))
                             buffer-undo-list))))"#,
        path_str
    )));

    assert_eq!(
        rendered,
        r#"OK ("alpha line
" t (("omega line" . 12) (12 . 22) (t . t)))"#
    );
}

#[cfg(unix)]
#[test]
fn builtin_do_auto_save_preserves_raw_unibyte_filename_and_bytes() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = raw_temp_path(b"neovm-auto-save-\xFF");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let visited_path = dir.join(std::ffi::OsStr::from_bytes(b"visited-\xFE"));
    let auto_path = dir.join(std::ffi::OsStr::from_bytes(b"#visited-\xFE#"));

    let mut eval = Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.set_multibyte_value(false);
        buf.set_file_name_value(raw_path_value(&visited_path));
        buf.insert_lisp_string(&crate::heap_types::LispString::from_unibyte(vec![
            0xFF, b'A',
        ]));
    }
    // `do-auto-save' names the buffer itself when it has no auto-save name;
    // `make-auto-save-file-name' is Lisp (lisp/files.el:7699) and cannot be
    // called here (DIVERGENCES.md 152).
    builtin_do_auto_save(&mut eval, vec![]).expect("do-auto-save should preserve raw filenames");

    assert_eq!(fs::read(&auto_path).unwrap(), vec![0xFF, b'A']);
    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_unibyte_string_bytes(
        buf.auto_save_file_name_value(),
        auto_path.as_os_str().as_bytes(),
    );
    assert_eq!(
        buf.buffer_local_value("buffer-saved-size"),
        Some(Value::fixnum(2))
    );

    let _ = fs::remove_dir_all(&dir);
}

/// GNU `Fdo_auto_save` runs `auto-save-hook` before it snapshots any
/// buffers.  Hook changes therefore belong to the auto-save image produced by
/// the same call, rather than being deferred until a later auto-save.
#[test]
fn builtin_do_auto_save_runs_auto_save_hook_before_writing_buffer() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let dir = tempfile::tempdir().expect("tempdir");
    let visited_path = dir.path().join("visited.txt");
    let auto_path = dir.path().join("#visited.txt#");

    let mut eval = Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.set_file_name_value(Value::string(visited_path.to_string_lossy()));
        let auto_name = Value::string(auto_path.to_string_lossy());
        buf.set_buffer_local("buffer-auto-save-file-name", auto_name);
        buf.set_auto_save_file_name_value(auto_name);
        buf.insert("before\n");
    }
    eval.eval_str(
        r#"(progn
             (setq neo-auto-save-hook-inhibit-quit nil)
             (setq auto-save-hook
                   (list (lambda ()
                           (setq neo-auto-save-hook-inhibit-quit inhibit-quit)
                           (goto-char (point-max))
                           (insert "from-hook\n")))))"#,
    )
    .expect("install auto-save-hook");

    eval.eval_str("(do-auto-save nil t)")
        .expect("do-auto-save should run its hook and write the current buffer");

    assert_eq!(
        fs::read_to_string(&auto_path).expect("read auto-save image"),
        "before\nfrom-hook\n",
        "auto-save image must include changes made by auto-save-hook"
    );
    assert_eq!(
        eval.eval_symbol("neo-auto-save-hook-inhibit-quit")
            .expect("hook should record inhibit-quit"),
        Value::T,
        "GNU safe hook execution dynamically binds inhibit-quit"
    );
}

#[test]
fn test_find_file_noselect_nonexistent() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;
    use crate::emacs_core::value::{ValueKind, VecLikeType};

    let mut eval = Context::new();
    let result = builtin_find_file_noselect(
        &mut eval,
        vec![Value::string("/tmp/neovm_nonexistent_file_xyz.txt")],
    );
    assert!(result.is_ok());
    let nonexistent_buf = result.unwrap();
    match nonexistent_buf.kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let buf = eval
                .buffers
                .get(nonexistent_buf.as_buffer_id().unwrap())
                .unwrap();
            // Buffer should be empty for a nonexistent file
            assert_eq!(buf.buffer_string(), "");
            assert!(buf.file_name_value().is_string());
        }
        _other => panic!("Expected Buffer, got {:?}", nonexistent_buf),
    }
}

#[test]
fn file_local_name_bootstrap_matches_gnu_files_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (subrp (symbol-function 'file-local-name))
        (file-local-name "/tmp/local")
        (file-local-name "/ssh:user@host#22:/tmp/file")
        "#,
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], r#"OK "/tmp/local""#);
    assert_eq!(results[2], r#"OK "/tmp/file""#);
}

#[test]
fn file_local_name_bootstrap_error_shapes_match_gnu_files_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (condition-case err (file-local-name nil) (error (car err)))
        "#,
    );
    assert_eq!(results[0], "OK wrong-type-argument");
}

// #189: cross-device `rename-file` fallback must handle directories and
// symlinks, not just regular files (GNU `Frename_file` EXDEV path).
#[test]
fn rename_by_copy_delete_moves_a_directory_tree() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("src");
    std::fs::create_dir_all(src.join("sub")).unwrap();
    std::fs::write(src.join("a.txt"), "hello").unwrap();
    std::fs::write(src.join("sub/b.txt"), "nested").unwrap();
    let dst = tmp.path().join("dst");
    rename_regular_file_by_copy_delete(&src, &dst, false).expect("dir move");
    assert!(!src.exists(), "source directory removed");
    assert_eq!(std::fs::read_to_string(dst.join("a.txt")).unwrap(), "hello");
    assert_eq!(
        std::fs::read_to_string(dst.join("sub/b.txt")).unwrap(),
        "nested"
    );
}

#[cfg(unix)]
#[test]
fn rename_by_copy_delete_recreates_a_symlink() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("target.txt");
    std::fs::write(&target, "data").unwrap();
    let src = tmp.path().join("link");
    std::os::unix::fs::symlink(&target, &src).unwrap();
    let dst = tmp.path().join("moved-link");
    rename_regular_file_by_copy_delete(&src, &dst, false).expect("symlink move");
    assert!(!src.exists(), "source symlink removed");
    assert_eq!(std::fs::read_link(&dst).unwrap(), target);
}

/// Task #26: the `directory-files` MATCH argument runs GNU
/// `fast_string_match_internal` (`src/dired.c:311`), which arms
/// `re_match_object` = the file name and `RE_SETUP_SYNTAX_TABLE_FOR_OBJECT`
/// (`src/syntax.c:277`): `\sw` classifies by the CURRENT BUFFER's syntax
/// table. Measured GNU 31 (buffer-local copy of the standard table, `?z`
/// made whitespace, files abc + zzz): → ("abc").
#[test]
fn directory_files_match_reads_current_buffer_syntax_table() {
    crate::test_utils::init_test_tracing();
    let dir = std::env::temp_dir().join("neovm_dirfiles_syntax_table");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let dir_str = dir.to_string_lossy().to_string();
    fs::write(dir.join("abc"), "").unwrap();
    fs::write(dir.join("zzz"), "").unwrap();

    let mut eval = Context::new();
    eval.eval_str("(set-syntax-table (copy-syntax-table (standard-syntax-table)))")
        .expect("set-syntax-table");
    eval.eval_str("(modify-syntax-entry ?z \" \")")
        .expect("modify-syntax-entry");
    let result = builtin_directory_files(
        &mut eval,
        vec![
            Value::string(&dir_str),
            Value::NIL,
            Value::string("\\`\\sw+\\'"),
        ],
    )
    .unwrap();
    let names: Vec<String> = list_to_vec(&result)
        .expect("file list")
        .iter()
        .map(|v| v.as_utf8_str().unwrap().to_string())
        .collect();
    LAST_TEST_CTX.with(|slot| slot.borrow_mut().push(eval));
    assert_eq!(
        names,
        vec!["abc"],
        "directory-files MATCH must consult the buffer's syntax table"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Task #26: GNU `Ffind_file_name_handler` (`src/fileio.c:411`) matches
/// `file-name-handler-alist` regexps with `fast_string_match`, under the
/// current buffer's syntax table. Measured GNU 31 (buffer-local table, `?z`
/// made whitespace, alist ("\\`\\sw+\\'" . my-handler)): "abc" → my-handler,
/// "zzz" → nil.
#[test]
fn find_file_name_handler_reads_current_buffer_syntax_table() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str("(set-syntax-table (copy-syntax-table (standard-syntax-table)))")
        .expect("set-syntax-table");
    eval.eval_str("(modify-syntax-entry ?z \" \")")
        .expect("modify-syntax-entry");
    eval.obarray.set_symbol_value(
        "file-name-handler-alist",
        Value::list(vec![Value::cons(
            Value::string("\\`\\sw+\\'"),
            Value::symbol("my-handler"),
        )]),
    );

    let hit = builtin_find_file_name_handler(
        &mut eval,
        vec![Value::string("abc"), Value::symbol("insert-file-contents")],
    )
    .unwrap();
    let miss = builtin_find_file_name_handler(
        &mut eval,
        vec![Value::string("zzz"), Value::symbol("insert-file-contents")],
    )
    .unwrap();
    let hit_name = hit.as_symbol_name().map(|name| name.to_string());
    LAST_TEST_CTX.with(|slot| slot.borrow_mut().push(eval));
    assert_eq!(
        hit_name.as_deref(),
        Some("my-handler"),
        "handler regexp \\sw must match abc under the buffer table"
    );
    assert!(
        miss.is_nil(),
        "zzz must not match \\sw+ once ?z is whitespace in the buffer table"
    );
}

/// GNU `Fverify_visited_file_modtime` (fileio.c:6129) decodes BUF and tests
/// THAT buffer's recorded modtime.  `lock_file` (filelock.c:605) passes the
/// buffer visiting the file being locked, which need not be the current one,
/// so ignoring BUF and always reading the current buffer is a real divergence.
#[test]
fn verify_visited_file_modtime_uses_its_buffer_argument_like_gnu() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let visited = dir.path().join("note.txt");
    std::fs::write(&visited, b"hello\n").expect("write visited file");

    let mut eval = Context::new();
    let original = eval.buffers.current_buffer_id().expect("current buffer");
    let visitor_value = eval
        .eval_str(r#"(get-buffer-create "visitor")"#)
        .expect("create the visiting buffer");
    let visitor_id = visitor_value.as_buffer_id().expect("buffer value");
    eval.buffers
        .set_buffer_file_name(visitor_id, Value::string(visited.to_string_lossy()))
        .expect("set buffer-file-name");
    eval.eval_str(r#"(set-buffer "visitor")"#)
        .expect("select the visiting buffer");
    eval.eval_str("(set-visited-file-modtime '(0 0))")
        .expect("record a stale modtime");

    // The visiting buffer is current and its recorded modtime is stale, so
    // an omitted BUF answers nil...
    let current_answer = builtin_verify_visited_file_modtime(&mut eval, vec![])
        .expect("verify-visited-file-modtime with no BUF");
    assert!(
        current_answer.is_nil(),
        "the current buffer's modtime is stale"
    );
    // ...while BUF naming a buffer that visits nothing answers t.  Ignoring
    // BUF and re-reading the current buffer would answer nil here.
    let other_answer =
        builtin_verify_visited_file_modtime(&mut eval, vec![Value::make_buffer(original)])
            .expect("verify-visited-file-modtime with BUF");
    assert_eq!(
        other_answer,
        Value::T,
        "BUF, not the current buffer, decides the answer"
    );
}

/// The other half of GNU's Bug#56397 change ("Fix undo of changes in cloned
/// indirect buffers", commit 74f43f82e6b): with `record_first_change` reading
/// the BASE buffer's modtime, `Fset_visited_file_modtime` refuses the
/// no-argument form in an indirect buffer instead of expanding its nil file
/// name (`src/fileio.c:6202-6203`).  GNU 31.0.90 signals
/// `(error "An indirect buffer does not have a visited file")`, while a plain
/// buffer with no visited file still gets `(wrong-type-argument stringp nil)`
/// from `Fexpand_file_name` -- the two cases must stay distinguishable.
#[test]
fn set_visited_file_modtime_refuses_an_indirect_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let base = eval.buffers.current_buffer_id().expect("current buffer");
    let indirect = eval
        .buffers
        .create_indirect_buffer(base, "indirect-modtime-145", false)
        .expect("indirect buffer");
    eval.buffers.set_current(indirect);

    match builtin_set_visited_file_modtime(&mut eval, vec![]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string(
                    "An indirect buffer does not have a visited file"
                )]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

/// GNU `Fset_visited_file_modtime`'s integer arm is
/// `check_integer_range (time_flag, -1, 0)` followed by
/// `make_timespec (0, UNKNOWN_MODTIME_NSECS - flag)` (`src/fileio.c:6188-6196`),
/// so the only integers it accepts are the two non-timestamps
/// `visited-file-modtime` can return, and each round-trips to itself.  GNU
/// 31.0.90: `0` -> `0`, `-1` -> `-1`, `5` -> `(args-out-of-range 5 -1 0)`.
/// Neomacs took any fixnum and answered `0` for all three, so the "the visited
/// file does not exist" state could not be expressed at all.
#[test]
fn set_visited_file_modtime_flags_are_the_two_values_visited_file_modtime_returns() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    for flag in [0, -1] {
        builtin_set_visited_file_modtime(&mut eval, vec![Value::fixnum(flag)])
            .expect("an in-range modtime flag");
        let reported =
            builtin_visited_file_modtime(&mut eval, vec![]).expect("visited-file-modtime");
        assert_eq!(
            reported.as_fixnum(),
            Some(flag),
            "flag {flag} must round-trip through visited-file-modtime"
        );
    }

    match builtin_set_visited_file_modtime(&mut eval, vec![Value::fixnum(5)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(
                sig.data,
                vec![Value::fixnum(5), Value::fixnum(-1), Value::fixnum(0)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

/// GNU asks the file-name handler BEFORE it stats
/// (`Fverify_visited_file_modtime`, src/fileio.c:6138-6143), which is the only
/// reason a remote buffer can answer at all: the visited name is not a path on
/// this filesystem, so the stat that follows can only fail.
///
/// Without the dispatch, a TRAMP buffer reports "the file changed" the instant
/// it is visited, and `tramp-handle-lock-file` (lisp/net/tramp.el:5137-5149)
/// turns that into `ask-user-about-supersession-threat` -- "Cannot resolve
/// conflict in batch mode" (lisp/userlock.el:178) -- on the first keystroke.
#[test]
fn verify_visited_file_modtime_asks_the_file_name_handler_before_it_stats() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str(
        "(fset 'pw174-modtime-handler \
         '(lambda (operation &rest _args) \
            (if (eq operation 'verify-visited-file-modtime) 'handled nil)))",
    )
    .expect("define the stand-in handler");
    eval.obarray.set_symbol_value(
        "file-name-handler-alist",
        Value::list(vec![Value::cons(
            Value::string("\\`/pw174:"),
            Value::symbol("pw174-modtime-handler"),
        )]),
    );
    eval.eval_str("(setq buffer-file-name \"/pw174:payments:/workspace/config.txt\")")
        .expect("visit a remote name");
    // A recorded modtime is required: GNU returns t for an unknown one before
    // it ever reaches the handler (src/fileio.c:6136).
    eval.eval_str("(set-visited-file-modtime (list 27272 1328 0 0))")
        .expect("record a modtime");

    assert_eq!(
        eval.eval_str("(verify-visited-file-modtime)")
            .expect("verify-visited-file-modtime"),
        Value::symbol("handled"),
    );
}

/// The other half of the same GNU rule: `Fset_visited_file_modtime` with no
/// argument asks the file-name handler before it stats (src/fileio.c:6211-6216),
/// because for a remote buffer only the handler can reach the file.
///
/// Without it the buffer keeps whatever `insert-file-contents` left -- in this
/// port, the wall clock at visit time -- and the gap between that and the
/// file's own mtime grows with however long the connection took to open.  Once
/// it passes TRAMP's two-second tolerance (lisp/net/tramp.el:5962) the buffer
/// reports "changed" and the first edit dies in
/// `ask-user-about-supersession-threat`.
#[test]
fn set_visited_file_modtime_asks_the_file_name_handler_before_it_stats() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    // GNU expands the buffer's file name first (src/fileio.c:6209), and that
    // expansion is itself a handled operation, so the stand-in must answer it
    // the way a real handler does -- by returning the name.
    eval.eval_str(
        "(fset 'pw174-set-modtime-handler \
         '(lambda (operation &rest args) \
            (cond ((eq operation 'set-visited-file-modtime) \
                   (setq pw174-set-modtime-called operation) nil) \
                  ((eq operation 'expand-file-name) (car args)) \
                  (t nil))))",
    )
    .expect("define the stand-in handler");
    eval.eval_str("(setq pw174-set-modtime-called nil)")
        .expect("seed the witness");
    eval.obarray.set_symbol_value(
        "file-name-handler-alist",
        Value::list(vec![Value::cons(
            Value::string("\\`/pw174:"),
            Value::symbol("pw174-set-modtime-handler"),
        )]),
    );
    eval.eval_str("(setq buffer-file-name \"/pw174:payments:/workspace/config.txt\")")
        .expect("visit a remote name");

    eval.eval_str("(set-visited-file-modtime)")
        .expect("set-visited-file-modtime");

    assert_eq!(
        eval.eval_str("pw174-set-modtime-called")
            .expect("read the witness"),
        Value::symbol("set-visited-file-modtime"),
    );
}
