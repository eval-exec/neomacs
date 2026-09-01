use expect_test::expect;

use super::ParityBatchCase;

/// The headline command with nothing cached: `M-x adafruit-wisdom' has to fetch
/// the feed and show a quote in the echo area.  This pins the request the
/// package makes -- a plain `GET' of the feed path with the headers the
/// transport adds -- the quote that reaches the echo area, the return value,
/// the feed stored verbatim in the cache file, and that the buffer the user was
/// in is untouched.  `request' picks its transport from whether curl is
/// installed, so both are exercised: the same fetch is repeated with an empty
/// cache over curl and must produce the same cached feed.
fn fetching_a_quote_asks_the_feed_and_shows_it_in_the_echo_area() -> ParityBatchCase {
    ParityBatchCase::value(
        "fetching_a_quote_asks_the_feed_and_shows_it_in_the_echo_area",
        r##"(progn
  (adaw-test-setup)
  (adaw-test-buffer)
  (let ((mark (adaw-test-message-mark))
        (url-result (cl-letf (((symbol-function 'random) (lambda (&optional _n) 0)))
                      (adafruit-wisdom))))
    (list :url-retrieve
          (list :result url-result
                :messages (adaw-test-messages-since mark)
                :headers (adaw-test-headers)
                :cache (adaw-test-cache-contents)
                :buffer (buffer-substring-no-properties (point-min) (point-max))
                :point (point))
          :curl
          (progn
            (adaw-test-forget-cache)
            (setq request-backend 'curl)
            (setq adaw-test-requests nil)
            (list :result (cl-letf (((symbol-function 'random)
                                     (lambda (&optional _n) 0)))
                            (adafruit-wisdom))
                  :request-lines (adaw-test-request-lines)
                  :cache-equal (equal (adaw-test-cache-contents) adaw-test-feed))))))"##,
        expect![[
            r##"OK (:url-retrieve (:result t :messages ("Make it work, then make it beautiful.") :headers (("GET /feed/quotes.xml HTTP/1.1" "MIME-Version: 1.0" "Connection: keep-alive" "Host: 127.0.0.1:<port>" "Accept-encoding: gzip" "Accept: */*" "User-Agent: <editor>")) :cache "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rss version=\"2.0\">\n  <channel>\n    <title>Adafruit Industries quotes</title>\n    <item><title>Make it work, then make it beautiful.</title></item>\n    <item><title>Solder &amp; patience &#8212; na&#239;ve questions win.</title></item>\n    <item><title>Ingénierie: mesure deux fois, coupe une fois.</title></item>\n  </channel>\n</rss>\n" :buffer "notes:\n" :point 8) :curl (:result t :request-lines ("GET /feed/quotes.xml HTTP/1.1") :cache-equal t))"##
        ]],
    )
}

fn the_cached_feed_is_reused_until_its_day_long_ttl_expires() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_cached_feed_is_reused_until_its_day_long_ttl_expires",
        r##"(progn
  (adaw-test-setup)
  (let ((observed nil))
    (adafruit-wisdom)
    (push (list :first-fetch (length (adaw-test-requests))) observed)
    (adafruit-wisdom)
    (adafruit-wisdom)
    (push (list :still-warm (length (adaw-test-requests))) observed)
    (adaw-test-age-cache (* 3600 23))
    (adafruit-wisdom)
    (push (list :within-ttl (length (adaw-test-requests))) observed)
    (adaw-test-age-cache (* 3600 25))
    (adafruit-wisdom)
    (push (list :past-ttl (length (adaw-test-requests))) observed)
    (list :phases (nreverse observed)
          :ttl adafruit-wisdom-cache-ttl
          :cache-path (file-relative-name adafruit-wisdom-cache-file
                                          (expand-file-name "~/")))))"##,
        expect![[
            r##"OK (:phases ((:first-fetch 1) (:still-warm 1) (:within-ttl 1) (:past-ttl 2)) :ttl 86400.0 :cache-path ".emacs.d/adafruit-wisdom.cache")"##
        ]],
    )
    .fresh_process()
}

fn a_prefix_argument_inserts_the_quote_at_point_instead_of_showing_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_prefix_argument_inserts_the_quote_at_point_instead_of_showing_it",
        r##"(progn
  (adaw-test-setup)
  (adaw-test-buffer)
  (goto-char (point-max))
  (adafruit-wisdom)
  (let ((mark (adaw-test-message-mark)))
    (cl-letf (((symbol-function 'random) (lambda (&optional _n) 1)))
      (list :result (adafruit-wisdom '(4))
            :buffer (buffer-substring-no-properties (point-min) (point-max))
            :point (point)
            :modified (buffer-modified-p)
            :messages (adaw-test-messages-since mark)
            :requests (length (adaw-test-requests))))))"##,
        expect![[
            r##"OK (:result t :buffer "notes:\nSolder & patience — naïve questions win." :point 48 :modified t :messages nil :requests 1)"##
        ]],
    )
    .fresh_process()
}

fn entities_and_non_ascii_survive_the_feed_and_the_cache_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "entities_and_non_ascii_survive_the_feed_and_the_cache_file",
        r##"(progn
  (adaw-test-setup)
  (adafruit-wisdom)
  (let ((picks nil))
    (dotimes (index 3)
      (cl-letf (((symbol-function 'random) (lambda (&optional _n) index)))
        (push (adafruit-wisdom-select) picks)))
    (list :picks (nreverse picks)
          :items (length (dom-by-tag (adafruit-wisdom-cached-get) 'item))
          :cache-is-the-raw-feed (equal (adaw-test-cache-contents) adaw-test-feed)
          :requests (length (adaw-test-requests)))))"##,
        expect![[
            r##"OK (:picks ("Make it work, then make it beautiful." "Solder & patience — naïve questions win." "Ingénierie: mesure deux fois, coupe une fois.") :items 3 :cache-is-the-raw-feed t :requests 1)"##
        ]],
    )
    .fresh_process()
}

fn a_quote_containing_a_percent_sign_cannot_be_displayed() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_quote_containing_a_percent_sign_cannot_be_displayed",
        r##"(progn
  (setq adaw-test-body
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<rss version=\"2.0\"><channel><item><title>Ship it 100% &amp; iterate</title></item></channel></rss>
")
  (adaw-test-setup)
  (adaw-test-buffer)
  (let ((mark (adaw-test-message-mark)))
    (list :quote (adafruit-wisdom-select)
          :display (condition-case failure (adafruit-wisdom)
                     (error failure))
          :messages (adaw-test-messages-since mark)
          :buffer (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect![[
            r##"OK (:quote "Ship it 100% & iterate" :display (error "Not enough arguments for format string") :messages nil :buffer "notes:\n")"##
        ]],
    )
    .fresh_process()
}

fn an_error_page_is_cached_as_if_it_were_the_feed() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_error_page_is_cached_as_if_it_were_the_feed",
        r##"(progn
  (setq adaw-test-status "500 Internal Server Error"
        adaw-test-content-type "text/html"
        adaw-test-body "<html><body>upstream is down</body></html>")
  (adaw-test-setup)
  (list :first (condition-case failure (adafruit-wisdom) (error failure))
        :cache (adaw-test-cache-contents)
        :requests (length (adaw-test-requests))
        :second (condition-case failure (adafruit-wisdom) (error failure))
        :requests-after (length (adaw-test-requests))))"##,
        expect![[
            r##"OK (:first (args-out-of-range 0) :cache "<html><body>upstream is down</body></html>" :requests 1 :second (args-out-of-range 0) :requests-after 1)"##
        ]],
    )
    .fresh_process()
}

fn a_refused_connection_reports_the_error_and_writes_no_cache() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_refused_connection_reports_the_error_and_writes_no_cache",
        r##"(progn
  (adaw-test-setup)
  (delete-process adaw-test-server)
  (list :error (condition-case failure (adafruit-wisdom)
                 (error (seq-take failure 3)))
        :cache (adaw-test-cache-exists)))"##,
        expect![[
            r##"OK (:error (file-error "make client process failed" "Connection refused") :cache nil)"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        fetching_a_quote_asks_the_feed_and_shows_it_in_the_echo_area(),
        the_cached_feed_is_reused_until_its_day_long_ttl_expires(),
        a_prefix_argument_inserts_the_quote_at_point_instead_of_showing_it(),
        entities_and_non_ascii_survive_the_feed_and_the_cache_file(),
        a_quote_containing_a_percent_sign_cannot_be_displayed(),
        an_error_page_is_cached_as_if_it_were_the_feed(),
        a_refused_connection_reports_the_error_and_writes_no_cache(),
    ]
}
