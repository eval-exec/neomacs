use expect_test::expect;

use super::ParityBatchCase;

fn custom_monitor_tracks_history_formats_annotations_and_cleans_up() -> ParityBatchCase {
    ParityBatchCase::value(
        "custom_monitor_tracks_history_formats_annotations_and_cleans_up",
        r##"
(let ((symon-history-size 4)
      (symon-refresh-rate 99)
      (neomacs-symon-fetch-values '(7 12 19))
      setup cleanup)
  (define-symon-monitor neomacs-symon-monitor
    :index "Q:" :unit " jobs"
    :setup (setq setup (1+ (or setup 0)))
    :cleanup (setq cleanup (1+ (or cleanup 0)))
    :fetch (pop neomacs-symon-fetch-values)
    :annotation "queued")
  (let* ((monitor (get 'neomacs-symon-monitor 'symon-monitor))
         (setup-fn (aref monitor 0))
         (cleanup-fn (aref monitor 1))
         (display-fn (aref monitor 2))
         first second third)
    (unwind-protect
        (progn
          (funcall setup-fn)
          (setq first (funcall display-fn))
          (ring-insert (aref neomacs-symon-monitor--cell 0)
                       (pop neomacs-symon-fetch-values))
          (setq second (funcall display-fn))
          (ring-insert (aref neomacs-symon-monitor--cell 0)
                       (pop neomacs-symon-fetch-values))
          (setq third (funcall display-fn))
          (funcall cleanup-fn)
          (list :setup setup
                :cleanup cleanup
                :history
                (ring-elements (aref neomacs-symon-monitor--cell 0))
                :displays (list first second third)))
      (let ((timer (aref neomacs-symon-monitor--cell 1)))
        (when (timerp timer) (cancel-timer timer))))))
"##,
        expect![[
            r#"OK (:setup 1 :cleanup 1 :history (19 12 7 nil) :displays ("Q:7 jobs (queued) " "Q:12 jobs (queued) " "Q:19 jobs (queued) "))"#
        ]],
    )
}

fn sparkline_types_generate_exact_bitmap_patterns_and_cache_independent_copies() -> ParityBatchCase
{
    ParityBatchCase::value(
        "sparkline_types_generate_exact_bitmap_patterns_and_cache_independent_copies",
        r##"
(let ((symon-sparkline-width 8)
      (symon-sparkline-height 5)
      (symon-sparkline-thickness 1)
      (symon-sparkline-ascent 90)
      (symon--sparkline-base-cache (vector nil -1 -1 nil)))
  (let (types)
    (dolist (type '(plain bounded boxed gridded))
      (let* ((symon-sparkline-type type)
             (base (symon--get-sparkline-base))
             (copy (symon--get-sparkline-base)))
        (aset base 0 (not (aref base 0)))
        (push (list type
                    :set (neomacs-symon-test-bool-indices copy)
                    :independent (not (eq base copy)))
              types)))
    (let* ((symon-sparkline-type 'plain)
           (spark (symon--make-sparkline '(0 25 50 75 100) 0 100)))
      (list :types (nreverse types)
            :spark
            (list :type (plist-get (cdr spark) :type)
                  :ascent (plist-get (cdr spark) :ascent)
                  :height (plist-get (cdr spark) :height)
                  :width (plist-get (cdr spark) :width)
                  :set
                  (neomacs-symon-test-bool-indices
                   (plist-get (cdr spark) :data)))))))
"##,
        expect![
            "OK (:types ((plain :set nil :independent t) (bounded :set (0 2 4 6 32 34 36 38) :independent t) (boxed :set (0 2 4 6 7 16 23 32 34 36 38) :independent t) (gridded :set (0 2 4 6 7 16 18 20 22 23 32 34 36 38) :independent t)) :spark (:type xbm :ascent 90 :height 5 :width 8 :set (7 13 14 20 26 27 32 33)))"
        ],
    )
}

fn xpm_conversion_preserves_geometry_pixels_transparency_and_ascent() -> ParityBatchCase {
    ParityBatchCase::value(
        "xpm_conversion_preserves_geometry_pixels_transparency_and_ascent",
        r##"
(let ((symon-sparkline-width 4)
      (symon-sparkline-height 3)
      (symon-sparkline-ascent 80))
  (let* ((bitmap (make-bool-vector 12 nil))
         (_ (dolist (index '(0 3 5 10)) (aset bitmap index t)))
         (xpm (cl-letf (((symbol-function 'face-foreground)
                         (lambda (&rest _) "#abcdef")))
                (symon--convert-sparkline-to-xpm
                 `(image :type xbm :data ,bitmap :ascent 80
                         :height 3 :width 4))))
         (data (plist-get (cdr xpm) :data)))
    (list :type (plist-get (cdr xpm) :type)
          :ascent (plist-get (cdr xpm) :ascent)
          :height (plist-get (cdr xpm) :height)
          :width (plist-get (cdr xpm) :width)
          :header (and (string-match-p "\"4 3 2 1\"" data) t)
          :color (and (string-match-p "\"@ c #abcdef\"" data) t)
          :transparent (and (string-match-p "\". c none\"" data) t)
          :rows
          (let ((start 0) rows)
            (while (string-match "\"\\([@.]+\\)\"" data start)
              (push (match-string 1 data) rows)
              (setq start (match-end 0)))
            (last (nreverse rows) 3)))))
"##,
        expect![[
            r#"OK (:type xpm :ascent 80 :height 3 :width 4 :header t :color t :transparent t :rows ("@..@" ".@.." "..@."))"#
        ]],
    )
}

fn multipage_display_updates_selected_page_and_still_samples_background_pages() -> ParityBatchCase {
    ParityBatchCase::value(
        "multipage_display_updates_selected_page_and_still_samples_background_pages",
        r##"
(let ((symon--display-fns
       (list
        (list (lambda () (push 'page-a neomacs-symon-calls) "A:1 "))
        (list (lambda () (push 'page-b neomacs-symon-calls) "B:2 "))))
      (symon--active-page 0)
      (symon--total-page-num 2)
      (symon--display-active nil)
      (neomacs-symon-calls nil)
      messages)
  (cl-letf (((symbol-function 'message)
             (lambda (format-string &rest arguments)
               (push (apply #'format format-string arguments) messages))))
    (symon--display-update)
    (let ((first (list :page symon--active-page
                       :active symon--display-active
                       :calls (nreverse neomacs-symon-calls)
                       :messages (nreverse messages))))
      (setq neomacs-symon-calls nil messages nil)
      (symon--redisplay)
      (let ((second (list :page symon--active-page
                          :active symon--display-active
                          :calls (nreverse neomacs-symon-calls)
                          :messages (nreverse messages))))
        (symon--display-end)
        (list :first first :second second
              :ended symon--display-active)))))
"##,
        expect![[
            r#"OK (:first (:page 0 :active t :calls (page-a page-b) :messages ("A:1 ")) :second (:page 1 :active t :calls (page-a page-b) :messages ("B:2 ")) :ended nil)"#
        ]],
    )
}

fn mode_lifecycle_sets_pages_timers_hooks_and_runs_monitor_cleanup() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_lifecycle_sets_pages_timers_hooks_and_runs_monitor_cleanup",
        r##"
(let ((symon-refresh-rate 99)
      (symon-delay 99)
      (symon-history-size 3)
      (symon-monitors '((neomacs-symon-life-a)
                        (neomacs-symon-life-b)))
      (neomacs-symon-life-events nil))
  (define-symon-monitor neomacs-symon-life-a
    :setup (push 'setup-a neomacs-symon-life-events)
    :cleanup (push 'cleanup-a neomacs-symon-life-events)
    :display "A")
  (define-symon-monitor neomacs-symon-life-b
    :setup (push 'setup-b neomacs-symon-life-events)
    :cleanup (push 'cleanup-b neomacs-symon-life-events)
    :display "B")
  (unwind-protect
      (progn
        (symon-mode 1)
        (let ((enabled
               (list :mode symon-mode
                     :events (reverse neomacs-symon-life-events)
                     :pages symon--total-page-num
                     :display-fns (mapcar #'length symon--display-fns)
                     :timers (length symon--timer-objects)
                     :pre-hook (and (memq #'symon--display-end
                                          pre-command-hook) t)
                     :kill-hook (and (memq #'symon--cleanup
                                           kill-emacs-hook) t))))
          (symon-mode -1)
          (list :enabled enabled
                :disabled
                (list :mode symon-mode
                      :events (reverse neomacs-symon-life-events)
                      :pre-hook (and (memq #'symon--display-end
                                           pre-command-hook) t)
                      :kill-hook (and (memq #'symon--cleanup
                                            kill-emacs-hook) t)))))
    (when symon-mode (symon-mode -1))
    (neomacs-symon-test-cancel-timers)))
"##,
        expect!["OK (:enabled (:mode t :events (setup-a setup-b) :pages 2 :display-fns (1 1) :timers 2 :pre-hook nil :kill-hook t) :disabled (:mode nil :events (setup-a setup-b cleanup-a cleanup-b) :pre-hook nil :kill-hook nil))"],
    )
    .fresh_process()
}

fn empty_monitor_configuration_warns_and_installs_an_empty_page_set() -> ParityBatchCase {
    ParityBatchCase::value(
        "empty_monitor_configuration_warns_and_installs_an_empty_page_set",
        r##"
(let ((symon-refresh-rate 99)
      (symon-delay 99)
      (symon-monitors nil)
      (symon--cleanup-fns nil)
      (symon--display-fns nil)
      (symon--total-page-num nil)
      (symon--timer-objects nil)
      messages)
  (unwind-protect
      (cl-letf (((symbol-function 'message)
                 (lambda (format-string &rest arguments)
                   (push (apply #'format format-string arguments)
                         messages))))
        (let ((outcome
               (condition-case err
                   (list :value (symon--initialize))
                 (error
                  (list :signal (car err)
                        :data (cdr err)
                        :message (error-message-string err))))))
          (list :messages (nreverse messages)
                :outcome outcome
                :pages symon--total-page-num
                :display-fns symon--display-fns
                :timers (length symon--timer-objects))))
    (symon--cleanup)
    (neomacs-symon-test-cancel-timers)))
"##,
        expect![[r#"OK (:messages ("Warning: `symon-monitors' is empty.") :outcome (:signal wrong-type-argument :data (arrayp nil) :message "Wrong type argument: arrayp, nil") :pages nil :display-fns nil :timers 0)"#]],
    )
    .fresh_process()
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        custom_monitor_tracks_history_formats_annotations_and_cleans_up(),
        sparkline_types_generate_exact_bitmap_patterns_and_cache_independent_copies(),
        xpm_conversion_preserves_geometry_pixels_transparency_and_ascent(),
        multipage_display_updates_selected_page_and_still_samples_background_pages(),
        mode_lifecycle_sets_pages_timers_hooks_and_runs_monitor_cleanup(),
        empty_monitor_configuration_warns_and_installs_an_empty_page_set(),
    ]
}
