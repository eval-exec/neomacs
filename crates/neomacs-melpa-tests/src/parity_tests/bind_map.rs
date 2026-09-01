use std::time::Duration;

use expect_test::expect;

use crate::{BIND_MAP_MELPA_PIN, CachedMelpaOracle, EVIL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const BIND_MAP_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const BIND_MAP_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'bind-map)
(require 'evil)

(defvar neomacs-bind-map-test-history nil)

(defun neomacs-bind-map-test-record (action)
  "Record an interactive ACTION with its effective editor context."
  (push (list :action action
              :buffer (buffer-name)
              :major-mode major-mode
              :review-mode (and (boundp 'neomacs-bind-map-review-mode)
                                neomacs-bind-map-review-mode)
              :conflict-mode
              (and (boundp 'neomacs-bind-map-conflict-mode)
                   neomacs-bind-map-conflict-mode)
              :evil-state (and (boundp 'evil-state) evil-state))
        neomacs-bind-map-test-history))

(dolist (definition
         '((neomacs-bind-map-build build)
           (neomacs-bind-map-test test)
           (neomacs-bind-map-deploy deploy)
           (neomacs-bind-map-notes notes)
           (neomacs-bind-map-child child)
           (neomacs-bind-map-global global)
           (neomacs-bind-map-override override)
           (neomacs-bind-map-conflict conflict)
           (neomacs-bind-map-evil evil)))
  (let ((command (car definition))
        (action (cadr definition)))
    (fset command
          (lambda ()
            (interactive)
            (neomacs-bind-map-test-record action)))))

(define-derived-mode neomacs-bind-map-classic-mode prog-mode
  "BM-Classic")
(define-derived-mode neomacs-bind-map-modern-mode prog-mode
  "BM-Modern")
(defalias 'neomacs-bind-map-legacy-alias-mode
  'neomacs-bind-map-modern-mode)

(define-minor-mode neomacs-bind-map-review-mode
  "Minor mode used by the Bind Map parity corpus.")

(defvar neomacs-bind-map-conflict-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "C-c o x") #'neomacs-bind-map-conflict)
    map))
(define-minor-mode neomacs-bind-map-conflict-mode
  "Minor mode with a binding that an override map must beat."
  :keymap neomacs-bind-map-conflict-mode-map)

(defun neomacs-bind-map-test-in-buffer (name function)
  "Run FUNCTION in a temporary buffer named for NAME."
  (let ((buffer (generate-new-buffer (format "*bind-map-%s*" name))))
    (unwind-protect
        (with-current-buffer buffer
          (funcall function))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))

(defun neomacs-bind-map-test-execute (key)
  "Execute the effective command bound to KEY and return its symbol."
  (let ((binding (key-binding (kbd key))))
    (when (commandp binding)
      (call-interactively binding))
    binding))

(defun neomacs-bind-map-test-properties (map)
  "Return the stable public properties installed for MAP."
  (list :root-map (get map :root-map)
        :active-var (get map :active-var)
        :prefix-cmd (get map :prefix-cmd)
        :override-minor-modes (get map :override-minor-modes)
        :override-mode-name (get map :override-mode-name)
        :keys (get map :keys)
        :evil-keys (get map :evil-keys)
        :evil-states (get map :evil-states)
        :minor-modes (get map :minor-modes)
        :major-modes (get map :major-modes)))
"####;

fn bind_map_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(BIND_MAP_MELPA_PIN, "bind-map.el")
        .expect("prepare revision-pinned Bind Map source below ./tmp")
        .with_melpa_dependency(EVIL_MELPA_PIN)
        .expect("prepare revision-pinned Evil dependency below ./tmp")
        .with_prelude(BIND_MAP_TEST_PRELUDE)
        .with_timeout(BIND_MAP_TEST_TIMEOUT)
}

fn global_project_leader_dispatches_commands_and_exposes_prefix_metadata() -> ParityBatchCase {
    let elisp_form = r####"
(let ((old-binding (lookup-key global-map (kbd "C-c p")))
      neomacs-bind-map-test-history)
  (unwind-protect
      (progn
        (bind-map neomacs-bind-map-project-map
          :keys ("C-c p" nil "")
          :prefix-cmd neomacs-bind-map-project-prefix
          :bindings ("b" 'neomacs-bind-map-build
                     "t" 'neomacs-bind-map-test
                     "d" 'neomacs-bind-map-deploy))
        (bind-map-set-key-defaults neomacs-bind-map-project-map
          "b" 'neomacs-bind-map-conflict
          "n" 'neomacs-bind-map-notes)
        (neomacs-bind-map-test-in-buffer
         "project"
         (lambda ()
           (list
            :map-symbol 'neomacs-bind-map-project-map
            :properties
            (neomacs-bind-map-test-properties
             'neomacs-bind-map-project-map)
            :prefix-binding (key-binding (kbd "C-c p"))
            :prefix-function-is-map
            (eq (symbol-function 'neomacs-bind-map-project-prefix)
                neomacs-bind-map-project-map)
            :map-bindings
            (mapcar (lambda (key)
                      (cons key (lookup-key neomacs-bind-map-project-map
                                            (kbd key))))
                    '("b" "t" "d" "n"))
            :executed
            (mapcar #'neomacs-bind-map-test-execute
                    '("C-c p b" "C-c p t" "C-c p d" "C-c p n"))
            :history (nreverse neomacs-bind-map-test-history)))))
    (define-key global-map (kbd "C-c p") old-binding)))
"####;
    let expected = expect![[
        r#"OK (:map-symbol neomacs-bind-map-project-map :properties (:root-map neomacs-bind-map-project-map-root-map :active-var neomacs-bind-map-project-map-active :prefix-cmd neomacs-bind-map-project-prefix :override-minor-modes nil :override-mode-name neomacs-bind-map-project-map-override-mode :keys ("C-c p" nil "") :evil-keys nil :evil-states (normal motion visual) :minor-modes nil :major-modes nil) :prefix-binding neomacs-bind-map-project-prefix :prefix-function-is-map t :map-bindings (("b" . neomacs-bind-map-build) ("t" . neomacs-bind-map-test) ("d" . neomacs-bind-map-deploy) ("n" . neomacs-bind-map-notes)) :executed (neomacs-bind-map-build neomacs-bind-map-test neomacs-bind-map-deploy neomacs-bind-map-notes) :history ((:action build :buffer "*bind-map-project*" :major-mode fundamental-mode :review-mode nil :conflict-mode nil :evil-state nil) (:action test :buffer "*bind-map-project*" :major-mode fundamental-mode :review-mode nil :conflict-mode nil :evil-state nil) (:action deploy :buffer "*bind-map-project*" :major-mode fundamental-mode :review-mode nil :conflict-mode nil :evil-state nil) (:action notes :buffer "*bind-map-project*" :major-mode fundamental-mode :review-mode nil :conflict-mode nil :evil-state nil)))"#
    ]];
    ParityBatchCase::value(
        "global_project_leader_dispatches_commands_and_exposes_prefix_metadata",
        elisp_form,
        expected,
    )
}

fn major_mode_maps_activate_for_existing_remapped_and_aliased_buffers() -> ParityBatchCase {
    let elisp_form = r####"
(let ((bind-map-major-modes-alist nil)
      (minor-mode-map-alist minor-mode-map-alist)
      (major-mode-remap-alist
       '((neomacs-bind-map-classic-mode . neomacs-bind-map-modern-mode)))
      (modern (generate-new-buffer "*bind-map-modern-existing*"))
      (plain (generate-new-buffer "*bind-map-plain-existing*"))
      neomacs-bind-map-test-history)
  (unwind-protect
      (progn
        (with-current-buffer modern (neomacs-bind-map-modern-mode))
        (with-current-buffer plain (fundamental-mode))
        (bind-map neomacs-bind-map-language-map
          :keys ("C-c m")
          :major-modes (neomacs-bind-map-classic-mode
                        neomacs-bind-map-legacy-alias-mode)
          :bindings ("b" 'neomacs-bind-map-build))
        (let ((modern-state
               (with-current-buffer modern
                 (list :major-mode major-mode
                       :active neomacs-bind-map-language-map-active
                       :binding (key-binding (kbd "C-c m b"))
                       :executed
                       (neomacs-bind-map-test-execute "C-c m b"))))
              (plain-before
               (with-current-buffer plain
                 (list :major-mode major-mode
                       :active neomacs-bind-map-language-map-active
                       :binding (key-binding (kbd "C-c m b"))))))
          (with-current-buffer plain
            (neomacs-bind-map-classic-mode))
          (list
           :registered (cdr (assq 'neomacs-bind-map-language-map-active
                                  bind-map-major-modes-alist))
           :classic-expansion
           (bind-map--lookup-major-modes 'neomacs-bind-map-classic-mode)
           :alias-expansion
           (bind-map--lookup-major-modes
            'neomacs-bind-map-legacy-alias-mode)
           :modern-existing modern-state
           :plain-before plain-before
           :plain-after
           (with-current-buffer plain
             (list :major-mode major-mode
                   :active neomacs-bind-map-language-map-active
                   :binding (key-binding (kbd "C-c m b"))
                   :executed
                   (neomacs-bind-map-test-execute "C-c m b")))
           :history (nreverse neomacs-bind-map-test-history))))
    (when (buffer-live-p modern) (kill-buffer modern))
    (when (buffer-live-p plain) (kill-buffer plain))))
"####;
    let expected = expect![[
        r#"OK (:registered (neomacs-bind-map-classic-mode neomacs-bind-map-legacy-alias-mode) :classic-expansion (neomacs-bind-map-classic-mode neomacs-bind-map-modern-mode) :alias-expansion (neomacs-bind-map-legacy-alias-mode neomacs-bind-map-modern-mode) :modern-existing (:major-mode neomacs-bind-map-modern-mode :active t :binding neomacs-bind-map-build :executed neomacs-bind-map-build) :plain-before (:major-mode fundamental-mode :active nil :binding nil) :plain-after (:major-mode neomacs-bind-map-classic-mode :active t :binding neomacs-bind-map-build :executed neomacs-bind-map-build) :history ((:action build :buffer "*bind-map-modern-existing*" :major-mode neomacs-bind-map-modern-mode :review-mode nil :conflict-mode nil :evil-state nil) (:action build :buffer "*bind-map-plain-existing*" :major-mode neomacs-bind-map-classic-mode :review-mode nil :conflict-mode nil :evil-state nil)))"#
    ]];
    ParityBatchCase::value(
        "major_mode_maps_activate_for_existing_remapped_and_aliased_buffers",
        elisp_form,
        expected,
    )
}

fn minor_mode_map_preserves_user_defaults_and_dispatches_only_when_enabled() -> ParityBatchCase {
    let elisp_form = r####"
(let ((minor-mode-map-alist minor-mode-map-alist)
      neomacs-bind-map-test-history)
  (bind-map neomacs-bind-map-review-map
    :keys ("C-c r")
    :minor-modes (neomacs-bind-map-review-mode)
    :bindings ("b" 'neomacs-bind-map-build
               "d" 'neomacs-bind-map-deploy))
  (bind-map-set-key-defaults neomacs-bind-map-review-map
    "b" 'neomacs-bind-map-conflict
    "n" 'neomacs-bind-map-notes)
  (bind-map-set-keys neomacs-bind-map-review-map
    "d" 'neomacs-bind-map-child)
  (neomacs-bind-map-test-in-buffer
   "review"
   (lambda ()
     (let ((disabled (key-binding (kbd "C-c r b"))))
       (neomacs-bind-map-review-mode 1)
       (let ((enabled
              (mapcar #'neomacs-bind-map-test-execute
                      '("C-c r b" "C-c r d" "C-c r n"))))
         (neomacs-bind-map-review-mode -1)
         (list :map-bindings
               (mapcar (lambda (key)
                         (cons key
                               (lookup-key neomacs-bind-map-review-map
                                           (kbd key))))
                       '("b" "d" "n"))
               :disabled disabled
               :enabled enabled
               :disabled-again (key-binding (kbd "C-c r b"))
               :history (nreverse neomacs-bind-map-test-history)))))))
"####;
    let expected = expect![[
        r#"OK (:map-bindings (("b" . neomacs-bind-map-build) ("d" . neomacs-bind-map-child) ("n" . neomacs-bind-map-notes)) :disabled nil :enabled (neomacs-bind-map-build neomacs-bind-map-child neomacs-bind-map-notes) :disabled-again nil :history ((:action build :buffer "*bind-map-review*" :major-mode fundamental-mode :review-mode t :conflict-mode nil :evil-state nil) (:action child :buffer "*bind-map-review*" :major-mode fundamental-mode :review-mode t :conflict-mode nil :evil-state nil) (:action notes :buffer "*bind-map-review*" :major-mode fundamental-mode :review-mode t :conflict-mode nil :evil-state nil)))"#
    ]];
    ParityBatchCase::value(
        "minor_mode_map_preserves_user_defaults_and_dispatches_only_when_enabled",
        elisp_form,
        expected,
    )
}

fn inherited_and_convenience_maps_route_mode_specific_leaders() -> ParityBatchCase {
    let elisp_form = r####"
(let ((old-parent (lookup-key global-map (kbd "C-c l")))
      (old-text (lookup-key global-map (kbd "C-c t")))
      (bind-map-major-modes-alist nil)
      (minor-mode-map-alist minor-mode-map-alist)
      (bind-map-default-map-suffix "-workspace-map")
      neomacs-bind-map-test-history)
  (unwind-protect
      (progn
        (bind-map neomacs-bind-map-parent-map
          :keys ("C-c l")
          :bindings ("g" 'neomacs-bind-map-global))
        (bind-map-for-mode-inherit neomacs-bind-map-elisp-map
            neomacs-bind-map-parent-map
          :major-modes (emacs-lisp-mode)
          :bindings ("x" 'neomacs-bind-map-child))
        (let ((generated
               (bind-map-for-major-mode text-mode
                 :keys ("C-c t")
                 :bindings ("n" 'neomacs-bind-map-notes))))
          (list
           :generated generated
           :child-properties
           (neomacs-bind-map-test-properties 'neomacs-bind-map-elisp-map)
           :generated-properties
           (neomacs-bind-map-test-properties generated)
           :fundamental
           (neomacs-bind-map-test-in-buffer
            "inherit-fundamental"
            (lambda ()
              (fundamental-mode)
              (list :global (neomacs-bind-map-test-execute "C-c l g")
                    :child (key-binding (kbd "C-c l x")))))
           :elisp
           (neomacs-bind-map-test-in-buffer
            "inherit-elisp"
            (lambda ()
              (emacs-lisp-mode)
              (list :child (neomacs-bind-map-test-execute "C-c l x")
                    :parent (key-binding (kbd "C-c l g")))))
           :text
           (neomacs-bind-map-test-in-buffer
            "inherit-text"
            (lambda ()
              (text-mode)
              (list :generated
                    (neomacs-bind-map-test-execute "C-c t n")
                    :active text-mode-workspace-map-active)))
           :history (nreverse neomacs-bind-map-test-history))))
    (define-key global-map (kbd "C-c l") old-parent)
    (define-key global-map (kbd "C-c t") old-text)))
"####;
    let expected = expect![[
        r#"OK (:generated text-mode-workspace-map :child-properties (:root-map neomacs-bind-map-elisp-map-root-map :active-var neomacs-bind-map-elisp-map-active :prefix-cmd neomacs-bind-map-elisp-map-prefix :override-minor-modes nil :override-mode-name neomacs-bind-map-elisp-map-override-mode :keys ("C-c l") :evil-keys nil :evil-states #1=(normal motion visual) :minor-modes nil :major-modes (emacs-lisp-mode)) :generated-properties (:root-map text-mode-workspace-map-root-map :active-var text-mode-workspace-map-active :prefix-cmd text-mode-workspace-map-prefix :override-minor-modes nil :override-mode-name text-mode-workspace-map-override-mode :keys ("C-c t") :evil-keys nil :evil-states #1# :minor-modes nil :major-modes (text-mode)) :fundamental (:global neomacs-bind-map-global :child nil) :elisp (:child neomacs-bind-map-child :parent neomacs-bind-map-global) :text (:generated neomacs-bind-map-notes :active t) :history ((:action global :buffer "*bind-map-inherit-fundamental*" :major-mode fundamental-mode :review-mode nil :conflict-mode nil :evil-state nil) (:action child :buffer "*bind-map-inherit-elisp*" :major-mode emacs-lisp-mode :review-mode nil :conflict-mode nil :evil-state nil) (:action notes :buffer "*bind-map-inherit-text*" :major-mode text-mode :review-mode nil :conflict-mode nil :evil-state nil)))"#
    ]];
    ParityBatchCase::value(
        "inherited_and_convenience_maps_route_mode_specific_leaders",
        elisp_form,
        expected,
    )
}

fn override_map_beats_an_ordinary_minor_mode_and_can_be_toggled() -> ParityBatchCase {
    let elisp_form = r####"
(let ((old-binding (lookup-key global-map (kbd "C-c o")))
      neomacs-bind-map-test-history)
  (unwind-protect
      (progn
        (bind-map neomacs-bind-map-override-map
          :keys ("C-c o")
          :override-minor-modes t
          :override-mode-name neomacs-bind-map-override-mode
          :bindings ("x" 'neomacs-bind-map-override))
        (neomacs-bind-map-test-in-buffer
         "override"
         (lambda ()
           (fundamental-mode)
           (neomacs-bind-map-conflict-mode 1)
           (bind-map--ensure-neomacs-bind-map-override-mode)
           (let ((overriding
                  (neomacs-bind-map-test-execute "C-c o x")))
             (neomacs-bind-map-override-mode -1)
             (let ((ordinary
                    (neomacs-bind-map-test-execute "C-c o x")))
               (neomacs-bind-map-override-mode 1)
               (list
                :global-mode global-neomacs-bind-map-override-mode
                :buffer-mode neomacs-bind-map-override-mode
                :overriding overriding
                :ordinary ordinary
                :overriding-again
                (neomacs-bind-map-test-execute "C-c o x")
                :emulation-entry
                (cl-some
                 (lambda (entry)
                   (and (listp entry)
                        (assq 'neomacs-bind-map-override-mode entry)
                        t))
                 emulation-mode-map-alists)
                :minor-entry
                (and (assq 'neomacs-bind-map-override-mode
                           minor-mode-map-alist)
                     t)
                :history (nreverse neomacs-bind-map-test-history)))))))
    (when (fboundp 'global-neomacs-bind-map-override-mode)
      (global-neomacs-bind-map-override-mode -1))
    (define-key global-map (kbd "C-c o") old-binding)))
"####;
    let expected = expect![[
        r#"OK (:global-mode t :buffer-mode t :overriding neomacs-bind-map-override :ordinary neomacs-bind-map-conflict :overriding-again neomacs-bind-map-override :emulation-entry t :minor-entry t :history ((:action override :buffer "*bind-map-override*" :major-mode fundamental-mode :review-mode nil :conflict-mode t :evil-state nil) (:action conflict :buffer "*bind-map-override*" :major-mode fundamental-mode :review-mode nil :conflict-mode t :evil-state nil) (:action override :buffer "*bind-map-override*" :major-mode fundamental-mode :review-mode nil :conflict-mode t :evil-state nil)))"#
    ]];
    ParityBatchCase::value(
        "override_map_beats_an_ordinary_minor_mode_and_can_be_toggled",
        elisp_form,
        expected,
    )
}

fn evil_state_leaders_follow_declared_inherited_and_mode_local_maps() -> ParityBatchCase {
    let elisp_form = r####"
(let ((old-binding (lookup-key global-map (kbd "C-c e")))
      neomacs-bind-map-test-history)
  (unwind-protect
      (progn
        (bind-map neomacs-bind-map-evil-map
          :keys ("C-c e")
          :evil-keys ("SPC")
          :evil-states (normal motion)
          :bindings ("x" 'neomacs-bind-map-evil))
        (bind-map neomacs-bind-map-evil-local-map
          :evil-keys (",")
          :evil-states (motion)
          :major-modes (emacs-lisp-mode)
          :bindings ("r" 'neomacs-bind-map-notes))
        (neomacs-bind-map-test-in-buffer
         "evil"
         (lambda ()
           (fundamental-mode)
           (evil-local-mode 1)
           (evil-motion-state)
           (evil-normalize-keymaps)
           (let ((motion (neomacs-bind-map-test-execute "SPC x"))
                 (fundamental-local (key-binding (kbd ", r"))))
             (evil-normal-state)
             (let ((normal (neomacs-bind-map-test-execute "SPC x")))
               (evil-visual-state)
               (let ((visual (key-binding (kbd "SPC x"))))
                 (emacs-lisp-mode)
                 (evil-motion-state)
                 (evil-normalize-keymaps)
                 (let ((mode-local
                        (neomacs-bind-map-test-execute ", r")))
                 (evil-emacs-state)
                 (list
                  :motion motion
                  :normal normal
                  :visual visual
                  :fundamental-local fundamental-local
                  :mode-local mode-local
                  :mode-local-active neomacs-bind-map-evil-local-map-active
                  :emacs-leader
                  (neomacs-bind-map-test-execute "C-c e x")
                  :motion-auxiliary
                  (lookup-key
                   (evil-get-auxiliary-keymap
                    neomacs-bind-map-evil-map-root-map 'motion)
                   (kbd "SPC x"))
                  :visual-auxiliary
                  (lookup-key
                   (evil-get-auxiliary-keymap
                    neomacs-bind-map-evil-map-root-map 'visual)
                   (kbd "SPC x"))
                  :local-motion-auxiliary
                  (lookup-key
                   (evil-get-auxiliary-keymap
                    neomacs-bind-map-evil-local-map-root-map 'motion)
                   (kbd ", r"))
                  :history (nreverse neomacs-bind-map-test-history)))))))))
    (define-key global-map (kbd "C-c e") old-binding)))
"####;
    let expected = expect![[
        r#"OK (:motion neomacs-bind-map-evil :normal neomacs-bind-map-evil :visual neomacs-bind-map-evil :fundamental-local nil :mode-local neomacs-bind-map-notes :mode-local-active t :emacs-leader neomacs-bind-map-evil :motion-auxiliary neomacs-bind-map-evil :visual-auxiliary 1 :local-motion-auxiliary neomacs-bind-map-notes :history ((:action evil :buffer "*bind-map-evil*" :major-mode fundamental-mode :review-mode nil :conflict-mode nil :evil-state motion) (:action evil :buffer "*bind-map-evil*" :major-mode fundamental-mode :review-mode nil :conflict-mode nil :evil-state normal) (:action notes :buffer "*bind-map-evil*" :major-mode emacs-lisp-mode :review-mode nil :conflict-mode nil :evil-state motion) (:action evil :buffer "*bind-map-evil*" :major-mode emacs-lisp-mode :review-mode nil :conflict-mode nil :evil-state emacs)))"#
    ]];
    ParityBatchCase::value(
        "evil_state_leaders_follow_declared_inherited_and_mode_local_maps",
        elisp_form,
        expected,
    )
}

#[test]
fn bind_map_package_batch() {
    assert_oracle_batch_cases(
        bind_map_oracle(),
        "bind-map-package-batch",
        "Bind Map",
        &[
            global_project_leader_dispatches_commands_and_exposes_prefix_metadata(),
            major_mode_maps_activate_for_existing_remapped_and_aliased_buffers(),
            minor_mode_map_preserves_user_defaults_and_dispatches_only_when_enabled(),
            inherited_and_convenience_maps_route_mode_specific_leaders(),
            override_map_beats_an_ordinary_minor_mode_and_can_be_toggled(),
            evil_state_leaders_follow_declared_inherited_and_mode_local_maps(),
        ],
    );
}
