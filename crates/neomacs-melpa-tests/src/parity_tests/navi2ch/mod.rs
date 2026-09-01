use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, NAVI2CH_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const NAVI2CH_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const NAVI2CH_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'navi2ch)

(defun navi2ch-test-read-file (file coding-system)
  (with-temp-buffer
    (let ((coding-system-for-read coding-system))
      (insert-file-contents file))
    (buffer-string)))

(defun navi2ch-test-sort-cookies (cookies)
  (sort (copy-tree cookies)
        (lambda (left right) (string< (car left) (car right)))))
"##;

fn navi2ch_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(NAVI2CH_MELPA_PIN, "navi2ch.el")
        .expect("prepare pinned Navi2ch source below ./tmp")
        .with_prelude(NAVI2CH_TEST_PRELUDE)
        .with_timeout(NAVI2CH_TEST_TIMEOUT)
}

fn archived_post_html_is_rendered_identically_in_string_and_buffer_paths() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((navi2ch-decode-character-references t)
       (html (concat
              "<div class=\"post\"><a href=\"/thread/42\">&gt;&gt;42</a><br>"
              "Status: &amp; ready &euro; &#9731; &#x2192;</div><hr>done"))
       (string-rendered (navi2ch-replace-html-tag html))
       (buffer-rendered
        (with-temp-buffer
          (insert html)
          (navi2ch-replace-html-tag-with-buffer)
          (buffer-string))))
  (list :source html
        :string-rendered string-rendered
        :buffer-rendered buffer-rendered
        :same (equal string-rendered buffer-rendered)
        :characters (string-to-list string-rendered)))
"##;
    let expect = expect![[
        r####"OK (:source "<div class=\"post\"><a href=\"/thread/42\">&gt;&gt;42</a><br>Status: &amp; ready &euro; &#9731; &#x2192;</div><hr>done" :string-rendered ">>42\nStatus: & ready € ☃ →\n--\ndone" :buffer-rendered ">>42\nStatus: & ready € ☃ →\n--\ndone" :same t :characters (62 62 52 50 10 83 116 97 116 117 115 58 32 38 32 114 101 97 100 121 32 8364 32 9731 32 8594 10 45 45 10 100 111 110 101))"####
    ]];
    ParityBatchCase::value(
        "archived_post_html_is_rendered_identically_in_string_and_buffer_paths",
        elisp_form,
        expect,
    )
}

fn legacy_http_dates_normalize_to_rfc1123_and_support_cache_age_calculation() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((inputs '("Sun, 06 Nov 1994 08:49:37 GMT"
                 "Sunday, 06-Nov-94 08:49:37 GMT"
                 "Sun Nov  6 08:49:37 1994"))
       (decoded (mapcar #'navi2ch-http-date-decode inputs))
       (normalized (mapcar #'navi2ch-http-date-encode decoded))
       (base (car decoded))
       (two-days-later (navi2ch-add-days-to-time base 2)))
  (list
   :normalized normalized
   :utc-times
   (mapcar (lambda (time) (format-time-string "%Y-%m-%dT%H:%M:%SZ" time t))
           decoded)
   :two-days-later
   (list (navi2ch-http-date-encode two-days-later)
         (format-time-string "%Y-%m-%dT%H:%M:%SZ" two-days-later t))
   :ordering
   (list (and (navi2ch-compare-times two-days-later base) t)
         (and (navi2ch-compare-times base two-days-later) t))))
"##;
    let expect = expect![[
        r####"OK (:normalized ("Sun, 06 Nov 1994 08:49:37 GMT" "Sun, 06 Nov 1994 08:49:37 GMT" "Sun, 06 Nov 1994 08:49:37 GMT") :utc-times ("1994-11-06T08:49:37Z" "1994-11-06T08:49:37Z" "1994-11-06T08:49:37Z") :two-days-later ("Tue, 08 Nov 1994 08:49:37 GMT" "1994-11-08T08:49:37Z") :ordering (t nil))"####
    ]];
    ParityBatchCase::value(
        "legacy_http_dates_normalize_to_rfc1123_and_support_cache_age_calculation",
        elisp_form,
        expect,
    )
}

fn authenticated_proxy_request_components_preserve_target_and_encode_form_data() -> ParityBatchCase
{
    let elisp_form = r##"
(let* ((url "http://alice:s3cret@forum.example.org:8080/board/read.cgi?id=42")
       (direct (navi2ch-net-split-url url))
       (proxied (navi2ch-net-split-url url "http://proxy.example.net:3128"))
       (credentials
        (navi2ch-net-http-basic-credentials
         (cdr (assq 'user direct))
         (cdr (assq 'pass direct))))
       (headers
        (navi2ch-net-make-request-header
         `(("Host" . ,(cdr (assq 'host2ch proxied)))
           ("Proxy-Authorization" . ,credentials)
           ("X-Optional")
           ("Connection" . "close"))))
       (form
        (navi2ch-net-get-param-string
         '(("query" . "alpha beta")
           ("path" . "/release?ready=yes"))
         'utf-8)))
  (list :direct direct
        :proxied proxied
        :credentials credentials
        :headers headers
        :form form
        :navigation
        (navi2ch-url-encode-string "board name/next\nline" 'utf-8 t)))
"##;
    let expect = expect![[
        r####"OK (:direct ((user . "alice") (pass . "s3cret") (host . "forum.example.org") (port . 8080) (file . "/board/read.cgi?id=42") (host2ch . "forum.example.org:8080")) :proxied ((user . "alice") (pass . "s3cret") (host . "proxy.example.net") (file . "http://alice:s3cret@forum.example.org:8080/board/read.cgi?id=42") (port . 3128) (host2ch . "forum.example.org:8080")) :credentials "Basic YWxpY2U6czNjcmV0" :headers "Host: forum.example.org:8080\15\nProxy-Authorization: Basic YWxpY2U6czNjcmV0\15\nConnection: close\15\n" :form "query=alpha%20beta&path=%2Frelease%3Fready%3Dyes" :navigation "board+name/next%0D%0Aline")"####
    ]];
    ParityBatchCase::value(
        "authenticated_proxy_request_components_preserve_target_and_encode_form_data",
        elisp_form,
        expect,
    )
}

fn cookie_store_replaces_session_values_and_scopes_headers_to_domain_and_path() -> ParityBatchCase {
    let elisp_form = r##"
(let ((navi2ch-net-cookies nil))
  (navi2ch-net-store-cookie '("session" "old") ".example.org" "/forum/")
  (navi2ch-net-store-cookie '("theme" "dark") "sub.example.org" "/")
  (navi2ch-net-store-cookie '("session" "renewed") ".example.org" "/forum/")
  (navi2ch-net-store-cookie '("admin" "hidden") ".example.org" "/admin/")
  (let* ((forum
          (navi2ch-test-sort-cookies
           (navi2ch-net-match-cookies
            "http://sub.example.org/forum/thread/123")))
         (admin
          (navi2ch-test-sort-cookies
           (navi2ch-net-match-cookies
            "http://sub.example.org/admin/panel"))))
    (list :forum forum
          :forum-header (navi2ch-net-cookie-string forum 'utf-8)
          :admin admin
          :admin-header (navi2ch-net-cookie-string admin 'utf-8)
          :domains (mapcar #'symbol-name
                           (navi2ch-net-cookie-domains "Sub.Example.Org"))
          :paths (mapcar #'symbol-name
                         (navi2ch-net-cookie-paths "/forum/thread/123")))))
"##;
    let expect = expect![[
        r####"OK (:forum (("session" "renewed") ("theme" "dark")) :forum-header "session=renewed; theme=dark" :admin (("admin" "hidden") ("theme" "dark")) :admin-header "admin=hidden; theme=dark" :domains ("example.org" ".example.org" "sub.example.org" ".sub.example.org") :paths ("/" "/forum" "/forum/" "/forum/thread" "/forum/thread/"))"####
    ]];
    ParityBatchCase::value(
        "cookie_store_replaces_session_values_and_scopes_headers_to_domain_and_path",
        elisp_form,
        expect,
    )
}

fn backend_router_parses_and_rebuilds_2ch_jbbs_and_local_board_urls() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((two-url "http://example.2ch.net/test/read.cgi/unix/1065246418/12-15")
       (jbbs-url
        "http://jbbs.shitaraba.com/bbs/read.cgi/computer/351/1040452814/7-9")
       (local-url "x-localbbs:///srv/team/dat/1704164645.dat/3")
       (two-board (navi2ch-multibbs-url-to-board two-url))
       (two-article (navi2ch-multibbs-url-to-article two-url))
       (jbbs-board (navi2ch-multibbs-url-to-board jbbs-url))
       (jbbs-article (navi2ch-multibbs-url-to-article jbbs-url))
       (local-board (navi2ch-multibbs-url-to-board local-url))
       (local-article (navi2ch-multibbs-url-to-article local-url)))
  (list
   :parsed
   (list
    (list :type (navi2ch-multibbs-url-to-bbstype two-url)
          :board two-board :article two-article)
    (list :type (navi2ch-multibbs-url-to-bbstype jbbs-url)
          :board jbbs-board :article jbbs-article)
    (list :type (navi2ch-multibbs-url-to-bbstype local-url)
          :board local-board :article local-article))
   :rebuilt
   (list
    (navi2ch-multibbs-article-to-url
     '((uri . "http://example.2ch.net/unix/") (id . "unix"))
     '((artid . "1065246418")) 12 15 t)
    (navi2ch-multibbs-article-to-url
     '((uri . "http://jbbs.shitaraba.com/computer/351/") (id . "351"))
     '((artid . "1040452814")) 7 9 t)
    (navi2ch-multibbs-article-to-url
     '((uri . "x-localbbs:///srv/team/") (id . "team"))
     '((artid . "1704164645")) 3 5 t))))
"##;
    let expect = expect![[
        r####"OK (:parsed ((:type unknown :board ((uri . "http://example.2ch.net/unix/") (id . "unix")) :article ((number . 12) (artid . "1065246418"))) (:type jbbs-shitaraba :board ((uri . "http://jbbs.shitaraba.com/computer/351/") (id . "351")) :article ((number . 7) (artid . "1040452814"))) (:type localfile :board ((id . "team") (uri . "x-localbbs:///srv/team/")) :article ((number . 3) (artid . "1704164645")))) :rebuilt ("http://example.2ch.net/test/read.cgi/unix/1065246418/12-15n" "http://jbbs.shitaraba.com/bbs/read.cgi/computer/351/1040452814/7-9n" "x-localbbs:///srv/team/dat/1704164645.dat/3-5n"))"####
    ]];
    ParityBatchCase::value(
        "backend_router_parses_and_rebuilds_2ch_jbbs_and_local_board_urls",
        elisp_form,
        expect,
    )
}

fn local_board_thread_creation_and_reply_update_dat_and_subject_files_atomically() -> ParityBatchCase
{
    let elisp_form = r##"
(let* ((directory (make-temp-file "navi2ch-local-board-" t))
       (fixed-time (encode-time 5 4 3 2 1 2024 t))
       (article-id (format-time-string "%s" fixed-time t))
       (dat-file (expand-file-name (concat "dat/" article-id ".dat") directory))
       (subject-file
        (expand-file-name navi2ch-localfile-subject-file-name directory))
       (real-format-time-string (symbol-function 'format-time-string))
       (navi2ch-localfile-use-lock nil))
  (unwind-protect
      (cl-letf (((symbol-function 'current-time) (lambda () fixed-time))
                ((symbol-function 'format-time-string)
                 (lambda (format &optional time universal)
                   (funcall real-format-time-string format time t))))
        (navi2ch-localfile-create-thread
         directory "Alice <ops>" "sage" "first > line\nsecond" "Release & Notes")
        (navi2ch-localfile-append-message
         directory article-id "Bob" "" "follow-up <ok>")
        (list
         :article-id article-id
         :dat (navi2ch-test-read-file dat-file navi2ch-localfile-coding-system)
         :subject
         (navi2ch-test-read-file subject-file navi2ch-localfile-coding-system)
         :files (sort (directory-files (expand-file-name "dat" directory)
                                       nil "\\.dat\\'")
                      #'string<)
         :url
         (navi2ch-localfile-article-to-url
          '((uri . "x-localbbs:///srv/team"))
          `((artid . ,article-id)) 1 2 t)))
    (delete-directory directory t)))
"##;
    let expect = expect![[
        r####"OK (:article-id "1704164645" :dat "Alice &lt;ops&gt;<>sage<>24/01/02 03:04<>first &gt; line<br>second<>Release & Notes\nBob<><>24/01/02 03:04<>follow-up &lt;ok&gt;<>\n" :subject "1704164645.dat<>Release & Notes (2)\n" :files ("1704164645.dat") :url "x-localbbs:///srv/team/dat/1704164645.dat/1-2n")"####
    ]];
    ParityBatchCase::value(
        "local_board_thread_creation_and_reply_update_dat_and_subject_files_atomically",
        elisp_form,
        expect,
    )
}

fn image_metadata_and_display_width_helpers_handle_binary_and_wide_content() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((png
        (apply #'unibyte-string
               '(137 80 78 71 13 10 26 10
                 0 0 0 13 73 72 68 82
                 0 0 1 64 0 0 0 100
                 8 2 0 0 0)))
       (gif
        (apply #'unibyte-string
               '(71 73 70 56 57 97
                 64 1 100 0 128 0 0
                 0 0 0 255 255 255 59)))
       (wide "status: 完了 / ready"))
  (list
   :png (navi2ch-thumbnail-image-png-identify png)
   :gif (navi2ch-thumbnail-image-gif-identify gif)
   :width (string-width wide)
   :headline (navi2ch-truncate-string-to-width wide 12 0 ?.)
   :window (navi2ch-truncate-string-to-width wide 16 0 ?_)
   :padded-status (navi2ch-truncate-string-to-width "ready" 8 0 ?.)
   :right-aligned
   (sort (copy-sequence '("9" "100" " 12" "2"))
         #'navi2ch-right-aligned-string<)))
"##;
    let expect = expect![[
        r####"OK (:png (320 100 nil) :gif (320 100 nil) :width 20 :headline "status: 完了" :window "status: 完了 / r" :padded-status "ready..." :right-aligned ("2" "9" " 12" "100"))"####
    ]];
    ParityBatchCase::value(
        "image_metadata_and_display_width_helpers_handle_binary_and_wide_content",
        elisp_form,
        expect,
    )
}

#[test]
fn navi2ch_package_batch() {
    let cases = vec![
        archived_post_html_is_rendered_identically_in_string_and_buffer_paths(),
        legacy_http_dates_normalize_to_rfc1123_and_support_cache_age_calculation(),
        authenticated_proxy_request_components_preserve_target_and_encode_form_data(),
        cookie_store_replaces_session_values_and_scopes_headers_to_domain_and_path(),
        backend_router_parses_and_rebuilds_2ch_jbbs_and_local_board_urls(),
        local_board_thread_creation_and_reply_update_dat_and_subject_files_atomically(),
        image_metadata_and_display_width_helpers_handle_binary_and_wide_content(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Navi2ch parity test");
    assert_oracle_batch_cases(navi2ch_oracle(), test_name, "navi2ch_parity", &cases);
}
