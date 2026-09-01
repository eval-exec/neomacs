use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, LOREM_IPSUM_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'lorem-ipsum)

(defun neomacs-lorem-ipsum-test-script-random (choices body)
  "Run BODY while returning successive CHOICES from `random'."
  (let ((remaining (copy-sequence choices))
        limits)
    (cl-letf (((symbol-function 'random)
               (lambda (limit)
                 (let ((choice (pop remaining)))
                   (push limit limits)
                   (unless (and (integerp choice)
                                (<= 0 choice)
                                (< choice limit))
                     (error "scripted random choice %S outside [0,%S)"
                            choice limit))
                   choice))))
      (let ((value (funcall body)))
        (list :limits (nreverse limits)
              :unused remaining
              :value value)))))

(defun neomacs-lorem-ipsum-test-buffer-state ()
  "Return exact current-buffer text and point."
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)))
"####;

fn deterministic_paragraphs_build_a_real_two_section_draft() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lorem-ipsum-test-script-random
 '(0 2)
 (lambda ()
   (with-temp-buffer
     (setq lorem-ipsum-sentence-separator " ")
     (setq lorem-ipsum-paragraph-separator "\n---\n")
     (insert "Release overview\n\n")
     (lorem-ipsum-insert-paragraphs 2)
     (insert "End of draft.")
     (list :state (neomacs-lorem-ipsum-test-buffer-state)
           :sentence-separator-local
           (local-variable-p 'lorem-ipsum-sentence-separator)
           :paragraph-separator-local
           (local-variable-p 'lorem-ipsum-paragraph-separator)))))
"####;
    let expected = expect![[
        r#"OK (:limits (4 4) :unused nil :value (:state (:text "Release overview\n\nLorem ipsum dolor sit amet, consectetuer adipiscing elit. Donec hendrerit tempor tellus. Donec pretium posuere tellus. Proin quam nisl, tincidunt et, mattis eget, convallis nec, purus. Cum sociis natoque penatibus et magnis dis parturient montes, nascetur ridiculus mus. Nulla posuere. Donec vitae dolor. Nullam tristique diam non turpis. Cras placerat accumsan nulla. Nullam rutrum. Nam vestibulum accumsan nisl.\n---\nAliquam erat volutpat. Nunc eleifend leo vitae magna. In id erat non orci commodo lobortis. Proin neque massa, cursus ut, gravida ut, lobortis eget, lacus. Sed diam. Praesent fermentum tempor tellus. Nullam tempus. Mauris ac felis vel velit tristique imperdiet. Donec at pede. Etiam vel neque nec dui dignissim bibendum. Vivamus id enim. Phasellus neque orci, porta a, aliquet quis, semper a, massa. Phasellus purus. Pellentesque tristique imperdiet tortor. Nam euismod tellus id erat.\n---\nEnd of draft." :point 940) :sentence-separator-local t :paragraph-separator-local t))"#
    ]];
    ParityBatchCase::value(
        "deterministic_paragraphs_build_a_real_two_section_draft",
        elisp_form,
        expected,
    )
}

fn selected_sentences_preserve_source_order_and_custom_spacing() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lorem-ipsum-test-script-random
 '(0 0 1 12 3 8)
 (lambda ()
   (with-temp-buffer
     (setq lorem-ipsum-sentence-separator " | ")
     (lorem-ipsum-insert-sentences 3)
     (neomacs-lorem-ipsum-test-buffer-state))))
"####;
    let expected = expect![[
        r#"OK (:limits (4 11 4 17 4 13) :unused nil :value (:text "Lorem ipsum dolor sit amet, consectetuer adipiscing elit. | Vestibulum convallis, lorem a tempus semper, dui dui euismod elit, vitae placerat urna tortor vitae lacus. | Curabitur vulputate vestibulum lorem. | " :point 210))"#
    ]];
    ParityBatchCase::value(
        "selected_sentences_preserve_source_order_and_custom_spacing",
        elisp_form,
        expected,
    )
}

fn customized_list_builds_a_deterministic_release_notes_section() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lorem-ipsum-test-script-random
 '(2 0 0 4 1 5)
 (lambda ()
   (with-temp-buffer
     (setq lorem-ipsum-list-beginning "## Release notes\n")
     (setq lorem-ipsum-list-bullet "- ")
     (setq lorem-ipsum-list-item-end "\n")
     (setq lorem-ipsum-list-end "-- end --\n")
     (lorem-ipsum-insert-list 3)
     (list :state (neomacs-lorem-ipsum-test-buffer-state)
           :locals
           (mapcar #'local-variable-p
                   '(lorem-ipsum-list-beginning
                     lorem-ipsum-list-bullet
                     lorem-ipsum-list-item-end
                     lorem-ipsum-list-end))))))
"####;
    let expected = expect![[
        r###"OK (:limits (4 15 4 11 4 17) :unused nil :value (:state (:text "## Release notes\n- Aliquam erat volutpat.\n- Cum sociis natoque penatibus et magnis dis parturient montes, nascetur ridiculus mus.\n- Donec neque quam, dignissim in, mollis nec, sagittis eu, wisi.\n-- end --\n" :point 206) :locals (t t t t)))"###
    ]];
    ParityBatchCase::value(
        "customized_list_builds_a_deterministic_release_notes_section",
        elisp_form,
        expected,
    )
}

fn sgml_mode_uses_html_separators_without_changing_plain_text_buffers() -> ParityBatchCase {
    let elisp_form = r####"
(let ((plain-defaults
       (with-temp-buffer
         (list lorem-ipsum-paragraph-separator
               lorem-ipsum-sentence-separator
               lorem-ipsum-list-beginning
               lorem-ipsum-list-bullet
               lorem-ipsum-list-item-end
               lorem-ipsum-list-end))))
  (list
   :plain-defaults plain-defaults
   :sgml
   (neomacs-lorem-ipsum-test-script-random
    '(0 0 1 1 2)
    (lambda ()
      (with-temp-buffer
        (sgml-mode)
        (insert "<section>\n")
        (lorem-ipsum-insert-list 2)
        (lorem-ipsum-insert-paragraphs 1)
        (insert "</section>\n")
        (list :format
              (list lorem-ipsum-paragraph-separator
                    lorem-ipsum-sentence-separator
                    lorem-ipsum-list-beginning
                    lorem-ipsum-list-bullet
                    lorem-ipsum-list-item-end
                    lorem-ipsum-list-end)
              :state (neomacs-lorem-ipsum-test-buffer-state)))))
   :plain-after
   (with-temp-buffer
     (list lorem-ipsum-paragraph-separator
           lorem-ipsum-sentence-separator
           lorem-ipsum-list-beginning
           lorem-ipsum-list-bullet
           lorem-ipsum-list-item-end
           lorem-ipsum-list-end))))
"####;
    let expected = expect![[
        r#"OK (:plain-defaults ("\n\n" "  " "" "* " "\n" "") :sgml (:limits (4 11 4 17 4) :unused nil :value (:format ("<br><br>\n" "&nbsp;&nbsp;" "<ul>\n" "<li>" "</li>\n" "</ul>\n") :state (:text "<section>\n<ul>\n<li>Lorem ipsum dolor sit amet, consectetuer adipiscing elit.</li>\n<li>Donec posuere augue in quam.</li>\n</ul>\nAliquam erat volutpat.&nbsp;&nbsp;Nunc eleifend leo vitae magna.&nbsp;&nbsp;In id erat non orci commodo lobortis.&nbsp;&nbsp;Proin neque massa, cursus ut, gravida ut, lobortis eget, lacus.&nbsp;&nbsp;Sed diam.&nbsp;&nbsp;Praesent fermentum tempor tellus.&nbsp;&nbsp;Nullam tempus.&nbsp;&nbsp;Mauris ac felis vel velit tristique imperdiet.&nbsp;&nbsp;Donec at pede.&nbsp;&nbsp;Etiam vel neque nec dui dignissim bibendum.&nbsp;&nbsp;Vivamus id enim.&nbsp;&nbsp;Phasellus neque orci, porta a, aliquet quis, semper a, massa.&nbsp;&nbsp;Phasellus purus.&nbsp;&nbsp;Pellentesque tristique imperdiet tortor.&nbsp;&nbsp;Nam euismod tellus id erat.<br><br>\n</section>\n" :point 786))) :plain-after ("\n\n" "  " "" "* " "\n" ""))"#
    ]];
    ParityBatchCase::value(
        "sgml_mode_uses_html_separators_without_changing_plain_text_buffers",
        elisp_form,
        expected,
    )
}

fn default_bindings_are_idempotent_and_nonpositive_counts_are_noops() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((prefix (kbd "C-c l"))
       (saved-prefix (lookup-key global-map prefix)))
  (unwind-protect
      (progn
        (lorem-ipsum-use-default-bindings)
        (let ((first
               (mapcar (lambda (key) (lookup-key global-map (kbd key)))
                       '("C-c l p" "C-c l s" "C-c l l"))))
          (lorem-ipsum-use-default-bindings)
          (list
           :first first
           :second
           (mapcar (lambda (key) (lookup-key global-map (kbd key)))
                   '("C-c l p" "C-c l s" "C-c l l"))
           :counts
           (let ((random-calls 0))
             (cl-letf (((symbol-function 'random)
                        (lambda (_limit)
                          (setq random-calls (1+ random-calls))
                          0)))
               (with-temp-buffer
                 (lorem-ipsum-insert-paragraphs 0)
                 (lorem-ipsum-insert-sentences -2)
                 (lorem-ipsum-insert-list 0)
                 (list :text (buffer-string)
                       :point (point)
                       :random-calls random-calls)))))))
    (define-key global-map prefix saved-prefix)))
"####;
    let expected = expect![[
        r#"OK (:first (lorem-ipsum-insert-paragraphs lorem-ipsum-insert-sentences lorem-ipsum-insert-list) :second (lorem-ipsum-insert-paragraphs lorem-ipsum-insert-sentences lorem-ipsum-insert-list) :counts (:text "" :point 1 :random-calls 0))"#
    ]];
    ParityBatchCase::value(
        "default_bindings_are_idempotent_and_nonpositive_counts_are_noops",
        elisp_form,
        expected,
    )
}

fn lorem_ipsum_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LOREM_IPSUM_MELPA_PIN, "lorem-ipsum.el")
        .expect("prepare pinned Lorem-Ipsum source below ./tmp")
        .with_timeout(Duration::from_secs(180))
        .with_prelude(PRELUDE)
}

#[test]
fn lorem_ipsum_practical_workflows_batch() {
    let cases = vec![
        deterministic_paragraphs_build_a_real_two_section_draft(),
        selected_sentences_preserve_source_order_and_custom_spacing(),
        customized_list_builds_a_deterministic_release_notes_section(),
        sgml_mode_uses_html_separators_without_changing_plain_text_buffers(),
        default_bindings_are_idempotent_and_nonpositive_counts_are_noops(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("lorem-ipsum parity batch");
    assert_oracle_batch_cases(
        lorem_ipsum_oracle(),
        test_name,
        "lorem-ipsum parity",
        &cases,
    );
}
