use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ES_LIB_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const ES_LIB_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn es_lib_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ES_LIB_MELPA_PIN, "es-lib.el")
        .expect("prepare pinned es-lib source below ./tmp")
        .with_timeout(ES_LIB_TEST_TIMEOUT)
}

fn scoped_refactoring_changes_only_the_selected_checkout_section() -> ParityBatchCase {
    ParityBatchCase::value(
        "scoped_refactoring_changes_only_the_selected_checkout_section",
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert
   ";; legacy-checkout compatibility entrypoint\n"
   "(defun legacy-checkout (cart)\n"
   "  (let ((subtotal (cart-subtotal cart))\n"
   "        (tax (cart-tax cart)))\n"
   "    (list :subtotal subtotal :tax tax)))\n"
   "(defun audit-checkout (subtotal tax)\n"
   "  (list subtotal tax))\n")
  (goto-char (point-min))
  (search-forward "(let")
  (let ((start (line-beginning-position))
        (end (save-excursion (forward-line 2) (line-end-position))))
    (string-match "\\(order\\)-\\([0-9]+\\)" "order-42")
    (es-replace-regexp-prog
     "\\_<\\(subtotal\\|tax\\)\\_>" "order-\\1" start end)
    (es-replace-prog "legacy-checkout" "checkout")
    (list :source (buffer-string)
          :point (list (point) (line-number-at-pos) (current-column))
          :restriction (list (point-min) (point-max))
          :match-data (list (match-string 1 "order-42")
                            (match-string 2 "order-42")))))
"##,
        expect![[
            r##"OK (:source ";; checkout compatibility entrypoint\n(defun checkout (cart)\n  (let ((order-subtotal (cart-subtotal cart))\n        (order-tax (cart-tax cart)))\n    (list :subtotal order-subtotal :tax order-tax)))\n(defun audit-checkout (subtotal tax)\n  (list subtotal tax))\n" :point (67 3 6) :restriction (1 257) :match-data ("order" "42"))"##
        ]],
    )
}

fn text_navigation_selects_and_classifies_a_real_indented_record() -> ParityBatchCase {
    ParityBatchCase::value(
        "text_navigation_selects_and_classifies_a_real_indented_record",
        r##"
(with-temp-buffer
  (insert
   "checkout:\n"
   "\n"
   "   \t\n"
   "\torder_total   \n"
   "    status: ready\n"
   "trailer\n")
  (goto-char (point-max))
  (es-goto-previous-non-blank-line)
  (let ((previous (list (line-number-at-pos) (point)
                        (es-line-empty-p) (es-line-visible-p)
                        (es-current-character-indentation)
                        (es-visible-end-of-line)
                        (es-next-visible-character-at-pos))))
    (goto-char (point-min))
    (search-forward "status")
    (es-mark-symbol-at-point)
    (let ((selection (list (es-active-region-string)
                           (region-beginning) (region-end)
                           mark-active deactivate-mark)))
      (deactivate-mark)
      (es-goto-line-prog 4)
      (back-to-indentation)
      (let ((record (list (line-number-at-pos) (point)
                          (es-indentation-end-pos)
                          (es-line-matches-p "order_[a-z]+"))))
        (erase-buffer)
        (insert "{}")
        (goto-char 2)
        (list :previous previous
              :selection selection
              :record record
              :between-pair (es-point-between-pairs-p))))))
"##,
        expect![[
            r##"OK (:previous (6 51 nil t 0 58 116) :selection ("status" 37 43 t nil) :record (4 18 18 1) :between-pair t)"##
        ]],
    )
}

fn buffer_local_shortcuts_execute_distinct_checkout_actions_without_leaking() -> ParityBatchCase {
    ParityBatchCase::value(
        "buffer_local_shortcuts_execute_distinct_checkout_actions_without_leaking",
        r##"
(let ((checkout-buffer (generate-new-buffer " *es-checkout*"))
      (audit-buffer (generate-new-buffer " *es-audit*")))
  (unwind-protect
      (progn
        (fset 'es-test-insert-order
              (lambda () (interactive) (insert "order-created")))
        (fset 'es-test-insert-summary
              (lambda () (interactive) (insert "summary-opened")))
        (fset 'es-test-insert-audit
              (lambda () (interactive) (insert "audit-recorded")))
        (with-current-buffer checkout-buffer
          (emacs-lisp-mode)
          (es-buffer-local-set-keys
           (kbd "C-c e") 'es-test-insert-order
           (kbd "C-c s") 'es-test-insert-summary)
          (call-interactively (key-binding (kbd "C-c e")))
          (insert "|")
          (call-interactively (key-binding (kbd "C-c s"))))
        (let ((checkout-mode
               (with-current-buffer checkout-buffer es-buffer-local-mode))
              audit-before)
          (with-current-buffer audit-buffer
            (emacs-lisp-mode)
            (setq audit-before (key-binding (kbd "C-c e")))
            (es-buffer-local-set-key (kbd "C-c e") 'es-test-insert-audit)
            (call-interactively (key-binding (kbd "C-c e"))))
          (list
           :checkout (with-current-buffer checkout-buffer (buffer-string))
           :audit (with-current-buffer audit-buffer (buffer-string))
           :audit-before audit-before
           :distinct-modes
           (not (eq checkout-mode
                    (with-current-buffer audit-buffer es-buffer-local-mode)))
           :checkout-bindings
           (with-current-buffer checkout-buffer
             (list (key-binding (kbd "C-c e"))
                   (key-binding (kbd "C-c s"))))
           :audit-bindings
           (with-current-buffer audit-buffer
             (list (key-binding (kbd "C-c e"))
                   (key-binding (kbd "C-c s")))))))
    (mapc (lambda (buffer)
            (when (buffer-live-p buffer) (kill-buffer buffer)))
          (list checkout-buffer audit-buffer))
    (mapc (lambda (symbol) (fmakunbound symbol))
          '(es-test-insert-order es-test-insert-summary es-test-insert-audit))))
"##,
        expect![[
            r##"OK (:checkout "order-created|summary-opened" :audit "audit-recorded" :audit-before nil :distinct-modes t :checkout-bindings (es-test-insert-order es-test-insert-summary) :audit-bindings (es-test-insert-audit nil))"##
        ]],
    )
}

fn functional_combinators_and_queue_macros_build_an_order_processing_pipeline() -> ParityBatchCase {
    ParityBatchCase::value(
        "functional_combinators_and_queue_macros_build_an_order_processing_pipeline",
        r##"
(progn
  (require 'subr-x)
  (let* ((normalize-id (es-comp #'upcase #'string-trim))
         (mark-ready (es-back-curry #'concat "-READY"))
         (usable-id-p (es-complement #'string-empty-p))
         (fallback (es-constantly "MANUAL-REVIEW"))
         (metadata-pair (es-flip #'cons))
         (queue '(" order-17 " "" " order-23 "))
         (processed nil))
    (while queue
      (let ((raw (pop queue)))
        (es-back-push
         (if (funcall usable-id-p (string-trim raw))
             (funcall mark-ready (funcall normalize-id raw))
           (funcall fallback raw))
         processed)))
    (let ((last (car (last processed))))
      (setq processed (butlast processed))
      (list :processed processed
            :last last
            :metadata (funcall metadata-pair last 'last-order)
            :neither-empty-nor-review
            (es-neither (string-empty-p last)
                        (string= last "MANUAL-REVIEW"))))))
"##,
        expect![[
            r##"OK (:processed ("ORDER-17-READY" "MANUAL-REVIEW") :last "ORDER-23-READY" :metadata (last-order . "ORDER-23-READY") :neither-empty-nor-review t)"##
        ]],
    )
}

fn overlay_annotations_survive_temporary_detachment_and_serialization() -> ParityBatchCase {
    ParityBatchCase::value(
        "overlay_annotations_survive_temporary_detachment_and_serialization",
        r##"
(with-temp-buffer
  (insert "Order 17: payment pending\nOrder 23: shipped\n")
  (let ((annotation (make-overlay 11 26)))
    (overlay-put annotation 'face 'warning)
    (overlay-put annotation 'help-echo "Payment requires review")
    (overlay-put annotation 'priority 80)
    (overlay-put annotation 'evaporate t)
    (let* ((saved (es-preserve-overlay annotation))
           (detached
            (list (overlay-start annotation) (overlay-end annotation)
                  (overlay-get annotation 'invisible)
                  (overlay-get annotation 'evaporate))))
      (goto-char (point-min))
      (insert "Queue: high priority\n")
      (es-restore-overlay saved)
      (let* ((restored
              (list (overlay-start annotation) (overlay-end annotation)
                    (overlay-get annotation 'face)
                    (overlay-get annotation 'help-echo)
                    (overlay-get annotation 'priority)
                    (overlay-get annotation 'invisible)
                    (overlay-get annotation 'evaporate)))
             (serialized (es-virtualize-overlay annotation))
             (realized (es-realize-overlay serialized)))
        (list :detached detached
              :restored restored
              :serialized-range (seq-take serialized 2)
              :deleted (overlay-buffer annotation)
              :realized
              (list (overlay-start realized) (overlay-end realized)
                    (overlay-get realized 'face)
                    (overlay-get realized 'help-echo)
                    (overlay-get realized 'priority)
                    (buffer-substring-no-properties
                     (overlay-start realized) (overlay-end realized))))))))
"##,
        expect![[
            r##"OK (:detached (1 1 t nil) :restored (11 26 warning "Payment requires review" 80 nil t) :serialized-range (11 26) :deleted nil :realized (11 26 warning "Payment requires review" 80 "h priority\nOrde"))"##
        ]],
    )
}

fn folded_checkout_sections_move_as_complete_visual_lines() -> ParityBatchCase {
    ParityBatchCase::value(
        "folded_checkout_sections_move_as_complete_visual_lines",
        r##"
(save-window-excursion
  (with-temp-buffer
    (set-window-buffer (selected-window) (current-buffer))
    (setq-local truncate-lines nil)
    (setq-local buffer-invisibility-spec '(checkout-fold))
    (insert
     "order-17\n"
     "  item: keyboard\n"
     "  item: mouse\n"
     "  total: 120\n"
     "order-23\n"
     "  status: shipped\n")
    (let ((fold-start (save-excursion
                        (goto-char (point-min))
                        (line-end-position)))
          (fold-end (save-excursion
                      (goto-char (point-min))
                      (forward-line 4)
                      (point))))
      (let ((fold (make-overlay fold-start fold-end)))
        (overlay-put fold 'invisible 'checkout-fold)
        (goto-char (point-min))
        (let ((first
               (list :begin (es-total-line-beginning-position)
                     :end (es-total-line-end-position)
                     :folded (es-line-folded-p))))
          (es-total-forward-line 1)
          (let ((next (list :point (point)
                            :line (line-number-at-pos)
                            :text (buffer-substring-no-properties
                                   (line-beginning-position)
                                   (line-end-position)))))
            (es-total-forward-line -1)
            (list :first first
                  :next next
                  :back (list (point) (line-number-at-pos)))))))))
"##,
        expect![[
            r##"OK (:first (:begin 1 :end 62 :folded t) :next (:point 63 :line 6 :text "  status: shipped") :back (1 1))"##
        ]],
    )
}

fn editing_helpers_normalize_a_checkout_configuration_and_palette() -> ParityBatchCase {
    ParityBatchCase::value(
        "editing_helpers_normalize_a_checkout_configuration_and_palette",
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert
   "(setq checkout-enabled true)\n"
   "(setq retry-enabled false)\n"
   "(setq audit-enabled nil)\n"
   ";; FIXME verify tax table\n")
  (let (results)
    (dolist (location '((1 "true") (2 "false") (3 "nil") (4 "FIXME")))
      (goto-char (point-min))
      (forward-line (1- (car location)))
      (search-forward (cadr location) (line-end-position))
      (push (es-toggle-true-false-maybe) results))
    (goto-char (point-min))
    (search-forward "checkout-enabled")
    (es-add-semicolon-at-eol)
    (goto-char (point-min))
    (search-forward "retry-enabled")
    (es-add-comma-at-eol)
    (list
     :source (buffer-string)
     :changed (nreverse results)
     :palette
     (mapcar (lambda (color)
               (list color
                     (es-color-normalize-hex color)
                     (es-color-hex-to-list color)))
             '("#3aF" "#102A4C"))
     :rendered (es-color-list-to-hex '(16 42 76))
     :template
     (es-replace-in-string-multiple
      "Order {{id}} for {{customer}}: {{status}}"
      '(("{{id}}" . "17")
        ("{{customer}}" . "Ada")
        ("{{status}}" . "ready")))
     :duplicates
     (es-find-duplicates '(order-17 order-23 order-17 order-42 order-23 order-17)))))
"##,
        expect![[
            r##"OK (:source "(setq checkout-enabled false);\n(setq retry-enabled true),\n(setq audit-enabled t)\n;; FIXED verify tax table\n" :changed (t t t t) :palette (("#3aF" "#33AAFF" (51 170 255)) ("#102A4C" "#102A4C" (16 42 76))) :rendered "#102A4C" :template "Order 17 for Ada: ready" :duplicates (order-17 order-23 order-17))"##
        ]],
    )
}

#[test]
fn es_lib_package_batch() {
    let cases = vec![
        scoped_refactoring_changes_only_the_selected_checkout_section(),
        text_navigation_selects_and_classifies_a_real_indented_record(),
        buffer_local_shortcuts_execute_distinct_checkout_actions_without_leaking(),
        functional_combinators_and_queue_macros_build_an_order_processing_pipeline(),
        overlay_annotations_survive_temporary_detachment_and_serialization(),
        folded_checkout_sections_move_as_complete_visual_lines(),
        editing_helpers_normalize_a_checkout_configuration_and_palette(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed es-lib parity test");
    assert_oracle_batch_cases(es_lib_oracle(), test_name, "es_lib_parity", &cases);
}
