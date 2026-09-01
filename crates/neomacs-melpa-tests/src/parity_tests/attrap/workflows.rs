use expect_test::expect;

use super::ParityBatchCase;

fn checkdoc_complaints_in_a_real_elisp_file_are_repaired_in_the_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "checkdoc_complaints_in_a_real_elisp_file_are_repaired_in_the_buffer",
        r##"
(let ((buffer (attrap-test-open)))
  (let ((found (attrap-test-diagnostics buffer)))
    (with-current-buffer buffer (set-buffer-modified-p nil))
    (kill-buffer buffer)
    (list
     :source attrap-test-sample
     ;; Real diagnostics from `elisp-flymake-checkdoc', which runs the
     ;; checker in a subprocess against the file on disk.
     :diagnostics found
     ;; Each repair below is `M-x attrap-attrap' with point on that
     ;; diagnostic, in a freshly checked copy of the same file.
     :capitalize (attrap-test-repair "First line should be capitalized")
     :two-spaces (attrap-test-repair "two spaces after a period")
     :punctuation (attrap-test-repair "should end with punctuation")
     :section-header (attrap-test-repair "section marked"))))
"##,
        expect![[
            r#"OK (:source ";;; sample.el --- A sample\n(defun sample-greet (name)\n  \"say hello to NAME. it is nice\"\n  (message \"hello %s.\" name))\n" :diagnostics ((:beg 1 :end 2 :backend elisp-flymake-checkdoc :text "You should have a section marked \";;; Commentary:\"") (:beg 29 :end 30 :backend elisp-flymake-checkdoc :text "You should have a section marked \";;; Code:\"") (:beg 58 :end 59 :backend elisp-flymake-checkdoc :text "First line should be capitalized") (:beg 75 :end 77 :backend elisp-flymake-checkdoc :text "There should be two spaces after a period") (:beg 87 :end 88 :backend elisp-flymake-checkdoc :text "First sentence should end with punctuation") (:beg 118 :end 119 :backend elisp-flymake-checkdoc :text "The footer should be: (provide ’sample)\\n;;; sample.el ends here")) :capitalize (:diagnostic "First line should be capitalized" :offered nil :signalled nil :source ";;; sample.el --- A sample\n(defun sample-greet (name)\n  \"Say hello to NAME. it is nice\"\n  (message \"hello %s.\" name))\n") :two-spaces (:diagnostic "There should be two spaces after a period" :offered nil :signalled nil :source ";;; sample.el --- A sample\n(defun sample-greet (name)\n  \"say hello to NAME.  it is nice\"\n  (message \"hello %s.\" name))\n") :punctuation (:diagnostic "First sentence should end with punctuation" :offered nil :signalled nil :source ";;; sample.el --- A sample\n(defun sample-greet (name)\n  \"say hello to NAME. it is nice.\"\n  (message \"hello %s.\" name))\n") :section-header (:diagnostic "You should have a section marked \";;; Commentary:\"" :offered nil :signalled nil :source ";;; Commentary:\n;;; sample.el --- A sample\n(defun sample-greet (name)\n  \"say hello to NAME. it is nice\"\n  (message \"hello %s.\" name))\n"))"#
        ]],
    )
}

fn the_footer_repair_copies_checkdocs_curly_quote_and_leaves_the_file_unreadable() -> ParityBatchCase
{
    ParityBatchCase::value(
        "the_footer_repair_copies_checkdocs_curly_quote_and_leaves_the_file_unreadable",
        r##"
(let ((repaired (attrap-test-repair "footer should be")))
  (list
   :repair repaired
   ;; checkdoc words its message with a right single quotation mark, and the
   ;; fixer inserts the message text verbatim.  The footer it writes is not
   ;; the quote form it looks like, so the repaired file no longer reads.
   :inserted-footer
   (car (last (split-string (plist-get repaired :source) "\n" t)
              2))
   :quote-character
   (let* ((source (plist-get repaired :source))
          (at (string-match "provide" source)))
     (and at (char-to-string (aref source (+ at 8)))))
   ;; It still reads, which is the trap: the reader takes the curly quote
   ;; as an ordinary symbol character, so the footer is a call to `provide'
   ;; with a symbol whose name begins with a right single quotation mark --
   ;; not the feature this file provides.
   :reads-as
   (condition-case error
       (car (read-from-string
             (substring (plist-get repaired :source)
                        (string-match "(provide" (plist-get repaired :source)))))
     (error (attrap-test-plain error)))
   :feature-it-would-provide
   (condition-case error
       (let ((form (car (read-from-string
                         (substring (plist-get repaired :source)
                                    (string-match
                                     "(provide"
                                     (plist-get repaired :source)))))))
         (list :quoted-p (eq (car-safe (nth 1 form)) 'quote)
               :symbol-name (symbol-name (nth 1 form))
               :is-the-feature (eq (nth 1 form) 'sample)))
     (error (attrap-test-plain error)))))
"##,
        expect![[
            r#"OK (:repair (:diagnostic "The footer should be: (provide ’sample)\\n;;; sample.el ends here" :offered nil :signalled nil :source ";;; sample.el --- A sample\n(defun sample-greet (name)\n  \"say hello to NAME. it is nice\"\n  (message \"hello %s.\" name))\n(provide ’sample)\n;;; sample.el ends here\n") :inserted-footer "(provide ’sample)" :quote-character "’" :reads-as (provide ’sample) :feature-it-would-provide (:quoted-p nil :symbol-name "’sample" :is-the-feature nil))"#
        ]],
    )
}

fn a_lone_repair_is_applied_without_asking_and_several_are_offered_by_name() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_lone_repair_is_applied_without_asking_and_several_are_offered_by_name",
        r##"
(list
 ;; `attrap-select-and-apply-option' short-circuits a single option, so a
 ;; one-repair diagnostic never reaches the prompt: nothing was offered.
 :single-repair-was-not-offered
 (plist-get (attrap-test-repair "First line should be capitalized") :offered)
 ;; A fixer that returns two options does reach it.  Both descriptions are
 ;; the fixer's own symbols, and either can be chosen.
 :two-repairs
 (attrap-test-fixer #'attrap-LaTeX-fixer
                    "Use either `` or '' as an alternative to `\"'."
                    "Say \"hi\" to them.\n" 5)
 ;; With no option at all the command says so rather than doing nothing.
 :no-repair-applies
 (attrap-test-fixer #'attrap-elisp-fixer "Something checkdoc never says"))
"##,
        expect![[
            r#"OK (:single-repair-was-not-offered nil :two-repairs (:options (fix-open-dquote fix-close-dquote) :buffer-untouched t :buffer "Say \"hi\" to them.\n" :applying-each ((fix-open-dquote . "Say ``hi\" to them.\n") (fix-close-dquote . "Say ''hi\" to them.\n"))) :no-repair-applies (:options nil :buffer-untouched t :buffer "" :applying-each nil))"#
        ]],
    )
}

fn the_command_reports_exactly_which_thing_is_missing() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_command_reports_exactly_which_thing_is_missing",
        r##"
(list
 ;; flymake is on, but point is nowhere near a diagnostic.
 :no-diagnostic-at-point
 (let ((buffer (attrap-test-open)))
   (unwind-protect
       (with-current-buffer buffer
         (goto-char (point-max))
         (condition-case error (progn (attrap-attrap (point)) nil)
           (error (attrap-test-plain error))))
     (with-current-buffer buffer (set-buffer-modified-p nil))
     (kill-buffer buffer)))
 ;; Neither checker is running.
 :no-checker
 (with-temp-buffer
   (condition-case error (progn (attrap-attrap (point)) nil)
     (error (attrap-test-plain error))))
 ;; flycheck is not installed, so the flycheck entry point cannot even
 ;; reach its own error message.
 :flycheck-entry-point
 (with-temp-buffer
   (condition-case error (progn (attrap-flycheck (point)) nil)
     (error (attrap-test-plain error))))
 ;; A diagnostic from a backend nobody registered yields no fixer, and the
 ;; selection step is what reports it.
 :unregistered-backend
 (let ((attrap-flymake-backends-alist nil))
   (let ((buffer (attrap-test-open)))
     (unwind-protect
         (with-current-buffer buffer
           (goto-char (flymake-diagnostic-beg (car (flymake-diagnostics))))
           (condition-case error (progn (attrap-attrap (point)) nil)
             (error (attrap-test-plain error))))
       (with-current-buffer buffer (set-buffer-modified-p nil))
       (kill-buffer buffer))))
 :registered-backends
 (mapcar #'car attrap-flymake-backends-alist)
 :registered-checkers
 (mapcar #'car attrap-flycheck-checkers-alist))
"##,
        expect![[
            r#"OK (:no-diagnostic-at-point (error "No flymake diagnostic at point") :no-checker (error "Expecting flymake or flycheck to be active") :flycheck-entry-point (void-function flycheck-overlays-at) :unregistered-backend (error "No fixer applies to the issue at point") :registered-backends (dante-flymake LaTeX-flymake attrap-flymake-hlint elisp-flymake-byte-compile elisp-flymake-checkdoc) :registered-checkers (haskell-dante emacs-lisp))"#
        ]],
    )
}

fn the_latex_fixer_rewrites_the_buffer_while_it_is_only_supposed_to_list_options() -> ParityBatchCase
{
    ParityBatchCase::value(
        "the_latex_fixer_rewrites_the_buffer_while_it_is_only_supposed_to_list_options",
        r##"
(list
 ;; The Commentary is explicit: "A fixer is a element is a side-effect-free
 ;; function mapping an error message MSG to a list of options."  Every
 ;; clause of this fixer honours that except one.
 :ellipsis
 (attrap-test-fixer #'attrap-LaTeX-fixer
                    "You should use \\ldots to achieve an ellipsis."
                    "Some text ... and more.\n" 11)
 ;; For contrast, three clauses of the same fixer that behave: they return
 ;; named options and leave the buffer alone until one is applied.
 :quotes
 (attrap-test-fixer #'attrap-LaTeX-fixer
                    "Use either `` or '' as an alternative to `\"'."
                    "Say \"hi\" to them.\n" 5)
 :interword
 (attrap-test-fixer #'attrap-LaTeX-fixer
                    "Interword spacing (`\\ ') should perhaps be used."
                    "Dr. Smith arrived.\n" 4)
 :terminated-with-space
 (attrap-test-fixer #'attrap-LaTeX-fixer
                    "Command terminated with space"
                    "\\alpha beta\n" 7))
"##,
        expect![[
            r#"OK (:ellipsis (:options nil :buffer-untouched nil :buffer "Some text \\ldots and more.\n" :applying-each nil) :quotes (:options (fix-open-dquote fix-close-dquote) :buffer-untouched t :buffer "Say \"hi\" to them.\n" :applying-each ((fix-open-dquote . "Say ``hi\" to them.\n") (fix-close-dquote . "Say ''hi\" to them.\n"))) :interword (:options (use-interword-spacing) :buffer-untouched t :buffer "Dr. Smith arrived.\n" :applying-each ((use-interword-spacing . "Dr.\\ Smith arrived.\n"))) :terminated-with-space (:options (add-empty-argument) :buffer-untouched t :buffer "\\alpha beta\n" :applying-each ((add-empty-argument . "\\alpha{} beta\n"))))"#
        ]],
    )
}

fn every_elisp_rule_turns_its_message_into_a_named_repair() -> ParityBatchCase {
    ParityBatchCase::value(
        "every_elisp_rule_turns_its_message_into_a_named_repair",
        r##"
(mapcar
 (lambda (case)
   (cons (car case)
         (attrap-test-fixer #'attrap-elisp-fixer
                            (nth 1 case) (nth 2 case) (nth 3 case))))
 ;; One entry per branch of `attrap-elisp-fixer', each with the message
 ;; checkdoc or the byte compiler actually words it that way and a buffer
 ;; the repair has something to do to.
 '((:first-line
    "The first line should be of the form: \";;; package --- Summary\""
    ";;; sample.el\n" 1)
   (:section
    "You should have a section marked \";;; Code:\""
    "(defun f ())\n" 1)
   (:symbol-quotes
    "Lisp symbol ‘nil’ should appear in quotes"
    "  \"Return nil when done.\"\n" 1)
   (:message-period
    "Error messages should *not* end with a period"
    "  (error \"Bad input.\")\n" 1)
   (:capitalize-emacs
    "Name emacs should appear capitalized as Emacs"
    "  \"Run under emacs only.\"\n" 1)
   (:capitalize
    "First line should be capitalized"
    "  \"say hello.\"\n" 6)
   (:trailing-space
    "White space found at end of line"
    "(defun f ())   \n" 1)
   (:two-spaces
    "There should be two spaces after a period"
    "  \"One. Two.\"\n" 1)
   (:might-as-well-document
    "All variables and subroutines might as well have a documentation string"
    "(defun f ()\n  nil)\n" 13)
   (:should-document
    "Argument should have documentation"
    "(defun f ()\n  nil)\n" 13)
   (:footer
    "The footer should be: (provide 'sample)\\n;;; sample.el ends here"
    "(defun f ())\n" 12)
   (:incomplete-sentence
    "First line is not a complete sentence"
    "  \"A summary\n  continued here.\"\n" 12)
   (:sentence-punctuation
    "First sentence should end with punctuation"
    "  \"A summary\"\n" 12)))
"##,
        expect![[
            r#"OK ((:first-line :options (insert-package) :buffer-untouched t :buffer ";;; sample.el\n" :applying-each ((insert-package . ";;; package --- Summary\n;;; sample.el\n"))) (:section :options (insert-section-header) :buffer-untouched t :buffer "(defun f ())\n" :applying-each ((insert-section-header . ";;; Code:\n(defun f ())\n"))) (:symbol-quotes :options (kill-message-period) :buffer-untouched t :buffer "  \"Return nil when done.\"\n" :applying-each ((kill-message-period . "  \"Return `nil' when done.\"\n"))) (:message-period :options (kill-message-period) :buffer-untouched t :buffer "  (error \"Bad input.\")\n" :applying-each ((kill-message-period . "  (error \"Bad input\")\n"))) (:capitalize-emacs :options (capitalize-emacs) :buffer-untouched t :buffer "  \"Run under emacs only.\"\n" :applying-each ((capitalize-emacs . "  \"Run under Emacs only.\"\n"))) (:capitalize :options (capitalize) :buffer-untouched t :buffer "  \"say hello.\"\n" :applying-each ((capitalize . "  \"saY hello.\"\n"))) (:trailing-space :options (delete-trailing-space) :buffer-untouched t :buffer "(defun f ())   \n" :applying-each ((delete-trailing-space . "(defun f ())\n"))) (:two-spaces :options (add-space) :buffer-untouched t :buffer "  \"One. Two.\"\n" :applying-each ((add-space . "  \"One.  Two.\"\n"))) (:might-as-well-document :options (add-empty-doc) :buffer-untouched t :buffer "(defun f ()\n  nil)\n" :applying-each ((add-empty-doc . "(defun f ()\n  \"\"\n  nil)\n"))) (:should-document :options (add-empty-doc) :buffer-untouched t :buffer "(defun f ()\n  nil)\n" :applying-each ((add-empty-doc . "(defun f ()\n  \"\"\n  nil)\n"))) (:footer :options (add-footer) :buffer-untouched t :buffer "(defun f ())\n" :applying-each ((add-footer . "(defun f ())\n(provide 'sample)\n;;; sample.el ends here\n"))) (:incomplete-sentence :options (merge-lines) :buffer-untouched t :buffer "  \"A summary\n  continued here.\"\n" :applying-each ((merge-lines . "  \"A summary  continued here.\"\n"))) (:sentence-punctuation :options (add-punctuation) :buffer-untouched t :buffer "  \"A summary\"\n" :applying-each ((add-punctuation . "  \"A summar.y\"\n"))))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        checkdoc_complaints_in_a_real_elisp_file_are_repaired_in_the_buffer(),
        the_footer_repair_copies_checkdocs_curly_quote_and_leaves_the_file_unreadable(),
        a_lone_repair_is_applied_without_asking_and_several_are_offered_by_name(),
        the_command_reports_exactly_which_thing_is_missing(),
        the_latex_fixer_rewrites_the_buffer_while_it_is_only_supposed_to_list_options(),
        every_elisp_rule_turns_its_message_into_a_named_repair(),
    ]
}
