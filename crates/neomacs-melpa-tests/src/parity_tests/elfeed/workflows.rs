use expect_test::expect;

use super::ParityBatchCase;

/// Drive `elfeed-feed-type' (the pure feed classifier) on parsed Atom and RSS
/// XML and assert the types it returns. No network: the parsers are pure.
fn feed_type_classifies_atom_and_rss() -> ParityBatchCase {
    ParityBatchCase::value(
        "feed_type_classifies_atom_and_rss",
        r####"
(list :atom (with-temp-buffer
              (insert "<feed><entry/></feed>")
              (elfeed-feed-type (xml-parse-region)))
      :rss (with-temp-buffer
             (insert "<rss><channel/></rss>")
             (elfeed-feed-type (xml-parse-region))))
"####,
        expect![[r#"OK (:atom :atom :rss :rss)"#]],
    )
}

/// Drive `elfeed--parse-opml' on an OPML subscription list and assert it
/// extracts the outlined feed URLs.
fn parse_opml_extracts_feed_urls() -> ParityBatchCase {
    ParityBatchCase::value(
        "parse_opml_extracts_feed_urls",
        r####"
(with-temp-buffer
  (insert "<opml version=\"1.0\"><body>"
          "<outline xmlUrl=\"http://a.example/feed\"/>"
          "<outline xmlUrl=\"http://b.example/feed\"/>"
          "</body></opml>")
  (elfeed--parse-opml (xml-parse-region)))
"####,
        expect![[r#"OK ("http://a.example/feed" "http://b.example/feed")"#]],
    )
}

/// Drive `elfeed-parse-simple-iso-8601' and decode the resulting timestamp.
fn parse_iso8601_decodes_timestamp() -> ParityBatchCase {
    ParityBatchCase::value(
        "parse_iso8601_decodes_timestamp",
        r####"
(decode-time (elfeed-parse-simple-iso-8601 "2024-01-15T10:30:00Z"))
"####,
        expect![[r#"OK (0 30 10 15 1 2024 1 nil 0)"#]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        feed_type_classifies_atom_and_rss(),
        parse_opml_extracts_feed_urls(),
        parse_iso8601_decodes_timestamp(),
    ]
}
