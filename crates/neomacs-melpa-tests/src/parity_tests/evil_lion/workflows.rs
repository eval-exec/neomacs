use expect_test::expect;

use super::ParityBatchCase;

fn left_alignment_formats_assignment_tables_and_squeezes_separator_spaces() -> ParityBatchCase {
    ParityBatchCase::value(
        "left_alignment_formats_assignment_tables_and_squeezes_separator_spaces",
        r##"
(neomacs-evil-lion-test-align
 "one  = 1
three   = 3
fifteen= 15
"
 'left nil ?= nil '(squeeze . t))
"##,
        expect![[
            r#"OK (:text "one    = 1\nthree  = 3\nfifteen= 15\n" :point 35 :mark nil :narrowed nil :mode text-mode)"#
        ]],
    )
}

fn right_alignment_handles_every_occurrence_or_only_the_first() -> ParityBatchCase {
    ParityBatchCase::value(
        "right_alignment_handles_every_occurrence_or_only_the_first",
        r##"
(let ((input "a, b, c
aa, bb, cc
aaa, bbb, ccc
"))
  (list :all (neomacs-evil-lion-test-align
              input 'right nil ?, nil '(squeeze . t))
        :first (neomacs-evil-lion-test-align
                input 'right 1 ?, nil '(squeeze . t))))
"##,
        expect![[
            r#"OK (:all (:text "a,   b,   c\naa,  bb,  cc\naaa, bbb, ccc\n" :point 40 :mark nil :narrowed nil :mode text-mode) :first (:text "a,   b, c\naa,  bb, cc\naaa, bbb, ccc\n" :point 37 :mark nil :narrowed nil :mode text-mode))"#
        ]],
    )
}

fn selected_middle_range_aligns_without_touching_neighboring_paragraphs() -> ParityBatchCase {
    ParityBatchCase::value(
        "selected_middle_range_aligns_without_touching_neighboring_paragraphs",
        r##"
(with-temp-buffer
  (insert "a, b, c
aa, bb, cc
aaa, bbb, ccc

x, y, z
xxxx, yy, zz
xx, yyyyy, zzz

a, b, c
aa, bb, cc
aaa, bbb, ccc
")
  (goto-char (point-min))
  (search-forward "x, y, z")
  (let ((beg (line-beginning-position)))
    (forward-line 3)
    (evil-lion-right nil beg (point) ?,))
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :narrowed (buffer-narrowed-p)))
"##,
        expect![[
            r#"OK (:text "a, b, c\naa, bb, cc\naaa, bbb, ccc\n\nx,    y,     z\nxxxx, yy,    zz\nxx,   yyyyy, zzz\n\na, b, c\naa, bb, cc\naaa, bbb, ccc\n" :point 83 :narrowed nil)"#
        ]],
    )
}

fn slash_prompt_uses_a_real_regex_and_updates_deduplicated_history() -> ParityBatchCase {
    ParityBatchCase::value(
        "slash_prompt_uses_a_real_regex_and_updates_deduplicated_history",
        r##"
(let ((evil-lion--user-regex-history '("OLD"))
      (prompts nil)
      (answers '("X+" "X+")))
  (cl-letf (((symbol-function 'read-string)
             (lambda (prompt &optional initial-input history default-value
                             &rest _)
               (push (list prompt initial-input history default-value) prompts)
               (pop answers))))
    (list
     :first (neomacs-evil-lion-test-align
             "aX bX c
aaXX bbX cc
aaaXXX bbbX ccc
"
             'right nil ?/ nil '(squeeze . t))
     :second (neomacs-evil-lion-test-align
              "pX q
ppXX qq
" 'left nil ?/ nil '(squeeze . t))
     :prompts (nreverse prompts)
     :history evil-lion--user-regex-history)))
"##,
        expect![[
            r#"OK (:first (:text "aX     bX   c\naaXX   bbX  cc\naaaXXX bbbX ccc\n" :point 46 :mark nil :narrowed nil :mode text-mode) :second (:text "p X q\nppXX qq\n" :point 15 :mark nil :narrowed nil :mode text-mode) :prompts (("Pattern [OLD]: " nil evil-lion--user-regex-history "OLD") ("Pattern [X+]: " nil evil-lion--user-regex-history "X+")) :history ("X+" "OLD"))"#
        ]],
    )
}

fn return_uses_real_perl_major_mode_alignment_rules() -> ParityBatchCase {
    ParityBatchCase::value(
        "return_uses_real_perl_major_mode_alignment_rules",
        r##"
(neomacs-evil-lion-test-align
 "my %hash = (
   a => 1,
   bbb => 2,
   cccc => 3,

   a => 1,
   bbb => 2,
   cccccc => 3
);
"
 'left nil ?\r #'perl-mode '(squeeze . t))
"##,
        expect![[
            r#"OK (:text "my %hash =  (\n   a     => 1,\n   bbb   => 2,\n   cccc  => 3,\n\n   a      => 1,\n   bbb    => 2,\n   cccccc => 3\n);\n" :point 111 :mark nil :narrowed nil :mode perl-mode)"#
        ]],
    )
}

fn squeeze_customization_preserves_or_collapses_existing_whitespace() -> ParityBatchCase {
    ParityBatchCase::value(
        "squeeze_customization_preserves_or_collapses_existing_whitespace",
        r##"
(let ((input "a    = 1,  one
b   = 2 , two
c  = 3 , three
"))
  (list :enabled (neomacs-evil-lion-test-align
                  input 'left nil ?= nil '(squeeze . t))
        :disabled (neomacs-evil-lion-test-align
                   input 'left nil ?= nil '(squeeze))))
"##,
        expect![[
            r#"OK (:enabled (:text "a = 1,  one\nb = 2 , two\nc = 3 , three\n" :point 39 :mark nil :narrowed nil :mode text-mode) :disabled (:text "a    = 1,  one\nb    = 2 , two\nc    = 3 , three\n" :point 48 :mark nil :narrowed nil :mode text-mode))"#
        ]],
    )
}

fn invalid_characters_are_noops_and_unsupported_counts_signal_atomically() -> ParityBatchCase {
    ParityBatchCase::value(
        "invalid_characters_are_noops_and_unsupported_counts_signal_atomically",
        r##"
(let ((input "a = 1
long = 2
"))
  (list
   :invalid
   (mapcar (lambda (char)
             (neomacs-evil-lion-test-align input 'left nil char))
           (list ?\e ?\d ?\b))
   :count
   (with-temp-buffer
     (insert input)
     (let ((before (buffer-string)))
       (condition-case error-data
           (list :value
                 (evil-lion-left 2 (point-min) (point-max) ?=)
                 :text (buffer-string))
         (error
          (list :signal (car error-data)
                :message (error-message-string error-data)
                :unchanged (equal before (buffer-string))
                :text (buffer-string))))))))
"##,
        expect![[
            r#"OK (:invalid ((:text "a = 1\nlong = 2\n" :point 16 :mark nil :narrowed nil :mode text-mode) (:text "a = 1\nlong = 2\n" :point 16 :mark nil :narrowed nil :mode text-mode) (:text "a = 1\nlong = 2\n" :point 16 :mark nil :narrowed nil :mode text-mode)) :count (:signal user-error :message "Only COUNT ‘1’ is supported at the moment" :unchanged t :text "a = 1\nlong = 2\n"))"#
        ]],
    )
}

fn global_mode_installs_default_and_custom_normal_visual_bindings() -> ParityBatchCase {
    ParityBatchCase::value(
        "global_mode_installs_default_and_custom_normal_visual_bindings",
        r##"
(let ((evil-lion-left-align-key (kbd "g l"))
      (evil-lion-right-align-key (kbd "g L")))
  (evil-lion-mode -1)
  (evil-lion-mode 1)
  (let ((default
         (list :mode evil-lion-mode
               :normal-left (neomacs-evil-lion-test-binding
                             'normal (kbd "g l"))
               :normal-right (neomacs-evil-lion-test-binding
                              'normal (kbd "g L"))
               :visual-left (neomacs-evil-lion-test-binding
                             'visual (kbd "g l"))
               :visual-right (neomacs-evil-lion-test-binding
                              'visual (kbd "g L")))))
    (evil-lion-mode -1)
    (let ((evil-lion-left-align-key (kbd "g a"))
          (evil-lion-right-align-key (kbd "g A")))
      (evil-lion-mode 1)
      (prog1
          (list :default default
                :custom
                (list :mode evil-lion-mode
                      :normal-left (neomacs-evil-lion-test-binding
                                    'normal (kbd "g a"))
                      :normal-right (neomacs-evil-lion-test-binding
                                     'normal (kbd "g A"))
                      :visual-left (neomacs-evil-lion-test-binding
                                    'visual (kbd "g a"))
                      :visual-right (neomacs-evil-lion-test-binding
                                     'visual (kbd "g A"))))
        (evil-lion-mode -1)))))
"##,
        expect![
            "OK (:default (:mode t :normal-left evil-lion-left :normal-right evil-lion-right :visual-left evil-lion-left :visual-right evil-lion-right) :custom (:mode t :normal-left evil-lion-left :normal-right evil-lion-right :visual-left evil-lion-left :visual-right evil-lion-right))"
        ],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        left_alignment_formats_assignment_tables_and_squeezes_separator_spaces(),
        right_alignment_handles_every_occurrence_or_only_the_first(),
        selected_middle_range_aligns_without_touching_neighboring_paragraphs(),
        slash_prompt_uses_a_real_regex_and_updates_deduplicated_history(),
        return_uses_real_perl_major_mode_alignment_rules(),
        squeeze_customization_preserves_or_collapses_existing_whitespace(),
        invalid_characters_are_noops_and_unsupported_counts_signal_atomically(),
        global_mode_installs_default_and_custom_normal_visual_bindings(),
    ]
}
