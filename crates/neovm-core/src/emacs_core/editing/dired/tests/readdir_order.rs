//! GNU consumes dot entries at their actual positions in the host stream.

use super::super::*;
use std::ffi::{CStr, CString};
use std::os::unix::ffi::OsStrExt;

fn host_names(path: &std::path::Path) -> Vec<Vec<u8>> {
    let path = CString::new(path.as_os_str().as_bytes()).unwrap();
    let mut names = Vec::new();
    unsafe {
        let dir = libc::opendir(path.as_ptr());
        assert!(!dir.is_null());
        loop {
            let entry = libc::readdir(dir);
            if entry.is_null() {
                break;
            }
            names.push(CStr::from_ptr((*entry).d_name.as_ptr()).to_bytes().to_vec());
        }
        libc::closedir(dir);
    }
    names
}

#[test]
fn directory_attributes_count_and_completions_preserve_host_dot_positions() {
    crate::test_utils::init_test_tracing();
    let tmp = crate::test_utils::workspace_root().join("tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    let dir = tempfile::tempdir_in(tmp).unwrap();
    for name in ["z", "a", "c", "B", "@", "0"] {
        std::fs::write(dir.path().join(name), b"").unwrap();
    }
    let names = host_names(dir.path());
    let mut ctx = Context::new();
    let directory = Value::string(dir.path().to_str().unwrap());
    let attrs = builtin_directory_files_and_attributes(
        &mut ctx,
        vec![directory, Value::NIL, Value::NIL, Value::T],
    )
    .unwrap();
    let actual: Vec<_> = list_to_vec(&attrs)
        .unwrap()
        .into_iter()
        .map(|pair| {
            pair.cons_car()
                .as_lisp_string()
                .unwrap()
                .as_bytes()
                .to_vec()
        })
        .collect();
    let expected: Vec<_> = names.iter().rev().cloned().collect();
    assert_eq!(actual, expected);

    for count in 1..=names.len() {
        let attrs = builtin_directory_files_and_attributes(
            &mut ctx,
            vec![
                directory,
                Value::NIL,
                Value::NIL,
                Value::NIL,
                Value::NIL,
                Value::fixnum(count as i64),
            ],
        )
        .unwrap();
        let actual: Vec<_> = list_to_vec(&attrs)
            .unwrap()
            .into_iter()
            .map(|pair| {
                pair.cons_car()
                    .as_lisp_string()
                    .unwrap()
                    .as_bytes()
                    .to_vec()
            })
            .collect();
        let mut expected = names[..count].to_vec();
        expected.sort();
        assert_eq!(actual, expected, "COUNT {count}");
    }
    let completions =
        builtin_file_name_all_completions(&mut ctx, vec![Value::string(""), directory]).unwrap();
    let actual: Vec<_> = list_to_vec(&completions)
        .unwrap()
        .into_iter()
        .map(|name| name.as_lisp_string().unwrap().as_bytes().to_vec())
        .collect();
    let expected: Vec<_> = names
        .iter()
        .rev()
        .map(|name| {
            let mut name = name.clone();
            if name == b"." || name == b".." {
                name.push(b'/');
            }
            name
        })
        .collect();
    assert_eq!(actual, expected);
}
