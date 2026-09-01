//! Oracle parity tests for GNU `url/url-parse.el` URL parser semantics.
//!
//! GNU stores path and query together in the `filename` slot, lowercases
//! schemes and hosts, has special `data:` URI handling, and recreates URLs
//! with default ports omitted.  These tests pin the public accessor behavior.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_url_parse_full_authority_components() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'url-parse)
  (let ((u (url-generic-parse-url
            "HTTP://Bob:Pass@Example.COM:8080/a/b?q=1#frag")))
    (list
     (url-type u)
     (url-user u)
     (url-password u)
     (url-host u)
     (url-portspec u)
     (url-port u)
     (url-filename u)
     (url-target u)
     (url-fullness u)
     (url-path-and-query u)
     (url-recreate-url u))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"http\" \"Bob\" \"Pass\" \"example.com\" 8080 8080 \"/a/b?q=1\" \"frag\" t (\"/a/b\" . \"q=1\") \"http://Bob:Pass@example.com:8080/a/b?q=1#frag\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_url_parse_default_ports_and_ipv6() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'url-parse)
  (let ((http (url-generic-parse-url "http://example.com:80/path"))
        (https (url-generic-parse-url "https://example.com:443/path"))
        (custom (url-generic-parse-url "http://example.com:81/path"))
        (ipv6 (url-generic-parse-url "http://[2001:db8::1]:8080/index")))
    (list
     (list (url-portspec http) (url-port http) (url-port-if-non-default http)
           (url-recreate-url http))
     (list (url-portspec https) (url-port https) (url-port-if-non-default https)
           (url-recreate-url https))
     (list (url-portspec custom) (url-port custom) (url-port-if-non-default custom)
           (url-recreate-url custom))
     (list (url-host ipv6) (url-portspec ipv6) (url-recreate-url ipv6)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((80 80 nil \"http://example.com/path\") (443 443 nil \"https://example.com/path\") (81 81 81 \"http://example.com:81/path\") (\"[2001:db8::1]\" 8080 \"http://[2001:db8::1]:8080/index\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_url_parse_relative_data_and_file_paths() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'url-parse)
  (let ((relative (url-generic-parse-url "/local/path?x=1#frag"))
        (bare (url-generic-parse-url "README"))
        (data (url-generic-parse-url "data:text/plain,a?b#c"))
        (file (url-generic-parse-url "file:///C:/Temp/file.txt")))
    (list
     (list (url-type relative) (url-host relative) (url-filename relative)
           (url-target relative) (url-fullness relative)
           (url-recreate-url relative))
     (list (url-type bare) (url-host bare) (url-filename bare)
           (url-target bare) (url-fullness bare)
           (url-recreate-url bare))
     (list (url-type data) (url-filename data) (url-target data)
           (url-recreate-url data))
     (list (url-type file) (url-host file) (url-filename file)
           (url-recreate-url file)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((nil nil \"/local/path?x=1\" \"frag\" nil \"/local/path?x=1#frag\") (nil nil \"README\" nil nil \"README\") (\"data\" \"text/plain,a?b#c\" nil \"data:text/plain,a?b#c\") (\"file\" \"\" \"C:/Temp/file.txt\" \"file:///C:/Temp/file.txt\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_url_parse_nil_empty_ports_and_host_unhex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'url-parse)
  (let ((nil-url (url-generic-parse-url nil))
        (empty-port (url-generic-parse-url "http://example.com:/p"))
        (escaped-host (url-generic-parse-url "http://foo%20bar.example/path")))
    (list
     (list (url-type nil-url) (url-user nil-url) (url-host nil-url)
           (url-filename nil-url) (url-fullness nil-url)
           (url-recreate-url nil-url))
     (list (url-host empty-port) (url-portspec empty-port)
           (url-port empty-port) (url-recreate-url empty-port))
     (list (url-host escaped-host) (url-recreate-url escaped-host)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((nil nil nil nil nil \"/\") (\"example.com\" nil 80 \"http://example.com/p\") (\"foo bar.example\" \"http://foo bar.example/path\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
