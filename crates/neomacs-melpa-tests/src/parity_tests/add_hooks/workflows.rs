use expect_test::expect;

use super::ParityBatchCase;

/// The example in the package's own docstrings, run as written: one function
/// onto two mode hooks, first through `add-hooks-pair' and then through the
/// alist form of `add-hooks'.  Both hook variables are read back and both hooks
/// are then run, so the report says the function was added *and* that it fires.
///
/// The `-hook' suffix is what makes the example read the way it does: the
/// caller writes `css-mode' and the function goes onto `css-mode-hook'.  A name
/// that already ends in `-hook' is left alone, so both spellings can be mixed in
/// one call, and a hook variable nobody has defined yet is simply created by
/// `add-hook' - which is how this works for modes that are not loaded.
fn the_documented_example_puts_one_function_on_several_mode_hooks() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_documented_example_puts_one_function_on_several_mode_hooks",
        r##"(progn
  (add-hooks-test-reset 'css-mode-hook 'sgml-mode-hook 'add-hooks-test-plain-hook)
  (add-hooks-pair '(css-mode sgml-mode) 'add-hooks-test-emmet-mode)
  (let ((by-pair (list :css css-mode-hook
                       :sgml sgml-mode-hook
                       :css-fires (add-hooks-test-fire 'css-mode-hook)
                       :sgml-fires (add-hooks-test-fire 'sgml-mode-hook))))
    (add-hooks-test-reset 'css-mode-hook 'sgml-mode-hook)
    (add-hooks '(((css-mode sgml-mode) . add-hooks-test-emmet-mode)))
    (let ((by-alist (list :css css-mode-hook :sgml sgml-mode-hook)))
      (add-hooks-test-reset 'css-mode-hook 'add-hooks-test-plain-hook)
      (add-hooks-pair '(css-mode add-hooks-test-plain-hook)
                      'add-hooks-test-emmet-mode)
      (list :through-a-pair by-pair
            :through-the-alist by-alist
            :both-forms-agree (equal (plist-get by-pair :css)
                                     (plist-get by-alist :css))
            :suffix-implied (add-hooks-normalize-hook 'css-mode)
            :suffix-kept (add-hooks-normalize-hook 'add-hooks-test-plain-hook)
            :mixed-spellings (list css-mode-hook add-hooks-test-plain-hook)
            :undefined-hook-before (boundp 'add-hooks-test-unheard-of-mode-hook)
            :undefined-hook-added (add-hooks-pair 'add-hooks-test-unheard-of-mode
                                                  'add-hooks-test-emmet-mode)
            :undefined-hook-after (and (boundp 'add-hooks-test-unheard-of-mode-hook)
                                       add-hooks-test-unheard-of-mode-hook)))))"##,
        expect![
            "OK (:through-a-pair (:css (add-hooks-test-emmet-mode) :sgml (add-hooks-test-emmet-mode) :css-fires (emmet-mode) :sgml-fires (emmet-mode)) :through-the-alist (:css (add-hooks-test-emmet-mode) :sgml (add-hooks-test-emmet-mode)) :both-forms-agree t :suffix-implied css-mode-hook :suffix-kept add-hooks-test-plain-hook :mixed-spellings ((add-hooks-test-emmet-mode) (add-hooks-test-emmet-mode)) :undefined-hook-before nil :undefined-hook-added nil :undefined-hook-after (add-hooks-test-emmet-mode))"
        ],
    )
}

fn one_pair_covers_every_hook_crossed_with_every_function() -> ParityBatchCase {
    ParityBatchCase::value(
        "one_pair_covers_every_hook_crossed_with_every_function",
        r##"(progn
  (add-hooks-test-reset 'css-mode-hook 'sgml-mode-hook 'text-mode-hook)
  (add-hooks-pair '(css-mode sgml-mode text-mode)
                  '(add-hooks-test-emmet-mode add-hooks-test-rainbow-mode))
  (let ((once (list :css css-mode-hook
                    :sgml sgml-mode-hook
                    :text text-mode-hook
                    :css-fires (add-hooks-test-fire 'css-mode-hook))))
    (add-hooks-pair '(css-mode sgml-mode text-mode)
                    '(add-hooks-test-emmet-mode add-hooks-test-rainbow-mode))
    (let ((twice (list :css css-mode-hook
                       :sgml sgml-mode-hook
                       :text text-mode-hook)))
      (add-hooks-test-reset 'css-mode-hook 'sgml-mode-hook 'text-mode-hook)
      (add-hooks '((css-mode . add-hooks-test-emmet-mode)
                   ((sgml-mode text-mode) . (add-hooks-test-emmet-mode
                                             add-hooks-test-rainbow-mode))))
      (list :after-one-call once
            :after-the-same-call-again twice
            :unchanged (equal (plist-get once :css) (plist-get twice :css))
            :the-very-same-list (eq (plist-get once :css) (plist-get twice :css))
            :from-an-alist-of-two-pairs
            (list :css css-mode-hook :sgml sgml-mode-hook :text text-mode-hook)
            :and-they-fire (list (add-hooks-test-fire 'css-mode-hook)
                                 (add-hooks-test-fire 'sgml-mode-hook)
                                 (add-hooks-test-fire 'text-mode-hook))))))"##,
        expect![
            "OK (:after-one-call (:css #1=(add-hooks-test-rainbow-mode add-hooks-test-emmet-mode) :sgml #2=(add-hooks-test-rainbow-mode add-hooks-test-emmet-mode) :text #3=(add-hooks-test-rainbow-mode add-hooks-test-emmet-mode) :css-fires (rainbow-mode emmet-mode)) :after-the-same-call-again (:css #1# :sgml #2# :text #3#) :unchanged t :the-very-same-list t :from-an-alist-of-two-pairs (:css (add-hooks-test-emmet-mode) :sgml (add-hooks-test-rainbow-mode add-hooks-test-emmet-mode) :text (add-hooks-test-rainbow-mode add-hooks-test-emmet-mode)) :and-they-fire ((emmet-mode) (rainbow-mode emmet-mode) (rainbow-mode emmet-mode)))"
        ],
    )
}

fn a_lambda_counts_as_one_function_and_a_list_as_several() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_lambda_counts_as_one_function_and_a_list_as_several",
        r##"(let ((hook 'add-hooks-test-plain-hook))
  (add-hooks-test-reset hook)
  (add-hooks-pair 'add-hooks-test-plain (add-hooks-test-recorder 'single))
  (let ((single (list :entries (length add-hooks-test-plain-hook)
                      :fires (add-hooks-test-fire hook))))
    (add-hooks-test-reset hook)
    (add-hooks-pair 'add-hooks-test-plain
                    (list (add-hooks-test-recorder 'first)
                          (add-hooks-test-recorder 'second)))
    (let ((several (list :entries (length add-hooks-test-plain-hook)
                         :fires (add-hooks-test-fire hook))))
      (add-hooks-test-reset hook)
      (add-hooks-pair 'add-hooks-test-plain 'add-hooks-test-emmet-mode)
      (let ((symbol (list :entries (length add-hooks-test-plain-hook)
                          :fires (add-hooks-test-fire hook))))
        (add-hooks-test-reset hook)
        (add-hooks-pair 'add-hooks-test-plain nil)
        (list :one-lambda single
              :two-lambdas several
              :one-symbol symbol
              :nil-functions (list :entries (length add-hooks-test-plain-hook)
                                   :value add-hooks-test-plain-hook)
              :listify (list :lambda (length (add-hooks-listify
                                              (add-hooks-test-recorder 'x)))
                             :list-of-two (length (add-hooks-listify '(a b)))
                             :symbol (add-hooks-listify 'add-hooks-test-emmet-mode)
                             :nil (add-hooks-listify nil)))))))"##,
        expect![
            "OK (:one-lambda (:entries 1 :fires (single)) :two-lambdas (:entries 2 :fires (second first)) :one-symbol (:entries 1 :fires (emmet-mode)) :nil-functions (:entries 0 :value nil) :listify (:lambda 1 :list-of-two 2 :symbol (add-hooks-test-emmet-mode) :nil nil))"
        ],
    )
}

fn a_string_hook_is_refused_at_once_and_a_list_of_non_functions_is_not() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_string_hook_is_refused_at_once_and_a_list_of_non_functions_is_not",
        r##"(progn
  (add-hooks-test-reset 'css-mode-hook 'add-hooks-test-plain-hook)
  (list
   :string-hook
   (list :signal (condition-case error
                     (add-hooks-pair "css-mode" 'add-hooks-test-emmet-mode)
                   (error (list (car error) (cadr error))))
         :hook-untouched css-mode-hook)
   :list-of-non-functions
   (progn
     (add-hooks-pair 'add-hooks-test-plain '(alpha beta))
     (list :stored add-hooks-test-plain-hook
           :looks-normal (listp add-hooks-test-plain-hook)
           :signal-when-run (condition-case error
                                (add-hooks-test-fire 'add-hooks-test-plain-hook)
                              (error (list (car error) (cadr error))))))
   :unevaluated-form
   (progn
     (add-hooks-test-reset 'add-hooks-test-plain-hook)
     (add-hooks-pair 'add-hooks-test-plain '(setq add-hooks-test-fired 'oops))
     (list :stored add-hooks-test-plain-hook
           :signal-when-run (condition-case error
                                (add-hooks-test-fire 'add-hooks-test-plain-hook)
                              (error (list (car error) (cadr error))))))))"##,
        expect![
            "OK (:string-hook (:signal (wrong-type-argument symbolp) :hook-untouched nil) :list-of-non-functions (:stored (beta alpha) :looks-normal t :signal-when-run (void-function beta)) :unevaluated-form (:stored (#1='oops add-hooks-test-fired setq) :signal-when-run (invalid-function #1#)))"
        ],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_documented_example_puts_one_function_on_several_mode_hooks(),
        one_pair_covers_every_hook_crossed_with_every_function(),
        a_lambda_counts_as_one_function_and_a_list_as_several(),
        a_string_hook_is_refused_at_once_and_a_list_of_non_functions_is_not(),
    ]
}
