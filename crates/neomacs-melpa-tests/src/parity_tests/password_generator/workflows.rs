use expect_test::expect;

use super::ParityBatchCase;

fn interactive_commands_insert_site_pin_and_high_entropy_passwords_at_point() -> ParityBatchCase {
    let elisp_form = r##"
(let ((neomacs-password-generator-test-draw 0))
  (cl-letf (((symbol-function 'random)
             #'neomacs-password-generator-test-random))
    (with-temp-buffer
      (insert "site=\npin=\nrecovery=")
      (goto-char (point-min))
      (search-forward "site=")
      (let ((current-prefix-arg 10))
        (call-interactively #'password-generator-simple))
      (search-forward "pin=")
      (let ((current-prefix-arg 6))
        (call-interactively #'password-generator-numeric))
      (goto-char (point-max))
      (let ((current-prefix-arg 18))
        (call-interactively #'password-generator-paranoid))
      (list :text (buffer-string)
            :point (point)
            :modified (buffer-modified-p)))))
"##;
    let expected = expect![[
        r#"OK (:text "site=fwDUl3Jar9\npin=529630\nrecovery=q8O^{o6M$[m4K@>k2I" :point 55 :modified t)"#
    ]];
    ParityBatchCase::value(
        "interactive_commands_insert_site_pin_and_high_entropy_passwords_at_point",
        elisp_form,
        expected,
    )
}

fn return_mode_honors_default_and_explicit_lengths_without_editing_the_buffer() -> ParityBatchCase {
    let elisp_form = r##"
(let ((neomacs-password-generator-test-draw 0))
  (cl-letf (((symbol-function 'random)
             #'neomacs-password-generator-test-random))
    (with-temp-buffer
      (insert "credentials stay unchanged")
      (let ((before (buffer-string))
            simple strong numeric paranoid)
        (setq simple (password-generator-simple nil t))
        (setq strong (password-generator-strong 16 t))
        (setq numeric (password-generator-numeric 8 t))
        (setq paranoid (password-generator-paranoid nil t))
        (list :simple simple
          :simple-length (length simple)
          :strong strong
          :strong-length (length strong)
          :strong-alphabet-only
          (cl-every
           (lambda (character)
             (memq character
                   (string-to-list
                    "abcdefghijklmnopqrstuvwxyz1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ_@!.^%,&-")))
           strong)
          :numeric numeric
          :numeric-length (length numeric)
          :numeric-only (not (string-match-p "[^0-9]" numeric))
          :paranoid paranoid
          :paranoid-length (length paranoid)
          :paranoid-alphabet-only
          (cl-every
           (lambda (character)
             (memq character
                   (string-to-list
                    "abcdefghijklmnopqrstuvwxyz1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ!@#$%^&*()_-+=/?,.><[]{}~")))
           paranoid)
              :buffer-unchanged (equal before (buffer-string))
              :point (point))))))
"##;
    let expected = expect![[
        r#"OK (:simple "fwDUl3Ja" :simple-length 8 :strong "-q8O%n5L!k2IZhyF" :strong-length 16 :strong-alphabet-only t :numeric "30741852" :numeric-length 8 :numeric-only t :paranoid "2IZ,izGX/gxEV+evCT_c" :paranoid-length 20 :paranoid-alphabet-only t :buffer-unchanged t :point 27)"#
    ]];
    ParityBatchCase::value(
        "return_mode_honors_default_and_explicit_lengths_without_editing_the_buffer",
        elisp_form,
        expected,
    )
}

fn phonetic_passwords_follow_consonant_vowel_digit_triplets_and_exact_lengths() -> ParityBatchCase {
    let elisp_form = r##"
(let ((neomacs-password-generator-test-draw 0))
  (cl-letf (((symbol-function 'random)
             #'neomacs-password-generator-test-random))
    (let (default short long)
      (setq default (password-generator-phonetic nil t))
      (setq short (password-generator-phonetic 2 t))
      (setq long (password-generator-phonetic 11 t))
      (list :default default
        :short short
        :long long
        :lengths (mapcar #'length (list default short long))
        :default-shape
        (and (string-match-p
              "\\`[wrtpsdfghjkzxcvbnm][eyuioa][123456789]"
              default)
             (string-match-p
              "[wrtpsdfghjkzxcvbnm][eyuioa]\\'"
              default)
             t)
        :long-shape
            (and (string-match-p
                  "\\`\\(?:[wrtpsdfghjkzxcvbnm][eyuioa][123456789]\\)\\{3\\}"
                  long)
                 t)))))
"##;
    let expected = expect![[
        r#"OK (:default "do4ty1mo" :short "vy" :long "zo1hy7do4ty" :lengths (8 2 11) :default-shape t :long-shape t)"#
    ]];
    ParityBatchCase::value(
        "phonetic_passwords_follow_consonant_vowel_digit_triplets_and_exact_lengths",
        elisp_form,
        expected,
    )
}

fn exact_triplet_phonetic_length_discards_one_triplet_before_the_next_password() -> ParityBatchCase
{
    let elisp_form = r##"
(let ((neomacs-password-generator-test-draw 0))
  (cl-letf (((symbol-function 'random)
             #'neomacs-password-generator-test-random))
    (let ((phonetic (password-generator-phonetic 6 t))
          (pin (password-generator-numeric 4 t)))
      (list :phonetic phonetic
            :phonetic-length (length phonetic)
            :pin-after-discarded-triplet pin
            :draw-count neomacs-password-generator-test-draw))))
"##;
    let expected = expect![[
        r#"OK (:phonetic "do4ty1" :phonetic-length 6 :pin-after-discarded-triplet "8529" :draw-count 13)"#
    ]];
    ParityBatchCase::value(
        "exact_triplet_phonetic_length_discards_one_triplet_before_the_next_password",
        elisp_form,
        expected,
    )
}

fn custom_unicode_alphabet_generates_and_inserts_a_phone_friendly_passphrase() -> ParityBatchCase {
    let elisp_form = r##"
(let ((neomacs-password-generator-test-draw 0))
  (cl-letf (((symbol-function 'random)
             #'neomacs-password-generator-test-random))
    (with-temp-buffer
      (insert "Код: ")
      (let ((password-generator-custom-alphabet "ёжλ界")
            (password-generator-custom-length 12))
        (password-generator-custom)
        (let ((inserted (buffer-substring-no-properties 6 (point-max))))
          (list :buffer (buffer-string)
            :inserted inserted
            :inserted-length (length inserted)
                :alphabet-only
                (not (string-match-p "[^ёжλ界]" inserted))
                :returned (password-generator-custom 7 t)))))))
"##;
    let expected = expect![[
        r#"OK (:buffer "Код: жλ界ёжλ界ёжλ界ё" :inserted "жλ界ёжλ界ёжλ界ё" :inserted-length 12 :alphabet-only t :returned "жλ界ёжλ界")"#
    ]];
    ParityBatchCase::value(
        "custom_unicode_alphabet_generates_and_inserts_a_phone_friendly_passphrase",
        elisp_form,
        expected,
    )
}

fn word_passwords_use_custom_vocabulary_separator_count_and_return_contract() -> ParityBatchCase {
    let elisp_form = r##"
(let ((neomacs-password-generator-test-draw 0))
  (cl-letf (((symbol-function 'random)
             #'neomacs-password-generator-test-random))
    (with-temp-buffer
      (insert "deploy-token: ")
      (let ((password-generator-vocabulary-nouns
         '("harbor" "signal" "bridge" "release"))
        (password-generator-vocabulary-adjectives
         '("calm" "green" "stable" "swift"))
        (password-generator-vocabulary-verbs
         '("build" "verify" "ship" "recover"))
        (password-generator-words-gap "-")
            inserted returned)
        (password-generator-words 5)
        (setq inserted (buffer-substring-no-properties 15 (point-max)))
        (setq returned (password-generator-words 4 t))
        (list :buffer (buffer-string)
          :inserted inserted
          :inserted-parts (split-string inserted "-")
              :returned returned
              :returned-parts (split-string returned "-")
              :point (point))))))
"##;
    let expected = expect![[
        r#"OK (:buffer "deploy-token: green-harbor-signal-calm-signal" :inserted "green-harbor-signal-calm-signal" :inserted-parts ("green" "harbor" "signal" "calm" "signal") :returned "harbor-green-harbor-signal" :returned-parts ("harbor" "green" "harbor" "signal") :point 46)"#
    ]];
    ParityBatchCase::value(
        "word_passwords_use_custom_vocabulary_separator_count_and_return_contract",
        elisp_form,
        expected,
    )
}

fn word_defaults_ignore_the_length_setting_and_trim_only_one_separator_character() -> ParityBatchCase
{
    let elisp_form = r##"
(let ((neomacs-password-generator-test-draw 0))
  (cl-letf (((symbol-function 'random)
             #'neomacs-password-generator-test-random))
    (let ((password-generator-vocabulary-nouns '("harbor"))
          (password-generator-vocabulary-adjectives '("stable"))
          (password-generator-vocabulary-verbs '("ship"))
          (password-generator-words-length 6)
          (password-generator-words-gap "::"))
      (let ((password (password-generator-words nil t)))
        (list :configured-count password-generator-words-length
              :password password
              :parts (split-string password "::")
              :separator-suffix (substring password -1))))))
"##;
    let expected = expect![[
        r#"OK (:configured-count 6 :password "stable::harbor::harbor:" :parts ("stable" "harbor" "harbor:") :separator-suffix ":")"#
    ]];
    ParityBatchCase::value(
        "word_defaults_ignore_the_length_setting_and_trim_only_one_separator_character",
        elisp_form,
        expected,
    )
}

fn random_list_selection_exposes_the_historical_negative_index_bias() -> ParityBatchCase {
    let elisp_form = r##"
(let ((choices '(alpha beta gamma delta))
      (draws '(0 1 2 3))
      calls
      samples)
  ;; Substitute only the unpredictable GNU random-number boundary.  The
  ;; package's public random wrapper and list-selection arithmetic stay real.
  (cl-letf (((symbol-function 'random)
             (lambda (max)
               (let ((draw (pop draws)))
                 (push (list :max max :draw draw) calls)
                 draw))))
    (dotimes (_ 4)
      (push (password-generator-random-list-element choices) samples)))
  (setq samples (nreverse samples))
  (list :calls (nreverse calls)
        :samples samples
        :first-count (length (delq nil (mapcar (lambda (item)
                                                 (eq item 'alpha))
                                               samples)))
        :last-reachable (and (memq 'delta samples) t)))
"##;
    let expected = expect![[
        r#"OK (:calls ((:max 4 :draw 0) (:max 4 :draw 1) (:max 4 :draw 2) (:max 4 :draw 3)) :samples (alpha alpha beta gamma) :first-count 2 :last-reachable nil)"#
    ]];
    ParityBatchCase::value(
        "random_list_selection_exposes_the_historical_negative_index_bias",
        elisp_form,
        expected,
    )
}

fn invalid_lengths_and_empty_sources_signal_without_partial_buffer_edits() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert "secret=")
  (goto-char (point-max))
  (let (results)
    (dolist (probe
             (list
              (lambda () (password-generator-simple -2))
              (lambda ()
                (let ((password-generator-custom-alphabet ""))
                  (password-generator-custom 3)))
              (lambda () (password-generator-words 0))
              (lambda ()
                (let ((current-prefix-arg '(4)))
                  (call-interactively #'password-generator-numeric)))))
      (push
       (condition-case err
           (list :value (funcall probe))
         (error (list :signal (car err) :data (cdr err))))
       results))
    (list :results (nreverse results)
          :buffer (buffer-string)
          :point (point))))
"##;
    let expected = expect![[
        r#"OK (:results ((:value nil) (:signal args-out-of-range :data (0)) (:signal args-out-of-range :data ("" 0 -1)) (:signal wrong-type-argument :data (number-or-marker-p (4)))) :buffer "secret=" :point 8)"#
    ]];
    ParityBatchCase::value(
        "invalid_lengths_and_empty_sources_signal_without_partial_buffer_edits",
        elisp_form,
        expected,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        interactive_commands_insert_site_pin_and_high_entropy_passwords_at_point(),
        return_mode_honors_default_and_explicit_lengths_without_editing_the_buffer(),
        phonetic_passwords_follow_consonant_vowel_digit_triplets_and_exact_lengths(),
        exact_triplet_phonetic_length_discards_one_triplet_before_the_next_password(),
        custom_unicode_alphabet_generates_and_inserts_a_phone_friendly_passphrase(),
        word_passwords_use_custom_vocabulary_separator_count_and_return_contract(),
        word_defaults_ignore_the_length_setting_and_trim_only_one_separator_character(),
        random_list_selection_exposes_the_historical_negative_index_bias(),
        invalid_lengths_and_empty_sources_signal_without_partial_buffer_edits(),
    ]
}
