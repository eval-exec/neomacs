use std::time::Duration;

use expect_test::expect;

use crate::{ASYNC_MELPA_PIN, CachedMelpaOracle, PYIM_MELPA_PIN, XR_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PYIM_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PYIM_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'pyim)
(require 'pyim-cstring-utils)

(setq pyim-dcache-auto-update nil)
"##;

fn pyim_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PYIM_MELPA_PIN, "pyim.el")
        .expect("prepare pinned pyim source below ./tmp")
        .with_melpa_dependency(ASYNC_MELPA_PIN)
        .expect("prepare pinned async dependency")
        .with_melpa_dependency(XR_MELPA_PIN)
        .expect("prepare pinned xr dependency")
        .with_prelude(PYIM_TEST_PRELUDE)
        .with_timeout(PYIM_TEST_TIMEOUT)
}

fn phonetic_composition_builds_quanpin_shuangpin_and_shape_codes() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((quanpin (pyim-scheme-get 'quanpin))
       (shuangpin (pyim-scheme-get 'pyim-shuangpin))
       (wubi (pyim-scheme-get 'wubi))
       (cangjie (pyim-scheme-get 'cangjie))
       (quanpin-imobjs
        (pyim-imobjs-create "nihaomapengyou" quanpin))
       (shuangpin-imobjs
        (pyim-imobjs-create "nihcmappyy" shuangpin))
       (wubi-imobjs
        (pyim-imobjs-create "aaaabbbbcccc" wubi)))
  (list
   :schemes
   (mapcar
    (lambda (scheme)
      (list
       (pyim-scheme-name scheme)
       (pyim-scheme-code-prefix scheme)
       (and
        (pyim-scheme-xingma-p scheme)
        (pyim-scheme-xingma-code-split-length
         scheme))
       (and
        (pyim-scheme-xingma-p scheme)
        (pyim-scheme-xingma-code-maximum-length
         scheme))))
    (list quanpin shuangpin wubi cangjie))
   :quanpin-imobjs
   (mapcar
    (lambda (imobj)
      (mapcar #'copy-tree imobj))
    quanpin-imobjs)
   :quanpin-codes
   (mapcar
    (lambda (imobj)
      (pyim-codes-create imobj quanpin))
    quanpin-imobjs)
   :shuangpin-imobjs
   (mapcar
    (lambda (imobj)
      (mapcar #'copy-tree imobj))
    shuangpin-imobjs)
   :shuangpin-codes
   (mapcar
    (lambda (imobj)
      (pyim-codes-create imobj shuangpin))
    shuangpin-imobjs)
   :wubi-imobjs wubi-imobjs
   :wubi-codes
   (mapcar
    (lambda (imobj)
      (pyim-codes-create imobj wubi))
    wubi-imobjs)))
"##;
    let expect = expect![[
        r##"OK (:schemes ((quanpin nil nil nil) (pyim-shuangpin nil nil nil) (wubi "wubi/" 4 4) (cangjie "cangjie/" 5 5)) :quanpin-imobjs ((("n" "i" "n" "i") ("h" "ao" "h" "ao") ("m" "a" "m" "a") ("p" "eng" "p" "eng") ("y" "ou" "y" "ou")) (("n" "i" "n" "i") ("h" "ao" "h" "ao") ("m" "a" "m" "a") ("p" "en" "p" "eng") ("y" "ou" "y" "ou"))) :quanpin-codes (("ni" "hao" "ma" "peng" "you") ("ni" "hao" "ma" "pen" "you")) :shuangpin-imobjs ((("n" "i" "n" "i") ("h" "ao" "h" "c") ("m" "a" "m" "a") ("p" "ie" "p" "p") ("y" "un" "y" "y")) (("n" "i" "n" "i") ("h" "ao" "h" "c") ("m" "a" "m" "a") ("p" "ie" "p" "p") ("y" "ong" "y" "y"))) :shuangpin-codes (("ni" "hao" "ma" "pie" "yun") ("ni" "hao" "ma" "pie" "yong")) :wubi-imobjs (("aaaa" "bbbb" "cccc")) :wubi-codes (("wubi/aaaa" "wubi/bbbb" "wubi/cccc")))"##
    ]];
    ParityBatchCase::value(
        "phonetic_composition_builds_quanpin_shuangpin_and_shape_codes",
        elisp_form,
        expect,
    )
}

fn chinese_text_conversion_handles_phrases_polyphony_ascii_and_initials() -> ParityBatchCase {
    let elisp_form = r##"
(cl-letf (((symbol-function 'pyim-dcache-get)
           (lambda (&rest _args) nil)))
  (let ((samples
         '("重庆银行"
           "音乐"
           "长大")))
    (list
     :mixed-release-note
     (mapconcat
      (lambda (segment)
        (if
            (cl-every
             (lambda (character)
               (pyim-pymap-cchar2py-get
                character))
             (string-to-list segment))
            (pyim-cstring-to-pinyin-simple
             segment nil "-" nil)
          segment))
      (pyim-pymap-split-string
       "你好，Neomacs v2 已发布！")
      "")
     :full
     (mapcar
      (lambda (sample)
        (list sample
              (pyim-cstring-to-pinyin
               sample nil "-" t nil)))
      samples)
     :initials
     (mapcar
      (lambda (sample)
        (list sample
              (pyim-cstring-to-pinyin-simple
               sample t "" nil)))
      samples)
     :segmentation
     (pyim-pymap-split-string
      "版本v2已发布，rollback-ready" t)
     :character-codes
     (mapcar
      (lambda (character)
        (cons character
              (pyim-pymap-cchar2py-get character)))
      '("重" "庆" "行" "乐")))))
"##;
    let expect = expect![[
        r##"OK (:mixed-release-note "ni-hao，Neomacs v2 yi-fa-bu！" :full (("重庆银行" ("chong-qing-yin-hang")) ("音乐" ("yin-yue")) ("长大" ("zhang-da"))) :initials (("重庆银行" "cqyh") ("音乐" "yy") ("长大" "zd")) :segmentation ("版" "本" "v2" "已" "发" "布" "，rollback-ready") :character-codes (("重" "zhong" "chong") ("庆" "qing") ("行" "xing" "heng" "hang") ("乐" "yue" "le")))"##
    ]];
    ParityBatchCase::value(
        "chinese_text_conversion_handles_phrases_polyphony_ascii_and_initials",
        elisp_form,
        expect,
    )
}

fn personalized_and_common_dictionaries_produce_ranked_composition_candidates() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((pyim-dcache-backend 'pyim-dhashcache)
       (pyim-enable-shortcode t)
       (pyim-dhashcache-code2word
        (make-hash-table :test #'equal))
       (pyim-dhashcache-shortcode2word
        (make-hash-table :test #'equal))
       (pyim-dhashcache-icode2word
        (make-hash-table :test #'equal))
       (pyim-dhashcache-ishortcode2word
        (make-hash-table :test #'equal))
       (pyim-dhashcache-iword2count
        (make-hash-table :test #'equal))
       (pyim-dhashcache-iword2count-recent-10-words
        (make-hash-table :test #'equal))
       (pyim-dhashcache-iword2count-recent-50-words
        (make-hash-table :test #'equal))
       (pyim-dhashcache-iword2priority
        (make-hash-table :test #'equal))
       (quanpin (pyim-scheme-get 'quanpin)))
  (puthash "ni-hao"
           '("你好" "尼耗")
           pyim-dhashcache-code2word)
  (puthash "ni-hao"
           '("您好" "你豪")
           pyim-dhashcache-icode2word)
  (puthash "ma"
           '("吗" "马")
           pyim-dhashcache-code2word)
  (puthash "ma"
           '("嘛")
           pyim-dhashcache-icode2word)
  (puthash "peng-you"
           '("朋友" "喷油")
           pyim-dhashcache-code2word)
  (puthash "ni-hao-ma"
           '("你好吗" "你好嘛")
           pyim-dhashcache-code2word)
  (puthash "ni-hao-ma-peng-you"
           '("你好吗朋友" "你好吗喷油")
           pyim-dhashcache-code2word)
  (puthash "n-h"
           '("你好" "你坏" "尼耗" "内核")
           pyim-dhashcache-ishortcode2word)
  (puthash "您好" 4
           pyim-dhashcache-iword2count-recent-10-words)
  (puthash "你豪" 9
           pyim-dhashcache-iword2count)
  (let* ((full-imobjs
          (pyim-imobjs-create
           "nihaomapengyou" quanpin))
         (short-imobjs
          (pyim-imobjs-create "nih" quanpin))
         (full
          (pyim-candidates-create
           full-imobjs quanpin))
         (short
          (pyim-candidates--jianpin-words
           short-imobjs quanpin)))
    (list
     :full-input
     (mapcar
      (lambda (imobj)
        (pyim-codes-create imobj quanpin))
      full-imobjs)
     :full-candidates
     (cl-subseq full 0 (min 14 (length full)))
     :short-input
     (mapcar
      (lambda (imobj)
        (pyim-codes-create imobj quanpin))
      short-imobjs)
     :short-candidates short
     :chief
     (pyim-candidates-get-chief
      quanpin '("你豪" "您好" "尼耗") nil))))
"##;
    let expect = expect![[
        r##"OK (:full-input (("ni" "hao" "ma" "peng" "you") ("ni" "hao" "ma" "pen" "you")) :full-candidates ("你好吗朋友" "你好吗喷油" "你好吗" "您好" "你好嘛" "你豪" "你好" "尼耗" "你" "尼" "呢" "泥" "拟" "逆") :short-input (("ni" "h")) :short-candidates ("你好" "你坏" "尼耗") :chief "您好")"##
    ]];
    ParityBatchCase::value(
        "personalized_and_common_dictionaries_produce_ranked_composition_candidates",
        elisp_form,
        expect,
    )
}

fn punctuation_round_trip_edits_a_mixed_release_note_and_preserves_point() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((pyim-punctuation--pair-status
         '(("\"" nil) ("'" nil))))
    (insert "发布[Neomacs],版本\"v2\";状态'ok'!")
    (goto-char (point-min))
    (while (re-search-forward "[][,\";'!]" nil t)
      (pyim-punctuation-translate 'full-width))
    (let ((full-width (buffer-string))
          (point-after-full-width (point)))
      (goto-char (point-min))
      (while (re-search-forward
              "[【】，；“”‘’！]" nil t)
        (pyim-punctuation-translate 'half-width))
      (list
       :full-width full-width
       :point-after-full-width point-after-full-width
       :round-trip (buffer-string)
       :point-after-round-trip (point)
       :pair-status
       (copy-tree pyim-punctuation--pair-status)
       :positions
       (mapcar
        #'pyim-punctuation-position
        '("[" "【" "'" "‘" "’"))))))
"##;
    let expect = expect![[
        r##"OK (:full-width "发布【Neomacs】，版本“v2”；状态‘ok’！" :point-after-full-width 26 :round-trip "发布[Neomacs],版本\"v2\";状态'ok'!" :point-after-round-trip 26 :pair-status (("\"" . t) ("'" . t)) :positions (0 1 0 1 2))"##
    ]];
    ParityBatchCase::value(
        "punctuation_round_trip_edits_a_mixed_release_note_and_preserves_point",
        elisp_form,
        expect,
    )
}

fn pinyin_regexp_searches_real_mixed_language_records_and_reports_exact_matches() -> ParityBatchCase
{
    let elisp_form = r##"
(cl-letf (((symbol-function 'pyim-dcache-init-variables)
           #'ignore)
          ((symbol-function 'pyim-dcache-get)
           (lambda (&rest _args) nil)))
  (let ((queries
         '(("nihao" 3 nil)
           ("nh" 2 t)
           ("chongqing.*yinhang" 2 t)))
        (records
         '("service=api owner=你好 status=ready"
           "service=web owner=尼耗 status=pending"
           "重庆银行 release branch"
           "chongqing-yinhang fallback"
           "牛蛤 telemetry"
           "unrelated record"))
        result)
    (dolist (query queries)
      (let ((regexp
             (pyim-cregexp-build
              (nth 0 query)
              (nth 1 query)
              (nth 2 query)))
            matches)
        (dolist (record records)
          (when (string-match regexp record)
            (push
             (list
              record
              (match-string 0 record)
              (match-beginning 0)
              (match-end 0))
             matches)))
        (push
         (list
          :query (nth 0 query)
          :chinese-only (nth 2 query)
          :valid (pyim-cregexp--valid-p regexp)
          :matches (nreverse matches))
         result)))
    (nreverse result)))
"##;
    let expect = expect![[
        r##"OK ((:query "nihao" :chinese-only nil :valid t :matches (("service=api owner=你好 status=ready" "你好" 18 20) ("service=web owner=尼耗 status=pending" "尼耗" 18 20))) (:query "nh" :chinese-only t :valid t :matches (("service=api owner=你好 status=ready" "你好" 18 20) ("service=web owner=尼耗 status=pending" "尼耗" 18 20) ("牛蛤 telemetry" "牛蛤" 0 2))) (:query "chongqing.*yinhang" :chinese-only t :valid t :matches (("重庆银行 release branch" "重庆银行" 0 4))))"##
    ]];
    ParityBatchCase::value(
        "pinyin_regexp_searches_real_mixed_language_records_and_reports_exact_matches",
        elisp_form,
        expect,
    )
}

fn candidate_pages_slice_navigate_and_render_the_selected_release_target() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((pyim-page-length 5)
       (pyim-process--candidates
        '("稳定版" "候选版" "每日版" "回滚版" "文档版"
          "Linux版" "macOS版" "Windows版" "源码版" "调试版"
          "最小版" "完整包"))
       snapshots)
  (dolist (position '(0 4 5 9 10 11))
    (let ((pyim-process--word-position position))
      (push
       (list
        :position position
        :page (pyim-page--current-page)
        :total (pyim-page--total-page)
        :bounds
        (list (pyim-page--start)
              (pyim-page--end))
        :visible
        (pyim-page--get-showed-candidates)
        :menu
        (substring-no-properties
         (pyim-page-menu-create
          (pyim-page--get-showed-candidates)
          (pyim-page--word-position-in-current-page)
          " | " t)))
       snapshots)))
  (nreverse snapshots))
"##;
    let expect = expect![[
        r##"OK ((:position 0 :page 1 :total 3 :bounds (0 5) :visible ("稳定版" "候选版" "每日版" "回滚版" "文档版") :menu "1[稳定版] | 2.候选版  | 3.每日版  | 4.回滚版  | 5.文档版 ") (:position 4 :page 1 :total 3 :bounds (0 5) :visible ("稳定版" "候选版" "每日版" "回滚版" "文档版") :menu "1.稳定版  | 2.候选版  | 3.每日版  | 4.回滚版  | 5[文档版]") (:position 5 :page 2 :total 3 :bounds (5 10) :visible ("Linux版" "macOS版" "Windows版" "源码版" "调试版") :menu "1[Linux版] | 2.macOS版  | 3.Windows版  | 4.源码版  | 5.调试版 ") (:position 9 :page 2 :total 3 :bounds (5 10) :visible ("Linux版" "macOS版" "Windows版" "源码版" "调试版") :menu "1.Linux版  | 2.macOS版  | 3.Windows版  | 4.源码版  | 5[调试版]") (:position 10 :page 3 :total 3 :bounds (10 12) :visible ("最小版" "完整包") :menu "1[最小版] | 2.完整包 ") (:position 11 :page 3 :total 3 :bounds (10 12) :visible ("最小版" "完整包") :menu "1.最小版  | 2[完整包]"))"##
    ]];
    ParityBatchCase::value(
        "candidate_pages_slice_navigate_and_render_the_selected_release_target",
        elisp_form,
        expect,
    )
}

fn personal_dictionary_persists_reloadable_values_and_exports_stable_records() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (make-temp-file "pyim-parity-dictionary-" t))
       (pyim-dcache-directory
        (file-name-as-directory root))
       (value-file
        (expand-file-name "word-counts.el" root))
       (export-file
        (expand-file-name "personal.pyim" root))
       (pyim-dcache-backend 'pyim-dhashcache)
       (pyim-dhashcache-iword2count
        (make-hash-table :test #'equal))
       (pyim-dhashcache-icode2word
        (make-hash-table :test #'equal)))
  (unwind-protect
      (progn
        (puthash "你好" 12
                 pyim-dhashcache-iword2count)
        (puthash "尼耗" 2
                 pyim-dhashcache-iword2count)
        (puthash "ni-hao"
                 '("你好" "尼耗")
                 pyim-dhashcache-icode2word)
        (puthash "fa-bu"
                 '("发布")
                 pyim-dhashcache-icode2word)
        (pyim-dcache-save-value-to-file
         pyim-dhashcache-iword2count
         value-file)
        (pyim-dcache-export-personal-words
         export-file)
        (let ((loaded
               (pyim-dcache-get-value-from-file
                value-file))
              export)
          (with-temp-buffer
            (insert-file-contents export-file)
            (setq export (buffer-string)))
          (list
           :loaded
           (mapcar
            (lambda (word)
              (cons word (gethash word loaded)))
            '("你好" "尼耗" "缺失"))
           :export export
           :files
           (sort
            (directory-files root nil
                             directory-files-no-dot-files-regexp)
            #'string<))))
    (delete-directory root t)))
"##;
    let expect = expect![[
        r##"OK (:loaded (("你好" . 12) ("尼耗" . 2) ("缺失")) :export ";;; -*- coding: utf-8-unix -*-\nfa-bu 发布\nni-hao 你好 尼耗\n" :files ("personal.pyim" "word-counts.el"))"##
    ]];
    ParityBatchCase::value(
        "personal_dictionary_persists_reloadable_values_and_exports_stable_records",
        elisp_form,
        expect,
    )
}

fn registered_input_method_activates_and_deactivates_with_the_expected_buffer_lifecycle()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((current-input-method nil)
        (current-input-method-title nil)
        (input-method-function nil)
        (deactivate-current-input-method-function nil)
        (kill-emacs-hook nil)
        events)
    (cl-letf
        (((symbol-function 'pyim-process-start-daemon)
          (lambda () (push 'daemon-start events)))
         ((symbol-function 'pyim-process-init-dcaches)
          (lambda (&rest args)
            (push (cons 'dcache-init args) events)))
         ((symbol-function 'pyim-process-stop-daemon)
          (lambda () (push 'daemon-stop events))))
      (let ((pyim-load-hook
             (list
              (lambda () (push 'load-hook events))))
            (pyim-activate-hook
             (list
              (lambda () (push 'activate-hook events))))
            (pyim-deactivate-hook
             (list
              (lambda () (push 'deactivate-hook events)))))
        (activate-input-method "pyim")
        (let ((active
               (list
                :method current-input-method
                :title current-input-method-title
                :function input-method-function
                :deactivator
                deactivate-current-input-method-function
                :kill-hook-installed
                (and
                 (memq
                  #'pyim--kill-emacs-hook-function
                  kill-emacs-hook)
                 t))))
          (deactivate-input-method)
          (list
           :active active
           :inactive
           (list
            :method current-input-method
            :title current-input-method-title
            :function input-method-function)
           :events (nreverse events)))))))
"##;
    let expect = expect![[
        r##"OK (:active (:method "pyim" :title "PYIM " :function pyim-input-method :deactivator pyim-deactivate :kill-hook-installed t) :inactive (:method nil :title nil :function nil) :events (daemon-start (dcache-init) load-hook activate-hook daemon-stop deactivate-hook))"##
    ]];
    ParityBatchCase::value(
        "registered_input_method_activates_and_deactivates_with_the_expected_buffer_lifecycle",
        elisp_form,
        expect,
    )
}

#[test]
fn pyim_package_batch() {
    let cases = vec![
        phonetic_composition_builds_quanpin_shuangpin_and_shape_codes(),
        chinese_text_conversion_handles_phrases_polyphony_ascii_and_initials(),
        personalized_and_common_dictionaries_produce_ranked_composition_candidates(),
        punctuation_round_trip_edits_a_mixed_release_note_and_preserves_point(),
        pinyin_regexp_searches_real_mixed_language_records_and_reports_exact_matches(),
        candidate_pages_slice_navigate_and_render_the_selected_release_target(),
        personal_dictionary_persists_reloadable_values_and_exports_stable_records(),
        registered_input_method_activates_and_deactivates_with_the_expected_buffer_lifecycle(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed pyim parity test");
    assert_oracle_batch_cases(pyim_oracle(), test_name, "pyim_parity", &cases);
}
