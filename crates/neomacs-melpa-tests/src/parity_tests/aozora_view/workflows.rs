use expect_test::expect;

use super::ParityBatchCase;

fn reading_an_annotated_aozora_book_builds_a_linked_read_only_view() -> ParityBatchCase {
    ParityBatchCase::value(
        "reading_an_annotated_aozora_book_builds_a_linked_read_only_view",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "aozora-reading-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (book (expand-file-name "wagahai.txt" root))
       (default-directory root)
       source
       view
       result)
  (unwind-protect
      (progn
        (neomacs-aozora-test-cleanup root)
        (make-directory root t)
        (with-temp-file book
          (insert
           "吾輩は猫である\n"
           "夏目漱石\n\n"
           "｜吾輩《わがはい》は猫である。名前はまだ無い。\n"
           "空を見上げる※［＃雪だるま、UCS-2603、1-1］。\n"
           "強調［＃「強調」に傍線］と注意［＃「注意」に白丸傍点］。\n"
           "漢［＃レ］文と［＃（小さな注）］を読む。\n"
           "前［＃ここから割り注］補足説明［＃ここで割り注終わり］後。\n"))
        (setq source (find-file-noselect book))
        (switch-to-buffer source)
        (text-mode)
        (let ((aozora-fill-column 48)
              (aozora-view-save-cache nil))
          (call-interactively #'aozora-view))
        (setq view (current-buffer))
        (setq result
              (list
               :source
               (list
                :file (file-relative-name
                       (buffer-file-name source)
                       root)
                :mode (buffer-local-value 'major-mode source)
                :content
                (with-current-buffer source
                  (buffer-substring-no-properties
                   (point-min)
                   (point-max)))
                :modified
                (with-current-buffer source
                  (buffer-modified-p)))
               :view
               (list
                :name (buffer-name view)
                :mode major-mode
                :mode-name mode-name
                :read-only buffer-read-only
                :view-mode view-mode
                :line-spacing line-spacing
                :content
                (buffer-substring-no-properties
                 (point-min)
                 (point-max))
                :tokens
                (mapcar
                 #'neomacs-aozora-test-token-state
                 '("吾輩は猫である"
                   "わがはい"
                   "☃"
                   "強調"
                   "注意"
                   "レ"
                   "小さな注"
                   "補足説明"))
                :linked-source
                (eq aozora-view-text-buffer source)
                :linked-file
                (file-relative-name
                 aozora-view-text-file
                 root)
                :source-links-back
                (eq
                 (buffer-local-value
                  'aozora-view-buffer
                  source)
                 view)))))
    (neomacs-aozora-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:source (:file "wagahai.txt" :mode text-mode :content "吾輩は猫である\n夏目漱石\n\n｜吾輩《わがはい》は猫である。名前はまだ無い。\n空を見上げる※［＃雪だるま、UCS-2603、1-1］。\n強調［＃「強調」に傍線］と注意［＃「注意」に白丸傍点］。\n漢［＃レ］文と［＃（小さな注）］を読む。\n前［＃ここから割り注］補足説明［＃ここで割り注終わり］後。\n" :modified nil) :view (:name "wagahai" :mode aozora-view-mode :mode-name "青空文庫" :read-only t :view-mode t :line-spacing 0 :content "\n吾輩は猫である\n\n夏目漱石\n\n\nわがはい\n吾輩は猫である。名前はまだ無い。\n\n空を見上げる☃。\n　　　　　　  ○   ○\n強調と注意。\n\n漢レ文と小さな注を読む。\n\n前補足説明後。\n" :tokens ((:text "吾輩は猫である" :position 2 :line-number 1 :display nil :face nil :read-only nil) (:text "わがはい" :position 18 :line-number nil :display ((height 0.5)) :face nil :read-only nil) (:text "☃" :position 47 :line-number nil :display nil :face nil :read-only nil) (:text "強調" :position 64 :line-number 6 :display nil :face underline :read-only nil) (:text "注意" :position 67 :line-number nil :display nil :face nil :read-only nil) (:text "レ" :position 73 :line-number nil :display ((height 0.5)) :face nil :read-only nil) (:text "小さな注" :position 76 :line-number nil :display ((height 0.5) (raise 1)) :face nil :read-only nil) (:text "補足説明" :position 87 :line-number nil :display ((height 0.5) (raise 0.5)) :face nil :read-only nil)) :linked-source t :linked-file "wagahai.txt" :source-links-back t))"#
        ]],
    )
}

fn bookmarking_redrawing_and_reopening_resumes_from_the_real_gzip_cache() -> ParityBatchCase {
    ParityBatchCase::value(
        "bookmarking_redrawing_and_reopening_resumes_from_the_real_gzip_cache",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "aozora-resume-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (book (expand-file-name "chapters.txt" root))
       (cache-root
        (file-name-as-directory
         (expand-file-name "cache" root)))
       (default-directory root)
       source
       first-view
       result)
  (unwind-protect
      (progn
        (neomacs-aozora-test-cleanup root)
        (make-directory root t)
        (with-temp-file book
          (insert
           "第一章\n"
           "｜旅人《たびびと》は朝に出発した。\n\n"
           "第二章\n"
           "旅人は青空の下で手紙を読んだ。\n\n"
           "第三章\n"
           "旅人は夕暮れに町へ戻った。\n"))
        (setq aozora-view-bookmarks nil
              source (find-file-noselect book))
        (switch-to-buffer source)
        (text-mode)
        (let ((aozora-fill-column 44)
              (aozora-view-cache-directory cache-root)
              (aozora-view-save-cache t))
          (call-interactively #'aozora-view)
          (setq first-view (current-buffer))
          (goto-char (point-min))
          (search-forward "第二章")
          (let ((bookmark-message
                 (call-interactively
                  (key-binding "b"))))
            (switch-to-buffer source)
            (goto-char (point-max))
            (insert "追記\n旅人は翌朝の予定を書き留めた。\n")
            (save-buffer)
              (switch-to-buffer first-view)
            (let ((aozora-view-cache-directory cache-root)
                  (aozora-view-save-cache t)
                  (aozora-fill-column 44))
              (call-interactively
               (key-binding "l")))
            (let* ((cache-file
                    (let ((aozora-view-cache-directory cache-root))
                      (aozora-view-cache-file book)))
                   (cache-magic
                   (with-temp-buffer
                      (set-buffer-multibyte nil)
                      (insert-file-contents-literally cache-file)
                      (list
                       :gzip-header-valid
                       (and
                        (= (char-after 1) 31)
                        (= (char-after 2) 139))
                       :has-payload
                       (> (buffer-size) 2))))
                   (after-redraw
                    (list
                     :content
                     (buffer-substring-no-properties
                      (point-min)
                      (point-max))
                     :point (point)
                     :line (line-number-at-pos)
                     :line-text
                     (buffer-substring-no-properties
                      (line-beginning-position)
                      (line-end-position))
                     :bookmark
                     (lax-plist-get
                      aozora-view-bookmarks
                      (expand-file-name book))
                     :cache-magic cache-magic)))
              (setq buffer-read-only nil)
              (kill-buffer first-view)
              (with-current-buffer source
                (set-buffer-modified-p nil)
                (kill-buffer source))
              (setq source (find-file-noselect book))
              (switch-to-buffer source)
              (text-mode)
              (let ((aozora-fill-column 44)
                    (aozora-view-cache-directory cache-root)
                    (aozora-view-save-cache t))
                (call-interactively #'aozora-view))
              (setq result
                    (list
                     :bookmark-message bookmark-message
                     :after-redraw after-redraw
                     :reopened
                     (list
                      :mode major-mode
                      :read-only buffer-read-only
                      :content
                      (buffer-substring-no-properties
                       (point-min)
                       (point-max))
                      :point (point)
                      :line (line-number-at-pos)
                      :line-text
                      (buffer-substring-no-properties
                       (line-beginning-position)
                       (line-end-position))
                      :same-rendering
                      (equal
                       (plist-get after-redraw :content)
                       (buffer-substring-no-properties
                        (point-min)
                        (point-max)))
                      :linked-source
                      (eq aozora-view-text-buffer source))))))))
    (neomacs-aozora-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:bookmark-message "Bookmarked!" :after-redraw (:content "\n第一章\nたびびと\n旅人は朝に出発した。\n\n\n\n第二章\n\n旅人は青空の下で手紙を読んだ。\n\n\n\n第三章\n\n旅人は夕暮れに町へ戻った。\n\n追記\n\n旅人は翌朝の予定を書き留めた。\n" :point 25 :line 8 :line-text "第二章" :bookmark 4 :cache-magic (:gzip-header-valid t :has-payload t)) :reopened (:mode aozora-view-mode :read-only t :content "\n第一章\nたびびと\n旅人は朝に出発した。\n\n\n\n第二章\n\n旅人は青空の下で手紙を読んだ。\n\n\n\n第三章\n\n旅人は夕暮れに町へ戻った。\n\n追記\n\n旅人は翌朝の予定を書き留めた。\n" :point 25 :line 8 :line-text "第二章" :same-rendering t :linked-source t))"#
        ]],
    )
}

fn redrawing_after_closing_a_cp932_source_buffer_recovers_the_japanese_book() -> ParityBatchCase {
    ParityBatchCase::value(
        "redrawing_after_closing_a_cp932_source_buffer_recovers_the_japanese_book",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "aozora-cp932-recovery"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (book (expand-file-name "classic.txt" root))
       (default-directory root)
       source
       view
       result)
  (unwind-protect
      (progn
        (neomacs-aozora-test-cleanup root)
        (make-directory root t)
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert
           (apply
            #'unibyte-string
            '(#x8c #xe1 #x94 #x79 #x82 #xcd #x94 #x4c
              #x82 #xc5 #x82 #xa0 #x82 #xe9 #x0d #x0a
              #x96 #xbc #x91 #x4f #x82 #xcd #x82 #xdc
              #x82 #xbe #x96 #xb3 #x82 #xa2 #x81 #x42
              #x0d #x0a)))
          (let ((coding-system-for-write 'no-conversion))
            (write-region
             (point-min)
             (point-max)
             book nil 'silent)))
        (setq source (find-file-noselect book))
        (switch-to-buffer source)
        (text-mode)
        (let ((aozora-fill-column 40)
              (aozora-view-save-cache nil))
          (call-interactively #'aozora-view))
        (setq view (current-buffer))
        (let ((initial-content
               (buffer-substring-no-properties
                (point-min)
                (point-max)))
              (initial-mode major-mode)
              (initial-read-only buffer-read-only)
              (linked-source
               (eq aozora-view-text-buffer source))
              (source-content
               (with-current-buffer source
                 (buffer-substring-no-properties
                  (point-min)
                  (point-max))))
              (source-coding
               (buffer-local-value
                'buffer-file-coding-system
                source)))
          (with-current-buffer source
            (set-buffer-modified-p nil)
            (kill-buffer source))
          (switch-to-buffer view)
          (let ((aozora-fill-column 40)
                (aozora-view-save-cache nil))
            (call-interactively #'aozora-view-redraw))
          (let ((redrawn-content
                 (buffer-substring-no-properties
                  (point-min)
                  (point-max))))
            (setq result
                  (list
                   :source
                   (list
                    :content source-content
                    :coding source-coding)
                   :initial-view
                   (list
                    :mode initial-mode
                    :read-only initial-read-only
                    :content initial-content
                    :decoded-correctly
                    (equal
                     initial-content
                     "\n吾輩は猫である\n\n名前はまだ無い。\n")
                    :linked-source linked-source)
                   :redrawn-view
                   (list
                    :mode major-mode
                    :read-only buffer-read-only
                    :content redrawn-content
                    :decoded-correctly
                    (equal
                     redrawn-content
                     "\n吾輩は猫である\n\n名前はまだ無い。\n")
                    :source-buffer-live
                    (buffer-live-p aozora-view-text-buffer)
                    :source-file
                    (file-relative-name
                     aozora-view-text-file
                     root)
                    :modified
                    (buffer-modified-p)))))))
    (neomacs-aozora-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:source (:content "吾輩は猫である\n名前はまだ無い。\n" :coding japanese-shift-jis-dos) :initial-view (:mode aozora-view-mode :read-only t :content "\n吾輩は猫である\n\n名前はまだ無い。\n" :decoded-correctly t :linked-source t) :redrawn-view (:mode aozora-view-mode :read-only t :content "\n吾輩は猫である\n\n名前はまだ無い。\n" :decoded-correctly t :source-buffer-live nil :source-file "classic.txt" :modified nil))"#
        ]],
    )
}

fn reading_a_book_with_decomposed_western_accents_reports_the_missing_normalizer() -> ParityBatchCase
{
    ParityBatchCase::signal(
        "reading_a_book_with_decomposed_western_accents_reports_the_missing_normalizer",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "aozora-accent-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (book (expand-file-name "foreign-names.txt" root))
       (default-directory root)
       source)
  (unwind-protect
      (progn
        (neomacs-aozora-test-cleanup root)
        (make-directory root t)
        (with-temp-file book
          (insert
           "欧文の人名\n\n"
           "〔〕はアクセント分解記号\n"
           "主人公は〔xCafe'〕で〔xAE&neas〕と会った。\n"
           "記号は〔x?!@〕として記された。\n"))
        (setq source (find-file-noselect book))
        (switch-to-buffer source)
        (text-mode)
        (let ((aozora-fill-column 50)
              (aozora-view-save-cache nil))
          (call-interactively #'aozora-view)))
    (neomacs-aozora-test-cleanup root))
  'unexpected-success)
"####,
        expect!["ERR (void-function ucs-normalize-NFC-region)"],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        reading_an_annotated_aozora_book_builds_a_linked_read_only_view(),
        bookmarking_redrawing_and_reopening_resumes_from_the_real_gzip_cache(),
        redrawing_after_closing_a_cp932_source_buffer_recovers_the_japanese_book(),
        reading_a_book_with_decomposed_western_accents_reports_the_missing_normalizer(),
    ]
}
