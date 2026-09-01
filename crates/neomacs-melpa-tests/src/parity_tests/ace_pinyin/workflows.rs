use expect_test::expect;

use super::ParityBatchCase;

/// The package's headline story: a page of mixed notes, `ace-pinyin-mode' on,
/// the user's own avy binding, one Latin letter, one avy key.  `j' has to offer
/// 京, 交 and 界 — the characters whose pinyin starts with `j' — alongside the
/// literal `J' of "Jiao", in buffer order, and each avy key has to land point
/// exactly on its character.  Jumping pushes a mark at the departure point and
/// never edits the text.
fn ace_pinyin_jumps_to_every_chinese_character_whose_pinyin_starts_with_the_typed_letter()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ace_pinyin_jumps_to_every_chinese_character_whose_pinyin_starts_with_the_typed_letter",
        r##"(progn
  (apy-test-setup)
  (apy-test-buffer)
  (ace-pinyin-mode 1)
  (global-set-key (kbd "C-c z") 'avy-goto-char)
  (let ((offered (apy-test-offer "C-c z" "j a")))
    (list :binding (key-binding (kbd "C-c z"))
          :remapped-to (apy-test-owner 'avy-goto-char 'ace-pinyin--original-avy)
          :candidates (plist-get offered :candidates)
          :key-a (plist-get offered :landing)
          :mark-after-a (mark t)
          :key-s (apy-test-press "C-c z" "j s")
          :key-d (apy-test-press "C-c z" "j d")
          :key-f (apy-test-press "C-c z" "j f")
          :text (buffer-substring-no-properties (point-min) (point-max))
          :modified (buffer-modified-p))))"##,
        expect![[
            r#"OK (:binding avy-goto-char :remapped-to ace-pinyin-jump-char :candidates ((2 "京") (26 "交") (40 "J") (81 "界")) :key-a (2 "京" 1 1) :mark-after-a 1 :key-s (26 "交" 2 2) :key-d (40 "J" 2 16) :key-f (81 "界" 4 4) :text "北京大学 Peking University\n上海交通大学 Shanghai Jiao Tong\n中文输入法 Chinese input method\n你好，世界！Hello, world.\n《汉语大词典》 traditional 學習漢語。\n" :modified nil)"#
        ]],
    )
}

fn ace_pinyin_mode_decides_per_buffer_whether_chinese_characters_are_jumpable() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_pinyin_mode_decides_per_buffer_whether_chinese_characters_are_jumpable",
        r##"(progn
  (apy-test-setup)
  (global-set-key (kbd "C-c z") 'avy-goto-char)
  (let ((notes (apy-test-buffer)))
    (list
     :before (list :cell (apy-test-owner 'avy-goto-char 'ace-pinyin--original-avy)
                   :mode ace-pinyin-mode
                   :offer (apy-test-offer "C-c z" "j a"))
     :after-turn-on (progn
                      (turn-on-ace-pinyin-mode)
                      (list :cell (apy-test-owner 'avy-goto-char 'ace-pinyin--original-avy)
                            :mode ace-pinyin-mode
                            :lighter (assq 'ace-pinyin-mode minor-mode-alist)
                            :offer (apy-test-offer "C-c z" "j a")))
     :other-buffer (let ((memo (apy-test-buffer "交给你 Jane\n" "*ace-pinyin-memo*")))
                     (prog1 (list :cell (apy-test-owner 'avy-goto-char 'ace-pinyin--original-avy)
                                  :mode ace-pinyin-mode
                                  :offer (apy-test-offer "C-c z" "j a"))
                       (set-window-buffer (selected-window) notes)
                       (set-buffer notes)
                       (kill-buffer memo)))
     :after-turn-off (progn
                       (turn-off-ace-pinyin-mode)
                       (list :cell (apy-test-owner 'avy-goto-char 'ace-pinyin--original-avy)
                             :mode ace-pinyin-mode
                             :offer (apy-test-offer "C-c z" "j a")))
     :modified (buffer-modified-p))))"##,
        expect![[
            r#"OK (:before (:cell avy-original :mode nil :offer (:landing (40 "J" 2 16) :candidates ((40 "J")))) :after-turn-on (:cell ace-pinyin-jump-char :mode t :lighter (ace-pinyin-mode " AcePY") :offer (:landing (2 "京" 1 1) :candidates ((2 "京") (26 "交") (40 "J") (81 "界")))) :other-buffer (:cell ace-pinyin-jump-char :mode nil :offer (:landing (5 "J" 1 4) :candidates ((5 "J")))) :after-turn-off (:cell avy-original :mode nil :offer (:landing (40 "J" 2 16) :candidates ((40 "J")))) :modified nil)"#
        ]],
    )
    .fresh_process()
}

fn ace_pinyin_finds_traditional_characters_once_simplified_only_is_turned_off() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_pinyin_finds_traditional_characters_once_simplified_only_is_turned_off",
        r##"(progn
  (apy-test-setup)
  (apy-test-buffer)
  (ace-pinyin-mode 1)
  (global-set-key (kbd "C-c z") 'avy-goto-char)
  (list :simplified-x (apy-test-offer "C-c z" "x a")
        :simplified-h (apy-test-offer "C-c z" "h a")
        :traditional (progn
                       (setq ace-pinyin-simplified-chinese-only-p nil)
                       (list :x (apy-test-offer "C-c z" "x a")
                             :h (apy-test-offer "C-c z" "h a")
                             :y (apy-test-offer "C-c z" "y a")))
        :restored (progn
                    (setq ace-pinyin-simplified-chinese-only-p t)
                    (apy-test-offer "C-c z" "x a"))
        :modified (buffer-modified-p)))"##,
        expect![[
            r#"OK (:simplified-x (:landing (4 "学" 1 3) :candidates ((4 "学") (29 "学"))) :simplified-h (:landing (25 "海" 2 1) :candidates ((25 "海") (32 "h") (36 "h") (57 "h") (73 "h") (78 "好") (83 "H") (98 "汉"))) :traditional (:x (:landing (117 "學" 5 20) :candidates ((117 "學") (118 "習"))) :h (:landing (25 "海" 2 1) :candidates ((25 "海") (32 "h") (36 "h") (57 "h") (73 "h") (78 "好") (83 "H") (119 "漢"))) :y (:landing (22 "y" 1 21) :candidates ((22 "y") (120 "語")))) :restored (:landing (4 "学" 1 3) :candidates ((4 "学") (29 "学"))) :modified nil)"#
        ]],
    )
}

fn ace_pinyin_translates_ascii_punctuation_to_its_chinese_counterpart() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_pinyin_translates_ascii_punctuation_to_its_chinese_counterpart",
        r##"(progn
  (apy-test-setup)
  (apy-test-buffer)
  (ace-pinyin-mode 1)
  (global-set-key (kbd "C-c z") 'avy-goto-char)
  (list :period (apy-test-offer "C-c z" ". a")
        :comma (apy-test-offer "C-c z" ", a")
        :angle (apy-test-offer "C-c z" "< a")
        :disabled (progn
                    (setq ace-pinyin-enable-punctuation-translation nil)
                    (list :period (apy-test-offer "C-c z" ". a")
                          :angle (apy-test-offer "C-c z" "< a")))
        :re-enabled (progn
                      (setq ace-pinyin-enable-punctuation-translation t)
                      (apy-test-offer "C-c z" "< a"))
        :modified (buffer-modified-p)))"##,
        expect![[
            r#"OK (:period (:landing (95 "." 4 18) :candidates ((95 ".") (121 "。"))) :comma (:landing (79 "，" 4 2) :candidates ((79 "，") (88 ","))) :angle (:landing (97 "《" 5 0) :candidates ((97 "《"))) :disabled (:period (:landing (95 "." 4 18) :candidates ((95 "."))) :angle (:landing (1 "北" 1 0) :candidates nil)) :re-enabled (:landing (97 "《" 5 0) :candidates ((97 "《"))) :modified nil)"#
        ]],
    )
}

fn ace_pinyin_reports_zero_candidates_and_keeps_point_when_no_character_matches() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ace_pinyin_reports_zero_candidates_and_keeps_point_when_no_character_matches",
        r##"(progn
  (apy-test-setup)
  (apy-test-buffer)
  (ace-pinyin-mode 1)
  (global-set-key (kbd "C-c z") 'avy-goto-char)
  (let ((mark (apy-test-message-mark)))
    (goto-char 50)
    (list :origin (apy-test-where)
          :offer (apy-test-offer "C-c z" "q a" 50)
          :mark (mark t)
          :text (buffer-substring-no-properties (point-min) (point-max))
          :modified (buffer-modified-p)
          :messages (apy-test-messages-since mark))))"##,
        expect![[
            r#"OK (:origin (50 "中" 3 0) :offer (:landing (50 "中" 3 0) :candidates nil) :mark nil :text "北京大学 Peking University\n上海交通大学 Shanghai Jiao Tong\n中文输入法 Chinese input method\n你好，世界！Hello, world.\n《汉语大词典》 traditional 學習漢語。\n" :modified nil :messages ("zero candidates"))"#
        ]],
    )
}

fn ace_pinyin_word_jumping_follows_treat_word_as_char() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_pinyin_word_jumping_follows_treat_word_as_char",
        r##"(progn
  (apy-test-setup)
  (apy-test-buffer)
  (ace-pinyin-mode 1)
  (global-set-key (kbd "C-c z") 'avy-goto-char)
  (global-set-key (kbd "C-c w") 'avy-goto-word-1)
  (list :enabled (list :variable ace-pinyin-treat-word-as-char
                       :cell (apy-test-owner 'avy-goto-word-1 'ace-pinyin--original-avy-word-1)
                       :word-j (apy-test-offer "C-c w" "j a")
                       :word-h (apy-test-offer "C-c w" "h a")
                       :char-h (apy-test-offer "C-c z" "h a"))
        :disabled (progn
                    (ace-pinyin-mode -1)
                    (setq ace-pinyin-treat-word-as-char nil)
                    (ace-pinyin-mode 1)
                    (list :cell (apy-test-owner 'avy-goto-word-1 'ace-pinyin--original-avy-word-1)
                          :char-cell (apy-test-owner 'avy-goto-char 'ace-pinyin--original-avy)
                          :word-j (apy-test-offer "C-c w" "j a")
                          :word-h (apy-test-offer "C-c w" "h a")
                          :char-j (apy-test-offer "C-c z" "j a")))
        :modified (buffer-modified-p)))"##,
        expect![[
            r#"OK (:enabled (:variable t :cell ace-pinyin-goto-word-1 :word-j (:landing (2 "京" 1 1) :candidates ((2 "京") (26 "交") (40 "J") (81 "界"))) :word-h (:landing (25 "海" 2 1) :candidates ((25 "海") (78 "好") (83 "H") (98 "汉"))) :char-h (:landing (25 "海" 2 1) :candidates ((25 "海") (32 "h") (36 "h") (57 "h") (73 "h") (78 "好") (83 "H") (98 "汉")))) :disabled (:cell avy-original :char-cell ace-pinyin-jump-char :word-j (:landing (40 "J" 2 16) :candidates ((40 "J"))) :word-h (:landing (83 "H" 4 6) :candidates ((83 "H"))) :char-j (:landing (2 "京" 1 1) :candidates ((2 "京") (26 "交") (40 "J") (81 "界")))) :modified nil)"#
        ]],
    )
}

fn ace_pinyin_jumps_to_a_two_character_chinese_word_by_its_two_pinyin_initials() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ace_pinyin_jumps_to_a_two_character_chinese_word_by_its_two_pinyin_initials",
        r##"(progn
  (apy-test-setup)
  (apy-test-buffer)
  (ace-pinyin-mode 1)
  (global-set-key (kbd "C-c 2") 'avy-goto-char-2)
  (list :cell (apy-test-owner 'avy-goto-char-2 'ace-pinyin--original-avy-2)
        :bei-jing (apy-test-offer "C-c 2" "b j a")
        :shang-hai (apy-test-offer "C-c 2" "s h a")
        :shang-hai-second (apy-test-press "C-c 2" "s h s")
        :hello (apy-test-offer "C-c 2" "h e a")
        :no-match (apy-test-offer "C-c 2" "d a a")
        :text (buffer-substring-no-properties (point-min) (point-max))
        :modified (buffer-modified-p)))"##,
        expect![[
            r#"OK (:cell ace-pinyin-jump-char-2 :bei-jing (:landing (1 "北" 1 0) :candidates ((1 "北"))) :shang-hai (:landing (24 "上" 2 0) :candidates ((24 "上") (31 "S"))) :shang-hai-second (31 "S" 2 7) :hello (:landing (83 "H" 4 6) :candidates ((83 "H"))) :no-match (:landing (1 "北" 1 0) :candidates nil) :text "北京大学 Peking University\n上海交通大学 Shanghai Jiao Tong\n中文输入法 Chinese input method\n你好，世界！Hello, world.\n《汉语大词典》 traditional 學習漢語。\n" :modified nil)"#
        ]],
    )
}

fn ace_pinyin_dwim_restricts_itself_to_chinese_when_given_a_prefix_argument() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_pinyin_dwim_restricts_itself_to_chinese_when_given_a_prefix_argument",
        r##"(progn
  (apy-test-setup)
  (apy-test-buffer)
  (ace-pinyin-mode 1)
  (global-set-key (kbd "C-c d") 'ace-pinyin-dwim)
  (list :without-prefix (apy-test-offer "C-c d" "j a")
        :with-prefix (let ((current-prefix-arg '(4)))
                       (apy-test-offer "C-c d" "j a"))
        :with-prefix-third (let ((current-prefix-arg '(4)))
                             (apy-test-press "C-c d" "j d"))
        :chinese-only-h (let ((current-prefix-arg '(4)))
                          (apy-test-offer "C-c d" "h a"))
        :text (buffer-substring-no-properties (point-min) (point-max))
        :modified (buffer-modified-p)))"##,
        expect![[
            r#"OK (:without-prefix (:landing (2 "京" 1 1) :candidates ((2 "京") (26 "交") (40 "J") (81 "界"))) :with-prefix (:landing (2 "京" 1 1) :candidates ((2 "京") (26 "交") (81 "界"))) :with-prefix-third (81 "界" 4 4) :chinese-only-h (:landing (25 "海" 2 1) :candidates ((25 "海") (78 "好") (98 "汉"))) :text "北京大学 Peking University\n上海交通大学 Shanghai Jiao Tong\n中文输入法 Chinese input method\n你好，世界！Hello, world.\n《汉语大词典》 traditional 學習漢語。\n" :modified nil)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ace_pinyin_jumps_to_every_chinese_character_whose_pinyin_starts_with_the_typed_letter(),
        ace_pinyin_mode_decides_per_buffer_whether_chinese_characters_are_jumpable(),
        ace_pinyin_finds_traditional_characters_once_simplified_only_is_turned_off(),
        ace_pinyin_translates_ascii_punctuation_to_its_chinese_counterpart(),
        ace_pinyin_reports_zero_candidates_and_keeps_point_when_no_character_matches(),
        ace_pinyin_word_jumping_follows_treat_word_as_char(),
        ace_pinyin_jumps_to_a_two_character_chinese_word_by_its_two_pinyin_initials(),
        ace_pinyin_dwim_restricts_itself_to_chinese_when_given_a_prefix_argument(),
    ]
}
