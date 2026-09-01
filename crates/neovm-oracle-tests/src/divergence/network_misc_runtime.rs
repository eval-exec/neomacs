//! Network process loopback (server+client roundtrip, process-contact) and
//! misc parity (format-message quoting, ngettext, key-description, char-fold,
//! special floats, ash/bignum, text-property-search).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn network_process_contact() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((server (make-network-process :name "neo-srv2-xxx" :server t :host 'local
               :service t :family 'ipv4 :noquery t)))
  (let ((local (process-contact server :local)))
    (prog1 (list (processp server) (eq (process-status server) 'listen)
                 (vectorp local) (integerp (aref local (1- (length local)))))
      (delete-process server))))"##,
        expect,
    );
}

#[test]
fn tcp_server_client() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hi-server\" \"ack\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((received nil) (server nil) (port nil))
  (setq server (make-network-process :name "neo-srv-xxx" :server t :host 'local
                 :service t :family 'ipv4 :noquery t
                 :filter (lambda (proc s) (setq received s)
                           (process-send-string proc "ack"))))
  (let ((local (process-contact server :local)))
    (setq port (aref local (1- (length local)))))
  (let ((client (make-network-process :name "neo-cli-xxx" :host 'local :service port
                  :family 'ipv4 :noquery t)) (cresp ""))
    (set-process-filter client (lambda (_p s) (setq cresp (concat cresp s))))
    (process-send-string client "hi-server")
    (let ((k 0)) (while (and (or (null received) (string= cresp "")) (< k 150))
                   (accept-process-output nil 0.02) (setq k (1+ k))))
    (delete-process client) (delete-process server)
    (list received cresp)))"##,
        expect,
    );
}

#[test]
fn ash_bignum() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (1180591620717411303424 32 9223372036854775808 40 717897987691852588770249)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (ash 1 70) (ash (ash 1 70) -65) (logand (1- (ash 1 64)) (ash 1 63))
        (logcount (1- (ash 1 40))) (expt 3 50))"##,
        expect,
    );
}

#[test]
fn char_fold_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'char-fold)
(with-temp-buffer
  (insert "the cafe is open")
  (goto-char (point-min))
  (let ((case-fold-search t))
    (list (re-search-forward (char-fold-to-regexp "cafe") nil t))))"##,
        expect,
    );
}

#[test]
fn format_message_quotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"use ‘foo’ here\" \"\\\\‘C-c\\\\’ test\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((text-quoting-style 'curve))
  (list (format-message "use `foo' here") (substitute-command-keys "\\`C-c\\' test")))"##,
        expect,
    );
}

#[test]
fn kbd_key_desc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"C-c C-x\" \"M-RET\" (1) \"C-a M-b\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (key-description (kbd "C-c C-x")) (key-description (kbd "M-RET"))
        (listify-key-sequence (kbd "C-a")) (key-description [?\C-a ?\M-b]))"##,
        expect,
    );
}

#[test]
fn ngettext_fn() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"%d file\" \"%d files\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (ngettext "%d file" "%d files" 1) (ngettext "%d file" "%d files" 2))"##,
        expect,
    );
}

#[test]
fn number_special_floats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t \"1.0e+INF\" 3.0 2.0 -2.0 2.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (= 1.0e+INF 1.0e+INF) (isnan 0.0e+NaN)
        (> 1.0e+INF most-positive-fixnum) (format "%s" 1.0e+INF)
        (ftruncate 3.7) (fround 2.5) (ffloor -1.5) (fceiling 1.2))"##,
        expect,
    );
}

#[test]
fn string_pixel_logical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 100 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "ab\tcd")
  (goto-char (point-max))
  (list (current-column) (char-before) (line-beginning-position)))"##,
        expect,
    );
}

#[test]
fn text_property_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'text-property-search)
(with-temp-buffer
  (insert "aaBBBcc")
  (put-text-property 3 6 'hi t)
  (goto-char (point-min))
  (let ((m (text-property-search-forward 'hi t t)))
    (list (prop-match-beginning m) (prop-match-end m))))"##,
        expect,
    );
}
