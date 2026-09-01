use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_timer_insert_items_pause_continue_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-timer)
  (with-temp-buffer
    (let ((org-timer-format "[%s] ")
          (org-timer-display nil)
          (events nil))
      (add-hook 'org-timer-start-hook
                (lambda () (push 'start events)) nil t)
      (add-hook 'org-timer-pause-hook
                (lambda () (push 'pause events)) nil t)
      (add-hook 'org-timer-continue-hook
                (lambda () (push 'continue events)) nil t)
      (add-hook 'org-timer-stop-hook
                (lambda () (push 'stop events)) nil t)
      (org-mode)
      (cl-letf (((symbol-function 'current-time)
                 (lambda () (seconds-to-time 1000))))
        (org-timer-start "0:01:05"))
      (cl-letf (((symbol-function 'current-time)
                 (lambda () (seconds-to-time 1070))))
        (org-timer-item nil)
        (org-timer-pause-or-continue nil))
      (let ((paused (org-timer-value-string)))
        (cl-letf (((symbol-function 'current-time)
                   (lambda () (seconds-to-time 1120))))
          (org-timer-pause-or-continue nil))
        (cl-letf (((symbol-function 'current-time)
                   (lambda () (seconds-to-time 1130))))
          (goto-char (point-max))
          (org-timer-item nil)
          (org-timer-stop))
        (list paused
              (nreverse events)
              org-timer-start-time
              org-timer-pause-time
              org-timer-countdown-timer
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_timer_region_shift_negative_default_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-timer)
  (with-temp-buffer
    (insert "Intro 0:00:10\n")
    (insert "- 0:01:00:: item\n")
    (insert "- -0:00:05:: negative\n")
    (insert "Outro 1:02:03\n")
    (let ((before (buffer-substring-no-properties
                   (point-min) (point-max))))
      (org-timer-change-times-in-region (point-min) (point-max) "-0:00:10")
      (let ((after-explicit
             (buffer-substring-no-properties (point-min) (point-max))))
        (org-timer-change-times-in-region (point-min) (point-max) "")
        (list before
              after-explicit
              (buffer-substring-no-properties
               (point-min) (point-max))
              (mapcar #'org-timer-hms-to-secs
                      '("-0:00:15" "0:00:00" "1:01:43"))
              (mapcar #'org-timer-secs-to-hms
                      '(-15 0 3703))))))"##,
        expect,
    );
}

#[test]
fn org_timer_countdown_effort_title_mode_line_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-timer)
  (with-temp-buffer
    (let ((org-timer-display 'mode-line)
          (global-mode-string nil)
          (frame-title-format nil)
          (org-timer-default-timer "0")
          (org-effort-property "Effort"))
      (org-mode)
      (insert "* TODO Timed task\n:PROPERTIES:\n:Effort: 0:02\n:END:\n")
      (goto-char (point-min))
      (let ((title (org-timer--get-timer-title)))
        (cl-letf (((symbol-function 'current-time)
                   (lambda () (seconds-to-time 2000))))
          (org-timer-set-timer '(4)))
        (let ((after-set
               (list (timerp org-timer-countdown-timer)
                     org-timer-countdown-timer-title
                     (org-timer-value-string)
                     global-mode-string
                     org-timer-mode-line-string)))
          (org-timer-pause-or-continue nil)
          (let ((after-pause
                 (list org-timer-countdown-timer
                       (not (null org-timer-pause-time))
                       org-timer-mode-line-timer)))
            (org-timer-stop)
            (list title
                  after-set
                  after-pause
                  org-timer-start-time
                  org-timer-countdown-timer
                  global-mode-string))))))"##,
        expect,
    );
}

#[test]
fn org_timer_restart_offset_parse_item_error_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-timer)
  (with-temp-buffer
    (let ((org-timer-format "<%s>")
          (org-timer-display nil)
          (events nil))
      (add-hook 'org-timer-start-hook
                (lambda () (push (list 'start org-timer-start-time)
                                 events))
                nil t)
      (add-hook 'org-timer-stop-hook
                (lambda () (push 'stop events)) nil t)
      (org-mode)
      (insert "* Timer\n")
      (insert "Existing stamp 0:02:03 here.\n")
      (insert "- plain item\n")
      (goto-char (point-min))
      (search-forward "0:02:03")
      (cl-letf (((symbol-function 'current-time)
                 (lambda () (seconds-to-time 5000)))
                ((symbol-function 'read-string)
                 (lambda (&rest _) "")))
        (org-timer-start '(4)))
      (let ((after-start (list org-timer-start-time
                               org-timer-pause-time
                               (org-timer-value-string))))
        (cl-letf (((symbol-function 'current-time)
                   (lambda () (seconds-to-time 5010))))
          (let ((no-insert (org-timer nil t)))
            (goto-char (point-max))
            (insert "\nInserted: ")
            (org-timer nil nil)
            (let ((after-insert
                   (buffer-substring-no-properties
                    (point-min) (point-max)))
                  (plain-item-error
                   (progn
                     (goto-char (point-min))
                     (search-forward "- plain item")
                     (condition-case err
                         (progn (org-timer-item nil) 'no-error)
                       (error (cons (car err) (cdr err)))))))
              (org-timer-stop)
              (list after-start
                    no-insert
                    after-insert
                    plain-item-error
                    (mapcar (lambda (s)
                              (condition-case err
                                  (list s
                                        (org-timer-fix-incomplete s)
                                        (org-timer-hms-to-secs
                                         (org-timer-fix-incomplete s)))
                                (error (list s (cons (car err) (cdr err))))))
                            '("7" "2:03" "1:02:03" "bad"))
                    (mapcar #'org-timer-secs-to-hms
                            '(-3723 -1 0 61 3661))
                    (mapcar (lambda (event)
                              (if (consp event) (car event) event))
                            (nreverse events))
                    org-timer-start-time
                    org-timer-pause-time)))))))"##,
        expect,
    );
}

#[test]
fn org_timer_list_region_countdown_element_lifecycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-timer)
  (require 'org-element)
  (with-temp-buffer
    (let ((org-timer-format "[%s] ")
          (org-timer-display nil)
          (org-timer-default-timer "0:03")
          (events nil)
          (global-mode-string nil))
      (add-hook 'org-timer-start-hook
                (lambda () (push (list 'start org-timer-start-time)
                                 events))
                nil t)
      (add-hook 'org-timer-set-hook
                (lambda () (push (list 'set
                                       org-timer-countdown-timer-title)
                                 events))
                nil t)
      (add-hook 'org-timer-pause-hook
                (lambda () (push (list 'pause org-timer-pause-time)
                                 events))
                nil t)
      (add-hook 'org-timer-continue-hook
                (lambda () (push (list 'continue org-timer-start-time)
                                 events))
                nil t)
      (add-hook 'org-timer-stop-hook
                (lambda () (push 'stop events))
                nil t)
      (org-mode)
      (insert "* Meeting\n")
      (insert ":PROPERTIES:\n:Effort: 0:03\n:END:\n")
      (insert "- kickoff\n")
      (insert "- 0:00:30:: existing timer item\n")
      (insert "Notes at 0:01:05 and 0:02:10.\n")
      (let ((snapshot
             (lambda (label)
               (list label
                     org-timer-start-time
                     org-timer-pause-time
                     (timerp org-timer-countdown-timer)
                     org-timer-countdown-timer-title
                     org-timer-mode-line-string
                     (org-timer-value-string)
                     (org-element-map (org-element-parse-buffer)
                         '(headline item plain-list node-property)
                       (lambda (el)
                         (list (org-element-type el)
                               (org-element-property :raw-value el)
                               (org-element-property :checkbox el)
                               (org-element-property :tag el)
                               (org-element-property :begin el)
                               (org-element-property :end el))))
                     (buffer-substring-no-properties
                      (point-min) (point-max))))))
        (let (states no-insert inserted-value)
          (push (funcall snapshot 'initial) states)
          (cl-letf (((symbol-function 'current-time)
                     (lambda () (seconds-to-time 1000))))
            (org-timer-start "0:00:05"))
          (push (funcall snapshot 'started) states)
          (cl-letf (((symbol-function 'current-time)
                     (lambda () (seconds-to-time 1035))))
            (goto-char (point-min))
            (search-forward "kickoff")
            (beginning-of-line)
            (org-timer-item nil)
            (setq no-insert (org-timer nil t)))
          (push (funcall snapshot 'after-item) states)
          (cl-letf (((symbol-function 'current-time)
                     (lambda () (seconds-to-time 1040))))
            (goto-char (point-max))
            (insert "Inserted timer: ")
            (org-timer nil nil)
            (setq inserted-value
                  (buffer-substring-no-properties
                   (line-beginning-position)
                   (line-end-position))))
          (push (funcall snapshot 'after-inline-insert) states)
          (org-timer-change-times-in-region (point-min) (point-max)
                                            "0:00:10")
          (push (funcall snapshot 'after-region-shift) states)
          (cl-letf (((symbol-function 'current-time)
                     (lambda () (seconds-to-time 1050))))
            (org-timer-pause-or-continue nil))
          (push (funcall snapshot 'paused) states)
          (cl-letf (((symbol-function 'current-time)
                     (lambda () (seconds-to-time 1070))))
            (org-timer-pause-or-continue nil))
          (push (funcall snapshot 'continued) states)
          (cl-letf (((symbol-function 'current-time)
                     (lambda () (seconds-to-time 1080))))
            (org-timer-set-timer nil))
          (push (funcall snapshot 'countdown-set) states)
          (org-timer-stop)
          (push (funcall snapshot 'stopped) states)
          (list (nreverse states)
                no-insert
                inserted-value
                (mapcar (lambda (s)
                          (condition-case err
                              (list s
                                    (org-timer-fix-incomplete s)
                                    (org-timer-hms-to-secs
                                     (org-timer-fix-incomplete s)))
                            (error (list s (cons (car err) (cdr err))))))
                        '("3" "4:05" "2:03:04" "-0:00:15"))
                (mapcar #'org-timer-secs-to-hms
                        '(-65 -1 0 125 7322))
                (nreverse events)
                org-timer-start-time
                org-timer-pause-time
                org-timer-countdown-timer
                global-mode-string
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_timer_countdown_replace_done_display_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-timer)
  (with-temp-buffer
    (let ((org-timer-display 'both)
          (org-timer-default-timer "0:04")
          (org-clock-sound "ding")
          (global-mode-string nil)
          (frame-title-format nil)
          (events nil)
          (fake-timers nil)
          (cancelled nil)
          (notify nil)
          (answers nil))
      (add-hook 'org-timer-set-hook
                (lambda () (push (list 'set
                                       org-timer-countdown-timer-title
                                       org-timer-start-time)
                                 events))
                nil t)
      (add-hook 'org-timer-pause-hook
                (lambda () (push (list 'pause org-timer-pause-time)
                                 events))
                nil t)
      (add-hook 'org-timer-continue-hook
                (lambda () (push (list 'continue org-timer-start-time)
                                 events))
                nil t)
      (add-hook 'org-timer-stop-hook
                (lambda () (push 'stop events))
                nil t)
      (add-hook 'org-timer-done-hook
                (lambda () (push 'done events))
                nil t)
      (org-mode)
      (insert "* TODO Countdown target\n")
      (insert ":PROPERTIES:\n:Effort: 0:06\n:END:\nBody\n")
      (goto-char (point-min))
      (cl-letf (((symbol-function 'run-with-timer)
                 (lambda (secs repeat function &rest args)
                   (let ((timer (list :fake-timer secs repeat function args)))
                     (push timer fake-timers)
                     timer)))
                ((symbol-function 'timerp)
                 (lambda (object)
                   (and (consp object) (eq (car object) :fake-timer))))
                ((symbol-function 'cancel-timer)
                 (lambda (timer) (push timer cancelled) nil))
                ((symbol-function 'timer--time)
                 (lambda (timer) (seconds-to-time (plist-get (cdr timer) :fake-timer))))
                ((symbol-function 'org-notify)
                 (lambda (message sound) (push (list message sound) notify)))
                ((symbol-function 'y-or-n-p)
                 (lambda (&rest _) (pop answers)))
                ((symbol-function 'force-mode-line-update)
                 (lambda (&rest _) (push 'force-mode-line events)))
                ((symbol-function 'current-time)
                 (lambda () (seconds-to-time 1000))))
        (let (states)
          (let ((snapshot
                 (lambda (label)
                   (list label
                         (timerp org-timer-countdown-timer)
                         org-timer-countdown-timer
                         org-timer-countdown-timer-title
                         org-timer-start-time
                         org-timer-pause-time
                         org-timer-mode-line-string
                         org-timer-mode-line-timer
                         global-mode-string
                         frame-title-format
                         (org-timer-value-string)
                         (nreverse (copy-sequence events))
                         (nreverse (copy-sequence cancelled))
                         (nreverse (copy-sequence notify))
                         (nreverse (copy-sequence fake-timers))))))
            (org-timer-set-timer nil)
            (push (funcall snapshot 'effort-set) states)
            (setq answers '(nil))
            (org-timer-set-timer "0:02")
            (push (funcall snapshot 'replace-declined) states)
            (setq answers '(t))
            (org-timer-set-timer "0:02")
            (push (funcall snapshot 'replace-accepted) states)
            (org-timer-pause-or-continue nil)
            (push (funcall snapshot 'paused) states)
            (cl-letf (((symbol-function 'current-time)
                       (lambda () (seconds-to-time 1015))))
              (org-timer-pause-or-continue nil))
            (push (funcall snapshot 'continued) states)
            (org-timer-set-timer '(16))
            (push (funcall snapshot 'forced-replace) states)
            (let* ((timer org-timer-countdown-timer)
                   (callback (nth 3 timer)))
              (funcall callback))
            (push (funcall snapshot 'after-callback) states)
            (condition-case err
                (org-timer-stop)
              (error (push (cons (car err) (cdr err)) events)))
            (list (nreverse states)
                  (nreverse events)
                  (nreverse cancelled)
                  (nreverse notify)
                  org-timer-start-time
                  org-timer-pause-time
                  org-timer-countdown-timer
                  global-mode-string
                  frame-title-format
                   (buffer-substring-no-properties
                    (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_timer_hms_secs_parse_region_modify_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((300 5445 10 7200) (\"0:05:00\" \"1:30:45\" \"0:00:10\" \"2:00:00\") (\"0:00:05\" \"0:01:02\" \"1:02:03\" \"0:00:30\") \"00:05:00\\n01:30:45\\n00:00:10\\n02:00:00\\n\" \"00:06:00\\n01:31:45\\n00:01:10\\n02:01:00\\n\" \"00:05:00\\n00:15:00\\n00:25:00\\n\" error \"* Task\\n- item one\\n- item two\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-timer)
  (with-temp-buffer
    (insert "00:05:00\n01:30:45\n00:00:10\n02:00:00\n")
    ;; Parse hms values
    (let ((parsed (mapcar #'org-timer-hms-to-secs
                          '("0:05:00" "1:30:45" "0:00:10" "2:00:00")))
          ;; Format secs to hms
          (formatted (mapcar #'org-timer-secs-to-hms '(300 5445 10 7200)))
          ;; Fix incomplete
          (fixed (mapcar #'org-timer-fix-incomplete
                         '("5" "1:02" "1:02:03" "0:30"))))
      ;; Modify region
      (let ((before-region (buffer-substring-no-properties
                            (point-min) (point-max))))
        (org-timer-change-times-in-region
         (point-min) (point-max) "0:01:00")
        (let ((after-add (buffer-substring-no-properties
                          (point-min) (point-max))))
          ;; Reset and subtract
          (erase-buffer)
          (insert "00:10:00\n00:20:00\n00:30:00\n")
          (org-timer-change-times-in-region
           (point-min) (point-max) "-0:05:00")
          (let ((after-subtract (buffer-substring-no-properties
                                 (point-min) (point-max))))
            ;; Item timer
            (erase-buffer)
            (org-mode)
            (insert "* Task\n- item one\n- item two\n")
            (let ((item-timer
                   (condition-case nil
                       (progn (org-timer-item) 'ok)
                     (error 'error))))
              (list parsed
                    formatted
                    fixed
                    before-region
                    after-add
                    after-subtract
                    item-timer
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}
