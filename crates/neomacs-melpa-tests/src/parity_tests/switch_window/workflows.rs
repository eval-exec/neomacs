use expect_test::expect;

use super::ParityBatchCase;

/// The configuration surface: the autoloaded commands, the documented
/// defcustoms with defaults and types, and the label/background faces as
/// registered specs (a batch frame is a mono display, so `((t ...))'
/// specs are what exists here).
fn the_configuration_surface_and_entry_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_configuration_surface_and_entry_commands",
        r####"(list
 :source (sw1cc-test-source-state)
 :commands
 (mapcar (lambda (command)
           (list :command command :commandp (commandp command)))
         '(switch-window
           switch-window-then-split-horizontally
           switch-window-then-split-vertically
           switch-window-then-delete
           switch-window-then-maximize
           switch-window-then-swap-buffer
           switch-window-mvswap-buffer
           switch-window-then-balanced-split
           switch-window-then-split-below-right
           switch-window-then-split-above-left))
 :options
 (mapcar
  (lambda (option)
    (list :option option
          :custom-variable-p (and (custom-variable-p option) t)
          :standard (eval (car (get option 'standard-value)))
          :type (get option 'custom-type)))
  '(switch-window-timeout
    switch-window-threshold
    switch-window-relative
    switch-window-shortcut-style
    switch-window-qwerty-shortcuts
    switch-window-shortcut-appearance
    switch-window-input-style
    switch-window-minibuffer-shortcut
    switch-window-multiple-frames
    switch-window-auto-resize-window
    switch-window-default-window-size
    switch-window-finish-hook
    switch-window-preferred))
 :faces
 (list :label (get 'switch-window-label 'face-defface-spec)
       :background (get 'switch-window-background 'face-defface-spec)))"####,
        expect![[
            r#"OK (:source (:upstream-tree "5bea09fa13b95375d95fd84ceaef60a503e39a21" :feature t :version "20260316.257") :commands ((:command switch-window :commandp t) (:command switch-window-then-split-horizontally :commandp t) (:command switch-window-then-split-vertically :commandp t) (:command switch-window-then-delete :commandp t) (:command switch-window-then-maximize :commandp t) (:command switch-window-then-swap-buffer :commandp t) (:command switch-window-mvswap-buffer :commandp nil) (:command switch-window-then-balanced-split :commandp nil) (:command switch-window-then-split-below-right :commandp nil) (:command switch-window-then-split-above-left :commandp nil)) :options ((:option switch-window-timeout :custom-variable-p t :standard 5 :type integer) (:option switch-window-threshold :custom-variable-p t :standard 2 :type integer) (:option switch-window-relative :custom-variable-p t :standard nil :type boolean) (:option switch-window-shortcut-style :custom-variable-p t :standard quail :type (choice (const :tag "Alphabet" alphabet) (const :tag "Keyboard Layout" quail) (const :tag "Qwerty Homekeys Layout" qwerty))) (:option switch-window-qwerty-shortcuts :custom-variable-p t :standard ("a" "s" "d" "f" "j" "k" "l" ";" "g" "h" "q" "w" "e" "r" "t" "y" "u" "i" "p" "z" "x" "c" "v" "b" "n" "m") :type (repeat string)) (:option switch-window-shortcut-appearance :custom-variable-p t :standard text :type (choice (const :tag "Show shortcut with text" text) (const :tag "Show shortcut with Ascii art." asciiart) (const :tag "Show shortcut with image." image))) (:option switch-window-input-style :custom-variable-p t :standard minibuffer :type (choice (const :tag "Get input by read-event" read-event) (const :tag "Get input from minibuffer" minibuffer))) (:option switch-window-minibuffer-shortcut :custom-variable-p t :standard nil :type (choice (const :tag "Off" nil) (character "m"))) (:option switch-window-multiple-frames :custom-variable-p t :standard nil :type boolean) (:option switch-window-auto-resize-window :custom-variable-p t :standard nil :type (choice boolean function)) (:option switch-window-default-window-size :custom-variable-p t :standard 0.7 :type (choice (const :tag "Off" nil) (float :tag "Fraction of frame size") (cons :tag "Fractions of frame width and height" (float :tag "Fraction of frame width") (float :tag "Fraction of frame height")))) (:option switch-window-finish-hook :custom-variable-p t :standard nil :type hook) (:option switch-window-preferred :custom-variable-p t :standard default :type (choice (const :tag "Emacs default" default) (const :tag "Helm" helm) (const :tag "Ivy or Counsel" ivy) (const :tag "Ido" ido)))) :faces (:label ((t (:inherit font-lock-builtin-face :height 3.0))) :background ((t (:foreground "gray40")))))"#
        ]],
    )
}

/// The window enumeration: `switch-window--list' walks the frame's
/// windows from the top-left, and from the selected window when relative
/// ordering is on; the label assignment numbers the windows in list
/// order.
fn the_enumeration_orders_windows_and_assigns_labels() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_enumeration_orders_windows_and_assigns_labels",
        r####"(unwind-protect
    (progn
      (sw1cc-test-reset)
      (get-buffer-create "sw-one")
      (get-buffer-create "sw-two")
      (get-buffer-create "sw-three")
      (switch-to-buffer "sw-one")
      (split-window-right)
      (other-window 1)
      (switch-to-buffer "sw-two")
      (split-window-below)
      (other-window 1)
      (switch-to-buffer "sw-three")
      (let ((absolute (sw1cc-test-window-ids))
            (listed (mapcar (lambda (w) (buffer-name (window-buffer w)))
                            (switch-window--list)))
            (relative (progn
                        (setq switch-window-relative t)
                        (mapcar (lambda (w) (buffer-name (window-buffer w)))
                                (switch-window--list))))
            (labels (mapcar #'switch-window--label '(1 2 3))))
        (setq switch-window-relative nil)
        (list :windows absolute
              :absolute listed
              :relative relative
              :labels labels)))
  (sw1cc-test-reset))"####,
        expect![[
            r#"OK (:windows (("sw-one" (0 1 40 25)) ("sw-two" (40 1 80 13)) ("sw-three" (40 13 80 25))) :absolute ("sw-one" "sw-two" "sw-three") :relative ("sw-three" "sw-one" "sw-two") :labels ("1" "2" "3"))"#
        ]],
    )
}

/// The shortcut keys: the quail keyboard layout supplies the keys in the
/// documented walk order, the qwerty shortcuts are used as given, and
/// the minibuffer shortcut is excluded when set.
fn the_shortcut_key_lists_follow_the_documented_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_shortcut_key_lists_follow_the_documented_order",
        r####"(unwind-protect
    (progn
      (sw1cc-test-reset)
      (let ((keyboard (switch-window--list-keyboard-keys))
            (quail (switch-window--list-keys))
            (qwerty (progn
                      (setq switch-window-shortcut-style 'qwerty)
                      (switch-window--list-keys)))
            (excluded (progn
                        (setq switch-window-shortcut-style 'qwerty
                              switch-window-minibuffer-shortcut ?s)
                        (switch-window--list-keys))))
        (list :keyboard keyboard
              :quail quail
              :qwerty qwerty
              :excluded excluded)))
  (sw1cc-test-reset))"####,
        expect![[
            r#"OK (:keyboard ("1" "2" "3" "4" "5" "6" "7" "8" "9" "0" "q" "w" "e" "r" "t" "y" "u" "i" "o" "p" "a" "s" "d" "f" "g" "h" "j" "k" "l" ";" "z" "x" "c" "v" "b" "n" "m" "," "." "/") :quail ("1" "2" "3" "4" "5" "6" "7" "8" "9" "0" "q" "w" "e" "r" "t" "y" "u" "o" "p" "a" "s" "d" "f" "g" "h" ";" "z" "x" "c" "v" "n" "m" "," "." "/") :qwerty ("a" "s" "d" "f") :excluded ("a" "d" "f"))"#
        ]],
    )
}

/// The label buffer: `switch-window--create-label-buffer' builds a buffer
/// whose content is the window's label carrying the documented face, and
/// the display-number selection picks the label function the input
/// style configures.
fn the_label_buffer_carries_the_documented_face() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_label_buffer_carries_the_documented_face",
        r####"(unwind-protect
    (progn
      (sw1cc-test-reset)
      (let ((target (generate-new-buffer " sw-label"))
            buffer)
        (unwind-protect
            (progn
              (setq buffer
                    (switch-window--create-label-buffer nil target "a" nil))
              (with-current-buffer buffer
                (list :name (substring-no-properties (buffer-string))
                      :face (get-text-property 0 'face (buffer-string))
                      :label-fn switch-window-label-buffer-function
                      :input-style switch-window-input-style
                      :appearance switch-window-shortcut-appearance)))
          (when (buffer-live-p target)
            (kill-buffer target)))))
  (sw1cc-test-reset))"####,
        expect![[
            r#"OK (:name "a " :face switch-window-label :label-fn switch-window--create-label-buffer :input-style minibuffer :appearance text)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_configuration_surface_and_entry_commands(),
        the_enumeration_orders_windows_and_assigns_labels(),
        the_shortcut_key_lists_follow_the_documented_order(),
        the_label_buffer_carries_the_documented_face(),
    ]
}
