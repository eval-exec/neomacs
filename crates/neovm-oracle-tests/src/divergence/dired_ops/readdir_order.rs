//! GNU src/dired.c:186-213, 297-371, 790-791 consumes the complete host stream.
//! Expected transcripts must be generated with UPDATE_EXPECT=1 from GNU 31.1.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_oracle_parity_expect, oracle_sandbox::OracleSandbox};
use std::ffi::{CStr, CString};
use std::os::unix::ffi::OsStrExt;

fn raw_names(path: &std::path::Path) -> Vec<String> {
    let path = CString::new(path.as_os_str().as_bytes()).unwrap();
    let mut names = Vec::new();
    unsafe {
        let directory = libc::opendir(path.as_ptr());
        assert!(!directory.is_null());
        loop {
            let entry = libc::readdir(directory);
            if entry.is_null() {
                break;
            }
            names.push(
                CStr::from_ptr((*entry).d_name.as_ptr())
                    .to_str()
                    .unwrap()
                    .to_owned(),
            );
        }
        libc::closedir(directory);
    }
    names
}

#[test]
fn oracle_dired_stream_order_count_and_completions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let dir = OracleSandbox::create_fixture_tempdir().unwrap();
    for name in ["z", "a", "c", "B", "@", "0"] {
        std::fs::write(dir.path().join(name), b"").unwrap();
    }
    let names = raw_names(dir.path());
    let names = names
        .iter()
        .map(|name| format!("{name:?}"))
        .collect::<Vec<_>>()
        .join(" ");
    // Compare to the same directory stream GNU reads, so the transcript is
    // independent of whether the host filesystem places dots first or later.
    let form = format!(
        r#"(let ((dir {:?}) (stream '({names})))
      (list
        (equal (directory-files dir nil nil t) (reverse stream))
        (equal (mapcar #'car (directory-files-and-attributes dir nil nil t))
               (reverse stream))
        (let ((count 0) (rest stream) (prefix nil) (ok t))
          (while rest
            (setq count (1+ count) prefix (cons (car rest) prefix) rest (cdr rest))
            (let ((sorted (sort (copy-sequence prefix) #'string<)))
              (setq ok (and ok
                (equal (directory-files dir nil nil nil count) sorted)
                (equal (mapcar #'car
                         (directory-files-and-attributes dir nil nil nil nil count)) sorted)))))
          ok)
        (equal (file-name-all-completions "" dir)
          (mapcar (lambda (name)
            (if (member name '("." "..")) (concat name "/") name))
            (reverse stream)))))"#,
        dir.path().to_str().unwrap()
    );
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    assert_oracle_parity_expect(&form, expect);
}
