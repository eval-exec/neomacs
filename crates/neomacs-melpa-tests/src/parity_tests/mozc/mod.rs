use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, MOZC_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const MOZC_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const MOZC_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'mozc)

(defun mozc-test-string-state (text)
  (and text
       (list
        :text (substring-no-properties text)
        :runs
        (let ((position 0)
              result)
          (while (< position (length text))
            (let ((next (next-property-change position text (length text))))
            (push
             (list :range (list position next)
                   :text (substring-no-properties text position next)
                   :face (get-text-property position 'face text)
                   :cursor (get-text-property position 'cursor text))
             result)
              (setq position next)))
          (nreverse result)))))

(defun mozc-test-region-state (region)
  (and region
       (list :range (list (marker-position (car region))
                          (marker-position (cdr region)))
             :live (and (marker-buffer (car region))
                        (marker-buffer (cdr region))
                        t))))

(defun mozc-test-overlay-state (overlay)
  (and overlay
       (list :range (list (overlay-start overlay) (overlay-end overlay))
             :buffer-live (and (overlay-buffer overlay) t)
             :display (mozc-test-string-state
                       (overlay-get overlay 'display)))))
"##;

fn mozc_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MOZC_MELPA_PIN, "mozc.el")
        .expect("prepare pinned Mozc source below ./tmp")
        .with_prelude(MOZC_TEST_PRELUDE)
        .with_timeout(MOZC_TEST_TIMEOUT)
}

fn input_mode_lifecycle_prioritizes_and_temporarily_disables_its_catch_all_keymap()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((minor-mode-map-alist (copy-tree minor-mode-map-alist))
        (mozc-mode-hook nil)
        events enabled disabled)
    (add-hook 'mozc-mode-hook
              (lambda () (push (list :hook (current-buffer)) events)))
    (cl-letf (((symbol-function 'mozc-session-create)
               (lambda (force)
                 (push (list :create force (current-buffer)) events)
                 1701))
              ((symbol-function 'mozc-clean-up-session)
               (lambda ()
                 (push (list :clean-up (current-buffer)) events))))
      (setq enabled (mozc-mode 1))
      (let ((entry (assq 'mozc-mode minor-mode-map-alist)))
        (setq enabled
              (list :return enabled
                    :variable mozc-mode
                    :entry-first (eq entry (car minor-mode-map-alist))
                    :entry-count (cl-count 'mozc-mode minor-mode-map-alist
                                           :key #'car)
                    :map-active (eq (cdr entry) mozc-mode-map)
                    :catch-all (lookup-key (cdr entry) [t])
                    :delete-frame (lookup-key (cdr entry) [delete-frame]))))
      (mozc-disable-keymap)
      (let ((entry (assq 'mozc-mode minor-mode-map-alist)))
        (setq disabled
              (list :empty (eq (cdr entry) mozc-empty-map)
                    :catch-all (lookup-key (cdr entry) [t]))))
      (mozc-enable-keymap)
      (let ((reenabled
             (eq (cdr (assq 'mozc-mode minor-mode-map-alist)) mozc-mode-map))
            (disable-return (mozc-mode -1)))
        (list :enabled enabled
              :temporarily-disabled disabled
              :reenabled reenabled
              :disabled-return disable-return
              :mode-after mozc-mode
              :mode-line-entry (assq 'mozc-mode minor-mode-alist)
              :events
              (mapcar
               (lambda (event)
                 (if (bufferp (car (last event)))
                     (append (butlast event) (list :current-buffer))
                   event))
               (nreverse events)))))))
"##;
    let expect = expect![[
        r####"OK (:enabled (:return t :variable t :entry-first t :entry-count 1 :map-active t :catch-all mozc-handle-event :delete-frame nil) :temporarily-disabled (:empty t :catch-all nil) :reenabled t :disabled-return nil :mode-after nil :mode-line-entry (mozc-mode mozc-mode-string) :events ((:hook :current-buffer) (:create t :current-buffer) (:clean-up :current-buffer)))"####
    ]];
    ParityBatchCase::value(
        "input_mode_lifecycle_prioritizes_and_temporarily_disables_its_catch_all_keymap",
        elisp_form,
        expect,
    )
}

fn key_translation_and_custom_kana_maps_build_the_helper_payload_used_for_real_typing()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((kana (aref (kbd "<hiragana-katakana>") 0))
       (shift-kana (aref (kbd "S-<hiragana-katakana>") 0))
       (events (list ?a ?A ?\s 'backspace 'next 'prior 'kp-5))
       (keymap (mozc-keymap-make-keymap-from-flat-list
                (list ?a "ち" ?A "ち-small" ?1 "ぬ")))
       (mozc-keymap-kana keymap)
       (mozc-config-protobuf '((preedit-method . kana)))
       sent)
  ;; Command-loop events have already had their symbolic modifiers parsed.
  (mapc #'event-modifiers (list kana shift-kana))
  (let ((converted
         (mapcar
          (lambda (event)
            (list event (mozc-key-event-to-key-and-modifiers event)))
          events)))
    (cl-letf (((symbol-function 'mozc-session-sendkey)
               (lambda (payload)
                 (push (copy-tree payload) sent)
                 (list (cons 'echo (copy-tree payload))))))
      (let ((responses
             (list (mozc-send-key-event ?a)
                   (mozc-send-key-event ?x)
                   (mozc-send-key-event ?A)
                   (mozc-send-key-event kana)
                   (mozc-send-key-event shift-kana))))
        (let ((before
               (mapcar (lambda (key) (list key (mozc-keymap-get-entry keymap key "missing")))
                       (list ?a ?A ?x ?1))))
          (list
           :converted converted
           :responses responses
           :sent (nreverse sent)
           :before before
           :put (mozc-keymap-put-entry keymap ?x "さ")
           :after-put (mozc-keymap-get-entry keymap ?x)
           :remove (mozc-keymap-remove-entry keymap ?a)
           :after-remove (mozc-keymap-get-entry keymap ?a "fallback")
           :invalid
           (list (mozc-keymap-put-entry keymap 'not-a-code "bad")
                 (mozc-keymap-put-entry keymap ?z 99)
                 (mozc-keymap-get-entry 'not-a-table ?a "fallback"))
           :active (eq (mozc-keymap-current-active-keymap) keymap)))))))
"##;
    let expect = expect![[
        r####"OK (:converted ((97 (97)) (65 (65)) (32 (space)) (backspace (backspace)) (next (pagedown)) (prior (pageup)) (kp-5 (numpad5))) :responses (((echo 97 "ち")) ((echo 120)) ((echo 65 "ち-small")) ((echo kana)) ((echo katakana))) :sent ((97 "ち") (120) (65 "ち-small") (kana) (katakana)) :before ((97 "ち") (65 "ち-small") (120 "missing") (49 "ぬ")) :put (120 . "さ") :after-put "さ" :remove nil :after-remove "fallback" :invalid (nil nil nil) :active t)"####
    ]];
    ParityBatchCase::value(
        "key_translation_and_custom_kana_maps_build_the_helper_payload_used_for_real_typing",
        elisp_form,
        expect,
    )
}

fn temporary_display_placeholders_preserve_document_undo_and_modified_state() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "alpha omega")
  (setq buffer-undo-list nil)
  (restore-buffer-modified-p nil)
  (goto-char 7)
  (let (primary extras states)
    (mozc-buffer-placeholder-setq primary "<" ?* ">")
    (push (list :operation 'primary
                :buffer (buffer-string)
                :point (point)
                :region (mozc-test-region-state primary)
                :modified (buffer-modified-p)
                :undo buffer-undo-list)
          states)
    (goto-char (point-max))
    (mozc-buffer-placeholder-push extras "候補")
    (mozc-buffer-placeholder-push-char extras ?# 2)
    (push (list :operation 'stacked
                :buffer (buffer-string)
                :point (point)
                :regions (mapcar #'mozc-test-region-state extras)
                :modified (buffer-modified-p)
                :undo buffer-undo-list)
          states)
    (restore-buffer-modified-p nil)
    (goto-char (car primary))
    (mozc-buffer-placeholder-setq primary "[変換中]")
    (push (list :operation 'replaced
                :buffer (buffer-string)
                :point (point)
                :region (mozc-test-region-state primary)
                :modified (buffer-modified-p)
                :undo buffer-undo-list)
          states)
    (mozc-buffer-placeholder-delete primary)
    (mozc-buffer-placeholder-delete-all extras)
    (list :states (nreverse states)
          :final-buffer (buffer-string)
          :final-point (point)
          :primary primary
          :extras extras
          :modified (buffer-modified-p)
          :undo buffer-undo-list)))
"##;
    let expect = expect![[
        r####"OK (:states ((:operation primary :buffer "alpha <*>omega" :point 10 :region (:range (7 10) :live t) :modified nil :undo nil) (:operation stacked :buffer "alpha <*>omega候補##" :point 19 :regions ((:range (17 19) :live t) (:range (15 17) :live t)) :modified t :undo nil) (:operation replaced :buffer "alpha [変換中]omega候補##" :point 12 :region (:range (7 12) :live t) :modified nil :undo nil)) :final-buffer "alpha omega" :final-point 7 :primary nil :extras nil :modified nil :undo nil)"####
    ]];
    ParityBatchCase::value(
        "temporary_display_placeholders_preserve_document_undo_and_modified_state",
        elisp_form,
        expect,
    )
}

fn preedit_composition_tracks_cursor_segments_and_a_live_overlay_without_editing_the_document()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert "Destination: .")
  (goto-char (1- (point-max)))
  (restore-buffer-modified-p nil)
  (let* ((mozc-preedit-style '(fence))
         (single '((cursor . 2)
                   (segment ((value . "かな入力")))))
         (segmented '((cursor . 4)
                      (segment ((value . "日本") (annotation . highlight))
                               ((value . "語入力")))))
         (conversion '((category . conversion)))
         (middle (mozc-preedit-make-text single "|" "|" " "))
         (at-end (mozc-preedit-make-text
                  '((cursor . 4) (segment ((value . "変換済み"))))
                  "|" "|" " "))
         first second cleared)
    (cl-letf (((symbol-function 'mozc-posn-at-point)
               (lambda (&rest _args) '(:test-position 12 4))))
      (mozc-preedit-update single)
      (setq first
            (list :buffer (buffer-string)
                  :point (point)
                  :origin (marker-position mozc-preedit-point-origin)
                  :position mozc-preedit-posn-origin
                  :in-session mozc-preedit-in-session-flag
                  :placeholder
                  (mozc-test-region-state mozc-preedit-overlay-placeholder-region)
                  :overlay (mozc-test-overlay-state mozc-preedit-overlay)
                  :modified (buffer-modified-p)))
      (mozc-preedit-update segmented conversion)
      (setq second
            (list :buffer (buffer-string)
                  :point (point)
                  :overlay (mozc-test-overlay-state mozc-preedit-overlay)
                  :modified (buffer-modified-p)))
      (mozc-preedit-clear)
      (setq cleared
            (list :buffer (buffer-string)
                  :overlay (mozc-test-overlay-state mozc-preedit-overlay)
                  :in-session mozc-preedit-in-session-flag))
      (mozc-preedit-clean-up)
      (list :composed
            (list :middle (mozc-test-string-state middle)
                  :at-end (mozc-test-string-state at-end))
            :first first
            :second second
            :cleared cleared
            :final
            (list :buffer (buffer-string)
                  :point (point)
                  :overlay mozc-preedit-overlay
                  :placeholder mozc-preedit-overlay-placeholder-region
                  :origin mozc-preedit-point-origin
                  :in-session mozc-preedit-in-session-flag
                  :modified (buffer-modified-p))))))
"##;
    let expect = expect![[
        r####"OK (:composed (:middle (:text "|かな入力|" :runs ((:range (0 1) :text "|" :face nil :cursor nil) (:range (1 3) :text "かな" :face mozc-preedit-face :cursor nil) (:range (3 4) :text "入" :face mozc-preedit-face :cursor 4) (:range (4 5) :text "力" :face mozc-preedit-face :cursor nil) (:range (5 6) :text "|" :face nil :cursor nil))) :at-end (:text "|変換済み|" :runs ((:range (0 1) :text "|" :face nil :cursor nil) (:range (1 5) :text "変換済み" :face mozc-preedit-face :cursor nil) (:range (5 6) :text "|" :face nil :cursor 1)))) :first (:buffer "Destination: *." :point 14 :origin 14 :position (:test-position 12 4) :in-session t :placeholder (:range (14 15) :live t) :overlay (:range (14 15) :buffer-live t :display (:text "|かな入力|" :runs ((:range (0 1) :text "|" :face nil :cursor nil) (:range (1 3) :text "かな" :face mozc-preedit-face :cursor nil) (:range (3 4) :text "入" :face mozc-preedit-face :cursor 4) (:range (4 5) :text "力" :face mozc-preedit-face :cursor nil) (:range (5 6) :text "|" :face nil :cursor nil)))) :modified nil) :second (:buffer "Destination: *." :point 14 :overlay (:range (14 15) :buffer-live t :display (:text "|日本 語入力|" :runs ((:range (0 1) :text "|" :face nil :cursor nil) (:range (1 3) :text "日本" :face mozc-preedit-selected-face :cursor nil) (:range (3 4) :text " " :face nil :cursor nil) (:range (4 7) :text "語入力" :face mozc-preedit-face :cursor nil) (:range (7 8) :text "|" :face nil :cursor 1)))) :modified nil) :cleared (:buffer "Destination: *." :overlay (:range (14 15) :buffer-live t :display nil) :in-session t) :final (:buffer "Destination: ." :point 14 :overlay nil :placeholder nil :origin nil :in-session nil :modified nil))"####
    ]];
    ParityBatchCase::value(
        "preedit_composition_tracks_cursor_segments_and_a_live_overlay_without_editing_the_document",
        elisp_form,
        expect,
    )
}

fn candidate_renderers_preserve_focus_shortcuts_annotations_and_dispatch_style() -> ParityBatchCase
{
    let elisp_form = r##"
(let* ((candidates
        '((focused-index . 1)
          (size . 12)
          (footer (index-visible . t))
          (candidate
           ((index . 0) (value . "東京")
            (annotation (description . "地名") (shortcut . "1")))
           ((index . 1) (value . "とうきょう")
            (annotation (description . "ひらがな") (shortcut . "2")))
           ((index . 2) (value . "東亰")
            (annotation (description . "異体字"))))))
       (echo (mozc-cand-echo-area-make-contents candidates))
       (overlay (mozc-cand-overlay-make-contents candidates))
       dispatch)
  (cl-letf (((symbol-function 'mozc-cand-overlay-clean-up)
             (lambda () (push 'overlay-clean-up dispatch)))
            ((symbol-function 'mozc-cand-overlay-clear)
             (lambda () (push 'overlay-clear dispatch)))
            ((symbol-function 'mozc-cand-overlay-update)
             (lambda (value)
               (push (list 'overlay-update
                           (mozc-protobuf-get value 'focused-index)
                           (length (mozc-protobuf-get value 'candidate)))
                     dispatch)))
            ((symbol-function 'mozc-cand-echo-area-clean-up)
             (lambda () (push 'echo-clean-up dispatch)))
            ((symbol-function 'mozc-cand-echo-area-clear)
             (lambda () (push 'echo-clear dispatch)))
            ((symbol-function 'mozc-cand-echo-area-update)
             (lambda (value)
               (push (list 'echo-update
                           (mozc-protobuf-get value 'focused-index)
                           (length (mozc-protobuf-get value 'candidate)))
                     dispatch))))
    (let ((mozc-candidate-style 'overlay))
      (mozc-candidate-update candidates)
      (mozc-candidate-clear)
      (mozc-candidate-clean-up))
    (let ((mozc-candidate-style 'echo-area))
      (mozc-candidate-update candidates)
      (mozc-candidate-clear)
      (mozc-candidate-clean-up)))
  (list :echo (mozc-test-string-state echo)
        :overlay overlay
        :dispatch (nreverse dispatch)))
"##;
    let expect = expect![[
        r####"OK (:echo (:text "2/12 1. 東京 (地名) 2. とうきょう (ひらがな) 3. 東亰 (異体字)" :runs ((:range (0 4) :text "2/12" :face mozc-cand-echo-area-stats-face :cursor nil) (:range (4 5) :text " " :face nil :cursor nil) (:range (5 7) :text "1." :face mozc-cand-echo-area-shortcut-face :cursor nil) (:range (7 8) :text " " :face nil :cursor nil) (:range (8 10) :text "東京" :face mozc-cand-echo-area-candidate-face :cursor nil) (:range (10 11) :text " " :face nil :cursor nil) (:range (11 15) :text "(地名)" :face mozc-cand-echo-area-annotation-face :cursor nil) (:range (15 16) :text " " :face nil :cursor nil) (:range (16 18) :text "2." :face mozc-cand-echo-area-shortcut-face :cursor nil) (:range (18 19) :text " " :face nil :cursor nil) (:range (19 24) :text "とうきょう" :face mozc-cand-echo-area-focused-face :cursor nil) (:range (24 25) :text " " :face nil :cursor nil) (:range (25 31) :text "(ひらがな)" :face mozc-cand-echo-area-annotation-face :cursor nil) (:range (31 32) :text " " :face nil :cursor nil) (:range (32 34) :text "3." :face mozc-cand-echo-area-shortcut-face :cursor nil) (:range (34 35) :text " " :face nil :cursor nil) (:range (35 37) :text "東亰" :face mozc-cand-echo-area-candidate-face :cursor nil) (:range (37 38) :text " " :face nil :cursor nil) (:range (38 43) :text "(異体字)" :face mozc-cand-echo-area-annotation-face :cursor nil))) :overlay (("1. 東京" "地名" mozc-cand-overlay-odd-face) ("2. とうきょう" "ひらがな" mozc-cand-overlay-focused-face) ("東亰" "異体字" mozc-cand-overlay-odd-face) (nil "2/12" mozc-cand-overlay-footer-face)) :dispatch ((overlay-update 1 3) overlay-clear overlay-clean-up (echo-update 1 3) echo-clear echo-clean-up))"####
    ]];
    ParityBatchCase::value(
        "candidate_renderers_preserve_focus_shortcuts_annotations_and_dispatch_style",
        elisp_form,
        expect,
    )
}

fn session_protocol_correlates_responses_and_wraps_event_ids_without_cross_talk() -> ParityBatchCase
{
    let elisp_form = r##"
(let ((mozc-session-seq 41)
      (mozc-session-id nil)
      (mozc-mode t)
      (responses
       '(((emacs-event-id . 7) (emacs-session-id . 7000)
          (output (ignored . stale)))
         ((emacs-event-id . 41) (emacs-session-id . 9001)
          (output (created . t)))
         ((emacs-event-id . 42) (emacs-session-id . 9001)
          (output (consumed . t) (result (type . string) (value . "東京"))))
         ((emacs-event-id . 43) (emacs-session-id . 1234)
          (output (consumed . t)))))
      sent wrap-results)
  (cl-letf (((symbol-function 'mozc-helper-process-send-sexpr)
             (lambda (&rest payload) (push payload sent)))
            ((symbol-function 'mozc-helper-process-recv-sexpr)
             (lambda () (pop responses))))
    (let ((created (mozc-session-execute-command 'CreateSession))
          sent-key mismatch)
      (setq sent-key (mozc-session-execute-command 'SendKey ?a 'shift))
      (setq mismatch (mozc-session-execute-command 'SendKey 'space))
      (dolist (start '(0 134217726 134217727 134217728 -1))
        (let ((mozc-session-seq start))
          (push (list start (mozc-session-seq-inc)) wrap-results)))
      (list :created created
            :sent-key sent-key
            :mismatch mismatch
            :session-id mozc-session-id
            :next-sequence mozc-session-seq
            :sent (nreverse sent)
            :responses-left responses
            :wrap (nreverse wrap-results)))))
"##;
    let expect = expect![[
        r####"OK (:created ((created . t)) :sent-key ((consumed . t) (result (type . string) (value . "東京"))) :mismatch nil :session-id 9001 :next-sequence 44 :sent ((41 CreateSession) (42 SendKey 9001 97 shift) (43 SendKey 9001 space)) :responses-left nil :wrap ((0 1) (134217726 134217727) (134217727 0) (134217728 0) (-1 0)))"####
    ]];
    ParityBatchCase::value(
        "session_protocol_correlates_responses_and_wraps_event_ids_without_cross_talk",
        elisp_form,
        expect,
    )
}

fn fragmented_helper_output_is_framed_once_and_rejects_malformed_sexpressions() -> ParityBatchCase {
    let elisp_form = r##"
(let ((fake-process 'mozc-test-helper)
      (mozc-helper-process 'mozc-test-helper)
      (mozc-helper-process-message-queue nil)
      (mozc-helper-process-string-buf nil)
      parsed stops)
  (mozc-helper-process-filter fake-process "((emacs-event-id . 1) ")
  (mozc-helper-process-filter fake-process "(output . ok))\npartial")
  (mozc-helper-process-filter 'stale-helper "-ignored\n")
  (mozc-helper-process-filter fake-process
                              "-message\n((emacs-event-id . 2) (output . next))\n")
  (let ((framed (list :queue (copy-sequence mozc-helper-process-message-queue)
                      :remainder mozc-helper-process-string-buf)))
    (let ((raw-responses
           '(" ((emacs-event-id . 9) (output (consumed . t)))  "
             "((x . 1)) trailing"
             "((x . 2)"
             nil)))
      (cl-letf (((symbol-function 'mozc-helper-process-recv-response)
                 (lambda () (pop raw-responses)))
                ((symbol-function 'mozc-helper-process-stop)
                 (lambda () (push 'stopped stops))))
        (dotimes (_ 4)
          (push (mozc-helper-process-recv-sexpr) parsed))))
    (list
     :framed framed
     :parsed (nreverse parsed)
     :stops (nreverse stops)
     :nested
     (list (mozc-protobuf-get
            '((output (candidate ((value . "東京")) ((value . "京都")))))
            'output 'candidate 1 'value)
           (mozc-protobuf-get '((output . invalid)) 'output 'candidate 0))
     :splits
     (list (mozc-split-at-last (list 'a 'b 'c 'd))
           (mozc-split-at-last (list 'a 'b 'c 'd) 2)
           (mozc-split-at-last (list 'a) 3)))))
"##;
    let expect = expect![[
        r####"OK (:framed (:queue ("((emacs-event-id . 1) (output . ok))" "partial-message" "((emacs-event-id . 2) (output . next))") :remainder "") :parsed (((emacs-event-id . 9) (output (consumed . t))) wrong-format wrong-format no-data-available) :stops (stopped stopped) :nested ("京都" nil) :splits (((a b c) d) ((a b) c d) (nil a)))"####
    ]];
    ParityBatchCase::value(
        "fragmented_helper_output_is_framed_once_and_rejects_malformed_sexpressions",
        elisp_form,
        expect,
    )
}

fn consumed_and_fallback_events_drive_the_complete_editor_state_machine() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert "Destination: ")
  (let ((outputs
         '(((consumed . t)
            (result (type . string) (value . "東京")))
           ((consumed . t)
            (preedit (cursor . 2) (segment ((value . "かな"))))
            (candidate-window
             (focused-index . 0) (size . 1)
             (candidate ((index . 0) (value . "仮名")))))
           ((consumed . t))
           ((consumed . nil))))
        events states)
    (cl-letf (((symbol-function 'mozc-send-key-event)
               (lambda (event)
                 (push (list :send event) events)
                 (pop outputs)))
              ((symbol-function 'mozc-clean-up-changes-on-buffer)
               (lambda () (push :clean-up events)))
              ((symbol-function 'mozc-preedit-update)
               (lambda (preedit candidates)
                 (push (list :preedit-update
                             (mozc-protobuf-get preedit 'cursor)
                             (mozc-protobuf-get preedit 'segment 0 'value)
                             (mozc-protobuf-get candidates 'focused-index))
                       events)))
              ((symbol-function 'mozc-preedit-clear)
               (lambda () (push :preedit-clear events)))
              ((symbol-function 'mozc-candidate-update)
               (lambda (candidates)
                 (push (list :candidate-update
                             (mozc-protobuf-get candidates 'focused-index)
                             (mozc-protobuf-get candidates 'candidate 0 'value))
                       events)))
              ((symbol-function 'mozc-candidate-clear)
               (lambda () (push :candidate-clear events)))
              ((symbol-function 'mozc-fall-back-on-default-binding)
               (lambda (event)
                 (push (list :fallback (if (consp event) (car event) event))
                       events))))
      (dolist (event (list ?t ?o ?x ?! '(mouse-1 test-position)))
        (mozc-handle-event event)
        (push (list :after (if (consp event) (car event) event)
                    :buffer (buffer-string)
                    :point (point)
                    :remaining (length outputs))
              states)))
    (list :states (nreverse states)
          :events (nreverse events)
          :final-buffer (buffer-string)
          :final-point (point))))
"##;
    let expect = expect![[
        r####"OK (:states ((:after 116 :buffer "Destination: 東京" :point 16 :remaining 3) (:after 111 :buffer "Destination: 東京" :point 16 :remaining 2) (:after 120 :buffer "Destination: 東京" :point 16 :remaining 1) (:after 33 :buffer "Destination: 東京" :point 16 :remaining 0) (:after mouse-1 :buffer "Destination: 東京" :point 16 :remaining 0)) :events ((:send 116) :clean-up :preedit-clear :candidate-clear (:send 111) (:preedit-update 2 "かな" 0) (:candidate-update 0 "仮名") (:send 120) :clean-up (:send 33) :clean-up (:fallback 33) (:fallback mouse-1)) :final-buffer "Destination: 東京" :final-point 16)"####
    ]];
    ParityBatchCase::value(
        "consumed_and_fallback_events_drive_the_complete_editor_state_machine",
        elisp_form,
        expect,
    )
}

#[test]
fn mozc_package_batch() {
    let cases = vec![
        input_mode_lifecycle_prioritizes_and_temporarily_disables_its_catch_all_keymap(),
        key_translation_and_custom_kana_maps_build_the_helper_payload_used_for_real_typing(),
        temporary_display_placeholders_preserve_document_undo_and_modified_state(),
        preedit_composition_tracks_cursor_segments_and_a_live_overlay_without_editing_the_document(
        ),
        candidate_renderers_preserve_focus_shortcuts_annotations_and_dispatch_style(),
        session_protocol_correlates_responses_and_wraps_event_ids_without_cross_talk(),
        fragmented_helper_output_is_framed_once_and_rejects_malformed_sexpressions(),
        consumed_and_fallback_events_drive_the_complete_editor_state_machine(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Mozc parity test");
    assert_oracle_batch_cases(mozc_oracle(), test_name, "mozc_parity", &cases);
}
