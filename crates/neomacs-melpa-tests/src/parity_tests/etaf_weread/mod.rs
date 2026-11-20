//! Behavioural parity for the pinned `etaf-weread` reader.
//!
//! Every probe drives the reader through the interfaces declared by the
//! package with a scripted client, so no network, login or graphical frame is
//! involved.  The local SQLite index scenarios are deliberately out of scope
//! here: the attested GNU oracle is not built with SQLite.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'etaf)
(require 'etaf-weread)

(defvar neomacs-weread-requests nil
  "Requests the scripted client received, newest first.")
(defvar neomacs-weread-fail nil
  "When non-nil, every scripted request fails with this message.")

(defun neomacs-weread-books (&optional prefix)
  "Return the two scripted books named with PREFIX."
  (cl-loop for index from 1 to 2
           collect (list :bookId (number-to-string index)
                         :title (format "%s书%d" (or prefix "") index)
                         :author (format "作者%d" index)
                         :progress (if (= index 1) 36 0)
                         :readingTime (if (= index 1) 4384 0))))

(defun neomacs-weread-notes (book-id title)
  "Return the notes reply of BOOK-ID named TITLE."
  (list :ok t
        :data (list :book (list :bookId book-id :title title)
                    :highlights (list (list :bookmarkId (format "%s-b1" book-id)
                                            :chapterName "第一章" :range "1-2"
                                            :markText (format "%s的划线" title)
                                            :createTime 100))
                    :reviews (append
                              (list (list :reviewId (format "%s-r1" book-id)
                                          :range "1-2" :content "想法内容"
                                          :createTime 101))
                              (when (equal book-id "2")
                                (list (list :reviewId (format "%s-r2" book-id)
                                            :range "9-9" :content "没有划线的想法"
                                            :createTime 102)))))))

(defun neomacs-weread-client (request callback)
  "Answer REQUEST on CALLBACK from the scripted account."
  (push request neomacs-weread-requests)
  (if neomacs-weread-fail
      (funcall callback (list :error neomacs-weread-fail :hint "scripted"))
    (funcall callback
             (list :ok
                   (pcase (plist-get request :op)
                     (:status (list :ok t :login "ok" :name "测试读者" :vid "1"))
                     (:login (list :ok t :login "ok" :name "测试读者" :vid "1"
                                   :conclusion "登录成功：测试读者"))
                     (:logout (list :ok t :login "out"
                                    :conclusion "已退出微信读书：本机登录态已删除"))
                     (:shelf (list :ok t :books 2 :reading 1 :finished 0
                                   :data (list :shelf (neomacs-weread-books))))
                     (:notebook (list :ok t :books 2 :notes 2 :reviews 2
                                      :data (list :books
                                                  (mapcar
                                                   (lambda (book)
                                                     (list :bookId (plist-get book :bookId)
                                                           :title (plist-get book :title)
                                                           :counts (list :notes 1 :reviews 1)))
                                                   (neomacs-weread-books)))))
                     (:search (list :ok t
                                    :data (list :books
                                                (list (list :bookId "9" :title "搜到的书"
                                                            :author "作者" :rating 93)))))
                     (:book (list :ok t
                                  :data (list :book (list :bookId (plist-get request :id)
                                                          :title (format "详情节%s" (plist-get request :id))
                                                          :author "作者" :category "文学"
                                                          :counts (list :notes 1 :reviews 1)
                                                          :intro "简介")
                                              :progress (list :progress 36 :readingTime 4384)
                                              :chapters (list (list :chapterIdx 1 :title "第一章"
                                                                    :wordCount 100)
                                                              (list :chapterIdx 2 :title "第二章"
                                                                    :wordCount 200)))))
                     (:notes (neomacs-weread-notes
                              (plist-get request :id)
                              (format "详情节%s" (plist-get request :id))))
                     (:stats (list :ok t :books 2 :finished 0 :reading 1 :notes 2
                                   :total_reading_seconds 4384
                                   :total_reading_time "1小时13分"
                                   :conclusion "读完 0 本 · 在读 1 本 · 累计 1小时13分"
                                   :data (list :top (neomacs-weread-books)
                                               :finished nil :reading nil)))
                     (:export (list :ok t :conclusion "已导出笔记")))))))

(defun neomacs-weread-fresh ()
  "Reset the scripted client."
  (setq neomacs-weread-requests nil
        neomacs-weread-fail nil))

(defun neomacs-weread-session ()
  "Return a session driven by the scripted client with a disposable settings file."
  (let ((etaf-weread-settings-file
         (make-temp-file "neomacs-weread-settings-" nil ".eld")))
    (etaf-weread-create :client #'neomacs-weread-client)))

(defun neomacs-weread-ops ()
  "Return the recorded requests compactly, oldest first."
  (mapcar (lambda (request)
            (list (plist-get request :op) (plist-get request :id)
                  (plist-get request :keyword)))
          (nreverse neomacs-weread-requests)))

(defun neomacs-weread-facts (app)
  "Return APP's reader state compactly."
  (let ((state (etaf-weread-state app))
        (detail (plist-get (etaf-weread-state app) :detail))
        (notes (plist-get (etaf-weread-state app) :notes)))
    (list :mode (plist-get state :mode)
          :section (plist-get state :section)
          :query (plist-get state :query)
          :selection (plist-get state :selection)
          :rows-total (plist-get state :rows-total)
          :list-total (plist-get state :list-total)
          :items (mapcar (lambda (row)
                           (list (plist-get row :title) (plist-get row :right)))
                         (plist-get state :items))
          :account (plist-get (plist-get state :account) :name)
          :stats (plist-get (plist-get state :stats) :total-time)
          :pane (plist-get state :pane)
          :detail (and detail
                       (list (plist-get detail :id) (plist-get detail :category)
                             (length (plist-get detail :chapters))
                             (plist-get detail :progress)))
          :notes (and notes
                      (list (plist-get notes :highlights)
                            (plist-get notes :thoughts)
                            (mapcar (lambda (group)
                                      (list (plist-get group :chapter)
                                            (mapcar (lambda (item)
                                                      (list (plist-get item :text)
                                                            (plist-get item :thought)))
                                                    (plist-get group :items))))
                                    (plist-get notes :groups))))
          :message (plist-get state :message)
          :error (plist-get state :error))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("etaf-weread", "0.2.5"), "etaf-weread.el")
        .expect("prepare pinned etaf-weread source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn etaf_weread_behavior_batch() {
    let cases = vec![
        // A new session is headless and sends nothing.
        ParityBatchCase::value(
            "headless_session",
            r####"
(progn
  (neomacs-weread-fresh)
  (let ((app (neomacs-weread-session)))
    (list (neomacs-weread-facts app) neomacs-weread-requests)))
"####,
            expect![[
                r#"OK ((:mode browse :section :shelf :query "" :selection 0 :rows-total 0 :list-total 0 :items nil :account nil :stats nil :pane list :detail nil :notes nil :message nil :error nil) nil)"#
            ]],
        ),
        // Refreshing loads the shelf, account and reading statistics.
        ParityBatchCase::value(
            "refresh_loads",
            r####"
(progn
  (neomacs-weread-fresh)
  (let ((app (neomacs-weread-session)))
    (etaf-weread-refresh app)
    (list (neomacs-weread-facts app) (neomacs-weread-ops))))
"####,
            expect![[r#"OK ((:mode browse :section :shelf :query "" :selection 0 :rows-total 2 :list-total 2 :items (("书1" "1小时13分") ("书2" "")) :account "测试读者" :stats "1小时13分" :pane list :detail ("1" nil 0 nil) :notes nil :message "书架 2 本 · 在读 1 · 读完 0" :error nil) ((:shelf nil nil) (:status nil nil) (:stats nil nil)))"#]],
        ),
        // Search trims its keyword and rejects a blank one.
        ParityBatchCase::value(
            "search_trims",
            r####"
(progn
  (neomacs-weread-fresh)
  (let ((app (neomacs-weread-session)))
    (etaf-weread-search app "  三体  ")
    (let ((state (neomacs-weread-facts app)))
      (list state (neomacs-weread-ops)
            (condition-case err
                (etaf-weread-search app "   ")
              (etaf-weread-error (list (car err) (cadr err))))))))
"####,
            expect![[r#"OK ((:mode search :section :search :query "三体" :selection 0 :rows-total 1 :list-total 1 :items (("搜到的书" "")) :account nil :stats nil :pane list :detail ("9" nil 0 nil) :notes nil :message "搜索「三体」· 展示了 1 本" :error nil) ((:search nil "三体")) (etaf-weread-error "搜索关键词不能为空"))"#]],
        ),
        // Selection clamps at both ends, and activating a row opens its
        // detail.
        ParityBatchCase::value(
            "selection_opens_detail",
            r####"
(progn
  (neomacs-weread-fresh)
  (let ((etaf-weread-page-size 2))
    (let ((app (neomacs-weread-session)))
      (etaf-weread-refresh app)
      (etaf-weread-move-selection app 5)
      (let ((bottom (plist-get (etaf-weread-state app) :selection)))
        (etaf-weread-move-selection app -9)
        (let ((top (plist-get (etaf-weread-state app) :selection)))
          (etaf-weread-activate-selection app)
          (list bottom top (neomacs-weread-facts app)))))))
"####,
            expect![[r#"OK (1 0 (:mode browse :section :shelf :query "" :selection 0 :rows-total 2 :list-total 2 :items (("书1" "1小时13分") ("书2" "")) :account "测试读者" :stats "1小时13分" :pane detail :detail ("1" "文学" 2 36) :notes nil :message "详情节1 · 2 章 · 36%" :error nil))"#]],
        ),
        // Showing a book publishes its detail and chapters; a repeat uses
        // the cache without another backend request.
        ParityBatchCase::value(
            "show_book_caches",
            r####"
(progn
  (neomacs-weread-fresh)
  (let ((app (neomacs-weread-session)))
    (etaf-weread-show-book app "7")
    (let ((first (neomacs-weread-facts app))
          (books (length (cl-remove-if-not
                          (lambda (op) (eq op :book)) (neomacs-weread-ops)
                          :key #'car))))
      (etaf-weread-show-book app "7")
      (list first books
            (length (cl-remove-if-not
                     (lambda (op) (eq op :book)) (neomacs-weread-ops)
                     :key #'car))))))
"####,
            expect![[r#"OK ((:mode browse :section :shelf :query "" :selection 0 :rows-total 0 :list-total 0 :items nil :account nil :stats nil :pane detail :detail ("7" "文学" 2 36) :notes nil :message "详情节7 · 2 章 · 36%" :error nil) 1 1)"#]],
        ),
        // Highlights group by chapter; an orphan thought is collected apart.
        ParityBatchCase::value(
            "notes_group",
            r####"
(progn
  (neomacs-weread-fresh)
  (let ((app (neomacs-weread-session)))
    (etaf-weread-show-book app "2" 'notes)
    (neomacs-weread-facts app)))
"####,
            expect![[r#"OK (:mode browse :section :shelf :query "" :selection 0 :rows-total 0 :list-total 0 :items nil :account nil :stats nil :pane notes :detail ("2" "文学" 2 36) :notes (1 2 (("第一章" (("详情节2的划线" "想法内容"))) ("其他想法" (("" "没有划线的想法"))))) :message "「详情节2」划线与想法：3 条" :error nil)"#]],
        ),
        // The side pane toggles between detail and notes; an unknown pane
        // signals without changing the pane.
        ParityBatchCase::value(
            "panes",
            r####"
(progn
  (neomacs-weread-fresh)
  (let ((app (neomacs-weread-session)))
    (etaf-weread-show-book app "2")
    (let ((detail (plist-get (etaf-weread-state app) :pane)))
      (etaf-weread-toggle-pane app)
      (let ((notes (plist-get (etaf-weread-state app) :pane))
            (loaded (and (plist-get (etaf-weread-state app) :notes) t)))
        (etaf-weread-toggle-pane app)
        (list detail notes loaded
              (plist-get (etaf-weread-state app) :pane)
              (condition-case err
                  (etaf-weread-show-pane app 'sideways)
                (etaf-weread-error (list (car err) (cadr err))))
              (plist-get (etaf-weread-state app) :pane))))))
"####,
            expect![[
                r#"OK (detail notes t detail (etaf-weread-error "Unknown pane: sideways") detail)"#
            ]],
        ),
        // Exporting the selected book sends the backend export and reports
        // its reply.
        ParityBatchCase::value(
            "export_notes",
            r####"
(progn
  (neomacs-weread-fresh)
  (let ((app (neomacs-weread-session)))
    (etaf-weread-refresh app)
    (etaf-weread-export-notes app)
    (list (neomacs-weread-ops)
          (plist-get (etaf-weread-state app) :message))))
"####,
            expect![[r#"OK (((:shelf nil nil) (:status nil nil) (:stats nil nil) (:export "1" nil)) "已导出笔记")"#]],
        ),
        // A failing backend reply lands in the status line; the reader stays
        // usable and the next section is empty rather than an error.
        ParityBatchCase::value(
            "backend_failure",
            r####"
(progn
  (neomacs-weread-fresh)
  (setq neomacs-weread-fail "登录态失效")
  (let ((app (neomacs-weread-session)))
    (etaf-weread-refresh app)
    (let ((failed (list (plist-get (etaf-weread-state app) :error)
                        (plist-get (etaf-weread-state app) :message))))
      (etaf-weread-locate app :notebook)
      (setq neomacs-weread-fail nil)
      (list failed (plist-get (etaf-weread-state app) :rows-total)))))
"####,
            expect![[r#"OK (("登录态失效" "登录态失效（scripted）") 0)"#]],
        ),
        // Reading time and the text progress bar keep their documented
        // shapes.
        ParityBatchCase::value(
            "format_duration_and_progress",
            r####"
(list (etaf-weread-format-duration 4384)
      (etaf-weread-format-duration 600)
      (etaf-weread-format-duration 0)
      (etaf-weread-progress-bar 0 4)
      (etaf-weread-progress-bar 100 4)
      (etaf-weread-progress-bar 50 4))
"####,
            expect![[r#"OK ("1小时13分" "10分" "" "░░░░" "████" "██░░")"#]],
        ),
        // Palette colors resolve per mode, and an unknown color signals.
        ParityBatchCase::value(
            "theme_tokens",
            r####"
(list (etaf-weread-color :bg 'light)
      (etaf-weread-color :bg 'dark)
      (let ((light (etaf-weread-theme-defaults 'light))
            (dark (etaf-weread-theme-defaults 'dark)))
        (list (plist-get light :ui-bg) (plist-get light :ui-fg)
              (plist-get dark :ui-bg) (plist-get dark :ui-fg)))
      (condition-case err
          (etaf-weread-color :not-a-color 'light)
        (etaf-weread-error (list (car err) (cadr err)))))
"####,
            expect![[
                r##"OK ("#FFFFFF" "#191a1c" ("#FFFFFF" "#1C2330" "#191a1c" "#E8ECF2") (etaf-weread-error "Unknown color: :not-a-color"))"##
            ]],
        ),
    ];
    assert_oracle_batch_cases(oracle(), "etaf-weread-behavior", "etaf-weread", &cases);
}
