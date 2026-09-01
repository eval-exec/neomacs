//! Oracle parity tests for GNU `file-remote-p`, `file-local-name`, and
//! `file-local-copy` semantics.
//!
//! GNU implements these in `lisp/files.el`.  `file-remote-p` dispatches through
//! file-name handlers and can parse Tramp-style names without opening a
//! connection; local paths return nil.  `file-local-name` returns the remote
//! localname component or the original local path, and `file-local-copy` returns
//! nil for directly accessible local files.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_remote_and_local_name_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((local "/tmp/neomacs-oracle-local.txt")
      (remote "/ssh:user@host:/tmp/remote.txt")
      (sudo "/sudo::/etc/hosts"))
  (list
   (file-remote-p local)
   (file-remote-p local nil t)
   (file-remote-p local 'method)
   (file-local-name local)
   (file-local-copy local)
   (file-remote-p remote)
   (file-remote-p remote 'method)
   (file-remote-p remote 'user)
   (file-remote-p remote 'host)
   (file-remote-p remote 'localname 'never)
   (file-local-name remote)
   ;; Missing method/user/host pieces are filled in by the handler for the full
   ;; identifier, but individual localname parsing should not connect.
   (file-remote-p sudo)
   (file-remote-p sudo 'method)
   (file-remote-p sudo 'localname 'never)
   (condition-case err
       (file-remote-p 42)
     (error (list (car err) (cdr err))))
   (condition-case err
       (file-local-name 42)
     (error (list (car err) (cdr err))))
   (condition-case err
       (file-local-copy 42)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil nil nil \"/tmp/neomacs-oracle-local.txt\" nil \"/ssh:user@host:\" \"ssh\" \"user\" \"host\" \"/tmp/remote.txt\" \"/tmp/remote.txt\" #(\"/sudo:root@oracle-host:\" 6 10 (tramp-default t) 11 22 (tramp-default t)) \"sudo\" \"/etc/hosts\" (wrong-type-argument (stringp 42)) (wrong-type-argument (stringp 42)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
