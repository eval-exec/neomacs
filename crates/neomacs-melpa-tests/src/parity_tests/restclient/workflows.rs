use expect_test::expect;

use super::ParityBatchCase;

/// Drive `restclient-http-parse-current-and-do' (the HTTP request parser) on a
/// realistic GET block with headers, capturing method/url/header-count via a
/// no-op func (no network). Asserts the parser extracted them correctly.
fn http_parse_extracts_get_method_url_and_headers() -> ParityBatchCase {
    ParityBatchCase::value(
        "http_parse_extracts_get_method_url_and_headers",
        r####"
(with-temp-buffer
  (insert "GET https://example.com/api
Authorization: Bearer xyz
Content-Type: application/json
")
  (goto-char (point-min))
  (let (captured)
    (restclient-http-parse-current-and-do
     (lambda (method url headers entity &rest args)
       (setq captured (list :method method
                            :url url
                            :header-count (length headers)))))
    captured))
"####,
        expect![[r#"OK (:method "GET" :url "https://example.com/api" :header-count 2)"#]],
    )
}

/// Drive the pure header parser on a header string and assert the alist it
/// builds (key/value shape from restclient-make-header).
fn parse_headers_builds_alist() -> ParityBatchCase {
    ParityBatchCase::value(
        "parse_headers_builds_alist",
        r####"
(let ((headers (restclient-parse-headers
                "Authorization: Bearer xyz
Content-Type: application/json
")))
  (list :count (length headers)
        :auth (assoc "Authorization" headers)
        :ctype (assoc "Content-Type" headers)))
"####,
        expect![[
            r#"OK (:count 2 :auth ("Authorization" . "Bearer xyz") :ctype ("Content-Type" . "application/json"))"#
        ]],
    )
}

/// A POST block with a JSON body: the parser captures the entity (body text)
/// past the blank line separating headers from body.
fn http_parse_captures_post_body_entity() -> ParityBatchCase {
    ParityBatchCase::value(
        "http_parse_captures_post_body_entity",
        r####"
(with-temp-buffer
  (insert "POST https://example.com/things
Content-Type: application/json

{\"name\": \"foo\"}
")
  (goto-char (point-min))
  (let (captured)
    (restclient-http-parse-current-and-do
     (lambda (method url headers entity &rest args)
       (setq captured (list :method method :entity entity))))
    captured))
"####,
        expect![[r#"OK (:method "POST" :entity "{\"name\": \"foo\"}")"#]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        http_parse_extracts_get_method_url_and_headers(),
        parse_headers_builds_alist(),
        http_parse_captures_post_body_entity(),
    ]
}
