//! Behavioural parity for the pinned `etaf-bili` Bilibili client.
//!
//! The client exposes `etaf-bili-session-create :client`, a scripted
//! `(REQUEST CALLBACK)` backend seam, and its own acceptance suite drives the
//! model through it.  These probes use the same seam, so no network, login or
//! subprocess is involved; the pure WBI signing and response parsing contracts
//! are asserted on fixed inputs.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'json)
(require 'etaf)
(require 'etaf-bili)

(defvar neomacs-bili-requests nil
  "Requests the fake backend received, newest first.")
(defvar neomacs-bili-script nil
  "Alist of op symbol to reply plist the fake backend answers with.")

(defun neomacs-bili-fake (request callback)
  "Fake backend: record REQUEST and answer from the scripted table."
  (push request neomacs-bili-requests)
  (funcall callback
           (or (cdr (assq (plist-get request :op) neomacs-bili-script))
               (list :error "unscripted request"))))

(defun neomacs-bili-fresh ()
  "Reset the fake backend."
  (setq neomacs-bili-requests nil
        neomacs-bili-script nil))

(defun neomacs-bili-session ()
  "Return a session wired to the fake backend."
  (etaf-bili-session-create :client #'neomacs-bili-fake))

(defun neomacs-bili-facts (session)
  "Return SESSION's state compactly."
  (let ((state (etaf-bili-session-state session)))
    (list :mode (plist-get state :mode)
          :status (plist-get state :status)
          :loading (plist-get state :loading)
          :selected (plist-get state :selected)
          :items (mapcar (lambda (row)
                           (list (plist-get row :bvid) (plist-get row :title)
                                 (plist-get row :author) (plist-get row :duration)))
                         (plist-get state :items)))))

(defun neomacs-bili-ops ()
  "Return the recorded requests, oldest first."
  (mapcar (lambda (request)
            (list (plist-get request :op) (plist-get request :bvid)
                  (plist-get request :keyword)))
          (nreverse neomacs-bili-requests)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("etaf-bili", "0.1.0"), "etaf-bili.el")
        .expect("prepare pinned etaf-bili source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn etaf_bili_behavior_batch() {
    let cases = vec![
        // Loading the popular feed replaces the rows, reports the count and
        // selects the first row.
        ParityBatchCase::value(
            "popular_loads_rows",
            r####"
(progn
  (neomacs-bili-fresh)
  (setq neomacs-bili-script
        '((:popular :ok ((:bvid "BV1" :title "第一条")
                         (:bvid "BV2" :title "第二条")))))
  (let ((session (neomacs-bili-session)))
    (etaf-bili-load-popular session)
    (list (neomacs-bili-facts session)
          (plist-get (etaf-bili-selected-row session) :title)
          (neomacs-bili-ops))))
"####,
            expect![[
                r#"OK ((:mode popular :status "热门 · 2 条" :loading nil :selected 0 :items (("BV1" "第一条" nil nil) ("BV2" "第二条" nil nil))) "第一条" ((:popular nil nil)))"#
            ]],
        ),
        // Search sends its keyword and replaces the rows; blank input sends
        // nothing and leaves the rows alone.
        ParityBatchCase::value(
            "search_keyword_and_blank",
            r####"
(progn
  (neomacs-bili-fresh)
  (setq neomacs-bili-script
        '((:search :ok ((:bvid "BV9" :title "结果一")))))
  (let ((session (neomacs-bili-session)))
    (etaf-bili-search session "Emacs 配置")
    (let ((before (neomacs-bili-facts session))
          (requests (neomacs-bili-ops)))
      (etaf-bili-search session "   ")
      (list before requests (neomacs-bili-ops)
            (neomacs-bili-facts session)))))
"####,
            expect![[
                r#"OK ((:mode search :status "搜索 Emacs 配置 · 1 条" :loading nil :selected 0 :items (("BV9" "结果一" nil nil))) ((:search nil "Emacs 配置")) ((:search nil "Emacs 配置")) (:mode search :status "搜索 Emacs 配置 · 1 条" :loading nil :selected 0 :items (("BV9" "结果一" nil nil))))"#
            ]],
        ),
        // Selection moves by one and clamps at both ends.
        ParityBatchCase::value(
            "selection_clamps",
            r####"
(progn
  (neomacs-bili-fresh)
  (setq neomacs-bili-script
        '((:popular :ok ((:bvid "BV1" :title "一")
                         (:bvid "BV2" :title "二")
                         (:bvid "BV3" :title "三")))))
  (let ((session (neomacs-bili-session)))
    (etaf-bili-load-popular session)
    (etaf-bili-move session 1)
    (let ((one (plist-get (etaf-bili-session-state session) :selected)))
      (etaf-bili-move session 5)
      (let ((last (plist-get (etaf-bili-session-state session) :selected)))
        (etaf-bili-move session -9)
        (list one last
              (plist-get (etaf-bili-session-state session) :selected)
              (plist-get (etaf-bili-selected-row session) :title))))))
"####,
            expect![[r#"OK (1 2 0 "一")"#]],
        ),
        // Playing the selected row hands the stream source to the caller,
        // reports the title, and reuses the resolved stream.
        ParityBatchCase::value(
            "play_delivers_stream",
            r####"
(progn
  (neomacs-bili-fresh)
  (setq neomacs-bili-script
        '((:popular :ok ((:bvid "BV1" :title "视频")))
          (:play :ok (:url "https://cdn.example/clip.mp4"
                           :bvid "BV1" :title "视频" :duration 61
                           :headers (("Referer" . "https://www.bilibili.com"))))))
  (let ((session (neomacs-bili-session))
        received again)
    (etaf-bili-load-popular session)
    (etaf-bili-play-selected session (lambda (source) (setq received source)))
    (let ((first (list (plist-get received :url)
                       (cdr (assoc "Referer" (plist-get received :headers)))
                       (plist-get (etaf-bili-session-state session) :status)
                       (neomacs-bili-ops))))
      (etaf-bili-play-selected session (lambda (source) (setq again source)))
      (list first (plist-get again :url) (neomacs-bili-ops)))))
"####,
            expect![[
                r#"OK (("https://cdn.example/clip.mp4" "https://www.bilibili.com" "播放：视频" ((:popular nil nil) (:play "BV1" nil))) "https://cdn.example/clip.mp4" ((:play "BV1" nil)))"#
            ]],
        ),
        // A failed stream lookup reports on the status line and plays
        // nothing.
        ParityBatchCase::value(
            "play_failure_status",
            r####"
(progn
  (neomacs-bili-fresh)
  (setq neomacs-bili-script
        '((:popular :ok ((:bvid "BV1" :title "视频")))
          (:play :error "Bilibili error -404: 啥都木有")))
  (let ((session (neomacs-bili-session))
        called)
    (etaf-bili-load-popular session)
    (etaf-bili-play-selected session (lambda (_source) (setq called t)))
    (list called (plist-get (etaf-bili-session-state session) :status))))
"####,
            expect![[r#"OK (nil "播放失败：Bilibili error -404: 啥都木有")"#]],
        ),
        // WBI signing sorts keys, encodes like encodeURIComponent and appends
        // the signature; a missing key is an error.
        ParityBatchCase::value(
            "wbi_signing",
            r####"
(list
 (etaf-bili-wbi-query
  '(("search_type" . "video") ("keyword" . "Emacs 配置") ("page" . "1"))
  "0123456789abcdef0123456789abcdef"
  "1789000000")
 (condition-case err
     (etaf-bili-wbi-query '(("page" . "1")) nil "1")
   (etaf-bili-error (list (car err) (cadr err)))))
"####,
            expect![[
                r#"OK ("keyword=Emacs%20%E9%85%8D%E7%BD%AE&page=1&search_type=video&wts=1789000000&w_rid=ed67f5c8870ac325a3443bbe94a655db" (etaf-bili-error "WBI keys are not available"))"#
            ]],
        ),
        // Response parsing normalizes both the popular and search shapes.
        ParityBatchCase::value(
            "parse_videos",
            r####"
(let* ((popular (json-parse-string
                 (concat "{\"list\":[{\"bvid\":\"BV1\",\"aid\":42,"
                         "\"title\":\"【标题】<em class=\\\"keyword\\\">Emacs</em>\","
                         "\"owner\":{\"name\":\"UP\"},\"stat\":{\"view\":123456},"
                         "\"duration\":754,\"pubdate\":1791808200}]}")
                 :object-type 'alist :array-type 'list))
       (search (json-parse-string
                (concat "{\"result\":[{\"bvid\":\"BV2\",\"title\":\"结果\","
                        "\"author\":\"作者\",\"duration\":\"12:34\",\"play\":\"1.2万\"}]}")
                :object-type 'alist :array-type 'list)))
  (list (etaf-bili-parse-videos popular)
        (etaf-bili-parse-videos search)))
"####,
            expect![[
                r#"OK (((:bvid "BV1" :aid 42 :title "【标题】Emacs" :author "UP" :duration 754 :views "123456" :date "2026-10-12" :cover "")) ((:bvid "BV2" :aid 0 :title "结果" :author "作者" :duration 754 :views "1.2万" :date "" :cover "")))"#
            ]],
        ),
    ];
    assert_oracle_batch_cases(oracle(), "etaf-bili-behavior", "etaf-bili", &cases);
}
