use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, LINK_HINT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'button)
(require 'cl-lib)
(require 'link-hint)

(defvar neomacs-link-hint-test-opened nil)

(defun neomacs-link-hint-test-with-buffer (text body)
  "Run BODY in a displayed buffer containing TEXT."
  (let ((buffer (generate-new-buffer " *link-hint-parity*")))
    (unwind-protect
        (save-window-excursion
          (delete-other-windows)
          (set-window-buffer (selected-window) buffer)
          (set-buffer buffer)
          (insert text)
          (goto-char (point-min))
          (set-window-start (selected-window) (point-min))
          (set-window-point (selected-window) (point-min))
          (funcall body))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))

(defun neomacs-link-hint-test-next-ticket (bound)
  "Return the position of the next ticket-property link before BOUND."
  (let ((position (point)))
    (when (< position bound)
      (setq position
            (if (get-text-property position 'neomacs-link-hint-ticket)
                (next-single-property-change
                 position 'neomacs-link-hint-ticket nil bound)
              (min (1+ position) bound)))
      (while (and (< position bound)
                  (not (get-text-property
                        position 'neomacs-link-hint-ticket)))
        (setq position
              (next-single-property-change
               position 'neomacs-link-hint-ticket nil bound)))
      (and (< position bound)
           (get-text-property position 'neomacs-link-hint-ticket)
           position))))

(defun neomacs-link-hint-test-ticket-at-point ()
  "Return the ticket identifier at point."
  (get-text-property (point) 'neomacs-link-hint-ticket))

(defun neomacs-link-hint-test-open-ticket (ticket)
  "Record opening TICKET at the test's external issue-tracker boundary."
  (push ticket neomacs-link-hint-test-opened))

(defun neomacs-link-hint-test-mark-tickets ()
  "Mark every NEO ticket in the current buffer as a custom link."
  (save-excursion
    (goto-char (point-min))
    (while (re-search-forward "\\_<NEO-[0-9]+\\_>" nil t)
      (add-text-properties
       (match-beginning 0) (match-end 0)
       (list 'neomacs-link-hint-ticket (match-string-no-properties 0))))))
"####;

fn url_copy_and_single_candidate_open_preserve_user_context() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-link-hint-test-with-buffer
 "Release notes: (https://example.com/releases/2.4)"
 (lambda ()
   (let ((link-hint-types '(link-hint-text-url))
         (link-hint-delete-trailing-paren t)
         (link-hint-restore t)
         (avy-single-candidate-jump t)
         (select-enable-clipboard nil)
         (select-enable-primary nil)
         (kill-ring nil)
         (kill-ring-yank-pointer nil)
         opened
         messages)
     (let ((link-hint-message
            (lambda (format-string &rest arguments)
              (push (apply #'format format-string arguments) messages))))
       (search-forward "releases")
       (link-hint-copy-link-at-point)
       (let ((copied (current-kill 0)))
         (goto-char (point-min))
         (let ((before (point)))
           (cl-letf (((symbol-function 'browse-url)
                      (lambda (url &rest _arguments) (push url opened))))
             (link-hint-open-link))
           (list :copied copied
                 :opened (nreverse opened)
                 :point-before before
                 :point-after (point)
                 :context-restored (= before (point))
                 :messages (nreverse messages))))))))
"####;
    let expected = expect![[
        r#"OK (:copied "https://example.com/releases/2.4" :opened ("https://example.com/releases/2.4") :point-before 1 :point-after 1 :context-restored t :messages ("Copied https://example.com/releases/2.4" "Opened https://example.com/releases/2.4"))"#
    ]];
    ParityBatchCase::value(
        "url_copy_and_single_candidate_open_preserve_user_context",
        elisp_form,
        expected,
    )
}

fn overlapping_shr_and_button_links_respect_configured_priority() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-link-hint-test-with-buffer
 "Release dashboard"
 (lambda ()
   (let* ((messages nil)
          (activated nil)
          (kill-ring nil)
          (kill-ring-yank-pointer nil)
          (select-enable-clipboard nil)
          (select-enable-primary nil)
          (link-hint-message
           (lambda (format-string &rest arguments)
             (push (apply #'format format-string arguments) messages))))
     (make-text-button
      (point-min) (point-max)
      'follow-link t
      'action (lambda (button)
                (push (button-label button) activated)))
     (add-text-properties
      (point-min) (point-max)
      '(shr-url "https://ops.example.com/releases/current"))
     (goto-char (+ (point-min) 3))
     (let ((link-hint-types '(link-hint-shr-url link-hint-button)))
       (link-hint-copy-link-at-point))
     (let ((shr-copy (current-kill 0)))
       (let ((link-hint-types '(link-hint-button link-hint-shr-url)))
         (link-hint-open-link-at-point))
       (list :shr-copy shr-copy
             :button-activated (nreverse activated)
             :point (point)
             :messages (nreverse messages))))))
"####;
    let expected = expect![[
        r#"OK (:shr-copy "https://ops.example.com/releases/current" :button-activated ("Release dashboard") :point 4 :messages ("Copied https://ops.example.com/releases/current" "Opened Release dashboard"))"#
    ]];
    ParityBatchCase::value(
        "overlapping_shr_and_button_links_respect_configured_priority",
        elisp_form,
        expected,
    )
}

fn custom_ticket_type_opens_and_copies_all_visible_work_items() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-link-hint-test-with-buffer
 "Deploy NEO-101 after NEO-202 passes review."
 (lambda ()
   (let* ((type 'link-hint-neomacs-ticket)
          (old-plist (copy-sequence (symbol-plist type)))
          (messages nil)
          (kill-ring nil)
          (kill-ring-yank-pointer nil)
          (select-enable-clipboard nil)
          (select-enable-primary nil)
          (neomacs-link-hint-test-opened nil)
          (link-hint-message
           (lambda (format-string &rest arguments)
             (push (apply #'format format-string arguments) messages))))
     (unwind-protect
         (progn
           (neomacs-link-hint-test-mark-tickets)
           (link-hint-define-type 'neomacs-ticket
             :next #'neomacs-link-hint-test-next-ticket
             :at-point-p #'neomacs-link-hint-test-ticket-at-point
             :open #'neomacs-link-hint-test-open-ticket
             :open-multiple t
             :copy #'kill-new
             :copy-multiple t
             :describe (lambda (ticket) (format "ticket %s" ticket)))
           (let ((link-hint-types (list type)))
             (goto-char (point-min))
             (search-forward "NEO-101")
             (backward-char 2)
             (link-hint-open-link-at-point)
             (let ((at-point-opened
                    (nreverse neomacs-link-hint-test-opened)))
               (setq neomacs-link-hint-test-opened nil)
               (goto-char (point-min))
               (link-hint-open-all-links)
               (let ((all-opened
                      (nreverse neomacs-link-hint-test-opened)))
                 (link-hint-copy-all-links)
                 (list :at-point-opened at-point-opened
                       :all-opened all-opened
                       :kill-ring kill-ring
                       :messages (nreverse messages))))))
       (setplist type old-plist)))))
"####;
    let expected = expect![[
        r#"OK (:at-point-opened ("NEO-101") :all-opened ("NEO-101" "NEO-202") :kill-ring ("NEO-202" "NEO-101") :messages ("Opened ticket NEO-101" "Opened 2 links" "Copied 2 links"))"#
    ]];
    ParityBatchCase::value(
        "custom_ticket_type_opens_and_copies_all_visible_work_items",
        elisp_form,
        expected,
    )
}

fn at_point_fallback_distinguishes_handled_and_unhandled_actions() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-link-hint-test-with-buffer
 "deploy_release();"
 (lambda ()
   (let* ((link-hint-types '(link-hint-text-url))
          (attempts nil)
          (messages nil)
          (link-hint-message
           (lambda (format-string &rest arguments)
             (push (apply #'format format-string arguments) messages))))
     (search-forward "deploy_release")
     (let ((link-hint-action-fallback-commands
            (list :open (lambda () (push 'handled attempts) t))))
       (link-hint-open-link-at-point))
     (let ((link-hint-action-fallback-commands
            (list :open (lambda () (push 'unhandled attempts) nil))))
       (link-hint-open-link-at-point))
     (list :attempts (nreverse attempts)
           :point (point)
           :messages (nreverse messages)))))
"####;
    let expected = expect![[
        r#"OK (:attempts (handled unhandled) :point 15 :messages ("There is no link supporting the :open action at the point."))"#
    ]];
    ParityBatchCase::value(
        "at_point_fallback_distinguishes_handled_and_unhandled_actions",
        elisp_form,
        expected,
    )
}

fn file_link_copy_and_open_use_the_real_local_file_workflow() -> ParityBatchCase {
    let elisp_form = r####"
(let ((file (expand-file-name
             "link-hint-release-notes.txt" temporary-file-directory)))
  (unwind-protect
      (progn
        (with-temp-file file
          (insert "release=2.4.0\nstatus=approved\n"))
        (neomacs-link-hint-test-with-buffer
         (concat "Deployment details:\n" file "\n")
         (lambda ()
           (let ((origin (current-buffer))
                 (link-hint-types '(link-hint-file-link))
                 (link-hint-message nil)
                 (select-enable-clipboard nil)
                 (select-enable-primary nil)
                 (kill-ring nil)
                 (kill-ring-yank-pointer nil))
             (search-forward file)
             (backward-char 4)
             (let ((origin-point (point)))
               (link-hint-copy-link-at-point)
               (let ((copied (current-kill 0)))
                 (link-hint-open-link-at-point)
                 (list :copied-path-matches (equal copied file)
                       :visited-path-matches (equal buffer-file-name file)
                       :visited-file-name (file-name-nondirectory
                                           buffer-file-name)
                       :contents (buffer-substring-no-properties
                                  (point-min) (point-max))
                       :origin-restored
                       (= origin-point (with-current-buffer origin (point))))))))))
    (let ((visited (get-file-buffer file)))
      (when (buffer-live-p visited)
        (kill-buffer visited)))
    (when (file-exists-p file)
      (delete-file file))))
"####;
    let expected = expect![[
        r#"OK (:copied-path-matches t :visited-path-matches t :visited-file-name "link-hint-release-notes.txt" :contents "release=2.4.0\nstatus=approved\n" :origin-restored t)"#
    ]];
    ParityBatchCase::value(
        "file_link_copy_and_open_use_the_real_local_file_workflow",
        elisp_form,
        expected,
    )
}

fn copying_from_another_window_restores_selection_and_link_point() -> ParityBatchCase {
    let elisp_form = r####"
(let ((origin (generate-new-buffer " *link-hint-origin*"))
      (links (generate-new-buffer " *link-hint-links*")))
  (unwind-protect
      (save-window-excursion
        (delete-other-windows)
        (with-current-buffer origin
          (insert "Deployment checklist\nKeep editing here"))
        (with-current-buffer links
          (insert "Release dashboard")
          (add-text-properties
           (point-min) (point-max)
           '(shr-url "https://ops.example.com/releases/2.4")))
        (let* ((origin-window (selected-window))
               (links-window (split-window-right)))
          (set-window-buffer origin-window origin)
          (set-window-buffer links-window links)
          (set-window-start origin-window 1)
          (set-window-start links-window 1)
          (set-window-point origin-window 8)
          (set-window-point links-window 4)
          (select-window origin-window)
          (let ((link-hint-types '(link-hint-shr-url))
                (link-hint-avy-all-windows t)
                (link-hint-restore t)
                (avy-single-candidate-jump t)
                (select-enable-clipboard nil)
                (select-enable-primary nil)
                (kill-ring nil)
                (kill-ring-yank-pointer nil)
                messages)
            (let ((link-hint-message
                   (lambda (format-string &rest arguments)
                     (push (apply #'format format-string arguments)
                           messages))))
              (link-hint-copy-link))
            (list :copied (current-kill 0)
                  :selected-origin (eq (selected-window) origin-window)
                  :current-origin (eq (current-buffer) origin)
                  :origin-point (window-point origin-window)
                  :link-point (window-point links-window)
                  :messages (nreverse messages)))))
    (when (buffer-live-p origin) (kill-buffer origin))
    (when (buffer-live-p links) (kill-buffer links))))
"####;
    let expected = expect![[
        r#"OK (:copied "https://ops.example.com/releases/2.4" :selected-origin t :current-origin t :origin-point 8 :link-point 4 :messages ("Copied https://ops.example.com/releases/2.4"))"#
    ]];
    ParityBatchCase::value(
        "copying_from_another_window_restores_selection_and_link_point",
        elisp_form,
        expected,
    )
}

#[test]
fn link_hint_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(LINK_HINT_MELPA_PIN, "link-hint.el")
            .expect("prepare revision-pinned Link Hint source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "link-hint-package-batch",
        "Link Hint",
        &[
            url_copy_and_single_candidate_open_preserve_user_context(),
            overlapping_shr_and_button_links_respect_configured_priority(),
            custom_ticket_type_opens_and_copies_all_visible_work_items(),
            at_point_fallback_distinguishes_handled_and_unhandled_actions(),
            file_link_copy_and_open_use_the_real_local_file_workflow(),
            copying_from_another_window_restores_selection_and_link_point(),
        ],
    );
}
