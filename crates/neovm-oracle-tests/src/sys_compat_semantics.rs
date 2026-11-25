//! GNU Emacs 31.1 oracle parity tests for three system compatibility fixes:
//! 1. `permission-denied` error-conditions `(permission-denied file-error error)`
//!    and error-message `"Cannot access file or directory"` (`src/fileio.c:6819-6822`).
//! 2. `(group-name GID)` returning `nil` for unknown GIDs and parsing via GNU
//!    `CONS_TO_INTEGER` (`src/editfns.c:1175-1191`, `src/data.c:2932-2981`).
//! 3. `make-network-process` resolving `:service` via `getaddrinfo` and
//!    formatting `"HOST/SERVICE GAI_STRERROR"` (`src/process.c:4133-4215`, `:4700-4736`).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[cfg(unix)]
#[test]
fn oracle_permission_denied_is_file_error_and_matches_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((permission-denied file-error error) \"Cannot access file or directory\" (caught-signal permission-denied) (caught-file-error permission-denied \"Opening input file\" \"Permission denied\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let* ((tmp-dir (make-temp-file "neomacs-perm-oracle-" t))
       (no-read (expand-file-name "no-read" tmp-dir)))
  (unwind-protect
      (progn
        (write-region "secret" nil no-read nil 'silent)
        (set-file-modes no-read #o000)
        (list
         (get 'permission-denied 'error-conditions)
         (get 'permission-denied 'error-message)
         (condition-case err
             (signal 'permission-denied '("Opening input file" "Permission denied" "/x"))
           (file-error (list 'caught-signal (car err)))
           (error (list 'caught-generic (car err))))
         (if (= (user-uid) 0)
             '(caught-file-error permission-denied "Opening input file" "Permission denied")
           (condition-case err
               (insert-file-contents no-read)
             (file-error (list 'caught-file-error (car err) (nth 1 err) (nth 2 err)))
             (error (list 'caught-generic (car err) (nth 1 err) (nth 2 err)))))))
    (ignore-errors (set-file-modes no-read #o600))
    (ignore-errors (delete-file no-read))
    (ignore-errors (delete-directory tmp-dir))))
"#,
        expect,
    );
}

#[cfg(unix)]
#[test]
fn oracle_group_name_missing_gid_and_cons_to_integer_matches_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t t t nil nil (error \"Not an in-range integer, integral float, or cons of integers\") (error \"Not an in-range integer, integral float, or cons of integers\") (error \"Not an in-range integer, integral float, or cons of integers\") (error \"Not an in-range integer, integral float, or cons of integers\") (error \"Invalid GID specification\") (error \"Invalid GID specification\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((g (group-gid)))
  (list
   (stringp (group-name g))
   (equal (group-name g) (group-name (float g)))
   (equal (group-name g) (group-name (cons (ash g -16) (logand g #xffff))))
   (equal (group-name 0) (group-name '(0 . 0)))
   (equal (group-name 0) (group-name '(0 0)))
   (group-name 4000000000)
   (group-name 4000000000.0)
   (condition-case e (group-name -1) (error e))
   (condition-case e (group-name 1.5) (error e))
   (condition-case e (group-name 5000000000) (error e))
   (condition-case e (group-name '(0 . -1)) (error e))
   (condition-case e (group-name nil) (error e))
   (condition-case e (group-name "0") (error e))))
"#,
        expect,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn oracle_make_network_process_unknown_service_matches_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((error \"127.0.0.1/neomacs-nonexistent-svc Servname not supported for ai_socktype\") (error \"127.0.0.1/neomacs-nonexistent-svc Servname not supported for ai_socktype\") (error \"127.0.0.1/neomacs-nonexistent-svc Servname not supported for ai_socktype\") (error \"127.0.0.1/neomacs-nonexistent-svc Servname not supported for ai_socktype\") (error \"::1/neomacs-nonexistent-svc Servname not supported for ai_socktype\") (error \"127.0.0.1/-1 Servname not supported for ai_socktype\") (wrong-type-argument stringp nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list
 (condition-case e
     (make-network-process :name "ns1" :host "127.0.0.1" :service "neomacs-nonexistent-svc")
   (error e))
 (condition-case e
     (make-network-process :name "ns2" :host 'local :service "neomacs-nonexistent-svc")
   (error e))
 (condition-case e
     (make-network-process :name "ns3" :server t :service "neomacs-nonexistent-svc")
   (error e))
 (condition-case e
     (make-network-process :name "ns4" :type 'datagram :host "127.0.0.1" :service "neomacs-nonexistent-svc")
   (error e))
 (condition-case e
     (make-network-process :name "ns5" :family 'ipv6 :service "neomacs-nonexistent-svc")
   (error e))
 (condition-case e
     (make-network-process :name "ns6" :host "127.0.0.1" :service -1)
   (error e))
 (condition-case e
     (make-network-process :name "ns7" :host "127.0.0.1" :service nil)
   (error e)))
"#,
        expect,
    );
}
