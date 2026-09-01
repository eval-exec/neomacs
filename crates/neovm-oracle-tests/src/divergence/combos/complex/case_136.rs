//! Complex combo batch 136 — `tramp` / `ange-ftp` / `vagrant` / `docker`
//! / `kubernetes` remote file methods availability and predicates.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx136_tramp_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'tramp)
      (list (fboundp 'tramp-file-name-handler)
            (boundp 'tramp-methods)
            (boundp 'tramp-default-method)
            (boundp 'tramp-default-host)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx136_remote_file_predicate_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (file-remote-p "/local/path")
      (file-remote-p "/home/user/file"))
"##,
        expect,
    );
}

#[test]
fn div_cx136_remote_file_predicate_method_host() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"ssh\" \"host\" \"user\" \"/remote/path\" \"localhost\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (file-remote-p "/ssh:host:" 'method)
      (file-remote-p "/ssh:host:" 'host)
      (file-remote-p "/ssh:user@host:/path" 'user)
      (file-remote-p "/method:host:/remote/path" 'localname)
      (file-remote-p "/ssh:localhost:" 'host))
"##,
        expect,
    );
}

#[test]
fn div_cx136_ange_ftp_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ange-ftp)
      (list (fboundp 'ange-ftp-get-buffer)
            (boundp 'ange-ftp-default-user)
            (boundp 'ange-ftp-default-password)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx136_tramp_completion_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'tramp-completion-mode-p)
          (boundp 'tramp-completion-reread-directory-timeout))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx136_tramp_method_list_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-variable)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((methods (mapcar #'car tramp-methods)))
      (list (consp methods)
            (member "ssh" methods)
            (member "scp" methods)
            (member "rsync" methods)
            (member "sudo" methods)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx136_docker_tramp_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'docker-tramp)
          (fboundp 'docker-tramp-cleanup)
          (boundp 'docker-tramp-docker-program))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx136_k8s_tramp_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'kubernetes)
          (fboundp 'kubernetes-overview))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx136_tramp_get_connection_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'tramp-get-connection-property)
          (fboundp 'tramp-set-connection-property)
          (boundp 'tramp-connection-properties))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx136_remote_directory_files_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"/ssh:fake-host:\" \"ssh\" \"fake-host\" \"/tmp/\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((remote-path "/ssh:fake-host:/tmp/"))
      (list (file-remote-p remote-path)
            (file-remote-p remote-path 'method)
            (file-remote-p remote-path 'host)
            (file-remote-p remote-path 'localname)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx136_tramp_persistency_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'tramp-read-persistency-file)
          (boundp 'tramp-persistency-file-name)
          (boundp 'tramp-verbose))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx136_tramp_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'tramp)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Tramp mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (fboundp 'tramp-file-name-handler)
                             (boundp 'tramp-default-method)
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
