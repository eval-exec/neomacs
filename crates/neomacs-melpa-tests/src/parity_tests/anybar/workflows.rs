use expect_test::expect;

use super::ParityBatchCase;

fn documented_indicator_lifecycle_updates_the_default_anybar_instance() -> ParityBatchCase {
    ParityBatchCase::value(
        "documented_indicator_lifecycle_updates_the_default_anybar_instance",
        r##"
(progn
  (neomacs-anybar-test-reset)
  (let ((anybar-executable-location "/Users/demo/Applications/AnyBar.app")
        states)
    (neomacs-anybar-test-call-with-boundary
     (lambda ()
       (push (neomacs-anybar-test-state) states)
       (anybar-start)
       (push (neomacs-anybar-test-state) states)
       (anybar-set "green")
       (push (neomacs-anybar-test-state) states)
       (anybar-set "purple")
       (push (neomacs-anybar-test-state) states)
       (anybar-quit)
       (push (neomacs-anybar-test-state) states)))
    (list
     (reverse states)
     (neomacs-anybar-test-events))))
"##,
        expect![[
            r#"OK ((nil ((:port 1738 :application "/Users/demo/Applications/AnyBar.app" :style "white")) ((:port 1738 :application "/Users/demo/Applications/AnyBar.app" :style "green")) ((:port 1738 :application "/Users/demo/Applications/AnyBar.app" :style "purple")) nil) ((launch :port 1738 :application "/Users/demo/Applications/AnyBar.app" :output-buffer nil :error-buffer nil) (connect :name "anybar" :type datagram :host local :port 1738) (send :port 1738 :command "green") (close :port 1738) (connect :name "anybar" :type datagram :host local :port 1738) (send :port 1738 :command "purple") (close :port 1738) (connect :name "anybar" :type datagram :host local :port 1738) (send :port 1738 :command "quit") (close :port 1738)))"#
        ]],
    )
}

fn custom_images_refresh_while_two_ports_keep_independent_indicator_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "custom_images_refresh_while_two_ports_keep_independent_indicator_state",
        r##"
(let* ((home (getenv "HOME"))
       (image-directory (expand-file-name ".AnyBar" home))
       (image-file
        (lambda (name)
          (expand-file-name name image-directory)))
       result)
  (unwind-protect
      (progn
        (when
            (file-exists-p image-directory)
          (delete-directory image-directory t))
        (make-directory image-directory t)
        (dolist
            (name
             '("deploy.png"
               "deploy@2x.png"
               "deploy_alt.png"
               "review.png"))
          (with-temp-file (funcall image-file name)
            (insert "fixture")))
        (neomacs-anybar-test-reset)
        (neomacs-anybar-test-call-with-boundary
         (lambda ()
           (let ((initial-images
                  (copy-sequence (anybar-images-reset))))
             (anybar-start 2401)
             (anybar-start 2402)
             (anybar-set "deploy" 2401)
             (anybar-set "review" 2402)
             (let ((before-refresh
                    (neomacs-anybar-test-state)))
               (dolist
                   (name
                    '("deploy.png"
                      "deploy@2x.png"
                      "deploy_alt.png"))
                 (delete-file (funcall image-file name)))
               (dolist
                   (name
                    '("canary.png"
                      "canary@2x.png"
                      "canary_alt.png"))
                 (with-temp-file (funcall image-file name)
                   (insert "fixture")))
               (let ((refreshed-images
                      (copy-sequence (anybar-images-reset))))
                 (when
                     (get-buffer "*Warnings*")
                   (kill-buffer "*Warnings*"))
                 (anybar-set "deploy" 2401)
                 (let ((warning-text
                        (with-current-buffer "*Warnings*"
                          (buffer-substring-no-properties
                           (point-min)
                           (point-max)))))
                   (anybar-set "canary" 2401)
                   (setq result
                         (list
                          :initial-images initial-images
                          :before-refresh before-refresh
                          :refreshed-images refreshed-images
                          :after-refresh
                          (neomacs-anybar-test-state)
                          :warning-buffer warning-text
                          :events
                          (neomacs-anybar-test-events))))))))))
    (when
        (file-exists-p image-directory)
      (delete-directory image-directory t)))
  result)
"##,
        expect![[
            r#"OK (:initial-images ("deploy" "review") :before-refresh ((:port 2401 :application "/Applications/AnyBar.app" :style "deploy") (:port 2402 :application "/Applications/AnyBar.app" :style "review")) :refreshed-images ("canary" "review") :after-refresh ((:port 2401 :application "/Applications/AnyBar.app" :style "canary") (:port 2402 :application "/Applications/AnyBar.app" :style "review")) :warning-buffer "Warning (AnyBar): Not a style: deploy\n" :events ((launch :port 2401 :application "/Applications/AnyBar.app" :output-buffer nil :error-buffer nil) (launch :port 2402 :application "/Applications/AnyBar.app" :output-buffer nil :error-buffer nil) (connect :name "anybar" :type datagram :host local :port 2401) (send :port 2401 :command "deploy") (close :port 2401) (connect :name "anybar" :type datagram :host local :port 2402) (send :port 2402 :command "review") (close :port 2402) (connect :name "anybar" :type datagram :host local :port 2401) (send :port 2401 :command "canary") (close :port 2401)))"#
        ]],
    )
}

fn interactive_commands_drive_a_complete_indicator_session_through_their_prompts() -> ParityBatchCase
{
    ParityBatchCase::value(
        "interactive_commands_drive_a_complete_indicator_session_through_their_prompts",
        r##"
(progn
  (neomacs-anybar-test-reset)
  (let ((ports '(4173 4173 4173 4173))
        (styles '("orange"))
        (commands '("question"))
        prompts
        states)
    (neomacs-anybar-test-call-with-boundary
     (lambda ()
       (cl-letf
           (((symbol-function 'read-number)
             (lambda (prompt default)
               (let ((answer (pop ports)))
                 (push
                  (list
                   :kind 'port
                   :prompt prompt
                   :default default
                   :answer answer)
                  prompts)
                 answer)))
            ((symbol-function 'completing-read)
             (lambda
               (prompt collection &optional predicate require-match
                       initial-input history default inherit-input-method)
               (let ((answer (pop styles)))
                 (push
                  (list
                   :kind 'style
                   :prompt prompt
                   :choices (all-completions "" collection predicate)
                   :require-match require-match
                   :initial-input initial-input
                   :history history
                   :default default
                   :inherit-input-method inherit-input-method
                   :answer answer)
                  prompts)
                 answer)))
            ((symbol-function 'read-string)
             (lambda
               (prompt &optional initial-input history default-value
                       inherit-input-method)
               (let ((answer (pop commands)))
                 (push
                  (list
                   :kind 'command
                   :prompt prompt
                   :initial-input initial-input
                   :history history
                   :default-value default-value
                   :inherit-input-method inherit-input-method
                   :answer answer)
                  prompts)
                 answer))))
         (call-interactively #'anybar-start)
         (push (neomacs-anybar-test-state) states)
         (call-interactively #'anybar-set)
         (push (neomacs-anybar-test-state) states)
         (call-interactively #'anybar-send)
         (push (neomacs-anybar-test-state) states)
         (call-interactively #'anybar-quit)
         (push (neomacs-anybar-test-state) states))))
    (list
     :prompts (reverse prompts)
     :states (reverse states)
     :events (neomacs-anybar-test-events)
     :unused-input (list ports styles commands))))
"##,
        expect![[
            r#"OK (:prompts ((:kind port :prompt "Port: " :default 1738 :answer 4173) (:kind style :prompt "Style: " :choices ("white" "red" "orange" "yellow" "green" "cyan" "blue" "purple" "black" "question" "exclamation") :require-match nil :initial-input nil :history nil :default nil :inherit-input-method nil :answer "orange") (:kind port :prompt "Port: " :default 1738 :answer 4173) (:kind command :prompt "Command: " :initial-input nil :history nil :default-value nil :inherit-input-method nil :answer "question") (:kind port :prompt "Port: " :default 1738 :answer 4173) (:kind port :prompt "Port: " :default 1738 :answer 4173)) :states (((:port 4173 :application "/Applications/AnyBar.app" :style "white")) ((:port 4173 :application "/Applications/AnyBar.app" :style "orange")) ((:port 4173 :application "/Applications/AnyBar.app" :style "question")) nil) :events ((launch :port 4173 :application "/Applications/AnyBar.app" :output-buffer nil :error-buffer nil) (connect :name "anybar" :type datagram :host local :port 4173) (send :port 4173 :command "orange") (close :port 4173) (connect :name "anybar" :type datagram :host local :port 4173) (send :port 4173 :command "question") (close :port 4173) (connect :name "anybar" :type datagram :host local :port 4173) (send :port 4173 :command "quit") (close :port 4173)) :unused-input (nil nil nil))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        documented_indicator_lifecycle_updates_the_default_anybar_instance(),
        custom_images_refresh_while_two_ports_keep_independent_indicator_state(),
        interactive_commands_drive_a_complete_indicator_session_through_their_prompts(),
    ]
}
