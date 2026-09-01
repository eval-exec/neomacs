use expect_test::expect;

use super::ParityBatchCase;

fn auto_dim_other_buffers_exact_descriptor_and_archive_payload_bytes_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_exact_descriptor_and_archive_payload_bytes_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'auto-dim-other-buffers
                                   package-alist)))
                                (directory
                                 (package-desc-dir descriptor)))
          (list
           (package-desc-name descriptor)
           (package-version-join
            (package-desc-version descriptor))
           (package-desc-summary descriptor)
           (package-desc-reqs descriptor)
           (package-desc-kind descriptor)
           (package-desc-extras descriptor)
           (mapcar
            (lambda (file)
              (let ((file
                     (expand-file-name
                      file
                      directory)))
                (list
                 (file-name-nondirectory file)
                 (file-attribute-size
                  (file-attributes file))
                 (with-temp-buffer
                   (insert-file-contents-literally file)
                   (secure-hash
                    'sha256
                    (current-buffer))))))
            '("auto-dim-other-buffers-pkg.el"
              "auto-dim-other-buffers.el"))))"##,
        expect![[
            r#"OK (auto-dim-other-buffers "20260624.950" "Makes windows without focus less prominent." ((emacs (27 1))) nil ((:maintainers ("Michal Nazarewicz" . "mina86@mina86.com")) (:authors ("Michal Nazarewicz" . "mina86@mina86.com")) (:keywords "faces") (:revdesc . "cf0263073470") (:commit . "cf0263073470190b85f6013066856126aac67d19") (:url . "https://github.com/mina86/auto-dim-other-buffers.el")) (("auto-dim-other-buffers-pkg.el" 460 "7f84c9400ad929c36ccee5d4891c8322de4e6b5a5c85cb9103969887654944d4") ("auto-dim-other-buffers.el" 25548 "e39dbe2ea0dbccb2d2d9790fa2dc2271fa0eba3bf6dd935197703ac2a527906c")))"#
        ]],
    )
}

fn auto_dim_other_buffers_complete_prefixed_symbol_inventory_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_complete_prefixed_symbol_inventory_matches",
        r##"(let (symbols)
          (mapatoms
           (lambda (symbol)
             (let ((name
                    (symbol-name symbol)))
               (when
                   (and
                    (or
                     (string-prefix-p "adob-" name)
                     (string-prefix-p
                      "auto-dim-other-buffers"
                      name))
                    (not
                     (string-prefix-p
                      "adob-test-"
                      name)))
                 (push
                  (list
                   symbol
                   (fboundp symbol)
                   (boundp symbol)
                   (and
                    (commandp symbol)
                    t)
                   (and
                    (facep symbol)
                    t)
                   (local-variable-if-set-p
                    symbol)
                   (file-name-nondirectory
                    (or
                     (symbol-file symbol)
                     "")))
                  symbols)))))
          (sort
           symbols
           (lambda (left right)
             (string<
              (symbol-name
               (car left))
              (symbol-name
               (car right))))))"##,
        expect![[
            r#"OK ((adob--buffer-list-update-hook t nil nil nil nil "auto-dim-other-buffers.el") (adob--dim nil nil nil nil nil "") (adob--dim-buffer t nil nil nil nil "auto-dim-other-buffers.el") (adob--face-mode-remapping nil t nil nil t "auto-dim-other-buffers.el") (adob--focus-change t nil nil nil nil "auto-dim-other-buffers.el") (adob--focus-change-debounce-delay nil t nil nil nil "auto-dim-other-buffers.el") (adob--focus-change-hook t nil nil nil nil "auto-dim-other-buffers.el") (adob--focus-change-last-state nil t nil nil nil "auto-dim-other-buffers.el") (adob--focus-change-timer nil t nil nil nil "auto-dim-other-buffers.el") (adob--force-fringes-refresh t nil nil nil nil "auto-dim-other-buffers.el") (adob--force-window-update t nil nil nil nil "auto-dim-other-buffers.el") (adob--hack nil nil nil t nil "auto-dim-other-buffers.el") (adob--has-fringes nil t nil nil nil "auto-dim-other-buffers.el") (adob--has-fringes--refresh t nil nil nil nil "auto-dim-other-buffers.el") (adob--initialize t nil nil nil nil "auto-dim-other-buffers.el") (adob--kill-all-local-variables-advice t nil nil nil nil "auto-dim-other-buffers.el") (adob--last-buffer nil t nil nil nil "auto-dim-other-buffers.el") (adob--last-window nil t nil nil nil "auto-dim-other-buffers.el") (adob--never-dim-p t nil nil nil nil "auto-dim-other-buffers.el") (adob--positive-assqp t nil nil nil nil "auto-dim-other-buffers.el") (adob--remap-add-relative t nil nil nil nil "auto-dim-other-buffers.el") (adob--remap-add-relative-process-entry t nil nil nil nil "auto-dim-other-buffers.el") (adob--remap-cycle-all t nil nil nil nil "auto-dim-other-buffers.el") (adob--remap-faces t nil nil nil nil "auto-dim-other-buffers.el") (adob--remap-remove-relative t nil nil nil nil "auto-dim-other-buffers.el") (adob--rescan-windows t nil nil nil nil "auto-dim-other-buffers.el") (adob--update t nil nil nil nil "auto-dim-other-buffers.el") (auto-dim-other-buffers nil nil nil t nil "auto-dim-other-buffers.el") (auto-dim-other-buffers-affected-faces nil t nil nil nil "auto-dim-other-buffers.el") (auto-dim-other-buffers-autoloads nil nil nil nil nil "auto-dim-other-buffers-autoloads.el") (auto-dim-other-buffers-dim-on-focus-out nil t nil nil nil "auto-dim-other-buffers.el") (auto-dim-other-buffers-dim-on-switch-to-minibuffer nil t nil nil nil "auto-dim-other-buffers.el") (auto-dim-other-buffers-face nil nil nil t nil "") (auto-dim-other-buffers-hide nil nil nil t nil "auto-dim-other-buffers.el") (auto-dim-other-buffers-hide-face nil nil nil t nil "") (auto-dim-other-buffers-mode t t t nil nil "auto-dim-other-buffers.el") (auto-dim-other-buffers-mode-hook nil t nil nil nil "auto-dim-other-buffers.el") (auto-dim-other-buffers-mode-map nil nil nil nil nil "") (auto-dim-other-buffers-mode-off-hook nil nil nil nil nil "") (auto-dim-other-buffers-mode-on-hook nil nil nil nil nil "") (auto-dim-other-buffers-never-dim-buffer-functions nil t nil nil nil "auto-dim-other-buffers.el"))"#
        ]],
    )
}

fn auto_dim_other_buffers_every_callable_arglist_interactivity_doc_and_source_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_every_callable_arglist_interactivity_doc_and_source_match",
        r##"(mapcar
          (lambda (symbol)
            (list
             symbol
             (help-function-arglist symbol t)
             (and
              (interactive-form symbol)
              t)
             (and
              (commandp symbol)
              t)
             (documentation symbol t)
             (file-name-nondirectory
              (or
               (symbol-file symbol 'defun)
               ""))))
          '(adob--never-dim-p
            adob--has-fringes--refresh
            adob--force-window-update
            adob--positive-assqp
            adob--force-fringes-refresh
            adob--remap-add-relative
            adob--remap-add-relative-process-entry
            adob--remap-remove-relative
            adob--remap-cycle-all
            adob--remap-faces
            adob--kill-all-local-variables-advice
            adob--dim-buffer
            adob--update
            adob--rescan-windows
            adob--buffer-list-update-hook
            adob--focus-change
            adob--focus-change-hook
            adob--initialize
            auto-dim-other-buffers-mode))"##,
        expect![[
            r#"OK ((adob--never-dim-p (buffer) nil nil "Return whether to never dim BUFFER.\nCall ‘auto-dim-other-buffers-never-dim-buffer-functions’ to see\nif any of them return non-nil in which case the BUFFER won’t be\ndimmed." "auto-dim-other-buffers.el") (adob--has-fringes--refresh nil nil nil "Refresh value of `adob--has-fringes'\nbased on ‘auto-dim-other-buffers-affected-faces’ variable." "auto-dim-other-buffers.el") (adob--force-window-update (object) nil nil "Force window to be updated on next redisplay.\nThis does more than `force-window-update' by also forcing redisplay of\nfringes if necessary (see `adob--has-fringes').  This is done by forcing\nredisplay of frames containing affected windows." "auto-dim-other-buffers.el") (adob--positive-assqp (symbol params) nil nil "Check that SYMBOL entry in PARAMS alist is a positive number." "auto-dim-other-buffers.el") (adob--force-fringes-refresh (windows) nil nil "Force refresh of fringes in WINDOWS.\nThis is done by forcing full frame redraws." "auto-dim-other-buffers.el") (adob--remap-add-relative nil nil nil "Map all necessary relative face in current buffer.\nUpdates ‘adob--face-mode-remapping’ variable accordingly and returns its\nnew value." "auto-dim-other-buffers.el") (adob--remap-add-relative-process-entry (entry) nil nil "Add a single face mapping specified in ENTRY.\nENTRY is either '(DIM-FACE . HIGHLIGHT-FACE) cons or (for backwards\ncompatibility) 'DIM-FACE." "auto-dim-other-buffers.el") (adob--remap-remove-relative nil nil nil "Remove all relative mappings that we’ve added.\nList of existing mappings is taken from ‘adob--face-mode-remapping’\nvariable whose local value is killed afterwards." "auto-dim-other-buffers.el") (adob--remap-cycle-all (add) nil nil "Remove and re-add face remappings in all buffers where they exist.\nIf ADD is nil, do not re-add the mappings.\n\nThis needs to be called after ‘auto-dim-other-buffers-affected-faces’ is\nchanged to update state of all affected buffers (which is done when the\nvariable is changed via Customize).  It is also used when disabling the\nadob mode." "auto-dim-other-buffers.el") (adob--remap-faces (buffer object) nil nil "Make sure face remappings are active in BUFFER unless its never-dim.\n\nDoes not preserve current buffer.\n\nIf BUFFER is never-dim (as determined by ‘adob--never-dim-p’),\nremove adob face remappings from it.  Otherwise, make sure the\nremappings are active by adding them if it’s missing.\n\nIf face remapping had to be changed, force update of OBJECT,\nwhich can be a window or a buffer.\n\nReturn non-nil if remappings have been added to BUFFER." "auto-dim-other-buffers.el") (adob--kill-all-local-variables-advice (kill &rest args) nil nil "Call KILL with ARGS and restore face remapping.\nIntended as an advice around ‘kill-all-local-variables’ function which\nkills all local variables and removes all face remapping." "auto-dim-other-buffers.el") (adob--dim-buffer (buffer &optional except-in) nil nil "Dim BUFFER if not already dimmed except in EXCEPT-IN window.\n\nDoes not preserve current buffer.\n\nEXCEPT-IN works by deactivating the dimmed face in specified window." "auto-dim-other-buffers.el") (adob--update nil nil nil "Make sure that selected window is not dimmed.\nDim previously selected window if selection has changed." "auto-dim-other-buffers.el") (adob--rescan-windows nil nil nil "Rescan all windows in selected frame and dim all non-selected windows." "auto-dim-other-buffers.el") (adob--buffer-list-update-hook nil nil nil "React to buffer list changes.\nIf selected buffer has changed, change which buffer is dimmed.\nOtherwise, if a new buffer is displayed somewhere, dim it." "auto-dim-other-buffers.el") (adob--focus-change nil nil nil "Based on focus status of selected frame dim or undim selected buffer.\nDo nothing if `auto-dim-other-buffers-dim-on-focus-out' is nil\nand frame’s doesn’t have focus." "auto-dim-other-buffers.el") (adob--focus-change-hook nil nil nil "Debounce focus-change event and call `adob--focus-change'." "auto-dim-other-buffers.el") (adob--initialize nil nil nil "Dim all except for the selected buffer." "auto-dim-other-buffers.el") (auto-dim-other-buffers-mode (&optional arg) t t "Visually makes windows without focus less prominent.\n\nWindows without input focus are made to look less prominent by applying\n‘auto-dim-other-buffers’ to them.  With many windows in a frame,\nthe idea is that this mode helps recognise which is the selected window\nby providing a non-intrusive but still noticeable visual indicator.\n\nBeware: This mode may cause flickering, especially if fringe changing is\nenabled (which is the default).  To mitigate the flickering, try\nremoving fringe changing (see `auto-dim-other-buffers-affected-faces').\n\nNote: Despite it’s name, this mode operates on *windows* rather than\nbuffers, i.e. even if a buffer is shown in multiple windows, only one of\nthem is considered selected and all other will be dimmed.  Historically,\nprior to Emacs 27, all or none windows displaying a buffer would be\ndimmed; this historical behaviour is where the mode gets its name from.\n\nThis is a global minor mode.  If called interactively, toggle the\n`Auto-Dim-Other-Buffers mode' mode.  If the prefix argument is positive,\nenable the mode, and if it is zero or negative, disable the mode.\n\nIf called from Lisp, toggle the mode if ARG is `toggle'.  Enable the\nmode if ARG is nil, omitted, or is a positive number.  Disable the mode\nif ARG is a negative number.\n\nTo check whether the minor mode is enabled in the current buffer,\nevaluate `(default-value \\='auto-dim-other-buffers-mode)'.\n\nThe mode's hook is called both when the mode is enabled and when it is\ndisabled." "auto-dim-other-buffers.el"))"#
        ]],
    )
}

fn auto_dim_other_buffers_customize_group_faces_and_obsolete_aliases_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_customize_group_faces_and_obsolete_aliases_match",
        r##"(list
          (get
           'auto-dim-other-buffers
           'custom-group)
          (get
           'auto-dim-other-buffers
           'group-documentation)
          (get
           'auto-dim-other-buffers
           'custom-prefix)
          (mapcar
           (lambda (face)
             (list
              face
              (get face 'face-defface-spec)
              (face-documentation face)
              (get face 'custom-group)
              (file-name-nondirectory
               (or
                (symbol-file face 'defface)
                ""))))
           '(auto-dim-other-buffers
             auto-dim-other-buffers-hide
             adob--hack))
          (mapcar
           (lambda (alias)
             (list
              alias
              (get alias 'face-alias)
              (get alias 'obsolete-face)))
           '(auto-dim-other-buffers-face
             auto-dim-other-buffers-hide-face)))"##,
        expect![[
            r##"OK (#1=((auto-dim-other-buffers custom-face) (auto-dim-other-buffers-hide custom-face) (auto-dim-other-buffers-dim-on-focus-out custom-variable) (auto-dim-other-buffers-dim-on-switch-to-minibuffer custom-variable) (adob--hack custom-face) (auto-dim-other-buffers-mode custom-variable) (auto-dim-other-buffers-never-dim-buffer-functions custom-variable) (auto-dim-other-buffers-affected-faces custom-variable)) "Visually makes windows without focus less prominent." "auto-dim-other-buffers-" ((auto-dim-other-buffers ((((background light)) :background "#eff") (t :background "#122")) "Face with a (presumably) dimmed background for non-selected window.\n\nBy default it is applied to, among others, the ‘default’ face and is\nintended to affect background of non-selected windows.  A related\n‘auto-dim-other-buffers-hide’ face is intended for faces which need\ntheir foreground to be changed in sync.  Which faces are modified is\nconfigured by the ‘auto-dim-other-buffers-affecteds’ variable." #1# "auto-dim-other-buffers.el") (auto-dim-other-buffers-hide ((((background light)) :foreground "#eff" :background "#eff") (t :foreground "#122" :background "#122")) "Face with a (presumably) dimmed background and matching foreground.\n\nThe intention is that the face has the same foreground and\nbackground as the background of ‘auto-dim-other-buffers’ and\nthat it’s used as remapping for faces which hide the text by\nrendering it in the same colour as background.\n\nBy default it is applied to the ‘org-hide’ face and is intended\nto modify foreground of faces which hide the text by rendering it\nin the same colour as the background.  Since the mode alters the\nbackground in a window such faces need to be updated as well.\n\nWhich faces are modified is configured by the\n‘auto-dim-other-buffers-affecteds’ variable." nil "auto-dim-other-buffers.el") (adob--hack nil "A hack to make fringe refresh work.  Do not use." nil "auto-dim-other-buffers.el")) ((auto-dim-other-buffers-face auto-dim-other-buffers "2.2.1") (auto-dim-other-buffers-hide-face auto-dim-other-buffers-hide "2.2.1")))"##
        ]],
    )
}

fn auto_dim_other_buffers_every_custom_option_default_type_setter_and_source_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_every_custom_option_default_type_setter_and_source_match",
        r##"(mapcar
          (lambda (symbol)
            (let ((setter
                   (get symbol 'custom-set)))
              (list
               symbol
               (symbol-value symbol)
               (default-value symbol)
               (get symbol 'standard-value)
               (get symbol 'custom-type)
               (and setter t)
               (and
                setter
                (help-function-arglist
                 setter
                 t))
               (get symbol 'custom-group)
               (documentation-property
                symbol
                'variable-documentation
                t)
               (local-variable-if-set-p symbol)
               (file-name-nondirectory
                (or
                 (symbol-file symbol 'defvar)
                 "")))))
          '(auto-dim-other-buffers-dim-on-focus-out
            auto-dim-other-buffers-dim-on-switch-to-minibuffer
            auto-dim-other-buffers-never-dim-buffer-functions
            auto-dim-other-buffers-affected-faces
            auto-dim-other-buffers-mode))"##,
        expect![[
            r#"OK ((auto-dim-other-buffers-dim-on-focus-out t t ((funcall #'#[nil (t) #1=(auto-dim-other-buffers-affected-faces t)])) boolean nil nil nil "Whether to dim all windows when frame looses focus." nil "auto-dim-other-buffers.el") (auto-dim-other-buffers-dim-on-switch-to-minibuffer t t ((funcall #'#[nil (t) #1#])) boolean nil nil nil "Whether to dim last buffer when switching to minibuffer or echo area." nil "auto-dim-other-buffers.el") (auto-dim-other-buffers-never-dim-buffer-functions nil nil ((funcall #'#[nil (nil) #1#])) hook t (symbol value) nil "A list of functions run to determine if a buffer should stay lit.\nEach function is called with buffer as its sole argument.  If any\nof them returns non-nil, the buffer will not be dimmed even if\nit’s not selected one.\n\nEach hook function should return the same value for the lifespan\nof a buffer.  Otherwise, display state of a buffers may be\ninconsistent with the determination of a hook function and remain\nstale until the buffer is selected.  Tests based on buffer name\nwill work well, but tests based on major mode, buffer file name\nor other properties which may change during lifespan of a buffer\nmay be problematic.\n\nChanging this variable outside of customize does not immediately\nupdate display state of all affected buffers." nil "auto-dim-other-buffers.el") (auto-dim-other-buffers-affected-faces #2=((default auto-dim-other-buffers) (fringe auto-dim-other-buffers) (org-block auto-dim-other-buffers) (org-hide auto-dim-other-buffers-hide)) #2# ((funcall #'#[nil ('((default auto-dim-other-buffers) (fringe auto-dim-other-buffers) (org-block auto-dim-other-buffers) (org-hide auto-dim-other-buffers-hide))) #1#])) (repeat (cons :tag "Remapping specification" (symbol :tag "Target face") (cons :tag "Remapping faces" (symbol :tag "Dimmed     ") (symbol :tag "Highlighted")))) t (symbol value) nil "A list of faces affected when dimming/highlighting a window.\n\nThe list comprising of (FACE . (DIM-FACE . HIGH-FACE)) cons pairs.\nFACE is an existing face for which a remapping will be added (see\n`face-remap-add-relative').  DIM-FACE and HIGH-FACE are remapping faces\nwhich are active in dimmed and highlighted windows respectively.  Either\nface can be nil; if they are both nil, the entry has no effect.\n\nTypically, DIM-FACE is either ‘auto-dim-other-buffers’ or\n‘auto-dim-other-buffers-hide’.  The former is used when the\nbackground of the face needs to be dimmed while the latter when in\naddition the foreground needs to be set to match the background.\n\nHIGH-FACE allows highlighting the selected window, for example as shown\nin example below.  Alas, it’s then up to the user to properly set up\nfaces such that all of the highlighting works.\n\n    (setq auto-dim-other-buffers-affected-faces\n          '((default   . (nil . auto-dim-other-buffers))\n            (fringe    . (nil . mode-line-active))\n            (org-block . (nil . auto-dim-other-buffers))\n            (org-hide  . (nil . auto-dim-other-buffers-hide))))\n\nBeware: inclusion of `fringe' face in the list forces a more expensive\nredraw procedure to be used.  This may cause additional flickering on\nsome systems.  If you’re observing flickering, try removing the `fringe'\nentry, e.g. by using code such as:\n\n    (setq auto-dim-other-buffers-affected-faces\n          (assq-delete-all 'fringe auto-dim-other-buffers-affected-faces))\n\nFor backwards compatibility, a (FACE . DIM-FACE) format for the entries\nis also accepted.  (Although, setting that is not supported through\nCustomize).\n\nChanging this variable outside of Customize does not update display\nstate of affected buffers and requires toggling the mode off and on." nil "auto-dim-other-buffers.el") (auto-dim-other-buffers-mode nil nil ((funcall #'#[nil (nil) #1#])) boolean t (variable value) nil "Non-nil if Auto-Dim-Other-Buffers mode is enabled.\nSee the `auto-dim-other-buffers-mode' command\nfor a description of this minor mode.\nSetting this variable directly does not take effect;\neither customize it (see the info node `Easy Customization')\nor call the function `auto-dim-other-buffers-mode'." nil "auto-dim-other-buffers.el"))"#
        ]],
    )
    .fresh_process()
}

fn auto_dim_other_buffers_internal_state_hook_advice_and_locality_metadata_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dim_other_buffers_internal_state_hook_advice_and_locality_metadata_match",
        r##"(mapcar
          (lambda (symbol)
            (list
             symbol
             (boundp symbol)
             (and
              (boundp symbol)
              (symbol-value symbol))
             (local-variable-if-set-p symbol)
             (documentation-property
              symbol
              'variable-documentation
              t)
             (file-name-nondirectory
              (or
               (symbol-file symbol 'defvar)
               ""))))
          '(adob--last-buffer
            adob--last-window
            adob--has-fringes
            adob--face-mode-remapping
            adob--focus-change-debounce-delay
            adob--focus-change-timer
            adob--focus-change-last-state
            auto-dim-other-buffers-mode
            auto-dim-other-buffers-mode-hook))"##,
        expect![[
            r#"OK ((adob--last-buffer t nil nil "Last selected buffer, i.e. buffer which is currently not dimmed." "auto-dim-other-buffers.el") (adob--last-window t nil nil "Last selected window, i.e. window which is currently not dimmed." "auto-dim-other-buffers.el") (adob--has-fringes t nil nil "Whether we are remapping `fringe' face; see `adob--has-fringes--refresh'." "auto-dim-other-buffers.el") (adob--face-mode-remapping t nil t "Current face remapping cookie for `auto-dim-other-buffers-mode'." "auto-dim-other-buffers.el") (adob--focus-change-debounce-delay t 0.015 nil "Delay in seconds to use when debouncing focus change events.\nWindow manager may send spurious focus change events.  To filter\nthem, the code delays handling of focus-change events by this\nnumber of seconds.  Based on rudimentary testing, 0.015 (i.e. 15\nmilliseconds) is a good compromise between performing the\nfiltering and introducing a visible delay.\n\nSetting this variable to zero will disable the debouncing." "auto-dim-other-buffers.el") (adob--focus-change-timer t nil nil "Timer used to debounce focus change events.\nTimer used by ‘adob--focus-change-hook’ when debouncing focus\nchange events.  The actual delay is specified by the\n`adob--focus-change-debounce-delay` variable." "auto-dim-other-buffers.el") (adob--focus-change-last-state t force-update nil "Last ‘frame-focus-state’ when handling focus change event.\nWindow manager may send spurious focus change events.  The code\nattempts to debounce them but this may result in getting a change\nevent even if the focus state hasn’t changed.  This variable\nstores the last state we’ve seen so that we can skip doing any\nwork if it hasn’t changed." "auto-dim-other-buffers.el") (auto-dim-other-buffers-mode t nil nil "Non-nil if Auto-Dim-Other-Buffers mode is enabled.\nSee the `auto-dim-other-buffers-mode' command\nfor a description of this minor mode.\nSetting this variable directly does not take effect;\neither customize it (see the info node `Easy Customization')\nor call the function `auto-dim-other-buffers-mode'." "auto-dim-other-buffers.el") (auto-dim-other-buffers-mode-hook t nil nil "Hook run after entering or leaving `auto-dim-other-buffers-mode'.\nNo problems result if this variable is not bound.\n`add-hook' automatically binds it.  (This is true for all hook variables.)" "auto-dim-other-buffers.el"))"#
        ]],
    )
}

fn auto_dim_other_buffers_source_load_history_records_complete_definition_order() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dim_other_buffers_source_load_history_records_complete_definition_order",
        r##"(let* ((history
                                 (seq-find
                                  (lambda (entry)
                                    (and
                                     (stringp
                                      (car entry))
                                     (string-suffix-p
                                      "auto-dim-other-buffers.el"
                                      (car entry))))
                                  load-history))
                                (events
                                 (seq-filter
                                  (lambda (event)
                                    (memq
                                     (car-safe event)
                                     '(require
                                       defface
                                       defun
                                       provide)))
                                  (cdr history))))
          (list
           (file-name-nondirectory
            (car history))
           events
           (featurep
            'auto-dim-other-buffers)
           (featurep 'face-remap)))"##,
        expect![[
            r#"OK ("auto-dim-other-buffers.el" ((require . face-remap) (defface . auto-dim-other-buffers) (defface . auto-dim-other-buffers-hide) (defun . adob--never-dim-p) (defface . adob--hack) (defun . adob--has-fringes--refresh) (defun . adob--force-window-update) (defun . adob--positive-assqp) (defun . adob--force-fringes-refresh) (defun . adob--remap-add-relative) (defun . adob--remap-add-relative-process-entry) (defun . adob--remap-remove-relative) (defun . adob--remap-cycle-all) (defun . adob--remap-faces) (defun . adob--kill-all-local-variables-advice) (defun . adob--dim-buffer) (defun . adob--update) (defun . adob--rescan-windows) (defun . adob--buffer-list-update-hook) (defun . adob--focus-change) (defun . adob--focus-change-hook) (defun . adob--initialize) (defun . auto-dim-other-buffers-mode) (provide . auto-dim-other-buffers)) t t)"#
        ]],
    )
}

fn auto_dim_other_buffers_source_reload_preserves_configuration_and_runtime_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_source_reload_preserves_configuration_and_runtime_state",
        r##"(let ((source
                                (getenv
                                 "NEOMACS_PACKAGE_SOURCE")))
          (setq
           auto-dim-other-buffers-dim-on-focus-out
           nil
           auto-dim-other-buffers-dim-on-switch-to-minibuffer
           nil
           auto-dim-other-buffers-never-dim-buffer-functions
           '(ignore)
           auto-dim-other-buffers-affected-faces
           '((default . (nil . bold)))
           adob--last-buffer
           :fixture-buffer
           adob--last-window
           :fixture-window
           adob--focus-change-last-state
           :fixture-focus)
          (load source nil t t)
          (list
           auto-dim-other-buffers-dim-on-focus-out
           auto-dim-other-buffers-dim-on-switch-to-minibuffer
           auto-dim-other-buffers-never-dim-buffer-functions
           auto-dim-other-buffers-affected-faces
           adob--last-buffer
           adob--last-window
           adob--focus-change-last-state
           (featurep
            'auto-dim-other-buffers)))"##,
        expect![
            "OK (nil nil (ignore) ((default nil . bold)) :fixture-buffer :fixture-window :fixture-focus t)"
        ],
    )
}

fn auto_dim_other_buffers_generated_autoload_exposes_mode_and_custom_metadata_before_activation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_generated_autoload_exposes_mode_and_custom_metadata_before_activation",
        r##"(let* ((definition
                                 (symbol-function
                                  'auto-dim-other-buffers-mode))
                                (history
                                 (seq-find
                                  (lambda (entry)
                                    (and
                                     (stringp
                                      (car entry))
                                     (string-suffix-p
                                      "auto-dim-other-buffers-autoloads.el"
                                      (car entry))))
                                  load-history)))
          (list
           (featurep
            'auto-dim-other-buffers-autoloads)
           (featurep
            'auto-dim-other-buffers)
           (autoloadp definition)
           (nth 1 definition)
           (commandp
            'auto-dim-other-buffers-mode)
           (boundp
            'auto-dim-other-buffers-affected-faces)
           (get
            'auto-dim-other-buffers
            'custom-group)
           (seq-filter
            (lambda (event)
              (memq
               (car-safe event)
               '(defun provide)))
            (cdr history))))"##,
        expect![[
            r#"OK (t nil t "auto-dim-other-buffers" t nil nil ((defun . auto-dim-other-buffers-mode) (provide . auto-dim-other-buffers-autoloads)))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn registry_auto_dim_other_buffers_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dim_other_buffers_exact_descriptor_and_archive_payload_bytes_match(),
        auto_dim_other_buffers_complete_prefixed_symbol_inventory_matches(),
        auto_dim_other_buffers_every_callable_arglist_interactivity_doc_and_source_match(),
        auto_dim_other_buffers_customize_group_faces_and_obsolete_aliases_match(),
        auto_dim_other_buffers_every_custom_option_default_type_setter_and_source_match(),
        auto_dim_other_buffers_internal_state_hook_advice_and_locality_metadata_match(),
        auto_dim_other_buffers_source_load_history_records_complete_definition_order(),
        auto_dim_other_buffers_source_reload_preserves_configuration_and_runtime_state(),
    ]
}

pub(super) fn registry_auto_dim_other_buffers_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dim_other_buffers_generated_autoload_exposes_mode_and_custom_metadata_before_activation(),
    ]
}
