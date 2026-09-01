use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, KEYFREQ_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const KEYFREQ_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const KEYFREQ_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'keyfreq)

(defvar keyfreq-test-pristine-buffers (buffer-list))

(define-derived-mode keyfreq-test-ops-mode fundamental-mode "Keyfreq-Ops")
(define-derived-mode keyfreq-test-review-mode fundamental-mode "Keyfreq-Review")

(defun keyfreq-test-deploy ()
  (interactive)
  (insert "[deploy]"))

(defun keyfreq-test-retry ()
  (interactive)
  (insert "[retry]"))

(defun keyfreq-test-audit ()
  (interactive)
  (insert "[audit]"))

(defun keyfreq-test-flush ()
  (interactive))

(dolist (binding '(("d" . keyfreq-test-deploy)
                   ("r" . keyfreq-test-retry)
                   ("a" . keyfreq-test-audit)
                   ("!" . keyfreq-test-flush)))
  (define-key keyfreq-test-ops-mode-map (car binding) (cdr binding))
  (define-key keyfreq-test-review-mode-map (car binding) (cdr binding)))

(defun keyfreq-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun keyfreq-test-reset ()
  (when (timerp keyfreq-autosave--timer)
    (cancel-timer keyfreq-autosave--timer))
  (setq keyfreq-autosave--timer nil
        keyfreq-autosave-mode nil
        keyfreq-mode nil
        keyfreq-table (make-hash-table :test 'equal :size 128)
        keyfreq-excluded-commands nil
        keyfreq-excluded-regexp nil
        real-last-command nil)
  (remove-hook 'pre-command-hook #'keyfreq-pre-command-hook)
  (remove-hook 'kill-emacs-hook #'keyfreq-mustsave--do)
  (dolist (buffer (buffer-list))
    (unless (memq buffer keyfreq-test-pristine-buffers)
      (kill-buffer buffer)))
  (let ((directory (keyfreq-test-path "keyfreq")))
    (when (file-directory-p directory)
      (delete-directory directory t))
    (make-directory directory t)
    (setq keyfreq-file (expand-file-name "frequencies.eld" directory)
          keyfreq-file-lock (expand-file-name "frequencies.lock" directory))))

(defun keyfreq-test-table (&rest entries)
  (let ((table (make-hash-table :test 'equal :size 32)))
    (dolist (entry entries table)
      (puthash (cons (nth 0 entry) (nth 1 entry)) (nth 2 entry) table))))

(defun keyfreq-test-canonical-table (table)
  (let (rows)
    (maphash (lambda (key count)
               (push (list (car key) (cdr key) count) rows))
             table)
    (sort rows (lambda (left right)
                 (string< (prin1-to-string left)
                          (prin1-to-string right))))))

(defun keyfreq-test-state-file ()
  (when (file-exists-p keyfreq-file)
    (with-temp-buffer
      (insert-file-contents keyfreq-file)
      (goto-char (point-min))
      (let ((records (read (current-buffer))))
        (sort (mapcar (lambda (record)
                        (list (caar record) (cdar record) (cdr record)))
                      records)
              (lambda (left right)
                (string< (prin1-to-string left)
                         (prin1-to-string right))))))))

(defun keyfreq-test-file-string (path)
  (with-temp-buffer
    (insert-file-contents path)
    (buffer-string)))
"##;

fn keyfreq_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(KEYFREQ_MELPA_PIN, "keyfreq.el")
        .expect("prepare pinned keyfreq source below ./tmp")
        .with_prelude(KEYFREQ_TEST_PRELUDE)
        .with_timeout(KEYFREQ_TEST_TIMEOUT)
}

fn live_command_session_counts_by_mode_and_honors_both_exclusion_policies() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (keyfreq-test-reset)
  (let ((buffer (generate-new-buffer "*keyfreq-live-session*"))
        before enabled disabled source)
    (unwind-protect
        (progn
          (set-window-buffer (selected-window) buffer)
          (set-buffer buffer)
          (keyfreq-test-ops-mode)
          (setq before
                (list keyfreq-mode
                      (and (memq #'keyfreq-pre-command-hook
                                 (default-value 'pre-command-hook))
                           t)))
          (keyfreq-mode 1)
          (setq enabled
                (list keyfreq-mode
                      (and (memq #'keyfreq-pre-command-hook
                                 (default-value 'pre-command-hook))
                           t)))
          (setq real-last-command nil)
          (execute-kbd-macro "ddrda!")

          (keyfreq-test-review-mode)
          (setq real-last-command nil)
          (execute-kbd-macro "rrd!")

          (keyfreq-test-ops-mode)
          (setq keyfreq-excluded-commands '(keyfreq-test-audit)
                keyfreq-excluded-regexp '("retry\\|flush")
                real-last-command nil)
          (execute-kbd-macro "rad!")
          (setq source (buffer-string))
          (keyfreq-mode -1)
          (setq disabled
                (list keyfreq-mode
                      (and (memq #'keyfreq-pre-command-hook
                                 (default-value 'pre-command-hook))
                           t))))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))
    (list :lifecycle (list before enabled disabled)
          :source source
          :frequencies (keyfreq-test-canonical-table keyfreq-table))))
"##;
    let expect = expect![[
        r##"OK (:lifecycle ((nil nil) (t t) (nil nil)) :source "[deploy][deploy][retry][deploy][audit][retry][retry][deploy][retry][audit][deploy]" :frequencies ((keyfreq-test-ops-mode keyfreq-test-audit 1) (keyfreq-test-ops-mode keyfreq-test-deploy 4) (keyfreq-test-ops-mode keyfreq-test-retry 1) (keyfreq-test-review-mode keyfreq-test-deploy 1) (keyfreq-test-review-mode keyfreq-test-retry 2)))"##
    ]];
    ParityBatchCase::value(
        "live_command_session_counts_by_mode_and_honors_both_exclusion_policies",
        elisp_form,
        expect,
    )
}

fn dashboard_aggregation_filtering_thresholds_and_custom_rows_preserve_totals() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (keyfreq-test-reset)
  (let* ((table
          (keyfreq-test-table
           '(keyfreq-test-ops-mode keyfreq-test-deploy 12)
           '(keyfreq-test-ops-mode keyfreq-test-retry 3)
           '(keyfreq-test-ops-mode keyfreq-test-audit 1)
           '(keyfreq-test-review-mode keyfreq-test-deploy 4)
           '(keyfreq-test-review-mode keyfreq-test-audit 6)
           '(fundamental-mode keyfreq-test-flush 2)))
         (grouped (keyfreq-groups-major-modes table))
         (ops (keyfreq-filter-major-mode table 'keyfreq-test-ops-mode)))
    (list
     :major-modes (sort (keyfreq-used-major-modes table)
                        (lambda (left right)
                          (string< (symbol-name left) (symbol-name right))))
     :grouped (keyfreq-list grouped)
     :ascending (keyfreq-list grouped t)
     :more-than-five (keyfreq-list grouped nil 5)
     :less-than-four (keyfreq-list grouped nil -4)
     :ops (keyfreq-list ops)
     :raw (keyfreq-format-list (keyfreq-list ops) 'raw)
     :custom
     (keyfreq-format-list
      (keyfreq-list ops)
      (lambda (count percentage command)
        (format "%s=%d/%.1f%%" command count percentage))))))
"##;
    let expect = expect![[
        r##"OK (:major-modes (fundamental-mode keyfreq-test-ops-mode keyfreq-test-review-mode) :grouped (28 (keyfreq-test-deploy . 16) (keyfreq-test-audit . 7) (keyfreq-test-retry . 3) (keyfreq-test-flush . 2)) :ascending (28 (keyfreq-test-flush . 2) (keyfreq-test-retry . 3) (keyfreq-test-audit . 7) (keyfreq-test-deploy . 16)) :more-than-five (28 (keyfreq-test-deploy . 16) (keyfreq-test-audit . 7)) :less-than-four (28 (keyfreq-test-retry . 3) (keyfreq-test-flush . 2)) :ops (16 (keyfreq-test-deploy . 12) (keyfreq-test-retry . 3) (keyfreq-test-audit . 1)) :raw "12 keyfreq-test-deploy\n3 keyfreq-test-retry\n1 keyfreq-test-audit\n" :custom "keyfreq-test-deploy=12/75.0%keyfreq-test-retry=3/18.8%keyfreq-test-audit=1/6.2%")"##
    ]];
    ParityBatchCase::value(
        "dashboard_aggregation_filtering_thresholds_and_custom_rows_preserve_totals",
        elisp_form,
        expect,
    )
}

fn cooperative_save_merges_existing_counts_respects_live_lock_and_round_trips() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (keyfreq-test-reset)
  (with-temp-file keyfreq-file
    (prin1 '(((keyfreq-test-ops-mode . keyfreq-test-deploy) . 5)
             ((keyfreq-test-review-mode . keyfreq-test-audit) . 2))
           (current-buffer)))
  (let ((first-delta
         (keyfreq-test-table
          '(keyfreq-test-ops-mode keyfreq-test-deploy 3)
          '(keyfreq-test-ops-mode keyfreq-test-retry 4)))
        after-first blocked saved-while-locked after-second restored)
    (keyfreq-table-save first-delta)
    (setq after-first
          (list :delta (keyfreq-test-canonical-table first-delta)
                :state (keyfreq-test-state-file)
                :lock-exists (file-exists-p keyfreq-file-lock)))

    (keyfreq-file-claim-lock)
    (let ((locked-delta
           (keyfreq-test-table
            '(keyfreq-test-review-mode keyfreq-test-audit 6)
            '(keyfreq-test-review-mode keyfreq-test-deploy 1))))
      (setq blocked
            (list :owner-is-current (= (keyfreq-file-owner) (emacs-pid))
                  :unlocked (keyfreq-file-is-unlocked)))
      (keyfreq-table-save locked-delta)
      (setq saved-while-locked
            (list :delta (keyfreq-test-canonical-table locked-delta)
                  :state (keyfreq-test-state-file)))
      (keyfreq-file-release-lock)
      (keyfreq-table-save locked-delta)
      (setq after-second
            (list :delta (keyfreq-test-canonical-table locked-delta)
                  :state (keyfreq-test-state-file)
                  :lock-exists (file-exists-p keyfreq-file-lock))))

    (setq restored
          (keyfreq-test-table
           '(keyfreq-test-ops-mode keyfreq-test-deploy 10)))
    (keyfreq-table-load restored)
    (list :first-save after-first
          :live-lock blocked
          :blocked-save saved-while-locked
          :second-save after-second
          :restored (keyfreq-test-canonical-table restored))))
"##;
    let expect = expect![[
        r##"OK (:first-save (:delta nil :state ((keyfreq-test-ops-mode keyfreq-test-deploy 8) (keyfreq-test-ops-mode keyfreq-test-retry 4) (keyfreq-test-review-mode keyfreq-test-audit 2)) :lock-exists nil) :live-lock (:owner-is-current t :unlocked nil) :blocked-save (:delta ((keyfreq-test-review-mode keyfreq-test-audit 6) (keyfreq-test-review-mode keyfreq-test-deploy 1)) :state ((keyfreq-test-ops-mode keyfreq-test-deploy 8) (keyfreq-test-ops-mode keyfreq-test-retry 4) (keyfreq-test-review-mode keyfreq-test-audit 2))) :second-save (:delta nil :state ((keyfreq-test-ops-mode keyfreq-test-deploy 8) (keyfreq-test-ops-mode keyfreq-test-retry 4) (keyfreq-test-review-mode keyfreq-test-audit 8) (keyfreq-test-review-mode keyfreq-test-deploy 1)) :lock-exists nil) :restored ((keyfreq-test-ops-mode keyfreq-test-deploy 18) (keyfreq-test-ops-mode keyfreq-test-retry 4) (keyfreq-test-review-mode keyfreq-test-audit 8) (keyfreq-test-review-mode keyfreq-test-deploy 1)))"##
    ]];
    ParityBatchCase::value(
        "cooperative_save_merges_existing_counts_respects_live_lock_and_round_trips",
        elisp_form,
        expect,
    )
}

fn interactive_reports_render_all_modes_and_a_filtered_operational_view() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (keyfreq-test-reset)
  (setq keyfreq-buffer "*keyfreq-practical-report*"
        keyfreq-table
        (keyfreq-test-table
         '(keyfreq-test-ops-mode keyfreq-test-deploy 8)
         '(keyfreq-test-ops-mode keyfreq-test-retry 2)
         '(keyfreq-test-review-mode keyfreq-test-deploy 3)))
  (let ((global-map (copy-keymap global-map))
        all-report ops-report)
    (global-set-key (kbd "C-c d") #'keyfreq-test-deploy)
    (global-set-key (kbd "C-c r") #'keyfreq-test-retry)
    (keyfreq-show)
    (setq all-report
          (with-current-buffer keyfreq-buffer
            (list :name (buffer-name)
                  :mode major-mode
                  :text (buffer-substring-no-properties (point-min) (point-max)))))
    (with-temp-buffer
      (keyfreq-test-ops-mode)
      (keyfreq-show 'keyfreq-test-ops-mode))
    (setq ops-report
          (with-current-buffer keyfreq-buffer
            (list :name (buffer-name)
                  :mode major-mode
                  :text (buffer-substring-no-properties (point-min) (point-max)))))
    (list :all all-report :ops ops-report)))
"##;
    let expect = expect![[
        r##"OK (:all (:name "*keyfreq-practical-report*" :mode fundamental-mode :text "For all major modes:\n\n     11   84.62%  keyfreq-test-deploy C-c d\n      2   15.38%  keyfreq-test-retry  C-c r\n") :ops (:name "*keyfreq-practical-report*" :mode fundamental-mode :text "For keyfreq-test-ops-mode:\n\n      8   80.00%  keyfreq-test-deploy d, C-c d\n      2   20.00%  keyfreq-test-retry  r, C-c r\n"))"##
    ]];
    ParityBatchCase::value(
        "interactive_reports_render_all_modes_and_a_filtered_operational_view",
        elisp_form,
        expect,
    )
}

fn exports_reveal_json_index_count_collision_while_html_preserves_the_full_report()
-> ParityBatchCase {
    let elisp_form = r##"
(progn
  (keyfreq-test-reset)
  (setq keyfreq-table
        (keyfreq-test-table
         '(keyfreq-test-ops-mode keyfreq-test-deploy 3)
         '(keyfreq-test-ops-mode keyfreq-test-audit 1)))
  (let ((json-path (keyfreq-test-path "keyfreq/report.json"))
        (html-path (keyfreq-test-path "keyfreq/report.html")))
    (keyfreq-json json-path nil)
    (keyfreq-html html-path nil)
    (let* ((json-text (keyfreq-test-file-string json-path))
           (json-object-type 'alist)
           (json-array-type 'list)
           (json-key-type 'symbol)
           (decoded (json-read-from-string json-text))
           (commands (alist-get 'commands decoded))
           (frequencies (alist-get 'frequencies decoded))
           (indexed (cadr frequencies))
           rows)
      (while indexed
        (let (index count)
          (setq index (pop indexed)
                count (pop indexed))
          (push (list (nth index commands) count) rows)))
      (setq rows (sort rows (lambda (left right)
                              (string< (car left) (car right)))))
      (list
       :json (list :format (alist-get 'format decoded)
                   :commands commands
                   :frequencies frequencies
                   :mode (car frequencies)
                   :rows rows)
       :html (keyfreq-test-file-string html-path)
       :files (list (file-exists-p json-path)
                    (file-exists-p html-path))))))
"##;
    let expect = expect![[
        r##"OK (:json (:format 1 :commands ("keyfreq-test-deploy" "keyfreq-test-audit") :frequencies ("keyfreq-test-ops-mode" (0 3 1)) :mode "keyfreq-test-ops-mode" :rows (("keyfreq-test-audit" nil) ("keyfreq-test-deploy" 3))) :html "<html>\n<body>\n<h1>Keyfreq Report</h1>\n<ul>\n<li><a href=\"#all\">All major modes</a></li>\n<li><a href=\"#keyfreq-test-ops-mode\">keyfreq-test-ops-mode</a></li>\n</ul>\n<h2><a name=\"all\">All major modes</a></h2>\n<table>\n<thead><tr><th>Times</th><th>Percetage</th><th>Command</th></tr></thead>\n<tbody>\n<tr><td>3</td><td>75.00%</td><td>keyfreq-test-deploy</td></tr>\n<tr><td>1</td><td>25.00%</td><td>keyfreq-test-audit</td></tr>\n</tbody>\n</table>\n<h2><a name=\"keyfreq-test-ops-mode\">keyfreq-test-ops-mode</a></h2>\n<table>\n<thead><tr><th>Times</th><th>Percetage</th><th>Command</th></tr></thead>\n<tbody>\n<tr><td>3</td><td>75.00%</td><td>keyfreq-test-deploy</td></tr>\n<tr><td>1</td><td>25.00%</td><td>keyfreq-test-audit</td></tr>\n</tbody>\n</table>\n</body>\n</html>\n" :files (t t))"##
    ]];
    ParityBatchCase::value(
        "exports_reveal_json_index_count_collision_while_html_preserves_the_full_report",
        elisp_form,
        expect,
    )
}

fn autosave_mode_schedules_periodic_flushes_and_persists_the_final_delta_on_disable()
-> ParityBatchCase {
    let elisp_form = r##"
(progn
  (keyfreq-test-reset)
  (let ((keyfreq-autosave-timeout 17)
        enabled first-flush disabled)
    (keyfreq-autosave-mode 1)
    (setq enabled
          (list :mode keyfreq-autosave-mode
                :timer (timerp keyfreq-autosave--timer)
                :repeat (timer--repeat-delay keyfreq-autosave--timer)
                :function (timer--function keyfreq-autosave--timer)
                :kill-hook (and (memq #'keyfreq-mustsave--do kill-emacs-hook) t)))
    (puthash '(keyfreq-test-ops-mode . keyfreq-test-deploy) 7 keyfreq-table)
    (keyfreq-autosave--do)
    (setq first-flush
          (list :memory (keyfreq-test-canonical-table keyfreq-table)
                :disk (keyfreq-test-state-file)))
    (puthash '(keyfreq-test-ops-mode . keyfreq-test-deploy) 2 keyfreq-table)
    (puthash '(keyfreq-test-ops-mode . keyfreq-test-audit) 1 keyfreq-table)
    (keyfreq-autosave-mode -1)
    (setq disabled
          (list :mode keyfreq-autosave-mode
                :timer keyfreq-autosave--timer
                :kill-hook (and (memq #'keyfreq-mustsave--do kill-emacs-hook) t)
                :memory (keyfreq-test-canonical-table keyfreq-table)
                :disk (keyfreq-test-state-file)))
    (list :enabled enabled :first-flush first-flush :disabled disabled)))
"##;
    let expect = expect![[
        r##"OK (:enabled (:mode t :timer t :repeat 17 :function keyfreq-autosave--do :kill-hook t) :first-flush (:memory nil :disk ((keyfreq-test-ops-mode keyfreq-test-deploy 7))) :disabled (:mode nil :timer nil :kill-hook nil :memory nil :disk ((keyfreq-test-ops-mode keyfreq-test-audit 1) (keyfreq-test-ops-mode keyfreq-test-deploy 9))))"##
    ]];
    ParityBatchCase::value(
        "autosave_mode_schedules_periodic_flushes_and_persists_the_final_delta_on_disable",
        elisp_form,
        expect,
    )
}

#[test]
fn keyfreq_package_batch() {
    let cases = vec![
        live_command_session_counts_by_mode_and_honors_both_exclusion_policies(),
        dashboard_aggregation_filtering_thresholds_and_custom_rows_preserve_totals(),
        cooperative_save_merges_existing_counts_respects_live_lock_and_round_trips(),
        interactive_reports_render_all_modes_and_a_filtered_operational_view(),
        exports_reveal_json_index_count_collision_while_html_preserves_the_full_report(),
        autosave_mode_schedules_periodic_flushes_and_persists_the_final_delta_on_disable(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed keyfreq parity test");
    assert_oracle_batch_cases(keyfreq_oracle(), test_name, "keyfreq_parity", &cases);
}
