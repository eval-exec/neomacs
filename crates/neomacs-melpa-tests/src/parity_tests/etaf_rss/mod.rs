//! Behavioural parity for the pinned `etaf-rss` feed reader.
//!
//! The parser, HTML renderer, store and OPML helpers are pure functions, and
//! the reader's service client reads `file://` URLs, so every probe here runs
//! offline: embedded documents exercise the parsers directly, and the session
//! cases subscribe to a temporary local feed through the real store/fetch/
//! parse/merge stack.  No network, server, or credentials are involved.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'etaf)
(require 'etaf-rss)

(defconst neomacs-rss-feed-rss
  "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<rss version=\"2.0\"><channel>
<title>示例周刊</title><link>https://example.com/</link>
<description>科技与生活</description>
<item><title>第一篇</title><link>https://example.com/a1</link>
<guid isPermaLink=\"false\">a1</guid><author>alice</author>
<pubDate>Mon, 02 Oct 2023 15:04:05 GMT</pubDate>
<description>Hello world</description></item>
<item><title>第二篇</title><link>https://example.com/a2</link>
<guid isPermaLink=\"false\">a2</guid>
<pubDate>Tue, 03 Oct 2023 15:04:05 GMT</pubDate>
<description>Another body</description></item>
</channel></rss>")

(defconst neomacs-rss-feed-atom
  "<?xml version=\"1.0\"?>
<feed xmlns=\"http://www.w3.org/2005/Atom\"><title>Atom 示例</title>
<link href=\"https://atom.example/\"/>
<updated>2023-10-03T15:04:05Z</updated>
<entry><title>原子条目</title><id>urn:uuid:1</id>
<link href=\"https://atom.example/1\"/>
<updated>2023-10-02T15:04:05Z</updated>
<summary>atom body</summary></entry></feed>")

(defconst neomacs-rss-feed-json
  "{\"version\":\"https://jsonfeed.org/version/1.1\",
    \"title\":\"JSON 示例\",\"home_page_url\":\"https://json.example/\",
    \"items\":[{\"id\":\"j1\",\"title\":\"JSON 条目\",
                \"url\":\"https://json.example/1\",
                \"content_text\":\"json body\",
                \"date_published\":\"2023-10-02T15:04:05Z\"}]}")

(defconst neomacs-rss-feed-rdf
  "<?xml version=\"1.0\"?>
<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"
         xmlns=\"http://purl.org/rss/1.0/\"
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\">
<channel rdf:about=\"https://rdf.example/\"><title>RDF 示例</title>
<link>https://rdf.example/</link><description>rdf desc</description>
<item rdf:about=\"https://rdf.example/1\"><title>RDF 条目</title>
<link>https://rdf.example/1</link>
<dc:date>2023-10-02T15:04:05Z</dc:date>
<description>rdf body</description></item></channel></rdf:RDF>")

(defconst neomacs-rss-opml
  "<?xml version=\"1.0\"?><opml version=\"2.0\">
<head><title>订阅</title></head><body>
<outline text=\"技术\"><outline text=\"示例周刊\" type=\"rss\"
  xmlUrl=\"https://example.com/feed\" htmlUrl=\"https://example.com/\"/></outline>
<outline text=\"独立博客\" type=\"rss\" xmlUrl=\"https://blog.example/feed\"/>
</body></opml>")

(defconst neomacs-rss-html
  "<html><head><title> 站点标题 </title>
<link rel=\"alternate\" type=\"application/rss+xml\" title=\"RSS\"
      href=\"/feed.xml\">
<link rel=\"alternate\" type=\"application/atom+xml\"
      href=\"https://example.com/atom.xml\"></head>
<body><h1>标题</h1><p>Hello <b>world</b> &amp; everyone.</p>
<ul><li>one</li><li>two</li></ul><blockquote>quote</blockquote>
<pre><code>(x)</code></pre>
<a href=\"https://example.com/x\">链接</a></body></html>")

(defun neomacs-rss-articles (feed)
  "Return FEED's articles compactly."
  (mapcar (lambda (article)
            (list (plist-get article :title)
                  (plist-get article :url)
                  (plist-get article :author)
                  (plist-get article :published)
                  (plist-get article :summary)))
          (plist-get feed :articles)))

(defun neomacs-rss-feed-facts (text)
  "Parse TEXT and return its feed facts compactly."
  (let ((feed (etaf-rss-parse-feed text "https://example.com/feed")))
    (list (plist-get feed :title)
          (plist-get feed :site)
          (plist-get feed :description)
          (neomacs-rss-articles feed))))

(defun neomacs-rss-wait (predicate &optional timeout)
  "Wait until PREDICATE is non-nil for at most TIMEOUT seconds."
  (let ((deadline (+ (float-time) (or timeout 10))))
    (while (and (not (funcall predicate)) (< (float-time) deadline))
      (sit-for 0.01))
    (funcall predicate)))

(defun neomacs-rss-temp-file (prefix suffix content)
  "Write CONTENT to a new temporary file and return its name."
  (let ((file (make-temp-file prefix nil suffix)))
    (with-temp-file file (insert content))
    file))

(defun neomacs-rss-scope-facts (scope)
  "Return SCOPE without backend-specific record ids."
  (list (plist-get scope :kind)
        (plist-get scope :query)
        (plist-get scope :name)))

(defun neomacs-rss-row-titles (app)
  "Return APP's loaded row titles."
  (mapcar (lambda (row) (plist-get row :title))
          (append (plist-get (etaf-rss-state app) :rows) nil)))

(defun neomacs-rss-facts (app)
  "Return APP's reader state compactly."
  (let ((state (etaf-rss-state app)))
    (list :scope (neomacs-rss-scope-facts (plist-get state :scope))
          :title (plist-get state :title)
          :rows (mapcar (lambda (row)
                          (list (plist-get row :title)
                                (plist-get row :read)
                                (plist-get row :starred)))
                        (append (plist-get state :rows) nil))
          :rows-total (plist-get state :rows-total)
          :total (plist-get state :total)
          :unread (plist-get state :unread)
          :error (plist-get state :error))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("etaf-rss", "0.3.0"), "etaf-rss.el")
        .expect("prepare pinned etaf-rss source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn etaf_rss_behavior_batch() {
    let cases = vec![
        // RSS 2.0, Atom and JSON Feed documents become the same article shape.
        ParityBatchCase::value(
            "parse_rss_atom_json",
            r####"
(list (neomacs-rss-feed-facts neomacs-rss-feed-rss)
      (neomacs-rss-feed-facts neomacs-rss-feed-atom)
      (neomacs-rss-feed-facts neomacs-rss-feed-json))
"####,
            expect![[
                r#"OK (("示例周刊" "https://example.com/" "科技与生活" (("第一篇" "https://example.com/a1" "alice" 1696259045.0 "Hello world") ("第二篇" "https://example.com/a2" "" 1696345445.0 "Another body"))) ("Atom 示例" "https://atom.example/" "" (("原子条目" "https://atom.example/1" "" 1696259045.0 "atom body"))) ("JSON 示例" "https://json.example/" "" (("JSON 条目" "https://json.example/1" "" 1696259045.0 "json body"))))"#
            ]],
        ),
        // RSS 1.0 (RDF) is accepted as well.
        ParityBatchCase::value(
            "parse_rdf",
            r####"
(neomacs-rss-feed-facts neomacs-rss-feed-rdf)
"####,
            expect![[
                r#"OK ("RDF 示例" "https://rdf.example/" "rdf desc" (("RDF 条目" "https://rdf.example/1" "" 1696259045.0 "rdf body")))"#
            ]],
        ),
        // OPML outlines become subscriptions with their nearest folder.
        ParityBatchCase::value(
            "parse_opml",
            r####"
(etaf-rss-parse-opml neomacs-rss-opml)
"####,
            expect![[
                r#"OK ((:title "示例周刊" :url "https://example.com/feed" :folder "技术") (:title "独立博客" :url "https://blog.example/feed" :folder ""))"#
            ]],
        ),
        // Article HTML becomes reading blocks and plain text.
        ParityBatchCase::value(
            "render_html_blocks",
            r####"
(list (etaf-rss-render-html neomacs-rss-html)
      (etaf-rss-plain-text neomacs-rss-html))
"####,
            expect![[
                r#"OK (((:kind heading :text "标题") (:kind para :text "Hello world & everyone.") (:kind item :text "one") (:kind item :text "two") (:kind quote :text "quote") (:kind code :text "(x)") (:kind para :text "链接") (:kind separator :text "") (:kind heading :text "链接") (:kind link :text "链接" :url "https://example.com/x")) "标题 Hello world & everyone. one two quote (x) 链接 链接 链接")"#
            ]],
        ),
        // Reading blocks wrap into display lines at the requested width.
        ParityBatchCase::value(
            "wrap_blocks",
            r####"
(let ((blocks (etaf-rss-render-html
               "<p>Hello wonderful world of feeds</p><h2>Heading</h2><p>Second paragraph</p>")))
  (mapcar (lambda (line) (list (plist-get line :kind) (plist-get line :text)))
          (etaf-rss-wrap-blocks blocks 12)))
"####,
            expect![[
                r#"OK ((para "Hello") (para "wonderful") (para "world of feeds") (blank "") (blank "") (heading "Heading") (blank "") (blank "") (para "Second") (para "paragraph"))"#
            ]],
        ),
        // A site page reports its title and its advertised feed URLs.
        ParityBatchCase::value(
            "html_discovery",
            r####"
(list (etaf-rss-html-title neomacs-rss-html)
      (etaf-rss-html-feed-links neomacs-rss-html "https://example.com/page"))
"####,
            expect![[
                r#"OK ("站点标题" ("https://example.com/feed.xml" "https://example.com/atom.xml"))"#
            ]],
        ),
        // Feed dates parse to absolute times, and relative labels are
        // computed against the caller's now.
        ParityBatchCase::value(
            "times",
            r####"
(let* ((rfc (etaf-rss-parse-time "Mon, 02 Oct 2023 15:04:05 GMT"))
       (iso (etaf-rss-parse-time "2023-10-03T15:04:05Z"))
       (plain (etaf-rss-parse-time "2023-10-04 15:04:05"))
       (now 1700000000.0))
  (list (= rfc 1696259045.0)
        (= iso 1696345445.0)
        (numberp plain)
        (etaf-rss-relative-time (+ now 30) now)
        (etaf-rss-relative-time (+ now 7200) now)
        (etaf-rss-relative-time (- now 86400) now)
        (etaf-rss-parse-time "not a date")))
"####,
            expect![[r#"OK (t t t "刚刚" "刚刚" "昨天 22:13" nil)"#]],
        ),
        // The store round-trips through JSON, prunes per feed, and a corrupt
        // file fails open without overwriting it.
        ParityBatchCase::value(
            "store_roundtrip",
            r####"
(let* ((file (make-temp-file "neomacs-rss-store-" nil ".json"))
       (store (etaf-rss-store-default)))
  (unwind-protect
      (progn
        (setf (alist-get 'feeds store)
              (vector (list (cons 'id "f1")
                            (cons 'url "https://example.com/feed")
                            (cons 'title "示例"))))
        (setf (alist-get 'articles store)
              (vector
               (list (cons 'id "a1") (cons 'feed "f1") (cons 'title "一")
                     (cons 'published 100.0) (cons 'read nil) (cons 'starred nil))
               (list (cons 'id "a2") (cons 'feed "f1") (cons 'title "二")
                     (cons 'published 200.0) (cons 'read t) (cons 'starred t))
               (list (cons 'id "a3") (cons 'feed "f1") (cons 'title "三")
                     (cons 'published 300.0) (cons 'read nil) (cons 'starred nil))))
        (etaf-rss-store-write file store)
        (let* ((read (etaf-rss-store-read file))
               (pruned (etaf-rss-store-prune read 2 10)))
          (list (alist-get 'schema read)
                (mapcar (lambda (feed) (alist-get 'title feed))
                        (append (alist-get 'feeds read) nil))
                (mapcar (lambda (article) (alist-get 'title article))
                        (append (alist-get 'articles read) nil))
                (mapcar (lambda (article) (alist-get 'title article))
                        (append (alist-get 'articles pruned) nil)))))
    (ignore-errors (delete-file file))))
"####,
            expect![[r#"OK (1 ("示例") ("三" "二") ("三" "二"))"#]],
        ),
        // A corrupt store file is renamed aside and an empty store returned.
        ParityBatchCase::value(
            "store_corrupt_fails_open",
            r####"
(let ((file (neomacs-rss-temp-file "neomacs-rss-bad-" ".json" "not-json")))
  (unwind-protect
      (let ((store (etaf-rss-store-read file)))
        (list (alist-get 'schema store)
              (length (append (alist-get 'feeds store) nil))
              (length (append (alist-get 'articles store) nil))
              (and etaf-rss-store-error t)
              (file-exists-p (concat file ".bad"))))
    (ignore-errors (delete-file file))
    (ignore-errors (delete-file (concat file ".bad")))))
"####,
            expect!["OK (1 0 0 t t)"],
        ),
        // Subscribing to a local feed loads its articles; search trims and a
        // blank keyword signals without querying.
        ParityBatchCase::value(
            "session_subscribe_search",
            r####"
(let* ((feed (neomacs-rss-temp-file "neomacs-rss-feed-" ".xml"
                                    neomacs-rss-feed-rss))
       (data (make-temp-file "neomacs-rss-data-" nil ".json")))
  (delete-file data)
  (unwind-protect
      (let ((app (etaf-rss-create :client (etaf-rss-make-client :data-file data))))
        (etaf-rss-add-feed app (concat "file://" feed))
        (neomacs-rss-wait
         (lambda () (> (length (append (plist-get (etaf-rss-state app) :rows) nil)) 0)))
        (let ((loaded (neomacs-rss-facts app)))
          (etaf-rss-search app "  第一篇 ")
          (neomacs-rss-wait
           (lambda () (equal (neomacs-rss-row-titles app) '("第一篇"))))
          (list loaded
                (neomacs-rss-facts app)
                (condition-case err
                    (etaf-rss-search app "   ")
                  (etaf-rss-error (list (car err) (cadr err)))))))
    (ignore-errors (delete-file feed))
    (ignore-errors (delete-file data))))
"####,
            expect![[
                r#"OK ((:scope (:feed nil nil) :title "示例周刊" :rows (("第二篇" nil nil) ("第一篇" nil nil)) :rows-total 2 :total 2 :unread 2 :error nil) (:scope (:search "第一篇" nil) :title "搜索：第一篇" :rows (("第一篇" nil nil)) :rows-total 1 :total 2 :unread 2 :error nil) (etaf-rss-error "搜索关键词不能为空"))"#
            ]],
        ),
        // Reading and starring the selected article update the row, and the
        // unread and starred scopes then filter by them.
        ParityBatchCase::value(
            "session_read_star_scopes",
            r####"
(let* ((feed (neomacs-rss-temp-file "neomacs-rss-feed-" ".xml"
                                    neomacs-rss-feed-rss))
       (data (make-temp-file "neomacs-rss-data-" nil ".json")))
  (delete-file data)
  (unwind-protect
      (let ((app (etaf-rss-create :client (etaf-rss-make-client :data-file data))))
        (etaf-rss-add-feed app (concat "file://" feed))
        (neomacs-rss-wait
         (lambda () (> (length (append (plist-get (etaf-rss-state app) :rows) nil)) 0)))
        (etaf-rss-toggle-star app)
        (etaf-rss-toggle-read app)
        (neomacs-rss-wait
         (lambda ()
           (let ((row (car (append (plist-get (etaf-rss-state app) :rows) nil))))
             (and (plist-get row :read) (plist-get row :starred)))))
        (let ((after (neomacs-rss-facts app)))
          (etaf-rss-locate app :unread)
          (neomacs-rss-wait
           (lambda () (equal (neomacs-rss-row-titles app) '("第一篇"))))
          (let ((unread (neomacs-rss-facts app)))
            (etaf-rss-locate app :starred)
            (neomacs-rss-wait
             (lambda () (equal (neomacs-rss-row-titles app) '("第二篇"))))
            (list after unread (neomacs-rss-facts app)))))
    (ignore-errors (delete-file feed))
    (ignore-errors (delete-file data))))
"####,
            expect![[
                r#"OK ((:scope (:feed nil nil) :title "示例周刊" :rows (("第二篇" t t) ("第一篇" nil nil)) :rows-total 2 :total 2 :unread 2 :error nil) (:scope (:unread nil nil) :title "未读" :rows (("第一篇" nil nil)) :rows-total 1 :total 2 :unread 1 :error nil) (:scope (:starred nil nil) :title "收藏" :rows (("第二篇" t t)) :rows-total 1 :total 2 :unread 1 :error nil))"#
            ]],
        ),
    ];
    assert_oracle_batch_cases(oracle(), "etaf-rss-behavior", "etaf-rss", &cases);
}
